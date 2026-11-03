# Scrubbed documentation fixtures

These are **doc-derived**, not live captures. Retrieved on 2026-11-03 from the
[official Claude Code hooks reference](https://code.claude.com/docs/en/hooks).
Version scope: rolling, unversioned documentation as retrieved; no installed CLI
version has been captured or validated. The directory date is not a CLI version.

Retained only documented event/common fields and example correlation identifiers.
Removed prompts, commands, paths, transcript locations, output, assistant messages,
file contents, and unrelated metadata. Empty `tool_input` objects mark scrubbing;
there is no captured outcome evidence. No fixture asserts KQ-003 outcomes.

| Fixture          | Primary source section                                       |
| ---------------- | ------------------------------------------------------------ |
| SessionStart     | https://code.claude.com/docs/en/hooks#sessionstart-input     |
| UserPromptSubmit | https://code.claude.com/docs/en/hooks#userpromptsubmit-input |
| PreToolUse       | https://code.claude.com/docs/en/hooks#pretooluse-input       |
| PostToolUse      | https://code.claude.com/docs/en/hooks#posttooluse-input      |
| Stop             | https://code.claude.com/docs/en/hooks#stop-input             |
| StopFailure      | https://code.claude.com/docs/en/hooks#stopfailure-input      |
| SessionEnd       | https://code.claude.com/docs/en/hooks#sessionend-input       |

The fixtures feed the actual hook normalizer before the agents parser and golden
state assertions. Synthetic command/classifier, ordering and privacy sentinels
live in test source, clearly separate from these documentation fixtures.
