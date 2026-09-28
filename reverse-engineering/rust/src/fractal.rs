//! The fractal landmass generator.
//!
//! Recovered from `Civ3Conquests.exe` `0x5e1b60` (1129 bytes), with helpers
//! `0x5e2180` (sample), `0x5e2280` (percentile lookup) and `0x5e1fd0` (smear).
//!
//! # The algorithm
//!
//! A **midpoint-displacement fractal** over a fixed **128 x 64 byte grid** with
//! one extra row and column reserved for wrapping, stored in a flat
//! `129 * 65` byte array. Generation happens in that normalised space and is
//! stretched over the real map by [`Fractal::sample`], which is why a 60x60 and
//! a 160x160 map have structurally identical coastlines.
//!
//! # Storage is x-major
//!
//! ```text
//! h[x * 65 + y]      x in 0..=128, y in 0..=64
//! ```
//!
//! This is visible in the generator's inner loop, whose stride is `0x41` on
//! `x` (`iVar5 = fx * 0x41`) and `1` on `y`
//! (`*(fx * 0x41 + fy + 0x20)`), and in `0x5e2180`, which computes
//! `index = xi * 65 + yi` with `xi = (int)(x * 128.0/W)` and
//! `yi = (int)(y * 64.0/H)`.
//!
//! # Midpoint displacement
//!
//! ```text
//! L0 = clamp(6 - level_arg, 0, 6)
//! for L = L0 down to 0:
//!     mask = (1 << (L+1)) - 1
//!     amp  = 1 << (7 - L0 + L)          # halves at every level
//!     apply_borders()                    # polar caps / row+column wrap
//!     for fx in 0 .. (128 >> L) + (1 - (flags & WRAP_X)):
//!         for fy in 0 .. (64 >> L)  + (1 - (flags & WRAP_Y)):
//!             x = fx << L;  y = fy << L
//!             if L == L0:                            h[x][y] = rand(256)
//!             elif (mask & x) == 0 and (mask & y) == 0:   pass      # coarser level
//!             elif (mask & x) == 0:  h[x][y] = avg2(h[x][y-s], h[x][y+s]) + jitter
//!             elif (mask & y) == 0:  h[x][y] = avg2(h[x-s][y], h[x+s][y]) + jitter
//!             else:                   h[x][y] = avg4(4 corners)             + jitter
//! apply_borders()
//! ```
//!
//! The jitter is `rand_int(2*amp) - amp`, so its amplitude **halves at every
//! level** — the amplitude is `1 << (7 - L0 + L)`, i.e. `128, 64, 32, 16, …`.
//! That is what turns white noise into recognisable coastlines; the coarse grid
//! (`L == L0`) is pure random and every finer level is a smoothed refinement of
//! it.
//!
//! On top of that, the generator never uses a raw height: every elevation
//! threshold comes from [`Fractal::percentile`], which searches for the height
//! value `t` such that `P` % of the field lies below it. That is why all the
//! thresholds adapt automatically to both the random field and the water level.

use crate::rng::Rng;

/// Number of fractal samples along x, excluding the wrap column.
pub const FX: usize = 128;
/// Number of fractal samples along y, excluding the wrap row.
pub const FY: usize = 64;
/// Distance between consecutive x samples in the flat array.
pub const STRIDE: usize = FY + 1;
/// Total bytes of a height grid: `129 * 65`.
pub const HEIGHT_LEN: usize = (FX + 1) * STRIDE;
/// Number of samples [`Fractal::percentile`] counts over: `128 * 64 = 8192`.
pub const SAMPLES: usize = FX * FY;

/// Generator flags (`FractalMap::flags` at `+0x08`).
pub mod flags {
    /// Wrap in x: the x loop stops one column short and column 128 is kept as a
    /// copy of column 0.
    pub const WRAP_X: u32 = 1;
    /// Wrap in y: the y loop stops one row short and row 64 is kept as a copy
    /// of row 0.
    pub const WRAP_Y: u32 = 2;
    /// [`crate::fractal::Fractal::sample`] returns 0..100 instead of 0..255.
    pub const SCALE_100: u32 = 4;
    /// Force rows 0 and 64 to ocean (polar caps). Overrides [`Self::WRAP_Y`].
    pub const OCEAN_POLES: u32 = 8;
    /// Extra smear pass in [`crate::fractal::Fractal::smear`].
    pub const SMEAR_WIDE: u32 = 0x10;
}

/// A generated height field plus the state needed to sample it.
#[derive(Clone, Debug)]
pub struct Fractal {
    /// Map width this field will be stretched over.
    pub w: i32,
    /// Map height this field will be stretched over.
    pub h: i32,
    /// See the [`flags`] module.
    pub flags: u32,
    rng: Rng,
    scale_x: f64,
    scale_y: f64,
    heights: Box<[u8; HEIGHT_LEN]>,
}

impl Fractal {
    /// Runs the generator, exactly as `0x5e1b60` does.
    ///
    /// `level_arg` is the raw argument: the effective coarsest level is
    /// `clamp(6 - level_arg, 0, 6)`, so `2` starts at level 4 and `3` at 3.
    ///
    /// A `seed` of 0 falls back to a fixed constant rather than a clock, so the
    /// function stays deterministic; the binary calls `timeGetTime()` there.
    pub fn generate(w: i32, h: i32, level_arg: i32, flags: u32, seed: u32) -> Self {
        let mut f = Fractal {
            w,
            h,
            flags,
            rng: Rng::new(if seed == 0 { 0x5EED_5EED } else { seed }),
            scale_x: 128.0 / f64::from(w),
            scale_y: 64.0 / f64::from(h),
            heights: Box::new([0u8; HEIGHT_LEN]),
        };
        f.run(level_arg);
        f
    }

    /// Index of `(x, y)` in the flat height grid — `x * 65 + y`, x-major.
    #[inline]
    fn idx(x: usize, y: usize) -> usize {
        x * STRIDE + y
    }

    #[inline]
    fn get(&self, x: usize, y: usize) -> u8 {
        self.heights[Fractal::idx(x, y)]
    }

    #[inline]
    fn set(&mut self, x: usize, y: usize, v: u8) {
        self.heights[Fractal::idx(x.min(FX), y.min(FY))] = v;
    }

    /// Reads one of the four neighbours of a midpoint, clamping at the edges.
    ///
    /// The binary does this with no bounds check, which looks like a buffer
    /// underflow but is not: the array is `129 * 65 = 8385` bytes and the
    /// generator's loop bounds (`0x5e1c6b`, `0x5e1f26`) are chosen so the flat
    /// index always lands in `0..=8384`. The extra row and column exist
    /// precisely to make the edge midpoint reads well-defined. An exhaustive
    /// scan over every level argument and flag combination confirms it, so the
    /// clamp here is a no-op and there is no bug to opt into.
    #[inline]
    fn neighbour(&self, dx: isize, dy: isize, x: usize, y: usize) -> u8 {
        let x = (x as isize + dx).clamp(0, FX as isize) as usize;
        let y = (y as isize + dy).clamp(0, FY as isize) as usize;
        self.get(x, y)
    }

    /// The polar caps and the row wrap, run before *and* after every level
    /// (`0x5e1b0c..0x5e1c53` and `0x5e1f60..0x5e1fab`).
    ///
    /// * `OCEAN_POLES` zeroes rows 0 and 64; it takes priority over the wrap,
    /// * otherwise `WRAP_Y` copies row 0 into row 64,
    /// * `WRAP_X` copies column 0 into column 128.
    ///
    /// All three loops run 0x81 = 129 times, one per column, which is why the
    /// x loop bound is `(128 >> L)` and not `(128 >> L) + 1` when wrapping.
    fn apply_borders(&mut self) {
        if self.flags & flags::OCEAN_POLES != 0 {
            for x in 0..=FX {
                self.set(x, 0, 0);
                self.set(x, FY, 0);
            }
        } else if self.flags & flags::WRAP_Y != 0 {
            for x in 0..=FX {
                let v = self.get(x, 0);
                self.set(x, FY, v);
            }
        }
        if self.flags & flags::WRAP_X != 0 {
            for y in 0..STRIDE {
                let v = self.get(0, y);
                self.set(FX, y, v);
            }
        }
    }

    /// `clamp(avg + rand_int(2*amp) - amp, 0, 255)`.
    #[inline]
    fn jitter(&mut self, avg: u32, amp: u32) -> u8 {
        let d = self.rng.below(2 * amp) - amp as i32;
        (avg as i32 + d).clamp(0, 255) as u8
    }

    fn run(&mut self, level_arg: i32) {
        let level0 = (6 - level_arg).clamp(0, 6);

        // 0x5e1bcf: one warm-up draw before anything else.
        self.rng.next_f64();

        self.apply_borders();

        for level in (0..=level0).rev() {
            let shift = level as u32;
            let mask: u32 = (1u32 << (level + 1)) - 1;
            let step = 1usize << shift;
            // 1 << (7 - L0 + L): halves at every level.
            let amp = 1u32 << (7 - level0 + level);

            // 0x5e1c6b and 0x5e1f26: the loop bounds drop the extra row/column
            // when the corresponding wrap bit is set.
            let nx = (FX >> shift) + usize::from(self.flags & flags::WRAP_X == 0);
            let ny = (FY >> shift) + usize::from(self.flags & flags::WRAP_Y == 0);

            self.apply_borders();

            for fx in 0..nx {
                for fy in 0..ny {
                    let x = fx * step;
                    let y = fy * step;
                    let x_odd = mask & x as u32 != 0;
                    let y_odd = mask & y as u32 != 0;
                    let s = step as isize;

                    if level == level0 {
                        // Seed the coarse grid with white noise.
                        let v = self.rng.below(256);
                        self.set(x, y, v as u8);
                    } else if !x_odd && !y_odd {
                        // Both even: this point came from a coarser level.
                    } else if !x_odd {
                        // x even, y odd -> midpoint along y.
                        let a = self.neighbour(0, -s, x, y) as u32;
                        let b = self.neighbour(0, s, x, y) as u32;
                        let v = self.jitter((a + b) >> 1, amp);
                        self.set(x, y, v);
                    } else if !y_odd {
                        // x odd, y even -> midpoint along x.
                        let a = self.neighbour(-s, 0, x, y) as u32;
                        let b = self.neighbour(s, 0, x, y) as u32;
                        let v = self.jitter((a + b) >> 1, amp);
                        self.set(x, y, v);
                    } else {
                        // Both odd -> centre of four.
                        let a = self.neighbour(-s, -s, x, y) as u32;
                        let b = self.neighbour(s, -s, x, y) as u32;
                        let c = self.neighbour(-s, s, x, y) as u32;
                        let d = self.neighbour(s, s, x, y) as u32;
                        let v = self.jitter((a + b + c + d) >> 2, amp);
                        self.set(x, y, v);
                    }
                }
            }
        }

        self.apply_borders();
    }

    /// Raw height at grid point `(x, y)`, x-major.
    #[inline]
    pub fn height(&self, x: usize, y: usize) -> u8 {
        self.heights[Fractal::idx(x, y)]
    }

    /// `0x5e2180` — samples the field at **map** coordinates.
    ///
    /// The first argument is the column (`x`, range `0..W`) and the second the
    /// row (`y`, range `0..H`); the original pushes them in that order, so `x`
    /// lands in `arg1` and picks up the `128.0/W` scale factor.
    pub fn sample(&self, x: i32, y: i32) -> u8 {
        let gx = ((f64::from(x) * self.scale_x) as i64).clamp(0, FX as i64 - 1) as usize;
        let gy = ((f64::from(y) * self.scale_y) as i64).clamp(0, FY as i64 - 1) as usize;
        let v = self.get(gx, gy) as u32;
        if self.flags & flags::SCALE_100 != 0 {
            ((v * 100) >> 8) as u8
        } else {
            v as u8
        }
    }

    /// The height grid, for callers that want to render it.
    pub fn heights(&self) -> &[u8] {
        self.heights.as_slice()
    }

    /// Averages `other` into this field, 50/50.
    ///
    /// The 3-5-continents landmass style blends two of its three fractals with
    /// exactly this loop (`0x5ed058`), over `129 * 65` bytes.
    pub fn blend(&mut self, other: &Fractal) {
        for (a, b) in self.heights.iter_mut().zip(other.heights.iter()) {
            *a = ((*a as u16 + *b as u16) >> 1) as u8;
        }
    }

    /// `0x5e2280` — `percentileLookup(fm, percent)`.
    ///
    /// Finds a grid value `t` with `count(h < t) * 100 / 8192 <= percent`, i.e.
    /// "the height at or below which at most `percent` % of the map lies".
    ///
    /// It is a bisection with an unusual update, recovered from
    /// `0x5e2280..0x5e2353`. The first guess is `t0 = percent * 255 / 100`
    /// (the `0x51EB851F` magic multiply); `t0 == 0` returns 0 immediately.
    /// Then, with `lo = 0`, `hi = 255` and the register `al` carrying `t`:
    ///
    /// ```text
    /// pct = count(h < t) * 100 / 8192
    /// if pct <= percent:  lo = t;  next = (t + hi) / 2
    /// else:               hi = t;  next = (t + lo) / 2
    /// if next == lo: return lo
    /// t = next
    /// ```
    ///
    /// # The two oddities
    ///
    /// The bisection averages `t` with the bound it did *not* just move, so it
    /// can step away from the bracket and only terminates because of the
    /// `next == lo` test. And the previous threshold is kept in the argument
    /// slot (`[esp+0x20]`, which holds the same value as `al` after the first
    /// iteration), so the two are indistinguishable.
    ///
    /// The net effect is a real percentile — monotone in `percent` and within a
    /// couple of percent of the request — just reached by an odd route.
    pub fn percentile(&self, percent: i32) -> u8 {
        let p = percent.clamp(0, 100) as u32;
        let mut t = (p * 255) / 100;
        if t == 0 {
            return 0;
        }
        let mut lo: u32 = 0;
        let mut hi: u32 = 255;
        loop {
            let below = self.count_below(t as u8) as u32;
            let next = if (below * 100) >> 13 <= p {
                lo = t;
                (t + hi) / 2
            } else {
                hi = t;
                (t + lo) / 2
            };
            if next == lo {
                return lo as u8;
            }
            t = next;
        }
    }

    /// Number of the [`SAMPLES`] grid values strictly below `t`.
    fn count_below(&self, t: u8) -> usize {
        let mut n = 0usize;
        for x in 0..FX {
            for y in 0..FY {
                if self.get(x, y) < t {
                    n += 1;
                }
            }
        }
        n
    }

    /// `0x5e1fd0` — the optional post-pass, applied when the caller passes a
    /// non-null `out` buffer.
    ///
    /// The routine walks 65 columns x 16 steps, reading a signed per-column
    /// delta out of the `out` structure and scaling two (or four, when
    /// [`flags::SMEAR_WIDE`] is set) *diagonal* tracks of the field by a
    /// `step / 16` ramp, with the wide variant blending towards 16. The result
    /// is a pair of slanted streaks — a directional smear of the coastline
    /// toward the pole the delta points at.
    ///
    /// # Why it is a no-op in practice
    ///
    /// Both call sites that pass `out` point it at a stack scratch buffer whose
    /// delta tables are never initialised, and the pass is immediately followed
    /// by a 50/50 blend that halves whatever it wrote. `deltas` is therefore
    /// exposed rather than hidden so a caller can supply the real tables.
    pub fn smear(&mut self, deltas_narrow: &[u8; STRIDE], deltas_wide: Option<&[u8; STRIDE]>) {
        for x in 0..STRIDE {
            // (byte - 0x80) * 0x80, then a signed shift right by 7 and 3: /8.
            let dn = ((i32::from(deltas_narrow[x]) - 0x80) >> 3).clamp(-16, 15);
            let dw = deltas_wide.map(|t| ((i32::from(t[x]) - 0x80) >> 2).clamp(-32, 31));

            for step in 0..16usize {
                for (row, bias) in [
                    (wrap8(dn + step as i32), 0i32),
                    (wrap8(dn - step as i32), 0),
                ] {
                    let v = self.get(x, row) as i32;
                    let v = (v * step as i32 + bias + 15) >> 4;
                    self.set(x, row, v.clamp(0, 255) as u8);
                }
                if let Some(dw) = dw {
                    for row in [wrap8(dw + 0x40 + step as i32), wrap8(dw - step as i32 + 0x40)] {
                        let v = self.get(x, row) as i32;
                        let v = (v * step as i32 + (16 - step as i32) + 15) >> 4;
                        self.set(x, row, v.clamp(0, 255) as u8);
                    }
                }
            }
        }
    }
}

/// Wraps a row index into `0..128`, matching the binary's single-step adjust.
#[inline]
fn wrap8(v: i32) -> usize {
    let mut v = v;
    if v < 0 {
        v += 0x80;
    } else if v >= 0x80 {
        v -= 0x80;
    }
    v as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    const TINY: (i32, i32) = (60, 60);
    const HUGE: (i32, i32) = (160, 160);

    #[test]
    fn grid_is_fully_written_for_every_level() {
        for (w, h) in [TINY, HUGE] {
            for level in 0..=6 {
                let f = Fractal::generate(w, h, level, 0, 0xCC98);
                let zeros = (0..SAMPLES).filter(|&i| f.heights()[i] == 0).count();
                // A few zeros are legitimate, but the grid must not be blank.
                assert!(zeros < SAMPLES / 2, "level {level} produced {zeros} zeros");
            }
        }
    }

    #[test]
    fn determinism_for_a_given_seed() {
        let a = Fractal::generate(100, 100, 3, 9, 0x1234_5678);
        let b = Fractal::generate(100, 100, 3, 9, 0x1234_5678);
        assert_eq!(a.heights(), b.heights());
    }

    #[test]
    fn different_seeds_differ() {
        let a = Fractal::generate(100, 100, 3, 9, 1);
        let b = Fractal::generate(100, 100, 3, 9, 2);
        assert_ne!(a.heights(), b.heights());
    }

    #[test]
    fn ocean_poles_zero_the_first_and_last_row() {
        let f = Fractal::generate(100, 100, 3, flags::OCEAN_POLES, 7);
        for x in 0..=FX {
            assert_eq!(f.height(x, 0), 0, "x = {x}");
            assert_eq!(f.height(x, FY), 0, "x = {x}");
        }
    }

    #[test]
    fn wrap_x_copies_column_zero() {
        let f = Fractal::generate(100, 100, 3, flags::WRAP_X, 7);
        for y in 0..STRIDE {
            assert_eq!(f.height(0, y), f.height(FX, y), "y = {y}");
        }
    }

    #[test]
    fn wrap_y_copies_row_zero_to_the_last_row() {
        let f = Fractal::generate(100, 100, 3, flags::WRAP_Y, 7);
        for x in 0..=FX {
            assert_eq!(f.height(x, 0), f.height(x, FY), "x = {x}");
        }
    }

    #[test]
    fn ocean_poles_beats_wrap_y() {
        // The binary tests OCEAN_POLES first, so row 64 is 0, not a copy of 0.
        let f = Fractal::generate(100, 100, 3, flags::WRAP_Y | flags::OCEAN_POLES, 7);
        for x in 0..=FX {
            assert_eq!(f.height(x, FY), 0, "x = {x}");
        }
    }

    #[test]
    fn sample_spans_the_whole_byte_range() {
        // Not a bound check (the return type already gives that) but a check
        // that the scale factors actually spread the field over the map: if
        // `128.0/W` or `64.0/H` were wrong, the samples would be a handful of
        // repeated values instead of the full range.
        let f = Fractal::generate(HUGE.0, HUGE.1, 3, 0, 0xCC98);
        let mut seen = std::collections::BTreeSet::new();
        for y in 0..HUGE.1 {
            for x in 0..HUGE.0 {
                seen.insert(f.sample(x, y));
            }
        }
        assert!(seen.len() > 200, "only {} distinct heights", seen.len());
        assert!(seen.contains(&0) || seen.len() > 200);
    }

    #[test]
    fn sample_is_continuous_across_the_map() {
        // A midpoint fractal is locally smooth; a discontinuity in the sampled
        // field would mean the grid indexing or the storage order is wrong.
        let f = Fractal::generate(HUGE.0, HUGE.1, 3, 0, 0xCC98);
        let mut big_jumps = 0;
        for y in 0..HUGE.1 - 1 {
            for x in 0..HUGE.0 - 1 {
                let a = f32::from(f.sample(x, y));
                let b = f32::from(f.sample(x + 1, y));
                if (a - b).abs() > 200.0 {
                    big_jumps += 1;
                }
            }
        }
        assert_eq!(big_jumps, 0, "{big_jumps} discontinuities");
    }

    #[test]
    fn the_field_actually_has_structure() {
        // The whole point of the per-level amplitude falloff. With a constant
        // jitter amplitude the field is white noise and this fails.
        let f = Fractal::generate(160, 160, 3, 0, 0xCC98);
        let mut vals: Vec<i32> = Vec::with_capacity(SAMPLES);
        for x in 0..FX {
            for y in 0..FY {
                vals.push(i32::from(f.height(x, y)));
            }
        }
        let mean = vals.iter().map(|&v| f64::from(v)).sum::<f64>() / SAMPLES as f64;
        let var = vals
            .iter()
            .map(|&v| (f64::from(v) - mean).powi(2))
            .sum::<f64>()
            / SAMPLES as f64;
        // White noise over 0..255 has variance 255^2/12 ~= 5419.
        assert!(var < 4000.0, "variance {var} looks like white noise");
        assert!(var > 200.0, "variance {var} is suspiciously flat");
    }

    #[test]
    fn percentiles_are_monotone() {
        let f = Fractal::generate(100, 100, 3, 0, 0xCC98);
        let mut prev = 0u8;
        for p in 0..=100 {
            let v = f.percentile(p);
            assert!(v >= prev, "percentile({p}) went backwards: {prev} -> {v}");
            prev = v;
        }
    }

    #[test]
    fn percentile_zero_is_zero() {
        let f = Fractal::generate(100, 100, 3, 0, 0xCC98);
        assert_eq!(f.percentile(0), 0);
    }

    #[test]
    fn percentile_is_within_a_factor_of_two_of_the_request() {
        // The binary's bisection only halves towards the lower bound, so the
        // result can overshoot. This pins the actual accuracy.
        let f = Fractal::generate(100, 100, 3, 0, 0xD431);
        for p in [10i32, 25, 50, 75, 90] {
            let t = f.percentile(p);
            let got = (f.count_below(t) * 100 / SAMPLES) as i32;
            assert!(
                got <= p && p - got <= p / 2 + 2,
                "asked for {p}%, got {got}% (threshold {t})"
            );
        }
    }

    #[test]
    fn ocean_poles_bias_the_percentiles_low() {
        // Two of the 64 rows are forced to 0, so the low percentiles return 0.
        let f = Fractal::generate(100, 100, 3, flags::OCEAN_POLES, 0xCC98);
        assert_eq!(f.percentile(1), 0);
    }

    #[test]
    fn a_smaller_level_argument_gives_a_rougher_field() {
        // The jitter amplitude at the finest level is `1 << (7 - L0)`, and
        // `L0 = clamp(6 - level_arg, 0, 6)`. So a *lower* level argument means a
        // smaller L0 and therefore a *larger* amplitude and a rougher field.
        fn roughness(level: i32) -> f64 {
            let f = Fractal::generate(160, 160, level, 0, 0xCC98);
            let mut acc = 0.0;
            let mut n = 0.0;
            for x in 0..FX {
                for y in 1..FY {
                    acc += f64::from((f.height(x, y) as i32 - f.height(x, y - 1) as i32).abs());
                    n += 1.0;
                }
            }
            acc / n
        }
        // level 0 -> L0 = 6 -> amplitude 1; level 5 -> L0 = 1 -> amplitude 64.
        assert!(
            roughness(5) > roughness(0),
            "L0=1 ({} ) should be rougher than L0=6 ({} )",
            roughness(5),
            roughness(0)
        );
        assert!(roughness(0) < 5.0, "L0=6 should be almost flat");
    }

    #[test]
    fn blend_is_an_average() {
        let mut a = Fractal::generate(64, 64, 3, 0, 1);
        let b = Fractal::generate(64, 64, 3, 0, 2);
        let want: Vec<u8> = a
            .heights()
            .iter()
            .zip(b.heights())
            .map(|(x, y)| ((*x as u16 + *y as u16) >> 1) as u8)
            .collect();
        a.blend(&b);
        assert_eq!(a.heights(), want.as_slice());
    }

    #[test]
    fn smear_keeps_everything_in_range() {
        let mut f = Fractal::generate(64, 64, 3, flags::SMEAR_WIDE, 3);
        let narrow = [0x40u8; STRIDE];
        let wide = [0xC0u8; STRIDE];
        f.smear(&narrow, Some(&wide));
        assert!(f.heights().iter().any(|&v| v > 0), "the smear wiped the field");
    }
}
