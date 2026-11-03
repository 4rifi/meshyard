//! Local persistence and explicitly selected identity stores. SQL runs on one dedicated thread.
mod database;
pub mod identity;
pub mod process_lock;
pub use database::{Diagnostics, InsertOutcome, RejectReason};
use meshyard_core::{wire::v1::*, *};
use std::{collections::BTreeMap, path::Path, sync::Arc};
use tokio::sync::{mpsc, oneshot};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("data directory is already in use")]
    Busy,
    #[error("unsafe data directory or file permissions")]
    Permissions,
    #[error("local storage I/O failed")]
    Io,
    #[error("database operation failed")]
    Database,
    #[error("database migration failed")]
    Migration,
    #[error("store worker is closed")]
    Closed,
    #[error("workspace not found")]
    UnknownWorkspace,
    #[error("workspace genesis conflicts with stored genesis")]
    GenesisConflict,
    #[error("sequence is outside SQLite's supported 1..=i64::MAX range")]
    SequenceRange,
    #[error("invalid bounded query limit (must be 1..=256)")]
    Limit,
    #[error("event could not be appended")]
    AppendRejected,
    #[error("signature or protocol verification failed: {0}")]
    Protocol(#[from] CryptoError),
}
impl From<rusqlite::Error> for StoreError {
    fn from(_: rusqlite::Error) -> Self {
        Self::Database
    }
}

type Job = Box<dyn FnOnce(&mut database::Database) + Send>;
enum Command {
    Run(Job),
    Close(oneshot::Sender<()>),
}
#[derive(Clone)]
pub struct Store {
    sender: mpsc::Sender<Command>,
}
impl Store {
    /// Initial filesystem/open/migration work is synchronous. Async runtimes may call
    /// `open_async`; all handle operations below use a bounded nonblocking channel.
    pub fn open(data_dir: impl AsRef<Path>) -> Result<Self, StoreError> {
        let lock = process_lock::DataDirLock::acquire(data_dir.as_ref())?;
        let path = lock.directory().join("meshyard.sqlite");
        let (sender, mut receiver) = mpsc::channel::<Command>(64);
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("meshyard-db".into())
            .spawn(move || {
                let mut db = match database::Database::open(&path) {
                    Ok(db) => db,
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                        return;
                    }
                };
                if ready_tx.send(Ok(())).is_err() {
                    return;
                }
                while let Some(command) = receiver.blocking_recv() {
                    match command {
                        Command::Run(job) => job(&mut db),
                        Command::Close(reply) => {
                            drop(db);
                            drop(lock);
                            let _ = reply.send(());
                            return;
                        }
                    }
                }
            })
            .map_err(|_| StoreError::Io)?;
        ready_rx.recv().map_err(|_| StoreError::Closed)??;
        Ok(Self { sender })
    }
    pub async fn open_async(data_dir: std::path::PathBuf) -> Result<Self, StoreError> {
        tokio::task::spawn_blocking(move || Self::open(data_dir))
            .await
            .map_err(|_| StoreError::Closed)?
    }
    async fn call<T: Send + 'static>(
        &self,
        work: impl FnOnce(&mut database::Database) -> Result<T, StoreError> + Send + 'static,
    ) -> Result<T, StoreError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(Command::Run(Box::new(move |db| {
                let _ = tx.send(work(db));
            })))
            .await
            .map_err(|_| StoreError::Closed)?;
        rx.await.map_err(|_| StoreError::Closed)?
    }
    /// Closes all cloned handles and releases the process lock before returning.
    pub async fn close(self) -> Result<(), StoreError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(Command::Close(tx))
            .await
            .map_err(|_| StoreError::Closed)?;
        rx.await.map_err(|_| StoreError::Closed)
    }
    /// Accepts the original outer postcard bytes; stores them without re-encoding.
    pub async fn add_workspace(&self, raw: Vec<u8>) -> Result<WorkspaceId, StoreError> {
        self.call(move |db| db.add_workspace(&raw)).await
    }
    pub async fn genesis(&self, workspace: WorkspaceId) -> Result<Vec<u8>, StoreError> {
        self.call(move |db| db.genesis(workspace)).await
    }
    pub async fn append_own(
        &self,
        workspace: WorkspaceId,
        body: EventBody,
        signer: Arc<dyn Signer>,
    ) -> Result<SignedEventV1, StoreError> {
        self.call(move |db| {
            let epoch = db.current_epoch(workspace)?;
            let millis = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| StoreError::Io)?
                .as_millis();
            let ts_ms = u64::try_from(millis).map_err(|_| StoreError::SequenceRange)?;
            db.append_own(
                workspace,
                body,
                signer.as_ref(),
                SharingPolicy::default(),
                ts_ms,
                epoch,
            )
        })
        .await
    }

    pub async fn append_own_with(
        &self,
        workspace: WorkspaceId,
        body: EventBody,
        signer: Arc<dyn Signer>,
        policy: SharingPolicy,
        ts_ms: u64,
        epoch: u32,
    ) -> Result<SignedEventV1, StoreError> {
        self.call(move |db| db.append_own(workspace, body, signer.as_ref(), policy, ts_ms, epoch))
            .await
    }
    pub async fn insert_remote(&self, raw: Vec<u8>) -> Result<InsertOutcome, StoreError> {
        self.call(move |db| db.insert_remote(&raw)).await
    }
    pub async fn contiguous_heads(
        &self,
        workspace: WorkspaceId,
    ) -> Result<BTreeMap<DeviceId, u64>, StoreError> {
        self.call(move |db| db.heads(workspace)).await
    }
    pub async fn events_after(
        &self,
        workspace: WorkspaceId,
        author: DeviceId,
        seq: u64,
        limit: usize,
    ) -> Result<Vec<Vec<u8>>, StoreError> {
        self.call(move |db| db.events_after(workspace, author, seq, limit))
            .await
    }
    pub async fn diagnostics(
        &self,
        workspace: WorkspaceId,
        author: DeviceId,
    ) -> Result<Diagnostics, StoreError> {
        self.call(move |db| db.diagnostics(workspace, author)).await
    }
}
// Used by the unchanged T-001 scaffold.
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
