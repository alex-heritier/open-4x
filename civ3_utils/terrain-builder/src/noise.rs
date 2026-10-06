//! Value noise that is periodic on the cell lattice.
//!
//! Evaluated at lattice coordinates `(a, b)` (see `geom`), the noise has
//! period 1 in both, so the same screen position inside any cell, in any
//! sheet, gets the same value. Anything driven by it stays continuous
//! across cell boundaries.

pub struct LatticeNoise {
    seed: u32,
}

fn hash(x: u32, y: u32, f: u32, seed: u32) -> f32 {
    let mut h = x.wrapping_mul(0x8DA6_B343)
        ^ y.wrapping_mul(0xD816_3841)
        ^ f.wrapping_mul(0xCB1A_B31F)
        ^ seed.wrapping_mul(0x1656_67B1);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5BD1_E995);
    h ^= h >> 15;
    (h & 0xFFFF) as f32 / 65535.0
}

impl LatticeNoise {
    pub fn new(seed: u32) -> Self {
        Self { seed }
    }

    /// Smooth value noise on a `freq` x `freq` grid wrapped onto the unit
    /// torus. Range [0, 1].
    pub fn value(&self, a: f32, b: f32, freq: u32) -> f32 {
        let u = a.rem_euclid(1.0) * freq as f32;
        let v = b.rem_euclid(1.0) * freq as f32;
        let (iu, iv) = (u.floor() as u32 % freq, v.floor() as u32 % freq);
        let (fu, fv) = (u.fract(), v.fract());
        let (su, sv) = (fu * fu * (3.0 - 2.0 * fu), fv * fv * (3.0 - 2.0 * fv));
        let n = |du: u32, dv: u32| hash((iu + du) % freq, (iv + dv) % freq, freq, self.seed);
        let top = n(0, 0) + (n(1, 0) - n(0, 0)) * su;
        let bot = n(0, 1) + (n(1, 1) - n(0, 1)) * su;
        top + (bot - top) * sv
    }

    /// Fractal sum of `octaves` layers from `freq` upward. Range ~[0, 1].
    pub fn fbm(&self, a: f32, b: f32, freq: u32, octaves: u32) -> f32 {
        let (mut sum, mut amp, mut norm, mut f) = (0.0, 1.0, 0.0, freq);
        for _ in 0..octaves {
            sum += amp * self.value(a, b, f);
            norm += amp;
            amp *= 0.5;
            f *= 2;
        }
        sum / norm
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn periodic_in_both_lattice_axes() {
        let n = LatticeNoise::new(7);
        for (a, b) in [(0.13, 0.71), (0.5, -0.25), (0.999, 0.001)] {
            let v = n.fbm(a, b, 4, 3);
            assert!((v - n.fbm(a + 1.0, b, 4, 3)).abs() < 1e-4);
            assert!((v - n.fbm(a, b - 2.0, 4, 3)).abs() < 1e-4);
        }
    }
}
