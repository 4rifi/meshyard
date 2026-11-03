//! Local envelope v1. This is deliberately independent of signed wire v1.
use meshyard_core::{AgentKind, HookEvent, ToolCategory};
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandClass {
    Test,
    GitMerge,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Envelope {
    pub(crate) agent: AgentKind,
    pub(crate) event: HookEvent,
    pub(crate) session_id: String,
    pub(crate) turn_id: Option<String>,
    pub(crate) tool_use_id: Option<String>,
    pub(crate) agent_id: Option<String>,
    pub(crate) tool: Option<ToolCategory>,
    pub(crate) command_class: Option<CommandClass>,
}
impl Envelope {
    pub fn agent(&self) -> AgentKind {
        self.agent
    }
    pub fn event(&self) -> HookEvent {
        self.event
    }
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("invalid local hook envelope")]
pub struct InvalidEnvelope;

fn valid_id(value: &str) -> bool {
    (1..=128).contains(&value.len())
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._:-".contains(&c))
}

/// Unknown fields are discarded, including any untrusted extra payload. No raw input is retained.
pub fn parse(line: &[u8]) -> Result<Envelope, InvalidEnvelope> {
    if line.len() > 1024 || !line.ends_with(b"\n") {
        return Err(InvalidEnvelope);
    }
    let raw: Value = serde_json::from_slice(line).map_err(|_| InvalidEnvelope)?;
    if raw.get("v").and_then(Value::as_u64) != Some(1) {
        return Err(InvalidEnvelope);
    }
    let agent = match raw.get("agent").and_then(Value::as_str) {
        Some("claude_code") => AgentKind::ClaudeCode,
        Some("codex") => AgentKind::Codex,
        _ => return Err(InvalidEnvelope),
    };
    let event: HookEvent =
        serde_json::from_value(raw.get("event").cloned().ok_or(InvalidEnvelope)?)
            .map_err(|_| InvalidEnvelope)?;
    let id = |key: &str| {
        raw.get(key)
            .and_then(Value::as_str)
            .filter(|s| valid_id(s))
            .map(str::to_owned)
    };
    let tool = match raw.get("tool").and_then(Value::as_str) {
        None => None,
        Some("edit") => Some(ToolCategory::Edit),
        Some("shell") => Some(ToolCategory::Shell),
        Some("read") => Some(ToolCategory::Read),
        Some("web") => Some(ToolCategory::Web),
        Some("other") => Some(ToolCategory::Other),
        _ => return Err(InvalidEnvelope),
    };
    let command_class = if tool == Some(ToolCategory::Shell) {
        raw.get("command_class")
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .map_err(|_| InvalidEnvelope)?
    } else {
        None
    };
    Ok(Envelope {
        agent,
        event,
        session_id: id("session_id").ok_or(InvalidEnvelope)?,
        turn_id: id("turn_id"),
        tool_use_id: id("tool_use_id"),
        agent_id: id("agent_id"),
        tool,
        command_class,
    })
}
