//! Cultural borders, fixed per city.
//!
//! Every city owns a preset region of the map, sized by its border level (Civ3's culture
//! levels 1 to 6). Regions never grow or shrink with time. A city's region belongs to whoever
//! holds the city, so conquering a city transfers its land, and neighbouring regions of one
//! nation read as a single territory because ownership is stored per nation.
//!
//! Level `L` reaches every tile within squared distance [`REACH_SQUARED`]`[L - 1]` on the
//! logical grid, which is Civ3's border shape: 9, 21, 37, 61, 89, and 137 tiles.
//!
//! A tile no city reaches can still end up owned: Civ3's orphan resolution fills a one-tile
//! hole its claimed neighbours surround (see [`Game::fill_border_gaps`]).
use crate::{
    City, Game, Id,
    terrain::{Coord, Terrain},
};

pub const MIN_LEVEL: u8 = 1;
pub const MAX_LEVEL: u8 = 6;
/// Squared tile distance reached by border levels 1 to 6.
pub const REACH_SQUARED: [i32; 6] = [2, 5, 10, 18, 26, 41];
/// Open water is only claimed this close to a city (Civ3's level-2 reach).
const WATER_REACH_SQUARED: i32 = 5;

/// Squared reach of a border level, clamped into the valid range.
pub fn reach_squared(level: u8) -> i32 {
    REACH_SQUARED[usize::from(level.clamp(MIN_LEVEL, MAX_LEVEL)) - 1]
}
/// The border level of a city with no explicit level: bigger cities and capitals reach farther.
pub fn default_level(population: i32, capital: bool) -> u8 {
    let level = match population {
        ..=3 => 2,
        4..=6 => 3,
        _ => 4,
    };
    (level + u8::from(capital)).min(MAX_LEVEL)
}
/// Percent of a held square's output its city can gather, from the square's squared distance
/// to the city. Everything within the city's own border reach is worked in full; past it the
/// share falls with the square of the distance, so land twice as far out as the border yields
/// a quarter and land three times as far a ninth. Only regional fill and orphan gaps hold
/// squares that far out.
pub fn reach_percent(level: u8, distance_squared: i32) -> i32 {
    let reach = reach_squared(level);
    if distance_squared <= reach {
        100
    } else {
        100 * reach / distance_squared
    }
}

impl Game {
    /// Every tile a city's border can reach, with its squared distance from the city.
    fn border_reach(&self, city: &City) -> Vec<(Coord, i32)> {
        let reach = reach_squared(city.border);
        let radius = (1..).find(|r| r * r > reach).unwrap_or(1) - 1;
        let mut tiles = Vec::new();
        for y in city.position.y - radius..=city.position.y + radius {
            for x in city.position.x - radius..=city.position.x + radius {
                let Some(p) = self.map.normalize(Coord::new(x, y)) else {
                    continue;
                };
                let d = self.map.distance_squared(p, city.position);
                let Some(tile) = self.map.get(p) else {
                    continue;
                };
                if d <= reach && (tile.is_land() || d <= WATER_REACH_SQUARED) {
                    tiles.push((p, d));
                }
            }
        }
        tiles
    }

    /// Lay out every city's region at once. Each tile goes to the nearest city that reaches
    /// it; a tie goes to the higher border level, then the larger city, then the older one.
    /// One-tile holes the surround agrees on are filled after, like Civ3's orphans.
    pub(crate) fn assign_start_territory(&mut self) {
        for tile in &mut self.map.tiles {
            tile.claim = 0;
            tile.owner = 0;
        }
        let beats = |game: &Game, challenger: &City, d: i32, holder: Id, held: i32| {
            let held_city = &game.cities[&holder];
            (
                d,
                std::cmp::Reverse(challenger.border),
                std::cmp::Reverse(challenger.population),
            ) < (
                held,
                std::cmp::Reverse(held_city.border),
                std::cmp::Reverse(held_city.population),
            )
        };
        let mut best: std::collections::HashMap<usize, i32> = std::collections::HashMap::new();
        let ids: Vec<Id> = self.cities.keys().copied().collect();
        for id in ids {
            let city = self.cities[&id].clone();
            for (p, d) in self.border_reach(&city) {
                let i = self.map.index(p).expect("reach is on the map");
                let holder = self.map.tiles[i].claim;
                if holder == 0 || beats(self, &city, d, holder, best[&i]) {
                    self.map.tiles[i].claim = id;
                    best.insert(i, d);
                }
            }
        }
        // A city always stands on its own land, whatever its neighbours reach.
        let seats: Vec<(Id, Coord)> = self.cities.values().map(|c| (c.id, c.position)).collect();
        for (id, position) in seats {
            self.map
                .get_mut(position)
                .expect("city is on the map")
                .claim = id;
        }
        self.fill_border_gaps();
    }

    /// Fill named historical regions left outside normal cultural borders. Each remaining
    /// land tile goes to the nearest city owned by its region's nation; unnamed fringe and
    /// regions without such a city stay unclaimed.
    pub(crate) fn fill_regional_territory(&mut self) {
        let cities: Vec<(Id, Id, Coord, u8, i32)> = self
            .cities
            .values()
            .map(|city| {
                (
                    city.id,
                    city.owner,
                    city.position,
                    city.border,
                    city.population,
                )
            })
            .collect();
        for i in 0..self.map.tiles.len() {
            let (position, land, claimed, region) = {
                let tile = &self.map.tiles[i];
                (tile.position, tile.is_land(), tile.claim, tile.region)
            };
            if !land || claimed != 0 || region == 0 {
                continue;
            }
            let Some(nation) = self
                .regions
                .get(usize::from(region) - 1)
                .map(|region| region.nation)
            else {
                continue;
            };
            let nearest = cities
                .iter()
                .filter(|(_, owner, ..)| *owner == nation)
                .map(|city| {
                    (
                        self.map.distance_squared(city.2, position),
                        std::cmp::Reverse(city.3),
                        std::cmp::Reverse(city.4),
                        city.0,
                    )
                })
                .min();
            if let Some((_, _, _, city)) = nearest {
                self.map.tiles[i].claim = city;
                self.map.tiles[i].owner = nation;
            }
        }
        self.fill_border_gaps();
    }

    /// A new city claims only land nobody holds yet, plus the square it stands on.
    pub(crate) fn claim_for_new_city(&mut self, id: Id) {
        let city = self.cities[&id].clone();
        for (p, _) in self.border_reach(&city) {
            let tile = self.map.get_mut(p).expect("reach is on the map");
            if tile.claim == 0 || p == city.position {
                tile.claim = id;
                tile.owner = city.owner;
            }
        }
        self.fill_border_gaps();
    }

    /// Fill one-tile holes Civ3's orphan resolution (`0x5D4370`) would settle: an unclaimed
    /// tile that is not open ocean takes the nation the claimed tiles across it agree on.
    /// The west/east pair wins over the north/south pair; open-4x cities carry no
    /// empire-culture rating, so a tile both pairs claim for different nations goes to the
    /// west/east pair the way Civ3's tied-rating case does. The tile's claim goes to the
    /// agreeing neighbour's city (west, else north), so `claim` and `owner` stay consistent
    /// through [`sync_territory_owners`]. Runs to a fixpoint: each filled tile can complete
    /// the surround of the next, and off-map and void neighbours count as nobody.
    pub(crate) fn fill_border_gaps(&mut self) {
        loop {
            let mut filled = Vec::new();
            for i in 0..self.map.tiles.len() {
                let position = self.map.tiles[i].position;
                if let Some(city) = self.orphan_claim(position) {
                    filled.push((position, city));
                }
            }
            if filled.is_empty() {
                break;
            }
            for (position, city) in filled {
                self.map.get_mut(position).expect("tile exists").claim = city;
            }
        }
        self.sync_territory_owners();
    }

    /// The city an unclaimed tile's surround names, or `None` when it names none.
    fn orphan_claim(&self, position: Coord) -> Option<Id> {
        let tile = self.map.get(position)?;
        if tile.claim != 0 || tile.terrain == Terrain::Ocean {
            return None;
        }
        let neighbour = |dx: i32, dy: i32| {
            let neighbour = self.map.get(position.offset(dx, dy))?;
            if neighbour.claim == 0 {
                return None;
            }
            let nation = self.cities.get(&neighbour.claim)?.owner;
            Some((nation, neighbour.claim))
        };
        let pair = |a: Option<(Id, Id)>, b: Option<(Id, Id)>| match (a, b) {
            (Some((nation, _)), Some((other, _))) if nation == other => a,
            _ => None,
        };
        let across_x = pair(neighbour(-1, 0), neighbour(1, 0));
        let across_y = pair(neighbour(0, -1), neighbour(0, 1));
        match (across_x, across_y) {
            (Some((_, city)), _) => Some(city),
            (None, Some((_, city))) => Some(city),
            (None, None) => None,
        }
    }

    /// Hand a city, and the region it carries, to a new nation.
    pub(crate) fn transfer_city(&mut self, id: Id, to: Id) {
        let old = self.cities[&id].owner;
        self.war_capture(old, to);
        let city = self.cities.get_mut(&id).expect("city exists");
        city.owner = to;
        city.production = None;
        city.progress = 0;
        city.construction = None;
        city.construction_progress = 0;
        for tile in &mut self.map.tiles {
            if tile.claim == id {
                tile.owner = to;
            }
        }
    }

    /// Recompute every tile's owner from the city that claims it.
    pub(crate) fn sync_territory_owners(&mut self) {
        for tile in &mut self.map.tiles {
            tile.owner = self.cities.get(&tile.claim).map_or(0, |c| c.owner);
        }
    }

    /// Every tile in a city's preset region.
    pub fn territory_of(&self, city: Id) -> Vec<Coord> {
        self.map
            .tiles
            .iter()
            .filter(|t| t.claim == city)
            .map(|t| t.position)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Region;
    use crate::tests::arena;

    #[test]
    fn reach_matches_civ3_tile_counts() {
        let count = |level| {
            let reach = reach_squared(level);
            (-7..=7)
                .flat_map(|y| (-7..=7).map(move |x| (x, y)))
                .filter(|(x, y)| x * x + y * y <= reach)
                .count()
        };
        assert_eq!(
            (1..=6).map(count).collect::<Vec<_>>(),
            [9, 21, 37, 61, 89, 137]
        );
    }

    #[test]
    fn bigger_cities_and_capitals_reach_farther() {
        assert_eq!(default_level(1, false), 2);
        assert_eq!(default_level(5, false), 3);
        assert_eq!(default_level(9, false), 4);
        assert_eq!(default_level(3, true), 3);
        assert_eq!(default_level(12, true), 5);
    }

    /// Two level-1 cities of one nation four steps apart, leaving a gap column neither
    /// reaches. `arena`'s capitals are far away and never interfere.
    fn gap_column() -> Game {
        let mut game = arena(24, 12);
        let west = game.add_city(1, Coord::new(10, 6), "West".into());
        let east = game.add_city(1, Coord::new(14, 6), "East".into());
        for id in [west, east] {
            game.cities.get_mut(&id).unwrap().border = 1;
        }
        game
    }

    #[test]
    fn surrounded_gap_tiles_take_the_surrounding_nation() {
        let mut game = gap_column();
        game.assign_start_territory();
        // The column between the two 3x3 regions is reached by neither city, but the tiles
        // across agree, so orphan resolution fills it like Civ3's `0x5D4370`.
        for y in 5..=7 {
            let tile = game.map.get(Coord::new(12, y)).unwrap();
            assert_ne!(tile.claim, 0, "gap tile (12, {y}) stays unclaimed");
            assert_eq!(tile.owner, 1);
        }
        // Rows outside the two claims stay open: only one side is owned.
        let open = game.map.get(Coord::new(12, 8)).unwrap();
        assert_eq!((open.claim, open.owner), (0, 0));
    }

    #[test]
    fn open_ocean_gaps_stay_unclaimed() {
        let mut game = gap_column();
        game.map.get_mut(Coord::new(12, 6)).unwrap().terrain = Terrain::Ocean;
        game.assign_start_territory();
        // Open ocean is never settled by adjacency; the land gaps around it still fill.
        let sea = game.map.get(Coord::new(12, 6)).unwrap();
        assert_eq!((sea.claim, sea.owner), (0, 0));
        assert_eq!(game.map.get(Coord::new(12, 5)).unwrap().owner, 1);
        assert_eq!(game.map.get(Coord::new(12, 7)).unwrap().owner, 1);
    }

    #[test]
    fn contested_gaps_go_to_the_west_east_pair() {
        let mut game = gap_column();
        let north = game.add_city(2, Coord::new(12, 4), "North".into());
        let south = game.add_city(2, Coord::new(12, 8), "South".into());
        for id in [north, south] {
            game.cities.get_mut(&id).unwrap().border = 1;
        }
        game.assign_start_territory();
        // West/east say nation 1, north/south say nation 2: the west/east pair wins, the way
        // Civ3's tied-rating case falls through to it.
        assert_eq!(game.map.get(Coord::new(12, 6)).unwrap().owner, 1);
    }

    #[test]
    fn regional_fill_puts_remote_named_land_with_the_nearest_matching_city() {
        let mut game = arena(24, 12);
        let capital = game.city_at(Coord::new(1, 1)).unwrap();
        // Two unnamed tiles: a lone one would be a one-tile hole once its neighbours fill,
        // which gap filling claims on purpose, so the fringe needs a neighbour of its own.
        let fringe = [Coord::new(22, 0), Coord::new(23, 0)];
        for tile in &mut game.map.tiles {
            tile.region = 1;
        }
        for p in fringe {
            game.map.get_mut(p).unwrap().region = 0;
        }
        game.regions = vec![Region {
            id: "red-land".to_string(),
            name: "Red Land".to_string(),
            nation: 1,
        }];

        game.assign_start_territory();
        let remote = Coord::new(12, 6);
        assert_eq!(game.map.get(remote).unwrap().claim, 0);
        game.fill_regional_territory();

        assert_eq!(game.map.get(remote).unwrap().claim, capital);
        assert_eq!(game.map.get(remote).unwrap().owner, 1);
        for p in fringe {
            let tile = game.map.get(p).unwrap();
            assert_eq!((tile.claim, tile.owner), (0, 0), "unnamed fringe {p:?}");
        }
    }
}
