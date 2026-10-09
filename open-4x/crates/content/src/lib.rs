//! Versioned, portable JSON content. Asset identifiers are pack-relative paths.
//!
//! A *pack* (`pack.json`) defines rules, art, and the campaign script. A *scenario*
//! (see [`fourx_sim::Scenario`]) defines a map, nations, and a start date. A pack lists the
//! scenario files it ships and names one as the default.
pub mod animation;
pub mod combat;
pub mod scenario;

use fourx_sim::{Domain, Flavor, Rules, Scenario, UnitDef};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Component, Path};

pub const PACK_FORMAT: u32 = 4;
pub const BASE_JSON: &str = include_str!("../../../assets/packs/base/pack.json");
pub const BASE_SCRIPT: &str = include_str!("../../../assets/packs/base/scripts/campaign.lua");
/// Scenario files embedded in the binary, keyed by their pack-relative path.
pub const BASE_SCENARIOS: &[(&str, &str)] = &[
    (
        "scenarios/world-1876.json",
        include_str!("../../../assets/packs/base/scenarios/world-1876.json"),
    ),
    (
        "scenarios/dawn-straits.json",
        include_str!("../../../assets/packs/base/scenarios/dawn-straits.json"),
    ),
    (
        "scenarios/terrain-study.json",
        include_str!("../../../assets/packs/base/scenarios/terrain-study.json"),
    ),
];
const MAX_SCENARIO_BYTES: u64 = 32 * 1024 * 1024;
/// Most land units one ship may carry.
pub const MAX_CAPACITY: u32 = 16;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Visuals {
    /// Ground sheet of the dual-grid terrain: grassland, plains, desert, tundra (16 × 16 cells,
    /// then tonal variants of the pure ones).
    pub terrain: String,
    /// Water sheet painted over the ground: shoreline, surf, and the depth of coast, sea, ocean.
    pub water: String,
    /// River sheet, 16 branch masks × meander variants, drawn over ground and water.
    pub rivers: String,
    pub forest: String,
    pub mountain: String,
    /// Named cover and relief sprites. Relief names accept `_dry` or `_cold`, followed
    /// by `_forest` or `_jungle` for combined relief/vegetation art. Missing combined
    /// sprites use separate relief and cover; missing basic sprites use forest/mountain.
    #[serde(default)]
    pub overlays: BTreeMap<String, String>,
    /// City sprites by [`fourx_sim::Flavor`] key (`western`, `east_asian`, ...): every flavor
    /// needs one, so a nation's cities always have an architecture to be drawn in.
    pub cities: BTreeMap<String, String>,
    /// Fog of war: a 9 x 9 sheet of tile diamonds, black with the fog's opacity in the alpha
    /// channel. Cell `(3*W + N, 3*S + E)` holds a tile whose north, east, south and west
    /// vertices are each 0 never seen, 1 remembered, or 2 in sight.
    pub fog: String,
    /// Culture borders: four tile diamonds side by side, each with a dashed line along one
    /// edge, in the order up-right, down-right, down-left, up-left. The client tints them with
    /// the nation's colour.
    pub borders: String,
    /// Optional surface-projected borders keyed by the relief sprite name. Each sheet
    /// has four 256x224 cells (or uniformly scaled equivalents) in the same edge order
    /// as `borders`, anchored exactly like its matching relief. Omission uses flat borders.
    #[serde(default)]
    pub relief_borders: BTreeMap<String, String>,
    /// Overlay drawn on tiles with a mine.
    pub mine: String,
    /// Overlay drawn on tiles with a farm.
    pub farm: String,
    pub turn_sound: String,
    /// How fights look and sound; every part is optional.
    #[serde(default)]
    pub combat: combat::CombatVisuals,
    /// Frame-by-frame clips by unit design id (idle, run, attack, victory, death, each in
    /// eight facings). A design left out is drawn from its single sprite.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub units: BTreeMap<String, animation::UnitAnimation>,
}

impl Visuals {
    /// Every asset path the visuals name.
    pub fn paths(&self) -> impl Iterator<Item = &String> {
        [
            &self.terrain,
            &self.water,
            &self.rivers,
            &self.forest,
            &self.mountain,
            &self.fog,
            &self.borders,
            &self.mine,
            &self.farm,
            &self.turn_sound,
        ]
        .into_iter()
        .chain(self.overlays.values())
        .chain(self.relief_borders.values())
        .chain(self.cities.values())
        .chain(self.combat.paths())
        .chain(animation::paths(&self.units))
    }
}

/// A scenario file shipped by a pack.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ScenarioRef {
    pub id: String,
    pub path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pack {
    pub format: u32,
    pub id: String,
    pub name: String,
    pub rules: Rules,
    pub script: String,
    pub visuals: Visuals,
    pub scenarios: Vec<ScenarioRef>,
    /// The scenario started when none is requested.
    pub default_scenario: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ContentError {
    #[error("Invalid content: {0}")]
    Invalid(String),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
pub fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && !path.contains(':')
        && !path.contains('\0')
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
        && !path
            .split('/')
            .any(|c| c == "." || c == ".." || c.is_empty())
}
impl Pack {
    pub fn parse(json: &str) -> Result<Self, ContentError> {
        let pack: Self = serde_json::from_str(json)?;
        pack.validate()?;
        Ok(pack)
    }
    pub fn base() -> Self {
        Self::parse(BASE_JSON).expect("bundled content is validated by tests")
    }
    pub fn validate(&self) -> Result<(), ContentError> {
        let invalid = |s: &str| ContentError::Invalid(s.into());
        if self.format != PACK_FORMAT {
            return Err(ContentError::Invalid(format!(
                "Unsupported pack format {} (expected {PACK_FORMAT})",
                self.format
            )));
        }
        if self.id.is_empty() || self.id.len() > 64 {
            return Err(invalid("Invalid pack id"));
        }
        if self.rules.units.is_empty() || self.rules.units.len() > 128 {
            return Err(invalid("A pack needs 1–128 unit designs"));
        }
        if !(1..=1_000_000).contains(&self.rules.victory_industry)
            || !(1..=100_000).contains(&self.rules.research_cost)
            || !(0..=1_000_000).contains(&self.rules.starting_gold)
        {
            return Err(invalid("Rules values out of range"));
        }
        let land = |u: &&UnitDef| u.domain == Domain::Land;
        if !self.rules.units.values().filter(land).any(|u| u.settler)
            || !self
                .rules
                .units
                .values()
                .filter(land)
                .any(|u| u.attack > 0 && u.defense > 0)
        {
            return Err(invalid(
                "A pack needs land pioneers and land units that can attack and defend",
            ));
        }
        for (key, unit) in &self.rules.units {
            let sea = unit.domain == Domain::Sea;
            if key != &unit.id
                || !scenario::valid_identifier(key)
                || unit.name.trim().is_empty()
                || unit.name.chars().count() > 48
                || !safe_path(&unit.sprite)
                || !(1..=10_000).contains(&unit.cost)
                || !(0..=100).contains(&unit.attack)
                || !(0..=100).contains(&unit.defense)
                || !(0..=100).contains(&unit.bombard)
                || !(0..=8).contains(&unit.range)
                || !(0..=10).contains(&unit.rate_of_fire)
                || !(0..=10).contains(&unit.hp)
                || !(1..=10).contains(&unit.moves)
                || unit.work > 20
                || (unit.bombard > 0) != (unit.range > 0 && unit.rate_of_fire > 0)
                || (unit.bombard == 0 && (unit.range > 0 || unit.rate_of_fire > 0))
                || unit.blitz && unit.attack == 0
                || sea && (unit.settler || unit.work > 0)
                || unit.capacity > MAX_CAPACITY
                || !sea && unit.capacity > 0
                || (unit.lethal_land || unit.lethal_sea) && unit.bombard == 0
            {
                return Err(ContentError::Invalid(format!(
                    "Invalid unit design {key:?}: check strengths, movement, bombard, lethality, capacity (ships only), work, and sprite path"
                )));
            }
        }
        let flavors: std::collections::BTreeSet<&str> =
            Flavor::ALL.iter().map(|f| f.key()).collect();
        if self
            .visuals
            .cities
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>()
            != flavors
        {
            return Err(invalid(
                "visuals.cities needs exactly one city sprite for each flavor",
            ));
        }
        for path in std::iter::once(&self.script).chain(self.visuals.paths()) {
            if !safe_path(path) {
                return Err(invalid("Asset paths must stay inside the pack"));
            }
        }
        self.visuals.combat.validate(&self.rules)?;
        animation::validate(&self.visuals.units, &self.rules)?;
        if self.scenarios.is_empty() || self.scenarios.len() > 64 {
            return Err(invalid("A pack needs 1–64 scenarios"));
        }
        let mut seen = std::collections::BTreeSet::new();
        for entry in &self.scenarios {
            if !scenario::valid_identifier(&entry.id)
                || !seen.insert(entry.id.as_str())
                || !safe_path(&entry.path)
            {
                return Err(ContentError::Invalid(format!(
                    "Scenario entry {:?} has an invalid, duplicate, or unsafe id/path",
                    entry.id
                )));
            }
        }
        if !seen.contains(self.default_scenario.as_str()) {
            return Err(invalid("default_scenario is not a listed scenario"));
        }
        Ok(())
    }
}

/// Everything loaded for a game: the pack, its campaign script, and each scenario document.
pub struct Content {
    pub pack: Pack,
    pub script: String,
    pub scenarios: BTreeMap<String, Scenario>,
}
impl Content {
    pub fn base() -> Self {
        let pack = Pack::base();
        let scenarios = pack
            .scenarios
            .iter()
            .map(|entry| {
                let json = BASE_SCENARIOS
                    .iter()
                    .find(|(path, _)| *path == entry.path)
                    .map(|(_, json)| *json)
                    .expect("every bundled scenario is embedded");
                let scenario: Scenario =
                    serde_json::from_str(json).expect("bundled scenarios are validated by tests");
                (entry.id.clone(), scenario)
            })
            .collect();
        Self {
            pack,
            script: BASE_SCRIPT.into(),
            scenarios,
        }
    }
    /// Look up a scenario by ID, or the pack's default when `id` is `None`.
    pub fn scenario(&self, id: Option<&str>) -> Result<&Scenario, ContentError> {
        let id = id.unwrap_or(&self.pack.default_scenario);
        self.scenarios.get(id).ok_or_else(|| {
            ContentError::Invalid(format!(
                "Unknown scenario {id:?}; available: {}",
                self.scenarios
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })
    }
    /// Check the pack and every scenario against its rules.
    pub fn validate(&self) -> Result<(), ContentError> {
        self.pack.validate()?;
        for entry in &self.pack.scenarios {
            let scenario = self.scenarios.get(&entry.id).ok_or_else(|| {
                ContentError::Invalid(format!("Scenario {:?} was not loaded", entry.id))
            })?;
            if scenario.id != entry.id {
                return Err(ContentError::Invalid(format!(
                    "{} declares id {:?}, but pack.json lists it as {:?}",
                    entry.path, scenario.id, entry.id
                )));
            }
            scenario::validate_scenario(scenario, &self.pack.rules)?;
        }
        Ok(())
    }
}

/// A complete pack replaces the base definitions. Explicit ordering keeps mod loads reproducible.
/// Canonicalize every asset to reject symlinks that escape the pack directory.
pub fn load_directory(directory: &Path) -> Result<Content, ContentError> {
    let root = directory.canonicalize()?;
    let pack = Pack::parse(&std::fs::read_to_string(root.join("pack.json"))?)?;
    let inside = |relative: &str| -> Result<std::path::PathBuf, ContentError> {
        let path = root.join(relative).canonicalize()?;
        if !path.starts_with(&root) || !path.is_file() {
            return Err(ContentError::Invalid(
                "Asset escapes pack directory or is not a file".into(),
            ));
        }
        Ok(path)
    };
    for relative in std::iter::once(&pack.script)
        .chain(pack.visuals.paths())
        .chain(pack.rules.units.values().map(|u| &u.sprite))
    {
        inside(relative)?;
    }
    let script = std::fs::read_to_string(root.join(&pack.script))?;
    if script.len() > 256 * 1024 {
        return Err(ContentError::Invalid("Script exceeds 256 KiB".into()));
    }
    let mut scenarios = BTreeMap::new();
    for entry in &pack.scenarios {
        let path = inside(&entry.path)?;
        if path.metadata()?.len() > MAX_SCENARIO_BYTES {
            return Err(ContentError::Invalid(format!(
                "{} exceeds {} MiB",
                entry.path,
                MAX_SCENARIO_BYTES >> 20
            )));
        }
        let scenario: Scenario = serde_json::from_str(&std::fs::read_to_string(path)?)
            .map_err(|e| ContentError::Invalid(format!("{}: {e}", entry.path)))?;
        scenarios.insert(entry.id.clone(), scenario);
    }
    let content = Content {
        pack,
        script,
        scenarios,
    };
    content.validate()?;
    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fourx_sim::{Game, terrain::Coord};

    fn dawn() -> Scenario {
        Content::base()
            .scenario(Some("dawn-straits"))
            .unwrap()
            .clone()
    }

    #[test]
    fn base_is_valid() {
        Content::base().validate().unwrap();
        Pack::base().validate().unwrap();
    }
    #[test]
    fn traversal_is_rejected() {
        for p in [
            "../x",
            "/x",
            "x/../y",
            "x\\y",
            "C:/x",
            "./x",
            "x//y",
            "http://host/x",
        ] {
            assert!(!safe_path(p), "{p}");
        }
        assert!(safe_path("sprites/ship.png"));
        let mut pack = Pack::base();
        pack.scenarios[0].path = "../outside.json".into();
        assert!(pack.validate().is_err());
        let mut pack = Pack::base();
        pack.visuals
            .relief_borders
            .insert("mountain".into(), "../outside.png".into());
        assert!(pack.validate().is_err());
    }
    #[test]
    fn surface_borders_are_optional_and_the_bundled_assets_all_exist() {
        let mut json: serde_json::Value = serde_json::from_str(BASE_JSON).unwrap();
        json["visuals"]
            .as_object_mut()
            .unwrap()
            .remove("relief_borders");
        let pack = Pack::parse(&json.to_string()).unwrap();
        assert!(pack.visuals.relief_borders.is_empty());
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/packs/base");
        load_directory(&root).unwrap().validate().unwrap();
    }
    #[test]
    fn dangerous_numbers_rejected() {
        let mut p = Pack::base();
        p.rules.victory_industry = 0;
        assert!(p.validate().is_err());
    }
    #[test]
    fn unit_designs_are_checked() {
        let breaks_unit = |id: &str, edit: &dyn Fn(&mut UnitDef)| {
            let mut pack = Pack::base();
            edit(pack.rules.units.get_mut(id).unwrap());
            assert!(pack.validate().is_err(), "{id} should be rejected");
        };
        let breaks = |edit: &dyn Fn(&mut UnitDef)| breaks_unit("infantry", edit);
        breaks(&|u| u.moves = 0);
        breaks(&|u| u.moves = 11);
        breaks(&|u| u.attack = 101);
        breaks(&|u| u.cost = 0);
        breaks(&|u| u.sprite = "../sprite.png".into());
        // bombard needs both a range and a rate of fire, and they need bombard
        breaks(&|u| u.bombard = 5);
        breaks(&|u| u.range = 1);
        breaks(&|u| {
            u.attack = 0;
            u.blitz = true;
        });
        // ships neither settle nor build
        breaks(&|u| {
            u.domain = Domain::Sea;
            u.work = 1;
        });
        // only ships carry, a ship holds a bounded number, and lethality is a bombard property
        breaks(&|u| u.capacity = 1);
        breaks_unit("transport", &|u| u.capacity = MAX_CAPACITY + 1);
        breaks_unit("ironclad", &|u| u.lethal_sea = true);
        breaks_unit("ironclad", &|u| u.lethal_land = true);
        let mut pack = Pack::base();
        pack.rules.units.get_mut("infantry").unwrap().id = "mismatch".into();
        assert!(pack.validate().is_err());
        // a pack must be able to found cities and fight
        let mut pack = Pack::base();
        pack.rules.units.retain(|_, u| !u.settler);
        assert!(pack.validate().is_err());
        let mut pack = Pack::base();
        pack.rules
            .units
            .retain(|_, u| u.attack == 0 || u.defense == 0);
        assert!(pack.validate().is_err());
    }
    #[test]
    fn the_base_pack_has_the_three_combat_units_of_civ3() {
        let units = &Pack::base().rules.units;
        let (infantry, cavalry, artillery) =
            (&units["infantry"], &units["cavalry"], &units["artillery"]);
        // Infantry: balanced, two tiles.
        assert_eq!(
            (infantry.attack, infantry.defense, infantry.moves),
            (4, 4, 2)
        );
        // Cavalry: hits hard, defends poorly, three tiles, attacks repeatedly.
        assert!(cavalry.attack > infantry.attack && cavalry.defense < infantry.defense);
        assert_eq!((cavalry.moves, cavalry.blitz), (3, true));
        // Artillery: cannot attack or defend, bombards from one tile away, two tiles.
        assert_eq!((artillery.attack, artillery.defense), (0, 0));
        assert!(artillery.can_bombard() && artillery.range == 1 && artillery.moves == 2);
        assert!(artillery.is_defenseless() && !artillery.can_attack());
        // Workers build; pioneers also found cities.
        assert!(units["worker"].can_work() && units["worker"].work > units["pioneer"].work);
        assert!(units["pioneer"].settler && units["pioneer"].can_work());
    }
    #[test]
    fn the_base_pack_has_the_naval_roles_of_civ3() {
        let units = &Pack::base().rules.units;
        let (transport, battleship, cruiser, torpedo, ironclad) = (
            &units["transport"],
            &units["battleship"],
            &units["protected-cruiser"],
            &units["torpedo-boat"],
            &units["ironclad"],
        );
        for ship in [transport, battleship, cruiser, torpedo] {
            assert!(ship.is_naval(), "{}", ship.id);
        }
        // Transport: carries a landing force and cannot fight.
        assert!(transport.is_carrier() && transport.capacity >= 4);
        assert!(!transport.can_attack() && !transport.is_defenseless());
        assert!(!battleship.is_carrier() && !cruiser.is_carrier() && !torpedo.is_carrier());
        // Battleship: the strongest attack afloat, medium defense, slow.
        assert!(battleship.attack > torpedo.attack && battleship.attack > cruiser.attack);
        assert!(battleship.defense > torpedo.defense && battleship.defense >= cruiser.defense);
        assert!(battleship.moves < cruiser.moves);
        // Protected cruiser: the middle of everything, and the fastest of the line.
        assert!(cruiser.attack > ironclad.attack && cruiser.attack < torpedo.attack);
        assert!(cruiser.defense > ironclad.defense && cruiser.defense > torpedo.defense);
        assert!(cruiser.moves > battleship.moves.max(torpedo.moves).max(ironclad.moves));
        // Torpedo boat: hits hard, dies easily, slow, and its bombardment kills at one tile.
        assert!(torpedo.attack > cruiser.attack && torpedo.defense < ironclad.defense);
        assert!(torpedo.moves <= battleship.moves && torpedo.moves < cruiser.moves);
        assert!(torpedo.can_bombard() && torpedo.range == 1);
        assert!(torpedo.lethal_land && torpedo.lethal_sea);
        for other in [transport, battleship, cruiser, ironclad] {
            assert!(!other.lethal_land && !other.lethal_sea, "{}", other.id);
        }
    }

    #[test]
    fn old_pack_format_is_rejected_with_a_clear_message() {
        let mut p = Pack::base();
        p.format = 1;
        let message = p.validate().unwrap_err().to_string();
        assert!(message.contains("Unsupported pack format 1"), "{message}");
        let mut p = Pack::base();
        p.default_scenario = "missing".into();
        assert!(p.validate().is_err());
        let mut p = Pack::base();
        p.scenarios.push(p.scenarios[0].clone());
        assert!(p.validate().is_err());
    }
    /// The world is a Web-Mercator map from 84°N to 58°S (no Antarctica), painted in layers.
    fn world_is_painted_with_layered_terrain(world: &Scenario) {
        use fourx_sim::terrain::{Cover, Relief, Terrain};
        let map = &world.map;
        // An upright world of 512 x 684 half-tile cells: 256 tiles across, 684 rows down,
        // turned into the square grid the simulation runs on.
        let lattice = map.lattice.expect("the world stands upright");
        assert_eq!((lattice.columns, lattice.rows), (256, 684));
        assert_eq!((map.width, map.height), (598, 598));
        assert_eq!(map.tile_count(), 256 * 684);
        // Where a longitude and latitude fall on the plane the tiles tile: 512 units across
        // for 360 degrees, a tile 2 wide and 1 tall, its centre at `(column, row / 2)`.
        let plane = |lon: f64, lat: f64| {
            let unit = (1.0 - lat.to_radians().tan().asinh() / std::f64::consts::PI) / 2.0;
            ((lon + 180.0) / 360.0 * 512.0, unit * 512.0 - 16.0)
        };
        let centre = |p: Coord| {
            let (cx, cy) = lattice.native(p);
            (f64::from(cx), f64::from(cy) / 2.0)
        };
        let at = |lon: f64, lat: f64| {
            let (px, py) = plane(lon, lat);
            map.positions()
                .filter(|&p| {
                    let (x, y) = centre(p);
                    (x - px).abs() <= 2.0 && (y - py).abs() <= 1.0
                })
                .min_by(|&a, &b| {
                    let metric = |p: Coord| {
                        let (x, y) = centre(p);
                        (x - px).abs() / 2.0 + (y - py).abs()
                    };
                    metric(a).total_cmp(&metric(b))
                })
                .and_then(|p| map.get(p))
                .unwrap()
        };
        assert!(map.tiles.iter().all(|t| t.layers_valid()));
        // Nothing below Cape Horn: the bottom rows are all open water.
        assert!(
            map.positions()
                .filter(|&p| lattice.native(p).1 >= lattice.rows - 2)
                .all(|p| map.get(p).unwrap().is_water())
        );
        // Climate decides the base, elevation the relief, and growth the cover.
        assert_eq!(at(12.0, 23.0).terrain, Terrain::Desert, "Sahara");
        assert_eq!(at(86.9, 28.0).relief, Relief::Mountains, "Himalaya");
        assert_eq!(at(6.9, 45.8).relief, Relief::Mountains, "Alps");
        assert_eq!(at(23.0, 0.0).cover, Cover::Jungle, "Congo");
        assert_eq!(at(100.0, 62.0).cover, Cover::Forest, "Siberian taiga");
        assert_eq!(at(51.0, 42.0).terrain, Terrain::Sea, "Caspian Sea");
        assert_eq!(at(-40.0, 30.0).terrain, Terrain::Ocean, "mid-Atlantic");
        assert_eq!(at(-19.0, 65.0).terrain, Terrain::Tundra, "Iceland");
        // The Nile runs through Egypt, and the Congo and Amazon have rivers too.
        let rivers_in = |west: f64, south: f64, east: f64, north: f64| {
            let ((x0, y0), (x1, y1)) = (plane(west, north), plane(east, south));
            map.positions()
                .filter(|&p| {
                    let (x, y) = centre(p);
                    (x0..=x1).contains(&x)
                        && (y0..=y1).contains(&y)
                        && map.get(p).is_some_and(|t| t.river != 0)
                })
                .count()
        };
        assert!(rivers_in(29.0, 22.0, 33.0, 31.0) >= 8, "Nile");
        assert!(rivers_in(-75.0, -10.0, -48.0, 0.0) >= 20, "Amazon");
        // Shares of the land, in the range of a Civ3 world.
        let land: Vec<_> = map.tiles.iter().filter(|t| t.is_land()).collect();
        let share = |wanted: &dyn Fn(&&fourx_sim::terrain::Tile) -> bool| {
            land.iter().filter(|t| wanted(t)).count() as f64 / land.len() as f64
        };
        assert!((0.04..0.10).contains(&share(&|t| t.relief == Relief::Mountains)));
        assert!((0.10..0.25).contains(&share(&|t| t.relief == Relief::Hills)));
        assert!((0.15..0.40).contains(&share(&|t| t.cover == Cover::Forest)));
        assert!(share(&|t| t.cover == Cover::Marsh) > 0.002);
        assert!(share(&|t| t.cover == Cover::Jungle) > 0.01);
        assert!(share(&|t| t.terrain == Terrain::Desert) > 0.05);
        // Water comes in three depths.
        for depth in [Terrain::Coast, Terrain::Sea, Terrain::Ocean] {
            assert!(
                map.tiles.iter().filter(|t| t.terrain == depth).count() > 1000,
                "{depth:?}"
            );
        }
    }
    #[test]
    fn the_default_scenario_is_the_world_of_january_1876() {
        use fourx_sim::Status;
        let content = Content::base();
        assert_eq!(content.pack.default_scenario, "world-1876");
        let world = content.scenario(None).unwrap();
        assert_eq!(world.id, "world-1876");
        assert_eq!(world.start_date.iso(), "1876-01-01");
        world_is_painted_with_layered_terrain(world);
        // Great powers, minor states, dependencies and unrecognised polities all appear.
        for id in [
            "united-kingdom",
            "france",
            "germany",
            "austria-hungary",
            "russia",
            "ottoman-empire",
            "italy",
            "spain",
            "united-states",
            "mexico",
            "brazil",
            "argentina",
            "qing-china",
            "japan",
            "korea",
            "persia",
            "siam",
            "egypt",
            "ethiopia",
            "transvaal",
            "hawaii",
            "tonga",
        ] {
            assert!(world.nations.iter().any(|n| n.id == id), "{id}");
        }
        let count = |status| world.nations.iter().filter(|n| n.status == status).count();
        assert!(
            count(Status::Sovereign) > 60,
            "{}",
            count(Status::Sovereign)
        );
        assert!(
            count(Status::Dependent) >= 20,
            "{}",
            count(Status::Dependent)
        );
        assert!(
            count(Status::Unrecognized) >= 5,
            "{}",
            count(Status::Unrecognized)
        );
        for nation in &world.nations {
            let capitals = world
                .cities
                .iter()
                .filter(|c| c.nation == nation.id && c.capital)
                .count();
            assert_eq!(capitals, 1, "{} needs exactly one capital", nation.id);
        }
        // The same world can be entered as a great power or as a speck in the Pacific.
        let rules = content.pack.rules.with_overrides(&world.rules);
        for id in ["japan", "united-kingdom", "tonga"] {
            let game = Game::from_scenario(1, &rules, world, Some(id)).unwrap();
            assert_eq!(game.factions[&game.commander].tag, id);
        }
    }
    #[test]
    fn every_flavor_has_its_own_city_sprite() {
        use fourx_sim::Flavor;
        let pack = Pack::base();
        let paths: std::collections::BTreeSet<_> = Flavor::ALL
            .iter()
            .map(|flavor| &pack.visuals.cities[flavor.key()])
            .collect();
        assert_eq!(paths.len(), Flavor::ALL.len(), "one skin per flavor");
        // A pack that leaves a flavor out, or names one that does not exist, is rejected.
        let mut missing = Pack::base();
        missing.visuals.cities.remove("arab");
        assert!(missing.validate().is_err());
        let mut unknown = Pack::base();
        unknown
            .visuals
            .cities
            .insert("martian".into(), "x.png".into());
        assert!(unknown.validate().is_err());
    }
    #[test]
    fn nations_are_dealt_the_architecture_of_their_part_of_the_world() {
        use fourx_sim::Flavor::*;
        let content = Content::base();
        let world = content.scenario(None).unwrap();
        let flavor = |id: &str| world.nations.iter().find(|n| n.id == id).expect(id).flavor;
        for (id, wanted) in [
            ("united-kingdom", Western),
            ("spain", Latin),
            ("russia", Orthodox),
            ("ottoman-empire", Arab),
            ("qing-china", EastAsian),
            ("siam", SoutheastAsian),
            ("ethiopia", African),
            ("mongolia", Steppe),
            ("tonga", Oceanic),
        ] {
            assert_eq!(flavor(id), wanted, "{id}");
        }
        let used: std::collections::BTreeSet<_> = world.nations.iter().map(|n| n.flavor).collect();
        assert_eq!(
            used.len(),
            fourx_sim::Flavor::ALL.len(),
            "every skin is used"
        );
        // The hand-made fixture has an eastern empire against a western league.
        let dawn = content.scenario(Some("dawn-straits")).unwrap();
        let flavors: Vec<_> = dawn.nations.iter().map(|n| n.flavor).collect();
        assert_eq!(flavors, [EastAsian, Western]);
    }
    #[test]
    fn scenario_lookup_reports_available_ids() {
        let content = Content::base();
        assert!(content.scenario(None).is_ok());
        let message = content.scenario(Some("nope")).err().unwrap().to_string();
        assert!(message.contains("dawn-straits"), "{message}");
    }

    fn rejects(edit: impl FnOnce(&mut Scenario), why: &str) {
        let mut s = dawn();
        edit(&mut s);
        let rules = &Pack::base().rules;
        let error = scenario::validate_scenario(&s, rules).expect_err(why);
        assert!(error.to_string().contains("dawn-straits"), "{error}");
    }
    #[test]
    fn malformed_scenarios_rejected_without_panics() {
        rejects(|s| s.units[1].kind = "dragon".into(), "unknown unit");
        rejects(|s| s.units[1].nation = "atlantis".into(), "unknown nation");
        rejects(|s| s.units[1].level = Some(4), "level beyond Elite");
        rejects(
            |s| s.units[0].position = Coord::new(500, 500),
            "unit off the map",
        );
        rejects(
            |s| {
                let artillery = s.units.iter().position(|u| u.kind == "artillery").unwrap();
                s.units[artillery].fortified = true;
            },
            "artillery cannot dig in",
        );
        rejects(
            |s| {
                // a second nation's unit on the first nation's square
                let mut intruder = s.units[0].clone();
                intruder.nation = "northern-league".into();
                s.units.push(intruder);
            },
            "two nations on one square",
        );
        rejects(
            |s| {
                for _ in 0..70 {
                    s.units.push(s.units[0].clone());
                }
            },
            "oversized stack",
        );
        rejects(|s| s.cities[0].border = Some(0), "border level below 1");
        rejects(|s| s.cities[0].border = Some(7), "border level above 6");
        rejects(
            |s| s.cities[0].position = Coord::new(-1, 0),
            "city off the map",
        );
        rejects(|s| s.cities[0].nation = "atlantis".into(), "unknown nation");
        rejects(|s| s.cities[0].population = 0, "empty city");
        rejects(
            |s| s.cities[1].position = s.cities[0].position,
            "overlapping cities",
        );
        rejects(|s| s.commander = "nobody".into(), "unknown commander");
        rejects(
            |s| s.nations[1].id = s.nations[0].id.clone(),
            "duplicate nation",
        );
        rejects(|s| s.nations[0].id = "Not Valid".into(), "bad identifier");
        rejects(
            |s| s.cities.retain(|c| c.nation != "northern-league"),
            "city-less nation",
        );
        rejects(|s| s.wars.push(s.wars[0].clone()), "duplicate war");
        rejects(|s| s.wars[0].b = s.wars[0].a.clone(), "war with itself");
        rejects(|s| s.rules.victory_industry = Some(0), "bad rules override");
        rejects(|s| s.format = 99, "future format");
        rejects(
            |s| s.map.tiles[0].region = 4,
            "region index beyond the table",
        );
        rejects(|s| s.map.tiles[0].owner = 1, "owners layer in a scenario");
        rejects(|s| s.map.tiles[0].claim = 1, "claims layer in a scenario");
    }
    #[test]
    fn domains_and_suzerainty_are_checked() {
        // A city or land unit on water, or a ship on land, is rejected rather than "fixed".
        rejects(
            |s| {
                let p = s.cities[0].position;
                s.map
                    .get_mut(p)
                    .unwrap()
                    .set_terrain(fourx_sim::terrain::Terrain::Ocean);
            },
            "city on water",
        );
        rejects(
            |s| {
                let ship = s.units.iter().position(|u| u.kind == "ironclad").unwrap();
                let p = s.units[ship].position;
                s.map.get_mut(p).unwrap().terrain = fourx_sim::terrain::Terrain::Grass;
            },
            "ship on land",
        );
        rejects(
            |s| s.nations[0].suzerain = Some(s.nations[1].id.clone()),
            "suzerain without dependent status",
        );
        rejects(
            |s| {
                s.nations[0].status = fourx_sim::Status::Dependent;
                s.nations[0].suzerain = Some(s.nations[0].id.clone());
            },
            "own suzerain",
        );
        rejects(
            |s| {
                for (i, other) in [(0, 1), (1, 0)] {
                    s.nations[i].status = fourx_sim::Status::Dependent;
                    s.nations[i].suzerain = Some(s.nations[other].id.clone());
                }
            },
            "circular suzerainty",
        );
    }
    #[test]
    fn ships_may_start_in_their_own_coastal_cities_only() {
        use fourx_sim::terrain::Terrain;
        let rules = &Pack::base().rules;
        // Dawn's cities are inland. Berth a ship of `nation` in city `city`, optionally
        // opening a harbour beside it first.
        let berth = |nation: &str, city: usize, harbour: bool| {
            let mut s = dawn();
            let square = s.cities[city].position;
            if harbour {
                let water = Coord::new(square.x - 1, square.y);
                s.map.get_mut(water).unwrap().set_terrain(Terrain::Ocean);
            }
            let ship = s
                .units
                .iter()
                .position(|u| u.kind == "ironclad" && u.nation == nation)
                .unwrap();
            s.units[ship].position = square;
            s
        };
        let own = dawn().cities[0].nation.clone();
        let rival = dawn().cities[1].nation.clone();

        let s = berth(&own, 0, true);
        scenario::validate_scenario(&s, rules).unwrap();
        let game = Game::from_scenario(1, rules, &s, None).unwrap();
        let port = s.cities[0].position;
        assert!(
            game.units
                .values()
                .any(|u| u.kind == "ironclad" && u.position == port)
        );

        let refused = |s: &Scenario| {
            let error = scenario::validate_scenario(s, rules)
                .unwrap_err()
                .to_string();
            assert!(error.contains("must start on"), "{error}");
        };
        // an inland city is no port, and neither is somebody else's
        refused(&berth(&own, 0, false));
        refused(&berth(&own, 1, true));
        refused(&berth(&rival, 0, true));
    }
    #[test]
    fn scenario_names_and_starting_industry_are_moddable() {
        let content = Content::base();
        let mut scenario = dawn();
        scenario.nations[0].name = "A custom empire".into();
        scenario.cities[0].industry = 9;
        scenario::validate_scenario(&scenario, &content.pack.rules).unwrap();
        let game = Game::from_scenario(5, &content.pack.rules, &scenario, None).unwrap();
        assert_eq!(game.factions[&1].name, "A custom empire");
        assert_eq!(game.cities[&1].industry, 9);
    }
    #[test]
    fn rule_overrides_apply_only_to_their_scenario() {
        let content = Content::base();
        let mut scenario = dawn();
        scenario.rules.victory_industry = Some(12_345);
        scenario.rules.starting_gold = Some(7);
        let rules = content.pack.rules.with_overrides(&scenario.rules);
        assert_eq!(rules.victory_industry, 12_345);
        assert_eq!(rules.starting_gold, 7);
        assert_eq!(rules.research_cost, content.pack.rules.research_cost);
        assert_ne!(content.pack.rules.victory_industry, 12_345);
    }
}
