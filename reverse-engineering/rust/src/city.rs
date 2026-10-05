//! The city's per-turn totals: from the worked tiles to food, shields and
//! commerce.
//!
//! Findings, addresses and the open list are in `../yields.md` (sections 5 to
//! 8); [`crate::yields`] is the tile level that feeds this one. The game keeps
//! three running sums in the city, `[city+0x1C8]` food, `[city+0x1CC]` shields
//! and `[city+0x1D0]` commerce, one term per worked tile (`0x4B0470` adds or
//! removes a tile, `0x4B0E80` rebuilds all three from scratch), and derives
//! everything else from them in a fixed chain:
//!
//! | step | binary | here |
//! |---|---|---|
//! | food eaten and surplus | `0x4B0540` (and inline copies at `0x4B1060`, `0x4B10F0`, `0x4B0E80`) | [`food_eaten`], [`food_surplus`] |
//! | shields: waste, then the building multiplier | `0x4B05D0` | [`net_shields`] |
//! | commerce: tourists, waste, the three-way split, the multipliers, Wealth, specialists | `0x4B07C0`, `0x4B0710`, `0x4B0AC0` | [`tourist_gold`], [`commerce_split`], [`wealth_gold`], [`specialist_streams`] |
//! | happiness | `0x4BCFF0` | [`crate::happiness::recompute`] |
//!
//! The corruption and waste routine `0x4B1190` is the *input* `lost` of the
//! shield and commerce steps; it is described in `../economy.md` and not
//! reimplemented here.

use crate::economy::split_share;

/// RULE `Shield Cost Per Gold` (the editor label), global `[0x9C7268]`, body
/// `+0xA4` of the RULE record: 4 in `conquests.biq`. It converts shields to
/// gold for Wealth ([`wealth_gold`], `0x4B0AC0`) and divides the price of a
/// sold improvement (`0x4B32F0`).
pub const SHIELDS_PER_GOLD: i32 = 4;

/// TECH flag bit 12, "Doubles Effect of (Wealth) Improvement" (Economics),
/// the mask `Player::knowsTechWithFlags` (`0x561480`) is asked for at
/// `0x4B0B08`.
pub const DOUBLE_WEALTH_TECH_FLAG: u32 = 0x1000;

/// Bits of BLDG `+0xEC` (the improvement flags) that this chain reads.
pub mod building_flag {
    /// "+50% Research Output" (Library, University, Research Lab).
    pub const RESEARCH_BONUS: u32 = 0x4;
    /// "+50% Luxury Output". No row of the shipped `conquests.biq` has it.
    pub const LUXURY_BONUS: u32 = 0x8;
    /// "+50% Tax Output" (Marketplace, Bank, Stock Exchange).
    pub const TAX_BONUS: u32 = 0x10;
    /// "Replaces All Impr. with this Flag Checked": the power plants.
    pub const REPLACES_ALL: u32 = 0x2000;
    /// "Capitalization": the Wealth improvement.
    pub const CAPITALIZATION: u32 = 0x8_0000;
}

/// Bits of BLDG `+0xF8` (the wonder flags) that this chain reads.
pub mod wonder_flag {
    /// "Doubles Research Output" (Copernicus, Newton's, SETI): counted in
    /// the city with `Player::countWonderFlag(0x10, city)` (`0x4B0988`).
    pub const DOUBLES_RESEARCH: u32 = 0x10;
    /// "Tourist Attraction" (every ancient wonder): `0x4B0728` tests it.
    pub const TOURIST_ATTRACTION: u32 = 0x2_0000;
}

/// Food the city eats (`0x4B0540`): in civil disorder (`[city+0x30] & 1`)
/// everything it makes, so the surplus is 0; otherwise
/// `(size - resisters) * per_citizen`, where `resisters` counts the citizens
/// whose byte `+0x20` is set (`0x4BB2A0(city, -1)`) and `per_citizen` is RULE
/// `Food Consumption/Citizen`, `[0x9C72B4]` (2). A resister eats nothing.
pub fn food_eaten(
    size: i32,
    resisters: i32,
    per_citizen: i32,
    in_disorder: bool,
    gross_food: i32,
) -> i32 {
    if in_disorder {
        gross_food
    } else {
        (size - resisters) * per_citizen
    }
}

/// The food surplus `[city+0x250]` (`0x4B0540..0x4B05C2`): what the food box
/// receives each turn ([`crate::economy::food_turn`]).
pub fn food_surplus(gross_food: i32, eaten: i32) -> i32 {
    gross_food - eaten
}

/// One building the city has, as the shield multiplier sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShieldBuilding {
    /// BLDG `+0xD0` (the editor's "Production"), in quarters of the net: 2 is
    /// +50% (shipped: Factory 2, Manufacturing Plant 2, the Coal, Hydro and
    /// Solar Plants 2, the Nuclear Plant 4, Iron Works 4).
    pub bonus: i32,
    /// BLDG `+0xEC & 0x2000` ([`building_flag::REPLACES_ALL`]).
    pub replaces_all: bool,
    /// For a [`replaces_all`](Self::replaces_all) building: the city has the
    /// improvement in BLDG `+0x90` (the required improvement, the Factory).
    pub prerequisite_present: bool,
}

/// Net shield production `[city+0x254]` (`0x4B05D0`).
///
/// `net = gross - lost` (`lost` is `0x4B1190(gross, 1)`, stored in
/// `[city+0x248]`); every building the city has that is not obsolete (BLDG
/// `+0xE0` is -1 or a tech the owner lacks) contributes: an ordinary building
/// adds its bonus to `a`, a power-plant style building (flag `0x2000`) counts
/// only with its prerequisite and only the **largest** of them is used (`b`),
/// so a better plant replaces a worse one. The result is
/// `(4 + sum + b) * net / 4`, truncating toward zero.
pub fn net_shields(
    gross: i32,
    lost: i32,
    buildings: impl IntoIterator<Item = ShieldBuilding>,
) -> i32 {
    let net = gross - lost;
    let mut a = 4;
    let mut b = 0;
    for x in buildings {
        if x.replaces_all {
            if x.prerequisite_present {
                b = b.max(x.bonus);
            }
        } else {
            a += x.bonus;
        }
    }
    (a + b) * net / 4
}

/// Gold per turn from a tourist attraction (`0x4B0710`): zero unless the
/// building has wonder flag [`wonder_flag::TOURIST_ATTRACTION`] (BLDG
/// `+0xF8 & 0x20000`) and the city has it. `age` is what
/// `0x4C2420(city+0xA8, building)` returns (HYPOTHESIS: the years since the
/// wonder was built; the record is not decoded, see `../yields.md`).
///
/// Seven steps, the Civilopedia chart ("1000 - 1500: +2", "1501 - 1750: +4",
/// ... "2501 +: +14") but with the edges the code has: 0 up to and including
/// 1000, then 2 below 1500, 4 below 1750, 6 below 1875, 8 below 2000, 10
/// below 2250, 12 below 2500 and 14 from there. The chart's inclusive upper
/// edges (1500, 1750, ...) are one year late: at age 1500 the code already
/// pays 4.
pub fn tourist_gold(age: i32) -> i32 {
    if age <= 1000 {
        0
    } else if age < 1500 {
        2
    } else if age < 1750 {
        4
    } else if age < 1875 {
        6
    } else if age < 2000 {
        8
    } else if age < 2250 {
        10
    } else if age < 2500 {
        12
    } else {
        14
    }
}

/// Gold the Wealth improvement turns the city's shields into (`0x4B0AC0`).
///
/// Only when the city is building an improvement (`[city+0x50] == 1`) whose
/// BLDG `+0xEC & 0x80000` is set and the net shields are positive. The
/// divisor is [`SHIELDS_PER_GOLD`] (`[0x9C7268]`), halved (truncating, at
/// least 1) when the owner knows a tech with [`DOUBLE_WEALTH_TECH_FLAG`]; a
/// divisor above the shields still yields 1 gold.
pub fn wealth_gold(
    capitalization: bool,
    net_shields: i32,
    shields_per_gold: i32,
    double_wealth: bool,
) -> i32 {
    if !capitalization || net_shields <= 0 {
        return 0;
    }
    let mut d = shields_per_gold;
    if double_wealth {
        d = (d / 2).max(1);
    }
    if d > net_shields {
        1
    } else {
        net_shields / d
    }
}

/// What [`commerce_split`] needs from the city and its owner.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CommerceInput {
    /// The running commerce sum `[city+0x1D0]`.
    pub tile_commerce: i32,
    /// Sum of [`tourist_gold`] over the city's tourist attractions (`0x4B07D7`
    /// to `0x4B0808`).
    pub tourist: i32,
    /// Corruption `0x4B1190(gross, 0)`, stored in `[city+0x24C]`.
    pub lost: i32,
    /// The owner's luxury rate in tenths, `Player+0x1A4`.
    pub luxury_rate: i32,
    /// The owner's science rate in tenths, `Player+0x1A8`.
    pub science_rate: i32,
    /// Buildings with [`building_flag::LUXURY_BONUS`] (present, not obsolete).
    pub luxury_buildings: i32,
    /// Buildings with [`building_flag::RESEARCH_BONUS`].
    pub research_buildings: i32,
    /// Buildings with [`building_flag::TAX_BONUS`].
    pub tax_buildings: i32,
    /// `Player::countWonderFlag(`[`wonder_flag::DOUBLES_RESEARCH`]`, city)`.
    pub research_wonders: i32,
    /// [`wealth_gold`], added to the tax stream.
    pub wealth: i32,
}

/// The city's commerce after the split (`[city+0x24C..0x264]`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Commerce {
    /// `[city+0x24C]`: corruption.
    pub lost: i32,
    /// `[city+0x25C]`: luxury (stream 0).
    pub luxury: i32,
    /// `[city+0x260]`: science (stream 1).
    pub science: i32,
    /// `[city+0x264]`: tax (stream 2).
    pub tax: i32,
    /// `[city+0x258]` at the end: the three streams added up.
    pub total: i32,
}

/// The commerce split (`0x4B07C0`).
///
/// 1. `gross = tile_commerce + tourist`, `net = gross - lost`;
/// 2. luxury and science are `(net * rate + 5) / 10` ([`split_share`]); when
///    they add up to more than `net` the luxury share becomes `net - science`
///    (`0x4B0887`); tax is the rest;
/// 3. each stream is multiplied by `(2 + n) / 2` with `n` the number of
///    buildings carrying its bonus flag (science also `+ 2 *` the
///    Doubles-Research wonders), truncating toward zero;
/// 4. the Wealth gold is added to tax, then each stream is clamped to at
///    least 0 and `total` is their sum.
pub fn commerce_split(i: &CommerceInput) -> Commerce {
    let net = i.tile_commerce + i.tourist - i.lost;
    let mut luxury = split_share(net, i.luxury_rate);
    let science = split_share(net, i.science_rate);
    if luxury + science > net {
        luxury = net - science;
    }
    let tax = net - luxury - science;
    let luxury = luxury * (2 + i.luxury_buildings) / 2;
    let science = science * (2 + i.research_buildings + 2 * i.research_wonders) / 2;
    let tax = tax * (2 + i.tax_buildings) / 2 + i.wealth;
    let (luxury, science, tax) = (luxury.max(0), science.max(0), tax.max(0));
    Commerce {
        lost: i.lost,
        luxury,
        science,
        tax,
        total: luxury + science + tax,
    }
}

/// The three outputs of one citizen type, `[luxury, research, taxes]`: the
/// CTZN row's body `+0x68`, `+0x6C`, `+0x70` (memory `+0x6C..`, row stride
/// `0x80`, table `[0x9C40B0]`). Shipped: Entertainer `[1, 0, 0]`, Scientist
/// `[0, 3, 0]`, Tax Collector `[0, 0, 2]`, the Laborer, Policeman and Civil
/// Engineer `[0, 0, 0]`.
pub type CitizenOutput = [i32; 3];

/// The specialists' share of the three streams (`[city+0x268..0x270]`,
/// `0x4B0A25..0x4B0A9C`): the sum of the type outputs over the citizens that
/// are not resisting. Each item is `(resisting, output)`.
pub fn specialist_streams(citizens: impl IntoIterator<Item = (bool, CitizenOutput)>) -> [i32; 3] {
    let mut sum = [0; 3];
    for (resisting, out) in citizens {
        if !resisting {
            for k in 0..3 {
                sum[k] += out[k];
            }
        }
    }
    sum
}

/// What the stream accessor `0x4ACA50(city, i, bonus)` returns: the city's
/// share plus the specialists' (`0x4ACAE0` is the share alone, `0x4AC9E0`
/// the specialists alone). For the science stream (index 1) the sum is
/// multiplied by 1.25 when the caller passes a non-zero `bonus` *and* the
/// owner's timed research bonus runs (`0x55C890`: `Player+0x15D0` bit 0 and
/// `turn <= Player+0x15D4`); `science_bonus` here is that conjunction. The
/// float is `[0x66905C]`; `fild`, `fmul`, `_ftol` truncate, so `x * 5 / 4`.
pub fn stream_total(base: i32, specialists: i32, science_bonus: bool) -> i32 {
    let sum = base + specialists;
    if science_bonus {
        sum * 5 / 4
    } else {
        sum
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FACTORY: ShieldBuilding = ShieldBuilding {
        bonus: 2,
        replaces_all: false,
        prerequisite_present: false,
    };

    fn plant(bonus: i32, with_factory: bool) -> ShieldBuilding {
        ShieldBuilding {
            bonus,
            replaces_all: true,
            prerequisite_present: with_factory,
        }
    }

    #[test]
    fn a_resister_eats_nothing() {
        // Five citizens, two resisting, two food each (`0x4B05A4`).
        assert_eq!(food_eaten(5, 2, 2, false, 99), 6);
        assert_eq!(food_eaten(5, 0, 2, false, 99), 10);
        assert_eq!(food_surplus(14, 10), 4);
    }

    #[test]
    fn disorder_eats_everything_that_is_made() {
        // `[city+0x30] & 1`: eaten = gross food, so the surplus is 0.
        let eaten = food_eaten(8, 0, 2, true, 13);
        assert_eq!(eaten, 13);
        assert_eq!(food_surplus(13, eaten), 0);
    }

    #[test]
    fn a_factory_adds_half_the_net() {
        // The Civilopedia: "increases shield production in its city by 50%".
        assert_eq!(net_shields(10, 0, []), 10);
        assert_eq!(net_shields(10, 0, [FACTORY]), 15);
        // Waste comes off first: (10 - 2) * 1.5.
        assert_eq!(net_shields(10, 2, [FACTORY]), 12);
    }

    #[test]
    fn ordinary_bonuses_add_up() {
        // Factory and Manufacturing Plant: +50% and +50%, so double.
        assert_eq!(net_shields(10, 0, [FACTORY, FACTORY]), 20);
        // Iron Works (4) alone doubles too.
        let iron_works = ShieldBuilding {
            bonus: 4,
            ..FACTORY
        };
        assert_eq!(net_shields(10, 0, [iron_works]), 20);
    }

    #[test]
    fn only_the_best_power_plant_counts() {
        // Coal (2), Hydro (2) and Nuclear (4) with a Factory: the plants do
        // not add, the largest wins ("replaces any other power plant").
        let all = [FACTORY, plant(2, true), plant(2, true), plant(4, true)];
        assert_eq!(net_shields(10, 0, all), 25); // (4 + 2 + 4) / 4
        let coal_only = [FACTORY, plant(2, true)];
        assert_eq!(net_shields(10, 0, coal_only), 20); // (4 + 2 + 2) / 4
    }

    #[test]
    fn a_plant_needs_its_factory() {
        assert_eq!(net_shields(10, 0, [plant(4, false)]), 10);
        // The order of the buildings does not matter.
        assert_eq!(
            net_shields(10, 0, [plant(4, true), plant(2, true), FACTORY]),
            net_shields(10, 0, [FACTORY, plant(2, true), plant(4, true)])
        );
    }

    #[test]
    fn a_negative_net_truncates_toward_zero() {
        // `cdq; and edx, 3; add eax, edx; sar eax, 2` biases a negative
        // product before the shift, so -6 / 4 is -1, not -2.
        assert_eq!(net_shields(0, 1, [FACTORY]), -1);
        assert_eq!(net_shields(3, 5, [FACTORY]), -3);
        assert_eq!(net_shields(2, 5, [plant(0, true)]), -3);
    }

    #[test]
    fn tourist_steps_match_the_chart() {
        assert_eq!(tourist_gold(0), 0);
        assert_eq!(tourist_gold(1000), 0);
        assert_eq!(tourist_gold(1001), 2);
        assert_eq!(tourist_gold(1499), 2);
        assert_eq!(tourist_gold(1500), 4);
        assert_eq!(tourist_gold(1749), 4);
        assert_eq!(tourist_gold(1750), 6);
        assert_eq!(tourist_gold(1874), 6);
        assert_eq!(tourist_gold(1875), 8);
        assert_eq!(tourist_gold(1999), 8);
        assert_eq!(tourist_gold(2000), 10);
        assert_eq!(tourist_gold(2249), 10);
        assert_eq!(tourist_gold(2250), 12);
        assert_eq!(tourist_gold(2499), 12);
        assert_eq!(tourist_gold(2500), 14);
        assert_eq!(tourist_gold(9000), 14);
    }

    #[test]
    fn wealth_is_four_to_one_and_two_to_one_with_economics() {
        // Civilopedia: "four to one ... reduced to two to one".
        assert_eq!(wealth_gold(true, 20, SHIELDS_PER_GOLD, false), 5);
        assert_eq!(wealth_gold(true, 20, SHIELDS_PER_GOLD, true), 10);
        assert_eq!(wealth_gold(true, 21, SHIELDS_PER_GOLD, false), 5);
    }

    #[test]
    fn wealth_gives_at_least_one_gold() {
        assert_eq!(wealth_gold(true, 3, SHIELDS_PER_GOLD, false), 1);
        assert_eq!(wealth_gold(true, 1, SHIELDS_PER_GOLD, true), 1);
        // The divisor never halves below 1.
        assert_eq!(wealth_gold(true, 7, 1, true), 7);
    }

    #[test]
    fn wealth_needs_the_building_and_a_positive_net() {
        assert_eq!(wealth_gold(false, 20, SHIELDS_PER_GOLD, false), 0);
        assert_eq!(wealth_gold(true, 0, SHIELDS_PER_GOLD, false), 0);
        assert_eq!(wealth_gold(true, -4, SHIELDS_PER_GOLD, false), 0);
    }

    fn base() -> CommerceInput {
        CommerceInput {
            tile_commerce: 20,
            luxury_rate: 0,
            science_rate: 6,
            ..CommerceInput::default()
        }
    }

    #[test]
    fn the_rest_after_luxury_and_science_is_tax() {
        // 40% tax, 0% luxury, 60% science of 20: 12 and 8.
        let c = commerce_split(&base());
        assert_eq!((c.luxury, c.science, c.tax, c.total), (0, 12, 8, 20));
        let c = commerce_split(&CommerceInput {
            luxury_rate: 3,
            science_rate: 5,
            ..base()
        });
        assert_eq!((c.luxury, c.science, c.tax), (6, 10, 4));
    }

    #[test]
    fn shares_round_half_up() {
        // (net * rate + 5) / 10: 7 * 3 = 21 -> 2; 7 * 4 = 28 -> 3.
        let c = commerce_split(&CommerceInput {
            tile_commerce: 7,
            science_rate: 3,
            ..CommerceInput::default()
        });
        assert_eq!((c.science, c.tax), (2, 5));
        let c = commerce_split(&CommerceInput {
            tile_commerce: 7,
            science_rate: 4,
            ..CommerceInput::default()
        });
        assert_eq!((c.science, c.tax), (3, 4));
    }

    #[test]
    fn luxury_gives_way_when_the_rates_overshoot() {
        // 8 + 8 tenths of 10 is 16 > 10: luxury becomes net - science.
        let c = commerce_split(&CommerceInput {
            tile_commerce: 10,
            luxury_rate: 8,
            science_rate: 8,
            ..CommerceInput::default()
        });
        assert_eq!((c.luxury, c.science, c.tax), (2, 8, 0));
    }

    #[test]
    fn libraries_add_half_each() {
        // Library and University: 12 * (2 + 2) / 2 = 24.
        let c = commerce_split(&CommerceInput {
            research_buildings: 2,
            ..base()
        });
        assert_eq!(c.science, 24);
        assert_eq!(c.total, 24 + 8);
    }

    #[test]
    fn a_research_wonder_counts_double() {
        // Newton's with one Library: 12 * (2 + 1 + 2) / 2 = 30.
        let c = commerce_split(&CommerceInput {
            research_buildings: 1,
            research_wonders: 1,
            ..base()
        });
        assert_eq!(c.science, 30);
    }

    #[test]
    fn marketplace_bank_and_exchange_scale_tax() {
        // 8 * (2 + 3) / 2 = 20.
        let c = commerce_split(&CommerceInput {
            tax_buildings: 3,
            ..base()
        });
        assert_eq!(c.tax, 20);
    }

    #[test]
    fn wealth_is_added_after_the_tax_multiplier() {
        let c = commerce_split(&CommerceInput {
            tax_buildings: 1,
            wealth: 5,
            ..base()
        });
        assert_eq!(c.tax, 8 * 3 / 2 + 5);
    }

    #[test]
    fn tourists_come_before_corruption() {
        // 20 from tiles + 10 from tourists - 5 lost = 25 to split.
        let c = commerce_split(&CommerceInput {
            tourist: 10,
            lost: 5,
            science_rate: 4,
            ..base()
        });
        assert_eq!(c.lost, 5);
        assert_eq!((c.science, c.tax, c.total), (10, 15, 25));
    }

    #[test]
    fn a_negative_stream_is_clamped_to_zero() {
        // Corruption above the gross leaves a negative net.
        let c = commerce_split(&CommerceInput {
            tile_commerce: 2,
            lost: 6,
            science_rate: 5,
            ..CommerceInput::default()
        });
        assert_eq!((c.luxury, c.science, c.tax, c.total), (0, 0, 0, 0));
    }

    #[test]
    fn only_working_citizens_add_to_the_streams() {
        let entertainer = [1, 0, 0];
        let scientist = [0, 3, 0];
        let taxman = [0, 0, 2];
        let laborer = [0, 0, 0];
        let s = specialist_streams([
            (false, entertainer),
            (false, scientist),
            (false, scientist),
            (false, taxman),
            (false, laborer),
            (true, scientist),
        ]);
        assert_eq!(s, [1, 6, 2]);
        assert_eq!(specialist_streams([]), [0, 0, 0]);
    }

    #[test]
    fn the_timed_research_bonus_is_a_quarter() {
        assert_eq!(stream_total(10, 2, false), 12);
        assert_eq!(stream_total(10, 2, true), 15);
        // `_ftol` truncates: 9 * 1.25 = 11.25.
        assert_eq!(stream_total(9, 0, true), 11);
        assert_eq!(stream_total(0, 0, true), 0);
    }
}
