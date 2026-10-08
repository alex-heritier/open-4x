//! Ground truth that does not come from the emulator: a shipped scenario whose
//! map is the exe's own generator output.
//!
//! A scenario made with the generator stores the generator's inputs: `WMAP`
//! holds the seed (the field the editor calls the Oceans level, which is a full
//! 32-bit seed in these files), the size and the wrap flags, and `WCHR` holds
//! the resolved world-setup sliders. Running the land/sea stage on them from
//! the first draw, as the exe does, and comparing the water mask against the
//! `TILE` rows measures how faithful the port is (`NOTES.md` section 19):
//! `dinobarbs.bix` agrees on 99.8 % of its tiles; the rest is the lake stage and
//! later hand edits.
//!
//! `Strategic Conquest (Small).bix` is not generator output (71 % at its stored
//! seed and sliders, which is what an unrelated map gives), so it is not an
//! anchor.
//!
//! The corpus is git-ignored (`civ3/`, or `CIV3_DIR`); without it these tests
//! return early. The exact, per-stage comparison is `tests/oracle.rs`.

use civ3_biq::Biq;
use civ3mapgen::bugs::OriginalBugs;
use civ3mapgen::landmass::{generate_landmass_wrapped, separate_continents};
use civ3mapgen::options::Options;
use std::path::{Path, PathBuf};

/// First file called `name` under `dir`, skipping hidden directories and the
/// RE scratch tree.
fn find(dir: &Path, name: &str) -> Option<PathBuf> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            let n = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if n.starts_with('.') || n == "re" {
                continue;
            }
            if let Some(f) = find(&p, name) {
                return Some(f);
            }
        } else if p.file_name().and_then(|s| s.to_str()) == Some(name) {
            return Some(p);
        }
    }
    None
}

/// A shipped generated map and the generator inputs stored beside it.
struct Shipped {
    width: i32,
    height: i32,
    wrap: u32,
    seed: i32,
    landform: i32,
    ocean: i32,
    size_index: i32,
    water: Vec<bool>,
}

fn load(name: &str) -> Option<Shipped> {
    let root = std::env::var("CIV3_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../civ3"));
    let path = find(&root, name)?;
    let biq = Biq::read_file(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let view = biq.map_view()?;
    let wm = biq.map.world_map.first()?;
    let wc = biq.map.characteristics.first()?;
    Some(Shipped {
        width: wm.width,
        height: wm.height,
        wrap: wm.wrap_flags as u32,
        seed: wm.water_level,
        landform: wc.landform_actual,
        ocean: wc.ocean_coverage_actual,
        size_index: wc.world_size_index,
        water: biq.map.tiles.iter().map(|t| view.is_water(t)).collect(),
    })
}

/// Fraction of cells whose water/land status matches, when the whole land/sea
/// stage runs on the scenario's size, wrap flags and sliders with `seed` and the
/// Ocean Coverage row `ocean`.
fn compare(s: &Shipped, seed: i32, ocean: i32) -> f64 {
    let opts = Options {
        seed,
        landmass: s.landform.clamp(0, 2),
        ocean,
        ..Options::default()
    };
    let mut lm = generate_landmass_wrapped(
        s.width,
        s.height,
        s.wrap,
        &opts,
        seed,
        OriginalBugs { swapped_wrap_flags: true, ..OriginalBugs::NONE },
    );
    if opts.landmass == 1 {
        separate_continents(&mut lm.grid);
    }
    assert_eq!(lm.grid.cells.len(), s.water.len());
    let same = lm
        .grid
        .cells
        .iter()
        .zip(&s.water)
        .filter(|(c, w)| c.is_water() == **w)
        .count();
    same as f64 / s.water.len() as f64
}

/// `(file, minimum agreement at the scenario's own seed and sliders)`.
const ANCHORS: [(&str, f64); 1] = [("dinobarbs.bix", 0.99)];

#[test]
fn generated_scenarios_match_at_their_own_seed() {
    let mut ran = 0;
    for (name, min) in ANCHORS {
        let Some(s) = load(name) else { continue };
        let agree = compare(&s, s.seed, s.ocean);
        assert!(agree >= min, "{name}: only {:.1} % of cells agree", agree * 100.0);
        ran += 1;
    }
    if ran == 0 {
        eprintln!("no scenario corpus found; nothing checked");
    }
}

/// How far the scenario's own seed must beat every other seed and slider.
const MARGIN: f64 = 0.10;

/// With another seed or another Ocean Coverage row the agreement falls to what
/// two unrelated maps with a similar ocean fraction give (47-85 %), which makes
/// the test above evidence rather than a tautology.
#[test]
fn wrong_seed_or_wrong_slider_does_not_match() {
    for (name, _) in ANCHORS {
        let Some(s) = load(name) else { continue };
        let best = compare(&s, s.seed, s.ocean);
        for d in 1..=20 {
            let agree = compare(&s, s.seed + d, s.ocean);
            assert!(
                agree + MARGIN < best,
                "{name}: seed + {d} agrees {:.1} % against {:.1} %",
                agree * 100.0,
                best * 100.0
            );
        }
        // Some rows of the percentile tables coincide for a landmass style (rows 2
        // and 3 do for this pangaea), so a row may give the same map; it may not
        // give a similar but different one.
        let mut worse = 0;
        for ocean in (0..=4).filter(|&o| o != s.ocean) {
            let agree = compare(&s, s.seed, ocean);
            if (agree - best).abs() < 1e-9 {
                continue;
            }
            assert!(
                agree + MARGIN < best,
                "{name}: ocean row {ocean} agrees {:.1} % against {:.1} %",
                agree * 100.0,
                best * 100.0
            );
            worse += 1;
        }
        assert!(worse >= 2, "{name}: the slider check is vacuous");
    }
}
