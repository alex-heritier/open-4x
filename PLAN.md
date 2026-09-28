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
