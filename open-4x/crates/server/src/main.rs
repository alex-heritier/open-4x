//! Headless command line: list scenarios and nations, or simulate days headlessly.
use fourx_content::{Content, load_directory};
use fourx_runtime::{Host, Start};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let value = |flag: &str| args.windows(2).find(|a| a[0] == flag).map(|a| a[1].clone());
    if args.iter().any(|a| a == "--help") {
        println!(
            "fourx-server [--seed 42] [--pack DIRECTORY] [--scenario ID] [--nation ID] [--list] [--load SAVE] [--save SAVE] [--simulate DAYS]"
        );
        return Ok(());
    }
    let asset_dir = value("--pack").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/packs/base")
    });
    let content = load_directory(&asset_dir)?;
    let seed = value("--seed").unwrap_or("42".into()).parse()?;
    let scenario = value("--scenario");
    let nation = value("--nation");
    if args.iter().any(|a| a == "--list") {
        print_catalog(&content, scenario.as_deref())?;
        return Ok(());
    }
    let mut host = if let Some(path) = value("--load") {
        Host::load(&std::fs::read_to_string(path)?)?
    } else {
        Host::start(
            content,
            Start {
                scenario: scenario.as_deref(),
                nation: nation.as_deref(),
                seed,
            },
        )?
    };
    if let Some(days) = value("--simulate") {
        simulate(&mut host, days.parse::<u32>()?)?;
        println!("{}", serde_json::to_string_pretty(&host.game)?);
    }
    if let Some(path) = value("--save") {
        std::fs::write(path, host.save()?)?;
    } else if value("--simulate").is_none() {
        return Err("nothing to do: pass --simulate DAYS, --save SAVE, or --list".into());
    }
    Ok(())
}

/// Play `days` days as the commander, stopping early on victory.
fn simulate(host: &mut Host, days: u32) -> Result<(), fourx_runtime::HostError> {
    let commander = host.commander();
    for _ in 0..days {
        if host.game.winner.is_some() {
            break;
        }
        host.command(commander, fourx_sim::Command::EndTurn)?;
    }
    Ok(())
}

/// Print the pack's scenarios and, for one of them, the nations that can be commanded.
fn print_catalog(
    content: &Content,
    scenario: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "Scenarios in pack {:?} (default: {}):",
        content.pack.id, content.pack.default_scenario
    );
    for (id, s) in &content.scenarios {
        println!(
            "  {id:<16} {} | starts {} | {} nations | {}x{}",
            s.name,
            s.start_date,
            s.nations.len(),
            s.map.width,
            s.map.height
        );
    }
    let s = content.scenario(scenario)?;
    println!(
        "\nNations in {:?} (default commander: {}):",
        s.id, s.commander
    );
    for n in &s.nations {
        println!("  {:<28} {}", n.id, n.name);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulate_ten_days_and_list_the_catalog() {
        let mut host = Host::base_scenario("russo-japanese-war", 42).unwrap();
        simulate(&mut host, 10).unwrap();
        assert_eq!(host.game.turn, 11);
        let content = Content::base();
        print_catalog(&content, Some("russo-japanese-war")).unwrap();
    }
}
