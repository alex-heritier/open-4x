//! Unit upgrades: eligibility pieces, the replacement walk, the gold price
//! and what survives (`Unit::canUpgrade` `0x5C0620`, `Unit::upgradeCost`
//! `0x5C04D0`, `Unit::upgrade` `0x5C0740`).
//!
//! Findings, addresses and the open list are in `../unit-upgrades.md`. The
//! tests are its golden vectors U1..U10 (hand-derived from the text, not
//! captured runs).

/// `BLDG.improvement_flags` bit a facility must carry for a unit of each
/// `PRTO` domain (`0x5C0620` step 5): Barracks `0x2` for land, Harbor
/// `0x20000` for sea, Airport `0x40000` for air. Any other domain: none.
pub fn facility_flag(domain: i32) -> Option<u32> {
    match domain {
        0 => Some(0x2),
        1 => Some(0x2_0000),
        2 => Some(0x4_0000),
        _ => None,
    }
}

/// The `PRTO` special-action bit 8 ("Upgrade Unit") the command gate
/// `0x5C1AD0` ANDs into the prototype's word; without it the unit never
/// upgrades even if `upgrade_to` is set.
pub const UPGRADE_UNIT: u32 = 1 << 8;

/// The replacement type (`City::replacement`, `0x4C0690`): the *furthest*
/// buildable successor along the `upgrade_to` chain. The exe accepts a
/// chain member only when none of its own upgrades is buildable
/// (`canBuildUnit` with the obsolescence check), so intermediate types are
/// skipped. `chain` is the successors in order, nearest first.
///
/// HYPOTHESIS (not modelled): the exe then swaps the result for its
/// "alternate of" prototype (`PRTO +0xA0`) and looks for a type of the same
/// AI strategy mask that is it or an alternate of it; the shipped rules use
/// this only for race-specific variants.
pub fn replacement(chain: impl IntoIterator<Item = usize>, buildable: impl Fn(usize) -> bool) -> Option<usize> {
    chain.into_iter().filter(|&s| buildable(s)).last()
}

/// The gold an upgrade costs (`0x5C04D0`): the RULE `upgrade_cost` times the
/// shield difference of the two types, halved by a Halves-Upgrade-Cost
/// wonder (Leonardo's Workshop), then cut for an AI civ by the game
/// difficulty (`> 4`: 1/8, `> 3`: 1/6, `> 2`: 1/4), a negative price
/// becoming 0. Every division truncates toward zero.
///
/// `ai_difficulty` is `None` for a human civ.
pub fn price(rule: i32, old: i32, new: i32, leonardo: bool, ai_difficulty: Option<i32>) -> i32 {
    let mut cost = rule * (new - old);
    if leonardo {
        cost /= 2;
    }
    if let Some(level) = ai_difficulty {
        cost /= match level {
            5.. => 8,
            4 => 6,
            3 => 4,
            _ => 1,
        };
    }
    cost.max(0)
}

/// The experience the new unit starts with: the old unit's, capped at
/// Veteran (index 2), so an Elite comes out a Veteran (`0x5C0740` step 4).
pub fn experience_after(level: i32) -> i32 {
    level.clamp(0, 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RULE: i32 = 3;

    #[test]
    fn golden_vectors_for_the_price() {
        // U1..U4: a human.
        assert_eq!(price(RULE, 30, 80, false, None), 150);
        assert_eq!(price(RULE, 30, 80, true, None), 75);
        assert_eq!(price(RULE, 25, 80, true, None), 82);
        assert_eq!(price(RULE, 80, 30, false, None), 0);
        // U5..U8: an AI by game difficulty.
        assert_eq!(price(RULE, 30, 80, false, Some(5)), 18);
        assert_eq!(price(RULE, 30, 80, false, Some(4)), 25);
        assert_eq!(price(RULE, 30, 80, false, Some(3)), 37);
        assert_eq!(price(RULE, 30, 80, false, Some(2)), 150);
        // U9: Leonardo first, then the AI cut.
        assert_eq!(price(RULE, 30, 80, true, Some(5)), 9);
    }

    #[test]
    fn a_negative_price_after_the_cuts_is_free() {
        assert_eq!(price(RULE, 80, 30, true, Some(5)), 0);
        assert_eq!(price(0, 30, 80, false, None), 0);
    }

    #[test]
    fn the_replacement_is_the_furthest_buildable_successor() {
        // Warrior -> Swordsman -> Medieval Infantry -> ... (ids as chain).
        let chain = [10, 20, 30];
        assert_eq!(replacement(chain, |s| s == 10), Some(10));
        // Both the first and the last are buildable: the intermediate
        // type is obsolete, the last one wins.
        assert_eq!(replacement(chain, |s| s == 10 || s == 30), Some(30));
        // The middle type is not buildable, the one after it is.
        assert_eq!(replacement(chain, |s| s != 20), Some(30));
        assert_eq!(replacement(chain, |_| false), None);
        assert_eq!(replacement([], |_| true), None);
    }

    #[test]
    fn a_facility_per_domain() {
        assert_eq!(facility_flag(0), Some(0x2));
        assert_eq!(facility_flag(1), Some(0x20000));
        assert_eq!(facility_flag(2), Some(0x40000));
        assert_eq!(facility_flag(3), None);
    }

    #[test]
    fn an_elite_comes_out_a_veteran() {
        assert_eq!([0, 1, 2, 3].map(experience_after), [0, 1, 2, 2]);
    }
}
