//! Per-user, bounded local transport. Never retains raw input or hook text.
use crate::{state::AppCore, view::AppError};
use meshyard_agents::{
    ingest::{self, Envelope},
    state::{StateChange, StateMachine},
};
use meshyard_core::{AgentKind, EventBody};
use meshyard_store::{StoreError, process_lock::private_directory};
use std::{
    collections::BTreeMap,
    os::unix::{
        ffi::OsStrExt,
        fs::{FileTypeExt, MetadataExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    io::AsyncReadExt,
    net::{UnixListener, UnixStream},
    sync::{Mutex, Semaphore, oneshot},
    task::{JoinHandle, JoinSet},
    time,
};

type Core = Mutex<Result<AppCore, AppError>>;
const READ_LIMIT: usize = 1024;
const READ_BUDGET: Duration = Duration::from_millis(200);
const APPEND_INTERVAL: Duration = Duration::from_millis(50);

/// Internal until the frontend/DTO handoff. Counters saturate instead of wrapping.
#[derive(Default)]
pub(crate) struct IngestStatus {
    pub(crate) listening: AtomicBool,
    pub(crate) unavailable: AtomicBool,
    pub(crate) dropped_invalid: AtomicU32,
    pub(crate) dropped_unbound: AtomicU32,
    pub(crate) dropped_excess: AtomicU32,
    pub(crate) storage_errors: AtomicU32,
    pub(crate) active_readers: AtomicU32,
    pub(crate) received: AtomicU32,
    #[cfg(test)]
    trace: TestTrace,
}
/// Bounded scheduling context for test failures only; no payloads or production logging.
#[cfg(test)]
#[derive(Default)]
struct TestTrace {
    ticks: AtomicU32,
    flushes: AtomicU32,
    received_ms: std::sync::atomic::AtomicU64,
    tick_ms: std::sync::atomic::AtomicU64,
    flush_ms: std::sync::atomic::AtomicU64,
}
#[cfg(test)]
fn test_millis(clock: &dyn Clock) -> u64 {
    u64::try_from(clock.now().as_millis()).unwrap_or(u64::MAX)
}
fn count(counter: &AtomicU32) {
    let _ = counter.try_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
        Some(n.saturating_add(1))
    });
}
trait Clock: Send + Sync {
    fn now(&self) -> Duration;
}
struct LocalClock(Instant);
impl Clock for LocalClock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }
}

/// Deletes only the inode created by this listener, including on task cancellation.
struct SocketFile {
    path: PathBuf,
    dev: u64,
    ino: u64,
}
impl Drop for SocketFile {
    fn drop(&mut self) {
        if let Ok(meta) = std::fs::symlink_metadata(&self.path)
            && meta.file_type().is_socket()
            && meta.dev() == self.dev
            && meta.ino() == self.ino
        {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}
fn listener(directory: &Path) -> std::io::Result<(UnixListener, SocketFile)> {
    let directory = private_directory(&directory.join("ingest"))
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::PermissionDenied))?;
    let path = directory.join("ingest.sock");
    if path.as_os_str().as_bytes().len() > 103 {
        return Err(std::io::ErrorKind::InvalidInput.into());
    }
    match std::fs::symlink_metadata(&path) {
        Ok(meta)
            if meta.file_type().is_socket()
                && meta.uid() == rustix::process::geteuid().as_raw() =>
        {
            std::fs::remove_file(&path)?
        }
        Ok(_) => return Err(std::io::ErrorKind::PermissionDenied.into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let listener = UnixListener::bind(&path)?;
    let meta = std::fs::symlink_metadata(&path)?;
    let file = SocketFile {
        path,
        dev: meta.dev(),
        ino: meta.ino(),
    };
    std::fs::set_permissions(&file.path, std::fs::Permissions::from_mode(0o600))?;
    Ok((listener, file))
}

pub(crate) struct Server {
    stop: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
}
impl Server {
    pub(crate) fn start(
        directory: &Path,
        core: Weak<Core>,
        status: Arc<IngestStatus>,
    ) -> Option<Self> {
        Self::start_with(
            directory,
            core,
            status,
            rustix::process::geteuid().as_raw(),
            Arc::new(LocalClock(Instant::now())),
        )
    }
    fn start_with(
        directory: &Path,
        core: Weak<Core>,
        status: Arc<IngestStatus>,
        expected_uid: u32,
        clock: Arc<dyn Clock>,
    ) -> Option<Self> {
        let (listener, file) = match listener(directory) {
            Ok(pair) => pair,
            Err(_) => {
                status.unavailable.store(true, Ordering::Relaxed);
                return None;
            }
        };
        status.unavailable.store(false, Ordering::Relaxed);
        status.listening.store(true, Ordering::Relaxed);
        let (stop, stopped) = oneshot::channel();
        let task = tokio::spawn(serve(
            listener,
            file,
            core,
            status,
            expected_uid,
            clock,
            stopped,
        ));
        Some(Self {
            stop: Some(stop),
            task: Some(task),
        })
    }
    pub(crate) async fn stop(mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        // Fallback for abnormal state teardown; normal exit explicitly joins first.
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}
async fn read_one(mut stream: UnixStream, expected_uid: u32) -> Option<Envelope> {
    if stream.peer_cred().ok()?.uid() != expected_uid {
        return None;
    }
    time::timeout(READ_BUDGET, async {
        // Fixed storage, one byte at a time: cannot allocate/read past the line cap.
        let mut bytes = [0; READ_LIMIT];
        for end in 0..READ_LIMIT {
            if stream.read(&mut bytes[end..end + 1]).await.ok()? != 1 {
                return None;
            }
            if bytes[end] == b'\n' {
                return ingest::parse(&bytes[..=end]).ok();
            }
        }
        None
    })
    .await
    .ok()
    .flatten()
}
async fn serve(
    listener: UnixListener,
    _file: SocketFile,
    core: Weak<Core>,
    status: Arc<IngestStatus>,
    uid: u32,
    clock: Arc<dyn Clock>,
    mut stopped: oneshot::Receiver<()>,
) {
    let mut readers = JoinSet::new();
    let permits = Arc::new(Semaphore::new(8));
    let mut pipeline = Pipeline::new(clock);
    let mut flush = time::interval(APPEND_INTERVAL);
    flush.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
    let mut tick = time::interval(Duration::from_secs(5));
    tick.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            biased;
            _ = &mut stopped => break,
            result = readers.join_next(), if !readers.is_empty() => {
                status.active_readers.store(readers.len() as u32, Ordering::Relaxed);
                match result {
                    Some(Ok(Some(input))) => {
                        let Some(core) = core.upgrade() else { break; };
                        if let Ok(app) = core.lock().await.as_mut() {
                            #[cfg(test)]
                            status.trace.received_ms.store(test_millis(pipeline.clock.as_ref()), Ordering::Relaxed);
                            if pipeline.receive(app, input, &status).await.is_err() { count(&status.storage_errors); }
                            count(&status.received);
                        }
                    }
                    _ => count(&status.dropped_invalid),
                }
            }
            _ = tick.tick() => {
                #[cfg(test)] {
                    count(&status.trace.ticks);
                    status.trace.tick_ms.store(test_millis(pipeline.clock.as_ref()), Ordering::Relaxed);
                }
                let Some(core) = core.upgrade() else { break; };
                if let Ok(app) = core.lock().await.as_mut()
                    && pipeline.tick(app).await.is_err() { count(&status.storage_errors); }
            }
            _ = flush.tick() => {
                #[cfg(test)] {
                    count(&status.trace.flushes);
                    status.trace.flush_ms.store(test_millis(pipeline.clock.as_ref()), Ordering::Relaxed);
                }
                let Some(core) = core.upgrade() else { break; };
                if let Ok(app) = core.lock().await.as_mut()
                    && pipeline.flush(app).await.is_err() { count(&status.storage_errors); }
            }
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => {
                    // JoinSet retains completed readers until consumed: at most eight tasks,
                    // including results waiting for the serialized store writer.
                    if readers.len() == 8 { count(&status.dropped_excess); drop(stream); }
                    else if let Ok(permit) = permits.clone().try_acquire_owned() {
                        readers.spawn(async move { let _permit = permit; read_one(stream, uid).await });
                        status.active_readers.store(readers.len() as u32, Ordering::Relaxed);
                    } else { count(&status.dropped_excess); }
                }
                Err(_) => { status.unavailable.store(true, Ordering::Relaxed); break; }
            }
        }
    }
    readers.abort_all();
    while readers.join_next().await.is_some() {}
    status.active_readers.store(0, Ordering::Relaxed);
    status.listening.store(false, Ordering::Relaxed);
}
struct Slot {
    kind: AgentKind,
    machine: StateMachine,
    binding: Option<[u8; 16]>,
    pending: Option<StateChange>,
}
impl Slot {
    fn new(kind: AgentKind) -> Self {
        Self {
            kind,
            machine: StateMachine::new(kind),
            binding: None,
            pending: None,
        }
    }
    async fn refresh(&mut self, core: &AppCore) -> Result<(), StoreError> {
        let binding = match core.workspace {
            Some(workspace) => {
                core.store
                    .agent_binding(workspace, self.kind, core.signer.clone())
                    .await?
            }
            None => None,
        };
        if self.binding != binding {
            self.binding = binding;
            self.machine = StateMachine::new(self.kind);
            self.pending = None;
        }
        Ok(())
    }
}
struct Pipeline {
    clock: Arc<dyn Clock>,
    slots: [Slot; 2],
    // At most one entry per own registered agent (bounded by the store projection cap).
    last_append: BTreeMap<[u8; 16], Duration>,
}
impl Pipeline {
    fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            clock,
            slots: [
                Slot::new(AgentKind::ClaudeCode),
                Slot::new(AgentKind::Codex),
            ],
            last_append: BTreeMap::new(),
        }
    }
    async fn receive(
        &mut self,
        core: &AppCore,
        input: Envelope,
        status: &IngestStatus,
    ) -> Result<(), StoreError> {
        let now = self.clock.now();
        for slot in &mut self.slots {
            if slot.kind != input.agent() {
                continue;
            }
            slot.refresh(core).await?;
            if slot.binding.is_none() {
                count(&status.dropped_unbound);
                return Ok(());
            }
            if let Some(change) = slot.machine.apply(&input, now) {
                slot.pending = Some(change);
            }
        }
        self.flush(core).await
    }
    async fn tick(&mut self, core: &AppCore) -> Result<(), StoreError> {
        let now = self.clock.now();
        for slot in &mut self.slots {
            slot.refresh(core).await?;
            if let Some(change) = slot.machine.tick(now) {
                slot.pending = Some(change);
            }
        }
        self.flush(core).await
    }
    async fn flush(&mut self, core: &AppCore) -> Result<(), StoreError> {
        let Some(workspace) = core.workspace else {
            return Ok(());
        };
        for slot in &mut self.slots {
            if slot.pending.is_none() {
                continue;
            }
            slot.refresh(core).await?;
            let (Some(id), Some(change)) = (slot.binding, slot.pending) else {
                continue;
            };
            if self
                .last_append
                .get(&id)
                .is_some_and(|last| self.clock.now().saturating_sub(*last) < APPEND_INTERVAL)
            {
                continue;
            }
            let agent = core
                .store
                .own_agent(workspace, core.signer.device_id(), id)
                .await?
                .ok_or(StoreError::InvalidBinding)?;
            if (agent.state, agent.provenance, agent.tool) != (change.0, Some(change.1), change.2) {
                core.store
                    .append_own(
                        workspace,
                        EventBody::AgentState {
                            agent_local_id: id,
                            state: change.0,
                            provenance: change.1,
                            tool: change.2,
                            task_label: None,
                        },
                        core.signer.clone(),
                    )
                    .await?;
                // Measure the next interval from completed persistence, not from an earlier
                // receive time: a slow DB must not permit a burst of delayed writes.
                self.last_append.insert(id, self.clock.now());
                core.changed();
            }
            slot.pending = None;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "ingest_tests.rs"]
mod tests;
