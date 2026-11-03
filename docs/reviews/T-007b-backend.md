# T-007b review — backend foundation (§1–3, §7 / ADR-014, ADR-015)

Reviewer: Claude · Date: 2026-11-03 · Reviewed head: **`dd11f8b2491de1fe4c61b0061c3ea6dd39c83924`**
(backend `d001183`, `e95ad2e`, `c6d3102`; ADR-014 `af550e0`/`38da6cf`; ADR-015 `6d93007`, report
`b6e2053`; normal merge of reviewed T-016 `090257d`). Scope: backend foundation only — the
installer and Connect UI/DTOs (§4–6) are intentionally not implemented yet and are not findings.

**Verdict: changes-requested (one bounded fix — the full check is red).** The projection and
peer-credential design is implemented soundly; code acceptance for frontend continuation follows
once the fix and a green full check exist. T-007b stays in progress; not done.

## Evidence boundary
- Source read: `crates/meshyard-store/src/database.rs` (migrations, binding APIs, membership,
  overflow, insert/promotion, `confirmed_author`, `current_epoch`, `add_workspace`),
  `database/projection.rs` (full), test names in `database/projection_tests.rs`,
  `app/src-tauri/src/ingest.rs` (listener, `peer_uid`, `read_one_with`, `serve` start),
  hot-path call sites in `state.rs`/`ingest.rs`, `tests/local_workspace.rs` (child-process test).
  I ran nothing.
- Root normal-shell evidence (read, not re-run):
  `t007b-darwin-uid-evidence/parent-closed-peer-first-proof` — exit 0: old tokio `peer_cred()` on a
  closed peer → raw errno 57 `ENOTCONN`; new `getpeereid` UID == euid → **ADR-015 cause confirmed on
  this Mac**. `parent-all-uds-after-uid-fix` — exit 0, six socket tests incl. the original immediate
  write/shutdown/drop → Thinking → Testing → Finished → Idle within the original 6 s budget, live
  peer, bounds, mismatch, lifecycle.
- **`parent-combined-full-check` on `dd11f8b` — exit 101 (73.281 s), head/source unchanged during
  the run.** Store, agents, hook and app unit tests passed (app lib 14/14 incl.
  `prep_counted_app_status_idle_and_binding_use_projection_across_store_clones`), but
  `tests/local_workspace.rs::commands_persist_across_two_fresh_processes_with_explicit_usb_file_profile`
  failed: its child panicked at `ingest.rs:161` ("A Tokio 1.x context was found, but IO is
  disabled"). Earlier prep failures and worker EPERM runs remain in the record.

## Accepted (source)
- **Projection (ADR-014):** `agent_projection`/`profile_projection`/`members.added_name` updated by
  `apply_contiguous_projection` right after `apply_contiguous_membership` on direct insert, on every
  promoted pending event (re-verified once at promotion) and in backfill — same transaction.
  Rules match `fold` phase 2: register upserts and resets (Idle, no provenance/tool/task, ts NULL);
  `AgentState` updates only an existing row (earlier/unknown ignored), records `ts_ms`; `AgentGone`
  deletes; `ProfileUpdated` upserts; membership events no-op. Rows kept for all authors; reads
  filter `members.removed = 0`, name = profile else `added_name`, owner role from cached genesis —
  equivalent to fold's removal/re-grant behaviour. Owner removal ignored as before.
- **Bounds and consistency:** never-confirmed author overflow (insert and reopen cleanup) deletes
  events and both projection rows together; `projection_counts` maintained by triggers; one
  transactional backfill per workspace with a `meta` marker (new workspaces marked at
  `add_workspace`); genesis verified once at open/add and cached in the single DB actor, so every
  `Store` clone sees the same state. `confirmed_author` and `current_epoch` are now indexed
  lookups. Receive-time verification unchanged; no wire change.
- **Hot paths:** `AppCore` status/open, ingest binding (`agent_binding`) and state lookup
  (`own_agent`) use `workspace_snapshot`/`own_agent`; `workspace_state` remains only in tests.
- **Bindings:** `agent_binding` joins on `a.author = own`, same kind, active member — another
  device's identical `local_id`, a wrong kind or a gone/removed agent yields `None`;
  `bind_first_own_agent` counts own rows of that kind via SQL.
- **ADR-015:** macOS `peer_uid` = `nix::unistd::getpeereid(&stream)`; `read_one_with` performs the
  lookup and the exact euid comparison before the first read; every lookup error or mismatch
  rejects with a stage counter (`peercred_error` + raw errno, `uid_mismatch`, `read_io`,
  `eof_empty`, `timeout`, `oversize`, `parse`, `reader_task_error`), summed into `dropped_invalid`;
  no payload stored; fixed 1 KiB buffer read byte-wise under the 200 ms budget. No hook, sleep, ACK
  or wire change; threat boundary stays per OS user ("Verified" = observed on the local socket).
- Tests present: `projection_5000_history_backfill_once_and_constant_reads_appends_promotions`,
  `projection_script_matches_fold_authority_reset_membership_gaps_and_atomic_rollback`, app-level
  counted test across Store clones (passed in the combined run).

## Required change
1. **Full check red: the T-014 two-process test now fails because `AppState::open` starts the ingest
   listener.** The child builds `tokio::runtime::Builder::new_current_thread().build()` without IO
   or time drivers, so `UnixListener::bind` panics. The production Tauri runtime enables both, so
   this is a test-runtime mismatch, but a panic is also the wrong failure mode. Fix both, minimally:
   (a) the child runtime uses `.enable_all()` (matches production; no assertion change);
   (b) the ingest start must not panic when no usable reactor exists — fail closed to
   `unavailable = true` (later `IngestStatus::Unavailable`) and let the workspace open normally.
   Then one full `CARGO_NET_OFFLINE=true npm run check` green in the normal shell.

## Notes (no change required)
- `check_projection` still applies the 100 000-event `ProjectionLimit` to snapshot reads although
  reads no longer scale with N; acceptable as a carried-over policy, revisit with compaction (M2).
- Projection blobs are postcard-encoded domain values with key cross-checks on decode — fine.

## Gates
- After fix 1 + green full check: **backend foundation code accepted** for the single combined
  §4–6 Connect/installer writer (DTOs exactly as spec §6).
- Still open and separate: owner Install gate and live CLI capture (L4), T-007a B-1, N-14, reference
  image, Keychain; no `main` merge.
