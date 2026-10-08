//! Culture, corruption, the commerce split, city growth and the gold side of a
//! turn (unit support, upkeep, the treasury).
//!
//! The functions mirror parent-verified instruction sequences; values are
//! non-negative game quantities so the truncating divisions match Rust `/`.

/// Culture level: the count of powers `base^k <= accum`, starting at 1, capped
/// at 6.
pub fn culture_level(accum: u32, base: u32) -> u32 {
    let mut level = 1;
    let mut power = base;
    while accum >= power && level < 6 {
        power = power.wrapping_mul(base);
        level += 1;
    }
    level
}

/// Commerce split: `(net * rate + 5) / 10` via the magic divider. `rate` is
/// tenths (0–10).
pub fn split_share(net: i32, rate: i32) -> i32 {
    net.wrapping_mul(rate).wrapping_add(5) / 10
}

/// Corruption rank prime: `rank >= R ? 2*rank - R : rank`.
pub fn rank_prime(rank: i32, r: i32) -> i32 {
    if rank >= r {
        rank.wrapping_mul(2).wrapping_sub(r)
    } else {
        rank
    }
}

/// Largest population of a town: RULE body `+0x11C` (shipped 6).
pub const TOWN_MAX: i32 = 6;

/// Largest population of a city: RULE body `+0x120` (shipped 12).
pub const CITY_MAX: i32 = 12;

/// Freshwater or an active size-level-2 improvement bypasses the town gate; the
/// level-3 gate never tests freshwater. Stock RULE limits are six and twelve.
pub fn growth_limit(fresh_water: bool, level_2: bool, level_3: bool) -> i32 {
    if !fresh_water && !level_2 {
        TOWN_MAX
    } else if !level_3 {
        CITY_MAX
    } else {
        i32::MAX
    }
}

/// City size class: 0 town, 1 city, 2 metropolis.
pub fn size_class(population: i32, town_max: i32, city_max: i32) -> i32 {
    if population > city_max {
        2
    } else {
        (population > town_max) as i32
    }
}

/// The box unit X: 10 for a human player, otherwise the difficulty's own
/// number; halved (truncating) when the flag is set; at least 1.
pub fn box_unit(human: bool, difficulty_x: i32, halved: bool) -> i32 {
    let x = if human { 10 } else { difficulty_x };
    let x = if halved { x / 2 } else { x };
    x.max(1)
}

/// The food box: `2 * X * (class + 1)`, i.e. 20 / 40 / 60 for a human.
pub fn food_box(x: i32, class: i32) -> i32 {
    2 * x * (class + 1)
}

/// What one turn of the food routine does to a city.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoodTurn {
    /// Stored food plus surplus is negative: the store is 0 and a citizen goes.
    Starved,
    /// No growth: this is the new store (a blocked city keeps a full box).
    Stored(i32),
    /// A citizen is added and this is the new store: half the box with a
    /// Granary-flagged improvement, else 0. The old total is not carried over.
    Grew(i32),
}

/// One turn of the food routine. Growth needs the *new* total to reach the box,
/// and the class is the pre-growth one.
pub fn food_turn(
    stored: i32,
    surplus: i32,
    x: i32,
    class: i32,
    granary: bool,
    blocked: bool,
) -> FoodTurn {
    let total = stored + surplus;
    if total < 0 {
        return FoodTurn::Starved;
    }
    let full = food_box(x, class);
    if total < full {
        FoodTurn::Stored(total)
    } else if blocked {
        FoodTurn::Stored(full)
    } else {
        FoodTurn::Grew(if granary { full / 2 } else { 0 })
    }
}

/// How the treasury is stored: two cells whose sum is the gold, split afresh
/// from the clock on every write. A tamper guard, not a game rule.
pub fn treasury_cells(n: i32, clock: u32) -> (i32, i32) {
    if n <= 0 {
        let a = (clock % 0xD431) as i32 - 0x8235;
        (a, -a)
    } else {
        let a = (clock % n as u32) as i32 - 0x3039;
        (a, n - a)
    }
}

/// What a government charges for units: the gold per unit past the free ones
/// and how many are free.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Support {
    /// GOVT `+0x1E0`; 0 when the base is -1.
    pub per_unit: i32,
    /// `max(0, base + sum of per-class allowances)`, civ-wide.
    pub free: i32,
}

/// The unit-support terms of a government. `base` is GOVT `+0x1D0`,
/// `per_class` the dwords at `+0x1D4 / +0x1D8 / +0x1DC` (town, city,
/// metropolis), `per_unit` `+0x1E0`, `classes` the size class of each of the
/// player's cities. A base of -1 means nothing is charged.
pub fn support_terms(
    base: i32,
    per_class: [i32; 3],
    per_unit: i32,
    classes: impl IntoIterator<Item = i32>,
) -> Support {
    if base == -1 {
        return Support { per_unit: 0, free: 0 };
    }
    let sum: i32 = classes.into_iter().map(|c| per_class[c as usize]).sum();
    Support {
        per_unit,
        free: (base + sum).max(0),
    }
}

/// Gold owed for units: nothing while the player has no cities, else
/// `(units - exempt - free) * per_unit`, and nothing when that is not positive.
pub fn unit_support_charge(cities: i32, units: i32, exempt: i32, free: i32, per_unit: i32) -> i32 {
    if cities == 0 {
        return 0;
    }
    ((units - exempt - free) * per_unit).max(0)
}

/// Pay a bill from the treasury: the new treasury and whether the bill went
/// unmet. Unmet, the treasury is 0 and the player loses one unit or one
/// improvement.
pub fn pay_bill(treasury: i32, bill: i32) -> (i32, bool) {
    if bill <= treasury {
        (treasury - bill, false)
    } else {
        (0, true)
    }
}

/// Whether a turn pays units before upkeep: one turn in four.
pub fn units_first(roll: u32) -> bool {
    roll == 0
}

/// Bit numbers of the civ trait mask (RACE `+0x948`).
pub mod trait_bit {
    /// Militaristic (promotion die, cheaper Barracks-class buildings).
    pub const MILITARISTIC: u32 = 0;
    /// Commercial (`+B / 4` optimal cities, `optimal_city_number`).
    pub const COMMERCIAL: u32 = 1;
    /// Expansionist.
    pub const EXPANSIONIST: u32 = 2;
    /// Scientific (cheaper Library-class buildings).
    pub const SCIENTIFIC: u32 = 3;
    /// Religious (cheaper Temple-class buildings).
    pub const RELIGIOUS: u32 = 4;
    /// Industrious.
    pub const INDUSTRIOUS: u32 = 5;
    /// Agricultural (cheaper Aqueduct-class buildings).
    pub const AGRICULTURAL: u32 = 6;
    /// Seafaring (cheaper Harbor-class buildings).
    pub const SEAFARING: u32 = 7;
}

/// Bit `bit` of the trait mask. The shift count is taken modulo 32.
pub fn has_trait(mask: u32, bit: u32) -> bool {
    mask & (1 << (bit & 31)) != 0
}

/// The (trait, BLDG `+0xF0` bit) pairs the discount probe walks, in code order:
/// Militaristic `0x2`, Religious `0x100`, Agricultural `0x400`, Seafaring
/// `0x800`, Scientific `0x20`.
pub const TRAIT_DISCOUNTS: [(u32, u32); 5] = [
    (trait_bit::MILITARISTIC, 0x2),
    (trait_bit::RELIGIOUS, 0x100),
    (trait_bit::AGRICULTURAL, 0x400),
    (trait_bit::SEAFARING, 0x800),
    (trait_bit::SCIENTIFIC, 0x20),
];

/// Does a civ with trait mask `traits` get the half price on a building whose
/// BLDG word at `+0xF0` is `flags`? Great wonders (bit 4) and small wonders
/// (bit 8) never do. For other buildings one matching pair is enough; several
/// do not stack.
pub fn trait_discount(flags: u32, traits: u32) -> bool {
    if flags & 0xC != 0 {
        return false;
    }
    TRAIT_DISCOUNTS
        .iter()
        .any(|&(bit, mask)| has_trait(traits, bit) && flags & mask != 0)
}

/// The multiplier a Center-of-Empire building gets: `6 * cities / B`, clamped
/// to `3..=10`, where `B` is the world size's optimal number of cities.
pub fn palace_cost_factor(cities: i32, optimal_cities: i32) -> i32 {
    (6 * cities / optimal_cities).clamp(3, 10)
}

/// An improvement's cost: BLDG `+0x94` times X, halved once when the trait
/// discount holds, times the palace factor for a Center-of-Empire building, at
/// least 1. The multiplications wrap; the halving truncates toward zero.
pub fn improvement_cost(base: i32, x: i32, trait_match: bool, palace_factor: Option<i32>) -> i32 {
    let mut cost = base.wrapping_mul(x);
    if trait_match {
        cost /= 2;
    }
    if let Some(factor) = palace_factor {
        cost = cost.wrapping_mul(factor);
    }
    cost.max(1)
}

/// Gold a sold improvement fetches: [`improvement_cost`] divided by the RULE
/// "Shield Cost Per Gold" divisor.
pub fn sale_price(cost: i32, divisor: i32) -> i32 {
    cost / divisor
}

/// The corruption class that makes a government "communal": GOVT `+0x18C` is
/// 0 minimal, 1 nuisance, 2 problematic, 3 rampant, 4 catastrophic, 5 communal.
pub const COMMUNAL_CLASS: i32 = 5;

/// Everything the optimal city number reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OcnInputs {
    /// "Optimal number of cities" of the world size (shipped 14 / 17 / 20 / 28 /
    /// 36 for Tiny to Huge).
    pub world_base: i32,
    /// The player's Forbidden-Palace-like wonders.
    pub palace_like: i32,
    /// GOVT `+0x18C` of the player's government (see [`COMMUNAL_CLASS`]).
    pub class: i32,
    /// The civ has [`trait_bit::COMMERCIAL`].
    pub commercial: bool,
    /// The player is human.
    pub human: bool,
    /// The game difficulty (0 Chieftain to 7 Sid); read for an AI player only.
    pub game_level: i32,
    /// The difficulty's percentage (shipped 100, 95, 90, 85, 80, 70, 60, 50).
    pub percent: i32,
}

/// The optimal city number, OCN.
pub fn optimal_city_number(i: &OcnInputs) -> i32 {
    let base = i.world_base;
    let divisor = if i.class == COMMUNAL_CLASS { 1 } else { 8 };
    let mut ocn = base + base.wrapping_mul(i.palace_like).wrapping_mul(3) / divisor;
    if i.commercial {
        ocn += base / 4;
    }
    ocn += match i.class {
        0 | 1 => base / 8,
        2 => base / 16,
        COMMUNAL_CLASS => base * 2,
        _ => 0,
    };
    if !i.human {
        ocn += if i.game_level > 4 {
            base / 2
        } else if i.game_level > 3 {
            base / 4
        } else if i.game_level > 2 {
            base / 8
        } else {
            0
        };
    }
    (i.percent.wrapping_mul(ocn) / 100).max(1)
}

/// What the corruption routine reads. Distances are the game's metric.
#[derive(Clone, Debug, Default)]
pub struct CorruptionInputs {
    /// Commerce (corruption) or shields (waste) before the loss.
    pub gross: i32,
    /// False for commerce corruption, true for waste.
    pub waste: bool,
    /// City flag: disorder.
    pub disorder: bool,
    /// City flag: celebration.
    pub celebrating: bool,
    /// The owner has a capital.
    pub has_capital: bool,
    /// GOVT `+0x18C` of the owner.
    pub class: i32,
    /// Present, non-obsolete Courthouses.
    pub courthouses: i32,
    /// This city is the owner's capital.
    pub is_capital: bool,
    /// Owned Reduces-Corruption wonders standing in this city.
    pub palaces_here: i32,
    /// [`optimal_city_number`].
    pub ocn: i32,
    /// The world size's optimal number `B`.
    pub world_base: i32,
    /// Distance to the capital from this city (`i32::MAX` when there is none).
    pub capital_distance: i32,
    /// Distance to the nearest such wonder city from this city.
    pub palace_distance: i32,
    /// Trade-connected to the capital.
    pub connected: bool,
    /// Map width in x units.
    pub width: i32,
    /// Map height.
    pub height: i32,
    /// Same-owner cities ranked ahead, or, for the communal class, half the
    /// owner's city count.
    pub rank: i32,
    /// CTZN `+0x78` summed over non-resisting citizens' jobs.
    pub specialists: i32,
    /// DIFF `+0x74` of the owner's difficulty.
    pub difficulty_percent: i32,
}

/// Toward-zero halving of `x + 1`.
fn half_up(x: i32) -> i32 {
    x.wrapping_add(1) / 2
}

/// The commerce or shields lost to corruption.
pub fn corruption(i: &CorruptionInputs) -> i32 {
    let gross = i.gross;
    if gross <= 0 {
        return 0;
    }
    if i.waste && i.disorder {
        return gross;
    }
    if !i.has_capital {
        return 0;
    }
    if i.class == 4 {
        return gross;
    }
    let mut c = i.courthouses + if i.is_capital { 10 } else { 0 };
    let b = i.world_base;
    let mut o = i.ocn + b.wrapping_mul(c) / 4;
    if i.waste && i.celebrating {
        o += b / 4;
    }
    c += 7 * i.palaces_here;
    let d = i.capital_distance.min(i.palace_distance);
    let m = (i.width + i.height) / 4;
    let mut e = match i.class {
        0 => 3 * d / 4,
        1 | 2 => d,
        3 => 3 * d / 2,
        5 => m / 4,
        _ => m,
    };
    if !i.connected {
        e = 5 * e / 4;
    }
    e = if m < 2 { 2 } else { e.clamp(2, m) };
    if i.waste && i.celebrating {
        e = half_up(e);
    }
    for _ in 0..c.max(0) {
        e = half_up(e);
    }
    let share = e.wrapping_mul(gross);
    let rank = rank_prime(i.rank, o);
    let half_rank = half_up(rank.wrapping_mul(gross));
    let num = m
        .wrapping_mul(half_rank)
        .wrapping_add(share.wrapping_mul(o))
        .wrapping_add(m.wrapping_mul(o) / 2);
    let mut lost = num / m.wrapping_mul(o);
    lost = if i.specialists >= lost { 0 } else { lost - i.specialists };
    let lost = lost.wrapping_mul(i.difficulty_percent) / 100;
    let cap = (9 - c).max(0).wrapping_mul(gross) / 10;
    if cap < 0 || lost < 0 {
        return 0;
    }
    lost.min(cap)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corruption_row(v: &[i32]) -> (CorruptionInputs, i32) {
        (
            CorruptionInputs {
                gross: v[0],
                waste: v[1] != 0,
                disorder: v[2] != 0,
                celebrating: v[3] != 0,
                has_capital: v[4] != 0,
                class: v[5],
                courthouses: v[6],
                is_capital: v[7] != 0,
                palaces_here: v[8],
                ocn: v[9],
                world_base: v[10],
                capital_distance: v[11],
                palace_distance: v[12],
                connected: v[13] != 0,
                width: v[14],
                height: v[15],
                rank: v[16],
                specialists: v[17],
                difficulty_percent: v[18],
            },
            v[19],
        )
    }

    /// Executed golden vectors: the real corruption routine in the emulator over
    /// a shipped save with the government class, Courthouses, a
    /// Reduces-Corruption wonder, disorder and celebration varied. Columns as in
    /// `corruption_row`, the last the executable's result.
    #[test]
    fn corruption_golden_vectors_from_the_executable() {
        let rows: &[[i32; 20]] = &[
            [60, 0, 0, 0, 1, 0, 1, 1, 0, 16, 16, 0, 2147483647, 1, 140, 132, 0, 0, 100, 0],
            [60, 0, 1, 0, 1, 0, 0, 0, 0, 16, 16, 60, 2147483647, 0, 140, 132, 12, 0, 100, 54],
            [60, 1, 1, 0, 1, 0, 0, 0, 0, 16, 16, 60, 2147483647, 0, 140, 132, 12, 0, 100, 60],
            [60, 0, 0, 1, 1, 0, 0, 0, 0, 16, 16, 59, 2147483647, 0, 140, 132, 11, 0, 100, 54],
            [60, 1, 0, 1, 1, 0, 0, 0, 0, 16, 16, 59, 2147483647, 0, 140, 132, 11, 0, 100, 41],
            [60, 0, 0, 0, 1, 0, 1, 0, 0, 16, 16, 33, 2147483647, 0, 140, 132, 5, 0, 100, 21],
            [60, 0, 0, 0, 1, 0, 0, 0, 0, 16, 16, 84, 2147483647, 0, 140, 132, 16, 0, 100, 54],
            [60, 0, 1, 0, 1, 0, 1, 0, 0, 16, 16, 46, 2147483647, 0, 140, 132, 8, 0, 100, 31],
            [60, 0, 0, 1, 1, 0, 1, 0, 0, 16, 16, 82, 2147483647, 0, 140, 132, 15, 0, 100, 48],
            [60, 0, 1, 1, 1, 0, 0, 0, 0, 16, 16, 65, 2147483647, 0, 140, 132, 13, 0, 100, 54],
            [60, 1, 1, 1, 1, 0, 0, 0, 0, 16, 16, 65, 2147483647, 0, 140, 132, 13, 0, 100, 60],
            [60, 0, 1, 0, 1, 5, 0, 1, 0, 86, 16, 0, 90, 1, 140, 132, 9, 0, 100, 0],
            [60, 0, 0, 1, 1, 5, 0, 0, 0, 86, 16, 60, 34, 0, 140, 132, 9, 0, 100, 22],
            [60, 1, 0, 1, 1, 5, 0, 0, 0, 86, 16, 60, 34, 0, 140, 132, 9, 0, 100, 13],
            [60, 0, 0, 0, 1, 5, 1, 0, 0, 86, 16, 59, 43, 0, 140, 132, 9, 0, 100, 13],
            [60, 0, 0, 0, 1, 5, 0, 0, 0, 86, 16, 33, 59, 0, 140, 132, 9, 0, 100, 22],
            [60, 0, 1, 0, 1, 5, 1, 0, 1, 86, 16, 90, 0, 0, 140, 132, 9, 0, 100, 4],
            [60, 1, 1, 0, 1, 5, 1, 0, 1, 86, 16, 90, 0, 0, 140, 132, 9, 0, 100, 60],
            [60, 1, 0, 1, 1, 5, 1, 0, 0, 86, 16, 71, 21, 0, 140, 132, 9, 0, 100, 8],
            [60, 1, 1, 0, 1, 5, 0, 0, 0, 86, 16, 22, 74, 0, 140, 132, 9, 0, 100, 60],
            [60, 1, 1, 1, 1, 5, 0, 0, 0, 86, 16, 7, 92, 0, 140, 132, 9, 0, 100, 60],
            [60, 0, 0, 1, 1, 4, 0, 1, 0, 14, 16, 0, 2147483647, 1, 140, 132, 0, 0, 100, 60],
            [60, 1, 0, 0, 1, 4, 1, 0, 0, 14, 16, 60, 2147483647, 0, 140, 132, 12, 0, 100, 60],
            [60, 0, 1, 0, 1, 4, 0, 0, 0, 14, 16, 82, 2147483647, 0, 140, 132, 15, 0, 100, 60],
            [60, 0, 0, 0, 1, 3, 1, 1, 0, 19, 16, 0, 7, 1, 140, 132, 0, 0, 100, 0],
            [60, 0, 0, 0, 1, 3, 0, 0, 0, 19, 16, 60, 58, 0, 140, 132, 12, 0, 100, 54],
            [60, 0, 1, 0, 1, 3, 1, 0, 0, 19, 16, 33, 33, 0, 140, 132, 5, 0, 100, 34],
            [60, 1, 1, 0, 1, 3, 1, 0, 0, 19, 16, 33, 33, 0, 140, 132, 5, 0, 100, 60],
            [60, 0, 0, 1, 1, 3, 1, 0, 0, 19, 16, 46, 41, 0, 140, 132, 8, 0, 100, 40],
            [60, 1, 0, 1, 1, 3, 1, 0, 0, 19, 16, 46, 41, 0, 140, 132, 8, 0, 100, 24],
            [60, 0, 1, 0, 1, 3, 0, 0, 0, 19, 16, 71, 71, 0, 140, 132, 14, 0, 100, 54],
            [60, 1, 1, 0, 1, 3, 0, 0, 0, 19, 16, 71, 71, 0, 140, 132, 14, 0, 100, 60],
            [60, 0, 0, 0, 1, 2, 0, 1, 0, 15, 16, 0, 2147483647, 1, 140, 132, 0, 0, 100, 0],
            [60, 1, 0, 0, 1, 2, 0, 1, 0, 15, 16, 0, 2147483647, 1, 140, 132, 0, 0, 100, 0],
        ];
        for v in rows {
            let (i, want) = corruption_row(v);
            assert_eq!(corruption(&i), want, "{v:?}");
        }
    }

    /// Vectors produced by running the real routine in the emulator over a
    /// loaded save whose state was varied. `CIV3_CORRUPTION_VECTORS` names the
    /// full CSV.
    #[test]
    fn corruption_matches_the_executable() {
        let Ok(path) = std::env::var("CIV3_CORRUPTION_VECTORS") else { return };
        let text = std::fs::read_to_string(path).unwrap();
        let mut n = 0;
        for line in text.lines() {
            let v: Vec<i32> = line.split(',').map(|x| x.parse().unwrap()).collect();
            let (i, want) = corruption_row(&v);
            assert_eq!(corruption(&i), want, "{line}");
            n += 1;
        }
        assert!(n > 0);
    }

    #[test]
    fn freshwater_bypasses_only_the_aqueduct_gate() {
        assert_eq!(growth_limit(false, false, false), 6);
        assert_eq!(growth_limit(false, false, true), 6);
        assert_eq!(growth_limit(true, false, false), 12);
        assert_eq!(growth_limit(false, true, false), 12);
        assert_eq!(growth_limit(true, false, true), i32::MAX);
        assert_eq!(growth_limit(false, true, true), i32::MAX);
    }

    #[test]
    fn size_classes_break_after_the_caps() {
        let class = |p| size_class(p, TOWN_MAX, CITY_MAX);
        assert_eq!([class(1), class(6), class(7), class(12), class(13)], [0, 0, 1, 1, 2]);
        assert_eq!(class(255), 2);
    }

    #[test]
    fn the_box_unit_is_ten_for_humans_and_never_below_one() {
        assert_eq!(box_unit(true, 7, false), 10);
        assert_eq!(box_unit(true, 7, true), 5);
        assert_eq!(box_unit(false, 7, false), 7);
        assert_eq!(box_unit(false, 7, true), 3);
        assert_eq!(box_unit(false, 1, true), 1);
        assert_eq!(box_unit(false, 0, false), 1);
    }

    #[test]
    fn the_food_box_is_twenty_forty_sixty() {
        let x = box_unit(true, 0, false);
        assert_eq!([0, 1, 2].map(|c| food_box(x, c)), [20, 40, 60]);
    }

    #[test]
    fn growth_fires_when_the_new_total_reaches_the_box() {
        use FoodTurn::*;
        let turn =
            |stored, surplus, granary, blocked| food_turn(stored, surplus, 10, 0, granary, blocked);
        assert_eq!(turn(17, 2, false, false), Stored(19));
        assert_eq!(turn(18, 2, false, false), Grew(0), "exactly the box grows");
        assert_eq!(turn(19, 5, false, false), Grew(0), "overflow is lost");
        assert_eq!(turn(18, 2, true, false), Grew(10));
        assert_eq!(food_turn(38, 2, 10, 1, true, false), Grew(20));
        assert_eq!(food_turn(58, 9, 10, 2, true, false), Grew(30));
        assert_eq!(turn(18, 2, true, true), Stored(20));
        assert_eq!(turn(19, 9, false, true), Stored(20));
    }

    #[test]
    fn starvation_needs_a_negative_total() {
        use FoodTurn::*;
        assert_eq!(food_turn(3, -3, 10, 0, false, false), Stored(0));
        assert_eq!(food_turn(3, -4, 10, 0, false, false), Starved);
        assert_eq!(food_turn(0, -1, 10, 0, true, false), Starved);
    }

    #[test]
    fn the_treasury_cells_always_sum_to_the_gold() {
        for clock in [0u32, 1, 12_345, 0xD430, 0xD431, 4_000_000_000] {
            for n in [0, 1, 7, 100, 123_456] {
                let (a, b) = treasury_cells(n, clock);
                assert_eq!(a + b, n, "n = {n}, clock = {clock}");
            }
            let (a, b) = treasury_cells(-5, clock);
            assert_eq!(a + b, 0, "a negative write stores nothing");
        }
    }

    #[test]
    fn despotism_supports_four_units_per_city_whatever_its_size() {
        let sizes = [
            size_class(3, 6, 12),
            size_class(8, 6, 12),
            size_class(14, 6, 12),
        ];
        let s = support_terms(0, [4, 4, 4], 1, sizes);
        assert_eq!(s, Support { per_unit: 1, free: 12 });
        let s = support_terms(0, [5, 2, 1], 3, [0, 1, 2]);
        assert_eq!(s, Support { per_unit: 3, free: 8 });
    }

    #[test]
    fn a_base_of_minus_one_charges_nothing_and_the_allowance_never_dips() {
        let s = support_terms(-1, [4, 4, 4], 1, [0, 0, 0]);
        assert_eq!(s, Support { per_unit: 0, free: 0 });
        let s = support_terms(-3, [1, 1, 1], 2, [0, 0]);
        assert_eq!(s.free, 0, "base -3 and +2 clamps to 0");
        assert_eq!(support_terms(0, [4, 4, 4], 1, []).free, 0);
    }

    #[test]
    fn units_cost_gold_only_past_the_allowance_and_only_with_cities() {
        assert_eq!(unit_support_charge(0, 40, 0, 0, 1), 0, "no city, no bill");
        assert_eq!(unit_support_charge(2, 8, 0, 8, 1), 0);
        assert_eq!(unit_support_charge(2, 9, 0, 8, 1), 1);
        assert_eq!(unit_support_charge(2, 12, 2, 8, 3), 6, "exempt units are free");
        assert_eq!(unit_support_charge(1, 1, 0, 4, 1), 0, "never negative");
    }

    #[test]
    fn a_bill_takes_the_whole_treasury_when_it_cannot_be_met() {
        assert_eq!(pay_bill(10, 4), (6, false));
        assert_eq!(pay_bill(4, 4), (0, false));
        assert_eq!(pay_bill(3, 4), (0, true));
        assert_eq!(pay_bill(0, 0), (0, false));
    }

    #[test]
    fn only_a_zero_roll_pays_units_first() {
        let first = (0..4).filter(|&r| units_first(r)).count();
        assert_eq!(first, 1);
        assert!(units_first(0) && !units_first(3));
    }

    #[test]
    fn improvement_costs_scale_with_x_and_halve_on_a_trait() {
        assert_eq!(improvement_cost(30, 10, false, None), 300);
        assert_eq!(improvement_cost(30, 10, true, None), 150);
        assert_eq!(improvement_cost(0, 10, false, None), 1, "at least one");
        assert_eq!(improvement_cost(15, 1, true, Some(3)), 7 * 3);
        assert_eq!(sale_price(300, 10), 30);
        assert_eq!(sale_price(300, 20), 15);
    }

    #[test]
    fn culture_levels_follow_powers() {
        assert_eq!(culture_level(0, 10), 1);
        assert_eq!(culture_level(9, 10), 1);
        assert_eq!(culture_level(10, 10), 2);
        assert_eq!(culture_level(99, 10), 2);
        assert_eq!(culture_level(100, 10), 3);
        assert_eq!(culture_level(10_000_000, 10), 6);
        assert_eq!(culture_level(u32::MAX, 10), 6);
    }

    #[test]
    fn split_rounds_like_magic_divider() {
        assert_eq!(split_share(100, 5), 50);
        assert_eq!(split_share(7, 3), 2);
        assert_eq!(split_share(1, 1), 0);
        assert_eq!(split_share(100, 0), 0);
        assert_eq!(split_share(100, 10), 100);
    }

    #[test]
    fn rank_prime_folds_at_r() {
        assert_eq!(rank_prime(0, 8), 0);
        assert_eq!(rank_prime(7, 8), 7);
        assert_eq!(rank_prime(8, 8), 8);
        assert_eq!(rank_prime(9, 8), 10);
        assert_eq!(rank_prime(12, 8), 16);
    }

    #[test]
    fn trait_bits_match_the_shipped_civilizations() {
        let (romans, egyptians, greeks) = (0x03, 0x30, 0x0A);
        assert!(has_trait(romans, trait_bit::MILITARISTIC));
        assert!(has_trait(romans, trait_bit::COMMERCIAL));
        assert!(!has_trait(romans, trait_bit::SCIENTIFIC));
        assert!(has_trait(egyptians, trait_bit::RELIGIOUS));
        assert!(has_trait(egyptians, trait_bit::INDUSTRIOUS));
        assert!(has_trait(greeks, trait_bit::SCIENTIFIC));
        assert!(has_trait(greeks, trait_bit::COMMERCIAL));
        assert!(!has_trait(greeks, trait_bit::MILITARISTIC));
        let dutch = (1 << trait_bit::SEAFARING) | (1 << trait_bit::AGRICULTURAL);
        assert_eq!(dutch, 0xC0);
        assert!(has_trait(dutch, 7) && has_trait(dutch, 6) && !has_trait(dutch, 0));
        assert!(has_trait(1, 32));
    }

    #[test]
    fn the_trait_discount_pairs_each_trait_with_its_building_bit() {
        let bit = |t: u32| 1u32 << t;
        assert!(trait_discount(0x2, bit(trait_bit::MILITARISTIC)));
        assert!(trait_discount(0x20, bit(trait_bit::SCIENTIFIC)));
        assert!(trait_discount(0x100, bit(trait_bit::RELIGIOUS)));
        assert!(trait_discount(0x400, bit(trait_bit::AGRICULTURAL)));
        assert!(trait_discount(0x800, bit(trait_bit::SEAFARING)));
        assert!(!trait_discount(0x2, bit(trait_bit::SCIENTIFIC)));
        assert!(!trait_discount(0x800, bit(trait_bit::MILITARISTIC)));
        assert!(!trait_discount(0x2, 0));
        let harbor = 0x2 | 0x40 | 0x800;
        assert!(trait_discount(harbor, bit(trait_bit::MILITARISTIC)));
        assert!(trait_discount(harbor, bit(trait_bit::SEAFARING)));
        assert!(!trait_discount(harbor, bit(trait_bit::COMMERCIAL)));
        assert!(!trait_discount(0x2 | 0x4, bit(trait_bit::MILITARISTIC)));
        assert!(!trait_discount(0x100 | 0x8, bit(trait_bit::RELIGIOUS)));
    }

    #[test]
    fn the_palace_costs_three_to_ten_times_as_the_empire_grows() {
        assert_eq!(palace_cost_factor(0, 20), 3);
        assert_eq!(palace_cost_factor(10, 20), 3);
        assert_eq!(palace_cost_factor(20, 20), 6);
        assert_eq!(palace_cost_factor(33, 20), 9);
        assert_eq!(palace_cost_factor(34, 20), 10);
        assert_eq!(palace_cost_factor(99, 20), 10);
        assert_eq!(palace_cost_factor(14, 14), 6);
    }

    fn standard(class: i32) -> OcnInputs {
        OcnInputs {
            world_base: 20,
            palace_like: 0,
            class,
            commercial: false,
            human: true,
            game_level: 2,
            percent: 100,
        }
    }

    #[test]
    fn the_government_class_adds_an_eighth_a_sixteenth_nothing_or_double() {
        let ocn = |class| optimal_city_number(&standard(class));
        assert_eq!([ocn(0), ocn(1), ocn(2), ocn(3), ocn(4), ocn(5)], [22, 22, 21, 20, 20, 60]);
        assert_eq!(ocn(6), 20);
        assert_eq!(ocn(-1), 20);
    }

    #[test]
    fn palace_like_buildings_add_three_eighths_each_but_three_whole_when_communal() {
        let mut i = standard(3);
        i.palace_like = 1;
        assert_eq!(optimal_city_number(&i), 20 + 60 / 8);
        i.palace_like = 2;
        assert_eq!(optimal_city_number(&i), 20 + 120 / 8);
        let mut c = standard(COMMUNAL_CLASS);
        c.palace_like = 1;
        assert_eq!(optimal_city_number(&c), 20 + 60 + 40);
    }

    #[test]
    fn a_commercial_civ_gains_a_quarter() {
        let mut i = standard(3);
        i.commercial = true;
        assert_eq!(optimal_city_number(&i), 25);
        i.world_base = 14;
        assert_eq!(optimal_city_number(&i), 17);
    }

    #[test]
    fn only_the_ai_gets_the_game_level_bonus() {
        let mut i = standard(3);
        for (level, extra) in [(2, 0), (3, 2), (4, 5), (5, 10), (7, 10)] {
            i.game_level = level;
            i.human = true;
            assert_eq!(optimal_city_number(&i), 20, "human, level {level}");
            i.human = false;
            assert_eq!(optimal_city_number(&i), 20 + extra, "AI, level {level}");
        }
    }

    #[test]
    fn the_difficulty_percentage_scales_last_and_the_floor_is_one() {
        let mut i = standard(0);
        i.percent = 90;
        assert_eq!(optimal_city_number(&i), 22 * 90 / 100);
        i.percent = 50;
        assert_eq!(optimal_city_number(&i), 11);
        i.world_base = 1;
        i.class = 4;
        assert_eq!(optimal_city_number(&i), 1);
        i.world_base = 0;
        assert_eq!(optimal_city_number(&i), 1);
    }
}
