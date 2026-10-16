//! Workers: improve the land the nation's cities gather from.
//!
//! An idle worker looks over the squares around it that its nation owns and picks the job
//! that adds the most for the days it costs (the walk there plus the work itself), then walks
//! over and starts it. A worker mid-job stays at it. Workers are captured when caught, so
//! one with an enemy nearby takes shelter in a city instead. Settlers with nowhere to settle
//! lend a hand too.
use super::{AI_NEAR_BUDGET, Realm};
use crate::economy::improvement_yield;
use crate::improvements::Job;
use crate::terrain::{Coord, Tile};
use crate::units::*;
use crate::{Command, Game, Id, Rules, TickRules};
use std::cmp::Reverse;
use std::collections::BTreeSet;

/// How far from itself (in tiles) a worker looks for something to improve. A city's land
/// reaches six tiles at most.
const WORKER_RADIUS: i32 = 6;
/// A worker keeps this far from enemy fighting units.
const WORKER_DANGER: i32 = 3;
/// What a unit of each resource is worth a day. Food grows a city and shields build, which
/// are what a city runs short of first; gold is worth half as much.
const FOOD_POINTS: i32 = 4;
const SHIELD_POINTS: i32 = 4;
const GOLD_POINTS: i32 = 2;
/// Candidate jobs a worker tries, best first, before it gives up for the day.
const WORKER_CHOICES: usize = 3;

impl Game {
    pub(super) fn ai_workers(&mut self, realm: &mut Realm, rules: &Rules) {
        let faction = realm.id;
        // Squares already being worked, or claimed by a worker today, are left to that worker.
        let mut taken: BTreeSet<Coord> = realm
            .units
            .iter()
            .filter_map(|id| self.units.get(id))
            .filter(|u| u.owner == faction && matches!(u.order, Order::Work(_)))
            .map(|u| u.position)
            .collect();
        let crew: Vec<Id> = realm
            .units
            .iter()
            .copied()
            .filter(|id| !realm.committed.contains(id))
            .filter(|id| {
                self.units.get(id).is_some_and(|u| {
                    u.owner == faction && u.carrier.is_none() && rules.def(u).can_work()
                })
            })
            .collect();
        for id in crew {
            self.ai_work(realm, id, &mut taken, rules);
            realm.committed.insert(id);
        }
    }

    fn ai_work(&mut self, realm: &Realm, id: Id, taken: &mut BTreeSet<Coord>, rules: &Rules) {
        let Some(unit) = self.units.get(&id).filter(|u| u.owner == realm.id).cloned() else {
            return;
        };
        let def = rules.def(&unit);
        let in_city = realm.post_at(unit.position).is_some();
        if realm.hostile_within(&self.map, unit.position, WORKER_DANGER) {
            if !in_city {
                self.ai_shelter(realm, id, rules);
            }
            return;
        }
        if let Order::Work(job) = unit.order
            && self.can_improve(unit.position, job).is_ok()
        {
            return; // the job goes on by itself each day
        }
        if unit.moves_left(def) == 0 {
            return;
        }
        for (target, job) in self.work_choices(realm, &unit, def, taken) {
            if target != unit.position
                && self.march_once(id, target, rules, AI_NEAR_BUDGET).is_err()
            {
                continue; // out of reach, such as across the water: try the next best
            }
            taken.insert(target);
            if self.units.get(&id).is_some_and(|u| u.position == target) {
                let _ = self.execute(
                    realm.id,
                    Command::Work { unit: id, job },
                    rules,
                    TickRules::default(),
                );
            }
            return;
        }
        // Nothing worth doing in reach: wait in a city, out of harm's way.
        if !in_city {
            self.ai_shelter(realm, id, rules);
        }
    }

    /// The best few jobs for `unit`, best first: squares of its own nation, scored by what
    /// the improvement adds each day it takes to walk there and build it.
    fn work_choices(
        &self,
        realm: &Realm,
        unit: &Unit,
        def: &UnitDef,
        taken: &BTreeSet<Coord>,
    ) -> Vec<(Coord, Job)> {
        let (rate, pace) = (def.work.max(1), def.moves.max(1) as i32);
        let mut found = Vec::new();
        for dy in -WORKER_RADIUS..=WORKER_RADIUS {
            for dx in -WORKER_RADIUS..=WORKER_RADIUS {
                let Some(p) = self.map.normalize(unit.position.offset(dx, dy)) else {
                    continue;
                };
                let Some(tile) = self.map.get(p) else {
                    continue;
                };
                if tile.owner != realm.id
                    || !tile.is_land()
                    || taken.contains(&p)
                    || self.blocked_for(realm.id, p)
                    || realm.hostile_within(&self.map, p, WORKER_DANGER)
                {
                    continue;
                }
                let distance = self.map.distance(unit.position, p);
                for job in Job::ALL {
                    let gain = job_gain(tile, job);
                    if gain <= 0 || self.can_improve(p, job).is_err() {
                        continue;
                    }
                    let days = self.work_required(p, job).div_ceil(rate) as i32 + distance / pace;
                    let score = gain * 64 / days.max(1);
                    found.push(((score, Reverse(distance), Reverse(p)), p, job));
                }
            }
        }
        found.sort_by(|a, b| b.0.cmp(&a.0));
        found
            .into_iter()
            .take(WORKER_CHOICES)
            .map(|(_, p, job)| (p, job))
            .collect()
    }
}

/// What `job` adds to a square each day, in points. A worker never tears down a mine to farm
/// or a farm to dig, so those count for nothing.
fn job_gain(tile: &Tile, job: Job) -> i32 {
    let rival = match job {
        Job::Mine => Tile::FARM,
        Job::Farm => Tile::MINE,
        Job::Road | Job::Rail => 0,
    };
    if tile.improvements & rival != 0 {
        return 0;
    }
    let before = improvement_yield(tile.improvements);
    let after = improvement_yield(tile.improvements | job.flag());
    (after.food - before.food) * FOOD_POINTS
        + (after.shields - before.shields) * SHIELD_POINTS
        + (after.gold - before.gold) * GOLD_POINTS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::{Cover, Relief, Terrain};

    fn tile(terrain: Terrain, relief: Relief) -> Tile {
        let mut tile = Tile::new(Coord::new(0, 0), terrain);
        tile.relief = relief;
        tile.cover = Cover::Bare;
        tile
    }

    #[test]
    fn jobs_are_worth_what_they_add() {
        let grass = tile(Terrain::Grass, Relief::Flat);
        assert_eq!(job_gain(&grass, Job::Farm), FOOD_POINTS);
        assert_eq!(job_gain(&grass, Job::Road), GOLD_POINTS);
        assert_eq!(job_gain(&grass, Job::Mine), 2 * SHIELD_POINTS);
        // A railroad adds a shield and a coin on top of its road, and only on top of one.
        let mut roaded = grass.clone();
        roaded.improvements = Tile::ROAD;
        assert_eq!(job_gain(&roaded, Job::Rail), SHIELD_POINTS + GOLD_POINTS);
        assert_eq!(job_gain(&roaded, Job::Road), 0, "already built");
        // Never undo one improvement for the other.
        let mut farmed = grass.clone();
        farmed.improvements = Tile::FARM;
        assert_eq!(job_gain(&farmed, Job::Mine), 0);
        let mut mined = grass;
        mined.improvements = Tile::MINE;
        assert_eq!(job_gain(&mined, Job::Farm), 0);
    }
}
