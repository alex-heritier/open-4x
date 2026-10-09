//! Civ3's combat, rule for rule: per-round odds, one hit point per round, hit points by
//! experience level, the defender chosen by rating, retreat at one hit point for fast units,
//! ranged bombardment that wounds but never kills, defensive bombard, capture of defenseless
//! units, and promotion by dice. Terms this game has no counterpart for (rivers, radar,
//! walls, barbarians) are omitted.
use crate::battle::{Battle, Support};
use crate::units::*;
use crate::{Game, Id, Result, Rules, error, terrain::Coord};
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;

/// Dice are drawn from 0..1024; a die at or above the odds is a hit for the attacker.
const DIE: u32 = 1024;
/// City size classes: towns up to this many citizens, cities up to the next.
const TOWN_MAX: i32 = 6;
const CITY_MAX: i32 = 12;
/// Defender bonus of a city by size class: town, city, metropolis.
const CITY_PERCENT: [i32; 3] = [0, 50, 100];
/// Promotion dice by current level: a promotion needs a roll of zero.
const PROMOTION_DIE: [u32; 3] = [2, 4, 8];

/// What a fight would probably come to, for the interface and the computer players.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Estimate {
    pub defender: Id,
    /// 1024 times the chance the defender wins a round.
    pub odds: i32,
    /// Chance the attacker wins the whole fight, ignoring retreats and defensive bombard.
    pub win_chance: f32,
}

/// How a duel ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    AttackerWon,
    DefenderWon,
    DefenderRetreated,
    AttackerRetreated,
}

/// 1024 times the chance the defender wins a round, from strengths and the defender's
/// percentage bonus. Never 0 or 1024, so no fight is certain.
pub fn round_odds(attack: i32, defense: i32, defender_percent: i32) -> i32 {
    let x = i64::from(defense) * i64::from(100 + defender_percent);
    let y = i64::from(attack) * 100;
    if x + y == 0 {
        return 512;
    }
    (i64::from(DIE) * x / (x + y)).clamp(1, i64::from(DIE) - 1) as i32
}

/// Chance the attacker wins when each round goes to it with probability `1 - odds/1024`.
pub fn win_chance(odds: i32, attacker_hp: i32, defender_hp: i32) -> f32 {
    let p = 1.0 - odds as f32 / DIE as f32;
    let (a, d) = (attacker_hp.max(0) as usize, defender_hp.max(0) as usize);
    let mut f = vec![vec![0.0f32; d + 1]; a + 1];
    for (i, row) in f.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = match (i, j) {
                (0, _) => 0.0,
                (_, 0) => 1.0,
                _ => 0.0,
            };
        }
    }
    for i in 1..=a {
        for j in 1..=d {
            f[i][j] = p * f[i][j - 1] + (1.0 - p) * f[i - 1][j];
        }
    }
    f[a][d]
}

impl Game {
    /// A city's defender bonus: 0, 50, or 100 percent by size.
    pub fn city_percent(&self, p: Coord) -> i32 {
        let Some(city) = self.city_at(p).and_then(|id| self.cities.get(&id)) else {
            return 0;
        };
        let class =
            usize::from(city.population > TOWN_MAX) + usize::from(city.population > CITY_MAX);
        CITY_PERCENT[class]
    }
    /// Fortify bonus a land unit has this instant. Units in a city count as fortified; the
    /// bonus needs movement left, so a unit that has spent its turn is exposed.
    pub fn fortify_percent(&self, unit: &Unit, def: &UnitDef) -> i32 {
        let on_land = self.map.get(unit.position).is_some_and(|t| t.is_land());
        let dug_in = unit.is_fortified() || self.city_at(unit.position).is_some();
        if def.domain == Domain::Land && on_land && dug_in && unit.moves_left(def) > 0 {
            FORTIFY_PERCENT
        } else {
            0
        }
    }
    /// Total defender bonus in percent: terrain, city size, and fortification.
    pub fn defender_percent(&self, unit: &Unit, def: &UnitDef) -> i32 {
        self.map
            .get(unit.position)
            .map_or(0, |t| t.defense_percent())
            + self.city_percent(unit.position)
            + self.fortify_percent(unit, def)
    }
    /// How good a defender a unit is right now: strength, fortification, and health.
    fn defense_rating(&self, unit: &Unit, def: &UnitDef) -> i64 {
        i64::from(100 + self.fortify_percent(unit, def))
            * i64::from(def.defense)
            * i64::from(unit.hp(def).clamp(0, 9999))
            / 100
    }
    /// Land units fight only on land and ships only at sea; a ship tied up in port does not
    /// defend against a melee attack, and a passenger never defends on its own.
    fn fights_here(def: &UnitDef, tile_is_land: bool) -> bool {
        (def.domain == Domain::Land) == tile_is_land
    }
    /// The unit that defends a square for `holder`: the best rating wins, and a tie goes to
    /// the weaker attacker, then the weaker bombarder, then the sturdier-looking older unit.
    /// Units with no defense never defend.
    pub fn best_defender(&self, position: Coord, holder: Id, rules: &Rules) -> Option<Id> {
        let land = self.map.get(position)?.is_land();
        self.units_at(position)
            .into_iter()
            .filter(|u| u.owner == holder && u.carrier.is_none())
            .filter_map(|u| {
                let def = rules.def(u);
                let rating = self.defense_rating(u, def);
                (Self::fights_here(def, land) && rating > 0).then_some((
                    rating,
                    Reverse(def.attack),
                    Reverse(def.bombard),
                    Reverse(u.max_hp(def)),
                    Reverse(u.id),
                    u.id,
                ))
            })
            .max()
            .map(|best| best.5)
    }
    /// Predict a melee attack by `attacker` on `target`.
    pub fn estimate_attack(&self, attacker: Id, target: Coord, rules: &Rules) -> Option<Estimate> {
        let unit = self.units.get(&attacker)?;
        let def = rules.def(unit);
        let holder = self.hostile_holder(target, unit.owner)?;
        let defender = self
            .units
            .get(&self.best_defender(target, holder, rules)?)?;
        let d_def = rules.def(defender);
        let odds = round_odds(
            def.attack,
            d_def.defense,
            self.defender_percent(defender, d_def),
        );
        Some(Estimate {
            defender: defender.id,
            odds,
            win_chance: win_chance(odds, unit.hp(def), defender.hp(d_def)),
        })
    }

    fn name_of(&self, unit: &Unit, rules: &Rules) -> String {
        format!(
            "{} {}",
            self.factions[&unit.owner].adjective,
            rules.def(unit).name
        )
    }
    fn damage(&mut self, id: Id) {
        self.units.get_mut(&id).expect("unit exists").damage += 1;
    }
    fn remaining(&self, id: Id, rules: &Rules) -> i32 {
        let unit = &self.units[&id];
        unit.hp(rules.def(unit))
    }
    fn die(&mut self, sides: u32) -> u32 {
        self.random(sides) as u32
    }

    /// A winner may be promoted: one roll, and a unit that already failed one this turn
    /// is promoted outright. Returns whether the unit gained a level.
    pub(crate) fn promote(&mut self, id: Id) -> bool {
        let unit = &self.units[&id];
        let Some(&die) = PROMOTION_DIE.get(usize::from(unit.level)) else {
            return false;
        };
        let rolled = unit.promotion_failed || self.die(die) == 0;
        let unit = self.units.get_mut(&id).unwrap();
        if rolled {
            unit.level += 1;
        } else {
            unit.promotion_failed = true;
        }
        rolled
    }

    /// Before a duel, artillery with the defender may shoot the attacker once. Returns the shot
    /// for the record, or `None` when nobody fired.
    fn defensive_bombard(
        &mut self,
        attacker: Id,
        defender: Id,
        rules: &Rules,
    ) -> Option<Support> {
        let victim = &self.units[&attacker];
        let victim_def = rules.def(victim);
        if victim_def.defense <= 0 || victim.hp(victim_def) <= 1 {
            return None;
        }
        let (position, holder) = {
            let d = &self.units[&defender];
            (d.position, d.owner)
        };
        let shooter = self
            .units_at(position)
            .into_iter()
            .filter(|u| u.owner == holder && u.id != defender && !u.fired && u.carrier.is_none())
            .filter(|u| {
                let def = rules.def(u);
                def.bombard > 0 && def.domain == victim_def.domain
            })
            // the strongest wins; of equals, the first stays
            .min_by_key(|u| (Reverse(rules.def(u).bombard), u.id))
            .map(|u| (u.id, rules.def(u).bombard));
        let (shooter, strength) = shooter?;
        let fighter = self.fighter(shooter, rules)?;
        self.units.get_mut(&shooter).unwrap().fired = true;
        // Raw odds: no terrain or fortification on either side.
        let odds = round_odds(strength, victim_def.defense, 0);
        let hit = self.die(DIE) as i32 >= odds;
        if hit {
            self.damage(attacker);
        }
        Some(Support {
            shooter: fighter,
            hit,
        })
    }

    /// The square a retreating defender slips back to: straight away from the attacker.
    fn retreat(&mut self, defender: Id, from: Coord, rules: &Rules) -> bool {
        let unit = &self.units[&defender];
        let (position, owner) = (unit.position, unit.owner);
        let to = position.offset(position.x - from.x, position.y - from.y);
        let def = rules.def(unit);
        if !self.passable(def.domain, owner, to) || self.blocked_for(owner, to) {
            return false;
        }
        self.set_position(defender, to);
        let unit = self.units.get_mut(&defender).unwrap();
        unit.goto = None;
        true
    }

    fn check_target(
        &self,
        player: Id,
        id: Id,
        target: Coord,
        rules: &Rules,
        range: i32,
        ranged: bool,
    ) -> Result<Id> {
        let unit = self.units.get(&id).ok_or_else(|| error("Unknown unit"))?;
        if unit.owner != player {
            return Err(error("You do not command this unit"));
        }
        if unit.carrier.is_some() {
            return Err(error("Passengers must come ashore before they can fight"));
        }
        let distance = unit.position.distance(target);
        if distance == 0 || distance > range {
            return Err(error(if ranged {
                "Target is out of range"
            } else {
                "Target is not adjacent"
            }));
        }
        let def = rules.def(unit);
        if unit.moves_left(def) == 0 {
            return Err(error("No movement left"));
        }
        if unit.attacked && !def.blitz {
            return Err(error("This unit has already attacked this turn"));
        }
        let tile = self.map.get(target).ok_or_else(|| error("Off the map"))?;
        if def.domain == Domain::Land && !tile.is_land() {
            return Err(error("Land units cannot attack across water"));
        }
        self.hostile_holder(target, player)
            .ok_or_else(|| error("Nothing to attack there"))
    }

    /// Melee attack on an adjacent square: fight its best defender, or capture what has none.
    pub(crate) fn attack(
        &mut self,
        player: Id,
        id: Id,
        target: Coord,
        rules: &Rules,
    ) -> Result<()> {
        let holder = self.check_target(player, id, target, rules, 1, false)?;
        let unit = &self.units[&id];
        let def = rules.def(unit);
        if !def.can_attack() {
            return Err(error("This unit cannot attack"));
        }
        let defender = self.best_defender(target, holder, rules);
        if defender.is_none() && def.is_naval() {
            return Err(error("Ships cannot take prisoners or cities"));
        }
        let cost = MOVE_UNIT.min(unit.moves_left(def));
        let total_moves = def.total_moves();
        self.declare_war(player, holder);
        {
            let unit = self.units.get_mut(&id).unwrap();
            unit.attacked = true;
            unit.moves_used = (unit.moves_used + cost).min(total_moves);
            unit.clear_orders();
        }
        match defender {
            None => self.capture_tile(player, id, target, holder, rules),
            Some(defender) => self.duel(id, defender, rules),
        }
        Ok(())
    }

    /// Nothing defends the square: its defenseless units change sides, and a city falls.
    fn capture_tile(&mut self, player: Id, attacker: Id, target: Coord, holder: Id, rules: &Rules) {
        let victims: Vec<Id> = self
            .unit_ids_at(target)
            .into_iter()
            .filter(|id| self.units[id].owner == holder)
            .collect();
        // Whatever rode a ship in this port is ashore, and so shares the fate of the garrison.
        for id in &victims {
            self.release_passengers(*id);
        }
        let attacker_before = self.fighter(attacker, rules);
        let (mut taken, mut destroyed) = (Vec::new(), Vec::new());
        for id in victims {
            let unit = &self.units[&id];
            let before = self.fighter(id, rules);
            if rules.def(unit).is_defenseless() {
                let total = rules.def(unit).total_moves();
                let unit = self.units.get_mut(&id).unwrap();
                unit.owner = player;
                unit.clear_orders();
                unit.moves_used = total;
                taken.extend(before);
            } else {
                // Defended in principle but unable to fight here, such as a ship in port.
                self.remove_unit(id);
                destroyed.extend(before);
            }
        }
        let who = self.factions[&player].name.clone();
        let mut fallen = None;
        if let Some(city) = self
            .city_at(target)
            .filter(|c| self.cities[c].owner != player)
        {
            self.transfer_city(city, player);
            self.set_position(attacker, target);
            let name = self.cities[&city].name.clone();
            self.note(format!("{who} captured {name}."));
            fallen = Some(name);
        } else if !taken.is_empty() {
            self.note(format!(
                "{who} captured {} undefended unit(s).",
                taken.len()
            ));
        }
        if let Some(attacker) = attacker_before {
            self.record(Battle::Capture {
                attacker,
                target,
                advanced: fallen.is_some(),
                city: fallen,
                taken,
                destroyed,
            });
        }
    }

    /// A fight to the death or to a retreat.
    fn duel(&mut self, attacker: Id, defender: Id, rules: &Rules) {
        let (a_unit, d_unit) = (self.units[&attacker].clone(), self.units[&defender].clone());
        let (a_def, d_def) = (rules.def(&a_unit), rules.def(&d_unit));
        let target = d_unit.position;
        let odds = round_odds(
            a_def.attack,
            d_def.defense,
            self.defender_percent(&d_unit, d_def),
        );
        let (mut a_retreats, mut d_retreats) = (a_def.is_fast(), d_def.is_fast());
        if a_retreats && d_retreats {
            (a_retreats, d_retreats) = (false, false);
        }
        if d_retreats && self.city_at(target).is_some() {
            d_retreats = false;
        }
        let names = (self.name_of(&a_unit, rules), self.name_of(&d_unit, rules));
        // The two sides as the record shows them: before the support shot and every round.
        let (a_before, d_before) = (self.fighter(attacker, rules), self.fighter(defender, rules));
        // A fortified defender stays dug in; any other is woken by the attack.
        let woken = self.units.get_mut(&defender).unwrap();
        if woken.order != Order::Fortified {
            woken.clear_orders();
        }
        let support = self.defensive_bombard(attacker, defender, rules);
        let mut rounds = Vec::new();
        let outcome = loop {
            let attacker_won = self.die(DIE) as i32 >= odds;
            rounds.push(attacker_won);
            if attacker_won {
                self.damage(defender);
                let left = self.remaining(defender, rules);
                if left <= 0 {
                    break Outcome::AttackerWon;
                }
                let (a_rate, d_rate) = (a_unit.retreat_percent(), d_unit.retreat_percent());
                if d_retreats
                    && left == 1
                    && self.remaining(attacker, rules) > 1
                    && (self.die((a_rate + 50) as u32) as i32) < d_rate
                    && self.retreat(defender, a_unit.position, rules)
                {
                    break Outcome::DefenderRetreated;
                }
            } else {
                self.damage(attacker);
                let left = self.remaining(attacker, rules);
                if left <= 0 {
                    break Outcome::DefenderWon;
                }
                let (a_rate, d_rate) = (a_unit.retreat_percent(), d_unit.retreat_percent());
                if a_retreats
                    && left == 1
                    && self.remaining(defender, rules) > 1
                    && (self.die((d_rate + 50) as u32) as i32) < a_rate
                {
                    break Outcome::AttackerRetreated;
                }
            }
        };
        let mut promoted = false;
        let text = match outcome {
            Outcome::AttackerWon => {
                self.remove_unit(defender);
                promoted = self.promote(attacker);
                format!("{} defeated {}.", names.0, names.1)
            }
            Outcome::DefenderWon => {
                self.remove_unit(attacker);
                promoted = self.promote(defender);
                format!("{} was lost attacking {}.", names.0, names.1)
            }
            Outcome::DefenderRetreated => {
                format!("{} withdrew from {}.", names.1, names.0)
            }
            Outcome::AttackerRetreated => {
                format!("{} broke off the attack on {}.", names.0, names.1)
            }
        };
        self.note(text);
        if let (Some(attacker_f), Some(defender_f)) = (a_before, d_before) {
            let retreat_to = (outcome == Outcome::DefenderRetreated)
                .then(|| self.units[&defender].position);
            self.record(Battle::Duel {
                attacker: attacker_f,
                defender: defender_f,
                support,
                rounds,
                outcome,
                retreat_to,
                promoted,
            });
        }
    }

    /// The unit a bombardment from `shooter` hits on a square held by `holder`, if any. Only
    /// the best defender in reach is chosen; passengers and units without defense are never
    /// shot at, and a unit down to one hit point is spared unless the shooter is lethal
    /// against its domain. A ship shoots at ships first, including those tied up in port;
    /// a land bombardment reaches land units only.
    pub fn bombard_victim(
        &self,
        shooter: &UnitDef,
        target: Coord,
        holder: Id,
        rules: &Rules,
    ) -> Option<Id> {
        let land = self.map.get(target)?.is_land();
        if shooter.domain == Domain::Land && !land {
            return None;
        }
        let in_port = land && self.city_at(target).is_some();
        self.units_at(target)
            .into_iter()
            .filter(|u| u.owner == holder && u.carrier.is_none())
            .filter_map(|u| {
                let d = rules.def(u);
                let reachable = match d.domain {
                    Domain::Land => land,
                    Domain::Sea => !land || (in_port && shooter.is_naval()),
                };
                (reachable && d.defense > 0 && (u.hp(d) > 1 || shooter.lethal_against(d.domain)))
                    .then(|| {
                        let ship_first = shooter.is_naval() && d.is_naval();
                        (ship_first, self.defense_rating(u, d), Reverse(u.id), u.id)
                    })
            })
            .max()
            .map(|best| best.3)
    }

    /// 1024 times the chance a bombarded unit survives one shot. A ship tied up in port is a
    /// sitting duck: its odds are halved, as in Civ3.
    pub fn bombard_odds(&self, strength: i32, victim: &Unit, v_def: &UnitDef) -> i32 {
        let odds = round_odds(
            strength,
            v_def.defense,
            self.defender_percent(victim, v_def),
        );
        if v_def.is_naval() && self.city_at(victim.position).is_some() {
            (odds + 1) / 2
        } else {
            odds
        }
    }

    /// Ranged attack: volleys wound the best defender in reach. They stop at one hit point,
    /// unless the shooter is lethal against that kind of unit, in which case they can kill.
    pub(crate) fn bombard(
        &mut self,
        player: Id,
        id: Id,
        target: Coord,
        rules: &Rules,
    ) -> Result<()> {
        let range = rules
            .def(self.units.get(&id).ok_or_else(|| error("Unknown unit"))?)
            .range;
        let holder = self.check_target(player, id, target, rules, range.max(1), true)?;
        let unit = &self.units[&id];
        let def = rules.def(unit);
        if !def.can_bombard() {
            return Err(error("This unit cannot bombard"));
        }
        let victim = self
            .bombard_victim(def, target, holder, rules)
            .ok_or_else(|| error("Nothing there can be bombarded"))?;
        let (strength, shots, total_moves) = (def.bombard, def.rate_of_fire, def.total_moves());
        let name = self.name_of(unit, rules);
        self.declare_war(player, holder);
        {
            let unit = self.units.get_mut(&id).unwrap();
            unit.attacked = true;
            unit.moves_used = total_moves;
            unit.clear_orders();
        }
        let v = &self.units[&victim];
        let v_def = rules.def(v);
        let lethal = rules.def(&self.units[&id]).lethal_against(v_def.domain);
        let odds = self.bombard_odds(strength, v, v_def);
        let (shooter_before, target_before) = (self.fighter(id, rules), self.fighter(victim, rules));
        let (mut hits, mut killed) = (0, false);
        let mut volleys = Vec::new();
        for _ in 0..shots {
            let hit = self.die(DIE) as i32 >= odds;
            volleys.push(hit);
            if hit {
                self.damage(victim);
                hits += 1;
                let left = self.remaining(victim, rules);
                if left <= 0 {
                    killed = true;
                    break;
                }
                // Without a lethal bombardment, the volley ends at one hit point.
                if left == 1 && !lethal {
                    break;
                }
            }
        }
        let victim_name = self.name_of(&self.units[&victim], rules);
        let mut text = format!("{name} bombarded {victim_name}: {hits} hit(s) in {shots} shot(s).");
        let mut promoted = false;
        if killed {
            self.remove_unit(victim);
            promoted = self.promote(id);
            text.push_str(&format!(" {victim_name} was destroyed."));
        }
        self.note(text);
        if let (Some(shooter), Some(target)) = (shooter_before, target_before) {
            self.record(Battle::Bombard {
                shooter,
                target,
                shots: volleys,
                killed,
                promoted,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn odds_match_the_worked_example() {
        // Attack 8 against defense 4, fortified (25) on mountains (100): 1024 * 900 / 1700.
        assert_eq!(round_odds(8, 4, 125), 542);
        // Equal strength on flat ground favors the defender by its bonus.
        assert_eq!(round_odds(4, 4, 0), 512);
        assert_eq!(round_odds(4, 4, 10), 536);
        // A zero-attack attacker still wins one round in 1024; so does a zero-defense target.
        assert_eq!(round_odds(0, 4, 0), 1023);
        assert_eq!(round_odds(4, 0, 0), 1);
    }

    #[test]
    fn win_chance_is_a_sensible_probability() {
        let even = win_chance(512, 3, 3);
        assert!((even - 0.5).abs() < 1e-6, "{even}");
        assert!(win_chance(300, 3, 3) > 0.5);
        assert!(win_chance(700, 3, 3) < 0.5);
        assert!((win_chance(512, 1, 1) - 0.5).abs() < 1e-6);
        assert_eq!(win_chance(512, 3, 0), 1.0);
    }
}
