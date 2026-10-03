//! The Civ3 map generator's PRNG, ported from the reverse-engineered
//! reference (`reverse-engineering/rust/src/rng.rs`, recovered from
//! `Civ3Conquests.exe` `0x60ba80` / `0x60bab0`).
//!
//! Classic ANSI-C LCG (`s = s * 1103515245 + 12345`) with output taken
//! from the high 16 bits as a float in `[0, 1)`. Every mapgen stage seeds
//! a fresh generator from `water_level + K` with its own per-stage K, so
//! stages are independently reproducible. Combat uses a separate instance
//! of this same class; hut rewards and some AI decisions use MSVC `rand()`
//! (`0x64a20e`), exposed below as `GameRng`.

/// Output scale: `1.0 / 32768.0` (`.rdata` at `0x6716C8` = `2^-15`).
#[cfg(test)]
const SCALE: f64 = 1.0 / 32768.0;
#[cfg(test)]
const A: u32 = 1_103_515_245;
#[cfg(test)]
const C: u32 = 12_345;

/// The map generator's LCG.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapRng {
    inner: civ3mapgen::rng::Rng,
}

impl MapRng {
    /// Fresh stage generator: `water_level + K` per the binary.
    #[inline]
    pub fn new(seed: u32) -> Self {
        MapRng { inner: civ3mapgen::rng::Rng::new(seed) }
    }

    /// `rand01()` — a double in `[0, 1)`.
    #[inline]
    pub fn next_f64(&mut self) -> f64 {
        self.inner.next_f64()
    }

    /// `rand_int(n)` — `(int)(n * rand01())`, a value in `0..n`.
    #[inline]
    pub fn below(&mut self, n: u32) -> i32 {
        debug_assert!(n > 0, "rand_int(0) divides by zero in spirit");
        self.inner.below(n)
    }

    /// Share the same dice state with reverse-engineered gameplay routines.
    pub fn reference(&mut self) -> &mut civ3mapgen::rng::Rng {
        &mut self.inner
    }

    /// Fisher-Yates shuffle with `rand_int(n - i)`, as the placement
    /// stages do (`0x5f22a0`, `0x5f2090`, `0x5eeee0`, ...).
    pub fn shuffle<T>(&mut self, xs: &mut [T]) {
        let n = xs.len();
        for i in 0..n {
            let j = i + self.below((n - i) as u32) as usize;
            xs.swap(i, j.min(n - 1));
        }
    }
}

/// The game's own RNG (`0x64a20e`), for AI/combat/hut rewards — never
/// mapgen. MSVC-compatible `rand()`: `state = state*214013 + 2531011`,
/// output `(state>>16)&0x7FFF`. Ported from `ai.rs` (disassembly-verified).
/// The binary seeds from `timeGetTime()`; we seed from the map seed so a
/// given map plays deterministically.
#[derive(Clone, Debug, bevy::prelude::Resource)]
pub struct GameRng {
    state: u32,
}

impl GameRng {
    #[inline]
    pub fn new(seed: u32) -> Self {
        GameRng { state: seed }
    }

    /// One `rand()` draw, range `0..32768`.
    #[inline]
    pub fn draw(&mut self) -> u32 {
        self.state = self
            .state
            .wrapping_mul(0x343FD)
            .wrapping_add(0x269EC3);
        (self.state >> 16) & 0x7FFF
    }

    /// `rand() % n`, the common call-site idiom (`call; cdq; idiv`).
    #[inline]
    pub fn next_bounded(&mut self, n: u32) -> u32 {
        self.draw() % n
    }

    /// Uniform `[0, 1)` float for reward rolls.
    #[inline]
    pub fn next_f32(&mut self) -> f32 {
        self.next_bounded(1000) as f32 / 1000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_draws_match_the_lcg_definition() {
        let mut r = MapRng::new(0);
        let mut expect = 0u32;
        for _ in 0..8 {
            expect = expect.wrapping_mul(A).wrapping_add(C);
            let want = f64::from((expect >> 16) & 0x7FFF) * SCALE;
            assert!((r.next_f64() - want).abs() < f64::EPSILON);
        }
    }

    #[test]
    fn below_stays_in_range() {
        let mut r = MapRng::new(12345);
        for n in 1..100u32 {
            for _ in 0..20 {
                let v = r.below(n);
                assert!((0..n as i32).contains(&v), "below({n}) gave {v}");
            }
        }
    }

    #[test]
    fn game_rng_first_draw_matches_binary() {
        // Seed 0: state = 2531011, (2531011>>16)&0x7FFF = 38.
        assert_eq!(GameRng::new(0).draw(), 38);
    }

    #[test]
    fn game_rng_bounded_stays_in_range() {
        let mut r = GameRng::new(99);
        for n in 1..200u32 {
            for _ in 0..20 {
                assert!(r.next_bounded(n) < n);
            }
        }
    }

    #[test]
    fn shuffle_is_a_permutation_and_deterministic() {
        let mut r = MapRng::new(0x8CF78);
        let mut a: Vec<usize> = (0..50).collect();
        r.shuffle(&mut a);
        let mut sorted = a.clone();
        sorted.sort();
        assert_eq!(sorted, (0..50).collect::<Vec<_>>());
        let mut r2 = MapRng::new(0x8CF78);
        let mut b: Vec<usize> = (0..50).collect();
        r2.shuffle(&mut b);
        assert_eq!(a, b);
    }
}
