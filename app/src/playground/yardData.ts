import type { ActivityState } from '../ipc/generated/ActivityState';
import type { AgentView } from '../ipc/generated/AgentView';
import type { Prop } from './sprites';
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

/** Reserved places are visible, but hooks never synthesize their activity. */
export const stations = [
  { state: null, label: 'Arrival', prop: 'arrival', reserved: false },
  { state: 'Thinking', label: 'Thinking', prop: 'desk', reserved: false },
  { state: 'Building', label: 'Build', prop: 'workbench', reserved: false },
  { state: 'Testing', label: 'Test', prop: 'test', reserved: false },
  { state: 'Idle', label: 'Idle', prop: 'bench', reserved: false },
  { state: 'Merging', label: 'Merge', prop: 'gate', reserved: false },
  { state: 'Finished', label: 'Finished', prop: 'flag', reserved: false },
  { state: 'Error', label: 'Error', prop: 'alarm', reserved: false },
  { state: 'Reviewing', label: 'Review', prop: 'review', reserved: true },
  { state: 'Fixing', label: 'Fix', prop: 'repair', reserved: true },
  {
    state: 'WaitingForInput',
    label: 'Waiting',
    prop: 'waiting',
    reserved: true,
  },
] as const;
export const stationHeight = 64;
export const plotHeight = (columns: 2 | 4) =>
  Math.ceil(stations.length / columns) * stationHeight;
export function yardPlots(
  agents: AgentView[],
  columns: 2 | 4 = 4,
  slots?: ReadonlyMap<string, number>,
) {
  type Station = {
    label: string;
    prop: Prop | null;
    reserved: boolean;
    x: number;
    y: number;
    agents: AgentView[];
  };
  const plots: { key: string; stations: Station[] }[] = [
    { key: 'base', stations: [] },
  ];
  stations.forEach((station, index) => {
    const group = agents.filter((agent) => agent.state === station.state);
    const pages = Math.max(
      1,
      ...group.map(
        (agent, ordinal) =>
          Math.floor((slots?.get(agent.id) ?? ordinal) / 2) + 1,
      ),
    );
    for (let page = 0; page < pages; page++) {
      const occupants = group.filter(
        (agent, ordinal) =>
          Math.floor((slots?.get(agent.id) ?? ordinal) / 2) === page,
      );
      const place = {
        ...station,
        x: page === 0 ? (index % columns) * 80 : 0,
        y: page === 0 ? Math.floor(index / columns) * stationHeight : 0,
        agents: occupants,
      };
      if (page === 0) plots[0]?.stations.push(place);
      else if (occupants.length)
        plots.push({
          key: `overflow:${station.label}:${String(page)}`,
          stations: [{ ...place, label: `${station.label} +` }],
        });
    }
  });
  return plots;
}
