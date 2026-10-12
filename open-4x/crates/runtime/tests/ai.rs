//! The computer nations on the bundled starter: they expand, improve their land, hold their
//! cities, and never touch the commander's nation.
use fourx_content::Content;
use fourx_runtime::{Host, Start};
use fourx_sim::terrain::Tile;
use fourx_sim::{Command, Id};

/// The starter at peace, with the industrial victory out of reach so that the run goes on.
fn dawn_at_peace(seed: u64) -> Host {
    let mut content = Content::base();
    let scenario = content
        .scenarios
        .get_mut("russo-japanese-war")
        .expect("the starter ships with the base pack");
    scenario.wars.clear();
    scenario.rules.victory_industry = Some(1_000_000);
    Host::start(
        content,
        Start {
            scenario: Some("russo-japanese-war"),
            seed,
            ..Start::default()
        },
    )
    .expect("the starter starts")
}

fn days(host: &mut Host, count: usize) {
    let player = host.commander();
    for _ in 0..count {
        host.command(player, Command::EndTurn).expect("turn ends");
    }
}

fn rival(host: &Host) -> Id {
    let player = host.commander();
    *host
        .game
        .factions
        .keys()
        .find(|id| **id != player)
        .expect("the starter has a rival")
}

fn cities_of(host: &Host, owner: Id) -> Vec<Id> {
    host.game
        .cities
        .values()
        .filter(|c| c.owner == owner)
        .map(|c| c.id)
        .collect()
}

#[test]
fn a_computer_nation_expands_improves_its_land_and_holds_its_cities() {
    let mut host = dawn_at_peace(1);
    let rival = rival(&host);
    let at_start = cities_of(&host, rival).len();

    days(&mut host, 20);
    let settled = cities_of(&host, rival);
    days(&mut host, 10);

    assert!(
        cities_of(&host, rival).len() > at_start,
        "the rival never founded a city"
    );
    let improved = host
        .game
        .map
        .tiles
        .iter()
        .filter(|t| t.owner == rival && t.improvements & Tile::IMPROVEMENT_MASK != 0)
        .count();
    assert!(improved > 0, "the rival's workers improved nothing");
    // A city is never left without a defender, or without one on the way.
    for id in settled {
        let city = &host.game.cities[&id];
        let held = host
            .game
            .units_at(city.position)
            .iter()
            .any(|u| u.owner == rival && u.kind == "infantry");
        assert!(
            held || city.production.as_deref() == Some("infantry"),
            "{} has no defender and is not building one",
            city.name
        );
    }
}

#[test]
fn the_commanders_nation_is_left_to_the_player() {
    let mut host = dawn_at_peace(1);
    let player = host.commander();
    let units = |host: &Host| {
        host.game
            .units
            .values()
            .filter(|u| u.owner == player)
            .map(|u| (u.id, u.position, u.order))
            .collect::<Vec<_>>()
    };
    let before = units(&host);
    let producing = |host: &Host| {
        host.game
            .cities
            .values()
            .filter(|c| c.owner == player)
            .map(|c| c.production.clone())
            .collect::<Vec<_>>()
    };
    let building = producing(&host);

    days(&mut host, 15);

    assert_eq!(
        units(&host),
        before,
        "a computer nation moved the player's units"
    );
    assert_eq!(
        producing(&host),
        building,
        "the player's production was chosen for them"
    );
}

#[test]
fn the_computer_plays_the_same_game_from_the_same_seed() {
    let mut a = dawn_at_peace(5);
    let mut b = dawn_at_peace(5);
    days(&mut a, 30);
    days(&mut b, 30);
    assert_eq!(a.game, b.game);
    // And a save carries no AI state: the resumed game plays on exactly as the original.
    let mut resumed = Host::load(&a.save().expect("saves")).expect("loads");
    days(&mut a, 10);
    days(&mut resumed, 10);
    assert_eq!(a.game, resumed.game);
}
