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

## 6. Connect flow and exact DTOs (addendum 2026-11-03, owner: "wie kann ich ein Agent registrieren damit es verbunden ist?")
Status of work: backend prep §1–3 is running in `t-007b-local-agent-integration` (base `3e8ccab`).
After it, that branch merges the reviewed T-015 code (`a859ed4`) and T-016 when reviewed, then
does §4–6. One writer per file; no competing frontend writer.

**User flow (one panel "Connect Claude Code" in the workspace, reachable from the title bar):**
1. Choose the agent: list of own `ClaudeCode` agents (`bind_agent`) or "Register one" (focuses the
   existing T-015 form). **Registration alone is never "connected"** — the agent shows
   "Not connected" until a real hook event arrives.
2. Review: `hook_plan` → show the exact JSON diff of `~/.claude/settings.json`, the base SHA-256,
   the hook binary path and socket path. Buttons "Install" / "Cancel".
3. Install: `hook_apply(plan_id)` only on the button. No startup install, no manual shell/config
   workaround offered.
4. Status (from backend, never guessed in the UI): Waiting for first event → Connected (receiving)
   → Stale/Error, plus "Remove hook".
- Codex row: "Codex connection is not available yet." (T-008); no button.

**Exact additions (Rust `view.rs`, ts-rs generated — frontend uses only these):**
```
AgentView   += stale: bool, bound: bool, connection: AgentConnection
enum AgentConnection { NotConnected, WaitingForFirstEvent, Connected, Stale }
   // NotConnected: not bound or hook not installed; WaitingForFirstEvent: bound + hook installed +
   // no ingest-derived AgentState yet; Connected: ≥1 ingest-derived AgentState and not stale;
   // Stale: per is_stale (10 min). Only the bound agent can leave NotConnected.
WorkspaceView += ingest: IngestView, hook: HookView
struct IngestView { status: IngestStatus, dropped_unbound: u32, dropped_invalid: u32 }
enum  IngestStatus { Disabled, Listening, Unavailable(AppError) }
struct HookView { installed: bool, binary_available: bool }   // installed = exact Meshyard entries present
struct HookPlan { plan_id: String, settings_path_display: String, base_sha256: String,
                  diff: String, action: HookAction }          // diff = unified text, display only
enum  HookAction { Install, Remove, NoChange }
AppError    += IngestUnavailable, HookBinaryMissing, HookPlanStale, HookConfigInvalid
```
`settings_path_display` may show `~/.claude/settings.json` (home abbreviated); no other paths in
views except the hook binary/socket inside the diff the owner must approve.

**Frontend files (after T-016):** `app/src/features/connect/ConnectClaudeCode.tsx` (new),
`ipc/client.ts` (+4 typed wrappers), `App.tsx` (`workspace-changed` listener), `AgentList.tsx`
(connection label), `WorkspaceScreen.tsx` (panel + title-bar entry). The yard needs no change:
real state changes walk via T-016 `update`.

**Optional isolated CLI capture (distinct from the owner flow, not required):** only if the
installed Claude Code CLI verifiably supports loading an extra settings file per run (check
`claude --help` for `--settings`; do not assume). Then: temporary settings JSON in the USB profile
with the real hook binary/socket, a throwaway prompt in a temporary USB directory, no change to
`~/.claude/settings.json`, no HOME override. Running it uses Dion's Claude account → ask Dion
first. Reported as evidence level (c′) "isolated CLI capture", never as the owner-flow gate L4.
Knower research (`/Volumes/MeshyardProjects/meshyard-smithers/knower-cli-capture-advice.md`, not
verified by a run): `--settings` alone **merges** with existing user hooks; isolation needs the
restricted-settings mode plus the explicit file (managed/auth settings still apply); `-p` may end
unfinished async hooks at teardown, so count actually captured/cancelled events, not exit 0.

## 7. Agent projection and bounded read paths (ADR-014, required before live onboarding is accepted)
**Problem (found in WIP):** `workspace_state` verifies the genesis and **every** stored event, then
folds. `own_agent`, `agent_binding`, `bind_first_own_agent`, ingest receive/flush and `app_status`
all call it, so each hook envelope costs ~3·N signature checks, each idle tick N, and the
`workspace-changed → app_status` refresh adds another full replay (O(N²) over a session). The
100 000-event cap only bounds N.

**Store changes (`crates/meshyard-store`, append-only migration):**
- `agent_projection(workspace, author, local_id, kind, label, state, provenance BLOB, tool,
  task_label, last_state_ts_ms, PRIMARY KEY(workspace, author, local_id))` (typed columns or a
  postcard-encoded domain value; decode errors → `StoreError::Database`).
- `profile_projection(workspace, author, display_name, avatar, PRIMARY KEY(workspace, author))`.
- `members.added_name TEXT NOT NULL DEFAULT ''`, set by `apply_contiguous_membership` from
  `MemberAdded { display_name }` (owner seeded with '').
- One function `apply_contiguous_projection(&VerifiedEvent)` called right next to every existing
  `apply_contiguous_membership` call (direct insert, each promoted pending event, backfill), same
  transaction. Rules = `fold` phase 2 per author: `AgentRegistered` upsert + reset (Idle, provenance
  NULL, tool NULL, task NULL, ts NULL); `AgentState` UPDATE only if the row exists (sets state,
  provenance, tool, task_label, `last_state_ts_ms = event.ts_ms`); `AgentGone` DELETE;
  `ProfileUpdated` upsert. Membership events: no projection change.
- Every place that deletes an author's `events` rows (pending/quota overflow in `insert`,
  `discard_oversized_unconfirmed` at open) also deletes that author's `agent_projection` and
  `profile_projection` rows in the same transaction.
- One-time backfill per workspace at open when `meta` lacks `projection_v1:<workspace>`: verify and
  apply every contiguous event in (author, seq) order — the only full verification pass, then set
  the marker. New workspaces set the marker at `add_workspace`.
- Genesis: verify once at open/`add_workspace`; keep the decoded `Genesis` per workspace in the DB
  actor (single DB thread, so every `Store` clone shares it).

**Read APIs (replace hot-path uses of `workspace_state`):**
- `Store::workspace_snapshot(workspace) -> WorkspaceSnapshot { genesis: Genesis, epoch: u32,
  members: Vec<Member>, agents: Vec<Agent>, agent_state_ts: BTreeMap<(DeviceId,[u8;16]), u64> }`
  built by SQL: active members (`removed = 0`, owner included), name =
  `profile_projection.display_name` if present else `members.added_name`, avatar from profile or 0;
  agents = `agent_projection` rows whose author is an active member. No signature verification.
- `Store::own_agent(workspace, own, local_id) -> Option<Agent>`: one indexed row lookup + active
  member check. `bind_own_agent`, `bind_first_own_agent` (count of own rows of that kind via SQL),
  `agent_binding` (binding row joined with `agent_projection` on author = own, same kind; stale
  binding → `None`) use only these lookups.
- App: `AppCore::status`, ingest receive/flush/tick and binding use `workspace_snapshot` /
  `own_agent` only. `workspace_state` remains for tests and an explicit audit.

**Required tests (store crate, counting verifier as in T-002):**
1. Seed ≥ 5 000 owner events (incl. agent registrations/states) and some member events; reopen once
   (backfill counted), then **reset the counter**: 100× `workspace_snapshot`, `own_agent`,
   `agent_binding`, `bind_own_agent` (no change), and app-level idle tick/status → **0**
   verifications. Each new own append → exactly 1 verification (the receive-time check); a promoted
   pending remote event → its bounded receive + promotion checks only, independent of N.
2. Differential: after each step of a scripted sequence, `workspace_snapshot` equals the members/
   agents of `fold(workspace_state)`: re-register same id resets state; `AgentState` before register
   ignored; `AgentGone` deletes; another author's same `local_id` stays separate; binding with wrong
   kind / other author / gone agent → `None`; member removal hides agents and profile, re-grant
   restores them; gap → pending (no projection change) → promotion applies in order; equivocating
   and rejected events change nothing; a forced error inside `insert` rolls back event and
   projection together; quota overflow deletes projection rows; reopen and fresh backfill give the
   same snapshot as incremental maintenance.
3. Existing T-002 tests unchanged and green; wire v1 untouched.

## Out of scope
Codex adapter (T-008), project-scope install, remote agents/networking. Avatar movement is T-016.
