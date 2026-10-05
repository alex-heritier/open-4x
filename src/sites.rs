//! Ancient tile structures (`worker-jobs.md`, `colonies.md`, `combat.md`).
//! Fortification bits are independent of colony objects. Outposts consume
//! their completing Worker and see a 3x3, 5x5 or 7x7 area on flat, hill or
//! mountain terrain. Foreign entry or cultural ownership destroys them.
//! Labor follows the native worker rules; railroads are absent.

use crate::features::{GOODS, GoodKind};
use crate::map::{Base, GameMap, Tile};

/// A structure on a tile.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Site {
    /// A plain Colony of this civilization.
    Colony(u8),
    Outpost(u8),
}

/// `BLDG`/`PRTO` tech index of Construction, the Fortress job's prerequisite
/// (`worker-jobs.md` 1.1).
pub const CONSTRUCTION: i32 = 20;
pub const MASONRY: i32 = 1;

/// The tile may take a Fortress: land, with no city and no Fortress yet.
pub fn can_fortress(map: &GameMap, has_city: bool, x: i32, y: i32) -> bool {
    !has_city
        && map.is_land(x, y)
        && map.get(x, y).is_some_and(|t| fort_terrain(t) && !t.fortress)
}

/// Shipped TERR gates for the terrain classes represented by this map.
fn fort_terrain(t: &Tile) -> bool { !matches!(t.base, Base::Ice | Base::Coast | Base::Sea | Base::Ocean) }

pub fn can_barricade(map: &GameMap, has_city: bool, x: i32, y: i32) -> bool {
    !has_city && map.get(x, y).is_some_and(|t| fort_terrain(t) && t.fortress && !t.barricade)
}

pub fn can_outpost(map: &GameMap, civ: usize, has_city: bool, x: i32, y: i32) -> bool {
    !has_city && map.get(x, y).is_some_and(|t| fort_terrain(t)
        && t.owner.is_none_or(|o| o as usize == civ) && !matches!(t.site, Some(Site::Outpost(_))))
}

pub fn sight(t: &Tile) -> i32 {
    match t.site {
        Some(Site::Colony(_)) => 1,
        Some(Site::Outpost(_)) => match t.relief {
            crate::map::Relief::Mountain => 3,
            crate::map::Relief::Hill => 2,
            _ => 1,
        },
        None => 0,
    }
}

pub fn owner(site: Site) -> usize { match site { Site::Colony(c) | Site::Outpost(c) => c as usize } }

/// City founding, foreign cultural ownership and foreign entry remove colony
/// objects (`colonies.md` 7). No war check belongs to the destroyer itself.
pub fn remove_overrun(
    mut map: bevy::prelude::ResMut<GameMap>,
    cities: bevy::prelude::Query<&crate::cities::City>,
    units: bevy::prelude::Query<&crate::units::Unit>,
) {
    let w = map.w;
    for (i, t) in map.tiles.iter_mut().enumerate() {
        let Some(site) = t.site else { continue };
        let civ = owner(site);
        let p = (i as i32 % w, i as i32 / w);
        if cities.iter().any(|c| (c.x, c.y) == p)
            || t.owner.is_some_and(|o| match site { Site::Colony(_) => true, Site::Outpost(_) => o as usize != civ })
            || units.iter().any(|u| u.carrier.is_none() && (u.x, u.y) == p && u.civ != civ)
        { t.site = None; }
    }
}

/// Why a Colony cannot be founded here, or `None` when it can
/// (`colonies.md` 2: a visible luxury or strategic resource whose advance is
/// known, an empty unowned tile of terrain that allows colonies).
pub fn colony_refusal(
    map: &GameMap,
    known: impl Fn(i32) -> bool,
    has_city: bool,
    border_owner: bool,
    x: i32,
    y: i32,
) -> Option<&'static str> {
    let t = map.get(x, y)?;
    let Some(id) = t.resource.filter(|_| t.seen) else {
        return Some("A colony needs a luxury or strategic resource.");
    };
    let good = &GOODS[id as usize];
    if good.kind == GoodKind::Bonus {
        return Some("A colony needs a luxury or strategic resource.");
    }
    if let Some(row) = crate::realm::strategic_row(id)
        && !known(crate::ruleset::GOOD[row])
    {
        return Some("We do not yet know how to use that resource.");
    }
    if has_city || t.camp || matches!(t.site, Some(Site::Colony(_))) {
        return Some("Something already stands on that tile.");
    }
    if !map.is_land(x, y) || matches!(t.base, Base::Coast | Base::Sea | Base::Ocean) {
        return Some("Colonies need land.");
    }
    if border_owner {
        return Some("That tile lies inside someone's borders.");
    }
    None
}

/// Found the colony: a road on the tile and the structure itself.
pub fn found_colony(t: &mut Tile, civ: usize) {
    t.road = true;
    t.site = Some(Site::Colony(civ as u8));
}

/// Resources of `civ`'s colonies, one entry per colony, unless another
/// civ's border has since closed over the tile.
pub fn colony_goods<'a>(
    map: &'a GameMap,
    civ: usize,
    border_owner: &'a dyn Fn(i32, i32) -> Option<usize>,
) -> impl Iterator<Item = u8> + 'a {
    (0..map.h).flat_map(move |y| {
        (0..map.w).filter_map(move |x| {
            let t = map.get(x, y)?;
            (t.site == Some(Site::Colony(civ as u8)) && border_owner(x, y).is_none_or(|o| o == civ))
                .then_some(t.resource?)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outpost_and_barricade_gates_follow_native_prerequisites() {
        let (mut map, (x, y), _) = world();
        let i = map.idx(x, y);
        assert!(can_outpost(&map, 0, false, x, y));
        assert!(!can_outpost(&map, 0, true, x, y));
        map.tiles[i].owner = Some(1);
        assert!(!can_outpost(&map, 0, false, x, y));
        assert!(can_outpost(&map, 1, false, x, y));
        assert!(!can_barricade(&map, false, x, y));
        map.tiles[i].fortress = true;
        assert!(can_barricade(&map, false, x, y));
        crate::improvements::apply_work(&mut map.tiles[i], crate::improvements::WorkAction::Barricade);
        assert!(map.tiles[i].fortress && map.tiles[i].barricade);
        assert!(!can_barricade(&map, false, x, y));
        assert_eq!(civ3mapgen::combat::Structure::from_overlay(true, true), civ3mapgen::combat::Structure::Fortress);
        map.tiles[i].site = Some(Site::Outpost(1));
        assert!(!can_outpost(&map, 1, false, x, y));
        for (relief, radius) in [(crate::map::Relief::Flat, 1), (crate::map::Relief::Hill, 2), (crate::map::Relief::Mountain, 3)] {
            map.tiles[i].relief = relief;
            assert_eq!(sight(&map.tiles[i]), radius);
        }
    }

    fn world() -> (GameMap, (i32, i32), u8) {
        let mut map = GameMap::generate_with_seed(1);
        let (x, y) = (0..map.h)
            .flat_map(|y| (0..map.w).map(move |x| (x, y)))
            .find(|&(x, y)| map.is_land(x, y))
            .unwrap();
        // The horses of the shipped sheet are a strategic good.
        let horses = GOODS.iter().position(|g| g.name == "Horses").unwrap() as u8;
        let diamonds = GOODS.iter().position(|g| g.name == "Diamonds").unwrap() as u8;
        let i = map.idx(x, y);
        map.tiles[i].resource = Some(diamonds);
        map.tiles[i].seen = true;
        map.tiles[i].camp = false;
        map.tiles[i].site = None;
        (map, (x, y), horses)
    }

    #[test]
    fn a_colony_needs_a_visible_unowned_luxury_or_strategic_tile() {
        let (mut map, (x, y), horses) = world();
        let ok = |m: &GameMap, city, owned| colony_refusal(m, |_| true, city, owned, x, y);
        assert_eq!(ok(&map, false, false), None);
        assert!(ok(&map, true, false).is_some(), "a city is there");
        assert!(ok(&map, false, true).is_some(), "someone's border");
        let i = map.idx(x, y);
        map.tiles[i].seen = false;
        assert!(ok(&map, false, false).is_some(), "not seen");
        map.tiles[i].seen = true;
        map.tiles[i].resource = None;
        assert!(ok(&map, false, false).is_some(), "no resource");
        // A strategic good waits for its advance.
        map.tiles[i].resource = Some(horses);
        assert!(colony_refusal(&map, |_| false, false, false, x, y).is_some());
        assert_eq!(colony_refusal(&map, |_| true, false, false, x, y), None);
        map.tiles[i].fortress = true;
        assert_eq!(ok(&map, false, false), None, "a fortress can share a colony tile");
    }

    #[test]
    fn a_colony_lays_a_road_and_counts_its_resource_until_a_rival_border_closes() {
        let (mut map, (x, y), _) = world();
        let i = map.idx(x, y);
        found_colony(&mut map.tiles[i], 1);
        assert!(map.tiles[i].road);
        let diamonds = map.tiles[i].resource.unwrap();
        let free = |_: i32, _: i32| None;
        assert_eq!(colony_goods(&map, 1, &free).collect::<Vec<_>>(), vec![diamonds]);
        assert_eq!(colony_goods(&map, 0, &free).count(), 0, "nobody else's");
        let rival = |_: i32, _: i32| Some(2);
        assert_eq!(colony_goods(&map, 1, &rival).count(), 0, "a rival's border");
        let own = |_: i32, _: i32| Some(1);
        assert_eq!(colony_goods(&map, 1, &own).count(), 1, "its own border is fine");
    }

    #[test]
    fn a_fortress_wants_land_and_no_city() {
        let (map, (x, y), _) = world();
        assert!(can_fortress(&map, false, x, y));
        assert!(!can_fortress(&map, true, x, y));
        let water = (0..map.h)
            .flat_map(|y| (0..map.w).map(move |x| (x, y)))
            .find(|&(x, y)| !map.is_land(x, y))
            .unwrap();
        assert!(!can_fortress(&map, false, water.0, water.1));
    }
}
