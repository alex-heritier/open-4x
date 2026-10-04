# Running Civ3's own files: `.biq` and `.sav`

Status: **brainstorm / proposal**. Open questions at the end.

## Goal

open-4x plays Civ3's own `.biq` and `.sav` files directly. There is no
import step for rules, maps or saves, and no open-4x content format. Art and
sound are still converted once to PNG/OGG (section 7). Everything is chosen on the
command line, with no main menu or setup screen:

```
open-4x                                    # new random game, Conquests rules (conquests.biq)
open-4x path/to/Scenario.biq               # a scenario or mod (.biq / .bix / .bic)
open-4x path/to/Game.SAV                   # a saved game
```

What happens with a `.biq` depends on what it holds:

| The `.biq` has | open-4x does |
|---|---|
| rules only (a mod, or `conquests.biq` itself) | random map, played by those rules |
| a map, no players (`LEAD`) | that map, civs picked as for a random game |
| a map and players | the scenario as authored: its map, players, cities, units, victory settings |
| no rules (the scenario uses the defaults) | the rules of the stock `conquests.biq`, like Civ3 does |

## Where we are today

| What | Where it lives now | Problem |
|---|---|---|
| Units, buildings, techs, civs, leaders, governments, terrain facts | `src/rules_data.rs`, generated from `conquests.biq` by `biq/examples/gen_game_rules.rs` and compiled in | One rule set per build |
| Game logic tied to specific rows | ~640 uses of named constants: `UnitType::Warrior` (104), `UnitType::Worker` (59), `Production::Temple` (37), `Production::ThePyramids` (17), ... | A mod that changes or drops those rows breaks the game |
| Player count | `civs::CIV_COUNT = 4`, `[T; CIV_COUNT]` arrays everywhere | Scenarios have 2 to 31 players |
| Map | `GameMap::generate` from `MAP_SEED` | No way to play a scenario's map |
| Art and audio | `tools/prep_assets.py` → `assets/gen/` PNG/OGG plus JSON manifests. It gets the unit list by pattern-matching `rules_data.rs`, hardcodes the leaders of four civs, and has a hand-written wonder table | Fine as a format (section 7), but it misses most leaders and every scenario's own art |
| Saves | `src/save.rs`, our own JSON (format 11) | Not a Civ3 save |

What already exists:

* `biq/` reads every `.biq`/`.bix`/`.bic` into typed rows (`Biq::read_file`,
  `Rules`, `MapData`, `Scenario`), including a scenario's art override folders
  (`GAME.search_folders`).
* `biq/src/sav` reads **and writes** Civ3 `.SAV` byte for byte. Units, cities,
  tiles, players, the turn and the embedded BIQ are decoded. Most of the big
  runtime blocks (`LEAD`, `CITY`, `CTZN`, `UNIT` bodies) are only partly
  decoded (`reverse-engineering/savegame.md` section 9).
* Civ3 itself is fully data-driven: the exe never knows what a "Granary" is,
  only BIQ fields and flags. Almost every named constant has a field that
  means the same thing (step 2 below).

## The shape

```
 CLI args ──▶ Civ3 install dir + file (.biq / .sav)
                    │
                    ▼
          biq crate: Biq / Save  ──▶  Ruleset (in memory)  ──▶  game
                    │                     │
                    │                     └─ art, sounds, text from assets/gen/
                    │                        (converted once by prep_assets.py)
                    └─ map / scenario / saved state ──▶ game world
```

The `biq` crate becomes a dependency of the game (today it is standalone on
purpose). The in-memory `Ruleset` is plain Rust structs built from the BIQ at
startup, not a file format; nothing is written to disk.

## 1. Command line

Hand-rolled over `std::env::args` (a few flags; no need for `clap`):

```
open-4x [FILE] [options]

  FILE                  .biq/.bix/.bic scenario or mod, or .sav saved game
                        (default: <civ3>/Conquests/conquests.biq, a new random game)
  --civ3 <dir>          Civ3 install root (default: $CIV3_DIR, else civ3/civ3-gog/app)

New games (ignored for a .sav):
  --civ <name>          the human's civilization (RACE name, case-insensitive)
  --opponents <n|names> number of rivals, or a comma list of RACE names
  --difficulty <name>   DIFF row (default: RULE.default_difficulty)
  --size <name>         WSIZ row (default: Standard)
  --land, --water, --climate, --temperature, --age, --barbarians
                        WCHR settings, as Civ3's custom-world screen names them
  --seed <n>            map seed
```

The current env vars fold into this: `CIV3_PLAYER` → `--civ`,
`CIV3_CIVS` → `--civ` + `--opponents`, `MAP_SEED` → `--seed`,
`CIV3_GOG` → `--civ3`. The dev/debug ones (`CIV3_SHOT`, `CIV3_SCRIPT`,
`CIV3_AUTOPLAY`, `CIV3_HOTSEAT`, `CIV3_REVEAL`, ...) can stay env vars or
become flags; either is fine, as long as the screenshot and script tooling
keeps working.

Since there is no menu, the app keeps starting straight into the game: the
CLI is parsed in `main` before `App::new()`, the file is loaded there, and
the resulting rules and world are handed to the existing `Startup` systems.
No app states needed.

## 2. Rules from the BIQ at startup

Replace `rules_data.rs` with a `Ruleset` built from `Biq::rules` when the
game starts.

Least invasive: keep the `&'static` accessors. Build the `Ruleset` once in
`main`, `Box::leak` it, and put the reference in a `OnceLock`.
`UnitType::row()`, `roster::unit()`, `CIVS.get()`, `rules()`, `tables()` keep
their signatures and callers do not change. One game per process means the
leak is once, by design.

Before deleting `rules_data.rs`: a test that builds the `Ruleset` from
`conquests.biq` and compares it field by field with the generated tables.
Then delete `rules_data.rs` and `biq/examples/gen_game_rules.rs`.

`UnitType(pub u8)` probably becomes `u16`: mods with more than 255 units exist.

## 3. Remove the named constants

The bulk of the work. Each `UnitType::X` / `Production::X` becomes the BIQ
field or flag that Civ3 itself reads:

| Today | What Civ3 reads instead |
|---|---|
| `UnitType::Settler` / `Worker` | the build-city / worker-job bits of the `PRTO` worker actions; start units from `DIFF` + `RULE` |
| `UnitType::Scout` | `RULE.scout_unit` |
| `UnitType::Warrior` for barbarians | `RULE.basic_barbarian_unit`, `advanced_barbarian_unit`, `barbarian_sea_unit` |
| `UnitType::Leader`, `Army` | `RULE.battle_created_unit`, `RULE.build_army_unit` |
| captured Settler → Workers | `RULE.captured_unit` |
| `UnitType::Galley` checks | sea class + `capacity` + the coast-only flag |
| `Production::Granary` | `DOUBLES_CITY_GROWTH_RATE` (`0x200`, the exe's own Granary test, `hurry.md`) |
| `Production::Aqueduct` / `Hospital` | `ALLOWS_CITY_SIZE_LEVEL_2` / `_3` against `RULE.town_max_size` / `city_max_size` |
| `Production::Temple`, `Marketplace`, `Library`... | `happy_faces`, `INCREASES_LUXURY_TRADE`, `TAX_BONUS`, `RESEARCH_BONUS` |
| `Production::Walls` | `defense_bonus` |
| `Production::Barracks` | `VETERAN_GROUND_UNITS` |
| `Production::ThePyramids` (wonders generally) | `gain_in_every_city_on_continent` (Pyramids = a Granary in every city, `buildable.md`), `gain_in_every_city`, the wonder flags |
| `Production::Palace` / `ForbiddenPalace` | `CENTER_OF_EMPIRE` / small-wonder `REDUCES_CORRUPTION` |
| `Production::Wealth` | `CAPITALIZATION` |

Many of the hits are in tests; those can look rows up by name.

Done when `grep -E "UnitType::[A-Z]|Production::[A-Z]" src` finds nothing
outside `#[cfg(test)]`.

## 4. Any number of players

`CIV_COUNT` becomes a runtime value (up to 31 civs plus the barbarians, as in
Civ3). `[T; CIV_COUNT]` → `Vec<T>`, or a fixed cap of 32 with a live count,
whichever means less churn in each file. Player slots hold `RACE` row indices,
which is what both the BIQ (`LEAD`) and the SAV use.

## 5. Maps and scenarios from the BIQ

* **Map**: `WMAP`/`TILE`/`CONT` → `GameMap` (terrain, overlays, resources,
  rivers, huts, barbarian camps). `MapView` in the `biq` crate already gives
  tiles by `(x, y)`.
* **Players and objects**: `LEAD` → players (civ, human or computer, gold,
  government, techs, start units), `CITY` → cities (size, buildings, name,
  owner), `UNIT` → units, `CLNY` → colonies, `SLOC` → start locations.
* **Settings**: `GAME` → victory conditions, turn limit, calendar, locked
  alliances, and which player slots a human may take.
* **Random maps** keep using our generator, with its parameters from
  `WSIZ`/`WCHR` and the CLI instead of constants.

## 6. Saved games are `.SAV`

**Loading** a `.SAV`: the embedded BIQ supplies the rules (exactly as in
Civ3), the decoded tiles, units, cities and players build the world. Fields
the RE has not decoded yet get defaults; each one we hit becomes a decoding
task in `savegame.md`.

**Writing** a `.SAV` (F5): build a `civ3_biq::Save` from the game state and
write it with `Save::to_bytes`. Two ways, depending on how much we care that
real Civ3 can open it:

* **open-4x only**: fill the decoded fields, zero the rest. Achievable as soon
  as everything the clone tracks has a decoded home.
* **Civ3 can open it too**: every field the exe's loader checks must be
  valid. Harder, but `tools/emu/` can run the game's own loader over our
  output and tell us exactly where it fails, which makes it a well-defined task.

The gap either way: state the clone keeps but whose place in the SAV is not
decoded yet (AI attitude memory, flip ratings, culture, war weariness, ...).
Until it is all mapped, the current JSON quicksave would stay as a stopgap,
and be deleted once `.SAV` writing covers it.

## 7. Art and sound: converted once, by `prep_assets.py`

Rules, maps and saves are read straight from Civ3's files. Art and sound are
not: `prep_assets.py` keeps converting them into formats Bevy loads natively
(PNG, OGG, WAV) under the gitignored `assets/gen/`. That output is a cache
regenerated from the install, never edited by hand, so it isn't an open-4x
content format.

Why keep converting:

* Bevy loads PNG/OGG/WAV with no extra code. PCX and FLC would need custom
  decoders and asset loaders, and the MP3 music an MP3 decoder.
* The pipeline already works and already holds the hard-won details: the
  transparency rules, unit shadows, team-color ramps, facing order, foot
  offsets, `.amb` sound cues, terrain cropping.
* Decoding FLCs at runtime (141 units × several clips × 8 directions) would
  cost startup time or need lazy loading. Converting once costs neither.
* Python/PIL/ffmpeg are a developer-side prerequisite only, which is fine
  while players have to supply their own Civ3 install anyway.

What changes:

* **The art list comes from the BIQ, not from `rules_data.rs` and hardcoded
  tables.** Simplest version: convert everything the install has (every
  `Art/Units` folder, every leader clip in `Art/Flics`, every wonder splash)
  instead of what the rules name, so prep never needs to parse a BIQ. That
  fixes the four-leader limit.
* **Scenario art.** A scenario's own art (its folder and
  `GAME.search_folders`) is converted into its own cache directory:
  `python3 tools/prep_assets.py --scenario path/to/X.biq` →
  `assets/gen/scenarios/<X>/`. The game looks there first, then in
  `assets/gen/`, the same override order Civ3 uses. To find the search
  folders, prep reads them from the BIQ through a tiny `biq` example that
  prints them; that keeps BIQ parsing in Rust. When converted art is missing,
  the game prints the exact prep command to run.
* **Case-insensitive lookup** in prep: the install's file names don't match
  their references in case. That works on Windows and breaks on macOS/Linux.
* Text files the game reads (`diplomacy.txt`, ...) keep being copied by prep.

## Suggested order

Each step leaves the game playable.

1. **CLI** with today's behavior as the default, env vars folded in.
2. **Rules from the BIQ** at startup, parity test, delete `rules_data.rs`.
   `open-4x some-mod.biq` now plays a mod's numbers on a random map.
3. **Named constants → BIQ fields.**
4. **Any number of players.**
5. **Scenario maps, players, cities and units** from the BIQ.
6. **Load `.SAV`**, then **write `.SAV`**.
7. **Prep driven by the install, not `rules_data.rs`** (all units, all
   leaders, all wonders), plus per-scenario art caches.

2 and 3 are the bulk and unblock the rest. 7 can run in parallel with any of
them.

## Open questions

1. **Must `.SAV` files written by open-4x open in real Civ3**, or only in
   open-4x? The first is much more work (section 6).
2. **Vanilla and PTW files**: Conquests `.biq` and Conquests `.SAV` first, or
   do Civ3 1.x `.bic` / PTW `.bix` and their saves need to work from the
   start? The parser reads all `.biq` versions; the SAV reader only reads
   Conquests saves (format 24).
3. **Rules the engine does not implement** (a mod's flag nobody has coded):
   print a warning at startup and ignore it (proposed), or refuse to start?
