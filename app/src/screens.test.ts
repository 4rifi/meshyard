import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { StatusScreen } from './App';
import type { AppStatus } from './ipc/generated/AppStatus';
import type { AppError } from './ipc/generated/AppError';
import type { WorkspaceView } from './ipc/generated/WorkspaceView';
import { yardPlots, yardZones } from './playground/yardData';

// SSR verifies text/form semantics; it cannot initialize or prove native/WebGL rendering.
vi.mock('./playground/yard', () => ({ mountYard: vi.fn() }));
const actions = {
  create: vi.fn(),
  profile: vi.fn(),
  reload: vi.fn(),
  register: vi.fn<() => Promise<void>>(),
};
const workspace: WorkspaceView = {
  id_short: '1234abcd',
  name: 'Demo <script>',
  mode: 'Strict',
  device_short: 'abcd1234',
  members: [
    {
      device_short: 'abcd1234',
      display_name: 'Dion',
      role: 'Owner',
      is_you: true,
    },
  ],
  agents: [],
};
function render(status: AppStatus, busy = false) {
  return renderToStaticMarkup(
    createElement(StatusScreen, { status, busy, actions }),
  );
}
const errors: Record<AppError, true> = {
  IdentityMissing: true,
  IdentityLocked: true,
  IdentityUnavailable: true,
  IdentityInvalid: true,
  IdentityMismatch: true,
  DataDirectoryBusy: true,
  UnsafePermissions: true,
  Storage: true,
  Migration: true,
  ProjectionLimit: true,
  MultipleWorkspaces: true,
  WorkspaceExists: true,
  NoWorkspace: true,
  InvalidInput: true,
  RandomUnavailable: true,
  PartialCreate: true,
  Configuration: true,
  IpcUnavailable: true,
};
describe('real-state screen contract', () => {
  it('offers labeled setup with Strict and disabled Auto', () => {
    const html = render('Setup');
    expect(html).toContain('Create workspace');
    expect(html).toContain('for="workspace-name"');
    expect(html).toContain('for="display-name"');
    expect(html).toContain('<option value="Auto" disabled=""');
    expect(html).toContain('Requires a self-hosted relay (G-1)');
    expect(html).toContain('tabindex="-1"');
    expect(render('Setup', true)).toContain('<fieldset disabled=""');
  });
  it('renders persisted workspace and honest empty states with escaped labels', () => {
    const html = render({ Ready: workspace });
    expect(html).toContain('Demo &lt;script&gt;');
    expect(html).toContain('Dion');
    expect(html).toContain('Owner · You');
    expect(html).toContain('Strict P2P');
    expect(html).toContain('Register an agent to see it here.');
    expect(html).toContain('No peers. Networking is not built yet.');
    expect(html).not.toContain('placeholder');
    expect(html).not.toContain('T-007');
    expect(html).toContain('href="#agent-label"');
    expect(html.match(/id="agent-label"/g)).toHaveLength(1);
    expect(html.match(/<button[^>]*>Register agent<\/button>/g)).toHaveLength(
      1,
    );
  });
  it('keeps registration separate from activity/provenance in both list and yard data', () => {
    const agents = [
      {
        id: 'agent1',
        label: 'main',
        kind: 'ClaudeCode' as const,
        state: null,
        provenance: null,
      },
    ];
    const html = render({ Ready: { ...workspace, agents } });
    expect(html).toContain('main');
    expect(html).toContain('Claude Code');
    expect(html).toContain('No events yet');
    expect(html).not.toContain('Verified');
    expect(html).not.toContain('Idle');
    expect(html).toContain('Live agent status is not connected yet.');
    expect(yardZones(agents)).toEqual([{ label: 'No events yet', agents }]);
  });
  it('places actual activity in its real zone and shows only supplied provenance', () => {
    const agent = {
      id: 'agent2',
      label: 'worker',
      kind: 'Codex' as const,
      state: 'Testing' as const,
      provenance: { Inferred: { rule: 'ToolShell' as const } },
    };
    expect(yardZones([agent])[0]?.label).toBe('Testing');
    const html = render({ Ready: { ...workspace, agents: [agent] } });
    expect(html).toContain('Testing');
    expect(html).toContain('Inferred · Shell tool');
    expect(html).not.toContain('Verified');
    const verified = render({
      Ready: {
        ...workspace,
        agents: [
          {
            ...agent,
            state: 'Finished',
            provenance: { Verified: { source: 'Stop' } },
          },
        ],
      },
    });
    expect(verified).toContain('Verified · Stop event');
    expect(verified).not.toContain('Inferred');
  });
  it.each([2, 4] as const)(
    'keeps each real agent on a distinct tile when a station is crowded (%i columns)',
    (columns) => {
      const agents = Array.from({ length: 5 }, (_, index) => ({
        id: String(index),
        label: 'same label',
        kind: 'ClaudeCode' as const,
        state: null,
        provenance: null,
      }));
      const plots = yardPlots(agents, columns);
      expect(plots).toHaveLength(3);
      expect(
        plots.flatMap((plot) =>
          plot.stations.flatMap((station) => station.agents),
        ),
      ).toEqual(agents);
      expect(
        plots.every((plot) =>
          plot.stations.every((station) => station.agents.length <= 2),
        ),
      ).toBe(true);
      const positions = plots.flatMap((plot, page) =>
        plot.stations.flatMap((station) =>
          station.agents.map((agent, index) => ({
            id: agent.id,
            x:
              station.x +
              2 +
              index * 40 +
              (station.agents.length === 1 ? 30 : 10),
            y: page * (8 / columns) * 96 + station.y + 46,
          })),
        ),
      );
      expect(new Set(positions.map(({ id }) => id)).size).toBe(agents.length);
      for (const [index, position] of positions.entries()) {
        expect(position.x).toBeGreaterThanOrEqual(0);
        expect(position.x + 16).toBeLessThanOrEqual(columns * 80);
        for (const other of positions.slice(index + 1)) {
          expect(
            Math.abs(position.x - other.x) >= 16 ||
              Math.abs(position.y - other.y) >= 16,
          ).toBe(true);
        }
      }
      const future = yardPlots(
        [
          {
            id: 'future',
            label: 'future DTO state',
            kind: 'Codex',
            state: 'Reviewing',
            provenance: null,
          },
        ],
        columns,
      );
      expect(
        future
          .flatMap((plot) => plot.stations)
          .filter((station) => station.agents.length),
      ).toMatchObject([{ label: 'Reviewing', prop: null }]);
    },
  );
  it.each([2, 4] as const)(
    'reflows all eight stations in order within %i columns',
    (columns) => {
      const plot = yardPlots([], columns)[0];
      expect(plot?.stations.map(({ label }) => label)).toEqual([
        'Arrival',
        'Thinking',
        'Building',
        'Testing',
        'Idle',
        'Merging',
        'Finished',
        'Error',
      ]);
      expect(plot?.stations.map(({ x, y }) => [x, y])).toEqual(
        columns === 2
          ? [
              [0, 0],
              [80, 0],
              [0, 96],
              [80, 96],
              [0, 192],
              [80, 192],
              [0, 288],
              [80, 288],
            ]
          : [
              [0, 0],
              [80, 0],
              [160, 0],
              [240, 0],
              [0, 96],
              [80, 96],
              [160, 96],
              [240, 96],
            ],
      );
    },
  );
  for (const error of Object.keys(errors) as AppError[]) {
    it(`renders actionable typed ${error} without OS details`, () => {
      const html = render({ Error: error });
      expect(html).toContain(error);
      expect(html).toContain('role="alert"');
      expect(html).toContain('Reload saved state');
      expect(html).not.toContain('/Volumes/');
      expect(html).not.toContain('os error');
      expect(html).toContain('tabindex="-1"');
      if (error === 'PartialCreate')
        expect(html).toContain('for="repair-name"');
      if (error === 'IdentityMissing')
        expect(html).toContain(
          'This workspace’s device identity is missing; restore it or use the matching key store.',
        );
    });
  }
});
