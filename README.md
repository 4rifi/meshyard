# Meshyard

Local-first desktop workspace scaffold. Product behavior is not implemented yet.

## Requirements

- Rust 1.99.1 via rustup (pinned in `rust-toolchain.toml`).
- Node 26.7.0+ and npm 11.19.0+.
- macOS with Xcode and its command-line tools for the native app.

## USB development (T-014 local workspace slice)

The current development slice creates and reopens a real SQLite workspace, saves signed
profile changes and registers local agents. Registered agents show **No events yet** until
activity arrives; this slice does not start networking or install hooks. Native acceptance
and the normal Keychain path remain separate owner checkpoints.

Run commands from the isolated USB worktree, with the external APFS image mounted:

```sh
source /Volumes/TokensAbuse/env.sh
npm ci
npm run bindings:generate
CARGO_NET_OFFLINE=true npm run check
```

Dependencies must be physically installed in this worktree; do not symlink `node_modules`.
The environment keeps Rust targets/toolchains, npm caches, temporary files and logs under
`/Volumes/TokensAbuse`. Do not use the historical internal-disk or
`/private/tmp` recovery checkouts for development.

For a future native demo, explicitly select a fresh private USB file-key development profile:

```sh
source /Volumes/TokensAbuse/env.sh
export MESHYARD_DATA_DIR="$(mktemp -d /Volumes/TokensAbuse/tmp/t014-demo.XXXXXX)"
export MESHYARD_KEY_STORE=file
# Only after the owner has freed/configured the dev port for this worktree:
npm run dev
```

The original T-001 native app on port 1420 is a separate checkout. Do not kill it or reuse
its server to demonstrate T-014. The native command above was **not** run for T-014 validation.
Keep the same data directory and explicit `file` setting on subsequent launches to reopen
the same workspace, profile and agents. Do not share that development identity directory.
Without `MESHYARD_DATA_DIR` the packaged app uses Tauri's application data directory; without
`MESHYARD_KEY_STORE=file` it uses macOS Keychain. Identity failures never switch backends.
Unknown key-store values fail with `Configuration`.

Demo steps: create **Demo / Dion / Strict**, register **Claude Code / main**, edit the display
name, then quit and reopen with the same profile. The agent should remain **No events yet**
without provenance. A second instance of that profile must show `DataDirectoryBusy`.
If creation saves genesis but fails to save the profile, `PartialCreate` explicitly reports
this and offers a display-name repair; do not create another workspace. After a write error,
reload saved state before retrying, since the write may have persisted before a view failed.

The real non-GUI command integration tests use fresh explicit file-key profiles under the
USB `TMPDIR` and close/reopen the store; they do not launch a WebView or query Keychain:

```sh
source /Volumes/TokensAbuse/env.sh
cargo test --offline -p meshyard-app --test local_workspace
```

`npm run bindings:generate` runs the core exporter followed by the app DTO exporter; do not
edit generated bindings manually. `check` runs formatting, Clippy, workspace tests, TypeScript,
ESLint, Vitest and Prettier. `npm run build:frontend` only builds frontend assets. App command
permissions allow exactly the four local workspace commands. CSP is unchanged: installed Tauri
2.12.4 first tries custom-protocol fetch and falls back to its postMessage transport if CSP
blocks it; actual native IPC under this policy still needs owner verification.

Evidence and explicit native/CSP boundaries: `docs/reports/T-014.md`. No numeric runtime-use
percentage is claimed. See `docs/PROTOCOL.md` for normative guarantees; T-014 does not change
wire v1. No license has been chosen (D-5); `LICENSE` is intentionally absent.

## Optional external Rust build directory

The rework environment sets `CARGO_TARGET_DIR` inside `/Volumes/TokensAbuse`;
keep all build artifacts and caches on that APFS image. Cargo and the Tauri CLI use it for Rust artifacts; the debug bundle is
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
