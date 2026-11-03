# T-007b — Local agent integration (live Claude Code status in the app)

Status: ready (stacked, D-9) · Owner: Codex (lane B) · Reviewer: Claude · Written 2026-11-03
Normative: PROTOCOL §9 v1.2 (envelope, hook-side check, state table, staleness), T-007 spec,
T-014 spec + review. Wire v1 unchanged. Extends T-014 files; replaces nothing.

## Branch and dependencies
- Branch `t-007b-local-agent-integration` from T-014's head **after** review fix 1 (stacked), then a
  normal merge of the reviewed T-007a head. Until both exist, work may start from `6aca159` on the
  parts below marked **(prep)** — they touch no T-014-owned logic. Final review needs T-014 code
  accepted and T-007a `done`; T-014 native gate N-14 is **not** required to implement or review
  T-007b, only for marking the combined flow done.

## Result
Running Claude Code with the Meshyard hook installed makes the selected agent in the yard and list
change state live (Thinking, Building, Testing, Merging, Finished, Idle, Error), with provenance,
from real signed `AgentState` events in the local store. Nothing is installed without the owner
seeing the exact diff and pressing "Install".

## 1. Ingest server (prep) — `app/src-tauri/src/ingest.rs`
- Socket dir `<canonical data dir>/ingest`, created/validated with the existing ACL-aware
  `meshyard-store` `private_directory` (make it `pub`, no logic change; expose the canonical data dir
  via `Store::directory()`); mode `0700`. Socket `ingest.sock`, `0600`.
- Path length: if the socket path exceeds 103 bytes → ingest disabled with `IngestUnavailable`
  (no fallback to `/tmp` or elsewhere).
- Existing file at the socket path: remove only if `symlink_metadata` shows a socket owned by
  `rustix::process::geteuid()` (the data-dir lock guarantees no other instance); otherwise
  `IngestUnavailable`.
- Listener: `tokio::net::UnixListener`. Each connection: `UnixStream::peer_cred()?.uid()` must equal
  `geteuid().as_raw()` (tokio uses `getpeereid` on macOS; no `unsafe`, no new crate) else close.
  Dependencies: app `tokio` gains features `net`, `io-util`, `time` (workspace pin `=1.53.3`
  unchanged); `meshyard-agents` and the workspace `rustix` (`process`) added to the app.
- Bounds: ≤ 8 concurrent connections (semaphore; excess closed), one line ≤ 1 KiB read within
  200 ms, then close. Parse with `meshyard-agents` ingest types; invalid → drop silently (counter only).
- Threat boundary stays per OS user: other processes of the same user can inject events;
  "Verified" keeps the PROTOCOL §9 meaning.

## 2. Agent binding (explicit, local only)
- T-014 allows several agents per kind, so routing is never guessed. New local table (append-only
  store migration) `agent_bindings(workspace, kind, agent_local_id)`, one row per kind, only for the
  device's **own** registered agents. Not signed, not synced.
- Command `bind_agent(agent_id)` (AgentView.id of an own agent) sets the binding for that agent's
  kind. Registering the first agent of a kind binds it automatically; later ones do not.
- Messages for a kind without a binding are dropped and counted; the UI shows
  "Hook events received, no agent selected for Claude Code".

## 3. State → signed events
- One `meshyard-agents` state machine per kind (sessions inside it per T-007a); the app feeds
  messages and a 5 s tick (Finished → Idle after 60 s).
- On a state change (state, provenance or tool differs from the last appended for that agent),
  `append_own(AgentState { agent_local_id, state, provenance, tool, task_label: None })` under the
  existing AppCore mutex. Identical repeats are not appended. ≤ 20 appends/s per agent (excess
  coalesced to the latest state).
- Sharing policy default applies; no task label from hooks.

## 4. Live UI
- After each append the app emits Tauri event `workspace-changed` (no payload). `App.tsx` listens
  and calls `app_status`. Capability adds only `core:event:allow-listen` and
  `core:event:allow-unlisten` for the main window.
- `AgentView` gains `stale: bool` (T-007a `is_stale`, last own `AgentState` `ts_ms` vs now, 10 min)
  and `bound: bool`; `WorkspaceView` gains `ingest: IngestView { status: Disabled | Listening |
  Unavailable(AppError), dropped_unbound: u32, dropped_invalid: u32 }`. Regenerate bindings.
- List/yard: stale → "Unknown (no events for 10 min)" with last state dimmed; "Select for hook"
  button on unbound own agents. Replace the static T-007 hint text with the real ingest status.

## 5. Hook installer (explicit approval)
- Commands `hook_plan() -> HookPlan`, `hook_apply(plan_id) -> AppStatus`, `hook_remove() -> HookPlan`.
  User scope only: `~/.claude/settings.json` (path injected for tests).
- Plan = exact before/after JSON of the `hooks` entries Meshyard adds for the 7 PROTOCOL §9 events:
  `{"type":"command","command":"<abs meshyard-hook> claude-code --socket <abs socket>","async":true}`.
  Shows base SHA-256. Hook binary: `MESHYARD_HOOK_BIN` (absolute, executable) in dev; absent →
  `HookBinaryMissing` (bundled path is T-013).
- Apply only if the file's current SHA-256 equals the plan's base hash (else `HookPlanStale`),
  invalid JSON → `HookConfigInvalid` (never overwritten), backup
  `settings.json.meshyard-backup-<unix ms>`, atomic write, idempotent (existing Meshyard entries
  recognized by exact command), unrelated keys preserved byte-for-byte where possible. Remove =
  reverse diff, same rules.
- Never at startup, never without the button. UI: "Connect Claude Code" panel shows the diff in a
  `<pre>`, "Install" and "Cancel"; afterwards "Remove hook".
- Commands total: T-014's four + `bind_agent`, `hook_plan`, `hook_apply`, `hook_remove`; manifest
  and capability list exactly these. No fs/shell plugin.

## Acceptance
1. Server tests (temp private dir): wrong-UID path via injected expected UID → rejected; > 1 KiB,
   slow (> 200 ms), malformed lines dropped; 9th concurrent connection closed; existing non-socket
   file at the path → `IngestUnavailable`, not deleted.
2. Live ingest test: write T-007a fixture envelopes to the real socket → signed `AgentState` events
   in the store → `app_status` shows the bound agent Thinking → Testing → Finished, then Idle after
   60 s (fake clock); unbound kind → no event, counter increments; identical repeats → one event.
3. Installer tests on a temp settings file: plan diff exact; apply → backup + entries; second apply →
   no change; external edit between plan and apply → `HookPlanStale`; invalid JSON untouched; remove
   restores the non-Meshyard content.
4. Frontend tests: stale/bound/ingest views, diff panel, event-triggered refresh (mocked listener).
5. `npm run check` green; each new module has a named caller in these flows.
6. **Evidence levels, reported separately:** (a) envelope-to-socket tests = server/pipeline proof;
   (b) hook binary tests in T-007a = hook proof; (c) **actual Claude Code CLI capture** = Dion installs
   via the diff in the native app and uses Claude Code (owner gate L4). Only (c) proves real hook
   payloads reach the app; (a)/(b) never count as (c).
7. Tests never touch the owner's real `~/.claude/settings.json` or Keychain.

## Out of scope
Codex adapter (T-008), project-scope install, remote agents/networking, avatar animation.
