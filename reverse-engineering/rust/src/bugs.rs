//! Opt-in reproduction of four original bugs in the Civ3C map generator.
//!
//! Every stage of the shipped binary contains a few places where the code does
//! not do what a reading of its own comments suggests it should. Reproducing
//! them faithfully is the point of a reference implementation, but they are also
//! actively confusing, and they are the sort of thing you would want *off* while
//! working out what the generator is doing.
//!
//! So each one is a separate flag and all four default to **off**, meaning this
//! crate implements what the code appears to have intended. Turning a flag on
//! restores the binary's actual behaviour.
//!
//! ```
//! use civ3mapgen::{generate_with, options::Options, OriginalBugs};
//!
//! // Corrected (the default).
//! let fixed = generate_with(&Options::default(), &OriginalBugs::NONE);
//!
//! // Faithful: the shipped binary, bugs and all.
//! let faithful = generate_with(&Options::default(), &OriginalBugs::ALL);
//!
//! // Or one at a time.
//! let one = generate_with(
//!     &Options::default(),
//!     &OriginalBugs { sea_level_split: true, ..OriginalBugs::NONE },
//! );
//! # let _ = (fixed, faithful, one);
//! ```
//!
//! # The four
//!
//! | flag | where | what the binary does | default |
//! |---|---|---|---|
//! | [`start_slot_index`](Self::start_slot_index) | `0x5eeb00` pass 1 | writes the class to the **x-offset of the neighbour it found** rather than to the loop index | fixed |
//! | [`sea_level_split`](Self::sea_level_split) | `0x5eceb0` | looks up a sea-level percentile `pLo` and then only uses it in a branch that can never be taken, so the ocean has no shelf | fixed |
//! | [`swapped_wrap_flags`](Self::swapped_wrap_flags) | `0x5ecf5b` | feeds the map's **y**-wrap flag into the fractal's **x**-wrap bit | fixed |
//! | [`contour_equality`](Self::contour_equality) | `0x5f1480` pass 2 | tests `h == p70` instead of `h <= p70` | fixed |
//!
//! # How sure is this?
//!
//! The first three are settled. The disassembly is unambiguous and deterministic,
//! and two of them have the signature of a real mistake: a register holding the
//! loop index is provably loaded and then never read, while a different value is
//! passed to the call instead.
//!
//! [`contour_equality`](Self::contour_equality) is the exception, and it is the
//! one left on the binary's behaviour by default. Both readings of that
//! comparison look wrong — `==` marks 0.0-0.5 % of the land, `<=` would mark
//! 52-61 % — which points to a deliberate choice made against a different
//! threshold than the one it resembles, rather than a slip. See the field docs.
//!
//! # A fifth that is *not* a bug
//!
//! `0x5e1b60` reads its midpoint neighbours with no bounds check, which looks
//! like a buffer underflow. It is not: the height array is `129 * 65` bytes and
//! the generator's loop bounds are chosen so the index always lands in
//! `0..=8384`. The extra row and column exist precisely to make the edge
//! midpoint reads well-defined. An exhaustive scan over every level argument
//! (0..=6) and every flag combination confirms the flat index never leaves the
//! array, so there is nothing to opt into and `Fractal::neighbour` clamps
//! unconditionally.

/// Which original bugs to reproduce.
///
/// Every field defaults to `false`, i.e. the intended behaviour. See the
/// [module docs](self) for what each one does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OriginalBugs {
    /// `0x5eeb00` pass 1 targets the wrong cell.
    ///
    /// The pass walks the ring-1 spiral around each start slot looking for a
    /// non-candidate neighbour, then writes the class with
    ///
    /// ```asm
    /// 0x005eeb7f  mov eax, dword [F+0x1c]   ; the x-offset spiralOffset wrote
    /// 0x005eeb86  push eax
    /// 0x005eeb87  call dword [ecx+0x34]    ; getCell(dx)
    /// ```
    ///
    /// where the loop index was loaded into `F+0x24` and then never read again.
    /// Since the ring-1 offsets are in `-2..=2`, the write lands on one of the
    /// first three cells no matter which slot was being processed — so the pass
    /// is close to a no-op and a handful of coastal cells end up mislabelled.
    ///
    /// **Corrected:** write to the cell the loop was on, which is what makes the
    /// pass do its job of separating the two deep-water classes.
    pub start_slot_index: bool,

    /// `0x5f1480` pass 2 compares heights for equality.
    ///
    /// The pass lays hills along the fractal's 70th-percentile height, and the
    /// comparison is
    ///
    /// ```asm
    /// 0x005f1ae1  jl 0x5f1ae7
    /// 0x005f1ae7  jg 0x5f1afb
    /// 0x005f1aff  push 7
    /// ```
    ///
    /// i.e. "skip unless `h` is *exactly* the threshold", rather than `<=`.
    ///
    /// # This is the one I am least sure is a bug
    ///
    /// `==` marks 0.0-0.5 % of the land, so the pass barely does anything. But
    /// `<=` marks 52-61 %, i.e. it would paint most of the map as hills, which is
    /// not what a contour pass is for. Both readings are bad, which suggests
    /// this is a deliberate choice made against a different threshold than the
    /// one it looks like, rather than a plain mistake.
    ///
    /// I have therefore left the binary's `==` as the **default** and put the
    /// other reading behind this flag. If you are trying to work out what the
    /// stage was for, turning this on is the more informative experiment.
    pub contour_equality: bool,

    /// `0x5eceb0`'s sea-level split is unreachable.
    ///
    /// The stage looks up three thresholds — `p96` (inland lakes), `pHi` (the
    /// coastline) and `pLo` (nominal sea level) — and the water case reads
    ///
    /// ```c
    /// getCell(i)->vfunc(0x128)( h3 > pLo ? 0x0C : 0x0D, -1, -1 );
    /// ```
    ///
    /// But the branch is only reached when `h <= pHi`, and `pHi` is
    /// `percentile(42..82)` while `pLo` is `percentile(27..67)`, so `pHi > pLo`
    /// always. `h3 > pLo` is therefore always false and every water cell gets
    /// class 13. The ocean comes out perfectly uniform, with no shallow shelf.
    ///
    /// **Corrected:** drop the unreachable test and split on `h > pLo` directly,
    /// giving class 12 to the band between the sea level and the coastline.
    pub sea_level_split: bool,

    /// `0x5ecf5b` swaps the fractal's wrap bits.
    ///
    /// ```asm
    /// 0x005ecf5b  and ecx, 2                 ; map->wrapFlags & 2  (bit 1 = wrap Y)
    /// 0x005ecf64  neg ebx / 1bdb sbb / 83e3fa / 83c308 / 83cb01
    /// 0x005ecf7c  or  ebx, 0x10              ; Landmass == 1
    /// ```
    ///
    /// which reduces to `3` (wrap x *and* y) when the map wraps in y, else `9`
    /// (wrap x + ocean poles). Bit 0 of a fractal's flags is its **x** wrap, so a
    /// map that wraps in y is handed a fractal that wraps in both, and a map that
    /// does not wrap in y is handed one that still wraps in x.
    ///
    /// Note the y wrap itself is never tested, so a map that wraps in x but not
    /// y produces a y-wrapping field.
    ///
    /// **Corrected:** test bit 0 for the fractal's x wrap and bit 1 for its y
    /// wrap, independently.
    pub swapped_wrap_flags: bool,
}

impl OriginalBugs {
    /// Reproduce all four, i.e. match the shipped binary.
    pub const ALL: OriginalBugs = OriginalBugs {
        start_slot_index: true,
        contour_equality: true,
        sea_level_split: true,
        swapped_wrap_flags: true,
    };

    /// Reproduce none of them, i.e. implement the intended behaviour throughout.
    pub const NONE: OriginalBugs = OriginalBugs {
        start_slot_index: false,
        contour_equality: false,
        sea_level_split: false,
        swapped_wrap_flags: false,
    };

    /// Every flag's name, in declaration order.
    pub const NAMES: [&'static str; 4] = [
        "start-slot-index",
        "contour-equality",
        "sea-level-split",
        "swapped-wrap-flags",
    ];

    /// How many of the four are enabled. Handy for reporting.
    pub fn count(self) -> u32 {
        u32::from(self.start_slot_index)
            + u32::from(self.contour_equality)
            + u32::from(self.sea_level_split)
            + u32::from(self.swapped_wrap_flags)
    }

    /// Looks up a single flag by name, for the CLI and for tests.
    ///
    /// Returns `None` for an unknown name.
    pub fn get(self, name: &str) -> Option<bool> {
        Some(match name {
            "start-slot-index" => self.start_slot_index,
            "contour-equality" => self.contour_equality,
            "sea-level-split" => self.sea_level_split,
            "swapped-wrap-flags" => self.swapped_wrap_flags,
            _ => return None,
        })
    }
}

impl From<u32> for OriginalBugs {
    /// A bitmask, LSB first, in [`OriginalBugs::NAMES`] order.
    fn from(mask: u32) -> Self {
        let bit = |i: u32| mask & (1 << i) != 0;
        OriginalBugs {
            start_slot_index: bit(0),
            contour_equality: bit(1),
            sea_level_split: bit(2),
            swapped_wrap_flags: bit(3),
        }
    }
}

impl Default for OriginalBugs {
    /// The intended behaviour throughout, i.e. [`OriginalBugs::NONE`].
    ///
    /// Reproducing a bug is opt-in: the interesting question when reading a
    /// reference implementation is what the algorithm *is*, and having to ask
    /// for the broken version is a better default than having to ask for the
    /// fixed one.
    fn default() -> Self {
        OriginalBugs::NONE
    }
}

impl From<OriginalBugs> for u32 {
    fn from(b: OriginalBugs) -> Self {
        let mut mask = 0;
        for (i, on) in [
            b.start_slot_index,
            b.contour_equality,
            b.sea_level_split,
            b.swapped_wrap_flags,
        ]
        .iter()
        .enumerate()
        {
            mask |= (*on as u32) << i;
        }
        mask
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_reproduces_nothing() {
        assert_eq!(OriginalBugs::default(), OriginalBugs::NONE);
    }

    #[test]
    fn all_reproduces_everything() {
        assert_eq!(OriginalBugs::ALL.count(), 4);
        for name in OriginalBugs::NAMES {
            assert_eq!(OriginalBugs::ALL.get(name), Some(true), "{name}");
        }
    }

    #[test]
    fn flags_are_independent() {
        // Each one alone must be settable without dragging the others in.
        for (i, name) in OriginalBugs::NAMES.iter().enumerate() {
            let one = OriginalBugs::from(1 << i);
            assert_eq!(one.count(), 1, "{name} dragged in a neighbour");
            assert_eq!(one.get(name), Some(true));
            assert_eq!(u32::from(one), 1 << i, "{name} is not bit {i}");
        }
    }

    #[test]
    fn every_named_flag_round_trips() {
        for mask in 0u32..16 {
            assert_eq!(u32::from(OriginalBugs::from(mask)), mask);
        }
    }

    #[test]
    fn unknown_names_are_rejected() {
        assert_eq!(OriginalBugs::ALL.get("no-such-bug"), None);
        assert_eq!(OriginalBugs::NONE.get(""), None);
    }
}
