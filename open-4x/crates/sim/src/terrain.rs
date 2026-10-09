//! Logical square coordinates are Civ3's native grid turned 45°: a tile's native column is `x - y`
//! and its row is `x + y` (see [`Lattice`]), so all eight neighbours are one step away.
//! Terrain art lives on the dual grid: each diamond's vertices are four tile centers.
//!
//! A square is built from layers, the way Civ3 builds a tile:
//!
//! * the **base** [`Terrain`]: ocean, sea, coast, grassland, plains, desert or tundra;
//! * the **relief** ([`Relief`]): flat ground, hills or mountains;
//! * the **cover** ([`Cover`]): forest, jungle or marsh growing on top of it;
//! * **rivers** along the square's edges ([`Tile::RIVER_E`], [`Tile::RIVER_S`]).
//!
//! A map serializes as stacked layers so a world stays small and diffable:
//!
//! * `lattice`: optional `{columns, rows}`. Present, it says the world is an upright rectangle of
//!   tiles embedded in the square grid (see [`Lattice`]); the cells around it are void and every
//!   layer must leave them as plain ocean.
//! * `terrain`: one string per row, one glyph per tile (see [`Terrain::glyph`]).
//! * `relief`: rows of `.` flat, `h` hills, `^` mountains. Omitted when the map is flat.
//! * `cover`: rows of `.` bare, `f` forest, `j` jungle, `m` marsh. Omitted when bare.
//! * `rivers`: run-length pairs `[edges, count, ...]`: bit 0 is the river on a tile's east edge
//!   (between it and the tile at x+1), bit 1 the one on its south edge (y+1).
//! * `regions`: run-length pairs `[region, count, ...]` in row-major order. `0` is unclaimed.
//! * `owners`: the same encoding for the current owning nation. Scenarios omit it.
//! * `claims`: the same encoding for the city whose preset border region holds each tile.
//!   Scenarios omit it; the simulation derives it from the cities' border levels.
//! * `improvements`: the same encoding for the worker improvements on each tile
//!   (see [`Tile::ROAD`]). Scenarios may pre-build roads, railroads, mines and farms.
//!
//! Rows also accept the old combined glyphs `. , f F m M n N` (grass or sand, with forest or
//! mountains already on it) so hand-written maps keep working; they are written back in layers.
use serde::{Deserialize, Serialize};

pub const MAX_DIMENSION: i32 = 1024;
pub const MAX_TILES: usize = 1 << 20;

#[derive(
    Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub struct Coord {
    pub x: i32,
    pub y: i32,
}
impl Coord {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
    pub fn distance(self, other: Self) -> i32 {
        (self.x - other.x).abs().max((self.y - other.y).abs())
    }
    /// The eight Chebyshev neighbours. On the logical grid these are Civ3's eight directions.
    pub const NEIGHBORS: [(i32, i32); 8] = [
        (0, -1),
        (1, -1),
        (1, 0),
        (1, 1),
        (0, 1),
        (-1, 1),
        (-1, 0),
        (-1, -1),
    ];
    pub const fn offset(self, dx: i32, dy: i32) -> Self {
        Self::new(self.x + dx, self.y + dy)
    }
    /// Squared Euclidean distance on the logical grid; Civ3's border reach is measured this way.
    pub fn distance_squared(self, other: Self) -> i32 {
        let (dx, dy) = (self.x - other.x, self.y - other.y);
        dx * dx + dy * dy
    }
    pub fn civ3(self) -> (i32, i32) {
        (self.x + self.y, self.y - self.x)
    }
    pub fn screen(self) -> (f32, f32) {
        (
            (self.x - self.y) as f32 * 64.0,
            -(self.x + self.y) as f32 * 32.0,
        )
    }
    pub fn from_screen(x: f32, y: f32) -> Self {
        Self::new(
            (x / 128.0 - y / 64.0).round() as i32,
            (-x / 128.0 - y / 64.0).round() as i32,
        )
    }
}

/// The base of a square. Water beside land is drawn as coast whatever it says here.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Terrain {
    Ocean,
    Sea,
    Coast,
    Grass,
    Plains,
    Desert,
    Tundra,
}
impl Terrain {
    pub const ALL: [Terrain; 7] = [
        Self::Ocean,
        Self::Sea,
        Self::Coast,
        Self::Grass,
        Self::Plains,
        Self::Desert,
        Self::Tundra,
    ];
    pub const fn is_water(self) -> bool {
        matches!(self, Self::Ocean | Self::Sea | Self::Coast)
    }
    pub const fn is_land(self) -> bool {
        !self.is_water()
    }
    /// Row glyph: `~` ocean, `s` sea, `c` coast, `g` grassland, `p` plains, `d` desert, `t` tundra.
    pub const fn glyph(self) -> char {
        match self {
            Self::Ocean => '~',
            Self::Sea => 's',
            Self::Coast => 'c',
            Self::Grass => 'g',
            Self::Plains => 'p',
            Self::Desert => 'd',
            Self::Tundra => 't',
        }
    }
    pub fn from_glyph(glyph: char) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.glyph() == glyph)
    }
    pub const fn name(self) -> &'static str {
        match self {
            Self::Ocean => "Ocean",
            Self::Sea => "Sea",
            Self::Coast => "Coast",
            Self::Grass => "Grassland",
            Self::Plains => "Plains",
            Self::Desert => "Desert",
            Self::Tundra => "Tundra",
        }
    }
    /// Digit of a land type on the ground sheet: grassland 0, plains 1, desert 2, tundra 3.
    pub const fn ground_digit(self) -> Option<usize> {
        match self {
            Self::Grass => Some(0),
            Self::Plains => Some(1),
            Self::Desert => Some(2),
            Self::Tundra => Some(3),
            _ => None,
        }
    }
    /// Digit on the water sheet: land 0, coast 1, sea 2, ocean 3.
    pub const fn water_digit(self) -> usize {
        match self {
            Self::Coast => 1,
            Self::Sea => 2,
            Self::Ocean => 3,
            _ => 0,
        }
    }
}

/// How the ground rises. Mountains and hills can carry forest or jungle; marsh needs flat ground.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Relief {
    #[default]
    Flat,
    Hills,
    Mountains,
}
impl Relief {
    pub const fn glyph(self) -> char {
        match self {
            Self::Flat => '.',
            Self::Hills => 'h',
            Self::Mountains => '^',
        }
    }
    pub fn from_glyph(glyph: char) -> Option<Self> {
        [Self::Flat, Self::Hills, Self::Mountains]
            .into_iter()
            .find(|r| r.glyph() == glyph)
    }
}

/// What grows on the square.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Cover {
    #[default]
    Bare,
    Forest,
    Jungle,
    Marsh,
}
impl Cover {
    pub const fn glyph(self) -> char {
        match self {
            Self::Bare => '.',
            Self::Forest => 'f',
            Self::Jungle => 'j',
            Self::Marsh => 'm',
        }
    }
    pub fn from_glyph(glyph: char) -> Option<Self> {
        [Self::Bare, Self::Forest, Self::Jungle, Self::Marsh]
            .into_iter()
            .find(|c| c.glyph() == glyph)
    }
}

/// One map square. `region` is a 1-based scenario region (0 = none).
///
/// `claim` is the city whose preset border region holds the square (0 = none) and `owner` is
/// that city's current owner, so a conquered city carries its whole region with it.
/// `improvements` holds the worker improvements as a bit set.
/// `river` holds the rivers on the square's east and south edges; the other two edges belong to
/// the neighbours (see [`Map::river_mask`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Tile {
    pub position: Coord,
    pub terrain: Terrain,
    pub relief: Relief,
    pub cover: Cover,
    pub river: u8,
    pub owner: u32,
    pub claim: u32,
    pub region: u16,
    pub improvements: u8,
}
impl Tile {
    pub const ROAD: u8 = 1;
    /// A railroad is built on a road and keeps it.
    pub const RAIL: u8 = 2;
    pub const MINE: u8 = 4;
    /// Irrigation. A tile holds a mine or a farm, never both.
    pub const FARM: u8 = 8;
    pub const IMPROVEMENT_MASK: u8 = 15;
    /// A river runs along the edge shared with the tile to the east (x+1).
    pub const RIVER_E: u8 = 1;
    /// A river runs along the edge shared with the tile to the south (y+1).
    pub const RIVER_S: u8 = 2;

    pub fn new(position: Coord, terrain: Terrain) -> Self {
        Self {
            position,
            terrain,
            relief: Relief::Flat,
            cover: Cover::Bare,
            river: 0,
            owner: 0,
            claim: 0,
            region: 0,
            improvements: 0,
        }
    }
    pub fn is_land(&self) -> bool {
        self.terrain.is_land()
    }
    pub fn is_water(&self) -> bool {
        self.terrain.is_water()
    }
    pub fn is_forest(&self) -> bool {
        self.cover == Cover::Forest
    }
    pub fn is_mountain(&self) -> bool {
        self.relief == Relief::Mountains
    }
    pub fn is_hills(&self) -> bool {
        self.relief == Relief::Hills
    }
    /// Turn the square into `terrain`, dropping every layer that cannot exist there.
    pub fn set_terrain(&mut self, terrain: Terrain) {
        self.terrain = terrain;
        if terrain.is_water() {
            self.relief = Relief::Flat;
            self.cover = Cover::Bare;
            self.river = 0;
            self.improvements = 0;
        }
    }
    pub fn has_road(&self) -> bool {
        self.improvements & (Self::ROAD | Self::RAIL) != 0
    }
    pub fn has_rail(&self) -> bool {
        self.improvements & Self::RAIL != 0
    }
    /// Whole moves spent entering this square without a road: Civ3's terrain move cost
    /// (flat 1, hills 2, forest/jungle/marsh 2, mountains 3).
    pub fn move_cost(&self) -> u32 {
        let relief = match self.relief {
            Relief::Mountains => 3,
            Relief::Hills => 2,
            Relief::Flat => 1,
        };
        let cover = if self.cover == Cover::Bare { 1 } else { 2 };
        relief.max(cover)
    }
    /// Defender bonus in percent: Civ3's terrain table (flat 10, hills 50, mountains 100,
    /// forest and jungle 25, marsh 20). The best of relief and cover counts.
    pub fn defense_percent(&self) -> i32 {
        if self.is_water() {
            return 10;
        }
        let relief = match self.relief {
            Relief::Mountains => 100,
            Relief::Hills => 50,
            Relief::Flat => 10,
        };
        let cover = match self.cover {
            Cover::Forest | Cover::Jungle => 25,
            Cover::Marsh => 20,
            Cover::Bare => 10,
        };
        relief.max(cover)
    }
    /// Base terrain, relief and cover for a row glyph, or `None` for unknown characters. Besides
    /// the base glyphs this reads the old combined ones: `.` grass, `,` desert, and `f` `m` `n`
    /// (forest, mountains, forested mountains on grass) with `F` `M` `N` on desert.
    pub fn from_glyph(glyph: char) -> Option<(Terrain, Relief, Cover)> {
        use {Cover::*, Relief::*, Terrain::*};
        Some(match glyph {
            '.' => (Grass, Flat, Bare),
            ',' => (Desert, Flat, Bare),
            'f' => (Grass, Flat, Forest),
            'm' => (Grass, Mountains, Bare),
            'n' => (Grass, Mountains, Forest),
            'F' => (Desert, Flat, Forest),
            'M' => (Desert, Mountains, Bare),
            'N' => (Desert, Mountains, Forest),
            other => (Terrain::from_glyph(other)?, Flat, Bare),
        })
    }
    /// Whether the layers can coexist: water carries nothing, and marsh needs flat ground.
    pub fn layers_valid(&self) -> bool {
        if self.is_water() {
            return self.relief == Relief::Flat
                && self.cover == Cover::Bare
                && self.river == 0
                && self.improvements == 0;
        }
        !(self.cover == Cover::Marsh && self.relief != Relief::Flat)
    }
}

/// A Civ3-style map: a rectangle of tiles that stands upright on screen, north up and east right,
/// with every other row shifted half a tile. The simulation's square grid is that rectangle
/// turned 45°, so the rectangle is embedded in a square of `side() * side()` cells and the cells
/// outside it are void: they hold nothing, and [`Map::get`] answers `None` for them as it does
/// for anything off the map.
///
/// Native column `cx` runs `0..2 * columns` and row `cy` runs `0..rows`; a tile exists where
/// `cx + cy` is even. Tile `(x, y)` of the square sits at `cx = x - y + columns` and
/// `cy = x + y - columns`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lattice {
    /// Tiles in each row; half the width of the rectangle measured in half-tile columns.
    pub columns: i32,
    /// Rows, north to south. Even, so the rectangle closes flush with the square.
    pub rows: i32,
}
impl Lattice {
    pub fn validate(self) -> Result<(), String> {
        if self.columns < 1 || self.rows < 2 || self.rows % 2 != 0 {
            return Err("a lattice needs at least one column and an even number of rows".into());
        }
        if self.side() > MAX_DIMENSION || (self.side() as usize).pow(2) > MAX_TILES {
            return Err(format!("a lattice must fit a {MAX_DIMENSION}-tile square"));
        }
        Ok(())
    }
    /// Width and height of the square that holds the rectangle.
    pub const fn side(self) -> i32 {
        self.columns + self.rows / 2
    }
    /// Tiles inside the rectangle.
    pub const fn tile_count(self) -> usize {
        self.columns as usize * self.rows as usize
    }
    /// Native `(column, row)` of a square cell. Only meaningful where [`Lattice::contains`].
    pub const fn native(self, p: Coord) -> (i32, i32) {
        (p.x - p.y + self.columns, p.x + p.y - self.columns)
    }
    /// The square cell of native column `cx`, row `cy` (`cx + cy` must be even).
    pub const fn cell(self, cx: i32, cy: i32) -> Coord {
        Coord::new((cx + cy) / 2, (cy - cx) / 2 + self.columns)
    }
    pub const fn contains(self, p: Coord) -> bool {
        let (cx, cy) = self.native(p);
        cx >= 0 && cx < 2 * self.columns && cy >= 0 && cy < self.rows
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "MapRepr", into = "MapRepr")]
pub struct Map {
    pub width: i32,
    pub height: i32,
    pub tiles: Vec<Tile>,
    /// Set when the world is a screen-upright rectangle embedded in the square grid.
    pub lattice: Option<Lattice>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MapRepr {
    width: i32,
    height: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    lattice: Option<Lattice>,
    terrain: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    relief: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    cover: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    rivers: Vec<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    regions: Vec<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    owners: Vec<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    claims: Vec<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    improvements: Vec<u32>,
}

/// Run-length encode as flat `[value, count, value, count, ...]`. An all-zero layer is empty.
fn encode_runs(values: impl Iterator<Item = u32>) -> Vec<u32> {
    let mut runs: Vec<u32> = Vec::new();
    for value in values {
        match runs.as_mut_slice() {
            [.., last_value, count] if *last_value == value => *count += 1,
            _ => runs.extend([value, 1]),
        }
    }
    if runs.len() == 2 && runs[0] == 0 {
        runs.clear();
    }
    runs
}
/// Decode into exactly `len` values. The total is checked before any allocation so malformed
/// input cannot request unbounded memory. An empty list means every value is zero.
fn decode_runs(runs: &[u32], len: usize) -> Result<Vec<u32>, String> {
    if runs.is_empty() {
        return Ok(vec![0; len]);
    }
    if !runs.len().is_multiple_of(2) {
        return Err("run-length layer has an odd number of entries".into());
    }
    let total: u64 = runs.chunks_exact(2).map(|pair| u64::from(pair[1])).sum();
    if total != len as u64 {
        return Err(format!(
            "run-length layer covers {total} tiles, expected {len}"
        ));
    }
    let mut values = Vec::with_capacity(len);
    for pair in runs.chunks_exact(2) {
        values.extend(std::iter::repeat_n(pair[0], pair[1] as usize));
    }
    Ok(values)
}

impl From<Map> for MapRepr {
    fn from(map: Map) -> Self {
        Self {
            width: map.width,
            height: map.height,
            lattice: map.lattice,
            terrain: map.rows(),
            relief: map.layer_rows(|t| t.relief != Relief::Flat, |t| t.relief.glyph()),
            cover: map.layer_rows(|t| t.cover != Cover::Bare, |t| t.cover.glyph()),
            rivers: encode_runs(map.tiles.iter().map(|t| u32::from(t.river))),
            regions: encode_runs(map.tiles.iter().map(|t| u32::from(t.region))),
            owners: encode_runs(map.tiles.iter().map(|t| t.owner)),
            claims: encode_runs(map.tiles.iter().map(|t| t.claim)),
            improvements: encode_runs(map.tiles.iter().map(|t| u32::from(t.improvements))),
        }
    }
}
impl TryFrom<MapRepr> for Map {
    type Error = String;
    fn try_from(repr: MapRepr) -> Result<Self, String> {
        let mut map = Map::from_rows(&repr.terrain)?;
        if (map.width, map.height) != (repr.width, repr.height) {
            return Err("map width/height do not match its terrain rows".into());
        }
        if let Some(lattice) = repr.lattice {
            lattice.validate()?;
            if (map.width, map.height) != (lattice.side(), lattice.side()) {
                return Err(format!(
                    "a lattice of {} columns and {} rows is held in a {side} x {side} square",
                    lattice.columns,
                    lattice.rows,
                    side = lattice.side()
                ));
            }
            map.lattice = Some(lattice);
        }
        map.apply_layer(&repr.relief, "relief", |tile, glyph| {
            let relief = Relief::from_glyph(glyph).ok_or("unknown relief glyph")?;
            if relief != Relief::Flat && tile.relief != Relief::Flat && tile.relief != relief {
                return Err("relief conflicts with the terrain glyph");
            }
            tile.relief = relief;
            Ok(())
        })?;
        map.apply_layer(&repr.cover, "cover", |tile, glyph| {
            let cover = Cover::from_glyph(glyph).ok_or("unknown cover glyph")?;
            if cover != Cover::Bare && tile.cover != Cover::Bare && tile.cover != cover {
                return Err("cover conflicts with the terrain glyph");
            }
            tile.cover = cover;
            Ok(())
        })?;
        let len = map.tiles.len();
        let rivers = decode_runs(&repr.rivers, len)?;
        let regions = decode_runs(&repr.regions, len)?;
        let owners = decode_runs(&repr.owners, len)?;
        let claims = decode_runs(&repr.claims, len)?;
        let improvements = decode_runs(&repr.improvements, len)?;
        let lattice = map.lattice;
        for (i, tile) in map.tiles.iter_mut().enumerate() {
            tile.river = u8::try_from(rivers[i])
                .ok()
                .filter(|bits| bits & !(Tile::RIVER_E | Tile::RIVER_S) == 0)
                .ok_or("rivers must be 0 to 3: east edge, south edge, or both")?;
            tile.region = u16::try_from(regions[i]).map_err(|_| "region index exceeds 65535")?;
            tile.owner = owners[i];
            tile.claim = claims[i];
            tile.improvements = u8::try_from(improvements[i])
                .ok()
                .filter(|bits| bits & !Tile::IMPROVEMENT_MASK == 0)
                .ok_or("improvements must be a combination of road, rail, mine, and farm")?;
            let bits = tile.improvements;
            let (x, y) = (tile.position.x, tile.position.y);
            if lattice.is_some_and(|l| !l.contains(tile.position))
                && *tile != Tile::new(tile.position, Terrain::Ocean)
            {
                return Err(format!(
                    "cell {x},{y} lies outside the lattice and must be empty open ocean"
                ));
            }
            if !tile.layers_valid() {
                return Err(format!(
                    "tile {x},{y} mixes layers that cannot coexist: water holds no relief, cover, river, or improvement, and marsh needs flat ground"
                ));
            }
            if bits & Tile::RAIL != 0 && bits & Tile::ROAD == 0 {
                return Err(format!("railroad at {x},{y} needs a road"));
            }
            if bits & Tile::MINE != 0 && bits & Tile::FARM != 0 {
                return Err(format!("tile {x},{y} has both a mine and a farm"));
            }
        }
        for tile in &map.tiles {
            for (bit, dx, dy) in [(Tile::RIVER_E, 1, 0), (Tile::RIVER_S, 0, 1)] {
                let other = map.get(tile.position.offset(dx, dy));
                if tile.river & bit != 0 && !(tile.is_land() && other.is_some_and(Tile::is_land)) {
                    let (x, y) = (tile.position.x, tile.position.y);
                    return Err(format!(
                        "river at {x},{y} must run between two land tiles inside the map"
                    ));
                }
            }
        }
        Ok(map)
    }
}

impl Map {
    /// An all-`terrain` map with no owners or regions.
    pub fn filled(width: i32, height: i32, terrain: Terrain) -> Self {
        let tiles = (0..height)
            .flat_map(|y| (0..width).map(move |x| Tile::new(Coord::new(x, y), terrain)))
            .collect();
        Self {
            width,
            height,
            tiles,
            lattice: None,
        }
    }
    /// An all-`terrain` upright rectangle of tiles inside its square, the rest void.
    pub fn embedded(lattice: Lattice, terrain: Terrain) -> Self {
        let mut map = Self::filled(lattice.side(), lattice.side(), Terrain::Ocean);
        map.lattice = Some(lattice);
        if terrain != Terrain::Ocean {
            for tile in &mut map.tiles {
                if lattice.contains(tile.position) {
                    tile.terrain = terrain;
                }
            }
        }
        map
    }
    /// Parse glyph rows. Rejects ragged, oversized, or unknown-glyph input.
    pub fn from_rows(rows: &[String]) -> Result<Self, String> {
        let height = rows.len();
        let width = rows.first().map_or(0, |r| r.len());
        if height == 0
            || width == 0
            || height > MAX_DIMENSION as usize
            || width > MAX_DIMENSION as usize
        {
            return Err(format!("map must be 1–{MAX_DIMENSION} tiles on each side"));
        }
        if width * height > MAX_TILES {
            return Err(format!("map exceeds {MAX_TILES} tiles"));
        }
        let mut tiles = Vec::with_capacity(width * height);
        for (y, row) in rows.iter().enumerate() {
            if row.len() != width || !row.is_ascii() {
                return Err(format!("terrain row {y} must be {width} ASCII glyphs"));
            }
            for (x, glyph) in row.chars().enumerate() {
                let (terrain, relief, cover) = Tile::from_glyph(glyph)
                    .ok_or_else(|| format!("unknown terrain glyph {glyph:?} at {x},{y}"))?;
                let mut tile = Tile::new(Coord::new(x as i32, y as i32), terrain);
                tile.relief = relief;
                tile.cover = cover;
                tiles.push(tile);
            }
        }
        Ok(Self {
            width: width as i32,
            height: height as i32,
            tiles,
            lattice: None,
        })
    }
    pub fn rows(&self) -> Vec<String> {
        self.tiles
            .chunks(self.width.max(1) as usize)
            .map(|row| row.iter().map(|t| t.terrain.glyph()).collect())
            .collect()
    }
    /// Rows of one painted layer, or none at all when no tile uses it.
    fn layer_rows(
        &self,
        used: impl Fn(&Tile) -> bool,
        glyph: impl Fn(&Tile) -> char,
    ) -> Vec<String> {
        if !self.tiles.iter().any(used) {
            return Vec::new();
        }
        self.tiles
            .chunks(self.width.max(1) as usize)
            .map(|row| row.iter().map(&glyph).collect())
            .collect()
    }
    /// Apply one glyph row layer (empty = untouched), checking its shape against the map.
    fn apply_layer(
        &mut self,
        rows: &[String],
        name: &str,
        apply: impl Fn(&mut Tile, char) -> Result<(), &'static str>,
    ) -> Result<(), String> {
        if rows.is_empty() {
            return Ok(());
        }
        let width = self.width as usize;
        if rows.len() != self.height as usize
            || rows.iter().any(|r| r.len() != width || !r.is_ascii())
        {
            return Err(format!(
                "{name} layer must be {} rows of {width} ASCII glyphs",
                self.height
            ));
        }
        for (tile, glyph) in self
            .tiles
            .iter_mut()
            .zip(rows.iter().flat_map(|r| r.chars()))
        {
            let (x, y) = (tile.position.x, tile.position.y);
            apply(tile, glyph)
                .map_err(|why| format!("{name} glyph {glyph:?} at {x},{y}: {why}"))?;
        }
        Ok(())
    }
    /// Row-major index of a tile, or `None` off the map and in the void around a lattice.
    pub fn index(&self, p: Coord) -> Option<usize> {
        (p.x >= 0
            && p.y >= 0
            && p.x < self.width
            && p.y < self.height
            && self.lattice.is_none_or(|lattice| lattice.contains(p)))
        .then(|| (p.y * self.width + p.x) as usize)
    }
    /// Whether a tile exists at `p`.
    pub fn contains(&self, p: Coord) -> bool {
        self.index(p).is_some()
    }
    /// Number of tiles that exist; a lattice's void is not counted.
    pub fn tile_count(&self) -> usize {
        self.lattice
            .map_or(self.tiles.len(), |lattice| lattice.tile_count())
    }
    /// Every tile that exists, in row-major order.
    pub fn positions(&self) -> impl Iterator<Item = Coord> + '_ {
        self.tiles
            .iter()
            .map(|tile| tile.position)
            .filter(|&p| self.contains(p))
    }
    /// The rectangle `(min_x, min_y, max_x, max_y)` that the centres of the map's tiles span on
    /// screen (see [`Coord::screen`]), y pointing up. A lattice spans an upright rectangle;
    /// a plain square grid spans a diamond, so the rectangle here is its bounding box.
    pub fn screen_bounds(&self) -> (f32, f32, f32, f32) {
        match self.lattice {
            Some(Lattice { columns, rows }) => (
                -(columns as f32) * 64.0,
                -((rows - 1 + columns) as f32) * 32.0,
                (columns - 1) as f32 * 64.0,
                -(columns as f32) * 32.0,
            ),
            None => (
                -((self.height - 1) as f32) * 64.0,
                -((self.width + self.height - 2) as f32) * 32.0,
                (self.width - 1) as f32 * 64.0,
                0.0,
            ),
        }
    }
    pub fn get(&self, p: Coord) -> Option<&Tile> {
        self.index(p).and_then(|i| self.tiles.get(i))
    }
    pub fn get_mut(&mut self, p: Coord) -> Option<&mut Tile> {
        self.index(p).and_then(|i| self.tiles.get_mut(i))
    }
    /// The in-bounds Chebyshev neighbours, clockwise from north.
    pub fn neighbors(&self, p: Coord) -> Vec<Coord> {
        Coord::NEIGHBORS
            .into_iter()
            .map(|(x, y)| p.offset(x, y))
            .filter(|p| self.index(*p).is_some())
            .collect()
    }
    /// Base terrain at `(x, y)`; everything off the map is open ocean.
    fn terrain_at(&self, x: i32, y: i32) -> Terrain {
        self.get(Coord::new(x, y))
            .map_or(Terrain::Ocean, |t| t.terrain)
    }
    /// The eight neighbours, orthogonal ones first, so ties between land types break the same way
    /// for every cell that touches a tile.
    const SHORE_ORDER: [(i32, i32); 8] = [
        (0, -1),
        (1, 0),
        (0, 1),
        (-1, 0),
        (1, -1),
        (1, 1),
        (-1, 1),
        (-1, -1),
    ];
    /// Ground-sheet digit under a vertex: a land tile's own type, or for water the land beside it
    /// (the water layer paints over it). Open sea has none, and answers grassland.
    fn ground_under(&self, x: i32, y: i32) -> usize {
        if let Some(digit) = self.terrain_at(x, y).ground_digit() {
            return digit;
        }
        Self::SHORE_ORDER
            .iter()
            .find_map(|&(dx, dy)| self.terrain_at(x + dx, y + dy).ground_digit())
            .unwrap_or(0)
    }
    /// Water-sheet digit of a vertex: land 0, and 1 coast, 2 sea, 3 ocean. Water that touches
    /// land is always coast, whatever the data says, so a cell never mixes land with deep water.
    fn water_under(&self, x: i32, y: i32) -> usize {
        let own = self.terrain_at(x, y);
        if own.is_land() {
            return 0;
        }
        let touches_land = Self::SHORE_ORDER
            .iter()
            .any(|&(dx, dy)| self.terrain_at(x + dx, y + dy).is_land());
        if touches_land { 1 } else { own.water_digit() }
    }
    /// The four vertices of the dual cell whose north vertex is tile `corner`:
    /// north `(x, y)`, east `(x+1, y)`, south `(x+1, y+1)`, west `(x, y+1)`.
    fn cell_vertices(corner: Coord) -> [(i32, i32); 4] {
        let (x, y) = (corner.x, corner.y);
        [(x, y), (x + 1, y), (x + 1, y + 1), (x, y + 1)]
    }
    /// Cell of the ground sheet, or `None` when all four vertices are water. Index
    /// `(4*S + E) * 16 + 4*W + N`; a cell with `N == E == S == W` is a pure one, `85 * digit`.
    pub fn ground_cell(&self, corner: Coord) -> Option<usize> {
        let v = Self::cell_vertices(corner);
        if v.iter().all(|&(x, y)| self.terrain_at(x, y).is_water()) {
            return None;
        }
        let [n, e, s, w] = v.map(|(x, y)| self.ground_under(x, y));
        Some((4 * s + e) * 16 + 4 * w + n)
    }
    /// Cell of the water sheet, or `None` when all four vertices are land; indexed like the ground.
    pub fn water_cell(&self, corner: Coord) -> Option<usize> {
        let [n, e, s, w] = Self::cell_vertices(corner).map(|(x, y)| self.water_under(x, y));
        (n | e | s | w != 0).then_some((4 * s + e) * 16 + 4 * w + n)
    }
    /// River branches in the dual cell at `corner`, as a mask for the river sheet:
    /// bit 0 north-east (the edge between the N and E tiles), bit 1 south-east, bit 2 south-west,
    /// bit 3 north-west. Zero when no river touches the corner.
    pub fn river_cell(&self, corner: Coord) -> usize {
        let (x, y) = (corner.x, corner.y);
        let has = |x: i32, y: i32, bit: u8| {
            self.get(Coord::new(x, y))
                .is_some_and(|t| t.river & bit != 0) as usize
        };
        has(x, y, Tile::RIVER_E)
            | has(x + 1, y, Tile::RIVER_S) << 1
            | has(x, y + 1, Tile::RIVER_E) << 2
            | has(x, y, Tile::RIVER_S) << 3
    }
    /// Rivers on the four edges of a tile: bit 0 north, 1 east, 2 south, 3 west.
    pub fn river_mask(&self, p: Coord) -> u8 {
        let edge = |q: Coord, bit: u8| self.get(q).is_some_and(|t| t.river & bit != 0) as u8;
        edge(p.offset(0, -1), Tile::RIVER_S)
            | edge(p, Tile::RIVER_E) << 1
            | edge(p, Tile::RIVER_S) << 2
            | edge(p.offset(-1, 0), Tile::RIVER_E) << 3
    }
    /// Whether a river runs along the edge between two orthogonally adjacent tiles.
    pub fn river_between(&self, a: Coord, b: Coord) -> bool {
        match (b.x - a.x, b.y - a.y) {
            (0, -1) => self.river_mask(a) & 1 != 0,
            (1, 0) => self.river_mask(a) & 2 != 0,
            (0, 1) => self.river_mask(a) & 4 != 0,
            (-1, 0) => self.river_mask(a) & 8 != 0,
            _ => false,
        }
    }
}

/// Per-nation fog of war: one bit per tile, serialized as alternating
/// unexplored/explored run lengths (starting with unexplored) in row-major order.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "ExploredRepr", into = "ExploredRepr")]
pub struct Explored {
    width: i32,
    height: i32,
    bits: Vec<u64>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExploredRepr {
    width: i32,
    height: i32,
    runs: Vec<u32>,
}

impl Explored {
    pub fn new(width: i32, height: i32) -> Self {
        let tiles = (width.max(0) as usize) * (height.max(0) as usize);
        Self {
            width,
            height,
            bits: vec![0; tiles.div_ceil(64)],
        }
    }
    fn locate(&self, p: Coord) -> Option<(usize, u64)> {
        (p.x >= 0 && p.y >= 0 && p.x < self.width && p.y < self.height).then(|| {
            let i = (p.y * self.width + p.x) as usize;
            (i / 64, 1 << (i % 64))
        })
    }
    pub fn contains(&self, p: Coord) -> bool {
        self.locate(p)
            .is_some_and(|(word, mask)| self.bits[word] & mask != 0)
    }
    pub fn insert(&mut self, p: Coord) {
        if let Some((word, mask)) = self.locate(p) {
            self.bits[word] |= mask;
        }
    }
    /// Reveal every tile within Chebyshev `radius` of `center`.
    pub fn reveal(&mut self, center: Coord, radius: i32) {
        for y in (center.y - radius).max(0)..=(center.y + radius).min(self.height - 1) {
            for x in (center.x - radius).max(0)..=(center.x + radius).min(self.width - 1) {
                self.insert(Coord::new(x, y));
            }
        }
    }
    /// Mark every tile of the map as explored.
    pub fn fill(&mut self) {
        let total = (self.width.max(0) as usize) * (self.height.max(0) as usize);
        self.bits.fill(u64::MAX);
        // keep the unused tail of the last word clear so `len` counts tiles only
        if !total.is_multiple_of(64)
            && let Some(last) = self.bits.last_mut()
        {
            *last = (1u64 << (total % 64)) - 1;
        }
    }
    pub fn clear(&mut self) {
        self.bits.fill(0);
    }
    pub fn len(&self) -> usize {
        self.bits.iter().map(|w| w.count_ones() as usize).sum()
    }
    pub fn is_empty(&self) -> bool {
        self.bits.iter().all(|w| *w == 0)
    }
}
impl From<Explored> for ExploredRepr {
    fn from(explored: Explored) -> Self {
        let total = (explored.width.max(0) as usize) * (explored.height.max(0) as usize);
        let mut runs = Vec::new();
        let (mut current, mut count) = (false, 0u32);
        for i in 0..total {
            let bit = explored.bits[i / 64] & (1 << (i % 64)) != 0;
            if bit == current {
                count += 1;
            } else {
                runs.push(count);
                current = bit;
                count = 1;
            }
        }
        if total > 0 {
            runs.push(count);
        }
        Self {
            width: explored.width,
            height: explored.height,
            runs,
        }
    }
}
impl TryFrom<ExploredRepr> for Explored {
    type Error = String;
    fn try_from(repr: ExploredRepr) -> Result<Self, String> {
        if repr.width < 0
            || repr.height < 0
            || repr.width > MAX_DIMENSION
            || repr.height > MAX_DIMENSION
        {
            return Err("explored layer has an invalid size".into());
        }
        let mut explored = Self::new(repr.width, repr.height);
        let total = (repr.width as usize) * (repr.height as usize);
        let covered: u64 = repr.runs.iter().map(|r| u64::from(*r)).sum();
        if covered != total as u64 {
            return Err(format!(
                "explored runs cover {covered} tiles, expected {total}"
            ));
        }
        let mut cursor = 0usize;
        for (i, run) in repr.runs.iter().enumerate() {
            let end = cursor + *run as usize;
            if i % 2 == 1 {
                for bit in cursor..end {
                    explored.bits[bit / 64] |= 1 << (bit % 64);
                }
            }
            cursor = end;
        }
        Ok(explored)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn neighbours_are_the_eight_chebyshev_directions() {
        let map = Map::filled(3, 3, Terrain::Grass);
        assert_eq!(map.neighbors(Coord::new(1, 1)).len(), 8);
        assert_eq!(map.neighbors(Coord::new(0, 0)).len(), 3);
        assert!(
            Coord::NEIGHBORS
                .iter()
                .all(|&(x, y)| Coord::new(0, 0).distance(Coord::new(x, y)) == 1)
        );
        assert_eq!(Coord::new(0, 0).distance_squared(Coord::new(3, 4)), 25);
    }

    #[test]
    fn screen_roundtrip_and_parity() {
        for x in -10..10 {
            for y in -10..10 {
                let p = Coord::new(x, y);
                let (sx, sy) = p.screen();
                assert_eq!(Coord::from_screen(sx, sy), p);
                let (cx, cy) = p.civ3();
                assert_eq!((cx + cy) % 2, 0);
            }
        }
    }
    /// A 4x4 sea with `vertices` (the four tiles of the dual cell at corner (1, 1)) replaced.
    fn cell_map(vertices: [Terrain; 4]) -> Map {
        let mut map = Map::filled(4, 4, Terrain::Ocean);
        for (&(x, y), terrain) in [(1, 1), (2, 1), (2, 2), (1, 2)].iter().zip(vertices) {
            map.get_mut(Coord::new(x, y)).unwrap().terrain = terrain;
        }
        map
    }
    #[test]
    fn ground_cells_are_unique_per_vertex_mix() {
        let lands = [
            Terrain::Grass,
            Terrain::Plains,
            Terrain::Desert,
            Terrain::Tundra,
        ];
        let mut seen = std::collections::BTreeSet::new();
        for &n in &lands {
            for &e in &lands {
                for &s in &lands {
                    for &w in &lands {
                        let cell = cell_map([n, e, s, w])
                            .ground_cell(Coord::new(1, 1))
                            .unwrap();
                        assert!(cell < 256);
                        seen.insert(cell);
                    }
                }
            }
        }
        assert_eq!(seen.len(), 256);
        // a pure cell sits at 85 * digit, where the sheet keeps its tonal variants
        for (digit, &land) in lands.iter().enumerate() {
            let map = cell_map([land; 4]);
            assert_eq!(map.ground_cell(Coord::new(1, 1)), Some(85 * digit));
            assert_eq!(map.water_cell(Coord::new(1, 1)), None);
        }
        assert_eq!(
            cell_map([Terrain::Sea; 4]).ground_cell(Coord::new(1, 1)),
            None
        );
    }
    #[test]
    fn water_beside_land_is_always_coast_and_open_water_keeps_its_depth() {
        let all = [Terrain::Grass, Terrain::Coast, Terrain::Sea, Terrain::Ocean];
        for &n in &all {
            for &e in &all {
                for &s in &all {
                    for &w in &all {
                        let map = cell_map([n, e, s, w]);
                        let corner = Coord::new(1, 1);
                        let land = [n, e, s, w].iter().any(|t| t.is_land());
                        let Some(cell) = map.water_cell(corner) else {
                            assert!([n, e, s, w].iter().all(|t| t.is_land()));
                            continue;
                        };
                        assert!(cell < 256);
                        let digits = [cell % 4, cell / 16 % 4, cell / 64, cell / 4 % 4];
                        if land {
                            assert!(
                                digits.iter().all(|&d| d <= 1),
                                "{digits:?} for {n:?} {e:?} {s:?} {w:?}"
                            );
                        }
                        // water vertices of a pure-water cell keep their own depth
                        if !land && [n, e, s, w].iter().all(|&t| t == n) {
                            assert_eq!(digits, [n.water_digit(); 4]);
                        }
                    }
                }
            }
        }
        // shore water borrows the land beside it for the ground below the surf
        let map = cell_map([
            Terrain::Desert,
            Terrain::Ocean,
            Terrain::Ocean,
            Terrain::Ocean,
        ]);
        assert_eq!(map.ground_cell(Coord::new(1, 1)), Some(85 * 2));
    }
    #[test]
    fn every_glyph_parses_and_layers_write_back_canonically() {
        for terrain in Terrain::ALL {
            let (parsed, relief, cover) = Tile::from_glyph(terrain.glyph()).unwrap();
            assert_eq!(
                (parsed, relief, cover),
                (terrain, Relief::Flat, Cover::Bare)
            );
        }
        // the old combined glyphs
        for (glyph, expected) in [
            ('.', (Terrain::Grass, Relief::Flat, Cover::Bare)),
            (',', (Terrain::Desert, Relief::Flat, Cover::Bare)),
            ('f', (Terrain::Grass, Relief::Flat, Cover::Forest)),
            ('m', (Terrain::Grass, Relief::Mountains, Cover::Bare)),
            ('n', (Terrain::Grass, Relief::Mountains, Cover::Forest)),
            ('F', (Terrain::Desert, Relief::Flat, Cover::Forest)),
            ('M', (Terrain::Desert, Relief::Mountains, Cover::Bare)),
            ('N', (Terrain::Desert, Relief::Mountains, Cover::Forest)),
        ] {
            assert_eq!(Tile::from_glyph(glyph), Some(expected), "{glyph}");
        }
        assert!(Tile::from_glyph('?').is_none());
        let map = Map::from_rows(&["~f.,".into(), "mnFN".into()]).unwrap();
        assert_eq!(map.rows(), ["~ggd", "ggdd"]);
        let json = serde_json::to_string(&map).unwrap();
        assert!(
            json.contains("\"relief\":[\"..^^\"") || json.contains("\"relief\":[\"...."),
            "{json}"
        );
        assert_eq!(serde_json::from_str::<Map>(&json).unwrap(), map);
    }
    #[test]
    fn layers_cost_and_defend_like_civ3() {
        let mut tile = Tile::new(Coord::new(0, 0), Terrain::Grass);
        assert_eq!((tile.move_cost(), tile.defense_percent()), (1, 10));
        tile.cover = Cover::Forest;
        assert_eq!((tile.move_cost(), tile.defense_percent()), (2, 25));
        tile.cover = Cover::Marsh;
        assert_eq!((tile.move_cost(), tile.defense_percent()), (2, 20));
        tile.cover = Cover::Bare;
        tile.relief = Relief::Hills;
        assert_eq!((tile.move_cost(), tile.defense_percent()), (2, 50));
        tile.cover = Cover::Forest;
        assert_eq!((tile.move_cost(), tile.defense_percent()), (2, 50));
        tile.relief = Relief::Mountains;
        assert_eq!((tile.move_cost(), tile.defense_percent()), (3, 100));
        tile.set_terrain(Terrain::Sea);
        assert!(tile.layers_valid() && tile.relief == Relief::Flat && tile.cover == Cover::Bare);
    }
    #[test]
    fn map_layers_roundtrip_through_json() {
        let mut map = Map::from_rows(&["~~gg".into(), "gdpt".into(), "~gg~".into()]).unwrap();
        map.tiles[5].relief = Relief::Hills;
        map.tiles[6].cover = Cover::Jungle;
        map.tiles[7].relief = Relief::Mountains;
        map.tiles[9].cover = Cover::Forest;
        map.tiles[9].relief = Relief::Hills;
        map.tiles[2].cover = Cover::Marsh;
        map.tiles[5].river = Tile::RIVER_E | Tile::RIVER_S;
        map.tiles[6].river = Tile::RIVER_S;
        map.tiles[3].region = 7;
        map.tiles[4].region = 7;
        map.tiles[4].owner = 2;
        map.tiles[11].owner = 300;
        map.tiles[2].claim = 5;
        map.tiles[3].claim = 5;
        map.tiles[2].improvements = Tile::ROAD | Tile::RAIL;
        map.tiles[3].improvements = Tile::MINE;
        let json = serde_json::to_string(&map).unwrap();
        assert!(json.contains("\"~~gg\""), "{json}");
        assert!(json.contains("\"relief\":[\"...."), "{json}");
        assert!(json.contains("\"cover\":[\"..m.\""), "{json}");
        let back: Map = serde_json::from_str(&json).unwrap();
        assert_eq!(back, map);
        // Layers that are all zero are omitted entirely.
        let plain = serde_json::to_string(&Map::filled(3, 2, Terrain::Ocean)).unwrap();
        assert!(
            ![
                "relief",
                "cover",
                "rivers",
                "regions",
                "owners",
                "claims",
                "improvements"
            ]
            .iter()
            .any(|layer| plain.contains(layer)),
            "{plain}"
        );
    }
    #[test]
    fn rivers_are_shared_by_the_tiles_on_either_side_and_by_four_dual_cells() {
        let mut map = Map::filled(4, 4, Terrain::Grass);
        // the edge between (1, 1) and (2, 1), and the edge between (2, 1) and (2, 2)
        map.get_mut(Coord::new(1, 1)).unwrap().river = Tile::RIVER_E;
        map.get_mut(Coord::new(2, 1)).unwrap().river = Tile::RIVER_S;
        assert_eq!(map.river_mask(Coord::new(1, 1)), 0b0010);
        assert_eq!(map.river_mask(Coord::new(2, 1)), 0b1100);
        assert_eq!(map.river_mask(Coord::new(2, 2)), 0b0001);
        assert!(map.river_between(Coord::new(1, 1), Coord::new(2, 1)));
        assert!(map.river_between(Coord::new(2, 1), Coord::new(1, 1)));
        assert!(map.river_between(Coord::new(2, 2), Coord::new(2, 1)));
        assert!(!map.river_between(Coord::new(1, 1), Coord::new(1, 2)));
        assert!(!map.river_between(Coord::new(1, 1), Coord::new(2, 2)));
        // each edge is a half-branch of the two dual cells at its ends
        assert_eq!(map.river_cell(Coord::new(1, 1)) & 0b0001, 0b0001); // north-east branch
        assert_eq!(map.river_cell(Coord::new(1, 0)), 0b0100); // and its south-west end next door
        // the S edge of (2, 1) is the south-east branch of cell (1, 1) and the north-west one of (2, 1)
        assert_eq!(map.river_cell(Coord::new(2, 1)), 0b1000);
        assert_eq!(map.river_cell(Coord::new(1, 1)), 0b0011);
        assert_eq!(map.river_cell(Coord::new(0, 0)), 0);
    }
    #[test]
    fn malformed_maps_are_rejected() {
        let parse = |json: &str| serde_json::from_str::<Map>(json).is_err();
        assert!(parse(r#"{"width":2,"height":2,"terrain":["~~","~"]}"#));
        assert!(parse(r#"{"width":2,"height":2,"terrain":["~~","~?"]}"#));
        assert!(parse(r#"{"width":3,"height":2,"terrain":["~~","~~"]}"#));
        assert!(parse(
            r#"{"width":2,"height":1,"terrain":["~~"],"regions":[1,1]}"#
        ));
        assert!(parse(
            r#"{"width":2,"height":1,"terrain":["~~"],"regions":[1]}"#
        ));
        assert!(parse(
            r#"{"width":2,"height":1,"terrain":["~~"],"regions":[70000,2]}"#
        ));
        assert!(parse(
            r#"{"width":2,"height":1,"terrain":["~~"],"owners":[1,4000000000]}"#
        ));
        assert!(parse(
            r#"{"width":2,"height":1,"terrain":["~~"],"extra":1}"#
        ));
        // improvements: only land, no rail without road, no mine and farm together
        assert!(parse(
            r#"{"width":2,"height":1,"terrain":["~."],"improvements":[1,2]}"#
        ));
        assert!(parse(
            r#"{"width":2,"height":1,"terrain":[".."],"improvements":[2,2]}"#
        ));
        assert!(parse(
            r#"{"width":2,"height":1,"terrain":[".."],"improvements":[12,2]}"#
        ));
        assert!(parse(
            r#"{"width":2,"height":1,"terrain":[".."],"improvements":[16,2]}"#
        ));
        assert!(!parse(
            r#"{"width":2,"height":1,"terrain":[".."],"improvements":[3,1,4,1]}"#
        ));
        assert!(!parse(
            r#"{"width":2,"height":1,"terrain":["~~"],"regions":[1,2]}"#
        ));
        // relief and cover: right shape, known glyphs, no water relief or sloping marsh
        assert!(parse(
            r#"{"width":2,"height":1,"terrain":["gg"],"relief":["h"]}"#
        ));
        assert!(parse(
            r#"{"width":2,"height":1,"terrain":["gg"],"relief":["hx"]}"#
        ));
        assert!(parse(
            r#"{"width":2,"height":1,"terrain":["g~"],"relief":[".^"]}"#
        ));
        assert!(parse(
            r#"{"width":2,"height":1,"terrain":["gs"],"cover":[".f"]}"#
        ));
        assert!(!parse(
            r#"{"width":2,"height":1,"terrain":["gg"],"relief":["h."],"cover":["j."]}"#
        ));
        assert!(!parse(
            r#"{"width":2,"height":1,"terrain":["gg"],"relief":["^."],"cover":["j."]}"#
        ));
        assert!(parse(
            r#"{"width":2,"height":1,"terrain":["gg"],"relief":["^."],"cover":["m."]}"#
        ));
        assert!(!parse(
            r#"{"width":2,"height":1,"terrain":["gg"],"relief":["h."],"cover":["f."]}"#
        ));
        // a legacy glyph and a layer may not disagree
        assert!(parse(
            r#"{"width":1,"height":1,"terrain":["m"],"relief":["h"]}"#
        ));
        assert!(parse(
            r#"{"width":1,"height":1,"terrain":["f"],"cover":["j"]}"#
        ));
        assert!(!parse(
            r#"{"width":1,"height":1,"terrain":["m"],"relief":["^"],"cover":["f"]}"#
        ));
        // rivers: 0..=3, between two land tiles, never off the edge of the map
        assert!(!parse(
            r#"{"width":2,"height":2,"terrain":["gg","gg"],"rivers":[1,1,0,3]}"#
        ));
        assert!(parse(
            r#"{"width":2,"height":2,"terrain":["gg","gg"],"rivers":[4,1,0,3]}"#
        ));
        assert!(parse(
            r#"{"width":2,"height":2,"terrain":["gg","gg"],"rivers":[0,1,1,1,0,2]}"#
        )); // east edge of the last column
        assert!(parse(
            r#"{"width":2,"height":2,"terrain":["gg","gg"],"rivers":[0,2,2,1,0,1]}"#
        )); // south edge of the last row
        assert!(parse(
            r#"{"width":2,"height":1,"terrain":["g~"],"rivers":[1,1,0,1]}"#
        )); // into the sea
    }
    #[test]
    fn explored_runs_roundtrip_and_reveal_is_chebyshev() {
        let mut fog = Explored::new(12, 9);
        assert!(fog.is_empty());
        fog.reveal(Coord::new(2, 2), 3);
        assert!(fog.contains(Coord::new(5, 5)) && fog.contains(Coord::new(0, 0)));
        assert!(!fog.contains(Coord::new(6, 2)) && !fog.contains(Coord::new(2, 6)));
        assert_eq!(fog.len(), 6 * 6);
        let back: Explored = serde_json::from_str(&serde_json::to_string(&fog).unwrap()).unwrap();
        assert_eq!(back, fog);
        let empty = Explored::new(12, 9);
        assert_eq!(
            serde_json::to_string(&empty).unwrap(),
            r#"{"width":12,"height":9,"runs":[108]}"#
        );
        let mut full = Explored::new(3, 3);
        full.reveal(Coord::new(1, 1), 9);
        let back: Explored = serde_json::from_str(&serde_json::to_string(&full).unwrap()).unwrap();
        assert_eq!(back, full);
        assert!(serde_json::from_str::<Explored>(r#"{"width":2,"height":2,"runs":[3]}"#).is_err());
    }
    #[test]
    fn a_lattice_is_an_upright_rectangle_inside_the_square() {
        let lattice = Lattice {
            columns: 4,
            rows: 6,
        };
        assert_eq!(lattice.side(), 7);
        let map = Map::embedded(lattice, Terrain::Grass);
        assert_eq!((map.width, map.height), (7, 7));
        assert_eq!(map.tile_count(), 24);
        assert_eq!(map.positions().count(), 24);
        // Every native cell with an even sum is a tile, each at its own square cell, and the
        // cell lands on screen where the native layout puts it: x by column, y by row.
        let mut seen = std::collections::BTreeSet::new();
        for cy in 0..6 {
            for cx in 0..8 {
                if (cx + cy) % 2 != 0 {
                    continue;
                }
                let p = lattice.cell(cx, cy);
                assert!(map.contains(p), "{cx},{cy} -> {p:?}");
                assert_eq!(lattice.native(p), (cx, cy));
                assert!(seen.insert(p));
                let (sx, sy) = p.screen();
                assert_eq!(sx, (cx - lattice.columns) as f32 * 64.0);
                assert_eq!(sy, -(cy + lattice.columns) as f32 * 32.0);
            }
        }
        // The rest of the square is void: no tile, no neighbour of a tile.
        assert_eq!(seen.len(), 24);
        assert_eq!(
            map.positions().collect::<std::collections::BTreeSet<_>>(),
            seen
        );
        let corner = lattice.cell(0, 0);
        assert!(map.neighbors(corner).len() < 8);
        assert!(
            map.neighbors(corner)
                .iter()
                .all(|&n| lattice.contains(n) && corner.distance(n) == 1)
        );
        let void = Coord::new(0, 0);
        assert!(!map.contains(void) && map.get(void).is_none());
        // North is up and east is right.
        let (nw_x, nw_y) = lattice.cell(0, 0).screen();
        let (ne_x, ne_y) = lattice.cell(6, 0).screen();
        let (sw_x, sw_y) = lattice.cell(0, 4).screen();
        assert!(ne_x > nw_x && ne_y == nw_y && sw_x == nw_x && sw_y < nw_y);
    }
    #[test]
    fn screen_bounds_hug_the_tile_centres() {
        let bounds_of = |map: &Map| {
            map.positions().map(Coord::screen).fold(
                (f32::MAX, f32::MAX, f32::MIN, f32::MIN),
                |(x0, y0, x1, y1), (x, y)| (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
            )
        };
        let upright = Map::embedded(
            Lattice {
                columns: 5,
                rows: 8,
            },
            Terrain::Ocean,
        );
        assert_eq!(upright.screen_bounds(), bounds_of(&upright));
        let square = Map::filled(7, 4, Terrain::Ocean);
        assert_eq!(square.screen_bounds(), bounds_of(&square));
    }
    #[test]
    fn a_lattice_roundtrips_and_its_void_must_be_empty() {
        let lattice = Lattice {
            columns: 3,
            rows: 4,
        };
        let mut map = Map::embedded(lattice, Terrain::Ocean);
        let city = lattice.cell(2, 2);
        map.get_mut(city).unwrap().set_terrain(Terrain::Plains);
        let json = serde_json::to_string(&map).unwrap();
        assert!(
            json.contains(r#""lattice":{"columns":3,"rows":4}"#),
            "{json}"
        );
        let back: Map = serde_json::from_str(&json).unwrap();
        assert_eq!(back, map);
        // Land in the void, an odd row count, and a square of the wrong size are all rejected.
        let mut dirty = map.clone();
        dirty.tiles[0].set_terrain(Terrain::Grass);
        assert!(serde_json::from_str::<Map>(&serde_json::to_string(&dirty).unwrap()).is_err());
        assert!(serde_json::from_str::<Map>(&json.replace("\"rows\":4", "\"rows\":5")).is_err());
        assert!(
            serde_json::from_str::<Map>(&json.replace("\"columns\":3", "\"columns\":4")).is_err()
        );
    }
    #[test]
    fn filling_the_chart_marks_exactly_the_map() {
        // 12 x 9 = 108 tiles is not a multiple of 64, so the last word is partly unused.
        let mut fog = Explored::new(12, 9);
        fog.fill();
        assert_eq!(fog.len(), 108);
        assert!(fog.contains(Coord::new(11, 8)) && !fog.contains(Coord::new(12, 8)));
        let mut by_hand = Explored::new(12, 9);
        by_hand.reveal(Coord::new(0, 0), 100);
        assert_eq!(fog, by_hand);
        assert_eq!(
            serde_json::to_string(&fog).unwrap(),
            r#"{"width":12,"height":9,"runs":[0,108]}"#
        );
    }
}
