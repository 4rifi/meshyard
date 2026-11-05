//! Owner-authorized pairing over the existing bounded, TLS-authenticated transport.
//! New ALPN payloads live here; signed wire v1 is immutable.
use crate::*;
use meshyard_core::{
    DeviceId, Ed25519Verifier, Signer, pairing::Transcript, wire::v1::SignedGenesisV1,
};
use meshyard_store::{Store, pairing::Reservation};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, sync::RwLock};
use tokio::time::sleep;

const REQUEST: u8 = 16;
const CHALLENGE: u8 = 17;
const CONFIRM: u8 = 18;
const WELCOME: u8 = 19;
const REJECT: u8 = 20;

/// Secret capability: deliberately redacted Debug, never sent to logs or argv.
#[derive(Serialize, Deserialize)]
pub struct Ticket {
    v: u8,
    genesis: SignedGenesisV1,
    inviter: EndpointAddr,
    invite_id: [u8; 16],
    token: [u8; 32],
    expires_ms: u64,
}
impl fmt::Debug for Ticket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Ticket([REDACTED])")
    }
}
impl Ticket {
    pub fn parse(link: &str) -> Result<Self, NetError> {
        if link.len() > 16 * 1024 {
            return Err(NetError::Frame);
        }
        let encoded = link
            .strip_prefix("meshyard://join/")
            .ok_or(NetError::Frame)?;
        let bytes = data_encoding::BASE32_NOPAD
            .decode(encoded.as_bytes())
            .map_err(|_| NetError::Frame)?;
        let ticket: Self = decode_payload(&bytes)?;
        ticket.validate()?;
        Ok(ticket)
    }
    pub fn inviter(&self) -> EndpointAddr {
        self.inviter.clone()
    }
    pub fn workspace(&self) -> Result<WorkspaceId, NetError> {
        Ok(
            meshyard_core::verify_genesis(&self.genesis, &Ed25519Verifier)
                .map_err(|_| NetError::Frame)?
                .0,
        )
    }
    fn validate(&self) -> Result<(), NetError> {
        let (_, genesis) = meshyard_core::verify_genesis(&self.genesis, &Ed25519Verifier)
            .map_err(|_| NetError::Frame)?;
        if self.v != 1
            || genesis.mode != Mode::Strict
            || genesis.owner.0 != *self.inviter.id.as_bytes()
            || self.expires_ms <= timestamp_ms()
            || self.inviter.ip_addrs().next().is_none()
            || self.inviter.relay_urls().next().is_some()
        {
            return Err(NetError::NotAdmitted);
        }
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
struct JoinRequest {
    invite_id: [u8; 16],
    token: [u8; 32],
    joiner_profile: Profile,
    joiner_nonce: [u8; 16],
}
#[derive(Serialize, Deserialize)]
struct Profile {
    display_name: String,
    avatar: u16,
}
#[derive(Serialize, Deserialize)]
struct JoinChallenge {
    inviter_nonce: [u8; 16],
}
#[derive(Serialize, Deserialize)]
struct Confirm {
    transcript_hash: [u8; 32],
}
#[derive(Serialize, Deserialize)]
struct Welcome {
    genesis: SignedGenesisV1,
}

#[derive(Clone, Debug)]
pub struct PendingPair {
    pub invite_id: [u8; 16],
    pub peer: DeviceId,
    pub words: [String; 6],
    pub is_inviter: bool,
    pub local_confirmed: bool,
    pub state: String,
    hash: [u8; 32],
    deadline_ms: u64,
    rejected: bool,
}
struct AdmissionView {
    members: BTreeSet<EndpointId>,
    open_until: u64,
    resumes: BTreeMap<EndpointId, u64>,
}
pub struct PairingService {
    pub store: Store,
    pub signer: Arc<dyn Signer>,
    pub workspace: WorkspaceId,
    admission: RwLock<AdmissionView>,
    transition: tokio::sync::Mutex<()>,
    expected_owner: RwLock<Option<EndpointId>>,
    pending: Mutex<BTreeMap<[u8; 16], PendingPair>>,
}
fn read<T>(value: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    value
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
fn write<T>(value: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    value
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
fn storage(_: meshyard_store::StoreError) -> NetError {
    NetError::NotAdmitted
}
fn random<const N: usize>() -> Result<[u8; N], NetError> {
    let mut b = [0; N];
    getrandom::fill(&mut b).map_err(|_| NetError::Closed)?;
    Ok(b)
}
impl PairingService {
    pub fn new(store: Store, signer: Arc<dyn Signer>, workspace: WorkspaceId) -> Arc<Self> {
        Arc::new(Self {
            store,
            signer,
            workspace,
            transition: tokio::sync::Mutex::new(()),
            admission: RwLock::new(AdmissionView {
                members: BTreeSet::new(),
                open_until: 0,
                resumes: BTreeMap::new(),
            }),
            expected_owner: RwLock::new(None),
            pending: Mutex::new(BTreeMap::new()),
        })
    }
    pub async fn refresh(&self) -> Result<(), NetError> {
        let _transition = self.transition.lock().await;
        self.refresh_inner().await
    }
    async fn refresh_inner(&self) -> Result<(), NetError> {
        let snapshot = self
            .store
            .workspace_snapshot(self.workspace)
            .await
            .map_err(storage)?;
        let open_until = self
            .store
            .pairing_deadline(self.workspace, timestamp_ms())
            .await
            .map_err(storage)?;
        let resumes = self
            .store
            .completed_pairing_resumes(self.workspace, timestamp_ms())
            .await
            .map_err(storage)?;
        let mut view = write(&self.admission);
        view.resumes = resumes
            .into_iter()
            .filter_map(|(id, until)| EndpointId::from_bytes(&id.0).ok().map(|id| (id, until)))
            .collect();
        view.members = snapshot
            .members
            .into_iter()
            .filter_map(|m| EndpointId::from_bytes(&m.device.0).ok())
            .collect();
        view.open_until = open_until;
        Ok(())
    }
    pub async fn invite(&self, node: &MeshNode) -> Result<String, NetError> {
        let raw = self.store.genesis(self.workspace).await.map_err(storage)?;
        let genesis: SignedGenesisV1 = decode_payload(&raw)?;
        let (_, g) = meshyard_core::verify_genesis(&genesis, &Ed25519Verifier)
            .map_err(|_| NetError::Frame)?;
        if g.owner != self.signer.device_id() || g.mode != Mode::Strict {
            return Err(NetError::NotAdmitted);
        }
        let inviter = node.address();
        if inviter.ip_addrs().next().is_none() || inviter.relay_urls().next().is_some() {
            return Err(NetError::StrictAddress);
        }
        let now = timestamp_ms();
        let invite_id = random()?;
        let token = random()?;
        self.store
            .create_invite(
                self.workspace,
                invite_id,
                *blake3::hash(&token).as_bytes(),
                now,
                self.signer.clone(),
            )
            .await
            .map_err(storage)?;
        self.refresh().await?;
        let ticket = Ticket {
            v: 1,
            genesis,
            inviter,
            invite_id,
            token,
            expires_ms: now + 900_000,
        };
        Ok(format!(
            "meshyard://join/{}",
            data_encoding::BASE32_NOPAD
                .encode(&postcard::to_stdvec(&ticket).map_err(|_| NetError::Frame)?)
        ))
    }
    fn pairing_open_at(&self, w: WorkspaceId, now: u64) -> bool {
        w == self.workspace && read(&self.admission).open_until > now
    }
    pub fn pending(&self) -> Vec<PendingPair> {
        lock(&self.pending).values().cloned().collect()
    }
    pub async fn confirm(&self, id: [u8; 16], matches: bool) -> Result<(), NetError> {
        let _transition = self.transition.lock().await;
        let pair = lock(&self.pending)
            .get(&id)
            .cloned()
            .ok_or(NetError::NotAdmitted)?;
        if pair.local_confirmed || pair.rejected {
            return if matches == pair.local_confirmed {
                Ok(())
            } else {
                Err(NetError::NotAdmitted)
            };
        }
        if pair.deadline_ms <= timestamp_ms()
            || !matches!(pair.state.as_str(), "Reserved" | "Disconnected")
        {
            return Err(NetError::Timeout);
        }
        let mut durable_state = None;
        if pair.is_inviter {
            write(&self.admission).open_until = 0;
            if matches {
                let persisted = self
                    .store
                    .confirm_pairing(
                        self.workspace,
                        id,
                        pair.peer,
                        pair.hash,
                        true,
                        timestamp_ms(),
                        self.signer.clone(),
                    )
                    .await
                    .map_err(storage)?;
                durable_state = Some(persisted.state);
            } else {
                self.store
                    .abort_pairing(self.workspace, id)
                    .await
                    .map_err(storage)?;
            }
            self.refresh_inner().await?;
        }
        let mut pending = lock(&self.pending);
        let p = pending.get_mut(&id).ok_or(NetError::NotAdmitted)?;
        p.local_confirmed = matches;
        p.rejected = !matches;
        if !matches {
            p.state = "Aborted".into();
        } else if let Some(state) = durable_state {
            p.state = state;
        }
        Ok(())
    }
    fn show(
        &self,
        id: [u8; 16],
        peer: DeviceId,
        hash: [u8; 32],
        inviter: bool,
    ) -> Result<(), NetError> {
        let mut pending = lock(&self.pending);
        pending.retain(|_, p| p.deadline_ms > timestamp_ms());
        if pending.len() >= 16 && !pending.contains_key(&id) {
            return Err(NetError::Limit);
        }
        pending.insert(
            id,
            PendingPair {
                invite_id: id,
                peer,
                words: meshyard_core::pairing::sas(&hash).map(str::to_owned),
                is_inviter: inviter,
                local_confirmed: false,
                state: "Reserved".into(),
                hash,
                deadline_ms: timestamp_ms() + 300_000,
                rejected: false,
            },
        );
        Ok(())
    }
    async fn wait_local(&self, id: [u8; 16]) -> Result<(), NetError> {
        loop {
            let p = lock(&self.pending)
                .get(&id)
                .cloned()
                .ok_or(NetError::NotAdmitted)?;
            if p.rejected {
                return Err(NetError::NotAdmitted);
            }
            if p.deadline_ms <= timestamp_ms() {
                return Err(NetError::Timeout);
            }
            if p.local_confirmed {
                return Ok(());
            }
            sleep(Duration::from_millis(50)).await;
        }
    }
    fn done(&self, id: [u8; 16], state: &str) {
        if let Some(p) = lock(&self.pending).get_mut(&id)
            && p.state != "Aborted"
        {
            p.state = state.into();
        }
    }
    async fn serve_pair(&self, connection: AdmittedConnection) -> Result<(), NetError> {
        let mut stream = connection.accept_pair_stream().await?;
        let (_, raw) = stream.read(&[REQUEST]).await?;
        let request: JoinRequest = decode_payload(&raw)?;
        meshyard_core::validate_received(&request.joiner_profile.display_name, 64)
            .map_err(|_| NetError::Frame)?;
        let peer = DeviceId(*connection.peer().as_bytes());
        let p = {
            let _transition = self.transition.lock().await;
            write(&self.admission).open_until = 0;
            let result = self
                .store
                .reserve_invite(Reservation {
                    workspace: self.workspace,
                    invite_id: request.invite_id,
                    token_hash: *blake3::hash(&request.token).as_bytes(),
                    joiner: peer,
                    joiner_nonce: request.joiner_nonce,
                    inviter_nonce: random()?,
                    display_name: request.joiner_profile.display_name,
                    now_ms: timestamp_ms(),
                })
                .await
                .map_err(storage);
            self.refresh_inner().await?;
            let p = result?;
            self.show(
                p.invite_id,
                peer,
                p.transcript_hash.ok_or(NetError::Frame)?,
                true,
            )?;
            if p.inviter_confirmed
                && let Some(entry) = lock(&self.pending).get_mut(&p.invite_id)
            {
                entry.local_confirmed = true;
            }
            p
        };
        let id = p.invite_id;
        let hash = p.transcript_hash.ok_or(NetError::Frame)?;
        let nonce = p.inviter_nonce.ok_or(NetError::Frame)?;
        let remaining = p
            .reserved_ms
            .ok_or(NetError::Frame)?
            .saturating_add(300_000)
            .saturating_sub(timestamp_ms());
        stream.wait_for_pair_decision(Duration::from_millis(remaining));
        stream
            .write(
                CHALLENGE,
                &postcard::to_stdvec(&JoinChallenge {
                    inviter_nonce: nonce,
                })
                .map_err(|_| NetError::Frame)?,
            )
            .await?;
        let (mut reader, mut writer) = stream.split();
        let result = async {
            let local = async {
                if let Err(error) = self.wait_local(id).await {
                    let _ = writer.write(REJECT, &[]).await;
                    return Err(error);
                }
                writer
                    .write(
                        CONFIRM,
                        &postcard::to_stdvec(&Confirm {
                            transcript_hash: hash,
                        })
                        .map_err(|_| NetError::Frame)?,
                    )
                    .await
            };
            let remote = async {
                let (kind, raw) = reader.read(&[CONFIRM, REJECT]).await?;
                if kind == REJECT {
                    let _transition = self.transition.lock().await;
                    write(&self.admission).open_until = 0;
                    self.store
                        .abort_pairing(self.workspace, id)
                        .await
                        .map_err(storage)?;
                    self.refresh_inner().await?;
                    return Err(NetError::NotAdmitted);
                }
                let confirm: Confirm = decode_payload(&raw)?;
                if p.state == "Completed" {
                    return if confirm.transcript_hash == hash {
                        Ok(())
                    } else {
                        Err(NetError::NotAdmitted)
                    };
                }
                let _transition = self.transition.lock().await;
                write(&self.admission).open_until = 0;
                let result = self
                    .store
                    .confirm_pairing(
                        self.workspace,
                        id,
                        peer,
                        confirm.transcript_hash,
                        false,
                        timestamp_ms(),
                        self.signer.clone(),
                    )
                    .await
                    .map_err(storage);
                self.refresh_inner().await?;
                result?;
                Ok(())
            };
            tokio::try_join!(local, remote)?;
            self.refresh().await?;
            let genesis: SignedGenesisV1 =
                decode_payload(&self.store.genesis(self.workspace).await.map_err(storage)?)?;
            writer
                .write(
                    WELCOME,
                    &postcard::to_stdvec(&Welcome { genesis }).map_err(|_| NetError::Frame)?,
                )
                .await?;
            writer.finish()?;
            writer.acknowledged().await?;
            Ok(())
        }
        .await;
        if result.is_ok() {
            self.done(id, "Completed");
        } else {
            self.done(id, "Disconnected");
        }
        result
    }
    pub async fn join(
        &self,
        node: &MeshNode,
        ticket: Ticket,
        display_name: String,
    ) -> Result<(), NetError> {
        ticket.validate()?;
        if ticket.workspace()? != self.workspace {
            return Err(NetError::NotMember);
        }
        meshyard_core::validate_received(&display_name, 64).map_err(|_| NetError::Frame)?;
        *write(&self.expected_owner) = Some(ticket.inviter.id);
        let id = ticket.invite_id;
        let result = self.join_inner(node, ticket, display_name).await;
        if result.is_err() {
            self.done(id, "Disconnected");
        }
        *write(&self.expected_owner) = None;
        result
    }
    async fn join_inner(
        &self,
        node: &MeshNode,
        ticket: Ticket,
        display_name: String,
    ) -> Result<(), NetError> {
        let connection = node.connect(ticket.inviter.clone(), Protocol::Pair).await?;
        let mut stream = connection.open_stream().await?;
        let joiner_nonce = self
            .store
            .join_nonce(
                self.workspace,
                ticket.invite_id,
                DeviceId(*ticket.inviter.id.as_bytes()),
                random()?,
                timestamp_ms(),
            )
            .await
            .map_err(storage)?;
        let request = JoinRequest {
            invite_id: ticket.invite_id,
            token: ticket.token,
            joiner_profile: Profile {
                display_name,
                avatar: 0,
            },
            joiner_nonce,
        };
        stream
            .write(
                REQUEST,
                &postcard::to_stdvec(&request).map_err(|_| NetError::Frame)?,
            )
            .await?;
        let (_, raw) = stream.read(&[CHALLENGE]).await?;
        let challenge: JoinChallenge = decode_payload(&raw)?;
        let hash = Transcript {
            workspace: self.workspace,
            mode: Mode::Strict,
            protocol_version: 1,
            inviter: DeviceId(*ticket.inviter.id.as_bytes()),
            joiner: self.signer.device_id(),
            invite_id: ticket.invite_id,
            inviter_nonce: challenge.inviter_nonce,
            joiner_nonce,
        }
        .hash();
        self.show(
            ticket.invite_id,
            DeviceId(*ticket.inviter.id.as_bytes()),
            hash,
            false,
        )?;
        stream.wait_for_pair_decision(Duration::from_secs(300));
        if let Err(error) = self.wait_local(ticket.invite_id).await {
            let _ = stream.write(REJECT, &[]).await;
            return Err(error);
        }
        self.store
            .confirm_join_attempt(
                self.workspace,
                ticket.invite_id,
                self.signer.device_id(),
                hash,
            )
            .await
            .map_err(storage)?;
        stream
            .write(
                CONFIRM,
                &postcard::to_stdvec(&Confirm {
                    transcript_hash: hash,
                })
                .map_err(|_| NetError::Frame)?,
            )
            .await?;
        let (_, raw) = stream.read(&[CONFIRM]).await?;
        let remote: Confirm = decode_payload(&raw)?;
        if remote.transcript_hash != hash {
            self.done(ticket.invite_id, "Aborted");
            return Err(NetError::NotAdmitted);
        }
        let (_, raw) = stream.read(&[WELCOME]).await?;
        let welcome: Welcome = decode_payload(&raw)?;
        if welcome.genesis != ticket.genesis {
            return Err(NetError::Frame);
        }
        self.store
            .accept_join(
                postcard::to_stdvec(&welcome.genesis).map_err(|_| NetError::Frame)?,
                meshyard_store::pairing::PendingJoin {
                    workspace: self.workspace,
                    device: self.signer.device_id(),
                    owner: DeviceId(*ticket.inviter.id.as_bytes()),
                    owner_address: postcard::to_stdvec(&ticket.inviter)
                        .map_err(|_| NetError::Frame)?,
                    display_name: request.joiner_profile.display_name.clone(),
                    invite_id: ticket.invite_id,
                    transcript_hash: hash,
                },
            )
            .await
            .map_err(storage)?;
        self.store
            .save_peer_address(
                self.workspace,
                DeviceId(*ticket.inviter.id.as_bytes()),
                postcard::to_stdvec(&ticket.inviter).map_err(|_| NetError::Frame)?,
            )
            .await
            .map_err(storage)?;
        self.refresh().await?;
        self.done(ticket.invite_id, "Completed");
        Ok(())
    }
}
impl Admission for PairingService {
    fn is_member(&self, workspace: WorkspaceId, peer: EndpointId) -> bool {
        workspace == self.workspace && read(&self.admission).members.contains(&peer)
    }
    fn pairing_incoming(&self, w: WorkspaceId, peer: EndpointId) -> bool {
        w == self.workspace
            && (self.pairing_open(w)
                || read(&self.admission)
                    .resumes
                    .get(&peer)
                    .is_some_and(|until| *until > timestamp_ms()))
    }
    fn pairing_outgoing(&self, w: WorkspaceId, peer: EndpointId) -> bool {
        w == self.workspace && *read(&self.expected_owner) == Some(peer)
    }
    fn pairing_open(&self, workspace: WorkspaceId) -> bool {
        self.pairing_open_at(workspace, timestamp_ms())
    }
}
impl ProtocolHandler for Arc<PairingService> {
    fn handle(&self, connection: AdmittedConnection) -> HandlerFuture {
        let service = self.clone();
        Box::pin(async move { service.serve_pair(connection).await })
    }
}

#[cfg(test)]
pub(crate) mod security_tests {
    use super::*;
    use meshyard_core::{Genesis, SecretKey as CoreKey, sign_genesis};
    use std::os::unix::fs::PermissionsExt;
    pub(crate) async fn fixture() -> (
        tempfile::TempDir,
        Arc<PairingService>,
        Arc<PairingService>,
        MeshNode,
        MeshNode,
    ) {
        let dir = tempfile::Builder::new()
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let a = Store::open(dir.path().join("owner")).unwrap();
        let b = Store::open(dir.path().join("joiner")).unwrap();
        let key = Arc::new(CoreKey::from_bytes(&[51; 32]));
        let joiner = Arc::new(CoreKey::from_bytes(&[52; 32]));
        let genesis = sign_genesis(
            Genesis {
                owner: key.device_id(),
                name: "Wire security".into(),
                mode: Mode::Strict,
                created_ms: timestamp_ms(),
                nonce: [1; 16],
            },
            key.as_ref(),
        )
        .unwrap();
        let w = a
            .add_workspace(postcard::to_stdvec(&genesis).unwrap())
            .await
            .unwrap();
        let owner = PairingService::new(a, key, w);
        let client = PairingService::new(b, joiner, w);
        owner.refresh().await.unwrap();
        *write(&client.expected_owner) =
            Some(EndpointId::from_bytes(&owner.signer.device_id().0).unwrap());
        async fn node(service: Arc<PairingService>) -> MeshNode {
            let mut bytes = [0; 32];
            // Deterministic test key, never a production identity.
            bytes.fill(
                if service.signer.device_id() == CoreKey::from_bytes(&[51; 32]).device_id() {
                    51
                } else {
                    52
                },
            );
            let mut config = NetConfig::new(
                SecretKey::from_bytes(&bytes),
                service.workspace,
                Mode::Strict,
                RelayChoice::Disabled,
            );
            config.mapping = MappingPolicy::Disabled;
            config.bind_addr = Some("127.0.0.1:0".parse().unwrap());
            config.admission = service.clone();
            config.sync_handler = Some(Arc::new(crate::sync::SyncService::new(service.clone())));
            config.pair_handler = Some(Arc::new(service));
            MeshNode::bind_inner(config, None, |b| b).await.unwrap()
        }
        let an = node(owner.clone()).await;
        let bn = node(client.clone()).await;
        (dir, owner, client, an, bn)
    }
    async fn wait_pending(service: &PairingService) -> PendingPair {
        for _ in 0..1000 {
            if let Some(p) = service.pending().first() {
                return p.clone();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("No pending comparison");
    }
    #[tokio::test]
    async fn simultaneous_owner_decisions_never_mark_completed_membership_rejected() {
        for positive_first in [true, false] {
            let (_dir, owner, client, a, b) = fixture().await;
            let id = [61; 16];
            let now = timestamp_ms();
            owner
                .store
                .create_invite(owner.workspace, id, [2; 32], now, owner.signer.clone())
                .await
                .unwrap();
            let p = owner
                .store
                .reserve_invite(Reservation {
                    workspace: owner.workspace,
                    invite_id: id,
                    token_hash: [2; 32],
                    joiner: client.signer.device_id(),
                    joiner_nonce: [3; 16],
                    inviter_nonce: [4; 16],
                    display_name: "Peer".into(),
                    now_ms: now,
                })
                .await
                .unwrap();
            let hash = p.transcript_hash.unwrap();
            owner
                .store
                .confirm_pairing(
                    owner.workspace,
                    id,
                    client.signer.device_id(),
                    hash,
                    false,
                    now,
                    owner.signer.clone(),
                )
                .await
                .unwrap();
            owner
                .show(id, client.signer.device_id(), hash, true)
                .unwrap();
            let (first, second) = tokio::join!(
                owner.confirm(id, positive_first),
                owner.confirm(id, !positive_first)
            );
            assert_ne!(first.is_ok(), second.is_ok());
            let durable = owner
                .store
                .pairings(owner.workspace, timestamp_ms())
                .await
                .unwrap()
                .remove(0);
            let ui = owner.pending().remove(0);
            assert_eq!(ui.state, durable.state);
            if durable.state == "Completed" {
                assert!(ui.local_confirmed);
                assert!(!ui.rejected);
                assert_eq!(
                    owner
                        .store
                        .workspace_snapshot(owner.workspace)
                        .await
                        .unwrap()
                        .members
                        .len(),
                    2
                );
            } else {
                assert_eq!(durable.state, "Aborted");
                assert!(ui.rejected);
                assert!(!ui.local_confirmed);
                assert_eq!(
                    owner
                        .store
                        .workspace_snapshot(owner.workspace)
                        .await
                        .unwrap()
                        .members
                        .len(),
                    1
                );
            }
            a.close().await;
            b.close().await;
            owner.store.clone().close().await.unwrap();
            client.store.clone().close().await.unwrap();
        }
    }
    #[tokio::test]
    async fn human_decisions_after_thirty_seconds_complete_in_both_orders() {
        async fn case(owner_first: bool) {
            let (_dir, owner, client, a, b) = fixture().await;
            let b = Arc::new(b);
            let ticket = Ticket::parse(&owner.invite(&a).await.unwrap()).unwrap();
            let mut task = {
                let client = client.clone();
                let node = b.clone();
                tokio::spawn(async move { client.join(&node, ticket, "Peer".into()).await })
            };
            let p = tokio::select! {p=wait_pending(&owner)=>p,result=&mut task=>panic!("Join ended before comparison: {result:?}")};
            wait_pending(&client).await;
            if owner_first {
                owner.confirm(p.invite_id, true).await.unwrap();
            } else {
                client.confirm(p.invite_id, true).await.unwrap();
            }
            tokio::time::sleep(PAIR_TIMEOUT + Duration::from_secs(1)).await;
            assert_eq!(
                owner
                    .store
                    .workspace_snapshot(owner.workspace)
                    .await
                    .unwrap()
                    .members
                    .len(),
                1
            );
            if owner_first {
                client.confirm(p.invite_id, true).await.unwrap();
            } else {
                owner.confirm(p.invite_id, true).await.unwrap();
            }
            task.await.unwrap().unwrap();
            assert_eq!(
                owner.store.contiguous_heads(owner.workspace).await.unwrap()
                    [&owner.signer.device_id()],
                1
            );
            assert!(
                client
                    .store
                    .pending_join(client.workspace, client.signer.device_id())
                    .await
                    .unwrap()
                    .is_some()
            );
            assert!(owner.confirm(p.invite_id, false).await.is_err());
            a.close().await;
            Arc::try_unwrap(b).unwrap().close().await;
            owner.store.clone().close().await.unwrap();
            client.store.clone().close().await.unwrap();
        }
        tokio::join!(case(true), case(false));
    }
    #[tokio::test]
    async fn completed_resume_replays_welcome_once_and_joining_never_opens_inbound_admission() {
        let (_dir, owner, client, a, b) = fixture().await;
        let ticket = Ticket::parse(&owner.invite(&a).await.unwrap()).unwrap();
        let id = ticket.invite_id;
        let session = b
            .connect(ticket.inviter.clone(), Protocol::Pair)
            .await
            .unwrap();
        let mut stream = session.open_stream().await.unwrap();
        let nonce = client
            .store
            .join_nonce(
                client.workspace,
                id,
                owner.signer.device_id(),
                [9; 16],
                timestamp_ms(),
            )
            .await
            .unwrap();
        stream
            .write(
                REQUEST,
                &postcard::to_stdvec(&JoinRequest {
                    invite_id: id,
                    token: ticket.token,
                    joiner_profile: Profile {
                        display_name: "Peer".into(),
                        avatar: 0,
                    },
                    joiner_nonce: nonce,
                })
                .unwrap(),
            )
            .await
            .unwrap();
        stream.read(&[CHALLENGE]).await.unwrap();
        let p = wait_pending(&owner).await;
        assert!(!client.pairing_incoming(client.workspace, a.address().id));
        let before = b.application_read_count();
        // The inviter is a third incoming caller from the joiner's perspective; outgoing pinning grants no inbound right.
        let mut config = NetConfig::new(
            SecretKey::from_bytes(&[54; 32]),
            client.workspace,
            Mode::Strict,
            RelayChoice::Disabled,
        );
        struct Open;
        impl Admission for Open {
            fn is_member(&self, _: WorkspaceId, _: EndpointId) -> bool {
                false
            }
            fn pairing_open(&self, _: WorkspaceId) -> bool {
                true
            }
        }
        config.admission = Arc::new(Open);
        config.mapping = MappingPolicy::Disabled;
        config.bind_addr = Some("127.0.0.1:0".parse().unwrap());
        let outsider = MeshNode::bind_inner(config, None, |b| b).await.unwrap();
        if let Ok(c) = outsider.connect(b.address(), Protocol::Pair).await
            && let Ok(mut s) = c.open_stream().await
        {
            let _ = s.write(REQUEST, &[0; 16]).await;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(b.application_read_count(), before);
        outsider.close().await;
        stream
            .write(
                CONFIRM,
                &postcard::to_stdvec(&Confirm {
                    transcript_hash: p.hash,
                })
                .unwrap(),
            )
            .await
            .unwrap();
        for _ in 0..100 {
            if owner
                .store
                .pairings(owner.workspace, timestamp_ms())
                .await
                .unwrap()[0]
                .joiner_confirmed
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        drop(stream);
        drop(session);
        tokio::time::sleep(Duration::from_millis(100)).await;
        owner.confirm(id, true).await.unwrap();
        assert!(owner.is_member(owner.workspace, b.address().id));
        // Reuse the exact original capability and persisted nonce, not a newly issued invitation.
        {
            let task = client.join(&b, ticket, "Peer".into());
            tokio::pin!(task);
            let confirm = async {
                loop {
                    if let Some(p) = client.pending().iter().find(|p| p.state == "Reserved") {
                        client.confirm(p.invite_id, true).await.unwrap();
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            };
            let (result, _) = tokio::join!(&mut task, confirm);
            result.unwrap();
        }
        assert_eq!(
            owner.store.contiguous_heads(owner.workspace).await.unwrap()[&owner.signer.device_id()],
            1
        );
        a.close().await;
        b.close().await;
        owner.store.clone().close().await.unwrap();
        client.store.clone().close().await.unwrap();
    }
    #[tokio::test]
    async fn admission_closes_on_last_reject_and_reservation_deadline_without_ui_polling() {
        let (_dir, owner, client, a, b) = fixture().await;
        let ticket = Ticket::parse(&owner.invite(&a).await.unwrap()).unwrap();
        let session = b
            .connect(ticket.inviter.clone(), Protocol::Pair)
            .await
            .unwrap();
        let mut stream = session.open_stream().await.unwrap();
        stream
            .write(
                REQUEST,
                &postcard::to_stdvec(&JoinRequest {
                    invite_id: ticket.invite_id,
                    token: ticket.token,
                    joiner_profile: Profile {
                        display_name: "Peer".into(),
                        avatar: 0,
                    },
                    joiner_nonce: [9; 16],
                })
                .unwrap(),
            )
            .await
            .unwrap();
        stream.read(&[CHALLENGE]).await.unwrap();
        owner.confirm(ticket.invite_id, false).await.unwrap();
        assert!(!owner.pairing_open(owner.workspace));
        drop(stream);
        drop(session);
        tokio::time::sleep(Duration::from_millis(100)).await;
        let before = a.application_read_count();
        if let Ok(attack) = b.connect(a.address(), Protocol::Pair).await
            && let Ok(mut stream) = attack.open_stream().await
        {
            let _ = stream.write(REQUEST, &[0; 32]).await;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(a.application_read_count(), before);
        let now = timestamp_ms();
        owner
            .store
            .create_invite(
                owner.workspace,
                [77; 16],
                [2; 32],
                now,
                owner.signer.clone(),
            )
            .await
            .unwrap();
        owner
            .store
            .reserve_invite(Reservation {
                workspace: owner.workspace,
                invite_id: [77; 16],
                token_hash: [2; 32],
                joiner: client.signer.device_id(),
                joiner_nonce: [3; 16],
                inviter_nonce: [4; 16],
                display_name: "Peer".into(),
                now_ms: now,
            })
            .await
            .unwrap();
        owner.refresh().await.unwrap();
        assert!(owner.pairing_open_at(owner.workspace, now + 299_999));
        assert!(!owner.pairing_open_at(owner.workspace, now + 300_000));
        // Active admission is independent of the 64-row UI history window.
        owner
            .store
            .create_invite(
                owner.workspace,
                [88; 16],
                [2; 32],
                timestamp_ms(),
                owner.signer.clone(),
            )
            .await
            .unwrap();
        for n in 0..70u8 {
            owner
                .store
                .create_invite(
                    owner.workspace,
                    [n; 16],
                    [2; 32],
                    timestamp_ms(),
                    owner.signer.clone(),
                )
                .await
                .unwrap();
            owner
                .store
                .abort_pairing(owner.workspace, [n; 16])
                .await
                .unwrap();
        }
        owner.refresh().await.unwrap();
        assert!(owner.pairing_open(owner.workspace));
        a.close().await;
        b.close().await;
        owner.store.clone().close().await.unwrap();
        client.store.clone().close().await.unwrap();
    }
    #[tokio::test]
    async fn rejection_frame_with_correct_hash_and_wrong_transcript_never_grant_membership() {
        for reject in [true, false] {
            let (_dir, owner, client, a, b) = fixture().await;
            let ticket = Ticket::parse(&owner.invite(&a).await.unwrap()).unwrap();
            let session = b
                .connect(ticket.inviter.clone(), Protocol::Pair)
                .await
                .unwrap();
            let mut stream = session.open_stream().await.unwrap();
            stream
                .write(
                    REQUEST,
                    &postcard::to_stdvec(&JoinRequest {
                        invite_id: ticket.invite_id,
                        token: ticket.token,
                        joiner_profile: Profile {
                            display_name: "Peer".into(),
                            avatar: 0,
                        },
                        joiner_nonce: [9; 16],
                    })
                    .unwrap(),
                )
                .await
                .unwrap();
            stream.read(&[CHALLENGE]).await.unwrap();
            let pending = owner.pending().remove(0);
            owner.confirm(pending.invite_id, true).await.unwrap();
            stream.read(&[CONFIRM]).await.unwrap();
            let hash = if reject { pending.hash } else { [0; 32] };
            stream
                .write(
                    if reject { REJECT } else { CONFIRM },
                    &postcard::to_stdvec(&Confirm {
                        transcript_hash: hash,
                    })
                    .unwrap(),
                )
                .await
                .unwrap();
            for _ in 0..100 {
                if owner
                    .store
                    .pairings(owner.workspace, timestamp_ms())
                    .await
                    .unwrap()[0]
                    .state
                    == "Aborted"
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            assert_eq!(
                owner
                    .store
                    .pairings(owner.workspace, timestamp_ms())
                    .await
                    .unwrap()[0]
                    .state,
                "Aborted"
            );
            assert_eq!(
                owner
                    .store
                    .workspace_snapshot(owner.workspace)
                    .await
                    .unwrap()
                    .members
                    .len(),
                1
            );
            assert!(!owner.is_member(
                owner.workspace,
                EndpointId::from_bytes(&client.signer.device_id().0).unwrap()
            ));
            drop(stream);
            drop(session);
            a.close().await;
            b.close().await;
            owner.store.clone().close().await.unwrap();
            client.store.clone().close().await.unwrap();
        }
    }
}
