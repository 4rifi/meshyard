# Meshyard — Protocol Specification v1 (normative)

Status: v1 draft (2026-11-02), owner: Claude (architect). Resolves the blocker/major findings of
the Codex architecture review (`docs/reviews/codex-architecture-2026-11-02.md`). Where this file
and `ARCHITECTURE.md` disagree, **this file wins**.

Keywords MUST / MUST NOT / SHOULD as in RFC 2119.

## 1. Keys and identifiers

- **Device key**: Ed25519 (= iroh `SecretKey`). `DeviceId` = 32-byte public key.
- **Workspace genesis**: on creation the owner device writes a `Genesis` record
  `{ v:1, owner: DeviceId, name, mode, created_ms, nonce: [u8;16] }`, signed by the owner.
  `WorkspaceId = BLAKE3("meshyard-ws-v1" || canonical(Genesis))[..16]`.
  The owner key is therefore anchored in the workspace id; it cannot be changed in v1.
- Every device holds the genesis record for each workspace it belongs to and verifies
  `WorkspaceId` against it.

## 2. Signed events

```
SignedEvent {
  header: EventHeader { v: u8 = 1, workspace: WorkspaceId, author: DeviceId, seq: u64 (from 1),
                        ts_ms: u64, membership_epoch: u32 },
  body:   EventBody (v1 enum, see §4),
  sig:    Ed25519 signature over  "meshyard-ev-v1" || wire(header) || wire(body)
}
```
- Signatures MUST be verified on receipt, independent of who delivered the event.
- **MVP forwarding rule:** a device MUST only accept events whose `author` equals the
  authenticated remote `DeviceId` of the connection (no third-party forwarding). Signatures make
  M2 forwarding possible without a protocol break.
- `(workspace, author, seq)` with a different signed payload than the stored one = **equivocation**:
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
- **Membership is evaluated first**: fold the owner's log (membership events only), then fold
  every other author's log in `seq` order. Events from an author who is not (yet) a member at the
  time of fold are held as *pending-membership* (bounded like §3) and re-evaluated when membership
  changes; they are never applied speculatively.
- `membership_epoch` in a header = the epoch the author had seen when writing. In v1 it is
  recorded only; M2 revocation rejects events with an epoch ≥ removal epoch from removed devices.
- Provenance: `Verified { source: HookEvent (enum, not free text) }` |
  `Inferred { rule: RuleId (enum) }`. No free-text provenance.
- All strings: UTF-8, length-capped as above, control characters stripped.
- Fold MUST be a pure function of (genesis, set of accepted events). Property test: any
  interleaving across authors that respects per-author order yields identical state.

## 5. Wire format

- Wire types live in `meshyard-core::wire::v1` and are **separate from domain and IPC types**.
  v1 types are frozen once T-002 is reviewed; changes require `v2` types.
- Encoding: `postcard` 1.x. Enum variant order is part of the format → each wire enum has a golden
  bytes test; adding a variant only at the end; never reorder or remove.
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

Invite ticket `meshyard://join/<base32(postcard(Ticket))>`:
`Ticket { v:1, genesis (signed), inviter: EndpointAddr, invite_id: [u8;16], token: [u8;32], expires_ms }`.
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
  `Welcome { genesis, member_events_so_far }` → done. Membership and allowlist become active only
  after `Completed` is persisted.
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
2. **Cursor-based catch-up**: for each author A in the union of heads, the side with more events
   streams `events_after(A, remote_head[A])` from the **store** in batches (≤ 256 events / 256 KiB).
3. **Live phase** reuses the same cursor: a local "new event" notification only *wakes* the
   per-connection sender, which reads from the store after its cursor. No separate in-memory push
   path → no gap between catch-up and live; receivers dedupe by `(author, seq)`.
4. Per-connection send queue ≤ 1 024 events; on overflow the sender drops the queue and resumes
   from the store cursor.
5. Auto mode: catch-up (step 2) starts after the selected path is direct
   (`is_selected() && is_ip()`) or after the 2 s prefer-direct window (ADR-006); no wait if the
   connection is already direct.

## 9. Local agent ingest

- `meshyard-hook <claude-code|codex>`: reads stdin with a 1 MiB cap and 200 ms deadline,
  extracts allowlisted fields (`hook_event_name`, `session_id`, `turn_id`, `tool_use_id`,
  `agent_id`, `tool_name`, and for shell tools `command` truncated to 4 KiB), connects to the
  socket with a 50 ms connect deadline and 50 ms write deadline, writes one JSON line, exits 0.
  Total budget ≤ 150 ms on the failure path. Prompts, tool outputs, file contents, transcript
  paths are never forwarded.
- Before writing, the hook checks that the socket file is owned by the current UID.
- The app checks peer UID (safe wrapper, e.g. `nix` `LocalPeerCred` on macOS; no `unsafe` in our
  code). This protects against **other OS users only**, not against other processes of the same
  user; "verified" therefore means "observed via a hook-shaped message on the local socket".
- The adapter classifies `command` in memory into fixed categories (`RuleId`), then drops it.
  Raw command strings are never persisted, logged, or shared.
- Async hooks may arrive out of order or not at all: the state machine correlates by
  `session_id` / `turn_id` / `tool_use_id`, ignores stale events (older turn), and marks an agent
  `stale` (UI: "unknown") if no event arrives for 10 min while not Idle/Finished.
- `Stop` = turn ended (→ Finished, then Idle after 60 s). `PostModelRequest status=failed` is not
  Error by itself (a retry may follow); Error only if the turn then ends without success
  (`StopFailure` for Claude Code; Codex: failed request followed by `Stop` without a successful request).
- Codex shell tool matcher name is `Bash` (also for `exec_command`).

## 10. Process model

- One running Meshyard process (app or `meshyard-node`) per data directory, enforced by an
  exclusive lock file; second instance fails with a clear error.
- Data directory is selectable (`--data-dir` / `MESHYARD_DATA_DIR`) for isolated test profiles.
- Identity: if a device key exists but the Keychain is locked/unavailable, MUST fail with a clear
  error, never generate a replacement identity. File key store only when explicitly selected
  (`--key-store file`) or in tests.
