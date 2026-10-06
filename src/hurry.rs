//! Hurrying production with gold or forced labor (`reverse-engineering/
//! hurry.md`). The validator (`0x4B5290`) is [`quote`], the executor
//! (`0x4B5CA0`) is [`apply`], the computer's price handicap (`0x4B50A0`) is
//! [`ai_handicap`]. The clone has no construction bonus pool, so the
//! available shields `S` are the box itself.

use std::collections::HashSet;

use civ3mapgen::government::hurry as method;

use crate::cities::{City, Production};
use crate::map::GameMap;
use crate::roster::{imp, oth};

/// RULE "Shield Value in Gold" (`[0x9C7280]`).
pub const SHIELD_GOLD: u32 = 4;
/// RULE "Citizen Value in Shields" (`[0x9C7284]`).
pub const CITIZEN_SHIELDS: i32 = 20;
/// RULE "Turn Penalty for Each Hurry Sacrifice" (`[0x9C72BC]`).
pub const SACRIFICE_TURNS: u16 = 20;

/// What a hurry costs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Offer {
    Gold(u32),
    People(u8),
}

/// Why a hurry is refused, in the validator's order (`hurry.md` 4); each
/// names the script entry the advisor reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    Disorder,
    /// A citizen still resists (`HURRY_RESISTANCE`).
    Resistance,
    /// A Palace, wonder, small wonder or Wealth.
    Cannot,
    /// The government has no hurry method (Anarchy).
    Unavailable,
    NotNecessary,
    NotEnoughGold(u32),
    NotEnoughPeople(u8),
}

impl Refusal {
    /// The advisor's line (`Text/script.txt`, `#HURRY_*`).
    pub fn text(self, item: &str) -> String {
        match self {
            Refusal::Disorder => {
                "We cannot hurry production while our city is in civil disorder.".into()
            }
            Refusal::Resistance => {
                "We cannot hurry production while our city is in resistance.".into()
            }
            Refusal::Cannot => format!("We cannot hurry {item}."),
            Refusal::Unavailable => {
                "Our current government type prevents us from hurrying production.".into()
            }
            Refusal::NotNecessary => "No need to hurry this, sir.".into(),
            Refusal::NotEnoughGold(n) => {
                format!("Umm.. sir. We'd need at least {n} gold to do that.")
            }
            Refusal::NotEnoughPeople(_) => {
                "Rushing this project would cost the lives of too many citizens!".into()
            }
        }
    }
}

impl Offer {
    /// The confirmation's question and its two answers (`#HURRY_GOLD`,
    /// `#HURRY_PEOPLE`); the second answer cancels.
    pub fn question(self, item: &str) -> (String, &'static str, &'static str) {
        match self {
            Offer::Gold(n) => (
                format!("Are you sure? Hurrying {item} will cost {n} gold..."),
                "Don't argue with me. Start counting!",
                "Oh, I see. Never mind.",
            ),
            Offer::People(n) => (
                format!("Hurrying {item} could cost the lives of {n} citizens."),
                "It's that important. Get out my whip!",
                "Oh, Really. Well never mind, then.",
            ),
        }
    }
}

/// `0x569BB0`: an ordinary improvement, one that gold or labor may hurry.
pub fn ordinary(p: Production) -> bool {
    p.bldg().is_none_or(|b| {
        b.flags & (imp::CENTER_OF_EMPIRE | imp::CAPITALIZATION) == 0
            && b.other & (oth::WONDER | oth::SMALL_WONDER) == 0
    })
}

/// The gold price of `rem` missing shields with `s` already in the box
/// (`hurry.md` 3): four gold a shield, doubled for an empty box.
pub fn gold_price(rem: i32, s: i32) -> u32 {
    if rem < 1 {
        return 0;
    }
    let price = SHIELD_GOLD * rem as u32;
    if s == 0 { price * 2 } else { price }
}

/// Citizens forced labor spends on `rem` missing shields: one per twenty,
/// rounded up, doubled for an empty box.
pub fn people(rem: i32, s: i32) -> i32 {
    if rem < 1 {
        return 0;
    }
    let mut q = rem / CITIZEN_SHIELDS;
    if q * CITIZEN_SHIELDS < rem {
        q += 1;
    }
    if s == 0 { q * 2 } else { q }
}

/// `0x4B50A0`: the computer's discount on a price (`mode` 0) or a head
/// count (`mode` 1) for purchase category `t` at difficulty `d`. Humans
/// pay `v`.
pub fn ai_handicap(v: i32, mode: u8, t: usize, d: i32) -> i32 {
    if mode == 1 && v <= 0 || mode == 0 && v <= 1 {
        return v;
    }
    const M: [i32; 9] = [3, 2, 1, 4, 5, 5, 3, 4, 3];
    let m = M.get(t).copied().unwrap_or(3);
    let w = m * v / (d + 3);
    if w >= v {
        v
    } else if w > 0 {
        w
    } else if mode == 1 {
        1
    } else {
        w
    }
}

/// Who is paying, for the computer's discount.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Buyer {
    Human,
    /// The computer, with its purchase category.
    Ai(usize),
}

/// `0x4B5290` without the dialog: what hurrying the city's build would cost
/// under government hurry method `how`, with `gold` in the treasury.
pub fn quote(city: &City, how: i32, gold: u32, buyer: Buyer) -> Result<Offer, Refusal> {
    if city.unrest > 0 {
        return Err(Refusal::Disorder);
    }
    if crate::resistance::resisters(city) > 0 {
        return Err(Refusal::Resistance);
    }
    if city.production.is_building() && !ordinary(city.production) {
        return Err(Refusal::Cannot);
    }
    let cost = i32::from(city.price(city.production));
    let s = i32::from(city.shields);
    let rem = cost - s;
    let discount = |v: i32, mode: u8| match buyer {
        Buyer::Human => v,
        Buyer::Ai(t) => ai_handicap(v, mode, t, crate::scenario::difficulty() as i32),
    };
    match how {
        method::PAY => {
            let price = discount(gold_price(rem, s) as i32, 0).max(0) as u32;
            if rem < 1 || price == 0 {
                Err(Refusal::NotNecessary)
            } else if price > gold {
                Err(Refusal::NotEnoughGold(price))
            } else {
                Ok(Offer::Gold(price))
            }
        }
        method::FORCED_LABOR => {
            let n = discount(people(rem, s), 1);
            if rem < 1 || n == 0 {
                Err(Refusal::NotNecessary)
            } else if n > i32::from(city.size()) / 2 {
                Err(Refusal::NotEnoughPeople(n as u8))
            } else {
                Ok(Offer::People(n as u8))
            }
        }
        _ => Err(Refusal::Unavailable),
    }
}

/// `0x4B5CA0`: the box fills to the cost, then the gold leaves the
/// treasury (never below zero) or the citizens die and the city remembers
/// the whip for twenty turns each.
pub fn apply(map: &GameMap, city: &mut City, taken: &HashSet<(i32, i32)>, offer: Offer, gold: &mut u32, rng: &mut crate::rng::MapRng) {
    city.shields = city.price(city.production);
    match offer {
        Offer::Gold(price) => *gold = gold.saturating_sub(price),
        Offer::People(n) => {
            city.hurry_timer = city.hurry_timer.saturating_add(SACRIFICE_TURNS * u16::from(n));
            remove_citizens(map, city, taken, n, rng);
        }
    }
}

/// Native cyclic-slot victims release their own tiles/jobs. A size-class
/// change caps food at half the new box with a Granary, zero without it.
pub fn remove_citizens(map: &GameMap, city: &mut City, taken: &HashSet<(i32, i32)>, n: u8, rng: &mut crate::rng::MapRng) {
    for _ in 0..n {
        if city.size() <= 1 {
            break;
        }
        city.lose_population(1, None, rng);
    }
    crate::cities::governor_fill(map, city, taken);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gold_prices_follow_the_golden_vectors() {
        // G1..G5 (`hurry.md` 9): cost 30.
        assert_eq!(gold_price(30, 0), 240);
        assert_eq!(gold_price(20, 10), 80);
        assert_eq!(gold_price(1, 29), 4);
        assert_eq!(gold_price(0, 30), 0);
    }

    #[test]
    fn people_follow_the_golden_vectors() {
        assert_eq!(people(30, 0), 4);
        assert_eq!(people(20, 10), 1);
        assert_eq!(people(40, 1), 2);
        assert_eq!(people(100, 0), 10);
        assert_eq!(people(21, 9), 2);
    }

    #[test]
    fn the_computer_discount_follows_the_golden_vectors() {
        assert_eq!(ai_handicap(240, 0, 0, 2), 144);
        assert_eq!(ai_handicap(240, 0, 2, 2), 48);
        assert_eq!(ai_handicap(240, 0, 4, 2), 240);
        assert_eq!(ai_handicap(240, 0, 3, 2), 192);
        assert_eq!(ai_handicap(240, 0, 0, 7), 72);
        assert_eq!(ai_handicap(3, 0, 2, 2), 0);
        assert_eq!(ai_handicap(4, 1, 2, 2), 1);
        assert_eq!(ai_handicap(1, 0, 2, 2), 1);
        assert_eq!(ai_handicap(240, 0, 0, 0), 240);
        assert_eq!(ai_handicap(240, 0, 12, 2), 144);
    }

    fn city(size: u8, shields: u16, p: Production) -> City {
        let mut c = City::new(0, "Kyoto", 5, 5);
        c.set_size(size);
        c.shields = shields;
        c.production = p;
        c
    }

    #[test]
    fn the_validator_checks_in_the_executables_order() {
        let mut c = city(7, 0, Production::named("Warrior"));
        let cost = u32::from(c.price(Production::named("Warrior")));
        assert_eq!(quote(&c, method::PAY, 1000, Buyer::Human), Ok(Offer::Gold(cost * 8)));
        assert_eq!(
            quote(&c, method::PAY, 1, Buyer::Human),
            Err(Refusal::NotEnoughGold(cost * 8))
        );
        assert_eq!(quote(&c, method::NONE, 1000, Buyer::Human), Err(Refusal::Unavailable));
        c.production = Production::named("Palace");
        assert_eq!(quote(&c, method::PAY, 1000, Buyer::Human), Err(Refusal::Cannot));
        c.unrest = 1;
        assert_eq!(quote(&c, method::PAY, 1000, Buyer::Human), Err(Refusal::Disorder));
        let full = city(3, 1000, Production::named("Warrior"));
        assert_eq!(quote(&full, method::PAY, 1000, Buyer::Human), Err(Refusal::NotNecessary));
    }

    #[test]
    fn a_size_one_city_cannot_whip_and_half_the_size_is_the_limit() {
        let c = city(1, 1, Production::named("Warrior"));
        assert_eq!(
            quote(&c, method::FORCED_LABOR, 0, Buyer::Human),
            Err(Refusal::NotEnoughPeople(1))
        );
        let c = city(2, 1, Production::named("Warrior"));
        assert_eq!(quote(&c, method::FORCED_LABOR, 0, Buyer::Human), Ok(Offer::People(1)));
    }

    #[test]
    fn whipping_fills_the_box_kills_and_starts_the_timer() {
        let map = GameMap::generate();
        let mut c = city(4, 1, Production::named("Warrior"));
        let mut gold = 0;
        apply(&map, &mut c, &HashSet::new(), Offer::People(1), &mut gold, &mut crate::rng::MapRng::new(1));
        assert_eq!(c.size(), 3);
        assert_eq!(c.shields, c.price(Production::named("Warrior")));
        assert_eq!(c.hurry_timer, 20);
        let mut c = city(4, 1, Production::named("Warrior"));
        let mut gold = 100;
        apply(&map, &mut c, &HashSet::new(), Offer::Gold(36), &mut gold, &mut crate::rng::MapRng::new(1));
        assert_eq!((c.size(), gold), (4, 64));
    }

    #[test]
    fn shrinking_across_a_limit_spills_food_without_a_granary() {
        // S1/S2 (`hurry.md` 9): 7 -> 6 crosses the town limit.
        let map = GameMap::generate();
        let mut c = city(7, 0, Production::named("Warrior"));
        c.food = 25;
        remove_citizens(&map, &mut c, &HashSet::new(), 1, &mut crate::rng::MapRng::new(1));
        assert_eq!((c.size(), c.food), (6, 0));
        // S4: 5 -> 4 keeps the store unchanged, even when the box is full.
        let mut c = city(5, 0, Production::named("Warrior"));
        c.food = 20;
        remove_citizens(&map, &mut c, &HashSet::new(), 1, &mut crate::rng::MapRng::new(1));
        assert_eq!((c.size(), c.food), (4, 20));
    }
}
