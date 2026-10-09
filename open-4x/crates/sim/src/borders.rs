//! Cultural borders, fixed per city.
//!
//! Every city owns a preset region of the map, sized by its border level (Civ3's culture
//! levels 1 to 6). Regions never grow or shrink with time. A city's region belongs to whoever
//! holds the city, so conquering a city transfers its land, and neighbouring regions of one
//! nation read as a single territory because ownership is stored per nation.
//!
//! Level `L` reaches every tile within squared distance [`REACH_SQUARED`]`[L - 1]` on the
//! logical grid, which is Civ3's border shape: 9, 21, 37, 61, 89, and 137 tiles.
use crate::{City, Game, Id, terrain::Coord};

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

impl Game {
    /// Every tile a city's border can reach, with its squared distance from the city.
    fn border_reach(&self, city: &City) -> Vec<(Coord, i32)> {
        let reach = reach_squared(city.border);
        let radius = (1..).find(|r| r * r > reach).unwrap_or(1) - 1;
        let mut tiles = Vec::new();
        for y in city.position.y - radius..=city.position.y + radius {
            for x in city.position.x - radius..=city.position.x + radius {
                let p = Coord::new(x, y);
                let d = p.distance_squared(city.position);
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
        self.sync_territory_owners();
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
    }

    /// Hand a city, and the region it carries, to a new nation.
    pub(crate) fn transfer_city(&mut self, id: Id, to: Id) {
        let city = self.cities.get_mut(&id).expect("city exists");
        city.owner = to;
        city.production = None;
        city.progress = 0;
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
}
