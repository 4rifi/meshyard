//! Public IPC views contain no private filesystem, key or network details.
use meshyard_core::{ActivityState, AgentKind, Mode, Provenance, Role};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum AppError {
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
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum AppStatus {
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
