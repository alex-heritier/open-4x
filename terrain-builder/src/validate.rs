//! Checks a built pack against the invariants the engine relies on.
//!
//! * Vertex purity: near each vertex a cell shows exactly the vertex
//!   terrain's tile, so every tile center reads as its own type.
//! * Shared edges: the two cells on either side of any map edge come from
//!   arbitrary sheets; their pixels along the edge must agree as well as
//!   neighbouring pixels inside a single all-one-terrain cell do.

use std::collections::HashMap;

use image::RgbaImage;
use serde::Serialize;

use crate::compose::Palette;
use crate::geom::*;
use crate::masks::SheetMasks;

#[derive(Serialize, Default, Debug)]
pub struct Report {
    /// Cell vertices whose core is not 100% the vertex terrain.
    pub vertex_failures: usize,
    pub vertices_checked: usize,
    /// Largest colour difference (0-255) in a vertex core between the
    /// sheet and the terrain's own tile.
    pub worst_vertex_color: f32,
    /// Mean colour step across shared edges between different-terrain
    /// edges, over the same step across all-one-terrain edges. ~1 is
    /// seamless.
    pub edge_seam_ratio: f32,
    /// Same, from the rendered preview map.
    pub preview_seam_ratio: Option<f32>,
}

pub fn check(sheets: &[(&SheetSpec, &SheetMasks, &RgbaImage)], pal: &Palette) -> Report {
    let mut r = Report::default();
    for (spec, masks, img) in sheets {
        for (i, m) in masks.cells.iter().enumerate() {
            let (col, row) = (i % 9, i / 9);
            let digs = cell_digits(col, row);
            for v in 0..4 {
                r.vertices_checked += 1;
                let d = spec.canon(digs[v]);
                let tile = &pal.tiles[&spec.kinds[d]];
                let mut ok = true;
                for y in 0..CH {
                    for x in 0..CW {
                        if !in_diamond(x, y) || vertex_dist(x, y, v) > 0.08 {
                            continue;
                        }
                        let p = y * CW + x;
                        if m.w[d][p] < 0.99 || m.shore[p] > 0.01 {
                            ok = false;
                        }
                        let c = img.get_pixel((col * CW + x) as u32, (row * CH + y) as u32);
                        for k in 0..3 {
                            r.worst_vertex_color = r.worst_vertex_color.max((c[k] as f32 - tile[p][k]).abs());
                        }
                    }
                }
                if !ok {
                    r.vertex_failures += 1;
                }
            }
        }
    }
    r.edge_seam_ratio = edge_seams(sheets);
    r
}

const SAMPLES: usize = 48;
const DEPTH: f32 = 0.7;

fn edge_seams(sheets: &[(&SheetSpec, &SheetMasks, &RgbaImage)]) -> f32 {
    // Colour profiles just inside every cell edge, grouped by the shared
    // edge they lie on: (axis, start terrain, end terrain), split by side.
    type Prof = Vec<[f32; 3]>;
    let mut sides: HashMap<(usize, Kind, Kind), [Vec<Prof>; 2]> = HashMap::new();
    for (spec, _, img) in sheets {
        for i in 0..81 {
            let (col, row) = (i % 9, i / 9);
            let digs = cell_digits(col, row);
            for (e, edge) in EDGES.iter().enumerate() {
                let key = (edge.axis, spec.kinds[digs[edge.start]], spec.kinds[digs[edge.end]]);
                let prof: Prof = (0..SAMPLES)
                    .map(|j| {
                        let u = (j as f32 + 0.5) / SAMPLES as f32;
                        let (x, y) = edge_px(e, u, DEPTH);
                        let c = img.get_pixel((col * CW + x) as u32, (row * CH + y) as u32);
                        [c[0] as f32, c[1] as f32, c[2] as f32]
                    })
                    .collect();
                // Edges 0 (NE) and 2 (NW) face edges 1 (SW) and 3 (SE).
                sides.entry(key).or_default()[e % 2].push(prof);
            }
        }
    }
    let (mut mixed, mut nm, mut pure, mut np) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for ((_, ks, ke), [a, b]) in &sides {
        // Cap the pairs per key; the profiles within a side are similar.
        for pa in a.iter().take(12) {
            for pb in b.iter().take(12) {
                let d: f64 = pa.iter().zip(pb).map(|(p, q)| (0..3).map(|c| (p[c] - q[c]).abs() as f64).sum::<f64>()).sum::<f64>()
                    / SAMPLES as f64;
                if ks == ke {
                    pure += d;
                    np += 1.0;
                } else {
                    mixed += d;
                    nm += 1.0;
                }
            }
        }
    }
    ((mixed / nm.max(1.0)) / (pure / np.max(1.0)).max(1e-6)) as f32
}
