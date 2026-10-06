//! Civilization III scenario files (`.biq`/`.bic`/`.bix`): reader, writer and
//! the PKWARE DCL codec the game wraps them in.
//!
//! * [`Biq::read_file`] / [`Biq::parse`] accept a DCL-compressed or plain file
//!   and decode every section into typed rows ([`sections`]); nothing is left
//!   unmodelled in any shipped file ([`Biq::unmodelled_bytes`]).
//! * [`Biq::to_bytes`] writes the file back byte for byte, wrapper included
//!   ([`dcl::Storage`]); [`Biq::to_stream`] writes the plain stream.
//! * [`dcl`] decompresses (both literal modes), [`implode`] compresses with a
//!   port of the game's own routine.
//!
//! The format is documented in `reverse-engineering/biq-format.md` and the
//! codec in `reverse-engineering/biq.md`. Tests that read the shipped files use
//! the git-ignored `civ3/` tree (or `CIV3_DIR`) and do nothing without it.
//!
//! Saved games (`.SAV`, magic `CIV3`) are a different stream: [`sav::Save`]
//! reads and writes them byte for byte (`reverse-engineering/savegame.md`).

pub mod corpus;
pub mod dcl;
pub mod file;
pub mod implode;
pub mod io;
pub mod owner;
pub mod raw;
pub mod sav;
pub mod sections;

pub use file::{Biq, MapData, MapView, Rules, Scenario, UnknownSection};
pub use sav::Save;

use std::fmt;

/// The BIQ format version stored in the `VER#` header (`major.minor`).
///
/// The game logs it as `"%d.%02d"` and gates loader behaviour on thresholds
/// of `major + minor * 0.01` (`0x5944F2`...). Compare with the derived
/// ordering, which is lexicographic on `(major, minor)`:
///
/// | files | version |
/// |---|---|
/// | Civ3 1.x `.bic` | 2.05 - 4.01 |
/// | Play the World `.bix` | 11.06 - 11.18 |
/// | Conquests `.biq` | 12.06 - 12.08 |
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Version {
    /// Major version (the loader accepts 2..=12, `0x594530`).
    pub major: u32,
    /// Minor version (two decimal digits).
    pub minor: u32,
}

impl Version {
    /// `Version { major, minor }`.
    pub const fn new(major: u32, minor: u32) -> Self {
        Version { major, minor }
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{:02}", self.major, self.minor)
    }
}

/// Test-only allocation probe: records, per thread, the largest single
/// allocation requested, so the corruption tests can fail on a file-supplied
/// count that sizes a buffer (an allocation bomb) even when the machine has the
/// memory to survive it.
#[cfg(test)]
pub(crate) mod alloc_probe {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    thread_local! {
        static LARGEST: Cell<usize> = const { Cell::new(0) };
    }

    struct Probe;

    unsafe impl GlobalAlloc for Probe {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let _ = LARGEST.try_with(|c| c.set(c.get().max(layout.size())));
            unsafe { System.alloc(layout) }
        }
        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            let _ = LARGEST.try_with(|c| c.set(c.get().max(layout.size())));
            unsafe { System.alloc_zeroed(layout) }
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) }
        }
        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            let _ = LARGEST.try_with(|c| c.set(c.get().max(new_size)));
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }

    #[global_allocator]
    static PROBE: Probe = Probe;

    /// Run `f` and return its result with the largest single allocation it made.
    pub fn largest_during<T>(f: impl FnOnce() -> T) -> (T, usize) {
        LARGEST.with(|c| c.set(0));
        let out = f();
        (out, LARGEST.with(|c| c.get()))
    }
}
