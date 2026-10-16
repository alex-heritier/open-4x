//! Computer-controlled nations.
//!
//! Every nation but the commander's plays one day after production has completed and the
//! workers have worked, in four passes over the same board the player sees:
//!
//! 1. **Settlers** found a city on the best free land in reach ([`settle`]).
//! 2. **Workers** improve the land of their nation's cities, and keep away from the enemy
//!    ([`workers`]).
//! 3. **Soldiers** hold the cities, send spare men to the ones that fall short, and take the
//!    war to the nations they are at war with ([`military`]).
//! 4. **Cities** that have nothing to build choose something ([`production`]).
//!
//! The computer plays by the player's rules, through the same commands, and it remembers
//! nothing from one day to the next: every decision is worked out again from the map, so a
//! saved game needs no AI state and a loaded one plays on exactly as before. It does not
//! respect the fog of war, and it never declares war: it fights the nations the scenario set
//! it against, and any that have declared war on it.
//!
//! Designs are chosen by what they can do, not by name (see [`Roster`]), so any pack plays.
//! Searches are bounded (see the budgets below) because a world turn has a hundred nations.
use crate::economy::SizeClass;
use crate::terrain::{Coord, Map};
use crate::units::*;
use crate::{City, Game, Id, Rules};
use std::collections::{BTreeMap, BTreeSet};

mod military;
mod production;
mod settle;
#[cfg(test)]
mod tests;
mod workers;

/// Computer-controlled forces only pursue targets this close and search a bounded number of
/// tiles, which keeps a turn cheap on a 512×342 world with a hundred nations.
const AI_RANGE: i32 = 48;
const AI_PATH_BUDGET: usize = 20_000;
/// Errands inside a dozen tiles (reinforcing, settling, working, taking shelter) need only a
/// small search, so a goal across the water costs little to give up on.
const AI_NEAR_BUDGET: usize = 2_000;
/// How far (in tiles) a unit in the open looks for a city of its own to go home to.
const AI_HOME_RANGE: i32 = 16;
/// The most cities a computer nation founds.
const AI_MAX_CITIES: usize = 12;
/// A unit acts at most this many times a turn, so blitzing cavalry cannot spin forever.
const AI_ACTIONS: usize = 6;

/// No city wants more than this many defenders of the strongest design.
const MAX_GARRISON: i32 = 3;
/// An enemy fighting unit this close to a city, or nearer, threatens it.
const THREAT_RADIUS: i32 = 8;

/// How many defenders of the strongest design a city wants: one, one more for a capital, one
/// more once it is a city rather than a town, and one more while an enemy is near.
///
/// Defense is counted in strength, not in heads: a city wants `goal * defense` strength
/// standing in it, so a cavalryman with half an infantryman's defense is half a man.
pub(crate) fn garrison_goal(city: &City, threatened: bool) -> i32 {
    let mut goal = 1;
    goal += i32::from(city.capital);
    goal += i32::from(city.size_class() > SizeClass::Town);
    goal += i32::from(threatened);
    goal.min(MAX_GARRISON)
}

/// A land unit that holds ground: it fights back, and neither builds nor founds.
fn is_guard(def: &UnitDef) -> bool {
    def.domain == Domain::Land && def.defense > 0 && !def.settler && !def.can_work()
}
/// What a guard adds to the defense of the city it stands in.
fn guard_strength(def: &UnitDef) -> i32 {
    if is_guard(def) { def.defense } else { 0 }
}
/// Units that can hurt an enemy.
fn is_fighter(def: &UnitDef) -> bool {
    def.can_attack() || def.can_bombard()
}
/// A builder that does not found cities.
fn is_worker(def: &UnitDef) -> bool {
    def.can_work() && !def.settler
}

/// The designs a computer nation builds from, picked by their numbers rather than their
/// names. Each is the best of its kind, ties going to the cheaper design.
struct Roster<'a> {
    /// The strongest defender on land.
    defender: Option<&'a UnitDef>,
    /// The hardest hitter on land.
    striker: Option<&'a UnitDef>,
    /// The best gun that bombards from land.
    gun: Option<&'a UnitDef>,
    worker: Option<&'a UnitDef>,
    settler: Option<&'a UnitDef>,
}
impl<'a> Roster<'a> {
    fn new(rules: &'a Rules) -> Self {
        let land = || rules.units.values().filter(|d| d.domain == Domain::Land);
        let cheaper = |d: &UnitDef| std::cmp::Reverse(d.cost);
        Self {
            defender: land()
                .filter(|d| is_guard(d))
                .max_by_key(|d| (d.defense, cheaper(d))),
            striker: land()
                .filter(|d| d.can_attack() && !d.settler && !d.can_work())
                .max_by_key(|d| (d.attack, cheaper(d))),
            gun: land()
                .filter(|d| d.can_bombard())
                .max_by_key(|d| (d.bombard * d.rate_of_fire, cheaper(d))),
            worker: land()
                .filter(|d| is_worker(d))
                .max_by_key(|d| (d.work, cheaper(d))),
            settler: land().filter(|d| d.settler).min_by_key(|d| d.cost),
        }
    }
}

/// One city of the nation and what is known of its defense today.
struct Post {
    city: Id,
    position: Coord,
    /// A fighting unit of an enemy stands within [`THREAT_RADIUS`].
    threatened: bool,
    /// Defense strength the city wants standing in it.
    wanted: i32,
    /// Defense strength standing in it now.
    held: i32,
    /// Defense strength on its way to it, sent today.
    inbound: i32,
}
impl Post {
    /// Strength still to find, counting what is already coming.
    fn deficit(&self) -> i32 {
        self.wanted - self.held - self.inbound
    }
}

/// One nation's picture of itself for the day: the cities and how well each is held, the
/// units it commands, whom it fights, and what it is already building. It is worked out once
/// from the board when the nation's turn begins and kept up to date as the passes act.
struct Realm {
    id: Id,
    enemies: BTreeSet<Id>,
    /// Where the enemies' fighting units stand.
    hostile: Vec<Coord>,
    posts: Vec<Post>,
    /// The nation's units when its turn began.
    units: Vec<Id>,
    /// Units that have already been given today's orders.
    committed: BTreeSet<Id>,
    /// Fighting units alive or being built.
    fighters: usize,
    /// Settlers alive or being built.
    settlers: usize,
    /// Where a worker stands, or is being built: its city counts as covered.
    worker_spots: Vec<Coord>,
}
impl Realm {
    fn post_at(&self, position: Coord) -> Option<usize> {
        self.posts.iter().position(|p| p.position == position)
    }
    fn hostile_within(&self, map: &Map, at: Coord, radius: i32) -> bool {
        self.hostile.iter().any(|&h| map.distance(h, at) <= radius)
    }
    /// The nearest of the nation's cities within `range` of `from` that `wanting` accepts.
    fn nearest_post(
        &self,
        map: &Map,
        from: Coord,
        range: i32,
        wanting: impl Fn(&Post) -> bool,
    ) -> Option<usize> {
        self.posts
            .iter()
            .enumerate()
            .filter(|(_, post)| wanting(post))
            .map(|(i, post)| (map.distance(post.position, from), post.city, i))
            .filter(|(distance, ..)| *distance <= range)
            .min()
            .map(|(.., i)| i)
    }
    /// Note that a city has started building `def`.
    fn reserve(&mut self, def: &UnitDef, at: Coord) {
        if def.settler {
            self.settlers += 1;
        } else if is_worker(def) {
            self.worker_spots.push(at);
        }
        if is_fighter(def) {
            self.fighters += 1;
        }
    }
}

impl Game {
    pub(crate) fn ai(&mut self, rules: &Rules) {
        let roster = Roster::new(rules);
        let controlled: Vec<Id> = self
            .factions
            .keys()
            .copied()
            .filter(|id| *id != self.commander)
            .collect();
        let mut units_by_owner: BTreeMap<Id, Vec<Id>> = BTreeMap::new();
        for u in self.units.values() {
            units_by_owner.entry(u.owner).or_default().push(u.id);
        }
        let mut cities_by_owner: BTreeMap<Id, Vec<Id>> = BTreeMap::new();
        for c in self.cities.values() {
            cities_by_owner.entry(c.owner).or_default().push(c.id);
        }
        for faction in controlled {
            let units = units_by_owner.get(&faction).cloned().unwrap_or_default();
            let cities = cities_by_owner.remove(&faction).unwrap_or_default();
            let mut realm = self.realm(faction, units, &cities, &units_by_owner, &roster, rules);
            if rules.economy.enabled {
                for u in self.units.values().filter(|u| u.owner == faction) {
                    if u.cargo.is_some()
                        || (u.goto.is_some()
                            && self
                                .units_at(u.position)
                                .iter()
                                .any(|c| c.owner == faction && c.cargo.is_some()))
                    {
                        realm.committed.insert(u.id);
                    }
                }
            }
            self.ai_settlers(&mut realm, rules);
            self.ai_workers(&mut realm, rules);
            self.ai_garrison(&mut realm, rules);
            self.ai_campaign(&mut realm, rules);
            self.ai_production(&mut realm, &roster, rules);
        }
    }

    /// Take stock of a nation at the start of its day.
    fn realm(
        &self,
        faction: Id,
        units: Vec<Id>,
        cities: &[Id],
        units_by_owner: &BTreeMap<Id, Vec<Id>>,
        roster: &Roster,
        rules: &Rules,
    ) -> Realm {
        let enemies: BTreeSet<Id> = self
            .wars
            .iter()
            .filter_map(|&(a, b)| match faction {
                f if f == a => Some(b),
                f if f == b => Some(a),
                _ => None,
            })
            .collect();
        // Earlier nations may have killed or captured some, so each unit is looked up afresh.
        let hostile: Vec<Coord> = enemies
            .iter()
            .filter_map(|enemy| units_by_owner.get(enemy))
            .flatten()
            .filter_map(|id| self.units.get(id))
            .filter(|u| enemies.contains(&u.owner) && is_fighter(rules.def(u)))
            .map(|u| u.position)
            .collect();
        let mut realm = Realm {
            id: faction,
            enemies,
            hostile,
            posts: Vec::new(),
            units: Vec::new(),
            committed: BTreeSet::new(),
            fighters: 0,
            settlers: 0,
            worker_spots: Vec::new(),
        };
        let defense = roster.defender.map_or(0, |d| d.defense);
        for city in cities
            .iter()
            .filter_map(|id| self.cities.get(id))
            .filter(|c| c.owner == faction)
        {
            let threatened = realm.hostile_within(&self.map, city.position, THREAT_RADIUS);
            let held = self
                .units_at(city.position)
                .iter()
                .filter(|u| u.owner == faction && u.carrier.is_none())
                .map(|u| guard_strength(rules.def(u)))
                .sum();
            if let Some(def) = city.production.as_ref().and_then(|k| rules.units.get(k)) {
                realm.reserve(def, city.position);
            }
            realm.posts.push(Post {
                city: city.id,
                position: city.position,
                threatened,
                wanted: garrison_goal(city, threatened) * defense,
                held,
                inbound: 0,
            });
        }
        // A unit alive counts like one being built: a worker covers the ground where it stands.
        for unit in units
            .iter()
            .filter_map(|id| self.units.get(id))
            .filter(|u| u.owner == faction)
        {
            realm.reserve(rules.def(unit), unit.position);
            realm.units.push(unit.id);
        }
        realm
    }
}
