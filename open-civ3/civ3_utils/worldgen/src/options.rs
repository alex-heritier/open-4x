//! The `WCHR` world-characteristics options and the random-option roll.
//!
//! The map keeps six selected option slots and six actual ones, in the `.biq`
//! `WCHR` order: ocean coverage, climate, age, landmass, temperature,
//! barbarians. A slot holding its "random" sentinel consumes one draw from a
//! single LCG seeded `seed + 0xCC98` after three discarded draws, so the result
//! depends only on the seed and on which slots are random. Slot ranges:
//!
//! | slot | range | random sentinel | draw |
//! |---|---|---|---|
//! | ocean | `0..=4` | `3` | `rand_int(3)` |
//! | climate | `0..=2` | `3` | `rand_int(3)` |
//! | age | `0..=2` | `3` | `rand_int(3)` |
//! | landmass | `0..=2` | `3` | `rand_int(3)` |
//! | temperature | `0..=2` | `3` | `rand_int(3)` |
//! | barbarians | `-1..=3` | `4` | `rand_int(5) - 1` |

use crate::rng::Rng;

/// Seed offset of the option randomiser: `0xCC98`.
pub const OPTION_SEED_BASE: u32 = 0xCC98;
/// Number of discarded warm-up draws before the first real one.
pub const OPTION_WARMUP: usize = 3;
/// The "random" sentinel of every slot except barbarians.
pub const RANDOM: i32 = 3;
/// The "random" sentinel of the barbarians slot.
pub const RANDOM_BARBARIANS: i32 = 4;

/// The selected (user-chosen) values; [`RANDOM`] and [`RANDOM_BARBARIANS`] mean
/// "let the generator decide".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawOptions {
    /// `0..=2` (arid, normal, wet).
    pub climate: i32,
    /// `-1..=3`.
    pub barbarians: i32,
    /// `0..=2` (archipelago, continents, pangaea).
    pub landmass: i32,
    /// `0..=4`; selects the sea-level percentile row.
    pub ocean: i32,
    /// `0..=2` (cool, temperate, warm).
    pub temperature: i32,
    /// `0..=2` (3, 4, 5 billion years).
    pub age: i32,
}

/// The resolved world setup, plus the map seed and the world-size preset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    /// Seeds every RNG in the generator.
    pub seed: i32,
    /// World-size preset, an index into the `WSIZ` records.
    pub size: i32,
    /// Actual climate.
    pub climate: i32,
    /// Actual barbarian activity.
    pub barbarians: i32,
    /// Actual landmass.
    pub landmass: i32,
    /// Actual ocean coverage.
    pub ocean: i32,
    /// Actual temperature.
    pub temperature: i32,
    /// Actual age.
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

    /// Resolves the slots into the actual values. `size` is carried through
    /// (it is not touched by the randomiser).
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

        // Barbarians: sentinel 4, a 5-way draw shifted down one so "none" (-1)
        // is reachable, clamped to -1..=3.
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

/// The five preset map sizes, in tiles: the stock Conquests `WSIZ` values.
pub const MAP_SIZES: [(i32, i32); 5] = [
    (60, 60),   // Tiny
    (80, 80),   // Small
    (100, 100), // Medium
    (130, 130), // Large
    (160, 160), // Huge
];

/// `(width, height)` for a map-size index, clamping out-of-range values to the
/// largest preset.
pub fn map_size(index: i32) -> (i32, i32) {
    MAP_SIZES[index.clamp(0, 4) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn randomize_respects_every_range_and_is_deterministic() {
        for seed in 0..=200 {
            let o = Options::randomize(seed);
            assert_eq!(o, Options::randomize(seed));
            assert!((0..=4).contains(&o.ocean));
            assert!((0..=2).contains(&o.climate));
            assert!((-1..=3).contains(&o.barbarians));
            assert!((0..=2).contains(&o.landmass));
            assert!((0..=2).contains(&o.temperature));
            assert!((0..=2).contains(&o.age));
        }
    }

    #[test]
    fn fixed_slots_pass_through_their_clamp() {
        let o = RawOptions {
            climate: 9,
            barbarians: -5,
            landmass: -2,
            ocean: 9,
            temperature: 2,
            age: 0,
        }
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
