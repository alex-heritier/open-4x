//! Whole-game behaviour: movement, Civ3 combat, workers, borders, and determinism.
use crate::terrain::{Coord, Tile};
use crate::units::*;
use crate::*;

/// The designs used throughout the tests, mirroring the base pack's Civ3-style numbers.
pub(crate) fn rules() -> Rules {
    let def = |id: &str, domain, attack, defense, moves| UnitDef {
        id: id.into(),
        name: id.into(),
        cost: 40,
        sprite: id.into(),
        domain,
        attack,
        defense,
        bombard: 0,
        range: 0,
        rate_of_fire: 0,
        hp: 0,
        moves,
        blitz: false,
        work: 0,
        settler: false,
        capacity: 0,
        lethal_land: false,
        lethal_sea: false,
    };
    let mut infantry = def("infantry", Domain::Land, 4, 4, 2);
    infantry.hp = 1;
    let mut cavalry = def("cavalry", Domain::Land, 8, 2, 3);
    cavalry.blitz = true;
    let mut artillery = def("artillery", Domain::Land, 0, 0, 2);
    artillery.bombard = 8;
    artillery.range = 1;
    artillery.rate_of_fire = 2;
    let mut worker = def("worker", Domain::Land, 0, 0, 2);
    worker.work = 2;
    let mut pioneer = def("pioneer", Domain::Land, 0, 0, 2);
    pioneer.work = 1;
    pioneer.settler = true;
    let militia = def("militia", Domain::Land, 1, 1, 1);
    let mut ironclad = def("ironclad", Domain::Sea, 4, 4, 4);
    ironclad.hp = 1;
    // The Civ3 ship roles of the base pack, with its relative strengths.
    let mut transport = def("transport", Domain::Sea, 0, 3, 4);
    transport.hp = 1;
    transport.capacity = 2;
    let mut battleship = def("battleship", Domain::Sea, 12, 8, 3);
    battleship.hp = 2;
    let mut cruiser = def("cruiser", Domain::Sea, 6, 6, 5);
    cruiser.hp = 1;
    let mut torpedo_boat = def("torpedo-boat", Domain::Sea, 8, 2, 3);
    torpedo_boat.bombard = 8;
    torpedo_boat.range = 1;
    torpedo_boat.rate_of_fire = 2;
    torpedo_boat.lethal_land = true;
    torpedo_boat.lethal_sea = true;
    // The same gun as a plain warship's: wounds, never kills.
    let mut gunboat = torpedo_boat.clone();
    gunboat.id = "gunboat".into();
    gunboat.lethal_land = false;
    gunboat.lethal_sea = false;
    Rules {
        units: [
            infantry,
            cavalry,
            artillery,
            worker,
            pioneer,
            militia,
            ironclad,
            transport,
            battleship,
            cruiser,
            torpedo_boat,
            gunboat,
        ]
        .into_iter()
        .map(|d| (d.id.clone(), d))
        .collect(),
        victory_industry: 1_000_000,
        research_cost: 1_000_000,
        starting_gold: 100,
    }
}

/// A grass map with red (nation 1, the commander) and blue (nation 2). Red's capital is at
/// (1,1) and blue's at (width-2,height-2); the middle is empty.
pub(crate) fn arena(width: i32, height: i32) -> Game {
    arena_rows(&vec![".".repeat(width as usize); height as usize])
}

pub(crate) fn arena_rows(rows: &[String]) -> Game {
    let (width, height) = (rows[0].len() as i32, rows.len() as i32);
    let json = serde_json::json!({
        "format": SCENARIO_FORMAT,
        "id": "arena",
        "name": "Arena",
        "start_date": "1876-01-01",
        "commander": "red",
        "map": {"width": width, "height": height, "terrain": rows},
        "nations": [
            {"id": "red", "name": "Red", "color": "#aa0000"},
            {"id": "blue", "name": "Blue", "color": "#0000aa"},
        ],
        "cities": [
            {"nation": "red", "position": {"x": 1, "y": 1}, "name": "Redville",
             "population": 3, "industry": 4, "capital": true},
            {"nation": "blue", "position": {"x": width - 2, "y": height - 2}, "name": "Blueton",
             "population": 3, "industry": 4, "capital": true},
        ],
    });
    let scenario: Scenario = serde_json::from_value(json).expect("valid arena");
    Game::from_scenario(7, &rules(), &scenario, None).expect("arena starts")
}

pub(crate) fn spawn(game: &mut Game, owner: Id, kind: &str, x: i32, y: i32) -> Id {
    game.add_unit(owner, kind, Coord::new(x, y))
}

const RED: Id = 1;
const BLUE: Id = 2;

fn run(game: &mut Game, player: Id, command: Command) -> Result<()> {
    game.apply(player, command, &rules(), TickRules::default())
}
fn end_turn(game: &mut Game) {
    run(game, RED, Command::EndTurn).expect("turn ends");
}
fn unit(game: &Game, id: Id) -> &Unit {
    &game.units[&id]
}

#[test]
fn units_march_in_eight_directions_two_tiles_a_turn() {
    let mut game = arena(16, 16);
    let a = spawn(&mut game, RED, "infantry", 4, 4);
    let goal = Coord::new(9, 9);
    run(
        &mut game,
        RED,
        Command::Move {
            unit: a,
            destination: goal,
        },
    )
    .unwrap();
    // Two diagonal steps, which are single tiles in this grid.
    assert_eq!(unit(&game, a).position, Coord::new(6, 6));
    assert_eq!(unit(&game, a).goto, Some(goal));
    end_turn(&mut game);
    assert_eq!(unit(&game, a).position, Coord::new(8, 8));
    end_turn(&mut game);
    assert_eq!(unit(&game, a).position, goal);
    assert_eq!(unit(&game, a).goto, None);
}

#[test]
fn rough_ground_costs_the_whole_move_and_roads_cost_a_third() {
    let rows: Vec<String> = ["......", "..mm..", "......", "......"]
        .map(String::from)
        .into();
    let mut game = arena_rows(&rows);
    let a = spawn(&mut game, RED, "infantry", 2, 0);
    // One step onto mountains (3 moves) uses everything an infantryman has.
    run(
        &mut game,
        RED,
        Command::Move {
            unit: a,
            destination: Coord::new(2, 1),
        },
    )
    .unwrap();
    assert_eq!(unit(&game, a).position, Coord::new(2, 1));
    assert_eq!(unit(&game, a).moves_left(&rules().units["infantry"]), 0);
    let err = run(
        &mut game,
        RED,
        Command::Move {
            unit: a,
            destination: Coord::new(2, 2),
        },
    );
    assert!(err.is_ok(), "orders can be given with no movement left");
    assert_eq!(unit(&game, a).position, Coord::new(2, 1));
    end_turn(&mut game);
    assert_eq!(unit(&game, a).position, Coord::new(2, 2));

    // A road along the top row: six steps on roads cost two moves, an infantryman's turn.
    let mut game = arena(10, 4);
    let a = spawn(&mut game, RED, "infantry", 0, 3);
    for x in 0..10 {
        game.map.get_mut(Coord::new(x, 3)).unwrap().improvements = Tile::ROAD;
    }
    run(
        &mut game,
        RED,
        Command::Move {
            unit: a,
            destination: Coord::new(9, 3),
        },
    )
    .unwrap();
    assert_eq!(unit(&game, a).position, Coord::new(6, 3));
    // On railroad the same march is free.
    let mut game = arena(10, 4);
    let a = spawn(&mut game, RED, "infantry", 0, 3);
    for x in 0..10 {
        game.map.get_mut(Coord::new(x, 3)).unwrap().improvements = Tile::ROAD | Tile::RAIL;
    }
    run(
        &mut game,
        RED,
        Command::Move {
            unit: a,
            destination: Coord::new(9, 3),
        },
    )
    .unwrap();
    assert_eq!(unit(&game, a).position, Coord::new(9, 3));
    assert_eq!(unit(&game, a).moves_used, 0);
}

#[test]
fn foreign_units_block_the_way_and_moves_stop_short() {
    let mut game = arena(12, 5);
    let a = spawn(&mut game, RED, "infantry", 2, 2);
    let wall: Vec<Id> = (0..5)
        .map(|y| spawn(&mut game, BLUE, "militia", 5, y))
        .collect();
    assert_eq!(wall.len(), 5);
    // The wall spans the map: there is no route past it, only up to it.
    let far = Coord::new(8, 2);
    assert!(
        run(
            &mut game,
            RED,
            Command::Move {
                unit: a,
                destination: far
            }
        )
        .is_err()
    );
    // Approaching an enemy square stops adjacent and never attacks.
    run(
        &mut game,
        RED,
        Command::Move {
            unit: a,
            destination: Coord::new(5, 2),
        },
    )
    .unwrap();
    assert_eq!(unit(&game, a).position.distance(Coord::new(5, 2)), 1);
    assert_eq!(unit(&game, a).goto, None);
    assert!(!game.at_war(RED, BLUE));
    assert_eq!(game.units.len(), 6);
}

#[test]
fn only_the_owner_commands_a_unit() {
    let mut game = arena(8, 8);
    let a = spawn(&mut game, RED, "infantry", 3, 3);
    assert!(run(&mut game, BLUE, Command::Fortify { unit: a }).is_err());
    assert!(
        run(
            &mut game,
            BLUE,
            Command::Move {
                unit: a,
                destination: Coord::new(4, 4)
            }
        )
        .is_err()
    );
    assert!(run(&mut game, BLUE, Command::Disband { unit: a }).is_err());
}

#[test]
fn fortifying_takes_a_turn_and_is_lost_when_movement_is_spent() {
    let mut game = arena(8, 8);
    let rules = rules();
    let a = spawn(&mut game, RED, "infantry", 4, 4);
    let def = &rules.units["infantry"];
    run(&mut game, RED, Command::Fortify { unit: a }).unwrap();
    assert_eq!(unit(&game, a).order, Order::Fortifying);
    assert_eq!(game.fortify_percent(unit(&game, a), def), 0);
    end_turn(&mut game);
    assert_eq!(unit(&game, a).order, Order::Fortified);
    assert_eq!(game.fortify_percent(unit(&game, a), def), FORTIFY_PERCENT);
    // The bonus needs movement left.
    game.units.get_mut(&a).unwrap().moves_used = def.total_moves();
    assert_eq!(game.fortify_percent(unit(&game, a), def), 0);
    // Moving ends fortification.
    game.units.get_mut(&a).unwrap().moves_used = 0;
    run(
        &mut game,
        RED,
        Command::Move {
            unit: a,
            destination: Coord::new(5, 5),
        },
    )
    .unwrap();
    assert_eq!(unit(&game, a).order, Order::None);
    // Workers and artillery have nothing to dig in with.
    let w = spawn(&mut game, RED, "worker", 3, 3);
    assert!(run(&mut game, RED, Command::Fortify { unit: w }).is_err());
}

#[test]
fn units_in_cities_count_as_fortified() {
    let mut game = arena(8, 8);
    let rules = rules();
    let a = spawn(&mut game, RED, "infantry", 1, 1);
    assert_eq!(
        game.fortify_percent(unit(&game, a), &rules.units["infantry"]),
        25
    );
    // Civ3's city size classes: town 0, city 50, metropolis 100.
    let city = game.city_at(Coord::new(1, 1)).unwrap();
    assert_eq!(game.city_percent(Coord::new(1, 1)), 0);
    game.cities.get_mut(&city).unwrap().population = 9;
    assert_eq!(game.city_percent(Coord::new(1, 1)), 50);
    game.cities.get_mut(&city).unwrap().population = 13;
    assert_eq!(game.city_percent(Coord::new(1, 1)), 100);
}

#[test]
fn attacks_only_reach_adjacent_hostile_squares() {
    let mut game = arena(10, 10);
    let a = spawn(&mut game, RED, "infantry", 4, 4);
    let far = spawn(&mut game, BLUE, "militia", 7, 7);
    let near = spawn(&mut game, BLUE, "militia", 5, 5);
    let friend = spawn(&mut game, RED, "militia", 3, 3);
    let err = |game: &mut Game, target: Coord| {
        run(game, RED, Command::Attack { unit: a, target })
            .unwrap_err()
            .0
    };
    assert!(err(&mut game, Coord::new(7, 7)).contains("adjacent"));
    assert!(err(&mut game, Coord::new(3, 3)).contains("Nothing"));
    assert!(err(&mut game, Coord::new(4, 5)).contains("Nothing"));
    assert!(game.units.contains_key(&far) && game.units.contains_key(&friend));
    assert!(!game.at_war(RED, BLUE));
    // The real attack declares war and spends a full move, whatever the dice say.
    run(
        &mut game,
        RED,
        Command::Attack {
            unit: a,
            target: Coord::new(5, 5),
        },
    )
    .unwrap();
    assert!(game.at_war(RED, BLUE));
    if let Some(a) = game.units.get(&a) {
        assert_eq!(a.moves_used, MOVE_UNIT);
        assert!(a.attacked);
    }
    let _ = near;
}

#[test]
fn only_blitz_units_attack_more_than_once_a_turn() {
    let mut game = arena(10, 10);
    let rules = rules();
    // A mountain full of hardened defenders makes the attackers lose hit points, not die.
    let infantry = spawn(&mut game, RED, "infantry", 4, 4);
    let cavalry = spawn(&mut game, RED, "cavalry", 4, 5);
    for _ in 0..6 {
        spawn(&mut game, BLUE, "militia", 5, 5);
    }
    // Make the attackers unkillable for this test: lots of experience-level hit points.
    for id in [infantry, cavalry] {
        game.units.get_mut(&id).unwrap().level = 3;
    }
    run(
        &mut game,
        RED,
        Command::Attack {
            unit: infantry,
            target: Coord::new(5, 5),
        },
    )
    .unwrap();
    if game.units.contains_key(&infantry) {
        let again = run(
            &mut game,
            RED,
            Command::Attack {
                unit: infantry,
                target: Coord::new(5, 5),
            },
        );
        assert!(again.unwrap_err().0.contains("already attacked"));
    }
    let mut attacks = 0;
    for _ in 0..3 {
        if !game.units.contains_key(&cavalry)
            || unit(&game, cavalry).moves_left(&rules.units["cavalry"]) == 0
        {
            break;
        }
        if run(
            &mut game,
            RED,
            Command::Attack {
                unit: cavalry,
                target: Coord::new(5, 5),
            },
        )
        .is_ok()
        {
            attacks += 1;
        }
    }
    assert!(
        attacks >= 2 || !game.units.contains_key(&cavalry),
        "attacks: {attacks}"
    );
}

/// Fight many duels from different dice and report the attacker's win rate.
fn win_rate(attacker: &str, defender: &str, tile: &str, fortified: bool, trials: u64) -> f32 {
    let rows = vec![tile.repeat(4); 3];
    let mut wins = 0;
    for seed in 1..=trials {
        let mut game = arena_rows(&rows);
        game.rng = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        let a = spawn(&mut game, RED, attacker, 1, 0);
        let d = spawn(&mut game, BLUE, defender, 2, 0);
        if fortified {
            game.units.get_mut(&d).unwrap().order = Order::Fortified;
        }
        run(
            &mut game,
            RED,
            Command::Attack {
                unit: a,
                target: Coord::new(2, 0),
            },
        )
        .unwrap();
        wins += u64::from(!game.units.contains_key(&d) && game.units.contains_key(&a));
    }
    wins as f32 / trials as f32
}

#[test]
fn simulated_fights_match_the_calculated_chance() {
    let rules = rules();
    for (attacker, defender, tile, fortified) in [
        ("infantry", "infantry", ".", false),
        ("cavalry", "infantry", ".", true),
        ("cavalry", "infantry", "m", true),
        ("infantry", "cavalry", ".", false),
    ] {
        let mut game = arena_rows(&vec![tile.repeat(4); 3]);
        let a = spawn(&mut game, RED, attacker, 1, 0);
        let d = spawn(&mut game, BLUE, defender, 2, 0);
        if fortified {
            game.units.get_mut(&d).unwrap().order = Order::Fortified;
        }
        let estimate = game.estimate_attack(a, Coord::new(2, 0), &rules).unwrap();
        assert_eq!(estimate.defender, d);
        let measured = win_rate(attacker, defender, tile, fortified, 3000);
        assert!(
            (measured - estimate.win_chance).abs() < 0.04,
            "{attacker} vs {defender} on {tile:?} fortified={fortified}: measured {measured}, expected {}",
            estimate.win_chance
        );
    }
}

#[test]
fn terrain_and_fortifying_favor_the_defender() {
    let rules = rules();
    let odds = |tile: &str, fortified: bool| {
        let mut game = arena_rows(&vec![tile.repeat(4); 3]);
        let a = spawn(&mut game, RED, "cavalry", 1, 0);
        let d = spawn(&mut game, BLUE, "infantry", 2, 0);
        if fortified {
            game.units.get_mut(&d).unwrap().order = Order::Fortified;
        }
        game.estimate_attack(a, Coord::new(2, 0), &rules)
            .unwrap()
            .odds
    };
    // Infantry defense 4 against cavalry attack 8: grass 10%, forest 25%, mountain 100%.
    assert_eq!(odds(".", false), 1024 * 440 / (440 + 800));
    assert_eq!(odds("f", false), 1024 * 500 / (500 + 800));
    assert_eq!(odds("m", false), 1024 * 800 / (800 + 800));
    // Fortified mountains: Civ3's worked example, 8 against 4 with +125%.
    assert_eq!(odds("m", true), 542);
}

#[test]
fn the_best_defender_takes_the_blow_and_the_defenseless_do_not_defend() {
    let rules = rules();
    let mut game = arena(10, 10);
    let worker = spawn(&mut game, BLUE, "worker", 5, 5);
    let gun = spawn(&mut game, BLUE, "artillery", 5, 5);
    assert_eq!(game.best_defender(Coord::new(5, 5), BLUE, &rules), None);
    let militia = spawn(&mut game, BLUE, "militia", 5, 5);
    assert_eq!(
        game.best_defender(Coord::new(5, 5), BLUE, &rules),
        Some(militia)
    );
    let infantry = spawn(&mut game, BLUE, "infantry", 5, 5);
    assert_eq!(
        game.best_defender(Coord::new(5, 5), BLUE, &rules),
        Some(infantry)
    );
    // A badly wounded defender rates lower than a fresh weaker one.
    let cavalry = spawn(&mut game, BLUE, "cavalry", 5, 5);
    assert_eq!(
        game.best_defender(Coord::new(5, 5), BLUE, &rules),
        Some(infantry)
    );
    game.units.get_mut(&infantry).unwrap().damage = 3;
    assert_eq!(
        game.best_defender(Coord::new(5, 5), BLUE, &rules),
        Some(cavalry)
    );
    let _ = (worker, gun);
}

#[test]
fn defenseless_units_are_captured_not_fought() {
    let mut game = arena(10, 10);
    let rules = rules();
    let soldier = spawn(&mut game, RED, "infantry", 4, 4);
    let worker = spawn(&mut game, BLUE, "worker", 5, 5);
    let gun = spawn(&mut game, BLUE, "artillery", 5, 5);
    run(
        &mut game,
        RED,
        Command::Attack {
            unit: soldier,
            target: Coord::new(5, 5),
        },
    )
    .unwrap();
    assert!(game.at_war(RED, BLUE));
    for id in [worker, gun] {
        assert_eq!(unit(&game, id).owner, RED, "captured");
        assert_eq!(unit(&game, id).moves_left(rules.def(unit(&game, id))), 0);
    }
    // The captors stay where they were; nobody was killed.
    assert_eq!(unit(&game, soldier).position, Coord::new(4, 4));
    assert_eq!(game.units.len(), 3);
    assert!(
        run(
            &mut game,
            RED,
            Command::Attack {
                unit: gun,
                target: Coord::new(4, 5)
            }
        )
        .is_err()
    );
}

#[test]
fn artillery_cannot_attack_but_bombards_from_one_tile_away() {
    let mut game = arena(10, 10);
    let rules = rules();
    let gun = spawn(&mut game, RED, "artillery", 4, 4);
    let target = spawn(&mut game, BLUE, "infantry", 5, 5);
    let far = spawn(&mut game, BLUE, "infantry", 7, 7);
    let attack = run(
        &mut game,
        RED,
        Command::Attack {
            unit: gun,
            target: Coord::new(5, 5),
        },
    );
    assert!(attack.unwrap_err().0.contains("cannot attack"));
    let out_of_range = run(
        &mut game,
        RED,
        Command::Bombard {
            unit: gun,
            target: Coord::new(7, 7),
        },
    );
    assert!(out_of_range.unwrap_err().0.contains("range"));
    run(
        &mut game,
        RED,
        Command::Bombard {
            unit: gun,
            target: Coord::new(5, 5),
        },
    )
    .unwrap();
    assert!(game.at_war(RED, BLUE));
    assert!(unit(&game, target).damage <= 2);
    assert!(unit(&game, gun).attacked);
    assert_eq!(unit(&game, gun).moves_left(&rules.units["artillery"]), 0);
    let again = run(
        &mut game,
        RED,
        Command::Bombard {
            unit: gun,
            target: Coord::new(5, 5),
        },
    );
    assert!(again.is_err());
    assert_eq!(unit(&game, far).damage, 0);
}

#[test]
fn bombardment_wounds_but_never_kills() {
    let rules = rules();
    for seed in 1..=60u64 {
        let mut game = arena(10, 10);
        game.rng = seed.wrapping_mul(0x2545_F491_4F6C_DD1D) | 1;
        let target = spawn(&mut game, BLUE, "militia", 5, 5);
        let max = unit(&game, target).max_hp(&rules.units["militia"]);
        for _ in 0..8 {
            let gun = spawn(&mut game, RED, "artillery", 4, 4);
            let _ = run(
                &mut game,
                RED,
                Command::Bombard {
                    unit: gun,
                    target: Coord::new(5, 5),
                },
            );
        }
        let left = unit(&game, target).hp(&rules.units["militia"]);
        assert!((1..=max).contains(&left), "seed {seed}: {left} hp left");
    }
}

#[test]
fn artillery_defends_a_stack_by_shooting_the_attacker_first() {
    let rules = rules();
    // Cavalry (8) attacking infantry with artillery beside it takes extra damage on average.
    let mut with_gun = 0;
    let mut without = 0;
    for seed in 1..=400u64 {
        for gun in [true, false] {
            let mut game = arena(10, 10);
            game.rng = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
            let a = spawn(&mut game, RED, "cavalry", 4, 4);
            // The attacker is nearly unbreakable so the fight always runs to the end.
            game.units.get_mut(&a).unwrap().level = 3;
            spawn(&mut game, BLUE, "infantry", 5, 5);
            if gun {
                spawn(&mut game, BLUE, "artillery", 5, 5);
            }
            run(
                &mut game,
                RED,
                Command::Attack {
                    unit: a,
                    target: Coord::new(5, 5),
                },
            )
            .unwrap();
            let lost = game.units.get(&a).map_or(7, |u| u.damage);
            if gun {
                with_gun += lost
            } else {
                without += lost
            }
        }
    }
    assert!(with_gun > without, "{with_gun} vs {without}");
    let _ = rules;
}

#[test]
fn fast_units_can_withdraw_and_slow_ones_cannot() {
    let (mut retreated, mut slow_retreated) = (0, 0);
    for seed in 1..=600u64 {
        let mut game = arena(10, 10);
        game.rng = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        // Cavalry (fast) against militia (slow): only the attacker may withdraw.
        let a = spawn(&mut game, RED, "cavalry", 4, 4);
        let d = spawn(&mut game, BLUE, "militia", 5, 5);
        game.units.get_mut(&d).unwrap().level = 3;
        run(
            &mut game,
            RED,
            Command::Attack {
                unit: a,
                target: Coord::new(5, 5),
            },
        )
        .unwrap();
        if game.units.contains_key(&a) && game.units.contains_key(&d) {
            retreated += 1;
            assert_eq!(
                unit(&game, d).position,
                Coord::new(5, 5),
                "the defender held"
            );
        }
        // Militia (slow) attacking infantry (fast): only the defender may withdraw.
        let mut game = arena(10, 10);
        game.rng = seed.wrapping_mul(0x2545_F491_4F6C_DD1D) | 1;
        let a = spawn(&mut game, RED, "militia", 4, 4);
        game.units.get_mut(&a).unwrap().level = 3;
        let d = spawn(&mut game, BLUE, "infantry", 5, 5);
        run(
            &mut game,
            RED,
            Command::Attack {
                unit: a,
                target: Coord::new(5, 5),
            },
        )
        .unwrap();
        if game.units.contains_key(&a) && game.units.contains_key(&d) {
            slow_retreated += 1;
            assert_ne!(
                unit(&game, d).position,
                Coord::new(5, 5),
                "the defender slipped away"
            );
            assert_eq!(
                unit(&game, d).position,
                Coord::new(6, 6),
                "straight away from the attacker"
            );
        }
    }
    assert!(retreated > 20, "{retreated}");
    assert!(slow_retreated > 20, "{slow_retreated}");
}

#[test]
fn winners_are_promoted_and_level_three_is_the_ceiling() {
    let mut promoted = 0;
    for seed in 1..=300u64 {
        let mut game = arena(10, 10);
        game.rng = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        let a = spawn(&mut game, RED, "cavalry", 4, 4);
        spawn(&mut game, BLUE, "militia", 5, 5);
        run(
            &mut game,
            RED,
            Command::Attack {
                unit: a,
                target: Coord::new(5, 5),
            },
        )
        .unwrap();
        if let Some(u) = game.units.get(&a) {
            assert!(u.level <= 3);
            promoted += usize::from(u.level > STARTING_LEVEL);
        }
    }
    assert!(promoted > 20, "{promoted}");
    // Elite units never roll again.
    let mut game = arena(10, 10);
    let a = spawn(&mut game, RED, "cavalry", 4, 4);
    game.units.get_mut(&a).unwrap().level = 3;
    game.promote(a);
    assert_eq!(unit(&game, a).level, 3);
    // A unit that already failed a roll this turn is promoted without one.
    game.units.get_mut(&a).unwrap().level = 1;
    game.units.get_mut(&a).unwrap().promotion_failed = true;
    game.promote(a);
    assert_eq!(unit(&game, a).level, 2);
}

#[test]
fn resting_units_heal_faster_in_cities_and_not_after_moving() {
    let rules = rules();
    let mut game = arena(10, 10);
    let field = spawn(&mut game, RED, "infantry", 5, 5);
    let city = spawn(&mut game, RED, "infantry", 1, 1);
    let moved = spawn(&mut game, RED, "infantry", 6, 6);
    let foreign = spawn(&mut game, BLUE, "infantry", 7, 7);
    for id in [field, city, moved, foreign] {
        game.units.get_mut(&id).unwrap().damage = 3;
    }
    game.units.get_mut(&moved).unwrap().moves_used = 3;
    end_turn(&mut game);
    assert_eq!(unit(&game, field).damage, 2);
    assert_eq!(unit(&game, city).damage, 1);
    assert_eq!(unit(&game, moved).damage, 3);
    assert_eq!(unit(&game, foreign).damage, 2, "its own land");
    assert_eq!(
        game.map.get(Coord::new(5, 5)).unwrap().owner,
        0,
        "the field is unclaimed"
    );
    // Foreign soil does not heal.
    let intruder = spawn(&mut game, RED, "infantry", 6, 6);
    game.units.get_mut(&intruder).unwrap().damage = 2;
    end_turn(&mut game);
    assert_eq!(unit(&game, intruder).damage, 2, "inside Blueton's border");
    let _ = rules;
}

#[test]
fn an_empty_city_is_taken_by_walking_in_and_its_land_goes_with_it() {
    let mut game = arena(14, 14);
    let city = game.city_at(Coord::new(12, 12)).unwrap();
    let before = game.territory_of(city);
    assert!(before.len() >= 21, "a capital claims its preset region");
    assert!(
        before
            .iter()
            .all(|p| game.map.get(*p).unwrap().owner == BLUE)
    );
    let soldier = spawn(&mut game, RED, "infantry", 11, 11);
    run(
        &mut game,
        RED,
        Command::Attack {
            unit: soldier,
            target: Coord::new(12, 12),
        },
    )
    .unwrap();
    assert!(game.at_war(RED, BLUE));
    assert_eq!(game.cities[&city].owner, RED);
    assert_eq!(unit(&game, soldier).position, Coord::new(12, 12));
    assert_eq!(
        game.territory_of(city),
        before,
        "the region itself never changes"
    );
    assert!(
        before
            .iter()
            .all(|p| game.map.get(*p).unwrap().owner == RED)
    );
    // Blue has no cities left, which ends the campaign.
    end_turn(&mut game);
    assert_eq!(game.winner, Some(RED));
}

#[test]
fn defended_cities_must_be_fought_for() {
    let mut game = arena(14, 14);
    let city = game.city_at(Coord::new(12, 12)).unwrap();
    spawn(&mut game, BLUE, "infantry", 12, 12);
    let gun = spawn(&mut game, BLUE, "artillery", 12, 12);
    let soldier = spawn(&mut game, RED, "cavalry", 11, 11);
    game.units.get_mut(&soldier).unwrap().level = 3;
    for _ in 0..3 {
        if game.units.contains_key(&soldier) && game.cities[&city].owner == BLUE {
            let _ = run(
                &mut game,
                RED,
                Command::Attack {
                    unit: soldier,
                    target: Coord::new(12, 12),
                },
            );
        }
    }
    if game.cities[&city].owner == RED {
        // Only possible by winning every fight, which then captured the artillery too.
        assert_eq!(unit(&game, gun).owner, RED);
    }
}

#[test]
fn borders_are_fixed_per_city_and_follow_the_nearest_city() {
    let mut game = arena(24, 12);
    let rules = rules();
    // Add two more red cities by hand: one close to the capital, one far away.
    let near = game.add_city(RED, Coord::new(4, 1), "Near".into());
    game.cities.get_mut(&near).unwrap().border = 2;
    game.index_city(near);
    let far = game.add_city(BLUE, Coord::new(11, 6), "Far".into());
    game.cities.get_mut(&far).unwrap().border = 3;
    game.index_city(far);
    game.assign_start_territory();
    let capital = game.city_at(Coord::new(1, 1)).unwrap();
    // Every owned tile belongs to the nearest city's region, and borders combine by nation.
    for tile in &game.map.tiles {
        match tile.claim {
            0 => assert_eq!(tile.owner, 0),
            id => {
                assert_eq!(tile.owner, game.cities[&id].owner);
                let own = tile.position.distance_squared(game.cities[&id].position);
                assert!(own <= borders::reach_squared(game.cities[&id].border));
                for other in game.cities.values() {
                    assert!(
                        other.position.distance_squared(tile.position) >= own
                            || !game_reaches(&game, other, tile.position),
                        "{:?} is nearer to {} than to its claimant",
                        tile.position,
                        other.name
                    );
                }
            }
        }
    }
    assert_eq!(game.map.get(Coord::new(2, 1)).unwrap().claim, capital);
    assert_eq!(game.map.get(Coord::new(3, 1)).unwrap().claim, near);
    assert_eq!(game.map.get(Coord::new(2, 1)).unwrap().owner, RED);
    assert_eq!(game.map.get(Coord::new(3, 1)).unwrap().owner, RED);
    // Level 3 reaches 37 tiles, level 2 reaches 21, and a city never loses its own square.
    assert!(game.territory_of(far).len() > game.territory_of(near).len());
    assert_eq!(game.map.get(Coord::new(11, 6)).unwrap().claim, far);
    // Time passing changes nothing.
    let snapshot = game.map.clone();
    for _ in 0..30 {
        end_turn(&mut game);
    }
    assert_eq!(
        game.map
            .tiles
            .iter()
            .map(|t| (t.claim, t.owner))
            .collect::<Vec<_>>(),
        snapshot
            .tiles
            .iter()
            .map(|t| (t.claim, t.owner))
            .collect::<Vec<_>>()
    );
    let _ = rules;
}

fn game_reaches(game: &Game, city: &City, p: Coord) -> bool {
    let d = p.distance_squared(city.position);
    d <= borders::reach_squared(city.border) && (game.map.get(p).unwrap().is_land() || d <= 5)
}

#[test]
fn a_new_city_takes_only_unclaimed_land() {
    let mut game = arena(20, 12);
    let rules = rules();
    let capital = game.city_at(Coord::new(1, 1)).unwrap();
    let held: Vec<Coord> = game.territory_of(capital);
    // A pioneer inside red's own border cannot found a city within three tiles of one,
    // but can farther out; the new city never steals existing land.
    let pioneer = spawn(&mut game, RED, "pioneer", 4, 4);
    run(
        &mut game,
        RED,
        Command::FoundCity {
            unit: pioneer,
            name: "Fourth".into(),
        },
    )
    .unwrap();
    let new = game.city_at(Coord::new(4, 4)).unwrap();
    assert!(!game.units.contains_key(&pioneer));
    assert!(game.territory_of(capital).len() + game.territory_of(new).len() >= held.len());
    for p in &held {
        let tile = game.map.get(*p).unwrap();
        assert!(tile.claim == capital || *p == Coord::new(4, 4), "{p:?}");
    }
    assert_eq!(game.map.get(Coord::new(4, 4)).unwrap().claim, new);
    assert!(game.territory_of(new).len() > 1);
    assert_eq!(game.cities[&new].border, borders::default_level(3, false));
    // Foreign territory is off limits.
    let intruder = spawn(&mut game, RED, "pioneer", 15, 10);
    let err = run(
        &mut game,
        RED,
        Command::FoundCity {
            unit: intruder,
            name: "Nope".into(),
        },
    );
    assert!(
        err.unwrap_err().0.contains("foreign"),
        "blue's capital reaches (15,10)"
    );
    let _ = rules;
}

#[test]
fn workers_build_roads_and_work_rates_add_up() {
    let mut game = arena(12, 12);
    let w = spawn(&mut game, RED, "worker", 5, 5);
    let p = Coord::new(5, 5);
    assert_eq!(game.work_required(p, Job::Road), 6);
    assert_eq!(game.work_required(p, Job::Rail), 12);
    run(
        &mut game,
        RED,
        Command::Work {
            unit: w,
            job: Job::Road,
        },
    )
    .unwrap();
    assert_eq!(unit(&game, w).work, 2);
    assert_eq!(unit(&game, w).moves_used, 6, "working uses the whole turn");
    assert!(!game.map.get(p).unwrap().has_road());
    end_turn(&mut game);
    assert_eq!(unit(&game, w).work, 4);
    end_turn(&mut game);
    assert!(game.map.get(p).unwrap().has_road());
    assert_eq!(unit(&game, w).order, Order::None);
    assert_eq!(unit(&game, w).work, 0);
    // A railroad needs the road; two workers finish twice as fast.
    let helper = spawn(&mut game, RED, "worker", 5, 5);
    run(
        &mut game,
        RED,
        Command::Work {
            unit: w,
            job: Job::Rail,
        },
    )
    .unwrap();
    run(
        &mut game,
        RED,
        Command::Work {
            unit: helper,
            job: Job::Rail,
        },
    )
    .unwrap();
    // The first worker had already spent its turn finishing the road, so it joins next turn.
    for _ in 0..3 {
        end_turn(&mut game);
    }
    assert!(
        game.map.get(p).unwrap().has_rail(),
        "12 work from two workers at rate 2"
    );
    assert!(game.map.get(p).unwrap().has_road(), "rail keeps the road");
}

#[test]
fn jobs_cost_more_on_rough_ground_and_follow_the_terrain_rules() {
    let rows: Vec<String> = ["~...m.,,", "~...m.,,", "ff..m.,,", "........"]
        .map(String::from)
        .into();
    let mut game = arena_rows(&rows);
    let mountain = Coord::new(4, 0);
    assert_eq!(
        game.work_required(mountain, Job::Road),
        18,
        "6 x 3 move cost"
    );
    assert_eq!(
        game.work_required(Coord::new(0, 2), Job::Road),
        12,
        "forest costs 2"
    );
    // Rail needs a road; mines only on mountains and desert; farms need water and open ground.
    assert_eq!(
        game.can_improve(Coord::new(2, 1), Job::Rail),
        Err("A railroad needs a road first")
    );
    assert!(game.can_improve(Coord::new(2, 1), Job::Mine).is_err());
    assert!(game.can_improve(mountain, Job::Mine).is_ok());
    assert!(game.can_improve(Coord::new(6, 1), Job::Mine).is_ok());
    assert!(game.can_improve(Coord::new(0, 2), Job::Mine).is_err());
    assert!(
        game.can_improve(Coord::new(0, 2), Job::Farm).is_err(),
        "forest"
    );
    assert!(game.can_improve(mountain, Job::Farm).is_err(), "mountain");
    assert!(
        game.can_improve(Coord::new(1, 0), Job::Farm).is_ok(),
        "next to the sea"
    );
    assert!(
        game.can_improve(Coord::new(3, 1), Job::Farm).is_err(),
        "dry land"
    );
    assert!(
        game.can_improve(Coord::new(2, 1), Job::Farm).is_ok(),
        "beside a coastal city"
    );
    // A farm makes its neighbour irrigable.
    game.map.get_mut(Coord::new(2, 1)).unwrap().improvements = Tile::FARM;
    assert!(game.can_improve(Coord::new(3, 1), Job::Farm).is_ok());
    assert!(game.can_improve(Coord::new(3, 2), Job::Farm).is_ok());
    assert!(game.can_improve(Coord::new(5, 1), Job::Farm).is_err());
    // Water and city squares are off limits.
    assert!(game.can_improve(Coord::new(0, 0), Job::Road).is_err());
    assert!(game.can_improve(Coord::new(1, 1), Job::Mine).is_err());
}

#[test]
fn mines_and_farms_replace_each_other_and_builders_must_stay_put() {
    let rows: Vec<String> = ["~,,,,,,,", ",,,,,,,,", "........", "........"]
        .map(String::from)
        .into();
    let mut game = arena_rows(&rows);
    let p = Coord::new(3, 1);
    game.map.get_mut(p).unwrap().improvements = Tile::FARM;
    let w = spawn(&mut game, RED, "worker", 3, 1);
    let helper = spawn(&mut game, RED, "worker", 3, 1);
    for id in [w, helper] {
        run(
            &mut game,
            RED,
            Command::Work {
                unit: id,
                job: Job::Mine,
            },
        )
        .unwrap();
    }
    for _ in 0..4 {
        end_turn(&mut game);
    }
    let tile = game.map.get(p).unwrap();
    assert!(tile.improvements & Tile::MINE != 0 && tile.improvements & Tile::FARM == 0);
    // Leaving a job unfinished forfeits the work.
    let q = Coord::new(5, 1);
    let w2 = spawn(&mut game, RED, "worker", 5, 1);
    run(
        &mut game,
        RED,
        Command::Work {
            unit: w2,
            job: Job::Road,
        },
    )
    .unwrap();
    assert_eq!(unit(&game, w2).work, 2);
    end_turn(&mut game);
    run(
        &mut game,
        RED,
        Command::Move {
            unit: w2,
            destination: Coord::new(6, 1),
        },
    )
    .unwrap();
    assert_eq!(unit(&game, w2).work, 0);
    assert_eq!(unit(&game, w2).order, Order::None);
    assert!(!game.map.get(q).unwrap().has_road());
    // Fighters cannot build.
    let s = spawn(&mut game, RED, "infantry", 3, 2);
    assert!(
        run(
            &mut game,
            RED,
            Command::Work {
                unit: s,
                job: Job::Road
            }
        )
        .is_err()
    );
}

#[test]
fn improvements_survive_json_and_reject_nonsense() {
    let mut game = arena(6, 3);
    game.map.get_mut(Coord::new(2, 1)).unwrap().improvements = Tile::ROAD | Tile::RAIL;
    game.map.get_mut(Coord::new(3, 1)).unwrap().improvements = Tile::MINE;
    let json = serde_json::to_string(&game).unwrap();
    let mut back: Game = serde_json::from_str(&json).unwrap();
    assert_eq!(back, game);
    back.reindex();
    assert_eq!(
        back.city_at(Coord::new(1, 1)),
        game.city_at(Coord::new(1, 1))
    );
    let bad = json.replace(r#""improvements":["#, r#""improvements":[2,"#);
    assert!(serde_json::from_str::<Game>(&bad).is_err());
}

#[test]
fn runs_are_deterministic_and_saves_roundtrip() {
    let play = || {
        let mut game = arena(16, 16);
        let a = spawn(&mut game, RED, "infantry", 6, 6);
        spawn(&mut game, BLUE, "infantry", 7, 7);
        run(
            &mut game,
            RED,
            Command::Attack {
                unit: a,
                target: Coord::new(7, 7),
            },
        )
        .unwrap();
        for _ in 0..10 {
            end_turn(&mut game);
        }
        game
    };
    let (one, two) = (play(), play());
    assert_eq!(one, two);
    let mut loaded: Game = serde_json::from_str(&serde_json::to_string(&one).unwrap()).unwrap();
    assert_eq!(loaded, one);
    // A reloaded game keeps playing identically, with or without the lookup tables.
    let mut fresh = one.clone();
    loaded.reindex();
    for _ in 0..5 {
        end_turn(&mut loaded);
        end_turn(&mut fresh);
    }
    assert_eq!(loaded, fresh);
}

#[test]
fn players_see_only_nearby_foreign_units() {
    let mut game = arena(40, 40);
    let mine = spawn(&mut game, RED, "infantry", 10, 10);
    let near = spawn(&mut game, BLUE, "infantry", 13, 13);
    let far = spawn(&mut game, BLUE, "infantry", 30, 30);
    let view = game.view(RED);
    assert!(view.units.contains_key(&mine) && view.units.contains_key(&near));
    assert!(!view.units.contains_key(&far));
    assert_eq!(
        view.units_at(Coord::new(13, 13)).len(),
        1,
        "lookups work without tables"
    );
    assert_eq!(
        view.city_at(Coord::new(1, 1)),
        game.city_at(Coord::new(1, 1))
    );
}

#[test]
fn computer_nations_fight_wars_and_the_world_keeps_turning() {
    let mut game = arena(30, 14);
    // Blue owns cavalry and artillery and is at war with the commander.
    game.wars.insert((RED, BLUE));
    for (kind, x) in [("infantry", 27), ("cavalry", 26), ("artillery", 26)] {
        spawn(&mut game, BLUE, kind, x, 12);
    }
    spawn(&mut game, RED, "infantry", 1, 1);
    let blue_cities = game.cities.values().filter(|c| c.owner == BLUE).count();
    let mut reached_fight = false;
    for _ in 0..60 {
        end_turn(&mut game);
        if game.winner.is_some() {
            break;
        }
        reached_fight |= game
            .log
            .iter()
            .any(|l| l.contains("defeated") || l.contains("lost attacking"));
        for unit in game.units.values() {
            assert!(game.map.get(unit.position).is_some());
            assert!(unit.damage < 10);
        }
    }
    assert!(blue_cities >= 1);
    assert!(
        reached_fight || game.winner.is_some(),
        "the war was joined: {:?}",
        game.log
    );
}
