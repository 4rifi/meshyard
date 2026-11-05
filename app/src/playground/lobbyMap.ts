import data from '../assets/yard/lobby-map.json';
import type { AgentView } from '../ipc/generated/AgentView';
import { stations } from './yardData';
import type { Point, YardGeometry } from './yardPath';

export const lobbySize = data.nativeSize;
export const lobbyNodes: Readonly<Record<string, Point>> = data.nodes;
export const lobbyStations = data.stations.map((item) => {
  const domain = stations.find((station) => station.state === item.state);
  if (!domain) throw new Error('Unknown lobby station state');
  return { ...item, state: domain.state };
});
const node = (id: string): Point => {
  const point = lobbyNodes[id];
  if (!point) throw new Error('Unknown lobby waypoint');
  return point;
};
/** One immutable map; capacity is expressed as occupants, never duplicated buildings. */
export function lobbyGeometry(
  agents: AgentView[],
  reservations: ReadonlyMap<string, number>,
): YardGeometry {
  const lanes = data.edges.map(({ from, to }) => {
    if (!from || !to) throw new Error('Incomplete lobby path');
    return { from: node(from), to: node(to) };
  });
  const slots = new Map<string, Point>();
  for (const station of lobbyStations) {
    const entry = node(station.entry);
    for (const foot of station.feet) lanes.push({ from: entry, to: foot });
    for (const agent of agents.filter((a) => a.state === station.state)) {
      const foot = station.feet[reservations.get(agent.id) ?? 0];
      if (foot) slots.set(agent.id, foot);
    }
  }
  return { lanes, slots };
}
export function stationFor(agent: AgentView) {
  const station = lobbyStations.find((s) => s.state === agent.state);
  if (!station) throw new Error('Agent state has no map entrance');
  return station;
}
