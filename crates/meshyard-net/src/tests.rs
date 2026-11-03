#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
#[cfg(not(feature = "strict-only"))]
use iroh_relay::tls::CaTlsConfig;
use std::{
    collections::BTreeSet,
    net::{Ipv4Addr, SocketAddrV4},
};

struct Members {
    workspace: WorkspaceId,
    peers: BTreeSet<EndpointId>,
    pairing: bool,
}
impl Admission for Members {
    fn is_member(&self, workspace: WorkspaceId, peer: EndpointId) -> bool {
        workspace == self.workspace && self.peers.contains(&peer)
    }
    fn pairing_open(&self, workspace: WorkspaceId) -> bool {
        workspace == self.workspace && self.pairing
    }
}
const WORKSPACE: WorkspaceId = WorkspaceId([9; 16]);
fn config(seed: u8, peers: &[u8]) -> NetConfig {
    let mut cfg = NetConfig::new(
        SecretKey::from_bytes(&[seed; 32]),
        WORKSPACE,
        Mode::Strict,
        RelayChoice::Disabled,
    );
    cfg.bind_addr = Some(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)));
    cfg.mapping = MappingPolicy::Disabled;
    cfg.admission = Arc::new(Members {
        workspace: WORKSPACE,
        peers: peers
            .iter()
            .map(|n| SecretKey::from_bytes(&[*n; 32]).public())
            .collect(),
        pairing: false,
    });
    cfg
}
async fn node(seed: u8, peers: &[u8]) -> MeshNode {
    MeshNode::bind_inner(config(seed, peers), None, |b| b)
        .await
        .expect("loopback bind; EPERM is sandbox denial, not a closed port")
}
async fn raw(seed: u8) -> Endpoint {
    Endpoint::builder(presets::Minimal)
        .secret_key(SecretKey::from_bytes(&[seed; 32]))
        .clear_address_lookup()
        .relay_mode(RelayMode::Disabled)
        .portmapper_config(PortmapperConfig::Disabled)
        .net_report_config(NetReportConfig::minimal())
        .clear_ip_transports()
        .bind_addr((Ipv4Addr::LOCALHOST, 0))
        .unwrap()
        .bind()
        .await
        .expect("raw loopback endpoint")
}

#[tokio::test]
async fn direct_suite_selected_ip_and_real_ping() {
    let a = node(1, &[2]).await;
    let b = node(2, &[1]).await;
    assert!(a.endpoint.address_lookup().unwrap().is_empty());
    let ping = a.ping(b.address()).await.unwrap();
    assert_eq!(ping.path, SelectedPath::Direct);
    assert!(ping.sent_ms > 0 && ping.remote_ms > 0);
    assert_eq!(b.application_read_count(), 1);
    let diag = a.diagnostics();
    assert_eq!(diag.peers[0].path, SelectedPath::None);
    assert_eq!(diag.mode, Mode::Strict);
    assert_eq!(diag.port_mapping, PortMapping::Disabled);
    a.close().await;
    b.close().await;
}

#[tokio::test]
async fn direct_suite_stranger_all_alpns_zero_application_reads() {
    let b = node(4, &[]).await;
    for alpn in [PING_ALPN, SYNC_ALPN, PAIR_ALPN] {
        let a = raw(3).await;
        let connection = a.connect(b.address(), alpn).await.unwrap();
        // Send a valid-looking application frame immediately. Server admission cannot read it.
        if let Ok((mut send, _recv)) = connection.open_bi().await {
            let _ = send
                .write_all(b"MSHY\x01\x00\x00\x00\x00\x08payload!")
                .await;
        }
        let closed = timeout(HANDSHAKE_TIMEOUT, connection.closed())
            .await
            .unwrap();
        match closed {
            iroh::endpoint::ConnectionError::ApplicationClosed(reason) => {
                assert_eq!(reason.error_code, VarInt::from_u32(NOT_ADMITTED))
            }
            other => panic!("unexpected close: {other:?}"),
        }
        assert_eq!(b.application_read_count(), 0);
        a.close().await;
    }
    b.close().await;
}

#[tokio::test]
async fn direct_suite_future_handlers_default_closed_without_reads() {
    let b = node(6, &[5]).await;
    let a = raw(5).await;
    let connection = a.connect(b.address(), SYNC_ALPN).await.unwrap();
    let closed = timeout(HANDSHAKE_TIMEOUT, connection.closed())
        .await
        .unwrap();
    match closed {
        iroh::endpoint::ConnectionError::ApplicationClosed(reason) => {
            assert_eq!(reason.error_code, VarInt::from_u32(UNAVAILABLE))
        }
        other => panic!("unexpected close: {other:?}"),
    }
    assert_eq!(b.application_read_count(), 0);
    a.close().await;
    b.close().await;
}

struct WrongWorkspace;
impl ProtocolHandler for WrongWorkspace {
    fn handle(&self, connection: AdmittedConnection) -> HandlerFuture {
        Box::pin(async move {
            assert!(matches!(
                connection.for_workspace(WorkspaceId([8; 16])),
                Err(NetError::NotMember)
            ));
            Ok(())
        })
    }
}
#[tokio::test]
async fn direct_suite_workspace_gate_precedes_sync_reads() {
    let mut cfg = config(8, &[7]);
    cfg.sync_handler = Some(Arc::new(WrongWorkspace));
    let b = MeshNode::bind_inner(cfg, None, |b| b).await.unwrap();
    let a = raw(7).await;
    let connection = a.connect(b.address(), SYNC_ALPN).await.unwrap();
    let closed = timeout(HANDSHAKE_TIMEOUT, connection.closed())
        .await
        .unwrap();
    match closed {
        iroh::endpoint::ConnectionError::ApplicationClosed(reason) => {
            assert_eq!(reason.error_code, VarInt::from_u32(NOT_MEMBER))
        }
        other => panic!("unexpected close: {other:?}"),
    }
    assert_eq!(b.application_read_count(), 0);
    a.close().await;
    b.close().await;
}

#[tokio::test]
async fn strict_suite_no_services_and_relay_only_peer_rejected_without_dial() {
    let a = node(9, &[10]).await;
    let diagnostics = a.diagnostics();
    assert_eq!(diagnostics.home_relay, None);
    assert!(a.endpoint.address_lookup().unwrap().is_empty());
    assert!(!diagnostics.direct_addrs.is_empty());
    assert!(
        diagnostics
            .direct_addrs
            .iter()
            .all(|a| a.ip().is_loopback())
    );
    // Explicit loopback URL only, fresh endpoint, no cached peer or lookup service.
    let peer = EndpointAddr::new(SecretKey::from_bytes(&[10; 32]).public())
        .with_relay_url("http://127.0.0.1:9".parse().unwrap());
    assert!(matches!(a.ping(peer).await, Err(NetError::StrictAddress)));
    assert_eq!(a.application_read_count(), 0);
    a.close().await;
}

#[cfg(not(feature = "strict-only"))]
#[tokio::test]
async fn relay_suite_selected_relay_with_ip_transports_physically_absent() {
    let (map, url, _relay) = iroh::test_utils::run_relay_server_with(false)
        .await
        .expect("strictly loopback relay server");
    assert!(url.to_string().contains("127.0.0.1"));
    let mut ca = config(11, &[12]);
    ca.mode = Mode::Auto;
    ca.relay = RelayChoice::Custom(map.clone());
    let mut cb = config(12, &[11]);
    cb.mode = Mode::Auto;
    cb.relay = RelayChoice::Custom(map);
    // Cert bypass is private to this local generated-certificate test, never config input.
    let adjust = |b: iroh::endpoint::Builder| {
        b.clear_ip_transports()
            .clear_address_lookup()
            .ca_tls_config(CaTlsConfig::insecure_skip_verify())
    };
    let a = MeshNode::bind_inner(ca, None, adjust).await.unwrap();
    let b = MeshNode::bind_inner(cb, None, adjust).await.unwrap();
    assert!(a.endpoint.bound_sockets().is_empty());
    assert!(b.endpoint.bound_sockets().is_empty());
    assert!(a.endpoint.address_lookup().unwrap().is_empty());
    assert!(b.endpoint.address_lookup().unwrap().is_empty());
    // Only the local relay may be contacted. Readiness uses a bounded relay wait, not sleeps.
    timeout(HANDSHAKE_TIMEOUT, a.endpoint.online())
        .await
        .unwrap();
    timeout(HANDSHAKE_TIMEOUT, b.endpoint.online())
        .await
        .unwrap();
    let peer = EndpointAddr::new(b.address().id).with_relay_url(url.clone());
    assert!(peer.ip_addrs().next().is_none());
    let ping = a.ping(peer).await.unwrap();
    assert_eq!(ping.path, SelectedPath::Relayed { relay_url: url });
    assert_eq!(b.application_read_count(), 1);
    a.close().await;
    b.close().await;
}

#[tokio::test]
async fn direct_suite_bad_header_and_oversize_rejected_before_payload_read() {
    let b = node(14, &[13]).await;
    let a = raw(13).await;
    for bytes in [
        *b"MSHY\x02\x00\x00\x00\x00\x00",
        *b"MSHY\x01\x00\x00\x04\x00\x01",
        *b"MSHY\x01\x63\x00\x00\x00\x00",
    ] {
        let connection = a.connect(b.address(), PING_ALPN).await.unwrap();
        let (mut send, _recv) = connection.open_bi().await.unwrap();
        send.write_all(&bytes).await.unwrap();
        // No payload is sent: invalid header must cause immediate close, not a payload timeout.
        timeout(Duration::from_secs(2), connection.closed())
            .await
            .unwrap();
    }
    assert_eq!(b.application_read_count(), 3);
    a.close().await;
    b.close().await;
}

#[test]
fn config_secrecy_strict_relay_and_process_lease() {
    let mut cfg = config(20, &[]);
    let secret = cfg.secret_key.to_bytes();
    let printed = format!("{cfg:?}");
    assert!(!printed.contains(&format!("{secret:?}")));
    assert!(!printed.contains("secret_key"));
    cfg.relay = RelayChoice::Custom(RelayMap::empty());
    assert_eq!(cfg.validate(), Err(NetError::StrictRelay));
    let lease = ProcessLease::acquire().unwrap();
    assert!(matches!(
        ProcessLease::acquire(),
        Err(NetError::ActiveEndpoint)
    ));
    drop(lease);
    assert!(ProcessLease::acquire().is_ok());
}
#[cfg(feature = "strict-only")]
#[tokio::test]
async fn strict_only_rejects_auto_before_socket_bind() {
    let mut cfg = config(21, &[]);
    cfg.mode = Mode::Auto;
    cfg.relay = RelayChoice::Custom(RelayMap::empty());
    assert!(matches!(
        MeshNode::bind(cfg).await,
        Err(NetError::AutoDisabled)
    ));
}

struct HoldConnections(tokio::sync::mpsc::Sender<()>);
impl ProtocolHandler for HoldConnections {
    fn handle(&self, connection: AdmittedConnection) -> HandlerFuture {
        let ready = self.0.clone();
        Box::pin(async move {
            let _connection = connection;
            ready.send(()).await.map_err(|_| NetError::Closed)?;
            std::future::pending::<Result<(), NetError>>().await
        })
    }
}
#[tokio::test]
async fn direct_suite_incoming_device_connection_cap() {
    let (tx, mut rx) = tokio::sync::mpsc::channel(4);
    let mut cfg = config(23, &[22]);
    cfg.sync_handler = Some(Arc::new(HoldConnections(tx)));
    let b = MeshNode::bind_inner(cfg, None, |b| b).await.unwrap();
    let a = raw(22).await;
    let mut held = Vec::new();
    for _ in 0..4 {
        held.push(a.connect(b.address(), SYNC_ALPN).await.unwrap());
        timeout(HANDSHAKE_TIMEOUT, rx.recv())
            .await
            .unwrap()
            .unwrap();
    }
    let fifth = a.connect(b.address(), SYNC_ALPN).await.unwrap();
    let closed = timeout(HANDSHAKE_TIMEOUT, fifth.closed()).await.unwrap();
    match closed {
        iroh::endpoint::ConnectionError::ApplicationClosed(reason) => {
            assert_eq!(reason.error_code, VarInt::from_u32(LIMIT_EXCEEDED))
        }
        other => panic!("unexpected close: {other:?}"),
    }
    assert_eq!(b.application_read_count(), 0);
    drop(held);
    a.close().await;
    b.close().await;
}

struct MapEvents(tokio::sync::mpsc::Receiver<portmap::MappingEvent>);
impl portmap::MappingSource for MapEvents {
    fn next(&mut self) -> Pin<Box<dyn Future<Output = Option<portmap::MappingEvent>> + Send + '_>> {
        Box::pin(self.0.recv())
    }
    fn restart(&mut self) {}
    fn deactivate(&mut self) {}
}
async fn wait_address(endpoint: &Endpoint, addr: SocketAddr, present: bool) {
    use iroh::Watcher;
    let mut watcher = endpoint.watch_addr();
    timeout(HANDSHAKE_TIMEOUT, async {
        loop {
            if watcher.get().ip_addrs().any(|a| *a == addr) == present {
                break;
            }
            watcher.updated().await.unwrap();
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn direct_suite_injected_mapping_updates_actual_ticket_candidates() {
    let mut a = node(24, &[]).await;
    let (tx, rx) = tokio::sync::mpsc::channel(8);
    let (state_tx, state) = watch::channel(PortMapping::Disabled);
    a.mapping = state;
    let mut mapper = MapperTask::spawn(MapEvents(rx), Arc::new(a.endpoint.clone()), state_tx);
    let external = SocketAddrV4::new(Ipv4Addr::new(192, 0, 2, 24), 4242);
    let address = SocketAddr::V4(external);
    tx.send(portmap::MappingEvent::External(Some(external)))
        .await
        .unwrap();
    wait_address(&a.endpoint, address, true).await;
    assert!(a.diagnostics().direct_addrs.contains(&address));
    tx.send(portmap::MappingEvent::External(None))
        .await
        .unwrap();
    wait_address(&a.endpoint, address, false).await;
    tx.send(portmap::MappingEvent::External(Some(external)))
        .await
        .unwrap();
    wait_address(&a.endpoint, address, true).await;
    tx.send(portmap::MappingEvent::NetworkChanged)
        .await
        .unwrap();
    wait_address(&a.endpoint, address, false).await;
    tx.send(portmap::MappingEvent::External(Some(external)))
        .await
        .unwrap();
    wait_address(&a.endpoint, address, true).await;
    mapper.shutdown().await;
    wait_address(&a.endpoint, address, false).await;
    a.close().await;
}

#[test]
fn ping_payloads_use_postcard_and_reject_trailing_bytes() {
    let ping = postcard::to_stdvec(&PingV1 { sent_ms: 1 }).unwrap();
    assert_eq!(ping, [1]);
    let pong = postcard::to_stdvec(&PongV1 {
        sent_ms: 1,
        remote_ms: 2,
    })
    .unwrap();
    assert_eq!(pong, [1, 2]);
    assert!(matches!(
        decode_payload::<PingV1>(&[1, 0]),
        Err(NetError::Frame)
    ));
    assert!(matches!(
        decode_payload::<PongV1>(&[1]),
        Err(NetError::Frame)
    ));
}
