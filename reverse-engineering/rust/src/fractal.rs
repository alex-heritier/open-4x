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
//! # Sampling is bilinear
//!
//! [`Fractal::sample`] does not read the nearest grid byte. It blends the four grid
//! points around `(x * 128/W, y * 64/H)` with the fractional parts as weights and
//! truncates. Treating it as nearest-neighbour gives the same coastline to within a few
//! per cent of the cells and is wrong exactly where it matters: a cell whose height sits
//! next to the coast threshold lands on the other side of it. Against generator-made
//! maps that was the whole of the 3-5% residual (`NOTES.md` section 19).
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
        Self::generate_with(w, h, level_arg, flags, seed, None)
    }

    /// [`Fractal::generate`] with the original's seventh argument, `out`: when a
    /// second fractal is given, [`Fractal::smear_from`] runs on the result
    /// (`0x5e1fb4`). `generateLandmass` passes `fmC` here for landmass styles 1
    /// and 2.
    pub fn generate_with(
        w: i32,
        h: i32,
        level_arg: i32,
        flags: u32,
        seed: u32,
        out: Option<&Fractal>,
    ) -> Self {
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
        if let Some(src) = out {
            f.smear_from(src);
        }
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

    /// `0x5e2180` — samples the field at **map** coordinates, with bilinear
    /// interpolation.
    ///
    /// The first argument is the column (`x`, range `0..W`) and the second the
    /// row (`y`, range `0..H`); the original pushes them in that order, so `x`
    /// lands in `arg1` and picks up the `128.0/W` scale factor.
    ///
    /// The map coordinate is scaled onto the grid (`px = x * 128/W`,
    /// `py = y * 64/H`), split into a cell `(ix, iy)` by truncation and a
    /// fraction `(fx, fy)`, and the four surrounding grid heights are blended:
    ///
    /// ```text
    /// S = h[ix][iy]     * (1-fx) * (1-fy)
    ///   + h[ix+1][iy]   * (1-fy) * fx
    ///   + h[ix][iy+1]   * (1-fx) * fy
    ///   + h[ix+1][iy+1] * fy     * fx
    /// ```
    ///
    /// The result is `trunc(S)` (`_ftol`, `0x64a230`: round toward zero), clamped to
    /// `0..=255`, and scaled by `* 100 >> 8` when [`flags::SCALE_100`] is set.
    ///
    /// The products are formed in the order the FPU code forms them
    /// (`0x5e21e9..0x5e2234`) so the double-precision rounding matches: with the C
    /// runtime's 53-bit precision control the x87 results are exactly IEEE doubles.
    /// That matters at grid-aligned points, where `x * 128/W` is within an ulp of an
    /// integer and `fx` is `1e-16` rather than `0`.
    pub fn sample(&self, x: i32, y: i32) -> u8 {
        let px = f64::from(x) * self.scale_x;
        let ix = px as i64;
        let py = f64::from(y) * self.scale_y;
        let iy = py as i64;
        let fx = px - ix as f64;
        let fy = py - iy as f64;

        // The original reads past the grid for out-of-range input; callers bounds-check
        // first, so clamping only keeps this safe.
        let ix = ix.clamp(0, FX as i64 - 1) as usize;
        let iy = iy.clamp(0, FY as i64 - 1) as usize;
        let h = |dx: usize, dy: usize| f64::from(self.get(ix + dx, iy + dy));

        let one_minus_fy = 1.0 - fy;
        let one_minus_fx = 1.0 - fx;
        let mut s = h(0, 0) * one_minus_fx * one_minus_fy;
        s += h(1, 0) * one_minus_fy * fx;
        s += h(0, 1) * one_minus_fx * fy;
        s += h(1, 1) * fy * fx;

        let v = (s as i64).clamp(0, 255) as u32;
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

    /// `0x5e1fd0` — the post-pass `fractal()` runs when its `out` argument is
    /// non-null (`0x5e1fb4`: `test eax,eax; je; push eax; call 0x5e1fd0`).
    ///
    /// `src` is the *other* fractal the caller passes, `fmC` in
    /// `generateLandmass`. Its columns are the random source of two ocean
    /// channels, so the pass is a **seam carver**, not a no-op:
    ///
    /// * for every row `y` (65 of them), column 96 of `src` (`src + 0x1880 + y`)
    ///   gives a centre `d = trunc((b - 128) / 8)`, in `-16..=15`;
    /// * for `step = 0..16` the columns `d + step` and `d - step` (wrapped into
    ///   `0..128`) are scaled to `h * step / 16`. The centre goes to 0 and the
    ///   height ramps back up over 15 columns on both sides: a V-shaped trough
    ///   near the left/right seam of the world, wandering with `y`;
    /// * with [`flags::SMEAR_WIDE`] (landmass style 1 only), column 32 of `src`
    ///   (`src + 0x840 + y`) gives a second centre `w = trunc((b - 128) / 4)`
    ///   around `x = 64`, and columns `64 + w +- step` become
    ///   `(h * step + 16 - step) / 16`, a second trough through the middle of the
    ///   world. That is what splits the style into 3-5 continents.
    ///
    /// The earlier version of this routine indexed `x` and `y` the wrong way
    /// round, floored the `/8` instead of truncating, and added a spurious `+15`;
    /// `NOTES.md` section 19 records the differential test against the original
    /// machine code that exposed it.
    ///
    /// The routine ends by copying column 0 over column 128 (`0x5e2166`:
    /// `rep movsb` of 65 bytes from `+0x20` to `+0x20a0`, which is `h[128][0]`),
    /// whatever the flags say. Nothing samples column 128, but it keeps the
    /// grid byte-identical to the original's.
    pub fn smear_from(&mut self, src: &Fractal) {
        let wide = self.flags & flags::SMEAR_WIDE != 0;
        for y in 0..STRIDE {
            let d = (i32::from(src.get(96, y)) - 0x80) / 8;
            let w = (i32::from(src.get(32, y)) - 0x80) / 4;
            for step in 0..16i32 {
                for x in [wrap8(d + step), wrap8(d - step)] {
                    let v = i32::from(self.get(x, y));
                    self.set(x, y, ((v * step) >> 4) as u8);
                }
                if wide {
                    for x in [wrap8(w + step + 0x40), wrap8(w - step + 0x40)] {
                        let v = i32::from(self.get(x, y));
                        self.set(x, y, ((v * step + (16 - step)) >> 4) as u8);
                    }
                }
            }
        }
        for y in 0..STRIDE {
            let v = self.get(0, y);
            self.set(FX, y, v);
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

    /// `Fractal` with every grid byte set from `f(x, y)`, over a `w x h` map.
    fn field(w: i32, h: i32, flags: u32, f: impl Fn(usize, usize) -> u8) -> Fractal {
        let mut fm = Fractal::generate(w, h, 3, flags, 1);
        for x in 0..=FX {
            for y in 0..=FY {
                fm.heights[Fractal::idx(x, y)] = f(x, y);
            }
        }
        fm
    }

    #[test]
    fn sample_on_a_grid_point_returns_that_grid_byte() {
        // W = 128, H = 64: the scale factors are exactly 1.0.
        let fm = field(128, 64, 0, |x, y| ((x * 7 + y * 3) % 256) as u8);
        for (x, y) in [(0, 0), (5, 9), (127, 63), (64, 32), (100, 1)] {
            assert_eq!(fm.sample(x, y), fm.height(x as usize, y as usize), "({x}, {y})");
        }
        // W = 64, H = 32: the scale factors are exactly 2.0.
        let fm = field(64, 32, 0, |x, y| ((x * 7 + y * 3) % 256) as u8);
        assert_eq!(fm.sample(10, 7), fm.height(20, 14));
    }

    #[test]
    fn sample_between_grid_points_blends_the_four_neighbours() {
        // W = 256, H = 128: the scale factors are exactly 0.5, so odd map
        // coordinates sit half way between grid points.
        let fm = field(256, 128, 0, |x, y| match (x, y) {
            (0, 0) => 10,
            (1, 0) => 21,
            (0, 1) => 30,
            (1, 1) => 41,
            _ => 0,
        });
        assert_eq!(fm.sample(0, 0), 10);
        assert_eq!(fm.sample(1, 0), 15, "(10 + 21) / 2 = 15.5, truncated");
        assert_eq!(fm.sample(0, 1), 20, "(10 + 30) / 2");
        assert_eq!(fm.sample(1, 1), 25, "(10 + 21 + 30 + 41) / 4 = 25.5, truncated");
    }

    #[test]
    fn scale_100_maps_the_byte_range_onto_zero_to_ninety_nine() {
        let fm = field(128, 64, flags::SCALE_100, |_, _| 255);
        assert_eq!(fm.sample(3, 3), 99);
        let fm = field(128, 64, flags::SCALE_100, |_, _| 128);
        assert_eq!(fm.sample(3, 3), 50);
    }

    /// Answers of the exe's own `sampleHeight` (`0x5e2180`) and `percentileLookup`
    /// (`0x5e2280`) on fractals the exe generated, under the C runtime's x87 control
    /// word. `tests/data/fractal_probe.txt`, regenerated by
    /// `.agents/skills/reverse-engineering-executables/scripts/emu/diff_sample.py`.
    ///
    /// Every sample set includes grid-aligned points, where `x * 128/W` is within an
    /// ulp of an integer and the order of the floating-point operations decides the
    /// answer.
    #[test]
    fn sample_and_percentile_match_the_exes_answers() {
        const CASES: &str = include_str!("../tests/data/fractal_probe.txt");
        let (mut samples, mut percentiles) = (0, 0);
        for line in CASES.lines().filter(|l| !l.trim().is_empty()) {
            let (query, answer) = line.split_once(" = ").expect("fixture line");
            let t: Vec<&str> = query.split_whitespace().collect();
            let n: Vec<i64> = t[..5].iter().map(|v| v.parse().unwrap()).collect();
            let args: Vec<i32> = t[6..].iter().map(|v| v.parse().unwrap()).collect();
            let want: Vec<u8> = answer.split_whitespace().map(|v| v.parse().unwrap()).collect();
            let fm = Fractal::generate(n[0] as i32, n[1] as i32, n[2] as i32, n[3] as u32, n[4] as u32);
            let got: Vec<u8> = match t[5] {
                "S" => args.chunks(2).map(|p| fm.sample(p[0], p[1])).collect(),
                "P" => args.iter().map(|&p| fm.percentile(p)).collect(),
                other => panic!("mode {other}"),
            };
            assert_eq!(got.len(), want.len());
            let wrong: Vec<_> = (0..got.len()).filter(|&i| got[i] != want[i]).take(5).collect();
            assert!(wrong.is_empty(), "{}: {} differ, first at {:?}", &query[..30], wrong.len(), wrong);
            match t[5] {
                "S" => samples += got.len(),
                _ => percentiles += got.len(),
            }
        }
        assert!(samples > 2000 && percentiles > 1000, "{samples} samples, {percentiles} percentiles");
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

    /// A source whose columns 96 and 32 are all `b96` / `b32`.
    fn smear_source(b96: u8, b32: u8) -> Fractal {
        let mut src = Fractal::generate(64, 64, 3, 0, 9);
        for y in 0..STRIDE {
            src.heights[Fractal::idx(96, y)] = b96;
            src.heights[Fractal::idx(32, y)] = b32;
        }
        src
    }

    #[test]
    fn smear_carves_a_v_shaped_trough_at_the_seam() {
        // 0x80 -> centre 0: columns 0 +- step, ramping h * step / 16.
        let src = smear_source(0x80, 0x80);
        let plain = Fractal::generate(64, 64, 3, 0, 5);
        let smeared = Fractal::generate_with(64, 64, 3, 0, 5, Some(&src));
        for y in 0..STRIDE {
            assert_eq!(smeared.get(0, y), 0, "the centre column is zeroed");
            for step in 1..16usize {
                let want = ((plain.get(step, y) as usize * step) >> 4) as u8;
                assert_eq!(smeared.get(step, y), want, "x = {step}, y = {y}");
                let mirrored = 128 - step;
                let want = ((plain.get(mirrored, y) as usize * step) >> 4) as u8;
                assert_eq!(smeared.get(mirrored, y), want, "x = {mirrored}, y = {y}");
            }
            // Outside the trough nothing changes.
            assert_eq!(smeared.get(40, y), plain.get(40, y));
            assert_eq!(smeared.get(100, y), plain.get(100, y));
        }
    }

    #[test]
    fn smear_centre_follows_the_source_byte_and_truncates_toward_zero() {
        // (0x7F - 0x80) / 8 truncates to 0, not -1: the original divides, it does not shift.
        let src = smear_source(0x7F, 0x80);
        let smeared = Fractal::generate_with(64, 64, 3, 0, 5, Some(&src));
        assert!((0..STRIDE).all(|y| smeared.get(0, y) == 0));
        // 0xA0 -> +4: the zero sits at column 4.
        let src = smear_source(0xA0, 0x80);
        let smeared = Fractal::generate_with(64, 64, 3, 0, 5, Some(&src));
        assert!((0..STRIDE).all(|y| smeared.get(4, y) == 0));
    }

    #[test]
    fn the_middle_trough_exists_only_with_the_wide_flag() {
        let src = smear_source(0x80, 0x80);
        let narrow = Fractal::generate_with(64, 64, 3, 0, 5, Some(&src));
        let wide = Fractal::generate_with(64, 64, 3, flags::SMEAR_WIDE, 5, Some(&src));
        let plain = Fractal::generate(64, 64, 3, flags::SMEAR_WIDE, 5);
        // x = 64 is the centre of the second trough: (h * 0 + 16) / 16 = 1.
        assert!((0..STRIDE).all(|y| wide.get(64, y) == 1));
        // Without the flag column 64 is untouched.
        let plain_narrow = Fractal::generate(64, 64, 3, 0, 5);
        assert!((0..STRIDE).all(|y| narrow.get(64, y) == plain_narrow.get(64, y)));
        // The wide pass leaves its far edge close to the unsmeared value.
        assert!((0..STRIDE).all(|y| {
            let want = ((plain.get(79, y) as usize * 15 + 1) >> 4) as u8;
            wide.get(79, y) == want
        }));
    }

    #[test]
    fn no_source_means_no_smear() {
        let a = Fractal::generate(64, 64, 3, flags::SMEAR_WIDE, 3);
        let b = Fractal::generate_with(64, 64, 3, flags::SMEAR_WIDE, 3, None);
        assert_eq!(a.heights(), b.heights());
    }
}
