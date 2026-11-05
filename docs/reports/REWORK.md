# Rework — implementation and validation evidence

Historical Phase 0/1 scope: the owner-authorized environment and consolidation on
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


## Phase 1 accepted normal-context verification (2026-11-05)

The parent rechecked clean SHA `57bc67295d955b86ac61e94da2091eb2a08451a2`
in the authorized normal macOS context with the root-owned APFS fixture:
`npm run check`: exit 0, 47.534 s; `cargo test --workspace`: exit 0, 28.328 s.
Exact evidence: `/Volumes/TokensAbuse/logs/phase1-normal/result.json` and its
`full-check.log` / `workspace-tests.log`. This supersedes the acceptance blocker
for Phase 1 while preserving every historical failure above. Two existing ignored
entries remain: manual Keychain probe (not validated) and the lock-child helper,
which the parent test executes separately. Phase 2 proceeds under the current brief.


## Phase 2 — solo functionality (2026-11-05)

Implementation is present on `rework`, but Phase 2 is **not fully accepted**:
positive packaged/default-Keychain acceptance and the normal-context full check
remain external validation gates. No push is recommended before those gates and
Phase 2 pixel/Phase 3 package review. No historical roadmap task is marked done.

### Changes and production callers

- `meshyard-hook::normalize`, called by the executable, recognizes Codex
  `apply_patch` as Edit and existing Bash/exec_command as Shell. The six compatible
  lifecycle events preserve frozen wire v1 and its provenance rules. Codex
  StopFailure is rejected: the official current documentation has no such event.
  No model-request error text is converted into terminal Error. PostModelRequest
  can retry; KQ-003 also requires a named-version captured structured outcome
  before adding outcome rules. Review/Fix/Waiting remain unsupported, as the UI
  and guides explain. No signed wire/core vocabulary was changed.
- Official reference actually opened/read: https://learn.chatgpt.com/docs/hooks
  on 2026-11-05, using the OpenAI Docs skill. Documentation-derived Codex fixtures
  identify their source and are explicitly not CLI captures. Golden pipeline tests
  cover state/provenance plus unknown-event and privacy rejection.
- `AppState::attach_app` creates independent installers for Claude settings and
  Codex hooks. `MESHYARD_HOOK_HOME` explicitly selects isolated native-test settings
  without relying on OS HOME lookup. Existing explicit `MESHYARD_DATA_DIR` and
  `MESHYARD_KEY_STORE=file` remain; ordinary Tauri app-data/Keychain defaults are
  unchanged. No default/backend fallback or replacement identity was introduced.
- `hooks::bundled_binary`, called at native attachment, resolves the hook beside
  the main executable (`Contents/MacOS/meshyard-hook`), canonicalizing its path.
  An explicit development override remains supported. Plans reject App
  Translocation and app bundles outside `/Applications`, including DMG and temp
  bundle paths, with typed move/reopen guidance. Phase 3 must actually bundle and
  sign that executable; this phase does not claim a packaged binary was tested.
- `Installer::plan/apply`, called through the existing eight IPC commands, now
  handles either kind independently. Exact settings target and hooks diff are
  shown before approval. Six Codex hooks use the documented async shape (Codex
  itself runs SessionEnd synchronously). The existing hash/stale-plan,
  create-only private backups, unrelated-byte preservation, atomic replacement,
  idempotence, symlink/permission rejection and reversible removal remain.
  Backup names use the actual settings filename. No permission manifest was
  hand-edited; the command allowlist is unchanged. DTOs were generated by tools.
- `AppCore::status/bind/hook_apply` uses an independent persisted binding for
  each kind, verifies ownership, and reports each install/connection state from
  the corresponding installer and observed provenance. Registration/binding alone
  cannot claim a connection. Foreign ownership rejection remains tested.
- `WorkspaceScreen` calls the connection selector, whose typed IPC sends the
  selected kind. Codex trust instructions tell users to review exact definitions
  in `/hooks`; there is no trust bypass. List shortcuts select the proper kind.
  `createYard` pointer selection and the accessible agent-name buttons open the
  real-agent inspector with state/provenance/staleness. Settings expose identity
  and privacy limits alongside display-name editing. Existing empty/error states
  and renderer fallback remain. No new dependencies, unsafe Rust or TS any.
- README is user-first; development/profile instructions moved to
  `docs/DEVELOPMENT.md`; `docs/TESTER-GUIDE.md` is German. Pixel polish, unsupported
  decorative stations, final screenshots and packaging belong to subsequent phases.

### Actual checks

Every shell command sourced `/Volumes/TokensAbuse/env.sh`; all profiles, HOME,
fixtures, builds and evidence stayed in the APFS image. Evidence root:
`/Volumes/TokensAbuse/logs/phase2/`. JSON records include actual exits and measured
wall time where available.

| Command | Actual result | Evidence |
| --- | --- | --- |
| `npm run bindings:generate` | exit 0; only AppError and WorkspaceView DTO additions | `bindings.log`, `bindings.json` |
| `cargo test -p meshyard-hook --lib --test binary` | exit 0, 5.528 s; 5 unit + 4 executable tests | `hook-final.*` |
| `cargo build -p meshyard-hook` | exit 0, 2.421 s | `hook-build.*` |
| `MESHYARD_TEST_HOOK_BIN="$CARGO_TARGET_DIR/debug/meshyard-hook" cargo test -p meshyard-app --test local_workspace isolated_hook_executable -- --ignored --nocapture` | exit 0, 18.061 s; 1 explicit smoke | `executable-smoke.*` |
| `npm run check:frontend` after UI review | exit 0, 20.772 s; TypeScript/ESLint, 6 files / 58 tests; no lint warnings | `frontend-reviewed.*` |
| `npm run format:check` before final UI review/docs | exit 0, 0.882 s; later edited files formatted with Prettier | `format-final.*` |
| `npm run check` (one full project run) | exit 101, 77.345 s; fmt and all-target Clippy pass; app/agents/core/hook tests pass; network 6 pass / 9 fail | `full-check.*` |
| `cargo test -p meshyard-store -p meshyard-node` | exit 0, 106.268 s; includes root-owned APFS fixture; no blocked network suite repeated | `remaining-crates.*` |
| `npm run format:check` after final UI and report edits | exit 0, 0.654 s | `format-reviewed.*` |
| isolated Keychain probe, then diagnostic assertion | each exit 101; initial load returned Unavailable rather than NotFound, before record creation | `keychain-probe.*`, `keychain-diagnostic.*` |

Full-check successful test targets: agent pipeline 12, app units 26, local
workspace 12 (explicit executable smoke ordinarily ignored), core 13 + 2,
hook units 5 + binary 4. It then stops at meshyard-net, so it does not claim a
complete workspace pass or execution of later targets. The exact network cause
is `Failed to create netmon monitor` → `Operation not permitted (os error 1)`.
No assertions, features or monitors were bypassed; no repeat of the known-blocked
network suite was attempted. The parent must rerun unchanged in normal context.
The Phase 1 normal-context green result above remains valid only for that SHA.
The separate store/node follow-up passes the later unaffected targets and APFS
fixture; it is not a substitute for the blocked full-check acceptance.

The explicit executable smoke applies both reviewed installers into a temporary
HOME, feeds doc-derived Claude/Codex prompt/edit/stop fixtures through the real
built executable → UID-checked socket → signed/persisted projection → view, checks
Thinking/Building/Finished, provenance and Connected, checks Codex cannot move the
Claude agent, reopens the profile with both states intact, and uninstalls both.
This is real executable/socket/store evidence, **not** CLI emission, bundled app,
Keychain, WebView or installation into a real user's configuration. The smoke's
ignore annotation requires an explicit image-local executable; it was actually
executed by the command above and is not counted as an unrun success.

### Preserved failures and corrections

- `app-tests.log`, exit 101: two existing test-only AppCore literals needed the
  new independent installer field; corrected before passing app runs.
- `frontend.log`, exit 1: old expected no-argument plans and unavailable-Codex
  message no longer matched. Assertions now check exact per-kind IPC arguments,
  selector ownership, Codex trust text and unsupported-event rejection.
- `focused.log`, exit 101: previous one-kind bound-count expectation; replaced
  with equality against exactly the two authorized own identities, while retaining
  foreign-agent rejection. `focused-fixed.log`, exit 101: exact diff header
  expected a hardcoded user path; now the test compares the full exact diff with
  the actual target path. `focused-final.log`, exit 101: old normalizer test
  wrongly expected Codex StopFailure; it now asserts all seven Claude events and
  specifically rejects that nonexistent Codex event. App's 26 units and 12 local
  integration cases passed in that run and the subsequent full check.
- `frontend-final.log`, exit 0: one new effect dependency warning. Final UI review
  corrected that dependency, Codex's displayed kind and the per-kind shortcut;
  `frontend-reviewed.log` is the warning-free final frontend evidence.
- Initial Git staging was blocked at `.git/index.lock` (exit 128). Authorized
  escalation permitted focused local commits; no push or history rewrite.

### Keychain and remaining validation gates

The probe uses a random isolated service/account and disables interaction; the
unchanged requirement is NotFound before create. A diagnostic-only assertion
change exposes the actual typed error without printing key material. It returned
Unavailable before any secret creation. This does **not** establish that sandbox
is the cause or that the production backend is defective. No fallback, key
replacement, agent trust/config mutation or OS permission bypass occurred.
Question/context/options/recommendation are recorded for the parent at
`/Volumes/TokensAbuse/logs/architect-question.txt`; no Dion question or independent
Claude launch was made. Recommended next action: unchanged isolated roundtrip and
ad-hoc packaged native launch in authorized normal context, retaining Keychain.

Not validated here: successful default Keychain roundtrip, packaged ad-hoc
onboarding/IPC, real Claude/Codex CLI event emission/trust, bundled executable,
DMG/translocation OS launch behavior (path rejection is unit-tested), native
inspector/settings keyboard interaction, screenshots or visual approval. No real
agent configuration was changed. Full pixel polish and final DMG acceptance
remain with subsequent assigned phases. Source implementation can proceed to
those phases with these gates explicit; it is not a release approval.

### Focused Phase 2 commits

- `0214cee` Normalize documented Codex lifecycle hooks without invented provenance.
- `e49ff6c` Install and route both agent kinds with stable bundled hook paths.
- `95d829d` Connect Codex in the app and inspect observed agent activity.
- `4614fbd` Expose typed errors in the isolated Keychain probe.
- A focused documentation evidence commit follows.
