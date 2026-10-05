//! Multiplayer gates: mode global `[0x9AFD74]`.
//!
//! See `../multiplayer.md`. Both predicates are exact transcriptions of
//! `0x499FC0`/`0x499FE0`; the meaning of the mode values is hypothesis.

/// Game-mode values (meaning: hypothesis — 2 single-player, 4/5 MP roles).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Mode(pub u32);

impl Mode {
    /// `0x47B550`: mode in {2, 4}.
    pub fn gate_a(self) -> bool {
        self.0 == 2 || self.0 == 4
    }

    /// `0x47B530`: mode in {4, 5}. Gates founding, disease, trade notify,
    /// combat resolve — the local-vs-remote split.
    pub fn gate_b(self) -> bool {
        self.0 == 4 || self.0 == 5
    }
}

/// Player-count clamp (`0x48E155`): into `[0x9C74C8]`, range 1..=31.
pub fn player_count_clamp(n: i32) -> i32 {
    n.clamp(1, 31)
}

/// Used-races mask (`0x5A09E7`): or-bit `1 << race` into `[0xA526C4]`.
pub fn race_mask_assign(used: u32, race: u32) -> u32 {
    used | (1 << race)
}

/// `FNetQueue` vtable (`0x6672FC`): dtor, base, pack, readData, measure.
/// Byte-exact dump; pack/read/measure are virtual-only (zero `E8` refs).
pub const FNETQUEUE_VTABLE: [u32; 5] =
    [0x483FE0, 0x4FCAB0, 0x484420, 0x4842E0, 0x484520];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gates_match_disassembly() {
        // Exhaustive over the plausible mode range.
        for m in 0..8 {
            let mode = Mode(m);
            assert_eq!(mode.gate_a(), m == 2 || m == 4, "gate_a({m})");
            assert_eq!(mode.gate_b(), m == 4 || m == 5, "gate_b({m})");
        }
        // Mode 4 is the overlap: passes both gates.
        assert!(Mode(4).gate_a() && Mode(4).gate_b());
    }

    #[test]
    fn race_assignment_math() {
        // 0x48E155: clamp into [1, 31].
        assert_eq!(player_count_clamp(0), 1);
        assert_eq!(player_count_clamp(-5), 1);
        assert_eq!(player_count_clamp(8), 8);
        assert_eq!(player_count_clamp(31), 31);
        assert_eq!(player_count_clamp(32), 31);
        // 0x5A09E7: or-bit into the used-races mask.
        assert_eq!(race_mask_assign(0, 0), 1);
        assert_eq!(race_mask_assign(1, 2), 0b101);
        assert_eq!(race_mask_assign(0b101, 2), 0b101);
    }

    #[test]
    fn fnetqueue_vtable_is_exact() {
        // Struct dump of .rdata 0x6672FC: dtor/base/pack/readData/measure.
        assert_eq!(FNETQUEUE_VTABLE[0], 0x483FE0);
        assert_eq!(FNETQUEUE_VTABLE[1], 0x4FCAB0);
        assert_eq!(FNETQUEUE_VTABLE[2], 0x484420);
        assert_eq!(FNETQUEUE_VTABLE[3], 0x4842E0);
        assert_eq!(FNETQUEUE_VTABLE[4], 0x484520);
    }
}
