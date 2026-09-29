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

### Terrain blending (landed)

`src/blend.rs`: base tiles draw a cell of the 9x9 transition sheets
chosen from the four vertex terrains (cell = (3S+E)*9 + 3W+N; see
`reverse-engineering/blending.md` update). Ice keeps its unblended art;
overlays (hills, forest, mountains) are unchanged. City screen uses the
same cells. Deviations: vertex priority and sheet selection are inferred.
