#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::IngestStatus;
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
        "state deadline: expected={expected:?} last_state={last_state:?} last_error={last_error:?} received={} invalid={} unbound={} storage={} readers={} excess={} listening={} unavailable={} ticks={} flushes={} received_ms={} tick_ms={} flush_ms={} rejections={:?}",
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
        state.2.trace.flush_ms.load(Ordering::Relaxed),
        state.2.rejections
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

#[test]
fn reactor_absent_runtime_fails_closed_before_socket_work() {
    let outcome = std::panic::catch_unwind(|| {
        let dir = private();
        let status = Arc::new(IngestStatus::default());
        // Even a previously published flag must be cleared on rejection.
        status.listening.store(true, Ordering::Relaxed);
        assert!(Server::start(dir.path(), Weak::new(), status.clone()).is_none());
        assert!(status.unavailable.load(Ordering::Relaxed));
        assert!(!status.listening.load(Ordering::Relaxed));
        assert!(!dir.path().join("ingest").exists());
        let error = listener(dir.path())
            .err()
            .expect("missing runtime rejected");
        assert_eq!(error.kind(), std::io::ErrorKind::Unsupported);
    });
    assert!(outcome.is_ok(), "startup must not propagate a panic");
}

#[test]
fn reactor_driver_matrix_preserves_workspace_and_cleans_socket() {
    for (io, timers) in [(false, false), (true, false), (false, true), (true, true)] {
        let outcome = std::panic::catch_unwind(|| {
            let mut builder = tokio::runtime::Builder::new_current_thread();
            match (io, timers) {
                (true, true) => {
                    builder.enable_all();
                }
                (true, false) => {
                    builder.enable_io();
                }
                (false, true) => {
                    builder.enable_time();
                }
                (false, false) => {}
            }
            let runtime = builder.build().unwrap();
            runtime.block_on(async {
                let dir = private();
                let path = dir.path().join("ingest/ingest.sock");
                let usable = io && timers;
                // Inspect the typed boundary result too: EPERM must fail this test,
                // never masquerade as a successfully caught missing-driver panic.
                match listener(dir.path()) {
                    Ok((resources, file)) => {
                        assert!(usable, "missing driver incorrectly accepted");
                        assert_eq!(resources.flush.period(), APPEND_INTERVAL);
                        assert_eq!(resources.tick.period(), Duration::from_secs(5));
                        assert_eq!(resources.flush.missed_tick_behavior(), time::MissedTickBehavior::Skip);
                        assert_eq!(resources.tick.missed_tick_behavior(), time::MissedTickBehavior::Skip);
                        assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
                        drop(resources);
                        drop(file);
                    }
                    Err(error) => {
                        assert!(!usable, "enabled runtime must bind the actual OS socket: {error}");
                        assert_eq!(error.kind(), std::io::ErrorKind::Unsupported,
                            "actual OS bind/registration required; permission errors are not reactor evidence: {error}");
                    }
                }
                assert!(!path.exists(), "resource rejection/drop must remove the owned socket");
                let state = AppState::open(dir.path().to_owned(), true).await;
                assert!(matches!(app_status(&state).await, AppStatus::Setup));
                ready(create_workspace(&state, CreateWorkspace {
                    name: "Reactor".into(), display_name: "Test".into(), mode: Mode::Strict,
                }).await);
                assert_eq!(state.2.unavailable.load(Ordering::Relaxed), !usable);
                assert_eq!(state.2.listening.load(Ordering::Relaxed), usable);
                assert_eq!(path.exists(), usable);
                ready(app_status(&state).await);
                let identity = std::fs::read(dir.path().join("identity")).unwrap();
                state.close().await.unwrap();
                assert!(!path.exists());

                // Reopening an existing workspace also starts ingest; it must remain
                // usable with a scheduler even when either optional driver is absent.
                let reopened = AppState::open(dir.path().to_owned(), true).await;
                assert_eq!(ready(app_status(&reopened).await).name, "Reactor");
                assert_eq!(reopened.2.unavailable.load(Ordering::Relaxed), !usable);
                assert_eq!(reopened.2.listening.load(Ordering::Relaxed), usable);
                assert_eq!(path.exists(), usable);
                assert_eq!(std::fs::read(dir.path().join("identity")).unwrap(), identity);
                if usable {
                    ready(register_agent(&reopened, RegisterAgent {
                        kind: AgentKind::ClaudeCode, label: "main".into(),
                    }).await);
                    send(&path, PROMPT).await;
                    wait_state(&reopened, S::Thinking).await;
                    assert_eq!(reopened.2.received.load(Ordering::Relaxed), 1);
                    assert_eq!(reopened.2.dropped_invalid.load(Ordering::Relaxed), 0);
                    assert_eq!(events(&reopened).await.len(), 3);
                }
                reopened.close().await.unwrap();
                assert!(!path.exists(), "shutdown must remove the owned socket");
            });
        });
        assert!(
            outcome.is_ok(),
            "outer unwind or failed assertion: io={io} time={timers}"
        );
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
    assert_eq!(state.2.rejections.uid_mismatch.load(Ordering::Relaxed), 1);
    assert_eq!(state.2.rejections.peercred_error.load(Ordering::Relaxed), 0);
    assert_eq!(state.2.received.load(Ordering::Relaxed), 0);
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
    assert_eq!(state.2.rejections.oversize.load(Ordering::Relaxed), 1);
    assert_eq!(state.2.rejections.parse.load(Ordering::Relaxed), 1);
    assert_eq!(state.2.rejections.timeout.load(Ordering::Relaxed), 1);
    assert_eq!(state.2.rejections.peercred_error.load(Ordering::Relaxed), 0);
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
    install_fixture(&state, dir.path()).await;
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
    assert_eq!(
        ready(app_status(&state).await).agents[0].connection,
        AgentConnection::Connected
    );
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
        ingest: Arc::default(),
        installer: None,
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
        ingest: Arc::default(),
        installer: None,
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

// Pure reader probes exercise the production authorization/read path without OS sockets.
struct ReadProbe<R> {
    inner: R,
    polls: Arc<AtomicU32>,
}
impl<R: AsyncRead + Unpin> AsyncRead for ReadProbe<R> {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        count(&self.polls);
        std::pin::Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}
fn rejection_counts(status: &IngestStatus) -> [u32; 8] {
    let r = &status.rejections;
    [
        &r.peercred_error,
        &r.uid_mismatch,
        &r.read_io,
        &r.eof_empty,
        &r.timeout,
        &r.oversize,
        &r.parse,
        &r.reader_task_error,
    ]
    .map(|n| n.load(Ordering::Relaxed))
}
#[tokio::test]
async fn prep_credential_error_and_wrong_uid_reject_before_read_or_state_mutation() {
    let (_dir, state) = profile(true).await;
    let before = events(&state).await;
    let expected = rustix::process::geteuid().as_raw();
    // An entire valid frame is already available. Any attempted read is a failure.
    for lookup in [
        Err(std::io::Error::from_raw_os_error(57)),
        Ok(expected.wrapping_add(1)),
    ] {
        let polls = Arc::new(AtomicU32::new(0));
        let reader = ReadProbe {
            inner: PROMPT,
            polls: polls.clone(),
        };
        let mut pipeline = Pipeline::new(Arc::new(FakeClock::default()));
        if let Some(input) = read_one_with(reader, expected, &state.2, |_| lookup).await {
            pipeline
                .receive(state.0.lock().await.as_ref().unwrap(), input, &state.2)
                .await
                .unwrap();
        }
        assert_eq!(
            polls.load(Ordering::Relaxed),
            0,
            "authentication must precede all reads"
        );
        assert_eq!(events(&state).await, before);
        assert_eq!(state.2.received.load(Ordering::Relaxed), 0);
    }
    assert_eq!(rejection_counts(&state.2), [1, 1, 0, 0, 0, 0, 0, 0]);
    assert_eq!(
        state
            .2
            .rejections
            .peercred_raw_errno
            .load(Ordering::Relaxed),
        57
    );
    assert_eq!(state.2.dropped_invalid.load(Ordering::Relaxed), 2);
    state.close().await.unwrap();
}
struct BrokenRead;
impl AsyncRead for BrokenRead {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        _buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Err(std::io::ErrorKind::ConnectionReset.into()))
    }
}
#[tokio::test]
async fn prep_reader_stages_count_once_and_accept_exact_limit_without_retaining_text() {
    let status = IngestStatus::default();
    let uid = rustix::process::geteuid().as_raw();
    assert!(
        read_one_with(BrokenRead, uid, &status, |_| Ok(uid))
            .await
            .is_none()
    );
    for line in [b"".as_slice(), b"truncated", b"not-json\n"] {
        assert!(
            read_one_with(line, uid, &status, |_| Ok(uid))
                .await
                .is_none()
        );
    }
    assert!(
        read_one_with(&[b'x'; READ_LIMIT][..], uid, &status, |_| Ok(uid))
            .await
            .is_none()
    );
    let (reader, _held_writer) = tokio::io::duplex(1);
    assert!(
        read_one_with(reader, uid, &status, |_| Ok(uid))
            .await
            .is_none()
    );
    assert_eq!(rejection_counts(&status), [0, 0, 1, 2, 1, 1, 1, 0]);
    assert_eq!(status.dropped_invalid.load(Ordering::Relaxed), 6);
    // A frame exactly 1 KiB, including its newline, is permitted.
    let mut exact = PROMPT[..PROMPT.len() - 1].to_vec();
    exact.resize(READ_LIMIT - 1, b' ');
    exact.push(b'\n');
    assert!(
        read_one_with(exact.as_slice(), uid, &status, |_| Ok(uid))
            .await
            .is_some()
    );
    assert_eq!(status.dropped_invalid.load(Ordering::Relaxed), 6);
    assert_eq!(
        status.rejections.peercred_raw_errno.load(Ordering::Relaxed),
        0
    );
}
#[tokio::test]
async fn uds_live_peer_reaches_signed_thinking_before_client_closes() {
    let (dir, state) = profile(true).await;
    let server = start(
        dir.path(),
        &state,
        rustix::process::geteuid().as_raw(),
        Arc::default(),
    );
    let mut client = UnixStream::connect(dir.path().join("ingest/ingest.sock"))
        .await
        .unwrap();
    client.write_all(PROMPT).await.unwrap();
    // This is the separate live-peer case. The original send()/fixture lifetime is unchanged.
    wait_state(&state, S::Thinking).await;
    assert_eq!(state.2.received.load(Ordering::Relaxed), 1);
    assert_eq!(state.2.dropped_invalid.load(Ordering::Relaxed), 0);
    assert_eq!(events(&state).await.len(), 3);
    drop(client);
    server.stop().await;
    state.close().await.unwrap();
}
#[cfg(target_os = "macos")]
#[tokio::test]
async fn uds_darwin_buffered_closed_peer_records_old_errno_and_new_uid() {
    let dir = private();
    let (uds, _file) = listener(dir.path()).unwrap();
    // Synchronous client: close/drop before the first server accept/poll. No sleeps or ACK.
    {
        let mut client =
            std::os::unix::net::UnixStream::connect(dir.path().join("ingest/ingest.sock")).unwrap();
        std::io::Write::write_all(&mut client, PROMPT).unwrap();
        client.shutdown(std::net::Shutdown::Both).unwrap();
    }
    let (stream, _) = uds.listener.accept().await.unwrap();
    let old = stream.peer_cred();
    let new = peer_uid(&stream);
    let expected_uid = rustix::process::geteuid().as_raw();
    let old_errno = old.as_ref().err().and_then(std::io::Error::raw_os_error);
    let expected_errno = nix::errno::Errno::ENOTCONN as i32;
    eprintln!(
        "closed_peer old_peercred_error={} old_raw_errno={old_errno:?} expected_raw_errno={expected_errno} new_uid_matches_euid={} new_raw_errno={:?}",
        old.is_err(),
        new.as_ref().is_ok_and(|uid| *uid == expected_uid),
        new.as_ref().err().and_then(std::io::Error::raw_os_error)
    );
    assert!(
        old.is_err(),
        "cause gate: old Tokio accessor must fail on the closed peer"
    );
    assert_eq!(
        old_errno,
        Some(expected_errno),
        "cause gate: preserve any unexpected old-path errno"
    );
    assert_eq!(new.unwrap(), expected_uid);
    let status = IngestStatus::default();
    assert!(read_one(stream, expected_uid, &status).await.is_some());
    assert_eq!(status.dropped_invalid.load(Ordering::Relaxed), 0);
    assert_eq!(rejection_counts(&status), [0; 8]);
}

async fn install_fixture(state: &AppState, dir: &Path) {
    let binary = dir.join("reviewed hook fixture");
    std::fs::write(&binary, b"not executed").unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
    state
        .configure_hooks(dir.join(".claude/settings.json"), Some(binary))
        .await;
    let plan = hook_plan(state).await.unwrap();
    assert_eq!(plan.action, HookAction::Install);
    let waiting = ready(hook_apply(state, plan.plan_id).await);
    assert_eq!(
        waiting.agents[0].connection,
        AgentConnection::WaitingForFirstEvent
    );
}
#[tokio::test]
async fn connect_dto_clock_only_staleness_binding_authority_and_disabled_settings() {
    let (dir, state) = profile(true).await;
    let initial = ready(app_status(&state).await);
    assert_eq!(initial.agents[0].connection, AgentConnection::NotConnected);
    assert!(initial.agents[0].bound && initial.agents[0].is_you);
    assert!(!initial.hook.installed && !initial.hook.binary_available);
    assert_eq!(hook_plan(&state).await, Err(AppError::HookBinaryMissing));
    assert!(!dir.path().join(".claude").exists());
    install_fixture(&state, dir.path()).await;
    let clock = Arc::new(FakeClock::default());
    let mut pipeline = Pipeline::new(clock.clone());
    apply(&state, &mut pipeline, &clock, PROMPT, 0).await;
    let connected = ready(app_status(&state).await);
    assert_eq!(connected.agents[0].connection, AgentConnection::Connected);
    let count = events(&state).await.len();
    {
        let guard = state.0.lock().await;
        let core = guard.as_ref().unwrap();
        let snapshot = core
            .store
            .workspace_snapshot(core.workspace.unwrap())
            .await
            .unwrap();
        let ts = snapshot.agent_state_ts.values().copied().max().unwrap();
        let boundary = ready(core.status_at(ts + 600_000).await.unwrap());
        assert_eq!(boundary.agents[0].connection, AgentConnection::Connected);
        let stale = ready(core.status_at(ts + 600_001).await.unwrap());
        assert!(stale.agents[0].stale);
        assert_eq!(stale.agents[0].state, Some(S::Thinking));
        assert_eq!(stale.agents[0].connection, AgentConnection::Stale);
    }
    assert_eq!(
        events(&state).await.len(),
        count,
        "derived staleness appends no wire event"
    );
    let with_codex = ready(
        register_agent(
            &state,
            RegisterAgent {
                kind: AgentKind::Codex,
                label: "unsupported".into(),
            },
        )
        .await,
    );
    let codex = with_codex
        .agents
        .iter()
        .find(|a| a.kind == AgentKind::Codex)
        .unwrap();
    assert_eq!(codex.connection, AgentConnection::NotConnected);
    assert_eq!(
        bind_agent(&state, codex.id.clone()).await,
        AppStatus::Error(AppError::InvalidInput)
    );
    let (foreign_id, own_id) = {
        let guard = state.0.lock().await;
        let core = guard.as_ref().unwrap();
        let workspace = core.workspace.unwrap();
        let id = core
            .store
            .agent_binding(workspace, AgentKind::ClaudeCode, core.signer.clone())
            .await
            .unwrap()
            .unwrap();
        let remote = Arc::new(SecretKey::from_bytes(&[111; 32]));
        core.store
            .append_own(
                workspace,
                EventBody::MemberAdded {
                    device: remote.device_id(),
                    display_name: "foreign".into(),
                },
                core.signer.clone(),
            )
            .await
            .unwrap();
        core.store
            .append_own(
                workspace,
                EventBody::AgentRegistered {
                    agent_local_id: id,
                    kind: AgentKind::ClaudeCode,
                    label: "same ID".into(),
                },
                remote.clone(),
            )
            .await
            .unwrap();
        let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
        (
            format!("{}:{}", hex(&remote.device_id().0), hex(&id)),
            connected.agents[0].id.clone(),
        )
    };
    let view = ready(app_status(&state).await);
    assert!(
        !view
            .agents
            .iter()
            .find(|a| a.id == foreign_id)
            .unwrap()
            .is_you
    );
    assert_eq!(
        bind_agent(&state, foreign_id).await,
        AppStatus::Error(AppError::InvalidInput)
    );
    assert_eq!(
        ready(bind_agent(&state, own_id).await)
            .agents
            .iter()
            .filter(|a| a.bound)
            .count(),
        1
    );
    let removal = hook_remove(&state).await.unwrap();
    assert!(
        ready(app_status(&state).await).hook.installed,
        "remove remains a review until Apply"
    );
    assert!(
        !ready(hook_apply(&state, removal.plan_id).await)
            .hook
            .installed
    );
    state.close().await.unwrap();
}
