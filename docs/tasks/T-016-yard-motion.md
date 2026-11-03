# T-016 — Living yard: agents walk to their real state's station

Status: ready · **Priority 1 (owner feedback 2026-11-03)** · Owner: Codex lane A (frontend) ·
Reviewer: Claude. Branch `t-016-yard-motion` from T-015 accepted code `a859ed4` (branch head incl.
reports). D-9 stacked; no `main` gate waived.

## Why
Dion: "es ist kein richtiger Playground, die Agents laufen nicht herum." Today the yard is
re-mounted on every `workspace.agents` change (`WorkspaceScreen.tsx` effect), so nothing can move
and positions reset. Movement must come only from **real** state changes in the DTO — never
invented work, idle wandering, peers or test activity in production.

## Scope (frontend only)
- **Ownership:** `app/src/playground/**` and the yard-mount effect in
  `app/src/features/workspace/WorkspaceScreen.tsx`. No Rust/IPC/generated/dependency/CSP change,
  no palette or new art direction (frozen until the reference image arrives); one minimal path
  tile for the vertical lane below, from the existing palette.
- **Persistent scene API** (`yard.ts`): `createYard(host, onError) → { update(agents: AgentView[]),
  destroy() }`. `WorkspaceScreen` creates it once per workspace id and calls `update` on every DTO
  change. Character identity = `AgentView.id`; position survives updates; new agents appear on the
  Arrival pad; removed agents disappear; slot within a station is stable per id.
- **Path graph** (pure, `yardPath.ts`): derived from `yardPlots` geometry for both layouts: each
  station has a door on the path side; horizontal path per row pair; in the 2-column layout one
  vertical lane at the left edge joins the paths. `plannedPath(from, to) → Point[]` (door → path →
  … → door → slot), integer logical coordinates.
- **Walking:** fixed speed 48 logical px/s, integer positions, 2-frame step pose from the existing
  sprite (no new character art); a route longer than 3 s is sped up to finish in 3 s. A new target
  during a walk re-plans from the current position. A Pixi ticker runs **only while** a character
  is moving and stops when all arrive (no permanent loop). `prefers-reduced-motion: reduce` → snap
  directly to the target slot. The HTML name tag follows its character at integer offsets.
- The accessible `AgentList` remains the equivalent (unchanged data); canvas `aria-hidden`.
- Empty yard unchanged (environment only).

## Acceptance
1. Pure tests: `plannedPath` for same-station, cross-row (4 col) and cross-page (2 col) moves;
   deterministic `step(position, path, dt)` positions; route cap; re-plan mid-walk.
2. Scene tests (Pixi mocked like today): `update` with a changed `state` for one id moves only that
   character and keeps the others' positions; adding/removing ids; reduced motion snaps; ticker
   stopped when nothing moves; destroy cleans up.
3. Browser-mock evidence (labelled, not native): a deterministic fixture sequence
   No events yet → Thinking → Testing → Finished for one agent, frames captured at start/mid/end at
   960×680 and 375×680; and a reduced-motion run showing the snap. Fixture-only states are labelled
   as fixtures; production never generates them.
4. `npm run check` once; each new module has a production caller; no unused helpers.
5. Native N-14 and reference-image acceptance stay separate and pending.

## Hand-off
T-007b's frontend part (§4–5) builds on T-016's `createYard().update` — real ingest-driven state
changes then walk automatically. T-016 does not touch `ipc/client.ts`, `App.tsx` or connect UI.
