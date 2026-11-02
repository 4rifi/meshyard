# T-002 — Core types, wire v1, local store, device identity

Status: ready (after T-001) · Owner: Codex · Reviewer: Claude
Revised 2026-11-02 after the Codex architecture review. **Normative reference: `docs/PROTOCOL.md`
§§1–5 and §10.** Where this spec is silent, PROTOCOL.md decides.

## Objective
Domain model, frozen wire format v1, signed events, persistence and identity that every later
layer builds on.

## Scope
### meshyard-core (no I/O dependencies)
- Domain types: `WorkspaceId`, `DeviceId` (32-byte public key; no iroh dependency here),
  `Genesis`, `Workspace`, `Member`, `Mode {Auto, Strict}`, `ActivityState`, `Provenance`
  (`Verified{source: HookEvent}` / `Inferred{rule: RuleId}` — enums, no free text),
  `ToolCategory`, `AgentKind`.
- `wire::v1`: `EventHeader`, `EventBody`, `SignedEvent`, frame header (`MSHY` magic, version, type,
  len) — **separate from domain and IPC types**, with conversion functions. Golden-bytes tests for
  every wire enum/struct (fixed bytes checked into tests).
- Signing/verification via a `Signer`/`Verifier` trait (implementation with `ed25519-dalek` or
  the key type iroh uses — pick one, justify it); domain-separated message per PROTOCOL §2.
- `WorkspaceId` derivation from `Genesis` per PROTOCOL §1.
- `fold(genesis, accepted_events) -> DerivedState` implementing PROTOCOL §4: membership first,
  authorization rules per body, agent ownership `(author, agent_local_id)`, pending-membership
  holding. Property test (`proptest`): any cross-author interleaving respecting per-author order
  folds to identical state.
- String caps + control-char stripping per PROTOCOL §4.
- `SharingPolicy::filter` (default per `SECURITY.md §5`) applied when *creating* own events.
- `ts-rs` export for UI-facing domain types (not wire types) into `app/src/ipc/generated/`;
  document the regeneration command.

### meshyard-store
- `Store::open(data_dir)`, WAL, `rusqlite_migration`.
- Tables: `workspaces` (id, genesis_blob), `events` (workspace, author, seq, signed_blob, PK
  (workspace, author, seq)), `pending_events` (bounded per PROTOCOL §3), `peers`, `pairings`
  (state machine columns for PROTOCOL §7; logic comes in T-005), `meta`.
- API: `append_own(workspace, body, signer) -> SignedEvent` (atomic next seq), `insert_remote(ev)
  -> Inserted | Duplicate | Pending | Equivocation | Rejected(reason)` (verifies signature),
  `contiguous_heads(workspace)`, `events_after(workspace, author, seq, limit)`.
- Dedicated DB thread + channel handle for async callers.

### Identity & process model (PROTOCOL §10)
- `DeviceKeyStore` trait; `KeychainKeyStore` (service `computer.meshyard.device`) and
  `FileKeyStore` (0600). Distinguish `NotFound` from `Unavailable/Locked`; on `Unavailable` with an
  existing identity → error, never a replacement key. File store only when explicitly selected.
- Data-dir lock file (exclusive) → second process on the same data dir fails clearly.
- Secret key bytes never in `Debug`/logs.

## Acceptance criteria
- [ ] Golden-bytes tests for all v1 wire types.
- [ ] Signature tests: tampered header/body rejected; wrong domain separator rejected.
- [ ] Store tests: gap → `Pending`, gap filled → contiguous head advances; same seq different
      payload → `Equivocation` (first kept); identical → `Duplicate`.
- [ ] Fold tests: non-owner `MemberAdded` ignored; `AgentState` for an agent owned by another
      author ignored; events of not-yet-member held then applied after `MemberAdded`; property test.
- [ ] Default `SharingPolicy` never emits prompt/command/path text (test).
- [ ] `FileKeyStore` file mode 0600; `Debug` prints no key bytes; lock file prevents 2nd open.
- [ ] Keychain path tested manually on macOS — report honestly (may prompt the user).
- [ ] `npm run check` green; report in `docs/reports/T-002.md`.
