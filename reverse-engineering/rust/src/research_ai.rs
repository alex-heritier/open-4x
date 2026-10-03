//! The AI's advance valuation: `0x448BF0`, the picks built on it, and the
//! per-advance category mask. Specification: `research-ai.md`.
//!
//! `value(P; t, a, mode)` (vtable `+0x54`, `ret 0xC`) scores one advance for
//! one player. The same routine serves the automatic research pick
//! (`0x449530`, `a = 1`), the human suggestion (`mode = 0`), stealing
//! (`0x44A5B0`), huts and the diplomacy deal scorer (`a = 0, mode = 0`).
//! Every constant is a figure read from the disassembly, with the address in
//! the comment next to it.
//!
//! **The two flags.** `a` is the second argument: it switches the weights
//! (`+6`/`+8` bonuses, the build weights `64`/`128` against `4`), the
//! `256/turnsLeft` term and the whole rival-versus-knower tail. `mode` only
//! gates the random draw. They are easy to swap, which is what an earlier
//! reading of this routine did.

use crate::research::{Brain, Dice, NONE, World, flags};

/// `PRTO.ai_strategies` bits the valuation reads.
pub mod ai {
    /// Offensive unit class.
    pub const OFFENSE: u32 = 1 << 0;
    /// Defensive unit class.
    pub const DEFENSE: u32 = 1 << 1;
    /// Naval transport.
    pub const NAVAL_TRANSPORT: u32 = 1 << 10;
}

/// `BLDG.improvement_flags` bits `0x800 | 0x1000` that earn the building
/// bonus (`test ch, 0x18` at `0x449122`).
pub const BLDG_VALUED_FLAGS: u32 = 0x1800;

/// `BLDG.other_characteristics` bit 2: a great wonder (`0x4490E9`).
pub const BLDG_GREAT_WONDER: u32 = 0x4;

/// One `PRTO` row, reduced to what the valuation reads.
#[derive(Clone, Debug, Default)]
pub struct UnitRow {
    /// `required_tech` (mem `+0x74`).
    pub required_tech: i32,
    /// `available_to_civs` (mem `+0x90`): bit `race` set means the civ may
    /// build it (`0x56AAB0`).
    pub available_to_civs: u32,
    /// `ai_strategies` (mem `+0x8C`).
    pub ai_strategies: u32,
    /// Any of `required_resource_1..3` (mem `+0x7C/+0x80/+0x84`) is not `-1`.
    pub needs_resource: bool,
}

/// One `BLDG` row, reduced to what the valuation reads.
#[derive(Clone, Debug, Default)]
pub struct BldgRow {
    /// `required_advance` (mem `+0xDC`).
    pub required_advance: i32,
    /// `spaceship_part` (mem `+0xD8`) is not `-1`.
    pub spaceship_part: bool,
    /// `improvement_flags` (mem `+0xEC`).
    pub improvement_flags: u32,
    /// `other_characteristics` (mem `+0xF0`).
    pub other_characteristics: u32,
    /// `flavors` mask (mem `+0x100`).
    pub flavors: u32,
}

/// The rule tables the valuation walks, one list per `BIQ` section.
#[derive(Clone, Debug, Default)]
pub struct Tables {
    /// `TFRM.required_tech` of the 13 worker jobs (mem `+0x48`).
    pub tfrm_required: Vec<i32>,
    /// `GOOD.prerequisite` of every resource (mem `+0x4C`).
    pub good_prerequisite: Vec<i32>,
    /// Every `PRTO` row.
    pub units: Vec<UnitRow>,
    /// `GOVT.prerequisite_tech` of every government (mem `+0x1B8`).
    pub govt_prerequisite: Vec<i32>,
    /// Every `BLDG` row.
    pub bldgs: Vec<BldgRow>,
    /// `CTZN.prerequisite` of every citizen type (mem `+0x68`).
    pub ctzn_prerequisite: Vec<i32>,
    /// `FLAV` relationship matrix: `flavors[i][j]` is the percentage.
    pub flavors: Vec<Vec<i32>>,
}

/// The per-civilization inputs: what the player's `RACE` row and its unit
/// census say.
#[derive(Clone, Debug, Default)]
pub struct Profile {
    /// `RACE.flavors` (mem `+0x960`).
    pub flavors: u32,
    /// `RACE.build_often` (mem `+0x954`).
    pub build_often: u32,
    /// `RACE` trait 0, *Militaristic* (`vtable[0](0)`, `0x53A080`).
    pub militaristic: bool,
    /// The race index (`Player.+0x20`), the bit of `PRTO.available_to_civs`.
    pub race: u32,
    /// `int16 Player.+0x10A + int16 Player.+0x14A`: the AI's defender
    /// tallies. Their writers are not decoded; the clone passes its count of
    /// defensive units.
    pub defenders: i32,
    /// `int16 Player.+0x11C`: the AI's naval transport tally (writer not
    /// decoded; the clone passes its transport count).
    pub transports: i32,
}

/// Everything the valuation needs besides the [`World`].
pub struct Valuer<'a> {
    /// The rule tables.
    pub tables: &'a Tables,
    /// One profile per slot.
    pub profiles: &'a [Profile],
    /// `Game+0x510[t]` (`[0xA52B68]`): the category mask of every advance,
    /// see [`category_mask_from_flags`]. May be empty (all zero).
    pub categories: &'a [u32],
    /// Game flag bit 1 of `[0xA5267C]`: the Space Race victory is enabled.
    pub space_race: bool,
    /// `0x538FE0(Game; building)`: this great wonder has been built.
    pub wonder_built: &'a dyn Fn(usize) -> bool,
}

/// `0x52D770(flavors; techMask, raceMask)`: the weighted overlap of two
/// flavor masks.
///
/// Any pair whose relationship is `>= 100` returns `100`; otherwise the
/// mean of the relationships over every (advance flavor, civ flavor) pair;
/// `50` when the table is empty or no pair exists.
pub fn flavor_overlap(table: &[Vec<i32>], tech_mask: u32, race_mask: u32) -> i32 {
    let n = table.len();
    if n == 0 {
        return 50;
    }
    let (mut count, mut sum) = (0i32, 0i32);
    for (i, row) in table.iter().enumerate().take(32) {
        if tech_mask >> i & 1 == 0 {
            continue;
        }
        for j in (0..n.min(32)).filter(|j| race_mask >> j & 1 != 0) {
            let w = row.get(j).copied().unwrap_or(0);
            if w >= 100 {
                return 100;
            }
            count += 1;
            sum += w;
        }
    }
    if count == 0 { 50 } else { sum / count }
}

/// The advance-flag part of `0x443730` (`Game+0x510[t]`), the category bits
/// an advance contributes. The remaining contributions (the 13 worker jobs,
/// the resources, units via `0x443600`, buildings via `0x443300`,
/// governments and citizens) are not ported; see `research-ai.md` section 6.
pub fn category_mask_from_flags(f: u32) -> u32 {
    // (advance flag, category bits), in the order of 0x44375A..0x44387C.
    const MAP: [(u32, u32); 18] = [
        (0x1, 0x3),
        (0x2, 0x80),
        (0x4, 0x2),
        (0x8, 0x80),
        (0x10, 0x2),
        (0x20, 0x101),
        (0x40, 0x100),
        (0x80, 0x40),
        (0x100, 0x2),
        (0x200, 0x2001),
        (0x400, 0x1),
        (0x800, 0x1000),
        (0x1000, 0x800),
        (0x2000, 0x1000),
        (0x4000, 0x1000),
        (0x8000, 0x2000),
        (0x10000, 0x1000),
        (0x40000, 0x1d90),
    ];
    MAP.iter().filter(|(bit, _)| f & bit != 0).fold(0, |m, (_, v)| m | v)
}

/// `tdiv(a, b)`: C division, which Rust's `/` is.
fn tdiv(a: i32, b: i32) -> i32 {
    a / b
}

/// `ftol((0.3f + f) * score)` with the exact single-precision constants and
/// double arithmetic of the x87 sequence at `0x449487..0x4494BA`.
fn flavor_scale(score: i32, matched: i32, tech_flavors: u32) -> i32 {
    let f = if matched >= 100 {
        1.5f32 as f64
    } else if tech_flavors == 0 {
        1.0
    } else {
        f64::from(matched * matched) * f64::from(1e-4f32)
    };
    ((f64::from(0.3f32) + f) * f64::from(score)) as i32
}

impl Valuer<'_> {
    fn profile(&self, p: u32) -> &Profile {
        static EMPTY: Profile = Profile {
            flavors: 0,
            build_often: 0,
            militaristic: false,
            race: 0,
            defenders: 0,
            transports: 0,
        };
        self.profiles.get(p as usize).unwrap_or(&EMPTY)
    }

    /// `0x448BF0 value(P; t, a, mode)`.
    ///
    /// `a` selects the research-pick weights and tail; `mode` enables the
    /// `rand(32) + rand(32)` jitter (it applies only when `a` is set).
    pub fn value(&self, w: &World, p: u32, t: i32, a: bool, mode: bool, dice: &mut dyn Dice) -> i32 {
        let Some(row) = usize::try_from(t).ok().and_then(|i| w.rules.techs.get(i)) else {
            return 10;
        };
        let me = self.profile(p);
        let tb = self.tables;
        let mut score = 40i32;
        let extra = if a { 6 } else { 0 };
        let mut not_required = false;
        let (mut govt_boost, mut space_boost) = (false, false);
        let fl = row.flags;

        // --- advance flags (0x448C3F..0x448DF0) -----------------------------
        if fl & flags::DIPLOMATS != 0 {
            score = 42 + extra; // assigned, not added (mov ebp, edx)
        }
        for (bit, add) in [(1 << 1, 4), (flags::BRIDGES, 2), (flags::DISABLE_FLOOD_PLAIN_DISEASE, 4)] {
            if fl & bit != 0 {
                score += add;
            }
        }
        for bit in [flags::CONSCRIPTION, flags::MOBILIZATION] {
            if fl & bit != 0 {
                score += 1;
            }
        }
        for bit in [flags::RECYCLING, flags::PRECISION_BOMBING] {
            if fl & bit != 0 {
                score += 1;
            }
        }
        for bit in [
            flags::MPP,
            flags::RIGHT_OF_PASSAGE,
            flags::MILITARY_ALLIANCE,
            flags::TRADE_EMBARGO,
        ] {
            if fl & bit != 0 {
                score += 2 + extra;
            }
        }
        for (bit, add) in [
            (flags::DOUBLE_WEALTH, 2),
            (flags::TRADE_OVER_SEA, 4),
            (flags::TRADE_OVER_OCEAN, 8),
            (flags::MAP_TRADING, 2),
            (flags::COMMUNICATION_TRADING, 2),
        ] {
            if fl & bit != 0 {
                score += add;
            }
        }
        if fl & flags::NOT_REQUIRED_FOR_ERA != 0 {
            not_required = true;
        } else {
            score += 1;
        }
        if fl & flags::DOUBLE_WORKER_RATE != 0 {
            score += 8;
        }

        // --- tables ----------------------------------------------------------
        score += tb.tfrm_required.iter().filter(|&&r| r == t).count() as i32;
        score += 16 * tb.good_prerequisite.iter().filter(|&&r| r == t).count() as i32;
        for other in &w.rules.techs {
            for &q in &other.prereq {
                if q == t {
                    score += 1;
                }
            }
        }
        if self.space_race {
            // 0x448E7F: each advance that needs `t` and leads to a building
            // that makes spaceship parts.
            for (o, other) in w.rules.techs.iter().enumerate() {
                for &q in &other.prereq {
                    if q != t {
                        continue;
                    }
                    for (b, bl) in tb.bldgs.iter().enumerate() {
                        if bl.required_advance != o as i32 {
                            continue;
                        }
                        if bl.other_characteristics & BLDG_GREAT_WONDER != 0 && (self.wonder_built)(b) {
                            continue;
                        }
                        if bl.spaceship_part {
                            score += 1;
                            space_boost = true;
                        }
                    }
                }
            }
        }
        for u in &tb.units {
            if u.available_to_civs >> me.race & 1 == 0 || u.required_tech != t {
                continue;
            }
            score += 2;
            if u.ai_strategies & (ai::OFFENSE | ai::DEFENSE) != 0 {
                score += 4;
                if !u.needs_resource {
                    score += if !a {
                        4
                    } else if me.militaristic {
                        128
                    } else {
                        64
                    };
                }
            }
            if u.ai_strategies & ai::DEFENSE != 0 && me.defenders == 0 {
                score += if a { 64 } else { 4 };
            }
            if u.ai_strategies & ai::NAVAL_TRANSPORT != 0 && me.transports == 0 {
                score += if a { 32 } else { 4 };
            }
        }
        for &g in &tb.govt_prerequisite {
            if g == t {
                score += 1;
                govt_boost = true;
            }
        }
        for (b, bl) in tb.bldgs.iter().enumerate() {
            if bl.required_advance != t {
                continue;
            }
            let wonder = bl.other_characteristics & BLDG_GREAT_WONDER != 0;
            if wonder && (self.wonder_built)(b) {
                continue;
            }
            score += 2;
            if wonder {
                score += 4;
            }
            if bl.improvement_flags & BLDG_VALUED_FLAGS != 0 {
                score += if a { 16 } else { 4 };
            }
            if self.space_race && bl.spaceship_part {
                score += 1;
                space_boost = true;
            }
            score += flavor_overlap(&tb.flavors, bl.flavors, me.flavors) - 50;
        }
        score += 2 * tb.ctzn_prerequisite.iter().filter(|&&r| r == t).count() as i32;

        // --- cost or urgency term (0x4491EF) ---------------------------------
        if score > 0 {
            if a {
                score += 256 / w.turns_left(p, t, true).max(1);
                if mode {
                    // `0x47B530()` (multiplayer) is false in single player.
                    score += (dice.below(32) & 0xFFFF) + (dice.below(32) & 0xFFFF);
                }
            } else {
                score += w.base_cost(p, t, true);
            }
        }
        if govt_boost {
            score *= 2;
        }
        if space_boost {
            score *= 2;
        }

        // --- the contact tail (0x4492C8) -------------------------------------
        let me_w = &w.players[p as usize];
        let contacted = |q: u32| w.in_play >> q & 1 != 0 && me_w.contact >> q & 1 != 0 && q != p;
        let human = w.human >> p & 1 != 0;
        if a {
            if !human {
                let often = self.categories.get(t as usize).copied().unwrap_or(0) & me.build_often != 0;
                if often {
                    score = tdiv(3 * score, 2);
                }
                let rivals = (1..32u32)
                    .filter(|&q| contacted(q) && w.players[q as usize].current == t)
                    .count() as i32;
                score = tdiv(score, rivals.clamp(1, 4));
            }
        } else {
            let knowers = (1..32u32).filter(|&q| contacted(q) && w.knows(q, t)).count();
            if knowers < 2 {
                score *= 2;
            }
            if me_w.current == t {
                let base = w.base_cost(p, t, true);
                let left = (base - me_w.beakers).max(0);
                if base != 0 {
                    score = tdiv(left * score, base);
                }
            }
            score = tdiv(2 * score, 3);
        }

        // --- the flavor stage (0x449464) -------------------------------------
        let matched = flavor_overlap(&self.tables.flavors, row.flavors, me.flavors);
        score = flavor_scale(score, matched, row.flavors);
        let halve = not_required && matched < 90;
        if score > 0 && fl & flags::CANNOT_BE_TRADED != 0 {
            score = (1.5 * f64::from(score)) as i32;
        }
        if halve {
            score = tdiv(2 * score, 3);
        }
        score.max(10)
    }

    /// `0x449530 defaultPick(P; mode)`: the best researchable advance, or
    /// `T` when none scores above zero.
    pub fn default_pick(&self, w: &World, p: u32, mode: u8, dice: &mut dyn Dice) -> i32 {
        let (mut best, mut best_score) = (NONE, 0);
        for t in 0..w.t() {
            if w.can_research(p, t) {
                let v = self.value(w, p, t, true, mode != 0, dice);
                if v > best_score {
                    best_score = v;
                    best = t;
                }
            }
        }
        if best == NONE { w.t() } else { best }
    }

    /// `0x44A5B0 stealPick(Thief; victim)`: the advance a thief takes from
    /// `victim`, or [`NONE`] when there is nothing worth taking.
    pub fn steal_pick(&self, w: &World, thief: u32, victim: u32, dice: &mut dyn Dice) -> i32 {
        let (mut best, mut best_score) = (NONE, 0);
        for t in 0..w.t() {
            if w.can_research(thief, t) && w.knows(victim, t) {
                let v = self.value(w, thief, t, false, false, dice);
                if v > best_score {
                    best_score = v;
                    best = t;
                }
            }
        }
        best
    }
}

impl Brain for Valuer<'_> {
    fn default_pick(&mut self, w: &World, p: u32, mode: u8, dice: &mut dyn Dice) -> i32 {
        Valuer::default_pick(self, w, p, mode, dice)
    }

    fn value(&mut self, w: &World, p: u32, t: i32, dice: &mut dyn Dice) -> i32 {
        Valuer::value(self, w, p, t, false, false, dice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::research::{Rules, TechRow};
    use crate::rng::Rng;

    /// Draws that return a fixed value (so the jitter is predictable).
    struct Fixed(i32);
    impl Dice for Fixed {
        fn below(&mut self, _n: u32) -> i32 {
            self.0
        }
    }

    fn tech(cost: i32, prereq: [i32; 4], flags: u32, flavors: u32) -> TechRow {
        TechRow { cost, era: 0, prereq, flags, flavors }
    }

    /// Three civs (slots 1..3) and three isolated advances of cost 3
    /// (base cost 72 for every civ).
    fn world() -> World {
        let rules = Rules {
            techs: vec![tech(3, [-1; 4], 0, 0), tech(3, [-1; 4], 0, 0), tech(3, [-1; 4], 0, 0)],
            future_tech_cost: 400,
            max_research_turns: 50,
            min_research_turns: 4,
        };
        let mut w = World::new(rules);
        w.in_play = 0b1110;
        w.human = 1 << 1;
        for p in 1..4 {
            w.players[p].rate = 10;
            w.players[p].cities = 1;
        }
        w
    }

    fn valuer<'a>(tables: &'a Tables, profiles: &'a [Profile]) -> Valuer<'a> {
        Valuer { tables, profiles, categories: &[], space_race: false, wonder_built: &|_| false }
    }

    fn profiles() -> Vec<Profile> {
        vec![Profile::default(); 32]
    }

    #[test]
    fn an_isolated_advance_without_contact() {
        // 40 + 1 (no "not required" flag) + base 72 = 113; no knowers so x2
        // = 226; x2/3 = 150; flavor-free (0.3f + 1.0) -> 195.
        let (w, pr, t) = (world(), profiles(), Tables::default());
        let v = valuer(&t, &pr);
        assert_eq!(v.value(&w, 2, 0, false, false, &mut Fixed(0)), 195);
    }

    #[test]
    fn the_research_pick_weights_a_human() {
        // 41 + 256/turnsLeft(8) = 73; human skips the tail; (0.3f+1)*73 = 94.
        let (w, pr, t) = (world(), profiles(), Tables::default());
        let v = valuer(&t, &pr);
        assert_eq!(v.value(&w, 1, 0, true, false, &mut Fixed(0)), 94);
        // The jitter only applies with mode set: + (7 + 7) = 87 -> 113.
        assert_eq!(v.value(&w, 1, 0, true, true, &mut Fixed(7)), 113);
    }

    #[test]
    fn rivals_divide_an_ai_pick() {
        let (mut w, pr, t) = (world(), profiles(), Tables::default());
        let v = valuer(&t, &pr);
        // One contacted rival also researching tech 0: /1 -> 73 -> 94.
        w.players[2].contact = 0b1000;
        w.players[3].current = 0;
        assert_eq!(v.value(&w, 2, 0, true, false, &mut Fixed(0)), 94);
        // Two rivals: 73/2 = 36 -> 46.
        w.players[2].contact = 0b1010;
        w.players[1].current = 0;
        assert_eq!(v.value(&w, 2, 0, true, false, &mut Fixed(0)), 46);
        // A rival without contact does not count: back to /1.
        w.players[2].contact = 0b0010;
        w.players[3].current = 0;
        assert_eq!(v.value(&w, 2, 0, true, false, &mut Fixed(0)), 94);
    }

    #[test]
    fn build_often_scales_three_halves() {
        let (w, mut pr, t) = (world(), profiles(), Tables::default());
        pr[2].build_often = 1;
        let cats = [1u32, 0, 0];
        let v = Valuer { categories: &cats, ..valuer(&t, &pr) };
        // 73 * 3 / 2 = 109 -> (0.3f+1)*109 = 141.7.
        assert_eq!(v.value(&w, 2, 0, true, false, &mut Fixed(0)), 141);
    }

    #[test]
    fn two_knowers_cancel_the_doubling() {
        let (mut w, pr, t) = (world(), profiles(), Tables::default());
        let v = valuer(&t, &pr);
        w.players[2].contact = 0b1010;
        w.known[0] |= 0b1010;
        // Two contacted civs know it, so baseCost drops to (3-2)/3 of 72 = 24
        // (research.md 4.1): 41 + 24 = 65; two knowers, no doubling;
        // 2*65/3 = 43; 43 * 1.3 = 55.9.
        assert_eq!(v.value(&w, 2, 0, false, false, &mut Fixed(0)), 55);
    }

    #[test]
    fn current_research_discounts_by_what_is_left() {
        let (mut w, pr, t) = (world(), profiles(), Tables::default());
        let v = valuer(&t, &pr);
        w.players[2].current = 0;
        w.players[2].beakers = 30;
        // 113 -> 226 -> 42*226/72 = 131 -> 2*131/3 = 87 -> 113.
        assert_eq!(v.value(&w, 2, 0, false, false, &mut Fixed(0)), 113);
    }

    #[test]
    fn flavor_overlap_cases() {
        let table = vec![vec![100, 80], vec![50, 100]];
        assert_eq!(flavor_overlap(&table, 0b01, 0b01), 100);
        assert_eq!(flavor_overlap(&table, 0b01, 0b10), 80);
        assert_eq!(flavor_overlap(&table, 0b10, 0b01), 50);
        assert_eq!(flavor_overlap(&table, 0, 0b11), 50);
        assert_eq!(flavor_overlap(&table, 0b11, 0b10), 100);
        assert_eq!(flavor_overlap(&[], 1, 1), 50);
    }

    #[test]
    fn flavor_scaling_uses_single_precision_constants() {
        // 150 * (0.3f + 6400 * 1e-4f) = 140.99999...; truncated to 140.
        assert_eq!(flavor_scale(150, 80, 1), 140);
        assert_eq!(flavor_scale(150, 100, 1), 270);
        assert_eq!(flavor_scale(150, 50, 1), 82);
        assert_eq!(flavor_scale(150, 50, 0), 195);
    }

    #[test]
    fn unit_enablers_gain_the_build_weights() {
        let (w, mut pr, mut t) = (world(), profiles(), Tables::default());
        t.units.push(UnitRow {
            required_tech: 0,
            available_to_civs: !0,
            ai_strategies: ai::OFFENSE,
            needs_resource: false,
        });
        // a = 0: 41 + 2 (enables) + 4 (offense) + 4 (no resource) = 51;
        // + 72 = 123; x2 = 246; x2/3 = 164; x1.3 = 213.2.
        let v = valuer(&t, &pr);
        assert_eq!(v.value(&w, 2, 0, false, false, &mut Fixed(0)), 213);
        // a = 1 (human): 41 + 2 + 4 + 64 = 111; + 256/8 = 143; x1.3 = 185.9.
        assert_eq!(v.value(&w, 1, 0, true, false, &mut Fixed(0)), 185);
        // Militaristic: 64 -> 128 = 175; +32 = 207 -> 269.
        pr[1].militaristic = true;
        let v = valuer(&t, &pr);
        assert_eq!(v.value(&w, 1, 0, true, false, &mut Fixed(0)), 269);
    }

    #[test]
    fn defenders_and_transports_add_need_when_there_are_none() {
        let (w, pr, mut t) = (world(), profiles(), Tables::default());
        t.units.push(UnitRow {
            required_tech: 0,
            available_to_civs: !0,
            ai_strategies: ai::DEFENSE | ai::NAVAL_TRANSPORT,
            needs_resource: true,
        });
        // a = 1: 41 + 2 + 4 (defense/offense, resource: no weight)
        //   + 64 (no defenders) + 32 (no transports) = 143; + 32 = 175 -> 227.
        let v = valuer(&t, &pr);
        assert_eq!(v.value(&w, 1, 0, true, false, &mut Fixed(0)), 227);
    }

    #[test]
    fn the_diplomats_flag_assigns_the_base() {
        let (mut w, pr, t) = (world(), profiles(), Tables::default());
        w.rules.techs[0].flags = flags::DIPLOMATS;
        let v = valuer(&t, &pr);
        // a = 1: 48 + 1 = 49; + 256/8 = 81 -> 105.
        assert_eq!(v.value(&w, 1, 0, true, false, &mut Fixed(0)), 105);
        // a = 0: 42 + 1 = 43 + 72 = 115 -> 230 -> 153 -> 198.
        assert_eq!(v.value(&w, 2, 0, false, false, &mut Fixed(0)), 198);
    }

    #[test]
    fn untradable_advances_are_worth_half_again() {
        let (mut w, pr, t) = (world(), profiles(), Tables::default());
        w.rules.techs[0].flags = flags::CANNOT_BE_TRADED;
        let v = valuer(&t, &pr);
        // Identical to the isolated case (195), then x1.5 = 292.
        assert_eq!(v.value(&w, 2, 0, false, false, &mut Fixed(0)), 292);
    }

    #[test]
    fn prerequisite_chain_adds_one_per_dependent() {
        let (mut w, pr, t) = (world(), profiles(), Tables::default());
        let v = valuer(&t, &pr);
        // Tech 0 gates tech 1 (and 2): 41 + 2 + 72 = 115 -> 230 -> 153 -> 198.
        w.rules.techs[1].prereq = [0, -1, -1, -1];
        w.rules.techs[2].prereq = [0, -1, -1, -1];
        assert_eq!(v.value(&w, 2, 0, false, false, &mut Fixed(0)), 198);
    }

    #[test]
    fn the_pick_takes_the_highest_strictly_positive_score() {
        let (mut w, pr, t) = (world(), profiles(), Tables::default());
        let v = valuer(&t, &pr);
        let mut dice = Rng::new(1);
        // All three score alike: the lowest index wins a tie.
        assert_eq!(v.default_pick(&w, 2, 0, &mut dice), 0);
        // Untradable advances are worth half again, so tech 1 now wins.
        w.rules.techs[1].flags = flags::CANNOT_BE_TRADED;
        assert_eq!(v.default_pick(&w, 2, 0, &mut dice), 1);
        // The thief takes only what the victim knows.
        assert_eq!(v.steal_pick(&w, 2, 3, &mut Fixed(0)), NONE);
    }

    #[test]
    fn category_masks_follow_the_flag_table() {
        assert_eq!(category_mask_from_flags(0), 0);
        assert_eq!(category_mask_from_flags(flags::DIPLOMATS | flags::MILITARY_ALLIANCE), 0x03);
        assert_eq!(category_mask_from_flags(flags::RIGHT_OF_PASSAGE), 0x2001);
        assert_eq!(category_mask_from_flags(flags::DOUBLE_WORKER_RATE), 0x1d90);
    }
}
