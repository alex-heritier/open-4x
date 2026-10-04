//! Land terrain gate in `Unit::canEnter`, `0x5CCBB0`.
//! This covers terrain only, after the domain/embarkation checks.

/// `0x5CCF2B` tests TERR +0x7A (impassable), `0x5CCF87` tests +0x7B
/// for wheeled prototypes. Either restriction falls through to the road
/// exception: destination `0x5CCFB1`, origin `0x5CCFE4`. Both need roads.
pub fn land_terrain_allowed(
    impassable: bool,
    impassable_wheeled: bool,
    wheeled: bool,
    from_road: bool,
    to_road: bool,
) -> bool {
    !(impassable || impassable_wheeled && wheeled) || from_road && to_road
}

/// `0x5801DF..0x580224`: origin river mask in the step direction requires
/// Player::hasTechFlag(4). Without it, the road branch falls back to terrain
/// (`0x580377..0x580388`); otherwise it costs one third (`0x580286`).
/// Railroad, special prototype movement and treaty gates are outside this helper.
pub fn land_step_cost(terrain: u8, from_road: bool, to_road: bool, river: bool, bridges: bool) -> u8 {
    if from_road && to_road && (!river || bridges) { 1 } else { terrain * 3 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridges_restore_the_road_discount_but_do_not_change_unroaded_costs() {
        for terrain in 1..=3 {
            assert_eq!(land_step_cost(terrain, true, true, true, false), terrain * 3);
            assert_eq!(land_step_cost(terrain, true, true, true, true), 1);
            assert_eq!(land_step_cost(terrain, true, true, false, false), 1);
            assert_eq!(land_step_cost(terrain, false, true, true, true), terrain * 3);
        }
    }

    #[test]
    fn restricted_terrain_requires_a_continuous_road() {
        for from in [false, true] {
            for to in [false, true] {
                assert!(land_terrain_allowed(false, true, false, from, to));
                assert!(land_terrain_allowed(false, false, true, from, to));
                assert_eq!(land_terrain_allowed(false, true, true, from, to), from && to);
                assert_eq!(land_terrain_allowed(true, false, false, from, to), from && to);
            }
        }
    }
}
