//! Soldiers: hold the cities, reinforce the ones that fall short, and fight the nations at war.
//!
//! The garrison pass comes first. Each guard, strongest first, either stays because its city
//! would fall below its goal without it, or is spare. A spare guard marches to the nearest
//! city in reach that still lacks defenders. Guards with nowhere to go, guns, and ships make
//! up the campaign pass: they strike what they can reach, march on the nearest enemy, and
//! otherwise stand fortified in a city.
use super::{
    AI_ACTIONS, AI_HOME_RANGE, AI_NEAR_BUDGET, AI_PATH_BUDGET, AI_RANGE, Realm, is_fighter,
    is_guard,
};
use crate::units::*;
use crate::{Command, Game, Id, Rules, TickRules, terrain::Coord};
use std::cmp::Reverse;
use std::collections::BTreeSet;

/// Attack only when the chance of winning is at least this good.
const AI_MIN_WIN_CHANCE: f32 = 0.55;
/// A unit the city cannot spare attacks only when it is all but sure to win, because losing
/// it loses the city's defense.
const AI_SAFE_WIN_CHANCE: f32 = 0.75;
/// Spare guards walk this far (in tiles) to fill a gap; farther cities build their own.
const REINFORCE_RANGE: i32 = 12;

impl Game {
    /// Guards hold their cities, and spare ones fill the gaps in other cities.
    pub(super) fn ai_garrison(&mut self, realm: &mut Realm, rules: &Rules) {
        let faction = realm.id;
        let mut guards: Vec<Id> = realm
            .units
            .iter()
            .copied()
            .filter(|id| {
                self.units.get(id).is_some_and(|u| {
                    u.owner == faction && u.carrier.is_none() && is_guard(rules.def(u))
                })
            })
            .collect();
        guards.sort_by_key(|id| (Reverse(rules.def(&self.units[id]).defense), *id));
        for id in guards {
            let Some(unit) = self.units.get(&id).filter(|u| u.owner == faction).cloned() else {
                continue; // lost in an earlier fight today
            };
            let def = rules.def(&unit);
            let home = realm.post_at(unit.position);
            if let Some(i) = home {
                if realm.posts[i].held - def.defense < realm.posts[i].wanted {
                    // The city cannot do without it.
                    realm.committed.insert(id);
                    self.stand_guard(realm, &unit, def, rules);
                    continue;
                }
                // Spare: the city no longer counts on it, whether it leaves or not, so the
                // guards weighed after it are measured against what is really left.
                realm.posts[i].held -= def.defense;
            }
            let home_at = home.map(|i| realm.posts[i].position);
            let Some(j) = realm.nearest_post(&self.map, unit.position, REINFORCE_RANGE, |post| {
                post.deficit() > 0 && Some(post.position) != home_at
            }) else {
                continue; // nowhere needs it: it is free for the campaign
            };
            let goal = realm.posts[j].position;
            if !matches!(self.march_once(id, goal, rules, AI_NEAR_BUDGET), Ok(true)) {
                continue;
            }
            realm.committed.insert(id);
            if self.units.get(&id).is_some_and(|u| u.position == goal) {
                realm.posts[j].held += def.defense;
                self.ai_fortify(faction, id, rules);
            } else {
                realm.posts[j].inbound += def.defense;
            }
        }
    }

    /// A guard the city cannot spare digs in, and hits an enemy at the gate only if sure to win.
    fn stand_guard(&mut self, realm: &Realm, unit: &Unit, def: &UnitDef, rules: &Rules) {
        if !realm.enemies.is_empty() && unit.moves_left(def) > 0 {
            self.ai_strike(
                realm.id,
                unit,
                def,
                &realm.enemies,
                AI_SAFE_WIN_CHANCE,
                rules,
            );
        }
        self.ai_fortify(realm.id, unit.id, rules);
    }

    /// Everything that is not tied to a city's defense goes to war, or home.
    pub(super) fn ai_campaign(&mut self, realm: &mut Realm, rules: &Rules) {
        let faction = realm.id;
        let mut free: Vec<Id> = realm
            .units
            .iter()
            .copied()
            .filter(|id| !realm.committed.contains(id))
            .filter(|id| {
                self.units.get(id).is_some_and(|u| {
                    u.owner == faction && u.carrier.is_none() && is_fighter(rules.def(u))
                })
            })
            .collect();
        // Guns soften the target before anyone charges, and strikers move out before the
        // sturdier defenders.
        free.sort_by_key(|id| {
            let def = rules.def(&self.units[id]);
            (!def.can_bombard(), Reverse(def.attack), *id)
        });
        for id in free {
            self.ai_free_unit(realm, id, rules);
        }
    }

    fn ai_free_unit(&mut self, realm: &Realm, id: Id, rules: &Rules) {
        let faction = realm.id;
        let Some(first) = self.units.get(&id).filter(|u| u.owner == faction) else {
            return;
        };
        let def = rules.def(first);
        let mut acted = false;
        if !realm.enemies.is_empty() {
            for _ in 0..AI_ACTIONS {
                let Some(unit) = self.units.get(&id).filter(|u| u.owner == faction).cloned() else {
                    return; // lost in battle
                };
                if unit.moves_left(def) == 0 {
                    return;
                }
                if self.ai_strike(
                    faction,
                    &unit,
                    def,
                    &realm.enemies,
                    AI_MIN_WIN_CHANCE,
                    rules,
                ) {
                    acted = true;
                    continue;
                }
                // Nothing to hit from here: close in on the nearest enemy.
                let Some(target) = self.nearest_enemy(&unit, def, &realm.enemies, rules) else {
                    break;
                };
                if self.map.distance(target, unit.position) > AI_RANGE {
                    break;
                }
                match self.march_once(id, target, rules, AI_PATH_BUDGET) {
                    Ok(true) => acted = true,
                    _ => break,
                }
            }
        }
        if acted || def.is_naval() {
            return;
        }
        // At peace, or with nobody to march on: a soldier in the open goes home to a city,
        // and one in a city digs in.
        let Some(unit) = self.units.get(&id).filter(|u| u.owner == faction) else {
            return;
        };
        if realm.post_at(unit.position).is_none()
            && let Some(home) =
                realm.nearest_post(&self.map, unit.position, AI_HOME_RANGE, |_| true)
            && matches!(
                self.march_once(id, realm.posts[home].position, rules, AI_NEAR_BUDGET),
                Ok(true)
            )
        {
            return;
        }
        self.ai_fortify(faction, id, rules);
    }

    /// Dig in, unless the unit already has something to do.
    pub(super) fn ai_fortify(&mut self, faction: Id, id: Id, rules: &Rules) {
        if self.units.get(&id).is_some_and(|u| u.order == Order::None) {
            // Units that cannot fortify, such as guns, simply stand.
            let _ = self.execute(
                faction,
                Command::Fortify { unit: id },
                rules,
                TickRules::default(),
            );
        }
    }

    /// Attack or bombard the best adjacent target that `min_chance` allows. Returns whether
    /// the unit acted.
    fn ai_strike(
        &mut self,
        faction: Id,
        unit: &Unit,
        def: &UnitDef,
        enemies: &BTreeSet<Id>,
        min_chance: f32,
        rules: &Rules,
    ) -> bool {
        let range = if def.can_bombard() { def.range } else { 1 };
        let mut best: Option<(f32, Coord, bool)> = None;
        for dy in -range..=range {
            for dx in -range..=range {
                let Some(target) = self.map.normalize(unit.position.offset(dx, dy)) else {
                    continue;
                };
                let adjacent = self.map.distance(unit.position, target) == 1;
                if (dx, dy) == (0, 0) || self.map.get(target).is_none() {
                    continue;
                }
                let Some(holder) = self.hostile_holder(target, faction) else {
                    continue;
                };
                if !enemies.contains(&holder) {
                    continue;
                }
                if def.can_bombard() {
                    if !unit.attacked && self.bombard_victim(def, target, holder, rules).is_some() {
                        keep_best(&mut best, (1.0, target, true));
                    }
                } else if adjacent && def.can_attack() {
                    let chance = match self.estimate_attack(unit.id, target, rules) {
                        Some(e) => e.win_chance,
                        None if def.domain == Domain::Land => 1.0, // nothing defends it
                        None => continue,
                    };
                    if chance >= min_chance {
                        keep_best(&mut best, (chance, target, false));
                    }
                }
            }
        }
        let Some((_, target, bombard)) = best else {
            return false;
        };
        let command = if bombard {
            Command::Bombard {
                unit: unit.id,
                target,
            }
        } else {
            Command::Attack {
                unit: unit.id,
                target,
            }
        };
        self.execute(faction, command, rules, TickRules::default())
            .is_ok()
    }

    /// The nearest thing worth marching toward: an enemy city for soldiers on land, enemy
    /// ships at sea for ships (those in port are out of reach).
    fn nearest_enemy(
        &self,
        unit: &Unit,
        def: &UnitDef,
        enemies: &BTreeSet<Id>,
        rules: &Rules,
    ) -> Option<Coord> {
        if def.is_naval() {
            self.units
                .values()
                .filter(|u| {
                    enemies.contains(&u.owner)
                        && rules.def(u).is_naval()
                        && self.map.get(u.position).is_some_and(|t| !t.is_land())
                })
                .min_by_key(|u| self.map.distance(u.position, unit.position))
                .map(|u| u.position)
        } else {
            self.cities
                .values()
                .filter(|c| enemies.contains(&c.owner))
                .min_by_key(|c| self.map.distance(c.position, unit.position))
                .map(|c| c.position)
        }
    }

    /// Walk to the nearest of the nation's cities, if one is close.
    pub(super) fn ai_shelter(&mut self, realm: &Realm, id: Id, rules: &Rules) {
        let Some(unit) = self.units.get(&id) else {
            return;
        };
        if let Some(home) = realm.nearest_post(&self.map, unit.position, AI_HOME_RANGE, |_| true) {
            let _ = self.march_once(id, realm.posts[home].position, rules, AI_NEAR_BUDGET);
        }
    }
}

/// Keep the more promising of two strike candidates.
fn keep_best(best: &mut Option<(f32, Coord, bool)>, candidate: (f32, Coord, bool)) {
    if best.is_none_or(|current| candidate.0 > current.0) {
        *best = Some(candidate);
    }
}
