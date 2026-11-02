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

## Temporary T-001 recovery environment

Validation currently runs from the isolated checkout
`/private/tmp/meshyard-t001-recovery-checkout` (baseline `fc749ca`). Keep the external
`/Volumes/Kursmodule` volume mounted while building or running the resulting bundle.
From that checkout's root:

```sh
export PATH="/Users/dionarifi/.cargo/bin:$PATH"
export RUSTUP_HOME=/Volumes/Kursmodule/Meshyard-build/rustup
export CARGO_HOME=/Volumes/Kursmodule/Meshyard-build/cargo
export CARGO_TARGET_DIR=/Volumes/Kursmodule/Meshyard-build/target
export npm_config_cache=/Volumes/Kursmodule/Meshyard-build/npm-cache
unset RUSTUP_TOOLCHAIN
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_RELEASE_DEBUG=0
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2
npm ci
npm run check
npm run build
cargo build -p meshyard-hook --release
```

The actual `rust-toolchain.toml` pin must resolve to 1.99.1; no stable override is used.
The expected debug bundle is
`/Volumes/Kursmodule/Meshyard-build/target/debug/bundle/macos/Meshyard.app`.
Native window/Pixi verification is performed separately by the parent with `npm run dev`.
The recovery worker leaves T-001 in progress until that verification and the hook latency
acceptance decision; see the appended recovery evidence in `docs/reports/T-001.md`.
