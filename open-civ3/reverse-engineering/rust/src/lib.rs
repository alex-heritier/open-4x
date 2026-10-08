//! Reference reimplementation of the **Civilization III: Conquests** random map
//! generator, recovered by static analysis of `Civ3Conquests.exe`.
//!
//! This crate reproduces the generator's *algorithm*, not its memory layout.
//! Every constant and control-flow decision is annotated with the address it
//! came from so a claim can be checked against the disassembly.
//!
//! # What the generator actually is
//!
//! Given a set of world-setup options, Civ3's generator is a **pure function**
//! of those options — it never consults the save file, MSVC `rand` (`0x64a20e`),
//! the gameplay `Random` instance (`0xA526B4`, see [`combat`]) or the wall
//! clock, except for a single `timeGetTime()` fallback when a
//! fractal seed is zero. Re-rolling the same options in the Game Setup screen
//! therefore produces the same map every time, which is why players can share
//! "good" settings.
//!
//! The whole thing is seeded from a single slider, the **Oceans** level:
//!
//! ```text
//! seed_base = water_level + <per-stage constant>
//! ```
//!
//! and the per-stage constants are `0xCC98` (options), `0x71 * k` (landmass
//! retries), `0x3039` (blend), `0xD431` (shuffles), `0x7C0`, `29 * continent`,
//! `0x9A2112` (desert conversion) and `0x8ACE` (barbarians).
//!
//! # Pipeline
//!
//! `Map::generate` (`0x5d16f0`) calls `generateMap` (`0x5eb580`), which runs a
//! fixed sequence of stages. See [`pipeline`] for the full list and for which
//! parts of this crate implement which stage.
//!
//! # The cell grid
//!
//! The map is **not** a `W x H` tile grid. Cells are stored in a `(W/2) x H`
//! array indexed by
//!
//! ```text
//! cell = (W >> 1) * y + (x >> 1)
//! ```
//!
//! See [`cell`] — this single expression explains most of what looks strange
//! about the rest of the code, including the "nominal start slot" arithmetic
//! `y = i/(W>>1)`, `x = 2*(i%(W>>1)) + (y&1)`.
//!
//! # The height field
//!
//! Elevation comes from a **midpoint-displacement fractal** over a fixed
//! **129 x 65 byte grid**, generated in a normalised 128 x 64 space and then
//! stretched over the map — which is why a 60x60 and a 160x160 map have
//! structurally identical coastlines. See [`fractal`].
//!
//! Crucially, the generator **never uses a raw height**. Every elevation
//! threshold comes from [`Fractal::percentile`], which binary-searches for the
//! height value `t` such that `P` % of the 8192 samples lie below it. That is
//! what makes all the thresholds adapt automatically to both the random field
//! and the water level.
//!
//! # Original bugs
//!
//! The shipped binary has four places where the code does not do what it appears
//! to intend. They are all **off by default** — this crate implements the
//! intended behaviour — and each can be switched on individually through
//! [`OriginalBugs`]:
//!
//! ```text
//! generate_with(&opts, &OriginalBugs::ALL)   // faithful to the binary
//! generate(&opts)                            // the default, corrected
//! generate_with(&opts, &OriginalBugs { sea_level_split: true, ..NONE })
//! ```
//!
//! See the [`bugs`] module for the evidence behind each one.
//!
//! # Example
//!
//! ```
//! use civ3mapgen::{generate, options::{map_size, Options}, water_percentile};
//!
//! let (w, h) = map_size(3);              // Large
//! let opts = Options { size: 3, ocean: 1, seed: 40, ..Options::default() };
//! let map = generate(&opts);
//!
//! assert_eq!(map.grid.w, w);
//! assert!(map.grid.num_cells() > 0);
//!
//! // Thresholds always land in the byte range regardless of the water level.
//! assert!(water_percentile(40) <= 100);
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod ai;
pub mod air;
pub mod art;
pub mod blend;
pub mod biomes;
pub mod bugs;
pub mod buildable;
pub mod capture;
pub mod cell;
pub mod coast;
pub mod city;
pub mod combat;
pub mod continents;
pub mod crt;
pub mod dcl;
pub mod disease;
pub mod population;
pub mod resistance;
pub mod diplomacy;
pub mod economy;
pub mod fractal;
pub mod government;
pub mod graphics;
pub mod happiness;
pub mod lakes;
pub mod landmass;
pub mod media;
pub mod movement;
pub mod net;
pub mod options;
pub mod oracle;
pub mod pipeline;
pub mod placement;
pub mod starts;
pub mod regions;
pub mod rivergen;
pub mod research;
pub mod research_ai;
pub mod resources;
pub mod rivers;
pub mod rng;
pub mod spiral;
pub mod stack;
pub mod ui;
pub mod upgrade;
pub mod words;
pub mod yields;

pub use bugs::OriginalBugs;
pub use cell::{Cell, MapGrid};
pub use fractal::Fractal;
pub use options::Options;
pub use pipeline::{generate, generate_with, GeneratedMap};
pub use rng::Rng;

/// The water level, as a percentage of the map that should be ocean.
///
/// The "Oceans" slider (0..100) is the generator's only external entropy
/// source, so two runs with the same value and the same options are identical.
pub fn water_percentile(water_level: i32) -> i32 {
    water_level.clamp(0, 100)
}

pub mod capital;
