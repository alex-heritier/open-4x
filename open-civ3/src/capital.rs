//! Replace a lost capital using `0x4482B0`'s score and add its Palace.
use crate::cities::{Capital, City};
use crate::map::GameMap;
use crate::units::{Unit, def};
use bevy::prelude::*;

fn choose<'a>(
    map: &GameMap,
    cities: impl IntoIterator<Item = (Entity, &'a City)>,
    civ: usize,
    police: impl Fn(i32, i32) -> i32,
) -> Option<Entity> {
    let cities: Vec<_> = cities.into_iter().filter(|(_, c)| c.civ == civ).collect();
    let mut best = None;
    let mut high = 0;
    for &(entity, city) in &cities {
        let neighbors = (1..289).filter_map(|i| {
            let (dx, dy) = civ3mapgen::spiral::spiral_offset(i);
            let pos = (map.wrap_x(city.x + (dx + dy) / 2), city.y + (dy - dx) / 2);
            cities
                .iter()
                .find(|(_, c)| (c.x, c.y) == pos)
                .map(|(_, c)| i32::from(c.size()))
        });
        let score = civ3mapgen::capital::score(
            i32::from(city.size()),
            city.nationals(civ) as i32,
            police(city.x, city.y),
            neighbors,
        );
        // Native compares strictly: the first occupied city pool slot wins ties.
        if score > high {
            high = score;
            best = Some(entity);
        }
    }
    best
}

pub fn replace_missing(
    map: Res<GameMap>,
    mut capital: ResMut<Capital>,
    mut cities: Query<(Entity, &mut City)>,
    units: Query<&Unit>,
) {
    for civ in 0..crate::civs::civ_count() {
        if capital.0[civ].is_some_and(|e| cities.get(e).is_ok_and(|(_, c)| c.civ == civ)) {
            continue;
        }
        let next = choose(&map, cities.iter(), civ, |x, y| {
            units
                .iter()
                .filter(|u| {
                    (u.x, u.y) == (x, y)
                        && def(u.utype).class == 0
                        && (u.attack() > 0 || u.defense() > 0)
                })
                .count() as i32
        });
        if capital.0[civ] != next {
            capital.0[civ] = next;
        }
        if let Some(e) = next {
            let (_, mut city) = cities.get_mut(e).unwrap();
            let palace = crate::roster::BLDGS
                .iter()
                .position(|b| b.flags & crate::roster::imp::CENTER_OF_EMPIRE != 0)
                .unwrap();
            let palace = crate::cities::Production::from_building_row(palace);
            if !city.has(palace) {
                city.buildings.push(palace);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacement_uses_nationals_garrison_and_keeps_first_tie() {
        let map = GameMap::generate();
        let mut foreign = City::new(0, "Foreign", 5, 5);
        foreign.set_size(10);
        foreign.set_nationality(1);
        let mut home = City::new(0, "Home", 30, 30);
        home.set_size(3);
        let a = Entity::from_bits(1);
        let b = Entity::from_bits(2);
        assert_eq!(
            choose(&map, [(a, &foreign), (b, &home)], 0, |x, _| if x == 30 {
                2
            } else {
                0
            }),
            Some(b)
        );
        let mut clone = home.clone();
        clone.x = 55;
        assert_eq!(
            choose(&map, [(a, &home), (b, &clone)], 0, |_, _| 0),
            Some(a)
        );
    }

    #[test]
    fn losing_a_capital_selects_the_native_score_winner_and_installs_a_palace() {
        let mut app = App::new();
        app.insert_resource(GameMap::generate());
        app.init_resource::<Capital>();
        app.add_systems(Update, replace_missing);
        let old = app.world_mut().spawn(City::new(0, "Old", 50, 45)).id();
        let mut foreign = City::new(0, "Foreign", 5, 5);
        foreign.set_size(10);
        foreign.set_nationality(1);
        let foreign = app.world_mut().spawn(foreign).id();
        let mut home = City::new(0, "Home", 30, 30);
        home.set_size(3);
        let home = app.world_mut().spawn(home).id();
        for _ in 0..2 {
            app.world_mut().spawn(Unit::new(
                1,
                crate::units::UnitType::named("Warrior"),
                30,
                30,
            ));
        }
        app.world_mut().resource_mut::<Capital>().0[0] = Some(old);
        app.update();
        assert_eq!(
            app.world().resource::<Capital>().0[0],
            Some(old),
            "valid capitals do not move"
        );
        app.world_mut().get_mut::<City>(old).unwrap().civ = 1;
        app.update();
        assert_eq!(app.world().resource::<Capital>().0[0], Some(home));
        let palace = crate::cities::Production::from_building_row(
            crate::roster::BLDGS
                .iter()
                .position(|b| b.flags & crate::roster::imp::CENTER_OF_EMPIRE != 0)
                .unwrap(),
        );
        assert!(app.world().get::<City>(home).unwrap().has(palace));
        assert_eq!(
            crate::cities::culture_per_turn(app.world().get::<City>(home).unwrap(), true),
            palace.culture(),
            "an installed Palace replaces the implicit one rather than doubling culture"
        );
        assert!(!app.world().get::<City>(foreign).unwrap().has(palace));
        app.world_mut().despawn(home);
        app.world_mut().despawn(foreign);
        app.update();
        assert_eq!(app.world().resource::<Capital>().0[0], None);
    }

    #[test]
    fn nearby_city_classes_and_wrapping_affect_the_choice() {
        let map = GameMap::generate();
        let edge = City::new(0, "Edge", 0, 20);
        let mut isolated = City::new(0, "Isolated", 30, 40);
        isolated.set_size(2);
        let mut neighbor = City::new(0, "Neighbor", map.w - 1, 20);
        neighbor.set_size(13);
        neighbor.set_nationality(1);
        let a = Entity::from_bits(1);
        let b = Entity::from_bits(2);
        let c = Entity::from_bits(3);
        // Edge: 3 + neighboring metropolis 3 = 6. Isolated: 6. The first
        // tie wins, whereas without wrapping Edge would only score 3.
        // Neighbor's foreign population still wins 13 + neighboring town 1.
        assert_eq!(
            choose(
                &map,
                [(a, &edge), (b, &isolated), (c, &neighbor)],
                0,
                |_, _| 0
            ),
            Some(c)
        );
        // Give Edge enough police to distinguish the wrapped contribution.
        assert_eq!(
            choose(
                &map,
                [(a, &edge), (b, &isolated), (c, &neighbor)],
                0,
                |x, _| if x == 0 { 9 } else { 0 }
            ),
            Some(a)
        );
    }
}
