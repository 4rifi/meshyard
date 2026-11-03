# T-002 — Core types, wire v1, local store, device identity

Status: review (audit fixes validated; owner exception: T-001 review; Keychain manual acceptance unverified) · Owner: Codex · Reviewer: Claude
Revised 2026-11-02 after the Codex architecture review; updated for PROTOCOL v1.1 (ADR-009,
ADR-010). **Normative reference: `docs/PROTOCOL.md` v1.1 §§1–5 and §10.** Where this spec is
silent, PROTOCOL.md decides.

## Objective
Domain model, frozen wire format v1, signed events, persistence and identity that every later
layer builds on.

## Scope
### meshyard-core (no I/O dependencies)
- Domain types: `WorkspaceId`, `DeviceId` (32-byte public key; no iroh dependency here),
  `Genesis`, `Workspace`, `Member`, `Mode {Auto, Strict}`, `ActivityState`, `Provenance`
  (`Verified{source: HookEvent}` / `Inferred{rule: RuleId}` — enums, no free text),
  `ToolCategory`, `AgentKind`.
- `wire::v1` (closed once reviewed — no added variants; PROTOCOL §5): `GenesisV1`,
  `SignedGenesisV1 { genesis_bytes, sig }`, `ModeV1` (`Auto` = 0, `Strict` = 1), `EventHeaderV1`,
  `EventBodyV1`, `SignedEventV1 { signed, sig }`, frame header (`MSHY` magic, version, type, len)
  — **separate from domain and IPC types**, with conversion functions. Field order exactly as in
  PROTOCOL §1/§2. Golden-bytes tests for every wire struct and every enum variant (fixed bytes
  checked into tests).
- Exact signed bytes (PROTOCOL v1.1 §1–2):
  - genesis: `genesis_bytes = postcard(GenesisV1)`; signature over
    `b"meshyard-genesis-v1" || genesis_bytes`; `WorkspaceId = BLAKE3(b"meshyard-ws-v1" ||
    genesis_bytes)[..16]`; decoded `owner` must equal the signing key.
  - event: `signed = postcard(EventHeaderV1) || postcard(EventBodyV1)`; signature by
    `header.author` over `b"meshyard-ev-v1" || signed`.
  - Raw received bytes are stored and used for verification and equivocation comparison;
    never re-encode a decoded value for verification.
- `verify_and_decode(SignedEventV1)` in the exact receive order of PROTOCOL §2 (header → verify
  → body with empty remainder → [store: duplicate/equivocation] → field validation).
- Signing/verification via a `Signer`/`Verifier` trait (implementation with `ed25519-dalek` or
  the key type iroh uses — pick one, justify it); domain-separated message per PROTOCOL §2.
- `WorkspaceId` derivation from raw `genesis_bytes` per PROTOCOL §1.
- `fold(genesis, accepted_events) -> DerivedState` implementing PROTOCOL §4: phase 1 membership
  (owner seeded from genesis at epoch 0; only owner-authored `MemberAdded`/`MemberRemoved`;
  `MemberAdded(owner)` is a no-op), phase 2 ordinary events for **every member including the
  owner**, authorization rules per body, agent ownership `(author, agent_local_id)`,
  pending-membership holding. Property test (`proptest`): any cross-author interleaving respecting per-author order
  folds to identical state.
- Strings per PROTOCOL §4a: `sanitize_for_signing()` (strip `Cc`, truncate at a char boundary
  to the byte cap) used only when creating own events/genesis; `validate_received()` rejects
  (never rewrites) invalid UTF-8, over-cap, or `Cc`-containing fields after signature
  verification.
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
  `Rejected(InvalidField)` events are not stored; the author's later events stay `Pending`.
  (Sender-only sync per ADR-009 is enforced in T-006, but the store API must not assume
  third-party authors are served.)
- Dedicated DB thread + channel handle for async callers.

### Identity & process model (PROTOCOL §10)
- `DeviceKeyStore` trait; `KeychainKeyStore` (service `computer.meshyard.device`) and
  `FileKeyStore` (0600). Distinguish `NotFound` from `Unavailable/Locked`; on `Unavailable` with an
  existing identity → error, never a replacement key. File store only when explicitly selected.
- Data-dir lock file (exclusive) → second process on the same data dir fails clearly.
- Secret key bytes never in `Debug`/logs.

## Acceptance criteria
- [x] Golden-bytes tests for `GenesisV1`, `SignedGenesisV1`, `EventHeaderV1`, every
      `EventBodyV1` variant, `ModeV1`, and the frame header.
- [x] Signature tests: tampered header/body rejected; wrong domain separator rejected (genesis
      and event); trailing bytes after body rejected; truncated `signed` rejected; genesis whose
      decoded `owner` ≠ signing key rejected; verification never re-encodes (test with stored raw
      bytes).
- [x] Store tests: gap → `Pending`, gap filled → contiguous head advances; same seq different
      payload → `Equivocation` (first kept); identical → `Duplicate`.
- [x] Fold tests: owner's own `ProfileUpdated`/`AgentRegistered`/`AgentState` applied;
      `MemberAdded(owner)` is a no-op (no epoch change); non-owner `MemberAdded` ignored;
      `AgentState` for an agent owned by another author ignored; events of not-yet-member held
      then applied after `MemberAdded`; property test.
- [x] Receive-validation tests: event with a `Cc` character, over-cap string, or invalid UTF-8
      → `Rejected(InvalidField)`, not stored, later events of that author stay `Pending`;
      sender-side `sanitize_for_signing` output always passes `validate_received`.
- [x] Default `SharingPolicy` never emits prompt/command/path text (test).
- [x] `FileKeyStore` file mode 0600; `Debug` prints no key bytes; lock file prevents 2nd open.
- [ ] Keychain path tested manually on macOS — isolated probe attempted, returned Unavailable;
      successful create/read/delete remains unverified (see `docs/reports/T-002.md`).
- [x] `npm run check` green; report in `docs/reports/T-002.md`.
