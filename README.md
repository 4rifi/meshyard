# Meshyard

Local-first desktop workspace scaffold. Product behavior is not implemented yet.

## Requirements

- Rust 1.99.1 via rustup (pinned in `rust-toolchain.toml`).
- Node 26.7.0+ and npm 11.19.0+.
- macOS with Xcode and its command-line tools for the native app.

## Run from the repository root

```sh
source "$HOME/.cargo/env" # if rustup is not already on PATH
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

## Optional external Rust build directory

Set `CARGO_TARGET_DIR` to an absolute writable directory on another volume if local disk
space is limited. Cargo and the Tauri CLI use it for Rust artifacts; the debug bundle is
then at `$CARGO_TARGET_DIR/debug/bundle/macos/Meshyard.app` instead of
`target/debug/bundle/macos/Meshyard.app`. Keep that volume mounted while building or using
its artifacts. The pinned Rust version still applies. Recovery-specific environments and
dated validation results are recorded in `docs/reports/T-001.md` and `docs/HANDOFF.md`.

## Optional production-CSP regression

`npm run check:csp` runs the existing headless browser regression separately from `check`.
First run `npm run build:frontend` (or `npm run build`) to populate `app/dist`. Supply an
already installed, compatible `playwright-core` module, `pngjs` module and Chromium headless
shell externally; this command installs no packages and downloads no browser. The browser
must support WebGL through ANGLE SwiftShader on the current platform.

Set these environment variables before running the command:

| Variable                       | Required value                                                                |
| ------------------------------ | ----------------------------------------------------------------------------- |
| `MESHYARD_PLAYWRIGHT_MODULE`   | Absolute module file URL for the external `playwright-core/index.mjs`.        |
| `MESHYARD_PNG_MODULE`          | Absolute module file URL for the external `pngjs/lib/png.js`.                 |
| `MESHYARD_CHROMIUM_EXECUTABLE` | Absolute filesystem path to the installed Chromium headless-shell executable. |
| `MESHYARD_CSP_OUTPUT`          | Writable evidence directory for `result.json` and `production-csp.png`.       |

Module file URLs use `file:///...` (including a drive letter on Windows); absolute module
filesystem paths also work on POSIX. Export variables with your shell's normal syntax,
then run `npm run check:csp` from the repository root. Missing or empty variables cause an
explicit setup error. Use a fresh output directory to keep earlier evidence.

The runner serves production assets on an ephemeral loopback port with the exact CSP from
`app/src-tauri/tauri.conf.json`, blocks off-origin requests and checks real rendered pixels
and dynamic-evaluation denial. Its screenshot is headless Chromium evidence; native
WKWebView rendering and native window acceptance require separate verification.
