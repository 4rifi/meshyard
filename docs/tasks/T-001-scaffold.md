# T-001 — Repository scaffold

Status: review (open native/latency acceptance items) · Owner: Codex · Reviewer: Claude

## Objective
A buildable, testable skeleton that matches `docs/ARCHITECTURE.md §3`, so every later task only
adds code. No product features yet.

## Prerequisites
- **Step 0 (owner-approved, D-1):** install the Rust stable toolchain user-level with the official
  rustup installer (`https://sh.rustup.rs`, default profile, no system-wide changes). Verify with
  `rustc --version && cargo --version` and record versions in the report.
- Xcode is installed (`/Applications/Xcode.app`). Node v26 + npm available.
- **Git (D-4):** first make the initial commit on `main` with the existing planning docs
  ("Add planning and architecture docs") and push `main`. Then work on branch `t-001-scaffold`,
  commit focused changes, push the branch. Never force-push.

## Scope
1. Cargo workspace (`resolver = "2"`, edition 2024 if supported by the pinned toolchain) with
   crates: `meshyard-core` (lib), `meshyard-store` (lib), `meshyard-net` (lib),
   `meshyard-agents` (lib), `meshyard-hook` (bin), `meshyard-node` (bin), and the Tauri crate
   `meshyard-app` in `app/src-tauri`.
   - Each lib has a minimal public API stub + one real unit test (no `todo!()` in public paths).
   - `meshyard-hook` depends only on std + `serde`/`serde_json`.
   - Workspace lints: `unsafe_code = "forbid"`, clippy `all` warn, `unwrap_used` deny in non-test code.
   - `rust-toolchain.toml` pinning the stable version you installed.
2. Tauri 2 app in `app/`: React 19 + TypeScript strict + Vite. Window shows "Meshyard" and a
   placeholder Pixi canvas (PixiJS v8, one sprite or rectangle) to prove the renderer works in
   WKWebView. Strict CSP, no remote URLs, minimal capabilities (no shell plugin).
3. Frontend tooling: ESLint (typescript-eslint, strict), Prettier, Vitest with one test.
4. Root scripts (in `package.json` at repo root or `app/`, your call — document it):
   - `npm run check` → `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && (frontend) tsc --noEmit && eslint && vitest run`
   - `npm run dev` → `tauri dev`
   - `npm run build` → `tauri build` (debug bundle acceptable for now)
5. `.gitignore` (target/, node_modules/, dist/, .DS_Store, *.sqlite, app data), `.editorconfig`,
   `LICENSE` placeholder → **ask Claude/owner, do not pick a license yourself**; leave
   `LICENSE` absent and note it in HANDOFF.
6. Fill `docs/ARCHITECTURE.md §11` with exact pinned versions (tauri, @tauri-apps/cli,
   @tauri-apps/api, react, vite, typescript, pixi.js, iroh (add as dependency of meshyard-net now,
   features `tls-ring` or the default crypto feature), rusqlite, tokio, tracing, ts-rs, serde).
   Verify each is a current stable release; prefer versions ≥ 2 weeks old.

## Out of scope
Networking logic, SQLite schema, UI beyond placeholder, CI config.

## Acceptance criteria
- [x] `npm run check` passes from a clean clone (after `npm ci`).
- [ ] `npm run dev` opens a window with the Pixi placeholder rendered (state honestly in the
      report whether you could visually verify; a screenshot path in the report if you can).
- [ ] `cargo build -p meshyard-hook --release` binary starts and exits in < 20 ms with empty stdin
      (measure and report).
- [x] `meshyard-net` compiles against iroh 1.x (just constructing nothing — a `pub fn version()`
      re-export is fine).
- [x] No hand-edited generated files; lockfiles committed as generated.
- [x] Report written to `docs/reports/T-001.md` (what was done, commands run + results, deviations,
      open questions).

## Acceptance adjustments (Claude, 2026-11-03, owner-approved plan)
- Native visual check: performed by Dion as owner checkpoint **L1** (`npm run dev` from the USB
  checkout); Claude records the result in `docs/reviews/T-001.md`.
- Hook first-launch `<20 ms`: criterion **unchanged**, but verified in T-007 (where the hook does
  real work) and measured from the installed app on the internal disk as well as from USB.
  T-001 is not blocked by it.

## Recovery status (2026-11-02, updated by Codex, pending review)

Initial recovery kept **in-progress** pending parent native visual verification and the unchanged <20 ms hook
criterion. In `/private/tmp/meshyard-t001-recovery-checkout` (fresh clone at `fc749ca`), `npm ci`
and the complete `npm run check` passed: five Rust tests across the full seven-package
workspace, three Vitest tests, formatting, Clippy -D warnings, TypeScript, ESLint, and Prettier.
The real Tauri CLI built a debug `.app` at
`/Volumes/Kursmodule/Meshyard-build/target/debug/bundle/macos/Meshyard.app`; static inspection
confirmed the production `tauri/custom-protocol` feature and all 11 embedded frontend assets.
The installed exact 1.99.1 pin resolved through external RUSTUP_HOME; target/cargo/npm caches
also used the external build environment documented in README and the appended report.

The release hook built successfully. Of 51 silent launches, the first was 289.108791 ms;
50 warm launches had median 2.026833 ms and maximum 3.010916 ms. The earlier 442.321500 ms
first-launch failure remains recorded; the <20 ms criterion is still unmet. No native window,
WKWebView rendering, or screenshot was verified by this worker. Historical ENOSPC/build failures
are preserved in `docs/reports/T-001.md` alongside the successful recovery. Do not start T-002
under this worker authorization.

## Review handoff (2026-11-03, updated by Codex, pending review)

Full `npm run check` and native `npm run build` pass after the Pixi CSP repair. A real headless
production-CSP regression fails before the fix and passes after it with the rectangle rendered;
the CSP still blocks dynamic evaluation. Native retest of the corrected bundle is pending:
Computer Use denied Meshyard screenshot access. The first-launch hook limit remains unmet;
the owner's decision on Claude's proposed criterion is pending. Review is requested with those
open acceptance items, not done/accepted. See `docs/reports/T-001.md` for real evidence and limits.
