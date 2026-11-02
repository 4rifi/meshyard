import { Application, Graphics } from 'pixi.js';
import { initializeScene } from './sceneLifecycle';

/** Own one renderer per mount; draw once, without an animation loop or remote assets. */
export function mountPlaceholder(
  host: HTMLDivElement,
  onError: () => void,
): () => void {
  const app = new Application();
  return initializeScene(
    {
      initialize: () =>
        app.init({
          width: 640,
          height: 320,
          background: '#111c25',
          preference: 'webgl',
          autoStart: false,
          sharedTicker: false,
          manageImports: false,
        }),
      attach: () => {
        const rectangle = new Graphics()
          .rect(264, 104, 112, 112)
          .fill('#5ee4c3');
        app.stage.addChild(rectangle);
        app.canvas.setAttribute('aria-hidden', 'true');
        host.appendChild(app.canvas);
        app.render();
      },
      destroy: () => {
        app.destroy(true, { children: true });
      },
    },
    onError,
  );
}
