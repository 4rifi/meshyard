#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use crate::{commands::*, state::AppState, view::*};
use meshyard_core::{
    ActivityState as S, HookEvent as H, Provenance as P, RuleId as R, ToolCategory as T, *,
};
use std::sync::atomic::AtomicU64;
use tokio::io::AsyncWriteExt;

#[derive(Default)]
struct FakeClock(AtomicU64);
impl FakeClock {
    fn set(&self, millis: u64) {
        self.0.store(millis, Ordering::Relaxed);
    }
}
impl Clock for FakeClock {
    fn now(&self) -> Duration {
        Duration::from_millis(self.0.load(Ordering::Relaxed))
    }
}
fn private() -> tempfile::TempDir {
    let root = PathBuf::from(std::env::var_os("TMPDIR").expect("USB TMPDIR"));
    assert!(root.starts_with("/Volumes/MeshyardProjects/"));
    tempfile::Builder::new()
        .prefix("b-")
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir_in(root)
        .unwrap()
}
fn ready(status: AppStatus) -> WorkspaceView {
    match status {
        AppStatus::Ready(view) => view,
        other => panic!("expected Ready: {other:?}"),
    }
}
async fn profile(register: bool) -> (tempfile::TempDir, AppState) {
    let dir = private();
    let state = AppState::open(dir.path().to_owned(), true).await;
    ready(
        create_workspace(
            &state,
            CreateWorkspace {
                name: "Prep".into(),
                display_name: "Test".into(),
                mode: Mode::Strict,
            },
        )
        .await,
    );
    if register {
        ready(
            register_agent(
                &state,
                RegisterAgent {
                    kind: AgentKind::ClaudeCode,
                    label: "main".into(),
                },
            )
            .await,
        );
    }
    state.shutdown().await;
    (dir, state)
}
// Synthetic local v1 envelopes using the scrubbed T-007a fixture session/event identifiers.
// Shell testing is a synthetic extension; these are not captured CLI payloads.
const PROMPT: &[u8] = b"{\"v\":1,\"agent\":\"claude_code\",\"event\":\"UserPromptSubmit\",\"session_id\":\"abc123\"}\n";
const TEST: &[u8] = b"{\"v\":1,\"agent\":\"claude_code\",\"event\":\"PreToolUse\",\"session_id\":\"abc123\",\"tool\":\"shell\",\"command_class\":\"test\"}\n";
const EDIT: &[u8] = b"{\"v\":1,\"agent\":\"claude_code\",\"event\":\"PreToolUse\",\"session_id\":\"abc123\",\"tool\":\"edit\"}\n";
const STOP: &[u8] =
    b"{\"v\":1,\"agent\":\"claude_code\",\"event\":\"Stop\",\"session_id\":\"abc123\"}\n";
async fn events(state: &AppState) -> Vec<Event> {
    let guard = state.0.lock().await;
    let core = guard.as_ref().unwrap();
    core.store
        .events_after(core.workspace.unwrap(), core.signer.device_id(), 0, 256)
        .await
        .unwrap()
        .into_iter()
        .map(|raw| {
            let event =
                verify_and_decode(&decode_signed_event(&raw).unwrap(), &Ed25519Verifier).unwrap();
            event.validate_received().unwrap();
            event.into_event()
        })
        .collect()
}
async fn apply(
    state: &AppState,
    pipeline: &mut Pipeline,
    clock: &FakeClock,
    line: &[u8],
    millis: u64,
) {
    clock.set(millis);
    let guard = state.0.lock().await;
    pipeline
        .receive(
            guard.as_ref().unwrap(),
            ingest::parse(line).unwrap(),
            &state.2,
        )
        .await
        .unwrap();
}
async fn wait_counter(counter: &AtomicU32, expected: u32) {
    time::timeout(Duration::from_secs(2), async {
        while counter.load(Ordering::Relaxed) < expected {
            time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
}
async fn wait_state(state: &AppState, expected: S) {
    let mut last_state = None;
    let mut last_error = None;
    let result = time::timeout(Duration::from_secs(6), async {
        loop {
            match app_status(state).await {
                AppStatus::Ready(view) => {
                    last_state = view.agents.first().and_then(|a| a.state);
                    last_error = None;
                    if last_state == Some(expected) {
                        break;
                    }
                }
                AppStatus::Error(error) => last_error = Some(error),
                AppStatus::Setup => {
                    last_state = None;
                    last_error = None;
                }
            }
            time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await;
    assert!(
        result.is_ok(),
        "state deadline: expected={expected:?} last_state={last_state:?} last_error={last_error:?} received={} invalid={} unbound={} storage={} readers={} excess={} listening={} unavailable={} ticks={} flushes={} received_ms={} tick_ms={} flush_ms={}",
        state.2.received.load(Ordering::Relaxed),
        state.2.dropped_invalid.load(Ordering::Relaxed),
        state.2.dropped_unbound.load(Ordering::Relaxed),
        state.2.storage_errors.load(Ordering::Relaxed),
        state.2.active_readers.load(Ordering::Relaxed),
        state.2.dropped_excess.load(Ordering::Relaxed),
        state.2.listening.load(Ordering::Relaxed),
        state.2.unavailable.load(Ordering::Relaxed),
        state.2.trace.ticks.load(Ordering::Relaxed),
        state.2.trace.flushes.load(Ordering::Relaxed),
        state.2.trace.received_ms.load(Ordering::Relaxed),
        state.2.trace.tick_ms.load(Ordering::Relaxed),
        state.2.trace.flush_ms.load(Ordering::Relaxed)
    );
}
async fn send(path: &Path, line: &[u8]) {
    let mut stream = UnixStream::connect(path).await.unwrap();
    stream.write_all(line).await.unwrap();
    stream.shutdown().await.unwrap();
}
fn start(dir: &Path, state: &AppState, uid: u32, clock: Arc<FakeClock>) -> Server {
    // Preserve the actual OS error (EPERM in restricted workers), never skip it as a pass.
    let (uds, file) = listener(dir).unwrap();
    state.2.listening.store(true, Ordering::Relaxed);
    let (stop, stopped) = oneshot::channel();
    Server {
        stop: Some(stop),
        task: Some(tokio::spawn(serve(
            uds,
            file,
            Arc::downgrade(&state.0),
            state.2.clone(),
            uid,
            clock,
            stopped,
        ))),
    }
}

#[tokio::test]
async fn prep_no_binding_no_event_and_identical_repeat_no_append() {
    let (_dir, state) = profile(false).await;
    let clock = Arc::new(FakeClock::default());
    let mut pipeline = Pipeline::new(clock.clone());
    apply(&state, &mut pipeline, &clock, PROMPT, 0).await;
    assert_eq!(state.2.dropped_unbound.load(Ordering::Relaxed), 1);
    assert_eq!(events(&state).await.len(), 1);
    ready(
        register_agent(
            &state,
            RegisterAgent {
                kind: AgentKind::ClaudeCode,
                label: "main".into(),
            },
        )
        .await,
    );
    apply(&state, &mut pipeline, &clock, PROMPT, 0).await;
    apply(&state, &mut pipeline, &clock, PROMPT, 100).await;
    assert_eq!(events(&state).await.len(), 3);
    // Restart the pipeline: equality uses signed store state, not just an in-memory cache.
    apply(
        &state,
        &mut Pipeline::new(clock.clone()),
        &clock,
        PROMPT,
        200,
    )
    .await;
    assert_eq!(events(&state).await.len(), 3);
    state.close().await.unwrap();
}
#[tokio::test]
async fn prep_latest_coalescing_provenance_tool_rate_and_fake_idle() {
    let (_dir, state) = profile(true).await;
    let clock = Arc::new(FakeClock::default());
    let mut pipeline = Pipeline::new(clock.clone());
    apply(&state, &mut pipeline, &clock, PROMPT, 0).await;
    apply(&state, &mut pipeline, &clock, EDIT, 1).await;
    apply(&state, &mut pipeline, &clock, TEST, 2).await;
    assert_eq!(events(&state).await.len(), 3);
    {
        let guard = state.0.lock().await;
        {
            clock.set(49);
            pipeline.flush(guard.as_ref().unwrap())
        }
        .await
        .unwrap();
        assert_eq!(pipeline.slots[0].pending.unwrap().0, S::Testing);
        {
            clock.set(50);
            pipeline.flush(guard.as_ref().unwrap())
        }
        .await
        .unwrap();
    }
    assert_eq!(
        ready(app_status(&state).await).agents[0].state,
        Some(S::Testing)
    );
    // Same state, changed provenance/tool must append.
    let read = b"{\"v\":1,\"agent\":\"claude_code\",\"event\":\"PreToolUse\",\"session_id\":\"abc123\",\"tool\":\"read\"}\n";
    apply(&state, &mut pipeline, &clock, read, 100).await;
    apply(&state, &mut pipeline, &clock, PROMPT, 150).await;
    apply(&state, &mut pipeline, &clock, STOP, 200).await;
    {
        let guard = state.0.lock().await;
        {
            clock.set(60199);
            pipeline.tick(guard.as_ref().unwrap())
        }
        .await
        .unwrap();
    }
    assert_eq!(
        ready(app_status(&state).await).agents[0].state,
        Some(S::Finished)
    );
    {
        let guard = state.0.lock().await;
        {
            clock.set(60200);
            pipeline.tick(guard.as_ref().unwrap())
        }
        .await
        .unwrap();
    }
    assert_eq!(
        ready(app_status(&state).await).agents[0].state,
        Some(S::Idle)
    );
    let activity: Vec<_> = events(&state)
        .await
        .into_iter()
        .filter_map(|e| match e.body {
            EventBody::AgentState {
                state,
                provenance,
                tool,
                task_label,
                ..
            } => {
                assert_eq!(task_label, None);
                Some((state, provenance, tool))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        activity,
        vec![
            (
                S::Thinking,
                P::Verified {
                    source: H::UserPromptSubmit
                },
                None
            ),
            (
                S::Testing,
                P::Inferred { rule: R::ToolShell },
                Some(T::Shell)
            ),
            (
                S::Thinking,
                P::Inferred { rule: R::ToolRead },
                Some(T::Read)
            ),
            (
                S::Thinking,
                P::Verified {
                    source: H::UserPromptSubmit
                },
                None
            ),
            (S::Finished, P::Verified { source: H::Stop }, None),
            (S::Idle, P::Inferred { rule: R::TurnEnded }, None),
        ]
    );
    // A sustained changing stream cannot exceed twenty appends in a half-open second.
    let before = events(&state).await.len();
    apply(&state, &mut pipeline, &clock, PROMPT, 61000).await;
    for millis in 61001..62000 {
        apply(
            &state,
            &mut pipeline,
            &clock,
            if (millis / 50) % 2 == 0 { EDIT } else { TEST },
            millis,
        )
        .await;
    }
    assert_eq!(events(&state).await.len() - before, 20);
    state.close().await.unwrap();
}
#[tokio::test]
async fn prep_binding_switch_discards_pending_old_agent_state() {
    let (_dir, state) = profile(true).await;
    let clock = Arc::new(FakeClock::default());
    let mut pipeline = Pipeline::new(clock.clone());
    apply(&state, &mut pipeline, &clock, PROMPT, 0).await;
    apply(&state, &mut pipeline, &clock, TEST, 1).await;
    let view = ready(
        register_agent(
            &state,
            RegisterAgent {
                kind: AgentKind::ClaudeCode,
                label: "second".into(),
            },
        )
        .await,
    );
    let new = view.agents.iter().find(|a| a.label == "second").unwrap();
    let guard = state.0.lock().await;
    let core = guard.as_ref().unwrap();
    let (_, projection) = core
        .store
        .workspace_state(core.workspace.unwrap())
        .await
        .unwrap();
    let agent = projection
        .agents
        .values()
        .find(|a| a.label == new.label)
        .unwrap();
    core.store
        .bind_own_agent(core.workspace.unwrap(), agent.local_id, core.signer.clone())
        .await
        .unwrap();
    {
        clock.set(1000);
        pipeline.flush(core)
    }
    .await
    .unwrap();
    drop(guard);
    assert_eq!(events(&state).await.len(), 4); // profile, two registrations, one Thinking
    assert_eq!(
        ready(app_status(&state).await)
            .agents
            .iter()
            .find(|a| a.label == "second")
            .unwrap()
            .state,
        None
    );
    state.close().await.unwrap();
}
#[tokio::test]
async fn prep_refuses_nonsocket_symlink_and_overlong_path_without_deletion() {
    let dir = private();
    let ingest = private_directory(&dir.path().join("ingest")).unwrap();
    let path = ingest.join("ingest.sock");
    std::fs::write(&path, b"keep").unwrap();
    assert!(listener(dir.path()).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"keep");
    std::fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink("missing-target", &path).unwrap();
    assert!(listener(dir.path()).is_err());
    assert!(
        std::fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    let long = dir.path().join("a".repeat(104));
    std::fs::create_dir(&long).unwrap();
    std::fs::set_permissions(&long, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(listener(&long).is_err());
    assert!(!long.join("ingest/ingest.sock").exists());
}

#[tokio::test]
async fn uds_wrong_expected_uid_is_rejected() {
    let (dir, state) = profile(true).await;
    let uid = rustix::process::geteuid().as_raw().wrapping_add(1);
    let server = start(dir.path(), &state, uid, Arc::default());
    let mut stream = UnixStream::connect(dir.path().join("ingest/ingest.sock"))
        .await
        .unwrap();
    let _ = stream.write_all(PROMPT).await; // peer may close before the rejected writer runs
    wait_counter(&state.2.dropped_invalid, 1).await;
    assert_eq!(events(&state).await.len(), 2);
    server.stop().await;
    assert!(!dir.path().join("ingest/ingest.sock").exists());
    state.close().await.unwrap();
}
#[tokio::test]
async fn uds_oversized_slow_malformed_and_ninth_connection_are_bounded() {
    let (dir, state) = profile(true).await;
    let server = start(
        dir.path(),
        &state,
        rustix::process::geteuid().as_raw(),
        Arc::default(),
    );
    let path = dir.path().join("ingest/ingest.sock");
    send(&path, &[b'x'; 1025]).await;
    send(&path, b"not-json\n").await;
    let _slow = UnixStream::connect(&path).await.unwrap();
    wait_counter(&state.2.dropped_invalid, 3).await;
    let mut held = Vec::new();
    for _ in 0..8 {
        held.push(UnixStream::connect(&path).await.unwrap());
    }
    wait_counter(&state.2.active_readers, 8).await;
    let mut ninth = UnixStream::connect(&path).await.unwrap();
    wait_counter(&state.2.dropped_excess, 1).await;
    let mut byte = [0];
    assert_eq!(
        time::timeout(Duration::from_millis(100), ninth.read(&mut byte))
            .await
            .unwrap()
            .unwrap(),
        0
    );
    drop(held);
    assert_eq!(events(&state).await.len(), 2);
    server.stop().await;
    state.close().await.unwrap();
}
#[tokio::test]
async fn uds_fixture_envelopes_to_signed_store_app_status_and_fake_idle() {
    let (dir, state) = profile(true).await;
    let clock = Arc::new(FakeClock::default());
    let server = start(
        dir.path(),
        &state,
        rustix::process::geteuid().as_raw(),
        clock.clone(),
    );
    let path = dir.path().join("ingest/ingest.sock");
    assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
    send(&path, PROMPT).await;
    wait_state(&state, S::Thinking).await;
    clock.set(100);
    send(&path, PROMPT).await;
    wait_counter(&state.2.received, 2).await;
    assert_eq!(events(&state).await.len(), 3); // repeated state is not appended
    let unbound =
        b"{\"v\":1,\"agent\":\"codex\",\"event\":\"UserPromptSubmit\",\"session_id\":\"abc123\"}\n";
    send(&path, unbound).await;
    wait_counter(&state.2.dropped_unbound, 1).await;
    assert_eq!(events(&state).await.len(), 3);
    clock.set(200);
    send(&path, TEST).await;
    wait_state(&state, S::Testing).await;
    clock.set(300);
    send(&path, STOP).await;
    wait_state(&state, S::Finished).await;
    clock.set(60300);
    // Same server/pipeline sees its real five-second tick, with injected receive time.
    wait_state(&state, S::Idle).await;
    server.stop().await;
    assert_eq!(
        events(&state)
            .await
            .iter()
            .filter(|e| matches!(
                e.body,
                EventBody::AgentState {
                    task_label: None,
                    ..
                }
            ))
            .count(),
        4
    );
    state.close().await.unwrap();
}
#[tokio::test]
async fn uds_existing_profile_and_create_lifecycle_start_and_shutdown_listener() {
    let dir = private();
    let state = AppState::open(dir.path().to_owned(), true).await;
    assert!(!state.2.listening.load(Ordering::Relaxed));
    ready(
        create_workspace(
            &state,
            CreateWorkspace {
                name: "Lifecycle".into(),
                display_name: "Test".into(),
                mode: Mode::Strict,
            },
        )
        .await,
    );
    assert!(
        state.2.listening.load(Ordering::Relaxed),
        "actual socket bind required"
    );
    let path = dir.path().join("ingest/ingest.sock");
    state.close().await.unwrap();
    assert!(!path.exists());
    let identity = std::fs::read(dir.path().join("identity")).unwrap();
    let reopened = AppState::open(dir.path().to_owned(), true).await;
    assert!(reopened.2.listening.load(Ordering::Relaxed));
    assert_eq!(
        std::fs::read(dir.path().join("identity")).unwrap(),
        identity
    );
    reopened.close().await.unwrap();
    assert!(!path.exists());
}

#[tokio::test]
async fn prep_counted_app_status_idle_and_binding_use_projection_across_store_clones() {
    use meshyard_store::Store;
    use std::sync::atomic::AtomicUsize;
    let dir = private();
    let count = Arc::new(AtomicUsize::new(0));
    let store = Store::open_counted(dir.path(), count.clone()).unwrap();
    let signer = Arc::new(SecretKey::from_bytes(&[101; 32]));
    let mut core = AppCore {
        store: store.clone(),
        signer: signer.clone(),
        workspace: None,
        app: None,
    };
    ready(
        core.create(CreateWorkspace {
            name: "Counted".into(),
            display_name: "Test".into(),
            mode: Mode::Strict,
        })
        .await
        .unwrap(),
    );
    ready(
        core.register(RegisterAgent {
            kind: AgentKind::ClaudeCode,
            label: "main".into(),
        })
        .await
        .unwrap(),
    );
    let w = core.workspace.unwrap();
    let id = store
        .agent_binding(w, AgentKind::ClaudeCode, signer.clone())
        .await
        .unwrap()
        .unwrap();
    // All are actual accepted, signed owner events; no raw-SQL seeding or fake verifier.
    for _ in 2..5000 {
        store
            .append_own(
                w,
                EventBody::ProfileUpdated {
                    display_name: "Test".into(),
                    avatar: 0,
                },
                signer.clone(),
            )
            .await
            .unwrap();
    }
    let member = Arc::new(SecretKey::from_bytes(&[102; 32]));
    store
        .append_own(
            w,
            EventBody::MemberAdded {
                device: member.device_id(),
                display_name: "member".into(),
            },
            signer.clone(),
        )
        .await
        .unwrap();
    store
        .append_own(
            w,
            EventBody::AgentRegistered {
                agent_local_id: [1; 16],
                kind: AgentKind::Codex,
                label: "remote".into(),
            },
            member,
        )
        .await
        .unwrap();
    assert_eq!(
        store.contiguous_heads(w).await.unwrap()[&signer.device_id()],
        5001
    );
    let clock = Arc::new(FakeClock::default());
    let mut pipeline = Pipeline::new(clock.clone());
    count.store(0, Ordering::Relaxed);
    for _ in 0..100 {
        ready(core.status().await.unwrap());
        pipeline.tick(&core).await.unwrap();
        store.workspace_snapshot(w).await.unwrap();
        store
            .own_agent(w, signer.device_id(), id)
            .await
            .unwrap()
            .unwrap();
        store
            .agent_binding(w, AgentKind::ClaudeCode, signer.clone())
            .await
            .unwrap()
            .unwrap();
        store.bind_own_agent(w, id, signer.clone()).await.unwrap();
        store
            .bind_first_own_agent(w, id, signer.clone())
            .await
            .unwrap();
    }
    assert_eq!(
        count.load(Ordering::Relaxed),
        0,
        "status/idle/bind and cloned handles must not replay history or genesis"
    );
    let status = IngestStatus::default();
    for (line, millis) in [(PROMPT, 0), (TEST, 50), (STOP, 100)] {
        count.store(0, Ordering::Relaxed);
        clock.set(millis);
        pipeline
            .receive(&core, ingest::parse(line).unwrap(), &status)
            .await
            .unwrap();
        assert_eq!(
            count.load(Ordering::Relaxed),
            1,
            "one new verification per ingest append, independent of N"
        );
    }
    count.store(0, Ordering::Relaxed);
    clock.set(60099);
    pipeline.tick(&core).await.unwrap();
    assert_eq!(
        ready(core.status().await.unwrap())
            .agents
            .iter()
            .find(|a| a.label == "main")
            .unwrap()
            .state,
        Some(S::Finished)
    );
    assert_eq!(count.load(Ordering::Relaxed), 0);
    clock.set(60100);
    pipeline.tick(&core).await.unwrap();
    assert_eq!(
        ready(core.status().await.unwrap())
            .agents
            .iter()
            .find(|a| a.label == "main")
            .unwrap()
            .state,
        Some(S::Idle)
    );
    assert_eq!(
        count.load(Ordering::Relaxed),
        1,
        "only the newly signed idle event"
    );
    // A fresh status must use the DB projection, not a replay or an AppCore cache.
    // Poison verified source bytes after construction; the explicit audit must detect this.
    let sql = rusqlite::Connection::open(dir.path().join("meshyard.sqlite")).unwrap();
    sql.execute(
        "UPDATE events SET signed_blob=x'00' WHERE workspace=?1 AND author=?2 AND seq=1",
        rusqlite::params![w.0.as_slice(), signer.device_id().0.as_slice()],
    )
    .unwrap();
    count.store(0, Ordering::Relaxed);
    for _ in 0..100 {
        ready(core.status().await.unwrap());
        pipeline.tick(&core).await.unwrap();
    }
    assert_eq!(count.load(Ordering::Relaxed), 0);
    assert!(
        store.workspace_state(w).await.is_err(),
        "audit still verifies and rejects damaged history"
    );
    drop(sql);
    store.close().await.unwrap();
}

#[tokio::test]
async fn prep_rate_deadline_starts_after_async_persistence_not_receive_sample() {
    use meshyard_store::Store;
    use std::sync::atomic::AtomicUsize;
    // Model a database operation consuming monotonic time: a completed strict verification
    // advances the clock by 100ms. This is independent of OS scheduling and wall-clock sleeps.
    struct PersistenceClock {
        base: AtomicU64,
        verified: Arc<AtomicUsize>,
    }
    impl Clock for PersistenceClock {
        fn now(&self) -> Duration {
            Duration::from_millis(
                self.base.load(Ordering::Relaxed)
                    + 100 * self.verified.load(Ordering::Relaxed) as u64,
            )
        }
    }
    let dir = private();
    let verified = Arc::new(AtomicUsize::new(0));
    let store = Store::open_counted(dir.path(), verified.clone()).unwrap();
    let signer = Arc::new(SecretKey::from_bytes(&[106; 32]));
    let mut core = AppCore {
        store: store.clone(),
        signer: signer.clone(),
        workspace: None,
        app: None,
    };
    ready(
        core.create(CreateWorkspace {
            name: "Deadline".into(),
            display_name: "Test".into(),
            mode: Mode::Strict,
        })
        .await
        .unwrap(),
    );
    ready(
        core.register(RegisterAgent {
            kind: AgentKind::ClaudeCode,
            label: "main".into(),
        })
        .await
        .unwrap(),
    );
    verified.store(0, Ordering::Relaxed);
    let clock = Arc::new(PersistenceClock {
        base: AtomicU64::new(0),
        verified: verified.clone(),
    });
    let mut pipeline = Pipeline::new(clock.clone());
    let status = IngestStatus::default();
    pipeline
        .receive(&core, ingest::parse(PROMPT).unwrap(), &status)
        .await
        .unwrap();
    assert_eq!(verified.load(Ordering::Relaxed), 1);
    assert_eq!(
        pipeline.last_append.values().copied().collect::<Vec<_>>(),
        vec![Duration::from_millis(100)]
    );
    pipeline
        .receive(&core, ingest::parse(TEST).unwrap(), &status)
        .await
        .unwrap();
    assert_eq!(
        verified.load(Ordering::Relaxed),
        1,
        "DB time cannot authorize an immediate second append"
    );
    clock.base.store(49, Ordering::Relaxed);
    pipeline.flush(&core).await.unwrap();
    assert_eq!(verified.load(Ordering::Relaxed), 1);
    clock.base.store(50, Ordering::Relaxed);
    pipeline.flush(&core).await.unwrap();
    assert_eq!(
        verified.load(Ordering::Relaxed),
        2,
        "coalesced state becomes eligible at exactly 50ms after completion"
    );
    assert_eq!(
        ready(core.status().await.unwrap()).agents[0].state,
        Some(S::Testing)
    );
    store.close().await.unwrap();
}
