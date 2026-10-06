//! C-runtime routines whose exact behaviour leaks into the generator's output.
//!
//! # `qsort` (`0x64baaf`)
//!
//! `finalizeMap` (`0x5eb7d0`) numbers the continents by calling the statically linked
//! MSVC 6 `qsort` with a comparator that returns 0 for two continents of equal size.
//! The routine is not stable, so *which of two equal-sized islands gets the lower id*
//! is decided by the algorithm's swaps. Every later stage that walks continents in id
//! order (start placement, resources) sees that order, so the port reproduces the sort
//! step for step instead of using `sort_by`.
//!
//! The structure is read from the machine code: sub-arrays of at most [`CUTOFF`]
//! elements go to `shortsort` (`0x64bc03`); larger ones swap their middle element to
//! the front (`0x64bc51`, called from `0x64bb38`), partition against it with
//! `higuy = hi + width`, and swap it back into place. This is the pre-median-of-three
//! variant that ships with VC 4 to VC 6. `tests/data/crt_qsort.txt` holds 120 cases
//! produced by running the exe's own `0x64baaf` under Unicorn
//! (`.agents/skills/reverse-engineering-executables/scripts/emu/gen_qsort_fixtures.py`);
//! the port matches all of them, ties included.

/// Sub-arrays of this many elements or fewer are selection-sorted (`cmp eax, 8` at
/// `0x64bafa`).
pub const CUTOFF: usize = 8;

/// `qsort(a, a.len(), size_of::<T>(), cmp)` of the game's C runtime.
///
/// `cmp(x, y)` follows the C convention: positive when `x` sorts after `y`.
pub fn crt_qsort<T, F: FnMut(&T, &T) -> i32>(a: &mut [T], mut cmp: F) {
    if a.len() < 2 {
        return;
    }
    // The pseudo-recursion stack of `0x64baee`: pending (lo, hi) pairs, inclusive.
    let mut pending: Vec<(usize, usize)> = Vec::new();
    let (mut lo, mut hi) = (0usize, a.len() - 1);
    loop {
        let size = hi - lo + 1;
        if size <= CUTOFF {
            shortsort(a, lo, hi, &mut cmp);
        } else {
            // 0x64bb2e..0x64bb38: the middle element becomes the pivot at `lo`.
            a.swap(lo + size / 2, lo);

            let mut loguy = lo;
            let mut higuy = hi + 1;
            loop {
                loop {
                    loguy += 1;
                    if !(loguy <= hi && cmp(&a[loguy], &a[lo]) <= 0) {
                        break;
                    }
                }
                loop {
                    higuy -= 1;
                    if !(higuy > lo && cmp(&a[higuy], &a[lo]) >= 0) {
                        break;
                    }
                }
                if higuy < loguy {
                    break;
                }
                a.swap(loguy, higuy);
            }
            a.swap(lo, higuy);

            // Sub-arrays [lo, higuy - 1] and [loguy, hi]; each is only sorted when it
            // has two or more elements. The order they are visited in cannot change
            // the result, only the stack depth.
            let left = (lo + 1 < higuy).then(|| (lo, higuy - 1));
            let right = (loguy < hi).then(|| (loguy, hi));
            let left_len = higuy - lo; // higuy - 1 - lo + 1
            let right_len = hi + 1 - loguy;
            let (small, big) = if left_len > right_len {
                (right, left)
            } else {
                (left, right)
            };
            if let Some(b) = big {
                pending.push(b);
            }
            if let Some((l, h)) = small {
                lo = l;
                hi = h;
                continue;
            }
        }
        match pending.pop() {
            Some((l, h)) => {
                lo = l;
                hi = h;
            }
            None => return,
        }
    }
}

/// `0x64bc03`: repeatedly move the maximum of `[lo, hi]` to `hi`.
fn shortsort<T, F: FnMut(&T, &T) -> i32>(a: &mut [T], lo: usize, mut hi: usize, cmp: &mut F) {
    while hi > lo {
        let mut max = lo;
        for p in lo + 1..=hi {
            if cmp(&a[p], &a[max]) > 0 {
                max = p;
            }
        }
        a.swap(max, hi);
        hi -= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cases produced by the exe's own qsort: `n | keys | resulting order`.
    const CASES: &str = include_str!("../tests/data/crt_qsort.txt");

    #[test]
    fn matches_the_exes_qsort_on_every_fixture() {
        let mut checked = 0;
        for line in CASES.lines().filter(|l| !l.trim().is_empty()) {
            let mut parts = line.split('|');
            let n: usize = parts.next().unwrap().trim().parse().unwrap();
            let nums = |s: &str| -> Vec<i32> { s.split_whitespace().map(|v| v.parse().unwrap()).collect() };
            let keys = nums(parts.next().unwrap());
            let want = nums(parts.next().unwrap());
            assert_eq!(keys.len(), n);

            let mut order: Vec<i32> = (0..n as i32).collect();
            crt_qsort(&mut order, |&x, &y| keys[x as usize] - keys[y as usize]);
            assert_eq!(order, want, "keys {keys:?}");
            checked += 1;
        }
        assert!(checked >= 100, "fixture file went missing");
    }

    #[test]
    fn sorts_correctly_even_where_ties_make_the_order_arbitrary() {
        let mut v: Vec<u32> = (0..500u32).map(|i| i.wrapping_mul(2_654_435_761u32) >> 24).collect();
        crt_qsort(&mut v, |a, b| *a as i32 - *b as i32);
        assert!(v.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn short_inputs_are_left_alone() {
        let mut one = [5];
        crt_qsort(&mut one, |a: &i32, b: &i32| a - b);
        assert_eq!(one, [5]);
        let mut none: [i32; 0] = [];
        crt_qsort(&mut none, |a, b| a - b);
    }
}
