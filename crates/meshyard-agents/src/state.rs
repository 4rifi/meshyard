//! Pure, bounded per-agent correlation. The caller supplies monotonic local receive time.
use crate::ingest::{CommandClass, Envelope};
use meshyard_core::{ActivityState, AgentKind, HookEvent, Provenance, RuleId, ToolCategory};
use std::collections::VecDeque;
use std::time::Duration;

const SESSION_LIMIT: usize = 32;
const ENDED_LIMIT: usize = 16;
const DUPLICATE_LIMIT: usize = 64;
const FINISHED_IDLE: Duration = Duration::from_secs(60);

pub type StateChange = (ActivityState, Provenance, Option<ToolCategory>);
type Duplicate = (HookEvent, Option<String>, Option<String>);

struct Session {
    id: String,
    state: ActivityState,
    change: StateChange,
    seen: Duration,
    turn: Option<String>,
    ended: VecDeque<String>,
    duplicates: VecDeque<Duplicate>,
}
impl Session {
    fn new(id: &str, now: Duration) -> Self {
        Self {
            id: id.to_owned(),
            state: ActivityState::Idle,
            change: verified(ActivityState::Idle, HookEvent::SessionStart),
            seen: now,
            turn: None,
            ended: VecDeque::new(),
            duplicates: VecDeque::new(),
        }
    }
    fn end_turn(&mut self) {
        if let Some(turn) = self.turn.take() {
            remember(&mut self.ended, turn, ENDED_LIMIT);
        }
    }
}

fn remember<T>(queue: &mut VecDeque<T>, value: T, limit: usize) {
    if queue.len() == limit {
        queue.pop_front();
    }
    queue.push_back(value);
}
fn verified(state: ActivityState, source: HookEvent) -> StateChange {
    (state, Provenance::Verified { source }, None)
}
fn inferred(state: ActivityState, rule: RuleId, tool: Option<ToolCategory>) -> StateChange {
    (state, Provenance::Inferred { rule }, tool)
}
fn inactive(state: ActivityState) -> bool {
    matches!(
        state,
        ActivityState::Idle | ActivityState::Finished | ActivityState::Error
    )
}
fn lifecycle(event: HookEvent) -> bool {
    matches!(
        event,
        HookEvent::SessionStart | HookEvent::UserPromptSubmit | HookEvent::SessionEnd
    )
}

/// One machine per registered agent kind. Subagents contribute to that same agent.
pub struct StateMachine {
    kind: AgentKind,
    sessions: VecDeque<Session>,
    latest: Option<String>,
}
impl StateMachine {
    pub fn new(kind: AgentKind) -> Self {
        Self {
            kind,
            sessions: VecDeque::new(),
            latest: None,
        }
    }
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }
    pub fn current(&self) -> Option<StateChange> {
        let id = self.latest.as_ref()?;
        self.sessions.iter().find(|s| &s.id == id).map(|s| s.change)
    }
    pub fn session_state(&self, id: &str) -> Option<ActivityState> {
        self.sessions.iter().find(|s| s.id == id).map(|s| s.state)
    }

    /// Only UserPromptSubmit opens a turn. Ignored traffic cannot refresh state or LRU.
    pub fn apply(&mut self, input: &Envelope, now: Duration) -> Option<StateChange> {
        if input.agent != self.kind {
            return None;
        }
        let position = self.sessions.iter().position(|s| s.id == input.session_id);
        if position.is_none() && !lifecycle(input.event) {
            return None;
        }
        let mut session = match position {
            Some(index) => self.sessions.remove(index)?,
            None => Session::new(&input.session_id, now),
        };
        let result = Self::apply_session(&mut session, input, now);
        if result.is_some() {
            self.latest = Some(session.id.clone());
        }
        // Keep rejected/duplicate traffic in its original LRU position.
        if result.is_some() || position.is_none() {
            if self.sessions.len() == SESSION_LIMIT {
                self.sessions.pop_front();
            }
            self.sessions.push_back(session);
        } else if let Some(index) = position {
            self.sessions.insert(index, session);
        }
        result
    }

    fn apply_session(
        session: &mut Session,
        input: &Envelope,
        now: Duration,
    ) -> Option<StateChange> {
        if now < session.seen
            || input
                .turn_id
                .as_ref()
                .is_some_and(|id| session.ended.contains(id))
            || (inactive(session.state) && !lifecycle(input.event))
        {
            return None;
        }
        let duplicate = (
            input.event,
            input.tool_use_id.clone(),
            input.agent_id.clone(),
        );
        // Keep identified tool duplicates across turns, including late events without a
        // turn ID. Unidentified lifecycle/activity keys are scoped to the turn so
        // later real prompts are not suppressed forever when optional IDs are absent.
        let new_turn = input.event == HookEvent::UserPromptSubmit
            && (input.turn_id.is_none()
                || inactive(session.state)
                || input.turn_id != session.turn);
        if !new_turn && session.duplicates.contains(&duplicate) {
            return None;
        }
        if new_turn {
            session.end_turn();
            session
                .duplicates
                .retain(|(_, tool_id, _)| tool_id.is_some());
            session.turn = input.turn_id.clone();
        }
        remember(&mut session.duplicates, duplicate, DUPLICATE_LIMIT);
        let change = match input.event {
            HookEvent::SessionStart => {
                session.end_turn();
                verified(ActivityState::Idle, input.event)
            }
            HookEvent::UserPromptSubmit => verified(ActivityState::Thinking, input.event),
            HookEvent::PreToolUse => match input.tool {
                Some(ToolCategory::Edit) => {
                    inferred(ActivityState::Building, RuleId::ToolEdit, input.tool)
                }
                Some(ToolCategory::Shell) => inferred(
                    match input.command_class {
                        Some(CommandClass::Test) => ActivityState::Testing,
                        Some(CommandClass::GitMerge) => ActivityState::Merging,
                        _ => ActivityState::Building,
                    },
                    RuleId::ToolShell,
                    input.tool,
                ),
                Some(ToolCategory::Read) => {
                    inferred(ActivityState::Thinking, RuleId::ToolRead, input.tool)
                }
                Some(ToolCategory::Web) => {
                    inferred(ActivityState::Thinking, RuleId::ToolWeb, input.tool)
                }
                _ => return None,
            },
            HookEvent::PostToolUse => return None,
            HookEvent::Stop | HookEvent::StopFailure | HookEvent::SessionEnd => {
                // Remember the explicit terminal ID too if the prompt had no ID.
                if session.turn.is_none() {
                    session.turn = input.turn_id.clone();
                }
                session.end_turn();
                verified(
                    match input.event {
                        HookEvent::Stop => ActivityState::Finished,
                        HookEvent::StopFailure => ActivityState::Error,
                        _ => ActivityState::Idle,
                    },
                    input.event,
                )
            }
        };
        session.seen = now;
        session.state = change.0;
        session.change = change;
        Some(change)
    }

    /// Expires Finished sessions without reviving an older session over newer activity.
    /// Returns only the registered agent's current state if that state changed.
    pub fn tick(&mut self, now: Duration) -> Option<StateChange> {
        let mut changed = None;
        for session in &mut self.sessions {
            if session.state == ActivityState::Finished
                && now.saturating_sub(session.seen) >= FINISHED_IDLE
            {
                session.state = ActivityState::Idle;
                session.change = inferred(ActivityState::Idle, RuleId::TurnEnded, None);
                if self.latest.as_ref() == Some(&session.id) {
                    changed = Some(session.change);
                }
            }
        }
        changed
    }
}

/// Derived view metadata only. Never emits ActivityState or Provenance, including RuleId::Stale.
pub fn is_stale(last_state: ActivityState, last_seen: Duration, now: Duration) -> bool {
    !matches!(last_state, ActivityState::Idle | ActivityState::Finished)
        && now.saturating_sub(last_seen) > Duration::from_secs(600)
}
