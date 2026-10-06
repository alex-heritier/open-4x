//! Cell geometry shared by every stage.
//!
//! A sheet is 9x9 cells of 128x64. Each cell is a diamond whose four
//! vertices (N, E, S, W = screen up, right, down, left) sit on the centers
//! of four map tiles, and the cell index encodes the terrain at each
//! vertex as a digit into the sheet's terrain triple:
//!
//! ```text
//! col = 3*W + N        row = 3*S + E
//! ```
//!
//! (`reverse-engineering/blending.md`, `src/blend.rs`.)
//!
//! Two coordinate systems are used inside a cell:
//!
//! * `(s, t)`: the diamond as a parallelogram, `p = W + s*(N-W) + t*(S-W)`.
//!   Corners: W = (0,0), N = (1,0), S = (0,1), E = (1,1). Bilinear vertex
//!   weights come straight from these.
//! * `(a, b)`: lattice coordinates, `a = x/128 + y/64`, `b = x/128 - y/64`.
//!   Cells are placed on the map at integer lattice steps, so anything that
//!   is periodic with period 1 in `a` and `b` lines up across every cell
//!   boundary, in every sheet. Materials and noise are built that way.

pub const CW: usize = 128;
pub const CH: usize = 64;
pub const NPX: usize = CW * CH;
pub const SHEET_W: usize = 9 * CW;
pub const SHEET_H: usize = 9 * CH;

/// Perpendicular distance between opposite diamond edges, in pixels
/// (area 4096 over edge length sqrt(64^2 + 32^2)).
pub const EDGE_SPAN_PX: f32 = 57.24;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum Kind {
    Grassland,
    Plains,
    Desert,
    Tundra,
    Coast,
    Sea,
    Ocean,
}

impl Kind {
    pub const ALL: [Kind; 7] = [
        Kind::Grassland,
        Kind::Plains,
        Kind::Desert,
        Kind::Tundra,
        Kind::Coast,
        Kind::Sea,
        Kind::Ocean,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Grassland => "grassland",
            Kind::Plains => "plains",
            Kind::Desert => "desert",
            Kind::Tundra => "tundra",
            Kind::Coast => "coast",
            Kind::Sea => "sea",
            Kind::Ocean => "ocean",
        }
    }

    pub fn from_name(s: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.name() == s)
    }

    pub fn is_water(self) -> bool {
        matches!(self, Kind::Coast | Kind::Sea | Kind::Ocean)
    }

    pub fn index(self) -> usize {
        self as usize
    }
}

/// One output sheet: file stem and its digit 0/1/2 terrain triple, in the
/// engine's load order. `wSSS`/`wOOO` are single-terrain variant sheets.
pub struct SheetSpec {
    pub stem: &'static str,
    pub kinds: [Kind; 3],
}

use Kind::*;
pub const SHEETS: [SheetSpec; 9] = [
    SheetSpec { stem: "xtgc", kinds: [Tundra, Grassland, Coast] },
    SheetSpec { stem: "xpgc", kinds: [Plains, Grassland, Coast] },
    SheetSpec { stem: "xdgc", kinds: [Desert, Grassland, Coast] },
    SheetSpec { stem: "xdpc", kinds: [Desert, Plains, Coast] },
    SheetSpec { stem: "xdgp", kinds: [Desert, Grassland, Plains] },
    SheetSpec { stem: "xggc", kinds: [Grassland, Grassland, Coast] },
    SheetSpec { stem: "wCSO", kinds: [Coast, Sea, Ocean] },
    SheetSpec { stem: "wSSS", kinds: [Sea, Sea, Sea] },
    SheetSpec { stem: "wOOO", kinds: [Ocean, Ocean, Ocean] },
];

impl SheetSpec {
    /// Digits naming the same terrain collapse onto the last one
    /// (`xggc` lists grass twice; the engine uses digit 1).
    pub fn canon(&self, d: usize) -> usize {
        let k = self.kinds[d];
        self.kinds.iter().rposition(|x| *x == k).unwrap()
    }
}

/// Vertex digits `[N, E, S, W]` of cell `(col, row)`.
pub fn cell_digits(col: usize, row: usize) -> [usize; 4] {
    [col % 3, row % 3, row / 3, col / 3]
}

/// Whether pixel `(x, y)` of a cell is inside the tile diamond. Matches the
/// opaque area of the Civ3 sheets exactly.
pub fn in_diamond(x: usize, y: usize) -> bool {
    (x as f32 + 0.5 - 64.0).abs() / 64.0 + (y as f32 + 0.5 - 32.0).abs() / 32.0 <= 1.0
}

/// Parallelogram coordinates `(s, t)` of a pixel center, clamped to [0, 1].
pub fn st(x: usize, y: usize) -> (f32, f32) {
    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
    let u = px / 64.0;
    let v = (py - 32.0) / 32.0;
    (((u - v) / 2.0).clamp(0.0, 1.0), ((u + v) / 2.0).clamp(0.0, 1.0))
}

/// Pixel of the cell at parallelogram coordinates `(s, t)`.
pub fn st_to_px(s: f32, t: f32) -> (usize, usize) {
    let x = 64.0 * (s + t);
    let y = 32.0 - 32.0 * s + 32.0 * t;
    (
        (x.floor() as isize).clamp(0, CW as isize - 1) as usize,
        (y.floor() as isize).clamp(0, CH as isize - 1) as usize,
    )
}

/// Bilinear vertex weights `[N, E, S, W]` at a pixel; they sum to 1 and
/// each is 1 at its own vertex. Along an edge only its two endpoints
/// carry weight, which is what keeps neighbouring cells consistent.
pub fn vertex_weights(x: usize, y: usize) -> [f32; 4] {
    let (s, t) = st(x, y);
    [s * (1.0 - t), s * t, (1.0 - s) * t, (1.0 - s) * (1.0 - t)]
}

/// Lattice coordinates `(a, b)` of a pixel center (see module docs).
pub fn lattice(x: usize, y: usize) -> (f32, f32) {
    let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
    (px / 128.0 + py / 64.0, px / 128.0 - py / 64.0)
}

/// Vertex positions `[N, E, S, W]` in cell pixels.
pub const VERTS: [(f32, f32); 4] = [(64.0, 0.0), (128.0, 32.0), (64.0, 64.0), (0.0, 32.0)];

/// Diamond-norm distance from a pixel center to a vertex (1.0 = a full
/// half-diamond away).
pub fn vertex_dist(x: usize, y: usize, v: usize) -> f32 {
    let (vx, vy) = VERTS[v];
    (x as f32 + 0.5 - vx).abs() / 64.0 + (y as f32 + 0.5 - vy).abs() / 32.0
}

/// The four edges of a cell. Edges on the same axis are shared between
/// neighbouring cells: cell A's NE edge is the SW edge of the cell above
/// and to its right, with the same start and end tiles.
#[derive(Clone, Copy, Debug)]
pub struct Edge {
    /// 0: the NE/SW axis, 1: the NW/SE axis.
    pub axis: usize,
    /// Vertex index (N, E, S, W) at u = 0 and u = 1.
    pub start: usize,
    pub end: usize,
}

pub const EDGES: [Edge; 4] = [
    Edge { axis: 0, start: 0, end: 1 }, // NE: s = 1, u = t
    Edge { axis: 0, start: 3, end: 2 }, // SW: s = 0, u = t
    Edge { axis: 1, start: 3, end: 0 }, // NW: t = 0, u = s
    Edge { axis: 1, start: 2, end: 1 }, // SE: t = 1, u = s
];

/// For edge `e` (index into `EDGES`): `(u, depth_px)` of a pixel.
pub fn edge_coords(e: usize, x: usize, y: usize) -> (f32, f32) {
    let (s, t) = st(x, y);
    match e {
        0 => (t, (1.0 - s) * EDGE_SPAN_PX),
        1 => (t, s * EDGE_SPAN_PX),
        2 => (s, t * EDGE_SPAN_PX),
        _ => (s, (1.0 - t) * EDGE_SPAN_PX),
    }
}

/// Inverse of `edge_coords`: pixel at `(u, depth_px)` from edge `e`.
pub fn edge_px(e: usize, u: f32, depth: f32) -> (usize, usize) {
    let d = depth / EDGE_SPAN_PX;
    match e {
        0 => st_to_px(1.0 - d, u),
        1 => st_to_px(d, u),
        2 => st_to_px(u, d),
        _ => st_to_px(u, 1.0 - d),
    }
}

pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertex_weights_peak_at_their_vertex() {
        let near = |v: usize| {
            let (vx, vy) = VERTS[v];
            let x = (vx as usize).min(CW - 1);
            let y = (vy as usize).min(CH - 1);
            vertex_weights(x, y)[v]
        };
        for v in 0..4 {
            assert!(near(v) > 0.95, "vertex {v}: {}", near(v));
        }
    }

    #[test]
    fn lattice_steps_are_cell_offsets() {
        // Neighbouring cells sit (64, 32) and (64, -32) apart on screen.
        let (a0, b0) = lattice(10, 40);
        let (a1, b1) = lattice(10 + 64, 40 + 32);
        assert!(((a1 - a0) - 1.0).abs() < 1e-5 && (b1 - b0).abs() < 1e-5);
        let (a2, b2) = lattice(10 + 64, 40 - 32);
        assert!((a2 - a0).abs() < 1e-5 && ((b2 - b0) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn edges_have_shared_endpoints() {
        // NE start/end = N, E; SW start/end = W, S, the same two tiles for
        // the cell above-right. NW W->N matches SE S->E of the cell
        // above-left.
        assert_eq!((EDGES[0].start, EDGES[0].end), (0, 1));
        assert_eq!((EDGES[1].start, EDGES[1].end), (3, 2));
        let (u, d) = edge_coords(0, 90, 20);
        let (x, y) = edge_px(0, u, d);
        assert!((x as i32 - 90).abs() <= 1 && (y as i32 - 20).abs() <= 1);
    }
}
