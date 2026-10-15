//! Converts the map of a Civilization III scenario (`.biq`) into an open-4x scenario.
//!
//! ```text
//! cargo run --release -- /path/to/AoI5.0.biq [--out scenario.json]
//! ```
//!
//! The Civ3 map is a staggered rectangle (`x + y` even), which is exactly the upright
//! [`Lattice`] open-4x draws, so every tile keeps its place: native column `x` and row `y` of
//! the file are column `x` and row `y` of the lattice. What is carried over:
//!
//! * **Terrain.** Desert, plains, grassland, and tundra are base terrains; hills and mountains
//!   are relief; forest and jungle are cover; coast, sea, and ocean are the three water depths.
//!   Civ3 has no ground beneath hills, mountains, forest, or jungle, so the base under them is
//!   voted by the open ground around. Flood plain becomes grassland, and the impassable
//!   mountain and volcano become mountains, because open-4x has no such terrain.
//! * **Rivers.** Civ3 joins a tile to its four diagonal neighbours by a river; those are the
//!   tile edges open-4x draws rivers on. An edge counts when either tile has it. Edges that
//!   touch water or run across the wrapped seam of the world cannot be written and are dropped.
//! * **Resources.** Every resource of the file except [`EXCLUDED_RESOURCES`], with its class and
//!   its food, shield, and gold bonuses.
//! * **Improvements.** Roads, railroads, mines, and irrigation.
//! * **Nations and cities.** One nation for each player and every pre-placed city, because a
//!   scenario needs them to be playable. Each city gets a garrison and each capital the usual
//!   starting units; the file's own 1,358 units are not converted. Each nation's `profile` (the
//!   nation selection screen's text) is written by hand and kept from the scenario it replaces.
//! * **Territory.** The 1895 regions, which hand land beyond the cities' borders to the nation
//!   that held it, are painted by `scenario-forge aoi` and likewise kept from the scenario it
//!   replaces.
use civ3_biq::{Biq, MapView, sections::tile::overlay};
use fourx_sim::{
    Flavor, Status,
    calendar::Date,
    resources::{ResourceClass, ResourceKind},
    scenario::{CityStart, NationStart, RuleOverrides, Scenario, UnitStart},
    terrain::{Coord, Cover, Lattice, Map, Relief, Terrain, Tile},
};
use std::collections::{BTreeMap, HashMap};
use std::process::ExitCode;

#[path = "../../scenario-forge/src/output.rs"]
mod output;

/// Resources left out of the map: the scenario's own bookkeeping markers for the industrial
/// and colonial economies, which are not things that lie in the ground.
const EXCLUDED_RESOURCES: [&str; 2] = ["Industry", "Colonialism"];

/// A civilization's look in open-4x: the colour and city architecture it is drawn in. Keyed by
/// the name the file gives the civilization.
const NATIONS: [(&str, &str, Flavor); 31] = [
    ("British Empire", "#e0607e", Flavor::Western),
    ("France", "#2f6fd6", Flavor::Western),
    ("Russia", "#3c9d57", Flavor::Orthodox),
    ("German Empire", "#7b7f86", Flavor::Western),
    ("USA", "#2c4fa3", Flavor::Western),
    ("Japan", "#e24b6a", Flavor::EastAsian),
    ("Portugal", "#1f8f6b", Flavor::Latin),
    ("Italy", "#3fae7a", Flavor::Latin),
    ("Netherlands", "#f06b2d", Flavor::Western),
    ("China", "#d6b43b", Flavor::EastAsian),
    ("Balcan Nations", "#7c5aa6", Flavor::Orthodox),
    ("Ottoman Empire", "#9b2335", Flavor::Arab),
    ("Brazil", "#4caf50", Flavor::Latin),
    ("Central America", "#6fb34e", Flavor::Latin),
    ("Boer Republics", "#c88b3c", Flavor::Western),
    ("Argentina", "#6cb3e6", Flavor::Latin),
    ("Scandinavia", "#5fb0c9", Flavor::Western),
    ("La Plata", "#8b9c3a", Flavor::Latin),
    ("Austria-Hungary", "#e6c84a", Flavor::Western),
    ("Spain", "#e08a2e", Flavor::Latin),
    ("Persia", "#cc7a2c", Flavor::Arab),
    ("Uruguay", "#3d7fa8", Flavor::Latin),
    ("Siam", "#7a52b8", Flavor::SoutheastAsian),
    ("Chile", "#b83280", Flavor::Latin),
    ("New Granada", "#d86a2b", Flavor::Latin),
    ("Belgium", "#cfa83a", Flavor::Western),
    ("Tibet", "#b0603f", Flavor::EastAsian),
    ("Hispañiola", "#ba3b78", Flavor::Latin),
    ("Mexico", "#3d8c5a", Flavor::Latin),
    ("Abyssinia", "#5da34a", Flavor::African),
    ("Romania", "#d6a15e", Flavor::Orthodox),
];

/// What a Civ3 terrain becomes.
#[derive(Clone, Copy)]
enum Ground {
    Water(Terrain),
    Open(Terrain),
    /// Relief or cover with no base terrain of its own.
    Feature(Relief, Cover),
}

/// Civ3 terrain ids in the numbering of a Conquests file (`TERR` row order).
fn ground(id: u8) -> Ground {
    match id {
        0 => Ground::Open(Terrain::Desert),
        1 => Ground::Open(Terrain::Plains),
        2 | 4 => Ground::Open(Terrain::Grass),
        3 => Ground::Open(Terrain::Tundra),
        5 => Ground::Feature(Relief::Hills, Cover::Bare),
        6 | 9 | 10 => Ground::Feature(Relief::Mountains, Cover::Bare),
        7 => Ground::Feature(Relief::Flat, Cover::Forest),
        8 => Ground::Feature(Relief::Flat, Cover::Jungle),
        11 => Ground::Water(Terrain::Coast),
        12 => Ground::Water(Terrain::Sea),
        _ => Ground::Water(Terrain::Ocean),
    }
}

fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        let c = match c {
            'ñ' => 'n',
            c => c.to_ascii_lowercase(),
        };
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (mut input, mut out, mut art) = (None, None, false);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => out = args.next(),
            "--art" => art = true,
            _ => input = Some(arg),
        }
    }
    let Some(input) = input else {
        eprintln!("usage: biq-import FILE.biq [--out scenario.json]");
        return ExitCode::from(2);
    };
    let out = out.unwrap_or_else(|| {
        format!(
            "{}/../../assets/packs/base/scenarios/age-of-imperialism.json",
            env!("CARGO_MANIFEST_DIR")
        )
    });
    if art {
        return match print_art(&input) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("{input}: {message}");
                ExitCode::FAILURE
            }
        };
    }
    match run(&input, &out) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{input}: {message}");
            ExitCode::FAILURE
        }
    }
}

/// `--art`: print, as JSON on stdout, which Civ3 art each player's civilization names: its
/// Civilopedia key (the key of `PediaIcons.txt`, which names the flag picture) and its era
/// animations (the leader's flics). Used by `tools/aoi-art/import.py`.
fn print_art(input: &str) -> Result<(), String> {
    let biq = Biq::read_file(input).map_err(|e| e.to_string())?;
    let mut nations = serde_json::Map::new();
    for lead in &biq.scenario.players {
        let civ = biq
            .rules
            .civilizations
            .get(lead.civilization as usize)
            .ok_or("a player has no civilization")?;
        let flics: Vec<_> = civ.era_art.iter().map(|s| s.to_string()).collect();
        nations.insert(
            slug(&civ.civilization_name.to_string()),
            serde_json::json!({
                "race": civ.civilopedia_entry.to_string(),
                "leader": lead.leader_name.to_string(),
                "flics": flics,
            }),
        );
    }
    println!("{}", serde_json::to_string_pretty(&nations).map_err(|e| e.to_string())?);
    Ok(())
}

fn run(input: &str, out: &str) -> Result<(), String> {
    let biq = Biq::read_file(input).map_err(|e| e.to_string())?;
    let view = biq.map_view().ok_or("the file has no map")?;
    let mut map = build_map(&biq, &view)?;
    let lattice = map.lattice.expect("built upright");
    eprintln!(
        "map {} columns x {} rows ({} tiles), {} resource kinds",
        lattice.columns,
        lattice.rows,
        map.tile_count(),
        map.resources.len()
    );

    let (mut nations, cities, units) = build_nations(&biq, &mut map)?;
    let kept = previous(out);
    keep_profiles(&mut nations, kept.as_ref());
    keep_regions(&mut map, kept.as_ref());
    for city in &cities {
        // A city's own square counts as roaded and farmed; worker improvements add nothing there.
        map.get_mut(city.position).expect("on the map").improvements = 0;
    }
    let scenario = Scenario {
        format: fourx_sim::SCENARIO_FORMAT,
        id: "age-of-imperialism".into(),
        name: "Age of Imperialism, 1 January 1895".into(),
        description: "The Age of Imperialism world map: its terrain, rivers, resources, roads, and \
            railways exactly as the scenario paints them, on a 90 x 180 upright map that stands the \
            Civ3 grid on its corners. The thirty-one powers of 1895 hold their cities. Turns are days."
            .into(),
        start_date: Date::new(1895, 1, 1).ok_or("bad start date")?,
        commander: "japan".into(),
        intro: "1 January 1895. The great powers have carried their empires across the globe and \
            are dividing what is left. Japan is the default command; any nation can be played \
            with --nation."
            .into(),
        charted: true,
        regional_fill: kept.as_ref().is_some_and(|old| old.regional_fill),
        rules: RuleOverrides {
            victory_industry: Some(1_000_000),
            research_cost: Some(3000),
            starting_gold: None,
        },
        map,
        nations,
        regions: kept.as_ref().map_or_else(Vec::new, |old| old.regions.clone()),
        cities,
        units,
        wars: Vec::new(),
    };
    let json = serde_json::to_value(&scenario).map_err(|e| e.to_string())?;
    std::fs::write(out, output::render(&json)).map_err(|e| format!("{out}: {e}"))?;
    eprintln!(
        "wrote {out}: {} nations, {} cities, {} units",
        scenario.nations.len(),
        scenario.cities.len(),
        scenario.units.len()
    );
    Ok(())
}

fn build_map(biq: &Biq, view: &MapView) -> Result<Map, String> {
    if view.width % 2 != 0 {
        return Err("a Civ3 map has an even width".into());
    }
    let lattice = Lattice {
        columns: view.width / 2,
        rows: view.height,
    };
    lattice.validate()?;
    let mut map = Map::embedded(lattice, Terrain::Ocean);
    let cell = |cx: i32, cy: i32| lattice.cell(cx, cy);

    // Terrain: water and open ground are final; features wait for a base.
    let mut features: Vec<Coord> = Vec::new();
    for ((cx, cy), t) in view.iter() {
        let p = cell(cx, cy);
        let tile = map.get_mut(p).ok_or("tile outside the lattice")?;
        match ground(view.numbering.to_current(t.terrain_id())) {
            Ground::Water(water) => tile.terrain = water,
            Ground::Open(base) => tile.terrain = base,
            Ground::Feature(relief, cover) => {
                tile.terrain = Terrain::Plains;
                tile.relief = relief;
                tile.cover = cover;
                features.push(p);
            }
        }
    }
    assign_bases(&mut map, features);

    // Rivers: the four diagonal connections are the grid's east and south edges.
    let (mut written, mut dropped) = (0, 0);
    for ((cx, cy), t) in view.iter() {
        let p = cell(cx, cy);
        let connects = |tile: &civ3_biq::sections::tile::Tile, d: u32| tile.river_towards(d);
        for (bit, grid, direction, opposite) in
            [(Tile::RIVER_E, (1, 0), 3, 7), (Tile::RIVER_S, (0, 1), 5, 1)]
        {
            let (nx, ny) = (cx + if bit == Tile::RIVER_E { 1 } else { -1 }, cy + 1);
            let far = view.tile(nx, ny).filter(|_| nx >= 0 && nx < view.width);
            let wanted = connects(t, direction) || far.is_some_and(|f| connects(f, opposite));
            if !wanted {
                continue;
            }
            let q = p.offset(grid.0, grid.1);
            let both_land =
                map.get(p).is_some_and(Tile::is_land) && map.get(q).is_some_and(Tile::is_land);
            if both_land {
                map.get_mut(p).expect("checked").river |= bit;
                written += 1;
            } else {
                dropped += 1;
            }
        }
    }
    eprintln!("rivers: {written} edges written, {dropped} dropped (water or seam)");

    // Resources, improvements.
    let goods = &biq.rules.goods;
    let mut kinds: Vec<ResourceKind> = Vec::new();
    let mut slot: HashMap<usize, u8> = HashMap::new();
    for (i, good) in goods.iter().enumerate() {
        let name = good.name.to_string();
        if EXCLUDED_RESOURCES.contains(&name.as_str()) {
            continue;
        }
        kinds.push(ResourceKind {
            id: slug(&name),
            name,
            class: match good.kind {
                0 => ResourceClass::Bonus,
                1 => ResourceClass::Luxury,
                _ => ResourceClass::Strategic,
            },
            food: good.food_bonus,
            shields: good.shield_bonus,
            gold: good.commerce_bonus,
        });
        slot.insert(
            i,
            u8::try_from(kinds.len()).map_err(|_| "too many resources")?,
        );
    }
    map.resources = kinds;
    let mut placed = 0;
    for ((cx, cy), t) in view.iter() {
        let tile = map.get_mut(cell(cx, cy)).expect("inside");
        if let Some(&n) = t.resource_index().and_then(|r| slot.get(&(r as usize))) {
            tile.resource = n;
            placed += 1;
        }
        if tile.is_land() {
            let plane = t.overlay_plane();
            let mut bits = 0;
            if plane & (overlay::ROAD | overlay::RAILROAD) != 0 {
                bits |= Tile::ROAD;
            }
            if plane & overlay::RAILROAD != 0 {
                bits |= Tile::RAIL;
            }
            if plane & overlay::MINE != 0 {
                bits |= Tile::MINE;
            } else if plane & overlay::IRRIGATION != 0 {
                bits |= Tile::FARM;
            }
            tile.improvements = bits;
        }
    }
    eprintln!("resources: {placed} tiles");
    Ok(map)
}

/// Give every relief and cover tile the base terrain the open ground around it has. Rounds
/// spread outward from definite ground, so a tile in the middle of a mountain range takes the
/// foot of the range.
fn assign_bases(map: &mut Map, features: Vec<Coord>) {
    let lattice = map.lattice.expect("upright");
    let is_feature: std::collections::HashSet<Coord> = features.iter().copied().collect();
    let mut resolved: HashMap<Coord, Terrain> = HashMap::new();
    let mut pending = features;
    let mut radius = 2;
    while !pending.is_empty() {
        let mut next = Vec::new();
        let mut round = Vec::new();
        for &p in &pending {
            let tile = map.get(p).expect("feature");
            // Jungle only grows on grass; everything else votes.
            if tile.cover == Cover::Jungle {
                round.push((p, Terrain::Grass));
                continue;
            }
            let mut votes: BTreeMap<u8, i32> = BTreeMap::new();
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    let q = p.offset(dx, dy);
                    let Some(other) = map.get(q) else { continue };
                    let base = if is_feature.contains(&q) {
                        match resolved.get(&q) {
                            Some(&b) => b,
                            None => continue,
                        }
                    } else if other.is_land() {
                        other.terrain
                    } else {
                        continue;
                    };
                    if tile.cover == Cover::Forest && base == Terrain::Desert {
                        continue;
                    }
                    let rank = match base {
                        Terrain::Grass => 0,
                        Terrain::Plains => 1,
                        Terrain::Desert => 2,
                        _ => 3,
                    };
                    *votes.entry(rank).or_default() += radius + 1 - dx.abs().max(dy.abs());
                }
            }
            match votes
                .iter()
                .max_by_key(|&(rank, weight)| (*weight, -i32::from(*rank)))
            {
                Some((rank, _)) => round.push((
                    p,
                    [
                        Terrain::Grass,
                        Terrain::Plains,
                        Terrain::Desert,
                        Terrain::Tundra,
                    ][usize::from(*rank)],
                )),
                None => next.push(p),
            }
        }
        if round.is_empty() {
            if radius < 8 {
                radius += 2;
                pending = next;
                continue;
            }
            // Nothing definite anywhere near: the poles are tundra, the rest plains.
            for p in next.drain(..) {
                let (_, cy) = lattice.native(p);
                let polar = cy < 6 || cy >= lattice.rows - 6;
                round.push((
                    p,
                    if polar {
                        Terrain::Tundra
                    } else {
                        Terrain::Plains
                    },
                ));
            }
        }
        for (p, base) in round {
            resolved.insert(p, base);
            map.get_mut(p).expect("feature").terrain = base;
        }
        pending = next;
    }
}

/// The scenario a re-import replaces, if there is one to read.
fn previous(out: &str) -> Option<Scenario> {
    std::fs::read_to_string(out)
        .ok()
        .and_then(|json| serde_json::from_str::<Scenario>(&json).ok())
}

/// The nation selection screen's profiles are written by hand in the scenario, not taken from
/// the file: a re-import keeps those of the scenario it replaces.
fn keep_profiles(nations: &mut [NationStart], old: Option<&Scenario>) {
    let Some(old) = old else {
        return;
    };
    for nation in nations {
        if let Some(kept) = old.nations.iter().find(|n| n.id == nation.id) {
            nation.profile = kept.profile.clone();
        }
    }
}

/// The 1895 territory is painted by `scenario-forge aoi`, not taken from the file: a re-import
/// keeps the region of every tile of the scenario it replaces.
fn keep_regions(map: &mut Map, old: Option<&Scenario>) {
    let Some(old) = old.filter(|old| old.map.tiles.len() == map.tiles.len()) else {
        return;
    };
    for (tile, kept) in map.tiles.iter_mut().zip(&old.map.tiles) {
        tile.region = if tile.is_land() { kept.region } else { 0 };
    }
}

fn build_nations(
    biq: &Biq,
    map: &mut Map,
) -> Result<(Vec<NationStart>, Vec<CityStart>, Vec<UnitStart>), String> {
    let look: HashMap<&str, (&str, Flavor)> = NATIONS
        .iter()
        .map(|&(name, color, flavor)| (name, (color, flavor)))
        .collect();
    let mut nations = Vec::new();
    for (i, lead) in biq.scenario.players.iter().enumerate() {
        let civ = biq
            .rules
            .civilizations
            .get(lead.civilization as usize)
            .ok_or_else(|| format!("player {i} has no civilization"))?;
        let name = civ.civilization_name.to_string();
        let &(color, flavor) = look
            .get(name.as_str())
            .ok_or_else(|| format!("no colour chosen for {name}"))?;
        let leader = lead.leader_name.to_string();
        let leader = if leader.is_empty() {
            civ.leader_name.to_string()
        } else {
            leader
        };
        let government = biq
            .rules
            .governments
            .get(lead.government as usize)
            .map(|g| g.name.to_string().trim().to_string())
            .unwrap_or_default();
        nations.push(NationStart {
            id: slug(&name),
            name,
            adjective: civ.adjective.to_string(),
            color: fourx_sim::Rgb::parse(color).expect("table colours are valid"),
            flavor,
            status: Status::Sovereign,
            suzerain: None,
            government,
            leader,
            gold: Some(lead.starting_treasury.clamp(0, 1_000_000)),
            technology: 3,
            notes: String::new(),
            profile: Default::default(),
        });
    }

    // Cities, owned through a player or a civilization.
    let owner_of = |owner_type: i32, owner: i32| -> Option<usize> {
        match owner_type {
            3 => Some(owner as usize),
            2 => biq
                .scenario
                .players
                .iter()
                .position(|l| l.civilization == owner),
            _ => None,
        }
        .filter(|&i| i < nations.len())
    };
    let mut cities: Vec<CityStart> = Vec::new();
    let mut palace: Vec<bool> = Vec::new();
    let mut at: HashMap<Coord, usize> = HashMap::new();
    let lattice = map.lattice.expect("upright");
    for c in biq.scenario.cities.iter() {
        let Some(owner) = owner_of(c.owner_type, c.owner) else {
            continue;
        };
        let position = lattice.cell(c.map_x, c.map_y);
        let land = map.get(position).is_some_and(Tile::is_land);
        if !land || at.insert(position, cities.len()).is_some() {
            return Err(format!("city {} is not on free land", c.name));
        }
        let size = c.size.clamp(1, 100);
        palace.push(c.has_palace != 0);
        cities.push(CityStart {
            home: true,
            nation: nations[owner].id.clone(),
            position,
            name: c.name.to_string(),
            population: size,
            industry: 1 + size / 3,
            capital: false,
            border: None,
        });
    }
    // The palace marks the capital; a nation without one takes its largest city.
    for nation in &nations {
        let own = || (0..cities.len()).filter(|&i| cities[i].nation == nation.id);
        let capital = own()
            .find(|&i| palace[i])
            .or_else(|| own().max_by_key(|&i| (cities[i].population, std::cmp::Reverse(i))))
            .ok_or_else(|| format!("{} has no city", nation.name))?;
        cities[capital].capital = true;
    }

    // Starting units: a fortified infantry in every city, and a field army, workers, pioneers,
    // and, in a port, an ironclad in each capital.
    let mut units = Vec::new();
    let coastal = |p: Coord| {
        map.neighbors(p)
            .iter()
            .any(|&q| map.get(q).is_some_and(Tile::is_water))
    };
    for city in &cities {
        let unit = |kind: &str, level: u8, fortified: bool| UnitStart {
            nation: city.nation.clone(),
            position: city.position,
            kind: kind.into(),
            level: Some(level),
            fortified,
        };
        units.push(unit("infantry", 1, true));
        if city.capital {
            units.extend([
                unit("infantry", 2, true),
                unit("infantry", 1, true),
                unit("cavalry", 1, false),
                unit("artillery", 1, false),
                unit("worker", 1, false),
                unit("worker", 1, false),
                unit("pioneer", 1, false),
            ]);
            if coastal(city.position) {
                units.push(unit("ironclad", 1, false));
            }
        }
    }
    Ok((nations, cities, units))
}
