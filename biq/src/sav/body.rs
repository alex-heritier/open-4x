//! Fixed-size chunk bodies.

use std::fmt;

/// The `N` bytes of one fixed-size chunk, exactly as stored.
///
/// A save chunk is a `memcpy` of a range of a live game object (see
/// `reverse-engineering/savegame.md`), so the bytes include padding and stale
/// heap contents the game never wrote; compare saves field by field through
/// the accessors, not as whole bodies. Offsets are **body offsets**: the
/// object offset minus the start of the range the chunk covers (the range
/// start is in the docs of each field that names one).
#[derive(Clone, PartialEq, Eq)]
pub struct Body<const N: usize>(pub [u8; N]);

impl<const N: usize> Default for Body<N> {
    fn default() -> Self {
        Body([0; N])
    }
}

impl<const N: usize> fmt::Debug for Body<N> {
    // Whole bodies are kilobytes of noise; keep `{:?}` of a save readable.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Body<{N}>")
    }
}

impl<const N: usize> Body<N> {
    fn array<const K: usize>(&self, off: usize) -> [u8; K] {
        let mut a = [0u8; K];
        a.copy_from_slice(&self.0[off..off + K]);
        a
    }

    /// Byte at `off`. Panics when `off` is out of range, like slice indexing.
    pub fn u8(&self, off: usize) -> u8 {
        self.0[off]
    }

    /// Little-endian `u16` at `off`.
    pub fn u16(&self, off: usize) -> u16 {
        u16::from_le_bytes(self.array(off))
    }

    /// Little-endian `i16` at `off`.
    pub fn i16(&self, off: usize) -> i16 {
        i16::from_le_bytes(self.array(off))
    }

    /// Little-endian `u32` at `off`.
    pub fn u32(&self, off: usize) -> u32 {
        u32::from_le_bytes(self.array(off))
    }

    /// Little-endian `i32` at `off`.
    pub fn i32(&self, off: usize) -> i32 {
        i32::from_le_bytes(self.array(off))
    }

    /// Store a byte at `off`.
    pub fn set_u8(&mut self, off: usize, v: u8) {
        self.0[off] = v;
    }

    /// Store a little-endian `i16` at `off`.
    pub fn set_i16(&mut self, off: usize, v: i16) {
        self.0[off..off + 2].copy_from_slice(&v.to_le_bytes());
    }

    /// Store a little-endian `i32` at `off`.
    pub fn set_i32(&mut self, off: usize, v: i32) {
        self.0[off..off + 4].copy_from_slice(&v.to_le_bytes());
    }

    /// Store a little-endian `u16` at `off`.
    pub fn set_u16(&mut self, off: usize, v: u16) {
        self.0[off..off + 2].copy_from_slice(&v.to_le_bytes());
    }

    /// Store a little-endian `u32` at `off`.
    pub fn set_u32(&mut self, off: usize, v: u32) {
        self.0[off..off + 4].copy_from_slice(&v.to_le_bytes());
    }
}
