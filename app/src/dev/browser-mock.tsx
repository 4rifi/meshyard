// Evidence-only entry. Never imported by main.tsx or present in its production graph.
import { mockIPC } from '@tauri-apps/api/mocks';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from '../App';
import type { AppStatus } from '../ipc/generated/AppStatus';
import type { AppError } from '../ipc/generated/AppError';
import type { AgentView } from '../ipc/generated/AgentView';
import type { WorkspaceView } from '../ipc/generated/WorkspaceView';
import type { CreateWorkspace } from '../ipc/generated/CreateWorkspace';
import type { RegisterAgent } from '../ipc/generated/RegisterAgent';
import type { HookPlan } from '../ipc/generated/HookPlan';
import '../styles.css';

if (import.meta.env.MODE !== 'browser-mock-ipc')
  throw new Error('Use the dedicated browser-mock-ipc evidence build.');
const scenario = new URLSearchParams(location.search).get('scenario');
const agent = (
  id: string,
  label: string,
  kind: AgentView['kind'] = 'ClaudeCode',
): AgentView => ({
  id,
  label,
  kind,
  is_you: true,
  state: null,
  provenance: null,
  stale: false,
  bound: false,
  connection: 'NotConnected',
});
let workspace: WorkspaceView = {
  id_short: 'a1b2c3d4',
  name: 'The local yard',
  mode: 'Strict',
  device_short: 'd4c3b2a1',
  members: [
    {
      device_short: 'd4c3b2a1',
      display_name: 'Dion',
      role: 'Owner',
      is_you: true,
    },
  ],
  ingest: { status: 'Listening', dropped_unbound: 0, dropped_invalid: 0 },
  codex_hook: { installed: false, binary_available: true },
  hook: { installed: false, binary_available: true },
  agents:
    scenario === 'agents' ||
    scenario === 'three-agents' ||
    scenario === 'installed'
      ? [
          {
            ...agent('fixture-build', 'Claude build'),
            state: 'Building',
            provenance: { Inferred: { rule: 'ToolEdit' } },
          },
          {
            ...agent('fixture-testing', 'Codex test', 'Codex'),
            state: 'Testing',
            provenance: { Inferred: { rule: 'ToolShell' } },
          },
        ]
      : [],
};
if (scenario === 'three-agents')
  workspace.agents.push({
    ...agent('fixture-finished', 'Claude done'),
    state: 'Finished',
    provenance: { Verified: { source: 'Stop' } },
  });
if (
  scenario === 'agents' ||
  scenario === 'three-agents' ||
  scenario === 'installed'
)
  workspace.agents = workspace.agents.map((a) => ({
    ...a,
    bound: true,
    connection: a.state ? 'Connected' : 'NotConnected',
  }));
if (scenario === 'installed') {
  workspace.hook.installed = true;
  workspace.codex_hook.installed = true;
}
if (scenario === 'connected') {
  workspace.hook.installed = true;
  workspace.agents = [
    {
      ...agent('fixture-clock-agent', 'clock runner'),
      bound: true,
      state: 'Thinking',
      provenance: { Verified: { source: 'UserPromptSubmit' } },
      connection: 'Connected',
    },
  ];
}
let status: AppStatus =
  scenario === 'joining'
    ? 'Joining'
    : scenario === 'setup' || scenario === 'join'
      ? 'Setup'
      : scenario === 'error'
        ? { Error: 'IdentityMissing' }
        : scenario === 'partial-create'
          ? { Error: 'PartialCreate' }
          : { Ready: workspace };
const listeners = new Map<number, number>();
const commands: { command: string; args: unknown }[] = [];
let nextPlan = 0;
let planned: HookPlan | null = null;
let failure: AppError | null = null;
let delay = 0;
let actionDelay = 0;
const hookCommand =
  "'/fixture USB/meshyard-hook' claude-code --socket '/fixture USB/ingest.sock'";
const fixtureHooks = Object.fromEntries(
  [
    'SessionStart',
    'UserPromptSubmit',
    'PreToolUse',
    'PostToolUse',
    'Stop',
    'StopFailure',
    'SessionEnd',
  ].map((event) => [
    event,
    [{ hooks: [{ type: 'command', command: hookCommand, async: true }] }],
  ]),
);
function fixtureDiff(remove: boolean, kind: string) {
  const path =
    kind === 'Codex' ? '~/.codex/hooks.json' : '~/.claude/settings.json';
  const hooks =
    kind === 'Codex'
      ? Object.fromEntries(
          Object.entries(fixtureHooks)
            .filter(([event]) => event !== 'StopFailure')
            .map(([event, value]) => [
              event,
              value.map((entry) => ({
                hooks: entry.hooks.map((hook) => ({
                  ...hook,
                  command: hook.command.replace('claude-code', 'codex'),
                })),
              })),
            ]),
        )
      : fixtureHooks;
  const before = JSON.stringify({ hooks: remove ? hooks : {} }, null, 2);
  const after = JSON.stringify({ hooks: remove ? {} : hooks }, null, 2);
  return `--- ${path} (hooks)\n+++ ${path} (hooks)\n@@ -1,${String(before.split('\n').length)} +1,${String(after.split('\n').length)} @@\n${before
    .split('\n')
    .map((line) => `-${line}`)
    .join('\n')}\n${after
    .split('\n')
    .map((line) => `+${line}`)
    .join('\n')}\n`;
}
const ready = () => {
  status = { Ready: structuredClone(workspace) };
  return status;
};
function emitChanged() {
  const internal = window as unknown as {
    __TAURI_INTERNALS__: { runCallback: (id: number, event: unknown) => void };
  };
  for (const [id, callback] of listeners)
    internal.__TAURI_INTERNALS__.runCallback(callback, {
      event: 'workspace-changed',
      id,
      payload: null,
    });
}
const peerScenario = scenario?.startsWith('peer-') ?? false;
const mockNetwork = {
  running: peerScenario || scenario === 'joining',
  mode: 'Strict P2P · no relay or public discovery',
  peers: peerScenario
    ? [
        {
          device_short: 'ab12cd34',
          path: ['peer-disconnected', 'peer-error'].includes(scenario ?? '')
            ? 'Offline'
            : 'Direct P2P',
        },
      ]
    : [],
  error:
    scenario === 'peer-error'
      ? 'Strict: no direct path observed within 10 s; possible causes include NAT without port mapping or a firewall'
      : null,
  pairings:
    peerScenario && !['peer-invite', 'peer-error'].includes(scenario ?? '')
      ? [
          {
            id: 'mock-pairing-id',
            peer_short: 'ab12cd34',
            words: [
              'clearpine',
              'coralacorn',
              'duskelm',
              'navywolf',
              'calmpine',
              'azurefinch',
            ],
            is_inviter: true,
            local_confirmed: scenario === 'peer-confirmed',
            state:
              scenario === 'peer-rejected'
                ? 'Aborted'
                : scenario === 'peer-disconnected'
                  ? 'Disconnected'
                  : 'Reserved',
          },
        ]
      : [],
};
mockIPC(async (command, args) => {
  commands.push({
    command,
    args:
      command === 'network_join' ? { redacted: true } : structuredClone(args),
  });
  if (command === 'network_status') return structuredClone(mockNetwork);
  if (command === 'network_start') {
    mockNetwork.running = true;
    return structuredClone(mockNetwork);
  }
  if (command === 'network_stop') {
    mockNetwork.running = false;
    mockNetwork.pairings = [];
    return;
  }
  if (command === 'network_invite')
    return 'meshyard://join/MOCK-ONLY-NOT-A-CAPABILITY-' + 'A'.repeat(650);
  if (command === 'network_join') {
    mockNetwork.running = true;
    return;
  }
  if (command === 'network_confirm') {
    const { matches } = args as { matches: boolean };
    mockNetwork.pairings.forEach((p) => {
      p.local_confirmed = matches;
      if (!matches) p.state = 'Aborted';
    });
    return;
  }
  if (command === 'plugin:event|listen') {
    // The pinned Tauri mock unregisters the callback using the returned event ID.
    const callback = (args as { handler: number }).handler;
    listeners.set(callback, callback);
    return callback;
  }
  if (command === 'plugin:event|unlisten') {
    listeners.delete((args as { eventId: number }).eventId);
    return;
  }
  if (scenario === 'loading') return new Promise<AppStatus>(() => {});
  // Capture status now, then delay completion: used to prove refresh/action race handling.
  if (command === 'app_status') {
    const captured = structuredClone(status);
    if (delay)
      await new Promise((yes) => {
        setTimeout(yes, delay);
      });
    return captured;
  }
  if (command === 'hook_plan' || command === 'hook_remove') {
    if (failure) {
      const error = failure;
      failure = null;
      // Tauri serializes this Rust enum rejection as a string.
      // eslint-disable-next-line @typescript-eslint/prefer-promise-reject-errors
      return Promise.reject(error);
    }
    const remove = command === 'hook_remove';
    const kind = (args as { kind: string }).kind;
    planned = {
      plan_id: `fixture-plan-${String(++nextPlan)}`,
      settings_path_display:
        kind === 'Codex' ? '~/.codex/hooks.json' : '~/.claude/settings.json',
      base_sha256: 'a'.repeat(64),
      action: remove
        ? 'Remove'
        : workspace.hook.installed
          ? 'NoChange'
          : 'Install',
      diff:
        workspace.hook.installed && !remove
          ? `No changes to hooks.\n${JSON.stringify({ hooks: fixtureHooks }, null, 2)}\n`
          : fixtureDiff(remove, kind),
    };
    return structuredClone(planned);
  }
  if (command === 'create_workspace') {
    const { input } = args as { input: CreateWorkspace };
    workspace = {
      ...workspace,
      name: input.name,
      members: workspace.members.map((member) => ({
        ...member,
        display_name: input.display_name,
      })),
    };
  } else if (command === 'register_agent') {
    const { input } = args as { input: RegisterAgent };
    const item = agent(
      `fixture-own-${String(workspace.agents.length)}`,
      input.label,
      input.kind,
    );
    item.bound =
      input.kind === 'ClaudeCode' &&
      !workspace.agents.some((a) => a.is_you && a.kind === 'ClaudeCode');
    workspace = { ...workspace, agents: [...workspace.agents, item] };
  } else if (command === 'bind_agent') {
    const { agentId } = args as { agentId: string };
    if (
      !workspace.agents.some(
        (a) => a.id === agentId && a.is_you && a.kind === 'ClaudeCode',
      )
    )
      return { Error: 'InvalidInput' };
    workspace = {
      ...workspace,
      agents: workspace.agents.map((a) =>
        a.kind === 'ClaudeCode'
          ? { ...a, bound: a.id === agentId, connection: 'NotConnected' }
          : a,
      ),
    };
  } else if (command === 'hook_apply') {
    if (actionDelay)
      await new Promise((yes) => {
        setTimeout(yes, actionDelay);
      });
    if (failure) {
      const error = failure;
      failure = null;
      return { Error: error };
    }
    if ((args as { planId: string }).planId !== planned?.plan_id)
      return { Error: 'HookPlanStale' };
    workspace.hook.installed = planned.action !== 'Remove';
    workspace.agents = workspace.agents.map((a) => ({
      ...a,
      connection:
        a.bound && workspace.hook.installed
          ? a.state
            ? 'Connected'
            : 'WaitingForFirstEvent'
          : 'NotConnected',
    }));
    planned = null;
  } else if (command === 'update_profile') {
    workspace = {
      ...workspace,
      members: workspace.members.map((m) => ({
        ...m,
        display_name: (args as { name: string }).name,
      })),
    };
  } else throw new Error('Unexpected fixture command');
  const result = ready();
  emitChanged();
  return result;
});
const fixture = {
  snapshot: () => ({
    commands: structuredClone(commands),
    listeners: listeners.size,
    workspace: structuredClone(workspace),
    planned: structuredClone(planned),
  }),
  event: (state: AgentView['state']) => {
    workspace.agents = workspace.agents.map((a) =>
      a.bound
        ? {
            ...a,
            state,
            provenance: state
              ? { Verified: { source: 'UserPromptSubmit' } }
              : null,
            stale: false,
            connection:
              workspace.hook.installed && state ? 'Connected' : 'NotConnected',
          }
        : a,
    );
    ready();
    emitChanged();
  },
  setAgents: (agents: AgentView[]) => {
    workspace.agents = agents;
    ready();
    emitChanged();
  },
  staleWithoutEvent: () => {
    workspace.agents = workspace.agents.map((a) =>
      a.bound ? { ...a, stale: true, connection: 'Stale' } : a,
    );
    ready();
  },
  failNext: (error: AppError) => {
    failure = error;
  },
  delayActions: (ms: number) => {
    actionDelay = ms;
  },
  delayReads: (ms: number) => {
    delay = ms;
  },
  addForeign: () => {
    workspace.agents.push({
      ...agent('d4c3b2a1-foreign-same-prefix', 'foreign agent'),
      is_you: false,
    });
    ready();
    emitChanged();
  },
  burst: () => {
    for (let i = 0; i < 20; i += 1) emitChanged();
  },
  unmount: () => {
    root.unmount();
  },
};
declare global {
  interface Window {
    connectFixture: typeof fixture;
  }
}
window.connectFixture = fixture;
const element = document.getElementById('root');
if (!element) throw new Error('Missing browser fixture root');
const root = createRoot(element);
root.render(
  <StrictMode>
    <App />
  </StrictMode>,
);
