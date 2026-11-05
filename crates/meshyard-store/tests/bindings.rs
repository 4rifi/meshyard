#![allow(clippy::unwrap_used, clippy::expect_used)]
use meshyard_core::*;
use meshyard_store::*;
use std::{os::unix::fs::PermissionsExt, sync::Arc};
fn private() -> tempfile::TempDir {
    let root = std::path::PathBuf::from(std::env::var_os("TMPDIR").expect("USB TMPDIR"));
    assert!(root.starts_with("/Volumes/TokensAbuse/"));
    tempfile::Builder::new()
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir_in(root)
        .unwrap()
}
async fn workspace(store: &Store, key: &dyn Signer, nonce: u8) -> WorkspaceId {
    store
        .add_workspace(
            postcard::to_stdvec(
                &sign_genesis(
                    Genesis {
                        owner: key.device_id(),
                        name: "Bindings".into(),
                        mode: Mode::Strict,
                        nonce: [nonce; 16],
                        created_ms: 0,
                    },
                    key,
                )
                .unwrap(),
            )
            .unwrap(),
        )
        .await
        .unwrap()
}
async fn register(store: &Store, w: WorkspaceId, key: Arc<dyn Signer>, id: u8, kind: AgentKind) {
    store
        .append_own(
            w,
            EventBody::AgentRegistered {
                agent_local_id: [id; 16],
                kind,
                label: "test".into(),
            },
            key,
        )
        .await
        .unwrap();
}
#[tokio::test]
async fn binding_is_explicit_own_kind_unique_local_and_persistent() {
    let dir = private();
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(
        store.directory(),
        std::fs::canonicalize(dir.path()).unwrap()
    );
    let own = Arc::new(SecretKey::from_bytes(&[15; 32]));
    let remote = Arc::new(SecretKey::from_bytes(&[16; 32]));
    let w = workspace(&store, own.as_ref(), 1).await;
    let other = workspace(&store, own.as_ref(), 2).await;
    assert_eq!(
        store
            .agent_binding(w, AgentKind::ClaudeCode, own.clone())
            .await
            .unwrap(),
        None
    );
    assert!(matches!(
        store.bind_own_agent(w, [99; 16], own.clone()).await,
        Err(StoreError::InvalidBinding)
    ));
    store
        .append_own(
            w,
            EventBody::MemberAdded {
                device: remote.device_id(),
                display_name: "peer".into(),
            },
            own.clone(),
        )
        .await
        .unwrap();
    register(&store, w, remote.clone(), 9, AgentKind::ClaudeCode).await;
    assert!(matches!(
        store.bind_own_agent(w, [9; 16], own.clone()).await,
        Err(StoreError::InvalidBinding)
    ));
    register(&store, w, own.clone(), 1, AgentKind::ClaudeCode).await;
    store
        .bind_first_own_agent(w, [1; 16], own.clone())
        .await
        .unwrap();
    register(&store, w, own.clone(), 2, AgentKind::ClaudeCode).await;
    store
        .bind_first_own_agent(w, [2; 16], own.clone())
        .await
        .unwrap();
    assert_eq!(
        store
            .agent_binding(w, AgentKind::ClaudeCode, own.clone())
            .await
            .unwrap(),
        Some([1; 16])
    );
    let before = store.contiguous_heads(w).await.unwrap();
    store.bind_own_agent(w, [2; 16], own.clone()).await.unwrap();
    assert_eq!(store.contiguous_heads(w).await.unwrap(), before); // no signed/synced binding event
    register(&store, w, own.clone(), 3, AgentKind::Codex).await;
    store
        .bind_first_own_agent(w, [3; 16], own.clone())
        .await
        .unwrap();
    assert_eq!(
        store
            .agent_binding(w, AgentKind::Codex, own.clone())
            .await
            .unwrap(),
        Some([3; 16])
    );
    assert!(matches!(
        store.bind_own_agent(other, [1; 16], own.clone()).await,
        Err(StoreError::InvalidBinding)
    ));
    store.close().await.unwrap();
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(
        store
            .agent_binding(w, AgentKind::ClaudeCode, own.clone())
            .await
            .unwrap(),
        Some([2; 16])
    );
    assert_eq!(
        store
            .agent_binding(other, AgentKind::ClaudeCode, own.clone())
            .await
            .unwrap(),
        None
    );
    // Even a forged local table entry cannot route an own event into a remote registration.
    let sql = rusqlite::Connection::open(dir.path().join("meshyard.sqlite")).unwrap();
    assert_eq!(
        sql.query_row(
            "SELECT count(*) FROM agent_bindings WHERE workspace=?1",
            [w.0.as_slice()],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        2
    );
    sql.execute(
        "UPDATE agent_bindings SET agent_local_id=?1 WHERE workspace=?2 AND kind=0",
        rusqlite::params![[9u8; 16].as_slice(), w.0.as_slice()],
    )
    .unwrap();
    assert_eq!(
        store
            .agent_binding(w, AgentKind::ClaudeCode, own.clone())
            .await
            .unwrap(),
        None
    );
    drop(sql);
    store.close().await.unwrap();
}
#[tokio::test]
async fn migration_adds_local_bindings_without_guessing_existing_agents() {
    let dir = private();
    let store = Store::open(dir.path()).unwrap();
    let own = Arc::new(SecretKey::from_bytes(&[17; 32]));
    let w = workspace(&store, own.as_ref(), 1).await;
    register(&store, w, own.clone(), 1, AgentKind::ClaudeCode).await;
    store.close().await.unwrap();
    // Reconstruct the previous schema version, retaining real signed workspace/registration.
    let sql = rusqlite::Connection::open(dir.path().join("meshyard.sqlite")).unwrap();
    sql.execute_batch("DROP TABLE join_attempts; ALTER TABLE pairings DROP COLUMN joiner_name; ALTER TABLE pairings DROP COLUMN attempt_window_ms; ALTER TABLE pairings DROP COLUMN attempts; DROP TABLE agent_bindings; DROP TRIGGER count_contiguous_insert; DROP TRIGGER count_contiguous_delete; DROP TABLE projection_counts; DROP TABLE agent_projection; DROP TABLE profile_projection; ALTER TABLE members DROP COLUMN added_name; DELETE FROM meta WHERE key LIKE 'projection_v1:%'; PRAGMA user_version=2;")
        .unwrap();
    drop(sql);
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(
        store
            .agent_binding(w, AgentKind::ClaudeCode, own.clone())
            .await
            .unwrap(),
        None
    );
    register(&store, w, own.clone(), 2, AgentKind::ClaudeCode).await;
    store
        .bind_first_own_agent(w, [2; 16], own.clone())
        .await
        .unwrap();
    assert_eq!(
        store
            .agent_binding(w, AgentKind::ClaudeCode, own.clone())
            .await
            .unwrap(),
        None
    );
    store.bind_own_agent(w, [1; 16], own.clone()).await.unwrap();
    assert_eq!(store.workspace_state(w).await.unwrap().1.agents.len(), 2);
    store.close().await.unwrap();
}
