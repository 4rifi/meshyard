// Static no-eval Pixi polyfills preserve the unchanged production CSP.
import 'pixi.js/unsafe-eval';
import {
  Application,
  Container,
  Graphics,
  TextureStyle,
  type Ticker,
} from 'pixi.js';
import type { AgentView } from '../ipc/generated/AgentView';
import { initializeScene } from './sceneLifecycle';
import { activityLabel, yardPlots } from './yardData';
import { ink, pixelSprite } from './sprites';
import {
  plannedPath,
  step,
  walkRoute,
  yardGeometry,
  type Point,
  type Walk,
} from './yardPath';

TextureStyle.defaultOptions.scaleMode = 'nearest';
export type Yard = {
  update: (agents: AgentView[]) => void;
  destroy: () => void;
};
type Character = {
  agent: AgentView;
  body: Container;
  poses: [Graphics, Graphics];
  tag: HTMLSpanElement;
  tagOffset: number;
  position: Point;
  target: Point;
  walk: Walk | null;
};
const same = (a: Point, b: Point) => a.x === b.x && a.y === b.y;

export function createYard(
  host: HTMLDivElement,
  onError: () => void,
  onSelect?: (id: string) => void,
): Yard {
  const app = new Application();
  const environment = new Container();
  const avatars = new Container();
  const characters = new Map<string, Character>();
  // Keep each live UUID's station reservations, even while it visits another station.
  const reservations = new Map<string, Map<string, number>>();
  let agents: AgentView[] = [];
  let ready = false;
  let disposed = false;
  let observer: ResizeObserver | undefined;
  let media: MediaQueryList | undefined;
  let columns: 2 | 4 = 4;
  let basePages = 1;
  // Retain empty visited plots, so overflow growth/removal cannot shift other UUIDs
  // or erase a corridor underneath a character being retargeted.
  const plotPages = new Map<string, number>();
  const retainedPlots: ReturnType<typeof yardPlots> = [];
  let dimensions = { width: 320, height: 192 };
  const labels = document.createElement('div');
  labels.className = 'yard-labels';
  const stationLabels = document.createElement('div');
  labels.appendChild(stationLabels);
  const tag = (
    text: string,
    x: number,
    y: number,
    className: string,
    parent: HTMLElement,
  ) => {
    const label = document.createElement('span');
    label.className = className;
    label.textContent = text;
    label.style.left = `${String(x)}px`;
    label.style.top = `${String(y)}px`;
    parent.appendChild(label);
    return label;
  };
  const reserve = (id: string, station: string) => {
    const group = reservations.get(station) ?? new Map<string, number>();
    reservations.set(station, group);
    let slot = group.get(id);
    if (slot === undefined) {
      slot = 0;
      const used = new Set(group.values());
      while (used.has(slot)) slot += 1;
      group.set(id, slot);
    }
    return slot;
  };
  const place = (character: Character) => {
    character.body.position.set(character.position.x, character.position.y);
    character.tag.style.left = `${String(character.position.x - character.tagOffset)}px`;
    character.tag.style.top = `${String(character.position.y + 16)}px`;
    const pose =
      character.walk && !character.walk.done
        ? Math.floor(character.walk.elapsed * 8) % 2
        : 0;
    character.poses[0].visible = pose === 0;
    character.poses[1].visible = pose === 1;
  };
  const moving = () =>
    [...characters.values()].some((character) => character.walk !== null);
  const syncTicker = () => {
    if (moving() && !media?.matches) app.ticker.start();
    else app.ticker.stop();
  };
  const tick = (ticker: Ticker) => {
    for (const character of characters.values()) {
      if (!character.walk) continue;
      character.walk = step(character.walk, ticker.deltaMS / 1000);
      character.position = character.walk.position;
      if (character.walk.done) character.walk = null;
      place(character);
    }
    syncTicker();
  };
  const reducedMotion = () => {
    if (media?.matches) {
      for (const character of characters.values()) {
        character.position = character.target;
        character.walk = null;
        place(character);
      }
      syncTicker();
      app.render();
    }
  };
  const reconcile = (reflow = false) => {
    const ids = new Set(agents.map((agent) => agent.id));
    for (const [id, character] of characters) {
      if (!ids.has(id)) {
        character.body.destroy({ children: true });
        character.tag.remove();
        characters.delete(id);
      }
    }
    for (const group of reservations.values()) {
      for (const id of group.keys()) if (!ids.has(id)) group.delete(id);
    }
    const slots = new Map<string, number>();
    // Sorting applies only to newcomers; retained reservations never depend on DTO order.
    for (const agent of [...agents].sort((a, b) => a.id.localeCompare(b.id))) {
      const slot = reserve(agent.id, activityLabel(agent.state));
      slots.set(agent.id, slot);
      reserve(agent.id, activityLabel(null));
    }
    for (const group of reservations.values()) {
      for (const slot of group.values())
        basePages = Math.max(basePages, Math.floor(slot / 2) + 1);
    }
    const freshPlots = yardPlots(agents, columns, slots, basePages);
    const occurrences = new Map<string, number>();
    for (const plot of retainedPlots) {
      for (const station of plot.stations) station.agents = [];
    }
    for (const plot of freshPlots) {
      const kind =
        plot.stations[0]?.prop === null
          ? `neutral:${plot.stations[0].label}`
          : 'base';
      const ordinal = occurrences.get(kind) ?? 0;
      occurrences.set(kind, ordinal + 1);
      const key = `${kind}:${String(ordinal)}`;
      let page = plotPages.get(key);
      if (page === undefined) {
        page = retainedPlots.length;
        plotPages.set(key, page);
      }
      retainedPlots[page] = plot;
    }
    // Recompute retained empty plot coordinates too when the column count changes.
    for (const plot of retainedPlots) {
      plot.stations.forEach((station, index) => {
        station.x = (index % columns) * 80;
        station.y = Math.floor(index / columns) * 96;
      });
    }
    const plots = retainedPlots;
    const geometry = yardGeometry(plots, columns, slots);
    const tagWidths = new Map(
      plots.flatMap((plot) =>
        plot.stations.flatMap((station) =>
          station.agents.map(
            (agent) =>
              [agent.id, station.agents.length === 1 ? 76 : 36] as const,
          ),
        ),
      ),
    );
    const width = columns * 80;
    const plotHeight = (8 / columns) * 96;
    const height = plots.length * plotHeight;
    dimensions = { width, height };
    app.renderer.resize(width, height);
    for (const child of environment.removeChildren())
      child.destroy({ children: true });
    stationLabels.replaceChildren();
    const ground = new Graphics();
    for (let y = 0; y < height; y += 16) {
      for (let x = 0; x < width; x += 16) {
        ground.rect(x, y, 16, 16).fill(ink.grass);
        ground.rect(x + 2, y + 3, 2, 1).fill(ink.grassDark);
        ground.rect(x + 10, y + 11, 1, 2).fill(ink.grassDark);
      }
    }
    // Each row PAIR shares a horizontal path. A minimal existing-palette left lane
    // connects pairs and overflow pages; the pure graph uses the same geometry.
    if (height > 192) ground.rect(0, 80, 16, height - 176).fill(ink.path);
    for (let y = 80; y < height; y += 192) {
      ground.rect(0, y, width, 16).fill(ink.wood);
      ground.rect(0, y + 2, width, 12).fill(ink.path);
      for (let x = 0; x < width; x += 16) {
        ground.rect(x, y + 4, 2, 2).fill(ink.wood);
        ground.rect(x + 10, y + 10, 2, 1).fill(ink.wood);
      }
    }
    plots.forEach((plot, page) => {
      const offset = page * plotHeight;
      plot.stations.forEach((station) => {
        const pathY = offset + Math.floor(station.y / 192) * 192 + 88;
        const centreY = offset + station.y + 62;
        ground
          .rect(station.x + 4, offset + station.y + 24, 72, 40)
          .fill(ink.grassDark);
        ground
          .rect(station.x + 6, offset + station.y + 26, 68, 36)
          .fill(ink.grass);
        ground
          .rect(
            station.x + 8,
            Math.min(pathY, centreY),
            16,
            Math.abs(pathY - centreY) + 1,
          )
          .fill(ink.path);
        ground.rect(station.x + 12, centreY - 8, 56, 10).fill(ink.path);
      });
    });
    environment.addChild(ground);
    plots.forEach((plot, page) => {
      const offset = page * plotHeight;
      // Trees stay off the route lane and preserve the existing sprite palette.
      environment.addChild(pixelSprite('tree', width - 16, offset + 64));
      plot.stations.forEach((station) => {
        if (station.prop)
          environment.addChild(
            pixelSprite(station.prop, station.x + 32, offset + station.y + 28),
          );
        tag(
          station.label,
          station.x + 2,
          offset + station.y + 8,
          'station-label',
          stationLabels,
        );
      });
    });
    for (const agent of agents) {
      const target = geometry.slots.get(agent.id);
      if (!target) continue;
      let character = characters.get(agent.id);
      if (!character) {
        const arrivalSlot = reserve(agent.id, activityLabel(null));
        const position = {
          x: 12 + (arrivalSlot % 2) * 40,
          y:
            (plotPages.get(`base:${String(Math.floor(arrivalSlot / 2))}`) ??
              0) *
              plotHeight +
            46,
        };
        const poses: [Graphics, Graphics] = [
          pixelSprite('agent', 0, 0, agent.kind),
          pixelSprite('agent', 0, 0, agent.kind, 1),
        ];
        const body = new Container();
        body.eventMode = 'static';
        body.cursor = 'pointer';
        body.on('pointertap', () => {
          onSelect?.(agent.id);
        });
        body.addChild(...poses);
        avatars.addChild(body);
        character = {
          agent,
          body,
          poses,
          tag: tag(agent.label, 0, 0, 'agent-tag', labels),
          tagOffset: 10,
          position,
          target: position,
          walk: null,
        };
        character.tag.dataset.agentId = agent.id;
        character.tag.style.pointerEvents = 'auto';
        character.tag.style.cursor = 'pointer';
        character.tag.onclick = () => {
          onSelect?.(agent.id);
        };
        characters.set(agent.id, character);
      }
      character.agent = agent;
      character.tag.textContent = agent.stale
        ? `${agent.label} · Unknown`
        : agent.label;
      character.tag.classList.toggle('stale-state', agent.stale);
      for (const pose of character.poses) pose.alpha = agent.stale ? 0.55 : 1;
      character.tag.title = `${agent.label} · ${agent.stale ? `Unknown (no events for 10 min) · Last: ${activityLabel(agent.state)}` : activityLabel(agent.state)}`;
      character.tag.dataset.kind = agent.kind;
      const tagWidth = tagWidths.get(agent.id) ?? 36;
      character.tag.style.width = `${String(tagWidth)}px`;
      // A lone second-slot occupant uses the whole station without moving its body.
      character.tagOffset =
        10 + (tagWidth === 76 ? ((slots.get(agent.id) ?? 0) % 2) * 40 : 0);
      if (reflow || media?.matches) {
        // Layout changes snap existing identities to their new logical slots. They
        // never invent a state transition or replace an avatar/name-tag instance.
        character.position = target;
        character.walk = null;
      } else if (!same(character.target, target)) {
        const route = walkRoute(
          plannedPath(character.position, target, geometry),
        );
        character.walk = route.done ? null : route;
      }
      character.target = target;
      place(character);
    }
    labels.style.width = `${String(width)}px`;
    labels.style.height = `${String(height)}px`;
    syncTicker();
    app.render();
  };
  const align = () => {
    const available = host.parentElement?.clientWidth ?? 320;
    const scale = Math.max(1, Math.floor(available / dimensions.width));
    host.style.setProperty('--yard-scale', String(scale));
    host.style.width = `${String(dimensions.width * scale)}px`;
    host.style.marginLeft = `${String(Math.max(0, Math.floor((available - dimensions.width * scale) / 2)))}px`;
    host.style.marginRight = '0';
    host.style.height = `${String(dimensions.height * scale)}px`;
    app.canvas.style.width = `${String(dimensions.width * scale)}px`;
    app.canvas.style.height = `${String(dimensions.height * scale)}px`;
    labels.style.transformOrigin = 'top left';
    labels.style.transform = `scale(${String(scale)})`;
    host.style.left = '0px';
    host.style.top = '0px';
    const origin = host.getBoundingClientRect();
    host.style.left = `${String(Math.ceil(origin.x) - origin.x)}px`;
    host.style.top = `${String(Math.ceil(origin.y) - origin.y)}px`;
  };
  const destroy = initializeScene(
    {
      initialize: () =>
        app.init({
          width: 320,
          height: 192,
          resolution: 1,
          background: ink.grass,
          preference: 'webgl',
          antialias: false,
          roundPixels: true,
          autoStart: false,
          sharedTicker: false,
          manageImports: false,
        }),
      attach: () => {
        app.canvas.setAttribute('aria-hidden', 'true');
        app.stage.addChild(environment, avatars);
        host.append(app.canvas, labels);
        app.ticker.add(tick);
        media = window.matchMedia('(prefers-reduced-motion: reduce)');
        media.addEventListener('change', reducedMotion);
        columns = (host.parentElement?.clientWidth ?? 320) >= 640 ? 4 : 2;
        ready = true;
        reconcile();
        align();
        observer = new ResizeObserver(() => {
          const next = (host.parentElement?.clientWidth ?? 320) >= 640 ? 4 : 2;
          if (next !== columns) {
            columns = next;
            reconcile(true);
          }
          align();
        });
        if (host.parentElement) observer.observe(host.parentElement);
      },
      destroy: () => {
        ready = false;
        observer?.disconnect();
        media?.removeEventListener('change', reducedMotion);
        app.ticker.stop();
        app.ticker.remove(tick);
        characters.clear();
        reservations.clear();
        retainedPlots.length = 0;
        plotPages.clear();
        labels.remove();
        app.destroy(true, { children: true });
      },
    },
    onError,
  );
  return {
    update: (next) => {
      if (disposed) return;
      agents = next;
      if (ready) {
        reconcile();
        align();
      }
    },
    destroy: () => {
      disposed = true;
      destroy();
    },
  };
}
