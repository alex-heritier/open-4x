//! The "Randomize" logic for the advanced world-setup sliders.
//!
//! Recovered from `Civ3Conquests.exe` `0x5f1f50` (316 bytes) — the handler
//! behind the **Randomize** button in Game Setup, and the first thing
//! `generateMap` calls.
//!
//! The map object keeps six *raw* option slots and six *derived* slots. A raw
//! value of `3` (or `4` for one of them) means "random"; the button replaces it
//! with a fresh draw and clamps the result:
//!
//! ```text
//! raw +0x1C : (== 3 ? rand_int(3) : as-is) -> clamp(.., 0, 4) -> +0x20   Map Size
//! raw +0x04 : (== 3 ? rand_int(3) : as-is) -> clamp(.., 0, 2) -> +0x08
//! raw +0x2C : (== 3 ? rand_int(3) : as-is) -> clamp(.., 0, 2) -> +0x30
//! raw +0x14 : (== 3 ? rand_int(3) : as-is) -> clamp(.., 0, 2) -> +0x18   Landmass
//! raw +0x24 : (== 3 ? rand_int(3) : as-is) -> clamp(.., 0, 2) -> +0x28   Resources
//! raw +0x0C : (== 4 ? rand_int(5)-1 : as-is) -> clamp(.., 0, 3) -> +0x10
//! ```
//!
//! Note the draw is `rand_int(3)`, i.e. `0..=2` — the "random" case can never
//! land back on 3. The first slot (map size) is allowed `0..=4` by the clamp
//! but only `0..=2` by the draw.
//!
//! # Seed
//!
//! ```text
//! 0x5f1f58  mov eax, [esi + 0x1EC]      ; water level
//! 0x5f1f5e  add eax, 0xCC98            ; 52376
//! 0x5f1f67  call rand01                ; three warm-up draws
//! 0x5f1f72  call rand01
//! 0x5f1f7d  call rand01
//! ```
//!
//! The seed is `0xCC98 + water_level`, warmed up three times.

use crate::rng::Rng;

/// Seed constant for the option randomiser: `0xCC98`.
pub const OPTION_SEED_BASE: u32 = 0xCC98;
/// Number of discarded warm-up draws before the first real one.
pub const OPTION_WARMUP: usize = 3;

/// Which UI slider a raw option slot corresponds to.
///
/// The field offsets and ranges are certain; the labels are inferred from how
/// each value is consumed (see the crate docs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slider {
    /// `+0x1C` -> `+0x20`, range `0..=4`. Drives the world size and the
    /// elevation percentiles used for start-site scoring.
    MapSize,
    /// `+0x14` -> `+0x18`, range `0..=2`. Selects the continent-size balance
    /// target that the landmass retry loop hunts for.
    Landmass,
    /// `+0x24` -> `+0x28`, range `0..=2`. `None / Normal / Plentiful`.
    Resources,
    /// `+0x04` -> `+0x08`, range `0..=2`.
    Climate,
    /// `+0x2C` -> `+0x30`, range `0..=2`.
    Temperature,
    /// `+0x0C` -> `+0x10`, range `0..=3`. Uses `4` (not `3`) as the sentinel.
    Oceans,
}

impl Slider {
    /// The raw slot offset in the map object.
    pub const fn raw_offset(self) -> usize {
        match self {
            Slider::Climate => 0x04,
            Slider::Oceans => 0x0C,
            Slider::Landmass => 0x14,
            Slider::MapSize => 0x1C,
            Slider::Resources => 0x24,
            Slider::Temperature => 0x2C,
        }
    }

    /// The derived slot offset in the map object.
    pub const fn derived_offset(self) -> usize {
        self.raw_offset() + 4
    }

    /// Inclusive upper bound of the derived value.
    pub const fn max(self) -> i32 {
        match self {
            Slider::MapSize => 4,
            Slider::Oceans => 3,
            _ => 2,
        }
    }

    /// The value that means "random".
    pub const fn sentinel(self) -> i32 {
        match self {
            Slider::Oceans => 4,
            _ => 3,
        }
    }

    /// Number of outcomes the randomiser draws from.
    const fn draw_count(self) -> u32 {
        3
    }
}

/// The six resolved world-setup values, plus the water level.
///
/// The water level is *not* one of the six sliders: it lives at `map + 0x1EC`
/// and is set by the caller before `generateMap` runs. It is the generator's
/// only external entropy source, so it is carried here for convenience.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    /// `+0x1EC` — "Oceans", 0..100. Seeds every RNG in the generator.
    pub water_level: i32,
    /// `+0x08` — see [`Slider::Climate`].
    pub climate: i32,
    /// `+0x10` — see [`Slider::Oceans`].
    pub oceans: i32,
    /// `+0x18` — see [`Slider::Landmass`].
    pub landmass: i32,
    /// `+0x20` — see [`Slider::MapSize`].
    pub map_size: i32,
    /// `+0x28` — see [`Slider::Resources`].
    pub resources: i32,
    /// `+0x30` — see [`Slider::Temperature`].
    pub temperature: i32,
}

impl Default for Options {
    /// The Game Setup defaults: tiny map, plenty of continents, normal
    /// resources.
    fn default() -> Self {
        Options {
            water_level: 50,
            climate: 1,
            oceans: 1,
            landmass: 0,
            map_size: 0,
            resources: 1,
            temperature: 1,
        }
    }
}

impl Options {
    /// Raw option values, keyed by slider.
    pub fn raw(slider: Slider) -> i32 {
        // Defaults before the button is pressed; the values themselves are not
        // recoverable, only their ranges and the sentinel.
        slider.sentinel()
    }

    /// `0x5f1f50` — re-roll every slider that is set to "random".
    ///
    /// `water_level` seeds the generator exactly as in the binary.
    pub fn randomize(water_level: i32) -> Self {
        let mut rng = Rng::new((water_level as u32).wrapping_add(OPTION_SEED_BASE));
        rng.discard(OPTION_WARMUP);

        let mut o = Options::default();
        let sliders = [
            Slider::MapSize,
            Slider::Climate,
            Slider::Temperature,
            Slider::Landmass,
            Slider::Resources,
            Slider::Oceans,
        ];
        for s in sliders {
            let raw = if s == Slider::Oceans {
                // rand_int(5) - 1 for the sentinel-4 slot.
                rng.below(5) - 1
            } else {
                rng.below(s.draw_count())
            };
            let v = raw.clamp(0, s.max());
            match s {
                Slider::MapSize => o.map_size = v,
                Slider::Climate => o.climate = v,
                Slider::Temperature => o.temperature = v,
                Slider::Landmass => o.landmass = v,
                Slider::Resources => o.resources = v,
                Slider::Oceans => o.oceans = v,
            }
        }
        o
    }
}

/// The five preset map sizes, in tiles.
///
/// The binary reads these from the BIQ "World Sizes" section at runtime
/// (record stride `0x1C`, height at `+0x44`, width at `+0x50`, both clamped
/// to `16..=362` and forced even), so the numbers are the stock Conquests
/// values rather than literals in the executable.
pub const MAP_SIZES: [(i32, i32); 5] = [
    (60, 60),  // Tiny
    (80, 80),  // Small
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
    fn randomize_is_deterministic_for_a_water_level() {
        assert_eq!(Options::randomize(37), Options::randomize(37));
    }

    #[test]
    fn randomize_respects_every_range() {
        for water in 0..=100 {
            let o = Options::randomize(water);
            assert!((0..=4).contains(&o.map_size), "map_size {}", o.map_size);
            assert!((0..=2).contains(&o.climate));
            assert!((0..=3).contains(&o.oceans));
            assert!((0..=2).contains(&o.landmass));
            assert!((0..=2).contains(&o.resources));
            assert!((0..=2).contains(&o.temperature));
        }
    }

    #[test]
    fn randomize_varies_with_the_water_level() {
        let a = Options::randomize(0);
        let b = Options::randomize(1);
        assert_ne!(a, b, "the seed does not depend on the water level");
    }

    #[test]
    fn randomize_covers_the_range() {
        let mut seen = [0usize; 3];
        for water in 0..=100 {
            seen[Options::randomize(water).landmass as usize] += 1;
        }
        for (v, c) in seen.iter().enumerate() {
            assert!(*c > 10, "landmass {v} only came up {c} times in 101 rolls");
        }
    }

    #[test]
    fn slider_offsets_match_the_binary() {
        assert_eq!(Slider::Climate.raw_offset(), 0x04);
        assert_eq!(Slider::Climate.derived_offset(), 0x08);
        assert_eq!(Slider::Oceans.raw_offset(), 0x0C);
        assert_eq!(Slider::Oceans.derived_offset(), 0x10);
        assert_eq!(Slider::Landmass.raw_offset(), 0x14);
        assert_eq!(Slider::Landmass.derived_offset(), 0x18);
        assert_eq!(Slider::MapSize.raw_offset(), 0x1C);
        assert_eq!(Slider::MapSize.derived_offset(), 0x20);
        assert_eq!(Slider::Resources.raw_offset(), 0x24);
        assert_eq!(Slider::Resources.derived_offset(), 0x28);
        assert_eq!(Slider::Temperature.raw_offset(), 0x2C);
        assert_eq!(Slider::Temperature.derived_offset(), 0x30);
    }

    #[test]
    fn map_sizes_are_the_stock_conquests_presets() {
        assert_eq!(map_size(0), (60, 60));
        assert_eq!(map_size(4), (160, 160));
        assert_eq!(map_size(99), (160, 160), "clamped");
    }
}
