import type { ActivityState } from '../ipc/generated/ActivityState';
import type { AgentView } from '../ipc/generated/AgentView';
const labels: Record<ActivityState, string> = {
  Idle: 'Idle',
  Thinking: 'Thinking',
  Building: 'Building',
  Reviewing: 'Reviewing',
  Testing: 'Testing',
  Fixing: 'Fixing',
  WaitingForInput: 'Waiting for input',
  Merging: 'Merging',
  Finished: 'Finished',
  Error: 'Error',
};
export function activityLabel(state: ActivityState | null): string {
  return state === null ? 'No events yet' : labels[state];
}
/** Both token count and zones derive solely from the current backend agent view. */
export function yardZones(
  agents: AgentView[],
): { label: string; agents: AgentView[] }[] {
  const zones = new Map<string, AgentView[]>();
  for (const agent of agents) {
    const label = activityLabel(agent.state);
    const zone = zones.get(label) ?? [];
    zone.push(agent);
    zones.set(label, zone);
  }
  return Array.from(zones, ([label, group]) => ({ label, agents: group }));
}
