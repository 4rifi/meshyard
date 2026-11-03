use serde::{Deserialize, Serialize};
use ts_rs::TS;

macro_rules! domain_enum {
    ($name:ident { $($variant:ident),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
        pub enum $name { $($variant),+ }
    };
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
pub struct WorkspaceId(pub [u8; 16]);
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
pub struct DeviceId(pub [u8; 32]);
domain_enum!(Mode { Auto, Strict });
domain_enum!(Role { Owner, Member });
domain_enum!(ActivityState {
    Idle,
    Thinking,
    Building,
    Reviewing,
    Testing,
    Fixing,
    WaitingForInput,
    Merging,
    Finished,
    Error
});
domain_enum!(AgentKind {
    ClaudeCode,
    Codex,
    Other
});
domain_enum!(ToolCategory {
    Edit,
    Shell,
    Read,
    Web,
    Other
});
domain_enum!(HookEvent {
    SessionStart,
    UserPromptSubmit,
    PreToolUse,
    PostToolUse,
    Stop,
    StopFailure,
    SessionEnd
});
domain_enum!(RuleId {
    ToolEdit,
    ToolShell,
    ToolRead,
    ToolWeb,
    TurnEnded,
    Stale
});
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum Provenance {
    Verified { source: HookEvent },
    Inferred { rule: RuleId },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Genesis {
    pub owner: DeviceId,
    pub name: String,
    pub mode: Mode,
    pub created_ms: u64,
    pub nonce: [u8; 16],
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Member {
    pub device: DeviceId,
    pub display_name: String,
    pub role: Role,
    pub avatar: u16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub genesis: Genesis,
    pub members: Vec<Member>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Agent {
    pub author: DeviceId,
    pub local_id: [u8; 16],
    pub kind: AgentKind,
    pub label: String,
    pub state: ActivityState,
    pub provenance: Option<Provenance>,
    pub tool: Option<ToolCategory>,
    pub task_label: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventBody {
    MemberAdded {
        device: DeviceId,
        display_name: String,
    },
    MemberRemoved {
        device: DeviceId,
    },
    ProfileUpdated {
        display_name: String,
        avatar: u16,
    },
    AgentRegistered {
        agent_local_id: [u8; 16],
        kind: AgentKind,
        label: String,
    },
    AgentState {
        agent_local_id: [u8; 16],
        state: ActivityState,
        provenance: Provenance,
        tool: Option<ToolCategory>,
        task_label: Option<String>,
    },
    AgentGone {
        agent_local_id: [u8; 16],
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub workspace: WorkspaceId,
    pub author: DeviceId,
    pub seq: u64,
    pub ts_ms: u64,
    pub membership_epoch: u32,
    pub body: EventBody,
}
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ValidationError {
    #[error("invalid protocol field")]
    InvalidField,
    #[error("counter overflow")]
    Overflow,
}
/// Unicode Cc is precisely U+0000..001F and U+007F..009F (`char::is_control`).
pub fn sanitize_for_signing(value: &str, cap: usize) -> String {
    let mut result = String::new();
    for ch in value.chars().filter(|ch| !ch.is_control()) {
        if result.len() + ch.len_utf8() > cap {
            break;
        }
        result.push(ch);
    }
    result
}
pub fn validate_received(value: &str, cap: usize) -> Result<(), ValidationError> {
    if value.len() > cap || value.chars().any(char::is_control) {
        Err(ValidationError::InvalidField)
    } else {
        Ok(())
    }
}
impl EventBody {
    pub fn sanitize_for_signing(&mut self) {
        match self {
            Self::MemberAdded { display_name, .. } | Self::ProfileUpdated { display_name, .. } => {
                *display_name = sanitize_for_signing(display_name, 64)
            }
            Self::AgentRegistered { label, .. } => *label = sanitize_for_signing(label, 64),
            Self::AgentState {
                task_label: Some(label),
                ..
            } => *label = sanitize_for_signing(label, 80),
            _ => {}
        }
    }
    pub fn validate_received(&self) -> Result<(), ValidationError> {
        match self {
            Self::MemberAdded { display_name, .. } | Self::ProfileUpdated { display_name, .. } => {
                validate_received(display_name, 64)
            }
            Self::AgentRegistered { label, .. } => validate_received(label, 64),
            Self::AgentState {
                task_label: Some(label),
                ..
            } => validate_received(label, 80),
            _ => Ok(()),
        }
    }
}
impl Event {
    pub fn validate_received(&self) -> Result<(), ValidationError> {
        if self.seq == 0 {
            return Err(ValidationError::InvalidField);
        }
        self.body.validate_received()
    }
}
