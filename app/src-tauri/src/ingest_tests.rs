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
    time::timeout(Duration::from_secs(6), async {
        loop {
            if ready(app_status(state).await).agents[0].state == Some(expected) {
                break;
            }
            time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
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
