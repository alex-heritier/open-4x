//! Playable-map adapter for the recovered generator in `rivergen.rs`.
//! The square cylinder is rotated into a temporary native diamond grid.
//! Wrapped copies protect the playable area from the temporary coast;
//! folding the copies back merges river edges at the horizontal seam.
//! This preserves native growth rules, not native map topology or density.

use crate::map::{Base, GameMap};

pub const NORTH: u8 = 0x02;
pub const EAST: u8 = 0x08;
pub const SOUTH: u8 = 0x20;
pub const WEST: u8 = 0x80;
const EDGES: [(i32, i32, u8, u8); 4] = [
    (0, -1, NORTH, SOUTH),
    (1, 0, EAST, WEST),
    (0, 1, SOUTH, NORTH),
    (-1, 0, WEST, EAST),
];

pub fn generate(map: &mut GameMap) {
    use civ3_worldgen::{cell::MapGrid, continents::number_continents, rivergen::grow_rivers};
    // The native chain limit is 20. Copies cover its neighborhood at either seam.
    let pad = 24;
    let side = (map.w + map.h + 2 * pad + 1) & !1;
    let mut grid = MapGrid::new(side, side, 0, map.seed as i32);
    for cell in &mut grid.cells {
        cell.set_class(13);
    }
    let pos = |x: i32, y: i32| (x - y + map.h + pad, x + y + pad);
    for y in 0..map.h {
        for x in -pad..map.w + pad {
            let tile = map.get(x, y).unwrap();
            let (nx, ny) = pos(x, y);
            let row = if tile.base == Base::Ice {
                13
            } else {
                crate::map::terrain_row(tile) as u8
            };
            grid.cell_at_mut(nx, ny).unwrap().set_class(row);
        }
    }
    number_continents(&mut grid);
    grow_rivers(&mut grid, map.seed as i32);
    for y in 0..map.h {
        for x in 0..map.w {
            let (nx, ny) = pos(x, y);
            let mask = grid.cell_at(nx, ny).unwrap().river & 0xAA;
            let i = map.idx(x, y);
            map.tiles[i].river = mask;
        }
    }
    // Each shared edge is present on both tiles, including the wrapped seam.
    for y in 0..map.h {
        for x in 0..map.w {
            let i = map.idx(x, y);
            let mask = map.tiles[i].river;
            for (dx, dy, bit, opposite) in EDGES {
                if mask & bit == 0 {
                    continue;
                }
                if map.get(x + dx, y + dy).is_some() {
                    let j = map.idx(map.wrap_x(x + dx), y + dy);
                    map.tiles[j].river |= opposite;
                } else {
                    map.tiles[i].river &= !bit;
                }
            }
        }
    }
    // 0x5F1240 / generator pass C: the two edges surrounding a diagonal step.
    let diagonals = [
        (-1, -1, WEST, NORTH, 0),
        (1, -1, EAST, NORTH, 2),
        (1, 1, EAST, SOUTH, 4),
        (-1, 1, WEST, SOUTH, 6),
    ];
    for y in 0..map.h {
        for x in 0..map.w {
            let i = map.idx(x, y);
            let mask = map.tiles[i].river;
            for (dx, dy, a, b, direction) in diagonals {
                let first = mask & a != 0 || map.get(x + dx, y).is_some_and(|t| t.river & b != 0);
                let second = mask & b != 0 || map.get(x, y + dy).is_some_and(|t| t.river & a != 0);
                if first && second {
                    map.tiles[i].river |= 1 << direction;
                }
            }
        }
    }
}

/// River branches leave a blended corner toward NW, NE, SW and SE.
/// Art addressing is based on the shipped 4x4 sheet, not the withdrawn
/// owner/war overlay trace in the old `rivers.md`.
pub fn corner_mask(map: &GameMap, x: i32, y: i32) -> u8 {
    let mask = |x, y| map.get(x, y).map_or(0, |t| t.river);
    u8::from(mask(x, y) & SOUTH != 0)
        | (u8::from(mask(x, y) & EAST != 0) << 1)
        | (u8::from(mask(x, y + 1) & EAST != 0) << 2)
        | (u8::from(mask(x + 1, y) & SOUTH != 0) << 3)
}

/// Native 0x56CE8C: defender river mask in the direction of the attacker.
pub fn crossed(map: &GameMap, defender: (i32, i32), attacker: (i32, i32)) -> bool {
    let mut dx = (attacker.0 - defender.0).rem_euclid(map.w);
    if dx > map.w / 2 {
        dx -= map.w;
    }
    let dy = attacker.1 - defender.1;
    let direction = civ3_rules::combat::dir_from_delta(dx - dy, dx + dy);
    map.get(defender.0, defender.1)
        .is_some_and(|t| civ3_rules::combat::river_edge(t.river, direction))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_rivers_share_edges_and_use_all_gameplay_gates() {
        let map = GameMap::generate_with_seed(1);
        let mut count = 0;
        for y in 0..map.h {
            for x in 0..map.w {
                let t = map.get(x, y).unwrap();
                if t.river != 0 {
                    count += 1;
                    assert!(map.fresh_water(x, y));
                }
                for (dx, dy, bit, opposite) in EDGES {
                    if let Some(nb) = map.get(x + dx, y + dy) {
                        assert_eq!(t.river & bit != 0, nb.river & opposite != 0);
                    } else {
                        assert_eq!(t.river & bit, 0);
                    }
                }
            }
        }
        assert!(count > 10, "the playable map contains rivers");
        let again = GameMap::generate_with_seed(1);
        assert!(
            map.tiles
                .iter()
                .zip(&again.tiles)
                .all(|(a, b)| a.river == b.river)
        );
    }

    #[test]
    fn river_commerce_floodplain_food_and_wrapped_combat_direction() {
        let mut map = GameMap::generate_with_seed(1);
        let i = map.idx(0, 10);
        let t = &mut map.tiles[i];
        t.base = Base::Desert;
        t.relief = crate::map::Relief::Flat;
        t.cover = crate::map::Cover::Bare;
        t.resource = None;
        t.road = false;
        t.river = WEST;
        assert_eq!(crate::map::terrain_row(t), 4);
        assert_eq!(crate::map::yields(t), (3, 0));
        assert_eq!(crate::cities::tile_commerce(t), 1);
        assert!(crossed(&map, (0, 10), (map.w - 1, 10)));
        assert!(!crossed(&map, (0, 10), (1, 10)));
        let attacker =
            crate::units::Unit::new(0, crate::units::UnitType::named("Warrior"), map.w - 1, 10);
        let defender = crate::units::Unit::new(1, crate::units::UnitType::named("Warrior"), 0, 10);
        let river_odds = crate::combat::round_odds(&map, &attacker, &defender, None);
        map.tiles[i].river = 0;
        assert!(river_odds > crate::combat::round_odds(&map, &attacker, &defender, None));
    }
}
