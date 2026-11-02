# T-001 — Repository scaffold

Status: in-progress · Owner: Codex · Reviewer: Claude

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
- [ ] `npm run check` passes from a clean clone (after `npm ci`).
- [ ] `npm run dev` opens a window with the Pixi placeholder rendered (state honestly in the
      report whether you could visually verify; a screenshot path in the report if you can).
- [ ] `cargo build -p meshyard-hook --release` binary starts and exits in < 20 ms with empty stdin
      (measure and report).
- [ ] `meshyard-net` compiles against iroh 1.x (just constructing nothing — a `pub fn version()`
      re-export is fine).
- [x] No hand-edited generated files; lockfiles committed as generated.
- [x] Report written to `docs/reports/T-001.md` (what was done, commands run + results, deviations,
      open questions).

## Recovery status (2026-11-02, updated by Codex, pending review)

Scaffold source and generated lockfiles are prepared. Keep **in-progress**: full workspace
Clippy/tests, iroh compilation and native bundling are not verified because local builds ran
out of disk space. Frontend checks/build and focused Rust checks passed. Vite startup and local
HTTP were observed; no native window or screenshot was verified. The first release-hook
invocation exceeded 20 ms; warm runs passed. Detailed commands and evidence are in
`docs/reports/T-001.md`. Do not start T-002 under this worker authorization.
