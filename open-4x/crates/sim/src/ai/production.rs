//! City production: what a city builds when it has nothing in hand.
//!
//! In order of need:
//!
//! 1. **Defenders**, while the city is short of its garrison goal (counting men on their way).
//! 2. **A worker**, when none stands near enough to work the city's land.
//! 3. **A settler**, while the nation has room and fewer than its limit of cities.
//! 4. **Soldiers**, but only at war, a mix of defenders, strikers, and guns up to a ceiling,
//!    so that a hundred nations cannot grow their armies without bound.
//!
//! A city with nothing it needs builds nothing. There is no upkeep in this game, but there is
//! also no use for an army the nation has no war for. A city with no guard at all drops
//! whatever it started, unless that is nearly done, and builds a defender.
use super::settle::SITE_SEARCH_DAYS;
use super::{AI_MAX_CITIES, Realm, Roster};
use crate::units::UnitDef;
use crate::{Command, Game, Rules, TickRules};

/// A worker covers every city within this many tiles of it.
const WORKER_COVER: i32 = 6;

/// Fighting units a nation at war may have, alive and building together.
fn war_ceiling(cities: usize) -> usize {
    4 + 3 * cities
}

impl Game {
    pub(super) fn ai_production(&mut self, realm: &mut Realm, roster: &Roster, rules: &Rules) {
        for i in 0..realm.posts.len() {
            let id = realm.posts[i].city;
            let Some(city) = self.cities.get(&id).filter(|c| c.owner == realm.id) else {
                continue;
            };
            let (building, progress) = (city.production.clone(), city.progress);
            let pick = match building {
                None => self.next_build(realm, i, roster),
                Some(current) => self.emergency_build(realm, i, roster, &current, progress, rules),
            };
            let Some(def) = pick else {
                continue;
            };
            let command = Command::Produce {
                city: id,
                unit: def.id.clone(),
            };
            if self
                .execute(realm.id, command, rules, TickRules::default())
                .is_ok()
            {
                realm.reserve(def, realm.posts[i].position);
            }
        }
    }

    /// What an idle city should start, if anything.
    fn next_build<'a>(&self, realm: &Realm, i: usize, roster: &Roster<'a>) -> Option<&'a UnitDef> {
        let post = &realm.posts[i];
        if post.deficit() > 0 {
            return roster.defender;
        }
        let covered = realm
            .worker_spots
            .iter()
            .any(|&spot| self.map.distance(spot, post.position) <= WORKER_COVER);
        if !covered && !post.threatened && roster.worker.is_some() {
            return roster.worker;
        }
        if roster.settler.is_some() && !post.threatened && self.wants_settler(realm, post.position)
        {
            return roster.settler;
        }
        if !realm.enemies.is_empty() && realm.fighters < war_ceiling(realm.posts.len()) {
            // Mostly defenders, with strikers and guns mixed in for the offensive.
            let pick = match (self.turn + post.city) % 4 {
                1 => roster.striker,
                2 => roster.gun,
                _ => roster.defender,
            };
            return pick.or(roster.defender);
        }
        None
    }

    /// A city with no guard at all, and nothing near done in hand, builds a defender instead.
    fn emergency_build<'a>(
        &self,
        realm: &Realm,
        i: usize,
        roster: &Roster<'a>,
        current: &str,
        progress: i32,
        rules: &Rules,
    ) -> Option<&'a UnitDef> {
        let post = &realm.posts[i];
        let defender = roster.defender?;
        let unguarded = post.wanted > 0 && post.held + post.inbound == 0;
        let nearly_done = rules
            .units
            .get(current)
            .is_none_or(|def| progress * 2 >= def.cost);
        (unguarded && current != defender.id && !nearly_done).then_some(defender)
    }

    /// Whether the nation should build another settler in the city at `from`: it has room
    /// for more cities and for more settlers on the road, and good free land is near.
    fn wants_settler(&self, realm: &Realm, from: crate::terrain::Coord) -> bool {
        let cities = realm.posts.len();
        cities + realm.settlers < AI_MAX_CITIES
            && realm.settlers < 1 + cities / 4
            && (self.turn + realm.id).is_multiple_of(SITE_SEARCH_DAYS)
            && !self.settlement_sites(realm, from).is_empty()
    }
}
