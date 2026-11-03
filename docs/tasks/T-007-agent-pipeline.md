# T-007 — Claude Code agent pipeline

Status: ready (part a) · Owner: Codex (lane B) · Reviewer: Claude · Written 2026-11-03
Normative: **PROTOCOL §9 v1.2** (2026-11-03 clarification: envelope, budget, hook-side command
classification and socket check, correlation IDs, staleness); ADR-004, ADR-005; SECURITY sharing
defaults. Where this file and PROTOCOL §9 differ, PROTOCOL wins.

## Split
- **T-007a (start now, stacked on `c93fbe2`, branch `t-007a-hook-normalizer`)**: `meshyard-hook`
  binary + `meshyard-agents` normalizer/state machine + fixtures. Pure crates, no app wiring.
- **T-007b** — authoritative spec: `docs/tasks/T-007b-local-agent-integration.md` (stacked per D-9;
  summary): ingest socket server inside the app process, hook
  installer (diff + explicit approval), appending `AgentState` for the agent registered in T-014,
  live UI refresh (first Tauri event, extends T-014's `commands.rs`/`client.ts`).

## Decisions (resolve KQ-002 / KQ-003, 2026-11-03)
- **KQ-002 single deadline:** the hook has one end-to-end budget of **150 ms** from process start:
  stdin read ≤ 100 ms (stop at EOF; over 1 MiB → drop), connect ≤ 20 ms, write ≤ 20 ms, then exit. On any
  timeout/error: drop the event, exit 0, print nothing (never block or fail the agent). Empty stdin
  exits immediately. ARCHITECTURE §7.1 "50 ms" summary is superseded. The unchanged T-001 criterion
  (start + exit < 20 ms with empty stdin) is verified here, measured from USB and from the installed
  app on the internal disk (T-013); report both, first run and warm p95.
- **KQ-003 outcome evidence:** outcome data is only ever bounded enums, never output/error text:
  `tool_outcome: Success | Failure | Unknown`, `request_status: Ok | Failed | Unknown`. A value other
  than `Unknown` needs a captured, scrubbed fixture from a named CLI version with a structured field
  that determines it. **Not part of the T-007a envelope:** no T-007a rule consumes them (Fixing and
  Codex error are not in scope), so they are added only together with their consuming rule.

## T-007a scope (all rules: PROTOCOL §9 v1.2)
- `crates/meshyard-hook/src/main.rs`: std + serde/serde_json, plus the workspace-pinned `rustix`
  (`process` feature) solely for `geteuid()`; no root-manifest version change. `--socket <abs path>` argument;
  reads stdin (1 MiB cap); forwards only the 7 `HookEvent` names; maps `tool_name` → tool category;
  for shell tools classifies `tool_input.command` (first 4 KiB) into `command_class`
  `test | git_merge | other` **in the hook** and drops the string; validates correlation IDs
  (`session_id` required; `turn_id`, `tool_use_id`, `agent_id` optional; 1–128 bytes
  `[A-Za-z0-9._:-]`); hook-side socket owner check (no symlinks, socket type, parent dir mode
  `0700`, socket UID **and** parent UID == `rustix::process::geteuid().as_raw()`; proves ownership
  only, see PROTOCOL §9); T-007b's server reuses `meshyard-store` `private_directory` for the
  socket dir and keeps the peer-UID check;
  writes one newline-terminated JSON envelope ≤ 1 KiB. No cwd, no hash of cwd, no outcome fields.
- `crates/meshyard-agents/src/{ingest,state}.rs`: envelope type parsed into the existing
  `meshyard-core` `HookEvent`/`ToolCategory` (no duplicate enums except the local `CommandClass`);
  state machine per the PROTOCOL §9 v1.2 table: per-session tracking (≤ 32 sessions), turn starts
  only with `UserPromptSubmit`, ended-turn drop (≤ 16 turn ids), duplicate drop (≤ 64 per session),
  Finished → Idle `Inferred{TurnEnded}` after 60 s, producing `(ActivityState, Provenance,
  Option<ToolCategory>)`. Pure functions + an injectable clock.
- **Staleness is not in the state machine and is never emitted.** It is local derived view
  metadata: T-007b adds `stale: bool` to `AgentView`, computed by the app when the last
  `AgentState` is not Idle/Finished and older than 10 min (local receive time); the UI shows
  "Unknown (no events for 10 min)" with the last state dimmed. Same rule for remote agents later.
  T-007a provides only a pure helper `is_stale(last_state, last_seen, now) -> bool` with tests,
  called by T-007b.
- Fixtures `crates/meshyard-agents/tests/fixtures/claude-code-<version>/*.json`: scrubbed real payloads
  (no prompt text, commands, paths, file contents). If no live capture is authorized, use payloads
  from the official hook documentation and label them `doc-derived`; never claim a live capture.

## Acceptance (T-007a)
- Golden tests: each fixture → expected state/provenance per the PROTOCOL §9 v1.2 table; unknown or
  non-forwarded event → no state change; late tool event after Stop ignored; ended `turn_id` ignored;
  duplicate `(event, tool_use_id, agent_id)` ignored; Finished → Idle after 60 s with the fake clock.
- Classifier table test: every listed `test`/`git_merge` form, compound commands (`cd x && cargo
  test`, `FOO=1 npm test`), precedence `git_merge` > `test`, and lookalikes (`echo cargo test` →
  `other`).
- Hook binary tests: empty stdin, malformed JSON, > 1 MiB stdin, invalid/missing `session_id`,
  absent socket, blocked socket, symlinked socket, parent dir not `0700`, and an **injected UID
  mismatch** (owner check takes the expected effective UID as a parameter; test passes a different
  value for socket and for parent separately) — each exits 0 / rejects within the budget without
  writing (generous in-test margin; real timings in report). No real cross-user or root test is
  required or claimed.
- Privacy test: no fixture string from prompt/command/path/cwd fields appears in any envelope.
- `npm run check` green; files touched only `crates/meshyard-hook/**`, `crates/meshyard-agents/**`,
  their manifests, `docs/reports/T-007.md`.
