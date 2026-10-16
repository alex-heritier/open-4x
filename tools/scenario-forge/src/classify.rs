//! Paints the map the way Civ3 layers a square: base terrain from climate, relief from
//! elevation, cover (forest, jungle, marsh) on top, rivers along tile edges, and depth classes
//! for the water.
//!
//! Everything here is derived from the pinned source data in `source.rs`, so the same tile
//! always gets the same answer. `data/terrain.json` only carries hand corrections.
use crate::data::{Patch, TerrainSource};
use crate::geo::{SIDE, contains, hash01, tile_bounds, tile_centre, tile_plane, tile_space};
use crate::source::{Climate, Elevation, Koeppen, River, Stats};
use crate::world::{at, exists};
use fourx_sim::terrain::{Cover, Relief, Terrain, Tile};
use std::collections::{HashSet, VecDeque};

pub struct Inputs<'a> {
    pub elevation: &'a Elevation,
    pub climate: &'a Climate,
    pub rivers: &'a [River],
}

/// One entry per cell of the `SIDE x SIDE` grid, row-major; cells outside the lattice are void
/// and hold open ocean.
pub struct Layers {
    pub terrain: Vec<Terrain>,
    pub relief: Vec<Relief>,
    pub cover: Vec<Cover>,
    pub river: Vec<u8>,
}

/// Fraction of land that becomes mountains and hills (the rest is flat). Civ3 maps run near
/// these shares; the actual tiles are the roughest ones by ETOPO5.
const MOUNTAIN_SHARE: f64 = 0.065;
const HILL_SHARE: f64 = 0.165;
/// Mountains need a real peak, hills at least a rise.
const MOUNTAIN_MIN_PEAK: f64 = 1500.0;
const HILL_MIN_PEAK: f64 = 250.0;
/// Trees stop growing above this mean elevation.
const TREE_LINE: f64 = 3300.0;
/// Water shallower than this mean depth is coast; deeper than `SEA_DEPTH` is ocean.
const COAST_DEPTH: f64 = 200.0;
const SEA_DEPTH: f64 = 2500.0;

/// What a climate grows: the base terrain and the chance (0..=1) that a tile is covered.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Biome {
    pub terrain: Terrain,
    pub forest: f64,
    pub jungle: f64,
}

/// Maps a Köppen class to its Civ3 base terrain and vegetation.
pub fn biome(class: Option<Koeppen>) -> Biome {
    let (terrain, forest, jungle) = match class {
        // Without a class (a tiny island) assume temperate grassland.
        None => (Terrain::Grass, 0.2, 0.0),
        Some(k) => match (k.group, k.rain, k.heat) {
            (b'A', b'f', _) => (Terrain::Grass, 0.0, 0.88),
            (b'A', b'm', _) => (Terrain::Grass, 0.1, 0.62),
            (b'A', _, _) => (Terrain::Plains, 0.16, 0.0),
            (b'B', b'W', _) => (Terrain::Desert, 0.0, 0.0),
            (b'B', _, _) => (Terrain::Plains, 0.03, 0.0),
            (b'C', b's', _) => (Terrain::Plains, 0.22, 0.0),
            (b'C', b'w', _) => (Terrain::Grass, 0.3, 0.0),
            (b'C', _, _) => (Terrain::Grass, 0.4, 0.0),
            (b'D', _, b'd') => (Terrain::Tundra, 0.4, 0.0),
            (b'D', _, b'c') => (Terrain::Plains, 0.72, 0.0),
            (b'D', b'f', _) => (Terrain::Grass, 0.5, 0.0),
            (b'D', b'w', _) => (Terrain::Plains, 0.4, 0.0),
            (b'D', _, _) => (Terrain::Plains, 0.3, 0.0),
            _ => (Terrain::Tundra, 0.0, 0.0),
        },
    };
    Biome {
        terrain,
        forest,
        jungle,
    }
}

/// Smooth value noise in 0..1 with lattice spacing `cell` (in plane units).
fn smooth(x: f64, y: f64, cell: f64, salt: u64) -> f64 {
    let (fx, fy) = (x / cell, y / cell);
    let (ix, iy) = (fx.floor(), fy.floor());
    let ease = |t: f64| t * t * (3.0 - 2.0 * t);
    let (tx, ty) = (ease(fx - ix), ease(fy - iy));
    let corner = |dx: i32, dy: i32| hash01(ix as i32 + dx, iy as i32 + dy, salt);
    let top = corner(0, 0) * (1.0 - tx) + corner(1, 0) * tx;
    let bottom = corner(0, 1) * (1.0 - tx) + corner(1, 1) * tx;
    top * (1.0 - ty) + bottom * ty
}

/// Clumpy noise: broad patches with a little tile-level grain. The patches are laid out on the
/// plane, not on the tile grid, so they are as round as the geography they sit on.
fn clumps(x: i32, y: i32, salt: u64) -> f64 {
    let (fx, fy) = tile_plane(x, y);
    0.5 * smooth(fx, fy, 6.0, salt)
        + 0.3 * smooth(fx, fy, 2.8, salt + 1)
        + 0.2 * hash01(x, y, salt + 2)
}

/// Replaces each selected value by its rank in `0..1`, so a threshold of `p` selects exactly
/// the fraction `p` of those tiles, in coherent clumps.
fn ranks(values: &[f64], selected: impl Fn(usize) -> bool) -> Vec<f64> {
    let mut order: Vec<usize> = (0..values.len()).filter(|&i| selected(i)).collect();
    order.sort_by(|&a, &b| values[a].total_cmp(&values[b]));
    let mut out = vec![1.0; values.len()];
    let count = order.len().max(1) as f64;
    for (rank, &i) in order.iter().enumerate() {
        out[i] = rank as f64 / count;
    }
    out
}

/// The value below which a fraction `share` of the selected values fall.
fn quantile(values: &[f64], selected: impl Fn(usize) -> bool, share: f64) -> f64 {
    let mut chosen: Vec<f64> = (0..values.len())
        .filter(|&i| selected(i))
        .map(|i| values[i])
        .collect();
    chosen.sort_by(f64::total_cmp);
    chosen
        .get(((chosen.len() as f64) * share) as usize)
        .or(chosen.last())
        .copied()
        .unwrap_or(f64::MAX)
}

fn tile_stats(elevation: &Elevation, x: i32, y: i32) -> Stats {
    let (west, south, east, north) = tile_bounds(x, y);
    elevation.stats(west, south, east, north)
}

fn touches(map: &[bool], x: i32, y: i32) -> bool {
    (-1..=1).any(|dy| {
        (-1..=1).any(|dx| (dx, dy) != (0, 0) && at(x + dx, y + dy).is_some_and(|i| map[i]))
    })
}

/// Depth classes for every water tile: coast on the shelf and beside land, sea on the slope and
/// in enclosed basins, ocean in the deeps.
fn water_terrain(land: &[bool], stats: &[Stats]) -> Vec<Terrain> {
    let mut component = vec![usize::MAX; land.len()];
    let mut sizes: Vec<usize> = Vec::new();
    for start in 0..land.len() {
        if land[start] || component[start] != usize::MAX || !exists(start) {
            continue;
        }
        let id = sizes.len();
        let mut size = 0;
        let mut queue = VecDeque::from([start]);
        component[start] = id;
        while let Some(i) = queue.pop_front() {
            size += 1;
            let (x, y) = ((i % SIDE) as i32, (i / SIDE) as i32);
            for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
                if let Some(j) = at(x + dx, y + dy)
                    && !land[j]
                    && component[j] == usize::MAX
                {
                    component[j] = id;
                    queue.push_back(j);
                }
            }
        }
        sizes.push(size);
    }
    let ocean = sizes
        .iter()
        .enumerate()
        .max_by_key(|&(_, &size)| size)
        .map(|(id, _)| id);
    (0..land.len())
        .map(|i| {
            if !exists(i) {
                return Terrain::Ocean;
            }
            if land[i] {
                return Terrain::Grass;
            }
            let (x, y) = ((i % SIDE) as i32, (i / SIDE) as i32);
            let depth = -stats[i].mean;
            if touches(land, x, y) {
                Terrain::Coast
            } else if Some(component[i]) != ocean {
                // A lake or an inland sea: its open water is sea, a pond is coast.
                if sizes[component[i]] >= 12 {
                    Terrain::Sea
                } else {
                    Terrain::Coast
                }
            } else if depth < COAST_DEPTH {
                Terrain::Coast
            } else if depth < SEA_DEPTH {
                Terrain::Sea
            } else {
                Terrain::Ocean
            }
        })
        .collect()
}

/// Corner `(cx, cy)` of the tile grid is the point where tiles `(cx-1, cy-1)`, `(cx, cy-1)`,
/// `(cx-1, cy)` and `(cx, cy)` meet. The edge between two adjacent corners separates two tiles:
/// `(owner, other, bit)` where the owner is the tile that stores the river bit.
fn edge_tiles(a: (i32, i32), b: (i32, i32)) -> Option<(usize, usize, u8)> {
    if a.1 == b.1 {
        // Horizontal: north tile above the edge stores it as its south river.
        let x = a.0.min(b.0);
        Some((at(x, a.1 - 1)?, at(x, a.1)?, Tile::RIVER_S))
    } else {
        // Vertical: west tile stores it as its east river.
        let y = a.1.min(b.1);
        Some((at(a.0 - 1, y)?, at(a.0, y)?, Tile::RIVER_E))
    }
}

/// A river between two land tiles, or `None` where it would touch water or leave the map.
fn river_edge(land: &[bool], a: (i32, i32), b: (i32, i32)) -> Option<(usize, u8)> {
    let (owner, other, bit) = edge_tiles(a, b)?;
    (land[owner] && land[other]).then_some((owner, bit))
}

/// Walks a river polyline (in tile space) along tile edges: the sequence of grid corners it
/// passes, each one step from the last.
pub fn snap(line: &[(f64, f64)]) -> Vec<(i32, i32)> {
    let corner = |p: (f64, f64)| (p.0.round() as i32, p.1.round() as i32);
    let mut path: Vec<(i32, i32)> = Vec::new();
    let mut place = |target: (i32, i32), at_point: (f64, f64)| {
        let Some(&last) = path.last() else {
            path.push(target);
            return;
        };
        let mut current = last;
        while current != target {
            let (dx, dy) = (
                (target.0 - current.0).signum(),
                (target.1 - current.1).signum(),
            );
            current = if dx != 0 && dy != 0 {
                // A diagonal move goes through whichever corner lies nearer the river.
                let across = (current.0 + dx, current.1);
                let down = (current.0, current.1 + dy);
                let distance = |c: (i32, i32)| {
                    (f64::from(c.0) - at_point.0).hypot(f64::from(c.1) - at_point.1)
                };
                if distance(across) <= distance(down) {
                    across
                } else {
                    down
                }
            } else {
                (current.0 + dx, current.1 + dy)
            };
            path.push(current);
        }
    };
    for pair in line.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let steps = ((b.0 - a.0).hypot(b.1 - a.1) / 0.2).ceil().max(1.0) as usize;
        for k in 0..=steps {
            let t = k as f64 / steps as f64;
            let point = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
            place(corner(point), point);
        }
    }
    path
}

/// Whether a grid corner touches water or the map edge.
fn wet(land: &[bool], corner: (i32, i32)) -> bool {
    [(-1, -1), (0, -1), (-1, 0), (0, 0)]
        .iter()
        .any(|&(dx, dy)| at(corner.0 + dx, corner.1 + dy).is_none_or(|i| !land[i]))
}

/// Lets a river that stops within a couple of edges of the sea carry on to it.
fn reach_sea(land: &[bool], from: (i32, i32)) -> Vec<(i32, i32)> {
    if wet(land, from) {
        return Vec::new();
    }
    let mut came: std::collections::HashMap<(i32, i32), (i32, i32)> = Default::default();
    let mut queue = VecDeque::from([(from, 0)]);
    while let Some((c, depth)) = queue.pop_front() {
        if depth == 2 {
            continue;
        }
        for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
            let next = (c.0 + dx, c.1 + dy);
            if next == from || came.contains_key(&next) || river_edge(land, c, next).is_none() {
                continue;
            }
            came.insert(next, c);
            if wet(land, next) {
                let mut path = vec![next];
                let mut cursor = next;
                while let Some(&previous) = came.get(&cursor) {
                    path.push(previous);
                    cursor = previous;
                }
                path.pop(); // `from` itself
                path.reverse();
                return path;
            }
            queue.push_back((next, depth + 1));
        }
    }
    Vec::new()
}

/// Lays the Natural Earth rivers onto tile edges. Returns the number of edges.
fn lay_rivers(land: &[bool], rivers: &[River], river: &mut [u8]) -> usize {
    let mut edges = 0;
    let mut put = |land: &[bool], a: (i32, i32), b: (i32, i32), river: &mut [u8]| {
        if let Some((owner, bit)) = river_edge(land, a, b) {
            if river[owner] & bit == 0 {
                edges += 1;
            }
            river[owner] |= bit;
        }
    };
    for source in rivers {
        for line in &source.lines {
            let tiles: Vec<(f64, f64)> = line
                .iter()
                .map(|&(lon, lat)| tile_space(lon, lat))
                .collect();
            let path = snap(&tiles);
            for pair in path.windows(2) {
                put(land, pair[0], pair[1], river);
            }
            for end in [path.first(), path.last()].into_iter().flatten() {
                let extension = reach_sea(land, *end);
                let mut previous = *end;
                for step in extension {
                    put(land, previous, step, river);
                    previous = step;
                }
            }
        }
    }
    edges
}

/// Lowland near a big river's mouth is delta marsh: tiles within `radius` plane units of a mouth.
fn delta_marsh(land: &[bool], rivers: &[River]) -> Vec<(f64, bool)> {
    let mut found: Vec<(f64, f64, f64, f64, i32, i32)> = Vec::new(); // plane x, y, radius, density, tile
    for source in rivers.iter().filter(|r| r.rank <= 4) {
        let (radius, density) = match source.rank {
            1 => (2.4, 0.75),
            2 => (1.9, 0.6),
            _ => (1.5, 0.5),
        };
        for line in &source.lines {
            for end in [line.first(), line.last()].into_iter().flatten() {
                let (u, v) = tile_space(end.0, end.1);
                let (tx, ty) = (u.floor() as i32, v.floor() as i32);
                let near_sea = (-1..=1)
                    .any(|dy| (-1..=1).any(|dx| at(tx + dx, ty + dy).is_none_or(|i| !land[i])));
                if near_sea {
                    let (px, py) = crate::geo::grid_to_plane(u, v);
                    found.push((px, py, radius, density, tx, ty));
                }
            }
        }
    }
    let mut out: Vec<(f64, bool)> = vec![(0.0, false); land.len()];
    for (px, py, radius, density, tx, ty) in found {
        // Every grid cell within reach: a plane radius of 2.4 spans at most 3 cells either way.
        for dy in -4..=4 {
            for dx in -4..=4 {
                let Some(i) = at(tx + dx, ty + dy) else {
                    continue;
                };
                let centre = tile_plane(tx + dx, ty + dy);
                if land[i] && (centre.0 - px).hypot(centre.1 - py) <= radius {
                    let best = out[i].0.max(density);
                    out[i] = (best, true);
                }
            }
        }
    }
    out
}

fn patch_shape(patch: &Patch) -> Result<Vec<Vec<(f64, f64)>>, String> {
    match (&patch.bbox, &patch.poly) {
        (Some([w, s, e, n]), None) => Ok(vec![vec![(*w, *s), (*e, *s), (*e, *n), (*w, *n)]]),
        (None, Some(poly)) if poly.len() >= 3 => {
            Ok(vec![poly.iter().map(|p| (p[0], p[1])).collect()])
        }
        _ => Err(format!(
            "patch {}: give exactly one of bbox or a poly of at least three points",
            patch.name
        )),
    }
}

pub fn classify(
    land: &[bool],
    cities: &HashSet<usize>,
    source: &TerrainSource,
    inputs: &Inputs,
    report: &mut Vec<String>,
) -> Result<Layers, String> {
    let count = land.len();
    let stats: Vec<Stats> = (0..count)
        .map(|i| {
            if exists(i) {
                tile_stats(inputs.elevation, (i % SIDE) as i32, (i / SIDE) as i32)
            } else {
                Stats {
                    mean: 0.0,
                    max: 0.0,
                    min: 0.0,
                }
            }
        })
        .collect();
    let mut terrain = water_terrain(land, &stats);
    let mut relief = vec![Relief::Flat; count];
    let mut cover = vec![Cover::Bare; count];
    let mut river = vec![0u8; count];
    let is_land = |i: usize| land[i];

    // Base terrain and the vegetation each climate would grow.
    let mut biomes = vec![
        Biome {
            terrain: Terrain::Grass,
            forest: 0.0,
            jungle: 0.0,
        };
        count
    ];
    let mut unknown = 0;
    for i in (0..count).filter(|&i| land[i]) {
        let (x, y) = ((i % SIDE) as i32, (i / SIDE) as i32);
        let (lon, lat) = tile_centre(x, y);
        let class = inputs.climate.class(lon, lat);
        unknown += usize::from(class.is_none());
        biomes[i] = biome(class);
        terrain[i] = biomes[i].terrain;
    }
    if unknown > 0 {
        report.push(format!(
            "{unknown} land tiles have no climate class and default to grassland"
        ));
    }

    // Relief: the roughest tiles, by quantile, with absolute floors.
    let roughness: Vec<f64> = stats
        .iter()
        .map(|s| (s.max.max(0.0) - s.min.max(0.0)) + 0.25 * s.mean.max(0.0))
        .collect();
    let mountain_cut = quantile(&roughness, |i| land[i], 1.0 - MOUNTAIN_SHARE);
    let hill_cut = quantile(&roughness, |i| land[i], 1.0 - MOUNTAIN_SHARE - HILL_SHARE);
    for i in (0..count).filter(|&i| land[i]) {
        relief[i] = if roughness[i] >= mountain_cut && stats[i].max >= MOUNTAIN_MIN_PEAK {
            Relief::Mountains
        } else if roughness[i] >= hill_cut && stats[i].max >= HILL_MIN_PEAK {
            Relief::Hills
        } else {
            Relief::Flat
        };
    }
    report.push(format!(
        "relief cut-offs: hills at roughness {hill_cut:.0}, mountains at {mountain_cut:.0}"
    ));

    // Cover: clumpy noise ranked so each climate's coverage is what the table says.
    let noise: Vec<f64> = (0..count)
        .map(|i| clumps((i % SIDE) as i32, (i / SIDE) as i32, 71))
        .collect();
    let rank = ranks(&noise, is_land);
    for i in (0..count).filter(|&i| land[i]) {
        if stats[i].mean >= TREE_LINE || (terrain[i] == Terrain::Desert && biomes[i].forest == 0.0)
        {
            continue;
        }
        let Biome { forest, jungle, .. } = biomes[i];
        let weight = if relief[i] == Relief::Mountains {
            0.6
        } else {
            1.0
        };
        let r = rank[i];
        cover[i] = if r < jungle {
            Cover::Jungle
        } else if r < (forest * weight).max(jungle) && forest > 0.0 {
            Cover::Forest
        } else {
            Cover::Bare
        };
    }

    // Rivers, then the marsh a big river leaves at its mouth.
    let edges = lay_rivers(land, inputs.rivers, &mut river);
    report.push(format!(
        "{edges} river edges from {} rivers",
        inputs.rivers.len()
    ));
    let delta = delta_marsh(land, inputs.rivers);
    for i in (0..count).filter(|&i| land[i]) {
        let (chance, near_mouth) = delta[i];
        let low = stats[i].mean <= 15.0 && stats[i].max <= 80.0;
        if near_mouth
            && low
            && relief[i] == Relief::Flat
            && terrain[i] != Terrain::Desert
            && hash01((i % SIDE) as i32, (i / SIDE) as i32, 311) < chance
        {
            cover[i] = Cover::Marsh;
        }
    }

    // Hand corrections.
    for patch in &source.patches {
        let polygons = patch_shape(patch)?;
        if patch.terrain.is_some_and(|t| t.is_water()) {
            return Err(format!("patch {}: terrain must be land", patch.name));
        }
        let mut hit = 0;
        for i in (0..count).filter(|&i| land[i]) {
            let (x, y) = ((i % SIDE) as i32, (i / SIDE) as i32);
            let (lon, lat) = tile_centre(x, y);
            if !polygons
                .iter()
                .any(|ring| contains(std::slice::from_ref(ring), lon, lat))
                || hash01(x, y, 4000) >= patch.density
            {
                continue;
            }
            hit += 1;
            if let Some(t) = patch.terrain {
                terrain[i] = t;
            }
            if let Some(r) = patch.relief {
                relief[i] = r;
            }
            if let Some(c) = patch.cover {
                // Forest and jungle retain their identity on slopes; marsh needs flat ground.
                let needs_flat = c == Cover::Marsh;
                if !needs_flat || relief[i] == Relief::Flat {
                    cover[i] = c;
                }
            }
        }
        if hit == 0 {
            report.push(format!("warning: patch {} changed no tile", patch.name));
        }
    }

    // Layers that cannot coexist, and the flat ground every city stands on.
    for i in (0..count).filter(|&i| land[i]) {
        if relief[i] != Relief::Flat && cover[i] == Cover::Marsh {
            cover[i] = Cover::Forest;
        }
        if cities.contains(&i) {
            relief[i] = Relief::Flat;
            cover[i] = Cover::Bare;
        }
    }

    let tally =
        |wanted: &dyn Fn(usize) -> bool| (0..count).filter(|&i| exists(i) && wanted(i)).count();
    let total = tally(&|i| land[i]).max(1);
    let share = |n: usize| 100.0 * n as f64 / total as f64;
    report.push(format!(
        "land {total}: grass {:.1}% plains {:.1}% desert {:.1}% tundra {:.1}%; hills {:.1}% mountains {:.1}%; forest {:.1}% jungle {:.1}% marsh {:.1}%",
        share(tally(&|i| land[i] && terrain[i] == Terrain::Grass)),
        share(tally(&|i| land[i] && terrain[i] == Terrain::Plains)),
        share(tally(&|i| land[i] && terrain[i] == Terrain::Desert)),
        share(tally(&|i| land[i] && terrain[i] == Terrain::Tundra)),
        share(tally(&|i| land[i] && relief[i] == Relief::Hills)),
        share(tally(&|i| land[i] && relief[i] == Relief::Mountains)),
        share(tally(&|i| land[i] && cover[i] == Cover::Forest)),
        share(tally(&|i| land[i] && cover[i] == Cover::Jungle)),
        share(tally(&|i| land[i] && cover[i] == Cover::Marsh)),
    ));
    report.push(format!(
        "water: coast {} sea {} ocean {}",
        tally(&|i| terrain[i] == Terrain::Coast),
        tally(&|i| terrain[i] == Terrain::Sea),
        tally(&|i| terrain[i] == Terrain::Ocean),
    ));
    Ok(Layers {
        terrain,
        relief,
        cover,
        river,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(code: &str) -> Option<Koeppen> {
        let mut bytes = [b' '; 3];
        bytes[..code.len()].copy_from_slice(code.as_bytes());
        Koeppen::parse(bytes)
    }

    #[test]
    fn climates_map_to_civ3_terrain() {
        assert_eq!(biome(k("Af")).terrain, Terrain::Grass);
        assert!(biome(k("Af")).jungle > 0.8);
        assert_eq!(biome(k("Aw")).terrain, Terrain::Plains);
        assert_eq!(biome(k("BWh")).terrain, Terrain::Desert);
        assert_eq!(biome(k("BSk")).terrain, Terrain::Plains);
        assert_eq!(biome(k("Cfb")).terrain, Terrain::Grass);
        assert_eq!(biome(k("Csa")).terrain, Terrain::Plains);
        assert_eq!(biome(k("Dfc")).terrain, Terrain::Plains);
        assert!(biome(k("Dfc")).forest > 0.6);
        assert_eq!(biome(k("Dfd")).terrain, Terrain::Tundra);
        assert_eq!(biome(k("ET")).terrain, Terrain::Tundra);
        assert_eq!(biome(k("EF")).forest, 0.0);
    }

    #[test]
    fn a_river_walks_tile_edges_one_step_at_a_time() {
        let path = snap(&[(10.1, 10.2), (14.0, 12.6), (14.2, 16.0)]);
        assert!(path.len() > 8);
        for pair in path.windows(2) {
            let (dx, dy) = ((pair[1].0 - pair[0].0).abs(), (pair[1].1 - pair[0].1).abs());
            assert_eq!(dx + dy, 1, "{pair:?}");
        }
        assert_eq!(path[0], (10, 10));
        assert_eq!(*path.last().unwrap(), (14, 16));
    }

    /// A grid cell well inside the lattice, so the tests do not run into the void.
    const HOME: (i32, i32) = (300, 200);
    fn home(x: i32, y: i32) -> (i32, i32) {
        (HOME.0 + x, HOME.1 + y)
    }
    fn land_at(x: i32, y: i32) -> usize {
        let (x, y) = home(x, y);
        at(x, y).expect("inside the lattice")
    }

    #[test]
    fn river_edges_belong_to_the_tile_that_stores_them() {
        // Along the top of tile (5, 7): the tile above stores it as a south river.
        let (owner, other, bit) = edge_tiles(home(5, 7), home(6, 7)).unwrap();
        assert_eq!(
            (owner, other, bit),
            (land_at(5, 6), land_at(5, 7), Tile::RIVER_S)
        );
        // Down the left of tile (5, 7): the tile to the west stores it as an east river.
        let (owner, other, bit) = edge_tiles(home(5, 8), home(5, 7)).unwrap();
        assert_eq!(
            (owner, other, bit),
            (land_at(4, 7), land_at(5, 7), Tile::RIVER_E)
        );
        // Nothing in the void around the lattice.
        assert!(edge_tiles((0, 0), (1, 0)).is_none());
    }

    #[test]
    fn rivers_only_run_between_land_tiles_and_reach_the_sea() {
        let mut land = vec![false; SIDE * SIDE];
        for y in 10..20 {
            for x in 10..20 {
                land[land_at(x, y)] = true;
            }
        }
        // Down the middle of the island: x = 15.
        let mut river = vec![0u8; land.len()];
        let at_home = |x: f64, y: f64| (x + f64::from(HOME.0), y + f64::from(HOME.1));
        let path = snap(&[at_home(15.0, 12.0), at_home(15.0, 18.0)]);
        for pair in path.windows(2) {
            if let Some((owner, bit)) = river_edge(&land, pair[0], pair[1]) {
                river[owner] |= bit;
            }
        }
        assert!(river.iter().filter(|&&r| r != 0).count() >= 5);
        // Every river bit sits on a land tile whose neighbour across the edge is land too.
        for (i, &bits) in river.iter().enumerate() {
            if bits != 0 {
                assert!(land[i]);
            }
        }
        // From a corner two edges from the shore, the river runs on to the sea.
        let extension = reach_sea(&land, home(15, 11));
        assert_eq!(extension.len(), 1, "{extension:?}");
        assert!(wet(&land, *extension.last().unwrap()));
    }

    #[test]
    fn ranks_select_exact_shares() {
        let values: Vec<f64> = (0..1000).map(|i| hash01(i, 3, 5)).collect();
        let r = ranks(&values, |i| i % 2 == 0);
        let chosen = (0..1000).filter(|&i| i % 2 == 0 && r[i] < 0.3).count();
        assert_eq!(chosen, 150);
        assert!((0..1000).filter(|&i| i % 2 == 1).all(|i| r[i] == 1.0));
        assert!((quantile(&values, |_| true, 0.5) - 0.5).abs() < 0.06);
    }

    #[test]
    fn deep_water_is_ocean_the_shelf_is_coast_and_basins_are_seas() {
        let mut land = vec![false; SIDE * SIDE];
        land[land_at(0, 0)] = true;
        let mut stats = vec![
            Stats {
                mean: -4000.0,
                max: -3900.0,
                min: -4100.0
            };
            land.len()
        ];
        stats[land_at(3, 0)].mean = -120.0; // shelf
        stats[land_at(6, 0)].mean = -1500.0; // slope
        let water = water_terrain(&land, &stats);
        assert_eq!(water[land_at(1, 0)], Terrain::Coast); // beside land
        assert_eq!(water[land_at(3, 0)], Terrain::Coast);
        assert_eq!(water[land_at(6, 0)], Terrain::Sea);
        assert_eq!(water[land_at(-100, 0)], Terrain::Ocean);
        // The void around the lattice is nothing, and does not join the sea to anything.
        assert_eq!(water[0], Terrain::Ocean);
    }
}
