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

## Visual evidence and packaging art

`assets/meshyard-icon.svg` is original pixel geometry; `npm run icons` generates
the platform icons. `assets/dmg-background.svg` is the original 640×440 installer
window source. `node scripts/render-dmg-background.mjs` renders its licensed local pixel font
and produces the 640×440 PNG. Phase 3 configures DMG icon locations. Local
Pixelify Sans and its exact OFL are in `app/src/assets/fonts/`; body text stays
system sans for readability. No remote assets are requested.

The browser evidence command is `node scripts/pixel-qa.mjs`. It uses the existing
local Chrome with image-local Playwright tooling, production CSP and isolated IPC
fixtures. Screenshots and viewport/DPR manifests go to the APFS dist directory.
It is visual/accessibility evidence, not native package acceptance.

For bundle smoke outside /Applications, run the copied bundled hook directly in
the existing executable-to-store test using MESHYARD_TEST_HOOK_BIN. Do not weaken
production installer location checks. Native file-store smoke remains explicit;
default production-service Keychain first launch is an owner checkpoint. The
real-HOME unique-service Keychain probe passed; see the rework report.

### Focused solo view regression

The primary scene has eleven places (including hatched, unsupported Review/Fix/Waiting),
in 4 columns on desktop or 2 on narrow windows. Each logical station is 80×64.
Hydrated DTOs spawn at their real state; only subsequent observed transitions
walk. Slot reservations are per actual state, never a global Arrival index.
True crowded stations use sparse labelled overflow plots; no empty complete map
is allocated merely because three agents exist at different stations.

The screenshot harness now records both full-page and actual viewport PNGs at
1120×900 DPR2, 960×680 DPR1, 375×800 DPR2 and 320×680 DPR2, including a three-agent
regression and a real mock-DTO reduced-motion transition. These remain synthetic
IPC evidence. Profile save and review actions have explicit viewport visibility
assertions. Yard, Agent list and Settings are separate reachable views.
