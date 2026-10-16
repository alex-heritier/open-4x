//! The approved economic milestone through the authoritative host and bundled campaign.
use fourx_runtime::Host;
use fourx_sim::{Building, Command, Good, Response};
#[test]
fn japan_has_real_homeland_and_colonies_in_the_imported_world() {
    let h = Host::base(42).unwrap();
    let japan = h.commander();
    let city = |name: &str| {
        h.game
            .cities
            .values()
            .find(|c| c.owner == japan && c.name == name)
            .unwrap()
            .id
    };
    for name in ["Tokyo", "Osaka", "Akita", "Sapporo", "Okinawa"] {
        assert!(h.game.home_city(city(name)), "{name}");
    }
    for name in ["Seoul", "Hamhung", "Pusan", "T'aipei"] {
        assert!(!h.game.home_city(city(name)), "{name}");
    }
}
#[test]
fn failing_to_switch_production_does_not_refund_or_change_the_live_project() {
    let mut h = Host::base(42).unwrap();
    let player = h.commander();
    let city = h
        .game
        .cities
        .values()
        .find(|c| c.owner == player && c.capital)
        .unwrap()
        .id;
    h.command(
        player,
        Command::Produce {
            city,
            unit: "infantry".into(),
        },
    )
    .unwrap();
    h.game
        .factions
        .get_mut(&player)
        .unwrap()
        .stockpile
        .0
        .insert(Good::Steel, 0);
    let before = h.game.clone();
    assert!(
        h.command(
            player,
            Command::Produce {
                city,
                unit: "artillery".into()
            }
        )
        .is_err()
    );
    assert_eq!(h.game, before);
}
#[test]
fn economy_and_diplomacy_survive_save_and_continue_identically() {
    let mut h = Host::base_scenario("russo-japanese-war", 42).unwrap();
    let player = h.commander();
    let enemy = *h.game.factions.keys().find(|id| **id != player).unwrap();
    let city = h
        .game
        .cities
        .values()
        .find(|c| c.owner == player && c.capital)
        .unwrap()
        .id;
    h.command(player, Command::Borrow).unwrap();
    h.command(
        player,
        Command::Trade {
            good: Good::Cotton,
            amount: 3,
        },
    )
    .unwrap();
    h.command(
        player,
        Command::Build {
            city,
            building: Building::Factory,
        },
    )
    .unwrap();
    h.command(
        player,
        Command::Pay {
            nation: enemy,
            amount: 10,
        },
    )
    .unwrap();
    h.command(player, Command::EndTurn).unwrap();
    let mut loaded = Host::load(&h.save().unwrap()).unwrap();
    assert_eq!(h.game, loaded.game);
    for _ in 0..8 {
        h.command(player, Command::EndTurn).unwrap();
        loaded.command(player, Command::EndTurn).unwrap();
        assert_eq!(h.game, loaded.game);
    }
    let Response::Snapshot { game, .. } = h.snapshot(player) else {
        panic!("snapshot")
    };
    assert_eq!(game.factions[&enemy].stockpile.get(Good::Steel), 0);
    assert_eq!(game.factions[&enemy].debt, 0);
}
#[test]
fn a_full_year_has_economic_activity_and_cannot_be_won_by_generic_industry_points() {
    let mut h = Host::base_scenario("russo-japanese-war", 42).unwrap();
    let player = h.commander();
    h.rules.economy.war_chance_per_thousand = 0;
    h.game.wars.clear();
    h.game.diplomacy.wars.clear();
    let initial_upkeep = h.game.budget_upkeep(player, &h.rules);
    h.game.factions.get_mut(&player).unwrap().industry = h.rules.victory_industry;
    for _ in 0..52 {
        h.command(player, Command::EndTurn).unwrap();
    }
    assert!(h.game.winner.is_none());
    assert_eq!(h.game.date().iso(), "1905-02-06");
    assert!(h.game.factions[&player].income > 0);
    assert!(initial_upkeep > 0);
    assert!(
        h.game.factions[&player].upkeep > 0
            || h.game.log.iter().any(|entry| entry.contains("bankruptcy"))
    );
}
