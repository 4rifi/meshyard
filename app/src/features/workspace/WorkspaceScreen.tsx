import { useEffect, useRef, useState } from 'react';
import type { WorkspaceView } from '../../ipc/generated/WorkspaceView';
import type { RegisterAgent as RegisterInput } from '../../ipc/generated/RegisterAgent';
import { mountYard } from '../../playground/yard';
import { MemberList } from './MemberList';
import { AgentList } from './AgentList';
import { RegisterAgent } from './RegisterAgent';
export function WorkspaceScreen({
  workspace,
  busy,
  onRegister,
  onProfile,
}: {
  workspace: WorkspaceView;
  busy: boolean;
  onRegister: (input: RegisterInput) => Promise<void>;
  onProfile: (name: string) => void;
}) {
  const host = useRef<HTMLDivElement>(null);
  const [rendererFailed, setRendererFailed] = useState(false);
  const [name, setName] = useState(
    workspace.members.find((m) => m.is_you)?.display_name ?? '',
  );
  useEffect(() => {
    if (!host.current) return;
    return mountYard(host.current, workspace.agents, () => {
      setRendererFailed(true);
    });
  }, [workspace.agents]);
  return (
    <>
      <header className="workspace-header">
        <div>
          <p className="eyebrow">Meshyard · Local workspace</p>
          <h1 tabIndex={-1}>{workspace.name}</h1>
        </div>
        <span className="mode">
          {workspace.mode === 'Strict' ? 'Strict P2P' : 'Auto'}
        </span>
        <p className="identity">
          Workspace <code>{workspace.id_short}</code> · Your device{' '}
          <code>{workspace.device_short}</code>
        </p>
      </header>
      <section className="panel yard" aria-labelledby="yard-heading">
        <div className="yard-heading">
          <h2 id="yard-heading">The yard</h2>
          <p>Stations reflect observed agent states</p>
        </div>
        <div
          className="yard-viewport"
          tabIndex={0}
          role="region"
          aria-label="Yard map; scroll horizontally on small screens"
        >
          <div className="canvas-host" ref={host} aria-hidden="true" />
        </div>
        {!workspace.agents.length && (
          <p className="empty-yard">Register an agent to see it here.</p>
        )}
        <p className="narrow-yard-help">
          Scroll the yard horizontally; full agent list below.
        </p>
        <p className="yard-caption">
          <span>{workspace.agents.length} registered agents</span>
          <span>Static view · Full names and status below</span>
        </p>
        {rendererFailed && (
          <p role="status">
            Graphics are unavailable. Every agent is listed below.
          </p>
        )}
      </section>
      <div className="workspace-grid">
        <section className="panel" aria-labelledby="agents-heading">
          <h2 id="agents-heading">
            Agents <span className="count">{workspace.agents.length}</span>
          </h2>
          <AgentList agents={workspace.agents} />
          <p>
            Live status arrives when the Claude Code hook is installed (T-007).
          </p>
          <RegisterAgent busy={busy} onRegister={onRegister} />
        </section>
        <div className="side-panels">
          <section className="panel" aria-labelledby="members-heading">
            <h2 id="members-heading">Members</h2>
            <MemberList members={workspace.members} />
            <form
              onSubmit={(e) => {
                e.preventDefault();
                onProfile(name);
              }}
            >
              <label htmlFor="edit-name">Edit display name</label>
              <input
                id="edit-name"
                required
                maxLength={64}
                value={name}
                onChange={(e) => {
                  setName(e.target.value);
                }}
                disabled={busy}
              />
              <button disabled={busy} type="submit">
                Save display name
              </button>
            </form>
          </section>
          <section className="panel" aria-labelledby="peers-heading">
            <h2 id="peers-heading">Peers</h2>
            <p>No peers. Networking is not built yet.</p>
          </section>
        </div>
      </div>
    </>
  );
}
