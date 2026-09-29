//! Culture, corruption, and commerce split.
//!
//! See `../economy.md`. All three functions mirror parent-verified
//! instruction sequences; values are non-negative game quantities so
//! the `cdq`-and-shift truncating divisions match Rust `/`.

/// Culture level (`0x4B0C60`): count of powers `base^k <= accum`,
/// starting at 1, capped at 6 (`cmp ebx,6`).
pub fn culture_level(accum: u32, base: u32) -> u32 {
    let mut level = 1;
    let mut power = base;
    while accum >= power && level < 6 {
        power = power.wrapping_mul(base);
        level += 1;
    }
    level
}

/// Commerce split (`0x4B0844`/`0x4B0864`): `(net * rate + 5) / 10`
/// via the `0x66666667` magic divider. `rate` is tenths (0–10).
pub fn split_share(net: i32, rate: i32) -> i32 {
    (net.wrapping_mul(rate).wrapping_add(5)) / 10
}

/// Corruption rank prime (`0x4B18D9`): `rank >= R ? 2*rank - R : rank`.
pub fn rank_prime(rank: i32, r: i32) -> i32 {
    if rank >= r {
        rank.wrapping_mul(2).wrapping_sub(r)
    } else {
        rank
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn culture_levels_follow_powers() {
        // Base 10: powers 10/100/1000/...; level counts powers <= accum.
        assert_eq!(culture_level(0, 10), 1);
        assert_eq!(culture_level(9, 10), 1);
        assert_eq!(culture_level(10, 10), 2);
        assert_eq!(culture_level(99, 10), 2);
        assert_eq!(culture_level(100, 10), 3);
        assert_eq!(culture_level(10_000_000, 10), 6); // capped
        assert_eq!(culture_level(u32::MAX, 10), 6);
    }

    #[test]
    fn split_rounds_like_magic_divider() {
        assert_eq!(split_share(100, 5), 50);
        assert_eq!(split_share(7, 3), 2); // (21+5)/10
        assert_eq!(split_share(1, 1), 0); // (1+5)/10
        assert_eq!(split_share(100, 0), 0);
        assert_eq!(split_share(100, 10), 100);
    }

    #[test]
    fn rank_prime_folds_at_r() {
        assert_eq!(rank_prime(0, 8), 0);
        assert_eq!(rank_prime(7, 8), 7);
        assert_eq!(rank_prime(8, 8), 8);
        assert_eq!(rank_prime(9, 8), 10);
        assert_eq!(rank_prime(12, 8), 16);
    }
}
