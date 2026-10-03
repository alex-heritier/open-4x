# The scenario file format: `.biq` / `.bix` / `.bic` (solved)

Owns the **section-level** format of Civilization III scenario and rule-set
files: container framing, versions and row lengths, every section's layout,
the cross-references between sections, and what the loader does with a row
after reading it. The PKWARE DCL transport codec is documented in
[`biq.md`](biq.md) (the crate implements it, compressor included); the editor's
dialogs that name the fields are in [`editor.md`](editor.md).

Reference implementation: the standalone crate [`../biq/`](../biq)
(`civ3-biq`, no dependencies, its own `[workspace]`). It is deliberately **not**
wired into the game (`src/`) or into [`rust/`](rust/): most `.biq`
functionality is not implemented in the game yet. The rustdoc of each module
under `biq/src/sections/` carries the field-level tables with exe addresses and
evidence tags, so the code and its documentation cannot drift; this file is the
map that ties them together and records what is not in any one module.

## Result

* All **28** section tags the game and the editor read or write are decoded
  into typed, documented structs (17 rules sections, 5 map sections, 5
  scenario sections, the `VER#` header).
* Corpus: **119** content-distinct files from the Steam and GOG installs
  (32 `BIC ` 2.05–4.01, 58 Play-the-World `BICX` 11.06–11.18, 29 Conquests
  `BICX` 12.06–12.08; 83 DCL-compressed, 36 plain), **935 687** rows, of which
  903 994 are map tiles. Every file parses with **zero unmodelled bytes**, has
  no unknown sections, and re-encodes **byte for byte** from the typed model
  (`file::tests::every_shipped_file_parses_fully_and_reencodes_exactly`); every
  section also has its own round-trip test (229 tests in all, the saved-game ones included).
* The **whole file** round-trips as bytes, wrapper included: all 240 on-disk
  copies (72 plain, 168 DCL) come back identical from `Biq::to_bytes`
  (`file::tests::every_shipped_file_round_trips_as_bytes`). The DCL writer is a
  port of the game's own `implode` and reproduces all 93 compressed scenario
  and save files byte for byte ([`biq.md`](biq.md), "The compressor").
* Robustness: 5 602 mutated copies of shipped files (truncations, bit flips,
  hostile counts) return an error or a model, never a panic, and a test-only
  allocator probe caps the memory any one of them may request at 128 MiB; the
  same holds for the 26 record types fed hostile length and count words
  (`sections::tests::hostile_words_never_panic_or_allocate_wildly`). Two
  allocation bombs this found (a `TERR` resource-mask count, a `raw::frame` row
  reserve) are fixed.
* The corpus lives in the git-ignored `civ3/` tree (override with `CIV3_DIR`).
  Without it the corpus tests turn into no-ops; they never fail for a missing
  install.

### Evidence classes

Used in the module docs and here. A claim needs A or B to be called verified.

| tag | source |
|---|---|
| **A** | game exe: the row reader / writer / constructor, or an engine consumer of the field |
| **B** | editor exe: dialog control ↔ row offset (DDX, load, save), editor strings |
| **C** | corpus: a semantic check over the shipped files (index ranges, equalities, counts) |
| **D** | corpus: a value pattern only (constant, always zero) |
| **E** | community documentation; a hypothesis, never sufficient on its own |

## 1. File structure

```text
file    := [DCL wrapper]? stream
stream  := magic[4] section*
section := tag[4] count:u32 row{count}          row := len:u32 body[len]
```

* **Wrapper.** 83 of 119 shipped files are PKWARE DCL streams (header
  `00 06 84`); the loader sniffs the magic and accepts the plain stream too, so
  compression is a transport detail ([`biq.md`](biq.md)). `Biq::to_stream`
  always writes the plain stream; `Biq::to_bytes` writes the file as it was
  read (`Biq::storage`: plain, or DCL with the stream's mode and window), and
  `Biq.storage` can be set to `Storage::GAME` (binary literals, 4 KiB window,
  what every shipped compressed scenario uses) to compress a new file.
* **Magic.** `BIC ` (Civ3 1.x), `BICX` (Play the World and Conquests), `BICQ`
  (accepted by the loader, `0x59433B`; not in any shipped file), `CIV3` (a
  *saved game*: a different stream, DCL-wrapped or raw, modelled by
  `civ3_biq::sav` and documented in [`savegame.md`](savegame.md)).
* **Integers** are little-endian; **strings** are fixed-width NUL-padded
  buffers. The editor leaves stale bytes after the NUL, so the crate keeps the
  whole buffer (`Str<N>`) to stay byte-exact.
* **Tags** are four of `A–Z 0–9 #`. There is no end marker and no section
  count: the loader loop stops when the next tag read comes back short
  (`0x59437B`). A tag the loader does not know is skipped through the shared
  path `0x594E7E`..`0x594F3E` (read the count, `fseek` over the rows). No
  shipped file has an unknown tag or a duplicate one.
* **`FLAV` is the one exception to row framing** (reader `0x52D2C0`, per-flavor
  reader `0x52CCC0`, writer `0x52D530`): there are no length words.

  ```text
  "FLAV"  u32 version (=1)  u32 numFlavors
  numFlavors × { u32 version (=1)   char name[256]   u32 numRelations   i32 relation[numRelations] }
  ```

  A stock record is `4 + 256 + 4 + 7·4 = 292` bytes; `relation[j]` of flavor
  `i` is the percentage chance that a civilization carrying flavor `i`
  researches advances of flavor `j` (diagonal 100, default 50).
* **Section order** is the writer's, not a requirement of the format. The
  game's own writer `0x597070` emits
  `VER# BLDG CTZN CULT DIFF ERAS ESPN EXPR FLAV GOOD GOVT RULE PRTO RACE TECH TFRM TERR WSIZ WCHR WMAP TILE CONT SLOC CITY UNIT CLNY GAME LEAD`
  (`file::CANONICAL_ORDER`); the editor writes others, and 11 distinct orders
  occur in the shipped files. The crate records the order it read
  (`Biq::section_order`) and writes it back.
* **Layers.** The sections fall into three groups that the editor and the crate
  both follow:

  | layer | sections |
  |---|---|
  | rules (the rule set the scenario plays by) | `BLDG CTZN CULT DIFF ERAS ESPN EXPR FLAV GOOD GOVT RULE PRTO RACE TECH TFRM TERR WSIZ` |
  | map | `WCHR WMAP TILE CONT SLOC` |
  | scenario (what is on the map and who plays) | `CITY UNIT CLNY GAME LEAD` |

  `conquests.biq` is a pure rule set (no map, no scenario); a scenario carries a
  full copy of its rules. Of the 119 shipped files, 45 carry a complete rule
  set (29 with `FLAV`, 16 without), the other 74 carry none and use the rules
  the game loads by default; 92 have a map (`TILE`), 27 are rules-only, and 91
  have a `GAME` row.

## 2. Versions

`VER#` is always the first section and always one 720-byte row:
`reserved_a:u32`, `reserved_b:u32`, `major:u32`, `minor:u32`,
`description[640]`, `title[64]`. The loader logs `Loading version %d.%02d BIC
file` and rejects majors outside `2..=12` (`0x594530`, "Incompatible scenario
file version").

| file kind | magic | `VER#` | corpus files |
|---|---|---|---|
| Civ3 1.x `.bic` | `BIC ` | 2.05, 2.07, 2.10, 3.08, 4.01 | 32 |
| Play the World `.bix` | `BICX` | 11.06, 11.09, 11.10, 11.13, 11.18 | 58 |
| Conquests `.biq` | `BICX` | 12.06, 12.07, 12.08 | 29 |

### 2.1 The row rule: older rows are prefixes of newer ones

Every row reader in the exe uses one idiom: each field is read **only if its
whole size is still left in the row**; whatever the reader does not know is
skipped with an `fseek`; fields that are not on disk keep the constructor
value. That is how one binary loads three generations whose rows grew by
appended fields, and it is why

* the real discriminator is the **row length**, not the version, and
* version gates are float compares of `major + minor·0.01` (for example the GAME
  gate `> 11.19` at `0x5E29B3`, constant `0x670538`), which the crate expresses
  as `Version::new(major, minor)` comparisons.

The crate follows this. A reader takes fields in order while they fit, whatever
the version, so a short row leaves later fields at their defaults (the
constructor's, not zero) and trailing bytes it does not know land in the
record's `extra` (every record type has one; a test appends three stray bytes to
a row of each type and checks they survive). A writer emits the row at the
layout of the file's own version, via `since (major, minor) { … }` field groups.
That reproduces every shipped row exactly; a synthetic row whose length
disagrees with its version is read tolerantly but normalised to its version's
layout on write. `GAME`, whose short rows are the common case in PTW files,
instead remembers how many fields it had (`fields_present`).

Where a group's boundary comes from, in order of preference:

1. the loader's own version tests and "Setting up data added in …" messages
   (2.3): `TERR` (3.09, 3.13, 11.10, 11.12, 12.00, 12.02, 12.03), `DIFF` (3.06),
   `UNIT` (11.07), `GAME` (> 11.19), `PRTO` (< 10.0 action translation);
2. the editor's release notes (2.4), which name the file version of each editor
   release: `PRTO` (11.00, 11.05, 11.09, 11.11), `RULE` (11.07), `DIFF` (11.10),
   `UNIT` (11.08), `CITY` (3.17);
3. failing both, the lowest version in the corpus whose rows are longer. This is
   the case for everything Conquests added to `BLDG`, `CTZN`, `ERAS`, `GOVT`,
   `TECH`, `LEAD` and `PRTO`: the Conquests editor's notes list features
   without version numbers, so the gate is `12.06`, the first Conquests file
   we have. A `12.00`–`12.05` file (none known) would be read with its extra
   bytes kept and re-encoded at its own length, but written back with the
   `11.x` shape for those sections.

### 2.2 Row lengths by generation (body bytes, corpus)

| tag | `BIC ` ≤ 4.01 | PTW 11.x | Conquests 12.x |
|---|---|---|---|
| `VER#` | 720 | 720 | 720 |
| `BLDG` | 252 | 252 | 268 (adds `flavors`, `revision`, `unit_produced`, `unit_frequency`) |
| `CTZN` | 116 | 116 | 124 (adds `corruption`, `construction`) |
| `CULT` | 88 | 88 | 88 |
| `DIFF` | 116 | 120 (adds `corruption_modifier`) | 120 |
| `ERAS` | 260 | 260 | 264 |
| `ESPN` | 232 | 232 | 232 |
| `EXPR` | 40 | 40 | 40 |
| `FLAV` | – | – | 292 per flavor (own framing) |
| `GOOD` | 88 | 88 | 88 |
| `GOVT` | 536 | 536, 572, 620 | 508 – 568 (`396 + 12·n + tail`; tail 68 B ≤ PTW, 76 B from 12.06) |
| `RULE` | 712 | 716 | 684, 720 |
| `PRTO` | 164 | 204 | 255 – 623 (`255 + 4 per list entry`) |
| `RACE` | 2 880 – 4 224 | 2 788 – 4 332 | 2 708 – 4 548 (variable lists, see 3.4) |
| `TECH` | 104 | 104 | 112 (two dwords inserted **before** `flags`) |
| `TFRM` | 112 | 112 | 112 |
| `TERR` | 113 | 119 – 123 | 232, 233 |
| `WSIZ` | 80 | 80 | 80 |
| `WCHR` | 52 | 52 | 52 |
| `WMAP` | `168 + 4·goods` | same | same |
| `TILE` | 22, 23 | 29 | 45, 49 |
| `CONT` | 8 | 8 | 8 |
| `SLOC` | 16 | 16 | 16 |
| `CITY` | – | `66 + 4·n` | `66 + 4·n` (n = starting buildings) |
| `UNIT` | – | 121 | 121 |
| `CLNY` | – | 20 | 20 |
| `GAME` | 16 | 32 – 5 440 | 7 353 – 7 581 (see 3.7) |
| `LEAD` | – | 84, 100 | 93 – 357 |

### 2.3 What the loader does for old files (exe log strings)

The loader prints one line per fix-up it applies to a file older than the
version that introduced the data. These are *fix-ups*, not size gates (the row
length decides what is read); they are useful as a version history:

| loader | message (`Setting up data …`) | known effect |
|---|---|---|
| `loadCULT` | removed in v4.00 | files < 4.0 carry eight extra bytes before the row count (`0x594BAD`, `0x594EA4`) |
| `loadGAME` | added in v12.08 | sets rule bit `0x40000` for every older file (3.7) |
| `loadBLDG` | removed in v11.18; added in v2.08 | version gate 2.08 (constant `0x66E9D4`) and the Center-of-Empire fix-up (3.4); the 11.18 removal is not traced |
| `loadDIFF` | added in v3.06, v2.07, v2.06 | not traced |
| `loadESPN` | added in v2.10 | files < 2.10 get default costs on the first three missions (`0x5959C8`) |
| `loadPRTO` | added in v11.05, v11.03, v11.01, v11.00, v2.11; "Translating Civ3 unit actions to Civ3X unit actions" | < 10.0: unit actions translated (`0x5E5AB0`); < 2.11: command mask recomputed (`0x595D5B`..`0x595F2A`); `revision` < 7 multiplies the shield cost by 10 (`0x5E5A80`) |
| `loadRULE` | added in v2.09 and v.12.05 [sic] | not traced |
| `loadTERR` | added in v11.12, v11.10, v3.13, v3.09 | 12-row (`BIC `) or 14-row layout accepted (`0x59692B`) |
| `loadTFRM` | added in v11.15 | not traced; the log sits next to the `TFRM_Outpost`, `TFRM_Radar_Tower`, `TFRM_Airfield` strings (HYPOTHESIS: those three jobs) |
| `loadUNIT` | added in v11.07 | files < 11.07 move the legacy name into `custom_name` (`0x596E96`) |

## 3. The sections

### 3.1 Inventory

Stock rows are those of `conquests.biq`. "arm" is the dispatcher case in
`0x594290`, "reader"/"writer" the row functions. Field tables are in the module
named in the last column (`biq/src/sections/<module>.rs`).

| tag | layer | stock rows | arm | reader / writer | module |
|---|---|--:|---|---|---|
| `VER#` | header | 1 | `0x594487` | `0x5E8180` / – | `ver` |
| `BLDG` | rules | 83 | `0x5947D7` | `0x5DFF40` / `0x5DFAE0` | `bldg` |
| `CTZN` | rules | 6 | `0x594864` | `0x5E06D0` / `0x5E05B0` | `ctzn` |
| `CULT` | rules | 6 | `0x594BAD` | `0x5E1180` / `0x5E1070` | `cult` |
| `DIFF` | rules | 8 | `0x5947FB` | `0x5E1460` / `0x5E1290` | `diff` |
| `ERAS` | rules | 4 | `0x594AFC` | `0x5E1790` / `0x5E16A0` | `eras` |
| `ESPN` | rules | 9 | `0x5948FB` | `0x5E1950` / `0x5E1860` | `espn` |
| `EXPR` | rules | 4 | `0x594991` | `0x5E1AD0` / `0x5E1A20` | `expr` |
| `FLAV` | rules | 7 flavors | (own) | `0x52D2C0` / `0x52D530` | `flav` |
| `GOOD` | rules | 26 | `0x59454B` | `0x5E3860` / `0x5E3740` | `good` |
| `GOVT` | rules | 8 | `0x594CFD` | `0x5E3E80` / `0x5E3B20` | `govt` |
| `RULE` | rules | 1 | `0x5945DF` | `0x5E78E0` / `0x5E7320` | `rule` |
| `PRTO` | rules | 141 | `0x594399` | `0x5E54B0` / `0x5E4F20` | `prto` |
| `RACE` | rules | 32 | `0x594632` | `0x5E6300` / `0x5E5EE0` | `race` |
| `TECH` | rules | 83 | `0x594742` | `0x5E85B0` / `0x5E8440` | `tech` |
| `TFRM` | rules | 13 | `0x594824` | `0x5E88F0` / `0x5E8800` | `tfrm` |
| `TERR` | rules | 14 | `0x594A97` | `0x5E9300` / `0x5E8EF0` | `terr` |
| `WSIZ` | rules | 5 | `0x594DD7` | `0x5F7390` / `0x5F7270` | `wsiz` |
| `WCHR` | map | 1 | `0x594A3E` | `0x5EB1E0` / `0x5EB040` | `wchr` |
| `WMAP` | map | 1 | `0x594A68` | `0x5F41B0` / `0x5F3FD0` | `wmap` |
| `TILE` | map | `(W/2)·H` | `0x594608` | `0x5EA1F0` / `0x5EA030` | `tile` |
| `CONT` | map | per continent | `0x594D26` | `0x5E0FE0` / `0x5E0F50` | `cont` |
| `SLOC` | map | per start | `0x594412` | `0x5E8290` / `0x5E8330` | `sloc` |
| `CITY` | scenario | per city | `0x594E09` | `0x5E0870` / `0x5E0AB0` | `city` |
| `UNIT` | scenario | per unit | `0x594AD3` | `0x5EADA0` / `0x5EAF00` | `unit` |
| `CLNY` | scenario | per colony | `0x594D50` | `0x5E0D10` / `0x5E0DD0` | `clny` |
| `GAME` | scenario | 1 | `0x5946D2` | `0x5E26E0` / `0x5E2F90` | `game` |
| `LEAD` | scenario | per player | (worker `0x595B80`) | `0x5E4490` / `0x5E47B0` | `lead` |

The section workers (`0x594FF0` `BLDG`, `0x5952C0` `DIFF`, `0x595C40` `PRTO`,
`0x596490` `TERR`, `0x596CE0` `TILE`, `0x596DE0` `UNIT`, …) own the post-read
fix-ups of section 4.

### 3.2 Cross-references

Every index field is `-1` for "none" unless noted; bit-mask fields index by
bit. The targets come from the editor's controls (B) and the engine's use (A);
for `BLDG`, `UNIT`, `LEAD`, `WCHR` and `TILE` corpus tests additionally check
every index against its target table (`corpus_fields_index_their_tables`,
`corpus_values_are_in_editor_range`, `world_size_index_matches_wsiz_row`,
`golden_rise_of_rome_city_indices_on_tiles`).

| source | field | target |
|---|---|---|
| `BLDG` | `doubles_happiness_of`, `gain_in_every_city`, `gain_in_every_city_on_continent`, `required_improvement` | `BLDG` |
| `BLDG` | `required_government` / `required_advance`, `rendered_obsolete_by` | `GOVT` / `TECH` |
| `BLDG` | `required_resource_1`, `required_resource_2` | `GOOD` |
| `BLDG` | `unit_produced` (12.06+) / `flavors` (bit mask) | `PRTO` / `FLAV` |
| `PRTO` | `required_tech` / `upgrade_to` / `alt_strategy_of` | `TECH` / `PRTO` / `PRTO` |
| `PRTO` | `required_resource_1..3` | `GOOD` |
| `PRTO` | `available_to_civs` (bit `i` = `RACE` row `i`; `-2` = all but the barbarians) | `RACE` |
| `PRTO` | `ignore_move_cost` (one byte per row), `legal_unit_telepads`, `stealth_attack_targets`, `enslave_results_in`, `legal_building_telepads` | `TERR`, `PRTO`, `PRTO`, `PRTO`, `BLDG` |
| `TECH` | prerequisites / `era` | `TECH` / `ERAS` |
| `GOOD` | prerequisite tech | `TECH` |
| `TERR` | resource mask (bit `i` = `GOOD` row `i`) | `GOOD` |
| `RACE` | `shunned_government`, `favorite_government` / `free_techs` / `king_unit` / `flavors` (bit mask) | `GOVT` / `TECH` / `PRTO` / `FLAV` |
| `WCHR` | world size index | `WSIZ` |
| `WMAP` | `resource_count` (= row count) | `GOOD` |
| `TILE` | `resource`, terrain nibble, `city_id`, `colony_id`, `continent_id` | `GOOD`, `TERR`, `CITY`, `CLNY`, `CONT` |
| `CITY` | `starting_buildings` | `BLDG` |
| `UNIT` | `unit_type` / `experience_level` / `ai_strategy` (bit index) | `PRTO` / `EXPR` / `PRTO.ai_strategies` |
| `LEAD` | `civilization` / `government` / `difficulty` / `initial_era` / starting units / free techs | `RACE` / `GOVT` / `DIFF` / `ERAS` / `PRTO` / `TECH` |
| `GAME` | `playable_civilizations` | `RACE` |
| `UNIT` `CITY` `SLOC` `CLNY` | `(owner_type, owner)` | see 3.3 |

### 3.3 Ownership pairs

`UNIT`, `CITY`, `SLOC` and `CLNY` rows carry an `(owner_type, owner)` dword pair
(`biq/src/owner.rs`):

| `owner_type` | meaning | `owner` |
|--:|---|---|
| 0 | nobody (an unassigned start location) | unused |
| 1 | barbarian tribe | index into the barbarian race's city-name list (`RACE` row 0; 76 names, `75` is the constructor default) |
| 2 | civilization | `RACE` row index (the normal form in rule-set-driven scenarios) |
| 3 | player | `LEAD` row index, 0-based (the game uses `LEAD index + 1` as the player slot; slot 0 is the barbarians) |

Evidence: game start placement `0x5D2D4B..0x5D2E0F` (A); the swap-two-players
fix-up `0x599CF0` rewrites `owner` only where `owner_type == 3` (A); the editor's
*Select Active Player* dialog 192 and its placement errors (B); `TETurkhan.bix`
units use type 1 with owners `0..=75`, `Rise_of_Rome` type 2 (C).

### 3.4 Rules sections: what is not obvious from the field tables

* **`BLDG`.** The in-memory row starts with a "bytes left" dword, so memory
  offset = body offset + 4 (the same holds for most rows below). Only the first
  row with `improvement_flags` bit 0 (*Center of Empire*) keeps it; the loader
  clamps `spaceship_part < -1` to `-1` and stamps `revision = 4` after migrating
  older rows (`0x5E0489..0x5E0519`).
* **`CULT`.** File order is opinion, two ratio spins, propaganda %, initial and
  continued resistance %, one tail dword (always `continued − 10`, **D**).
  *Border Factor* and *Level Multiplier* are `RULE` fields, not `CULT`.
* **`ESPN`.** There is no *Mission Performed By* field in the row (the reader
  ends at `base_cost`, `0x5E19F9`); community layouts that add one describe a
  different build.
* **`FLAV`.** See section 1 for the framing; civilizations pick their flavors in
  `RACE`, advances and buildings carry flavor masks.
* **`GOVT`.** On-disk order is the writer's `fwrite` order, not the memory
  layout. The row stores the government count `n` and then `n` relation triples
  (can-bribe, bribery modifier, resistance modifier toward each government),
  which is why its length grows by 12 bytes per government.
* **`RULE`.** A single row. The game's reader stops after the combat scalars
  (about 304 bytes of a 720-byte row); the culture-level labels (six 64-byte
  entries: name + 32 zero bytes) and the trailing eight dwords
  `{1000, 10, 400, 20, 50, 4, 76, 3}` stay in the file and are kept by the crate.
  `Movement Rate Along Roads` is body `+0x100` (3 in stock rules); the border
  factor, hurry values, fortification bonus, starting treasury and the city and
  town size limits that `CITY` uses are here too.
* **`PRTO`.** Layout table in the module docs. Body `+0x00` is the **Zone of
  Control** flag (the editor reads it from the first body dword, `0x45E2B2`;
  16 stock rows) — not an ability bit. Rows without `revision` (Civ3 1.x, PTW)
  store `shield_cost` in tens of shields; `UnitType::shield_cost_in_shields`
  applies the loader's rule. `ignore_move_cost` has one byte per `TERR` row (12
  in a PTW row, 14 in a Conquests row). Rows with `alt_strategy_of != -1` are
  the second strategy of a unit; the loader counts only the `-1` rows as real
  unit types (`0x595CD6`).
* **`RACE`.** `[u32 n][n × char[24]]` city names, `[u32 n][n × char[32]]` great
  leaders, then `leader_name[32]`, `title[24]`, `civilopedia_entry[32]`, and the
  three 40-byte strings in the order **adjective, singular name, plural noun**,
  then `0`, `4` or `8` era-art paths of 260 bytes, then the tail of dwords
  (table in `biq/src/sections/race.rs`; `t` = offset inside the tail, memory
  offset in brackets):
  culture group `0x00`, leader gender, civ gender, aggression, civ index (the
  row's own number; 0 marks the Barbarians), shunned and favorite government,
  default color `0x1C` and alternate ("unique") color `0x20` (both read by the
  player colour allocator around `0x5A16C2`), **four free techs** `0x24..0x30`
  (`TECH` index, −1 none; getter `0x53A0A0`), the **trait mask** `0x34`
  (`+0x948`; bit 0 Militaristic, 1 Commercial, 2 Expansionist, 3 Scientific,
  4 Religious, 5 Industrious, 6 Agricultural, 7 Seafaring; tested by
  `RACE.vtable[0]` `0x53A080`, assembled by the editor from eight check boxes),
  the AI **governor settings** `0x38` (7 bits, constructor default `0x11`),
  **build never** `0x3C` and **build often** `0x40` (15 bits each, copied to
  the player object at `0x4BCF89`..`0x4BCFD9`), plurality `0x44` (the 72-byte
  Civ3 1.x tail ends here), the **king unit** `0x48` (`PRTO` index, PTW and
  later; used by `use_civ_king_unit`, `0x5D2E80`) and the Conquests block:
  **flavors** `0x4C` (7-bit mask, `FLAV` rows), a revision dword `0x50`
  (`0` = old layout, then the loader moves trait-mask bits 8–14 into the
  flavors and stores 3, `0x5E6892`; every shipped Conquests row holds 2), the
  **diplomacy text index** `0x54` (−1 = use the civ's own index − 1, `0x515557`)
  and `scientific_leader_count` plus that many 32-byte names (`16 + 32·n` bytes
  for the whole block; every shipped Conquests row has 1 ≤ n ≤ 10).
* **`TECH`.** The two Conquests dwords sit **before** `flags` on disk, they are
  not appended.
* **`TERR`.** Head `[u32 goods_count][resource mask][name 32][TERR_* key 32]`
  with a mask of `max(4, ⌈goods_count/8⌉)` bytes, then a tail of 41 (Civ3 1.x),
  47–48 (PTW) or 160–161 (Conquests) bytes; the extra Conquests byte is an
  optional `0xCC` separator before the landmark key and the disease block, not
  a row-length field. The loader accepts exactly **12** (`BIC `) or **14**
  (Conquests: Marsh and Volcano added) rows.
* **`DIFF`, `ERAS`, `CTZN`.** Field order is the reader's, not the dialog's
  visual grouping; `ERAS` holds up to five 32-character researcher titles.

### 3.5 Map sections

* **`WCHR`** (13 dwords): the six generator sliders (climate, barbarians,
  landform, oceans, temperature, age), each stored as *selected* then
  *resolved* — when the player chose *Random* the selected value is `3` and the
  resolved one holds the roll (C: `islands.bic`, `Intro2`) — and the `WSIZ`
  preset.
* **`WMAP`** (`168 + 4·goods`): `resource_count` and that many resource rolls;
  then `land_continent_count` (equals the number of `CONT` rows with
  `in_use == 1`, 92 of 92 files), `height`, `start_site_radius`,
  `number_of_players` (the editor's *Number of Players*; equals the `LEAD` row
  count when both exist, 25 of 25), `cell_count_isqrt`, one always-zero dword,
  `width`, 32 start-candidate seeds, `water_level`, `wrap_flags`. The `TILE`
  row count must equal `(width/2)·height`.
* **Coordinates.** Maps are staggered: only `(x, y)` with `x + y` even exist,
  `index = (width/2)·y + x/2`, inverse `y = index / (width/2)`,
  `x = 2·(index % (width/2)) + (y & 1)` (`0x5DC1C0`, `0x5EB7D0`). `TILE` rows
  are in index order. `Biq::map_view()` wraps this.
* **`TILE`** (22 – 49 bytes, a row is read through a `Cell` object):

  | body | cell | field |
  |---|---|---|
  | `+0x00` | `+0x04` | `river_connection_mask`: bit `d` = river towards the neighbour in direction `d` (N, NE, E, SE, S, SW, W, NW at `(0,-2) (1,-1) (2,0) (1,1) (0,2) (-1,1) (-2,0) (-1,-1)`); 98.3 % of land-land edges are mirrored, so do not assume the mirror |
  | `+0x01` | `+0x05` | `owner` (player slot, 0 = nobody; zeroed by the loader) |
  | `+0x02` | `+0x08` | `resource` (`GOOD` index, `-1` none) |
  | `+0x06`, `+0x07` | `+0x10`, `+0x11` | terrain art: cell in the sheet, sheet id |
  | `+0x0A` | `+0x14` | packed dword (Civ3 1.x / PTW encoding of overlays, terrain, features) |
  | `+0x0E`..`+0x14` | `+0x18`..`+0x1E` | barbarian tribe id, `city_id`, `colony_id`, `continent_id` (2 bytes each) |
  | `+0x16` | `+0x20` | water depth byte |
  | `+0x17`, `+0x19` | `+0x22`, `+0x24` | victory-point-location id, ruin id |
  | `+0x1D`, `+0x21`, `+0x25` | `+0x28`, `+0x2C`, `+0x30` | overlay plane, terrain word (terrain id is nibble 3), feature plane (Conquests encoding) |
  | `+0x29` | `+0x34` | flags (meaning open) |
  | `+0x2D` | – | four padding bytes the game never reads (the 49-byte rows of `Intro3_New_Alliances`) |

  Civ3 1.x / PTW rows carry the three planes packed into the dword at `+0x0A`;
  Conquests rows carry them expanded and leave the packed dword zero.
  `Cell::fixup` (`0x5EA410`) rebuilds the planes from the packed dword when the
  depth byte is below 4. Overlay bits: road `0`, railroad `1`, mine `2`,
  irrigation `3`, fortress `4`, goody hut `5`, pollution `6`, barbarian camp
  `7`, craters `8`, barricade `28`, airfield `29`, radar tower `30`, outpost
  `31`. Feature bits: bonus grassland `16`, start location `19`, snow-capped
  `20`, pine forest `21`, river corner marks `24..27`. Full constants with
  addresses are in `tile::{overlay, feature, river_connection}`.
* **`CONT`** (8 bytes): `in_use` (1 = land continent: every tile is land; 0 = water
  body, 184 of which also hold stray land tiles) and `tile_count`. Row index =
  the tile's `continent_id`. The game renumbers continents in `finalizeMap`; the
  file values are caches.
* **`SLOC`** (16 bytes): `owner_type`, `owner`, `x`, `y`. Type 2 + `RACE` index
  is the normal form; 27 rule-less maps carry only type-0 (unassigned) starts.

### 3.6 Scenario sections

* **`CITY`** (`66 + 4n`): `has_walls`, `has_palace` (bytes), `name[24]`,
  `owner_type`, `n` + `n` `BLDG` indices, `culture`, `owner`, `size`, `x`, `y`,
  `city_level`, `border_level`, `use_auto_name`. The editor derives `has_palace`
  (any starting building with the Center-of-Empire flag), `has_walls` (any with a
  positive `bombard_defense`) and `city_level` (`2` above `RULE.largest_city_size`,
  `1` above `largest_town_size`); all 1 712 shipped cities satisfy these.
* **`UNIT`** (121 bytes, PTW and later): `legacy_name[32]` (zero in all 5 877
  rows), `owner_type`, `experience_level`, `owner`, `unit_type`, `ai_strategy`
  (`-1` or the bit index tested against the `PRTO` strategy masks of the type and
  its variants), `x`, `y`, `custom_name[57]`, `use_civ_king_unit` (replaces the
  type by the owner civilization's `king_unit`). The strategy-variant resolution
  of game start (`0x5D2E96..0x5D2F8D`) is `Unit::instantiated_types`.
* **`CLNY`** (20 bytes): `owner_type`, `owner`, `x`, `y`, `kind` (colony,
  airfield, radar tower, outpost). The tile under a colony has `colony_id` = the
  row index and the kind as an overlay bit; the loader clears `colony_id` while
  reading `TILE`, so the link is rebuilt from these rows.
* **`LEAD`** (84 – 357 bytes): one row per player slot: `custom_civ_data`
  (= *not* "Civilization Defaults"), `human_player`, `leader_name[40]`, starting
  units (`n` × `{count, unit}`), `gender`, free techs, `difficulty` (raw index in
  PTW files), `initial_era`, `starting_treasury`, `government`, `civilization`
  (`RACE` index or "any"/"random"), `team_color`, then from 12.06
  `skip_first_turn`, an unused dword and `start_embassies`. The first human row
  becomes the human slot; if none, row 0 is forced to human.

### 3.7 `GAME`: the scenario-wide settings (fully decoded)

The scenario manager (instance `0x9C3508`) embeds the game object at `+0xBC8` =
**`0x9C40D0`**; call it `G`. The editor keeps the same record at the same
offsets (`G = S+0x5C` in the Scenario-page save `0x44A5B0`), and the engine
addresses fields as absolute operands (`G+0x7C` is `[0x9C414C]`), which is how
the consumers were found. Constructor `0x5E2450` (the defaults), reader `0x5E26E0`
(called from `0x5946D2`), writer `0x5E2F90` (called from `0x597951`), list
allocator `0x5E3610`. Editor pages: Scenario (190), Locked Alliances (207),
Victory Point Limits (208), Disasters! (209). The Crop Map page (210) is not
stored.

The row is a **56-field walk** (the shared row rule applies, so shorter rows end
at an earlier field). Field 3 is a count `N` followed by `N` `RACE` indices;
`N` also sizes field 19.

| # | `G+` | bytes | field | default |
|--:|---|--:|---|---|
| 1 | `0x00` | 4 | `use_default_game_rules` | 1 |
| 2 | `0x04` | 4 | `use_default_victory_conditions` | 1 |
| 3 | `0x10` | 4 + 4N | `N`, then `N` × `RACE` index (playable civilizations) | – |
| 4 | `0x14` | 4 | `rules_flags` (below) | 0 |
| 5–7 | `0x18 0x1C 0x20` | 4 each | auto-place capture units / king units / victory locations | 1 |
| 8 | `0x24` | 4 | `debug_mode` | 0 |
| 9 | `0x28` | 4 | `use_time_limit` | 0 |
| 10 | `0x2C` | 4 | `base_unit_of_time` (0 years, 1 months, 2 weeks) | 0 |
| 11–13 | `0x30 0x34 0x38` | 4 each | start month, start week, start year | 1, 1, −4000 |
| 14–15 | `0x3C 0x40` | 4 each | time limit minutes, turns | 0, 540 |
| 16 | `0x44` | 28 | `time_scale_turns[7]` | 25 25 40 50 100 100 100 |
| 17 | `0x60` | 28 | `time_scale_units[7]` | 50 40 25 20 10 5 2 |
| 18 | `0xB4` | 5200 | `search_folders` (semicolon-separated) | empty |
| | | | *end of a PTW row: `5316 + 4N` bytes* | |
| 19 | – | 4N | `N` × alliance number (**version > 11.19**) | 0 |
| 19′ | → `0x1A71` | 4 | legacy reveal-map dword (**version ≤ 11.19**; the row then ends; none shipped) | 0 |
| 20–31 | `0x7C`–`0xA8` | 4 each | victory limits: VP limit 50000, city elimination count 1, culture 1 city 20000, culture civ 100000, % terrain 66, % population 66, wonder ×10, defeat unit ×10, advancement ×5, city conquest ×100, VP scoring 25, special unit 1000 | as listed |
| 32 | `0xB0` | 4 | `theme` (1 = Fantasy) | 0 |
| 33 | `0x1A72` | 1 | runtime flag, always 0 in files | 0 |
| 34 | `0x1504` | 1280 | `alliance_names` 5 × 256 | empty |
| 35 | `0x1A04` | 100 | `alliance_war` 5 × 5 dwords | 0 |
| 36 | `0x1A68` | 4 | `alliance_victory_type` (0 individual, 1 coalition) | 0 |
| 37 | `0x1A73` | 260 | `plague_name` | `Black Death` |
| 38 | `0x1B77` | 1 | `permit_plagues` | 0 |
| 39–44 | `0x1B78`–`0x1B8C` | 4 each | plague earliest start, variance, duration, strength, grace period, max occurrences | 0 0 0 0 1 1 |
| 45 | `0x1B94` | 4 | `campaign_record` | 0 |
| 46 | `0x1B98` | 260 | unknown string | `Unknown` |
| 47 | `0x1A6C` | 4 | respawn flag unit on capture | 1 |
| 48 | `0x1A70` | 1 | allow anyone to capture any flag | 0 |
| 49 | `0xAC` | 4 | gold for capture | 0 |
| 50 | `0x1A71` | 1 | reveal entire map | 0 |
| 51 | `0x1C9C` | 1 | retain culture on capture | 0 |
| 52 | `0x1B90` | 4 | plague scheduler state | −1 |
| 53 | `0x1CA0` | 4 | volcano max eruption period | 5000 |
| 54–56 | `0x30F4 0x30F8 0x30FC` | 4 each | multiplayer timer: base, per city, per unit | 24, 3, 1 |

Row sizes: Civ3 1.x 16 bytes (fields 1–4); the oldest PTW rows 32, 124 or 128
bytes (cut after a dword); PTW 11.18 `5316 + 4N`; Conquests `7333 + 8N` (all 56
fields) or `7333 + 8N − 12` (no multiplayer timers, fields 1–53). Check:
`5316 + 4N + 4N + 2017 = 7333 + 8N`. `Game::fields_present` counts how many
leading fields a row had, which is what makes every short row round-trip.

**Flags (`rules_flags`).** One word, two groups the engine separates:

| group | bits (editor label) |
|---|---|
| victory conditions, mask `0x1001F` (`0x585913`) | `0x1` Domination, `0x2` Space Race, `0x4` Diplomatic, `0x8` Conquest, `0x10` Cultural, `0x10000` Wonder |
| game rules, mask `0x6FFE0` (`0x585932`) | `0x40` Culturally Linked Start, `0x80` Respawn AI Players, `0x100` Preserve Random Seed, `0x200` Accelerated Production, `0x400` City Elimination, `0x800` Regicide, `0x1000` Regicide (all Kings), `0x2000` Victory Point Scoring, `0x4000` Capture the Unit, `0x8000` Allow Cultural Conversions, `0x20000` Reverse Capture the Flag |
| unlabelled, inside the rules mask | `0x20` (legacy bit 5, in four PTW files); `0x40000` (set by the loader for every file older than 12.08; its only runtime test, `0x561C0E`, gates a 3 % / 5 % roll that creates a Leader unit and posts `NEWSCILEADER`: HYPOTHESIS "scientific leaders") |

**How the engine merges them** (A `0x5858D0..0x585939`; `custom` is the word the
player chose in the custom-game screen, `[0xA52B7C]`): if both `use_default_*`
are set the result is `custom & !0x20000`; otherwise the victory bits come from
`custom` when `use_default_victory_conditions` is set and from the scenario
(plus `0x40000` for pre-12.08 files) otherwise, and the rule bits likewise with
`use_default_game_rules`. The editor writes a flag only when its `use_default_*`
is 0. (`Game::engine_flags`.)

**Other semantics (A unless noted).**

* `theme == 1` switches the background music to the `Fantasy\Fantasy Mix` tables
  and seeds the live search path (`G+0x1CA4`, a 5200-byte runtime buffer that is
  not in the file) with `Fantasy` (`0x5361F7`, `0x536263`, `0x536413`,
  `0x5987B4`). No editor control; 0 in every shipped file.
* Calendar: `base_unit_of_time`, the start date and the two time-scale arrays
  feed the date code `0x5DF03C..0x5DF76D`. Step `i` lasts `time_scale_turns[i]`
  turns of `time_scale_units[i]` base units; the last step runs on. *WWII in the
  Pacific* (C): months, December 1941, 300 turns × 1 unit. `time_limit_turns` 0
  means 540; the engine clamps to 1..=1000 (`0x581800..0x581890`).
* `alliance_*`: `PlayableCiv.alliance` is `0` (none) or `1..=4`; the war matrix
  is symmetric in every shipped file. `Intro3` locks `[1, 2, 2, 1]` with
  coalition victory; *Rise of Rome* (N = 8) `[1, 0, 3, 0, 0, 4, 0, 2]` with
  wars 1↔2 and 3↔4 (C).
* Disasters: the plague routine returns at once unless `permit_plagues` is set
  (`0x4F5265`), stops at `max_occurrences` (`0x4F5285`) and, while the scheduler
  state is `-1`, stores a random number below `variance` (`0x4F52CD`). Stock
  values: `Plague`, −700, 5, 100, 80, 1000, 3; *Middle Ages* starts in 1346.
  Volcano period read at `0x4F478B`.
* `campaign_record` is `1` in the nine non-multiplayer Conquests campaign
  scenarios (0 in the intros and multiplayer files); at the end of the human
  player's game the engine hands it with the scenario name and score to the
  campaign-record writer (`0x4F16FA → 0x4A97A0`; strings `CAMPAIGN_RECORD`,
  `GMRC`, `CMRC`, `art\interface\campRec.pcx`). The editor zeroes it on save.
* Multiplayer timer: `base + per_city·cities + per_unit·units`, scaled by game
  speed (`0x467FC8..0x468027`, `0x468499..0x4684FA`); the unit is presumably
  seconds (HYPOTHESIS). `MPTournament` stores 16, 2, 1.
* `search_folders` examples (C): `New Alliances`; `..\Extras\Medieval Japan`.
* The twelve victory limits (fields 20–31) are copied into the game state at
  `0x585945..0x5859C9` (A). The three fields of the same editor page that sit
  elsewhere in the row (47–49) are read where they are used; for example
  `0x5B7FF3` compares the respawn flag with 1.

## 4. What the game does after reading a row

These are observable in the loaded game state, not in the file; a writer that
wants to mimic a "normalised" file has to apply them.

| section | behaviour |
|---|---|
| `VER#` | majors outside `2..=12` are rejected |
| `GAME` | pre-12.08 files get rule bit `0x40000`; `use_default_*` decide whose rules apply (3.7) |
| `BLDG` | only the first Center-of-Empire row keeps the bit; `spaceship_part < -1 → -1`; rows are migrated by `revision` and stamped `4` |
| `PRTO` | `shield_cost × 10` below revision 7, then `revision = 7`; old unit actions translated; `alt_strategy_of == -1` rows counted as the unit types |
| `TERR` | only 12- or 14-row sections load |
| `TILE` | after reading, city id, colony id, the unsaved `Cell+0x0C` id and the owner are cleared (rebuilt from `CITY`, `CLNY` and culture); packed planes are expanded, and `Cell::fixup` (`0x5EA410`) also normalises water depth (terrain classes 11/10/9 forced to 13/12/11, `Cell+0x34` cleared; conditions in `NOTES.md` §13) |
| `CONT` | continents are renumbered by `finalizeMap` |
| `LEAD` | first `human_player` row is the human; row 0 forced human if none |
| `UNIT` | game start shuffles the list, resolves the owner by `owner_type`, skips inactive owners, and creates one of the type's strategy variants |

## 5. Using the crate

```rust
use civ3_biq::Biq;

let biq = Biq::read_file("2 Rise of Rome.biq")?;   // DCL or plain
println!("{} {}", biq.version(), biq.header.as_ref().unwrap().title);
for unit in &biq.rules.unit_types { /* typed PRTO rows */ }
let tile = biq.map_view().and_then(|m| m.tile(40, 40));
assert_eq!(biq.unmodelled_bytes(), 0);
let plain = biq.to_stream();                        // byte-exact re-encode, no wrapper
let file = biq.to_bytes()?;                         // byte-exact, wrapper as read
biq.write_file("copy.biq")?;                        // the same, to disk
```

`cargo run --release --example dump -- FILE [--check|--debug|--map]` prints an
overview (row counts per section, rules and map summary), re-checks each
section's round trip, dumps the model, or draws the terrain; the `unpack`
example decompresses every scenario and save under the given roots into
`/tmp/biq/` for ad-hoc analysis. Tests: `CIV3_DIR=…/civ3 cargo test --release`
inside `biq/`. The codec is usable on its own: `civ3_biq::dcl::decompress`,
`civ3_biq::implode::compress(bytes, dcl::Mode::Binary, 6)` and
`compress_like_the_game`.

## 6. Corrections to earlier readings

Where earlier notes, early drafts of the crate, or community documentation
disagreed, the exe and the corpus decided:

| earlier reading | now |
|---|---|
| `WMAP +0x150` is a player count, `+0x15C` an "area factor" | `+0x150` = land continents (`CONT` rows with `in_use == 1`); `+0x15C` = *Number of Players* |
| `CONT.in_use` is "primary/secondary continent" | 1 = pure land continent, 0 = water body (184 rows also hold land tiles) |
| `UNIT` body `+36` is the owner, `+44`/`+48` hit points (early crate draft) | `+32` owner type, `+36` experience level, `+40` owner, `+44` `PRTO` index, `+48` AI strategy |
| community `CULT` lists put propaganda or one "percentage" dword first | not so: row 0 propaganda is 1, not 30; real order in 3.4 |
| community `ESPN` layouts end with a performed-by dword | the reader ends at `base_cost` |
| Zone of Control is an ability bit (`editor.md`, `workers.md`) | the first `PRTO` body dword |
| the `RULE` offset of the road multiplier is unknown (`workers.md`) | body `+0x100` (`movement_rate_along_roads`) |
| the DCL compressor is "unused by scenario load", so a file cannot be written back compressed (`biq.md`, earlier crate docs) | unused by *load*, but the game uses it for saved games and it is ported (`biq/src/implode.rs`); it reproduces every shipped compressed file exactly |
| mode-1 (ASCII-literal) DCL streams are unsupported (`Mode1Unsupported`; `rust/src/dcl.rs` still `Mode1Unverified`) | decoded and encoded; the literal tables are checked against both copies in the exe and form a complete prefix code. No shipped file uses mode 1, so the mode has round-trip tests only |
| `FLAV` is "not count/row framed, so the walk ends there" (`biq.md`) | own framing (section 1); not necessarily the last section |
| `GAME` split into separate scenario, alliance and victory-point tables (early crate draft) | one 56-field walk with two layouts split at version 11.19; the `use_default_*` flags decide whose rules apply (3.7) |
| `RACE`: "three 40-byte noun/adjective strings" (`biq.md`) | adjective, singular name, plural noun, in that order |
| `RACE` Conquests tail `124 + 32·n` bytes (early crate docs) | `92 + 32·n` with `n` = `scientific_leader_count` (1 ≤ n ≤ 10 shipped) |
| `RACE` tail from `0x24` on: two "starting bonus" slots, governor and build-never masks, "unique unit" (Romans 3 = Legion) and "unique building" (17), plurality, diplomacy text index, king unit, then a "free tech" (early crate draft) | the dword positions were right, the meanings were not (the labels came from community notes and guesses). The editor's page ↔ row copy (`0x4441xx..0x444AB3`, controls of dialog 169) and the reader give: four **free techs** (the "bonus" values 5/2 are *Warrior Code*/*Alphabet*), the **trait mask** (the "unique unit"; Romans 3 = Militaristic + Commercial), **governor settings** (the "unique building", constant 17 = default), **build never**, **build often** (the "diplomacy text index" 387…), **plurality** (the "king unit", 1), the real **king unit** (Romans 82 = *Caesar*), flavors, a revision dword and the real diplomacy text index. The trait-mask correction was first spotted in `capture.md` §11 |

## 7. Open items

Left as documented unknowns; every one is stored and written back verbatim, so
none blocks round-tripping.

| where | what |
|---|---|
| `GAME` | exact meaning of rule bits `0x20` and `0x40000` (name hypothesis only); `unknown_1b98` (default `Unknown`, no reader); the mode global `[0x990394]` behind `G+0x1A72`; units of the plague parameters and of the multiplayer timer |
| `RULE` | `unknown_0xfc` (hurry gate vs a `PRTO` type; stock `{1}`); the trailing eight dwords `{1000,10,400,20,50,4,76,3}` (neither the game reader nor the editor labels them) |
| `WMAP` | `unknown_0x164` (always 0); `start_candidate_seeds` are generator state |
| `TILE` | `unknown_0x08/09` (always 0), feature bits 15 and 29, which bit is which river corner (`24..=27`), `Cell+0x34`, tile owner `33 + player id` in 15 shipped files |
| `BLDG` | flag bits 26 and 28; exact semantics of `bombard_defense`, `air_power`, `naval_power` |
| `PRTO` | bit meanings of `command_mask` (derived data); `ignore_move_cost` bytes 12–13 are conversion residue (HYPOTHESIS) |
| `RACE` | upper bits of `flavors` (shipped rows of some scenarios carry the debug-fill pattern `0xCCCCCC..`); what the revision dword `2` (editor) versus `3` (loader) distinguishes; `culture_group` value names |
| padding-like fields | `VER#` `reserved_a/b`, `CULT` `unknown_0x54`, `ERAS` `unknown_0x104`, `WSIZ` `reserved_0x0c`, `TERR` `unknown_0xec` and pads, `LEAD` `unused_0x64` |
| loader history | what `loadDIFF`, `loadRULE`, `loadTERR`, `loadTFRM` fill for old versions. Not modelled here: `BICQ` as a standalone file (the loader accepts it, no shipped file has it; saves embed one, which `Save::embedded_biq` reads). The saved-game stream behind magic `CIV3` is in [`savegame.md`](savegame.md). No byte-level oracle for DCL mode 1 or window bits 4/5: none ship (see `biq.md`) |

## 8. Method

For each section: the exe's row **reader** gives the field order and sizes (each
`fread` is one field), the **writer** and the editor's page ↔ row copies
(control id → offset) give names, engine **consumers** give meaning (for `GAME`,
by scanning the disassembly for the absolute operand `0x9C40D0 + offset`), and
the corpus checks the result (index ranges, equalities such as `WMAP`
`number_of_players == |LEAD|`, byte-exact round trip of 119 files). Version
history was cross-checked against the loader's "Setting up data added in …"
strings and the observed row lengths. The scripts that drive radare2 and
Capstone are described in `.agents/skills/reverse-engineering-executables/`.
