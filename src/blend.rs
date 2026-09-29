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
//! A vertex touches four tiles; every tile sharing it must agree on its
//! type for edges to line up, so the type is chosen by a priority over
//! those four tiles (water beats land, shallow water beats deep).

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

/// Land vertex priority, strongest first.
const LAND_PRI: [Terr; 5] = [Coast, Tundra, Desert, Plains, Grass];

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

/// The four tiles around each vertex, in N, E, S, W order (offsets from
/// the tile; the tile itself is always included).
pub(crate) const VERTEX_TILES: [[(i32, i32); 4]; 4] = [
    [(0, 0), (-1, 0), (0, -1), (-1, -1)], // N
    [(0, 0), (0, -1), (1, 0), (1, -1)],   // E
    [(0, 0), (1, 0), (0, 1), (1, 1)],     // S
    [(0, 0), (-1, 0), (0, 1), (-1, 1)],   // W
];

fn vertex_types(map: &GameMap, x: i32, y: i32, water_tile: bool) -> [Terr; 4] {
    let mut out = [Grass; 4];
    for (i, offs) in VERTEX_TILES.iter().enumerate() {
        let mut best: Option<Terr> = None;
        for (dx, dy) in offs {
            let Some(t) = map.get(x + dx, y + dy) else {
                continue;
            };
            let mut ty = terr_of(t);
            let better = if water_tile {
                // Land counts as coast; shallower water wins.
                if !is_water(ty) {
                    ty = Coast;
                }
                best.is_none_or(|b| depth(ty) < depth(b))
            } else {
                let rank = |t: Terr| {
                    let t = if is_water(t) { Coast } else { t };
                    LAND_PRI.iter().position(|p| *p == t).unwrap()
                };
                best.is_none_or(|b| rank(ty) < rank(b))
            };
            if better {
                best = Some(ty);
            }
        }
        out[i] = best.unwrap_or(Grass);
    }
    out
}

fn depth(t: Terr) -> u8 {
    match t {
        Coast => 0,
        Sea => 1,
        _ => 2,
    }
}

/// Sheet stem, column and row for a tile, or None when the tile keeps its
/// unblended sprite (ice).
pub fn cell_for(map: &GameMap, x: i32, y: i32) -> Option<(&'static str, u32, u32)> {
    let t = map.get(x, y)?;
    if t.base == Base::Ice {
        return None;
    }
    let own = terr_of(t);
    let water = is_water(own);
    let mut v = vertex_types(map, x, y, water);
    let (stem, triple) = if water {
        WATER_SHEET
    } else {
        // A land tile that lost every vertex to stronger neighbors keeps a
        // patch of its own type at the south vertex, so lone tiles never
        // vanish (a full diamond reads as a hard-edged tile).
        if !v.iter().any(|t| *t == own) {
            v[2] = own;
        }
        let weight = |ty: Terr| if is_water(ty) { 3 } else { 1 };
        let mut needed: Vec<Terr> = v.to_vec();
        needed.push(own);
        needed.dedup();
        let mut best = LAND_SHEETS[0];
        let mut best_score = -1;
        for sheet in LAND_SHEETS {
            let triple = sheet.1;
            if !triple.contains(&own) {
                continue;
            }
            let mut seen: Vec<Terr> = vec![];
            let mut score = 0;
            for ty in &needed {
                if triple.contains(ty) && !seen.contains(ty) {
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
    Some((stem, 3 * w + n, 3 * s + e))
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
    let sheet = if t.cover == Cover::Jungle { "grassland forests" } else { sheet };
    let y0 = (row as f32 * COVER_ROW_H).round() + 2.0;
    let y1 = ((row + 1) as f32 * COVER_ROW_H).round() - 2.0;
    Some(CoverSprite {
        path: format!("gen/terrain/sheets/{sheet}.png"),
        rect: Rect::new(col as f32 * 128.0 + 2.0, y0, (col + 1) as f32 * 128.0 - 2.0, y1),
        anchor_px: Vec2::new(62.0, 48.0),
    })
}

pub fn sheet_path(stem: &str) -> String {
    format!("gen/terrain/sheets/{stem}.png")
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
        let (_, col, row) = cell_for(&map, 10, 10).unwrap();
        assert_eq!((col, row), (4, 4));
        // All-ocean: the wCSO all-ocean cell is digit 2 everywhere -> (8,8).
        let map = flat_map(|_, _| Base::Ocean);
        assert_eq!(cell_for(&map, 10, 10), Some(("wCSO", 8, 8)));
    }

    #[test]
    fn coast_bites_the_shared_vertex_only() {
        // A single water tile north-west of (10,10) touches only the N vertex.
        let map = flat_map(|x, y| {
            if (x, y) == (9, 9) {
                Base::Coast
            } else {
                Base::Grassland
            }
        });
        let (stem, col, row) = cell_for(&map, 10, 10).unwrap();
        assert!(stem == "xtgc" || stem == "xpgc" || stem == "xdgc" || stem == "xggc");
        // N digit = coast (2), the other three vertices grass (1).
        assert_eq!(col, 3 * 1 + 2);
        assert_eq!(row, 3 * 1 + 1);
    }

    #[test]
    fn tundra_grass_uses_xtgc_and_ice_is_unblended() {
        let map = flat_map(|x, _| if x < 10 { Base::Tundra } else { Base::Grassland });
        let (stem, _, _) = cell_for(&map, 10, 10).unwrap();
        assert_eq!(stem, "xtgc");
        let map = flat_map(|_, _| Base::Ice);
        assert_eq!(cell_for(&map, 10, 10), None);
    }

    #[test]
    fn lone_land_tile_survives_surrounding_water() {
        let map = flat_map(|x, y| if (x, y) == (10, 10) { Base::Plains } else { Base::Ocean });
        let (_, col, row) = cell_for(&map, 10, 10).unwrap();
        // Not the all-coast cell: the tile keeps its own type at S.
        assert_ne!((col, row), (8, 8));
        assert_ne!(row / 3, 2); // S digit is plains, not coast
    }

    #[test]
    fn cell_rect_geometry() {
        let r = cell_rect(2, 3);
        assert_eq!((r.min.x, r.min.y, r.max.x, r.max.y), (256.0, 192.0, 384.0, 256.0));
    }
}
