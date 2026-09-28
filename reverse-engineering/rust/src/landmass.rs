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
use crate::cell::{Cell, MapGrid};
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

/// The pair of height fields this stage works with.
///
/// Only [`Fields::sampled`] is ever read; the other is an intermediate whose
/// only role is to be blended into the first.
pub struct Fields {
    /// `fmA` — the field that gets sampled and thresholded.
    pub sampled: Fractal,
    /// `fmC` or `fmD`, depending on the style. Kept for fidelity.
    pub other: Fractal,
}

/// Draws the height fields for one attempt.
///
/// The fractal configurations per landmass style, byte-confirmed at
/// `0x5ecfc8..0x5ed0a5`:
///
/// | `map->landmass` | calls `(fm, W, H, level, flags, seed, out)` |
/// |---|---|
/// | 0 | `fmA, W, H, 3, ff, ~(wl+113n), 0` |
/// | 1 | `fmC, W, H, 2, wrapFlags&2, wl+113n+0x3039, 0`<br>`fmA, W, H, 2, ff, ~(wl+113n), &fmC`<br>`fmD, W, H, 3, ff, wl+113n, &fmC`<br>blend: `fmA = (fmA + fmD) / 2`` |
/// | 2 | `fmC, W, H, 2, wrapFlags&2, wl+113n+0x3039, 0`<br>`fmA, W, H, 2, ff, ~(wl+113n), &fmC` |
pub fn draw_fields(map: &MapGrid, landmass: i32, n: u32, water_level: i32, bugs: OriginalBugs) -> Fields {
    let ff = fractal_flags(map, landmass, bugs);
    let wrap = map.wrap_flags & 2;
    let seed = (water_level as u32).wrapping_add(P_STRIDE * n);
    let (w, h) = (map.w, map.h);

    match landmass {
        0 => Fields {
            sampled: Fractal::generate(w, h, 3, ff, !seed),
            other: Fractal::generate(w, h, 3, ff, !seed),
        },
        1 => {
            let mut fm_a = Fractal::generate(w, h, 2, ff, !seed);
            let fm_d = Fractal::generate(w, h, 3, ff, seed);
            // 0x5ed058..0x5ed07c: fmA.h[i] = (fmA.h[i] + fmD.h[i]) / 2.
            fm_a.blend(&fm_d);
            Fields {
                sampled: fm_a,
                other: fm_d,
            }
        }
        _ => Fields {
            sampled: Fractal::generate(w, h, 2, ff, !seed),
            other: Fractal::generate(w, h, 2, wrap, seed.wrapping_add(SEED_EXTRA)),
        },
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
    /// Dimensions, options and the cell grid, with a class in every cell.
    pub grid: MapGrid,
    /// The height field the classes were derived from.
    pub field: Fractal,
    /// The three thresholds used.
    pub thresholds: Thresholds,
    /// Draws taken by the accepted attempt (0-based).
    pub draws: u32,
    /// Outer iterations used, 1-based.
    pub iterations: u32,
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
    let map_size = opts.map_size.clamp(0, 4) as usize;
    let (pl_hi, pl_lo) = (
        COAST_PERCENTILE[map_size],
        SEA_PERCENTILE[map_size],
    );

    let grid = MapGrid::new(w, h, 0, water_level);
    let mut grid = grid;

    // The retry loop draws until the coastline test passes, re-rolling the seed
    // by 113 each time. The inner loop is self-limiting: acceptance is
    // unconditional from the fifteenth draw.
    let mut n = 0u32;
    let (fields, thresholds) = loop {
        let fields = draw_fields(&grid, opts.landmass, n, water_level, bugs);
        let t = Thresholds {
            lake: fields.sampled.percentile(LAKE_PERCENTILE),
            coast: fields.sampled.percentile(pl_hi),
            sea: fields.sampled.percentile(pl_lo),
        };
        if accept_coast(P_STRIDE * n, t.coast) || n >= ALWAYS_ACCEPT_FROM {
            break (fields, t);
        }
        n += 1;
    };

    paint_land_and_sea(&mut grid, &fields.sampled, thresholds, water_level, bugs);

    Landmass {
        grid,
        field: fields.sampled,
        thresholds,
        draws: n,
        iterations: 1,
    }
}

/// The per-cell write: `0x5ed178..0x5ed2c7`.
fn paint_land_and_sea(
    grid: &mut MapGrid,
    fm: &Fractal,
    t: Thresholds,
    water_level: i32,
    bugs: OriginalBugs,
) {
    // rng = waterLevel + 0xCC98, warmed up twice.
    let mut rng = Rng::new((water_level as u32).wrapping_add(LANDMASS_SEED_BASE));
    rng.discard(LANDMASS_WARMUP);

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
    if let Some(Cell { class, .. }) = grid.cell_mut(i) {
        // vfunc(0x128) routes t >= 11 to the special slot untouched and
        // everything below 11 through the normaliser.
        *class = crate::cell::normalise_class(t, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::map_size;

    fn opts(size: i32, water: i32) -> Options {
        Options {
            map_size: size,
            water_level: water,
            ..Options::default()
        }
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
    fn the_retry_loop_never_overruns_its_cap() {
        let (w, h) = map_size(1);
        for water in 0..=100 {
            let lm = generate_landmass(w, h, &opts(1, water), water, OriginalBugs::NONE);
            assert!(
                lm.draws <= ALWAYS_ACCEPT_FROM,
                "water {water}: took {} draws",
                lm.draws
            );
            if lm.draws < ALWAYS_ACCEPT_FROM {
                assert!(
                    accept_coast(P_STRIDE * lm.draws, lm.thresholds.coast),
                    "water {water}: accepted a field it should have rejected"
                );
            }
        }
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
            assert!(c.class <= 13, "class {}", c.class);
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
        let shelf = lm.grid.cells.iter().filter(|c| c.class == 12).count();
        let deep = lm.grid.cells.iter().filter(|c| c.class == 13).count();
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
            lm.grid.cells.iter().filter(|c| c.class == 12).count()
        };
        assert!(
            shelf_of(&fixed) > shelf_of(&faithful) * 20,
            "corrected {} vs faithful {} shelf cells",
            shelf_of(&fixed),
            shelf_of(&faithful)
        );
        // And the deep class still exists, so the difference is a real split and
        // not the map having collapsed.
        assert!(faithful.grid.cells.iter().filter(|c| c.class == 13).count() > 0);
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
        let (w, h) = map_size(0);
        let lm = generate_landmass(w, h, &opts(0, 50), 50, OriginalBugs::NONE);
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
                assert!(m.draws <= ALWAYS_ACCEPT_FROM);
                assert!(m.land_cells() > 0);
            }
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
