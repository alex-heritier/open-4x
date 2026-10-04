# Content packs: playing Civ3, mods and saves from data

Status: **brainstorm / proposal**. Nothing here is accepted yet; the open
questions are at the end.

## Goal

Start the game three ways:

1. **New random game** on the stock Civ3 rules (Conquests by default).
2. **A mod or scenario** (a PTW/Conquests scenario, a rules-only mod) with its
   own rules, map, players and art.
3. **A saved game.**

The game never reads raw Civ3 files. An importer converts them once into the
open-4x format; the runtime loads only that.

## Where we are today

| What | Where it lives now | Problem |
|---|---|---|
| Units, buildings, techs, civs, leaders, governments, terrain facts | `src/rules_data.rs`, generated from `conquests.biq` by `biq/examples/gen_game_rules.rs`, compiled in as `static` arrays | One rule set per build. A mod means regenerating and recompiling. |
| Game logic tied to specific rows | ~640 uses of named constants: `UnitType::Warrior` (104), `UnitType::Worker` (59), `Production::Temple` (37), `Production::ThePyramids` (17), ... | A mod that renames or drops a Warrior breaks the game. |
| Player count | `civs::CIV_COUNT = 4`, `[T; CIV_COUNT]` arrays in saves, AI, diplomacy, borders | Scenarios have 2-31 players. |
| Map | `GameMap::generate` (own noise plus `civ3mapgen` stages) from `MAP_SEED` | No way to load a scenario's map. |
| Art and audio | `tools/prep_assets.py` → `assets/gen/`. Unit list scraped from `rules_data.rs` by regex; leaders hardcoded for Japan/Rome/Egypt/China; wonder → pedia key table hand-written | Not driven by the rule set; ignores a scenario's search folders. |
| Saves | `src/save.rs`, JSON format 11. Requires the same seed and roster. | Does not record which rules it was played with. |
| Startup | Straight into a game, configured by env vars (`CIV3_CIVS`, `MAP_SEED`, ...) | No menu, no setup screen. |

What we already have that makes this tractable:

* `biq/` reads every `.biq`/`.bix`/`.bic` section into typed rows
  (`Biq::read_file`, `Rules`, `MapData`, `Scenario`), including the scenario
  search folders (`GAME.search_folders`).
* `biq/src/sav` reads Civ3 `.SAV` files structurally, with units, cities,
  tiles and players decoded (`reverse-engineering/savegame.md`).
* Civ3 itself is fully data-driven: the exe never knows "Granary", only BIQ
  fields and flags. Almost every named constant above has a BIQ field that says
  the same thing (below).

## The shape

```
 Civ3 install (read-only, never committed)        open-4x
 ┌───────────────────────────────┐   import   ┌──────────────────────┐   load   ┌──────────┐
 │ conquests.biq, PTW/Conquests  │ ─────────▶ │ content pack (dir)   │ ───────▶ │  game    │
 │ Scenarios/*.biq, Art/, Text/, │            │ pack.json + rules/*  │          │ runtime  │
 │ Sounds/, search folders       │            │ + map/scenario + art │ ◀─────── │          │
 └───────────────────────────────┘            └──────────────────────┘   saves  └──────────┘
```

Three layers, each with one job:

1. **Importer** (offline, run once per install or mod): BIQ + art → pack.
2. **Content pack** (a directory, the open-4x format): everything the game
   needs to play a rule set, plus optionally a map and a scenario setup.
3. **Runtime**: loads a pack (new game / scenario) or a save (which embeds its
   rules), and nothing else.

## Content pack format

A directory. JSON throughout (see "Why JSON").

```
packs/
  civ3-conquests/                 # the stock rule set, imported from conquests.biq
    pack.json
    rules/
      general.json                # RULE: city sizes, hurry rates, start/scout/barbarian units...
      terrain.json                # TERR + TFRM (worker jobs)
      resources.json              # GOOD
      techs.json                  # TECH + ERAS
      units.json                  # PRTO
      buildings.json              # BLDG
      governments.json            # GOVT
      civs.json                   # RACE (+ leader, city names, colors, art refs)
      difficulties.json           # DIFF
      world.json                  # WSIZ, culture levels, experience, citizens, flavors, espionage
    text/                         # diplomacy lines, civilopedia (converted from Text/*.txt)
    art/  audio/                  # converted PNG/OGG/WAV + per-asset JSON manifests (today's assets/gen/)
  some-scenario/                  # a scenario, imported from a .biq
    pack.json                     # "extends": "civ3-conquests"
    rules/...                     # present only if the scenario has custom rules
    scenario/
      map.json                    # WMAP/WCHR header, tiles, continents, start locations
      setup.json                  # GAME (victory, turn limit, calendar, alliances), LEAD players, CITY, UNIT, CLNY
    art/...                       # only the files its search folders override
```

`pack.json`:

```json
{
  "format": 1,
  "id": "some-scenario",
  "name": "Some Scenario",
  "extends": "civ3-conquests",
  "source": { "file": "Conquests/Scenarios/Some Scenario.biq", "biq_version": "12.08" },
  "has_map": true,
  "has_setup": true
}
```

### Rules: string IDs, not row numbers

Every record gets a stable slug; cross-references use slugs:

```json
{
  "id": "swordsman",
  "name": "Swordsman",
  "art": "Swordsman",
  "class": "land",
  "attack": 3, "defense": 2, "moves": 1, "hp_bonus": 0,
  "cost": 30, "pop_cost": 0,
  "requires_tech": "iron_working",
  "requires_resources": ["iron"],
  "upgrades_to": "medieval_infantry",
  "abilities": ["wheeled", "amphibious", "zoc"],
  "worker_jobs": [],
  "ai_roles": ["offense"],
  "civs": "all"
}
```

At load time the runtime resolves slugs to dense indices (`UnitType(u16)`,
`Production(u16)`), so hot code keeps today's index arithmetic and pays
nothing for strings. Bit masks (`abilities`, `special`, `traits`, building
flags) become lists of names: readable, diffable, and a mod author can edit
them without the BIQ docs open. The importer owns the bit ↔ name tables.

### Inheritance: keep it to two simple rules

* **Rules**: a pack either has a complete `rules/` or none, in which case it
  uses its base pack's rules wholesale. No per-record merging. This is exactly
  what Civ3 does (a scenario either has custom rules or uses `conquests.biq`),
  so the importer never has to compute a diff.
* **Assets**: looked up through a layered search path, the pack's own `art/`
  first, then each `extends` ancestor. This mirrors Civ3's scenario search
  folders and avoids copying hundreds of MB of unit art into every scenario.

## Importer

One command, `open4x-import`, that replaces both `gen_game_rules.rs` and the
per-civ special cases in `prep_assets.py`:

```
open4x-import --civ3 <GOG dir>                       # stock packs: civ3-vanilla, civ3-ptw, civ3-conquests
open4x-import --civ3 <GOG dir> --biq <file.biq>      # one scenario/mod
open4x-import --civ3 <GOG dir> --all-scenarios       # every .biq/.bix under */Scenarios
```

Steps for one BIQ:

1. **Read** it with the `biq` crate.
2. **Rules** (if the BIQ has them): map each section to its JSON file, assign
   slugs, translate bit masks to names. Write a warning for every field or
   flag the engine does not implement, so a mod's unsupported features are
   visible up front.
3. **Map / scenario** (if present): `WMAP`/`WCHR`/`TILE`/`CONT`/`SLOC` →
   `map.json`; `GAME`/`LEAD`/`CITY`/`UNIT`/`CLNY` → `setup.json`.
4. **Art plan**: from the rules, list what art the pack needs: every
   `PRTO.art` unit folder, `RACE.era_art` leader clips for **all** civs,
   wonder splashes from `BLDG.civilopedia_entry`, tech icons by name. Resolve
   each through the search path (scenario folder → `GAME.search_folders` →
   Conquests → PTW → base), and emit only what differs from the base pack.
5. **Convert** the art plan into PNG/OGG/WAV.

Implementation split, to avoid rewriting working code: steps 1-4 in Rust
(new crate next to `biq/`, it already has the parser); step 5 stays in
`prep_assets.py`, which reads the art plan JSON instead of regex-scraping
`rules_data.rs` and hardcoding the four leaders. Porting the pixel work to
Rust (PCX via the `image` crate, plus a small FLC decoder to drop ffmpeg) is
only worth doing if we want a one-click "Import Civ3" button in the game for
end users. Later, if at all.

## Runtime changes

### 1. Load rules instead of compiling them

Replace `rules_data.rs` with a `Ruleset` loaded from the pack's JSON.

The least invasive way: keep the `&'static` accessors. On game start,
`Box::leak` the loaded `Ruleset` and store the reference in a global
(`RwLock<&'static Ruleset>`). `UnitType::row()`, `roster::unit()`,
`CIVS.get()` and friends keep their signatures and keep working; the leak is
a few hundred KB per game started in one session. The Bevy `Res<Ruleset>`
can come later where it helps, but it is not a prerequisite.

Parity check for the switch: a test that loads the imported
`civ3-conquests` pack and compares it field by field with today's generated
tables, run once before `rules_data.rs` is deleted.

### 2. Kill the named constants

The real work. Each `UnitType::X` / `Production::X` use becomes the BIQ
field or flag Civ3 itself reads. Some of the mapping:

| Today | Data-driven replacement |
|---|---|
| `UnitType::Settler` / `Worker` | the build-city / worker-job bits of `PRTO` worker actions; start units from `DIFF` + `RULE` |
| `UnitType::Scout` | `RULE.scout_unit` |
| `UnitType::Warrior` for barbarians | `RULE.basic_barbarian_unit`, `advanced_barbarian_unit`, `barbarian_sea_unit` |
| `UnitType::Leader`, `Army` | `RULE.battle_created_unit`, `RULE.build_army_unit` |
| captured Settler → Workers | `RULE.captured_unit` |
| `UnitType::Galley` checks | `class: sea` + `capacity` + the "coast only" flag |
| `Production::Granary` | `DOUBLES_CITY_GROWTH_RATE` (`0x200`, the exe's own Granary test, `hurry.md`) |
| `Production::Aqueduct` / `Hospital` | `ALLOWS_CITY_SIZE_LEVEL_2` / `_3` vs `RULE.town_max_size` / `city_max_size` |
| `Production::Temple`, `Marketplace`, `Library`... | `happy_faces`, `INCREASES_LUXURY_TRADE`, `TAX_BONUS`, `RESEARCH_BONUS` |
| `Production::Walls` | `defense_bonus` |
| `Production::Barracks` | `VETERAN_GROUND_UNITS` |
| `Production::ThePyramids` (wonders generally) | `gain_in_every_city_on_continent` (Pyramids = a Granary per city, `buildable.md`), `gain_in_every_city`, wonder flags |
| `Production::Palace` / `ForbiddenPalace` | `CENTER_OF_EMPIRE` / small-wonder `REDUCES_CORRUPTION` |
| `Production::Wealth` | `CAPITALIZATION` |

Many of the remaining hits are in tests; those can keep naming units by
looking them up (`rules.unit("warrior")`).

Rule of thumb for review: after this phase, `grep -E "UnitType::[A-Z]|Production::[A-Z]" src`
outside `#[cfg(test)]` returns nothing.

### 3. Any number of players

`CIV_COUNT` becomes a runtime value (2..=31 plus barbarians, as in Civ3).
`[T; CIV_COUNT]` → `Vec<T>` (or a small fixed cap of 32 with a live count,
whichever is less churn file by file). Player slots point at `civs.json` IDs,
not `RACE` row numbers.

### 4. App states and a front end

Today everything is spawned at `Startup`. Move game setup to
`OnEnter(AppState::InGame)` and teardown to `OnExit`, and add:

```
MainMenu ──▶ NewGameSetup ──▶ Loading ──▶ InGame
   │  ├────▶ ScenarioPicker ──┘   ▲
   │  └────▶ LoadGame ────────────┘
```

* **New game**: pick a pack (default `civ3-conquests`), then the Civ3 setup
  choices: world size (`WSIZ`), landmass/water/climate/temperature/age
  (`WCHR`), civ and opponents (`civs.json`), difficulty (`DIFF`), barbarians.
  Map generation reads its parameters from those instead of constants.
* **Scenario / mod**: pick a pack. With a map: load tiles, players, cities,
  units, settings from `scenario/`; let the human pick among the playable
  `LEAD` slots. Without a map (a rules-only mod): go to the new-game setup
  with that pack's rules.
* **Load game**: a file picker over `saves/`.

The env vars stay as dev shortcuts that skip the menu
(`OPEN4X_PACK`, `OPEN4X_SCENARIO`, `CIV3_SAVE`, `MAP_SEED`, `CIV3_CIVS`, ...),
so the screenshot and script tooling keeps working.

### 5. Saves

Bump the save format and make a save self-contained for rules:

* **Embed the ruleset** (the resolved JSON, ~hundreds of KB) plus the pack id
  for finding art. Civ3 does the same: every `.SAV` embeds its BIQ. A save then
  keeps loading after the pack is edited or re-imported, and there is no
  version-matching logic to write.
* Store the player list by civ ID and the whole map (tiles already are), and
  drop "same seed and roster" as a load requirement.
* Keep JSON. Compress (zstd/gzip) if size becomes a problem.

Importing **Civ3 `.SAV` files** is a separate, later converter: the embedded
BIQ becomes a pack, the decoded units/cities/tiles/players become an open-4x
save. The `biq` crate gets us most of the way, but many runtime blocks
(`LEAD`, `CITY`, `CTZN` bodies) are only partly decoded, so expect a
best-effort import (positions, owners, sizes, buildings, techs) rather than a
perfect one.

## Why JSON

* `serde_json` is already a dependency; Python writes it natively; no new
  parsers on either side.
* The data is machine-generated first, hand-edited second. JSON round-trips
  exactly and diffs well with one record per line or pretty-printed.
* YAML's main Rust crate (`serde_yaml`) is unmaintained, and YAML's implicit
  typing ("Norway problem", `no` → false) is a real risk with 31 civs' worth of
  names. TOML gets awkward with arrays of 140 tables.

If hand-editing becomes common, accepting JSON5/JSONC (comments, trailing
commas) at load time is a one-crate change and needs no format migration.

## Suggested order

Each step leaves the game playable.

1. **Pack format + importer, rules only.** Import `conquests.biq` to
   `packs/civ3-conquests/rules/`. Game loads it at startup (leaked `&'static`).
   Parity test, then delete `rules_data.rs` and `gen_game_rules.rs`.
2. **Named constants → data.** Work through the grep list module by module.
3. **Data-driven art.** Importer writes the art plan; `prep_assets.py` follows
   it (all 31 leaders, every unit, wonders). Asset paths go through a resolver
   with the pack search path instead of literal `"gen/..."` strings.
4. **Dynamic player count.**
5. **App states, main menu, new-game setup screen.**
6. **Scenario import and start-from-scenario** (map, players, cities, units,
   victory/turn-limit settings). First targets: one PTW and one Conquests
   scenario, plus one popular rules-only mod.
7. **Saves embed rules**, load from a picker.
8. *(Stretch)* Civ3 `.SAV` import.

Steps 1-2 are the bulk of the refactor and unblock everything else; 3-5 are
mostly independent of each other.

## Open questions

1. **"Load a save game"**: only open-4x saves, or original Civ3 `.SAV` files
   too? (This doc assumes open-4x first, Civ3 `.SAV` as a stretch.)
2. **JSON vs YAML**: this doc recommends JSON. Any strong preference?
3. **Pack location**: `assets/packs/` in the repo (gitignored, like
   `assets/gen/` today) or a per-user data dir (`~/.local/share/open-4x/packs`)?
4. **Original content**: should open-4x eventually ship its own non-Civ3 pack
   so it runs without a Civ3 install? If so, effect names should be our own
   vocabulary (`"keeps_food_on_growth"`) rather than Civ3 bit names, which this
   doc already leans toward.
5. **Unimplemented rules in mods**: warn at import and ignore (proposed), or
   refuse to load packs that use them?
6. **Vanilla Civ3 / PTW rule sets**: import them as their own packs
   (`civ3-vanilla`, `civ3-ptw`), or is Conquests the only base that matters?
   Older BIQ rows are prefixes of newer ones, so the parser already handles
   them; the cost is testing.
