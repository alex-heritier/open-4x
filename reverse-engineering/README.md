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
| [`ai.md`](ai.md) | Game RNG pair, caller census, action gate, combat cluster, score jitter → `rust/src/ai.rs` |
| [`biq.md`](biq.md) | `.biq` container framing → `rust/src/dcl.rs` |
| [`editor.md`](editor.md) | Conquests scenario editor image: tag census, `3D` dispatch map, save-path mirror (open) |
| [`economy.md`](economy.md) | Culture/corruption/trade economy: border event, culture/corruption/split math → `rust/src/economy.rs` |
| [`stacking.md`](stacking.md) | Per-tile unit stacks: the `Cell+0x0C` list, the pool at `0xA52DD4`, placement order, consumers → `rust/src/stack.rs` |
| [`workers.md`](workers.md) | Unit orders/worker automation: string-negative verdict, goto enumerator, struct head (no Rust module yet) |
| [`dynamic-tracing.md`](dynamic-tracing.md) | Live-debugging runbook (Wine/winedbg); static analysis is exhausted for its questions |
| [`ui.md`](ui.md) | UI text: civilopedia hypertext tokenizer → `rust/src/ui.rs` |
| [`diplomacy.md`](diplomacy.md) | Diplomacy: relation matrix, tech trade → `rust/src/diplomacy.rs` |
| [`multiplayer.md`](multiplayer.md) | Multiplayer: mode global, net gates → `rust/src/net.rs` |
| [`air.md`](air.md) | Air combat: move dispatch, bombard-move log stub → `rust/src/air.rs` |
| [`media.md`](media.md) | Movies/victory media: intro gate, selectors, wonder art → `rust/src/media.rs` |
| [`rust/`](rust/) | Reference implementation. 182 tests (`cargo test --release`: 175 lib + 5 bin + 2 doc) |

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
* **The Oceans slider is only ever a seed.** It is never used as a threshold, so
  its effect on land fraction is indirect and non-monotone.
* **Rivers are not placed by map generation.** No stage places them, and the
  generator reads only two `.biq` sections — `TERR` and `GOOD` — so there is not
  even a data path by which a river could enter. Rivers come from a separate
  system applied to an already-generated map. See [`rivers.md`](rivers.md).
* **Goody huts and barbarian camps are separate stages**, `0x5f21b0` and
  `0x5f2090`, fully specified in `NOTES.md` and `rust/src/resources.rs`.
* **AI/combat randomness is independent of map randomness.** The game
  `rand`/`srand` pair (`0x64A20E`/`0x64A201`, state at owner `+0x14`) is never
  touched by map generation, which uses its own LCG (`0x60BA80`). See
  [`ai.md`](ai.md).
* **The `.biq` codec is now validated against the game itself.** The PKWARE
  DCL distance mask comes from `dict_bits`, not from the third header byte
  (`biq.md` correction, 2026-09-29): the old reading corrupted every
  `00 06 84` file — every Conquests `.biq` — while `EGYPT.SAV` happened to be
  unaffected, so the `.sav` golden tests stayed green. With the fix, the
  decode of `conquests.biq` is byte-identical to the temp file the game
  itself writes while loading its rules, and the rules data (`GOOD` names and
  frequencies, the 14 `TERR` resource allow-masks) can be read from the file
  instead of hardcoded. See [`biq.md`](biq.md), [`resources.md`](resources.md).
* **A tile's units are one list, newest placement first.** The list head sits
  in `Cell+0x0C`; `Unit::setPosition` (`0x5BD220`) pushes each placed unit at
  the head, and nothing else reorders a tile (no "fortify to the bottom").
  Every query and panel walks from the head, while the display path that draws
  a *single* unit sprite for a tile keeps the last entry — the tile's oldest
  resident. See [`stacking.md`](stacking.md).

## Scope (non-goals)

* Combat odds math proper is open; the turn-loop root is now mapped (record
  queue pump `0x468210`, kind dispatch `0x46F8B0` + table `0x47055C`), leaving
  only semantic names for its four producer routines. [`ai.md`](ai.md) lists both
  as concrete next targets. Nothing else in this directory claims them.
* Byte[cell+4] vs byte[cell+5] river-candidacy and the river-grained
  renderer mapping are marked `HYPOTHESIS` in [`rivers.md`](rivers.md) and
  `rust/src/rivers.rs`. Do not treat them as verified.
* Overlay stacking order in `graphics.rs` is inferred from file roles, not
  disassembly.
