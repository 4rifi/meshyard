import { describe, expect, it } from 'vitest';
import type { AgentView } from '../ipc/generated/AgentView';
import { yardPlots } from './yardData';
import {
  plannedPath,
  step,
  walkRoute,
  yardGeometry,
  type Point,
} from './yardPath';
const agent = (id: string, state: AgentView['state']): AgentView => ({
  id,
  state,
  label: id,
  kind: 'ClaudeCode',
  provenance: null,
  is_you: true,
  stale: false,
  bound: false,
  connection: 'NotConnected' as const,
});
const length = (path: Point[]) =>
  path.reduce((sum, point, index) => {
    const previous = path[index - 1] ?? point;
    return (
      sum + Math.abs(previous.x - point.x) + Math.abs(previous.y - point.y)
    );
  }, 0);

describe('actual yard corridor paths', () => {
  it.each([2, 4] as const)(
    'connects same-station slots, row pairs and overflow pages (%i columns)',
    (columns) => {
      const agents = [
        agent('a', null),
        agent('b', null),
        agent('c', 'Finished'),
        agent('d', 'Error'),
      ];
      const slots = new Map([
        ['a', 0],
        ['b', 1],
        ['c', 0],
        ['d', 4],
      ]);
      const geometry = yardGeometry(
        yardPlots(agents, columns, slots),
        columns,
        slots,
      );
      const arrival = geometry.slots.get('a');
      if (!arrival) throw new Error('Missing fixture slot');
      for (const id of ['b', 'c', 'd']) {
        const target = geometry.slots.get(id);
        if (!target) throw new Error('Missing fixture target');
        const path = plannedPath(arrival, target, geometry);
        expect(path[0]).toEqual(arrival);
        expect(path.at(-1)).toEqual(target);
        for (const [index, point] of path.entries()) {
          const previous = path[index - 1] ?? point;
          expect(Number.isInteger(point.x) && Number.isInteger(point.y)).toBe(
            true,
          );
          expect(point.x === previous.x || point.y === previous.y).toBe(true);
          expect(
            geometry.lanes.some(
              (lane) =>
                length([lane.from, previous, point, lane.to]) ===
                  length([lane.from, lane.to]) ||
                length([lane.from, point, previous, lane.to]) ===
                  length([lane.from, lane.to]),
            ),
          ).toBe(true);
        }
        if (id === 'd') expect(path.some((point) => point.x === 0)).toBe(true);
        const walk = walkRoute(path);
        expect(walk.duration).toBeLessThanOrEqual(3);
        expect(step(walk, 3)).toMatchObject({ position: target, done: true });
      }
    },
  );
  it('moves at 48 pixels/sec, preserves subpixel time and caps long routes at three seconds', () => {
    const short = walkRoute([
      { x: 0, y: 0 },
      { x: 96, y: 0 },
    ]);
    expect(short.duration).toBe(2);
    expect(step(short, 0.5).position).toEqual({ x: 24, y: 0 });
    let frames = short;
    for (let frame = 0; frame < 30; frame += 1) frames = step(frames, 1 / 60);
    expect(frames.position).toEqual(step(short, 0.5).position);
    const capped = walkRoute([
      { x: 0, y: 0 },
      { x: 480, y: 0 },
    ]);
    expect(capped.duration).toBe(3);
    expect(step(capped, 1.5).position).toEqual({ x: 240, y: 0 });
    expect(step(capped, 10)).toMatchObject({
      done: true,
      position: { x: 480, y: 0 },
    });
    expect(step(short, -1).position).toEqual({ x: 0, y: 0 });
  });
  it.each([2, 4] as const)(
    'retargets from every sampled current corridor position without teleport (%i columns)',
    (columns) => {
      const agents = [
        agent('a', null),
        agent('b', 'Testing'),
        agent('c', 'Finished'),
      ];
      const geometry = yardGeometry(
        yardPlots(agents, columns),
        columns,
        new Map(),
      );
      const start = geometry.slots.get('a');
      const first = geometry.slots.get('b');
      const next = geometry.slots.get('c');
      if (!start || !first || !next)
        throw new Error('Missing fixture destinations');
      const walk = walkRoute(plannedPath(start, first, geometry));
      for (let time = 0; time < walk.duration; time += 0.037) {
        const current = step(walk, time).position;
        const replan = plannedPath(current, next, geometry);
        expect(replan[0]).toEqual(current);
        expect(step(walkRoute(replan), 3).position).toEqual(next);
      }
    },
  );
});
