use crate::StoreError;
use meshyard_core::{wire::v1::*, *};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use rusqlite_migration::{M, Migrations};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RejectReason {
    InvalidSignature,
    Malformed,
    InvalidField,
    UnknownWorkspace,
    SequenceRange,
    PendingOverflow,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InsertOutcome {
    Inserted,
    Duplicate,
    Pending,
    Equivocation,
    Rejected(RejectReason),
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Diagnostics {
    pub equivocated: bool,
    pub protocol_violation: bool,
}
pub(crate) struct Database {
    conn: Connection,
    verifier: Box<dyn Verifier>,
}
fn migrations() -> Migrations<'static> {
    Migrations::new(vec![M::up("
        CREATE TABLE workspaces (id BLOB PRIMARY KEY CHECK(length(id)=16), genesis_blob BLOB NOT NULL);
        CREATE TABLE events (workspace BLOB NOT NULL REFERENCES workspaces(id), author BLOB NOT NULL CHECK(length(author)=32), seq INTEGER NOT NULL CHECK(seq>0), signed_blob BLOB NOT NULL, PRIMARY KEY(workspace,author,seq));
        CREATE TABLE pending_events (workspace BLOB NOT NULL REFERENCES workspaces(id), author BLOB NOT NULL CHECK(length(author)=32), seq INTEGER NOT NULL CHECK(seq>0), signed_blob BLOB NOT NULL, PRIMARY KEY(workspace,author,seq));
        CREATE TABLE peers (workspace BLOB NOT NULL REFERENCES workspaces(id), device BLOB NOT NULL, equivocated INTEGER NOT NULL DEFAULT 0, protocol_violation INTEGER NOT NULL DEFAULT 0, PRIMARY KEY(workspace,device));
        CREATE TABLE pairings (workspace BLOB NOT NULL REFERENCES workspaces(id), invite_id BLOB NOT NULL, token_hash BLOB NOT NULL, state TEXT NOT NULL CHECK(state IN ('Open','Reserved','Completed','Aborted','Expired')), expires_ms INTEGER NOT NULL, joiner BLOB, joiner_nonce BLOB, inviter_nonce BLOB, reserved_ms INTEGER, inviter_confirmed INTEGER NOT NULL DEFAULT 0, joiner_confirmed INTEGER NOT NULL DEFAULT 0, transcript_hash BLOB, PRIMARY KEY(workspace,invite_id));
        CREATE TABLE meta (key TEXT PRIMARY KEY, value BLOB NOT NULL);
    "), M::up("
        CREATE TABLE membership_state (workspace BLOB PRIMARY KEY REFERENCES workspaces(id), owner BLOB NOT NULL CHECK(length(owner)=32), epoch INTEGER NOT NULL CHECK(epoch BETWEEN 0 AND 4294967295));
        CREATE TABLE members (workspace BLOB NOT NULL REFERENCES membership_state(workspace), device BLOB NOT NULL CHECK(length(device)=32), epoch_added INTEGER NOT NULL CHECK(epoch_added BETWEEN 0 AND 4294967295), removed INTEGER NOT NULL CHECK(removed IN (0,1)), PRIMARY KEY(workspace,device));
    "), M::up("
        CREATE TABLE agent_bindings (workspace BLOB NOT NULL REFERENCES workspaces(id), kind INTEGER NOT NULL CHECK(kind IN (0,1,2)), agent_local_id BLOB NOT NULL CHECK(length(agent_local_id)=16), PRIMARY KEY(workspace,kind));
    ")])
}
fn binding_kind(kind: AgentKind) -> i32 {
    match kind {
        AgentKind::ClaudeCode => 0,
        AgentKind::Codex => 1,
        AgentKind::Other => 2,
    }
}
fn sql_seq(seq: u64) -> Result<i64, StoreError> {
    i64::try_from(seq).map_err(|_| StoreError::SequenceRange)
}
fn decode_genesis(raw: &[u8]) -> Result<SignedGenesisV1, StoreError> {
    if raw.len() > MAX_FRAME {
        return Err(CryptoError::Encoding.into());
    }
    let (value, rest) = postcard::take_from_bytes(raw).map_err(|_| CryptoError::Encoding)?;
    if !rest.is_empty() {
        return Err(CryptoError::Encoding.into());
    }
    Ok(value)
}
impl Database {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        Self::open_with_verifier(path, Box::new(Ed25519Verifier))
    }
    // Private injection lets tests count real strict verification without bypassing it.
    fn open_with_verifier(path: &Path, verifier: Box<dyn Verifier>) -> Result<Self, StoreError> {
        // Only the canonical directory retained by DataDirLock reaches this API.
        // Validate existing sidecars too; SQLite creates future ones inside that
        // private, non-substitutable directory, inheriting the database mode.
        crate::process_lock::sqlite_file(path, true)?;
        for suffix in ["-wal", "-shm", "-journal"] {
            let mut sidecar = path.as_os_str().to_owned();
            sidecar.push(suffix);
            crate::process_lock::sqlite_file(Path::new(&sidecar), false)?;
        }
        let mut conn = Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::default() | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        migrations()
            .to_latest(&mut conn)
            .map_err(|_| StoreError::Migration)?;
        let db = Self { conn, verifier };
        // Backfill must precede quota cleanup: historical grants protect removed
        // members too. A missing state row is the durable, retryable backfill marker.
        db.backfill_membership()?;
        db.discard_oversized_unconfirmed()?;
        Ok(db)
    }
    pub fn workspaces(&self) -> Result<Vec<WorkspaceId>, StoreError> {
        let mut query = self.conn.prepare("SELECT id FROM workspaces ORDER BY id")?;
        query
            .query_map([], |row| row.get::<_, Vec<u8>>(0))?
            .map(|row| {
                let bytes = row?;
                Ok(WorkspaceId(
                    bytes.try_into().map_err(|_| StoreError::Database)?,
                ))
            })
            .collect()
    }
    pub fn workspace_state(
        &self,
        workspace: WorkspaceId,
    ) -> Result<(Genesis, DerivedState), StoreError> {
        let (id, genesis) = verify_genesis(
            &decode_genesis(&self.genesis(workspace)?)?,
            self.verifier.as_ref(),
        )?;
        if id != workspace {
            return Err(StoreError::Database);
        }
        // The bounded subquery stops counting after the first disallowed row.
        let count: i64 = self.conn.query_row(
            "SELECT count(*) FROM (SELECT 1 FROM events WHERE workspace=?1 LIMIT 100001)",
            [workspace.0.as_slice()],
            |row| row.get(0),
        )?;
        if count > 100_000 {
            return Err(StoreError::ProjectionLimit);
        }
        let mut query = self.conn.prepare(
            "SELECT author,seq,signed_blob FROM events WHERE workspace=?1 ORDER BY author,seq",
        )?;
        let mut rows = query.query([workspace.0.as_slice()])?;
        let mut events =
            Vec::with_capacity(usize::try_from(count).map_err(|_| StoreError::Database)?);
        let mut heads = BTreeMap::new();
        while let Some(row) = rows.next()? {
            let author: Vec<u8> = row.get(0)?;
            let seq: i64 = row.get(1)?;
            let raw: Vec<u8> = row.get(2)?;
            let event = verify_and_decode(&decode_signed_event(&raw)?, self.verifier.as_ref())?;
            event.validate_received()?;
            let expected = heads.entry(event.author).or_insert(0u64);
            *expected = expected.checked_add(1).ok_or(StoreError::SequenceRange)?;
            if event.workspace != workspace
                || event.author.0.as_slice() != author
                || sql_seq(event.seq)? != seq
                || event.seq != *expected
            {
                return Err(StoreError::Database);
            }
            events.push(event.into_event());
        }
        let state = fold(&genesis, &events).map_err(|_| StoreError::Database)?;
        Ok((genesis, state))
    }
    fn own_agent(
        &self,
        workspace: WorkspaceId,
        local_id: [u8; 16],
        own: DeviceId,
    ) -> Result<Agent, StoreError> {
        let (_, state) = self.workspace_state(workspace)?;
        if !state.members.contains_key(&own) {
            return Err(StoreError::InvalidBinding);
        }
        state
            .agents
            .get(&(own, local_id))
            .cloned()
            .ok_or(StoreError::InvalidBinding)
    }
    pub fn bind_own_agent(
        &self,
        workspace: WorkspaceId,
        local_id: [u8; 16],
        own: DeviceId,
    ) -> Result<(), StoreError> {
        let agent = self.own_agent(workspace, local_id, own)?;
        self.conn.execute(
            "INSERT INTO agent_bindings(workspace,kind,agent_local_id) VALUES(?1,?2,?3) ON CONFLICT(workspace,kind) DO UPDATE SET agent_local_id=excluded.agent_local_id",
            params![workspace.0.as_slice(), binding_kind(agent.kind), local_id.as_slice()],
        )?;
        Ok(())
    }
    pub fn bind_first_own_agent(
        &self,
        workspace: WorkspaceId,
        local_id: [u8; 16],
        own: DeviceId,
    ) -> Result<(), StoreError> {
        let agent = self.own_agent(workspace, local_id, own)?;
        let (_, state) = self.workspace_state(workspace)?;
        if state
            .agents
            .values()
            .filter(|a| a.author == own && a.kind == agent.kind)
            .count()
            == 1
            && self.agent_binding(workspace, agent.kind, own)?.is_none()
        {
            self.bind_own_agent(workspace, local_id, own)?;
        }
        Ok(())
    }
    pub fn agent_binding(
        &self,
        workspace: WorkspaceId,
        kind: AgentKind,
        own: DeviceId,
    ) -> Result<Option<[u8; 16]>, StoreError> {
        let raw: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT agent_local_id FROM agent_bindings WHERE workspace=?1 AND kind=?2",
                params![workspace.0.as_slice(), binding_kind(kind)],
                |row| row.get(0),
            )
            .optional()?;
        raw.map(|bytes| {
            let id = bytes.try_into().map_err(|_| StoreError::InvalidBinding)?;
            let agent = self.own_agent(workspace, id, own)?;
            if agent.kind != kind {
                return Err(StoreError::InvalidBinding);
            }
            Ok(id)
        })
        .transpose()
    }
    fn seed_membership(&self, workspace: WorkspaceId, owner: DeviceId) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO membership_state(workspace,owner,epoch) VALUES(?1,?2,0)",
            params![workspace.0.as_slice(), owner.0.as_slice()],
        )?;
        self.conn.execute(
            "INSERT INTO members(workspace,device,epoch_added,removed) VALUES(?1,?2,0,0)",
            params![workspace.0.as_slice(), owner.0.as_slice()],
        )?;
        Ok(())
    }
    fn backfill_membership(&self) -> Result<(), StoreError> {
        let transaction = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let mut workspaces = self.conn.prepare(
            "SELECT id,genesis_blob FROM workspaces WHERE id NOT IN (SELECT workspace FROM membership_state)",
        )?;
        let rows = workspaces.query_map([], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?))
        })?;
        for row in rows {
            let (id, raw) = row?;
            let (workspace, genesis) =
                verify_genesis(&decode_genesis(&raw)?, self.verifier.as_ref())?;
            if id != workspace.0 {
                return Err(StoreError::Database);
            }
            self.seed_membership(workspace, genesis.owner)?;
            let mut query = self.conn.prepare(
                "SELECT seq,signed_blob FROM events WHERE workspace=?1 AND author=?2 ORDER BY seq",
            )?;
            let owner_rows = query.query_map(
                params![workspace.0.as_slice(), genesis.owner.0.as_slice()],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )?;
            let mut expected = 1u64;
            for row in owner_rows {
                let (seq, raw) = row?;
                let event = verify_and_decode(&decode_signed_event(&raw)?, self.verifier.as_ref())?;
                event.validate_received()?;
                if event.workspace != workspace
                    || event.author != genesis.owner
                    || event.seq != expected
                    || sql_seq(event.seq)? != seq
                {
                    return Err(StoreError::Database);
                }
                self.apply_contiguous_membership(&event)?;
                expected = expected.checked_add(1).ok_or(StoreError::SequenceRange)?;
            }
        }
        transaction.commit()?;
        Ok(())
    }
    fn owner(&self, workspace: WorkspaceId) -> Result<DeviceId, StoreError> {
        let bytes: Vec<u8> = self
            .conn
            .query_row(
                "SELECT owner FROM membership_state WHERE workspace=?1",
                [workspace.0.as_slice()],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(StoreError::UnknownWorkspace)?;
        Ok(DeviceId(
            bytes.try_into().map_err(|_| StoreError::Database)?,
        ))
    }
    // Called only for newly accepted contiguous events (or once during upgrade),
    // inside the same transaction as the event row and any gap promotions.
    fn apply_contiguous_membership(&self, event: &VerifiedEvent) -> Result<(), StoreError> {
        let owner = self.owner(event.workspace)?;
        if matches!(event.body, EventBody::MemberRemoved { device } if device == owner) {
            self.flag(event.workspace, event.author, false)?;
        }
        if event.author != owner {
            return Ok(());
        }
        let (device, removed) = match event.body {
            EventBody::MemberAdded { device, .. } if device != owner => (device, false),
            EventBody::MemberRemoved { device } if device != owner => (device, true),
            _ => return Ok(()),
        };
        let epoch = self
            .current_epoch(event.workspace)?
            .checked_add(1)
            .ok_or(StoreError::SequenceRange)?;
        self.conn.execute(
            "UPDATE membership_state SET epoch=?2 WHERE workspace=?1",
            params![event.workspace.0.as_slice(), epoch],
        )?;
        if removed {
            // A removal alone never establishes the ever-confirmed quota exemption.
            self.conn.execute(
                "UPDATE members SET removed=1 WHERE workspace=?1 AND device=?2",
                params![event.workspace.0.as_slice(), device.0.as_slice()],
            )?;
        } else {
            self.conn.execute("INSERT INTO members(workspace,device,epoch_added,removed) VALUES(?1,?2,?3,0) ON CONFLICT(workspace,device) DO UPDATE SET removed=0",
                params![event.workspace.0.as_slice(), device.0.as_slice(), epoch])?;
        }
        Ok(())
    }
    // Upgrade existing T-002 databases as well: an old oversized unconfirmed log
    // must not survive reopen and become an unbounded projection input.
    fn discard_oversized_unconfirmed(&self) -> Result<(), StoreError> {
        let transaction = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let oversized = {
            let mut query = self.conn.prepare(
                "SELECT workspace,author FROM (SELECT workspace,author,signed_blob FROM events UNION ALL SELECT workspace,author,signed_blob FROM pending_events) GROUP BY workspace,author HAVING COUNT(*)>?1 OR SUM(length(signed_blob))>?2",
            )?;
            let rows = query.query_map(
                params![PENDING_EVENTS as i64, PENDING_BYTES as i64],
                |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        for (workspace, author) in oversized {
            let workspace = WorkspaceId(workspace.try_into().map_err(|_| StoreError::Database)?);
            let author = DeviceId(author.try_into().map_err(|_| StoreError::Database)?);
            if !self.confirmed_author(workspace, author)? {
                for table in ["events", "pending_events"] {
                    self.conn.execute(
                        &format!("DELETE FROM {table} WHERE workspace=?1 AND author=?2"),
                        params![workspace.0.as_slice(), author.0.as_slice()],
                    )?;
                }
            }
        }
        transaction.commit()?;
        Ok(())
    }
    pub fn add_workspace(&mut self, raw: &[u8]) -> Result<WorkspaceId, StoreError> {
        let signed = decode_genesis(raw)?;
        let (id, genesis) = verify_genesis(&signed, self.verifier.as_ref())?;
        if let Some(old) = self
            .conn
            .query_row(
                "SELECT genesis_blob FROM workspaces WHERE id=?1",
                [id.0.as_slice()],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()?
        {
            if old != raw {
                return Err(StoreError::GenesisConflict);
            }
            return Ok(id);
        }
        let transaction = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        self.conn.execute(
            "INSERT INTO workspaces VALUES (?1,?2)",
            params![id.0.as_slice(), raw],
        )?;
        self.seed_membership(id, genesis.owner)?;
        transaction.commit()?;
        Ok(id)
    }
    pub fn genesis(&self, workspace: WorkspaceId) -> Result<Vec<u8>, StoreError> {
        self.conn
            .query_row(
                "SELECT genesis_blob FROM workspaces WHERE id=?1",
                [workspace.0.as_slice()],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(StoreError::UnknownWorkspace)
    }
    fn check_workspace(&self, workspace: WorkspaceId) -> Result<(), StoreError> {
        self.genesis(workspace).map(|_| ())
    }
    fn head(&self, workspace: WorkspaceId, author: DeviceId) -> Result<u64, StoreError> {
        let value: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(seq),0) FROM events WHERE workspace=?1 AND author=?2",
            params![workspace.0.as_slice(), author.0.as_slice()],
            |row| row.get(0),
        )?;
        u64::try_from(value).map_err(|_| StoreError::SequenceRange)
    }
    fn flag(
        &self,
        workspace: WorkspaceId,
        author: DeviceId,
        equivocated: bool,
    ) -> Result<(), StoreError> {
        self.conn.execute("INSERT INTO peers(workspace,device,equivocated,protocol_violation) VALUES(?1,?2,?3,?4) ON CONFLICT(workspace,device) DO UPDATE SET equivocated=MAX(equivocated,excluded.equivocated),protocol_violation=MAX(protocol_violation,excluded.protocol_violation)", params![workspace.0.as_slice(),author.0.as_slice(),equivocated,!equivocated])?;
        Ok(())
    }
    pub fn diagnostics(
        &self,
        workspace: WorkspaceId,
        author: DeviceId,
    ) -> Result<Diagnostics, StoreError> {
        self.check_workspace(workspace)?;
        Ok(self
            .conn
            .query_row(
                "SELECT equivocated,protocol_violation FROM peers WHERE workspace=?1 AND device=?2",
                params![workspace.0.as_slice(), author.0.as_slice()],
                |row| {
                    Ok(Diagnostics {
                        equivocated: row.get(0)?,
                        protocol_violation: row.get(1)?,
                    })
                },
            )
            .optional()?
            .unwrap_or_default())
    }
    pub fn insert_remote(&mut self, raw: &[u8]) -> Result<InsertOutcome, StoreError> {
        let transaction = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let result = self.insert(raw)?;
        transaction.commit()?;
        Ok(result)
    }

    fn insert(&self, raw: &[u8]) -> Result<InsertOutcome, StoreError> {
        let signed = match decode_signed_event(raw) {
            Ok(value) => value,
            Err(_) => return Ok(InsertOutcome::Rejected(RejectReason::Malformed)),
        };
        let event = match verify_and_decode(&signed, self.verifier.as_ref()) {
            Ok(event) => event,
            Err(error) => {
                // A verified malformed UTF-8 body still identifies the violating author.
                if error == CryptoError::InvalidField
                    && let Ok((header, _)) =
                        postcard::take_from_bytes::<EventHeaderV1>(&signed.signed)
                {
                    let workspace = WorkspaceId(header.workspace);
                    if self.check_workspace(workspace).is_ok() {
                        self.flag(workspace, DeviceId(header.author), false)?;
                    }
                }
                return Ok(InsertOutcome::Rejected(match error {
                    CryptoError::Signature => RejectReason::InvalidSignature,
                    CryptoError::InvalidField => RejectReason::InvalidField,
                    _ => RejectReason::Malformed,
                }));
            }
        };
        match self.check_workspace(event.workspace) {
            Ok(()) => {}
            Err(StoreError::UnknownWorkspace) => {
                return Ok(InsertOutcome::Rejected(RejectReason::UnknownWorkspace));
            }
            Err(error) => return Err(error),
        }
        let seq = match sql_seq(event.seq) {
            Ok(value) if value > 0 => value,
            _ => return Ok(InsertOutcome::Rejected(RejectReason::SequenceRange)),
        };
        let w = event.workspace.0.as_slice();
        let a = event.author.0.as_slice();
        let old: Option<Vec<u8>> = self.conn.query_row("SELECT signed_blob FROM events WHERE workspace=?1 AND author=?2 AND seq=?3 UNION ALL SELECT signed_blob FROM pending_events WHERE workspace=?1 AND author=?2 AND seq=?3 LIMIT 1", params![w,a,seq], |row| row.get(0)).optional()?;
        // Dedupe by signed bytes, NOT by signature or decoded/re-encoded body.
        if let Some(old) = old {
            let previous = decode_signed_event(&old)?;
            if previous.signed == signed.signed {
                return Ok(InsertOutcome::Duplicate);
            }
            self.flag(event.workspace, event.author, true)?;
            return Ok(InsertOutcome::Equivocation);
        }
        if event.validate_received().is_err() {
            self.flag(event.workspace, event.author, false)?;
            return Ok(InsertOutcome::Rejected(RejectReason::InvalidField));
        }
        let head = self.head(event.workspace, event.author)?;
        let gapped = event.seq != head.checked_add(1).ok_or(StoreError::SequenceRange)?;
        // Ever-confirmed authors keep their history, including after reserved M2
        // MemberRemoved. Authority comes only from verified contiguous owner data.
        let confirmed = self.confirmed_author(event.workspace, event.author)?;
        if !confirmed || gapped {
            let (count, bytes): (i64, i64) = self.conn.query_row(
                "SELECT COUNT(*),COALESCE(SUM(length(signed_blob)),0) FROM (SELECT signed_blob FROM pending_events WHERE workspace=?1 AND author=?2 UNION ALL SELECT signed_blob FROM events WHERE workspace=?1 AND author=?2 AND ?3)",
                params![w, a, !confirmed],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            let added = i64::try_from(raw.len()).map_err(|_| StoreError::Limit)?;
            if count >= PENDING_EVENTS as i64
                || bytes
                    .checked_add(added)
                    .is_none_or(|size| size > PENDING_BYTES as i64)
            {
                self.conn.execute(
                    "DELETE FROM pending_events WHERE workspace=?1 AND author=?2",
                    params![w, a],
                )?;
                if !confirmed {
                    self.conn.execute(
                        "DELETE FROM events WHERE workspace=?1 AND author=?2",
                        params![w, a],
                    )?;
                }
                return Ok(InsertOutcome::Rejected(RejectReason::PendingOverflow));
            }
        }
        if gapped {
            if matches!(event.body, EventBody::MemberRemoved { device } if device == self.owner(event.workspace)?)
            {
                self.flag(event.workspace, event.author, false)?;
            }
            self.conn.execute(
                "INSERT INTO pending_events VALUES(?1,?2,?3,?4)",
                params![w, a, seq, raw],
            )?;
            return Ok(InsertOutcome::Pending);
        }
        self.conn.execute(
            "INSERT INTO events VALUES(?1,?2,?3,?4)",
            params![w, a, seq, raw],
        )?;
        self.apply_contiguous_membership(&event)?;
        let mut next = seq;
        while let Some(candidate) = next.checked_add(1) {
            let pending: Option<Vec<u8>> = self.conn.query_row(
                "SELECT signed_blob FROM pending_events WHERE workspace=?1 AND author=?2 AND seq=?3",
                params![w,a,candidate], |row| row.get(0),
            ).optional()?;
            let Some(raw) = pending else {
                break;
            };
            // Only this newly promoted event is reverified; never the old log.
            let promoted = verify_and_decode(&decode_signed_event(&raw)?, self.verifier.as_ref())?;
            promoted.validate_received()?;
            if promoted.workspace != event.workspace
                || promoted.author != event.author
                || sql_seq(promoted.seq)? != candidate
            {
                return Err(StoreError::Database);
            }
            self.conn.execute(
                "INSERT INTO events VALUES(?1,?2,?3,?4)",
                params![w, a, candidate, raw],
            )?;
            self.apply_contiguous_membership(&promoted)?;
            self.conn.execute(
                "DELETE FROM pending_events WHERE workspace=?1 AND author=?2 AND seq=?3",
                params![w, a, candidate],
            )?;
            next = candidate;
        }
        Ok(InsertOutcome::Inserted)
    }
    fn confirmed_author(
        &self,
        workspace: WorkspaceId,
        author: DeviceId,
    ) -> Result<bool, StoreError> {
        Ok(self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM members WHERE workspace=?1 AND device=?2)",
            params![workspace.0.as_slice(), author.0.as_slice()],
            |row| row.get(0),
        )?)
    }
    pub fn append_own(
        &mut self,
        workspace: WorkspaceId,
        body: EventBody,
        signer: &dyn Signer,
        policy: SharingPolicy,
        ts_ms: u64,
        epoch: u32,
    ) -> Result<SignedEventV1, StoreError> {
        self.check_workspace(workspace)?;
        let transaction = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let result: Result<_, StoreError> = (|| {
            let seq = self
                .head(workspace, signer.device_id())?
                .checked_add(1)
                .ok_or(StoreError::SequenceRange)?;
            sql_seq(seq)?;
            let signed = sign_event(
                Event {
                    workspace,
                    author: signer.device_id(),
                    seq,
                    ts_ms,
                    membership_epoch: epoch,
                    body,
                },
                signer,
                &policy,
            )?;
            let raw = postcard::to_stdvec(&signed).map_err(|_| CryptoError::Encoding)?;
            Ok((signed, self.insert(&raw)?))
        })();
        let (value, outcome) = result?;
        // Quota overflow deliberately commits the held-data drop, even when an
        // own append cannot return a successfully persisted event.
        transaction.commit()?;
        if outcome != InsertOutcome::Inserted {
            return Err(StoreError::AppendRejected);
        }
        Ok(value)
    }
    pub fn current_epoch(&self, workspace: WorkspaceId) -> Result<u32, StoreError> {
        let epoch: i64 = self
            .conn
            .query_row(
                "SELECT epoch FROM membership_state WHERE workspace=?1",
                [workspace.0.as_slice()],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(StoreError::UnknownWorkspace)?;
        u32::try_from(epoch).map_err(|_| StoreError::SequenceRange)
    }
    pub fn heads(&self, workspace: WorkspaceId) -> Result<BTreeMap<DeviceId, u64>, StoreError> {
        self.check_workspace(workspace)?;
        let mut query = self.conn.prepare("SELECT author,MAX(seq) FROM events WHERE workspace=?1 GROUP BY author UNION ALL SELECT DISTINCT author,0 FROM pending_events WHERE workspace=?1 AND author NOT IN (SELECT author FROM events WHERE workspace=?1)")?;
        let rows = query.query_map([workspace.0.as_slice()], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?))
        })?;
        let mut heads = BTreeMap::new();
        for row in rows {
            let (author, seq) = row?;
            heads.insert(
                DeviceId(author.try_into().map_err(|_| StoreError::Database)?),
                u64::try_from(seq).map_err(|_| StoreError::SequenceRange)?,
            );
        }
        Ok(heads)
    }
    pub fn events_after(
        &self,
        workspace: WorkspaceId,
        author: DeviceId,
        seq: u64,
        limit: usize,
    ) -> Result<Vec<Vec<u8>>, StoreError> {
        self.check_workspace(workspace)?;
        if !(1..=256).contains(&limit) {
            return Err(StoreError::Limit);
        }
        let seq = sql_seq(seq)?;
        let mut query = self.conn.prepare("SELECT signed_blob FROM events WHERE workspace=?1 AND author=?2 AND seq>?3 ORDER BY seq LIMIT ?4")?;
        let rows = query.query_map(
            params![
                workspace.0.as_slice(),
                author.0.as_slice(),
                seq,
                i64::try_from(limit).map_err(|_| StoreError::Limit)?
            ],
            |row| row.get::<_, Vec<u8>>(0),
        )?;
        let mut bytes = 0usize;
        let mut result = vec![];
        for row in rows {
            let raw = row?;
            bytes += raw.len();
            if bytes > MAX_FRAME {
                break;
            }
            result.push(raw);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests;
