//! Per-tile unit stacks: the linked list behind "which unit is on this tile".
//!
//! See `../stacking.md`. Civ3 keeps **one singly-linked list per tile** in an
//! index pool; the list head lives in the `Cell` (`Cell+0x0C`, getter slot
//! `0xA0` = `0x5EAA90`, setter slot `0x104` = `0x5EAD10`). The pool is the
//! global container at `0xA52DD4`:
//!
//! ```text
//! +0x04 -> 0xA52DD8  node array, 8 bytes each: {i32 next, u32 unit}
//! +0x08 -> 0xA52DDC  free-list head (freed node's `next` is the link)
//! +0x0C -> 0xA52DE0  free count
//! +0x10 -> 0xA52DE4  highest slot index handed out (-1 before the first)
//! +0x14 -> 0xA52DE8  slot capacity (100 at init; grow at cap == capacity - 1)
//! +0x18 -> 0xA52DEC  out-of-range return, -1
//! ```
//!
//! A node's `unit` field is the unit's **id** (its index in the unit pool
//! `0xA52E80`/`0xA52E84`/`0xA52E90`), which the unit also keeps at `unit+0x20`.
//!
//! The ordering rule: `Unit::setPosition` (`0x5BD220`) **pushes the placed unit
//! at the head**, so the tile field is the most recently placed unit and
//! following `next` walks backwards in time. Nothing else mutates a tile's
//! list — fortify/sentry/wake do not reorder it.
//!
//! ```text
//! place A, then B, then C on one tile:
//!     order() == [C, B, A]      // C = head = drawn/queried first
//! ```

/// A pool slot: `{next, unit}` (`0x5BD756` writes both, `0x426C80` reads them).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Node {
    /// Index of the next node in the tile's list, `-1` = end of list.
    pub next: i32,
    /// The unit's id (its index in the unit pool; mirrors `unit+0x20`).
    pub unit: u32,
}

/// Nothing / end of list. The cell field, a node's `next`, and the pool's
/// out-of-range return are all this value (`0x4C8F10` stores `-1` into
/// `0xA52DEC`; `0x5BD6E7` stores it into an empty tile).
pub const NONE: i32 = -1;

/// The global node pool and its free list (`0xA52DD4`).
///
/// The shipped pool starts at **100 nodes** (`malloc 0x320` at `0x4C8F05`,
/// slot count - 1 = `0x64` at `0x4C8F0A`) and is reset to that state by the
/// scenario/save path (`0x590305`, `0x59FDDD`).
///
/// [`StackPool::new`] is that shipped state; [`StackPool::default`] is the
/// pre-allocation state the mover guards on (`0xA52DD8 == 0`).
#[derive(Clone, Debug, Default)]
pub struct StackPool {
    nodes: Vec<Node>,
    free_head: i32,
    free_count: i32,
    slot_count: i32,
}

impl StackPool {
    /// Fresh pool, as `0x4C8EF3`-`0x4C8F28` leaves it: 100 nodes, empty free
    /// list, and every slot `{next: -1, unit: 0}`.
    pub fn new() -> Self {
        StackPool {
            nodes: vec![
                Node {
                    next: NONE,
                    unit: 0
                };
                100
            ],
            free_head: NONE,
            free_count: 0,
            slot_count: NONE,
        }
    }

    /// Slot count as the binary tracks it (`0xA52DE4`): `-1` before the first
    /// allocation, then the highest slot index handed out.
    pub fn slot_count(&self) -> i32 {
        self.slot_count
    }

    /// Free count (`0xA52DE0`).
    pub fn free_count(&self) -> i32 {
        self.free_count
    }

    /// Node fetch, `0x426C80(container, index, &next)`:
    /// returns the node's unit, or `None` when `index` is out of range (the
    /// binary's caller then sees the `-1` default and stops walking).
    pub fn get(&self, index: i32) -> Option<Node> {
        if index < 0 || index > self.slot_count {
            return None;
        }
        self.nodes.get(index as usize).copied()
    }

    /// `Unit::setPosition`'s new-tile link, `0x5BD6DD`-`0x5BD763`.
    ///
    /// `head` is the tile's `Cell+0x0C`; on return it is the new node.
    /// Reuses a freed slot when one is available, otherwise takes the next
    /// slot (growing the array when the pool is exhausted — the binary's grow
    /// routine `0x4C22C0` is not recovered, this model just extends).
    ///
    /// A pool whose node array has not been allocated yet (`0xA52DD8 == 0`,
    /// i.e. [`StackPool::default`]) does not link anything: the binary clears
    /// the tile's head (`0x5BD6E7`) and returns `-1` here.
    pub fn place(&mut self, head: &mut i32, unit: u32) -> i32 {
        if self.nodes.is_empty() {
            *head = NONE;
            return NONE;
        }
        let old_head = *head;
        let idx = self.alloc();
        self.nodes[idx as usize] = Node {
            next: old_head,
            unit,
        };
        *head = idx;
        idx
    }

    /// The mover's unlink path, `0x5BD3C5`-`0x5BD4E4` + `0x5BD4FA`-`0x5BD521`.
    ///
    /// Walks from the head tracking `prev`; on a match unlinks the node and
    /// pushes it on the free list. **Returns false and clears the tile head
    /// when the unit is not in the list** (`0x5BD3F3` sets the head field to
    /// `-1`) — the binary drops the whole list rather than leaving it alone.
    pub fn remove(&mut self, head: &mut i32, unit: u32) -> bool {
        let mut cur = *head;
        let mut prev = NONE;
        while cur != NONE {
            let node = match self.nodes.get(cur as usize).copied() {
                Some(n) => n,
                None => break,
            };
            if node.unit == unit {
                if prev == NONE {
                    *head = node.next;
                } else {
                    self.nodes[prev as usize].next = node.next;
                }
                self.free(cur);
                return true;
            }
            prev = cur;
            cur = node.next;
        }
        *head = NONE;
        false
    }

    /// Tile order as every consumer walks it: head -> tail, i.e. most recently
    /// placed first.
    pub fn order(&self, head: i32) -> Vec<u32> {
        let mut out = Vec::new();
        let mut cur = head;
        while let Some(node) = self.get(cur) {
            out.push(node.unit);
            cur = node.next;
        }
        out
    }

    /// The first match from the head: what `0x56D340` (foreign unit at tile)
    /// and the garrison row's per-unit decisions return.
    pub fn first_match(&self, head: i32, pred: impl Fn(u32) -> bool) -> Option<u32> {
        let mut cur = head;
        while let Some(node) = self.get(cur) {
            if pred(node.unit) {
                return Some(node.unit);
            }
            cur = node.next;
        }
        None
    }

    /// The **last** match, i.e. the tile's oldest resident: what `0x573B20`
    /// keeps when it draws the single unit sprite for a tile.
    pub fn last_match(&self, head: i32, pred: impl Fn(u32) -> bool) -> Option<u32> {
        let mut found = None;
        let mut cur = head;
        while let Some(node) = self.get(cur) {
            if pred(node.unit) {
                found = Some(node.unit);
            }
            cur = node.next;
        }
        found
    }

    /// `0x5BD6F0`-`0x5BD728`: pop the free list, else take the next slot.
    fn alloc(&mut self) -> i32 {
        if self.free_count > 0 {
            let idx = self.free_head;
            self.free_head = self.nodes[idx as usize].next;
            self.free_count -= 1;
            return idx;
        }
        let idx = if self.slot_count == NONE {
            0
        } else {
            self.slot_count + 1
        };
        self.slot_count = idx;
        if idx as usize >= self.nodes.len() {
            self.nodes.push(Node {
                next: NONE,
                unit: 0,
            });
        }
        idx
    }

    /// `0x5BD50D`-`0x5BD521`: unlinked nodes go on the free list.
    fn free(&mut self, idx: i32) {
        self.nodes[idx as usize].next = self.free_head;
        self.free_head = idx;
        self.free_count += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `0x5BD6DD`-`0x5BD6EE`: with the node array unallocated the mover links
    /// nothing and clears the tile's head instead.
    #[test]
    fn an_unallocated_pool_clears_the_tile_head() {
        let mut pool = StackPool::default();
        let mut head = 7;
        assert_eq!(pool.place(&mut head, 1), NONE);
        assert_eq!(head, NONE);
    }

    /// `0x5BD220` pushes at the head, so the tile field is the newest
    /// placement and the walk runs newest -> oldest.
    #[test]
    fn placements_go_to_the_head_and_walk_backwards_in_time() {
        let mut pool = StackPool::new();
        let mut head = NONE;
        pool.place(&mut head, 10);
        pool.place(&mut head, 11);
        pool.place(&mut head, 12);
        assert_eq!(pool.get(head).unwrap().unit, 12, "head = newest");
        assert_eq!(pool.order(head), vec![12, 11, 10]);
    }

    /// A unit that leaves and returns is re-placed, so it is newest again.
    #[test]
    fn a_returning_unit_becomes_the_head_again() {
        let mut pool = StackPool::new();
        let mut head = NONE;
        pool.place(&mut head, 1);
        pool.place(&mut head, 2);
        assert!(pool.remove(&mut head, 1));
        pool.place(&mut head, 1);
        assert_eq!(pool.order(head), vec![1, 2]);
    }

    /// `0x5BD4FA`: head, middle and tail removal all relink correctly.
    #[test]
    fn removal_relinks_head_middle_and_tail() {
        for victim in [1u32, 2, 3] {
            let mut pool = StackPool::new();
            let mut head = NONE;
            for u in [1u32, 2, 3] {
                pool.place(&mut head, u);
            }
            assert!(pool.remove(&mut head, victim));
            let expected: Vec<u32> = [3, 2, 1].iter().copied().filter(|u| *u != victim).collect();
            assert_eq!(pool.order(head), expected, "removing {victim}");
        }
    }

    /// `0x5BD3F3`: a unit that is not on the tile clears the tile's head — the
    /// binary drops the whole list, it does not leave it untouched.
    #[test]
    fn removing_an_absent_unit_clears_the_tile_head() {
        let mut pool = StackPool::new();
        let mut head = NONE;
        for u in [1u32, 2] {
            pool.place(&mut head, u);
        }
        assert!(!pool.remove(&mut head, 99));
        assert_eq!(head, NONE);
        assert!(pool.order(head).is_empty());
    }

    /// `0x5BD6F0`-`0x5BD728`: the free list is consumed before the pool grows.
    #[test]
    fn freed_slots_are_reused_before_the_pool_grows() {
        let mut pool = StackPool::new();
        let mut head = NONE;
        for u in [1u32, 2, 3] {
            pool.place(&mut head, u);
        }
        let slots = pool.slot_count();
        assert!(pool.remove(&mut head, 2));
        assert_eq!(pool.free_count(), 1);
        let reused = pool.place(&mut head, 4);
        assert_eq!(pool.slot_count(), slots, "no new slot taken");
        assert_eq!(pool.get(reused).unwrap().unit, 4);
        assert_eq!(pool.free_count(), 0);
    }

    /// Walking stops on the `-1` default (`0x426C80` out-of-range), so an empty
    /// tile and a truncated list both terminate.
    #[test]
    fn walks_terminate_on_the_empty_sentinel() {
        let pool = StackPool::new();
        assert!(pool.order(NONE).is_empty());
        assert_eq!(pool.get(NONE), None);
        assert_eq!(pool.first_match(NONE, |_| true), None);
        assert_eq!(pool.last_match(NONE, |_| true), None);
        // Out-of-range indices behave like the empty sentinel.
        assert_eq!(pool.get(4096), None);
    }

    /// The two walk shapes differ only in which end wins: `0x56D340` returns the
    /// first match from the head, `0x573B20` keeps overwriting and returns the
    /// tail-most match.
    #[test]
    fn first_and_last_match_agree_with_the_two_consumers() {
        let mut pool = StackPool::new();
        let mut head = NONE;
        for u in [1u32, 2, 3, 4] {
            pool.place(&mut head, u);
        }
        let even = |u: u32| u.is_multiple_of(2);
        assert_eq!(pool.order(head), vec![4, 3, 2, 1]);
        assert_eq!(pool.first_match(head, even), Some(4));
        assert_eq!(pool.last_match(head, even), Some(2));
    }
}
