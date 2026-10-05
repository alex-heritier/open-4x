//! Tests over the saves of the install (`civ3/**/Saves/*.SAV`). Like the other
//! corpus tests they do nothing when the git-ignored install is absent.

use super::*;
use crate::corpus;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;

fn saves() -> Vec<(PathBuf, Vec<u8>)> {
    let mut paths = Vec::new();
    corpus::walk_ext(&corpus::install_root(), &["sav"], &mut paths);
    paths
        .into_iter()
        .filter_map(|p| std::fs::read(&p).ok().map(|b| (p, b)))
        .collect()
}

fn name(p: &std::path::Path) -> String {
    p.file_name().unwrap().to_string_lossy().into_owned()
}

#[test]
fn every_save_round_trips_as_bytes() {
    let saves = saves();
    for (path, bytes) in &saves {
        let n = name(path);
        let save = Save::parse(bytes).unwrap_or_else(|e| panic!("{n}: {e}"));
        let stream = save.to_stream().unwrap_or_else(|e| panic!("{n}: {e}"));
        let raw = Raw::parse(bytes).unwrap();
        if stream != raw.data {
            let at = stream
                .iter()
                .zip(&raw.data)
                .position(|(a, b)| a != b)
                .unwrap_or(stream.len().min(raw.data.len()));
            panic!(
                "{n}: stream differs at {at:#x} (wrote {} bytes, read {})",
                stream.len(),
                raw.data.len()
            );
        }
        let out = save.to_bytes().unwrap_or_else(|e| panic!("{n}: {e}"));
        assert!(out == *bytes, "{n}: file bytes differ after re-encoding");
        assert_eq!(save.header.version, VERSION, "{n}");
        assert_eq!(save.header.sub_version, SUB_VERSION, "{n}");
    }
}

/// The decoded fields agree with each other: every cross-reference that the
/// accessors rely on holds in every save.
#[test]
fn decoded_fields_are_consistent() {
    for (path, bytes) in &saves() {
        let n = name(path);
        let s = Save::parse(bytes).unwrap();
        let m = &s.map;
        let (w, h) = (m.width(), m.height());
        assert_eq!(m.tiles.len(), (w / 2 * h) as usize, "{n}");
        assert!(w > 0 && h > 0, "{n}");

        // per-continent city counts add up to the city list
        let total: u32 = s.game.cities_per_continent.iter().sum();
        assert_eq!(total as usize, s.cities.len(), "{n}: cities per continent");
        assert_eq!(
            s.game.cities_per_continent.len(),
            m.continent_count.u16(0) as usize,
            "{n}"
        );

        // units: ids ascend (records of dead units are gone, so they have
        // gaps), tile on the grid, valid owner and type
        assert!(
            s.units.windows(2).all(|p| p[0].id() < p[1].id()),
            "{n}: unit ids"
        );
        assert!(s.units.len() as u32 <= s.units.last().map_or(0, |u| u.id() + 1));
        for (i, u) in s.units.iter().enumerate() {
            assert!(
                u.x() >= 0 && (u.x() as u32) < w && u.y() >= 0 && (u.y() as u32) < h,
                "{n}: unit {i} off the map"
            );
            assert_eq!((u.x() + u.y()) % 2, 0, "{n}: unit {i} not on the grid");
            assert!((u.owner() as usize) < PLAYER_SLOTS, "{n}: unit owner");
            assert!(
                (u.unit_type() as usize) < s.counts.unit_types,
                "{n}: unit {i} type {}",
                u.unit_type()
            );
            assert!(u.experience_level() <= 3, "{n}: unit {i} experience");
            assert_eq!(u.version(), 2, "{n}");
            assert_eq!(u.ids.as_ref().map(|i| i.ids.len()), Some(10), "{n}");
        }

        // cities: ids ascend, and the tile under the city names it back and
        // carries the owner's border
        assert!(
            s.cities.windows(2).all(|p| p[0].id() < p[1].id()),
            "{n}: city ids"
        );
        for (i, c) in s.cities.iter().enumerate() {
            let t = m
                .tile(c.x().into(), c.y().into())
                .unwrap_or_else(|| panic!("{n}: city {i} off the map"));
            assert_eq!(t.city_id() as u32, c.id(), "{n}: tile of city {i}");
            assert_eq!(t.owner(), c.owner(), "{n}: border owner of city {i}");
            assert!((c.owner() as usize) < PLAYER_SLOTS, "{n}");
            assert_eq!(c.citizens.len() as u32, c.size(), "{n}");
            assert!(!c.name().is_empty(), "{n}: city {i} has no name");
        }
        // the converse: every tile that names a city is that city's tile
        let cities: std::collections::HashMap<u32, &City> =
            s.cities.iter().map(|c| (c.id(), c)).collect();
        let mut named = 0;
        for (idx, t) in m.tiles.iter().enumerate() {
            if t.city_id() >= 0 {
                named += 1;
                let c = cities
                    .get(&(t.city_id() as u32))
                    .unwrap_or_else(|| panic!("{n}: tile names a missing city"));
                let y = idx as u32 / (w / 2);
                let x = 2 * (idx as u32 % (w / 2)) + (y & 1);
                assert_eq!((u32::from(c.x()), u32::from(c.y())), (x, y), "{n}");
            }
            if t.owner() != 0 {
                assert_eq!(t.claim_mask() >> t.owner() & 1, 1, "{n}: owner bit");
            }
            if t.worked_by_city() >= 0 {
                let c = cities
                    .get(&(t.worked_by_city() as u32))
                    .unwrap_or_else(|| panic!("{n}: tile worked by a missing city"));
                // inside the city radius (21 tiles: at most 2 rows away in x
                // steps of 2, allowing for east-west wrap)
                let y = idx as u32 / (w / 2);
                let x = 2 * (idx as u32 % (w / 2)) + (y & 1);
                let dx = x.abs_diff(c.x().into());
                let dx = dx.min(w - dx);
                let dy = y.abs_diff(c.y().into());
                assert!(dx + dy <= 4, "{n}: worked tile {dx},{dy} from its city");
            }
            assert!(
                t.resource() >= -1 && t.resource() < s.counts.goods as i32,
                "{n}: resource {}",
                t.resource()
            );
            assert!(
                (t.continent_id() as i32) < m.continent_count.u16(0) as i32,
                "{n}: continent {}",
                t.continent_id()
            );
        }
        assert_eq!(named, s.cities.len(), "{n}");

        // wonders: a city index exactly when the flag is set
        for (i, (&city, &built)) in s
            .game
            .wonder_city
            .iter()
            .zip(&s.game.wonder_built)
            .enumerate()
        {
            assert_eq!(city != u32::MAX, built != 0, "{n}: wonder {i}");
            if built != 0 {
                assert!((city as usize) < s.cities.len(), "{n}: wonder {i}");
            }
        }

        // 32 player slots; slot 0 is not a civilization
        assert_eq!(s.players.len(), 32, "{n}");
        for (slot, p) in s.players.iter().enumerate() {
            assert_eq!(p.version(), 4, "{n}");
            assert_eq!(p.tables.is_some(), p.in_use(), "{n}");
            if p.in_use() {
                assert!((0..32).contains(&p.race()), "{n}: slot {slot} race");
                assert!(p.government() >= 0, "{n}: slot {slot} government");
                // the capital is one of the player's own cities
                if p.capital_city() >= 0 {
                    let c = cities
                        .get(&(p.capital_city() as u32))
                        .unwrap_or_else(|| panic!("{n}: slot {slot} capital missing"));
                    assert_eq!(c.owner() as usize, slot, "{n}: slot {slot} capital owner");
                }
            } else {
                assert_eq!(p.race(), -1, "{n}: unused slot {slot}");
            }
        }
        // every city is its owner's, and only slot 0 has none of its own
        for c in &s.cities {
            assert!(s.players[c.owner() as usize].in_use(), "{n}: city owner");
        }
        assert!(s.game.turn() < 10_000, "{n}");
        assert!((-4000..=2100).contains(&s.year()), "{n}: year");
        // the history has one record per turn, 0..=turn, over slots 1..=m
        let h = &s.history;
        assert_eq!(h.records.len(), s.game.turn() as usize + 1, "{n}: history");
        for (i, r) in h.records.iter().enumerate() {
            assert_eq!(r.a as usize, i, "{n}: history turn");
            assert!(
                r.series[0].iter().zip(1..).all(|(&v, k)| v == k),
                "{n}: history slots"
            );
        }
        let m = h.records[0].len();
        assert_eq!(u64::from(h.x), (1u64 << (m + 1)) - 2, "{n}: history mask");
    }
}

/// The embedded scenario parses and its counts size the arrays.
#[test]
fn embedded_scenario_gives_the_array_sizes() {
    for (path, bytes) in &saves() {
        let n = name(path);
        let s = Save::parse(bytes).unwrap();
        let biq = s.embedded_biq().unwrap_or_else(|e| panic!("{n}: {e}"));
        assert_eq!(RuleCounts::from_biq(&biq), s.counts, "{n}");
        assert_eq!(s.game.tech_known_by.len(), s.counts.techs, "{n}");
        assert_eq!(s.map.goods.len(), s.counts.goods, "{n}");
    }
}

/// The sub-version gates (guid, map order, tile padding, city date, player
/// lists, reserved block) write and read back consistently. The same
/// transformation of `yolo.SAV` was fed to the game's own loader in the
/// emulator (`savegame.md`, "Sub-version gates"): it consumed every stream to
/// the last byte with an identical chunk list.
#[test]
fn older_sub_versions_round_trip() {
    let saves = saves();
    let Some((_, bytes)) = saves
        .iter()
        .filter(|(_, b)| Save::parse(b).is_ok_and(|s| !s.cities.is_empty()))
        .min_by_key(|(_, b)| b.len())
    else {
        return;
    };
    let base = Save::parse(bytes).unwrap();
    for sub in 2..=SUB_VERSION {
        let s = base.with_sub_version(sub).unwrap();
        let out = s.to_stream().unwrap_or_else(|e| panic!("sub {sub}: {e}"));
        let back = Save::from_stream(&out, Storage::Plain, None)
            .unwrap_or_else(|e| panic!("sub {sub}: {e}"));
        assert_eq!(back.to_stream().unwrap(), out, "sub {sub}");
        assert_eq!(back.header.sub_version, sub);
        assert_eq!(back.cities.len(), base.cities.len());
        assert_eq!(back.map.tiles.len(), base.map.tiles.len());
        assert_eq!(back.map.tiles[0].cell_04, base.map.tiles[0].cell_04);
        // the same save written for a sub-version that wants other pieces is refused
        let mut wrong = s.clone();
        wrong.header.sub_version = if sub >= 8 { 2 } else { 10 };
        assert!(wrong.to_stream().is_err(), "sub {sub}: mismatched pieces");
    }
}

/// Counts that disagree with the lists are refused on write, not written.
#[test]
fn writer_refuses_inconsistent_models() {
    let saves = saves();
    let Some((_, bytes)) = saves.iter().min_by_key(|(_, b)| b.len()) else {
        return;
    };
    let base = Save::parse(bytes).unwrap();
    let inconsistent = |s: &Save| matches!(s.to_stream(), Err(Error::Inconsistent(_)));

    let mut s = base.clone();
    s.units.pop();
    assert!(inconsistent(&s), "unit list shorter than GAME says");

    let mut s = base.clone();
    s.map.tiles.pop();
    assert!(inconsistent(&s), "tile count");

    let mut s = base.clone();
    s.game.wonder_city.pop();
    assert!(inconsistent(&s), "array length");

    let mut s = base.clone();
    s.palv.pop();
    assert!(inconsistent(&s), "PALV count");

    let mut s = base.clone();
    s.biq.push(0);
    assert!(inconsistent(&s), "BIC length word");

    // editing a count *and* its list together is fine
    let mut s = base.clone();
    if let Some(u) = s.units.pop() {
        s.game.body.set_u32(game_field::UNITS, s.units.len() as u32);
        drop(u);
        let out = s.to_stream().expect("consistent edit writes");
        let again = Save::from_stream(&out, s.storage, None).expect("edit reads back");
        assert_eq!(again.units.len(), s.units.len());
        assert_eq!(again.to_stream().unwrap(), out);
    }
}

/// Truncated and corrupted input errors out: no panic, no huge allocation.
#[test]
fn corrupt_saves_error_instead_of_panicking() {
    const MAX_ALLOC: usize = 64 << 20;
    let saves = saves();
    let Some((_, bytes)) = saves.iter().min_by_key(|(_, b)| b.len()) else {
        return;
    };
    let raw = Raw::parse(bytes).unwrap();
    let stream = &raw.data;

    let poke = |data: &[u8], what: &str| {
        let (r, largest) = crate::alloc_probe::largest_during(|| {
            catch_unwind(AssertUnwindSafe(|| {
                if let Ok(s) = Save::from_stream(data, Storage::Plain, None) {
                    let _ = s.to_stream();
                }
            }))
        });
        assert!(r.is_ok(), "{what}: panicked");
        assert!(largest <= MAX_ALLOC, "{what}: allocated {largest} bytes");
    };

    // every truncation point of a sample, and all of the first kilobyte
    let mut cuts: Vec<usize> = (0..1200.min(stream.len())).collect();
    cuts.extend((1200..stream.len()).step_by(997));
    for cut in cuts {
        let r = catch_unwind(AssertUnwindSafe(|| {
            Save::from_stream(&stream[..cut], Storage::Plain, None)
        }));
        let r = r.unwrap_or_else(|_| panic!("truncation at {cut} panicked"));
        assert!(r.is_err(), "truncation at {cut} parsed");
    }

    // flip bytes: a deterministic pseudo-random walk over the whole stream
    let mut x = 0x2545_F491_4F6C_DD1Du64;
    for i in 0..400 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        let mut d = stream.clone();
        let at = (x as usize) % d.len();
        d[at] = (x >> 40) as u8;
        poke(&d, &format!("flip #{i} at {at}"));
    }

    // counts that promise more than the data holds (the unit count lives at
    // GAME body +0x18 after the BIC chunk, the embedded scenario and the
    // GAME chunk header)
    let s = Save::from_stream(stream, Storage::Plain, None).unwrap();
    let game_body = s.header.len() + 8 + 524 + s.biq.len() + 8;
    for off in [
        game_field::UNITS,
        game_field::CITIES,
        game_field::COLONIES,
        game_field::CONTINENTS,
        game_field::AIBS,
        game_field::VLOC,
    ] {
        let mut d = stream.clone();
        d[game_body + off..game_body + off + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        poke(&d, &format!("count at GAME+{off:#x}"));
    }

    // wrong versions are refused by name
    for (at, what) in [(6usize, "save format version"), (10, "save sub-version")] {
        let mut d = stream.clone();
        d[at] = 0x7F;
        match Save::from_stream(&d, Storage::Plain, None) {
            Err(Error::Unsupported { what: w, .. }) => assert_eq!(w, what),
            other => panic!("expected Unsupported({what}), got {other:?}"),
        }
    }
    // a scenario is not a save
    let scenario = Biq::new(crate::Version::new(12, 8)).to_bytes().unwrap();
    assert!(matches!(Save::parse(&scenario), Err(Error::BadMagic(_))));
}

/// A save written from nothing reads back through the accessors, with
/// the same players, tiles, units and cities that went in.
#[test]
fn a_blank_save_takes_what_is_set_and_reads_back() {
    let mut s = Save::blank(Vec::new(), 8, 6, 12, -3000).unwrap();
    s.set_player(1, 5, 2, 123);
    s.set_player(2, 7, 1, 9);
    {
        let t = s.map.tile_mut(2, 2).unwrap();
        t.set_terrain(5, 2);
        t.set_overlay_plane(0b1);
        t.set_river_connection_mask(0xAA);
        t.set_resource(3);
        t.set_owner(1);
    }
    s.add_city(City::new(0, 2, 2, 1, "Roma", 3, &[1, 5, 70], &s.counts.clone()));
    s.add_unit(Unit::new(0, 2, 2, 1, 4, 2, 1, 1));
    s.add_unit(Unit::new(1, 4, 2, 2, 7, 0, 0, 0));
    s.players[1].set_capital_city(0);
    s.finish();
    let bytes = s.to_bytes().unwrap();
    let back = Save::parse(&bytes).unwrap();
    assert_eq!(back.game.turn(), 12);
    assert_eq!(back.year(), -3000);
    assert_eq!((back.map.width(), back.map.height()), (8, 6));
    assert!(back.players[0].in_use() && back.players[1].in_use() && back.players[2].in_use() && !back.players[3].in_use());
    assert_eq!((back.players[1].race(), back.players[1].government(), back.players[1].gold()), (5, 2, 123));
    assert_eq!(back.players[1].capital_city(), 0);
    let t = back.map.tile(2, 2).unwrap();
    assert_eq!((t.terrain_id(), t.river_connection_mask(), t.resource(), t.owner(), t.city_id()), (5, 0xAA, 3, 1, 0));
    assert_eq!(back.map.tile(0, 0).unwrap().city_id(), -1);
    assert_eq!(back.cities.len(), 1);
    let c = &back.cities[0];
    assert_eq!((c.name().as_str(), c.size(), c.owner(), c.x(), c.y()), ("Roma", 3, 1, 2, 2));
    assert_eq!(c.improvements(), vec![1, 5, 70]);
    assert_eq!(back.units.len(), 2);
    let u = &back.units[0];
    assert_eq!((u.x(), u.y(), u.owner(), u.unit_type(), u.experience_level(), u.damage(), u.order()), (2, 2, 1, 4, 2, 1, 1));
    assert_eq!(back.to_bytes().unwrap(), bytes, "and writes back byte for byte");
}
