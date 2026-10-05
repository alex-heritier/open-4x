//! The trade network (`trade-network.md`): which cities a civilization's
//! roads, harbors and airports join. Corruption reads it for the "connected
//! to the capital" test (`0x57F0A0`, `economy.md`).
//!
//! For civilization `p`, pass 0 floods road components from every city tile
//! in city order; a tile carries `p`'s road when it has a road (a city tile
//! counts, **H** as in the specification) and its owner is not at war with
//! `p` (`0x5DA060`). Two cities on one component are linked. Pass 1 links
//! cities that both hold a non-obsolete air-trade improvement whose far
//! tile `p` has seen; pass 2 links cities that both hold a water-trade
//! improvement over a sea route, owners at peace. Links merge transitively
//! (`0x57D840`).
//!
//! HYPOTHESIS: the sea route stands in for path finder mode 9 (`0x580540`,
//! open in `movement.md`): water tiles connected by king moves, Coast always,
//! Sea once `p` knows a Trade-over-Sea advance and Ocean a Trade-over-Ocean
//! advance, starting from water next to each city. Seen tiles are taken as
//! explored ones.

use std::collections::{HashMap, VecDeque};

use crate::map::{Base, GameMap};

/// BLDG `improvement_flags` bits (`trade-network.md` 2.2).
pub const WATER_TRADE: u32 = 1 << 20;
pub const AIR_TRADE: u32 = 1 << 21;
/// TECH flags (`research.md` 1.1).
pub const TRADE_OVER_SEA: u32 = 1 << 13;
pub const TRADE_OVER_OCEAN: u32 = 1 << 14;

/// One city as the network sees it.
#[derive(Clone, Copy, Debug)]
pub struct Node {
    pub at: (i32, i32),
    pub owner: usize,
    pub water: bool,
    pub air: bool,
}

/// What `p` may cross and see.
pub struct View<'a> {
    pub p: usize,
    pub at_war: &'a dyn Fn(usize, usize) -> bool,
    pub sea: bool,
    pub ocean: bool,
    pub seen: &'a dyn Fn(i32, i32) -> bool,
}

/// `p`'s network: the component of every city (two cities are connected
/// exactly when their entries are equal) and, for every tile carrying `p`'s
/// road, the index of the city whose fill labelled it (`cell +0x6E + 2p`).
pub struct Network {
    pub comp: Vec<usize>,
    pub label: HashMap<(i32, i32), usize>,
}

impl Network {
    /// The cities a tile's road component reaches (`trade-network.md` 7.1
    /// step 1): every city connected to the city that labelled it.
    pub fn reaches(&self, tile: (i32, i32)) -> Option<usize> {
        self.label.get(&tile).map(|&l| self.comp[l])
    }
}

/// Build `p`'s network.
pub fn components(map: &GameMap, nodes: &[Node], v: &View) -> Network {
    let city_at: HashMap<(i32, i32), usize> = nodes.iter().enumerate().map(|(i, n)| (n.at, i)).collect();
    let road_for = |x: i32, y: i32| {
        let Some(t) = map.get(x, y) else { return false };
        (t.road || city_at.contains_key(&(x, y)))
            && t.owner.is_none_or(|o| o as usize == v.p || !(v.at_war)(v.p, o as usize))
    };
    // Pass 0: road labels, the first city in order names its component.
    let mut label: HashMap<(i32, i32), usize> = HashMap::new();
    for (i, n) in nodes.iter().enumerate() {
        if label.contains_key(&n.at) || !road_for(n.at.0, n.at.1) {
            continue;
        }
        let mut queue = VecDeque::from([n.at]);
        label.insert(n.at, i);
        while let Some((x, y)) = queue.pop_front() {
            for (nx, ny) in map.neighbors(x, y) {
                if !label.contains_key(&(nx, ny)) && road_for(nx, ny) {
                    label.insert((nx, ny), i);
                    queue.push_back((nx, ny));
                }
            }
        }
    }
    let mut parent: Vec<usize> = (0..nodes.len()).collect();
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let union = |parent: &mut Vec<usize>, a: usize, b: usize| {
        let (ra, rb) = (find(parent, a), find(parent, b));
        if ra != rb {
            parent[ra.max(rb)] = ra.min(rb);
        }
    };
    for (i, n) in nodes.iter().enumerate() {
        if let Some(&l) = label.get(&n.at) {
            union(&mut parent, i, l);
        }
    }
    // Pass 1: air.
    for i in 0..nodes.len() {
        for j in i + 1..nodes.len() {
            let (a, b) = (&nodes[i], &nodes[j]);
            if a.air && b.air && !(v.at_war)(a.owner, b.owner) && (v.seen)(b.at.0, b.at.1) {
                union(&mut parent, i, j);
            }
        }
    }
    // Pass 2: water.
    let sea = water_bodies(map, v);
    let ports: Vec<Vec<u32>> = nodes
        .iter()
        .map(|n| {
            let mut ids: Vec<u32> = map.neighbors(n.at.0, n.at.1).into_iter().filter_map(|(x, y)| sea.get(&(x, y)).copied()).collect();
            ids.sort_unstable();
            ids.dedup();
            ids
        })
        .collect();
    for i in 0..nodes.len() {
        for j in i + 1..nodes.len() {
            let (a, b) = (&nodes[i], &nodes[j]);
            if a.water && b.water && !(v.at_war)(a.owner, b.owner) && ports[i].iter().any(|w| ports[j].contains(w)) {
                union(&mut parent, i, j);
            }
        }
    }
    let comp = (0..nodes.len()).map(|i| find(&mut parent, i)).collect();
    Network { comp, label }
}

/// Water bodies `p` can sail for trade, labelled by king-move flood.
fn water_bodies(map: &GameMap, v: &View) -> HashMap<(i32, i32), u32> {
    let sailable = |x: i32, y: i32| {
        map.get(x, y).is_some_and(|t| match t.base {
            Base::Coast => true,
            Base::Sea => v.sea,
            Base::Ocean => v.ocean,
            _ => false,
        })
    };
    let mut out = HashMap::new();
    let mut next = 0;
    for y in 0..map.h {
        for x in 0..map.w {
            if out.contains_key(&(x, y)) || !sailable(x, y) {
                continue;
            }
            out.insert((x, y), next);
            let mut queue = VecDeque::from([(x, y)]);
            while let Some((cx, cy)) = queue.pop_front() {
                for (nx, ny) in map.neighbors(cx, cy) {
                    if !out.contains_key(&(nx, ny)) && sailable(nx, ny) {
                        out.insert((nx, ny), next);
                        queue.push_back((nx, ny));
                    }
                }
            }
            next += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{Base, GameMap};

    fn land() -> GameMap {
        let mut map = GameMap::generate();
        for t in map.tiles.iter_mut() {
            t.base = Base::Grassland;
            t.road = false;
            t.owner = None;
        }
        map
    }

    fn node(at: (i32, i32)) -> Node {
        Node { at, owner: 0, water: false, air: false }
    }

    fn view<'a>(war: &'a dyn Fn(usize, usize) -> bool) -> View<'a> {
        View { p: 0, at_war: war, sea: false, ocean: false, seen: &|_, _| true }
    }

    #[test]
    fn roads_join_cities_and_enemy_territory_cuts_them() {
        let mut map = land();
        for x in 11..15 {
            let i = map.idx(x, 10);
            map.tiles[i].road = true;
        }
        let nodes = [node((10, 10)), node((15, 10)), node((20, 10))];
        let peace = |_: usize, _: usize| false;
        let c = components(&map, &nodes, &view(&peace)).comp;
        assert_eq!(c[0], c[1]);
        assert_ne!(c[0], c[2]);
        let i = map.idx(12, 10);
        map.tiles[i].owner = Some(1);
        let war = |a: usize, b: usize| a != b;
        let c = components(&map, &nodes, &view(&war)).comp;
        assert_ne!(c[0], c[1], "a road through enemy land is a wall");
        let c = components(&map, &nodes, &view(&peace)).comp;
        assert_eq!(c[0], c[1], "a neutral's road still links");
    }

    #[test]
    fn harbors_link_across_a_shared_coast_but_not_open_sea_in_the_ancient_age() {
        let mut map = land();
        for x in 11..20 {
            let i = map.idx(x, 10);
            map.tiles[i].base = if x == 15 { Base::Sea } else { Base::Coast };
        }
        let mut a = node((10, 10));
        let mut b = node((20, 10));
        a.water = true;
        b.water = true;
        let peace = |_: usize, _: usize| false;
        let c = components(&map, &[a, b], &view(&peace)).comp;
        assert_ne!(c[0], c[1], "the Sea tile needs a Trade-over-Sea advance");
        let mut v = view(&peace);
        v.sea = true;
        let c = components(&map, &[a, b], &v).comp;
        assert_eq!(c[0], c[1]);
        b.water = false;
        let c = components(&map, &[a, b], &v).comp;
        assert_ne!(c[0], c[1], "both cities need the improvement");
    }
}
