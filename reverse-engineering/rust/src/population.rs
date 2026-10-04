//! Citizen pool selection (`0x4BA230`) and food storage after removal.
//! Caller effects (nationality counters, worked tiles, specialists and
//! resistance messages) depend on the returned citizen and stay outside here.

/// Fields consulted by the removal routine, with native offsets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Citizen {
    /// RACE row at +0x140.
    pub race: i32,
    /// Worked-radius index at +0x21; zero means no worked tile.
    pub work: u8,
    /// Citizen job at +0x13C; compare with RULE's ordinary worker job.
    pub job: i32,
    /// Resistance byte at +0x20.
    pub resister: bool,
    /// Turn the current race began, +0x130 (`0x4ABD90` stamps the birth turn).
    pub since: i32,
    /// Pending race change, +0x144 (-1 none; `resistance.rs`).
    pub pending_race: i32,
    /// Turn the pending change was recorded, +0x148 (-1 none).
    pub pending_turn: i32,
}

impl Citizen {
    /// `0x4ABD90`: a new citizen of `race`, born on `turn`, with no pending change.
    pub fn new(race: i32, turn: i32) -> Self {
        Citizen { race, work: 0, job: 0, resister: false, since: turn, pending_race: -1, pending_turn: -1 }
    }
}

/// Native citizen slots through the highest index, including empty slots.
/// Heap capacity/pointers are not modelled; freed indices retain LIFO order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pool {
    slots: Vec<Option<Citizen>>,
    free: Vec<usize>,
}

impl Pool {
    /// `0x4B9F98` calls allocator `0x4C2040`: reuse the free head
    /// (`0x4C2114..0x4C212C`) or increment last (`0x4C2131..0x4C213E`).
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
    pub fn slots(&self) -> &[Option<Citizen>] { &self.slots }

    /// Edit an occupied citizen without changing slot/free-list identity.
    pub fn get_mut(&mut self, slot: usize) -> Option<&mut Citizen> {
        self.slots.get_mut(slot)?.as_mut()
    }

    /// Clone save stream: slot count, presence and citizen fields in slot
    /// order, then the free indices from oldest to newest. This preserves
    /// native allocation/selection history; it is not a native SAV chunk.
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
        for &i in &self.free { w.put(i as i64); }
        w.0
    }

    /// Restore the clone stream atomically. Every hole must occur exactly
    /// once in the free list; occupied slots cannot be freed, and trailing,
    /// truncated or out-of-range field/index data are rejected.
    pub fn restore(&mut self, words: &[i64]) -> bool {
        let mut r = crate::words::Reader::new(words);
        let parsed = (|| {
            let n = usize::try_from(r.get()?).ok()?;
            // Every slot needs at least a presence word, so bound before
            // allocation even when an input length word is enormous.
            if n > words.len() { return None; }
            let mut slots = Vec::with_capacity(n);
            for _ in 0..n {
                slots.push(match r.get()? {
                    0 => None,
                    1 => Some(Citizen {
                        race: r.i32()?,
                        work: u8::try_from(r.get()?).ok()?,
                        job: r.i32()?,
                        resister: match r.get()? { 0 => false, 1 => true, _ => return None },
                        since: r.i32()?,
                        pending_race: r.i32()?,
                        pending_turn: r.i32()?,
                    }),
                    _ => return None,
                });
            }
            let free = r.run(|v| usize::try_from(v).ok())?;
            if !r.done() || free.len() != slots.iter().filter(|s| s.is_none()).count() { return None; }
            let mut seen = vec![false; n];
            for &i in &free {
                if i >= n || slots[i].is_some() || seen[i] { return None; }
                seen[i] = true;
            }
            Some(Pool { slots, free })
        })();
        let Some(pool) = parsed else { return false };
        *self = pool;
        true
    }

    /// One removal pass (`0x4BA24F..0x4BA2D0`). Always consumes a draw,
    /// even with no matching race or an empty pool. None means race -1.
    /// Selection is cyclic from a random slot, not uniform among citizens.
    pub fn remove(&mut self, race: Option<i32>, mut roll: impl FnMut(u32) -> i32) -> Option<(usize, Citizen)> {
        let n = self.slots.len();
        let start = (roll(n as u32) & 0xffff) as usize;
        for offset in 0..n {
            let i = (start + offset) % n;
            if self.slots[i].as_ref().is_some_and(|c| race.is_none_or(|r| c.race == r)) {
                let citizen = self.slots[i].take().unwrap();
                // `0x4BA3F3..0x4BA411`: null the node, push free head,
                // increment free count; highest index is not lowered.
                self.free.push(i);
                return Some((i, citizen));
            }
        }
        None
    }
}

/// `0x4BA4EF..0x4BA5D4`: only a size-class change alters food storage.
/// A Granary caps it at half the new food box; without one it becomes zero.
/// `box_unit` is the caller's 0x5660E0 result (human/AI/acceleration).
pub fn food_after_loss(food: i32, before: i32, after: i32, granary: bool, box_unit: i32) -> i32 {
    use crate::economy::{size_class, TOWN_MAX, CITY_MAX};
    let old = size_class(before, TOWN_MAX, CITY_MAX);
    let new = size_class(after, TOWN_MAX, CITY_MAX);
    if old == new { food } else if granary { food.min(box_unit * (new + 1)) } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn citizen(race: i32) -> Citizen { Citizen { work: 1, ..Citizen::new(race, 0) } }
    fn pool() -> Pool {
        let mut p = Pool::default();
        for race in [1, 9, 2, 1] { p.add(citizen(race)); }
        p.remove(None, |_| 1).unwrap();
        p
    }
    #[test]
    fn cyclic_selection_skips_holes_and_wrong_races() {
        for (start, expected) in [(0, 0), (1, 3), (2, 3), (3, 3)] {
            assert_eq!(pool().remove(Some(1), |n| { assert_eq!(n, 4); start }).unwrap().0, expected);
        }
        assert_eq!(pool().remove(None, |_| 1).unwrap().0, 2);
        assert_eq!(pool().remove(Some(2), |_| 3).unwrap().0, 2);
    }
    #[test]
    fn failed_removals_consume_draws_and_keep_slots() {
        let mut p = pool(); let before = p.slots().to_vec(); let mut draws = 0;
        assert!(p.remove(Some(7), |_| { draws += 1; 0 }).is_none());
        assert_eq!(draws, 1); assert_eq!(p.slots(), before);
        assert!(Pool::default().remove(None, |n| { assert_eq!(n, 0); draws += 1; 0 }).is_none());
        assert_eq!(draws, 2);
    }
    #[test]
    fn released_slots_are_reused_lifo_without_compacting_the_pool() {
        let mut p = pool(); p.remove(None, |_| 2).unwrap(); p.remove(None, |_| 3).unwrap();
        assert_eq!(p.slots().len(), 4);
        assert_eq!(p.add(citizen(8)), 3); assert_eq!(p.add(citizen(8)), 2);
        assert_eq!(p.add(citizen(8)), 1); assert_eq!(p.add(citizen(8)), 4);
    }
    #[test]
    fn food_tracks_size_class_and_owner_box_unit() {
        assert_eq!(food_after_loss(25, 7, 6, true, 10), 10);
        assert_eq!(food_after_loss(25, 7, 6, false, 10), 0);
        assert_eq!(food_after_loss(35, 13, 12, true, 10), 20);
        assert_eq!(food_after_loss(25, 5, 4, false, 10), 25);
        assert_eq!(food_after_loss(25, 7, 6, true, 5), 5);
        assert_eq!(food_after_loss(3, 7, 6, true, 10), 3);
    }

    #[test]
    fn disease_recovery_uses_the_draw_after_citizen_removal() {
        let mut p = Pool::default();
        for race in [1, 2, 3] { p.add(citizen(race)); }
        let mut rng = crate::rng::Rng::new(1);
        let (slot, victim) = p.remove(None, |n| rng.below(n)).unwrap();
        assert_eq!((slot, victim.race), (1, 2));
        assert!(crate::disease::recovers(2, |n| rng.below(n)));
        assert_eq!(rng.state(), 2_524_885_223, "two gameplay draws, removal before recovery");
    }

    #[test]
    fn restoring_keeps_victim_choice_and_future_free_slot_reuse() {
        let mut original = pool();
        original.remove(None, |_| 2).unwrap();
        // Slots A, hole, hole, C, each citizen ending with birth turn 0 and
        // no pending race (-1, -1); freed order 1 then 2 (next allocation
        // takes 2).
        assert_eq!(original.to_words(), [4, 1, 1, 1, 0, 0, 0, -1, -1, 0, 0, 1, 1, 1, 0, 0, 0, -1, -1, 2, 1, 2]);
        let mut restored = Pool::default();
        assert!(restored.restore(&original.to_words()));
        assert_eq!(restored, original);
        for start in 0..4 {
            assert_eq!(restored.clone().remove(None, |_| start), original.clone().remove(None, |_| start));
        }
        for race in [8, 7, 6] {
            assert_eq!(restored.add(citizen(race)), original.add(citizen(race)));
        }
        assert_eq!(restored, original);
        let c = Citizen { work: 20, job: 2, resister: true, since: 7, pending_race: 4, pending_turn: 9, ..Citizen::new(3, 0) };
        original.add(c);
        assert!(restored.restore(&original.to_words()));
        assert_eq!(restored, original);
        assert!(restored.restore(&Pool::default().to_words()));
        assert_eq!(restored, Pool::default());
    }

    #[test]
    fn invalid_streams_never_replace_a_valid_pool() {
        let mut original = pool();
        original.remove(None, |_| 2).unwrap();
        let good = original.to_words();
        let mut bad = Vec::new();
        for len in 0..good.len() { bad.push(good[..len].to_vec()); }
        let mut extra = good.clone(); extra.push(0); bad.push(extra);
        let mut duplicate = good.clone(); *duplicate.last_mut().unwrap() = 1; bad.push(duplicate);
        let mut occupied = good.clone(); *occupied.last_mut().unwrap() = 0; bad.push(occupied);
        let mut beyond_last = good.clone(); *beyond_last.last_mut().unwrap() = 4; bad.push(beyond_last);
        let mut negative = good.clone(); *negative.last_mut().unwrap() = -1; bad.push(negative);
        let mut invalid_work = good.clone(); invalid_work[3] = 256; bad.push(invalid_work);
        let mut invalid_race = good.clone(); invalid_race[2] = i64::MAX; bad.push(invalid_race);
        let mut invalid_presence = good.clone(); invalid_presence[1] = 2; bad.push(invalid_presence);
        let mut invalid_resistance = good.clone(); invalid_resistance[5] = 2; bad.push(invalid_resistance);
        bad.push(vec![i64::MAX]);
        for words in bad {
            let mut restored = original.clone();
            assert!(!restored.restore(&words), "invalid stream accepted: {words:?}");
            assert_eq!(restored, original);
        }
    }

    #[test]
    fn saving_between_removal_and_recovery_preserves_the_next_turn() {
        let mut uninterrupted = Pool::default();
        for race in [1, 2, 3] { uninterrupted.add(citizen(race)); }
        let mut rng = crate::rng::Rng::new(1);
        assert_eq!(uninterrupted.remove(None, |n| rng.below(n)).unwrap().0, 1);
        let mut loaded = Pool::default();
        assert!(loaded.restore(&uninterrupted.to_words()));
        let mut loaded_rng = crate::rng::Rng::new(rng.state());
        assert_eq!(crate::disease::recovers(2, |n| loaded_rng.below(n)),
            crate::disease::recovers(2, |n| rng.below(n)));
        assert_eq!(loaded.add(citizen(4)), uninterrupted.add(citizen(4)));
        assert_eq!(loaded.remove(None, |n| loaded_rng.below(n)),
            uninterrupted.remove(None, |n| rng.below(n)));
        assert_eq!(loaded, uninterrupted);
        assert_eq!(loaded_rng.state(), rng.state());
    }
}
