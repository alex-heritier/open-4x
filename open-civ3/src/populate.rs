//! Startup: put a scenario's players, cities and units on the map
//! (`docs/civ3-files.md` section 5). A random game has nothing to place here:
//! `units::spawn_party` gives each civ its settlers.

use bevy::prelude::*;

use crate::cities::{self, City, Founding, Production, Treasury};
use crate::civs::{BARBARIANS, civ_count, is_ai};
use crate::combat::Level;
use crate::map::GameMap;
use crate::scenario::{self, Scenario};
use crate::units::{Selected, UnitArt, spawn_unit_at_level};

/// Where the camera opens: the first human's start in a scenario.
pub fn human_start() -> Option<(i32, i32)> {
    let sc = scenario::scenario()?;
    let me = sc.humans(crate::cli::options().civ.as_deref());
    me.iter().find_map(|&slot| sc.start_of(slot))
}

/// `EXPR` row to the clone's experience levels.
fn level_of(row: i32) -> Level {
    match row {
        ..=0 => Level::Conscript,
        1 => Level::Regular,
        2 => Level::Veteran,
        _ => Level::Elite,
    }
}

/// Governments, treasuries, cities and units of the scenario.
pub fn populate(
    mut commands: Commands,
    mut map: ResMut<GameMap>,
    art: Res<UnitArt>,
    mut f: Founding,
    mut treasury: ResMut<Treasury>,
    mut selected: ResMut<Selected>,
) {
    let Some(sc) = scenario::scenario() else {
        return;
    };
    place_leads(sc, &mut treasury);
    let mut placed: Vec<City> = vec![];
    let mut skipped = 0;
    for row in &sc.cities {
        let Some(slot) = sc.slot_of(row.owner).filter(|&s| s < civ_count()) else {
            skipped += 1;
            continue;
        };
        let (x, y) = row.at;
        if !map.is_land(x, y) || placed.iter().any(|c| (c.x, c.y) == (x, y)) {
            skipped += 1;
            continue;
        }
        let i = map.idx(x, y);
        map.tiles[i].hut = false;
        map.tiles[i].camp = false;
        map.tiles[i].road = true;
        let mut city = City {
            coastal: map.coastal_site(x, y),
            river: map.get(x, y).is_some_and(|t| t.river != 0),
            culture: row.culture.max(0) as u32,
            ..City::new(slot, row.name.clone(), x, y)
        };
        city.citizens = crate::citizens::new_pool(slot, row.size.max(1));
        let mut rows: Vec<Production> = row
            .buildings
            .iter()
            .map(|&b| Production((crate::roster::unit_count() + b) as u16))
            .collect();
        if let Some(p) = crate::roles::palace().filter(|_| row.palace) {
            rows.push(p);
        }
        if let Some(w) = crate::roles::walls().filter(|_| row.walls) {
            rows.push(w);
        }
        rows.sort_by_key(|p| p.0);
        rows.dedup();
        city.buildings = rows;
        let others: Vec<&City> = placed.iter().collect();
        let city = cities::place_city(&mut commands, &mut f, &mut map, &others, city, slot);
        placed.push(city);
    }
    let human = (0..civ_count()).find(|&c| !is_ai(c)).unwrap_or(0);
    for row in &sc.units {
        let Some(slot) = sc
            .slot_of(row.owner)
            .filter(|&s| s < civ_count() || s == BARBARIANS)
        else {
            skipped += 1;
            continue;
        };
        if row.utype >= crate::roster::unit_count() {
            skipped += 1;
            continue;
        }
        let (x, y) = row.at;
        let t = crate::units::UnitType(row.utype as u16);
        let e = if let civ3_biq::owner::Owner::BarbarianTribe(tribe) = row.owner {
            crate::barbarians::spawn(&mut commands, &art, t, (x, y), tribe.clamp(0, 255) as u8)
        } else {
            spawn_unit_at_level(&mut commands, &art, t, x, y, slot, level_of(row.level))
        };
        commands
            .entity(e)
            .entry::<crate::units::Unit>()
            .and_modify({
                let (damage, fortified) = (row.damage, row.fortified);
                move |mut u| {
                    u.damage = damage;
                    u.fortified = fortified;
                }
            });
        if slot == human && selected.0.is_none() {
            selected.0 = Some(e);
        }
    }
    for (slot, lead) in sc
        .leads
        .iter()
        .enumerate()
        .filter(|(s, _)| *s < civ_count() && !sc.has_objects(*s))
    {
        let Some((x, y)) = sc.start_of(slot) else {
            continue;
        };
        for &(utype, n) in &lead.starting_units {
            for _ in 0..n.min(20) {
                if utype < crate::roster::unit_count() {
                    spawn_unit_at_level(
                        &mut commands,
                        &art,
                        crate::units::UnitType(utype as u16),
                        x,
                        y,
                        slot,
                        Level::Regular,
                    );
                }
            }
        }
    }
    // Fortresses and barricades come with the tile overlays; resource
    // colonies are not modelled.
    if skipped > 0 {
        warn!("scenario: {skipped} city or unit rows could not be placed and were skipped");
    }
    info!(
        "scenario: {} cities, {} units placed",
        placed.len(),
        sc.units.len()
    );
}

/// Governments and gold of each lead.
fn place_leads(sc: &Scenario, treasury: &mut Treasury) {
    for (slot, lead) in sc
        .leads
        .iter()
        .enumerate()
        .filter(|(s, _)| *s < civ_count())
    {
        treasury.0[slot] = lead.gold;
        if let Some(g) = lead
            .government
            .filter(|&g| g < civ3mapgen::government::SHIPPED.len())
        {
            crate::realm::write(slot, |r| r.adopt(g));
        }
    }
}
