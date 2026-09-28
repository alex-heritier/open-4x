//! Resource placement, stage 10 (`0x5f22a0`).
//!
//! Implements the `.biq`-data-independent math of `placeResources`: the
//! default frequency roll, the quantity formula, the terrain-score weighting,
//! and the block-3 acceptance probabilities. See `../resources.md`.
//!
//! Fully specified no-data stages `placeGoodyHuts` (`0x5f21b0`) and
//! `placeBarbarianCamps` (`0x5f2090`) live here too.
//!
//! Every constant cites the address it came from.

use crate::rng::Rng;

/// A `GOOD` resource row: frequency from `good->[0x40]`. `0x5f22a0`.
#[derive(Clone, Copy, Debug)]
pub struct Good {
    /// Per-resource frequency. `0` means "roll it". (`0x5f22a0`)
    pub freq: i32,
}

impl Good {
    /// `pct = freq ? freq : rand_int(26) + rand_int(26) + 50`. (`0x5f22a0`)
    pub fn frequency(&self, rng: &mut Rng) -> i32 {
        if self.freq != 0 {
            self.freq
        } else {
            rng.below(26) + rng.below(26) + 50
        }
    }
}

/// Quantity of copies for one resource. (`0x5f22a0`)
///
/// ```text
/// n1 = (area_factor * pct) / 32
/// n  = score<2 ? n1*0.5 : score<4 ? n1*0.75 : n1
/// n  = max(n, score>=4 ? 2 : 1)
/// ```
/// `score` counts `TERR` rows allowing the resource, +4 each for `t >= 11`.
pub fn quantity(area_factor: i32, pct: i32, score: i32) -> i32 {
    let n1 = (area_factor * pct) / 32;
    let n = if score < 2 {
        (n1 as f32 * 0.5) as i32
    } else if score < 4 {
        (n1 as f32 * 0.75) as i32
    } else {
        n1
    };
    n.max(if score >= 4 { 2 } else { 1 })
}

/// Block-3 die sides by score band. (`0x5f22a0`, branch at `0x5F2B3E`)
///
/// The roll **skips** iff `rand_int(sides) > 1` (`cmp ax,1; ja 0x5F2C06`,
/// where `0x5F2C06` advances the loop): acceptance is `roll <= 1`, i.e.
/// 2/6 = 33 %, 2/4 = 50 %, 2/2 = 100 %.
pub fn block3_sides(score: i32) -> u32 {
    if score < 2 {
        6
    } else if score < 4 {
        4
    } else {
        2
    }
}

/// Block-3 skip predicate: `true` means no placement this round. (`0x5F2B62`)
pub fn block3_skip(roll: u32) -> bool {
    roll > 1
}

/// All copies of one resource must share one `vfunc(0xB8)` region id.
pub fn same_region(regions: &[u16]) -> bool {
    regions.iter().all(|&r| r == regions[0])
}

/// `placeGoodyHuts` (`0x5f21b0`): count of hut rolls for a goody count.
///
/// Returns `None` when the stage is a no-op: `count == -1`, or the
/// `>= 32` guard fails (no-op in every normal game, max 31 civs).
pub fn goody_hut_rolls(goody_count: i32) -> Option<i32> {
    if goody_count == -1 {
        return None;
    }
    if (goody_count & 0xFFE0) == 0 {
        return None;
    }
    Some(goody_count >> 5)
}

/// `placeBarbarianCamps` (`0x5f2090`): 1-in-3 per eligible land tile.
pub fn barbarian_camp_lands(rng: &mut Rng) -> bool {
    rng.one_in(3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_frequency_range() {
        let mut rng = Rng::new(0x180E3); // 0x5f22a0 resource seed
        for _ in 0..50 {
            let p = Good { freq: 0 }.frequency(&mut rng);
            assert!((50..100).contains(&p));
        }
        assert_eq!(Good { freq: 70 }.frequency(&mut rng), 70);
    }

    #[test]
    fn quantity_weighting() {
        // score 0: half, min 1. score 2..3: three quarters. score 4+: full, min 2.
        assert_eq!(quantity(320, 80, 0), 400);
        assert_eq!(quantity(320, 80, 2), 600);
        assert_eq!(quantity(320, 80, 5), 800);
        assert_eq!(quantity(0, 80, 0), 1);
        assert_eq!(quantity(0, 80, 5), 2);
    }

    #[test]
    fn block3_probabilities() {
        assert_eq!(block3_sides(0), 6);
        assert_eq!(block3_sides(3), 4);
        assert_eq!(block3_sides(4), 2);
        // Acceptance is roll <= 1 over rand_int(sides) in 0..sides.
        let rate = |sides: u32| {
            (0..sides).filter(|&r| !block3_skip(r)).count()
        };
        assert_eq!(rate(6), 2); // 33 %
        assert_eq!(rate(4), 2); // 50 %
        assert_eq!(rate(2), 2); // 100 %
    }

    #[test]
    fn goody_guard_is_noop_for_normal_games() {
        assert_eq!(goody_hut_rolls(-1), None);
        assert_eq!(goody_hut_rolls(31), None); // max real civ count
        assert_eq!(goody_hut_rolls(32), Some(1));
    }

    #[test]
    fn same_region_invariant() {
        assert!(same_region(&[7, 7, 7]));
        assert!(!same_region(&[7, 8, 7]));
    }

    #[test]
    fn good_section_layout() {
        // GOOD section of decoded conquests.biq: tag + u16 26 at +4.
        // Row stride 0x5C comes from the loader loop (0x5974B4), not the file.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let p = root.join("../../../civ3-gog/app/Conquests/conquests.biq");
        let raw = std::fs::read(&p).unwrap_or_else(|_| panic!("missing {}", p.display()));
        let out = crate::dcl::decompress(&raw).expect("biq decodes");
        let at = out.windows(4).position(|w| w == b"GOOD").expect("GOOD section");
        assert_eq!(u16::from_le_bytes([out[at + 4], out[at + 5]]), 26);
    }
}
