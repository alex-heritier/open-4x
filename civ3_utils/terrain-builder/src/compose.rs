//! Compositing: masks + periodic material tiles -> Civ3-format sheets.

use std::collections::HashMap;

use image::{Rgba, RgbaImage};

use crate::geom::*;
use crate::masks::{CellMask, SheetMasks};
use crate::noise::LatticeNoise;

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Blend {
    /// Land terrains meet by texture height: the brighter detail of one
    /// pokes through the other, giving ragged, painterly boundaries.
    #[default]
    Height,
    /// Plain weighted average of the land textures.
    Smooth,
}

/// What each terrain looks like, as lattice-periodic cell tiles.
pub struct Palette {
    pub tiles: HashMap<Kind, Vec<[f32; 3]>>,
    pub heights: HashMap<Kind, Vec<f32>>,
    pub shore: Vec<[f32; 3]>,
}

pub struct ComposeParams {
    pub blend: Blend,
    /// How much the line where water meets land is darkened, 0..1.
    pub shoreline_darken: f32,
    pub seed: u32,
}

/// Sharpness of the height blend (lower = harder boundaries).
const HEIGHT_TAU: f32 = 0.1;
/// How far texture height may shift a land boundary, in weight units.
const HEIGHT_GAIN: f32 = 0.5;

/// Colour of every pixel of one cell (alpha is applied by the caller).
pub fn compose_cell(spec: &SheetSpec, m: &CellMask, pal: &Palette, cp: &ComposeParams) -> Vec<[f32; 3]> {
    let noise: HashMap<Kind, LatticeNoise> =
        Kind::ALL.iter().map(|k| (*k, LatticeNoise::new(cp.seed.wrapping_mul(97) + k.index() as u32))).collect();
    let has_land = spec.kinds.iter().any(|k| !k.is_water());
    let mut out = vec![[0.0f32; 3]; NPX];
    let mut water_map = vec![0.0f32; NPX];
    for y in 0..CH {
        for x in 0..CW {
            let p = y * CW + x;
            // Outside the diamond the mask is empty: fill with the N
            // vertex terrain so filtered edges never pull in black.
            let mut kw: Vec<(Kind, f32)> = Vec::with_capacity(3);
            for d in 0..3 {
                let w = m.w[d][p];
                if w <= 0.0 {
                    continue;
                }
                match kw.iter_mut().find(|e| e.0 == spec.kinds[d]) {
                    Some(e) => e.1 += w,
                    None => kw.push((spec.kinds[d], w)),
                }
            }
            if kw.is_empty() {
                out[p] = pal.tiles[&spec.kinds[cell_n(spec, m)]][p];
                continue;
            }
            let water: f32 = kw.iter().filter(|e| e.0.is_water()).map(|e| e.1).sum();
            let land: Vec<(Kind, f32)> = kw.iter().copied().filter(|e| !e.0.is_water()).collect();
            let waters: Vec<(Kind, f32)> = kw.iter().copied().filter(|e| e.0.is_water()).collect();

            let land_c = if land.is_empty() {
                [0.0; 3]
            } else {
                let lsum: f32 = land.iter().map(|e| e.1).sum();
                let mix: Vec<(Kind, f32)> = match cp.blend {
                    Blend::Smooth => land.iter().map(|(k, w)| (*k, w / lsum)).collect(),
                    Blend::Height => {
                        let (a, b) = lattice(x, y);
                        let g: Vec<(Kind, f32)> = land
                            .iter()
                            .map(|(k, w)| {
                                let h = 0.6 * pal.heights[k][p] + 0.4 * noise[k].fbm(a, b, 8, 3);
                                (*k, w / lsum + HEIGHT_GAIN * (h - 0.5))
                            })
                            .collect();
                        let mx = g.iter().map(|e| e.1).fold(f32::MIN, f32::max);
                        let e: Vec<(Kind, f32)> = g.iter().map(|(k, v)| (*k, ((v - mx) / HEIGHT_TAU).exp())).collect();
                        let s: f32 = e.iter().map(|x| x.1).sum();
                        e.into_iter().map(|(k, v)| (k, v / s)).collect()
                    }
                };
                let mut c = [0.0f32; 3];
                for (k, w) in mix {
                    let t = pal.tiles[&k][p];
                    for i in 0..3 {
                        c[i] += t[i] * w;
                    }
                }
                let sh = m.shore[p];
                std::array::from_fn(|i| c[i] + (pal.shore[p][i] - c[i]) * sh)
            };
            let water_c = if waters.is_empty() {
                [0.0; 3]
            } else {
                let mut c = [0.0f32; 3];
                for (k, w) in &waters {
                    let t = pal.tiles[k][p];
                    for i in 0..3 {
                        c[i] += t[i] * w / water;
                    }
                }
                c
            };
            let wm = if !has_land || land.is_empty() { 1.0 } else if waters.is_empty() { 0.0 } else { smoothstep(0.3, 0.7, water) };
            water_map[p] = wm;
            out[p] = std::array::from_fn(|i| land_c[i] + (water_c[i] - land_c[i]) * wm);
        }
    }
    // A dark line where water meets land, like Civ3's coast outline.
    if has_land && cp.shoreline_darken > 0.0 {
        let b = box3(&water_map);
        for p in 0..NPX {
            let edge = (4.0 * b[p] * (1.0 - b[p])).clamp(0.0, 1.0);
            if edge > 0.0 && in_diamond(p % CW, p / CW) {
                let k = 1.0 - cp.shoreline_darken * edge;
                out[p] = out[p].map(|c| c * k);
            }
        }
    }
    out
}

/// Digit at the N vertex (fallback colour outside the diamond).
fn cell_n(spec: &SheetSpec, m: &CellMask) -> usize {
    let p = 2 * CW + 64;
    (0..3).max_by(|a, b| m.w[*a][p].total_cmp(&m.w[*b][p])).map(|d| spec.canon(d)).unwrap_or(0)
}

fn box3(f: &[f32]) -> Vec<f32> {
    let mut out = vec![0.0; NPX];
    for y in 0..CH {
        for x in 0..CW {
            let (mut s, mut n) = (0.0, 0.0);
            for dy in -1isize..=1 {
                for dx in -1isize..=1 {
                    let (xx, yy) = (x as isize + dx, y as isize + dy);
                    if xx >= 0 && yy >= 0 && (xx as usize) < CW && (yy as usize) < CH && in_diamond(xx as usize, yy as usize) {
                        s += f[yy as usize * CW + xx as usize];
                        n += 1.0;
                    }
                }
            }
            out[y * CW + x] = if n > 0.0 { s / n } else { 0.0 };
        }
    }
    out
}

/// The whole 1152x576 sheet. Pixels outside each cell's diamond are
/// transparent, as in the converted Civ3 sheets.
pub fn compose_sheet(spec: &SheetSpec, masks: &SheetMasks, pal: &Palette, cp: &ComposeParams) -> RgbaImage {
    let mut img = RgbaImage::new(SHEET_W as u32, SHEET_H as u32);
    for (i, m) in masks.cells.iter().enumerate() {
        let (col, row) = (i % 9, i / 9);
        let px = compose_cell(spec, m, pal, cp);
        for y in 0..CH {
            for x in 0..CW {
                let c = px[y * CW + x];
                let a = if in_diamond(x, y) { 255 } else { 0 };
                img.put_pixel((col * CW + x) as u32, (row * CH + y) as u32, Rgba([c[0] as u8, c[1] as u8, c[2] as u8, a]));
            }
        }
    }
    img
}

/// A single 128x64 terrain tile the way `tools/prep_assets.py` cuts them:
/// alpha zero outside the diamond, with opaque edge pixels bled one pixel
/// outward so GPU filtering at the seams never samples transparency.
pub fn pure_tile(tile: &[[f32; 3]]) -> RgbaImage {
    let mut img = RgbaImage::new(CW as u32, CH as u32);
    let inside = |x: i32, y: i32| (x - 64).abs() as f32 / 64.0 + (y - 32).abs() as f32 / 32.0 <= 1.0;
    for y in 0..CH as i32 {
        for x in 0..CW as i32 {
            let c = tile[y as usize * CW + x as usize];
            let bleed = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dy)| {
                let (nx, ny) = (x + dx, y + dy);
                nx >= 0 && ny >= 0 && nx < CW as i32 && ny < CH as i32 && inside(nx, ny)
            });
            let a = if inside(x, y) || bleed { 255 } else { 0 };
            img.put_pixel(x as u32, y as u32, Rgba([c[0] as u8, c[1] as u8, c[2] as u8, a]));
        }
    }
    img
}
