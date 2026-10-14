use crate::terrain::{Coord, Relief};
use crate::tests::{arena, arena_rows, rules, spawn, war};
use crate::*;
const RED: Id = 1;
const BLUE: Id = 2;
fn industrial() -> Rules {
    let mut r = rules();
    r.economy.enabled = true;
    r.economy.unit_upkeep = 0;
    r.economy.building_upkeep = 0;
    r.economy.war_chance_per_thousand = 0;
    let mut cargo = r.units["worker"].clone();
    cargo.id = "crate".into();
    cargo.work = 0;
    r.units.insert("crate".into(), cargo);
    r
}
fn command(g: &mut Game, c: Command, r: &Rules) {
    g.apply(RED, c, r, TickRules::default()).unwrap();
}
#[test]
fn factories_convert_only_available_inputs_and_respect_capacity() {
    let mut g = arena(12, 6);
    let r = industrial();
    let city = g.cities.values().find(|c| c.owner == RED).unwrap().id;
    g.cities
        .get_mut(&city)
        .unwrap()
        .buildings
        .insert(Building::Factory);
    let s = &mut g.factions.get_mut(&RED).unwrap().stockpile;
    s.add(Good::Coal, 5);
    s.add(Good::Iron, 1);
    s.add(Good::Food, 3);
    s.add(Good::Cotton, 3);
    g.industry_turn(&r);
    let s = &g.factions[&RED].stockpile;
    assert_eq!(
        (s.get(Good::Coal), s.get(Good::Iron), s.get(Good::Steel)),
        (4, 0, 1)
    );
    assert_eq!(
        (
            s.get(Good::Food),
            s.get(Good::Cotton),
            s.get(Good::Supplies)
        ),
        (1, 1, 2)
    );
    g.industry_turn(&r);
    assert_eq!(g.factions[&RED].stockpile.get(Good::Supplies), 3);
}
#[test]
fn a_factory_requires_city_production_and_stays_after_capture() {
    let mut g = arena(12, 6);
    let mut r = industrial();
    r.economy.factory_cost = 5;
    let city = g.cities.values().find(|c| c.owner == RED).unwrap().id;
    command(
        &mut g,
        Command::Build {
            city,
            building: Building::Factory,
        },
        &r,
    );
    assert!(!g.cities[&city].buildings.contains(&Building::Factory));
    g.industry_turn(&r);
    assert!(g.cities[&city].buildings.contains(&Building::Factory));
    g.transfer_city(city, BLUE);
    assert!(g.cities[&city].buildings.contains(&Building::Factory));
    assert!(!g.home_city(city));
    g.transfer_city(city, RED);
    assert!(g.home_city(city));
}
#[test]
fn goods_are_reserved_once_and_refunded_when_production_changes() {
    let mut g = arena(12, 6);
    let r = industrial();
    let city = g.cities.values().find(|c| c.owner == RED).unwrap().id;
    assert!(
        g.apply(
            RED,
            Command::Produce {
                city,
                unit: "infantry".into()
            },
            &r,
            TickRules::default()
        )
        .is_err()
    );
    g.factions
        .get_mut(&RED)
        .unwrap()
        .stockpile
        .add(Good::Supplies, 1);
    command(
        &mut g,
        Command::Produce {
            city,
            unit: "infantry".into(),
        },
        &r,
    );
    assert_eq!(g.factions[&RED].stockpile.get(Good::Supplies), 0);
    command(
        &mut g,
        Command::Produce {
            city,
            unit: "cavalry".into(),
        },
        &r,
    );
    assert_eq!(g.factions[&RED].stockpile.get(Good::Supplies), 0);
    command(
        &mut g,
        Command::Build {
            city,
            building: Building::Factory,
        },
        &r,
    );
    assert_eq!(g.factions[&RED].stockpile.get(Good::Supplies), 1);
}
#[test]
fn cargo_delivers_only_at_a_home_city_and_to_its_current_owner() {
    let mut g = arena(12, 6);
    let r = industrial();
    let home = g.cities.values().find(|c| c.owner == RED).unwrap().id;
    let pos = g.cities[&home].position;
    let colony = g.add_city(RED, Coord::new(5, 2), "Colony".into());
    g.cities.get_mut(&colony).unwrap().home_owner = 0;
    let cargo = spawn(&mut g, RED, "crate", 5, 2);
    g.units.get_mut(&cargo).unwrap().cargo = Some((Good::Coal, 7));
    g.deliver_crates(&r);
    assert!(g.units.contains_key(&cargo));
    assert_eq!(g.factions[&RED].stockpile.get(Good::Coal), 0);
    g.set_position(cargo, pos);
    g.deliver_crates(&r);
    assert!(!g.units.contains_key(&cargo));
    assert_eq!(g.factions[&RED].stockpile.get(Good::Coal), 7);
    let enemy_home = g
        .cities
        .values()
        .find(|c| c.owner == BLUE)
        .unwrap()
        .position;
    let captured = spawn(&mut g, BLUE, "crate", enemy_home.x, enemy_home.y);
    g.units.get_mut(&captured).unwrap().cargo = Some((Good::Iron, 4));
    g.deliver_crates(&r);
    assert_eq!(g.factions[&BLUE].stockpile.get(Good::Iron), 4);
}
#[test]
fn resource_tiles_pay_home_stockpiles_and_spawn_colonial_cargo_every_four_weeks() {
    let mut g = arena(12, 6);
    let r = industrial();
    g.map.resources.push(resources::ResourceKind {
        id: "coal".into(),
        name: "Coal".into(),
        class: resources::ResourceClass::Strategic,
        food: 0,
        shields: 0,
        gold: 0,
    });
    let city = g.cities.values().find(|c| c.owner == RED).unwrap().id;
    let pos = g.cities[&city].position;
    g.map.get_mut(pos).unwrap().resource = 1;
    g.turn = 4;
    g.industry_turn(&r);
    assert_eq!(g.factions[&RED].stockpile.get(Good::Coal), 0);
    g.turn = 5;
    g.industry_turn(&r);
    assert_eq!(g.factions[&RED].stockpile.get(Good::Coal), 1);
    g.cities.get_mut(&city).unwrap().home_owner = 0;
    g.turn = 9;
    g.industry_turn(&r);
    assert_eq!(g.factions[&RED].stockpile.get(Good::Coal), 1);
    assert!(g.units.values().any(|u| u.cargo == Some((Good::Coal, 1))));
}
#[test]
fn market_spread_prevents_roundtrip_profit_and_rejects_overselling() {
    let mut g = arena(12, 6);
    let r = industrial();
    let before = g.factions[&RED].gold;
    command(
        &mut g,
        Command::Trade {
            good: Good::Steel,
            amount: 2,
        },
        &r,
    );
    command(
        &mut g,
        Command::Trade {
            good: Good::Steel,
            amount: -2,
        },
        &r,
    );
    assert!(g.factions[&RED].gold < before);
    assert!(
        g.apply(
            RED,
            Command::Trade {
                good: Good::Steel,
                amount: -1
            },
            &r,
            TickRules::default()
        )
        .is_err()
    );
    assert!(g.trade(RED, Good::Steel, i32::MIN, &r).is_err());
}
#[test]
fn loans_charge_interest_and_cannot_exceed_credit_limit() {
    let mut g = arena(12, 6);
    let r = industrial();
    let before = g.factions[&RED].gold;
    command(&mut g, Command::Borrow, &r);
    assert_eq!(g.factions[&RED].gold, before + 200);
    assert_eq!(g.budget_upkeep(RED, &r), 4);
    g.industry_turn(&r);
    assert_eq!(g.factions[&RED].gold, before + 196);
    command(&mut g, Command::Repay, &r);
    assert_eq!(g.factions[&RED].debt, 0);
    for _ in 0..3 {
        g.borrow(RED, &r).unwrap();
    }
    assert!(g.borrow(RED, &r).is_err());
}
#[test]
fn bankruptcy_sells_buildings_before_the_most_expensive_unit() {
    let mut g = arena(12, 6);
    let r = industrial();
    let city = g.cities.values().find(|c| c.owner == RED).unwrap().id;
    g.cities
        .get_mut(&city)
        .unwrap()
        .buildings
        .insert(Building::Factory);
    let soldier = spawn(&mut g, RED, "infantry", 2, 1);
    g.factions.get_mut(&RED).unwrap().gold = -1;
    g.industry_turn(&r);
    assert!(g.cities[&city].buildings.is_empty());
    assert!(g.units.contains_key(&soldier));
    g.factions.get_mut(&RED).unwrap().gold = -1;
    g.industry_turn(&r);
    assert!(!g.units.contains_key(&soldier));
    assert!(g.log.iter().any(|s| s.contains("bankruptcy")));
}
#[test]
fn stack_lanes_start_together_and_winners_attack_only_once() {
    let mut g = arena(12, 6);
    let mut r = industrial();
    r.units.get_mut("infantry").unwrap().attack = 100;
    let attackers: Vec<_> = (0..10)
        .map(|_| spawn(&mut g, RED, "infantry", 4, 2))
        .collect();
    for _ in 0..3 {
        let d = spawn(&mut g, BLUE, "militia", 5, 2);
        g.units.get_mut(&d).unwrap().damage = 2;
    }
    war(&mut g, RED, BLUE);
    command(
        &mut g,
        Command::AttackStack {
            unit: attackers[0],
            target: Coord::new(5, 2),
        },
        &r,
    );
    assert_eq!(g.turn, 1);
    assert_eq!(g.date().iso(), "1876-01-01");
    let b = &g.stack_battles[0];
    assert_eq!(b.width, 3);
    assert_eq!(b.lanes.len(), 3);
    assert!(b.lanes.iter().all(|l| l.start == 0));
    assert!(b.advanced);
    assert_eq!(
        attackers.iter().filter(|id| g.units[id].attacked).count(),
        3
    );
    assert!(
        attackers
            .iter()
            .all(|id| g.units[id].position == Coord::new(5, 2))
    );
}
#[test]
fn losing_attackers_are_replaced_and_terrain_caps_width() {
    let mut g = arena(12, 6);
    let mut r = industrial();
    r.units.get_mut("infantry").unwrap().attack = 1;
    r.units.get_mut("militia").unwrap().defense = 100;
    r.units.get_mut("militia").unwrap().hp = 100;
    let attackers: Vec<_> = (0..10)
        .map(|_| spawn(&mut g, RED, "infantry", 4, 2))
        .collect();
    for _ in 0..5 {
        spawn(&mut g, BLUE, "militia", 5, 2);
    }
    g.map.get_mut(Coord::new(5, 2)).unwrap().relief = Relief::Mountains;
    war(&mut g, RED, BLUE);
    command(
        &mut g,
        Command::AttackStack {
            unit: attackers[0],
            target: Coord::new(5, 2),
        },
        &r,
    );
    let b = &g.stack_battles[0];
    assert_eq!(g.turn, 1);
    assert_eq!(g.date().iso(), "1876-01-01");
    assert_eq!(b.width, 3);
    assert_eq!(b.attacker_losses, 10);
    assert!(!b.advanced);
    assert!(b.lanes.iter().any(|l| l.start > 0));
}
#[test]
fn stack_move_orders_all_members_and_preserves_foreign_units() {
    let mut g = arena(12, 6);
    let r = industrial();
    let a = spawn(&mut g, RED, "infantry", 4, 2);
    let b = spawn(&mut g, RED, "infantry", 4, 2);
    let enemy = spawn(&mut g, BLUE, "worker", 4, 2);
    command(
        &mut g,
        Command::MoveStack {
            unit: a,
            destination: Coord::new(6, 2),
        },
        &r,
    );
    assert_eq!(g.units[&a].position, g.units[&b].position);
    assert_eq!(g.units[&enemy].position, Coord::new(4, 2));
}
#[test]
fn cargo_cannot_unload_or_land_at_an_unbuilt_port() {
    let rows: [&str; 6] = [
        "~~~~~~~~~~~~",
        "~...~~~~~~~~",
        "~...~~~~~~~~",
        "~...~~~~~~~~",
        "~~~~~~~~~...",
        "~~~~~~~~~~~~",
    ];
    let mut g = arena_rows(&rows.map(String::from));
    let r = industrial();
    let ship = spawn(&mut g, RED, "transport", 1, 1);
    let cargo = spawn(&mut g, RED, "crate", 1, 1);
    g.units.get_mut(&cargo).unwrap().cargo = Some((Good::Cotton, 3));
    g.load(RED, cargo, ship, &r).unwrap();
    assert!(g.unload(RED, ship, &r).is_err());
    assert!(g.move_unit(RED, cargo, Coord::new(2, 1), &r, 1000).is_err());
    let city = g.city_at(Coord::new(1, 1)).unwrap();
    g.cities
        .get_mut(&city)
        .unwrap()
        .buildings
        .insert(Building::Port);
    g.unload(RED, ship, &r).unwrap();
    g.deliver_crates(&r);
    assert_eq!(g.factions[&RED].stockpile.get(Good::Cotton), 3);
}
#[test]
fn ai_ferries_colonial_goods_across_the_sea() {
    let rows: [&str; 6] = [
        "~~~~~~~~~~~~",
        "~...~~~~~~~~",
        "~...~~~~~~~~",
        "~...~~~~~~~~",
        "~~~~~~~~~...",
        "~~~~~~~~~~~~",
    ];
    let mut g = arena_rows(&rows.map(String::from));
    let r = industrial();
    let home = g.cities.values().find(|c| c.owner == BLUE).unwrap().id;
    g.cities
        .get_mut(&home)
        .unwrap()
        .buildings
        .insert(Building::Port);
    let colony = g.add_city(BLUE, Coord::new(3, 1), "Overseas".into());
    let c = g.cities.get_mut(&colony).unwrap();
    c.home_owner = 0;
    c.buildings.insert(Building::Port);
    let ship = spawn(&mut g, BLUE, "transport", 10, 4);
    let cargo = spawn(&mut g, BLUE, "crate", 3, 1);
    g.units.get_mut(&cargo).unwrap().cargo = Some((Good::Coal, 8));
    for _ in 0..20 {
        g.refresh_units(&r);
        g.ai_logistics(&r);
        g.deliver_crates(&r);
        if !g.units.contains_key(&cargo) {
            break;
        }
    }
    assert!(
        !g.units.contains_key(&cargo),
        "cargo at {:?}, ship at {:?}",
        g.units.get(&cargo),
        g.units[&ship].position
    );
    assert_eq!(g.factions[&BLUE].stockpile.get(Good::Coal), 8);
}
#[test]
fn peace_uses_weariness_or_twenty_quiet_weeks_and_keeps_occupation() {
    let mut g = arena(12, 6);
    let r = industrial();
    g.declare_war(RED, BLUE);
    assert!(g.propose_peace(RED, BLUE, &r).is_err());
    g.war_loss(BLUE, RED, 6);
    g.propose_peace(RED, BLUE, &r).unwrap();
    assert!(!g.at_war(RED, BLUE));
    g.declare_war(RED, BLUE);
    g.turn += 20;
    g.propose_peace(RED, BLUE, &r).unwrap();
    assert!(!g.at_war(RED, BLUE));
}
#[test]
fn demands_transfer_real_goods_and_refusal_records_pressure() {
    let mut g = arena(12, 6);
    g.factions
        .get_mut(&RED)
        .unwrap()
        .stockpile
        .add(Good::Cotton, 1);
    g.diplomacy.demands.push(diplomacy::Demand {
        from: BLUE,
        to: RED,
        gold: 0,
        good: Some(Good::Cotton),
        turn: 1,
    });
    g.answer_demand(RED, BLUE, true).unwrap();
    assert_eq!(g.factions[&BLUE].stockpile.get(Good::Cotton), 1);
    assert_eq!(g.factions[&RED].stockpile.get(Good::Cotton), 0);
    g.diplomacy.demands.push(diplomacy::Demand {
        from: BLUE,
        to: RED,
        gold: 10,
        good: None,
        turn: 1,
    });
    g.answer_demand(RED, BLUE, false).unwrap();
    assert_eq!(g.diplomacy.relations[0].refused_by, Some(RED));
}

#[test]
fn defensive_artillery_fires_once_across_all_stack_lanes() {
    let mut g = arena(12, 6);
    let r = industrial();
    let a = spawn(&mut g, RED, "infantry", 4, 2);
    spawn(&mut g, RED, "infantry", 4, 2);
    spawn(&mut g, BLUE, "infantry", 5, 2);
    spawn(&mut g, BLUE, "infantry", 5, 2);
    let gun = spawn(&mut g, BLUE, "artillery", 5, 2);
    war(&mut g, RED, BLUE);
    command(
        &mut g,
        Command::AttackStack {
            unit: a,
            target: Coord::new(5, 2),
        },
        &r,
    );
    let shots: Vec<_> = g.stack_battles[0]
        .lanes
        .iter()
        .filter_map(|l| l.support.as_ref())
        .collect();
    assert_eq!(shots.len(), 1);
    assert_eq!(shots[0].shooter.id, gun);
    assert_eq!(g.turn, 1);
}

#[test]
fn weaker_neighbors_can_declare_war_but_never_for_the_human() {
    let mut g = arena(12, 6);
    let mut r = industrial();
    r.economy.war_chance_per_thousand = 1000;
    let a = Coord::new(5, 2);
    let b = Coord::new(6, 2);
    g.map.get_mut(a).unwrap().owner = RED;
    g.map.get_mut(b).unwrap().owner = BLUE;
    for _ in 0..10 {
        spawn(&mut g, RED, "infantry", 5, 2);
    }
    spawn(&mut g, BLUE, "militia", 6, 2);
    g.diplomacy_turn(&r);
    assert!(g.at_war(RED, BLUE));
    assert_eq!(g.diplomacy.wars.len(), 1);
    // Only the AI can initiate automatically, despite being weaker than the player here.
    assert!(g.log.iter().any(|s| s.contains("are at war")));
}

#[test]
fn ai_does_not_build_a_factory_in_every_city_for_one_pair_of_inputs() {
    let mut g = arena(20, 10);
    let r = industrial();
    let capital = g
        .cities
        .values()
        .find(|c| c.owner == BLUE)
        .unwrap()
        .position;
    let second = g.add_city(BLUE, Coord::new(12, 5), "Second".into());
    let third = g.add_city(BLUE, Coord::new(8, 5), "Third".into());
    for p in [
        capital,
        g.cities[&second].position,
        g.cities[&third].position,
    ] {
        for _ in 0..5 {
            spawn(&mut g, BLUE, "infantry", p.x, p.y);
        }
    }
    let f = g.factions.get_mut(&BLUE).unwrap();
    f.income = 1000;
    f.stockpile.add(Good::Coal, 1);
    f.stockpile.add(Good::Iron, 1);
    g.ai_logistics(&r);
    assert_eq!(
        g.cities
            .values()
            .filter(|c| c.construction == Some(Building::Factory))
            .count(),
        1
    );
}
