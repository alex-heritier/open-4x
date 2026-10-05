//! Renders a small map from a set of sheets exactly the way open-4x does
//! (`src/blend.rs::corner_cell`, `src/render.rs::spawn_terrain`), so a
//! terrain pack can be judged, and regression-tested, without the game.

use anyhow::{Context, Result, bail};
use image::{Rgba, RgbaImage};
use std::collections::HashMap;
use std::path::Path;

use crate::geom::{CH, CW, Kind};

use Kind::*;

/// A map of the terrain kinds a sheet can show, row-major, `y` down the
/// rows. Uses the engine's tile placement: `(x+1, y)` is down-right of
/// `(x, y)` on screen and `(x, y+1)` down-left.
pub struct TestMap {
    pub w: i32,
    pub h: i32,
    pub tiles: Vec<Kind>,
}

/// Every pairing the sheets have: tundra, grass, plains and desert
/// meeting each other and the coast, coast against sea and ocean.
pub const DEFAULT_MAP: &str = "\
oooooooooooooooooo
ooosssssssssssoooo
oosccccccccccsssoo
oscgggggppdddccsoo
oscggggpppdddccsoo
osctggppppddddcsoo
oscttgggpdddpccsoo
osccttgggpppcccsoo
oscctttgcgppccssoo
oosccttccggpcsssoo
ooossccsccccssoooo
oooossssssssoooooo
oooooooooooooooooo
";

impl TestMap {
    pub fn parse(text: &str) -> Result<Self> {
        let rows: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
        if rows.is_empty() {
            bail!("empty map");
        }
        let w = rows[0].len();
        let mut tiles = Vec::new();
        for r in &rows {
            if r.len() != w {
                bail!("map rows must all be {w} wide");
            }
            for ch in r.chars() {
                tiles.push(match ch {
                    'g' => Grassland,
                    'p' => Plains,
                    'd' => Desert,
                    't' => Tundra,
                    'c' => Coast,
                    's' => Sea,
                    'o' => Ocean,
                    _ => bail!("unknown map letter {ch:?} (use g p d t c s o)"),
                });
            }
        }
        let mut m = Self { w: w as i32, h: rows.len() as i32, tiles };
        m.coast_shores();
        Ok(m)
    }

    pub fn get(&self, x: i32, y: i32) -> Kind {
        let x = x.clamp(0, self.w - 1);
        let y = y.clamp(0, self.h - 1);
        self.tiles[(y * self.w + x) as usize]
    }

    /// Like `map::coast_shores` in the game: deep water touching land
    /// (diagonals included) becomes coast, since land sheets only know
    /// coast.
    fn coast_shores(&mut self) {
        let snapshot = self.tiles.clone();
        let at = |x: i32, y: i32| snapshot[(y.clamp(0, self.h - 1) * self.w + x.clamp(0, self.w - 1)) as usize];
        for y in 0..self.h {
            for x in 0..self.w {
                let i = (y * self.w + x) as usize;
                if matches!(snapshot[i], Sea | Ocean)
                    && (-1..=1).any(|dy| (-1..=1).any(|dx| !at(x + dx, y + dy).is_water()))
                {
                    self.tiles[i] = Coast;
                }
            }
        }
    }
}

const LAND_SHEETS: [(&str, [Kind; 3]); 6] = [
    ("xggc", [Grassland, Grassland, Coast]),
    ("xtgc", [Tundra, Grassland, Coast]),
    ("xpgc", [Plains, Grassland, Coast]),
    ("xdgc", [Desert, Grassland, Coast]),
    ("xdpc", [Desert, Plains, Coast]),
    ("xdgp", [Desert, Grassland, Plains]),
];
const WATER_SHEET: (&str, [Kind; 3]) = ("wCSO", [Coast, Sea, Ocean]);

fn substitutes(v: Kind) -> &'static [Kind] {
    match v {
        Tundra => &[Grassland, Plains, Desert, Coast],
        Grassland => &[Plains, Desert, Tundra, Coast],
        Plains => &[Grassland, Desert, Coast, Tundra],
        Desert => &[Plains, Grassland, Coast, Tundra],
        _ => &[Coast],
    }
}

/// Port of `blend::corner_cell`: sheet stem, column and row for the cell
/// whose N, E, S, W vertices are `v`.
pub fn choose_cell(v: [Kind; 4]) -> (&'static str, u32, u32) {
    let (stem, triple) = if v.iter().all(|t| t.is_water()) {
        WATER_SHEET
    } else {
        let needed: Vec<Kind> = v.iter().map(|t| if t.is_water() { Coast } else { *t }).collect();
        let weight = |ty: Kind| if ty == Coast { 3 } else { 1 };
        let mut best = LAND_SHEETS[0];
        let mut best_score = -1;
        for sheet in LAND_SHEETS {
            let mut seen: Vec<Kind> = vec![];
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
    let digit = |ty: Kind| -> u32 {
        let ty = if stem != WATER_SHEET.0 && ty.is_water() { Coast } else { ty };
        let pick = if triple.contains(&ty) {
            ty
        } else {
            *substitutes(ty).iter().find(|s| triple.contains(s)).unwrap_or(&triple[0])
        };
        triple.iter().rposition(|t| *t == pick).unwrap() as u32
    };
    let (n, e, s, w) = (digit(v[0]), digit(v[1]), digit(v[2]), digit(v[3]));
    (stem, 3 * w + n, 3 * s + e)
}

pub struct Render {
    pub image: RgbaImage,
    /// Mean colour step across cell boundaries over the mean step between
    /// neighbouring pixels inside cells. ~1.0 means seams are invisible.
    pub seam_ratio: f32,
}

pub fn render(map: &TestMap, sheets_dir: &Path) -> Result<Render> {
    let mut sheets: HashMap<&str, RgbaImage> = HashMap::new();
    for stem in LAND_SHEETS.iter().map(|s| s.0).chain([WATER_SHEET.0]) {
        let path = sheets_dir.join(format!("{stem}.png"));
        sheets.insert(stem, image::open(&path).with_context(|| format!("opening {}", path.display()))?.to_rgba8());
    }
    // Cell (x, y): top-left at (64(x - y) - 64, 32(x + y)) in screen pixels
    // with y down; rows -1..h cover the whole map.
    let cells: Vec<(i32, i32)> = (-1..map.h).flat_map(|y| (0..map.w).map(move |x| (x, y))).collect();
    let min_x = cells.iter().map(|(x, y)| 64 * (x - y) - 64).min().unwrap();
    let max_x = cells.iter().map(|(x, y)| 64 * (x - y) + 64).max().unwrap();
    let min_y = cells.iter().map(|(x, y)| 32 * (x + y)).min().unwrap();
    let max_y = cells.iter().map(|(x, y)| 32 * (x + y) + 64).max().unwrap();
    let (iw, ih) = ((max_x - min_x) as u32, (max_y - min_y) as u32);
    let mut image = RgbaImage::from_pixel(iw, ih, Rgba([20, 20, 28, 255]));
    let mut owner = vec![u32::MAX; (iw * ih) as usize];
    for (i, &(x, y)) in cells.iter().enumerate() {
        let v = [map.get(x, y), map.get(x + 1, y), map.get(x + 1, y + 1), map.get(x, y + 1)];
        let (stem, col, row) = choose_cell(v);
        let sheet = &sheets[stem];
        let (ox, oy) = (64 * (x - y) - 64 - min_x, 32 * (x + y) - min_y);
        for py in 0..CH as i32 {
            for px in 0..CW as i32 {
                let c = sheet.get_pixel(col * CW as u32 + px as u32, row * CH as u32 + py as u32);
                if c[3] == 0 {
                    continue;
                }
                let (sx, sy) = ((ox + px) as u32, (oy + py) as u32);
                image.put_pixel(sx, sy, *c);
                owner[(sy * iw + sx) as usize] = i as u32;
            }
        }
    }
    // Seam metric over horizontal neighbours.
    let (mut seam, mut ns, mut inner, mut ni) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for y in 0..ih {
        for x in 0..iw - 1 {
            let (a, b) = (owner[(y * iw + x) as usize], owner[(y * iw + x + 1) as usize]);
            if a == u32::MAX || b == u32::MAX {
                continue;
            }
            let (p, q) = (image.get_pixel(x, y), image.get_pixel(x + 1, y));
            let d: f64 = (0..3).map(|c| (p[c] as f64 - q[c] as f64).abs()).sum();
            if a == b {
                inner += d;
                ni += 1.0;
            } else {
                seam += d;
                ns += 1.0;
            }
        }
    }
    let seam_ratio = ((seam / ns.max(1.0)) / (inner / ni.max(1.0)).max(1e-6)) as f32;
    Ok(Render { image, seam_ratio })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choose_cell_matches_the_engine() {
        // Same cases as src/blend.rs tests.
        assert_eq!(choose_cell([Grassland; 4]), ("xggc", 4, 4));
        assert_eq!(choose_cell([Ocean; 4]), ("wCSO", 8, 8));
        assert_eq!(choose_cell([Grassland, Coast, Grassland, Grassland]), ("xggc", 4, 5));
        assert_eq!(choose_cell([Tundra; 4]), ("xtgc", 0, 0));
    }

    #[test]
    fn default_map_parses_and_shores_are_coast() {
        let m = TestMap::parse(DEFAULT_MAP).unwrap();
        for y in 0..m.h {
            for x in 0..m.w {
                if matches!(m.get(x, y), Sea | Ocean) {
                    for (dx, dy) in [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
                        assert!(m.get(x + dx, y + dy).is_water());
                    }
                }
            }
        }
    }
}
