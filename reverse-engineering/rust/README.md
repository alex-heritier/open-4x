# Civ3C map generator — reference implementation

A reimplementation of the **Civilization III: Conquests** random map generator,
recovered by static analysis of
`civ3/civ3-gog/app/Conquests/Civ3Conquests.exe`.

Every constant and control-flow decision is annotated with the address it came
from, so a claim can be checked against the disassembly. See
[`../NOTES.md`](../NOTES.md) for the full write-up.

```
cargo run --release -- --size 2 --water 50
cargo test --release
```

The renderer is faithful to the binary by default. Pass `--bugs none` for the
intended behaviour, or a subset like `--bugs sea-level-split,swapped-wrap-flags`.

The library has no dependencies. `tests/ground_truth.rs` uses the
`civ3_utils/biq` crate (a dev-dependency) to run the land/sea stage on shipped scenarios that are
the generator's own output and compare the result with their tiles; it returns
early when the git-ignored `civ3/` corpus (or `CIV3_DIR`) is absent.

## What this reproduces

| stage | address | status |
|---|---|---|
| option randomiser | `0x5f1f50` | exact |
| land/sea generation | `0x5eceb0` | exact for the draw it is given; 89-95 % of cells match three shipped maps at the game's draw (ocean 0 and 2), five ocean-1 saves do not match, and the continent-balance test that picks the draw is missing (`../NOTES.md` section 19, `tests/ground_truth.rs`) |
| start deconfliction | `0x5eeb00` | exact, including the original's index bug |
| desert conversion at starts | `0x5edb70` | exact |
| biome / climate assignment | `0x5f1480` | selection exact; `0x5f1ce0` flood mapped (`write_biome_class`), wiring awaits region ids |
| resource placement math (freq roll, quantity, block odds) | `0x5f22a0` | data-independent math only; needs `.biq` rows for full placement (`resources`) |
| goody huts, barbarian camps | `0x5f21b0`, `0x5f2090` | fully specified, implemented (`resources`) |
| start-location and smoothing stages | `0x5eeee0`+ | not implemented |
| river art path + overlay gate | `0x4C5FDA`, `0x407D56` | art path verified; the gate is the owner / at-war overlay, river storage open (`rivers`) |
| terrain / unit / city sprite selection | `0x4C5F9F`, `0x407C30` | observed inventory (`graphics`) |
| MSVC `rand`/`srand` (rolls no combat die) | `0x64A20E`, `0x64A201` | exact (`ai`); strategy/turn logic open |
| combat: odds, duel rounds, retreat, defender choice, ranged attacks, city strikes, defensive bombard, victory bookkeeping | `0x4A0ED0`, `0x4A53A0`, `0x4A3A70`, `0x4A2650`, `0x5BEF00` | formulas exact, rule constants from `conquests.biq`; open items in `../combat.md` (`combat`) |
| air defense: SAM, flak, patrolling interceptors | `0x5C68A0`, `0x4A4520` | exact, every draw in order (`air`, `../air.md`) |
| city capture and transfer: plunder, population loss, building survival, barbarian raid, the AI's raze and accept decisions | `0x563410`, `0x564800`, `0x563370`, `0x443B60`, `0x443A60` | formulas and decision order exact; the facts the AI reads are caller inputs (`capture`, `../capture.md`) |
| economy: growth box, upkeep, optimal city number, improvement cost with trait discounts, civ trait mask | `0x4B2030`, `0x5676C0`, `0x569FE0`, `0x53A080` | formulas exact; open constants listed in `../economy.md` (`economy`) |
| happiness: citizen moods, civil disorder and riots, We Love the King Day | `0x4BCFF0`, `0x4BDFF0`, `0x4BE440`, `0x4BE970` | exact, 30 000 + 30 000 random cases identical to the real code; the riot roll is read, not run; open items in `../happiness.md` (`happiness`) |
| `.biq`/`.bic` container codec | `0x649400` family | exact mode-0 decode; both shipped files verified (`dcl`, see `../biq.md`) |

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
| `rng.rs` | the `Random` class, `0x60ba80` / `0x60bab0`: map generator instances and the gameplay instance `0xA526B4` |
| `combat.rs` | combat resolution: odds and percentage terms, duel rounds, retreat, defender choice, ranged attacks, city strikes, defensive bombard, victory bookkeeping (`../combat.md`) |
| `air.rs` | air defense: SAM, flak and patrol interceptors, interception order and range test (`../air.md`) |
| `capture.rs` | city capture and transfer: the three modes, plunder, population loss, building survival, barbarian raid, the AI's raze and accept decisions (`../capture.md`) |
| `economy.rs` | growth and food box, support and payment, optimal city number, improvement cost and trait discounts, the civ trait bits (`../economy.md`) |
| `yields.rs` | tile yields: the three `Map` functions `0x5D7180` / `0x5D75F0` / `0x5D7AD0`, the TERR and GOOD tables, centre, water, wonder, Golden Age and Despotism rules (`../yields.md`) |
| `city.rs` | the city's per-turn totals: food eaten and surplus, the shield multiplier, tourists, Wealth, the commerce split, specialists (`../yields.md`) |
| `government.rs` | governments: the GOVT table, anarchy and revolution, war weariness, call to arms, the declaration of war (`../government.md`) |
| `research.rs` | research: the base cost and the turn clamp, the research step, `acquire` in order (eras, the scientific leader, Philosophy, the Great Library, the Science Age), the queue, the default-pick plumbing, the goody-hut advance (`World::hut_advance`) (`../research.md`) |
| `research_ai.rs` | the AI's valuation of an advance (`Valuer::value`, `0x448BF0`), the flavor overlap, the category mask, the default and steal picks (`../research-ai.md`) |
| `upgrade.rs` | unit upgrades: the facility flag by domain, the replacement walk, the gold price with Leonardo and the AI discount, the experience cap (`../unit-upgrades.md`) |
| `diplomacy.rs` | diplomacy: the per-pair relation state (`Relations`), contact, `declare_war` with the alliance and pact call-in, `make_peace`, the attitude score and class, `wants_war`, the deal clauses and their executor, the packages, the verdict ladder (`weigh`) (`../diplomacy.md`) |
| `happiness.rs` | citizen moods: the recompute `0x4BCFF0` (base mood, buildings, martial law, luxury, draft, war, foreign nationals, face distribution, reason percentages), disorder and the riot roll, celebration; checked against the real code by differential test (`../happiness.md`) |
| `buildable.rs` | `canBuildImprovement` / `canBuildUnit` and the free-building set key (`../buildable.md`) |
| `fractal.rs` | midpoint displacement, `0x5e1b60`, plus sampling and percentiles |
| `spiral.rs` | Manhattan-ring neighbour enumeration, `0x5e6e50` |
| `cell.rs` | the `(W/2) x H` cell grid and the Cell layout |
| `options.rs` | the world-setup sliders and the Randomize button |
| `landmass.rs` | `0x5eceb0`: the land/sea stage and its retry loop |
| `pipeline.rs` | the stage order and the stages that are implemented |
| `bugs.rs` | the opt-in flags for the original bugs |
| `main.rs` | terminal and PGM renderer |
