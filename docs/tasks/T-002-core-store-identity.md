# T-002 — Core types, local store, device identity

Status: ready (after T-001) · Owner: Codex · Reviewer: Claude

## Objective
Domain model + persistence + identity that every later layer builds on.

## Scope
### meshyard-core (no I/O dependencies)
- Types from `ARCHITECTURE.md §6`: `WorkspaceId`, `DeviceId` (wraps 32-byte public key; do not
  depend on iroh here — convert in `meshyard-net`), `Workspace`, `Member`, `Mode {Auto, Strict}`,
  `Event { author, seq, ts_ms, kind }`, `EventKind`, `ActivityState`, `Provenance`,
  `ToolCategory`, `AgentKind {ClaudeCode, Codex, Other(String)}`.
- `fold(events) -> DerivedState` (members, agents with current state) — deterministic regardless of
  interleaving across authors given per-author order. Property test with `proptest`.
- `SharingPolicy` with `filter(&EventKind) -> Option<EventKind>`; default policy per
  `SECURITY.md §5`. Unit tests proving free-text fields are dropped unless explicitly allowed.
- Serde derive for all wire types; `ts-rs` export for UI-facing types into
  `app/src/ipc/generated/` (generation via a test or build step; document how to regenerate).

### meshyard-store
- `Store::open(path)` with WAL mode, `rusqlite_migration` migrations.
- Tables: `workspace`, `members`, `peers` (endpoint_id, display_name, last_seen, last_path),
  `events` (workspace_id, author, seq, ts_ms, kind_json, PRIMARY KEY(workspace_id, author, seq)),
  `invites` (token_hash, expires_at, used_at).
- API: `append_own(kind) -> Event` (assigns next seq atomically), `insert_remote(event)`
  (idempotent, rejects gaps? → store but mark; document choice), `heads()`,
  `events_after(author, seq, limit)`.
- Store runs on a dedicated thread; async callers use a channel-based handle (no blocking in tokio
  worker threads).

### Identity
- `DeviceKeyStore` trait; `KeychainKeyStore` (service `computer.meshyard.device`, account =
  workspace-independent "device") and `FileKeyStore` (0600, used in tests and as fallback with a
  logged warning). Secret key bytes never appear in `Debug`/logs.

## Acceptance criteria
- [ ] Unit + property tests pass; `npm run check` green.
- [ ] Test: two stores with shuffled-but-per-author-ordered event sets fold to identical state.
- [ ] Test: default `SharingPolicy` never emits prompt/command/file-path text.
- [ ] Test: `FileKeyStore` creates files with mode 0600; `Debug` of key type prints no key bytes.
- [ ] Keychain path tested manually on macOS (report honestly; may prompt the user).
- [ ] Report in `docs/reports/T-002.md`.
