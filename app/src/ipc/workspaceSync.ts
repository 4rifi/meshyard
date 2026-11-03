import type { AppStatus } from './generated/AppStatus';

type Stop = () => void;
type Source = {
  read: () => Promise<AppStatus>;
  listen: (refresh: () => void) => Promise<Stop>;
  accept: (value: AppStatus) => void;
};
// A refresh cannot overtake an action. Bursts coalesce to one trailing read; every
// completion checks its generation and lifetime, including late listener setup.
export function workspaceSync(source: Source) {
  const lifetime = { active: true };
  const isActive = () => lifetime.active;
  let generation = 0;
  let reading = false;
  let acting = false;
  let dirty = false;
  let unlisten: Stop | undefined;
  const refresh = () => {
    if (!lifetime.active) return;
    if (reading || acting) {
      dirty = true;
      return;
    }
    reading = true;
    dirty = false;
    const started = generation;
    void source
      .read()
      .then((value) => {
        if (lifetime.active && started === generation && !dirty)
          source.accept(value);
      })
      .finally(() => {
        reading = false;
        if (dirty && lifetime.active && !acting) refresh();
      });
  };
  void source
    .listen(refresh)
    .then((stop) => {
      if (lifetime.active) unlisten = stop;
      else stop();
      // Subscribe before the trailing read so setup cannot miss an append.
      refresh();
    })
    .catch(() => {
      /* bounded polling remains available */
    });
  const timer = setInterval(refresh, 5000);
  refresh();
  return {
    refresh,
    async action(action: () => Promise<AppStatus>, localError = false) {
      if (acting || !isActive()) return undefined;
      acting = true;
      generation += 1;
      try {
        const result = await action();
        if (
          isActive() &&
          !(localError && typeof result === 'object' && 'Error' in result)
        )
          source.accept(result);
        return isActive() ? result : undefined;
      } finally {
        acting = false;
        generation += 1;
        if (dirty && isActive()) refresh();
      }
    },
    dispose() {
      lifetime.active = false;
      clearInterval(timer);
      unlisten?.();
    },
  };
}
export type WorkspaceSync = ReturnType<typeof workspaceSync>;
