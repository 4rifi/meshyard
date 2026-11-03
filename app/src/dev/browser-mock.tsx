// Evidence-only entry, never imported by main.tsx or included in its production graph.
import { mockIPC } from '@tauri-apps/api/mocks';
import { createRoot } from 'react-dom/client';
import { App } from '../App';
import type { AppStatus } from '../ipc/generated/AppStatus';
import type { WorkspaceView } from '../ipc/generated/WorkspaceView';
import type { CreateWorkspace } from '../ipc/generated/CreateWorkspace';
import type { RegisterAgent } from '../ipc/generated/RegisterAgent';
import '../styles.css';

if (import.meta.env.MODE !== 'browser-mock-ipc') {
  throw new Error('Use the dedicated browser-mock-ipc evidence build.');
}
const scenario = new URLSearchParams(location.search).get('scenario');
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
  agents:
    scenario === 'agents'
      ? [
          {
            id: 'fixture-arrival',
            label: 'main',
            kind: 'ClaudeCode',
            state: null,
            provenance: null,
          },
          {
            id: 'fixture-testing',
            label: 'test runner',
            kind: 'Codex',
            state: 'Testing',
            provenance: { Inferred: { rule: 'ToolShell' } },
          },
          {
            id: 'fixture-finished',
            label: 'review complete',
            kind: 'ClaudeCode',
            state: 'Finished',
            provenance: { Verified: { source: 'Stop' } },
          },
        ]
      : [],
};
let status: AppStatus =
  scenario === 'setup'
    ? 'Setup'
    : scenario === 'error'
      ? { Error: 'IdentityMissing' }
      : scenario === 'partial-create'
        ? { Error: 'PartialCreate' }
        : { Ready: workspace };
mockIPC((command, args) => {
  if (scenario === 'loading')
    return new Promise<AppStatus>(() => {
      /* deliberately unresolved fixture */
    });
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
    status = { Ready: workspace };
  } else if (command === 'register_agent') {
    const { input } = args as { input: RegisterAgent };
    workspace = {
      ...workspace,
      agents: [
        ...workspace.agents,
        {
          ...input,
          id: `fixture-${String(workspace.agents.length)}`,
          state: null,
          provenance: null,
        },
      ],
    };
    status = { Ready: workspace };
  } else if (command === 'update_profile') {
    const { name } = args as { name: string };
    workspace = {
      ...workspace,
      members: workspace.members.map((member) => ({
        ...member,
        display_name: name,
      })),
    };
    status = { Ready: workspace };
  } else if (command !== 'app_status') {
    throw new Error('Unexpected fixture command');
  }
  return status;
});
const root = document.getElementById('root');
if (!root) throw new Error('Missing browser fixture root');
createRoot(root).render(<App />);
