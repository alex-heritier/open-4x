//! Computer-controlled nations: garrison cities, found a few more, recruit, and make war only
//! on nations they are at war with. Their units obey the same rules as the player's.
use crate::units::*;
use crate::{Command, Game, Id, Rules, TickRules, terrain::Coord};
use std::collections::{BTreeMap, BTreeSet};

/// Computer-controlled forces only pursue targets this close and search a bounded number of
/// tiles, which keeps a turn cheap on a 512×342 world with a hundred nations.
const AI_RANGE: i32 = 48;
const AI_PATH_BUDGET: usize = 20_000;
const AI_SETTLE_RADIUS: i32 = 12;
const AI_MAX_CITIES: usize = 12;
/// Attack only when the chance of winning is at least this good.
const AI_MIN_WIN_CHANCE: f32 = 0.55;
/// A unit acts at most this many times per turn, so blitzing cavalry cannot spin forever.
const AI_ACTIONS: usize = 6;

impl Game {
    pub(crate) fn ai(&mut self, rules: &Rules) {
        let controlled: Vec<Id> = self
            .factions
            .keys()
            .copied()
            .filter(|id| *id != self.commander)
            .collect();
        let mut units_by_owner: BTreeMap<Id, Vec<Id>> = BTreeMap::new();
        for u in self.units.values() {
            units_by_owner.entry(u.owner).or_default().push(u.id);
        }
        let mut cities_by_owner: BTreeMap<Id, usize> = BTreeMap::new();
        for c in self.cities.values() {
            *cities_by_owner.entry(c.owner).or_default() += 1;
        }
        for faction in controlled {
            let enemies: BTreeSet<Id> = self
                .wars
                .iter()
                .filter_map(|&(a, b)| match faction {
                    f if f == a => Some(b),
                    f if f == b => Some(a),
                    _ => None,
                })
                .collect();
            let mut ids = units_by_owner.remove(&faction).unwrap_or_default();
            // Guns soften the target before anyone charges, and strikers move out before the
            // sturdier defenders, who are the ones left on guard.
            ids.retain(|id| self.units.contains_key(id)); // earlier nations may have killed some
            ids.sort_by_key(|id| {
                let def = rules.def(&self.units[id]);
                (!def.can_bombard(), std::cmp::Reverse(def.attack), *id)
            });
            let unit_count = ids.len();
            let city_count = cities_by_owner.get(&faction).copied().unwrap_or(0);
            let mut garrison = self.garrison_counts(faction, rules);
            for id in ids {
                if !self.units.contains_key(&id) {
                    continue;
                }
                self.ai_unit(faction, id, &enemies, &mut garrison, city_count, rules);
            }
            self.ai_recruit(faction, unit_count, city_count, !enemies.is_empty(), rules);
        }
    }

    /// Defending land units standing in each of a nation's cities.
    fn garrison_counts(&self, owner: Id, rules: &Rules) -> BTreeMap<Coord, usize> {
        let mut counts = BTreeMap::new();
        for city in self.cities.values().filter(|c| c.owner == owner) {
            let n = self
                .units_at(city.position)
                .into_iter()
                .filter(|u| u.owner == owner && u.carrier.is_none())
                .filter(|u| {
                    let d = rules.def(u);
                    d.domain == Domain::Land && d.can_attack()
                })
                .count();
            counts.insert(city.position, n);
        }
        counts
    }

    fn ai_unit(
        &mut self,
        faction: Id,
        id: Id,
        enemies: &BTreeSet<Id>,
        garrison: &mut BTreeMap<Coord, usize>,
        city_count: usize,
        rules: &Rules,
    ) {
        let unit = self.units[&id].clone();
        let def = rules.def(&unit);
        if unit.carrier.is_some() {
            return; // a passenger goes where its ship goes
        }
        if def.settler {
            if city_count >= AI_MAX_CITIES {
                return;
            }
            if let Some(site) = self.settlement_site(faction, unit.position) {
                if unit.position == site {
                    let name = format!(
                        "{} Port {}",
                        self.factions[&faction].adjective, self.next_id
                    );
                    let _ = self.execute(
                        faction,
                        Command::FoundCity { unit: id, name },
                        rules,
                        TickRules::default(),
                    );
                } else {
                    let _ = self.march_once(id, site, rules, AI_PATH_BUDGET);
                }
            }
            return;
        }
        if !def.can_attack() && !def.can_bombard() {
            return; // workers and the like
        }
        // The last defender of a city stays put.
        if let Some(count) = garrison.get_mut(&unit.position)
            && def.domain == Domain::Land
            && *count <= 1
        {
            if unit.order == Order::None {
                self.units.get_mut(&id).unwrap().order = Order::Fortifying;
            }
            return;
        }
        if enemies.is_empty() {
            if unit.order == Order::None && def.domain == Domain::Land {
                self.units.get_mut(&id).unwrap().order = Order::Fortifying;
            }
            return;
        }
        for _ in 0..AI_ACTIONS {
            let Some(unit) = self.units.get(&id).cloned() else {
                return; // lost in battle
            };
            if unit.moves_left(def) == 0 {
                return;
            }
            if self.ai_strike(faction, &unit, def, enemies, rules) {
                continue;
            }
            // Nothing to hit from here: close in on the nearest enemy.
            let Some(target) = self.nearest_enemy(&unit, def, enemies, rules) else {
                return;
            };
            if target.distance(unit.position) > AI_RANGE {
                return;
            }
            if let Some(count) = garrison.get_mut(&unit.position) {
                *count = count.saturating_sub(1);
            }
            match self.march_once(id, target, rules, AI_PATH_BUDGET) {
                Ok(true) => {}
                _ => return,
            }
        }
    }

    /// Attack or bombard the best adjacent target. Returns whether the unit acted.
    fn ai_strike(
        &mut self,
        faction: Id,
        unit: &Unit,
        def: &UnitDef,
        enemies: &BTreeSet<Id>,
        rules: &Rules,
    ) -> bool {
        let range = if def.can_bombard() { def.range } else { 1 };
        let mut best: Option<(f32, Coord, bool)> = None;
        for dy in -range..=range {
            for dx in -range..=range {
                let target = unit.position.offset(dx, dy);
                let adjacent = unit.position.distance(target) == 1;
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
                    if chance >= AI_MIN_WIN_CHANCE {
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
                .min_by_key(|u| u.position.distance(unit.position))
                .map(|u| u.position)
        } else {
            self.cities
                .values()
                .filter(|c| enemies.contains(&c.owner))
                .min_by_key(|c| c.position.distance(unit.position))
                .map(|c| c.position)
        }
    }

    /// Recruit only while the realm is below a garrison ceiling, so a hundred AI nations
    /// cannot grow the unit list without bound.
    fn ai_recruit(
        &mut self,
        faction: Id,
        units: usize,
        cities: usize,
        at_war: bool,
        rules: &Rules,
    ) {
        if units >= 4 + 3 * cities {
            return;
        }
        let soldier = |wanted: &str| {
            rules
                .units
                .get(wanted)
                .filter(|d| d.domain == Domain::Land && !d.settler)
                .map(|d| d.id.clone())
        };
        let infantry = soldier("infantry").or_else(|| {
            rules
                .units
                .values()
                .find(|d| d.domain == Domain::Land && d.can_attack() && d.defense > 0)
                .map(|d| d.id.clone())
        });
        let turn = self.turn;
        for c in self
            .cities
            .values_mut()
            .filter(|c| c.owner == faction && c.production.is_none())
        {
            // At war, mix in cavalry and artillery; otherwise just garrison.
            let wanted = match (at_war, (turn + c.id) % 4) {
                (true, 1) => soldier("cavalry"),
                (true, 2) => soldier("artillery"),
                _ => None,
            };
            c.production = wanted.or_else(|| infantry.clone());
        }
    }

    /// Nearest land tile within reach that keeps three tiles from every city.
    fn settlement_site(&self, owner: Id, from: Coord) -> Option<Coord> {
        let mut best: Option<(i32, Coord)> = None;
        for y in (from.y - AI_SETTLE_RADIUS).max(0)
            ..=(from.y + AI_SETTLE_RADIUS).min(self.map.height - 1)
        {
            for x in (from.x - AI_SETTLE_RADIUS).max(0)
                ..=(from.x + AI_SETTLE_RADIUS).min(self.map.width - 1)
            {
                let p = Coord::new(x, y);
                let distance = p.distance(from);
                if best.is_some_and(|(d, _)| d <= distance) {
                    continue;
                }
                let free = self
                    .map
                    .get(p)
                    .is_some_and(|t| t.is_land() && (t.owner == 0 || t.owner == owner))
                    && self.cities.values().all(|c| c.position.distance(p) >= 3);
                if free {
                    best = Some((distance, p));
                }
            }
        }
        best.map(|(_, p)| p)
    }
}

/// Keep the more promising of two strike candidates.
fn keep_best(best: &mut Option<(f32, Coord, bool)>, candidate: (f32, Coord, bool)) {
    if best.is_none_or(|current| candidate.0 > current.0) {
        *best = Some(candidate);
    }
}
