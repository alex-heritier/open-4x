//! Terrain disease rolls from `City::diseaseStep`. Citizen removal is a separate
//! operation: on infection, remove one citizen; on subsequent diseased turns,
//! remove one first when size > 1, then pass the resulting size to [`recovers`].

/// Literal technology argument at the cure check. Stock row 8 is Writing,
/// despite TERR's editor label "Cured by Sanitation". No silent correction.
pub const CURE_TECH: i32 = 8;

/// A unit's turn hazard roll: carried units skip hazards; otherwise only a
/// fortified, zero-population-cost PRTO in native TERR row 8 rolls. This rule
/// does not use city disease flags, terrain disease strength, or the city's cure
/// technology.
pub fn unit_jungle_loss(
    carried: bool,
    population_cost: i32,
    fortified: bool,
    terrain: usize,
    mut roll: impl FnMut(u32) -> i32,
) -> bool {
    !carried && population_cost == 0 && fortified && terrain == 8 && roll(1000) & 0xffff == 0
}

/// TERR single-bit tests and its signed percent strength.
#[derive(Clone, Copy, Debug, Default)]
pub struct Terrain {
    /// TERR bit 2, Causes Disease.
    pub causes: bool,
    /// TERR bit 3, Cured by Sanitation.
    pub cured: bool,
    /// TERR signed percent strength.
    pub strength: i32,
}

/// Healthy-city branch, after counting only tiles worked by this city,
/// including its center. Returns the infecting terrain. `roll` is gameplay
/// `rand_int`, which masks the bound to 16 bits. The caller sets
/// diseased/cause=3, posts the message and removes a citizen only when this
/// returns `Some`.
pub fn infection(
    size: i32,
    counts: &[u32; 14],
    terrains: &[Terrain; 14],
    knows_cure: bool,
    mut roll: impl FnMut(u32) -> i32,
) -> Option<usize> {
    // Native visits ascending TERR rows, not the spatial order of tiles.
    for (i, (&count, t)) in counts.iter().zip(terrains).enumerate() {
        if !t.causes || knows_cure && t.cured || t.strength <= 0 {
            continue;
        }
        // Signed multiply first, truncating division, then 0x300 minus the
        // quotient; `rand_int` masks the bound itself.
        let bound = 768i32.wrapping_sub(t.strength.wrapping_mul(512) / 100) as u32;
        for _ in 0..count {
            if roll(bound) & 0xffff <= size {
                // A successful roll ends the loop even at size 1.
                return (size > 1).then_some(i);
            }
        }
    }
    None
}

/// Diseased-city branch after the caller's citizen removal, if any. The caller
/// removes one at size > 1; this rolls 128 and compares with the new size. True
/// clears both diseased and cause code. Size-one cities lose no citizen but
/// still consume this recovery draw.
pub fn recovers(size_after_removal: i32, mut roll: impl FnMut(u32) -> i32) -> bool {
    roll(128) & 0xffff > size_after_removal
}

#[cfg(test)]
mod tests {
    use super::*;
    use civ3_worldgen::rng::Rng;

    #[test]
    fn jungle_unit_loss_skips_immune_units_and_tests_the_low_word() {
        for (carried, cost, fortified, terrain) in [
            (true, 0, true, 8),
            (false, 1, true, 8),
            (false, -1, true, 8),
            (false, 0, false, 8),
            (false, 0, true, 9),
        ] {
            assert!(!unit_jungle_loss(carried, cost, fortified, terrain, |_| panic!(
                "immune units must not draw"
            )));
        }
        for (value, lost) in [(0, true), (1, false), (999, false), (65536, true)] {
            let mut draws = 0;
            assert_eq!(
                unit_jungle_loss(false, 0, true, 8, |bound| {
                    assert_eq!(bound, 1000);
                    draws += 1;
                    value
                }),
                lost
            );
            assert_eq!(draws, 1);
        }
    }

    fn terrains() -> [Terrain; 14] {
        let mut t = [Terrain::default(); 14];
        for i in [4, 8, 9] {
            t[i] = Terrain { causes: true, cured: i == 4, strength: 50 };
        }
        t
    }

    #[test]
    fn infection_and_recovery_use_inclusive_size_thresholds() {
        let mut counts = [0; 14];
        counts[8] = 1;
        for (roll, expected) in [(8, Some(8)), (9, None)] {
            assert_eq!(
                infection(8, &counts, &terrains(), false, |n| {
                    assert_eq!(n, 512);
                    roll
                }),
                expected
            );
        }
        for roll in [0, 4, 5, 127] {
            assert_eq!(recovers(4, |n| {
                assert_eq!(n, 128);
                roll
            }), roll > 4);
        }
        assert!(!recovers(1, |_| 1));
        assert!(recovers(1, |_| 2));
    }

    #[test]
    fn size_one_still_rolls_but_never_infects_and_stops_on_success() {
        let mut counts = [0; 14];
        counts[8] = 2;
        let mut draws = 0;
        assert_eq!(infection(1, &counts, &terrains(), false, |_| {
            draws += 1;
            0
        }), None);
        assert_eq!(draws, 1);
    }

    #[test]
    fn cure_technology_removes_only_flagged_terrain_without_draws() {
        let mut counts = [0; 14];
        counts[4] = 1;
        assert_eq!(infection(8, &counts, &terrains(), true, |_| panic!(
            "cured Flood Plain must not roll"
        )), None);
        counts[8] = 1;
        let mut draws = 0;
        assert_eq!(infection(8, &counts, &terrains(), true, |_| {
            draws += 1;
            0
        }), Some(8));
        assert_eq!(draws, 1);
    }

    #[test]
    fn draws_follow_terrain_order_and_stop_at_first_infection() {
        let mut counts = [0; 14];
        counts[4] = 1;
        counts[8] = 2;
        counts[9] = 1;
        let mut rolls = [9, 9, 8].into_iter();
        assert_eq!(infection(8, &counts, &terrains(), false, |_| rolls.next().unwrap()), Some(8));
        assert_eq!(rolls.next(), None);
    }

    #[test]
    fn strength_bounds_keep_native_truncation_zero_and_negative_semantics() {
        let mut counts = [0; 14];
        counts[8] = 1;
        for (strength, bound) in [(50, 512), (100, 256), (25, 640), (10, 717), (150, 0), (151, (-5i32) as u32)] {
            let mut t = terrains();
            t[8].strength = strength;
            let mut rng = Rng::new(1);
            let result = infection(8, &counts, &t, false, |n| {
                assert_eq!(n, bound);
                rng.below(n)
            });
            assert_ne!(rng.state(), 1, "even a zero bound consumes a draw");
            if strength == 150 {
                assert_eq!(result, Some(8));
            }
            if strength == 151 {
                assert_eq!(result, None);
            }
        }
        for strength in [-1, 0] {
            let mut t = terrains();
            t[8].strength = strength;
            assert_eq!(infection(8, &counts, &t, false, |_| panic!(
                "nonpositive strength must not roll"
            )), None);
        }
    }
}
