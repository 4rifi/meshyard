use crate::EventBody;
/// Labels originate in explicit user/adapter label fields, never in raw command/prompt/path
/// fields. Arbitrary text cannot be proven secret-free by content inspection.
#[derive(Clone, Copy, Debug, Default)]
pub struct SharingPolicy {
    /// Set only for a coarse label deliberately typed in Meshyard, or explicit per-field opt-in.
    pub allow_task_label: bool,
}
impl SharingPolicy {
    pub fn filter(&self, mut body: EventBody) -> EventBody {
        if !self.allow_task_label
            && let EventBody::AgentState { task_label, .. } = &mut body
        {
            *task_label = None;
        }
        body
    }
}
