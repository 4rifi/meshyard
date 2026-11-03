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

- Planning baseline: `0f8d52f` on `main`, committed and pushed by parent.
- T-001 branch: `t-001-scaffold`; CSP fix `9e31b6c`, full check and native build pass; review requested with native/first-launch acceptance open.
- Active safe checkout: `/private/tmp/meshyard-t001-recovery-checkout`; original Documents checkout is incomplete and was not repaired or overwritten.
- Rust toolchain, caches and build output: `/Volumes/Kursmodule/Meshyard-build` (USB APFS image must stay mounted).
- Toolchain verified: Rust/cargo 1.99.1 (parent installed via official rustup), Node 26.7.0, npm 11.19.0; Xcode present.

## 4. Task board

| Task | Status | Assignee | Notes |
|---|---|---|---|
| T-001 scaffold | review | Codex → Claude | Full check/build pass; corrected CSP rendering proven headlessly; native retest blocked by tool permission and first-launch latency still over budget; no acceptance claimed |
| T-002 core/store/identity | review | Codex → Claude | Core/wire/fold, threaded store and identity implemented; full offline check green; isolated native Keychain probe unavailable; see `docs/reports/T002.md` |
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

- 2026-11-03 — Codex T-002: review requested on `t-002-core-store-identity` (base `330dd71`,
  implementation `263544b81a1621f6367b2dd6c3f57cc30436cac6`).
  Full offline `npm run check` passes: 33 Rust tests + one process-lock child helper, 3 frontend
  tests; 30 task tests, 512 property cases, 48 golden/frame vectors, 14 reproducible UI bindings.
  Isolated native Keychain probe failed with Unavailable; no successful Keychain roundtrip or
  native acceptance claimed. File identity/locking and no-replacement errors are tested locally.
  See `docs/reports/T002.md` and Architecture §11 additions (pending Claude review). T-001 review
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
