//! The AI's advance valuation and the picks built on it, plus the per-advance
//! category mask.
//!
//! `value(P; t, a, mode)` scores one advance for one player. The same routine
//! serves the automatic research pick (`a = true`), the human suggestion, and
//! stealing (`a = false`). The two flags matter: `a` switches the weights and
//! the rival-versus-knower tail; `mode` only gates the random draw.

use crate::research::{flags, Brain, Dice, World, NONE};

/// `PRTO.ai_strategies` bits the valuation reads.
pub mod ai {
    /// Offensive unit class.
    pub const OFFENSE: u32 = 1 << 0;
    /// Defensive unit class.
    pub const DEFENSE: u32 = 1 << 1;
    /// Naval transport.
    pub const NAVAL_TRANSPORT: u32 = 1 << 10;
}

/// `BLDG.improvement_flags` bits that earn the building bonus.
pub const BLDG_VALUED_FLAGS: u32 = 0x1800;

/// `BLDG.other_characteristics` bit 2: a great wonder.
pub const BLDG_GREAT_WONDER: u32 = 0x4;

/// One `PRTO` row, reduced to what the valuation reads.
#[derive(Clone, Debug, Default)]
pub struct UnitRow {
    /// `required_tech`.
    pub required_tech: i32,
    /// `available_to_civs`: bit `race` set means the civ may build it.
    pub available_to_civs: u32,
    /// `ai_strategies`.
    pub ai_strategies: u32,
    /// Any of `required_resource_1..3` is not `-1`.
    pub needs_resource: bool,
}

/// One `BLDG` row, reduced to what the valuation reads.
#[derive(Clone, Debug, Default)]
pub struct BldgRow {
    /// `required_advance`.
    pub required_advance: i32,
    /// `spaceship_part` is not `-1`.
    pub spaceship_part: bool,
    /// `improvement_flags`.
    pub improvement_flags: u32,
    /// `other_characteristics`.
    pub other_characteristics: u32,
    /// Flavor mask.
    pub flavors: u32,
}

/// The rule tables the valuation walks, one list per `BIQ` section.
#[derive(Clone, Debug, Default)]
pub struct Tables {
    /// `TFRM.required_tech` of the 13 worker jobs.
    pub tfrm_required: Vec<i32>,
    /// `GOOD.prerequisite` of every resource.
    pub good_prerequisite: Vec<i32>,
    /// Every `PRTO` row.
    pub units: Vec<UnitRow>,
    /// `GOVT.prerequisite_tech` of every government.
    pub govt_prerequisite: Vec<i32>,
    /// Every `BLDG` row.
    pub bldgs: Vec<BldgRow>,
    /// `CTZN.prerequisite` of every citizen type.
    pub ctzn_prerequisite: Vec<i32>,
    /// `FLAV` relationship matrix: `flavors[i][j]` is the percentage.
    pub flavors: Vec<Vec<i32>>,
}

/// The per-civilization inputs.
#[derive(Clone, Debug, Default)]
pub struct Profile {
    /// `RACE.flavors`.
    pub flavors: u32,
    /// `RACE.build_often`.
    pub build_often: u32,
    /// The race's Militaristic trait.
    pub militaristic: bool,
    /// The race index, the bit of `PRTO.available_to_civs`.
    pub race: u32,
    /// The AI's defender tally.
    pub defenders: i32,
    /// The AI's naval transport tally.
    pub transports: i32,
}

/// Everything the valuation needs besides the [`World`].
pub struct Valuer<'a> {
    /// The rule tables.
    pub tables: &'a Tables,
    /// One profile per slot.
    pub profiles: &'a [Profile],
    /// The category mask of every advance. May be empty (all zero).
    pub categories: &'a [u32],
    /// The Space Race victory is enabled.
    pub space_race: bool,
    /// This great wonder has been built.
    pub wonder_built: &'a dyn Fn(usize) -> bool,
}

/// The weighted overlap of two flavor masks. Any pair whose relationship is
/// `>= 100` returns `100`; otherwise the mean of the relationships over every
/// (advance flavor, civ flavor) pair; `50` when the table is empty or no pair
/// exists.
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
    if count == 0 {
        50
    } else {
        sum / count
    }
}

/// The advance-flag part of the category mask: the category bits an advance
/// contributes from its flags.
pub fn category_mask_from_flags(f: u32) -> u32 {
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

/// C division, which Rust's `/` is.
fn tdiv(a: i32, b: i32) -> i32 {
    a / b
}

/// `ftol((0.3f + f) * score)`.
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

    /// `0x448BF0 value(P; t, a, mode)`. `a` selects the research-pick weights
    /// and tail; `mode` enables the jitter (only when `a` is set).
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

        if fl & flags::DIPLOMATS != 0 {
            score = 42 + extra;
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

        if score > 0 {
            if a {
                score += 256 / w.turns_left(p, t, true).max(1);
                if mode {
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

    /// `0x449530 defaultPick(P; mode)`: the best researchable advance, or `T`
    /// when none scores above zero.
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
        if best == NONE {
            w.t()
        } else {
            best
        }
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
    use civ3_worldgen::rng::Rng;

    #[test]
    fn flavor_overlap_averages_and_short_circuits() {
        assert_eq!(flavor_overlap(&[], 1, 1), 50);
        let table = vec![vec![0, 60], vec![80, 100]];
        assert_eq!(flavor_overlap(&table, 0b01, 0b10), 60);
        assert_eq!(flavor_overlap(&table, 0b10, 0b10), 100, ">= 100 short-circuits");
        assert_eq!(flavor_overlap(&table, 0b01, 0), 50, "no civ flavor");
    }

    #[test]
    fn category_mask_ors_the_flag_bits() {
        assert_eq!(category_mask_from_flags(0), 0);
        assert_eq!(category_mask_from_flags(0x1), 0x3);
        assert_eq!(category_mask_from_flags(0x1 | 0x400), 0x3 | 0x1);
    }

    fn rules() -> Rules {
        Rules {
            techs: vec![
                TechRow { cost: 10, era: 0, prereq: [NONE; 4], flags: 0, flavors: 0 },
                TechRow { cost: 20, era: 0, prereq: [0, NONE, NONE, NONE], flags: 0, flavors: 0 },
            ],
            future_tech_cost: 100,
            max_research_turns: 40,
            min_research_turns: 4,
        }
    }

    fn no_wonder(_: usize) -> bool {
        false
    }

    fn valuer<'a>(tables: &'a Tables, profiles: &'a [Profile]) -> Valuer<'a> {
        Valuer {
            tables,
            profiles,
            categories: &[],
            space_race: false,
            wonder_built: &no_wonder,
        }
    }

    #[test]
    fn a_valuable_advance_scores_above_the_floor() {
        let mut w = World::new(rules());
        w.in_play = 0b11;
        w.human = 0b11;
        let tables = Tables::default();
        let profiles = vec![Profile::default()];
        let v = valuer(&tables, &profiles);
        let mut dice = Rng::new(1);
        let score = v.value(&w, 0, 0, true, false, &mut dice);
        assert!(score >= 10);
    }

    #[test]
    fn default_pick_returns_the_best_researchable() {
        let mut w = World::new(rules());
        w.in_play = 0b11;
        w.human = 0b11;
        let tables = Tables::default();
        let profiles = vec![Profile::default()];
        let v = valuer(&tables, &profiles);
        let mut dice = Rng::new(1);
        let pick = v.default_pick(&w, 0, 1, &mut dice);
        assert!(pick == 0 || pick == 1, "a researchable advance");
    }
}
