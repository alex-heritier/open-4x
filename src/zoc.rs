//! Zones of control.
//!
//! HYPOTHESIS: the executable's ZOC test is not decoded (`movement.md` 10);
//! what is known is that ZOC is the first `PRTO` body dword
//! (`UnitRow::zoc`, set on 16 shipped rows). The rule here is the one the
//! manual states: a land unit that stands next to an enemy unit with the
//! flag may not step straight onto another tile next to such a unit, unless
//! that tile holds a friendly unit or city. Ships and aircraft are neither
//! held nor holding, and only a civilization at war is an enemy.

use crate::map::GameMap;
use crate::units::{Unit, def};

/// A unit that exerts a zone: a land unit with the `PRTO` flag, on the map.
pub fn exerts(u: &Unit) -> bool {
    let d = def(u.utype);
    d.zoc && d.class == 0 && u.carrier.is_none()
}

/// A land unit that the zones bind.
pub fn bound(u: &Unit) -> bool {
    def(u.utype).class == 0
}

/// May a unit of `civ` not step from `from` to `to`? `zones` lists the
/// `(civ, x, y)` of every unit that exerts one; `friendly_at_to` is true when
/// the target holds a unit or a city of the mover's own.
pub fn blocks(
    map: &GameMap,
    civ: usize,
    from: (i32, i32),
    to: (i32, i32),
    zones: &[(usize, i32, i32)],
    at_war: impl Fn(usize, usize) -> bool,
    friendly_at_to: bool,
) -> bool {
    if friendly_at_to || from == to {
        return false;
    }
    let in_zone = |p: (i32, i32)| {
        zones.iter().any(|&(c, x, y)| {
            c != civ && (x, y) != p && map.distance(p, (x, y)) <= 1 && at_war(civ, c)
        })
    };
    in_zone(from) && in_zone(to)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> GameMap {
        GameMap::generate_with_seed(1)
    }

    #[test]
    fn a_step_from_one_zone_straight_into_another_is_refused() {
        let m = map();
        let zones = [(1, 10, 10)];
        let war = |_: usize, _: usize| true;
        // Both tiles touch the enemy at (10, 10).
        assert!(blocks(&m, 0, (9, 10), (9, 11), &zones, war, false));
        assert!(blocks(&m, 0, (9, 9), (10, 9), &zones, war, false));
        // Out of the zone, or into open ground, is free.
        assert!(!blocks(&m, 0, (9, 9), (8, 8), &zones, war, false));
        assert!(!blocks(&m, 0, (8, 8), (9, 9), &zones, war, false));
    }

    #[test]
    fn a_friend_peace_and_ones_own_zone_lift_it() {
        let m = map();
        let zones = [(1, 10, 10)];
        assert!(
            !blocks(&m, 0, (9, 10), (9, 11), &zones, |_, _| true, true),
            "a friendly stack or city"
        );
        assert!(
            !blocks(&m, 0, (9, 10), (9, 11), &zones, |_, _| false, false),
            "no war"
        );
        assert!(
            !blocks(&m, 1, (9, 10), (9, 11), &zones, |_, _| true, false),
            "its own zone"
        );
        // Two enemies: either one's zone binds both ends of the step.
        let zones = [(1, 10, 10), (2, 12, 10)];
        assert!(blocks(&m, 0, (11, 9), (11, 10), &zones, |_, _| true, false));
        // Peace with one of them leaves the other's zone.
        assert!(blocks(
            &m,
            0,
            (11, 9),
            (11, 10),
            &zones,
            |_, c| c == 2,
            false
        ));
        assert!(!blocks(
            &m,
            0,
            (11, 9),
            (11, 10),
            &zones,
            |_, c| c == 3,
            false
        ));
    }
}
