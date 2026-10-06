//! Region map: `paintContinents`, `0x5EDDB0`, with its worker `0x5EE470`.
//!
//! The biome stage ([`crate::biomes`]) paints one terrain class over a whole
//! *region* at a time, so before it runs the game cuts every landmass into
//! small contiguous regions and keeps the result as a `u16` per cell (the
//! scratch array at `Map+0x3C`, `0xFFFF` for none; freed again when the biome
//! stage ends). Region ids are only unique within a continent: the id of a new
//! region is the number of regions the continent already has. The array is
//! internal, but the oracle fixtures record it (the `region` plane) so this
//! stage can be tested on its own.
//!
//! # The outer loop
//!
//! Shuffle the cell indices (Fisher-Yates, LCG seeded with `seed + 0xD431`).
//! Walk the shuffled order; for every land cell that has no region yet, call
//! [`paint_region`] with the continent's running region count as the id. (The
//! binary has a second branch for a continent whose counter has reached
//! `0xFFFF`, which an `i16` cell count cannot reach; it is not modelled.)
//!
//! # [`paint_region`]
//!
//! The region grows by a random walk over the four *diagonal* neighbours
//! (`(+1,-1) (+1,+1) (-1,+1) (-1,-1)`, in cell coordinates the four cardinal
//! neighbours) with its own LCG, seeded `seed + 29 * id` and warmed up by four
//! draws. Up to 16 walks start from the seed cell; each walk steps through
//! cells that are on the same continent, have the same hills/mountains-ness as
//! the seed cell and are free or already in this region, until it reaches a
//! free cell (which it claims) or has taken three steps. A walk that finds
//! every direction blocked ends the whole growth.
//!
//! A region that ends with fewer than four cells is then dissolved into its
//! neighbours: for the seed cell and the first cell of ring 1 that belong to it,
//! the four diagonal neighbours' regions are collected, and the cell is handed
//! to a region that occurs at least twice among them (checked in the order of
//! the four), else to the last neighbour that has one. This repeats while the
//! count keeps changing.

use crate::cell::MapGrid;
use crate::rng::Rng;
use crate::spiral::spiral_offset;

/// Seed offset of the shuffle: `0xD431`.
pub const SHUFFLE_SEED: u32 = 0xD431;
/// Seed multiplier per region id: `29`.
pub const REGION_SEED_STRIDE: u32 = 29;
/// "No region".
pub const NONE: u16 = 0xFFFF;
/// Largest number of walks a region gets.
pub const MAX_WALKS: u32 = 16;
/// Regions smaller than this are merged into their neighbours.
pub const MIN_SIZE: u32 = 4;

/// The four walk directions, `0x67055C` (x) and `0x67056C` (y).
const DIAGONALS: [(i32, i32); 4] = [(1, -1), (1, 1), (-1, 1), (-1, -1)];

/// `vfunc(0x40)`: hills or mountains.
fn hilly(grid: &MapGrid, x: i32, y: i32) -> bool {
    grid.cell_at(x, y).is_some_and(|c| matches!(c.class(), 5 | 6))
}

/// Region id of the cell at `(x, y)`.
fn region_at(grid: &MapGrid, regions: &[u16], x: i32, y: i32) -> u16 {
    regions[grid.index(x, y)]
}

/// Continent of the cell at `(x, y)`.
fn continent_at(grid: &MapGrid, x: i32, y: i32) -> u16 {
    grid.cell_at(x, y).map_or(NONE, |c| c.continent)
}

/// `0x5EE470`: grows region `id` from the free land cell `(x0, y0)` and counts
/// it in `counters` when it kept at least one cell.
pub fn paint_region(
    grid: &MapGrid,
    regions: &mut [u16],
    counters: &mut [u16],
    stack: &mut [u16; 4],
    seed: i32,
    x0: i32,
    y0: i32,
    id: u16,
) {
    let mut rng = Rng::new((seed as u32).wrapping_add(REGION_SEED_STRIDE.wrapping_mul(u32::from(id))));
    rng.discard(4);
    let cont0 = continent_at(grid, x0, y0);
    let mut painted: u8 = 0;

    // Growth: up to 16 walks from the seed cell.
    'walks: for _ in 0..MAX_WALKS {
        let (mut cx, mut cy) = (x0, y0);
        let group = hilly(grid, x0, y0);
        let mut steps = 0u8;
        loop {
            if region_at(grid, regions, cx, cy) == NONE {
                let i = grid.index(cx, cy);
                regions[i] = id;
                painted = painted.wrapping_add(1);
                break;
            }
            let mut dir = rng.below(4) as usize;
            let mut moved = false;
            for _ in 0..4 {
                dir %= 4;
                let (dx, dy) = DIAGONALS[dir];
                if let Some((nx, ny)) = grid.wrap_and_check(cx + dx, cy + dy) {
                    let r = region_at(grid, regions, nx, ny);
                    if continent_at(grid, nx, ny) == continent_at(grid, cx, cy)
                        && hilly(grid, nx, ny) == group
                        && (r == NONE || r == id)
                    {
                        cx = nx;
                        cy = ny;
                        moved = true;
                        break;
                    }
                }
                dir += 1;
            }
            if !moved {
                break 'walks;
            }
            steps += 1;
            if steps >= 3 {
                break;
            }
        }
    }

    // Dissolve a region that stayed small.
    if u32::from(painted) < MIN_SIZE {
        let mut prev: u8 = 0;
        let neighbours = stack;
        while u32::from(painted) < MIN_SIZE && painted != prev {
            prev = painted;
            for n in (0..=1).rev() {
                let (dx, dy) = spiral_offset(n);
                let Some((px, py)) = grid.wrap_and_check(x0 + dx, y0 + dy) else {
                    continue;
                };
                if continent_at(grid, px, py) != cont0 || region_at(grid, regions, px, py) != id {
                    continue;
                }
                for (d, &(ddx, ddy)) in DIAGONALS.iter().enumerate() {
                    if let Some((nx, ny)) = grid.wrap_and_check(px + ddx, py + ddy) {
                        neighbours[d] = NONE;
                        if continent_at(grid, nx, ny) == cont0 {
                            let r = region_at(grid, regions, nx, ny);
                            if r != id {
                                neighbours[d] = r;
                            }
                        }
                    }
                }
                if let Some(r) = adopt(*neighbours) {
                    let i = grid.index(px, py);
                    regions[i] = r;
                    painted = painted.wrapping_sub(1);
                }
            }
        }
    }

    if painted != 0 {
        if let Some(c) = counters.get_mut(cont0 as usize) {
            *c = c.wrapping_add(1);
        }
    }
}

/// Which neighbouring region takes over a dissolved cell (`0x5EE9C2..0x5EEAA9`).
///
/// A value that repeats among the four wins, checked in the order `v0 (vs v1,
/// v2, v3)`, `v1 (vs v2, v3)`, `v2 (vs v3)`; with no repeat the last valid one
/// of `v3, v2, v1, v0` does.
fn adopt(v: [u16; 4]) -> Option<u16> {
    let [v0, v1, v2, v3] = v;
    if v0 != NONE && (v0 == v1 || v0 == v2 || v0 == v3) {
        return Some(v0);
    }
    if v1 != NONE && (v1 == v2 || v1 == v3) {
        return Some(v1);
    }
    if v2 != NONE && v2 == v3 {
        return Some(v2);
    }
    [v3, v2, v1, v0].into_iter().find(|&r| r != NONE)
}

/// `0x5EDDB0`: the region map for `grid`, one entry per cell.
pub fn paint_regions(grid: &MapGrid, seed: i32) -> Vec<u16> {
    let n = grid.num_cells();
    let mut regions = vec![NONE; n];
    let continents = grid
        .cells
        .iter()
        .filter(|c| c.continent != NONE)
        .map(|c| c.continent as usize + 1)
        .max()
        .unwrap_or(0);
    let mut counters = vec![0u16; continents];
    let mut stack = [NONE; 4];

    let mut rng = Rng::new((seed as u32).wrapping_add(SHUFFLE_SEED));
    let mut perm: Vec<usize> = (0..n).collect();
    for i in 0..n {
        let j = i + rng.below((n - i) as u32) as usize;
        perm.swap(i, j);
    }

    for &c in &perm {
        if regions[c] != NONE || grid.cells[c].is_water() {
            continue;
        }
        let (x, y) = grid.coords(c);
        let id = counters[grid.cells[c].continent as usize];
        paint_region(grid, &mut regions, &mut counters, &mut stack, seed, x, y, id);
    }
    regions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adoption_prefers_a_repeated_neighbour() {
        assert_eq!(adopt([NONE; 4]), None);
        assert_eq!(adopt([3, 4, 4, NONE]), Some(4), "v1 repeats, v0 does not");
        assert_eq!(adopt([3, 3, 9, 9]), Some(3), "v0 first");
        assert_eq!(adopt([1, 2, 3, 4]), Some(4), "no repeat: the last valid");
        assert_eq!(adopt([1, NONE, NONE, NONE]), Some(1));
    }

    #[test]
    fn every_land_cell_gets_a_region_and_water_none() {
        let mut g = MapGrid::new(16, 16, 0, 0);
        for (i, c) in g.cells.iter_mut().enumerate() {
            let land = i % 5 != 0;
            c.set_class(if land { 2 } else { 13 });
            c.continent = if land { 0 } else { 1 };
        }
        let r = paint_regions(&g, 7);
        for (i, c) in g.cells.iter().enumerate() {
            assert_eq!(r[i] == NONE, c.is_water(), "cell {i}");
        }
    }
}
