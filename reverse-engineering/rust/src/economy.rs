//! Culture, corruption, commerce split, city growth and the gold side of
//! a turn (unit support, upkeep, the treasury).
//!
//! See `../economy.md`. The functions mirror parent-verified instruction
//! sequences; values are non-negative game quantities so the
//! `cdq`-and-shift truncating divisions match Rust `/`. The growth and
//! gold functions (from [`size_class`] on) were read in this pass with
//! `r2`; each doc comment names the address it mirrors and says what is
//! still a HYPOTHESIS.

/// Culture level (`0x4B0C60`): count of powers `base^k <= accum`,
/// starting at 1, capped at 6 (`cmp ebx,6`).
pub fn culture_level(accum: u32, base: u32) -> u32 {
    let mut level = 1;
    let mut power = base;
    while accum >= power && level < 6 {
        power = power.wrapping_mul(base);
        level += 1;
    }
    level
}

/// Commerce split (`0x4B0844`/`0x4B0864`): `(net * rate + 5) / 10`
/// via the `0x66666667` magic divider. `rate` is tenths (0–10).
pub fn split_share(net: i32, rate: i32) -> i32 {
    (net.wrapping_mul(rate).wrapping_add(5)) / 10
}

/// Corruption rank prime (`0x4B18D9`): `rank >= R ? 2*rank - R : rank`.
pub fn rank_prime(rank: i32, r: i32) -> i32 {
    if rank >= r {
        rank.wrapping_mul(2).wrapping_sub(r)
    } else {
        rank
    }
}

/// Largest population of a town: RULE body `+0x11C`, global `[0x9C72E4]`
/// (shipped 6). The RULE reader `0x5E78E0` stores it directly (it is the
/// address-to-body map of `../combat.md` section 9: `0x9C72E4 - 0x9C71E4 =
/// 0x100`, body = object offset + `0x1C`), and the decoded `conquests.biq`
/// has 6 there; the Civilopedia agrees ("can grow beyond population six").
pub const TOWN_MAX: i32 = 6;

/// Largest population of a city: RULE body `+0x120`, global `[0x9C72E8]`
/// (shipped 12). Same provenance as [`TOWN_MAX`].
pub const CITY_MAX: i32 = 12;

/// `0x4B1DC0`: freshwater (`0x4B1E7F`) or an active size-level-2
/// improvement bypasses the town gate. The level-3 gate at `0x4B1EBF`
/// never tests freshwater. Stock RULE limits are six and twelve.
pub fn growth_limit(fresh_water: bool, level_2: bool, level_3: bool) -> i32 {
    if !fresh_water && !level_2 {
        TOWN_MAX
    } else if !level_3 {
        CITY_MAX
    } else {
        i32::MAX
    }
}

/// City size class (`0x427540`): 0 town, 1 city, 2 metropolis.
/// `pop > city_max` is 2, else `pop > town_max` is 1 (`setg`), else 0.
pub fn size_class(population: i32, town_max: i32, city_max: i32) -> i32 {
    if population > city_max {
        2
    } else {
        (population > town_max) as i32
    }
}

/// The box unit X (`0x5660E0`): 10 for a human player, otherwise the
/// difficulty's own number (`DIFF[level].+0x68`); halved, truncating
/// toward zero (`cdq/sub/sar`), when `[0xA5267C] & 0x200` (that flag's
/// name is open); at least 1.
pub fn box_unit(human: bool, difficulty_x: i32, halved: bool) -> i32 {
    let x = if human { 10 } else { difficulty_x };
    let x = if halved { x / 2 } else { x };
    x.max(1)
}

/// The food box (`0x4B21C6..0x4B21DA`): `2 * X * (class + 1)`, 20 / 40 /
/// 60 for a human.
pub fn food_box(x: i32, class: i32) -> i32 {
    2 * x * (class + 1)
}

/// What one turn of the food routine (`0x4B2030`) does to a city.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoodTurn {
    /// Stored food plus surplus is negative (`jns 0x4B219D` not taken):
    /// the store is 0 and a citizen goes.
    Starved,
    /// No growth: this is the new store (a blocked city keeps a full box).
    Stored(i32),
    /// A citizen is added and this is the new store: half the box with a
    /// Granary-flagged improvement (BLDG `+0xEC & 0x200`), else 0. The old
    /// total is not carried over.
    Grew(i32),
}

/// One turn of the food routine. `granary` is "the city has an active
/// improvement with BLDG `+0xEC & 0x200`", `blocked` is "the growth gate
/// `0x4B1DC0` returned a building index". Growth needs the *new* total to
/// reach the box (`cmp edi, ebp; jl`), and the class is the pre-growth one.
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

/// How the treasury is stored (`0x4C2350`): two cells whose sum is the
/// gold, split afresh from the clock on every write. `n <= 0` stores a
/// sum of 0; `n > 0` stores `clock % n - 0x3039` and the rest. A tamper
/// guard, not a game rule.
pub fn treasury_cells(n: i32, clock: u32) -> (i32, i32) {
    if n <= 0 {
        let a = (clock % 0xD431) as i32 - 0x8235;
        (a, -a)
    } else {
        let a = (clock % n as u32) as i32 - 0x3039;
        (a, n - a)
    }
}

/// What a government charges for units (`0x53A960`, one GOVT record):
/// the gold per unit past the free ones and how many are free.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Support {
    /// GOVT `+0x1E0`; 0 when the base is -1.
    pub per_unit: i32,
    /// `max(0, base + sum of per-class allowances)`, civ-wide.
    pub free: i32,
}

/// The unit-support terms of a government (`0x53A960`). `base` is GOVT
/// `+0x1D0`, `per_class` the dwords at `+0x1D4 / +0x1D8 / +0x1DC` (town,
/// city, metropolis), `per_unit` `+0x1E0`, `classes` the size class of
/// each of the player's cities. A base of -1 (the shipped first row) means
/// nothing is charged: `per_unit` 0, no city loop, `free` 0.
pub fn support_terms(
    base: i32,
    per_class: [i32; 3],
    per_unit: i32,
    classes: impl IntoIterator<Item = i32>,
) -> Support {
    if base == -1 {
        return Support {
            per_unit: 0,
            free: 0,
        };
    }
    let sum: i32 = classes.into_iter().map(|c| per_class[c as usize]).sum();
    Support {
        per_unit,
        free: (base + sum).max(0),
    }
}

/// Gold owed for units (`0x55DFD0`): nothing while the player has no
/// cities (`[player+0x194] == 0`, `0x55DFE7`), else `(units - exempt -
/// free) * per_unit`, and nothing when that is not positive. `exempt` is
/// `0x55D030`: units of another civ (HYPOTHESIS) and units whose PRTO
/// `+0xD0` ("Req. Support") is 0, the Leaders.
pub fn unit_support_charge(cities: i32, units: i32, exempt: i32, free: i32, per_unit: i32) -> i32 {
    if cities == 0 {
        return 0;
    }
    ((units - exempt - free) * per_unit).max(0)
}

/// Pay a bill from the treasury (`0x55E067` for units, `0x560C23` for
/// upkeep): the new treasury and whether the bill went unmet. Unmet, the
/// treasury is 0 (the whole of it was paid) and the player loses one unit
/// (`NOSUPPORT`) or one improvement (`MAINTSHORT`).
pub fn pay_bill(treasury: i32, bill: i32) -> (i32, bool) {
    if bill <= treasury {
        (treasury - bill, false)
    } else {
        (0, true)
    }
}

/// Whether a turn pays units before upkeep (`0x560BFC`): the gameplay
/// `Random` (`0xA526B4`) rolls `next(4)` and only a 0 does, so one turn in
/// four.
pub fn units_first(roll: u32) -> bool {
    roll == 0
}

/// Bit numbers of the civ trait mask (RACE `+0x948`).
///
/// `RACE.vtable[0]` (`0x53A080`, `ret 4`) is `mask >> bit & 1`; the callers
/// pass these numbers (the promotion die of `combat.md` 6.2 passes 0,
/// `0x5676C0` passes 1, `0x569FE0` passes 0, 4, 6, 7 and 3). **Verified:** the mask of every one of
/// the 31 playable civilizations in `conquests.biq` (tail dword 13, which the
/// `biq` crate calls `unique_unit`) equals the OR of the two traits the
/// Civilopedia names for it.
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

/// `RACE.vtable[0]` (`0x53A080`): bit `bit` of the trait mask. The shift
/// count is taken modulo 32 like the CPU's `shl edx, cl`.
pub fn has_trait(mask: u32, bit: u32) -> bool {
    mask & (1 << (bit & 31)) != 0
}

/// The (trait, BLDG `+0xF0` bit) pairs `0x569FE0` probes, in code order
/// (`0x56A07C`, `0x56A0B7`, `0x56A0F6`, `0x56A131`, `0x56A16C`): Militaristic
/// `0x2`, Religious `0x100`, Agricultural `0x400`, Seafaring `0x800`,
/// Scientific `0x20`. **Verified** against the shipped rows: `0x2` is on
/// Barracks, Walls, SAM Missile Battery, Coastal Fortress and Airport;
/// `0x20` on Library, University and Research Lab; `0x100` on Temple and
/// Cathedral; `0x400` on Aqueduct, Recycling Center and Solar Plant; `0x800`
/// on Harbor, Offshore Platform and Commercial Dock. (Commercial has no
/// probe; `0x40` marks Marketplace, Bank and Stock Exchange.)
pub const TRAIT_DISCOUNTS: [(u32, u32); 5] = [
    (trait_bit::MILITARISTIC, 0x2),
    (trait_bit::RELIGIOUS, 0x100),
    (trait_bit::AGRICULTURAL, 0x400),
    (trait_bit::SEAFARING, 0x800),
    (trait_bit::SCIENTIFIC, 0x20),
];

/// Does a civ with trait mask `traits` get the half price on a building
/// whose BLDG word at memory `+0xF0` is `flags`? (`0x56A059..0x56A19D`.)
/// Great wonders (bit 4) and small wonders (bit 8) never do: the whole
/// block is skipped for them. For other buildings one matching pair is
/// enough; several do not stack.
pub fn trait_discount(flags: u32, traits: u32) -> bool {
    if flags & 0xC != 0 {
        return false;
    }
    TRAIT_DISCOUNTS
        .iter()
        .any(|&(bit, mask)| has_trait(traits, bit) && flags & mask != 0)
}

/// The multiplier a Center-of-Empire building (BLDG memory `+0xEC & 1`, the
/// Palace) gets in `0x569FE0` (`0x56A1B2..0x56A1F0`): `6 * cities / B`
/// with the signed `idiv`, clamped to `3..=10`, where `B` is the world
/// size's optimal number of cities (`WSIZ` memory `+4`).
pub fn palace_cost_factor(cities: i32, optimal_cities: i32) -> i32 {
    (6 * cities / optimal_cities).clamp(3, 10)
}

/// An improvement's cost in the game's units (`0x569FE0`): BLDG `+0x94`
/// times X ([`box_unit`] with the human mask or the `flag` argument as
/// "human"), halved once when [`trait_discount`] holds, times
/// [`palace_cost_factor`] for a Center-of-Empire building, at least 1.
/// The multiplications wrap like `imul`; the halving truncates toward zero.
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

/// Gold a sold improvement fetches (`0x4B32F0`): [`improvement_cost`]
/// divided by `[0x9C7268]`, RULE "Shield Cost Per Gold" (body `+0xA4`, 4 in
/// `conquests.biq`: [`crate::city::SHIELDS_PER_GOLD`]), so a 60-shield
/// improvement sells for 15 gold. The same divisor turns Wealth shields into
/// gold ([`crate::city::wealth_gold`]).
pub fn sale_price(cost: i32, divisor: i32) -> i32 {
    cost / divisor
}

/// The corruption class that makes a government "communal": GOVT `+0x18C`
/// (the editor's "Corruption and Waste" level, which `conquests.biq` stores
/// at row body `+0x178`) is 0 minimal, 1 nuisance, 2 problematic, 3 rampant,
/// 4 catastrophic, 5 communal. Shipped: Democracy 0, Republic and Fascism 1,
/// Monarchy and Feudalism 2, Despotism 3, Anarchy 4, Communism 5.
pub const COMMUNAL_CLASS: i32 = 5;

/// Everything the optimal city number reads (`0x5676C0`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OcnInputs {
    /// "Optimal number of cities" of the world size: `WSIZ[[0x9C73A0]]` at
    /// memory `+4` (stride 84, global `[0x9C7330]`). Shipped 14 / 17 / 20 /
    /// 28 / 36 for Tiny to Huge.
    pub world_base: i32,
    /// `0x55AA10(player, 0x20, 0)`: the player's wonders whose BLDG `+0xF4`
    /// has bit `0x20` (Forbidden Palace; Secret Police HQ, which also needs
    /// GOVT 3), allowed by their required government, and standing in a
    /// city of the player.
    pub palace_like: i32,
    /// GOVT `+0x18C` of the player's government (see [`COMMUNAL_CLASS`]).
    pub class: i32,
    /// `RACE.vtable[0](1)`: the civ has [`trait_bit::COMMERCIAL`].
    pub commercial: bool,
    /// The player's bit is set in the human mask `[0xA526BC]`.
    pub human: bool,
    /// The game difficulty `[0xA52684]` (0 Chieftain to 7 Sid); read for an
    /// AI player only.
    pub game_level: i32,
    /// `DIFF[player +0x30]` at memory `+0x6C` (row body `+0x68`): shipped 100,
    /// 95, 90, 85, 80, 70, 60, 50 from Chieftain to Sid.
    pub percent: i32,
}

/// The optimal city number, OCN (`0x5676C0`, `this` = the player).
///
/// Starting from the world size's base `B`:
///
/// * plus `3 * palace_like * B / d`, with `d` 1 for a communal government
///   and 8 otherwise (`idiv`, truncating);
/// * plus `B / 4` for a Commercial civ;
/// * plus `B / 8` for corruption class 0 or 1, `B / 16` for class 2,
///   nothing for 3 or 4, and `2 * B` for class 5 (jump table `0x567818`);
/// * for an AI player only, plus `B / 2` above game level 4, `B / 4` at
///   level 4, `B / 8` at level 3;
///
/// then scaled by the difficulty percentage (`imul` and the `0x51EB851F`
/// divide by 100, truncating) and floored at 1. Every `/` above is the
/// `cdq`-mask-`sar` truncation toward zero, which is Rust's `/`.
///
/// The result is the "optimal" count of cities; the AI keeps a flipped city
/// while it owns fewer than twice this many (`capture::ai_accepts_city`).
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

#[cfg(test)]
mod tests {
    #[test]
    fn freshwater_bypasses_only_the_aqueduct_gate() {
        assert_eq!(super::growth_limit(false, false, false), 6);
        assert_eq!(super::growth_limit(false, false, true), 6);
        assert_eq!(super::growth_limit(true, false, false), 12);
        assert_eq!(super::growth_limit(false, true, false), 12);
        assert_eq!(super::growth_limit(true, false, true), i32::MAX);
        assert_eq!(super::growth_limit(false, true, true), i32::MAX);
    }

    use super::*;

    #[test]
    fn size_classes_break_after_the_caps() {
        let class = |p| size_class(p, TOWN_MAX, CITY_MAX);
        assert_eq!(
            [class(1), class(6), class(7), class(12), class(13)],
            [0, 0, 1, 1, 2]
        );
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
        // A Granary keeps half the box of the size it grew from.
        assert_eq!(turn(18, 2, true, false), Grew(10));
        assert_eq!(food_turn(38, 2, 10, 1, true, false), Grew(20));
        assert_eq!(food_turn(58, 9, 10, 2, true, false), Grew(30));
        // The gate keeps the store full and the city small.
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
        // Row 1 of the shipped GOVT table: base 0, 4 / 4 / 4, one gold.
        let sizes = [
            size_class(3, 6, 12),
            size_class(8, 6, 12),
            size_class(14, 6, 12),
        ];
        let s = support_terms(0, [4, 4, 4], 1, sizes);
        assert_eq!(
            s,
            Support {
                per_unit: 1,
                free: 12
            }
        );
        // Row 7: 5 / 2 / 1 and three gold: the class picks the term.
        let s = support_terms(0, [5, 2, 1], 3, [0, 1, 2]);
        assert_eq!(
            s,
            Support {
                per_unit: 3,
                free: 8
            }
        );
    }

    #[test]
    fn a_base_of_minus_one_charges_nothing_and_the_allowance_never_dips() {
        let s = support_terms(-1, [4, 4, 4], 1, [0, 0, 0]);
        assert_eq!(
            s,
            Support {
                per_unit: 0,
                free: 0
            }
        );
        let s = support_terms(-3, [1, 1, 1], 2, [0, 0]);
        assert_eq!(s.free, 0, "base -3 and +2 clamps to 0");
        assert_eq!(support_terms(0, [4, 4, 4], 1, []).free, 0);
    }

    #[test]
    fn units_cost_gold_only_past_the_allowance_and_only_with_cities() {
        assert_eq!(unit_support_charge(0, 40, 0, 0, 1), 0, "no city, no bill");
        assert_eq!(unit_support_charge(2, 8, 0, 8, 1), 0);
        assert_eq!(unit_support_charge(2, 9, 0, 8, 1), 1);
        assert_eq!(
            unit_support_charge(2, 12, 2, 8, 3),
            6,
            "exempt units are free"
        );
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
        // The halving comes before the Palace factor and truncates.
        assert_eq!(improvement_cost(15, 1, true, Some(3)), 7 * 3);
        assert_eq!(sale_price(300, 10), 30);
        assert_eq!(sale_price(300, 20), 15);
    }

    #[test]
    fn culture_levels_follow_powers() {
        // Base 10: powers 10/100/1000/...; level counts powers <= accum.
        assert_eq!(culture_level(0, 10), 1);
        assert_eq!(culture_level(9, 10), 1);
        assert_eq!(culture_level(10, 10), 2);
        assert_eq!(culture_level(99, 10), 2);
        assert_eq!(culture_level(100, 10), 3);
        assert_eq!(culture_level(10_000_000, 10), 6); // capped
        assert_eq!(culture_level(u32::MAX, 10), 6);
    }

    #[test]
    fn split_rounds_like_magic_divider() {
        assert_eq!(split_share(100, 5), 50);
        assert_eq!(split_share(7, 3), 2); // (21+5)/10
        assert_eq!(split_share(1, 1), 0); // (1+5)/10
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
        // Tail dword 13 of the RACE rows in `conquests.biq`: Romans are
        // militaristic and commercial, Egyptians religious and industrious,
        // Greeks scientific and commercial, Russians scientific and
        // expansionist, the Dutch seafaring and agricultural.
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
        // The shift count wraps at 32 like `shl edx, cl`.
        assert!(has_trait(1, 32));
    }

    #[test]
    fn the_trait_discount_pairs_each_trait_with_its_building_bit() {
        let bit = |t: u32| 1u32 << t;
        // One pair each: Barracks-like, Library-like, Temple-like,
        // Aqueduct-like and Harbor-like buildings.
        assert!(trait_discount(0x2, bit(trait_bit::MILITARISTIC)));
        assert!(trait_discount(0x20, bit(trait_bit::SCIENTIFIC)));
        assert!(trait_discount(0x100, bit(trait_bit::RELIGIOUS)));
        assert!(trait_discount(0x400, bit(trait_bit::AGRICULTURAL)));
        assert!(trait_discount(0x800, bit(trait_bit::SEAFARING)));
        // A trait only discounts its own bit.
        assert!(!trait_discount(0x2, bit(trait_bit::SCIENTIFIC)));
        assert!(!trait_discount(0x800, bit(trait_bit::MILITARISTIC)));
        assert!(!trait_discount(0x2, 0));
        // The Harbor carries 0x2, 0x40 and 0x800: Militaristic or Seafaring
        // halves it (once, never to a quarter), Commercial does not.
        let harbor = 0x2 | 0x40 | 0x800;
        assert!(trait_discount(harbor, bit(trait_bit::MILITARISTIC)));
        assert!(trait_discount(harbor, bit(trait_bit::SEAFARING)));
        assert!(!trait_discount(harbor, bit(trait_bit::COMMERCIAL)));
        // Wonders never: Sun Tzu has 0x2 and the great-wonder bit 0x4, the
        // Heroic Epic a small-wonder bit 0x8.
        assert!(!trait_discount(0x2 | 0x4, bit(trait_bit::MILITARISTIC)));
        assert!(!trait_discount(0x100 | 0x8, bit(trait_bit::RELIGIOUS)));
    }

    #[test]
    fn the_palace_costs_three_to_ten_times_as_the_empire_grows() {
        // Standard map, B = 20: 6 * cities / 20, truncated, clamped.
        assert_eq!(palace_cost_factor(0, 20), 3);
        assert_eq!(palace_cost_factor(10, 20), 3);
        assert_eq!(palace_cost_factor(20, 20), 6);
        assert_eq!(palace_cost_factor(33, 20), 9);
        assert_eq!(palace_cost_factor(34, 20), 10);
        assert_eq!(palace_cost_factor(99, 20), 10);
        // A tiny map (B = 14) grows the factor faster.
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
        // Standard map, base 20: classes 0 and 1 add 20/8 = 2, class 2 adds
        // 20/16 = 1, classes 3 and 4 nothing, the communal class adds 40.
        assert_eq!(
            [ocn(0), ocn(1), ocn(2), ocn(3), ocn(4), ocn(5)],
            [22, 22, 21, 20, 20, 60]
        );
        // A class outside 0..=5 skips the jump table.
        assert_eq!(ocn(6), 20);
        assert_eq!(ocn(-1), 20);
    }

    #[test]
    fn palace_like_buildings_add_three_eighths_each_but_three_whole_when_communal() {
        let mut i = standard(3);
        i.palace_like = 1;
        assert_eq!(optimal_city_number(&i), 20 + 60 / 8); // 27
        i.palace_like = 2;
        assert_eq!(optimal_city_number(&i), 20 + 120 / 8); // 35
        let mut c = standard(COMMUNAL_CLASS);
        c.palace_like = 1;
        assert_eq!(optimal_city_number(&c), 20 + 60 + 40); // d = 1
    }

    #[test]
    fn a_commercial_civ_gains_a_quarter() {
        let mut i = standard(3);
        i.commercial = true;
        assert_eq!(optimal_city_number(&i), 25);
        i.world_base = 14; // Tiny: 14 / 4 truncates to 3
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
        let mut i = standard(0); // 22 before scaling
        i.percent = 90; // Regent
        assert_eq!(optimal_city_number(&i), 22 * 90 / 100); // 19
        i.percent = 50; // Sid
        assert_eq!(optimal_city_number(&i), 11);
        i.world_base = 1;
        i.class = 4;
        assert_eq!(optimal_city_number(&i), 1); // 1 * 50 / 100 = 0, floored
        i.world_base = 0;
        assert_eq!(optimal_city_number(&i), 1);
    }
}
