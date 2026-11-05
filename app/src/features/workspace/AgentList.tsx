import { connectionLabels } from '../connect/connectionLabels';
import type { AgentKind } from '../../ipc/generated/AgentKind';
import type { AgentView } from '../../ipc/generated/AgentView';
import type { HookEvent } from '../../ipc/generated/HookEvent';
import type { RuleId } from '../../ipc/generated/RuleId';
import { activityLabel } from '../../playground/yardData';
// Verified names identify source events; inferred names identify classifier rules.
const sourceLabels: Record<HookEvent, string> = {
  SessionStart: 'Session start event',
  UserPromptSubmit: 'Prompt submitted event',
  PreToolUse: 'Before tool event',
  PostToolUse: 'After tool event',
  Stop: 'Stop event',
  StopFailure: 'Stop failure event',
  SessionEnd: 'Session end event',
};
const ruleLabels: Record<RuleId, string> = {
  ToolEdit: 'Edit tool',
  ToolShell: 'Shell tool',
  ToolRead: 'Read tool',
  ToolWeb: 'Web tool',
  TurnEnded: 'Turn ended',
  Stale: 'Stale activity',
};
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
            {agent.kind === 'ClaudeCode' ? 'Claude Code' : agent.kind} ·{' '}
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
            <span className="badge">
              {'Verified' in agent.provenance
                ? `Verified · ${sourceLabels[agent.provenance.Verified.source]}`
                : `Inferred · ${ruleLabels[agent.provenance.Inferred.rule]}`}
            </span>
          )}
        </li>
      ))}
    </ul>
  );
}
