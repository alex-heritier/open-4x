# Civ3 Conquests random map generation

Reverse-engineered documentation and a reference implementation of the
**Civilization III: Conquests** random map generator, recovered by static
analysis of `civ3-gog/app/Conquests/Civ3Conquests.exe`.

## Contents

| path | what it is |
|---|---|
| [`NOTES.md`](NOTES.md) | The full write-up. 18 sections: the complete 12-stage pipeline, the fractal, the `Cell` layout, the PRNGs, the tile-record format, the river investigation, the `.biq` codec, and the open questions. |
| [`rust/`](rust/) | A dependency-free Rust reference implementation. 3 832 lines, 93 tests. |

## Quick start

```sh
cd rust
cargo run --release -- --size 2 --water 50
cargo test --release
```

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
  system applied to an already-generated map. See `NOTES.md` §14.
* **Goody huts and barbarian camps are separate stages**, `0x5f21b0` and
  `0x5f2090`, previously unexamined and now fully specified.
