//! Tile yields: the food, shields and commerce one worked tile gives a city.
//!
//! [`food`], [`shields`] and [`commerce`] are line-for-line reimplementations
//! of the three `Map` methods the game calls for every tile of every city every
//! turn. Everything the routines read out of the map, the player records and the
//! city is a plain field of [`Tile`], [`Civ`], [`Centre`] and [`Working`], so
//! the functions are pure.

use crate::economy::{self, has_trait, trait_bit};
use crate::government::Govt;

/// Terrain ids: `TERR` row order.
pub mod terrain {
    /// Desert.
    pub const DESERT: usize = 0;
    /// Plains.
    pub const PLAINS: usize = 1;
    /// Grassland (the only terrain a "bonus grassland" shield sits on).
    pub const GRASSLAND: usize = 2;
    /// Tundra.
    pub const TUNDRA: usize = 3;
    /// Flood plain.
    pub const FLOOD_PLAIN: usize = 4;
    /// Hills.
    pub const HILLS: usize = 5;
    /// Mountains.
    pub const MOUNTAINS: usize = 6;
    /// Forest.
    pub const FOREST: usize = 7;
    /// Jungle.
    pub const JUNGLE: usize = 8;
    /// Marsh.
    pub const MARSH: usize = 9;
    /// Volcano.
    pub const VOLCANO: usize = 10;
    /// Coast (water: ids 11 to 13).
    pub const COAST: usize = 11;
    /// Sea.
    pub const SEA: usize = 12;
    /// Ocean.
    pub const OCEAN: usize = 13;
}

/// A water body with more tiles than this is ocean-like: the Harbor, Offshore
/// Platform and Commercial Dock bonuses apply to its tiles, and a city on its
/// shore counts as coastal for the Seafaring centre bonus. A body of this size
/// or smaller is a lake: +1 food and nothing else.
pub const LARGE_WATER: i32 = 20;

/// Food a city centre tile is worth, whatever the terrain: RULE word (shipped
/// 2), the same word that multiplies the citizens in the "food eaten" total.
pub const FOOD_PER_CITIZEN: i32 = 2;

/// Bits of BLDG `+0xEC` (the improvement flags) that the yield functions count
/// in the working city.
pub mod building_flag {
    /// +1 food on each ocean-like water tile (the Harbor).
    pub const HARBOR: u32 = 0x0100_0000;
    /// +1 shield on each ocean-like water tile (Offshore Platform).
    pub const OFFSHORE_PLATFORM: u32 = 0x0080_0000;
    /// +1 commerce on each ocean-like water tile (Commercial Dock).
    pub const COMMERCIAL_DOCK: u32 = 0x0200_0000;
}

/// BLDG `+0xF8` wonder-flag mask of "+1 trade in each trade-producing tile"
/// (the Colossus), counted with the working city as filter.
pub const COLOSSUS_FLAG: u32 = 0x20;

/// The numbers of a `TERR` row for one variant of the terrain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Variant {
    /// Base food.
    pub food: i32,
    /// Base shields.
    pub shields: i32,
    /// Base commerce.
    pub commerce: i32,
    /// Food an irrigated tile adds.
    pub irrigation: i32,
    /// Shields a mined tile adds.
    pub mining: i32,
    /// Commerce a road adds.
    pub road: i32,
}

/// One `TERR` row as far as the yields go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Terrain {
    /// The row's display name.
    pub name: &'static str,
    /// The ordinary tile.
    pub normal: Variant,
    /// The tile with the landmark flag.
    pub landmark: Variant,
}

impl Terrain {
    /// The variant a tile with (`true`) or without the landmark flag uses.
    pub fn variant(&self, landmark: bool) -> &Variant {
        if landmark {
            &self.landmark
        } else {
            &self.normal
        }
    }
}

/// `Variant` from `(food, shields, commerce)` and `(irrigation, mining, road)`.
const fn v(base: (i32, i32, i32), bonus: (i32, i32, i32)) -> Variant {
    Variant {
        food: base.0,
        shields: base.1,
        commerce: base.2,
        irrigation: bonus.0,
        mining: bonus.1,
        road: bonus.2,
    }
}

const fn terr(name: &'static str, normal: Variant, landmark: Variant) -> Terrain {
    Terrain { name, normal, landmark }
}

/// The 14 `TERR` rows of `conquests.biq`, in file order (see [`terrain`]).
pub const TERRAIN: [Terrain; 14] = [
    terr("Desert", v((0, 1, 0), (1, 1, 1)), v((0, 1, 0), (1, 1, 1))),
    terr("Plains", v((1, 1, 0), (1, 1, 1)), v((1, 1, 0), (1, 1, 1))),
    terr("Grassland", v((2, 0, 0), (1, 1, 1)), v((2, 0, 0), (1, 0, 1))),
    terr("Tundra", v((1, 0, 0), (0, 1, 1)), v((1, 0, 0), (0, 0, 1))),
    terr("Flood Plain", v((3, 0, 0), (1, 0, 1)), v((3, 0, 0), (1, 0, 1))),
    terr("Hills", v((1, 1, 0), (0, 2, 1)), v((1, 1, 0), (0, 1, 1))),
    terr("Mountains", v((0, 1, 0), (0, 2, 1)), v((0, 1, 0), (0, 1, 1))),
    terr("Forest", v((1, 2, 0), (0, 0, 1)), v((1, 2, 0), (0, 2, 1))),
    terr("Jungle", v((1, 0, 0), (0, 0, 1)), v((1, 0, 0), (0, 0, 1))),
    terr("Marsh", v((1, 0, 0), (0, 0, 1)), v((1, 0, 0), (1, 0, 0))),
    terr("Volcano", v((0, 3, 0), (0, 0, 0)), v((0, 1, 0), (0, 1, 1))),
    terr("Coast", v((1, 0, 2), (0, 0, 0)), v((1, 0, 2), (0, 0, 0))),
    terr("Sea", v((1, 0, 1), (0, 0, 0)), v((1, 0, 1), (0, 0, 0))),
    terr("Ocean", v((0, 0, 0), (0, 0, 0)), v((0, 0, 0), (0, 0, 0))),
];

/// One `GOOD` row as far as the yields go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resource {
    /// The row's display name.
    pub name: &'static str,
    /// The tech that reveals it (`-1` always visible). A resource the owner
    /// cannot see yields nothing.
    pub reveal_tech: i32,
    /// Bonus food.
    pub food: i32,
    /// Bonus shields.
    pub shields: i32,
    /// Bonus commerce.
    pub commerce: i32,
}

const fn res(name: &'static str, reveal_tech: i32, bonus: (i32, i32, i32)) -> Resource {
    Resource {
        name,
        reveal_tech,
        food: bonus.0,
        shields: bonus.1,
        commerce: bonus.2,
    }
}

/// Resource ids: `GOOD` row order.
pub mod resource {
    /// Horses.
    pub const HORSES: usize = 0;
    /// Iron.
    pub const IRON: usize = 1;
    /// Coal.
    pub const COAL: usize = 3;
    /// Uranium.
    pub const URANIUM: usize = 7;
    /// Wines.
    pub const WINES: usize = 8;
    /// Whales.
    pub const WHALES: usize = 16;
    /// Fish.
    pub const FISH: usize = 18;
    /// Cattle.
    pub const CATTLE: usize = 19;
    /// Wheat.
    pub const WHEAT: usize = 20;
    /// Gold.
    pub const GOLD: usize = 21;
}

/// The 26 `GOOD` rows of `conquests.biq`. The strategic resources are revealed
/// by The Wheel (4), Iron Working (7), Gunpowder (30), Steam Power (44),
/// Refining (53), Replaceable Parts (57), Rocketry (64) and Fission (65).
pub const RESOURCES: [Resource; 26] = [
    res("Horses", 4, (0, 0, 1)),
    res("Iron", 7, (0, 1, 0)),
    res("Saltpeter", 30, (0, 0, 1)),
    res("Coal", 44, (0, 2, 1)),
    res("Oil", 53, (0, 1, 2)),
    res("Rubber", 57, (0, 0, 2)),
    res("Aluminum", 64, (0, 2, 0)),
    res("Uranium", 65, (0, 2, 3)),
    res("Wines", -1, (1, 0, 1)),
    res("Furs", -1, (0, 1, 1)),
    res("Dyes", -1, (0, 0, 1)),
    res("Incense", -1, (0, 0, 2)),
    res("Spices", -1, (0, 0, 2)),
    res("Ivory", -1, (0, 0, 2)),
    res("Silks", -1, (0, 0, 3)),
    res("Gems", -1, (0, 0, 4)),
    res("Whales", -1, (1, 1, 2)),
    res("Game", -1, (2, 0, 0)),
    res("Fish", -1, (2, 0, 1)),
    res("Cattle", -1, (2, 1, 0)),
    res("Wheat", -1, (2, 0, 0)),
    res("Gold", -1, (0, 0, 4)),
    res("Sugar", -1, (1, 0, 1)),
    res("Tropical Fruit", -1, (1, 0, 1)),
    res("Oasis", -1, (2, 0, 0)),
    res("Tobacco", -1, (0, 0, 1)),
];

/// The tech of worker job 4 (Railroad) in `conquests.biq`, Steam Power. The
/// Road job (job 3) needs none.
pub const SHIPPED_RAILROAD_TECH: i32 = 44;

/// A city centre on the tile being evaluated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Centre {
    /// The city's population; the size class picks the centre bonuses.
    pub size: i32,
    /// The city is its owner's capital.
    pub is_capital: bool,
    /// An adjacent water body of the city has more than [`LARGE_WATER`] tiles.
    pub large_water_adjacent: bool,
    /// Trait mask of the race that owns the city: the Agricultural and
    /// Seafaring centre steps read it.
    pub owner_traits: u32,
}

/// What the routines read out of the map for one tile.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tile {
    /// Terrain id, see [`terrain`].
    pub terrain: usize,
    /// The landmark flag.
    pub landmark: bool,
    /// The resource on the tile (a [`RESOURCES`] index).
    pub resource: Option<usize>,
    /// Overlay bit 6: the whole tile yields nothing.
    pub pollution: bool,
    /// Overlay bit 8: one less of each positive base yield.
    pub crater: bool,
    /// Overlay bit 3.
    pub irrigated: bool,
    /// Overlay bit 2.
    pub mined: bool,
    /// The road predicate: overlay bit 0 *and* the tile owner knows the Road
    /// job's tech.
    pub road: bool,
    /// The railroad predicate: overlay bit 1 and the owner knows the Railroad
    /// job's tech.
    pub railroad: bool,
    /// A river runs along an edge.
    pub river: bool,
    /// Feature bit 16, the "bonus grassland" shield.
    pub shield_bonus: bool,
    /// For a water tile, the size of its water body; `None` for land.
    pub water_body: Option<i32>,
    /// A river or lake within the 3x3 neighbourhood. Only read for a centre.
    pub fresh_water: bool,
    /// The city on the tile, if any.
    pub centre: Option<Centre>,
}

/// The player the yield is computed for (the `civ` argument).
pub struct Civ<'a> {
    /// The government's GOVT record.
    pub govt: &'a Govt,
    /// The race's trait mask.
    pub traits: u32,
    /// The known-tech bits.
    pub knows: &'a dyn Fn(i32) -> bool,
    /// The Railroad job's required tech, see [`SHIPPED_RAILROAD_TECH`].
    pub railroad_tech: i32,
    /// A Golden Age is running.
    pub golden_age: bool,
}

impl Civ<'_> {
    /// `-1` is always true, anything else asks the player's known-tech bits.
    pub fn has_tech(&self, tech: i32) -> bool {
        tech == -1 || (self.knows)(tech)
    }
}

/// The city that works the tile (may be null).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Working {
    /// Buildings in the city with [`building_flag::HARBOR`].
    pub harbors: i32,
    /// Buildings with [`building_flag::OFFSHORE_PLATFORM`].
    pub offshore_platforms: i32,
    /// Buildings with [`building_flag::COMMERCIAL_DOCK`].
    pub commercial_docks: i32,
    /// The Colossus count in this city.
    pub colossus: i32,
    /// The owner is mobilized and the city is building a military unit.
    pub military_build: bool,
}

/// The resource on the tile if the civ can see it.
fn revealed(tile: &Tile, civ: &Civ<'_>) -> Option<&'static Resource> {
    let r = &RESOURCES[tile.resource?];
    civ.has_tech(r.reveal_tech).then_some(r)
}

/// Food of the tile. `planning` counts the tile as irrigated (and with a
/// railroad), as if the improvement were already there.
pub fn food(tile: &Tile, civ: &Civ<'_>, city: Option<&Working>, planning: bool) -> i32 {
    if tile.pollution {
        return 0;
    }
    let var = TERRAIN[tile.terrain].variant(tile.landmark);
    let mut f = var.food;
    if f > 0 && tile.crater {
        f -= 1;
    }
    if tile.irrigated || planning {
        f += var.irrigation;
        if var.irrigation > 0 && (tile.railroad || planning) && civ.has_tech(civ.railroad_tech) {
            f += 1;
        }
        if tile.terrain == terrain::DESERT && has_trait(civ.traits, trait_bit::AGRICULTURAL) {
            f += 1;
        }
    }
    if let Some(r) = revealed(tile, civ) {
        f += r.food;
    }
    if let Some(body) = tile.water_body {
        if body > LARGE_WATER {
            if let Some(w) = city {
                f += w.harbors;
            }
        } else {
            f += 1;
        }
    }
    if let Some(c) = &tile.centre {
        f = FOOD_PER_CITIZEN;
        if has_trait(c.owner_traits, trait_bit::AGRICULTURAL) {
            f += 1;
        }
    }
    if civ.govt.tile_penalty {
        let exempt = has_trait(civ.traits, trait_bit::AGRICULTURAL)
            && tile.centre.is_some()
            && tile.fresh_water;
        if f > 2 && !exempt {
            f -= 1;
        }
    }
    f.max(0)
}

/// Shields of the tile.
pub fn shields(tile: &Tile, civ: &Civ<'_>, city: Option<&Working>, planning: bool) -> i32 {
    if tile.pollution {
        return 0;
    }
    let var = TERRAIN[tile.terrain].variant(tile.landmark);
    let mut s = var.shields;
    if s > 0 && tile.crater {
        s -= 1;
    }
    if tile.mined || planning {
        s += var.mining;
        if var.mining > 0 && (tile.railroad || planning) && civ.has_tech(civ.railroad_tech) {
            s += 1;
        }
    }
    if let Some(r) = revealed(tile, civ) {
        s += r.shields;
    }
    if tile.terrain == terrain::GRASSLAND && tile.shield_bonus {
        s += 1;
    }
    if let Some(body) = tile.water_body {
        if body > LARGE_WATER {
            if let Some(w) = city {
                s += w.offshore_platforms;
            }
        }
    }
    if let Some(c) = &tile.centre {
        match economy::size_class(c.size, economy::TOWN_MAX, economy::CITY_MAX) {
            2 => {
                s += 2;
                if has_trait(civ.traits, trait_bit::INDUSTRIOUS) {
                    s += 1;
                }
            }
            1 => s += 1,
            _ => {}
        }
        s = s.max(1);
    }
    if civ.golden_age && s > 0 {
        s += 1;
    }
    if let Some(w) = city {
        if s > 0 && w.military_build {
            s += 1;
        }
    }
    if civ.govt.tile_penalty && s > 2 {
        s -= 1;
    }
    s.max(0)
}

/// Commerce of the tile.
pub fn commerce(tile: &Tile, civ: &Civ<'_>, city: Option<&Working>, planning: bool) -> i32 {
    if tile.pollution {
        return 0;
    }
    let var = TERRAIN[tile.terrain].variant(tile.landmark);
    let mut c = var.commerce;
    if c > 0 && tile.crater {
        c -= 1;
    }
    if tile.road || planning {
        c += var.road;
    }
    if let Some(r) = revealed(tile, civ) {
        c += r.commerce;
    }
    if tile.river {
        c += 1;
    }
    if let Some(body) = tile.water_body {
        if body > LARGE_WATER {
            if let Some(w) = city {
                c += w.commercial_docks;
            }
        }
    }
    if let Some(ctr) = &tile.centre {
        let commercial = has_trait(civ.traits, trait_bit::COMMERCIAL);
        match economy::size_class(ctr.size, economy::TOWN_MAX, economy::CITY_MAX) {
            2 => c += if commercial { 5 } else { 2 },
            1 => c += if commercial { 3 } else { 1 },
            _ => {}
        }
        c = c.max(if ctr.is_capital { 4 } else { 1 });
        if ctr.large_water_adjacent && has_trait(ctr.owner_traits, trait_bit::SEAFARING) {
            c += 1;
        }
    }
    if civ.golden_age && c > 0 {
        c += 1;
    }
    if let Some(w) = city {
        if c > 0 {
            c += w.colossus;
        }
    }
    if civ.govt.trade_bonus && c > 0 {
        c += 1;
    }
    if civ.govt.tile_penalty && c > 2 {
        c -= 1;
    }
    c.max(0)
}

/// `(food, shields, commerce)` of the tile.
pub fn tile_yield(
    tile: &Tile,
    civ: &Civ<'_>,
    city: Option<&Working>,
    planning: bool,
) -> (i32, i32, i32) {
    (
        food(tile, civ, city, planning),
        shields(tile, civ, city, planning),
        commerce(tile, civ, city, planning),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::government::{row, SHIPPED};

    static GOVERNMENTS: [Govt; 8] = SHIPPED;
    const ALL_TECH: &dyn Fn(i32) -> bool = &|_| true;
    const NO_TECH: &dyn Fn(i32) -> bool = &|_| false;

    fn civ(government: usize, traits: u32) -> Civ<'static> {
        Civ {
            govt: &GOVERNMENTS[government],
            traits,
            knows: ALL_TECH,
            railroad_tech: SHIPPED_RAILROAD_TECH,
            golden_age: false,
        }
    }

    fn land(terrain: usize) -> Tile {
        Tile {
            terrain,
            ..Tile::default()
        }
    }

    fn water(terrain: usize, body: i32) -> Tile {
        Tile {
            terrain,
            water_body: Some(body),
            ..Tile::default()
        }
    }

    fn y(tile: &Tile, civ: &Civ<'_>) -> (i32, i32, i32) {
        tile_yield(tile, civ, None, false)
    }

    #[test]
    fn shipped_terrain_rows() {
        assert_eq!(TERRAIN[terrain::GRASSLAND].normal, v((2, 0, 0), (1, 1, 1)));
        assert_eq!(TERRAIN[terrain::DESERT].normal.irrigation, 1);
        assert_eq!(TERRAIN[terrain::HILLS].normal.mining, 2);
        assert_eq!(TERRAIN[terrain::FLOOD_PLAIN].normal.food, 3);
        assert_eq!(TERRAIN[terrain::COAST].normal.commerce, 2);
        assert_eq!(TERRAIN[terrain::OCEAN].normal, v((0, 0, 0), (0, 0, 0)));
        let differing: Vec<_> = TERRAIN
            .iter()
            .filter(|t| t.normal != t.landmark)
            .map(|t| t.name)
            .collect();
        assert_eq!(
            differing,
            ["Grassland", "Tundra", "Hills", "Mountains", "Forest", "Marsh", "Volcano"]
        );
    }

    #[test]
    fn shipped_resource_rows() {
        assert_eq!(RESOURCES[resource::HORSES].reveal_tech, 4);
        assert_eq!(RESOURCES[resource::COAL].reveal_tech, SHIPPED_RAILROAD_TECH);
        assert_eq!(RESOURCES[resource::GOLD].commerce, 4);
        assert_eq!(RESOURCES[resource::FISH].food, 2);
        let whales = &RESOURCES[resource::WHALES];
        assert_eq!((whales.food, whales.shields, whales.commerce), (1, 1, 2));
        for (i, r) in RESOURCES.iter().enumerate() {
            assert_eq!(r.reveal_tech != -1, i < 8, "{}", r.name);
        }
    }

    #[test]
    fn plain_tiles() {
        let m = civ(row::MONARCHY, 0);
        assert_eq!(y(&land(terrain::PLAINS), &m), (1, 1, 0));
        assert_eq!(y(&land(terrain::GRASSLAND), &m), (2, 0, 0));
        assert_eq!(y(&land(terrain::FOREST), &m), (1, 2, 0));
        assert_eq!(y(&land(terrain::MOUNTAINS), &m), (0, 1, 0));
        assert_eq!(y(&water(terrain::OCEAN, 500), &m), (0, 0, 0));
    }

    #[test]
    fn pollution_zeroes_everything() {
        let m = civ(row::MONARCHY, 0);
        let mut t = land(terrain::FLOOD_PLAIN);
        t.irrigated = true;
        t.pollution = true;
        t.resource = Some(resource::WHEAT);
        assert_eq!(y(&t, &m), (0, 0, 0));
    }

    #[test]
    fn crater_takes_one_off_positive_bases() {
        let m = civ(row::MONARCHY, 0);
        let mut t = land(terrain::PLAINS);
        t.crater = true;
        assert_eq!(y(&t, &m), (0, 0, 0));
        let mut t = land(terrain::GRASSLAND);
        t.crater = true;
        assert_eq!(y(&t, &m), (1, 0, 0));
        let mut t = land(terrain::GRASSLAND);
        t.crater = true;
        t.irrigated = true;
        assert_eq!(food(&t, &m, None, false), 2);
    }

    #[test]
    fn irrigation_mine_road_and_river() {
        let m = civ(row::MONARCHY, 0);
        let mut t = land(terrain::GRASSLAND);
        t.irrigated = true;
        assert_eq!(food(&t, &m, None, false), 3);
        t.railroad = true;
        assert_eq!(food(&t, &m, None, false), 4);
        let mut t = land(terrain::HILLS);
        t.mined = true;
        assert_eq!(shields(&t, &m, None, false), 3);
        let mut t = land(terrain::PLAINS);
        t.road = true;
        t.river = true;
        assert_eq!(commerce(&t, &m, None, false), 2);
    }

    #[test]
    fn a_resource_shows_only_with_its_tech() {
        let mut t = land(terrain::PLAINS);
        t.resource = Some(resource::HORSES);
        let all = civ(row::MONARCHY, 0);
        let none = Civ {
            govt: &GOVERNMENTS[row::MONARCHY],
            traits: 0,
            knows: NO_TECH,
            railroad_tech: SHIPPED_RAILROAD_TECH,
            golden_age: false,
        };
        assert_eq!(commerce(&t, &all, None, false), 1);
        assert_eq!(commerce(&t, &none, None, false), 0);
        // Planning adds the road's commerce but never reveals the resource.
        assert_eq!(commerce(&t, &none, None, true), 1);
    }

    #[test]
    fn the_despotism_cap_and_trade_bonus() {
        let despot = civ(row::DESPOTISM, 0);
        let mut wet = land(terrain::GRASSLAND);
        wet.irrigated = true;
        assert_eq!(food(&wet, &despot, None, false), 2);
        let republic = civ(row::REPUBLIC, 0);
        let t = water(terrain::COAST, 50);
        assert_eq!(commerce(&t, &republic, None, false), 3, "trade bonus on water");
    }

    #[test]
    fn a_centre_uses_the_size_class_and_the_capital_floor() {
        let m = civ(row::MONARCHY, 0);
        let mut t = land(terrain::PLAINS);
        t.centre = Some(Centre {
            size: 13,
            is_capital: false,
            large_water_adjacent: false,
            owner_traits: 0,
        });
        assert_eq!(tile_yield(&t, &m, None, false), (FOOD_PER_CITIZEN, 3, 2));
        let mut t = land(terrain::DESERT);
        t.centre = Some(Centre {
            size: 1,
            is_capital: true,
            large_water_adjacent: false,
            owner_traits: 0,
        });
        assert_eq!(commerce(&t, &m, None, false), 4);
    }

    #[test]
    fn harbors_and_docks_need_large_water() {
        let m = civ(row::MONARCHY, 0);
        let working = Working {
            harbors: 1,
            offshore_platforms: 1,
            commercial_docks: 1,
            ..Working::default()
        };
        let big = water(terrain::COAST, 50);
        assert_eq!(food(&big, &m, Some(&working), false), 2);
        assert_eq!(shields(&big, &m, Some(&working), false), 1);
        let lake = water(terrain::COAST, 10);
        assert_eq!(food(&lake, &m, Some(&working), false), 2);
        assert_eq!(shields(&lake, &m, Some(&working), false), 0);
    }
}
