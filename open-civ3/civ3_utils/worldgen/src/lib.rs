//! Map-side primitives recovered from `Civ3Conquests.exe`, re-homed here for
//! the game to use.
//!
//! These are the pieces of the generator that the game actually drives: the
//! `(W/2) x H` cell grid the native algorithms index ([`cell`]), the diamond
//! spiral every neighbourhood scan walks ([`spiral`]), the generator's LCG
//! ([`rng`]), the `WCHR` option roll ([`options`]), and the MSVC `qsort` whose
//! tie-breaking decides continent ids ([`crt`]).
//!
//! The full generator pipeline, the fractal, landmass and biome stages, and
//! the rest of the binary mirror live in the reverse-engineering tree as a
//! reference; the game builds its own world in `map.rs` and only needs these
//! primitives (plus [`rivergen`] and [`continents`], added as they are ported).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod cell;
pub mod continents;
pub mod crt;
pub mod options;
pub mod rivergen;
pub mod rng;
pub mod spiral;
pub mod starts;

pub use cell::{Cell, MapGrid};
pub use continents::{number_continents, Continent};
pub use options::{map_size, Options, RawOptions};
pub use rng::Rng;
