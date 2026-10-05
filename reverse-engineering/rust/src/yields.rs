//! Tile yields: the food, shields and commerce one worked tile gives a city.
//!
//! Findings, addresses and the open list are in `../yields.md`. The three
//! functions here are line-for-line reimplementations of the three `Map`
//! methods the game calls for every tile of every city, every turn:
//!
//! | here | binary | `ret` |
//! |---|---|---|
//! | [`food`] | `0x5D7180` | `0x18` |
//! | [`shields`] | `0x5D75F0` | `0x18` |
//! | [`commerce`] | `0x5D7AD0` | `0x18` |
//!
//! Each takes `(x, y, terrain, civ, planning, city)` in the binary;
//! `City::tileYield(kind, x, y)` (`0x4B0330`) is the only wrapper and passes
//! the city's owner, `planning = 0` and the city itself. Everything the
//! binary reads out of the map, the player records and the city is a plain
//! field of [`Tile`], [`Civ`], [`Centre`] and [`Working`] here, so the
//! functions are pure.
//!
//! The common shape of all three:
//!
//! 1. a polluted tile (overlay bit 6) is worth **nothing**;
//! 2. the terrain's own number (the landmark variant when the tile carries
//!    the landmark flag), minus one for a crater when it is positive;
//! 3. the improvement bonus (irrigation, mine, road) plus the railroad
//!    step when the owner knows Steam Power's worker job;
//! 4. the resource bonus, the water-body rules, the city-centre rules;
//! 5. the Golden Age step (shields and commerce only), the wonder or
//!    government steps, and last the Despotism cap on anything above two.

use crate::economy::{self, has_trait, trait_bit};
use crate::government::Govt;

/// Terrain ids: `TERR` row order, also the high nibble of the tile word
/// `[cell+0x2C]` (`0x5EAB30`, cell vtable `+0xC8`).
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
    /// Coast (water: ids 11 to 13, `0x5EAA30`).
    pub const COAST: usize = 11;
    /// Sea.
    pub const SEA: usize = 12;
    /// Ocean.
    pub const OCEAN: usize = 13;
}

/// A water body with more tiles than this is ocean-like: the Harbor,
/// Offshore Platform and Commercial Dock bonuses apply to its tiles, and a
/// city on its shore counts as coastal for the Seafaring centre bonus. A body
/// of this size or smaller is a lake: +1 food and nothing else
/// (`cmp [body+0x24], 0x14` at `0x5D7470`, `0x5D790F`, `0x5D7D79`, `0x5D7EF4`).
pub const LARGE_WATER: i32 = 20;

/// Food a city centre tile is worth, whatever the terrain: RULE word
/// `[0x9C72B4]` (shipped 2), the same word that multiplies the citizens in
/// the "food eaten" total (`0x4B05A4`).
pub const FOOD_PER_CITIZEN: i32 = 2;

/// Bits of BLDG `+0xEC` (the improvement flags) that the yield functions
/// count in the working city with `City::countFlag` (`0x4B1F90`).
pub mod building_flag {
    /// +1 food on each ocean-like water tile (the Harbor; `0x5D7484`).
    pub const HARBOR: u32 = 0x0100_0000;
    /// +1 shield on each ocean-like water tile (`0x5D791D`).
    pub const OFFSHORE_PLATFORM: u32 = 0x0080_0000;
    /// +1 commerce on each ocean-like water tile (`0x5D7D87`).
    pub const COMMERCIAL_DOCK: u32 = 0x0200_0000;
}

/// BLDG `+0xF8` wonder-flag mask of "+1 trade in each trade-producing tile"
/// (the Colossus), counted by `Player::countWonderFlag` (`0x55A8D0`) with the
/// working city as filter, the one consumer of this mask (`0x5D7F97`).
pub const COLOSSUS_FLAG: u32 = 0x20;

/// The numbers of a `TERR` row for one variant of the terrain (the normal
/// tile or the landmark tile).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Variant {
    /// Base food (memory `+0x64`, landmark `+0x88`).
    pub food: i32,
    /// Base shields (`+0x68`, landmark `+0x8C`).
    pub shields: i32,
    /// Base commerce (`+0x6C`, landmark `+0x90`).
    pub commerce: i32,
    /// Food an irrigated tile adds (`+0x4C`, landmark `+0x94`).
    pub irrigation: i32,
    /// Shields a mined tile adds (`+0x50`, landmark `+0x98`).
    pub mining: i32,
    /// Commerce a road adds (`+0x54`, landmark `+0x9C`).
    pub road: i32,
}

/// One `TERR` row as far as the yields go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Terrain {
    /// The row's display name.
    pub name: &'static str,
    /// The ordinary tile.
    pub normal: Variant,
    /// The tile with the landmark flag (`[cell+0x30]` bit 29).
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
    Terrain {
        name,
        normal,
        landmark,
    }
}

/// The 14 `TERR` rows of `conquests.biq`, in file order (see [`terrain`]).
///
/// Row layout in the file (body offsets): `+0x48 / +0x4C / +0x50`
/// irrigation, mining and road bonus, `+0x5C / +0x60 / +0x64` base food,
/// shields, commerce; the landmark block follows the flag bytes at `+0x7D`
/// as food, shields, commerce, irrigation, mining, road, defense, movement.
/// The reader is `0x5E9300`; the file offsets are its `fread` sequence.
pub const TERRAIN: [Terrain; 14] = [
    terr("Desert", v((0, 1, 0), (1, 1, 1)), v((0, 1, 0), (1, 1, 1))),
    terr("Plains", v((1, 1, 0), (1, 1, 1)), v((1, 1, 0), (1, 1, 1))),
    terr(
        "Grassland",
        v((2, 0, 0), (1, 1, 1)),
        v((2, 0, 0), (1, 0, 1)),
    ),
    terr("Tundra", v((1, 0, 0), (0, 1, 1)), v((1, 0, 0), (0, 0, 1))),
    terr(
        "Flood Plain",
        v((3, 0, 0), (1, 0, 1)),
        v((3, 0, 0), (1, 0, 1)),
    ),
    terr("Hills", v((1, 1, 0), (0, 2, 1)), v((1, 1, 0), (0, 1, 1))),
    terr(
        "Mountains",
        v((0, 1, 0), (0, 2, 1)),
        v((0, 1, 0), (0, 1, 1)),
    ),
    terr("Forest", v((1, 2, 0), (0, 0, 1)), v((1, 2, 0), (0, 2, 1))),
    terr("Jungle", v((1, 0, 0), (0, 0, 1)), v((1, 0, 0), (0, 0, 1))),
    terr("Marsh", v((1, 0, 0), (0, 0, 1)), v((1, 0, 0), (1, 0, 0))),
    terr("Volcano", v((0, 3, 0), (0, 0, 0)), v((0, 1, 0), (0, 1, 1))),
    terr("Coast", v((1, 0, 2), (0, 0, 0)), v((1, 0, 2), (0, 0, 0))),
    terr("Sea", v((1, 0, 1), (0, 0, 0)), v((1, 0, 1), (0, 0, 0))),
    terr("Ocean", v((0, 0, 0), (0, 0, 0)), v((0, 0, 0), (0, 0, 0))),
];

/// One `GOOD` row as far as the yields go (the in-memory `RESOURCE` row,
/// stride 92, table `[0x9C71D4]`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resource {
    /// The row's display name.
    pub name: &'static str,
    /// The tech that reveals it (`+0x4C`; `-1` always visible). A resource
    /// the owner cannot see yields nothing (`0x5D7B5A`).
    pub reveal_tech: i32,
    /// Bonus food (`+0x50`).
    pub food: i32,
    /// Bonus shields (`+0x54`).
    pub shields: i32,
    /// Bonus commerce (`+0x58`).
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

/// The 26 `GOOD` rows of `conquests.biq`: body `+0x48` reveal tech and
/// `+0x4C / +0x50 / +0x54` food, shield and commerce bonus (memory `+0x4C`
/// and `+0x50 / +0x54 / +0x58`). The strategic resources are revealed by
/// The Wheel (4), Iron Working (7), Gunpowder (30), Steam Power (44),
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

/// The tech of worker job 4 (Railroad) in `conquests.biq`, Steam Power:
/// `TFRM[4].required_tech`, the dword at `[[0x9C7324] + 0x218]`
/// (`0x5D7345`). The Road job (`TFRM[3]`, `+0x1A4`) needs none.
pub const SHIPPED_RAILROAD_TECH: i32 = 44;

/// A city centre on the tile being evaluated (`cell +0x1A` is not `-1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Centre {
    /// The city's population (`+0x138`); the size class picks the centre
    /// bonuses (`0x427540`).
    pub size: i32,
    /// The city is its owner's capital: `city+0x20 == Player+0x2C`.
    pub is_capital: bool,
    /// An adjacent water body of the city has more than [`LARGE_WATER`]
    /// tiles (`0x4AE280`, then `[0x9C7580] + 40 * body + 0x24`).
    pub large_water_adjacent: bool,
    /// Trait mask of the race that owns the city (cell vtable `+0x114`):
    /// the Agricultural centre step and the Seafaring centre step read it.
    pub owner_traits: u32,
}

/// What the binary reads out of the map for one tile.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tile {
    /// Terrain id, see [`terrain`].
    pub terrain: usize,
    /// The landmark flag (`[cell+0x30]` bit 29, cell vtable `+0x78`).
    pub landmark: bool,
    /// The resource on the tile (`[cell+8]`, a [`RESOURCES`] index).
    pub resource: Option<usize>,
    /// Overlay bit 6: the whole tile yields nothing.
    pub pollution: bool,
    /// Overlay bit 8: one less of each positive base yield.
    pub crater: bool,
    /// Overlay bit 3.
    pub irrigated: bool,
    /// Overlay bit 2.
    pub mined: bool,
    /// The road predicate (cell vtable `+0x64`, `0x5D9FF0`): overlay bit 0
    /// *and* the tile owner knows the Road job's tech.
    pub road: bool,
    /// The railroad predicate (`+0x5C`, `0x5DA0D0`): overlay bit 1 and the
    /// owner knows the Railroad job's tech.
    pub railroad: bool,
    /// A river runs along an edge: `byte [cell+4] != 0` (`+0x60`).
    pub river: bool,
    /// Feature bit 16, the "bonus grassland" shield (`+0x6C`).
    pub shield_bonus: bool,
    /// For a water tile, the size of its water body; `None` for land
    /// (`+0x8C`: terrain 11 to 13 is water).
    pub water_body: Option<i32>,
    /// `Map::freshWater(x, y)` (Map vtable `+0x60`, `0x5F39E0`): a river or
    /// lake within the 3x3 neighbourhood. Only read for a city centre.
    pub fresh_water: bool,
    /// The city on the tile, if any.
    pub centre: Option<Centre>,
}

/// The player the yield is computed for (the `civ` argument).
pub struct Civ<'a> {
    /// The government's GOVT record.
    pub govt: &'a Govt,
    /// The race's trait mask (RACE `+0x948`).
    pub traits: u32,
    /// `Player::hasTech` for a real tech id (the bit in `[0xA52B4C][tech]`).
    pub knows: &'a dyn Fn(i32) -> bool,
    /// `TFRM[4].required_tech`, see [`SHIPPED_RAILROAD_TECH`].
    pub railroad_tech: i32,
    /// `turn < Player+0x3C`: a Golden Age is running (`[0xA526AC]`).
    pub golden_age: bool,
}

impl Civ<'_> {
    /// `Player::hasTech` (`0x561440`): `-1` is always true, anything else
    /// asks the player's known-tech bits.
    pub fn has_tech(&self, tech: i32) -> bool {
        tech == -1 || (self.knows)(tech)
    }
}

/// The city that works the tile (the `city` argument, which may be null).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Working {
    /// Buildings in the city with [`building_flag::HARBOR`].
    pub harbors: i32,
    /// Buildings with [`building_flag::OFFSHORE_PLATFORM`].
    pub offshore_platforms: i32,
    /// Buildings with [`building_flag::COMMERCIAL_DOCK`].
    pub commercial_docks: i32,
    /// `Player::countWonderFlag(`[`COLOSSUS_FLAG`]`, city)`: 1 in the
    /// city that has the Colossus, else 0.
    pub colossus: i32,
    /// `City::mobilizationBonus` (`0x4BFEE0`, the only condition at
    /// `0x5D7A6D`): the owner is mobilized and the city is building a
    /// military unit, see [`crate::government::mobilization_bonus`].
    pub military_build: bool,
}

/// The resource on the tile if the civ can see it (`0x5D7B4C..0x5D7B99`).
fn revealed(tile: &Tile, civ: &Civ<'_>) -> Option<&'static Resource> {
    let r = &RESOURCES[tile.resource?];
    civ.has_tech(r.reveal_tech).then_some(r)
}

/// Food of the tile (`0x5D7180`).
///
/// `planning` is the fifth argument: when set the tile counts as irrigated
/// (and with a railroad), as if the improvement were already there.
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

/// Shields of the tile (`0x5D75F0`).
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

/// Commerce of the tile (`0x5D7AD0`).
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

    fn centre(terrain: usize, size: i32) -> Tile {
        Tile {
            terrain,
            road: true,
            centre: Some(Centre {
                size,
                is_capital: false,
                large_water_adjacent: false,
                owner_traits: 0,
            }),
            ..Tile::default()
        }
    }

    fn y(tile: &Tile, civ: &Civ<'_>) -> (i32, i32, i32) {
        tile_yield(tile, civ, None, false)
    }

    /// The table is the shipped `TERR` section: spot checks against the
    /// values the editor shows (Grassland 2/0/0, Desert irrigation 1) and
    /// against the landmark block's own differences.
    #[test]
    fn shipped_terrain_rows() {
        assert_eq!(TERRAIN[terrain::GRASSLAND].name, "Grassland");
        assert_eq!(TERRAIN[terrain::GRASSLAND].normal, v((2, 0, 0), (1, 1, 1)));
        assert_eq!(TERRAIN[terrain::DESERT].normal.irrigation, 1);
        assert_eq!(TERRAIN[terrain::HILLS].normal.mining, 2);
        assert_eq!(TERRAIN[terrain::FLOOD_PLAIN].normal.food, 3);
        assert_eq!(TERRAIN[terrain::COAST].normal.commerce, 2);
        assert_eq!(TERRAIN[terrain::OCEAN].normal, v((0, 0, 0), (0, 0, 0)));
        // The landmark block differs from the normal one only in a few rows.
        let differing: Vec<_> = TERRAIN
            .iter()
            .filter(|t| t.normal != t.landmark)
            .map(|t| t.name)
            .collect();
        assert_eq!(
            differing,
            [
                "Grassland",
                "Tundra",
                "Hills",
                "Mountains",
                "Forest",
                "Marsh",
                "Volcano"
            ]
        );
        assert_eq!(TERRAIN[terrain::FOREST].landmark.mining, 2);
        assert_eq!(TERRAIN[terrain::FOREST].normal.mining, 0);
    }

    /// Resource rows, with the strategic reveal techs and the known luxury
    /// and bonus numbers (Fish +2 food, Gold +4 commerce, Whales 1/1/2).
    #[test]
    fn shipped_resource_rows() {
        assert_eq!(RESOURCES[resource::HORSES].reveal_tech, 4);
        assert_eq!(RESOURCES[resource::COAL].reveal_tech, SHIPPED_RAILROAD_TECH);
        assert_eq!(RESOURCES[resource::GOLD].commerce, 4);
        assert_eq!(RESOURCES[resource::FISH].food, 2);
        let whales = &RESOURCES[resource::WHALES];
        assert_eq!((whales.food, whales.shields, whales.commerce), (1, 1, 2));
        // Every strategic resource has a reveal tech, nothing else has one.
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

    /// `0x5D7192..0x5D71AC` and its twins: overlay bit 6 returns 0.
    #[test]
    fn pollution_zeroes_everything() {
        let m = civ(row::MONARCHY, 0);
        let mut t = land(terrain::FLOOD_PLAIN);
        t.irrigated = true;
        t.pollution = true;
        t.resource = Some(resource::WHEAT);
        assert_eq!(y(&t, &m), (0, 0, 0));
    }

    /// A crater takes one off each positive base yield and never makes a
    /// zero negative (`0x5D7289..0x5D72A5`).
    #[test]
    fn crater_takes_one_off_positive_bases() {
        let m = civ(row::MONARCHY, 0);
        let mut t = land(terrain::PLAINS);
        t.crater = true;
        assert_eq!(y(&t, &m), (0, 0, 0));
        let mut t = land(terrain::GRASSLAND);
        t.crater = true;
        assert_eq!(y(&t, &m), (1, 0, 0));
        // The bonus of an improvement is not reduced.
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
        t.road = true;
        assert_eq!(y(&t, &m), (3, 0, 1));
        t.river = true;
        assert_eq!(commerce(&t, &m, None, false), 2);
        // A river alone is worth a commerce even on a tile with none.
        let mut r = land(terrain::HILLS);
        r.river = true;
        assert_eq!(commerce(&r, &m, None, false), 1);
        let mut h = land(terrain::HILLS);
        h.mined = true;
        assert_eq!(shields(&h, &m, None, false), 3);
    }

    /// The railroad step needs a positive irrigation or mining bonus, the
    /// railroad itself (or `planning`) and the Railroad job's tech
    /// (`0x5D7341..0x5D7370`, `0x5D7833..0x5D786E`).
    #[test]
    fn railroad_step() {
        let m = civ(row::MONARCHY, 0);
        let mut hills = land(terrain::HILLS);
        hills.mined = true;
        hills.railroad = true;
        assert_eq!(shields(&hills, &m, None, false), 4);
        // No tech: no step.
        let none = Civ {
            knows: NO_TECH,
            ..civ(row::MONARCHY, 0)
        };
        assert_eq!(shields(&hills, &none, None, false), 3);
        // Hills have no irrigation bonus, so irrigating adds nothing and
        // the railroad step is not reached.
        let mut irrigated = land(terrain::HILLS);
        irrigated.irrigated = true;
        irrigated.railroad = true;
        assert_eq!(food(&irrigated, &m, None, false), 1);
        // Desert: base 0, irrigation 1, railroad step 1.
        let mut d = land(terrain::DESERT);
        d.irrigated = true;
        d.railroad = true;
        assert_eq!(food(&d, &m, None, false), 2);
        // A railroad without the improvement does nothing.
        let mut bare = land(terrain::DESERT);
        bare.railroad = true;
        assert_eq!(food(&bare, &m, None, false), 0);
    }

    /// `planning` stands in for the improvement and for the railroad
    /// (`0x5D72C3`, `0x5D7339`, `0x5D7780`, `0x5D7833`, `0x5D7C55`).
    #[test]
    fn planning_assumes_the_improvements() {
        let m = civ(row::MONARCHY, 0);
        let t = land(terrain::PLAINS);
        assert_eq!(tile_yield(&t, &m, None, true), (3, 3, 1));
        let none = Civ {
            knows: NO_TECH,
            ..civ(row::MONARCHY, 0)
        };
        assert_eq!(tile_yield(&t, &none, None, true), (2, 2, 1));
    }

    #[test]
    fn desert_and_the_agricultural_trait() {
        let plain = civ(row::MONARCHY, 0);
        let agri = civ(row::MONARCHY, 1 << trait_bit::AGRICULTURAL);
        let mut d = land(terrain::DESERT);
        assert_eq!(food(&d, &agri, None, false), 0);
        d.irrigated = true;
        assert_eq!(food(&d, &plain, None, false), 1);
        assert_eq!(food(&d, &agri, None, false), 2);
        // Only the desert.
        let mut p = land(terrain::PLAINS);
        p.irrigated = true;
        assert_eq!(food(&p, &agri, None, false), 2);
    }

    #[test]
    fn resources_need_their_tech() {
        let all = civ(row::MONARCHY, 0);
        let none = Civ {
            knows: NO_TECH,
            ..civ(row::MONARCHY, 0)
        };
        let mut wheat = land(terrain::PLAINS);
        wheat.resource = Some(resource::WHEAT);
        assert_eq!(y(&wheat, &none), (3, 1, 0));
        let mut horses = land(terrain::PLAINS);
        horses.resource = Some(resource::HORSES);
        assert_eq!(y(&horses, &none), (1, 1, 0));
        assert_eq!(y(&horses, &all), (1, 1, 1));
        let mut gold = land(terrain::MOUNTAINS);
        gold.resource = Some(resource::GOLD);
        assert_eq!(y(&gold, &none), (0, 1, 4));
        let mut whales = water(terrain::SEA, 400);
        whales.resource = Some(resource::WHALES);
        assert_eq!(y(&whales, &none), (2, 1, 3));
    }

    /// Lakes give +1 food; ocean-like bodies give the three building
    /// bonuses to the working city only (`0x5D7470`, `0x5D747C`,
    /// `0x5D790F`, `0x5D7D79`).
    #[test]
    fn water_bodies_and_harbor_buildings() {
        let m = civ(row::MONARCHY, 0);
        let city = Working {
            harbors: 1,
            offshore_platforms: 1,
            commercial_docks: 1,
            ..Working::default()
        };
        let lake = water(terrain::COAST, LARGE_WATER);
        assert_eq!(tile_yield(&lake, &m, Some(&city), false), (2, 0, 2));
        let sea = water(terrain::SEA, LARGE_WATER + 1);
        assert_eq!(tile_yield(&sea, &m, None, false), (1, 0, 1));
        assert_eq!(tile_yield(&sea, &m, Some(&city), false), (2, 1, 2));
        // A building that is not there adds nothing; two add twice.
        let two = Working {
            harbors: 2,
            ..Working::default()
        };
        assert_eq!(food(&sea, &m, Some(&two), false), 3);
        // Land tiles ignore the buildings.
        assert_eq!(
            tile_yield(&land(terrain::PLAINS), &m, Some(&city), false),
            (1, 1, 0)
        );
    }

    #[test]
    fn bonus_grassland_shield() {
        let m = civ(row::MONARCHY, 0);
        let mut g = land(terrain::GRASSLAND);
        g.shield_bonus = true;
        assert_eq!(shields(&g, &m, None, false), 1);
        let mut p = land(terrain::PLAINS);
        p.shield_bonus = true;
        assert_eq!(shields(&p, &m, None, false), 1);
    }

    #[test]
    fn landmark_tiles_use_their_own_numbers() {
        let m = civ(row::MONARCHY, 0);
        let mut f = land(terrain::FOREST);
        f.mined = true;
        assert_eq!(shields(&f, &m, None, false), 2);
        f.landmark = true;
        assert_eq!(shields(&f, &m, None, false), 4);
        let mut volcano = land(terrain::VOLCANO);
        assert_eq!(shields(&volcano, &m, None, false), 3);
        volcano.landmark = true;
        assert_eq!(shields(&volcano, &m, None, false), 1);
    }

    /// City centre food is fixed at [`FOOD_PER_CITIZEN`] whatever the
    /// terrain, resource or improvement (`0x5D74AE..0x5D7520`), plus one
    /// for an Agricultural owner.
    #[test]
    fn centre_food() {
        let m = civ(row::MONARCHY, 0);
        let mut c = centre(terrain::DESERT, 3);
        assert_eq!(food(&c, &m, None, false), 2);
        c.irrigated = true;
        c.resource = Some(resource::WHEAT);
        assert_eq!(food(&c, &m, None, false), 2);
        let mut agri = centre(terrain::PLAINS, 3);
        agri.centre.as_mut().unwrap().owner_traits = 1 << trait_bit::AGRICULTURAL;
        assert_eq!(food(&agri, &m, None, false), 3);
    }

    /// The Despotism cap takes one off anything above two, in all three
    /// yields (`0x5D75B0`, `0x5D7AB5`, `0x5D7FCB`), and an Agricultural
    /// civ's city centre next to fresh water is exempt from the food cap
    /// (`0x5D759F..0x5D75AE`).
    #[test]
    fn despotism_cap() {
        let d = civ(row::DESPOTISM, 0);
        let m = civ(row::MONARCHY, 0);
        let mut fp = land(terrain::FLOOD_PLAIN);
        fp.irrigated = true;
        fp.railroad = true;
        assert_eq!(food(&fp, &m, None, false), 5);
        assert_eq!(food(&fp, &d, None, false), 4);
        // Two is not above two.
        assert_eq!(food(&land(terrain::GRASSLAND), &d, None, false), 2);
        // Shields: Forest mined landmark 4 -> 3; commerce: coast + road? 2 stays.
        let mut f = land(terrain::FOREST);
        f.landmark = true;
        f.mined = true;
        assert_eq!(shields(&f, &d, None, false), 3);
        assert_eq!(commerce(&water(terrain::COAST, 500), &d, None, false), 2);
        let mut g = water(terrain::COAST, 500);
        g.resource = Some(resource::WHALES);
        assert_eq!(commerce(&g, &d, None, false), 3); // 2 + 2 = 4, minus the cap
                                                      // Anarchy has the same cap.
        let a = civ(row::ANARCHY, 0);
        assert_eq!(food(&fp, &a, None, false), 4);

        // The Agricultural exemption: centre, fresh water, Agricultural civ.
        let agri = civ(row::DESPOTISM, 1 << trait_bit::AGRICULTURAL);
        let mut c = centre(terrain::PLAINS, 3);
        c.centre.as_mut().unwrap().owner_traits = 1 << trait_bit::AGRICULTURAL;
        assert_eq!(food(&c, &agri, None, false), 2);
        c.fresh_water = true;
        assert_eq!(food(&c, &agri, None, false), 3);
        // The exemption belongs to the civ's trait, not to the tile's owner.
        assert_eq!(food(&c, &d, None, false), 2);
    }

    /// City centre shields: the size class (town 0, city 1, metropolis 2)
    /// adds 0, 1 or 2 (3 for an Industrious civ), the tile is worth at
    /// least one (`0x5D79B2..0x5D7A35`). The Industrious test in the city
    /// class is made but its result is dropped.
    #[test]
    fn centre_shields() {
        let m = civ(row::MONARCHY, 0);
        let ind = civ(row::MONARCHY, 1 << trait_bit::INDUSTRIOUS);
        let g = |size| centre(terrain::GRASSLAND, size);
        assert_eq!(shields(&g(3), &m, None, false), 1);
        assert_eq!(shields(&g(6), &m, None, false), 1);
        assert_eq!(shields(&g(7), &m, None, false), 1);
        assert_eq!(shields(&g(12), &m, None, false), 1);
        assert_eq!(shields(&g(13), &m, None, false), 2);
        assert_eq!(shields(&g(13), &ind, None, false), 3);
        assert_eq!(shields(&g(12), &ind, None, false), 1);
        // Hills centre, city class: base 1 + 1.
        assert_eq!(shields(&centre(terrain::HILLS, 8), &m, None, false), 2);
        // A mine under a city adds as usual.
        let mut h = centre(terrain::HILLS, 3);
        h.mined = true;
        assert_eq!(shields(&h, &m, None, false), 3);
    }

    /// City centre commerce: class steps (Commercial gives 3 more in the
    /// metropolis class and 2 more in the city class), a floor of 1 and
    /// of 4 for the capital, and +1 for a Seafaring owner on a large water
    /// body (`0x5D7E1F..0x5D7F56`).
    #[test]
    fn centre_commerce() {
        let m = civ(row::MONARCHY, 0);
        let com = civ(row::MONARCHY, 1 << trait_bit::COMMERCIAL);
        let g = |size| centre(terrain::GRASSLAND, size);
        // Grassland with a road: base 0 + road 1.
        assert_eq!(commerce(&g(3), &m, None, false), 1);
        assert_eq!(commerce(&g(8), &m, None, false), 2);
        assert_eq!(commerce(&g(8), &com, None, false), 4);
        assert_eq!(commerce(&g(14), &m, None, false), 3);
        assert_eq!(commerce(&g(14), &com, None, false), 6);
        // The floor: no road, no bonus, still one.
        let mut bare = g(3);
        bare.road = false;
        assert_eq!(commerce(&bare, &m, None, false), 1);
        // The capital is worth four at least.
        let mut cap = g(3);
        cap.centre.as_mut().unwrap().is_capital = true;
        assert_eq!(commerce(&cap, &m, None, false), 4);
        // ... and more when the steps take it higher.
        let mut big = g(14);
        big.centre.as_mut().unwrap().is_capital = true;
        assert_eq!(commerce(&big, &com, None, false), 6);
        // Seafaring: only with a large adjacent water body, by the owner's
        // trait.
        let mut sea = g(3);
        sea.centre.as_mut().unwrap().large_water_adjacent = true;
        assert_eq!(commerce(&sea, &m, None, false), 1);
        sea.centre.as_mut().unwrap().owner_traits = 1 << trait_bit::SEAFARING;
        assert_eq!(commerce(&sea, &m, None, false), 2);
    }

    /// The Golden Age adds one shield and one commerce to every tile that
    /// has some, and never any food (`0x5D7A52`, `0x5D7F75`; the food
    /// function has no such clause).
    #[test]
    fn golden_age() {
        let gold = Civ {
            golden_age: true,
            ..civ(row::MONARCHY, 0)
        };
        assert_eq!(y(&land(terrain::PLAINS), &gold), (1, 2, 0));
        assert_eq!(y(&land(terrain::GRASSLAND), &gold), (2, 0, 0));
        assert_eq!(y(&water(terrain::COAST, 500), &gold), (1, 0, 3));
    }

    /// Republic and Democracy: +1 commerce on tiles that have any; the
    /// Colossus city adds its step first (`0x5D7F97`, then `0x5D7FC3`).
    #[test]
    fn trade_bonus_and_colossus() {
        let rep = civ(row::REPUBLIC, 0);
        let coast = water(terrain::COAST, 500);
        assert_eq!(commerce(&coast, &rep, None, false), 3);
        assert_eq!(commerce(&land(terrain::GRASSLAND), &rep, None, false), 0);
        let colossus = Working {
            colossus: 1,
            ..Working::default()
        };
        assert_eq!(commerce(&coast, &rep, Some(&colossus), false), 4);
        // A tile with no commerce gets neither.
        assert_eq!(
            commerce(&land(terrain::PLAINS), &rep, Some(&colossus), false),
            0
        );
        // Under Despotism the cap follows the steps: 2 + 1 = 3 -> 2.
        let desp = civ(row::DESPOTISM, 0);
        assert_eq!(commerce(&coast, &desp, Some(&colossus), false), 2);
    }

    /// Mobilization: +1 shield on tiles that already give some while the
    /// city builds a military unit (`0x5D7A5F..0x5D7A95`).
    #[test]
    fn mobilization_step() {
        let m = civ(row::MONARCHY, 0);
        let war = Working {
            military_build: true,
            ..Working::default()
        };
        assert_eq!(shields(&land(terrain::PLAINS), &m, Some(&war), false), 2);
        assert_eq!(shields(&land(terrain::GRASSLAND), &m, Some(&war), false), 0);
        assert_eq!(shields(&land(terrain::PLAINS), &m, None, false), 1);
    }
}
