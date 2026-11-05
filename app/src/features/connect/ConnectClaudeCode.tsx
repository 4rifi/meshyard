import type { AgentKind } from '../../ipc/generated/AgentKind';
import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { WorkspaceView } from '../../ipc/generated/WorkspaceView';
import type { AppStatus } from '../../ipc/generated/AppStatus';
import type { AppError } from '../../ipc/generated/AppError';
import type { HookPlan } from '../../ipc/generated/HookPlan';
import { hookPlan, hookRemove } from '../../ipc/client';
import { advice } from '../errors/advice';

import { connectionLabels } from './connectionLabels';
export function IngestNotice({ workspace }: { workspace: WorkspaceView }) {
  const { status, dropped_unbound, dropped_invalid } = workspace.ingest;
  return (
    <div role="status">
      <p>
        {status === 'Listening'
          ? 'Local hook intake is listening.'
          : status === 'Disabled'
            ? 'Local hook intake is disabled.'
            : advice[status.Unavailable]}
      </p>
      {dropped_unbound > 0 && (
        <p>
          Hook events received, no agent selected for this agent kind (
          {dropped_unbound}).
        </p>
      )}
      {dropped_invalid > 0 && (
        <p>{dropped_invalid} invalid hook events rejected.</p>
      )}
    </div>
  );
}
export function ConnectClaudeCode({
  workspace,
  initialKind = 'ClaudeCode',
  busy,
  onBind,
  onApply,
  onRegister,
  onClose,
}: {
  workspace: WorkspaceView;
  initialKind?: AgentKind;
  busy: boolean;
  onBind: (id: string) => Promise<AppStatus | undefined>;
  onApply: (id: string) => Promise<AppStatus | undefined>;
  onRegister: () => void;
  onClose: () => void;
}) {
  const [kind, setKind] = useState<AgentKind>(initialKind);
  const label = kind === 'Codex' ? 'Codex' : 'Claude Code';
  const hook = kind === 'Codex' ? workspace.codex_hook : workspace.hook;
  const [plan, setPlan] = useState<HookPlan | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [pending, setPending] = useState(false);
  const lock = useRef(false);
  const active = useRef(false);
  const section = useRef<HTMLElement>(null);
  const heading = useRef<HTMLHeadingElement>(null);
  useLayoutEffect(() => {
    const fit = () => {
      const element = section.current;
      if (element)
        element.style.setProperty(
          '--connect-height',
          `${String(Math.max(160, Math.floor(window.innerHeight - element.getBoundingClientRect().top - 12)))}px`,
        );
    };
    fit();
    window.addEventListener('resize', fit);
    const observer = new ResizeObserver(fit);
    const header = section.current
      ?.closest('main')
      ?.querySelector('.workspace-header');
    if (header) observer.observe(header);
    return () => {
      window.removeEventListener('resize', fit);
      observer.disconnect();
    };
  }, [plan]);
  const review = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    active.current = true;
    heading.current?.focus();
    return () => {
      active.current = false;
    };
  }, []);
  useEffect(() => {
    if (plan) review.current?.focus();
  }, [plan]);
  const bound = workspace.agents.find(
    (agent) => agent.is_you && agent.kind === kind && agent.bound,
  );
  const own = workspace.agents.filter(
    (agent) => agent.is_you && agent.kind === kind,
  );
  const disabled = busy || pending;
  const run = async (action: () => Promise<void>) => {
    if (lock.current || busy) return;
    lock.current = true;
    setPending(true);
    setError(null);
    try {
      await action();
    } finally {
      lock.current = false;
      if (active.current) setPending(false);
    }
  };
  const prepare = (remove: boolean) => {
    void run(async () => {
      const result = await (remove ? hookRemove(kind) : hookPlan(kind));
      if (!active.current) return;
      if ('error' in result) setError(result.error);
      else setPlan(result.plan);
    });
  };
  return (
    <section
      ref={section}
      className="panel connect-panel"
      aria-labelledby="connect-heading"
      aria-busy={disabled}
    >
      <h2 id="connect-heading" tabIndex={-1} ref={heading}>
        Connect agents
      </h2>
      <p hidden={plan !== null}>
        Choose your local agent, review the exact settings changes, then
        install. Registration alone does not connect an agent.
      </p>
      <div hidden={plan !== null}>
        <label htmlFor="connect-kind">Agent kind</label>
        <select
          id="connect-kind"
          value={kind}
          disabled={disabled || plan !== null}
          onChange={(event) => {
            setKind(event.target.value === 'Codex' ? 'Codex' : 'ClaudeCode');
            setError(null);
          }}
        >
          <option value="ClaudeCode">Claude Code</option>
          <option value="Codex">Codex</option>
        </select>
        {kind === 'Codex' && (
          <p>
            After Install, open /hooks in Codex and review and trust the exact
            Meshyard hooks. Restart Codex to receive SessionStart. Changed hooks
            require trust again. Project hooks also require a trusted project.
          </p>
        )}
        <IngestNotice workspace={workspace} />
        {error && (
          <p role="alert">
            <strong>{error}</strong>: {advice[error]}
          </p>
        )}
        <ul className="items">
          {own.map((agent) => (
            <li key={agent.id}>
              <strong>{agent.label}</strong>
              <code>{agent.id}</code>
              <span>{connectionLabels[agent.connection]}</span>
              <button
                type="button"
                disabled={disabled || agent.bound || plan !== null}
                onClick={() => {
                  void run(async () => {
                    const result = await onBind(agent.id);
                    if (
                      active.current &&
                      result &&
                      typeof result === 'object' &&
                      'Error' in result
                    )
                      setError(result.Error);
                  });
                }}
              >
                {agent.bound ? 'Selected for hook' : 'Select for hook'}
              </button>
            </li>
          ))}
        </ul>
        <button
          type="button"
          disabled={disabled || plan !== null}
          onClick={onRegister}
        >
          Register one
        </button>
        <p role="status">
          {bound
            ? bound.connection === 'WaitingForFirstEvent'
              ? `Start ${label} or send a message in an existing session. Waiting for the first event.`
              : connectionLabels[bound.connection]
            : `Choose your own ${label} agent to continue.`}
        </p>
        {!hook.binary_available && (
          <p>
            The hook executable is unavailable. Move Meshyard to /Applications
            and reopen it before installing hooks.
          </p>
        )}
      </div>
      {plan ? (
        <div className="hook-review">
          <h3 tabIndex={-1} ref={review}>
            Review {plan.action === 'Remove' ? 'hook removal' : 'hook settings'}
          </h3>
          <p>
            Settings: <code>{plan.settings_path_display}</code>
          </p>
          <details className="plan-metadata">
            <summary>Technical details</summary>
            <p>
              Base SHA-256: <code className="hash">{plan.base_sha256}</code>
            </p>
          </details>
          <pre tabIndex={0} aria-label="Exact settings diff">
            {plan.diff}
          </pre>
          <div className="review-actions">
            {plan.action !== 'NoChange' && (
              <button
                type="button"
                disabled={disabled}
                onClick={() => {
                  void run(async () => {
                    const result = await onApply(plan.plan_id);
                    if (!active.current || !result) return;
                    if (typeof result === 'object' && 'Error' in result)
                      setError(result.Error);
                    else {
                      setPlan(null);
                      heading.current?.focus();
                    }
                  });
                }}
              >
                {plan.action === 'Remove' ? 'Remove hook' : 'Install'}
              </button>
            )}
            <button
              type="button"
              disabled={disabled}
              onClick={() => {
                setPlan(null);
                setError(null);
                heading.current?.focus();
              }}
            >
              Cancel
            </button>
            <button type="button" disabled={disabled} onClick={onClose}>
              Close connection panel
            </button>
          </div>
        </div>
      ) : (
        <>
          <button
            type="button"
            disabled={disabled || !bound}
            onClick={() => {
              prepare(false);
            }}
          >
            Review settings
          </button>
          {hook.installed && (
            <button
              type="button"
              disabled={disabled}
              onClick={() => {
                prepare(true);
              }}
            >
              Review hook removal
            </button>
          )}
        </>
      )}

      {!plan && (
        <button type="button" disabled={disabled} onClick={onClose}>
          Close connection panel
        </button>
      )}
    </section>
  );
}
