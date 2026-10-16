//! Supersampled land and region rasters, and their reduction to the tile lattice.
use crate::geo::{
    GX, GY, Polygon, Rows, SIDE, SUPER, fill_polygon, in_lattice, plane_to_grid, sample_cols,
    sample_rows,
};
use std::collections::BTreeMap;

/// Land counts for a tile to be land when at least this many of its (about) 16 samples are land.
pub const LAND_SAMPLES: usize = 8;
/// A land mass must cover at least this many samples to earn a tile of its own.
pub const MIN_ISLAND_SAMPLES: usize = 3;

pub struct Raster {
    pub rows: Rows,
    pub land: Vec<bool>,
    pub region: Vec<u16>,
}

pub struct PaintArea {
    /// west, south, east, north in degrees.
    pub bbox: Option<[f64; 4]>,
    /// Only claim samples that are land (the default for everything except explicit sea claims).
    pub land_only: bool,
}

impl Raster {
    pub fn new(countries: &BTreeMap<String, Vec<Polygon>>, lakes: &[Polygon]) -> Self {
        let rows = Rows::new();
        let mut land = vec![false; GX * GY];
        for polygons in countries.values() {
            for polygon in polygons {
                fill_polygon(&rows, polygon, |j, a, b| {
                    land[j * GX + a..j * GX + b].fill(true)
                });
            }
        }
        for lake in lakes {
            fill_polygon(&rows, lake, |j, a, b| {
                land[j * GX + a..j * GX + b].fill(false)
            });
        }
        Self {
            rows,
            land,
            region: vec![0; GX * GY],
        }
    }

    /// Assigns `region` to the samples covered by the polygons.
    pub fn paint(&mut self, polygons: &[Polygon], area: &PaintArea, region: u16) {
        let (rmin, rmax, cmin, cmax) = match area.bbox {
            Some([west, south, east, north]) => {
                let (rmin, rmax) = sample_rows(south, north);
                let (cmin, cmax) = sample_cols(west, east);
                (rmin, rmax, cmin, cmax)
            }
            None => (0, GY, 0, GX),
        };
        for polygon in polygons {
            fill_polygon(&self.rows, polygon, |j, a, b| {
                if j < rmin || j >= rmax {
                    return;
                }
                for i in a.max(cmin)..b.min(cmax) {
                    let at = j * GX + i;
                    if !area.land_only || self.land[at] {
                        self.region[at] = region;
                    }
                }
            });
        }
    }
}

pub struct Tiles {
    pub land: Vec<bool>,
    /// Region (0 = unclaimed) by plurality of the tile's land samples.
    pub region: Vec<u16>,
}

/// The tile (index into the `SIDE x SIDE` grid) whose diamond holds raster sample `(i, j)`, if
/// the lattice reaches it: the half-tile rows leave a ragged margin at the plane's edge.
fn sample_tile(i: usize, j: usize) -> Option<usize> {
    let (x, y) = (
        (i as f64 + 0.5) / SUPER as f64,
        (j as f64 + 0.5) / SUPER as f64,
    );
    let (u, v) = plane_to_grid(x, y);
    at(u.floor() as i32, v.floor() as i32)
}

impl Tiles {
    pub fn reduce(raster: &Raster) -> Self {
        let cells = SIDE * SIDE;
        let owner: Vec<Option<usize>> = (0..GX * GY)
            .map(|at| sample_tile(at % GX, at / GX))
            .collect();
        let mut coverage = vec![0u8; cells];
        for (at, tile) in owner.iter().enumerate() {
            if let (true, Some(tile)) = (raster.land[at], tile) {
                coverage[*tile] += 1;
            }
        }
        let mut land: Vec<bool> = coverage
            .iter()
            .map(|&c| c as usize >= LAND_SAMPLES)
            .collect();

        // Small islands would otherwise vanish: give each sizeable land mass without a tile its
        // best-covered one.
        let mut seen = vec![false; GX * GY];
        let mut stack = Vec::new();
        for start in 0..GX * GY {
            if !raster.land[start] || seen[start] {
                continue;
            }
            let mut size = 0usize;
            let mut per_tile: BTreeMap<usize, u8> = BTreeMap::new();
            seen[start] = true;
            stack.push(start);
            while let Some(at) = stack.pop() {
                size += 1;
                let (i, j) = (at % GX, at / GX);
                if let Some(tile) = owner[at] {
                    *per_tile.entry(tile).or_default() += 1;
                }
                let mut visit = |next: usize| {
                    if raster.land[next] && !seen[next] {
                        seen[next] = true;
                        stack.push(next);
                    }
                };
                if i > 0 {
                    visit(at - 1);
                }
                if i + 1 < GX {
                    visit(at + 1);
                }
                if j > 0 {
                    visit(at - GX);
                }
                if j + 1 < GY {
                    visit(at + GX);
                }
            }
            if size >= MIN_ISLAND_SAMPLES
                && !per_tile.is_empty()
                && !per_tile.keys().any(|&tile| land[tile])
            {
                let best = per_tile
                    .iter()
                    .max_by_key(|&(&tile, &count)| (count, std::cmp::Reverse(tile)))
                    .map(|(&tile, _)| tile)
                    .expect("component has samples");
                land[best] = true;
            }
        }

        let mut tallies: Vec<Vec<(u16, u8)>> = vec![Vec::new(); cells];
        for (at, tile) in owner.iter().enumerate() {
            let Some(tile) = tile.filter(|&t| land[t]) else {
                continue;
            };
            if !raster.land[at] {
                continue;
            }
            let value = raster.region[at];
            let tally = &mut tallies[tile];
            match tally.iter_mut().find(|(r, _)| *r == value) {
                Some(entry) => entry.1 += 1,
                None => tally.push((value, 1)),
            }
        }
        // Plurality; ties prefer a claimed region over unclaimed, then the earlier region.
        let region = tallies
            .iter()
            .map(|tally| {
                tally
                    .iter()
                    .max_by_key(|&&(r, n)| (n, r != 0, std::cmp::Reverse(r)))
                    .map_or(0, |&(r, _)| r)
            })
            .collect();
        Self { land, region }
    }
}

/// Index of grid cell `(x, y)` when it is a tile of the lattice (not the void around it).
pub fn at(x: i32, y: i32) -> Option<usize> {
    in_lattice(x, y).then(|| y as usize * SIDE + x as usize)
}

/// Whether grid index `i` is a tile of the lattice.
pub fn exists(i: usize) -> bool {
    in_lattice((i % SIDE) as i32, (i / SIDE) as i32)
}
