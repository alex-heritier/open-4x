//! Reproducible desktop battle fixture or headless campaign benchmark.
//! cargo run -p fourx-runtime --example milestone_probe -- battle /tmp/battle.save.json
//! cargo run -p fourx-runtime --example milestone_probe -- campaign /tmp/japan.save.json
use fourx_content::Content;
use fourx_runtime::{Host, Start};
use fourx_sim::{Command, SCENARIO_FORMAT, Scenario};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let campaign = args.first().is_some_and(|a| a == "campaign");
    let mut host = if campaign {
        Host::base(42)?
    } else {
        let units:Vec<_>=(0..10).map(|_|serde_json::json!({"nation":"japan","position":{"x":5,"y":3},"kind":"infantry","level":2})).chain((0..3).map(|_|serde_json::json!({"nation":"russia","position":{"x":6,"y":3},"kind":"infantry","level":1}))).collect();
        let scenario: Scenario = serde_json::from_value(
            serde_json::json!({"format":SCENARIO_FORMAT,"id":"stack-study","name":"Stack Battle Study","start_date":"1895-01-01","commander":"japan","charted":true,"map":{"width":14,"height":8,"terrain":vec!["..............";8]},"nations":[{"id":"japan","name":"Japan","color":"#aa3333","flavor":"east_asian"},{"id":"russia","name":"Russia","color":"#3366aa","flavor":"orthodox"}],"cities":[{"nation":"japan","name":"Headquarters","position":{"x":2,"y":3},"population":3,"industry":4,"capital":true},{"nation":"russia","name":"Fortress","position":{"x":11,"y":3},"population":3,"industry":4,"capital":true}],"units":units}),
        )?;
        let mut content = Content::base();
        content.scenarios.insert(scenario.id.clone(), scenario);
        Host::start(
            content,
            Start {
                scenario: Some("stack-study"),
                nation: None,
                seed: 42,
            },
        )?
    };
    let player = host.commander();
    let start = std::time::Instant::now();
    if campaign {
        for _ in 0..30 {
            host.command(player, Command::EndTurn)?;
        }
        let f = &host.game.factions[&player];
        println!(
            "30 Japan campaign weeks: {:.2}s; {} units, {} cargo carts, {} wars; treasury {}, net weekly {:+}; goods {:?}",
            start.elapsed().as_secs_f64(),
            host.game.units.len(),
            host.game
                .units
                .values()
                .filter(|u| u.cargo.is_some())
                .count(),
            host.game.wars.len(),
            f.gold,
            f.income - f.upkeep,
            f.stockpile
        );
        for note in &host.game.log {
            if note.contains("war") || note.contains("demand") {
                println!("{note}");
            }
        }
    } else {
        host.command(player, Command::DeclareWar { nation: 2 })?;
        let unit = host
            .game
            .units
            .values()
            .find(|u| u.owner == player)
            .unwrap()
            .id;
        host.command(
            player,
            Command::AttackStack {
                unit,
                target: fourx_sim::terrain::Coord::new(6, 3),
            },
        )?;
        let b = &host.game.stack_battles[0];
        println!(
            "Battle resolved on turn {} / {}: {} lanes, {} combat rounds, losses {}:{}, advanced {}",
            host.game.turn,
            host.game.date(),
            b.width,
            b.rounds,
            b.attacker_losses,
            b.defender_losses,
            b.advanced
        );
    }
    let path = args
        .get(1)
        .map(String::as_str)
        .unwrap_or("/tmp/milestone.save.json");
    std::fs::write(path, host.save()?)?;
    println!("Saved {path}");
    Ok(())
}
