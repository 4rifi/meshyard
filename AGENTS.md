# Meshyard — Codex instructions

Role: **Lead developer / implementer.** Claude Code is the architect and reviewer.

## Start of every session
1. Read `docs/HANDOFF.md` (protocol, current state, open decisions).
2. Pick the lowest-numbered `ready` task in `docs/ROADMAP.md` whose dependencies are `done`.
3. Read its spec in `docs/tasks/`, plus the referenced sections of `docs/ARCHITECTURE.md` and
   `docs/SECURITY.md`.

## While implementing
- Follow the spec. If it is wrong or impossible (e.g. an iroh API differs), make the smallest
  reasonable, reversible choice, document it in your report, and update the doc section marked
  "updated by Codex, pending review". Bigger architectural deviations: add to HANDOFF "Open
  decisions" and stop that part.
- Rust: no `unsafe`, no `unwrap()`/`expect()` outside tests, `thiserror` for library errors,
  `tracing` for logs, never log secrets/prompts/commands/paths.
- TypeScript: strict, no `any`. Generated files (`app/src/ipc/generated/`, lockfiles,
  Tauri gen) are never edited by hand.
- Tests must not need the public internet; mark optional internet tests `#[ignore]`.
- Keep dependencies minimal; justify each new one in the report.

## Done means
- `npm run check` passes and task-specific tests pass — paste the real command output summary
  into `docs/reports/T-XXX.md`.
- Report lists: what was done, files, commands + results, deviations, gaps, questions.
- Set the task to `review` in ROADMAP + HANDOFF. Never set your own task to `done`.
- Never claim a run, screenshot, or hardware test that did not happen.

## Git
- No `Co-Authored-By` or "Generated with …" lines. No force push, no history rewrite.
- Commit/push only as defined in HANDOFF decision D-4.
