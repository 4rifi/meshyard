//! Explicit opt-in network actions, shared by native IPC and the headless process harness.
use crate::{state::AppState, view::AppError};
use meshyard_core::WorkspaceId;
use meshyard_net::{
    EndpointAddr, MeshNode, NetConfig, NetError, Protocol, RelayChoice,
    pairing::{PairingService, Ticket},
    sync::SyncService,
};
use serde::Serialize;
use std::{sync::Arc, time::Duration};
use tokio::{sync::Mutex, task::JoinHandle};
use ts_rs::TS;

#[derive(Clone, Debug, Serialize, TS)]
pub struct NetworkView {
    pub running: bool,
    pub mode: String,
    pub peers: Vec<PeerView>,
    pub pairings: Vec<PairView>,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Serialize, TS)]
pub struct PeerView {
    pub device_short: String,
    pub path: String,
}
#[derive(Clone, Debug, Serialize, TS)]
pub struct PairView {
    pub id: String,
    pub peer_short: String,
    pub words: Vec<String>,
    pub is_inviter: bool,
    pub local_confirmed: bool,
    pub state: String,
}
pub(crate) struct Network {
    node: Arc<MeshNode>,
    service: Arc<PairingService>,
    tasks: Vec<JoinHandle<()>>,
    error: Arc<Mutex<Option<String>>>,
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn network_error(_: NetError) -> AppError {
    AppError::NetworkUnavailable
}
fn storage_error(_: meshyard_store::StoreError) -> AppError {
    AppError::Storage
}
fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, AppError> {
    let (value, rest) = postcard::take_from_bytes(bytes).map_err(|_| AppError::Storage)?;
    if !rest.is_empty() {
        return Err(AppError::Storage);
    }
    Ok(value)
}
async fn bind(state: &AppState, workspace: WorkspaceId) -> Result<Network, AppError> {
    let (store, signer) = {
        let guard = state.0.lock().await;
        let core = guard.as_ref().map_err(Clone::clone)?;
        (core.store.clone(), core.signer.clone())
    };
    if let Ok(core) = state.0.lock().await.as_mut() {
        core.network_error = None;
    }
    let service = PairingService::new(store, signer.clone(), workspace);
    if service
        .store
        .workspaces()
        .await
        .map_err(storage_error)?
        .contains(&workspace)
    {
        service.refresh().await.map_err(network_error)?;
    }
    let secret = signer.with_secret_bytes(meshyard_net::SecretKey::from_bytes);
    let mut config = NetConfig::new(
        secret,
        workspace,
        meshyard_core::Mode::Strict,
        RelayChoice::Disabled,
    );
    if std::env::var_os("MESHYARD_TEST_DISABLE_MAPPING").as_deref()
        == Some(std::ffi::OsStr::new("1"))
    {
        config.mapping = meshyard_net::MappingPolicy::Disabled;
    }
    config.bind_port = service.store.endpoint_port().await.map_err(storage_error)?;
    config.admission = service.clone();
    config.pair_handler = Some(Arc::new(service.clone()));
    config.sync_handler = Some(Arc::new(SyncService::new(service.clone())));
    let node = Arc::new(MeshNode::bind(config).await.map_err(network_error)?);
    if let Some(port) = node.ipv4_port() {
        service
            .store
            .save_endpoint_port(port)
            .await
            .map_err(storage_error)?;
    }
    Ok(Network {
        node,
        service,
        tasks: Vec::new(),
        error: Arc::new(Mutex::new(None)),
    })
}
pub async fn start(state: &AppState) -> Result<NetworkView, AppError> {
    let mut runtime = state.3.lock().await;
    if runtime.is_none() {
        let workspace = {
            let guard = state.0.lock().await;
            guard
                .as_ref()
                .map_err(Clone::clone)?
                .workspace
                .ok_or(AppError::NoWorkspace)?
        };
        let mut network = bind(state, workspace).await?;
        let mut addresses = network
            .service
            .store
            .peer_addresses(workspace)
            .await
            .map_err(storage_error)?;
        if let Some(p) = network
            .service
            .store
            .pending_join(workspace, network.service.signer.device_id())
            .await
            .map_err(storage_error)?
            && !addresses.contains(&p.owner_address)
        {
            addresses.push(p.owner_address);
        }
        for raw in addresses {
            let address: EndpointAddr = decode(&raw)?;
            reconnect(&mut network, address, state.0.clone());
        }
        *runtime = Some(network);
    }
    drop(runtime);
    status(state).await
}
fn reconnect(
    network: &mut Network,
    address: EndpointAddr,
    core: Arc<Mutex<Result<crate::state::AppCore, AppError>>>,
) {
    let node = network.node.clone();
    let service = network.service.clone();
    let errors = network.error.clone();
    network.tasks.push(tokio::spawn(supervise(
        node, service, address, errors, core,
    )));
}
async fn supervise(
    node: Arc<MeshNode>,
    service: Arc<PairingService>,
    address: EndpointAddr,
    errors: Arc<Mutex<Option<String>>>,
    core: Arc<Mutex<Result<crate::state::AppCore, AppError>>>,
) {
    let mut backoff = 1;
    loop {
        let result: Result<(), NetError> = async {
            service.refresh().await?;
            let connection = node.connect(address.clone(), Protocol::Sync).await?;
            let sync = SyncService::new(service.clone());
            let mut ready = sync.ready();
            tokio::try_join!(sync.session(connection, true), async {
                ready.changed().await.map_err(|_| NetError::Closed)?;
                *errors.lock().await = None;
                while service
                    .store
                    .pending_join(service.workspace, service.signer.device_id())
                    .await
                    .map_err(|_| NetError::Closed)?
                    .is_some()
                {
                    if service
                        .store
                        .finish_join(
                            service.workspace,
                            service.signer.clone(),
                            crate::state::unix_ms().map_err(|_| NetError::Closed)?,
                        )
                        .await
                        .map_err(|_| NetError::Closed)?
                    {
                        let mut guard = core.lock().await;
                        if let Ok(core) = guard.as_mut() {
                            core.workspace = Some(service.workspace);
                            core.changed();
                        }
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                std::future::pending::<Result<(), NetError>>().await
            })?;
            Ok(())
        }
        .await;
        if let Err(error) = result {
            *errors.lock().await = Some(error.to_string());
        }
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(60);
    }
}
pub async fn invite(state: &AppState) -> Result<String, AppError> {
    start(state).await?;
    let runtime = state.3.lock().await;
    let network = runtime.as_ref().ok_or(AppError::NetworkUnavailable)?;
    network
        .service
        .invite(&network.node)
        .await
        .map_err(network_error)
}
pub async fn join(state: &AppState, link: String, display_name: String) -> Result<(), AppError> {
    let ticket = Ticket::parse(&link).map_err(|_| AppError::InvalidInvite)?;
    let workspace = ticket.workspace().map_err(|_| AppError::InvalidInvite)?;
    let address = ticket.inviter();
    {
        let guard = state.0.lock().await;
        let core = guard.as_ref().map_err(Clone::clone)?;
        if core.workspace.is_some() {
            return Err(AppError::WorkspaceExists);
        }
    }
    let mut runtime = state.3.lock().await;
    if runtime.is_some() {
        return Err(AppError::NetworkUnavailable);
    }
    let mut network = bind(state, workspace).await?;
    let node = network.node.clone();
    let service = network.service.clone();
    let error = network.error.clone();
    let core = state.0.clone();
    let runtime_state = state.3.clone();
    network.tasks.push(tokio::spawn(async move {
        if let Err(e) = service.join(&node, ticket, display_name).await {
            let message = e.to_string();
            *error.lock().await = Some(message.clone());
            if let Ok(core) = core.lock().await.as_mut() {
                core.network_error = Some(message);
            }
            let mut runtime = runtime_state.lock().await;
            if let Some(failed) = runtime.take() {
                drop(node);
                if let Ok(node) = Arc::try_unwrap(failed.node) {
                    node.close().await;
                }
            }
            return;
        }
        {
            let mut guard = core.lock().await;
            if let Ok(core) = guard.as_mut() {
                core.workspace = Some(workspace);
                core.changed();
            }
        }
        supervise(node, service, address, error, core).await;
    }));
    *runtime = Some(network);
    Ok(())
}
pub async fn confirm(state: &AppState, id: String, matches: bool) -> Result<(), AppError> {
    if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(AppError::InvalidInput);
    }
    let mut bytes = [0; 16];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte =
            u8::from_str_radix(&id[i * 2..i * 2 + 2], 16).map_err(|_| AppError::InvalidInput)?;
    }
    let runtime = state.3.lock().await;
    let network = runtime.as_ref().ok_or(AppError::NetworkUnavailable)?;
    network
        .service
        .confirm(bytes, matches)
        .await
        .map_err(network_error)
}
pub async fn status(state: &AppState) -> Result<NetworkView, AppError> {
    state.start_ingest().await;
    let runtime = state.3.lock().await;
    let Some(network) = runtime.as_ref() else {
        return Ok(NetworkView {
            running: false,
            mode: "Strict P2P".into(),
            peers: vec![],
            pairings: vec![],
            error: state
                .0
                .lock()
                .await
                .as_ref()
                .ok()
                .and_then(|core| core.network_error.clone()),
        });
    };
    let diagnostics = network.node.diagnostics();
    Ok(NetworkView {
        running: true,
        mode: "Strict P2P · no relay or public discovery".into(),
        peers: diagnostics
            .peers
            .into_iter()
            .map(|p| PeerView {
                device_short: hex(&p.endpoint_id.as_bytes()[..4]),
                path: match p.path {
                    meshyard_net::SelectedPath::Direct => "Direct P2P",
                    _ => "Offline",
                }
                .into(),
            })
            .collect(),
        pairings: network
            .service
            .pending()
            .into_iter()
            .map(|p| PairView {
                id: hex(&p.invite_id),
                peer_short: hex(&p.peer.0[..4]),
                words: p.words.into(),
                is_inviter: p.is_inviter,
                local_confirmed: p.local_confirmed,
                state: p.state,
            })
            .collect(),
        error: network.error.lock().await.clone(),
    })
}
pub async fn stop(state: &AppState) {
    let runtime = state.3.lock().await.take();
    if let Some(mut network) = runtime {
        for task in network.tasks.drain(..) {
            task.abort();
            let _ = task.await;
        }
        if let Ok(node) = Arc::try_unwrap(network.node) {
            node.close().await;
        }
    }
}
