//! AI play: the game RNG (exact) + turn/difficulty scaffolding.
//!
//! See `../ai.md`. The game RNG at `0x64A20E` is verified by disassembly;
//! everything else here is observed-string inventory with open mechanics.

/// The game RNG (`0x64A20E`): MSVC-compatible `rand()`.
///
/// ```asm
/// imul ecx, ecx, 0x343fd   ; a = 214013
/// add  ecx, 0x269ec3       ; c = 2531011
/// shr eax, 16 / and 0x7FFF
/// ```
/// State lives in the owner object at `+0x14`; seeded from `timeGetTime()`
/// in `main` (`0x56C1F9`) and at game start (`0x6389DD`).
/// Map generation never uses it (it uses `0x60BA80`); the streams are
/// independent by construction.
#[derive(Clone, Debug)]
pub struct GameRng {
    state: u32,
}

impl GameRng {
    /// Fresh game RNG with the given seed word.
    pub fn new(seed: u32) -> Self {
        GameRng { state: seed }
    }

    /// One `rand()` draw, range `0..32768`.
    pub fn draw(&mut self) -> u32 {
        self.state = self
            .state
            .wrapping_mul(0x343FD)
            .wrapping_add(0x269EC3);
        (self.state >> 16) & 0x7FFF
    }

    /// `rand() % n`, the common call-site idiom.
    pub fn next_bounded(&mut self, n: u32) -> u32 {
        self.draw() % n
    }
}

/// Difficulty levels keying the AI bonus tables (`Difficulty`, `0x732D41C`).
/// Table values are unmapped; order is the UI order. **HYPOTHESIS.**
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Difficulty {
    /// Easiest level: AI pays full price.
    Chieftain,
    /// Second level.
    Warlord,
    /// Middle level: even costs.
    Regent,
    /// AI production edge begins.
    Monarch,
    /// Large AI bonuses.
    Emperor,
    /// Maximum AI bonuses.
    Deity,
}

/// Unit action gate, stages 1–2 of `0x5C1AD0`.
///
/// Indexed record fetch with back-pointer (`0x5B2820` head).
///
/// `idx` comes from the caller's `+0x1C`, the table base from
/// `[0xA52E84]`, the bound from `[0xA52E90]`; entry stride is 8 bytes with
/// the pointer in the second dword, minus `0x1C` (`container_of`).
/// Null table, out-of-range index, or null entry all yield `None`
/// (the exe returns 0).
pub fn table_backptr(table: &[u32], idx: i32, bound: i32) -> Option<u32> {
    if idx < 0 || idx > bound {
        return None;
    }
    let v = *table.get(idx as usize * 2 + 1)?;
    if v == 0 {
        return None;
    }
    Some(v - 0x1C)
}

/// Cell-array index from map coordinates (`0x4A15D5`, `0x4A1955`).
///
/// `idx = (x >> 1) + (w >> 1) * y`, masked to 16 bits by the callers
/// (`and eax, 0xFFFF`) before the `0x5D16A0` fetch. Both step guards use
/// this convention; whether cells are half-resolution in X or callers pass
/// doubled coordinates is unproven — the formula is exact either way.
pub fn cell_index(x: i32, y: i32, w: i32) -> u32 {
    (((x >> 1) + ((w >> 1) * y)) & 0xFFFF) as u32
}

/// Map wrap (`0x426C00` for X, `0x426C40` for Y).
///
/// Without the wrap flag (`+0x1F0` bit 0 for X, bit 1 for Y) the coordinate
/// passes through; with it, it wraps into `[0, dim)`. The step constructors
/// `0x4A49C0`/`0x4A47C0` route every destination through these.
pub fn wrap_coord(v: i32, dim: i32, wraps: bool) -> i32 {
    if !wraps {
        return v;
    }
    if v < 0 {
        return v + dim;
    }
    if v >= dim {
        return v - dim;
    }
    v
}

/// Stage 3 of the gate: in-bounds test (`0x426BD0`).
///
/// `0 <= x < dims.0 && 0 <= y < dims.1`, where the dims are the object's
/// `+0x168`/`+0x154` fields — the same offsets the `0x4C3210` view check
/// compares against.
pub fn in_bounds(x: i32, y: i32, dims: (i32, i32)) -> bool {
    x >= 0 && x < dims.0 && y >= 0 && y < dims.1
}

/// Unit action gate, stages 1–2 of `0x5C1AD0` plus the stage-3 bounds check.
///
/// A 9999-scale meter must stay positive after the cost (`clamp` at
/// `0x5C1AEF`, `> 0` at `0x5C1AF7`), the capability entry must share a bit
/// with the action inside a 28-bit mask (`and` + `test 0xFFFFFFF` at
/// `0x5C1B1C`), and the unit tile must pass [`in_bounds`].
pub fn action_available(
    meter_remaining: i32,
    capability: u32,
    action: u32,
    x: i32,
    y: i32,
    dims: (i32, i32),
) -> bool {
    meter_remaining > 0
        && (capability & action & 0x0FFF_FFFF) != 0
        && in_bounds(x, y, dims)
}

/// Per-faction turn-slice counter, from the upkeep format string at
/// `0x684C70` (`m_iTurnSlice`, `GetTurnSlicesBetweenFactionUpkeep()`).
#[derive(Clone, Copy, Debug, Default)]
pub struct TurnSlice {
    /// Current faction turn slice (`m_iTurnSlice`).
    pub slice: u32,
}

impl TurnSlice {
    /// True when this faction owes upkeep this slice.
    pub fn owes_upkeep(self, slices_between: u32) -> bool {
        slices_between != 0 && self.slice.is_multiple_of(slices_between)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_rng_sequence_from_disassembly() {
        // seed 0: state = 2531011; 2531011>>16 & 0x7FFF = 38.
        let mut rng = GameRng::new(0);
        assert_eq!(rng.draw(), 38);
    }

    #[test]
    fn game_rng_deterministic_and_bounded() {
        let (mut a, mut b) = (GameRng::new(1234), GameRng::new(1234));
        for _ in 0..100 {
            // Lockstep draws stay equal; bounded draws stay in range.
            let (x, y) = (a.draw(), b.draw());
            assert_eq!(x, y);
            let (xb, yb) = (a.next_bounded(100), b.next_bounded(100));
            assert_eq!(xb, yb);
            assert!(xb < 100);
        }
        // Own state per instance: two instances never share draws.
        let (mut c, mut d) = (GameRng::new(42), GameRng::new(43));
        assert_ne!(c.draw(), d.draw());
    }

    #[test]
    fn action_gate() {
        let dims = (80, 60);
        // Meter gate: spent meter blocks. Capability gate: disjoint bits block.
        assert!(action_available(1, 0x20010000, 0x20010000, 10, 10, dims));
        assert!(!action_available(0, 0x20010000, 0x20010000, 10, 10, dims));
        assert!(!action_available(1, 0x20010000, 0x00020000, 10, 10, dims));
        // Bits above the 28-bit mask do not count (test 0xFFFFFFF).
        assert!(!action_available(1, 0xF0000000, 0xF0000000, 10, 10, dims));
        // Bounds gate (0x426BD0): off-map tile blocks.
        assert!(!action_available(1, 0x20010000, 0x20010000, 80, 10, dims));
        assert!(!action_available(1, 0x20010000, 0x20010000, 10, -1, dims));
        assert!(in_bounds(0, 0, dims));
        assert!(!in_bounds(80, 59, dims));
    }

    #[test]
    fn halved_cell_index() {
        // idx = (x>>1) + (w>>1)*y, masked to 16 bits.
        assert_eq!(cell_index(0, 0, 80), 0);
        assert_eq!(cell_index(3, 0, 80), 1);
        assert_eq!(cell_index(0, 1, 80), 40);
        assert_eq!(cell_index(5, 2, 80), 2 + 80);
    }

    #[test]
    fn wrap_helper() {
        // No wrap flag: passthrough, even off-map.
        assert_eq!(wrap_coord(-1, 80, false), -1);
        assert_eq!(wrap_coord(80, 80, false), 80);
        // Wrap flag: single-step wrap into range.
        assert_eq!(wrap_coord(-1, 80, true), 79);
        assert_eq!(wrap_coord(80, 80, true), 0);
        assert_eq!(wrap_coord(10, 80, true), 10);
    }

    #[test]
    fn backptr_fetch() {
        // Strided table: pointer in second dword of each 8-byte entry.
        let table = [0u32, 0x100, 0u32, 0u32, 0u32, 0x200];
        assert_eq!(table_backptr(&table, 0, 2), Some(0x100 - 0x1C));
        assert_eq!(table_backptr(&table, 1, 2), None); // null entry
        assert_eq!(table_backptr(&table, 2, 2), Some(0x200 - 0x1C));
        assert_eq!(table_backptr(&table, -1, 2), None);
        assert_eq!(table_backptr(&table, 3, 2), None); // past bound
        assert_eq!(table_backptr(&table, 5, 9), None); // past end
    }

    #[test]
    fn upkeep_slices() {
        let t = TurnSlice { slice: 6 };
        assert!(t.owes_upkeep(3));
        assert!(!t.owes_upkeep(4));
        assert!(!t.owes_upkeep(0));
    }
}
