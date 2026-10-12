//! Settlers: found cities where there is land worth having.
//!
//! A new city claims only land that nobody holds (see [`crate::borders`]), so a site is worth
//! what that free land would yield, less a charge for the walk. A site that would claim next
//! to nothing is not worth a city, which keeps the computer from crowding its own cities.
use super::{AI_MAX_CITIES, AI_NEAR_BUDGET, Realm};
use crate::borders::{default_level, reach_squared};
use crate::economy::square_yield;
use crate::terrain::Coord;
use crate::{Command, Game, Id, Rules, TickRules};
use std::cmp::Reverse;

/// How far from the settler (or from the city that would build one), in tiles, a site is
/// sought.
const AI_SETTLE_RADIUS: i32 = 12;
/// A site must have at least this many free land squares to claim.
const SETTLE_MIN_FREE_LAND: i32 = 6;
/// Points a site loses for each tile of walking.
const SETTLE_DISTANCE_COST: i32 = 3;
/// A site this close to an enemy fighting unit is passed over.
const SETTLE_DANGER: i32 = 6;
/// Sites handed back, best first, so that a settler can fall back if the best is out of reach.
const SETTLE_CHOICES: usize = 3;
/// A computer nation looks for room for a new city one day in this many.
pub(super) const SITE_SEARCH_DAYS: u32 = 4;

impl Game {
    pub(super) fn ai_settlers(&mut self, realm: &mut Realm, rules: &Rules) {
        let faction = realm.id;
        let settlers: Vec<Id> = realm
            .units
            .iter()
            .copied()
            .filter(|id| {
                self.units.get(id).is_some_and(|u| {
                    u.owner == faction && u.carrier.is_none() && rules.def(u).settler
                })
            })
            .collect();
        let mut cities = realm.posts.len();
        for id in settlers {
            if cities >= AI_MAX_CITIES {
                return; // the rest of them work the land instead
            }
            let Some(unit) = self.units.get(&id).filter(|u| u.owner == faction) else {
                continue;
            };
            for site in self.settlement_sites(realm, unit.position) {
                if self.units[&id].position != site
                    && self.march_once(id, site, rules, AI_NEAR_BUDGET).is_err()
                {
                    continue; // out of reach: try the next best
                }
                realm.committed.insert(id);
                if self.units[&id].position == site {
                    let name = format!(
                        "{} Port {}",
                        self.factions[&faction].adjective, self.next_id
                    );
                    let command = Command::FoundCity { unit: id, name };
                    if self
                        .execute(faction, command, rules, TickRules::default())
                        .is_ok()
                    {
                        cities += 1;
                    }
                }
                break;
            }
        }
    }

    /// The best places to found a city within reach of `from`, best first.
    pub(super) fn settlement_sites(&self, realm: &Realm, from: Coord) -> Vec<Coord> {
        let mut sites = Vec::new();
        for y in from.y - AI_SETTLE_RADIUS..=from.y + AI_SETTLE_RADIUS {
            for x in from.x - AI_SETTLE_RADIUS..=from.x + AI_SETTLE_RADIUS {
                let Some(p) = self.map.normalize(Coord::new(x, y)) else {
                    continue;
                };
                let Some(tile) = self.map.get(p) else {
                    continue;
                };
                if !tile.is_land() || (tile.owner != 0 && tile.owner != realm.id) {
                    continue;
                }
                let distance = self.map.distance(from, p);
                if self.city_within(p, 2) || realm.hostile_within(&self.map, p, SETTLE_DANGER) {
                    continue;
                }
                let Some(value) = self.site_value(p) else {
                    continue;
                };
                sites.push((
                    value - SETTLE_DISTANCE_COST * distance,
                    Reverse(distance),
                    Reverse(p),
                ));
            }
        }
        sites.sort_by(|a, b| b.cmp(a));
        sites
            .into_iter()
            .take(SETTLE_CHOICES)
            .map(|(.., Reverse(p))| p)
            .collect()
    }

    /// Whether a city stands within `radius` tiles of `p`, `p` itself included.
    fn city_within(&self, p: Coord, radius: i32) -> bool {
        (-radius..=radius)
            .any(|dy| (-radius..=radius).any(|dx| self.city_at(p.offset(dx, dy)).is_some()))
    }

    /// What a city founded on `p` would gather: the yield of every free square its border
    /// would claim, or `None` if there is too little free land to bother.
    fn site_value(&self, p: Coord) -> Option<i32> {
        let reach = reach_squared(default_level(3, false));
        let radius = (0..).take_while(|r| r * r <= reach).last().unwrap_or(0);
        let (mut free_land, mut value) = (0, 0);
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let centre = (dx, dy) == (0, 0);
                if dx * dx + dy * dy > reach {
                    continue;
                }
                let Some(tile) = self.map.get(p.offset(dx, dy)) else {
                    continue;
                };
                if tile.claim != 0 && !centre {
                    continue; // held by a city already
                }
                let gathered = square_yield(&self.map, tile, centre);
                value += gathered.food + gathered.shields + gathered.gold;
                free_land += i32::from(tile.is_land());
            }
        }
        (free_land >= SETTLE_MIN_FREE_LAND).then_some(value)
    }
}
