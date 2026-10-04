# Running Civ3's own files: `.biq` and `.sav`

Status: **brainstorm / proposal**. Open questions at the end.

## Goal

open-4x plays Civ3's own `.biq` and `.sav` files directly. There is no
import step and no open-4x content format. Art is converted to PNG
automatically on first use and cached (section 7). Everything is chosen on the
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
| Art and audio | `tools/prep_assets.py` → `assets/gen/` PNG/OGG plus JSON manifests. It gets the unit list by pattern-matching `rules_data.rs`, hardcodes the leaders of four civs, and has a hand-written wonder table | A manual step; misses most leaders and every scenario's own art (section 7) |
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
                    │                     └─ art: Civ3 path → assets/cache/ (converted on
                    │                        first use); sound and text from the install
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

## 7. Art and sound: converted automatically, cached by Civ3 path

Rules, maps and saves are read straight from Civ3's files. Art is converted
to PNG, but nobody runs a script: the game converts what it needs the first
time it needs it and caches the result in the gitignored `assets/cache/`.
Later runs load the cache directly.

### Only two formats need converting

* **PCX** (images) → PNG.
* **FLC** (unit and leader animations) → one PNG strip per direction, plus a
  small JSON sidecar with what the game reads today from `prep_assets.py`'s
  manifests: frame count, ms per frame, foot offsets.
* **WAV** loads in Bevy as is, and so does **MP3** with Bevy's `mp3` feature.
  Audio is read from the install directly, with no conversion and no cache.
* **Text** (`diplomacy.txt`, `PediaIcons.txt`, unit INIs, `.amb` sound cues)
  is parsed directly from the install.

### The mapping: Civ3 path in, cache path out

Game code names assets the way Civ3 does, so Civ3's own references (BIQ
fields, unit INIs, `PediaIcons.txt`) keep working unchanged:

```rust
civ3::image("Art/Units/Warrior/WarriorRun.flc")   // -> Handle<Image> (or a clip)
civ3::image("Art/Terrain/xtgc.pcx")
civ3::sound("Sounds/Diplomusic/DipASEarlyPeace.mp3")
```

A lookup goes through two steps:

1. **Resolve** the reference to a real file, in Civ3's order: the scenario's
   own folder and `GAME.search_folders`, then `Conquests/`, `civ3PTW/`, then
   the base install. Matching is **case-insensitive** (Civ3's references and
   file names disagree in case, which only Windows forgives), using an index
   of the install's file names built once at startup.
2. **Map** the resolved file to its cache path, by a fixed rule: the path
   relative to the install root, lowercased, with the output extension
   appended:

   ```
   Conquests/Art/Units/Warrior/WarriorRun.flc
     -> assets/cache/conquests/art/units/warrior/warriorrun.flc/<dir>.png  + clip.json
   Art/Terrain/xtgc.pcx
     -> assets/cache/art/terrain/xtgc.pcx.png
   ```

Because the cache key is the *resolved* file, a scenario that overrides
the Warrior gets its own cache entry and the stock Warrior keeps its own.
Nothing has to know about scenarios beyond the resolver.

`assets/cache/index.json` records the mapping explicitly: for every
converted source, its outputs, the source's size and modification time, and
the converter version. It serves two purposes:

* **Invalidation**: an entry whose source changed, or whose converter version
  is older than the game's, is converted again.
* **Debugging**: a quick answer to "which Civ3 file did this PNG come from".

Deleting `assets/cache/` is always safe: it is rebuilt on the next run.

### When conversion runs

At startup, once the BIQ is read, the game knows most of what it will
draw: every unit's INI and clips, the terrain sheets, the leaders of the
civs in play, the wonder splashes, the interface. It converts whatever of
that is missing from the cache, in parallel, printing progress to the
console. The first run against a new install or scenario takes a while;
later runs skip it. Anything not predicted is converted on first use.

Team colors (palette entries 0-63 swapped for the owner's ramp) are baked
the same way, per team color actually in play, as `<dir>.team04.png` next
to the base strip.

### Moving off `prep_assets.py`

The conversion lives in Rust, in the game, rather than shelling out to
Python. "Automatic" means it runs on every player's machine, and requiring
Python, PIL and ffmpeg there is fragile. PCX already has a Rust decoder
(`terrain-builder/src/pcx.rs`); FLC needs a small one (a palette plus a
handful of chunk types).

`prep_assets.py` holds a lot of hard-won detail that has to come along:
transparency (magenta and index 255 clear, pure red as shadow), facing
order, foot offsets, `.amb` parsing, team ramps. Its crops also have to find
a new home. The cache stores whole sheets, as Civ3 does, and the game cuts
cells out of them with texture atlases instead of loading pre-cropped files.
The few computed picks (the purest terrain cells, splitting canopy sprites)
run when the sheet is first converted and go into its sidecar JSON.

Feasibility: everything the script does is pixel work (crops, pastes, alpha
masks, palette swaps, connected components) plus two calls to ffmpeg: FLC
decoding and MP3 → OGG. The MP3 step disappears (Bevy plays MP3). The risks
are narrow:

* **FLC must decode exactly like ffmpeg**: same frame count (including the
  trailing ring frame), same pixels, every chunk type the install uses.
* **PCX variants**: `terrain-builder`'s reader only takes 8-bit single-plane
  files. Any other variant in the install needs adding.
* **The wonder thumbnails** use PIL's Lanczos resize; a Rust Lanczos will
  differ by a few levels per pixel. Invisible, but not byte-identical.

Each ported stage is checked by **diffing its output pixel by pixel against
`prep_assets.py`'s output** over the whole install, before the Python stage
is removed.

Port stage by stage. Until a stage is ported, its `assets/gen/` files keep
working; once all are, `prep_assets.py` and `assets/gen/` are deleted.

## Suggested order

Each step leaves the game playable.

1. **CLI** with today's behavior as the default, env vars folded in.
2. **Rules from the BIQ** at startup, parity test, delete `rules_data.rs`.
   `open-4x some-mod.biq` now plays a mod's numbers on a random map.
3. **Named constants → BIQ fields.**
4. **Any number of players.**
5. **Scenario maps, players, cities and units** from the BIQ.
6. **Load `.SAV`**, then **write `.SAV`**.
7. **Automatic asset cache**: the resolver, PCX and FLC conversion in Rust,
   `index.json`, then `prep_assets.py`'s stages ported one by one and the
   script deleted.

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
