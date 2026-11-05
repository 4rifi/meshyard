import { plotHeight, type yardPlots } from './yardData';

export type Point = { x: number; y: number };
export type Lane = { from: Point; to: Point };
export type YardGeometry = { lanes: Lane[]; slots: Map<string, Point> };
const key = (point: Point) => `${String(point.x)},${String(point.y)}`;
const distance = (a: Point, b: Point) => Math.hypot(a.x - b.x, a.y - b.y);

/** Sprite origins: feet sit eight pixels right and sixteen pixels below these lanes. */
export function yardGeometry(
  plots: ReturnType<typeof yardPlots>,
  columns: 2 | 4,
  slots: ReadonlyMap<string, number>,
): YardGeometry {
  const lanes: Lane[] = [];
  const destinations = new Map<string, Point>();
  const rows = new Set<number>();
  plots.forEach((plot, page) => {
    const offset = page * plotHeight(columns);
    plot.stations.forEach((station) => {
      const y = offset + Math.floor(station.y / 128) * 128 + 40;
      rows.add(y);
      const centre = { x: station.x + 8, y: offset + station.y + 28 };
      lanes.push({ from: centre, to: { x: centre.x, y } });
      // Both permanent slots join the door, including slots vacated during a walk.
      for (const x of [station.x + 12, station.x + 52]) {
        lanes.push({ from: { x, y: centre.y }, to: centre });
      }
      for (const agent of station.agents) {
        destinations.set(agent.id, {
          x: station.x + 12 + ((slots.get(agent.id) ?? 0) % 2) * 40,
          y: centre.y,
        });
      }
    });
  });
  const sorted = [...rows].sort((a, b) => a - b);
  sorted.forEach((y, index) => {
    lanes.push({ from: { x: 0, y }, to: { x: columns * 80 - 16, y } });
    const previous = sorted[index - 1];
    if (previous !== undefined)
      lanes.push({ from: { x: 0, y: previous }, to: { x: 0, y } });
  });
  return { lanes, slots: destinations };
}

export const segmentContains = (point: Point, lane: Lane, tolerance = 0.71) => {
  const dx = lane.to.x - lane.from.x,
    dy = lane.to.y - lane.from.y;
  const squared = dx * dx + dy * dy;
  if (squared === 0) return distance(point, lane.from) < 0.01;
  const fraction =
    ((point.x - lane.from.x) * dx + (point.y - lane.from.y) * dy) / squared;
  const perpendicular =
    Math.abs((point.x - lane.from.x) * dy - (point.y - lane.from.y) * dx) /
    Math.sqrt(squared);
  // A rounded mid-walk pixel can lie at most sqrt(0.5) away from its segment.
  return (
    fraction >= -tolerance / Math.sqrt(squared) &&
    fraction <= 1 + tolerance / Math.sqrt(squared) &&
    perpendicular <= tolerance
  );
};
const onLane = segmentContains;
const intersection = (a: Lane, b: Lane): Point | null => {
  const ax = a.to.x - a.from.x,
    ay = a.to.y - a.from.y,
    bx = b.to.x - b.from.x,
    by = b.to.y - b.from.y;
  const cross = ax * by - ay * bx;
  if (Math.abs(cross) < 0.0001) return null;
  const dx = b.from.x - a.from.x,
    dy = b.from.y - a.from.y;
  const t = (dx * by - dy * bx) / cross,
    u = (dx * ay - dy * ax) / cross;
  if (t < 0 || t > 1 || u < 0 || u > 1) return null;
  return {
    x: Math.round((a.from.x + t * ax) * 1e6) / 1e6,
    y: Math.round((a.from.y + t * ay) * 1e6) / 1e6,
  };
};

/** Split the actual corridors at intersections and at the current mid-walk position. */
export function plannedPath(
  from: Point,
  to: Point,
  geometry: YardGeometry,
): Point[] {
  if (distance(from, to) === 0) return [from];
  const lanes = [...geometry.lanes];
  // Rejoin the exact graph from a rounded sprite pixel without bending its corridors.
  if (!lanes.some((lane) => onLane(from, lane, 0.00001))) {
    const projections = lanes
      .map((lane) => {
        const dx = lane.to.x - lane.from.x,
          dy = lane.to.y - lane.from.y;
        const squared = dx * dx + dy * dy;
        const t = squared
          ? Math.max(
              0,
              Math.min(
                1,
                ((from.x - lane.from.x) * dx + (from.y - lane.from.y) * dy) /
                  squared,
              ),
            )
          : 0;
        return { x: lane.from.x + t * dx, y: lane.from.y + t * dy };
      })
      .sort((a, b) => distance(from, a) - distance(from, b));
    const closest = projections[0];
    if (!closest || distance(from, closest) > 0.71) return [];
    lanes.push({ from, to: closest });
  }
  const points = new Map<string, Point>([
    [key(from), from],
    [key(to), to],
  ]);
  for (const lane of lanes) {
    points.set(key(lane.from), lane.from);
    points.set(key(lane.to), lane.to);
    for (const other of lanes) {
      const crossing = intersection(lane, other);
      if (crossing) points.set(key(crossing), crossing);
    }
  }
  const edges = new Map<string, Point[]>();
  for (const lane of lanes) {
    const members = [...points.values()]
      .filter((point) => onLane(point, lane, 0.00001))
      .sort((a, b) => distance(lane.from, a) - distance(lane.from, b));
    members.forEach((point, index) => {
      const adjacent = edges.get(key(point)) ?? [];
      for (const neighbour of [members[index - 1], members[index + 1]]) {
        if (neighbour) adjacent.push(neighbour);
      }
      edges.set(key(point), adjacent);
    });
  }
  const costs = new Map<string, number>([[key(from), 0]]);
  const previous = new Map<string, Point>();
  const pending = new Map(points);
  while (pending.size) {
    const current = [...pending.values()].sort(
      (a, b) =>
        (costs.get(key(a)) ?? Infinity) - (costs.get(key(b)) ?? Infinity),
    )[0];
    if (!current || !costs.has(key(current))) break;
    pending.delete(key(current));
    if (key(current) === key(to)) {
      const path = [to];
      let cursor = to;
      while (key(cursor) !== key(from)) {
        const parent = previous.get(key(cursor));
        if (!parent) return [];
        path.unshift(parent);
        cursor = parent;
      }
      return path;
    }
    for (const neighbour of edges.get(key(current)) ?? []) {
      const cost =
        (costs.get(key(current)) ?? 0) + distance(current, neighbour);
      if (cost < (costs.get(key(neighbour)) ?? Infinity)) {
        costs.set(key(neighbour), cost);
        previous.set(key(neighbour), current);
      }
    }
  }
  return [];
}

export type Walk = {
  path: Point[];
  elapsed: number;
  duration: number;
  position: Point;
  done: boolean;
};
export function walkRoute(path: Point[]): Walk {
  const length = path.reduce(
    (sum, point, index) => sum + distance(path[index - 1] ?? point, point),
    0,
  );
  return {
    path,
    elapsed: 0,
    duration: Math.min(3, length / 48),
    position: path[0] ?? { x: 0, y: 0 },
    done: length === 0,
  };
}

/** Measure from the route origin, avoiding frame-rate-dependent integer rounding drift. */
export function step(walk: Walk, dt: number): Walk {
  const elapsed = Math.min(walk.duration, walk.elapsed + Math.max(0, dt));
  const length = walk.path.reduce(
    (sum, point, index) => sum + distance(walk.path[index - 1] ?? point, point),
    0,
  );
  let remaining = walk.duration ? (length * elapsed) / walk.duration : 0;
  let position = walk.path[0] ?? walk.position;
  for (let index = 1; index < walk.path.length; index += 1) {
    const end = walk.path[index];
    if (!end) continue;
    const segment = distance(position, end);
    if (remaining < segment) {
      position = {
        x: Math.round(
          position.x + ((end.x - position.x) * remaining) / segment,
        ),
        y: Math.round(
          position.y + ((end.y - position.y) * remaining) / segment,
        ),
      };
      break;
    }
    remaining -= segment;
    position = end;
  }
  return { ...walk, elapsed, position, done: elapsed >= walk.duration };
}
