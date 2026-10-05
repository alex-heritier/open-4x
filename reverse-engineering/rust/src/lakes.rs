//! The lake stage: `0x5ED5D0`, which `NOTES.md` first called
//! `assignStartsPerContinent`.
//!
//! Irrigation in Civ3 needs fresh water: a river on the tile, or a lake next to
//! it (a water body of at most [`LAKE_MAX`] cells counts as a lake, bigger ones
//! are sea). The land/sea stage knows nothing of this, so a large landmass can
//! come out with no tile that can ever be irrigated. This stage fixes that:
//! every continent of at least [`MIN_CONTINENT`] cells is checked, and one that
//! lacks an irrigable tile gets a lake dug into its interior.
//!
//! # Per continent
//!
//! 1. **Look.** Scan the cells in index order. The continent is fine as soon as
//!    one of its cells has fresh water ([`fresh_water`]) and an irrigation bonus
//!    above 0 ([`irrigation_bonus`]).
//! 2. **Dig, strict.** Otherwise walk the cells in a shuffled order (a 32-bit
//!    Fisher-Yates with the LCG seeded `seed + 0x7C0` after three warm-up
//!    draws). Take the first cell of the continent that has no river and whose
//!    3x3 neighbourhood (spiral `0..9`, cells off the map are ignored) is all
//!    land with an irrigation bonus above 0, and turn it into class 11 (coast).
//! 3. **Dig, loose.** If no cell qualifies, repeat with the same walk and only
//!    the "no river, all land" test.
//!
//! The ocean (a "continent" of water cells) goes through the same code and never
//! qualifies, because its own cells are water. After the last continent the
//! continents are renumbered, so a new lake becomes a water body of one cell.
//!
//! The shipped binary also looks up the `'TERR'` row of every tile it tests;
//! the result is never used.
//!
//! # What the oracle shows
//!
//! On the fixtures this stage digs a lake in only one of six maps (the flat
//! world), so the "look" step is the one that nearly always decides.

use crate::cell::{Cell, MapGrid, NO_CONTINENT, PLANE_FEATURE};
use crate::continents::number_continents;
use crate::rng::Rng;
use crate::spiral::spiral_offset;
use crate::yields::TERRAIN;

/// Continents with fewer cells are left alone.
pub const MIN_CONTINENT: usize = 75;
/// A water body this small is a lake (`0x5F38C0` tests `size <= 20`).
pub const LAKE_MAX: usize = 20;
/// Seed offset of the shuffle.
pub const SHUFFLE_SEED: u32 = 0x7C0;
/// The terrain class a lake gets.
pub const LAKE_CLASS: u8 = 11;
/// Cells of the 3x3 neighbourhood scanned (spiral `0..9`).
const NEIGHBOURHOOD: i32 = 9;
/// The landmark flag in the feature plane (`vfunc(0x78)`).
const LANDMARK: u32 = 0x2000_0000;

/// `0x5DBE70`: the irrigation bonus of the tile, from its terrain row or, with
/// the landmark flag, the landmark block of the row.
pub fn irrigation_bonus(cell: &Cell) -> i32 {
    let landmark = cell.flag(PLANE_FEATURE) & LANDMARK != 0;
    TERRAIN.get(cell.class() as usize).map_or(0, |t| t.variant(landmark).irrigation)
}

/// `0x5F38C0` with `n = 9`: is there a lake within the 3x3 neighbourhood?
///
/// A lake is a water cell whose continent has at most [`LAKE_MAX`] cells.
fn near_lake(grid: &MapGrid, sizes: &[usize], x: i32, y: i32) -> bool {
    (0..NEIGHBOURHOOD).any(|n| {
        let (dx, dy) = spiral_offset(n);
        grid.wrap_and_check(x + dx, y + dy)
            .and_then(|(nx, ny)| grid.cell_at(nx, ny))
            .is_some_and(|c| c.is_water() && size_of(sizes, c.continent) <= LAKE_MAX)
    })
}

/// `0x5F39E0`: fresh water at `(x, y)`, a lake nearby or a river on the tile.
pub fn fresh_water(grid: &MapGrid, sizes: &[usize], x: i32, y: i32) -> bool {
    near_lake(grid, sizes, x, y) || grid.cell_at(x, y).is_some_and(|c| c.river != 0)
}

/// The continent record's tile count, `0` for an id past the records.
fn size_of(sizes: &[usize], id: u16) -> usize {
    sizes.get(id as usize).copied().unwrap_or(0)
}

/// `0x5ED5D0`: digs lakes into continents with no irrigable tile.
pub fn add_lakes(grid: &mut MapGrid, seed: i32) {
    let n = grid.num_cells();
    let mut rng = Rng::new((seed as u32).wrapping_add(SHUFFLE_SEED));
    rng.discard(3);
    let mut perm: Vec<usize> = (0..n).collect();
    for i in 0..n {
        let j = i + rng.below((n - i) as u32) as usize;
        perm.swap(i, j);
    }

    // The continent records as the last renumbering left them.
    let mut sizes: Vec<usize> = Vec::new();
    for c in &grid.cells {
        if c.continent != NO_CONTINENT {
            let id = c.continent as usize;
            if sizes.len() <= id {
                sizes.resize(id + 1, 0);
            }
            sizes[id] += 1;
        }
    }

    for id in 0..sizes.len() {
        if sizes[id] < MIN_CONTINENT {
            continue;
        }
        let id = id as u16;

        // 1. Is there an irrigable tile already?
        let satisfied = (0..n).any(|i| {
            let (x, y) = grid.coords(i);
            let c = &grid.cells[i];
            c.continent == id && fresh_water(grid, &sizes, x, y) && irrigation_bonus(c) > 0
        });
        if satisfied {
            continue;
        }

        // 2. and 3. Dig: strict, then loose.
        for strict in [true, false] {
            let pick = perm.iter().copied().find(|&i| {
                let (x, y) = grid.coords(i);
                let c = &grid.cells[i];
                c.continent == id
                    && c.river == 0
                    && (0..NEIGHBOURHOOD).all(|k| {
                        let (dx, dy) = spiral_offset(k);
                        match grid.wrap_and_check(x + dx, y + dy).and_then(|(nx, ny)| grid.cell_at(nx, ny)) {
                            None => true,
                            Some(nb) => !nb.is_water() && (!strict || irrigation_bonus(nb) != 0),
                        }
                    })
            });
            if let Some(i) = pick {
                grid.cells[i].set_class(LAKE_CLASS);
                break;
            }
        }
    }
    number_continents(grid);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn irrigation_follows_the_stock_terrain_rows() {
        let mut c = Cell::default();
        for (class, want) in [(0, 1), (1, 1), (2, 1), (3, 0), (4, 1), (5, 0), (6, 0), (7, 0), (11, 0)] {
            c.set_class(class);
            assert_eq!(irrigation_bonus(&c), want, "class {class}");
        }
    }

    /// A 12x12 island of grassland with no water and no river gets one lake.
    #[test]
    fn a_dry_continent_gets_exactly_one_lake() {
        let mut g = MapGrid::new(40, 40, 0, 0);
        for i in 0..g.num_cells() {
            let (x, y) = g.coords(i);
            let land = (10..30).contains(&x) && (10..30).contains(&y);
            g.cells[i].set_class(if land { 2 } else { 13 });
        }
        number_continents(&mut g);
        let before = g.cells.iter().filter(|c| c.class() == LAKE_CLASS).count();
        add_lakes(&mut g, 5);
        let lakes = g.cells.iter().filter(|c| c.class() == LAKE_CLASS).count() - before;
        assert_eq!(lakes, 1);
        // Stable: a second call finds the lake and digs nothing more.
        add_lakes(&mut g, 5);
        assert_eq!(g.cells.iter().filter(|c| c.class() == LAKE_CLASS).count() - before, 1);
    }
}
