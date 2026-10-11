# Scenario and save fixtures

Two small original files, so the parsers and the game can be exercised
without the git-ignored `civ3/` install (`../../src/corpus.rs` explains that
install-based corpus). Nothing in them comes from a Civ3 install: both are
written from nothing by `../../examples/make_fixtures.rs`.

| file | what | bytes |
|---|---|--:|
| `Lantern Isles.biq` | `BICX` 12.08 scenario, DCL-compressed: a full rule set, a 40x40 map, 4 players, 12 cities, 16 units | 6 986 |
| `LANTERN.SAV` | the same game at turn 20: grown cities, a fourth city each, roads, 25 units | 14 253 |

The pair is matched: the save embeds the whole `.biq` and names it as its
scenario. The rule set is small (14 units, 16 advances, 11 buildings, 4 eras,
4 civilizations plus the barbarians) and every name in it is original. The
tables the engine indexes by row keep the stock shape: 14 terrains, 13 worker
jobs, 8 governments, 4 eras and 4 experience levels. Resources use plain
commodity nouns (Horses, Iron, Wheat, ...) because the game matches those by
name. The art references (unit folders, leader clips, advance and building
pictures) are what `tools/make_stub_assets.py --biq` draws into `test-assets/`.

```text
cargo run --release --example make_fixtures            # rewrite both (deterministic)
cargo run --release --example dump -- "tests/data/Lantern Isles.biq"
cargo run --release --example sav  -- tests/data/LANTERN.SAV [--check]
```

`--check` round-trips the save byte-exactly (430 422 stream bytes, 14 253 file
bytes). After changing the generator, regenerate the art from the repo root:

```text
python3 tools/make_stub_assets.py --clean --biq "civ3_utils/biq/tests/data/Lantern Isles.biq"
```
