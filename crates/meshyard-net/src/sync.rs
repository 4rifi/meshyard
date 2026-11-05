//! Sender-only store-cursor replication; catch-up and live use exactly the same path.
use crate::*;
use meshyard_core::{DeviceId, Ed25519Verifier, Verifier, decode_signed_event, verify_and_decode};
use meshyard_store::{InsertOutcome, Store};
use serde::{Deserialize, Serialize};
const HELLO: u8 = 32;
const BATCH: u8 = 33;
const KEEPALIVE: u8 = 34;
#[derive(Serialize, Deserialize)]
struct Hello {
    workspace: WorkspaceId,
    protocol_version: u8,
    heads: BTreeMap<DeviceId, u64>,
    epoch: u32,
}
#[derive(Serialize, Deserialize)]
struct Batch {
    events: Vec<Vec<u8>>,
}
pub struct SyncService {
    pub pairing: Arc<crate::pairing::PairingService>,
    ready: watch::Sender<bool>,
}
impl SyncService {
    pub fn new(pairing: Arc<crate::pairing::PairingService>) -> Arc<Self> {
        Arc::new(Self {
            pairing,
            ready: watch::channel(false).0,
        })
    }
    pub fn ready(&self) -> watch::Receiver<bool> {
        self.ready.subscribe()
    }
    pub async fn session(
        &self,
        connection: AdmittedConnection,
        outgoing: bool,
    ) -> Result<(), NetError> {
        let workspace = self.pairing.workspace;
        let peer = DeviceId(*connection.peer().as_bytes());
        // Device gate already ran before application reads. Explicit workspace gate remains.
        let connection = connection.for_workspace(workspace)?;
        let mut stream = if outgoing {
            connection.open_stream().await?
        } else {
            connection.accept_stream().await?
        };
        let store = &self.pairing.store;
        let state = store
            .workspace_snapshot(workspace)
            .await
            .map_err(|_| NetError::Closed)?;
        let own = self.pairing.signer.device_id();
        let hello = Hello {
            workspace,
            protocol_version: 1,
            heads: store
                .contiguous_heads(workspace)
                .await
                .map_err(|_| NetError::Closed)?,
            epoch: state.epoch,
        };
        stream
            .write(
                HELLO,
                &postcard::to_stdvec(&hello).map_err(|_| NetError::Frame)?,
            )
            .await?;
        let (_, bytes) = stream.read(&[HELLO]).await?;
        let remote: Hello = decode_payload(&bytes)?;
        if remote.workspace != workspace || remote.protocol_version != 1 || remote.heads.len() > 256
        {
            return Err(NetError::NotMember);
        }
        self.ready.send_replace(true);
        let cursor = remote.heads.get(&own).copied().unwrap_or_default();
        stream.enter_live_phase();
        let (reader, writer) = stream.split();
        // Neither read_exact future is cancelled and reused. A failure ends the whole session.
        tokio::try_join!(
            send(store, workspace, own, cursor, writer),
            receive(self.pairing.clone(), peer, reader)
        )?;
        Ok(())
    }
}
async fn send(
    store: &Store,
    workspace: WorkspaceId,
    own: DeviceId,
    mut cursor: u64,
    mut writer: FrameWriter,
) -> Result<(), NetError> {
    let mut heartbeat = Instant::now();
    loop {
        // Bounded batch read from the durable source. Never forwards another author's log.
        let events = store
            .events_after(workspace, own, cursor, 256)
            .await
            .map_err(|_| NetError::Closed)?;
        let mut batch = Vec::new();
        let mut bytes = 0;
        for event in events {
            if bytes + event.len() > 240 * 1024 {
                break;
            }
            bytes += event.len();
            batch.push(event);
        }
        if let Some(last) = batch.last() {
            let verified = verify_and_decode(
                &decode_signed_event(last).map_err(|_| NetError::Frame)?,
                &Ed25519Verifier,
            )
            .map_err(|_| NetError::Frame)?;
            let next = verified.seq;
            writer
                .write(
                    BATCH,
                    &postcard::to_stdvec(&Batch { events: batch }).map_err(|_| NetError::Frame)?,
                )
                .await?;
            cursor = next;
            heartbeat = Instant::now();
        } else {
            if heartbeat.elapsed() >= Duration::from_secs(30) {
                writer.write(KEEPALIVE, &[]).await?;
                heartbeat = Instant::now();
            }
            // Polling is a bounded wakeup only; no second in-memory event path or queue exists.
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }
}
async fn receive(
    service: Arc<crate::pairing::PairingService>,
    peer: DeviceId,
    mut reader: FrameReader,
) -> Result<(), NetError> {
    loop {
        let (kind, bytes) = reader.read(&[BATCH, KEEPALIVE]).await?;
        if kind == KEEPALIVE {
            if !bytes.is_empty() {
                return Err(NetError::Frame);
            }
            continue;
        }
        let batch: Batch = decode_payload(&bytes)?;
        if batch.events.is_empty() || batch.events.len() > 256 {
            return Err(NetError::Frame);
        }
        for raw in batch.events {
            let signed = decode_signed_event(&raw).map_err(|_| NetError::Frame)?;
            let (header, _) =
                postcard::take_from_bytes::<meshyard_core::wire::v1::EventHeaderV1>(&signed.signed)
                    .map_err(|_| NetError::Frame)?;
            Ed25519Verifier
                .verify(
                    DeviceId(header.author),
                    &meshyard_core::domain_message(meshyard_core::EVENT_DOMAIN, &signed.signed),
                    &signed.sig,
                )
                .map_err(|_| NetError::Frame)?;
            if DeviceId(header.author) != peer || WorkspaceId(header.workspace) != service.workspace
            {
                return Err(NetError::NotMember);
            }
            match service
                .store
                .insert_remote(raw)
                .await
                .map_err(|_| NetError::Closed)?
            {
                InsertOutcome::Inserted | InsertOutcome::Duplicate | InsertOutcome::Pending => {}
                // Closing forces the next Hello to advertise the current persisted cursor,
                // including zero after overflow. Equivocation diagnostics stay durable.
                _ => return Err(NetError::Frame),
            }
        }
        service.refresh().await?;
    }
}
impl ProtocolHandler for Arc<SyncService> {
    fn handle(&self, connection: AdmittedConnection) -> HandlerFuture {
        let service = self.clone();
        Box::pin(async move { service.session(connection, false).await })
    }
}

#[cfg(test)]
mod security_tests {
    use super::*;
    use meshyard_core::{
        EVENT_DOMAIN, EventBody, SecretKey, Signer, domain_message,
        wire::v1::{EventHeaderV1, SignedEventV1},
    };
    #[tokio::test]
    async fn authenticated_sender_gate_and_invalid_utf8_diagnostics_survive_transport() {
        for malformed in [false, true] {
            let (_dir, owner, client, a, b) = crate::pairing::security_tests::fixture().await;
            owner
                .store
                .append_own(
                    owner.workspace,
                    EventBody::MemberAdded {
                        device: client.signer.device_id(),
                        display_name: "Peer".into(),
                    },
                    owner.signer.clone(),
                )
                .await
                .unwrap();
            client
                .store
                .add_workspace(owner.store.genesis(owner.workspace).await.unwrap())
                .await
                .unwrap();
            owner.refresh().await.unwrap();
            client.refresh().await.unwrap();
            let connection = b
                .connect(a.address(), Protocol::Sync)
                .await
                .unwrap()
                .for_workspace(owner.workspace)
                .unwrap();
            let mut stream = connection.open_stream().await.unwrap();
            let hello = Hello {
                workspace: owner.workspace,
                protocol_version: 1,
                heads: BTreeMap::new(),
                epoch: 0,
            };
            stream
                .write(HELLO, &postcard::to_stdvec(&hello).unwrap())
                .await
                .unwrap();
            stream.read(&[HELLO]).await.unwrap();
            let third = SecretKey::from_bytes(&[53; 32]);
            let signer: &dyn Signer = if malformed {
                client.signer.as_ref()
            } else {
                &third
            };
            let mut signed = postcard::to_stdvec(&EventHeaderV1 {
                v: 1,
                workspace: owner.workspace.0,
                author: signer.device_id().0,
                seq: 1,
                ts_ms: 1,
                membership_epoch: 1,
            })
            .unwrap();
            // Frozen ProfileUpdated tag=2, one-byte invalid UTF8 name, avatar=0.
            if malformed {
                signed.extend_from_slice(&[2, 1, 255, 0]);
            } else {
                signed.extend(
                    postcard::to_stdvec(&meshyard_core::wire::v1::EventBodyV1::from(
                        EventBody::ProfileUpdated {
                            display_name: "Third".into(),
                            avatar: 0,
                        },
                    ))
                    .unwrap(),
                );
            }
            let sig = signer.sign(&domain_message(EVENT_DOMAIN, &signed)).unwrap();
            let raw = postcard::to_stdvec(&SignedEventV1 { signed, sig }).unwrap();
            stream
                .write(
                    BATCH,
                    &postcard::to_stdvec(&Batch { events: vec![raw] }).unwrap(),
                )
                .await
                .unwrap();
            for _ in 0..100 {
                if malformed
                    && owner
                        .store
                        .diagnostics(owner.workspace, client.signer.device_id())
                        .await
                        .unwrap()
                        .protocol_violation
                {
                    break;
                }
                if !malformed
                    && a.diagnostics()
                        .peers
                        .iter()
                        .any(|p| p.last_error == Some(NetError::NotMember))
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            assert!(
                !owner
                    .store
                    .contiguous_heads(owner.workspace)
                    .await
                    .unwrap()
                    .contains_key(&signer.device_id())
            );
            assert_eq!(
                owner
                    .store
                    .diagnostics(owner.workspace, client.signer.device_id())
                    .await
                    .unwrap()
                    .protocol_violation,
                malformed
            );
            if !malformed {
                assert!(
                    a.diagnostics()
                        .peers
                        .iter()
                        .any(|p| p.last_error == Some(NetError::NotMember))
                );
            }
            drop(stream);
            drop(connection);
            a.close().await;
            b.close().await;
            owner.store.clone().close().await.unwrap();
            client.store.clone().close().await.unwrap();
        }
    }
}
