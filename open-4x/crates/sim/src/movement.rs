//! Positions, pathfinding, and marching.
//!
//! Movement is eight-directional, counted in thirds of a move. Entering rough ground costs its
//! whole move cost, a step between two roads costs a third of a move, and a step between two
//! railroads is free. A unit with any movement left may always make a step, spending what it has.
//!
//! Every walk is also written down as a [`March`], square by square, so the interface can show
//! the unit walking instead of jumping. The simulation never reads them back.
use crate::{Game, GameError, Id, Index, Result, Rules, error, terrain::Coord, units::*};
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

/// Most marches one command keeps; the oldest are dropped first.
pub const MARCH_LIMIT: usize = 1024;

/// A unit's walk during one command, for the interface to replay. Like the battle records, it
/// says what happened and never how long it takes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct March {
    pub unit: Id,
    pub owner: Id,
    pub kind: String,
    /// The square it started on, then each square it stepped onto, in order.
    pub path: Vec<Coord>,
}

impl March {
    /// What a player may know of the walk: all of it for their own units, otherwise the
    /// stretch between the first and last squares they can see, or nothing.
    pub fn seen_by(&self, player: Id, sees: impl Fn(Coord) -> bool) -> Option<March> {
        if self.owner == player {
            return Some(self.clone());
        }
        let first = self.path.iter().position(|p| sees(*p))?;
        let last = self.path.iter().rposition(|p| sees(*p))?;
        (last > first).then(|| March {
            path: self.path[first..=last].to_vec(),
            ..self.clone()
        })
    }
}

/// Weights a step so that among equally cheap routes the one with fewer steps wins.
const STEP_WEIGHT: u32 = 16;

/// Search state reused between calls. Stamping visited tiles with a generation number means a
/// search costs what it explores, not the size of the map.
struct Search {
    stamp: u32,
    seen: Vec<u32>,
    cost: Vec<u32>,
    previous: Vec<u32>,
    heap: BinaryHeap<Reverse<(u32, u32)>>,
}
impl Search {
    const fn new() -> Self {
        Self {
            stamp: 0,
            seen: Vec::new(),
            cost: Vec::new(),
            previous: Vec::new(),
            heap: BinaryHeap::new(),
        }
    }
    /// Prepare for a search over a map of `tiles` tiles.
    fn begin(&mut self, tiles: usize) {
        if self.seen.len() != tiles {
            self.seen = vec![0; tiles];
            self.cost = vec![0; tiles];
            self.previous = vec![0; tiles];
            self.stamp = 0;
        }
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            self.seen.fill(0);
            self.stamp = 1;
        }
        self.heap.clear();
    }
}
thread_local! {
    static SEARCH: std::cell::RefCell<Search> = const { std::cell::RefCell::new(Search::new()) };
}

impl Game {
    /// Build the lookup tables for units and cities. Lookups work without them (by scanning),
    /// but a freshly loaded game should call this once.
    pub fn reindex(&mut self) {
        let mut units: HashMap<u32, Vec<Id>> = HashMap::new();
        for unit in self.units.values() {
            if let Some(i) = self.map.index(unit.position) {
                units.entry(i as u32).or_default().push(unit.id);
            }
        }
        let cities = self
            .cities
            .values()
            .filter_map(|c| self.map.index(c.position).map(|i| (i as u32, c.id)))
            .collect();
        *self.index = Index {
            built: true,
            units,
            cities,
        };
    }
    fn tile_key(&self, p: Coord) -> Option<u32> {
        self.map.index(p).map(|i| i as u32)
    }

    /// IDs of the units on a square, oldest first.
    pub fn unit_ids_at(&self, p: Coord) -> Vec<Id> {
        if self.index.built {
            self.tile_key(p)
                .and_then(|k| self.index.units.get(&k))
                .cloned()
                .unwrap_or_default()
        } else {
            self.units
                .values()
                .filter(|u| u.position == p)
                .map(|u| u.id)
                .collect()
        }
    }
    pub fn units_at(&self, p: Coord) -> Vec<&crate::Unit> {
        self.unit_ids_at(p)
            .into_iter()
            .filter_map(|id| self.units.get(&id))
            .collect()
    }
    pub fn city_at(&self, p: Coord) -> Option<Id> {
        if self.index.built {
            self.tile_key(p)
                .and_then(|k| self.index.cities.get(&k))
                .copied()
        } else {
            self.cities.values().find(|c| c.position == p).map(|c| c.id)
        }
    }
    /// A city or unit that belongs to someone other than `owner` stands here.
    pub fn blocked_for(&self, owner: Id, p: Coord) -> bool {
        if self
            .city_at(p)
            .is_some_and(|c| self.cities[&c].owner != owner)
        {
            return true;
        }
        if self.index.built {
            self.tile_key(p)
                .and_then(|k| self.index.units.get(&k))
                .is_some_and(|ids| ids.iter().any(|id| self.units[id].owner != owner))
        } else {
            self.units
                .values()
                .any(|u| u.position == p && u.owner != owner)
        }
    }
    /// The nation other than `player` that holds a square through a city or a unit.
    pub fn hostile_holder(&self, p: Coord, player: Id) -> Option<Id> {
        if let Some(c) = self.city_at(p)
            && self.cities[&c].owner != player
        {
            return Some(self.cities[&c].owner);
        }
        self.unit_ids_at(p)
            .into_iter()
            .map(|id| self.units[&id].owner)
            .find(|owner| *owner != player)
    }

    pub(crate) fn add_unit(&mut self, owner: Id, kind: &str, position: Coord) -> Id {
        let id = self.id();
        self.units
            .insert(id, crate::Unit::new(id, owner, kind, position));
        if self.index.built
            && let Some(k) = self.tile_key(position)
        {
            self.index.units.entry(k).or_default().push(id);
        }
        id
    }
    /// Take a unit off the board. A ship lost at sea takes its passengers with it; one lost in
    /// port sets them ashore.
    pub(crate) fn remove_unit(&mut self, id: Id) -> Option<crate::Unit> {
        let position = self.units.get(&id)?.position;
        let at_sea = self.map.get(position).is_some_and(|t| !t.is_land());
        for passenger in self.passengers(id) {
            if at_sea {
                self.remove_unit(passenger);
            } else {
                self.release(passenger);
            }
        }
        let unit = self.units.remove(&id)?;
        self.unindex_unit(id, unit.position);
        Some(unit)
    }
    fn unindex_unit(&mut self, id: Id, position: Coord) {
        if self.index.built
            && let Some(k) = self.tile_key(position)
            && let Some(ids) = self.index.units.get_mut(&k)
        {
            ids.retain(|other| *other != id);
            if ids.is_empty() {
                self.index.units.remove(&k);
            }
        }
    }
    /// Put a unit on a square, keeping the lookup tables right. Tests and scenarios should use
    /// this rather than writing `position` directly. A ship takes its passengers along, and a
    /// passenger that moves away from its ship's square is no longer aboard.
    pub fn set_position(&mut self, id: Id, to: Coord) {
        let Some(from) = self.units.get(&id).map(|u| u.position) else {
            return;
        };
        if from == to {
            return;
        }
        let passengers = self.passengers(id);
        self.unindex_unit(id, from);
        let unit = self.units.get_mut(&id).unwrap();
        unit.position = to;
        if let Some(ship) = unit.carrier {
            let still_aboard = self.units.get(&ship).is_some_and(|s| s.position == to);
            if !still_aboard {
                self.units.get_mut(&id).unwrap().carrier = None;
            }
        }
        if self.index.built
            && let Some(k) = self.tile_key(to)
        {
            let ids = self.index.units.entry(k).or_default();
            ids.push(id);
            ids.sort_unstable();
        }
        for passenger in passengers {
            self.units.get_mut(&passenger).unwrap().clear_orders();
            self.set_position(passenger, to);
        }
    }
    /// Place a new city in the lookup tables.
    pub(crate) fn index_city(&mut self, id: Id) {
        if self.index.built
            && let Some(k) = self.tile_key(self.cities[&id].position)
        {
            self.index.cities.insert(k, id);
        }
    }

    /// Movement, in thirds, to step from `from` to the adjacent `to`.
    pub fn step_cost(&self, from: Coord, to: Coord, domain: Domain) -> u32 {
        let Some(tile) = self.map.get(to) else {
            return 0;
        };
        if domain == Domain::Sea {
            return MOVE_UNIT;
        }
        let from_tile = self.map.get(from);
        let connected = |tile: Option<&crate::terrain::Tile>, p: Coord, rail: bool| {
            self.city_at(p).is_some()
                || tile.is_some_and(|t| if rail { t.has_rail() } else { t.has_road() })
        };
        if connected(from_tile, from, true) && connected(Some(tile), to, true) {
            0
        } else if connected(from_tile, from, false) && connected(Some(tile), to, false) {
            1
        } else {
            tile.move_cost() * MOVE_UNIT
        }
    }

    /// Cheapest route for a unit of `owner`, giving up once more than `limit` squares have
    /// been discovered. Squares held by other nations block the route, except the goal itself.
    /// Ships keep to water, but may enter and cross the nation's own ports.
    pub fn path(
        &self,
        owner: Id,
        domain: Domain,
        start: Coord,
        goal: Coord,
        limit: usize,
    ) -> Option<Vec<Coord>> {
        let goal_index = self.map.index(goal)?;
        let start_index = self.map.index(start)?;
        let width = self.map.width;
        let at = |i: usize| Coord::new(i as i32 % width, i as i32 / width);
        SEARCH.with(|cell| {
            let search = &mut *cell.borrow_mut();
            search.begin(self.map.tiles.len());
            let stamp = search.stamp;
            search.seen[start_index] = stamp;
            search.cost[start_index] = 0;
            search.previous[start_index] = start_index as u32;
            search.heap.push(Reverse((0, start_index as u32)));
            let mut discovered = 1usize;
            while let Some(Reverse((cost, current))) = search.heap.pop() {
                let current = current as usize;
                if cost > search.cost[current] {
                    continue;
                }
                if current == goal_index {
                    let mut path = vec![at(current)];
                    let mut cursor = current;
                    while cursor != start_index {
                        cursor = search.previous[cursor] as usize;
                        path.push(at(cursor));
                    }
                    path.reverse();
                    return Some(path);
                }
                let here = at(current);
                for (dx, dy) in Coord::NEIGHBORS {
                    let next = here.offset(dx, dy);
                    let Some(i) = self.map.index(next) else {
                        continue;
                    };
                    if !self.passable(domain, owner, next)
                        || (i != goal_index && self.blocked_for(owner, next))
                    {
                        continue;
                    }
                    let total = cost + self.step_cost(here, next, domain) * STEP_WEIGHT + 1;
                    if search.seen[i] != stamp {
                        discovered += 1;
                        if discovered > limit {
                            return None;
                        }
                    } else if total >= search.cost[i] {
                        continue;
                    }
                    search.seen[i] = stamp;
                    search.cost[i] = total;
                    search.previous[i] = current as u32;
                    search.heap.push(Reverse((total, i as u32)));
                }
            }
            None
        })
    }

    /// Walk a unit toward `goal` as far as this turn's movement allows. Returns whether it
    /// moved at all. It stops short of a square held by another nation and never attacks.
    pub(crate) fn march(
        &mut self,
        id: Id,
        goal: Coord,
        rules: &Rules,
        budget: usize,
    ) -> Result<bool> {
        let unit = self.units.get(&id).ok_or_else(|| error("Unknown unit"))?;
        let def = rules.def(unit);
        if unit.position == goal {
            return Ok(false);
        }
        let (owner, domain, start) = (unit.owner, def.domain, unit.position);
        let path = self
            .path(owner, domain, start, goal, budget)
            .ok_or_else(|| error("No passable route"))?;
        let mut walked = vec![start];
        for next in path.into_iter().skip(1) {
            let unit = &self.units[&id];
            let left = unit.moves_left(def);
            if left == 0 || self.blocked_for(owner, next) {
                break;
            }
            // Landing from the sea uses up the rest of the turn.
            let landing =
                domain == Domain::Land && self.map.get(unit.position).is_some_and(|t| !t.is_land());
            let spend = if landing {
                left
            } else {
                self.step_cost(unit.position, next, domain).min(left)
            };
            self.set_position(id, next);
            let unit = self.units.get_mut(&id).unwrap();
            unit.moves_used += spend;
            unit.order = Order::None;
            unit.work = 0;
            walked.push(next);
        }
        let moved = walked.len() > 1;
        if moved {
            self.record_march(id, walked);
        }
        Ok(moved)
    }

    /// Write down a walk for the interface.
    pub(crate) fn record_march(&mut self, id: Id, path: Vec<Coord>) {
        let Some(unit) = self.units.get(&id) else {
            return;
        };
        self.marches.push(March {
            unit: id,
            owner: unit.owner,
            kind: unit.kind.clone(),
            path,
        });
        let excess = self.marches.len().saturating_sub(MARCH_LIMIT);
        self.marches.drain(..excess);
    }

    /// The `Move` order: march toward `destination`, and keep marching each turn until there.
    pub(crate) fn move_unit(
        &mut self,
        player: Id,
        id: Id,
        destination: Coord,
        rules: &Rules,
        budget: usize,
    ) -> Result<()> {
        let unit = self.units.get(&id).ok_or_else(|| error("Unknown unit"))?;
        if unit.owner != player {
            return Err(error("You do not command this unit"));
        }
        if unit.position == destination {
            return Err(error("Unit is already there"));
        }
        let def = rules.def(unit);
        let target = self
            .map
            .get(destination)
            .ok_or_else(|| error("Off the map"))?;
        if !def.is_naval() && !target.is_land() {
            // The only way onto the water is up a ship's side.
            return self.board(id, destination, rules);
        }
        if def.is_naval() && !self.passable(Domain::Sea, player, destination) {
            return Err(error("Ships stay at sea or in their own coastal cities"));
        }
        let held = self.blocked_for(player, destination);
        if held && unit.position.distance(destination) == 1 {
            return Err(error("Another nation holds that square; attack it instead"));
        }
        let left = unit.moves_left(def);
        let unit = self.units.get_mut(&id).unwrap();
        unit.clear_orders();
        unit.goto = Some(destination);
        if left == 0 {
            return Ok(());
        }
        if let Err(e) = self.march(id, destination, rules, budget) {
            self.units.get_mut(&id).unwrap().goto = None;
            return Err(e);
        }
        self.finish_march(id, destination);
        Ok(())
    }
    /// End the standing march once the unit has arrived or is held at the border.
    fn finish_march(&mut self, id: Id, goal: Coord) {
        let unit = &self.units[&id];
        let arrived = unit.position == goal;
        let held = unit.position.distance(goal) == 1 && self.blocked_for(unit.owner, goal);
        if arrived || held {
            self.units.get_mut(&id).unwrap().goto = None;
        }
    }
    /// Computer-controlled movement: march this turn and leave no standing order.
    pub(crate) fn march_once(
        &mut self,
        id: Id,
        goal: Coord,
        rules: &Rules,
        budget: usize,
    ) -> Result<bool> {
        let moved = self.march(id, goal, rules, budget)?;
        if let Some(unit) = self.units.get_mut(&id) {
            unit.goto = None;
        }
        Ok(moved)
    }
    /// Units with a standing march continue it as the turn begins.
    pub(crate) fn continue_marches(&mut self, rules: &Rules) {
        let marching: Vec<(Id, Coord)> = self
            .units
            .values()
            .filter_map(|u| u.goto.map(|goal| (u.id, goal)))
            .collect();
        for (id, goal) in marching {
            match self.march(id, goal, rules, usize::MAX) {
                Ok(_) => self.finish_march(id, goal),
                Err(GameError(_)) => {
                    if let Some(unit) = self.units.get_mut(&id) {
                        unit.goto = None;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::March;
    use crate::tests::{arena, rules, spawn};
    use crate::units::MOVE_UNIT;
    use crate::{Command, TickRules, terrain::Coord};

    #[test]
    fn a_walk_is_recorded_square_by_square_and_cleared_by_the_next_command() {
        let mut game = arena(8, 3);
        let id = spawn(&mut game, 1, "cavalry", 0, 1);
        let to = Coord::new(3, 1);
        let mv = Command::Move {
            unit: id,
            destination: to,
        };
        game.apply(1, mv, &rules(), TickRules::default()).unwrap();
        assert_eq!(game.units[&id].position, to);
        let [march] = &game.marches[..] else {
            panic!("one walk: {:?}", game.marches);
        };
        assert_eq!((march.unit, march.owner, &*march.kind), (id, 1, "cavalry"));
        assert_eq!(march.path.first(), Some(&Coord::new(0, 1)));
        assert_eq!(march.path.last(), Some(&to));
        assert_eq!(march.path.len(), 4, "three steps: {:?}", march.path);
        assert!(march.path.windows(2).all(|w| w[0].distance(w[1]) == 1));
        let fortify = Command::Fortify { unit: id };
        game.apply(1, fortify, &rules(), TickRules::default())
            .unwrap();
        assert!(game.marches.is_empty());
    }

    #[test]
    fn others_see_only_the_part_of_a_walk_in_their_sight() {
        let march = March {
            unit: 9,
            owner: 2,
            kind: "infantry".into(),
            path: (0..6).map(|x| Coord::new(x, 0)).collect(),
        };
        let sees = |p: Coord| (2..=3).contains(&p.x);
        assert_eq!(march.seen_by(2, |_| false), Some(march.clone()));
        let seen = march.seen_by(1, sees).unwrap();
        assert_eq!(seen.path, vec![Coord::new(2, 0), Coord::new(3, 0)]);
        assert_eq!(
            march.seen_by(1, |p: Coord| p.x == 4),
            None,
            "one square is no walk"
        );
        assert_eq!(march.seen_by(1, |_| false), None);
    }

    #[test]
    fn roads_and_rails_make_steps_cheaper() {
        let mut game = arena(8, 3);
        let (a, b, c) = (
            crate::terrain::Coord::new(0, 1),
            crate::terrain::Coord::new(1, 1),
            crate::terrain::Coord::new(2, 1),
        );
        let land = crate::units::Domain::Land;
        assert_eq!(game.step_cost(a, b, land), MOVE_UNIT);
        for p in [a, b, c] {
            game.map.get_mut(p).unwrap().improvements = crate::terrain::Tile::ROAD;
        }
        assert_eq!(game.step_cost(a, b, land), 1);
        // a road does not help a step onto bare ground
        assert_eq!(
            game.step_cost(c, crate::terrain::Coord::new(3, 1), land),
            MOVE_UNIT
        );
        for p in [a, b] {
            game.map.get_mut(p).unwrap().improvements |= crate::terrain::Tile::RAIL;
        }
        assert_eq!(game.step_cost(a, b, land), 0);
        assert_eq!(game.step_cost(b, c, land), 1);
    }
}
