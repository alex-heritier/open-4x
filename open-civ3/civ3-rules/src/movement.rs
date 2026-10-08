//! Land terrain gate in `Unit::canEnter`. This covers terrain only, after the
//! domain/embarkation checks.

/// Wheeled prototypes and impassable terrain fall through to the road
/// exception: both the origin and destination must have roads.
pub fn land_terrain_allowed(
    impassable: bool,
    impassable_wheeled: bool,
    wheeled: bool,
    from_road: bool,
    to_road: bool,
) -> bool {
    !(impassable || impassable_wheeled && wheeled) || from_road && to_road
}

/// The cost of a land step: a road-to-road step costs one, unless a river is
/// crossed without bridges; otherwise the terrain cost is tripled.
pub fn land_step_cost(terrain: u8, from_road: bool, to_road: bool, river: bool, bridges: bool) -> u8 {
    if from_road && to_road && (!river || bridges) {
        1
    } else {
        terrain * 3
    }
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
