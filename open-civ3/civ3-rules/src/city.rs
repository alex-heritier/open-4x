//! The city's per-turn totals: from the worked tiles to food, shields and
//! commerce.
//!
//! The game keeps three running sums in the city — food, shields and commerce —
//! one term per worked tile, and derives everything else from them in a fixed
//! chain: food eaten and surplus; shields (waste, then the building multiplier);
//! commerce (tourists, waste, the three-way split, the multipliers, Wealth,
//! specialists); and happiness.

use crate::economy::split_share;

/// RULE `Shield Cost Per Gold`: 4 in `conquests.biq`. Converts shields to gold
/// for Wealth and divides the price of a sold improvement.
pub const SHIELDS_PER_GOLD: i32 = 4;

/// TECH flag bit 12, "Doubles Effect of (Wealth) Improvement" (Economics).
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
    /// "Doubles Research Output" (Copernicus, Newton's, SETI).
    pub const DOUBLES_RESEARCH: u32 = 0x10;
    /// "Tourist Attraction" (every ancient wonder).
    pub const TOURIST_ATTRACTION: u32 = 0x2_0000;
}

/// Food the city eats: in civil disorder everything it makes, so the surplus is
/// 0; otherwise `(size - resisters) * per_citizen`. A resister eats nothing.
pub fn food_eaten(size: i32, resisters: i32, per_citizen: i32, in_disorder: bool, gross_food: i32) -> i32 {
    if in_disorder {
        gross_food
    } else {
        (size - resisters) * per_citizen
    }
}

/// The food surplus: what the food box receives each turn.
pub fn food_surplus(gross_food: i32, eaten: i32) -> i32 {
    gross_food - eaten
}

/// One building the city has, as the shield multiplier sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShieldBuilding {
    /// BLDG `+0xD0` ("Production"), in quarters of the net: 2 is +50% (Factory,
    /// Manufacturing Plant, the Coal/Hydro/Solar Plants), 4 (Nuclear, Iron
    /// Works).
    pub bonus: i32,
    /// BLDG `+0xEC & 0x2000` ([`building_flag::REPLACES_ALL`]).
    pub replaces_all: bool,
    /// For a [`replaces_all`](Self::replaces_all) building: the city has the
    /// improvement in BLDG `+0x90` (the required improvement, the Factory).
    pub prerequisite_present: bool,
}

/// Net shield production.
///
/// `net = gross - lost`; every building the city has contributes: an ordinary
/// building adds its bonus to `a`, a power-plant style building (flag `0x2000`)
/// counts only with its prerequisite and only the **largest** of them is used
/// (`b`), so a better plant replaces a worse one. The result is
/// `(4 + sum + b) * net / 4`, truncating toward zero.
pub fn net_shields(gross: i32, lost: i32, buildings: impl IntoIterator<Item = ShieldBuilding>) -> i32 {
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

/// Gold per turn from a tourist attraction: zero up to and including age 1000,
/// then 2 below 1500, 4 below 1750, 6 below 1875, 8 below 2000, 10 below 2250,
/// 12 below 2500 and 14 from there. The Civilopedia chart's inclusive upper
/// edges are one year late.
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

/// Gold the Wealth improvement turns the city's shields into. Only when the
/// city is building an improvement whose BLDG `+0xEC & 0x80000` is set and the
/// net shields are positive. The divisor is [`SHIELDS_PER_GOLD`], halved
/// (truncating, at least 1) when the owner knows a tech with
/// [`DOUBLE_WEALTH_TECH_FLAG`]; a divisor above the shields still yields 1 gold.
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
    /// The running commerce sum.
    pub tile_commerce: i32,
    /// Sum of [`tourist_gold`] over the city's tourist attractions.
    pub tourist: i32,
    /// Corruption `0x4B1190(gross, 0)`.
    pub lost: i32,
    /// The owner's luxury rate in tenths.
    pub luxury_rate: i32,
    /// The owner's science rate in tenths.
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

/// The city's commerce after the split.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Commerce {
    /// Corruption.
    pub lost: i32,
    /// Luxury (stream 0).
    pub luxury: i32,
    /// Science (stream 1).
    pub science: i32,
    /// Tax (stream 2).
    pub tax: i32,
    /// The three streams added up.
    pub total: i32,
}

/// The commerce split.
///
/// 1. `gross = tile_commerce + tourist`, `net = gross - lost`;
/// 2. luxury and science are `(net * rate + 5) / 10` ([`split_share`]); when
///    they add up to more than `net` the luxury share becomes `net - science`;
///    tax is the rest;
/// 3. each stream is multiplied by `(2 + n) / 2` with `n` the number of
///    buildings carrying its bonus flag (science also `+ 2 *` the
///    Doubles-Research wonders), truncating toward zero;
/// 4. the Wealth gold is added to tax, then each stream is clamped to at least
///    0 and `total` is their sum.
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

/// The three outputs of one citizen type, `[luxury, research, taxes]`.
pub type CitizenOutput = [i32; 3];

/// The specialists' share of the three streams: the sum of the type outputs
/// over the citizens that are not resisting. Each item is `(resisting, output)`.
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

/// The stream accessor: the city's share plus the specialists'. For the science
/// stream the sum is multiplied by 1.25 when the caller passes a non-zero
/// `bonus` *and* the owner's timed research bonus runs; `science_bonus` here is
/// that conjunction. The multiply truncates, so `x * 5 / 4`.
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
        assert_eq!(food_eaten(5, 2, 2, false, 99), 6);
        assert_eq!(food_eaten(5, 0, 2, false, 99), 10);
        assert_eq!(food_surplus(14, 10), 4);
    }

    #[test]
    fn disorder_eats_everything_that_is_made() {
        let eaten = food_eaten(8, 0, 2, true, 13);
        assert_eq!(eaten, 13);
        assert_eq!(food_surplus(13, eaten), 0);
    }

    #[test]
    fn a_factory_adds_half_the_net() {
        assert_eq!(net_shields(10, 0, []), 10);
        assert_eq!(net_shields(10, 0, [FACTORY]), 15);
        assert_eq!(net_shields(10, 2, [FACTORY]), 12);
    }

    #[test]
    fn ordinary_bonuses_add_up() {
        assert_eq!(net_shields(10, 0, [FACTORY, FACTORY]), 20);
        let iron_works = ShieldBuilding { bonus: 4, ..FACTORY };
        assert_eq!(net_shields(10, 0, [iron_works]), 20);
    }

    #[test]
    fn only_the_best_power_plant_counts() {
        let all = [FACTORY, plant(2, true), plant(2, true), plant(4, true)];
        assert_eq!(net_shields(10, 0, all), 25);
        let coal_only = [FACTORY, plant(2, true)];
        assert_eq!(net_shields(10, 0, coal_only), 20);
    }

    #[test]
    fn a_plant_needs_its_factory() {
        assert_eq!(net_shields(10, 0, [plant(4, false)]), 10);
        assert_eq!(
            net_shields(10, 0, [plant(4, true), plant(2, true), FACTORY]),
            net_shields(10, 0, [FACTORY, plant(2, true), plant(4, true)])
        );
    }

    #[test]
    fn a_negative_net_truncates_toward_zero() {
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
        assert_eq!(wealth_gold(true, 20, SHIELDS_PER_GOLD, false), 5);
        assert_eq!(wealth_gold(true, 20, SHIELDS_PER_GOLD, true), 10);
        assert_eq!(wealth_gold(true, 21, SHIELDS_PER_GOLD, false), 5);
    }

    #[test]
    fn wealth_gives_at_least_one_gold() {
        assert_eq!(wealth_gold(true, 3, SHIELDS_PER_GOLD, false), 1);
        assert_eq!(wealth_gold(true, 1, SHIELDS_PER_GOLD, true), 1);
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
        let c = commerce_split(&CommerceInput {
            research_buildings: 2,
            ..base()
        });
        assert_eq!(c.science, 24);
        assert_eq!(c.total, 24 + 8);
    }

    #[test]
    fn a_research_wonder_counts_double() {
        let c = commerce_split(&CommerceInput {
            research_buildings: 1,
            research_wonders: 1,
            ..base()
        });
        assert_eq!(c.science, 30);
    }

    #[test]
    fn marketplace_bank_and_exchange_scale_tax() {
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
        assert_eq!(stream_total(9, 0, true), 11);
        assert_eq!(stream_total(0, 0, true), 0);
    }
}
