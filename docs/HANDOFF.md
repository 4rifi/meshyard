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
  review awaits Dion's native checkpoint L1; the original first-start <20 ms criterion is now T-007.
  Its merge still awaits Claude-recorded Dion L1 evidence and an instruction to proceed.
- T-002: `t-002-core-store-identity` in
  `/Volumes/MeshyardProjects/meshyard-worktrees/t-002-core-store-identity`, returned to **review**
  after Claude's required fixes 1–3 from `dce3613`: `793d0f4` preserves owner authority,
  `753ca63` persists derived membership/epoch, `ce34a79` proves the existing outer-frame guard.
  Full USB offline check at `ce34a79`: exit 0, 68.657 s, 55 top-level Rust passes plus lock
  child, 3 frontend passes; 2 ignored entries. Report/status commit follows that checked source.
  Provider-capacity interruption and preserved WIP continuation are recorded in the report.
  Lead inspection and Claude code re-review are complete; wire v1 is frozen.
  Positive native Keychain acceptance remains pending; task status stays review. No merge or T-003 instruction is implied.
- Smithers workspace: `/Volumes/MeshyardProjects/meshyard-smithers`; existing run databases/logs:
  `/Volumes/MeshyardProjects/Meshyard-build/orchestration` (initial `t001.db` migrated with the workspace).
  Current active implementation runs: T-007a `b90bb555` and T-003 `bf7148c0`;
  T-014 `d09db809` finished with focused commits and a passing full offline check. Historical
  runs are archived; do not resume them in temporary checkouts.
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
| T-001 scaffold | review | Codex → Claude | Full check/build pass; corrected CSP rendering proven headlessly; Dion performs native checkpoint L1; unchanged hook latency requirement moved to T-007; no acceptance claimed |
| T-002 core/store/identity | review | Dion → Claude | Claude re-review 2026-11-03: code accepted, fixes 1–3 resolved, wire v1 frozen (`docs/reviews/T-002.md`). Only gate to `done`: Dion's native Keychain roundtrip (unverified). Merge after T-001 merge |
| T-003 net endpoint | in-progress | Codex lane C | stacked per D-9 (no longer waits for merge); KQ-009 decided (ADR-012 own mapper), ADR-011, strict-only switch |
| T-014 local workspace slice | changes-requested | Codex lane A | `t-014-local-workspace-slice` from `c93fbe2`; real store/IPC/UI, no native acceptance assumed |
| T-007a hook/normalizer | in-progress | Codex lane B | Corrected PROTOCOL §9 v1.2 implementation active (`b90bb555`); no app ingest/installer yet |
| T-004…T-013 | todo | Claude (specs) | other tasks follow their specs/dependencies; T-013 = tester DMG |

## 5. Open decisions

| # | Question | Proposed | Decider | Status |
|---|---|---|---|---|
| D-1 | Install Rust via rustup on this Mac | Codex installs it as step 0 of T-001 | Dion | **decided** |
| D-2 | Relays for P2P Auto | n0 public relays for dev prototype with non-sensitive data only; self-hosted redundant CH/EU relays (`RelayMode::Custom`) before external distribution (gate G-1); Strict = `RelayMode::Disabled` | Dion | **decided** (ADR-006) |
| D-3 | How Codex receives tasks | `codex queue` to session `01a19e0d…` + this board | Dion | **decided** |
| D-4 | Commit policy for Codex | Codex may commit **and push**: initial docs commit on `main`, then one branch `t-XXX-*` per task, focused commits; no force push, no history rewrite | Dion | **decided** |
| D-5 | License | none chosen | Dion | open (needed before the DMG leaves Dion's Mac) |
| D-6 | Merge to `main` | After Claude's review verdict `done`, Codex merges the task branch into `main` with a normal merge commit and pushes. Stacked branches merge in order (t-001, then t-002, …). No force push, no rebase of published branches. | Dion | **decided** 2026-11-03 |
| D-7 | Test-partner build | Full MVP (T-001…T-013) as ad-hoc signed `aarch64` DMG, **Strict P2P only** (ADR-011, G-1 addendum), owner checkpoints L1–L5 first (ROADMAP) | Dion | **decided** 2026-11-03 |
| D-9 | Stacked development while T-001/T-002 owner gates are open | Owner request 2026-11-03 ("substantial working app") → lanes A/B/C branch from `c93fbe2`; reviewed lanes merge into `m1-integration`, not `main`; if T-002 changes later, merge it into the lanes (no rebase). Gates for `main` unchanged | Claude (within D-4/D-6) | **decided** 2026-11-03 |
| D-8 | KQ-008 "Join the Agent H+" action meaning/label | pending owner answer; needed before T-011 spec | Dion | open |

## 6. Log

- 2026-11-03 — Codex lead: published T-014 `6aca159` (approved planning merged normally;
  source identical to checked `026ff54`), remote SHA verified. Claude exact-head review
  `61abebd8-e4a5-465c-89d6-22d9570b471a` is active. Replaced only the verified original
  scaffold run with T-014 `npm run dev`, explicit fresh USB file-key profile. Build 20.72 s;
  launcher 99939, own Vite 1420 PID 180, native PID 193; HTTP 200 and onscreen window metadata
  confirmed. No native action/Keychain/owner acceptance claimed. Screen-capture preflight false:
  no screenshot or permission bypass. New window left open; owner creation/registration test
  question pending. Evidence: `Meshyard-build/T014-owner-start-20261103T191327Z`. T-007a and
  T-003 continue independently; main gates unchanged.

- 2026-11-03 — Codex lead: T-014 implementation run
  `d09db809-77d6-40eb-bc82-28d508976880` finished; focused commits `e375076`, `9b6bd95`,
  `026ff54`, report `2d04a7e`. Full `CARGO_NET_OFFLINE=true npm run check` exit 0, 56.65 s:
  65 top-level Rust passes plus existing lock child, 26 frontend passes, 2 ignored Rust entries;
  separate regression reopens the same real USB file-key profile in two fresh test processes.
  No native UI/IPC/ACL/Keychain/owner acceptance is inferred. Next: source/visual inspection,
  exact-head Claude review and a runnable owner window. T-007a corrected run
  `b90bb555-c91f-40aa-92e5-7f1bdc663a42` and T-003
  `bf7148c0-8b41-4fa7-a95f-713332507b12` remain active in separate owned worktrees.
  Claude's local-ingest clarification and explicit effective-UID correction were committed
  unchanged as `386a107` / `0cdb5d8`, pushed and remote-verified. Planning docs will be merged
  into clean task branches normally; main/native gates remain unchanged. No knowledge edits.

- 2026-11-03 — Codex lead: committed Claude's six delivery-planning files unchanged as
  `415c69c` and pushed; remote SHA verified. D-9 now authorizes stacked implementation while
  the original native L1/Keychain/main gates remain open. T-014 lane A is actively implementing
  on `t-014-local-workspace-slice` from `c93fbe2` (Smithers
  `d09db809-77d6-40eb-bc82-28d508976880`). T-007a lane B started
  `3989a6fe-5d36-42e6-a711-ff928f254dba`, then was interrupted cleanly before code after KNOWER
  identified conflicts with PROTOCOL §9 (correlation, stale state, socket UID). Claude's bounded
  clarification run `2ecf3f32-c6b6-4ae2-bfcf-1e0dde35a5f6` is active; A continues. Worktrees,
  workflows, caches and logs stay under `/Volumes/MeshyardProjects`. T-001 dev app remains open
  (native PID 90403; own Vite 1420 PID 90396); this proves startup, not owner acceptance.
  Next: resume B from corrected spec, inspect A's real persisted flow, then exact-head review.
  No new checks, native Keychain test, merge, hook installation or network acceptance claimed.

- 2026-11-03 — Claude: owner screenshot (19:39) shows the native window with only the scaffold
  rectangle — matches the literal L1 rendering criterion, but L1 is recorded only on Dion's
  explicit confirmation; Keychain roundtrip still unverified. Neither gate is closed. Per Dion's
  request for a substantial working app (≥ 90 % of own production code actually used, not a
  coverage target): D-9 stacked lanes; new specs T-014 (local workspace slice, priority) and
  T-007 (KQ-002 single 150 ms budget, KQ-003 bounded outcome enums); T-003 updated with KQ-009 →
  ADR-012 (Meshyard-owned `portmapper =0.19.3`, honest `NotObserved`). GitHub: `gh` not
  authenticated, mapping recorded in ROADMAP. Docs only, uncommitted in the T-002 worktree.

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

- 2026-11-03 — Codex lead: both Claude review files committed unchanged and pushed first
  (`f59aeca` T-001, `dce3613` T-002). Inspected focused fixes `793d0f4`, `753ca63`, `ce34a79`
  and report `1b91945`: immutable owner, transactional derived membership/epoch with 5,000-event
  counting regression, and proof of the already-existing outer-envelope guard. Full USB offline
  check exit 0 (68.657 s; 55 top-level Rust + lock child, 3 Vitest; 2 ignored). No wire, generated,
  dependency or review-file changes. Publishing task commits for Claude's renewed review; no
  T-001 merge until Claude records L1 and instructs it, no T-002 merge/T-003 before approval.

- 2026-11-03 — Codex T-002 continuation: preserved committed fix 3 (`793d0f4`) and fix-1 WIP
  after parent-reported Smithers `3719c9c9-a314-4a9d-a249-c256a211bb94` provider-capacity failure.
  Completed fix 1 (`753ca63`): append-only migration, transactional derived membership/epoch,
  one-time verified backfill before cleanup, 5,000-event constant-signature-work regression,
  promotion/removal/reopen/SQL/overflow rollback coverage. Fix 2 (`ce34a79`) is already guarded:
  actual outer MAX_FRAME+1 envelope rejected before verification; normal pagination progresses.
  Targeted core/store suite exit 0 (11.675 s), full offline npm check exit 0 (68.657 s),
  55 top-level Rust passes + lock child, 3 frontend passes, 2 ignored entries. Evidence:
  `/Volumes/Kursmodule/Meshyard-build/orchestration/t002-claude-fixes-continuation-20261103T074000Z`.
  Report includes the earlier unsuccessful fixture compile and infrastructure interruption.
  Returned T-002 to review, never done. No push, merge, model change, agent contact/spawn,
  dependencies/wire/generated changes, GUI/Keychain/network probe or T-003. Parent reviews/pushes;
  Claude decides acceptance. T-001 merge awaits Claude-recorded Dion L1 and instruction;
  manual Keychain remains Dion's isolated Terminal probe, unverified by this worker.

- 2026-11-03 — Codex lead: committed/pushed Claude's nine planning doc edits unchanged as
  `866fcf3`. Bounded USB-only final-source audit `8ea3d48a-9858-4d97-905a-5a2487ef30b7`
  finished in 232.758 s with zero substantiated blockers; both P1 fixes traced, limits recorded.
  Fresh USB validation `ee5a4ba5-cacd-4dfe-9b80-2c54a8227615`: offline npm ci exit 0,
  full offline npm run check exit 0 in 71.633 s (47 Rust + lock child, 3 frontend; two ignored).
  No source/dependency mutation or new native/Keychain/network check. Report and JSON evidence
  updated; T-002 remains review. Both runs finished. Stop after push and wait for Claude's
  T-001/T-002 reviews; Dion owns L1, unchanged hook budget belongs to T-007. No merges/T-003 yet.

- 2026-11-03 — Claude (owner-approved plan): decided KQ-005 (one active workspace/endpoint per
  process, mode fixed by genesis) and **ADR-011** (Strict = no relay and no n0 DNS/pkarr; port
  mapping on; tester builds Strict-only). Owner decisions D-6 (merge rule) and D-7 (full MVP
  as Strict-only arm64 ad-hoc DMG for one test partner). ROADMAP: T-013 tester DMG, G-1
  addendum, owner checkpoints L1–L5. T-001: native check = L1, hook latency criterion moved
  unchanged to T-007. Updated ARCHITECTURE §5.1/§10, SECURITY §3, PROTOCOL §10, T-001/T-003
  specs, CLAUDE.md, AGENTS.md. Edits uncommitted in the T-002 worktree; Codex commits them.
  Sequence: T-002 final audit → Claude review T-001/T-002 → merge → T-003.

- 2026-11-03 — USB-only migration completed per Dion: both task branches fetched and
  fast-forwarded from origin; T-002 now has a USB-owned worktree/Git directory. Moved Smithers
  workspace/state from temporary storage; verified all 29 original entries before path updates,
  archived original inputs, checked seven SQLite databases and bundled seven workflow files
  without running jobs. KNOWER explicitly confirmed readiness; five unchanged knowledge files
  committed separately as `a101deb`, including advisory KQ-008 (DMG/onboarding planning pointer
  for Claude, CTA meaning still awaiting Dion). Updated §3 on both task branches; no source,
  task acceptance or product-scope changes. No new app checks/native test or implementation run;
  old checkouts preserved. Final-source T-002 audit is the next bounded USB-only run.

- 2026-11-03 — Codex T-002 fix worker: both P1 findings at `569b879` addressed. Canonical
  root-down directory trust uses safe effective-UID and macOS ACL checks; no secret-read exploit
  is claimed. Never-confirmed-author quotas now cover contiguous and gap SQL rows together,
  survive reopen, reset held heads on overflow, and preserve established history. Added
  deterministic substitution/ACL, quota/reopen/membership/resend and SQL rollback regressions.
  Code commits `8e64216` (directory trust) and `9b9931f` (durable quotas) are local only.
  Full offline `npm run check` exit 0: 47 top-level Rust tests + lock child, 3 frontend tests,
  768 property cases, unchanged 48 golden/frame vectors; two binding generator runs reproduce
  all 14 SHA-256 hashes. Report renamed to `docs/reports/T-002.md`; see final validation there.
  Renewed final-source review requested; never marked done. Keychain Unavailable was not
  retried; no GUI launch, T-003, additional agent/workflow, push or native acceptance.

- 2026-11-03 — Codex T-002: review requested on `t-002-core-store-identity` (base `330dd71`,
  implementation `263544b81a1621f6367b2dd6c3f57cc30436cac6`).
  Full offline `npm run check` passes: 33 Rust tests + one process-lock child helper, 3 frontend
  tests; 30 task tests, 512 property cases, 48 golden/frame vectors, 14 reproducible UI bindings.
  Isolated native Keychain probe failed with Unavailable; no successful Keychain roundtrip or
  native acceptance claimed. File identity/locking and no-replacement errors are tested locally.
  See `docs/reports/T-002.md` and Architecture §11 additions (pending Claude review). T-001 review
  and its acceptance gaps remain unchanged. Local commits only; parent publishes after audit.

- 2026-11-03 — Codex T-002: implementation started in the isolated `t-002-core-store-identity`
  worktree at `330dd71`, under the owner exception recorded above. T-001 remains review
  with its native and first-launch acceptance gaps. No push; parent publishes after audit.
  Plan: frozen wire/domain/signatures/fold → threaded store/identity → offline checks/report.

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
