//! Worlds from Civ3's files (`docs/civ3-files.md` sections 5 and 1): the
//! map, players, cities, units, colonies and settings of a `.biq`, and the
//! parameters of a random map from `WSIZ`/`WCHR` and the command line.
//!
//! Civ3's map is *staggered* (`x + y` even, eight neighbors at `(±1, ±1)`,
//! `(±2, 0)`, `(0, ±2)`). The clone's grid is that lattice rotated 45
//! degrees: `u = (x + y) / 2`, `v = (y - x) / 2`, so a native diagonal step
//! is a king move one tile along an axis and the screen position of every
//! tile is unchanged (`tile_to_world(u, v) = (x * 64, -y * 32)`). The
//! rotated square holds the native diamond plus a margin of ocean; the cells
//! outside the diamond are ocean, and the east-west wrap of the native map is
//! not kept (the clone wraps its own `u` axis instead, which the ocean margin
//! keeps harmless).

use std::sync::OnceLock;

use civ3_biq::Biq;
use civ3_biq::owner::Owner;
use civ3_biq::sections::tile::{self as native, feature, overlay};

use crate::boot::{Boot, World};
use crate::map::{Base, Cover, GameMap, Relief, Tile};

/// Ocean cells around the native diamond.
pub const MARGIN: i32 = 2;

/// The native lattice of one scenario map embedded in the clone's grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lattice {
    /// Native `width / 2`.
    pub half_w: i32,
    /// Native height.
    pub rows: i32,
}

impl Lattice {
    /// Clone coordinates of a native cell.
    pub fn to_clone(self, x: i32, y: i32) -> (i32, i32) {
        ((x + y) / 2 + MARGIN, (y - x) / 2 + self.half_w + MARGIN)
    }

    /// Native coordinates of a clone cell (the inverse).
    pub fn to_native(self, u: i32, v: i32) -> (i32, i32) {
        let (a, b) = (u - MARGIN, v - MARGIN - self.half_w);
        (a - b, a + b)
    }
}

/// One player slot (`LEAD`).
#[derive(Clone, Debug)]
pub struct Lead {
    /// `RACE` row, `None` for *Any* / *Random*.
    pub race: Option<usize>,
    pub human: bool,
    pub gold: u32,
    /// `GOVT` row.
    pub government: Option<usize>,
    /// `TECH` rows the player starts with (a custom row's, else the race's).
    pub free_techs: Vec<i32>,
    /// `(PRTO row, how many)` for a player with no placed objects.
    pub starting_units: Vec<(usize, u32)>,
}

#[derive(Clone, Debug)]
pub struct CityRow {
    pub owner: Owner,
    pub name: String,
    /// Clone coordinates.
    pub at: (i32, i32),
    pub size: u8,
    /// `BLDG` rows.
    pub buildings: Vec<usize>,
    pub palace: bool,
    pub walls: bool,
    pub culture: i32,
}

#[derive(Clone, Debug)]
pub struct UnitRow {
    pub owner: Owner,
    /// Hit points lost.
    pub damage: i32,
    pub fortified: bool,
    /// `PRTO` row.
    pub utype: usize,
    pub at: (i32, i32),
    /// `EXPR` row.
    pub level: i32,
}

#[derive(Clone, Debug)]
pub struct ColonyRow {
    pub owner: Owner,
    pub kind: i32,
    pub at: (i32, i32),
}

#[derive(Clone, Debug)]
pub struct StartRow {
    pub owner: Owner,
    pub at: (i32, i32),
}

/// `GAME`: what ends the game and how time passes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    /// The game ends after this many turns.
    pub turn_limit: u32,
    /// 0 years, 1 months, 2 weeks.
    pub base_unit: u32,
    pub start_year: i32,
    pub start_month: i32,
    pub start_week: i32,
    pub scale_turns: [i32; 7],
    pub scale_units: [i32; 7],
    /// Domination victory is on, with these shares of land and people.
    pub domination: bool,
    pub domination_tiles: i32,
    pub domination_people: i32,
    /// Conquest victory: the last civilization standing wins.
    pub conquest: bool,
    /// Start with the whole map seen.
    pub reveal_map: bool,
}

impl Default for Settings {
    /// Conquests' own calendar and the default victory conditions.
    fn default() -> Self {
        Settings {
            turn_limit: 540,
            base_unit: 0,
            start_year: -4000,
            start_month: 1,
            start_week: 1,
            scale_turns: [25, 25, 40, 50, 100, 100, 100],
            scale_units: [50, 40, 25, 20, 10, 5, 2],
            domination: true,
            domination_tiles: 66,
            domination_people: 66,
            conquest: true,
            reveal_map: false,
        }
    }
}

impl Settings {
    /// From a `GAME` row.
    pub fn from_game(g: &civ3_biq::sections::game::Game) -> Settings {
        use civ3_biq::sections::game::flags;
        let d = Settings::default();
        let custom = g.use_default_victory_conditions == 0;
        let on = |bit: u32, default: bool| {
            if custom {
                g.rules_flags & bit != 0
            } else {
                default
            }
        };
        let share = |v: i32| if v > 0 { v } else { 66 };
        let limit = if g.time_limit_turns == 0 {
            540
        } else {
            g.time_limit_turns.clamp(1, 1000)
        };
        for (name, bit) in [
            ("space race", flags::SPACE_RACE),
            ("diplomatic", flags::DIPLOMATIC),
            ("cultural", flags::CULTURAL),
        ] {
            if custom && g.rules_flags & bit != 0 {
                bevy::log::warn!(
                    "the scenario enables the {name} victory, which open-4x does not play"
                );
            }
        }
        Settings {
            turn_limit: limit as u32,
            base_unit: g.base_unit_of_time,
            start_year: if g.start_year == 0 { 1 } else { g.start_year },
            start_month: g.start_month.clamp(1, 12),
            start_week: g.start_week.clamp(1, 52),
            scale_turns: g.time_scale_turns,
            scale_units: g.time_scale_units,
            domination: on(flags::DOMINATION, d.domination),
            domination_tiles: share(g.victory.domination_terrain_percent),
            domination_people: share(g.victory.domination_population_percent),
            conquest: on(flags::CONQUEST, d.conquest),
            reveal_map: g.reveal_entire_map != 0,
        }
    }
}

/// What a `.biq` places in the world.
#[derive(Clone, Debug)]
pub struct Scenario {
    pub lattice: Lattice,
    pub leads: Vec<Lead>,
    /// The roster index (`civs::players`) each lead plays.
    pub civs: Vec<usize>,
    pub cities: Vec<CityRow>,
    pub units: Vec<UnitRow>,
    pub colonies: Vec<ColonyRow>,
    pub starts: Vec<StartRow>,
    /// The turn the game opens on (a saved game's, else 1).
    pub turn: u32,
}

/// The globally chosen parameters of the match.
#[derive(Clone, Debug)]
pub struct Setup {
    pub seed: u64,
    /// The clone grid of a random map.
    pub w: i32,
    pub h: i32,
    /// `WSIZ` row.
    pub size: usize,
    /// Climate, barbarians, landmass, ocean, temperature, age (`WCHR`).
    pub opts: civ3mapgen::options::Options,
    /// `DIFF` row.
    pub difficulty: usize,
    pub settings: Settings,
}

impl Default for Setup {
    fn default() -> Self {
        Setup {
            seed: crate::map::MAP_SEED,
            w: crate::map::MAP_W,
            h: crate::map::MAP_H,
            size: 2,
            opts: civ3mapgen::options::Options {
                seed: 0,
                size: 2,
                climate: 1,
                barbarians: 1,
                landmass: 1,
                ocean: 1,
                temperature: 1,
                age: 1,
            },
            difficulty: 2,
            settings: Settings::default(),
        }
    }
}

struct Installed {
    setup: Setup,
    scenario: Option<Scenario>,
    map: Option<GameMap>,
}

static INSTALLED: OnceLock<Installed> = OnceLock::new();

/// The match's parameters (defaults before `install`, and in tests).
pub fn setup() -> &'static Setup {
    static DEFAULT: OnceLock<Setup> = OnceLock::new();
    INSTALLED
        .get()
        .map(|i| &i.setup)
        .unwrap_or_else(|| DEFAULT.get_or_init(Setup::default))
}

/// The scenario being played, if the file placed a world.
pub fn scenario() -> Option<&'static Scenario> {
    INSTALLED.get().and_then(|i| i.scenario.as_ref())
}

/// The map the file carries, if it has one.
pub fn file_map() -> Option<&'static GameMap> {
    INSTALLED.get().and_then(|i| i.map.as_ref())
}

/// `DIFF` row of the match.
pub fn difficulty() -> usize {
    setup().difficulty
}

/// `GAME` settings of the match.
pub fn settings() -> &'static Settings {
    &setup().settings
}

/// Barbarian activity `S`: -1 none, 0 sedentary, 1 roaming, 2 restless, 3 raging.
pub fn barbarian_activity() -> i32 {
    setup().opts.barbarians
}

/// The clone grid size: the installed map's, else the random map's.
pub fn map_dims() -> (i32, i32) {
    file_map()
        .map(|m| (m.w, m.h))
        .unwrap_or((setup().w, setup().h))
}

// ---------------------------------------------------------------- setup

fn pick(value: &str, names: &[&str]) -> Option<usize> {
    let v = value.trim();
    names
        .iter()
        .position(|n| n.eq_ignore_ascii_case(v))
        .or_else(|| v.parse::<usize>().ok().filter(|&i| i < names.len()))
}

fn bad(flag: &str, value: &str, wanted: &str) -> String {
    format!("{flag} {value}: expected {wanted}")
}

/// A `WCHR` slot from its option text; `Ok(None)` when unset.
fn slot(flag: &str, value: &Option<String>, names: &[&str]) -> Result<Option<i32>, String> {
    let Some(v) = value else { return Ok(None) };
    if v.trim().eq_ignore_ascii_case("random") {
        return Ok(Some(3));
    }
    pick(v, names)
        .map(|i| Some(i as i32))
        .ok_or_else(|| bad(flag, v, &names.join(", ")))
}

/// Resolve the options and the file's `WCHR`/`GAME` into the match's setup.
pub fn setup_from(o: &crate::cli::Options, biq: Option<&Biq>) -> Result<Setup, String> {
    use civ3mapgen::options::RawOptions;
    let mut s = Setup::default();
    s.seed = o.seed.unwrap_or(s.seed);
    // The file's world characteristics, then the command line over them.
    let wchr = biq.and_then(|b| b.map.characteristics.first());
    let mut raw = RawOptions {
        climate: wchr.map_or(1, |w| w.climate_selected),
        barbarians: wchr.map_or(1, |w| w.barbarian_activity_selected),
        landmass: wchr.map_or(1, |w| w.landform_selected),
        ocean: wchr.map_or(1, |w| w.ocean_coverage_selected),
        temperature: wchr.map_or(1, |w| w.temperature_selected),
        age: wchr.map_or(1, |w| w.age_selected),
    };
    if let Some(v) = slot("--climate", &o.climate, &["arid", "normal", "wet"])? {
        raw.climate = v;
    }
    if let Some(v) = slot("--land", &o.land, &["archipelago", "continents", "pangaea"])? {
        raw.landmass = v;
    }
    if let Some(v) = slot(
        "--temperature",
        &o.temperature,
        &["cool", "temperate", "warm"],
    )? {
        raw.temperature = v;
    }
    if let Some(v) =
        slot("--age", &o.age, &["3 billion", "4 billion", "5 billion"]).or_else(|_| {
            slot(
                "--age",
                &o.age.as_ref().map(|a| format!("{a} billion")),
                &["3 billion", "4 billion", "5 billion"],
            )
        })?
    {
        raw.age = v;
    }
    if let Some(v) = &o.water {
        raw.ocean = if v.trim().eq_ignore_ascii_case("random") {
            3
        } else if let Some(i) = pick(v, &["low", "medium", "high", "very high", "highest"]) {
            i as i32
        } else if let Some(i) = v
            .trim()
            .trim_end_matches('%')
            .parse::<i32>()
            .ok()
            .filter(|p| (50..=80).contains(p))
        {
            (i - 50) / 10
        } else {
            return Err(bad("--water", v, "low, medium, high or 0-4"));
        };
    }
    if let Some(v) = &o.barbarians {
        raw.barbarians = if v.trim().eq_ignore_ascii_case("random") {
            4
        } else if let Some(i) = pick(v, &["none", "sedentary", "roaming", "restless", "raging"]) {
            i as i32 - 1
        } else {
            return Err(bad(
                "--barbarians",
                v,
                "none, sedentary, roaming, restless, raging",
            ));
        };
    }
    // The world size: a name or index of a `WSIZ` row.
    let names = crate::ruleset::SIZE_NAMES.to_vec();
    let standard = 2.min(names.len().saturating_sub(1));
    let mut size = wchr
        .map(|w| w.world_size_index)
        .filter(|&i| i >= 0 && (i as usize) < names.len())
        .map_or(standard, |i| i as usize);
    let sized = o.size.is_some() || wchr.is_some();
    if let Some(v) = &o.size {
        size = pick(v, &names).ok_or_else(|| bad("--size", v, &names.join(", ")))?;
    }
    s.size = size;
    let (w, h) = (s.w, s.h);
    if sized && !names.is_empty() {
        // The clone's Standard world is its 80 x 60 grid; the other sizes
        // scale by their number of tiles.
        let tiles = |i: usize| {
            (crate::ruleset::SIZE_DIMS[i].0 / 2 * crate::ruleset::SIZE_DIMS[i].1).max(1) as f64
        };
        let k = (tiles(size) / tiles(standard)).sqrt();
        s.w = ((w as f64 * k).round() as i32).max(16);
        s.h = ((h as f64 * k).round() as i32).max(12);
    }
    s.opts = raw.resolve(s.seed as i32, size as i32);
    // The difficulty: a `DIFF` row by name or index.
    let rules = crate::ruleset::get();
    s.difficulty = match &o.difficulty {
        Some(v) => pick(v, &rules.difficulty_names)
            .ok_or_else(|| bad("--difficulty", v, &rules.difficulty_names.join(", ")))?,
        None => (rules.general.default_difficulty.max(0) as usize)
            .min(rules.difficulty_names.len().saturating_sub(1)),
    };
    if let Some(g) = biq.and_then(|b| b.scenario.game.first()) {
        s.settings = Settings::from_game(g);
    }
    Ok(s)
}

/// Install the match's parameters, map and world for the file in `boot`.
pub fn install(boot: &Boot) -> Result<(), String> {
    let biq: Option<&Biq> = match &boot.world {
        World::Random => None,
        World::Map(b) | World::Scenario(b) => Some(b),
        World::Saved(_) => None,
    };
    let embedded;
    let biq = if let (None, World::Saved(save)) = (biq, &boot.world) {
        embedded = save
            .embedded_biq()
            .map_err(|e| format!("embedded scenario: {e}"))?;
        Some(&embedded)
    } else {
        biq
    };
    let setup = setup_from(&boot.options, biq)?;
    let (scenario, map) = match (&boot.world, biq) {
        (World::Saved(save), _) => {
            let (map, lattice) =
                map_from_save(save, setup.seed).ok_or("the saved game's map is malformed")?;
            (Some(scenario_from_save(save, lattice)), Some(map))
        }
        (World::Map(_) | World::Scenario(_), Some(b)) => {
            let seed = setup.seed;
            let (map, lattice) = map_from_biq(b, seed).ok_or("the file's map is malformed")?;
            let sc = objects_from_biq(b, lattice, matches!(boot.world, World::Scenario(_)));
            (Some(sc), Some(map))
        }
        _ => (None, None),
    };
    // Tests and a second call keep the first.
    let _ = INSTALLED.set(Installed {
        setup,
        scenario,
        map,
    });
    Ok(())
}

// ------------------------------------------------------------------ map

/// A native tile in the clone's terms. Terrains the clone does not model
/// (Marsh, Volcano) become the closest it has.
fn tile_of(
    t: &native::Tile,
    numbering: civ3_biq::sections::terr::TerrainNumbering,
    at: (i32, i32),
) -> Tile {
    let id = numbering.to_current(t.terrain_id());
    let sub = numbering.to_current(t.terrain_sub_class());
    let under = |s: u8| match s {
        0 | 4 => Base::Desert,
        1 => Base::Plains,
        3 => Base::Tundra,
        _ => Base::Grassland,
    };
    let pine = t.has_feature(feature::PINE_FOREST);
    let (base, relief, cover) = match id {
        0 | 4 => (Base::Desert, Relief::Flat, Cover::Bare),
        1 => (Base::Plains, Relief::Flat, Cover::Bare),
        2 => (Base::Grassland, Relief::Flat, Cover::Bare),
        3 => (Base::Tundra, Relief::Flat, Cover::Bare),
        5 => (under(sub), Relief::Hill, Cover::Bare),
        6 | 10 => (under(sub), Relief::Mountain, Cover::Bare),
        7 => (
            under(sub),
            Relief::Flat,
            if pine { Cover::Pine } else { Cover::Forest },
        ),
        8 => (under(sub), Relief::Flat, Cover::Jungle),
        9 => (Base::Grassland, Relief::Flat, Cover::Bare),
        11 => (Base::Coast, Relief::Flat, Cover::Bare),
        12 => (Base::Sea, Relief::Flat, Cover::Bare),
        _ => (Base::Ocean, Relief::Flat, Cover::Bare),
    };
    let has = |bit| t.has_overlay(bit);
    Tile {
        base,
        relief,
        cover,
        variant: ((at.0 * 31 + at.1 * 17).rem_euclid(3)) as u8,
        seen: false,
        visible: false,
        hut: has(overlay::GOODY_HUT),
        camp: has(overlay::BARBARIAN_CAMP),
        resource: t
            .resource_index()
            .and_then(|r| crate::realm::good_id(r as i32)),
        road: has(overlay::ROAD) || has(overlay::RAILROAD),
        irrigation: has(overlay::IRRIGATION),
        // The clone keeps the four edge directions (NE, SE, SW, NW).
        river: t.river_connection_mask & 0xAA,
        mine: has(overlay::MINE),
        site: None,
        fortress: has(overlay::FORTRESS),
        barricade: has(overlay::BARRICADE),
        forest_harvested: false,
        owner: None,
    }
}

fn ocean_tile() -> Tile {
    Tile {
        base: Base::Ocean,
        relief: Relief::Flat,
        cover: Cover::Bare,
        variant: 0,
        seen: false,
        visible: false,
        hut: false,
        camp: false,
        resource: None,
        road: false,
        irrigation: false,
        river: 0,
        mine: false,
        site: None,
        fortress: false,
        barricade: false,
        forest_harvested: false,
        owner: None,
    }
}

/// The file's map as a `GameMap`, and where the native lattice sits in it.
pub fn map_from_biq(biq: &Biq, seed: u64) -> Option<(GameMap, Lattice)> {
    let view = biq.map_view()?;
    Some(embed(
        view.width,
        view.height,
        view.numbering,
        view.iter(),
        seed,
    ))
}

/// The native cells, laid on the clone's grid with an ocean margin.
fn embed<T: std::borrow::Borrow<native::Tile>>(
    width: i32,
    height: i32,
    numbering: civ3_biq::sections::terr::TerrainNumbering,
    cells: impl Iterator<Item = ((i32, i32), T)>,
    seed: u64,
) -> (GameMap, Lattice) {
    let lattice = Lattice {
        half_w: width / 2,
        rows: height,
    };
    let side = (width + height) / 2 + 2 * MARGIN;
    let mut map = GameMap {
        w: side,
        h: side,
        tiles: vec![ocean_tile(); (side * side) as usize],
        start: (side / 2, side / 2),
        seed,
    };
    for ((x, y), t) in cells {
        let (u, v) = lattice.to_clone(x, y);
        let i = map.idx(u, v);
        map.tiles[i] = tile_of(t.borrow(), numbering, (x, y));
    }
    map.coast_shores();
    (map, lattice)
}

/// A saved game's map. Saves number terrains the way the game holds them.
pub fn map_from_save(save: &civ3_biq::Save, seed: u64) -> Option<(GameMap, Lattice)> {
    let (w, h) = (save.map.width() as i32, save.map.height() as i32);
    if w < 2 || h < 1 || save.map.tiles.len() != save.map.cell_count() {
        return None;
    }
    let cells = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .filter(|(x, y)| (x + y) % 2 == 0)
        .filter_map(|(x, y)| {
            let t = save.map.tile(x as u32, y as u32)?;
            Some((
                (x, y),
                native::Tile {
                    river_connection_mask: t.river_connection_mask(),
                    owner: t.owner(),
                    resource: t.resource(),
                    water_depth_byte: Some(6),
                    flags_0x28: Some(t.overlay_plane() as i32),
                    terrain_class: Some(t.terrain_word() as i32),
                    flags_0x30: Some(t.feature_plane() as i32),
                    ..native::Tile::default()
                },
            ))
        });
    Some(embed(
        w,
        h,
        civ3_biq::sections::terr::TerrainNumbering::Current,
        cells,
        seed,
    ))
}

// -------------------------------------------------------------- objects

/// Roster index of a `RACE` row.
pub fn roster_of_race(row: i32) -> Option<usize> {
    crate::ruleset::RACE_ROSTER
        .iter()
        .position(|r| r.race as i32 == row)
}

/// Each lead's roster index: its race, else the next one nobody plays.
fn assign_civs(leads: &[Lead]) -> Vec<usize> {
    let mut civs: Vec<Option<usize>> = leads
        .iter()
        .map(|l| l.race.and_then(|r| roster_of_race(r as i32)))
        .collect();
    for i in 0..civs.len() {
        if civs[..i].contains(&civs[i]) && civs[i].is_some() {
            civs[i] = None;
        }
    }
    let taken = civs.clone();
    let mut free = (0..crate::ruleset::CIV_ROSTER.len()).filter(|r| !taken.contains(&Some(*r)));
    civs.into_iter()
        .map(|c| c.or_else(|| free.next()).unwrap_or(0))
        .collect()
}

fn objects_from_biq(biq: &Biq, lattice: Lattice, placed: bool) -> Scenario {
    use civ3_biq::sections::lead::{CIV_ANY, CIV_RANDOM};
    let at = |x: i32, y: i32| lattice.to_clone(x, y);
    let leads: Vec<Lead> = if placed {
        biq.scenario
            .players
            .iter()
            .take(crate::civs::MAX_CIVS)
            .map(|p| Lead {
                race: (p.civilization != CIV_ANY && p.civilization != CIV_RANDOM)
                    .then(|| p.civilization)
                    .filter(|&r| r >= 0)
                    .map(|r| r as usize),
                human: p.human_player != 0,
                gold: p.starting_treasury.max(0) as u32,
                government: usize::try_from(p.government).ok(),
                free_techs: if p.custom_civ_data != 0 {
                    p.free_techs.clone()
                } else {
                    vec![]
                },
                starting_units: p
                    .starting_units
                    .iter()
                    .filter(|u| u.unit_type >= 0)
                    .map(|u| (u.unit_type as usize, u.count.max(1) as u32))
                    .collect(),
            })
            .collect()
    } else {
        vec![]
    };
    let nobody = |o: &Owner| matches!(o, Owner::Nobody);
    let cities = biq
        .scenario
        .cities
        .iter()
        .filter_map(|c| {
            let owner = Owner::from_raw(c.owner_type, c.owner)?;
            (placed && !nobody(&owner)).then(|| CityRow {
                owner,
                name: c.name.text().to_string(),
                at: at(c.map_x, c.map_y),
                size: c.size.clamp(1, 255) as u8,
                buildings: c
                    .starting_buildings
                    .iter()
                    .filter_map(|&b| usize::try_from(b).ok())
                    .collect(),
                palace: c.has_palace != 0,
                walls: c.has_walls != 0,
                culture: c.culture,
            })
        })
        .collect();
    let units = biq
        .scenario
        .units
        .iter()
        .filter_map(|u| {
            let owner = Owner::from_raw(u.owner_type, u.owner)?;
            (placed && !nobody(&owner) && u.unit_type >= 0).then(|| UnitRow {
                owner,
                damage: 0,
                fortified: false,
                utype: u.unit_type as usize,
                at: at(u.map_x, u.map_y),
                level: u.experience_level,
            })
        })
        .collect();
    let colonies = biq
        .scenario
        .colonies
        .iter()
        .filter_map(|c| {
            let owner = Owner::from_raw(c.owner_type, c.owner)?;
            (placed && !nobody(&owner)).then(|| ColonyRow {
                owner,
                kind: c.kind,
                at: at(c.map_x, c.map_y),
            })
        })
        .collect();
    let starts = biq
        .map
        .start_locations
        .iter()
        .filter_map(|s| {
            let owner = Owner::from_raw(s.owner_type, s.owner)?;
            (!nobody(&owner)).then(|| StartRow {
                owner,
                at: at(s.map_x, s.map_y),
            })
        })
        .collect();
    Scenario {
        lattice,
        civs: assign_civs(&leads),
        leads,
        cities,
        units,
        colonies,
        starts,
        turn: 1,
    }
}

/// Save slots playing civs, in chair order: slot 0 is the barbarians.
pub fn save_used_slots(save: &civ3_biq::Save) -> Vec<usize> {
    (1..save.players.len())
        .filter(|&k| save.players[k].in_use() && save.players[k].race() >= 0)
        .take(crate::civs::MAX_CIVS)
        .collect()
}

/// The players, cities and units of a saved game. Slot 0 of a save is the
/// barbarians; the other used slots are the civs, in order. What the save
/// keeps undecoded (research, production, attitudes) starts at its default.
pub fn scenario_from_save(save: &civ3_biq::Save, lattice: Lattice) -> Scenario {
    use civ3_biq::owner::Owner;
    let used: Vec<usize> = save_used_slots(save);
    let ours = |k: u32| -> Option<Owner> {
        if k == 0 {
            Some(Owner::BarbarianTribe(0))
        } else {
            used.iter()
                .position(|&u| u as u32 == k)
                .map(|i| Owner::Player(i as i32))
        }
    };
    let leads: Vec<Lead> = used
        .iter()
        .map(|&k| {
            let p = &save.players[k];
            Lead {
                race: usize::try_from(p.race()).ok(),
                human: false,
                gold: p.gold().max(0) as u32,
                government: usize::try_from(p.government()).ok(),
                free_techs: save
                    .game
                    .tech_known_by
                    .iter()
                    .enumerate()
                    .filter(|(_, bits)| *bits >> k & 1 != 0)
                    .map(|(t, _)| t as i32)
                    .collect(),
                starting_units: vec![],
            }
        })
        .collect();
    let at = |x: i32, y: i32| lattice.to_clone(x, y);
    let buildings = crate::roster::bldg_count();
    let cities = save
        .cities
        .iter()
        .filter_map(|c| {
            let owner = ours(c.owner() as u32)?;
            (c.owner() != 0).then(|| CityRow {
                owner,
                name: c.name(),
                at: at(c.x() as i32, c.y() as i32),
                size: c.size().clamp(1, 255) as u8,
                // The improvement bit set: bit `i` is `BLDG` row `i`.
                buildings: (0..buildings.min(c.bitm.0.len() * 8))
                    .filter(|&i| c.bitm.0[i / 8] >> (i % 8) & 1 != 0)
                    .collect(),
                palace: false,
                walls: false,
                culture: 0,
            })
        })
        .collect();
    let units = save
        .units
        .iter()
        .filter_map(|u| {
            let owner = ours(u.owner())?;
            Some(UnitRow {
                owner,
                damage: u.damage() as i32,
                fortified: u.order() == 1,
                utype: u.unit_type() as usize,
                at: at(u.x(), u.y()),
                level: u.experience_level() as i32,
            })
        })
        .collect();
    Scenario {
        lattice,
        civs: assign_civs(&leads),
        leads,
        cities,
        units,
        colonies: vec![],
        starts: vec![],
        turn: save.game.turn().max(1),
    }
}

/// The turn the match opens on.
pub fn start_turn() -> u32 {
    scenario().map(|s| s.turn).unwrap_or(1)
}

impl Scenario {
    /// The slot an owner plays: the lead itself, the lead playing a race, or
    /// the barbarians.
    pub fn slot_of(&self, owner: Owner) -> Option<usize> {
        match owner {
            Owner::Player(i) => usize::try_from(i).ok().filter(|&i| i < self.leads.len()),
            Owner::Civilization(race) => {
                let r = roster_of_race(race)?;
                self.civs.iter().position(|&c| c == r)
            }
            Owner::BarbarianTribe(_) => Some(crate::civs::BARBARIANS),
            Owner::Nobody => None,
        }
    }

    /// Who sits at the keyboard: the lead of `--civ`, else every lead marked
    /// human, else the first.
    pub fn humans(&self, civ: Option<&str>) -> Vec<usize> {
        if let Some(name) = civ
            && let Some(r) = crate::civs::roster_index_named(name)
            && let Some(slot) = self.civs.iter().position(|&c| c == r)
        {
            return vec![slot];
        }
        let marked: Vec<usize> = self
            .leads
            .iter()
            .enumerate()
            .filter(|(_, l)| l.human)
            .map(|(i, _)| i)
            .collect();
        if marked.is_empty() { vec![0] } else { marked }
    }

    /// Where a slot starts when it has nothing placed: its `SLOC`, else its
    /// first city or unit.
    pub fn start_of(&self, slot: usize) -> Option<(i32, i32)> {
        self.starts
            .iter()
            .find(|s| self.slot_of(s.owner) == Some(slot))
            .map(|s| s.at)
            .or_else(|| {
                self.cities
                    .iter()
                    .find(|c| self.slot_of(c.owner) == Some(slot))
                    .map(|c| c.at)
            })
            .or_else(|| {
                self.units
                    .iter()
                    .find(|u| self.slot_of(u.owner) == Some(slot))
                    .map(|u| u.at)
            })
    }

    /// Whether the slot has a city or a unit on the map.
    pub fn has_objects(&self, slot: usize) -> bool {
        self.cities
            .iter()
            .any(|c| self.slot_of(c.owner) == Some(slot))
            || self
                .units
                .iter()
                .any(|u| self.slot_of(u.owner) == Some(slot))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scenario_file(name: &str) -> Option<Biq> {
        let root =
            std::env::var("CIV3_DIR").unwrap_or_else(|_| crate::cli::DEFAULT_CIV3_DIR.into());
        let path = crate::install::resolve_in(
            std::path::Path::new(&root),
            &format!("Conquests/Conquests/{name}"),
        )?;
        Biq::read_file(path).ok()
    }

    #[test]
    fn the_lattice_round_trips_and_keeps_neighbors_adjacent() {
        let l = Lattice {
            half_w: 50,
            rows: 40,
        };
        for (x, y) in [(0, 0), (1, 1), (50, 20), (98, 40), (2, 98)] {
            let (u, v) = l.to_clone(x, y);
            assert_eq!(l.to_native(u, v), (x, y));
            for (dx, dy) in civ3_biq::sections::tile::river_connection::DELTAS {
                let (nu, nv) = l.to_clone(x + dx, y + dy);
                assert_eq!(
                    (nu - u).abs().max((nv - v).abs()),
                    1,
                    "native step {dx},{dy} is one king move"
                );
            }
        }
    }

    #[test]
    fn a_scenarios_map_players_and_objects_come_across() {
        let Some(biq) = scenario_file("3 Fall of Rome.biq") else {
            return;
        };
        let (map, lattice) = map_from_biq(&biq, 1).expect("the scenario has a map");
        let view = biq.map_view().unwrap();
        // Every native tile lands on its own cell, water stays water.
        let mut land = 0;
        for ((x, y), t) in view.iter() {
            let (u, v) = lattice.to_clone(x, y);
            assert_eq!(map.is_land(u, v), !view.is_water(t), "tile {x},{y}");
            land += map.is_land(u, v) as usize;
        }
        assert!(land > 100);
        let sc = objects_from_biq(&biq, lattice, true);
        assert_eq!(
            sc.leads.len(),
            biq.scenario.players.len().min(crate::civs::MAX_CIVS)
        );
        assert_eq!(sc.civs.len(), sc.leads.len());
        let mut distinct = sc.civs.clone();
        distinct.sort();
        distinct.dedup();
        assert_eq!(
            distinct.len(),
            sc.civs.len(),
            "every lead plays a different civilization"
        );
        assert!(!sc.cities.is_empty() && !sc.units.is_empty());
        for c in &sc.cities {
            assert!(map.is_land(c.at.0, c.at.1), "{} stands on land", c.name);
            assert!(sc.slot_of(c.owner).is_some(), "{} has an owner", c.name);
        }
        assert!(!sc.humans(None).is_empty());
    }

    #[test]
    fn the_command_line_sets_the_random_map() {
        let parse = |args: &[&str]| {
            crate::cli::parse(
                args.iter().copied(),
                &std::collections::HashMap::<&str, &str>::new(),
            )
            .unwrap()
        };
        let s = setup_from(&parse(&[]), None).unwrap();
        assert_eq!((s.w, s.h, s.difficulty), (80, 60, 2));
        assert_eq!(
            (
                s.opts.climate,
                s.opts.landmass,
                s.opts.ocean,
                s.opts.temperature,
                s.opts.age,
                s.opts.barbarians
            ),
            (1, 1, 1, 1, 1, 1)
        );
        let o = parse(&[
            "--size",
            "Huge",
            "--land",
            "pangaea",
            "--water",
            "high",
            "--climate",
            "arid",
            "--temperature",
            "warm",
            "--age",
            "3",
            "--barbarians",
            "raging",
            "--difficulty",
            "deity",
            "--seed",
            "7",
        ]);
        let s = setup_from(&o, None).unwrap();
        assert!(
            s.w > 80 && s.h > 60,
            "Huge is bigger than Standard: {}x{}",
            s.w,
            s.h
        );
        assert_eq!(
            (
                s.opts.landmass,
                s.opts.ocean,
                s.opts.climate,
                s.opts.temperature,
                s.opts.age,
                s.opts.barbarians
            ),
            (2, 2, 0, 2, 0, 3)
        );
        assert_eq!(s.seed, 7);
        assert_eq!(
            crate::ruleset::get().difficulty_names[s.difficulty],
            "Deity"
        );
        assert!(setup_from(&parse(&["--size", "Gigantic"]), None).is_err());
        assert!(setup_from(&parse(&["--land", "dry"]), None).is_err());
    }

    #[test]
    fn a_games_settings_come_from_its_game_row() {
        let Some(biq) = scenario_file("9 WWII in the Pacific.biq") else {
            return;
        };
        let s = Settings::from_game(biq.scenario.game.first().unwrap());
        assert_eq!(s.base_unit, 1, "the Pacific counts months");
        assert_eq!(s.start_year, 1941);
        assert!(s.turn_limit >= 1 && s.turn_limit <= 1000);
    }
}
