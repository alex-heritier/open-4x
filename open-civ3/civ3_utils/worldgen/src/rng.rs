//! The generator's pseudo-random generator: a classic ANSI-C LCG
//! (`s = s * 1103515245 + 12345`) whose output is the high 16 bits as a float
//! in `[0, 1)`.
//!
//! The same class is also the gameplay die (combat, retreat, bombard, ...), but
//! the generator keeps its own instances, seeded
//! `water_level + <per-stage constant>`, so generating a map never disturbs the
//! gameplay stream.

/// Multiplier of the LCG.
const A: u32 = 1_103_515_245;
/// Increment of the LCG.
const C: u32 = 12_345;
/// Output scale: `1.0 / 32768.0` (`2^-15`).
const SCALE: f64 = 1.0 / 32768.0;

/// The generator's LCG.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rng {
    state: u32,
}

impl Rng {
    /// Creates a generator from an explicit seed.
    #[inline]
    pub fn new(seed: u32) -> Self {
        Rng { state: seed }
    }

    /// The generator's whole state (a saved game keeps it; `new` restores it).
    #[inline]
    pub fn state(&self) -> u32 {
        self.state
    }

    /// `rand01()` — a double in `[0, 1)`.
    #[inline]
    pub fn next_f64(&mut self) -> f64 {
        self.state = self.state.wrapping_mul(A).wrapping_add(C);
        let bits = (self.state >> 16) & 0x7FFF;
        f64::from(bits) * SCALE
    }

    /// `rand_int(n)` — `(int)((n & 0xFFFF) * rand01())`, a value in
    /// `0..(n & 0xFFFF)`. The argument is masked to 16 bits before the multiply;
    /// truncation is toward zero.
    #[inline]
    pub fn below(&mut self, n: u32) -> i32 {
        (f64::from(n & 0xFFFF) * self.next_f64()) as i32
    }

    /// `true` with probability `1/n`.
    #[inline]
    pub fn one_in(&mut self, n: u32) -> bool {
        self.below(n) == 0
    }

    /// Adds `by` to the state without drawing.
    #[inline]
    pub fn skew(&mut self, by: u32) {
        self.state = self.state.wrapping_add(by);
    }

    /// Advances the LCG `n` times, discarding the results (used to decorrelate
    /// successive seeds).
    #[inline]
    pub fn discard(&mut self, n: usize) {
        for _ in 0..n {
            self.next_f64();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_lcg_definition() {
        let mut r = Rng::new(0);
        let mut state = 0u32;
        for _ in 0..16 {
            state = state.wrapping_mul(A).wrapping_add(C);
            let want = f64::from((state >> 16) & 0x7FFF) * SCALE;
            assert_eq!(r.next_f64(), want);
        }
    }

    #[test]
    fn below_stays_in_range_and_repeats() {
        for n in 1..200u32 {
            let mut a = Rng::new(7);
            let mut b = Rng::new(7);
            for _ in 0..20 {
                let v = a.below(n);
                assert!((0..n as i32).contains(&v), "below({n}) gave {v}");
                assert_eq!(v, b.below(n));
            }
        }
    }
}
