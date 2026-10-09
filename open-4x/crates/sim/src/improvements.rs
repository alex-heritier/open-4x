//! Worker improvements: roads, railroads, mines, and farms.
//!
//! Jobs follow Civ3: a job takes a fixed number of work-turns times the terrain's move cost,
//! every unit on the tile working the same job adds its rate each turn, and an unfinished job
//! is lost when the unit leaves. Improvements change movement only; nothing produces yet.
use crate::{
    Game, Id, Rules,
    terrain::{Coord, Cover, Terrain, Tile},
    units::Order,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Job {
    Road,
    Rail,
    Mine,
    Farm,
}
impl Job {
    pub const ALL: [Job; 4] = [Job::Road, Job::Rail, Job::Mine, Job::Farm];
    pub fn name(self) -> &'static str {
        match self {
            Self::Road => "Road",
            Self::Rail => "Railroad",
            Self::Mine => "Mine",
            Self::Farm => "Farm",
        }
    }
    /// Work-turns on flat ground; rougher terrain multiplies this by its move cost.
    pub fn turns(self) -> u32 {
        match self {
            Self::Road => 6,
            Self::Rail => 12,
            Self::Mine => 12,
            Self::Farm => 8,
        }
    }
    /// The tile bit this job builds.
    pub fn flag(self) -> u8 {
        match self {
            Self::Road => Tile::ROAD,
            Self::Rail => Tile::RAIL,
            Self::Mine => Tile::MINE,
            Self::Farm => Tile::FARM,
        }
    }
}

impl Game {
    /// Whether `job` can be started on this square, ignoring who is asking.
    pub fn can_improve(&self, position: Coord, job: Job) -> Result<(), &'static str> {
        let tile = self.map.get(position).ok_or("Off the map")?;
        if !tile.is_land() {
            return Err("Water cannot be improved");
        }
        if self.city_at(position).is_some() {
            return Err("Cities need no improvements");
        }
        if tile.improvements & job.flag() != 0 {
            return Err("Already built here");
        }
        match job {
            Job::Road => {}
            Job::Rail if !tile.has_road() => return Err("A railroad needs a road first"),
            Job::Rail => {}
            Job::Mine if tile.cover != Cover::Bare => {
                return Err("Mines cannot be dug under forest, jungle, or marsh");
            }
            Job::Mine
                if !tile.is_mountain() && !tile.is_hills() && tile.terrain != Terrain::Desert =>
            {
                return Err("Mines need hills, mountains, or desert");
            }
            Job::Mine => {}
            Job::Farm if tile.cover != Cover::Bare || tile.is_mountain() => {
                return Err("Farms need open ground");
            }
            Job::Farm if !self.has_water_source(position) => {
                return Err("Farms need water: a coast, lake, farm, or city nearby");
            }
            Job::Farm => {}
        }
        Ok(())
    }
    /// Work needed to finish `job` on this square.
    pub fn work_required(&self, position: Coord, job: Job) -> u32 {
        job.turns() * self.map.get(position).map_or(1, Tile::move_cost)
    }
    /// Irrigation needs a river along the square, adjacent water, an adjacent farm, or an
    /// adjacent city that itself stands by water.
    fn has_water_source(&self, position: Coord) -> bool {
        let by_water = |p: Coord| {
            self.map.river_mask(p) != 0
                || Coord::NEIGHBORS
                    .iter()
                    .any(|&(dx, dy)| self.map.get(p.offset(dx, dy)).is_some_and(|t| !t.is_land()))
        };
        by_water(position)
            || Coord::NEIGHBORS.iter().any(|&(dx, dy)| {
                let p = position.offset(dx, dy);
                self.map
                    .get(p)
                    .is_some_and(|t| t.improvements & Tile::FARM != 0)
                    || (self.city_at(p).is_some() && by_water(p))
            })
    }

    /// Start (or continue) a job and apply the first day's work if the unit can still act.
    pub(crate) fn start_work(&mut self, id: Id, job: Job, rules: &Rules) {
        let unit = self.units.get_mut(&id).expect("unit exists");
        let changed = unit.order != Order::Work(job);
        unit.goto = None;
        unit.order = Order::Work(job);
        if changed {
            unit.work = 0;
        }
        if unit.moves_used < rules.def(unit).total_moves() {
            self.work_unit(id, rules);
        }
    }

    /// One day of work by one unit; finishes the job when the tile's workers have done enough.
    fn work_unit(&mut self, id: Id, rules: &Rules) {
        let Some(unit) = self.units.get(&id) else {
            return;
        };
        let Order::Work(job) = unit.order else {
            return;
        };
        let (position, owner) = (unit.position, unit.owner);
        let def = rules.def(unit);
        let (rate, total_moves) = (def.work.max(1), def.total_moves());
        if self.can_improve(position, job).is_err() {
            self.units.get_mut(&id).unwrap().clear_orders();
            return;
        }
        let unit = self.units.get_mut(&id).unwrap();
        unit.work += rate;
        unit.moves_used = total_moves;
        // Every worker on the tile counts, whatever its nation.
        let team: Vec<Id> = self
            .unit_ids_at(position)
            .into_iter()
            .filter(|w| self.units[w].order == Order::Work(job))
            .collect();
        let done: u32 = team.iter().map(|w| self.units[w].work).sum();
        if done < self.work_required(position, job) {
            return;
        }
        for worker in team {
            self.units.get_mut(&worker).unwrap().clear_orders();
        }
        let tile = self.map.get_mut(position).unwrap();
        tile.improvements |= job.flag();
        match job {
            Job::Mine => tile.improvements &= !Tile::FARM,
            Job::Farm => tile.improvements &= !Tile::MINE,
            Job::Road | Job::Rail => {}
        }
        if owner == self.commander {
            self.note(format!(
                "{} completed at {},{}.",
                job.name(),
                position.x,
                position.y
            ));
        }
    }

    /// Workers add a day of work as each turn begins.
    pub(crate) fn work_turn(&mut self, rules: &Rules) {
        let working: Vec<Id> = self
            .units
            .values()
            .filter(|u| matches!(u.order, Order::Work(_)))
            .map(|u| u.id)
            .collect();
        for id in working {
            self.work_unit(id, rules);
        }
    }

    /// Work still needed per job for a unit's tile, for the interface.
    pub fn job_options(&self, position: Coord) -> BTreeMap<Job, Result<u32, &'static str>> {
        Job::ALL
            .into_iter()
            .map(|job| {
                (
                    job,
                    self.can_improve(position, job)
                        .map(|()| self.work_required(position, job)),
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_costs_follow_civ3() {
        assert_eq!(Job::ALL.map(Job::turns), [6, 12, 12, 8]);
        assert_eq!(Job::Rail.flag(), Tile::RAIL);
    }
}
