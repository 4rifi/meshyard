use super::*;
use crate::pairing::{Pairing, Reservation};

impl Database {
    pub fn pairing_deadline(&self, workspace: WorkspaceId, now: u64) -> Result<u64, StoreError> {
        self.expire_pairings(workspace, now)?;
        let deadline:Option<i64>=self.conn.query_row("SELECT MAX(CASE state WHEN 'Open' THEN expires_ms WHEN 'Reserved' THEN reserved_ms+300000 END) FROM pairings WHERE workspace=?1 AND state IN ('Open','Reserved')",[workspace.0.as_slice()],|r|r.get(0))?;
        deadline
            .map(u64::try_from)
            .transpose()
            .map_err(|_| StoreError::Database)
            .map(|d| d.unwrap_or_default())
    }

    pub fn endpoint_port(&self) -> Result<Option<u16>, StoreError> {
        let bytes: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key='network_udp_port'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        bytes.map(|b| Ok(u16::from_be_bytes(array(b)?))).transpose()
    }
    pub fn save_endpoint_port(&self, port: u16) -> Result<(), StoreError> {
        if port == 0 {
            return Err(StoreError::Pairing);
        }
        self.conn.execute("INSERT INTO meta(key,value) VALUES('network_udp_port',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[port.to_be_bytes().as_slice()])?;
        Ok(())
    }

    pub fn save_peer_address(
        &self,
        workspace: WorkspaceId,
        peer: DeviceId,
        address: Vec<u8>,
    ) -> Result<(), StoreError> {
        if address.len() > 16384 {
            return Err(StoreError::Limit);
        }
        let prefix = format!("address:{}:", hex(&workspace.0));
        let key = format!("{}{}", prefix, hex(&peer.0));
        self.conn.execute("INSERT INTO meta(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,address])?;
        Ok(())
    }
    pub fn peer_addresses(&self, workspace: WorkspaceId) -> Result<Vec<Vec<u8>>, StoreError> {
        let pattern = format!("address:{}:%", hex(&workspace.0));
        let mut query = self
            .conn
            .prepare("SELECT value FROM meta WHERE key LIKE ?1 LIMIT 256")?;
        Ok(query
            .query_map([pattern], |r| r.get(0))?
            .collect::<Result<_, _>>()?)
    }

    pub fn join_nonce(
        &self,
        workspace: WorkspaceId,
        id: [u8; 16],
        inviter: DeviceId,
        nonce: [u8; 16],
        now: u64,
    ) -> Result<[u8; 16], StoreError> {
        self.conn.execute("INSERT OR IGNORE INTO join_attempts(workspace,invite_id,inviter,nonce,started_ms) VALUES(?1,?2,?3,?4,?5)",params![workspace.0.as_slice(),id.as_slice(),inviter.0.as_slice(),nonce.as_slice(),sql_seq(now)?])?;
        let (owner,nonce,started):(Vec<u8>,Vec<u8>,i64)=self.conn.query_row("SELECT inviter,nonce,started_ms FROM join_attempts WHERE workspace=?1 AND invite_id=?2",params![workspace.0.as_slice(),id.as_slice()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        if owner != inviter.0
            || now.saturating_sub(u64::try_from(started).map_err(|_| StoreError::Database)?)
                >= 300_000
        {
            return Err(StoreError::Pairing);
        }
        array(nonce)
    }

    pub fn create_invite(
        &self,
        workspace: WorkspaceId,
        id: [u8; 16],
        hash: [u8; 32],
        now: u64,
        signer: &dyn Signer,
    ) -> Result<(), StoreError> {
        let genesis = self
            .genesis_cache
            .get(&workspace)
            .ok_or(StoreError::UnknownWorkspace)?;
        if genesis.owner != signer.device_id() || genesis.mode != Mode::Strict {
            return Err(StoreError::Pairing);
        }
        let expires = now.checked_add(900_000).ok_or(StoreError::SequenceRange)?;
        self.expire_pairings(workspace, now)?;
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM pairings WHERE workspace=?1 AND state IN ('Open','Reserved')",
            [workspace.0.as_slice()],
            |r| r.get(0),
        )?;
        if count >= 16 {
            return Err(StoreError::PairingRate);
        }
        self.conn.execute("INSERT INTO pairings(workspace,invite_id,token_hash,state,expires_ms) VALUES(?1,?2,?3,'Open',?4)", params![workspace.0.as_slice(), id.as_slice(), hash.as_slice(), sql_seq(expires)?])?;
        Ok(())
    }
    fn expire_pairings(&self, workspace: WorkspaceId, now: u64) -> Result<(), StoreError> {
        let now = sql_seq(now)?;
        self.conn.execute("UPDATE pairings SET state='Expired' WHERE workspace=?1 AND state='Open' AND expires_ms<=?2", params![workspace.0.as_slice(), now])?;
        self.conn.execute("UPDATE pairings SET state='Aborted' WHERE workspace=?1 AND state='Reserved' AND reserved_ms<=?2", params![workspace.0.as_slice(), now.saturating_sub(300_000)])?;
        Ok(())
    }
    pub fn pairings(&self, workspace: WorkspaceId, now: u64) -> Result<Vec<Pairing>, StoreError> {
        self.query_pairings(workspace, now, None)
    }
    fn query_pairings(
        &self,
        workspace: WorkspaceId,
        now: u64,
        id: Option<[u8; 16]>,
    ) -> Result<Vec<Pairing>, StoreError> {
        self.expire_pairings(workspace, now)?;
        let mut query = self.conn.prepare("SELECT invite_id,state,expires_ms,joiner,joiner_nonce,inviter_nonce,transcript_hash,inviter_confirmed,joiner_confirmed,reserved_ms FROM pairings WHERE workspace=?1 AND (?2 IS NULL OR invite_id=?2) ORDER BY rowid DESC LIMIT 64")?;
        let rows = query.query_map(
            params![workspace.0.as_slice(), id.as_ref().map(|b| b.as_slice())],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<Vec<u8>>>(3)?,
                    row.get::<_, Option<Vec<u8>>>(4)?,
                    row.get::<_, Option<Vec<u8>>>(5)?,
                    row.get::<_, Option<Vec<u8>>>(6)?,
                    row.get::<_, bool>(7)?,
                    row.get::<_, bool>(8)?,
                    row.get::<_, Option<i64>>(9)?,
                ))
            },
        )?;
        rows.map(|row| {
            let (id, state, expires_ms, joiner, jn, invn, hash, ic, jc, reserved) = row?;
            Ok(Pairing {
                invite_id: array(id)?,
                reserved_ms: reserved
                    .map(u64::try_from)
                    .transpose()
                    .map_err(|_| StoreError::Database)?,
                state,
                expires_ms: u64::try_from(expires_ms).map_err(|_| StoreError::Database)?,
                joiner: joiner.map(array).transpose()?.map(DeviceId),
                joiner_nonce: jn.map(array).transpose()?,
                inviter_nonce: invn.map(array).transpose()?,
                transcript_hash: hash.map(array).transpose()?,
                inviter_confirmed: ic,
                joiner_confirmed: jc,
            })
        })
        .collect()
    }
    fn pairing(
        &self,
        workspace: WorkspaceId,
        id: [u8; 16],
        now: u64,
    ) -> Result<Pairing, StoreError> {
        self.query_pairings(workspace, now, Some(id))?
            .into_iter()
            .find(|p| p.invite_id == id)
            .ok_or(StoreError::Pairing)
    }
    pub fn reserve_invite(&self, r: Reservation) -> Result<Pairing, StoreError> {
        validate_received(&r.display_name, 64).map_err(|_| StoreError::Pairing)?;
        let genesis = self
            .genesis_cache
            .get(&r.workspace)
            .ok_or(StoreError::UnknownWorkspace)?;
        if r.joiner == genesis.owner {
            return Err(StoreError::Pairing);
        }
        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let state = self.pairing(r.workspace, r.invite_id, r.now_ms)?;
        let (expected, window, mut attempts): (Vec<u8>, i64, u32) = self.conn.query_row("SELECT token_hash,attempt_window_ms,attempts FROM pairings WHERE workspace=?1 AND invite_id=?2", params![r.workspace.0.as_slice(),r.invite_id.as_slice()], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)))?;
        let window = u64::try_from(window).map_err(|_| StoreError::Database)?;
        let window = if r.now_ms.saturating_sub(window) >= 60_000 {
            attempts = 0;
            r.now_ms
        } else {
            window
        };
        if attempts >= 10 {
            tx.commit()?;
            return Err(StoreError::PairingRate);
        }
        self.conn.execute("UPDATE pairings SET attempts=?3,attempt_window_ms=?4 WHERE workspace=?1 AND invite_id=?2", params![r.workspace.0.as_slice(),r.invite_id.as_slice(), attempts+1,sql_seq(window)?])?;
        let expected: [u8; 32] = array(expected)?;
        // The fixed-length primitive guarantees data-independent comparison.
        let matches = constant_time_eq::constant_time_eq_32(&expected, &r.token_hash);
        let valid = matches
            && (state.state == "Open"
                || ((state.state == "Reserved"
                    || (state.state == "Completed"
                        && state
                            .reserved_ms
                            .is_some_and(|t| r.now_ms.saturating_sub(t) < 300_000)))
                    && state.joiner == Some(r.joiner)
                    && state.joiner_nonce == Some(r.joiner_nonce)));
        if !valid {
            if matches
                && state.state == "Reserved"
                && state.joiner == Some(r.joiner)
                && state.joiner_nonce != Some(r.joiner_nonce)
            {
                self.abort_pairing(r.workspace, r.invite_id)?;
            }
            tx.commit()?;
            return Err(StoreError::Pairing);
        }
        if state.state == "Open" {
            let hash = meshyard_core::pairing::Transcript {
                workspace: r.workspace,
                mode: genesis.mode,
                protocol_version: 1,
                inviter: genesis.owner,
                joiner: r.joiner,
                invite_id: r.invite_id,
                inviter_nonce: r.inviter_nonce,
                joiner_nonce: r.joiner_nonce,
            }
            .hash();
            self.conn.execute("UPDATE pairings SET state='Reserved',joiner=?3,joiner_nonce=?4,inviter_nonce=?5,reserved_ms=?6,transcript_hash=?7,joiner_name=?8 WHERE workspace=?1 AND invite_id=?2 AND state='Open'", params![r.workspace.0.as_slice(),r.invite_id.as_slice(),r.joiner.0.as_slice(),r.joiner_nonce.as_slice(),r.inviter_nonce.as_slice(),sql_seq(r.now_ms)?,hash.as_slice(),r.display_name])?;
        }
        tx.commit()?;
        self.pairing(r.workspace, r.invite_id, r.now_ms)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn confirm_pairing(
        &mut self,
        workspace: WorkspaceId,
        id: [u8; 16],
        peer: DeviceId,
        hash: [u8; 32],
        local: bool,
        now: u64,
        signer: &dyn Signer,
    ) -> Result<Pairing, StoreError> {
        let genesis = self
            .genesis_cache
            .get(&workspace)
            .ok_or(StoreError::UnknownWorkspace)?;
        if genesis.owner != signer.device_id() {
            return Err(StoreError::Pairing);
        }
        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let state = self.pairing(workspace, id, now)?;
        if state.state != "Reserved" || state.joiner != Some(peer) {
            return Err(StoreError::Pairing);
        }
        if state.transcript_hash != Some(hash) {
            self.abort_pairing(workspace, id)?;
            tx.commit()?;
            return Err(StoreError::Pairing);
        }
        let sql = if local {
            "UPDATE pairings SET inviter_confirmed=1 WHERE workspace=?1 AND invite_id=?2"
        } else {
            "UPDATE pairings SET joiner_confirmed=1 WHERE workspace=?1 AND invite_id=?2"
        };
        self.conn
            .execute(sql, params![workspace.0.as_slice(), id.as_slice()])?;
        let state = self.pairing(workspace, id, now)?;
        if state.inviter_confirmed && state.joiner_confirmed {
            let display_name: String = self.conn.query_row(
                "SELECT joiner_name FROM pairings WHERE workspace=?1 AND invite_id=?2",
                params![workspace.0.as_slice(), id.as_slice()],
                |r| r.get(0),
            )?;
            let seq = self
                .head(workspace, signer.device_id())?
                .checked_add(1)
                .ok_or(StoreError::SequenceRange)?;
            let signed = sign_event(
                Event {
                    workspace,
                    author: signer.device_id(),
                    seq,
                    ts_ms: now,
                    membership_epoch: self.current_epoch(workspace)?,
                    body: EventBody::MemberAdded {
                        device: peer,
                        display_name,
                    },
                },
                signer,
                &SharingPolicy::default(),
            )?;
            if self.insert(&postcard::to_stdvec(&signed).map_err(|_| CryptoError::Encoding)?)?
                != InsertOutcome::Inserted
            {
                return Err(StoreError::AppendRejected);
            }
            self.conn.execute(
                "UPDATE pairings SET state='Completed' WHERE workspace=?1 AND invite_id=?2",
                params![workspace.0.as_slice(), id.as_slice()],
            )?;
        }
        tx.commit()?;
        self.pairing(workspace, id, now)
    }
    pub fn abort_pairing(&self, workspace: WorkspaceId, id: [u8; 16]) -> Result<(), StoreError> {
        self.conn.execute("UPDATE pairings SET state='Aborted' WHERE workspace=?1 AND invite_id=?2 AND state IN ('Open','Reserved')", params![workspace.0.as_slice(),id.as_slice()])?;
        Ok(())
    }
}
fn array<const N: usize>(bytes: Vec<u8>) -> Result<[u8; N], StoreError> {
    bytes.try_into().map_err(|_| StoreError::Database)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

impl Database {
    pub fn completed_pairing_resumes(
        &self,
        w: WorkspaceId,
        now: u64,
    ) -> Result<Vec<(DeviceId, u64)>, StoreError> {
        let mut q=self.conn.prepare("SELECT joiner,reserved_ms FROM pairings WHERE workspace=?1 AND state='Completed' AND reserved_ms>?2")?;
        let rows = q.query_map(
            params![w.0.as_slice(), sql_seq(now.saturating_sub(300_000))?],
            |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, i64>(1)?)),
        )?;
        rows.map(|r| {
            let (device, time) = r?;
            Ok((
                DeviceId(array(device)?),
                u64::try_from(time).map_err(|_| StoreError::Database)? + 300_000,
            ))
        })
        .collect()
    }
    pub fn confirm_join_attempt(
        &self,
        w: WorkspaceId,
        id: [u8; 16],
        device: DeviceId,
        hash: [u8; 32],
    ) -> Result<(), StoreError> {
        let count=self.conn.execute("UPDATE join_attempts SET device=?3,confirmed_hash=?4 WHERE workspace=?1 AND invite_id=?2 AND (device IS NULL OR device=?3)",params![w.0.as_slice(),id.as_slice(),device.0.as_slice(),hash.as_slice()])?;
        if count != 1 {
            return Err(StoreError::Pairing);
        }
        Ok(())
    }
    pub fn persist_pending_join(
        &self,
        w: WorkspaceId,
        g: &Genesis,
        p: crate::pairing::PendingJoin,
    ) -> Result<(), StoreError> {
        if p.workspace != w
            || p.owner != g.owner
            || p.device == g.owner
            || p.owner_address.len() > 16 * 1024
        {
            return Err(StoreError::Pairing);
        }
        validate_received(&p.display_name, 64).map_err(|_| StoreError::Pairing)?;
        let valid:bool=self.conn.query_row("SELECT EXISTS(SELECT 1 FROM join_attempts WHERE workspace=?1 AND invite_id=?2 AND inviter=?3 AND device=?4 AND confirmed_hash=?5)",params![w.0.as_slice(),p.invite_id.as_slice(),p.owner.0.as_slice(),p.device.0.as_slice(),p.transcript_hash.as_slice()],|r|r.get(0))?;
        if !valid {
            return Err(StoreError::Pairing);
        }
        self.conn.execute(
            "INSERT INTO pending_joins VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![
                w.0.as_slice(),
                p.device.0.as_slice(),
                p.owner.0.as_slice(),
                p.owner_address,
                p.display_name,
                p.invite_id.as_slice(),
                p.transcript_hash.as_slice()
            ],
        )?;
        Ok(())
    }
    pub fn pending_join(
        &self,
        w: WorkspaceId,
        device: DeviceId,
    ) -> Result<Option<crate::pairing::PendingJoin>, StoreError> {
        let row=self.conn.query_row("SELECT p.owner,p.owner_address,p.display_name,p.invite_id,p.transcript_hash FROM pending_joins p JOIN join_attempts j ON j.workspace=p.workspace AND j.invite_id=p.invite_id AND j.device=p.device AND j.inviter=p.owner AND j.confirmed_hash=p.transcript_hash WHERE p.workspace=?1 AND p.device=?2",params![w.0.as_slice(),device.0.as_slice()],|r|Ok((r.get::<_,Vec<u8>>(0)?,r.get::<_,Vec<u8>>(1)?,r.get::<_,String>(2)?,r.get::<_,Vec<u8>>(3)?,r.get::<_,Vec<u8>>(4)?))).optional()?;
        row.map(|(owner, owner_address, display_name, id, hash)| {
            Ok(crate::pairing::PendingJoin {
                workspace: w,
                device,
                owner: DeviceId(array(owner)?),
                owner_address,
                display_name,
                invite_id: array(id)?,
                transcript_hash: array(hash)?,
            })
        })
        .transpose()
    }
    pub fn finish_join(
        &self,
        w: WorkspaceId,
        signer: &dyn Signer,
        now: u64,
    ) -> Result<bool, StoreError> {
        let Some(p) = self.pending_join(w, signer.device_id())? else {
            return Ok(false);
        };
        if !self
            .workspace_snapshot(w)?
            .members
            .iter()
            .any(|m| m.device == signer.device_id())
        {
            return Ok(false);
        }
        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let seq = self
            .head(w, signer.device_id())?
            .checked_add(1)
            .ok_or(StoreError::SequenceRange)?;
        let event = sign_event(
            Event {
                workspace: w,
                author: signer.device_id(),
                seq,
                ts_ms: now,
                membership_epoch: self.current_epoch(w)?,
                body: EventBody::ProfileUpdated {
                    display_name: p.display_name,
                    avatar: 0,
                },
            },
            signer,
            &SharingPolicy::default(),
        )?;
        if self.insert(&postcard::to_stdvec(&event).map_err(|_| CryptoError::Encoding)?)?
            != InsertOutcome::Inserted
        {
            return Err(StoreError::AppendRejected);
        }
        self.conn.execute(
            "DELETE FROM pending_joins WHERE workspace=?1 AND device=?2",
            params![w.0.as_slice(), signer.device_id().0.as_slice()],
        )?;
        tx.commit()?;
        Ok(true)
    }
}
