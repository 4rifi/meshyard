import type { AgentView } from '../../ipc/generated/AgentView';
import { activityLabel } from '../../playground/yardData';
export function AgentList({ agents }: { agents: AgentView[] }) {
  if (!agents.length) return <p>No agents registered yet.</p>;
  return (
    <ul className="items">
      {agents.map((agent) => (
        <li key={agent.id}>
          <strong>{agent.label}</strong>
          <span>
            {agent.kind === 'ClaudeCode' ? 'Claude Code' : agent.kind} ·{' '}
            {activityLabel(agent.state)}
          </span>
          {agent.provenance && (
            <span>
              {'Verified' in agent.provenance
                ? `Verified · ${agent.provenance.Verified.source}`
                : `Inferred · ${agent.provenance.Inferred.rule}`}
            </span>
          )}
        </li>
      ))}
    </ul>
  );
}
