//! Continent numbering — `finalizeMap` (`0x5eb7d0`, `Map` vtable slot `0x7C`).
//!
//! The generator calls this once per land/sea attempt, to judge the shape of the
//! world, and again later when the world is final. It writes the continent id of every
//! cell (`Cell+0x1E`) and leaves one record per connected region in the `Map`.
//!
//! # What it does
//!
//! 1. Every cell's id is reset to `0xFFFF` (`0x5eb829..0x5eb849`).
//! 2. Scanning cells in index order, each cell still at `0xFFFF` starts a new region
//!    (`0x5eb859`). `water = cell.vfunc(0x8C)()` decides its kind and the record
//!    created by `0x5e0f30` has `isLand = !water` at `+0x20` and a tile count at `+0x24`.
//! 3. A queue flood fill (`malloc(2 * cellCount)` of `u16`) grows the region:
//!    * **land** is joined through **all eight neighbours**, `spiralOffset(1..=8)`
//!      (`0x5eba6e`: `(dx, dy)` with `|dx| + |dy| == 2`);
//!    * **water** is joined through the **four diagonal neighbours only**,
//!      `(1,-1) (1,1) (-1,1) (-1,-1)` from the table at `0x67055c` / `0x67056c`
//!      (`0x5eb92d`).
//!
//!    So two ocean tiles that meet only across a land tile's corner stay in different
//!    seas, while two land tiles that touch at a corner are one continent. Neighbours go
//!    through the usual single-step wrap and bounds check.
//! 4. The records are ranked with the C runtime `qsort` ([`crate::crt::crt_qsort`]) and
//!    the comparator at `0x5ebd20`: land before water, then larger before smaller, ties
//!    compare equal. The cells are then relabelled to the ranks (`0x5ebcb4..0x5ebd07`).
//!
//! So id `0` is always the largest landmass and the ids run through the land in
//! descending size, then continue through the water bodies, also in descending size.
//! That is the order the saved `TILE` records show, and it is what the continent-balance
//! test in `generateLandmass` (section 9.5 of `NOTES.md`) reads as `continents[0..=2]`.
//!
//! Checked against four generator-made saves: the partition rebuilt from the saved
//! terrain is the saved partition, region for region (0 cells outside the majority
//! mapping), and 4 994 to 9 220 cells of 5 000 to 9 240 carry the same id (the rest are
//! the tie order among equal-sized islands and a handful of tiles later stages changed).

use crate::cell::{MapGrid, NO_CONTINENT};
use crate::crt::crt_qsort;
use crate::spiral::spiral_offset;

/// The four neighbours a water body spreads through: `0x67055c` (dx) and `0x67056c` (dy).
pub const WATER_NEIGHBOURS: [(i32, i32); 4] = [(1, -1), (1, 1), (-1, 1), (-1, -1)];

/// One entry of `Map+0x214`: `Continent`, stride `0x28`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Continent {
    /// `+0x20`. `1` for land, `0` for a body of water (`0x5e0f30` stores `arg == 0`).
    pub is_land: bool,
    /// `+0x24`. Number of cells in the region.
    pub size: u32,
}

/// The comparator at `0x5ebd20`, over two records.
///
/// Negative when `a` ranks first: land before water, then more cells first.
pub fn compare(a: &Continent, b: &Continent) -> i32 {
    let (fa, fb) = (i32::from(a.is_land), i32::from(b.is_land));
    if fb < fa {
        return -1;
    }
    if fb > fa {
        return 1;
    }
    match b.size.cmp(&a.size) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Greater => 1,
        std::cmp::Ordering::Equal => 0,
    }
}

/// `finalizeMap` (`0x5eb7d0`): numbers the regions and returns the ranked records.
///
/// `grid.cells[i].continent` holds the index into the returned vector afterwards.
pub fn number_continents(grid: &mut MapGrid) -> Vec<Continent> {
    for c in &mut grid.cells {
        c.continent = NO_CONTINENT;
    }

    let land_neighbours: Vec<(i32, i32)> = (1..=8).map(spiral_offset).collect();
    let mut records: Vec<Continent> = Vec::new();
    let mut queue: Vec<usize> = Vec::with_capacity(grid.num_cells());

    for start in 0..grid.num_cells() {
        if grid.cells[start].continent != NO_CONTINENT {
            continue;
        }
        let water = grid.cells[start].is_water();
        let id = records.len() as u16;
        grid.cells[start].continent = id;
        let mut size = 1u32;
        queue.clear();
        queue.push(start);

        let offsets: &[(i32, i32)] = if water { &WATER_NEIGHBOURS } else { &land_neighbours };
        let mut head = 0;
        while head < queue.len() {
            let (x, y) = grid.coords(queue[head]);
            head += 1;
            for &(dx, dy) in offsets {
                let Some((nx, ny)) = grid.wrap_and_check(x + dx, y + dy) else {
                    continue;
                };
                let j = grid.index(nx, ny);
                let Some(cell) = grid.cells.get_mut(j) else {
                    continue;
                };
                // 0x5eb9df / 0x5ebb3b: the neighbour must be the same kind, and
                // 0x5eba00 / 0x5ebb5c: not already carry this region's id.
                if cell.is_water() != water || cell.continent == id {
                    continue;
                }
                cell.continent = id;
                size += 1;
                queue.push(j);
            }
        }
        records.push(Continent { is_land: !water, size });
    }

    // 0x5ebc40..0x5ebd07: rank, reorder the records, relabel the cells.
    let mut order: Vec<usize> = (0..records.len()).collect();
    crt_qsort(&mut order, |&a, &b| compare(&records[a], &records[b]));

    let mut rank = vec![NO_CONTINENT; records.len()];
    for (new_id, &old) in order.iter().enumerate() {
        rank[old] = new_id as u16;
    }
    for c in &mut grid.cells {
        if let Some(&r) = rank.get(usize::from(c.continent)) {
            c.continent = r;
        }
    }
    order.iter().map(|&old| records[old]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::{Cell, LAND_GRASSLAND, WATER_ABYSSAL};

    /// Builds a `w x h` grid from rows of `#` (land) and `.` (water), one char per
    /// *cell*, not per tile.
    fn grid(rows: &[&str], wrap: u32) -> MapGrid {
        let h = rows.len() as i32;
        let half = rows[0].len() as i32;
        let mut g = MapGrid::new(half * 2, h, wrap, 0);
        for (y, row) in rows.iter().enumerate() {
            for (x, ch) in row.chars().enumerate() {
                let mut cell = Cell::default();
                cell.put_class(if ch == '#' { LAND_GRASSLAND } else { WATER_ABYSSAL });
                g.cells[y * half as usize + x] = cell;
            }
        }
        g
    }

    #[test]
    fn one_island_in_a_sea_gives_land_first() {
        let mut g = grid(&["......", "..##..", "..##..", "......"], 0);
        let recs = number_continents(&mut g);
        assert_eq!(recs[0], Continent { is_land: true, size: 4 });
        assert!(recs[1..].iter().all(|r| !r.is_land));
        assert_eq!(g.cells[1 * 6 + 2].continent, 0);
        assert_eq!(recs.iter().map(|r| r.size).sum::<u32>(), 24);
    }

    #[test]
    fn larger_land_ranks_before_smaller_whatever_the_scan_order() {
        // A single cell is met first, the 3-cell island second.
        let mut g = grid(&["#.....", "......", "...###", "......"], 0);
        let recs = number_continents(&mut g);
        assert_eq!(recs[0].size, 3);
        assert_eq!(recs[1].size, 1);
        assert!(recs[0].is_land && recs[1].is_land);
        assert_eq!(g.cells[0].continent, 1);
    }

    #[test]
    fn land_is_eight_connected_and_water_is_four_connected() {
        // Row 1 is land: tiles (1,1) (3,1) (5,1), neighbours along x by 2. Rows 0 and 2
        // are water: tiles (0,0) (2,0) (4,0) and (0,2) (2,2) (4,2). Two water tiles
        // two apart in x are *not* joined (only the four (+-1, +-1) links are), so
        // every one of the six is its own body of water.
        let mut g = grid(&["...", "###", "..."], 0);
        let recs = number_continents(&mut g);
        assert_eq!(recs[0], Continent { is_land: true, size: 3 });
        assert_eq!(recs.len(), 7);
        assert!(recs[1..].iter().all(|r| !r.is_land && r.size == 1));
    }

    #[test]
    fn diagonal_neighbours_join_both_land_and_water() {
        // Cell (0,0) is tile (0,0) and cell (0,1) is tile (1,1): a (+1,+1) neighbour.
        let mut g = grid(&["#.", "#."], 0);
        let recs = number_continents(&mut g);
        assert_eq!(recs, vec![Continent { is_land: true, size: 2 }, Continent { is_land: false, size: 2 }]);
    }

    #[test]
    fn ids_cover_every_cell_and_match_the_records() {
        let mut g = grid(&["#.#.#", ".#.#.", "#.#.#", ".#.#."], 1);
        let recs = number_continents(&mut g);
        assert!(g.cells.iter().all(|c| usize::from(c.continent) < recs.len()));
        let mut counts = vec![0u32; recs.len()];
        for c in &g.cells {
            counts[usize::from(c.continent)] += 1;
        }
        assert_eq!(counts, recs.iter().map(|r| r.size).collect::<Vec<_>>());
        assert!(recs.windows(2).all(|w| compare(&w[0], &w[1]) <= 0));
    }

    #[test]
    fn x_wrap_joins_the_two_edges() {
        // Four cells = eight tiles. Tile 0 and tile 6 are two apart across the seam.
        let rows = ["#..#", "....", "...."];
        let open = number_continents(&mut grid(&rows, 0)).iter().filter(|r| r.is_land).count();
        let wrapped = number_continents(&mut grid(&rows, 1));
        assert_eq!(open, 2);
        assert_eq!(wrapped.iter().filter(|r| r.is_land).count(), 1);
        assert_eq!(wrapped[0].size, 2);
    }
}
