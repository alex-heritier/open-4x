//! The computer nations, one behaviour at a time. `plan` runs a single computer turn; `end_turn`
//! runs the whole day, with production, work, and the computer's turn after it.
use super::*;
use crate::improvements::Job;
use crate::terrain::Tile;
use crate::tests::{arena, arena_rows, rules, spawn};
use crate::{Command, TickRules};
use std::collections::BTreeSet;

const RED: Id = 1;
const BLUE: Id = 2;

fn plan(game: &mut Game) {
    game.ai(&rules());
}
fn end_turn(game: &mut Game) {
    game.apply(RED, Command::EndTurn, &rules(), TickRules::default())
        .expect("the day ends");
}
/// Blue's capital: the only blue city in an arena.
fn capital(game: &Game) -> &City {
    game.cities
        .values()
        .find(|c| c.owner == BLUE)
        .expect("blue has a city")
}
fn building(game: &Game) -> Option<&str> {
    capital(game).production.as_deref()
}
/// Put `count` blue infantry in the blue capital.
fn garrison(game: &mut Game, count: usize) -> Vec<Id> {
    let at = capital(game).position;
    (0..count)
        .map(|_| spawn(game, BLUE, "infantry", at.x, at.y))
        .collect()
}
fn blue_guards_at(game: &Game, at: Coord) -> usize {
    game.units_at(at)
        .iter()
        .filter(|u| u.owner == BLUE && u.kind == "infantry")
        .count()
}
fn war(game: &mut Game) {
    game.wars.insert((RED, BLUE));
}
fn position(game: &Game, id: Id) -> Coord {
    game.units[&id].position
}

#[test]
fn designs_are_picked_by_their_numbers_not_their_names() {
    let rules = rules();
    let roster = Roster::new(&rules);
    let id = |d: Option<&UnitDef>| d.map(|d| d.id.clone());
    assert_eq!(id(roster.defender).as_deref(), Some("infantry"));
    assert_eq!(id(roster.striker).as_deref(), Some("cavalry"));
    assert_eq!(id(roster.gun).as_deref(), Some("artillery"));
    assert_eq!(id(roster.worker).as_deref(), Some("worker"));
    assert_eq!(id(roster.settler).as_deref(), Some("pioneer"));
    // Renaming every design changes nothing about who does what.
    let mut renamed = rules.clone();
    renamed.units = rules
        .units
        .into_values()
        .map(|mut d| {
            d.id = format!("x-{}", d.id);
            (d.id.clone(), d)
        })
        .collect();
    let roster = Roster::new(&renamed);
    assert_eq!(id(roster.defender).as_deref(), Some("x-infantry"));
    assert_eq!(id(roster.worker).as_deref(), Some("x-worker"));
}

#[test]
fn an_undefended_city_builds_a_defender_before_anything_else() {
    let mut game = arena(24, 14);
    plan(&mut game);
    assert_eq!(building(&game), Some("infantry"));
}

#[test]
fn a_held_city_turns_to_workers_then_settlers_and_then_rests() {
    let mut game = arena(30, 20);
    // A capital wants two defenders; with them, it wants a worker to work its land.
    garrison(&mut game, 2);
    plan(&mut game);
    assert_eq!(building(&game), Some("worker"));
    // With a worker at hand, and no day for looking at the map, there is nothing to build.
    let at = capital(&game).position;
    let id = capital(&game).id;
    spawn(&mut game, BLUE, "worker", at.x, at.y);
    game.cities.get_mut(&id).unwrap().production = None;
    assert_eq!((game.turn + BLUE) % 4, 3);
    plan(&mut game);
    assert_eq!(building(&game), None);
    // On the day it does look, free land nearby calls for a settler.
    game.turn = 2;
    plan(&mut game);
    assert_eq!(building(&game), Some("pioneer"));
}

#[test]
fn a_nation_with_no_free_land_builds_no_settlers() {
    let mut game = arena(30, 20);
    garrison(&mut game, 2);
    tend(&mut game);
    no_free_land(&mut game);
    game.turn = 2;
    plan(&mut game);
    assert_eq!(building(&game), None);
}

/// Fighters far from the blue capital, enough to meet any ceiling on a war production.
fn standing_army(game: &mut Game) {
    for _ in 0..20 {
        spawn(game, BLUE, "cavalry", 29, 2);
    }
}
/// A worker for the capital, so that it has no need of one.
fn tend(game: &mut Game) {
    let at = capital(game).position;
    spawn(game, BLUE, "worker", at.x, at.y);
}
/// Every square is spoken for, so that no settler is wanted.
fn no_free_land(game: &mut Game) {
    let red = game.cities.values().find(|c| c.owner == RED).unwrap().id;
    for tile in &mut game.map.tiles {
        tile.claim = red;
    }
}

#[test]
fn a_city_with_enemies_at_the_gate_wants_one_more_defender() {
    let mut game = arena(30, 20);
    garrison(&mut game, 2);
    tend(&mut game);
    standing_army(&mut game);
    war(&mut game);
    let at = capital(&game).position;
    // A far-off enemy changes nothing: the capital is held, and the army is large enough.
    let enemy = spawn(&mut game, RED, "infantry", at.x - 20, at.y - 12);
    plan(&mut game);
    assert_eq!(building(&game), None);
    // Close by, the same enemy makes two defenders too few.
    game.set_position(enemy, Coord::new(at.x - 5, at.y - 3));
    plan(&mut game);
    assert_eq!(building(&game), Some("infantry"));
}

#[test]
fn at_war_cities_that_need_nothing_else_build_a_mix_of_soldiers_up_to_a_ceiling() {
    let mut game = arena(30, 20);
    garrison(&mut game, 2);
    tend(&mut game);
    no_free_land(&mut game);
    war(&mut game);
    let id = capital(&game).id;
    let mut built = BTreeSet::new();
    for day in 0..4 {
        game.turn = day;
        game.cities.get_mut(&id).unwrap().production = None;
        plan(&mut game);
        built.extend(building(&game).map(String::from));
    }
    assert_eq!(
        built.iter().map(String::as_str).collect::<Vec<_>>(),
        ["artillery", "cavalry", "infantry"]
    );
    // An army past its ceiling is enough: the city stands idle.
    standing_army(&mut game);
    game.cities.get_mut(&id).unwrap().production = None;
    plan(&mut game);
    assert_eq!(building(&game), None);
}

#[test]
fn an_unguarded_city_drops_what_it_started_unless_it_is_nearly_done() {
    let mut game = arena(24, 14);
    let id = capital(&game).id;
    let set = |game: &mut Game, progress| {
        let city = game.cities.get_mut(&id).unwrap();
        city.production = Some("pioneer".into());
        city.progress = progress;
    };
    set(&mut game, 5);
    plan(&mut game);
    assert_eq!(building(&game), Some("infantry"));
    set(&mut game, 30); // three quarters of a settler
    plan(&mut game);
    assert_eq!(building(&game), Some("pioneer"));
}

#[test]
fn the_last_defenders_of_a_city_never_leave_it() {
    let mut game = arena(30, 20);
    let guards = garrison(&mut game, 2); // all a capital wants
    let at = capital(&game).position;
    war(&mut game);
    plan(&mut game);
    for id in guards {
        assert_eq!(position(&game, id), at);
    }
}

#[test]
fn spare_soldiers_march_on_the_enemy_while_the_rest_stay() {
    let mut game = arena(30, 20);
    let guards = garrison(&mut game, 3);
    let at = capital(&game).position;
    war(&mut game);
    plan(&mut game);
    let home = guards.iter().filter(|g| position(&game, **g) == at).count();
    assert_eq!(home, 2, "the capital keeps what it wants");
    let gone = guards
        .iter()
        .find(|g| position(&game, **g) != at)
        .expect("the spare man marches");
    let red = Coord::new(1, 1);
    assert!(game.map.distance(position(&game, *gone), red) < game.map.distance(at, red));
}

#[test]
fn at_peace_spare_soldiers_stay_and_dig_in() {
    let mut game = arena(30, 20);
    let guards = garrison(&mut game, 4);
    let at = capital(&game).position;
    plan(&mut game);
    for id in guards {
        assert_eq!(position(&game, id), at);
        assert_eq!(game.units[&id].order, Order::Fortifying);
    }
}

#[test]
fn spare_soldiers_fill_the_gaps_in_other_cities() {
    let mut game = arena(30, 20);
    let second = game.add_city(BLUE, Coord::new(28, 8), "Second".into());
    game.assign_start_territory();
    game.refresh_harvest();
    let guards = garrison(&mut game, 3);
    let (home, there) = (capital(&game).position, game.cities[&second].position);
    plan(&mut game);
    let mover = guards
        .iter()
        .find(|g| position(&game, **g) != home)
        .expect("one man is spare");
    assert!(game.map.distance(position(&game, *mover), there) < game.map.distance(home, there));
    // Within a few days he is there, and nobody else has left the capital.
    for _ in 0..6 {
        end_turn(&mut game);
    }
    assert!(blue_guards_at(&game, there) >= 1, "{:?}", game.log);
    assert!(blue_guards_at(&game, home) >= 2);
}

#[test]
fn soldiers_in_the_open_go_home_at_peace() {
    let mut game = arena(30, 20);
    let home = capital(&game).position;
    let wanderer = spawn(&mut game, BLUE, "infantry", home.x - 6, home.y - 6);
    plan(&mut game);
    assert!(game.map.distance(position(&game, wanderer), home) < 6);
}

#[test]
fn a_garrison_captures_what_is_undefended_but_will_not_gamble_on_even_odds() {
    let mut game = arena(30, 20);
    garrison(&mut game, 2);
    let at = capital(&game).position;
    war(&mut game);
    // A fortified enemy at the gate is an even fight at best: the garrison holds its fire.
    let tough = spawn(&mut game, RED, "infantry", at.x - 1, at.y);
    game.units.get_mut(&tough).unwrap().order = Order::Fortified;
    plan(&mut game);
    assert!(game.battles.is_empty(), "{:?}", game.battles);
    assert_eq!(game.units[&tough].owner, RED);
    // A worker nobody defends is taken at no risk.
    let worker = spawn(&mut game, RED, "worker", at.x, at.y - 1);
    plan(&mut game);
    assert_eq!(game.units[&worker].owner, BLUE);
}

/// A 14×14 arena with a pond west of blue's capital, so that nearby ground can be farmed.
fn lakeside() -> Game {
    let mut rows = vec![".".repeat(14); 14];
    rows[12].replace_range(9..10, "c");
    arena_rows(&rows)
}

#[test]
fn a_worker_walks_to_the_best_job_on_its_nations_land_and_starts_it() {
    let mut game = lakeside();
    let capital_at = capital(&game).position;
    let worker = spawn(&mut game, BLUE, "worker", capital_at.x, capital_at.y);
    plan(&mut game);
    // Farming beside the pond adds food, the most prized yield, for the least work.
    let at = position(&game, worker);
    assert_ne!(at, capital_at, "cities cannot be improved, so it walked");
    assert_eq!(game.units[&worker].order, Order::Work(Job::Farm));
    assert!(
        game.map.neighbors(at).contains(&Coord::new(9, 12)),
        "{at:?}"
    );
    assert_eq!(game.map.get(at).unwrap().owner, BLUE);
    // And the job is carried through by the day's work.
    for _ in 0..8 {
        end_turn(&mut game);
    }
    assert!(game.map.get(at).unwrap().improvements & Tile::FARM != 0);
}

#[test]
fn a_worker_keeps_to_one_job_until_it_is_done() {
    let mut game = lakeside();
    let capital_at = capital(&game).position;
    let worker = spawn(&mut game, BLUE, "worker", capital_at.x, capital_at.y);
    plan(&mut game);
    let (at, order) = (position(&game, worker), game.units[&worker].order);
    end_turn(&mut game);
    assert_eq!(
        (position(&game, worker), game.units[&worker].order),
        (at, order)
    );
    assert!(game.units[&worker].work > 0, "the work is under way");
}

#[test]
fn a_worker_never_works_foreign_land() {
    let mut game = arena(14, 14);
    let capital_at = capital(&game).position;
    // Deep in red's land, and nothing of blue's within reach of it.
    let worker = spawn(&mut game, BLUE, "worker", 3, 2);
    assert_eq!(game.map.get(Coord::new(3, 2)).unwrap().owner, RED);
    plan(&mut game);
    assert_eq!(game.units[&worker].order, Order::None);
    assert!(
        game.map.tiles.iter().all(|t| t.improvements == 0),
        "nothing was built"
    );
    // With nothing to do, it heads for a city of its own.
    assert!(game.map.distance(position(&game, worker), capital_at) < 10);
}

#[test]
fn a_worker_runs_from_the_enemy_into_the_city() {
    let mut game = arena(14, 14);
    let capital_at = capital(&game).position;
    war(&mut game);
    let worker = spawn(&mut game, BLUE, "worker", capital_at.x - 2, capital_at.y);
    spawn(&mut game, RED, "infantry", capital_at.x - 4, capital_at.y);
    plan(&mut game);
    assert_eq!(position(&game, worker), capital_at);
    assert_eq!(game.units[&worker].order, Order::None);
}

#[test]
fn a_pioneer_founds_a_city_on_free_land() {
    let mut game = arena(30, 20);
    let pioneer = spawn(&mut game, BLUE, "pioneer", 14, 10);
    plan(&mut game);
    assert!(!game.units.contains_key(&pioneer), "the pioneer is spent");
    let city = game
        .city_at(Coord::new(14, 10))
        .expect("a city stands there");
    assert_eq!(game.cities[&city].owner, BLUE);
    assert_eq!(game.map.get(Coord::new(14, 10)).unwrap().claim, city);
}

#[test]
fn a_pioneer_walks_out_of_foreign_land_before_it_founds_anything() {
    let mut game = arena(30, 20);
    let pioneer = spawn(&mut game, BLUE, "pioneer", 3, 2);
    assert_eq!(game.map.get(Coord::new(3, 2)).unwrap().owner, RED);
    let cities = game.cities.len();
    plan(&mut game);
    assert_eq!(game.cities.len(), cities, "not in red's territory");
    assert!(game.units.contains_key(&pioneer));
    assert!(
        position(&game, pioneer).distance(Coord::new(3, 2)) > 0,
        "it set out"
    );
}

#[test]
fn a_pioneer_will_not_found_a_city_that_would_gather_nothing() {
    let mut game = arena(30, 20);
    let red = game.cities.values().find(|c| c.owner == RED).unwrap().id;
    for tile in &mut game.map.tiles {
        tile.claim = red; // every square is spoken for
    }
    let pioneer = spawn(&mut game, BLUE, "pioneer", 14, 10);
    plan(&mut game);
    assert!(game.units.contains_key(&pioneer));
    assert!(game.city_at(Coord::new(14, 10)).is_none());
}

#[test]
fn settlers_that_cannot_settle_work_the_land_instead() {
    let mut game = lakeside();
    let capital_at = capital(&game).position;
    let pioneer = spawn(&mut game, BLUE, "pioneer", capital_at.x, capital_at.y);
    // A level-three city leaves no free land in a 14×14 arena once red holds its own.
    for tile in &mut game.map.tiles {
        tile.claim = tile.claim.max(1);
    }
    plan(&mut game);
    assert!(
        matches!(game.units[&pioneer].order, Order::Work(_)),
        "{:?}",
        game.units[&pioneer]
    );
}

#[test]
fn a_computer_nation_garrisons_works_and_expands_on_its_own() {
    let play = || {
        let mut game = arena(30, 30);
        let at = capital(&game).position;
        spawn(&mut game, BLUE, "worker", at.x, at.y);
        spawn(&mut game, BLUE, "pioneer", at.x, at.y);
        for _ in 0..60 {
            end_turn(&mut game);
        }
        game
    };
    let game = play();
    assert_eq!(game, play(), "the same seed plays the same game");
    let cities: Vec<&City> = game.cities.values().filter(|c| c.owner == BLUE).collect();
    assert!(
        cities.len() >= 2,
        "the pioneer founded a city: {:?}",
        game.log
    );
    // A city with no guard is building one (a young city has had few days to).
    for city in &cities {
        assert!(
            blue_guards_at(&game, city.position) >= 1
                || city.production.as_deref() == Some("infantry"),
            "{} is bare and idle: {:?}",
            city.name,
            city.production
        );
    }
    for (a, b) in cities
        .iter()
        .flat_map(|a| cities.iter().map(move |b| (a, b)))
    {
        assert!(a.id == b.id || game.map.distance(a.position, b.position) >= 3);
    }
    assert!(
        game.map
            .tiles
            .iter()
            .any(|t| t.owner == BLUE && t.improvements != 0),
        "the workers built something"
    );
    assert!(
        game.units
            .values()
            .all(|u| game.map.get(u.position).is_some())
    );
}
