//! The economy: every square yields food, shields, and gold, and a city gathers all of them
//! from the squares of its preset border region (see [`crate::borders`]).
//!
//! * **Food** feeds the citizens (2 each). The surplus fills the city's granary and the city
//!   grows when it is full; a shortfall drains it and the city starves a citizen at a time.
//! * **Shields** are production. They finish what the city is building and add up to the
//!   nation's industrial output, which is one way to win.
//! * **Gold** is the nation's income.
//!
//! The land decides what a square yields; workers improve it: a mine adds shields, a farm adds
//! food, a road adds gold, and a railroad adds shields and gold on top of its road. Every
//! square of the region counts, so there is no citizen assignment. The only link to borders is
//! [`crate::terrain::Tile::claim`], so whatever shape a region takes, the economy follows it,
//! and a conquered city takes its harvest to its new owner along with its land.
use crate::{
    City, Game, Id,
    terrain::{Cover, Terrain, Tile},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ops::{Add, AddAssign};

/// Food a citizen eats each day.
pub const FOOD_PER_CITIZEN: i32 = 2;

/// An amount of each resource, per day.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Yield {
    pub food: i32,
    pub shields: i32,
    pub gold: i32,
}
impl Yield {
    pub const fn new(food: i32, shields: i32, gold: i32) -> Self {
        Self {
            food,
            shields,
            gold,
        }
    }
}
impl Add for Yield {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self::new(
            self.food + other.food,
            self.shields + other.shields,
            self.gold + other.gold,
        )
    }
}
impl AddAssign for Yield {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}
impl std::iter::Sum for Yield {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::default(), Add::add)
    }
}

/// What unimproved ground yields, as food/shields/gold from Civ3's terrain table. Relief and
/// cover replace the soil beneath them: mountains win over everything, then forest, jungle
/// and marsh, then hills; bare flat ground yields what its base terrain does.
pub const GRASSLAND: Yield = Yield::new(2, 0, 0);
pub const PLAINS: Yield = Yield::new(1, 1, 0);
pub const DESERT: Yield = Yield::new(0, 1, 0);
pub const TUNDRA: Yield = Yield::new(1, 0, 0);
pub const HILLS: Yield = Yield::new(1, 0, 0);
pub const MOUNTAINS: Yield = Yield::new(0, 1, 0);
pub const FOREST: Yield = Yield::new(1, 2, 0);
pub const JUNGLE: Yield = Yield::new(1, 0, 0);
pub const MARSH: Yield = Yield::new(1, 0, 0);
/// Coast, sea, and ocean alike.
pub const WATER: Yield = Yield::new(1, 0, 2);
/// A river along any edge of a land square adds trade.
pub const RIVER: Yield = Yield::new(0, 0, 1);

/// What each improvement adds to its square. A railroad is built on a road and keeps it, so a
/// railed square collects both rows.
pub const IMPROVEMENT_YIELD: [(u8, Yield); 4] = [
    (Tile::ROAD, Yield::new(0, 0, 1)),
    (Tile::RAIL, Yield::new(0, 1, 1)),
    (Tile::MINE, Yield::new(0, 2, 0)),
    (Tile::FARM, Yield::new(1, 0, 0)),
];

/// Food a city of this size must store to grow by one citizen. Tuned so that a typical new
/// city (a surplus of about 20 a day) grows every few days at first, then slows as the land
/// fills, instead of every city gaining a citizen on a timer.
pub fn granary_size(population: i32) -> i32 {
    20 * (population.max(1) + 1)
}
/// A city starts, and restarts after famine, with half a granary, so a poor city shrinks over
/// days rather than all at once.
pub fn starting_granary(population: i32) -> i32 {
    granary_size(population) / 2
}

/// The yield of the bare ground. Every base terrain is named, so adding one to the map model
/// stops the build until its yield is chosen.
pub fn land_yield(tile: &Tile) -> Yield {
    if tile.is_water() {
        return WATER;
    }
    if tile.is_mountain() {
        return MOUNTAINS;
    }
    match tile.cover {
        Cover::Forest => FOREST,
        Cover::Jungle => JUNGLE,
        Cover::Marsh => MARSH,
        Cover::Bare if tile.is_hills() => HILLS,
        Cover::Bare => match tile.terrain {
            Terrain::Grass => GRASSLAND,
            Terrain::Plains => PLAINS,
            Terrain::Desert => DESERT,
            Terrain::Tundra => TUNDRA,
            Terrain::Ocean | Terrain::Sea | Terrain::Coast => WATER,
        },
    }
}

/// The yield added by a set of improvement bits.
pub fn improvement_yield(improvements: u8) -> Yield {
    IMPROVEMENT_YIELD
        .iter()
        .filter(|(flag, _)| improvements & flag != 0)
        .map(|(_, bonus)| *bonus)
        .sum()
}

/// What one square yields: its ground, a river along its edges (`river`, see
/// [`crate::terrain::Map::river_mask`]), and its improvements. A city's own square counts as
/// roaded and, on open ground, farmed (Civ3's free irrigation), and never yields less than one
/// shield.
pub fn tile_yield(tile: &Tile, river: bool, city_centre: bool) -> Yield {
    let mut improvements = tile.improvements;
    if city_centre {
        improvements |= Tile::ROAD;
        let open = tile.cover == Cover::Bare && !tile.is_mountain();
        if open && improvements & Tile::MINE == 0 {
            improvements |= Tile::FARM;
        }
    }
    let mut total = land_yield(tile) + improvement_yield(improvements);
    if river && tile.is_land() {
        total += RIVER;
    }
    if city_centre {
        total.shields = total.shields.max(1);
    }
    total
}

/// What a city's population did over a day.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Grew,
    Starved,
}

impl City {
    /// Production per day: the land's shields plus the city's own industry.
    pub fn shields(&self) -> i32 {
        self.harvest.shields + self.industry
    }
    pub fn food_eaten(&self) -> i32 {
        self.population * FOOD_PER_CITIZEN
    }
    /// Food left after feeding everyone. Negative means famine is coming.
    pub fn food_surplus(&self) -> i32 {
        self.harvest.food - self.food_eaten()
    }
    pub fn granary_size(&self) -> i32 {
        granary_size(self.population)
    }

    /// One day of eating: store the surplus, then grow if the granary is full or starve a
    /// citizen if it has run dry. A city never starves below one citizen.
    pub(crate) fn eat(&mut self) -> Option<Change> {
        self.granary += self.food_surplus();
        if self.granary >= self.granary_size() {
            self.population += 1;
            self.granary = 0;
            Some(Change::Grew)
        } else if self.granary < 0 {
            if self.population > 1 {
                self.population -= 1;
                self.granary = starting_granary(self.population);
                Some(Change::Starved)
            } else {
                self.granary = 0;
                None
            }
        } else {
            None
        }
    }
}

impl Game {
    /// What every city gathers from the squares its region holds. A single pass over the map,
    /// whatever the number of cities.
    pub fn harvests(&self) -> BTreeMap<Id, Yield> {
        let mut totals: BTreeMap<Id, Yield> = BTreeMap::new();
        for tile in self.map.tiles.iter().filter(|t| t.claim != 0) {
            let Some(city) = self.cities.get(&tile.claim) else {
                continue;
            };
            let river = self.map.river_mask(tile.position) != 0;
            *totals.entry(city.id).or_default() +=
                tile_yield(tile, river, city.position == tile.position);
        }
        totals
    }

    /// Recount every city's harvest. The count is derived from the map, so it is redone
    /// whenever the map or the cities may have changed (after each command and before each
    /// day's income), never trusted from an earlier turn.
    pub(crate) fn refresh_harvest(&mut self) {
        let mut totals = self.harvests();
        for city in self.cities.values_mut() {
            city.harvest = totals.remove(&city.id).unwrap_or_default();
        }
    }

    /// Gold a nation's cities bring in each day, before the turn script's adjustment.
    pub fn income(&self, owner: Id) -> i32 {
        self.cities
            .values()
            .filter(|c| c.owner == owner)
            .map(|c| c.harvest.gold)
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{arena, arena_rows, rules, spawn};
    use crate::{
        Command, Job, TickRules,
        terrain::{Coord, Map, Relief},
    };

    const RED: Id = 1;
    const BLUE: Id = 2;

    fn run(game: &mut Game, command: Command) {
        game.apply(RED, command, &rules(), TickRules::default())
            .expect("command accepted");
    }
    fn end_turn(game: &mut Game) {
        run(game, Command::EndTurn);
    }
    fn tile(terrain: Terrain, relief: Relief, cover: Cover) -> Tile {
        let mut tile = Tile::new(Coord::new(0, 0), terrain);
        tile.relief = relief;
        tile.cover = cover;
        tile
    }
    fn flat(terrain: Terrain) -> Tile {
        tile(terrain, Relief::Flat, Cover::Bare)
    }
    /// Red's capital stands at (1,1) in every arena.
    fn red_capital(game: &Game) -> &City {
        &game.cities[&game.city_at(Coord::new(1, 1)).unwrap()]
    }
    /// All grass, with room around red's capital (level 3, 37 squares in the open).
    fn open_ground() -> Vec<String> {
        vec![".".repeat(14); 14]
    }
    fn red_soldiers(game: &Game) -> usize {
        game.units
            .values()
            .filter(|u| u.owner == RED && u.kind == "infantry")
            .count()
    }

    #[test]
    fn the_land_decides_what_a_square_yields() {
        // Civ3's terrain table, as food/shields/gold.
        for (terrain, expected) in [
            (Terrain::Grass, (2, 0, 0)),
            (Terrain::Plains, (1, 1, 0)),
            (Terrain::Desert, (0, 1, 0)),
            (Terrain::Tundra, (1, 0, 0)),
            (Terrain::Coast, (1, 0, 2)),
            (Terrain::Sea, (1, 0, 2)),
            (Terrain::Ocean, (1, 0, 2)),
        ] {
            let (food, shields, gold) = expected;
            assert_eq!(
                land_yield(&flat(terrain)),
                Yield::new(food, shields, gold),
                "{terrain:?}"
            );
        }
        let on_grass = |relief, cover| land_yield(&tile(Terrain::Grass, relief, cover));
        assert_eq!(on_grass(Relief::Hills, Cover::Bare), Yield::new(1, 0, 0));
        assert_eq!(
            on_grass(Relief::Mountains, Cover::Bare),
            Yield::new(0, 1, 0)
        );
        assert_eq!(on_grass(Relief::Flat, Cover::Forest), Yield::new(1, 2, 0));
        assert_eq!(on_grass(Relief::Flat, Cover::Jungle), Yield::new(1, 0, 0));
        assert_eq!(on_grass(Relief::Flat, Cover::Marsh), Yield::new(1, 0, 0));
        // Cover and relief replace the soil beneath: desert forest is a forest, and a forest
        // on a hill is still a forest, but mountains hide whatever grows on them.
        assert_eq!(
            land_yield(&tile(Terrain::Desert, Relief::Flat, Cover::Forest)),
            land_yield(&tile(Terrain::Grass, Relief::Flat, Cover::Forest))
        );
        assert_eq!(
            land_yield(&tile(Terrain::Plains, Relief::Hills, Cover::Forest)),
            FOREST
        );
        assert_eq!(
            land_yield(&tile(Terrain::Grass, Relief::Mountains, Cover::Forest)),
            MOUNTAINS
        );
    }

    #[test]
    fn rivers_add_gold_to_land_but_not_to_water() {
        let grass = flat(Terrain::Grass);
        assert_eq!(
            tile_yield(&grass, true, false),
            tile_yield(&grass, false, false) + Yield::new(0, 0, 1)
        );
        let sea = flat(Terrain::Sea);
        assert_eq!(
            tile_yield(&sea, true, false),
            tile_yield(&sea, false, false)
        );
        // The squares either side of a river edge both count it.
        let mut map = Map::filled(3, 1, Terrain::Grass);
        map.get_mut(Coord::new(0, 0)).unwrap().river = Tile::RIVER_E;
        let harvest = |x| {
            let tile = map.get(Coord::new(x, 0)).unwrap();
            tile_yield(tile, map.river_mask(tile.position) != 0, false).gold
        };
        assert_eq!([harvest(0), harvest(1), harvest(2)], [1, 1, 0]);
    }

    #[test]
    fn each_improvement_adds_its_own_resource() {
        let mut grass = flat(Terrain::Grass);
        let bare = tile_yield(&grass, false, false);
        let with = |tile: &mut Tile, bits: u8| {
            tile.improvements = bits;
            tile_yield(tile, false, false)
        };
        // road = gold, farm = food, mine = shields, railroad = shields and gold
        assert_eq!(with(&mut grass, Tile::ROAD), bare + Yield::new(0, 0, 1));
        assert_eq!(with(&mut grass, Tile::FARM), bare + Yield::new(1, 0, 0));
        assert_eq!(with(&mut grass, Tile::MINE), bare + Yield::new(0, 2, 0));
        // A railroad keeps its road, so it adds to the road's gold.
        assert_eq!(
            with(&mut grass, Tile::ROAD | Tile::RAIL),
            bare + Yield::new(0, 1, 2)
        );
        let mut mountain = tile(Terrain::Grass, Relief::Mountains, Cover::Bare);
        assert_eq!(with(&mut mountain, Tile::MINE), Yield::new(0, 3, 0));
        assert_eq!(
            with(&mut mountain, Tile::MINE | Tile::ROAD | Tile::RAIL),
            Yield::new(0, 4, 2)
        );
    }

    #[test]
    fn a_city_square_is_roaded_farmed_and_never_idle() {
        let centre = |t: Tile| tile_yield(&t, false, true);
        assert_eq!(centre(flat(Terrain::Grass)), Yield::new(3, 1, 1));
        assert_eq!(centre(flat(Terrain::Plains)), Yield::new(2, 1, 1));
        assert_eq!(centre(flat(Terrain::Desert)), Yield::new(1, 1, 1));
        assert_eq!(
            centre(tile(Terrain::Grass, Relief::Hills, Cover::Bare)),
            Yield::new(2, 1, 1)
        );
        // No free irrigation under a forest or on a mountain, but the road stays.
        let forest = tile(Terrain::Grass, Relief::Flat, Cover::Forest);
        assert_eq!(centre(forest), Yield::new(1, 2, 1));
        let mountain = tile(Terrain::Grass, Relief::Mountains, Cover::Bare);
        assert_eq!(centre(mountain), Yield::new(0, 1, 1));
        // A free farm never doubles up with a mine the scenario put there.
        let mut mined = flat(Terrain::Desert);
        mined.improvements = Tile::MINE;
        assert_eq!(centre(mined), Yield::new(0, 3, 1));
    }

    #[test]
    fn a_city_gathers_every_square_of_its_region_and_only_those() {
        let mut game = arena_rows(&open_ground());
        let id = red_capital(&game).id;
        let region = game.territory_of(id);
        let squares = region.len() as i32;
        assert!(squares >= 9, "a city holds its preset region");
        // Open grass: 2 food a square, and the city square adds a farm, a road, and a shield.
        assert_eq!(
            red_capital(&game).harvest,
            Yield::new(2 * squares + 1, 1, 1),
            "{squares} squares"
        );
        // Land the city does not hold gives it nothing, however it is improved.
        let outside = (0..14)
            .flat_map(|y| (0..14).map(move |x| Coord::new(x, y)))
            .find(|p| game.map.get(*p).unwrap().claim == 0)
            .expect("open land beyond both regions");
        game.map.get_mut(outside).unwrap().improvements = Tile::ROAD | Tile::RAIL;
        game.refresh_harvest();
        assert_eq!(red_capital(&game).harvest.gold, 1);
        // Land it does hold does, resource by resource.
        let held: Vec<Coord> = region
            .into_iter()
            .filter(|p| *p != Coord::new(1, 1))
            .collect();
        game.map.get_mut(held[0]).unwrap().improvements = Tile::ROAD | Tile::RAIL;
        game.map.get_mut(held[1]).unwrap().improvements = Tile::FARM;
        game.map.get_mut(held[2]).unwrap().improvements = Tile::MINE;
        game.refresh_harvest();
        assert_eq!(
            red_capital(&game).harvest,
            Yield::new(2 * squares + 1, 1, 1) + Yield::new(1, 1 + 2, 2)
        );
    }

    #[test]
    fn the_harvest_follows_the_claim_whatever_shape_it_takes() {
        // The economy reads only which city claims a square, so whoever reshapes the borders
        // reshapes the harvest with them and nothing else has to change.
        let mut game = arena_rows(&open_ground());
        let (red, blue) = (
            red_capital(&game).id,
            game.city_at(Coord::new(12, 12)).unwrap(),
        );
        let held = *game
            .territory_of(red)
            .iter()
            .find(|p| **p != Coord::new(1, 1))
            .unwrap();
        let (red_before, blue_before) = (game.cities[&red].harvest, game.cities[&blue].harvest);
        game.map.get_mut(held).unwrap().claim = blue;
        game.refresh_harvest();
        assert_eq!(game.cities[&red].harvest, red_before + Yield::new(-2, 0, 0));
        assert_eq!(
            game.cities[&blue].harvest,
            blue_before + Yield::new(2, 0, 0)
        );
        // An unclaimed square, or a claim by a city that no longer exists, counts for no one.
        game.map.get_mut(held).unwrap().claim = 0;
        game.map.get_mut(Coord::new(2, 1)).unwrap().claim = 999;
        game.refresh_harvest();
        assert_eq!(game.cities[&red].harvest, red_before + Yield::new(-4, 0, 0));
        assert_eq!(game.cities[&blue].harvest, blue_before);
    }

    #[test]
    fn neighbouring_cities_never_count_a_square_twice() {
        let mut game = arena(24, 8);
        game.add_city(RED, Coord::new(5, 1), "Near".into());
        game.assign_start_territory();
        game.refresh_harvest();
        let claimed = game.map.tiles.iter().filter(|t| t.claim != 0).count() as i32;
        let total: Yield = game.cities.values().map(|c| c.harvest).sum();
        // Grass gives 2 food a square; each of the three city squares adds a farm, a road,
        // and a shield.
        assert_eq!(total, Yield::new(2 * claimed + 3, 3, 3));
        for city in game.cities.values() {
            let own = game.territory_of(city.id).len() as i32;
            assert_eq!(city.harvest.food, 2 * own + 1, "{}", city.name);
        }
    }

    #[test]
    fn workers_raise_the_harvest_the_moment_a_job_is_done() {
        let mut game = arena_rows(&open_ground());
        let (road, mine) = (Coord::new(3, 1), Coord::new(2, 2));
        game.map.get_mut(mine).unwrap().relief = Relief::Mountains;
        game.refresh_harvest();
        for p in [road, mine] {
            assert_eq!(game.map.get(p).unwrap().claim, red_capital(&game).id);
        }
        let before = red_capital(&game).harvest;
        let worker = spawn(&mut game, RED, "worker", road.x, road.y);
        let job = |job| Command::Work { unit: worker, job };
        run(&mut game, job(Job::Road));
        assert_eq!(red_capital(&game).harvest, before, "still being laid");
        end_turn(&mut game);
        end_turn(&mut game);
        assert!(game.map.get(road).unwrap().has_road());
        let roaded = before + Yield::new(0, 0, 1);
        assert_eq!(red_capital(&game).harvest, roaded, "a road is gold");
        // A mine on the mountain: shields. Mountains take three times as long to dig.
        game.set_position(worker, mine);
        run(&mut game, job(Job::Mine));
        let mut days = 0;
        while game.map.get(mine).unwrap().improvements & Tile::MINE == 0 {
            assert!(days < 40, "the mine never finished");
            assert_eq!(red_capital(&game).harvest, roaded, "until it is done");
            end_turn(&mut game);
            days += 1;
        }
        assert_eq!(red_capital(&game).harvest, roaded + Yield::new(0, 2, 0));
        // A farm on open ground beside the city: food. A coast, lake, or farm must be near.
        game.map.get_mut(Coord::new(1, 2)).unwrap().improvements = Tile::FARM;
        game.refresh_harvest();
        assert_eq!(
            red_capital(&game).harvest,
            roaded + Yield::new(1, 2, 0),
            "a farm is food"
        );
    }

    #[test]
    fn gold_and_shields_are_paid_each_day() {
        let mut game = arena_rows(&open_ground());
        let id = red_capital(&game).id;
        let gold = red_capital(&game).harvest.gold;
        let shields = red_capital(&game).shields();
        assert_eq!(
            (gold, shields),
            (1, 1 + 4),
            "the road on the city square; its shield and the city's own industry"
        );
        assert_eq!(game.income(RED), gold);
        let (start, output) = (game.factions[&RED].gold, game.factions[&RED].industry);
        game.cities.get_mut(&id).unwrap().production = Some("infantry".into());
        end_turn(&mut game);
        assert_eq!(game.factions[&RED].gold, start + gold);
        assert_eq!(game.factions[&RED].industry, output + shields);
        assert_eq!(red_capital(&game).progress, shields);
        // The turn script scales a nation's whole income once.
        let start = game.factions[&RED].gold;
        let boom = TickRules {
            income_percent: 300,
        };
        game.apply(RED, Command::EndTurn, &rules(), boom).unwrap();
        assert_eq!(game.factions[&RED].gold, start + 3 * gold);
    }

    #[test]
    fn production_comes_from_shields() {
        let mut game = arena_rows(&open_ground());
        let id = red_capital(&game).id;
        let cost = rules().units["infantry"].cost;
        game.cities.get_mut(&id).unwrap().production = Some("infantry".into());
        let per_day = red_capital(&game).shields();
        let days = (cost + per_day - 1) / per_day;
        for _ in 1..days {
            end_turn(&mut game);
        }
        assert_eq!(red_soldiers(&game), 0, "not done after {} days", days - 1);
        end_turn(&mut game);
        assert_eq!(red_soldiers(&game), 1, "done on day {days}");
        // Railroads and mines on the city's land add up to more shields every day.
        for p in game.territory_of(id).iter().take(8) {
            if *p != Coord::new(1, 1) {
                game.map.get_mut(*p).unwrap().improvements = Tile::ROAD | Tile::RAIL;
            }
        }
        game.refresh_harvest();
        let railed = game.territory_of(id).iter().take(8).count() as i32 - 1;
        assert_eq!(red_capital(&game).shields(), per_day + railed);
    }

    #[test]
    fn surplus_food_grows_a_city() {
        let mut game = arena_rows(&open_ground());
        let id = red_capital(&game).id;
        let city = game.cities.get_mut(&id).unwrap();
        city.harvest = Yield::new(10, 1, 1);
        city.population = 3;
        city.granary = 0;
        assert_eq!(city.food_surplus(), 4);
        // A granary of 80 fills at 4 a day.
        for day in 1..20 {
            assert_eq!(city.eat(), None, "day {day}");
        }
        assert_eq!(city.eat(), Some(Change::Grew));
        assert_eq!((city.population, city.granary), (4, 0));
    }

    #[test]
    fn a_city_cannot_outgrow_its_land() {
        let mut game = arena_rows(&open_ground());
        let id = red_capital(&game).id;
        let city = game.cities.get_mut(&id).unwrap();
        city.harvest = Yield::new(25, 1, 1);
        // 25 food feeds 12 citizens. A 13th is hungry, starves, and the city regrows.
        for _ in 0..2000 {
            city.eat();
            assert!(city.population <= 13);
        }
        assert!(matches!(city.population, 12 | 13), "{}", city.population);
    }

    #[test]
    fn famine_shrinks_a_city_slowly_and_never_below_one() {
        let mut game = arena_rows(&open_ground());
        let id = red_capital(&game).id;
        let city = game.cities.get_mut(&id).unwrap();
        city.harvest = Yield::new(3, 1, 1); // the city square alone
        city.population = 5;
        city.granary = starting_granary(5);
        assert_eq!(city.food_surplus(), -7);
        let mut first = None;
        for day in 1..=400 {
            let change = city.eat();
            if first.is_none() && change.is_some() {
                first = Some((day, change, city.population));
            }
            assert!(city.population >= 1 && city.granary >= 0, "day {day}");
        }
        // The 60 stored food lasts eight days at 7 short; the ninth starves one citizen.
        assert_eq!(first, Some((9, Some(Change::Starved), 4)));
        assert!(
            city.population <= 2,
            "the square's own food feeds one or two"
        );
    }

    #[test]
    fn a_captured_city_pays_its_harvest_to_its_new_owner() {
        let mut game = arena(14, 14);
        let blue = game.city_at(Coord::new(12, 12)).unwrap();
        let (red_gold, blue_gold) = (
            red_capital(&game).harvest.gold,
            game.cities[&blue].harvest.gold,
        );
        assert!(red_gold > 0 && blue_gold > 0);
        game.transfer_city(blue, RED);
        assert_eq!(game.income(RED), red_gold + blue_gold);
        assert_eq!(game.income(BLUE), 0);
        let start = game.factions[&RED].gold;
        end_turn(&mut game);
        assert_eq!(game.factions[&RED].gold, start + red_gold + blue_gold);
    }

    #[test]
    fn a_new_city_harvests_only_land_nobody_held() {
        let mut game = arena(20, 12);
        let capital = red_capital(&game).harvest;
        let pioneer = spawn(&mut game, RED, "pioneer", 6, 6);
        run(
            &mut game,
            Command::FoundCity {
                unit: pioneer,
                name: "Sixth".into(),
            },
        );
        // Counted the moment the city exists, not a day later: a level-2 region of 21 squares.
        let new = game.city_at(Coord::new(6, 6)).unwrap();
        assert_eq!(game.cities[&new].harvest, Yield::new(2 * 21 + 1, 1, 1));
        assert_eq!(red_capital(&game).harvest, capital, "nothing was taken");
    }

    #[test]
    fn players_see_their_own_harvest_where_the_map_is_still_dark() {
        let mut game = arena(24, 24);
        let truth = red_capital(&game).harvest;
        // The fog hides the claims on the far edge of a big city's land from its owner, so the
        // client cannot recount; it reads the figure the host counted on the real map.
        game.factions.get_mut(&RED).unwrap().explored.clear();
        let view = game.view(RED);
        assert_eq!(view.map.get(Coord::new(2, 2)).unwrap().claim, 0, "masked");
        assert_eq!(view.cities[&red_capital(&game).id].harvest, truth);
    }

    #[test]
    fn harvest_is_saved_and_older_saves_still_load() {
        let game = arena(16, 16);
        let json = serde_json::to_string(&game).unwrap();
        assert_eq!(serde_json::from_str::<Game>(&json).unwrap(), game);
        // A city saved before this economy existed has neither field. It reads as empty until
        // the next command or turn counts the land.
        let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
        for city in value["cities"].as_object_mut().unwrap().values_mut() {
            let city = city.as_object_mut().unwrap();
            city.remove("harvest").expect("saved");
            city.remove("granary").expect("saved");
        }
        let mut old: Game = serde_json::from_value(value).unwrap();
        assert_eq!(old.cities[&1].harvest, Yield::default());
        assert_eq!(old.cities[&1].granary, 0);
        old.refresh_harvest();
        assert_eq!(old.cities[&1].harvest, game.cities[&1].harvest);
    }
}
