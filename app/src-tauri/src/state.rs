//! One store/identity per process; one mutex covers each complete read/write/view operation.
use crate::hooks::Installer;
use crate::ingest::{IngestStatus as IngestMetadata, Server};
use crate::view::*;
use meshyard_core::*;
use meshyard_store::{Store, StoreError, identity::*};
use std::{path::PathBuf, sync::Arc};
use std::{sync::atomic::Ordering, time::Duration};
use tauri::{Emitter, Manager};
use tokio::sync::Mutex;

pub struct AppCore {
    pub(crate) store: Store,
    pub(crate) signer: Arc<dyn Signer>,
    pub(crate) workspace: Option<WorkspaceId>,
    pub(crate) app: Option<tauri::AppHandle>,
    pub(crate) ingest: Arc<IngestMetadata>,
    pub(crate) installer: Option<Installer>,
    pub(crate) codex_installer: Option<Installer>,
}
pub struct AppState(
    pub(crate) Arc<Mutex<Result<AppCore, AppError>>>,
    pub(crate) Mutex<Option<Server>>,
    pub(crate) Arc<IngestMetadata>,
);

impl AppState {
    pub fn failed(error: AppError) -> Self {
        Self(
            Arc::new(Mutex::new(Err(error))),
            Mutex::new(None),
            Arc::default(),
        )
    }
    pub async fn open(data_dir: PathBuf, file_key: bool) -> Self {
        if file_key {
            let keys = FileKeyStore::new(data_dir.join("identity"));
            Self::open_with_keys(data_dir, &keys).await
        } else {
            #[cfg(target_os = "macos")]
            {
                Self::open_with_keys(data_dir, &KeychainKeyStore::new()).await
            }
            #[cfg(not(target_os = "macos"))]
            {
                Self::failed(AppError::IdentityUnavailable)
            }
        }
    }
    /// Explicit injection supports non-GUI tests without ever opening a Keychain backend.
    pub async fn open_with_keys(data_dir: PathBuf, keys: &dyn DeviceKeyStore) -> Self {
        let store = match Store::open_async(data_dir).await {
            Ok(store) => store,
            Err(error) => return Self::failed(storage_error(error)),
        };
        let result = async {
            let workspaces = store.workspaces().await.map_err(storage_error)?;
            if workspaces.len() > 1 {
                return Err(AppError::MultipleWorkspaces);
            }
            let workspace = workspaces.first().copied();
            let signer = Arc::new(
                if workspace.is_some() {
                    keys.load()
                } else {
                    load_or_create(keys)
                }
                .map_err(identity_error)?,
            );
            if let Some(id) = workspace {
                let state = store.workspace_snapshot(id).await.map_err(storage_error)?;
                if !state.members.iter().any(|m| m.device == signer.device_id()) {
                    return Err(AppError::IdentityMismatch);
                }
            }
            Ok((signer, workspace))
        }
        .await;
        match result {
            Ok((signer, workspace)) => {
                let ingest = Arc::new(IngestMetadata::default());
                let state = Self(
                    Arc::new(Mutex::new(Ok(AppCore {
                        store,
                        signer,
                        workspace,
                        app: None,
                        ingest: ingest.clone(),
                        installer: None,
                        codex_installer: None,
                    }))),
                    Mutex::new(None),
                    ingest,
                );
                state.start_ingest().await;
                state
            }
            Err(error) => {
                let _ = store.close().await;
                Self::failed(error)
            }
        }
    }
    /// Also called after create; setup profiles without a workspace have no listener.
    pub(crate) async fn start_ingest(&self) {
        let mut runtime = self.1.lock().await;
        if runtime.is_some() {
            return;
        }
        let guard = self.0.lock().await;
        if let Ok(core) = guard.as_ref() {
            if core.workspace.is_none() {
                return;
            }
            *runtime = Server::start(
                core.store.directory(),
                Arc::downgrade(&self.0),
                self.2.clone(),
            );
        }
    }
    pub(crate) async fn attach_app(&self, app: tauri::AppHandle) {
        if let Ok(core) = self.0.lock().await.as_mut() {
            // Production paths are activated only by the native attachment, never inferred
            // from file-key mode (which is also used by developer profiles). No writes here.
            if core.installer.is_none()
                && let Ok(home) = std::env::var_os("MESHYARD_HOOK_HOME")
                    .map(PathBuf::from)
                    .map(Ok)
                    .unwrap_or_else(|| app.path().home_dir())
            {
                core.installer = Some(Installer::new(
                    home.join(".claude/settings.json"),
                    crate::hooks::bundled_binary(),
                    core.store.directory().join("ingest/ingest.sock"),
                ));
            }
            if core.codex_installer.is_none()
                && let Ok(home) = std::env::var_os("MESHYARD_HOOK_HOME")
                    .map(PathBuf::from)
                    .map(Ok)
                    .unwrap_or_else(|| app.path().home_dir())
            {
                core.codex_installer = Some(
                    Installer::new(
                        home.join(".codex/hooks.json"),
                        crate::hooks::bundled_binary(),
                        core.store.directory().join("ingest/ingest.sock"),
                    )
                    .for_kind(AgentKind::Codex),
                );
            }
            core.app = Some(app);
        }
    }
    /// Explicit isolated settings injection; constructors never read owner configuration.
    pub async fn configure_hooks(&self, settings: PathBuf, binary: Option<PathBuf>) {
        if let Ok(core) = self.0.lock().await.as_mut() {
            core.installer = Some(Installer::new(
                settings,
                binary,
                core.store.directory().join("ingest/ingest.sock"),
            ));
        }
    }
    pub async fn configure_codex_hooks(&self, settings: PathBuf, binary: Option<PathBuf>) {
        if let Ok(core) = self.0.lock().await.as_mut() {
            core.codex_installer = Some(
                Installer::new(
                    settings,
                    binary,
                    core.store.directory().join("ingest/ingest.sock"),
                )
                .for_kind(AgentKind::Codex),
            );
        }
    }
    /// Stop/join the listener and all bounded readers before releasing the profile lock.
    pub async fn shutdown(&self) {
        if let Some(server) = self.1.lock().await.take() {
            server.stop().await;
        }
        self.2
            .listening
            .store(false, std::sync::atomic::Ordering::Relaxed);
        if let Ok(core) = self.0.lock().await.as_mut() {
            core.app = None;
        }
    }
    pub async fn close(self) -> Result<(), AppError> {
        self.shutdown().await;
        let mut guard = self.0.lock().await;
        if let Ok(core) = guard.as_ref() {
            core.store.clone().close().await.map_err(storage_error)?;
        }
        *guard = Err(AppError::Storage);
        Ok(())
    }
}
impl AppCore {
    pub(crate) async fn status(&self) -> Result<AppStatus, AppError> {
        self.status_at(unix_ms()?).await
    }
    pub(crate) async fn status_at(&self, now: u64) -> Result<AppStatus, AppError> {
        let Some(id) = self.workspace else {
            return Ok(AppStatus::Setup);
        };
        let state = self
            .store
            .workspace_snapshot(id)
            .await
            .map_err(storage_error)?;
        let genesis = &state.genesis;
        let device = self.signer.device_id();
        // Genesis may have persisted while the first profile append failed. Never hide this
        // behind a success view, including after reopening; update_profile can repair it.
        if genesis.owner == device
            && state
                .members
                .iter()
                .find(|m| m.device == device)
                .is_some_and(|m| m.display_name.is_empty())
        {
            return Err(AppError::PartialCreate);
        }
        let binding = self
            .store
            .agent_binding(id, AgentKind::ClaudeCode, self.signer.clone())
            .await
            .map_err(storage_error)?;
        let codex_binding = self
            .store
            .agent_binding(id, AgentKind::Codex, self.signer.clone())
            .await
            .map_err(storage_error)?;
        let codex_hook = self
            .codex_installer
            .as_ref()
            .map(|installer| {
                installer.view().unwrap_or_else(|_| HookView {
                    installed: false,
                    binary_available: installer.binary_available(),
                })
            })
            .unwrap_or(HookView {
                installed: false,
                binary_available: false,
            });
        let hook = self
            .installer
            .as_ref()
            .map(|installer| {
                installer.view().unwrap_or_else(|_| HookView {
                    installed: false,
                    binary_available: installer.binary_available(),
                })
            })
            .unwrap_or(HookView {
                installed: false,
                binary_available: false,
            });
        Ok(AppStatus::Ready(WorkspaceView {
            id_short: short(&id.0),
            name: genesis.name.clone(),
            mode: genesis.mode,
            device_short: short(&device.0),
            members: state
                .members
                .into_iter()
                .map(|m| MemberView {
                    device_short: short(&m.device.0),
                    display_name: m.display_name,
                    role: m.role,
                    is_you: m.device == device,
                })
                .collect(),
            agents: state
                .agents
                .into_iter()
                .map(|a| {
                    let bound = a.author == device
                        && match a.kind {
                            AgentKind::ClaudeCode => binding == Some(a.local_id),
                            AgentKind::Codex => codex_binding == Some(a.local_id),
                            _ => false,
                        };
                    let stale = a.provenance.is_some()
                        && state
                            .agent_state_ts
                            .get(&(a.author, a.local_id))
                            .is_some_and(|ts| {
                                meshyard_agents::state::is_stale(
                                    a.state,
                                    Duration::from_millis(*ts),
                                    Duration::from_millis(now),
                                )
                            });
                    let connection = if !bound
                        || !(if a.kind == AgentKind::Codex {
                            codex_hook.installed
                        } else {
                            hook.installed
                        }) {
                        AgentConnection::NotConnected
                    } else if a.provenance.is_none() {
                        AgentConnection::WaitingForFirstEvent
                    } else if stale {
                        AgentConnection::Stale
                    } else {
                        AgentConnection::Connected
                    };
                    AgentView {
                        id: format!("{}:{}", hex(&a.author.0), hex(&a.local_id)),
                        kind: a.kind,
                        label: a.label,
                        state: a.provenance.map(|_| a.state),
                        provenance: a.provenance,
                        is_you: a.author == device,
                        stale,
                        bound,
                        connection,
                    }
                })
                .collect(),
            ingest: IngestView {
                status: if self.ingest.unavailable.load(Ordering::Relaxed) {
                    IngestStatus::Unavailable(AppError::IngestUnavailable)
                } else if self.ingest.listening.load(Ordering::Relaxed) {
                    IngestStatus::Listening
                } else {
                    IngestStatus::Disabled
                },
                dropped_unbound: self.ingest.dropped_unbound.load(Ordering::Relaxed),
                dropped_invalid: self.ingest.dropped_invalid.load(Ordering::Relaxed),
            },
            hook,
            codex_hook,
        }))
    }
    pub(crate) async fn create(&mut self, input: CreateWorkspace) -> Result<AppStatus, AppError> {
        if self.workspace.is_some() {
            return Err(AppError::WorkspaceExists);
        }
        if input.mode != Mode::Strict {
            return Err(AppError::InvalidInput);
        }
        let name = text(&input.name)?;
        let display_name = text(&input.display_name)?;
        let genesis = Genesis {
            owner: self.signer.device_id(),
            name,
            mode: Mode::Strict,
            nonce: random_id()?,
            created_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .and_then(|d| u64::try_from(d.as_millis()).ok())
                .ok_or(AppError::Storage)?,
        };
        let signed = sign_genesis(genesis, self.signer.as_ref()).map_err(|_| AppError::Storage)?;
        let raw = postcard::to_stdvec(&signed).map_err(|_| AppError::Storage)?;
        let id = self.store.add_workspace(raw).await.map_err(storage_error)?;
        self.workspace = Some(id); // durable before the next operation; never allow a second create
        self.store
            .append_own(
                id,
                EventBody::ProfileUpdated {
                    display_name,
                    avatar: 0,
                },
                self.signer.clone(),
            )
            .await
            .map_err(|_| AppError::PartialCreate)?;
        self.changed();
        self.status().await
    }
    pub(crate) async fn profile(&mut self, name: String) -> Result<AppStatus, AppError> {
        let id = self.workspace.ok_or(AppError::NoWorkspace)?;
        let display_name = text(&name)?;
        self.store
            .append_own(
                id,
                EventBody::ProfileUpdated {
                    display_name,
                    avatar: 0,
                },
                self.signer.clone(),
            )
            .await
            .map_err(storage_error)?;
        self.changed();
        self.status().await
    }
    pub(crate) async fn register(&mut self, input: RegisterAgent) -> Result<AppStatus, AppError> {
        let id = self.workspace.ok_or(AppError::NoWorkspace)?;
        let label = text(&input.label)?;
        if !matches!(input.kind, AgentKind::ClaudeCode | AgentKind::Codex) {
            return Err(AppError::InvalidInput);
        }
        let agent_local_id = random_id()?;
        self.store
            .append_own(
                id,
                EventBody::AgentRegistered {
                    agent_local_id,
                    kind: input.kind,
                    label,
                },
                self.signer.clone(),
            )
            .await
            .map_err(storage_error)?;
        let binding = self
            .store
            .bind_first_own_agent(id, agent_local_id, self.signer.clone())
            .await;
        self.changed();
        binding.map_err(storage_error)?;
        self.status().await
    }
    pub(crate) async fn bind(&mut self, agent_id: String) -> Result<AppStatus, AppError> {
        let workspace = self.workspace.ok_or(AppError::NoWorkspace)?;
        let (author, local) = agent_id.split_once(':').ok_or(AppError::InvalidInput)?;
        if author != hex(&self.signer.device_id().0) || local.len() != 32 {
            return Err(AppError::InvalidInput);
        }
        let mut id = [0; 16];
        for (i, byte) in id.iter_mut().enumerate() {
            *byte = local
                .get(i * 2..i * 2 + 2)
                .and_then(|part| u8::from_str_radix(part, 16).ok())
                .ok_or(AppError::InvalidInput)?;
        }
        let agent = self
            .store
            .own_agent(workspace, self.signer.device_id(), id)
            .await
            .map_err(storage_error)?
            .ok_or(AppError::InvalidInput)?;
        if !matches!(agent.kind, AgentKind::ClaudeCode | AgentKind::Codex) {
            return Err(AppError::InvalidInput);
        }
        self.store
            .bind_own_agent(workspace, id, self.signer.clone())
            .await
            .map_err(storage_error)?;
        self.changed();
        self.status().await
    }
    pub(crate) fn hook_plan(&mut self, remove: bool) -> Result<HookPlan, AppError> {
        self.hook_plan_for(remove, AgentKind::ClaudeCode)
    }
    pub(crate) fn hook_plan_for(
        &mut self,
        remove: bool,
        kind: AgentKind,
    ) -> Result<HookPlan, AppError> {
        self.workspace.ok_or(AppError::NoWorkspace)?;
        let installer = match kind {
            AgentKind::ClaudeCode => &mut self.installer,
            AgentKind::Codex => &mut self.codex_installer,
            _ => return Err(AppError::InvalidInput),
        };
        installer
            .as_mut()
            .ok_or(AppError::HookBinaryMissing)?
            .plan(remove)
    }
    pub(crate) async fn hook_apply(&mut self, plan_id: String) -> Result<AppStatus, AppError> {
        let id = self.workspace.ok_or(AppError::NoWorkspace)?;
        let kind = if self
            .codex_installer
            .as_ref()
            .is_some_and(|i| i.has_plan(&plan_id))
        {
            AgentKind::Codex
        } else {
            AgentKind::ClaudeCode
        };
        let installer = if kind == AgentKind::Codex {
            &mut self.codex_installer
        } else {
            &mut self.installer
        };
        let installer = installer.as_mut().ok_or(AppError::HookBinaryMissing)?;
        if installer.requires_binding()
            && self
                .store
                .agent_binding(id, kind, self.signer.clone())
                .await
                .map_err(storage_error)?
                .is_none()
        {
            return Err(AppError::InvalidInput);
        }
        installer.apply(&plan_id)?;
        self.changed();
        self.status().await
    }
    pub(crate) fn changed(&self) {
        if let Some(app) = &self.app {
            let _ = app.emit("workspace-changed", ());
        }
    }
}
fn text(value: &str) -> Result<String, AppError> {
    // Protocol sanitation is performed before signing, UTF-8 byte capped by the existing utility.
    let value = sanitize_for_signing(value.trim(), 64);
    if value.trim().is_empty() {
        Err(AppError::InvalidInput)
    } else {
        Ok(value)
    }
}
fn random_id() -> Result<[u8; 16], AppError> {
    let mut id = [0; 16];
    getrandom::fill(&mut id).map_err(|_| AppError::RandomUnavailable)?;
    Ok(id)
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn short(bytes: &[u8]) -> String {
    hex(&bytes[..4])
}
fn storage_error(error: StoreError) -> AppError {
    match error {
        StoreError::Busy => AppError::DataDirectoryBusy,
        StoreError::Permissions => AppError::UnsafePermissions,
        StoreError::Migration => AppError::Migration,
        StoreError::ProjectionLimit => AppError::ProjectionLimit,
        _ => AppError::Storage,
    }
}
fn identity_error(error: IdentityError) -> AppError {
    match error {
        IdentityError::NotFound => AppError::IdentityMissing,
        IdentityError::Locked => AppError::IdentityLocked,
        IdentityError::Unavailable => AppError::IdentityUnavailable,
        IdentityError::Permissions => AppError::UnsafePermissions,
        IdentityError::Random => AppError::RandomUnavailable,
        IdentityError::Io => AppError::Storage,
        IdentityError::AlreadyExists | IdentityError::Invalid => AppError::IdentityInvalid,
    }
}

pub(crate) fn unix_ms() -> Result<u64, AppError> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|d| u64::try_from(d.as_millis()).ok())
        .ok_or(AppError::Storage)
}
