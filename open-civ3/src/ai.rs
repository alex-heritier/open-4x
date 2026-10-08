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
use crate::citycalc;
use crate::civs::{CIV_CAP, Civilizations, civ_count, is_ai};
use crate::combat;
use crate::economy;
use crate::improvements::{self, Work, WorkAction};
use crate::map::{GameMap, Relief, Tile, move_cost};
use crate::roles;
use crate::roster;
use crate::splash::SplashUp;
use crate::units::{Turn, TurnEnded, Unit, UnitAnim, UnitType, def};
mod naval;

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
    war: [[bool; CIV_CAP]; CIV_CAP],
}

impl Board {
    pub fn new(cities: Vec<City>, units: Vec<(Entity, Unit)>, map: &GameMap) -> Self {
        let owner = cities::territory(map);
        let mut at: HashMap<(i32, i32), Vec<(Entity, Unit)>> = HashMap::new();
        for (e, u) in units {
            at.entry((u.x, u.y)).or_default().push((e, u));
        }
        Board {
            cities,
            at,
            owner,
            war: [[true; CIV_CAP]; CIV_CAP],
        }
    }

    /// The wars of the moment.
    pub fn with_war(mut self, war: [[bool; CIV_CAP]; CIV_CAP]) -> Self {
        self.war = war;
        self
    }

    /// `a` is at war with `b`.
    fn at_war(&self, a: usize, b: usize) -> bool {
        if a >= civ_count() || b >= civ_count() {
            return a != b;
        }
        a != b && self.war[a][b]
    }

    fn city_at(&self, tile: (i32, i32)) -> Option<&City> {
        self.cities.iter().find(|c| (c.x, c.y) == tile)
    }

    /// Units standing on `tile` that belong to a civ at war with `civ`.
    fn foreign_at(&self, tile: (i32, i32), civ: usize) -> Vec<(Entity, Unit)> {
        self.at
            .get(&tile)
            .map(|v| {
                v.iter()
                    .filter(|(_, u)| self.at_war(civ, u.civ))
                    .cloned()
                    .collect()
            })
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

    /// Civilian routes obey the mover's foreign-occupancy gate, including
    /// peaceful units. Search around blockers instead of stopping before them.
    fn civilian_path(
        &self,
        map: &GameMap,
        civ: usize,
        from: (i32, i32),
        to: (i32, i32),
    ) -> Option<Vec<(i32, i32)>> {
        map.find_path_by(from, to, |a, b| {
            if self.held_by_other(b, civ) {
                return None;
            }
            map.land_cost(a, b, crate::units::bridges(civ))
        })
    }

    fn mine(&self, civ: usize) -> impl Iterator<Item = &City> {
        self.cities.iter().filter(move |c| c.civ == civ)
    }

    fn units_of(&self, civ: usize) -> impl Iterator<Item = &Unit> {
        self.at
            .values()
            .flatten()
            .map(|(_, u)| u)
            .filter(move |u| u.civ == civ)
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
    if map.neighbors(x, y).iter().any(|&(nx, ny)| {
        map.get(nx, ny)
            .is_some_and(|t| improvements::is_water_base(t.base))
    }) {
        score += 4;
    }
    score
}

/// Whether the computer may settle `(x, y)`: legal, three tiles clear of
/// every city, not inside another civilization's border, and with no enemy
/// standing on it.
fn site_ok(
    map: &GameMap,
    board: &Board,
    civ: usize,
    claimed: &HashSet<(i32, i32)>,
    t: (i32, i32),
) -> bool {
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
                scored.push((
                    site_score(map, &spots, t.0, t.1) - 2 * map.distance(from, t),
                    t,
                ));
            }
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    // A higher-scoring island must not hide a reachable mainland site.
    scored
        .into_iter()
        .find(|&(_, t)| board.civilian_path(map, civ, from, t).is_some())
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
    pub transports: usize,
    pub overseas_room: bool,
    /// Enemy cities lie across the water, out of a land army's reach.
    pub overseas_war: bool,
    /// Workers alive or being built in the civ's other cities.
    pub workers: usize,
    /// Soldiers alive.
    pub soldiers: usize,
    pub artillery: usize,
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
    /// Freshwater at the city tile replaces its Aqueduct requirement.
    pub fresh_water: bool,
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

    /// The land unit of the roster the civ can build that is best at a
    /// role (a `PRTO` AI strategy bit: 0x1 offense, 0x2 defense), by
    /// `power` per shield, then raw power. What the computer may build is
    /// already pruned of obsolete types (`Needs::locked`), so this finds
    /// the newest of each line; `fast` asks for two or more moves.
    fn best(
        &self,
        role: u32,
        fast: Option<bool>,
        power: impl Fn(&roster::UnitRow) -> i32,
    ) -> Option<Production> {
        Production::all()
            .filter_map(|p| Some((p, p.unit()?.row())))
            .filter(|(p, r)| {
                r.playable
                    && r.class == 0
                    && r.ai & role != 0
                    && r.pop_cost == 0
                    && power(r) > 0
                    && fast.is_none_or(|f| (r.moves >= 2) == f)
                    && self.can(*p)
            })
            .max_by_key(|(_, r)| (power(r) * 100 / r.cost.max(1), power(r), -r.cost))
            .map(|(p, _)| p)
    }

    /// The best defender the civ has the advance for.
    fn defender(&self) -> Production {
        self.best(DEFENSE, None, |r| r.defense)
            // Failing a defender, whatever stands its ground best per shield.
            .or_else(|| self.best(u32::MAX, None, |r| r.defense))
            .unwrap_or_else(roles::guard_production)
    }

    /// The cheapest ship that carries land units, which the civ can build in `city`.
    fn ferry(&self, city: &City) -> Option<Production> {
        Production::all()
            .filter(|&p| p.unit().is_some_and(is_ferry) && self.can(p) && city.can_build_here(p))
            .min_by_key(|p| (p.cost(), p.index()))
    }
}

/// A ship that carries land units.
fn is_ferry(t: UnitType) -> bool {
    let r = t.row();
    r.class == 1 && r.capacity > 0 && r.pop_cost == 0
}

/// `PRTO` AI strategy bits (`ai.md`): what a unit type is for.
const OFFENSE: u32 = 0x1;
const DEFENSE: u32 = 0x2;

/// Choose a city's build. Order of concern: a garrison, expansion, workers,
/// an army, then buildings.
pub fn choose_build(city: &City, n: &Needs) -> Production {
    // 1. Nobody home: the quickest soldier available.
    if n.defenders_here == 0 {
        return if n.soldiers == 0 {
            roles::guard_production()
        } else {
            n.defender()
        };
    }
    if n.defenders_here < n.garrison && n.threatened {
        return n.defender();
    }
    // One ferry while overseas expansion needs it. This is the clone's
    // settlement policy, not a recovered Civ3 production weight.
    if n.overseas_room
        && n.cities < MAX_CITIES
        && n.transports == 0
        && let Some(ferry) = n.ferry(city)
    {
        return ferry;
    }
    // A war across the water needs a ferry and a band to fill it.
    if n.overseas_war
        && n.transports == 0
        && n.soldiers >= n.soldiers_wanted.saturating_sub(2)
        && let Some(ferry) = n.ferry(city)
    {
        return ferry;
    }
    // 2. Expansion, while there is room and the city can spare the people.
    if (n.room || (n.overseas_room && n.transports > 0))
        && n.cities < MAX_CITIES
        && n.settlers < 2
        && settler_ready(city, n)
    {
        return roles::settler_production();
    }
    // 3. Workers, about one per city.
    if n.workers < n.cities.min(3) {
        return roles::worker_production();
    }
    // 4. Soldiers up to the wanted count, defenders first.
    if n.soldiers < n.soldiers_wanted {
        if n.defenders_here < n.garrison {
            return n.defender();
        }
        return attacker(city, n);
    }
    // HYPOTHESIS: one artillery unit per four soldiers, after meeting the
    // garrison and expansion needs. The exact AI ratio is unrecovered.
    if n.artillery < n.soldiers / 4
        && let Some(p) = n.best(0x4, None, |r| r.bombard * r.rof)
        && p.unit().is_some_and(crate::bombard::capable)
    {
        return p;
    }
    // 5. Buildings, when the treasury can carry their upkeep.
    if n.rich
        && let Some(b) = building(city, n)
    {
        return b;
    }
    // 6. More army, within what the cities support for free.
    if (n.units < n.cities * 4 + 2 && n.soldiers < n.soldiers_cap) || n.threatened {
        return attacker(city, n);
    }
    // Nothing worth adding: a hand, else coin for the treasury (Wealth
    // turns the shields into gold, which pays for buildings later), never
    // an endless army.
    if n.workers < n.cities * 2 {
        roles::worker_production()
    } else if let Some(wealth) = roles::wealth().filter(|&w| city.can_build_here(w)) {
        wealth
    } else {
        roles::guard_production()
    }
}

/// A city can finish a Settler soon: it has the people, or is about to
/// (a Settler held for size 3 would idle the shield box).
fn settler_ready(city: &City, n: &Needs) -> bool {
    if city.size() >= cities::SETTLER_MIN_SIZE {
        return true;
    }
    // Otherwise start it when the shield box will fill about as the city
    // reaches size 3, so neither waits long for the other.
    if n.net_food <= 0 {
        return false;
    }
    let mut food_needed = economy::food_box(city.size()) as i32 - city.food as i32;
    for size in city.size() + 1..cities::SETTLER_MIN_SIZE {
        food_needed += economy::food_box(size) as i32;
    }
    let growth = (food_needed + n.net_food as i32 - 1) / n.net_food as i32;
    let left = city.price(roles::settler_production()) as i32 - city.shields as i32;
    let build = (left.max(0) + n.shield_rate.max(1) as i32 - 1) / n.shield_rate.max(1) as i32;
    growth <= build + 4
}

/// Archers hit hard, Horsemen run: alternate by how old the city is.
fn attacker(city: &City, n: &Needs) -> Production {
    let fast = (city.founded + n.soldiers as u32) % 3 == 0;
    n.best(OFFENSE, Some(fast), |r| r.attack)
        .or_else(|| n.best(OFFENSE, Some(!fast), |r| r.attack))
        .unwrap_or_else(roles::guard_production)
}

/// The improvement worth its upkeep in a city with money to spare, if any:
/// first what lifts a size limit the city has hit, then the contentment,
/// science and trade buildings a growing city wants, the barracks of a
/// civ going to war, and walls under threat.
fn building(city: &City, n: &Needs) -> Option<Production> {
    let want = |p: Production| n.can(p) && !city.has(p) && city.can_build_here(p);
    if citycalc::growth_blocked(city, n.fresh_water) {
        let flag = if citycalc::size_limit(city, n.fresh_water) == civ3_rules::economy::TOWN_MAX {
            roster::imp::ALLOWS_SIZE_LEVEL_2
        } else {
            roster::imp::ALLOWS_SIZE_LEVEL_3
        };
        let lifts =
            Production::all().find(|&p| p.bldg().is_some_and(|b| b.flags & flag != 0) && want(p));
        if lifts.is_some() {
            return lifts;
        }
    }
    let wants = [
        (roles::temple(), city.size() >= 3),
        (roles::granary(), city.size() >= 4),
        (roles::barracks(), n.turn >= AGGRESSION_TURN),
        (roles::walls(), n.threatened && city.size() >= 3),
        (roles::library(), city.size() >= 4),
        (roles::marketplace(), city.size() >= 5),
        (
            roles::harbor(),
            city.coastal && city.size() >= 3 && n.net_food <= 2,
        ),
        (roles::courthouse(), city.size() >= 6),
    ];
    if let Some(p) = wants
        .into_iter()
        .find_map(|(p, ok)| p.filter(|&p| ok && want(p)))
    {
        return Some(p);
    }
    // HYPOTHESIS (the item chooser `0x42C8A0` is open, `ai.md` 4): a
    // second seat of government in a sizable city far from the capital,
    // then great wonders in grown cities, each city on its own wonder so
    // that the civ does not race itself.
    let capital = crate::realm::read(city.civ, |r| r.capital);
    let far = capital.is_some_and(|c| {
        crate::citycalc::native_distance_on(crate::scenario::map_dims().0, c, (city.x, city.y)) >= 8
    });
    if far
        && city.size() >= 4
        && let Some(seat) = roles::forbidden_palace().filter(|&p| want(p))
    {
        return Some(seat);
    }
    if city.size() >= 5 {
        let wonders: Vec<Production> = Production::all()
            .filter(|&p| p.bldg().is_some_and(|b| b.is_great_wonder()) && want(p))
            .collect();
        if !wonders.is_empty() {
            return Some(wonders[city.founded as usize % wonders.len()]);
        }
    }
    None
}

/// The most soldiers a civ with these garrisons keeps; the rest are upkeep.
pub fn army_cap(garrisons: usize, cities: usize) -> usize {
    garrisons + 4 * cities + 4
}

/// A soldier riding a ship, ready to land.
fn land_soldier_aboard(u: &Unit) -> bool {
    u.carrier.is_some() && def(u.utype).class == 0 && def(u.utype).attack > 0
}

fn land_soldier(u: &Unit) -> bool {
    u.carrier.is_none() && def(u.utype).class == 0 && def(u.utype).attack > 0
}

/// Soldiers a city wants for its own defense.
pub fn garrison_need(city: &City, threatened: bool) -> usize {
    1 + usize::from(city.size() >= 4) + usize::from(threatened)
}

/// An enemy soldier within `THREAT_RANGE` of `tile`.
fn threatened(map: &GameMap, board: &Board, civ: usize, tile: (i32, i32)) -> bool {
    board.at.values().flatten().any(|(_, u)| {
        board.at_war(civ, u.civ)
            && def(u.utype).attack > 0
            && map.distance((u.x, u.y), tile) <= THREAT_RANGE
    })
}

// ---------------------------------------------------------------------------
// Orders for units
// ---------------------------------------------------------------------------

/// What an automated worker of `civ` does next, by the computer's own
/// worker brain: `claimed` holds the tiles other workers are headed for
/// (not this unit's own target), `target` is the tile this one was headed
/// for. Returns the order and the tile it now means to work.
pub fn worker_act(
    map: &GameMap,
    board: &Board,
    u: &Unit,
    target: Option<(i32, i32)>,
    claimed: &HashSet<(i32, i32)>,
) -> (Act, Option<(i32, i32)>) {
    let mut brain = Brain {
        map,
        board,
        civ: u.civ,
        turn: 0,
        known: &[],
        surplus: 0,
        garrisoned: HashMap::new(),
        claimed: claimed.clone(),
    };
    brain.worker(u, target)
}

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
    Bombard((i32, i32)),
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

/// The job a worker should do: irrigate farmland, otherwise mine eligible
/// terrain, then build roads. Existing irrigation and mines are preserved.
pub fn job_at(map: &GameMap, cities: &[(i32, i32)], tile: (i32, i32)) -> Option<WorkAction> {
    let t = map.get(tile.0, tile.1)?;
    if cities.contains(&tile) || t.hut || t.camp {
        return None;
    }
    // A luxury or strategic resource counts only on the road network
    // (`trade-network.md` 7): road it first.
    if t.resource.is_some_and(|id| {
        crate::features::GOODS[id as usize].kind != crate::features::GoodKind::Bonus
    }) && improvements::can_road(map, tile.0, tile.1)
    {
        return Some(WorkAction::Road);
    }
    if improvements::can_irrigate(map, cities, tile.0, tile.1) && !t.mine {
        Some(WorkAction::Irrigate)
    } else if improvements::can_mine(map, tile.0, tile.1) && !t.irrigation {
        Some(WorkAction::Mine)
    } else if improvements::can_road(map, tile.0, tile.1) {
        Some(WorkAction::Road)
    } else {
        None
    }
}

/// Chance the attacker wins a fight against whatever defends `at`, or 1.0
/// when nothing there can fight back (it is taken instead).
pub fn attack_chance(
    map: &GameMap,
    att: &Unit,
    stack: &[(Entity, Unit)],
    hold: Option<combat::Hold>,
) -> f64 {
    let Some(d) = combat::pick_defender(map, att, hold, stack) else {
        return 1.0;
    };
    let dfn = &stack
        .iter()
        .find(|(e, _)| *e == d)
        .expect("picked from the stack")
        .1;
    let odds = combat::round_odds(map, att, dfn, hold);
    let win = |hp| 1.0 - combat::defender_win_chance(odds, hp, dfn.hp());
    if let Some(s) = combat::supporting_shooter(att, d, stack) {
        let shooter = &stack.iter().find(|(e, _)| *e == s).unwrap().1;
        let miss = f64::from(combat::support_odds(shooter, att)) / 1024.0;
        miss * win(att.hp()) + (1.0 - miss) * win(att.hp() - 1)
    } else {
        win(att.hp())
    }
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
        // Artillery also uses this to approach an occupied target; safe_walk
        // stops it before the final tile. Intermediate blockers need a detour.
        match self.map.find_path_by((u.x, u.y), goal, |a, b| {
            if b != goal && self.board.held_by_other(b, self.civ) {
                return None;
            }
            crate::units::entry_cost(self.map, u, a, b, &[])
        }) {
            Some(p) => {
                let walk = safe_walk(self.board, self.civ, &p);
                if walk.is_empty() {
                    Act::Stay
                } else {
                    Act::Go(walk)
                }
            }
            None => Act::Stay,
        }
    }

    fn think(
        &mut self,
        e: Entity,
        u: &Unit,
        target: Option<(i32, i32)>,
    ) -> (Act, Option<(i32, i32)>) {
        // A unit's own plan does not stand in its way.
        if let Some(t) = target {
            self.claimed.remove(&t);
        }
        match u.utype {
            _ if def(u.utype).class == 1 => (Act::Stay, None),
            t if roles::founds_cities(t) => self.settler(u, target),
            t if roles::is_worker(t) => self.worker(u, target),
            t if t == roles::scout() => (self.scout(u), None),
            t if crate::bombard::capable(t) => (self.artillery(u), None),
            _ => (self.soldier(e, u), None),
        }
    }

    fn artillery(&self, u: &Unit) -> Act {
        if u.attacked {
            return Act::Stay;
        }
        let here = (u.x, u.y);
        let visible = |p: (i32, i32)| self.known[self.map.idx(p.0, p.1)];
        let enemies = self
            .board
            .at
            .iter()
            .filter(|(p, stack)| {
                visible(**p)
                    && stack.iter().any(|(_, enemy)| {
                        self.board.at_war(u.civ, enemy.civ)
                            && def(enemy.utype).defense > 0
                            && enemy.hp() > 1
                    })
            })
            .map(|(&p, _)| p);
        let cities = self
            .board
            .cities
            .iter()
            .filter(|c| self.board.at_war(u.civ, c.civ) && visible((c.x, c.y)))
            .map(|c| (c.x, c.y));
        // HYPOTHESIS: after combat targets, use an in-range enemy improvement.
        let improvements = self
            .board
            .owner
            .iter()
            .filter(|(p, owner)| {
                visible(**p)
                    && self.board.at_war(u.civ, **owner)
                    && self.map.distance(here, **p) > 0
                    && self.map.distance(here, **p) <= def(u.utype).bomb_range
                    && crate::bombard::improved(self.map.get(p.0, p.1).unwrap())
                    && self.board.at.get(*p).is_none_or(|stack| {
                        stack
                            .iter()
                            .all(|(_, unit)| self.board.at_war(u.civ, unit.civ))
                    })
            })
            .map(|(&p, _)| p);
        let target = enemies
            .chain(cities)
            .min_by_key(|&p| self.map.distance(here, p));
        let at = target
            .filter(|&p| self.map.distance(here, p) <= def(u.utype).bomb_range)
            .or_else(|| improvements.min_by_key(|&p| (self.map.distance(here, p), p)))
            .or(target);
        let Some(at) = at else { return Act::Stay };
        if self.map.distance(here, at) <= def(u.utype).bomb_range {
            Act::Bombard(at)
        } else {
            self.walk_to(u, at)
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
                .filter(|&t| {
                    site_ok(self.map, self.board, self.civ, &others, t)
                        && self
                            .board
                            .civilian_path(self.map, self.civ, here, t)
                            .is_some()
                })
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
        let keep = target.filter(|&t| {
            job(t).is_some()
                && !self.claimed.contains(&t)
                && self
                    .board
                    .civilian_path(self.map, self.civ, here, t)
                    .is_some()
        });
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
                    let penalty = if c.worked(self.map).contains(&t) {
                        0
                    } else {
                        6
                    };
                    options.push((self.map.distance(here, t) + penalty, t));
                }
            }
            options.sort();
            options.into_iter().map(|(_, t)| t).find(|&t| {
                self.board
                    .civilian_path(self.map, self.civ, here, t)
                    .is_some()
            })
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
                if walk == Act::Stay {
                    Act::Fortify
                } else {
                    walk
                }
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
                || self
                    .board
                    .mine(self.civ)
                    .any(|c| self.map.distance((c.x, c.y), t) <= THREAT_RANGE)
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
            let city = self
                .board
                .city_at(t)
                .filter(|c| self.board.at_war(self.civ, c.civ));
            let stack = self.board.foreign_at(t, self.civ);
            // Only a city whose owner differs is a city to take; a tile held
            // only by a city and no units is empty and falls.
            let chance = attack_chance(self.map, u, &stack, city.map(combat::Hold::of));
            let value = match (city, stack.iter().any(|(_, s)| def(s.utype).defense > 0)) {
                (Some(c), _) => 4.0 + 0.3 * c.size() as f64,
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
            let Some(path) = crate::units::route(self.map, u, t, &[]) else {
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
            let path = crate::units::route(self.map, u, (goal.x, goal.y), &[])?;
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
    if state.known.len() != civ_count() {
        state.known = vec![vec![]; CIV_CAP];
    }
    for civ in 0..civ_count() {
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
            let hill = matches!(
                map.get(u.x, u.y).map(|t| t.relief),
                Some(Relief::Hill | Relief::Mountain)
            );
            light(u.x, u.y, def(u.utype).sight as i32 + i32::from(hill));
        }
        for c in board.mine(civ) {
            light(c.x, c.y, 2);
        }
        for (i, t) in map.tiles.iter().enumerate() {
            if t.site.is_some_and(|site| crate::sites::owner(site) == civ) {
                light(i as i32 % map.w, i as i32 / map.w, crate::sites::sight(t));
            }
        }
    }
}

/// Pick every city's build for the turn.
fn manage_cities(
    civ: usize,
    turn: u32,
    map: &GameMap,
    board: &Board,
    gold: &mut u32,
    room: bool,
    cities_q: &mut Query<(Entity, &mut City)>,
    rng: &mut crate::rng::MapRng,
) {
    let reserve = 50 + 10 * board.mine(civ).count() as u32;
    let mine: Vec<&City> = board.mine(civ).collect();
    let soldiers = board.units_of(civ).filter(|u| land_soldier(u)).count();
    let units = board.units_of(civ).count();
    let net = economy::finance(map, mine.iter().copied(), units).net();
    let rich = net >= 2 && *gold >= 10;
    let wanted: usize = mine
        .iter()
        .map(|c| garrison_need(c, threatened(map, board, civ, (c.x, c.y))))
        .sum::<usize>()
        + 2
        + mine.len();
    let count = |t: UnitType| board.units_of(civ).filter(|u| u.utype == t).count();
    let building = |p: Production, except: (i32, i32)| {
        mine.iter()
            .filter(|c| c.production == p && (c.x, c.y) != except)
            .count()
    };
    let entities: Vec<(Entity, (i32, i32))> = cities_q
        .iter()
        .filter(|(_, c)| c.civ == civ)
        .map(|(e, c)| (e, (c.x, c.y)))
        .collect();
    for (e, tile) in entities {
        let transports = board.units_of(civ).filter(|u| is_ferry(u.utype)).count()
            + cities_q
                .iter()
                .filter(|(_, c)| {
                    c.civ == civ && (c.x, c.y) != tile && c.production.unit().is_some_and(is_ferry)
                })
                .count();
        let Ok((_, mut city)) = cities_q.get_mut(e) else {
            continue;
        };
        let here = board.at.get(&tile).map(|v| v.as_slice()).unwrap_or(&[]);
        let danger = threatened(map, board, civ, tile);
        let needs = Needs {
            defenders_here: here
                .iter()
                .filter(|(_, u)| u.civ == civ && land_soldier(u))
                .count(),
            garrison: garrison_need(&city, danger),
            cities: mine.len(),
            settlers: count(roles::settler()) + building(roles::settler_production(), tile),
            transports,
            overseas_room: city.coastal && naval::room(map, board, civ, tile),
            overseas_war: city.coastal
                && turn >= AGGRESSION_TURN
                && naval::sea_war(map, board, civ, tile),
            workers: count(roles::worker()) + building(roles::worker_production(), tile),
            soldiers,
            artillery: board
                .units_of(civ)
                .filter(|u| crate::bombard::capable(u.utype))
                .count()
                + mine
                    .iter()
                    .filter(|c| {
                        (c.x, c.y) != tile
                            && c.production.unit().is_some_and(crate::bombard::capable)
                    })
                    .count(),
            soldiers_wanted: wanted,
            soldiers_cap: army_cap(wanted - 2 - mine.len(), mine.len()),
            units,
            room,
            turn,
            rich,
            net_food: cities::city_income(map, &city).0,
            fresh_water: map.fresh_water(city.x, city.y),
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
        if pick != city.production
            && (pick.is_building() == city.production.is_building() || city.shields <= 5)
        {
            city.change_build(pick);
        }
        ai_hurry(
            map,
            &mut city,
            &taken,
            danger,
            needs.defenders_here < needs.garrison,
            reserve,
            gold,
            rng,
        );
    }
}

/// The computer's purchases. HYPOTHESIS: the native purchase logic
/// (`0x433CD0`, `hurry.md` 10) is not decoded. Here a threatened city
/// short of defenders buys (or whips) the soldier it is building with
/// purchase category 1, and gold above a reserve of 50 plus 10 a city
/// buys improvements and settlers at category 0, through the executable's
/// own prices and discount (`0x4B50A0`).
pub fn ai_hurry(
    map: &GameMap,
    city: &mut City,
    taken: &HashSet<(i32, i32)>,
    danger: bool,
    short: bool,
    reserve: u32,
    gold: &mut u32,
    rng: &mut crate::rng::MapRng,
) {
    use crate::hurry::{self, Buyer, Offer};
    let how = crate::realm::govt(city.civ).hurry;
    let soldier = city
        .production
        .unit()
        .is_some_and(|u| u.row().defense > 0 && u.row().attack + u.row().defense > 1);
    let urgent = danger && short && soldier;
    let t = if urgent { 1 } else { 0 };
    let Ok(offer) = hurry::quote(city, how, *gold, Buyer::Ai(t)) else {
        return;
    };
    let want = match offer {
        Offer::Gold(price) => {
            urgent
                || (*gold >= reserve + price
                    && city.production.unit().is_none_or(roles::founds_cities))
        }
        Offer::People(n) => urgent || (city.size() >= 6 && n == 1 && city.production.is_building()),
    };
    if want {
        hurry::apply(map, city, taken, offer, gold, rng);
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
    mut treasury: ResMut<cities::Treasury>,
    diplomacy: Res<crate::diplomacy::Diplomacy>,
    mut state: ResMut<AiState>,
    mut units: Query<(Entity, &mut Unit)>,
    mut cities_q: Query<(Entity, &mut City)>,
    mut found: MessageWriter<FoundCityOrder>,
    mut end: MessageWriter<TurnEnded>,
    mut bombard: MessageWriter<crate::bombard::Order>,
    mut commands: Commands,
    mut rng: ResMut<crate::combat::CombatRng>,
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
    for (e, path) in naval::orders(&map, &board, civ, turn.0, &state.targets) {
        if let Ok((_, mut u)) = units.get_mut(e) {
            u.path = path.into();
            u.fortified = false;
            u.sentry = false;
            state.done.remove(&e);
        }
    }
    if !state.managed {
        state.managed = true;
        let room = board
            .mine(civ)
            .next()
            .map(|c| (c.x, c.y))
            .or_else(|| board.units_of(civ).next().map(|u| (u.x, u.y)))
            .and_then(|from| best_site(&map, &board, civ, from, &HashSet::new()))
            .is_some();
        manage_cities(
            civ,
            turn.0,
            &map,
            &board,
            &mut treasury.0[civ],
            room,
            &mut cities_q,
            &mut rng.0,
        );
        if std::env::var("CIV3_AI_LOG").is_ok() {
            let count = |t: UnitType| board.units_of(civ).filter(|u| u.utype == t).count();
            println!(
                "ai: turn {} {}: {} cities (pop {}), {} gold, settlers {} workers {} soldiers {} scouts {}, room {}",
                turn.0,
                crate::civs::CIVS[civ].name,
                board.mine(civ).count(),
                board.mine(civ).map(|c| c.size() as u32).sum::<u32>(),
                treasury.0[civ],
                count(roles::settler()),
                count(roles::worker()),
                board.units_of(civ).filter(|u| land_soldier(u)).count(),
                count(roles::scout()),
                room,
            );
            for c in board.mine(civ) {
                let (f, sh) = cities::city_income(&map, c);
                println!(
                    "ai:   {} size {} food {}/{} ({:+}) shields {}/{} (+{}) building {:?} worked {}",
                    c.name,
                    c.size(),
                    c.food,
                    economy::food_box(c.size()),
                    f,
                    c.shields,
                    c.price(c.production),
                    sh,
                    c.production,
                    c.worked(&map).len()
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
            && land_soldier(u)
            && state.done.contains(e)
            && board.city_at((u.x, u.y)).is_some_and(|c| c.civ == civ)
        {
            *garrisoned.entry((u.x, u.y)).or_default() += 1;
        }
    }
    let soldiers = board.units_of(civ).filter(|u| land_soldier(u)).count();
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
            .filter(|(_, u)| u.civ == civ && land_soldier(u))
            .map(|(e, u)| {
                (
                    (def(u.utype).attack + def(u.utype).defense) as u32 * 100 + u.hp() as u32,
                    e,
                )
            })
            .collect();
        weakest.sort();
        for &(_, e) in weakest.iter().take(soldiers - cap) {
            commands.entity(e).despawn();
            state.targets.remove(&e);
        }
        let gone: HashSet<Entity> = weakest
            .iter()
            .take(soldiers - cap)
            .map(|&(_, e)| e)
            .collect();
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
        if u.carrier.is_some() {
            pending |= !u.path.is_empty();
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
        if roles::founds_cities(snapshot.utype) && std::env::var("CIV3_AI_LOG").is_ok() {
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
                    u.sentry = u.utype == roles::scout();
                    u.moves = 0;
                    if lit(u.x, u.y) {
                        u.anim = UnitAnim::OneShot {
                            slot: "FORTIFY",
                            t: 0.0,
                        };
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
            Act::Bombard(to) => {
                bombard.write(crate::bombard::Order { attacker: e, to });
                state.done.insert(e);
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
                u.work = Some(Work {
                    action,
                    progress: 0,
                });
                u.moves = 0;
                u.path.clear();
                u.fortified = false;
                u.sentry = false;
                if lit(u.x, u.y) {
                    u.anim = UnitAnim::OneShot {
                        slot: improvements::action_slot(action),
                        t: 0.0,
                    };
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
            gifts: vec![],
            goods: 0,
            coastal: false,
            river: false,
            unrest: 0,
            hurry_timer: 0,
            stakes: Default::default(),
            cooldown: 0,
            unit_clocks: Vec::new(),
            civ,
            name: format!("C{x},{y}"),
            x,
            y,
            diseased: false,
            citizens: crate::citizens::new_pool(civ, size),
            food: 0,
            shields: 0,
            production: Production::named("Warrior"),
            queue: vec![],
            buildings: vec![],
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
            transports: 0,
            overseas_room: false,
            overseas_war: false,
            workers: 2,
            soldiers: 3,
            artillery: 0,
            soldiers_wanted: 3,
            soldiers_cap: 20,
            units: 6,
            room: true,
            turn: 5,
            rich: false,
            net_food: 2,
            fresh_water: false,
            shield_rate: 2,
            threatened: false,
            locked: bronze_age(),
        }
    }

    /// Everything but the first units and a few buildings: what a civ with
    /// the earliest advances cannot build.
    fn bronze_age() -> Vec<Production> {
        let known = [
            Production::named("Settler"),
            Production::named("Worker"),
            Production::named("Scout"),
            Production::named("Warrior"),
            Production::named("Archer"),
            Production::named("Horseman"),
            Production::named("Spearman"),
            Production::named("Temple"),
            Production::named("Granary"),
            Production::named("Barracks"),
            Production::named("Library"),
            Production::named("Marketplace"),
            Production::named("Walls"),
            Production::named("Harbor"),
            Production::named("Courthouse"),
            Production::named("Aqueduct"),
        ];
        Production::all().filter(|p| !known.contains(p)).collect()
    }

    #[test]
    fn an_established_army_adds_artillery_without_building_it_forever() {
        let city = city_at(1, 5, 5, 1);
        let mut n = needs();
        n.room = false;
        n.soldiers = 8;
        n.units = 12;
        n.locked.retain(|&p| p != Production::named("Catapult"));
        assert_eq!(choose_build(&city, &n), Production::named("Catapult"));
        n.artillery = 2;
        assert_ne!(choose_build(&city, &n), Production::named("Catapult"));
    }

    #[test]
    fn an_empty_city_builds_a_soldier_first() {
        let city = city_at(1, 5, 5, 1);
        let mut n = needs();
        n.defenders_here = 0;
        n.soldiers = 0;
        assert_eq!(choose_build(&city, &n), Production::named("Warrior"));
        n.soldiers = 2;
        assert_eq!(choose_build(&city, &n), Production::named("Spearman"));
        // Without Bronze Working it settles for the cheapest thing that
        // stands its ground.
        n.locked.push(Production::named("Spearman"));
        assert_eq!(choose_build(&city, &n), Production::named("Warrior"));
        // With Iron Working's Pikemen in place of the Spearman, they win.
        n.locked.retain(|&p| p != Production::named("Pikeman"));
        n.locked.push(Production::named("Spearman"));
        assert_eq!(choose_build(&city, &n), Production::named("Pikeman"));
    }

    #[test]
    fn a_later_age_builds_its_own_units() {
        let city = city_at(1, 5, 5, 3);
        let mut n = needs();
        let only = [
            Production::named("Settler"),
            Production::named("Worker"),
            Production::named("Pikeman"),
            Production::named("Knight"),
            Production::named("Longbowman"),
        ];
        n.locked = Production::all().filter(|p| !only.contains(p)).collect();
        n.defenders_here = 0;
        n.soldiers = 2;
        assert_eq!(choose_build(&city, &n), Production::named("Pikeman"));
        // The army alternates between the fast and the slow hitter.
        n.defenders_here = 1;
        n.settlers = 2;
        n.workers = 3;
        n.soldiers = 0;
        n.soldiers_wanted = 3;
        let picks: HashSet<Production> = (0..3)
            .map(|k| {
                n.soldiers = k;
                choose_build(&city, &n)
            })
            .collect();
        assert_eq!(
            picks,
            HashSet::from([Production::named("Knight"), Production::named("Longbowman")])
        );
    }

    #[test]
    fn a_rich_city_at_its_size_limit_builds_what_lifts_it() {
        let mut city = city_at(1, 5, 5, 6);
        let mut n = needs();
        n.room = false;
        n.units = 20;
        n.rich = true;
        // A temple would do, but the Aqueduct comes first for a full town.
        assert_eq!(choose_build(&city, &n), Production::named("Aqueduct"));
        city.buildings.push(Production::named("Aqueduct"));
        assert_ne!(choose_build(&city, &n), Production::named("Aqueduct"));
        city.buildings.clear();
        n.fresh_water = true;
        assert_ne!(choose_build(&city, &n), Production::named("Aqueduct"));
        city.set_size(12);
        n.locked.clear();
        assert_eq!(choose_build(&city, &n), Production::named("Hospital"));
    }

    #[test]
    fn coastal_expansion_builds_one_unlocked_ferry_before_overseas_settlers() {
        crate::realm::reset();
        crate::research::set_trainable(1, UnitType::named("Galley").0 as usize, true);
        let mut city = city_at(1, 2, 3, 3);
        city.coastal = true;
        let mut n = needs();
        n.room = false;
        n.overseas_room = true;
        assert_ne!(choose_build(&city, &n), Production::named("Galley"));
        n.locked.retain(|&p| p != Production::named("Galley"));
        assert_eq!(choose_build(&city, &n), Production::named("Galley"));
        n.transports = 1;
        assert_eq!(choose_build(&city, &n), Production::named("Settler"));
        n.transports = 0;
        city.coastal = false;
        assert_ne!(choose_build(&city, &n), Production::named("Galley"));
        city.coastal = true;
        n.defenders_here = 0;
        assert_ne!(choose_build(&city, &n), Production::named("Galley"));
        assert!(!land_soldier(&Unit::new(
            1,
            UnitType::named("Galley"),
            2,
            3
        )));
    }

    #[test]
    fn a_garrisoned_city_with_room_expands_then_works_then_arms() {
        let city = city_at(1, 5, 5, 3);
        let mut n = needs();
        assert_eq!(choose_build(&city, &n), Production::named("Settler"));
        n.settlers = 2;
        n.workers = 0;
        assert_eq!(choose_build(&city, &n), Production::named("Worker"));
        n.workers = 2;
        n.soldiers = 0;
        let pick = choose_build(&city, &n);
        assert!(pick == Production::named("Archer") || pick == Production::named("Horseman"));
        // No room, no cities beyond the cap: no Settlers.
        n.settlers = 0;
        n.room = false;
        n.soldiers = 3;
        assert_ne!(choose_build(&city, &n), Production::named("Settler"));
        n.room = true;
        n.cities = MAX_CITIES;
        assert_ne!(choose_build(&city, &n), Production::named("Settler"));
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
        assert_eq!(choose_build(&city, &n), Production::named("Temple"));
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
            t.river = 0;
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
    fn workers_irrigate_farmland_and_mine_bare_land() {
        let mut map = GameMap::generate();
        let (x, y) = (10, 10);
        let i = map.idx(x, y);
        map.tiles[i].base = Base::Grassland;
        map.tiles[i].relief = Relief::Flat;
        map.tiles[i].cover = Cover::Bare;
        map.tiles[i].irrigation = false;
        map.tiles[i].road = false;
        // Isolate a one-tile lake: ocean adjacency does not allow irrigation.
        for nx in x - 1..=x + 3 {
            for ny in y - 2..=y + 2 {
                let n = map.idx(nx, ny);
                map.tiles[n].base = Base::Grassland;
            }
        }
        let water = map.idx(x + 1, y);
        map.tiles[water].base = Base::Coast;
        assert_eq!(job_at(&map, &[], (x, y)), Some(WorkAction::Irrigate));
        // No water: mining is available on bare grassland.
        map.tiles[water].base = Base::Grassland;
        for (nx, ny) in map.neighbors(x, y) {
            let n = map.idx(nx, ny);
            map.tiles[n].base = Base::Grassland;
            map.tiles[n].irrigation = false;
        }
        assert_eq!(job_at(&map, &[], (x, y)), Some(WorkAction::Mine));
        map.tiles[i].relief = Relief::Hill;
        assert_eq!(job_at(&map, &[], (x, y)), Some(WorkAction::Mine));
        // A city center is left alone.
        assert_eq!(job_at(&map, &[(x, y)], (x, y)), None);
    }

    #[test]
    fn the_ai_accounts_for_supporting_fire_when_estimating_a_fight() {
        let map = GameMap::generate();
        let (x, y) = map.start;
        let att = Unit::new(0, UnitType::named("Warrior"), x, y);
        let d = (
            Entity::from_bits(1),
            Unit::new(1, UnitType::named("Warrior"), x + 1, y),
        );
        let mut s = (
            Entity::from_bits(2),
            Unit::new(1, UnitType::named("Catapult"), x + 1, y),
        );
        let unassisted = attack_chance(&map, &att, &[d.clone()], None);
        let assisted = attack_chance(&map, &att, &[d.clone(), s.clone()], None);
        assert!(assisted < unassisted);
        s.1.defensive_fired = true;
        assert_eq!(attack_chance(&map, &att, &[d, s], None), unassisted);
    }

    #[test]
    fn a_warrior_does_not_pick_a_fight_with_a_walled_in_veteran() {
        let map = GameMap::generate();
        let (x, y) = map.start;
        let att = Unit::new(1, UnitType::named("Warrior"), x, y);
        // An Archer attacking a lone Warrior is a good bet; a Warrior
        // attacking a Spearman in a city is not.
        let weak = (
            Entity::from_bits(1),
            Unit::new(0, UnitType::named("Warrior"), x + 1, y),
        );
        let strong = (
            Entity::from_bits(2),
            Unit::new(0, UnitType::named("Spearman"), x + 1, y),
        );
        let archer = Unit::new(1, UnitType::named("Archer"), x, y);
        assert!(attack_chance(&map, &archer, &[weak.clone()], None) > 0.6);
        assert!(
            attack_chance(&map, &att, &[strong], Some(combat::Hold::bare(3)))
                < attack_chance(&map, &att, &[weak], Some(combat::Hold::bare(3)))
        );
        // Nothing that can fight back: the tile is simply taken.
        let worker = (
            Entity::from_bits(3),
            Unit::new(0, UnitType::named("Worker"), x + 1, y),
        );
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
        let enemy = (
            Entity::from_bits(9),
            Unit::new(2, UnitType::named("Warrior"), x + 2, y),
        );
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
    fn ai_artillery_routes_detour_around_unroaded_mountains() {
        let mut map = GameMap::generate();
        for t in &mut map.tiles {
            t.base = Base::Grassland;
            t.relief = Relief::Flat;
            t.cover = Cover::Bare;
            t.river = 0;
            t.road = false;
        }
        let i = map.idx(11, 10);
        map.tiles[i].relief = Relief::Mountain;
        let artillery = Unit::new(1, UnitType::named("Catapult"), 10, 10);
        let board = Board::new(vec![], vec![], &map);
        let known = vec![true; map.tiles.len()];
        let b = brain(&map, &board, &known, 1, 30);
        let Act::Go(path) = b.walk_to(&artillery, (12, 10)) else {
            panic!("a detour is available")
        };
        assert_eq!(path.last(), Some(&(12, 10)));
        assert!(!path.contains(&(11, 10)));
    }

    #[test]
    fn ai_artillery_bombards_hostile_units_but_never_peaceful_units() {
        let map = GameMap::generate();
        let a = Unit::new(1, UnitType::named("Catapult"), 10, 10);
        let d = Unit::new(0, UnitType::named("Spearman"), 11, 10);
        let units = vec![(Entity::from_bits(1), a.clone()), (Entity::from_bits(2), d)];
        let known = vec![true; map.tiles.len()];
        let mut war = [[false; CIV_CAP]; CIV_CAP];
        war[1][0] = true;
        war[0][1] = true;
        let board = Board::new(vec![], units.clone(), &map).with_war(war);
        let mut b = brain(&map, &board, &known, 1, 30);
        assert_eq!(
            b.think(Entity::from_bits(1), &a, None).0,
            Act::Bombard((11, 10))
        );
        let board = Board::new(vec![], units, &map).with_war([[false; CIV_CAP]; CIV_CAP]);
        let mut b = brain(&map, &board, &known, 1, 30);
        assert_eq!(b.think(Entity::from_bits(1), &a, None).0, Act::Stay);
    }

    #[test]
    fn ai_artillery_targets_known_enemy_improvements_in_range() {
        let mut map = GameMap::generate();
        let i = map.idx(11, 10);
        map.tiles[i].road = true;
        let a = Unit::new(1, UnitType::named("Catapult"), 10, 10);
        let mut known = vec![true; map.tiles.len()];
        let mut war = [[false; CIV_CAP]; CIV_CAP];
        war[1][0] = true;
        war[0][1] = true;
        let town = city_at(0, 12, 10, 1);
        crate::cities::recompute_borders(&mut map, &[&town], &[0; CIV_CAP]);
        let board = Board::new(vec![town], vec![], &map).with_war(war);
        let mut b = brain(&map, &board, &known, 1, 30);
        assert_eq!(
            b.think(Entity::from_bits(1), &a, None).0,
            Act::Bombard((11, 10))
        );
        known[i] = false;
        let mut b = brain(&map, &board, &known, 1, 30);
        assert_ne!(
            b.think(Entity::from_bits(1), &a, None).0,
            Act::Bombard((11, 10))
        );
        known[i] = true;
        let board = board.with_war([[false; CIV_CAP]; CIV_CAP]);
        let mut b = brain(&map, &board, &known, 1, 30);
        assert_eq!(b.think(Entity::from_bits(1), &a, None).0, Act::Stay);
    }

    #[test]
    fn a_settler_without_a_city_founds_one_where_it_stands() {
        let map = GameMap::generate();
        let at = open_site(&map);
        let board = Board::new(vec![], vec![], &map);
        let known = vec![true; map.tiles.len()];
        let settler = Unit::new(0, UnitType::named("Settler"), at.0, at.1);
        let mut b = brain(&map, &board, &known, 0, 1);
        assert_eq!(b.think(Entity::from_bits(1), &settler, None).0, Act::Found);
    }

    fn blocked_settlement(detour: bool) -> (GameMap, City, Unit) {
        let mut map = GameMap::generate_with_seed(1);
        for t in &mut map.tiles {
            t.base = Base::Ocean;
            t.hut = false;
            t.camp = false;
        }
        for x in 1..=12 {
            let i = map.idx(x, 10);
            map.tiles[i].base = Base::Grassland;
            map.tiles[i].cover = Cover::Bare;
            map.tiles[i].relief = Relief::Flat;
        }
        if detour {
            for x in 5..=7 {
                let i = map.idx(x, 9);
                map.tiles[i].base = Base::Grassland;
                map.tiles[i].cover = Cover::Bare;
                map.tiles[i].relief = Relief::Flat;
            }
        }
        (
            map,
            City::new(1, "Home", 1, 10),
            Unit::new(2, UnitType::named("Warrior"), 6, 10),
        )
    }

    #[test]
    fn a_settler_detours_around_a_peaceful_unit_and_reaches_its_city_site() {
        crate::realm::reset();
        crate::civs::set_controllers();
        let (map, city, blocker) = blocked_settlement(true);
        let mut app = App::new();
        app.edit_schedule(Update, |s| {
            s.set_executor_kind(bevy::ecs::schedule::ExecutorKind::SingleThreaded);
        });
        app.insert_resource(Time::<()>::default());
        app.insert_resource(map);
        app.insert_resource(SplashUp(false));
        app.init_resource::<Civilizations>();
        app.world_mut().resource_mut::<Civilizations>().active = 1;
        app.insert_resource(Turn(1));
        app.init_resource::<AiState>();
        app.insert_resource(crate::combat::CombatRng(crate::rng::MapRng::new(1)));
        app.init_resource::<cities::Treasury>();
        app.init_resource::<crate::unit_picker::UnitPicker>();
        app.insert_resource(crate::diplomacy::Diplomacy::new());
        app.add_message::<FoundCityOrder>();
        app.add_message::<TurnEnded>();
        app.add_message::<crate::bombard::Order>();
        app.add_message::<crate::combat::AttackOrder>();
        app.insert_resource(crate::units::UnitArt::blank());
        app.add_systems(Update, (play_turn, crate::units::drive_movement).chain());
        app.world_mut().spawn(city);
        app.world_mut().spawn(blocker);
        let settler = app
            .world_mut()
            .spawn(Unit::new(1, UnitType::named("Settler"), 4, 10))
            .id();
        app.world_mut()
            .resource_mut::<AiState>()
            .targets
            .insert(settler, (10, 10));
        let mut detoured = false;
        let mut founded = false;
        for _ in 0..120 {
            app.update();
            let u = app.world().get::<Unit>(settler).unwrap();
            assert_ne!((u.x, u.y), (6, 10));
            detoured |= u.y == 9;
            if app
                .world()
                .resource::<Messages<FoundCityOrder>>()
                .iter_current_update_messages()
                .any(|o| o.0 == settler)
            {
                assert_eq!((u.x, u.y), (10, 10));
                founded = true;
                break;
            }
            if !app.world().resource::<Messages<TurnEnded>>().is_empty() {
                app.world_mut()
                    .resource_mut::<Messages<TurnEnded>>()
                    .clear();
                app.world_mut().resource_mut::<Turn>().0 += 1;
                for mut u in app
                    .world_mut()
                    .query::<&mut Unit>()
                    .iter_mut(app.world_mut())
                {
                    u.moves = crate::naval::moves(u.utype, u.civ);
                }
            }
            for mut u in app
                .world_mut()
                .query::<&mut Unit>()
                .iter_mut(app.world_mut())
            {
                u.anim = UnitAnim::Idle { t: 0.0 };
            }
        }
        assert!(
            detoured && founded,
            "detoured={detoured}, founded={founded}"
        );
        assert!(
            app.world()
                .resource::<Messages<crate::combat::AttackOrder>>()
                .is_empty()
        );
    }

    #[test]
    fn a_blocked_settler_replaces_its_old_site_with_a_reachable_one() {
        let (map, city, blocker) = blocked_settlement(false);
        let board = Board::new(vec![city], vec![(Entity::from_bits(2), blocker)], &map);
        let known = vec![true; map.tiles.len()];
        let settler = Unit::new(1, UnitType::named("Settler"), 4, 10);
        let mut b = brain(&map, &board, &known, 1, 5);
        b.claimed.insert((10, 10));
        let (act, target) = b.think(Entity::from_bits(1), &settler, Some((10, 10)));
        assert_ne!(act, Act::Stay);
        assert_ne!(target, Some((10, 10)));
        if let Some(t) = target {
            assert!(t.0 < 6);
        }
        assert!(
            best_site(&map, &board, 1, (4, 10), &HashSet::new())
                .unwrap()
                .0
                < 6
        );
    }

    #[test]
    fn a_worker_replaces_a_blocked_job_with_reachable_work() {
        let (map, city, blocker) = blocked_settlement(false);
        let board = Board::new(vec![city], vec![(Entity::from_bits(2), blocker)], &map);
        let known = vec![true; map.tiles.len()];
        let worker = Unit::new(1, UnitType::named("Worker"), 4, 10);
        let mut b = brain(&map, &board, &known, 1, 5);
        b.claimed.insert((10, 10));
        let (act, target) = b.think(Entity::from_bits(1), &worker, Some((10, 10)));
        assert_ne!(act, Act::Stay);
        assert!(target.is_some_and(|t| t.0 < 6));
        if let Act::Go(path) = act {
            assert!(path.iter().all(|t| t.0 < 6));
        }
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
        let settler = Unit::new(0, UnitType::named("Settler"), far.0, far.1);
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
        let guard = Unit::new(0, UnitType::named("Warrior"), at.0, at.1);
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
            Unit::new(1, UnitType::named("Worker"), step.0, step.1),
        );
        let board = Board::new(vec![city_at(0, at.0, at.1, 1)], vec![prey], &map);
        let mut b = brain(&map, &board, &known, 0, 5);
        b.garrisoned.insert(at, 1);
        let spare = Unit::new(0, UnitType::named("Warrior"), at.0, at.1);
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
        let prey = (
            Entity::from_bits(7),
            Unit::new(1, UnitType::named("Worker"), step.0, step.1),
        );
        let mut peace = [[true; CIV_CAP]; CIV_CAP];
        peace[0][1] = false;
        peace[1][0] = false;
        let board = Board::new(vec![city_at(0, at.0, at.1, 1)], vec![prey], &map).with_war(peace);
        assert!(!board.enemy_on(step, 0));
        assert!(
            board.held_by_other(step, 0),
            "the tile is still not free to stand on"
        );
        let known = vec![true; map.tiles.len()];
        let mut b = brain(&map, &board, &known, 0, 5);
        b.garrisoned.insert(at, 1);
        let spare = Unit::new(0, UnitType::named("Warrior"), at.0, at.1);
        assert!(!matches!(
            b.think(Entity::from_bits(2), &spare, None).0,
            Act::Attack(_)
        ));
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
            Unit::new(1, UnitType::named("Worker"), step.0, step.1),
        );
        let board = Board::new(vec![], vec![prey], &map);
        let known = vec![true; map.tiles.len()];
        let mut b = brain(&map, &board, &known, 0, 5);
        let mut spent = Unit::new(0, UnitType::named("Warrior"), at.0, at.1);
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
        let scout = Unit::new(0, UnitType::named("Scout"), at.0, at.1);
        let mut b = brain(&quiet, &board, &known, 0, 5);
        assert_eq!(b.think(Entity::from_bits(1), &scout, None).0, Act::Disband);
    }
}
