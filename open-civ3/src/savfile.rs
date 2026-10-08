//! F5 writes a Civ3 `.SAV` next to the quicksave (`docs/civ3-files.md`
//! section 6): the map, players, cities and units go into the decoded fields
//! of `civ3_biq::Save`, built by `Save::blank`; what the SAV format cannot
//! hold yet (research targets, diplomacy, the books of each city, dice) stays
//! in the JSON quicksave, which F8 reads. A game opens from the `.SAV` with
//! `open-4x FILE.SAV`; what it does not carry starts at its defaults.
//!
//! The file is for open-4x: the undecoded parts of the format are zeros, not
//! the values Civ3 itself would write.

use bevy::prelude::*;
use civ3_biq::Save;
use civ3_biq::sav::{City as SavCity, Tile as SavTile, Unit as SavUnit};
use civ3_biq::sections::tile::{feature, overlay};

use crate::barbarians::Tribe;
use crate::boot::{Boot, World as BootWorld};
use crate::cities::{Capital, City, Treasury};
use crate::civs::{BARBARIANS, civ_count};
use crate::combat::Level;
use crate::map::{Base, Cover, GameMap, Relief, Tile};
use crate::units::{Turn, Unit};

/// How clone cells and native cells map for the game being written.
struct Frame {
    /// Native width and height.
    dims: (i32, i32),
    to_native: Box<dyn Fn(i32, i32) -> (i32, i32)>,
    to_clone: Box<dyn Fn(i32, i32) -> (i32, i32)>,
}

impl Frame {
    /// A scenario or saved game keeps the lattice it came with; a random map
    /// is laid on a native diamond that holds the whole grid.
    fn of(map: &GameMap) -> Frame {
        if let Some(sc) = crate::scenario::scenario() {
            let l = sc.lattice;
            return Frame {
                dims: (2 * l.half_w, l.rows),
                to_native: Box::new(move |u, v| l.to_native(u, v)),
                to_clone: Box::new(move |x, y| l.to_clone(x, y)),
            };
        }
        let (w, h) = (map.w, map.h);
        let off = h + (h & 1);
        let width = (w + off + 1) & !1;
        Frame {
            dims: (width, w + h),
            to_native: Box::new(move |u, v| (u - v + off, u + v)),
            to_clone: Box::new(move |x, y| ((x + y - off) / 2, (y - x + off) / 2)),
        }
    }
}

/// `GOOD` row of a placed resource (the inverse of `realm::good_id`).
fn good_row(id: u8) -> i32 {
    let name = match crate::features::GOODS[id as usize].name {
        "Wine" => "Wines",
        "Diamonds" => "Gems",
        n => n,
    };
    crate::ruleset::GOOD_NAMES
        .iter()
        .position(|g| *g == name)
        .map_or(-1, |i| i as i32)
}

/// `TERR` row and secondary class of a tile, in the numbering saves use.
fn terrain_of(t: &Tile) -> (u8, u8) {
    let under = |b: Base| match b {
        Base::Desert => 0,
        Base::Plains => 1,
        Base::Tundra | Base::Ice => 3,
        _ => 2,
    };
    match (t.base, t.relief, t.cover) {
        (Base::Coast, ..) => (11, 11),
        (Base::Sea, ..) => (12, 12),
        (Base::Ocean, ..) => (13, 13),
        (b, Relief::Hill, _) => (5, under(b)),
        (b, Relief::Mountain, _) => (6, under(b)),
        (b, _, Cover::Forest | Cover::Pine) => (7, under(b)),
        (b, _, Cover::Jungle) => (8, under(b)),
        (b, ..) => (under(b), under(b)),
    }
}

fn put_tile(dst: &mut SavTile, t: &Tile) {
    let (id, sub) = terrain_of(t);
    dst.set_terrain(id, sub);
    let mut over = 0;
    for (on, bit) in [
        (t.road, overlay::ROAD),
        (t.irrigation, overlay::IRRIGATION),
        (t.mine, overlay::MINE),
        (t.fortress, overlay::FORTRESS),
        (t.hut, overlay::GOODY_HUT),
        (t.camp, overlay::BARBARIAN_CAMP),
        (t.barricade, overlay::BARRICADE),
    ] {
        if on {
            over |= bit;
        }
    }
    dst.set_overlay_plane(over);
    dst.set_feature_plane(if t.cover == Cover::Pine {
        feature::PINE_FOREST
    } else {
        0
    });
    dst.set_river_connection_mask(t.river & 0xAA);
    dst.set_resource(t.resource.map_or(-1, good_row));
    dst.set_owner(t.owner.map_or(0, |c| c + 1));
}

fn level_index(l: Level) -> u32 {
    match l {
        Level::Conscript => 0,
        Level::Regular => 1,
        Level::Veteran => 2,
        Level::Elite => 3,
    }
}

/// The bytes of the BIQ the game runs on: the file played, or the one a
/// saved game carries.
fn embedded(boot: &Boot) -> Result<Vec<u8>, String> {
    match &boot.world {
        BootWorld::Saved(s) => Ok(s.biq.clone()),
        _ => std::fs::read(&boot.file).map_err(|e| format!("{}: {e}", boot.file.display())),
    }
}

/// The game as a `Save`.
pub fn capture(world: &mut World) -> Result<Save, String> {
    let boot = world
        .get_resource::<Boot>()
        .ok_or("no game file to embed")?;
    let biq = embedded(boot)?;
    let paths = match &boot.world {
        BootWorld::Saved(save) => save.bic.0[4..524].to_vec(),
        _ => {
            let file = boot.file.canonicalize().map_err(|e| e.to_string())?;
            let dir = file.parent().expect("scenario file has a parent");
            [
                civ3_biq::io::Str::<260>::new(&dir.to_string_lossy()).0,
                civ3_biq::io::Str::<260>::new(&file.to_string_lossy()).0,
            ]
            .concat()
        }
    };
    let mut save = build(world, biq)?;
    save.bic.0[4..524].copy_from_slice(&paths);
    Ok(save)
}

/// The game as a `Save` embedding `biq` (the bytes of the BIQ it runs on;
/// empty for the shipped Conquests rules).
fn build(world: &mut World, biq: Vec<u8>) -> Result<Save, String> {
    let frame = Frame::of(world.resource::<GameMap>());
    let turn = world.resource::<Turn>().0;
    let year = crate::calendar::year(turn);
    let (w, h) = frame.dims;
    let mut save = Save::blank(biq, w as u32, h as u32, turn, year).map_err(|e| e.to_string())?;

    let players = civ_count().min(31);
    let rows = &crate::ruleset::RACE_ROSTER;
    let treasury = world.resource::<Treasury>().0;
    for civ in 0..players {
        let race = rows[crate::civs::roster_index(civ)].race as i32;
        let govt = crate::realm::read(civ, |r| r.govt) as i32;
        save.set_player(civ + 1, race, govt, treasury[civ] as i32);
    }
    {
        let research = world.resource::<crate::research::Research>();
        for t in 0..save.counts.techs {
            for civ in 0..players {
                if research.knows(civ, t as i32) {
                    save.game.tech_known_by[t] |= 1 << (civ + 1);
                }
            }
        }
    }

    // Cities, in a fixed order so ids are stable.
    let mut cities: Vec<(Entity, City)> = world
        .query::<(Entity, &City)>()
        .iter(world)
        .map(|(e, c)| (e, c.clone()))
        .collect();
    cities.sort_by_key(|(_, c)| (c.civ, c.founded, c.name.clone()));
    let capital = world.resource::<Capital>().0;
    let nbld = save.counts.buildings;
    let unit_rows = crate::roster::unit_count();
    for (id, (e, c)) in cities.iter().enumerate() {
        if c.civ >= players {
            continue;
        }
        let (x, y) = (frame.to_native)(c.x, c.y);
        let built: Vec<usize> = c
            .buildings
            .iter()
            .chain(c.gifts.iter())
            .filter_map(|p| (p.index() >= unit_rows).then(|| p.index() - unit_rows))
            .filter(|&b| b < nbld)
            .collect();
        for &b in &built {
            if crate::roster::bldg(b).other
                & (crate::roster::oth::WONDER | crate::roster::oth::SMALL_WONDER)
                != 0
            {
                save.game.wonder_city[b] = id as u32;
                save.game.wonder_built[b] = 1;
            }
        }
        let counts = save.counts;
        save.add_city(SavCity::new(
            id as u32,
            x as u16,
            y as u16,
            (c.civ + 1) as u8,
            &c.name,
            c.size() as u32,
            &built,
            &counts,
        ));
        if capital[c.civ] == Some(*e) {
            save.players[c.civ + 1].set_capital_city(id as i32);
        }
    }

    // Units.
    let mut units: Vec<(usize, Unit)> = world
        .query::<(&Unit, Option<&Tribe>)>()
        .iter(world)
        .map(|(u, _)| (u.civ, u.clone()))
        .collect();
    units.sort_by_key(|(civ, u)| (*civ, u.utype.0, u.x, u.y));
    for (id, (civ, u)) in units.iter().enumerate() {
        let owner = if *civ == BARBARIANS {
            0
        } else if *civ < players {
            civ + 1
        } else {
            continue;
        };
        let (x, y) = (frame.to_native)(u.x, u.y);
        save.add_unit(SavUnit::new(
            id as u32,
            x,
            y,
            owner as u32,
            u.utype.0 as u32,
            level_index(u.level),
            u.damage.max(0) as u32,
            u32::from(u.fortified),
        ));
    }

    // The map: every native cell takes the clone's tile there.
    let map = world.resource::<GameMap>();
    for y in 0..h {
        for x in 0..w {
            if (x + y) % 2 != 0 {
                continue;
            }
            let (u, v) = (frame.to_clone)(x, y);
            let Some(t) = map.get(u, v) else {
                if let Some(d) = save.map.tile_mut(x as u32, y as u32) {
                    d.set_terrain(13, 13);
                }
                continue;
            };
            if let Some(d) = save.map.tile_mut(x as u32, y as u32) {
                put_tile(d, t);
            }
        }
    }
    // Cities were linked before the tiles were written over them.
    for c in &save.cities.clone() {
        if let Some(d) = save.map.tile_mut(c.x() as u32, c.y() as u32) {
            d.set_city_id(c.id() as i16);
        }
    }
    save.finish();
    Ok(save)
}

/// Write the `.SAV` for the quicksave at `json`: the same name, `.SAV`.
pub fn write(world: &mut World, json: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let path = json.with_extension("SAV");
    let save = capture(world)?;
    save.write_file(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cities::Production;
    use crate::units::UnitType;

    #[test]
    fn saves_keep_the_original_scenario_location_when_saved_again() {
        let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("civ3_utils/biq/tests/data/Intro3 New Alliances.biq");
        let mut w = world();
        w.insert_resource(Boot {
            options: Default::default(),
            install: crate::install::Install::new("test-assets", vec![]),
            file: file.clone(),
            world: BootWorld::Random,
            cache_namespace: "test-scenario".into(),
        });
        let save = capture(&mut w).unwrap();
        assert_eq!(
            save.scenario_file(),
            file.canonicalize().unwrap().to_string_lossy()
        );
        assert_eq!(
            save.scenario_dir(),
            file.parent().unwrap().to_string_lossy()
        );
        let boot = &mut *w.resource_mut::<Boot>();
        boot.file = "renamed-save.SAV".into();
        boot.world = BootWorld::Saved(Box::new(save.clone()));
        let resaved = capture(&mut w).unwrap();
        assert_eq!(resaved.scenario_file(), save.scenario_file());
        assert_eq!(resaved.scenario_dir(), save.scenario_dir());
    }

    fn world() -> World {
        crate::civs::set_controllers();
        let mut w = World::new();
        w.insert_resource(GameMap::generate());
        w.insert_resource(Turn(42));
        w.init_resource::<Treasury>();
        w.init_resource::<Capital>();
        w.insert_resource(crate::research::Research::new());
        w
    }

    #[test]
    fn a_game_written_as_a_sav_opens_with_what_it_had() {
        let mut w = world();
        let map = w.resource::<GameMap>().clone();
        let starts = crate::civs::starting_positions(&map);
        let mut city = City::new(0, "Kyoto", starts[0].0, starts[0].1);
        city.set_size(4);
        city.buildings.push(Production::named("Temple"));
        city.buildings.push(Production::named("Palace"));
        let capital = w.spawn(city).id();
        w.resource_mut::<Capital>().0[0] = Some(capital);
        w.spawn(City::new(1, "Memphis", starts[1].0, starts[1].1));
        let mut warrior = Unit::new(0, UnitType::named("Warrior"), starts[0].0 + 1, starts[0].1);
        warrior.damage = 1;
        warrior.fortified = true;
        warrior.level = Level::Veteran;
        w.spawn(warrior);
        w.spawn((
            Unit::new(
                BARBARIANS,
                UnitType::named("Warrior"),
                starts[1].0,
                starts[1].1 + 2,
            ),
            Tribe(0),
        ));
        w.resource_mut::<Treasury>().0[0] = 321;
        w.resource_mut::<GameMap>().tiles[map.idx(starts[0].0, starts[0].1 + 1)].road = true;

        let save = build(&mut w, Vec::new()).unwrap();
        let bytes = save.to_bytes().unwrap();
        let back = Save::parse(&bytes).unwrap();
        assert_eq!(back.game.turn(), 42);
        assert_eq!(back.year(), crate::calendar::year(42));
        assert_eq!(back.players[1].gold(), 321);
        assert_eq!(back.cities.len(), 2);
        assert_eq!(back.units.len(), 2);

        // And it plays: the scenario read from it has the same people.
        let (map2, lattice) = crate::scenario::map_from_save(&back, 1).expect("the map reads back");
        let sc = crate::scenario::scenario_from_save(&back, lattice);
        assert_eq!(sc.turn, 42);
        assert_eq!(sc.leads.len(), civ_count());
        assert_eq!(sc.leads[0].gold, 321);
        let kyoto = sc.cities.iter().find(|c| c.name == "Kyoto").expect("Kyoto");
        assert_eq!(kyoto.size, 4);
        assert!(
            kyoto
                .buildings
                .contains(&(Production::named("Temple").index() - crate::roster::unit_count()))
        );
        assert_eq!(sc.slot_of(kyoto.owner), Some(0));
        assert!(
            map2.is_land(kyoto.at.0, kyoto.at.1),
            "the city stands on land"
        );
        let veteran = sc.units.iter().find(|u| u.level == 2).expect("the veteran");
        assert_eq!((veteran.damage, veteran.fortified), (1, true));
        assert!(
            sc.units
                .iter()
                .any(|u| sc.slot_of(u.owner) == Some(BARBARIANS))
        );
        // The land is the land, and the road is where it was.
        let land = |m: &GameMap| {
            m.tiles
                .iter()
                .filter(|t| !matches!(t.base, Base::Ocean | Base::Sea | Base::Coast))
                .count()
        };
        let original = w.resource::<GameMap>();
        assert!(
            land(&map2) >= land(original) * 9 / 10,
            "{} of {} land tiles",
            land(&map2),
            land(original)
        );
        assert!(map2.tiles.iter().any(|t| t.road));
    }
}
