//! AI play: MSVC `rand`/`srand` (exact) + turn/difficulty scaffolding.
//!
//! See `../ai.md`. The `rand()` at `0x64A20E` is verified by disassembly; it is
//! **not** the combat die (that is [`crate::rng::Rng`] on the global instance
//! `0xA526B4`, see `../combat.md`). Everything else here is observed-string
//! inventory with open mechanics.

/// MSVC `rand()` at `0x64A20E` (the `GameRng` name is historical; combat does
/// not use it), paired with `srand` at `0x64A201` (back to back, classic MSVC
/// layout).
///
/// ```asm
/// ; 0x64A201 srand(seed):
/// call 0x64dc93                  ; owner object in eax
/// mov ecx, [esp+4]                ; seed argument
/// mov [eax+0x14], ecx             ; store into owner state
/// ret
/// ; 0x64A20E rand():
/// mov ecx, [eax+0x14]             ; same state word
/// imul ecx, ecx, 0x343fd   ; a = 214013
/// add  ecx, 0x269ec3       ; c = 2531011
/// shr eax, 16 / and 0x7FFF
/// ```
/// Seeded from `timeGetTime()` in `main` (`0x56C1F9`) and at game start
/// (`0x6389DD`: `push eax; call 0x64A201`, then flags `[esi+0xAF8] = 1`).
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

    /// Re-seed live state (`0x64A201`: overwrite the `+0x14` state word).
    ///
    /// Seeded twice per run with different clocks: `timeGetTime()` on the
    /// `0x56C1EA` startup path, `GetTickCount()` at game start (`0x6389DD`,
    /// called from `0x6226A0` via `0x6388E0`, which also zeroes
    /// `[esi+0xAFC]`/`[esi+0xB1C]` and sets start flag `[esi+0xAF8]`).
    pub fn reseed(&mut self, seed: u32) {
        self.state = seed;
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

/// AI score jitter at `0x4D7F16` (verified by r2 disassembly).
///
/// `delta = rand() % 1200 - 600`, i.e. uniform in `[-600, +599]`, pushed to
/// the clamped accumulator `0x5FACC0`. The `and edx, 0xFFFF` in the
/// disassembly is a no-op on the `[0, 1200)` idiv remainder (kept by MSVC),
/// so the model is exactly modulo-then-recenter.
pub fn score_jitter(draw: u32) -> i32 {
    (draw % 1200) as i32 - 600
}

/// Clamped score store (`0x5FACC0` head): saturate to `[-1200, +1200]`
/// (`cmp 0xFFFFFB50` / `cmp 0x4B0`) and write to `[ecx+0x5C]`.
pub fn clamp_score(v: i32) -> i32 {
    v.clamp(-1200, 1200)
}

/// Random-Nth passing candidate (`0x5AF703–0x5AFB1E` scan loops).
///
/// The engine draws `countdown = rand() % mask` once, then walks candidates
/// in table order; each candidate that passes all gates decrements the
/// countdown, and the candidate that drives it below zero is selected.
/// Returns the index of the selected candidate, or `None` when fewer than
/// `countdown + 1` candidates pass. The per-candidate predicate
/// (`0x501B80` and friends) is the caller's `passes` closure.
pub fn random_nth_passing(countdown: u32, passes: &[bool]) -> Option<usize> {
    let mut remaining = countdown;
    for (i, pass) in passes.iter().enumerate() {
        if !pass {
            continue;
        }
        if remaining == 0 {
            return Some(i);
        }
        remaining -= 1;
    }
    None
}

/// Unit-effect descriptor selection (`0x5C7C7B` disease instance).
///
/// When the unit's direct selector byte (`+0x74`, passed as `direct`) is
/// nonzero it is used as-is; otherwise the descriptor comes from the
/// unit's action record (`+0x40` base, 312-byte stride into `[0x9C71E0]`,
/// field at `+8`), passed here as `table_entry`. Either way the chosen
/// descriptor feeds the shared applicator `0x61C5A0`.
pub fn effect_descriptor(direct: u32, table_entry: u32) -> u32 {
    if direct != 0 {
        direct
    } else {
        table_entry
    }
}

/// AI flavor percent-to-fraction (`0x4406E0`): integer percent loaded
/// via `0x585B00`, `fild` to x87, times the 0.01 **double** constant
/// (`[0x666AC8]`), stored to float. Observed: 90 → 0x3F666666
/// (`[0x6849B0]`), 30 → 0x3E99999A (`[0x684A40]`) — only the double
/// intermediate reproduces both bit-exact.
pub fn flavor_fraction(percent: i32) -> f32 {
    (f64::from(percent) * 0.01) as f32
}

/// Hurry gate (`0x4B5290`): city flags word bit 0 set means civil
/// disorder, which rejects hurrying with `HURRY_CIVIL_DISORDER`.
pub fn hurry_blocked_by_disorder(city_flags: u32) -> bool {
    city_flags & 1 == 1
}

/// Riot-sound selector (`0x4BE1CF`/`0x4BE3B2`): `rand() % 3`, decoded by
/// dec/je into three arms; the selected arm feeds `0x535D20` (arg 3/2/1).
pub fn riot_select(draw: u32) -> u32 {
    draw % 3
}

/// Assassin outcome dispatch (`0x5B63DB`, after `call 0x4A53A0`):
/// `sub al,0; je A; dec al; jne C` — three arms on the resolve byte.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AssassinOutcome {
    /// `al == 0` after the no-op sub.
    Arm0,
    /// `al == 1` (dec reaches zero).
    Arm1,
    /// Any other value.
    ArmOther,
}

/// Classify the assassin dispatch byte.
pub fn assassin_branch(al: u8) -> AssassinOutcome {
    match al {
        0 => AssassinOutcome::Arm0,
        1 => AssassinOutcome::Arm1,
        _ => AssassinOutcome::ArmOther,
    }
}

/// Turn-step mode gate (`0x499FC0`, reached via the `0x47B550` trampoline).
///
/// Both step executors (`0x4708B0`, `0x470A60`) require the mode word at
/// `[0x9AFD74]` to be 2 or 4 before doing anything.
pub fn step_mode_open(mode: u32) -> bool {
    mode == 4 || mode == 2
}

/// Committable action kinds for `0x4708B0` step 6.
///
/// `[esi+4]-9` indexes the 76-entry byte table at `0x470A10`; table 0
/// forces `cl = 1` (commit allowed), table 1 leaves `cl = 0`. Out of
/// range (`> 75` after the `-9`) skips the dispatch with `cl = 0`.
pub fn step_kind_committable(kind: u32) -> bool {
    matches!(
        kind.wrapping_sub(9),
        0 | 1 | 5 | 8 | 9 | 10 | 20 | 22 | 23 | 24 | 72 | 73 | 75
    )
}

/// Upkeep record slot (`0x470A60` step 3).
///
/// `+0x28 = [edi+0x20] + slice` where `slice` is the faction turn slice
/// (`[edi+0x211C]`), or `-1` when the global `[0x9905C4]` is `-1`.
pub fn upkeep_record_due(turn_base: i32, slice_or_neg1: i32) -> i32 {
    turn_base.wrapping_add(slice_or_neg1)
}

/// Upkeep queue dedup (`0x470A60` step 4).
///
/// The queue scan sets `bl` when any entry carries kind `0x0C`; the
/// `0x47B490` commit fires only when no such entry is queued (an empty
/// queue commits immediately).
pub fn upkeep_commit_allowed(queued_kinds: &[u32]) -> bool {
    !queued_kinds.contains(&0x0c)
}

/// City-name uniqueness scan (`0x4D9DDE` founding validation).
///
/// The candidate name is byte-compared against every existing city name
/// (table base `[0xA52E6C]`, bound `[0xA52E78]`); any exact match routes to
/// the `BADCITYNAME` dialog instead of the `0x611530` commit.
pub fn city_name_taken(existing: &[&str], candidate: &str) -> bool {
    existing.iter().any(|name| *name == candidate)
}

/// Shuffled 6-of-8 picker (`0x4AEC48-0x4AECAB`).
///
/// Draws `rand() % 8` (MSVC signed-mod idiom; `rand()` is never negative
/// so plain `% 8`) with rejection re-draws until 6 distinct values fill
/// the slots (init `-1`). HYPOTHESIS: randomized neighbor/move-direction
/// order for unit AI; the mechanic is exact either way.
pub fn shuffled6(rng: &mut GameRng) -> [u32; 6] {
    let mut slots = [u32::MAX; 6];
    let mut filled = 0;
    while filled < 6 {
        let v = rng.draw() % 8;
        if !slots[..filled].contains(&v) {
            slots[filled] = v;
            filled += 1;
        }
    }
    slots
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
    fn srand_reseed_restarts_sequence() {
        // 0x64A201 overwrites the same +0x14 word rand() reads.
        let mut rng = GameRng::new(999);
        rng.draw();
        rng.reseed(0);
        assert_eq!(rng.draw(), 38); // same first draw as a fresh seed-0 RNG
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
    fn score_jitter_range_and_recenter() {
        // 0x4D7F16: rand() % 1200, minus 600 -> [-600, +599].
        assert_eq!(score_jitter(0), -600);
        assert_eq!(score_jitter(599), -1);
        assert_eq!(score_jitter(600), 0);
        assert_eq!(score_jitter(1199), 599);
        assert_eq!(score_jitter(1200), -600); // modulo wraps
        // Jitter never escapes the accumulator's clamp window.
        for d in (0..32768).step_by(137) {
            assert!((-600..600).contains(&score_jitter(d)));
        }
    }

    #[test]
    fn score_clamp_saturates() {
        // 0x5FACC0: saturate both ends at +/-1200.
        assert_eq!(clamp_score(-1201), -1200);
        assert_eq!(clamp_score(-1200), -1200);
        assert_eq!(clamp_score(0), 0);
        assert_eq!(clamp_score(1200), 1200);
        assert_eq!(clamp_score(1201), 1200);
    }

    #[test]
    fn random_nth_selection() {
        // Countdown 0 selects the first passing candidate.
        assert_eq!(random_nth_passing(0, &[false, true, true]), Some(1));
        // Countdown 1 skips one passing candidate, fails on gated ones.
        assert_eq!(random_nth_passing(1, &[false, true, false, true]), Some(3));
        // Exhausted table: fewer passers than countdown + 1.
        assert_eq!(random_nth_passing(2, &[true, false, true]), None);
        assert_eq!(random_nth_passing(0, &[false, false]), None);
        assert_eq!(random_nth_passing(0, &[]), None);
    }

    #[test]
    fn effect_descriptor_selection() {
        // Nonzero +0x74 bypasses the action-table lookup.
        assert_eq!(effect_descriptor(0x1234, 0xAAAA), 0x1234);
        assert_eq!(effect_descriptor(0, 0xAAAA), 0xAAAA);
    }

    #[test]
    fn flavor_fractions_match_binary() {
        // Bit-exact: [0x6849B0]=3F666666, [0x684A40]=3E99999A.
        assert_eq!(flavor_fraction(90).to_bits(), 0x3F66_6666);
        assert_eq!(flavor_fraction(30).to_bits(), 0x3E99_999A);
        assert_eq!(flavor_fraction(0).to_bits(), 0x0000_0000);
    }

    #[test]
    fn hurry_and_riot_gates() {
        assert!(hurry_blocked_by_disorder(0x01));
        assert!(hurry_blocked_by_disorder(0xFF));
        assert!(!hurry_blocked_by_disorder(0x00));
        assert!(!hurry_blocked_by_disorder(0xFE));
        assert_eq!(riot_select(0), 0);
        assert_eq!(riot_select(1), 1);
        assert_eq!(riot_select(2), 2);
        assert_eq!(riot_select(3), 0);
        assert_eq!(assassin_branch(0), AssassinOutcome::Arm0);
        assert_eq!(assassin_branch(1), AssassinOutcome::Arm1);
        assert_eq!(assassin_branch(7), AssassinOutcome::ArmOther);
    }

    #[test]
    fn step_executor_gates() {
        // 0x499FC0: modes 2 and 4 only.
        assert!(step_mode_open(2));
        assert!(step_mode_open(4));
        assert!(!step_mode_open(0));
        assert!(!step_mode_open(3));
        // 0x470A10 byte table: 13 committable kinds (table 0).
        for k in [9, 10, 14, 17, 18, 19, 29, 31, 32, 33, 81, 82, 84] {
            assert!(step_kind_committable(k), "kind {k}");
        }
        for k in [0, 8, 11, 15, 30, 80, 83, 85, 100] {
            assert!(!step_kind_committable(k), "kind {k}");
        }
        // 0x470A60: upkeep record slot + kind-0x0C queue dedup.
        assert_eq!(upkeep_record_due(100, 6), 106);
        assert_eq!(upkeep_record_due(100, -1), 99);
        assert!(upkeep_commit_allowed(&[]));
        assert!(upkeep_commit_allowed(&[0x0b, 0x11]));
        assert!(!upkeep_commit_allowed(&[0x0b, 0x0c]));
    }

    #[test]
    fn city_name_uniqueness_scan() {
        // 0x4D9DDE: byte-compare the candidate against every existing name.
        let existing = ["Rome", "Veii", "Antium"];
        assert!(city_name_taken(&existing, "Rome"));
        assert!(!city_name_taken(&existing, "Cumae"));
        assert!(!city_name_taken(&[], "Rome"));
    }

    #[test]
    fn upkeep_slices() {
        let t = TurnSlice { slice: 6 };
        assert!(t.owes_upkeep(3));
        assert!(!t.owes_upkeep(4));
        assert!(!t.owes_upkeep(0));
    }

    #[test]
    fn shuffled_six_of_eight() {
        // 0x4AEC63: rand()%8 rejection-sampled to 6 distinct slots.
        let mut rng = GameRng::new(0);
        let a = shuffled6(&mut rng);
        // First draw for seed 0 is 38, so slot 0 is 38 % 8.
        assert_eq!(a[0], 38 % 8);
        let mut sorted = a;
        sorted.sort_unstable();
        assert!(sorted.windows(2).all(|w| w[0] != w[1]));
        assert!(a.iter().all(|&v| v < 8));
        // Deterministic: same seed replays the same shuffle.
        let mut rng2 = GameRng::new(0);
        assert_eq!(shuffled6(&mut rng2), a);
    }
}
