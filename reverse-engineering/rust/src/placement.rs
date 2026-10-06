//! The three stages that put things on a finished map: resources (`0x5F22A0`),
//! goody huts (`0x5F21B0`) and bonus grassland (`0x5F2090`).
//!
//! `generateMap` (`0x5EB580`) runs them in this order after the terrain art,
//! each one called with the constant `1` and each separated by `Map::invalidate`
//! (vtable slot `0xC`). Every stage shuffles the whole cell array first and then
//! walks it, so a tile earlier in the shuffle is preferred. The shuffle is the
//! forward Fisher-Yates of [`shuffled_cells`]; the generators are seeded
//! `(seed + constant) * 1` and, unlike the art stage, are **not** warmed up.
//!
//! | stage | address | seed constant | what it does |
//! |---|---|---|---|
//! | [`place_resources`] | `0x5F22A0` | `0x180E3` | luxuries in clusters, strategics one by one, bonus resources in rounds |
//! | [`place_huts`] | `0x5F21B0` | `0x8ACE` | `cells / 32` random tries to put a goody hut on a tile |
//! | [`bonus_grassland`] | `0x5F2090` | `0x8CF78` | one grassland-based tile in three without a resource gets the bonus-grassland flag |
//!
//! The first two read the world's rules ([`Rules`]): the `GOOD` rows (class and
//! frequency) and which resources each `TERR` row allows. A generated world
//! uses `conquests.biq` for them ([`Rules::conquests`]); a scenario brings its own.
//!
//! # Resources
//!
//! For each `GOOD` row of the right class the stage computes how many copies to
//! place, `n = civs * pct / 100`, where `civs` is the civilization count
//! (`Map+0x15C`, **not** an area) and `pct` is the row's frequency, or, for
//! a row with frequency 0, `rand(26) + rand(26) + 50` (two draws even when the
//! resource is later skipped). The terrain *score* of the resource, one point
//! for every `TERR` row that allows it and four more for the three water rows,
//! scales that: score 0 skips the resource, below 2 halves `n`, below 4 takes
//! three quarters, and `n` is at least 1 (2 from score 4 on). Because the score
//! counts terrains, a resource that grows on many of them is placed in full
//! and one that grows on a single terrain in half.
//!
//! * **Luxuries** (`GOOD` class 1) go first. All copies of a luxury stay on one
//!   continent, the one of the first tile the shuffle offers that
//!   [`Site::can_place`] accepts. After each site the stage tosses a coin
//!   (`rand(2)`, 0 continues) and may add a neighbour (spiral offsets 1 to 8)
//!   that has fewer than three copies around it, repeating while copies remain.
//! * **Strategic resources** (class 2) go second, one copy at a time, each at
//!   the first acceptable tile of the shuffle. They can be anywhere.
//! * **Bonus resources** (every other class) go last, in rounds: every bonus
//!   resource with a non-zero score is tried once per round with probability
//!   `2 / sides` (`sides` 6, 4 or 2 for scores below 2, below 4 and above), and
//!   a tried resource takes the first acceptable tile. Rounds repeat while the
//!   strategic and bonus copies placed together are fewer than `cells / 32`
//!   and the last round placed something.
//!
//! # Acceptable tiles
//!
//! [`Site::can_place`] is `Map` vtable slot `0x44` (`0x5F3320`). A tile is
//! acceptable for resource `g` when all of these hold:
//!
//! 1. for a luxury, the tile's continent has at least 37 tiles (75 while the
//!    shuffle position is in the first two thirds);
//! 2. the tile's terrain allows `g`, the tile has no resource yet and no bonus
//!    grassland flag;
//! 3. no neighbour on the same continent within a radius blocks it (the radius
//!    is a 3x3 for bonus resources and `2 * min(((W + H) / 2) / 50, 4) + 5`
//!    squared spiral offsets for the others): a different resource in the 8
//!    nearest tiles blocks; the same resource blocks only for strategics;
//!    farther out a strategic is blocked by its own kind, other goods by a
//!    different *luxury*;
//! 4. a water tile also needs land within 20 spiral offsets.
//!
//! # Huts and bonus grassland
//!
//! A hut needs a land tile with no start location, camp, city, colony, ruin or
//! resource and no other hut within the 49 nearest spiral offsets
//! ([`hut_site`]). With barbarians off (`-1`) there are no huts.

use crate::cell::{MapGrid, PLANE_FEATURE, PLANE_OVERLAY};
use crate::continents::Continent;
use crate::rng::Rng;
use crate::spiral::spiral_offset;

/// Seed constant of the resource stage, `0x5F2301`.
pub const RESOURCE_SEED: u32 = 0x180E3;
/// Seed constant of the hut stage, `0x5F21BC`.
pub const HUT_SEED: u32 = 0x8ACE;
/// Seed constant of the bonus-grassland stage, `0x5F209B`.
pub const BONUS_GRASSLAND_SEED: u32 = 0x8CF78;

/// Overlay bit of a goody hut (`Cell` slot `0x3C`).
pub const HUT: u32 = 1 << 5;
/// Overlay bit of a barbarian camp (`Cell` slot `0x1C`).
pub const CAMP: u32 = 1 << 7;
/// Feature bit of a start location (`Cell` slot `0x80`).
pub const START: u32 = 1 << 19;
/// Feature bit of bonus grassland (`Cell` slot `0x6C`).
pub const BONUS_GRASSLAND: u32 = 1 << 16;

/// `GOOD` class of a luxury resource.
pub const LUXURY: u32 = 1;
/// `GOOD` class of a strategic resource.
pub const STRATEGIC: u32 = 2;

/// The `TERR` row index from which water terrains start (coast, sea, ocean).
const FIRST_WATER_TERRAIN: usize = 11;
/// A luxury needs a continent of this many tiles (`0x5F3389`).
const LUXURY_CONTINENT: i32 = 0x25;
/// ... and this many more while the shuffle position is in the first two thirds.
const LUXURY_CONTINENT_STRICT_EXTRA: i32 = 0x26;
/// Ring size of the neighbour check of a bonus resource (spiral offsets `1..9`).
const BONUS_RING: i32 = 9;
/// How many spiral offsets a hut keeps clear of other huts.
const HUT_CLEARANCE: i32 = 49;
/// How many spiral offsets a water resource looks for land in (`1..=20`).
const WATER_LAND_RING: i32 = 20;

/// A `GOOD` row, as far as placement and the site evaluator read it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GoodRule {
    /// Row `+0x3C`: 0 bonus, 1 luxury, 2 strategic (`0x5E3700..0x5E3730`).
    pub class: u32,
    /// Row `+0x40`: frequency in percent, 0 to draw `rand(26) + rand(26) + 50`.
    pub freq: i32,
    /// Row `+0x4C`: the `TECH` row that makes the resource usable, `-1` for none.
    pub prerequisite: i32,
    /// Row `+0x50`: food the resource adds to its tile.
    pub food: i32,
    /// Row `+0x54`: shields it adds.
    pub shields: i32,
    /// Row `+0x58`: commerce it adds.
    pub commerce: i32,
}

/// A `TERR` row, as far as the site evaluator reads it (the resource allow-mask
/// is kept apart in [`Rules::terr`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainRule {
    /// Row `+0x64`.
    pub food: i32,
    /// Row `+0x68`.
    pub shields: i32,
    /// Row `+0x6C`.
    pub commerce: i32,
    /// Row `+0x4C`: what irrigation adds to the food.
    pub irrigation: i32,
    /// Row `+0x50`: what a mine adds to the shields.
    pub mining: i32,
    /// Row `+0x54`: what a road adds to the commerce.
    pub road: i32,
    /// Row `+0x70`: the `TFRM` job a worker does here, `-1` for none.
    pub worker_job: i32,
    /// Row byte `+0x78`: cities may be founded here.
    pub allows_cities: bool,
}

/// The rules tables placement and the site evaluator read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rules {
    /// The `GOOD` rows in file order.
    pub goods: Vec<GoodRule>,
    /// For every `TERR` row, the bytes of its resource allow-mask: bit `g & 7`
    /// of byte `g >> 3` allows resource `g` (`0x5F2486`).
    pub terr: Vec<Vec<u8>>,
    /// The rest of every `TERR` row.
    pub terrain: Vec<TerrainRule>,
}

impl Rules {
    /// The `GOOD` and `TERR` tables of the shipped `conquests.biq`, which every
    /// random-map game starts from (`resources.md`).
    pub fn conquests() -> Rules {
        // class, frequency, prerequisite tech, food, shields, commerce.
        let goods: [(u32, i32, i32, i32, i32, i32); 26] = [
            (2, 160, 4, 0, 0, 1), (2, 160, 7, 0, 1, 0), (2, 120, 30, 0, 0, 1), (2, 120, 44, 0, 2, 1),
            (2, 120, 53, 0, 1, 2), (2, 120, 57, 0, 0, 2), (2, 120, 64, 0, 2, 0), (2, 100, 65, 0, 2, 3),
            (1, 0, -1, 1, 0, 1), (1, 0, -1, 0, 1, 1), (1, 0, -1, 0, 0, 1), (1, 0, -1, 0, 0, 2),
            (1, 0, -1, 0, 0, 2), (1, 0, -1, 0, 0, 2), (1, 0, -1, 0, 0, 3), (1, 0, -1, 0, 0, 4),
            (0, 0, -1, 1, 1, 2), (0, 0, -1, 2, 0, 0), (0, 0, -1, 2, 0, 1), (0, 0, -1, 2, 1, 0),
            (0, 0, -1, 2, 0, 0), (0, 0, -1, 0, 0, 4), (0, 0, -1, 1, 0, 1), (0, 0, -1, 1, 0, 1),
            (0, 0, -1, 2, 0, 0), (0, 0, -1, 0, 0, 1),
        ];
        // Desert, plains, grassland, tundra, flood plain, hills, mountains,
        // forest, jungle, marsh, volcano, coast, sea, ocean.
        let masks: [u32; 14] = [
            0x0100_0814, 0x0058_2101, 0x0218_0101, 0x0002_0250, 0x0010_0000, 0x0260_094F,
            0x0020_808E, 0x0002_76A0, 0x0080_D428, 0x0006_0030, 0x0000_0000, 0x0004_0000,
            0x0005_0000, 0x0000_0000,
        ];
        // food, shields, commerce, irrigation, mining, road, worker job, cities.
        let terrain: [(i32, i32, i32, i32, i32, i32, i32, bool); 14] = [
            (0, 1, 0, 1, 1, 1, -1, true), (1, 1, 0, 1, 1, 1, 5, true), (2, 0, 0, 1, 1, 1, 5, true),
            (1, 0, 0, 0, 1, 1, 5, true), (3, 0, 0, 1, 0, 1, -1, true), (1, 1, 0, 0, 2, 1, -1, true),
            (0, 1, 0, 0, 2, 1, -1, false), (1, 2, 0, 0, 0, 1, 6, true), (1, 0, 0, 0, 0, 1, 7, true),
            (1, 0, 0, 0, 0, 1, 7, false), (0, 3, 0, 0, 0, 0, -1, false), (1, 0, 2, 0, 0, 0, -1, false),
            (1, 0, 1, 0, 0, 0, -1, false), (0, 0, 0, 0, 0, 0, -1, false),
        ];
        Rules {
            goods: goods
                .iter()
                .map(|&(class, freq, prerequisite, food, shields, commerce)| GoodRule {
                    class,
                    freq,
                    prerequisite,
                    food,
                    shields,
                    commerce,
                })
                .collect(),
            terr: masks.iter().map(|m| m.to_le_bytes().to_vec()).collect(),
            terrain: terrain
                .iter()
                .map(|&(food, shields, commerce, irrigation, mining, road, worker_job, allows_cities)| TerrainRule {
                    food,
                    shields,
                    commerce,
                    irrigation,
                    mining,
                    road,
                    worker_job,
                    allows_cities,
                })
                .collect(),
        }
    }

    /// Whether terrain row `terrain` allows resource `good`.
    pub fn allows(&self, terrain: usize, good: usize) -> bool {
        self.terr
            .get(terrain)
            .and_then(|mask| mask.get(good >> 3))
            .is_some_and(|byte| byte & (1 << (good & 7)) != 0)
    }

    /// The terrain score of a resource: one point for every `TERR` row that
    /// allows it, and four more for each water row (`0x5F2490`).
    pub fn score(&self, good: usize) -> i32 {
        (0..self.terr.len())
            .filter(|&t| self.allows(t, good))
            .map(|t| if t >= FIRST_WATER_TERRAIN { 5 } else { 1 })
            .sum()
    }
}

/// The forward Fisher-Yates shuffle of all cell indices that every placement
/// stage starts with: `for i in 0..n { swap(i, i + rand(n - i)) }`.
pub fn shuffled_cells(cells: usize, rng: &mut Rng) -> Vec<u32> {
    let mut order: Vec<u32> = (0..cells as u32).collect();
    for i in 0..cells {
        let j = i + rng.below((cells - i) as u32) as usize;
        order.swap(i, j);
    }
    order
}

/// The number of copies of a resource: `civs * pct / 100`, scaled by the
/// terrain score. `None` for score 0, which skips the resource (`0x5F24A1`).
pub fn copies(civs: i32, pct: i32, score: i32) -> Option<i32> {
    if score == 0 {
        return None;
    }
    let n = civs * pct / 100;
    let scaled = match score {
        ..=1 => (f64::from(n) * 0.5) as i32,
        2..=3 => (f64::from(n) * 0.75) as i32,
        _ => n,
    };
    Some(scaled.max(if score >= 4 { 2 } else { 1 }))
}

/// The sides of the die a bonus resource rolls each round (`0x5F2B3E`); it is
/// placed when the roll is 0 or 1.
pub fn bonus_sides(score: i32) -> u32 {
    match score {
        ..=1 => 6,
        2..=3 => 4,
        _ => 2,
    }
}

/// What the tile predicates need besides the map.
pub struct Site<'a> {
    /// The `GOOD` and `TERR` tables.
    pub rules: &'a Rules,
    /// The continent records, indexed by continent id.
    pub continents: &'a [Continent],
}

/// `vfunc 0xB8` of the cell at `(x, y)`, sign-extended the way `0x5F3320` uses it.
fn continent_of(grid: &MapGrid, x: i32, y: i32) -> i32 {
    grid.cell_at(x, y).map_or(-1, |c| i32::from(c.continent as i16))
}

impl Site<'_> {
    /// `Map` vtable slot `0x44` (`0x5F3320`): may resource `good` go on `(x, y)`?
    /// `strict` is set while the shuffle position is in the first two thirds
    /// of the cells (only luxuries read it).
    pub fn can_place(&self, grid: &MapGrid, x: i32, y: i32, good: usize, strict: bool) -> bool {
        let Some(cell) = grid.cell_at(x, y) else { return false };
        let class = self.rules.goods[good].class;
        let cont = continent_of(grid, x, y);
        if class == LUXURY {
            let need = LUXURY_CONTINENT + if strict { LUXURY_CONTINENT_STRICT_EXTRA } else { 0 };
            let size = usize::try_from(cont).ok().and_then(|c| self.continents.get(c)).map_or(0, |c| c.size);
            if (size as i32) < need {
                return false;
            }
        }
        if !self.rules.allows(cell.class().into(), good)
            || cell.resource != -1
            || cell.flag(PLANE_FEATURE) & BONUS_GRASSLAND != 0
        {
            return false;
        }
        let ring = if class == LUXURY || class == STRATEGIC {
            let r = ((grid.w + grid.h) / 2 / 50).min(4);
            (2 * r + 5) * (2 * r + 5)
        } else {
            BONUS_RING
        };
        let strategic = class == STRATEGIC;
        for n in 1..ring {
            let (dx, dy) = spiral_offset(n);
            let Some((nx, ny)) = grid.wrap_and_check(x + dx, y + dy) else { continue };
            let Some(nb) = grid.cell_at(nx, ny) else { continue };
            if continent_of(grid, nx, ny) != cont || nb.resource == -1 {
                continue;
            }
            let same = nb.resource == good as i32;
            let blocked = if n < 9 {
                !same || strategic
            } else if strategic {
                same
            } else {
                !same && self.rules.goods.get(nb.resource as usize).is_some_and(|g| g.class == LUXURY)
            };
            if blocked {
                return false;
            }
        }
        // A resource at sea needs land close by.
        !cell.is_water()
            || (1..=WATER_LAND_RING).any(|n| {
                let (dx, dy) = spiral_offset(n);
                grid.wrap_and_check(x + dx, y + dy)
                    .and_then(|(nx, ny)| grid.cell_at(nx, ny))
                    .is_some_and(|nb| !nb.is_water())
            })
    }
}

/// `Map` vtable slot `0x48` (`0x5F2C60`): may a goody hut go on `(x, y)`?
pub fn hut_site(grid: &MapGrid, x: i32, y: i32) -> bool {
    let Some(cell) = grid.cell_at(x, y) else { return false };
    // The city, colony and ruin tests of the exe read fields a generated map
    // never fills in.
    if cell.is_water()
        || cell.flag(PLANE_FEATURE) & START != 0
        || cell.flag(PLANE_OVERLAY) & CAMP != 0
        || cell.resource != -1
    {
        return false;
    }
    !(0..HUT_CLEARANCE).any(|n| {
        let (dx, dy) = spiral_offset(n);
        grid.wrap_and_check(x + dx, y + dy)
            .and_then(|(nx, ny)| grid.cell_at(nx, ny))
            .is_some_and(|nb| nb.flag(PLANE_OVERLAY) & HUT != 0)
    })
}

/// Writes resource `good` on `(x, y)` (`vfunc 0xEC`) and counts it (`Map+0x14C`).
fn put(grid: &mut MapGrid, x: i32, y: i32, good: usize, counts: &mut [i32]) {
    if let Some(cell) = grid.cell_at_mut(x, y) {
        cell.resource = good as i32;
    }
    counts[good] += 1;
}

/// `0x5F22A0`: places every resource. Returns how many copies of each `GOOD` row
/// were placed (`Map+0x14C`).
///
/// `continents` are the records `finalizeMap` left; `civs` is `Map+0x15C`.
pub fn place_resources(
    grid: &mut MapGrid,
    rules: &Rules,
    continents: &[Continent],
    seed: i32,
    civs: i32,
) -> Vec<i32> {
    let goods = rules.goods.len();
    let cells = grid.num_cells();
    let mut counts = vec![0; goods];
    let mut region = vec![-1i32; goods];
    let mut rng = Rng::new((seed as u32).wrapping_add(RESOURCE_SEED));
    let order = shuffled_cells(cells, &mut rng);
    // `cmp ebp, (2 * cells) / 3` at every canPlace call.
    let two_thirds = (2 * cells / 3) as i32;
    let site = Site { rules, continents };

    // How many copies of `good`, drawing the frequency when the row has none.
    let quantity = |rng: &mut Rng, good: usize| -> Option<i32> {
        let freq = rules.goods[good].freq;
        let pct = if freq != 0 { freq } else { rng.below(26) + rng.below(26) + 50 };
        copies(civs, pct, rules.score(good))
    };
    let first_site = |grid: &MapGrid, good: usize| {
        order
            .iter()
            .map(|&i| grid.coords(i as usize))
            .find(|&(x, y)| site.can_place(grid, x, y, good, true))
    };

    // Luxuries: clustered, all on one continent.
    for good in (0..goods).filter(|&g| rules.goods[g].class == LUXURY) {
        let Some(n) = quantity(&mut rng, good) else { continue };
        let mut placed = 0;
        while placed < n {
            for (k, &idx) in order.iter().enumerate() {
                let (x, y) = grid.coords(idx as usize);
                if region[good] != -1 && continent_of(grid, x, y) != region[good] {
                    continue;
                }
                let strict = (k as i32) + 1 < two_thirds;
                if !site.can_place(grid, x, y, good, strict) {
                    continue;
                }
                region[good] = continent_of(grid, x, y);
                put(grid, x, y, good, &mut counts);
                // Neighbours: a coin toss per attempt, up to the copies left.
                while rng.below(2) == 0 {
                    for r in 1..=8 {
                        let (dx, dy) = spiral_offset(r);
                        let Some((cx, cy)) = grid.wrap_and_check(x + dx, y + dy) else { continue };
                        let around = (1..=8)
                            .filter_map(|m| {
                                let (ox, oy) = spiral_offset(m);
                                let (ax, ay) = grid.wrap_and_check(cx + ox, cy + oy)?;
                                grid.cell_at(ax, ay)
                            })
                            .filter(|c| c.resource == good as i32)
                            .count();
                        if around < 3 && site.can_place(grid, cx, cy, good, strict) {
                            put(grid, cx, cy, good, &mut counts);
                            placed += 1;
                            break;
                        }
                    }
                    if placed >= n {
                        break;
                    }
                }
                break;
            }
            placed += 1;
        }
    }

    // Strategic resources: one copy at a time at the first acceptable tile.
    let mut total = 0;
    for good in (0..goods).filter(|&g| rules.goods[g].class == STRATEGIC) {
        let Some(n) = quantity(&mut rng, good) else { continue };
        for _ in 0..n {
            if let Some((x, y)) = first_site(grid, good) {
                put(grid, x, y, good, &mut counts);
                total += 1;
            }
        }
    }

    // Bonus resources: rounds of one try each, until enough have been placed.
    let bonus: Vec<usize> =
        (0..goods).filter(|&g| !matches!(rules.goods[g].class, LUXURY | STRATEGIC)).collect();
    loop {
        let before = total;
        for &good in &bonus {
            let score = rules.score(good);
            if score == 0 || rng.below(bonus_sides(score)) > 1 {
                continue;
            }
            if let Some((x, y)) = first_site(grid, good) {
                put(grid, x, y, good, &mut counts);
                total += 1;
            }
        }
        if total == before || total >= (cells >> 5) as i32 {
            break;
        }
    }
    counts
}

/// `0x5F21B0`: tries `cells / 32` random tiles for a goody hut. `barbarians`
/// is the derived barbarian setting (`Map+0x10`); `-1` (none) places no huts.
pub fn place_huts(grid: &mut MapGrid, seed: i32, barbarians: i32) {
    let cells = grid.num_cells();
    if barbarians == -1 || cells < 32 {
        return;
    }
    let mut rng = Rng::new((seed as u32).wrapping_add(HUT_SEED));
    for _ in 0..cells >> 5 {
        let idx = rng.below(cells as u32) as usize;
        let (x, y) = grid.coords(idx);
        if hut_site(grid, x, y) {
            grid.cells[idx].set_flag(PLANE_OVERLAY, HUT);
        }
    }
}

/// `0x5F2090`: gives a third of the grassland-based tiles without a resource
/// the bonus-grassland flag. Relief and forest on grass count (their base
/// terrain is grassland).
pub fn bonus_grassland(grid: &mut MapGrid, seed: i32) {
    let mut rng = Rng::new((seed as u32).wrapping_add(BONUS_GRASSLAND_SEED));
    let order = shuffled_cells(grid.num_cells(), &mut rng);
    for idx in order {
        let cell = &mut grid.cells[idx as usize];
        if cell.sub_class() == 2 && cell.resource == -1 && rng.below(3) == 0 {
            cell.set_flag(PLANE_FEATURE, BONUS_GRASSLAND);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terrain_scores_of_the_shipped_resources() {
        let rules = Rules::conquests();
        let score = |name: usize| rules.score(name);
        // Horses: plains, grassland, hills.
        assert_eq!(score(0), 3);
        // Fish: marsh (1), coast (5) and sea (5).
        assert_eq!(score(18), 5 + 5 + 1);
        // Whales: sea only.
        assert_eq!(score(16), 5);
        // Wheat: plains, grassland, flood plain.
        assert_eq!(score(20), 3);
        // Oasis: desert only.
        assert_eq!(score(24), 1);
    }

    #[test]
    fn copies_scale_with_the_score() {
        assert_eq!(copies(8, 160, 0), None);
        assert_eq!(copies(8, 160, 1), Some(6)); // 12 / 2
        assert_eq!(copies(8, 160, 3), Some(9)); // 12 * 3 / 4
        assert_eq!(copies(8, 160, 5), Some(12));
        // The floors.
        assert_eq!(copies(1, 50, 1), Some(1));
        assert_eq!(copies(1, 50, 5), Some(2));
    }

    #[test]
    fn bonus_dice() {
        assert_eq!([0, 1, 2, 3, 4, 9].map(bonus_sides), [6, 6, 4, 4, 2, 2]);
    }

    #[test]
    fn the_shuffle_is_a_permutation() {
        let mut rng = Rng::new(1234);
        let mut order = shuffled_cells(500, &mut rng);
        order.sort_unstable();
        assert!(order.iter().enumerate().all(|(i, &v)| i as u32 == v));
    }
}
