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
      label.dataset.agentId = agent.id;
    }
    labels.appendChild(label);
  };
  return initializeScene(
    {
      initialize: () =>
        app.init({
          width: 320,
          height: 192,
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
        // Rebuild logical positions at the breakpoint, not just the canvas CSS size.
        const draw = (columns: 2 | 4) => {
          for (const child of app.stage.removeChildren()) child.destroy();
          labels.replaceChildren();
          const plots = yardPlots(agents, columns);
          const width = columns * 80;
          const plotHeight = (8 / columns) * 96;
          const height = plots.length * plotHeight;
          app.renderer.resize(width, height);
          const ground = new Graphics();
          plots.forEach((plot, page) => {
            const offset = page * plotHeight;
            for (let y = 0; y < plotHeight; y += 16) {
              for (let x = 0; x < width; x += 16) {
                ground.rect(x, offset + y, 16, 16).fill(ink.grass);
                ground.rect(x + 2, offset + y + 3, 2, 1).fill(ink.grassDark);
                ground.rect(x + 10, offset + y + 11, 1, 2).fill(ink.grassDark);
              }
            }
            // Every station row joins a continuous path through the real-data plots.
            ground.rect(0, offset + 80, 16, plotHeight - 80).fill(ink.path);
            for (let y = 80; y < plotHeight; y += 96) {
              ground.rect(0, offset + y, width, 16).fill(ink.wood);
              ground.rect(0, offset + y + 2, width, 12).fill(ink.path);
              for (let x = 0; x < width; x += 16) {
                ground.rect(x, offset + y + 4, 2, 2).fill(ink.wood);
                ground.rect(x + 10, offset + y + 10, 2, 1).fill(ink.wood);
              }
            }
            plot.stations.forEach(({ x, y }) => {
              ground.rect(x + 32, offset + y + 32, 16, 48).fill(ink.path);
              ground.rect(x + 4, offset + y + 24, 72, 40).fill(ink.grassDark);
              ground.rect(x + 6, offset + y + 26, 68, 36).fill(ink.grass);
            });
          });
          app.stage.addChild(ground);
          plots.forEach((plot, page) => {
            const offset = page * plotHeight;
            app.stage.addChild(
              pixelSprite('tree', 0, offset + 80),
              pixelSprite('tree', width - 16, offset + 80),
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
                tag(
                  agent.label,
                  x,
                  y + 16,
                  'agent-tag',
                  agent,
                  single ? 76 : 36,
                );
              });
            });
          });
          labels.style.width = `${String(width)}px`;
          labels.style.height = `${String(height)}px`;
          app.render();
          return { width, height };
        };
        // No canvas text/font rasterization or asset loading; labels are solid HTML tags.
        app.canvas.setAttribute('aria-hidden', 'true');
        host.append(app.canvas, labels);
        let previousColumns: 2 | 4 | undefined;
        let dimensions = { width: 320, height: 192 };
        const resize = () => {
          const available = host.parentElement?.clientWidth ?? 320;
          const columns = available >= 640 ? 4 : 2;
          if (columns !== previousColumns) {
            dimensions = draw(columns);
            previousColumns = columns;
          }
          const scale = Math.max(1, Math.floor(available / dimensions.width));
          host.style.setProperty('--yard-scale', String(scale));
          host.style.width = `${String(dimensions.width * scale)}px`;
          host.style.marginLeft = `${String(Math.max(0, Math.floor((available - dimensions.width * scale) / 2)))}px`;
          host.style.marginRight = '0';
          host.style.height = `${String(dimensions.height * scale)}px`;
          app.canvas.style.width = `${String(dimensions.width * scale)}px`;
          app.canvas.style.height = `${String(dimensions.height * scale)}px`;
          labels.style.transformOrigin = 'top left';
          labels.style.transform = `scale(${String(scale)})`;
        };
        resize();
        observer = new ResizeObserver(resize);
        if (host.parentElement) observer.observe(host.parentElement);
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
