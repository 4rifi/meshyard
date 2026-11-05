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

## Phase 2 — pixel UI and user documentation (2026-11-05)

Implemented on isolated `rework`; no push, P2P implementation, production identity
creation or real agent configuration write. Historical reports and roadmap statuses
remain unchanged. The current owner brief overrides the historical T-015 design
freeze and paths. AGENTS.md and CLAUDE.md now enforce this isolated APFS workflow
in a focused documentation commit, rather than directing future agents into old
checkouts or allowing pushes/resumed workers.

### Result and production callers

- App-wide sixteen-color pixel frames, controls, focus rings, error strips, inputs,
  selects and scrollbars build on T-015. Locally bundled Pixelify Sans serves
  headings, controls and map labels; body text remains readable system sans and
  diffs use monospace. The exact OFL 1.1, upstream source and font/license SHA-256
  are retained in `app/src/assets/fonts/`. No new runtime dependency or remote asset.
- `WorkspaceScreen` keeps the persistent T-016 renderer and real DTOs. Its compact
  desktop header gives the yard space. Original distinct Claude worker and Codex
  robot maps are consumed by `pixelSprite` in the production yard. Four poses:
  normal, walking, idle blink and working arm. The low-rate cosmetic pose timer
  never changes state, provenance, routes or positions; it pauses for hidden
  documents and reduced motion and is cleared at destruction. Movement ticker
  still stops at arrival. Reduced motion snaps and resets cosmetic poses.
- Build, Test, Merge and all other true stations retain the same state routing.
  Review, Fix and Waiting remain visibly reserved signs outside the activity map,
  labelled not reported by current hooks. Neither backend nor frozen wire v1 is
  modified. Provenance tags use V/tilde, paired with full inspector/list wording.
- Agent-name buttons remain the keyboard equivalent of canvas selection. Inspector
  close restores its opener; pointer canvas selection falls back to the focusable
  yard viewport. Existing connection panel focus return and reviewed diff behavior
  remain. No simulated event has a production caller.
- Original `assets/meshyard-icon.svg` is consumed by `npm run icons`; all platform
  icons were generated by Tauri tooling. `assets/dmg-background.svg` is the original
  640×440 source; `scripts/render-dmg-background.mjs` renders its local licensed
  font to `assets/dmg-background.png`. Both PNG and icon were visually inspected.
  Phase 3 must wire the DMG background/window/icon positions into the real bundle.
- README remains user-first, with local pixel art/accessibility and honest Keychain
  checkpoint notes added. Developer/profile notes live in DEVELOPMENT; the German
  TESTER-GUIDE covers install, Gatekeeper/quarantine, exact diff approval, Codex
  `/hooks` trust, reversible removal, privacy, unsupported activity and P2P status.

### Actual validation and preserved failures

Every shell command sourced `/Volumes/TokensAbuse/env.sh`. Logs, tooling, temporary
profiles, builds and screenshots remained on APFS. Existing local Google Chrome
was reused; only Playwright Core (one package) was installed in image-local
`tooling/visual`, outside product dependencies. No browser download.

| Command | Actual result | Evidence under `/Volumes/TokensAbuse/logs/pixel/` |
| --- | --- | --- |
| `npm run icons` | exit 0; Tauri-generated platform icon files | `icons.log` |
| `npm run check:frontend` final handoff | exit 0; 6 files, 59 tests, TypeScript/ESLint pass | `frontend-handoff.log` |
| `npm run check` normal context, final corrected run | exit 0, 40.934 s; fmt/Clippy/Rust/frontend/Prettier pass | `check-final-normal.json`, `full-check-final-normal.log` |
| `cargo test --workspace` normal context | exit 0, 30.311 s; no Rust source change in this phase | `checks-normal.json`, `workspace-tests-normal.log` |
| `npm run build:frontend` | exit 0; local font emitted; Vite warns about >500 kB chunk | `production-final.log` |
| Node production asset audit | exit 0; no mockIPC/connectFixture/fixture-plan marker; one local font; CSP unchanged | `production-audit.json` |
| `node scripts/render-dmg-background.mjs` normal context | exit 0; PNG rendered and viewed | `dmg-art.log` |

The first Chrome launch aborted (SIGABRT) inside the restricted process context;
its actual log is preserved as `screenshots.log`. Normal-context execution was
approved and succeeded; no OS privacy setting or permission was bypassed. The
first full normal check exited 1 after 47.855 s solely because two frontend tests
expected the old Building/Testing/Merging station labels. Exact ordered label
assertions were updated to Build/Test/Merge, retaining all state/provenance and
geometry assertions. The corrected full check above passes. The pose regression
test additionally checks exact idle/work pose visibility, invariant positions,
stopped movement ticker, reduced-motion reset and zero timers after destruction.
No assertion was removed or skipped to get green.

### Keychain evidence supersedes the earlier phase summary

Parent evidence at clean `c4b38d061f98660c82d114933b1be1b5d3fe6fca`:
`npm run check` exit 0 / 71.707 s and `cargo test --workspace` exit 0 / 30.038 s
(`/Volumes/TokensAbuse/logs/phase2-solo-normal/result.json`). The unchanged
`HOME=/Users/dionarifi cargo test -p meshyard-store isolated_keychain_roundtrip
-- --ignored --nocapture` passed exit 0 / 3.297 s
(`/Volumes/TokensAbuse/logs/phase2-real-home-keychain/result.json`). This random,
prompt-disabled service probe created, loaded and deleted its own record. Earlier
fake-HOME and normal-context-with-fake-HOME failures remain evidence, not a proven
sandbox-only failure. Keep default Keychain, with no file identity UI or fallback.

Per architect decision recorded in DECISIONS.md, first default-Keychain launch of
the ad-hoc packaged app with the production service is an owner checkpoint, **not
validated**. Do not create that identity unattended. Phase 3 smoke uses explicit
file profiles. To test a copied bundle outside /Applications, run its bundled hook
in the existing executable smoke via MESHYARD_TEST_HOOK_BIN; do not weaken the
production installer location restriction. Actual native packaged rendering,
Gatekeeper, bundled helper, real CLI emission/trust and DMG acceptance remain
Phase 3 checks. Browser mock pictures do not establish any of these facts.

### Screenshot manifest and visual review

The first completed browser runs produced 39 captures with zero errors. The
stronger handoff run then exposed a fractional Y origin after connection layout
changes (10 captures, exit 1; `screenshots-handoff.log`). Root/default paragraph
line heights are now fixed integer pixels, so preceding content cannot introduce
fractional yard origins. Computed-layout diagnosis also identified default h3
em margins (18.71875px); headings now have explicit integer margins. The renderer
also observes the containing layout so preceding panels trigger realignment even
when the yard viewport size stays fixed. The failed origin assertion remains unchanged. Final
recapture result is recorded below.
Preserved failure log: `/Volumes/TokensAbuse/logs/pixel/screenshots-handoff.log`.
Final unchanged-assertion run: **exit 0, 39 captures, zero errors**,
`/Volumes/TokensAbuse/logs/pixel/screenshots-origin-fixed.log`.
Manifest: `/Volumes/TokensAbuse/dist/screenshots/manifest.json`, identifying every
scenario, actual viewport, DPR, canvas origin/dimensions, evidence level and exact
capture command. Cases use 1120×900 @2x, 375×800 @2x and 960×680 @1x, plus actual
640px resize and restoration of the same scene. Each capture is full-page.

All 13 scenarios at all three sizes: onboarding, loading, empty yard, identity
error, partial-create repair, two labelled agent kinds at Build/Test, inspector,
list, settings, connection choice, Claude settings diff, Codex settings diff and
resized layout. Synthetic IPC is explicitly labelled in every image and manifest.
The harness enforces no document horizontal overflow, local font availability,
production CSP without violations, integral canvas scale/origin, keyboard Tab/Enter
access to inspector and restored opener focus. Codex diff excludes StopFailure.

Reviewed actual images with the image viewer, including all 39 through three
contact sheets and individual yard/onboarding/installer images, plus native icon
PNG and raster DMG background. Fixed the oversized desktop header after the first
review. Final captures retain readable body/diff type, crisp tile pixels, distinct
robot/worker art, coherent square controls/error/loading/settings and honest
unsupported signs. Narrow screens use a tall two-column yard and stacked panels;
the page scrolls vertically. Browser screenshots are **not native/live evidence**.
No reference-image match or OS-native visual acceptance is claimed.

Handoff: proceed directly to mandatory Phase 3 DMG packaging, not P2P. No phase-local
blocker remains; production Keychain first launch remains the documented owner
checkpoint. No push is recommended before actual DMG/native acceptance. Local
implementation and evidence commits follow the operational-instructions commit.

The exact font license is also available inside production Settings through a
collapsed keyboard-accessible disclosure, not merely retained in the repository.
The committed production build audit confirms that license text is bundled and
that mock IPC is excluded (`production-audit-committed.json`). Source implementation
commit: `4fc08ee`; operational documentation commit: `5e44b71`.

## Phase 2 — parent visual corrections and accepted browser handoff

The initial 39-image set above was draft visual evidence, **not parent acceptance**.
The parent's findings in `logs/pixel-parent-review.md` and
`control-plane/phase2-visual-fix.txt` superseded that presentation. Current feedback
at `/Volumes/TokensAbuse/logs/parent-feedback.md` accepts the corrected browser
visuals only; native/owner acceptance remains separate. The worker read that file
before validation batches and commits, and added the same operational rule to
this isolated checkout's AGENTS/CLAUDE instructions.

### Corrections, committed source `99a6d16`

- One compact local map now contains eleven actual pixel places, including original
  Review/Fix/Waiting props and hatched reserved labels. They remain explicitly not
  reported by current hooks. No signed wire or backend vocabulary was changed.
- Three agents in three different states no longer allocate an empty second map.
  The cause was allocating a global Arrival reservation for every hydrated agent.
  Hydrated DTOs now appear at their actual station; only subsequent observed state
  changes walk. Reservations remain per-state and stable. Actual crowded stations
  use sparse named `+` overflow plots rather than complete duplicate maps. Drawn
  corridors and the path graph use matching 64px station rows and per-plot offsets.
- Yard, Agent list and Settings are distinct reachable views with keyboard buttons.
  The renderer survives view changes. The normal 960×680 viewport shows the primary
  local map, with bounded scrolling for genuinely crowded/narrow cases. Header and
  first-launch flow are concise; onboarding creates Strict internally and offers
  only workspace/display names, without a non-choice network selector.
- Inspector is a built-in HTML modal dialog styled in the same pixel system, focused
  when opened and closed by Escape or its button. Metadata/ID and actions have
  separate rows. `activityLabels.ts`, called by AgentList and WorkspaceScreen,
  provides friendly kind/provenance text. Keyboard list selection restores the
  real opener; canvas selection restores the map region.
- Connection review hides setup choices while a plan is open. The exact diff alone
  scrolls; approve/cancel/close occupy a non-scrolling footer. The baseline hash is
  in collapsed diagnostic details. Remaining panel height is measured from its
  actual viewport top and recalculated on resize/header reflow, avoiding fixed
  header-height assumptions at 320px. No apply occurs without approval; cancelling
  install/removal is asserted to send zero `hook_apply` calls in browser evidence.
- Settings puts display-name edit/save directly after the member card. Keychain,
  mode and privacy wording remain in a disclosure; font license remains available.
  Original font/art/icon/background, immutable wire, safe installers and integer
  origin fixes are preserved. No further icon/generated asset churn in this fix.

### Final visual evidence

`node scripts/pixel-qa.mjs`: **exit 0, 68 scenarios/captures, zero errors**
(`/Volumes/TokensAbuse/logs/pixel/parent-final-captures.log`). Each has both a
full-page PNG and an actual viewport PNG: **136 images**, at 1120×900 DPR2,
960×680 DPR1, 375×800 DPR2 and 320×680 DPR2. The manifest names both files and actual
viewport/DPR/canvas geometry/command/evidence level. Every image has the browser
mock banner. The final cases include onboarding, loading, empty, errors/repair,
two kinds, **three agents at distinct stations**, modal inspector, list, settings,
connection choice, Claude/Codex install diffs, same-scene resize, reduced-motion
state transition, and Claude/Codex removal diffs.

Assertions include no horizontal document overflow, local font availability,
production CSP, integer canvas origin/scale, one map for three distinct states,
keyboard inspector/open/close focus, visible profile save, visible review actions
within the actual panel/viewport at every width, Tab reachability and Enter on
Cancel without any apply call, and reduced-motion snap stability. Exact per-kind
diffs are preserved; Codex excludes StopFailure.

Preserved failures: the first correction capture stopped before any image because
a local `height` variable shadowed the viewport binding; renamed, not skipped.
The stronger footer assertion then caught the 320px Close action at y=654..694,
panel y=266..718 in a 680px viewport. This proved that a fixed subtraction was
wrong. The measured remaining-height layout resolves it, with the original
visibility assertion unchanged. Logs: `parent-fixes-captures.log`,
`parent-actions-captures.log`, `parent-actions-narrow-fixed.log`,
`parent-actions-diagnosis.log`; the failure PNG is retained in logs. Initial
frontend correction failures were unused shared exports/parameter and the still
required mode disclosure. Shared labels moved to their own production module;
mode is displayed in Settings. No assertion was dropped to force green.

Final relevant frontend check: `npm run check:frontend`, exit 0, **60 tests**,
TypeScript and ESLint pass (`parent-handoff-frontend.log`). Prettier and diff
whitespace checks pass (`parent-handoff-format.log`). The new scene regression
asserts exact three-state coordinates, eleven places, one map height and stopped
movement ticker. Existing collision-free slots, retained identities, reflow,
path/replan, provenance and reduced-motion tests still pass.

Parent actually inspected corrected viewport images for the three-agent yard,
modal inspector, both agent installers (including 320px), settings, onboarding,
list, errors and narrow connections. It accepted these as browser visual evidence
and requested direct Phase 3 handoff after final checks/exports. The worker also
reviewed actual images and final contact sheets; no native visual claim is made.

### Final checked source and exports

At committed UI source `99a6d163e616887e7ed75d809cd32046e953e78d`, normal-context `npm run check` passes
**exit 0, 42.013 s**, including all Rust workspace tests, fmt, all-target
Clippy, TypeScript, ESLint, **60 frontend tests** and Prettier. Exact command/result:
`/Volumes/TokensAbuse/logs/pixel/parent-final-full-check-normal.json` and its log.
No Rust source changed in this visual phase; the separately executed workspace
run also passed exit 0 / 30.311 s. Production build and static asset audit pass
(`parent-production.log`, `parent-production-audit.json`); the local font and
exact OFL text are included, mock IPC is excluded, CSP unchanged. Vite's existing
>500 kB chunk advisory remains; no build error is hidden.

Final full-page and viewport files, manifest and four reviewed viewport contact
sheets are exported under `/Volumes/Untitled/projects/tokens-abuse/dist/screenshots/`.
German phase report and tester guide are exported to that dist directory. APFS
originals remain under `/Volumes/TokensAbuse/dist/`. The export index records
SHA-256 for the final files; old draft/failed captures remain image-local history.

Browser visual review is accepted by the parent (current parent-feedback.md).
Default packaged Keychain first launch remains the architect owner checkpoint;
DMG/native/bundled-helper/actual CLI acceptance is not claimed. This completes
the assigned pixel/documentation implementation and hands off directly to the
mandatory Phase 3 writer. No push, no nested writer, no P2P, no real configs.

## Phase 3 — production DMG, 2026-11-05

Implementation commit `de90016` builds version 0.2.0 for aarch64-apple-darwin with
Tauri's real external hook binary, ad-hoc signing, identifier ch.arifi.meshyard,
meshyard URL scheme registration, and the accepted original pixel icon/background
in a 640×440 DMG window. Cargo/npm generated the version-only lockfile updates.
No wire-v1 or production runtime behavior was changed. No T-013 task spec exists
in this checkout; the current owner brief and normative protocol govern this work.

`npm run build:production` builds the release hook, stages the target-suffixed
binary, self-tests the strict guard, runs `tauri build --target
aarch64-apple-darwin --bundles app,dmg`, and scans all resulting app files. The
host list derives from locked iroh 1.3.0 defaults.rs/address_lookup/pkarr.rs and
locked iroh-dns 1.4.1 dns.rs: production/staging regional relays and DNS/pkarr
services. Parent domains iroh.link, n0.computer and pkarr.org are forbidden.
No strings are hidden/rewritten. The solo app has no meshyard-net dependency and
constructs no network endpoint; Auto and joining remain unsupported. URL scheme
registration is metadata evidence, not a runtime invite/join implementation.

### Commands and actual results

Every shell sourced `/Volumes/TokensAbuse/env.sh`. Full output, exact commands,
exit files and package JSON evidence live in `/Volumes/TokensAbuse/logs/phase3/`.

- `npm install --package-lock-only --ignore-scripts` and
  `cargo generate-lockfile --offline`: exit 0, version-only regeneration.
  Logs `npm-lock.log`, `cargo-lock.log`.
- Initial configuration-writing Python had a syntax error; corrected before the
  final build. An accidentally duplicated build was identified with a narrowly
  scoped normal-context process inventory and terminated. No other run touched.
- Sandboxed `npm run build:production`: app compiled and signed, DMG failed
  `hdiutil create` with device-not-configured. Preserved `build-sandbox.log`.
- Normal-context `npm run build:production`: **exit 0**, two release bundles,
  app 13.25 MiB and DMG 4.35 MiB, actual strict scan **5 files pass**.
  `build-normal.log`, `build-normal.exit`.
- `node scripts/check-strict-bundle.mjs --self-test`: **exit 0**, every forbidden
  synthetic host rejected, clean control accepted. `guard-self-test.log`.
- Same guard on `/Volumes/TokensAbuse/tmp/phase3-forbidden.app` containing a
  synthetic n0 relay URL: **expected exit 1**. `guard-forbidden.log/.exit`.
- Sandboxed `npm run check`: **exit 101**, known netmon sandbox denial in nine
  network tests; fmt/Clippy and preceding crate tests passed. `check.log/.exit`.
  No assertions skipped/removed; no repeated sandbox network run.
- After current parent feedback explicitly authorized normal-context validation,
  unchanged `npm run check`: **exit 0**, fmt, all-target Clippy, full Rust workspace,
  all 15 network tests, TypeScript, ESLint, **60 frontend tests**, Prettier.
  `check-normal.log/.exit`.
- Separate normal-context `cargo test --workspace`: **exit 0**,
  `workspace-normal.log/.exit`. Existing ignored entries remain reported; the
  executable hook smoke is run explicitly below and the production-service
  Keychain checkpoint is not bypassed.
- Separate `npm run check:frontend`, `npm run format:check`: **exit 0** each,
  `frontend.log/.exit`, `format.log/.exit`. Focused store/node tests also exit 0:
  `cargo test --locked -p meshyard-store -p meshyard-node`, `remaining-crates.log`.
- `python3 /Volumes/TokensAbuse/logs/phase3/accept-package.py`: **exit 0** in normal
  context. Individual argument arrays/exits/logs in `package-results.json`:
  `hdiutil verify` 0; attach `-readonly -nobrowse -mountpoint` 0; `ditto` app to
  APFS Applications-like directory 0; detach 0; `codesign --verify --deep --strict
  --verbose=2` 0; codesign details 0 (adhoc/runtime, arm64); `spctl --assess --type
  execute --verbose=4` **expected 3/rejected**; `plutil -p` 0 (version, id, URL);
  `file` on both main/helper 0 (arm64); strict scan of copied app 0.
- Actual signed bundled hook extracted byte-for-byte to temporary APFS path.
  `MESHYARD_TEST_HOOK_BIN=/Volumes/TokensAbuse/tmp/phase3-package-020/extracted/meshyard-hook
  MESHYARD_TEST_PACKAGED_HOOK_BIN=/Volumes/TokensAbuse/tmp/phase3-package-020/Applications/Meshyard.app/Contents/MacOS/meshyard-hook
  cargo test --locked -p meshyard-app --test local_workspace
  isolated_hook_executable_routes_both_agent_kinds_and_reopens_state -- --ignored
  --exact --nocapture`: **exit 0, one test passed**, `bundled-hook-e2e.log/.exit`.
  Original outside-/Applications .app path rejects HookInstallLocation; extracted
  bytes equal original. Temporary HOME/settings/project only, reviewed installer
  apply/removal for both kinds, golden fixtures through real executable and UDS,
  production signed append/view pipeline Thinking→Building→Finished with provenance
  and Connected, other kind unchanged, persistence/reopen verified. This is backend
  evidence using the production app core, not native UI or actual CLI emission.

### Export and native smoke

Built/exported `Meshyard_0.2.0_aarch64.dmg` SHA-256:
`abade83367b6988db0ef69193c9ace6300ea2bbf582698fa591d78dcc5030df8`.
APFS source, APFS dist copy and exFAT export match; `dmg-export.json` records
source/export. Hook hash `472e226b9674734f06131cc752f45f727ce4b81a17fce2f93cee253573581ab5`
for original and extraction, `hook-hash.json`.

Exact copied native app: `/Volumes/TokensAbuse/tmp/phase3-package-020/Applications/Meshyard.app`.
Smoke PID **28537**, stayed alive after 12 seconds, empty `native-smoke.log`.
`native-smoke.json` records executable, PID, time and environment: explicit TEST
file key; data `/Volumes/TokensAbuse/tmp/phase3-package-020/native-data`, HOME and
hook HOME `/Volumes/TokensAbuse/tmp/phase3-package-020/home`. Parent requested this
one instance remain running for CUA selection; no other app should be opened.
Process survival/empty output are not native screen or default-Keychain evidence.

Unchanged accepted browser mock screenshots: 68 scenarios, 136 paired PNGs plus
four viewport contact sheets, manifest/README; 142 exported files hash-compared
with APFS originals, `screenshot-export.json`. Personally reviewed all four
viewport contact sheets and original pixel DMG background. Required states and
both installer diffs are covered. DPR2 is browser emulation, not native Retina.
Native CUA capture/inspection is delegated to parent; no permission bypass here.

German report and tester guide are written in `/Volumes/TokensAbuse/dist/` and
exported beside the DMG to `/Volumes/Untitled/projects/tokens-abuse/dist/`.

### Remaining acceptance boundaries

The architect forbids unattended production-service Keychain identity creation.
Keychain remains default, no fallback. Earlier real-HOME unique-service probe
passed; default installed-app first launch is still an owner checkpoint, **not
validated** by explicit-file smoke. Native UI/Retina/resize and installer clicks,
real Claude/Codex CLI hook capture, runtime deep-link handling and P2P/cross-network
remain not validated here. Known Vite >500 kB chunk advisory remains. Phase 3
**complete=false**, no phase3-acceptance.json success claim, no Phase 4, no push.
Git sandbox denied .git/index.lock; local focused commit used authorized normal
context. No real agent configs, global memory or protected projects modified.

## Phase 4 implementation and interim evidence (2026-11-05)

Root authorization: `/Volumes/TokensAbuse/logs/phase3-acceptance.json` and
`architect-phase4-1.log`. Mandatory Phase 3 acceptance passed; the production-service
Keychain first launch remains an unvalidated owner checkpoint, not a Phase 4 gate.
The exported German Phase 3 report was corrected; the v0.2.0 DMG is preserved.
No task specs for T004/T005/T006/T011 exist in this checkout; normative PROTOCOL
and SECURITY govern this implementation. Signed wire v1 is unchanged.

Initial implementation plan is retained in logs/current-status.md. The headless
node now uses the same AppState, local ingest, pairing and sync services as native
IPC, with explicit temporary file-key profiles in checks. Pairing persists hashed
tokens, single-use reservations, nonce resume, both transcript-bound confirmations
and the signed owner MemberAdded atomically. The SAS uses six words from a fixed
256-entry original compound-word vocabulary (48 bits); an explicit hash/word vector
pins interoperability. Sync sends only the author's durable store cursor and
retains original signed bytes and store diagnostics. Sharing is an explicit action.

Actual commands and results (all sourced `/Volumes/TokensAbuse/env.sh`):

- `cargo build -p meshyard-node --features strict-only --release`: exit 0,
  `logs/phase4-node-build.log`. The first actual binary inspection exited 1,
  finding `iroh.link` (`phase4-strict-node-inspection.log`). No string was hidden.
- Architect authorized a true compile-time vendor feature (`architect-strict-build-1.log`).
  Subsequent release build exit 0 (`phase4-node-strict-build3.log`), actual binary
  inspection exit 0 with no forbidden hosts (`phase4-strict-node-inspection2.log`).
  Default/staging relay and DNS/pkarr constructors are compiled out. ADR-013 TLS
  settings and the reviewed independent capable-client positive control remain intact.
- The normalized vendored manifest was initially edited directly, then replaced
  with deterministic generated output from a SHA-256 verified original archive
  plus maintained Cargo.toml.orig feature input. Generator and verification:
  `python3 scripts/generate-iroh-strict-manifest.py --archive /Volumes/TokensAbuse/tmp/iroh-1.3.0.crate [--check]`,
  exit 0 (`phase4-vendor-manifest2.log`). The first no-archive attempt failed honestly.
- `cargo test -p meshyard-core -p meshyard-store`: first exit 101 because the
  previous-schema migration fixture still retained the new schema's pairing columns;
  its fixture rollback now reconstructs the actual previous schema. All existing
  assertions remain. Rerun exit 0 (`phase4-core-store-all2.log`), including four new
  pairing security/reopen/transactional rollback tests and frozen wire goldens.
- `cargo test -p meshyard-net security_tests -- --nocapture`: normal context exit 0,
  two tests covering reject-with-correct-hash, wrong transcript, sender-only refusal
  and signed invalid-UTF8 diagnostic preservation (`phase4-net-security.log`).
- `cargo test -p meshyard-net --features strict-only --doc`: first failed because
  an upstream test helper referenced removed discovery services. The helper's discovery
  portions are now feature-gated too; rerun exit 0, five compile-fail capability
  checks (`phase4-strict-capabilities2.log`). Later complete strict suite recorded below.
- `npm run check`: initial exit 101 at three new Clippy findings
  (`phase4-check1.log`); corrected. `cargo clippy --workspace --all-targets -- -D warnings`
  then exit 0 (`phase4-clippy2.log`). Full final gate still required after final source.
- `npm run check:frontend`: initial type coverage and new strict ESLint failures
  retained; corrected without assertion changes. Rerun exit 0, 62 tests
  (`phase4-frontend3.log`). Bindings and Tauri command permissions generated by tools.
- Extended `MESHYARD_TEST_HOOK_BIN=<copied v0.2.0 bundled hook> python3 scripts/check-p2p-processes.py`:
  first failed because joined profiles did not start ingest until a network-status
  read. Production app_status now starts the eligible listener as well. Rerun exit 0
  (`phase4-two-process3.log`): both confirmation orders, reservation crash/reopen
  with same nonce/SAS, no early membership, actual bundled golden fixtures yielding
  remote Building/ToolEdit, reconnect yielding Finished/Stop, single-use refusal.
  Node SHA-256 `887e5f441677f6cf28b16e0982e551a28c2ecac210758bb496f6a5a739b3d7ef`;
  helper SHA-256 `472e226b9674734f06131cc752f45f727ce4b81a17fce2f93cee253573581ab5`.
  These are two real same-Mac production-core processes, not native UI, real CLI,
  two-Mac or cross-network evidence. Final packaged bytes require a final rerun.
- `node scripts/p2p-ui-qa.mjs`: initial sandbox Chrome launch denied (SIGABRT/EPERM),
  retained (`phase4-ui-qa.log`). Normal-context first run exposed fourth-tab horizontal
  overflow at 320; fixed by tab wrapping. Normal rerun exit 0, 28 browser-only mock
  screenshots at 320/375/960/1120 (`phase4-ui-qa-normal2.log`). Personally inspected
  invite/QR at 320, SAS at 960, join at 375 and error at 1120. Invitations are invalid
  synthetic strings; no native scan or native Retina claim. Stale footer/settings
  copy was found during review and corrected; final recapture remains required.

Parent feedback is incorporated throughout, including persistent nonce resume,
REJECT frame handling, malformed UTF-8 diagnostics, source manifest generation,
compact onboarding and cache admission transition/deadline hardening. Native app
launch/capture remains parent-only; the exact isolated smoke is prepared in
`scripts/check-package-smoke.py`. No real agent configuration or production
identity is created by these checks. No push or historical roadmap completion.

### Final security closure and owner-map integration (2026-11-05)

Architect closure review (`logs/architect-security-closure-1.log`) accepts the four
material findings and admission history/deadline fixes in source; it executed no tests.
Actual focused tests passed in `phase4-security-recovery3.log` (five cases, including
real 31-second human decisions in both orders) and `phase4-welcome-crash2.log`
(actual killed/reopened production-core joiner, unchanged identity, restored owner,
signed membership and own ProfileUpdated). The recovery harness now uses a two-thread
Tokio runtime so its blocking child-pipe reads cannot starve the owner endpoint.
Optional LOW decision-race and manual retry fixes are included. The completed owner
race regression passes in the final workspace run.

The first full run exposed a real ingest lifecycle regression: starting ingest in
read-only app_status replaced the explicitly stopped fake-clock test listener with
a real-clock one. Removed that startup; explicit registration starts eligible ingest.
The original fake-clock assertions pass (`phase4-ingest-lifecycle-fix.log`). Earlier
statement above about app_status startup describes the superseded implementation.

`npm run check` normal-context exit 0 (`phase4-check6.log`): fmt, Clippy with warnings
denied, workspace tests including independent 0-RTT positive control, frontend 67
cases, and Prettier. Earlier check1–5 failures are retained, not claimed passed.
Checks 4/5 ran while map mocks were being updated; final map tests retain meaningful
slot identity/promotion, path, obstacle and integer layout assertions.

Owner updated BRIEF and PLAYGROUND-MAP.md require Dion's original lobby. Source PNG
is preserved byte-for-byte, processing/provenance in `app/src/assets/yard/`. The
Pillow 11.3.0 script uses nearest sampling, removes only the last source row and
quantizes to 96 colours without dithering; equal-scale comparisons and path/feet
review overlays are APFS `dist/map-review/`. Parent accepted the processed variant.
The map has exactly eleven buildings, fixed anchors and three visible slots per
station. Additional agents remain available in the list and station count. The
plaza graph avoids fountain/benches/banner poles/lamps; Fix traverses the monitor
under a tight same-art occluder per architect decision (`architect-map-1.log`).
Graph tests sample edges and all pairwise routes/rounded mid-walk retargets; they
are geometric authored-polygon checks, not a whole-image pixel-mask proof.

`node scripts/map-qa.mjs` normal-context exit 0 (`map-scene-qa2.log`): sixteen labelled
synthetic browser captures at four widths/DPR1/2; three visible people-sized avatars,
a fourth counted/listed, and an actual animated synthetic Fix transit behind monitor.
Fix remains unsupported by current hooks. HTML labels, integer minimum 2x scale,
panning and accessible list preserve readability. UI/icon/DMG art share a sea/stone/
wood/crystal palette. Pixi image detection/blob workers are explicitly disabled;
only bundled PNGs are loaded under the unchanged production CSP. Initial CSP
failures retained in map-solo-qa.log; no policy relaxation.

- `python3 scripts/check-iroh-vendor.py /Volumes/TokensAbuse/tmp/iroh-1.3.0.crate`:
  exit 0, all 84 upstream files reproduced from verified archive + TLS/strict patches
  (`phase4-vendor-final.log`).
- `python3 scripts/generate-iroh-strict-manifest.py --archive /Volumes/TokensAbuse/tmp/iroh-1.3.0.crate --check`:
  exit 0 (`phase4-manifest-final.log`). Generated manifest matches maintained source.
- `node scripts/pixel-qa.mjs`: exit 0, 68 scenarios / 136 full-page and viewport PNGs,
  no page/CSP errors, no horizontal document overflow and integer canvas origins/scales
  (`map-solo-qa3.log`). Captures are browser mocks, not native/live agents.
- `node scripts/map-qa.mjs`: final exit 0, sixteen map screenshots and actual focused
  ArrowRight/ArrowDown panning, right-hand Test station revealed at narrow widths
  (`map-scene-qa3.log`). Personally inspected two-station, three-visible/count-four,
  synthetic occlusion and narrow captures; parent visually accepts these images.

Focused local implementation commits: `2aece5f`, `a0765b8`, `4cdae1c`, `5ed71ac`.
No push. Final package/release-process tests and parent native smoke still pending
at this evidence checkpoint; no final 0.3.0 artifact exported yet.

Final frontend rerun initially hit only the combined all-pairs test's 5-second CPU
budget while release compilation was active (`phase4-frontend-final.log`). Split
into one parameterized test per source station, retaining every destination and
rounded sample/assertion. `npm run check:frontend` exit 0, 77 tests
(`phase4-frontend-final2.log`). No production algorithm or assertion was weakened.

Final map-inclusive build emitted signed app but DMG bundling failed at its own
normal detach: `hdiutil: couldn't unmount "disk8" - Ressource ist belegt`
(`phase4-build-final.log`, exit 1). `hdiutil info -plist` identified the exact
build-owned `rw.39346.Meshyard_0.3.0_aarch64.dmg` mounted at `/Volumes/dmg.2XhQgX`.
A subsequent ordinary `hdiutil detach /dev/disk8` exited 0 (`phase4-build-owned-detach.log`).
No force detach, unrelated volume action or app process intervention; unchanged
strict build retry recorded separately in `phase4-build-final2.log`.
