//! Real owner pairing, then two production-core processes recovering a post-Welcome crash.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use iroh::{Endpoint, RelayMode, endpoint::presets};
use meshyard_core::{wire::v1::*, *};
use meshyard_net::{MappingPolicy, MeshNode, NetConfig, RelayChoice, pairing::PairingService};
use meshyard_store::{Store, identity::*, pairing::PendingJoin};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    io::{BufRead, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::Arc,
    time::Duration,
};
#[derive(Deserialize)]
struct Ticket {
    v: u8,
    genesis: SignedGenesisV1,
    inviter: iroh::EndpointAddr,
    invite_id: [u8; 16],
    token: [u8; 32],
    expires_ms: u64,
}
#[derive(Serialize)]
struct Profile {
    display_name: String,
    avatar: u16,
}
#[derive(Serialize)]
struct Request {
    invite_id: [u8; 16],
    token: [u8; 32],
    joiner_profile: Profile,
    joiner_nonce: [u8; 16],
}
#[derive(Deserialize)]
struct Challenge {
    inviter_nonce: [u8; 16],
}
#[derive(Serialize, Deserialize)]
struct Confirm {
    transcript_hash: [u8; 32],
}
#[derive(Deserialize)]
struct Welcome {
    genesis: SignedGenesisV1,
}
fn now() -> u64 {
    u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap()
}
fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> T {
    let (v, rest) = postcard::take_from_bytes(bytes).unwrap();
    assert!(rest.is_empty());
    v
}
async fn send<T: Serialize>(s: &mut iroh::endpoint::SendStream, kind: u8, value: &T) {
    let bytes = postcard::to_stdvec(value).unwrap();
    s.write_all(
        &FrameHeaderV1 {
            frame_type: kind,
            len: bytes.len() as u32,
        }
        .encode()
        .unwrap(),
    )
    .await
    .unwrap();
    s.write_all(&bytes).await.unwrap();
}
async fn receive<T: serde::de::DeserializeOwned>(
    s: &mut iroh::endpoint::RecvStream,
    kind: u8,
) -> T {
    let mut h = [0; 10];
    s.read_exact(&mut h).await.unwrap();
    let h = FrameHeaderV1::decode(h, &[kind]).unwrap();
    assert!(h.len <= 16384);
    let mut b = vec![0; h.len as usize];
    s.read_exact(&mut b).await.unwrap();
    decode(&b)
}
struct NodeChild {
    child: Child,
    input: ChildStdin,
    lines: std::sync::mpsc::Receiver<String>,
}
impl NodeChild {
    fn start(data: &Path, home: &Path) -> Self {
        std::fs::create_dir_all(home).unwrap();
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(home.join("headless.log"))
            .unwrap();
        let mut child = Command::new(
            std::env::var_os("MESHYARD_TEST_NODE_BIN")
                .expect("Explicit production-core node binary required"),
        )
        .args(["--data-dir", data.to_str().unwrap(), "--key-store", "file"])
        .env("HOME", home)
        .env("MESHYARD_HOOK_HOME", home)
        .env("MESHYARD_KEY_STORE", "file")
        .env("MESHYARD_DATA_DIR", data)
        .env("MESHYARD_TEST_DISABLE_MAPPING", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(log)
        .spawn()
        .unwrap();
        let input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let (tx, lines) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(output).lines() {
                if tx.send(line.unwrap()).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            input,
            lines,
        }
    }
    fn request(&mut self, action: &str) -> Value {
        writeln!(self.input, "{}", json!({"action":action})).unwrap();
        self.input.flush().unwrap();
        let line = self.lines.recv_timeout(Duration::from_secs(20)).unwrap();
        let v: Value = serde_json::from_str(&line).unwrap();
        assert!(v.get("error").is_none());
        v["ok"].clone()
    }
}
impl Drop for NodeChild {
    fn drop(&mut self) {
        if self.child.try_wait().unwrap().is_none() {
            self.child.kill().unwrap();
            let _ = self.child.wait();
        }
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires explicitly built production-core node and normal macOS local socket permissions"]
async fn welcome_then_crash_reopens_same_identity_and_resumes_signed_membership() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::Builder::new()
        .prefix("p4-rec-")
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let kept = dir.keep();
    let dir = kept;
    println!("Recovery evidence profile: {}", dir.display());
    let owner_dir = dir.as_path().join("o");
    let join_dir = dir.as_path().join("j");
    let owner_store = Store::open(&owner_dir).unwrap();
    let join_store = Store::open(&join_dir).unwrap();
    let owner_key =
        Arc::new(load_or_create(&FileKeyStore::new(owner_dir.join("identity"))).unwrap());
    let join_key = Arc::new(load_or_create(&FileKeyStore::new(join_dir.join("identity"))).unwrap());
    let original_id = join_key.device_id();
    let genesis = sign_genesis(
        Genesis {
            owner: owner_key.device_id(),
            name: "Recovery".into(),
            mode: Mode::Strict,
            created_ms: now(),
            nonce: [11; 16],
        },
        owner_key.as_ref(),
    )
    .unwrap();
    let workspace = owner_store
        .add_workspace(postcard::to_stdvec(&genesis).unwrap())
        .await
        .unwrap();
    owner_store
        .append_own(
            workspace,
            EventBody::ProfileUpdated {
                display_name: "Owner".into(),
                avatar: 0,
            },
            owner_key.clone(),
        )
        .await
        .unwrap();
    let service = PairingService::new(owner_store.clone(), owner_key.clone(), workspace);
    service.refresh().await.unwrap();
    let mut config = NetConfig::new(
        owner_key.with_secret_bytes(iroh::SecretKey::from_bytes),
        workspace,
        Mode::Strict,
        RelayChoice::Disabled,
    );
    config.mapping = MappingPolicy::Disabled;
    config.admission = service.clone();
    config.pair_handler = Some(Arc::new(service.clone()));
    let owner = MeshNode::bind(config).await.unwrap();
    owner_store
        .save_endpoint_port(owner.ipv4_port().unwrap())
        .await
        .unwrap();
    let link = service.invite(&owner).await.unwrap();
    let ticket: Ticket = decode(
        &data_encoding::BASE32_NOPAD
            .decode(link.strip_prefix("meshyard://join/").unwrap().as_bytes())
            .unwrap(),
    );
    assert_eq!(ticket.v, 1);
    assert!(ticket.expires_ms > now());
    assert_eq!(ticket.genesis, genesis);
    let raw = Endpoint::builder(presets::Minimal)
        .secret_key(join_key.with_secret_bytes(iroh::SecretKey::from_bytes))
        .relay_mode(RelayMode::Disabled)
        .clear_address_lookup()
        .bind()
        .await
        .unwrap();
    let connection = raw
        .connect(ticket.inviter.clone(), meshyard_net::PAIR_ALPN)
        .await
        .unwrap();
    let (mut tx, mut rx) = connection.open_bi().await.unwrap();
    let nonce = join_store
        .join_nonce(
            workspace,
            ticket.invite_id,
            owner_key.device_id(),
            [22; 16],
            now(),
        )
        .await
        .unwrap();
    send(
        &mut tx,
        16,
        &Request {
            invite_id: ticket.invite_id,
            token: ticket.token,
            joiner_profile: Profile {
                display_name: "Recovered profile".into(),
                avatar: 0,
            },
            joiner_nonce: nonce,
        },
    )
    .await;
    let challenge: Challenge = receive(&mut rx, 17).await;
    let hash = meshyard_core::pairing::Transcript {
        workspace,
        mode: Mode::Strict,
        protocol_version: 1,
        inviter: owner_key.device_id(),
        joiner: original_id,
        invite_id: ticket.invite_id,
        inviter_nonce: challenge.inviter_nonce,
        joiner_nonce: nonce,
    }
    .hash();
    assert_eq!(
        service.pending()[0].words,
        meshyard_core::pairing::sas(&hash).map(str::to_owned)
    );
    service.confirm(ticket.invite_id, true).await.unwrap();
    let c: Confirm = receive(&mut rx, 18).await;
    assert_eq!(c.transcript_hash, hash);
    join_store
        .confirm_join_attempt(workspace, ticket.invite_id, original_id, hash)
        .await
        .unwrap();
    send(
        &mut tx,
        18,
        &Confirm {
            transcript_hash: hash,
        },
    )
    .await;
    let welcome: Welcome = receive(&mut rx, 19).await;
    assert_eq!(welcome.genesis, genesis);
    join_store
        .accept_join(
            postcard::to_stdvec(&welcome.genesis).unwrap(),
            PendingJoin {
                workspace,
                device: original_id,
                owner: owner_key.device_id(),
                owner_address: postcard::to_stdvec(&ticket.inviter).unwrap(),
                display_name: "Recovered profile".into(),
                invite_id: ticket.invite_id,
                transcript_hash: hash,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        join_store
            .workspace_snapshot(workspace)
            .await
            .unwrap()
            .members
            .len(),
        1
    );
    drop(tx);
    drop(rx);
    drop(connection);
    raw.close().await;
    owner.close().await;
    owner_store.clone().close().await.unwrap();
    join_store.close().await.unwrap();
    // Owner is offline immediately after Welcome, before any sync session. Kill the exact owned joiner child.
    let mut joiner = NodeChild::start(&join_dir, &dir.as_path().join("jh"));
    assert_eq!(joiner.request("status"), "Joining");
    joiner.child.kill().unwrap();
    joiner.child.wait().unwrap();
    drop(joiner);
    let mut joiner = NodeChild::start(&join_dir, &dir.as_path().join("jh"));
    assert_eq!(joiner.request("status"), "Joining");
    let mut owner = NodeChild::start(&owner_dir, &dir.as_path().join("oh"));
    owner.request("start");
    let mut ready = false;
    for _ in 0..250 {
        if joiner.request("status").get("Ready").is_some() {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(
        ready,
        "Pending join must recover without deleting the profile"
    );
    assert_eq!(
        FileKeyStore::new(join_dir.join("identity"))
            .load()
            .unwrap()
            .device_id(),
        original_id
    );
    let mut profile_received = false;
    for _ in 0..100 {
        let sql = rusqlite::Connection::open(owner_dir.join("meshyard.sqlite")).unwrap();
        let row = sql.query_row(
            "SELECT signed_blob FROM events WHERE author=?1 AND seq=1",
            [original_id.0.as_slice()],
            |r| r.get::<_, Vec<u8>>(0),
        );
        if let Ok(bytes) = row {
            let event =
                verify_and_decode(&decode_signed_event(&bytes).unwrap(), &Ed25519Verifier).unwrap();
            assert_eq!(event.author, original_id);
            assert_eq!(
                event.body,
                EventBody::ProfileUpdated {
                    display_name: "Recovered profile".into(),
                    avatar: 0
                }
            );
            profile_received = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(
        profile_received,
        "Recovered profile must sync from its author over the real authenticated socket"
    );
    assert!(joiner.request("pairings")["error"].is_null());
    joiner.request("quit");
    owner.request("quit");
    println!(
        "PASS: actual Welcome, offline owner, killed/reopened production-core joiner, unchanged file identity, real owner restoration and signed membership/profile sync"
    );
}
