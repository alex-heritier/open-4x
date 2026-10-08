//! Continent numbering: the flood fill that labels connected regions and ranks
//! them into ids.
//!
//! Scanning cells in index order, each unvisited cell starts a new region. Land
//! is joined through all **eight** neighbours; a body of water is joined through
//! the **four diagonals only**, so two oceans that meet only across a land tile's
//! corner stay separate while two land tiles that touch at a corner are one
//! continent. Neighbours go through the single-step wrap and bounds check.
//!
//! The regions are then ranked by [`compare`] (land before water, larger before
//! smaller, ties equal) using the runtime `qsort` ([`crate::crt::crt_qsort`]) whose
//! tie order is observable, so id `0` is the largest landmass and the ids run
//! through the land in descending size and then the water bodies.

use crate::cell::{MapGrid, NO_CONTINENT};
use crate::crt::crt_qsort;
use crate::spiral::spiral_offset;

/// The four neighbours a water body spreads through.
pub const WATER_NEIGHBOURS: [(i32, i32); 4] = [(1, -1), (1, 1), (-1, 1), (-1, -1)];

/// One continent record: kind and cell count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Continent {
    /// `true` for land, `false` for a body of water.
    pub is_land: bool,
    /// Number of cells in the region.
    pub size: u32,
}

/// The ranking comparator: negative when `a` ranks first — land before water,
/// then more cells first.
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

/// Numbers the regions and returns the ranked records. `grid.cells[i].continent`
/// holds the index into the returned vector afterwards.
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

        let offsets: &[(i32, i32)] = if water {
            &WATER_NEIGHBOURS
        } else {
            &land_neighbours
        };
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

    // Rank, reorder the records, relabel the cells.
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

    /// Builds a grid from rows of `#` (land) and `.` (water), one char per cell.
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
        assert_eq!(recs.iter().map(|r| r.size).sum::<u32>(), 24);
    }

    #[test]
    fn larger_land_ranks_before_smaller_whatever_the_scan_order() {
        let mut g = grid(&["#.....", "......", "...###", "......"], 0);
        let recs = number_continents(&mut g);
        assert_eq!(recs[0].size, 3);
        assert_eq!(recs[1].size, 1);
        assert_eq!(g.cells[0].continent, 1);
    }

    #[test]
    fn land_is_eight_connected_and_water_is_four_connected() {
        let mut g = grid(&["...", "###", "..."], 0);
        let recs = number_continents(&mut g);
        assert_eq!(recs[0], Continent { is_land: true, size: 3 });
        assert_eq!(recs.len(), 7);
        assert!(recs[1..].iter().all(|r| !r.is_land && r.size == 1));
    }

    #[test]
    fn x_wrap_joins_the_two_edges() {
        let rows = ["#..#", "....", "...."];
        let open = number_continents(&mut grid(&rows, 0)).iter().filter(|r| r.is_land).count();
        let wrapped = number_continents(&mut grid(&rows, 1));
        assert_eq!(open, 2);
        assert_eq!(wrapped.iter().filter(|r| r.is_land).count(), 1);
        assert_eq!(wrapped[0].size, 2);
    }
}
