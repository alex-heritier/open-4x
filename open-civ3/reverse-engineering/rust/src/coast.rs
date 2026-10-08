//! Water depth classes: `0x5EEB00`, the stage right after the land/sea stage.
//!
//! The land/sea stage (`0x5ECEB0`) leaves every water cell as deep ocean
//! (class 13) or shelf (class 12). This stage then makes the sea around the
//! land shallow, in two passes over every cell, both scanning the eight
//! neighbours of ring 1 of the spiral (offsets `1..=8`, wrapped per the map
//! flags and dropped when they fall off a non-wrapping edge):
//!
//! 1. A water cell (class 11..=13) with at least one **non-water** neighbour
//!    becomes **coast** (class 11).
//! 2. A **deep ocean** cell (class 13) with at least one **coast** neighbour
//!    becomes **sea** (class 12), unless bit `0x400000` of its feature plane is
//!    set. (That bit is scratch the start-site stages use; it is cleared again
//!    at the end of `generateMap`, and is 0 everywhere at this point.)
//!
//! So the sea is 11 next to land, 12 for one more cell, 13 beyond that, plus
//! whatever class-12 cells the land/sea stage left in its shelf band. The
//! passes run in place but cannot feed on their own output: pass 1 only reads
//! "is water", which its writes do not change, and pass 2 turns 13 into 12 and
//! only reads 11.
//!
//! Earlier notes called this stage "deconflictStarts" and described a bug in
//! which pass 1 wrote to the wrong cell. The register in question is reloaded
//! with the loop index before the write (`0x5EEC0E`), so there is no such bug;
//! the oracle fixtures confirm it.

use crate::cell::{MapGrid, PLANE_FEATURE};
use crate::spiral::spiral_offset;

/// Water class next to land.
pub const COAST: u8 = 11;
/// Water class one cell further out.
pub const SEA: u8 = 12;
/// Water class of the open ocean.
pub const OCEAN: u8 = 13;
/// Feature-plane scratch bit that keeps pass 2 away from a cell.
pub const KEEP_DEEP: u32 = 0x40_0000;

/// `true` when any of the eight ring-1 neighbours of cell `i` satisfies `pred`.
fn any_neighbour(grid: &MapGrid, i: usize, pred: impl Fn(&crate::cell::Cell) -> bool) -> bool {
    let (x, y) = grid.coords(i);
    (1..=8).any(|n| {
        let (dx, dy) = spiral_offset(n);
        grid.wrap_and_check(x + dx, y + dy)
            .and_then(|(nx, ny)| grid.cell_at(nx, ny))
            .is_some_and(&pred)
    })
}

/// `0x5EEB00`: coast and sea classes around the land.
pub fn classify_water_depth(grid: &mut MapGrid) {
    for i in 0..grid.num_cells() {
        if grid.cells[i].is_water() && any_neighbour(grid, i, |c| !c.is_water()) {
            grid.cells[i].set_class(COAST);
        }
    }
    for i in 0..grid.num_cells() {
        let c = &grid.cells[i];
        if c.class() == OCEAN
            && any_neighbour(grid, i, |n| n.class() == COAST)
            && c.flag(PLANE_FEATURE) & KEEP_DEEP == 0
        {
            grid.cells[i].set_class(SEA);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 8x4 grid (4 cells wide) of ocean with one land cell.
    fn island() -> MapGrid {
        let mut g = MapGrid::new(16, 8, 0, 0);
        let mid = g.index(8, 4);
        g.cells[mid].set_class(2);
        g
    }

    #[test]
    fn water_next_to_land_becomes_coast_and_the_next_ring_becomes_sea() {
        let mut g = island();
        classify_water_depth(&mut g);
        let (lx, ly) = g.coords(g.index(8, 4));
        assert!(!g.cell_at(lx, ly).unwrap().is_water());
        let mut seen = [0usize; 14];
        for c in &g.cells {
            seen[c.class() as usize] += 1;
        }
        assert_eq!(seen[2], 1, "the land cell is untouched");
        assert!(seen[COAST as usize] > 0, "no coast");
        assert!(seen[SEA as usize] > 0, "no sea ring");
        assert!(seen[OCEAN as usize] > 0, "no open ocean left");
    }

    #[test]
    fn the_scratch_bit_protects_a_deep_cell_from_pass_two() {
        let mut a = island();
        let mut b = island();
        // Any cell that would become sea; guard it in `b`.
        classify_water_depth(&mut a);
        let victim = a.cells.iter().position(|c| c.class() == SEA).unwrap();
        // Rebuild, guarding the cell before the stage runs.
        b.cells[victim].set_flag(PLANE_FEATURE, KEEP_DEEP);
        classify_water_depth(&mut b);
        assert_eq!(b.cells[victim].class(), OCEAN);
    }

    #[test]
    fn an_all_water_map_is_left_alone() {
        let mut g = MapGrid::new(16, 8, 3, 0);
        let before = g.cells.clone();
        classify_water_depth(&mut g);
        assert_eq!(g.cells, before);
    }
}
