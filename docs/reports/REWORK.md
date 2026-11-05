# Rework — Phase 0 completion and Phase 1 consolidation

Scope: only the current owner-authorized environment and consolidation phase on
`rework`, starting at `39d7541293f6a224a5157a4c3478164058f0b1af`.
No Phase 2 or new P2P implementation, push, historical session contact, nested
worker, real agent configuration change, or knowledge-file edit.

## Environment and merge decisions

All commands source `/Volumes/TokensAbuse/env.sh`. The checkout, toolchain,
download caches, build target, test HOME and temporary data stay on the APFS image.
The existing mount was reused without attachment. The environment report from
Phase 0 records APFS with owners enabled; this worker's `diskutil info` could not
use the DiskManagement framework. `df -h` confirms `/dev/disk7s1` mounted at
`/Volumes/TokensAbuse`, with 44 GiB available during compilation.

Verified: rustc 1.99.1 (`58cf8f9c3`), cargo 1.99.1 (`5f94df478`), installed target
`aarch64-apple-darwin`, Node v26.7.0, npm 11.19.0, Xcode 26.6 (17F113).
Initial Xcode tool shims attempted `/var/folders/.../xcrun_db-*` cache creation;
the sandbox rejected those writes. The image-local environment now sets explicit
Xcode clang/linker and SDK paths; `xcrun --no-cache` alone still emitted cache
creation denials. No system protection or real agent configuration was changed.

`git merge-base --is-ancestor <ref> HEAD` initially returned 0 for
`origin/t-007a-hook-normalizer`, and 1 for both `origin/t-001-scaffold` and
`origin/t-003-net-endpoint`. Both absent branches were normally merged:

- `7b58833` — Merge scaffold updates into rework.
- `20de52c` — Merge strict network endpoint into rework.

The first Git write was sandbox-blocked (exit 128). The authorized Git metadata
retry reached the expected content conflicts (exit 1); resolution commits passed.
Scaffold conflicts preserved the advanced app documentation/current task rows and
retained incoming CSP instructions and historical log entries. Network conflicts
preserved projection/ingest ADRs 014/015, added TLS ADR-013, retained both log
histories and imported the existing reviewer-accepted T-003 status. No historical
task was self-certified done. Signed wire v1 remains unchanged.

Cargo.lock was seeded from pre-merge HEAD and resolved by `cargo fetch`, not
hand-edited. This preserves existing pins while adding the endpoint dependency
edges and vendored iroh source. `._*` joins the existing `.DS_Store` ignore rule.
README development commands now use this image's environment and temporary paths;
the full user-first README rewrite belongs to Phase 2.

## Dependency installation and generated bindings

Evidence directory: `/Volumes/TokensAbuse/logs/consolidation/`.
Each `.json` result records command, exit code and elapsed seconds where measured.
`setup-results.json` records setup and merge results without invented timings.

- `cargo fetch`: exit 0 (`cargo-fetch.log`). Required because binding exporters
  deliberately run Cargo offline.
- `npm ci --no-audit --no-fund`: exit 0, 193 packages (`npm-ci.log`), with npm 11's
  explicit install-script approval warning for `fsevents@2.3.3`.
- Reviewed installed pinned package metadata and native artifact, then
  `npm install-scripts approve fsevents --allow-scripts-pin`: exit 0.
  Approval is project-local `allowScripts`, not global npm configuration.
- `npm rebuild fsevents`: exit 0. Native import reports `watch` as a function
  (`npm-scripts.log`).
- Repeated `npm ci --no-audit --no-fund` with temporary APFS HOME: exit 0,
  193 packages, no missing-approval warning (`npm-ci-approved.log`).
- `npm run bindings:generate`: exit 0 (`bindings.log`). Core/app exporters and
  Prettier ran; final generated binding diff is empty.
- `npm run check:frontend`: exit 0, 31.377 s; TypeScript and ESLint pass,
  six Vitest files / 56 tests pass (`frontend.log`, `frontend.json`).
- `npm run format:check`: exit 0, 1.757 s; all matched files use Prettier style
  (`prettier.log`, `prettier.json`).

## External-only safety regression fixture

The existing `canonical_tmp_aliases_work_under_root_owned_sticky_parent` test
hardcoded `/tmp`, creating an identity and SQLite database on the internal disk.
It now accepts `MESHYARD_STICKY_TEST_ROOT` and checks root ownership plus mode
1777 **before writing**. Without the override it honors `TMPDIR`, falling back to `/tmp` only when neither
variable is set; an unsuitable parent fails before writes.
All existing identity/store assertions remain; no test is removed or weakened.
For this run the override points into APFS. A root-owned sticky fixture cannot
be provisioned here: authorized `sudo -n /usr/bin/true` exits 1 with
`sudo: a password is required`. No password prompt or internal-disk fallback was
used. The full regression therefore cannot pass until such a fixture is provided.
A temporary test HOME lives at `/Volumes/TokensAbuse/tmp/phase1-home`.

## Validation status

Full-check and explicit workspace-test outcomes are recorded below after execution.
Native app/Keychain, real hooks, screenshots, packaging/DMG and network hardware
acceptance are outside this assigned phase and not validated.

### First full runs and mount guard correction

`npm run check` exited 101 after 291.631 s (`full-check.{log,json}`). Rust fmt
and all-target Clippy passed. App unit tests stopped at 1 pass / 23 failures:
the fixture helpers asserted the historical `/Volumes/MeshyardProjects/` prefix.
The explicit `cargo test --workspace` also exited 101, after 162.959 s including
waiting for the build lock (`workspace-tests.{log,json}`), with the same cause.
These were stale environment guards, not product behavior failures.

All six such assertions in five project test files now require the owner-mandated
`/Volumes/TokensAbuse/` prefix. This retains the external-only assertion rather
than deleting it or accepting arbitrary paths. No old mount is accessed. A new
full run follows this source correction; original failures remain in the logs.

`npm run build:frontend` passed (exit 0, 58.891 s;
`frontend-build.{log,json}`). Vite reported the existing large-chunk advisory and
plugin timing advisory. This is a production frontend build, not native evidence.
`cargo fmt --check` passed (exit 0, 2.024 s; `rust-format.{log,json}`).

### Post-path-fix run

`npm run check` exited 101 after 32.516 s
(`full-check-after-path-fix.{log,json}`). All 24 app unit tests and all 12 local
workspace integration tests passed, as did agent/core tests. The hook binary
suite had 3 passes and one existing timing assertion failure:
`full_socket_queue_fails_silently` exceeded its unchanged 400 ms bound.
A single focused `cargo test -p meshyard-hook --test binary` recheck passed all
four tests, exit 0 after 7.729 s (`hook-binary-recheck.{log,json}`). The original
failure remains evidence of timing variability; no timeout/assertion was changed.

### Endpoint and store follow-up

The explicit post-fix workspace run exited 101 after 25.154 s
(`workspace-after-path-fix.{log,json}`): the network crate had 6 passes / 9
endpoint-bind failures. A filtered workspace run excluding only the sticky
fixture also stopped at the network crate (exit 101, 13.393 s;
`unaffected-workspace.{log,json}`). It is **not** a passing partial suite.

Standalone Unix/TCP and IPv4/IPv6 UDP socket preflights all succeeded
(`socket-preflight.json`, `udp-preflight.json`). Therefore an iroh `Bind` failure
alone is not classified as a sandbox denial. A test-only diagnostic exposes the
underlying iroh bind error; production errors still contain only `NetError::Bind`.
A separate store `--no-fail-fast` run ensures later store integration suites are
actually executed, even though the sticky-root regression cannot pass here.

### Confirmed external blockers

The focused `cargo test -p meshyard-net direct_suite_selected_ip_and_real_ping
-- --nocapture` exits 101 after 99.582 s including compilation
(`net-bind-diagnostic.{log,json}`). Exact underlying error:

```text
Failed to create netmon monitor
Caused by:
    actor error
    IO
    Operation not permitted (os error 1)
```

The blocked operation is **creating iroh's network-change monitor**, not ordinary
loopback socket binding. No alternate monitor, permission bypass, feature change
or removed network assertion was used. Direct/relay/Strict endpoint runtime
acceptance requires a context where macOS permits that monitor.

`cargo test -p meshyard-store --no-fail-fast` exits 101 after 93.146 s
(`store-tests.{log,json}`). It executes all store targets: 51 regular passes plus
the successful child-lock test; two pre-existing ignored entries (manual Keychain
and the child helper entry). The only failure is the APFS sticky fixture's new
pre-write owner assertion, actual UID 501 versus required UID 0. Bindings,
identity security and store/projection integration assertions otherwise pass.

To unblock that test, the owner must provision a root-owned 1777 directory **on
the APFS image** and set `MESHYARD_STICKY_TEST_ROOT` to it. A possible command is
`sudo install -d -o root -g wheel -m 1777 /Volumes/TokensAbuse/tmp/root-sticky`;
**not executed**, because noninteractive sudo requires a password. No internal
`/tmp` identity/database test fallback is permitted for this run.

The phase is **not fully accepted** and is not represented as green or ready for
Phase 2 acceptance. No push is recommended until the unfiltered full check and
workspace tests pass in an authorized context with that fixture. Consolidation
source is preserved in focused local commits; later product phases remain unrun.

### Final source checks and honest partial-run limit

- Final `cargo clippy --workspace --all-targets -- -D warnings`: exit 0,
  4.726 s (`final-clippy.{log,json}`).
- Final `cargo fmt --check`: exit 0, 0.303 s (`final-format.{log,json}`).
- The final filtered workspace command, excluding `direct_suite`, `relay_suite`,
  `strict_suite` and the sticky-root test, exited **101**, 39.336 s
  (`final-partial-workspace.{log,json}`). It stopped at the hook binary suite:
  all four process-launch cases exceeded the unchanged 2 s watchdog. Earlier
  focused hook tests passed, but this later run does **not** validate reliable
  hook startup. No assertion, timeout, OS protection, quarantine or signing
  policy was changed to force a pass. The cause of this later launch delay is
  not established; it must not be called a confirmed product or OS defect.

Accordingly, no filtered workspace pass is claimed. Further repeated full runs
were stopped because the netmon permission and root-fixture blockers are
confirmed and the launch-timing variability remains unresolved. The existing
historical first-launch latency concern is not waived by consolidation.

## Files and local commits

- Merge resolutions: README, HANDOFF, ROADMAP, ARCHITECTURE and Cargo-generated
  lockfile; incoming scaffold/network/vendor changes retained normally.
- `.gitignore`, `README.md`, `package.json`: image paths, AppleDouble exclusion,
  pinned project npm install-script approval.
- `crates/meshyard-store/tests/identity.rs`: configurable root-owned sticky
  fixture, pre-write UID/mode assertions.
- Five test files: six external-only path guards updated to this image.
- `crates/meshyard-net/src/lib.rs`: test-only underlying bind diagnostic;
  production typed error unchanged.
- `docs/HANDOFF.md`, this report: current rework pointer and evidence.
- Image-local `env.sh` and `logs/consolidation/`: environment/cache settings,
  command outputs, structured exits/timings, preflights and blockers.

Focused first-parent commits after the supplied baseline:

1. `7b58833` Merge scaffold updates into rework.
2. `20de52c` Merge strict network endpoint into rework.
3. `95d1682` Keep rework setup and dependency approvals on the APFS image.
4. `2310004` Allow an external root-owned sticky fixture for store tests.
5. `ca3dd99` Point external-only test guards at the current APFS image.
6. `4a44dab` Expose endpoint bind causes in network tests.

A final report-only commit follows. No remote branch was changed.
