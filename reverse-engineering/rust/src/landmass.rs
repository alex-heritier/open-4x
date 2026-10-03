//! The land/sea stage — `generateLandmass`, `Civ3Conquests.exe` `0x5eceb0`.
//!
//! This is the **first** stage `generateMap` (`0x5eb580`) calls, and it is where
//! the fractal becomes an actual map. It has three jobs, in order:
//!
//! 1. draw a height field, retrying until its distribution is usably bimodal;
//! 2. derive three elevation thresholds from the field's own percentiles;
//! 3. walk the cells and write a terrain class into each one.
//!
//! # The thresholds are not constants
//!
//! ```c
//! switch (map->mapSize) {                    // 0x5ecec2..0x5ecf27
//!   case 0: PL_HI = 82; PL_LO = 67; break;    // Tiny
//!   case 1: PL_HI = 72; PL_LO = 57; break;    // Small
//!   case 4: PL_HI = 42; PL_LO = 27; break;    // Huge
//!   default: PL_HI = 62; PL_LO = 47;          // Std / Large
//! }
//! p96 = percentileLookup(fm, 96);
//! pHi = percentileLookup(fm, PL_HI);
//! pLo = percentileLookup(fm, PL_LO);
//! ```
//!
//! So the *fractions* are constants but the *heights* are recovered from the
//! generated field. `PL_HI` is the coastline and `PL_LO` the sea level; both fall
//! as the map grows, because a bigger map needs proportionally more qualifying
//! land. The reason percentiles rather than raw heights is that it makes the
//! land fraction independent of the field's own bias — the fractal is white
//! noise at its coarsest level, so an absolute cut-off would give wildly
//! different ocean coverage for different seeds.
//!
//! # The water level slider is only a seed
//!
//! `map->waterLevel` (`map + 0x1EC`) is read exactly twice in this stage, and
//! both uses are seed derivations:
//!
//! ```asm
//! 0x005ecf2f  mov eax, dword [esi+0x1ec]  ; waterLevel
//! 0x005ecf39  add eax, 0xcc98              ; + 52376
//! 0x005ecf3e  mov dword [esp+0x24], eax   ; rng state
//! 0x005ecfb6  mov edi, dword [esi+0x1ec]
//! 0x005ecfc1  add edi, eax                ; fractal seed = waterLevel + 113*n
//! ```
//!
//! The slider therefore chooses *which* fractal is generated; because the sea
//! level is then re-derived from that fractal's percentiles, the effect on the
//! land fraction is indirect and **not monotone** in the slider. That surprises
//! people: nudging "Oceans" can produce *more* ocean. This is correct original
//! behaviour, and it is the single most mis-modelled part of Civ3 map generation.
//!
//! # The per-cell decision
//!
//! `Cell::vfunc(0x8C)` is **"is water"** (`0x5eaa30`: `11 <= class <= 13`), not
//! "is land".
//!
//! ```c
//! for (i = 0; i < numCells; i++) {
//!     y = i / (W>>1);
//!     x = 2*(i % (W>>1)) + (y & 1);            // the nominal slot
//!
//!     h = sampleHeight(fm, x, y);
//!     if (h > p96 && rand_int(50) == 0) {      // 0x5ed186..0x5ed1aa
//!         getCell(i)->vfunc(0x128)(0x0C, -1, -1);
//!         continue;                            // an inland lake
//!     }
//!
//!     if (h > pHi) {                           // 0x5ed1c5
//!         for (n = 1; n < 9; n++) {            // ring-1 spiral
//!             (dx, dy) = spiralOffset(n);
//!             if (in bounds && sampleHeight(fm, x+dx, y+dy) > pHi) {
//!                 getCell(i)->vfunc(0x128)(0x02, -1, -1);
//!                 goto next;
//!             }
//!         }
//!         goto next;                           // nothing written
//!     } else {
//!         getCell(i)->vfunc(0x128)(0x0D, -1, -1);
//!     }
//! }
//! ```
//!
//! Two things are worth calling out:
//!
//! * **Coast carving.** A cell above `pHi` only becomes land if a ring-1
//!   neighbour is *also* above `pHi`. An isolated high cell keeps the Cell
//!   constructor's water class, which removes the 1-cell speckles a plain
//!   threshold would leave. This is the closest thing in the generator to a
//!   morphological filter.
//!
//! * **The `12` branch is dead in the binary.** The comment in the decompilation
//!   has it as `h3 > pLo ? 12 : 13`, but `pHi = PL(42..82) > pLo = PL(27..67)`
//!   always, and this branch is only reached when `h <= pHi`, so it always
//!   writes 13 and the ocean has no shelf. That is opt-in via
//!   [`OriginalBugs::sea_level_split`]; by default the split works, giving class
//!   12 to the band between the sea level and the coastline.
//!
//! # The retry loop
//!
//! ```asm
//! 0x005ed0ee  mov eax, dword [esp+0x14]   ; 113*n
//! 0x005ed0f2  cmp eax, 0x46a               ; 1130   -> n < 10
//! 0x005ed0f9  cmp edi, 0x46                ; pHi <  70  -> accept
//! 0x005ed0fe  cmp edi, 0x82                ; pHi <= 130 -> reject
//! 0x005ed106  cmp eax, 0x69f               ; 1695   -> n < 15
//! 0x005ed10d  cmp edi, 0x50                ; pHi <  80  -> accept
//! 0x005ed112  cmp edi, 0x78                ; pHi <= 120 -> reject
//! 0x005ed11c                                 ; else accept
//! ```
//!
//! i.e. re-roll unless the coastline height sits **outside the middle of the
//! byte range** — `pHi < 70 || pHi > 130` for the first ten draws, widening to
//! `pHi < 80 || pHi > 120` for the next five, and accepting unconditionally
//! from the fifteenth draw on. The test rejects fields whose coastline lands
//! near the middle of `0..255`, i.e. it wants a cleanly bimodal land/sea split
//! and will not accept a field where the sea level is mushy. This is what makes
//! the generator's output look deliberate rather than noisy.
//!
//! Each rejection bumps the seed by `0x71` = 113 (`0x5ed120`), and the outer
//! loop gives up after 10 tries (`0x5ed41f: cmp dword [var_28h], 0xa`).

use crate::bugs::OriginalBugs;
use crate::cell::MapGrid;
use crate::continents::{number_continents, Continent};
use crate::fractal::{flags as fflags, Fractal};
use crate::options::Options;
use crate::rng::Rng;
use crate::spiral::spiral_offset;

/// Seed stride between successive fractal draws: `0x71` = 113.
pub const P_STRIDE: u32 = 113;
/// Constant added to the `fmC`/`fmD` seeds in the multi-fractal styles: `0x3039`.
pub const SEED_EXTRA: u32 = 0x3039;
/// Seed base for the land/sea RNG: `0xCC98` = 52376.
pub const LANDMASS_SEED_BASE: u32 = 0xCC98;
/// Number of discarded warm-up draws before the first real one (`0x5ecf42`,
/// `0x5ecf4d`).
pub const LANDMASS_WARMUP: usize = 2;
/// First draw index at which the coastline test is skipped outright.
pub const ALWAYS_ACCEPT_FROM: u32 = 15;
/// Outer-loop cap (`0x5ed41f`).
pub const MAX_ITERATIONS: u32 = 10;

/// The two elevation percentiles used for the coastline and the sea level.
///
/// `0x5ecec2..0x5ecf27`, indexed by `map->mapSize`.
pub const COAST_PERCENTILE: [i32; 5] = [82, 72, 62, 62, 42];
/// The sea-level percentile. Always below [`COAST_PERCENTILE`].
pub const SEA_PERCENTILE: [i32; 5] = [67, 57, 47, 47, 27];
/// The percentile above which a cell may become an inland lake.
pub const LAKE_PERCENTILE: i32 = 96;
/// Odds of turning a cell above [`LAKE_PERCENTILE`] into a lake: 1 in 50.
pub const LAKE_ODDS: u32 = 50;

/// Fractional flags handed to the fractal generator.
///
/// With [`OriginalBugs::swapped_wrap_flags`] off (the default) each fractal
/// wrap bit is set from the matching map wrap bit, independently:
///
/// ```text
/// flags = (map.wrapFlags & 1 ? WRAP_X : 0) | (map.wrapFlags & 2 ? WRAP_Y : OCEAN_POLES)
/// ```
///
/// With it on, the binary's `0x5ecf5b..0x5ecf7c` sequence is reproduced:
///
/// ```asm
/// 0x005ecf5b  and ecx, 2                  ; map->wrapFlags & 2
/// 0x005ecf60  neg ebx / 1bdb sbb / 83e3fa / 83c308 / 83cb01
/// 0x005ecf7c  or  ebx, 0x10               ; Landmass == 1
/// ```
///
/// which reduces to `3` when the map wraps in y and `9` when it does not, plus
/// `0x10` for the 3-5-continents style. Bit 0 is the fractal's x wrap, so the
/// original feeds the map's **y** wrap flag into the fractal's **x** wrap bit and
/// never tests the y wrap at all.
pub fn fractal_flags(map: &MapGrid, landmass: i32, bugs: OriginalBugs) -> u32 {
    let base = if bugs.swapped_wrap_flags {
        if map.wrap_flags & 2 != 0 {
            fflags::WRAP_X | fflags::WRAP_Y
        } else {
            fflags::WRAP_X | fflags::OCEAN_POLES
        }
    } else {
        let mut f = if map.wrap_flags & 2 != 0 {
            fflags::WRAP_Y
        } else {
            fflags::OCEAN_POLES
        };
        if map.wrap_flags & 1 != 0 {
            f |= fflags::WRAP_X;
        }
        f
    };
    if landmass == 1 {
        base | fflags::SMEAR_WIDE
    } else {
        base
    }
}

/// The height fields one draw produces.
///
/// Only [`Fields::sampled`] is ever read by the cell loop. `fmC` exists to steer
/// the smear passes of `fmA` and `fmD` ([`Fractal::smear_from`]) and is
/// otherwise discarded.
pub struct Fields {
    /// `fmA` — the field that gets sampled and thresholded.
    pub sampled: Fractal,
    /// `fmC` — the smear source, drawn for landmass styles 1 and 2 only.
    pub source: Option<Fractal>,
}

/// Draws the height fields for one attempt.
///
/// The fractal configurations per landmass style, byte-confirmed at
/// `0x5ecfc8..0x5ed0a5` and re-checked against the original machine code
/// (`NOTES.md` section 19). `out` is the seventh argument: when it is a fractal
/// the callee runs the smear pass `0x5e1fd0` on its result.
///
/// | `map->landmass` | calls `(fm, W, H, level, flags, seed, out)` |
/// |---|---|
/// | 0 | `fmA, W, H, 3, ff, ~(wl+113n), 0` |
/// | 1 | `fmC, W, H, 2, wrapFlags&2, wl+113n+0x3039, 0`<br>`fmA, W, H, 2, ff, ~(wl+113n), &fmC`<br>`fmD, W, H, 3, ff, wl+113n, &fmC`<br>blend: `fmA = (fmA + fmD) / 2`, truncating |
/// | 2 | `fmC, W, H, 2, wrapFlags&2, wl+113n+0x3039, 0`<br>`fmA, W, H, 2, ff, ~(wl+113n), &fmC` |
pub fn draw_fields(map: &MapGrid, landmass: i32, n: u32, water_level: i32, bugs: OriginalBugs) -> Fields {
    let ff = fractal_flags(map, landmass, bugs);
    let wrap = map.wrap_flags & 2;
    let seed = (water_level as u32).wrapping_add(P_STRIDE * n);
    let (w, h) = (map.w, map.h);

    match landmass {
        0 => Fields {
            sampled: Fractal::generate(w, h, 3, ff, !seed),
            source: None,
        },
        1 => {
            let fm_c = Fractal::generate(w, h, 2, wrap, seed.wrapping_add(SEED_EXTRA));
            let mut fm_a = Fractal::generate_with(w, h, 2, ff, !seed, Some(&fm_c));
            let fm_d = Fractal::generate_with(w, h, 3, ff, seed, Some(&fm_c));
            // 0x5ed058..0x5ed07c: fmA.h[i] = (fmA.h[i] + fmD.h[i]) / 2.
            fm_a.blend(&fm_d);
            Fields {
                sampled: fm_a,
                source: Some(fm_c),
            }
        }
        _ => {
            let fm_c = Fractal::generate(w, h, 2, wrap, seed.wrapping_add(SEED_EXTRA));
            Fields {
                sampled: Fractal::generate_with(w, h, 2, ff, !seed, Some(&fm_c)),
                source: Some(fm_c),
            }
        }
    }
}

/// The three thresholds derived from a field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Thresholds {
    /// `percentileLookup(fm, 96)` — the top of the height range.
    pub lake: u8,
    /// `percentileLookup(fm, PL_HI)` — the coastline.
    pub coast: u8,
    /// `percentileLookup(fm, PL_LO)` — the sea level.
    pub sea: u8,
}

/// Whether the retry loop accepts this field's coastline height.
pub fn accept_coast(p: u32, coast: u8) -> bool {
    if p < 1130 {
        !(70..=130).contains(&coast)
    } else if p < 1695 {
        !(80..=120).contains(&coast)
    } else {
        true
    }
}

/// The land/sea stage's output.
pub struct Landmass {
    /// Dimensions, options and the cell grid, with a class and a continent id in
    /// every cell.
    pub grid: MapGrid,
    /// The height field the classes were derived from.
    pub field: Fractal,
    /// The three thresholds used.
    pub thresholds: Thresholds,
    /// Index of the fractal draw the accepted pass used (0-based). Every draw the
    /// coast test looks at consumes an index, accepted or not, so a map that took
    /// `k` balance retries sits at draw `k` or later.
    pub draws: u32,
    /// Painting passes used, 1-based (at most [`MAX_ITERATIONS`]).
    pub iterations: u32,
    /// The ranked continent records of the accepted pass (`finalizeMap`).
    pub continents: Vec<Continent>,
}

impl Landmass {
    /// Number of land cells — everything the stage did not leave as water.
    pub fn land_cells(&self) -> usize {
        self.grid.cells.iter().filter(|c| !c.is_water()).count()
    }
}

/// Runs the land/sea stage: the retry loop, the thresholds and the per-cell
/// write.
pub fn generate_landmass(
    w: i32,
    h: i32,
    opts: &Options,
    water_level: i32,
    bugs: OriginalBugs,
) -> Landmass {
    generate_landmass_wrapped(w, h, 0, opts, water_level, bugs)
}

/// [`generate_landmass`] for a map with the given `Map+0x1F0` wrap flags
/// (bit 0 wraps x, bit 1 wraps y; see [`fractal_flags`]). A scenario file
/// stores them in `WMAP`.
pub fn generate_landmass_wrapped(
    w: i32,
    h: i32,
    wrap_flags: u32,
    opts: &Options,
    water_level: i32,
    bugs: OriginalBugs,
) -> Landmass {
    generate_landmass_from_draw(w, h, wrap_flags, opts, water_level, 0, bugs)
}

/// [`generate_landmass_wrapped`] with the retry loop entered at draw
/// `first_draw` instead of draw 0.
///
/// Only the fractal moves with the draw (its seed is `water_level + 113 n`). The
/// painting RNG, the grid and the pass counter start fresh, so this is **not** the
/// exe's state after `first_draw` rejected passes. It exists to look at a single
/// draw in isolation; [`generate_landmass_wrapped`] is the faithful entry point.
pub fn generate_landmass_from_draw(
    w: i32,
    h: i32,
    wrap_flags: u32,
    opts: &Options,
    water_level: i32,
    first_draw: u32,
    bugs: OriginalBugs,
) -> Landmass {
    generate_landmass_core(w, h, wrap_flags, opts, water_level, first_draw, MAX_ITERATIONS, bugs)
}

/// The whole stage with both loop parameters exposed: the draw the coast loop
/// starts at, and the number of painting passes after which the last one is kept
/// whatever its shape (the exe's constant is [`MAX_ITERATIONS`], `0x5ed41f`).
///
/// A small `max_passes` shows the intermediate passes of the exe's own sequence,
/// state carried over, which is how a map that the exe kept after the cap rather than
/// after the balance test can be recognised.
pub fn generate_landmass_core(
    w: i32,
    h: i32,
    wrap_flags: u32,
    opts: &Options,
    water_level: i32,
    first_draw: u32,
    max_passes: u32,
    bugs: OriginalBugs,
) -> Landmass {
    let map_size = opts.ocean.clamp(0, 4) as usize;
    let (pl_hi, pl_lo) = (COAST_PERCENTILE[map_size], SEA_PERCENTILE[map_size]);

    let mut grid = MapGrid::new(w, h, wrap_flags, water_level);

    // 0x5ecf2f..0x5ecf4d: one RNG for every painting pass, seeded once.
    let mut rng = Rng::new((water_level as u32).wrapping_add(LANDMASS_SEED_BASE));
    rng.discard(LANDMASS_WARMUP);

    // `n` (`[esp+0x20]`) is bumped on every exit of the coast test, accepted or not
    // (`0x5ed11c..0x5ed12e`), so a pass the balance test throws away moves on to the
    // next fractal rather than repainting the same one.
    let mut n = first_draw;
    let mut iterations = 0;
    loop {
        iterations += 1;

        // The coast loop: draw until the coastline height passes the test.
        let (fields, thresholds, draw) = loop {
            let fields = draw_fields(&grid, opts.landmass, n, water_level, bugs);
            let t = Thresholds {
                lake: fields.sampled.percentile(LAKE_PERCENTILE),
                coast: fields.sampled.percentile(pl_hi),
                sea: fields.sampled.percentile(pl_lo),
            };
            let accepted = accept_coast(P_STRIDE * n, t.coast);
            let draw = n;
            n += 1;
            if accepted {
                break (fields, t, draw);
            }
        };

        // 0x5ed138..0x5ed2da. The grid is *not* cleared between passes: a cell the
        // pass declines to write keeps what the previous pass gave it.
        paint_land_and_sea(&mut grid, &fields.sampled, thresholds, &mut rng, bugs);

        // 0x5ed2e4: finalizeMap, then the shape test for this landmass style.
        let continents = number_continents(&mut grid);
        if balanced(opts.landmass, &continents) || iterations >= max_passes {
            return Landmass {
                grid,
                field: fields.sampled,
                thresholds,
                draws: draw,
                iterations,
                continents,
            };
        }
    }
}

/// The continent-balance test at `0x5ed2eb..0x5ed42a`: does the landmass the pass
/// painted have the shape the Landmass slider asked for?
///
/// `c` is the ranked record list from [`number_continents`], so `c[0]` is the largest
/// landmass. `n = c.len()` counts water bodies too.
///
/// | style | accepted when |
/// |---|---|
/// | 0 | `n > 2`, `c[2]` is land and `c[0] < 2 * c[2]`: several landmasses of comparable size |
/// | 1 | `n > 2`, `c[1]` is land, `c[0] < 3 * c[1] / 2`, and either `c[2]` is not land or `c[0] > 4 * c[2]`: two similar big ones, the rest small |
/// | other | `n > 1`, and either `c[1]` is not land or `c[0] > 8 * c[1]`: one dominant landmass |
///
/// A pass that fails is repainted from the next fractal, up to [`MAX_ITERATIONS`]
/// passes; the last one is kept whatever its shape.
pub fn balanced(landmass: i32, c: &[Continent]) -> bool {
    let n = c.len();
    // getContinent(i) past the end reads whatever follows the array; every use below
    // is behind an `n` guard, so a missing record is simply "not land".
    let land = |i: usize| c.get(i).is_some_and(|r| r.is_land);
    let size = |i: usize| i64::from(c.get(i).map_or(0, |r| r.size));
    match landmass {
        0 => n > 2 && land(2) && size(0) < 2 * size(2),
        1 => {
            n > 2
                && land(1)
                && size(0) < (3 * size(1)) / 2
                && (!land(2) || size(0) > 4 * size(2))
        }
        _ => n > 1 && (!land(1) || size(0) > 8 * size(1)),
    }
}

/// The per-cell write: `0x5ed178..0x5ed2c7`.
fn paint_land_and_sea(
    grid: &mut MapGrid,
    fm: &Fractal,
    t: Thresholds,
    rng: &mut Rng,
    bugs: OriginalBugs,
) {
    for i in 0..grid.num_cells() {
        let (x, y) = grid.nominal_slot(i);

        let h = fm.sample(x, y);
        if h > t.lake && rng.one_in(LAKE_ODDS) {
            set_class(grid, i, 12);
            continue;
        }

        if h > t.coast {
            // Coast carving: land only if a ring-1 neighbour is also above the
            // coastline. The cell keeps its water class otherwise.
            let mut land = false;
            for n in 1..=8 {
                let (dx, dy) = spiral_offset(n);
                let Some((nx, ny)) = grid.wrap_and_check(x + dx, y + dy) else {
                    continue;
                };
                if fm.sample(nx, ny) > t.coast {
                    land = true;
                    break;
                }
            }
            if land {
                set_class(grid, i, 2);
            }
        } else {
            // The binary writes `h > pLo ? 12 : 13` here, but this branch is only
            // reachable when `h <= pHi` and `pHi > pLo` always, so it always
            // writes 13. Reproduce that with `sea_level_split`; otherwise split on
            // the sea level directly, which is what the code is plainly after.
            let class = if bugs.sea_level_split || h <= t.sea {
                13
            } else {
                12
            };
            set_class(grid, i, class);
        }
    }
}

/// `getCell(i)->vfunc(0x128)(t, -1, -1)`.
fn set_class(grid: &mut MapGrid, i: usize, t: u8) {
    if let Some(cell) = grid.cell_mut(i) {
        cell.set_class(t);
    }
}


/// `landmassFix`, `0x5ED440`: pulls the two biggest continents apart.
///
/// `generateMap` calls it only for the land/sea style that asks for two similar
/// big landmasses (`Map+0x18 == 1`, see [`balanced`]). Continent `1` is the
/// second-largest landmass and continent `0` the largest ([`number_continents`]
/// ranks land by size). Every cell of continent `1` that has a cell of continent
/// `0` anywhere in the first `(2*m + 5)^2` positions of the spiral becomes sea
/// (`setTerrain(12)`), where `m = min(4, ((W + H) / 2) / 50)`, so the radius is
/// 6 tiles on a Tiny or Small map. The continent ids are then renumbered.
pub fn separate_continents(grid: &mut MapGrid) {
    let m = (((grid.w + grid.h) >> 1) / 50).min(4);
    let limit = (2 * m + 5) * (2 * m + 5);
    for i in 0..grid.num_cells() {
        if grid.cells[i].continent != 1 {
            continue;
        }
        let (x, y) = grid.coords(i);
        for n in 1..limit {
            let (dx, dy) = spiral_offset(n);
            let Some((nx, ny)) = grid.wrap_and_check(x + dx, y + dy) else {
                continue;
            };
            if grid.cell_at(nx, ny).is_some_and(|c| c.continent == 0) {
                grid.cells[i].set_class(12);
            }
        }
    }
    number_continents(grid);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::map_size;

    fn opts(size: i32, water: i32) -> Options {
        Options {
            size,
            ocean: size,
            seed: water,
            ..Options::default()
        }
    }

    #[test]
    fn unwrapped_entry_point_is_the_wrapped_one_with_no_wrap() {
        let o = opts(1, 37);
        let a = generate_landmass(60, 60, &o, 37, OriginalBugs::NONE);
        let b = generate_landmass_wrapped(60, 60, 0, &o, 37, OriginalBugs::NONE);
        assert_eq!(a.grid.cells, b.grid.cells);
        assert_eq!(a.draws, b.draws);
    }

    #[test]
    fn wrap_flags_change_the_map() {
        let o = opts(1, 37);
        let a = generate_landmass_wrapped(60, 60, 0, &o, 37, OriginalBugs::NONE);
        let b = generate_landmass_wrapped(60, 60, 1, &o, 37, OriginalBugs::NONE);
        assert_ne!(a.grid.cells, b.grid.cells, "x wrap must reach the fractal");
    }

    #[test]
    fn entering_the_loop_at_the_accepted_draw_reproduces_the_field() {
        // The field and its thresholds depend on the draw alone. The cells also depend
        // on the painting RNG and on cells earlier passes left behind, so they match
        // only when the pass was the first one.
        let o = opts(1, 37);
        let a = generate_landmass_wrapped(60, 60, 1, &o, 37, OriginalBugs::NONE);
        let b = generate_landmass_from_draw(60, 60, 1, &o, 37, a.draws, OriginalBugs::NONE);
        assert_eq!(b.draws, a.draws);
        assert_eq!(b.thresholds, a.thresholds);
        assert_eq!(b.field.heights(), a.field.heights());
        if a.iterations == 1 {
            assert_eq!(a.grid.cells, b.grid.cells);
        }
    }

    #[test]
    fn a_first_pass_that_is_accepted_is_reproduced_cell_for_cell() {
        let mut checked = 0;
        for water in 0..60 {
            let o = opts(1, water);
            let a = generate_landmass_wrapped(60, 60, 1, &o, water, OriginalBugs::NONE);
            if a.iterations != 1 {
                continue;
            }
            let b = generate_landmass_from_draw(60, 60, 1, &o, water, a.draws, OriginalBugs::NONE);
            assert_eq!(a.grid.cells, b.grid.cells, "water {water}");
            checked += 1;
        }
        assert!(checked > 0, "no single-pass map in the sample");
    }

    #[test]
    fn a_later_first_draw_gives_a_different_map() {
        let o = opts(1, 37);
        let a = generate_landmass_wrapped(60, 60, 1, &o, 37, OriginalBugs::NONE);
        let b = generate_landmass_from_draw(60, 60, 1, &o, 37, a.draws + 1, OriginalBugs::NONE);
        assert!(b.draws > a.draws);
        assert_ne!(a.grid.cells, b.grid.cells);
    }

    #[test]
    fn coast_percentile_always_above_sea_percentile() {
        for i in 0..5 {
            assert!(
                COAST_PERCENTILE[i] > SEA_PERCENTILE[i],
                "size {i}: coast {} <= sea {}",
                COAST_PERCENTILE[i],
                SEA_PERCENTILE[i]
            );
        }
    }

    #[test]
    fn both_percentiles_fall_with_map_size() {
        for i in 1..5 {
            assert!(COAST_PERCENTILE[i] <= COAST_PERCENTILE[i - 1]);
            assert!(SEA_PERCENTILE[i] <= SEA_PERCENTILE[i - 1]);
        }
        assert!(COAST_PERCENTILE[0] > COAST_PERCENTILE[4]);
    }

    #[test]
    fn the_accepted_thresholds_are_ordered() {
        let (w, h) = map_size(1);
        for water in 0..=100 {
            let lm = generate_landmass(w, h, &opts(1, water), water, OriginalBugs::NONE);
            assert!(
                lm.thresholds.sea <= lm.thresholds.coast,
                "water {water}: sea {} > coast {}",
                lm.thresholds.sea,
                lm.thresholds.coast
            );
            assert!(lm.thresholds.coast <= lm.thresholds.lake);
        }
    }

    #[test]
    fn the_retry_loops_never_overrun_their_caps() {
        // Passes are capped at MAX_ITERATIONS, and from draw 15 on the coast test
        // accepts anything, so the last pass cannot sit beyond draw 14 + 10.
        let (w, h) = map_size(1);
        for landmass in 0..3 {
            for water in 0..=100 {
                let o = Options { landmass, ..opts(1, water) };
                let lm = generate_landmass(w, h, &o, water, OriginalBugs::NONE);
                assert!(lm.iterations >= 1 && lm.iterations <= MAX_ITERATIONS);
                assert!(
                    lm.draws < ALWAYS_ACCEPT_FROM + MAX_ITERATIONS,
                    "style {landmass} water {water}: took draw {}",
                    lm.draws
                );
                assert!(
                    accept_coast(P_STRIDE * lm.draws, lm.thresholds.coast),
                    "style {landmass} water {water}: accepted a field the coast test rejects"
                );
                assert!(lm.draws + 1 >= lm.iterations, "every pass consumes a draw");
            }
        }
    }

    #[test]
    fn a_map_is_only_unbalanced_when_the_passes_ran_out() {
        let (w, h) = map_size(1);
        let mut retried = 0;
        for landmass in 0..3 {
            for water in 0..=100 {
                let o = Options { landmass, ..opts(1, water) };
                let lm = generate_landmass(w, h, &o, water, OriginalBugs::NONE);
                assert!(
                    balanced(landmass, &lm.continents) || lm.iterations == MAX_ITERATIONS,
                    "style {landmass} water {water}: gave up after {} pass(es)",
                    lm.iterations
                );
                retried += usize::from(lm.iterations > 1);
            }
        }
        assert!(retried > 0, "the balance test never rejected anything");
    }

    #[test]
    fn the_retry_loop_terminates() {
        // Even a pathological field must not spin: acceptance is unconditional
        // from ALWAYS_ACCEPT_FROM on.
        for n in 0..200u32 {
            let p = P_STRIDE * n;
            if n >= ALWAYS_ACCEPT_FROM {
                assert!(accept_coast(p, 100), "draw {n} was not auto-accepted");
            }
        }
    }

    #[test]
    fn coast_test_rejects_the_middle_of_the_range() {
        assert!(!accept_coast(0, 100), "the middle must be rejected");
        assert!(!accept_coast(0, 70));
        assert!(!accept_coast(0, 130));
        assert!(accept_coast(0, 69));
        assert!(accept_coast(0, 131));
        // The window narrows for later draws.
        assert!(!accept_coast(1130, 100));
        assert!(!accept_coast(1130, 80));
        assert!(!accept_coast(1130, 120));
        assert!(accept_coast(1130, 79));
        assert!(accept_coast(1130, 121));
    }

    #[test]
    fn every_cell_gets_a_class() {
        let (w, h) = map_size(0);
        let lm = generate_landmass(w, h, &opts(0, 50), 50, OriginalBugs::NONE);
        for c in &lm.grid.cells {
            assert!(c.class() <= 13, "class {}", c.class());
        }
    }

    #[test]
    fn there_is_land_and_there_is_sea() {
        for size in 0..5 {
            let (w, h) = map_size(size);
            let lm = generate_landmass(w, h, &opts(size, 50), 50, OriginalBugs::NONE);
            let land = lm.land_cells();
            let total = lm.grid.num_cells();
            assert!(land > 0, "size {size}: no land at all");
            assert!(land < total, "size {size}: no water at all");
        }
    }

    #[test]
    fn land_fraction_tracks_the_coastline_percentile() {
        // Land is `h > pHi`, so the land fraction should be `100 - PL_HI`:
        // 18% on a Tiny map rising to 58% on a Huge one. The sea-level
        // percentile is *not* the land/water boundary — see
        // `sea_level_percentile_is_computed_but_unused`.
        for size in 0..5 {
            let (w, h) = map_size(size);
            let lm = generate_landmass(w, h, &opts(size, 50), 50, OriginalBugs::NONE);
            let pct = (100 * lm.land_cells() / lm.grid.num_cells()) as i32;
            let want = 100 - COAST_PERCENTILE[size as usize];
            assert!(
                (pct - want).abs() <= 6,
                "size {size}: {pct}% land, expected about {want}%"
            );
        }
    }

    #[test]
    fn the_land_fraction_falls_as_the_map_grows() {
        // A bigger map needs proportionally less land for the same number of
        // civs, which is why PL_HI drops with the map size.
        let pct = |size: i32| {
            let (w, h) = map_size(size);
            let lm = generate_landmass(w, h, &opts(size, 50), 50, OriginalBugs::NONE);
            100 * lm.land_cells() / lm.grid.num_cells()
        };
        let small = pct(0);
        let large = pct(4);
        assert!(
            small < large,
            "Tiny gave {small}% land, Huge gave {large}%"
        );
    }

    #[test]
    fn the_sea_level_split_gives_the_ocean_a_shelf() {
        // With the bug off (the default) the water case splits on `h > pLo`, so
        // the band between the sea level and the coastline is class 12 and only
        // the deep ocean is 13. The class-12 band should be roughly the gap
        // between the two percentiles.
        let (w, h) = map_size(2);
        let lm = generate_landmass(w, h, &opts(2, 50), 50, OriginalBugs::NONE);
        let shelf = lm.grid.cells.iter().filter(|c| c.class() == 12).count();
        let deep = lm.grid.cells.iter().filter(|c| c.class() == 13).count();
        assert!(shelf > 0, "no class-12 shelf");
        assert!(deep > 0, "no deep water");
        // SEA_PERCENTILE[2] = 47 and COAST_PERCENTILE[2] = 62, so the shelf
        // should be roughly that 15 % gap.
        let pct = 100 * shelf / lm.grid.num_cells();
        assert!(
            (8..=22).contains(&pct),
            "the shelf is {pct}% of the map, expected roughly 15%"
        );
    }

    #[test]
    fn the_sea_level_split_is_dead_in_the_binary() {
        // With `sea_level_split` on, the class-12 band disappears entirely and
        // the only 12s left are the `p96` inland lakes, which are 1-in-50 of the
        // top 4 % of heights — i.e. vanishingly rare. The ocean becomes uniform.
        let (w, h) = map_size(2);
        let o = opts(2, 50);
        let faithful = generate_landmass(w, h, &o, 50, OriginalBugs::ALL);
        let fixed = generate_landmass(w, h, &o, 50, OriginalBugs::NONE);

        let shelf_of = |lm: &crate::landmass::Landmass| {
            lm.grid.cells.iter().filter(|c| c.class() == 12).count()
        };
        assert!(
            shelf_of(&fixed) > shelf_of(&faithful) * 20,
            "corrected {} vs faithful {} shelf cells",
            shelf_of(&fixed),
            shelf_of(&faithful)
        );
        // And the deep class still exists, so the difference is a real split and
        // not the map having collapsed.
        assert!(faithful.grid.cells.iter().filter(|c| c.class() == 13).count() > 0);
        assert!(
            faithful.thresholds.sea < faithful.thresholds.coast,
            "pLo {} should still sit below pHi {}",
            faithful.thresholds.sea,
            faithful.thresholds.coast
        );
    }

    #[test]
    fn isolated_peaks_are_carved_away() {
        // A cell above the coastline with no neighbour above it stays water.
        //
        // Only true of a first pass: a later pass leaves a declined cell as the
        // pass before it had it, which may be land. So look for a single-pass map.
        let (w, h) = map_size(0);
        let lm = (0..100)
            .map(|water| generate_landmass(w, h, &opts(0, water), water, OriginalBugs::NONE))
            .find(|lm| lm.iterations == 1)
            .expect("no single-pass map in the sample");
        let t = lm.thresholds;
        let mut single = 0;

        // Only the nominal slot of each cell is written by the stage, so the
        // check has to visit the same coordinates the stage did.
        for i in 0..lm.grid.num_cells() {
            let (x, y) = lm.grid.nominal_slot(i);
            if lm.field.sample(x, y) <= t.coast {
                continue;
            }
            let mut high = 0;
            for n in 1..=8 {
                let (dx, dy) = spiral_offset(n);
                let Some((nx, ny)) = lm.grid.wrap_and_check(x + dx, y + dy) else {
                    continue;
                };
                if lm.field.sample(nx, ny) > t.coast {
                    high += 1;
                }
            }
            if high == 0 {
                single += 1;
                let c = lm.grid.cell(i).unwrap();
                assert!(c.is_water(), "({x},{y}) should have been carved");
            }
        }
        assert!(single > 0, "no isolated peaks to carve — the test is vacuous");
    }

    #[test]
    fn water_level_is_only_a_seed() {
        // Two different water levels give different maps, but neither is
        // systematically wetter — that is the original's behaviour.
        let (w, h) = map_size(1);
        let a = generate_landmass(w, h, &opts(1, 0), 0, OriginalBugs::NONE);
        let b = generate_landmass(w, h, &opts(1, 100), 100, OriginalBugs::NONE);
        assert_ne!(a.grid.cells, b.grid.cells);
    }

    #[test]
    fn landmass_is_deterministic() {
        let (w, h) = map_size(1);
        let a = generate_landmass(w, h, &opts(1, 42), 42, OriginalBugs::NONE);
        let b = generate_landmass(w, h, &opts(1, 42), 42, OriginalBugs::NONE);
        assert_eq!(a.grid.cells, b.grid.cells);
        assert_eq!(a.draws, b.draws);
    }

    #[test]
    fn all_three_landmass_styles_terminate() {
        for lm in 0..3 {
            for water in [0, 25, 50, 75, 100] {
                let o = Options {
                    landmass: lm,
                    ..opts(2, water)
                };
                let m = generate_landmass(map_size(2).0, map_size(2).1, &o, water, OriginalBugs::NONE);
                assert!(m.draws < ALWAYS_ACCEPT_FROM + MAX_ITERATIONS);
                assert!(m.land_cells() > 0);
            }
        }
    }

    fn rec(is_land: bool, size: u32) -> Continent {
        Continent { is_land, size }
    }

    #[test]
    fn style_0_wants_three_comparable_landmasses() {
        let sea = rec(false, 900);
        // c[0] < 2 * c[2], and c[2] is land.
        assert!(balanced(0, &[rec(true, 59), rec(true, 40), rec(true, 30), sea]));
        assert!(!balanced(0, &[rec(true, 60), rec(true, 40), rec(true, 30), sea]), "60 is not < 2 * 30");
        assert!(!balanced(0, &[rec(true, 50), rec(true, 40), sea]), "c[2] is water");
        assert!(!balanced(0, &[rec(true, 50), sea]), "n <= 2");
    }

    #[test]
    fn style_1_wants_two_similar_landmasses_and_a_small_third() {
        let sea = rec(false, 900);
        // c[0] < 3 * c[1] / 2 (truncated), and c[2] absent or c[0] > 4 * c[2].
        assert!(balanced(1, &[rec(true, 59), rec(true, 40), sea]));
        assert!(!balanced(1, &[rec(true, 60), rec(true, 40), sea]), "60 is not < 60");
        assert!(balanced(1, &[rec(true, 59), rec(true, 40), rec(true, 14), sea]), "59 > 4 * 14");
        assert!(!balanced(1, &[rec(true, 59), rec(true, 40), rec(true, 15), sea]), "59 is not > 60");
        assert!(!balanced(1, &[rec(true, 50), sea, sea]), "c[1] is water");
        assert!(!balanced(1, &[rec(true, 50), rec(true, 40)]), "n <= 2");
        // 3 * 3 / 2 truncates to 4.
        assert!(balanced(1, &[rec(true, 3), rec(true, 3), sea]));
        assert!(!balanced(1, &[rec(true, 5), rec(true, 3), sea]));
    }

    #[test]
    fn other_styles_want_one_dominant_landmass() {
        let sea = rec(false, 900);
        for style in [2, 3] {
            assert!(balanced(style, &[rec(true, 81), rec(true, 10), sea]), "81 > 80");
            assert!(!balanced(style, &[rec(true, 80), rec(true, 10), sea]));
            // A single landmass: the record after it is the sea.
            assert!(balanced(style, &[rec(true, 5), sea]));
            assert!(!balanced(style, &[rec(true, 5)]), "n <= 1");
        }
    }

    #[test]
    fn fractal_flags_map_each_wrap_bit_to_its_own() {
        // The corrected behaviour: bit 0 of the map's wrap flags drives the
        // fractal's x wrap, bit 1 its y wrap, independently. A map that does not
        // wrap in y gets ocean poles instead of a y wrap.
        let cases = [
            (0b00, fflags::OCEAN_POLES),
            (0b01, fflags::WRAP_X | fflags::OCEAN_POLES),
            (0b10, fflags::WRAP_Y),
            (0b11, fflags::WRAP_X | fflags::WRAP_Y),
        ];
        for (wrap, want) in cases {
            let g = MapGrid::new(100, 100, wrap, 0);
            assert_eq!(
                fractal_flags(&g, 0, OriginalBugs::NONE),
                want,
                "map wrap flags {wrap:#04b}"
            );
        }
    }

    #[test]
    fn fractal_flags_reproduce_the_binary_when_the_bug_is_on() {
        // 0x5ecf5b: the map's *y* wrap is tested and fed to the fractal's *x*
        // bit, and the y wrap is never looked at.
        let mut g = MapGrid::new(100, 100, 0b10, 0);
        assert_eq!(
            fractal_flags(&g, 0, OriginalBugs::ALL),
            fflags::WRAP_X | fflags::WRAP_Y,
            "a y-wrapping map gives the fractal both wraps"
        );
        g.wrap_flags = 0;
        assert_eq!(
            fractal_flags(&g, 0, OriginalBugs::ALL),
            fflags::WRAP_X | fflags::OCEAN_POLES,
            "a map that wraps in x only still gets an x-wrapping fractal"
        );
        assert_eq!(
            fractal_flags(&g, 1, OriginalBugs::ALL),
            fflags::WRAP_X | fflags::OCEAN_POLES | fflags::SMEAR_WIDE
        );
    }

    #[test]
    fn the_swapped_flag_always_sets_the_fractals_x_wrap() {
        // The binary's x-wrap bit is set unconditionally; the corrected version
        // follows the map. They agree only when the map itself wraps in x.
        for wrap in [0b00, 0b01, 0b10, 0b11] {
            let g = MapGrid::new(100, 100, wrap, 0);
            let fixed = fractal_flags(&g, 0, OriginalBugs::NONE);
            let faithful = fractal_flags(&g, 0, OriginalBugs::ALL);
            assert_ne!(
                faithful & fflags::WRAP_X,
                0,
                "map wrap flags {wrap:#04b}: the binary always wraps in x"
            );
            assert_eq!(
                fixed & fflags::WRAP_X != 0,
                wrap & 1 != 0,
                "map wrap flags {wrap:#04b}: the corrected path should follow the map"
            );
        }
    }

    #[test]
    fn style_one_blends_two_fields() {
        let g = MapGrid::new(80, 80, 0, 30);
        assert_ne!(
            draw_fields(&g, 1, 0, 30, OriginalBugs::NONE)
                .sampled
                .heights(),
            draw_fields(&g, 0, 0, 30, OriginalBugs::NONE)
                .sampled
                .heights()
        );
    }
}
