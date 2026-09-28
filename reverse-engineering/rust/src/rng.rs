//! The map generator's private pseudo-random number generator.
//!
//! Recovered from `Civ3Conquests.exe`:
//!
//! * `0x60ba80` — `double rand01(uint32 *state)`
//! * `0x60bab0` — `int rand_int(uint32 n)`
//!
//! This is the classic ANSI-C linear congruential generator
//! `s = s * 1103515245 + 12345`, but only the **high 16 bits of the state** are
//! used for the output, and the result is a float in `[0, 1)`:
//!
//! ```text
//! 0x60ba80  mov  eax, [ecx]
//! 0x60ba83  imul eax, eax, 0x41C64E6D     ; 1103515245
//! 0x60ba89  add  eax, 0x3039              ; 12345
//! 0x60ba8e  mov  [ecx], eax
//! 0x60ba90  mov  cx, [ecx+2]              ; bits 16..31
//! 0x60ba94  and  ecx, 0x7FFF
//! 0x60ba9e  fild dword [esp]
//! 0x60baa2  fmul qword [0x6716C8]         ; 2^-15
//! ```
//!
//! This is a *different* generator from the game's own `rand()` at `0x64a20e`
//! (MSVC's LCG, state stored at `this+0x14`). The map generator never uses the
//! game RNG, so map generation is completely independent of combat, AI and the
//! save file.

/// Multiplier of the map generator's LCG (`0x41C64E6D`).
const A: u32 = 1_103_515_245;
/// Increment of the map generator's LCG (`0x3039`).
const C: u32 = 12_345;
/// Output scale: `1.0 / 32768.0` (`.rdata` at `0x6716C8` = `2^-15`).
const SCALE: f64 = 1.0 / 32768.0;

/// The map generator's LCG.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rng {
    state: u32,
}

impl Rng {
    /// Creates a generator from an explicit seed.
    ///
    /// Every stage of the pipeline seeds a fresh `Rng` from
    /// `water_level + <per-stage constant>`; see [`crate::pipeline`].
    #[inline]
    pub fn new(seed: u32) -> Self {
        Rng { state: seed }
    }

    /// `rand01()` — returns a double in `[0, 1)`.
    #[inline]
    pub fn next_f64(&mut self) -> f64 {
        self.state = self.state.wrapping_mul(A).wrapping_add(C);
        let bits = (self.state >> 16) & 0x7FFF;
        f64::from(bits) * SCALE
    }

    /// `rand_int(n)` — returns `(int)(n * rand01())`, i.e. a value in
    /// `0..n`. Truncation is toward zero, matching MSVC's `_ftol2`.
    #[inline]
    pub fn below(&mut self, n: u32) -> i32 {
        (f64::from(n) * self.next_f64()) as i32
    }

    /// Returns `true` with probability `1/n`.
    #[inline]
    pub fn one_in(&mut self, n: u32) -> bool {
        self.below(n) == 0
    }

    /// Advances the LCG `n` times, discarding the results.
    ///
    /// The original code does this to decorrelate successive seeds, e.g. in
    /// `0x5edb70` (`for (i = 0; i < num_players; i++) rand_int(&st, num_players - i);`
    /// with the return value never used) and in `0x5f1f50` (three warm-up draws
    /// before the option randomisation).
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
    fn first_draws_match_the_binary() {
        // Hand-computed from the LCG definition.
        let mut r = Rng::new(0);
        let mut expect = 0u32;
        for _ in 0..8 {
            expect = expect.wrapping_mul(A).wrapping_add(C);
            let want = f64::from((expect >> 16) & 0x7FFF) * SCALE;
            assert!((r.next_f64() - want).abs() < f64::EPSILON);
        }
    }

    #[test]
    fn output_is_in_unit_interval() {
        let mut r = Rng::new(0xCC98);
        for _ in 0..10_000 {
            let v = r.next_f64();
            assert!((0.0..1.0).contains(&v), "{v}");
        }
    }

    #[test]
    fn below_is_in_range() {
        let mut r = Rng::new(12345);
        for n in 1..300u32 {
            for _ in 0..50 {
                let v = r.below(n);
                assert!((0..n as i32).contains(&v), "below({n}) gave {v}");
            }
        }
    }

    #[test]
    fn below_is_reasonably_uniform() {
        // 3 is the workhorse modulus in the generator (0x5edb70, 0x5ed5d0, ...).
        let mut r = Rng::new(0xD431);
        let mut counts = [0u32; 3];
        for _ in 0..30_000 {
            counts[r.below(3) as usize] += 1;
        }
        for c in counts {
            assert!((9_000..11_000).contains(&c), "skewed: {counts:?}");
        }
    }

    #[test]
    fn distribution_of_256_looks_flat() {
        // The coarse level of the fractal is seeded with rand_int(256); if this
        // were biased the landmass would show visible stripes.
        let mut r = Rng::new(1);
        let mut counts = [0u32; 256];
        for _ in 0..256 * 400 {
            counts[r.below(256) as usize] += 1;
        }
        let mean = counts.iter().map(|&c| c as i64).sum::<i64>() / 256;
        for (i, &c) in counts.iter().enumerate() {
            assert!(
                (c as i64 - mean).abs() < mean / 4,
                "bin {i} has {c}, mean {mean}"
            );
        }
    }
}
