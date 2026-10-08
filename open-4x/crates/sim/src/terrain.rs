//! Logical square coordinates correspond to Civ3's parity grid (x+y, y-x).
//! Terrain art lives on the dual grid: each diamond's vertices are four tile centers.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
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
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Terrain {
    Grass,
    Sand,
    Water,
}
impl Terrain {
    pub fn digit(self) -> usize {
        match self {
            Self::Grass => 0,
            Self::Sand => 1,
            Self::Water => 2,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Tile {
    pub position: Coord,
    pub terrain: Terrain,
    pub forest: bool,
    pub mountain: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Map {
    pub width: i32,
    pub height: i32,
    pub tiles: Vec<Tile>,
}
impl Map {
    pub fn get(&self, p: Coord) -> Option<&Tile> {
        if p.x < 0 || p.y < 0 || p.x >= self.width || p.y >= self.height {
            None
        } else {
            self.tiles.get((p.y * self.width + p.x) as usize)
        }
    }
    pub fn get_mut(&mut self, p: Coord) -> Option<&mut Tile> {
        if p.x < 0 || p.y < 0 || p.x >= self.width || p.y >= self.height {
            None
        } else {
            self.tiles.get_mut((p.y * self.width + p.x) as usize)
        }
    }
    pub fn neighbors(&self, p: Coord) -> Vec<Coord> {
        [(-1, 0), (0, -1), (1, 0), (0, 1)]
            .into_iter()
            .map(|(x, y)| Coord::new(p.x + x, p.y + y))
            .filter(|p| self.get(*p).is_some())
            .collect()
    }
    pub fn archipelago(width: i32, height: i32, seed: u64) -> Self {
        let mut tiles = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let h = (x as u64).wrapping_mul(7919)
                    ^ (y as u64).wrapping_mul(104729)
                    ^ seed.wrapping_mul(31);
                let mainland = ((x - 12).pow(2) * 2 + (y - 9).pow(2) * 5) < 190;
                let island =
                    (x - 4).pow(2) + (y - 4).pow(2) < 9 || (x - 21).pow(2) + (y - 14).pow(2) < 7;
                let land = mainland || island;
                let terrain = if !land {
                    Terrain::Water
                } else if h % 7 < 2 {
                    Terrain::Sand
                } else {
                    Terrain::Grass
                };
                tiles.push(Tile {
                    position: Coord::new(x, y),
                    terrain,
                    forest: land && h % 5 < 2,
                    mountain: land && h % 13 == 0,
                });
            }
        }
        Self {
            width,
            height,
            tiles,
        }
    }
    /// Sheet column = 3*W + N, row = 3*S + E; 81 cells per triple.
    pub fn blend_cell(&self, corner: Coord) -> usize {
        let sample = |x, y| {
            self.get(Coord::new(x, y))
                .map_or(Terrain::Water, |t| t.terrain)
                .digit()
        };
        let n = sample(corner.x, corner.y);
        let e = sample(corner.x + 1, corner.y);
        let s = sample(corner.x + 1, corner.y + 1);
        let w = sample(corner.x, corner.y + 1);
        (3 * s + e) * 9 + 3 * w + n
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    #[test]
    fn all_blend_cells_are_unique() {
        let mut seen = std::collections::BTreeSet::new();
        let ts = [Terrain::Grass, Terrain::Sand, Terrain::Water];
        for &n in &ts {
            for &e in &ts {
                for &s in &ts {
                    for &w in &ts {
                        let mut m = Map::archipelago(2, 2, 1);
                        m.tiles[0].terrain = n;
                        m.tiles[1].terrain = e;
                        m.tiles[2].terrain = w;
                        m.tiles[3].terrain = s;
                        seen.insert(m.blend_cell(Coord::new(0, 0)));
                    }
                }
            }
        }
        assert_eq!(seen.len(), 81);
    }
}
