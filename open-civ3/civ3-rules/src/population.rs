//! Citizen pool selection and food storage after removal. Caller effects
//! (nationality counters, worked tiles, specialists and resistance messages)
//! depend on the returned citizen and stay outside here.

/// Fields consulted by the removal routine, with native offsets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Citizen {
    /// RACE row.
    pub race: i32,
    /// Worked-radius index; zero means no worked tile.
    pub work: u8,
    /// Citizen job; compare with RULE's ordinary worker job.
    pub job: i32,
    /// Resistance byte.
    pub resister: bool,
    /// Turn the current race began.
    pub since: i32,
    /// Pending race change, -1 none (see `resistance`).
    pub pending_race: i32,
    /// Turn the pending change was recorded, -1 none.
    pub pending_turn: i32,
}

impl Citizen {
    /// A new citizen of `race`, born on `turn`, with no pending change.
    pub fn new(race: i32, turn: i32) -> Self {
        Citizen {
            race,
            work: 0,
            job: 0,
            resister: false,
            since: turn,
            pending_race: -1,
            pending_turn: -1,
        }
    }
}

/// Native citizen slots through the highest index, including empty slots. Freed
/// indices retain LIFO order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pool {
    slots: Vec<Option<Citizen>>,
    free: Vec<usize>,
}

impl Pool {
    /// Reuse the free head or add a new slot.
    pub fn add(&mut self, citizen: Citizen) -> usize {
        if let Some(i) = self.free.pop() {
            self.slots[i] = Some(citizen);
            i
        } else {
            self.slots.push(Some(citizen));
            self.slots.len() - 1
        }
    }

    /// Slots, preserving holes and stable citizen ids for caller bookkeeping.
    pub fn slots(&self) -> &[Option<Citizen>] {
        &self.slots
    }

    /// Edit an occupied citizen without changing slot/free-list identity.
    pub fn get_mut(&mut self, slot: usize) -> Option<&mut Citizen> {
        self.slots.get_mut(slot)?.as_mut()
    }

    /// Clone save stream: slot count, presence and citizen fields in slot order,
    /// then the free indices from oldest to newest.
    pub fn to_words(&self) -> Vec<i64> {
        let mut w = crate::words::Writer::default();
        w.put(self.slots.len() as i64);
        for slot in &self.slots {
            w.flag(slot.is_some());
            if let Some(c) = slot {
                w.put(c.race);
                w.put(c.work);
                w.put(c.job);
                w.flag(c.resister);
                w.put(c.since);
                w.put(c.pending_race);
                w.put(c.pending_turn);
            }
        }
        w.put(self.free.len() as i64);
        for &i in &self.free {
            w.put(i as i64);
        }
        w.0
    }

    /// Restore the clone stream atomically. Every hole must occur exactly once
    /// in the free list; trailing, truncated or out-of-range data are rejected.
    pub fn restore(&mut self, words: &[i64]) -> bool {
        let mut r = crate::words::Reader::new(words);
        let parsed = (|| {
            let n = usize::try_from(r.get()?).ok()?;
            if n > words.len() {
                return None;
            }
            let mut slots = Vec::with_capacity(n);
            for _ in 0..n {
                slots.push(match r.get()? {
                    0 => None,
                    1 => Some(Citizen {
                        race: r.i32()?,
                        work: u8::try_from(r.get()?).ok()?,
                        job: r.i32()?,
                        resister: match r.get()? {
                            0 => false,
                            1 => true,
                            _ => return None,
                        },
                        since: r.i32()?,
                        pending_race: r.i32()?,
                        pending_turn: r.i32()?,
                    }),
                    _ => return None,
                });
            }
            let free = r.run(|v| usize::try_from(v).ok())?;
            if !r.done() || free.len() != slots.iter().filter(|s| s.is_none()).count() {
                return None;
            }
            let mut seen = vec![false; n];
            for &i in &free {
                if i >= n || slots[i].is_some() || seen[i] {
                    return None;
                }
                seen[i] = true;
            }
            Some(Pool { slots, free })
        })();
        let Some(pool) = parsed else { return false };
        *self = pool;
        true
    }

    /// One removal pass. Always consumes a draw, even with no matching race or
    /// an empty pool. Selection is cyclic from a random slot, not uniform.
    pub fn remove(
        &mut self,
        race: Option<i32>,
        mut roll: impl FnMut(u32) -> i32,
    ) -> Option<(usize, Citizen)> {
        let n = self.slots.len();
        let start = (roll(n as u32) & 0xffff) as usize;
        for offset in 0..n {
            let i = (start + offset) % n;
            if self.slots[i].as_ref().is_some_and(|c| race.is_none_or(|r| c.race == r)) {
                let citizen = self.slots[i].take().unwrap();
                self.free.push(i);
                return Some((i, citizen));
            }
        }
        None
    }
}

/// Only a size-class change alters food storage. A Granary caps it at half the
/// new food box; without one it becomes zero. `box_unit` is the caller's box
/// unit (human/AI/acceleration).
pub fn food_after_loss(food: i32, before: i32, after: i32, granary: bool, box_unit: i32) -> i32 {
    use crate::economy::{size_class, CITY_MAX, TOWN_MAX};
    let old = size_class(before, TOWN_MAX, CITY_MAX);
    let new = size_class(after, TOWN_MAX, CITY_MAX);
    if old == new {
        food
    } else if granary {
        food.min(box_unit * (new + 1))
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn citizen(race: i32) -> Citizen {
        Citizen { work: 1, ..Citizen::new(race, 0) }
    }
    fn pool() -> Pool {
        let mut p = Pool::default();
        for race in [1, 9, 2, 1] {
            p.add(citizen(race));
        }
        p.remove(None, |_| 1).unwrap();
        p
    }

    #[test]
    fn cyclic_selection_skips_holes_and_wrong_races() {
        for (start, expected) in [(0, 0), (1, 3), (2, 3), (3, 3)] {
            assert_eq!(
                pool().remove(Some(1), |n| {
                    assert_eq!(n, 4);
                    start
                }).unwrap().0,
                expected
            );
        }
        assert_eq!(pool().remove(None, |_| 1).unwrap().0, 2);
    }

    #[test]
    fn failed_removals_consume_draws_and_keep_slots() {
        let mut p = pool();
        let before = p.slots().to_vec();
        let mut draws = 0;
        assert!(p.remove(Some(7), |_| {
            draws += 1;
            0
        }).is_none());
        assert_eq!(draws, 1);
        assert_eq!(p.slots(), before);
    }

    #[test]
    fn released_slots_are_reused_lifo_without_compacting_the_pool() {
        let mut p = pool();
        p.remove(None, |_| 2).unwrap();
        p.remove(None, |_| 3).unwrap();
        assert_eq!(p.slots().len(), 4);
        assert_eq!(p.add(citizen(8)), 3);
        assert_eq!(p.add(citizen(8)), 2);
        assert_eq!(p.add(citizen(8)), 1);
        assert_eq!(p.add(citizen(8)), 4);
    }

    #[test]
    fn food_tracks_size_class_and_owner_box_unit() {
        assert_eq!(food_after_loss(25, 7, 6, true, 10), 10);
        assert_eq!(food_after_loss(25, 7, 6, false, 10), 0);
        assert_eq!(food_after_loss(35, 13, 12, true, 10), 20);
        assert_eq!(food_after_loss(25, 5, 4, false, 10), 25);
        assert_eq!(food_after_loss(25, 7, 6, true, 5), 5);
    }

    #[test]
    fn restoring_keeps_victim_choice_and_future_free_slot_reuse() {
        let mut original = pool();
        original.remove(None, |_| 2).unwrap();
        let mut restored = Pool::default();
        assert!(restored.restore(&original.to_words()));
        assert_eq!(restored, original);
        for start in 0..4 {
            assert_eq!(
                restored.clone().remove(None, |_| start),
                original.clone().remove(None, |_| start)
            );
        }
    }

    #[test]
    fn invalid_streams_never_replace_a_valid_pool() {
        let mut original = pool();
        original.remove(None, |_| 2).unwrap();
        let good = original.to_words();
        let mut bad = Vec::new();
        for len in 0..good.len() {
            bad.push(good[..len].to_vec());
        }
        let mut extra = good.clone();
        extra.push(0);
        bad.push(extra);
        let mut beyond_last = good.clone();
        *beyond_last.last_mut().unwrap() = 4;
        bad.push(beyond_last);
        for words in bad {
            let mut restored = original.clone();
            assert!(!restored.restore(&words), "invalid stream accepted");
            assert_eq!(restored, original);
        }
    }
}
