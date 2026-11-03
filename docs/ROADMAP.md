# Meshyard — Roadmap

Status legend: `todo` · `ready` (spec complete, can be picked up) · `in-progress` · `review` ·
`changes-requested` · `done` (reviewed + verified). Detailed specs live in `docs/tasks/`.

## Milestone 1 — Working MVP (two Macs, different networks)

| ID | Task | Depends on | Status |
|---|---|---|---|
| T-001 | Repo scaffold: Cargo workspace, Tauri 2 app, React/TS/Vite, lint/typecheck/test scripts, pinned versions | — | review (native check = owner checkpoint L1; hook first-launch <20 ms criterion moved unchanged to T-007) |
| T-002 | `meshyard-core` domain types + `meshyard-store` (SQLite, migrations, event log) + device identity (Keychain) | T-001 | review (code accepted by Claude re-review 2026-11-03; `done` after a real native Keychain roundtrip by Dion; merge after T-001) |
| T-003 | `meshyard-net` endpoint: Auto/Strict modes, ALPN routing, allowlist, path diagnostics; direct + relay test suites | T-002 | in-progress (lane C, D-9) |
| T-003b | Field test 1: Strict feasibility (owner checkpoint L2 on Dion's Mac: two profiles + router port-mapping check); cross-network Strict with the test partner happens in T-012. Relay paths are tested only locally (no n0 with the partner, D-7) | T-004 | todo |
| T-004 | `meshyard-node` headless CLI skeleton + test harness (`--data-dir` profiles, `create`, `status`, `ping`, `tail`); `invite`/`join` wired after T-005 | T-003 | todo |
| T-005 | Pairing protocol `pair/1`: ticket, single-use token, SAS, membership | T-003, T-004 | todo |
| T-006 | Sync protocol `sync/1` per PROTOCOL §8: Hello/workspace admission, cursor-based catch-up + live, reconnect supervisor, prefer-direct window (ADR-006) | T-005 | todo |
| T-006b | Field test 2: headless pairing, catch-up, disconnect, sleep/wake on two networks | T-006 | todo |
| T-007 | Agent pipeline per PROTOCOL §9: `meshyard-hook`, ingest socket, Claude Code adapter, correlation/staleness state machine, hook installer (diff + approval) | T-002 | in-progress (T-007a hook/normalizer started; T-007b after T-014 review) |
| T-008 | Codex adapter (hooks.json, trust instructions) | T-007 | todo (M1 stretch) |
| T-009 | Tauri bridge: commands/events, ts-rs types, view-model store | T-006, T-007 | todo |
| T-010 | Playground v1: tilemap, stations, avatars, movement, inspector, list view | T-009 | todo |
| T-011 | Onboarding + diagnostics UI: create workspace, connect agent, invite (link + QR), join, SAS, peer status | T-009 | todo |
| T-012 | System validation: two-node scripted test, `docs/VALIDATION.md` real-device protocol, results log, field test with the test partner (same LAN first, then different networks) | T-010, T-011, T-013 | todo |
| T-007b | Local app ingest, explicit agent binding and approved hook installer (backend prep first; frontend follows T-015) | T-014 code accepted; T-007a code accepted, final gates pending | in-progress (Connect UI and installer) |
| T-014 | **Local workspace slice** (pulls forward minimal T-009 bridge + T-011 create): create/reopen persisted workspace, own profile, register local agent, yard + list from real store state, honest error screens. Spec `docs/tasks/T-014-local-workspace-slice.md` | T-002 (stacked, D-9) | review |
| T-015 | **App-wide pixel redesign** (owner priority 1): pixel tokens/texture across all screens, yard as visual centre with stations + pixel characters from real agents. Spec `docs/tasks/T-015-pixel-app-redesign.md` | T-014 code-accepted (`1f90a07`) | review (code accepted `a859ed4`; reference image + N-14 pending) |
| T-016 | **Living yard** (owner priority 1): persistent scene, agents walk along yard paths to the station of their real state; reduced motion snaps. Spec `docs/tasks/T-016-yard-motion.md` | T-015 code-accepted | review |
| T-013 | Tester DMG: `aarch64-apple-darwin`, ad-hoc signed, stable id `ch.arifi.meshyard`, `meshyard://join` deep link, bundled `meshyard-hook`, Strict-only profile + bundle check for relay/n0 hosts, `docs/TESTER-GUIDE.md` (German) | T-011 | todo |

Parallelizable: T-007 can run alongside T-003…T-006 (separate worktree).

**Active lanes (D-9, 2026-11-03)** — all stacked on `t-002-core-store-identity` @ `c93fbe2`,
reviewed lanes merge into `m1-integration` (normal merge commits); `main` receives t-001, t-002,
then `m1-integration` once the T-001/T-002 owner gates close (D-6 unchanged):

| Lane | Task | Branch | Owns |
|---|---|---|---|
| A (priority) | T-015 code-accepted → **T-016 living yard** | `t-016-yard-motion` from `a859ed4` | `app/src/playground/**` + yard effect in `WorkspaceScreen.tsx` |
| B | T-007b §1–3 backend prep (running), then merge T-015/T-016 and do §4–6 connect flow | `t-007b-local-agent-integration` (base `3e8ccab`) | `app/src-tauri/**`, store binding; later `features/connect/**`, `ipc/client.ts`, `App.tsx`, `AgentList.tsx` |
| B | T-007a hook + normalizer | `t-007a-hook-normalizer` | `crates/meshyard-hook`, `crates/meshyard-agents` |
| C (when A/B have capacity) | T-003 Strict endpoint | `t-003-net-endpoint` | `crates/meshyard-net` |

Lanes edit only their own crate manifests; `Cargo.lock` conflicts are resolved by regenerating it
on merge, never by hand. Next after review: T-007b (app ingest + installer) on top of T-014, then
T-004 → T-005 → T-006, T-009/T-010/T-011 extending T-014's files.

**GitHub mapping (pending `gh auth login`; issues/milestones not inspectable yet):** milestone
`M1 Working MVP` with one issue per T-ID above (T-014, T-007a/b, T-003…T-006, T-009…T-013), label
`lane:A|B|C`; milestone `G-1 Release gate` with T-R1. Body = link to the spec file.

Delivery target (owner, 2026-11-03): the **full MVP** goes to one named test partner (Apple
Silicon Mac) as an ad-hoc signed DMG, **Strict P2P only** (no n0 or own relays). Owner
checkpoints on Dion's Mac, before the DMG is sent:

| Checkpoint | After | What Dion checks |
|---|---|---|
| L1 | T-001/T-002 review | `npm run dev` from the USB checkout shows the window with the Pixi placeholder |
| L2 | T-004 | two `--data-dir` profiles ping each other; `meshyard-node diagnose` shows whether the router grants port mapping (early Strict feasibility check) |
| L3 | T-006 | headless pairing (SAS) and messages between two profiles |
| L4 | T-011 | app with two profiles: pair, see each other, own Claude Code avatar moves on real events |
| L5 | T-013 | installed DMG works; invite link opened from another app lands in Meshyard |

If Strict cannot connect across the two networks (L2 or field test), Claude reports the
diagnosis and Dion decides: enable port mapping on a router, self-hosted relay (T-R1), or n0.
No silent relay fallback.

## Release gate G-1 — before any external user gets a build
Addendum (owner, 2026-11-03): a **Strict-only** build (Auto disabled, no relay URL and no n0
DNS/pkarr host compiled in, proven by a bundle check) may go to named test partners before G-1.
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
