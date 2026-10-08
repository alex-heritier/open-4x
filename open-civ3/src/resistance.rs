//! Resistance of captured citizens and nationality drift, the game side of
//! `civ3_rules::resistance` (`city-turn.md` 8).
//!
//! A military capture marks every citizen of another race with a pending
//! change to the captor's race and rolls whether it resists (`0x4BB090`).
//! A resister works no tile, holds no specialist job and eats nothing; while
//! one remains the city loses its size defense bonus and cannot hurry. Each
//! of the owner's turns, police units on the city tile re-roll resisters
//! with the continued chance (`0x4B2E10`), and citizens who no longer
//! resist may assimilate (`0x4AC140`). Resistance needs war with the
//! citizens' old nation: once it ends, the next quelling try ends it.
//!
//! HYPOTHESIS: the police count is the owner's attacking units on the tile
//! (`realm` garrison), standing in for `0x5A6060` mode 4 (land units with
//! positive attack or defense, any owner).

use civ3_rules::population::Citizen;
use civ3_rules::resistance::{self as native, Standing};

use crate::cities::City;
use crate::civs::{CIV_CAP, civ_count};
use crate::ruleset::{CULT, DIFF_QUELLED, GOVT_ASSIMILATION, GOVT_RESISTANCE};

/// The civ playing a RACE row (`0x539D60`); `None` when nobody does.
pub fn civ_of_race(race: i32) -> Option<usize> {
    (0..civ_count()).find(|&c| crate::civs::roster_index(c) as i32 == race)
}

/// What the rolls read about every civ, taken once per use.
#[derive(Clone, Debug)]
pub struct Nations {
    pub turn: i32,
    pub rating: [i32; CIV_CAP],
    pub govt: [usize; CIV_CAP],
    pub cities: [i32; CIV_CAP],
    pub war: [[bool; CIV_CAP]; CIV_CAP],
}

impl Nations {
    /// The realm table as `realm::sync` last refreshed it.
    pub fn current() -> Self {
        let mut n = Nations {
            turn: crate::realm::turn() as i32,
            rating: [0; CIV_CAP],
            govt: [0; CIV_CAP],
            cities: [0; CIV_CAP],
            war: [[false; CIV_CAP]; CIV_CAP],
        };
        for civ in 0..civ_count() {
            crate::realm::read(civ, |r| {
                n.rating[civ] = r.rating;
                n.govt[civ] = r.govt;
                n.cities[civ] = r.cities as i32;
                n.war[civ] = r.at_war;
            });
        }
        n
    }

    fn standing(&self, owner: usize, race: i32) -> Standing {
        match civ_of_race(race) {
            Some(s) => Standing {
                other_has_cities: self.cities[s] > 0,
                at_war: self.war[owner][s],
                owner_rating: self.rating[owner],
                other_rating: self.rating[s],
                owner_govt: self.govt[owner],
                other_govt: self.govt[s],
            },
            None => Standing {
                other_has_cities: false,
                at_war: false,
                owner_rating: self.rating[owner],
                other_rating: 0,
                owner_govt: self.govt[owner],
                other_govt: 0,
            },
        }
    }
}

fn ids(city: &City) -> Vec<usize> {
    city.citizens
        .slots()
        .iter()
        .enumerate()
        .filter(|(_, c)| c.is_some())
        .map(|(i, _)| i)
        .collect()
}

fn reroll(
    c: &mut Citizen,
    initial: bool,
    owner: usize,
    n: &Nations,
    rng: &mut crate::rng::MapRng,
) -> bool {
    native::reroll(
        c,
        initial,
        &CULT,
        &GOVT_RESISTANCE,
        |race| n.standing(owner, race),
        |m| rng.below(m as u32),
    )
}

/// `0x4BB090` after a capture by `captor` (the city's new owner): returns
/// how many citizens resist, the `RESISTERS` count. A culture conversion
/// only marks the pending race and rolls nothing.
pub fn seed(
    city: &mut City,
    captor: usize,
    convert: bool,
    n: &Nations,
    rng: &mut crate::rng::MapRng,
) -> i32 {
    let race = crate::civs::roster_index(captor) as i32;
    let mut resisting = 0;
    for i in ids(city) {
        let c = city.citizens.get_mut(i).unwrap();
        native::mark_capture(c, race, n.turn);
        if !convert && reroll(c, true, captor, n, rng) {
            resisting += 1;
        }
    }
    resisting
}

/// Resisting citizens (`0x4BB2A0(city; -1)`).
pub fn resisters(city: &City) -> i32 {
    city.citizens
        .slots()
        .iter()
        .flatten()
        .filter(|c| c.resister)
        .count() as i32
}

/// What a city's resistance step reports to its owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notice {
    /// `RESISTANCEQUELLED`: some were quelled, some still resist.
    Quelled(i32),
    /// `RESISTANCEENDS`: the last resister stopped.
    Ends,
    /// A citizen took the owner's nationality.
    Assimilated(i32),
}

/// Sequencer steps 5 and 6 (`0x4BE970`): every citizen's nationality drift,
/// then resistance quelling by `police` units on the tile.
pub fn city_step(
    city: &mut City,
    police: i32,
    n: &Nations,
    rng: &mut crate::rng::MapRng,
) -> Vec<Notice> {
    let owner = city.civ;
    let mut out = vec![];
    let mut assimilated = 0;
    for i in ids(city) {
        let c = city.citizens.get_mut(i).unwrap();
        let other = civ_of_race(c.race).map_or(0, |s| n.rating[s]);
        if native::drift(
            c,
            n.turn,
            n.rating[owner],
            other,
            GOVT_ASSIMILATION[n.govt[owner]],
            |m| rng.below(m as u32),
        ) {
            assimilated += 1;
        }
    }
    if assimilated > 0 {
        out.push(Notice::Assimilated(assimilated));
    }
    let before = resisters(city);
    let quelling = police * DIFF_QUELLED[crate::scenario::difficulty()];
    if before > 0 && quelling > 0 {
        let slots = ids(city);
        let mut pool: Vec<Citizen> = slots
            .iter()
            .map(|&i| city.citizens.slots()[i].clone().unwrap())
            .collect();
        let quelled = {
            let mut refs: Vec<&mut Citizen> = pool.iter_mut().collect();
            native::quell(&mut refs, quelling, |c| reroll(c, false, owner, n, rng))
        };
        for (&i, c) in slots.iter().zip(pool) {
            *city.citizens.get_mut(i).unwrap() = c;
        }
        let left = resisters(city);
        if left == 0 {
            out.push(Notice::Ends);
        } else if quelled > 0 {
            out.push(Notice::Quelled(quelled));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::MapRng;

    fn nations(war: bool) -> Nations {
        let mut n = Nations {
            turn: 40,
            rating: [100; CIV_CAP],
            govt: [1; CIV_CAP],
            cities: [3; CIV_CAP],
            war: [[false; CIV_CAP]; CIV_CAP],
        };
        n.war[0][1] = war;
        n.war[1][0] = war;
        n
    }

    fn captured(size: u8) -> City {
        crate::realm::reset();
        let mut city = City::new(1, "Roma", 5, 5);
        city.set_size(size);
        city.civ = 0;
        city
    }

    #[test]
    fn capture_rolls_the_initial_chance_for_each_foreigner() {
        // Equal culture: CULT row 2 (60%), Despotism vs Despotism +0.
        let mut city = captured(4);
        let mut rng = MapRng::new(1);
        let mut expect = MapRng::new(1);
        let want = (0..4).filter(|_| expect.below(100) < 60).count() as i32;
        let n = seed(&mut city, 0, false, &nations(true), &mut rng);
        assert_eq!(n, want);
        assert_eq!(rng.state(), expect.state(), "one draw per foreign citizen");
        assert_eq!(resisters(&city), want);
        assert!(
            city.citizens
                .slots()
                .iter()
                .flatten()
                .all(|c| c.pending_race == crate::civs::roster_index(0) as i32
                    && c.pending_turn == 40)
        );
        assert!(
            city.citizens
                .slots()
                .iter()
                .flatten()
                .filter(|c| c.resister)
                .all(|c| c.work == 0 && c.job == 0)
        );
    }

    #[test]
    fn peace_and_conversion_roll_nothing() {
        let mut city = captured(3);
        let mut rng = MapRng::new(1);
        assert_eq!(seed(&mut city, 0, false, &nations(false), &mut rng), 0);
        assert_eq!(seed(&mut city, 0, true, &nations(true), &mut rng), 0);
        assert_eq!(rng.state(), 1, "neither path draws");
    }

    #[test]
    fn police_quell_and_the_last_one_ends_resistance() {
        let mut city = captured(2);
        for i in 0..2 {
            let c = city.citizens.get_mut(i).unwrap();
            native::mark_capture(c, crate::civs::roster_index(0) as i32, 40);
            c.resister = true;
        }
        let n = nations(true);
        // No police: nobody is re-rolled.
        let mut rng = MapRng::new(1);
        assert!(city_step(&mut city, 0, &n, &mut rng).is_empty());
        assert_eq!(resisters(&city), 2);
        // Peace ends resistance without a draw at the first police try.
        let mut rng = MapRng::new(1);
        let notes = city_step(&mut city, 2, &nations(false), &mut rng);
        assert_eq!(notes, vec![Notice::Ends]);
        assert_eq!(resisters(&city), 0);
    }
}
