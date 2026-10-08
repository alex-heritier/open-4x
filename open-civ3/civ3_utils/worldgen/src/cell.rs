//! The map cell grid.
//!
//! The generator does not work on a `W x H` tile grid. Cells live in a
//! `(W/2) x H` array — two map tiles share a cell along x, indexed by
//! `cell = (W >> 1) * y + (x >> 1)`. Only the fields the generator touches are
//! modelled here.

/// A terrain class value. The terrain word packs the class in bits 12..15 and
/// the sub-class in bits 8..11.
pub const CLASS_PLAINS: u8 = 0;
/// The group that classes `4, 5, 6, 8, 9, 10` collapse into.
pub const CLASS_GROUP2: u8 = 2;
/// The three water classes, in increasing depth.
pub const WATER_SHALLOW: u8 = 11;
/// Mid-depth water.
pub const WATER_DEEP: u8 = 12;
/// The deepest water; what the land/sea stage writes for every cell below the
/// sea-level percentile.
pub const WATER_ABYSSAL: u8 = 13;
/// The land class the land/sea stage writes.
pub const LAND_GRASSLAND: u8 = 2;

/// `true` for the three water classes (`11 <= class <= 13`).
#[inline]
pub const fn is_water(class: u8) -> bool {
    class > 10 && class < 14
}

/// Normalises a raw terrain index the way `setTerrain` does, given the cell's
/// current sub-class.
pub fn normalise_class(t: u8, sub_class: u8) -> u8 {
    match t {
        0..=3 => t,
        4 => CLASS_PLAINS,
        7 => {
            if sub_class != 0 && sub_class < 11 {
                sub_class
            } else {
                CLASS_GROUP2
            }
        }
        5 | 6 | 8 | 9 | 10 => CLASS_GROUP2,
        _ => t,
    }
}

/// Number of flag planes the cell carries.
pub const NUM_FLAG_PLANES: usize = 6;

/// Plane 0: the overlay bits (road, mine, goody hut, ...).
pub const PLANE_OVERLAY: usize = 0;
/// Plane 1: the terrain word (class in bits 12..15, sub-class in bits 8..11).
pub const PLANE_TERRAIN: usize = 1;
/// Plane 2: the feature plane (bonus grassland, start location, ...).
pub const PLANE_FEATURE: usize = 2;

/// Continent id used for "not assigned yet".
pub const NO_CONTINENT: u16 = 0xFFFF;

/// The terrain word of a freshly constructed cell: class and sub-class 13.
pub const TERRAIN_DEFAULT: u32 = 0xDD00;

/// One cell of the map: the part of the game's `Cell` object that `Map::generate`
/// writes. The `.biq` `TILE` row and the `.sav` per-cell chunks carry the same
/// fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    /// Eight-direction river adjacency mask.
    pub river: u8,
    /// `GOOD` row of the resource, `-1` for none.
    pub resource: i32,
    /// Terrain sprite variant.
    pub image: u8,
    /// Terrain sprite file.
    pub file: u8,
    /// Continent id, [`NO_CONTINENT`] when unassigned.
    pub continent: u16,
    /// Overlay, terrain word, feature plane and three more words; see the
    /// `PLANE_*` constants.
    pub planes: [u32; NUM_FLAG_PLANES],
}

impl Default for Cell {
    /// A new cell is deep water; the land/sea stage relies on that default when
    /// it leaves a cell untouched.
    fn default() -> Self {
        let mut planes = [0; NUM_FLAG_PLANES];
        planes[PLANE_TERRAIN] = TERRAIN_DEFAULT;
        Cell {
            river: 0,
            resource: -1,
            image: 0,
            file: 0,
            continent: NO_CONTINENT,
            planes,
        }
    }
}

impl Cell {
    /// Terrain class: bits 12..15 of the terrain word.
    #[inline]
    pub fn class(&self) -> u8 {
        ((self.planes[PLANE_TERRAIN] >> 12) & 0xF) as u8
    }

    /// Sub-class: bits 8..11 of the terrain word.
    #[inline]
    pub fn sub_class(&self) -> u8 {
        ((self.planes[PLANE_TERRAIN] >> 8) & 0xF) as u8
    }

    /// Replaces the class nibble only.
    #[inline]
    pub fn put_class(&mut self, class: u8) {
        let w = &mut self.planes[PLANE_TERRAIN];
        *w = (*w & !0xF000) | (u32::from(class & 0xF) << 12);
    }

    /// Replaces the sub-class nibble only.
    #[inline]
    pub fn put_sub_class(&mut self, sub: u8) {
        let w = &mut self.planes[PLANE_TERRAIN];
        *w = (*w & !0x0F00) | (u32::from(sub & 0xF) << 8);
    }

    /// `setTerrain(t)`: writes both nibbles of the terrain word, normalising the
    /// sub-class. Desert (`t == 0`) on a cell that already has a river becomes
    /// flood plain (class 4, sub-class 0) instead.
    #[inline]
    pub fn set_class(&mut self, t: u8) {
        if t == 0 && self.river != 0 {
            self.put_class(4);
            self.put_sub_class(0);
            return;
        }
        let sub = normalise_class(t, self.sub_class());
        self.put_class(t);
        self.put_sub_class(sub);
    }

    /// `addRiver(mask)`: ORs `mask` into the river mask, and a desert tile that
    /// now has a river becomes flood plain.
    #[inline]
    pub fn add_river(&mut self, mask: u8) {
        self.river |= mask;
        if self.class() == 0 {
            self.put_class(4);
        }
    }

    /// `removeRiver(mask)`: clears `mask`; when nothing is left, clears the
    /// river entirely.
    #[inline]
    pub fn remove_river(&mut self, mask: u8) {
        self.river &= !mask;
        if self.river == 0 {
            self.clear_river();
        }
    }

    /// No river at all: zeroes the mask, clears the four river-corner feature
    /// bits, and a flood plain goes back to desert.
    #[inline]
    pub fn clear_river(&mut self) {
        self.river = 0;
        self.clear_flag(PLANE_FEATURE, 0x0F00_0000);
        if self.class() == 4 {
            self.put_class(0);
        }
    }

    /// Sets the continent id.
    #[inline]
    pub fn set_continent(&mut self, id: u16) {
        self.continent = id;
    }

    /// The continent id.
    #[inline]
    pub fn continent(&self) -> u16 {
        self.continent
    }

    /// `true` for the water classes.
    #[inline]
    pub fn is_water(&self) -> bool {
        is_water(self.class())
    }

    /// Sets bits in one of the flag planes. Planes are `dword`s, not bits within
    /// one word, so the mask is frequently a byte- or nibble-sized constant.
    #[inline]
    pub fn set_flag(&mut self, plane: usize, mask: u32) {
        if let Some(p) = self.planes.get_mut(plane) {
            *p |= mask;
        }
    }

    /// Clears bits in one of the flag planes.
    #[inline]
    pub fn clear_flag(&mut self, plane: usize, mask: u32) {
        if let Some(p) = self.planes.get_mut(plane) {
            *p &= !mask;
        }
    }

    /// Reads one flag plane; out-of-range planes read as 0.
    #[inline]
    pub fn flag(&self, plane: usize) -> u32 {
        self.planes.get(plane).copied().unwrap_or(0)
    }
}

/// The map: dimensions, the cell grid, and the generator's option set.
#[derive(Clone, Debug)]
pub struct MapGrid {
    /// Full map width in tiles.
    pub w: i32,
    /// Full map height in tiles.
    pub h: i32,
    /// Bit 0 wraps x, bit 1 wraps y.
    pub wrap_flags: u32,
    /// The generator seed (historically named "water level"; it holds the seed).
    pub water_level: i32,
    /// The cell grid, `(w/2) * h` entries, row-major.
    pub cells: Vec<Cell>,
}

impl MapGrid {
    /// Creates an empty grid. Every cell starts as deep water.
    pub fn new(w: i32, h: i32, wrap_flags: u32, water_level: i32) -> Self {
        let n = (w / 2) * h;
        MapGrid {
            w,
            h,
            wrap_flags,
            water_level,
            cells: vec![Cell::default(); n.max(0) as usize],
        }
    }

    /// The generator seed.
    #[inline]
    pub fn seed(&self) -> i32 {
        self.water_level
    }

    /// Number of cells.
    #[inline]
    pub fn num_cells(&self) -> usize {
        self.cells.len()
    }

    /// Cell index for map coordinates. `x` is halved with a 16-bit logical
    /// shift, so a negative `x` does not land in the previous row.
    #[inline]
    pub fn index(&self, x: i32, y: i32) -> usize {
        (((self.w >> 1) * y + i32::from((x as u16) >> 1)) & 0xFFFF) as usize
    }

    /// Bounds-checked cell lookup; out-of-range yields `None`.
    #[inline]
    pub fn cell_at(&self, x: i32, y: i32) -> Option<&Cell> {
        self.cells.get(self.index(x, y))
    }

    /// Mutable variant of [`MapGrid::cell_at`].
    #[inline]
    pub fn cell_at_mut(&mut self, x: i32, y: i32) -> Option<&mut Cell> {
        let i = self.index(x, y);
        self.cells.get_mut(i)
    }

    /// Cell by flat index, unchecked.
    #[inline]
    pub fn cell(&self, i: usize) -> Option<&Cell> {
        self.cells.get(i)
    }

    /// Mutable variant of [`MapGrid::cell`].
    #[inline]
    pub fn cell_mut(&mut self, i: usize) -> Option<&mut Cell> {
        self.cells.get_mut(i)
    }

    /// Recovers `(x, y)` from a flat cell index: the inverse of
    /// [`MapGrid::index`], `y = i / (W>>1); x = 2*(i % (W>>1)) + (y & 1)`.
    /// The recovered `x` is the nominal tile of the 2-wide cell.
    #[inline]
    pub fn coords(&self, i: usize) -> (i32, i32) {
        let half = (self.w >> 1) as usize;
        if half == 0 {
            return (0, 0);
        }
        let y = (i / half) as i32;
        let x = 2 * (i % half) as i32 + (y & 1);
        (x, y)
    }

    /// The nominal start slot for civ `i`.
    #[inline]
    pub fn nominal_slot(&self, i: usize) -> (i32, i32) {
        self.coords(i)
    }

    /// Single-step wrap of `v` into `0..n` on the requested axis.
    #[inline]
    pub fn wrap_axis(&self, v: i32, axis_x: bool) -> i32 {
        let (n, enabled) = if axis_x {
            (self.w, self.wrap_flags & 1 != 0)
        } else {
            (self.h, self.wrap_flags & 2 != 0)
        };
        if !enabled {
            return v;
        }
        if v < 0 {
            v + n
        } else if v >= n {
            v - n
        } else {
            v
        }
    }

    /// Wraps a coordinate pair and reports whether it landed inside the map.
    #[inline]
    pub fn wrap_and_check(&self, x: i32, y: i32) -> Option<(i32, i32)> {
        let nx = self.wrap_axis(x, true);
        let ny = self.wrap_axis(y, false);
        if (0..self.w).contains(&nx) && (0..self.h).contains(&ny) {
            Some((nx, ny))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_shape() {
        let g = MapGrid::new(160, 160, 0, 30);
        assert_eq!(g.num_cells(), 80 * 160);
        assert_eq!(g.cell_at(0, 0).map(|c| c.class()), Some(WATER_ABYSSAL));
        assert!(g.cell_at(160, 159).is_none());
        assert!(g.cell_at(159, 160).is_none());
    }

    #[test]
    fn two_adjacent_columns_share_a_cell() {
        let g = MapGrid::new(60, 60, 0, 0);
        for y in 0..60 {
            for x in (0..60).step_by(2) {
                assert_eq!(g.index(x, y), g.index(x + 1, y), "at {x},{y}");
            }
        }
    }

    #[test]
    fn coords_inverts_index() {
        let g = MapGrid::new(60, 60, 0, 0);
        for i in 0..g.num_cells() {
            let (x, y) = g.coords(i);
            assert_eq!(((x >> 1) as usize, y as usize), (i % 30, i / 30));
            assert!(x < 60 && y < 60);
        }
    }

    #[test]
    fn wrapping_is_a_single_step() {
        let g = MapGrid::new(60, 60, 0b11, 0);
        assert_eq!(g.wrap_axis(-1, true), 59);
        assert_eq!(g.wrap_axis(60, true), 0);
        let nw = MapGrid::new(60, 60, 0, 0);
        assert_eq!(nw.wrap_axis(-1, true), -1);
        assert_eq!(nw.wrap_and_check(60, 0), None);
    }

    #[test]
    fn class_normalisation_and_water_predicate() {
        assert_eq!(normalise_class(0, 0), 0);
        assert_eq!(normalise_class(4, 0), CLASS_PLAINS);
        assert_eq!(normalise_class(7, 0), CLASS_GROUP2);
        assert_eq!(normalise_class(7, 5), 5);
        assert_eq!(normalise_class(13, 0), 13);
        for c in 0u8..=15 {
            assert_eq!(is_water(c), c > 10 && c < 14, "class {c}");
        }
    }

    #[test]
    fn flags_are_plane_addressed() {
        let mut c = Cell::default();
        c.set_flag(0, 0x20);
        c.set_flag(3, 0x200);
        assert_eq!(c.flag(0), 0x20);
        assert_eq!(c.flag(3), 0x200);
        c.clear_flag(0, 0x20);
        assert_eq!(c.flag(0), 0);
        assert_eq!(c.flag(3), 0x200);
        assert_eq!(c.flag(NUM_FLAG_PLANES), 0);
    }
}
