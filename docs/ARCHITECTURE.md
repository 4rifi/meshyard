# Meshyard — Architecture

Status: v1 (2026-11-02). Owner: Claude Code (architect). Changes go through an ADR entry (§10).

## 1. Overview

```
┌──────────────────────── Meshyard desktop app (Tauri 2) ────────────────────────┐
│  WebView (React + TS + PixiJS)                                                 │
│   ├─ Playground (PixiJS scene, driven by a read-only view model)               │
│   ├─ Panels: Onboarding, Peers/Diagnostics, Inspector, Settings                │
│   └─ typed IPC client (generated TS types)                                     │
│                ▲ Tauri events (state snapshots/diffs)   │ Tauri commands        │
│  ──────────────┼────────────────────────────────────────▼───────────────────── │
│  Rust core (crate `meshyard-app` wires these together)                         │
│   ├─ meshyard-core    domain types, events, state machine, sharing policy      │
│   ├─ meshyard-store   SQLite (rusqlite), migrations, event log, peers          │
│   ├─ meshyard-net     iroh Endpoint, modes, pairing, sync protocol, diag       │
│   └─ meshyard-agents  adapter trait, Claude Code + Codex adapters, ingest sock │
└────────────────────────────────────────────────────────────────────────────────┘
        ▲ Unix domain socket (0600)                     ▲ QUIC (iroh), E2EE
        │                                               │
  meshyard-hook (tiny CLI, invoked by                 other Meshyard peers
  Claude Code / Codex hooks, async)                   (direct, or relayed in P2P Auto)

  meshyard-node: headless binary using the same crates (tests, CI, real-device checks)
```

Principles: local-first, no central app backend, control plane (discovery/relay) separated from
application data, every claim about agent activity traceable to a real event.

## 2. Technology decisions

| Area | Choice | Why / alternatives considered |
|---|---|---|
| Desktop shell | **Tauri 2** | Rust core in-process, small binaries, native WKWebView on macOS, capability-based IPC permissions. Electron rejected (size, Node in main process, weaker isolation). |
| Frontend | **React 19 + TypeScript (strict) + Vite** | Team familiarity; panels/forms are classic UI. |
| Playground | **PixiJS v8** | Pure 2D renderer, WebGL/WebGPU, small API surface, MIT. Phaser rejected: full game engine (physics, scenes, loader, input) we don't need, harder to embed into a React-owned layout. Pixi scene is driven imperatively from a view-model store, not via `@pixi/react`, to keep React re-renders out of the frame loop. |
| P2P | **iroh 1.x** (n0) | Dial-by-public-key, QUIC + TLS 1.3 with raw public keys (mutual auth for free), hole punching, relay fallback, multipath, stable wire/API since 1.0 (2026), MIT/Apache-2.0, native Rust → fits Tauri. libp2p rejected (much larger surface, more config for the same result). WebRTC rejected (needs signaling server we'd have to run, data channels in the webview, weaker Rust story). Raw QUIC (quinn) rejected (we'd rebuild NAT traversal). |
| Persistence | **SQLite via `rusqlite`** (bundled) + `rusqlite_migration` | Local, zero-ops, synchronous API wrapped in a dedicated thread. |
| Device key storage | **macOS Keychain** via `keyring` crate; fallback file `0600` in app data dir | Keys never in SQLite or logs. |
| TS types from Rust | **`ts-rs`** | Single source of truth for IPC/event types; generated files are never hand-edited. |
| Agent integration | **Official hooks** of Claude Code and Codex → `meshyard-hook` → Unix socket | Both CLIs expose stable, documented hook events with JSON on stdin and async mode. No transcript scraping in MVP. |
| Logging | `tracing` + redaction layer | Structured; secrets/prompt text never logged. |
| Package manager | **npm** (frontend), **cargo** workspace | Lowest friction; Tauri CLI via `@tauri-apps/cli`. |
| Tests | `cargo test` (unit + in-process multi-endpoint integration), Vitest (frontend), Playwright against Vite build with mocked Tauri IPC (UI E2E) | `tauri-driver` WebDriver is unsupported on macOS, so app-level E2E uses `meshyard-node` peers + mocked IPC; real-device runs are manual and documented. |

All versions are pinned in manifests; Codex verifies current stable versions at scaffold time (T-001)
and records them in §11.

## 3. Repository layout

```
meshyard/
├─ CLAUDE.md, AGENTS.md
├─ docs/                     PRODUCT, ARCHITECTURE, SECURITY, ROADMAP, HANDOFF, tasks/, reviews/
├─ Cargo.toml                workspace
├─ crates/
│  ├─ meshyard-core/         no I/O; pure types + logic (state machine, policy, event schema)
│  ├─ meshyard-store/        SQLite
│  ├─ meshyard-net/          iroh, pairing, sync, diagnostics
│  ├─ meshyard-agents/       adapters, hook ingest server, state inference rules
│  ├─ meshyard-hook/         bin: hook forwarder (stdin JSON → socket), must stay tiny & fast
│  └─ meshyard-node/         bin: headless peer (CLI) for tests and field validation
├─ app/
│  ├─ src-tauri/             crate `meshyard-app` (Tauri commands, event bridge, capabilities)
│  └─ src/                   React + Pixi frontend
│     ├─ ipc/                typed client; `ipc/generated/` from ts-rs (do not edit)
│     ├─ playground/         Pixi scene, sprites, movement, hit-testing
│     ├─ features/           onboarding, peers, inspector, settings
│     └─ state/              view-model store (Zustand)
└─ assets/                   original pixel-art (sprites, tiles) + license notes
```

Dependency direction: `core` ← `store`, `net`, `agents` ← `app`, `node`. `core` has no I/O deps.
`meshyard-hook` depends only on `serde_json` + std (no tokio, no iroh) to keep start-up < 20 ms.

## 4. Identity, workspace, membership

- **Device identity** = iroh `SecretKey` (Ed25519). `EndpointId` (public key) is the device id.
- **User profile** = display name + avatar choice, local; announced to peers on connect, signed
  implicitly by the authenticated QUIC session.
- **Workspace** = `{ workspace_id (random 128-bit), name, mode: Auto|Strict, sharing_policy,
  members: [{endpoint_id, display_name, role: owner|member, paired_at}] }`.
- Membership changes are events in the workspace log (§6), authored by the owner in MVP.
  Multi-admin / revocation semantics are M3.

## 5. Networking (`meshyard-net`)

### 5.1 Endpoint configuration

| Mode | iroh preset / config | Discovery | Data path |
|---|---|---|---|
| **P2P Auto** | `presets::N0` (relay mode default n0 relays, or a custom relay map if configured) | n0 DNS/pkarr: publishes EndpointId → home relay URL | Direct after hole punching; encrypted relay fallback, **UI shows "Relayed via <host>"** |
| **Strict P2P** | `presets::N0DisableRelay` (`RelayMode::Disabled`) | n0 DNS/pkarr publishes EndpointId → **direct IP addresses** (disclosed!) | Direct only. No relay exists in the endpoint, so data cannot be relayed. Fails with a diagnosis if unreachable. |

Relay phases (owner decision D-2, 2026-11-02):
- **Dev prototype:** n0 public relays (`presets::N0`, default relay mode), **non-sensitive test
  data only**. The UI shows a "Development relays (n0)" badge in Auto mode.
- **Before any distribution to external users (release gate G-1):** self-hosted iroh relay(s) in
  Switzerland/EU via `RelayMode::Custom(RelayMap)`, at least two for redundancy, configured with
  minimal logging (no per-connection logs beyond what is needed for abuse/rate limiting).
  The relay URL list ships in app config, not hardcoded in code paths.
- Relays forward encrypted QUIC packets and store nothing; we never put project contents,
  prompts, or agent messages on relay servers in any other form either.

Initial relay path in Auto: iroh may carry the first packets of a connection over the relay
before hole punching upgrades it to a direct path. Consequences:
- Path state per peer starts as `Relayed` and transitions to `Direct`; the UI reflects every
  transition (no optimistic "Direct" label).
- **Prefer-direct window:** after connect, the sync layer waits up to 2 s (configurable) for a
  direct path before starting catch-up of the event log; presence/hello frames may go first. If
  no direct path appears, sync proceeds over the encrypted relay (allowed in Auto) and the UI says
  "Encrypted relay".
- Pairing (`pair/1`) in Auto may complete entirely via relay; it is end-to-end encrypted and
  contains only the token, display name and workspace snapshot metadata.
- In Strict mode none of this applies: there is no relay, so a connection is direct or nothing.

Notes:
- Strict P2P without a relay loses relay-coordinated hole punching; it works when at least one
  side is reachable (public IP, port mapping via UPnP/NAT-PMP/PCP, same LAN, IPv6). That is the
  honest price of the guarantee. A future "Strict + coordination-only relay" variant (relay used
  only until a direct path exists, app streams gated on `Path::is_ip()`, connection closed if the
  selected path ever becomes relay) is tracked as a spike (R-3), not shipped until we can prove no
  application bytes traverse the relay.
- Discovery can be replaced: invite tickets already carry the inviter's addresses, and a
  self-hosted relay/pkarr server can be configured (M2) for users who want zero n0 infra.
- Each mode change rebuilds the endpoint; an endpoint never silently switches mode.

### 5.2 ALPNs (protocol ids)

- `meshyard/pair/1` — pairing handshake, accepted only while an invite is open.
- `meshyard/sync/1` — workspace sync, accepted only from allowlisted EndpointIds.
- `meshyard/ping/1` — diagnostics (RTT, path report), allowlisted only.

Unknown ALPN or non-member EndpointId → connection refused before any app frame is read.

### 5.3 Pairing (invite)

Invite ticket (URL `meshyard://join/<base32>` and QR):
```
{ v:1, workspace_id, workspace_name, inviter: EndpointAddr (id + relay url + direct addrs),
  token: 32 random bytes, expires_at (default 15 min), mode }
```
Flow:
1. Inviter stores `hash(token)`, expiry, single-use flag; opens `pair/1` acceptance.
2. Joiner dials inviter by EndpointAddr on `pair/1`, sends `{token, display_name}`.
3. QUIC/TLS already proved both public keys. Inviter checks token (constant-time compare on
   hash), marks it used.
4. Both sides display a **Short Authentication String** (SAS): 6 words/emoji derived from
   `BLAKE3("meshyard-sas-v1" || sort(id_a, id_b) || token)`. Users compare out-of-band and
   confirm. Mitigates a leaked/intercepted invite link.
5. On confirm: inviter adds joiner to members (membership event), sends the workspace snapshot;
   both persist each other in `peers`. Then normal `sync/1`.

Invite links are secrets: never logged, shown masked, single use, short expiry.

### 5.4 Sync protocol (`sync/1`)

MVP model: **per-device append-only event logs**, replicated full-mesh.

- Each device writes only to its own log: `(author: EndpointId, seq: u64, ts, kind, payload)`.
- On connect: exchange `Hello { workspace_id, protocol_version, heads: {author → max_seq} }`;
  each side streams events the other is missing (bounded batches), then live-pushes new events.
- Frames: length-prefixed (u32 BE) `postcard`-encoded `Frame` enum on a bidirectional QUIC
  stream; max frame 256 KiB; version field in `Hello`, unknown versions rejected with an error
  frame.
- Ephemeral data (presence heartbeats, avatar positions are *not* synced — positions are derived
  locally from state) uses unreliable QUIC datagrams or a separate uni stream, not the log.
- Derived state (current agent states, playground) = deterministic fold over the logs;
  identical on every device that has the same events.
- Log compaction: agent state events older than N hours are snapshotted per agent (M2).
- Upgrade path: for richer shared documents (config, tasks) evaluate Automerge or iroh-docs in M2;
  for > 8 peers evaluate iroh-gossip. Not needed for MVP.

### 5.5 Reconnection & diagnostics

- Supervisor per peer: states `Offline → Connecting → Connected{path: Direct|Relayed, rtt}`
  `→ Degraded → Offline`, exponential backoff 1 s…60 s with jitter, immediate retry on network
  change.
- Path info from `Connection::paths()` / `paths_stream()` (`is_ip`, `is_relay`, `rtt`).
- `Diagnostics` struct exposed to UI: mode, own reachability (home relay, direct addrs count,
  port mapping result), per-peer path/RTT/last error/last seen, plus human-readable advice
  ("Strict P2P: no direct path — both peers are behind NAT without port mapping").

## 6. Event model (`meshyard-core`)

```rust
enum EventKind {
  MemberAdded { endpoint_id, display_name, role },      // owner-authored
  ProfileUpdated { display_name, avatar },
  AgentRegistered { agent_id, kind: AgentKind, label },  // agent_id = hash(device, kind, session)
  AgentState { agent_id, state: ActivityState, provenance: Provenance,
               task_label: Option<String>, tool: Option<ToolCategory> },
  AgentGone { agent_id },
}
enum ActivityState { Idle, Thinking, Building, Reviewing, Testing, Fixing,
                     WaitingForInput, Merging, Finished, Error }
enum Provenance { Verified { source_event: String }, Inferred { rule_id: String } }
```
Every shared payload passes through `SharingPolicy::filter()` before it is written to the log.

## 7. Agent integration (`meshyard-agents`)

### 7.1 Pipeline
```
agent CLI ──hook (async)──► meshyard-hook ──UDS──► IngestServer ──► Adapter::normalize()
   ──► AgentEvent (raw, local only) ──► StateMachine ──► SharingPolicy ──► EventKind::AgentState
```
- `meshyard-hook <agent-kind>` reads stdin JSON (cap 1 MiB), extracts an **allowlisted** subset of
  fields, connects to `<app_data>/ingest.sock` (dir `0700`, socket `0600`), writes one line of
  JSON, exits 0. Socket missing / app not running → exit 0 silently within 50 ms. It never
  blocks or alters the agent (registered with `async: true`; never emits decision output).
- Ingest server verifies peer UID (`getpeereid`) equals the current user.

### 7.2 Adapter trait
```rust
trait AgentAdapter {
  fn kind(&self) -> AgentKind;
  fn capabilities(&self) -> Capabilities;        // which states can be verified vs inferred
  fn normalize(&self, raw: RawHookEvent) -> Vec<AgentSignal>;
  fn install_plan(&self, scope: InstallScope) -> InstallPlan; // exact file + diff, no side effects
  fn apply_install(&self, plan: &InstallPlan) -> Result<()>;   // only after user approval
}
```

### 7.3 State mapping (initial rules; Codex verifies against real payloads in T-007/T-008)

| Signal | Claude Code event | Codex event | State | Provenance |
|---|---|---|---|---|
| session started | `SessionStart` | `SessionStart` | Idle | verified |
| prompt submitted | `UserPromptSubmit` | `UserPromptSubmit` | Thinking | verified |
| file edit tool | `PreToolUse` Edit/Write/MultiEdit | `PreToolUse` apply_patch | Building | inferred (`tool.edit`) |
| test command | `PreToolUse` Bash matching test patterns | `PreToolUse` shell matching | Testing | inferred (`cmd.test`) |
| git merge/rebase | Bash `git merge|rebase|cherry-pick` | same | Merging | inferred (`cmd.git_merge`) |
| review | `/review`-style prompt or read-only tool streak | `codex review` session | Reviewing | inferred (`review.*`) |
| fix after failure | edit following failed test/tool | same | Fixing | inferred (`fix.after_failure`) |
| needs human | `Notification` (permission/idle), `PermissionRequest` | `PermissionRequest` | WaitingForInput | verified |
| tool failed | `PostToolUseFailure` | `PostToolUse` with error | (input to Fixing rule) | — |
| turn finished | `Stop` | `Stop` | Finished → Idle after 60 s | verified |
| model/API failure | `StopFailure` | `PostModelRequest status=failed` | Error | verified |
| session ended | `SessionEnd` | `SessionEnd` | AgentGone | verified |

Pattern matching runs on the **local** command string; the command string itself is never shared.

### 7.4 Installation
- Claude Code: add hook entries to `~/.claude/settings.json` (user scope) or
  `<repo>/.claude/settings.local.json` (project scope). Show the exact JSON diff, require approval,
  back up the original, make it idempotent and removable.
- Codex: add to `~/.codex/hooks.json` (or project `.codex/hooks.json`); tell the user that Codex
  requires trusting new hooks via `/hooks` before they run.

## 8. Frontend

- View-model store (Zustand) fed by Tauri events: `peers`, `agents`, `diagnostics`, `workspace`.
- Playground: Pixi `Application` owned by a React component (mount/unmount only). Scene graph:
  tilemap layer → stations layer → avatars layer → labels/UI overlay. Avatars move with A*/
  waypoint paths on a coarse grid, tweened at fixed px/s; state change = new target station.
- Pixel art: original assets in `assets/` (16×16 / 32×32, nearest-neighbour scaling, integer zoom).
- Accessibility: every playground fact is also available in a list view (screen readers, low-end).

## 9. Testing strategy

| Layer | What | How |
|---|---|---|
| core | state machine, inference rules, policy filter, event fold determinism | unit + property tests (`proptest`) |
| store | migrations, log append/read, heads | tmp-dir SQLite |
| net | pairing (good/expired/reused/wrong token), allowlist rejection, sync catch-up, reconnection | in-process endpoints, `RelayMode::Disabled` + direct addrs ("direct" suite) |
| net relay | relayed path works and is reported as relayed; Strict mode never has a relay path | local `iroh-relay` test server (`test-utils`) ("relay" suite) |
| agents | normalize real captured hook payloads (fixtures recorded from real CLIs, scrubbed) | golden tests |
| hook | latency budget, silent exit without socket | integration test |
| frontend | store reducers, scene logic | Vitest |
| UI E2E | onboarding, inspector, diagnostics rendering | Playwright on Vite build with mocked IPC |
| system | two `meshyard-node` processes pairing + syncing on localhost | scripted test |
| real devices | two Macs, different networks, both modes | manual protocol `docs/VALIDATION.md` (T-012); results recorded honestly |

## 10. ADR log

- **ADR-001** Tauri 2 + React/TS + PixiJS v8 + iroh 1.x + SQLite. (2026-11-02, accepted)
- **ADR-002** Strict P2P = relay disabled at endpoint level, not application-level gating. (accepted; revisit via R-3)
- **ADR-003** Per-device append-only logs instead of CRDT library for MVP. (accepted)
- **ADR-004** Agent observation via official hooks only; transcript parsing out of MVP. (accepted)
- **ADR-005** `meshyard-hook` is a separate minimal binary over a UID-checked Unix socket. (accepted)
- **ADR-006** Relays: n0 public for dev only; self-hosted CH/EU redundant relays required before
  external distribution (gate G-1); 2 s prefer-direct window before log catch-up in Auto. (accepted, owner D-2)

## 11. Pinned versions

Filled in by Codex during T-001 (crate/npm name → exact version, date verified).
