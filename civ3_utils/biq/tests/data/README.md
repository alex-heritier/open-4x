# Scenario and save fixtures

Committed copies of two small Civ3 files, so the parsers can be exercised
without the git-ignored `civ3/` install (`../../src/corpus.rs` explains that
install-based corpus).

| file | source | md5 | bytes |
|---|---|---|---|
| `Intro3 New Alliances.biq` | `civ3/civ3-gog/app/Conquests/Conquests/Intro3 New Alliances.biq` | `39da0b49545ed60a7eea2afeecf2eaeb` | 39 715 |
| `TEST.SAV` | `civ3/civ3-gog/app/Conquests/Saves/TEST.SAV` | `0bdbd8e43b1f0b975d0cef6b55375dbb` | 65 182 |

The pair is matched: `TEST.SAV` is a turn-0 / 700 AD game of the *New Alliances:
Introductory Conquest 3* scenario (60x60, 4 players, 16 cities, 68 units), so the
save's embedded scenario names this `.biq`. It is **not** a byte copy of it — the
save embeds only the rules half, rewritten by the game as `BICQ` 12.08 (209 753 B,
no `TILE`/`CONT`/`CITY`/`UNIT`), while the shipped file is `BICX` 12.06 and
315 393 B decoded.

```text
cargo run --release --example dump -- tests/data/Intro3\ New\ Alliances.biq
cargo run --release --example sav  -- tests/data/TEST.SAV [--check]
```

`--check` round-trips the save byte-exactly (940 672 stream bytes, 65 182 file
bytes).
