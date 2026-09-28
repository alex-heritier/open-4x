# Civ3C map generator — reference implementation

A reimplementation of the **Civilization III: Conquests** random map generator,
recovered by static analysis of `civ3-gog/app/Conquests/Civ3Conquests.exe`.

Every constant and control-flow decision is annotated with the address it came
from, so a claim can be checked against the disassembly. See
[`../NOTES.md`](../NOTES.md) for the full write-up.

```
cargo run --release -- --size 2 --water 50
cargo test --release
```

The renderer is faithful to the binary by default. Pass `--bugs none` for the
intended behaviour, or a subset like `--bugs sea-level-split,swapped-wrap-flags`.

## What this reproduces

| stage | address | status |
|---|---|---|
| option randomiser | `0x5f1f50` | exact |
| land/sea generation | `0x5eceb0` | exact |
| start deconfliction | `0x5eeb00` | exact, including the original's index bug |
| desert conversion at starts | `0x5edb70` | exact |
| biome / climate assignment | `0x5f1480` | exact except the `0x5f1ce0` call |
| resource, barbarian, start-location and smoothing stages | `0x5f22a0`+ | not implemented |

Everything not implemented only writes resource and feature ids into cells whose
terrain is already fixed, so the coastline, the ocean fraction, the biome layout
and the continent shapes are all decided by the stages that *are* here.

## The four things that matter

**1. The cell grid is `(W/2) x H`, not `W x H`.** Two map columns share a cell:

```text
cell = (W >> 1) * y + (x >> 1)
```

This single expression explains the otherwise baffling "nominal start slot"
arithmetic `y = i/(W>>1)`, `x = 2*(i%(W>>1)) + (y&1)` that every stage derives
its coordinates from. See `cell.rs`.

**2. Elevation comes from percentile lookups, not from absolute heights.** The
coastline and sea level are `percentileLookup(fm, 72)` and `percentileLookup(fm,
57)` on the *generated* fractal, with the percentiles chosen by map size. That
is why the land fraction is stable across seeds even though the field itself is
white noise at its coarsest level. See `fractal.rs` and `landmass.rs`.

**3. The "Oceans" slider is only a seed.** It is read exactly twice in the
land/sea stage and both uses are seed derivations
(`water + 0xCC98` and `water + 113n`). The slider chooses *which* fractal is
generated; the land fraction is then re-derived from that fractal's own
percentiles. **Nudging Oceans can therefore produce more ocean.** This is
original behaviour, and it is the single most mis-modelled part of Civ3 map
generation.

**4. There is no five-way temperature switch.** The biome stage computes one
continuous axis

```text
v = ((h - 128) * 30) / 256 + lat,   lat = 180 * |H/2 - y| / H
```

ranging over roughly `[-15, 105]`, and the biome boundaries are constant
cut-points on it. Because `lat` is 0 at the equator and 90 at the poles, the
wet classes (8, 9) land near the equator, desert and plains (0, 1) in the
mid-latitudes, and forest (3) at the poles. See `pipeline::assign_biomes`.

## Original bugs

The shipped binary has four places where the code does not do what it appears to
intend. Each is a separate flag on `OriginalBugs`, and all four default to
**off** — this crate implements the intended behaviour:

```rust
generate_with(&opts, &OriginalBugs::NONE)            // the default
generate_with(&opts, &OriginalBugs::ALL)             // faithful to the binary
generate_with(&opts, &OriginalBugs { sea_level_split: true, ..NONE })
```

| flag | where | what the binary does |
|---|---|---|
| `start_slot_index` | `0x5eeb00` pass 1 | writes the class to the **x-offset of the neighbour it found** rather than to the loop index, so the pass relabels 4 cells instead of the ~1000 that border land |
| `sea_level_split` | `0x5eceb0` | looks up a sea-level percentile `pLo` and only uses it in a branch guarded by `h <= pHi`; since `pHi > pLo` always, the ocean comes out with no shelf |
| `swapped_wrap_flags` | `0x5ecf5b` | feeds the map's **y**-wrap flag into the fractal's **x**-wrap bit, and never tests the y wrap at all |
| `contour_equality` | `0x5f1480` pass 2 | tests `h == p70` rather than `h <= p70` |

`cargo run --release -- --bugs none,sea-level-split` works the same way from the
command line.

Two notes on confidence. The first three are settled: the disassembly is
unambiguous, and two of them have the signature of a real slip — a register
holding the loop index is loaded and then never read, while a different value is
passed to the call.

`contour_equality` is the one I would not call a bug, and it is the reason the
default matters. `==` marks 0.0-0.5 % of the land; `<=` would mark 52-61 %, i.e.
paint most of the map as hills. Both readings are bad, which points to a
deliberate choice made against a different threshold than the one it resembles.
The flag is there so you can try the other reading.

A fifth suspicion did not survive checking. `0x5e1b60` reads its midpoint
neighbours with no bounds check, which looks like a buffer underflow, but the
array is `129 * 65` bytes and the loop bounds keep the flat index in `0..=8384`
for every level argument and flag combination. The extra row and column exist
precisely to make those edge reads well-defined.

## Layout

| file | contents |
|---|---|
| `rng.rs` | the map LCG, `0x60ba80` / `0x60bab0` |
| `fractal.rs` | midpoint displacement, `0x5e1b60`, plus sampling and percentiles |
| `spiral.rs` | Manhattan-ring neighbour enumeration, `0x5e6e50` |
| `cell.rs` | the `(W/2) x H` cell grid and the Cell layout |
| `options.rs` | the world-setup sliders and the Randomize button |
| `landmass.rs` | `0x5eceb0`: the land/sea stage and its retry loop |
| `pipeline.rs` | the stage order and the stages that are implemented |
| `bugs.rs` | the opt-in flags for the original bugs |
| `main.rs` | terminal and PGM renderer |
