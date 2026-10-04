//! The economy a civ runs on, kept in one place so the turn, the city
//! screen and the bottom-right info box read the same numbers: the food
//! box, unit support, building upkeep and the treasury's turn. The tile
//! penalty and the rates belong to the civ's government (`realm`,
//! `citycalc`).
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

/// Gold a civ pays each turn for its units, by its government's terms
/// (`0x53A960`, `0x55DFD0`): a free allowance made of the government's base
/// and what each town, city or metropolis supports (Despotism: 4 each; the
/// civ's total, not per city), then the government's gold per unit beyond
/// it. Every unit counts, "even Settlers" (manual, "Paying for support").
/// A civ without cities pays nothing: the binary's payer leaves when the city
/// count is zero, which is why the starting party costs nothing before the
/// capital stands. `sizes` are the sizes of the civ's cities.
pub fn unit_support_cost(civ: usize, sizes: &[u8], units: usize) -> u32 {
    let classes = sizes.iter().map(|&s| i32::from(size_class(s)));
    let terms = crate::realm::govt(civ).support(classes);
    civ3mapgen::economy::unit_support_charge(sizes.len() as i32, units as i32, 0, terms.free, terms.per_unit)
        .max(0) as u32
}

/// Gold of upkeep a city's improvements cost each turn (none in Anarchy).
pub fn building_upkeep(city: &City) -> u32 {
    crate::citycalc::upkeep(city).max(0) as u32
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
    let (mut tax, mut upkeep) = (0, 0);
    let mut sizes = vec![];
    let mut civ = None;
    for c in cities {
        tax += city_tax(map, c);
        upkeep += building_upkeep(c);
        sizes.push(c.size());
        civ = Some(c.civ);
    }
    Finance {
        tax,
        upkeep,
        unit_cost: civ.map_or(0, |civ| unit_support_cost(civ, &sizes, units)),
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

/// RULE "Shield Cost Per Gold" (`[0x9C7268]`, body `+0xA4`).
pub const SHIELDS_PER_GOLD: u32 = 4;

/// Gold a sold improvement fetches: its shield cost over the RULE "Shield
/// Cost Per Gold", 4 in `conquests.biq` (`0x4B32F0` is `0x569FE0(P; b, 0)
/// / [0x9C7268]`, `yields.md` 5.6).
pub fn sale_price(civ: usize, improvement: Production) -> u32 {
    crate::cities::price_for(civ, improvement) as u32 / SHIELDS_PER_GOLD
}

/// How much a unit type is worth keeping: its shield cost, then fighters
/// before civilians (a Warrior goes before the Worker that keeps working),
/// then its place in the roster. A type the game has no art for is worth
/// the most.
fn keep_rank(t: UnitType) -> (u16, bool, usize) {
    let r = t.row();
    if r.playable {
        (Production::from_unit(t).cost(), r.attack == 0 && r.defense == 0, t.0 as usize)
    } else {
        (u16::MAX, true, usize::MAX)
    }
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
    fn units_are_free_up_to_four_per_city_and_never_without_cities() {
        // The starting party, before the capital stands, costs nothing.
        crate::realm::reset();
        assert_eq!(unit_support_cost(0, &[], 4), 0);
        assert_eq!(unit_support_cost(0, &[], 40), 0);
        assert_eq!(unit_support_cost(0, &[1], 4), 0);
        assert_eq!(unit_support_cost(0, &[1], 5), 1);
        // The allowance is pooled across the civ's cities.
        assert_eq!(unit_support_cost(0, &[1, 1], 8), 0);
        assert_eq!(unit_support_cost(0, &[1, 1], 9), 1);
        assert_eq!(unit_support_cost(0, &[1, 1], 12), 4);
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
    fn an_improvement_sells_for_a_quarter_of_its_shield_cost() {
        // 60, 60 and 40 shields over the 4 shields a gold costs, for a
        // civ with neither of their traits (Rome: Militaristic and
        // Commercial, so its Barracks are the one half-price building).
        assert_eq!(sale_price(1, Production::Temple), 15);
        assert_eq!(sale_price(1, Production::Granary), 15);
        assert_eq!(sale_price(1, Production::Barracks), 5);
        // Japan is Religious: its Temples cost half.
        assert_eq!(sale_price(0, Production::Temple), 7);
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
            gifts: vec![],
            goods: 0,
            coastal: false,
            river: false,
            unrest: 0,
            hurry_timer: 0,
            stakes: Default::default(),
            cooldown: 0,
            unit_clocks: Vec::new(),
            civ,
            name: format!("C{x}"),
            x,
            y,
            diseased: false,
            citizens: crate::citizens::new_pool(civ, 1),
            food: 0,
            shields: 0,
            production: Production::Warrior,
            queue: vec![],
            buildings: vec![],
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
            for &(x, y) in &c.worked(&map) {
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
        // Warriors, Scouts and Workers cost 10 shields, the least; fighters
        // go first, so a Warrior goes.
        assert_eq!(disband_pick(units), Some((5, UnitType::Warrior)));
        // Without them, the Scout (10) goes before the Settler (30).
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
        // With no Warrior left the Worker is next, ahead of the Scout that
        // costs the same but is listed after it.
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
