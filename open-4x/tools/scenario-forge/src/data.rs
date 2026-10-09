//! Curated history tables, read from `data/`. These are the forge's source of truth; the
//! generated scenario is a build artifact.
use fourx_sim::terrain::{Cover, Relief, Terrain};
use fourx_sim::{Flavor, Rgb, RuleOverrides, Status, WarStart};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::path::Path;

fn one() -> f64 {
    1.0
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Meta {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub start_date: String,
    pub commander: String,
    #[serde(default)]
    pub intro: String,
    #[serde(default)]
    pub charted: bool,
    #[serde(default)]
    pub rules: RuleOverrides,
    #[serde(default)]
    pub wars: Vec<WarStart>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fleet {
    /// Name of one of the nation's cities; ships start on the nearest water tile.
    pub port: String,
    pub ships: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NationSource {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub adjective: String,
    pub color: Rgb,
    /// The architectural and cultural look of the nation's cities.
    pub flavor: Flavor,
    #[serde(default)]
    pub status: Status,
    #[serde(default)]
    pub suzerain: Option<String>,
    #[serde(default)]
    pub government: String,
    #[serde(default)]
    pub leader: String,
    /// Development tier 0–10: drives industry, technology, and treasury.
    pub dev: u32,
    /// Military tier 0–4: drives the size of the starting armies.
    pub army: u32,
    #[serde(default)]
    pub navy: Vec<Fleet>,
    #[serde(default)]
    pub notes: String,
}

/// One painting operation: exactly one of `country`, `poly`, `rect`, or `tiles`.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Paint {
    /// Natural Earth `ADM0_A3` code; paints the whole country, optionally clipped to `bbox`.
    #[serde(default)]
    pub country: Option<String>,
    /// west, south, east, north in degrees.
    #[serde(default)]
    pub bbox: Option<[f64; 4]>,
    /// Polygon of `[lon, lat]` vertices.
    #[serde(default)]
    pub poly: Option<Vec<[f64; 2]>>,
    #[serde(default)]
    pub rect: Option<[f64; 4]>,
    /// Individual points. Each becomes a land tile belonging to the region, even if the
    /// coastline is too fine to show it (micro-states, small islands).
    #[serde(default)]
    pub tiles: Option<Vec<[f64; 2]>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegionSource {
    pub id: String,
    pub name: String,
    /// `None` paints "unclaimed" over earlier claims.
    pub nation: Option<String>,
    pub paint: Vec<Paint>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CitySource {
    pub nation: String,
    pub name: String,
    pub lon: f64,
    pub lat: f64,
    /// Population in thousands around 1876.
    pub pop: u32,
    #[serde(default)]
    pub capital: bool,
    /// Development tier 0-10 for this city when it differs from its nation's (colonies).
    #[serde(default)]
    pub dev: Option<u32>,
}

/// A hand correction to the terrain the climate and elevation data produce: every tile of
/// the area (thinned by `density`) takes the layers given here and keeps the rest.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Patch {
    pub name: String,
    /// west, south, east, north in degrees.
    #[serde(default)]
    pub bbox: Option<[f64; 4]>,
    /// Polygon of `[lon, lat]` vertices.
    #[serde(default)]
    pub poly: Option<Vec<[f64; 2]>>,
    /// A land type: `Grass`, `Plains`, `Desert` or `Tundra`.
    #[serde(default)]
    pub terrain: Option<Terrain>,
    /// `Flat`, `Hills` or `Mountains`.
    #[serde(default)]
    pub relief: Option<Relief>,
    /// `Bare`, `Forest`, `Jungle` or `Marsh`.
    #[serde(default)]
    pub cover: Option<Cover>,
    /// Chance in 0..=1 that a tile inside the area is changed.
    #[serde(default = "one")]
    pub density: f64,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct TerrainSource {
    #[serde(default)]
    pub patches: Vec<Patch>,
    /// Points forced to be land (isthmuses and narrow land bridges).
    #[serde(default)]
    pub land: Vec<[f64; 2]>,
    /// Points forced to be water (straits and canals too narrow for the raster).
    #[serde(default)]
    pub water: Vec<[f64; 2]>,
}

pub struct Sources {
    pub meta: Meta,
    pub nations: Vec<NationSource>,
    pub regions: Vec<RegionSource>,
    pub cities: Vec<CitySource>,
    pub terrain: TerrainSource,
}

fn read<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let text =
        std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))
}

impl Sources {
    /// Work-in-progress builds: keep only nations that already have a city, and whatever
    /// refers to them. Never used for the shipped scenario.
    pub fn restrict_to_nations_with_cities(&mut self) {
        let with_city: std::collections::HashSet<String> =
            self.cities.iter().map(|c| c.nation.clone()).collect();
        self.nations.retain(|n| with_city.contains(&n.id));
        let keep: std::collections::HashSet<String> =
            self.nations.iter().map(|n| n.id.clone()).collect();
        self.regions
            .retain(|r| r.nation.as_deref().is_none_or(|n| keep.contains(n)));
        self.meta
            .wars
            .retain(|w| keep.contains(&w.a) && keep.contains(&w.b));
        for nation in &mut self.nations {
            if nation
                .suzerain
                .as_deref()
                .is_some_and(|s| !keep.contains(s))
            {
                nation.suzerain = None;
                nation.status = Status::Sovereign;
            }
            let own: std::collections::HashSet<&str> = self
                .cities
                .iter()
                .filter(|c| c.nation == nation.id)
                .map(|c| c.name.as_str())
                .collect();
            nation
                .navy
                .retain(|fleet| own.contains(fleet.port.as_str()));
        }
        if !keep.contains(&self.meta.commander) {
            self.meta.commander = self.nations[0].id.clone();
        }
    }

    pub fn load(directory: &Path) -> Result<Self, String> {
        let mut region_files: Vec<_> = std::fs::read_dir(directory.join("regions"))
            .map_err(|error| format!("{}/regions: {error}", directory.display()))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .collect();
        region_files.sort();
        let mut regions = Vec::new();
        for path in region_files {
            regions.extend(read::<Vec<RegionSource>>(&path)?);
        }
        let mut cities = Vec::new();
        let mut city_files: Vec<_> = std::fs::read_dir(directory.join("cities"))
            .map_err(|error| format!("{}/cities: {error}", directory.display()))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .collect();
        city_files.sort();
        for path in city_files {
            cities.extend(read::<Vec<CitySource>>(&path)?);
        }
        Ok(Self {
            meta: read(&directory.join("meta.json"))?,
            nations: read(&directory.join("nations.json"))?,
            regions,
            cities,
            terrain: read(&directory.join("terrain.json"))?,
        })
    }
}
