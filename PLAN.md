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

### Full civilization and unit roster (landed)

The match is still four chairs, but they are now drawn from all 31
civilizations of `conquests.biq` instead of the four hard-coded ones.
`civ3_utils/biq/examples/gen_game_rules.rs` emits `CIV_ROSTER`, `LEADER_ROSTER` and
`RACE_ROSTER` (names, adjective, noun, ruler, ruler title, `diplomacy.txt`
text set, team color, badge color and the per-civ city list) alongside the
unit and building tables. `src/civs.rs` keeps `CIV_COUNT = 4` so every
`[_; CIV_COUNT]` array is unchanged, and exposes `CIVS`/`RACES`/`LEADERS`
as index tables over the roster, so `CIVS[slot]`, `RACES[slot]` and
`LEADERS[slot]` read as before. `CIV3_CIVS=Japan,Rome,Egypt,China` (or
`CIV3_PLAYER=Greece`) picks the chairs; the first name is the human. Above
the default four, selecting a civ changes its free advances, traits,
unique unit, city names, adjective and leader speech.

The unit roster now marks every `PRTO` row with art playable, not only the
land units and Galley: 87 units in 85 `Art/Units` folders, including the
Play the World unique units (their art lives in `civ3PTW/Art/Units`, now a
`prep_assets.py` source root) and the Conquests units. `Numidian Mercenary`
is the one unit with no art anywhere in the install and borrows the
Hoplite's. `tools/prep_assets.py units` converted the 85 folders for the 13
team ramps the roster uses. Abilities with no clone implementation (stealth,
nuclear strikes, air missions, army loading) are inert.

Verified: 417 game tests and a release build. Rendered runs of the default
match and of `CIV3_CIVS=Greece,Mongols,Carthage,Inca` (free advances and
`RACES` traits change), an autoplay run of 900 frames with `CIV3_AI_FAST`,
and a Greece-vs-Mongols contact showing "Genghis Khan Temujin of the
Mongols" with the Mongol `diplomacy.txt` greeting. The animated leaderheads
are still converted for the default four only; a civ chosen beyond them
greets without a portrait.

### Ancient Age continuation: culture, weariness, domination, ZOC, sites, transport choice (landed)

- Culture flip of cities (`src/flip.rs`) and war weariness (`src/weariness.rs`)
  run in the end-turn chain.
- Domination victory (`civs::check_domination`, `victory.md` section 16).
- Zones of control (`src/zoc.rs`, `movement.md` section 11; the rule is a
  HYPOTHESIS, the executable reader was not decoded).
- Fortress and colony worker jobs (`src/sites.rs`, Construction gate, era art
  via `tools/prep_assets.py improvements`; hotkeys Ctrl+F and B).
- Select-transport dialog for several carriers (`Diplomacy.board_ask`,
  `Screen::Transport`, `movement.md` section 12).

- Wonder audit: the Statue of Zeus and Knights Templar (improvement flag 30,
  `BLDG.unit_produced` / `unit_frequency`) hand out a Crusader or Ancient
  Cavalry every `frequency` turns (`citycalc::produced_units`, `happiness.md`
  section 10; the period is a HYPOTHESIS). The other Ancient wonders were
  already data-driven (`grant_*`, `doubles`, `wonder_flags`, `happy*`).
  `gen_game_rules` now emits `produces`/`frequency` and marks the produced
  units playable (their `races` are empty, so nobody can build them).

- Era art (`tools/prep_assets.py cities|units`): city sprites come from all
  five culture sheets by size (town, city, metro) and the owner's era, walls
  replace the whole sprite (`cities::CityArt`); Leaders and Armies show the
  Middle Ages, Industrial and Modern variants the install has
  (`units::UnitArt::set_era`, `sync_art_eras`). Other units have one art set in
  the install. Crusader and Ancient Cavalry now carry art for the wonders.
- Unit-producing wonders: `City.unit_clocks` and `citycalc::produced_units`.
- Computer sea war (`ai/naval.rs`): with a coastal city, an enemy city across
  the water and two spare soldiers, it builds a Galley, loads soldiers at a
  port and lands them on a beach beside the enemy city (land units cannot
  attack from ships, only Marines can).
- Save/load (`src/save.rs`): F5 writes `saves/quicksave.json` (or
  `CIV3_SAVE`), F8 reads it back; scripts use `key F5`/`key F8`. A save holds
  the tiles, cities, units (carriers by index), treasury, capital, flip
  ratings, research and diplomacy (word streams from `civ3mapgen::words`,
  `World::to_words`, `Relations::to_words`), realms, wonders, barbarian camps
  and both dice. A load replaces every city and unit with its sprites and is
  refused, changing nothing, when the file is for another map or does not fit.
  Not saved: trades on offer, questions waiting, what the computer planned
  this turn, other civs' explored map; terrain sprites a worker cleared stay
  cleared if an older save is loaded into the same session.

Not done: AI use of fortresses and colonies, Barricade and Outpost, era art
for units other than Leaders and Armies (not in the install).

Verified: 423 game tests and 595 reference tests; an autoplay run saved
at turn 6 and loaded at turn 17 replays the eleven turns identically (90
logged AI lines equal), so the dice and the books come back whole. A rendered load shows the city, units
and the "Game loaded" notice.

### Ancient Age continuation: reliable save restoration

Exploration history now lives in a saved resource rather than system-local
memory. Loading restores every viewer's discoveries, rolls back later
exploration, and rebuilds terrain sprites so previously cleared forests
reappear. Saves record the civilization roster and reject a different roster
or malformed exploration history. Research and diplomacy decode before any
world mutation; a rejected diplomatic stream leaves research unchanged.
Loading also refreshes research build locks and closes stale unit pickers.

Save format is now version 2; version 1 saves are unsupported. Verified:
427 game tests and a build. A rendered seed-1 run founded Kyoto, saved,
cleared forest at `(52,38)`, and loaded the earlier game. The forest, turn 1
and city returned; screenshots `/tmp/open4x-forest-verified-190.png` and
`/tmp/open4x-forest-verified-280.png` were inspected with the runtime log.

The overall Ancient Age goal remains open: a complete era playthrough and
the remaining combat, AI and feature fidelity audits still need evidence.

### Ancient Age continuation: scientific leaders and Science Ages

The reference acquisition step now rolls for scientific leaders before era
and target selection, including Philosophy's nested free advance, gated by
the native game flag and first-discoverer checks (`research.md` 10.2).
Gameplay enables the flag and spawns successful rewards at the current capital.
No capital means no unit, but still consumes the die. Scientific leaders carry
the native flag marker, cannot form an Army, can hurry production, and can be
consumed in a city by the Science Age command (A or native flask art, cell 19).

The age is active through start turn +20, changes the estimate/cost-clamp
research rate by the native truncated 1.25 multiplier, and leaves raw beaker
income unchanged (`research.md` 3.2, 3.3, 11). It expires after research at the
turn boundary. The computer prefers finishing an unfinished great wonder,
otherwise starts an age; this choice is HYPOTHESIS policy. Leader markers,
active ages and pending rewards are saved in format 3. Older saves are unsupported.
Loading closes old advisor panels and order modes, fixing a stale startup
Science Advisor that blocked commands after an otherwise successful load.

Verified: 431 game tests, 596 reference tests and a build. Regressions cover
roll chances/gates and Philosophy order, capital spawning, marker persistence,
Science Age availability, repeat-use refusal, expiry, unboosted income,
computer use and save restoration. A rendered save fixture showed the named
Scientific Leader and flask button, then consumed the leader with A; captures
`/tmp/open4x-science-final-95.png`, `135.png` and `200.png` were inspected. This fixture
checks presentation and command execution; the discovery/spawn path is covered
by the integrated regression, not the rendered fixture.

An unmodified seed-1 autoplay baseline reached turn 121, with seven Japanese
cities and discoveries through Mathematics (`/tmp/open4x-era-run.log`,
`/tmp/open4x-era-run.json`). This is early-game evidence, not a full era
playthrough. The full Ancient Age objective remains active.

### Ancient Age continuation: citizen specialists

Idle citizens can now cycle Entertainer, Tax Collector, Scientist by clicking
their city-screen portrait, in the shipped CTZN table order. Their native
outputs are 1 luxury, 2 taxes and 3 research, added outside commerce
multipliers and corruption (`yields.md` 5.5). Specialists remain productive
during Anarchy and disorder and do not count as ordinary unhappy citizens.
The Domestic Advisor shows each job's native portrait, cropped from
`popHeads.pcx` rows 16, 17 and 18.

Explicit jobs survive growth, population loss and save/load; assigning a
tile returns a specialist to work, and Governor resets assignments. The
computer uses spare entertainers for research or taxes while retaining those
needed for order; this selection is HYPOTHESIS policy. Jobs are saved in
format 4; older saves are unsupported.

Verified: 435 game tests and a build. Regressions cover job cycling,
population changes, returning to work, independent outputs under multipliers,
Anarchy and disorder, computer happiness protection, and save restoration.
A rendered seed-1 run cycled both jobs, saved a scientist, changed it back
to an entertainer and loaded. Inspected captures
`/tmp/open4x-specialists-155.png` and `360.png` show the tax collector's
2 extra gold and the restored scientist's 3 extra science, with native heads.
The full Ancient Age playthrough and remaining fidelity audit are outstanding.

### Ancient Age continuation: sustained play and era boundary

Civilian pathfinding now avoids foreign units and cities while searching,
instead of choosing a terrain-only path and stopping at its first blocker.
Settlers and Workers reselect a destination when its route becomes sealed;
Workers also search beyond the first three blocked job candidates. Artillery
can still approach an occupied target without stepping onto it. This fixes
China's repeated wait at `(27,38)` behind Roman units in the seed-1 match.

The second-civilization era uprising now runs at research acquisition,
independent of announcements. Previously computer research skipped it, and
hut rewards discarded the event. Both research and external acquisition now
request the native uprising even when their caller displays no message.

Verified: 439 game tests and a build. The movement regression runs the
computer and shared mover through a peaceful-unit detour to a city-founding
order; sealed-route cases cover replacement settlement and worker targets.
The era regression uses the native last-required-technology acquisition and
the game's uprising system: computer research and external awards each spawn
32 Horsemen from four camps (eight per camp), without an announcement.

A rendered, unboosted seed-1 autoplay match reached turn 140, then resumed
from that save with the routing fix. China grew from one city to three by
turn 167; Rome also expanded. Japan acquired Currency at turn 320, Feudalism
at 329 and Engineering at 340. The turn-344 save has era 1, all required
Ancient advances known, and Invention under research, with 27 Japanese cities
and 205 citizens (`/tmp/open4x-era-routing.json`, `.log`). Inspected captures
`/tmp/open4x-era-routing-3005.png` and `12000.png` show ongoing play. The
latest build loaded the turn-344 save and continued, with Invention visible
in `/tmp/open4x-era-boundary.png`. The long run used the routing build;
the subsequent uprising fix is verified by the integrated regression and build.

Era progression is now demonstrated, but full Ancient feature parity remains
open. Confirmed remaining worker actions include Outpost (Masonry) and
Barricade (Construction), `worker-jobs.md` section 1. Retreat placement and
Army combat still have documented deviations that need resolving.

### Ancient Age continuation: Outposts and Barricades

Workers can build Outposts after Masonry (Ctrl+O) and Barricades on Fortresses
after Construction (Ctrl+F). The action bar uses native cells 35 and 36.
Outpost completion consumes one Worker, releases the others, and keeps
structure sight after units leave: 3x3 flat, 5x5 hill, 7x7 mountain, following
the native creation footprints. Foreign entry or ownership removes it while
preserving explored history. Plain Colonies also provide structure sight.

Fortress / Barricade overlay bits are now independent of colony objects.
Their completion removes Outposts while preserving plain Colonies; Colony
founding can share a Fortress tile. Barricade construction retains the
Fortress bit, as the native job does. The defense routine tests Fortress
first (`combat.md` 4.2), so both bits yield its 50-point bonus; a lone
Barricade bit yields 100. Art is cropped from the Conquests terrain sheet.

Pillage and terrain bombardment now share the recovered destruction order
(`colonies.md` 8): downgrade Barricade to Fortress, then remove low terrain
improvements together, then remove Fortress / Outpost. Plain Colonies survive
these strikes. Save format 5 records the independent bits and new object;
older formats are unsupported. Scripted key combinations now support Ctrl
and Shift so captures exercise the actual hotkey dispatch.

Verified: 447 game tests and a build. Regressions cover construction gates,
pooled Worker consumption, persistent sight and destruction, native overlay
precedence, destruction classes and save restoration. Rendered hotseat Egypt
(whose native start grants Masonry) built an Outpost with Ctrl+O and saved /
loaded it; inspected `/tmp/open4x-outpost-180.png` and its save confirm the
native art, sight and consumed Worker. A fixture granted Construction and
placed the prerequisite Fortress, then Ctrl+F and end turns completed the
Barricade; `/tmp/open4x-barricade-185.png` and its restored save confirm both
bits, retained Worker and removed Outpost. This fixture checks execution and
presentation, not discovery of Construction.

The Ancient Age goal remains active. Worker labor timing and AI use of these
jobs, retreat placement and Army combat still need fidelity work; construction
times in this slice retain the clone's shortened scale.


### Ancient Age continuation: native worker labor

Replaced the shared shortened countdown with per-unit accumulated work
(`worker-jobs.md` 6). Generated TFRM labor costs and PRTO worker strength
come from `conquests.biq`. Required labor includes terrain movement cost,
independent of roads. Jungle clearing uses Clear Wetlands rather than Clear
Forest. The rate follows the native single-precision order: government,
Industrious, doubling advance, foreign nationality, PRTO strength, then
truncation with a minimum of one.

Only the outgoing owner's workers add labor. All matching work already on
the tile counts, including foreign workers; completion releases all of them.
Outpost consumes the actor whose contribution completes it. Founding a city
cancels a worker job without spending labor. Automation stays enabled when a
job finishes so the worker can choose its next task. The unit readout derives
remaining turns from the current pooled progress and owner's combined rate.
Captured Workers and both Workers produced from a captured Settler retain
their original nationality. Save format 6 preserves nationality and individual
progress; previous save formats are unsupported.

Verified: 449 automated tests pass and the game builds. Regressions cover
pooled foreign labor, terrain costs, Industrious and captured-worker rates,
Outpost's completing actor, city cancellation, capture nationality and
mid-job save roundtrip. In the rendered seed-1 hotseat run, an Egyptian Worker
builds a road at (52,38), a forest tile: after three owner turns the turn-4
save has progress 9 and no road (`/tmp/open4x-worker-progress.json`). Loading
that save and ending one owner turn completes the road; the resulting
turn-5 save and inspected `/tmp/open4x-worker-resume.png` retain it after
another load. The script uses actual R/F5/F8 dispatch with debug placement.

The Ancient Age goal remains active. Remaining worker deviations include
terrain eligibility, mountain access, forest-chop shields and AI use of
structure jobs. Retreat placement and Army combat also need the final
fidelity audit. This slice replaces the shortened timings noted above.


### Ancient Age continuation: terrain rules and forest harvesting

Normal TERR values now come from `conquests.biq`: yields, improvement
bonuses, labor movement costs and clearing jobs. Mines are available on bare
grassland, plains and tundra as well as desert, hills and mountains; forests
and jungles reject them. Tundra rejects irrigation. Roads accept mountains
at the terrain gate, though mountain path access still needs fixing. Clearing
requires the effective terrain's Clear Forest / Wetlands job and unowned or
own territory. All manual worker jobs reject city tiles. The AI now mines
bare inland land when irrigation is unavailable.

Removed the old MVP yield deviations: coast food 1, ocean food 0, tundra
shields 0, jungle shields 0, and pine uses Forest's 1 food / 2 shields.
Bonuses come from the effective terrain too, so a forced road on water adds
no commerce. Jungle's native movement cost is 3, including labor duration;
clearing jungle therefore needs 48 labor rather than 32. Terrain row lookup
is shared by tile yields, combat and worker gates.

Forest clearing pays the shipped RULE value of 10 shields once per tile.
The recipient is the first own city in native spiral indices 1..20 whose
current production can receive shields (`hurry::ordinary`); wonders, Palace
and Wealth are skipped. The box is capped at that city's current price.
Pine is Forest for this purpose; jungle gives no bonus. The pay-once bit is
spent even if no city receives shields and persists through save/load.
Worker completion runs before research and city production so the harvest
can finish a build in that turn. Save format 7 records the harvest bit;
previous formats are unsupported.

Verified: 456 full game tests pass and the game builds. New system-level
regressions cover recipient ordering, foreign/wonder exclusion, wrapping,
cost caps, re-clearing, no eligible city, jungle and terrain eligibility.
A seed-1 Egyptian hotseat fixture founded Thebes at (52,37), selected
Barracks, placed a Worker on the forest at (52,38), and issued C. After
three owner turns the tile is bare, its harvest bit is true and Thebes has
13 shields (10 harvested plus three normal production). Actual F5/F8
dispatch preserves it in `/tmp/open4x-forest-chop.json` (format 7, turn 4).
Inspected `/tmp/open4x-forest-chop-100.png` and `185.png` show the cleared
map, harvest notice and city production after restoration. Placement is a
debug fixture; clearing, turn processing and saves use the actual systems.

Remaining worker fidelity work: mountain movement and wheeled gates,
fresh-water irrigation and city propagation, AI clearing / structure job
choices. The broader Ancient Age completion audit remains outstanding.

### Mountain access and wheeled terrain gates

Land movement no longer treats all mountains as impassable. Mountains and
jungle use the shipped TERR cost of 3 MP. The generated table now includes
native impassable and impassable-to-wheeled bytes. The decoded land arm of
`0x5CCBB0` permits a restricted step only with roads at both ends; a
homogeneous Army inherits its shared member prototype's Wheeled ability,
while mixed Armies do not (`0x5BC6D0`). See movement.md section 13 and the
new address-annotated Rust reference module.

Player routes, AI military routes, movement, attack refusal and retreats
share this gate. Movement rechecks before issuing an attack so pillaging a
road invalidates a previously planned wheeled entry. Settler/Worker civilian
routes now reach mountains using the raw terrain cost; city founding still
rejects mountains.

Verified: 460 game tests pass, game build succeeds, native reference tests
pass (597 library, 22 integration and 2 doctests). Native release clippy
succeeds with existing warnings outside the new module. Regressions cover
mountain/jungle Worker access, Chariot/Catapult road exceptions, road removal,
Army composition, AI artillery detours, and both stale-route and direct
attack rejection.

Rendered seed-1 Egyptian hotseat fixture: Worker is debug-placed at (77,14)
and ordered to walk normally to mountain (78,14). Arrival spends its remaining
movement. After one full hotseat round, R starts the road; six owner job turns
complete it. `/tmp/open4x-mountain-road.json` is turn 8, terrain Mountain,
road true, worker at (78,14), job None. Inspected captures at frames 210 and
290 show the mountain worker, building notice and saved result. No fixture
code entered the game. Mountain mine eligibility was already covered by
the worker terrain tests; this rendered run exercised road completion.

Remaining: fresh-water irrigation and city propagation, AI clearing and
structure choices, full step costs (rivers and prototype abilities), native
ZOC verification, and the broader Ancient Age completion audit. Marsh and
Volcano terrain are not represented by the current map model.

### Freshwater lakes and irrigation through cities

Worker commands and AI job selection now use `worker-jobs.md` section 3.2:
water bodies of at most 20 tiles supply freshwater, existing farms supply
adjacent tiles, and one neighboring city can relay either source. Consecutive
city relays do not chain. This fixes the previous ocean-adjacency shortcut.
Native water continents use four edge neighbors, mapped to the game's square
grid orthogonal offsets. The same bounded flood fill now classifies shipyard
coastal eligibility, so corner-touching water bodies remain separate. No new
map state or save format is needed.

Regressions cover 20 versus 21 connected water tiles, horizontal wrap,
diagonal contacts, one-city relay, farm relay, two-city rejection, and matching
worker-command/AI choices. All 463 game tests pass, including 20 targeted worker tests; the game
build succeeds. Logs: `/tmp/open4x-freshwater-verified-tests.log` and
`/tmp/open4x-freshwater-build.log`.

Rendered seed-1 Egyptian hotseat fixture: a naturally generated lake lies at
(14,22); Thebes is founded at (15,22), and a Worker at (16,22) receives I.
The worker tile is outside the lake's direct freshwater neighborhood and
requires the city relay. Three owner turns complete irrigation. Actual F5/F8
saves and restores `/tmp/open4x-city-irrigation.json` (format 7, turn 4), with
Thebes at (15,22) and irrigation true at (16,22). The inspected frame-145
capture shows the restored farm beyond the city and lake. Only placement is
a debug fixture; founding, worker orders, turns and persistence use normal
systems.

Remaining freshwater work: river generation/rendering, river commerce and
movement effects, freshwater exemption from Aqueduct growth limits, and the
post-Ancient technology override. The recovered worker-progress validation
query is still undecoded, so source removal does not acquire a speculative
cancellation rule. AI clearing/structure choices and the broader Ancient Age
completion audit remain outstanding.

### Freshwater city growth and population joining

Decoded the complete growth gate `0x4B1DC0..0x4B1F8A`: freshwater exempts
the size-six Aqueduct requirement, but never the size-twelve Hospital-class
requirement. `rust/src/economy.rs::growth_limit` now captures both stock RULE
gates. Natural growth and Join City use the same rule with the map's
freshwater classification. Existing farms and city irrigation relays do not
qualify as freshwater for growth. The AI receives freshwater in its city
needs and chooses the level-2 or level-3 improvement matching its actual
blocked limit, avoiding an unnecessary Aqueduct at a lakeside town.

Verified: all 465 game tests pass; native release tests pass (598 library,
22 integration, 2 doctests); native release clippy succeeds with the existing
10 warnings outside this change; game build and diff whitespace checks pass.
New regressions exercise actual city turns from 6 to 7 beside a lake, blocked
12 without Hospital and growth to 13 with Hospital, population-join limits,
and AI Aqueduct/Hospital choice. Golden reference vectors include a dry city
with only a Hospital (still limited to six) and freshwater with Hospital.
Logs are `/tmp/open4x-lake-growth-{tests,native,clippy,build}.log`.

Rendered seed-1 Egyptian hotseat fixture founded Thebes at (15,22), adjacent
to the natural small lake at (14,22). Debug `size 6` sets the starting limit;
the normal J command joins a Worker, grows the city to seven and consumes the
worker. Actual F5/F8 saves and restores `/tmp/open4x-lake-growth.json` (v7),
which records size 7, no built improvements, and no civ-0 Worker. Inspected
frame 135 shows the restored city screen at POP 7 with Palace only. The
fixture's civil disorder is expected for seven citizens without happiness
support and was not changed by this growth-gate work.

Remaining: rivers and their freshwater, commerce, movement and combat effects;
Agricultural freshwater food behavior; AI clearing/structure decisions;
remaining native movement/ZOC audit; and the full Ancient Age completion
audit. This is verified progress, not a claim of complete Ancient Age parity.

### Playable rivers, freshwater, yields and combat

`src/rivers.rs` now adapts the recovered `rivergen` stage to the existing
square cylinder via a rotated native temporary grid and wrapped copies.
Native continent numbering and river-growth logic are reused; folding and
seam repair are explicit clone choices, so topology and density do not claim
exact native mapgen parity. Masks have reciprocal shared edges and native
diagonal continuity bits. A fresh disassembly of `0x5F0370` and `0x5EACC0`
confirms Cell +4 writes and Desert-to-Flood-Plain conversion, resolving the
contradictory old hills-only notes.

Rivers supply freshwater to irrigation and the Aqueduct exemption, +1 tile
commerce, native Flood Plain yields and mine eligibility, actual city river
eligibility (replacing an adjacent-mountain placeholder), and the native
+25 directional combat defense. Corner sprites use shipped mtnRivers and
coastal deltaRivers sheets; this art selection is a visually verified clone
choice rather than a decoded native selection rule. Prep clears green
exteriors. Save version 8 includes the river byte; earlier formats unsupported.

Verified: 467 full game tests pass. The additional combat-odds assertion
passes with the two targeted river tests. Shared edges, deterministic masks,
freshwater, floodplain food, commerce, wrapped river direction and persistence
are covered. Native release test/clippy results and game build are logged in
`/tmp/open4x-rivers-{native,clippy,final-build}.log`; clippy retains preexisting
warnings. Existing flat test fixtures explicitly clear rivers, preserving the
meaning of their dry-map cases.

Inspected `/tmp/open4x-rivers.png`: seed 1 shows continuous rivers across
terrain and into a coast. The generated v8 save has 587 river-marked tiles.
A normal seed-1 Egyptian game founds Thebes at the start (39,30), with river
eligibility true, and debug-places its Worker at (39,29). I starts irrigation
using the river on the tile. Three owner turns complete it. Actual F5/F8
saves and restores `/tmp/open4x-river-irrigation.json` (v8, turn 4), recording
mask 135 and irrigation true on (39,29). Inspected frame 145 shows the farm
and continuous river after load.

Remaining: river-crossing movement costs/Engineering bridges, Agricultural
freshwater food, floodplain/jungle disease and floodplain decoration, AI
clearing/structures, remaining native movement/ZOC work, and the complete
Ancient Age requirements audit. This is progress toward the full objective.


### River road crossings and Engineering bridges

Decoded the normal road branch in `0x580070`: both ends need roads, and
crossing the source river mask removes the discount until the moving civ
knows a TECH with Enables Bridges (stock Engineering). The fallback costs
destination terrain movement; it does not universally consume all movement.
Implemented the native subset in `civ3mapgen::movement::land_step_cost` and
shared it across movement, unit/AI routes, undefended city capture and route
previews. Removed the obsolete raw step-cost helper. Previews now use the
actual unit, domain, allowance and owner technology. Generated BRIDGES uses
TECH flags rather than a named-tech shortcut.

Verified 468 game tests, game build, 599 native library tests, 22 native
integration tests and 2 doctests. Release clippy retains 10 existing warnings.
Logs: `/tmp/open4x-bridges-{final-tests,final-build,native,clippy}.log`.
Regression covers detours, previews, actual ECS movement, civilization-specific
bridges and roaded mountain crossings. ECS regression runs on one thread
because test realms are thread-local.

Rendered Chariot crosses the same roaded river before/after Engineering:
remaining movement 3/5 thirds respectively, confirmed in v8 F5/F8 save/load
and inspected `/tmp/open4x-bridge-{before,after}-map.png`. After fixture grants
Engineering to native research slot 1, the Egyptian player; an initial fixture
incorrectly granted barbarian slot 0 and was corrected before acceptance.

Remaining toward the full goal: Agricultural freshwater food, floodplain and
jungle disease, floodplain decoration, AI clearing/structure choices, remaining
native movement and ZOC/treaty rules, and a complete Ancient Age requirements
and playthrough audit. The objective remains active.


### Agricultural freshwater and irrigation food

`citycalc::Rules` now keeps Agricultural city-center food at three under
Despotism/Anarchy when local freshwater exists. Other center yields remain
capped. Agricultural irrigated effective TERR Desert gains one food before
the cap; Flood Plain and covered/raised Desert do not get that bonus. These
are existing native `yields.rs` rules, rechecked against raw
`0x5D737A..0x5D73D0` and `0x5D7564..0x5D75AE` this turn. Corrected stale
center-exemption/growth claims in economy.md and the trim documentation.

Verified 469 game tests and build, 599 native library tests, 22 integration
tests and two doctests. Native release clippy retains ten existing warnings.
Logs `/tmp/open4x-agricultural-{tests,build,native,clippy}.log`. Regression
covers the owner trait, fresh/dry centers, lake 20/21 boundary, non-penalty
government, irrigated Desert versus Flood Plain and real city-turn storage.

Rendered Amsterdam at (39,30) under Despotism, identical worked tile
(38,29), with river masks present versus cleared. After an owner turn from
an empty food box, v8 saves hold two versus one food at turn 3. F5/F8 loads
both. Inspected `/tmp/open4x-agricultural-{fresh,dry}-city.png`: total food
four versus three, with three versus two center food icons. Debug fixtures
only alter the saved food box and river masks; the normal city-turn system
calculates production. Initial input fixture used mismatched civilizations
and was rejected by the loader; comparison was rerun from matching Dutch
saves. Actual research popup uses case-sensitive `adv Ok` in scripts.

Still pending: floodplain/jungle disease and floodplain decoration, AI
clearing/structures, remaining movement/treaties/ZOC, food-box difficulty
and accelerated-production rules, starvation destruction, and a complete
Ancient Age requirements/playthrough audit. The full objective is active.


### Lake yields, Seafaring commerce and the Colossus

Native `0x5D7470..0x5D748E` gives +1 food to water bodies of at most 20
tiles, independently of Harbors; larger bodies use Harbor food instead.
The clone now uses its existing bounded water-body traversal for worked
tile yields as well as freshwater/construction. Government caps follow the
bonus. Native `0x5D7EF4..0x5D7F55` adds Seafaring center commerce beside a
large water body after the capital floor, before other bonuses and the cap.
Native `0x5D7F82..0x5D7F9C` gives Colossus commerce to every producing tile,
including the center. Removed the previous Sea/Ocean-only restriction and
added the missing center effect. No commerce is created on zero-yield tiles.

Verified 471 game tests and game build; 599 native library tests, 22 native
integration tests and two doctests. Release clippy retains ten existing
warnings. Logs `/tmp/open4x-water-yields-{tests,build,native,clippy}.log`.
Regression checks lakes with/without Harbors, 20/21 threshold, Seafaring
ownership/capital/government order, land/Coast/center Colossus commerce,
zero-commerce tiles, and the following cap.

Rendered debug Monarchy Dutch city at (39,30) works Coast (39,31). A
20-tile body gives five total food, storing three from an empty box; a
21-tile body gives four food, storing two. Harbor on the larger body gives
five, storing three. Commerce is six beside the lake and seven beside the
larger body, showing the Seafaring bonus. Actual F5/F8 saves restore each.
Inspected `/tmp/open4x-water-{lake,ocean,harbor}.png` and corresponding v8
JSON saves, turn 4. Fixture terrain/topology and policy were supplied through
saves; production and storage came from the actual owner-turn system.

Colossus initially triggers the clone's existing Golden Age rule: eleven
commerce includes two extra Golden Age commerce, one on the center and one
on Coast. To isolate the wonder, a second fixture marks that age ended;
inspected `/tmp/open4x-water-colossus-no-age.png` shows nine commerce versus
seven without the wonder, with both center and Coast bonuses visible.
`/tmp/open4x-water-colossus-no-age.json` preserves the ended age and wonder.

Next fidelity work: city and unit terrain disease (`disease.md` / unit-turn
3.3, decoded but not integrated), Golden Age wonder trait coverage (current
one-matching-trait rule is explicitly a hypothesis), remaining AI clearing
and structures, movement/treaties/ZOC, food-box difficulty/acceleration and
starvation destruction, then the complete Ancient Age requirements audit
and playthrough. The full objective remains active.


### Terrain disease roll reference and population boundary

Added `civ3mapgen::disease` for the healthy infection loop and diseased-city
recovery roll, with native TERR order, strength arithmetic, low-word RNG
semantics, inclusive size comparisons, size-one early return and literal
Writing row 8 cure behavior. Raw disassembly checked at `0x4B45A0`,
`0x4B4640`, `0x4B4797` and the strength/roll tail. Five golden tests cover
D1-D8 and zero/negative strength-bound semantics. The caller owns citizen
removal and flag/cause/notification updates, matching the native calls.

Population integration needs `0x4BA230` cyclic occupied/free-slot selection,
its gameplay draw before recovery, nationality/work/specialist removal and
food reset on size-class change. Existing City aggregate size/foreign/work
state cannot reproduce those slots. This is the next implementation task,
not an external blocker. Gameplay disease, persistence and rendered evidence
remain unverified and unimplemented; no claim of completion is made.


Verification for this reference stage: 604 native library tests, 22
integration tests and two doctests pass; native release clippy returns to
ten preexisting warnings, with no disease-module warnings. Game cargo check
passes. Logs `/tmp/open4x-disease-{native-final,clippy-final,game-check}.log`.
The previously verified 471 game tests are not presented as disease gameplay
coverage. The full Ancient Age objective remains active.


### Citizen slots and population-loss food retention

Added native `population::Pool`: stable slots with holes, cyclic selection
from one gameplay draw per attempt, optional race filter, LIFO free-slot
reuse and no compaction of last. Citizen record fields retain race, work
index, job and resistance for caller effects. Rechecked raw selection
`0x4BA230`, release `0x4BA3F3..0x4BA411`, allocation call `0x4B9F98` and
free-head/last branches `0x4C2114..0x4C213E`. Five new reference tests cover
race/hole bias, failed and empty attempts, reuse order, class-change food,
and removal-before-disease-recovery draw state (seed 1 -> 2524885223).

`City::lose_population` now shares native class-change food retention across
current population-loss callers: forced labor, Settler completion, capture,
barbarian raids, bombardment and starvation. Same-class loss preserves even
a full store; class change empties it without an active Granary and caps it
at half the new box with one. Removed hurry's unconditional smaller-box cap
and Settler's incorrect clamp. Capture loss now precedes building/owner
transfer (native `0x5642F1` before takeCity), so retention sees the old
owner's Granary. X remains ten; AI/difficulty/acceleration remains pending.

Verified 472 full game tests, build, 609 native library tests, 22 integration
tests and two doctests. Native clippy retains ten preexisting warnings.
Logs `/tmp/open4x-population-game-{tests,build}-verified.log` and
`/tmp/open4x-population-{native,clippy}-final.log`. Regression exercises real
Settler completion across town/city class, Granary/no-Granary behavior and
same-class full-store retention. Updated the older Settler test's expectation
from the discarded clamp to native empty-store behavior.

Rendered identical debug Dutch size-seven Monarchy cities complete a
Settler and reach size five. Turn-5 v8 saves hold ten food with a Granary
and zero without. New player Settler exists in both; F5/F8 restores each.
Inspected `/tmp/open4x-population-{granary,no-granary}.png` and JSON saves.
Fixtures supply starting population/builds/food/policy, while actual city
turns perform completion, loss and storage.

Pending disease integration: migrate aggregate game nationality/work/job
population to persisted stable citizen slots, route growth/join/capture/
job assignment and all loss callers through that authority, then persist
disease state and run it at the native owner-turn position with generated
terrain inputs and notifications. The native pool is not yet used for game
victim selection. Native nationality-loss/resistance effects, unit jungle
disease, Golden Age trait hypothesis and the remaining Ancient Age audit
also remain. The full goal is active.


### Citizen pool snapshot validation before game migration

Current-state mutation audit finds 154 city-size references and 11 foreign
population references, plus separate worked/specialist assignment. Integrating
native slots requires replacing these aggregates rather than silently
rebuilding slot identity from their current totals. Before that migration,
added native Pool clone-stream snapshots that preserve holes, all citizen
fields and exact free-list reuse history. Atomic restore validates presence,
integer widths, free-list completeness/uniqueness/occupancy/bounds and exact
stream length; invalid data leaves the original pool untouched.

Three new regressions cover roundtrip victim choice and future births,
malformed snapshots, and save between removal and disease recovery with
matching RNG state and subsequent slot choices. An independent Python codec
agrees with Rust's golden 16-word vector (checksum 15) and rejection cases.
This is a clone stream, not native SAV decoding. No dependency was added.

Verified 612 native library tests, 22 integration tests and two doctests;
release clippy retains ten preexisting warnings, none in population. Game
cargo check passes. Logs `/tmp/open4x-population-snapshot-{native,clippy,
game-check,python}.log`. Game tests/rendered checks from the previous turn
are not presented as citizen-slot gameplay coverage.

Next: replace City size/foreign/work/job aggregates with persisted citizen
records; adapt founding/growth/join, capture and UI/governor assignment,
then use the native slot victim for all population-loss callers and integrate
owner-turn terrain disease. This turn supplies the validated persistence
boundary; City saves and gameplay still use their existing aggregate state.
The Ancient Age objective remains active and incomplete.

### Unit jungle disease gameplay (2026-10-04)

Integrated the verified `Unit::turn` jungle branch: a fortified unit with
zero PRTO population cost on effective Jungle terrain rolls rand(1000)
once at its owner's turn boundary and dies on zero. Carried units skip
both sea and jungle hazards. Native helper and gameplay use the shared
combat RNG; immunity consumes no draw. Deferred deaths cannot draw again
when scripted input sends multiple boundaries in one frame.

Verified 473 game tests, game build, 613 native library tests, 22 integration
tests and two doctests. Release clippy adds no disease warnings. The
updated targeted regression also passes after adding the multiple-boundary
case. Actual game F8/end/F5/F8 checks match independent seed-0 death and
seed-1 survival calculations, including exact saved RNG states and surviving
Settler/Worker/foreign Warrior. Evidence and limits are in `unit-turn.md` 10.

Next: the persistent citizen migration described above and city terrain
disease; Golden Age trait coverage and the full Ancient Age audit remain
pending. This completes unit jungle disease, not the whole Ancient Age
objective or every native unit-turn/death side effect.

### Persistent city citizens and native population victims (2026-10-04)

Replaced City's size/foreign/worked/specialist aggregates with the native
citizen pool as source of truth. Births and joins preserve race and LIFO
slot reuse; ownership changes preserve identity. Governor and UI edit
individual assignments. Every population-loss caller now draws a native
cyclic-slot victim and releases that citizen's job or tile. Barbarian raids
use owner-national count and race filtering. Food retention remains native.

Unit production now pays population cost for Workers as well as Settlers,
prefers owner nationals, then other players in slot order, and transfers the
last consumed foreign nationality to the unit. Growing cities wait while
size <= population cost. Native ABANDONBASE for a nongrowing city remains
pending; the clone currently holds production rather than consuming its
last citizen. The existing shield-overflow behavior is also still nonnative.

Save version 9 stores citizen slots/holes/free-list order instead of the
removed aggregates. Old saves remain unsupported. Verified 477 game tests,
game build, 613 native library tests, 22 integration tests and two doctests.
Clippy has the ten existing native library warnings. Actual F8/end/F5/F8
evidence selects a foreign scientist on starvation, preserves both workers
and the foreign entertainer, and restores the exact hole/RNG state. Golden
continuation also verifies later victims and birth ids. `hurry.md` 13 records
the evidence and limits; captures `/tmp/open4x-citizens-{90,270}.png`.

Next: City terrain disease can now use persistent victims. Nationality-loss
attitude counters, resistance lifecycle/foreign-at-war moods, abandonment
UI, production overflow, Golden Age trait coverage and the full Ancient Age
audit remain pending. The full objective remains active and incomplete.

### City terrain disease (2026-10-04)

Generated TERR disease flags/strength now drive healthy-city infection from
actual worked tiles and the center. Cities persist disease in save version
10. Owner turns remove a native random citizen before food/growth/production;
continuing disease removes before recovery, and population one survives.
Literal Writing row 8 suppresses new Flood Plain infections, matching the
executable. The city screen and turn notices expose losses and disease.

Verified 481 game tests and game build. Actual rendered F8/end/F5/F8 checks
match infection victim/RNG, persisted disease, following-turn scientist loss
and recovery, and Writing immunity with no draw. `disease.md` 12 records
evidence and limits. The native disease/population helpers were unchanged;
this stage's full regression run covers gameplay integration.

Next: nationality-loss attitude counters, resistance lifecycle/foreign-at-war
moods, native city abandonment, production overflow, Golden Age trait
coverage and the full Ancient Age audit remain pending. The objective
remains active and incomplete.

### Native shield completion and production switches (2026-10-04)

Corrected production stock: positive income clamps at current cost,
completion empties the box, and excess never funds the next queued/repeated
item. Wealth clears stored shields and advances its queue without building
the next item in the same turn. Removed the incorrect half-shield penalty
for changing unit/building classes; the shared UI setter keeps stock up to
the new item's price, matching the executable.

Verified 483 game tests and game build. Actual rendered/save-load checks
confirm a newly produced Warrior and queued Barracks at box 0, Wealth box
30 becoming queued Barracks box 0, and Change/Pick from Barracks box 30 to
Warrior box 10. `city-turn.md` 13 contains addresses, evidence and limits.

Next: abandonment and city removal, CONFIRMSWITCH warning/cancel, native AI
production reselection, nationality-loss attitudes, resistance lifecycle,
Golden Age trait coverage and full Ancient Age audit. The goal remains
active and incomplete.

### Confirm shield loss before changing production (2026-10-04)

City-screen and production-advisor picks now share CONFIRMSWITCH behavior.
Lossless changes happen immediately. A cheaper item warns with the exact
shield loss before committing; cancel/Escape preserves the city, Enter or
accept applies the native cost clamp. Ordinary city/turn input is blocked
while the question is open. Stale/foreign choices cannot commit, and loading
another game discards the old unanswered choice and modal.

Verified 487 game tests, game build, and the extended save round-trip test.
Actual rendered Change/Pick shows a 20-shield warning, ignores underlying
Governor/Space input, preserves the entire city/turn/RNG on cancel, and keeps
Warrior box 10 on accept and reload. `city-turn.md` 14 records evidence.

Next: ABANDONBASE and native city removal, AI production reselection,
nationality-loss attitudes, resistance lifecycle, Golden Age trait coverage
and the complete Ancient Age audit. The objective remains active.

### Replacement capital prerequisite for abandonment (2026-10-04)

Traced `0x4AECC0` city removal and decoded its replacement-capital helper
`0x4482B0`. Added native scoring and game replacement for a missing or
no-longer-owned capital: population + twice owner nationals + military
presence + town/city/metropolis weights across spiral indices 1..288.
Strict comparisons retain the first tie. A valid capital stays put;
replacement installs a Palace. Culture/display now avoid duplicating the
clone's implicit Palace when an actual Palace row is installed.

Verified 490 game tests, game build, 615 native library tests, 22 integration
tests and two doctests. Release clippy retains ten existing warnings. Actual
F8/F5/F8 evidence selects the three-person owner-national/garrison city over
a ten-person foreign-population city, persists the capital and Palace, and
renders one Palace with culture 1. `city-removal.md` records decoded sites,
opened removal observations and explicit gaps.

Next: finish removal's terrain/visual/wonder cleanup and wire ABANDONBASE
cancel/zoom/accept before population production destroys a city. Persisted
native city-pool tie ordering, AI reselection, nationality-loss attitudes,
resistance and Golden Age coverage remain in the broader Ancient Age audit.
The full objective remains active and incomplete.

### Resistance, executed corruption, trade network, Forbidden Palace (2026-10-04)

Captured citizens now resist (`city-turn.md` 8.4): `takeCity` seeding
`0x4BB090` marks every foreign citizen's pending nationality and rolls
the CULT initial chance plus the government modifier (fallback row
`[0x9C3D6C]` = the smallest-ratio row, executed in the emulator).
Resisters work no tile, eat nothing, cancel the size defense bonus and
block hurrying; the garrison quells them each turn with the continued
chance, peace ends resistance, and non-resisters assimilate by
`0x4AC140`. Citizens carry birth turn and pending race (save format 11).

Corruption and waste are now the whole of `0x4B1190`, decoded and
checked against 3 456 results of the real routine run in the emulator
over a varied shipped save (64 kept as golden vectors). It needs the
trade network, added in `src/trade.rs` (roads with the war rule, harbors
over coast; `trade-network.md`). The Forbidden Palace is playable: one
per civ, at least half the optimal city count, a second distance origin,
seven halvings in its city and a larger optimal city number. Great
wonders finished elsewhere switch the city's build (`WONDERCHANGE`,
`city-turn.md` 6.1); the computer builds wonders and the Forbidden
Palace (clone policy, the item chooser is open).

Verified: 505 game tests, 623 native library tests, game build, and an
unboosted autoplay to turn 59 without errors. Open: native city-pool
ids and the corruption/border tie words (`+0x358..+0x364`, seen non-zero
in saves), the birth draw `Random.next(2)` (`0x4ABD90`), police count
by `0x5A6060` mode 4.

Resources now reach cities through the road network (`trade-network.md`
7.1 step 1, per-city masks; `0x4ADE30` gates units and buildings per city;
luxuries count per city); AI workers road resource tiles first.

Next: resource trading, Palace relocation, celebration
(WLTKD) and the remaining Ancient Age audit.
