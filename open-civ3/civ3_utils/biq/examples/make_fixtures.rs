//! Write the committed scenario and save fixtures from nothing:
//!
//! ```text
//! cargo run --release --example make_fixtures [-- OUT_DIR]     # tests/data
//! ```
//!
//! *Lantern Isles* is an original rule set (eras, advances, units, buildings,
//! governments, civilizations, terrain names), an original 40x40 archipelago
//! and four players; `LANTERN.SAV` is the same game twenty turns on. Nothing is
//! read from a Civ3 install. Only the table shapes the engine indexes by row
//! (14 terrains, 13 worker jobs, 8 governments, 4 eras and 4 experience levels)
//! keep the stock row order, and the resources use the plain commodity nouns
//! the clone matches by name. The art references name the folders, clips and
//! pictures `tools/make_stub_assets.py --biq` draws for them.
//!
//! The output is deterministic: running this again writes the same bytes.

use civ3_biq::dcl::Storage;
use civ3_biq::file::CANONICAL_ORDER;
use civ3_biq::io::Str;
use civ3_biq::sav::{City as SavCity, Unit as SavUnit};
use civ3_biq::sections::bldg::{Building, improvement_flags as imp, other_characteristics as oc};
use civ3_biq::sections::city::City;
use civ3_biq::sections::cont::Continent;
use civ3_biq::sections::ctzn::Citizen;
use civ3_biq::sections::cult::CultureLevel;
use civ3_biq::sections::diff::Difficulty;
use civ3_biq::sections::eras::Era;
use civ3_biq::sections::espn::EspionageMission;
use civ3_biq::sections::expr::ExperienceLevel;
use civ3_biq::sections::flav::{Flavor, Flavors};
use civ3_biq::sections::game::{Game, PlayableCiv};
use civ3_biq::sections::good::{Good, kind};
use civ3_biq::sections::govt::{Government, RulerTitle, VsGovernment};
use civ3_biq::sections::lead::{Player, StartingUnit};
use civ3_biq::sections::prto::{UnitType, ability, ai, class, special, worker};
use civ3_biq::sections::race::{Civilization, CivilizationConquestsExtension};
use civ3_biq::sections::rule::GeneralRules;
use civ3_biq::sections::sloc::StartLocation;
use civ3_biq::sections::tech::{Tech, flags as tf};
use civ3_biq::sections::terr::{Terrain, disease};
use civ3_biq::sections::tfrm::WorkerJob;
use civ3_biq::sections::tile::{Tile, feature, index_from_coords};
use civ3_biq::sections::unit::Unit;
use civ3_biq::sections::wchr::WorldCharacteristics;
use civ3_biq::sections::wmap::WorldMap;
use civ3_biq::sections::wsiz::WorldSize;
use civ3_biq::{Biq, Save, Version};
use std::path::PathBuf;

/// File names of the two fixtures.
const BIQ_NAME: &str = "Lantern Isles.biq";
const SAV_NAME: &str = "LANTERN.SAV";

/// Native map size (staggered: `(W/2)·H` cells).
const W: i32 = 40;
const H: i32 = 40;

/// Cell of the first fixture unit in the 14-wide unit icon sheets: the
/// generated sheets draw the `unit` entries of `tools/stub_asset_refs.json`
/// in order, and this rule set's units follow the 139 already listed there.
const ICON_BASE: i32 = 139;

/// The turn and year the save is taken at.
const SAVE_TURN: u32 = 20;

fn s<const N: usize>(text: &str) -> Str<N> {
    Str::new(text)
}

/// `Name With Spaces` -> `PREFIX_Name_With_Spaces`.
fn key(prefix: &str, name: &str) -> String {
    format!("{prefix}_{}", name.replace(' ', "_"))
}

/// A small integer hash, so the map needs no RNG crate and never changes.
fn hash(x: i32, y: i32, salt: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA77) ^ salt.wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^ (h >> 15)
}

// ------------------------------------------------------------------ rules

const ERAS: [(&str, [&str; 3]); 4] = [
    ("Ember Age", ["Hearth Keeper", "Elder", "Storyteller"]),
    ("Tide Age", ["Scribe", "Tallier", "Astrologer"]),
    ("Gear Age", ["Tinkerer", "Clockwright", "Lens Maker"]),
    ("Spark Age", ["Coilwright", "Engineer", "Savant"]),
];

/// Advances: name, era, cost, prerequisites, flags, tree column, tree row.
const TECHS: [(&str, i32, i32, [i32; 2], u32, i32, i32); 16] = [
    ("Fire Keeping", 0, 2, [-1, -1], 0, 0, 0),
    ("Stone Shaping", 0, 2, [-1, -1], 0, 0, 2),
    ("Rope Weaving", 0, 3, [0, -1], 0, 1, 0),
    ("Ox Taming", 0, 3, [0, -1], 0, 1, 1),
    ("Clay Tablets", 0, 4, [1, -1], tf::DIPLOMATS | tf::MILITARY_ALLIANCE, 1, 2),
    ("Star Reading", 0, 5, [2, 4], 0, 2, 1),
    ("Tin Smelting", 1, 6, [1, 0], 0, 0, 0),
    ("Reed Boats", 1, 6, [2, -1], tf::RIGHT_OF_PASSAGE, 0, 1),
    ("Tally Sticks", 1, 7, [4, -1], 0, 0, 2),
    ("Chant Halls", 1, 7, [5, -1], 0, 1, 1),
    ("Terrace Farming", 1, 8, [3, 6], tf::BRIDGES, 1, 0),
    ("Gear Trains", 2, 10, [8, 10], 0, 0, 0),
    ("Lens Grinding", 2, 10, [9, 8], tf::TRADE_OVER_SEA, 0, 2),
    ("Black Salts", 2, 12, [11, -1], 0, 1, 1),
    ("Steam Kettles", 3, 16, [13, 12], 0, 0, 1),
    ("Spark Coils", 3, 18, [14, -1], 0, 1, 1),
];

fn techs() -> Vec<Tech> {
    TECHS
        .iter()
        .enumerate()
        .map(|(i, &(name, era, cost, pre, flags, col, row))| Tech {
            name: s(name),
            civilopedia_entry: s(&key("TECH", name)),
            cost,
            era,
            icon: i as i32,
            tree_x: 60 + col * 220,
            tree_y: 110 + row * 130,
            prerequisites: [pre[0], pre[1], -1, -1],
            flags,
            flavors: 1 << (i % 3),
            ..Tech::default()
        })
        .collect()
}

/// The commodities the clone draws by name (`features::GOODS`).
const GOODS: [(&str, u32, i32, [i32; 3]); 8] = [
    ("Horses", kind::STRATEGIC, 3, [0, 1, 0]),
    ("Iron", kind::STRATEGIC, 6, [0, 1, 0]),
    ("Wheat", kind::BONUS, -1, [2, 0, 0]),
    ("Cattle", kind::BONUS, -1, [2, 0, 0]),
    ("Fish", kind::BONUS, -1, [2, 0, 0]),
    ("Gold", kind::BONUS, -1, [0, 0, 4]),
    ("Silks", kind::LUXURY, -1, [0, 0, 1]),
    ("Wines", kind::LUXURY, -1, [1, 0, 1]),
];
const HORSES: i32 = 0;
const IRON: i32 = 1;

fn goods() -> Vec<Good> {
    GOODS
        .iter()
        .enumerate()
        .map(|(i, &(name, k, prerequisite, [food, shields, commerce]))| Good {
            name: s(name),
            civilopedia_entry: s(&key("GOOD", name)),
            kind: k,
            appearance_ratio: 120,
            disappearance_probability: if k == kind::STRATEGIC { 20 } else { 0 },
            icon: i as i32,
            prerequisite,
            food_bonus: food,
            shield_bonus: shields,
            commerce_bonus: commerce,
            ..Good::default()
        })
        .collect()
}

/// Terrain rows in the fixed Conquests order (desert, plains, grassland,
/// tundra, flood plain, hills, mountains, forest, jungle, marsh, volcano,
/// coast, sea, ocean): name, food, shields, commerce, irrigation, mining,
/// road, movement, defense, worker job, the goods it may carry.
#[allow(clippy::type_complexity)]
const TERRAINS: [(&str, [i32; 9], i32, &[i32]); 14] = [
    ("Dunes", [0, 1, 0, 1, 1, 1, 1, 10, 0], -1, &[5]),
    ("Steppe", [1, 1, 0, 1, 0, 1, 1, 0, 0], 5, &[0, 2]),
    ("Meadow", [2, 0, 0, 1, 0, 1, 1, 0, 0], 5, &[3]),
    ("Frostfield", [1, 0, 0, 0, 0, 1, 1, 0, 0], 5, &[]),
    ("Silt Flats", [3, 0, 0, 1, 0, 1, 1, 0, 0], -1, &[2]),
    ("Downs", [1, 0, 0, 1, 3, 0, 2, 50, 0], -1, &[1, 7]),
    ("Crags", [0, 1, 0, 0, 2, 0, 3, 100, 0], -1, &[1, 5]),
    ("Woodland", [1, 2, 0, 0, 0, 0, 2, 25, 0], 6, &[6]),
    ("Tanglewood", [1, 0, 0, 0, 0, 0, 2, 25, 0], 7, &[6]),
    ("Fen", [1, 0, 0, 0, 0, 0, 2, 20, 0], 7, &[]),
    ("Fire Peak", [0, 3, 0, 0, 0, 0, 3, 80, 0], -1, &[]),
    ("Shallows", [1, 0, 2, 0, 0, 0, 1, 10, 0], -1, &[4]),
    ("Open Water", [1, 0, 2, 0, 0, 0, 1, 10, 0], -1, &[4]),
    ("Deep Water", [1, 0, 2, 0, 0, 0, 1, 10, 0], -1, &[]),
];
const DESERT: u8 = 0;
const PLAINS: u8 = 1;
const GRASS: u8 = 2;
const TUNDRA: u8 = 3;
const HILLS: u8 = 5;
const MOUNTAINS: u8 = 6;
const FOREST: u8 = 7;
const JUNGLE: u8 = 8;
const MARSH: u8 = 9;
const COAST: u8 = 11;
const SEA: u8 = 12;
const OCEAN: u8 = 13;

fn terrains() -> Vec<Terrain> {
    let n = GOODS.len();
    TERRAINS
        .iter()
        .enumerate()
        .map(|(i, &(name, [food, shields, commerce, irr, mine, road, mv, def, _], job, goods))| {
            let mut mask = vec![0u8; n.div_ceil(8).max(4)];
            for &g in goods {
                mask[g as usize / 8] |= 1 << (g % 8);
            }
            let mut t = Terrain {
                goods_count: n as u32,
                resource_mask: mask,
                name: s(name),
                civilopedia_entry: s(&key("TERR", name)),
                irrigation_bonus: irr,
                mining_bonus: mine,
                road_bonus: road,
                defense_bonus: def,
                movement_cost: mv,
                food,
                shields,
                commerce,
                worker_job: job,
                ..Terrain::default()
            };
            let water = i >= COAST as usize;
            if water {
                t.flags.allow_cities = 0;
                t.flags.allow_colonies = 0;
                t.flags.allow_forts = 0;
                t.flags.allow_outposts = 0;
                t.flags.allow_radar_towers = 0;
                t.flags.allow_airfields = 0;
            }
            if [MOUNTAINS, JUNGLE, MARSH, 10].contains(&(i as u8)) {
                t.flags.impassable_wheeled = 1;
            }
            if [4, JUNGLE, MARSH].contains(&(i as u8)) {
                t.disease_flags = disease::CAUSES_DISEASE | disease::CURED_BY_SANITATION;
                t.disease_strength = 30;
            }
            t
        })
        .collect()
}

/// Worker jobs in the fixed order the engine numbers them: name, turns,
/// required advance, order text.
const JOBS: [(&str, i32, i32, &str); 13] = [
    ("Shaft", 10, -1, "Sink Shaft"),
    ("Channels", 6, -1, "Dig Channels"),
    ("Stockade", 14, 8, "Raise Stockade"),
    ("Track", 4, -1, "Lay Track"),
    ("Rail", 12, 14, "Lay Rail"),
    ("Sow Woods", 16, 10, "Sow Woods"),
    ("Fell Woods", 4, -1, "Fell Woods"),
    ("Drain Bog", 14, -1, "Drain Bog"),
    ("Mend Ground", 20, -1, "Mend Ground"),
    ("Airstrip", 1, 15, "Lay Airstrip"),
    ("Watchtower", 1, 15, "Raise Watchtower"),
    ("Lookout", 1, 1, "Raise Lookout"),
    ("Barrier", 14, 8, "Raise Barrier"),
];

fn worker_jobs() -> Vec<WorkerJob> {
    JOBS.iter()
        .map(|&(name, turns, tech, order)| WorkerJob {
            name: s(name),
            civilopedia_entry: s(&key("TFRM", name).to_ascii_uppercase()),
            turns_to_complete: turns,
            required_tech: tech,
            required_resource_1: if name == "Rail" { IRON } else { -1 },
            required_resource_2: -1,
            order: s(order),
            ..WorkerJob::default()
        })
        .collect()
}

/// A unit row. `name` is also the art folder: `make_stub_assets.py` draws
/// a folder per name and picks the figure from words in it.
struct U {
    name: &'static str,
    stats: [i32; 4], // attack, defense, movement, shield cost
    tech: i32,
    class: i32,
    abilities: u32,
    ai: u32,
    special: u32,
    worker: u32,
    pop: i32,
    upgrade: i32,
    resource: i32,
    bombard: [i32; 3], // strength, range, rate of fire
    capacity: i32,
    civs: i32,
}

const ALL_CIVS: i32 = -2; // every RACE row but the barbarians
const EVERYONE: i32 = -1;
const FIGHT: u32 = special::PILLAGE | special::CAPTURE | special::UPGRADE_UNIT;

const UNITS: [U; 14] = [
    U { name: "Homesteader", stats: [0, 0, 1, 30], tech: -1, class: class::LAND, abilities: ability::FOOT_UNIT, ai: ai::SETTLE, special: 0, worker: worker::BUILD_CITY | worker::BUILD_ROAD | worker::JOIN_CITY, pop: 1, upgrade: -1, resource: -1, bombard: [0; 3], capacity: 0, civs: ALL_CIVS },
    U { name: "Delver", stats: [0, 0, 1, 10], tech: -1, class: class::LAND, abilities: ability::FOOT_UNIT, ai: ai::TERRAFORM, special: 0, worker: worker::BUILD_ROAD | worker::BUILD_MINE | worker::IRRIGATE | worker::CLEAR_FOREST | worker::CLEAR_JUNGLE | worker::PLANT_FOREST | worker::CLEAR_POLLUTION | worker::AUTOMATE | worker::JOIN_CITY | worker::BUILD_FORT | worker::BUILD_RAILROAD | worker::BUILD_OUTPOST | worker::BUILD_BARRICADE, pop: 0, upgrade: -1, resource: -1, bombard: [0; 3], capacity: 0, civs: ALL_CIVS },
    U { name: "Wayfinder", stats: [0, 1, 2, 10], tech: -1, class: class::LAND, abilities: ability::FOOT_UNIT | ability::ALL_TERRAIN_AS_ROADS, ai: ai::EXPLORE, special: 0, worker: 0, pop: 0, upgrade: -1, resource: -1, bombard: [0; 3], capacity: 0, civs: ALL_CIVS },
    U { name: "Cudgeler", stats: [1, 1, 1, 10], tech: -1, class: class::LAND, abilities: ability::FOOT_UNIT, ai: ai::OFFENSE | ai::DEFENSE, special: FIGHT, worker: 0, pop: 0, upgrade: 5, resource: -1, bombard: [0; 3], capacity: 0, civs: EVERYONE },
    U { name: "Slinger", stats: [2, 1, 1, 20], tech: 1, class: class::LAND, abilities: ability::FOOT_UNIT, ai: ai::OFFENSE, special: FIGHT, worker: 0, pop: 0, upgrade: -1, resource: -1, bombard: [0; 3], capacity: 0, civs: ALL_CIVS },
    U { name: "Pikeguard", stats: [1, 2, 1, 20], tech: 6, class: class::LAND, abilities: ability::FOOT_UNIT, ai: ai::DEFENSE, special: FIGHT, worker: 0, pop: 0, upgrade: 12, resource: -1, bombard: [0; 3], capacity: 0, civs: ALL_CIVS },
    U { name: "Bladehand", stats: [3, 2, 1, 30], tech: 6, class: class::LAND, abilities: ability::FOOT_UNIT, ai: ai::OFFENSE, special: FIGHT, worker: 0, pop: 0, upgrade: -1, resource: IRON, bombard: [0; 3], capacity: 0, civs: EVERYONE },
    U { name: "Ox Rider", stats: [2, 1, 2, 20], tech: 3, class: class::LAND, abilities: 0, ai: ai::OFFENSE, special: FIGHT, worker: 0, pop: 0, upgrade: -1, resource: HORSES, bombard: [0; 3], capacity: 0, civs: ALL_CIVS },
    U { name: "Stone Thrower", stats: [0, 1, 1, 30], tech: 11, class: class::LAND, abilities: ability::WHEELED | ability::RANGED_ATTACK_ANIMATION, ai: ai::ARTILLERY, special: special::BOMBARD, worker: 0, pop: 0, upgrade: -1, resource: -1, bombard: [4, 1, 1], capacity: 0, civs: ALL_CIVS },
    U { name: "Reed Skiff", stats: [1, 1, 3, 30], tech: 7, class: class::SEA, abilities: ability::TRANSPORTS_ONLY_FOOT_UNITS | ability::SINKS_IN_SEA | ability::SINKS_IN_OCEAN, ai: ai::NAVAL_TRANSPORT | ai::EXPLORE, special: special::LOAD | special::UNLOAD, worker: 0, pop: 0, upgrade: -1, resource: -1, bombard: [0; 3], capacity: 2, civs: EVERYONE },
    U { name: "Fusilier", stats: [3, 4, 1, 40], tech: 13, class: class::LAND, abilities: ability::FOOT_UNIT, ai: ai::DEFENSE, special: FIGHT, worker: 0, pop: 0, upgrade: -1, resource: -1, bombard: [0; 3], capacity: 0, civs: ALL_CIVS },
    U { name: "Steam Crawler", stats: [8, 4, 2, 80], tech: 15, class: class::LAND, abilities: ability::WHEELED | ability::BLITZ, ai: ai::OFFENSE, special: FIGHT, worker: 0, pop: 0, upgrade: -1, resource: IRON, bombard: [0; 3], capacity: 0, civs: ALL_CIVS },
    U { name: "Warleader", stats: [0, 0, 1, 0], tech: -1, class: class::LAND, abilities: ability::LEADER | ability::FOOT_UNIT, ai: ai::LEADER, special: special::BUILD_ARMY, worker: 0, pop: 0, upgrade: -1, resource: -1, bombard: [0; 3], capacity: 0, civs: ALL_CIVS },
    U { name: "War Host", stats: [0, 0, 1, 0], tech: -1, class: class::LAND, abilities: ability::ARMY, ai: ai::ARMY, special: 0, worker: 0, pop: 0, upgrade: -1, resource: -1, bombard: [0; 3], capacity: 3, civs: ALL_CIVS },
];
const HOMESTEADER: i32 = 0;
const DELVER: i32 = 1;
const WAYFINDER: i32 = 2;
const CUDGELER: i32 = 3;
const SLINGER: i32 = 4;
const BLADEHAND: i32 = 6;
const OX_RIDER: i32 = 7;
const REED_SKIFF: i32 = 9;
const WARLEADER: i32 = 12;
const WAR_HOST: i32 = 13;

fn unit_types() -> Vec<UnitType> {
    UNITS
        .iter()
        .enumerate()
        .map(|(i, u)| {
            let [attack, defense, movement, cost] = u.stats;
            let mut t = UnitType {
                zone_of_control: u32::from(u.name == "Steam Crawler"),
                name: s(u.name),
                civilopedia_entry: s(&key("PRTO", u.name)),
                bombard_strength: u.bombard[0],
                bombard_range: u.bombard[1],
                transport_capacity: u.capacity,
                shield_cost: cost,
                defense,
                icon: ICON_BASE + i as i32,
                attack,
                population_cost: u.pop,
                rate_of_fire: u.bombard[2],
                movement,
                required_tech: u.tech,
                upgrade_to: u.upgrade,
                required_resource_1: u.resource,
                abilities: u.abilities,
                ai_strategies: u.ai,
                available_to_civs: u.civs,
                unit_class: u.class,
                special_actions: u.special,
                worker_actions: u.worker,
                ignore_move_cost: vec![0; TERRAINS.len()],
                ..UnitType::default()
            };
            t.command_mask = t.derived_command_mask();
            t.worker_strength = t.derived_worker_strength();
            t
        })
        .collect()
}

/// Buildings: name, cost, upkeep, culture, advance, improvement flags,
/// other characteristics, happy faces, defense bonus, bombard defense.
const BUILDINGS: [(&str, i32, i32, i32, i32, u32, u32, i32, i32, i32); 11] = [
    ("Lantern Hall", 100, 0, 1, -1, imp::CENTER_OF_EMPIRE, 0, 0, 0, 0),
    ("Drill Yard", 40, 1, 0, -1, imp::VETERAN_GROUND_UNITS, oc::MILITARISTIC, 0, 0, 0),
    ("Grain Vault", 60, 1, 0, 3, imp::DOUBLES_CITY_GROWTH_RATE, oc::AGRICULTURAL, 0, 0, 0),
    ("Lamp Shrine", 40, 1, 2, 0, 0, oc::RELIGIOUS, 1, 0, 0),
    ("Bazaar", 80, 1, 0, 8, imp::INCREASES_LUXURY_TRADE | imp::TAX_BONUS, oc::COMMERCIAL, 0, 0, 0),
    ("Scroll House", 90, 1, 2, 4, imp::RESEARCH_BONUS, oc::SCIENTIFIC, 0, 0, 0),
    ("Palisade", 30, 0, 0, 1, 0, oc::MILITARISTIC, 0, 100, 8),
    ("Wharf", 60, 1, 0, 7, imp::MUST_BE_NEAR_WATER | imp::INCREASES_FOOD_IN_WATER, oc::COASTAL_INSTALLATION | oc::SEAFARING, 0, 0, 0),
    ("The Tide Bell", 200, 0, 3, 9, 0, oc::WONDER, 2, 0, 0),
    ("Beacon of Asra", 220, 0, 3, 7, 0, oc::WONDER, 0, 0, 0),
    ("Star Loom", 150, 0, 2, 12, imp::RESEARCH_BONUS, oc::SMALL_WONDER, 0, 0, 0),
];
const LANTERN_HALL: i32 = 0;
const LAMP_SHRINE: i32 = 3;
const PALISADE: i32 = 6;

fn buildings() -> Vec<Building> {
    BUILDINGS
        .iter()
        .map(|&(name, cost, upkeep, culture, tech, flags, other, happy, defense, bombard)| Building {
            description: s(&format!("{name}.")),
            name: s(name),
            civilopedia_entry: s(&key("BLDG", name)),
            doubles_happiness_of: -1,
            gain_in_every_city: -1,
            gain_in_every_city_on_continent: -1,
            required_improvement: -1,
            cost,
            culture,
            bombard_defense: bombard,
            defense_bonus: defense,
            maintenance: upkeep,
            happy_faces: happy,
            required_government: -1,
            spaceship_part: -1,
            required_advance: tech,
            rendered_obsolete_by: -1,
            required_resource_1: -1,
            required_resource_2: -1,
            improvement_flags: flags as i32,
            other_characteristics: other as i32,
            flavors: 1,
            revision: 4,
            unit_produced: -1,
            ..Building::default()
        })
        .collect()
}

/// Governments in the role order the engine indexes (transition, default,
/// then six more): name, prerequisite, rate cap, ruler title.
const GOVERNMENTS: [(&str, i32, i32, &str); 8] = [
    ("Interregnum", -1, 0, "Steward"),
    ("Chieftaincy", -1, 6, "Chief"),
    ("Crown Rule", 9, 7, "Sovereign"),
    ("Collective", 11, 8, "Chair"),
    ("Commonwealth", 8, 8, "Consul"),
    ("Assembly", 12, 9, "Speaker"),
    ("Iron Rule", 14, 8, "Marshal"),
    ("Liege Pact", 5, 7, "Liege"),
];

fn governments() -> Vec<Government> {
    let n = GOVERNMENTS.len();
    GOVERNMENTS
        .iter()
        .enumerate()
        .map(|(i, &(name, tech, cap, title))| Government {
            default_type: i32::from(i == 1),
            transition_type: i32::from(i == 0),
            requires_maintenance: i32::from(i != 0),
            name: s(name),
            civilopedia_entry: s(&key("GOVT", name)),
            ruler_titles: [RulerTitle { male: s(title), female: s(title) }; 4],
            ruler_title_count: 4,
            corruption_and_waste: (i as i32 % 5).min(4),
            governments_in_file: n as u32,
            vs: vec![VsGovernment::default(); n],
            hurrying_production: match i {
                0 => 0,
                1 | 3 | 6 => 2,
                _ => 1,
            },
            assimilation_chance: 5 + i as i32,
            prerequisite_tech: tech,
            rate_cap: cap,
            worker_rate: 2,
            free_units: 2,
            free_units_per_city: 2,
            cost_per_unit: 1,
            ..Government::default()
        })
        .collect()
}

/// Playable civilizations (`RACE` rows 1..): name, adjective, plural, leader,
/// title, gender (1 female), leader-clip prefix, city names, traits, flavor.
/// `make_stub_assets.py` hands leader archetypes out in the sorted order of
/// these prefixes, so they start with `Z` to sort after every stock civ and
/// leave the stock clips as they are.
#[allow(clippy::type_complexity)]
const CIVS: [(&str, &str, &str, &str, &str, i32, &str, [&str; 8], [i32; 2], u32); 4] = [
    ("Veloria", "Velorian", "Velorians", "Amaru Tessel", "Lamplord", 0, "Zv",
     ["Tessmere", "Brightwater", "Calloway", "Duskport", "Emberfall", "Fennick", "Glimmerholt", "Halden"],
     [3, 5], 1),
    ("Kethra", "Kethran", "Kethrans", "Ilsa Morrow", "Tidewarden", 1, "Zw",
     ["Saltmarrow", "Orrin", "Pellucid", "Quillby", "Ravelstone", "Sorrowfen", "Thistlecove", "Umber"],
     [7, 1], 2),
    ("Oruun", "Oruuni", "Oruuni", "Daro Vask", "Stonefather", 0, "Zx",
     ["Vask Hold", "Wendrell", "Yarrow Rise", "Zephyr Gate", "Ashgrove", "Bramblemoor", "Cinderwick", "Driftholm"],
     [0, 5], 4),
    ("Sablemark", "Sablemarker", "Sablemarkers", "Nerys Quell", "Gloamqueen", 1, "Zy",
     ["Quellhaven", "Nightmere", "Owlcrest", "Pinewatch", "Ravenmoor", "Silverbank", "Thornwick", "Vesper"],
     [1, 2], 1),
];
/// Team colour (`ntpNN.pcx`) of the barbarians and of each civ.
const COLORS: [i32; 5] = [0, 1, 3, 5, 7];

fn civilizations() -> Vec<Civilization> {
    let barbarians = Civilization {
        city_names: ["Wrack", "Gullrock", "Skerry", "Brinehold", "Shoal Camp", "Foamtooth"]
            .iter()
            .map(|n| s(n))
            .collect(),
        leader_name: s("Driftking"),
        title: s("Reaver"),
        civilopedia_entry: s("RACE_Driftfolk"),
        adjective: s("Driftfolk"),
        civilization_name: s("Driftfolk"),
        noun: s("Driftfolk"),
        default_color: COLORS[0],
        unique_color: COLORS[0],
        king_unit: Some(-1),
        conquests: Some(CivilizationConquestsExtension {
            flavors: 0,
            flavor_revision: 2,
            diplomacy_text_index: -1,
            scientific_leader_count: 1,
            scientific_leaders: vec![s("Driftking")],
        }),
        ..Civilization::default()
    };
    let mut out = vec![barbarians];
    for (i, &(name, adj, noun, leader, title, gender, clip, cities, traits, flavor)) in CIVS.iter().enumerate() {
        let row = i as i32 + 1;
        let forward = ["A", "B", "C", "D"].map(|era| s(&format!("Art/Flics/{clip}_{era}01.flc")));
        let reverse = ["A", "B", "C", "D"].map(|era| s(&format!("Art/Flics/{clip}_{era}02.flc")));
        out.push(Civilization {
            city_names: cities.iter().map(|n| s(n)).collect(),
            great_leaders: vec![s(&format!("{} the Bold", cities[0])), s(&format!("{} the Wise", cities[1]))],
            leader_name: s(leader),
            title: s(title),
            civilopedia_entry: s(&key("RACE", name)),
            adjective: s(adj),
            civilization_name: s(name),
            noun: s(noun),
            era_art: forward.into_iter().chain(reverse).collect(),
            culture_group: i as i32 % 2,
            leader_gender: gender,
            aggression: i as i32 - 1,
            civilization_index: row,
            favorite_government: 2 + i as i32,
            default_color: COLORS[row as usize],
            unique_color: COLORS[row as usize],
            traits: (1 << traits[0]) | (1 << traits[1]),
            king_unit: Some(-1),
            conquests: Some(CivilizationConquestsExtension {
                flavors: flavor,
                flavor_revision: 2,
                diplomacy_text_index: -1,
                scientific_leader_count: 1,
                scientific_leaders: vec![s(&format!("Sage of {}", cities[2]))],
            }),
            ..Civilization::default()
        });
    }
    out
}

fn general_rules() -> GeneralRules {
    GeneralRules {
        town_name: s("Hamlet"),
        city_name: s("Burg"),
        metropolis_name: s("Great Burg"),
        advanced_barbarian_unit: BLADEHAND,
        basic_barbarian_unit: CUDGELER,
        barbarian_sea_unit: REED_SKIFF,
        cities_needed_to_support_army: 4,
        chance_of_rioting: 20,
        draft_turn_penalty: 20,
        shield_cost_per_gold: 4,
        fortress_defense_bonus: 50,
        citizens_per_happy_face: 2,
        forest_value_in_shields: 10,
        hurry_shield_value_in_gold: 2,
        hurry_citizen_value_in_shields: 20,
        default_difficulty: 1,
        battle_created_unit: WARLEADER,
        build_army_unit: WAR_HOST,
        building_defense_rating: 50,
        citizen_defense_rating: 50,
        default_money_resource: -1,
        intercept_air_missions_chance: 50,
        intercept_stealth_missions_chance: 50,
        starting_treasury: 20,
        food_per_citizen: 2,
        river_defense_bonus: 25,
        hurry_sacrifice_turn_penalty: 20,
        scout_unit: WAYFINDER,
        captured_unit: DELVER,
        road_movement_rate: 3,
        start_unit_1: HOMESTEADER,
        start_unit_2: DELVER,
        wltk_min_population: 3,
        town_defense_bonus: 0,
        city_defense_bonus: 50,
        metropolis_defense_bonus: 100,
        town_max_size: 6,
        city_max_size: 12,
        metropolis_max_size: 1000,
        fortification_defense_bonus: 25,
        culture_level_names: ["Faint", "Flickering", "Steady", "Bright", "Radiant", "Blazing"]
            .iter()
            .map(|n| s(n))
            .collect(),
        culture_level_multiplier: 1000,
        border_factor: 10,
        future_tech_cost: 20,
        golden_age_duration: 20,
        max_research_time: 40,
        min_research_time: 4,
        upgrade_cost: 3,
        ..GeneralRules::default()
    }
}

fn small_tables(biq: &mut Biq) {
    let r = &mut biq.rules;
    r.eras = ERAS
        .iter()
        .map(|&(name, titles)| {
            let mut researcher_titles = [Str::default(); 5];
            for (slot, t) in researcher_titles.iter_mut().zip(titles) {
                *slot = s(t);
            }
            Era {
                name: s(name),
                civilopedia_entry: s(&key("ERAS", name)),
                researcher_titles,
                researcher_title_count: titles.len() as i32,
                ..Era::default()
            }
        })
        .collect();
    // The specialist roles in the engine's order: default, entertainer, tax
    // collector, scientist, police, engineer.
    r.citizens = [
        ("Laborer", "Laborers", -1, [0, 0, 0, 0, 0]),
        ("Minstrel", "Minstrels", -1, [2, 0, 0, 0, 0]),
        ("Tallyman", "Tallymen", 4, [0, 0, 2, 0, 0]),
        ("Sage", "Sages", 4, [0, 2, 0, 0, 0]),
        ("Warden", "Wardens", 13, [0, 0, 0, 1, 0]),
        ("Builder", "Builders", 14, [0, 0, 0, 0, 2]),
    ]
    .iter()
    .enumerate()
    .map(|(i, &(name, plural, prerequisite, [lux, sci, tax, corr, build]))| Citizen {
        default_citizen: i32::from(i == 0),
        name: s(name),
        civilopedia_entry: s(&key("CTZN", name)),
        plural_name: s(plural),
        prerequisite,
        luxuries: lux,
        research: sci,
        taxes: tax,
        corruption: corr,
        construction: build,
        ..Citizen::default()
    })
    .collect();
    r.cultures = [
        ("dazzled by", 3, 1, 5, 20, 10),
        ("warm towards", 2, 1, 10, 40, 30),
        ("even with", 1, 1, 20, 60, 50),
        ("cool towards", 1, 2, 40, 75, 65),
        ("dismissive of", 1, 3, 60, 85, 75),
        ("blind to", 1, 10, 80, 95, 85),
    ]
    .iter()
    .map(|&(opinion, num, den, propaganda, initial, continued)| CultureLevel {
        opinion: s(opinion),
        propaganda_success_percent: propaganda,
        culture_ratio_percent: (num * 100 + den / 2) / den,
        culture_ratio_denominator: den,
        culture_ratio_numerator: num,
        resistance_initial_percent: initial,
        resistance_continued_percent: continued,
        ..CultureLevel::default()
    })
    .collect();
    r.difficulties = [("Sapling", 4, 14), ("Sprout", 3, 12), ("Oak", 3, 10), ("Ironwood", 2, 9)]
        .iter()
        .map(|&(name, content, cost)| Difficulty {
            name: s(name),
            citizens_born_content: content,
            max_government_transition_time: 2,
            defensive_land_units: 1,
            offensive_land_units: 1,
            cost_factor: cost,
            ..Difficulty::default()
        })
        .collect();
    r.espionage_missions = [
        ("Seat an informant in the city.", "Seat Informant", 1, 20),
        ("Sway the townsfolk to our banner.", "Sway Hearts", 2, 80),
        ("Copy the plans of their workshops.", "Copy Plans", 2, 60),
        ("Kindle unrest among the citizens.", "Kindle Unrest", 2, 50),
    ]
    .iter()
    .map(|&(description, name, by, cost)| EspionageMission {
        description: s(description),
        name: s(name),
        civilopedia_entry: s(&key("ESPN", name)),
        performed_by: by,
        base_cost: cost,
        ..EspionageMission::default()
    })
    .collect();
    r.experience_levels = [("Green", 2, 0), ("Seasoned", 3, 0), ("Hardened", 4, 10), ("Storied", 5, 20)]
        .iter()
        .map(|&(name, hp, retreat)| ExperienceLevel {
            name: s(name),
            base_hit_points: hp,
            retreat_bonus_percent: retreat,
            ..ExperienceLevel::default()
        })
        .collect();
    r.world_sizes = [("Islet", 40, 6, 10, 150, 4), ("Isle", 60, 8, 12, 200, 6), ("Expanse", 80, 10, 14, 250, 8)]
        .iter()
        .map(|&(name, side, cities, distance, rate, civs)| WorldSize {
            optimal_city_count: cities,
            tech_rate: rate,
            name: s(name),
            height: side,
            distance_between_civs: distance,
            civ_count: civs,
            width: side,
            ..WorldSize::default()
        })
        .collect();
    biq.flavors = Some(Flavors {
        version: 1,
        flavors: ["Hearth", "Tide", "Forge"]
            .iter()
            .enumerate()
            .map(|(i, name)| Flavor {
                version: 1,
                name: s(name),
                relations: (0..3).map(|j| if i == j { 100 } else { 40 }).collect(),
            })
            .collect(),
    });
}

// ------------------------------------------------------------------ map

/// Every staggered cell `(x, y)` with `x + y` even.
fn cells() -> impl Iterator<Item = (i32, i32)> {
    (0..H).flat_map(|y| (0..W).filter(move |x| (x + y) % 2 == 0).map(move |x| (x, y)))
}

/// The eight neighbours of a cell (no wrap).
fn neighbours(x: i32, y: i32) -> impl Iterator<Item = (i32, i32)> {
    [(0, -2), (1, -1), (2, 0), (1, 1), (0, 2), (-1, 1), (-2, 0), (-1, -1)]
        .into_iter()
        .map(move |(dx, dy)| (x + dx, y + dy))
        .filter(|&(x, y)| (0..W).contains(&x) && (0..H).contains(&y))
}

/// Island centres and radii, in cells.
const ISLANDS: [(i32, i32, f32); 3] = [(12, 14, 9.5), (28, 26, 9.5), (32, 7, 3.0)];

fn is_land(x: i32, y: i32) -> bool {
    ISLANDS.iter().any(|&(cx, cy, r)| {
        // A cell is half as wide as it is tall in native units.
        let (dx, dy) = ((x - cx) as f32 / 2.0, (y - cy) as f32 / 2.0);
        let wobble = (hash(x, y, 1) % 100) as f32 / 100.0 * 2.0 - 1.0;
        (dx * dx + dy * dy).sqrt() + wobble < r / 2.0
    })
}

/// City sites, per player, the first one the capital. All on land.
const SITES: [[(i32, i32); 3]; 4] = [
    [(10, 12), (14, 18), (6, 14)],
    [(16, 10), (18, 16), (12, 8)],
    [(28, 24), (24, 28), (32, 28)],
    [(30, 22), (34, 24), (32, 8)],
];

struct Ground {
    terrain: Vec<(u8, u8)>,
    resource: Vec<i32>,
    continent: Vec<i16>,
    continents: Vec<Continent>,
}

fn idx(x: i32, y: i32) -> usize {
    index_from_coords(x, y, W) as usize
}

fn ground() -> Ground {
    let n = (W / 2 * H) as usize;
    let mut land = vec![false; n];
    for (x, y) in cells() {
        land[idx(x, y)] = is_land(x, y);
    }
    let cities: Vec<(i32, i32)> = SITES.iter().flatten().copied().collect();
    for &(x, y) in &cities {
        assert!(land[idx(x, y)], "city site ({x}, {y}) is in the water");
    }
    let mut terrain = vec![(OCEAN, OCEAN); n];
    let near = |x: i32, y: i32, dist: u32| -> bool {
        let mut frontier = vec![(x, y)];
        for _ in 0..dist {
            frontier = frontier.iter().flat_map(|&(x, y)| neighbours(x, y)).collect();
            if frontier.iter().any(|&(x, y)| land[idx(x, y)]) {
                return true;
            }
        }
        false
    };
    for (x, y) in cells() {
        let i = idx(x, y);
        terrain[i] = if land[i] {
            let base = if y < 4 || y >= H - 4 {
                TUNDRA
            } else if hash(x, y, 2) % 7 == 0 {
                PLAINS
            } else if hash(x, y, 3) % 11 == 0 {
                DESERT
            } else {
                GRASS
            };
            let roll = hash(x, y, 4) % 100;
            if cities.contains(&(x, y)) {
                (if base == TUNDRA { PLAINS } else { base }, base)
            } else if roll < 14 {
                (FOREST, base)
            } else if roll < 22 {
                (HILLS, base)
            } else if roll < 26 {
                (MOUNTAINS, base)
            } else if roll < 29 && (14..26).contains(&y) {
                (JUNGLE, GRASS)
            } else if roll < 32 {
                (MARSH, GRASS)
            } else {
                (base, base)
            }
        } else if near(x, y, 1) {
            (COAST, COAST)
        } else if near(x, y, 2) {
            (SEA, SEA)
        } else {
            (OCEAN, OCEAN)
        };
    }
    // Resources: one roll per cell against the goods its terrain allows.
    let mut resource = vec![-1; n];
    for (x, y) in cells() {
        let i = idx(x, y);
        let allowed = TERRAINS[terrain[i].0 as usize].3;
        if !allowed.is_empty() && !cities.contains(&(x, y)) && hash(x, y, 5) % 9 == 0 {
            resource[i] = allowed[hash(x, y, 6) as usize % allowed.len()];
        }
    }
    // Continents: each island one land row, then all the water one more.
    let mut continent = vec![-1i16; n];
    let mut continents = Vec::new();
    for (x, y) in cells() {
        if !land[idx(x, y)] || continent[idx(x, y)] >= 0 {
            continue;
        }
        let id = continents.len() as i16;
        let mut stack = vec![(x, y)];
        let mut count = 0;
        continent[idx(x, y)] = id;
        while let Some((x, y)) = stack.pop() {
            count += 1;
            for (nx, ny) in neighbours(x, y) {
                if land[idx(nx, ny)] && continent[idx(nx, ny)] < 0 {
                    continent[idx(nx, ny)] = id;
                    stack.push((nx, ny));
                }
            }
        }
        continents.push(Continent { in_use: 1, tile_count: count, ..Continent::default() });
    }
    let water = continents.len() as i16;
    let wet = continent.iter().filter(|&&c| c < 0).count();
    for c in continent.iter_mut().filter(|c| **c < 0) {
        *c = water;
    }
    continents.push(Continent { in_use: 0, tile_count: wet as i32, ..Continent::default() });
    Ground { terrain, resource, continent, continents }
}

fn map(biq: &mut Biq, g: &Ground) {
    biq.map.characteristics = vec![WorldCharacteristics {
        climate_selected: 1,
        climate_actual: 1,
        barbarian_activity_selected: 1,
        barbarian_activity_actual: 1,
        landform_selected: 1,
        landform_actual: 1,
        ocean_coverage_selected: 2,
        ocean_coverage_actual: 2,
        temperature_selected: 1,
        temperature_actual: 1,
        age_selected: 1,
        age_actual: 1,
        world_size_index: 0,
        ..WorldCharacteristics::default()
    }];
    biq.map.world_map = vec![WorldMap {
        resource_count: GOODS.len() as i32,
        resource_rolls: vec![0; GOODS.len()],
        land_continent_count: g.continents.iter().filter(|c| c.in_use == 1).count() as i32,
        height: H,
        start_site_radius: 2,
        number_of_players: SITES.len() as i32,
        cell_count_isqrt: ((W / 2 * H) as f64).sqrt() as i32,
        width: W,
        start_candidate_seeds: std::array::from_fn(|i| hash(i as i32, 0, 7) as i32 & 0x7FFF),
        water_level: 2,
        wrap_flags: 0,
        ..WorldMap::default()
    }];
    let capitals: Vec<(i32, i32)> = SITES.iter().map(|s| s[0]).collect();
    biq.map.tiles = cells()
        .map(|(x, y)| {
            let i = idx(x, y);
            let (id, sub) = g.terrain[i];
            Tile {
                resource: g.resource[i],
                continent_id: g.continent[i],
                water_depth_byte: Some(6),
                victory_point_location_id: Some(-1),
                ruin_id: Some(0),
                flags_0x28: Some(0),
                terrain_class: Some(((id as i32) << 12) | ((sub as i32) << 8)),
                flags_0x30: Some(if capitals.contains(&(x, y)) { feature::START_LOCATION as i32 } else { 0 }),
                flags_0x34: Some(0),
                ..Tile::default()
            }
        })
        .collect();
    biq.map.continents = g.continents.clone();
    biq.map.start_locations = capitals
        .iter()
        .enumerate()
        .map(|(i, &(x, y))| StartLocation { owner_type: 2, owner: i as i32 + 1, map_x: x, map_y: y, ..StartLocation::default() })
        .collect();
}

// ------------------------------------------------------------------ scenario

/// Advances every player starts with.
const START_TECHS: [[i32; 2]; 4] = [[0, 1], [0, 2], [1, 3], [0, 4]];

fn scenario(biq: &mut Biq) {
    let sc = &mut biq.scenario;
    sc.players = CIVS
        .iter()
        .enumerate()
        .map(|(i, civ)| Player {
            custom_civ_data: 1,
            human_player: u32::from(i == 0),
            leader_name: s(civ.3),
            starting_units: vec![StartingUnit { count: 1, unit_type: WAYFINDER }],
            gender: civ.5 as u32,
            free_techs: START_TECHS[i].to_vec(),
            difficulty: 1,
            initial_era: 0,
            starting_treasury: 30,
            government: 1,
            civilization: i as i32 + 1,
            team_color: COLORS[i + 1],
            ..Player::default()
        })
        .collect();
    let city_level = |size: i32| (size > 6) as i32 + (size > 12) as i32;
    for (p, sites) in SITES.iter().enumerate() {
        for (k, &(x, y)) in sites.iter().enumerate() {
            let size = if k == 0 { 4 } else { 2 };
            let mut built = vec![];
            if k == 0 {
                built = vec![LANTERN_HALL, PALISADE];
            } else if k == 1 {
                built = vec![LAMP_SHRINE];
            }
            sc.cities.push(City {
                has_walls: u8::from(built.contains(&PALISADE)),
                has_palace: u8::from(built.contains(&LANTERN_HALL)),
                name: s(CIVS[p].7[k]),
                owner_type: 3,
                starting_buildings: built,
                culture: 0,
                owner: p as i32,
                size,
                map_x: x,
                map_y: y,
                city_level: city_level(size),
                border_level: 0,
                use_auto_name: 0,
                ..City::default()
            });
        }
    }
    for (p, sites) in SITES.iter().enumerate() {
        let (cx, cy) = sites[0];
        for (k, (unit, (dx, dy))) in [(CUDGELER, (0, 0)), (SLINGER, (0, 0)), (DELVER, (1, 1)), (OX_RIDER, (-1, 1))].into_iter().enumerate() {
            sc.units.push(Unit {
                owner_type: 3,
                experience_level: if k == 0 { 2 } else { 1 },
                owner: p as i32,
                unit_type: unit,
                ai_strategy: -1,
                map_x: cx + dx,
                map_y: cy + dy,
                ..Unit::default()
            });
        }
    }
    sc.game = vec![Game {
        use_default_game_rules: 1,
        use_default_victory_conditions: 1,
        playable_civilizations: (1..=CIVS.len() as i32).map(|c| PlayableCiv { civilization: c, alliance: 0 }).collect(),
        start_year: -2400,
        time_scale_turns: [40, 40, 50, 60, 100, 100, 100],
        time_scale_units: [40, 25, 20, 10, 5, 2, 1],
        unknown_1b98: s("Lantern Isles"),
        disasters: civ3_biq::sections::game::DisasterSettings {
            plague_name: s("Grey Fever"),
            ..Default::default()
        },
        ..Game::default()
    }];
}

fn lantern_isles() -> (Biq, Ground) {
    let mut biq = Biq::new(Version::new(12, 8));
    let h = biq.header.as_mut().unwrap();
    h.title = s("Lantern Isles");
    h.description = s(
        "Four lamp-keeping peoples share a pair of green islands and a rock to the \
         north-east. A test scenario for open-4x: every name, number and tile here \
         was written for it.",
    );
    biq.rules.buildings = buildings();
    biq.rules.goods = goods();
    biq.rules.governments = governments();
    biq.rules.general_rules = vec![general_rules()];
    biq.rules.unit_types = unit_types();
    biq.rules.civilizations = civilizations();
    biq.rules.techs = techs();
    biq.rules.worker_jobs = worker_jobs();
    biq.rules.terrains = terrains();
    small_tables(&mut biq);
    let g = ground();
    map(&mut biq, &g);
    scenario(&mut biq);
    biq.section_order = CANONICAL_ORDER.to_vec();
    biq.storage = Storage::GAME;
    (biq, g)
}

// ------------------------------------------------------------------ save

/// The scenario twenty turns on: the same players with grown cities, a
/// fourth city each, roads round the capitals and a few more advances.
fn lantern_save(biq: &Biq, bytes: Vec<u8>, g: &Ground) -> Save {
    let mut save = Save::blank(bytes, W as u32, H as u32, SAVE_TURN, -2400 + 40 * SAVE_TURN as i32).expect("blank save");
    for (i, p) in biq.scenario.players.iter().enumerate() {
        save.set_player(i + 1, p.civilization, p.government, 30 + 17 * i as i32);
    }
    for (x, y) in cells() {
        let i = idx(x, y);
        let t = save.map.tile_mut(x as u32, y as u32).unwrap();
        t.set_terrain(g.terrain[i].0, g.terrain[i].1);
        t.set_resource(g.resource[i]);
        t.set_continent_id(g.continent[i]);
    }
    let counts = save.counts;
    let mut cities = Vec::new();
    for (p, sites) in SITES.iter().enumerate() {
        for (k, &(x, y)) in sites.iter().enumerate() {
            let c = biq.scenario.cities.iter().find(|c| (c.map_x, c.map_y) == (x, y)).unwrap();
            let built: Vec<usize> = c.starting_buildings.iter().map(|&b| b as usize).collect();
            cities.push((p, k, x, y, c.name.text().into_owned(), c.size + 2, built));
        }
        // A city founded since the scenario began: the first land cell two
        // steps from the second city that no other city crowds.
        let (x, y) = sites[1];
        let taken: Vec<(i32, i32)> = SITES.iter().flatten().copied().chain(cities.iter().map(|c| (c.2, c.3))).collect();
        let (nx, ny) = [(2, 2), (-2, 2), (2, -2), (-2, -2), (0, 4), (4, 0), (0, -4), (-4, 0)]
            .into_iter()
            .map(|(dx, dy)| (x + dx, y + dy))
            .find(|&(nx, ny)| {
                (0..W).contains(&nx)
                    && (0..H).contains(&ny)
                    && g.terrain[idx(nx, ny)].0 < COAST
                    && taken.iter().all(|&(tx, ty)| (tx - nx).abs() + (ty - ny).abs() > 2)
            })
            .expect("room for a fourth city");
        cities.push((p, 3, nx, ny, CIVS[p].7[3].to_string(), 1, vec![]));
    }
    for (id, (p, k, x, y, name, size, built)) in cities.iter().enumerate() {
        let slot = p + 1;
        save.add_city(SavCity::new(id as u32, *x as u16, *y as u16, slot as u8, name, *size as u32, built, &counts));
        if *k == 0 {
            save.players[slot].set_capital_city(id as i32);
        }
        for (nx, ny) in std::iter::once((*x, *y)).chain(neighbours(*x, *y)) {
            let t = save.map.tile_mut(nx as u32, ny as u32).unwrap();
            t.set_owner(slot as u8);
            if (nx, ny) == (*x, *y) || (*k == 0 && g.terrain[idx(nx, ny)].0 < COAST && hash(nx, ny, 8) % 2 == 0) {
                t.set_overlay_plane(civ3_biq::sections::tile::overlay::ROAD);
            }
        }
    }
    let mut id = 0;
    for (p, sites) in SITES.iter().enumerate() {
        let slot = p as u32 + 1;
        let (cx, cy) = sites[0];
        let (sx, sy) = sites[1];
        for (unit, x, y, level, order) in [
            (CUDGELER, cx, cy, 2, 1),
            (SLINGER, cx, cy, 1, 1),
            (BLADEHAND, sx, sy, 1, 1),
            (DELVER, cx + 1, cy + 1, 1, 0),
            (HOMESTEADER, sx, sy, 1, 0),
            (OX_RIDER, cx - 1, cy + 1, 1, 0),
        ] {
            save.add_unit(SavUnit::new(id, x, y, slot, unit as u32, level, 0, order));
            id += 1;
        }
    }
    // A barbarian raider off the northern rock.
    save.add_unit(SavUnit::new(id, 34, 6, 0, REED_SKIFF as u32, 1, 1, 0));
    for (p, known) in START_TECHS.iter().enumerate() {
        for &t in known.iter().chain(&[2, 5, 6]) {
            save.game.tech_known_by[t as usize] |= 1 << (p + 1);
        }
    }
    save.finish();
    let dir = civ3_biq::io::Str::<260>::new("").0;
    let file = civ3_biq::io::Str::<260>::new(BIQ_NAME).0;
    save.bic.0[4..524].copy_from_slice(&[dir, file].concat());
    save
}

fn main() {
    let out = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data"));
    let (biq, g) = lantern_isles();
    let bytes = biq.to_bytes().expect("encode BIQ");
    let back = Biq::parse(&bytes).expect("the BIQ reads back");
    assert_eq!(back.to_bytes().unwrap(), bytes, "the BIQ round-trips");
    assert_eq!(back.unmodelled_bytes(), 0);
    let biq_path = out.join(BIQ_NAME);
    std::fs::write(&biq_path, &bytes).expect("write BIQ");

    let save = lantern_save(&biq, bytes, &g);
    let sav = save.to_bytes().expect("encode SAV");
    let back = Save::parse(&sav).expect("the SAV reads back");
    assert_eq!(back.to_bytes().unwrap(), sav, "the SAV round-trips");
    let sav_path = out.join(SAV_NAME);
    std::fs::write(&sav_path, &sav).expect("write SAV");
    for p in [&biq_path, &sav_path] {
        println!("{} ({} bytes)", p.display(), std::fs::metadata(p).unwrap().len());
    }
}
