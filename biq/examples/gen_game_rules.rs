//! Generate `src/rules_data.rs`, the research and diplomacy rules the game
//! hard-codes (the same approach as the combat factors in `src/combat.rs`:
//! the table lives in the source, a corpus-skipping test checks it against
//! the file).
//!
//! ```text
//! cargo run --release --manifest-path biq/Cargo.toml --example gen_game_rules \
//!     -- civ3/civ3-gog/app/Conquests/conquests.biq > src/rules_data.rs
//! ```
use civ3_biq::Biq;
use civ3_biq::sections::bldg::other_characteristics as oc;
use civ3_biq::sections::prto::ability as ab;
use std::fmt::Write as _;
use std::process::ExitCode;

/// Improvements whose effects the game implements, by `BLDG` name. Every
/// great wonder is playable besides `BLDG_NOT_PLAYABLE` (they all pay
/// culture and the game's border and score rules use it).
const BLDG_PLAYABLE: &[&str] = &[
    "Barracks",
    "Granary",
    "Temple",
    "Marketplace",
    "Library",
    "Courthouse",
    "Walls",
    "Aqueduct",
    "Bank",
    "Cathedral",
    "University",
    "Colosseum",
    "Factory",
    "Manufacturing Plant",
    "Hydro Plant",
    "Solar Plant",
    "Hospital",
    "Research Lab",
    "Harbor",
    "Stock Exchange",
    "Civil Defense",
    "Wealth",
    "Forbidden Palace",
];
/// Great wonders with effects the game cannot honor yet.
const BLDG_NOT_PLAYABLE: &[&str] = &["The Manhattan Project", "The United Nations"];

fn is_great_wonder(other: u32) -> bool {
    other & oc::WONDER != 0
}

/// A team color the clone keeps Civ3's hand-picked tone for, so the default
/// four civilizations look unchanged after the roster import. The rest are
/// derived from the `ntpNN.pcx` ramp (most saturated entry).
fn civ_color_override(name: &str) -> Option<(u8, u8, u8)> {
    Some(match name {
        "Japan" => (25, 148, 24),
        "Rome" => (190, 48, 48),
        "Egypt" => (220, 184, 48),
        "China" => (40, 180, 190),
        _ => return None,
    })
}

/// The most saturated entry of the 64-color team ramp of `ntpNN.pcx`, the
/// color city badges, borders and the unit tint share. The VGA palette is the
/// last 768 bytes of a 256-color PCX.
fn team_rgb(n: i32) -> (u8, u8, u8) {
    let path = format!("civ3/civ3-gog/app/Art/Units/Palettes/ntp{n:02}.pcx");
    let Ok(bytes) = std::fs::read(&path) else { return (128, 128, 128) };
    let pal = &bytes[bytes.len() - 768..];
    let mut best = (0i32, (128u8, 128u8, 128u8));
    for i in 0..64 {
        let (r, g, b) = (pal[i * 3], pal[i * 3 + 1], pal[i * 3 + 2]);
        let hi = r.max(g).max(b) as i32;
        let lo = r.min(g).min(b) as i32;
        let score = (hi - lo) * 2 + hi;
        if score > best.0 {
            best = (score, (r, g, b));
        }
    }
    best.1
}

fn q(s: &str) -> String {
    format!("{s:?}")
}

/// `Three-Man Chariot` -> `ThreeManChariot`.
fn ident(name: &str) -> String {
    let mut out = String::new();
    let mut up = true;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            if up {
                out.extend(c.to_uppercase());
            } else {
                out.push(c);
            }
            up = false;
        } else if c != '\'' {
            up = true;
        }
    }
    out
}

/// The `Art/Units` folder of a unit, when the install has one (Conquests
/// folders win over the Play the World and base game folders).
fn art_dir(name: &str) -> Option<String> {
    let want = match name {
        "Warrior" => "warrior",
        "Chariot" => "chariot",
        "Three-Man Chariot" => "Three Man Chariot",
        "Chasqui Scout" => "Chasquis Scout",
        "Modern Paratrooper" => "Paratrooper",
        "Mech Infantry" => "Mech Infantry",
        "Javelin Thrower" => "Javelin Thrower",
        "Hwach'a" => "Hwacha",
        // The only unit without its own art folder anywhere in the install;
        // its brother mercenaries use the Hoplite's art.
        "Numidian Mercenary" => "Hoplite",
        // The Ancient Age art; the later eras' art would follow the era.
        "Leader" => "Leader Ancient Times",
        "Army" => "Army Ancient Times",
        other => other,
    };
    for root in [
        "civ3/civ3-gog/app/Conquests/Art/Units",
        "civ3/civ3-gog/app/civ3PTW/Art/Units",
        "civ3/civ3-gog/app/Art/Units",
    ] {
        if std::path::Path::new(root).join(want).is_dir() {
            return Some(want.to_string());
        }
    }
    None
}

fn main() -> ExitCode {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: gen_game_rules conquests.biq > src/rules_data.rs");
        return ExitCode::from(2);
    };
    let biq = match Biq::read_file(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::from(1);
        }
    };
    let r = &biq.rules;
    let mut o = String::new();
    let w = &mut o;
    macro_rules! p {
        ($($a:tt)*) => { writeln!(w, $($a)*).unwrap() };
    }

    p!("//! Research and diplomacy rules of `conquests.biq`, hard-coded.");
    p!("//!");
    p!("//! @generated by `biq/examples/gen_game_rules.rs`; do not edit by hand.");
    p!("//! `tests::techs_match_the_biq` (here) re-reads the file and compares every");
    p!("//! advance when the game install is present; the other tables are checked by");
    p!("//! re-running the generator and diffing.");
    p!("#![allow(clippy::type_complexity)]");
    p!("");
    p!("use bevy::prelude::Color;");
    p!("use crate::cities::Production;");
    p!("use crate::roster::{{BldgDef, UnitRow}};");
    p!("use crate::units::UnitType;");
    p!("use civ3mapgen::research::{{Rules, TechRow}};");
    p!("use civ3mapgen::research_ai::{{BldgRow, Tables, UnitRow as AiUnitRow}};");
    p!("");

    // --- advances -----------------------------------------------------------
    p!("/// Name of every advance, in `TECH` order.");
    p!("pub const TECH_NAMES: [&str; {}] = [", r.techs.len());
    for t in &r.techs {
        p!("    {},", q(&t.name.text()));
    }
    p!("];");
    p!("");
    p!("/// `(cost, era, prerequisites, flags, flavors)` of every advance.");
    p!("const TECHS: [(i32, i32, [i32; 4], u32, u32); {}] = [", r.techs.len());
    for t in &r.techs {
        p!(
            "    ({}, {}, {:?}, {:#x}, {:#x}),",
            t.cost,
            t.era,
            t.prerequisites,
            t.flags,
            t.flavors
        );
    }
    p!("];");
    p!("");
    p!("/// `(era, icon, x, y)` of every advance: the era page of the Science");
    p!("/// Advisor it sits on, its icon's index in the tech icon sheets, and its");
    p!("/// box's top-left on that page (`TECH` +0x44, +0x48, +0x4C, +0x50).");
    p!("pub const TECH_TREE: [(i32, i32, i32, i32); {}] = [", r.techs.len());
    for t in &r.techs {
        p!("    ({}, {}, {}, {}),", t.era, t.icon, t.tree_x, t.tree_y);
    }
    p!("];");
    p!("");
    let g = &r.general_rules[0];
    p!("/// `RULE` future technology cost.");
    p!("pub const FUTURE_TECH_COST: i32 = {};", g.future_tech_cost);
    p!("/// `RULE` maximum research time.");
    p!("pub const MAX_RESEARCH_TIME: i32 = {};", g.max_research_time);
    p!("/// `RULE` minimum research time.");
    p!("pub const MIN_RESEARCH_TIME: i32 = {};", g.min_research_time);
    p!("/// `RULE` upgrade cost: gold per shield of price difference (`unit-upgrades.md` 6).");
    p!("pub const UPGRADE_COST: i32 = {};", g.upgrade_cost);
    p!("");
    p!("/// `ERAS` names.");
    p!("pub const ERA_NAMES: [&str; {}] = [", r.eras.len());
    for e in &r.eras {
        p!("    {},", q(&e.name.text()));
    }
    p!("];");
    p!("");
    p!("/// `DIFF.cost_factor` per difficulty level.");
    p!("pub const DIFFICULTY_COST_FACTOR: [i32; {}] = {:?};", r.difficulties.len(), r.difficulties.iter().map(|d| d.cost_factor).collect::<Vec<_>>());
    p!("/// `WSIZ.tech_rate` per world size.");
    p!("pub const WORLD_TECH_RATE: [i32; {}] = {:?};", r.world_sizes.len(), r.world_sizes.iter().map(|d| d.tech_rate).collect::<Vec<_>>());
    p!("");

    p!("/// `TFRM` base labor needed, before terrain movement cost.");
    p!("pub const WORK_NEEDED: [i32; {}] = {:?};", r.worker_jobs.len(), r.worker_jobs.iter().map(|j| j.turns_to_complete).collect::<Vec<_>>());
    p!("/// Advances that double the worker rate (`TECH` flag `0x40000`).");
    let doubles_work: u128 = r.techs.iter().enumerate().filter(|(_, t)| t.flags & 0x40000 != 0).fold(0, |bits, (i, _)| bits | (1u128 << i));
    p!("pub const DOUBLES_WORK: u128 = {doubles_work:#x};");
    let bridges: u128 = r.techs.iter().enumerate().filter(|(_, t)| t.flags & 4 != 0).fold(0, |bits, (i, _)| bits | (1u128 << i));
    p!("/// Advances enabling road/rail bridges (`TECH` flag `4`).");
    p!("pub const BRIDGES: u128 = {bridges:#x};");
    p!("");

    p!("/// Normal TERR values used by tile yields, worker jobs and land movement.");
    p!("pub struct TerrainFacts {{ pub name: &'static str, pub disease: civ3mapgen::disease::Terrain, pub food: u8, pub shields: u8, pub commerce: u8, pub irrigation: u8, pub mining: u8, pub road: u8, pub movement: u8, pub worker_job: i32, pub impassable: bool, pub impassable_wheeled: bool }}");
    p!("pub const TERRAINS: [TerrainFacts; {}] = [", r.terrains.len());
    for t in &r.terrains {
        p!("    TerrainFacts {{ name: {:?}, disease: civ3mapgen::disease::Terrain {{ causes: {}, cured: {}, strength: {} }}, food: {}, shields: {}, commerce: {}, irrigation: {}, mining: {}, road: {}, movement: {}, worker_job: {}, impassable: {}, impassable_wheeled: {} }}, // {}", t.name.text(), t.causes_disease(), t.cured_by_sanitation(), t.disease_strength, t.food, t.shields, t.commerce, t.irrigation_bonus, t.mining_bonus, t.road_bonus, t.movement_cost, t.worker_job, t.flags.impassable != 0, t.flags.impassable_wheeled != 0, t.name.text());
    }
    p!("];");
    p!("pub const FOREST_SHIELDS: u16 = {};", g.forest_value_in_shields);
    p!("");

    // --- valuation tables ---------------------------------------------------
    p!("const TFRM: [i32; {}] = {:?};", r.worker_jobs.len(), r.worker_jobs.iter().map(|j| j.required_tech).collect::<Vec<_>>());
    p!("/// `GOOD.prerequisite` per `GOOD` row (strategic resources need it to be usable).\npub const GOOD: [i32; {}] = {:?};", r.goods.len(), r.goods.iter().map(|j| j.prerequisite).collect::<Vec<_>>());
    p!("/// `GOOD` row names, matched by name to the clone's placed resources.");
    p!("pub const GOOD_NAMES: [&str; {}] = {:?};", r.goods.len(), r.goods.iter().map(|g| g.name.text().to_string()).collect::<Vec<_>>());
    p!("const GOVT: [i32; {}] = {:?};", r.governments.len(), r.governments.iter().map(|j| j.prerequisite_tech).collect::<Vec<_>>());
    // --- resistance and assimilation (`city-turn.md` 8) ---------------------
    p!("/// `CULT` rows: `(culture_ratio_percent, resistance_initial_percent, resistance_continued_percent)`.");
    p!("pub const CULT: [(i32, i32, i32); {}] = {:?};", r.cultures.len(), r.cultures.iter().map(|c| (c.culture_ratio_percent, c.resistance_initial_percent, c.resistance_continued_percent)).collect::<Vec<_>>());
    p!("/// `GOVT.assimilation_chance` (memory `+0x1A4`) per `GOVT` row.");
    p!("pub const GOVT_ASSIMILATION: [i32; {}] = {:?};", r.governments.len(), r.governments.iter().map(|g| g.assimilation_chance).collect::<Vec<_>>());
    p!("/// `GOVT[owner].vs[other].resistance_modifier` (the `+8` dword of each 12-byte record behind `+0x19C`).");
    p!("pub const GOVT_RESISTANCE: [[i32; {n}]; {n}] = {:?};", r.governments.iter().map(|g| g.vs.iter().map(|v| v.resistance_modifier).collect::<Vec<_>>()).collect::<Vec<_>>(), n = r.governments.len());
    p!("/// `DIFF.corruption_percent` (memory `+0x74`) per difficulty (`0x4B19BC`).");
    p!("pub const DIFF_CORRUPTION: [i32; {}] = {:?};", r.difficulties.len(), r.difficulties.iter().map(|d| d.corruption_percent).collect::<Vec<_>>());
    p!("/// `DIFF.citizens_quelled_by_military` (memory `+0x78`) per difficulty.");
    p!("pub const DIFF_QUELLED: [i32; {}] = {:?};", r.difficulties.len(), r.difficulties.iter().map(|d| d.citizens_quelled_by_military).collect::<Vec<_>>());
    p!("const CTZN: [i32; {}] = {:?};", r.citizens.len(), r.citizens.iter().map(|j| j.prerequisite).collect::<Vec<_>>());
    p!("/// `(required_tech, available_to_civs, ai_strategies, needs_resource)`.");
    p!("const PRTO: [(i32, u32, u32, bool); {}] = [", r.unit_types.len());
    for u in &r.unit_types {
        let res = u.required_resource_1 != -1 || u.required_resource_2 != -1 || u.required_resource_3 != -1;
        p!("    ({}, {:#x}, {:#x}, {}),", u.required_tech, u.available_to_civs as u32, u.ai_strategies, res);
    }
    p!("];");
    p!("/// `(required_advance, spaceship_part, improvement_flags, other_characteristics, flavors)`.");
    p!("const BLDG: [(i32, bool, u32, u32, u32); {}] = [", r.buildings.len());
    for b in &r.buildings {
        p!(
            "    ({}, {}, {:#x}, {:#x}, {:#x}),",
            b.required_advance,
            b.spaceship_part != -1,
            b.improvement_flags as u32,
            b.other_characteristics as u32,
            b.flavors as u32
        );
    }
    p!("];");
    let flav: Vec<Vec<i32>> = biq
        .flavors
        .as_ref()
        .map(|f| f.flavors.iter().map(|x| x.relations.clone()).collect())
        .unwrap_or_default();
    p!("const FLAVORS: [&[i32]; {}] = [", flav.len());
    for row in &flav {
        p!("    &{row:?},");
    }
    p!("];");
    p!("");

    // --- civilizations ------------------------------------------------------
    // The playable roster is every `RACE` row but the barbarians (row 0, the
    // one with `civilization_index == 0`). Roster index `i` is `RACE` row
    // `i + 1`, so a roster entry's `race` field is its row.
    let roster: Vec<&_> = r
        .civilizations
        .iter()
        .filter(|c| c.civilization_index != 0)
        .collect();
    let nr = roster.len();
    p!("/// The fixed data of one civilization (a `RACE` row), the fields the");
    p!("/// clone reads: the names the interface shows, the team color (`ntpNN.pcx`");
    p!("/// for the unit tint, `color` for badges) and the city name list. The");
    p!("/// ruler lives in `LEADER_ROSTER`.");
    p!("#[derive(Clone, Copy, Debug)]");
    p!("pub struct CivDefinition {{");
    p!("    pub name: &'static str,");
    p!("    pub adjective: &'static str,");
    p!("    pub noun: &'static str,");
    p!("    pub color: Color,");
    p!("    /// The team-color ramp of the art (`RACE.default_color`).");
    p!("    pub team_color: u8,");
    p!("    pub city_names: &'static [&'static str],");
    p!("}}");
    p!("");
    p!("/// One civilization's ruler, as the diplomacy screens name them.");
    p!("#[derive(Clone, Copy, Debug)]");
    p!("pub struct Leader {{");
    p!("    pub name: &'static str,");
    p!("    pub title: &'static str,");
    p!("    pub text_set: usize,");
    p!("}}");
    p!("");
    p!("/// The `RACE` facts the research and diplomacy rules read for one civ.");
    p!("#[derive(Clone, Copy, Debug)]");
    p!("pub struct RaceFacts {{");
    p!("    /// `RACE` row.");
    p!("    pub race: u32,");
    p!("    /// `flavors` mask (mem `+0x960`).");
    p!("    pub flavors: u32,");
    p!("    /// `build_often` mask (mem `+0x954`).");
    p!("    pub build_often: u32,");
    p!("    /// `traits` mask (mem `+0x948`).");
    p!("    pub traits: u32,");
    p!("    /// `aggression` (the AI's attitude base, clamped to -2..=2).");
    p!("    pub aggression: i32,");
    p!("    /// `culture_group` (mem `+0x90C`).");
    p!("    pub culture_group: i32,");
    p!("    /// `shunned_government` (mem `+0x920`), a `GOVT` row or -1.");
    p!("    pub shunned_government: i32,");
    p!("    /// `favorite_government` (mem `+0x924`), a `GOVT` row or -1.");
    p!("    pub favorite_government: i32,");
    p!("    /// `scientific_leader_count`... free advances, `-1` for none.");
    p!("    pub free_techs: [i32; 4],");
    p!("}}");
    p!("");
    p!("/// Every playable civilization, in `RACE` row order (the barbarians, row 0,");
    p!("/// are not one).");
    p!("pub static CIV_ROSTER: [CivDefinition; {nr}] = [");
    for (i, c) in roster.iter().enumerate() {
        let name = c.civilization_name.text();
        let (r, g, b) = civ_color_override(&name).unwrap_or_else(|| team_rgb(c.default_color));
        let cities: Vec<String> = c
            .city_names
            .iter()
            .map(|s| s.text())
            .filter(|s| !s.is_empty())
            .map(|s| q(&s))
            .collect();
        p!(
            "    CivDefinition {{ name: {}, adjective: {}, noun: {}, color: Color::srgb_u8({r}, {g}, {b}), team_color: {}, city_names: &[{}] }}, // row {}",
            q(&name),
            q(&c.adjective.text()),
            q(&c.noun.text()),
            c.default_color,
            cities.join(", "),
            i + 1,
        );
    }
    p!("];");
    p!("");
    p!("/// Every playable civilization's ruler, in the same order as `CIV_ROSTER`.");
    p!("pub static LEADER_ROSTER: [Leader; {nr}] = [");
    for (i, c) in roster.iter().enumerate() {
        p!(
            "    Leader {{ name: {}, title: {}, text_set: {} }}, // {}",
            q(&c.leader_name.text()),
            q(&c.title.text()),
            i,
            c.civilization_name.text(),
        );
    }
    p!("];");
    p!("");
    p!("/// The `RACE` facts of every playable civilization, in `CIV_ROSTER` order.");
    p!("pub static RACE_ROSTER: [RaceFacts; {nr}] = [");
    for (i, c) in roster.iter().enumerate() {
        let ext = c.conquests.as_ref();
        p!(
            "    RaceFacts {{ race: {}, flavors: {:#x}, build_often: {:#x}, traits: {:#x}, aggression: {}, culture_group: {}, shunned_government: {}, favorite_government: {}, free_techs: {:?} }}, // {}",
            i + 1,
            ext.map_or(0, |e| e.flavors),
            c.build_often,
            c.traits,
            c.aggression,
            c.culture_group,
            c.shunned_government,
            c.favorite_government,
            c.free_techs,
            c.civilization_name.text(),
        );
    }
    p!("];");
    p!("");

    // --- the unit and building rosters -------------------------------------
    let our_mask: u32 = roster.iter().enumerate().fold(0u32, |a, (i, _)| a | (1 << (i + 1)));
    let nu = r.unit_types.len();
    let nb = r.buildings.len();
    p!("/// Number of `PRTO` rows: `UnitType(i)` is row `i`, `Production(i)` the same unit.");
    p!("pub const UNIT_COUNT: usize = {nu};");
    p!("/// Number of `BLDG` rows: `Production(UNIT_COUNT + i)` is row `i`.");
    p!("pub const BLDG_COUNT: usize = {nb};");
    p!("");
    p!("/// Every `PRTO` row.");
    p!("pub static UNITS: [UnitRow; {nu}] = [");
    let mut unit_names = vec![];
    for (i, u) in r.unit_types.iter().enumerate() {
        let name = u.name.text();
        let art = art_dir(&name);
        let abil = u.abilities;
        let excluded = abil
            & (ab::KING
                | ab::FLAG_UNIT
                | ab::CRUISE_MISSILE
                | ab::NUCLEAR_WEAPON
                | ab::TACTICAL_MISSILE)
            != 0;
        // Nobody builds the units a wonder hands out (`races` is empty), but
        // they have to exist as units.
        let produced = r.buildings.iter().any(|b| b.unit_produced == i as i32);
        let playable = u.alt_strategy_of == -1
            && !excluded
            && ((u.available_to_civs as u32) & our_mask != 0 || &*name == "Scout" || produced)
            && art.is_some();
        let sight = match (u.unit_class, &*name) {
            (1, _) => 2,
            (2, _) => 3,
            (_, "Scout" | "Explorer" | "Chasqui Scout") => 2,
            _ => 1,
        };
        p!(
            "    UnitRow {{ name: {}, art: {}, icon: {}, attack: {}, defense: {}, moves: {}, sight: {sight}, hp_bonus: {}, cost: {}, pop_cost: {}, tech: {}, upgrade_to: {}, resources: [{}, {}, {}], abilities: {:#x}, special: {:#x}, worker: {:#x}, worker_strength: {:?}, bombard: {}, bomb_range: {}, rof: {}, capacity: {}, class: {}, races: {:#x}, ai: {:#x}, zoc: {}, playable: {playable} }}, // {i}",
            q(&name),
            q(if playable { art.as_deref().unwrap_or("") } else { "" }),
            u.icon,
            u.attack,
            u.defense,
            u.movement,
            u.hit_point_bonus,
            u.shield_cost,
            u.population_cost,
            u.required_tech,
            u.upgrade_to,
            u.required_resource_1,
            u.required_resource_2,
            u.required_resource_3,
            abil,
            u.special_actions,
            u.worker_actions,
            u.worker_strength,
            u.bombard_strength,
            u.bombard_range,
            u.rate_of_fire,
            u.transport_capacity,
            u.unit_class,
            u.available_to_civs as u32,
            u.ai_strategies,
            u.zone_of_control != 0,
        );
        unit_names.push((i, name, u.alt_strategy_of));
    }
    p!("];");
    p!("");
    p!("/// Every `BLDG` row.");
    p!("pub static BLDGS: [BldgDef; {nb}] = [");
    let mut bldg_names = vec![];
    for (i, b) in r.buildings.iter().enumerate() {
        let name = b.name.text();
        let playable = BLDG_PLAYABLE.iter().any(|n| *n == &*name) || is_great_wonder(b.other_characteristics as u32) && !BLDG_NOT_PLAYABLE.iter().any(|n| *n == &*name);
        p!(
            "    BldgDef {{ name: {}, cost: {}, upkeep: {}, culture: {}, tech: {}, obsolete: {}, requires: {}, govt: {}, resources: [{}, {}], happy: {}, happy_all: {}, unhappy: {}, unhappy_all: {}, defense: {}, production: {}, grant_all: {}, grant_continent: {}, doubles: {}, flags: {:#x}, other: {:#x}, small: {:#x}, wonder: {:#x}, produces: {}, frequency: {}, playable: {playable} }}, // {i}",
            q(&name),
            b.cost,
            b.maintenance,
            b.culture,
            b.required_advance,
            b.rendered_obsolete_by,
            b.required_improvement,
            b.required_government,
            b.required_resource_1,
            b.required_resource_2,
            b.happy_faces,
            b.happy_faces_all_cities,
            b.unhappy_faces,
            b.unhappy_faces_all_cities,
            b.defense_bonus,
            b.production,
            b.gain_in_every_city,
            b.gain_in_every_city_on_continent,
            b.doubles_happiness_of,
            b.improvement_flags as u32,
            b.other_characteristics as u32,
            b.small_wonder_flags as u32,
            b.wonder_flags as u32,
            b.unit_produced,
            b.unit_frequency,
        );
        bldg_names.push((i, name));
    }
    p!("];");
    p!("");
    p!("#[allow(non_upper_case_globals)]");
    p!("impl UnitType {{");
    let mut seen = std::collections::HashSet::new();
    for (i, name, alt) in &unit_names {
        let id = ident(name);
        if *alt != -1 || !seen.insert(id.clone()) {
            continue;
        }
        p!("    pub const {id}: UnitType = UnitType({i});");
    }
    p!("}}");
    p!("");
    p!("#[allow(non_upper_case_globals)]");
    p!("impl Production {{");
    let mut seen_p = std::collections::HashSet::new();
    for (i, name, alt) in &unit_names {
        let id = ident(name);
        if *alt != -1 || !seen_p.insert(id.clone()) {
            continue;
        }
        p!("    pub const {id}: Production = Production({i});");
    }
    for (i, name) in &bldg_names {
        let id = ident(name);
        if !seen_p.insert(id.clone()) {
            continue;
        }
        p!("    pub const {id}: Production = Production({});", nu + i);
    }
    p!("}}");
    p!("");

    // --- constructors -------------------------------------------------------
    p!("/// The advance rules for `civ3mapgen::research`.");
    p!("pub fn rules() -> Rules {{");
    p!("    Rules {{");
    p!("        techs: TECHS");
    p!("            .iter()");
    p!("            .map(|&(cost, era, prereq, flags, flavors)| TechRow {{ cost, era, prereq, flags, flavors }})");
    p!("            .collect(),");
    p!("        future_tech_cost: FUTURE_TECH_COST,");
    p!("        max_research_turns: MAX_RESEARCH_TIME,");
    p!("        min_research_turns: MIN_RESEARCH_TIME,");
    p!("    }}");
    p!("}}");
    p!("");
    p!("/// The tables the AI valuation walks.");
    p!("pub fn tables() -> Tables {{");
    p!("    Tables {{");
    p!("        tfrm_required: TFRM.to_vec(),");
    p!("        good_prerequisite: GOOD.to_vec(),");
    p!("        units: PRTO");
    p!("            .iter()");
    p!("            .map(|&(required_tech, available_to_civs, ai_strategies, needs_resource)| AiUnitRow {{");
    p!("                required_tech,");
    p!("                available_to_civs,");
    p!("                ai_strategies,");
    p!("                needs_resource,");
    p!("            }})");
    p!("            .collect(),");
    p!("        govt_prerequisite: GOVT.to_vec(),");
    p!("        bldgs: BLDG");
    p!("            .iter()");
    p!("            .map(|&(required_advance, spaceship_part, improvement_flags, other_characteristics, flavors)| BldgRow {{");
    p!("                required_advance,");
    p!("                spaceship_part,");
    p!("                improvement_flags,");
    p!("                other_characteristics,");
    p!("                flavors,");
    p!("            }})");
    p!("            .collect(),");
    p!("        ctzn_prerequisite: CTZN.to_vec(),");
    p!("        flavors: FLAVORS.iter().map(|r| r.to_vec()).collect(),");
    p!("    }}");
    p!("}}");
    p!("");

    // --- the check ---------------------------------------------------------------
    p!("#[cfg(test)]");
    p!("mod tests {{");
    p!("    use super::*;");
    p!("");
    p!("    #[test]");
    p!("    fn techs_match_the_biq() {{");
    p!("        // TECH rows (file offsets): name +0, cost +0x40, era +0x44,");
    p!("        // prerequisites +0x54, flags +0x64, flavors +0x68. Skipped without");
    p!("        // the (git-ignored) GOG install.");
    p!("        let path = \"civ3/civ3-gog/app/Conquests/conquests.biq\";");
    p!("        let Ok(raw) = std::fs::read(path) else {{");
    p!("            eprintln!(\"skipped: {{path}} is not installed\");");
    p!("            return;");
    p!("        }};");
    p!("        let body = civ3mapgen::dcl::decompress(&raw).expect(\"biq decodes\");");
    p!("        let tech = civ3mapgen::dcl::sections(&body)");
    p!("            .into_iter()");
    p!("            .find(|s| s.tag_str() == \"TECH\")");
    p!("            .expect(\"TECH section\");");
    p!("        assert_eq!(tech.count as usize, TECH_NAMES.len());");
    p!("        let dword = |r: &[u8], at: usize| i32::from_le_bytes(r[at..at + 4].try_into().unwrap());");
    p!("        for (i, &(cost, era, prereq, flags, flavors)) in TECHS.iter().enumerate() {{");
    p!("            let row = tech.row(&body, i as u32).unwrap();");
    p!("            let name = &row[0..32];");
    p!("            let name = &name[..name.iter().position(|&c| c == 0).unwrap_or(name.len())];");
    p!("            assert_eq!(name, TECH_NAMES[i].as_bytes());");
    p!("            assert_eq!((cost, era), (dword(row, 0x40), dword(row, 0x44)), \"{{}}\", TECH_NAMES[i]);");
    p!("            for k in 0..4 {{");
    p!("                assert_eq!(prereq[k], dword(row, 0x54 + 4 * k), \"{{}}\", TECH_NAMES[i]);");
    p!("            }}");
    p!("            assert_eq!((flags, flavors), (dword(row, 0x64) as u32, dword(row, 0x68) as u32 & 0x7F), \"{{}}\", TECH_NAMES[i]);");
    p!("        }}");
    p!("    }}");
    p!("");
    p!("    #[test]");
    p!("    fn the_tables_are_consistent() {{");
    p!("        let r = rules();");
    p!("        let t = r.count();");
    p!("        for (i, row) in r.techs.iter().enumerate() {{");
    p!("            for &p in &row.prereq {{");
    p!("                assert!(p == -1 || (0..t).contains(&p), \"{{}} prerequisite {{p}}\", TECH_NAMES[i]);");
    p!("            }}");
    p!("        }}");
    p!("        let tb = tables();");
    p!("        assert!(tb.units.iter().all(|u| u.required_tech >= -1 && u.required_tech < t));");
    p!("        assert!(tb.bldgs.iter().all(|b| b.required_advance >= -1 && b.required_advance < t));");
    p!("        assert_eq!(tb.flavors.len(), 7);");
    p!("    }}");
    p!("}}");

    print!("{o}");
    ExitCode::SUCCESS
}
