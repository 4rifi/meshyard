# Verified project knowledge and progress

## Current refresh — 2026-11-03

Canonical checkout: `/Volumes/Kursmodule/meshyard` (owner decision).
Read-only inspection base: `c960af2eb81f20b69af4396cde37b5034a10a4e3` on
`t-001-scaffold`, approximately 05:45 UTC / 06:45 Europe/Zurich. The five knowledge files
were byte-identical to the original Documents copies before this refresh; no file was missing.
KNOWER writes only this directory in the canonical checkout; the original remains untouched.

- **Coordination verified:** HANDOFF registers KNOWER's thread ID and records that Claude and
  Hardworker read the onboarding. Native Codex thread messaging is now exposed in this runtime.
- **Accepted architecture update:** inspected PROTOCOL v1.1 and ARCHITECTURE ADR-009/010 resolve
  KQ-001 and KQ-004: sender-only sync, owner-only invitations, genesis-only Welcome, exact raw
  signed bytes, receiver validation without mutation, owner fold and closed wire v1.
  T-002 spec incorporates these contracts. This is documentation verification, not proof of a
  T-002 implementation.
- **T-001 status:** HANDOFF and ROADMAP say `review`, with native retest and first-launch
  acceptance open. The owner explicitly allows T-002 when T-001 is review and T-002 ready,
  including a stacked task branch; do not apply the old blanket done-dependency to that case.
- **Reported execution evidence:** the current [T-001 report](../reports/T-001.md) records full
  `npm run check` and native debug bundle build passing after CSP fix `9e31b6c`. The latest
  bounded-investigation check exited 0 in 158.863596 s: five Rust unit tests and three Vitest
  tests passed, alongside formatting, Clippy, TypeScript, ESLint and Prettier. KNOWER read the
  report and did not rerun these commands or independently inspect their raw logs.
- **Native boundary:** corrected production-CSP rendering has reported headless Chromium
  red/green evidence. The corrected native WKWebView retest remains blocked by app launch/control
  approval; neither the build nor Chromium result establishes native visual acceptance.
- **Latency remains failed:** the owner retained the original first-start `<20 ms` requirement.
  Two reported fresh-build series measured first launches of 245.659458 ms (baseline) and
  241.191166 ms (size/LTO candidate); all 50 following samples per series passed. No candidate
  was promoted, no profile/source change was retained, and no causal explanation was established.
  Previous 442.321500 ms and 289.108791 ms results remain historical failures.
- **Remaining questions:** KQ-002/003/005 remain open; KQ-006 needs summary reconciliation and
  future networking API/runtime validation; KQ-007 retains owner-level license/hosting items.
  HANDOFF's checkout-path paragraph still needs its owner to apply the newer canonical-path
  decision. No task status or canonical spec was changed by KNOWER.

This documentation-only refresh checks copy integrity, local references and whitespace. It
does not rerun product checks, launch the app, claim acceptance, commit or push. The dated
onboarding snapshot below is retained as history and is superseded by this section for current
status, coordination and architecture decisions.

## Historical onboarding snapshot — 2026-11-02

Observation: 2026-11-02, approximately 21:33 UTC / 22:33 Europe/Zurich.
Basis: local branch `t-001-scaffold`, HEAD `0f8d52f77c834072adcda06d8a9bbaa9a9dee888`.
Substantial uncommitted scaffold and planning changes were already present and are being edited
by other agents. This is a working-tree observation, not a review of a frozen release or proof of
remote publication. Refresh it before the next milestone.

## Product understanding

Meshyard is an independent, privacy-first, local-first desktop product inspired by an internal
hackathon helper, not the hackathon submission. Vision: "Your agents. Your machines. Your data."
Devices should pair, observe real local coding-agent events, synchronize selected state over
encrypted P2P links, and render agents in a pixel-art yard. Task handoffs, Git coordination and
natural-language configuration are later milestones, not current functionality.

M1 targets two Macs on different networks, genuine Claude Code hooks, paired authenticated
connections, synchronized activity, direct/relay diagnostics and honest Strict failure behavior.
Codex integration is an M1 stretch goal. See [PRODUCT](../PRODUCT.md) §§4–6.

## Accepted design, not implemented capabilities

- Tauri 2 hosts React/strict TypeScript/Vite panels and an imperative PixiJS v8 scene.
- Rust crates separate pure domain/wire logic (`core`), SQLite (`store`), iroh (`net`) and
  adapters/ingest (`agents`). `app` and the headless `node` compose these; `hook` is a tiny forwarder.
- Ed25519 device identity, explicit Keychain/file-store policy, one process per data directory.
- Owner-anchored workspace genesis, signed per-device event logs, contiguous heads,
  equivocation detection, deterministic fold and separate frozen wire v1 types.
- Device admission precedes application decoding; workspace admission follows a bounded Hello.
  Pairing is a separate persistent token/SAS protocol. MVP prohibits third-party event forwarding.
- Auto permits disclosed encrypted relaying and a prefer-direct catch-up window; Strict disables
  relays at endpoint construction. Strict still has discovery/metadata dependencies.
- Raw commands may pass only through the local ingest pipeline for classification, then are
  discarded. No prompts, commands, paths or contents in shared events by default. Hook-shaped
  input from the same OS user is not cryptographic proof of agent origin.
- External distribution is gated on redundant self-hosted CH/EU relays (G-1/T-R1). Self-hosting
  does not remove metadata exposure or make Strict connections universally possible.

Sources: [ARCHITECTURE](../ARCHITECTURE.md) §§1–3, 10; [PROTOCOL](../PROTOCOL.md);
[SECURITY](../SECURITY.md); [ROADMAP](../ROADMAP.md). Outstanding ambiguities are tracked separately.

## Inspected implementation

| Area | Observed files | Confirmed source state |
|---|---|---|
| Workspace | [Cargo.toml](../../Cargo.toml), [package.json](../../package.json) | Seven Rust workspace members, strict lint configuration, exact direct dependency pins and check/dev/build scripts |
| Core/store/agents | `crates/meshyard-{core,store,agents}/src/lib.rs` | Version functions and small unit tests only; no event model, SQLite or adapter implementation |
| Networking | [net/lib.rs](../../crates/meshyard-net/src/lib.rs) | Re-exports `iroh::EndpointId`; no endpoint constructed, pairing or sync |
| Hook | [hook/main.rs](../../crates/meshyard-hook/src/main.rs) | Empty main; no stdin parsing, forwarding or installation |
| Headless peer | [node/main.rs](../../crates/meshyard-node/src/main.rs) | Prints scaffold version; no peer commands |
| Native shell | [app/lib.rs](../../app/src-tauri/src/lib.rs), [Tauri config](../../app/src-tauri/tauri.conf.json), [capability](../../app/src-tauri/capabilities/main.json) | Basic Tauri builder, local CSP, empty IPC permission list; no registered product commands/plugins |
| Frontend | [App.tsx](../../app/src/App.tsx), [placeholder.ts](../../app/src/playground/placeholder.ts) | Placeholder page and one static Pixi rectangle, described in text, graphics-error fallback; no agents/peers |
| Scene lifecycle | [sceneLifecycle.ts](../../app/src/playground/sceneLifecycle.ts), [tests](../../app/src/playground/sceneLifecycle.test.ts) | Async initialization/unmount cleanup logic; three test cases exist, execution unverified |

Manifests currently select iroh `1.3.0`, Tauri `2.12.4`, React `19.3.1`, PixiJS `8.22.0` and
Rust `1.99.1`. These are observed pins, not independently verified registry releases or successful
builds. Reserved workspace dependencies (SQLite, Tokio, ts-rs, etc.) do not prove usage.
ARCHITECTURE §11 contains the implementer's pending-review provenance for version selection.

## Roadmap observation

HANDOFF and ROADMAP both show T-001 **in-progress**. T-002 and T-003 are marked ready, but their
dependencies are not done, so they are not eligible for pickup. No T-001 implementation report or
task acceptance review was present during this inspection. The architecture review exists and
has informed the new protocol; it is not a scaffold acceptance review.

Next: Hardworker completes T-001 and reports actual checks; Claude reviews. Then T-002 establishes
core/store/identity and wire contracts. KNOWER recommends resolving KQ-001/KQ-004 before freezing
those types. Network field tests remain required later; localhost/mocked tests cannot replace them.

## Commands actually attempted by KNOWER

| Command | Actual result | Boundary |
|---|---|---|
| `git status --short`, `git branch --show-current`, `git rev-parse HEAD`, source/doc reads | Succeeded | Read-only local working-tree evidence |
| `codex --version` | `codex-cli 0.166.0` | Warned it could not create PATH aliases in this sandbox |
| `codex queue --help`, `codex exec resume --help`, `claude --help` | Help returned | Interfaces available; no session receipt/activation proven |
| `printenv CODEX_THREAD_ID`, `printenv CODEX_SESSION_ID` | Both returned KNOWER ID recorded in README | Current-process identity only |
| `npm run check` | Exit 127: `sh: cargo: command not found` | Stopped before formatting, Clippy or tests |
| `/Users/dionarifi/.cargo/bin/cargo --version` | rustup tried to sync pinned toolchain; temp-file creation in `~/.rustup/tmp` denied | No compiler version or build success established; no toolchain installation attempted |
| `npm run check:frontend` | Exit 127: `sh: tsc: command not found` | No TypeScript, ESLint or Vitest checks executed |
| `npm run format:check` | Exit 127: `sh: prettier: command not found` | No successful formatting check |
| `codex queue --thread <Hardworker ID> --message <onboarding pointer>` (without `--resume`) | Initial sandbox attempt exit 1, embedded server not permitted; authorized retry exit 0, queued message `01a19e30-a56c-7433-ad28-acc2a455ea2d` | Queue accepted; no acknowledgement, activation or Claude receipt proven |
| Read-only Python Markdown link-target check for `docs/knowledge/*.md` | Exit 0: 5 files, 35 local targets checked, 0 missing | Checks target existence only, not section anchors or external URLs |

`node_modules/` exists but required executables were unavailable to npm in this environment.
KNOWER did not install dependencies or repair the shared scaffold. No app launch, screenshot,
hardware/network run, benchmark, full passing test suite, deployment, commit or push was performed.
These failures describe this session's environment, not a proven application defect.
