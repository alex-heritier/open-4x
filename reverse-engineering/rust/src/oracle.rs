//! Reader for the oracle fixtures in `tests/data/oracle/`.
//!
//! A fixture is the output of the game's own `Map::generate` (`0x5D16F0`), run
//! under emulation by `reverse-engineering/tools/mapgen/fixtures.py`, with a
//! snapshot of every cell at the entry of each `generateMap` stage. So the
//! snapshot named `S` is the *output of the stage before `S`*, and the one named
//! `end` is the finished map. A test feeds the snapshot of stage `S` to the
//! port of stage `S` and compares the result with the snapshot of the stage
//! after it.
//!
//! # Format
//!
//! Line based, `key value...`:
//!
//! ```text
//! civ3-mapgen-oracle 1
//! scenario NAME / template SAVE / width W / height H / wrap F / seed S / civs N / size I
//! radius R                              Map+0x158, the minimum start distance
//! ret N                                 Map::generate's second argument (the seafaring civ count)
//! multiplayer 0|1                       whether Map::generate told generateMap this is a network game
//! opt climate 1 barbarians 1 ...        actual (resolved) options after the run
//! raw climate 1 barbarians 1 ...        selected options the run was configured with
//! goods CLASS:FREQ ...                  the GOOD rows (class 0 bonus, 1 luxury, 2 strategic)
//! terr HEX ...                          the TERR rows' resource allow-mask bytes
//! goodfx PREREQ:FOOD:SHIELDS:COMMERCE ...   the GOOD rows' tile bonuses (one entry per `goods` entry)
//! terrfx FOOD:SHIELDS:COMMERCE:IRRIGATION:MINING:ROAD:JOB:CITIES ...   the TERR rows' yields
//! stage NAME                            state at the entry of stage NAME
//! plane NAME RLE...                     one plane, run-length encoded: HEX or HEX*COUNT
//! continents N IS_LAND SIZE ...         the Continent records (absent before the first numbering)
//! slots V0 ... V31                      Map+0x16C, the start cell of each civ slot (when it changed)
//! header150 V                           Map+0x150, the land continent count
//! ```
//!
//! A plane that a stage entry does not list is unchanged from the entry before.
//! The planes are the cell fields `Map::generate` writes; see [`Planes`]. The
//! `final_pass` entry also carries two scratch planes, `value` and `shore`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::cell::{Cell, MapGrid, PLANE_FEATURE, PLANE_OVERLAY, PLANE_TERRAIN};
use crate::options::{Options, RawOptions};
use crate::placement::{GoodRule, Rules, TerrainRule};

/// The names of the cell planes in a fixture, in file order. (A fixture may
/// also carry the scratch plane `region`; see [`Planes::region`].)
pub const PLANE_NAMES: [&str; 8] = [
    "river",
    "resource",
    "image",
    "file",
    "continent",
    "overlay",
    "terrain",
    "feature",
];

/// All generator-written cell fields, one vector per field, indexed by cell.
///
/// | plane | `Cell` offset | meaning |
/// |---|---|---|
/// | `river` | `+0x04` | river adjacency mask |
/// | `resource` | `+0x08` | `GOOD` row, `0xFFFFFFFF` for none |
/// | `image` | `+0x10` | terrain sprite variant |
/// | `file` | `+0x11` | terrain sprite file |
/// | `continent` | `+0x1E` | continent id |
/// | `overlay` | `+0x28` | overlay bits |
/// | `terrain` | `+0x2C` | terrain word |
/// | `feature` | `+0x30` | feature plane |
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Planes {
    /// `Cell+0x04`: the river adjacency mask.
    pub river: Vec<u32>,
    /// `Cell+0x08`: the `GOOD` row, `0xFFFFFFFF` for none.
    pub resource: Vec<u32>,
    /// `Cell+0x10`: the terrain sprite variant.
    pub image: Vec<u32>,
    /// `Cell+0x11`: the terrain sprite file.
    pub file: Vec<u32>,
    /// `Cell+0x1E`: the continent id.
    pub continent: Vec<u32>,
    /// `Cell+0x28`: the overlay bits.
    pub overlay: Vec<u32>,
    /// `Cell+0x2C`: the terrain word.
    pub terrain: Vec<u32>,
    /// `Cell+0x30`: the feature plane.
    pub feature: Vec<u32>,
    /// Not a cell field: the region map `paintContinents` (`0x5EDDB0`) builds
    /// at `Map+0x3C`, present only in the snapshots taken while it is alive
    /// (empty otherwise, and never carried forward).
    pub region: Vec<u32>,
    /// Not a cell field: what `Map` vtable slot 8 (`0x5D3830`) returns for
    /// each tile, the city-site score `finalPass` ranks the tiles by. Present
    /// only in the `final_pass` entry snapshot.
    pub value: Vec<u32>,
    /// Not a cell field: `0x5EEDB0` of each tile, the id of the largest water
    /// body among its 8 neighbours or `0xFFFFFFFF`. Present only in the
    /// `final_pass` entry snapshot.
    pub shore: Vec<u32>,
}

impl Planes {
    /// The plane called `name`.
    pub fn get(&self, name: &str) -> Option<&Vec<u32>> {
        Some(match name {
            "river" => &self.river,
            "resource" => &self.resource,
            "image" => &self.image,
            "file" => &self.file,
            "continent" => &self.continent,
            "overlay" => &self.overlay,
            "terrain" => &self.terrain,
            "feature" => &self.feature,
            "region" => &self.region,
            "value" => &self.value,
            "shore" => &self.shore,
            _ => return None,
        })
    }

    fn get_mut(&mut self, name: &str) -> Option<&mut Vec<u32>> {
        Some(match name {
            "river" => &mut self.river,
            "resource" => &mut self.resource,
            "image" => &mut self.image,
            "file" => &mut self.file,
            "continent" => &mut self.continent,
            "overlay" => &mut self.overlay,
            "terrain" => &mut self.terrain,
            "feature" => &mut self.feature,
            "region" => &mut self.region,
            "value" => &mut self.value,
            "shore" => &mut self.shore,
            _ => return None,
        })
    }

    /// Number of cells.
    pub fn len(&self) -> usize {
        self.terrain.len()
    }

    /// `true` when there are no cells.
    pub fn is_empty(&self) -> bool {
        self.terrain.is_empty()
    }

    /// Builds the port's cell grid from the planes.
    pub fn to_grid(&self, w: i32, h: i32, wrap: u32, seed: i32) -> MapGrid {
        let mut grid = MapGrid::new(w, h, wrap, seed);
        assert_eq!(grid.cells.len(), self.len(), "plane length vs {w}x{h}");
        for (i, c) in grid.cells.iter_mut().enumerate() {
            let mut planes = [0u32; crate::cell::NUM_FLAG_PLANES];
            planes[PLANE_OVERLAY] = self.overlay[i];
            planes[PLANE_TERRAIN] = self.terrain[i];
            planes[PLANE_FEATURE] = self.feature[i];
            *c = Cell {
                river: self.river[i] as u8,
                resource: self.resource[i] as i32,
                image: self.image[i] as u8,
                file: self.file[i] as u8,
                continent: self.continent[i] as u16,
                planes,
            };
        }
        grid
    }

    /// The planes of a cell grid.
    pub fn from_grid(grid: &MapGrid) -> Planes {
        let mut p = Planes::default();
        for c in &grid.cells {
            p.river.push(u32::from(c.river));
            p.resource.push(c.resource as u32);
            p.image.push(u32::from(c.image));
            p.file.push(u32::from(c.file));
            p.continent.push(u32::from(c.continent));
            p.overlay.push(c.planes[PLANE_OVERLAY]);
            p.terrain.push(c.planes[PLANE_TERRAIN]);
            p.feature.push(c.planes[PLANE_FEATURE]);
        }
        p
    }

    /// Compares the named planes, returning one entry per differing plane.
    ///
    /// An empty result means every listed plane is identical.
    pub fn diff(&self, other: &Planes, names: &[&str]) -> Vec<PlaneDiff> {
        let mut out = Vec::new();
        for &name in names {
            let (a, b) = (self.get(name).expect("plane name"), other.get(name).expect("plane name"));
            let mut first = Vec::new();
            let mut count = 0;
            for (i, (x, y)) in a.iter().zip(b).enumerate() {
                if x != y {
                    count += 1;
                    if first.len() < 6 {
                        first.push((i, *x, *y));
                    }
                }
            }
            if count > 0 || a.len() != b.len() {
                out.push(PlaneDiff {
                    plane: name.to_string(),
                    count,
                    first,
                });
            }
        }
        out
    }
}

/// How one plane differs between two [`Planes`].
#[derive(Clone, Debug)]
pub struct PlaneDiff {
    /// Plane name.
    pub plane: String,
    /// Number of differing cells.
    pub count: usize,
    /// The first few differences: `(cell index, left, right)`.
    pub first: Vec<(usize, u32, u32)>,
}

impl std::fmt::Display for PlaneDiff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {} cells differ, first", self.plane, self.count)?;
        for (i, a, b) in &self.first {
            write!(f, " [{i}] {a:x}!={b:x}")?;
        }
        Ok(())
    }
}

/// One snapshot: the state at the entry of a stage.
#[derive(Clone, Debug)]
pub struct Stage {
    /// Stage name; `end` is the finished map.
    pub name: String,
    /// Every plane, complete (carried forward from earlier snapshots).
    pub planes: Planes,
    /// The `Continent` records `(is_land, size)` when the exe had numbered
    /// them by then.
    pub continents: Option<Vec<(bool, u32)>>,
    /// `Map+0x150`.
    pub land_continents: u32,
    /// `Map+0x16C`: the 32 start slots, a cell index each, `-1` for none.
    pub slots: Vec<i32>,
}

/// A whole fixture.
#[derive(Clone, Debug)]
pub struct Fixture {
    /// Scenario name.
    pub name: String,
    /// Map width.
    pub width: i32,
    /// Map height.
    pub height: i32,
    /// `Map+0x1F0` wrap flags.
    pub wrap: u32,
    /// `Map+0x15C`, the civ count.
    pub civs: i32,
    /// `Map+0x158`, the minimum distance between two starts.
    pub radius: i32,
    /// The second argument of `Map::generate`, which the New Game screen
    /// sets to the number of civs with the Seafaring trait.
    pub seafarers: i32,
    /// Whether `Map::generate` called `generateMap` for a network game.
    pub multiplayer: bool,
    /// The actual option values after the run, with the seed and size.
    pub options: Options,
    /// The selected values the run was configured with.
    pub raw: RawOptions,
    /// The `GOOD` and `TERR` tables the run used.
    pub rules: Rules,
    /// Snapshots in pipeline order.
    pub stages: Vec<Stage>,
}

/// The directory the fixtures live in.
pub fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/oracle")
}

/// Names of every fixture on disk, sorted.
pub fn fixture_names() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(fixture_dir())
        .map(|d| {
            d.filter_map(|e| e.ok())
                .filter_map(|e| {
                    let n = e.file_name().into_string().ok()?;
                    n.strip_suffix(".txt").map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// Loads the fixture `name` from [`fixture_dir`].
pub fn load(name: &str) -> Result<Fixture, String> {
    let path = fixture_dir().join(format!("{name}.txt"));
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse(&text)
}

fn rle(tokens: &[&str]) -> Result<Vec<u32>, String> {
    let mut out = Vec::new();
    for t in tokens {
        let (v, n) = match t.split_once('*') {
            Some((v, n)) => (v, n.parse::<usize>().map_err(|e| format!("{t}: {e}"))?),
            None => (*t, 1),
        };
        let v = u32::from_str_radix(v, 16).map_err(|e| format!("{t}: {e}"))?;
        out.extend(std::iter::repeat(v).take(n));
    }
    Ok(out)
}

/// Splits a `a:b:c` token into numbers.
fn numbers(token: &str, what: &str) -> Result<Vec<i32>, String> {
    token.split(':').map(|n| n.parse::<i32>().map_err(|e| format!("{what} {token}: {e}"))).collect()
}

/// Parses fixture text.
pub fn parse(text: &str) -> Result<Fixture, String> {
    let mut lines = text.lines();
    if lines.next() != Some("civ3-mapgen-oracle 1") {
        return Err("not a civ3-mapgen-oracle 1 file".into());
    }
    let mut header: BTreeMap<String, String> = BTreeMap::new();
    let mut opt = BTreeMap::new();
    let mut raw = BTreeMap::new();
    let mut rules = Rules { goods: Vec::new(), terr: Vec::new(), terrain: Vec::new() };
    let mut stages: Vec<Stage> = Vec::new();
    let mut cur = Planes::default();
    let mut cur_slots: Vec<i32> = Vec::new();
    let mut pending: Option<Stage> = None;

    let pairs = |rest: &str| -> Result<BTreeMap<String, i32>, String> {
        let toks: Vec<&str> = rest.split_whitespace().collect();
        toks.chunks(2)
            .map(|c| {
                let v = c.get(1).ok_or("odd option list")?;
                Ok((c[0].to_string(), v.parse::<i32>().map_err(|e| format!("{v}: {e}"))?))
            })
            .collect()
    };

    for line in lines {
        let (key, rest) = line.split_once(' ').unwrap_or((line, ""));
        match key {
            "scenario" | "template" | "width" | "height" | "wrap" | "seed" | "civs" | "size" | "radius"
            | "ret" | "multiplayer" => {
                header.insert(key.to_string(), rest.to_string());
            }
            "opt" => opt = pairs(rest)?,
            "raw" => raw = pairs(rest)?,
            "goods" => {
                rules.goods = rest
                    .split_whitespace()
                    .map(|t| {
                        let (class, freq) = t.split_once(':').ok_or(format!("goods: {t}"))?;
                        Ok(GoodRule {
                            class: class.parse().map_err(|e| format!("goods {t}: {e}"))?,
                            freq: freq.parse().map_err(|e| format!("goods {t}: {e}"))?,
                            prerequisite: -1,
                            food: 0,
                            shields: 0,
                            commerce: 0,
                        })
                    })
                    .collect::<Result<_, String>>()?;
            }
            "goodfx" => {
                for (g, t) in rules.goods.iter_mut().zip(rest.split_whitespace()) {
                    let v = numbers(t, "goodfx")?;
                    let [prerequisite, food, shields, commerce] = v[..] else {
                        return Err(format!("goodfx: {t}"));
                    };
                    (g.prerequisite, g.food, g.shields, g.commerce) = (prerequisite, food, shields, commerce);
                }
            }
            "terrfx" => {
                rules.terrain = rest
                    .split_whitespace()
                    .map(|t| {
                        let v = numbers(t, "terrfx")?;
                        let [food, shields, commerce, irrigation, mining, road, worker_job, cities] = v[..] else {
                            return Err(format!("terrfx: {t}"));
                        };
                        Ok(TerrainRule {
                            food,
                            shields,
                            commerce,
                            irrigation,
                            mining,
                            road,
                            worker_job,
                            allows_cities: cities != 0,
                        })
                    })
                    .collect::<Result<_, String>>()?;
            }
            "terr" => {
                rules.terr = rest
                    .split_whitespace()
                    .map(|t| {
                        (0..t.len() / 2)
                            .map(|i| u8::from_str_radix(&t[2 * i..2 * i + 2], 16).map_err(|e| format!("terr {t}: {e}")))
                            .collect::<Result<Vec<u8>, String>>()
                    })
                    .collect::<Result<_, String>>()?;
            }
            "stage" => {
                if let Some(s) = pending.take() {
                    stages.push(s);
                }
                cur.region.clear();
                cur.value.clear();
                cur.shore.clear();
                pending = Some(Stage {
                    name: rest.to_string(),
                    planes: Planes::default(),
                    continents: None,
                    land_continents: 0,
                    slots: Vec::new(),
                });
            }
            "plane" => {
                let (name, vals) = rest.split_once(' ').unwrap_or((rest, ""));
                let toks: Vec<&str> = vals.split_whitespace().collect();
                let v = rle(&toks)?;
                *cur.get_mut(name).ok_or(format!("unknown plane {name}"))? = v;
            }
            "continents" => {
                let toks: Vec<u32> = rest
                    .split_whitespace()
                    .map(|t| t.parse::<u32>().map_err(|e| format!("{t}: {e}")))
                    .collect::<Result<_, _>>()?;
                let n = *toks.first().ok_or("empty continents line")? as usize;
                if toks.len() != 1 + 2 * n {
                    return Err(format!("continents: {n} records but {} numbers", toks.len() - 1));
                }
                let recs = toks[1..].chunks(2).map(|c| (c[0] != 0, c[1])).collect();
                if let Some(s) = pending.as_mut() {
                    s.continents = Some(recs);
                }
            }
            "slots" => {
                cur_slots = rest
                    .split_whitespace()
                    .map(|t| t.parse::<i32>().map_err(|e| format!("slots {t}: {e}")))
                    .collect::<Result<_, _>>()?;
            }
            "header150" => {
                if let Some(s) = pending.as_mut() {
                    s.land_continents = rest.trim().parse().map_err(|e| format!("header150: {e}"))?;
                }
            }
            "" => {}
            other => return Err(format!("unknown line key {other:?}")),
        }
        // A `plane` line belongs to the stage opened last; snapshot it eagerly so
        // `cur` can keep accumulating for the next stage.
        if let Some(s) = pending.as_mut() {
            s.planes = cur.clone();
            s.slots = cur_slots.clone();
        }
    }
    if let Some(s) = pending.take() {
        stages.push(s);
    }

    let num = |k: &str| -> Result<i32, String> {
        header
            .get(k)
            .ok_or(format!("missing {k}"))?
            .trim()
            .parse::<i32>()
            .map_err(|e| format!("{k}: {e}"))
    };
    let get = |m: &BTreeMap<String, i32>, k: &str| m.get(k).copied().ok_or(format!("missing option {k}"));
    let seed = num("seed")?;
    let size = num("size")?;
    Ok(Fixture {
        name: header.get("scenario").cloned().unwrap_or_default(),
        width: num("width")?,
        height: num("height")?,
        wrap: num("wrap")? as u32,
        civs: num("civs")?,
        radius: num("radius")?,
        seafarers: num("ret")?,
        multiplayer: num("multiplayer")? != 0,
        options: Options {
            seed,
            size,
            climate: get(&opt, "climate")?,
            barbarians: get(&opt, "barbarians")?,
            landmass: get(&opt, "landmass")?,
            ocean: get(&opt, "ocean")?,
            temperature: get(&opt, "temperature")?,
            age: get(&opt, "age")?,
        },
        raw: RawOptions {
            climate: get(&raw, "climate")?,
            barbarians: get(&raw, "barbarians")?,
            landmass: get(&raw, "landmass")?,
            ocean: get(&raw, "ocean")?,
            temperature: get(&raw, "temperature")?,
            age: get(&raw, "age")?,
        },
        rules,
        stages,
    })
}

impl Fixture {
    /// Whether the exe ran stage `name` (a stage that `generateMap` skips for
    /// this fixture's options has no snapshot).
    pub fn has_stage(&self, name: &str) -> bool {
        self.stages.iter().any(|s| s.name == name)
    }

    /// The snapshot at the entry of stage `name`.
    pub fn stage(&self, name: &str) -> &Stage {
        self.stages
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("fixture {} has no stage {name}", self.name))
    }

    /// The snapshot after `name`, i.e. the entry of the next stage.
    pub fn after(&self, name: &str) -> &Stage {
        let i = self
            .stages
            .iter()
            .position(|s| s.name == name)
            .unwrap_or_else(|| panic!("fixture {} has no stage {name}", self.name));
        &self.stages[i + 1]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rle_decodes_runs_and_singles() {
        assert_eq!(rle(&["dd00*3", "2200", "0*2"]).unwrap(), vec![0xdd00, 0xdd00, 0xdd00, 0x2200, 0, 0]);
        assert!(rle(&["zz"]).is_err());
    }

    #[test]
    fn a_minimal_fixture_parses_and_carries_planes_forward() {
        let text = "civ3-mapgen-oracle 1\nscenario t\nwidth 4\nheight 2\nwrap 0\nseed 9\ncivs 2\nsize 0\nradius 4\nret 0\nmultiplayer 0\n\
                    opt climate 1 barbarians 1 landmass 1 ocean 1 temperature 1 age 1\n\
                    raw climate 3 barbarians 4 landmass 1 ocean 1 temperature 1 age 1\n\
                    stage a\nplane terrain dd00*4\nplane river 0*4\n\
                    stage b\nplane terrain 2200*2 dd00*2\ncontinents 1 1 2\n";
        let f = parse(text).unwrap();
        assert_eq!(f.stages.len(), 2);
        assert_eq!(f.stage("a").planes.terrain, vec![0xdd00; 4]);
        assert_eq!(f.stage("b").planes.terrain, vec![0x2200, 0x2200, 0xdd00, 0xdd00]);
        assert_eq!(f.stage("b").planes.river, vec![0; 4], "unlisted plane carried forward");
        assert_eq!(f.stage("b").continents, Some(vec![(true, 2)]));
        assert_eq!(f.raw.climate, 3);
    }
}
