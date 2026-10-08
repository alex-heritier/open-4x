//! War weariness: the per-turn update of each civ's counters against the
//! civs it fights, and the collapse of a High-weariness government.
//!
//! Rules and addresses: `reverse-engineering/government.md` section 5. The
//! counters live in `diplomacy::Diplomacy` (the binary's `Player +0xCB4`);
//! the declaration kicker is in the reference `declare_war`; the incident
//! weights are booked by combat (`Diplomacy::incident`); the unhappy
//! citizens come from `citycalc::moods` through `realm::Realm::war_counters`.
//!
//! HYPOTHESIS: the military filter of `0x500BA4` is read as a unit with an
//! attack or bombard strength (the helpers `0x5BE6E0`, `0x5BE820` are not
//! decoded). Nothing mobilizes, so the mobilization bonus never applies.

use bevy::prelude::*;
use civ3_rules::government as exe;

use crate::cities::{City, territory};
use crate::civs::{CIV_CAP, CivilizationEnded, civ_count};
use crate::diplomacy::Diplomacy;
use crate::features::{MessageBoard, post};
use crate::map::GameMap;
use crate::realm;
use crate::rng::GameRng;
use crate::units::{Unit, def};

/// A unit counts as military for the weariness scans.
pub fn military(u: &Unit) -> bool {
    let d = def(u.utype);
    d.attack > 0 || d.bombard > 0
}

/// Who stands where, for one civ's update: `abroad[c]` is a soldier of
/// `civ` on land owned by `c`, `at_home[c]` one of `c` on land owned by `civ`.
pub fn scan(
    civ: usize,
    owner_of: impl Fn(i32, i32) -> Option<usize>,
    units: &[&Unit],
) -> ([bool; CIV_CAP], [bool; CIV_CAP]) {
    let mut abroad = [false; CIV_CAP];
    let mut at_home = [false; CIV_CAP];
    for u in units.iter().filter(|u| military(u) && u.civ < civ_count()) {
        let Some(owner) = owner_of(u.x, u.y) else {
            continue;
        };
        if u.civ == civ && owner != civ {
            abroad[owner] = true;
        } else if u.civ != civ && owner == civ {
            at_home[u.civ] = true;
        }
    }
    (abroad, at_home)
}

/// The turn update (`0x500AD0`) of the civ whose turn just ended, then the
/// collapse check of every High-weariness government (`0x560D18`).
#[allow(clippy::too_many_arguments)]
pub fn end_turn(
    mut ended: MessageReader<CivilizationEnded>,
    map: Res<GameMap>,
    cities: Query<&City>,
    units: Query<&Unit>,
    mut diplomacy: ResMut<Diplomacy>,
    mut dice: ResMut<GameRng>,
    mut board: ResMut<MessageBoard>,
) {
    for ev in ended.read() {
        let civ = ev.0;
        if civ >= civ_count() {
            continue;
        }
        let cs: Vec<&City> = cities.iter().collect();
        let us: Vec<&Unit> = units.iter().collect();
        let owners = territory(&map);
        let owner_of = |x, y| owners.get(&(x, y)).copied();
        let (abroad, at_home) = scan(civ, owner_of, &us);
        diplomacy.update_weariness(civ, &abroad, &at_home, false);
        let class = realm::govt(civ).war_weariness;
        let average = diplomacy.average_weariness(civ);
        if exe::democracy_collapses(class, average) && !realm::in_anarchy(civ) {
            let mine: Vec<&City> = cs.iter().copied().filter(|c| c.civ == civ).collect();
            if mine.is_empty() {
                continue;
            }
            // The score refuses a government this tired of war, so the pick
            // is never the one that just fell.
            let current = realm::read(civ, |r| r.govt);
            let then = crate::govern::choose_government(
                civ,
                &mine,
                us.iter().filter(|u| u.civ == civ).count(),
            )
            .filter(|&g| g != current)
            .unwrap_or(exe::row::DESPOTISM);
            crate::govern::revolt(civ, then, &mut dice);
            if !crate::civs::is_ai(civ)
                || (0..civ_count()).any(|h| !crate::civs::is_ai(h) && diplomacy.contact(h, civ))
            {
                post(
                    &mut board,
                    format!(
                        "War weariness has toppled the government of the {}!",
                        crate::civs::CIVS[civ].name
                    ),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::UnitType;

    #[test]
    fn the_scan_tells_invaders_from_defenders_and_ignores_civilians() {
        let soldier = |civ, x| Unit::new(civ, UnitType::named("Warrior"), x, 0);
        let worker = Unit::new(1, UnitType::named("Worker"), 0, 0);
        let mine = soldier(0, 1);
        let theirs = soldier(1, 0);
        let owner = |x: i32, _| Some(if x == 0 { 0 } else { 1 });
        let (abroad, home) = scan(0, owner, &[&mine, &theirs, &worker]);
        assert!(abroad[1], "our warrior stands on their land");
        assert!(home[1], "their warrior stands on ours");
        let (abroad, home) = scan(0, owner, &[&worker]);
        assert_eq!((abroad, home), ([false; CIV_CAP], [false; CIV_CAP]));
    }

    #[test]
    fn an_incident_feeds_both_counters_every_turn_until_peace_clears_it() {
        let war = |incident: i32| {
            let mut d = Diplomacy::new();
            let mut board = MessageBoard::default();
            d.declare(&crate::diplomacy::Facts::even(), 0, 1, 0, &mut board);
            // Pin the counters so the declaration's kicker does not matter.
            *d.rel.war_counter_mut(1, 2) = 0;
            *d.rel.war_counter_mut(2, 1) = 0;
            d.incident(1, 0, incident);
            d
        };
        let none = [false; CIV_CAP];
        let (mut hit, mut calm) = (war(16), war(0));
        // Above 30 with no invader, the decay of one comes off each turn.
        for (turn, want) in [16, 31, 46].into_iter().enumerate() {
            hit.update_weariness(0, &none, &none, false);
            calm.update_weariness(0, &none, &none, false);
            assert_eq!(
                (hit.weariness(0, 1), calm.weariness(0, 1)),
                (want, 0),
                "turn {}",
                turn + 1
            );
        }
        hit.make_peace(0, 1);
        let before = hit.weariness(0, 1);
        hit.update_weariness(0, &none, &none, false);
        assert!(hit.weariness(0, 1) < before, "peace lets it decay");
    }

    #[test]
    fn at_war_the_counter_falls_toward_thirty_when_no_one_is_invading() {
        let mut d = Diplomacy::new();
        let mut board = MessageBoard::default();
        d.declare(&crate::diplomacy::Facts::even(), 0, 1, 0, &mut board);
        *d.rel.war_counter_mut(1, 2) = 40;
        let none = [false; CIV_CAP];
        d.update_weariness(0, &none, &none, false);
        assert_eq!(d.weariness(0, 1), 39);
        let mut abroad = none;
        abroad[1] = true;
        d.update_weariness(0, &abroad, &none, false);
        assert_eq!(d.weariness(0, 1), 40, "an invasion adds one");
    }

    #[test]
    fn a_long_war_makes_a_democracy_city_unhappy_and_a_despotism_shrugs() {
        realm::reset();
        let mut city = City::new(0, "Kyoto", 3, 3);
        city.set_size(6);
        let map = crate::map::GameMap::generate();
        for tile in crate::cities::radius_tiles(&map, 3, 3).into_iter().take(6) {
            city.work_tile(&map, tile);
        }
        let calm = crate::citycalc::moods(&city, 0);
        realm::write(0, |r| {
            r.adopt(exe::row::DEMOCRACY);
            r.war_counters = vec![130];
        });
        let tired = crate::citycalc::moods(&city, 0);
        assert!(
            tired.unhappy > calm.unhappy,
            "130 against a Democracy: {tired:?} vs {calm:?}"
        );
        realm::write(0, |r| r.adopt(exe::row::DESPOTISM));
        assert_eq!(crate::citycalc::moods(&city, 0).unhappy, calm.unhappy);
    }
}
