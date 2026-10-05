//! Terrain assignment: `assignBiomes`, `0x5F1480`, with the region flood
//! `0x5F1CE0`.
//!
//! After the land/sea stage every land cell is grassland, and `0x5EDB70` has
//! scattered hills, mountains and volcanoes over it. This stage decides what
//! the rest of the land is: desert, plains, grassland, tundra, jungle, marsh,
//! forest. It has three passes.
//!
//! # Pass 1 - climate bands
//!
//! A height field (`level 3`, seed `seed + 0x1E1735`) and a Fisher-Yates
//! shuffle of the cell indices (seed `seed + 0xF0FF3`; the same LCG goes on to
//! supply every later draw of this stage) set the order. For each cell, in that
//! order, with `(x, y)` its nominal tile:
//!
//! ```text
//! lat = |H/2 - y| * 180 / H                       0 at the equator, 90 at the poles
//! v   = (height(x, y) - 128) * 30 / 256 + lat     truncating
//! ```
//!
//! Water cells are skipped. `near` is `2` when one of the eight ring-1 cells is
//! water, `1` when one of the sixteen ring-2 cells is, else `0`, and shifts
//! `v` by `+2*near` in the band `23 <= v < 48`, by `-near` outside it.
//!
//! The cell gets the forest-flag (`0x200000` in the feature plane) when
//! `v > B`, or when `v > M` and a `rand(100)` comes up below 50. Then, unless
//! an earlier flood has already claimed the cell, its class is
//!
//! ```text
//! 2 (grassland)
//!   1 (plains)   if H < v < I
//!   0 (desert)   if J < v < K
//!   8 (jungle)   if v < A
//!   3 (tundra)   else if v > B and lat > C
//!   9 (marsh)    if v < D and the class is 2 or 8 and rand(100) < near*E + G (or F)
//! ```
//!
//! and [`write_biome_class`] paints that class over the whole region the cell
//! belongs to. The thresholds come from the Temperature (`A`..`E`) and Climate
//! (`F`..`K`) options, see [`Thresholds`]. Because a flood marks every cell it
//! reaches, a region gets *one* class, from whichever of its cells came first
//! in the shuffled order and was not water; the later cells in the region only
//! contribute their RNG draws.
//!
//! # Pass 2 - forest contour
//!
//! A second height field (`level 5`, seed `seed + 0x34B59E`). A cell becomes
//! forest (class 7) when its base terrain (sub-class) is not desert, its class
//! is not hills, mountains, jungle, marsh or volcano, and its height (minus 10
//! for plains) lies between the field's 70th and 100th percentile.
//!
//! # Pass 3 - snow caps
//!
//! A mountain cell gets the snow-cap bit (`0x100000`) with probability `p%`,
//! tried for each cell of the spiral `0..=48` whose base terrain is tundra (or
//! that is itself snow-capped, within radius 2). `p` starts at 50 and halves
//! when the walk reaches spiral index 9 and index 25.
//!
//! Both fractals use fractal flags `3` or `9` (always wrapping in x; y wrap or
//! ocean poles) - the same bit mix-up as the land/sea stage, see
//! [`crate::landmass::fractal_flags`].

use crate::cell::{MapGrid, PLANE_FEATURE};
use crate::fractal::{flags as fflags, Fractal};
use crate::rng::Rng;
use crate::spiral::spiral_offset;

/// Seed offset of the stage's LCG (shuffle and every draw): `0xF0FF3`.
pub const SEED_BASE: u32 = 0x000F_0FF3;
/// Seed offset of the pass-1 height field: `0x1E1735`.
pub const FRACTAL_SEED: u32 = 0x001E_1735;
/// Seed offset of the pass-2 height field: `0x34B59E`.
pub const CONTOUR_SEED: u32 = 0x0034_B59E;
/// Level of the pass-1 field.
pub const FRACTAL_LEVEL: i32 = 3;
/// Level of the pass-2 field.
pub const CONTOUR_LEVEL: i32 = 5;
/// Percentile that pass 2 starts forest at.
pub const FOREST_FROM_PERCENTILE: i32 = 70;
/// Feature-plane bit: forest-flag (`pine forest`).
pub const FLAG_PINE: u32 = 0x20_0000;
/// Feature-plane bit: snow-capped.
pub const FLAG_SNOW_CAP: u32 = 0x10_0000;

/// The cut-points on the `v` axis, from the Temperature and Climate options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Thresholds {
    /// Below this `v`: jungle.
    pub a: i32,
    /// Above this `v`: forest-flag, tundra (with `c`).
    pub b: i32,
    /// Tundra needs `lat` above this.
    pub c: i32,
    /// Below this `v`: grassland and jungle may turn to marsh.
    pub d: i32,
    /// Marsh chance per `near` step.
    pub e: i32,
    /// Marsh base chance for jungle.
    pub f: i32,
    /// Marsh base chance for grassland.
    pub g: i32,
    /// Plains from here...
    pub h: i32,
    /// ...up to here.
    pub i: i32,
    /// Desert from here...
    pub j: i32,
    /// ...up to here.
    pub k: i32,
    /// `(b + a) / 2`: the half-chance forest-flag threshold.
    pub m: i32,
}

impl Thresholds {
    /// `Temperature` selects `a0, b, c, d, e`; `Climate` selects `f..k` and
    /// shifts `a` by `-1, 0, +2`.
    pub fn new(temperature: i32, climate: i32) -> Self {
        let (a0, b, c, d, e) = match temperature {
            0 => (3, 35, 45, 10, 3),
            1 => (4, 50, 60, 12, 5),
            _ => (5, 55, 65, 16, 7),
        };
        let (da, f, g, h, i, j, k) = match climate {
            0 => (-1, 7, 2, 12, 44, 13, 34),
            1 => (0, 10, 5, 14, 42, 17, 30),
            _ => (2, 15, 8, 16, 40, 20, 27),
        };
        let a = a0 + da;
        Thresholds {
            a,
            b,
            c,
            d,
            e,
            f,
            g,
            h,
            i,
            j,
            k,
            m: (b + a) / 2,
        }
    }
}

/// Fractal flags of this stage: `3` when the map wraps in y, else `9`.
fn fractal_flags(grid: &MapGrid) -> u32 {
    if grid.wrap_flags & 2 != 0 {
        fflags::WRAP_X | fflags::WRAP_Y
    } else {
        fflags::WRAP_X | fflags::OCEAN_POLES
    }
}

/// `0x5F1CE0`: paints `class` over the region of the cell at `(x, y)`.
///
/// Marks the cell in `visited`, and unless it is hills, mountains or a volcano
/// sets its class (nothing is written for class 2, which land already is).
/// Then it recurses into every ring-1 neighbour that is on the same continent
/// and in the same `regions` entry, is not hills, mountains or a volcano, and
/// is not marked yet. The binary recurses; this walks an explicit stack, which
/// reaches the same set of cells, because a cell is claimed once and the write
/// is idempotent.
pub fn write_biome_class(grid: &mut MapGrid, regions: &[u16], visited: &mut [u8], x: i32, y: i32, class: u8) {
    let mut stack = vec![(x, y)];
    while let Some((x, y)) = stack.pop() {
        let idx = grid.index(x, y);
        visited[idx] = 1;
        if matches!(grid.cells[idx].class(), 5 | 6 | 10) {
            continue;
        }
        if class != 2 {
            grid.cells[idx].set_class(class);
        }
        let (cont, region) = (grid.cells[idx].continent, regions[idx]);
        for n in 1..=8 {
            let (dx, dy) = spiral_offset(n);
            let Some((nx, ny)) = grid.wrap_and_check(x + dx, y + dy) else {
                continue;
            };
            let nidx = grid.index(nx, ny);
            let nb = &grid.cells[nidx];
            if nb.continent == cont
                && regions[nidx] == region
                && !matches!(nb.class(), 5 | 6 | 10)
                && visited[nidx] == 0
            {
                stack.push((nx, ny));
            }
        }
    }
}

/// Distance class of the nearest water: `2` within ring 1, `1` within ring 2.
fn near_water(grid: &MapGrid, x: i32, y: i32) -> i32 {
    for n in 1..=24 {
        let (dx, dy) = spiral_offset(n);
        let wet = grid
            .wrap_and_check(x + dx, y + dy)
            .and_then(|(nx, ny)| grid.cell_at(nx, ny))
            .is_some_and(|c| c.is_water());
        if wet {
            return if n < 9 { 2 } else { 1 };
        }
    }
    0
}

/// `0x5F1480`: assigns the land terrain.
///
/// `regions` is the `paintContinents` region map (`Map+0x3C`), one `u16` per
/// cell. `seed`, `temperature` and `climate` are the map's seed and the actual
/// option values.
pub fn assign_biomes(grid: &mut MapGrid, seed: i32, temperature: i32, climate: i32, regions: &[u16]) {
    let t = Thresholds::new(temperature, climate);
    let n = grid.num_cells();
    let flags = fractal_flags(grid);
    let fm = Fractal::generate(grid.w, grid.h, FRACTAL_LEVEL, flags, (seed as u32).wrapping_add(FRACTAL_SEED));
    let mut rng = Rng::new((seed as u32).wrapping_add(SEED_BASE));
    let mut visited = vec![0u8; n];

    let mut perm: Vec<usize> = (0..n).collect();
    for i in 0..n {
        let j = i + rng.below((n - i) as u32) as usize;
        perm.swap(i, j);
    }

    // Pass 1.
    for &c in &perm {
        let (x, y) = grid.coords(c);
        let lat = (grid.h / 2 - y).abs() * 180 / grid.h;
        let mut v = (i32::from(fm.sample(x, y)) - 128) * 30 / 256 + lat;
        if grid.cells[c].is_water() {
            continue;
        }
        let near = near_water(grid, x, y);
        if (23..48).contains(&v) {
            v += 2 * near;
        } else {
            v -= near;
        }

        if v > t.b || (v > t.m && rng.below(100) < 50) {
            grid.cells[c].set_flag(PLANE_FEATURE, FLAG_PINE);
        }
        if visited[c] != 0 {
            continue;
        }

        let mut class = 2;
        if t.h < v && v < t.i {
            class = 1;
        }
        if t.j < v && v < t.k {
            class = 0;
        }
        if v < t.a {
            class = 8;
        } else if v > t.b && lat > t.c {
            class = 3;
        }
        if v < t.d {
            let base = match class {
                2 => Some(t.g),
                8 => Some(t.f),
                _ => None,
            };
            if let Some(base) = base {
                if rng.below(100) < near * t.e + base {
                    class = 9;
                }
            }
        }
        write_biome_class(grid, regions, &mut visited, x, y, class);
    }

    // Pass 2.
    let fm2 = Fractal::generate(grid.w, grid.h, CONTOUR_LEVEL, flags, (seed as u32).wrapping_add(CONTOUR_SEED));
    let lo = i32::from(fm2.percentile(FOREST_FROM_PERCENTILE));
    let hi = i32::from(fm2.percentile(100));
    for i in 0..n {
        let c = &grid.cells[i];
        if c.sub_class() == 0 || matches!(c.class(), 8 | 5 | 6 | 9 | 10) || c.is_water() {
            continue;
        }
        let (x, y) = grid.coords(i);
        let mut h = i32::from(fm2.sample(x, y));
        if c.class() == 1 {
            h -= 10;
        }
        if h < lo || h > hi {
            continue;
        }
        grid.cells[i].set_class(7);
    }

    // Pass 3.
    for i in 0..n {
        if grid.cells[i].class() != 6 {
            continue;
        }
        let (x, y) = grid.coords(i);
        let mut p = 50u32;
        for step in 0..=48 {
            let (dx, dy) = spiral_offset(step);
            let Some((nx, ny)) = grid.wrap_and_check(x + dx, y + dy) else {
                continue;
            };
            if step == 9 || step == 25 {
                p >>= 1;
            }
            let Some(nb) = grid.cell_at(nx, ny) else { continue };
            let snow_capped = nb.class() == 6 && nb.flag(PLANE_FEATURE) & FLAG_SNOW_CAP != 0;
            if (nb.sub_class() == 3 || (step < 25 && snow_capped)) && rng.below(100) < p as i32 {
                grid.cells[i].set_flag(PLANE_FEATURE, FLAG_SNOW_CAP);
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thresholds_follow_the_option_tables() {
        let t = Thresholds::new(1, 1);
        assert_eq!((t.a, t.b, t.c, t.d, t.e), (4, 50, 60, 12, 5));
        assert_eq!((t.f, t.g, t.h, t.i, t.j, t.k), (10, 5, 14, 42, 17, 30));
        assert_eq!(t.m, 27, "(50 + 4) / 2");
        assert_eq!(Thresholds::new(0, 0).a, 2, "3 - 1");
        assert_eq!(Thresholds::new(2, 2).a, 7, "5 + 2");
    }

    /// 8x4 land grid, one continent, one region, all grassland.
    fn flood_grid() -> (MapGrid, Vec<u16>, Vec<u8>) {
        let mut grid = MapGrid::new(8, 4, 0, 0);
        for c in grid.cells.iter_mut() {
            c.set_class(2);
            c.continent = 1;
        }
        let n = grid.num_cells();
        (grid, vec![7u16; n], vec![0u8; n])
    }

    #[test]
    fn the_flood_claims_the_whole_region() {
        let (mut grid, regions, mut visited) = flood_grid();
        write_biome_class(&mut grid, &regions, &mut visited, 0, 0, 0);
        assert!(visited.iter().all(|&b| b == 1));
        assert!(grid.cells.iter().all(|c| c.class() == 0 && c.sub_class() == 0));
    }

    #[test]
    fn the_flood_is_stopped_by_region_continent_terrain_and_visited_marks() {
        type Gate = Box<dyn Fn(&mut MapGrid, &mut Vec<u16>, &mut Vec<u8>)>;
        // Cell 1 is walled off by each gate in turn.
        let gates: Vec<(&str, Gate)> = vec![
            ("region", Box::new(|_, r, _| r[1] = 9)),
            ("continent", Box::new(|g, _, _| g.cells[1].continent = 2)),
            ("terrain", Box::new(|g, _, _| g.cells[1].set_class(6))),
            ("visited", Box::new(|_, _, v| v[1] = 1)),
        ];
        for (name, gate) in gates {
            let (mut grid, mut regions, mut visited) = flood_grid();
            gate(&mut grid, &mut regions, &mut visited);
            let before = grid.cells[1].class();
            write_biome_class(&mut grid, &regions, &mut visited, 0, 0, 0);
            assert_eq!(grid.cells[1].class(), before, "{name}: blocked cell written");
        }
    }

    #[test]
    fn a_hill_start_cell_floods_nothing() {
        let (mut grid, regions, mut visited) = flood_grid();
        grid.cells[0].set_class(5);
        write_biome_class(&mut grid, &regions, &mut visited, 0, 0, 0);
        assert_eq!(visited[0], 1);
        assert!(visited[1..].iter().all(|&b| b == 0));
        assert_eq!(grid.cells[0].class(), 5);
    }
}
