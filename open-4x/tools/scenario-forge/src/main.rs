//! Builds `world-1876.json`, the default scenario: the world from 84°N to 58°S on a 512×342
//! Mercator plane, laid out upright as 175,104 tiles, as it stood on 1 January 1876.
//!
//! ```text
//! scenario-forge build [--out FILE] [--preview FILE] [--terrain FILE] [--crop X,Y,W,H,SCALE,FILE]...
//! ```
//!
//! History (who held what, the cities, the units, the wars) is authored in `data/`. Coastlines,
//! rivers, elevation, and climate come from public sources (Natural Earth, ETOPO5, Köppen–Geiger),
//! fetched once into `cache/` and verified against a pinned SHA-256.
mod classify;
mod data;
mod forge;
mod geo;
mod output;
mod preview;
mod source;
mod world;

use fourx_content::{Pack, scenario::validate_scenario};
use fourx_sim::Game;
use std::path::PathBuf;
use std::time::Instant;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("build") => build(&args[1..]),
        Some("fetch") => fetch(),
        _ => Err("usage: scenario-forge build [--out FILE] [--preview FILE] [--terrain FILE] [--crop X,Y,W,H,SCALE,FILE]...\n       scenario-forge fetch".to_string()),
    };
    if let Err(error) = result {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn fetch() -> Result<(), String> {
    let cache = root().join("cache");
    source::fetch(&cache, &source::COUNTRIES)?;
    source::fetch(&cache, &source::LAKES)?;
    source::fetch(&cache, &source::RIVERS)?;
    source::fetch(&cache, &source::ETOPO5)?;
    source::fetch_koeppen(&cache)?;
    println!("source data verified in {}", cache.display());
    Ok(())
}

fn build(args: &[String]) -> Result<(), String> {
    let mut out = root().join("../../assets/packs/base/scenarios/world-1876.json");
    let mut preview_path = root().join("cache/world-1876.png");
    let mut terrain_path: Option<PathBuf> = None;
    let mut crops: Vec<String> = Vec::new();
    let mut partial = false;
    let mut iter = args.iter();
    while let Some(flag) = iter.next() {
        let mut value = || {
            iter.next()
                .cloned()
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        match flag.as_str() {
            "--out" => out = PathBuf::from(value()?),
            "--preview" => preview_path = PathBuf::from(value()?),
            "--terrain" => terrain_path = Some(PathBuf::from(value()?)),
            "--crop" => crops.push(value()?),
            "--partial" => {
                partial = true;
                // Work-in-progress builds never overwrite the shipped scenario.
                out = root().join("cache/partial.json");
            }
            other => return Err(format!("unknown option {other}")),
        }
    }

    let started = Instant::now();
    let cache = root().join("cache");
    let countries = source::countries(&source::fetch(&cache, &source::COUNTRIES)?)?;
    let lakes = source::lakes(&source::fetch(&cache, &source::LAKES)?)?;
    let elevation = source::Elevation::parse(&source::fetch(&cache, &source::ETOPO5)?)?;
    let climate = source::Climate::parse(&source::fetch_koeppen(&cache)?)?;
    let rivers = source::rivers(&source::fetch(&cache, &source::RIVERS)?)?;
    let mut sources = data::Sources::load(&root().join("data"))?;
    if partial {
        sources.restrict_to_nations_with_cities();
    }
    let inputs = classify::Inputs {
        elevation: &elevation,
        climate: &climate,
        rivers: &rivers,
    };
    let built = forge::build(&sources, &countries, &lakes, &inputs)?;
    for line in &built.report {
        eprintln!("{line}");
    }
    let scenario = built.scenario;

    // The output must satisfy the same validation the game applies when it loads a scenario.
    let pack = Pack::base();
    let rules = pack.rules.with_overrides(&scenario.rules);
    validate_scenario(&scenario, &rules).map_err(|error| error.to_string())?;
    let game = Game::from_scenario(1, &rules, &scenario, None)
        .map_err(|error| format!("scenario does not start: {error}"))?;

    let value = serde_json::to_value(&scenario).map_err(|error| error.to_string())?;
    let text = output::render(&value);
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(&out, &text).map_err(|error| format!("{}: {error}", out.display()))?;

    let image = preview::map_image(&scenario, &game, 4, true);
    if let Some(path) = &terrain_path {
        preview::map_image(&scenario, &game, 4, false).save(path)?;
    }
    image.save(&preview_path)?;
    for spec in &crops {
        let parts: Vec<&str> = spec.splitn(6, ',').collect();
        let [x, y, w, h, scale, file] = parts[..] else {
            return Err(format!("--crop wants X,Y,W,H,SCALE,FILE, got {spec}"));
        };
        let number = |text: &str| {
            text.parse::<usize>()
                .map_err(|_| format!("bad number {text} in --crop"))
        };
        let (x, y, w, h, scale) = (
            number(x)?,
            number(y)?,
            number(w)?,
            number(h)?,
            number(scale)?,
        );
        // The render is 4 px per tile; scale it down or up to the requested size per tile.
        let base = preview::map_image(&scenario, &game, scale.max(1), terrain_path.is_none());
        base.crop_scaled(x * scale, y * scale, w * scale, h * scale, 1)
            .save(&PathBuf::from(file))?;
    }

    let land = scenario.map.tiles.iter().filter(|t| t.is_land()).count();
    println!(
        "{}: {} nations, {} regions, {} cities, {} units, {} wars, {land} land tiles; {} bytes in {:.1}s",
        scenario.id,
        scenario.nations.len(),
        scenario.regions.len(),
        scenario.cities.len(),
        scenario.units.len(),
        scenario.wars.len(),
        text.len(),
        started.elapsed().as_secs_f32()
    );
    println!("wrote {} and {}", out.display(), preview_path.display());
    Ok(())
}
