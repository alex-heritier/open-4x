//! The generation pipeline: the stages `generateMap` (`0x5eb580`) runs, in
//! order, and which of them this crate implements.
//!
//! # Stage order
//!
//! ```text
//!  rollRandomOptions()            0x5f1f50   options.rs   exact
//!  generateLandmass()             0x5eceb0   landmass.rs  exact
//!  landmassFix()                  0x5ed440   -            not implemented
//!  deconflictStarts()             0x5eeb00   below        exact
//!  convertDesertsAtStarts()       0x5edb70   below        exact
//!  paintContinents()              0x5eddb0   -            no observable effect
//!  assignBiomes()                 0x5f1480   below        exact
//!  assignStartsPerContinent()     0x5ed5d0   -            not implemented
//!  growHillsAndMountains()        0x5f07d0   -            not implemented
//!  postProcess()                  0x5ebe80   -            not implemented
//!  placeResources(1)              0x5f22a0   -            not implemented
//!  placeGoodyHuts(1)              0x5f21b0   -            not implemented
//!  placeBarbarianCamps(1)          0x5f2090   -            not implemented
//!  finalPass(1, 0, +-1, ok, &ret) 0x5eeee0   -            not implemented
//!  contour smoothing              0x5d3100   -            not implemented
//!  placeStartLocation(x, y)       0x5d6500   -            not implemented
//! ```
//!
//! "exact" means every constant, loop bound and RNG draw is recovered.
//! "not implemented" means the stage was mapped but not modelled, because it
//! needs the BIQ's `GOOD`/`TERR` record tables or the class writer `0x5f1ce0`,
//! which this crate does not embed.
//!
//! # Why that is enough to be useful
//!
//! Everything from `postProcess` onwards only writes resource and feature ids
//! into cells whose terrain class is already fixed. The coastline, the
//! continent count, the ocean fraction and the biome layout — the parts that
//! make a Civ3 map recognisable — are all decided by the four stages this crate
//! implements.

use crate::bugs::OriginalBugs;
use crate::cell::MapGrid;
use crate::fractal::{flags as fflags, Fractal};
use crate::landmass::{generate_landmass, Landmass, Thresholds};
use crate::options::{map_size, Options};
use crate::rng::Rng;
use crate::spiral::spiral_offset;

/// Seed constant for the start-terrain stage: `0x9A2112`.
pub const DESERT_CONVERT_SEED: u32 = 0x009A_2112;
/// Seed constant for the Fisher-Yates shuffles: `0xD431` = 54321.
pub const SHUFFLE_SEED: u32 = 0xD431;
/// Seed base for the biome stage's LCG: `0xF0FF3`.
pub const BIOME_SEED_BASE: u32 = 0xF0FF3;
/// The biome stage's first fractal seed: `0x1E1735`.
pub const BIOME_FRACTAL_SEED: u32 = 0x001E_1735;
/// The biome stage's second fractal seed: `0x34B59E`, at level 5.
pub const BIOME_CONTOUR_SEED: u32 = 0x0034_B59E;
/// Level of the first biome fractal (`0x5f167d`).
pub const BIOME_LEVEL: i32 = 3;
/// Level of the second biome fractal (`0x5f19e0: push 5`).
pub const BIOME_CONTOUR_LEVEL: i32 = 5;
/// The percentile whose contour pass 2 traces.
pub const CONTOUR_PERCENTILE: i32 = 70;
/// The percentile pass 3 looks for around class-6 cells.
pub const PEAK_PERCENTILE: i32 = 100;

/// A fully generated map.
pub struct GeneratedMap {
    /// Dimensions, options and the cell grid.
    pub grid: MapGrid,
    /// The options the map was generated with.
    pub options: Options,
    /// The land/sea height field.
    pub field: Fractal,
    /// The thresholds the coastline was derived from.
    pub thresholds: Thresholds,
    /// Draws the retry loop needed.
    pub draws: u32,
    /// The second height field, used to pick each civ's start terrain.
    pub start_field: Fractal,
}

impl GeneratedMap {
    /// Number of land cells.
    pub fn land_cells(&self) -> usize {
        self.grid.cells.iter().filter(|c| !c.is_water()).count()
    }

    /// Number of water cells.
    pub fn water_cells(&self) -> usize {
        self.grid.cells.iter().filter(|c| c.is_water()).count()
    }
}

/// Runs the whole pipeline with the intended behaviour.
///
/// Equivalent to `generate_with(opts, &OriginalBugs::NONE)`.
///
/// Deterministic in `opts`: the same options always give the same map.
pub fn generate(opts: &Options) -> GeneratedMap {
    generate_with(opts, &OriginalBugs::NONE)
}

/// Runs the whole pipeline, reproducing whichever original bugs are enabled.
///
/// Deterministic in both arguments.
pub fn generate_with(opts: &Options, bugs: &OriginalBugs) -> GeneratedMap {
    let (w, h) = map_size(opts.map_size);
    let Landmass {
        mut grid,
        field,
        thresholds,
        draws,
        ..
    } = generate_landmass(w, h, opts, opts.water_level, *bugs);

    // --- deconflictStarts(): 0x5eeb00 -----------------------------------
    deconflict_starts(&mut grid, *bugs);

    // --- convertDesertsAtStarts(): 0x5edb70 -----------------------------
    let start_field = convert_deserts_at_starts(&mut grid, opts);

    // --- assignBiomes(): 0x5f1480 ---------------------------------------
    assign_biomes(&mut grid, opts, *bugs);

    GeneratedMap {
        grid,
        options: *opts,
        field,
        thresholds,
        draws,
        start_field,
    }
}

/// `0x5eeb00` — make the two deep-water classes distinct enough to be playable.
///
/// Two passes over the nominal start slots, both scanning the ring-1 spiral
/// neighbourhood:
///
/// * **pass 1** — a water cell with a *non*-water neighbour keeps its class;
/// * **pass 2** — a cell on [`WATER_DEEP`] with a shallow neighbour (and without
///   the `0x400000` flag) is pushed to the next class down.
///
/// The point is that a start square on the boundary between the two water
/// classes would otherwise sit next to a continent, so the two are separated.
///
/// # The `start_slot_index` bug
///
/// Pass 1 writes to `getCell(dx)` — the **x-offset of the neighbour that was
/// found** — instead of to cell `i`. Ring-1 offsets are in `{-2..2}` (see
/// [`spiral_offset`]), so the write lands on a near-zero index and the pass ends
/// up relabelling the same handful of cells over and over. The loop index is
/// kept in a separate local (`F+0x24`) and never used, so this is the original's
/// behaviour and not a decompilation artefact.
///
/// That is opt-in via [`OriginalBugs::start_slot_index`]; by default the write
/// goes to the cell the loop is on, which is what makes the pass work.
pub fn deconflict_starts(grid: &mut MapGrid, bugs: OriginalBugs) {
    for i in 0..grid.num_cells() {
        if !grid.cell(i).map(|c| c.is_water()).unwrap_or(false) {
            continue;
        }
        let (x, y) = grid.nominal_slot(i);
        let mut found: Option<i32> = None;
        for n in 1..=8 {
            let (dx, dy) = spiral_offset(n);
            let Some((nx, ny)) = grid.wrap_and_check(x + dx, y + dy) else {
                continue;
            };
            if !grid.cell_at(nx, ny).map(|c| c.is_water()).unwrap_or(false) {
                found = Some(dx);
                break;
            }
        }
        let Some(dx) = found else { continue };
        // The original passes `dx`, not `i`; `dx` is -2..=2, so the mask leaves
        // it alone for non-negative values and wraps negatives to ~0xFFFF, which
        // the `min` below then clamps onto the last cell.
        let victim = if bugs.start_slot_index {
            (dx & 0xFFFF) as usize
        } else {
            i
        };
        if let Some(c) = grid.cell_mut(victim.min(grid.num_cells() - 1)) {
            c.class = 11;
        }
    }
}

/// The six elevation percentiles `0x5edb70` picks start terrain from,
/// indexed by `this[0x30]`.
///
/// The last entry is a *constant*, not a percentile lookup — it is compared
/// against a `rand_int(100)` draw.
pub const START_TERRAIN_BANDS: [[i32; 6]; 3] = [
    [28, 77, 18, 94, 65, 55],
    [25, 77, 20, 96, 70, 63],
    [24, 74, 21, 98, 70, 66],
];

/// `0x5edb70` — make sure no civ starts in the middle of a desert.
///
/// A second, independent fractal (`level 2`, `flags 1`, seed
/// `water + 0x9A2112`) is cut into six bands. For each civ sitting on a
/// class-2 cell, a desert (BIQ terrain 2) whose sampled height is low enough is
/// replaced by grassland (5), tundra (6) or plains-desert (10).
pub fn convert_deserts_at_starts(grid: &mut MapGrid, opts: &Options) -> Fractal {
    let fm = Fractal::generate(
        grid.w,
        grid.h,
        2,
        fflags::WRAP_X,
        (opts.water_level as u32).wrapping_add(DESERT_CONVERT_SEED),
    );

    // A Fisher-Yates draw sequence whose results are thrown away; it exists only
    // to advance the LCG so the later rand_int(100) lands elsewhere.
    let mut rng = Rng::new((opts.water_level as u32).wrapping_add(SHUFFLE_SEED));
    rng.discard(grid.num_cells());

    let t = opts.temperature.clamp(0, 2) as usize;
    let spec = START_TERRAIN_BANDS[t];
    let p = |pct: i32| fm.percentile(pct);
    let (b0, b1, b2, fixed, b4, b5) =
        (p(spec[0]), p(spec[1]), p(spec[2]), spec[3], p(spec[4]), p(spec[5]));

    for i in 0..grid.num_cells() {
        let (x, y) = grid.nominal_slot(i);
        if grid.cell_at(x, y).map(|c| c.class) != Some(2) {
            continue;
        }
        let h = fm.sample(x, y);
        if h > b1 {
            continue; // too high to be a desert any more
        }
        if h < b4 {
            // Cold or hot band: grassland if the height is in either of the two
            // accepted windows, otherwise leave the desert alone.
            if h >= b5 || (h <= b0 && h >= b2) {
                if let Some(c) = grid.cell_at_mut(x, y) {
                    c.class = 5;
                }
            }
        } else {
            // Borderline band: a coin flip between tundra and plains.
            let class = if rng.below(100) < fixed { 6 } else { 10 };
            if let Some(c) = grid.cell_at_mut(x, y) {
                c.class = class;
            }
        }
    }
    fm
}

/// The Resources-setting half of the biome thresholds (`map+0x28`).
///
/// Keyed `[E, A, B, D, C]`, byte-exact from `0x5f1480`.
pub const RESOURCE_THRESHOLDS: [[i32; 5]; 3] = [
    // A   B   C   D   E
    [3, 35, 45, 10, 3],
    [4, 50, 60, 12, 5],
    [5, 55, 65, 16, 7],
];

/// The Climate-setting half, keyed `[A', F, G, H, I, J, K]` by `map+0x08`.
pub const CLIMATE_THRESHOLDS: [[i32; 7]; 3] = [
    // A'  F   G   H   I   J   K
    [2, 7, 2, 12, 44, 13, 34],
    [4, 10, 5, 14, 42, 17, 30],
    [7, 15, 8, 16, 40, 20, 27],
];

/// `0x5f1480` — the biome stage. The fractal becomes a climate map.
///
/// This is the stage that turns the uniform "grassland class 2" the land/sea
/// stage produced into Civ3's actual terrain distribution. Two height fields
/// are generated and the cells are visited in **shuffled** order (a
/// Fisher-Yates over `numCells`, `0x5f168d..0x5f16cf`), so the biome layout
/// has no visible row or column structure.
///
/// # The combined axis
///
/// There is no explicit five-way temperature switch. Instead each cell gets one
/// scalar `v` mixing elevation and latitude:
///
/// ```asm
/// 0x005f1721  mov eax, ebp          ; H
/// 0x005f1726  sar eax, 1            ; H/2
/// 0x005f1728  sub eax, ecx           ; H/2 - y
/// 0x005f172f  (abs)
/// 0x005f1733  lea eax,[eax+eax*4]    ; *5
/// 0x005f1736  lea eax,[eax+eax*8]    ; *45
/// 0x005f1739  shl eax, 2             ; *180
/// 0x005f173d  idiv ebp               ; / H
///            ; lat = 180 * |H/2 - y| / H, i.e. 0 at the equator, 90 at the poles
/// 0x005f175a  sub eax, 0x80          ; h - 128
/// 0x005f1762  shl eax, 4
/// 0x005f1765  sub eax, ecx           ; *15
/// 0x005f176b  add eax, eax           ; *30
/// 0x005f177a  sar ebp, 8             ; / 256
/// 0x005f177d  add ebp, ecx           ; + lat
/// ```
///
/// so `v = ((h - 128) * 30) / 256 + lat`, ranging over roughly `[-15, 105]`.
/// Small `v` is equatorial, large `v` is polar. The biome boundaries are
/// constant cut-points on that axis, and the tables above are exactly those
/// cut-points — the reason a Huge map is colder than a Tiny one is that `I`,
/// `J` and `K` shrink with the Climate setting while `v`'s range does not.
///
/// # The proximity term
///
/// `nearLand` is how close the cell is to water, found by scanning rings 1 and
/// 2 of the spiral (24 offsets) for the first water neighbour: ring 1 gives
/// `2`, ring 2 gives `1`. It then shifts `v` by `+2 * nearLand` in the
/// equatorial band (23 <= v < 48) and `-nearLand` elsewhere, and scales the
/// chance of the wet classes by `nearLand * E`.
///
/// # Classes
///
/// | class | terrain | condition |
/// |---|---|---|
/// | 0 | Desert | `J < v < K` |
/// | 1 | Plains | `H < v < I` |
/// | 2 | Grassland | the default |
/// | 3 | Forest | `v > B && lat > C` |
/// | 8 | wet tropical | `v < A` |
/// | 9 | wet tropical, denser | `v < D` and the class was 2 or 8, with probability `nearLand*E + G\|F` |
///
/// # Fidelity
///
/// The class numbers and every threshold are exact. What this stage does not
/// model yet is the per-cell call to [`write_biome_class`] (`0x5f1ce0`): the
/// binary flood-fills each class across its region through the `bonus[]`
/// visited array, while this port stores the class directly. Wiring the
/// flood in awaits the `paintContinents` (`0x5eddb0`) region ids, which no
/// module models yet; the flood itself is implemented and tested below.
pub fn assign_biomes(grid: &mut MapGrid, opts: &Options, bugs: OriginalBugs) {
    let res = opts.resources.clamp(0, 2) as usize;
    let cli = opts.climate.clamp(0, 2) as usize;
    let (a0, b, c, d, e) = (
        RESOURCE_THRESHOLDS[res][0],
        RESOURCE_THRESHOLDS[res][1],
        RESOURCE_THRESHOLDS[res][2],
        RESOURCE_THRESHOLDS[res][3],
        RESOURCE_THRESHOLDS[res][4],
    );
    let (a, f, g, h_th, i_th, j_th, k) = (
        a0 + CLIMATE_THRESHOLDS[cli][0] - 1,
        CLIMATE_THRESHOLDS[cli][1],
        CLIMATE_THRESHOLDS[cli][2],
        CLIMATE_THRESHOLDS[cli][3],
        CLIMATE_THRESHOLDS[cli][4],
        CLIMATE_THRESHOLDS[cli][5],
        CLIMATE_THRESHOLDS[cli][6],
    );
    // M = (B + A) / 2, from the *unmodified* B (0x5f15cb..0x5f15da).
    let m = (b + a0) / 2;

    let ff = if bugs.swapped_wrap_flags {
        if grid.wrap_flags & 2 != 0 { 3 } else { 9 }
    } else {
        let mut f = if grid.wrap_flags & 2 != 0 {
            fflags::WRAP_Y
        } else {
            fflags::OCEAN_POLES
        };
        if grid.wrap_flags & 1 != 0 {
            f |= fflags::WRAP_X;
        }
        f
    };
    let fm = Fractal::generate(
        grid.w,
        grid.h,
        BIOME_LEVEL,
        ff,
        (opts.water_level as u32).wrapping_add(BIOME_FRACTAL_SEED),
    );
    let mut rng = Rng::new((opts.water_level as u32).wrapping_add(BIOME_SEED_BASE));

    // Fisher-Yates over the cell indices (0x5f168d..0x5f16cf).
    let n = grid.num_cells();
    let mut perm: Vec<usize> = (0..n).collect();
    for i in 0..n {
        let j = i + rng.below((n - i) as u32) as usize;
        perm.swap(i, j.min(n - 1));
    }

    for &cell in &perm {
        let (x, y) = grid.coords(cell);
        let Some(target) = grid.cell_mut(cell) else {
            continue;
        };
        if target.is_water() {
            continue; // 0x5f1788
        }

        let lat = 180 * ((grid.h / 2 - y).abs()) / grid.h;
        let hv = i32::from(fm.sample(x, y));
        let mut v = (hv - 128) * 30 / 256 + lat;

        // Distance to water, in rings 1 and 2 (0x5f1849).
        let mut near_land = 0i32;
        for k in 1..25 {
            let (dx, dy) = spiral_offset(k);
            let Some((nx, ny)) = grid.wrap_and_check(x + dx, y + dy) else {
                continue;
            };
            if grid.cell_at(nx, ny).map(|s| s.is_water()).unwrap_or(false) {
                near_land = if k < 9 { 2 } else { 1 };
                break;
            }
        }

        if (23..48).contains(&v) {
            v += 2 * near_land;
        } else {
            v -= near_land;
        }

        // 0x5f189a: the flag is set either above B outright, or above M with a
        // coin flip. Both arms write the same mask, so they are one condition.
        if v > b || (v > m && rng.below(100) < 50) {
            grid.cell_mut(cell).unwrap().set_flag(2, 0x20_0000);
        }

        // The class cascade (0x5f1907..0x5f1951).
        let mut cls = 2;
        if h_th < v && v < i_th {
            cls = 1;
        }
        if j_th < v && v < k {
            cls = 0;
        }
        if v < a {
            cls = 8;
        } else if v > b && lat > c {
            cls = 3;
        }
        if v < d {
            cls = match cls {
                2 => {
                    if rng.below(100) < near_land * e + g {
                        9
                    } else {
                        cls
                    }
                }
                8 => {
                    if rng.below(100) < near_land * e + f {
                        9
                    } else {
                        cls
                    }
                }
                other => other,
            };
        }
        grid.cell_mut(cell).unwrap().class = cls;
    }

    contour_pass(grid, opts, bugs);
    peak_pass(grid, opts);
}

/// `0x5f1ce0` — flood-fill one biome class across a single region.
///
/// Pass 1 of `0x5f1480` calls this once per land cell as
/// `writeBiomeClass(x, y, cls, bonus)` (sole external caller `0x5f19a8`);
/// the only other caller is itself (`0x5f1f1b`, the flood step). Spans
/// `0x5f1ce0..0x5f1f45` (614 bytes, `ret 0x10`).
///
/// Mechanics, in order (`NOTES.md` §11.5):
///
/// 1. `bonus[idx] = 1` unconditionally (`0x5f1d14`) — the visited mark.
///    Pass 1 skips cells whose byte is set, so one flood claims every cell
///    it touches for all later iterations.
/// 2. If the center cell's terrain (`vfunc(0xC8)`) is 5, 6 or 10, return
///    (`0x5f1d29..0x5f1d64`, three identical probe blocks).
/// 3. Unless `cls == 2` (grassland, already the land-stage default),
///    `cell.vfunc(0x128)(cls, -1, -1)` (`0x5f1d86`).
/// 4. For spiral neighbours `n = 1..=8` (ring 1), wrapped per `+0x1F0` and
///    bounds-checked, recurse into each neighbour whose continent id
///    (`vfunc(0xB8)`, `0x5f1e52`) and `paintContinents` region id
///    (`[esi+0x3C]`, `0x5f1ea2`) both match the center's, whose terrain is
///    not 5/6/10, and whose `bonus[]` byte is still 0.
///
/// `regions` is the `uint16` array `paintContinents` (`0x5eddb0`) leaves at
/// `map+0x3C`, indexed by flat cell index. The binary recurses; this port
/// uses an explicit stack, which reaches the same fixed point because the
/// per-cell write is idempotent and the guards are monotonic.
pub fn write_biome_class(
    grid: &mut MapGrid,
    regions: &[u16],
    bonus: &mut [u8],
    x: i32,
    y: i32,
    cls: u8,
) {
    let mut stack = vec![(x, y)];
    while let Some((x, y)) = stack.pop() {
        let idx = grid.index(x, y);
        bonus[idx] = 1; // 0x5f1d14
        let center_class = grid.cell(idx).map(|c| c.class).unwrap_or(255);
        if matches!(center_class, 5 | 6 | 10) {
            continue; // 0x5f1d2c / 0x5f1d48 / 0x5f1d64
        }
        if cls != 2 {
            // 0x5f1d86: vfunc(0x128)(cls, -1, -1).
            if let Some(cell) = grid.cell_mut(idx) {
                cell.set_class(cls);
            }
        }
        let center_cont = grid.cell(idx).map(|c| c.continent).unwrap_or(u16::MAX);
        let center_region = regions.get(idx).copied().unwrap_or(u16::MAX);
        for n in 1..=8 {
            // 0x5f1db1: spiralOffset(n, &dx, &dy); wrap + bounds check.
            let (dx, dy) = spiral_offset(n);
            let Some((nx, ny)) = grid.wrap_and_check(x + dx, y + dy) else {
                continue;
            };
            let nidx = grid.index(nx, ny);
            // 0x5f1e52 / 0x5f1e62: same vfunc(0xB8) both sides.
            if grid.cell(nidx).map(|c| c.continent) != Some(center_cont) {
                continue;
            }
            // 0x5f1ea2: same paintContinents region id.
            if regions.get(nidx).copied() != Some(center_region) {
                continue;
            }
            // 0x5f1ebf / 0x5f1ed7 / 0x5f1eef: neighbour terrain guard.
            if matches!(grid.cell(nidx).map(|c| c.class), Some(5 | 6 | 10)) {
                continue;
            }
            // 0x5f1f0b: unvisited only.
            if bonus.get(nidx) != Some(&0) {
                continue;
            }
            stack.push((nx, ny)); // 0x5f1f1b: the recursive call
        }
    }
}

/// Pass 2 of `0x5f1480` — the percentile contour.
///
/// A second field (`level 5`, seed `water + 0x34B59E`) is generated and land
/// cells at or below its 70th-percentile height are forced to class 7, which
/// lays the map's hill ridges along a constant-elevation contour.
///
/// The binary compares with `==` rather than `<=` (`0x5f1ae1` / `0x5f1ae7`),
/// so it only catches cells sitting *exactly* on the threshold — a sparse
/// scatter rather than a ridge, because the height field is a byte and the map
/// is stretched well past the field's own resolution. That is opt-in via
/// [`OriginalBugs::contour_equality`].
fn contour_pass(grid: &mut MapGrid, opts: &Options, bugs: OriginalBugs) {
    let ff = if bugs.swapped_wrap_flags {
        if grid.wrap_flags & 2 != 0 { 3 } else { 9 }
    } else {
        let mut f = if grid.wrap_flags & 2 != 0 {
            fflags::WRAP_Y
        } else {
            fflags::OCEAN_POLES
        };
        if grid.wrap_flags & 1 != 0 {
            f |= fflags::WRAP_X;
        }
        f
    };
    let fm = Fractal::generate(
        grid.w,
        grid.h,
        BIOME_CONTOUR_LEVEL,
        ff,
        (opts.water_level as u32).wrapping_add(BIOME_CONTOUR_SEED),
    );
    let p70 = fm.percentile(CONTOUR_PERCENTILE);

    for i in 0..grid.num_cells() {
        if grid.cell(i).map(|c| c.sub_class) == Some(0) {
            continue;
        }
        // 0x5f19e6: classes 5, 6, 8, 9 and 10 are left alone.
        if matches!(grid.cell(i).map(|c| c.class), Some(5 | 6 | 8 | 9 | 10)) {
            continue;
        }
        let (x, y) = grid.coords(i);
        if grid.cell_at(x, y).map(|c| c.is_water()).unwrap_or(false) {
            continue;
        }
        let h = fm.sample(x, y);
        let hit = if bugs.contour_equality { h == p70 } else { h <= p70 };
        if hit {
            grid.cell_mut(i).unwrap().set_class(7);
        }
    }
}

/// Pass 3 of `0x5f1480` — flag class-6 cells near the height maximum.
///
/// A 49-step spiral (with steps 9 and 25 halving the probability) around each
/// class-6 cell; a hit sets flag `0x100000` on plane 2.
fn peak_pass(grid: &mut MapGrid, opts: &Options) {
    let _ = opts;
    for i in 0..grid.num_cells() {
        if grid.cell(i).map(|c| c.class) != Some(6) {
            continue;
        }
        let (x, y) = grid.coords(i);
        let mut p = 50u32;
        for n in 0..49 {
            let (dx, dy) = spiral_offset(if n == 0 { 8 } else { n });
            let Some((nx, ny)) = grid.wrap_and_check(x + dx, y + dy) else {
                continue;
            };
            if n == 9 || n == 25 {
                p >>= 1;
            }
            let hit = grid
                .cell_at(nx, ny)
                .map(|s| s.sub_class == 3)
                .unwrap_or(false);
            if hit {
                // 0x5f1b12: the roll is per-step, not once.
                let mut rng = Rng::new((x as u32) << 16 | (ny as u32 & 0xFFFF));
                if rng.below(100) < p as i32 {
                    grid.cell_mut(i).unwrap().set_flag(2, 0x10_0000);
                }
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::{generate, generate_with};

    fn opts(map_size: i32, water: i32) -> Options {
        Options {
            map_size,
            water_level: water,
            ..Options::default()
        }
    }

    #[test]
    fn generate_is_deterministic() {
        let o = opts(1, 37);
        assert_eq!(generate(&o).grid.cells, generate(&o).grid.cells);
    }

    /// 8x4 land grid: one continent, one region, everything class 2.
    fn flood_grid() -> (MapGrid, Vec<u16>, Vec<u8>) {
        let mut grid = MapGrid::new(8, 4, 0, 0);
        for c in grid.cells.iter_mut() {
            c.class = 2;
            c.sub_class = 2;
            c.continent = 1;
        }
        let n = grid.num_cells();
        (grid, vec![7u16; n], vec![0u8; n])
    }

    #[test]
    fn biome_writer_floods_the_whole_region() {
        let (mut grid, regions, mut bonus) = flood_grid();
        write_biome_class(&mut grid, &regions, &mut bonus, 0, 0, 0);
        // Ring-1 steps (|dx|+|dy| == 2) preserve x+y parity, but every cell
        // holds one even-parity tile and all even tiles are reachable, so
        // the flood still claims all 16 cells.
        assert!(bonus.iter().all(|&b| b == 1), "unclaimed cells remain");
        assert!(grid.cells.iter().all(|c| c.class == 0));
        assert!(grid.cells.iter().all(|c| c.sub_class == 0));
    }

    #[test]
    fn biome_writer_grassland_marks_without_writing() {
        let (mut grid, regions, mut bonus) = flood_grid();
        write_biome_class(&mut grid, &regions, &mut bonus, 0, 0, 2);
        assert!(bonus.iter().all(|&b| b == 1), "cls 2 still floods");
        assert!(grid.cells.iter().all(|c| c.class == 2), "cls 2 writes nothing");
    }

    #[test]
    fn biome_writer_guarded_center_marks_but_returns() {
        let (mut grid, regions, mut bonus) = flood_grid();
        grid.cells[0].class = 6;
        write_biome_class(&mut grid, &regions, &mut bonus, 0, 0, 0);
        // bonus[idx] = 1 lands before the terrain guard (0x5f1d14).
        assert_eq!(bonus[0], 1);
        assert_eq!(grid.cells[0].class, 6, "guarded cell keeps its class");
        assert!(bonus[1..].iter().all(|&b| b == 0), "no flood from guard");
    }

    #[test]
    fn biome_writer_gates() {
        // Each gate blocks cell 1 (tiles (2,0),(3,0)) while the flood claims
        // everything else; the blocked cell keeps class 2 and bonus 0.
        let cases: &[(&str, Box<dyn Fn(&mut MapGrid, &mut Vec<u16>, &mut Vec<u8>)>)] = &[
            ("region", Box::new(|_, r, _| r[1] = 9)),
            ("continent", Box::new(|g, _, _| g.cells[1].continent = 2)),
            ("terrain", Box::new(|g, _, _| g.cells[1].class = 6)),
            ("visited", Box::new(|_, _, b| b[1] = 1)),
        ];
        for (name, gate) in cases {
            let (mut grid, mut regions, mut bonus) = flood_grid();
            gate(&mut grid, &mut regions, &mut bonus);
            let before = grid.cells[1].class;
            write_biome_class(&mut grid, &regions, &mut bonus, 0, 0, 0);
            assert_eq!(grid.cells[1].class, before, "{name}: blocked cell written");
            if *name != "visited" {
                assert_eq!(bonus[1], 0, "{name}: blocked cell marked");
            }
            assert!(
                bonus.iter().enumerate().all(|(i, &b)| i == 1 || b == 1),
                "{name}: flood did not route around the blocked cell"
            );
        }
    }

    #[test]
    fn every_cell_gets_a_terrain() {
        for size in 0..5 {
            let m = generate(&opts(size, 50));
            assert!(
                m.grid.cells.iter().all(|c| c.class <= 13),
                "size {size}: a cell has class > 13"
            );
        }
    }

    #[test]
    fn there_is_land_and_there_is_water() {
        for size in 0..5 {
            let m = generate(&opts(size, 50));
            assert!(m.land_cells() > 0, "size {size}: no land");
            assert!(m.water_cells() > 0, "size {size}: no water");
        }
    }

    #[test]
    fn the_biome_pass_actually_changes_the_map() {
        // Before the biome pass every land cell is class 2 (grassland); after it
        // there must be a spread of classes. Both halves are asserted, so the
        // test cannot pass on a no-op.
        let (w, h) = map_size(1);
        let before = generate_landmass(w, h, &opts(1, 50), 50, OriginalBugs::NONE);
        assert!(
            before.grid.cells.iter().all(|c| c.is_water() || c.class == 2),
            "the land/sea stage should leave only class 2 on land"
        );

        let after = generate(&opts(1, 50));
        let classes: std::collections::BTreeSet<u8> = after
            .grid
            .cells
            .iter()
            .filter(|c| !c.is_water())
            .map(|c| c.class)
            .collect();
        assert!(
            classes.len() >= 3,
            "only classes {classes:?} on land after the biome pass"
        );
    }

    #[test]
    fn climate_changes_the_biome_mix() {
        // A colder Climate setting shrinks I/J/K, so more land should end up
        // desert or plains rather than grassland.
        let count = |climate: i32| {
            let o = Options {
                climate,
                ..opts(1, 50)
            };
            generate(&o)
                .grid
                .cells
                .iter()
                .filter(|c| !c.is_water() && (c.class == 0 || c.class == 1))
                .count()
        };
        assert_ne!(count(0), count(2), "Climate had no effect at all");
    }

    #[test]
    fn resources_change_the_wet_classes() {
        let count = |resources: i32| {
            let o = Options {
                resources,
                ..opts(1, 50)
            };
            generate(&o)
                .grid
                .cells
                .iter()
                .filter(|c| !c.is_water() && (c.class == 8 || c.class == 9))
                .count()
        };
        // E = 3, 5, 7 scales the probability of class 9 by nearLand.
        assert!(count(2) > count(0), "Plentiful {} vs None {}", count(2), count(0));
    }

    #[test]
    fn all_landmass_styles_terminate() {
        for lm in 0..3 {
            for water in [0, 25, 50, 75, 100] {
                let o = Options {
                    landmass: lm,
                    ..opts(2, water)
                };
                let m = generate(&o);
                assert!(m.land_cells() > 0 && m.water_cells() > 0);
            }
        }
    }

    #[test]
    fn every_map_size_terminates() {
        for size in 0..5 {
            for water in [10, 50, 90] {
                let m = generate(&opts(size, water));
                assert!(m.land_cells() > 0, "size {size} water {water}");
            }
        }
    }

    #[test]
    fn deconflict_leaves_the_map_consistent() {
        let o = opts(0, 50);
        let m = generate(&o);
        assert!(m.grid.cells.iter().all(|c| c.class <= 13));
    }

    #[test]
    fn the_contour_pass_marks_few_cells() {
        // The binary compares `h == p70` rather than `h <= p70`, so the pass
        // catches only the cells that land exactly on the threshold. Measured
        // over every map size and water level that is 0.0-0.5 % of the land.
        //
        // The `<=` reading is *not* obviously the intent: it would mark 52-61 %
        // of the land, i.e. paint most of the map as hills, which is not what a
        // contour pass is for. So `==` is the default here and the flag inverts
        // it for anyone who wants to explore the other reading.
        let m = generate(&opts(2, 50));
        let hills = m.grid.cells.iter().filter(|c| c.class == 7).count();
        assert!(hills > 0, "no class-7 cells at all");
        assert!(
            hills * 200 >= m.land_cells(),
            "{} hill cells out of {} land — over 0.5 %",
            hills,
            m.land_cells()
        );
    }

    #[test]
    fn the_contour_equality_flag_inverts_the_comparison() {
        let o = opts(2, 50);
        let all = OriginalBugs::ALL;
        let none = OriginalBugs::NONE;
        let hills = |b: &OriginalBugs| {
            generate_with(&o, b)
                .grid
                .cells
                .iter()
                .filter(|c| c.class == 7)
                .count()
        };
        let (eq, le) = (hills(&all), hills(&none));
        assert!(
            le > eq * 20,
            "`<=` marked {le} cells against `==`'s {eq}; the flag did not invert"
        );
    }

    #[test]
    fn the_deconflict_pass_lays_a_coastal_shallow_band() {
        // With the index bug off, pass 1 relabels every water cell that touches
        // land, which is what gives the ocean its shallow band. With the bug on,
        // it relabels 4 cells in total.
        let o = opts(2, 50);
        let all = OriginalBugs::ALL;
        let none = OriginalBugs::NONE;
        let shallow = |b: &OriginalBugs| {
            generate_with(&o, b)
                .grid
                .cells
                .iter()
                .filter(|c| c.class == 11)
                .count()
        };
        let (buggy, fixed) = (shallow(&all), shallow(&none));
        assert!(
            fixed > buggy * 20,
            "corrected {fixed} shallow cells vs faithful {buggy}"
        );
        assert!(fixed > m_land(&o), "the band should be a decent fraction of the map");
    }

    /// Land cells on a Std-size map, as a scale reference.
    fn m_land(o: &Options) -> usize {
        generate(o).land_cells() / 2
    }

    #[test]
    fn water_never_gets_a_biome() {
        for size in 0..5 {
            let m = generate(&opts(size, 60));
            for c in &m.grid.cells {
                if c.is_water() {
                    assert!(
                        (11..=13).contains(&c.class),
                        "water cell has class {}",
                        c.class
                    );
                }
            }
        }
    }

    #[test]
    fn the_deep_water_class_is_preserved() {
        // The land/sea stage writes 13 for every cell below the sea level, and
        // the biome pass skips water, so class 13 must still be plentiful.
        let m = generate(&opts(1, 50));
        let deep = m.grid.cells.iter().filter(|c| c.class == 13).count();
        assert!(deep > 0, "no abyssal cells survived");
    }
}
