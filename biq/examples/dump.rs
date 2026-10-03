//! Inspect a Civilization III scenario file.
//!
//! ```text
//! cargo run --release --example dump -- FILE            # overview
//! cargo run --release --example dump -- FILE --check    # per-section re-encode check
//! cargo run --release --example dump -- FILE --debug    # Debug-print everything
//! cargo run --release --example dump -- FILE --map      # ASCII terrain map
//! cargo run --release --example dump -- FILE --stream OUT  # the decoded stream (DCL removed)
//! ```
use civ3_biq::Biq;
use civ3_biq::raw::Raw;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: dump FILE [--check|--debug|--map]");
        return ExitCode::from(2);
    };
    let mode = args.next().unwrap_or_default();
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::from(1);
        }
    };
    let raw = match Raw::parse(&bytes) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::from(1);
        }
    };
    let biq = match Biq::from_raw(&raw) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::from(1);
        }
    };
    match mode.as_str() {
        "--debug" => println!("{biq:#?}"),
        "--check" => return check(&raw, &biq),
        "--map" => ascii_map(&biq),
        "--stream" => {
            let Some(out) = args.next() else {
                eprintln!("--stream needs an output path");
                return ExitCode::from(2);
            };
            if let Err(e) = std::fs::write(&out, &raw.data) {
                eprintln!("{out}: {e}");
                return ExitCode::from(1);
            }
            println!("{} stream bytes written to {out}", raw.data.len());
        }
        _ => overview(&path, &raw, &biq),
    }
    ExitCode::SUCCESS
}

fn overview(path: &str, raw: &Raw, biq: &Biq) {
    println!("{path}");
    println!(
        "  magic {:?}, {}, version {}",
        raw.magic,
        if raw.compressed() {
            "DCL-compressed"
        } else {
            "plain"
        },
        biq.version()
    );
    if let Some(h) = &biq.header {
        println!("  title       {}", h.title);
        println!("  description {}", first_line(&h.description.text()));
    }
    println!("  sections (file order):");
    for tag in &biq.section_order {
        let t = String::from_utf8_lossy(tag);
        let n = raw.section(tag).map_or(0, |s| s.rows.len());
        let bytes = raw.section(tag).map_or(0, |s| s.end - s.start);
        println!("    {t}  {n:>5} rows  {bytes:>8} bytes");
    }
    println!("  unmodelled bytes: {}", biq.unmodelled_bytes());
    let r = &biq.rules;
    println!(
        "  rules: {} buildings, {} unit types, {} techs, {} civilizations, {} governments, {} terrains, {} resources",
        r.buildings.len(),
        r.unit_types.len(),
        r.techs.len(),
        r.civilizations.len(),
        r.governments.len(),
        r.terrains.len(),
        r.goods.len()
    );
    if let Some(m) = biq.map_view() {
        println!(
            "  map: {} x {} ({} tiles), wrap x={} y={}, {} continents, {} start locations",
            m.width,
            m.height,
            biq.map.tiles.len(),
            m.wrap_x,
            m.wrap_y,
            biq.map.continents.len(),
            biq.map.start_locations.len()
        );
    }
    let s = &biq.scenario;
    if !s.players.is_empty() || !s.cities.is_empty() || !s.units.is_empty() {
        println!(
            "  scenario: {} players, {} cities, {} units, {} colonies",
            s.players.len(),
            s.cities.len(),
            s.units.len(),
            s.colonies.len()
        );
    }
}

fn first_line(s: &str) -> String {
    let l = s.lines().next().unwrap_or("");
    if l.len() > 100 {
        format!("{}...", &l[..100])
    } else {
        l.to_string()
    }
}

/// Re-encode and compare section by section; prints the first difference.
fn check(raw: &Raw, biq: &Biq) -> ExitCode {
    let out = biq.to_stream();
    if out == raw.data {
        println!("OK: {} bytes re-encode exactly", out.len());
        return ExitCode::SUCCESS;
    }
    let again = match Raw::parse(&out) {
        Ok(r) => r,
        Err(e) => {
            println!("re-encoded stream does not parse: {e}");
            return ExitCode::from(1);
        }
    };
    for (a, b) in raw.sections.iter().zip(&again.sections) {
        let (ba, bb) = (&raw.data[a.start..a.end], &again.data[b.start..b.end]);
        if ba != bb {
            let at = ba.iter().zip(bb).position(|(x, y)| x != y).unwrap_or(0);
            println!(
                "section {} differs: {} -> {} bytes, first difference at +{at}",
                a.tag_str(),
                ba.len(),
                bb.len()
            );
            for (i, (ra, rb)) in a.rows.iter().zip(&b.rows).enumerate() {
                let (x, y) = (raw.row(ra), again.row(rb));
                if x != y {
                    println!("  row {i}: {} -> {} bytes", x.len(), y.len());
                    break;
                }
            }
            return ExitCode::from(1);
        }
    }
    println!("sections equal but streams differ (order or count)");
    ExitCode::from(1)
}

/// One character per tile: the terrain id as a hex digit (in the numbering of
/// the file; see `Biq::terrain_numbering`), water as `~`.
fn ascii_map(biq: &Biq) {
    let Some(map) = biq.map_view() else {
        println!("no map in this file");
        return;
    };
    for y in 0..map.height {
        let mut line = String::new();
        for x in 0..map.width {
            line.push(match map.tile(x, y) {
                None => ' ',
                Some(t) if map.is_water(t) => '~',
                Some(t) => char::from_digit(u32::from(t.terrain_id()) & 15, 16).unwrap_or('?'),
            });
        }
        println!("{line}");
    }
}
