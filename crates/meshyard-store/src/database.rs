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
}
fn migrations() -> Migrations<'static> {
    Migrations::new(vec![M::up("
        CREATE TABLE workspaces (id BLOB PRIMARY KEY CHECK(length(id)=16), genesis_blob BLOB NOT NULL);
        CREATE TABLE events (workspace BLOB NOT NULL REFERENCES workspaces(id), author BLOB NOT NULL CHECK(length(author)=32), seq INTEGER NOT NULL CHECK(seq>0), signed_blob BLOB NOT NULL, PRIMARY KEY(workspace,author,seq));
        CREATE TABLE pending_events (workspace BLOB NOT NULL REFERENCES workspaces(id), author BLOB NOT NULL CHECK(length(author)=32), seq INTEGER NOT NULL CHECK(seq>0), signed_blob BLOB NOT NULL, PRIMARY KEY(workspace,author,seq));
        CREATE TABLE peers (workspace BLOB NOT NULL REFERENCES workspaces(id), device BLOB NOT NULL, equivocated INTEGER NOT NULL DEFAULT 0, protocol_violation INTEGER NOT NULL DEFAULT 0, PRIMARY KEY(workspace,device));
        CREATE TABLE pairings (workspace BLOB NOT NULL REFERENCES workspaces(id), invite_id BLOB NOT NULL, token_hash BLOB NOT NULL, state TEXT NOT NULL CHECK(state IN ('Open','Reserved','Completed','Aborted','Expired')), expires_ms INTEGER NOT NULL, joiner BLOB, joiner_nonce BLOB, inviter_nonce BLOB, reserved_ms INTEGER, inviter_confirmed INTEGER NOT NULL DEFAULT 0, joiner_confirmed INTEGER NOT NULL DEFAULT 0, transcript_hash BLOB, PRIMARY KEY(workspace,invite_id));
        CREATE TABLE meta (key TEXT PRIMARY KEY, value BLOB NOT NULL);
    ")])
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
        let db = Self { conn };
        db.discard_oversized_unconfirmed()?;
        Ok(db)
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
        let (id, _) = verify_genesis(&signed, &Ed25519Verifier)?;
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
        self.conn.execute(
            "INSERT INTO workspaces VALUES (?1,?2)",
            params![id.0.as_slice(), raw],
        )?;
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
        let event = match verify_and_decode(&signed, &Ed25519Verifier) {
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
        if let EventBody::MemberRemoved { device } = event.body {
            let (_, genesis) = verify_genesis(
                &decode_genesis(&self.genesis(event.workspace)?)?,
                &Ed25519Verifier,
            )?;
            if device == genesis.owner {
                self.flag(event.workspace, event.author, false)?;
            }
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
        let mut next = seq;
        while let Some(candidate) = next.checked_add(1) {
            let changed = self.conn.execute("INSERT INTO events SELECT * FROM pending_events WHERE workspace=?1 AND author=?2 AND seq=?3",params![w,a,candidate])?;
            if changed == 0 {
                break;
            }
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
        let (_, genesis) = verify_genesis(
            &decode_genesis(&self.genesis(workspace)?)?,
            &Ed25519Verifier,
        )?;
        if author == genesis.owner {
            return Ok(true);
        }
        let mut query = self.conn.prepare(
            "SELECT signed_blob FROM events WHERE workspace=?1 AND author=?2 ORDER BY seq",
        )?;
        let rows = query.query_map(
            params![workspace.0.as_slice(), genesis.owner.0.as_slice()],
            |row| row.get::<_, Vec<u8>>(0),
        )?;
        let mut expected = 1u64;
        for row in rows {
            let event = verify_and_decode(&decode_signed_event(&row?)?, &Ed25519Verifier)?;
            event.validate_received()?;
            if event.workspace != workspace
                || event.author != genesis.owner
                || event.seq != expected
            {
                return Err(StoreError::Database);
            }
            if matches!(event.body, EventBody::MemberAdded { device, .. } if device == author) {
                return Ok(true);
            }
            expected = expected.checked_add(1).ok_or(StoreError::SequenceRange)?;
        }
        Ok(false)
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
        let raw = self.genesis(workspace)?;
        let (_, genesis) = verify_genesis(&decode_genesis(&raw)?, &Ed25519Verifier)?;
        let mut query = self.conn.prepare(
            "SELECT signed_blob FROM events WHERE workspace=?1 AND author=?2 ORDER BY seq",
        )?;
        let rows = query.query_map(
            params![workspace.0.as_slice(), genesis.owner.0.as_slice()],
            |row| row.get::<_, Vec<u8>>(0),
        )?;
        let mut events = vec![];
        for row in rows {
            events.push(
                verify_and_decode(&decode_signed_event(&row?)?, &Ed25519Verifier)?.into_event(),
            );
        }
        fold(&genesis, &events)
            .map(|state| state.epoch)
            .map_err(|_| StoreError::SequenceRange)
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
