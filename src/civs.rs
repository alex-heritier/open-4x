//! The civilizations of the game: any number of players (up to 31) drawn
//! from the roster of `RACE` rows, plus the barbarians.
use bevy::prelude::*;

use std::collections::VecDeque;

use crate::map::{GameMap, move_cost};

pub use crate::ruleset::CivDefinition;

/// Player slots, the barbarians included (the exe's 32 slots).
pub const CIV_CAP: usize = 32;

/// The owner of barbarian units: the last slot, so civilizations are always
/// `0..civ_count()`. Not a civilization: it has no cities, no turn of its
/// own in the hotseat rotation and no diplomacy, and is at war with
/// everyone (`barbarians.md`).
pub const BARBARIANS: usize = CIV_CAP - 1;

/// Most civilizations a match holds.
pub const MAX_CIVS: usize = BARBARIANS;

pub fn is_barbarian(civ: usize) -> bool {
    civ == BARBARIANS
}

/// The roster indices of the default match: Japan, Rome, Egypt, China. The
/// roster is in `RACE` row order (row 1 at index 0), so Japan is index 8.
pub const DEFAULT_PLAYERS: [usize; 4] = [8, 0, 1, 6];

/// The slots' roster indices. A process-wide setting rather than a resource
/// because the rules code that has to know is plain functions without ECS
/// access. Under test each thread has its own (tests run in parallel and
/// some choose their own roster).
#[cfg(not(test))]
static PLAYERS: std::sync::RwLock<Vec<usize>> = std::sync::RwLock::new(Vec::new());

#[cfg(test)]
thread_local! {
    static PLAYERS: std::cell::RefCell<Vec<usize>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn with_players<R>(f: impl FnOnce(&mut Vec<usize>) -> R) -> R {
    #[cfg(not(test))]
    return f(&mut PLAYERS.write().unwrap());
    #[cfg(test)]
    return PLAYERS.with(|p| f(&mut p.borrow_mut()));
}

/// The roster index each game slot plays. Before a match is set up: the
/// default four.
pub fn players() -> Vec<usize> {
    with_players(|p| {
        if p.is_empty() {
            p.extend(DEFAULT_PLAYERS);
        }
        p.clone()
    })
}

/// How many civilizations are in the match (the barbarians are not).
pub fn civ_count() -> usize {
    with_players(|p| if p.is_empty() { DEFAULT_PLAYERS.len() } else { p.len() })
}

/// The roster index of the civilization in `slot`.
pub fn roster_index(slot: usize) -> usize {
    with_players(|p| {
        if p.is_empty() {
            p.extend(DEFAULT_PLAYERS);
        }
        assert!(slot < p.len(), "slot {slot} is not a civilization");
        p[slot]
    })
}

/// A per-civ table from its first entries; the rest default. For tests and
/// loaders that name only the civs they care about.
pub fn pad<T: Copy + Default>(first: &[T]) -> [T; CIV_CAP] {
    std::array::from_fn(|i| first.get(i).copied().unwrap_or_default())
}

/// The roster index of a civilization by name, case-insensitively.
pub fn roster_index_named(name: &str) -> Option<usize> {
    crate::ruleset::CIV_ROSTER
        .iter()
        .position(|c| c.name.eq_ignore_ascii_case(name))
}

/// Choose which civilization each chair plays, from `--civ` (the human) and
/// `--opponents` (a count or names), see `cli`. Unknown names are skipped
/// with a warning. Without options: the default four.
fn set_players() {
    let o = crate::cli::options();
    if let Some(sc) = crate::scenario::scenario().filter(|s| !s.leads.is_empty()) {
        if o.opponents.is_some() {
            warn!("--opponents: the scenario names its own players");
        }
        set_player_list(sc.civs.clone());
        return;
    }
    set_player_list(pick_players(o));
}

/// The match's roster indices for the options (see `set_players`).
fn pick_players(o: &crate::cli::Options) -> Vec<usize> {
    let named = |name: &str, flag: &str| {
        let found = roster_index_named(name);
        if found.is_none() {
            warn!("{flag}: no civilization called {name}");
        }
        found
    };
    let mut picks = vec![o.civ.as_deref().and_then(|n| named(n, "--civ")).unwrap_or(DEFAULT_PLAYERS[0])];
    let total = match &o.opponents {
        Some(crate::cli::Opponents::Names(names)) => {
            picks.extend(names.iter().filter_map(|n| named(n, "--opponents")));
            picks.len().max(2)
        }
        Some(crate::cli::Opponents::Count(n)) => (*n).max(1) + 1,
        None => DEFAULT_PLAYERS.len(),
    }
    .min(MAX_CIVS);
    // Fill up in the default order, then by roster row.
    let rows = 0..crate::ruleset::CIV_ROSTER.len();
    for r in DEFAULT_PLAYERS.into_iter().chain(rows) {
        if picks.len() >= total {
            break;
        }
        if !picks.contains(&r) {
            picks.push(r);
        }
    }
    picks.truncate(MAX_CIVS);
    distinct(picks)
}

/// Make the picks distinct, left to right: a repeat takes the first unused
/// roster row.
fn distinct(mut picks: Vec<usize>) -> Vec<usize> {
    let roster = crate::ruleset::CIV_ROSTER.len();
    let mut used = vec![false; roster];
    for slot in 0..picks.len() {
        if used[picks[slot]] {
            picks[slot] = (0..roster).find(|&i| !used[i]).unwrap_or(picks[slot]);
        }
        used[picks[slot]] = true;
    }
    picks
}

fn set_player_list(picks: Vec<usize>) {
    with_players(|p| *p = picks);
}

/// Switch the match's roster in a test (the env vars are process-wide).
#[cfg(test)]
pub fn set_players_for_test(picks: &[usize]) {
    set_player_list(picks.to_vec());
}

/// The civilizations of this match, addressable by game slot. `CIVS[i]` is
/// the civ in chair `i`; `CIVS.get(i)` is `None` for the barbarians.
pub struct CivTable;
pub const CIVS: CivTable = CivTable;

impl CivTable {
    pub fn get(&self, i: usize) -> Option<&'static CivDefinition> {
        (i < civ_count()).then(|| &crate::ruleset::CIV_ROSTER[players()[i]])
    }

    pub fn iter(&self) -> impl Iterator<Item = &'static CivDefinition> {
        (0..civ_count()).map(|i| &crate::ruleset::CIV_ROSTER[players()[i]])
    }
}

impl std::ops::Index<usize> for CivTable {
    type Output = CivDefinition;
    fn index(&self, i: usize) -> &CivDefinition {
        &crate::ruleset::CIV_ROSTER[roster_index(i)]
    }
}

/// The `RACE` facts of this match, by game slot, like `CIVS`.
pub struct RaceTable;
pub const RACES: RaceTable = RaceTable;

impl RaceTable {
    pub fn get(&self, i: usize) -> Option<&'static crate::ruleset::RaceFacts> {
        (i < civ_count()).then(|| &crate::ruleset::RACE_ROSTER[players()[i]])
    }

    pub fn iter(&self) -> impl Iterator<Item = &'static crate::ruleset::RaceFacts> {
        (0..civ_count()).map(|i| &crate::ruleset::RACE_ROSTER[players()[i]])
    }
}

impl std::ops::Index<usize> for RaceTable {
    type Output = crate::ruleset::RaceFacts;
    fn index(&self, i: usize) -> &crate::ruleset::RaceFacts {
        &crate::ruleset::RACE_ROSTER[roster_index(i)]
    }
}

/// A unit owner's name, the barbarians included.
pub fn name(civ: usize) -> &'static str {
    CIVS.get(civ).map_or("Barbarians", |c| c.name)
}

/// The `ntpNN.pcx` team-color ramp of a unit owner (0 for the barbarians).
pub fn team_color(civ: usize) -> u8 {
    CIVS.get(civ).map_or(0, |c| c.team_color)
}

/// Which civilizations the computer plays, one bit per `CIVS` index. A
/// process-wide setting rather than a resource because the rules code that
/// has to know (a city's fog check) is plain functions without ECS access.
#[cfg(not(test))]
static AI_MASK: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

// Tests run in parallel threads and some choose their own controllers, so
// under test each thread has a mask of its own.
#[cfg(test)]
thread_local! {
    static AI_MASK: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

fn ai_mask() -> u32 {
    #[cfg(not(test))]
    return AI_MASK.load(std::sync::atomic::Ordering::Relaxed);
    #[cfg(test)]
    return AI_MASK.get();
}

fn set_ai_mask(mask: u32) {
    #[cfg(not(test))]
    AI_MASK.store(mask, std::sync::atomic::Ordering::Relaxed);
    #[cfg(test)]
    AI_MASK.set(mask);
}

/// The computer plays this civilization.
pub fn is_ai(civ: usize) -> bool {
    civ == BARBARIANS || ai_mask() & (1 << civ) != 0
}

/// Choose who plays whom: the first chair is the human and everyone else is
/// the computer, unless `CIV3_HOTSEAT` asks for the old four-chair sandbox or
/// `CIV3_AUTOPLAY` for a game the computer plays alone. `CIV3_CIVS` and
/// `CIV3_PLAYER` choose the roster (`set_players`).
pub fn set_controllers() {
    set_players();
    let all = (1u32 << civ_count()) - 1;
    let mask = if std::env::var("CIV3_HOTSEAT").is_ok() {
        0
    } else if std::env::var("CIV3_AUTOPLAY").is_ok() {
        // A spectator game: the computer plays every civilization.
        all
    } else if let Some(sc) = crate::scenario::scenario().filter(|s| !s.leads.is_empty()) {
        // The scenario's human players (or `--civ`) sit down; the rest are the computer.
        let civ = crate::cli::options().civ.as_deref();
        if let Some(name) = civ && roster_index_named(name).is_none_or(|r| !sc.civs.contains(&r)) {
            warn!("--civ: {name} is not a player of this scenario");
        }
        sc.humans(civ).into_iter().fold(all, |m, h| m & !(1 << h))
    } else {
        all - 1
    };
    set_ai_mask(mask);
}

/// `CIV3_AI_FAST=1`: the computer's moves and fights are never animated,
/// even in the human's sight. For unattended runs.
pub fn ai_fast() -> bool {
    static FAST: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FAST.get_or_init(|| std::env::var("CIV3_AI_FAST").is_ok())
}

/// How the game ended.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    /// Only this civilization is left.
    Victory(usize),
    /// This civilization holds more than the required share of the world's
    /// land and of its people (`victory.md` 7.5).
    Domination(usize),
    /// Every human civilization is gone.
    Defeat,
    /// The scenario's turn limit passed.
    TimeLimit,
}

#[derive(Resource, Default)]
pub struct Civilizations {
    /// The civilization whose turn it is.
    pub active: usize,
    /// The last human to hold the chair: while the computer plays, the
    /// screen keeps showing this civilization's view.
    pub last_human: usize,
    /// Civilizations that lost their last city and settler.
    pub eliminated: [bool; CIV_CAP],
    pub outcome: Option<Outcome>,
}

impl Civilizations {
    /// The state at the start of a match: the first chair acts, and the
    /// screen follows the first human.
    pub fn start() -> Civilizations {
        Civilizations {
            last_human: (0..civ_count()).find(|&c| !is_ai(c)).unwrap_or(0),
            ..Default::default()
        }
    }

    /// Whose fog and treasury the screen shows.
    pub fn viewer(&self) -> usize {
        self.viewer_with(is_ai)
    }

    fn viewer_with(&self, ai: impl Fn(usize) -> bool) -> usize {
        if ai(self.active) {
            self.last_human
        } else {
            self.active
        }
    }

    /// The civilization after `active` that is still in the game.
    pub fn next_active(&self) -> usize {
        (1..=civ_count())
            .map(|k| (self.active + k) % civ_count())
            .find(|&c| !self.eliminated[c])
            .unwrap_or(self.active)
    }
}

/// Whether the game is decided: the humans are all gone (a defeat), or one
/// civilization stands alone (its victory).
pub fn outcome_of(eliminated: &[bool], ai: impl Fn(usize) -> bool) -> Option<Outcome> {
    let alive: Vec<usize> = (0..civ_count()).filter(|&c| !eliminated[c]).collect();
    let humans = (0..civ_count()).any(|c| !ai(c));
    if humans && !alive.iter().any(|&c| !ai(c)) {
        Some(Outcome::Defeat)
    } else if alive.len() == 1 && crate::scenario::settings().conquest {
        Some(Outcome::Victory(alive[0]))
    } else {
        None
    }
}

/// A civilization with no city and no Settler is out of the game: its
/// remaining units disband. The game ends when no human is left, or when
/// one civilization stands alone.
pub fn check_elimination(
    mut commands: Commands,
    mut civs: ResMut<Civilizations>,
    units: Query<(Entity, &crate::units::Unit)>,
    cities: Query<&crate::cities::City>,
    mut board: ResMut<crate::features::MessageBoard>,
    mut strikes: Local<[u8; CIV_CAP]>,
) {
    if civs.outcome.is_some() {
        return;
    }
    for civ in 0..civ_count() {
        if civs.eliminated[civ] {
            continue;
        }
        let rooted = cities.iter().any(|c| c.civ == civ)
            || units
                .iter()
                .any(|(_, u)| u.civ == civ && crate::roles::founds_cities(u.utype));
        // A founding or a capture lands over a frame or two; only a civ
        // that stays empty is gone.
        strikes[civ] = if rooted { 0 } else { strikes[civ].saturating_add(1) };
        if strikes[civ] < 3 {
            continue;
        }
        civs.eliminated[civ] = true;
        for (e, u) in &units {
            if u.civ == civ {
                commands.entity(e).despawn();
            }
        }
        crate::features::post(
            &mut board,
            format!("The {} have been destroyed!", CIVS[civ].name),
        );
        if std::env::var("CIV3_AI_LOG").is_ok() {
            println!("ai: the {} are eliminated", CIVS[civ].name);
        }
    }
    // Assigned only on a change: this runs every frame, and a write would
    // mark `Civilizations` (and so the diplomacy refresh) changed each time.
    let outcome = outcome_of(&civs.eliminated, is_ai);
    if civs.outcome != outcome {
        civs.outcome = outcome;
    }
    if civs.outcome.is_some() && std::env::var("CIV3_AI_LOG").is_ok() {
        println!("ai: game over: {:?}", civs.outcome);
    }
}

/// The game is over when the turn limit has passed (`GAME.time_limit_turns`).
pub fn check_time_limit(turn: Res<crate::units::Turn>, mut civs: ResMut<Civilizations>, mut board: ResMut<crate::features::MessageBoard>) {
    if civs.outcome.is_none() && turn.0 > crate::scenario::settings().turn_limit {
        civs.outcome = Some(Outcome::TimeLimit);
        crate::features::post(&mut board, "The turn limit has been reached.".to_string());
    }
}

/// Type 0, domination (`0x4F1B60`, `victory.md` 7.5): the first civ, in slot
/// order, whose tiles AND citizens are both strictly above
/// `percent * total / 100` (C truncating division). The totals include
/// ground nobody owns and the barbarians' cities.
pub fn domination(
    tiles: &[i32],
    people: &[i32],
    total_tiles: i32,
    total_people: i32,
    in_play: &[bool],
) -> Option<usize> {
    let s = crate::scenario::settings();
    if !s.domination {
        return None;
    }
    let tiles_needed = s.domination_tiles * total_tiles / 100;
    let people_needed = s.domination_people * total_people / 100;
    (0..civ_count()).find(|&c| in_play[c] && tiles[c] > tiles_needed && people[c] > people_needed)
}

/// `CheckVictory` runs once per round, after the last civilization has
/// finished its turn (`victory.md` 11).
pub fn check_domination(
    mut ended: MessageReader<CivilizationEnded>,
    mut civs: ResMut<Civilizations>,
    map: Res<crate::map::GameMap>,
    cities: Query<&crate::cities::City>,
    mut board: ResMut<crate::features::MessageBoard>,
) {
    for CivilizationEnded(civ) in ended.read() {
        if civs.outcome.is_some() || (civ + 1..civ_count()).any(|c| !civs.eliminated[c]) {
            continue;
        }
        let all: Vec<&crate::cities::City> = cities.iter().collect();
        let owners = crate::cities::territory(&map);
        let mut tiles = [0; CIV_CAP];
        let mut total_tiles = 0;
        for y in 0..map.h {
            for x in 0..map.w {
                let Some(t) = map.get(x, y) else { continue };
                // Land and coast count; the open sea and the ice do not.
                if !(map.is_land(x, y) || t.base == crate::map::Base::Coast) {
                    continue;
                }
                total_tiles += 1;
                if let Some(&civ) = owners.get(&(x, y))
                    && civ < civ_count()
                {
                    tiles[civ] += 1;
                }
            }
        }
        let mut people = [0; CIV_CAP];
        let mut total_people = 0;
        for c in &all {
            total_people += i32::from(c.size());
            if c.civ < civ_count() {
                people[c.civ] += i32::from(c.size());
            }
        }
        let in_play: [bool; CIV_CAP] = std::array::from_fn(|c| !civs.eliminated[c]);
        if let Some(winner) = domination(&tiles, &people, total_tiles, total_people, &in_play) {
            civs.outcome = Some(Outcome::Domination(winner));
            crate::features::post(&mut board, format!("The {} dominate the world!", CIVS[winner].name));
            if std::env::var("CIV3_AI_LOG").is_ok() {
                println!("ai: domination victory for the {} ({} of {} tiles, {} of {} people)",
                    CIVS[winner].name, tiles[winner], total_tiles, people[winner], total_people);
            }
        }
    }
}

/// Emitted once for each outgoing civilization, before the next player acts.
#[derive(Message)]
pub struct CivilizationEnded(pub usize);

/// Handoffs move the view to the incoming player's unit or capital.
pub fn focus_active_civ(
    civs: Res<Civilizations>,
    selected: Res<crate::units::Selected>,
    units: Query<&crate::units::Unit>,
    cities: Query<&crate::cities::City>,
    mut camera: Query<&mut Transform, With<Camera2d>>,
    mut previous: Local<usize>,
) {
    // The camera follows the human's chair only: the computer's turns play
    // out wherever they are.
    let viewer = civs.viewer();
    if *previous == viewer {
        return;
    }
    *previous = viewer;
    let position = selected
        .0
        .and_then(|e| units.get(e).ok())
        .map(|u| (u.x, u.y))
        .or_else(|| {
            cities
                .iter()
                .find(|c| c.civ == viewer)
                .map(|c| (c.x, c.y))
        })
        .or_else(|| {
            units
                .iter()
                .find(|u| u.civ == viewer)
                .map(|u| (u.x, u.y))
        });
    if let (Some((x, y)), Ok(mut transform)) = (position, camera.single_mut()) {
        let world = crate::map::tile_to_world(x, y);
        transform.translation.x = world.x;
        transform.translation.y = world.y;
    }
}

/// The preset scenario crowds everyone around Japan: the other civilizations
/// start within this many tiles (king moves) of Japan's capital site.
pub const START_RADIUS: i32 = 10;

/// No two starts closer than this, so every civ has room for a first city.
const MIN_START_SPACING: i32 = 4;

/// Where the other civilizations would like to start, as offsets from the
/// first one's start. Up to four civs: about 7 tiles out in the classic
/// layout (`CIVS[1..]` in order). More: evenly around a ring that grows with
/// the number of players, so a crowded map is shared out.
fn neighbor_offset(civ: usize, n: usize, map: &GameMap) -> (i32, i32) {
    const CLASSIC: [(i32, i32); 3] = [(-6, 4), (0, -7), (6, 4)];
    if n <= 4 {
        return CLASSIC[civ - 1];
    }
    let radius = (7 + n as i32 / 2).min(map.h / 2 - 2).max(7);
    let angle = std::f32::consts::TAU * (civ - 1) as f32 / (n - 1) as f32 + 2.2;
    ((radius as f32 * 1.3 * angle.cos()).round() as i32, (radius as f32 * angle.sin()).round() as i32)
}

/// Capital sites. The first civ keeps `map.start`; each other takes the open
/// site nearest its preferred spot (`neighbor_offset`). No ships exist, so a
/// site must be walkable from the first's. Terrain that can't be settled,
/// huts, camps (units pop them on the spot), and sites crowding another
/// start are out. A site beyond `START_RADIUS` or on poor ground is taken
/// only when nothing better is left, and a crowded map closes the spacing
/// between starts a step at a time, so it degrades instead of failing.
pub fn starting_positions(map: &GameMap) -> Vec<(i32, i32)> {
    let n = civ_count();
    let mut starts = vec![map.start];
    let walkable = walkable_from(map, map.start);
    let reach = if n <= 4 { START_RADIUS } else { START_RADIUS.max(7 + n as i32 / 2 + 3) };
    for civ in 1..n {
        let (dx, dy) = neighbor_offset(civ, n, map);
        let wanted = (map.start.0 + dx, map.start.1 + dy);
        let site = [MIN_START_SPACING, 3, 2, 1].into_iter().find_map(|spacing| {
            (0..map.h)
                .flat_map(|y| (0..map.w).map(move |x| (x, y)))
                .filter(|&(x, y)| {
                    let t = &map.tiles[map.idx(x, y)];
                    walkable[map.idx(x, y)]
                        && !t.hut
                        && !t.camp
                        && crate::cities::can_found(map, &starts, x, y)
                        && starts.iter().all(|&s| map.distance(s, (x, y)) >= spacing)
                })
                .min_by_key(|&site| {
                    (
                        map.distance(map.start, site) > reach,
                        !map.tiles[map.idx(site.0, site.1)].good_start(),
                        // King-move distance leaves ring-shaped ties that scan
                        // order would break toward the north-west.
                        straight_distance_sq(map, wanted, site),
                    )
                })
        });
        starts.push(site.expect("map must have room for all civilizations"));
    }
    starts
}

/// Squared straight-line distance in tiles, wrapping in x.
fn straight_distance_sq(map: &GameMap, a: (i32, i32), b: (i32, i32)) -> i32 {
    let dx = (map.wrap_x(a.0) - map.wrap_x(b.0)).abs();
    let dx = dx.min(map.w - dx);
    let dy = a.1 - b.1;
    dx * dx + dy * dy
}

/// Tiles a land unit can walk to from `from`.
fn walkable_from(map: &GameMap, from: (i32, i32)) -> Vec<bool> {
    let mut reached = vec![false; map.tiles.len()];
    reached[map.idx(from.0, from.1)] = true;
    let mut queue = VecDeque::from([from]);
    while let Some((x, y)) = queue.pop_front() {
        for (nx, ny) in map.neighbors(x, y) {
            let i = map.idx(nx, ny);
            if !reached[i] && move_cost(&map.tiles[i]).is_some() {
                reached[i] = true;
                queue.push_back((nx, ny));
            }
        }
    }
    reached
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domination_needs_both_shares_strictly_above_sixty_six_percent() {
        let play = [true; CIV_CAP];
        // 66 % of 3200 tiles is 2112: 2112 is not enough, 2113 is.
        assert_eq!(domination(&[2112, 0, 0, 0], &[100, 0, 0, 0], 3200, 100, &play), None);
        assert_eq!(domination(&[2113, 0, 0, 0], &[100, 0, 0, 0], 3200, 100, &play), Some(0));
        // Land without the people (or the people without the land) is not enough.
        assert_eq!(domination(&[2200, 0, 0, 0], &[66, 34, 0, 0], 3200, 100, &play), None);
        assert_eq!(domination(&[0, 0, 0, 2200], &[0, 0, 0, 67], 3200, 100, &play), Some(3));
        // Unowned ground and barbarian towns stay in the totals.
        assert_eq!(domination(&[40, 0, 0, 0], &[40, 0, 0, 0], 100, 100, &play), None);
        // An eliminated civ cannot win.
        assert_eq!(domination(&[90, 0, 0, 0], &[90, 0, 0, 0], 100, 100, &[false, true, true, true]), None);
    }

    #[test]
    fn the_round_ends_with_the_last_civ_and_the_world_can_be_dominated() {
        use crate::map::{Base, Cover, GameMap, Relief, Tile};
        let tile = Tile {
            base: Base::Grassland,
            relief: Relief::Flat,
            cover: Cover::Bare,
            variant: 0,
            seen: true,
            visible: true,
            hut: false,
            camp: false,
            resource: None,
            road: false,
            irrigation: false,
            river: 0,
            mine: false,
            site: None,
            fortress: false,
            barricade: false,
            forest_harvested: false,
            owner: None,
        };
        let mut app = App::new();
        app.insert_resource(GameMap { w: 9, h: 9, tiles: vec![tile; 81], start: (4, 4), seed: 0 });
        app.init_resource::<Civilizations>();
        app.init_resource::<crate::features::MessageBoard>();
        app.add_message::<CivilizationEnded>();
        app.init_resource::<crate::cities::BorderKey>();
        app.add_systems(Update, (crate::cities::update_borders, check_domination).chain());
        // A level-four border covers 61 of the 81 tiles, and the only people are ours.
        let mut city = crate::cities::City::new(0, "Edo", 4, 4);
        city.culture = 1000;
        city.set_size(5);
        app.world_mut().spawn(city);
        app.world_mut().write_message(CivilizationEnded(2));
        app.update();
        assert_eq!(app.world().resource::<Civilizations>().outcome, None, "mid-round");
        app.world_mut().write_message(CivilizationEnded(3));
        app.update();
        assert_eq!(app.world().resource::<Civilizations>().outcome, Some(Outcome::Domination(0)));
    }

    #[test]
    fn roster_is_japan_rome_egypt_china_with_distinct_colors() {
        let names: Vec<_> = CIVS.iter().map(|c| c.name).collect();
        assert_eq!(names, ["Japan", "Rome", "Egypt", "China"]);
        let all: Vec<_> = CIVS.iter().collect();
        for (i, a) in all.iter().enumerate() {
            assert!(!a.city_names.is_empty(), "{} has no city names", a.name);
            for b in &all[i + 1..] {
                assert_ne!(a.color, b.color, "{} and {} share a color", a.name, b.name);
            }
        }
    }

    #[test]
    fn the_roster_is_every_playable_civilization_of_the_biq() {
        let roster = &crate::ruleset::CIV_ROSTER;
        assert_eq!(roster.len(), 31, "RACE rows 1..=31");
        let mut names: Vec<&str> = roster.iter().map(|c| c.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 31, "one row per civilization");
        for (i, c) in roster.iter().enumerate() {
            assert!(!c.name.is_empty() && !c.adjective.is_empty(), "{c:?}");
            assert!(!c.city_names.is_empty(), "{} has no city names", c.name);
            assert!(c.team_color <= 31, "{}: ntp{}", c.name, c.team_color);
            let l = &crate::ruleset::LEADER_ROSTER[i];
            assert!(!l.name.is_empty() && !l.title.is_empty(), "{} has no ruler", c.name);
            assert!(l.text_set < crate::speech::TEXT_SETS, "{}", c.name);
        }
        // The barbarians are not in the roster, and the default match is the
        // first four the game shipped with.
        let names: Vec<&str> = CIVS.iter().map(|c| c.name).collect();
        assert_eq!(names, ["Japan", "Rome", "Egypt", "China"]);
    }

    fn options(args: &[&str]) -> crate::cli::Options {
        crate::cli::parse(args.iter().copied(), &std::collections::HashMap::<&str, &str>::new()).unwrap()
    }

    #[test]
    fn opponents_set_how_many_civs_play() {
        assert_eq!(pick_players(&options(&[])), [8, 0, 1, 6], "the default four");
        assert_eq!(pick_players(&options(&["--opponents", "1"])).len(), 2);
        let eight = pick_players(&options(&["--civ", "Rome", "--opponents", "7"]));
        assert_eq!(eight.len(), 8);
        assert_eq!(eight[0], roster_index_named("Rome").unwrap());
        let all = pick_players(&options(&["--opponents", "99"]));
        assert_eq!(all.len(), MAX_CIVS, "31 civilizations and the barbarians fill the 32 slots");
        let mut sorted = all.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), all.len(), "every chair a different civ");
        let named = pick_players(&options(&["--civ", "Egypt", "--opponents", "Mongols,Greece"]));
        assert_eq!(named, [1, roster_index_named("Mongols").unwrap(), roster_index_named("Greece").unwrap()]);
    }

    #[test]
    fn a_big_match_has_its_own_chairs_and_the_barbarians_keep_theirs() {
        set_players_for_test(&pick_players(&options(&["--opponents", "30"])));
        assert_eq!(civ_count(), 31);
        assert_eq!(CIVS[30].name, CIVS.iter().last().unwrap().name);
        assert_eq!(CIVS.get(BARBARIANS).map(|c| c.name), None);
        set_ai_mask((1u32 << civ_count()) - 2);
        assert!(!is_ai(0) && is_ai(1) && is_ai(30) && is_ai(BARBARIANS));
        let civs = Civilizations { active: 30, ..Default::default() };
        assert_eq!(civs.next_active(), 0, "the round wraps after the last chair");
    }

    #[test]
    fn many_civs_all_get_distinct_starts() {
        let mut checked = 0;
        for civs in [6usize, 12, 31] {
            set_players_for_test(&pick_players(&options(&["--opponents", &(civs - 1).to_string()])));
            let map = GameMap::generate();
            let starts = starting_positions(&map);
            assert_eq!(starts.len(), civs);
            for (i, a) in starts.iter().enumerate() {
                assert!(map.is_land(a.0, a.1));
                for b in &starts[..i] {
                    assert_ne!(a, b, "{civs} civs: two starts on one tile");
                }
            }
            checked += 1;
        }
        assert_eq!(checked, 3);
    }

    #[test]
    fn a_repeated_pick_takes_a_free_chair() {
        assert_eq!(distinct(vec![0, 0, 1, 1]), [0, 1, 2, 3]);
        assert_eq!(distinct(vec![8, 0, 1, 6]), [8, 0, 1, 6], "distinct picks are kept");
        assert_eq!(distinct(vec![2, 2, 2, 2]), [2, 0, 1, 3]);
    }

    #[test]
    fn choosing_a_roster_swaps_the_civs_their_traits_and_their_cities() {
        set_controllers();
        // Greece (roster 2), the Mongols (16), Carthage (22) and the Inca (29).
        set_players_for_test(&[2, 16, 22, 29]);
        assert_eq!(CIVS[0].name, "Greece");
        assert_eq!(CIVS[0].city_names[0], "Athens");
        assert_eq!(RACES[0].race, 3, "Greece is RACE row 3");
        assert_eq!(RACES[0].traits, 0xa, "Scientific and Commercial");
        assert_eq!(crate::leaders::LEADERS[0].name, "Alexander");
        assert_eq!(crate::leaders::LEADERS[0].text_set, 2);
        assert_eq!(CIVS[3].name, "Inca");
        assert_eq!(RACES[3].race, 30);
        assert_eq!(team_color(0), 10, "Greece's team ramp is ntp10");
        // Unknown names leave the pick alone.
        assert_eq!(roster_index_named("greece"), Some(2));
        assert_eq!(roster_index_named("Atlantis"), None);
        set_controllers();
        assert_eq!(CIVS[0].name, "Japan", "back to the default match");
    }

    #[test]
    fn unique_units_follow_the_chosen_civilization() {
        set_controllers();
        set_players_for_test(&[2, 0, 1, 6]);
        // The Greek Hoplite is available to Greece and not to Rome.
        let row = crate::roster::unit(
            (0..crate::roster::unit_count())
                .find(|&i| crate::roster::unit(i).name == "Hoplite")
                .expect("Hoplite row"),
        );
        assert!(row.races >> RACES[0].race & 1 != 0, "Greece may train it");
        assert!(row.races >> RACES[1].race & 1 == 0, "Rome may not");
        set_controllers();
    }

    /// Sites every start must satisfy, whatever the seed.
    fn assert_sound_starts(map: &GameMap, starts: &[(i32, i32)], what: &str) {
        assert_eq!(starts[0], map.start, "{what}: Japan keeps the map's start");
        for (civ, &(x, y)) in starts.iter().enumerate() {
            let t = map.get(x, y).unwrap();
            assert!(
                !t.hut && !t.camp,
                "{what}: {} starts on a hut or camp",
                CIVS[civ].name
            );
            if civ > 0 {
                assert!(
                    map.find_path(map.start, (x, y)).is_some(),
                    "{what}: {} cannot walk to Japan",
                    CIVS[civ].name
                );
            }
            for (other, &o) in starts.iter().enumerate().skip(civ + 1) {
                assert!(
                    map.distance((x, y), o) >= MIN_START_SPACING,
                    "{what}: {} and {} start too close",
                    CIVS[civ].name,
                    CIVS[other].name
                );
            }
        }
    }

    /// The preset scenario: Rome, Egypt, and China start on good ground
    /// within ten tiles of Japan.
    #[test]
    fn default_map_puts_everyone_within_ten_tiles_of_japan() {
        let map = GameMap::generate();
        let starts = starting_positions(&map);
        assert_sound_starts(&map, &starts, "default map");
        for civ in 1..civ_count() {
            let d = map.distance(map.start, starts[civ]);
            assert!(d <= 10, "{} starts {d} tiles from Japan", CIVS[civ].name);
            let (x, y) = starts[civ];
            assert!(
                map.get(x, y).unwrap().good_start(),
                "{} starts on poor ground",
                CIVS[civ].name
            );
        }
    }

    #[test]
    fn starts_stay_close_and_sound_across_seeds() {
        for seed in 0..40 {
            let map = GameMap::generate_with_seed(seed);
            let starts = starting_positions(&map);
            assert_sound_starts(&map, &starts, &format!("seed {seed}"));
            for civ in 1..civ_count() {
                let d = map.distance(map.start, starts[civ]);
                assert!(
                    d <= START_RADIUS,
                    "seed {seed}: {} starts {d} tiles away",
                    CIVS[civ].name
                );
            }
        }
    }

    #[test]
    fn the_turn_order_skips_civilizations_that_are_gone() {
        let mut civs = Civilizations::default();
        assert_eq!(civs.next_active(), 1);
        civs.eliminated[1] = true;
        assert_eq!(civs.next_active(), 2);
        civs.active = 3;
        civs.eliminated[0] = true;
        assert_eq!(civs.next_active(), 2);
        // Alone, a civ is its own successor.
        civs.active = 2;
        civs.eliminated = crate::civs::pad(&[true, true, false, true]);
        assert_eq!(civs.next_active(), 2);
    }

    #[test]
    fn the_screen_stays_with_the_last_human_while_the_computer_plays() {
        let ai = |c: usize| c != 0;
        let mut civs = Civilizations::default();
        assert_eq!(civs.viewer_with(ai), 0);
        civs.active = 2;
        assert_eq!(civs.viewer_with(ai), 0, "Egypt's turn is watched as Japan");
        // Hotseat: every chair is human, so the view follows the turn.
        assert_eq!(civs.viewer_with(|_| false), 2);
    }

    #[test]
    fn the_game_ends_when_the_turn_limit_passes() {
        let mut app = App::new();
        app.init_resource::<Civilizations>().init_resource::<crate::features::MessageBoard>();
        app.insert_resource(crate::units::Turn(1));
        app.add_systems(Update, check_time_limit);
        app.update();
        assert_eq!(app.world().resource::<Civilizations>().outcome, None);
        app.insert_resource(crate::units::Turn(crate::scenario::settings().turn_limit + 1));
        app.update();
        assert_eq!(app.world().resource::<Civilizations>().outcome, Some(Outcome::TimeLimit));
    }

    #[test]
    fn the_game_ends_when_the_human_falls_or_one_civ_is_left() {
        let ai = |c: usize| c != 0;
        assert_eq!(outcome_of(&[false; CIV_CAP], ai), None);
        assert_eq!(
            outcome_of(&[true, false, false, false], ai),
            Some(Outcome::Defeat)
        );
        assert_eq!(
            outcome_of(&[false, true, true, true], ai),
            Some(Outcome::Victory(0))
        );
        // Computer civs fighting on do not end the game while Japan stands.
        assert_eq!(outcome_of(&[false, true, false, true], ai), None);
        // Four chairs of humans: the last one standing wins.
        assert_eq!(
            outcome_of(&[true, true, false, true], |_| false),
            Some(Outcome::Victory(2))
        );
        // A game with no humans ends only on a lone survivor.
        assert_eq!(outcome_of(&[true, false, false, false], |_| true), None);
        assert_eq!(
            outcome_of(&[true, true, false, true], |_| true),
            Some(Outcome::Victory(2))
        );
    }

    #[test]
    fn a_civ_with_no_city_and_no_settler_is_eliminated_and_its_units_go() {
        use crate::cities::City;
        use crate::units::{Unit, UnitType};
        let mut app = App::new();
        app.init_resource::<Civilizations>();
        app.init_resource::<crate::features::MessageBoard>();
        app.add_systems(Update, check_elimination);
        let settler = |civ| Unit::new(civ, UnitType::named("Settler"), civ as i32 * 5, 0);
        app.world_mut().spawn(settler(0));
        let doomed_settler = app.world_mut().spawn(settler(1)).id();
        let straggler = app
            .world_mut()
            .spawn(Unit::new(1, UnitType::named("Warrior"), 6, 0))
            .id();
        // Civ 2 holds a city and no units; civ 3 has nothing at all.
        app.world_mut().spawn(City {
            gifts: vec![],
            goods: 0,
            coastal: false,
            river: false,
            unrest: 0,
            hurry_timer: 0,
            stakes: Default::default(),
            cooldown: 0,
            unit_clocks: Vec::new(),
            civ: 2,
            name: "Thebes".into(),
            x: 10,
            y: 0,
            diseased: false,
            citizens: crate::citizens::new_pool(2, 1),
            food: 0,
            shields: 0,
            production: crate::cities::Production::named("Warrior"),
            queue: vec![],
            buildings: vec![],
            culture: 0,
            founded: 1,
        });
        for _ in 0..2 {
            app.update();
        }
        let civs = app.world().resource::<Civilizations>();
        assert_eq!(civs.eliminated[..4], [false; 4], "grace period");
        app.update();
        let civs = app.world().resource::<Civilizations>();
        assert_eq!(civs.eliminated[..4], [false, false, false, true]);
        assert_eq!(civs.outcome, None);
        // Civ 1 loses its settler: its warrior disbands with it.
        app.world_mut().despawn(doomed_settler);
        for _ in 0..4 {
            app.update();
        }
        assert!(app.world().get_entity(straggler).is_err());
        let civs = app.world().resource::<Civilizations>();
        assert_eq!(civs.eliminated[..4], [false, true, false, true]);
        assert_eq!(civs.outcome, None);
    }
}
