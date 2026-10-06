//! The world-characteristics options and `rollRandomOptions`.
//!
//! The map object keeps six *selected* option slots and six *actual* ones, in
//! the same order the `.biq` `WCHR` section stores them (selected climate,
//! actual climate, selected barbarians, ...). Recovered from
//! `Civ3Conquests.exe` `0x5F1F50` (316 bytes), the first thing `generateMap`
//! (`0x5EB580`) runs:
//!
//! | slot | selected | actual | range | clamp / random draw |
//! |---|---|---|---|---|
//! | Ocean coverage | `+0x1C` | `+0x20` | `0..=4` | `3` -> `rand_int(3)` |
//! | Climate | `+0x04` | `+0x08` | `0..=2` | `3` -> `rand_int(3)` |
//! | Age | `+0x2C` | `+0x30` | `0..=2` | `3` -> `rand_int(3)` |
//! | Landmass | `+0x14` | `+0x18` | `0..=2` | `3` -> `rand_int(3)` |
//! | Temperature | `+0x24` | `+0x28` | `0..=2` | `3` -> `rand_int(3)` |
//! | Barbarians | `+0x0C` | `+0x10` | `-1..=3` | `4` -> `rand_int(5) - 1` |
//!
//! The slots are handled in that order, and only a slot holding its "random"
//! value consumes a draw. Every draw comes from one LCG seeded with
//! `seed + 0xCC98` (the map's seed lives at `+0x1EC`) after three discarded
//! draws, so the result depends only on the seed and on which slots are
//! random.
//!
//! The names are the in-game ones, fixed by the `WCHR` field order; the
//! generator itself only ever sees the numbers. `tests/oracle.rs` compares
//! [`RawOptions::resolve`] with the values the exe leaves in the actual slots.

use crate::rng::Rng;

/// Seed offset of the option randomiser: `0xCC98`.
pub const OPTION_SEED_BASE: u32 = 0xCC98;
/// Number of discarded warm-up draws before the first real one.
pub const OPTION_WARMUP: usize = 3;
/// The "random" value of every slot except barbarians.
pub const RANDOM: i32 = 3;
/// The "random" value of the barbarians slot.
pub const RANDOM_BARBARIANS: i32 = 4;

/// The selected (user-chosen) values; [`RANDOM`] and [`RANDOM_BARBARIANS`] mean
/// "let the generator decide".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawOptions {
    /// `+0x04`: `0..=2` (arid, normal, wet).
    pub climate: i32,
    /// `+0x0C`: `-1..=3`.
    pub barbarians: i32,
    /// `+0x14`: `0..=2` (archipelago, continents, pangaea).
    pub landmass: i32,
    /// `+0x1C`: `0..=4`; selects the sea-level percentile row.
    pub ocean: i32,
    /// `+0x24`: `0..=2` (cool, temperate, warm).
    pub temperature: i32,
    /// `+0x2C`: `0..=2` (3, 4, 5 billion years).
    pub age: i32,
}

/// The resolved world setup `generateMap` works from, plus the map seed and the
/// world-size preset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    /// `+0x1EC`: seeds every RNG in the generator. (Older notes call it the
    /// "water level"; it is not a percentage. Saves from the in-game generator
    /// store a full 32-bit value here.)
    pub seed: i32,
    /// `+0x34`: world-size preset, an index into the `WSIZ` records.
    pub size: i32,
    /// `+0x08`: actual climate.
    pub climate: i32,
    /// `+0x10`: actual barbarian activity.
    pub barbarians: i32,
    /// `+0x18`: actual landmass.
    pub landmass: i32,
    /// `+0x20`: actual ocean coverage.
    pub ocean: i32,
    /// `+0x28`: actual temperature.
    pub temperature: i32,
    /// `+0x30`: actual age.
    pub age: i32,
}

impl Default for Options {
    /// A tiny map with every slider on its middle value.
    fn default() -> Self {
        Options {
            seed: 50,
            size: 0,
            climate: 1,
            barbarians: 1,
            landmass: 0,
            ocean: 1,
            temperature: 1,
            age: 1,
        }
    }
}

impl RawOptions {
    /// Every slot random.
    pub const ALL_RANDOM: RawOptions = RawOptions {
        climate: RANDOM,
        barbarians: RANDOM_BARBARIANS,
        landmass: RANDOM,
        ocean: RANDOM,
        temperature: RANDOM,
        age: RANDOM,
    };

    /// `0x5F1F50`: resolve the slots into the actual values.
    ///
    /// `size` is carried through (it is not touched by the randomiser).
    pub fn resolve(&self, seed: i32, size: i32) -> Options {
        let mut rng = Rng::new((seed as u32).wrapping_add(OPTION_SEED_BASE));
        rng.discard(OPTION_WARMUP);

        // Draw (if the slot is random) and clamp, in the exe's order.
        let mut slot = |raw: i32, hi: i32| {
            let v = if raw == RANDOM { rng.below(3) } else { raw };
            v.clamp(0, hi)
        };
        let ocean = slot(self.ocean, 4);
        let climate = slot(self.climate, 2);
        let age = slot(self.age, 2);
        let landmass = slot(self.landmass, 2);
        let temperature = slot(self.temperature, 2);

        // Barbarians: the sentinel is 4, the draw is 5-way and shifted down one
        // so that "none" (-1) is reachable; the lower clamp is -1, the upper 3.
        let barbarians = if self.barbarians == RANDOM_BARBARIANS {
            rng.below(5) - 1
        } else {
            self.barbarians
        }
        .clamp(-1, 3);

        Options {
            seed,
            size,
            climate,
            barbarians,
            landmass,
            ocean,
            temperature,
            age,
        }
    }
}

impl Options {
    /// The options with every slot random, resolved for `seed`.
    pub fn randomize(seed: i32) -> Self {
        RawOptions::ALL_RANDOM.resolve(seed, 0)
    }
}

/// The five preset map sizes, in tiles.
///
/// The binary reads these from the BIQ "World Sizes" section at runtime
/// (record stride `0x54` in the loaded table, height at `+0x44`, width at
/// `+0x50`, both clamped to `16..=362` and forced even), so the numbers are the
/// stock Conquests values rather than literals in the executable.
pub const MAP_SIZES: [(i32, i32); 5] = [
    (60, 60),   // Tiny
    (80, 80),   // Small
    (100, 100), // Medium
    (130, 130), // Large
    (160, 160), // Huge
];

/// `(width, height)` for a map-size index, clamping out-of-range values to
/// the largest preset.
pub fn map_size(index: i32) -> (i32, i32) {
    MAP_SIZES[index.clamp(0, 4) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn randomize_is_deterministic_for_a_seed() {
        assert_eq!(Options::randomize(37), Options::randomize(37));
    }

    #[test]
    fn randomize_respects_every_range() {
        for seed in 0..=200 {
            let o = Options::randomize(seed);
            assert!((0..=2).contains(&o.ocean), "ocean {}", o.ocean);
            assert!((0..=2).contains(&o.climate));
            assert!((-1..=3).contains(&o.barbarians));
            assert!((0..=2).contains(&o.landmass));
            assert!((0..=2).contains(&o.temperature));
            assert!((0..=2).contains(&o.age));
        }
    }

    #[test]
    fn randomize_varies_with_the_seed() {
        assert_ne!(Options::randomize(0), Options::randomize(1));
    }

    #[test]
    fn randomize_covers_the_range() {
        let mut seen = [0usize; 3];
        let mut barb = [0usize; 5];
        for seed in 0..=300 {
            let o = Options::randomize(seed);
            seen[o.landmass as usize] += 1;
            barb[(o.barbarians + 1) as usize] += 1;
        }
        for (v, c) in seen.iter().enumerate() {
            assert!(*c > 30, "landmass {v} only came up {c} times in 301 rolls");
        }
        for (v, c) in barb.iter().enumerate() {
            assert!(*c > 20, "barbarians {} only came up {c} times", v as i32 - 1);
        }
    }

    #[test]
    fn a_fixed_slot_consumes_no_draw() {
        // With only the age random, its value must equal the first draw of a
        // randomiser that has every slot random... for the *first* random slot,
        // which is the ocean slot, so compare against the ocean instead.
        let all = RawOptions::ALL_RANDOM.resolve(77, 0);
        let only_ocean = RawOptions {
            ocean: RANDOM,
            ..RawOptions { climate: 1, barbarians: 1, landmass: 1, ocean: 1, temperature: 1, age: 1 }
        }
        .resolve(77, 0);
        assert_eq!(only_ocean.ocean, all.ocean, "the first draw is the ocean slot's");
        assert_eq!((only_ocean.climate, only_ocean.age, only_ocean.landmass), (1, 1, 1));
    }

    #[test]
    fn fixed_slots_pass_through_their_clamp() {
        let o = RawOptions { climate: 9, barbarians: -5, landmass: -2, ocean: 9, temperature: 2, age: 0 }
            .resolve(1, 3);
        assert_eq!(
            (o.climate, o.barbarians, o.landmass, o.ocean, o.temperature, o.age, o.size),
            (2, -1, 0, 4, 2, 0, 3)
        );
    }

    #[test]
    fn map_sizes_are_the_stock_conquests_presets() {
        assert_eq!(map_size(0), (60, 60));
        assert_eq!(map_size(4), (160, 160));
        assert_eq!(map_size(99), (160, 160), "clamped");
    }
}
