#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use std::{cell::Cell, rc::Rc};

struct CountingVerifier(Rc<Cell<usize>>);
impl Verifier for CountingVerifier {
    fn verify(
        &self,
        author: DeviceId,
        message: &[u8],
        signature: &[u8; 64],
    ) -> Result<(), CryptoError> {
        self.0.set(self.0.get() + 1);
        Ed25519Verifier.verify(author, message, signature)
    }
}
fn directory() -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;
    tempfile::Builder::new()
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap()
}
fn open(path: &Path, count: &Rc<Cell<usize>>) -> Database {
    Database::open_with_verifier(path, Box::new(CountingVerifier(count.clone()))).unwrap()
}
fn genesis(owner: &SecretKey) -> Vec<u8> {
    postcard::to_stdvec(
        &sign_genesis(
            Genesis {
                owner: owner.device_id(),
                name: "test".into(),
                mode: Mode::Strict,
                created_ms: 0,
                nonce: [0; 16],
            },
            owner,
        )
        .unwrap(),
    )
    .unwrap()
}
fn profile() -> EventBody {
    EventBody::ProfileUpdated {
        display_name: "test".into(),
        avatar: 0,
    }
}
fn grant(device: DeviceId) -> EventBody {
    EventBody::MemberAdded {
        device,
        display_name: "member".into(),
    }
}
fn removal(device: DeviceId) -> EventBody {
    EventBody::MemberRemoved { device }
}
fn raw(w: WorkspaceId, key: &SecretKey, seq: u64, body: EventBody) -> Vec<u8> {
    postcard::to_stdvec(
        &sign_event(
            Event {
                workspace: w,
                author: key.device_id(),
                seq,
                ts_ms: 0,
                membership_epoch: 0,
                body,
            },
            key,
            &SharingPolicy::default(),
        )
        .unwrap(),
    )
    .unwrap()
}
fn append(db: &mut Database, w: WorkspaceId, key: &SecretKey, body: EventBody) -> SignedEventV1 {
    // Include the production handle's observed-epoch lookup in signature-work accounting.
    let epoch = db.current_epoch(w).unwrap();
    db.append_own(w, body, key, SharingPolicy::default(), 0, epoch)
        .unwrap()
}
fn member_row(db: &Database, w: WorkspaceId, device: DeviceId) -> Option<(u32, bool)> {
    db.conn
        .query_row(
            "SELECT epoch_added,removed FROM members WHERE workspace=?1 AND device=?2",
            params![w.0.as_slice(), device.0.as_slice()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .unwrap()
}
#[test]
fn signature_work_is_constant_after_5000_actual_owner_events_and_reopen() {
    let dir = directory();
    let path = dir.path().join("meshyard.sqlite");
    let count = Rc::new(Cell::new(0));
    let owner = SecretKey::from_bytes(&[71; 32]);
    let member = SecretKey::from_bytes(&[72; 32]);
    let unknown = SecretKey::from_bytes(&[73; 32]);
    let mut db = open(&path, &count);
    let w = db.add_workspace(&genesis(&owner)).unwrap();
    assert_eq!(member_row(&db, w, owner.device_id()), Some((0, false)));
    for seq in 1..=5000 {
        let body = if seq == 1 {
            grant(member.device_id())
        } else {
            EventBody::AgentState {
                agent_local_id: [0; 16],
                state: ActivityState::Testing,
                provenance: Provenance::Verified {
                    source: HookEvent::PreToolUse,
                },
                tool: Some(ToolCategory::Shell),
                task_label: None,
            }
        };
        assert_eq!(
            db.insert_remote(&raw(w, &owner, seq, body)).unwrap(),
            InsertOutcome::Inserted
        );
    }
    assert_eq!(db.head(w, owner.device_id()).unwrap(), 5000);
    for seq in 1..=3 {
        for author in [&member, &unknown] {
            count.set(0);
            assert_eq!(
                db.insert_remote(&raw(w, author, seq, profile())).unwrap(),
                InsertOutcome::Inserted
            );
            assert_eq!(
                count.get(),
                1,
                "remote insert may verify only its new event"
            );
        }
        count.set(0);
        let signed = append(&mut db, w, &owner, profile());
        assert_eq!(
            count.get(),
            1,
            "own append including current_epoch must not verify history"
        );
        assert_eq!(
            verify_and_decode(&signed, &Ed25519Verifier)
                .unwrap()
                .membership_epoch,
            1
        );
    }
    drop(db);
    count.set(0);
    let mut db = open(&path, &count);
    assert_eq!(
        count.get(),
        0,
        "completed backfill must never rescan history on reopen"
    );
    assert_eq!(db.current_epoch(w).unwrap(), 1);
    assert!(db.confirmed_author(w, member.device_id()).unwrap());
    assert!(!db.confirmed_author(w, unknown.device_id()).unwrap());
    assert_eq!(count.get(), 0);
    assert_eq!(
        db.insert_remote(&raw(w, &member, 4, profile())).unwrap(),
        InsertOutcome::Inserted
    );
    assert_eq!(count.get(), 1);
    count.set(0);
    append(&mut db, w, &member, profile());
    assert_eq!(count.get(), 1);
}
#[test]
fn legacy_backfill_precedes_cleanup_and_persists_removal_and_gap_semantics() {
    let dir = directory();
    let path = dir.path().join("meshyard.sqlite");
    let owner = SecretKey::from_bytes(&[74; 32]);
    let established = SecretKey::from_bytes(&[75; 32]);
    let removed_only = SecretKey::from_bytes(&[76; 32]);
    let gapped = DeviceId([77; 32]);
    let forged = DeviceId([78; 32]);
    let bytes = genesis(&owner);
    let (w, g) = verify_genesis(&decode_genesis(&bytes).unwrap(), &Ed25519Verifier).unwrap();
    let mut conn = Connection::open(&path).unwrap();
    migrations().to_version(&mut conn, 1).unwrap();
    let tx = conn.transaction().unwrap();
    tx.execute(
        "INSERT INTO workspaces VALUES(?1,?2)",
        params![w.0.as_slice(), bytes],
    )
    .unwrap();
    let bodies = vec![
        grant(established.device_id()),
        removal(established.device_id()),
        removal(removed_only.device_id()),
        removal(owner.device_id()),
        grant(owner.device_id()),
        profile(),
    ];
    let mut events = vec![];
    for (index, body) in bodies.into_iter().enumerate() {
        let raw = raw(w, &owner, index as u64 + 1, body);
        events.push(
            verify_and_decode(&decode_signed_event(&raw).unwrap(), &Ed25519Verifier)
                .unwrap()
                .into_event(),
        );
        tx.execute(
            "INSERT INTO events VALUES(?1,?2,?3,?4)",
            params![
                w.0.as_slice(),
                owner.device_id().0.as_slice(),
                index as i64 + 1,
                raw
            ],
        )
        .unwrap();
    }
    tx.execute(
        "INSERT INTO pending_events VALUES(?1,?2,8,?3)",
        params![
            w.0.as_slice(),
            owner.device_id().0.as_slice(),
            raw(w, &owner, 8, grant(gapped))
        ],
    )
    .unwrap();
    for author in [&established, &removed_only] {
        for seq in 1..=1025 {
            tx.execute(
                "INSERT INTO events VALUES(?1,?2,?3,?4)",
                params![
                    w.0.as_slice(),
                    author.device_id().0.as_slice(),
                    sql_seq(seq).unwrap(),
                    raw(w, author, seq, profile())
                ],
            )
            .unwrap();
        }
    }
    // Non-owner grants cannot seed the derived membership table.
    tx.execute(
        "INSERT INTO events VALUES(?1,?2,1026,?3)",
        params![
            w.0.as_slice(),
            established.device_id().0.as_slice(),
            raw(w, &established, 1026, grant(forged))
        ],
    )
    .unwrap();
    tx.commit().unwrap();
    drop(conn);
    let count = Rc::new(Cell::new(0));
    let db = open(&path, &count);
    assert_eq!(count.get(), 7, "one genesis plus six owner events, once");
    let state = fold(&g, &events).unwrap();
    assert_eq!(db.current_epoch(w).unwrap(), state.epoch);
    assert_eq!(state.epoch, 3);
    assert_eq!(member_row(&db, w, owner.device_id()), Some((0, false)));
    assert_eq!(member_row(&db, w, established.device_id()), Some((1, true)));
    for device in [removed_only.device_id(), gapped, forged] {
        assert!(!db.confirmed_author(w, device).unwrap());
    }
    assert_eq!(db.head(w, established.device_id()).unwrap(), 1026);
    assert_eq!(db.head(w, removed_only.device_id()).unwrap(), 0);
    assert!(
        db.diagnostics(w, owner.device_id())
            .unwrap()
            .protocol_violation
    );
    drop(db);
    count.set(0);
    let mut db = open(&path, &count);
    assert_eq!(count.get(), 0);
    assert_eq!(db.current_epoch(w).unwrap(), 3);
    assert_eq!(member_row(&db, w, established.device_id()), Some((1, true)));
    assert_eq!(
        db.insert_remote(&raw(w, &owner, 7, profile())).unwrap(),
        InsertOutcome::Inserted
    );
    assert_eq!(count.get(), 2, "only the gap filler and promoted grant");
    assert_eq!(db.current_epoch(w).unwrap(), 4);
    assert_eq!(member_row(&db, w, gapped), Some((4, false)));
    append(&mut db, w, &owner, grant(established.device_id()));
    assert_eq!(
        member_row(&db, w, established.device_id()),
        Some((1, false))
    );
}
#[test]
fn promotion_and_epoch_overflow_rollback_events_members_and_diagnostics() {
    let dir = directory();
    let count = Rc::new(Cell::new(0));
    let mut db = open(&dir.path().join("meshyard.sqlite"), &count);
    let owner = SecretKey::from_bytes(&[79; 32]);
    let target = DeviceId([80; 32]);
    let w = db.add_workspace(&genesis(&owner)).unwrap();
    let gap = raw(w, &owner, 3, grant(target));
    assert_eq!(db.insert_remote(&gap).unwrap(), InsertOutcome::Pending);
    assert!(!db.confirmed_author(w, target).unwrap());
    db.conn.execute_batch("CREATE TRIGGER fail_promotion BEFORE DELETE ON pending_events BEGIN SELECT RAISE(ABORT,'test rollback'); END;").unwrap();
    let filler = raw(w, &owner, 1, removal(owner.device_id()));
    assert_eq!(db.insert_remote(&filler).unwrap(), InsertOutcome::Inserted);
    assert_eq!(db.current_epoch(w).unwrap(), 0);
    assert!(matches!(
        db.insert_remote(&raw(w, &owner, 2, grant(DeviceId([81; 32])))),
        Err(StoreError::Database)
    ));
    assert_eq!(db.head(w, owner.device_id()).unwrap(), 1);
    assert_eq!(db.current_epoch(w).unwrap(), 0);
    assert!(!db.confirmed_author(w, target).unwrap());
    assert!(!db.confirmed_author(w, DeviceId([81; 32])).unwrap());
    assert_eq!(db.insert_remote(&gap).unwrap(), InsertOutcome::Duplicate);
    db.conn
        .execute_batch("DROP TRIGGER fail_promotion")
        .unwrap();
    db.conn
        .execute("UPDATE membership_state SET epoch=?1", [u32::MAX - 1])
        .unwrap();
    // The filler reaches MAX; promotion overflows and rolls the entire transaction back.
    let filler = raw(w, &owner, 2, grant(DeviceId([81; 32])));
    assert!(matches!(
        db.insert_remote(&filler),
        Err(StoreError::SequenceRange)
    ));
    assert_eq!(db.current_epoch(w).unwrap(), u32::MAX - 1);
    assert_eq!(db.head(w, owner.device_id()).unwrap(), 1);
    assert!(!db.confirmed_author(w, target).unwrap());
    assert!(!db.confirmed_author(w, DeviceId([81; 32])).unwrap());
    db.conn
        .execute("UPDATE membership_state SET epoch=0", [])
        .unwrap();
    assert_eq!(db.insert_remote(&filler).unwrap(), InsertOutcome::Inserted);
    assert_eq!(db.current_epoch(w).unwrap(), 2);
    assert!(db.confirmed_author(w, target).unwrap());
    assert_eq!(db.insert_remote(&filler).unwrap(), InsertOutcome::Duplicate);
    assert_eq!(
        db.insert_remote(&raw(w, &owner, 2, removal(target)))
            .unwrap(),
        InsertOutcome::Equivocation
    );
    let mut invalid = decode_signed_event(&raw(w, &owner, 4, grant(DeviceId([82; 32])))).unwrap();
    invalid.sig[0] ^= 1;
    assert_eq!(
        db.insert_remote(&postcard::to_stdvec(&invalid).unwrap())
            .unwrap(),
        InsertOutcome::Rejected(RejectReason::InvalidSignature)
    );
    assert_eq!(db.current_epoch(w).unwrap(), 2);
    assert_eq!(member_row(&db, w, target), Some((2, false)));
    assert!(!db.confirmed_author(w, DeviceId([82; 32])).unwrap());
    db.conn
        .execute("UPDATE membership_state SET epoch=?1", [u32::MAX])
        .unwrap();
    assert!(matches!(
        db.append_own(
            w,
            removal(target),
            &owner,
            SharingPolicy::default(),
            0,
            u32::MAX
        ),
        Err(StoreError::SequenceRange)
    ));
    assert_eq!(db.head(w, owner.device_id()).unwrap(), 3);
    assert_eq!(member_row(&db, w, target), Some((2, false)));
    // Self mutations remain no-ops even at MAX.
    append(&mut db, w, &owner, removal(owner.device_id()));
    append(&mut db, w, &owner, grant(owner.device_id()));
    assert_eq!(db.current_epoch(w).unwrap(), u32::MAX);
    assert_eq!(member_row(&db, w, owner.device_id()), Some((0, false)));
}
#[test]
fn failed_legacy_backfill_is_atomic_retryable_and_does_not_clean_logs() {
    let dir = directory();
    let path = dir.path().join("meshyard.sqlite");
    let owner = SecretKey::from_bytes(&[83; 32]);
    let unknown = SecretKey::from_bytes(&[85; 32]);
    let bytes = genesis(&owner);
    let (w, _) = verify_genesis(&decode_genesis(&bytes).unwrap(), &Ed25519Verifier).unwrap();
    let mut conn = Connection::open(&path).unwrap();
    migrations().to_version(&mut conn, 1).unwrap();
    conn.execute(
        "INSERT INTO workspaces VALUES(?1,?2)",
        params![w.0.as_slice(), bytes],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO events VALUES(?1,?2,1,?3)",
        params![
            w.0.as_slice(),
            owner.device_id().0.as_slice(),
            raw(w, &owner, 1, grant(DeviceId([84; 32])))
        ],
    )
    .unwrap();
    // Unsupported manually edited SQL: seq/body mismatch must fail closed, never
    // seed partial authority or run quota cleanup. This is not a network fixture.
    conn.execute(
        "INSERT INTO events VALUES(?1,?2,2,?3)",
        params![
            w.0.as_slice(),
            owner.device_id().0.as_slice(),
            raw(w, &owner, 9, profile())
        ],
    )
    .unwrap();
    for seq in 1..=1025 {
        conn.execute(
            "INSERT INTO events VALUES(?1,?2,?3,?4)",
            params![
                w.0.as_slice(),
                unknown.device_id().0.as_slice(),
                sql_seq(seq).unwrap(),
                raw(w, &unknown, seq, profile())
            ],
        )
        .unwrap();
    }
    drop(conn);
    assert!(matches!(Database::open(&path), Err(StoreError::Database)));
    let conn = Connection::open(&path).unwrap();
    for table in ["members", "membership_state"] {
        assert_eq!(
            conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM events", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1027,
        "failed backfill must not run oversized unknown-log cleanup"
    );
    let mut invalid = decode_signed_event(&raw(w, &owner, 2, profile())).unwrap();
    invalid.sig[0] ^= 1;
    conn.execute(
        "UPDATE events SET signed_blob=?1 WHERE author=?2 AND seq=2",
        params![
            postcard::to_stdvec(&invalid).unwrap(),
            owner.device_id().0.as_slice()
        ],
    )
    .unwrap();
    drop(conn);
    assert!(matches!(
        Database::open(&path),
        Err(StoreError::Protocol(CryptoError::Signature))
    ));
    let conn = Connection::open(&path).unwrap();
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM members", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM events", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1027
    );
    conn.execute(
        "UPDATE events SET signed_blob=?1 WHERE author=?2 AND seq=2",
        params![raw(w, &owner, 2, profile()), owner.device_id().0.as_slice()],
    )
    .unwrap();
    drop(conn);
    let db = Database::open(&path).unwrap();
    assert_eq!(db.current_epoch(w).unwrap(), 1);
    assert!(db.confirmed_author(w, DeviceId([84; 32])).unwrap());
    assert_eq!(db.head(w, unknown.device_id()).unwrap(), 0);
}

#[test]
fn live_membership_agrees_with_fold_and_failed_promotion_rolls_back_diagnostics() {
    let dir = directory();
    let mut db = Database::open(&dir.path().join("meshyard.sqlite")).unwrap();
    let owner = SecretKey::from_bytes(&[86; 32]);
    let member = SecretKey::from_bytes(&[87; 32]);
    let unknown = SecretKey::from_bytes(&[88; 32]);
    let w = db.add_workspace(&genesis(&owner)).unwrap();
    let (_, g) =
        verify_genesis(&decode_genesis(&genesis(&owner)).unwrap(), &Ed25519Verifier).unwrap();
    let mut events = vec![];
    let bodies = [
        grant(member.device_id()),
        removal(unknown.device_id()),
        removal(member.device_id()),
        grant(owner.device_id()),
        removal(owner.device_id()),
        grant(member.device_id()),
    ];
    for (index, body) in bodies.into_iter().enumerate() {
        let bytes = raw(w, &owner, index as u64 + 1, body);
        let event =
            verify_and_decode(&decode_signed_event(&bytes).unwrap(), &Ed25519Verifier).unwrap();
        assert_eq!(db.insert_remote(&bytes).unwrap(), InsertOutcome::Inserted);
        events.push(event.into_event());
        let state = fold(&g, &events).unwrap();
        assert_eq!(db.current_epoch(w).unwrap(), state.epoch);
        for device in [owner.device_id(), member.device_id()] {
            if let Some((_, removed)) = member_row(&db, w, device) {
                assert_eq!(!removed, state.members.contains_key(&device));
            }
        }
        assert!(!db.confirmed_author(w, unknown.device_id()).unwrap());
    }
    let epoch = db.current_epoch(w).unwrap();
    assert_eq!(member_row(&db, w, member.device_id()), Some((1, false)));
    for (seq, body) in [
        (1, grant(unknown.device_id())),
        (2, removal(member.device_id())),
    ] {
        assert_eq!(
            db.insert_remote(&raw(w, &member, seq, body)).unwrap(),
            InsertOutcome::Inserted
        );
    }
    assert_eq!(db.current_epoch(w).unwrap(), epoch);
    assert!(!db.confirmed_author(w, unknown.device_id()).unwrap());
    assert_eq!(member_row(&db, w, member.device_id()), Some((1, false)));

    // A contiguous cross-author owner-removal diagnostic is part of the receive
    // transaction, and must roll back if promoting the next row fails.
    let pending = raw(w, &unknown, 2, profile());
    assert_eq!(db.insert_remote(&pending).unwrap(), InsertOutcome::Pending);
    db.conn.execute_batch("CREATE TRIGGER fail_diagnostic_promotion BEFORE DELETE ON pending_events BEGIN SELECT RAISE(ABORT,'test rollback'); END;").unwrap();
    let attempt = raw(w, &unknown, 1, removal(owner.device_id()));
    assert!(matches!(
        db.insert_remote(&attempt),
        Err(StoreError::Database)
    ));
    assert!(
        !db.diagnostics(w, unknown.device_id())
            .unwrap()
            .protocol_violation
    );
    assert_eq!(db.head(w, unknown.device_id()).unwrap(), 0);
    assert_eq!(db.current_epoch(w).unwrap(), epoch);
    assert_eq!(
        db.insert_remote(&pending).unwrap(),
        InsertOutcome::Duplicate
    );
    db.conn
        .execute_batch("DROP TRIGGER fail_diagnostic_promotion")
        .unwrap();
    assert_eq!(db.insert_remote(&attempt).unwrap(), InsertOutcome::Inserted);
    assert!(
        db.diagnostics(w, unknown.device_id())
            .unwrap()
            .protocol_violation
    );
    assert_eq!(db.current_epoch(w).unwrap(), epoch);
    assert_eq!(member_row(&db, w, owner.device_id()), Some((0, false)));
    assert!(!db.confirmed_author(w, unknown.device_id()).unwrap());
}
