import { useEffect, useRef, useState } from 'react';
import { mountPlaceholder } from './playground/placeholder';

export function App() {
  const host = useRef<HTMLDivElement>(null);
  const [rendererFailed, setRendererFailed] = useState(false);

  useEffect(() => {
    if (!host.current) return;
    return mountPlaceholder(host.current, () => {
      setRendererFailed(true);
    });
  }, []);

  return (
    <main>
      <header>
        <p className="eyebrow">Desktop scaffold</p>
        <h1>Meshyard</h1>
        <p>A local space for your agents. The workspace is taking shape.</p>
      </header>
      <figure aria-labelledby="placeholder-description">
        <div className="canvas-host" ref={host} aria-hidden="true" />
        <figcaption id="placeholder-description">
          Playground placeholder: one teal rectangle on a dark background.
        </figcaption>
      </figure>
      {rendererFailed && (
        <p role="status">
          The graphics preview is unavailable. The placeholder is described
          above.
        </p>
      )}
      <p className="footnote">
        Scaffold only. No agents or peers are connected.
      </p>
    </main>
  );
}
