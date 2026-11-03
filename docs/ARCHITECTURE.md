# Meshyard — Architecture

Status: v1 (2026-11-02). Owner: Claude Code (architect). Changes go through an ADR entry (§10).
**Wire formats, authorization, pairing, sync and ingest are specified normatively in
`docs/PROTOCOL.md`; it overrides the summaries in §§4–7 below.**

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
  Revocation is M2 (epochs reserved in PROTOCOL v1); multi-admin is later.

## 5. Networking (`meshyard-net`)

### 5.1 Endpoint configuration

| Mode | iroh preset / config | Discovery | Data path |
|---|---|---|---|
| **P2P Auto** | `presets::N0` (relay mode default n0 relays, or a custom relay map if configured) | n0 DNS/pkarr: publishes EndpointId → home relay URL | Direct after hole punching; encrypted relay fallback, **UI shows "Relayed via <host>"** |
| **Strict P2P** (ADR-011) | `RelayMode::Disabled`, **no n0 address lookup or publishing** (no DNS/pkarr), port mapping (UPnP/NAT-PMP/PCP) enabled | None via third parties: peer addresses come from the invite ticket and are refreshed over the authenticated connection; LAN discovery only if explicitly added later | Direct only. No relay exists in the endpoint, so data cannot be relayed. Fails with a diagnosis if unreachable. |

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
- **Endpoint lifecycle (KQ-005, decided 2026-11-03):** in the MVP a process (app or
  `meshyard-node`) has exactly **one active workspace** and one endpoint. The workspace mode is
  fixed by its genesis (it is part of the WorkspaceId); changing mode means creating a new
  workspace, never mutating an existing one. Multiple workspaces per process are M2.
- **Tester builds (owner decision 2026-11-03):** the first build that leaves Dion's Mac is
  Strict-only: Auto is compiled out or disabled (shown as "requires a self-hosted relay, G-1"),
  no relay URL and no n0 DNS/pkarr host is compiled in, and a build check greps the bundle for
  them. Auto stays in the architecture and is tested against a local relay only.

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
- Path info from `Connection::paths()` / `paths_stream()`; status is based on the **selected**
  path (`is_selected() && is_ip()` = Direct). Verified on docs.rs for iroh 1.0.0 by Codex review.
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
| test command | `PreToolUse` Bash matching test patterns | `PreToolUse` `Bash` (also `exec_command`) matching | Testing | inferred (`cmd.test`) |
| git merge/rebase | Bash `git merge|rebase|cherry-pick` | same | Merging | inferred (`cmd.git_merge`) |
| review | read-only tool streak / review subagent type (prompt text is never read) | `codex review` session | Reviewing | inferred (`review.*`) |
| fix after failure | edit following failed test/tool | same | Fixing | inferred (`fix.after_failure`) |
| needs human | `Notification` (permission/idle), `PermissionRequest` | `PermissionRequest` | WaitingForInput | verified |
| tool failed | `PostToolUseFailure` | `PostToolUse` with error | (input to Fixing rule) | — |
| turn finished | `Stop` | `Stop` | Finished → Idle after 60 s | verified |
| model/API failure | `StopFailure` | failed `PostModelRequest` followed by `Stop` without success (a single failure may be retried) | Error | verified / inferred (Codex) |
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
- **ADR-007** Signed per-device events, owner key anchored in WorkspaceId, no third-party forwarding in MVP, contiguous heads, equivocation detection, frozen wire v1 separate from domain types. (accepted, from Codex review)
- **ADR-008** Two-stage admission (device, then workspace) and persistent pairing state machine with transcript-bound SAS. (accepted, from Codex review)
- **ADR-009** MVP sync is sender-only (each peer serves only its own log); only the genesis owner can invite; `Welcome` carries only the signed genesis and membership arrives via normal sync. Third-party forwarding and delegated invites are M2. (accepted 2026-11-02, resolves KQ-001; PROTOCOL v1.1 §§7–8)
- **ADR-010** Exact signed bytes: `SignedGenesisV1` over raw `genesis_bytes`, `SignedEventV1` over `postcard(header) || postcard(body)`; verify before decoding the body, never re-encode or mutate received bytes; receivers reject invalid strings after signature verification (senders sanitize before signing); owner is seeded by genesis and its ordinary events fold like any member's; wire v1 is closed (no added variants, v2 via Hello negotiation). (accepted 2026-11-02, resolves KQ-004; PROTOCOL v1.1 §§1, 2, 4, 4a, 5)
- **ADR-006** Relays: n0 public for dev only; self-hosted CH/EU redundant relays required before
  external distribution (gate G-1); 2 s prefer-direct window before log catch-up in Auto. (accepted, owner D-2)
- **ADR-011** Strict P2P uses no n0 service at all: `RelayMode::Disabled` **and** no address
  lookup/publishing (no DNS/pkarr), so a Strict device never discloses its IP addresses to n0.
  Addresses come from the invite ticket and are exchanged over the authenticated connection;
  iroh port mapping is enabled and its result is shown in diagnostics. One active workspace per
  process, mode fixed by genesis (KQ-005). Tester builds are Strict-only (G-1 addendum).
  (accepted 2026-11-03, owner chose Strict-only for the colleague test)

## 11. Pinned versions

Updated by Codex, pending review (T-001, verified 2026-11-02 UTC).

Selection policy: newest compatible non-prerelease, non-deprecated/non-yanked registry release
published before 2026-10-19 UTC (at least two weeks old). TypeScript stays below 6.1 to satisfy
the pinned typescript-eslint peer range. Newer stable releases are listed separately so
these pins are not mistaken for the latest published version. Exact direct versions are pinned;
transitive resolution is recorded by generated `Cargo.lock` and `package-lock.json`.

| Registry / package | Pinned version | Published (UTC) | Latest stable at verification | Use |
|---|---|---|---|---|
| [@tauri-apps/cli](https://registry.npmjs.org/@tauri-apps/cli) (npm) | `2.12.3` | 2026-10-14 | `2.12.4` | Active scaffold dependency |
| [@tauri-apps/api](https://registry.npmjs.org/@tauri-apps/api) (npm) | `2.12.4` | 2026-10-14 | `2.12.4` | Active scaffold dependency |
| [react](https://registry.npmjs.org/react) (npm) | `19.3.1` | 2026-10-14 | `19.3.1` | Active scaffold dependency |
| [react-dom](https://registry.npmjs.org/react-dom) (npm) | `19.3.1` | 2026-10-14 | `19.3.1` | Active scaffold dependency |
| [vite](https://registry.npmjs.org/vite) (npm) | `8.3.4` | 2026-10-08 | `8.3.5` | Active scaffold dependency |
| [@vitejs/plugin-react](https://registry.npmjs.org/@vitejs/plugin-react) (npm) | `6.1.2` | 2026-10-05 | `6.2.1` | Active scaffold dependency |
| [typescript](https://registry.npmjs.org/typescript) (npm) | `6.0.3` | 2026-04-16 | `7.0.2` | Active; latest compatible with typescript-eslint (<6.1) |
| [pixi.js](https://registry.npmjs.org/pixi.js) (npm) | `8.22.0` | 2026-10-01 | `8.22.0` | Active scaffold dependency |
| [eslint](https://registry.npmjs.org/eslint) (npm) | `10.13.0` | 2026-10-16 | `10.14.0` | Active scaffold dependency |
| [@eslint/js](https://registry.npmjs.org/@eslint/js) (npm) | `10.0.1` | 2026-02-06 | `10.0.1` | Active scaffold dependency |
| [typescript-eslint](https://registry.npmjs.org/typescript-eslint) (npm) | `8.71.2` | 2026-10-12 | `8.72.1` | Active scaffold dependency |
| [eslint-plugin-react-hooks](https://registry.npmjs.org/eslint-plugin-react-hooks) (npm) | `7.1.1` | 2026-04-17 | `7.1.1` | Active scaffold dependency |
| [eslint-plugin-react-refresh](https://registry.npmjs.org/eslint-plugin-react-refresh) (npm) | `0.5.7` | 2026-09-14 | `0.5.7` | Active scaffold dependency |
| [globals](https://registry.npmjs.org/globals) (npm) | `17.13.0` | 2026-10-01 | `17.14.0` | Active scaffold dependency |
| [prettier](https://registry.npmjs.org/prettier) (npm) | `3.9.10` | 2026-10-10 | `3.9.10` | Active scaffold dependency |
| [vitest](https://registry.npmjs.org/vitest) (npm) | `5.0.5` | 2026-10-15 | `5.1.0` | Active scaffold dependency |
| [@types/react](https://registry.npmjs.org/@types/react) (npm) | `19.3.0` | 2026-09-09 | `19.3.0` | Active scaffold dependency |
| [@types/react-dom](https://registry.npmjs.org/@types/react-dom) (npm) | `19.3.0` | 2026-09-09 | `19.3.0` | Active scaffold dependency |
| [@types/node](https://registry.npmjs.org/@types/node) (npm) | `26.6.5` | 2026-10-09 | `26.6.6` | Active scaffold dependency |
| [tauri](https://crates.io/api/v1/crates/tauri) (crate) | `2.12.4` | 2026-10-14 | `2.12.5` | Active scaffold dependency |
| [tauri-build](https://crates.io/api/v1/crates/tauri-build) (crate) | `2.7.3` | 2026-10-14 | `2.7.4` | Active scaffold dependency |
| [iroh](https://crates.io/api/v1/crates/iroh) (crate) | `1.3.0` | 2026-09-28 | `1.4.1` | Active scaffold dependency |
| [rusqlite](https://crates.io/api/v1/crates/rusqlite) (crate) | `0.40.2` | 2026-08-08 | `0.41.1` | Reserved workspace pin; not activated |
| [tokio](https://crates.io/api/v1/crates/tokio) (crate) | `1.53.3` | 2026-10-13 | `1.54.1` | Reserved workspace pin; not activated |
| [tracing](https://crates.io/api/v1/crates/tracing) (crate) | `0.1.44` | 2025-12-18 | `0.1.44` | Active scaffold dependency |
| [ts-rs](https://crates.io/api/v1/crates/ts-rs) (crate) | `12.0.1` | 2026-01-31 | `12.0.1` | Reserved workspace pin; not activated |
| [serde](https://crates.io/api/v1/crates/serde) (crate) | `1.0.229` | 2026-07-18 | `1.0.229` | Reserved workspace pin; not activated |
| [serde_json](https://crates.io/api/v1/crates/serde_json) (crate) | `1.0.154` | 2026-10-11 | `1.0.154` | Reserved workspace pin; not activated |
| [thiserror](https://crates.io/api/v1/crates/thiserror) (crate) | `2.0.21` | 2026-09-23 | `2.0.21` | Reserved workspace pin; not activated |

Rust stable is pinned to `1.99.1` (compiler commit date 2026-10-13, rustup release metadata
2026-10-15, verified locally). Edition 2024 and workspace resolver 2 are used. `meshyard-hook`
has **no dependencies**; serde is reserved until T-007. SQLite, async runtime, error derives,
and IPC generation are intentionally not activated merely to exercise future dependencies.

`iroh = 1.3.0` is configured with default features disabled and `tls-ring` enabled; the scaffold only
re-exports `EndpointId` and never constructs an endpoint. Tauri uses `wry` and `compression`
without default features, plugins, or IPC permissions. The JS API dependency is pinned as
specified, but no IPC is called yet. Root npm scripts own frontend checks and Tauri CLI invocation.

Runtime security configuration: production CSP denies connections, frames, workers, objects,
and forms; scripts/styles/images/fonts are local only. Dev-only CSP adds loopback Vite HMR
and inline Vite React refresh scripts. Updated by Codex, pending review (2026-11-03):
`pixi.js/unsafe-eval` installs static shader/uniform/UBO/particle polyfills before renderer
initialization; despite its name, it removes the need for dynamic evaluation. Production CSP
continues to deny `unsafe-eval`. See the [upstream CSP setup](https://github.com/pixijs/pixijs/blob/dev/skills/pixijs-environments/SKILL.md#csp-compliant-setup).
Pixi draws once without a ticker, external assets, or
network requests. Async disposal is guarded across React StrictMode mount/unmount races.

Primary implementation references: [Tauri CSP](https://v2.tauri.app/security/csp/),
[Tauri configuration](https://v2.tauri.app/reference/config/),
[Pixi Application lifecycle](https://pixijs.com/8.x/guides/components/application).

### T-002 additions — updated by Codex, pending review (2026-11-03)

The T-001 table above records scaffold-time activation. T-002 activates the existing exact
`rusqlite 0.40.2` (bundled), `tokio 1.53.3` (sync/rt/macros), `serde 1.0.229`, `ts-rs 12.0.1`
and `thiserror 2.0.21` pins without changing their versions. New exact direct pins:

| Crate | Pin | Purpose / compatibility |
|---|---|---|
| postcard | 1.1.3 | Frozen v1 codec; default features disabled, use-std only |
| serde-big-array | 0.5.1 | Fixed `[u8; 64]` signatures, without a vector-length prefix |
| ed25519-dalek | 3.0.0 | Pure Signer/Verifier implementation, strict verification, zeroizing signing key; same version used by pinned iroh, with no iroh dependency in core |
| blake3 | 1.8.7 | Raw-genesis WorkspaceId derivation |
| rusqlite_migration | 2.6.0 | Requires rusqlite ^0.40.0; matches existing =0.40.2; MSRV 1.95 fits 1.99.1 |
| keyring | 3.6.3 | Explicit apple-native backend on macOS; established 3.x platform API avoids the 4.x facade's default platform-store bundle |
| security-framework | 3.7.0 | Same backend as keyring: safe create-only Keychain insertion instead of upsert, OS status classification, prompt-disabled isolated manual probe |
| libc | 0.2.191 | O_NOFOLLOW constant only, no unsafe calls; already resolved in scaffold |
| getrandom | 0.4.3 | OS entropy in identity layer, never in pure core |
| zeroize | 1.9.1 | Zeroizing identity load/generation buffers; same version as dalek's resolved dependency |
| tempfile | 3.27.0 | Dev-only isolated directories, explicitly configured 0700 |
| proptest | 1.12.0 | Dev-only, std-only, 256 cases per property; no fork/timeout features |

Authoritative registry version, publication, MSRV, feature and dependency evidence is recorded in
`docs/reports/T002-dependencies.json`; each record includes its crates.io API source. Every pin is
non-yanked and predates the scaffold's 2026-10-19 cutoff. See the report for selection rationale.
Generated `Cargo.lock` records resolution; it was updated by Cargo only. No npm version changes.

Implementation choices pending review:

- Wire v1 owns every nested enum as well as the six body variants. Activity states follow §6;
  agent kinds are ClaudeCode/Codex/Other, tool categories Edit/Shell/Read/Web/Other. Hook sources
  are SessionStart/UserPromptSubmit/PreToolUse/PostToolUse/Stop/StopFailure/SessionEnd; inference
  rules are ToolEdit/ToolShell/ToolRead/ToolWeb/TurnEnded/Stale. Membership wire role is only
  Member (tag 0); Owner exists in the domain and is seeded by genesis. GoldenBytes pin all tags.
  Claude should review this initial vocabulary before closing v1; future additions require v2.
- Frame headers are fixed ten-byte network headers, not postcard structs. A caller-provided
  ALPN-specific type allowlist is checked before payload decoding; T-002 does not invent
  numeric assignments for future pairing/sync message types.
- `verify_and_decode` returns `VerifiedEvent`, retaining header version separately from the
  domain event. `validate_received` runs after database duplicate/equivocation comparison;
  `into_event` is for already accepted projection inputs. Even the received outer postcard
  envelope is retained unchanged. Postcard-compatible redundant integer encodings remain raw;
  signatures, identity derivation and dedupe never use re-encoded values.
- SQLite sequences are limited to `1..=i64::MAX`. Larger wire sequences return a typed rejection;
  cursor overflow is a typed error. All conversions and next-sequence arithmetic are checked.
  The single DB thread serializes jobs over a bounded 64-entry async channel. Transactions
  provide atomic own allocation/insert and remote insertion/promotion. `open` is synchronous;
  async callers use `open_async`. Explicit `close` releases the lock before returning.
- `append_own` records the locally observed membership epoch and current wall-clock timestamp;
  `append_own_with` allows explicit policy/time/epoch context. Neither timestamp decides order.
- Directory policy after the independent T-002 audit: resolve once to a canonical absolute
  path, validate its chain from root down, and use only that spelling for every later identity,
  lock and SQLite operation. Every ancestor must belong to root or the actual effective UID
  (`rustix::process::geteuid`) and lack group/other write permission, except root-owned sticky
  directories (including canonical `/private/tmp`). The final directory must belong to the
  effective UID and be 0700. Root-owned sticky parents cannot replace another owner's entry;
  directories owned by other UIDs are rejected even if currently 0700. macOS nonempty allow
  ACLs, including inherited grants, are conservatively rejected throughout the chain and on
  private files; deny-only ACLs are allowed. Linux POSIX access ACL grants are constrained by
  the group-class mode mask. Other platforms fail closed pending M5. Root/admin and malicious
  same-UID mutation, including same-UID symlink/mount replacement, remain outside protection.
  No directory-FD alias trick or custom SQLite VFS is used.
- Lock/identity files use no-follow opens, effective-UID ownership, 0600 and single-link inode
  checks. A staging identity inode is checked before secret writes, fsynced, and published with
  a create-only hard link; cleanup uses the canonical directory. SQLite main/sidecar files
  reject symlink/hardlink/foreign-owner aliases before opening; safe older 0644 SQLite inodes
  are tightened to 0600 without replacing content. SQLite also uses OPEN_NOFOLLOW. The DB
  worker uses the canonical directory retained by the process lock. Close/process exit releases
  the persistent OS lock; its filename is never deleted to acquire a lock.
- Additional exact direct dependencies for that policy: `rustix 1.1.6` (already resolved,
  activate process feature for safe effective-UID lookup), `exacl 0.13.0` (macOS-only safe ACL
  reads, default features disabled), and already resolved `log 0.4.35` (macOS-only feature
  activation `max_level_info`). exacl's failure debug instrumentation includes paths, so the
  legacy log facade's debug/trace calls are removed at compile time, even with a future runtime
  logger; tracing remains independent. This compilation-wide legacy-log cap and conservative
  rejection of benign allow ACLs are explicit pending-review tradeoffs. Cargo generated the
  lockfile. Raw version evidence is `docs/reports/T-002-fix-dependencies.json`; the original
  `T002-dependencies.json` evidence is retained.
- Keychain production service/account are `computer.meshyard.device` / `device`. Reads use
  keyring; insertion uses its backend's safe create-only API because keyring's setter upserts.
  NotFound alone permits generation; Locked/Unavailable/invalid/permission failures propagate.
  File storage is explicitly constructed, never selected automatically. No secret is in SQL,
  IPC types, Debug or error details. No runtime logging is introduced by this task.
- SharingPolicy's default drops every free-text task label, preserves deliberate agent labels,
  and carries only structured fields otherwise. Explicit task-label opt-in is a caller's claim
  of Meshyard user entry or field-level permission; arbitrary text cannot be inspected to prove
  it secret-free. Adapter allowlist/source enforcement belongs to T-007, not a text classifier.
- Durable pending-membership quota: before each new insert, never-confirmed authors are
  limited to 1024 total rows / 1 MiB of original outer-envelope bytes across both `events`
  (contiguous holding) and `pending_events` (gap holding), per workspace/author. All bodies
  count, including unauthorized peer membership bodies. Duplicate/equivocation and invalid
  field handling still precede accounting. Overflow returns Rejected(PendingOverflow), drops
  both held sets transactionally, and removes that head (absent means zero); T-006 must
  re-request from zero. An own-append overflow commits the same drop and returns AppendRejected.
  Opening an older database also transactionally discards oversized never-confirmed holdings.
  No new table or wire change is needed: the durable rows themselves are the accounting source.
- Confirmation authority is verified genesis plus a verified contiguous prefix of the owner's
  `events` log only. A peer grant or an owner grant still in `pending_events` cannot exempt an
  author. For retention purposes confirmation means ever validly added, so reserved M2
  MemberRemoved never deletes established logs. Current membership for projection still uses
  the full owner fold. Established authors retain the separate gap-only quota; its overflow
  never erases their contiguous history.
- Inserted means persisted contiguously, not membership authorization; Pending means a gap.
  This preserves the public API and protocol definition of contiguous heads. Bounded unknown
  contiguous rows can be returned for folding but are never speculatively applied. A later
  owner grant makes their permissible ordinary events applicable directly from the existing
  rows; gap promotion moves each row once and raw duplicates remain duplicates. Fold indexes
  only confirmed authors' complete logs; unknown ordinary events are held with bounded storage
  while iterating, rather than first collecting every unknown input reference. Projection
  retains conservative wire-size accounting; SQL uses exact original envelope lengths.
- Regenerate the 14 UI-domain-only bindings from the repository root with
  `npm run bindings:generate` (offline cargo example + deterministic Prettier). Wire, signing
  keys and signed byte envelopes are never exported. No IPC transport is connected in T-002.
  Scaffold `version()` helpers remain only because the unchanged T-001 bridge still uses them.

Manual boundary: the isolated Keychain probe returned `Unavailable`; no successful macOS
Keychain roundtrip is claimed. T-001 native/first-launch acceptance remains open independently.
