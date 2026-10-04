//! City citizens: native pool identity, nationality and individual work/jobs.
//! Slot allocation/removal is `population.rs`; radius indices use the native
//! spiral (`0x5E6E50`) transformed onto the clone's square map.
use std::collections::HashSet;
use civ3mapgen::population::{Citizen, Pool};
use crate::cities::{City, Specialist};
use crate::map::GameMap;

pub fn new_pool(civ: usize, size: u8) -> Pool {
    let mut pool = Pool::default();
    for _ in 0..size { pool.add(Citizen { race: crate::civs::roster_index(civ) as i32, work: 0, job: 0, resister: false }); }
    pool
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::MapRng;

    #[test]
    fn saved_holes_preserve_random_victims_jobs_and_birth_ids() {
        crate::realm::reset();
        let map = GameMap::generate();
        let mut city = City::new(0, "Town", 5, 5);
        city.set_size(4);
        city.food = 21;
        city.citizens.get_mut(0).unwrap().work = 1;
        *city.citizens.get_mut(1).unwrap() = Citizen {
            race: crate::civs::roster_index(1) as i32, work: 0, job: 3, resister: false,
        };
        city.citizens.get_mut(2).unwrap().work = 2;
        *city.citizens.get_mut(3).unwrap() = Citizen {
            race: crate::civs::roster_index(2) as i32, work: 0, job: 1, resister: true,
        };
        let mut rng = MapRng::new(1);
        let victim = city.lose_population(1, None, &mut rng).pop().unwrap();
        assert_eq!(victim.work, 2);
        assert_eq!(rng.state(), 1_103_527_590);
        assert_eq!(city.worked(&map), HashSet::from([city.work_at(&map, 1)]));
        assert_eq!(city.size(), 3);
        assert_eq!(city.food, 21);
        let mut loaded: City = serde_json::from_str(&serde_json::to_string(&city).unwrap()).unwrap();
        let mut loaded_rng = MapRng::new(rng.state());
        assert_eq!(loaded.citizens, city.citizens);
        for expected_job in [0, 3] {
            let a = city.lose_population(1, None, &mut rng);
            let b = loaded.lose_population(1, None, &mut loaded_rng);
            assert_eq!(a, b);
            assert_eq!(a[0].job, expected_job);
        }
        assert_eq!(rng.state(), 662_824_084);
        assert_eq!(loaded_rng.state(), rng.state());
        assert!(city.worked(&map).is_empty());
        assert!(!city.specialist_jobs().any(|s| s == Specialist::Scientist));
        city.add_citizens(1, crate::civs::roster_index(0));
        loaded.add_citizens(1, crate::civs::roster_index(0));
        assert!(city.citizens.slots()[1].is_some(), "last released slot is reused");
        assert_eq!(loaded.citizens, city.citizens);
        assert_eq!(loaded.food, city.food);
    }

    #[test]
    fn production_prefers_owner_nationals_and_inherits_the_last_foreign_race() {
        crate::realm::reset();
        let mut city = City::new(0, "Captured", 5, 5);
        city.set_size(3);
        let foreign = crate::civs::roster_index(1);
        city.citizens.get_mut(1).unwrap().race = foreign as i32;
        city.citizens.get_mut(2).unwrap().race = foreign as i32;
        let mut rng = MapRng::new(1);
        assert_eq!(city.pay_population_cost(2, &mut rng), foreign);
        assert_eq!(city.size(), 1);
        assert_eq!(city.nationals(0), 0);
        assert_eq!(city.nationals(1), 1);
        assert_eq!(rng.state(), 662_824_084, "owner success, owner failure, foreign success");
        let before = city.citizens.clone();
        city.civ = 2;
        assert_eq!(city.citizens, before, "ownership does not rewrite race or slots");
        assert_eq!(city.nationals(1), 1);
    }

    #[test]
    fn native_radius_indices_cover_the_city_radius_and_wrap_the_seam() {
        let map = GameMap::generate();
        let city = City::new(0, "Seam", 0, 5);
        let native: HashSet<_> = (1..=20).map(|i| city.work_at(&map, i)).collect();
        assert_eq!(native, crate::cities::radius_tiles(&map, 0, 5).into_iter().collect());
        assert!(native.contains(&(map.w - 1, 5)));
    }
}

pub(crate) mod pool_serde {
    use super::*;
    use serde::{Deserialize, Serialize};
    pub fn serialize<S: serde::Serializer>(pool: &Pool, s: S) -> Result<S::Ok, S::Error> {
        pool.to_words().serialize(s)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Pool, D::Error> {
        let words = Vec::<i64>::deserialize(d)?;
        let mut pool = Pool::default();
        if !pool.restore(&words) || pool.slots().iter().flatten().any(|c|
            !(0..31).contains(&c.race) || c.work > 20 || !(0..6).contains(&c.job)
            || c.work != 0 && c.job != 0)
            || pool.slots().iter().flatten().count() > u8::MAX as usize
        {
            return Err(serde::de::Error::custom("invalid city citizen pool"));
        }
        Ok(pool)
    }
}

fn job(s: Specialist) -> i32 {
    match s { Specialist::Entertainer => 1, Specialist::TaxCollector => 2, Specialist::Scientist => 3 }
}
fn specialist(job: i32) -> Specialist {
    match job { 2 => Specialist::TaxCollector, 3 => Specialist::Scientist, _ => Specialist::Entertainer }
}

impl City {
    pub fn size(&self) -> u8 { self.citizens.slots().iter().flatten().count() as u8 }

    pub fn add_citizens(&mut self, n: u8, race: usize) {
        for _ in 0..n.min(u8::MAX - self.size()) {
            self.citizens.add(Citizen { race: race as i32, work: 0, job: 0, resister: false });
        }
    }

    /// Debug setup only; gameplay births and losses use their own routines.
    pub fn set_size(&mut self, size: u8) {
        self.add_citizens(size.saturating_sub(self.size()), crate::civs::roster_index(self.civ));
        while self.size() > size {
            let i = self.citizens.slots().iter().rposition(|c| c.as_ref().is_some_and(|c| c.job == 0))
                .or_else(|| self.citizens.slots().iter().rposition(Option::is_some)).unwrap();
            self.citizens.remove(None, |_| i as i32);
        }
    }

    pub fn nationals(&self, civ: usize) -> u8 {
        let race = crate::civs::roster_index(civ) as i32;
        self.citizens.slots().iter().flatten().filter(|c| c.race == race).count() as u8
    }

    /// Each native removal pass consumes a slot draw, including failed race
    /// searches. Removing the record also releases its job and worked tile.
    pub fn lose_population(&mut self, n: u8, race: Option<i32>, rng: &mut crate::rng::MapRng) -> Vec<Citizen> {
        let granary = crate::citycalc::has_flag(self, crate::roster::imp::KEEPS_FOOD);
        let mut removed = Vec::new();
        for _ in 0..n {
            let before = self.size();
            if let Some((_, victim)) = self.citizens.remove(race, |n| rng.reference().below(n)) {
                self.food = civ3mapgen::population::food_after_loss(
                    i32::from(self.food), i32::from(before), i32::from(self.size()), granary, 10,
                ) as u8;
                removed.push(victim);
            }
        }
        removed
    }

    /// `0x4B8DC4..0x4B8EB1`: owner's race first, then other players in
    /// slot order. The unit adopts the last foreign race actually removed.
    pub fn pay_population_cost(&mut self, n: u8, rng: &mut crate::rng::MapRng) -> usize {
        let mut race = crate::civs::roster_index(self.civ);
        let n = n.min(self.size());
        let mut remaining = n - self.lose_population(n, Some(race as i32), rng).len() as u8;
        for civ in 0..crate::civs::CIV_COUNT {
            if remaining == 0 { break; }
            if civ == self.civ { continue; }
            let foreign = crate::civs::roster_index(civ);
            let removed = self.lose_population(remaining, Some(foreign as i32), rng);
            if !removed.is_empty() { race = foreign; }
            remaining -= removed.len() as u8;
        }
        race
    }

    fn work_at(&self, map: &GameMap, index: u8) -> (i32, i32) {
        let (x, y) = civ3mapgen::spiral::spiral_offset(i32::from(index));
        (map.wrap_x(self.x + (x + y) / 2), self.y + (y - x) / 2)
    }
    fn work_index(&self, map: &GameMap, tile: (i32, i32)) -> Option<u8> {
        (1..=20).find(|&i| self.work_at(map, i) == tile)
    }
    pub fn worked(&self, map: &GameMap) -> HashSet<(i32, i32)> {
        self.citizens.slots().iter().flatten().filter(|c| c.work != 0)
            .map(|c| self.work_at(map, c.work)).collect()
    }
    pub fn clear_worked(&mut self) {
        for i in 0..self.citizens.slots().len() {
            if let Some(c) = self.citizens.get_mut(i) { c.work = 0; }
        }
    }
    pub fn unwork(&mut self, map: &GameMap, tile: (i32, i32), entertainer: bool) -> bool {
        let Some(index) = self.work_index(map, tile) else { return false };
        let Some(i) = self.citizens.slots().iter().position(|c| c.as_ref().is_some_and(|c| c.work == index)) else { return false };
        let c = self.citizens.get_mut(i).unwrap();
        c.work = 0;
        if entertainer { c.job = 1; }
        true
    }
    pub fn work_tile(&mut self, map: &GameMap, tile: (i32, i32)) -> bool {
        let Some(index) = self.work_index(map, tile) else { return false };
        if self.worked(map).contains(&tile) { return false; }
        let slots = self.citizens.slots();
        let i = slots.iter().position(|c| c.as_ref().is_some_and(|c| c.work == 0 && c.job == 0))
            .or_else(|| slots.iter().position(|c| c.as_ref().is_some_and(|c| c.work == 0 && c.job == 1)))
            .or_else(|| slots.iter().rposition(|c| c.as_ref().is_some_and(|c| c.work == 0)));
        let Some(i) = i else { return false };
        let c = self.citizens.get_mut(i).unwrap(); c.work = index; c.job = 0;
        true
    }
    pub fn replace_work(&mut self, map: &GameMap, from: (i32, i32), to: (i32, i32)) {
        let (Some(from), Some(to)) = (self.work_index(map, from), self.work_index(map, to)) else { return };
        if let Some(i) = self.citizens.slots().iter().position(|c| c.as_ref().is_some_and(|c| c.work == from)) {
            self.citizens.get_mut(i).unwrap().work = to;
        }
    }
    pub fn specialists(&self) -> Vec<Specialist> {
        self.citizens.slots().iter().flatten().filter(|c| c.job != 0).map(|c| specialist(c.job)).collect()
    }
    fn idle_ids(&self) -> Vec<usize> {
        self.citizens.slots().iter().enumerate().filter_map(|(i, c)|
            c.as_ref().filter(|c| c.work == 0).map(|_| i)).collect()
    }
    pub fn specialist_jobs(&self) -> impl Iterator<Item = Specialist> + '_ {
        self.citizens.slots().iter().flatten().filter(|c| c.job != 0 && !c.resister).map(|c| specialist(c.job))
    }
    pub fn idle_job(&self, i: usize) -> Option<Specialist> {
        self.idle_ids().get(i).map(|&id| specialist(self.citizens.slots()[id].as_ref().unwrap().job))
    }
    pub fn entertain_unassigned(&mut self) {
        for i in 0..self.citizens.slots().len() {
            if let Some(c) = self.citizens.get_mut(i) && c.work == 0 && c.job == 0 { c.job = 1; }
        }
    }
    pub fn set_specialist(&mut self, i: usize, s: Specialist) {
        if let Some(&id) = self.idle_ids().get(i) { self.citizens.get_mut(id).unwrap().job = job(s); }
    }
    pub fn cycle_specialist(&mut self, i: usize) {
        let s = self.idle_job(i);
        if let Some(s) = s { self.set_specialist(i, s.next()); }
    }
    pub fn clear_specialists(&mut self) {
        for i in 0..self.citizens.slots().len() {
            if let Some(c) = self.citizens.get_mut(i) { c.job = 0; }
        }
    }
    #[cfg(test)]
    pub fn set_specialists(&mut self, jobs: Vec<Specialist>) {
        self.clear_specialists();
        for (i, job) in jobs.into_iter().enumerate() { self.set_specialist(i, job); }
    }
    #[cfg(test)]
    pub fn set_worked(&mut self, map: &GameMap, tiles: HashSet<(i32, i32)>) {
        self.clear_worked();
        for tile in tiles { self.work_tile(map, tile); }
    }
    #[cfg(test)]
    pub fn set_nationality(&mut self, civ: usize) {
        for i in 0..self.citizens.slots().len() {
            if let Some(c) = self.citizens.get_mut(i) { c.race = crate::civs::roster_index(civ) as i32; }
        }
    }
}
