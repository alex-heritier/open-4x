//! Transition masks: for every cell of every sheet, how much of each
//! vertex terrain covers each pixel, plus where a beach runs along a
//! coastline.
//!
//! Masks say *where* terrain goes; materials say *what it looks like*.
//! Two sources:
//!
//! * `civ3`: recovered from the original hand-painted sheets by
//!   classifying each pixel against the sheet's pure cells (the 0/0, 4/4
//!   and 8/8 cells are one terrain at all four vertices). The Civ3 cells
//!   are individually painted, so this is colour classification, not a
//!   pixel match.
//! * `procedural`: bilinear vertex weights pushed around by lattice-periodic
//!   noise. Everything is a pointwise function of screen position, so
//!   neighbouring cells agree along their shared edges by construction.
//!
//! Both end with the vertex invariant: within a small radius of each
//! vertex the mask is 100% that vertex's terrain, so a tile center always
//! shows its own type.

use std::collections::HashMap;

use crate::geom::*;
use crate::noise::LatticeNoise;
use crate::pcx::Rgba;

#[derive(Clone)]
pub struct CellMask {
    /// Weight of each sheet digit per pixel; sums to 1 inside the diamond.
    pub w: [Vec<f32>; 3],
    /// Beach on the land side of a coastline, 0..1.
    pub shore: Vec<f32>,
}

impl CellMask {
    fn empty() -> Self {
        Self { w: [vec![0.0; NPX], vec![0.0; NPX], vec![0.0; NPX]], shore: vec![0.0; NPX] }
    }
}

/// The 81 cells of one sheet, indexed `row * 9 + col`.
pub struct SheetMasks {
    pub cells: Vec<CellMask>,
}

/// Diamond-norm radius around each vertex that is forced to its terrain.
const VERTEX_CORE: f32 = 0.15;

fn diamond_pixels() -> impl Iterator<Item = (usize, usize, usize)> {
    (0..CH).flat_map(|y| (0..CW).map(move |x| (x, y, y * CW + x))).filter(|&(x, y, _)| in_diamond(x, y))
}

/// Unique canonical digits at the four vertices.
fn classes(spec: &SheetSpec, digs: &[usize; 4]) -> Vec<usize> {
    let mut c: Vec<usize> = digs.iter().map(|d| spec.canon(*d)).collect();
    c.sort();
    c.dedup();
    c
}

/// The sheet's water digit, if it mixes land and water.
fn water_digit(spec: &SheetSpec) -> Option<usize> {
    if spec.kinds.iter().all(|k| k.is_water()) {
        return None;
    }
    spec.kinds.iter().position(|k| k.is_water())
}

/// Pull everything near a vertex to 100% that vertex's terrain.
fn force_vertices(m: &mut CellMask, spec: &SheetSpec, digs: &[usize; 4]) {
    for (x, y, p) in diamond_pixels() {
        for v in 0..4 {
            let d = vertex_dist(x, y, v);
            if d > VERTEX_CORE {
                continue;
            }
            let a = 1.0 - smoothstep(VERTEX_CORE * 0.6, VERTEX_CORE, d);
            let target = spec.canon(digs[v]);
            for k in 0..3 {
                let t = if k == target { 1.0 } else { 0.0 };
                m.w[k][p] += (t - m.w[k][p]) * a;
            }
            m.shore[p] *= 1.0 - a;
        }
    }
}

// ---------------------------------------------------------------- civ3

fn dist2(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// Box blur of a scalar field over the diamond only (outside pixels carry
/// no weight), radius `r`.
fn blur(field: &[f32], r: usize) -> Vec<f32> {
    let inside: Vec<f32> = (0..NPX).map(|p| if in_diamond(p % CW, p / CW) { 1.0 } else { 0.0 }).collect();
    let pass = |src: &[f32], horizontal: bool| -> Vec<f32> {
        let mut out = vec![0.0; NPX];
        for y in 0..CH {
            for x in 0..CW {
                let mut s = 0.0;
                for o in -(r as isize)..=(r as isize) {
                    let (xx, yy) = if horizontal { (x as isize + o, y as isize) } else { (x as isize, y as isize + o) };
                    if xx >= 0 && yy >= 0 && (xx as usize) < CW && (yy as usize) < CH {
                        s += src[yy as usize * CW + xx as usize];
                    }
                }
                out[y * CW + x] = s;
            }
        }
        out
    };
    let num: Vec<f32> = field.iter().zip(&inside).map(|(f, i)| f * i).collect();
    let num = pass(&pass(&num, true), false);
    let den = pass(&pass(&inside, true), false);
    num.iter().zip(&den).map(|(n, d)| if *d > 0.0 { n / d } else { 0.0 }).collect()
}

fn blur_rgb(rgb: &[[f32; 3]], r: usize) -> Vec<[f32; 3]> {
    let ch: Vec<Vec<f32>> = (0..3).map(|c| blur(&rgb.iter().map(|p| p[c]).collect::<Vec<_>>(), r)).collect();
    (0..NPX).map(|p| [ch[0][p], ch[1][p], ch[2][p]]).collect()
}

/// 3x3 majority vote over the diamond.
fn majority(m: &[bool]) -> Vec<bool> {
    let f: Vec<f32> = m.iter().map(|b| *b as u8 as f32).collect();
    blur(&f, 1).iter().map(|v| *v > 0.5).collect()
}

/// Smallest water body or islet kept from the Civ3 masks, in pixels.
const SPECK_PX: usize = 24;

/// Flip connected regions of `value` smaller than `min` pixels that do not
/// reach the diamond's rim (rim-touching ones continue into a neighbour).
fn drop_specks(m: &[bool], value: bool, min: usize) -> Vec<bool> {
    let mut out = m.to_vec();
    let mut seen = vec![false; NPX];
    for start in 0..NPX {
        if seen[start] || m[start] != value || !in_diamond(start % CW, start / CW) {
            continue;
        }
        let (mut stack, mut comp, mut rim) = (vec![start], vec![], false);
        seen[start] = true;
        while let Some(p) = stack.pop() {
            comp.push(p);
            let (x, y) = ((p % CW) as isize, (p / CW) as isize);
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= CW as isize || ny >= CH as isize || !in_diamond(nx as usize, ny as usize) {
                    rim = true;
                    continue;
                }
                let q = ny as usize * CW + nx as usize;
                if !seen[q] && m[q] == value {
                    seen[q] = true;
                    stack.push(q);
                }
            }
        }
        if !rim && comp.len() < min {
            for p in comp {
                out[p] = !value;
            }
        }
    }
    out
}

/// Chamfer (3-4) distance in pixels from every pixel to the nearest `true`.
fn distance_to(m: &[bool]) -> Vec<f32> {
    let big = 1e6;
    let mut d: Vec<f32> = m.iter().map(|b| if *b { 0.0 } else { big }).collect();
    let at = |x: isize, y: isize| -> Option<usize> {
        (x >= 0 && y >= 0 && (x as usize) < CW && (y as usize) < CH).then(|| y as usize * CW + x as usize)
    };
    let fwd = [(-1, 0, 1.0), (-1, -1, 1.4142), (0, -1, 1.0), (1, -1, 1.4142)];
    for y in 0..CH as isize {
        for x in 0..CW as isize {
            let p = at(x, y).unwrap();
            for (dx, dy, c) in fwd {
                if let Some(q) = at(x + dx, y + dy) {
                    d[p] = d[p].min(d[q] + c);
                }
            }
        }
    }
    for y in (0..CH as isize).rev() {
        for x in (0..CW as isize).rev() {
            let p = at(x, y).unwrap();
            for (dx, dy, c) in fwd {
                if let Some(q) = at(x - dx, y - dy) {
                    d[p] = d[p].min(d[q] + c);
                }
            }
        }
    }
    d
}

fn cell_rgb(img: &Rgba, col: usize, row: usize) -> Vec<[f32; 3]> {
    let mut v = Vec::with_capacity(NPX);
    for y in 0..CH {
        for x in 0..CW {
            let c = img.get(col * CW + x, row * CH + y);
            v.push([c[0] as f32, c[1] as f32, c[2] as f32]);
        }
    }
    v
}

pub fn mean_color(rgb: &[[f32; 3]]) -> [f32; 3] {
    let mut s = [0.0f32; 3];
    let mut n = 0.0;
    for (_, _, p) in diamond_pixels() {
        for c in 0..3 {
            s[c] += rgb[p][c];
        }
        n += 1.0;
    }
    s.map(|v| v / n)
}

/// Mean and standard deviation of a sheet's pure cell for digit `d`.
pub fn pure_stats(img: &Rgba, d: usize) -> ([f32; 3], [f32; 3]) {
    let rgb = cell_rgb(img, 4 * d, 4 * d);
    let m = mean_color(&rgb);
    let mut v = [0.0f32; 3];
    let mut n = 0.0;
    for (_, _, p) in diamond_pixels() {
        for c in 0..3 {
            v[c] += (rgb[p][c] - m[c]).powi(2);
        }
        n += 1.0;
    }
    (m, v.map(|x| (x / n).sqrt()))
}

/// Colour distance (sigma) separating terrains in the soft land weights.
const COLOR_SIGMA: f32 = 18.0;
/// A land pixel this far from every land terrain colour, near water, is
/// beach (Civ3 paints a light sand strip along coastlines).
const BEACH_COLOR_DIST: f32 = 38.0;
const BEACH_MAX_PX: f32 = 10.0;

/// Recover masks from one original Civ3 sheet.
pub fn extract_civ3(spec: &SheetSpec, img: &Rgba, shore_min_px: f32) -> SheetMasks {
    let means: Vec<[f32; 3]> = (0..3).map(|d| pure_stats(img, d).0).collect();
    let wd = water_digit(spec);
    let mut cells = Vec::with_capacity(81);
    for row in 0..9 {
        for col in 0..9 {
            let digs = cell_digits(col, row);
            let cls = classes(spec, &digs);
            let water_here = wd.filter(|w| cls.contains(w));
            let land: Vec<usize> = cls.iter().copied().filter(|d| Some(*d) != water_here).collect();
            let rgb = cell_rgb(img, col, row);
            let blur1 = blur_rgb(&rgb, 1);
            let blur2 = blur_rgb(&rgb, 2);
            let vw: Vec<[f32; 4]> = (0..NPX).map(|p| vertex_weights(p % CW, p / CW)).collect();
            let prior = |d: usize, p: usize| -> f32 {
                (0..4).filter(|v| spec.canon(digs[*v]) == d).map(|v| vw[p][v]).sum()
            };
            let land_dist = |c: [f32; 3]| land.iter().map(|l| dist2(c, means[*l])).fold(f32::MAX, f32::min);

            // Water: a crisp cut in Civ3, so classify raw pixels.
            let water: Vec<bool> = match water_here {
                None => vec![false; NPX],
                Some(_) if land.is_empty() => vec![true; NPX],
                Some(w) => {
                    let raw: Vec<bool> = rgb.iter().map(|c| dist2(*c, means[w]) < land_dist(*c)).collect();
                    let m = majority(&majority(&raw));
                    // Classification specks: tiny ponds in land and islets in water.
                    let m = drop_specks(&m, true, SPECK_PX);
                    drop_specks(&m, false, SPECK_PX)
                }
            };

            // Land: soft weights from local colour, nudged by the vertex prior.
            let mut lw: Vec<Vec<f32>> = vec![vec![0.0; NPX]; 3];
            for (_, _, p) in diamond_pixels() {
                if land.len() == 1 {
                    lw[land[0]][p] = 1.0;
                    continue;
                }
                let logs: Vec<f32> = land
                    .iter()
                    .map(|d| -dist2(blur2[p], means[*d]) / (2.0 * COLOR_SIGMA * COLOR_SIGMA) + (prior(*d, p) + 0.02).ln())
                    .collect();
                let mx = logs.iter().copied().fold(f32::MIN, f32::max);
                let e: Vec<f32> = logs.iter().map(|l| (l - mx).exp()).collect();
                let sum: f32 = e.iter().sum();
                for (i, d) in land.iter().enumerate() {
                    lw[*d][p] = e[i] / sum;
                }
            }
            if land.len() > 1 {
                for d in &land {
                    lw[*d] = blur(&lw[*d], 1);
                }
            }

            // Beach: off-colour land pixels close to the water, plus a
            // minimum band so sand-coloured land still gets a shoreline.
            let mut shore = vec![false; NPX];
            if let (Some(w), false) = (water_here, land.is_empty()) {
                let dw = distance_to(&water);
                for (_, _, p) in diamond_pixels() {
                    if water[p] {
                        continue;
                    }
                    let off = land_dist(blur1[p]).sqrt() > BEACH_COLOR_DIST
                        && dist2(blur1[p], means[w]).sqrt() > BEACH_COLOR_DIST;
                    shore[p] = (off && dw[p] <= BEACH_MAX_PX) || dw[p] <= shore_min_px;
                }
                shore = majority(&shore).iter().zip(&water).map(|(s, w)| *s && !*w).collect();
            }

            let mut m = CellMask::empty();
            for (_, _, p) in diamond_pixels() {
                let lsum: f32 = land.iter().map(|d| lw[*d][p]).sum::<f32>().max(1e-6);
                for d in 0..3 {
                    m.w[d][p] = match water_here {
                        Some(w) if water[p] => (d == w) as u8 as f32,
                        _ if land.contains(&d) => lw[d][p] / lsum,
                        _ => 0.0,
                    };
                }
                m.shore[p] = shore[p] as u8 as f32;
            }
            force_vertices(&mut m, spec, &digs);
            cells.push(m);
        }
    }
    SheetMasks { cells }
}

// --------------------------------------------------------- harmonize

const PROFILE_N: usize = 64;
const SAMPLE_DEPTH: f32 = 1.5;
const BLEND_DEPTH: f32 = 5.0;
/// Seven terrain kinds, then shore.
type KindVec = [f32; 8];

fn kind_vec(spec: &SheetSpec, m: &CellMask, p: usize) -> KindVec {
    let mut v = [0.0; 8];
    for d in 0..3 {
        v[spec.kinds[d].index()] += m.w[d][p];
    }
    v[7] = m.shore[p];
    v
}

/// Make every shared cell edge agree. Neighbouring cells can come from
/// different sheets, and the Civ3 artists matched edges only roughly
/// (~97% within a sheet), so the extracted masks are pulled, within a few
/// pixels of each edge, onto one canonical profile per
/// (edge axis, start terrain, end terrain), averaged over every sheet.
pub fn harmonize(sheets: &mut [SheetMasks]) {
    let mut sums: HashMap<(usize, Kind, Kind), (Vec<KindVec>, f32)> = HashMap::new();
    for (spec, sheet) in SHEETS.iter().zip(sheets.iter()) {
        for (i, m) in sheet.cells.iter().enumerate() {
            let digs = cell_digits(i % 9, i / 9);
            for (e, edge) in EDGES.iter().enumerate() {
                let key = (edge.axis, spec.kinds[digs[edge.start]], spec.kinds[digs[edge.end]]);
                let entry = sums.entry(key).or_insert_with(|| (vec![[0.0; 8]; PROFILE_N], 0.0));
                for j in 0..PROFILE_N {
                    let u = (j as f32 + 0.5) / PROFILE_N as f32;
                    let (x, y) = edge_px(e, u, SAMPLE_DEPTH);
                    let v = kind_vec(spec, m, y * CW + x);
                    for k in 0..8 {
                        entry.0[j][k] += v[k];
                    }
                }
                entry.1 += 1.0;
            }
        }
    }
    // Average, then re-sharpen the binary parts (land vs water, beach) so
    // the shared profile is a crisp coastline, not a smear.
    let canon: HashMap<(usize, Kind, Kind), Vec<KindVec>> = sums
        .into_iter()
        .map(|(key, (s, n))| {
            let prof = s
                .into_iter()
                .map(|mut v| {
                    v.iter_mut().for_each(|x| *x /= n);
                    let water: f32 = Kind::ALL.iter().filter(|k| k.is_water()).map(|k| v[k.index()]).sum();
                    let sharp = smoothstep(0.3, 0.7, water);
                    for k in Kind::ALL {
                        let i = k.index();
                        v[i] *= if k.is_water() { sharp / water.max(1e-6) } else { (1.0 - sharp) / (1.0 - water).max(1e-6) };
                    }
                    v[7] = smoothstep(0.3, 0.7, v[7]) * (1.0 - sharp);
                    v
                })
                .collect();
            (key, prof)
        })
        .collect();

    for (spec, sheet) in SHEETS.iter().zip(sheets.iter_mut()) {
        for (i, m) in sheet.cells.iter_mut().enumerate() {
            let digs = cell_digits(i % 9, i / 9);
            for (e, edge) in EDGES.iter().enumerate() {
                let (ds, de) = (spec.canon(digs[edge.start]), spec.canon(digs[edge.end]));
                let prof = &canon[&(edge.axis, spec.kinds[ds], spec.kinds[de])];
                for (x, y, p) in diamond_pixels() {
                    let (u, depth) = edge_coords(e, x, y);
                    let a = 1.0 - smoothstep(SAMPLE_DEPTH, BLEND_DEPTH, depth);
                    if a <= 0.0 {
                        continue;
                    }
                    let f = (u * PROFILE_N as f32 - 0.5).clamp(0.0, (PROFILE_N - 1) as f32);
                    let (j0, j1) = (f.floor() as usize, (f.floor() as usize + 1).min(PROFILE_N - 1));
                    let t = f - j0 as f32;
                    let v: KindVec = std::array::from_fn(|k| prof[j0][k] * (1.0 - t) + prof[j1][k] * t);
                    // Only the edge's two endpoint terrains exist on it.
                    let mut target = [0.0f32; 3];
                    for d in [ds, de] {
                        target[d] = v[spec.kinds[d].index()];
                    }
                    let tsum: f32 = target.iter().sum();
                    if tsum <= 1e-6 {
                        continue;
                    }
                    for d in 0..3 {
                        m.w[d][p] += (target[d] / tsum - m.w[d][p]) * a;
                    }
                    m.shore[p] += (v[7] - m.shore[p]) * a;
                }
            }
            force_vertices(m, spec, &digs);
        }
    }
}

// --------------------------------------------------------- procedural

pub struct ProcParams {
    pub seed: u32,
    /// How far noise may push a boundary, in vertex-weight units.
    pub amp: f32,
    /// Beach width, in vertex-weight units.
    pub shore: f32,
}

/// Masks from bilinear vertex weights distorted by lattice-periodic noise.
pub fn procedural(spec: &SheetSpec, pp: &ProcParams) -> SheetMasks {
    let noise: Vec<LatticeNoise> = Kind::ALL.iter().map(|k| LatticeNoise::new(pp.seed.wrapping_mul(31) + k.index() as u32)).collect();
    let shore_noise = LatticeNoise::new(pp.seed.wrapping_mul(31) + 101);
    let wd = water_digit(spec);
    let mut cells = Vec::with_capacity(81);
    for row in 0..9 {
        for col in 0..9 {
            let digs = cell_digits(col, row);
            let cls = classes(spec, &digs);
            let water_here = wd.filter(|w| cls.contains(w));
            let land: Vec<usize> = cls.iter().copied().filter(|d| Some(*d) != water_here).collect();
            let mut m = CellMask::empty();
            for (x, y, p) in diamond_pixels() {
                let vw = vertex_weights(x, y);
                let taper = 1.0 - vw.iter().copied().fold(0.0, f32::max);
                let (a, b) = lattice(x, y);
                let field = |d: usize| -> f32 {
                    let base: f32 = (0..4).filter(|v| spec.canon(digs[*v]) == d).map(|v| vw[v]).sum();
                    let n = noise[spec.kinds[d].index()].fbm(a, b, 3, 4) * 2.0 - 1.0;
                    // Noise fades with the terrain's own weight, so a terrain
                    // never appears on an edge it has no vertex on.
                    base + pp.amp * taper * n * (base * 3.0).min(1.0)
                };
                let f: Vec<(usize, f32)> = cls.iter().map(|d| (*d, field(*d))).collect();
                let fl = f.iter().filter(|(d, _)| land.contains(d)).map(|x| x.1).fold(f32::MIN, f32::max);
                let (water, shore) = match water_here {
                    Some(_) if land.is_empty() => (1.0, 0.0),
                    Some(w) => {
                        let fw = f.iter().find(|x| x.0 == w).unwrap().1;
                        let water = smoothstep(-0.015, 0.015, fw - fl);
                        let width = pp.shore * (0.4 + 1.2 * shore_noise.fbm(a, b, 4, 3));
                        (water, (1.0 - water) * (1.0 - smoothstep(0.0, width, fl - fw)))
                    }
                    None => (0.0, 0.0),
                };
                let tau = 0.07;
                let e: Vec<(usize, f32)> = f
                    .iter()
                    .filter(|(d, _)| land.contains(d))
                    .map(|(d, v)| (*d, ((v - fl) / tau).exp()))
                    .collect();
                let sum: f32 = e.iter().map(|x| x.1).sum::<f32>().max(1e-9);
                for (d, v) in e {
                    m.w[d][p] = (1.0 - water) * v / sum;
                }
                if let Some(w) = water_here {
                    m.w[w][p] = water;
                }
                m.shore[p] = shore;
            }
            force_vertices(&mut m, spec, &digs);
            cells.push(m);
        }
    }
    SheetMasks { cells }
}

/// Debug view of a sheet's masks: each digit drawn in a flat colour for
/// its terrain, beach in light sand.
pub fn visualize(spec: &SheetSpec, sheet: &SheetMasks) -> image::RgbaImage {
    let color = |k: Kind| -> [f32; 3] {
        match k {
            Kind::Grassland => [70.0, 150.0, 40.0],
            Kind::Plains => [190.0, 160.0, 60.0],
            Kind::Desert => [235.0, 210.0, 140.0],
            Kind::Tundra => [200.0, 205.0, 190.0],
            Kind::Coast => [90.0, 200.0, 190.0],
            Kind::Sea => [40.0, 120.0, 170.0],
            Kind::Ocean => [20.0, 50.0, 110.0],
        }
    };
    let mut img = image::RgbaImage::new(SHEET_W as u32, SHEET_H as u32);
    for (i, m) in sheet.cells.iter().enumerate() {
        let (col, row) = (i % 9, i / 9);
        for (x, y, p) in diamond_pixels() {
            let mut c = [0.0f32; 3];
            for d in 0..3 {
                let k = color(spec.kinds[d]);
                for j in 0..3 {
                    c[j] += k[j] * m.w[d][p];
                }
            }
            for j in 0..3 {
                c[j] += ([255.0, 245.0, 200.0][j] - c[j]) * m.shore[p];
            }
            img.put_pixel((col * CW + x) as u32, (row * CH + y) as u32, image::Rgba([c[0] as u8, c[1] as u8, c[2] as u8, 255]));
        }
    }
    img
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(stem: &str) -> &'static SheetSpec {
        SHEETS.iter().find(|s| s.stem == stem).unwrap()
    }

    fn pp() -> ProcParams {
        ProcParams { seed: 3, amp: 0.45, shore: 0.1 }
    }

    #[test]
    fn procedural_vertices_are_pure() {
        for s in &SHEETS {
            let sheet = procedural(s, &pp());
            for (i, m) in sheet.cells.iter().enumerate() {
                let digs = cell_digits(i % 9, i / 9);
                for (x, y, p) in diamond_pixels() {
                    for v in 0..4 {
                        if vertex_dist(x, y, v) < VERTEX_CORE * 0.6 {
                            assert!(m.w[s.canon(digs[v])][p] > 0.999, "{} cell {i} vertex {v}", s.stem);
                            assert!(m.shore[p] < 1e-3);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn procedural_weights_sum_to_one() {
        let sheet = procedural(spec("xpgc"), &pp());
        for m in &sheet.cells {
            for (_, _, p) in diamond_pixels() {
                let s: f32 = (0..3).map(|d| m.w[d][p]).sum();
                assert!((s - 1.0).abs() < 1e-3, "{s}");
            }
        }
    }

    #[test]
    fn procedural_edges_match_their_partner() {
        // Cell A's NE edge is the SW edge of a cell whose W/S vertices
        // carry A's N/E terrains. In xpgc: A = (N plains, E coast, S grass,
        // W grass); B = (N grass, E grass, S coast, W plains).
        let s = spec("xpgc");
        let sheet = procedural(s, &pp());
        let cell = |n: usize, e: usize, so: usize, w: usize| &sheet.cells[(3 * so + e) * 9 + 3 * w + n];
        let a = cell(0, 2, 1, 1);
        let b = cell(1, 1, 2, 0);
        let mut worst: f32 = 0.0;
        for j in 1..63 {
            let u = j as f32 / 64.0;
            let (xa, ya) = edge_px(0, u, 0.6);
            let (xb, yb) = edge_px(1, u, 0.6);
            let pa = ya * CW + xa;
            let pb = yb * CW + xb;
            worst = worst.max((a.w[2][pa] - b.w[2][pb]).abs());
        }
        assert!(worst < 0.35, "coast weight differs across the shared edge by {worst}");
    }
}
