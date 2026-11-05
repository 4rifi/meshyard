//! Authenticated, bounded iroh routing for one active workspace.
//! Pairing (T-005) and sync (T-006) are default-closed extension boundaries.
mod frame;
mod limits;
mod portmap;

pub use frame::{FrameStream, HANDSHAKE_TIMEOUT, IDLE_TIMEOUT, PAIR_MAX_FRAME, PAIR_TIMEOUT};
use futures_util::StreamExt;
use iroh::{
    Endpoint, RelayMode, TransportAddr,
    endpoint::{
        Connection, NetReportConfig, PathList, PortmapperConfig, QuicTransportConfig, VarInt,
        presets,
    },
};
pub use iroh::{EndpointAddr, EndpointId, RelayMap, RelayUrl, SecretKey};
use limits::{ConnectionPermit, Limits, MAX_TOTAL, lock};
pub use meshyard_core::domain::{Mode, WorkspaceId};
use portmap::MapperTask;
use std::{
    collections::BTreeMap,
    fmt,
    future::{Future, IntoFuture},
    net::{SocketAddr, SocketAddrV4},
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, SystemTime},
};
use tokio::{
    sync::{Semaphore, watch},
    task::{JoinHandle, JoinSet},
    time::{Instant, timeout},
};

pub const PAIR_ALPN: &[u8] = b"meshyard/pair/1";
pub const SYNC_ALPN: &[u8] = b"meshyard/sync/1";
pub const PING_ALPN: &[u8] = b"meshyard/ping/1";
pub const NOT_ADMITTED: u32 = 1;
pub const NOT_MEMBER: u32 = 2;
pub const LIMIT_EXCEEDED: u32 = 3;
const UNAVAILABLE: u32 = 4;
const PING: u8 = 0;
const PONG: u8 = 1;
#[derive(serde::Serialize, serde::Deserialize)]
struct PingV1 {
    sent_ms: u64,
}
#[derive(serde::Serialize, serde::Deserialize)]
struct PongV1 {
    sent_ms: u64,
    remote_ms: u64,
}
fn decode_payload<'a, T: serde::Deserialize<'a>>(bytes: &'a [u8]) -> Result<T, NetError> {
    let (value, rest) = postcard::take_from_bytes(bytes).map_err(|_| NetError::Frame)?;
    if !rest.is_empty() {
        return Err(NetError::Frame);
    }
    Ok(value)
}
static PROCESS_ENDPOINT: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    Pair,
    Sync,
    Ping,
}
impl Protocol {
    fn alpn(self) -> &'static [u8] {
        match self {
            Self::Pair => PAIR_ALPN,
            Self::Sync => SYNC_ALPN,
            Self::Ping => PING_ALPN,
        }
    }
    fn from_alpn(alpn: &[u8]) -> Option<Self> {
        match alpn {
            PAIR_ALPN => Some(Self::Pair),
            SYNC_ALPN => Some(Self::Sync),
            PING_ALPN => Some(Self::Ping),
            _ => None,
        }
    }
}

/// Implemented by the active workspace's membership/invite store. No speculative membership.
pub trait Admission: Send + Sync + 'static {
    fn is_member(&self, workspace: WorkspaceId, peer: EndpointId) -> bool;
    fn pairing_open(&self, workspace: WorkspaceId) -> bool;
}
pub struct DenyAll;
impl Admission for DenyAll {
    fn is_member(&self, _: WorkspaceId, _: EndpointId) -> bool {
        false
    }
    fn pairing_open(&self, _: WorkspaceId) -> bool {
        false
    }
}

pub type HandlerFuture = Pin<Box<dyn Future<Output = Result<(), NetError>> + Send>>;
/// T-005/T-006 integration point. The connection exposes only bounded streams.
/// Sync streams additionally require `for_workspace`, enforcing the second admission stage.
pub trait ProtocolHandler: Send + Sync + 'static {
    fn handle(&self, connection: AdmittedConnection) -> HandlerFuture;
}

pub enum RelayChoice {
    Disabled,
    #[cfg(not(feature = "strict-only"))]
    N0Default,
    Custom(RelayMap),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MappingPolicy {
    Router,
    Disabled,
}
/// Intentionally no derived Debug: contains the device secret and caller-owned admission state.
pub struct NetConfig {
    pub secret_key: SecretKey,
    pub workspace: WorkspaceId,
    pub mode: Mode,
    pub relay: RelayChoice,
    pub bind_addr: Option<SocketAddr>,
    pub mapping: MappingPolicy,
    pub admission: Arc<dyn Admission>,
    pub pair_handler: Option<Arc<dyn ProtocolHandler>>,
    pub sync_handler: Option<Arc<dyn ProtocolHandler>>,
}
impl NetConfig {
    pub fn new(
        secret_key: SecretKey,
        workspace: WorkspaceId,
        mode: Mode,
        relay: RelayChoice,
    ) -> Self {
        Self {
            secret_key,
            workspace,
            mode,
            relay,
            bind_addr: None,
            mapping: MappingPolicy::Router,
            admission: Arc::new(DenyAll),
            pair_handler: None,
            sync_handler: None,
        }
    }
    fn validate(&self) -> Result<(), NetError> {
        if cfg!(feature = "strict-only") && self.mode == Mode::Auto {
            return Err(NetError::AutoDisabled);
        }
        if self.mode == Mode::Strict && !matches!(self.relay, RelayChoice::Disabled) {
            return Err(NetError::StrictRelay);
        }
        if self.mode == Mode::Auto && matches!(self.relay, RelayChoice::Disabled) {
            return Err(NetError::RelayRequired);
        }
        Ok(())
    }
}
impl fmt::Debug for NetConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NetConfig")
            .field("mode", &self.mode)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum NetError {
    #[error(
        "Network endpoint could not bind; check local socket permissions and port availability"
    )]
    Bind,
    #[error("Only one active workspace endpoint is permitted; close it before rebinding")]
    ActiveEndpoint,
    #[error("Auto is unavailable in this Strict-only build")]
    AutoDisabled,
    #[error("Strict requires disabled relays; choose a direct address")]
    StrictRelay,
    #[error("Auto requires an explicit relay choice")]
    RelayRequired,
    #[error("Strict: no direct address; obtain current direct addresses from the peer's invite")]
    StrictAddress,
    #[error(
        "Strict: no direct path observed within 10 s; possible causes include NAT without port mapping or a firewall"
    )]
    StrictUnreachable,
    #[error("Connection failed; verify the peer's reachability and configured relay")]
    Connect,
    #[error("Device is not admitted to the active workspace")]
    NotAdmitted,
    #[error("Device is not a member of this workspace")]
    NotMember,
    #[error("Device connection or stream limit exceeded")]
    Limit,
    #[error("Device ingress rate exceeded")]
    RateLimited,
    #[error("Invalid, oversized or incomplete protocol frame")]
    Frame,
    #[error("Protocol deadline expired")]
    Timeout,
    #[error("Early application data is disabled")]
    EarlyData,
    #[error("Endpoint or connection is closed")]
    Closed,
    #[error("This protocol handler is not available yet")]
    HandlerUnavailable,
    #[error("Port mapping requires a bound IPv4 UDP port")]
    MappingPort,
    #[error("Cannot observe network changes; port mapping was not started")]
    MappingMonitor,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectedPath {
    Direct,
    Relayed { relay_url: RelayUrl },
    None,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PortMapping {
    Disabled,
    Probing,
    Mapped { external: SocketAddrV4 },
    NotObserved { after_secs: u64 },
}
#[derive(Clone, Debug)]
pub struct PeerDiagnostics {
    pub endpoint_id: EndpointId,
    /// Local selected send path, not a claim about every received byte.
    pub path: SelectedPath,
    pub rtt: Option<Duration>,
    pub last_error: Option<NetError>,
    pub last_seen: SystemTime,
}
#[derive(Clone, Debug)]
pub struct NodeDiagnostics {
    pub mode: Mode,
    pub home_relay: Option<RelayUrl>,
    pub direct_addrs: Vec<SocketAddr>,
    pub port_mapping: PortMapping,
    pub peers: Vec<PeerDiagnostics>,
}
#[derive(Clone, Debug)]
pub struct PingResult {
    pub sent_ms: u64,
    pub remote_ms: u64,
    pub rtt: Duration,
    /// Selected local send path sampled when the request was sent.
    pub path: SelectedPath,
}

struct LivePath {
    peer: EndpointId,
    path: SelectedPath,
    rtt: Option<Duration>,
}

struct State {
    workspace: WorkspaceId,
    mode: Mode,
    admission: Arc<dyn Admission>,
    limits: Arc<Limits>,
    pairing: Arc<Semaphore>,
    reads: Arc<AtomicU64>,
    peers: Mutex<BTreeMap<EndpointId, PeerDiagnostics>>,
    live_paths: Mutex<BTreeMap<usize, LivePath>>,
}
impl State {
    fn admitted(&self, peer: EndpointId, protocol: Protocol) -> bool {
        match protocol {
            Protocol::Pair => self.admission.pairing_open(self.workspace),
            _ => self.admission.is_member(self.workspace, peer),
        }
    }
    fn register(&self, connection: &Connection) {
        lock(&self.live_paths).insert(
            connection.stable_id(),
            LivePath {
                peer: connection.remote_id(),
                path: SelectedPath::None,
                rtt: None,
            },
        );
        self.observe(connection, &connection.paths());
    }
    fn observe(&self, connection: &Connection, paths: &PathList<'_>) {
        let peer = connection.remote_id();
        let (path, rtt) = selected(paths);
        let mut live = lock(&self.live_paths);
        let Some(entry) = live.get_mut(&connection.stable_id()) else {
            return;
        };
        entry.path = path.clone();
        entry.rtt = rtt;
        // Hold the live-map lock through the diagnostics update. A late watcher
        // snapshot cannot resurrect a connection after disconnected() removes it.
        let mut peers = lock(&self.peers);
        if peers.len() >= MAX_TOTAL
            && !peers.contains_key(&peer)
            && let Some(key) = peers
                .iter()
                .min_by_key(|(_, d)| d.last_seen)
                .map(|(k, _)| *k)
        {
            peers.remove(&key);
        }
        let last_error = peers.get(&peer).and_then(|d| d.last_error);
        peers.insert(
            peer,
            PeerDiagnostics {
                endpoint_id: peer,
                path,
                rtt,
                last_error,
                last_seen: SystemTime::now(),
            },
        );
    }
    fn disconnected(&self, connection: &Connection) {
        let mut live = lock(&self.live_paths);
        live.remove(&connection.stable_id());
        let remaining = live
            .values()
            .find(|entry| entry.peer == connection.remote_id());
        if let Some(d) = lock(&self.peers).get_mut(&connection.remote_id()) {
            d.path = remaining
                .map(|entry| entry.path.clone())
                .unwrap_or(SelectedPath::None);
            d.rtt = remaining.and_then(|entry| entry.rtt);
        }
    }
    fn error(&self, peer: EndpointId, error: NetError) {
        if let Some(d) = lock(&self.peers).get_mut(&peer) {
            d.last_error = Some(error);
        }
    }
}
fn selected(paths: &PathList<'_>) -> (SelectedPath, Option<Duration>) {
    paths
        .iter()
        .find(|p| p.is_selected())
        .map(|p| {
            let kind = if p.is_ip() {
                SelectedPath::Direct
            } else if let TransportAddr::Relay(url) = p.remote_addr() {
                SelectedPath::Relayed {
                    relay_url: url.clone(),
                }
            } else {
                SelectedPath::None
            };
            (kind, Some(p.rtt()))
        })
        .unwrap_or((SelectedPath::None, None))
}

struct ProcessLease;
impl ProcessLease {
    fn acquire() -> Result<Self, NetError> {
        PROCESS_ENDPOINT
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| NetError::ActiveEndpoint)?;
        Ok(Self)
    }
}
impl Drop for ProcessLease {
    fn drop(&mut self) {
        PROCESS_ENDPOINT.store(false, Ordering::Release);
    }
}

/// Owns endpoint, router and mapper. Mode/workspace are immutable; close and rebind to change.
/// No raw Endpoint is exposed, so runtime relay/lookup mutation cannot weaken Strict.
pub struct MeshNode {
    endpoint: Endpoint,
    state: Arc<State>,
    server: Option<JoinHandle<()>>,
    mapper: Option<MapperTask>,
    mapping: watch::Receiver<PortMapping>,
    lease: Option<ProcessLease>,
}
impl fmt::Debug for MeshNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MeshNode")
            .field("mode", &self.state.mode)
            .finish_non_exhaustive()
    }
}
impl MeshNode {
    pub async fn bind(config: NetConfig) -> Result<Self, NetError> {
        config.validate()?;
        let lease = ProcessLease::acquire()?;
        Self::bind_inner(config, Some(lease), |b| b).await
    }
    async fn bind_inner(
        config: NetConfig,
        lease: Option<ProcessLease>,
        adjust: impl FnOnce(iroh::endpoint::Builder) -> iroh::endpoint::Builder,
    ) -> Result<Self, NetError> {
        config.validate()?;
        let mut builder = Endpoint::builder(presets::Minimal);
        match &config.relay {
            RelayChoice::Disabled => builder = builder.relay_mode(RelayMode::Disabled),
            #[cfg(not(feature = "strict-only"))]
            RelayChoice::N0Default => builder = builder.preset(presets::N0),
            RelayChoice::Custom(map) => {
                builder = builder
                    .relay_mode(RelayMode::Custom(map.clone()))
                    .clear_address_lookup()
            }
        }
        if config.mode == Mode::Strict {
            builder = builder
                .clear_address_lookup()
                .net_report_config(NetReportConfig::minimal());
        } else {
            // Measure only configured relays, including relays without QUIC discovery.
            let mut report = NetReportConfig::minimal();
            report.https_probes = true;
            report.captive_portal_check = false;
            builder = builder.net_report_config(report);
        }
        builder = builder
            .portmapper_config(PortmapperConfig::Disabled)
            .secret_key(config.secret_key)
            .max_tls_tickets(0)
            .alpns(vec![
                PAIR_ALPN.to_vec(),
                SYNC_ALPN.to_vec(),
                PING_ALPN.to_vec(),
            ])
            .transport_config(
                QuicTransportConfig::builder()
                    .max_concurrent_bidi_streams(VarInt::from_u32(16))
                    .max_concurrent_uni_streams(VarInt::from_u32(0))
                    .stream_receive_window(VarInt::from_u32(256 * 1024 + 10))
                    .receive_window(VarInt::from_u32(4 * 1024 * 1024))
                    .send_window(4 * 1024 * 1024)
                    .datagram_receive_buffer_size(None)
                    .keep_alive_interval(Duration::from_secs(30))
                    .max_idle_timeout(Some(
                        Duration::from_secs(120)
                            .try_into()
                            .map_err(|_| NetError::Bind)?,
                    ))
                    .build(),
            );
        if let Some(addr) = config.bind_addr {
            // Remove both defaults so loopback tests cannot bind another interface implicitly.
            builder = builder
                .clear_ip_transports()
                .bind_addr(addr)
                .map_err(|_| NetError::Bind)?;
        }
        let endpoint = adjust(builder).bind().await.map_err(|error| {
            #[cfg(test)]
            eprintln!("test endpoint bind failed: {error:?}");
            #[cfg(not(test))]
            let _ = error;
            NetError::Bind
        })?;
        let (mapping_tx, mapping) = watch::channel(PortMapping::Disabled);
        let mapper = if config.mapping == MappingPolicy::Router {
            match MapperTask::router(endpoint.clone(), mapping_tx).await {
                Ok(task) => Some(task),
                Err(e) => {
                    endpoint.close().await;
                    return Err(e);
                }
            }
        } else {
            None
        };
        let state = Arc::new(State {
            workspace: config.workspace,
            mode: config.mode,
            admission: config.admission,
            limits: Arc::new(Limits::default()),
            pairing: Arc::new(Semaphore::new(2)),
            reads: Arc::new(AtomicU64::new(0)),
            peers: Mutex::new(BTreeMap::new()),
            live_paths: Mutex::new(BTreeMap::new()),
        });
        let server = tokio::spawn(serve(
            endpoint.clone(),
            state.clone(),
            config.pair_handler,
            config.sync_handler,
        ));
        Ok(Self {
            endpoint,
            state,
            server: Some(server),
            mapper,
            mapping,
            lease,
        })
    }
    pub fn address(&self) -> EndpointAddr {
        self.endpoint.addr()
    }
    pub fn diagnostics(&self) -> NodeDiagnostics {
        let addr = self.endpoint.addr();
        NodeDiagnostics {
            mode: self.state.mode,
            home_relay: addr.relay_urls().next().cloned(),
            direct_addrs: addr.ip_addrs().copied().collect(),
            port_mapping: self.mapping.borrow().clone(),
            peers: lock(&self.state.peers).values().cloned().collect(),
        }
    }
    /// Instrumentation counts actual application header reads, after admission.
    pub fn application_read_count(&self) -> u64 {
        self.state.reads.load(Ordering::Relaxed)
    }
    pub async fn connect(
        &self,
        peer: EndpointAddr,
        protocol: Protocol,
    ) -> Result<AdmittedConnection, NetError> {
        if self.endpoint.is_closed() {
            return Err(NetError::Closed);
        }
        if !self.state.admitted(peer.id, protocol) {
            return Err(NetError::NotAdmitted);
        }
        if self.state.mode == Mode::Strict && peer.ip_addrs().next().is_none() {
            return Err(NetError::StrictAddress);
        }
        let permit = self.state.limits.connection(peer.id)?;
        let pairing = if protocol == Protocol::Pair {
            Some(
                self.state
                    .pairing
                    .clone()
                    .try_acquire_owned()
                    .map_err(|_| NetError::Limit)?,
            )
        } else {
            None
        };
        let connection = timeout(
            HANDSHAKE_TIMEOUT,
            self.endpoint.connect(peer.clone(), protocol.alpn()),
        )
        .await
        .map_err(|_| {
            if self.state.mode == Mode::Strict {
                NetError::StrictUnreachable
            } else {
                NetError::Timeout
            }
        })?
        .map_err(|_| NetError::Connect)?;
        // Both directions await a full authenticated handshake; there is no into_0rtt caller.
        if !self.state.admitted(connection.remote_id(), protocol) {
            connection.close(VarInt::from_u32(NOT_ADMITTED), b"NOT_ADMITTED");
            return Err(NetError::NotAdmitted);
        }
        Ok(AdmittedConnection::new(
            connection,
            self.state.clone(),
            permit,
            protocol,
            pairing,
        ))
    }
    pub async fn ping(&self, peer: EndpointAddr) -> Result<PingResult, NetError> {
        let session = self.connect(peer, Protocol::Ping).await?;
        let started = Instant::now();
        let sent_ms = timestamp_ms();
        let mut stream = session.open_stream().await?;
        let payload = postcard::to_stdvec(&PingV1 { sent_ms }).map_err(|_| NetError::Frame)?;
        let path = session.selected_path();
        stream.write(PING, &payload).await?;
        stream.finish()?;
        let (_, bytes) = stream.read(&[PONG]).await?;
        let pong: PongV1 = decode_payload(&bytes)?;
        if pong.sent_ms != sent_ms {
            return Err(NetError::Frame);
        }
        let remote_ms = pong.remote_ms;
        let rtt = started.elapsed();
        self.state
            .observe(&session.connection, &session.connection.paths());
        Ok(PingResult {
            sent_ms,
            remote_ms,
            rtt,
            path,
        })
    }
    pub async fn close(mut self) {
        self.stop().await;
        self.lease.take();
    }
    async fn stop(&mut self) {
        if let Some(mut mapper) = self.mapper.take() {
            mapper.shutdown().await;
        }
        if let Some(server) = self.server.take() {
            server.abort();
            let _ = server.await;
        }
        self.endpoint.close().await;
    }
}
impl Drop for MeshNode {
    fn drop(&mut self) {
        if self.server.is_none() && self.mapper.is_none() && self.lease.is_none() {
            return;
        }
        if let Some(server) = self.server.take() {
            server.abort();
        }
        let mut mapper = self.mapper.take();
        let endpoint = self.endpoint.clone();
        let lease = self.lease.take();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            // Retain the process lease until mapper cleanup and endpoint shutdown finish.
            runtime.spawn(async move {
                if let Some(task) = mapper.as_mut() {
                    task.shutdown().await;
                }
                endpoint.close().await;
                drop(lease);
            });
        } else {
            drop(mapper);
            drop(endpoint);
            // No runtime can complete async close. Fail closed rather than permit a
            // new endpoint while shutdown is unobserved. Call close before teardown.
            if let Some(lease) = lease {
                std::mem::forget(lease);
            }
        }
    }
}

/// Opaque authenticated session. No frame read API exists for ungated sync connections.
pub struct AdmittedConnection {
    connection: Connection,
    state: Arc<State>,
    permit: Arc<ConnectionPermit>,
    protocol: Protocol,
    _pairing: Option<tokio::sync::OwnedSemaphorePermit>,
    path_task: JoinHandle<()>,
}
impl AdmittedConnection {
    fn new(
        connection: Connection,
        state: Arc<State>,
        permit: Arc<ConnectionPermit>,
        protocol: Protocol,
        pairing: Option<tokio::sync::OwnedSemaphorePermit>,
    ) -> Self {
        state.register(&connection);
        let watched = connection.clone();
        let observer = state.clone();
        let path_task = tokio::spawn(async move {
            let mut paths = watched.paths_stream();
            while let Some(snapshot) = paths.next().await {
                observer.observe(&watched, &snapshot);
            }
            observer.disconnected(&watched);
        });
        Self {
            connection,
            state,
            permit,
            protocol,
            _pairing: pairing,
            path_task,
        }
    }
    pub fn selected_path(&self) -> SelectedPath {
        selected(&self.connection.paths()).0
    }
    pub fn peer(&self) -> EndpointId {
        self.connection.remote_id()
    }
    pub fn protocol(&self) -> Protocol {
        self.protocol
    }
    pub fn for_workspace(self, workspace: WorkspaceId) -> Result<WorkspaceConnection, NetError> {
        if workspace != self.state.workspace
            || !self.state.admission.is_member(workspace, self.peer())
        {
            self.connection
                .close(VarInt::from_u32(NOT_MEMBER), b"NOT_MEMBER");
            return Err(NetError::NotMember);
        }
        Ok(WorkspaceConnection(self))
    }
    pub async fn accept_pair_stream(&self) -> Result<FrameStream, NetError> {
        if self.protocol != Protocol::Pair
            || !self.state.admission.pairing_open(self.state.workspace)
        {
            return Err(NetError::NotAdmitted);
        }
        self.accept_stream().await
    }
    async fn accept_stream(&self) -> Result<FrameStream, NetError> {
        let (send, recv) = timeout(
            if self.protocol == Protocol::Pair {
                PAIR_TIMEOUT
            } else {
                HANDSHAKE_TIMEOUT
            },
            self.connection.accept_bi(),
        )
        .await
        .map_err(|_| NetError::Timeout)?
        .map_err(|_| NetError::Closed)?;
        FrameStream::new(
            send,
            recv,
            self.permit.clone(),
            self.state.reads.clone(),
            self.protocol == Protocol::Pair,
        )
    }
    async fn open_stream(&self) -> Result<FrameStream, NetError> {
        let (send, recv) = timeout(HANDSHAKE_TIMEOUT, self.connection.open_bi())
            .await
            .map_err(|_| NetError::Timeout)?
            .map_err(|_| NetError::Closed)?;
        FrameStream::new(
            send,
            recv,
            self.permit.clone(),
            self.state.reads.clone(),
            self.protocol == Protocol::Pair,
        )
    }
}
impl Drop for AdmittedConnection {
    fn drop(&mut self) {
        self.path_task.abort();
        self.state.disconnected(&self.connection);
        self.connection
            .close(VarInt::from_u32(0), b"SESSION_CLOSED");
    }
}
pub struct WorkspaceConnection(AdmittedConnection);
impl WorkspaceConnection {
    pub fn peer(&self) -> EndpointId {
        self.0.peer()
    }
    pub async fn accept_stream(&self) -> Result<FrameStream, NetError> {
        self.0.accept_stream().await
    }
    pub async fn open_stream(&self) -> Result<FrameStream, NetError> {
        self.0.open_stream().await
    }
}

async fn serve(
    endpoint: Endpoint,
    state: Arc<State>,
    pair: Option<Arc<dyn ProtocolHandler>>,
    sync: Option<Arc<dyn ProtocolHandler>>,
) {
    let mut tasks = JoinSet::new();
    loop {
        tokio::select! {
            incoming = endpoint.accept() => {
                let Some(incoming) = incoming else { break; };
                if tasks.len() >= MAX_TOTAL { incoming.refuse(); continue; }
                let state = state.clone();
                let pair = pair.clone();
                let sync = sync.clone();
                tasks.spawn(async move {
                    // No early-data conversion: remote identity is authenticated before routing.
                    let Ok(Ok(connection)) = timeout(HANDSHAKE_TIMEOUT, incoming.into_future()).await else { return; };
                    let Some(protocol) = Protocol::from_alpn(connection.alpn()) else { connection.close(VarInt::from_u32(NOT_ADMITTED), b"NOT_ADMITTED"); return; };
                    if !state.admitted(connection.remote_id(), protocol) {
                        connection.close(VarInt::from_u32(NOT_ADMITTED), b"NOT_ADMITTED"); return;
                    }
                    let Ok(permit) = state.limits.connection(connection.remote_id()) else { connection.close(VarInt::from_u32(LIMIT_EXCEEDED), b"LIMIT_EXCEEDED"); return; };
                    let pairing = if protocol == Protocol::Pair {
                        match state.pairing.clone().try_acquire_owned() { Ok(p) => Some(p), Err(_) => { connection.close(VarInt::from_u32(LIMIT_EXCEEDED), b"LIMIT_EXCEEDED"); return; } }
                    } else { None };
                    let session = AdmittedConnection::new(connection, state.clone(), permit, protocol, pairing);
                    let peer = session.peer();
                    let result = match protocol {
                        Protocol::Ping => serve_ping(session).await,
                        Protocol::Pair | Protocol::Sync => {
                            match if protocol == Protocol::Pair { pair } else { sync } {
                                Some(h) => h.handle(session).await,
                                None => { session.connection.close(VarInt::from_u32(UNAVAILABLE), b"HANDLER_UNAVAILABLE"); Err(NetError::HandlerUnavailable) }
                            }
                        }
                    };
                    if let Err(error) = result { state.error(peer, error); }
                });
            }
            _ = tasks.join_next(), if !tasks.is_empty() => {}
        }
    }
}
async fn serve_ping(session: AdmittedConnection) -> Result<(), NetError> {
    let mut stream = session.accept_stream().await?;
    let (_, bytes) = stream.read(&[PING]).await?;
    let ping: PingV1 = decode_payload(&bytes)?;
    let response = postcard::to_stdvec(&PongV1 {
        sent_ms: ping.sent_ms,
        remote_ms: timestamp_ms(),
    })
    .map_err(|_| NetError::Frame)?;
    stream.write(PONG, &response).await?;
    stream.finish()?;
    stream.acknowledged().await
}
fn timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis().min(u64::MAX as u128) as u64)
        .unwrap_or_default()
}
pub const fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod tls_tests;
