import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { ConnectClaudeCode } from './features/connect/ConnectClaudeCode';
import { StatusScreen } from './App';
import type { AppStatus } from './ipc/generated/AppStatus';
import type { AppError } from './ipc/generated/AppError';
import type { WorkspaceView } from './ipc/generated/WorkspaceView';
import { yardPlots, yardZones } from './playground/yardData';

// SSR verifies text/form semantics; it cannot initialize or prove native/WebGL rendering.
vi.mock('./playground/yard', () => ({ createYard: vi.fn() }));
const actions = {
  bind: vi.fn(),
  apply: vi.fn(),
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
  ingest: { status: 'Disabled', dropped_invalid: 0, dropped_unbound: 0 },
  codex_hook: { installed: false, binary_available: false },
  hook: { installed: false, binary_available: false },
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
  IngestUnavailable: true,
  HookBinaryMissing: true,
  HookInstallLocation: true,
  HookPlanStale: true,
  HookConfigInvalid: true,
};
describe('real-state screen contract', () => {
  it('offers two labeled setup fields without a misleading network choice', () => {
    const html = render('Setup');
    expect(html).toContain('Create workspace');
    expect(html).toContain('for="workspace-name"');
    expect(html).toContain('for="display-name"');
    expect(html).not.toContain('Connection mode');
    expect(html).not.toContain('<select');
    expect(html).toContain('Your yard stays on this Mac.');
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
        is_you: true,
        stale: false,
        bound: false,
        connection: 'NotConnected' as const,
      },
    ];
    const html = render({ Ready: { ...workspace, agents } });
    expect(html).toContain('main');
    expect(html).toContain('Claude Code');
    expect(html).toContain('No events yet');
    expect(html).not.toContain('Verified');
    expect(html).not.toContain('Idle');
    expect(html).toContain('<option value="Codex">Codex</option>');
    expect(yardZones(agents)).toEqual([{ label: 'No events yet', agents }]);
  });
  it('places actual activity in its real zone and shows only supplied provenance', () => {
    const agent = {
      id: 'agent2',
      label: 'worker',
      kind: 'Codex' as const,
      state: 'Testing' as const,
      provenance: { Inferred: { rule: 'ToolShell' as const } },
      is_you: true,
      stale: false,
      bound: false,
      connection: 'NotConnected' as const,
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
        is_you: true,
        stale: false,
        bound: false,
        connection: 'NotConnected' as const,
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
            is_you: true,
            stale: false,
            bound: false,
            connection: 'NotConnected' as const,
          },
        ],
        columns,
      );
      expect(
        future
          .flatMap((plot) => plot.stations)
          .filter((station) => station.agents.length),
      ).toMatchObject([{ label: 'Review', prop: 'review', reserved: true }]);
    },
  );
  it.each([2, 4] as const)(
    'reflows all eleven true and reserved stations within %i columns',
    (columns) => {
      const plot = yardPlots([], columns)[0];
      expect(plot?.stations.map(({ label }) => label)).toEqual([
        'Arrival',
        'Thinking',
        'Build',
        'Test',
        'Idle',
        'Merge',
        'Finished',
        'Error',
        'Review',
        'Fix',
        'Waiting',
      ]);
      expect(plot?.stations.map(({ x, y }) => [x, y])).toEqual(
        Array.from({ length: 11 }, (_, index) => [
          (index % columns) * 80,
          Math.floor(index / columns) * 64,
        ]),
      );
    },
  );
  it('limits the connection selector to backend-owned Claude agents and shows typed connection truth', () => {
    const own = {
      id: 'own-full-id',
      label: 'own worker',
      kind: 'ClaudeCode' as const,
      state: 'Thinking' as const,
      provenance: null,
      is_you: true,
      bound: true,
      stale: true,
      connection: 'Stale' as const,
    };
    const view = {
      ...workspace,
      agents: [
        own,
        { ...own, id: 'other-full-id', label: 'foreign worker', is_you: false },
        { ...own, id: 'codex', label: 'Codex worker', kind: 'Codex' as const },
      ],
    };
    const html = renderToStaticMarkup(
      createElement(ConnectClaudeCode, {
        workspace: view,
        busy: true,
        onBind: actions.bind,
        onApply: actions.apply,
        onRegister: () => {},
        onClose: () => {},
      }),
    );
    expect(html).toContain('own-full-id');
    expect(html).not.toContain('foreign worker');
    expect(html).not.toContain('Codex worker');
    expect(html).toContain('Unknown (no events for 10 min)');
    expect(html).toContain('<option value="Codex">Codex</option>');
    expect(html).toContain('disabled=""');
    const codexHtml = renderToStaticMarkup(
      createElement(ConnectClaudeCode, {
        workspace: view,
        initialKind: 'Codex',
        busy: false,
        onBind: actions.bind,
        onApply: actions.apply,
        onRegister: () => {},
        onClose: () => {},
      }),
    );
    expect(codexHtml).toContain('Codex worker');
    expect(codexHtml).not.toContain('own worker');
    expect(codexHtml).not.toContain('foreign worker');
    expect(codexHtml).toContain('/hooks');
    expect(codexHtml).toContain('review and trust');
    const list = render({ Ready: view });
    expect(list).toContain('Last: Thinking');
    expect(list).toContain('stale-state');
  });
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
