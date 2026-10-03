//! The computer's civilizations.
//!
//! Japan is the human's; Rome, Egypt, and China are played here. The AI is a
//! plain rule-based player that drives the same pieces a human does: it sets
//! unit paths (movement and attacks then run through `units::drive_movement`
//! and the combat sequencer), starts worker jobs, asks for cities to be
//! founded, and picks what each city builds. It plays during its own turn
//! and then ends it with `TurnEnded`.
//!
//! Deviations from a real Civ3 AI, all deliberate for a basic opponent:
//! - It sees every unit and city on the map (no fog for its decisions), but
//!   keeps its own record of explored ground for its scouts.
//! - It fights only the civilizations it is at war with (`diplomacy.rs`) and
//!   builds only what its advances allow (`research.rs`). Who it goes to war
//!   with, and when, is decided by `diplomacy::Diplomacy::war_plan` and the
//!   border pressure, not here.
//! - It ignores happiness, tile trade, and the luxury rate.
//!
//! The decisions are plain functions of the map and a snapshot of the world,
//! so they are tested without the engine.

use bevy::prelude::*;
use std::collections::{HashMap, HashSet, VecDeque};

use crate::cities::{self, City, FoundCityOrder, Production};
use crate::civs::{CIV_COUNT, Civilizations, is_ai};
use crate::combat;
use crate::economy;
use crate::improvements::{self, Work, WorkAction};
use crate::map::{GameMap, Relief, Tile, move_cost};
use crate::splash::SplashUp;
use crate::units::{Turn, TurnEnded, Unit, UnitAnim, UnitType, def};

/// The most cities the computer will found.
pub const MAX_CITIES: usize = 8;
/// From this turn on the computer goes looking for fights; before it, it
/// only defends what is near.
pub const AGGRESSION_TURN: u32 = 15;
/// An enemy this close to a city or a unit counts as a threat.
const THREAT_RANGE: i32 = 3;
/// How far a free soldier looks for prey once the aggression turn is past.
const HUNT_RANGE: i32 = 14;
/// How far from a settler a city site may be.
const SITE_RANGE: i32 = 12;
/// Free soldiers needed before the computer marches on a distant city.
const CAMPAIGN_SIZE: usize = 3;
/// Real seconds the computer may take for one turn before it is cut off.
const TURN_BUDGET: f32 = 40.0;
/// Frames with nothing left to do before the turn is handed over.
const SETTLE_FRAMES: u32 = 4;

/// Where the computer is in its turn.
#[derive(Resource, Default)]
pub struct AiState {
    /// Civilization and turn being played.
    civ: Option<usize>,
    turn: u32,
    /// Cities have chosen their builds for this turn.
    managed: bool,
    /// Units that have nothing more to decide this turn.
    done: HashSet<Entity>,
    elapsed: f32,
    quiet_frames: u32,
    /// Where each settler and worker is headed, kept across turns so a plan
    /// is not dropped and redone every frame.
    targets: HashMap<Entity, (i32, i32)>,
    /// What each civilization has explored, for its own scouts.
    known: Vec<Vec<bool>>,
}

/// A snapshot of every city and unit, so decisions can look at the whole
/// board while the units themselves are being changed.
pub struct Board {
    cities: Vec<City>,
    at: HashMap<(i32, i32), Vec<(Entity, Unit)>>,
    /// Civilization owning each claimed tile.
    owner: HashMap<(i32, i32), usize>,
    /// `war[a][b]`: `a` and `b` are at war. A board starts with every civ at
    /// war with every other, which `with_war` replaces.
    war: [[bool; CIV_COUNT]; CIV_COUNT],
}

impl Board {
    pub fn new(cities: Vec<City>, units: Vec<(Entity, Unit)>, map: &GameMap) -> Self {
        let refs: Vec<&City> = cities.iter().collect();
        let owner = cities::territory(map, &refs)
            .into_iter()
            .map(|(tile, i)| (tile, cities[i].civ))
            .collect();
        let mut at: HashMap<(i32, i32), Vec<(Entity, Unit)>> = HashMap::new();
        for (e, u) in units {
            at.entry((u.x, u.y)).or_default().push((e, u));
        }
        Board { cities, at, owner, war: [[true; CIV_COUNT]; CIV_COUNT] }
    }

    /// The wars of the moment.
    pub fn with_war(mut self, war: [[bool; CIV_COUNT]; CIV_COUNT]) -> Self {
        self.war = war;
        self
    }

    /// `a` is at war with `b`.
    fn at_war(&self, a: usize, b: usize) -> bool {
        a != b && self.war[a][b]
    }

    fn city_at(&self, tile: (i32, i32)) -> Option<&City> {
        self.cities.iter().find(|c| (c.x, c.y) == tile)
    }

    /// Units standing on `tile` that belong to a civ at war with `civ`.
    fn foreign_at(&self, tile: (i32, i32), civ: usize) -> Vec<(Entity, Unit)> {
        self.at
            .get(&tile)
            .map(|v| v.iter().filter(|(_, u)| self.at_war(civ, u.civ)).cloned().collect())
            .unwrap_or_default()
    }

    /// An enemy (a civ at war with `civ`) holds the tile with a unit or a city.
    fn enemy_on(&self, tile: (i32, i32), civ: usize) -> bool {
        self.city_at(tile).is_some_and(|c| self.at_war(civ, c.civ))
            || self
                .at
                .get(&tile)
                .is_some_and(|v| v.iter().any(|(_, u)| self.at_war(civ, u.civ)))
    }

    /// Another civ's unit or city holds the tile, at war or not: the way is
    /// shut either way.
    fn held_by_other(&self, tile: (i32, i32), civ: usize) -> bool {
        self.city_at(tile).is_some_and(|c| c.civ != civ)
            || self
                .at
                .get(&tile)
                .is_some_and(|v| v.iter().any(|(_, u)| u.civ != civ))
    }

    fn mine(&self, civ: usize) -> impl Iterator<Item = &City> {
        self.cities.iter().filter(move |c| c.civ == civ)
    }

    fn units_of(&self, civ: usize) -> impl Iterator<Item = &Unit> {
        self.at.values().flatten().map(|(_, u)| u).filter(move |u| u.civ == civ)
    }

    fn city_spots(&self) -> Vec<(i32, i32)> {
        self.cities.iter().map(|c| (c.x, c.y)).collect()
    }
}

// ---------------------------------------------------------------------------
// City sites
// ---------------------------------------------------------------------------

/// How good a city on `(x, y)` would be: the food, shields and commerce of
/// the ground it could work, discounting what existing cities already
/// cover, plus a bonus for the coast.
pub fn site_score(map: &GameMap, cities: &[(i32, i32)], x: i32, y: i32) -> i32 {
    let value = |t: &Tile| {
        let (f, s) = crate::map::yields(t);
        f as i32 * 3 + s as i32 * 2 + cities::tile_commerce(t) as i32
    };
    let mut score = map.get(x, y).map(value).unwrap_or(0);
    for (rx, ry) in cities::radius_tiles(map, x, y) {
        let Some(t) = map.get(rx, ry) else { continue };
        let shared = cities.iter().any(|&c| map.distance(c, (rx, ry)) <= 2);
        let v = value(t);
        score += if shared { v / 2 } else { v };
    }
    if map
        .neighbors(x, y)
        .iter()
        .any(|&(nx, ny)| map.get(nx, ny).is_some_and(|t| improvements::is_water_base(t.base)))
    {
        score += 4;
    }
    score
}

/// Whether the computer may settle `(x, y)`: legal, three tiles clear of
/// every city, not inside another civilization's border, and with no enemy
/// standing on it.
fn site_ok(map: &GameMap, board: &Board, civ: usize, claimed: &HashSet<(i32, i32)>, t: (i32, i32)) -> bool {
    let spots = board.city_spots();
    cities::can_found(map, &spots, t.0, t.1)
        && spots.iter().all(|&c| map.distance(c, t) >= 3)
        && claimed.iter().all(|&c| map.distance(c, t) >= 3)
        && board.owner.get(&t).is_none_or(|&o| o == civ)
        && !board.held_by_other(t, civ)
}

/// The best reachable city site near `from`, by score less the walk.
pub fn best_site(
    map: &GameMap,
    board: &Board,
    civ: usize,
    from: (i32, i32),
    claimed: &HashSet<(i32, i32)>,
) -> Option<(i32, i32)> {
    let spots = board.city_spots();
    let mut scored: Vec<(i32, (i32, i32))> = vec![];
    for dy in -SITE_RANGE..=SITE_RANGE {
        for dx in -SITE_RANGE..=SITE_RANGE {
            let y = from.1 + dy;
            if y < 0 || y >= map.h {
                continue;
            }
            let t = (map.wrap_x(from.0 + dx), y);
            if site_ok(map, board, civ, claimed, t) {
                scored.push((site_score(map, &spots, t.0, t.1) - 2 * map.distance(from, t), t));
            }
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    // Only the front-runners are worth a path search.
    scored
        .into_iter()
        .take(6)
        .find(|&(_, t)| map.find_path(from, t).is_some())
        .map(|(_, t)| t)
}

// ---------------------------------------------------------------------------
// What each city builds
// ---------------------------------------------------------------------------

/// What a city's choice of build depends on.
#[derive(Clone, Debug, Default)]
pub struct Needs {
    /// Soldiers standing in this city.
    pub defenders_here: usize,
    /// Soldiers this city wants for its own defense.
    pub garrison: usize,
    /// All of the civ's cities.
    pub cities: usize,
    /// Settlers alive or being built in the civ's other cities.
    pub settlers: usize,
    /// Workers alive or being built in the civ's other cities.
    pub workers: usize,
    /// Soldiers alive.
    pub soldiers: usize,
    /// Soldiers wanted across the civ.
    pub soldiers_wanted: usize,
    /// Soldiers above this are not worth their upkeep.
    pub soldiers_cap: usize,
    /// Every unit the civ owns, against its free support.
    pub units: usize,
    /// There is ground left to settle.
    pub room: bool,
    pub turn: u32,
    /// Upkeep is covered with gold to spare.
    pub rich: bool,
    /// The city's food surplus.
    pub net_food: i16,
    /// The city's shields per turn.
    pub shield_rate: u8,
    /// An enemy soldier is near.
    pub threatened: bool,
    /// Productions the civ lacks the advance for.
    pub locked: Vec<Production>,
}

impl Needs {
    /// The civ can build `p`.
    fn can(&self, p: Production) -> bool {
        !self.locked.contains(&p)
    }

    /// The best defender the civ has the advance for.
    fn defender(&self) -> Production {
        [Production::Spearman, Production::Archer, Production::Warrior]
            .into_iter()
            .find(|&p| self.can(p))
            .unwrap_or(Production::Warrior)
    }
}

/// Choose a city's build. Order of concern: a garrison, expansion, workers,
/// an army, then buildings.
pub fn choose_build(city: &City, n: &Needs) -> Production {
    use Production::*;
    // 1. Nobody home: the quickest soldier available.
    if n.defenders_here == 0 {
        return if n.soldiers == 0 { Warrior } else { n.defender() };
    }
    if n.defenders_here < n.garrison && n.threatened {
        return n.defender();
    }
    // 2. Expansion, while there is room and the city can spare the people.
    if n.room
        && n.cities < MAX_CITIES
        && n.settlers < 2
        && settler_ready(city, n)
    {
        return Settler;
    }
    // 3. Workers, about one per city.
    if n.workers < n.cities.min(3) {
        return Worker;
    }
    // 4. Soldiers up to the wanted count, defenders first.
    if n.soldiers < n.soldiers_wanted {
        if n.defenders_here < n.garrison {
            return n.defender();
        }
        return attacker(city, n);
    }
    // 5. Buildings, when the treasury can carry their upkeep.
    if n.rich {
        if n.can(Temple) && !city.has(Temple) && city.size >= 3 {
            return Temple;
        }
        if n.can(Granary) && !city.has(Granary) && city.size >= 4 {
            return Granary;
        }
        if n.can(Barracks) && !city.has(Barracks) && n.turn >= AGGRESSION_TURN {
            return Barracks;
        }
    }
    // 6. More army, within what the cities support for free.
    if (n.units < n.cities * 4 + 2 && n.soldiers < n.soldiers_cap) || n.threatened {
        return attacker(city, n);
    }
    // Nothing worth adding: a hand or a spare soldier, never an endless army.
    if n.workers < n.cities * 2 {
        Worker
    } else {
        Warrior
    }
}

/// A city can finish a Settler soon: it has the people, or is about to
/// (a Settler held for size 3 would idle the shield box).
fn settler_ready(city: &City, n: &Needs) -> bool {
    if city.size >= cities::SETTLER_MIN_SIZE {
        return true;
    }
    // Otherwise start it when the shield box will fill about as the city
    // reaches size 3, so neither waits long for the other.
    if n.net_food <= 0 {
        return false;
    }
    let mut food_needed = economy::food_box(city.size) as i32 - city.food as i32;
    for size in city.size + 1..cities::SETTLER_MIN_SIZE {
        food_needed += economy::food_box(size) as i32;
    }
    let growth = (food_needed + n.net_food as i32 - 1) / n.net_food as i32;
    let left = Production::Settler.cost() as i32 - city.shields as i32;
    let build = (left.max(0) + n.shield_rate.max(1) as i32 - 1) / n.shield_rate.max(1) as i32;
    growth <= build + 4
}

/// Archers hit hard, Horsemen run: alternate by how old the city is.
fn attacker(city: &City, n: &Needs) -> Production {
    let preferred = if (city.founded + n.soldiers as u32) % 3 == 0 {
        Production::Horseman
    } else {
        Production::Archer
    };
    [preferred, Production::Archer, Production::Horseman, Production::Warrior]
        .into_iter()
        .find(|&p| n.can(p))
        .unwrap_or(Production::Warrior)
}

/// The most soldiers a civ with these garrisons keeps; the rest are upkeep.
pub fn army_cap(garrisons: usize, cities: usize) -> usize {
    garrisons + 4 * cities + 4
}

/// Soldiers a city wants for its own defense.
pub fn garrison_need(city: &City, threatened: bool) -> usize {
    1 + usize::from(city.size >= 4) + usize::from(threatened)
}

/// An enemy soldier within `THREAT_RANGE` of `tile`.
fn threatened(map: &GameMap, board: &Board, civ: usize, tile: (i32, i32)) -> bool {
    board
        .at
        .values()
        .flatten()
        .any(|(_, u)| board.at_war(civ, u.civ) && def(u.utype).attack > 0 && map.distance((u.x, u.y), tile) <= THREAT_RANGE)
}

// ---------------------------------------------------------------------------
// Orders for units
// ---------------------------------------------------------------------------

/// What a unit does with its turn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Act {
    /// Nothing to change.
    Stay,
    Fortify,
    /// Walk this path (movement continues on its own).
    Go(Vec<(i32, i32)>),
    /// Attack the adjacent tile.
    Attack((i32, i32)),
    Found,
    Work(WorkAction),
    /// Nothing left for this unit to do, and it costs upkeep.
    Disband,
}

/// The part of `path` before the first tile an enemy holds: a walk never
/// runs into a fight it did not choose.
fn safe_walk(board: &Board, civ: usize, path: &[(i32, i32)]) -> Vec<(i32, i32)> {
    path.iter()
        .take_while(|&&t| !board.held_by_other(t, civ))
        .copied()
        .collect()
}

/// The job a worker should do on a tile, if any: irrigation for farmland,
/// a mine on hills and mountains, otherwise a road.
pub fn job_at(map: &GameMap, cities: &[(i32, i32)], tile: (i32, i32)) -> Option<WorkAction> {
    let t = map.get(tile.0, tile.1)?;
    if cities.contains(&tile) || t.hut || t.camp {
        return None;
    }
    if improvements::can_irrigate(map, tile.0, tile.1) && !t.mine {
        Some(WorkAction::Irrigate)
    } else if improvements::can_mine(map, tile.0, tile.1)
        && t.relief != Relief::Flat
        && !t.irrigation
    {
        Some(WorkAction::Mine)
    } else if improvements::can_road(map, tile.0, tile.1) {
        Some(WorkAction::Road)
    } else {
        None
    }
}

/// Chance the attacker wins a fight against whatever defends `at`, or 1.0
/// when nothing there can fight back (it is taken instead).
pub fn attack_chance(map: &GameMap, att: &Unit, stack: &[(Entity, Unit)], city_size: Option<u8>) -> f64 {
    let Some(d) = combat::pick_defender(map, att, city_size, stack) else {
        return 1.0;
    };
    let dfn = &stack.iter().find(|(e, _)| *e == d).expect("picked from the stack").1;
    let odds = combat::round_odds(map, att, dfn, city_size);
    1.0 - combat::defender_win_chance(odds, att.hp(), dfn.hp())
}

/// The nearest passable unseen tile (or hut) from `from`, as a path.
pub fn explore_path(map: &GameMap, known: &[bool], from: (i32, i32)) -> Option<Vec<(i32, i32)>> {
    let mut queue = VecDeque::from([from]);
    let mut done = HashSet::from([from]);
    while let Some((x, y)) = queue.pop_front() {
        for (nx, ny) in map.neighbors(x, y) {
            if !done.insert((nx, ny)) {
                continue;
            }
            let Some(t) = map.get(nx, ny) else { continue };
            if move_cost(t).is_none() {
                continue;
            }
            if !known.get(map.idx(nx, ny)).copied().unwrap_or(false) || t.hut || t.camp {
                if let Some(p) = map.find_path(from, (nx, ny)) {
                    if !p.is_empty() {
                        return Some(p);
                    }
                }
            }
            queue.push_back((nx, ny));
        }
    }
    None
}

/// Everything a unit's decision looks at.
struct Brain<'a> {
    map: &'a GameMap,
    board: &'a Board,
    civ: usize,
    turn: u32,
    known: &'a [bool],
    /// Soldiers beyond what the cities need at home.
    surplus: usize,
    /// Soldiers already told to hold each city.
    garrisoned: HashMap<(i32, i32), usize>,
    /// Tiles other settlers and workers are headed for.
    claimed: HashSet<(i32, i32)>,
}

impl Brain<'_> {
    fn own_city_at(&self, tile: (i32, i32)) -> Option<&City> {
        self.board.city_at(tile).filter(|c| c.civ == self.civ)
    }

    fn nearest_own_city(&self, from: (i32, i32)) -> Option<&City> {
        self.board
            .mine(self.civ)
            .min_by_key(|c| self.map.distance(from, (c.x, c.y)))
    }

    /// Walk to a tile, stopping short of enemies; stay if there is no way.
    fn walk_to(&self, u: &Unit, goal: (i32, i32)) -> Act {
        match self.map.find_path((u.x, u.y), goal) {
            Some(p) => {
                let walk = safe_walk(self.board, self.civ, &p);
                if walk.is_empty() { Act::Stay } else { Act::Go(walk) }
            }
            None => Act::Stay,
        }
    }

    fn think(&mut self, e: Entity, u: &Unit, target: Option<(i32, i32)>) -> (Act, Option<(i32, i32)>) {
        // A unit's own plan does not stand in its way.
        if let Some(t) = target {
            self.claimed.remove(&t);
        }
        match u.utype {
            UnitType::Settler => self.settler(u, target),
            UnitType::Worker => self.worker(u, target),
            UnitType::Scout => (self.scout(u), None),
            _ => (self.soldier(e, u), None),
        }
    }

    fn settler(&mut self, u: &Unit, target: Option<(i32, i32)>) -> (Act, Option<(i32, i32)>) {
        let here = (u.x, u.y);
        let others: HashSet<(i32, i32)> = self.claimed.clone();
        // The first city goes down where the party landed.
        let first = self.board.mine(self.civ).next().is_none();
        let site = if first && site_ok(self.map, self.board, self.civ, &others, here) {
            Some(here)
        } else {
            target
                .filter(|&t| site_ok(self.map, self.board, self.civ, &others, t))
                .or_else(|| best_site(self.map, self.board, self.civ, here, &others))
        };
        match site {
            Some(s) if s == here => (Act::Found, None),
            Some(s) => {
                self.claimed.insert(s);
                (self.walk_to(u, s), Some(s))
            }
            None => (Act::Stay, None),
        }
    }

    fn worker(&mut self, u: &Unit, target: Option<(i32, i32)>) -> (Act, Option<(i32, i32)>) {
        let spots = self.board.city_spots();
        let here = (u.x, u.y);
        let job = |t: (i32, i32)| job_at(self.map, &spots, t);
        let keep = target.filter(|&t| job(t).is_some() && !self.claimed.contains(&t));
        let tile = keep.or_else(|| {
            // Worked ground first, then anything else in the radius.
            let mut options: Vec<(i32, (i32, i32))> = vec![];
            for c in self.board.mine(self.civ) {
                for t in cities::radius_tiles(self.map, c.x, c.y) {
                    if job(t).is_none() || self.claimed.contains(&t) {
                        continue;
                    }
                    if self.board.owner.get(&t).is_some_and(|&o| o != self.civ) {
                        continue;
                    }
                    let penalty = if c.worked.contains(&t) { 0 } else { 6 };
                    options.push((self.map.distance(here, t) + penalty, t));
                }
            }
            options.sort();
            options
                .into_iter()
                .take(3)
                .map(|(_, t)| t)
                .find(|&t| t == here || self.map.find_path(here, t).is_some())
        });
        let Some(tile) = tile else {
            return (Act::Stay, None);
        };
        self.claimed.insert(tile);
        if tile == here {
            let action = job(tile).expect("a job was found here");
            (Act::Work(action), Some(tile))
        } else {
            (self.walk_to(u, tile), Some(tile))
        }
    }

    fn scout(&self, u: &Unit) -> Act {
        if let Some(p) = explore_path(self.map, self.known, (u.x, u.y)) {
            let walk = safe_walk(self.board, self.civ, &p);
            if !walk.is_empty() {
                return Act::Go(walk);
            }
        }
        // Nothing left to see: a scout is only upkeep from here on.
        Act::Disband
    }

    fn soldier(&mut self, _e: Entity, u: &Unit) -> Act {
        let here = (u.x, u.y);
        // Hold a city that wants defenders.
        if let Some(c) = self.own_city_at(here) {
            let need = garrison_need(c, threatened(self.map, self.board, self.civ, here));
            let have = self.garrisoned.get(&here).copied().unwrap_or(0);
            if have < need {
                *self.garrisoned.entry(here).or_default() += 1;
                return Act::Fortify;
            }
        }
        // A hurt unit goes home to mend.
        let hurt = u.hp() * 2 <= u.max_hp();
        if !hurt && let Some(act) = self.hunt(u) {
            return act;
        }
        // Back to a city, to the one most short of defenders first.
        let lacking = self
            .board
            .mine(self.civ)
            .filter(|c| {
                let tile = (c.x, c.y);
                self.garrisoned.get(&tile).copied().unwrap_or(0)
                    < garrison_need(c, threatened(self.map, self.board, self.civ, tile))
            })
            .min_by_key(|c| self.map.distance(here, (c.x, c.y)))
            .map(|c| (c.x, c.y));
        let home = lacking.or_else(|| self.nearest_own_city(here).map(|c| (c.x, c.y)));
        match home {
            Some(h) if h == here => Act::Fortify,
            Some(h) => {
                let walk = self.walk_to(u, h);
                if walk == Act::Stay { Act::Fortify } else { walk }
            }
            None => Act::Fortify,
        }
    }

    /// Pick a fight worth having, or march toward one.
    fn hunt(&self, u: &Unit) -> Option<Act> {
        if def(u.utype).attack == 0 {
            return None;
        }
        let here = (u.x, u.y);
        let aggressive = self.turn >= AGGRESSION_TURN;
        let near_home = |t: (i32, i32)| {
            self.map.distance(here, t) <= THREAT_RANGE
                || self.board.mine(self.civ).any(|c| self.map.distance((c.x, c.y), t) <= THREAT_RANGE)
        };
        let range = if aggressive { HUNT_RANGE } else { THREAT_RANGE };

        let mut prey: Vec<(f64, (i32, i32))> = vec![];
        let mut tiles: HashSet<(i32, i32)> = self.board.at.keys().copied().collect();
        tiles.extend(self.board.cities.iter().map(|c| (c.x, c.y)));
        for t in tiles {
            let dist = self.map.distance(here, t);
            if dist > range || !self.board.enemy_on(t, self.civ) {
                continue;
            }
            if !aggressive && !near_home(t) {
                continue;
            }
            let city = self.board.city_at(t).filter(|c| self.board.at_war(self.civ, c.civ));
            let stack = self.board.foreign_at(t, self.civ);
            // Only a city whose owner differs is a city to take; a tile held
            // only by a city and no units is empty and falls.
            let chance = attack_chance(self.map, u, &stack, city.map(|c| c.size));
            let value = match (city, stack.iter().any(|(_, s)| def(s.utype).defense > 0)) {
                (Some(c), _) => 4.0 + 0.3 * c.size as f64,
                (None, false) => 2.5,
                (None, true) => 1.0 + 0.2 * stack.len() as f64,
            };
            // A big army spends lives on a city: each blow wears the
            // defenders down for the next.
            let needed = match city {
                Some(_) if self.surplus >= CAMPAIGN_SIZE => 0.12,
                Some(_) => 0.4,
                None => 0.5,
            };
            if chance < needed {
                continue;
            }
            prey.push((value * chance / (dist as f64 + 1.5), t));
        }
        prey.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        for &(_, t) in prey.iter().take(4) {
            let Some(path) = self.map.find_path(here, t) else {
                continue;
            };
            if path.len() == 1 {
                if u.attacked || u.moves == 0 {
                    continue;
                }
                return Some(Act::Attack(t));
            }
            // Walk up to the target, not onto it.
            let walk = safe_walk(self.board, self.civ, &path[..path.len() - 1]);
            if !walk.is_empty() {
                return Some(Act::Go(walk));
            }
        }
        // Nothing to hit: a big enough force marches on the nearest city.
        if aggressive && self.surplus >= CAMPAIGN_SIZE {
            let goal = self
                .board
                .cities
                .iter()
                .filter(|c| self.board.at_war(self.civ, c.civ))
                .min_by_key(|c| self.map.distance(here, (c.x, c.y)))?;
            let path = self.map.find_path(here, (goal.x, goal.y))?;
            let walk = safe_walk(self.board, self.civ, &path[..path.len().saturating_sub(1)]);
            if !walk.is_empty() {
                return Some(Act::Go(walk));
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// The turn
// ---------------------------------------------------------------------------

/// Remember what each civilization's units and cities can see.
fn refresh_known(state: &mut AiState, map: &GameMap, board: &Board) {
    if state.known.len() != CIV_COUNT {
        state.known = vec![vec![]; CIV_COUNT];
    }
    for civ in 0..CIV_COUNT {
        if !is_ai(civ) {
            continue;
        }
        let known = &mut state.known[civ];
        known.resize(map.tiles.len(), false);
        let mut light = |x: i32, y: i32, r: i32| {
            for dy in -r..=r {
                let ny = y + dy;
                if ny < 0 || ny >= map.h {
                    continue;
                }
                for dx in -r..=r {
                    known[map.idx(map.wrap_x(x + dx), ny)] = true;
                }
            }
        };
        for u in board.units_of(civ) {
            let hill = matches!(map.get(u.x, u.y).map(|t| t.relief), Some(Relief::Hill | Relief::Mountain));
            light(u.x, u.y, def(u.utype).sight as i32 + i32::from(hill));
        }
        for c in board.mine(civ) {
            light(c.x, c.y, 2);
        }
    }
}

/// Pick every city's build for the turn.
fn manage_cities(
    civ: usize,
    turn: u32,
    map: &GameMap,
    board: &Board,
    gold: u32,
    room: bool,
    cities_q: &mut Query<(Entity, &mut City)>,
) {
    let mine: Vec<&City> = board.mine(civ).collect();
    let soldiers = board.units_of(civ).filter(|u| def(u.utype).attack > 0).count();
    let units = board.units_of(civ).count();
    let net = economy::finance(map, mine.iter().copied(), units).net();
    let rich = net >= 2 && gold >= 10;
    let wanted: usize = mine
        .iter()
        .map(|c| garrison_need(c, threatened(map, board, civ, (c.x, c.y))))
        .sum::<usize>()
        + 2
        + mine.len();
    let count = |t: UnitType| board.units_of(civ).filter(|u| u.utype == t).count();
    let building = |p: Production, except: (i32, i32)| {
        mine.iter().filter(|c| c.production == p && (c.x, c.y) != except).count()
    };
    let entities: Vec<(Entity, (i32, i32))> = cities_q
        .iter()
        .filter(|(_, c)| c.civ == civ)
        .map(|(e, c)| (e, (c.x, c.y)))
        .collect();
    for (e, tile) in entities {
        let Ok((_, mut city)) = cities_q.get_mut(e) else {
            continue;
        };
        let here = board.at.get(&tile).map(|v| v.as_slice()).unwrap_or(&[]);
        let danger = threatened(map, board, civ, tile);
        let needs = Needs {
            defenders_here: here
                .iter()
                .filter(|(_, u)| u.civ == civ && def(u.utype).attack > 0)
                .count(),
            garrison: garrison_need(&city, danger),
            cities: mine.len(),
            settlers: count(UnitType::Settler) + building(Production::Settler, tile),
            workers: count(UnitType::Worker) + building(Production::Worker, tile),
            soldiers,
            soldiers_wanted: wanted,
            soldiers_cap: army_cap(wanted - 2 - mine.len(), mine.len()),
            units,
            room,
            turn,
            rich,
            net_food: cities::city_income(map, &city).0,
            shield_rate: cities::city_income(map, &city).1,
            threatened: danger,
            locked: crate::research::locked(civ),
        };
        let pick = choose_build(&city, &needs);
        city.queue.clear();
        let taken = cities::taken_tiles(map, cities_q.iter().map(|(_, c)| c), tile);
        let Ok((_, mut city)) = cities_q.get_mut(e) else {
            continue;
        };
        cities::balanced_governor(map, &mut city, &taken);
        // Changing between a unit and a building forfeits half the stored
        // shields, so only a city with little banked swaps classes.
        if pick != city.production && (pick.is_building() == city.production.is_building() || city.shields <= 5) {
            city.change_build(pick);
        }
    }
}

/// Play the computer's turn: decide for every unit that can still act, and
/// hand the turn over once nothing is left to do.
pub fn play_turn(
    time: Res<Time>,
    civs: Res<Civilizations>,
    turn: Res<Turn>,
    map: Res<GameMap>,
    splash: Res<SplashUp>,
    treasury: Res<cities::Treasury>,
    diplomacy: Res<crate::diplomacy::Diplomacy>,
    mut state: ResMut<AiState>,
    mut units: Query<(Entity, &mut Unit)>,
    mut cities_q: Query<(Entity, &mut City)>,
    mut found: MessageWriter<FoundCityOrder>,
    mut end: MessageWriter<TurnEnded>,
    mut commands: Commands,
) {
    let civ = civs.active;
    if !is_ai(civ) || civs.outcome.is_some() || splash.0 {
        state.civ = None;
        return;
    }
    if state.civ != Some(civ) || state.turn != turn.0 {
        state.civ = Some(civ);
        state.turn = turn.0;
        state.managed = false;
        state.done.clear();
        state.elapsed = 0.0;
        state.quiet_frames = 0;
    }
    state.elapsed += time.delta_secs();

    let board = Board::new(
        cities_q.iter().map(|(_, c)| c.clone()).collect(),
        units.iter().map(|(e, u)| (e, u.clone())).collect(),
        &map,
    )
    .with_war(diplomacy.war_matrix());
    refresh_known(&mut state, &map, &board);
    if !state.managed {
        state.managed = true;
        let room = board
            .mine(civ)
            .next()
            .map(|c| (c.x, c.y))
            .or_else(|| board.units_of(civ).next().map(|u| (u.x, u.y)))
            .and_then(|from| best_site(&map, &board, civ, from, &HashSet::new()))
            .is_some();
        manage_cities(civ, turn.0, &map, &board, treasury.0[civ], room, &mut cities_q);
        if std::env::var("CIV3_AI_LOG").is_ok() {
            let count = |t: UnitType| board.units_of(civ).filter(|u| u.utype == t).count();
            println!(
                "ai: turn {} {}: {} cities (pop {}), {} gold, settlers {} workers {} soldiers {} scouts {}, room {}",
                turn.0,
                crate::civs::CIVS[civ].name,
                board.mine(civ).count(),
                board.mine(civ).map(|c| c.size as u32).sum::<u32>(),
                treasury.0[civ],
                count(UnitType::Settler),
                count(UnitType::Worker),
                board.units_of(civ).filter(|u| def(u.utype).attack > 0).count(),
                count(UnitType::Scout),
                room,
            );
            for c in board.mine(civ) {
                let (f, sh) = cities::city_income(&map, c);
                println!(
                    "ai:   {} size {} food {}/{} ({:+}) shields {}/{} (+{}) building {:?} worked {}",
                    c.name,
                    c.size,
                    c.food,
                    economy::food_box(c.size),
                    f,
                    c.shields,
                    c.production.cost(),
                    sh,
                    c.production,
                    c.worked.len()
                );
            }
        }
    }

    // Targets of the settlers and workers already underway.
    state.targets.retain(|e, _| units.get(*e).is_ok());
    let mut claimed: HashSet<(i32, i32)> = HashSet::new();
    for (e, &t) in &state.targets {
        if units.get(*e).is_ok_and(|(_, u)| u.civ == civ) {
            claimed.insert(t);
        }
    }
    // Soldiers already holding a city.
    let mut garrisoned: HashMap<(i32, i32), usize> = HashMap::new();
    for (e, u) in board.at.values().flatten() {
        if u.civ == civ
            && def(u.utype).attack > 0
            && state.done.contains(e)
            && board.city_at((u.x, u.y)).is_some_and(|c| c.civ == civ)
        {
            *garrisoned.entry((u.x, u.y)).or_default() += 1;
        }
    }
    let soldiers = board.units_of(civ).filter(|u| def(u.utype).attack > 0).count();
    let needs: usize = board
        .mine(civ)
        .map(|c| garrison_need(c, threatened(&map, &board, civ, (c.x, c.y))))
        .sum();
    let known = state.known[civ].clone();
    let mut brain = Brain {
        map: &map,
        board: &board,
        civ,
        turn: turn.0,
        known: &known,
        surplus: soldiers.saturating_sub(needs),
        garrisoned,
        claimed,
    };

    // Only what the human can see is worth animating.
    let lit = |x: i32, y: i32| !crate::civs::ai_fast() && map.get(x, y).is_some_and(|t| t.visible);
    // An army beyond what the treasury can carry sheds its weakest soldiers.
    let broke = treasury.0[civ] < 20
        && economy::finance(&map, board.mine(civ), board.units_of(civ).count()).net() < 0;
    let mut mine: Vec<Entity> = units
        .iter()
        .filter(|(_, u)| u.civ == civ)
        .map(|(e, _)| e)
        .collect();
    let cap = army_cap(needs, board.mine(civ).count());
    if broke && soldiers > cap {
        let mut weakest: Vec<(u32, Entity)> = units
            .iter()
            .filter(|(_, u)| u.civ == civ && def(u.utype).attack > 0)
            .map(|(e, u)| ((def(u.utype).attack + def(u.utype).defense) as u32 * 100 + u.hp() as u32, e))
            .collect();
        weakest.sort();
        for &(_, e) in weakest.iter().take(soldiers - cap) {
            commands.entity(e).despawn();
            state.targets.remove(&e);
        }
        let gone: HashSet<Entity> = weakest.iter().take(soldiers - cap).map(|&(_, e)| e).collect();
        mine.retain(|e| !gone.contains(e));
    }
    let mut pending = false;
    for e in mine {
        let Ok((_, mut u)) = units.get_mut(e) else {
            continue;
        };
        if !matches!(u.anim, UnitAnim::Idle { .. }) {
            pending = true;
            continue;
        }
        if u.moves == 0 || u.work.is_some() || state.done.contains(&e) {
            continue;
        }
        if !u.path.is_empty() {
            pending = true; // walking; drive_movement moves it
            continue;
        }
        let snapshot = u.clone();
        let (act, target) = brain.think(e, &snapshot, state.targets.get(&e).copied());
        if snapshot.utype == UnitType::Settler && std::env::var("CIV3_AI_LOG").is_ok() {
            println!(
                "ai:   settler of {} at ({},{}) -> {:?} (target {:?})",
                crate::civs::CIVS[civ].name,
                snapshot.x,
                snapshot.y,
                act,
                target
            );
        }
        match target {
            Some(t) => {
                state.targets.insert(e, t);
            }
            None => {
                state.targets.remove(&e);
            }
        }
        match act {
            Act::Stay => {
                state.done.insert(e);
            }
            Act::Fortify => {
                // Settling in takes the unit's turn; one already dug in is
                // left alone.
                if !u.fortified {
                    u.fortified = true;
                    u.sentry = u.utype == UnitType::Scout;
                    u.moves = 0;
                    if lit(u.x, u.y) {
                        u.anim = UnitAnim::OneShot { slot: "FORTIFY", t: 0.0 };
                    }
                }
                u.path.clear();
                state.done.insert(e);
            }
            Act::Go(path) => {
                u.path = path.into();
                u.fortified = false;
                u.sentry = false;
                pending = true;
            }
            Act::Attack(t) => {
                u.path = VecDeque::from([t]);
                u.fortified = false;
                u.sentry = false;
                pending = true;
            }
            Act::Found => {
                found.write(FoundCityOrder(e));
                state.done.insert(e);
            }
            Act::Disband => {
                commands.entity(e).despawn();
                state.done.insert(e);
            }
            Act::Work(action) => {
                u.work = Some(Work { action, turns_left: improvements::work_turns(action) });
                u.moves = 0;
                u.path.clear();
                u.fortified = false;
                u.sentry = false;
                if lit(u.x, u.y) {
                    u.anim = UnitAnim::OneShot { slot: improvements::action_slot(action), t: 0.0 };
                }
                state.done.insert(e);
            }
        }
        // Later units see this one's claims and garrison duty.
        brain.claimed.extend(state.targets.values().copied());
    }

    if pending {
        state.quiet_frames = 0;
    } else {
        state.quiet_frames += 1;
    }
    if state.quiet_frames >= SETTLE_FRAMES || state.elapsed > TURN_BUDGET {
        state.civ = None;
        end.write(TurnEnded);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{Base, Cover};

    fn city_at(civ: usize, x: i32, y: i32, size: u8) -> City {
        City {
            civ,
            name: format!("C{x},{y}"),
            x,
            y,
            size,
            food: 0,
            shields: 0,
            production: Production::Warrior,
            queue: vec![],
            buildings: vec![],
            worked: HashSet::new(),
            culture: 0,
            founded: 1,
        }
    }

    fn needs() -> Needs {
        Needs {
            defenders_here: 1,
            garrison: 1,
            cities: 2,
            settlers: 0,
            workers: 2,
            soldiers: 3,
            soldiers_wanted: 3,
            soldiers_cap: 20,
            units: 6,
            room: true,
            turn: 5,
            rich: false,
            net_food: 2,
            shield_rate: 2,
            threatened: false,
            locked: vec![],
        }
    }

    #[test]
    fn an_empty_city_builds_a_soldier_first() {
        let city = city_at(1, 5, 5, 1);
        let mut n = needs();
        n.defenders_here = 0;
        n.soldiers = 0;
        assert_eq!(choose_build(&city, &n), Production::Warrior);
        n.soldiers = 2;
        assert_eq!(choose_build(&city, &n), Production::Spearman);
        // Without Bronze Working it settles for an Archer, then a Warrior.
        n.locked = vec![Production::Spearman];
        assert_eq!(choose_build(&city, &n), Production::Archer);
        n.locked.push(Production::Archer);
        assert_eq!(choose_build(&city, &n), Production::Warrior);
    }

    #[test]
    fn a_garrisoned_city_with_room_expands_then_works_then_arms() {
        let city = city_at(1, 5, 5, 3);
        let mut n = needs();
        assert_eq!(choose_build(&city, &n), Production::Settler);
        n.settlers = 2;
        n.workers = 0;
        assert_eq!(choose_build(&city, &n), Production::Worker);
        n.workers = 2;
        n.soldiers = 0;
        assert!(matches!(
            choose_build(&city, &n),
            Production::Archer | Production::Horseman
        ));
        // No room, no cities beyond the cap: no Settlers.
        n.settlers = 0;
        n.room = false;
        n.soldiers = 3;
        assert_ne!(choose_build(&city, &n), Production::Settler);
        n.room = true;
        n.cities = MAX_CITIES;
        assert_ne!(choose_build(&city, &n), Production::Settler);
    }

    #[test]
    fn buildings_wait_for_a_treasury_that_can_carry_them() {
        let city = city_at(1, 5, 5, 4);
        let mut n = needs();
        n.room = false;
        n.units = 20;
        n.rich = false;
        assert!(!choose_build(&city, &n).is_building());
        n.rich = true;
        assert_eq!(choose_build(&city, &n), Production::Temple);
    }

    #[test]
    fn the_site_scorer_prefers_fertile_ground_and_the_search_finds_legal_sites() {
        let map = GameMap::generate();
        let starts = crate::civs::starting_positions(&map);
        let board = Board::new(vec![], vec![], &map);
        let site = best_site(&map, &board, 0, starts[0], &HashSet::new()).expect("a site");
        assert!(cities::can_found(&map, &[], site.0, site.1));
        assert!(map.distance(starts[0], site) <= SITE_RANGE);
        assert!(map.find_path(starts[0], site).is_some());
        // Desert beats nothing, but grassland around it beats desert.
        let mut flat = map.clone();
        for t in flat.tiles.iter_mut() {
            t.base = Base::Desert;
            t.relief = Relief::Flat;
            t.cover = Cover::Bare;
            t.resource = None;
            t.irrigation = false;
            t.mine = false;
        }
        let poor = site_score(&flat, &[], 10, 10);
        for (x, y) in cities::radius_tiles(&flat, 10, 10) {
            let i = flat.idx(x, y);
            flat.tiles[i].base = Base::Grassland;
        }
        assert!(site_score(&flat, &[], 10, 10) > poor);
    }

    #[test]
    fn sites_keep_clear_of_cities_and_foreign_borders() {
        let map = GameMap::generate();
        let starts = crate::civs::starting_positions(&map);
        let mine = city_at(0, starts[0].0, starts[0].1, 1);
        let board = Board::new(vec![mine.clone()], vec![], &map);
        assert!(!site_ok(&map, &board, 0, &HashSet::new(), starts[0]));
        for dx in -2..=2 {
            let t = (map.wrap_x(starts[0].0 + dx), starts[0].1);
            if map.distance(t, starts[0]) < 3 {
                assert!(!site_ok(&map, &board, 0, &HashSet::new(), t));
            }
        }
        // The city's border keeps another civ off its first ring.
        let near = map
            .neighbors(starts[0].0, starts[0].1)
            .into_iter()
            .next()
            .unwrap();
        assert!(!site_ok(&map, &board, 1, &HashSet::new(), near));
    }

    #[test]
    fn workers_irrigate_farmland_mine_hills_and_road_the_rest() {
        let mut map = GameMap::generate();
        let (x, y) = (10, 10);
        let i = map.idx(x, y);
        map.tiles[i].base = Base::Grassland;
        map.tiles[i].relief = Relief::Flat;
        map.tiles[i].cover = Cover::Bare;
        map.tiles[i].irrigation = false;
        map.tiles[i].road = false;
        // Water beside it makes irrigation possible.
        let water = map.idx(x + 1, y);
        map.tiles[water].base = Base::Coast;
        assert_eq!(job_at(&map, &[], (x, y)), Some(WorkAction::Irrigate));
        // No water: a road instead.
        map.tiles[water].base = Base::Grassland;
        for (nx, ny) in map.neighbors(x, y) {
            let n = map.idx(nx, ny);
            map.tiles[n].base = Base::Grassland;
            map.tiles[n].irrigation = false;
        }
        assert_eq!(job_at(&map, &[], (x, y)), Some(WorkAction::Road));
        map.tiles[i].relief = Relief::Hill;
        assert_eq!(job_at(&map, &[], (x, y)), Some(WorkAction::Mine));
        // A city center is left alone.
        assert_eq!(job_at(&map, &[(x, y)], (x, y)), None);
    }

    #[test]
    fn a_warrior_does_not_pick_a_fight_with_a_walled_in_veteran() {
        let map = GameMap::generate();
        let (x, y) = map.start;
        let att = Unit::new(1, UnitType::Warrior, x, y);
        // An Archer attacking a lone Warrior is a good bet; a Warrior
        // attacking a Spearman in a city is not.
        let weak = (Entity::from_bits(1), Unit::new(0, UnitType::Warrior, x + 1, y));
        let strong = (Entity::from_bits(2), Unit::new(0, UnitType::Spearman, x + 1, y));
        let archer = Unit::new(1, UnitType::Archer, x, y);
        assert!(attack_chance(&map, &archer, &[weak.clone()], None) > 0.6);
        assert!(
            attack_chance(&map, &att, &[strong], Some(3))
                < attack_chance(&map, &att, &[weak], Some(3))
        );
        // Nothing that can fight back: the tile is simply taken.
        let worker = (Entity::from_bits(3), Unit::new(0, UnitType::Worker, x + 1, y));
        assert_eq!(attack_chance(&map, &att, &[worker], None), 1.0);
    }

    #[test]
    fn scouts_head_for_unseen_ground_until_none_is_left() {
        let map = GameMap::generate();
        let from = map.start;
        let mut known = vec![false; map.tiles.len()];
        let path = explore_path(&map, &known, from).expect("something unseen");
        assert!(!path.is_empty());
        known.iter_mut().for_each(|k| *k = true);
        let mut quiet = map.clone();
        for t in quiet.tiles.iter_mut() {
            t.hut = false;
            t.camp = false;
        }
        assert!(explore_path(&quiet, &known, from).is_none());
    }

    #[test]
    fn walks_stop_short_of_enemies() {
        let map = GameMap::generate();
        let (x, y) = map.start;
        let enemy = (Entity::from_bits(9), Unit::new(2, UnitType::Warrior, x + 2, y));
        let board = Board::new(vec![], vec![enemy], &map);
        let path = [(x + 1, y), (x + 2, y), (x + 3, y)];
        assert_eq!(safe_walk(&board, 1, &path), vec![(x + 1, y)]);
        assert_eq!(safe_walk(&board, 2, &path), path.to_vec());
    }

    fn brain<'a>(
        map: &'a GameMap,
        board: &'a Board,
        known: &'a [bool],
        civ: usize,
        turn: u32,
    ) -> Brain<'a> {
        Brain {
            map,
            board,
            civ,
            turn,
            known,
            surplus: 0,
            garrisoned: HashMap::new(),
            claimed: HashSet::new(),
        }
    }

    /// A tile with open ground all round, near the start.
    fn open_site(map: &GameMap) -> (i32, i32) {
        let starts = crate::civs::starting_positions(map);
        starts[0]
    }

    #[test]
    fn a_settler_without_a_city_founds_one_where_it_stands() {
        let map = GameMap::generate();
        let at = open_site(&map);
        let board = Board::new(vec![], vec![], &map);
        let known = vec![true; map.tiles.len()];
        let settler = Unit::new(0, UnitType::Settler, at.0, at.1);
        let mut b = brain(&map, &board, &known, 0, 1);
        assert_eq!(b.think(Entity::from_bits(1), &settler, None).0, Act::Found);
    }

    #[test]
    fn a_settler_that_reaches_its_site_founds_there_instead_of_wandering_off() {
        let map = GameMap::generate();
        let at = open_site(&map);
        // The home city sits well away; the settler has walked to a site.
        let far = best_site(
            &map,
            &Board::new(vec![city_at(0, at.0, at.1, 3)], vec![], &map),
            0,
            at,
            &HashSet::new(),
        )
        .expect("a second site");
        let board = Board::new(vec![city_at(0, at.0, at.1, 3)], vec![], &map);
        let known = vec![true; map.tiles.len()];
        let settler = Unit::new(0, UnitType::Settler, far.0, far.1);
        let mut b = brain(&map, &board, &known, 0, 5);
        // Its own plan is on file as a claim, as the turn loop leaves it.
        b.claimed.insert(far);
        let (act, target) = b.think(Entity::from_bits(1), &settler, Some(far));
        assert_eq!(act, Act::Found);
        assert_eq!(target, None);
    }

    #[test]
    fn an_empty_city_keeps_its_soldier_at_home_and_a_spare_one_hunts() {
        let map = GameMap::generate();
        let at = open_site(&map);
        let home = city_at(0, at.0, at.1, 1);
        let guard = Unit::new(0, UnitType::Warrior, at.0, at.1);
        let board = Board::new(vec![home], vec![], &map);
        let known = vec![true; map.tiles.len()];
        let mut b = brain(&map, &board, &known, 0, 5);
        assert_eq!(b.think(Entity::from_bits(1), &guard, None).0, Act::Fortify);
        // A second soldier is surplus: next to an unprotected enemy Worker
        // it takes the prize.
        let step = map
            .neighbors(at.0, at.1)
            .into_iter()
            .find(|&(x, y)| map.get(x, y).and_then(move_cost).is_some())
            .expect("an open neighbor");
        let prey = (
            Entity::from_bits(7),
            Unit::new(1, UnitType::Worker, step.0, step.1),
        );
        let board = Board::new(
            vec![city_at(0, at.0, at.1, 1)],
            vec![prey],
            &map,
        );
        let mut b = brain(&map, &board, &known, 0, 5);
        b.garrisoned.insert(at, 1);
        let spare = Unit::new(0, UnitType::Warrior, at.0, at.1);
        assert_eq!(
            b.think(Entity::from_bits(2), &spare, None).0,
            Act::Attack(step)
        );
    }

    #[test]
    fn a_soldier_leaves_a_civ_at_peace_alone() {
        let map = GameMap::generate();
        let at = open_site(&map);
        let step = map
            .neighbors(at.0, at.1)
            .into_iter()
            .find(|&(x, y)| map.get(x, y).and_then(move_cost).is_some())
            .expect("an open neighbor");
        let prey = (Entity::from_bits(7), Unit::new(1, UnitType::Worker, step.0, step.1));
        let mut peace = [[true; CIV_COUNT]; CIV_COUNT];
        peace[0][1] = false;
        peace[1][0] = false;
        let board = Board::new(vec![city_at(0, at.0, at.1, 1)], vec![prey], &map).with_war(peace);
        assert!(!board.enemy_on(step, 0));
        assert!(board.held_by_other(step, 0), "the tile is still not free to stand on");
        let known = vec![true; map.tiles.len()];
        let mut b = brain(&map, &board, &known, 0, 5);
        b.garrisoned.insert(at, 1);
        let spare = Unit::new(0, UnitType::Warrior, at.0, at.1);
        assert!(!matches!(b.think(Entity::from_bits(2), &spare, None).0, Act::Attack(_)));
    }

    #[test]
    fn a_unit_that_already_attacked_does_not_attack_again() {
        let map = GameMap::generate();
        let at = open_site(&map);
        let step = map
            .neighbors(at.0, at.1)
            .into_iter()
            .find(|&(x, y)| map.get(x, y).and_then(move_cost).is_some())
            .expect("an open neighbor");
        let prey = (
            Entity::from_bits(7),
            Unit::new(1, UnitType::Worker, step.0, step.1),
        );
        let board = Board::new(vec![], vec![prey], &map);
        let known = vec![true; map.tiles.len()];
        let mut b = brain(&map, &board, &known, 0, 5);
        let mut spent = Unit::new(0, UnitType::Warrior, at.0, at.1);
        spent.attacked = true;
        assert!(!matches!(
            b.think(Entity::from_bits(2), &spent, None).0,
            Act::Attack(_)
        ));
    }

    #[test]
    fn a_scout_with_nothing_left_to_see_is_disbanded() {
        let map = GameMap::generate();
        let at = open_site(&map);
        let mut quiet = map.clone();
        for t in quiet.tiles.iter_mut() {
            t.hut = false;
            t.camp = false;
        }
        let board = Board::new(vec![], vec![], &quiet);
        let known = vec![true; quiet.tiles.len()];
        let scout = Unit::new(0, UnitType::Scout, at.0, at.1);
        let mut b = brain(&quiet, &board, &known, 0, 5);
        assert_eq!(b.think(Entity::from_bits(1), &scout, None).0, Act::Disband);
    }
}
