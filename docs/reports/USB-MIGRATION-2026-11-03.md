# USB projects migration — 2026-11-03

Owner request: keep all active Meshyard data under `/Volumes/Untitled/projects` and continue
working toward the genuine MVP. The host filesystem is ExFAT; the active data lives inside
`/Volumes/Untitled/projects/MeshyardProjects.sparsebundle` (APFS). The successful normal mount
is **`/Volumes/MeshyardProjects`**, with owners enabled. The literal nested
`projects/workspace` mount was denied and is unused; it did not become the checkout path.

Active paths:

- Canonical repository: `/Volumes/MeshyardProjects/meshyard`.
- Task worktrees: `/Volumes/MeshyardProjects/meshyard-worktrees/`.
- Smithers: `/Volumes/MeshyardProjects/meshyard-smithers`.
- Toolchains, targets, caches and temporary files: `/Volumes/MeshyardProjects/Meshyard-build`.
- Source before project commands: `/Volumes/MeshyardProjects/meshyard-smithers/env.sh`.
- Migration control/evidence: `/Volumes/MeshyardProjects/migration-control`.
- Remount guide: `/Volumes/Untitled/projects/README-MESHYARD.txt`.

The bounded Smithers migration copied exactly four roots from `/Volumes/Kursmodule`:
`meshyard`, `meshyard-worktrees`, `meshyard-smithers`, `Meshyard-build`. Before intentional
operational edits, full SHA-256/mode/link manifests matched: **214,973 regular files,
133 symlinks, 19,782 directories, 9,621,900,751 regular-file bytes**. The old source trees
also matched their saved baselines after validation. Original images, unrelated `Modul322`,
other projects and Documents/temporary checkouts remain preserved. No source deletion.

Only destination worktree pointers were repaired; active worktrees use the new canonical
`.git`, with matching HEADs, refs and logical staged entries. There are no source object
alternates. Initial copied indexes were byte-identical; later ctime/device/inode stat-cache
refreshes were structurally reconciled without resetting/restoring indexes. The old copied
`Meshyard-build/t002-worktree` remains an inactive historical worktree tied to its old clone.

Operational configs were archived before path changes. Environment/workflow/prompt paths and
owned absolute links now target the new image. Historical workflow inputs have replay guards;
new tasks need fresh workflows. Raw history, product code, wire/generated files and old reports
were not blanket-rewritten. The lead additionally corrected only the active T-002 `CLAUDE.md`
operational paths after inspecting the completed worker result; original bytes/hash are archived.
All five dirty KNOWER files remain byte-identical, unstaged and uncommitted by the lead.

Actual validation:

| Command / check | Result |
|---|---|
| Regular APFS attach with `-owners on` | Exit 0; actual mount and owners verified |
| Filesystem UID, 0700/0600, symlink and executable probes | Passed |
| Source/copy full manifests before operational rewrites | Identical for all four roots |
| `git worktree repair` at the two explicit new active paths | Exit 0 |
| New canonical `git fsck --full --no-dangling` | Exit 0 |
| Copied Smithers SQLite `quick_check` via SHA-verified scratch copies | 11/11 OK; historical DB/WAL/SHM preserved |
| New environment Cargo/rustc versions | Both 1.99.1; new sysroot/target paths verified |
| Full `npm run check` after sourcing new env, `CARGO_NET_OFFLINE=true npm_config_offline=true` | Exit 0; approximately 169 s (168.776 s launch-to-observed-exit; 169.367 s including Git baseline) |
| Full-check tests | 55 top-level Rust passes + 1 lock child, 3 Vitest passes; 2 ignored entries |
| `git diff --check` | Passed; repeated after lead path-only correction |

The first sandboxed ditto attempt exited 1 on metadata operations; an exact scoped retry
exited 0. Contents/modes/links and the ten warning-file metadata records were reconciled;
the retry's exact wall duration was not captured. An initial non-repository worker startup
failed before copying, then the control was explicitly configured. Git index guards paused
before stat-cache classification; the successful application check was not rerun. Details,
command exits, manifests and limits are retained in `migration-control/MIGRATION-REPORT.md`,
its JSON report and `evidence/`. No native app, Keychain, router, two-Mac or public-network test.
No dependency installation, binding regeneration, merge, reformat or source/image deletion.

Product status remains core/store/identity implementation plus scaffold UI; genuine pairing,
sync, hook events, product UI and Strict-only tester DMG remain milestone work. T-001 requires
Claude-recorded Dion L1 and explicit merge instruction. T-002 fixes at `6ac6c1f` await Claude
re-review; positive native Keychain coverage remains unverified. The unchanged hook <20 ms
criterion belongs to T-007. No T-003 until both predecessor tasks are done and merged in order.
Laptop caffeinate/hotspot were owner plans, not independently verified or configured here.
