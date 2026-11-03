# Meshyard — Roadmap

Status legend: `todo` · `ready` (spec complete, can be picked up) · `in-progress` · `review` ·
`changes-requested` · `done` (reviewed + verified). Detailed specs live in `docs/tasks/`.

## Milestone 1 — Working MVP (two Macs, different networks)

| ID | Task | Depends on | Status |
|---|---|---|---|
| T-001 | Repo scaffold: Cargo workspace, Tauri 2 app, React/TS/Vite, lint/typecheck/test scripts, pinned versions | — | review (native retest and first-launch latency open) |
| T-002 | `meshyard-core` domain types + `meshyard-store` (SQLite, migrations, event log) + device identity (Keychain) | T-001 | review |
| T-003 | `meshyard-net` endpoint: Auto/Strict modes, ALPN routing, allowlist, path diagnostics; direct + relay test suites | T-002 | ready |
| T-003b | Field test 1: two Macs on different networks, direct + relay + strict via `meshyard-node ping` (manual, results logged honestly) | T-004 | todo |
| T-004 | `meshyard-node` headless CLI skeleton + test harness (`--data-dir` profiles, `create`, `status`, `ping`, `tail`); `invite`/`join` wired after T-005 | T-003 | todo |
| T-005 | Pairing protocol `pair/1`: ticket, single-use token, SAS, membership | T-003, T-004 | todo |
| T-006 | Sync protocol `sync/1` per PROTOCOL §8: Hello/workspace admission, cursor-based catch-up + live, reconnect supervisor, prefer-direct window (ADR-006) | T-005 | todo |
| T-006b | Field test 2: headless pairing, catch-up, disconnect, sleep/wake on two networks | T-006 | todo |
| T-007 | Agent pipeline per PROTOCOL §9: `meshyard-hook`, ingest socket, Claude Code adapter, correlation/staleness state machine, hook installer (diff + approval) | T-002 | todo |
| T-008 | Codex adapter (hooks.json, trust instructions) | T-007 | todo (M1 stretch) |
| T-009 | Tauri bridge: commands/events, ts-rs types, view-model store | T-006, T-007 | todo |
| T-010 | Playground v1: tilemap, stations, avatars, movement, inspector, list view | T-009 | todo |
| T-011 | Onboarding + diagnostics UI: create workspace, connect agent, invite (link + QR), join, SAS, peer status | T-009 | todo |
| T-012 | System validation: two-node scripted test, `docs/VALIDATION.md` real-device protocol, results log | T-010, T-011 | todo |

Parallelizable: T-007 can run alongside T-003…T-006.

## Release gate G-1 — before any external user gets a build
- Task **T-R1** (owner: Dion for hosting/costs, Claude for spec, Codex for config/tests):
  self-hosted iroh relays in CH/EU (≥ 2, redundant), `RelayMode::Custom`, minimal logging,
  documented operator + metadata exposure, relay-failover test, CI check that release config
  contains no n0 relay URLs.

## Milestone 2 — Team-ready
- 3–8 peers (full mesh), member list management, revocation (`MemberRemoved`, epochs — fields already reserved in PROTOCOL v1).
- Signed-event forwarding between peers (signatures already in v1).
- Self-hosted relay deployment (G-1) + optional self-hosted pkarr; relay operator shown in UI.
- Spike R-3: Strict + coordination-only relay (prove no app bytes on relay).
- Log compaction/snapshots; evaluate Automerge/iroh-docs for shared documents.
- Sharing policy UI with per-field opt-in.
- Packaging: signed + notarized macOS build, auto-update channel.

## Milestone 3 — AI-configured workspaces
- Workspace config schema (roles, stations, workflows, rules, permissions, layout) + validator.
- Setup assistant (uses the user's own local agent or API key, never a Meshyard server).
- Playground generated from config; customization editor.

## Milestone 4 — Agent collaboration & Git
- Task board, agent-to-agent handoffs (task + selected context, explicit approval).
- Git awareness: branches, per-agent worktrees, PR review workflow, conflict surfacing.
- Human approval gates for merges and destructive ops. Scoped remote execution grants.

## Milestone 5 — Platforms
- Windows + Linux builds (named pipes / UDS, keyring backends), more agent adapters.

## Risks

| ID | Risk | Mitigation |
|---|---|---|
| R-1 | Hole punching fails on some networks | P2P Auto relay fallback, clear diagnostics, honest docs |
| R-2 | Strict P2P without relay has a lower success rate | document; port mapping; IPv6; R-3 spike. (Self-hosted relays do **not** help Strict mode.) |
| R-3 | Gating app data on direct path with a relay present is hard to prove | not shipped until proven by tests |
| R-4 | Hook payload schemas change between CLI versions | adapters tolerant (unknown fields ignored), golden fixtures per CLI version, version detection |
| R-5 | Inferred states mislead users | provenance badge on every state; conservative rules |
| R-6 | n0 public relays have rate limits / no SLA | self-hosted relay option (M2) |
| R-7 | macOS notarization requires paid Apple developer account | owner decision before M2 packaging |
