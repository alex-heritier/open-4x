//! The rules of the game being played, read from a `.biq` at startup
//! (`docs/civ3-files.md` section 2).
//!
//! A [`Ruleset`] is plain Rust structs built once from [`civ3_biq::Biq`] in
//! `main`, leaked and kept in a `OnceLock`; nothing is written to disk. One
//! game runs per process, so the leak is once, by design. The tables are
//! reached through [`Table`]s (`UNITS`, `BLDGS`, `TECH_NAMES`, ...) that
//! deref to slices of the installed ruleset, so callers index them like the
//! arrays they replaced. `UnitType(i)` is `PRTO` row `i` and
//! `Production(UNIT_COUNT + i)` is `BLDG` row `i`: every cross-reference the
//! file makes is a plain index here too.

use std::collections::HashMap;
use std::sync::OnceLock;

use bevy::prelude::Color;
use civ3_biq::Biq;
use civ3_biq::sections::bldg::other_characteristics as oc;
use civ3_biq::sections::prto::ability as ab;
use civ3mapgen::research::{Rules, TechRow};
use civ3mapgen::research_ai::{BldgRow, Tables, UnitRow as AiUnitRow};

use crate::install::Install;
use crate::roster::{BldgDef, UnitRow};

/// The fixed data of one civilization (a `RACE` row), the fields the
/// clone reads: the names the interface shows, the team color (`ntpNN.pcx`
/// for the unit tint, `color` for badges) and the city name list. The
/// ruler lives in `LEADER_ROSTER`.
#[derive(Clone, Copy, Debug)]
pub struct CivDefinition {
    pub name: &'static str,
    pub adjective: &'static str,
    pub noun: &'static str,
    pub color: Color,
    /// The team-color ramp of the art (`RACE.default_color`).
    pub team_color: u8,
    pub city_names: &'static [&'static str],
}

/// One civilization's ruler, as the diplomacy screens name them.
#[derive(Clone, Copy, Debug)]
pub struct Leader {
    pub name: &'static str,
    pub title: &'static str,
    pub text_set: usize,
}

/// The `RACE` facts the research and diplomacy rules read for one civ.
#[derive(Clone, Copy, Debug)]
pub struct RaceFacts {
    /// `RACE` row.
    pub race: u32,
    /// `flavors` mask (mem `+0x960`).
    pub flavors: u32,
    /// `build_often` mask (mem `+0x954`).
    pub build_often: u32,
    /// `traits` mask (mem `+0x948`).
    pub traits: u32,
    /// `aggression` (the AI's attitude base, clamped to -2..=2).
    pub aggression: i32,
    /// `culture_group` (mem `+0x90C`).
    pub culture_group: i32,
    /// `shunned_government` (mem `+0x920`), a `GOVT` row or -1.
    pub shunned_government: i32,
    /// `favorite_government` (mem `+0x924`), a `GOVT` row or -1.
    pub favorite_government: i32,
    /// `scientific_leader_count`... free advances, `-1` for none.
    pub free_techs: [i32; 4],
}

/// Normal TERR values used by tile yields, worker jobs and land movement.
pub struct TerrainFacts {
    pub name: &'static str,
    pub disease: civ3mapgen::disease::Terrain,
    pub food: u8,
    pub shields: u8,
    pub commerce: u8,
    pub irrigation: u8,
    pub mining: u8,
    pub road: u8,
    pub movement: u8,
    pub worker_job: i32,
    pub impassable: bool,
    pub impassable_wheeled: bool,
}

/// `(cost, era, prerequisites, flags, flavors)` of an advance.
pub type TechFacts = (i32, i32, [i32; 4], u32, u32);

/// The `RULE` row: the numbers and unit slots the game reads from it.
#[derive(Clone, Debug, Default)]
pub struct General {
    pub future_tech_cost: i32,
    pub max_research_time: i32,
    pub min_research_time: i32,
    /// Gold per shield of price difference (`unit-upgrades.md` 6).
    pub upgrade_cost: i32,
    pub forest_shields: i32,
    pub default_difficulty: i32,
    pub town_max_size: i32,
    pub city_max_size: i32,
    pub metropolis_max_size: i32,
    pub scout_unit: i32,
    pub basic_barbarian_unit: i32,
    pub advanced_barbarian_unit: i32,
    pub barbarian_sea_unit: i32,
    pub battle_created_unit: i32,
    pub build_army_unit: i32,
    pub captured_unit: i32,
    pub start_unit_1: i32,
    pub start_unit_2: i32,
    pub flag_unit: i32,
    pub starting_treasury: i32,
}

/// Everything the game reads from the rules of a `.biq`.
pub struct Ruleset {
    pub general: General,
    pub units: Vec<UnitRow>,
    pub bldgs: Vec<BldgDef>,
    pub tech_names: Vec<&'static str>,
    pub techs: Vec<TechFacts>,
    /// `(era, icon, x, y)` of every advance on the Science Advisor.
    pub tech_tree: Vec<(i32, i32, i32, i32)>,
    pub era_names: Vec<&'static str>,
    pub difficulty_names: Vec<&'static str>,
    pub difficulty_cost_factor: Vec<i32>,
    pub difficulty_corruption: Vec<i32>,
    pub difficulty_quelled: Vec<i32>,
    pub size_names: Vec<&'static str>,
    /// `WSIZ` `(width, height)` per world size, in native tiles.
    pub size_dims: Vec<(i32, i32)>,
    pub world_tech_rate: Vec<i32>,
    pub work_needed: Vec<i32>,
    pub doubles_work: u128,
    pub bridges: u128,
    pub terrains: Vec<TerrainFacts>,
    pub tfrm: Vec<i32>,
    pub good: Vec<i32>,
    pub good_names: Vec<&'static str>,
    pub govt: Vec<i32>,
    pub cult: Vec<(i32, i32, i32)>,
    pub govt_assimilation: Vec<i32>,
    pub govt_resistance: Vec<Vec<i32>>,
    pub ctzn: Vec<i32>,
    pub prto: Vec<(i32, u32, u32, bool)>,
    pub bldg_rows: Vec<(i32, bool, u32, u32, u32)>,
    pub flavors: Vec<Vec<i32>>,
    pub civ_roster: Vec<CivDefinition>,
    pub leader_roster: Vec<Leader>,
    pub race_roster: Vec<RaceFacts>,
    /// The Civ3 art files the rules refer to and where the cache keeps them.
    pub art: crate::assets::ArtPlan,
}

/// The installed ruleset.
static RULESET: OnceLock<&'static Ruleset> = OnceLock::new();

/// Make `rs` the ruleset of this process. Returns the installed one (the
/// first call wins; one game runs per process).
pub fn install(rs: Ruleset) -> &'static Ruleset {
    RULESET.get_or_init(|| Box::leak(Box::new(rs)))
}

/// The ruleset in play. Tests load `conquests.biq` from the install on first
/// use; the game installs the rules of its file in `main` before any system
/// runs, so a miss anywhere else is a bug.
pub fn get() -> &'static Ruleset {
    #[cfg(test)]
    return RULESET.get_or_init(|| Box::leak(Box::new(test_ruleset())));
    #[cfg(not(test))]
    return RULESET.get().expect("no ruleset installed: load a .biq first (ruleset::install)");
}

#[cfg(test)]
fn test_ruleset() -> Ruleset {
    let root = std::env::var("CIV3_DIR")
        .or_else(|_| std::env::var("CIV3_GOG"))
        .unwrap_or_else(|_| crate::cli::DEFAULT_CIV3_DIR.to_string());
    let install = Install::new(root, vec![]);
    let path = install
        .resolve("Conquests/conquests.biq")
        .unwrap_or_else(|| panic!("tests need the Civ3 install ({}): set CIV3_DIR", install.root.display()));
    let biq = Biq::read_file(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    build(&biq, &install)
}

/// A table of the installed ruleset that derefs to a slice, so
/// `TECH_NAMES[i]`, `UNITS.iter()` and `UNITS.len()` read like the arrays
/// they replaced.
pub struct Table<T: 'static>(fn(&'static Ruleset) -> &'static [T]);

impl<T: 'static> std::ops::Deref for Table<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        (self.0)(get())
    }
}

macro_rules! tables {
    ($($(#[$m:meta])* $name:ident: $t:ty = $field:ident;)*) => {
        $($(#[$m])* pub static $name: Table<$t> = Table(|r| &r.$field);)*
    };
}

tables! {
    /// Every `PRTO` row.
    UNITS: UnitRow = units;
    /// Every `BLDG` row.
    BLDGS: BldgDef = bldgs;
    /// Name of every advance, in `TECH` order.
    TECH_NAMES: &'static str = tech_names;
    /// `(era, icon, x, y)` of every advance.
    TECH_TREE: (i32, i32, i32, i32) = tech_tree;
    /// `ERAS` names.
    ERA_NAMES: &'static str = era_names;
    /// `DIFF.cost_factor` per difficulty level.
    DIFFICULTY_COST_FACTOR: i32 = difficulty_cost_factor;
    /// `WSIZ` name per world size.
    SIZE_NAMES: &'static str = size_names;
    /// `WSIZ` `(width, height)` per world size.
    SIZE_DIMS: (i32, i32) = size_dims;
    /// `WSIZ.tech_rate` per world size.
    WORLD_TECH_RATE: i32 = world_tech_rate;
    /// `TFRM` base labor needed, before terrain movement cost.
    WORK_NEEDED: i32 = work_needed;
    /// Normal TERR values.
    TERRAINS: TerrainFacts = terrains;
    /// `GOOD.prerequisite` per `GOOD` row.
    GOOD: i32 = good;
    /// `GOOD` row names.
    GOOD_NAMES: &'static str = good_names;
    /// `CULT` rows: `(culture_ratio_percent, resistance_initial_percent, resistance_continued_percent)`.
    CULT: (i32, i32, i32) = cult;
    /// `GOVT.assimilation_chance` per `GOVT` row.
    GOVT_ASSIMILATION: i32 = govt_assimilation;
    /// `GOVT[owner].vs[other].resistance_modifier`.
    GOVT_RESISTANCE: Vec<i32> = govt_resistance;
    /// `DIFF.corruption_percent` per difficulty.
    DIFF_CORRUPTION: i32 = difficulty_corruption;
    /// `DIFF.citizens_quelled_by_military` per difficulty.
    DIFF_QUELLED: i32 = difficulty_quelled;
    /// Every playable civilization, in `RACE` row order (the barbarians, row 0, are not one).
    CIV_ROSTER: CivDefinition = civ_roster;
    /// Every playable civilization's ruler, in the same order as `CIV_ROSTER`.
    LEADER_ROSTER: Leader = leader_roster;
    /// The `RACE` facts of every playable civilization, in `CIV_ROSTER` order.
    RACE_ROSTER: RaceFacts = race_roster;
}

/// Number of `PRTO` rows: `UnitType(i)` is row `i`, `Production(i)` the same unit.
pub fn unit_count() -> usize {
    get().units.len()
}

/// Number of `BLDG` rows: `Production(unit_count() + i)` is row `i`.
pub fn bldg_count() -> usize {
    get().bldgs.len()
}

/// `RULE` future technology cost.
pub fn future_tech_cost() -> i32 {
    get().general.future_tech_cost
}
/// `RULE` maximum research time.
pub fn max_research_time() -> i32 {
    get().general.max_research_time
}
/// `RULE` minimum research time.
pub fn min_research_time() -> i32 {
    get().general.min_research_time
}
/// `RULE` upgrade cost: gold per shield of price difference (`unit-upgrades.md` 6).
pub fn upgrade_cost() -> i32 {
    get().general.upgrade_cost
}
/// `RULE.forest_value_in_shields`.
pub fn forest_shields() -> u16 {
    get().general.forest_shields.max(0) as u16
}
/// Advances that double the worker rate (`TECH` flag `0x40000`), one bit per `TECH` row.
pub fn doubles_work() -> u128 {
    get().doubles_work
}
/// Advances enabling road/rail bridges (`TECH` flag `4`), one bit per `TECH` row.
pub fn bridges() -> u128 {
    get().bridges
}

/// The advance rules for `civ3mapgen::research`.
pub fn rules() -> Rules {
    let r = get();
    Rules {
        techs: r
            .techs
            .iter()
            .map(|&(cost, era, prereq, flags, flavors)| TechRow { cost, era, prereq, flags, flavors })
            .collect(),
        future_tech_cost: r.general.future_tech_cost,
        max_research_turns: r.general.max_research_time,
        min_research_turns: r.general.min_research_time,
    }
}

/// The tables the AI valuation walks.
pub fn tables() -> Tables {
    let r = get();
    Tables {
        tfrm_required: r.tfrm.clone(),
        good_prerequisite: r.good.clone(),
        units: r
            .prto
            .iter()
            .map(|&(required_tech, available_to_civs, ai_strategies, needs_resource)| AiUnitRow {
                required_tech,
                available_to_civs,
                ai_strategies,
                needs_resource,
            })
            .collect(),
        govt_prerequisite: r.govt.clone(),
        bldgs: r
            .bldg_rows
            .iter()
            .map(|&(required_advance, spaceship_part, improvement_flags, other_characteristics, flavors)| BldgRow {
                required_advance,
                spaceship_part,
                improvement_flags,
                other_characteristics,
                flavors,
            })
            .collect(),
        ctzn_prerequisite: r.ctzn.clone(),
        flavors: r.flavors.clone(),
    }
}

// --- building the ruleset ----------------------------------------------------

fn leak(s: impl AsRef<str>) -> &'static str {
    Box::leak(s.as_ref().to_string().into_boxed_str())
}

/// Great wonders with effects the game cannot honor yet.
const BLDG_NOT_PLAYABLE: &[&str] = &["The Manhattan Project", "The United Nations"];

/// Improvements, besides the great wonders, whose effects the game
/// implements, by `BLDG` name. A mod's building with none of these names is
/// still built if it carries only flags the game reads (`implemented`).
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

/// The `#KEY` / value pairs of every `Text/PediaIcons.txt` along the search
/// path; the nearest file wins.
pub fn pedia_icons(install: &Install) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for path in install.resolve_all("Text/PediaIcons.txt") {
        let Ok(bytes) = crate::web::read_bytes(&path) else { continue };
        let text = String::from_utf8_lossy(&bytes);
        let mut key: Option<String> = None;
        for line in text.lines() {
            let line = line.trim();
            if let Some(k) = line.strip_prefix('#') {
                key = Some(k.trim().to_ascii_lowercase());
            } else if !line.is_empty() {
                if let Some(k) = key.take() {
                    out.entry(k).or_insert_with(|| line.to_string());
                }
            }
        }
    }
    out
}

/// The most saturated entry of the 64-color team ramp of `ntpNN.pcx`, the
/// color city badges, borders and the unit tint share. The VGA palette is the
/// last 768 bytes of a 256-color PCX.
fn team_rgb(install: &Install, n: i32) -> (u8, u8, u8) {
    let Some(path) = install.resolve(&format!("Art/Units/Palettes/ntp{n:02}.pcx")) else { return (128, 128, 128) };
    let Ok(bytes) = crate::web::read_bytes(path) else { return (128, 128, 128) };
    if bytes.len() < 768 {
        return (128, 128, 128);
    }
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

/// A team color the clone keeps Civ3's hand-picked tone for, so the default
/// four civilizations of the screenshots look the same whatever the ramp.
fn civ_color_override(name: &str) -> Option<(u8, u8, u8)> {
    Some(match name {
        "Japan" => (25, 148, 24),
        "Rome" => (190, 48, 48),
        "Egypt" => (220, 184, 48),
        "China" => (40, 180, 190),
        _ => return None,
    })
}

/// The `Art/Units` folder of a unit: the `#ANIMNAME_<civilopedia entry>` of
/// `PediaIcons.txt`, for an entry with era variants (`..._ERAS_<era>`) the
/// first era's, when the folder exists somewhere on the search path.
fn art_dir(install: &Install, pedia: &HashMap<String, String>, entry: &str, first_era: &str) -> Option<String> {
    let entry = entry.trim();
    let key = format!("animname_{}", entry.to_ascii_lowercase());
    let era_key = format!("{key}_{}", first_era.trim().to_ascii_lowercase());
    let name = pedia.get(&key).or_else(|| pedia.get(&era_key))?;
    // The folder's own spelling: references and file names disagree in case.
    let dir = install.resolve(&format!("Art/Units/{name}")).filter(|p| install.is_dir(p))?;
    dir.file_name().map(|f| f.to_string_lossy().into_owned())
}

/// Build the ruleset of `biq`, resolving art through `install`.
pub fn build(biq: &Biq, install: &Install) -> Ruleset {
    let r = &biq.rules;
    let g = r.general_rules.first().cloned().unwrap_or_else(|| panic!("the rules have no RULE row"));
    let general = General {
        future_tech_cost: g.future_tech_cost,
        max_research_time: g.max_research_time,
        min_research_time: g.min_research_time,
        upgrade_cost: g.upgrade_cost,
        forest_shields: g.forest_value_in_shields,
        default_difficulty: g.default_difficulty,
        town_max_size: g.town_max_size,
        city_max_size: g.city_max_size,
        metropolis_max_size: g.metropolis_max_size,
        scout_unit: g.scout_unit,
        basic_barbarian_unit: g.basic_barbarian_unit,
        advanced_barbarian_unit: g.advanced_barbarian_unit,
        barbarian_sea_unit: g.barbarian_sea_unit,
        battle_created_unit: g.battle_created_unit,
        build_army_unit: g.build_army_unit,
        captured_unit: g.captured_unit,
        start_unit_1: g.start_unit_1,
        start_unit_2: g.start_unit_2,
        flag_unit: g.flag_unit,
        starting_treasury: g.starting_treasury,
    };
    if r.techs.len() > 128 {
        eprintln!("warning: {} advances; the game tracks the first 128", r.techs.len());
    }
    let mask = |pred: &dyn Fn(u32) -> bool| -> u128 {
        r.techs.iter().enumerate().filter(|(i, t)| *i < 128 && pred(t.flags as u32)).fold(0, |bits, (i, _)| bits | (1u128 << i))
    };

    let pedia = pedia_icons(install);
    let first_era = r.eras.first().map(|e| e.civilopedia_entry.text().to_string()).unwrap_or_default();
    // The roster is every `RACE` row but the barbarians (the one with
    // `civilization_index == 0`). Roster index `i` is `RACE` row `i + 1`.
    let roster: Vec<&_> = r.civilizations.iter().filter(|c| c.civilization_index != 0).collect();
    let our_mask: u32 = (0..roster.len()).fold(0u32, |a, i| a | (1u32.checked_shl(i as u32 + 1).unwrap_or(0)));

    let units: Vec<UnitRow> = r
        .unit_types
        .iter()
        .enumerate()
        .map(|(i, u)| {
            let name = u.name.text().to_string();
            let art = art_dir(install, &pedia, &u.civilopedia_entry.text(), &first_era);
            let abil = u.abilities;
            let excluded = abil & (ab::KING | ab::FLAG_UNIT | ab::CRUISE_MISSILE | ab::NUCLEAR_WEAPON | ab::TACTICAL_MISSILE) != 0;
            // Nobody builds the units a wonder hands out (`races` is empty), but
            // they have to exist as units.
            let produced = r.buildings.iter().any(|b| b.unit_produced == i as i32);
            let playable = u.alt_strategy_of == -1
                && !excluded
                && ((u.available_to_civs as u32) & our_mask != 0 || i as i32 == general.scout_unit || produced)
                && art.is_some();
            UnitRow {
                name: leak(&name),
                art: if playable { leak(art.as_deref().unwrap_or("")) } else { "" },
                icon: u.icon,
                attack: u.attack,
                defense: u.defense,
                moves: u.movement as u8,
                sight: 1,
                hp_bonus: u.hit_point_bonus,
                cost: u.shield_cost,
                pop_cost: u.population_cost,
                tech: u.required_tech,
                upgrade_to: u.upgrade_to,
                resources: [u.required_resource_1, u.required_resource_2, u.required_resource_3],
                abilities: abil,
                special: u.special_actions,
                worker: u.worker_actions,
                worker_strength: u.worker_strength,
                bombard: u.bombard_strength,
                bomb_range: u.bombard_range,
                rof: u.rate_of_fire,
                capacity: u.transport_capacity,
                class: u.unit_class,
                races: u.available_to_civs as u32,
                ai: u.ai_strategies,
                zoc: u.zone_of_control != 0,
                playable,
            }
        })
        .collect();
    let alt: Vec<i32> = r.unit_types.iter().map(|u| u.alt_strategy_of).collect();
    let units = set_sights(units, &alt);
    let art = crate::assets::plan(biq, install, &pedia, &units);

    let bldgs: Vec<BldgDef> = r
        .buildings
        .iter()
        .map(|b| {
            let name = b.name.text().to_string();
            let wonder = b.other_characteristics as u32 & oc::WONDER != 0;
            let playable = BLDG_PLAYABLE.contains(&name.as_str()) || wonder && !BLDG_NOT_PLAYABLE.contains(&name.as_str());
            BldgDef {
                name: leak(&name),
                cost: b.cost,
                upkeep: b.maintenance,
                culture: b.culture,
                tech: b.required_advance,
                obsolete: b.rendered_obsolete_by,
                requires: b.required_improvement,
                govt: b.required_government,
                resources: [b.required_resource_1, b.required_resource_2],
                happy: b.happy_faces,
                happy_all: b.happy_faces_all_cities,
                unhappy: b.unhappy_faces,
                unhappy_all: b.unhappy_faces_all_cities,
                defense: b.defense_bonus,
                production: b.production,
                grant_all: b.gain_in_every_city,
                grant_continent: b.gain_in_every_city_on_continent,
                doubles: b.doubles_happiness_of,
                bombard_defense: b.bombard_defense,
                flags: b.improvement_flags as u32,
                other: b.other_characteristics as u32,
                small: b.small_wonder_flags as u32,
                wonder: b.wonder_flags as u32,
                produces: b.unit_produced,
                frequency: b.unit_frequency,
                playable,
            }
        })
        .collect();

    let civ_roster = roster
        .iter()
        .map(|c| {
            let name = c.civilization_name.text().to_string();
            let (rr, gg, bb) = civ_color_override(&name).unwrap_or_else(|| team_rgb(install, c.default_color));
            let cities: Vec<&'static str> = c
                .city_names
                .iter()
                .map(|s| s.text())
                .filter(|s| !s.is_empty())
                .map(|s| leak(&*s))
                .collect();
            CivDefinition {
                name: leak(&name),
                adjective: leak(&*c.adjective.text()),
                noun: leak(&*c.noun.text()),
                color: Color::srgb_u8(rr, gg, bb),
                team_color: c.default_color as u8,
                city_names: Box::leak(cities.into_boxed_slice()),
            }
        })
        .collect();
    let leader_roster = roster
        .iter()
        .enumerate()
        .map(|(i, c)| Leader { name: leak(&*c.leader_name.text()), title: leak(&*c.title.text()), text_set: i })
        .collect();
    let race_roster = roster
        .iter()
        .enumerate()
        .map(|(i, c)| RaceFacts {
            race: i as u32 + 1,
            flavors: c.conquests.as_ref().map_or(0, |e| e.flavors),
            build_often: c.build_often,
            traits: c.traits,
            aggression: c.aggression,
            culture_group: c.culture_group,
            shunned_government: c.shunned_government,
            favorite_government: c.favorite_government,
            free_techs: c.free_techs,
        })
        .collect();

    let terrains = r
        .terrains
        .iter()
        .map(|t| TerrainFacts {
            name: leak(&*t.name.text()),
            disease: civ3mapgen::disease::Terrain {
                causes: t.causes_disease(),
                cured: t.cured_by_sanitation(),
                strength: t.disease_strength,
            },
            food: t.food as u8,
            shields: t.shields as u8,
            commerce: t.commerce as u8,
            irrigation: t.irrigation_bonus as u8,
            mining: t.mining_bonus as u8,
            road: t.road_bonus as u8,
            movement: t.movement_cost as u8,
            worker_job: t.worker_job,
            impassable: t.flags.impassable != 0,
            impassable_wheeled: t.flags.impassable_wheeled != 0,
        })
        .collect();

    Ruleset {
        general,
        units,
        bldgs,
        tech_names: r.techs.iter().map(|t| leak(&*t.name.text())).collect(),
        techs: r.techs.iter().map(|t| (t.cost, t.era, t.prerequisites, t.flags, t.flavors)).collect(),
        tech_tree: r.techs.iter().map(|t| (t.era, t.icon, t.tree_x, t.tree_y)).collect(),
        era_names: r.eras.iter().map(|e| leak(&*e.name.text())).collect(),
        difficulty_names: r.difficulties.iter().map(|d| leak(&*d.name.text())).collect(),
        difficulty_cost_factor: r.difficulties.iter().map(|d| d.cost_factor).collect(),
        difficulty_corruption: r.difficulties.iter().map(|d| d.corruption_percent).collect(),
        difficulty_quelled: r.difficulties.iter().map(|d| d.citizens_quelled_by_military).collect(),
        size_names: r.world_sizes.iter().map(|d| leak(&*d.name.text())).collect(),
        size_dims: r.world_sizes.iter().map(|d| (d.width, d.height)).collect(),
        world_tech_rate: r.world_sizes.iter().map(|d| d.tech_rate).collect(),
        work_needed: r.worker_jobs.iter().map(|j| j.turns_to_complete).collect(),
        doubles_work: mask(&|f| f & 0x40000 != 0),
        bridges: mask(&|f| f & 4 != 0),
        terrains,
        tfrm: r.worker_jobs.iter().map(|j| j.required_tech).collect(),
        good: r.goods.iter().map(|j| j.prerequisite).collect(),
        good_names: r.goods.iter().map(|g| leak(&*g.name.text())).collect(),
        govt: r.governments.iter().map(|j| j.prerequisite_tech).collect(),
        cult: r
            .cultures
            .iter()
            .map(|c| (c.culture_ratio_percent, c.resistance_initial_percent, c.resistance_continued_percent))
            .collect(),
        govt_assimilation: r.governments.iter().map(|g| g.assimilation_chance).collect(),
        govt_resistance: r.governments.iter().map(|g| g.vs.iter().map(|v| v.resistance_modifier).collect()).collect(),
        ctzn: r.citizens.iter().map(|j| j.prerequisite).collect(),
        prto: r
            .unit_types
            .iter()
            .map(|u| {
                let res = u.required_resource_1 != -1 || u.required_resource_2 != -1 || u.required_resource_3 != -1;
                (u.required_tech, u.available_to_civs as u32, u.ai_strategies, res)
            })
            .collect(),
        bldg_rows: r
            .buildings
            .iter()
            .map(|b| {
                (
                    b.required_advance,
                    b.spaceship_part != -1,
                    b.improvement_flags as u32,
                    b.other_characteristics as u32,
                    b.flavors as u32,
                )
            })
            .collect(),
        flavors: biq
            .flavors
            .as_ref()
            .map(|f| f.flavors.iter().map(|x| x.relations.clone()).collect())
            .unwrap_or_default(),
        civ_roster,
        leader_roster,
        race_roster,
        art,
    }
}

/// How far a unit sees. Civ3 gives every unit radius 2 with line of sight;
/// the clone's simpler model sees 1 on land, 2 at sea, 3 in the air, and 2
/// for explorers: the units with the *Explore* AI strategy and the units
/// those strategy rows stand in for (`alt_strategy_of`).
fn set_sights(mut units: Vec<UnitRow>, alt_strategy_of: &[i32]) -> Vec<UnitRow> {
    const EXPLORE: u32 = civ3_biq::sections::prto::ai::EXPLORE;
    let mut explorers = vec![false; units.len()];
    for (i, u) in units.iter().enumerate() {
        if u.ai & EXPLORE != 0 {
            explorers[i] = true;
            if let Some(&of) = alt_strategy_of.get(i).filter(|&&o| o >= 0 && (o as usize) < explorers.len()) {
                explorers[of as usize] = true;
            }
        }
    }
    for (i, u) in units.iter_mut().enumerate() {
        u.sight = match u.class {
            1 => 2,
            2 => 3,
            _ if explorers[i] => 2,
            _ => 1,
        };
    }
    units
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shipped Conquests rules: row counts, Walls' bombard defense and
    /// the roster tables the BIQ fills in.
    #[test]
    fn conquests_rows() {
        let r = get();
        assert_eq!((r.units.len(), r.bldgs.len()), (141, 83));
        let walls = r.bldgs.iter().find(|b| b.name == "Walls").unwrap();
        assert_eq!(walls.bombard_defense, 8);
        assert_eq!(r.bldgs.iter().filter(|b| b.bombard_defense != 0).count(), 1);
        assert!(!r.race_roster.is_empty() && !r.leader_roster.is_empty());
        assert_eq!(r.tech_names.len(), r.tech_tree.len());
    }

    /// `PediaIcons.txt` names the art folder, not the unit's name.
    #[test]
    fn art_folders_follow_the_pedia_icons() {
        let r = get();
        let art = |n: &str| r.units.iter().find(|u| u.name == n).unwrap().art;
        assert!(art("Numidian Mercenary").eq_ignore_ascii_case("Libyan Mercenary"));
        assert!(art("Paratrooper").eq_ignore_ascii_case("WWII Paratrooper"));
    }
}
