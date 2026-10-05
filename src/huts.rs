//! Goody huts (`reverse-engineering/goody-huts.md`): the outcome roll
//! (`0x55B7D0`), the dispatcher that re-rolls until an outcome's
//! preconditions hold (`0x55B8B0`) and the eight outcomes. All draws come
//! from the gameplay die in the executable's order.
//!
//! Adaptations: the free city (outcome 2) needs the Expansionist trait,
//! which none of the clone's four civilizations has, so it always re-rolls
//! here as it would for them in the game; the map outcome's spiral and the
//! barbarian band's ring are walked on the clone's grid with Civ3's ring
//! order; the site valuation of outcome 2 is therefore never reached.

use crate::cities::City;
use crate::map::GameMap;
use crate::rng::MapRng;
use crate::units::{Unit, UnitType, def};

/// Outcome numbers (= message kinds, section 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Gold = 0,
    Map = 1,
    City = 2,
    Nothing = 3,
    Settlers = 4,
    Mercenaries = 5,
    Advance = 6,
    Barbarians = 7,
}

const T: [[i32; 9]; 6] = [
    [2, 2, 2, 1, 1, 1, 0, 0, 0],
    [9, 8, 7, 5, 4, 3, 1, 0, 0],
    [12, 11, 9, 7, 6, 4, 2, 1, 0],
    [15, 14, 11, 9, 8, 5, 3, 1, 0],
    [17, 16, 13, 11, 10, 6, 4, 2, 1],
    [19, 18, 15, 13, 12, 7, 5, 3, 2],
];
const T7: [Outcome; 9] = [
    Outcome::Nothing,
    Outcome::Barbarians,
    Outcome::Barbarians,
    Outcome::Barbarians,
    Outcome::Barbarians,
    Outcome::Barbarians,
    Outcome::Barbarians,
    Outcome::Barbarians,
    Outcome::Barbarians,
];

/// The Expansionist trait bit (`RACE.hasTrait(2)`).
pub const EXPANSIONIST: u32 = 1 << 2;

/// `idx` of section 4: the difficulty row, one easier for an Expansionist.
pub fn row(difficulty: i32, expansionist: bool) -> i32 {
    difficulty + if expansionist { 0 } else { 1 }
}

/// `0x55B7D0` for a draw `r` of `rand(20)`.
pub fn outcome(r: i32, idx: i32) -> Outcome {
    if !(0..=8).contains(&idx) {
        return Outcome::Nothing;
    }
    let i = idx as usize;
    if r < T[0][i] {
        Outcome::City
    } else if r < T[1][i] {
        Outcome::Advance
    } else if r < T[2][i] {
        Outcome::Gold
    } else if r < T[3][i] {
        Outcome::Settlers
    } else if r < T[4][i] {
        Outcome::Map
    } else if r < T[5][i] {
        Outcome::Mercenaries
    } else {
        T7[i]
    }
}

/// The gold of outcome 0: 25 before round 50, 50 from it on (`round` is
/// 0-based).
pub fn gold(round: u32) -> u32 {
    if round >= 50 { 50 } else { 25 }
}

/// Section 6.3: a free tribe name of the civ's culture group, never marked.
pub fn tribe(dice: &mut MapRng, used: &dyn Fn(usize) -> bool, group: usize) -> u8 {
    let r = dice.below(15) as usize;
    (0..15).map(|j| 15 * group + (r + j) % 15).find(|&i| !used(i)).map_or(75, |i| i as u8)
}

/// Section 6.4, `chooseHutUnit`: a mercenary type, or `None`.
/// `have(row)` is the popularity sum over the civilizations in play and
/// `players` their count.
pub fn choose_unit(
    dice: &mut MapRng,
    race: usize,
    era: i32,
    water: bool,
    players: i32,
    have: &dyn Fn(usize) -> i32,
) -> Option<UnitType> {
    let n = crate::roster::UNITS.len();
    let step = if n % 3 != 0 { 3 } else { 1 };
    let mut cur = dice.below(n as u32) as usize;
    for _ in 0..n {
        let r = crate::roster::unit(cur);
        let tech_era = if r.tech < 0 { 0 } else { crate::ruleset::TECH_TREE[r.tech as usize].0 };
        let ok = r.abilities & 1 == 0
            && r.races & 1 != 0
            && r.races & (1 << race) != 0
            && r.class == i32::from(water)
            && tech_era == era
            && r.playable;
        if ok && players > 0 && have(cur) >= players {
            return Some(UnitType(cur as u16));
        }
        cur = (cur + step) % n;
    }
    None
}

/// Ring 1 in Civ3's spiral order (`0x5E6E50`, `n = 1..8`).
pub const RING: [(i32, i32); 8] = [(1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1), (0, -1)];

/// The spiral offsets `n = 1..76`: rings 1 to 3, then the first 28 of
/// ring 4, each ring starting at its north-east corner and turning
/// clockwise as ring 1 does.
pub fn spiral76() -> Vec<(i32, i32)> {
    let mut out = vec![];
    for r in 1..=4 {
        let mut ring = vec![];
        // From the north-east corner down the east side, along the south,
        // up the west and back along the north.
        for dy in -r..=r {
            ring.push((r, dy));
        }
        for dx in (-r..r).rev() {
            ring.push((dx, r));
        }
        for dy in (-r..r).rev() {
            ring.push((-r, dy));
        }
        for dx in -r + 1..r {
            ring.push((dx, -r));
        }
        out.extend(ring);
    }
    out.truncate(76);
    out
}

/// What a pop asks the world to do.
#[derive(Debug, Default, PartialEq)]
pub struct Pop {
    pub outcome: Option<Outcome>,
    pub gold: u32,
    /// Tiles revealed to the civ.
    pub reveal: Vec<(i32, i32)>,
    /// Units for the civ (type, conscript?).
    pub units: Vec<(UnitType, bool)>,
    /// Barbarian units around the hut.
    pub barbarians: Vec<(i32, i32)>,
    pub advance: Option<i32>,
    pub tribe: u8,
}

/// Everything the dispatcher reads about the popping civ.
pub struct Context<'a> {
    pub map: &'a GameMap,
    pub civ: usize,
    pub tile: (i32, i32),
    /// The unit that walked in (`None` for a border pop).
    pub unit: Option<&'a Unit>,
    pub cities: &'a [&'a City],
    pub units: &'a [&'a Unit],
    pub round: u32,
    pub era: i32,
    pub difficulty: i32,
    /// Civilizations in play (`N - 1`).
    pub players: i32,
    pub used_tribe: &'a dyn Fn(usize) -> bool,
    /// Whether `civ` can build a unit type now.
    pub can_build: &'a dyn Fn(usize, UnitType) -> bool,
    /// The hut advance (`research.md` 10.5), drawing from the die.
    pub advance: &'a mut dyn FnMut(&mut MapRng) -> Option<i32>,
}

/// `0x55B8B0` from the first roll on.
pub fn pop(ctx: &mut Context, dice: &mut MapRng) -> Pop {
    let traits = crate::cities::traits(ctx.civ);
    let expansionist = traits & EXPANSIONIST != 0;
    let idx = row(ctx.difficulty, expansionist);
    let group = crate::civs::RACES.get(ctx.civ).map_or(0, |r| r.culture_group as usize);
    let race = crate::civs::RACES.get(ctx.civ).map_or(0, |r| r.race as usize);
    let map = ctx.map;
    let (x, y) = ctx.tile;
    let land = crate::realm::continents(map);
    let cont = land[map.idx(x, y)];
    // The nearest own city on this continent (`0x56D040`): no city keeps
    // both globals at their maximum.
    let near = ctx
        .cities
        .iter()
        .filter(|c| c.civ == ctx.civ && land[map.idx(c.x, c.y)] == cont)
        .map(|c| map.distance((c.x, c.y), (x, y)))
        .min()
        .unwrap_or(i32::MAX);
    let mine = ctx.cities.iter().filter(|c| c.civ == ctx.civ).count() as i32;
    let average_ok = mine <= ctx.cities.len() as i32 / ctx.players.max(1);
    let mut out = Pop::default();
    let mut o = if ctx.unit.is_none() { Outcome::City } else { outcome(dice.below(20) as i32, idx) };
    loop {
        let done = match o {
            Outcome::Gold => {
                if map.tiles[map.idx(x, y)].resource.is_some() {
                    false
                } else {
                    out.gold = gold(ctx.round);
                    true
                }
            }
            Outcome::Map => {
                for (dx, dy) in spiral76() {
                    let (tx, ty) = (map.wrap_x(x + dx), y + dy);
                    let Some(t) = map.get(tx, ty) else { continue };
                    if land[map.idx(tx, ty)] == cont || t.base == crate::map::Base::Coast {
                        if dice.below(4) != 0 {
                            out.reveal.push((tx, ty));
                        }
                    }
                }
                true
            }
            // Expansionist only; see the module docs.
            Outcome::City => false,
            Outcome::Nothing => true,
            Outcome::Settlers => {
                let settling = ctx.units.iter().filter(|u| u.civ == ctx.civ && u.utype == crate::roles::settler()).count()
                    + ctx.cities.iter().filter(|c| c.civ == ctx.civ && c.production.unit() == Some(crate::roles::settler())).count();
                if settling == 0 && average_ok {
                    out.units.push((crate::roles::settler(), false));
                    true
                } else {
                    false
                }
            }
            Outcome::Mercenaries => {
                let have = |row: usize| -> i32 {
                    (0..ctx.players as usize)
                        .map(|q| {
                            let live = ctx.units.iter().filter(|u| u.civ == q && u.utype.0 as usize == row).count() as i32;
                            if live > 0 {
                                live
                            } else {
                                i32::from((ctx.can_build)(q, UnitType(row as u16)))
                            }
                        })
                        .sum()
                };
                let water = map.get(x, y).is_some_and(|t| crate::improvements::is_water_base(t.base));
                match choose_unit(dice, race, ctx.era, water, ctx.players, &have) {
                    Some(t) => {
                        out.units.push((t, true));
                        true
                    }
                    None => false,
                }
            }
            Outcome::Advance => {
                if ctx.era > 0 {
                    false
                } else {
                    match (ctx.advance)(dice) {
                        Some(t) => {
                            out.advance = Some(t);
                            true
                        }
                        None => false,
                    }
                }
            }
            Outcome::Barbarians => {
                let soldiers = ctx.units.iter().any(|u| u.civ == ctx.civ && def(u.utype).attack > 0);
                let explorer = ctx.unit.is_some_and(|u| def(u.utype).abilities & (1 << 4) != 0);
                if expansionist || near <= 1 || mine == 0 || !soldiers || explorer {
                    false
                } else {
                    out.tribe = tribe(dice, ctx.used_tribe, group);
                    let mut budget = 4u32;
                    for n in 1..=8u32 {
                        let k = ((ctx.round + n) % 8) as usize;
                        let (dx, dy) = RING[k];
                        let (tx, ty) = (map.wrap_x(x + dx), y + dy);
                        let Some(t) = map.get(tx, ty) else { continue };
                        let empty = !crate::improvements::is_water_base(t.base)
                            && !t.camp
                            && !ctx.cities.iter().any(|c| (c.x, c.y) == (tx, ty))
                            && !ctx.units.iter().any(|u| (u.x, u.y) == (tx, ty));
                        if empty && dice.below(budget.max(1)) != 0 {
                            out.barbarians.push((tx, ty));
                            budget -= 1;
                        }
                    }
                    !out.barbarians.is_empty()
                }
            }
        };
        if done {
            if o != Outcome::Barbarians {
                out.tribe = tribe(dice, ctx.used_tribe, group);
            }
            out.outcome = Some(o);
            return out;
        }
        o = outcome(dice.below(20) as i32, idx);
    }
}

/// The message of each outcome (`#GOODY_*`).
pub fn message(p: &Pop, civ: usize) -> String {
    let tribe = crate::barbarians::TRIBES[p.tribe as usize];
    match p.outcome {
        Some(Outcome::Gold) => format!("We got {} gold from the {tribe} tribe's village.", p.gold),
        Some(Outcome::Map) => format!("The friendly {tribe} tribe gave us maps of their region."),
        Some(Outcome::City) => format!("An advanced {tribe} village has joined us!"),
        Some(Outcome::Settlers) => format!(
            "A friendly {tribe} Settler wants to join our {}.",
            crate::realm::GOVT_NAMES[crate::realm::read(civ, |r| r.govt)]
        ),
        Some(Outcome::Mercenaries) => format!(
            "This friendly {tribe} village gave us a skilled {}.",
            p.units.first().map_or("unit", |(t, _)| def(*t).name)
        ),
        Some(Outcome::Advance) => format!(
            "The {tribe} tribe has taught us {}.",
            p.advance.map_or("nothing", crate::research::tech_name)
        ),
        Some(Outcome::Barbarians) => format!("We have disturbed an angry {tribe} Warrior."),
        Some(Outcome::Nothing) | None => format!("This {tribe} village is deserted."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_roll_reads_the_executables_grid() {
        // Row 3 of section 4: CAAAAGGSSMMUUBBBBBBB.
        let grid: String = (0..20)
            .map(|r| match outcome(r, 3) {
                Outcome::City => 'C',
                Outcome::Advance => 'A',
                Outcome::Gold => 'G',
                Outcome::Settlers => 'S',
                Outcome::Map => 'M',
                Outcome::Mercenaries => 'U',
                Outcome::Barbarians => 'B',
                Outcome::Nothing => 'N',
            })
            .collect();
        assert_eq!(grid, "CAAAAGGSSMMUUBBBBBBB");
        assert_eq!(outcome(19, 0), Outcome::Nothing);
        assert_eq!(outcome(0, 9), Outcome::Nothing);
        // Regent (row 2), not Expansionist: idx 3.
        assert_eq!(row(2, false), 3);
    }

    #[test]
    fn gold_doubles_from_round_fifty() {
        assert_eq!((gold(49), gold(50)), (25, 50));
    }

    #[test]
    fn the_spiral_covers_three_rings_and_part_of_the_fourth() {
        let s = spiral76();
        assert_eq!(s.len(), 76);
        assert_eq!(&s[..8], &RING);
        assert_eq!(s.iter().filter(|(dx, dy)| dx.abs().max(dy.abs()) <= 3).count(), 48);
    }

    #[test]
    fn mercenaries_are_warriors_or_horsemen_in_the_first_age() {
        let mut dice = MapRng::new(7);
        for _ in 0..50 {
            let t = choose_unit(&mut dice, 9, 0, false, 4, &|_| 4).unwrap();
            assert!(t == UnitType::named("Warrior") || t == UnitType::named("Horseman"), "{}", def(t).name);
        }
        assert_eq!(choose_unit(&mut dice, 9, 1, false, 4, &|_| 4), None);
    }
}
