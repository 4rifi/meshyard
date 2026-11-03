// Install static no-eval Pixi polyfills under the unchanged production CSP.
import 'pixi.js/unsafe-eval';
import { Application, Graphics, Text } from 'pixi.js';
import type { AgentView } from '../ipc/generated/AgentView';
import { initializeScene } from './sceneLifecycle';
import { yardZones } from './yardData';

export function mountYard(
  host: HTMLDivElement,
  agents: AgentView[],
  onError: () => void,
): () => void {
  const app = new Application();
  const zones = yardZones(agents);
  const height = Math.max(
    200,
    zones.reduce(
      (sum, zone) => sum + 64 + Math.ceil(zone.agents.length / 3) * 84,
      16,
    ),
  );
  return initializeScene(
    {
      initialize: () =>
        app.init({
          width: 720,
          height,
          background: '#111c25',
          preference: 'webgl',
          autoStart: false,
          sharedTicker: false,
          manageImports: false,
        }),
      attach: () => {
        let top = 20;
        for (const zone of zones) {
          const title = new Text({
            text: zone.label,
            style: { fill: '#b9c9d0', fontSize: 18 },
          });
          title.position.set(24, top);
          app.stage.addChild(title);
          zone.agents.forEach((agent, index) => {
            const x = 24 + (index % 3) * 232;
            const y = top + 34 + Math.floor(index / 3) * 84;
            const token = new Graphics()
              .roundRect(x, y, 216, 64, 12)
              .fill('#203d46')
              .stroke({ width: 1, color: '#5ee4c3' });
            const label = new Text({
              text: agent.label,
              style: {
                fill: '#edf6f4',
                fontSize: 16,
                wordWrap: true,
                wordWrapWidth: 190,
                breakWords: true,
              },
            });
            label.position.set(x + 12, y + 10);
            label.scale.set(Math.min(1, 44 / Math.max(1, label.height)));
            app.stage.addChild(token, label);
          });
          top += 64 + Math.ceil(zone.agents.length / 3) * 84;
        }
        app.canvas.setAttribute('aria-hidden', 'true');
        host.appendChild(app.canvas);
        app.render(); // draw once; no animation, including reduced-motion profiles
      },
      destroy: () => {
        app.destroy(true, { children: true });
      },
    },
    onError,
  );
}
