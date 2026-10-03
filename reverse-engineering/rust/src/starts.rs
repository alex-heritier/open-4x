//! The last generator stage: start locations (`finalPass`, `0x5EEEE0`).
//!
//! `generateMap` (`0x5EB580`) ends with
//! `finalPass(1, 0, single_player ? 1 : -1, single_player, seafarers)`
//! ([`Args::generate`]). The same routine runs again, with `fresh = false`,
//! from `0x5D1E7E` when a loaded scenario has fewer start locations than civs
//! (it then only fills the free slots); that variant is ported too.
//!
//! The stage works on three numbers per tile and one per continent:
//!
//! * the **site value** of every tile, `Map` vtable slot 8 (`0x5D3830`), which
//!   calls the city-site evaluator `0x442480` with player `-1` ([`crate::site`]);
//! * the **shore rank** of a tile ([`shore_rank`], `0x5EEDB0`): the largest
//!   body of water among its 8 neighbours;
//! * the continent of the tile and its size;
//! * per continent, how many luxury resources have their first tile on it
//!   (`lux`) and how many starts it has got so far (`taken`).
//!
//! # Phases
//!
//! 1. **Luxury tally.** For each `GOOD` row of class 1 find the first tile
//!    (lowest cell index) that carries it and count it for that tile's
//!    continent. A luxury's copies all lie on one continent
//!    ([`crate::placement`]), so this is "how many luxuries each continent has".
//! 2. **Order.** Warm the generator up (32 draws; `seed + 0x16062`), shuffle
//!    all cell indices, ask for the site value of each in shuffled order and sort
//!    the cells by value, best first ([`sort_by_value`], `0x5CDCD0`: a counting
//!    sort whose order inside a group of equal values is whatever its
//!    swap-to-the-end passes leave).
//! 3. **Eight passes** ("levels" 0 to 7) over that order, each placing the next
//!    start on every tile that qualifies, until every civ has one. A tile
//!    qualifies at level `L` when it is
//!    * on a row from 1 to `H - 2`, on land, with no start, hut, camp, city, colony
//!      or claim already;
//!    * for the first `seafarers` starts of `L < 4`: next to a body of water of
//!      more than 20 tiles (the largest neighbour, [`shore_rank`]);
//!    * at `L = 0` while starts are fewer than luxuries: on a continent that has
//!      more luxuries than starts;
//!    * on a continent of at least 75 tiles, or from `L = 2` 37, or from `L = 5`
//!      any;
//!    * for `L < 7`: of positive site value;
//!    * for `L < 6`: farther than `radius` from every start on its continent
//!      (`radius / 2` from `L = 3`), by the isometric distance of [`distance`].
//!
//!    The start is recorded in the first free slot of `Map+0x16C` and the tile
//!    gets feature bit `0x80000` ([`START`]).
//! 4. **Shuffle** the slots 1.. (Fisher-Yates). With `jump = 1` (a single-player
//!    game) the swap for slot 1 is not random: slot 1 trades with slot
//!    `(2n - 2) / 3 + 1`, so the first civ (the human) does not get the best start.
//! 5. **Group by continent** (single player only): for each slot `i` from 2 on, the
//!    slots from `i` whose continent differs from the one of slot `i - 1` swap with
//!    the next slot that has it, so civs on one continent are neighbours in the
//!    slot order.
//!
//! The slot a civ gets is its index: civ `k` starts at `slots[k]`.

use crate::cell::{Cell, MapGrid, PLANE_FEATURE, PLANE_OVERLAY};
use crate::continents::Continent;
use crate::placement::{shuffled_cells, Rules, CAMP, HUT, LUXURY, START};
use crate::rng::Rng;
use crate::spiral::spiral_offset;

/// Seed constant of the stage, `0x5EF03D` (`seed + 0x16062 + salt`).
pub const START_SEED: u32 = 0x16062;
/// The number of start slots, `Map+0x16C..0x1EC`. Slot 0 is never used.
pub const SLOTS: usize = 32;
/// A water body must be larger than this to count as a shore (`0x5EF2AF`).
const SHORE_MIN_SIZE: u32 = 20;
/// A continent this big is always acceptable (`0x5EF40B`).
const BIG_CONTINENT: u32 = 75;
/// ... from level 2 on this big (`0x5EF425`).
const MEDIUM_CONTINENT: u32 = 37;
/// The number of passes.
const LEVELS: i32 = 8;

/// The five arguments of `finalPass`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Args {
    /// `a1`: `true` starts from empty slots, `false` completes the slots
    /// already filled.
    pub fresh: bool,
    /// `a2`: added to the generator state before the slot shuffle.
    pub salt: u32,
    /// `a3`: the slot index that is swapped without a random draw.
    pub jump: i32,
    /// `a4`: group the slots by continent afterwards.
    pub group: bool,
    /// `a5`: how many of the first starts must be on a shore.
    pub seafarers: i32,
}

impl Args {
    /// What `generateMap` passes. `single_player` is its second argument being
    /// false (`Map::generate` makes it `true` for a multiplayer game);
    /// `seafarers` is `Map::generate`'s second argument, which the New Game
    /// screen sets to the number of civs with the Seafaring trait (RACE trait
    /// bit 7, tested by `RACE.vtable[0]`, `0x53A080`).
    pub fn generate(single_player: bool, seafarers: i32) -> Args {
        Args {
            fresh: true,
            salt: 0,
            jump: if single_player { 1 } else { -1 },
            group: single_player,
            seafarers,
        }
    }
}

/// The isometric distance between two tiles given their coordinate
/// differences (`0x5EF4DC`..`0x5EF576`), after wrapping each axis: the larger
/// difference less half of what the two differences have beyond the smaller.
/// One step in the 8 directions is 1.
pub fn distance(dx: i32, dy: i32) -> i32 {
    let (hi, lo) = (dx.max(dy), dx.min(dy));
    hi - ((dx + dy) / 2 - lo + 1) / 2
}

/// The distance between tiles `(x1, y1)` and `(x2, y2)` on `grid`, wrapping
/// the axes the map wraps.
pub fn grid_distance(grid: &MapGrid, x1: i32, y1: i32, x2: i32, y2: i32) -> i32 {
    let mut dx = (x1 - x2).abs();
    if grid.wrap_flags & 1 != 0 && dx > grid.w / 2 {
        dx = grid.w - dx;
    }
    let mut dy = (y1 - y2).abs();
    if grid.wrap_flags & 2 != 0 && dy > grid.h / 2 {
        dy = grid.h - dy;
    }
    distance(dx, dy)
}

/// `0x5EEDB0`: the continent id of the largest body of water among the 8
/// neighbours of `(x, y)` (the first of equals), `-1` when none of them is
/// water. `continents` are the records of `Map+0x214`.
pub fn shore_rank(grid: &MapGrid, continents: &[Continent], x: i32, y: i32) -> i32 {
    let size = |id: i32| usize::try_from(id).ok().and_then(|i| continents.get(i)).map_or(0, |c| c.size);
    let mut best = -1;
    for n in 1..9 {
        let (dx, dy) = spiral_offset(n);
        let Some((nx, ny)) = grid.wrap_and_check(x + dx, y + dy) else { continue };
        let Some(cell) = grid.cell_at(nx, ny) else { continue };
        if !cell.is_water() {
            continue;
        }
        let id = i32::from(cell.continent as i16);
        if id == best || (best != -1 && size(id) <= size(best)) {
            continue;
        }
        best = id;
    }
    best
}

/// `0x5CDCD0(count, vals, keys, mode)`: reorders `vals` and `keys` together
/// by `keys`. The exe visits the key values one at a time (smallest first when
/// `mode` is set) and swaps every element that carries the value to the end of
/// the part not yet placed, so the result is in decreasing key order and the
/// order inside a group of equal keys is a product of those swaps. Only the
/// key values that occur matter (the exe also walks the ones between).
pub fn sort_by_value(vals: &mut [u32], keys: &mut [i32], mode: bool) {
    assert_eq!(vals.len(), keys.len());
    let mut targets = keys.to_vec();
    targets.sort_unstable();
    targets.dedup();
    if !mode {
        targets.reverse();
    }
    let mut last = keys.len() as isize - 1;
    for target in targets {
        if last < 0 {
            break;
        }
        let mut i = 0isize;
        loop {
            if keys[i as usize] == target {
                if i != last {
                    keys.swap(i as usize, last as usize);
                    vals.swap(i as usize, last as usize);
                }
                last -= 1;
            } else {
                i += 1;
            }
            if i > last {
                break;
            }
        }
    }
}

/// What a tile must not have for a start (`0x5EF2BB`..`0x5EF3B1`): a start, a
/// hut or a camp, water. (The exe also refuses tiles with a city, colony, claim
/// or ruin; a generated map has none and [`Cell`] does not model them.)
fn blocked(cell: &Cell) -> bool {
    cell.is_water()
        || cell.flag(PLANE_FEATURE) & START != 0
        || cell.flag(PLANE_OVERLAY) & (HUT | CAMP) != 0
}

/// The continent id of tile `(x, y)`, sign-extended (`vfunc 0xB8`).
fn continent_at(grid: &MapGrid, x: i32, y: i32) -> i32 {
    grid.cell_at(x, y).map_or(-1, |c| i32::from(c.continent as i16))
}

/// `0x5EEEE0`: places the starts of `civs` civilizations.
///
/// * `radius` is `Map+0x158`, the world size's civ distance (`WSIZ` `+0x48`).
/// * `values` is the site value of every tile by cell index (`Map` vtable slot 8).
/// * `slots` is `Map+0x16C`; with `args.fresh` its contents are ignored.
///
/// Returns nothing; the result is `slots` and feature bit [`START`] on the tiles.
pub fn final_pass(
    grid: &mut MapGrid,
    args: &Args,
    civs: i32,
    radius: i32,
    continents: &[Continent],
    rules: &Rules,
    values: &[i32],
    slots: &mut [i32; SLOTS],
) {
    let cells = grid.num_cells();
    assert_eq!(values.len(), cells, "one site value per tile");
    let half = (grid.w >> 1) as usize;
    let coords = |index: i32| {
        let i = (index as u16) as usize;
        let y = (i / half) as i32;
        (2 * (i % half) as i32 + (y & 1), y)
    };

    // Phase 1: luxuries per continent.
    let mut lux = vec![0i32; continents.len()];
    let mut taken = vec![0i32; continents.len()];
    let mut lux_total = 0;
    for good in (0..rules.goods.len()).filter(|&g| rules.goods[g].class == LUXURY) {
        if let Some(cell) = grid.cells.iter().find(|c| c.resource == good as i32) {
            if let Some(n) = lux.get_mut(cell.continent as i16 as usize) {
                *n += 1;
            }
            lux_total += 1;
        }
    }

    // Phase 2: the generator, the cell order and the sort.
    let mut rng = Rng::new((grid.seed() as u32).wrapping_add(START_SEED).wrapping_add(args.salt));
    let mut used = 1; // `nSlots`: the next slot index to fill (slot 0 is never used)
    if args.fresh {
        slots.fill(-1);
        rng.discard(SLOTS);
    } else {
        slots[0] = -1;
        used += (1..=civs.clamp(0, 31) as usize).filter(|&k| slots[k] != -1).count() as i32;
        rng.discard(4);
    }
    let mut order = shuffled_cells(cells, &mut rng);
    let mut keys: Vec<i32> = order.iter().map(|&i| values[i as usize]).collect();
    sort_by_value(&mut order, &mut keys, true);

    // First free slot, `1 + civs` when all are taken.
    let mut free = 1;
    while free <= civs && slots[free as usize] != -1 {
        free += 1;
    }

    // Phase 3: the passes.
    let mut placed = 0;
    for level in 0..LEVELS {
        if used > civs {
            continue;
        }
        for pos in 0..cells {
            if used > civs {
                break;
            }
            let index = order[pos] as i32;
            let (x, y) = coords(index);
            if level < 4 && placed < args.seafarers {
                let id = shore_rank(grid, continents, x, y);
                let size = usize::try_from(id).ok().and_then(|i| continents.get(i)).map(|c| c.size);
                if size.is_none_or(|s| s <= SHORE_MIN_SIZE) {
                    continue;
                }
            }
            if y == 0 || y >= grid.h - 1 || blocked(&grid.cells[index as usize]) {
                continue;
            }
            let cont = continent_at(grid, x, y);
            let slot = usize::try_from(cont).ok();
            if level < 1 && used <= lux_total {
                let got = |v: &Vec<i32>| slot.and_then(|c| v.get(c)).copied().unwrap_or(0);
                if got(&lux) <= got(&taken) {
                    continue;
                }
            }
            let size = slot.and_then(|c| continents.get(c)).map_or(0, |c| c.size);
            if size < BIG_CONTINENT && !(level >= 2 && size >= MEDIUM_CONTINENT) && level < 5 {
                continue;
            }
            if level < 7 && keys[pos] <= 0 {
                continue;
            }
            let reach = if level < 3 { radius } else { radius / 2 };
            let clear = (1..=civs.clamp(0, 31) as usize).all(|k| {
                let other = slots[k];
                if other == -1 {
                    return true;
                }
                let (x2, y2) = coords(other);
                continent_at(grid, x2, y2) != cont || grid_distance(grid, x, y, x2, y2) > reach
            });
            if level < 6 && !clear {
                continue;
            }

            // Accept.
            slots[free as usize] = index;
            grid.cells[index as usize].set_flag(PLANE_FEATURE, START);
            if let Some(n) = slot.and_then(|c| taken.get_mut(c)) {
                *n += 1;
            }
            used += 1;
            placed += 1;
            if free <= civs {
                while free <= civs && slots[free as usize] != -1 {
                    free += 1;
                }
            }
        }
    }

    if !args.fresh {
        return;
    }

    // Phase 4: shuffle the slots 1..used.
    rng.skew(args.salt);
    let used = used as usize;
    for k in 1..used {
        let swap_with = if k as i32 == args.jump {
            (2 * used - 2) / 3 + 1
        } else {
            k + rng.below((used - k) as u32) as usize
        };
        if swap_with != k {
            slots.swap(k, swap_with);
        }
    }

    // Phase 5: neighbours in the slot order share a continent where they can.
    if args.group {
        let continent_of_slot = |slots: &[i32; SLOTS], k: usize| {
            let (x, y) = coords(slots[k]);
            continent_at(grid, x, y)
        };
        for i in 2..used {
            let anchor = continent_of_slot(slots, i - 1);
            for j in i..used {
                if continent_of_slot(slots, j) == anchor {
                    continue;
                }
                if let Some(m) = (j + 1..used).find(|&m| continent_of_slot(slots, m) == anchor) {
                    slots.swap(j, m);
                }
            }
        }
    }
}
