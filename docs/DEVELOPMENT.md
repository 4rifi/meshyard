# Development

Build only in the mounted APFS image. Source `/Volumes/TokensAbuse/env.sh` before
every command. Rust, npm, target, temporary profiles and logs stay in that image.
The current branch is `rework`; local focused commits only, no push.

```sh
source /Volumes/TokensAbuse/env.sh
npm ci
npm run bindings:generate
npm run check
cargo test --workspace
```

Rust 1.99.1 is pinned in rust-toolchain.toml. Node 26.7/npm 11.19 and Xcode
are required. Generate IPC bindings with the command above; never edit generated
IPC, permissions or lockfiles by hand. Frozen signed wire v1 is unchanged.

## Isolated native profile

Set a private APFS data directory and an explicit file key store for tests only.
`MESHYARD_HOOK_HOME` selects an isolated hook configuration HOME for the native
installer, independent of OS home-directory lookup. No constructor enables an
installer; native attachment or explicit test injection does. Planning is read-only.

```sh
source /Volumes/TokensAbuse/env.sh
export MESHYARD_DATA_DIR=$(mktemp -d /Volumes/TokensAbuse/tmp/solo.XXXXXX)
export MESHYARD_HOOK_HOME=$(mktemp -d /Volumes/TokensAbuse/tmp/hooks-home.XXXXXX)
export MESHYARD_KEY_STORE=file
cargo build -p meshyard-hook
export MESHYARD_HOOK_BIN="$CARGO_TARGET_DIR/debug/meshyard-hook"
npm run dev
```

Ordinary packaged use requires none of these variables: Tauri app data directory,
macOS Keychain and Contents/MacOS/meshyard-hook are the defaults. Unknown key-store
values fail. Keychain errors never create a replacement identity or select files.
Keep a test profile and its original key-store setting together when reopening.

## Executable-to-store smoke

This explicitly ignored test requires a built executable and creates its own HOME,
settings and file-key profile on APFS. It applies reviewed plans for both kinds,
sends documentation-derived fixtures through the real executable/socket, checks
persisted station/provenance/connection changes, reopens the profile, and removes
hooks. It never reads or writes real agent configurations.

```sh
source /Volumes/TokensAbuse/env.sh
cargo build -p meshyard-hook
MESHYARD_TEST_HOOK_BIN="$CARGO_TARGET_DIR/debug/meshyard-hook" cargo test -p meshyard-app --test local_workspace isolated_hook_executable -- --ignored --nocapture
```

This proves executable/socket/store routing, not actual CLI emission or packaged
app behavior. Real CLI capture, Keychain and packaged GUI acceptance need separate
evidence. Codex fixture provenance and unsupported events are recorded beside the
fixtures. No PostModelRequest outcome is invented from error text.

## Packaging handoff

Phase 3 must bundle the target-suffixed Tauri external binary so its installed
name is `Contents/MacOS/meshyard-hook`, ad-hoc sign it with the app, and rerun the
smoke using the copied app executable. The resolver derives this sibling path.
Installation from a DMG, App Translocation or any app outside `/Applications` is
blocked with move-and-reopen guidance. A development binary outside an app can be
selected explicitly. Keychain positive acceptance is still required; do not replace
its backend in response to an environment failure.

`npm run build:frontend` only builds frontend assets. `npm run check:csp` uses
externally supplied image-local Playwright/Chromium/PNG tools; see its script for
required environment variables. Browser mocks never count as native acceptance.
