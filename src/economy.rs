//! The economy a civ runs on, kept in one place so the turn, the city
//! screen and the bottom-right info box read the same numbers: the food
//! box, Despotism's tile penalty, unit support, building upkeep and the
//! treasury's turn.
//!
//! Findings, addresses and what is still a guess: `reverse-engineering/
//! economy.md` ("Growth and the food box", "Gold: unit support and
//! upkeep"). Each rule below says where it comes from.

use crate::cities::{City, Production, city_tax};
use crate::map::GameMap;
use crate::units::UnitType;

/// The food box's unit, "X" in `2 * X * (class + 1)`. The binary reads 10
/// for a human player (`0x5660E0`); only AI players use the difficulty's
/// own number, and every civ here is human.
const FOOD_BOX_X: u8 = 10;

/// Town (0), city (1) or metropolis (2). RULE ints 71 and 72 hold the
/// cut-offs 6 and 12 (`0x427540`); the city sprites change at the same
/// sizes.
pub fn size_class(size: u8) -> u8 {
    match size {
        0..=6 => 0,
        7..=12 => 1,
        _ => 2,
    }
}

/// Food a city of `size` stores before it grows: 20, 40 or 60 by class
/// (`0x4B21C6..0x4B21E0`; growth fires when box plus surplus reaches it).
pub fn food_box(size: u8) -> u8 {
    2 * FOOD_BOX_X * (size_class(size) + 1)
}

/// What a Granary keeps when its city grows: half the box, `X * (class
/// + 1)`. Without one the box empties (`[bldg+0xEC] & 0x200` picks).
pub fn granary_keep(size: u8) -> u8 {
    food_box(size) / 2
}

/// Despotism's cap on a worked tile: a yield above two loses one, food,
/// shields and commerce alike (Civilopedia `GOVT_Despotism`, manual
/// "Despotism"). The city center is exempt: nobody works it. That
/// exemption is the clone's choice; the sources only say "city production
/// square".
pub fn despotism(yield_: u8) -> u8 {
    if yield_ > 2 { yield_ - 1 } else { yield_ }
}

/// Units each town, city and metropolis supports for free under
/// Despotism (GOVT row, ints 135..137, all 4; the Civilopedia table says
/// the same). The allowance is the civ's total, not per city.
pub const FREE_UNITS_PER_CITY: u32 = 4;

/// Gold per unit beyond the allowance, each turn (Civilopedia
/// `GCON_Unit_Support`).
pub const GOLD_PER_UNIT: u32 = 1;

/// Units the civ supports at no cost. Zero cities gives zero, but see
/// `unit_support_cost`: without cities nothing is charged at all.
pub fn free_units(cities: usize) -> u32 {
    FREE_UNITS_PER_CITY * cities as u32
}

/// Gold a civ pays each turn for its units. Every unit counts, "even
/// Settlers" (manual, "Paying for support"); captured units would be free
/// but the clone has none. A civ without cities pays nothing: the
/// binary's payer (`0x55DFD0`) leaves when the city count is zero, which
/// is why the starting party costs nothing before the capital stands.
pub fn unit_support_cost(units: usize, cities: usize) -> u32 {
    if cities == 0 {
        return 0;
    }
    (units as u32).saturating_sub(free_units(cities)) * GOLD_PER_UNIT
}

/// Gold of upkeep a city's improvements cost each turn.
pub fn building_upkeep(city: &City) -> u32 {
    city.buildings.iter().map(|b| b.upkeep() as u32).sum()
}

/// One civ's gold for a turn: what comes in and what goes out. The turn
/// closes its books on exactly this, and the info box shows its `net`, so
/// the two can never disagree.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Finance {
    /// Tax from every city's commerce.
    pub tax: u32,
    /// Improvement upkeep, all cities.
    pub upkeep: u32,
    /// Support for units beyond the free allowance.
    pub unit_cost: u32,
}

impl Finance {
    /// Gold gained (positive) or lost (negative) per turn.
    pub fn net(&self) -> i32 {
        self.tax as i32 - self.upkeep as i32 - self.unit_cost as i32
    }
}

/// A civ's finances: `cities` are its own cities, `units` how many units
/// it owns.
pub fn finance<'a>(
    map: &GameMap,
    cities: impl IntoIterator<Item = &'a City>,
    units: usize,
) -> Finance {
    let (mut tax, mut upkeep, mut count) = (0, 0, 0);
    for c in cities {
        tax += city_tax(map, c);
        upkeep += building_upkeep(c);
        count += 1;
    }
    Finance {
        tax,
        upkeep,
        unit_cost: unit_support_cost(units, count),
    }
}

/// A bill met from the treasury as far as it goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Paid {
    /// What the treasury has left.
    pub treasury: u32,
    /// What it could not cover; zero when the bill was met.
    pub short: u32,
}

/// Pay `bill` out of `treasury`. A civ that cannot pay hands over all it
/// has and the rest is forgiven: the treasury never goes negative, and
/// the shortfall costs one unit (`disband_pick`) or one improvement
/// (`sale_pick`) instead. Both of the binary's payers start this way:
/// units at `0x55E067` (`edi = treasury`), upkeep at `0x560C27` (treasury
/// set to zero).
///
/// The turn closes in this order (`cities::end_turn_cities`): the tax
/// comes in, upkeep is paid and a shortfall sells an improvement for its
/// `sale_price`, then units are paid from what is left. The binary rolls
/// `next(4)` on the gameplay `Random` (`0x560BFC`) and takes that order
/// three turns in four, units first on a zero; the clone always takes the
/// likelier one. Income before charges matches play: a civ with a
/// positive net and an empty treasury loses nothing.
pub fn pay(treasury: u32, bill: u32) -> Paid {
    Paid {
        treasury: treasury.saturating_sub(bill),
        short: bill.saturating_sub(treasury),
    }
}

/// Gold a sold improvement fetches: one per shield of its cost. The binary
/// computes `cost * 10 / [0x9C7268]` (`0x4B32F0`; `0x569FE0` is the cost
/// times the human 10), and that divisor is not pinned down, so one gold
/// per shield is the clone's HYPOTHESIS.
pub fn sale_price(improvement: Production) -> u32 {
    improvement.cost() as u32
}

/// How much a unit type is worth keeping: its shield cost, then its place
/// in the build list. A type that cannot be built is worth the most.
fn keep_rank(t: UnitType) -> (u8, usize) {
    Production::ALL
        .iter()
        .position(|p| p.unit() == Some(t))
        .map_or((u8::MAX, usize::MAX), |i| (Production::ALL[i].cost(), i))
}

/// The unit a broke civ gives up: the cheapest to rebuild, and at equal
/// price the type listed first when building (a Warrior goes before a
/// Worker, which keeps working). The id only separates units of one type,
/// which are interchangeable; it is not an age, since Bevy orders an entity
/// by its generation before its index. The binary lets its player object
/// pick (vtable `+0x34`); this is the clone's rule.
pub fn disband_pick<I: Ord + Copy>(
    units: impl IntoIterator<Item = (I, UnitType)>,
) -> Option<(I, UnitType)> {
    units
        .into_iter()
        .min_by_key(|&(id, t)| (keep_rank(t), std::cmp::Reverse(id)))
}

/// The improvement a broke civ sells: the dearest to keep, then the one in
/// the newest city, then the newest in it. Returns the city's id and the
/// index into its `buildings`. The binary starts at a random city and a
/// random improvement and sells the first one that qualifies, never a
/// wonder (`0x560280`, `0x4C1590`); this is the clone's dice-free rule.
pub fn sale_pick<'a, I: Ord + Copy + 'a>(
    cities: impl IntoIterator<Item = (I, &'a City)>,
) -> Option<(I, usize)> {
    cities
        .into_iter()
        .flat_map(|(id, c)| {
            c.buildings
                .iter()
                .enumerate()
                .filter(|(_, b)| b.upkeep() > 0)
                .map(move |(i, b)| ((b.upkeep(), c.founded, i, id), (id, i)))
        })
        .max_by_key(|&(key, _)| key)
        .map(|(_, pick)| pick)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{Base, Relief};
    use std::collections::HashSet;

    #[test]
    fn the_food_box_follows_the_size_class() {
        for size in 1..=6 {
            assert_eq!(food_box(size), 20, "town of {size}");
        }
        for size in 7..=12 {
            assert_eq!(food_box(size), 40, "city of {size}");
        }
        for size in [13, 20, 40] {
            assert_eq!(food_box(size), 60, "metropolis of {size}");
        }
        assert_eq!(
            [granary_keep(6), granary_keep(7), granary_keep(13)],
            [10, 20, 30]
        );
    }

    #[test]
    fn despotism_trims_only_what_exceeds_two() {
        assert_eq!([0, 1, 2, 3, 4, 6].map(despotism), [0, 1, 2, 2, 3, 5]);
    }

    #[test]
    fn units_are_free_up_to_four_per_city_and_never_without_cities() {
        // The starting party, before the capital stands, costs nothing.
        assert_eq!(unit_support_cost(4, 0), 0);
        assert_eq!(unit_support_cost(40, 0), 0);
        assert_eq!(unit_support_cost(4, 1), 0);
        assert_eq!(unit_support_cost(5, 1), 1);
        // The allowance is pooled across the civ's cities.
        assert_eq!(unit_support_cost(8, 2), 0);
        assert_eq!(unit_support_cost(9, 2), 1);
        assert_eq!(unit_support_cost(12, 2), 4);
    }

    #[test]
    fn a_bill_takes_what_the_treasury_has_and_forgives_the_rest() {
        let paid = |treasury, short| Paid { treasury, short };
        assert_eq!(pay(10, 4), paid(6, 0));
        assert_eq!(pay(3, 3), paid(0, 0));
        assert_eq!(pay(3, 5), paid(0, 2));
        assert_eq!(pay(0, 1), paid(0, 1));
        assert_eq!(pay(0, 0), paid(0, 0));
        assert_eq!(pay(u32::MAX, 7).treasury, u32::MAX - 7);
    }

    #[test]
    fn an_improvement_sells_for_its_shield_cost() {
        assert_eq!(sale_price(Production::Temple), 30);
        assert_eq!(sale_price(Production::Granary), 40);
        assert_eq!(sale_price(Production::Barracks), 30);
    }

    #[test]
    fn net_is_tax_less_upkeep_and_support() {
        let f = Finance {
            tax: 2,
            upkeep: 1,
            unit_cost: 3,
        };
        assert_eq!(f.net(), -2);
        assert_eq!(Finance::default().net(), 0);
    }

    /// A flat grassland map: every city center pays one commerce, which
    /// the 50% tax rate rounds down to nothing, so taxes come from roads.
    fn flat_map() -> GameMap {
        let mut map = GameMap::generate();
        for t in map.tiles.iter_mut() {
            t.base = Base::Grassland;
            t.relief = Relief::Flat;
            t.cover = crate::map::Cover::Bare;
            t.resource = None;
            t.road = false;
            t.seen = true;
        }
        map
    }

    fn city_at(map: &GameMap, civ: usize, x: i32, y: i32) -> City {
        let mut city = City {
            civ,
            name: format!("C{x}"),
            x,
            y,
            size: 1,
            food: 0,
            shields: 0,
            production: Production::Warrior,
            queue: vec![],
            buildings: vec![],
            worked: HashSet::new(),
            culture: 0,
            founded: x as u32,
        };
        crate::cities::governor_assign(map, &mut city, &HashSet::new());
        city
    }

    #[test]
    fn finance_adds_the_cities_and_charges_units_past_the_allowance() {
        let mut map = flat_map();
        let mut a = city_at(&map, 0, 10, 10);
        let mut b = city_at(&map, 0, 20, 10);
        // A road under each worked tile: 2 commerce each (the center's one
        // plus the road), so 1 tax per city.
        for c in [&a, &b] {
            for &(x, y) in &c.worked {
                let i = map.idx(x, y);
                map.tiles[i].road = true;
            }
        }
        a.buildings.push(Production::Temple);
        b.buildings.extend([Production::Barracks, Production::Granary]);
        let f = finance(&map, [&a, &b], 9);
        assert_eq!(f.tax, 2);
        assert_eq!(f.upkeep, 3);
        assert_eq!(f.unit_cost, 1, "nine units, eight free");
        assert_eq!(f.net(), -2);
        assert_eq!(finance(&map, [], 9), Finance::default());
    }

    #[test]
    fn a_broke_civ_gives_up_the_cheapest_unit() {
        let units = [
            (1u32, UnitType::Settler),
            (2, UnitType::Scout),
            (3, UnitType::Warrior),
            (4, UnitType::Worker),
            (5, UnitType::Warrior),
        ];
        // Warriors and Workers cost 10 shields, the least; a Warrior goes.
        assert_eq!(disband_pick(units), Some((5, UnitType::Warrior)));
        // Without them, the Scout (20) goes before the Settler (30).
        assert_eq!(
            disband_pick(units[..2].iter().copied()),
            Some((2, UnitType::Scout))
        );
        assert_eq!(disband_pick(Vec::<(u32, UnitType)>::new()), None);
    }

    #[test]
    fn at_equal_price_a_warrior_goes_before_a_worker_whatever_the_ids() {
        for (warrior, worker) in [(1u32, 9u32), (9, 1)] {
            let units = [(worker, UnitType::Worker), (warrior, UnitType::Warrior)];
            assert_eq!(
                disband_pick(units),
                Some((warrior, UnitType::Warrior)),
                "warrior {warrior}, worker {worker}"
            );
        }
        // With no Warrior left the Worker is next, ahead of the dearer Scout.
        let rest = [(2u32, UnitType::Scout), (7, UnitType::Worker)];
        assert_eq!(disband_pick(rest), Some((7, UnitType::Worker)));
    }

    #[test]
    fn a_broke_civ_sells_the_dearest_newest_improvement() {
        let map = flat_map();
        let mut old = city_at(&map, 0, 10, 10);
        let mut new = city_at(&map, 0, 20, 10);
        old.founded = 1;
        new.founded = 5;
        old.buildings = vec![Production::Temple, Production::Granary];
        new.buildings = vec![Production::Barracks];
        // Equal upkeep: the newer city, then its newest building.
        assert_eq!(sale_pick([(1u32, &old), (2, &new)]), Some((2, 0)));
        new.buildings.clear();
        assert_eq!(sale_pick([(1u32, &old), (2, &new)]), Some((1, 1)));
        old.buildings.clear();
        assert_eq!(sale_pick([(1u32, &old), (2, &new)]), None);
    }
}
