use crate::*;
use std::collections::BTreeMap;

pub const PENDING_EVENTS: usize = 1024;
pub const PENDING_BYTES: usize = 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DerivedState {
    pub epoch: u32,
    pub members: BTreeMap<DeviceId, Member>,
    pub agents: BTreeMap<(DeviceId, [u8; 16]), Agent>,
    pub pending_membership: BTreeMap<DeviceId, Vec<Event>>,
    pub pending_overflow: Vec<DeviceId>,
}
/// Input is the store's accepted contiguous event set, with unique (author, seq).
/// Ordering across authors and timestamps never influence the projection.
pub fn fold(genesis: &Genesis, events: &[Event]) -> Result<DerivedState, ValidationError> {
    let mut state = DerivedState {
        epoch: 0,
        members: BTreeMap::from([(
            genesis.owner,
            Member {
                device: genesis.owner,
                display_name: String::new(),
                role: Role::Owner,
                avatar: 0,
            },
        )]),
        agents: BTreeMap::new(),
        pending_membership: BTreeMap::new(),
        pending_overflow: vec![],
    };
    let mut authors: BTreeMap<DeviceId, Vec<&Event>> = BTreeMap::new();
    for event in events {
        authors.entry(event.author).or_default().push(event);
    }
    for log in authors.values_mut() {
        log.sort_by_key(|event| event.seq);
    }
    if let Some(log) = authors.get(&genesis.owner) {
        for event in log {
            match &event.body {
                EventBody::MemberAdded {
                    device,
                    display_name,
                } if *device != genesis.owner => {
                    state.epoch = state
                        .epoch
                        .checked_add(1)
                        .ok_or(ValidationError::Overflow)?;
                    state.members.insert(
                        *device,
                        Member {
                            device: *device,
                            display_name: display_name.clone(),
                            role: Role::Member,
                            avatar: 0,
                        },
                    );
                }
                EventBody::MemberRemoved { device } => {
                    state.epoch = state
                        .epoch
                        .checked_add(1)
                        .ok_or(ValidationError::Overflow)?;
                    state.members.remove(device);
                }
                _ => {}
            }
        }
    }
    for (author, log) in authors {
        if !state.members.contains_key(&author) {
            let mut bytes = 0usize;
            let mut pending = vec![];
            for event in log {
                if matches!(
                    event.body,
                    EventBody::MemberAdded { .. } | EventBody::MemberRemoved { .. }
                ) {
                    continue;
                }
                // Conservative wire-size accounting including header (≤74 B), framing (≤13 B) and signature (64 B).
                let size = postcard::to_stdvec(&wire::v1::EventBodyV1::from(event.body.clone()))
                    .map_err(|_| ValidationError::InvalidField)?
                    .len()
                    + 152;
                bytes = bytes.checked_add(size).ok_or(ValidationError::Overflow)?;
                if pending.len() == PENDING_EVENTS || bytes > PENDING_BYTES {
                    pending.clear();
                    state.pending_overflow.push(author);
                    break;
                }
                pending.push(event.clone());
            }
            if !pending.is_empty() {
                state.pending_membership.insert(author, pending);
            }
            continue;
        }
        for event in log {
            match &event.body {
                EventBody::ProfileUpdated {
                    display_name,
                    avatar,
                } => {
                    if let Some(member) = state.members.get_mut(&author) {
                        member.display_name.clone_from(display_name);
                        member.avatar = *avatar;
                    }
                }
                EventBody::AgentRegistered {
                    agent_local_id,
                    kind,
                    label,
                } => {
                    state.agents.insert(
                        (author, *agent_local_id),
                        Agent {
                            author,
                            local_id: *agent_local_id,
                            kind: *kind,
                            label: label.clone(),
                            state: ActivityState::Idle,
                            provenance: None,
                            tool: None,
                            task_label: None,
                        },
                    );
                }
                EventBody::AgentState {
                    agent_local_id,
                    state: activity,
                    provenance,
                    tool,
                    task_label,
                } => {
                    if let Some(agent) = state.agents.get_mut(&(author, *agent_local_id)) {
                        agent.state = *activity;
                        agent.provenance = Some(*provenance);
                        agent.tool = *tool;
                        agent.task_label.clone_from(task_label);
                    }
                }
                EventBody::AgentGone { agent_local_id } => {
                    state.agents.remove(&(author, *agent_local_id));
                }
                _ => {}
            }
        }
    }
    Ok(state)
}
