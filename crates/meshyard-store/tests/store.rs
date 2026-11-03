#![allow(clippy::unwrap_used, clippy::expect_used)]
mod support;
use meshyard_core::{wire::v1::*, *};
use meshyard_store::*;
use std::sync::Arc;
fn key() -> Arc<SecretKey> {
    Arc::new(SecretKey::from_bytes(&[42; 32]))
}
fn profile(name: &str) -> EventBody {
    EventBody::ProfileUpdated {
        display_name: name.into(),
        avatar: 0,
    }
}
fn signed_genesis(key: &dyn Signer, nonce: u8) -> Vec<u8> {
    postcard::to_stdvec(
        &sign_genesis(
            Genesis {
                owner: key.device_id(),
                name: "test".into(),
                mode: Mode::Strict,
                created_ms: 0,
                nonce: [nonce; 16],
            },
            key,
        )
        .unwrap(),
    )
    .unwrap()
}
fn remote(workspace: WorkspaceId, seq: u64, body: EventBody, key: &dyn Signer) -> Vec<u8> {
    // Raw signed construction intentionally bypasses sender sanitation for adversarial tests.
    let mut signed = postcard::to_stdvec(&EventHeaderV1 {
        v: 1,
        workspace: workspace.0,
        author: key.device_id().0,
        seq,
        ts_ms: 0,
        membership_epoch: 0,
    })
    .unwrap();
    signed.extend(postcard::to_stdvec(&EventBodyV1::from(body)).unwrap());
    let sig = key.sign(&domain_message(EVENT_DOMAIN, &signed)).unwrap();
    postcard::to_stdvec(&SignedEventV1 { signed, sig }).unwrap()
}
#[tokio::test]
async fn persistence_reopen_workspace_isolation_and_no_secret_in_sql() {
    let dir = support::private_tempdir();
    let key = key();
    let store = Store::open(dir.path()).unwrap();
    let g = signed_genesis(key.as_ref(), 0);
    let w = store.add_workspace(g.clone()).await.unwrap();
    let other = store
        .add_workspace(signed_genesis(key.as_ref(), 1))
        .await
        .unwrap();
    assert_eq!(store.add_workspace(g.clone()).await.unwrap(), w);
    let first = store
        .append_own(w, profile("one"), key.clone())
        .await
        .unwrap();
    store
        .append_own(other, profile("two"), key.clone())
        .await
        .unwrap();
    assert_eq!(
        store.contiguous_heads(w).await.unwrap()[&key.device_id()],
        1
    );
    assert_eq!(
        store
            .events_after(w, key.device_id(), 0, 256)
            .await
            .unwrap(),
        vec![postcard::to_stdvec(&first).unwrap()]
    );
    assert_eq!(store.genesis(w).await.unwrap(), g);
    assert!(matches!(
        store.events_after(w, key.device_id(), 0, 0).await,
        Err(StoreError::Limit)
    ));
    assert!(matches!(
        store.events_after(w, key.device_id(), 0, 257).await,
        Err(StoreError::Limit)
    ));
    assert!(matches!(
        store
            .events_after(WorkspaceId([99; 16]), key.device_id(), 0, 1)
            .await,
        Err(StoreError::UnknownWorkspace)
    ));
    store.close().await.unwrap();
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(
        store.contiguous_heads(w).await.unwrap()[&key.device_id()],
        1
    );
    store
        .append_own(w, profile("three"), key.clone())
        .await
        .unwrap();
    assert_eq!(
        store.contiguous_heads(w).await.unwrap()[&key.device_id()],
        2
    );
    store.close().await.unwrap();
    let conn = rusqlite::Connection::open(dir.path().join("meshyard.sqlite")).unwrap();
    assert_eq!(
        conn.query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "wal"
    );
    let tables:i64=conn.query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('workspaces','events','pending_events','peers','pairings','meta')",[],|row|row.get(0)).unwrap();
    assert_eq!(tables, 6);
    drop(conn);
    for name in [
        "meshyard.sqlite",
        "meshyard.sqlite-wal",
        "meshyard.sqlite-shm",
    ] {
        if let Ok(bytes) = std::fs::read(dir.path().join(name)) {
            assert!(!bytes.windows(32).any(|slice| slice == [42; 32]));
        }
    }
}
#[tokio::test]
async fn pending_promotes_only_contiguous_prefix_and_preserves_exact_bytes() {
    let dir = support::private_tempdir();
    let store = Store::open(dir.path()).unwrap();
    let key = key();
    let w = store
        .add_workspace(signed_genesis(key.as_ref(), 0))
        .await
        .unwrap();
    let one = remote(w, 1, profile("one"), key.as_ref());
    let two = remote(w, 2, profile("two"), key.as_ref());
    let three = remote(w, 3, profile("three"), key.as_ref());
    assert_eq!(
        store.insert_remote(three.clone()).await.unwrap(),
        InsertOutcome::Pending
    );
    assert_eq!(
        store.contiguous_heads(w).await.unwrap()[&key.device_id()],
        0
    );
    assert!(
        store
            .events_after(w, key.device_id(), 0, 256)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        store.insert_remote(three.clone()).await.unwrap(),
        InsertOutcome::Duplicate
    );
    assert_eq!(
        store.insert_remote(one.clone()).await.unwrap(),
        InsertOutcome::Inserted
    );
    assert_eq!(
        store.contiguous_heads(w).await.unwrap()[&key.device_id()],
        1
    );
    store.close().await.unwrap();
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(
        store.insert_remote(two.clone()).await.unwrap(),
        InsertOutcome::Inserted
    );
    assert_eq!(
        store.contiguous_heads(w).await.unwrap()[&key.device_id()],
        3
    );
    assert_eq!(
        store
            .events_after(w, key.device_id(), 0, 256)
            .await
            .unwrap(),
        vec![one, two, three]
    );
    store.close().await.unwrap();
}
#[tokio::test]
async fn duplicate_and_equivocation_before_field_validation_keep_first_and_flag() {
    let dir = support::private_tempdir();
    let store = Store::open(dir.path()).unwrap();
    let key = key();
    let w = store
        .add_workspace(signed_genesis(key.as_ref(), 0))
        .await
        .unwrap();
    let one = remote(w, 1, profile("one"), key.as_ref());
    assert_eq!(
        store.insert_remote(one.clone()).await.unwrap(),
        InsertOutcome::Inserted
    );
    assert_eq!(
        store.insert_remote(one.clone()).await.unwrap(),
        InsertOutcome::Duplicate
    );
    assert_eq!(
        store
            .insert_remote(remote(w, 1, profile("bad\n"), key.as_ref()))
            .await
            .unwrap(),
        InsertOutcome::Equivocation
    );
    assert_eq!(
        store
            .events_after(w, key.device_id(), 0, 256)
            .await
            .unwrap(),
        vec![one.clone()]
    );
    assert!(
        store
            .diagnostics(w, key.device_id())
            .await
            .unwrap()
            .equivocated
    );
    let pending = remote(w, 3, profile("three"), key.as_ref());
    assert_eq!(
        store.insert_remote(pending).await.unwrap(),
        InsertOutcome::Pending
    );
    assert_eq!(
        store
            .insert_remote(remote(w, 3, profile("conflict"), key.as_ref()))
            .await
            .unwrap(),
        InsertOutcome::Equivocation
    );
    // Same semantic seq, different raw varint: equivocation, never re-encode for comparison.
    let mut alternate = decode_signed_event(&one).unwrap();
    alternate.signed.splice(49..50, [0x81, 0]);
    alternate.sig = key
        .sign(&domain_message(EVENT_DOMAIN, &alternate.signed))
        .unwrap();
    assert_eq!(
        store
            .insert_remote(postcard::to_stdvec(&alternate).unwrap())
            .await
            .unwrap(),
        InsertOutcome::Equivocation
    );
    store.close().await.unwrap();
}
#[tokio::test]
async fn raw_nonminimal_varints_are_stored_unchanged() {
    let dir = support::private_tempdir();
    let store = Store::open(dir.path()).unwrap();
    let key = key();
    let w = store
        .add_workspace(signed_genesis(key.as_ref(), 0))
        .await
        .unwrap();
    let mut alternate = decode_signed_event(&remote(w, 1, profile("raw"), key.as_ref())).unwrap();
    alternate.signed.splice(49..50, [0x81, 0]);
    alternate.sig = key
        .sign(&domain_message(EVENT_DOMAIN, &alternate.signed))
        .unwrap();
    let canonical_outer = postcard::to_stdvec(&alternate).unwrap();
    let mut raw = canonical_outer.clone();
    let prefix = raw[0];
    assert!(prefix < 128);
    raw.splice(0..1, [prefix | 128, 0]);
    assert_eq!(
        store.insert_remote(raw.clone()).await.unwrap(),
        InsertOutcome::Inserted
    );
    assert_eq!(
        store.events_after(w, key.device_id(), 0, 1).await.unwrap(),
        vec![raw]
    );
    assert_eq!(
        store.insert_remote(canonical_outer).await.unwrap(),
        InsertOutcome::Duplicate
    );
    store.close().await.unwrap();
}
#[tokio::test]
async fn invalid_strings_never_persist_and_leave_a_gap() {
    for case in 0..4 {
        let dir = support::private_tempdir();
        let store = Store::open(dir.path()).unwrap();
        let key = key();
        let w = store
            .add_workspace(signed_genesis(key.as_ref(), 0))
            .await
            .unwrap();
        let bad = match case {
            0 => remote(w, 1, profile("bad\n"), key.as_ref()),
            1 => remote(w, 1, profile(&"x".repeat(65)), key.as_ref()),
            2 => remote(w, 1, profile("bad\u{85}"), key.as_ref()),
            _ => {
                let mut signed =
                    decode_signed_event(&remote(w, 1, profile("x"), key.as_ref())).unwrap();
                let index = signed.signed.len() - 2;
                signed.signed[index] = 0xff;
                signed.sig = key
                    .sign(&domain_message(EVENT_DOMAIN, &signed.signed))
                    .unwrap();
                postcard::to_stdvec(&signed).unwrap()
            }
        };
        assert_eq!(
            store.insert_remote(bad).await.unwrap(),
            InsertOutcome::Rejected(RejectReason::InvalidField)
        );
        assert!(
            store
                .diagnostics(w, key.device_id())
                .await
                .unwrap()
                .protocol_violation
        );
        assert_eq!(
            store
                .insert_remote(remote(w, 2, profile("valid later"), key.as_ref()))
                .await
                .unwrap(),
            InsertOutcome::Pending
        );
        assert_eq!(
            store.contiguous_heads(w).await.unwrap()[&key.device_id()],
            0
        );
        assert!(
            store
                .events_after(w, key.device_id(), 0, 256)
                .await
                .unwrap()
                .is_empty()
        );
        store.close().await.unwrap();
        let conn = rusqlite::Connection::open(dir.path().join("meshyard.sqlite")).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM pending_events WHERE seq=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
    }
}
#[tokio::test]
async fn malformed_signature_unknown_workspace_and_numeric_bounds_are_rejected() {
    let dir = support::private_tempdir();
    let store = Store::open(dir.path()).unwrap();
    let key = key();
    let w = store
        .add_workspace(signed_genesis(key.as_ref(), 0))
        .await
        .unwrap();
    for seq in [0, u64::MAX, i64::MAX as u64 + 1] {
        assert_eq!(
            store
                .insert_remote(remote(w, seq, profile("bad"), key.as_ref()))
                .await
                .unwrap(),
            InsertOutcome::Rejected(RejectReason::SequenceRange)
        );
    }
    assert_eq!(
        store
            .insert_remote(remote(
                w,
                i64::MAX as u64,
                profile("pending boundary"),
                key.as_ref()
            ))
            .await
            .unwrap(),
        InsertOutcome::Pending
    );
    assert!(matches!(
        store.events_after(w, key.device_id(), u64::MAX, 1).await,
        Err(StoreError::SequenceRange)
    ));
    let mut raw = remote(w, 1, profile("signed"), key.as_ref());
    let end = raw.len() - 1;
    raw[end] ^= 1;
    assert_eq!(
        store.insert_remote(raw).await.unwrap(),
        InsertOutcome::Rejected(RejectReason::InvalidSignature)
    );
    let mut raw = remote(w, 1, profile("signed"), key.as_ref());
    raw.push(0);
    assert_eq!(
        store.insert_remote(raw).await.unwrap(),
        InsertOutcome::Rejected(RejectReason::Malformed)
    );
    assert_eq!(
        store.insert_remote(vec![0]).await.unwrap(),
        InsertOutcome::Rejected(RejectReason::Malformed)
    );
    assert_eq!(
        store
            .insert_remote(remote(
                WorkspaceId([99; 16]),
                1,
                profile("unknown"),
                key.as_ref()
            ))
            .await
            .unwrap(),
        InsertOutcome::Rejected(RejectReason::UnknownWorkspace)
    );
    store.close().await.unwrap();
}
#[tokio::test]
async fn atomic_own_sequence_sanitation_policy_and_concurrent_channel_calls() {
    let dir = support::private_tempdir();
    let store = Store::open(dir.path()).unwrap();
    let key = key();
    let w = store
        .add_workspace(signed_genesis(key.as_ref(), 0))
        .await
        .unwrap();
    let mut jobs = vec![];
    for _ in 0..32 {
        let store = store.clone();
        let key = key.clone();
        jobs.push(tokio::spawn(async move {
            store.append_own(w, profile("A\n😺"), key).await.unwrap()
        }));
    }
    let mut seqs = vec![];
    for job in jobs {
        let signed = job.await.unwrap();
        let e = verify_and_decode(&signed, &Ed25519Verifier).unwrap();
        assert!(e.validate_received().is_ok());
        seqs.push(e.seq);
    }
    seqs.sort();
    assert_eq!(seqs, (1..=32).collect::<Vec<_>>());
    let signed = store
        .append_own(
            w,
            EventBody::AgentState {
                agent_local_id: [0; 16],
                state: ActivityState::Testing,
                provenance: Provenance::Inferred {
                    rule: RuleId::ToolShell,
                },
                tool: Some(ToolCategory::Shell),
                task_label: Some("prompt command /path".into()),
            },
            key.clone(),
        )
        .await
        .unwrap();
    assert!(matches!(
        verify_and_decode(&signed, &Ed25519Verifier).unwrap().body,
        EventBody::AgentState {
            task_label: None,
            ..
        }
    ));
    assert_eq!(
        store.contiguous_heads(w).await.unwrap()[&key.device_id()],
        33
    );
    store.close().await.unwrap();
}
#[tokio::test]
async fn pending_overflow_drops_only_that_author_buffer_and_can_rerequest() {
    let dir = support::private_tempdir();
    let store = Store::open(dir.path()).unwrap();
    let key = key();
    let other = SecretKey::from_bytes(&[43; 32]);
    let w = store
        .add_workspace(signed_genesis(key.as_ref(), 0))
        .await
        .unwrap();
    assert_eq!(
        store
            .insert_remote(remote(w, 2, profile("other"), &other))
            .await
            .unwrap(),
        InsertOutcome::Pending
    );
    for seq in 2..=1025 {
        assert_eq!(
            store
                .insert_remote(remote(w, seq, profile("pending"), key.as_ref()))
                .await
                .unwrap(),
            InsertOutcome::Pending
        );
    }
    assert_eq!(
        store
            .insert_remote(remote(w, 1026, profile("overflow"), key.as_ref()))
            .await
            .unwrap(),
        InsertOutcome::Rejected(RejectReason::PendingOverflow)
    );
    assert!(
        !store
            .contiguous_heads(w)
            .await
            .unwrap()
            .contains_key(&key.device_id())
    );
    assert_eq!(
        store.contiguous_heads(w).await.unwrap()[&other.device_id()],
        0
    );
    assert_eq!(
        store
            .insert_remote(remote(w, 1, profile("restart"), key.as_ref()))
            .await
            .unwrap(),
        InsertOutcome::Inserted
    );
    assert_eq!(
        store.contiguous_heads(w).await.unwrap()[&key.device_id()],
        1
    );
    store.close().await.unwrap();
}
#[tokio::test]
async fn exclusive_lock_released_on_close_and_checked_in_separate_process() {
    let dir = support::private_tempdir();
    let store = Store::open(dir.path()).unwrap();
    assert!(matches!(Store::open(dir.path()), Err(StoreError::Busy)));
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "lock_child_probe", "--ignored"])
        .env("MESHYARD_LOCK_TEST", dir.path())
        .status()
        .unwrap();
    assert!(status.success());
    store.close().await.unwrap();
    Store::open(dir.path()).unwrap().close().await.unwrap();
}
#[test]
#[ignore = "subprocess helper, invoked by exclusive lock test"]
fn lock_child_probe() {
    let path = std::env::var_os("MESHYARD_LOCK_TEST").unwrap();
    assert!(matches!(
        Store::open(std::path::Path::new(&path)),
        Err(StoreError::Busy)
    ));
}

#[tokio::test]
async fn own_append_records_observed_membership_epoch_and_explicit_task_opt_in() {
    let dir = support::private_tempdir();
    let store = Store::open_async(dir.path().to_owned()).await.unwrap();
    let key = key();
    let w = store
        .add_workspace(signed_genesis(key.as_ref(), 0))
        .await
        .unwrap();
    store
        .append_own(
            w,
            EventBody::MemberAdded {
                device: DeviceId([8; 32]),
                display_name: "Member".into(),
            },
            key.clone(),
        )
        .await
        .unwrap();
    let signed = store
        .append_own(w, profile("owner"), key.clone())
        .await
        .unwrap();
    let event = verify_and_decode(&signed, &Ed25519Verifier).unwrap();
    assert_eq!(event.membership_epoch, 1);
    assert!(event.ts_ms > 0);
    let signed = store
        .append_own_with(
            w,
            EventBody::AgentState {
                agent_local_id: [0; 16],
                state: ActivityState::Thinking,
                provenance: Provenance::Verified {
                    source: HookEvent::UserPromptSubmit,
                },
                tool: None,
                task_label: Some("User task\n😺".repeat(20)),
            },
            key.clone(),
            SharingPolicy {
                allow_task_label: true,
            },
            123,
            1,
        )
        .await
        .unwrap();
    let event = verify_and_decode(&signed, &Ed25519Verifier).unwrap();
    assert_eq!(event.ts_ms, 123);
    assert_eq!(event.membership_epoch, 1);
    assert!(event.validate_received().is_ok());
    assert!(
        matches!(&event.body,EventBody::AgentState { task_label:Some(value),..} if value.len() <= 80)
    );
    store.close().await.unwrap();
}

#[tokio::test]
async fn version_field_is_validated_after_equivocation_and_never_stored() {
    let dir = support::private_tempdir();
    let store = Store::open(dir.path()).unwrap();
    let key = key();
    let w = store
        .add_workspace(signed_genesis(key.as_ref(), 0))
        .await
        .unwrap();
    let raw = remote(w, 1, profile("first"), key.as_ref());
    assert_eq!(
        store.insert_remote(raw.clone()).await.unwrap(),
        InsertOutcome::Inserted
    );
    let mut other = decode_signed_event(&raw).unwrap();
    other.signed[0] = 2;
    other.sig = key
        .sign(&domain_message(EVENT_DOMAIN, &other.signed))
        .unwrap();
    assert_eq!(
        store
            .insert_remote(postcard::to_stdvec(&other).unwrap())
            .await
            .unwrap(),
        InsertOutcome::Equivocation
    );
    let mut next =
        decode_signed_event(&remote(w, 2, profile("bad version"), key.as_ref())).unwrap();
    next.signed[0] = 2;
    next.sig = key
        .sign(&domain_message(EVENT_DOMAIN, &next.signed))
        .unwrap();
    assert_eq!(
        store
            .insert_remote(postcard::to_stdvec(&next).unwrap())
            .await
            .unwrap(),
        InsertOutcome::Rejected(RejectReason::InvalidField)
    );
    assert_eq!(
        store.contiguous_heads(w).await.unwrap()[&key.device_id()],
        1
    );
    store.close().await.unwrap();
}

fn held_sql(dir: &std::path::Path, w: WorkspaceId, author: DeviceId) -> (i64, i64) {
    let conn = rusqlite::Connection::open(dir.join("meshyard.sqlite")).unwrap();
    conn.query_row(
        "SELECT COUNT(*),COALESCE(SUM(length(signed_blob)),0) FROM (SELECT signed_blob FROM events WHERE workspace=?1 AND author=?2 UNION ALL SELECT signed_blob FROM pending_events WHERE workspace=?1 AND author=?2)",
        rusqlite::params![w.0.as_slice(), author.0.as_slice()],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).unwrap()
}
fn member(device: DeviceId) -> EventBody {
    EventBody::MemberAdded {
        device,
        display_name: "Member".into(),
    }
}
async fn project(store: &Store, w: WorkspaceId, authors: &[DeviceId]) -> DerivedState {
    let raw = store.genesis(w).await.unwrap();
    let signed: SignedGenesisV1 = postcard::from_bytes(&raw).unwrap();
    let (_, genesis) = verify_genesis(&signed, &Ed25519Verifier).unwrap();
    let mut events = vec![];
    for author in authors {
        let mut cursor = 0;
        loop {
            let batch = store.events_after(w, *author, cursor, 256).await.unwrap();
            if batch.is_empty() {
                break;
            }
            for raw in batch {
                let event =
                    verify_and_decode(&decode_signed_event(&raw).unwrap(), &Ed25519Verifier)
                        .unwrap()
                        .into_event();
                cursor = event.seq;
                events.push(event);
            }
        }
    }
    fold(&genesis, &events).unwrap()
}
#[tokio::test]
async fn unknown_contiguous_quota_survives_reopen_resets_head_and_allows_resend() {
    let dir = support::private_tempdir();
    let owner = key();
    let author = SecretKey::from_bytes(&[61; 32]);
    let store = Store::open(dir.path()).unwrap();
    let w = store
        .add_workspace(signed_genesis(owner.as_ref(), 0))
        .await
        .unwrap();
    let other = store
        .add_workspace(signed_genesis(owner.as_ref(), 1))
        .await
        .unwrap();
    store
        .insert_remote(remote(other, 1, profile("isolated"), &author))
        .await
        .unwrap();
    for seq in 1..=1024 {
        assert_eq!(
            store
                .insert_remote(remote(w, seq, profile("held"), &author))
                .await
                .unwrap(),
            InsertOutcome::Inserted
        );
    }
    assert_eq!(held_sql(dir.path(), w, author.device_id()).0, 1024);
    assert_eq!(
        store.contiguous_heads(w).await.unwrap()[&author.device_id()],
        1024
    );
    store.close().await.unwrap();
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(
        store
            .insert_remote(remote(w, 1024, profile("held"), &author))
            .await
            .unwrap(),
        InsertOutcome::Duplicate
    );
    assert_eq!(
        store
            .insert_remote(remote(w, 1024, profile("bad\n"), &author))
            .await
            .unwrap(),
        InsertOutcome::Equivocation
    );
    assert_eq!(
        store
            .insert_remote(remote(w, 1025, profile("overflow"), &author))
            .await
            .unwrap(),
        InsertOutcome::Rejected(RejectReason::PendingOverflow)
    );
    assert_eq!(held_sql(dir.path(), w, author.device_id()), (0, 0));
    assert!(
        !store
            .contiguous_heads(w)
            .await
            .unwrap()
            .contains_key(&author.device_id())
    );
    assert_eq!(held_sql(dir.path(), other, author.device_id()).0, 1);
    // The sender can see zero after reopen and re-request its dropped prefix.
    store.close().await.unwrap();
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(
        store
            .insert_remote(remote(w, 2, profile("two"), &author))
            .await
            .unwrap(),
        InsertOutcome::Pending
    );
    store
        .append_own(w, member(author.device_id()), owner.clone())
        .await
        .unwrap();
    assert_eq!(
        store
            .insert_remote(remote(w, 1, profile("one"), &author))
            .await
            .unwrap(),
        InsertOutcome::Inserted
    );
    assert_eq!(
        store.contiguous_heads(w).await.unwrap()[&author.device_id()],
        2
    );
    let state = project(&store, w, &[owner.device_id(), author.device_id()]).await;
    assert_eq!(state.members[&author.device_id()].display_name, "two");
    assert!(state.pending_membership.is_empty());
    assert_eq!(held_sql(dir.path(), w, author.device_id()).0, 2);
    store.close().await.unwrap();
}
#[tokio::test]
async fn quota_combines_gapped_and_contiguous_rows_and_only_contiguous_owner_can_confirm() {
    let dir = support::private_tempdir();
    let owner = key();
    let author = SecretKey::from_bytes(&[62; 32]);
    let forger = SecretKey::from_bytes(&[63; 32]);
    let store = Store::open(dir.path()).unwrap();
    let w = store
        .add_workspace(signed_genesis(owner.as_ref(), 0))
        .await
        .unwrap();
    // Neither a peer's forged grant nor an owner event behind a gap is authority.
    store
        .insert_remote(remote(w, 1, member(author.device_id()), &forger))
        .await
        .unwrap();
    assert_eq!(
        store
            .insert_remote(remote(w, 2, member(author.device_id()), owner.as_ref()))
            .await
            .unwrap(),
        InsertOutcome::Pending
    );
    for seq in 1..=512 {
        store
            .insert_remote(remote(w, seq, profile("held"), &author))
            .await
            .unwrap();
    }
    for seq in 514..=1025 {
        assert_eq!(
            store
                .insert_remote(remote(w, seq, profile("held gap"), &author))
                .await
                .unwrap(),
            InsertOutcome::Pending
        );
    }
    assert_eq!(held_sql(dir.path(), w, author.device_id()).0, 1024);
    assert_eq!(
        store
            .insert_remote(remote(w, 513, profile("fills gap but over quota"), &author))
            .await
            .unwrap(),
        InsertOutcome::Rejected(RejectReason::PendingOverflow)
    );
    assert_eq!(held_sql(dir.path(), w, author.device_id()), (0, 0));
    assert!(
        !store
            .contiguous_heads(w)
            .await
            .unwrap()
            .contains_key(&author.device_id())
    );
    // Hold new permissible events, then activate the owner's contiguous grant.
    let one = remote(w, 1, profile("before grant"), &author);
    let three = remote(w, 3, profile("after gap"), &author);
    store.insert_remote(one.clone()).await.unwrap();
    store.insert_remote(three.clone()).await.unwrap();
    let before = project(
        &store,
        w,
        &[owner.device_id(), forger.device_id(), author.device_id()],
    )
    .await;
    assert!(!before.members.contains_key(&author.device_id()));
    store
        .insert_remote(remote(w, 1, profile("owner"), owner.as_ref()))
        .await
        .unwrap();
    let after = project(
        &store,
        w,
        &[owner.device_id(), forger.device_id(), author.device_id()],
    )
    .await;
    assert_eq!(
        after.members[&author.device_id()].display_name,
        "before grant"
    );
    store.close().await.unwrap();
    let store = Store::open(dir.path()).unwrap();
    let two = remote(w, 2, profile("gap filled"), &author);
    store.insert_remote(two.clone()).await.unwrap();
    assert_eq!(
        store
            .events_after(w, author.device_id(), 0, 256)
            .await
            .unwrap(),
        vec![one.clone(), two, three]
    );
    assert_eq!(
        store.insert_remote(one).await.unwrap(),
        InsertOutcome::Duplicate
    );
    assert_eq!(
        project(&store, w, &[owner.device_id(), author.device_id()])
            .await
            .members[&author.device_id()]
            .display_name,
        "after gap"
    );
    // Confirmed history is exempt from pending-membership quotas, even after the
    // reserved MemberRemoved body. Gap overflow may discard only the gap buffer.
    store
        .append_own(
            w,
            EventBody::MemberRemoved {
                device: author.device_id(),
            },
            owner.clone(),
        )
        .await
        .unwrap();
    for seq in 4..=1026 {
        assert_eq!(
            store
                .insert_remote(remote(w, seq, profile("confirmed history"), &author))
                .await
                .unwrap(),
            InsertOutcome::Inserted
        );
    }
    for seq in 1028..=2051 {
        assert_eq!(
            store
                .insert_remote(remote(w, seq, profile("gap"), &author))
                .await
                .unwrap(),
            InsertOutcome::Pending
        );
    }
    assert_eq!(
        store
            .insert_remote(remote(w, 2052, profile("gap overflow"), &author))
            .await
            .unwrap(),
        InsertOutcome::Rejected(RejectReason::PendingOverflow)
    );
    assert_eq!(held_sql(dir.path(), w, author.device_id()).0, 1026);
    assert_eq!(
        store.contiguous_heads(w).await.unwrap()[&author.device_id()],
        1026
    );
    store.close().await.unwrap();
}

#[tokio::test]
async fn promotion_sql_failure_rolls_back_entire_receive_transaction() {
    let dir = support::private_tempdir();
    let owner = key();
    let store = Store::open(dir.path()).unwrap();
    let w = store
        .add_workspace(signed_genesis(owner.as_ref(), 0))
        .await
        .unwrap();
    store
        .insert_remote(remote(w, 2, profile("two"), owner.as_ref()))
        .await
        .unwrap();
    let conn = rusqlite::Connection::open(dir.path().join("meshyard.sqlite")).unwrap();
    conn.execute_batch("CREATE TRIGGER fail_promotion BEFORE INSERT ON events WHEN NEW.seq=2 BEGIN SELECT RAISE(ABORT, 'test'); END;").unwrap();
    assert!(matches!(
        store
            .insert_remote(remote(w, 1, profile("one"), owner.as_ref()))
            .await,
        Err(StoreError::Database)
    ));
    assert_eq!(
        store.contiguous_heads(w).await.unwrap()[&owner.device_id()],
        0
    );
    assert_eq!(held_sql(dir.path(), w, owner.device_id()).0, 1);
    conn.execute_batch("DROP TRIGGER fail_promotion").unwrap();
    store
        .insert_remote(remote(w, 1, profile("one"), owner.as_ref()))
        .await
        .unwrap();
    assert_eq!(
        store.contiguous_heads(w).await.unwrap()[&owner.device_id()],
        2
    );
    store.close().await.unwrap();
}

#[tokio::test]
async fn byte_quota_counts_original_sql_blob_lengths_across_both_tables() {
    let dir = support::private_tempdir();
    let owner = key();
    let author = SecretKey::from_bytes(&[64; 32]);
    let store = Store::open(dir.path()).unwrap();
    let w = store
        .add_workspace(signed_genesis(owner.as_ref(), 0))
        .await
        .unwrap();
    store
        .insert_remote(remote(w, 1, profile("one"), &author))
        .await
        .unwrap();
    let three = remote(w, 3, profile("three"), &author);
    let conn = rusqlite::Connection::open(dir.path().join("meshyard.sqlite")).unwrap();
    // Synthetic SQL size fixture, not a valid signed event or an exploit: valid
    // current-v1 fields hit the count limit before 1 MiB. Exercise the byte guard
    // independently of that incidental wire-size restriction.
    conn.execute(
        "UPDATE events SET signed_blob=zeroblob(?1) WHERE workspace=?2 AND author=?3",
        rusqlite::params![
            i64::try_from(PENDING_BYTES - three.len()).unwrap(),
            w.0.as_slice(),
            author.device_id().0.as_slice()
        ],
    )
    .unwrap();
    assert_eq!(
        store.insert_remote(three).await.unwrap(),
        InsertOutcome::Pending
    );
    assert_eq!(
        held_sql(dir.path(), w, author.device_id()),
        (2, PENDING_BYTES as i64)
    );
    assert_eq!(
        store
            .insert_remote(remote(w, 2, profile("overflow"), &author))
            .await
            .unwrap(),
        InsertOutcome::Rejected(RejectReason::PendingOverflow)
    );
    assert_eq!(held_sql(dir.path(), w, author.device_id()), (0, 0));
    assert!(
        !store
            .contiguous_heads(w)
            .await
            .unwrap()
            .contains_key(&author.device_id())
    );
    store.close().await.unwrap();
}

#[tokio::test]
async fn reopen_cleans_old_oversized_unknown_logs_but_preserves_confirmed_history() {
    let dir = support::private_tempdir();
    let owner = key();
    let unknown = SecretKey::from_bytes(&[65; 32]);
    let confirmed = SecretKey::from_bytes(&[66; 32]);
    let store = Store::open(dir.path()).unwrap();
    let w = store
        .add_workspace(signed_genesis(owner.as_ref(), 0))
        .await
        .unwrap();
    store
        .append_own(w, member(confirmed.device_id()), owner.clone())
        .await
        .unwrap();
    store.close().await.unwrap();
    // Reproduce the pre-fix durable shape using correctly signed rows. Bypass the
    // fixed receive API only to build an old database for startup repair coverage.
    let mut conn = rusqlite::Connection::open(dir.path().join("meshyard.sqlite")).unwrap();
    let tx = conn.transaction().unwrap();
    for author in [&unknown, &confirmed] {
        for seq in 1..=1025 {
            tx.execute(
                "INSERT INTO events VALUES(?1,?2,?3,?4)",
                rusqlite::params![
                    w.0.as_slice(),
                    author.device_id().0.as_slice(),
                    i64::try_from(seq).unwrap(),
                    remote(w, seq, profile("legacy"), author)
                ],
            )
            .unwrap();
        }
    }
    tx.commit().unwrap();
    drop(conn);
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(held_sql(dir.path(), w, unknown.device_id()), (0, 0));
    assert_eq!(held_sql(dir.path(), w, confirmed.device_id()).0, 1025);
    let heads = store.contiguous_heads(w).await.unwrap();
    assert!(!heads.contains_key(&unknown.device_id()));
    assert_eq!(heads[&confirmed.device_id()], 1025);
    assert_eq!(heads[&owner.device_id()], 1);
    store.close().await.unwrap();
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(held_sql(dir.path(), w, confirmed.device_id()).0, 1025);
    store.close().await.unwrap();
}
