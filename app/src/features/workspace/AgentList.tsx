import { kindLabel, provenanceLabel } from './activityLabels';
import { connectionLabels } from '../connect/connectionLabels';
import type { AgentKind } from '../../ipc/generated/AgentKind';
import type { AgentView } from '../../ipc/generated/AgentView';
import { activityLabel } from '../../playground/yardData';
export function AgentList({
  agents,
  busy,
  onSelect,
  onInspect,
}: {
  agents: AgentView[];
  busy: boolean;
  onSelect: (kind: AgentKind) => void;
  onInspect?: (id: string) => void;
}) {
  if (!agents.length) return <p>No agents registered yet.</p>;
  return (
    <ul className="items">
      {agents.map((agent) => (
        <li key={agent.id}>
          {onInspect ? (
            <button
              type="button"
              onClick={() => {
                onInspect(agent.id);
              }}
              aria-label={`Inspect ${agent.label}`}
            >
              {agent.label}
            </button>
          ) : (
            <strong>{agent.label}</strong>
          )}
          <span>
            {kindLabel(agent.kind)} ·{' '}
            <span className={`badge${agent.stale ? ' stale-state' : ''}`}>
              {agent.stale
                ? `Unknown (no events for 10 min) · Last: ${activityLabel(agent.state)}`
                : activityLabel(agent.state)}
            </span>
          </span>
          <span>{connectionLabels[agent.connection]}</span>
          {agent.is_you &&
            (agent.kind === 'ClaudeCode' || agent.kind === 'Codex') &&
            !agent.bound && (
              <button
                type="button"
                disabled={busy}
                onClick={() => {
                  onSelect(agent.kind);
                }}
              >
                Select for hook
              </button>
            )}
          {agent.provenance && (
            <span className="badge">{provenanceLabel(agent.provenance)}</span>
          )}
        </li>
      ))}
    </ul>
  );
}
