//! The map cell grid.
//!
//! # Grid geometry
//!
//! The generator does not work on a `W x H` tile grid. Cells live in a
//! **`(W/2) x H`** array — two map tiles share a cell along x. This shows up
//! everywhere in the code as the index expression
//!
//! ```text
//! cell = (W >> 1) * y + (x >> 1)
//! ```
//!
//! which appears in `map->vfunc(0x30)(x, y)` (`0x5dc1c0`), in the global
//! mirror at `0x9c74b4` / `0x9c73ac`, and in the region map that `0x5eddb0`
//! and `0x5ee470` paint.
//!
//! The array is `word[map + 0x40]` entries long, and `map + 0x148` points at
//! it. Bounds-checked access is `0x5d16a0`, which returns the shared dummy
//! object at `0xCAA330` for out-of-range indices; the unchecked form is
//! `map->vfunc(0x34)(i)` (`0x5dc1b0`).
//!
//! # Cell layout (0xDC bytes, vtable `0x6701C8` / `0x670690`)
//!
//! Only the fields the generator touches are modelled here.

/// Size of a cell in the original binary, kept for reference.
pub const CELL_SIZE: usize = 0xDC;

/// A terrain class value.
///
/// The field at `+0x2C` packs two 4-bit values: bits 12..15 (the "terrain",
/// read by `vfunc(0xC8)`) and bits 8..11 (the "sub-class", read by
/// `vfunc(0xC4)`). `vfunc(0x128)(t, -1, -1)` stores `t` in bits 12..15 and
/// derives the sub-class (the base terrain underneath) from it:
///
/// | `t`      | stored sub-class |
/// |-----------|--------------|
/// | `0..=3`    | `t`          |
/// | `4` (flood plain) | `0`  |
/// | `5,6,8,9,10` | `2`       |
/// | `7` (forest) | `2`, unless a sub-class in `1..=10` is already set |
/// | `>= 11`   | `t`          |
///
/// Classes `11`, `12` and `13` are the water classes; `vfunc(0x8C)` is
/// `11 <= class <= 13`.
pub const CLASS_PLAINS: u8 = 0;
/// `class == 2`, the group that `4, 5, 6, 8, 9, 10` collapse into.
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

/// Returns `true` for the water classes — `Cell::vfunc(0x8C)` at `0x5EAA30`.
///
/// The predicate is `11 <= class <= 13`. It is named `is_candidate` in older
/// notes of this generator, but the land/sea stage (`0x5eceb0`) uses it to test
/// for **water**, and the biome stage (`0x5f1480`) skips cells where it holds.
/// The three water classes are: 11 shallow/coast, 12 deep, 13 the deepest.
/// They are not the same as the 11/12/13 "start-site candidate" markers the
/// start-placement stages write; those go through the same slot and the
/// distinction is not visible in the cell layout.
#[inline]
pub const fn is_water(class: u8) -> bool {
    class > 10 && class < 14
}

/// Normalises a raw terrain index the way `vfunc(0x128)` does.
///
/// `sub_class` is the current value of `vfunc(0xC4)`.
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

/// Number of flag planes the cell carries from `+0x28`, i.e. `(0x40 - 0x28) / 4`.
pub const NUM_FLAG_PLANES: usize = 6;

/// Plane 0, `Cell+0x28`: the overlay bits (road, mine, goody hut `0x20`, ...).
pub const PLANE_OVERLAY: usize = 0;
/// Plane 1, `Cell+0x2C`: the terrain word (class in bits 12..15, sub-class in
/// bits 8..11).
pub const PLANE_TERRAIN: usize = 1;
/// Plane 2, `Cell+0x30`: the feature plane (bonus grassland `0x10000`, start
/// location `0x80000`, snow-capped `0x100000`, pine forest `0x200000`, ...).
pub const PLANE_FEATURE: usize = 2;

/// Continent id used for "not assigned yet".
pub const NO_CONTINENT: u16 = 0xFFFF;

/// The terrain word of a freshly constructed cell: class and sub-class 13.
pub const TERRAIN_DEFAULT: u32 = 0xDD00;

/// One cell of the map: the part of the game's `Cell` object (`0xDC` bytes)
/// that `Map::generate` writes, at the offsets it has in the object. The
/// `.biq` `TILE` row and the `.sav` per-cell chunks carry the same fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    /// `+0x04`: eight-direction river adjacency mask.
    pub river: u8,
    /// `+0x08`: `GOOD` row of the resource, `-1` for none.
    pub resource: i32,
    /// `+0x10`: terrain sprite variant.
    pub image: u8,
    /// `+0x11`: terrain sprite file.
    pub file: u8,
    /// `+0x1E`: continent id, [`NO_CONTINENT`] when unassigned.
    pub continent: u16,
    /// `+0x28..+0x40`: overlay, terrain word, feature plane and three more
    /// words; see the `PLANE_*` constants.
    pub planes: [u32; NUM_FLAG_PLANES],
}

impl Default for Cell {
    /// The cell `Map::generate` starts from. The constructor (`0x5E98A0` via
    /// `0x5E9850`) zeroes the terrain word and calls `setTerrain(13)`, so a new
    /// cell is deep water; the land/sea stage relies on that default when it
    /// leaves a cell untouched.
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
    /// Terrain class: bits 12..15 of the terrain word (`vfunc(0xC8)`).
    #[inline]
    pub fn class(&self) -> u8 {
        ((self.planes[PLANE_TERRAIN] >> 12) & 0xF) as u8
    }

    /// Sub-class: bits 8..11 of the terrain word (`vfunc(0xC4)`).
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

    /// `vfunc(0x128)(t, -1, -1)` - `Cell::setTerrain` at `0x5E9940`.
    ///
    /// Writes both nibbles of the terrain word: bits 12..15 (`vfunc(0x0C)`) get
    /// `t`, bits 8..11 (`vfunc(0x04)`, or `vfunc(0x08)` for water) get the
    /// base terrain from [`normalise_class`]. Desert (`t == 0`) on a cell that
    /// already has a river becomes flood plain (class 4, base 0) instead;
    /// `vfunc(0x60)` is "river mask is non-zero".
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

    /// `vfunc(0xF4)(mask)` - `Cell::addRiver`, `0x5EACC0`: ORs `mask` into the
    /// river mask, and a desert tile that now has a river becomes flood plain
    /// (`vfunc(0x0C)(4)`: the class nibble only).
    #[inline]
    pub fn add_river(&mut self, mask: u8) {
        self.river |= mask;
        if self.class() == 0 {
            self.put_class(4);
        }
    }

    /// `vfunc(0xD0)(mask)` - `Cell::removeRiver`, `0x5EAB70`: clears `mask` from
    /// the river mask; when nothing is left, [`Cell::clear_river`] runs.
    #[inline]
    pub fn remove_river(&mut self, mask: u8) {
        self.river &= !mask;
        if self.river == 0 {
            self.clear_river();
        }
    }

    /// `vfunc(0xD4)()`, `0x5EAB90`: no river at all. Zeroes the mask, clears the
    /// four river-corner bits of the feature plane (`0x0F00_0000`), and a flood
    /// plain goes back to desert.
    #[inline]
    pub fn clear_river(&mut self) {
        self.river = 0;
        self.clear_flag(PLANE_FEATURE, 0x0F00_0000);
        if self.class() == 4 {
            self.put_class(0);
        }
    }

    /// `vfunc(0xE8)(id)` / `vfunc(0xB8)()` - continent id accessors.
    #[inline]
    pub fn set_continent(&mut self, id: u16) {
        self.continent = id;
    }

    /// `vfunc(0xB8)()`.
    #[inline]
    pub fn continent(&self) -> u16 {
        self.continent
    }

    /// `vfunc(0x8C)()`.
    #[inline]
    pub fn is_water(&self) -> bool {
        is_water(self.class())
    }

    /// `vfunc(0xE0)(plane, mask)` - set bits in one of the flag planes.
    ///
    /// Planes are `dword`s, not bits within one word, which is why the mask in
    /// the binary is frequently a byte or a nibble-sized constant like `0x200`.
    #[inline]
    pub fn set_flag(&mut self, plane: usize, mask: u32) {
        if let Some(p) = self.planes.get_mut(plane) {
            *p |= mask;
        }
    }

    /// `vfunc(0xCC)(plane, mask)` - clear bits in one of the flag planes.
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
    /// `word[+0x1F0]`. Bit 0 wraps x, bit 1 wraps y.
    pub wrap_flags: u32,
    /// "Oceans" slider, 0..100. Seeds every RNG in the generator and scales
    /// the ocean coverage.
    pub water_level: i32,
    /// The cell grid, `(w/2) * h` entries, row-major.
    pub cells: Vec<Cell>,
}

impl MapGrid {
    /// Creates an empty grid. Every cell starts with class 0 (ocean).
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

    /// The generator seed, `Map+0x1EC`. (The field is called `water_level` for
    /// historical reasons; it has always held the seed.)
    #[inline]
    pub fn seed(&self) -> i32 {
        self.water_level
    }

    /// Number of cells, i.e. `word[map + 0x40]`.
    #[inline]
    pub fn num_cells(&self) -> usize {
        self.cells.len()
    }

    /// Cell index for map coordinates.
    #[inline]
    pub fn index(&self, x: i32, y: i32) -> usize {
        // `Map::getTile` (0x5DC1C0) halves x with a 16-bit logical shift, so a
        // negative x does not land in the previous row.
        (((self.w >> 1) * y + i32::from((x as u16) >> 1)) & 0xFFFF) as usize
    }

    /// `map->vfunc(0x30)(x, y)`, bounds-checked; out-of-range yields a scratch
    /// cell the caller must not retain (the binary returns a shared dummy).
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

    /// `map->vfunc(0x34)(i)` — cell by flat index, unchecked.
    #[inline]
    pub fn cell(&self, i: usize) -> Option<&Cell> {
        self.cells.get(i)
    }

    /// Mutable variant of [`MapGrid::cell`].
    #[inline]
    pub fn cell_mut(&mut self, i: usize) -> Option<&mut Cell> {
        self.cells.get_mut(i)
    }

    /// Recovers `(x, y)` from a flat cell index.
    ///
    /// This is the inverse of [`MapGrid::index`] and appears verbatim in
    /// `0x5eb7d0`, `0x5edb70` and friends as
    /// `y = i / (W>>1); x = 2*(i % (W>>1)) + (y & 1)`.
    ///
    /// Note the asymmetry: the recovered `x` is the *nominal* tile of the
    /// 2-wide cell — the even tile in even rows, the odd tile in odd rows.
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

    /// The nominal start slot for civ `i`, the arithmetic every stage uses.
    #[inline]
    pub fn nominal_slot(&self, i: usize) -> (i32, i32) {
        self.coords(i)
    }

    /// Single-step wrap of `v` into `0..n`, matching the binary's idiom
    /// (valid because offsets are small).
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
        // (W/2) columns x H rows.
        assert_eq!(g.num_cells(), 80 * 160);
        assert_eq!(g.cell_at(0, 0).map(|c| c.class()), Some(WATER_ABYSSAL));
        assert!(g.cell_at(159, 159).is_some());
        assert!(g.cell_at(160, 159).is_none(), "x == W is out of range");
        assert!(g.cell_at(159, 160).is_none(), "y == H is out of range");
    }

    #[test]
    fn two_adjacent_columns_share_a_cell() {
        // The whole point of the (W/2) x H grid: x and x+1 land in the same
        // cell, so a cell covers a 2-wide pair of map columns.
        let g = MapGrid::new(60, 60, 0, 0);
        for y in 0..60 {
            for x in (0..60).step_by(2) {
                assert_eq!(g.index(x, y), g.index(x + 1, y), "at {x},{y}");
            }
            assert_ne!(g.index(0, y), g.index(2, y));
        }
    }

    #[test]
    fn index_matches_the_binary_expression() {
        let g = MapGrid::new(100, 60, 0, 0);
        for (x, y) in [(0, 0), (1, 0), (2, 3), (99, 59), (50, 30)] {
            assert_eq!(g.index(x, y), ((100 >> 1) * y + (x >> 1)) as usize);
        }
    }

    #[test]
    fn index_is_injective_over_cell_columns() {
        // Two map columns share a cell, so injectivity only holds once x is
        // halved. This pins the (W/2) x H shape from the other direction.
        let g = MapGrid::new(60, 60, 0, 0);
        let mut seen = std::collections::HashSet::new();
        for y in 0..60 {
            for cx in 0..30 {
                assert!(
                    seen.insert(g.index(cx * 2, y)),
                    "collision at cell column {cx}, row {y}"
                );
            }
        }
        assert_eq!(seen.len(), 30 * 60, "the index must be a bijection");
    }

    #[test]
    fn coords_and_index_are_mutual_inverses() {
        let g = MapGrid::new(60, 60, 0, 0);
        for i in 0..g.num_cells() {
            let (x, y) = g.coords(i);
            assert_eq!(
                ((x >> 1) as usize, y as usize),
                (i % 30, i / 30),
                "coords({i}) = ({x},{y}) does not decode the index"
            );
        }
    }

    #[test]
    fn coords_inverts_index() {
        let g = MapGrid::new(60, 60, 0, 0);
        for i in 0..g.num_cells() {
            let (x, y) = g.coords(i);
            assert!(x < 60 && y < 60, "i = {i} -> {x},{y} out of range");
        }
    }

    #[test]
    fn nominal_slots_are_distinct() {
        let g = MapGrid::new(60, 60, 0, 0);
        let mut seen = std::collections::HashSet::new();
        for i in 0..g.num_cells() {
            let s = g.nominal_slot(i);
            assert!(seen.insert(s), "duplicate slot {s:?} at i = {i}");
        }
    }

    #[test]
    fn wrapping_is_a_single_step() {
        let g = MapGrid::new(60, 60, 0b11, 0);
        assert_eq!(g.wrap_axis(-1, true), 59);
        assert_eq!(g.wrap_axis(60, true), 0);
        assert_eq!(g.wrap_axis(59, true), 59);
        let nw = MapGrid::new(60, 60, 0, 0);
        assert_eq!(nw.wrap_axis(-1, true), -1, "no wrap requested");
        assert_eq!(nw.wrap_and_check(60, 0), None);
    }

    #[test]
    fn class_normalisation_table() {
        assert_eq!(normalise_class(0, 0), 0);
        assert_eq!(normalise_class(3, 0), 3);
        assert_eq!(normalise_class(4, 0), CLASS_PLAINS);
        assert_eq!(normalise_class(5, 0), CLASS_GROUP2);
        assert_eq!(normalise_class(6, 0), CLASS_GROUP2);
        assert_eq!(normalise_class(7, 0), CLASS_GROUP2);
        assert_eq!(normalise_class(7, 5), 5, "hills keep their sub-class");
        assert_eq!(normalise_class(8, 0), CLASS_GROUP2);
        assert_eq!(normalise_class(10, 0), CLASS_GROUP2);
        assert_eq!(normalise_class(11, 0), 11);
        assert_eq!(normalise_class(13, 0), 13);
    }

    #[test]
    fn water_predicate() {
        for c in 0u8..=15 {
            assert_eq!(is_water(c), c > 10 && c < 14, "class {c}");
        }
        assert!(is_water(11) && is_water(13));
        assert!(!is_water(10) && !is_water(14));
    }

    #[test]
    fn flags_are_plane_addressed() {
        // (Plane 1 is the terrain word, which starts at TERRAIN_DEFAULT; planes 3..
        // start empty.)
        let mut c = Cell::default();
        c.set_flag(0, 0x20);
        c.set_flag(3, 0x200);
        assert_eq!(c.flag(0), 0x20);
        assert_eq!(c.flag(3), 0x200, "the mask is per-plane, not shifted");
        c.clear_flag(0, 0x20);
        assert_eq!(c.flag(0), 0);
        assert_eq!(c.flag(3), 0x200, "clearing plane 0 left plane 3 alone");
        assert_eq!(c.flag(NUM_FLAG_PLANES), 0, "out of range reads as 0");
    }
}
