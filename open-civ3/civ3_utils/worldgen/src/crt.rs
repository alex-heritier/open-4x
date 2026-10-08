//! The C-runtime `qsort` whose exact swap order leaks into the generator.
//!
//! The generator numbers continents by calling the statically linked MSVC 6
//! `qsort` with a comparator that returns 0 for two equally sized continents.
//! The routine is not stable, so which of two equal islands gets the lower id
//! is decided by its swaps, and every later stage that walks continents in id
//! order sees that choice. This is the pre-median-of-three VC4/VC6 variant:
//! sub-arrays of at most [`CUTOFF`] elements go to a selection sort; larger
//! ones move the middle element to the front, partition against it, and swap
//! it back.

/// Sub-arrays of this many elements or fewer are selection-sorted.
pub const CUTOFF: usize = 8;

/// `qsort(a, a.len(), size_of::<T>(), cmp)` of the game's C runtime.
///
/// `cmp(x, y)` follows the C convention: positive when `x` sorts after `y`.
pub fn crt_qsort<T, F: FnMut(&T, &T) -> i32>(a: &mut [T], mut cmp: F) {
    if a.len() < 2 {
        return;
    }
    // The routine's pseudo-recursion stack of pending inclusive (lo, hi) pairs.
    let mut pending: Vec<(usize, usize)> = Vec::new();
    let (mut lo, mut hi) = (0usize, a.len() - 1);
    loop {
        let size = hi - lo + 1;
        if size <= CUTOFF {
            shortsort(a, lo, hi, &mut cmp);
        } else {
            // The middle element becomes the pivot at `lo`.
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

            // The two sub-arrays; each is only sorted when it has two or more
            // elements. Which is visited first cannot change the result, only
            // the stack depth.
            let left = (lo + 1 < higuy).then(|| (lo, higuy - 1));
            let right = (loguy < hi).then(|| (loguy, hi));
            let left_len = higuy - lo;
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

/// Repeatedly move the maximum of `[lo, hi]` to `hi`.
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

    #[test]
    fn sorts_ascending_with_ties_kept_in_a_fixed_order() {
        let mut v: Vec<u32> = (0..500u32)
            .map(|i| i.wrapping_mul(2_654_435_761u32) >> 24)
            .collect();
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
