//! Terrain materials: a large top-down texture turned into tile art.
//!
//! A material is viewed through the isometric projection: lattice
//! coordinates `(a, b)` (see `geom`) are ground-plane coordinates in tile
//! units, so one map tile is a unit square of the material and appears on
//! screen as the familiar 128x64 diamond.
//!
//! Every cell of every sheet draws a terrain from the same *periodic*
//! tile: a texture with period 1 in `a` and `b`. Cells sit on the map at
//! integer lattice offsets, so a terrain is continuous wherever two cells
//! meet, whichever sheets they come from.

use anyhow::{Context, Result};
use image::imageops::FilterType;
use std::path::Path;

use crate::geom::*;

pub struct Material {
    w: usize,
    h: usize,
    /// Resampled so that one tile side is `TILE_PX` pixels.
    px: Vec<[f32; 3]>,
    pub mean: [f32; 3],
    pub std: [f32; 3],
}

/// Material pixels per tile side after resampling: a 128x64 diamond holds
/// 4096 pixels, the area of a 64x64 square.
const TILE_PX: f32 = 64.0;

/// Width of the cross-fade between neighbouring periods, in tile units.
const SEAM_MARGIN: f32 = 0.18;

impl Material {
    /// Load `path`, scaling so the image spans `tiles_across` tiles, and
    /// optionally removing large-scale lighting so every tile looks alike.
    pub fn load(path: &Path, tiles_across: f32, flatten: bool) -> Result<Self> {
        let img = image::open(path).with_context(|| format!("opening material {}", path.display()))?.to_rgb8();
        let side = img.width() as f32 / tiles_across;
        let scale = TILE_PX / side;
        let (w, h) = (
            ((img.width() as f32 * scale).round() as u32).max(64),
            ((img.height() as f32 * scale).round() as u32).max(64),
        );
        let img = image::imageops::resize(&img, w, h, FilterType::Lanczos3);
        let px = img.pixels().map(|p| [p[0] as f32, p[1] as f32, p[2] as f32]).collect();
        let mut m = Self::from_pixels(w as usize, h as usize, px);
        if flatten {
            m.flatten();
        }
        Ok(m)
    }

    pub fn from_pixels(w: usize, h: usize, px: Vec<[f32; 3]>) -> Self {
        let mut m = Self { w, h, px, mean: [0.0; 3], std: [0.0; 3] };
        m.restat();
        m
    }

    fn restat(&mut self) {
        let n = self.px.len() as f32;
        let mut mean = [0.0f32; 3];
        for p in &self.px {
            for c in 0..3 {
                mean[c] += p[c] / n;
            }
        }
        let mut var = [0.0f32; 3];
        for p in &self.px {
            for c in 0..3 {
                var[c] += (p[c] - mean[c]).powi(2) / n;
            }
        }
        self.mean = mean;
        self.std = var.map(f32::sqrt);
    }

    /// Subtract low-frequency colour (vignetting, lighting gradients, big
    /// patches) so any region of the material can stand for the terrain.
    fn flatten(&mut self) {
        let r = (self.w.min(self.h) / 6).max(8) as isize;
        let (w, h) = (self.w as isize, self.h as isize);
        let pass = |src: &[[f32; 3]], horizontal: bool| -> Vec<[f32; 3]> {
            let mut out = vec![[0.0; 3]; src.len()];
            for y in 0..h {
                for x in 0..w {
                    let mut s = [0.0f32; 3];
                    for o in -r..=r {
                        let (xx, yy) = if horizontal { (reflect(x + o, w), y) } else { (x, reflect(y + o, h)) };
                        let q = src[(yy * w + xx) as usize];
                        for c in 0..3 {
                            s[c] += q[c];
                        }
                    }
                    out[(y * w + x) as usize] = s.map(|v| v / (2 * r + 1) as f32);
                }
            }
            out
        };
        let low = pass(&pass(&self.px, true), false);
        let mean = self.mean;
        for (p, l) in self.px.iter_mut().zip(&low) {
            for c in 0..3 {
                p[c] = (p[c] - l[c] + mean[c]).clamp(0.0, 255.0);
            }
        }
        self.restat();
    }

    /// Shift and scale each channel to a target mean and deviation.
    pub fn color_match(&mut self, mean: [f32; 3], std: [f32; 3]) {
        let (m0, s0) = (self.mean, self.std);
        for p in &mut self.px {
            for c in 0..3 {
                p[c] = ((p[c] - m0[c]) / s0[c].max(1.0) * std[c] + mean[c]).clamp(0.0, 255.0);
            }
        }
        self.restat();
    }

    /// Bilinear sample at ground-plane coordinates in tile units, with
    /// mirrored addressing past the image edge.
    pub fn sample(&self, a: f32, b: f32) -> [f32; 3] {
        let (fx, fy) = (a * TILE_PX - 0.5, b * TILE_PX - 0.5);
        let (x0, y0) = (fx.floor() as isize, fy.floor() as isize);
        let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
        let at = |x: isize, y: isize| self.px[reflect(y, self.h as isize) as usize * self.w + reflect(x, self.w as isize) as usize];
        let (p00, p10, p01, p11) = (at(x0, y0), at(x0 + 1, y0), at(x0, y0 + 1), at(x0 + 1, y0 + 1));
        std::array::from_fn(|c| {
            let top = p00[c] + (p10[c] - p00[c]) * tx;
            let bot = p01[c] + (p11[c] - p01[c]) * tx;
            top + (bot - top) * ty
        })
    }

    /// Size of the material in tile units.
    pub fn tiles(&self) -> (f32, f32) {
        (self.w as f32 / TILE_PX, self.h as f32 / TILE_PX)
    }

    /// A lattice-periodic tile (one cell's worth of pixels, `NPX` long)
    /// cut from the material around ground position `origin` (tile
    /// units). Each pixel mixes the material at its translates by whole
    /// lattice steps, windowed around the cell so the cell is mostly one
    /// clean region and only its rim cross-fades with the neighbouring
    /// periods. The mix preserves variance (Heitz & Neyret 2018), so the
    /// rim keeps the material's contrast instead of turning to mush.
    pub fn periodic_tile(&self, origin: (f32, f32)) -> Vec<[f32; 3]> {
        let win = |d: f32| 1.0 - smoothstep(0.5 - SEAM_MARGIN, 0.5 + SEAM_MARGIN, d.abs());
        let mut out = Vec::with_capacity(NPX);
        for y in 0..CH {
            for x in 0..CW {
                let (a, b) = lattice(x, y);
                // Fundamental domain: a in [0.5, 1.5), b in [-0.5, 0.5).
                let a = a - (a - 0.5).floor();
                let b = b - (b + 0.5).floor();
                let mut parts: Vec<(f32, [f32; 3])> = Vec::with_capacity(4);
                for i in -1..=1 {
                    for j in -1..=1 {
                        let (aa, bb) = (a + i as f32, b + j as f32);
                        let w = win(aa - 1.0) * win(bb);
                        if w > 1e-4 {
                            // aa spans [-0.5, 2.5], bb [-1.5, 1.5]: shift
                            // both into [0, 3] past the origin.
                            parts.push((w, self.sample(origin.0 + aa + 0.5, origin.1 + bb + 1.5)));
                        }
                    }
                }
                let wsum: f32 = parts.iter().map(|p| p.0).sum();
                let norm = parts.iter().map(|p| (p.0 / wsum).powi(2)).sum::<f32>().sqrt();
                let c: [f32; 3] = std::array::from_fn(|c| {
                    let dev: f32 = parts.iter().map(|(w, s)| w / wsum * (s[c] - self.mean[c])).sum();
                    (self.mean[c] + dev / norm).clamp(0.0, 255.0)
                });
                out.push(c);
            }
        }
        out
    }

    /// Up to `n` well-separated origins for periodic tiles, inside the
    /// material so the three tile spans needed fit without mirroring.
    pub fn origins(&self, n: usize) -> Vec<(f32, f32)> {
        let (tw, th) = self.tiles();
        let span = |t: f32| (t - 3.0).max(0.0);
        (0..n)
            .map(|i| {
                let f = [(0.5, 0.5), (0.2, 0.75), (0.8, 0.25), (0.25, 0.2), (0.75, 0.8)][i % 5];
                (span(tw) * f.0, span(th) * f.1)
            })
            .collect()
    }
}

fn reflect(i: isize, n: isize) -> isize {
    let p = 2 * n;
    let m = i.rem_euclid(p);
    if m < n { m } else { p - 1 - m }
}

/// Luminance of each pixel of a periodic tile, normalised to roughly
/// [0, 1] around the tile's own median: the "height" used to decide which
/// terrain pokes through where two meet.
pub fn heights(tile: &[[f32; 3]]) -> Vec<f32> {
    let lum: Vec<f32> = tile.iter().map(|c| 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]).collect();
    let n = lum.len() as f32;
    let mean = lum.iter().sum::<f32>() / n;
    let sd = (lum.iter().map(|l| (l - mean).powi(2)).sum::<f32>() / n).sqrt().max(1.0);
    lum.iter().map(|l| 1.0 / (1.0 + (-(l - mean) / sd * 1.5).exp())).collect()
}

/// Simple multi-octave noise material, for building without AI textures.
pub fn placeholder(color: [f32; 3], seed: u32, size: usize) -> image::RgbImage {
    let n = crate::noise::LatticeNoise::new(seed);
    let mut img = image::RgbImage::new(size as u32, size as u32);
    for y in 0..size {
        for x in 0..size {
            let (a, b) = (x as f32 / size as f32, y as f32 / size as f32);
            let v = n.fbm(a, b, 16, 5) - 0.5;
            let fine = n.value(a, b, 256) - 0.5;
            let k = 1.0 + 0.35 * v + 0.12 * fine;
            img.put_pixel(x as u32, y as u32, image::Rgb(color.map(|c| (c * k).clamp(0.0, 255.0) as u8)));
        }
    }
    img
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noisy_material() -> Material {
        let img = placeholder([120.0, 140.0, 60.0], 9, 256);
        let px = img.pixels().map(|p| [p[0] as f32, p[1] as f32, p[2] as f32]).collect();
        Material::from_pixels(256, 256, px)
    }

    #[test]
    fn periodic_tile_matches_across_cell_edges() {
        // Pixel (x, y) and the pixel one lattice step away in the next cell
        // are the same ground point, so the periodic tile must agree at
        // matching positions across the NE edge: (x, y) near the edge in
        // this cell and (x - 64, y + 32) in the cell above-right.
        let t = noisy_material().periodic_tile((0.5, 0.5));
        let at = |x: usize, y: usize| t[y * CW + x];
        let mut worst: f32 = 0.0;
        for (x, y) in [(100usize, 15usize), (90, 10), (120, 28)] {
            let a = at(x, y);
            let b = at(x - 64, y + 32);
            for c in 0..3 {
                worst = worst.max((a[c] - b[c]).abs());
            }
        }
        assert!(worst < 1e-3, "periodic tile differs by {worst}");
    }

    #[test]
    fn reflect_addresses() {
        assert_eq!(reflect(-1, 4), 0);
        assert_eq!(reflect(4, 4), 3);
        assert_eq!(reflect(9, 4), 1);
    }
}
