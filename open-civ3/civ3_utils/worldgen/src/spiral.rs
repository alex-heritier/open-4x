//! Diamond (Manhattan) spiral neighbour enumeration.
//!
//! `spiral_offset` walks rings of radius `2, 4, 6, 8, …`, enumerating every
//! cell of each ring: `n = 0` is the origin, `n = 1..=8` is ring 1, `n = 9..=24`
//! is ring 2, and so on. Ring `k` holds exactly `8k` cells and ends at index
//! `(2k+1)^2 - 1`. Every neighbourhood scan uses this, so the order inside a
//! ring matters and is reproduced exactly (the binary has two code paths, for
//! `n < 9` and `n >= 9`).

/// Number of offsets covered by the first `rings` rings (`(2*rings+1)^2 - 1`).
#[inline]
pub const fn spiral_count(rings: u32) -> u32 {
    (2 * rings + 1) * (2 * rings + 1) - 1
}

/// Manhattan radius of the ring containing spiral index `n`.
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

/// Maps a spiral index to an `(dx, dy)` offset. Out-of-range inputs give
/// `(0, 0)`.
pub fn spiral_offset(n: i32) -> (i32, i32) {
    if n <= 0 {
        return (0, 0);
    }

    // k = ring number, 1-based; ring k spans n in ((2k-1)^2) ..= ((2k+1)^2 - 1).
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

/// Inverse lookup: the smallest `n < limit` with `spiral_offset(n) == (dx, dy)`,
/// or `None` when the offset is not on the spiral within `limit`.
pub fn spiral_index(dx: i32, dy: i32, limit: i32) -> Option<i32> {
    (0..limit.max(0)).find(|&n| spiral_offset(n) == (dx, dy))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ring_one_matches_the_binary() {
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
        assert_eq!(spiral_offset(0), (0, 0));
    }

    #[test]
    fn every_cell_of_a_ring_is_distinct_and_on_the_ring() {
        for k in 1..=4i32 {
            let lo = (2 * k - 1) * (2 * k - 1);
            let hi = (2 * k + 1) * (2 * k + 1) - 1;
            let mut seen = HashSet::new();
            for n in lo..=hi {
                let (dx, dy) = spiral_offset(n);
                assert_eq!(dx.abs() + dy.abs(), 2 * k, "n = {n}");
                assert!(seen.insert((dx, dy)), "duplicate {dx},{dy} at n = {n}");
            }
            assert_eq!(seen.len(), (8 * k) as usize, "ring {k} size");
        }
    }

    #[test]
    fn inverse_lookup_round_trips() {
        for n in 0..60 {
            let o = spiral_offset(n);
            assert_eq!(spiral_index(o.0, o.1, 200), Some(n), "{o:?}");
        }
        assert_eq!(spiral_index(1, 0, 200), None, "odd ring is never visited");
    }

    #[test]
    fn count_helper() {
        assert_eq!(spiral_count(0), 0);
        assert_eq!(spiral_count(1), 8);
        assert_eq!(spiral_count(2), 24);
        assert_eq!(spiral_count(3), 48);
    }
}
