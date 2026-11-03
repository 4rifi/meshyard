# T-015 — App-wide pixel redesign

Status: ready · **Priority 1 (owner, 2026-11-03)** · Owner: Codex lane A · Reviewer: Claude
Branch `t-015-pixel-app-redesign` from `t-014-local-workspace-slice` @ `1f90a07` (+ this planning
commit when committed). Stacked under D-9; no `main` gate is waived.

## Why
Dion: "die app sieht schon sehr sehr schlecht aus, ein redesign ist dringendst notwendig, wie auf
dem ersten bild mit dem pixel texture auf die ganze app verteilt." The current UI is a generic
dark dashboard (16 px rounded panels, pill badges, smooth buttons, a rounded canvas box, blurred
Pixi text). The whole app — not only the avatars — must look like one pixel-art place.

**Reference image:** not available. No image is in the architect's context or repo (only
`assets/scaffold-icon.svg`; the 19:39 screenshot shows the old scaffold and is *not* a reference).
Palette and layout below are **provisional**; no "matches the reference" claim until Dion supplies
the image. All art is original (no copied sprites, tilesets or fonts).

## Hard constraints
- **Exclusive ownership:** `app/src/**` (except `app/src/ipc/generated/**`), `app/src/assets/**`,
  `app/index.html`, `app/vite.config.ts`. No other app writer until hand-off. Not touched:
  `app/src-tauri/**`, `crates/**`, generated bindings, `tauri.conf.json`, lockfiles.
- **No** Rust/IPC/wire/identity change, **no** new npm or Rust dependency, **no** remote fonts/assets,
  CSP unchanged. Under `img-src 'self'` Vite must not inline assets as `data:` → set
  `build.assetsInlineLimit: 0`; prefer CSS hard-stop gradients and local SVG (`shape-rendering:
  crispEdges`) files.
- Keep every existing flow and caller: `App` → `StatusScreen` → `CreateWorkspace` / `ErrorScreen`
  (incl. PartialCreate repair) / `WorkspaceScreen` (`MemberList`, `AgentList`, `RegisterAgent`,
  profile form), `client.ts`, `yardData.ts`, `sceneLifecycle.ts`. Restyle and restructure markup;
  do not change what is sent or shown as state/provenance.

## Visual system (provisional tokens in `app/src/styles.css` `:root`)
- **Grid:** 4 px base unit; all sizes, borders, shadows multiples of 4 (2 px only for inner lines).
- **Palette (provisional, ≤ 16 colours):** night ground `#14111f`, deep `#1f1a33`, panel `#2b2645`,
  panel-light `#3d3663`, line `#0b0912`, text `#f4ecd8`, muted `#b9b0d0`, grass `#3e8948`,
  grass-dark `#265c42`, path `#c28b56`, wood `#8a5a3b`, accent (Claude Code) `#e8823a`, accent
  (Codex) `#4fa4e8`, ok `#7bd389`, warn `#f4c542`, danger `#e5534b`. Text/background pairs ≥ 4.5:1.
- **Texture everywhere:** body/background = repeating 4 px dither (CSS `repeating-conic-gradient`
  or local SVG tile); panels/headers = brick/plank pattern at low contrast; forms = inset wells;
  setup, error and loading screens use the same textures. Texture never behind body text without a
  solid text backing.
- **Shapes:** `border-radius: 0` everywhere. Panels, buttons, inputs, badges: 4 px solid borders
  with stepped (notched) corners via `clip-path` polygon or a local 9-slice SVG `border-image`;
  hard shadows `4px 4px 0 var(--line)` (no blur, no smooth gradients).
- **Buttons:** pixel "key" look (light top edge, dark bottom edge), pressed state shifts 4 px and
  drops the shadow; disabled = desaturated + hatched texture (not only opacity). ≥ 44 px targets.
- **Type:** headings, labels, badges in system monospace (`ui-monospace, Menlo, monospace`),
  uppercase, integer sizes; body in the system sans at ≥ 15 px. No remote or added font files.
- **Badges:** mode ("STRICT P2P"), provenance ("VERIFIED · STOP" / "INFERRED · TOOL SHELL"),
  "NO EVENTS YET" as square pixel tags with a small pixel icon (SVG), never pills.

## Layout
- Workspace (960×680): pixel title bar (workspace name, mode tag, short ids) → **yard as the
  visual centre** (full width, ≥ 55 % of the first viewport) → beneath it a two-column "signpost"
  row: Agents (list + register form) | Members + Peers + profile. Footer status line in the same style.
- Narrow (375 px): single column, yard first, integer-scaled to fit width.
- Setup: a single framed "sign" panel on the textured ground with the existing three fields; Auto
  option still visibly disabled with the G-1 text.
- Error: same frame with a danger-coloured header strip and the existing advice + actions.
- Loading: same ground with a static pixel "Opening local workspace…" sign (no spinner animation).

## Yard (`app/src/playground/yard.ts`, `yardData.ts`)
- Logical resolution 320×176 px (16 px tiles), rendered with nearest-neighbour: Pixi
  `TextureStyle.defaultOptions.scaleMode = 'nearest'`, `roundPixels: true`, canvas CSS size =
  logical × **integer** scale (`max(1, floor(hostWidth/320))`, re-applied by a ResizeObserver),
  `image-rendering: pixelated`. Labels as pixel tags in the HTML overlay or Pixi bitmap-like text at
  integer scale — must be legible at 375 px.
- Ground: tiled grass with a path network; **stations** (original pixel props) for exactly the
  states the pipeline can produce (PROTOCOL §9 v1.2): Arrival pad = "No events yet", Desk =
  Thinking, Workbench = Building, Test bench = Testing, Merge gate = Merging, Flag = Finished,
  Bench = Idle, Alarm = Error. Reviewing/Fixing/WaitingForInput get no station (not produced).
- **Characters:** one 16×16 original pixel sprite per real `AgentView` (variant by `AgentKind`
  accent), standing at its state's station, name tag below; ≥ 2 agents at a station are placed on
  distinct tiles. Static (no idle animation, no movement until real state changes exist).
- **Empty yard:** environment and stations shown, a wooden sign "Register an agent to see it here",
  **no** characters, peers or activity. No peer characters at all (networking not built).
- The accessible `AgentList` stays the equivalent of the yard (same DTO, same labels); canvas
  `aria-hidden`. Renderer failure text unchanged in meaning.

### Responsive yard (amendment 2026-11-03, after review)
- Host width ≥ 640 CSS px: 4-column layout, logical 320 wide, integer scale `floor(host/320)`.
- Host width < 640: **2-column** layout, logical 160 wide (two stations per row, four rows, same
  eight stations in the same order), integer scale `max(1, floor(host/160))` — 2 at 375 px.
  Same real agents/states; nothing hidden or invented. Height grows accordingly (yard stays first).
- Name tags may wrap to two lines within the tag width before an ellipsis, in both layouts.
- Crisp integer pixels and no horizontal overflow in both layouts; the accessible list is unchanged.

## Code shape (minimal)
- Rewrite `styles.css` around the tokens; add `app/src/assets/pixel/*.svg` only for textures,
  borders, icons and props actually referenced. Small sprite/prop definitions as typed pixel maps in
  `app/src/playground/sprites.ts` (palette indices → Pixi `Graphics` rects), used by `yard.ts`.
- No theme framework, no CSS-in-JS, no new components beyond what the layout needs (e.g. a
  `PixelPanel` wrapper only if used by ≥ 3 screens). Every new export has a named production caller.
- Keyboard, labels, focus-to-`h1`, `:focus-visible` (pixel outline), reduced motion and contrast
  kept; no transitions beyond the 4 px press offset (disabled under reduced motion).

## Acceptance
1. Screenshots (PNG, stored under `/Volumes/MeshyardProjects/Meshyard-build/orchestration/t015-pixel-evidence/`)
   for setup, workspace with 0 agents, workspace with ≥ 3 agents in different states, every
   distinct error layout (one generic + PartialCreate), loading — each at **960×680 and 375×680**.
   Each file name/README states its evidence level:
   - (a) **native app with real backend** (parent may run a normal ad-hoc debug start with a fresh
     USB file-key profile; no OS-permission bypass, no Keychain, no hook install) — preferred for
     setup/empty/real-registered agents ("No events yet");
   - (b) **headless/browser with mocked IPC** (dev-only fixture entry using the existing
     `@tauri-apps/api/mocks`, excluded from the production build) — allowed for states not yet
     producible (Testing, Error…) and error layouts; explicitly **not** native or persistence evidence.
2. Visual checklist reviewed on the screenshots: no `border-radius`, no blurred shadow, texture on
   background/panels/headers/forms/buttons/badges/setup/error/loading, yard is the largest element,
   crisp integer-scaled pixels (no blur), text legible at 375 px.
3. Behaviour unchanged: existing Rust tests untouched and passing; Vitest updated only for changed
   markup (same assertions on honest states: "No events yet", no false Idle/Verified, error kinds).
4. Production bundle contains no `data:` asset URLs and no remote URL (grep in report); CSP file
   unchanged (diff).
5. `npm run check` once after the changes, green. Report lists each new module/asset with its
   caller; no unused assets.
6. Native gate N-14 (Dion) and the exact-reference visual acceptance remain separate and pending.

## Hand-off
After Claude's review, T-007b's frontend part (§4 live UI) builds on T-015's files; T-007b backend
prep (`app/src-tauri/src/ingest.rs`, store binding) may proceed in parallel since it touches no
T-015 file.
