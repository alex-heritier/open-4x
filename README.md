# Civ3 Clone

A single-player Civ3-style game built with Rust Bevy, using art and audio
converted from a local Civilization 3 GOG install. You play Japan against
computer-controlled Rome, Egypt, and China. Nobody is at war until someone
declares it: found cities, explore, research advances, pop goody huts, disperse
barbarian camps, meet the rivals and trade or make treaties with them, work
resources, grow cities, build armies, declare war, fight, end turns. Conquer
all three rivals to win; lose your last city and unit and the game is lost. Map-feature placement follows the
reverse-engineered mapgen stages (see `reverse-engineering/NOTES.md`).

Each civilization starts with its own Settler, Worker, Warrior, and Scout on
separate land tiles. Japan starts at the map's best site; Rome, Egypt, and
China start within 10 tiles of it, on open grassland or plains it can walk to,
spread around it (`civs::starting_positions`). Units, cities, capitals, gold,
resources, and explored terrain belong to their civilization. City badges and
borders use its color.
End Turn advances only that civilization's cities and worker jobs, then passes
control to the next civilization; the computer civs play their turns on their
own (see "Computer opponents") and the camera stays on your view. The turn number increases
after all four have played. Only the active civilization's units and cities
can receive orders; production decisions wait for their owner's next turn.

## Setup

Prereqs: Rust, Python 3 with PIL, ffmpeg.

1. Point prep at the GOG install (default `civ3/civ3-gog/app`):
   `export CIV3_GOG=$PWD/civ3/civ3-gog/app`
2. Convert assets once (writes gitignored `assets/gen/`):
   `python3 tools/prep_assets.py`
3. Run: `cargo run`

`CIV3_HOTSEAT=1` makes all four civs human again (the old hotseat mode),
`CIV3_AUTOPLAY=1` hands all four to the computer to watch, `CIV3_AI_FAST=1`
skips the computer's animations and `CIV3_AI_LOG=1` prints what it decides
each turn. `CIV3_SCRIPT` `end` is meant for hotseat: use it with
`CIV3_HOTSEAT=1`.

`MAP_SEED` overrides the fixed default map seed; `MAP_CENTER=x,y` and
`MAP_ZOOM=<0.35..2.5>` frame the starting camera, `CIV3_REVEAL=1` starts with
fog off and `CIV3_NO_SPLASH=1` skips the greeting.

For testing and debugging, `CIV3_SHOT=out.png CIV3_SHOT_FRAME=120 cargo run`
writes the window to `out.png` and exits; `CIV3_SHOT_FRAME=30,120` (or `{}` in
the path) grabs several frames, and `CIV3_SHOT_KEEP=1` leaves the game open.
`CIV3_SCRIPT='20:key B;60:city;90:btn Change'` drives the game unattended
(keys, turns, unit placement, city-screen buttons and tiles); the action list
is in `src/script.rs`.
See `.agents/skills/game-screenshotting/SKILL.md` for the wrapper script and
the desktop-capture fallback.

## Controls

- Click: select unit, order move, open city; the selected unit wears
  Civ3's white selection ring. Left-click selects the displayed stack unit
  without cycling. Right-click a unit or stack: open the unit picker.
  Hold the left button on
  a tile and the route appears after a moment — the same path line and
  destination marker the Go-to button shows (G arms it, ESC cancels) —
  and releasing orders the move. A plain hover shows no route, as in
  Civ3.
- W/A/S/D: pan the camera; wheel: zoom. The left button never pans, as in
  Civ3, so a press and drag is only ever a move.
- Unit action buttons float over the bottom of the map, as in Civ3; hovering one names the command
  and its key, unavailable ones are darkened.
- Worker: R road, I irrigate, M mine, C clear forest/jungle. Road to road
  costs 1/3 MP; a mine replaces irrigation and vice versa; workers sharing a
  tile and job pool their turns. Z automates a Worker: it picks its own jobs
  with the computer's worker brain until any manual order (a click, a move,
  Wake) stops it.
- U: upgrade the selected unit for gold (it must stand in a city with the
  facility for its domain, a Barracks for land units, and have movement left);
  Shift+U upgrades every unit of its type, paying unit by unit. The button
  shows the successor and the price on hover, and only appears in such a city.
  J: a Settler or Worker joins the city it stands in (2 and 1 citizens, up to
  the size the city's Aqueduct and Hospital allow). Shift+P: a soldier pillages
  the mine, else the irrigation, else the road of its tile (outside cities,
  on ground nobody owns or a civ at war with you owns).
- Arrow keys: step. Tab: cycle units that need orders. F: fortify. Space: skip.
  Units with no moves left cannot be selected; when none need orders the
  selection clears and the bottom-right box blinks its next-turn prompt.
- Map Making unlocks Galleys in cities bordering water bodies larger than
  20 tiles. They sail water and enter friendly ports. Sea and Ocean tiles
  can sink an unprotected Galley and its cargo when its turn ends. A Galley
  carries two land units: move onto a friendly ship to board, or press L
  while sharing its port tile. L on a ship in port opens a passenger choice
  and unloads the selected unit without spending movement. Order a Galley
  toward shore to choose a passenger or Unload all; the ship stays offshore.
  Alternatively, right-click its tile to select and move one passenger.
  Landing spends the passenger's remaining movement. Attacking from water
  requires Amphibious ability; cargo in port can attack normally. Naval AI
  and the multiple-carrier boarding choice remain unfinished.
- B: found a city with a Settler, or arm bombardment with a Catapult.
  Click a visible enemy target in range to fire; Esc cancels. Catapults
  require war, spend one move, and leave military units at at least 1 HP.
  City bombardment can destroy Walls, ordinary buildings, or population.
  When no unit or city qualifies, a terrain hit destroys roads, mines and
  irrigation together. Improvements in peaceful or own territory are refused.
  A supporting Catapult or Archer also fires automatically before melee
  against its stack, once between its civilization's turns.
  Enter, the next-turn disc, or the
  bottom-right box (when it shows the prompt): end the active civilization's
  turn and pass control to Japan, Rome, Egypt, or China in that order.
- City screen: click tiles to assign workers (yields show as Civ3's food
  and shield icons; roads, irrigation and mines are drawn; tiles another
  city works, or another civilization's border covers, are dimmed), Change
  build (Build now / Queue), Governor, X or ESC closes. Idle citizens show
  as entertainers.
- F1: Domestic Advisor (the tax, luxury and science rates, each city's mood
  and surplus, and Revolution to a government you know). F6: Science Advisor (choose what to research). F4: Foreign Advisor (who you
  have met, their attitude, and Talk). F7: Wonders of the World (every great
  wonder built, who owns it and where, and the ones under construction; the eye
  button looks at the city).
- A leader greets you the first time you meet, and Talk, the trade table and the
  computer's proposals show their animated leaderhead and speak in their own
  words (Civ3's `diplomacy.txt`). Completing a wonder brings up its splash.
- P: save a window screenshot as `shot-<unix>.png`.
- F9: reveal-all debug toggle.

## Economy

Each citizen works one tile around its city (the governor picks the best, or
click tiles on the city screen); the city center works for free and yields
what the executable gives it (`reverse-engineering/yields.md`: 2 food, shields
and commerce by size class, more for an Industrious or Commercial civ). A
city grows when its food box fills: 20 food while it is a town (size 1 to 6),
40 as a city (7 to 12), 60 beyond; it cannot grow past 6 without an Aqueduct
or past 12 without a Hospital; a Granary keeps half the box on growth, and a
city short of food loses a citizen.

Every civ starts in Despotism and changes government (Anarchy for a few turns,
then Monarchy, Republic, ...) once it knows the advance: the government sets
the tile penalty, the trade bonus, the corruption of the commerce far from the
Palace, unit support and upkeep, martial law and war weariness
(`reverse-engineering/government.md`). The Domestic Advisor (F1) sets the
tax/luxury/science rates and starts a revolution. Citizens are content or
unhappy by the executable's rules (`happiness.md`): a city with more unhappy
than happy citizens riots, produces nothing and may lose a building to the mob;
Temples, Marketplaces, luxuries and soldiers under martial law calm it.

Tax goes into its civilization's treasury when that civilization ends its turn.
Improvements cost their upkeep, and units cost gold each beyond the free allowance
the government and the cities' sizes give. Upkeep the treasury cannot
cover sells one improvement for its shield cost over four in gold; unit support
it cannot cover disbands the cheapest unit. A civ's traits matter: a Religious
civ builds Temples at half cost and endures half the anarchy, a Militaristic
civ promotes its victors twice as often and halves its Barracks, and so on
(`rules_data::RACES`). The bottom-right box shows the active civilization's
`N Gold (+M per turn)`, in red when it is losing gold.

The rules are the executable's where `reverse-engineering/economy.md` could
read them (the food box, the Granary, the upkeep and unit-support formulas,
one sale or one disbanding per turn). Where it could not, or where the clone
simplifies, the notes say so: the sale price and which unit goes are marked
**HYPOTHESIS**, and the executable rolls a die for which bill comes first
where the clone always pays upkeep first. `src/economy.rs` and
`src/citycalc.rs` hold the rules as plain functions with tests; the
corruption by distance, the computer's tax rates and the choice of the human's
next government are marked **HYPOTHESIS** where the executable read stops.

## Research

Every civilization has the 83 advances of the Conquests rules, with their
prerequisites and costs (`src/rules_data.rs`, generated from `conquests.biq`
by `biq/examples/gen_game_rules.rs`). A civ's cities add their science (the
fixed 50% of commerce) to its beakers at the start of its turn; when the
beakers pay the advance's cost the advance arrives and the civ picks the next
one. The cost follows the executable (`reverse-engineering/research.md`): the
advance's rule cost scaled by the world size and the difficulty, cheaper for
each civ you have met that already knows it, and held between 4 and 50 turns at
the current rate. Changing the target resets the beakers.

The human is asked what to research when the last advance arrives and in the
Science Advisor (F6); the bottom-right box shows the target and its beakers
against the cost. The computer picks with the executable's valuation
(`reverse-engineering/research-ai.md`). Goody huts can give an advance to a
civ still in the ancient era.

The whole unit and building roster of `conquests.biq` is available
(`src/rules_data.rs`, `src/roster.rs`), gated by advance, strategic resource,
race and wonder, and a unit with a buildable successor is obsolete: the city
builds the upgrade instead (`reverse-engineering/buildable.md`). Not modelled:
the scientific-leader roll and the remaining wonder effects (the Great
Library, Theory of Evolution).

## Diplomacy

No two civilizations are at war at the start, and a war begins only when one
declares it. A civ meets another when its soldiers see a foreign unit or stand
at its border, and the two swap embassies at once (the Foreign Advisor, F4,
lists everyone met with their attitude, treaties and the war state, and opens
Talk). Ordering a soldier to attack a civ you are at peace with asks first
whether to declare war.

Talk is a trade table: peace, right of passage, mutual protection, military
alliance and embargo against a third civ, contact with a third civ, gold, gold
per turn for 20 turns, and advances. The computer answers with the
executable's four verdicts (accept, almost, not interested, no). It asks five
times the price of anything after you broke a deal with it (the executable's
`4T + 1`). Declaring war calls in every ally and every mutual-protection
partner, as in the executable (`reverse-engineering/diplomacy.md`). A computer
civ's attitude toward you is the executable's score (its personality, what you
did to it, what you did to others, governments, treaties and so on), and its
decision to go to war is the executable's roll. The computer proposes advance
swaps now and then.

The diplomacy screens are Civ3's own art: the leader's animated portrait
(`Art/Flics`, one clip per era, played back and forth) sits in the frame, and
what the leader says comes from `diplomacy.txt` by the block for the occasion
(first contact, a greeting, a proposal, a verdict) and by the leader's strength
and mood. The clone stands in the tone and which block a deal calls for.

Clone-level choices, because the executable's drivers are not decoded
(`diplomacy.md` section 12): soldiers standing in a civ's borders build up
pressure that provokes it, the computer plans a war every few turns from turn
15, the prices of anything but an advance are stand-ins, and the computer
never offers peace (nothing in the executable lowers its war memory).
World maps and cities cannot be traded.

## Combat

Move a soldier into an enemy unit or city (click, Go-to, arrow keys) and it
attacks on the last step. A fight is played out round by round: both units
face each other and both keep swinging their attack clips for the whole fight,
whoever wins the round, the blow lands partway through the round, the vertical bar
beside each unit (green, yellow at two-thirds or less, red at one-third or
less) drops, and at the end the loser plays its death clip and the winner its victory
clip. Input waits until the
sequence ends. `CIV3_COMBAT_SPEED=<n>` plays it `n` times faster, for
unattended runs.

From the executable (`reverse-engineering/combat.md`, consumed through
`civ3mapgen::combat`): the round die and odds `defense * (100 + D)` against
`attack * (100 + P)` with terrain, city size and walls, and fortify terms,
clamped to `1..=1023` of 1024; one hit point per lost round; the `EXPR` hit
points per level (2, 3, 4, 5, plus the unit's bonus); the dice are the same
LCG as the map generator. A test pins the round loop to the reverse-engineered
`duel`.

A victor promotes on the executable's die (`combat.md` 6.2): one in 2, 4 or 8
for a Conscript, Regular or Veteran, twice as often for a Militaristic civ; a
unit that failed a roll this turn promotes on its next win. A city's Walls
(a town) and Civil Defense add their percentage to the defense, the best one
counting.

Units upgrade for gold (`reverse-engineering/unit-upgrades.md`, `src/upgrades.rs`):
the price is the rule's 3 gold per shield of difference to the furthest
successor the civ can build, halved by Leonardo's Workshop; the new unit keeps
its tile, its fortified order and at most a Veteran's rank, and starts healed
and out of moves. The computer spends spare gold on upgrades in its cities.

Marked **HYPOTHESIS** in `src/combat.rs`, because the executable read does not
cover them yet: healing (1
hit point a turn in the field, 2 in a city, full with a Barracks, only after a
turn of rest), which defender of a stack fights (the likeliest to win), which
clip plays in a round, when in the clip the blow lands, the details of
capturing a city (one citizen lost, queue and Palace lost, no plunder, no
raze), and that a stack dies with its defender outside cities. Units never
retreat. Only units with attack can attack, one attack per unit per turn;
workers and settlers are captured (a Settler becomes two Workers).

Barracks cities build Veteran soldiers. The action bar shows a soldier's rank
and hit points. Scripted captures can set up a fight with
`spawn Warrior 1 @1,1;sel Warrior;go @1,1` and read the result with `report`.

## Computer opponents

The computer (`src/ai.rs`) plays Rome, Egypt, and China by simple rules, with
the same units, costs, combat odds, and upkeep as you. It sees where units
are, but remembers only the ground it has explored. Each turn it:

- builds a garrison first, then Settlers while there is room, Workers (about
  one per city), soldiers (the best defender or attacker of the roster it can
  build, by its `PRTO` strategy bits), and Temples, Granaries, Barracks,
  Libraries, Marketplaces, Harbors and what lifts a city's size limit when its
  treasury can carry the upkeep, with Wealth when nothing is worth adding;
- picks its government by the executable's score, sets its rates, and spends
  spare gold upgrading units that stand in a city with a Barracks;
- settles the best open sites away from other cities and foreign borders;
- has Workers road, irrigate, and mine around its cities;
- sends Scouts to unexplored ground;
- researches by the executable's valuation and builds only what its advances
  allow;
- keeps defenders in cities, attacks civs it is at war with when the odds are
  fair, and marches a large army on the nearest enemy city, taking it when it
  is empty. It goes to war by the rules under "Diplomacy".

A civ with no cities and no units is eliminated after a short grace period.
Eliminating every rival is a victory; being eliminated is a defeat.
Deviations from Civ3: one fixed difficulty, no naval play, and the computer
plays at one fixed skill.

## Scope

A single-player game against three computer civilizations, terrain, movement, melee combat with
Civ3's odds and animations, private fog, settling, food and
shield boxes, Warrior/Settler/Worker production, Tokugawa splash, UI and
unit sounds, the Asian peace music loop, plus goody huts (poppable for
units, maps, or settlers), capturable barbarian camps, and 22 placed
resources with bonus yields, worker improvements (roads, irrigation, mines,
clearing), city production of units and buildings with a queue, and cultural
borders that start as the 3x3 square, grow to the 21-tile city radius at 10 culture and to 37 tiles at 100 (measured from the game's own saves), and merge between cities of one civ. Governments, happiness, research, diplomacy, gold upgrades and promotions are
in. Out of scope: smarter AI, trade of cities and maps, ranged and naval
combat, retreat, railroads, fortresses, pollution, save/load, minimap.
