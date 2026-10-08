//! Start-placement geometry.
//!
//! The full start-placement stage (the eight-pass `finalPass`) stays in the
//! reverse-engineering reference; the game only needs the isometric distance it
//! shares with the rest of the map code, so that is all that lives here.

use crate::cell::MapGrid;

/// The number of start slots the generator keeps.
pub const SLOTS: usize = 32;

/// The isometric distance between two tiles given their coordinate differences,
/// after each axis is wrapped: the larger difference less half of what the two
/// differences have beyond the smaller. One step in the 8 directions is 1.
pub fn distance(dx: i32, dy: i32) -> i32 {
    let (hi, lo) = (dx.max(dy), dx.min(dy));
    hi - ((dx + dy) / 2 - lo + 1) / 2
}

/// The distance between tiles `(x1, y1)` and `(x2, y2)` on `grid`, wrapping the
/// axes the map wraps.
pub fn grid_distance(grid: &MapGrid, x1: i32, y1: i32, x2: i32, y2: i32) -> i32 {
    let mut dx = (x1 - x2).abs();
    if grid.wrap_flags & 1 != 0 && dx > grid.w / 2 {
        dx = grid.w - dx;
    }
    let mut dy = (y1 - y2).abs();
    if grid.wrap_flags & 2 != 0 && dy > grid.h / 2 {
        dy = grid.h - dy;
    }
    distance(dx, dy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_step_is_one() {
        for (dx, dy) in [(0, 1), (1, 1), (1, 0)] {
            assert_eq!(distance(dx, dy), 1, "{dx},{dy}");
        }
        assert_eq!(distance(0, 0), 0);
        // Two tiles along one axis are one cell apart.
        assert_eq!(distance(2, 0), 1);
    }

    #[test]
    fn grid_distance_wraps_where_the_map_does() {
        let open = MapGrid::new(60, 60, 0, 0);
        let wrapped = MapGrid::new(60, 60, 1, 0);
        assert!(grid_distance(&open, 0, 0, 59, 0) > 1);
        assert_eq!(grid_distance(&wrapped, 0, 0, 59, 0), 1);
    }
}
