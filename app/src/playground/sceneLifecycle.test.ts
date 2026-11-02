import { describe, expect, it, vi } from 'vitest';
import { initializeScene } from './sceneLifecycle';

function deferred() {
  let resolve = () => {
    /* replaced by the Promise executor */
  };
  const promise = new Promise<void>((complete) => {
    resolve = complete;
  });
  return { promise, resolve };
}

describe('scene lifecycle', () => {
  it('destroys a renderer initialized after unmount without attaching it', async () => {
    const init = deferred();
    const scene = {
      initialize: () => init.promise,
      attach: vi.fn(),
      destroy: vi.fn(),
    };
    const onError = vi.fn();
    const dispose = initializeScene(scene, onError);
    dispose();
    expect(scene.destroy).not.toHaveBeenCalled();
    init.resolve();
    await init.promise;
    expect(scene.attach).not.toHaveBeenCalled();
    expect(scene.destroy).toHaveBeenCalledOnce();
    dispose();
    expect(scene.destroy).toHaveBeenCalledOnce();
    expect(onError).not.toHaveBeenCalled();
  });

  it('attaches after initialization and destroys exactly once on unmount', async () => {
    const init = deferred();
    const scene = {
      initialize: () => init.promise,
      attach: vi.fn(),
      destroy: vi.fn(),
    };
    const dispose = initializeScene(scene, vi.fn());
    init.resolve();
    await init.promise;
    expect(scene.attach).toHaveBeenCalledOnce();
    dispose();
    dispose();
    expect(scene.destroy).toHaveBeenCalledOnce();
  });

  it('reports unavailable graphics without destroying an uninitialized renderer', async () => {
    const scene = {
      initialize: () => Promise.reject(new Error('unavailable')),
      attach: vi.fn(),
      destroy: vi.fn(),
    };
    const onError = vi.fn();
    initializeScene(scene, onError);
    await vi.waitFor(() => {
      expect(onError).toHaveBeenCalledOnce();
    });
    expect(scene.attach).not.toHaveBeenCalled();
    expect(scene.destroy).not.toHaveBeenCalled();
  });
});
