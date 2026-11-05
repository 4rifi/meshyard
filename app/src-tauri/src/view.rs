//! Public IPC views contain no private filesystem, key or network details.
use meshyard_core::{ActivityState, AgentKind, Mode, Provenance, Role};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum AppError {
    NetworkUnavailable,
    InvalidInvite,
    IdentityMissing,
    IdentityLocked,
    IdentityUnavailable,
    IdentityInvalid,
    IdentityMismatch,
    DataDirectoryBusy,
    UnsafePermissions,
    Storage,
    Migration,
    ProjectionLimit,
    MultipleWorkspaces,
    WorkspaceExists,
    NoWorkspace,
    InvalidInput,
    RandomUnavailable,
    PartialCreate,
    Configuration,
    IpcUnavailable,
    IngestUnavailable,
    HookBinaryMissing,
    HookInstallLocation,
    HookPlanStale,
    HookConfigInvalid,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum AppStatus {
    Joining,
    Setup,
    Ready(WorkspaceView),
    Error(AppError),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WorkspaceView {
    pub id_short: String,
    pub name: String,
    pub mode: Mode,
    pub device_short: String,
    pub members: Vec<MemberView>,
    pub agents: Vec<AgentView>,
    pub ingest: IngestView,
    pub hook: HookView,
    pub codex_hook: HookView,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct MemberView {
    pub device_short: String,
    pub display_name: String,
    pub role: Role,
    pub is_you: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AgentView {
    /// Stable public identity for list keys; never inferred from labels or list position.
    pub id: String,
    pub kind: AgentKind,
    pub label: String,
    pub state: Option<ActivityState>,
    pub provenance: Option<Provenance>,
    pub is_you: bool,
    pub stale: bool,
    pub bound: bool,
    pub connection: AgentConnection,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum AgentConnection {
    Synced,
    NotConnected,
    WaitingForFirstEvent,
    Connected,
    Stale,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct IngestView {
    pub status: IngestStatus,
    pub dropped_unbound: u32,
    pub dropped_invalid: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum IngestStatus {
    Disabled,
    Listening,
    Unavailable(AppError),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct HookView {
    pub installed: bool,
    pub binary_available: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct HookPlan {
    pub plan_id: String,
    pub settings_path_display: String,
    pub base_sha256: String,
    pub diff: String,
    pub action: HookAction,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum HookAction {
    Install,
    Remove,
    NoChange,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CreateWorkspace {
    pub name: String,
    pub display_name: String,
    pub mode: Mode,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RegisterAgent {
    pub kind: AgentKind,
    pub label: String,
}
