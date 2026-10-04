//! The port against the game's own generator.
//!
//! Each fixture in `tests/data/oracle/` is `Map::generate` of the real exe, run
//! under emulation (`reverse-engineering/tools/mapgen`), with the cell planes
//! recorded at the entry of every `generateMap` stage. A stage test feeds the
//! snapshot at the stage's entry to the port and compares the result with the
//! snapshot at the entry of the next stage, so a mistake in one stage cannot
//! hide behind, or be blamed on, another.

use civ3mapgen::bugs::OriginalBugs;
use civ3mapgen::landmass::generate_landmass_wrapped;
use civ3mapgen::oracle::{self, Fixture, Planes, PLANE_NAMES};

fn fixtures() -> Vec<Fixture> {
    let names = oracle::fixture_names();
    assert!(!names.is_empty(), "no fixtures in {}", oracle::fixture_dir().display());
    names.iter().map(|n| oracle::load(n).unwrap_or_else(|e| panic!("{n}: {e}"))).collect()
}

/// Asserts that no plane differs, naming the fixture and stage.
fn assert_same(f: &Fixture, stage: &str, got: &Planes, want: &Planes, names: &[&str]) {
    let d = got.diff(want, names);
    assert!(
        d.is_empty(),
        "{} / {stage}: {}",
        f.name,
        d.iter().map(|p| p.to_string()).collect::<Vec<_>>().join("; ")
    );
}

#[test]
fn rolled_options_match_the_exe() {
    for f in fixtures() {
        assert_eq!(f.raw.resolve(f.options.seed, f.options.size), f.options, "{}", f.name);
    }
}

#[test]
fn landmass_stage_matches_the_exe() {
    for f in fixtures() {
        let lm = generate_landmass_wrapped(
            f.width,
            f.height,
            f.wrap,
            &f.options,
            f.options.seed,
            OriginalBugs { swapped_wrap_flags: true, ..OriginalBugs::NONE },
        );
        let got = Planes::from_grid(&lm.grid);
        assert_same(&f, "landmass", &got, &f.after("landmass").planes, &PLANE_NAMES);
    }
}

/// Runs `stage` on the snapshot at its entry and compares with the snapshot
/// after it. `names` selects the planes that must match.
///
/// A stage test that compared two identical snapshots would pass for any code,
/// so the total number of cells the exe's stage changed must be non-zero.
fn check_stage(stage: &str, names: &[&str], run: impl Fn(&Fixture, &mut civ3mapgen::MapGrid)) {
    let mut changed = 0;
    let mut ran = 0;
    for f in fixtures() {
        if !f.has_stage(stage) {
            continue;
        }
        ran += 1;
        let entry = &f.stage(stage).planes;
        let mut grid = entry.to_grid(f.width, f.height, f.wrap, f.options.seed);
        run(&f, &mut grid);
        let got = Planes::from_grid(&grid);
        let want = &f.after(stage).planes;
        assert_same(&f, stage, &got, want, names);
        changed += entry.diff(want, names).iter().map(|d| d.count).sum::<usize>();
    }
    assert!(ran > 0, "{stage}: no fixture runs this stage");
    assert!(changed > 0, "{stage}: the exe changed nothing in any fixture, so this proves nothing");
    println!("{stage}: matches in {ran} fixtures, {changed} plane entries changed");
}

#[test]
fn coast_stage_matches_the_exe() {
    check_stage("coast", &PLANE_NAMES, |_, grid| {
        civ3mapgen::coast::classify_water_depth(grid)
    });
}

#[test]
fn relief_stage_matches_the_exe() {
    check_stage("relief", &PLANE_NAMES, |f, grid| {
        civ3mapgen::pipeline::convert_deserts_at_starts(grid, &f.options);
    });
}

#[test]
fn biome_stage_matches_the_exe() {
    check_stage("biomes", &PLANE_NAMES, |f, grid| {
        let regions: Vec<u16> = f.stage("biomes").planes.region.iter().map(|&r| r as u16).collect();
        assert_eq!(regions.len(), grid.num_cells(), "{}: region plane", f.name);
        civ3mapgen::biomes::assign_biomes(grid, f.options.seed, f.options.temperature, f.options.climate, &regions);
    });
}

#[test]
fn region_map_matches_the_exe() {
    let mut painted = 0;
    for f in fixtures() {
        let grid = f.stage("paint_continents").planes.to_grid(f.width, f.height, f.wrap, f.options.seed);
        let got = civ3mapgen::regions::paint_regions(&grid, f.options.seed);
        let want: Vec<u16> = f.stage("biomes").planes.region.iter().map(|&r| r as u16).collect();
        assert_eq!(got.len(), want.len(), "{}", f.name);
        let bad: Vec<usize> = (0..got.len()).filter(|&i| got[i] != want[i]).collect();
        assert!(
            bad.is_empty(),
            "{}: {} of {} region ids differ, first at cell {:?} (got {:#x}, exe {:#x})",
            f.name,
            bad.len(),
            got.len(),
            bad.first().map(|&i| grid.coords(i)),
            got[bad[0]],
            want[bad[0]]
        );
        painted += want.iter().filter(|&&r| r != 0xFFFF).count();
    }
    assert!(painted > 0, "the fixtures carry no region ids");
}

#[test]
fn landmass_fix_matches_the_exe() {
    check_stage("landmass_fix", &PLANE_NAMES, |_, grid| civ3mapgen::landmass::separate_continents(grid));
}

#[test]
fn lake_stage_matches_the_exe() {
    check_stage("lakes", &PLANE_NAMES, |f, grid| civ3mapgen::lakes::add_lakes(grid, f.options.seed));
}

#[test]
fn post_process_stage_matches_the_exe() {
    check_stage("post_process", &PLANE_NAMES, |f, grid| {
        civ3mapgen::art::post_process(grid, f.options.seed);
    });
}

/// The continent records `place_resources` reads: the fixture's own, which are
/// what a fresh numbering of the same cells gives (checked in `continents_are_current`).
fn continent_records(f: &Fixture, stage: &str) -> Vec<civ3mapgen::continents::Continent> {
    f.stage(stage)
        .continents
        .as_ref()
        .expect("continent records")
        .iter()
        .map(|&(is_land, size)| civ3mapgen::continents::Continent { is_land, size })
        .collect()
}

#[test]
fn resource_stage_matches_the_exe() {
    check_stage("resources", &PLANE_NAMES, |f, grid| {
        let continents = continent_records(f, "resources");
        civ3mapgen::placement::place_resources(grid, &f.rules, &continents, f.options.seed, f.civs);
    });
}

#[test]
fn hut_stage_matches_the_exe() {
    check_stage("huts", &PLANE_NAMES, |f, grid| {
        civ3mapgen::placement::place_huts(grid, f.options.seed, f.options.barbarians);
    });
}

#[test]
fn bonus_grassland_stage_matches_the_exe() {
    check_stage("bonus_grassland", &PLANE_NAMES, |f, grid| {
        civ3mapgen::placement::bonus_grassland(grid, f.options.seed);
    });
}

/// The stage's entry state with the continent records and the exe's own site values.
fn start_inputs(f: &Fixture) -> (civ3mapgen::MapGrid, Vec<civ3mapgen::continents::Continent>, Vec<i32>) {
    let entry = f.stage("final_pass");
    let grid = entry.planes.to_grid(f.width, f.height, f.wrap, f.options.seed);
    let values = entry.planes.value.iter().map(|&v| v as i32).collect();
    (grid, continent_records(f, "final_pass"), values)
}

#[test]
fn shore_rank_matches_the_exe() {
    let mut checked = 0;
    for f in fixtures() {
        let (grid, continents, _) = start_inputs(&f);
        let want = &f.stage("final_pass").planes.shore;
        for i in 0..grid.num_cells() {
            let (x, y) = grid.coords(i);
            let got = civ3mapgen::starts::shore_rank(&grid, &continents, x, y);
            assert_eq!(got, want[i] as i32, "{} cell {i} ({x},{y})", f.name);
            checked += usize::from(got != -1);
        }
    }
    assert!(checked > 1000, "only {checked} tiles had a shore, the test proves little");
}

#[test]
fn start_stage_matches_the_exe() {
    let mut starts = 0;
    for f in fixtures() {
        let (mut grid, continents, values) = start_inputs(&f);
        let mut slots = [-1; civ3mapgen::starts::SLOTS];
        let args = civ3mapgen::starts::Args::generate(!f.multiplayer, f.seafarers);
        civ3mapgen::starts::final_pass(&mut grid, &args, f.civs, f.radius, &continents, &f.rules, &values, &mut slots);
        let got = Planes::from_grid(&grid);
        let want = f.after("final_pass");
        assert_same(&f, "final_pass", &got, &want.planes, &PLANE_NAMES);
        assert_eq!(slots.to_vec(), want.slots, "{}: start slots", f.name);
        starts += slots.iter().filter(|&&s| s != -1).count();
    }
    assert!(starts > 100, "only {starts} starts were placed");
}

#[test]
fn river_stage_matches_the_exe() {
    check_stage("rivers", &PLANE_NAMES, |f, grid| {
        civ3mapgen::rivergen::grow_rivers(grid, f.options.seed);
        civ3mapgen::rivergen::flood_deserts(grid);
    });
}
