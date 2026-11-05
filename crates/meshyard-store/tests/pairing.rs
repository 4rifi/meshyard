#![allow(clippy::unwrap_used)]
mod support;
use meshyard_core::*;
use meshyard_store::{Store, StoreError, pairing::Reservation};
use std::sync::Arc;

const NOW: u64 = 1_000_000;
async fn fixture() -> (tempfile::TempDir, Store, Arc<SecretKey>, WorkspaceId) {
    let dir = support::private_tempdir();
    let store = Store::open(dir.path()).unwrap();
    let key = Arc::new(SecretKey::from_bytes(&[71; 32]));
    let genesis = sign_genesis(
        Genesis {
            owner: key.device_id(),
            name: "Pairing".into(),
            mode: Mode::Strict,
            created_ms: NOW,
            nonce: [1; 16],
        },
        key.as_ref(),
    )
    .unwrap();
    let workspace = store
        .add_workspace(postcard::to_stdvec(&genesis).unwrap())
        .await
        .unwrap();
    (dir, store, key, workspace)
}
fn reservation(workspace: WorkspaceId) -> Reservation {
    Reservation {
        workspace,
        invite_id: [1; 16],
        token_hash: [2; 32],
        joiner: SecretKey::from_bytes(&[72; 32]).device_id(),
        joiner_nonce: [3; 16],
        inviter_nonce: [4; 16],
        display_name: "Joiner".into(),
        now_ms: NOW,
    }
}
#[tokio::test]
async fn owner_only_single_use_resumes_and_requires_both_confirmations_after_reopen() {
    let (dir, store, key, w) = fixture().await;
    let stranger = Arc::new(SecretKey::from_bytes(&[73; 32]));
    assert!(matches!(
        store
            .create_invite(w, [1; 16], [2; 32], NOW, stranger)
            .await,
        Err(StoreError::Pairing)
    ));
    store
        .create_invite(w, [1; 16], [2; 32], NOW, key.clone())
        .await
        .unwrap();
    let first = store.reserve_invite(reservation(w)).await.unwrap();
    let mut replay = reservation(w);
    replay.inviter_nonce = [99; 16];
    assert_eq!(store.reserve_invite(replay).await.unwrap(), first);
    let mut stolen = reservation(w);
    stolen.joiner = DeviceId([8; 32]);
    assert!(matches!(
        store.reserve_invite(stolen).await,
        Err(StoreError::Pairing)
    ));
    let peer = first.joiner.unwrap();
    let hash = first.transcript_hash.unwrap();
    assert_eq!(store.workspace_snapshot(w).await.unwrap().members.len(), 1);
    let one = store
        .confirm_pairing(w, [1; 16], peer, hash, false, NOW, key.clone())
        .await
        .unwrap();
    assert_eq!(one.state, "Reserved");
    assert_eq!(store.workspace_snapshot(w).await.unwrap().members.len(), 1);
    store.close().await.unwrap();
    let store = Store::open(dir.path()).unwrap();
    assert!(store.pairings(w, NOW).await.unwrap()[0].joiner_confirmed);
    let complete = store
        .confirm_pairing(w, [1; 16], peer, hash, true, NOW, key.clone())
        .await
        .unwrap();
    assert_eq!(complete.state, "Completed");
    let snapshot = store.workspace_snapshot(w).await.unwrap();
    assert_eq!(snapshot.members.len(), 2);
    assert_eq!(snapshot.epoch, 1);
    assert!(
        snapshot
            .members
            .iter()
            .any(|m| m.device == peer && m.display_name == "Joiner")
    );
    assert_eq!(
        store.contiguous_heads(w).await.unwrap()[&key.device_id()],
        1
    );
    assert!(matches!(
        store.reserve_invite(reservation(w)).await,
        Err(StoreError::Pairing)
    ));
    store.close().await.unwrap();
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(store.pairings(w, NOW).await.unwrap()[0].state, "Completed");
    assert_eq!(store.workspace_state(w).await.unwrap().1.members.len(), 2);
    store.close().await.unwrap();
}
#[tokio::test]
async fn wrong_tokens_are_rate_limited_and_expiry_and_mismatch_consume_invites() {
    let (_dir, store, key, w) = fixture().await;
    store
        .create_invite(w, [1; 16], [2; 32], NOW, key.clone())
        .await
        .unwrap();
    for _ in 0..10 {
        let mut request = reservation(w);
        request.token_hash = [0; 32];
        assert!(matches!(
            store.reserve_invite(request).await,
            Err(StoreError::Pairing)
        ));
    }
    assert!(matches!(
        store.reserve_invite(reservation(w)).await,
        Err(StoreError::PairingRate)
    ));
    let mut request = reservation(w);
    request.now_ms += 60_000;
    let reserved = store.reserve_invite(request).await.unwrap();
    assert!(matches!(
        store
            .confirm_pairing(
                w,
                [1; 16],
                reserved.joiner.unwrap(),
                [0; 32],
                true,
                NOW + 60_000,
                key.clone()
            )
            .await,
        Err(StoreError::Pairing)
    ));
    assert_eq!(
        store.pairings(w, NOW + 60_000).await.unwrap()[0].state,
        "Aborted"
    );
    assert_eq!(store.workspace_snapshot(w).await.unwrap().members.len(), 1);
    store
        .create_invite(w, [5; 16], [2; 32], NOW, key.clone())
        .await
        .unwrap();
    let mut expired = reservation(w);
    expired.invite_id = [5; 16];
    expired.now_ms = NOW + 900_000;
    assert!(matches!(
        store.reserve_invite(expired).await,
        Err(StoreError::Pairing)
    ));
    assert_eq!(
        store.pairings(w, NOW + 900_000).await.unwrap()[0].state,
        "Expired"
    );
    store
        .create_invite(w, [6; 16], [2; 32], NOW + 900_000, key)
        .await
        .unwrap();
    let mut request = reservation(w);
    request.invite_id = [6; 16];
    request.now_ms += 900_000;
    store.reserve_invite(request).await.unwrap();
    assert_eq!(
        store.pairings(w, NOW + 1_200_000).await.unwrap()[0].state,
        "Aborted"
    );
    store.close().await.unwrap();
}
#[tokio::test]
async fn failed_membership_append_rolls_back_completion_and_confirmation_together() {
    let (dir, store, key, w) = fixture().await;
    store
        .create_invite(w, [1; 16], [2; 32], NOW, key.clone())
        .await
        .unwrap();
    let p = store.reserve_invite(reservation(w)).await.unwrap();
    let peer = p.joiner.unwrap();
    let hash = p.transcript_hash.unwrap();
    store
        .confirm_pairing(w, [1; 16], peer, hash, true, NOW, key.clone())
        .await
        .unwrap();
    let db = rusqlite::Connection::open(dir.path().join("meshyard.sqlite")).unwrap();
    db.execute_batch("CREATE TRIGGER reject_pair_member BEFORE INSERT ON events BEGIN SELECT RAISE(ABORT,'test rollback'); END;").unwrap();
    assert!(
        store
            .confirm_pairing(w, [1; 16], peer, hash, false, NOW, key.clone())
            .await
            .is_err()
    );
    let p = store.pairings(w, NOW).await.unwrap().remove(0);
    assert_eq!(p.state, "Reserved");
    assert!(p.inviter_confirmed);
    assert!(!p.joiner_confirmed);
    assert_eq!(store.workspace_snapshot(w).await.unwrap().members.len(), 1);
    assert!(store.contiguous_heads(w).await.unwrap().is_empty());
    db.execute_batch("DROP TRIGGER reject_pair_member;")
        .unwrap();
    drop(db);
    assert_eq!(
        store
            .confirm_pairing(w, [1; 16], peer, hash, false, NOW, key)
            .await
            .unwrap()
            .state,
        "Completed"
    );
    store.close().await.unwrap();
}

#[tokio::test]
async fn nonce_mismatch_aborts_and_old_open_invite_survives_terminal_history() {
    let (_dir, store, key, w) = fixture().await;
    store
        .create_invite(w, [1; 16], [2; 32], NOW, key.clone())
        .await
        .unwrap();
    for n in 2..70u8 {
        store
            .create_invite(w, [n; 16], [2; 32], NOW, key.clone())
            .await
            .unwrap();
        store.abort_pairing(w, [n; 16]).await.unwrap();
    }
    store.reserve_invite(reservation(w)).await.unwrap();
    let mut bad = reservation(w);
    bad.joiner_nonce = [88; 16];
    assert!(matches!(
        store.reserve_invite(bad).await,
        Err(StoreError::Pairing)
    ));
    assert!(matches!(
        store.reserve_invite(reservation(w)).await,
        Err(StoreError::Pairing)
    ));
    assert_eq!(store.workspace_snapshot(w).await.unwrap().members.len(), 1);
    store.close().await.unwrap();
}
