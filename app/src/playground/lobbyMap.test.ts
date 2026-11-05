import { describe, expect, it } from 'vitest';
import data from '../assets/yard/lobby-map.json';
import { lobbyGeometry, lobbyNodes, lobbyStations } from './lobbyMap';
import { plannedPath, step, walkRoute, type Point } from './yardPath';
import type { AgentView } from '../ipc/generated/AgentView';
const shape = (vertices: number[][]): Point[] =>
  vertices.map(([x, y]) => {
    if (x === undefined || y === undefined)
      throw new Error('Incomplete polygon');
    return { x, y };
  });
const inside = (p: Point, vertices: Point[], boundary = true) => {
  let result = false;
  for (let i = 0, j = vertices.length - 1; i < vertices.length; j = i++) {
    const a = vertices[i],
      b = vertices[j];
    if (!a || !b) continue;
    const cross = (p.x - a.x) * (b.y - a.y) - (p.y - a.y) * (b.x - a.x);
    const dot = (p.x - a.x) * (p.x - b.x) + (p.y - a.y) * (p.y - b.y);
    if (Math.abs(cross) < 1e-7 && dot <= 0) return boundary;
    if (
      a.y > p.y !== b.y > p.y &&
      p.x < ((b.x - a.x) * (p.y - a.y)) / (b.y - a.y) + a.x
    )
      result = !result;
  }
  return result;
};
const obstacles = data.obstacles.map((o) => ({
  name: o.name,
  polygon: shape(o.points),
}));
const mask = shape(data.occluder.polygon);
const agent = (id: string, state: AgentView['state']): AgentView => ({
  id,
  label: id,
  state,
  kind: 'ClaudeCode',
  provenance: null,
  is_you: true,
  bound: false,
  stale: false,
  connection: 'NotConnected',
});
describe('owner lobby map navigation', () => {
  it('has exactly one entrance per domain state and three clear feet slots', () => {
    expect(lobbyStations).toHaveLength(11);
    expect(new Set(lobbyStations.map((s) => s.state)).size).toBe(11);
    for (const station of lobbyStations) {
      const entry = lobbyNodes[station.entry];
      expect(entry).toBeDefined();
      expect(station.feet).toHaveLength(3);
      for (const point of [entry, ...station.feet]) {
        if (!point) throw new Error('Missing entrance');
        expect(inside(point, mask), `${station.label} in monitor`).toBe(false);
        for (const o of obstacles)
          expect(
            inside(point, o.polygon),
            `${station.label} inside ${o.name}`,
          ).toBe(false);
      }
    }
  });
  it('keeps visible edges outside props and confines the marked transit edge to the monitor mask', () => {
    for (const edge of data.edges) {
      const a = lobbyNodes[edge.from],
        b = lobbyNodes[edge.to];
      if (!a || !b) throw new Error('Missing edge node');
      for (let sample = 1; sample < 100; sample++) {
        const t = sample / 100;
        const point = { x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t };
        for (const o of obstacles)
          expect(
            inside(point, o.polygon),
            `${edge.from}->${edge.to} enters ${o.name} at ${JSON.stringify(point)}`,
          ).toBe(false);
        expect(
          inside(point, mask),
          `${edge.from}->${edge.to} monitor visibility`,
        ).toBe('behindOccluder' in edge);
      }
    }
  });
  it.each(lobbyStations)(
    'connects $label to every station and preserves rounded diagonal retargets',
    (station) => {
      const agents = lobbyStations.map((s, i) => agent(String(i), s.state));
      const geometry = lobbyGeometry(agents, new Map());
      const source = agents.find((a) => a.state === station.state);
      if (!source) throw new Error('Missing source station');
      for (const target of agents) {
        const from = geometry.slots.get(source.id),
          to = geometry.slots.get(target.id);
        if (!from || !to) throw new Error('Missing feet');
        const route = plannedPath(from, to, geometry);
        expect(route[0]).toEqual(from);
        expect(route.at(-1)).toEqual(to);
        const walk = walkRoute(route);
        expect(step(walk, 3).position).toEqual(to);
        for (let t = 0; t < walk.duration; t += 0.137) {
          const current = step(walk, t).position;
          const retarget = plannedPath(current, from, geometry);
          expect(retarget[0]).toEqual(current);
          expect(step(walkRoute(retarget), 3).position).toEqual(from);
        }
      }
    },
  );
  it('interpolates diagonal distance rather than moving a full axis distance twice', () => {
    const walk = walkRoute([
      { x: 0, y: 0 },
      { x: 48, y: 64 },
    ]);
    expect(walk.duration).toBeCloseTo(80 / 48);
    expect(step(walk, walk.duration / 2).position).toEqual({ x: 24, y: 32 });
  });
});
