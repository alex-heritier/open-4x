//! Statecraft: how a civ changes its government and sets its commerce
//! rates, for the player (the Domestic Advisor, `domestic.rs`) and for the
//! computer (`ai_turn`).
//!
//! Rules and addresses: `reverse-engineering/government.md` (the revolution
//! `0x55CE50`, its length `0x53A860`, the AI's choice `0x4446C0` /
//! `0x4448F0` and its gate `0x444A10`). The AI's rate setting is not in the
//! binary's reach of this clone's notes: **HYPOTHESIS**, see `best_rates`.

use bevy::prelude::*;
use civ3mapgen::economy as exe_econ;
use civ3mapgen::government as exe;
use civ3mapgen::rng::Rng;

use crate::cities::{City, Treasury};
use crate::civs::{civ_count, Civilizations, is_ai};
use crate::diplomacy::Diplomacy;
use crate::economy;
use crate::map::GameMap;
use crate::realm::{self, Rates};
use crate::rng::GameRng;
use crate::units::{Turn, Unit};

/// "Optimal number of cities" of this map's world size (`WSIZ` memory `+4`,
/// shipped 14 / 17 / 20 / 28 / 36 from Tiny to Huge); the 80x60 map is the
/// Standard size.
pub const WORLD_BASE: i32 = 20;

/// `DIFF` per difficulty from Chieftain to Sid: the percentage that scales
/// the optimal city number (`+0x68`), the computer's anarchy cap (`+0x48`)
/// and its free units, flat (`+0x5C`) and per city (`+0x60`).
const OCN_PERCENT: [i32; 8] = [100, 95, 90, 85, 80, 70, 60, 50];
const AI_ANARCHY_CAP: [i32; 8] = [0, 0, 0, 4, 3, 2, 2, 1];
const AI_FREE_FLAT: [i32; 8] = [0, 0, 0, 4, 8, 12, 16, 24];
const AI_FREE_PER_CITY: [i32; 8] = [0, 0, 0, 1, 2, 3, 4, 8];

/// The civ's optimal number of cities (`0x5676C0`).
pub fn optimal_cities(civ: usize) -> i32 {
    let class = realm::govt(civ).corruption_class;
    exe_econ::optimal_city_number(&exe_econ::OcnInputs {
        world_base: WORLD_BASE,
        // `0x55AA10(player, 0x20, 0)`: the Reduces-Corruption wonders held.
        palace_like: realm::read(civ, |r| {
            let mut rows: Vec<(i32, i32)> = r.palaces.clone();
            rows.dedup();
            rows.len() as i32
        }),
        class,
        commercial: exe_econ::has_trait(crate::cities::traits(civ), exe_econ::trait_bit::COMMERCIAL),
        human: !is_ai(civ),
        game_level: crate::scenario::difficulty() as i32,
        percent: OCN_PERCENT[crate::scenario::difficulty()],
    })
}

/// A generator for the exe's formulas, seeded from the game's own dice.
fn rng_from(dice: &mut GameRng) -> Rng {
    Rng::new(dice.draw() << 15 | dice.draw())
}

/// Start a revolution toward `then` (`0x55CE50`): the civ falls into
/// Anarchy at once and takes `then` when it ends, or at once when the roll
/// is a turn or none (`0x55CE91`). Returns the turns of Anarchy.
pub fn revolt(civ: usize, then: usize, dice: &mut GameRng) -> u8 {
    let cities = realm::read(civ, |r| r.cities) as i32;
    let cap = (is_ai(civ) && AI_ANARCHY_CAP[crate::scenario::difficulty()] > 0).then_some(AI_ANARCHY_CAP[crate::scenario::difficulty()]);
    let religious = exe_econ::has_trait(crate::cities::traits(civ), exe_econ::trait_bit::RELIGIOUS);
    let turns = exe::anarchy_turns(religious, &mut rng_from(dice), cities, optimal_cities(civ), cap)
        .clamp(0, 255) as u8;
    realm::write(civ, |r| r.revolt(then, turns));
    turns
}

/// Every commerce rate the civ's government allows, tenths adding to ten.
fn all_rates(cap: u8) -> impl Iterator<Item = Rates> {
    (0..=10u8).flat_map(move |tax| {
        (0..=10 - tax).map(move |sci| Rates { tax, sci, lux: 10 - tax - sci })
    })
    .filter(move |r| r.valid(cap))
}

/// The computer's rates: as much science as the books allow. It tries every
/// legal split and keeps the one with the most science whose gold per turn
/// is not negative (or, with a deep treasury, not worse than a fortieth of
/// it); ties go to the higher science rate, then more tax, then less luxury. **HYPOTHESIS**: the
/// binary's rate routine is not read; this is the plain greedy reading of
/// Civ3's advice "run as much science as you can pay for".
pub fn best_rates(civ: usize, map: &GameMap, cities: &[&City], units: usize, gold: u32) -> Rates {
    let now = realm::rates(civ);
    let cap = realm::govt(civ).rate_cap as u8;
    let floor = -((gold / 40) as i32);
    let mut best: Option<(Rates, (i32, i32, i32, i32))> = None;
    for rates in all_rates(cap) {
        realm::write(civ, |r| r.rates = rates);
        let net = economy::finance(map, cities.iter().copied(), units).net();
        let sci: i32 = cities.iter().map(|c| crate::citycalc::totals(map, c).sci).sum();
        if net < floor {
            continue;
        }
        let key = (sci, rates.sci as i32, rates.tax as i32, -(rates.lux as i32));
        if best.is_none_or(|(_, k)| key > k) {
            best = Some((rates, key));
        }
    }
    realm::write(civ, |r| r.rates = now);
    best.map_or(now, |(r, _)| r)
}

/// Once per computer turn, before its units move: set the rates, and now
/// and then think about a revolution.
pub fn ai_turn(
    civs: Res<Civilizations>,
    turn: Res<Turn>,
    map: Res<GameMap>,
    treasury: Res<Treasury>,
    diplomacy: Res<Diplomacy>,
    cities: Query<&City>,
    units: Query<&Unit>,
    mut dice: ResMut<GameRng>,
    mut done: Local<Option<(usize, u32)>>,
) {
    let civ = civs.active;
    if !is_ai(civ) || civs.outcome.is_some() || *done == Some((civ, turn.0)) {
        return;
    }
    *done = Some((civ, turn.0));
    let mine: Vec<&City> = cities.iter().filter(|c| c.civ == civ).collect();
    if mine.is_empty() {
        return;
    }
    let owned = units.iter().filter(|u| u.civ == civ).count();
    let war = diplomacy.war_matrix();
    let wars = (0..civ_count()).filter(|&o| o != civ && war[civ][o]).count() as i32;

    if !realm::in_anarchy(civ) {
        // Cooldown counts down in `Realm::tick`; the gate is `0x444A10`.
        let cooling = realm::read(civ, |r| r.cooldown) > 0;
        let gate = exe::revolution_denominator(&exe::GateInputs {
            golden_age: realm::read(civ, |r| r.golden),
            no_enemies: wars == 0,
            wars,
            religious: exe_econ::has_trait(crate::cities::traits(civ), exe_econ::trait_bit::RELIGIOUS),
            weariness: realm::govt(civ).war_weariness,
            enemy_weariness: &diplomacy.enemy_weariness(civ),
        });
        if !cooling && exe::considers_revolution(&mut rng_from(&mut dice), gate) {
            let choice = choose_government(civ, &mine, owned);
            if let Some(g) = choice
                && g != realm::read(civ, |r| r.govt)
            {
                let turns = revolt(civ, g, &mut dice);
                let religious = exe_econ::has_trait(crate::cities::traits(civ), exe_econ::trait_bit::RELIGIOUS);
                let wait = exe::revolution_cooldown(religious, i32::from(turns));
                realm::write(civ, |r| r.cooldown = wait.unwrap_or(0).clamp(0, 255) as u8);
                if std::env::var("CIV3_AI_LOG").is_ok() {
                    println!(
                        "ai: {} starts a revolution toward {} ({turns} turns)",
                        crate::civs::CIVS[civ].name,
                        realm::GOVT_NAMES[g]
                    );
                }
            }
        }
    }
    if !realm::in_anarchy(civ) {
        let rates = best_rates(civ, &map, &mine, owned, treasury.0[civ]);
        realm::write(civ, |r| r.rates = rates);
    }
}

/// The government the AI would take (`0x4448F0`), among those it may adopt.
pub fn choose_government(civ: usize, mine: &[&City], units: usize) -> Option<usize> {
    let cities = mine.len() as i32;
    let classes: Vec<i32> = mine.iter().map(|c| i32::from(economy::size_class(c.size()))).collect();
    let upkeep: i32 = mine.iter().map(|c| crate::citycalc::upkeep(c)).sum();
    let ai_bonus = is_ai(civ).then_some((AI_FREE_FLAT[crate::scenario::difficulty()], AI_FREE_PER_CITY[crate::scenario::difficulty()]));
    let race = &crate::civs::RACES[civ];
    let (enemy_weariness, average_weariness) = realm::read(civ, |r| (r.war_counters.clone(), r.average_weariness));
    let scores = (0..exe::SHIPPED.len()).map(|g| {
        if !realm::read(civ, |r| r.can_adopt(g)) {
            return None;
        }
        let gov = &exe::SHIPPED[g];
        let support = exe::candidate_support_charge(
            cities,
            units as i32,
            0,
            gov.support(classes.iter().copied()),
            ai_bonus,
        );
        Some(exe::score_government(
            gov,
            &exe::ScoreInputs {
                cities,
                support_charge: support,
                upkeep,
                average_weariness,
                enemy_weariness: &enemy_weariness,
                favorite: race.favorite_government == g as i32,
                shunned: race.shunned_government == g as i32,
            },
        ))
    });
    exe::choose_government(scores)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cities::City;
    use civ3mapgen::government::row;

    fn dice() -> GameRng {
        GameRng::new(7)
    }

    #[test]
    fn a_revolution_lasts_at_least_two_turns_for_a_human() {
        realm::reset();
        // The civ knows Monarchy's advance.
        realm::write(0, |r| {
            r.known |= 1 << exe::SHIPPED[row::MONARCHY].prerequisite_tech;
            r.cities = 1;
        });
        let mut dice = dice();
        let turns = revolt(0, row::MONARCHY, &mut dice);
        assert!((2..=9).contains(&turns), "anarchy of {turns} turns");
        assert!(realm::in_anarchy(0));
        assert_eq!(realm::read(0, |r| r.anarchy), turns);
        // It ends by itself, into the chosen government.
        for _ in 0..turns {
            realm::write(0, |r| r.tick());
        }
        assert_eq!(realm::read(0, |r| r.govt), row::MONARCHY);
    }

    #[test]
    fn the_optimal_city_number_scales_with_the_world() {
        realm::reset();
        // Standard world (20), Despotism (corruption class 3 adds nothing),
        // a human on Regent (90%).
        assert_eq!(optimal_cities(0), 18);
    }

    #[test]
    fn rates_never_break_the_cap_or_the_books() {
        realm::reset();
        let all: Vec<Rates> = all_rates(10).collect();
        assert_eq!(all.len(), 66);
        assert!(all.iter().all(|r| r.tax + r.sci + r.lux == 10));
        assert_eq!(all_rates(6).count(), 36);
        assert!(all_rates(6).all(|r| r.tax.max(r.sci).max(r.lux) <= 6));
    }

    #[test]
    fn the_computer_runs_science_it_can_pay_for() {
        realm::reset();
        let map = {
            let mut m = GameMap::generate();
            for t in m.tiles.iter_mut() {
                t.base = crate::map::Base::Coast;
                t.relief = crate::map::Relief::Flat;
                t.cover = crate::map::Cover::Bare;
                t.resource = None;
            }
            m
        };
        let mut city = City::new(1, "Rome", 20, 20);
        city.set_size(4);
        city.buildings = vec![crate::cities::Production::named("Temple"), crate::cities::Production::named("Granary")];
        crate::cities::governor_assign(&map, &mut city, &Default::default());
        let one = [&city];
        // With upkeep to pay and no savings, taxes must cover it.
        let rates = best_rates(1, &map, &one, 1, 0);
        realm::write(1, |r| r.rates = rates);
        let books = economy::finance(&map, one.iter().copied(), 1);
        assert!(books.net() >= 0, "{books:?} at {rates:?}");
        // A rich civ with nothing to pay for goes all science.
        let bare = City::new(1, "Rome", 20, 20);
        let rich = best_rates(1, &map, &[&bare], 0, 1000);
        assert_eq!(rich.sci, 10);
    }

    #[test]
    fn the_computer_picks_a_better_government_once_it_knows_one() {
        realm::reset();
        let mut cities = vec![];
        for i in 0..4 {
            cities.push(City::new(1, format!("C{i}"), 10 + 3 * i, 10));
        }
        let refs: Vec<&City> = cities.iter().collect();
        realm::write(1, |r| {
            r.cities = 4;
            r.known = 0;
        });
        // Only Despotism is available at first (its prerequisite is none).
        assert_eq!(choose_government(1, &refs, 4), Some(row::DESPOTISM));
        // Monarchy's advance makes a government without the tile penalty
        // available, and it scores above Despotism.
        realm::write(1, |r| r.known |= 1 << exe::SHIPPED[row::MONARCHY].prerequisite_tech);
        assert_eq!(choose_government(1, &refs, 4), Some(row::MONARCHY));
    }
}
