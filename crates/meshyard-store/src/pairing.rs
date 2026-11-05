//! Local durable pairing commands. Tokens are stored only as hashes and never implement Debug.
use crate::{Store, StoreError};
use meshyard_core::{DeviceId, Signer, WorkspaceId};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pairing {
    pub invite_id: [u8; 16],
    pub state: String,
    pub expires_ms: u64,
    pub reserved_ms: Option<u64>,
    pub joiner: Option<DeviceId>,
    pub joiner_nonce: Option<[u8; 16]>,
    pub inviter_nonce: Option<[u8; 16]>,
    pub transcript_hash: Option<[u8; 32]>,
    pub inviter_confirmed: bool,
    pub joiner_confirmed: bool,
}
pub struct Reservation {
    pub workspace: WorkspaceId,
    pub invite_id: [u8; 16],
    pub token_hash: [u8; 32],
    pub joiner: DeviceId,
    pub joiner_nonce: [u8; 16],
    pub inviter_nonce: [u8; 16],
    pub display_name: String,
    pub now_ms: u64,
}
#[derive(Clone)]
pub struct PendingJoin {
    pub workspace: WorkspaceId,
    pub device: DeviceId,
    pub owner: DeviceId,
    pub owner_address: Vec<u8>,
    pub display_name: String,
    pub invite_id: [u8; 16],
    pub transcript_hash: [u8; 32],
}
impl Store {
    pub async fn confirm_join_attempt(
        &self,
        w: WorkspaceId,
        id: [u8; 16],
        device: DeviceId,
        hash: [u8; 32],
    ) -> Result<(), StoreError> {
        self.call(move |db| db.confirm_join_attempt(w, id, device, hash))
            .await
    }
    pub async fn accept_join(
        &self,
        raw: Vec<u8>,
        pending: PendingJoin,
    ) -> Result<WorkspaceId, StoreError> {
        self.call(move |db| db.add_workspace_pending(&raw, Some(pending)))
            .await
    }
    pub async fn pending_join(
        &self,
        w: WorkspaceId,
        device: DeviceId,
    ) -> Result<Option<PendingJoin>, StoreError> {
        self.call(move |db| db.pending_join(w, device)).await
    }
    pub async fn finish_join(
        &self,
        w: WorkspaceId,
        signer: Arc<dyn Signer>,
        now: u64,
    ) -> Result<bool, StoreError> {
        self.call(move |db| db.finish_join(w, signer.as_ref(), now))
            .await
    }
    pub async fn completed_pairing_resumes(
        &self,
        w: WorkspaceId,
        now: u64,
    ) -> Result<Vec<(DeviceId, u64)>, StoreError> {
        self.call(move |db| db.completed_pairing_resumes(w, now))
            .await
    }

    /// Admission includes every active record, independently of bounded UI history.
    pub async fn pairing_deadline(
        &self,
        workspace: WorkspaceId,
        now: u64,
    ) -> Result<u64, StoreError> {
        self.call(move |db| db.pairing_deadline(workspace, now))
            .await
    }

    /// Retain only the joiner's nonce for bounded crash/reconnect resume; never the ticket token.
    pub async fn join_nonce(
        &self,
        workspace: WorkspaceId,
        id: [u8; 16],
        inviter: DeviceId,
        nonce: [u8; 16],
        now: u64,
    ) -> Result<[u8; 16], StoreError> {
        self.call(move |db| db.join_nonce(workspace, id, inviter, nonce, now))
            .await
    }

    pub async fn create_invite(
        &self,
        workspace: WorkspaceId,
        invite_id: [u8; 16],
        token_hash: [u8; 32],
        now_ms: u64,
        signer: Arc<dyn Signer>,
    ) -> Result<(), StoreError> {
        self.call(move |db| {
            db.create_invite(workspace, invite_id, token_hash, now_ms, signer.as_ref())
        })
        .await
    }
    pub async fn pairings(
        &self,
        workspace: WorkspaceId,
        now_ms: u64,
    ) -> Result<Vec<Pairing>, StoreError> {
        self.call(move |db| db.pairings(workspace, now_ms)).await
    }
    pub async fn reserve_invite(&self, request: Reservation) -> Result<Pairing, StoreError> {
        self.call(move |db| db.reserve_invite(request)).await
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn confirm_pairing(
        &self,
        workspace: WorkspaceId,
        invite_id: [u8; 16],
        peer: DeviceId,
        hash: [u8; 32],
        local: bool,
        now_ms: u64,
        signer: Arc<dyn Signer>,
    ) -> Result<Pairing, StoreError> {
        self.call(move |db| {
            db.confirm_pairing(
                workspace,
                invite_id,
                peer,
                hash,
                local,
                now_ms,
                signer.as_ref(),
            )
        })
        .await
    }
    pub async fn abort_pairing(
        &self,
        workspace: WorkspaceId,
        invite_id: [u8; 16],
    ) -> Result<(), StoreError> {
        self.call(move |db| db.abort_pairing(workspace, invite_id))
            .await
    }
}
