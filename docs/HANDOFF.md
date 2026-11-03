# Meshyard — Agent Handoff Board

Single source of truth for coordination between Claude Code (architect/reviewer) and OpenAI Codex
(implementer). Both agents read this file at the start of every work session and update it at the
end. Newest log entries on top.

## 1. Protocol

1. **Claude** writes a task spec in `docs/tasks/T-XXX-*.md` and sets it `ready` in `ROADMAP.md`.
2. **Codex** picks the lowest-numbered `ready` task whose dependencies are `done`, sets it
   `in-progress` here and in ROADMAP, implements it, runs `npm run check` (+ task-specific tests).
3. **Codex** writes `docs/reports/T-XXX.md`: summary, files changed, commands run with real results,
   deviations from spec, known gaps, questions. Sets status `review`. Never marks its own task `done`.
4. **Claude** reviews against spec + architecture, writes `docs/reviews/T-XXX.md` with verdict
   `done` or `changes-requested` and a numbered list of actionable fixes.
5. Codex addresses fixes, appends to the same report, sets `review` again.
6. Architectural changes: proposer adds an entry under "Open decisions" below; Claude decides
   and records an ADR in `ARCHITECTURE.md §10`. Owner-level decisions (scope, paid services,
   privacy trade-offs, destructive ops, major UX) go to Dion.

Rules for both: never claim a test/hardware run that did not happen; never commit secrets; follow
`~/.claude/CLAUDE.md` / `AGENTS.md` git rules (no co-author lines, no force push, commit only as
agreed below).

## 2. Session channels (verified 2026-11-02)

| Channel | Status |
|---|---|
| Direct Claude ↔ Codex messaging | Not available as a native feature. `mcp-agent-mail` MCP server is configured for Claude but failed to connect this session. |
| Codex CLI `codex queue --thread <id> --message … [--resume]` | Exists in codex-cli 0.166.0. Owner-approved (D-3) as the way to hand tasks to the implementer session. |
| Codex as **reviewer / second opinion** | `codex exec -s read-only --ephemeral -C <repo> -o docs/reviews/codex-<topic>.md "<question>"` — separate, read-only, ephemeral run; independent of the implementer session. Used by Claude for architecture, security, P2P and feasibility questions. |
| ChatGPT conversations | Not accessible. Questions needing that context go to Dion. |
| Codex CLI `codex exec resume <id> "<prompt>"` | Exists. Non-interactive resume of a persisted session. |
| Codex implementer session `01a19e0d-2c19-7810-8e5c-410055baffb6` | `codex-tui` thread, cwd = this repo. T-001 handoff delivered via `codex queue` on 2026-11-02 22:23 (verified in its rollout file). Works as parent + worker. |
| **Knower** (Codex, technical knowledge authority) | Thread `01a19e2a-ebb0-7c21-8860-08f501a12e7e` (verified by the Codex lead, 2026-11-02). Role: research, troubleshooting, explanations, persistent project knowledge; inspects real docs/code before answering. Owns `docs/knowledge/` (README, PROGRESS, OPEN_QUESTIONS, QUESTIONS_FOR_KNOWER, KNOWER_RESPONSES); other agents do not edit those files. Consulted via `codex exec resume 01a19e2a-ebb0-7c21-8860-08f501a12e7e "<question>"` or `codex queue --thread 01a19e2a-…`. Architectural decisions it raises (KQ-xxx) are decided by Claude and recorded as ADRs; owner-level ones go to Dion. |
| Repository files (this board, tasks, reports, reviews) | Primary, always-available channel. |

## 3. Current state

Migration path update (2026-11-03): active paths below are on the APFS image backed by
`/Volumes/Untitled/projects/MeshyardProjects.sparsebundle`; the exposed mount is
`/Volumes/MeshyardProjects`. Task facts retained below reflect this branch's historical
state; use the migration log entry and the active T-002 branch for the current review gates.
No prior task status, review decision or owner decision is changed by this path-only migration.

- Owner decision (2026-11-03): develop and build **only on the USB volume**. Canonical checkout:
  `/Volumes/MeshyardProjects/meshyard`; task worktrees: `/Volumes/MeshyardProjects/meshyard-worktrees/`.
  Both use Git metadata at `/Volumes/MeshyardProjects/meshyard/.git`, independent of temporary clones.
- Planning baseline: `0f8d52f` on `main`, committed and pushed.
- T-001: `t-001-scaffold` in the canonical checkout; source cleanup `ac125bb`, status ping
  `8275fb0`, knowledge commit `a101deb`. Check/build evidence is in `docs/reports/T-001.md`;
  review remains open for native visual acceptance and the original first-start <20 ms criterion.
- T-002: active branch `t-002-core-store-identity` at
  `/Volumes/MeshyardProjects/meshyard-worktrees/t-002-core-store-identity`.
  Code fixes are accepted by Claude; wire v1 is frozen. The task remains **review** until
  Dion's native Keychain roundtrip passes; no merge or T-003 is authorized. See the active
  branch's review/report; historical branch-specific implementation notes below are superseded.
- Smithers workspace: `/Volumes/MeshyardProjects/meshyard-smithers`; existing run databases/logs:
  `/Volumes/MeshyardProjects/Meshyard-build/orchestration` (initial `t001.db` migrated with the workspace).
  No Meshyard Smithers/`codex exec` worker is active at migration completion. Historical runs are
  archived; do not resume them in temporary checkouts.
- Before project or Smithers commands, source `/Volumes/MeshyardProjects/meshyard-smithers/env.sh`.
  It sets `CARGO_HOME`, `RUSTUP_HOME`, `CARGO_TARGET_DIR`, npm/Bun caches, `TMPDIR` and SQLite
  temporary files under `/Volumes/MeshyardProjects/Meshyard-build`. Install frontend dependencies
  physically inside their USB checkout/worktree; do not symlink them from an internal checkout.
- The APFS sparsebundle on the USB stick **must remain mounted**. The Documents checkout,
  `/private/tmp/meshyard-t001-recovery-checkout` and the old T-002 worktree remain preserved,
  archival and unused; no deletion without Dion's confirmation. New runs must use USB paths.
- Toolchain verified after migration: Rust/cargo 1.99.1 from the external toolchain. Node/npm
  versions previously verified: Node 26.7.0, npm 11.19.0; Xcode present.

## 4. Task board

| Task | Status | Assignee | Notes |
|---|---|---|---|
| T-001 scaffold | review | Codex → Claude | Full check/build pass; corrected CSP rendering proven headlessly; native retest blocked by tool permission and first-launch latency still over budget; no acceptance claimed |
| T-002 core/store/identity | review | Dion → Claude | Code accepted on active T-002 branch; awaiting real native Keychain pass, then Claude records done; merge after T-001 |
| T-003 net endpoint | ready | Codex | after T-002; revised test requirements (no public discovery, selected path) |

## 5. Open decisions

| # | Question | Proposed | Decider | Status |
|---|---|---|---|---|
| D-1 | Install Rust via rustup on this Mac | Codex installs it as step 0 of T-001 | Dion | **decided** |
| D-2 | Relays for P2P Auto | n0 public relays for dev prototype with non-sensitive data only; self-hosted redundant CH/EU relays (`RelayMode::Custom`) before external distribution (gate G-1); Strict = `RelayMode::Disabled` | Dion | **decided** (ADR-006) |
| D-3 | How Codex receives tasks | `codex queue` to session `01a19e0d…` + this board | Dion | **decided** |
| D-4 | Commit policy for Codex | Codex may commit **and push**: initial docs commit on `main`, then one branch `t-XXX-*` per task, focused commits; no force push, no history rewrite | Dion | **decided** |
| D-5 | License | none chosen | Dion | open |

## 6. Log

- 2026-11-03 — Codex lead: Claude's re-review is confirmed complete (read-only run
  `d6837df0-8a3b-45a4-8805-2db0b8db3780`). All three code fixes accepted; wire v1 frozen;
  no code blockers. T-002 stays review until Dion's real native Keychain pass; T-001 still
  needs recorded L1 and explicit merge instruction. Owner-status question pending; no evidence
  assumed from elapsed time. Published review `21fbf2f`; updated T-001 commands `d76f2c1`.
  No merge/T-003; KQ-009 is the remaining separate architect prerequisite before T-003.

- 2026-11-03 — Codex lead: inspected the completed projects migration and actual offline
  check (exit 0; 55 Rust + lock child, 3 Vitest). Active checkout is now
  `/Volumes/MeshyardProjects/meshyard`, backed physically by the image under
  `/Volumes/Untitled/projects`; all old sources and five dirty knowledge files preserved.
  Corrected the remaining active architect path reference. Publishing only migration docs
  and operational instruction paths; no code, review verdict, merge or native acceptance.
  See `docs/reports/USB-MIGRATION-2026-11-03.md`; Claude re-review is the next gate.

- 2026-11-03 — Owner-approved bounded storage migration (Hardworker): verified complete
  SHA256/mode/link manifests before operational edits; copied exactly the four Meshyard roots
  from `/Volumes/Kursmodule` using ditto. Active APFS mount: `/Volumes/MeshyardProjects`;
  physical backing image: `/Volumes/Untitled/projects/MeshyardProjects.sparsebundle` on ExFAT.
  A literal nested mount under `projects/workspace` did not succeed and was not retried.
  New canonical HEAD remains `f59aeca`; active T-002 remains `6ac6c1f`; detached review remains
  `54ecc91`; origin/main remains `0f8d52f`. Repaired only new worktree registration; indexes,
  refs, KNOWER's five dirty knowledge files, original sources/images and temporary/Documents
  checkouts are preserved. Operational inputs archived; historical runs must not be replayed.
  The copied `Meshyard-build/t002-worktree` remains inactive with its historical .git intact.
  No implementation, commit, push, merge, native UI or manual Keychain test in this migration.
  Current gates: T-001 awaits Claude-recorded Dion L1 plus instruction; T-002 fixes checked/pushed
  at `6ac6c1f` await Claude rereview; Keychain positive native coverage remains unverified.
  Original hook <20 ms requirement is T-007; no T-003 until T-001 and T-002 are done and merged.
  Owner target is genuine pairing/sync/hooks/UI plus a Strict-only tester DMG; this is future
  work, not available functionality. Detailed actual validation and limits: `/Volumes/MeshyardProjects/migration-control/MIGRATION-REPORT.md`.

- 2026-11-03 — USB-only migration completed per Dion: both task branches fetched and
  fast-forwarded from origin; T-002 now has a USB-owned worktree/Git directory. Moved Smithers
  workspace/state from temporary storage; verified all 29 original entries before path updates,
  archived original inputs, checked seven SQLite databases and bundled seven workflow files
  without running jobs. KNOWER explicitly confirmed readiness; five unchanged knowledge files
  committed separately as `a101deb`, including advisory KQ-008 (DMG/onboarding planning pointer
  for Claude, CTA meaning still awaiting Dion). Updated §3 on both task branches; no source,
  task acceptance or product-scope changes. No new app checks/native test or implementation run;
  old checkouts preserved. Final-source T-002 audit is the next bounded USB-only run.

- 2026-11-03 07:46 CET — Codex lead status ping: no active Meshyard Smithers/`codex exec`
  processes. T-002 fix run `ecc5ea73-ecc2-4cea-baec-1a1fe31d3348` completed implementation/checks;
  its final-source audit failed on a 600-second idle timeout. Unrelated OpenTwin Smithers watcher
  PID 2358 (`thinker-team-watch-20261031-1216`) remains active, untouched. Latest task commits:
  `t-001-scaffold` / `ac125bb`, `t-002-core-store-identity` / `9889c7b`, both pushed; this
  status-only commit follows. Blockers: low system disk (~1 GB reported, 2.1 GiB measured;
  external build volume 55 GiB free), last Computer Use app-access denial and automatic-review
  rejection of alternate Terminal launch (shell now unrestricted; GUI not retested), hook first
  start above original <20 ms limit, Keychain positive check unavailable. Next: final-source
  T-002 audit then Claude review; native T-001 recheck after app approval. No new run started.

- 2026-11-03 — Bounded T-001 review cleanup: make the existing external headless CSP check
  discoverable, use portable README setup, allow unwrap/expect only in tests through Clippy
  configuration, and build only an rlib for the desktop app. Dependencies and production CSP
  unchanged. Earlier worker-only T-002 restrictions are historical; the owner's permission
  to start T-002 after review remains in force. This worker does not touch T-002. The owner
  kept the original first-start `<20 ms` criterion; profiling found no credible fix. Automatic
  approval review blocked Meshyard GUI launch/control; no native launch, capture, alternate
  channel or visual acceptance is authorized by this cleanup. T-001 stays **review, not done**.
  The existing main tracing event has no subscriber/console sink; observability and the later
  agent-pipeline redaction layer are not implemented by this scaffold cleanup.

- 2026-11-03 — Codex lead: T-001 review requested after clean-clone full checks/native build
  and red/green production-CSP rendering test. Native pre-fix bundle showed graphics fallback;
  repaired using Pixi static polyfills without weakening CSP. Corrected native retest is pending
  because Computer Use denied Meshyard access. First launch 289.109 ms, warm p95 2.409 ms;
  original criterion unchanged, owner decision pending. User explicitly permits T-002 when
  T-001 is review and T-002 ready; a stacked task branch is allowed without marking T-001 done.

- 2026-11-02 — Claude: Knower thread `01a19e2a-ebb0-7c21-8860-08f501a12e7e` registered (§2).
  Knower onboarding received; Claude read `docs/knowledge/` README, PROGRESS, OPEN_QUESTIONS
  (KQ-001…KQ-007), QUESTIONS_FOR_KNOWER, KNOWER_RESPONSES; Codex lead also read them. Decided
  KQ-001 → ADR-009 and KQ-004 → ADR-010; normative text in PROTOCOL v1.1, T-002 spec updated
  accordingly. KQ-002/003 (before T-007 spec), KQ-005 (before T-003 start), KQ-007 (owner
  items) remain open. T-001 hook acceptance rewrite is a **proposal only** (sent to the lead,
  not applied); existing criterion unchanged. Edits made in the isolated recovery checkout;
  no commits by Claude.

- 2026-11-02 — Codex lead: Rust stable 1.99.1 installed via official rustup (default profile,
  user-level, no shell-profile changes); planning commit `0f8d52f` pushed to main. T-001 source
  and report committed on t-001-scaffold. Partial target/debug artifacts removed after ENOSPC;
  release hook and evidence retained. Need 8–10 GB free or an external build location to finish
  validation. Owner explicitly permits T-002 after T-001 reaches review; this overrides the
  general done-dependency for that continuation, but T-001 has not yet reached review.

- 2026-11-02 — Codex, T-001 recovery: generated both lockfiles; npm ci, frontend checks/build,
  Rust formatting, three small-library tests, focused Clippy and release hook build passed.
  Full check/native build stopped at the low-disk safeguard; native dev compilation also
  reported ENOSPC in objc2. Vite served HTTP 200 and was stopped; native window/visual check
  pending. First hook invocation was 442.322 ms (warm median 2.122 ms), so the <20 ms criterion
  is not met for all measured runs. Keep in-progress; see docs/reports/T-001.md. T-002 untouched.

- 2026-11-02 — Claude: independent Codex architecture review (`docs/reviews/codex-architecture-2026-11-02.md`,
  1 blocker, ~15 major). Resolved in new normative `docs/PROTOCOL.md` (signed events, owner-anchored
  WorkspaceId, contiguous heads, equivocation, frozen wire v1, two-stage admission, pairing state
  machine, cursor-based sync, ingest deadlines/correlation, single-instance lock); ADR-007/008;
  T-002/T-003 specs revised; roadmap adds T-003b/T-006b field tests and T-R1 relay gate task.

- 2026-11-02 — Claude: owner decisions D-1..D-4 recorded; ADR-006 (relay phases, prefer-direct
  window); reviewer channel via read-only `codex exec` established.
- 2026-11-02 — Claude: initial inspection, research (iroh 1.0, Codex + Claude Code hooks,
  Tauri 2, PixiJS v8), architecture + MVP plan, specs T-001..T-003.
