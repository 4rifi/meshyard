#![allow(clippy::unwrap_used, clippy::expect_used)]
use meshyard_app::{commands::*, state::AppState, view::*};
use meshyard_core::{wire::v1::*, *};
use meshyard_store::{Store, identity::*};
use std::{
    os::unix::fs::PermissionsExt,
    sync::atomic::{AtomicUsize, Ordering},
};

fn profile() -> tempfile::TempDir {
    let root =
        std::path::PathBuf::from(std::env::var_os("TMPDIR").expect("USB TMPDIR must be set"));
    assert!(
        root.starts_with("/Volumes/TokensAbuse/"),
        "tests must remain on the USB image"
    );
    tempfile::Builder::new()
        .prefix("t014-file-profile-")
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir_in(root)
        .unwrap()
}
fn create_input() -> CreateWorkspace {
    CreateWorkspace {
        name: "Demo".into(),
        display_name: "Dion".into(),
        mode: Mode::Strict,
    }
}
fn ready(status: AppStatus) -> WorkspaceView {
    match status {
        AppStatus::Ready(view) => view,
        other => panic!("expected Ready, got {other:?}"),
    }
}

#[tokio::test]
async fn bad_hook_config_keeps_real_workspace_status_register_and_profile_usable() {
    for symlink in [false, true] {
        let dir = profile();
        let state = AppState::open(dir.path().to_owned(), true).await;
        let initial = ready(create_workspace(&state, create_input()).await);
        assert!(!initial.hook.installed && !initial.hook.binary_available);
        assert_eq!(hook_plan(&state).await, Err(AppError::HookBinaryMissing));
        assert!(!dir.path().join(".claude").exists());

        let settings_parent = dir.path().join(".claude");
        std::fs::create_dir(&settings_parent).unwrap();
        std::fs::set_permissions(&settings_parent, std::fs::Permissions::from_mode(0o755)).unwrap();
        let settings = settings_parent.join("settings.json");
        let target = dir.path().join("untouched-settings-target");
        let expected = if symlink {
            std::fs::write(&target, b"{}").unwrap();
            std::os::unix::fs::symlink(&target, &settings).unwrap();
            AppError::UnsafePermissions
        } else {
            std::fs::write(&settings, b"invalid JSON").unwrap();
            std::fs::set_permissions(&settings, std::fs::Permissions::from_mode(0o600)).unwrap();
            AppError::HookConfigInvalid
        };
        let binary = dir.path().join("explicit hook fixture");
        std::fs::write(&binary, b"test fixture; never executed").unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        state
            .configure_hooks(settings.clone(), Some(binary.clone()))
            .await;
        let view = ready(app_status(&state).await);
        assert!(!view.hook.installed && view.hook.binary_available);
        assert_eq!(hook_plan(&state).await, Err(expected.clone()));
        let registered = ready(
            register_agent(
                &state,
                RegisterAgent {
                    kind: AgentKind::ClaudeCode,
                    label: "still usable".into(),
                },
            )
            .await,
        );
        assert_eq!(registered.agents.len(), 1);
        let bound = ready(bind_agent(&state, registered.agents[0].id.clone()).await);
        assert!(bound.agents[0].bound);
        assert_eq!(bound.agents[0].connection, AgentConnection::NotConnected);
        let updated = ready(update_profile(&state, "Still editable".into()).await);
        assert_eq!(updated.members[0].display_name, "Still editable");
        assert!(!updated.hook.installed && updated.hook.binary_available);
        assert_eq!(hook_plan(&state).await, Err(expected.clone()));

        // Availability comes from the injected file, independently of config parsing.
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o600)).unwrap();
        let unavailable = ready(app_status(&state).await);
        assert!(!unavailable.hook.installed && !unavailable.hook.binary_available);
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(hook_plan(&state).await, Err(expected));
        if symlink {
            assert!(
                std::fs::symlink_metadata(&settings)
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
            assert_eq!(std::fs::read(target).unwrap(), b"{}");
        } else {
            assert_eq!(std::fs::read(settings).unwrap(), b"invalid JSON");
        }
        state.close().await.unwrap();
    }
}
#[tokio::test]
async fn signed_create_profile_register_reopen_and_exclusive_profile() {
    let dir = profile();
    let state = AppState::open(dir.path().to_owned(), true).await;
    assert_eq!(app_status(&state).await, AppStatus::Setup);
    let created = ready(create_workspace(&state, create_input()).await);
    assert_eq!(created.name, "Demo");
    assert_eq!(created.mode, Mode::Strict);
    assert_eq!(created.members.len(), 1);
    assert_eq!(created.members[0].display_name, "Dion");
    assert_eq!(created.members[0].role, Role::Owner);
    assert!(created.members[0].is_you);
    assert!(created.agents.is_empty());
    let second = AppState::open(dir.path().to_owned(), true).await;
    assert_eq!(
        app_status(&second).await,
        AppStatus::Error(AppError::DataDirectoryBusy)
    );
    second.close().await.unwrap();
    assert_eq!(
        create_workspace(&state, create_input()).await,
        AppStatus::Error(AppError::WorkspaceExists)
    );
    assert_eq!(app_status(&state).await, AppStatus::Ready(created.clone()));
    let registered = ready(
        register_agent(
            &state,
            RegisterAgent {
                kind: AgentKind::ClaudeCode,
                label: "main".into(),
            },
        )
        .await,
    );
    assert_eq!(registered.agents.len(), 1);
    assert_eq!(registered.agents[0].state, None);
    assert_eq!(registered.agents[0].provenance, None);
    let updated = ready(update_profile(&state, "Dion updated".into()).await);
    state.close().await.unwrap();
    let reopened = AppState::open(dir.path().to_owned(), true).await;
    assert_eq!(
        app_status(&reopened).await,
        AppStatus::Ready(updated.clone())
    );
    reopened.close().await.unwrap();

    // Inspect actual persisted signed events through Store, with real strict verification.
    let store = Store::open(dir.path()).unwrap();
    let ids = store.workspaces().await.unwrap();
    assert_eq!(ids.len(), 1);
    let raw = store.genesis(ids[0]).await.unwrap();
    let signed: SignedGenesisV1 = postcard::from_bytes(&raw).unwrap();
    let (id, genesis) = verify_genesis(&signed, &Ed25519Verifier).unwrap();
    assert_eq!(id, ids[0]);
    let events = store.events_after(id, genesis.owner, 0, 256).await.unwrap();
    assert_eq!(events.len(), 3); // no redundant MemberAdded or fabricated AgentState
    let bodies: Vec<_> = events
        .into_iter()
        .map(|raw| {
            let event =
                verify_and_decode(&decode_signed_event(&raw).unwrap(), &Ed25519Verifier).unwrap();
            event.validate_received().unwrap();
            event.into_event().body
        })
        .collect();
    assert!(
        matches!(&bodies[0], EventBody::ProfileUpdated { display_name, .. } if display_name == "Dion")
    );
    assert!(matches!(&bodies[1], EventBody::AgentRegistered { label, .. } if label == "main"));
    assert!(
        matches!(&bodies[2], EventBody::ProfileUpdated { display_name, .. } if display_name == "Dion updated")
    );
    store.close().await.unwrap();
    assert_eq!(
        std::fs::metadata(dir.path().join("identity"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}
#[tokio::test]
async fn concurrent_create_and_register_are_serialized_without_lost_updates() {
    let dir = profile();
    let state = AppState::open(dir.path().to_owned(), true).await;
    let (a, b) = tokio::join!(
        create_workspace(&state, create_input()),
        create_workspace(&state, create_input())
    );
    assert_eq!(
        [a, b]
            .iter()
            .filter(|s| matches!(s, AppStatus::Ready(_)))
            .count(),
        1
    );
    let input = || RegisterAgent {
        kind: AgentKind::Codex,
        label: "same label".into(),
    };
    let (a, b) = tokio::join!(
        register_agent(&state, input()),
        register_agent(&state, input())
    );
    assert!(matches!(a, AppStatus::Ready(_)) && matches!(b, AppStatus::Ready(_)));
    let final_view = ready(app_status(&state).await);
    assert_eq!(final_view.agents.len(), 2);
    assert_ne!(final_view.agents[0].id, final_view.agents[1].id);
    state.close().await.unwrap();
    let reopened = AppState::open(dir.path().to_owned(), true).await;
    assert_eq!(app_status(&reopened).await, AppStatus::Ready(final_view));
    reopened.close().await.unwrap();
}
struct RefusingKeys {
    error: IdentityError,
    creates: AtomicUsize,
}
impl DeviceKeyStore for RefusingKeys {
    fn load(&self) -> Result<SecretKey, IdentityError> {
        Err(self.error)
    }
    fn create(&self, _: &SecretKey) -> Result<(), IdentityError> {
        self.creates.fetch_add(1, Ordering::SeqCst);
        panic!("must never create a replacement key")
    }
}
#[tokio::test]
async fn identity_errors_do_not_create_a_key_or_fallback() {
    for (error, expected) in [
        (IdentityError::Locked, AppError::IdentityLocked),
        (IdentityError::Unavailable, AppError::IdentityUnavailable),
        (IdentityError::Invalid, AppError::IdentityInvalid),
        (IdentityError::Permissions, AppError::UnsafePermissions),
    ] {
        let dir = profile();
        let keys = RefusingKeys {
            error,
            creates: AtomicUsize::new(0),
        };
        let state = AppState::open_with_keys(dir.path().to_owned(), &keys).await;
        assert_eq!(app_status(&state).await, AppStatus::Error(expected));
        assert_eq!(keys.creates.load(Ordering::SeqCst), 0);
        assert!(!dir.path().join("identity").exists());
        // Error startup releases the data-directory lock.
        Store::open(dir.path()).unwrap().close().await.unwrap();
    }
}
#[tokio::test]
async fn existing_workspace_identity_errors_only_load_without_creating_or_fallback() {
    let dir = profile();
    let state = AppState::open(dir.path().to_owned(), true).await;
    let original = ready(create_workspace(&state, create_input()).await);
    state.close().await.unwrap();
    let identity = std::fs::read(dir.path().join("identity")).unwrap();

    for (error, expected) in [
        (IdentityError::NotFound, AppError::IdentityMissing),
        (IdentityError::Locked, AppError::IdentityLocked),
        (IdentityError::Unavailable, AppError::IdentityUnavailable),
        (IdentityError::Invalid, AppError::IdentityInvalid),
        (IdentityError::Permissions, AppError::UnsafePermissions),
    ] {
        // A different selected backend cannot see this profile's existing file key.
        let keys = RefusingKeys {
            error,
            creates: AtomicUsize::new(0),
        };
        let state = AppState::open_with_keys(dir.path().to_owned(), &keys).await;
        assert_eq!(app_status(&state).await, AppStatus::Error(expected));
        assert_eq!(keys.creates.load(Ordering::SeqCst), 0);
        state.close().await.unwrap();
        assert_eq!(
            std::fs::read(dir.path().join("identity")).unwrap(),
            identity
        );
    }

    // Selecting the matching backend still opens the unchanged workspace and identity.
    let reopened = AppState::open(dir.path().to_owned(), true).await;
    assert_eq!(app_status(&reopened).await, AppStatus::Ready(original));
    reopened.close().await.unwrap();
}
#[tokio::test]
async fn removed_file_identity_on_existing_workspace_is_missing_without_replacement() {
    let dir = profile();
    let state = AppState::open(dir.path().to_owned(), true).await;
    let original = ready(create_workspace(&state, create_input()).await);
    state.close().await.unwrap();
    let path = dir.path().join("identity");
    let identity = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();

    let reopened = AppState::open(dir.path().to_owned(), true).await;
    assert_eq!(
        app_status(&reopened).await,
        AppStatus::Error(AppError::IdentityMissing)
    );
    assert!(!path.exists());
    reopened.close().await.unwrap();
    assert!(!path.exists());

    // Restoring the original identity permits reopening the same saved workspace.
    FileKeyStore::new(&path)
        .create(&SecretKey::from_bytes(
            identity.as_slice().try_into().unwrap(),
        ))
        .unwrap();
    let restored = AppState::open(dir.path().to_owned(), true).await;
    assert_eq!(app_status(&restored).await, AppStatus::Ready(original));
    restored.close().await.unwrap();
}
#[tokio::test]
async fn partial_create_is_durable_and_repairable_without_a_second_workspace() {
    let dir = profile();
    let state = AppState::open(dir.path().to_owned(), true).await;
    let sql = rusqlite::Connection::open(dir.path().join("meshyard.sqlite")).unwrap();
    sql.execute_batch("CREATE TRIGGER fail_profile BEFORE INSERT ON events BEGIN SELECT RAISE(ABORT, 'private failure text'); END;").unwrap();
    assert_eq!(
        create_workspace(&state, create_input()).await,
        AppStatus::Error(AppError::PartialCreate)
    );
    assert_eq!(
        create_workspace(&state, create_input()).await,
        AppStatus::Error(AppError::WorkspaceExists)
    );
    state.close().await.unwrap();
    let reopened = AppState::open(dir.path().to_owned(), true).await;
    assert_eq!(
        app_status(&reopened).await,
        AppStatus::Error(AppError::PartialCreate)
    );
    sql.execute_batch("DROP TRIGGER fail_profile;").unwrap();
    let view = ready(update_profile(&reopened, "Recovered".into()).await);
    assert_eq!(view.members[0].display_name, "Recovered");
    reopened.close().await.unwrap();
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(store.workspaces().await.unwrap().len(), 1);
    store.close().await.unwrap();
}
#[tokio::test]
async fn multiple_workspaces_fail_closed_and_missing_membership_is_rejected() {
    let dir = profile();
    let store = Store::open(dir.path()).unwrap();
    let keys = FileKeyStore::new(dir.path().join("identity"));
    let owner = SecretKey::from_bytes(&[12; 32]);
    keys.create(&SecretKey::from_bytes(&[13; 32])).unwrap();
    let genesis = sign_genesis(
        Genesis {
            owner: owner.device_id(),
            name: "test".into(),
            mode: Mode::Strict,
            created_ms: 0,
            nonce: [1; 16],
        },
        &owner,
    )
    .unwrap();
    store
        .add_workspace(postcard::to_stdvec(&genesis).unwrap())
        .await
        .unwrap();
    store.close().await.unwrap();
    let state = AppState::open(dir.path().to_owned(), true).await;
    assert_eq!(
        app_status(&state).await,
        AppStatus::Error(AppError::IdentityMismatch)
    );
    state.close().await.unwrap();
    let store = Store::open(dir.path()).unwrap();
    let genesis = sign_genesis(
        Genesis {
            owner: owner.device_id(),
            name: "other".into(),
            mode: Mode::Strict,
            created_ms: 0,
            nonce: [2; 16],
        },
        &owner,
    )
    .unwrap();
    store
        .add_workspace(postcard::to_stdvec(&genesis).unwrap())
        .await
        .unwrap();
    store.close().await.unwrap();
    let state = AppState::open(dir.path().to_owned(), true).await;
    assert_eq!(
        app_status(&state).await,
        AppStatus::Error(AppError::MultipleWorkspaces)
    );
    state.close().await.unwrap();
}
#[tokio::test]
async fn invalid_inputs_do_not_persist_and_protocol_sanitation_is_used() {
    let dir = profile();
    let state = AppState::open(dir.path().to_owned(), true).await;
    let mut input = create_input();
    input.mode = Mode::Auto;
    assert_eq!(
        create_workspace(&state, input).await,
        AppStatus::Error(AppError::InvalidInput)
    );
    let mut input = create_input();
    input.name = "\0\n".into();
    assert_eq!(
        create_workspace(&state, input).await,
        AppStatus::Error(AppError::InvalidInput)
    );
    assert_eq!(app_status(&state).await, AppStatus::Setup);
    create_workspace(&state, create_input()).await;
    assert_eq!(
        register_agent(
            &state,
            RegisterAgent {
                kind: AgentKind::Other,
                label: "x".into()
            }
        )
        .await,
        AppStatus::Error(AppError::InvalidInput)
    );
    let label = "é".repeat(40) + "\0";
    let view = ready(
        register_agent(
            &state,
            RegisterAgent {
                kind: AgentKind::Codex,
                label,
            },
        )
        .await,
    );
    assert_eq!(view.agents[0].label, "é".repeat(32));
    let sql = rusqlite::Connection::open(dir.path().join("meshyard.sqlite")).unwrap();
    sql.execute_batch("CREATE TRIGGER fail_register BEFORE INSERT ON events BEGIN SELECT RAISE(ABORT, 'private-path-and-os-error'); END;").unwrap();
    assert_eq!(
        register_agent(
            &state,
            RegisterAgent {
                kind: AgentKind::Codex,
                label: "failure".into()
            }
        )
        .await,
        AppStatus::Error(AppError::Storage)
    );
    assert_eq!(ready(app_status(&state).await).agents.len(), 1);
    state.close().await.unwrap();
}

#[test]
fn commands_persist_across_two_fresh_processes_with_explicit_usb_file_profile() {
    let dir = profile();
    let child = |phase: &str| {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "command_process_child", "--nocapture"])
            .env("MESHYARD_T014_TEST_PHASE", phase)
            .env("MESHYARD_DATA_DIR", dir.path())
            .env("MESHYARD_KEY_STORE", "file")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "command child failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .find(|line| line.starts_with("T014_VIEW:"))
            .expect("child must return its actual view")
            .to_owned()
    };
    let first = child("create");
    let restarted = child("reopen");
    assert_eq!(
        first, restarted,
        "public persisted view and workspace ID must survive process restart"
    );
}

#[test]
fn command_process_child() {
    let Ok(phase) = std::env::var("MESHYARD_T014_TEST_PHASE") else {
        return;
    };
    assert_eq!(std::env::var("MESHYARD_KEY_STORE").unwrap(), "file");
    let dir = std::path::PathBuf::from(std::env::var_os("MESHYARD_DATA_DIR").unwrap());
    assert!(dir.starts_with("/Volumes/TokensAbuse/"));
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let state = AppState::open(dir, true).await;
            if phase == "create" {
                ready(create_workspace(&state, create_input()).await);
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
            } else {
                assert_eq!(phase, "reopen");
            }
            let view = ready(app_status(&state).await);
            assert_eq!(view.name, "Demo");
            assert_eq!(view.members[0].display_name, "Dion");
            assert_eq!(view.agents.len(), 1);
            assert_eq!(view.agents[0].label, "main");
            assert_eq!(view.agents[0].state, None);
            assert_eq!(view.agents[0].provenance, None);
            println!("T014_VIEW:{view:?}");
            state.close().await.unwrap();
        });
}

#[tokio::test]
async fn actual_file_identity_and_data_directory_failures_are_typed_without_replacement() {
    let dir = profile();
    let path = dir.path().join("identity");
    std::fs::write(&path, b"invalid secret length").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let before = std::fs::read(&path).unwrap();
    let state = AppState::open(dir.path().to_owned(), true).await;
    assert_eq!(
        app_status(&state).await,
        AppStatus::Error(AppError::IdentityInvalid)
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    state.close().await.unwrap();

    let dir = profile();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o777)).unwrap();
    let state = AppState::open(dir.path().to_owned(), true).await;
    assert_eq!(
        app_status(&state).await,
        AppStatus::Error(AppError::UnsafePermissions)
    );
    assert!(!dir.path().join("identity").exists());
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    state.close().await.unwrap();

    let dir = profile();
    Store::open(dir.path()).unwrap().close().await.unwrap();
    let sql = rusqlite::Connection::open(dir.path().join("meshyard.sqlite")).unwrap();
    sql.pragma_update(None, "user_version", 999).unwrap();
    drop(sql);
    let state = AppState::open(dir.path().to_owned(), true).await;
    assert_eq!(
        app_status(&state).await,
        AppStatus::Error(AppError::Migration)
    );
    assert!(!dir.path().join("identity").exists());
    state.close().await.unwrap();
}
