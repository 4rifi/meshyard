# T-007 — Claude Code agent pipeline

Status: ready (part a) · Owner: Codex (lane B) · Reviewer: Claude · Written 2026-11-03
Normative: PROTOCOL §9 as amended below (KQ-002, KQ-003); ADR-004, ADR-005; SECURITY sharing defaults.

## Split
- **T-007a (start now, stacked on `c93fbe2`, branch `t-007a-hook-normalizer`)**: `meshyard-hook`
  binary + `meshyard-agents` normalizer/state machine + fixtures. Pure crates, no app wiring.
- **T-007b (after T-014 review `done`)**: ingest socket server inside the app process, hook
  installer (diff + explicit approval), appending `AgentState` for the agent registered in T-014,
  live UI refresh (first Tauri event, extends T-014's `commands.rs`/`client.ts`).

## Decisions (resolve KQ-002 / KQ-003, 2026-11-03)
- **KQ-002 single deadline:** the hook has one end-to-end budget of **150 ms** from process start:
  stdin read ≤ 100 ms (stop at EOF or 64 KiB), connect ≤ 20 ms, write ≤ 20 ms, then exit. On any
  timeout/error: drop the event, exit 0, print nothing (never block or fail the agent). Empty stdin
  exits immediately. ARCHITECTURE §7.1 "50 ms" summary is superseded. The unchanged T-001 criterion
  (start + exit < 20 ms with empty stdin) is verified here, measured from USB and from the installed
  app on the internal disk (T-013); report both, first run and warm p95.
- **KQ-003 outcome evidence:** ingest messages carry only bounded enums, never output/error text:
  `tool_outcome: Success | Failure | Unknown`, `request_status: Ok | Failed | Unknown`. A value other
  than `Unknown` is allowed only where a captured, scrubbed fixture from a named CLI version shows a
  structured field that determines it; everything else maps to `Unknown`. States derived from
  `Unknown` outcomes are `Inferred`, never `Verified`.

## T-007a scope
- `crates/meshyard-hook/src/main.rs`: std + serde/serde_json only. Reads stdin, extracts the
  allowlist (`hook_event_name`, `session_id`, `tool_name`→category, outcome enums, cwd **hash** only),
  writes one length-prefixed JSON message to `$MESHYARD_SOCKET` (UID check happens server-side in
  T-007b). Unknown fields ignored.
- `crates/meshyard-agents/src/{ingest,normalize,state}.rs`: message type, `normalize(msg) ->
  Option<NormalizedEvent>`, state machine per PROTOCOL §9 (correlation by session, staleness → Idle
  after 60 s, Finished on Stop), producing `(ActivityState, Provenance, Option<ToolCategory>)`.
  Pure functions + an injectable clock.
- Fixtures `crates/meshyard-agents/tests/fixtures/claude-code-<version>/*.json`: scrubbed real payloads
  (no prompt text, commands, paths, file contents). If no live capture is authorized, use payloads
  from the official hook documentation and label them `doc-derived`; never claim a live capture.

## Acceptance (T-007a)
- Golden tests: each fixture → expected state/provenance; unknown event → no state change.
- Hook binary tests: empty stdin, malformed JSON, 1 MiB stdin, absent socket, blocked socket —
  each exits 0 within the budget (measured in-test with generous CI margin; real timings in report).
- Privacy test: no fixture string from prompt/command/path fields appears in any produced message.
- `npm run check` green; files touched only `crates/meshyard-hook/**`, `crates/meshyard-agents/**`,
  their manifests, `docs/reports/T-007.md`.
