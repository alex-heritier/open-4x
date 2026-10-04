# Civ3 Conquests reverse engineering: map generation, resources, rivers, graphics, AI

Descriptive (not normative) documentation plus a dependency-free Rust
reference implementation, recovered by static analysis of
`civ3/civ3-gog/app/Conquests/Civ3Conquests.exe` (PE32, MSVC 6.0, 3 417 464
bytes). Each system owns exactly one findings file and one Rust module;
`NOTES.md` remains the map-generation source of truth and everything else
cross-references it instead of duplicating it.

## Contents

| path | owns |
|---|---|
| [`NOTES.md`](NOTES.md) | Map generation: 12-stage pipeline, fractal, `Cell` layout, PRNGs, tile record, `.biq` codec, open questions |
| [`resources.md`](resources.md) | Resource placement stage `0x5f22a0` (`GOOD`/`TERR` data path) → `rust/src/resources.rs` |
| [`rivers.md`](rivers.md) | River system (not a mapgen stage) and river art path → `rust/src/rivers.rs` |
| [`graphics-terrain.md`](graphics-terrain.md) | Terrain art table, sprite inventory, sheet geometry, cultural border sheet, map-view renderer class → `rust/src/graphics.rs` |
| [`blending.md`](blending.md) | Terrain display and blending: sprite addressing, neighbor masks, painter order, context variants → `rust/src/blend.rs` |
| [`graphics-units.md`](graphics-units.md) | Unit `.ini` slot universe, FLC format sample → `rust/src/graphics.rs` |
| [`graphics-city.md`](graphics-city.md) | City-view backgrounds, screen chrome, map-view cities → `rust/src/graphics.rs` |
| [`ai.md`](ai.md) | MSVC `rand`/`srand` pair, caller census, action gate, score jitter, turn-loop root → `rust/src/ai.rs` |
| [`combat.md`](combat.md) | Combat resolution: odds and every percentage term, rounds, retreat, choosing the defender, ranged attacks / city strikes / defensive bombard, victory bookkeeping, ordinary stack survival and cargo-only kill recursion, the barbarian raid, the diplomatic gates in front of an attack, the gameplay `Random` instance and its seeding → `rust/src/combat.rs`, `rust/src/rng.rs` |
| [`biq.md`](biq.md) | `.biq` container framing and the PKWARE DCL codec (decoder, both literal modes; the `implode` compressor ported from the exe and checked against every shipped compressed file) → `rust/src/dcl.rs`, `../biq/src/dcl.rs`, `../biq/src/implode.rs` |
| [`biq-format.md`](biq-format.md) | Section-level scenario format: all 28 sections, versions and row lengths, cross-references, `GAME` settings, open items → `../biq/` (standalone crate, 229 tests) |
| [`savegame.md`](savegame.md) | Saved games (`.SAV`, magic `CIV3`): header and sub-version gates, the chunk and raw-block primitives, the whole stream grammar of format 24 (embedded rules, `GAME`, map, 32 players, units, cities, turn history, replay log, network queue), field maps for the decoded objects, the rule counts that size the raw arrays; every claim checked by running the exe's own loader in an emulator over 22 real saves and 8 synthetic sub-version layouts → `../biq/src/sav/` (`Save`: byte-exact round trip, DCL included), `tools/emu/` |
| [`editor.md`](editor.md) | Conquests scenario editor image: tag census, `3D` dispatch map, save-path mirror (open) |
| [`economy.md`](economy.md) | Culture/corruption/trade economy, city growth (food box, Granary), gold (unit support, upkeep, treasury, forced sales): border event, culture/corruption/split math, the optimal city number, growth and payment rules, improvement cost and trait discounts → `rust/src/economy.rs` |
| [`government.md`](government.md) | Governments: the GOVT record, adoption (`setGovernment`, revolution, the anarchy countdown, the AI's choice), war weariness and the Democracy collapse, mobilization, peace and the declaration of war with the calls to arms → `rust/src/government.rs` |
| [`yields.md`](yields.md) | Tile yields (food, shields, commerce: centre, water, resources, Golden Age, the Despotism cap) and the city's per-turn totals (food eaten, the shield multiplier, the commerce split, Wealth, tourists, specialists) → `rust/src/yields.rs`, `rust/src/city.rs` |
| [`happiness.md`](happiness.md) | Happiness: citizen moods and the recompute `0x4BCFF0` (base mood, buildings, martial law, luxury, draft, war, foreign nationals, the face distribution), civil disorder and the riot roll, We Love the King Day, the per-turn order, the reason bytes; verified against the real code by differential test → `rust/src/happiness.rs` |
| [`buildable.md`](buildable.md) | What a player may build: `canBuildImprovement` / `canBuildUnit` and the free-building set → `rust/src/buildable.rs` |
| [`stacking.md`](stacking.md) | Per-tile unit stacks: the `Cell+0x0C` list, the pool at `0xA52DD4`, placement order, consumers → `rust/src/stack.rs` |
| [`workers.md`](workers.md) | Unit orders/worker automation: string-negative verdict, goto enumerator, struct head (the job rules are in [`worker-jobs.md`](worker-jobs.md); automation itself is still open) |
| [`dynamic-tracing.md`](dynamic-tracing.md) | Live-debugging runbook (Wine/winedbg); static analysis is exhausted for its questions |
| [`ui.md`](ui.md) | UI text: civilopedia hypertext tokenizer → `rust/src/ui.rs` |
| [`research.md`](research.md) | Research: beakers and the income pass, the cost of an advance (base cost, the turn clamp, the rate), the research step, `acquire` in order (eras, the scientific leader, Philosophy, the Great Library, the Science Age), the queue, the chooser dialogs, the default pick, goody-hut and stolen advances, the draws in order → `rust/src/research.rs` |
| [`research-ai.md`](research-ai.md) | How the AI values and picks advances: the valuation `0x448BF0` (flags, rule tables, the cost or urgency term, the flavor overlap, the contact tails), the category mask, the default and steal picks, the price of an advance in a deal → `rust/src/research_ai.rs` |
| [`diplomacy.md`](diplomacy.md) | Diplomacy: the per-pair state of a `Player` (at-war, embassy, relation word, treaty, allies, embargo, war memory, pair record), first contact, `declareWar` with the alliance and pact call-in, `makePeace`, the attitude score and class, `wantsWar`, the AI's peace gates, the deal item types and executor, the deal scorer and its verdict ladder, packages, the tech-trade counter, the diplomacy and espionage dialogs → `rust/src/diplomacy.rs` |
| [`multiplayer.md`](multiplayer.md) | Multiplayer: mode global, net gates → `rust/src/net.rs` |
| [`air.md`](air.md) | Air combat: SAM / flak / patrol-interceptor defense layers, interception order, air-move dispatch, landing and bombing runs → `rust/src/air.rs` |
| [`capture.md`](capture.md) | City capture and transfer: the capture routine and its three modes, gold plunder, population loss, keep or raze, the culture conversion, the AI's raze and accept decisions (Player vtable `+0x18` / `+0x14`), the transfer (buildings destroyed, counters, units on the tile), razing; the RACE trait mask → `rust/src/capture.rs` |
| [`victory.md`](victory.md) | Victory, defeat and score: the per-turn score and its running means, every victory-point source, `CheckVictory` and the eight victory types in evaluation order (ties, teams), the diplomatic vote and the AI's side, the time limit and Retire, the final announcement and early bonus, civilization elimination and respawn, call-site and round-end order. Clean-room specification, no Rust module |
| [`turn.md`](turn.md) | The round and `Player::turn`: the round processor `0x4F5EF0` (loop A units and AI planners, loop B per-player routine, world phases, turn counter), the human turn driver, the per-player step order, first contact, interest, upkeep, the network round `0x476330`. Clean-room specification, no Rust module |
| [`world-events.md`](world-events.md) | World events: resource upkeep (disappearance and relocation, every 5th turn), pollution and meltdown, nuclear counter, global warming, volcanoes, plague (scheduler and per-city model), the cell slot table, the calendar and the colony pools. Clean-room specification, no Rust module |
| [`barbarians.md`](barbarians.md) | Barbarians: the slot-0 branch of `Player::turn` (gates, caps, spawn draws), camp founding and site rules, tribe names, the uprising on era entry, game-start garrisons, camp destruction and its 25-gold reward. Behaviour of barbarian units is open. Clean-room specification, no Rust module |
| [`unit-turn.md`](unit-turn.md) | The per-unit turn `0x5C7700` and unit death: ship sinking and jungle disease (chances, immunities), the healing rule (eligibility, amounts per situation, army variants), auto-wake, movement reset, and `Unit::kill` `0x5BBBC0` (cargo, counters, score, pool free list, elimination check). Clean-room specification, no Rust module |
| [`goody-huts.md`](goody-huts.md) | Goody huts: the two triggers (unit arrival, tile-owner change), the outcome roll with its seven threshold tables and golden grids, all eight outcomes (gold, map, free city, nothing, settlers, mercenaries, advance, barbarian band) with preconditions, re-roll loop, RNG draw order and helper routines (tile occupant resolver, hut unit chooser, tribe pick, city-creation gates). Clean-room specification, no Rust module |
| [`city-founding.md`](city-founding.md) | City founding: `createCity` (pool id allocation, tile clearing, counters, worked-tile hand-over), the 18 steps of `City::init` with initial field values, the automatic city-name generator (rounds, prefix, easter eggs, collision rule, counter), the AI start bonus (best defender / attacker choosers, DIFF unit counts), the AI unit cost function, golden name vectors. Clean-room specification, no Rust module |
| [`city-turn.md`](city-turn.md) | The per-city turn sequencer `0x4BE970` (15 steps), the production system: shield box and construction-bonus pool, building and unit cost functions with every discount, the production tick `0x4B9950`, building completion and "wonder built elsewhere" `0x4B9270`, unit completion and population cost `0x4B8BA0`, coinage, change-of-production bookkeeping and the confirm dialog, the build queue, `chooseProduction`, citizen nationality drift and resistance quelling, golden vectors. Clean-room specification, no Rust module |
| [`hurry.md`](hurry.md) | Hurrying production: the command `0x4B6010`, the validator `0x4B5290` (ordered refusals), gold price (linear, doubled for an empty box) and forced-labor people count, the executor `0x4B5CA0` (box filled, treasury cell rewrite, hurry-sacrifice timer), the AI price handicap `0x4B50A0` with its jump table, and the citizen removal routine `0x4BA230` (victim selection, tile release, size-class food clamp, recompute). Clean-room specification, no Rust module |
| [`city-buildings.md`](city-buildings.md) | Adding and removing a building, `0x4ACF40`: the ordered add path (spaceship part, Palace move and capital registration, bitmaps and the 12-byte builder records, counters, great-wonder registry and announcement, small-wonder holder, replace-by-flag sale with its treasury and Recycling refund, wonder notice, victory points, two free advances), the remove path, the common tail (trade network, reach set, corruption, water yields, food eaten, culture cache), the per-building culture `0x4F8CE0`, the obsolescence test `0x4ACCC0`, the 21 callers, golden vectors |
| [`trade-network.md`](trade-network.md) | The trade network: the cell-level road predicate and war rule, the label fill (`0x580540` mode 4), the city connection matrix builder `0x57D980` (road, air and water passes), the pair closure `0x57D840`, the incremental road update `0x57DEF0`, the entry points and their triggers, the resource-availability rebuild `0x57E450` / `0x57EDD0` with the supply records `Player +0x1614`, the resource-deal writers, golden vectors |
| [`unit-upgrades.md`](unit-upgrades.md) | Unit upgrades: the command gate `0x5C1AD0`, eligibility `0x5C0620` (Army carrier, city, replacement type, gold, Barracks/Harbor/Airport by domain), the price `0x5C04D0` (shield difference times RULE upgrade cost, Leonardo's halving, AI difficulty discounts, clamp), the execution `0x5C0740` (obfuscated treasury write, field copies, cargo transfer, old-unit kill even on factory failure), Upgrade All `0x56AAE0`/`0x56AF00` and message kind 47, the AI helper at Unit vtable `+0x44`, golden vectors → `rust/src/upgrade.rs` |
| [`borders-culture.md`](borders-culture.md) | Borders, tile ownership and the culture flip: the culture accumulation `0x4B2680`, the culture level and its thresholds `0x4B0C60`, the ring enumerator `0x5E6E50` (closed form, squared reach per level), the three-phase owner recompute `0x5D4830` (claim, ocean and bracket rules, tie-break `0x5D3850`, orphan resolution `0x5D4370`, release), the owner write `0x5D3AB0` and its side effects (vision, colonies, camps, huts, airfields, radar, outposts), the empire culture total `0x4F8E20`, the flip `0x4B28D0` (gates, score, modifiers, distance metric, roll), golden vectors |
| [`disease.md`](disease.md) | City disease `0x4B45A0`: the worked-tile count by terrain (*Causes Disease*), the literal technology row 8 that removes *Cured by Sanitation* terrains, the strength-to-bound arithmetic and the per-tile infection draws, the daily citizen loss and the cure roll, notification variants, draw order, golden vectors executed on the real routine. Clean-room specification, no Rust module |
| [`worker-jobs.md`](worker-jobs.md) | Worker jobs: the 13 TFRM job rows and the job-row table (set / clear / required / terrain), the order tokens of `Unit::canDoAction`, `Player::canImprove` `0x55EFA0` (per-job gates, water source, destruction confirmation), the work rate `0x5B33C0` (government, Industrious, Doubles Work Rate, foreign nationality, PRTO strength), `Unit::workOnTile` `0x461470` completion effects in order, overlay set/clear, forest chop bonus, golden vectors |
| [`colonies.md`](colonies.md) | Colonies, airfields, radar towers and outposts: the four pools and the pool protocol, object layout, the Colony site gate `0x5D7080` / `0x5F3090` (return codes), the creators and inits with their vision footprints (3x3 / 5x5 / 9, 25, 49), the observation test `0x55AC10`, the four destroyers and wrappers, the airfield transfer `0x5DB0D0`, every trigger (arrival, border change, city founding, lava, nuke, civ destruction), bombard improvement destruction `0x5B4DC0`, golden vectors and quirks |
| [`espionage.md`](espionage.md) | Espionage: the nine ESPN mission rows and the government fields that gate them (`immune_to`, `diplomats_are`, `spies_are`, VsGovernment propaganda modifier), the mission agents embedded in `Player`, the start dispatcher `0x528CA0`, cost `0x523E40` (per-row terms, distance and size-class term, final ratio), quote `0x5240E0`, setup `0x524410`, dispatcher `0x5266A0` (gold payment and re-split), the incident tail `0x502CC0`, every mission executor (Establish Embassy, Investigate City, Steal Technology / World Map / Plans, Plant Spy, Initiate Propaganda, Sabotage, Expose Enemy Spy) with its rolls and draw order, the computer player's driver `0x445490`, `tryEspionage` `0x445160` and the pickers `0x44A5B0` / `0x44A630` / `0x44A800`, golden vectors, quirks. Clean-room specification, no Rust module |
| [`vision.md`](vision.md) | Vision: the per-cell sight masks (`+0x58` discovered, `+0x5C` unit, `+0x60` structure, `+0x64` territory, `+0xD0` air reveal, `+0xD4` radar, `+0x68` line-of-sight mask, `+0xAE+slot` remembered overlay) and their set / clear primitives, the visible-now test, the line-of-sight mask builder `0x5D6500` (tables, three phases, golden vectors, emulator-verified on 208 032 tiles), the sight predicate `canSee` `0x5BA010` (radius 2, Radar, sentry ships radius 3), Army ability inheritance, the unit sight refresh `0x5BA1D0` (modes, all call sites), the air reveal `0x5C74A0` / `0x5C7570`, territory sight, quirks |
| [`movement.md`](movement.md) | Movement (partial): the movement-point scale, prototype and unit allowance (`0x5CDDF0`, `0x5BE470`, ship wonders, Seafaring, Army rule), terrain cost fields, `Unit::setPosition` in full (leave / enter tile, cargo, worked-tile eviction, martial law, hut / camp / colony / flag hooks), teleport, the mover `0x5B8FC0` stages and return codes, carrier selection, movement-free port commands, shore disembark choices (`0x5C5420`) and amphibious refusal gates, entry guards of the step evaluator `0x57F360` (its body, the path finder and ZOC are open) |
| [`unit-ai.md`](unit-ai.md) | Unit commands and the unit AI (partial): the per-turn pump `0x449B20` (orders 2..14, 15 AI sweeps), the strategy dispatcher `0x4611F0` with the full bit-to-handler table (Terraform `0x45C750` = worker automation), the Flag Unit / Cruise Missile / ICBM handlers and Explore's order ladder; the large handler bodies are open |
| [`STATUS.md`](STATUS.md) | Inventory by document: what is specified to clean-room grade, what predates it, what is lead-only, and the known unspecified gameplay systems (no coverage percentage is claimed) |
| [`media.md`](media.md) | Movies/victory media: intro gate, selectors, wonder art → `rust/src/media.rs` |
| [`rust/`](rust/) | Reference implementation. 611 tests (`cargo test`: 587 lib + 5 bin + 2 ground-truth + 15 oracle + 2 doc) |

## Quick start

```sh
cd rust
cargo run --release -- --size 2 --water 50
cargo test --release
```

Paths in this directory are relative to the repo root (`open-4x/`). The GOG
install, the `re/` scratch tree (exe copies, string dumps, `.venv` toolkit), the
pinned Wine runtime and its prefix all live under `civ3/`.

The renderer is faithful to the binary by default. Pass `--bugs none` for the
intended behaviour, or a subset like `--bugs contour-equality`.

## Headline findings

* **The generator is fully mapped.** `generateMap` (`0x5eb580`) is twelve stages;
  all twelve are identified. The core is a midpoint-displacement fractal whose
  sea level and coastline are **percentiles of the fractal it just generated**,
  re-rolled up to ten times until the continent sizes match the landmass slider.
  Against three shipped maps the land/sea stage reproduces 89-95 % of cells at the
  game's own draw, but not the choice of draw (the balance test is not implemented)
  and not five ocean-1 saves; see `NOTES.md` section 19.
* **The stored "Oceans" value (`WMAP.water_level`) is only ever a seed.** It is never
  used as a threshold, so its effect on land fraction is indirect and non-monotone.
  The land fraction comes from the separate Ocean Coverage slider, which picks the
  percentile row.
* **Rivers are not placed by map generation.** No stage places them, and the
  generator reads only two `.biq` sections — `TERR` and `GOOD` — so there is not
  even a data path by which a river could enter. Rivers come from a separate
  system applied to an already-generated map. See [`rivers.md`](rivers.md).
* **Goody huts and barbarian camps are separate stages**, `0x5f21b0` and
  `0x5f2090`, fully specified in `NOTES.md` and `rust/src/resources.rs`.
* **A fight is one duel at fixed odds, played a round at a time.**
  `odds = 1024*X/(X+Y)` clamped to 1..1023, where each side's strength is scaled
  by additive percentages: terrain, river, city size, buildings, fortress,
  radar tower, fortify, barbarian difficulty, amphibious assault. A round is
  `next(1024) >= odds`; the loser of a round loses exactly one hit point; retreat
  is a single roll when a side is left at exactly 1 HP. See [`combat.md`](combat.md).
* **Taking a city is one routine with three modes.** A military capture plunders
  gold first (the last city yields the whole treasury, otherwise `treasury / cities`
  scaled 1/2, 3/4, 1 by town, city, metropolis), then shrinks or destroys the city,
  then asks keep or raze; the transfer destroys the Palace of a lost capital, every
  building with culture, every small wonder and a quarter of the rest. Great wonders
  survive. An AI player answers keep-or-raze (and a culture flip's accept-or-refuse)
  with two Player virtual methods that weigh wonders, resources, citizens' race and
  culture, and cap its cities at twice the optimal number (`0x5676C0`, which
  [`economy.md`](economy.md) gives in full). See [`capture.md`](capture.md).
* **Civ traits are a bit mask, and `biq`'s `unique_unit` field is it.** `RACE +0x948`
  (the Conquests RACE tail's dword 13) holds two trait bits per civ (0 Militaristic,
  1 Commercial, 2 Expansionist, 3 Scientific, 4 Religious, 5 Industrious,
  6 Agricultural, 7 Seafaring) that the exe tests through `RACE.vtable[0]`. Five of
  them halve the cost of ordinary buildings of their class (never wonders),
  Militaristic halves the promotion die, and Commercial adds a quarter to the optimal
  city number. See [`capture.md`](capture.md) section 11 and [`economy.md`](economy.md).
* **A worked tile's yield is one skeleton in three functions, and the city's totals are a
  fixed chain.** `0x5D7180` / `0x5D75F0` / `0x5D7AD0` share the steps pollution (0), terrain
  (landmark column), crater, improvement plus the railroad step, resource (only once its tech
  is known), water, city centre, Golden Age, government, and last the Despotism cap, which
  **also caps the city centre**. A city sums its centre and the tiles whose worked-by word
  `[cell+0x6C]` is its id (21 tiles at most), then derives food eaten `(size - resisters) * 2`,
  shields `(4 + bonuses + best power plant) * net / 4`, and commerce (tourists and corruption,
  luxury/science shares in tenths, tax the rest, `(2 + n) / 2` multipliers, Wealth at 4 shields
  per gold). `[city+0x25C]` is luxury and `[city+0x260]` science. See [`yields.md`](yields.md).
* **Moods are rebuilt from scratch, and disorder is decided separately.** `0x4BCFF0` gives every
  ordinary citizen a base mood (the first `citizens_born_content` of the difficulty content, the rest
  unhappy), sums a happy-face and a content-face accumulator (luxury, buildings, martial law, minus the
  draft, propaganda, war weariness and enemy nationals), and spreads them over the citizens in four
  sign regions; two leftover happy faces lift an unhappy citizen straight to happy. It sets no flag.
  Once a turn `0x4BDFF0` puts a city in disorder when **happy < unhappy** (strictly; content,
  resisters and specialists do not count) and may riot a building away (20 % a turn, and a city of 6
  or fewer must also pass a 40 % roll; never the capital, a wonder or the palace); `0x4BE440` starts a celebration for a city of at
  least 6 with nobody unhappy, more happy than content, no resister and a food surplus of 0 or more.
  Disorder doubles and celebration halves the culture-flip score. The Rust port matches the real code
  on 30 000 random cities and 30 000 decisions (differential test, Unicorn). See
  [`happiness.md`](happiness.md).
* **Gameplay dice and map randomness are separate instances of one generator.**
  The `Random` class (`0x60BA80`/`0x60BAB0`) drives map generation through
  private instances and every combat die through the global instance `0xA526B4`
  (171 call sites). MSVC `rand`/`srand` (`0x64A20E`/`0x64A201`) is a different
  LCG and rolls no combat die. An earlier version of this file said combat used
  `rand`; that was wrong. See [`combat.md`](combat.md) section 1, [`ai.md`](ai.md).
* **The `.biq` codec is now validated against the game itself.** The PKWARE
  DCL distance mask comes from `dict_bits`, not from the third header byte
  (`biq.md` correction, 2026-09-29): the old reading corrupted every
  `00 06 84` file — every Conquests `.biq` — while `EGYPT.SAV` happened to be
  unaffected, so the `.sav` golden tests stayed green. With the fix, the
  decode of `conquests.biq` is byte-identical to the temp file the game
  itself writes while loading its rules, and the rules data (`GOOD` names and
  frequencies, the 14 `TERR` resource allow-masks) can be read from the file
  instead of hardcoded. See [`biq.md`](biq.md), [`resources.md`](resources.md).
* **A save is a memory dump with a fixed order, and the order was read off the game.**
  `game_data` (`0x590030`) walks the live objects in one fixed sequence: the embedded
  `BICQ` rules, `GAME`, console, map, 32 players, units, cities, history, replay,
  network queue; each object is a few tagged chunks (`memcpy` of a range of the
  object, uninitialised heap included) plus raw arrays sized by the embedded rules'
  row counts (BLDG / PRTO / TECH / GOOD / spaceship parts, defaulting to 83 / 141 / 83 /
  26 / 10). The sub-version gates (map before the players from 9, a `DATE` in each
  city from 4, the GUID from 7, ...) were fixed by running the exe's loader under
  Unicorn over every corpus save and over synthetic downgrades, and the Rust
  `civ3_biq::Save` reproduces all 22 files byte for byte. The current year is the
  last history record's, not the `GAME` date's, which lags a turn. See
  [`savegame.md`](savegame.md).
* **A tile's units are one list, newest placement first.** The list head sits
  in `Cell+0x0C`; `Unit::setPosition` (`0x5BD220`) pushes each placed unit at
  the head, and nothing else reorders a tile (no "fortify to the bottom").
  Every query and panel walks from the head, while the display path that draws
  a *single* unit sprite for a tile keeps the last entry — the tile's oldest
  resident. See [`stacking.md`](stacking.md).

## Scope (non-goals)

* Combat odds, rounds, retreat and bombard are recovered ([`combat.md`](combat.md));
  its section 12 lists what stays open (stack and army participant reselection
  `0x4A4B30`, the kill routine `0x5BBBC0`, the AI fight estimator `0x4A7280`, the
  mounted-unit bonus). City capture is recovered too ([`capture.md`](capture.md)),
  including the AI's raze and accept decisions; the other Player vtable methods
  (`0x437F80..0x44A800`) are unread. The turn-loop root is mapped (record queue
  pump `0x468210`, kind dispatch `0x46F8B0` + table `0x47055C`), leaving only semantic
  names for its four producer routines; [`ai.md`](ai.md) lists them as next targets.
* The river-grained renderer mapping is marked `HYPOTHESIS` in [`rivers.md`](rivers.md)
  and `rust/src/rivers.rs`. Do not treat it as verified. The combat river bonus reads
  `byte[cell+4]` as an eight-direction set ([`combat.md`](combat.md) section 4.1), which
  supports the `byte[cell+4]` lead but does not fix its layout. `byte[cell+5]` is **not**
  a river byte: it is the tile's owner civ id, and the table `0xA53BC8` is the at-war
  table ([`combat.md`](combat.md) section 14.3).
* Overlay stacking order in `graphics.rs` is inferred from file roles, not
  disassembly.
