//! The economy through the host, on the bundled scenarios: pacing, determinism, and saves.
use fourx_content::Content;
use fourx_runtime::{Host, Start};
use fourx_sim::Command;

fn dawn_at_peace(seed: u64) -> Host {
    let mut content = Content::base();
    content
        .scenarios
        .get_mut("dawn-straits")
        .expect("the starter ships with the base pack")
        .wars
        .clear();
    Host::start(
        content,
        Start {
            scenario: Some("dawn-straits"),
            seed,
            ..Start::default()
        },
    )
    .expect("the starter starts")
}

fn end_turn(host: &mut Host) {
    let player = host.commander();
    host.command(player, Command::EndTurn).expect("turn ends");
}

#[test]
fn the_starter_is_won_by_industry_in_weeks() {
    // Cities produce shields from their land, not a flat rate, and the shields add up to the
    // industrial victory. The pack's threshold is tuned so the compact starter is decided in
    // weeks: not on the tenth day, and not after a year of peace.
    let mut host = dawn_at_peace(1);
    let mut days = 0;
    while host.game.winner.is_none() && days < 400 {
        end_turn(&mut host);
        days += 1;
    }
    assert!(host.game.winner.is_some(), "nobody won in {days} days");
    assert!((20..=150).contains(&days), "won on day {days}");
}

#[test]
fn every_city_gathers_something_and_cities_level_off() {
    let mut host = dawn_at_peace(2);
    let starting: Vec<_> = host.game.cities.values().map(|c| c.population).collect();
    for _ in 0..30 {
        end_turn(&mut host);
    }
    for city in host.game.cities.values() {
        assert!(city.harvest.food > 0, "{} gathers no food", city.name);
        assert!(city.harvest.shields > 0, "{} gathers no shields", city.name);
        assert!(city.harvest.gold > 0, "{} gathers no gold", city.name);
        assert!(city.population >= 1);
        assert!(city.granary >= 0);
    }
    // The land's surplus fed growth, which is bounded by what the land can feed.
    let grown: Vec<_> = host.game.cities.values().map(|c| c.population).collect();
    assert!(grown.iter().zip(&starting).any(|(now, then)| now > then));
    for city in host.game.cities.values() {
        // At most one mouth too many: a city that has just outgrown its land is about to starve.
        assert!(
            city.food_surplus() >= -fourx_sim::economy::FOOD_PER_CITIZEN,
            "{} outgrew its land",
            city.name
        );
    }
}

#[test]
fn the_world_economy_is_deterministic_and_survives_a_save() {
    let mut host = Host::base(7).expect("the world starts");
    let player = host.commander();
    let treasury = host.game.factions[&player].gold;
    for _ in 0..6 {
        end_turn(&mut host);
    }
    assert!(
        host.game.factions[&player].gold > treasury,
        "income is paid"
    );
    // A save keeps the granaries and the harvest, and play carries on exactly as before.
    let mut resumed = Host::load(&host.save().expect("saves")).expect("loads");
    assert_eq!(resumed.game, host.game);
    for _ in 0..6 {
        end_turn(&mut host);
        end_turn(&mut resumed);
    }
    assert_eq!(resumed.game, host.game);
    // And a second run from the same seed lands in the same place.
    let mut again = Host::base(7).expect("the world starts");
    for _ in 0..12 {
        end_turn(&mut again);
    }
    assert_eq!(again.game, host.game);
}

#[test]
fn clients_receive_the_economy_in_their_snapshot() {
    let mut host = dawn_at_peace(3);
    end_turn(&mut host);
    let player = host.commander();
    let fourx_sim::Response::Snapshot { game, .. } = host.snapshot(player) else {
        panic!("a snapshot");
    };
    let truth = &host.game;
    for city in truth.cities.values().filter(|c| c.owner == player) {
        let seen = &game.cities[&city.id];
        assert_eq!((seen.harvest, seen.granary), (city.harvest, city.granary));
        assert!(seen.harvest.food > 0);
    }
    assert_eq!(game.income(player), truth.income(player));
}
