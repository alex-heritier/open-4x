# Civ3 Clone Plan

Status: Final. Accepted by the owner via the grill interview. All items settled, none open.

## Goal

A playable solo sandbox: open straight into a new game as Japan, move units, explore a random map, found cities, end turns. Built with Rust Bevy under `civ3-clone/`, using art and audio converted from the local GOG install.

## Non-goals for MVP

Settled: rule-based AI civs only (`src/ai.rs`: one skill level), no multiplayer, no trade of cities or world maps (technology, diplomacy and the wonder screens were added after the MVP: `src/research.rs`, `src/diplomacy.rs`, `src/advisors.rs`, and for the animated leaderheads, their speech and the wonders window `src/leaders.rs`, `src/speech.rs`, `src/wonders.rs`), no worker improvements (D4), no save or load, no minimap, no combat targets (no huts, no barbarians), no main menu or settings.
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
per-civ `Treasury` (its income, upkeep and bills are under "Economy"
below).
Deviations to revisit: no happiness model, so luxury rows show one face per
pair of sources; population shows the citizen count rather than Civ3's
scaled figure; the calendar is the turn number; government is fixed to
despotism; the scrollbar is chrome, since at most four improvements exist.
Verified: `cargo test` 78/78, `cargo build` clean, and screenshots of
Kyoto (granary, roads, garrison, culture box, shield grid) plus the top
bar's arrows walking Kyoto <-> Osaka.

### Cultural borders (landed)

Cities now claim plots on the map and the map draws Civ3's dashed border
ribbon around them. `cities::{culture_level, culture_reach_sq, territory}`
turn culture into a shape, read off the border owners in the shipped saves
(`reverse-engineering/economy.md`): a city claims every tile within squared
distance `level² + 1`, so the 3x3 square at the start (culture < 10), the
21-tile city radius from culture 10, 37 tiles from 100, and the same rule
on up to level 6 (levels 4 to 6 are a **HYPOTHESIS**, no save is that old).
The nearest city owns a tile; on a tie the city with more culture wins,
then the older one, so ownership is stable frame to frame. `resources_owned`
counts goods inside the real borders now instead of standing in the work
radius.

`src/borders.rs` draws a ribbon on a tile edge only when the tile is owned
and the tile across that edge belongs to another civ or nobody, which
keeps the ribbon inside its own territory and merges the cities of one civ
into one outline, as Civ3 does. Art is
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

### Economy: tile working, growth and gold (landed)

Cities now run Civ3's food, shield and gold loop. The pure rules are in
`src/economy.rs` (each with a test); `cities::end_turn_cities` applies them
when a civ hands the hotseat over. Findings and addresses are in
`reverse-engineering/economy.md` ("Growth and the food box", "Gold: unit
support and upkeep"), mirrored in `reverse-engineering/rust/src/economy.rs`.

- Working tiles: each citizen works one tile of the 21-tile radius, picked
  by the governor (food, then shields, then commerce) or by a click on the
  city screen. A tile is out of reach when it is any city's center, another
  city is working it, or it lies inside another civ's border
  (`cities::taken_tiles`). When a city is founded on a picked tile, a border
  grows over it or a neighbor takes it, `reconcile_tiles` drops the pick and
  the governor refills the gap. Under Despotism a worked tile's yield above
  2 loses 1 (`economy::despotism`); the city center is exempt.
- Growth: the food box is 20 / 40 / 60 for a town (size 1-6), city (7-12)
  or metropolis (13+); a city grows when stored food plus the surplus
  reaches it. A Granary keeps half of the box that filled; without one the
  store empties and the overflow is lost. A negative total empties the
  store and costs a citizen (a size 1 city only loses the food).
- Gold: tax is the tax share of each city's commerce. Improvements cost
  their upkeep. Units cost 1 gold each past a free allowance of 4 per city,
  pooled civ-wide (the Despotism row of the game's GOVT table); a civ with
  no city pays nothing. The books close before the cities build or grow, so
  a unit completed this turn is charged from the next one.
- Shortfalls: the tax joins the treasury, upkeep is paid first and units
  second. Upkeep the gold cannot cover sells one improvement, whose shield
  cost joins the treasury, and the rest of that bill is forgiven. Unit
  support the gold cannot cover empties the treasury and disbands the
  cheapest unit to rebuild (a Warrior before an equally priced Worker).
  Both post the game's own wording.
- The bottom-right info box shows the active civ's `N Gold (+M per turn)`,
  from the same `economy::finance` the turn settles (a `debug_assert`
  checks it); a deficit is red. The city screen's food, shield and
  commerce rows read the same per-city yield functions.

Deviations to revisit (the sale price, growth caps, corruption and the
happiness model have since landed, see the next section): Civ3 rolls a die to pay units before upkeep one turn
in four and starts its sale at a random city, and the clone is
deterministic; the sale price is the shield cost, since the binary's divisor
is not recovered (`[0x9C7268]`); no Aqueduct or Hospital growth caps (no such
buildings, no fresh water); no corruption or waste; the happiness model is
still absent, so no disorder; no captured-unit exemption (no capture).
Verified: `cargo test` 162/162, `cargo build` clean; the reference crate's
222 lib tests pass; screenshots of the city screen (commerce icons, "Growth
in 10 turns" with a 20-cell food box); and a scripted run (one city, eleven
units by the game's own `report`) whose info box read `0 Gold (-7 per turn)`
in red, then, after the turn disbanded a Warrior, `(-6 per turn)`.

### Governments, happiness, research, upgrades and unit actions (landed)

The rest of the Civ3 loop, each rule from the executable where
`reverse-engineering/` read it, with the pure part in the `civ3mapgen` crate
and the Bevy part a thin system over it. Tests: `cargo test --bin civ3-clone`
(324) and the crate's `cargo test --lib` (592).

- **City numbers** (`src/citycalc.rs`): one source for a city's food, shields,
  commerce, science, upkeep, mood and disorder, from the tile yields under the
  civ's government (Despotism/Anarchy penalty, Republic/Democracy trade bonus),
  the city-centre rule (`yields.md` 4, with the Industrious/Commercial bonus),
  corruption by distance from the Palace, the buildings in effect (wonders'
  gifts, obsolescence), martial law, luxuries, and the Aqueduct/Hospital size
  limits. `src/realm.rs` mirrors each civ's government, rates, known advances
  and holdings for plain functions.
- **Government** (`src/govern.rs`, `src/domestic.rs`): revolution with the
  executable's anarchy length (halved for a Religious civ), the Domestic Advisor
  (F1) with the tax/luxury/science rates and a government list, the computer's
  score-based choice (favorite and shunned governments) and rate setting.
- **Research** (`src/research.rs`): the 83 advances with the executable's cost,
  contact discount and the computer's valuation; the whole roster gated by
  advance, resource, race and obsolescence.
- **Unit upgrades** (`src/upgrades.rs`, `civ3mapgen::upgrade`): `U` and `Shift+U`,
  the furthest buildable successor, the price with Leonardo's halving, the
  facility by domain, the AI's habit. Golden vectors U1-U9 are tests.
- **Promotions**: the executable's die (`combat.md` 6.2), halved for a
  Militaristic civ; a failed roll marks the unit for the turn.
- **Unit actions** (`src/actions.rs`): Join City, Pillage, Automate (the AI's
  worker brain run for the human's workers).
- **Computer opponents**: roster-aware choice of defenders and attackers by
  `PRTO` strategy bits, buildings by need (size limits, contentment, science,
  trade), Wealth as the sink, gold upgrades.
- **Traits** (`cities::price_for`): the half-price improvements of Militaristic,
  Religious, Agricultural, Seafaring and Scientific civs, the Palace's price by
  empire size, the centre bonuses, Religious anarchy.
- **City defense**: Walls (towns only) and Civil Defense in the odds
  (`combat::Hold`).

Still HYPOTHESIS (flagged where they stand): corruption by distance, the
computer's tax rates and upgrade reserve, the human's next government being
chosen when a revolution starts, healing rates, the pillage order and rule,
automation by the computer's own worker brain.

Not landed: railroads, fortresses, pollution and plant-forest jobs (no art
prepared), naval/air ranged combat, Theory of
Evolution, war weariness, the scientific leader, save/load.

### Ancient Age continuation: Great Library (landed)

The Great Library now grants eligible advances known by two contacted
civilizations at the owner's research step (`research.md` 8). Ownership is
read from the cities each turn, so capture transfers the effect and removing
the wonder stops it. The building's government requirement and obsoleting
advance are respected. Obsolescence is checked after research completion:
completing Education prevents grants that same turn. Free advances unlock
production, prompt for a new research target when needed, and announce the
Library as their source.

Verified: 326 game tests, 593 reference-model tests, and `cargo build`.
The Bevy regression runs the contact refresh and actual research end-turn
system; no rendered playthrough was run for this slice.

Full Ancient Age equivalence remains incomplete. Confirmed gaps include
naval play (Galley is not playable and embarkation/transport is absent),
terrain strikes and exact retreat movement. A complete Ancient Age playthrough and
an audit of the remaining wonder effects are still required.

### Ancient Age continuation: mounted-unit retreat (landed, movement provisional)

Fast land units now use the executable's retreat eligibility and experience
rolls (`combat.md` 7). Neither side retreats when both are fast; defenders
cannot retreat from cities. The round log includes retreat draws in the
original order, including a defender's failed escape. A retreat stops damage
at one HP and plays no death clip; both combatants and the defending stack
survive, with no promotion. Attackers stay on their starting tile; defenders
step away, and an attacker occupies the emptied tile only for a lone defender.

HYPOTHESIS remains for `0x5BFB60` movement: the defender attempts the adjacent
tile directly away from the attacker, rejecting impassable terrain and foreign
units or cities. Retreat consumes the retreating unit's remaining movement.
These placement and movement-cost details require executable verification
before claiming Civ3 equivalence.

Verification includes a round/damage/RNG comparison against the reference
duel across both retreat outcomes, full Bevy sequences for both sides, blocked
escapes, city defense, two fast combatants, surviving bystanders, and wrapped
escape coordinates. A seed-2 scripted rendered battle left Japan's Chariot at
`@1,0`, 1/3 HP and zero movement, and the enemy Warrior at `@2,0`, 2/3 HP.
The screenshot `/tmp/open4x-retreat.png` was inspected alongside the runtime
report `/tmp/open4x-retreat-run.log`.

### Ancient Age continuation: Catapult bombardment (landed)

Catapults now have the original Bombard action button and B key: select a
visible target in range, click to fire, or Esc to cancel. A shot requires war,
spends one move, and marks the unit as having attacked without moving it.
The strongest eligible defender is selected with the reference comparator;
ordinary artillery cannot hit land or sea units already at one HP or reduce
them below one HP. The shared combat RNG directly drives the reference
volley, wall-interception and city-strike routines. Town Walls absorb the
attack and may be destroyed; gifts such as the Great Wall's Walls only report
the hit and allow the garrison pass. When no unit qualifies in a city,
bombardment may kill a citizen (minimum size one) or destroy an ordinary
building, preserving wonders.

AI artillery can approach known hostile targets and fire through the same
resolution system. HYPOTHESIS: its production ratio is one artillery per
four soldiers, after meeting garrison, worker and expansion needs. The exact
AI production ratio and random non-wonder building selection remain
unrecovered. Bombarding in peace currently refuses the order instead of
opening a war-declaration prompt. Destroying terrain improvements still needs
wiring, so this is not complete bombardment parity.

Verified: 338 game tests and `cargo build`, including reference damage/RNG
comparisons, city walls, city strikes, peace/range/fog refusals, human keyboard
and click input, and AI choice/production. Rendered seed-2 and seed-1 runs
exercised a miss and a hit. The latter left the stationary Catapult at `@1,0`
with zero movement and the enemy Spearman at `@2,0` with 2/3 HP; screenshot
`/tmp/open4x-bombard-hit.png` was inspected against its runtime report.

### Ancient Age continuation: defensive bombardment (landed)

Before melee, the strongest eligible supporting unit on the defender's tile
fires one shot (`combat.md` 8.9). The defender itself is excluded; a shooter
must match the attacker's domain, have bombard strength, lack ability 3, and
have its separate defensive-fire flag clear. A land attacker needs defense
above zero and at least two HP. The reference routine rolls raw odds without
terrain or fortification and marks the shot used on a hit or miss. Its damage
feeds the subsequent melee dice, while the sequencer shows the supporting
attack before applying the damage and starting the first melee round.

Defensive fire does not spend movement or the offensive attack allowance.
Its flag resets when the shooter's civilization begins its next turn. A hit
leaving the attacker at one HP records the reference incident. Attacked units
keep their fortified order, as confirmed at `0x4A56F1`. The AI's fight estimate
weights both the supporting shot's hit and miss outcomes.

Verified: 342 game tests and `cargo build`. Tests compare the supporting shot
and subsequent melee damage and RNG sequence with the executable model, cover
shooter choice, the HP floor, reuse limits, incoming-civ reset, fortification,
and the AI's valuation. A rendered seed-1 run showed the defensive-hit message
and attacker at 2/3 HP before the defender took melee damage; capture
`/tmp/open4x-support-180.png` was inspected alongside the runtime report.

Remaining combat fidelity audits include retreat placement/movement.
Naval transport, remaining wonder effects, and a full
Ancient Age playthrough remain outstanding.

### Ancient Age continuation: defending stack survival (landed)

Removed the outside-city whole-stack deletion. The executable's melee
victory paths kill the losing fighter (`0x4A63EF`, `0x4A6EF2`); the kill
routine's recursive pass only considers units linked to it by carrier id
(`0x5BC000..0x5BC009`). Ordinary soldiers, Workers and artillery sharing
the tile survive. This partial finding is recorded in `combat.md` 6.3;
the complete cargo and destruction bookkeeping remain open.

Verified: 343 game tests, `cargo build`, reference release tests, and
reference release clippy (existing warnings). Regressions cover surviving
military/civilian/artillery units and a later move capturing a surviving
Worker. A rendered seed-1 run left the victorious Swordsman at `@0,0`
and the enemy Worker at `@1,0` with 3/3 HP after its defending Warrior
died. `/tmp/open4x-stack-survival.png` was inspected alongside
`/tmp/open4x-stack-run.log`.

### Ancient Age continuation: terrain bombardment (landed for existing improvements)

Catapults can now target road, mine and irrigation improvements when no legal
unit or city target takes precedence, including a defender already at the
nonlethal one-HP floor. The reference tile strike uses implicit strength 16,
terrain defense and one die (`combat.md` 8.8, `0x4A2460`). A hit clears roads,
mines and irrigation together (`colonies.md` 8, `0x5B4DC0`); a miss preserves
them. Movement, attack allowance and the firing animation use the same path
as unit bombardment. Own or peaceful territory is refused before spending
movement or dice; unowned improvements are legal targets. Improvement sprites
and their neighbor masks already synchronize from the map.

The AI can fire on known enemy improvements in range after checking nearby
unit/city targets. HYPOTHESIS: this targeting priority is game policy; the
native AI's exact terrain-target priorities remain unrecovered.

Verified: 347 game tests and `cargo build`, including reference hit/miss and
RNG comparisons, clearing all improvements together, target precedence, HP
floor, ownership refusals, and AI war/fog choices. Rendered seed-29 and seed-7
keyboard/click runs exercised a miss and hit. The miss retained Road and Mine;
the hit removed both and left the Catapult at `@1,0` with zero movement.
Captures `/tmp/open4x-terrain-250.png` and
`/tmp/open4x-terrain-hit-145.png` / `250.png` were inspected.

Fortress/Barricade and Railroad destruction need those map improvements first.
Naval transport, exact retreat movement, remaining wonder effects and a full
Ancient Age playthrough still need completion.

### Ancient Age continuation: Galley sailing foundation (landed; cargo incomplete)

The generated roster enables the Galley, with its native stats and converted
animation/sound assets. Sea-unit production requires a coastal city bordering
a water body larger than 20 tiles (`buildable.md` 3.1, `0x4C05BB`). The same
path search now accepts domain-specific entry costs: Galleys sail Coast, Sea
and Ocean for one movement point per step and enter friendly ports; ordinary
land units still cannot enter water. Manual clicks, arrow orders, Go-to
previews, scripted moves and auto-exploration use the unit's movement domain.

Ship allowance includes Seafaring and the native active wonder flags for +1
and +2 movement (`movement.md` 2). The turn boundary rolls sinking only on
unsafe water (`unit-turn.md` 3.2), respecting sea/ocean technology immunity,
Safe Sea Travel wonders and the Seafaring 1/4 instead of 1/2 loss chance.
The Galley's sound converter exposed two sampler chunks whose declared sizes
omit a trailing byte. Following the WAV terminator and final sampler word
fixes that file; all 413 previously readable sound schedules stay unchanged.

Verified: 350 game tests and a build, including port routes, impassable land,
movement spending and next-turn continuation, coastal production, the 20/21
water-body boundary, sinking dice/RNG and owner isolation, and wonder
obsolescence. A rendered seed-1 run moved a Galley from `(12,4)` to `(15,4)`
with zero movement remaining. `/tmp/open4x-galley-sailing.png` was inspected
against `/tmp/open4x-galley-sailing.log`.

This is the naval foundation, not transport completion. Cargo capacity,
boarding, disembarking, cargo movement/death and the AI's naval production
and transport decisions remain required. Exact retreat movement, remaining
wonder effects and a full Ancient Age playthrough also remain outstanding.


### Ancient Age continuation: Galley cargo (landed; naval AI incomplete)

The generated PRTO roster now retains transport capacity. Land units board
a friendly ship by moving onto its water tile or using Load (L) in port.
The Galley's two slots are reserved as each move commits, so simultaneous
orders cannot overfill it. The carrier link follows native Unit +0x60; cargo
follows ship movement, clears independent orders, stays hidden from map
rendering and defensive combat, and does not receive independent AI orders.

Unload (L) opens a passenger choice in port. At sea, right-click the ship's
tile to select a passenger for a move to shore. A successful landing clears the link and spends all
remaining movement (`movement.md` 7, native mover stage 4). Destroying a ship
outside a city kills its cargo; destroying one in port releases the cargo,
while ordinary bystanders survive (`combat.md` 6.3, `0x5BC041..0x5BC09D`).
Unsafe-water sinking uses the same cargo cleanup.

Verified: 357 game tests and a build. Regressions cover capacity reservation,
foreign/full carrier refusals and preview routes, port loading/unloading,
sea passenger selection and visibility, movement following, landing movement,
cargo defense exclusion, and ship destruction/sinking. A rendered seed-1
sequence boarded a Settler and Worker at `(8,6)`, sailed to `(9,7)`, landed
the Settler at `(8,8)` with zero movement left, and founded Kyoto on the next
turn. The Worker remained aboard. Captures at frames 150 and 320 in
`/tmp/open4x-galley-cargo{}.png` were inspected against
`/tmp/open4x-galley-cargo.log`.

Remaining: AI naval production and transport planning and a choice dialog for
multiple eligible carriers.
Carrier selection currently uses a deterministic available ship. Full
Ancient Age parity, remaining wonder effects, exact retreat movement, and
a complete era playthrough remain outstanding.

### Ancient Age continuation: native transport commands and shore attacks

Recovered the friendly-carrier/capacity selector (`0x5C5F70`), manual Load
commit (`0x5C5110`), port-only Unload gate (`0x5C1C45`) and human passenger
choice (`0x5C5420`). Loading and same-tile unloading preserve movement.
Unload now opens the existing unit picker and detaches only the chosen
passenger, including one with no moves left. At sea, ordinary passenger
selection wakes the unit without detaching it.

The water-origin attack gate (`0x5B5DDC..0x5B5E22`) requires a land unit with
Amphibious ability, positive attack, and an unused attack unless it has Blitz.
Port cargo can attack normally. Successful advances detach cargo; amphibious
landings spend the remaining movement, and eligible attacks apply the native
amphibious combat modifier. Detailed evidence is in `movement.md` section 9.

Verified: 364 game tests and a build, including sea attack refusal, port capture,
amphibious city entry, and selecting one exhausted passenger for disembark.
A rendered seed-1 run boarded both passengers, landed the Settler and founded
Kyoto, then returned the Galley to Kyoto and opened its Disembark dialog for
the Worker. Captures at frames 150 and 380 in
`/tmp/open4x-native-landing{}.png` were inspected against
`/tmp/open4x-native-landing.log`.

### Ancient Age continuation: ship-to-shore disembark

A Galley ordered toward land now sails to the adjacent water tile and opens
Disembark without spending movement on the final shore attempt. One
passenger or Unload all sends cargo through ordinary movement and combat;
the ship stays offshore. Exhausted passengers remain aboard, cancellation
preserves the cargo, and the same All choice works for port unloading.
The AI mover dispatches cargo without opening a human dialog, though naval
production and voyage planning remain unfinished.

Evidence: native ship mover `0x5B91EC`, individual choice `0x5C5821`,
all-passenger/AI loop `0x5C5835..0x5C5924`, and the DISEMBARK script template.
Detailed findings and remaining terrain-helper questions are in `movement.md`
section 9.2. Multiple-carrier selection still uses a deterministic ship.

Verified: 365 game tests and a successful rendered build. An integrated
regression covers individual landing, Unload all, exhausted cargo and cancel.
A rendered seed-1 run chose Unload all, landed both Settler and Worker at
`(8,8)` with zero moves, left the Galley at `(9,7)` with all movement intact,
then founded Kyoto. Frames 200, 270 and 340 in
`/tmp/open4x-shore-dialog{}.png` were inspected against
`/tmp/open4x-shore-dialog-render.log`.
