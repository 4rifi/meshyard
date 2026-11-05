# Codex documentation fixtures

Doc-derived, not captured from a running CLI. Source retrieved 2026-11-05:
https://learn.chatgpt.com/docs/hooks (rolling documentation, no CLI version claim).
Retained only event names, example identifiers and allowlisted tool metadata.
Paths, prompts, commands, transcript and tool output are omitted. Empty tool_input
marks scrubbing. Synthetic shell commands for classifier tests live in test source.

Codex reports apply_patch as the canonical edit tool and Bash for shell tools.
All six supported lifecycle events fit frozen wire v1. PostModelRequest failures
can be retried; without captured structured outcome evidence and an honest wire
provenance rule, they are not mapped to StopFailure or Error. PermissionRequest,
Interrupt and subagent lifecycle events are unsupported and dropped.
