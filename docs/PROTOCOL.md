# Meshyard — Protocol Specification v1.1 (normative)

Status: v1.1 (2026-11-02; §10 clarified 2026-11-03 for KQ-005; §9 local ingest clarified as v1.2 on 2026-11-03 for KQ-002/KQ-003/T-007; no signed wire change), owner: Claude (architect). v1.1 adds ADR-009 (sender-only sync,
owner-only invitations) and ADR-010 (exact signed bytes, closed v1, fold of owner events, string
validation stage), resolving Knower questions KQ-001 and KQ-004 (`docs/knowledge/OPEN_QUESTIONS.md`).
v1 resolved the blocker/major findings of the Codex architecture review (`docs/reviews/codex-architecture-2026-11-02.md`). Where this file
and `ARCHITECTURE.md` disagree, **this file wins**.

Keywords MUST / MUST NOT / SHOULD as in RFC 2119.

## 1. Keys and identifiers

- **Device key**: Ed25519 (= iroh `SecretKey`). `DeviceId` = 32-byte public key.
- **Workspace genesis** (v1.1, ADR-010): on creation the owner device builds
  `wire::v1::GenesisV1` with fields in exactly this order:
  `v: u8 = 1`, `owner: [u8; 32]`, `name: String` (≤ 64 bytes UTF-8), `mode: ModeV1`
  (`Auto` = variant 0, `Strict` = variant 1), `created_ms: u64`, `nonce: [u8; 16]`.
  - `genesis_bytes := postcard::to_stdvec(&GenesisV1)`.
  - `SignedGenesisV1 { genesis_bytes: Vec<u8>, sig: [u8; 64] }`, where `sig` is the owner's
    Ed25519 signature over `b"meshyard-genesis-v1" || genesis_bytes`.
  - `WorkspaceId := BLAKE3(b"meshyard-ws-v1" || genesis_bytes)[..16]`.
  - The raw `genesis_bytes` are stored and transmitted as received. Verification and id
    derivation always use those raw bytes; implementations MUST NOT re-encode a decoded
    `GenesisV1` for verification. Decoding MUST consume all bytes (trailing bytes → reject).
  - `owner` in the decoded genesis MUST equal the signing key.
  The owner key is therefore anchored in the workspace id; it cannot be changed in v1.
- Every device holds the `SignedGenesisV1` for each workspace it belongs to and verifies the
  signature and `WorkspaceId` against it.

## 2. Signed events

```
EventHeaderV1 { v: u8 = 1, workspace: [u8; 16], author: [u8; 32], seq: u64 (from 1),
                ts_ms: u64, membership_epoch: u32 }
EventBodyV1   (closed v1 enum, see §4)

SignedEventV1 {
  signed: Vec<u8>,   // = postcard(EventHeaderV1) || postcard(EventBodyV1), concatenated
  sig:    [u8; 64],  // Ed25519 by header.author over  b"meshyard-ev-v1" || signed
}
```
- **Receive order (v1.1, ADR-010)**, all steps MUST run in this order:
  1. Decode only the outer `SignedEventV1` frame.
  2. Decode `EventHeaderV1` from the front of `signed` (`postcard::take_from_bytes`) to learn
     `author`; verify `sig` over `b"meshyard-ev-v1" || signed` with that key. Failure → reject.
  3. Decode `EventBodyV1` from the remainder; the remainder after the body MUST be empty
     (trailing bytes → reject; truncated → reject).
  4. Equivocation/duplicate check by comparing the stored and received `signed` bytes.
  5. Field validation (§4a). Failure → `Rejected(InvalidField)`.
  6. Store the received `SignedEventV1` bytes unchanged.
  Received signed bytes are never modified, normalized or re-encoded at any step.
- Signatures MUST be verified on receipt, independent of who delivered the event.
- **MVP forwarding rule:** a device MUST only accept events whose `author` equals the
  authenticated remote `DeviceId` of the connection (no third-party forwarding). Signatures make
  M2 forwarding possible without a protocol break.
- `(workspace, author, seq)` with different `signed` bytes than the stored ones = **equivocation**:
  reject, keep the first, mark the author `equivocated` in diagnostics, alert the user. It is not an
  idempotent duplicate. Identical payload = duplicate, ignored.
- `ts_ms` is informational (UI) only; it MUST NOT decide ordering.

## 3. Heads and gaps

- `head(author)` = highest `seq` such that **all** `1..=seq` are persisted (contiguous prefix).
- Events with `seq > head + 1` are kept in a bounded per-author pending buffer
  (≤ 1 024 events or 1 MiB, whichever first; overflow → drop the pending buffer and re-request).
  They are not folded until the gap is filled.
- `Hello.heads` always reports contiguous heads.

## 4. Event bodies (v1) and authorization

| Body | Valid only if | Fold effect |
|---|---|---|
| `MemberAdded { device, display_name, role: Member }` | author == genesis.owner | add member (epoch +1) |
| `MemberRemoved { device }` (reserved, M2) | author == genesis.owner | remove member (epoch +1) |
| `ProfileUpdated { display_name ≤ 64 B, avatar: u16 }` | author is a member | set author's profile |
| `AgentRegistered { agent_local_id: [u8;16], kind, label ≤ 64 B }` | author is a member | create agent `(author, agent_local_id)` |
| `AgentState { agent_local_id, state, provenance, tool: Option<ToolCategory>, task_label ≤ 80 B }` | author is a member **and** owns that agent | set state |
| `AgentGone { agent_local_id }` | same | remove agent |

- Agent identity is `(author, agent_local_id)` → only the owning device can change its agents.
  There are no cross-author conflicts on agent state by construction.
- **Fold phase 1 — membership** (v1.1): the genesis seeds `genesis.owner` as a member with role
  `Owner` at epoch 0. Then the owner's `MemberAdded`/`MemberRemoved` events are applied in `seq`
  order; each changes the epoch by +1. `MemberAdded` naming the owner itself is a no-op
  (no epoch change). Membership bodies authored by anyone else are ignored.
- **Fold phase 2 — ordinary events** (v1.1): `ProfileUpdated`, `AgentRegistered`, `AgentState`,
  `AgentGone` are folded for **every member, including the owner**, per author in `seq` order.
  The owner's log is thus read in both phases: membership bodies in phase 1, ordinary bodies in
  phase 2.
- Membership is evaluated first: fold phase 1 completely, then phase 2. Phase-2 events from an
  author who is not (yet) a member at the time of fold are held as *pending-membership* (bounded like §3) and re-evaluated when membership
  changes; they are never applied speculatively.
- `membership_epoch` in a header = the epoch the author had seen when writing. In v1 it is
  recorded only; M2 revocation rejects events with an epoch ≥ removal epoch from removed devices.
- Provenance: `Verified { source: HookEvent (enum, not free text) }` |
  `Inferred { rule: RuleId (enum) }`. No free-text provenance.
- All strings: see §4a (v1.1) — senders sanitize before signing, receivers validate and reject.
### 4a. String fields: sanitize on create, reject on receive (v1.1, ADR-010)

- **Sender side** (before signing, when creating an own event or genesis): remove all Unicode
  general-category `Cc` control characters, then truncate to the byte cap at a UTF-8 character
  boundary. The signed bytes contain only the sanitized value.
- **Receiver side** (after signature verification, before storing; §2 step 5): a string field
  that is not valid UTF-8, exceeds its byte cap, or contains any `Cc` character → the event is
  `Rejected(InvalidField)`; the peer is flagged `protocol_violation` in diagnostics. The receiver
  never sanitizes or rewrites signed data.
- The resulting gap in that author's log is intentional: a conforming sender never produces such
  an event, so later events of that author stay pending (§3) and are not folded.
- UI layers additionally escape all strings at render time.

### 4b. Fold purity

- Fold MUST be a pure function of (genesis, set of accepted events). Property test: any
  interleaving across authors that respects per-author order yields identical state.

## 5. Wire format

- Wire types live in `meshyard-core::wire::v1` and are **separate from domain and IPC types**.
- **v1 is closed** (v1.1, ADR-010): once T-002 is reviewed, no v1 type, field, field order,
  variant, or variant discriminant changes, and **no variants are added** to v1 enums. Any change
  requires `wire_version = 2` types in `wire::v2`, negotiated in `Hello` (both sides list the
  versions they support; the highest common version is used; no common version → error frame
  `UNSUPPORTED_VERSION` and close).
- Encoding: `postcard` 1.x. Enum variant order is part of the format → each wire struct and enum
  variant has a golden-bytes test.
- Deferred to M2 (not v1): storing events whose body variant is unknown, to keep per-author
  contiguity across versions.
- **Framing** on every stream: `magic "MSHY" (4) | wire_version u8 | frame_type u8 | len u32 BE | payload`.
  Version and type are checked **before** decoding the payload. `len ≤ 256 KiB`.
- Read deadlines: handshake frames 10 s, idle stream 120 s (keepalive via ping/datagram 30 s).

## 6. Connection admission (all ALPNs)

Two-stage admission:
1. **Device admission** (before reading any application frame): remote `DeviceId` MUST be a
   member of ≥ 1 local workspace (for `sync/1`, `ping/1`), or there MUST be an open pairing
   session (for `pair/1`). Otherwise close with error code `NOT_ADMITTED`. Note: QUIC may already
   have buffered bytes; the guarantee is "no application frame is decoded or processed".
2. **Workspace admission**: first frame on `sync/1` is `Hello { workspace, protocol_version,
   heads, epoch }`. The remote MUST be a member of *that* workspace; else `NOT_MEMBER` + close.
   A device key gives no access to workspaces it was not paired into.
- 0-RTT data MUST NOT be accepted for any state-changing message (disable 0-RTT on all ALPNs in v1).
- Limits per remote device: ≤ 4 concurrent connections, ≤ 16 streams, ingress ≤ 1 MiB/s burst
  4 MiB, pending buffers per §3.

## 7. Pairing (`pair/1`) — persistent state machine

**Invitation authority (v1.1, ADR-009):** in v1 only `genesis.owner` can create invites, so the
inviter is always the owner. Non-owner members cannot invite (the UI hides the action, the node
refuses). Delegated invitations are M2.

Invite ticket `meshyard://join/<base32(postcard(Ticket))>`:
`Ticket { v:1, genesis: SignedGenesisV1, inviter: EndpointAddr, invite_id: [u8;16], token: [u8;32], expires_ms }`.
The joiner MUST verify the genesis signature and that `inviter` id == `genesis.owner`.
Tickets MUST NOT appear in logs, error messages, CLI argument lists (CLI reads from stdin or a
file), crash reports, or telemetry. Default expiry 15 min, single use.

Inviter-side states, persisted in `pairings` table:
```
Open ──(valid token from joiner J, atomically reserve invite_id → J)──► Reserved(J)
Reserved(J) ──(both confirmations)──► Completed  (MemberAdded(J) written, J allowlisted)
Reserved(J) ──(timeout 5 min | reject | disconnect without resume)──► Aborted (invite consumed)
Open ──(expiry)──► Expired
```
- A second joiner presenting the same token while `Reserved` or later → `INVITE_USED`.
- Messages: `JoinRequest { invite_id, token, joiner_profile, joiner_nonce: [u8;16] }` →
  `JoinChallenge { inviter_nonce: [u8;16] }` → both compute SAS → UI confirm →
  `Confirm { transcript_hash }` from **both** sides (order-independent) → inviter sends
  `Welcome { genesis: SignedGenesisV1 }` → done. Membership and allowlist become active only
  after `Completed` is persisted.
- (v1.1, ADR-009) `Welcome` carries **no** membership or other events. `MemberAdded(J)` and the
  rest of the owner's log reach J through a normal `sync/1` session right after pairing: this is
  the owner sending its own log, consistent with §8. J admits the owner because the owner is a
  member by genesis (§4 phase 1); the owner admits J once `Completed` is persisted.
- **Transcript**: `T = BLAKE3("meshyard-pair-v1" || workspace_id || mode || protocol_version ||
  inviter_id || joiner_id || invite_id || inviter_nonce || joiner_nonce)`.
  `Confirm.transcript_hash` MUST equal the local `T` (detects any mismatch).
- **SAS** = first 48 bits of `BLAKE3("meshyard-sas-v1" || T)` rendered as **4 words** from a fixed
  4 096-word list (12 bits/word) committed in `meshyard-core` (or 6 words from 256 if the list
  isn't ready; document which). Both users compare and press "Matches"; a mismatch aborts.
- Crash/disconnect while `Reserved`: joiner may reconnect within the 5 min window with the same
  `(invite_id, joiner_id, joiner_nonce)` to resume; anything else aborts. Re-invite = new ticket.
- **Mode**: the ticket carries the workspace mode. If the joiner's local default is Strict and
  the workspace is Auto, the joiner UI MUST require explicit consent before joining. A link can
  never silently weaken the local mode.
- `pair/1` limits: ≤ 2 concurrent pairing connections, 10 attempts/min per invite, 30 s per
  message deadline, 16 KiB max message.

## 8. Sync (`sync/1`)

1. Exchange `Hello` (both directions).
2. **Cursor-based catch-up, sender-only** (v1.1, ADR-009): each side streams **only its own
   log**: `events_after(self, remote_head[self])` from the **store** in batches
   (≤ 256 events / 256 KiB). `Hello.heads` may list other authors for diagnostics only; those
   events are never requested from or served to anyone but their author (third-party forwarding
   is M2). Consequence: an offline author's events are unavailable until that author connects.
3. **Live phase** reuses the same cursor: a local "new event" notification only *wakes* the
   per-connection sender, which reads from the store after its cursor. No separate in-memory push
   path → no gap between catch-up and live; receivers dedupe by `(author, seq)`.
4. Per-connection send queue ≤ 1 024 events; on overflow the sender drops the queue and resumes
   from the store cursor.
5. Auto mode: catch-up (step 2) starts after the selected path is direct
   (`is_selected() && is_ip()`) or after the 2 s prefer-direct window (ADR-006); no wait if the
   connection is already direct.

## 9. Local agent ingest

v1.2 clarification (2026-11-03, KQ-002/KQ-003, T-007): local ingest only; the signed wire v1 is
unchanged and frozen. Everything below is local to one machine and never signed or shared.

- **Invocation:** `meshyard-hook <claude-code|codex> --socket <absolute path>` (the installer writes
  the path; missing/relative path → exit 0 silently). Dependencies: std + serde/serde_json, plus
  the workspace's already-pinned `rustix` with the `process` feature for the effective UID only.
- **Budget (KQ-002):** one end-to-end budget of 150 ms from process start: stdin ≤ 100 ms with a
  **1 MiB** cap (over cap, timeout or invalid JSON → drop), connect ≤ 20 ms, write ≤ 20 ms. Any
  failure: drop the event, print nothing, exit 0. Empty stdin exits immediately (first launch
  < 20 ms, T-001 criterion unchanged).
- **Forwarded events:** only the seven `HookEvent` names of `meshyard-core` (`SessionStart`,
  `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `Stop`, `StopFailure`, `SessionEnd`). Any other
  hook name → exit 0 without connecting.
- **Socket owner check (hook side):** `symlink_metadata` on the socket path must be a socket (not
  a symlink); its parent directory (`symlink_metadata`, not a symlink) must be a real directory
  with mode bits exactly `0700`; **both** the socket's and the parent's owner UID must equal
  `rustix::process::geteuid().as_raw()` (effective UID, not the real UID). Any mismatch → exit 0
  without writing. What this proves: at check time the socket and its parent directory are owned
  by the hook's effective user. It does **not** prove ancestor-directory trust, absence of ACL
  grants, freedom from check-then-connect races (TOCTOU), or the identity of the process
  listening on the socket; no root or same-user race resistance is promised. Mode bits alone do
  not exclude ACL access, which is why the UID comparison is direct.
- **Server side (still required):** the app creates the socket directory with the existing
  ACL-aware `meshyard-store` `private_directory` check and checks the connecting peer's UID (safe
  wrapper, no `unsafe` in our code).
  This protects against **other OS users only**, not other processes of the same user; "verified"
  means "observed via a hook-shaped message on the local socket".
- **Command classification happens in the hook, at the earliest boundary.** For shell tools
  (Claude Code `Bash`; Codex `Bash`, also for `exec_command`) the hook reads `tool_input.command`
  (first 4 KiB only) in memory, computes `command_class`, and drops the string. Raw commands never
  leave the hook process: not sent, persisted, logged or shared. Rule: split on `&&`, `||`, `;`,
  `|`; per segment skip leading `cd …`, `env`, `VAR=value` tokens; the first program token (+ its
  subcommand) decides:
  - `git_merge`: `git merge`, `git rebase`, `git cherry-pick`, `git pull --rebase`.
  - `test`: `cargo test`, `cargo nextest`, `npm|pnpm|yarn|bun test`, `npm|pnpm|yarn|bun run test`,
    `npx vitest|jest`, `vitest`, `jest`, `pytest`, `go test`, `swift test`, `xcodebuild test`,
    `make test`.
  - precedence `git_merge` > `test` > `other`. Non-shell tools: field absent.
- **Envelope:** exactly one newline-terminated JSON object, ≤ 1 KiB:
  `{"v":1,"agent":"claude_code"|"codex","event":<HookEvent name>,"session_id":S,
  "turn_id":S?,"tool_use_id":S?,"agent_id":S?,"tool":"edit"|"shell"|"read"|"web"|"other"?,
  "command_class":"test"|"git_merge"|"other"?}`. Correlation strings: 1–128 bytes of
  `[A-Za-z0-9._:-]`; an invalid optional ID is omitted, an invalid/missing `session_id` drops the
  event. Tool mapping (Claude Code): Edit/Write/MultiEdit/NotebookEdit → edit, Bash → shell,
  Read/Grep/Glob/LS → read, WebFetch/WebSearch → web, anything else → other. No prompt, tool input
  other than the classified command, output, file content, path, cwd or transcript path is sent.
  Tool outcome / request status enums (KQ-003) are **not** in v1 of the envelope; they are added
  only with the rule that consumes them (Fixing, Codex error), under the KQ-003 evidence rule.
- **State mapping within frozen wire v1** (`ActivityState`, `Provenance`, `RuleId` unchanged):

  | Input | State | Provenance |
  |---|---|---|
  | SessionStart | Idle | Verified{SessionStart} |
  | UserPromptSubmit | Thinking | Verified{UserPromptSubmit} |
  | PreToolUse tool=edit | Building | Inferred{ToolEdit} |
  | PreToolUse shell + test | Testing | Inferred{ToolShell} |
  | PreToolUse shell + git_merge | Merging | Inferred{ToolShell} |
  | PreToolUse shell + other | Building | Inferred{ToolShell} |
  | PreToolUse read / web | Thinking | Inferred{ToolRead} / Inferred{ToolWeb} |
  | PreToolUse other, PostToolUse | no change | — |
  | Stop | Finished | Verified{Stop} |
  | 60 s after Finished, no newer event | Idle | Inferred{TurnEnded} |
  | StopFailure | Error | Verified{StopFailure} |
  | SessionEnd | Idle (UI: "Session ended") | Verified{SessionEnd} |

  `Reviewing`, `Fixing` and `WaitingForInput` are **not produced** in the MVP: frozen v1 has no
  `HookEvent`/`RuleId` to label them honestly (needs wire v2).
- **Correlation and ordering:** state is tracked per `session_id` (≤ 32 sessions per agent, LRU).
  A turn starts only with `UserPromptSubmit`: in Idle/Finished/Error only `SessionStart`,
  `UserPromptSubmit`, `SessionEnd` are applied; late tool events are ignored. If `turn_id` is
  present, events of an already ended turn are ignored (≤ 16 ended turn ids per session). A
  repeated `(event, tool_use_id, agent_id)` is ignored as a duplicate (≤ 64 per session). Subagent
  events (`agent_id` present) count as activity of the same agent. The agent's state is the state
  of its most recently updated session. T-007b maps adapter kind → the registered agent of that
  kind (T-014) for `agent_local_id`.
- **Staleness is local derived metadata, not a wire state and not an emitted event:** every viewer
  (own and remote) marks an agent `stale` if its last `AgentState` is not Idle/Finished and is
  older than **10 min** (local receive time). The UI shows "Unknown (no events for 10 min)" with
  the last known state dimmed. `RuleId::Stale` stays reserved and is not emitted in the MVP.
  This is separate from the Finished → Idle 60 s rule.
- Codex specifics (shell matcher `Bash` also for `exec_command`, failed `PostModelRequest`
  handling) are T-008 and follow the same rules.

## 10. Process model

- One running Meshyard process (app or `meshyard-node`) per data directory, enforced by an
  exclusive lock file; second instance fails with a clear error.
- Data directory is selectable (`--data-dir` / `MESHYARD_DATA_DIR`) for isolated test profiles.
- (KQ-005) A process has exactly one active workspace and one endpoint in the MVP. The mode is
  bound to the genesis; there is no mode-change message. Strict endpoints use no relay and no
  third-party address lookup (ADR-011), so `Ticket` addresses MUST include every current direct
  address (local interfaces + port-mapped external address, if any).
- Identity: if a device key exists but the Keychain is locked/unavailable, MUST fail with a clear
  error, never generate a replacement identity. File key store only when explicitly selected
  (`--key-store file`) or in tests.
