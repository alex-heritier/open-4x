//! The converted art and sound cache (`docs/civ3-files.md` section 7).
//!
//! The game draws PNG and plays WAV/OGG; Civ3 ships PCX, FLC and MP3.
//! `tools/prep_assets.py` converts, and nobody runs it by hand: after the
//! rules are read, [`plan`] works out which Civ3 files the rules refer to,
//! and [`ensure`] checks them against `assets/cache/index.json`. When
//! anything is missing or stale it writes `assets/cache/request.json` and runs
//! `python3 tools/prep_assets.py --request` before the window opens.
//!
//! Everything Civ3's data refers to is cached under its **resolved path**,
//! lowercased and relative to the source asset root: the unit folder
//! `Conquests/Art/Units/Warrior` is `assets/cache/conquests/art/units/warrior/`.
//! The key says which file won the search, so a scenario's own Warrior and the
//! stock one are separate entries. Interface pieces nothing in Civ3's data
//! names keep fixed output names (`ui/`, `terrain/`, ...).

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use civ3_biq::Biq;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::install::Install;
use crate::roster::UnitRow;

/// Where the cache lives, from the game's working directory.
pub const CACHE: &str = "assets/cache";
/// The path the asset server loads from (relative to `assets/`).
pub const CACHE_URL: &str = "cache";
const SCRIPT: &str = "tools/prep_assets.py";

/// The folder names of the units that change with the era (leaders, armies).
pub const ERA_NAMES: [&str; 4] = [
    "Ancient Times",
    "Middle Ages",
    "Industrial Ages",
    "Modern Times",
];

/// The interface stages of the script: the pieces our code draws from shared
/// sheets, which no Civ3 data refers to.
const STAGES: &[&str] = &[
    "terrain",
    "cityscreen",
    "cities",
    "splash",
    "audio",
    "fonts",
    "features",
    "advisors",
    "improvements",
    "unitbuttons",
    "hud",
    "fog",
    "borders",
    "cursor",
    "diplomacy",
    "wonders_ui",
];

/// One `Art/Units` folder to convert.
#[derive(Clone, Debug)]
pub struct UnitItem {
    /// The folder's name in the BIQ's spelling.
    pub art: String,
    pub key: String,
    /// The folders that hold it, nearest first.
    pub dirs: Vec<PathBuf>,
}

/// One leader clip and its reverse twin.
#[derive(Clone, Debug)]
pub struct LeaderItem {
    pub key: String,
    pub forward: PathBuf,
    pub reverse: Option<PathBuf>,
}

/// A picture to convert (a wonder splash or an advance icon).
#[derive(Clone, Debug)]
pub struct PicItem {
    pub key: String,
    pub path: PathBuf,
}

/// What the rules refer to, and where each piece lands in the cache.
#[derive(Debug, Default)]
pub struct ArtPlan {
    pub units: Vec<UnitItem>,
    /// Unit folder (lowercase, as the BIQ spells it) -> cache key.
    pub unit_keys: HashMap<String, String>,
    /// By `RACE` roster index, the leader clip of each era (`None` without).
    pub leaders: Vec<[Option<LeaderItem>; 4]>,
    /// `BLDG` row -> splash picture.
    pub wonders: HashMap<usize, PicItem>,
    /// Advance row -> icon.
    pub techs: Vec<Option<PicItem>>,
}

impl ArtPlan {
    /// The cache key of the unit folder `art` (any case).
    pub fn unit_key(&self, art: &str) -> Option<&str> {
        self.unit_keys
            .get(&art.to_ascii_lowercase())
            .map(String::as_str)
    }

    /// The cache file of a wonder's splash and thumbnail, when converted.
    pub fn wonder_art(&self, row: usize, thumb: bool) -> Option<String> {
        let key = &self.wonders.get(&row)?.key;
        let rel = if thumb {
            format!("{key}.thumb.png")
        } else {
            format!("{key}.png")
        };
        crate::web::cache_exists(Path::new(CACHE).join(&rel)).then(|| format!("{CACHE_URL}/{rel}"))
    }
}

impl ArtPlan {
    /// The asset path of an advance's icon (even when not converted: a
    /// missing file shows nothing).
    pub fn tech_icon(&self, row: usize) -> Option<String> {
        let p = self.techs.get(row)?.as_ref()?;
        Some(format!("{CACHE_URL}/{}.png", p.key))
    }
}

/// A file's cache key: its path under the install, lowercased and `/`
/// separated (anything outside the install keeps the path without its root).
pub fn key_of(install: &Install, path: &Path) -> String {
    let rel: PathBuf = match path.strip_prefix(&install.root) {
        Ok(rel) => rel.to_path_buf(),
        Err(_) => path
            .components()
            .filter(|c| matches!(c, Component::Normal(_)))
            .collect(),
    };
    rel.components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().to_lowercase()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Work out the Civ3 files the rules of `biq` refer to. `pedia` is the
/// merged `PediaIcons.txt`; `units` the built unit rows (their `art` is the
/// `Art/Units` folder, empty for a unit that cannot be played).
pub fn plan(
    biq: &Biq,
    install: &Install,
    pedia: &HashMap<String, String>,
    units: &[UnitRow],
) -> ArtPlan {
    let r = &biq.rules;
    let mut plan = ArtPlan::default();

    for art in units.iter().map(|u| u.art).filter(|a| !a.is_empty()) {
        let variants: Vec<String> = if art.ends_with(ERA_NAMES[0]) {
            ERA_NAMES
                .iter()
                .map(|era| art.replace(ERA_NAMES[0], era))
                .collect()
        } else {
            vec![art.to_string()]
        };
        for name in variants {
            if plan.unit_keys.contains_key(&name.to_ascii_lowercase()) {
                continue;
            }
            let dirs: Vec<PathBuf> = install
                .resolve_all(&format!("Art/Units/{name}"))
                .into_iter()
                .filter(|p| install.is_dir(p))
                .collect();
            let Some(first) = dirs.first() else { continue };
            let key = key_of(install, first);
            plan.unit_keys
                .insert(name.to_ascii_lowercase(), key.clone());
            plan.units.push(UnitItem {
                art: name,
                key,
                dirs,
            });
        }
    }

    // `era_art`: four forward clips, then their four reverse twins.
    for c in r.civilizations.iter().filter(|c| c.civilization_index != 0) {
        let at = |i: usize| -> Option<PathBuf> {
            let s = c.era_art.get(i)?.text();
            let s = s.trim();
            if s.is_empty() {
                None
            } else {
                install.resolve(s).filter(|p| install.is_file(p))
            }
        };
        plan.leaders.push(std::array::from_fn(|era| {
            let forward = at(era)?;
            Some(LeaderItem {
                key: key_of(install, &forward),
                forward,
                reverse: at(era + 4),
            })
        }));
    }

    let pic = |key: String| -> Option<PicItem> {
        let path = install
            .resolve(pedia.get(&key)?)
            .filter(|p| install.is_file(p))?;
        Some(PicItem {
            key: key_of(install, &path),
            path,
        })
    };
    for (row, b) in r.buildings.iter().enumerate() {
        let entry = b.civilopedia_entry.text().trim().to_ascii_lowercase();
        if let Some(p) = pic(format!("won_splash_{entry}")) {
            plan.wonders.insert(row, p);
        }
    }
    plan.techs = r
        .techs
        .iter()
        .map(|t| pic(t.civilopedia_entry.text().trim().to_ascii_lowercase()))
        .collect();
    plan
}

// ---- the request and the index ---------------------------------------------

#[derive(Deserialize)]
struct Source {
    path: String,
    size: u64,
    mtime: u64,
}

#[derive(Deserialize)]
struct Entry {
    /// Canonical source root; old indices without it are stale.
    root: Option<String>,
    outputs: Vec<String>,
    sources: Vec<Source>,
    params: Value,
    search: Option<Vec<String>>,
}

#[derive(Deserialize, Default)]
struct Index {
    script: String,
    entries: HashMap<String, Entry>,
}

/// FNV-1a 64 of `bytes`, as the script computes it.
fn fnv(bytes: &[u8]) -> String {
    let mut h: u64 = 0xCBF29CE484222325;
    for &b in bytes {
        h = (h ^ u64::from(b)).wrapping_mul(0x100000001B3);
    }
    format!("{h:016x}")
}

fn read_index(script_hash: &str) -> Index {
    let index: Index = std::fs::read_to_string(Path::new(CACHE).join("index.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    if index.script == script_hash {
        index
    } else {
        Index::default()
    }
}

impl Index {
    /// Is `key` converted from the files that are there now?
    fn fresh(&self, root: &Path, key: &str, params: &Value, search: Option<&[String]>) -> bool {
        let Some(e) = self.entries.get(key) else {
            return false;
        };
        let Ok(root) = root.canonicalize() else {
            return false;
        };
        if e.root.as_deref() != Some(path_str(&root).as_str()) {
            return false;
        }
        // A unit's team colors only grow: a copy with more colors will do.
        let same = match (params, &e.params) {
            (Value::Array(want), Value::Array(have)) => want.iter().all(|w| have.contains(w)),
            (want, have) => want == have,
        };
        if !same {
            return false;
        }
        if let Some(search) = search {
            if e.search.as_deref() != Some(search) {
                return false;
            }
        }
        if e.outputs.iter().any(|o| !Path::new(CACHE).join(o).exists()) {
            return false;
        }
        e.sources.iter().all(|s| {
            let full = if Path::new(&s.path).is_absolute() {
                PathBuf::from(&s.path)
            } else {
                root.join(&s.path)
            };
            let Ok(meta) = std::fs::metadata(full) else {
                return false;
            };
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());
            meta.len() == s.size && mtime == s.mtime
        })
    }
}

/// What a game with these civs needs converted.
pub struct Wanted<'a> {
    pub install: &'a Install,
    pub plan: &'a ArtPlan,
    /// `RACE` roster index of every civ in play.
    pub roster: &'a [usize],
    /// Team colors (`ntpNN.pcx`) in play.
    pub colors: Vec<u8>,
}

fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

impl Wanted<'_> {
    fn leaders(&self) -> Vec<&LeaderItem> {
        let mut out: Vec<&LeaderItem> = vec![];
        for &r in self.roster {
            for l in self.plan.leaders.get(r).into_iter().flatten().flatten() {
                if !out.iter().any(|o| o.key == l.key) {
                    out.push(l);
                }
            }
        }
        out
    }

    /// The request the script reads.
    fn request(&self) -> Value {
        json!({
            "root": path_str(&self.install.root),
            "search": self.install.search.iter().map(|p| path_str(p)).collect::<Vec<_>>(),
            "team_colors": self.colors,
            "stages": STAGES,
            "units": self.plan.units.iter().map(|u| json!({
                "art": u.art, "key": u.key, "dirs": u.dirs.iter().map(|p| path_str(p)).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "leaders": self.leaders().iter().map(|l| json!({
                "key": l.key, "forward": path_str(&l.forward), "reverse": l.reverse.as_deref().map(path_str),
            })).collect::<Vec<_>>(),
            "wonders": self.plan.wonders.values().map(|p| json!({"key": p.key, "path": path_str(&p.path)})).collect::<Vec<_>>(),
            "techs": self.plan.techs.iter().flatten().map(|p| json!({"key": p.key, "path": path_str(&p.path)})).collect::<Vec<_>>(),
        })
    }

    /// How many items are missing or stale.
    fn stale(&self, index: &Index) -> usize {
        let root = &self.install.root;
        let search: Vec<String> = self.install.search.iter().map(|p| path_str(p)).collect();
        let colors = json!(self.colors);
        let mut n = 0;
        for s in STAGES {
            let want_search = (*s == "diplomacy" || *s == "cities").then_some(search.as_slice());
            n += usize::from(!index.fresh(root, &format!("stage:{s}"), &Value::Null, want_search));
        }
        n += self
            .plan
            .units
            .iter()
            .filter(|u| !index.fresh(root, &u.key, &colors, None))
            .count();
        n += self
            .leaders()
            .iter()
            .filter(|l| !index.fresh(root, &l.key, &Value::Null, None))
            .count();
        let pics = self
            .plan
            .wonders
            .values()
            .chain(self.plan.techs.iter().flatten());
        n += pics
            .filter(|p| !index.fresh(root, &format!("{}.png", p.key), &Value::Null, None))
            .count();
        n
    }
}

fn have(cmd: &str, args: &[&str]) -> bool {
    Command::new(cmd)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Convert what the match needs and the cache lacks. `Err` is a message for
/// the user (a tool is missing, or the script failed).
#[cfg(not(target_arch = "wasm32"))]
pub fn ensure(wanted: &Wanted) -> Result<(), String> {
    let script = std::fs::read(SCRIPT)
        .map_err(|e| format!("{SCRIPT}: {e} (run from the repository root)"))?;
    let index = read_index(&fnv(&script));
    let stale = wanted.stale(&index);
    if stale == 0 {
        return Ok(());
    }
    if !have("python3", &["--version"]) {
        return Err("the art cache needs Python 3 (python3 is not on the PATH)".into());
    }
    if !have("python3", &["-c", "import PIL"]) {
        return Err("the art cache needs the Python imaging library: pip install pillow".into());
    }
    if !have("ffmpeg", &["-version"]) {
        return Err("the art cache needs ffmpeg (it is not on the PATH)".into());
    }
    std::fs::create_dir_all(CACHE).map_err(|e| format!("{CACHE}: {e}"))?;
    let request = Path::new(CACHE).join("request.json");
    std::fs::write(
        &request,
        serde_json::to_string_pretty(&wanted.request()).expect("the request serializes"),
    )
    .map_err(|e| format!("{}: {e}", request.display()))?;
    eprintln!(
        "open-4x: converting {stale} art item(s) into {CACHE}/ (the first run takes a while)"
    );
    let status = Command::new("python3")
        .args([SCRIPT, "--request"])
        .arg(&request)
        .status()
        .map_err(|e| format!("python3 {SCRIPT}: {e}"))?;
    if !status.success() {
        return Err(format!("{SCRIPT} failed ({status})"));
    }
    Ok(())
}

#[cfg(target_arch = "wasm32")]
pub fn ensure(_wanted: &Wanted) -> Result<(), String> {
    crate::web::read_text(Path::new(CACHE).join("request.json"))
        .map(|_| ())
        .map_err(|e| format!("{CACHE}/request.json: {e}"))
}

/// The team colors and civs of the match in play, for [`Wanted`].
pub fn in_play() -> (Vec<usize>, Vec<u8>) {
    let roster: Vec<usize> = crate::civs::players();
    let mut colors: Vec<u8> = vec![0];
    for &r in &roster {
        colors.push(crate::ruleset::CIV_ROSTER[r].team_color);
    }
    colors.sort_unstable();
    colors.dedup();
    (roster, colors)
}

/// Whether the cache was last built for these rules (its request held every
/// unit folder and picture the plan names), so a test of the converted art
/// has something to check; a cache built for another game is skipped.
#[cfg(test)]
pub fn cache_covers_plan() -> bool {
    let Ok(text) = std::fs::read_to_string(Path::new(CACHE).join("request.json")) else {
        return false;
    };
    let Ok(req) = serde_json::from_str::<Value>(&text) else {
        return false;
    };
    let keys = |kind: &str| -> Vec<String> {
        req[kind]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|i| i["key"].as_str().map(str::to_string))
            .collect()
    };
    let plan = &crate::ruleset::get().art;
    let (units, wonders, techs) = (keys("units"), keys("wonders"), keys("techs"));
    plan.units.iter().all(|u| units.contains(&u.key))
        && plan.wonders.values().all(|p| wonders.contains(&p.key))
        && plan.techs.iter().flatten().all(|p| techs.contains(&p.key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_the_lowercased_path_under_the_install() {
        let install = Install {
            root: PathBuf::from("/civ3"),
            search: vec![],
        };
        assert_eq!(
            key_of(&install, Path::new("/civ3/Conquests/Art/Units/Warrior")),
            "conquests/art/units/warrior"
        );
        assert_eq!(
            key_of(&install, Path::new("/elsewhere/Mod/Art/X")),
            "elsewhere/mod/art/x"
        );
    }

    #[test]
    fn switching_source_roots_invalidates_identical_entries() {
        let a = std::env::temp_dir().join(format!("open4x-cache-root-{}", std::process::id()));
        let b = a.join("other");
        std::fs::create_dir_all(&b).unwrap();
        for root in [&a, &b] {
            let file = std::fs::File::create(root.join("source")).unwrap();
            file.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1000))
                .unwrap();
        }
        let entry = json!({"root": path_str(&a.canonicalize().unwrap()),
            "outputs": [], "sources": [{"path": "source", "size": 0, "mtime": 1000}], "params": null, "search": []});
        let index: Index =
            serde_json::from_value(json!({"script": "", "entries": {"stage:test": entry}}))
                .unwrap();
        assert!(index.fresh(&a, "stage:test", &Value::Null, Some(&[])));
        assert!(!index.fresh(&b, "stage:test", &Value::Null, Some(&[])));
        let legacy: Index = serde_json::from_value(json!({"script": "", "entries": {
            "stage:test": {"outputs": [], "sources": [], "params": null}
        }}))
        .unwrap();
        assert!(!legacy.fresh(&a, "stage:test", &Value::Null, None));
        std::fs::remove_dir_all(a).unwrap();
    }

    #[test]
    fn the_hash_is_fnv_1a() {
        assert_eq!(fnv(b""), "cbf29ce484222325");
        assert_eq!(fnv(b"a"), "af63dc4c8601ec8c");
    }

    #[test]
    fn the_plan_finds_art_for_the_stock_rules() {
        let art = &crate::ruleset::get().art;
        assert!(
            art.unit_key("warrior").is_some(),
            "{:?}",
            art.unit_keys.keys().collect::<Vec<_>>()
        );
        assert!(art.unit_key("Settler").is_some());
        assert!(
            art.leaders.iter().flatten().flatten().count() > 100,
            "four clips for most civs"
        );
        assert!(art.techs.iter().flatten().count() > 80);
    }
}
