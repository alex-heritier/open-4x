//! City::diseaseStep (`0x4B45A0`), before food/growth/production (`0x4BEB00`).
use crate::cities::City;
use crate::map::GameMap;
use crate::rng::MapRng;
use civ3mapgen::disease as exe;

/// Some(Some(TERR)) is a new infection; Some(None) is a continuing loss.
/// Recovery itself has no native notification. The flag changes no yields.
pub fn step(map: &GameMap, city: &mut City, rng: &mut MapRng) -> Option<Option<usize>> {
    if city.diseased {
        let lost = city.size() > 1;
        if lost { city.lose_population(1, None, rng); }
        if exe::recovers(i32::from(city.size()), |n| rng.reference().below(n)) {
            city.diseased = false;
        }
        return lost.then_some(None);
    }
    let mut counts = [0; 14];
    let mut worked = city.worked(map);
    worked.insert((city.x, city.y));
    for (x, y) in worked {
        if let Some(t) = map.get(x, y) { counts[crate::map::terrain_row(t)] += 1; }
    }
    let terrains: [exe::Terrain; 14] = std::array::from_fn(|i| crate::ruleset::TERRAINS[i].disease);
    let cured = crate::realm::read(city.civ, |r| r.knows(exe::CURE_TECH));
    let terrain = exe::infection(i32::from(city.size()), &counts, &terrains, cured,
        |n| rng.reference().below(n))?;
    city.diseased = true;
    city.lose_population(1, None, rng);
    Some(Some(terrain))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{Base, Cover, Relief};

    fn setup() -> (GameMap, City) {
        crate::realm::reset();
        let mut map = GameMap::generate();
        for t in &mut map.tiles { t.base = Base::Plains; t.cover = Cover::Bare; t.relief = Relief::Flat; t.river = 0; }
        let mut city = City::new(0, "Town", 5, 5);
        city.set_size(4);
        (map, city)
    }

    #[test]
    fn infection_counts_center_and_worked_tiles_and_removes_a_native_victim() {
        let (mut map, mut city) = setup();
        let i = map.idx(5, 5); map.tiles[i].cover = Cover::Jungle;
        city.set_specialists(vec![crate::cities::Specialist::Scientist; 4]);
        let mut rng = MapRng::new(0);
        assert_eq!(step(&map, &mut city, &mut rng), Some(Some(8)));
        assert!(city.diseased);
        assert_eq!(city.size(), 3);
        assert_eq!(city.specialists().len(), 3);
        assert_eq!(rng.state(), 3_554_416_254, "infection, then victim draw");
        let victim = city.citizens.slots().iter().position(Option::is_none).unwrap();
        assert_eq!(victim, 2);
    }

    #[test]
    fn unworked_disease_tiles_do_not_draw_and_writing_cures_only_flood_plain() {
        let (mut map, mut city) = setup();
        let i = map.idx(6, 5); map.tiles[i].cover = Cover::Jungle;
        let mut rng = MapRng::new(0);
        assert_eq!(step(&map, &mut city, &mut rng), None);
        assert_eq!(rng.state(), 0);
        let i = map.idx(5, 5); map.tiles[i].base = Base::Desert; map.tiles[i].river = 2;
        crate::realm::write(0, |r| r.known |= 1 << exe::CURE_TECH);
        assert_eq!(step(&map, &mut city, &mut rng), None);
        assert_eq!(rng.state(), 0);
        city.work_tile(&map, (6, 5));
        assert_eq!(step(&map, &mut city, &mut rng), Some(Some(8)));
    }

    #[test]
    fn continuing_disease_loses_before_recovery_and_never_kills_the_last_citizen() {
        let (map, mut city) = setup();
        city.diseased = true;
        let mut rng = MapRng::new(1);
        assert_eq!(step(&map, &mut city, &mut rng), Some(None));
        assert_eq!(city.size(), 3);
        assert!(!city.diseased);
        assert_eq!(rng.state(), 2_524_885_223);
        city.set_size(1);
        city.diseased = true;
        rng = MapRng::new(0);
        assert_eq!(step(&map, &mut city, &mut rng), None);
        assert_eq!(city.size(), 1);
        assert!(city.diseased, "zero recovery roll stays diseased");
        assert_eq!(rng.state(), 12_345, "size one draws recovery only");
        assert_eq!(step(&map, &mut city, &mut rng), None);
        assert!(!city.diseased);
    }

    #[test]
    fn owner_turn_applies_disease_before_food_and_population_cost_production() {
        let (map, mut city) = setup();
        crate::realm::write(0, |r| r.born_content = 7);
        city.set_size(3);
        city.set_specialists(vec![crate::cities::Specialist::Scientist; 3]);
        city.diseased = true;
        city.food = 10;
        city.production = crate::cities::Production::named("Settler");
        city.shields = city.price(city.production);
        let events = crate::cities::process_city_turn(&map, &mut city,
            &std::collections::HashSet::new(), &mut MapRng::new(1));
        assert!(matches!(events.first(), Some(crate::cities::CityEvent::Disease(None))));
        assert_eq!(city.size(), 2);
        assert!(!events.iter().any(|e| matches!(e, crate::cities::CityEvent::Completed(..))));

        city.food = 0;
        city.diseased = true;
        let events = crate::cities::process_city_turn(&map, &mut city,
            &std::collections::HashSet::new(), &mut MapRng::new(1));
        assert_eq!(city.size(), 1);
        assert_eq!(city.food, 0, "one citizen is fed by the center after the loss");
        assert!(!events.iter().any(|e| matches!(e, crate::cities::CityEvent::Starved)));
    }
}
