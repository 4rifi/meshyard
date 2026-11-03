//! Transactional phase-two fold, retained for all authors and filtered only at read time.
use super::*;

fn marker(workspace: WorkspaceId) -> String {
    let id: String = workspace.0.iter().map(|b| format!("{b:02x}")).collect();
    format!("projection_v1:{id}")
}
pub(super) fn agent_row(row: &rusqlite::Row<'_>) -> Result<Agent, StoreError> {
    let author: Vec<u8> = row.get(0)?;
    let id: Vec<u8> = row.get(1)?;
    let kind: i32 = row.get(2)?;
    let agent: Agent =
        postcard::from_bytes(&row.get::<_, Vec<u8>>(3)?).map_err(|_| StoreError::Database)?;
    if agent.author.0.as_slice() != author
        || agent.local_id.as_slice() != id
        || binding_kind(agent.kind) != kind
    {
        return Err(StoreError::Database);
    }
    Ok(agent)
}
impl Database {
    pub(super) fn load_genesis(&mut self) -> Result<(), StoreError> {
        for workspace in self.workspaces()? {
            let (id, genesis) = verify_genesis(
                &decode_genesis(&self.genesis(workspace)?)?,
                self.verifier.as_ref(),
            )?;
            if workspace != id {
                return Err(StoreError::Database);
            }
            self.genesis_cache.insert(id, genesis);
        }
        Ok(())
    }
    pub(super) fn mark_projection(&self, workspace: WorkspaceId) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO meta(key,value) VALUES(?1,x'01')",
            [marker(workspace)],
        )?;
        Ok(())
    }
    pub(super) fn check_projection(&self, workspace: WorkspaceId) -> Result<(), StoreError> {
        self.check_workspace(workspace)?;
        let count: i64 = self.conn.query_row(
            "SELECT event_count FROM projection_counts WHERE workspace=?1",
            [workspace.0.as_slice()],
            |r| r.get(0),
        )?;
        if count > 100_000 {
            return Err(StoreError::ProjectionLimit);
        }
        Ok(())
    }
    pub(super) fn backfill_projections(&self) -> Result<(), StoreError> {
        // Authority and ordinary projections share one retryable transaction. Resetting derived
        // membership here also reconstructs added_name for databases upgraded from T-002.
        let transaction = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        for (&workspace, genesis) in &self.genesis_cache {
            let complete: bool = self.conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM meta WHERE key=?1)",
                [marker(workspace)],
                |r| r.get(0),
            )?;
            if complete {
                continue;
            }
            // Stream even a legacy over-limit log: cleanup still runs before reads, and
            // reads retain ProjectionLimit without making valid history impossible to open.
            for table in [
                "members",
                "membership_state",
                "agent_projection",
                "profile_projection",
            ] {
                self.conn.execute(
                    &format!("DELETE FROM {table} WHERE workspace=?1"),
                    [workspace.0.as_slice()],
                )?;
            }
            self.seed_membership(workspace, genesis.owner)?;
            let mut query = self.conn.prepare(
                "SELECT author,seq,signed_blob FROM events WHERE workspace=?1 ORDER BY author,seq",
            )?;
            let mut rows = query.query([workspace.0.as_slice()])?;
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
                self.apply_contiguous_membership(&event)?;
                self.apply_contiguous_projection(&event)?;
            }
            self.mark_projection(workspace)?;
        }
        transaction.commit()?;
        Ok(())
    }
    pub(super) fn apply_contiguous_projection(
        &self,
        event: &VerifiedEvent,
    ) -> Result<(), StoreError> {
        let w = event.workspace.0.as_slice();
        let a = event.author.0.as_slice();
        match &event.body {
            EventBody::AgentRegistered {
                agent_local_id,
                kind,
                label,
            } => {
                let value = Agent {
                    author: event.author,
                    local_id: *agent_local_id,
                    kind: *kind,
                    label: label.clone(),
                    state: ActivityState::Idle,
                    provenance: None,
                    tool: None,
                    task_label: None,
                };
                self.conn.execute("INSERT INTO agent_projection(workspace,author,local_id,kind,value,last_state_ts_ms) VALUES(?1,?2,?3,?4,?5,NULL) ON CONFLICT(workspace,author,local_id) DO UPDATE SET kind=excluded.kind,value=excluded.value,last_state_ts_ms=NULL", params![w,a,agent_local_id.as_slice(),binding_kind(*kind),postcard::to_stdvec(&value).map_err(|_| StoreError::Database)?])?;
            }
            EventBody::AgentState {
                agent_local_id,
                state,
                provenance,
                tool,
                task_label,
            } => {
                // Do not membership-filter here: an unknown author can be granted later.
                if let Some(mut agent) =
                    self.projected_agent(event.workspace, event.author, *agent_local_id)?
                {
                    agent.state = *state;
                    agent.provenance = Some(*provenance);
                    agent.tool = *tool;
                    agent.task_label.clone_from(task_label);
                    self.conn.execute("UPDATE agent_projection SET value=?4,last_state_ts_ms=?5 WHERE workspace=?1 AND author=?2 AND local_id=?3", params![w,a,agent_local_id.as_slice(),postcard::to_stdvec(&agent).map_err(|_| StoreError::Database)?,postcard::to_stdvec(&event.ts_ms).map_err(|_| StoreError::Database)?])?;
                }
            }
            EventBody::AgentGone { agent_local_id } => {
                self.conn.execute(
                    "DELETE FROM agent_projection WHERE workspace=?1 AND author=?2 AND local_id=?3",
                    params![w, a, agent_local_id.as_slice()],
                )?;
            }
            EventBody::ProfileUpdated {
                display_name,
                avatar,
            } => {
                self.conn.execute("INSERT INTO profile_projection(workspace,author,display_name,avatar) VALUES(?1,?2,?3,?4) ON CONFLICT(workspace,author) DO UPDATE SET display_name=excluded.display_name,avatar=excluded.avatar", params![w,a,display_name,avatar])?;
            }
            EventBody::MemberAdded { .. } | EventBody::MemberRemoved { .. } => {}
        }
        Ok(())
    }
    fn projected_agent(
        &self,
        workspace: WorkspaceId,
        own: DeviceId,
        local_id: [u8; 16],
    ) -> Result<Option<Agent>, StoreError> {
        let mut query = self.conn.prepare("SELECT author,local_id,kind,value FROM agent_projection WHERE workspace=?1 AND author=?2 AND local_id=?3")?;
        let mut rows = query.query(params![
            workspace.0.as_slice(),
            own.0.as_slice(),
            local_id.as_slice()
        ])?;
        rows.next()?.map(agent_row).transpose()
    }
    pub fn own_agent(
        &self,
        workspace: WorkspaceId,
        local_id: [u8; 16],
        own: DeviceId,
    ) -> Result<Option<Agent>, StoreError> {
        self.check_projection(workspace)?;
        let mut query = self.conn.prepare("SELECT a.author,a.local_id,a.kind,a.value FROM agent_projection a JOIN members m ON m.workspace=a.workspace AND m.device=a.author AND m.removed=0 WHERE a.workspace=?1 AND a.author=?2 AND a.local_id=?3")?;
        let mut rows = query.query(params![
            workspace.0.as_slice(),
            own.0.as_slice(),
            local_id.as_slice()
        ])?;
        rows.next()?.map(agent_row).transpose()
    }
    pub fn workspace_snapshot(
        &self,
        workspace: WorkspaceId,
    ) -> Result<WorkspaceSnapshot, StoreError> {
        self.check_projection(workspace)?;
        let genesis = self
            .genesis_cache
            .get(&workspace)
            .ok_or(StoreError::UnknownWorkspace)?
            .clone();
        let mut members = Vec::new();
        let mut query = self.conn.prepare("SELECT m.device,COALESCE(p.display_name,m.added_name),COALESCE(p.avatar,0) FROM members m LEFT JOIN profile_projection p ON p.workspace=m.workspace AND p.author=m.device WHERE m.workspace=?1 AND m.removed=0 ORDER BY m.device")?;
        let mut rows = query.query([workspace.0.as_slice()])?;
        while let Some(row) = rows.next()? {
            let device = DeviceId(
                row.get::<_, Vec<u8>>(0)?
                    .try_into()
                    .map_err(|_| StoreError::Database)?,
            );
            members.push(Member {
                device,
                display_name: row.get(1)?,
                role: if device == genesis.owner {
                    Role::Owner
                } else {
                    Role::Member
                },
                avatar: row.get(2)?,
            });
        }
        let mut agents = Vec::new();
        let mut agent_state_ts = BTreeMap::new();
        let mut query = self.conn.prepare("SELECT a.author,a.local_id,a.kind,a.value,a.last_state_ts_ms FROM agent_projection a JOIN members m ON m.workspace=a.workspace AND m.device=a.author AND m.removed=0 WHERE a.workspace=?1 ORDER BY a.author,a.local_id")?;
        let mut rows = query.query([workspace.0.as_slice()])?;
        while let Some(row) = rows.next()? {
            let agent = agent_row(row)?;
            if let Some(ts) = row.get::<_, Option<Vec<u8>>>(4)? {
                agent_state_ts.insert(
                    (agent.author, agent.local_id),
                    postcard::from_bytes(&ts).map_err(|_| StoreError::Database)?,
                );
            }
            agents.push(agent);
        }
        Ok(WorkspaceSnapshot {
            genesis,
            epoch: self.current_epoch(workspace)?,
            members,
            agents,
            agent_state_ts,
        })
    }
}
