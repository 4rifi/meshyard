import type { AgentKind } from '../../ipc/generated/AgentKind';
import type { HookEvent } from '../../ipc/generated/HookEvent';
import type { RuleId } from '../../ipc/generated/RuleId';
import type { Provenance } from '../../ipc/generated/Provenance';
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
export const kindLabel = (kind: AgentKind) =>
  kind === 'ClaudeCode' ? 'Claude Code' : kind;
export function provenanceLabel(provenance: Provenance | null): string | null {
  return provenance === null
    ? null
    : 'Verified' in provenance
      ? `Verified · ${sourceLabels[provenance.Verified.source]}`
      : `Inferred · ${ruleLabels[provenance.Inferred.rule]}`;
}
