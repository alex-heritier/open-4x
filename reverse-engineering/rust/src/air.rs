//! Air combat: move dispatch (`0x456840`) and the defenses an aircraft meets
//! when it arrives on a tile (`0x5C68A0`).
//!
//! See `../air.md`. The four move sites push their log strings and share
//! one commit tail; the log call itself targets a bare `ret` stub.
//!
//! # Air defense (`Unit::airDefenseAt`, `0x5C68A0`)
//!
//! Every air unit that finishes a move onto `(x, y)` runs three layers, in
//! this order, and stops at the first that fires (callers `0x5B53EA`,
//! `0x5C11D6`, `0x5C1950`, `0x5C73FF`, all behind `PRTO +0x9C == 2`):
//!
//! 1. **SAM** ([`sam_pass`]): an enemy city on the tile whose buildings sum
//!    to a SAM strength `> 0` (`0x4C11E0`).
//! 2. **Flak** ([`flak_shoots`]): up to four enemy units on the tile with a
//!    non-zero PRTO `+0x134` ([`FlakSlots`]). No shipped unit has one.
//! 3. **Interceptors** ([`pick_interceptor`]): enemy air units anywhere on
//!    the map that carry order 15, still have movement and sit within half
//!    their range of the tile ([`within_radius`]). The chosen unit then
//!    fights the aircraft with [`crate::combat::interception_duel`].
//!
//! The chance gates use the two RULE words `[0x9C72A0]` (50) and
//! `[0x9C72A4]` (5, for aircraft with the Stealth ability), see
//! [`InterceptRules`].

use crate::rng::Rng;
use crate::spiral::spiral_offset;

/// `AirBombardMove 1..4` strings in move order (VAs `0x684A8C/78/64/50`).
pub const MOVE_STRINGS: &[&str] = &[
    "AirBombardMove 1",
    "AirBombardMove 2",
    "AirBombardMove 3",
    "AirBombardMove 4",
];

/// `0x5F98B0` is `C3` + 15 `90` (bare ret): the bombard-move log call is
/// a disabled no-op at every one of its 50 call sites.
pub fn log_is_noop(stub: &[u8]) -> bool {
    !stub.is_empty() && stub[0] == 0xC3 && stub[1..].iter().all(|&b| b == 0x90)
}

/// `cmp [PRTO+0x9C], 2` (`0x5C11C4`, `0x5C6E17`): the air domain.
pub const AIR_DOMAIN: i32 = 2;
/// `PRTO::hasAbility(0x15)` (`0x5C692A`): the Stealth ability. It selects
/// the stealth chance word instead of the ordinary one.
pub const ABILITY_STEALTH: u32 = 21;
/// `cmp [unit+0x64], 0xF` (`0x5C6E25`): the order of a patrolling
/// interceptor. Verified as the Interception air mission: the command
/// handler `0x4D93C0` tests token `0x30000004` (PRTO `+0xB4` air-missions
/// bit 2 = Interception), calls `Unit::setOrder(15)` (`0x5B3040`) and then
/// sets `unit +0x50 = max movement`; the AI sets the same order at `0x458104`.
pub const INTERCEPT_ORDER: i32 = 15;
/// The chance die, `push 0x64` (`0x5C6961`, `0x5C70BD`).
pub const CHANCE_DIE: u32 = 100;
/// Flak threshold factor, `lea esi,[eax+eax*4]` (`0x5C6D68`).
pub const FLAK_FACTOR: i32 = 5;
/// The flak candidate list holds four units (`cmp edi,4`, `0x5C6C91`).
pub const FLAK_SLOTS: usize = 4;

/// The RULE words the air-defense code reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InterceptRules {
    /// `[0x9C72A0]` (RULE body `+0xDC`): percent chance that each defender
    /// engages an ordinary aircraft.
    pub air_pct: i32,
    /// `[0x9C72A4]` (RULE body `+0xE0`): the same for a Stealth aircraft.
    pub stealth_pct: i32,
    /// `[0x9C72C8]` (body `+0x100`): movement units per full movement point.
    /// An engaged aircraft is charged exactly one point (3 units); every
    /// air move and bombing run adds the same amount (`0x5C71D4`,
    /// `0x5C7489`).
    pub move_unit: i32,
}

impl InterceptRules {
    /// The values in `conquests.biq` and in every Conquests scenario file
    /// that uses the 720-byte RULE record.
    pub const CONQUESTS: InterceptRules = InterceptRules { air_pct: 50, stealth_pct: 5, move_unit: 3 };

    /// `0x5C6932..0x5C6961` and `0x5C70AA..0x5C70F7`: the stealth word if
    /// the unit (or the units loaded into an Army, `0x5BC6D0`) has ability 21.
    pub fn chance(&self, stealth: bool) -> i32 {
        if stealth {
            self.stealth_pct
        } else {
            self.air_pct
        }
    }
}

impl Default for InterceptRules {
    fn default() -> Self {
        InterceptRules::CONQUESTS
    }
}

/// One building as `0x4C11E0` / `0x4C1280` see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StrengthSource {
    /// `0x4ACB50(city, building, 1)`: the building is in the city or acts on it.
    pub acts_on_city: bool,
    /// `BLDG +0xE0 >= 0` and the owner knows that technology (`0x561440`).
    pub obsolete: bool,
    /// `BLDG +0xC4` (SAM strength, `0x4C11E0`) or `+0xC8` (coastal strength,
    /// `0x4C1280`).
    pub strength: i32,
}

/// `0x4C11E0` and `0x4C1280`: the **sum** over every building that acts on
/// the city and is not obsolete. With one SAM Missile Battery (8) the SAM
/// strength is 8; the Coastal Fortress contributes 8 to the coastal sum.
pub fn strength_sum(buildings: &[StrengthSource]) -> i32 {
    buildings
        .iter()
        .filter(|b| b.acts_on_city && !b.obsolete)
        .fold(0i32, |acc, b| acc.wrapping_add(b.strength))
}

/// The percentage gate: one `next(100)`, passing when it is below `chance`
/// (`cmp eax,esi; jge skip`, `0x5C6970`; `jl engage`, `0x5C70CE`). A chance
/// of zero or less never passes but still consumes the draw.
pub fn chance_passes(rng: &mut Rng, chance: i32) -> bool {
    rng.below(CHANCE_DIE) < chance
}

/// How the SAM layer ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SamResult {
    /// No SAM engaged (strength 0, or the chance gate failed): the next
    /// layer runs. No movement is charged.
    NotEngaged,
    /// The SAM fired and missed. The aircraft keeps its life, but the
    /// routine still returns true, so the move is cut short, and the
    /// movement was charged.
    Survived,
    /// The SAM fired and hit: `AIRSAMINTERCEPTED`, the aircraft is killed.
    ShotDown,
}

/// A SAM pass with the movement it cost.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SamPass {
    /// What happened.
    pub result: SamResult,
    /// Added to `unit +0x50` (`0x5C697A..0x5C6987`) before the hit roll.
    pub movement_cost: i32,
}

/// `0x5C68FF..0x5C69AE`. With `sam <= 0` nothing is drawn. Otherwise one
/// `next(100)` against the chance, then `next(defense + sam)`: the aircraft
/// survives when the roll is **below its defense**, so the kill probability
/// is `sam / (defense + sam)`.
pub fn sam_pass(
    rng: &mut Rng,
    sam: i32,
    aircraft_defense: i32,
    stealth: bool,
    rules: &InterceptRules,
) -> SamPass {
    let idle = SamPass { result: SamResult::NotEngaged, movement_cost: 0 };
    if sam <= 0 || !chance_passes(rng, rules.chance(stealth)) {
        return idle;
    }
    let roll = rng.below(aircraft_defense.wrapping_add(sam) as u32);
    let result = if roll < aircraft_defense { SamResult::Survived } else { SamResult::ShotDown };
    SamPass { result, movement_cost: rules.move_unit }
}

/// The flak threshold `T = 5 * defense * (experience level + 1)`
/// (`0x5C6D51..0x5C6D68`).
pub fn flak_threshold(aircraft_defense: i32, level: i32) -> i32 {
    aircraft_defense.wrapping_mul(level.wrapping_add(1)).wrapping_mul(FLAK_FACTOR)
}

/// `0x5C6D6B..0x5C6D8B`: `next(strength + T) >= T` shoots the aircraft down,
/// a probability of `strength / (strength + T)`.
pub fn flak_shoots(rng: &mut Rng, strength: i32, aircraft_defense: i32, level: i32) -> bool {
    let t = flak_threshold(aircraft_defense, level);
    rng.below(strength.wrapping_add(t) as u32) >= t
}

/// The four-slot candidate list `0xCA6044` built at `0x5C6C80..0x5C6CE1`.
///
/// `T` identifies a unit. The list is **not sorted**: a candidate takes the
/// first empty slot; when all four are full it replaces the first slot (in
/// index order) whose strength is strictly lower, or is dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlakSlots<T: Copy> {
    slots: [Option<(T, i32)>; FLAK_SLOTS],
}

impl<T: Copy> Default for FlakSlots<T> {
    fn default() -> Self {
        FlakSlots { slots: [None; FLAK_SLOTS] }
    }
}

impl<T: Copy> FlakSlots<T> {
    /// Offers one hostile unit with `PRTO +0x134 = strength`. Strengths of
    /// zero or less are not candidates (`jle`, `0x5C6C7C`).
    pub fn offer(&mut self, who: T, strength: i32) {
        if strength <= 0 {
            return;
        }
        if let Some(slot) = self.slots.iter_mut().find(|s| s.is_none()) {
            *slot = Some((who, strength));
            return;
        }
        if let Some(slot) = self.slots.iter_mut().find(|s| s.is_some_and(|(_, v)| v < strength)) {
            *slot = Some((who, strength));
        }
    }

    /// The occupied slots in index order, the order in which they shoot.
    pub fn candidates(&self) -> impl Iterator<Item = (T, i32)> + '_ {
        self.slots.iter().flatten().copied()
    }
}

/// The first candidate (in slot order) that shoots, if any. Draws once per
/// candidate until one hits.
pub fn flak_pass<T: Copy>(
    rng: &mut Rng,
    slots: &FlakSlots<T>,
    aircraft_defense: i32,
    level: i32,
) -> Option<T> {
    slots
        .candidates()
        .find(|&(_, strength)| flak_shoots(rng, strength, aircraft_defense, level))
        .map(|(who, _)| who)
}

/// What `0x5C6D9D..0x5C6E70` asks of every unit in the unit table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PatrolFacts {
    /// The unit's owner is at war with the aircraft's (`0x558F70`).
    pub at_war: bool,
    /// `PRTO +0x9C`.
    pub domain: i32,
    /// `unit +0x64`.
    pub order: i32,
    /// `0x5BE470 - unit +0x50`.
    pub moves_left: i32,
}

/// All four tests of the interceptor filter (`0x5C6E02`, `0x5C6E1F`,
/// `0x5C6E29`, `0x5C6E3B..0x5C6E4A`).
pub fn patrol_eligible(f: &PatrolFacts) -> bool {
    f.at_war && f.domain == AIR_DOMAIN && f.order == INTERCEPT_ORDER && f.moves_left > 0
}

/// `0x5C7034..0x5C704D`: half the unit's operational range (`PRTO +0x64`),
/// at least one tile; a unit with no range reaches only its own tile.
pub fn interceptor_radius(range: i32) -> i32 {
    if range == 0 {
        0
    } else {
        (range.wrapping_sub(range >> 31) >> 1).max(1)
    }
}

/// The map facts `0x52C7A0` reads: width `[0x9C74D4]`, height `[0x9C74C0]`
/// and the wrap flags `[0x9C755C]` (bit 0 wraps x, bit 1 wraps y).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapGeometry {
    /// `[0x9C74D4]`.
    pub width: i32,
    /// `[0x9C74C0]`.
    pub height: i32,
    /// `[0x9C755C] & 1`.
    pub wrap_x: bool,
    /// `[0x9C755C] & 2`.
    pub wrap_y: bool,
}

/// `0x52C7A0(unit_x, unit_y, tx, ty, r)`: is `(tx, ty)` among the first
/// `(2r+1)^2` cells of the spiral around the unit (`0x5E6E50`)? That is every
/// cell with `|dx| + |dy| <= 2r`. Each offset is wrapped as the map flags say
/// and then bounds-checked.
pub fn within_radius(map: &MapGeometry, unit: (i32, i32), target: (i32, i32), r: i32) -> bool {
    let side = r.wrapping_mul(2).wrapping_add(1);
    let count = side.wrapping_mul(side);
    (0..count.max(0)).any(|n| {
        let (dx, dy) = spiral_offset(n);
        let mut x = unit.0 + dx;
        if map.wrap_x {
            if x < 0 {
                x += map.width;
            } else if x >= map.width {
                x -= map.width;
            }
        }
        let mut y = unit.1 + dy;
        if map.wrap_y {
            if y < 0 {
                y += map.height;
            } else if y >= map.height {
                y -= map.height;
            }
        }
        (0..map.width).contains(&x) && (0..map.height).contains(&y) && (x, y) == target
    })
}

/// `0x5C6DAD..0x5C70DB`: walks the unit table in id order. Every unit that
/// passes the filter and the range test (`eligible[i]`) gets its own chance
/// roll; the first to pass is the interceptor and the walk stops.
pub fn pick_interceptor(rng: &mut Rng, eligible: &[bool], chance: i32) -> Option<usize> {
    eligible
        .iter()
        .enumerate()
        .filter(|&(_, &ok)| ok)
        .find(|_| chance_passes(rng, chance))
        .map(|(i, _)| i)
}

/// What an arriving aircraft meets, in the form the three layers need.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arrival {
    /// `0x4C11E0` of the enemy city on the tile (0 without one).
    pub sam: i32,
    /// `0x5BE820` of the aircraft.
    pub defense: i32,
    /// `unit +0x44`.
    pub level: i32,
    /// Ability 21 on the unit or on the units in its Army.
    pub stealth: bool,
    /// Flak candidates on the tile, as unit-table ids.
    pub flak: FlakSlots<usize>,
    /// For every unit-table id: passes the patrol filter and the range test.
    pub patrol_eligible: Vec<bool>,
}

/// What stopped the aircraft.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AirDefenseEvent {
    /// Nothing engaged; the routine returns false and the move goes on.
    Clear,
    /// SAM engaged and missed (the move is still cut short).
    SamMissed {
        /// Movement charged.
        cost: i32,
    },
    /// SAM engaged and hit.
    SamKill {
        /// Movement charged before the kill.
        cost: i32,
    },
    /// The flak candidate with this unit-table id shot the aircraft down.
    FlakKill {
        /// Unit-table id of the shooter.
        by: usize,
    },
    /// The patrolling unit with this unit-table id scrambles: the aircraft
    /// is charged `cost`, the interceptor loses all its movement and its
    /// order, and `interception_duel` decides the fight.
    Scramble {
        /// Unit-table id of the interceptor.
        interceptor: usize,
        /// Movement charged to the aircraft.
        cost: i32,
    },
}

impl AirDefenseEvent {
    /// The routine's return value (`al`): true for everything but `Clear`.
    pub fn stops_the_move(&self) -> bool {
        !matches!(self, AirDefenseEvent::Clear)
    }
}

/// The whole of `0x5C68A0` for one arrival.
pub fn air_defense(rng: &mut Rng, a: &Arrival, rules: &InterceptRules) -> AirDefenseEvent {
    let sam = sam_pass(rng, a.sam, a.defense, a.stealth, rules);
    match sam.result {
        SamResult::Survived => return AirDefenseEvent::SamMissed { cost: sam.movement_cost },
        SamResult::ShotDown => return AirDefenseEvent::SamKill { cost: sam.movement_cost },
        SamResult::NotEngaged => {}
    }
    if let Some(by) = flak_pass(rng, &a.flak, a.defense, a.level) {
        return AirDefenseEvent::FlakKill { by };
    }
    match pick_interceptor(rng, &a.patrol_eligible, rules.chance(a.stealth)) {
        Some(interceptor) => AirDefenseEvent::Scramble { interceptor, cost: rules.move_unit },
        None => AirDefenseEvent::Clear,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_moves_in_order() {
        assert_eq!(MOVE_STRINGS.len(), 4);
        assert!(MOVE_STRINGS[0].ends_with('1'));
        assert!(MOVE_STRINGS[3].ends_with('4'));
    }

    #[test]
    fn ret_stub_shape() {
        let mut stub = vec![0x90u8; 16];
        stub[0] = 0xC3;
        assert!(log_is_noop(&stub));
        assert!(!log_is_noop(&[0x90u8; 16]));
        assert!(!log_is_noop(&[]));
    }

    const GEO: MapGeometry = MapGeometry { width: 40, height: 30, wrap_x: false, wrap_y: false };

    fn rules(air: i32, stealth: i32) -> InterceptRules {
        InterceptRules { air_pct: air, stealth_pct: stealth, move_unit: 3 }
    }

    #[test]
    fn conquests_rules_pick_the_word_by_stealth() {
        let r = InterceptRules::CONQUESTS;
        assert_eq!((r.air_pct, r.stealth_pct, r.move_unit), (50, 5, 3));
        assert_eq!(r.chance(false), 50);
        assert_eq!(r.chance(true), 5);
        assert_eq!(InterceptRules::default(), r);
    }

    #[test]
    fn strength_is_a_sum_over_active_buildings() {
        let b = |acts_on_city, obsolete, strength| StrengthSource { acts_on_city, obsolete, strength };
        assert_eq!(strength_sum(&[]), 0);
        assert_eq!(strength_sum(&[b(true, false, 8)]), 8);
        // Obsolete and absent buildings add nothing; two live ones add up.
        let all = [b(true, false, 8), b(true, true, 8), b(false, false, 8), b(true, false, 3)];
        assert_eq!(strength_sum(&all), 11);
    }

    #[test]
    fn a_failed_gate_still_consumes_the_draw() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        assert!(!chance_passes(&mut a, -1));
        b.below(CHANCE_DIE);
        assert_eq!(a, b);
        // A chance of 100 passes on every draw (the die is 0..=99).
        assert!((0..500).all(|s| chance_passes(&mut Rng::new(s), 100)));
        assert!(!(0..500).any(|s| chance_passes(&mut Rng::new(s), 0)));
    }

    #[test]
    fn no_sam_draws_nothing() {
        let mut rng = Rng::new(3);
        let before = rng;
        let p = sam_pass(&mut rng, 0, 2, false, &InterceptRules::CONQUESTS);
        assert_eq!(p, SamPass { result: SamResult::NotEngaged, movement_cost: 0 });
        assert_eq!(rng, before);
        sam_pass(&mut rng, -4, 2, false, &InterceptRules::CONQUESTS);
        assert_eq!(rng, before);
    }

    #[test]
    fn sam_pass_replays_its_two_draws() {
        let rules = InterceptRules::CONQUESTS;
        for seed in 0..600u32 {
            let mut probe = Rng::new(seed);
            let gate = probe.below(CHANCE_DIE);
            let (want, cost) = if gate >= 50 {
                (SamResult::NotEngaged, 0)
            } else if probe.below(10) < 2 {
                (SamResult::Survived, 3)
            } else {
                (SamResult::ShotDown, 3)
            };
            let mut rng = Rng::new(seed);
            let got = sam_pass(&mut rng, 8, 2, false, &rules);
            assert_eq!((got.result, got.movement_cost), (want, cost), "seed {seed}");
            assert_eq!(rng, probe, "draw count, seed {seed}");
        }
    }

    #[test]
    fn sam_odds_over_a_long_run() {
        let rules = InterceptRules::CONQUESTS;
        let mut rng = Rng::new(12345);
        let (mut engaged, mut kills) = (0u32, 0u32);
        let trials = 40_000u32;
        for _ in 0..trials {
            match sam_pass(&mut rng, 8, 2, false, &rules).result {
                SamResult::NotEngaged => {}
                SamResult::Survived => engaged += 1,
                SamResult::ShotDown => {
                    engaged += 1;
                    kills += 1;
                }
            }
        }
        // 50% engage; 8 / (2 + 8) of those die.
        let engaged_rate = f64::from(engaged) / f64::from(trials);
        let kill_rate = f64::from(kills) / f64::from(engaged);
        assert!((engaged_rate - 0.50).abs() < 0.015, "{engaged_rate}");
        assert!((kill_rate - 0.80).abs() < 0.02, "{kill_rate}");

        // A Stealth aircraft is engaged one time in twenty.
        let mut rng = Rng::new(999);
        let engaged = (0..trials)
            .filter(|_| sam_pass(&mut rng, 8, 2, true, &rules).result != SamResult::NotEngaged)
            .count();
        let rate = engaged as f64 / f64::from(trials);
        assert!((rate - 0.05).abs() < 0.01, "{rate}");
    }

    #[test]
    fn a_defenseless_aircraft_always_dies_to_an_engaged_sam() {
        let rules = rules(100, 100);
        for seed in 0..300 {
            let mut rng = Rng::new(seed);
            assert_eq!(sam_pass(&mut rng, 8, 0, false, &rules).result, SamResult::ShotDown);
        }
    }

    #[test]
    fn flak_threshold_scales_with_defense_and_level() {
        assert_eq!(flak_threshold(2, 0), 10);
        assert_eq!(flak_threshold(2, 3), 40);
        assert_eq!(flak_threshold(0, 5), 0);
        assert_eq!(flak_threshold(6, 1), 60);
    }

    #[test]
    fn flak_shoots_replays_and_has_the_stated_odds() {
        for seed in 0..300u32 {
            let mut probe = Rng::new(seed);
            let want = probe.below(20) >= 10;
            let mut rng = Rng::new(seed);
            assert_eq!(flak_shoots(&mut rng, 10, 2, 0), want, "seed {seed}");
            assert_eq!(rng, probe);
        }
        let mut rng = Rng::new(4242);
        let hits = (0..40_000).filter(|_| flak_shoots(&mut rng, 10, 2, 0)).count();
        let rate = hits as f64 / 40_000.0;
        assert!((rate - 0.5).abs() < 0.015, "{rate}");
        // No defense means no escape.
        let mut rng = Rng::new(1);
        assert!((0..100).all(|_| flak_shoots(&mut rng, 3, 0, 2)));
    }

    #[test]
    fn flak_slots_fill_then_replace_the_first_weaker_slot() {
        let mut s = FlakSlots::<u8>::default();
        s.offer(1, 0);
        s.offer(2, -3);
        assert_eq!(s.candidates().count(), 0);
        for (who, v) in [(10, 5), (11, 9), (12, 3), (13, 7)] {
            s.offer(who, v);
        }
        let ids = |s: &FlakSlots<u8>| s.candidates().map(|(w, _)| w).collect::<Vec<_>>();
        assert_eq!(ids(&s), [10, 11, 12, 13]);

        // 6 beats slot 0 (5), the first weaker slot, although slot 2 (3) is weaker still.
        s.offer(20, 6);
        assert_eq!(ids(&s), [20, 11, 12, 13]);
        // 4 beats only slot 2.
        s.offer(21, 4);
        assert_eq!(ids(&s), [20, 11, 21, 13]);
        // Equal strength does not replace; a weaker one is dropped.
        s.offer(22, 4);
        s.offer(23, 1);
        assert_eq!(ids(&s), [20, 11, 21, 13]);
        assert_eq!(s.candidates().map(|(_, v)| v).collect::<Vec<_>>(), [6, 9, 4, 7]);
    }

    #[test]
    fn flak_pass_stops_at_the_first_shooter() {
        let mut slots = FlakSlots::<usize>::default();
        slots.offer(5, 4);
        slots.offer(6, 60_000);
        slots.offer(7, 9);
        for seed in 0..200u32 {
            let mut probe = Rng::new(seed);
            let mut want = None;
            for (who, v) in slots.candidates() {
                if probe.below((v + 10) as u32) >= 10 {
                    want = Some(who);
                    break;
                }
            }
            let mut rng = Rng::new(seed);
            assert_eq!(flak_pass(&mut rng, &slots, 2, 0), want, "seed {seed}");
            assert_eq!(rng, probe, "seed {seed}");
        }
        let empty = FlakSlots::<usize>::default();
        let mut rng = Rng::new(1);
        let before = rng;
        assert_eq!(flak_pass(&mut rng, &empty, 2, 0), None);
        assert_eq!(rng, before);
    }

    #[test]
    fn the_patrol_filter_needs_all_four_tests() {
        let ok = PatrolFacts { at_war: true, domain: 2, order: 15, moves_left: 1 };
        assert!(patrol_eligible(&ok));
        assert!(!patrol_eligible(&PatrolFacts { at_war: false, ..ok }));
        assert!(!patrol_eligible(&PatrolFacts { domain: 0, ..ok }));
        assert!(!patrol_eligible(&PatrolFacts { domain: 1, ..ok }));
        assert!(!patrol_eligible(&PatrolFacts { order: 1, ..ok }));
        assert!(!patrol_eligible(&PatrolFacts { moves_left: 0, ..ok }));
        assert!(!patrol_eligible(&PatrolFacts { moves_left: -2, ..ok }));
        // More than 9999 units left still counts (`cmp eax,0x270F; jg`).
        assert!(patrol_eligible(&PatrolFacts { moves_left: 20_000, ..ok }));
    }

    #[test]
    fn the_radius_is_half_the_range_but_at_least_one() {
        let r = interceptor_radius;
        assert_eq!([r(0), r(1), r(2), r(3), r(4), r(6), r(9), r(10), r(12), r(16)], [0, 1, 1, 1, 2, 3, 4, 5, 6, 8]);
    }

    #[test]
    fn the_range_test_is_a_diamond_of_doubled_coordinates() {
        let home = (10, 10);
        // Radius 0 is the unit's own tile.
        assert!(within_radius(&GEO, home, home, 0));
        assert!(!within_radius(&GEO, home, (12, 10), 0));
        // Radius 1: |dx| + |dy| <= 2 with an even sum, the eight neighbours.
        for t in [(12, 10), (8, 10), (10, 12), (10, 8), (11, 11), (9, 9), (11, 9), (9, 11)] {
            assert!(within_radius(&GEO, home, t, 1), "{t:?}");
        }
        assert!(!within_radius(&GEO, home, (11, 10), 1), "odd sums are not tiles");
        assert!(!within_radius(&GEO, home, (12, 12), 1));
        assert!(within_radius(&GEO, home, (12, 12), 2));
        assert!(within_radius(&GEO, home, (14, 10), 2));
        assert!(!within_radius(&GEO, home, (16, 10), 2));
        assert!(within_radius(&GEO, home, (16, 10), 3));
    }

    #[test]
    fn the_range_test_wraps_only_when_asked() {
        let edge = (1, 10);
        let across = (39, 10);
        assert!(!within_radius(&GEO, edge, across, 1));
        let wrapped = MapGeometry { wrap_x: true, ..GEO };
        assert!(within_radius(&wrapped, edge, across, 1));
        // The top edge wraps only with the y flag.
        assert!(!within_radius(&wrapped, (5, 0), (5, 28), 1));
        let both = MapGeometry { wrap_y: true, ..wrapped };
        assert!(within_radius(&both, (5, 0), (5, 28), 1));
        // Targets off the map never match.
        assert!(!within_radius(&GEO, (0, 0), (-2, 0), 1));
    }

    #[test]
    fn only_eligible_units_roll_and_the_first_pass_wins() {
        // Nothing eligible: no draws, no result.
        let mut rng = Rng::new(9);
        let before = rng;
        assert_eq!(pick_interceptor(&mut rng, &[false, false], 50), None);
        assert_eq!(rng, before);
        assert_eq!(pick_interceptor(&mut rng, &[], 50), None);
        assert_eq!(rng, before);

        // A certain chance picks the first eligible id after one draw.
        let mut rng = Rng::new(9);
        assert_eq!(pick_interceptor(&mut rng, &[false, true, true], 100), Some(1));
        let mut one = Rng::new(9);
        one.below(CHANCE_DIE);
        assert_eq!(rng, one);

        // A hopeless chance rolls once per eligible unit and finds nobody.
        let mut rng = Rng::new(9);
        assert_eq!(pick_interceptor(&mut rng, &[true, false, true, true], -1), None);
        let mut three = Rng::new(9);
        three.discard(3);
        assert_eq!(rng, three);

        // Replay for a middling chance.
        for seed in 0..200u32 {
            let eligible = [true, false, true, true, false, true];
            let mut probe = Rng::new(seed);
            let want = eligible
                .iter()
                .enumerate()
                .filter(|&(_, &e)| e)
                .find(|_| probe.below(CHANCE_DIE) < 30)
                .map(|(i, _)| i);
            let mut rng = Rng::new(seed);
            assert_eq!(pick_interceptor(&mut rng, &eligible, 30), want, "seed {seed}");
            assert_eq!(rng, probe, "seed {seed}");
        }
    }

    fn arrival() -> Arrival {
        Arrival {
            sam: 0,
            defense: 2,
            level: 0,
            stealth: false,
            flak: FlakSlots::default(),
            patrol_eligible: Vec::new(),
        }
    }

    #[test]
    fn a_quiet_tile_lets_the_aircraft_through_without_a_draw() {
        let mut rng = Rng::new(5);
        let before = rng;
        let ev = air_defense(&mut rng, &arrival(), &InterceptRules::CONQUESTS);
        assert_eq!(ev, AirDefenseEvent::Clear);
        assert!(!ev.stops_the_move());
        assert_eq!(rng, before);
    }

    #[test]
    fn the_layers_fire_in_order() {
        let sure = rules(100, 100);

        // SAM first, even when flak and interceptors are waiting.
        let mut a = arrival();
        a.sam = 8;
        a.defense = 0;
        a.flak.offer(3, 50);
        a.patrol_eligible = vec![true];
        let ev = air_defense(&mut Rng::new(1), &a, &sure);
        assert_eq!(ev, AirDefenseEvent::SamKill { cost: 3 });
        assert!(ev.stops_the_move());

        // A SAM that misses still ends the routine: no flak, no interceptor.
        let mut a = arrival();
        a.sam = 1;
        a.defense = 5_000;
        a.flak.offer(3, 50);
        a.patrol_eligible = vec![true];
        let seed = (0..).find(|&s| {
            let mut p = Rng::new(s);
            p.below(CHANCE_DIE);
            p.below(5_001) < 5_000
        });
        let mut rng = Rng::new(seed.unwrap());
        let ev = air_defense(&mut rng, &a, &sure);
        assert_eq!(ev, AirDefenseEvent::SamMissed { cost: 3 });
        let mut probe = Rng::new(seed.unwrap());
        probe.discard(2);
        assert_eq!(rng, probe, "nothing is drawn after a SAM miss");

        // No SAM: flak next (a defenseless aircraft cannot dodge).
        let mut a = arrival();
        a.defense = 0;
        a.flak.offer(7, 4);
        a.patrol_eligible = vec![true];
        assert_eq!(air_defense(&mut Rng::new(2), &a, &sure), AirDefenseEvent::FlakKill { by: 7 });

        // Neither: the first patrolling unit that passes its roll scrambles.
        let mut a = arrival();
        a.patrol_eligible = vec![false, true, true];
        let ev = air_defense(&mut Rng::new(2), &a, &sure);
        assert_eq!(ev, AirDefenseEvent::Scramble { interceptor: 1, cost: 3 });
        assert!(ev.stops_the_move());

        // A failed SAM gate falls through to the next layer.
        let mut a = arrival();
        a.sam = 8;
        a.patrol_eligible = vec![true];
        let gate_fails = |s: u32| Rng::new(s).below(CHANCE_DIE) >= 50;
        let seed = (0..).find(|&s| {
            // The SAM gate fails and the interceptor roll (second draw) passes.
            let mut p = Rng::new(s);
            gate_fails(s) && {
                p.below(CHANCE_DIE);
                p.below(CHANCE_DIE) < 50
            }
        });
        let ev = air_defense(&mut Rng::new(seed.unwrap()), &a, &InterceptRules::CONQUESTS);
        assert_eq!(ev, AirDefenseEvent::Scramble { interceptor: 0, cost: 3 });
    }

    #[test]
    fn rule_words_are_the_shipped_ones() {
        // RULE body +0xDC / +0xE0 / +0x100 of `conquests.biq`, mem
        // [0x9C72A0] / [0x9C72A4] / [0x9C72C8].
        let r = InterceptRules::CONQUESTS;
        assert_eq!([r.air_pct, r.stealth_pct, r.move_unit], [50, 5, 3]);
    }
}
