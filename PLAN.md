# Civ3 Clone Plan

Status: Final. Accepted by the owner via the grill interview. All items settled, none open.

## Goal

A playable solo sandbox: open straight into a new game as Japan, move units, explore a random map, found cities, end turns. Built with Rust Bevy under `civ3-clone/`, using art and audio converted from the local GOG install.

## Non-goals for MVP

Settled: no AI civs, no technology, no diplomacy or trade, no multiplayer, no worker improvements (D4), no save or load, no minimap, no combat targets (no huts, no barbarians), no main menu or settings.
Proposed, not yet accepted: none.

## Settled decisions

- D1: Engine is Rust Bevy, project lives under `civ3-clone/`.
- D2: One fixed scenario. Japan, straight into a new game.
- D3: Starting party is one settler, one worker, one warrior, one scout.
- D4: Worker in MVP can move and fortify only.
- D5: Random continent map with a fixed default seed.
- D6: Asset pipeline pre-converts PCX to PNG and FLC to PNG strips (ffmpeg). The game loads only PNG and WAV.
- D7: Terrain renders one sprite per tile with no edge blending in MVP.
- D8: Rules are hardcoded for the MVP unit and terrain set. The BIQ/BIC binary rules files are out of scope.
- D9: City model is Civ3-like: worked tiles with yields, food box growth, shield box production.
- D10: City screen recreates the full original Civ3 city view using the city screen art.
- D11: Music is DipASEarlyPeace converted to OGG at prep and looped, with Menu1 on the greeting splash.

## Constraints

- Converted and raw GOG assets stay local and gitignored. They are purchased assets and are never committed.
- Primary target is macOS on arm64.
- Bevy version is pinned via Cargo.lock and updated deliberately.

## Risks

- FLC facing order: direction-major layout is verified (default 8x16, run 8x11), but which strip maps to which compass direction is still unknown. Resolved visually once units render.
- Full terrain blend decoding is deferred; MVP will look tiley at borders.
- Isometric picking and sprite sort order need a grid overlay test.

## Validation

Each phase ends with `cargo run` plus a visual check. No phase depends on a later one to be testable.

MVP landed and verified: `cargo test` 13/13 green; `cargo build`
clean; screenshot evidence for splash, map with founded Kyoto and
banner, and full city screen; live keypress E2E (splash dismiss,
B founds Kyoto, build sound plays without crash). Mouse click paths
verified by code plus unit tests (tile picking roundtrip, city
screen cursor math); OS event delivery is Bevy/winit machinery.

## Scope contract (accepted)

- Artifact boundary: Rust source under `civ3-clone/src/`, asset prep tool under `civ3-clone/tools/`, this plan, and a README. Out of scope: later-stage specs, tests beyond smoke checks, packaging or installers.
- Done means: app opens directly into a new game as Japan with no menu; settler, worker, warrior, and scout spawn on a random map (fixed default seed); units select and move by click, arrow keys, and right-click path; fog of war updates; settler founds a city; city grows via food box and builds warrior, settler, or worker via shield box in the full city screen; end turn cycles; Tokugawa greeting splash shows; UI and unit sounds plus the Asian peace music loop play.
- Deferred stages (worker improvements, blending, barbs and huts, city screen, save or load, minimap) each return for their own interview. Accepting this record never approves them.

## Unresolved

- SETTLED-1: MVP boundary is the full loop (terrain, movement, fog, settling, growth, production, end turn, splash, sounds, music).
- SETTLED-2: No save or load. Single-session sandbox.
- SETTLED-3: No minimap. Pan and zoom only.
- SETTLED-4: City model is Civ3-like food and shields (worked tiles, food box, shield box).
- SETTLED-5: Neither huts nor barbarians. Pure sandbox.
- SETTLED-6: City interaction is the full Civ3 city screen recreation.
- SETTLED-7: Music is the Asian early peace loop plus Menu1 on the splash, converted to OGG at prep.
- SETTLED-8: No main menu or settings. The app opens directly into the new game.

## Phase 2: map features (landed, supersedes SETTLED-5)

Goody huts, barbarian camps, and resources per `reverse-engineering/NOTES.md`
§11.8-11.10 (exact stage seeds, quantity math, Fisher-Yates, 1-in-3 camp
gate, block-3 odds). Deviations: hut/camp counts implement evident intent
(the binary's hut stage is a no-op under 32 civs); GOOD frequencies and the
TERR allow-matrix are hardcoded until BIQ framing lands; luxury/strategic
goods give no yield yet (trade slice); camps are capturable, barb units and
combat wait on RE. Verified: `cargo test` 30/30, `cargo build` clean, live
screenshots (hut adjacent to start, hover label, hut pop with Warrior
reward, bonus clustering, camp render). Demo: `MAP_SEED=2` puts a hut at
(42,30) next to the start (43,30); pinned by
`features::tests::demo_seed_layout_is_stable`.

## Phase 3: worker improvements (next, RE-ready)

`graphics-terrain.md` gives exact overlay tables (roads 16x16 neighbor
mask, irrigation 4x4 edge mask) and Worker FLCs exist for
ROAD/MINE/IRRIGATE/FORTRESS/JUNGLE/FOREST/PLANT. Also port `GameRng`
(`ai.rs`, exact) for gameplay randomness. Deferred: terrain blending
(letter mapping still inference), rivers (hypothesis), combat/AI
(strings only), BIQ data (DCL mode 0 done, `biq.md` pending).

### Phase 3 progress

Landed: roads, irrigation, mines (M: hills/mountains +2 shields, desert +1,
6 turns), and clearing forest/jungle/pine on flat land (C, 4 turns). Work
sounds per action via `GameAudio::work_sfx`. Not yet: mine overlay art
(state only, shown in hover text), fortress, plant-forest, `GameRng` use
for gameplay randomness, worker auto-mode.

### Unit controls, pathfinding, action bar, city management (landed)

- `actionbar.rs`: `UnitCommand` messages from keys or the bottom bar
  (Go to, Skip, Sentry, Fortify, Wake, Found City, Road, Irrigate, Mine,
  Clear, Disband); buttons show per unit type and dim when unavailable.
  Go to (G) then click; Esc cancels. Map picking is off over the whole bar,
  so clicks on the strip or the 3-px gaps between discs never reach the map.
- Bar art is Civ3's own: `Conquests/Art/interface/{NormButtons,
  rolloverbuttons,highlightedbuttons}.PCX`, with `ButtonAlpha.pcx` for the
  disc shape, converted by `tools/prep_assets.py unitbuttons` into
  `gen/ui/unitbtns_{norm,over,down}.png` (an 8x10 grid of 32-px cells). Cell
  order is the `#UNIT_ACTIONS` order of `Conquests/Text/labels.txt`
  row-major, checked against gameplay screenshots: a Warrior's seven buttons
  are cells 0-6 (skip, wait, fortify, disband, go to, explore, sentry). The
  bar packs the discs edge to edge in that order, hover shows the rollover
  art, a held or armed button the blue "highlighted" art, and unavailable
  ones are darkened. The hovered command's name replaces the unit readout,
  as Civ3's help line does. Deviation: Civ3 has no Wake button on the map
  panel (Wake is a right-click entry there), so Wake reuses Explore's
  circular-arrow art, the one action this clone has no command for.
- Pathfinding: hover shows a route preview and step/turn estimate;
  units with any movement left may enter a costlier tile (fixes 1-MP
  units never entering hills/forest).
- Cities: buildings (Barracks, Granary, Temple), production queue,
  change-build modal (Build now / Queue), class-switch shield penalty,
  surplus shield carry-over, Granary keeps half the food box. City screen
  has food/shield fill bars, citizens, owned buildings, queue panel,
  Governor button. V opens the city under the selected unit.

### City screen, construction, tile improvements (landed)

- Improvements: mine overlay art (`TerrainBuildings.PCX` col 2 row 1, prep
  stage `improvements` -> `gen/improvements/mine.png`); a mine replaces
  irrigation and vice versa; workers on one tile and job pool labor.
  Movement is counted in thirds (`map::MP`): road to road costs 1/3 MP, and
  city tiles carry a road.
- City tiles: the center yields its own terrain, irrigated for free when it
  could be, with at least one shield. Other cities' worked tiles and
  unexplored tiles cannot be worked. Growth adds the best free tile and
  keeps manual picks; shrinking drops the worst.
- Construction: Settlers need size 3 (held at full cost, announced once);
  unit completion, growth and starvation post messages.
- City screen: Civ3 food/shield icons from `CityIcons.pcx` in a top layer;
  road, irrigation and mine overlays; dimmed foreign tiles; entertainer heads
  for idle citizens (`popHeads.pcx` row 16 col 1, prep stage `cities`);
  correct `buildings-small.pcx` cells (32-px grid). The screen rebuilds only
  when the city, menu or its radius tiles change.
- `CIV3_SCRIPT` input driver for unattended captures (`src/script.rs`).
- Open (partly resolved 2026-09-29): rule numbers stay hardcoded. The old
  "`conquests.biq` decodes to corrupt records" note was a `.biq` decode bug
  (`reverse-engineering/biq.md`): the file now parses cleanly and carries
  the GOOD names/frequencies and the 14 TERR resource allow-masks, so the
  hardcoded tables can be replaced by the real rules — that wiring is not
  done yet. Civ3's Worker pop cost and the governor's food-first weighting
  are still unverified.

### Civ3 map HUD and selection (landed)

- No bottom bar: action discs float over the map; Civ3's `box right`
  panel (bottom right) holds the unit readout and the `nextturn states`
  disc (prep stage `hud`). With no unit needing orders nothing is selected,
  and the disc and "Press ENTER or click here for next turn" blink.
- Units with no moves left cannot be selected; auto-select moves to the
  nearest unit that needs orders.
- City sprites anchor at the 167x95 cell center (they drew a tile north).

### Terrain blending (landed)

`src/blend.rs`: base tiles draw a cell of the 9x9 transition sheets
chosen from the four vertex terrains (cell = (3S+E)*9 + 3W+N; see
`reverse-engineering/blending.md` update). Ice keeps its unblended art;
overlays (hills, forest, mountains) are unchanged. City screen uses the
same cells. Deviations: vertex priority and sheet selection are inferred.

### Civ3 city panel (landed)

The city screen now presents Civ3's own city panel rather than a bespoke
layout: the top bar carries the civ's strategic resources (one count per
good in the workable radius of the cities) and the city's readout (name,
founded turn, treasury, government, population, turn, and culture with its
next border expansion and powers-of-ten total), plus the previous/next
city arrows and the close button from `cityMgmtButtons.pcx` (three states
each, hover/press art via `update_panel_buttons`). The city's land stays in
the middle with the citizen heads along the bottom (`popHeads.pcx`), and
the bottom panel holds the improvements list (Palace for the capital, then
each owned building with its culture notes, upkeep figure and happy face),
luxuries by source count, the list's scrollbar, pollution, the garrison,
and the production, food and commerce rows: one icon per unit of per-turn
output, commerce split into tax, science and luxury (Civ3's 50/50/0, each
share rounded down), the food box and granary grids, the current build's
shield grid, and Civ3's production button with its "Complete in N turns".
The prep `cities` stage also bakes the fade bars' Alpha sheets into the
alpha channel and crops the scrollbar from `Art/scroll.pcx`.

Model pieces added as the panel's data sources: `cities::tile_commerce` /
`city_commerce` (water yields commerce, roads add one, the city tile always
one), `commerce_split`, `Production::{upkeep, culture, happy}`,
`City::{culture, founded}` with `culture_thresholds` powers of ten, and a
`Treasury` that accumulates the tax share each turn.
Deviations to revisit: no happiness model, so luxury rows show one face per
pair of sources; population shows the citizen count rather than Civ3's
scaled figure; the calendar is the turn number; government is fixed to
despotism; the scrollbar is chrome, since at most four improvements exist.
Verified: `cargo test` 78/78, `cargo build` clean, and screenshots of
Kyoto (granary, roads, garrison, culture box, shield grid) plus the top
bar's arrows walking Kyoto <-> Osaka.

### Cultural borders (landed)

Cities now claim plots on the map and the map draws Civ3's dashed border
ribbon around them. `cities::{culture_level, culture_radius, territory}`
turn culture into a radius: level 1 (the founding state, culture < 10)
reaches every tile within two tiles — the same 21 tiles the work radius
has — and each further power of ten adds a ring, to Civ3's five-tile cap.
The nearest city owns a tile; on a tie the city with more culture wins,
then the older one, so ownership is stable frame to frame. `resources_owned`
counts goods inside the real borders now instead of standing in the work
radius, which is the same set until a city's culture passes 10.

`src/borders.rs` draws a ribbon on a tile edge only when the tile is owned
and the tile across that edge belongs to another city or nobody, which
keeps the ribbon inside its own territory and gives two cities of one civ
a line down the middle, the way Civ3 shows internal borders. Art is
Civ3's own `Art/Terrain/Territory.pcx` (prep stage `borders`): the four
straight cells of its 2x4 sheet, baked white and tinted with the civ's map
color, `cities::CIV_BADGE` (Japan's white). Ribbons hide on never-seen
tiles and the fog diamonds dim remembered ones; they sit above the
improvement overlays and below units and cities.

Deviations to revisit: the sheet's second column (a curved variant of each
edge) is unused — no game screenshot shows it, and the straight column
reproduces the game's zigzag at every corner; the shape of a growing
border is Euclidean distance, which fits the 21-tile start and Civ3's
first small expansion, but the later rings are unverified; water tiles are
claimed by distance like land, with no coast rule.
Verified: `cargo test` 83/83, `cargo build` clean, and screenshots of
Kyoto's border at level 1 and again after 30 turns of culture, one ring
further out.

### Unit selection ring and the move preview (landed)

Selection now looks like Civ3's. The selected unit wears the game's own
dashed ellipse, `Art/Animations/Cursor/Cursor.flc` (prep stage `cursor`:
31 frames of a 93x46 crawl, 175 ms each per `Cursor.ini`; the red under
the dashes is Civ3's shadow, so it bakes to a translucent dark outline) —
not the interface art, which has no selection sprite. `units::SelectionRing`
loops those frames and `units::ring_follow` keeps it under the selected
unit. A plain hover no longer draws a route: with a unit selected the tile
cursor is the destination marker, so it only appears while a route is
being aimed.
`input::MovePreview` is the single source for that: the armed Go-to
command previews under the pointer, and otherwise only a press held on a
tile for `input::HOLD_SECS` (0.3 s) does — a quick click still just
orders the move, and the preview follows the pointer while the button is
down. The left button never pans the map, as in Civ3: panning is W/A/S/D
and the wheel zoom (`input::camera_control`). The route line and the end
marker (`units::selection_gizmo`) and the "path N steps, M
turns" readout (`ui::update_hover_label`) both follow the preview, so
they show for Go-to, for the held press, and never for a hover.

Deviations to revisit: Civ3 also draws the selected unit's readout (moves
left, home city) in its bottom-right box, which the clone already labels
with the unit and its turns; the ring has no civ color, matching the
game's white ellipse; the hold delay is a guess (0.3 s) set by feel; the
ring's 4 dark notches are part of the FLC's frame and are left as drawn.
Verified: `cargo test` 92/92 (7 hold/preview and 2 ring tests), `cargo build`
clean, unattended captures of the plain selection, the held preview, the
move after release and the armed Go-to preview, plus a live cliclick
press-and-hold on the running game showing the route appear and the unit
move on release.
