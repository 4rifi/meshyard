import { PeerPanel } from '../peers/PeerPanel';
import { useCallback, useEffect, useRef, useState } from 'react';
import type { AgentKind } from '../../ipc/generated/AgentKind';
import type { WorkspaceView } from '../../ipc/generated/WorkspaceView';
import type { RegisterAgent as RegisterInput } from '../../ipc/generated/RegisterAgent';
import { createYard, type Yard } from '../../playground/yard';
import { MemberList } from './MemberList';
import { AgentList } from './AgentList';
import { provenanceLabel, kindLabel } from './activityLabels';
import type { AppStatus } from '../../ipc/generated/AppStatus';
import { ConnectClaudeCode, IngestNotice } from '../connect/ConnectClaudeCode';
import { activityLabel } from '../../playground/yardData';
import { RegisterAgent } from './RegisterAgent';
import fontLicense from '../../assets/fonts/OFL.txt?raw';
export function WorkspaceScreen({
  workspace,
  busy,
  onRegister,
  onProfile,
  onBind,
  onApply,
}: {
  workspace: WorkspaceView;
  busy: boolean;
  onRegister: (input: RegisterInput) => Promise<void>;
  onProfile: (name: string) => void;
  onBind: (id: string) => Promise<AppStatus | undefined>;
  onApply: (id: string) => Promise<AppStatus | undefined>;
}) {
  const [view, setView] = useState<'yard' | 'agents' | 'settings' | 'peers'>(
    'yard',
  );
  const [selected, setSelected] = useState<string | null>(null);
  const opener = useRef<HTMLElement | null>(null);
  const selectAgent = useCallback((id: string) => {
    opener.current =
      document.activeElement instanceof HTMLElement &&
      document.activeElement !== document.body
        ? document.activeElement
        : (host.current?.parentElement ?? null);
    setSelected(id);
  }, []);
  const inspector = useRef<HTMLDialogElement>(null);
  const inspected = workspace.agents.find((agent) => agent.id === selected);
  const inspectedId = inspected?.id;
  const closeInspector = () => {
    inspector.current?.close();
    setSelected(null);
    opener.current?.focus();
  };
  useEffect(() => {
    if (inspectedId) {
      inspector.current?.showModal();
      inspector.current?.focus();
    }
  }, [inspectedId]);
  const host = useRef<HTMLDivElement>(null);
  const yard = useRef<Yard | null>(null);
  const connectButton = useRef<HTMLButtonElement>(null);
  const [connecting, setConnecting] = useState<AgentKind | null>(null);
  const agentLabel = useRef<HTMLInputElement>(null);
  const registrationPending = useRef(false);
  useEffect(() => {
    if (view === 'agents' && registrationPending.current) {
      registrationPending.current = false;
      agentLabel.current?.focus();
    }
  }, [view]);
  const [rendererFailed, setRendererFailed] = useState(false);
  const [name, setName] = useState(
    workspace.members.find((m) => m.is_you)?.display_name ?? '',
  );
  useEffect(() => {
    if (!host.current) return;
    const scene = createYard(
      host.current,
      () => {
        setRendererFailed(true);
      },
      (id) => {
        opener.current = host.current?.parentElement ?? null;
        setSelected(id);
      },
    );
    yard.current = scene;
    return () => {
      scene.destroy();
      yard.current = null;
    };
  }, [workspace.id_short, selectAgent]);
  useEffect(() => {
    yard.current?.update(workspace.agents);
  }, [workspace.id_short, workspace.agents]);
  const focusRegistration = () => {
    setConnecting(null);
    if (view !== 'agents') {
      registrationPending.current = true;
      setView('agents');
    } else agentLabel.current?.focus();
  };
  return (
    <>
      <header className="workspace-header">
        <div>
          <p className="eyebrow">Meshyard · Local workspace</p>
          <h1 tabIndex={-1}>{workspace.name}</h1>
        </div>
        <button
          ref={connectButton}
          className="register-shortcut"
          type="button"
          disabled={busy}
          aria-expanded={connecting !== null}
          aria-controls="connection-panel"
          onClick={() => {
            setConnecting('ClaudeCode');
          }}
        >
          Connect agents
        </button>
        {busy ? (
          <span className="register-shortcut">Saving…</span>
        ) : (
          <a
            className="pixel-control register-shortcut"
            href="#agent-label"
            onClick={(event) => {
              event.preventDefault();
              focusRegistration();
            }}
          >
            Register agent
          </a>
        )}
        <p className="identity">
          Workspace <code>{workspace.id_short}</code> · Your device{' '}
          <code>{workspace.device_short}</code>
        </p>
      </header>
      <nav className="view-tabs" aria-label="Workspace views">
        {(['yard', 'agents', 'settings', 'peers'] as const).map((tab) => (
          <button
            type="button"
            key={tab}
            aria-current={view === tab && !connecting ? 'page' : undefined}
            onClick={() => {
              setConnecting(null);
              setView(tab);
            }}
          >
            {tab === 'yard'
              ? 'Yard'
              : tab === 'agents'
                ? 'Agent list'
                : tab === 'settings'
                  ? 'Settings'
                  : 'Peers'}
          </button>
        ))}
      </nav>
      {view === 'peers' && !connecting && (
        <PeerPanel
          owner={workspace.members.some((m) => m.is_you && m.role === 'Owner')}
        />
      )}
      <div id="connection-panel" hidden={!connecting}>
        {connecting && (
          <ConnectClaudeCode
            workspace={workspace}
            initialKind={connecting}
            busy={busy}
            onBind={onBind}
            onApply={onApply}
            onRegister={focusRegistration}
            onClose={() => {
              setConnecting(null);
              connectButton.current?.focus();
            }}
          />
        )}
      </div>
      <section
        hidden={view !== 'yard' || connecting !== null}
        className="panel yard"
        aria-labelledby="yard-heading"
      >
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
        <p className="yard-caption">
          <span>{workspace.agents.length} registered agents</span>
          <span>3 visible per station; all in Agent list</span>
          <span>
            Pan with touch, scrollbar or arrow keys · Agent list for all details
          </span>
          <span>Review / Fix / Waiting: reserved, no hook activity</span>
        </p>
        {rendererFailed && (
          <p role="status">
            Graphics are unavailable. Every agent is listed below.
          </p>
        )}
      </section>
      {inspected && (
        <dialog
          ref={inspector}
          onCancel={(event) => {
            event.preventDefault();
            closeInspector();
          }}
          tabIndex={-1}
          className="panel inspector"
          aria-labelledby="inspector-heading"
        >
          <h2 id="inspector-heading">Agent inspector · {inspected.label}</h2>
          <p>
            {kindLabel(inspected.kind)} · {activityLabel(inspected.state)}
          </p>
          <p>
            {inspected.stale
              ? 'Unknown (no events for 10 min)'
              : 'Last observed state'}
          </p>
          <p>
            {provenanceLabel(inspected.provenance) ??
              'No hook events yet. Registration alone does not connect an agent.'}
          </p>
          <p>
            Verified means a hook-shaped message from the same OS user, not
            proof of what the agent accomplished.
          </p>
          <p className="inspector-id">
            Agent ID
            <br />
            <code>{inspected.id}</code>
          </p>
          <div className="inspector-actions">
            <button
              type="button"
              onClick={() => {
                closeInspector();
              }}
            >
              Close inspector
            </button>
          </div>
        </dialog>
      )}
      <div
        className="workspace-grid"
        hidden={connecting !== null || view === 'yard'}
      >
        <section
          hidden={view !== 'agents'}
          className="panel"
          aria-labelledby="agents-heading"
        >
          <h2 id="agents-heading">
            Agents <span className="count">{workspace.agents.length}</span>
          </h2>
          <AgentList
            agents={workspace.agents}
            busy={busy}
            onInspect={selectAgent}
            onSelect={setConnecting}
          />
          <IngestNotice workspace={workspace} />
          <RegisterAgent
            busy={busy}
            onRegister={onRegister}
            labelInput={agentLabel}
          />
        </section>
        <div className="side-panels" hidden={view !== 'settings'}>
          <section className="panel" aria-labelledby="members-heading">
            <h2 id="members-heading">Settings and members</h2>
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
            <details className="storage-details">
              <summary>Storage and privacy</summary>
              <p>
                Workspace mode:{' '}
                {workspace.mode === 'Strict' ? 'Strict P2P' : 'Auto'}. Peer
                pairing uses private invitations and a six-word comparison under
                Peers.
              </p>
              <p>
                Local-first storage. Device identity uses macOS Keychain by
                default. A locked or unavailable Keychain is reported; Meshyard
                never silently changes key stores.
              </p>
              <p>
                Hooks share only activity categories and provenance. Prompts,
                paths, commands and tool output stay out of Meshyard events.
              </p>
            </details>
            <details className="font-license">
              <summary>Pixelify Sans · Font license</summary>
              <pre>{fontLicense}</pre>
            </details>
          </section>
          <section className="panel" aria-labelledby="peers-heading">
            <h2 id="peers-heading">Peers</h2>
            <p>
              Open Peers to pair devices with a private invitation and six-word
              comparison. Direct connections only; no relay.
            </p>
          </section>
        </div>
      </div>
    </>
  );
}
