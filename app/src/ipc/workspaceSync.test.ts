import { afterEach, describe, expect, it, vi } from 'vitest';
import { workspaceSync } from './workspaceSync';
import type { AppStatus } from './generated/AppStatus';
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((yes) => {
    resolve = yes;
  });
  return { promise, resolve };
}
const flush = async () => {
  await Promise.resolve();
  await Promise.resolve();
  await Promise.resolve();
};
afterEach(() => {
  vi.useRealTimers();
});
describe('workspace refresh lifetime and ordering', () => {
  it('cleans a listener that resolves after unmount and ignores old StrictMode reads', async () => {
    const setup = deferred<() => void>();
    const read = deferred<AppStatus>();
    const stop = vi.fn();
    const accept = vi.fn();
    const old = workspaceSync({
      read: () => read.promise,
      listen: () => setup.promise,
      accept,
    });
    old.dispose();
    setup.resolve(stop);
    read.resolve('Setup');
    await flush();
    expect(stop).toHaveBeenCalledOnce();
    expect(accept).not.toHaveBeenCalled();
    const current = workspaceSync({
      read: () => Promise.resolve('Setup' as AppStatus),
      listen: () => Promise.resolve(stop),
      accept,
    });
    await flush();
    expect(accept).toHaveBeenLastCalledWith('Setup');
    current.dispose();
    expect(stop).toHaveBeenCalledTimes(2);
  });
  it('coalesces bursts and does not let a status read overwrite a newer action', async () => {
    const initial = deferred<AppStatus>();
    const action = deferred<AppStatus>();
    const read = vi
      .fn<() => Promise<AppStatus>>()
      .mockReturnValueOnce(initial.promise)
      .mockResolvedValue({ Error: 'IdentityLocked' });
    const accept = vi.fn();
    const sync = workspaceSync({
      read,
      listen: () => new Promise(() => {}),
      accept,
    });
    const done = sync.action(() => action.promise);
    for (let i = 0; i < 20; i += 1) sync.refresh();
    action.resolve('Setup');
    await done;
    initial.resolve({ Error: 'Storage' });
    await flush();
    await flush();
    expect(accept.mock.calls).toEqual([
      ['Setup'],
      [{ Error: 'IdentityLocked' }],
    ]);
    expect(read).toHaveBeenCalledTimes(2);
    sync.dispose();
  });
  it('refreshes derived metadata without any append and stops polling on disposal', async () => {
    vi.useFakeTimers();
    const read = vi.fn<() => Promise<AppStatus>>().mockResolvedValue('Setup');
    const sync = workspaceSync({
      read,
      listen: () => new Promise(() => {}),
      accept: vi.fn(),
    });
    await flush();
    const before = read.mock.calls.length;
    await vi.advanceTimersByTimeAsync(5000);
    expect(read).toHaveBeenCalledTimes(before + 1);
    sync.dispose();
    await vi.advanceTimersByTimeAsync(600000);
    expect(read).toHaveBeenCalledTimes(before + 1);
  });
  it('keeps typed connection errors local and serializes conflicting actions', async () => {
    const action = deferred<AppStatus>();
    const accept = vi.fn();
    const sync = workspaceSync({
      read: () => Promise.resolve('Setup' as AppStatus),
      listen: () => new Promise(() => {}),
      accept,
    });
    await flush();
    accept.mockClear();
    const first = sync.action(() => action.promise, true);
    const second = vi.fn<() => Promise<AppStatus>>();
    expect(await sync.action(second)).toBeUndefined();
    expect(second).not.toHaveBeenCalled();
    action.resolve({ Error: 'HookPlanStale' });
    expect(await first).toEqual({ Error: 'HookPlanStale' });
    expect(accept).not.toHaveBeenCalled();
    sync.dispose();
  });
});
