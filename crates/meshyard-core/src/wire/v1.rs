//! Field and variant order are frozen on T-002 review. Changes require wire v2.
use crate::domain as d;
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

macro_rules! wire_enum {
    ($name:ident, $domain:ident { $($variant:ident),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
        pub enum $name { $($variant),+ }
        impl From<d::$domain> for $name {
            fn from(value: d::$domain) -> Self { match value { $(d::$domain::$variant => Self::$variant),+ } }
        }
        impl From<$name> for d::$domain {
            fn from(value: $name) -> Self { match value { $($name::$variant => Self::$variant),+ } }
        }
    };
}
wire_enum!(ModeV1, Mode { Auto, Strict });
wire_enum!(
    ActivityStateV1,
    ActivityState {
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
    }
);
wire_enum!(
    AgentKindV1,
    AgentKind {
        ClaudeCode,
        Codex,
        Other
    }
);
wire_enum!(
    ToolCategoryV1,
    ToolCategory {
        Edit,
        Shell,
        Read,
        Web,
        Other
    }
);
wire_enum!(
    HookEventV1,
    HookEvent {
        SessionStart,
        UserPromptSubmit,
        PreToolUse,
        PostToolUse,
        Stop,
        StopFailure,
        SessionEnd
    }
);
wire_enum!(
    RuleIdV1,
    RuleId {
        ToolEdit,
        ToolShell,
        ToolRead,
        ToolWeb,
        TurnEnded,
        Stale
    }
);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemberRoleV1 {
    Member,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProvenanceV1 {
    Verified { source: HookEventV1 },
    Inferred { rule: RuleIdV1 },
}
impl From<d::Provenance> for ProvenanceV1 {
    fn from(value: d::Provenance) -> Self {
        match value {
            d::Provenance::Verified { source } => Self::Verified {
                source: source.into(),
            },
            d::Provenance::Inferred { rule } => Self::Inferred { rule: rule.into() },
        }
    }
}
impl From<ProvenanceV1> for d::Provenance {
    fn from(value: ProvenanceV1) -> Self {
        match value {
            ProvenanceV1::Verified { source } => Self::Verified {
                source: source.into(),
            },
            ProvenanceV1::Inferred { rule } => Self::Inferred { rule: rule.into() },
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenesisV1 {
    pub v: u8,
    pub owner: [u8; 32],
    pub name: String,
    pub mode: ModeV1,
    pub created_ms: u64,
    pub nonce: [u8; 16],
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedGenesisV1 {
    pub genesis_bytes: Vec<u8>,
    #[serde(with = "BigArray")]
    pub sig: [u8; 64],
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventHeaderV1 {
    pub v: u8,
    pub workspace: [u8; 16],
    pub author: [u8; 32],
    pub seq: u64,
    pub ts_ms: u64,
    pub membership_epoch: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventBodyV1 {
    MemberAdded {
        device: [u8; 32],
        display_name: String,
        role: MemberRoleV1,
    },
    MemberRemoved {
        device: [u8; 32],
    },
    ProfileUpdated {
        display_name: String,
        avatar: u16,
    },
    AgentRegistered {
        agent_local_id: [u8; 16],
        kind: AgentKindV1,
        label: String,
    },
    AgentState {
        agent_local_id: [u8; 16],
        state: ActivityStateV1,
        provenance: ProvenanceV1,
        tool: Option<ToolCategoryV1>,
        task_label: Option<String>,
    },
    AgentGone {
        agent_local_id: [u8; 16],
    },
}
impl From<d::EventBody> for EventBodyV1 {
    fn from(value: d::EventBody) -> Self {
        match value {
            d::EventBody::MemberAdded {
                device,
                display_name,
            } => Self::MemberAdded {
                device: device.0,
                display_name,
                role: MemberRoleV1::Member,
            },
            d::EventBody::MemberRemoved { device } => Self::MemberRemoved { device: device.0 },
            d::EventBody::ProfileUpdated {
                display_name,
                avatar,
            } => Self::ProfileUpdated {
                display_name,
                avatar,
            },
            d::EventBody::AgentRegistered {
                agent_local_id,
                kind,
                label,
            } => Self::AgentRegistered {
                agent_local_id,
                kind: kind.into(),
                label,
            },
            d::EventBody::AgentState {
                agent_local_id,
                state,
                provenance,
                tool,
                task_label,
            } => Self::AgentState {
                agent_local_id,
                state: state.into(),
                provenance: provenance.into(),
                tool: tool.map(Into::into),
                task_label,
            },
            d::EventBody::AgentGone { agent_local_id } => Self::AgentGone { agent_local_id },
        }
    }
}
impl From<EventBodyV1> for d::EventBody {
    fn from(value: EventBodyV1) -> Self {
        match value {
            EventBodyV1::MemberAdded {
                device,
                display_name,
                ..
            } => Self::MemberAdded {
                device: d::DeviceId(device),
                display_name,
            },
            EventBodyV1::MemberRemoved { device } => Self::MemberRemoved {
                device: d::DeviceId(device),
            },
            EventBodyV1::ProfileUpdated {
                display_name,
                avatar,
            } => Self::ProfileUpdated {
                display_name,
                avatar,
            },
            EventBodyV1::AgentRegistered {
                agent_local_id,
                kind,
                label,
            } => Self::AgentRegistered {
                agent_local_id,
                kind: kind.into(),
                label,
            },
            EventBodyV1::AgentState {
                agent_local_id,
                state,
                provenance,
                tool,
                task_label,
            } => Self::AgentState {
                agent_local_id,
                state: state.into(),
                provenance: provenance.into(),
                tool: tool.map(Into::into),
                task_label,
            },
            EventBodyV1::AgentGone { agent_local_id } => Self::AgentGone { agent_local_id },
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedEventV1 {
    pub signed: Vec<u8>,
    #[serde(with = "BigArray")]
    pub sig: [u8; 64],
}

pub const MAX_FRAME: usize = 256 * 1024;
/// Fixed 10-byte network header, NOT postcard. Frame-type assignment belongs to each ALPN.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameHeaderV1 {
    pub frame_type: u8,
    pub len: u32,
}
impl FrameHeaderV1 {
    pub fn encode(self) -> Result<[u8; 10], crate::CryptoError> {
        if self.len > MAX_FRAME as u32 {
            return Err(crate::CryptoError::Frame);
        }
        let mut bytes = *b"MSHY\x01\x00\x00\x00\x00\x00";
        bytes[5] = self.frame_type;
        bytes[6..].copy_from_slice(&self.len.to_be_bytes());
        Ok(bytes)
    }
    pub fn decode(bytes: [u8; 10], allowed_types: &[u8]) -> Result<Self, crate::CryptoError> {
        if &bytes[..4] != b"MSHY" || bytes[4] != 1 || !allowed_types.contains(&bytes[5]) {
            return Err(crate::CryptoError::Frame);
        }
        let len = u32::from_be_bytes([bytes[6], bytes[7], bytes[8], bytes[9]]);
        if len > MAX_FRAME as u32 {
            return Err(crate::CryptoError::Frame);
        }
        Ok(Self {
            frame_type: bytes[5],
            len,
        })
    }
}
impl From<d::Genesis> for GenesisV1 {
    fn from(g: d::Genesis) -> Self {
        Self {
            v: 1,
            owner: g.owner.0,
            name: g.name,
            mode: g.mode.into(),
            created_ms: g.created_ms,
            nonce: g.nonce,
        }
    }
}
impl From<GenesisV1> for d::Genesis {
    fn from(g: GenesisV1) -> Self {
        Self {
            owner: d::DeviceId(g.owner),
            name: g.name,
            mode: g.mode.into(),
            created_ms: g.created_ms,
            nonce: g.nonce,
        }
    }
}
