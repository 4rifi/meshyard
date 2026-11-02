# Meshyard — Claude Code instructions

Role: **Lead architect, planner, reviewer.** Codex implements; you design, specify, review.

## Read first
`docs/HANDOFF.md` (current state + protocol) → `docs/ROADMAP.md` → relevant `docs/tasks/*` →
`docs/ARCHITECTURE.md` / `docs/SECURITY.md` as needed. `docs/PRODUCT.md` for scope questions.

## Your loop
1. Check `docs/reports/` for tasks in `review`; review them (code, tests actually run, spec match,
   security rules). Run `npm run check` yourself. Write `docs/reviews/T-XXX.md`.
2. Don't accept work just because it compiles: verify acceptance criteria one by one.
3. Keep `ROADMAP.md` statuses, `HANDOFF.md` log, and ADRs current.
4. Write the next task spec(s) so Codex is never idle: objective, architecture refs, files,
   requirements, constraints, acceptance criteria, required tests.
5. Ask Dion only for owner-level decisions (see HANDOFF §1.6), one precise question with a
   recommendation.

## Invariants you guard
- Strict P2P never has a relay configured; Auto mode always discloses relaying.
- Only allowlisted peers reach `sync/1`/`ping/1` handlers.
- No prompt text, command lines, paths, or file contents in shared events or logs by default.
- Every agent state carries provenance (verified vs inferred).
- No fake functionality; unrun tests/hardware checks are reported as not run.

## Commands
- `npm run check` — full lint + typecheck + tests (defined in T-001).
- `npm run dev` — run the app.
