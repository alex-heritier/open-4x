//! Renders a generated map to the terminal, and can dump it as a PGM.
//!
//! ```text
//! cargo run --release -- --size 2 --water 50 --climate 1 --resources 1
//! cargo run --release -- --out map.pgm --size 4 --temperature 2
//! ```

use std::io::Write;

use civ3mapgen::cell::{is_water, MapGrid};
use civ3mapgen::options::Options;
use civ3mapgen::pipeline::generate_with;
use civ3mapgen::OriginalBugs;

/// One character per cell, north to south.
fn glyph(c: u8) -> char {
    match c {
        0 => '.',  // deep water
        1 => ',',  // shallow water
        2 => '"',  // grass
        3 => 'w',  // forest
        4 => 'o',  // ocean / shelf
        5 => 'g',  // grassland
        6 => 'h',  // hills
        7 => 'H',  // hill ridge
        8 => '~',  // wet
        9 => 'j',  // jungle
        10 => '-', // plains
        11 => ':', // lake
        12 => '=', // deep lake
        _ => '#',  // 13, abyssal
    }
}

fn render(grid: &MapGrid) -> String {
    let mut s = String::with_capacity(((grid.w * 2) * grid.h) as usize);
    for y in 0..grid.h {
        for x in 0..grid.w {
            s.push(glyph(grid.cell_at(x, y).map_or(b'.', |c| c.class())));
        }
        s.push('\n');
    }
    s
}

/// Writes a greyscale PGM, one pixel per cell.
fn write_pgm(path: &str, grid: &MapGrid) -> std::io::Result<()> {
    let mut out = format!("P2\n{} {}\n255\n", grid.w, grid.h);
    for y in 0..grid.h {
        for x in 0..grid.w {
            let c = grid.cell_at(x, y).map_or(0, |c| c.class());
            // Land bright, water dark.
            let v = if is_water(c) { 20 } else { 200 };
            out.push_str(&format!("{v} "));
        }
        out.push('\n');
    }
    std::fs::File::create(path)?.write_all(out.as_bytes())
}

/// Parses `--bugs`, accepting `all`, `none`, or a comma-separated subset.
fn parse_bugs(spec: &str) -> Result<OriginalBugs, String> {
    let spec = spec.trim();
    if spec.eq_ignore_ascii_case("all") {
        return Ok(OriginalBugs::ALL);
    }
    if spec.eq_ignore_ascii_case("none") {
        return Ok(OriginalBugs::NONE);
    }
    let mut mask = 0u32;
    for name in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if !OriginalBugs::NAMES.contains(&name) {
            return Err(format!(
                "unknown bug {name:?}; expected `all`, `none`, or a subset of {}",
                OriginalBugs::NAMES.join(", ")
            ));
        }
        mask |= 1 << OriginalBugs::NAMES.iter().position(|n| *n == name).unwrap();
    }
    Ok(OriginalBugs::from(mask))
}

fn main() {
    let mut opts = Options::default();
    let mut out: Option<String> = None;
    // Faithful to the binary by default, since that is what this tool is for.
    let mut bugs = OriginalBugs::ALL;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut v = || args.next().expect("a value after the flag");
        match a.as_str() {
            "--size" => opts.size = v().parse().expect("a number"),
            "--seed" => opts.seed = v().parse().expect("a number"),
            "--ocean" => opts.ocean = v().parse().expect("a number"),
            "--climate" => opts.climate = v().parse().expect("a number"),
            "--temperature" => opts.temperature = v().parse().expect("a number"),
            "--age" => opts.age = v().parse().expect("a number"),
            "--landmass" => opts.landmass = v().parse().expect("a number"),
            "--out" => out = Some(v()),
            "--bugs" => bugs = parse_bugs(&v()).unwrap_or_else(|e| panic!("{e}")),
            "-h" | "--help" => {
                println!(
                    "usage: civ3mapgen [--size N] [--seed N] [--ocean N] [--climate N] \\
                         [--temperature N] [--age N] [--landmass N] \\
                         [--out FILE.pgm] [--bugs SPEC]\n\n\
                         --bugs takes `all` (the default), `none`, or a comma-separated\n\
                         subset of: {}",
                    OriginalBugs::NAMES.join(", ")
                );
                return;
            }
            other => panic!("unknown flag {other}"),
        }
    }

    let map = generate_with(&opts, &bugs);
    print!("{}", render(&map.grid));

    eprintln!(
        "{:>4}x{:<4} cells   land {:>3}%   draws {}   coast h={} sea h={} lake h={}   bugs {}/{}",
        map.grid.w,
        map.grid.h,
        100 * map.land_cells() / map.grid.num_cells(),
        map.draws,
        map.thresholds.coast,
        map.thresholds.sea,
        map.thresholds.lake,
        bugs.count(),
        OriginalBugs::NAMES.len(),
    );
    if let Some(path) = out {
        write_pgm(&path, &map.grid).expect("writing the PGM");
        eprintln!("wrote {path}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_and_none_parse() {
        assert_eq!(parse_bugs("all").unwrap(), OriginalBugs::ALL);
        assert_eq!(parse_bugs("NONE").unwrap(), OriginalBugs::NONE);
        assert_eq!(parse_bugs(" all ").unwrap(), OriginalBugs::ALL);
    }

    #[test]
    fn a_single_name_parses() {
        assert_eq!(
            parse_bugs("sea-level-split").unwrap(),
            OriginalBugs {
                sea_level_split: true,
                ..OriginalBugs::NONE
            }
        );
    }

    #[test]
    fn a_subset_parses() {
        assert_eq!(
            parse_bugs("start-slot-index, swapped-wrap-flags").unwrap(),
            OriginalBugs {
                start_slot_index: true,
                swapped_wrap_flags: true,
                ..OriginalBugs::NONE
            }
        );
    }

    #[test]
    fn an_empty_spec_is_none() {
        assert_eq!(parse_bugs("").unwrap(), OriginalBugs::NONE);
        assert_eq!(parse_bugs(" , ").unwrap(), OriginalBugs::NONE);
    }

    #[test]
    fn an_unknown_name_is_rejected() {
        let err = parse_bugs("no-such-bug").unwrap_err();
        assert!(err.contains("no-such-bug"), "{err}");
        assert!(err.contains("start-slot-index"), "the error should list the names: {err}");
    }
}
