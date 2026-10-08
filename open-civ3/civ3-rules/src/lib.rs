//! The gameplay rules of the Civ3 clone, recovered from `Civ3Conquests.exe`
//! and re-homed here so the game no longer builds against the
//! reverse-engineering reference tree.
//!
//! These are pure functions and small state types: the turn, the city and its
//! numbers, the government, research and diplomacy, combat odds, and so on. The
//! map-side primitives they lean on (the cell grid, the diamond spiral, the
//! generator LCG) come from [`civ3_worldgen`].
//!
//! Ports are done in dependency order; modules appear here as they land.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod capital;
pub mod capture;
pub mod city;
pub mod combat;
pub mod disease;
pub mod diplomacy;
pub mod economy;
pub mod government;
pub mod happiness;
pub mod lakes;
pub mod movement;
pub mod population;
pub mod resistance;
pub mod research;
pub mod research_ai;
pub mod upgrade;
pub mod words;
pub mod yields;
