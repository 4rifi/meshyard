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

/** Only states emitted by frozen wire v1 receive a named station. */
export const stations = [
  { state: null, label: 'Arrival', prop: 'arrival' },
  { state: 'Thinking', label: 'Thinking', prop: 'desk' },
  { state: 'Building', label: 'Building', prop: 'workbench' },
  { state: 'Testing', label: 'Testing', prop: 'test' },
  { state: 'Idle', label: 'Idle', prop: 'bench' },
  { state: 'Merging', label: 'Merging', prop: 'gate' },
  { state: 'Finished', label: 'Finished', prop: 'flag' },
  { state: 'Error', label: 'Error', prop: 'alarm' },
] as const;

export function yardPlots(
  agents: AgentView[],
  columns: 2 | 4 = 4,
  slots?: ReadonlyMap<string, number>,
  minPlots = 1,
) {
  const groups = yardZones(agents);
  // Two distinct 16px character tiles per station and plot. Extra plots keep all
  // real agents visible without shrinking pixels or dropping crowded registries.
  const count = Math.max(
    minPlots,
    ...groups.map((zone) =>
      Math.max(
        ...zone.agents.map(
          (agent, index) => Math.floor((slots?.get(agent.id) ?? index) / 2) + 1,
        ),
      ),
    ),
  );
  type Plot = {
    stations: {
      label: string;
      prop: Prop | null;
      x: number;
      y: number;
      agents: AgentView[];
    }[];
  };
  const plots: Plot[] = Array.from({ length: count }, (_, page) => ({
    stations: stations.map((station, index) => ({
      ...station,
      x: (index % columns) * 80,
      y: Math.floor(index / columns) * 96,
      agents: (
        groups.find((zone) => zone.label === activityLabel(station.state))
          ?.agents ?? []
      ).filter(
        (agent, index) =>
          Math.floor((slots?.get(agent.id) ?? index) / 2) === page,
      ),
    })),
  }));
  // Preserve future DTO states as neutral plots, with their exact labels and no
  // invented workstation. Today's pipeline never emits these three states.
  for (const zone of groups.filter(
    (group) =>
      !stations.some((station) => activityLabel(station.state) === group.label),
  )) {
    const pages = Math.max(
      ...zone.agents.map(
        (agent, index) => Math.floor((slots?.get(agent.id) ?? index) / 2) + 1,
      ),
    );
    for (let page = 0; page < pages; page += 1) {
      plots.push({
        stations: [
          {
            label: zone.label,
            prop: null,
            x: 0,
            y: 0,
            agents: zone.agents.filter(
              (agent, index) =>
                Math.floor((slots?.get(agent.id) ?? index) / 2) === page,
            ),
          },
        ],
      });
    }
  }
  return plots;
}
