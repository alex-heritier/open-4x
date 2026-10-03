//! The Golden Age (`research.md` 11, `combat.md` 6.2, `city-buildings.md`
//! 10). A civilization has one: it starts when the civ holds a great wonder
//! of one of its own traits (`0x55C9A0`, checked after every great wonder
//! is added) or when its unique unit wins a fight against a civilization
//! (`0x5BEF00`), and lasts RULE `[0x9C7308]` turns. While it runs, every
//! worked tile that yields a shield or commerce yields one more
//! (`yields.md` 4.2 step 8, 4.3 step 8).
//!
//! HYPOTHESIS: `0x55C9A0` is described as testing "every trait the civ
//! has"; the wording does not settle whether one matching wonder suffices.
//! The manual's rule, one wonder of either trait, is used here.

use bevy::prelude::*;

use crate::cities::City;
use crate::civs::{CIV_COUNT, Civilizations};
use crate::features::{MessageBoard, post};
use crate::roster::{self, oth};
use crate::units::Turn;

/// RULE "golden age length" (`[0x9C7308]`).
pub const LENGTH: u32 = 20;

/// The `BLDG +0xF0` category bit of each trait `k = 0..7` (Militaristic,
/// Commercial, Expansionist, Scientific, Religious, Industrious,
/// Agricultural, Seafaring).
pub const TRAIT_WONDER: [u32; 8] = [oth::MILITARISTIC, oth::COMMERCIAL, 0x80, oth::SCIENTIFIC, oth::RELIGIOUS, 0x200, oth::AGRICULTURAL, oth::SEAFARING];

/// A great wonder of one of the civ's traits stands among `rows`.
pub fn wonder_triggers(traits: u32, rows: impl IntoIterator<Item = usize>) -> bool {
    rows.into_iter().any(|row| {
        let b = roster::bldg(row);
        b.other & oth::WONDER != 0
            && (0..8).any(|k| traits & (1 << k) != 0 && b.other & TRAIT_WONDER[k] != 0)
    })
}

/// `combat.md` 6.2 step 1: a unique unit's victory over a civilization.
pub fn unit_triggers(winner: crate::units::UnitType, loser_barbarian: bool, civ: usize) -> bool {
    crate::units::def(winner).abilities & roster::ability::STARTS_GOLDEN_AGE != 0
        && !loser_barbarian
        && civ < CIV_COUNT
        && crate::realm::read(civ, |r| r.golden_end.is_none())
}

/// Ask for a Golden Age for `civ` at the next check (the combat sequencer
/// has no calendar).
pub fn request(civ: usize) {
    if civ < CIV_COUNT {
        crate::realm::write(civ, |r| r.golden_due = true);
    }
}

/// Start ages that are due, flag the ones in effect, and announce the end.
pub fn track(
    turn: Res<Turn>,
    cities: Query<&City>,
    civs: Res<Civilizations>,
    mut board: ResMut<MessageBoard>,
    mut ended: Local<[bool; CIV_COUNT]>,
) {
    for civ in 0..CIV_COUNT {
        let (end, due) = crate::realm::read(civ, |r| (r.golden_end, r.golden_due));
        if end.is_none() {
            let rows = cities
                .iter()
                .filter(|c| c.civ == civ)
                .flat_map(|c| c.buildings.iter().filter_map(|b| b.building_row()));
            if due || wonder_triggers(crate::cities::traits(civ), rows) {
                crate::realm::write(civ, |r| {
                    r.golden_end = Some(turn.0 + LENGTH);
                    r.golden_due = false;
                });
                if civ == civs.viewer() {
                    post(&mut board, "Our Great Civilization has entered a Golden Age!");
                }
            }
        }
        let end = crate::realm::read(civ, |r| r.golden_end);
        let golden = end.is_some_and(|e| turn.0 < e);
        crate::realm::write(civ, |r| r.golden = golden);
        if end.is_some_and(|e| turn.0 >= e) && !ended[civ] {
            ended[civ] = true;
            if civ == civs.viewer() {
                post(&mut board, "Our Civilization's Golden Age has ended. So say our analysts...");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str) -> usize {
        (0..roster::BLDG_COUNT).find(|&r| roster::bldg(r).name == name).unwrap()
    }

    #[test]
    fn a_wonder_of_ones_own_trait_starts_the_age() {
        // Japan is Militaristic and Religious (0x11).
        let japan = crate::rules_data::RACES[0].traits;
        let temple = row("The Temple of Artemis");
        let lighthouse = row("The Great Lighthouse");
        assert!(roster::bldg(temple).other & oth::RELIGIOUS != 0);
        assert!(wonder_triggers(japan, [temple]));
        // The Great Lighthouse only counts if it carries one of the traits.
        assert_eq!(wonder_triggers(japan, [lighthouse]), roster::bldg(lighthouse).other & (oth::MILITARISTIC | oth::RELIGIOUS) != 0);
        // An ordinary building never does.
        assert!(!wonder_triggers(japan, [row("Temple")]));
    }

    #[test]
    fn the_age_adds_a_shield_and_a_commerce_where_there_already_is_one() {
        let map = crate::map::GameMap::generate();
        let city = City::new(1, "Rome", map.start.0, map.start.1);
        let tile = (0..map.h)
            .flat_map(|y| (0..map.w).map(move |x| (x, y)))
            .find(|&(x, y)| {
                let (_, s, c) = crate::citycalc::tile_yields(&map, &city, x, y);
                s == 1 && c == 0
            })
            .expect("a one-shield tile without commerce");
        crate::realm::write(1, |r| r.golden = true);
        let (_, s, c) = crate::citycalc::tile_yields(&map, &city, tile.0, tile.1);
        crate::realm::write(1, |r| r.golden = false);
        assert_eq!((s, c), (2, 0));
    }
}
