//! River generation: the stage that writes the river masks.
//!
//! Rivers run along tile *edges*, on a lattice with twice the resolution of the
//! tile grid: a lattice point is a tile corner, and the tile at lattice offset
//! `(a, b)` is `(a / 2, b / 2)`. The four lattice steps are the diagonals
//! `DX = [1, 1, -1, -1]`, `DY = [-1, 1, 1, -1]`, indexed by direction `0..4`.
//!
//! 1. **Quota.** Up to 256 extra sources are shared out over the continents
//!    (one for each continent of at least [`BIG_CONTINENT`] cells, the rest in
//!    proportion to size); see [`quota`].
//! 2. **Pass A, candidates.** In a shuffled order, a coastal corner (exactly two
//!    of four tiles water and not opposite) is scored from the 121 cells around
//!    it and kept in its continent's window unless a kept entry is within
//!    [`MIN_SEPARATION`].
//! 3. **Pass B, growth.** Each kept candidate grows a river chain.
//! 4. **Pass C, orthogonal bits.** The four orthogonal mask bits are filled from
//!    the edges on both sides of each direction ([`joined`]).
//!
//! Original quirks are reproduced: the window insertion starts one slot *after*
//! the slot it found; an empty slot's position `0` blocks candidates near the
//! map corner; and the proportional share divides by a 16-bit running total.

use crate::cell::MapGrid;
use crate::rng::Rng;
use crate::spiral::{spiral_index, spiral_offset};

/// Lattice step per direction.
pub const DX: [i32; 4] = [1, 1, -1, -1];
/// Lattice step per direction.
pub const DY: [i32; 4] = [-1, 1, 1, -1];
/// A chain deeper than this ends in a mountain.
pub const MAX_DEPTH: i32 = 20;
/// Seed offset of the candidate shuffle.
pub const SHUFFLE_SEED: u32 = 0x87B01;
/// Continents with at least this many cells get a source of their own.
pub const BIG_CONTINENT: usize = 37;
/// Slots of the candidate tables.
pub const SLOTS: usize = 256;
/// Kept candidates must be more than this many steps apart.
pub const MIN_SEPARATION: i32 = 3;

/// Lattice point to tile, `None` off the map.
fn tile(g: &MapGrid, a: i32, b: i32) -> Option<(i32, i32)> {
    let (x, y) = (a / 2, b / 2);
    ((0..g.w).contains(&x) && (0..g.h).contains(&y)).then_some((x, y))
}

fn in_map(g: &MapGrid, x: i32, y: i32) -> bool {
    (0..g.w).contains(&x) && (0..g.h).contains(&y)
}

/// Marks the edge of lattice point `(x, y)` with orientation `k`.
fn mark(g: &mut MapGrid, x: i32, y: i32, k: usize) {
    let (a, b) = if k != 0 { (0x80, 0x08) } else { (0x20, 0x02) };
    for (kk, bit) in [(k, a), (k + 2, b)] {
        if let Some((tx, ty)) = tile(g, x + DX[kk], y + DY[kk]) {
            if let Some(c) = g.cell_at_mut(tx, ty) {
                c.add_river(bit);
            }
        }
    }
}

/// The inverse of [`mark`].
fn unmark(g: &mut MapGrid, x: i32, y: i32, k: usize) {
    let (a, b) = if k != 0 { (0x80, 0x08) } else { (0x20, 0x02) };
    for (kk, bit) in [(k, a), (k + 2, b)] {
        if let Some((tx, ty)) = tile(g, x + DX[kk], y + DY[kk]) {
            if let Some(c) = g.cell_at_mut(tx, ty) {
                c.remove_river(bit);
            }
        }
    }
}

/// The spiral index of the step from `(x1, y1)` to `(x2, y2)`, with the offset
/// folded to the shorter way round the map; `-1` if not among the first `limit`.
pub fn direction(g: &MapGrid, x1: i32, y1: i32, x2: i32, y2: i32, limit: i32) -> i32 {
    let (mut dx, mut dy) = (x2 - x1, y2 - y1);
    if dx > g.w >> 1 {
        dx -= g.w;
    } else if dx < -(g.w >> 1) {
        dx += g.w;
    }
    if dy > g.h >> 1 {
        dy -= g.h;
    } else if dy < -(g.h >> 1) {
        dy += g.h;
    }
    spiral_index(dx, dy, limit).unwrap_or(-1)
}

/// Are the river edges on both sides of the step from `(x, y)` to `(nx, ny)`
/// present? With `d` the spiral index of the step, `T1` and `T2` the neighbours
/// at `d - 1` and `d + 1` (mod 8), the edge `cell|T1` is bit `d - 1` of the cell
/// or bit `d + 1` of `T1`, and vice versa; both edges must exist.
pub fn joined(g: &MapGrid, x: i32, y: i32, nx: i32, ny: i32) -> bool {
    let d = direction(g, x, y, nx, ny, 9);
    let prev = (d + 7) & 7;
    let next = (d + 1) & 7;
    let mask = |a: i32, b: i32| g.cell_at(a, b).map_or(0, |c| c.river);
    let around = |idx: i32| {
        let (dx, dy) = spiral_offset(idx);
        g.wrap_and_check(x + dx, y + dy)
    };
    let own = mask(x, y);
    let f1 = around(prev).is_some_and(|(tx, ty)| {
        own & (1 << prev) != 0 || mask(tx, ty) & (1 << next) != 0
    });
    let f2 = around(next).is_some_and(|(tx, ty)| {
        own & (1 << next) != 0 || mask(tx, ty) & (1 << prev) != 0
    });
    f1 && f2
}

/// Which of the three continuation scores comes first, second and third: a
/// decision tree with fixed tie-breaks, so equal scores keep the order left,
/// straight, right except where the tree says otherwise.
fn rank(c: [i32; 3]) -> [usize; 3] {
    let [c0, c1, c2] = c;
    if c0 >= c1 && c1 >= c2 {
        [0, 1, 2]
    } else if c0 >= c2 && c2 >= c1 {
        [0, 2, 1]
    } else if c1 >= c0 && c0 >= c2 {
        [1, 0, 2]
    } else if c1 >= c2 {
        [1, 2, 0]
    } else if c2 >= c0 && c0 >= c1 {
        [2, 0, 1]
    } else {
        [2, 1, 0]
    }
}

/// The river generator; it owns the grid for the duration of the stage.
pub struct RiverGen<'a> {
    /// The map.
    pub grid: &'a mut MapGrid,
    seed: u32,
}

impl<'a> RiverGen<'a> {
    /// A generator for `grid` with the map seed `seed`.
    pub fn new(grid: &'a mut MapGrid, seed: i32) -> Self {
        RiverGen { grid, seed: seed as u32 }
    }

    /// How good the land at `(x, y)` is for a river `n` spiral steps from the
    /// probe, `depth` steps into the chain. `-10` for water and off-map. Noise
    /// comes from an LCG seeded `seed + 666 * cell`, warmed up by four draws,
    /// drawn twice (`rand(11) - 5` each time).
    pub fn score_cell(&self, x: i32, y: i32, depth: i32, n: i32) -> i32 {
        let g = &*self.grid;
        let idx = ((g.w >> 1) * y + i32::from((x as u16) >> 1)) & 0xFFFF;
        let mut rng = Rng::new(self.seed.wrapping_add(666u32.wrapping_mul(idx as u32)));
        rng.discard(4);
        if !in_map(g, x, y) {
            return -10;
        }
        let Some(cell) = g.cell_at(x, y).filter(|c| !c.is_water()) else {
            return -10;
        };
        let by_depth = |a: i32, b: i32, c: i32| {
            if depth < 4 {
                a
            } else if depth < 8 {
                b
            } else {
                c
            }
        };
        let base = match cell.class() {
            0 => by_depth(2, 0, -1),
            2 => by_depth(5, 2, -1),
            1 | 7 | 8 | 9 => by_depth(4, 2, -1),
            5 => by_depth(-3, 5, 5),
            6 => by_depth(-3, 10, 15),
            10 => -5,
            _ => by_depth(0, 0, -1),
        };
        let mut s = base + rng.below(11) - 5;
        if n <= 2 {
            s *= 2;
        }
        if n <= 1 {
            s *= 2;
        }
        s + rng.below(11) - 5
    }

    /// The score of continuing a chain with lattice step `dir` from `(x, y)`,
    /// or `-1` if the way is blocked.
    pub fn probe(&self, x: i32, y: i32, dir: usize, depth: i32, flag: i32) -> i32 {
        let g = &*self.grid;
        let k = (!dir) & 1;
        let at = |a: i32, b: i32| tile(g, a, b).and_then(|(tx, ty)| g.cell_at(tx, ty));
        for kk in [k, k + 2] {
            match at(x + DX[kk], y + DY[kk]) {
                Some(c) if !c.is_water() => {}
                _ => return -1,
            }
        }
        let ahead = |steps: i32| (x + steps * DX[dir], y + steps * DY[dir]);
        let (x2, y2) = ahead(2);
        for kk in [k, k + 2] {
            match at(x2 + DX[kk], y2 + DY[kk]) {
                Some(c) if !c.is_water() && c.river == 0 => {}
                _ => return -1,
            }
            match at(x2 + 3 * DX[kk], y2 + 3 * DY[kk]) {
                Some(c) if c.river == 0 => {}
                _ => return -1,
            }
        }
        let (x4, y4) = ahead(4);
        for kk in [k, k + 2] {
            for m in [1, 3] {
                match at(x4 + m * DX[kk], y4 + m * DY[kk]) {
                    Some(c) if c.river == 0 => {}
                    _ => return -1,
                }
            }
        }
        let (x6, y6) = ahead(6);
        let Some(e1) = tile(g, x6 + DX[k], y6 + DY[k]) else {
            return -1;
        };
        let Some(e2) = tile(g, x6 + DX[k + 2], y6 + DY[k + 2]) else {
            return -1;
        };
        let mut sum = 0;
        for n in 0..49 {
            let (dx, dy) = spiral_offset(n);
            for (ex, ey) in [e1, e2] {
                let (px, py) = (ex + dx, ey + dy);
                if in_map(g, px, py) {
                    sum += self.score_cell(px, py, depth, n + 1);
                }
            }
        }
        if flag > 0 {
            sum / flag
        } else {
            sum
        }
    }

    /// The mountain that ends a river: the tile at lattice offset `-D[d]` from
    /// `(x, y)`, `(x'/2, y'/2 - 1)`, becomes class 6.
    fn source(&mut self, x: i32, y: i32, d: usize) {
        let (px, py) = (x - DX[d], y - DY[d]);
        let (mut tx, mut ty) = (px / 2, py / 2 - 1);
        let g = &mut *self.grid;
        if g.wrap_flags & 1 != 0 {
            if tx < 0 {
                tx += g.w;
            } else if tx >= g.w {
                tx -= g.w;
            }
        }
        if g.wrap_flags & 2 != 0 {
            if ty < 0 {
                ty += g.h;
            } else if ty >= g.h {
                ty -= g.h;
            }
        }
        if let Some(c) = g.cell_at_mut(tx, ty) {
            c.set_class(6);
        }
    }

    /// Extends a river from lattice point `(x, y)` in direction `dir`; `depth`
    /// counts steps, `w` forks, `z` the net turn, `p` the run of straight steps.
    /// Returns whether the chain survived.
    pub fn grow_chain(
        &mut self,
        x: i32,
        y: i32,
        dir: usize,
        depth: i32,
        w: i32,
        z: i32,
        p: i32,
    ) -> bool {
        if depth > MAX_DEPTH {
            self.source(x, y, dir);
            return true;
        }
        let k = (!dir) & 1;
        mark(self.grid, x, y, k);
        let (nx, ny) = (x + DX[dir], y + DY[dir]);

        let c0 = if z < -1 {
            -1
        } else {
            let d = (dir + 3) & 3;
            self.probe(nx + DX[d], ny + DY[d], d, depth, if z >= 0 { 0 } else { 2 })
        };
        let flag = if p >= 2 { 1 << (p - 1) } else { 0 };
        let c1 = self.probe(nx + DX[dir], ny + DY[dir], dir, depth, flag);
        let c2 = if z > 1 {
            -1
        } else {
            let d = (dir + 1) & 3;
            self.probe(nx + DX[d], ny + DY[d], d, depth, if z <= 0 { 0 } else { 2 })
        };

        if c0 < 0 && c1 < 0 && c2 < 0 {
            if depth >= 4 && w >= 2 {
                self.source(x, y, (dir + 2) & 3);
                return true;
            }
            unmark(self.grid, x, y, k);
            return false;
        }
        if self.fan_out(nx, ny, dir, depth, w, z, p, [c0, c1, c2]) {
            return true;
        }
        unmark(self.grid, x, y, k);
        false
    }

    /// Follows the best one to three of the continuations `c` (left, straight,
    /// right) of a chain, in order of score.
    #[allow(clippy::too_many_arguments)]
    pub fn fan_out(
        &mut self,
        x: i32,
        y: i32,
        dir: usize,
        depth: i32,
        w: i32,
        z: i32,
        p: i32,
        c: [i32; 3],
    ) -> bool {
        let idx = (((self.grid.w >> 1) * y + i32::from((x as u16) >> 1)) & 0xFFFF) as u32;
        let mut rng = Rng::new(self.seed.wrapping_add(42u32.wrapping_mul(idx)));
        rng.discard(4);
        let order = rank(c);
        let mut n = 1;
        if w >= 2 {
            let (cf, cs, ct) = (c[order[0]], c[order[1]], c[order[2]]);
            if cf - cs > (cf >> 3) {
                if cs >= 0 {
                    n = rng.below(2) + 1;
                }
            } else {
                n = 2;
                if ct >= 0 {
                    n = rng.below(2) + 2;
                }
            }
        }
        let mut grown = false;
        for &branch in order.iter().take(n as usize) {
            let (d, z2, p2) = match branch {
                0 => ((dir + 3) & 3, z - 1, 0),
                1 => (dir, z, p + 1),
                _ => ((dir + 1) & 3, z + 1, 0),
            };
            let w2 = if n == 2 { 0 } else { w + 1 };
            if self.grow_chain(x + DX[d], y + DY[d], d, depth + 1, w2, z2, p2) {
                grown = true;
            }
        }
        grown
    }
}

/// The per-continent source quota, one byte per continent: after this `w[c]` is
/// the end of the window of continent `c` and `w[c - 1]` its start.
pub fn quota(sizes: &[usize], cells: usize) -> Vec<u8> {
    let mut w = vec![0u8; sizes.len()];
    let mut extra = ((75 * cells as i32) / 5000).clamp(0, 256);
    let mut total: u16 = 0;
    for (c, &s) in sizes.iter().enumerate() {
        w[c] = u8::from(s >= BIG_CONTINENT);
        total = total.wrapping_add(s as u16);
        extra -= i32::from(w[c]);
    }
    for c in (1..sizes.len()).rev() {
        let share = if total == 0 { 0 } else { (sizes[c] as i32 * extra) / i32::from(total) };
        w[c] = w[c].wrapping_add(share as u8);
        extra -= share;
        total = total.wrapping_sub(sizes[c] as u16);
    }
    if let Some(first) = w.first_mut() {
        *first = first.wrapping_add(extra as u8);
    }
    for c in 1..w.len() {
        if w[c] == 0 {
            break;
        }
        w[c] = w[c].wrapping_add(w[c - 1]);
    }
    w
}

/// The number of cells of every continent, as the last numbering left them.
pub fn continent_sizes(grid: &MapGrid) -> Vec<usize> {
    let mut sizes: Vec<usize> = Vec::new();
    for c in &grid.cells {
        if c.continent != crate::cell::NO_CONTINENT {
            let id = c.continent as usize;
            if sizes.len() <= id {
                sizes.resize(id + 1, 0);
            }
            sizes[id] += 1;
        }
    }
    sizes
}

/// Grows the rivers of the map.
pub fn grow_rivers(grid: &mut MapGrid, seed: i32) {
    let n = grid.num_cells();
    let half = grid.w >> 1;
    let w = quota(&continent_sizes(grid), n);

    let mut rng = Rng::new((seed as u32).wrapping_add(SHUFFLE_SEED));
    let mut perm: Vec<u16> = (0..n as u16).collect();
    for i in 0..n {
        let j = i + rng.below((n - i) as u32) as usize;
        perm.swap(i, j);
    }

    // Pass A: candidates.
    let mut score_tab = [0u16; SLOTS];
    let mut pos_tab = [0u16; SLOTS];
    for &cell_idx in &perm {
        let ci = cell_idx as usize;
        let cont = grid.cells[ci].continent;
        let Some(&wc) = w.get(cont as usize) else { continue };
        if wc == 0 {
            continue;
        }
        let (x, y) = grid.coords(ci);

        let mut water = [false; 4];
        let mut water_count = 0;
        let mut inside = true;
        for d in 0..4 {
            let nx = grid.wrap_axis(x + (d >> 1) - (d & 1), true);
            let ny = grid.wrap_axis(y + ((d + 1) >> 1), false);
            if !in_map(grid, nx, ny) {
                inside = false;
                break;
            }
            if grid.cell_at(nx, ny).is_some_and(|c| c.is_water()) {
                water[d as usize] = true;
                water_count += 1;
            }
        }
        if !inside || water_count != 2 || water[3] == water[0] {
            continue;
        }

        let mut score: i32 = 0;
        for k in 0..121 {
            let (dx, dy) = spiral_offset(k);
            let px = grid.wrap_axis(x + dx, true);
            let py = grid.wrap_axis(y + dy, false);
            if !in_map(grid, px, py) {
                continue;
            }
            let Some(nb) = grid.cell_at(px, py) else { continue };
            if nb.continent != cont {
                continue;
            }
            score += 8;
            let (a, b, c) = match nb.class() {
                0..=3 | 5..=7 | 10 => (8, 16, 32),
                8 => (16, 32, 64),
                9 => (32, 64, 64),
                _ => (0, 0, 0),
            };
            if k < 25 {
                score += a;
            }
            if k < 9 {
                score += b;
            }
            if k < 1 {
                score += c;
            }
        }
        let score = score as u16;

        let start = if cont != 0 { w[cont as usize - 1] as usize } else { 0 };
        let end = wc as usize;
        let mut slot = start;
        let mut found = false;
        while slot < end {
            found = score > score_tab[slot];
            slot += 1;
            if found {
                break;
            }
        }
        if !found {
            continue;
        }
        let too_close = (start..end).any(|k| {
            let p = i32::from(pos_tab[k]);
            let (row, col) = (p / half, p % half);
            let xk = (row & 1) + 2 * col;
            let mut dx = (x - xk).abs();
            if grid.wrap_flags & 1 != 0 && dx > grid.w >> 1 {
                dx = grid.w - dx;
            }
            let mut dy = (y - row).abs();
            if grid.wrap_flags & 2 != 0 && dy > grid.h >> 1 {
                dy = grid.h - dy;
            }
            (dx + dy) / 2 <= MIN_SEPARATION
        });
        if too_close || slot >= end {
            continue;
        }
        let (mut cs, mut cx, mut cy) = (score, x, y);
        for k in slot..end {
            let old = i32::from(pos_tab[k]);
            let (orow, ocol) = (old / half, old % half);
            pos_tab[k] = (half * cy + (cx >> 1)) as u16;
            std::mem::swap(&mut score_tab[k], &mut cs);
            cx = 2 * ocol + (orow & 1);
            cy = orow;
        }
    }

    // Pass B: grow a river from each kept candidate.
    let mut gen = RiverGen::new(grid, seed);
    for &pos in pos_tab.iter() {
        if pos == 0 {
            continue;
        }
        let (x, y) = gen.grid.coords(pos as usize);
        let (mut sx, mut sy) = (0, 0);
        let mut water = [false; 4];
        for d in 0..4 {
            let nx = gen.grid.wrap_axis(x + (d >> 1) - (d & 1), true);
            let ny = gen.grid.wrap_axis(y + ((d + 1) >> 1), false);
            if !in_map(gen.grid, nx, ny) {
                continue;
            }
            if gen.grid.cell_at(nx, ny).is_some_and(|c| c.is_water()) {
                water[d as usize] = true;
            } else {
                sx += nx;
                sy += ny;
            }
        }
        let kind = match (water[0], water[1]) {
            (true, true) => 1,
            (true, false) => 2,
            (false, true) => 0,
            (false, false) => 3,
        };
        gen.grow_chain(sx, sy, kind, 0, 0, 0, 0);
    }

    // Pass C: the orthogonal bits.
    for i in 0..n {
        let (x, y) = grid.coords(i);
        for d in [0, 2, 4, 6] {
            let (dx, dy) = spiral_offset(if d == 0 { 8 } else { d });
            let nx = grid.wrap_axis(x + dx, true);
            let ny = grid.wrap_axis(y + dy, false);
            if in_map(grid, nx, ny) && joined(grid, x, y, nx, ny) {
                grid.cells[i].add_river(1 << d);
            }
        }
    }
}

/// A desert tile with a river becomes flood plain.
pub fn flood_deserts(grid: &mut MapGrid) {
    for c in grid.cells.iter_mut() {
        if c.river != 0 && c.class() == 0 {
            c.set_class(4);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rank_is_a_descending_order_with_fixed_ties() {
        assert_eq!(rank([5, 3, 1]), [0, 1, 2]);
        assert_eq!(rank([5, 1, 3]), [0, 2, 1]);
        assert_eq!(rank([3, 5, 1]), [1, 0, 2]);
        assert_eq!(rank([1, 5, 3]), [1, 2, 0]);
        assert_eq!(rank([3, 1, 5]), [2, 0, 1]);
        assert_eq!(rank([1, 3, 5]), [2, 1, 0]);
        for a in -2..=2 {
            for b in -2..=2 {
                for c in -2..=2 {
                    let v = [a, b, c];
                    let r = rank(v);
                    assert!(v[r[0]] >= v[r[1]] && v[r[1]] >= v[r[2]], "{v:?} -> {r:?}");
                }
            }
        }
    }

    #[test]
    fn quota_gives_each_big_continent_a_slot() {
        let w = quota(&[1000, 10, 10, 5000], 5000);
        assert!(w[0] >= 1);
        assert_eq!(w[1], 0, "a small continent gets no window");
    }

    #[test]
    fn mark_and_unmark_are_inverse() {
        let mut g = MapGrid::new(20, 20, 0, 0);
        for c in g.cells.iter_mut() {
            c.set_class(2);
        }
        let before = g.clone().cells;
        mark(&mut g, 10, 10, 0);
        assert!(g.cells != before);
        unmark(&mut g, 10, 10, 0);
        assert_eq!(g.cells, before);
    }

    #[test]
    fn a_river_makes_desert_flood_plain_and_back() {
        let mut g = MapGrid::new(20, 20, 0, 0);
        for c in g.cells.iter_mut() {
            c.set_class(0);
        }
        mark(&mut g, 10, 10, 1);
        assert_eq!(g.cells.iter().filter(|c| c.class() == 4).count(), 2);
        unmark(&mut g, 10, 10, 1);
        assert!(g.cells.iter().all(|c| c.class() == 0 && c.river == 0));
    }
}
