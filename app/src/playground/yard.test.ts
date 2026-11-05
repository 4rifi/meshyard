import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { AgentView } from '../ipc/generated/AgentView';

class Element {
  children: Element[] = [];
  parentElement: Element | null = null;
  clientWidth = 700;
  textContent = '';
  title = '';
  className = '';
  classList = { toggle: vi.fn() };
  dataset: Record<string, string> = {};
  style: Record<string, string | ((name: string, value: string) => void)> = {
    setProperty: (name: string, value: string) => {
      this.style[name] = value;
    },
  };
  append(...children: Element[]) {
    children.forEach((child) => {
      this.appendChild(child);
    });
  }
  appendChild(child: Element) {
    this.children.push(child);
    child.parentElement = this;
    return child;
  }
  replaceChildren() {
    this.children = [];
  }
  remove() {
    if (this.parentElement)
      this.parentElement.children = this.parentElement.children.filter(
        (child) => child !== this,
      );
  }
  setAttribute = vi.fn();
  getBoundingClientRect() {
    return { x: 0.5, y: 0.25 };
  }
}
const mocks = vi.hoisted(() => ({
  apps: [] as MockApp[],
  initialize: () => Promise.resolve(),
}));
class Display {
  eventMode = 'auto';
  cursor = 'auto';
  handlers = new Map<string, () => void>();
  on(event: string, handler: () => void) {
    this.handlers.set(event, handler);
    return this;
  }
  children: Display[] = [];
  parent: Display | null = null;
  visible = true;
  position = {
    x: 0,
    y: 0,
    set: (x: number, y: number) => {
      this.position.x = x;
      this.position.y = y;
    },
  };
  addChild(...children: Display[]) {
    for (const child of children) {
      child.parent = this;
      this.children.push(child);
    }
  }
  removeChildren() {
    const children = this.children;
    this.children = [];
    return children;
  }
  destroy = vi.fn(() => {
    if (this.parent)
      this.parent.children = this.parent.children.filter(
        (child) => child !== this,
      );
  });
  rect() {
    return this;
  }
  fill() {
    return this;
  }
}
type MockTicker = {
  started: boolean;
  deltaMS: number;
  listener: null | ((ticker: MockTicker) => void);
  start: ReturnType<typeof vi.fn>;
  stop: ReturnType<typeof vi.fn>;
  add: ReturnType<typeof vi.fn>;
  remove: ReturnType<typeof vi.fn>;
};
class MockApp {
  stage = new Display();
  canvas = new Element();
  renderer = { resize: vi.fn() };
  render = vi.fn();
  destroy = vi.fn(() => {
    this.canvas.remove();
  });
  ticker: MockTicker = {
    started: false,
    deltaMS: 0,
    listener: null as null | ((ticker: MockTicker) => void),
    start: vi.fn(() => {
      this.ticker.started = true;
    }),
    stop: vi.fn(() => {
      this.ticker.started = false;
    }),
    add: vi.fn((listener: (ticker: MockTicker) => void) => {
      this.ticker.listener = listener;
    }),
    remove: vi.fn(() => {
      this.ticker.listener = null;
    }),
  };
  init() {
    return mocks.initialize();
  }
  constructor() {
    mocks.apps.push(this);
  }
  advance(dt: number) {
    if (this.ticker.started) {
      this.ticker.deltaMS = dt * 1000;
      this.ticker.listener?.(this.ticker);
    }
  }
}
vi.mock('pixi.js/unsafe-eval', () => ({}));
vi.doMock('pixi.js', () => ({
  Application: MockApp,
  Container: Display,
  Graphics: Display,
  TextureStyle: { defaultOptions: {} },
}));
const { createYard } = await import('./yard');
const dto = (id: string, state: AgentView['state'] = null): AgentView => ({
  id,
  state,
  label: `${id} full name`,
  kind: 'ClaudeCode',
  provenance: null,
  is_you: true,
  stale: false,
  bound: false,
  connection: 'NotConnected' as const,
});
let media: {
  matches: boolean;
  addEventListener: ReturnType<
    typeof vi.fn<(event: string, handler: () => void) => void>
  >;
  removeEventListener: ReturnType<typeof vi.fn>;
};
let observer: {
  observe: ReturnType<typeof vi.fn>;
  disconnect: ReturnType<typeof vi.fn>;
};
let resize: () => void;
let viewport: Element;
let host: Element;
const tags = () =>
  host.children[1]?.children.filter((child) => child.dataset.agentId) ?? [];
const position = (id: string) => {
  const index = tags().findIndex((item) => item.dataset.agentId === id);
  const body = app().stage.children[1]?.children[index];
  return [body?.position.x, body?.position.y];
};
const app = () => {
  const current = mocks.apps.at(-1);
  if (!current) throw new Error('Missing fixture app');
  return current;
};
const flush = async () => {
  await Promise.resolve();
  await Promise.resolve();
};
beforeEach(() => {
  mocks.apps = [];
  mocks.initialize = () => Promise.resolve();
  media = {
    matches: false,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  };
  observer = { observe: vi.fn(), disconnect: vi.fn() };
  vi.stubGlobal('document', { createElement: () => new Element() });
  vi.stubGlobal('window', { matchMedia: () => media });
  vi.stubGlobal(
    'ResizeObserver',
    class {
      constructor(callback: () => void) {
        resize = callback;
      }
      observe = observer.observe;
      disconnect = observer.disconnect;
    },
  );
  viewport = new Element();
  host = new Element();
  viewport.append(host);
});
afterEach(() => {
  vi.unstubAllGlobals();
});
const mount = () => createYard(host as unknown as HTMLDivElement, vi.fn());

describe('persistent yard scene', () => {
  it('opens the inspector for the clicked real avatar identity', async () => {
    const select = vi.fn();
    const yard = createYard(host as unknown as HTMLDivElement, vi.fn(), select);
    yard.update([dto('a'), dto('b')]);
    await flush();
    app().stage.children[1]?.children[1]?.handlers.get('pointertap')?.();
    expect(select).toHaveBeenCalledExactlyOnceWith('b');
    yard.destroy();
  });
  it('buffers updates before init; changes only one real identity and stops at arrival', async () => {
    const yard = mount();
    yard.update([dto('a'), dto('b')]);
    await flush();
    const aTag = tags()[0];
    const avatar = app().stage.children[1]?.children[0];
    const bPosition = position('b');
    expect(app().ticker.started).toBe(false);
    yard.update([dto('b'), dto('a', 'Thinking')]);
    expect(app().ticker.started).toBe(true);
    expect(tags()[0]).toBe(aTag);
    expect(app().stage.children[1]?.children[0]).toBe(avatar);
    app().advance(0.2);
    expect(position('a')).not.toEqual([12, 28]);
    expect(position('b')).toEqual(bPosition);
    expect(avatar?.children.map((pose) => pose.visible)).toEqual([
      false,
      true,
      false,
      false,
    ]);
    app().advance(3);
    expect(position('a')).toEqual([92, 28]);
    expect(position('b')).toEqual(bPosition);
    expect(app().ticker.started).toBe(false);
    expect(avatar?.children.map((pose) => pose.visible)).toEqual([
      true,
      false,
      false,
      false,
    ]);
    yard.destroy();
  });
  it('hydrates three distinct states into one local map without global Arrival overflow', async () => {
    const yard = mount();
    yard.update([
      dto('build', 'Building'),
      dto('test', 'Testing'),
      dto('done', 'Finished'),
    ]);
    await flush();
    expect(position('build')).toEqual([172, 28]);
    expect(position('test')).toEqual([252, 28]);
    expect(position('done')).toEqual([172, 92]);
    expect(host.style.height).toBe('384px');
    expect(host.children[1]?.children[0]?.children).toHaveLength(11);
    expect(app().ticker.started).toBe(false);
    yard.destroy();
  });
  it('retargets from the current integer position; label/provenance/order-only updates never restart it', async () => {
    const yard = mount();
    yard.update([dto('a'), dto('b')]);
    await flush();
    yard.update([dto('a', 'Testing'), dto('b')]);
    app().advance(0.6);
    const current = position('a');
    yard.update([
      dto('b'),
      {
        ...dto('a', 'Finished'),
        label: 'renamed',
        provenance: { Verified: { source: 'Stop' } },
      },
    ]);
    expect(position('a')).toEqual(current);
    app().advance(0.3);
    const next = position('a');
    yard.update([dto('a', 'Finished'), dto('b')]);
    expect(position('a')).toEqual(next);
    app().advance(3);
    expect(position('a')).toEqual([172, 92]);
    expect(position('b')).toEqual([52, 28]);
    expect(app().ticker.started).toBe(false);
    yard.destroy();
  });
  it('adds at Arrival, keeps stable holes/overflow slots, removes only the removed identity', async () => {
    const yard = mount();
    yard.update([dto('a'), dto('b'), dto('c'), dto('d'), dto('e')]);
    await flush();
    const positions = new Map(
      tags().map((tag) => [
        tag.dataset.agentId,
        position(tag.dataset.agentId ?? ''),
      ]),
    );
    const retained = tags().find((tag) => tag.dataset.agentId === 'e');
    yard.update([dto('e'), dto('c'), dto('b'), dto('d')]);
    for (const id of ['b', 'c', 'd', 'e'])
      expect(position(id)).toEqual(positions.get(id));
    expect(tags().find((tag) => tag.dataset.agentId === 'e')).toBe(retained);
    yard.update([dto('f', 'Thinking'), dto('e'), dto('c'), dto('b'), dto('d')]);
    expect(position('f')).toEqual([92, 28]);
    expect(position('a')).toEqual([undefined, undefined]);
    expect(tags()).toHaveLength(5);
    app().advance(3);
    expect(position('f')).toEqual([92, 28]);
    yard.destroy();
  });
  it('keeps neutral DTO identities stable across overflow growth and routes back from a vacated neutral plot', async () => {
    const yard = mount();
    yard.update([dto('neutral', 'Reviewing'), dto('a')]);
    await flush();
    app().advance(3);
    const neutralPosition = position('neutral');
    const neutralTag = tags().find((tag) => tag.dataset.agentId === 'neutral');
    yard.update([
      dto('neutral', 'Reviewing'),
      dto('a'),
      dto('b'),
      dto('c'),
      dto('d'),
      dto('e'),
    ]);
    expect(position('neutral')).toEqual(neutralPosition);
    expect(tags().find((tag) => tag.dataset.agentId === 'neutral')).toBe(
      neutralTag,
    );
    yard.update([dto('neutral', 'Testing'), dto('a')]);
    expect(position('neutral')).toEqual(neutralPosition);
    expect(app().ticker.started).toBe(true);
    app().advance(3);
    expect(position('neutral')).toEqual([252, 28]);
    expect(app().ticker.started).toBe(false);
    yard.update([]);
    expect(tags()).toHaveLength(0);
    expect(app().ticker.started).toBe(false);
    yard.destroy();
  });
  it('snaps reduced motion immediately and on preference change without any idle loop', async () => {
    media.matches = true;
    const yard = mount();
    yard.update([dto('a')]);
    await flush();
    yard.update([dto('a', 'Testing')]);
    expect(position('a')).toEqual([252, 28]);
    expect(app().ticker.started).toBe(false);
    media.matches = false;
    yard.update([dto('a', 'Finished')]);
    app().advance(0.2);
    media.matches = true;
    const handler = media.addEventListener.mock.calls[0]?.[1];
    if (typeof handler !== 'function') throw new Error('Missing listener');
    handler();
    expect(position('a')).toEqual([172, 92]);
    expect(app().ticker.started).toBe(false);
    yard.destroy();
  });
  it('retains avatar and label instances on responsive reflow and snaps to the same DTO state', async () => {
    const yard = mount();
    yard.update([dto('a', 'Finished')]);
    await flush();
    app().advance(0.2);
    const identity = tags()[0];
    const avatar = app().stage.children[1]?.children[0];
    viewport.clientWidth = 350;
    resize();
    expect(tags()[0]).toBe(identity);
    expect(app().stage.children[1]?.children[0]).toBe(avatar);
    expect(position('a')).toEqual([12, 220]);
    expect(app().ticker.started).toBe(false);
    expect(host.style.width).toBe('320px');
    expect(host.style.left).toBe('0.5px');
    expect(host.style.top).toBe('0.75px');
    yard.destroy();
  });
  it.each([700, 350])(
    'sizes tags per target station row without moving bodies or replacing identities at viewport %i',
    async (width) => {
      viewport.clientWidth = width;
      const yard = mount();
      yard.update([dto('a', 'Testing'), dto('b')]);
      await flush();
      const aTag = tags()[0];
      const bTag = tags()[1];
      const bodies = [...(app().stage.children[1]?.children ?? [])];
      expect(aTag?.style.width).toBe('76px');
      expect(bTag?.style.width).toBe('76px');
      app().advance(3);
      const aPosition = position('a');
      yard.update([dto('b', 'Testing'), dto('a', 'Testing')]);
      expect(position('a')).toEqual(aPosition);
      expect(aTag?.style.width).toBe('36px');
      expect(bTag?.style.width).toBe('36px');
      app().advance(3);
      const bPosition = position('b');
      const left = Number.parseInt(String(aTag?.style.left));
      const right = Number.parseInt(String(bTag?.style.left));
      expect(left + 36).toBeLessThanOrEqual(right);
      expect(bPosition[0]).toBe((aPosition[0] ?? 0) + 40);
      expect(bPosition[1]).toBe(aPosition[1]);
      // Vacated historical reservations do not make the current lone occupant narrow.
      yard.update([dto('b', 'Testing'), dto('a', 'Thinking')]);
      expect(position('a')).toEqual(aPosition);
      expect(position('b')).toEqual(bPosition);
      expect(bTag?.style.width).toBe('76px');
      expect(bTag?.style.left).toBe(`${String(left)}px`);
      app().advance(0.2);
      const moving = position('a');
      yard.update([dto('a', 'Thinking'), dto('b', 'Testing')]);
      expect(position('a')).toEqual(moving);
      expect(position('b')).toEqual(bPosition);
      app().advance(3);
      expect(tags()).toEqual([aTag, bTag]);
      expect(app().stage.children[1]?.children).toEqual(bodies);
      expect(bTag?.style.width).toBe('76px');
      expect(app().ticker.started).toBe(false);
      // Each overflow row has its own occupancy, rather than a global state count.
      yard.update([
        dto('a', 'Testing'),
        dto('b', 'Testing'),
        dto('c', 'Testing'),
      ]);
      expect(
        tags().find((tag) => tag.dataset.agentId === 'c')?.style.width,
      ).toBe('76px');
      expect(aTag?.style.width).toBe('36px');
      expect(bTag?.style.width).toBe('36px');
      app().advance(3);
      yard.update([dto('b', 'Testing')]);
      expect(position('b')).toEqual(bPosition);
      expect(tags()).toEqual([bTag]);
      expect(bTag?.style.width).toBe('76px');
      expect(bTag?.style.left).toBe(`${String(left)}px`);
      expect(app().ticker.started).toBe(false);
      yard.destroy();
    },
  );
  it('animates cosmetic idle and work poses without changing positions, and reduced motion disables them', async () => {
    vi.useFakeTimers();
    const yard = mount();
    try {
      yard.update([dto('idle', 'Idle'), dto('work', 'Building')]);
      await flush();
      app().advance(3);
      const positions = [position('idle'), position('work')];
      vi.advanceTimersByTime(800);
      const bodies = app().stage.children[1]?.children ?? [];
      expect(bodies[0]?.children.map((pose) => pose.visible)).toEqual([
        false,
        false,
        true,
        false,
      ]);
      expect(bodies[1]?.children.map((pose) => pose.visible)).toEqual([
        false,
        false,
        false,
        true,
      ]);
      expect([position('idle'), position('work')]).toEqual(positions);
      expect(app().ticker.started).toBe(false);
      media.matches = true;
      media.addEventListener.mock.calls[0]?.[1]();
      vi.advanceTimersByTime(1600);
      expect(bodies[1]?.children.map((pose) => pose.visible)).toEqual([
        true,
        false,
        false,
        false,
      ]);
      expect([position('idle'), position('work')]).toEqual(positions);
    } finally {
      yard.destroy();
      expect(vi.getTimerCount()).toBe(0);
      vi.useRealTimers();
    }
  });
  it('cleans ticker, observer, listener, canvas and tags once; ignores updates after destroy', async () => {
    const yard = mount();
    yard.update([dto('a', 'Testing')]);
    await flush();
    yard.destroy();
    yard.destroy();
    yard.update([dto('b')]);
    expect(app().destroy).toHaveBeenCalledOnce();
    expect(app().ticker.started).toBe(false);
    expect(app().ticker.listener).toBeNull();
    expect(observer.disconnect).toHaveBeenCalledOnce();
    expect(media.removeEventListener).toHaveBeenCalledOnce();
    expect(host.children).toEqual([]);
  });
  it('handles StrictMode destroy during init and attach failure without a leaked ticker', async () => {
    let resolve = () => {
      /* set by executor */
    };
    mocks.initialize = () =>
      new Promise<void>((complete) => {
        resolve = complete;
      });
    const first = mount();
    first.update([dto('a')]);
    first.destroy();
    resolve();
    await flush();
    expect(app().destroy).toHaveBeenCalledOnce();
    expect(host.children).toEqual([]);
    mocks.initialize = () => Promise.resolve();
    vi.stubGlobal(
      'ResizeObserver',
      class {
        constructor() {
          throw new Error('fixture attach failure');
        }
        observe() {}
      },
    );
    const error = vi.fn();
    const failed = createYard(host as unknown as HTMLDivElement, error);
    failed.update([dto('a', 'Testing')]);
    await flush();
    expect(error).toHaveBeenCalledOnce();
    expect(app().destroy).toHaveBeenCalledOnce();
    expect(app().ticker.started).toBe(false);
    expect(host.children).toEqual([]);
    failed.destroy();
    expect(app().destroy).toHaveBeenCalledOnce();
  });
});
