# T-014 — Local workspace slice (first real app screen)

Status: ready · Owner: Codex (lane A) · Reviewer: Claude · Written 2026-11-03
Branch: `t-014-local-workspace-slice` from `t-002-core-store-identity` @ `c93fbe2` (stacked, D-9).
Normative: PROTOCOL §§1, 4, 10; ADR-010, ADR-011; KQ-005.

## Why
Dion's screenshot (2026-11-03 19:39) shows the scaffold: one teal rectangle, "No agents or
peers are connected". The app must start doing real work with the code that already exists
(T-002 core/store/identity). This task pulls forward the **minimal** parts of T-009 (typed
bridge) and T-011 (create workspace). T-009/T-011 later **extend these files**; they do not
replace them. No networking, no hooks (T-003…T-007).

## User-visible result
1. First start: "Create workspace" form (name, display name, mode). Strict P2P is the only
   enabled mode; Auto is shown disabled with "Requires a self-hosted relay (G-1)".
2. After create, and on every later start: the workspace screen, reopened from SQLite:
   workspace name, short WorkspaceId, mode badge, own device short id, member list (owner = you),
   agent list and a yard canvas.
3. "Register local agent" (kind Claude Code / Codex, label ≤ 64) appends a real signed
   `AgentRegistered` event. The agent appears with state **"No events yet"** and no provenance
   badge. Text states: "Live status arrives when the Claude Code hook is installed (T-007)."
4. "Edit display name" appends `ProfileUpdated`.
5. Peers panel: "No peers. Networking is not built yet." (plain text, no button).
6. Yard canvas (Pixi, reuse `playground/sceneLifecycle.ts`): one token per registered agent with
   its label, placed in the zone of its `ActivityState` (only the "No events yet" zone is
   reachable until T-007). Empty yard shows "Register an agent to see it here." The rectangle
   placeholder (`playground/placeholder.ts`) and its caption are **deleted**.
7. Honest error screens (no fallback): identity Locked/Unavailable/Invalid, data directory busy
   (second instance), unsafe permissions, store/migration error, more than one workspace in the
   data dir (KQ-005). Each shows the typed error kind and one sentence of advice.

## Backend (Rust)
- **Store read API** (`crates/meshyard-store/src/{lib,database}.rs`):
  `Store::workspaces() -> Vec<WorkspaceId>` and
  `Store::workspace_state(id) -> (Genesis, DerivedState)` running the existing `fold` over the
  stored contiguous events on the DB thread. Bounded: refuse > 100 000 events with a typed error
  (incremental projection is later work, review note). Tests for both.
- **App state** `app/src-tauri/src/state.rs`: one `AppCore { store, signer, workspace: Option<WorkspaceId> }`
  behind `tokio::sync::Mutex`, opened once at startup.
  - Data dir: `MESHYARD_DATA_DIR` if set, else Tauri `app_data_dir()`. Dev runs on the USB image
    set it (document in README); the app must not write elsewhere.
  - Key store: `MESHYARD_KEY_STORE=file` → `FileKeyStore(<data_dir>/identity)` (explicit dev/test
    selection, PROTOCOL §10); otherwise `KeychainKeyStore`. Use `load_or_create`; any error other
    than first-time `NotFound` is surfaced as an error screen. **Never** switch backend on error.
  - Create = `sign_genesis` (owner = own device, mode Strict) → `add_workspace` →
    `append_own(ProfileUpdated { display_name })`. No `MemberAdded` for the owner (seeded by
    genesis, PROTOCOL §4). Reject create if a workspace already exists.
- **Commands** `app/src-tauri/src/commands.rs` (only these four):
  `app_status() -> AppStatus`, `create_workspace(CreateWorkspace) -> AppStatus`,
  `update_profile(String) -> AppStatus`, `register_agent(RegisterAgent) -> AppStatus`.
  Each mutating command returns the fresh view; no event bus yet (T-009 adds `listen` for live
  hook/network updates).
- **View model** `app/src-tauri/src/view.rs`: `AppStatus = Setup | Ready(WorkspaceView) | Error(AppError)`,
  `WorkspaceView { id_short, name, mode, device_short, members: Vec<MemberView>, agents: Vec<AgentView> }`,
  `AgentView { kind, label, state: Option<ActivityState>, provenance: Option<Provenance> }`
  (`None` = no events yet). Derive `ts_rs::TS`; export into `app/src/ipc/generated/` through the
  existing `npm run bindings:generate` pipeline (extend it; generated files never hand-edited).
  No secrets, paths or IP addresses in any view type.
- **Permissions:** register the four commands in the app manifest
  (`tauri_build::Attributes`/`AppManifest`) and allow exactly them in `capabilities/main.json`.
  CSP unchanged.

## Frontend (React)
- `app/src/ipc/client.ts`: typed wrappers around `invoke` using generated types only (no `any`,
  no string-typed payloads).
- `app/src/features/setup/CreateWorkspace.tsx`, `features/workspace/{WorkspaceScreen,MemberList,AgentList,RegisterAgent}.tsx`,
  `features/errors/ErrorScreen.tsx`, `playground/yard.ts` (Pixi tokens from `AgentView[]`).
  `App.tsx` only switches on `AppStatus`. Small components, keyboard reachable, labelled inputs,
  focus moves to the heading after each screen change, `prefers-reduced-motion` respected.
- The agent list is the accessible equivalent of the yard (same data).

## File ownership (lane A only)
`app/**`, `crates/meshyard-store/src/{lib,database}.rs`, `crates/meshyard-store/tests/store.rs`
(append only), `crates/meshyard-core/examples/export_bindings.rs`, `package.json` scripts,
`README.md` dev section, `docs/reports/T-014.md`. Do not touch `crates/meshyard-net`,
`crates/meshyard-agents`, `crates/meshyard-hook` (other lanes).

## Acceptance scenarios
1. Fresh `MESHYARD_DATA_DIR` on USB + `MESHYARD_KEY_STORE=file`: create "Demo" / "Dion" → workspace
   screen shows name, Strict badge, one member "Dion (owner)", empty agent list, empty-yard text.
2. Quit and restart with the same env: the same workspace, member and agents reappear (WorkspaceId
   unchanged).
3. Register "Claude Code" / "main": list and yard show it with "No events yet"; restart keeps it.
4. Second app instance on the same data dir → "Data directory is already in use" screen.
5. Mock key store returning `Locked` (Rust test) → `AppStatus::Error(IdentityLocked)`; no key file
   created, no other backend tried.
6. Rust integration test drives the command functions directly (no GUI): create → register →
   reopen → `AppStatus` equals expected view.
7. Vitest: each screen renders from a fixture `AppStatus`; ErrorScreen covers every `AppError` kind.
8. `npm run check` green. No new npm dependency; any new Rust dependency justified.
9. Every new production module has a named caller in these flows (list them in the report);
   `placeholder.ts` removed; no unused exports (`cargo clippy -D warnings`, ESLint).
10. Report states exactly which scenarios ran headless/in tests and which (1–4) Dion verifies in
    the native window; no native/Keychain result is claimed unless run.

## Out of scope
Networking, invites, hooks/ingest, live updates, avatars/animation, multiple workspaces, Auto mode.
