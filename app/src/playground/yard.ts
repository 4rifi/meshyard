// Static no-eval Pixi polyfills preserve the production CSP.
import 'pixi.js/unsafe-eval';
import {
  Application,
  Assets,
  Container,
  Graphics,
  Sprite,
  TextureStyle,
  type Texture,
  type Ticker,
} from 'pixi.js';
import lobbyUrl from '../assets/yard/lobby-native.png';
import monitorUrl from '../assets/yard/lobby-monitor.png';
import type { AgentView } from '../ipc/generated/AgentView';
import { initializeScene } from './sceneLifecycle';
import { activityLabel } from './yardData';
import {
  lobbyGeometry,
  lobbySize,
  lobbyStations,
  stationFor,
} from './lobbyMap';
import { ink, personSprite } from './sprites';
import {
  plannedPath,
  step,
  walkRoute,
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
  poses: [Graphics, Graphics, Graphics, Graphics];
  tag: HTMLSpanElement;
  leader: HTMLSpanElement;
  position: Point;
  target: Point;
  walk: Walk | null;
  tagTop: number;
  tagWidth: number;
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
  const reservations = new Map<string, Map<string, number>>();
  let texture: Texture | undefined;
  let monitor: Texture | undefined;
  let agents: AgentView[] = [];
  let ready = false;
  let disposed = false;
  let observer: ResizeObserver | undefined;
  let poseTimer: ReturnType<typeof setInterval> | undefined;
  let media: MediaQueryList | undefined;
  let restFrame = 0;
  host.classList.add('lobby-map');
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
  const reserve = (agent: AgentView) => {
    const station = stationFor(agent);
    const group = reservations.get(station.label) ?? new Map<string, number>();
    reservations.set(station.label, group);
    let slot = group.get(agent.id);
    const used = new Set(
      [...group].filter(([id]) => id !== agent.id).map(([, n]) => n),
    );
    if (
      slot === undefined ||
      (slot >= 3 && [0, 1, 2].some((n) => !used.has(n)))
    ) {
      slot = 0;
      while (used.has(slot)) slot++;
      group.set(agent.id, slot);
    }
    return slot;
  };
  const place = (c: Character) => {
    c.body.position.set(c.position.x - 4, c.position.y - 10);
    c.tag.dataset.footX = String(c.position.x);
    c.tag.dataset.footY = String(c.position.y);
    c.tag.dataset.walking = String(c.walk !== null);
    const tagY = c.walk ? c.position.y + 2 : c.tagTop;
    c.tag.style.left = `${String(Math.max(0, Math.min(lobbySize.width - c.tagWidth, Math.round(c.position.x - c.tagWidth / 2))))}px`;
    c.tag.style.top = `${String(Math.min(lobbySize.height - 10, tagY))}px`;
    c.leader.style.left = `${String(c.position.x)}px`;
    c.leader.style.top = `${String(c.position.y)}px`;
    c.leader.style.height = `${String(Math.max(0, tagY - c.position.y))}px`;
    const pose =
      c.walk && !c.walk.done
        ? Math.floor(c.walk.elapsed * 8) % 2
        : restFrame === 0
          ? 0
          : ['Building', 'Testing', 'Merging'].includes(c.agent.state ?? '')
            ? 3
            : 2;
    c.poses.forEach((drawing, i) => {
      drawing.visible = i === pose;
    });
  };
  const syncTicker = () => {
    if ([...characters.values()].some((c) => c.walk) && !media?.matches)
      app.ticker.start();
    else app.ticker.stop();
  };
  const tick = (ticker: Ticker) => {
    for (const c of characters.values()) {
      if (!c.walk) continue;
      c.walk = step(c.walk, ticker.deltaMS / 1000);
      c.position = c.walk.position;
      if (c.walk.done) c.walk = null;
      place(c);
    }
    syncTicker();
  };
  const reducedMotion = () => {
    if (media?.matches) {
      restFrame = 0;
      for (const c of characters.values()) {
        c.position = c.target;
        c.walk = null;
        place(c);
      }
      syncTicker();
      app.render();
    }
  };
  const reconcile = () => {
    const ids = new Set(agents.map((a) => a.id));
    for (const [id, c] of characters) {
      if (!ids.has(id)) {
        c.body.destroy({ children: true });
        c.tag.remove();
        c.leader.remove();
        characters.delete(id);
      }
    }
    for (const [label, group] of reservations) {
      for (const id of group.keys()) {
        if (!agents.some((a) => a.id === id && stationFor(a).label === label))
          group.delete(id);
      }
    }
    const slots = new Map<string, number>();
    for (const a of [...agents].sort((a, b) => a.id.localeCompare(b.id)))
      slots.set(a.id, reserve(a));
    const geometry = lobbyGeometry(agents, slots);
    stationLabels.replaceChildren();
    for (const station of lobbyStations) {
      const count = agents.filter((a) => a.state === station.state).length;
      const label = tag(
        `${station.label}${count ? ` · ${String(count)}` : ''}`,
        station.labelAnchor.x,
        station.labelAnchor.y,
        station.reserved ? 'station-label reserved-label' : 'station-label',
        stationLabels,
      );
      label.dataset.station = station.label;
      label.title = station.reserved
        ? 'Reserved: hooks do not produce this state'
        : station.label;
      if (count > 3)
        label.title += ` · ${String(count - 3)} additional agents in Agent list`;
    }
    for (const agent of agents) {
      const station = stationFor(agent);
      const target = geometry.slots.get(agent.id);
      const visible = target !== undefined;
      let c = characters.get(agent.id);
      if (!c) {
        const position = target ?? station.feet[0];
        if (!position) continue;
        const poses: [Graphics, Graphics, Graphics, Graphics] = [
          personSprite(agent.kind, 0),
          personSprite(agent.kind, 1),
          personSprite(agent.kind, 2),
          personSprite(agent.kind, 3),
        ];
        const body = new Container();
        body.eventMode = 'static';
        body.cursor = 'pointer';
        body.on('pointertap', () => onSelect?.(agent.id));
        body.addChild(...poses);
        avatars.addChild(body);
        const leader = tag('', 0, 0, 'agent-leader', labels);
        const name = tag(agent.label, 0, 0, 'agent-tag', labels);
        name.dataset.agentId = agent.id;
        name.style.pointerEvents = 'auto';
        name.style.cursor = 'pointer';
        name.onclick = () => onSelect?.(agent.id);
        c = {
          agent,
          body,
          poses,
          tag: name,
          leader,
          position,
          target: position,
          walk: null,
          tagTop: position.y + 2,
          tagWidth: 64,
        };
        characters.set(agent.id, c);
      }
      c.agent = agent;
      c.tag.dataset.kind = agent.kind;
      c.tag.dataset.provenance = agent.provenance
        ? 'Verified' in agent.provenance
          ? 'verified'
          : 'inferred'
        : 'unknown';
      c.body.visible = visible;
      c.tag.hidden = !visible;
      c.leader.hidden = !visible;
      if (!target) {
        c.walk = null;
        continue;
      }
      const occupants = agents.filter((a) => a.state === agent.state);
      const rank = slots.get(agent.id) ?? 0;
      c.tagWidth = occupants.length === 1 ? 64 : 48;
      c.tag.style.width = `${String(c.tagWidth)}px`;
      c.tagTop =
        Math.max(...occupants.map((a) => geometry.slots.get(a.id)?.y ?? 0)) +
        2 +
        rank * 10;
      c.tag.textContent = `${agent.label} ${agent.provenance ? ('Verified' in agent.provenance ? '[V]' : '[I]') : '[?]'}`;
      c.tag.title = `${agent.label} · ${activityLabel(agent.state)}${agent.stale ? ' · Unknown (no events for 10 min)' : ''}`;
      c.tag.classList.toggle('stale', agent.stale);
      if (media?.matches) {
        c.position = target;
        c.walk = null;
      } else if (!same(c.target, target)) {
        const route = walkRoute(plannedPath(c.position, target, geometry));
        c.walk = route.done ? null : route;
      }
      c.target = target;
      place(c);
    }
    labels.style.width = `${String(lobbySize.width)}px`;
    labels.style.height = `${String(lobbySize.height)}px`;
    syncTicker();
    app.render();
  };
  const align = () => {
    const available = host.parentElement?.clientWidth ?? 320;
    const scale = Math.max(2, Math.floor(available / lobbySize.width));
    host.style.setProperty('--yard-scale', String(scale));
    host.style.width = `${String(lobbySize.width * scale)}px`;
    host.style.height = `${String(lobbySize.height * scale)}px`;
    host.style.marginLeft = `${String(Math.max(0, Math.floor((available - lobbySize.width * scale) / 2)))}px`;
    host.style.marginRight = '0';
    app.canvas.style.width = `${String(lobbySize.width * scale)}px`;
    app.canvas.style.height = `${String(lobbySize.height * scale)}px`;
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
      initialize: async () => {
        // Bundled PNGs need no format probes or blob workers under the production CSP.
        await Assets.init({
          skipDetections: true,
          preferences: { preferWorkers: false, preferCreateImageBitmap: false },
        });
        texture = await Assets.load<Texture>(lobbyUrl);
        monitor = await Assets.load<Texture>(monitorUrl);
        await app.init({
          width: lobbySize.width,
          height: lobbySize.height,
          resolution: 1,
          background: ink.deep,
          preference: 'webgl',
          antialias: false,
          roundPixels: true,
          autoStart: false,
          sharedTicker: false,
          manageImports: false,
        });
      },
      attach: () => {
        if (!texture) throw new Error('Lobby texture missing');
        environment.addChild(new Sprite(texture));
        app.canvas.setAttribute('aria-hidden', 'true');
        app.stage.addChild(environment, avatars);
        if (monitor) app.stage.addChild(new Sprite(monitor));
        host.append(app.canvas, labels);
        app.ticker.add(tick);
        media = window.matchMedia('(prefers-reduced-motion: reduce)');
        media.addEventListener('change', reducedMotion);
        poseTimer = setInterval(() => {
          if (media?.matches || document.visibilityState === 'hidden') return;
          restFrame = 1 - restFrame;
          for (const c of characters.values()) {
            if (!c.walk && !c.agent.stale) place(c);
          }
          app.render();
        }, 800);
        ready = true;
        reconcile();
        align();
        observer = new ResizeObserver(align);
        if (host.parentElement) observer.observe(host.parentElement);
        const layout = host.parentElement?.parentElement?.parentElement;
        if (layout) observer.observe(layout);
      },
      destroy: () => {
        ready = false;
        observer?.disconnect();
        clearInterval(poseTimer);
        media?.removeEventListener('change', reducedMotion);
        app.ticker.stop();
        app.ticker.remove(tick);
        characters.clear();
        reservations.clear();
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
