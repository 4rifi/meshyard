# Meshyard

Local-first desktop workspace scaffold. Product behavior is not implemented yet.

## Requirements

- Rust 1.99.1 via rustup (pinned in `rust-toolchain.toml`).
- Node 26.7.0+ and npm 11.19.0+.
- macOS with Xcode and its command-line tools for the native app.

## Run from the repository root

```sh
source /Users/dionarifi/.cargo/env # or your own ~/.cargo/env
npm ci
npm run check
npm run dev
npm run build
```

All npm scripts and the generated lockfile live at the repository root. `check` runs Rust
formatting, workspace Clippy and tests, strict TypeScript, ESLint, Vitest, and Prettier.
`build` creates a local debug `.app` bundle; signing, notarization, and distribution are later work.
`npm run build:frontend` only builds frontend assets. `npm run icons` regenerates Tauri icons
from the original placeholder SVG. No networking endpoint is constructed by this scaffold.

See `docs/HANDOFF.md`, `docs/ARCHITECTURE.md`, and `docs/reports/T-001.md` for protocol and evidence.
No license has been chosen (D-5); `LICENSE` is intentionally absent.
