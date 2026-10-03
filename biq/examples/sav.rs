//! Inspect a Civilization III Conquests saved game.
//!
//! ```text
//! cargo run --release --example sav -- FILE            # overview
//! cargo run --release --example sav -- FILE --check    # byte-exact round trip
//! cargo run --release --example sav -- FILE --cities   # city list
//! cargo run --release --example sav -- FILE --biq OUT  # extract the embedded scenario
//! cargo run --release --example sav -- FILE --stream OUT  # the decoded stream (DCL removed)
//! cargo run --release --example sav -- FILE --sub N OUT   # stream laid out for sub-version N (loader tests)
//! ```
use civ3_biq::Save;
use civ3_biq::raw::Raw;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: sav FILE [--check|--cities|--biq OUT|--stream OUT|--sub N OUT]");
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
    let save = match Save::parse(&bytes) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::from(1);
        }
    };
    match mode.as_str() {
        "--check" => return check(&bytes, &save),
        "--cities" => cities(&save),
        "--stream" => {
            let Some(out) = args.next() else {
                eprintln!("--stream needs an output path");
                return ExitCode::from(2);
            };
            let data = match Raw::parse(&bytes) {
                Ok(r) => r.data,
                Err(e) => {
                    eprintln!("{path}: {e}");
                    return ExitCode::from(1);
                }
            };
            if let Err(e) = std::fs::write(&out, &data) {
                eprintln!("{out}: {e}");
                return ExitCode::from(1);
            }
            println!("{} stream bytes written to {out}", data.len());
        }
        "--sub" => {
            let (Some(n), Some(out)) = (args.next(), args.next()) else {
                eprintln!("--sub needs a sub-version and an output path");
                return ExitCode::from(2);
            };
            let stream = n
                .parse::<u32>()
                .map_err(|e| e.to_string())
                .and_then(|n| save.with_sub_version(n).map_err(|e| e.to_string()))
                .and_then(|s| s.to_stream().map_err(|e| e.to_string()));
            match stream {
                Ok(data) => {
                    if let Err(e) = std::fs::write(&out, &data) {
                        eprintln!("{out}: {e}");
                        return ExitCode::from(1);
                    }
                    println!("{} stream bytes written to {out}", data.len());
                }
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::from(1);
                }
            }
        }
        "--biq" => {
            let Some(out) = args.next() else {
                eprintln!("--biq needs an output path");
                return ExitCode::from(2);
            };
            if let Err(e) = std::fs::write(&out, &save.biq) {
                eprintln!("{out}: {e}");
                return ExitCode::from(1);
            }
            println!("{} bytes of BICQ written to {out}", save.biq.len());
        }
        _ => overview(&path, &save),
    }
    ExitCode::SUCCESS
}

fn overview(path: &str, s: &Save) {
    let h = &s.header;
    println!("{path}");
    println!(
        "  format {}.{}, {}, guid {}",
        h.version,
        h.sub_version,
        if s.storage == civ3_biq::dcl::Storage::Plain {
            "plain"
        } else {
            "DCL-compressed"
        },
        h.guid
            .map(|g| g.iter().map(|b| format!("{b:02x}")).collect::<String>())
            .unwrap_or_else(|| "none".into())
    );
    println!(
        "  scenario: {} bytes embedded ({:?} / {:?}), rule counts {:?}",
        s.biq.len(),
        s.scenario_dir(),
        s.scenario_file(),
        s.counts
    );
    let year = s.year();
    println!(
        "  turn {}, year {}",
        s.game.turn(),
        if year < 0 {
            format!("{} BC", -year)
        } else {
            format!("{year} AD")
        }
    );
    let m = &s.map;
    println!(
        "  map {}x{} ({} tiles), {} continents, wrap flags {:#x}",
        m.width(),
        m.height(),
        m.tiles.len(),
        m.continent_count.u16(0),
        m.wrap_flags()
    );
    println!(
        "  {} units, {} cities, {} colonies, {} replay turns, {} history records",
        s.units.len(),
        s.cities.len(),
        s.colonies.len(),
        s.replay.turns.len(),
        s.history.records.len()
    );
    println!("  slot  race  gov  gold  cities  units");
    for (slot, p) in s.players.iter().enumerate().filter(|(_, p)| p.in_use()) {
        let cities = s
            .cities
            .iter()
            .filter(|c| c.owner() as usize == slot)
            .count();
        let units = s
            .units
            .iter()
            .filter(|u| u.owner() as usize == slot)
            .count();
        println!(
            "  {slot:>4}  {:>4}  {:>3}  {:>4}  {cities:>6}  {units:>5}",
            p.race(),
            p.government(),
            p.gold()
        );
    }
}

fn cities(s: &Save) {
    println!("   id  owner  size   x   y  name");
    for c in &s.cities {
        println!(
            "{:>5}  {:>5}  {:>4}  {:>3} {:>3}  {}",
            c.id(),
            c.owner(),
            c.size(),
            c.x(),
            c.y(),
            c.name()
        );
    }
}

fn check(bytes: &[u8], s: &Save) -> ExitCode {
    let stream = match s.to_stream() {
        Ok(v) => v,
        Err(e) => {
            println!("cannot write: {e}");
            return ExitCode::from(1);
        }
    };
    let want = match Raw::parse(bytes) {
        Ok(r) => r.data,
        Err(e) => {
            println!("{e}");
            return ExitCode::from(1);
        }
    };
    if stream != want {
        let at = stream.iter().zip(&want).position(|(a, b)| a != b);
        println!(
            "stream differs at {at:?} (wrote {}, read {})",
            stream.len(),
            want.len()
        );
        return ExitCode::from(1);
    }
    match s.to_bytes() {
        Ok(b) if b == bytes => {
            println!(
                "round trip exact: {} stream bytes, {} file bytes",
                stream.len(),
                b.len()
            );
            ExitCode::SUCCESS
        }
        Ok(b) => {
            println!(
                "stream exact, but the file differs ({} vs {} bytes)",
                b.len(),
                bytes.len()
            );
            ExitCode::from(1)
        }
        Err(e) => {
            println!("cannot encode: {e}");
            ExitCode::from(1)
        }
    }
}
