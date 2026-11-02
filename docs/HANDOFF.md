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
| **Knower** (Codex, project-knowledge agent) | Being created by Dion 2026-11-02. Purpose: answers questions about the project when Claude or the implementer is unsure. Thread id: _pending_ — Claude asks via `codex exec resume <id>` / `codex queue` once known; answers that change architecture still go through ADRs. |
| Repository files (this board, tasks, reports, reviews) | Primary, always-available channel. |

## 3. Current state

- Planning baseline: `0f8d52f` on `main`, committed and pushed by parent.
- Active branch: `t-001-scaffold`; source `148b730`, report `7cf63b2`; lead developer publishes the branch.
- Toolchain verified: Rust/cargo 1.99.1 (parent installed via official rustup), Node 26.7.0, npm 11.19.0; Xcode present.

## 4. Task board

| Task | Status | Assignee | Notes |
|---|---|---|---|
| T-001 scaffold | in-progress | Codex | Scaffold committed; full Rust/native validation blocked by disk space; first-launch latency over budget; see docs/reports/T-001.md |
| T-002 core/store/identity | ready | Codex | after T-001; **revised** — normative spec now `docs/PROTOCOL.md` |
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
