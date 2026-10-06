//! Spiral neighbour enumeration.
//!
//! Recovered from `Civ3Conquests.exe`:
//!
//! * `0x5e6e50` — `void spiralOffset(int n, int *dx, int *dy)`
//! * `0x5e6d20` — `int spiralIndex(int dx, int dy, int limit)` (inverse)
//!
//! # Geometry
//!
//! The generator's cell grid is `(W/2) x H` (see [`crate::cell`]), and
//! `spiralOffset` walks **Manhattan (diamond) rings of radius 2, 4, 6, 8, …**,
//! enumerating *every* cell of each ring:
//!
//! ```text
//! n = 0        -> ( 0,  0)
//! n = 1 .. 8   -> ring 1, |dx| + |dy| == 2   (8 cells)
//! n = 9 .. 24  -> ring 2, |dx| + |dy| == 4   (16 cells)
//! n = 25 .. 48 -> ring 3, |dx| + |dy| == 6   (24 cells)
//! ```
//!
//! Ring `k` therefore holds exactly `8k` cells and ends at index
//! `(2k+1)^2 - 1`, which is the loop in the original:
//!
//! ```text
//! 0x5e6e61  mov eax, 1
//! 0x5e6e66  add eax, 2
//! 0x5e6e69  inc ecx
//! 0x5e6e6c  imul esi, eax
//! 0x5e6e6f  cmp edx, esi
//! 0x5e6e71  jge 0x5e6e66
//! ```
//!
//! Every neighbourhood scan in the generator uses this: radius 1 ring in
//! `0x5eeb00`, rings 0..1 (9 candidates) in `0x5ed5d0`, and up to
//! `(2r+5)^2 - 1` in `0x5ed440`.
//!
//! # Ordering inside a ring
//!
//! The binary has two separate code paths. For `n < 9` (ring 1) the four sides
//! each hold `2k` steps and the cardinals land at `m = 2k, 4k, 6k, 8k`. For
//! `n >= 9` the sides hold `2k-1, 2k-1, 2k-1, 2k-3` steps and the four
//! cardinals are emitted last. Both enumerate the same cells, only the order
//! inside the ring differs, and both are reproduced verbatim below.

/// Number of offsets covered by the first `rings` rings (`(2rings+1)^2 - 1`).
#[inline]
pub const fn spiral_count(rings: u32) -> u32 {
    (2 * rings + 1) * (2 * rings + 1) - 1
}

/// Manhattan radius of the ring containing spiral index `n`.
///
/// `n = 0` maps to radius 0; `n` in `1..=8` to 2; `n` in `9..=24` to 4; and so on.
#[inline]
pub fn spiral_radius(n: i32) -> i32 {
    if n <= 0 {
        return 0;
    }
    let mut k: i32 = 0;
    while (2 * k + 1) * (2 * k + 1) <= n {
        k += 1;
    }
    2 * k
}

/// Maps a spiral index to an `(dx, dy)` offset.
///
/// Faithful to `0x5e6e50`, including the two distinct code paths. Out-of-range
/// inputs return `(0, 0)`.
pub fn spiral_offset(n: i32) -> (i32, i32) {
    if n <= 0 {
        return (0, 0);
    }

    // k = ring number, 1-based. Ring k spans n in ((2k-1)^2) ..= ((2k+1)^2 - 1).
    let mut k: i32 = 0;
    while (2 * k + 1) * (2 * k + 1) <= n {
        k += 1;
    }
    let inner = 2 * k - 1;
    let m = n - inner * inner + 1; // 1-based step around this ring

    if n < 9 {
        // Ring 1 only: four sides of 2k steps each.
        if m <= 2 * k {
            (m, m - 2 * k)
        } else if m <= 4 * k {
            (4 * k - m, m - 2 * k)
        } else if m <= 6 * k {
            (4 * k - m, 6 * k - m)
        } else if m <= 8 * k {
            (m - 8 * k, 6 * k - m)
        } else {
            (0, 0)
        }
    } else {
        // Rings >= 2: the four cardinals are emitted last.
        if m > 8 * k - 4 {
            // The four cardinal points, emitted last.
            match m - (8 * k - 3) {
                0 => (0, -2 * k),
                1 => (2 * k, 0),
                2 => (0, 2 * k),
                3 => (-2 * k, 0),
                _ => (0, 0),
            }
        } else if m < 2 * k {
            (m, m - 2 * k)
        } else if m < 4 * k - 1 {
            (4 * k - (m + 1), (m + 1) - 2 * k)
        } else if m < 6 * k - 2 {
            (4 * k - (m + 2), 6 * k - (m + 2))
        } else if m < 8 * k - 3 {
            ((m + 3) - 8 * k, 6 * k - (m + 3))
        } else {
            (0, 0)
        }
    }
}

/// Inverse lookup: the smallest `n < limit` with `spiral_offset(n) == (dx, dy)`.
///
/// Mirrors `0x5e6d20`, which is a plain linear scan. Returns `None` when the
/// offset is not on the spiral within `limit`.
pub fn spiral_index(dx: i32, dy: i32, limit: i32) -> Option<i32> {
    (0..limit.max(0)).find(|&n| spiral_offset(n) == (dx, dy))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn centre_is_the_origin() {
        assert_eq!(spiral_offset(0), (0, 0));
    }

    #[test]
    fn ring_one_matches_the_binary() {
        // Traced out of 0x5e6e50 for k = 1, m = 1..8.
        let expect = [
            (1, -1),
            (2, 0),
            (1, 1),
            (0, 2),
            (-1, 1),
            (-2, 0),
            (-1, -1),
            (0, -2),
        ];
        for (i, want) in expect.iter().enumerate() {
            assert_eq!(spiral_offset(i as i32 + 1), *want, "n = {}", i + 1);
        }
    }

    #[test]
    fn ring_two_is_the_full_diamond_of_radius_four() {
        let got: HashSet<(i32, i32)> = (9..=24).map(spiral_offset).collect();
        let mut want = HashSet::new();
        for dx in -4i32..=4 {
            for dy in -4i32..=4 {
                if dx.abs() + dy.abs() == 4 {
                    want.insert((dx, dy));
                }
            }
        }
        assert_eq!(got.len(), 16);
        assert_eq!(got, want);
    }

    #[test]
    fn every_cell_of_a_ring_is_distinct_and_on_the_ring() {
        for k in 1..=4i32 {
            let lo = (2 * k - 1) * (2 * k - 1);
            let hi = (2 * k + 1) * (2 * k + 1) - 1;
            let mut seen = HashSet::new();
            for n in lo..=hi {
                let (dx, dy) = spiral_offset(n);
                assert_eq!(
                    dx.abs() + dy.abs(),
                    2 * k,
                    "n = {n} offset {dx},{dy} is not on ring {k}"
                );
                assert!(seen.insert((dx, dy)), "duplicate {dx},{dy} at n = {n}");
            }
            assert_eq!(seen.len(), (8 * k) as usize, "ring {k} size");
        }
    }

    #[test]
    fn radii_are_even_and_grow() {
        assert_eq!(spiral_radius(0), 0);
        for n in 1..=48 {
            assert_eq!(spiral_radius(n) % 2, 0, "odd radius at n = {n}");
        }
        let mut prev = 0;
        for n in 0..=48 {
            let r = spiral_radius(n);
            assert!(r >= prev, "radius shrank at n = {n}");
            prev = r;
        }
        assert_eq!(spiral_radius(48), 6);
    }

    #[test]
    fn inverse_lookup_round_trips() {
        for n in 0..60 {
            let o = spiral_offset(n);
            assert_eq!(spiral_index(o.0, o.1, 200), Some(n), "{o:?}");
        }
    }

    #[test]
    fn inverse_rejects_off_ring_offsets() {
        // (1, 0) sits on the odd ring, which the spiral never visits.
        assert_eq!(spiral_index(1, 0, 200), None);
    }

    #[test]
    fn count_helper() {
        assert_eq!(spiral_count(0), 0);
        assert_eq!(spiral_count(1), 8);
        assert_eq!(spiral_count(2), 24);
        assert_eq!(spiral_count(3), 48);
    }
}
