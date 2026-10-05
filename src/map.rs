//! Map model: tiles, procedural generation, iso coordinates, yields.

use bevy::prelude::*;

pub const MAP_W: i32 = 80;
pub const MAP_H: i32 = 60;
pub const MAP_SEED: u64 = 20260928;
pub const TILE_W: f32 = 128.0;
pub const TILE_H: f32 = 64.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Base {
    Ocean,
    Sea,
    Coast,
    Ice,
    Grassland,
    Plains,
    Desert,
    Tundra,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Relief {
    Flat,
    Hill,
    Mountain,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Cover {
    Bare,
    Forest,
    Jungle,
    Pine,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Tile {
    pub base: Base,
    pub relief: Relief,
    pub cover: Cover,
    pub variant: u8,
    pub seen: bool,
    pub visible: bool,
    pub hut: bool,
    pub camp: bool,
    /// Resource GOOD id (0..22, sheet order) or none.
    pub resource: Option<u8>,
    pub road: bool,
    pub irrigation: bool,
    /// Native Cell +4 river mask, indexed by the native ring-1 directions.
    pub river: u8,
    pub mine: bool,
    /// Colony object on the tile (`sites.rs`).
    pub site: Option<crate::sites::Site>,
    /// Native overlay bits 4 and 28, independent of colony objects.
    pub fortress: bool,
    pub barricade: bool,
    /// Native feature bit 0x10000000: this tile has already paid for a forest chop.
    pub forest_harvested: bool,
    /// Civ whose cultural border holds the tile (the cell's owner byte),
    /// written only by `cities::recompute_borders`.
    #[serde(default)]
    pub owner: Option<u8>,
}

impl Tile {
    /// Open grassland or plains: the ground a capital is best founded on.
    pub fn good_start(&self) -> bool {
        matches!(self.base, Base::Grassland | Base::Plains)
            && self.relief == Relief::Flat
            && self.cover == Cover::Bare
    }
}

/// What the world characteristics do to the generator: the grid size and a
/// shift of each threshold. The default is a Standard world with every
/// setting in the middle.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub w: i32,
    pub h: i32,
    /// Spatial frequency of the elevation noise: Archipelago, Continents, Pangaea.
    pub frequency: f32,
    /// Added to the sea-level thresholds (more water when positive).
    pub sea: f32,
    /// Added to the moisture field (Arid below zero, Wet above).
    pub moisture: f32,
    /// Latitude beyond which land is tundra.
    pub tundra: f32,
    /// Latitude below which wet land can be jungle.
    pub jungle: f32,
    /// Added to the hill and mountain thresholds (Young worlds below zero).
    pub relief: f32,
    /// Barbarian activity `S`; below zero the map has no camps.
    pub barbarians: i32,
}

impl Default for Shape {
    fn default() -> Self {
        Shape { w: MAP_W, h: MAP_H, frequency: 0.055, sea: 0.0, moisture: 0.0, tundra: 0.78, jungle: 0.30, relief: 0.0, barbarians: 1 }
    }
}

impl Shape {
    /// The shape of the match's `WCHR` settings (`scenario::Setup`).
    pub fn of(s: &crate::scenario::Setup) -> Shape {
        let o = &s.opts;
        let pick = |v: i32, table: &[f32]| table[v.clamp(0, table.len() as i32 - 1) as usize];
        Shape {
            w: s.w,
            h: s.h,
            frequency: pick(o.landmass, &[0.080, 0.055, 0.040]),
            sea: pick(o.ocean, &[-0.05, 0.0, 0.05, 0.09, 0.13]),
            moisture: pick(o.climate, &[-0.08, 0.0, 0.08]),
            tundra: pick(o.temperature, &[0.66, 0.78, 0.88]),
            jungle: pick(o.temperature, &[0.20, 0.30, 0.40]),
            relief: pick(o.age, &[-0.06, 0.0, 0.06]),
            barbarians: o.barbarians,
        }
    }
}

#[derive(Resource, Clone)]
pub struct GameMap {
    pub w: i32,
    pub h: i32,
    pub tiles: Vec<Tile>,
    pub start: (i32, i32),
    pub seed: u64,
}

impl GameMap {
    pub fn idx(&self, x: i32, y: i32) -> usize {
        (y * self.w + x) as usize
    }

    pub fn wrap_x(&self, x: i32) -> i32 {
        ((x % self.w) + self.w) % self.w
    }

    /// Tiles between two positions: the king-move count, wrapping in x.
    pub fn distance(&self, a: (i32, i32), b: (i32, i32)) -> i32 {
        let dx = (self.wrap_x(a.0) - self.wrap_x(b.0)).abs();
        dx.min(self.w - dx).max((a.1 - b.1).abs())
    }

    pub fn get(&self, x: i32, y: i32) -> Option<&Tile> {
        if y < 0 || y >= self.h {
            return None;
        }
        Some(&self.tiles[self.idx(self.wrap_x(x), y)])
    }

    pub fn neighbors(&self, x: i32, y: i32) -> Vec<(i32, i32)> {
        let mut out = Vec::with_capacity(8);
        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let ny = y + dy;
                if ny >= 0 && ny < self.h {
                    out.push((self.wrap_x(x + dx), ny));
                }
            }
        }
        out
    }

    /// A* path with movement costs, or None when unreachable.
    /// Returns the steps excluding the start tile.
    pub fn find_path(&self, start: (i32, i32), goal: (i32, i32)) -> Option<Vec<(i32, i32)>> {
        self.find_path_by(start, goal, |from, to| self.land_cost(from, to, false))
    }

    /// Normal terrain/road cost; bridge technology preserves the road
    /// discount across a river. Unit legality is checked by `entry_cost`.
    pub fn land_cost(&self, from: (i32, i32), to: (i32, i32), bridges: bool) -> Option<u8> {
        let origin = self.get(from.0, from.1)?;
        let destination = self.get(to.0, to.1)?;
        Some(civ3mapgen::movement::land_step_cost(move_cost(destination)?,
            origin.road, destination.road, crate::rivers::crossed(self, from, to), bridges))
    }

    /// The same path search with domain-specific entry costs.
    pub fn find_path_by(&self, start: (i32, i32), goal: (i32, i32),
        cost: impl Fn((i32, i32), (i32, i32)) -> Option<u8>) -> Option<Vec<(i32, i32)>>
    {
        if start == goal {
            return Some(vec![]);
        }
        cost(start, goal)?;
        use std::cmp::Reverse;
        use std::collections::{BinaryHeap, HashMap};
        let w = self.w;
        // Admissible: a road step costs one third, the cheapest move.
        let h = |(x, y): (i32, i32)| {
            let dx = (x - goal.0).abs();
            let dx = dx.min(w - dx);
            dx.max((y - goal.1).abs()) as u32
        };
        let mut open = BinaryHeap::new();
        let mut gscore = HashMap::new();
        let mut came: HashMap<(i32, i32), (i32, i32)> = HashMap::new();
        gscore.insert(start, 0u32);
        open.push((Reverse(h(start)), start));
        while let Some((_, cur)) = open.pop() {
            if cur == goal {
                let mut path = vec![];
                let mut c = cur;
                while c != start {
                    path.push(c);
                    c = came[&c];
                }
                path.reverse();
                return Some(path);
            }
            let g = gscore[&cur];
            for nb in self.neighbors(cur.0, cur.1) {
                let Some(cost) = cost(cur, nb) else {
                    continue;
                };
                let ng = g + cost as u32;
                if ng < *gscore.get(&nb).unwrap_or(&u32::MAX) {
                    gscore.insert(nb, ng);
                    came.insert(nb, cur);
                    open.push((Reverse(ng + h(nb)), nb));
                }
            }
        }
        None
    }

    pub fn is_land(&self, x: i32, y: i32) -> bool {
        matches!(
            self.get(x, y).map(|t| t.base),
            Some(
                Base::Grassland | Base::Plains | Base::Desert | Base::Tundra
            )
        )
    }

    /// Native water regions join through shared edges (0x5EB92D), which
    /// are orthogonal neighbors in this square-grid representation. Stop
    /// above the lake threshold; the callers only need small versus large.
    pub fn water_body_size(&self, start: (i32, i32)) -> usize {
        use std::collections::{HashSet, VecDeque};
        let water = |p: (i32, i32)| self.get(p.0, p.1).is_some_and(|t|
            matches!(t.base, Base::Coast | Base::Sea | Base::Ocean));
        if !water(start) { return 0; }
        let start = (self.wrap_x(start.0), start.1);
        let mut seen = HashSet::from([start]);
        let mut queue = VecDeque::from([start]);
        while let Some((x, y)) = queue.pop_front() {
            for (dx, dy) in [(1, 0), (0, 1), (-1, 0), (0, -1)] {
                let nb = (self.wrap_x(x + dx), y + dy);
                if water(nb) && seen.insert(nb) {
                    if seen.len() > civ3mapgen::lakes::LAKE_MAX { return seen.len(); }
                    queue.push_back(nb);
                }
            }
        }
        seen.len()
    }

    /// 0x5F38C0: water bodies of at most 20 tiles supply fresh water in
    /// the center-inclusive 3x3 neighborhood; a river on this tile also supplies water.
    pub fn fresh_water(&self, x: i32, y: i32) -> bool {
        self.get(x, y).is_some_and(|t| t.river != 0)
            || std::iter::once((x, y)).chain(self.neighbors(x, y)).any(|p| {
            let size = self.water_body_size(p);
            size > 0 && size <= civ3mapgen::lakes::LAKE_MAX
        })
    }

    /// Shipyards need an adjacent water body larger than 20 tiles (0x4C05BB).
    pub fn coastal_site(&self, x: i32, y: i32) -> bool {
        self.neighbors(x, y).into_iter().any(|p|
            self.water_body_size(p) > civ3mapgen::lakes::LAKE_MAX)
    }

    /// The match's map: the file's own when it has one, else a random one
    /// from the match's size and world characteristics.
    pub fn generate() -> Self {
        if let Some(map) = crate::scenario::file_map() {
            return map.clone();
        }
        let s = crate::scenario::setup();
        Self::generate_with(s.seed, &Shape::of(s))
    }

    pub fn generate_with_seed(seed: u64) -> Self {
        Self::generate_with(seed, &Shape::default())
    }

    pub fn generate_with(seed: u64, shape: &Shape) -> Self {
        let w = shape.w;
        let h = shape.h;
        let mut tiles = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            let lat = ((y as f32 - (h - 1) as f32 / 2.0).abs() * 2.0) / (h - 1) as f32;
            for x in 0..w {
                // sample noise on a horizontal cylinder so x wraps seamlessly
                let t = x as f32 / w as f32;
                let blend = |s: u64, f: f32| {
                    fbm(x as f32 * f, y as f32 * f, s) * (1.0 - t)
                        + fbm((x - w) as f32 * f, y as f32 * f, s) * t
                };
                let e = blend(seed, shape.frequency);
                let m = blend(seed + 101, 0.07) + shape.moisture;
                let r = blend(seed + 202, 0.09);

                let mut base = if e < 0.36 + shape.sea {
                    Base::Ocean
                } else if e < 0.42 + shape.sea {
                    Base::Sea
                } else if e < 0.46 + shape.sea {
                    Base::Coast
                } else if lat > shape.tundra {
                    Base::Tundra
                } else if (0.15..0.5).contains(&lat) && m < 0.45 || m < 0.30 {
                    Base::Desert
                } else if m < 0.55 {
                    Base::Plains
                } else {
                    Base::Grassland
                };
                let water = matches!(base, Base::Ocean | Base::Sea | Base::Coast);
                if water && lat > 0.92 || !water && lat > 0.95 {
                    base = Base::Ice;
                }

                let land = matches!(
                    base,
                    Base::Grassland | Base::Plains | Base::Desert | Base::Tundra
                );
                let mut relief = Relief::Flat;
                if land {
                    if e > 0.74 + shape.relief && r > 0.5 {
                        relief = Relief::Mountain;
                    } else if e > 0.64 + shape.relief || r > 0.66 + shape.relief {
                        relief = Relief::Hill;
                    }
                }
                let mut cover = Cover::Bare;
                if relief == Relief::Flat {
                    if base == Base::Tundra {
                        if m > 0.55 && hash2(x as i64, y as i64, seed) < 0.5 {
                            cover = Cover::Pine;
                        }
                    } else if land {
                        if lat < shape.jungle && m > 0.68 {
                            cover = Cover::Jungle;
                        } else if m > 0.60 {
                            cover = Cover::Forest;
                        }
                    }
                }
                let variant = (hash2(x as i64, y as i64, seed + 7) * 100.0) as u8 % 3;
                tiles.push(Tile {
                    base,
                    relief,
                    cover,
                    variant,
                    seen: false,
                    visible: false,
                    hut: false,
                    camp: false,
                    resource: None,
                    road: false,
                    irrigation: false,
            river: 0,
                    mine: false,
                    site: None,
            fortress: false,
            barricade: false,
            forest_harvested: false,
                    owner: None,
                });
            }
        }
        let mut map = Self {
            w,
            h,
            tiles,
            start: (w / 2, h / 2),
            seed,
        };
        map.coast_shores();
        crate::rivers::generate(&mut map);
        map.start = map.pick_start();
        // RE stages 10-12: resources, goody huts, barbarian camps. Stage
        // seeds derive from the water level (0..100 Oceans-slider semantics)
        // so MAP_SEED still varies them.
        let wl = (seed % 101) as u32;
        crate::features::place_resources(&mut map, wl);
        crate::features::place_goody_huts(&mut map, wl);
        if shape.barbarians >= 0 {
            crate::features::place_barbarian_camps(&mut map, wl);
        }
        map
    }

    /// Any water touching land, diagonals included, is coast, as on every
    /// Civ3 map. The terrain art depends on it: a cell with land on a
    /// vertex comes from a land sheet, whose only water is coast, so a sea
    /// or ocean tile beside land would meet its all-water neighbor cells
    /// with a hard coast/sea seam.
    pub(crate) fn coast_shores(&mut self) {
        let shore: Vec<usize> = (0..self.h)
            .flat_map(|y| (0..self.w).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                matches!(self.tiles[self.idx(x, y)].base, Base::Sea | Base::Ocean)
                    && self.neighbors(x, y).iter().any(|&(nx, ny)| self.is_land(nx, ny))
            })
            .map(|(x, y)| self.idx(x, y))
            .collect();
        for i in shore {
            self.tiles[i].base = Base::Coast;
        }
    }

    /// Best grassland or plains capital site: most land nearby, near center.
    fn pick_start(&self) -> (i32, i32) {
        let mut best = (self.w / 2, self.h / 2);
        let mut best_score = f32::MIN;
        for y in 0..self.h {
            for x in 0..self.w {
                if !self.tiles[self.idx(x, y)].good_start() {
                    continue;
                }
                let mut land = 0;
                for dy in -2..=2 {
                    for dx in -2..=2 {
                        if self.is_land(x + dx, y + dy) {
                            land += 1;
                        }
                    }
                }
                let dcx = (x - self.w / 2) as f32;
                let dcy = (y - self.h / 2) as f32;
                let score = land as f32 - (dcx * dcx + dcy * dcy).sqrt() * 0.05;
                if score > best_score {
                    best_score = score;
                    best = (x, y);
                }
            }
        }
        best
    }
}

/// TERR row of the tile's effective terrain, with relief taking precedence.
pub(crate) fn terrain_row(t: &Tile) -> usize {
    match (t.relief, t.cover, t.base) {
        (Relief::Mountain, _, _) => 6,
        (Relief::Hill, _, _) => 5,
        (_, Cover::Forest | Cover::Pine, _) => 7,
        (_, Cover::Jungle, _) => 8,
        (_, _, Base::Desert) if t.river != 0 => 4,
        (_, _, Base::Desert) => 0,
        (_, _, Base::Plains) => 1,
        (_, _, Base::Grassland) => 2,
        (_, _, Base::Tundra | Base::Ice) => 3,
        (_, _, Base::Coast) => 11,
        (_, _, Base::Sea) => 12,
        (_, _, Base::Ocean) => 13,
    }
}

/// Food and shields from shipped TERR values, improvements and GOOD bonuses.
pub fn yields(t: &Tile) -> (u8, u8) {
    if t.base == Base::Ice { return (0, 0); }
    let terrain = &crate::ruleset::TERRAINS[terrain_row(t)];
    let (df, ds) = t.resource.map_or((0, 0), |id| crate::features::GOODS[id as usize].bonus);
    (terrain.food + df + if t.irrigation { terrain.irrigation } else { 0 },
     terrain.shields + ds + if t.mine { terrain.mining } else { 0 })
}

/// Movement points are counted in thirds so roads can cost 1/3 MP.
pub const MP: u8 = 3;

/// Terrain cost to enter in whole MP, or None when impassable. Roads are
/// handled by `GameMap::land_cost`, which needs both ends of the step.
pub fn move_cost(t: &Tile) -> Option<u8> {
    match t.base {
        Base::Ocean | Base::Sea | Base::Coast | Base::Ice => None,
        _ => Some(crate::ruleset::TERRAINS[terrain_row(t)].movement),
    }
}

pub fn tile_to_world(x: i32, y: i32) -> Vec2 {
    Vec2::new(
        (x - y) as f32 * TILE_W / 2.0,
        -((x + y) as f32) * TILE_H / 2.0,
    )
}

pub fn world_to_tile(map: &GameMap, p: Vec2) -> Option<(i32, i32)> {
    let s = -p.y / (TILE_H / 2.0);
    let d = p.x / (TILE_W / 2.0);
    let fx = (s + d) / 2.0;
    let fy = (s - d) / 2.0;
    let period = map.w as f32 * TILE_W / 2.0;
    let mut cands = vec![
        (fx.floor() as i32, fy.floor() as i32),
        (fx.floor() as i32, fy.ceil() as i32),
        (fx.ceil() as i32, fy.floor() as i32),
        (fx.ceil() as i32, fy.ceil() as i32),
    ];
    cands.sort();
    cands.dedup();
    for (cx, cy) in cands {
        if cy < 0 || cy >= map.h {
            continue;
        }
        let wx = map.wrap_x(cx);
        let mut c = tile_to_world(wx, cy);
        c.x += ((p.x - c.x) / period).round() * period;
        if (p.x - c.x).abs() / (TILE_W / 2.0) + (p.y - c.y).abs() / (TILE_H / 2.0)
            <= 1.0 + 1e-3
        {
            return Some((wx, cy));
        }
    }
    None
}

fn hash2(x: i64, y: i64, seed: u64) -> f32 {
    let mut h =
        x.wrapping_mul(374761393).wrapping_add(y.wrapping_mul(668265263)) as u64
            ^ seed.wrapping_mul(974634211);
    h = h.wrapping_mul(1274126177);
    h ^= h >> 16;
    ((h & 0xffff) as f32) / 65535.0
}

fn vnoise(x: f32, y: f32, seed: u64) -> f32 {
    let xi = x.floor() as i64;
    let yi = y.floor() as i64;
    let xf = x - x.floor();
    let yf = y - y.floor();
    let u = xf * xf * (3.0 - 2.0 * xf);
    let v = yf * yf * (3.0 - 2.0 * yf);
    let a = hash2(xi, yi, seed);
    let b = hash2(xi + 1, yi, seed);
    let c = hash2(xi, yi + 1, seed);
    let d = hash2(xi + 1, yi + 1, seed);
    a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v
}

fn fbm(x: f32, y: f32, seed: u64) -> f32 {
    let mut v = 0.0;
    let mut amp = 0.5;
    let (mut fx, mut fy) = (x, y);
    for _ in 0..4 {
        v += amp * vnoise(fx, fy, seed);
        fx *= 2.03;
        fy *= 2.01;
        amp *= 0.5;
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(base: Base, relief: Relief, cover: Cover) -> Tile {
        Tile {
            base,
            relief,
            cover,
            variant: 0,
            seen: false,
            visible: false,
            hut: false,
            camp: false,
            resource: None,
            road: false,
            irrigation: false,
            river: 0,
            mine: false,
            site: None,
            fortress: false,
            barricade: false,
            forest_harvested: false,
            owner: None,
        }
    }

    #[test]
    fn yields_table() {
        let flat = Relief::Flat;
        let bare = Cover::Bare;
        assert_eq!(yields(&tile(Base::Grassland, flat, bare)), (2, 0));
        assert_eq!(yields(&tile(Base::Plains, flat, bare)), (1, 1));
        assert_eq!(yields(&tile(Base::Desert, flat, bare)), (0, 1));
        assert_eq!(yields(&tile(Base::Tundra, flat, bare)), (1, 0));
        assert_eq!(yields(&tile(Base::Coast, flat, bare)), (1, 0));
        assert_eq!(yields(&tile(Base::Ocean, flat, bare)), (0, 0));
        assert_eq!(yields(&tile(Base::Grassland, flat, Cover::Jungle)), (1, 0));
        assert_eq!(yields(&tile(Base::Tundra, flat, Cover::Pine)), (1, 2));
        assert_eq!(
            yields(&tile(Base::Grassland, flat, Cover::Forest)),
            (1, 2)
        );
        assert_eq!(
            yields(&tile(Base::Grassland, Relief::Hill, bare)),
            (1, 1)
        );
        assert_eq!(
            yields(&tile(Base::Grassland, Relief::Mountain, bare)),
            (0, 1)
        );
    }

    #[test]
    fn resource_bonus_adds_to_base_yield() {
        let mut t = tile(Base::Grassland, Relief::Flat, Cover::Bare);
        t.resource = Some(20); // wheat +2 food
        assert_eq!(yields(&t), (4, 0));
        let mut t = tile(Base::Sea, Relief::Flat, Cover::Bare);
        t.resource = Some(16); // whales +1/+1
        assert_eq!(yields(&t), (2, 1));
        let mut t = tile(Base::Grassland, Relief::Flat, Cover::Bare);
        t.resource = Some(0); // horses: no yield yet
        assert_eq!(yields(&t), (2, 0));
    }

    #[test]
    fn irrigation_adds_one_food() {
        let mut t = tile(Base::Plains, Relief::Flat, Cover::Bare);
        assert_eq!(yields(&t), (1, 1));
        t.irrigation = true;
        assert_eq!(yields(&t), (2, 1));
    }

    #[test]
    fn movement_costs() {
        let flat = Relief::Flat;
        let bare = Cover::Bare;
        assert_eq!(move_cost(&tile(Base::Grassland, flat, bare)), Some(1));
        assert_eq!(move_cost(&tile(Base::Ocean, flat, bare)), None);
        assert_eq!(move_cost(&tile(Base::Ice, flat, bare)), None);
        assert_eq!(
            move_cost(&tile(Base::Grassland, Relief::Mountain, bare)),
            Some(3)
        );
        assert_eq!(
            move_cost(&tile(Base::Grassland, Relief::Hill, bare)),
            Some(2)
        );
        assert_eq!(
            move_cost(&tile(Base::Grassland, flat, Cover::Forest)),
            Some(2)
        );
    }

    #[test]
    fn water_beside_land_is_coast() {
        for seed in [1, 2, 7] {
            let map = GameMap::generate_with_seed(seed);
            for y in 0..map.h {
                for x in 0..map.w {
                    let t = &map.tiles[map.idx(x, y)];
                    if matches!(t.base, Base::Sea | Base::Ocean) {
                        assert!(
                            !map.neighbors(x, y).iter().any(|&(nx, ny)| map.is_land(nx, ny)),
                            "seed {seed}: {:?} at ({x},{y}) touches land",
                            t.base
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn neighbors_wrap_x_not_y() {
        let map = GameMap::generate();
        let nbs = map.neighbors(0, 10);
        assert!(nbs.contains(&(map.w - 1, 10)));
        assert!(nbs.contains(&(1, 10)));
        let top = map.neighbors(10, 0);
        assert!(!top.iter().any(|(_, y)| *y < 0));
    }

    #[test]
    fn distance_counts_king_moves_and_wraps_in_x_only() {
        let map = GameMap::generate();
        assert_eq!(map.distance((0, 10), (map.w - 3, 12)), 3);
        assert_eq!(map.distance((0, 0), (0, 9)), 9);
        // A position past the edge wraps like the tile it stands for.
        assert_eq!(map.distance((-2, 5), (2, 5)), 4);
    }

    #[test]
    fn paths_avoid_impassable() {
        let map = GameMap::generate();
        let (sx, sy) = map.start;
        assert_eq!(map.find_path((sx, sy), (sx, sy)).unwrap(), vec![]);
        let land = map
            .neighbors(sx, sy)
            .into_iter()
            .find(|(x, y)| {
                move_cost(map.get(*x, *y).unwrap()).is_some()
            })
            .expect("start has a passable neighbor");
        assert_eq!(map.find_path((sx, sy), land).unwrap(), vec![land]);
        let ocean = (0..map.h)
            .flat_map(|y| (0..map.w).map(move |x| (x, y)))
            .find(|(x, y)| {
                matches!(map.get(*x, *y).map(|t| t.base), Some(Base::Ocean))
            })
            .expect("map has ocean");
        assert_eq!(map.find_path((sx, sy), ocean), None);
    }

    #[test]
    fn picking_roundtrips() {
        let map = GameMap::generate();
        for (x, y) in [(0, 0), (37, 32), (79, 59), (10, 50)] {
            let p = tile_to_world(x, y);
            assert_eq!(world_to_tile(&map, p), Some((x, y)));
        }
        assert_eq!(world_to_tile(&map, Vec2::new(1e6, 1e6)), None);
    }
}

#[cfg(test)]
mod probe_tests {
    use super::*;
    #[test]
    fn probe_biome_locations() {
        let map = GameMap::generate();
        println!("start={:?} seed={}", map.start, map.seed);
        let mut show: Vec<(&str, i32, i32)> = vec![];
        for y in (0..map.h).step_by(4) {
            for x in (0..map.w).step_by(4) {
                let t = &map.tiles[map.idx(x, y)];
                let tag = match (t.base, t.relief) {
                    (Base::Ocean, _) => "ocean",
                    (Base::Ice, _) => "ice",
                    (Base::Tundra, _) => "tundra",
                    (_, Relief::Mountain) => "mtn",
                    (_, Relief::Hill) => "hill",
                    (Base::Desert, _) => "desert",
                    _ => continue,
                };
                if show.iter().filter(|(t, _, _)| *t == tag).count() < 3 {
                    show.push((tag, x, y));
                }
            }
        }
        for (tag, x, y) in &show {
            let w = tile_to_world(*x, *y);
            println!("{tag} tile=({x},{y}) world=({:.0},{:.0})", w.x, w.y);
        }
        let s = tile_to_world(map.start.0, map.start.1);
        println!("start world=({:.0},{:.0})", s.x, s.y);
        let (mut hills, mut mtns) = (0, 0);
        for t in &map.tiles {
            match t.relief { Relief::Hill => hills += 1, Relief::Mountain => mtns += 1, _ => {} }
        }
        println!("relief hills={hills} mountains={mtns}");
        let mut lone = 0;
        let mut lone_by_base = std::collections::HashMap::new();
        for y in 0..map.h { for x in 0..map.w {
            let b = map.tiles[map.idx(x, y)].base;
            let same = map.neighbors(x, y).iter()
                .filter(|(nx, ny)| map.tiles[map.idx(*nx, *ny)].base == b).count();
            if same == 0 { lone += 1; *lone_by_base.entry(format!("{b:?}")).or_insert(0) += 1; }
        }}
        println!("lone tiles (no same-base neighbor): {lone} {lone_by_base:?}");
        let mut first_mtn = None;
        for y in 0..map.h { for x in 0..map.w {
            if map.tiles[map.idx(x, y)].relief == Relief::Mountain && first_mtn.is_none() {
                first_mtn = Some((x, y));
            }
        }}
        if let Some((x, y)) = first_mtn {
            let w = tile_to_world(x, y);
            println!("first_mtn tile=({x},{y}) world=({:.0},{:.0})", w.x, w.y);
        }
    }
}
