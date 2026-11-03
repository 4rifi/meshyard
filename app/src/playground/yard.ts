// Static no-eval Pixi polyfills preserve the unchanged production CSP.
import 'pixi.js/unsafe-eval';
import { Application, Graphics, TextureStyle } from 'pixi.js';
import type { AgentView } from '../ipc/generated/AgentView';
import { initializeScene } from './sceneLifecycle';
import { activityLabel, yardPlots } from './yardData';
import { ink, pixelSprite } from './sprites';

TextureStyle.defaultOptions.scaleMode = 'nearest';

export function mountYard(
  host: HTMLDivElement,
  agents: AgentView[],
  onError: () => void,
): () => void {
  const app = new Application();
  const plots = yardPlots(agents);
  const height = plots.length * 176;
  let observer: ResizeObserver | undefined;
  const labels = document.createElement('div');
  labels.className = 'yard-labels';
  const tag = (
    text: string,
    x: number,
    y: number,
    className: string,
    agent?: AgentView,
    width?: number,
  ) => {
    const label = document.createElement('span');
    label.className = className;
    label.textContent = text;
    label.style.left = `${String(x)}px`;
    label.style.top = `${String(y)}px`;
    if (width) label.style.width = `${String(width)}px`;
    if (agent) {
      label.title = `${agent.label} · ${activityLabel(agent.state)}`;
      label.dataset.kind = agent.kind;
    }
    labels.appendChild(label);
  };
  return initializeScene(
    {
      initialize: () =>
        app.init({
          width: 320,
          height,
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
        const ground = new Graphics();
        plots.forEach((plot, page) => {
          const offset = page * 176;
          for (let y = 0; y < 176; y += 16) {
            for (let x = 0; x < 320; x += 16) {
              ground.rect(x, offset + y, 16, 16).fill(ink.grass);
              ground.rect(x + 2, offset + y + 3, 2, 1).fill(ink.grassDark);
              ground.rect(x + 10, offset + y + 11, 1, 2).fill(ink.grassDark);
            }
          }
          // A continuous tile path connects all eight original station props.
          ground.rect(0, offset + 80, 320, 16).fill(ink.wood);
          ground.rect(0, offset + 82, 320, 12).fill(ink.path);
          for (let x = 0; x < 320; x += 16) {
            ground.rect(x, offset + 84, 2, 2).fill(ink.wood);
            ground.rect(x + 10, offset + 90, 2, 1).fill(ink.wood);
          }
          plot.stations.forEach(({ x, y }) => {
            ground.rect(x + 32, offset + y + 32, 16, 48).fill(ink.path);
            ground.rect(x + 4, offset + y + 24, 72, 40).fill(ink.grassDark);
            ground.rect(x + 6, offset + y + 26, 68, 36).fill(ink.grass);
          });
        });
        app.stage.addChild(ground);
        plots.forEach((plot, page) => {
          const offset = page * 176;
          app.stage.addChild(
            pixelSprite('tree', 0, offset + 80),
            pixelSprite('tree', 304, offset + 80),
          );
          plot.stations.forEach((station) => {
            if (station.prop)
              app.stage.addChild(
                pixelSprite(
                  station.prop,
                  station.x + 32,
                  offset + station.y + 28,
                ),
              );
            tag(
              station.label,
              station.x + 2,
              offset + station.y + 8,
              'station-label',
            );
            station.agents.forEach((agent, index) => {
              const single = station.agents.length === 1;
              const x = station.x + 2 + index * 40;
              const y = offset + station.y + 46;
              app.stage.addChild(
                pixelSprite('agent', x + (single ? 30 : 10), y, agent.kind),
              );
              tag(agent.label, x, y + 16, 'agent-tag', agent, single ? 76 : 36);
            });
          });
        });
        // No canvas text/font rasterization or asset loading; labels are solid HTML tags.
        app.canvas.setAttribute('aria-hidden', 'true');
        host.append(app.canvas, labels);
        const resize = () => {
          const scale = Math.max(
            1,
            Math.floor((host.parentElement?.clientWidth ?? 320) / 320),
          );
          host.style.setProperty('--yard-scale', String(scale));
          host.style.height = `${String(height * scale)}px`;
          app.canvas.style.width = `${String(320 * scale)}px`;
          app.canvas.style.height = `${String(height * scale)}px`;
          labels.style.width = '320px';
          labels.style.height = `${String(height)}px`;
          labels.style.transformOrigin = 'top left';
          labels.style.transform = `scale(${String(scale)})`;
        };
        observer = new ResizeObserver(resize);
        if (host.parentElement) observer.observe(host.parentElement);
        resize();
        app.render();
      },
      destroy: () => {
        observer?.disconnect();
        labels.remove();
        app.destroy(true, { children: true });
      },
    },
    onError,
  );
}
