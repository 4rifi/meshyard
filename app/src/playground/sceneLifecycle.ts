interface Scene {
  initialize: () => Promise<void>;
  attach: () => void;
  destroy: () => void;
}

/** Async Pixi initialization may finish after React StrictMode has disposed a mount. */
export function initializeScene(scene: Scene, onError: () => void): () => void {
  let disposed = false;
  let initialized = false;
  let destroyed = false;
  const destroy = () => {
    if (destroyed) return;
    destroyed = true;
    scene.destroy();
  };

  void scene
    .initialize()
    .then(() => {
      initialized = true;
      if (disposed) {
        destroy();
        return;
      }
      scene.attach();
    })
    .catch(() => {
      // Pixi cannot destroy an Application before its renderer finishes initializing.
      if (initialized) destroy();
      if (!disposed) onError();
    });

  return () => {
    disposed = true;
    if (initialized) destroy();
  };
}
