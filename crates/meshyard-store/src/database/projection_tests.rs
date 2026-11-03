use super::*;

fn registration(id: u8, kind: AgentKind) -> EventBody {
    EventBody::AgentRegistered {
        agent_local_id: [id; 16],
        kind,
        label: "agent".into(),
    }
}
fn activity(id: u8) -> EventBody {
    EventBody::AgentState {
        agent_local_id: [id; 16],
        state: ActivityState::Testing,
        provenance: Provenance::Inferred {
            rule: RuleId::ToolShell,
        },
        tool: Some(ToolCategory::Shell),
        task_label: None,
    }
}
fn gone(id: u8) -> EventBody {
    EventBody::AgentGone {
        agent_local_id: [id; 16],
    }
}
fn assert_oracle(db: &Database, w: WorkspaceId) -> WorkspaceSnapshot {
    let snapshot = db.workspace_snapshot(w).unwrap();
    let (genesis, oracle) = db.workspace_state(w).unwrap();
    assert_eq!(snapshot.genesis, genesis);
    assert_eq!(snapshot.epoch, oracle.epoch);
    assert_eq!(
        snapshot.members,
        oracle.members.into_values().collect::<Vec<_>>()
    );
    assert_eq!(
        snapshot.agents,
        oracle.agents.into_values().collect::<Vec<_>>()
    );
    snapshot
}
#[test]
fn projection_5000_history_backfill_once_and_constant_reads_appends_promotions() {
    let dir = directory();
    let path = dir.path().join("meshyard.sqlite");
    let count = Rc::new(Cell::new(0));
    let owner = SecretKey::from_bytes(&[91; 32]);
    let member = SecretKey::from_bytes(&[92; 32]);
    let mut db = open(&path, &count);
    let w = db.add_workspace(&genesis(&owner)).unwrap();
    // Real signatures and production insertion, batched only to avoid 5000 fsyncs.
    let tx = Transaction::new_unchecked(&db.conn, TransactionBehavior::Immediate).unwrap();
    for seq in 1..=5000 {
        let body = match seq {
            1 => grant(member.device_id()),
            2 => registration(1, AgentKind::ClaudeCode),
            _ => activity(1),
        };
        assert_eq!(
            db.insert(&raw(w, &owner, seq, body)).unwrap(),
            InsertOutcome::Inserted
        );
    }
    for (seq, body) in [
        (1, profile()),
        (2, registration(1, AgentKind::Codex)),
        (3, activity(1)),
    ] {
        assert_eq!(
            db.insert(&raw(w, &member, seq, body)).unwrap(),
            InsertOutcome::Inserted
        );
    }
    tx.commit().unwrap();
    db.bind_first_own_agent(w, [1; 16], owner.device_id())
        .unwrap();
    let before = assert_oracle(&db, w);
    // Exercise an actual upgraded database, not an already-marked new workspace.
    db.conn
        .execute("DELETE FROM meta WHERE key LIKE 'projection_v1:%'", [])
        .unwrap();
    db.conn.execute("DELETE FROM agent_projection", []).unwrap();
    db.conn
        .execute("DELETE FROM profile_projection", [])
        .unwrap();
    drop(db);
    count.set(0);
    let mut db = open(&path, &count);
    assert_eq!(
        count.get(),
        5004,
        "one genesis and each contiguous event exactly once at upgrade"
    );
    assert_eq!(db.workspace_snapshot(w).unwrap(), before);
    count.set(0);
    for _ in 0..100 {
        assert_eq!(db.workspace_snapshot(w).unwrap(), before);
        assert!(
            db.own_agent(w, [1; 16], owner.device_id())
                .unwrap()
                .is_some()
        );
        assert_eq!(
            db.agent_binding(w, AgentKind::ClaudeCode, owner.device_id())
                .unwrap(),
            Some([1; 16])
        );
        db.bind_own_agent(w, [1; 16], owner.device_id()).unwrap();
        db.bind_first_own_agent(w, [1; 16], owner.device_id())
            .unwrap();
    }
    assert_eq!(
        count.get(),
        0,
        "all reads/binds must avoid historical and genesis verification"
    );
    for _ in 0..10 {
        count.set(0);
        append(&mut db, w, &owner, activity(1));
        assert_eq!(
            count.get(),
            1,
            "exactly the new own event, independent of history"
        );
    }
    count.set(0);
    assert_eq!(
        db.insert_remote(&raw(w, &member, 5, activity(1))).unwrap(),
        InsertOutcome::Pending
    );
    assert_eq!(count.get(), 1);
    count.set(0);
    assert_eq!(
        db.insert_remote(&raw(w, &member, 4, profile())).unwrap(),
        InsertOutcome::Inserted
    );
    assert_eq!(count.get(), 2, "only filler and promoted event");
    let final_snapshot = assert_oracle(&db, w);
    drop(db);
    count.set(0);
    let db = open(&path, &count);
    assert_eq!(count.get(), 1, "cached genesis once per new DB actor");
    count.set(0);
    assert_eq!(db.workspace_snapshot(w).unwrap(), final_snapshot);
    assert_eq!(count.get(), 0);
}

#[test]
fn projection_script_matches_fold_authority_reset_membership_gaps_and_atomic_rollback() {
    let dir = directory();
    let path = dir.path().join("meshyard.sqlite");
    let mut db = Database::open(&path).unwrap();
    let owner = SecretKey::from_bytes(&[93; 32]);
    let remote = SecretKey::from_bytes(&[94; 32]);
    let w = db.add_workspace(&genesis(&owner)).unwrap();
    let mut heads = BTreeMap::new();
    for (key, body) in [
        (&owner, profile()),
        (&owner, activity(1)), // State before registration cannot materialize an agent.
        (&owner, registration(1, AgentKind::ClaudeCode)),
        (&owner, activity(1)),
        (&owner, registration(1, AgentKind::ClaudeCode)), // Reset all activity metadata.
        (&remote, registration(1, AgentKind::Codex)), // Same ID, separate author, hidden until grant.
        (&remote, profile()),
        (&remote, activity(1)),
        (&owner, grant(remote.device_id())),
        (&owner, removal(remote.device_id())),
        (&remote, registration(2, AgentKind::ClaudeCode)),
        (
            &owner,
            EventBody::MemberAdded {
                device: remote.device_id(),
                display_name: "new grant".into(),
            },
        ),
        (&remote, grant(DeviceId([95; 32]))), // No membership authority.
        (&remote, removal(owner.device_id())),
        (&owner, grant(owner.device_id())),
        (&owner, removal(owner.device_id())),
    ] {
        let seq = heads.entry(key.device_id()).or_insert(0u64);
        *seq += 1;
        assert_eq!(
            db.insert_remote(&raw(w, key, *seq, body)).unwrap(),
            InsertOutcome::Inserted
        );
        assert_oracle(&db, w);
    }
    let snapshot = db.workspace_snapshot(w).unwrap();
    assert_eq!(snapshot.agents.len(), 3);
    let owner_agent = snapshot
        .agents
        .iter()
        .find(|a| a.author == owner.device_id())
        .unwrap();
    assert_eq!(owner_agent.provenance, None);
    assert!(
        !snapshot
            .agent_state_ts
            .contains_key(&(owner.device_id(), [1; 16]))
    );
    assert_eq!(
        snapshot
            .members
            .iter()
            .find(|m| m.device == remote.device_id())
            .unwrap()
            .display_name,
        "test"
    );
    db.bind_own_agent(w, [1; 16], owner.device_id()).unwrap();
    assert_eq!(
        db.agent_binding(w, AgentKind::ClaudeCode, owner.device_id())
            .unwrap(),
        Some([1; 16])
    );
    assert!(matches!(
        db.bind_own_agent(w, [2; 16], owner.device_id()),
        Err(StoreError::InvalidBinding)
    ));
    append(&mut db, w, &owner, registration(1, AgentKind::Codex));
    assert_eq!(
        db.agent_binding(w, AgentKind::ClaudeCode, owner.device_id())
            .unwrap(),
        None,
        "wrong kind is stale"
    );
    assert_oracle(&db, w);
    db.bind_own_agent(w, [1; 16], owner.device_id()).unwrap();
    append(&mut db, w, &owner, gone(1));
    assert_eq!(
        db.agent_binding(w, AgentKind::Codex, owner.device_id())
            .unwrap(),
        None,
        "gone is stale"
    );
    assert_oracle(&db, w);
    let head = db.head(w, owner.device_id()).unwrap();
    let before = db.workspace_snapshot(w).unwrap();
    let pending = raw(w, &owner, head + 2, activity(3));
    assert_eq!(db.insert_remote(&pending).unwrap(), InsertOutcome::Pending);
    assert_eq!(assert_oracle(&db, w), before);
    let filler = raw(w, &owner, head + 1, registration(3, AgentKind::ClaudeCode));
    db.conn.execute_batch("CREATE TRIGGER fail_projection_promotion BEFORE DELETE ON pending_events BEGIN SELECT RAISE(ABORT,'rollback'); END;").unwrap();
    assert!(matches!(
        db.insert_remote(&filler),
        Err(StoreError::Database)
    ));
    assert_eq!(db.head(w, owner.device_id()).unwrap(), head);
    assert_eq!(
        assert_oracle(&db, w),
        before,
        "no partial event or projection may escape"
    );
    db.conn
        .execute_batch("DROP TRIGGER fail_projection_promotion")
        .unwrap();
    assert_eq!(db.insert_remote(&filler).unwrap(), InsertOutcome::Inserted);
    let promoted = assert_oracle(&db, w);
    assert_eq!(
        promoted.agent_state_ts.get(&(owner.device_id(), [3; 16])),
        Some(&0)
    );
    assert_eq!(
        db.insert_remote(&raw(w, &owner, head + 1, gone(3)))
            .unwrap(),
        InsertOutcome::Equivocation
    );
    assert_eq!(
        db.insert_remote(&pending).unwrap(),
        InsertOutcome::Duplicate
    );
    let mut invalid = decode_signed_event(&raw(w, &owner, head + 3, gone(3))).unwrap();
    invalid.sig[0] ^= 1;
    assert_eq!(
        db.insert_remote(&postcard::to_stdvec(&invalid).unwrap())
            .unwrap(),
        InsertOutcome::Rejected(RejectReason::InvalidSignature)
    );
    assert_eq!(assert_oracle(&db, w), promoted);
    append(&mut db, w, &owner, removal(remote.device_id()));
    assert_eq!(
        db.agent_binding(w, AgentKind::Codex, remote.device_id())
            .unwrap(),
        None
    );
    assert!(
        db.own_agent(w, [1; 16], remote.device_id())
            .unwrap()
            .is_none()
    );
    assert_oracle(&db, w);
    append(&mut db, w, &owner, grant(remote.device_id()));
    let final_snapshot = assert_oracle(&db, w);
    drop(db);
    let db = Database::open(&path).unwrap();
    assert_eq!(assert_oracle(&db, w), final_snapshot);
    db.conn
        .execute("DELETE FROM meta WHERE key LIKE 'projection_v1:%'", [])
        .unwrap();
    drop(db);
    let db = Database::open(&path).unwrap();
    assert_eq!(
        assert_oracle(&db, w),
        final_snapshot,
        "fresh backfill equals incremental and reopen"
    );
}

#[test]
fn projection_quota_deletes_unknown_rows_incrementally_and_at_reopen() {
    let dir = directory();
    let path = dir.path().join("meshyard.sqlite");
    let mut db = Database::open(&path).unwrap();
    let owner = SecretKey::from_bytes(&[96; 32]);
    let unknown = SecretKey::from_bytes(&[97; 32]);
    let w = db.add_workspace(&genesis(&owner)).unwrap();
    let tx = Transaction::new_unchecked(&db.conn, TransactionBehavior::Immediate).unwrap();
    for seq in 1..=1024 {
        let body = if seq == 1 {
            registration(1, AgentKind::ClaudeCode)
        } else {
            profile()
        };
        assert_eq!(
            db.insert(&raw(w, &unknown, seq, body)).unwrap(),
            InsertOutcome::Inserted
        );
    }
    tx.commit().unwrap();
    let rows = |db: &Database, table: &str| {
        db.conn
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE workspace=?1 AND author=?2"),
                params![w.0.as_slice(), unknown.device_id().0.as_slice()],
                |r| r.get::<_, i64>(0),
            )
            .unwrap()
    };
    assert_eq!(rows(&db, "agent_projection"), 1);
    assert_eq!(rows(&db, "profile_projection"), 1);
    assert_eq!(
        db.insert_remote(&raw(w, &unknown, 1025, profile()))
            .unwrap(),
        InsertOutcome::Rejected(RejectReason::PendingOverflow)
    );
    for table in [
        "events",
        "pending_events",
        "agent_projection",
        "profile_projection",
    ] {
        assert_eq!(rows(&db, table), 0);
    }
    append(&mut db, w, &owner, grant(unknown.device_id()));
    assert!(
        assert_oracle(&db, w).agents.is_empty(),
        "quota-deleted state cannot revive on a grant"
    );
    // Another unconfirmed author, reconstruct an old oversized database at the actual SQL layer.
    let legacy = SecretKey::from_bytes(&[98; 32]);
    for (seq, body) in [(1, registration(1, AgentKind::ClaudeCode)), (2, profile())] {
        db.insert_remote(&raw(w, &legacy, seq, body)).unwrap();
    }
    let tx = Transaction::new_unchecked(&db.conn, TransactionBehavior::Immediate).unwrap();
    for seq in 3..=1025 {
        db.conn
            .execute(
                "INSERT INTO events VALUES(?1,?2,?3,?4)",
                params![
                    w.0.as_slice(),
                    legacy.device_id().0.as_slice(),
                    sql_seq(seq).unwrap(),
                    raw(w, &legacy, seq, profile())
                ],
            )
            .unwrap();
    }
    tx.commit().unwrap();
    drop(db);
    let db = Database::open(&path).unwrap();
    for table in [
        "events",
        "pending_events",
        "agent_projection",
        "profile_projection",
    ] {
        assert_eq!(
            db.conn
                .query_row(
                    &format!("SELECT COUNT(*) FROM {table} WHERE author=?1"),
                    [legacy.device_id().0.as_slice()],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
    }
    assert_oracle(&db, w);
}

#[test]
fn projection_backfill_failure_rolls_back_markers_and_rows_then_retries() {
    let dir = directory();
    let path = dir.path().join("meshyard.sqlite");
    let mut db = Database::open(&path).unwrap();
    let owner = SecretKey::from_bytes(&[99; 32]);
    let w = db.add_workspace(&genesis(&owner)).unwrap();
    append(&mut db, w, &owner, registration(1, AgentKind::ClaudeCode));
    append(&mut db, w, &owner, activity(1));
    let before = assert_oracle(&db, w);
    db.conn
        .execute("DELETE FROM meta WHERE key LIKE 'projection_v1:%'", [])
        .unwrap();
    db.conn.execute_batch("CREATE TRIGGER fail_backfill BEFORE INSERT ON agent_projection BEGIN SELECT RAISE(ABORT,'rollback'); END;").unwrap();
    drop(db);
    assert!(matches!(Database::open(&path), Err(StoreError::Database)));
    let sql = Connection::open(&path).unwrap();
    assert_eq!(
        sql.query_row(
            "SELECT COUNT(*) FROM meta WHERE key LIKE 'projection_v1:%'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        sql.query_row("SELECT COUNT(*) FROM agent_projection", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1,
        "preexisting rows survive failed rebuild"
    );
    assert_eq!(
        sql.query_row("SELECT COUNT(*) FROM events", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    sql.execute_batch("DROP TRIGGER fail_backfill").unwrap();
    drop(sql);
    let db = Database::open(&path).unwrap();
    assert_eq!(assert_oracle(&db, w), before);
}

#[test]
fn projection_preserves_limit_errors_and_fails_closed_on_corrupt_domain_rows() {
    let dir = directory();
    let path = dir.path().join("meshyard.sqlite");
    let count = Rc::new(Cell::new(0));
    let mut db = open(&path, &count);
    let owner = SecretKey::from_bytes(&[103; 32]);
    let w = db.add_workspace(&genesis(&owner)).unwrap();
    append(&mut db, w, &owner, registration(1, AgentKind::ClaudeCode));
    db.bind_own_agent(w, [1; 16], owner.device_id()).unwrap();
    db.conn.execute("WITH RECURSIVE n(x) AS (VALUES(2) UNION ALL SELECT x+1 FROM n WHERE x<100001) INSERT INTO events(workspace,author,seq,signed_blob) SELECT ?1,?2,x,x'00' FROM n",params![w.0.as_slice(),owner.device_id().0.as_slice()]).unwrap();
    count.set(0);
    assert!(matches!(
        db.workspace_snapshot(w),
        Err(StoreError::ProjectionLimit)
    ));
    assert!(matches!(
        db.own_agent(w, [1; 16], owner.device_id()),
        Err(StoreError::ProjectionLimit)
    ));
    assert!(matches!(
        db.agent_binding(w, AgentKind::ClaudeCode, owner.device_id()),
        Err(StoreError::ProjectionLimit)
    ));
    assert!(matches!(
        db.bind_own_agent(w, [1; 16], owner.device_id()),
        Err(StoreError::ProjectionLimit)
    ));
    assert_eq!(count.get(), 0);
    db.conn
        .execute("DELETE FROM events WHERE seq>1", [])
        .unwrap();
    db.conn
        .execute("UPDATE agent_projection SET value=x'ff'", [])
        .unwrap();
    assert!(matches!(
        db.workspace_snapshot(w),
        Err(StoreError::Database)
    ));
    assert!(matches!(
        db.own_agent(w, [1; 16], owner.device_id()),
        Err(StoreError::Database)
    ));
    // Audit ignores derived corruption and still verifies the retained accepted event.
    assert_eq!(db.workspace_state(w).unwrap().1.agents.len(), 1);
    assert!(matches!(
        db.workspace_snapshot(WorkspaceId([255; 16])),
        Err(StoreError::UnknownWorkspace)
    ));
}

#[test]
fn projection_preserves_full_u64_state_timestamp_and_latest_grant_name() {
    let dir = directory();
    let mut db = Database::open(&dir.path().join("meshyard.sqlite")).unwrap();
    let owner = SecretKey::from_bytes(&[104; 32]);
    let member = SecretKey::from_bytes(&[105; 32]);
    let w = db.add_workspace(&genesis(&owner)).unwrap();
    append(&mut db, w, &owner, grant(member.device_id()));
    append(&mut db, w, &member, registration(1, AgentKind::ClaudeCode));
    db.bind_own_agent(w, [1; 16], member.device_id()).unwrap();
    db.append_own(
        w,
        activity(1),
        &member,
        SharingPolicy::default(),
        u64::MAX,
        1,
    )
    .unwrap();
    assert_eq!(
        assert_oracle(&db, w).agent_state_ts[&(member.device_id(), [1; 16])],
        u64::MAX
    );
    append(&mut db, w, &owner, removal(member.device_id()));
    assert_eq!(
        db.agent_binding(w, AgentKind::ClaudeCode, member.device_id())
            .unwrap(),
        None
    );
    assert_oracle(&db, w);
    append(
        &mut db,
        w,
        &owner,
        EventBody::MemberAdded {
            device: member.device_id(),
            display_name: "latest grant".into(),
        },
    );
    let snapshot = assert_oracle(&db, w);
    assert_eq!(
        snapshot
            .members
            .iter()
            .find(|m| m.device == member.device_id())
            .unwrap()
            .display_name,
        "latest grant"
    );
    assert_eq!(
        snapshot.agent_state_ts[&(member.device_id(), [1; 16])],
        u64::MAX
    );
    assert_eq!(
        db.agent_binding(w, AgentKind::ClaudeCode, member.device_id())
            .unwrap(),
        Some([1; 16])
    );
    append(&mut db, w, &member, registration(1, AgentKind::ClaudeCode));
    let snapshot = assert_oracle(&db, w);
    assert!(
        !snapshot
            .agent_state_ts
            .contains_key(&(member.device_id(), [1; 16]))
    );
    let agent = db
        .own_agent(w, [1; 16], member.device_id())
        .unwrap()
        .unwrap();
    assert_eq!(
        (agent.state, agent.provenance, agent.tool, agent.task_label),
        (ActivityState::Idle, None, None, None)
    );
}
