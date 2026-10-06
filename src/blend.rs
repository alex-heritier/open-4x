//! Terrain blending from the 9x9 transition sheets (`xtgc`, `wCSO`, ...).
//!
//! `reverse-engineering/blending.md` recovers the addressing
//! (`(sheet*81 + cell)`, 128x64 cells) but leaves cell selection open. Pixel
//! measurement of every sheet closes it: a cell is `row*9 + col` with
//!
//! ```text
//! col = 3*W + N        row = 3*S + E
//! ```
//!
//! where N/E/S/W are the terrain types at the diamond's four *vertices*
//! and each digit indexes the sheet's triple (`xtgc` = tundra, grass,
//! coast; `wCSO` = coast, sea, ocean; ...). Verified by classifying vertex
//! and edge regions of all 81 cells: the E vertex depends only on
//! `row%3`, S on `row/3`, W on `col/3`, N on `col%3`.
//!
//! The cells sit on a grid offset half a tile from the map: a cell is
//! centered on a tile *corner*, and its four vertices are the centers of
//! the four tiles around that corner. Each vertex takes its own tile's
//! terrain, so every tile center shows exactly its own type and the
//! transitions fall between tile centers. A land tile therefore never
//! reads as water: a tile that looks mostly water is always a coast tile,
//! as in Civ3. Each tile's diamond is covered by the four cells that
//! have it as a vertex.

use bevy::prelude::*;

use crate::map::{Base, Cover, GameMap, Relief, Tile};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Terr {
    Tundra,
    Grass,
    Plains,
    Desert,
    Coast,
    Sea,
    Ocean,
}

use Terr::*;

/// Land sheets: file stem and its (digit 0, 1, 2) terrain triple.
const LAND_SHEETS: [(&str, [Terr; 3]); 6] = [
    ("xggc", [Grass, Grass, Coast]),
    ("xtgc", [Tundra, Grass, Coast]),
    ("xpgc", [Plains, Grass, Coast]),
    ("xdgc", [Desert, Grass, Coast]),
    ("xdpc", [Desert, Plains, Coast]),
    ("xdgp", [Desert, Grass, Plains]),
];
const WATER_SHEET: (&str, [Terr; 3]) = ("wCSO", [Coast, Sea, Ocean]);

pub const CELL_W: f32 = 128.0;
pub const CELL_H: f32 = 64.0;

pub fn terr_of(t: &Tile) -> Terr {
    match t.base {
        Base::Grassland => Grass,
        Base::Plains => Plains,
        Base::Desert => Desert,
        Base::Tundra | Base::Ice => Tundra,
        Base::Coast => Coast,
        Base::Sea => Sea,
        Base::Ocean => Ocean,
    }
}

fn is_water(t: Terr) -> bool {
    matches!(t, Coast | Sea | Ocean)
}

/// Nearest stand-in when a sheet lacks a needed terrain.
fn substitutes(v: Terr) -> &'static [Terr] {
    match v {
        Tundra => &[Grass, Plains, Desert, Coast],
        Grass => &[Plains, Desert, Tundra, Coast],
        Plains => &[Grass, Desert, Coast, Tundra],
        Desert => &[Plains, Grass, Coast, Tundra],
        _ => &[Coast],
    }
}

/// The four tiles around each vertex of a tile's diamond, in N, E, S, W
/// order (offsets from the tile; the tile itself is always included).
/// The fog sheet's per-vertex addressing uses these.
pub(crate) const VERTEX_TILES: [[(i32, i32); 4]; 4] = [
    [(0, 0), (-1, 0), (0, -1), (-1, -1)], // N
    [(0, 0), (0, -1), (1, 0), (1, -1)],   // E
    [(0, 0), (1, 0), (0, 1), (1, 1)],     // S
    [(0, 0), (-1, 0), (0, 1), (-1, 1)],   // W
];

/// Tiles on the N, E, S, W vertices of the terrain cell keyed `(x, y)`:
/// the cell centered on tile `(x, y)`'s south corner. `tile_to_world`
/// puts `(x+1, y)` down-right and `(x, y+1)` down-left, so the tile is the
/// cell's top vertex and its three southern neighbors the others.
pub const CORNER_TILES: [(i32, i32); 4] = [(0, 0), (1, 0), (1, 1), (0, 1)];

/// World offset from tile `(x, y)`'s center to the center of cell `(x, y)`.
pub const CORNER_OFFSET: Vec2 = Vec2::new(0.0, -CELL_H / 2.0);

/// Terrain at one cell vertex. Rows past the map's north or south edge
/// repeat the edge row, so the cells there stay whole.
fn vertex_terr(map: &GameMap, x: i32, y: i32) -> Terr {
    let y = y.clamp(0, map.h - 1);
    terr_of(map.get(x, y).expect("row clamped onto the map"))
}

/// Sheet stem, column and row of the terrain cell keyed `(x, y)` (see
/// `CORNER_TILES`). `y` may be -1: that row of cells covers the north
/// half of map row 0.
pub fn corner_cell(map: &GameMap, x: i32, y: i32) -> (&'static str, u32, u32) {
    let v = CORNER_TILES.map(|(dx, dy)| vertex_terr(map, x + dx, y + dy));
    let (stem, triple) = if v.iter().all(|t| is_water(*t)) {
        WATER_SHEET
    } else {
        // Mixed cells draw from a land sheet, whose water digit is coast.
        let needed: Vec<Terr> = v
            .iter()
            .map(|t| if is_water(*t) { Coast } else { *t })
            .collect();
        let weight = |ty: Terr| if ty == Coast { 3 } else { 1 };
        let mut best = LAND_SHEETS[0];
        let mut best_score = -1;
        for sheet in LAND_SHEETS {
            let mut seen: Vec<Terr> = vec![];
            let mut score = 0;
            for ty in &needed {
                if sheet.1.contains(ty) && !seen.contains(ty) {
                    seen.push(*ty);
                    score += weight(*ty);
                }
            }
            if score > best_score {
                best_score = score;
                best = sheet;
            }
        }
        best
    };
    let digit = |ty: Terr| -> u32 {
        let ty = if stem != WATER_SHEET.0 && is_water(ty) {
            Coast
        } else {
            ty
        };
        let pick = if triple.contains(&ty) {
            ty
        } else {
            *substitutes(ty)
                .iter()
                .find(|s| triple.contains(s))
                .unwrap_or(&triple[0])
        };
        // `xggc` lists grass twice; digit 1 matches the other sheets.
        triple.iter().rposition(|t| *t == pick).unwrap() as u32
    };
    let (n, e, s, w) = (digit(v[0]), digit(v[1]), digit(v[2]), digit(v[3]));
    (stem, 3 * w + n, 3 * s + e)
}

/// A tree-cover sprite cut from a `* forests` sheet.
pub struct CoverSprite {
    pub path: String,
    pub rect: Rect,
    /// Pixel inside the cell that sits on the tile center.
    pub anchor_px: Vec2,
}

/// Row pitch of the forest sheets (884 px / 10 rows).
const COVER_ROW_H: f32 = 88.4;

fn cover_hash(x: i32, y: i32) -> u32 {
    let h = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA6B);
    (h ^ (h >> 15)).wrapping_mul(0x2C1B_3C6D) >> 8
}

/// Cover art for flat forest/jungle/pine tiles. The sheets hold "large"
/// canopies (128x88, meant to overlap neighbors so woods read as one mass)
/// and "small" isolated clumps; large ones are used whenever a neighbor
/// carries the same cover.
pub fn cover_sprite(map: &GameMap, x: i32, y: i32) -> Option<CoverSprite> {
    let t = map.get(x, y)?;
    if t.relief != Relief::Flat || t.cover == Cover::Bare {
        return None;
    }
    let joined = map
        .neighbors(x, y)
        .iter()
        .filter(|(nx, ny)| {
            map.get(*nx, *ny)
                .is_some_and(|n| n.relief == Relief::Flat && n.cover == t.cover)
        })
        .count();
    let sheet = match (t.cover, t.base) {
        (Cover::Pine, _) | (_, Base::Tundra | Base::Ice) => "tundra forests",
        (_, Base::Plains) => "plains forests",
        _ => "grassland forests",
    };
    // (first row, rows, columns) of the variant block.
    let (row0, cols) = match (t.cover, joined >= 1) {
        (Cover::Jungle, true) => (0, 4),
        (Cover::Jungle, false) => (2, 6),
        (Cover::Forest, true) => (4, 4),
        (Cover::Forest, false) => (6, 5),
        _ => (8, 6), // pines
    };
    let h = cover_hash(x, y);
    let col = h % cols;
    let row = row0 + (h / 7) % 2;
    // Jungle only exists on the grassland sheet; the tundra sheet has none.
    let sheet = if t.cover == Cover::Jungle {
        "grassland forests"
    } else {
        sheet
    };
    let y0 = (row as f32 * COVER_ROW_H).round() + 2.0;
    let y1 = ((row + 1) as f32 * COVER_ROW_H).round() - 2.0;
    Some(CoverSprite {
        path: format!("cache/terrain/sheets/{sheet}.png"),
        rect: Rect::new(
            col as f32 * 128.0 + 2.0,
            y0,
            (col + 1) as f32 * 128.0 - 2.0,
            y1,
        ),
        anchor_px: Vec2::new(62.0, 48.0),
    })
}

pub fn sheet_path(stem: &str) -> String {
    format!("cache/terrain/sheets/{stem}.png")
}

/// Pixel rect of a cell inside its sheet.
pub fn cell_rect(col: u32, row: u32) -> Rect {
    Rect::new(
        col as f32 * CELL_W,
        row as f32 * CELL_H,
        (col + 1) as f32 * CELL_W,
        (row + 1) as f32 * CELL_H,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_map(f: impl Fn(i32, i32) -> Base) -> GameMap {
        let mut map = GameMap::generate();
        for y in 0..map.h {
            for x in 0..map.w {
                let i = map.idx(x, y);
                map.tiles[i].base = f(x, y);
            }
        }
        map
    }

    #[test]
    fn uniform_terrain_uses_the_pure_cells() {
        // All-grass: every vertex digit is grass (1) -> cell (4,4).
        let map = flat_map(|_, _| Base::Grassland);
        let (_, col, row) = corner_cell(&map, 10, 10);
        assert_eq!((col, row), (4, 4));
        // All-ocean: the wCSO all-ocean cell is digit 2 everywhere -> (8,8).
        let map = flat_map(|_, _| Base::Ocean);
        assert_eq!(corner_cell(&map, 10, 10), ("wCSO", 8, 8));
    }

    #[test]
    fn each_vertex_is_its_own_tile() {
        // Cell (10,10) has tile (10,10) on N, (11,10) on E, (11,11) on S
        // and (10,11) on W. Only the E tile is water.
        let map = flat_map(|x, y| {
            if (x, y) == (11, 10) {
                Base::Coast
            } else {
                Base::Grassland
            }
        });
        let (stem, col, row) = corner_cell(&map, 10, 10);
        assert_eq!(stem, "xggc");
        // N, S, W grass (1), E coast (2).
        assert_eq!((col, row), (3 * 1 + 1, 3 * 1 + 2));
        // A cell with no water vertex stays all land, however close the
        // water is: water never spreads past its own tile's center.
        assert_eq!(corner_cell(&map, 11, 11).1, 4);
        assert_eq!(corner_cell(&map, 11, 11).2, 4);
    }

    #[test]
    fn a_land_tile_keeps_its_center_among_water() {
        // A lone plains tile in the ocean is the vertex of the four cells
        // around it, and each draws plains there; the rest is water.
        let map = flat_map(|x, y| {
            if (x, y) == (10, 10) {
                Base::Plains
            } else {
                Base::Ocean
            }
        });
        let plains_digit = |stem: &str| {
            LAND_SHEETS
                .iter()
                .find(|s| s.0 == stem)
                .unwrap()
                .1
                .iter()
                .position(|t| *t == Plains)
                .unwrap() as u32
        };
        // Tile is N of cell (10,10), E of (9,10), W of (10,9), S of (9,9).
        let (stem, col, _) = corner_cell(&map, 10, 10);
        assert_eq!(col % 3, plains_digit(stem));
        let (stem, _, row) = corner_cell(&map, 9, 10);
        assert_eq!(row % 3, plains_digit(stem));
        let (stem, col, _) = corner_cell(&map, 10, 9);
        assert_eq!(col / 3, plains_digit(stem));
        let (stem, _, row) = corner_cell(&map, 9, 9);
        assert_eq!(row / 3, plains_digit(stem));
        // Its other vertices are the water, drawn as the sheet's coast.
        let (_, col, row) = corner_cell(&map, 10, 10);
        assert_eq!((col / 3, row % 3, row / 3), (2, 2, 2));
    }

    #[test]
    fn tundra_grass_uses_xtgc_and_ice_blends_as_tundra() {
        let map = flat_map(|x, _| {
            if x <= 10 {
                Base::Tundra
            } else {
                Base::Grassland
            }
        });
        assert_eq!(corner_cell(&map, 10, 10).0, "xtgc");
        let map = flat_map(|_, _| Base::Ice);
        assert_eq!(corner_cell(&map, 10, 10), ("xtgc", 0, 0));
    }

    #[test]
    fn edge_rows_repeat_onto_the_missing_vertices() {
        let map = flat_map(|_, y| if y == 0 { Base::Ocean } else { Base::Grassland });
        // Row -1 cells cover row 0's north half: all four vertices clamp
        // to row 0, so they are pure ocean.
        assert_eq!(corner_cell(&map, 5, -1), ("wCSO", 8, 8));
        let last = map.h - 1;
        assert_eq!(corner_cell(&map, 5, last).1, 4);
    }

    #[test]
    fn cell_rect_geometry() {
        let r = cell_rect(2, 3);
        assert_eq!(
            (r.min.x, r.min.y, r.max.x, r.max.y),
            (256.0, 192.0, 384.0, 256.0)
        );
    }
}
