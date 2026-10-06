//! Diplomacy: contact, war and peace, treaties and deals, and the computer's
//! attitude (reverse-engineered in `civ3mapgen::diplomacy`, specification
//! `reverse-engineering/diplomacy.md`).
//!
//! The relation state is the binary's: no pair is at war until somebody
//! declares it (nothing else writes the at-war byte), contact comes from
//! seeing a foreign unit or standing at a border, and a war is ended only by
//! a peace treaty. What the game adds, because the binary's own drivers are
//! not decoded, is marked *clone* below:
//! - First contact swaps embassies at once (the binary leaves that to a
//!   dialog).
//! - The border pressure that provokes the computer is a count of foreign
//!   soldiers in its territory (`rel.tension`), with the binary's thresholds
//!   (warning, then war track at `0x200`) but a made-up growth rate.
//! - The computer plans a war every few turns with the binary's war
//!   decision (`wantsWar`, `0x440B60`), aimed at the strongest civ it has met.
//! - Deal prices: the ladder and the `4T+1` scaling are the binary's; the
//!   price of each item is a stand-in on the binary's `x20` scale.
//! - Deals are limited to treaties, alliances, embargoes, contact, gold,
//!   gold per turn and advances; no world maps, luxuries or cities.
use bevy::prelude::*;
use std::collections::{HashMap, HashSet, VecDeque};

use civ3mapgen::diplomacy::{Clause, Env, Relations, Verdict, WarCall, rec, relbit, treaty, weigh};
use civ3mapgen::research::{Dice, Event};

use crate::cities::{City, Treasury, territory};
use crate::civs::RACES;
use crate::civs::{CIV_CAP, CIVS, CivilizationEnded, Civilizations, civ_count, is_ai};
use crate::combat::CombatRng;
use crate::features::{MessageBoard, post};
use crate::map::GameMap;
use crate::research::{Research, civ_of, slot, tech_name};
use crate::units::{Turn, Unit, def};

/// From this turn the computer plans wars.
pub const WAR_PLAN_TURN: u32 = 15;
/// It considers one every this many turns.
pub const WAR_PLAN_PERIOD: u32 = 6;
/// The odds of a trade pitch to a human each turn are 1 in this.
const PITCH_ODDS: u32 = 10;
/// Pressure one soldier in a foreign border adds each turn.
const PRESSURE_PER_UNIT: i32 = 0x40;
/// Pressure that raises the warning (`relbit::BORDER_WARNING`).
const WARNING_AT: i32 = 0x80;
/// Pressure that raises the war track (`relbit::BORDER_WAR`, `0x446C3B`).
const WAR_TRACK_AT: i32 = 0x200;
/// Pressure lost on a turn without intruders.
const PRESSURE_DECAY: i32 = 0x20;

/// A trade the computer offers the human.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proposal {
    pub from: usize,
    pub to: usize,
    pub deal: Deal,
}

/// A deal between two civs: what `a` hands over and what `b` hands over.
/// Treaty clauses (peace, passage, pacts, embargoes) go in `from_a` by
/// convention and bind both.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Deal {
    pub a: usize,
    pub b: usize,
    pub from_a: Vec<Clause>,
    pub from_b: Vec<Clause>,
}

impl Deal {
    pub fn new(a: usize, b: usize) -> Self {
        Deal {
            a,
            b,
            from_a: vec![],
            from_b: vec![],
        }
    }

    pub fn is_empty(&self) -> bool {
        self.from_a.is_empty() && self.from_b.is_empty()
    }

    /// Every clause with the civ that gives it and the civ that gets it.
    pub fn clauses(&self) -> impl Iterator<Item = (usize, usize, &Clause)> {
        self.from_a
            .iter()
            .map(|c| (self.a, self.b, c))
            .chain(self.from_b.iter().map(|c| (self.b, self.a, c)))
    }
}

/// What the attitude and war decisions read from the board.
#[derive(Clone, Debug)]
pub struct Facts {
    pub score: [i32; CIV_CAP],
    pub rank: [i32; CIV_CAP],
    pub cities: [i32; CIV_CAP],
    shared: [[bool; CIV_CAP]; CIV_CAP],
}

impl Facts {
    /// Four equal civilizations sharing a continent.
    #[cfg(test)]
    pub fn even() -> Self {
        Facts {
            score: [10; CIV_CAP],
            rank: [1; CIV_CAP],
            cities: [2; CIV_CAP],
            shared: [[true; CIV_CAP]; CIV_CAP],
        }
    }

    /// Score is cities, advances and soldiers; rank is one plus the civs
    /// scoring strictly more (the binary keeps `+0x183C` and `+0x24`; the
    /// formula is the clone's).
    pub fn new(
        map: &GameMap,
        land: &[u16],
        cities: &[&City],
        units: &[&Unit],
        known: [i32; CIV_CAP],
    ) -> Self {
        let mut count = [0i32; CIV_CAP];
        let mut soldiers = [0i32; CIV_CAP];
        let mut on: Vec<HashSet<u16>> = vec![HashSet::new(); CIV_CAP];
        for c in cities {
            count[c.civ] += 1;
            on[c.civ].insert(land[map.idx(c.x, c.y)]);
        }
        for u in units {
            if def(u.utype).attack > 0 && u.civ < civ_count() {
                soldiers[u.civ] += 1;
            }
        }
        let mut score = [0i32; CIV_CAP];
        for i in 0..civ_count() {
            score[i] = 10 * count[i] + 3 * known[i] + soldiers[i];
        }
        let mut rank = [1i32; CIV_CAP];
        for i in 0..civ_count() {
            rank[i] = 1 + (0..civ_count()).filter(|&j| score[j] > score[i]).count() as i32;
        }
        let mut shared = [[false; CIV_CAP]; CIV_CAP];
        for a in 0..civ_count() {
            for b in 0..civ_count() {
                shared[a][b] = on[a].iter().any(|c| *c != 0 && on[b].contains(c));
            }
        }
        Facts {
            score,
            rank,
            cities: count,
            shared,
        }
    }

    fn civ(p: u32) -> Option<usize> {
        (1..=civ_count() as u32).contains(&p).then(|| civ_of(p))
    }
}

impl Env for Facts {
    fn aggression(&self, p: u32) -> i32 {
        Self::civ(p).map_or(0, |c| RACES[c].aggression)
    }
    fn government(&self, p: u32) -> i32 {
        Self::civ(p).map_or(1, |c| crate::realm::read(c, |r| r.govt as i32))
    }
    fn shunned(&self, p: u32) -> i32 {
        Self::civ(p).map_or(-1, |c| RACES[c].shunned_government)
    }
    fn favorite(&self, p: u32) -> i32 {
        Self::civ(p).map_or(-1, |c| RACES[c].favorite_government)
    }
    fn culture_group(&self, p: u32) -> i32 {
        Self::civ(p).map_or(-1, |c| RACES[c].culture_group)
    }
    fn score(&self, p: u32) -> i32 {
        Self::civ(p).map_or(0, |c| self.score[c])
    }
    fn rank(&self, p: u32) -> i32 {
        Self::civ(p).map_or(civ_count() as i32, |c| self.rank[c])
    }
    fn shares_continent(&self, p: u32, q: u32) -> bool {
        matches!((Self::civ(p), Self::civ(q)), (Some(a), Some(b)) if self.shared[a][b])
    }
    fn nationals(&self, _: u32, _: u32) -> i32 {
        0
    }
    /// `GOVT.war_weariness` of a government row.
    fn war_weariness(&self, government: i32) -> i32 {
        civ3mapgen::government::SHIPPED
            .get(government as usize)
            .map_or(0, |g| g.war_weariness)
    }
    fn map_size(&self) -> (i32, i32) {
        crate::scenario::map_dims()
    }
    fn cities(&self, p: u32) -> i32 {
        Self::civ(p).map_or(0, |c| self.cities[c])
    }
}

/// All diplomatic state of the game.
#[derive(Resource)]
pub struct Diplomacy {
    pub rel: Relations,
    /// Trades the computer offers the human, oldest first.
    pub proposals: VecDeque<Proposal>,
    /// Continent label of every tile, 0 for water.
    pub land: Vec<u16>,
    /// The last turn each civ pitched a trade.
    pitched: [u32; CIV_CAP],
    /// A human's soldier is about to strike a civ it is at peace with: the
    /// question waits for an answer.
    pub war_ask: Option<WarAsk>,
    /// A culture flip toward a human civ waits for its answer.
    pub convert_ask: Option<ConvertAsk>,
    pub board_ask: Option<BoardAsk>,
}

/// What a saved game keeps of the diplomacy: the books and the pitch turns.
/// The trades on offer and the questions asked are not kept.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Saved {
    rel: Vec<i64>,
    pitched: Vec<u32>,
}

impl Diplomacy {
    pub fn snapshot(&self) -> Saved {
        Saved {
            rel: self.rel.to_words(),
            pitched: self.pitched.to_vec(),
        }
    }

    /// Put the books back; false, with nothing changed, if they do not fit.
    pub fn restore(&mut self, saved: &Saved) -> bool {
        let Ok(pitched) = <[u32; CIV_CAP]>::try_from(saved.pitched.clone()) else {
            return false;
        };
        if !self.rel.restore(&saved.rel) {
            return false;
        }
        self.pitched = pitched;
        self.proposals.clear();
        self.war_ask = None;
        self.convert_ask = None;
        self.board_ask = None;
        true
    }
}

/// "Great! Install a new governor" or "Rebuff the rebels" (`capture.md` 6):
/// the city that wants to join `to`, and the human's answer once given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConvertAsk {
    pub city: Entity,
    pub to: usize,
    pub answer: Option<bool>,
}

/// "Select transport": the ships on the tile that could carry `unit`
/// (`movement.md` 9.1, SELECT_TRANSPORT), and the human's pick once made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardAsk {
    pub unit: Entity,
    /// The tile with the ships.
    pub at: (i32, i32),
    /// The candidates with the line each is listed by.
    pub options: Vec<(Entity, String)>,
    /// The explicit Load command asked, not a step onto the ships' tile.
    pub load: bool,
    pub answer: Option<Entity>,
}

/// "Declare war on them?": the strike that asked it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WarAsk {
    pub attacker: Entity,
    pub to: (i32, i32),
    pub target: usize,
}

impl Default for Diplomacy {
    fn default() -> Self {
        Self::new()
    }
}

impl Diplomacy {
    pub fn new() -> Self {
        let mut in_play = 0;
        let mut human = 0;
        for civ in 0..civ_count() {
            in_play |= 1 << slot(civ);
            if !is_ai(civ) {
                human |= 1 << slot(civ);
            }
        }
        Diplomacy {
            rel: Relations::new(in_play, human),
            proposals: VecDeque::new(),
            land: vec![],
            pitched: [0; CIV_CAP],
            war_ask: None,
            convert_ask: None,
            board_ask: None,
        }
    }

    pub fn at_war(&self, a: usize, b: usize) -> bool {
        if crate::civs::is_barbarian(a) || crate::civs::is_barbarian(b) {
            return a != b;
        }
        a != b && self.rel.at_war(slot(a), slot(b))
    }

    pub fn contact(&self, a: usize, b: usize) -> bool {
        a != b && a < civ_count() && b < civ_count() && self.rel.contact(slot(a), slot(b))
    }

    /// A civ among `others` (a bit mask of civs) that `civ` is not at war
    /// with: the one it would first have to declare war on.
    pub fn first_at_peace(&self, civ: usize, others: u32) -> Option<usize> {
        (0..civ_count()).find(|&o| o != civ && others >> o & 1 != 0 && !self.at_war(civ, o))
    }

    /// A blow from `by` against the civs in `victims` goes into their books.
    pub fn note_attack(&mut self, by: usize, victims: u32) {
        for v in (0..civ_count()).filter(|&v| v != by && victims >> v & 1 != 0) {
            self.rel.rec_mut(slot(v), slot(by))[rec::ATTACKS] += 1;
        }
    }

    /// An incident (`Player::0x5631B0`, `government.md` 5.1): `actor` did
    /// something to `victim` that weighs `amount`. Both accumulators of the
    /// victim's book about the actor gain it; the second one feeds the war
    /// weariness of both sides every turn until peace clears it.
    pub fn incident(&mut self, actor: usize, victim: usize, amount: i32) {
        if actor == victim || actor >= civ_count() || victim >= civ_count() {
            return;
        }
        let book = self.rel.rec_mut(slot(victim), slot(actor));
        book[rec::ACC34] += amount;
        book[rec::ACC38] += amount;
    }

    /// War weariness `civ` carries against `other` (`Player +0xCB4`).
    pub fn weariness(&self, civ: usize, other: usize) -> i32 {
        if civ >= civ_count() || other >= civ_count() {
            return 0;
        }
        self.rel.war_counter(slot(civ), slot(other))
    }

    /// The turn update of `civ`'s counters (`0x500AD0`, `government.md`
    /// 5.2). `abroad[c]`: a soldier of `civ` stands on land `c` owns;
    /// `at_home[c]`: a soldier of `c` stands on land `civ` owns.
    pub fn update_weariness(
        &mut self,
        civ: usize,
        abroad: &[bool],
        at_home: &[bool],
        mobilized: bool,
    ) {
        for c in (0..civ_count()).filter(|&c| c != civ) {
            let (p, q) = (slot(civ), slot(c));
            if self.rel.in_play & (1 << q) == 0 {
                continue;
            }
            let counter = self.rel.war_counter(p, q);
            let next = if self.at_war(civ, c) {
                let incidents = self.rel.rec(p, q)[rec::ACC38] + self.rel.rec(q, p)[rec::ACC38];
                civ3mapgen::government::weariness_at_war(
                    counter, incidents, abroad[c], at_home[c], mobilized,
                )
            } else {
                civ3mapgen::government::weariness_at_peace(counter, mobilized)
            };
            *self.rel.war_counter_mut(p, q) = next;
        }
    }

    /// The civs `civ` is at war with, with the weariness against each: the
    /// inputs of the happiness routine and the AI's government score.
    pub fn enemy_weariness(&self, civ: usize) -> Vec<i32> {
        (0..civ_count())
            .filter(|&c| c != civ && self.rel.in_play & (1 << slot(c)) != 0 && self.at_war(civ, c))
            .map(|c| self.weariness(civ, c))
            .collect()
    }

    /// The average war weariness (`0x5007B0`).
    pub fn average_weariness(&self, civ: usize) -> i32 {
        civ3mapgen::government::average_weariness((0..civ_count()).filter(|&c| c != civ).map(|c| {
            civ3mapgen::government::Relation {
                in_play: self.rel.in_play & (1 << slot(c)) != 0,
                met: self.contact(civ, c),
                flag_20: false,
                at_war: self.at_war(civ, c),
                weariness: self.weariness(civ, c),
            }
        }))
    }

    /// Slots `civ` has met, as research counts them.
    pub fn contacts(&self, civ: usize) -> u32 {
        (0..civ_count())
            .filter(|&o| self.contact(civ, o))
            .fold(0, |m, o| m | 1 << slot(o))
    }

    /// Who is at war with whom, by civ.
    pub fn war_matrix(&self) -> [[bool; CIV_CAP]; CIV_CAP] {
        let mut m = [[false; CIV_CAP]; CIV_CAP];
        for (a, row) in m.iter_mut().enumerate() {
            for (b, cell) in row.iter_mut().enumerate() {
                *cell = self.at_war(a, b);
            }
        }
        m
    }

    /// A civ that is gone takes no part.
    pub fn sync_in_play(&mut self, eliminated: &[bool]) {
        for (civ, gone) in eliminated.iter().enumerate() {
            if *gone {
                self.rel.in_play &= !(1 << slot(civ));
            }
        }
    }

    /// Embassies are swapped at the first meeting (*clone*).
    pub fn meet(&mut self, a: usize, b: usize) -> bool {
        let new = self.rel.establish_contact(slot(a), slot(b));
        if new {
            self.rel.exchange_embassies(slot(a), slot(b));
        }
        new
    }

    /// `declareWar`, with the call-in of allies; reports to the human.
    pub fn declare(
        &mut self,
        env: &dyn Env,
        by: usize,
        on: usize,
        reason: i32,
        board: &mut MessageBoard,
    ) -> Vec<WarCall> {
        let calls = self.rel.declare_war(env, slot(by), slot(on), reason, false);
        if std::env::var("CIV3_AI_LOG").is_ok() {
            println!(
                "diplomacy: {} declares war on {} ({} wars started)",
                CIVS[by].name,
                CIVS[on].name,
                calls.len()
            );
        }
        for c in &calls {
            let (a, b) = (civ_of(c.by), civ_of(c.on));
            if !is_ai(b) {
                post(board, format!("The {} have declared war on us!", people(a)));
            } else if !is_ai(a) {
                post(board, format!("We have declared war on the {}.", people(b)));
            } else if self.watched(a) && self.watched(b) {
                post(
                    board,
                    format!("The {} have declared war on the {}.", people(a), people(b)),
                );
            }
        }
        calls
    }

    /// The board as the attitude and war decisions see it.
    pub fn facts(
        &self,
        map: &GameMap,
        research: &Research,
        cities: &[&City],
        units: &[&Unit],
    ) -> Facts {
        Facts::new(map, &self.land, cities, units, known_counts(research))
    }

    /// The treaties between two civs, worded for the Foreign Advisor
    /// (", right of passage").
    pub fn treaties_text(&self, a: usize, b: usize) -> String {
        let bits = self.rel.treaty(slot(a), slot(b));
        let mut s = String::new();
        for (bit, name) in [
            (treaty::ROP, "right of passage"),
            (treaty::MPP, "mutual protection"),
            (treaty::ALLIANCE, "alliance"),
        ] {
            if bits & bit != 0 {
                s += ", ";
                s += name;
            }
        }
        s
    }

    /// A civ some human has met.
    fn watched(&self, civ: usize) -> bool {
        (0..civ_count()).any(|h| !is_ai(h) && (h == civ || self.contact(h, civ)))
    }

    /// `makePeace`.
    #[cfg(test)]
    pub fn make_peace(&mut self, a: usize, b: usize) {
        self.rel.make_peace(slot(a), slot(b));
    }

    /// The attitude class of `p` towards `q` (0 friendliest .. 4 most hostile).
    pub fn attitude(&self, env: &dyn Env, p: usize, q: usize) -> i32 {
        self.rel.attitude_class(env, slot(p), slot(q), 0)
    }

    // -----------------------------------------------------------------
    // Deals
    // -----------------------------------------------------------------

    /// Whether the deal can be made at all, and if not why.
    pub fn check(
        &self,
        research: &Research,
        treasury: &[u32],
        deal: &Deal,
    ) -> Result<(), &'static str> {
        if deal.is_empty() {
            return Err("Nothing is on the table.");
        }
        let (a, b) = (deal.a, deal.b);
        if !self.contact(a, b) {
            return Err("We have not met.");
        }
        let at_war = self.at_war(a, b);
        let mut gold = [0u32; CIV_CAP];
        let mut peace = 0;
        for (giver, taker, c) in deal.clauses() {
            if at_war && !matches!(c, Clause::Peace) {
                return Err("There is no trading in a war; only peace can be made.");
            }
            let (g, t) = (slot(giver), slot(taker));
            match c {
                Clause::Peace => {
                    peace += 1;
                    if !at_war {
                        return Err("We are not at war.");
                    }
                }
                Clause::MutualProtection | Clause::RightOfPassage => {
                    if !self.rel.clause_valid(g, t, c) {
                        return Err("That treaty is not possible now.");
                    }
                    if !(research.knows_flag(giver, c.tech_flag())
                        && research.knows_flag(taker, c.tech_flag()))
                    {
                        return Err("Both sides need the advance for that treaty.");
                    }
                }
                Clause::MilitaryAlliance(x) | Clause::Embargo(x) => {
                    let x = civ_of(*x);
                    if !self.rel.clause_valid(g, t, c)
                        || !self.contact(giver, x)
                        || !self.contact(taker, x)
                    {
                        return Err("That pact is not possible now.");
                    }
                    if !(research.knows_flag(giver, c.tech_flag())
                        && research.knows_flag(taker, c.tech_flag()))
                    {
                        return Err("Both sides need the advance for that pact.");
                    }
                }
                Clause::Contact(x) => {
                    if !self.rel.clause_valid(g, t, c) || civ_of(*x) == giver || civ_of(*x) == taker
                    {
                        return Err("That introduction is not possible.");
                    }
                }
                Clause::Gold(n) | Clause::GoldPerTurn(n) => {
                    if *n <= 0 {
                        return Err("A sum of gold must be more than nothing.");
                    }
                    gold[giver] += *n as u32;
                    if gold[giver] > treasury[giver] {
                        return Err("There is not that much gold.");
                    }
                }
                Clause::Tech(x) => {
                    if !(research.knows(giver, *x) && research.giftable(giver, taker).contains(x)) {
                        return Err("That advance cannot be given.");
                    }
                }
                Clause::WorldMap | Clause::City(_) => return Err("That cannot be traded yet."),
            }
        }
        if peace > 1 {
            return Err("One peace treaty is enough.");
        }
        Ok(())
    }

    /// What the computer `ai` would give for a clause or want for it, on the
    /// binary's x20 scale (*clone* prices). `None` makes the deal impossible
    /// (verdict 40).
    #[allow(clippy::too_many_arguments)]
    pub fn price(
        &self,
        env: &dyn Env,
        research: &Research,
        ai: usize,
        other: usize,
        clause: &Clause,
        ai_gets: bool,
        dice: &mut dyn Dice,
    ) -> Option<i32> {
        let class = self.attitude(env, ai, other);
        Some(match clause {
            Clause::Gold(n) => 20 * n,
            Clause::GoldPerTurn(n) => 20 * n * 10,
            // What the advance is worth to the one who would have it.
            Clause::Tech(t) => {
                let who = if ai_gets { ai } else { other };
                20 * research.worth(who, *t, dice).max(1)
            }
            Clause::Contact(_) => {
                if ai_gets {
                    20 * 4
                } else {
                    20 * 10
                }
            }
            // The price of peace: nothing for a friend or a loser, more the
            // angrier it is and the better the war is going.
            Clause::Peace => {
                let wc = self.rel.war_counter(slot(ai), slot(other));
                let k = class - 1 - wc / 20;
                500 * k.max(0)
            }
            Clause::RightOfPassage | Clause::MutualProtection => {
                let strict = if matches!(clause, Clause::MutualProtection) {
                    1
                } else {
                    3
                };
                if class > strict {
                    return None;
                }
                20 * (4 - class) * 10
            }
            Clause::MilitaryAlliance(x) => {
                let x = civ_of(*x);
                if class > 2 || !(self.at_war(ai, x) || self.attitude(env, ai, x) >= 3) {
                    return None;
                }
                20 * 150
            }
            Clause::Embargo(x) => {
                if self.attitude(env, ai, civ_of(*x)) < 3 {
                    return None;
                }
                20 * 60
            }
            Clause::WorldMap | Clause::City(_) => return None,
        })
    }

    /// `0x440EE0` for the deal as the computer `ai` sees it.
    pub fn weigh(
        &self,
        env: &dyn Env,
        research: &Research,
        ai: usize,
        deal: &Deal,
        dice: &mut dyn Dice,
    ) -> Verdict {
        let other = if deal.a == ai { deal.b } else { deal.a };
        let (mut offer, mut ask) = (vec![], vec![]);
        for (giver, _, c) in deal.clauses() {
            let mutual = matches!(
                c,
                Clause::Peace
                    | Clause::RightOfPassage
                    | Clause::MutualProtection
                    | Clause::MilitaryAlliance(_)
                    | Clause::Embargo(_)
            );
            // A treaty is a thing the computer grants (peace) or gains
            // (the rest); everything else is by who hands it over.
            let in_ask = if mutual {
                matches!(c, Clause::Peace)
            } else {
                giver == ai
            };
            if in_ask {
                ask.push(c.clone());
            } else {
                offer.push(c.clone());
            }
        }
        weigh(
            &self.rel,
            slot(ai),
            slot(other),
            &offer,
            &ask,
            &mut |c, offered| self.price(env, research, ai, other, c, offered, dice),
        )
    }

    /// Carry out a deal that has been checked and accepted. Returns the
    /// research events of any advance handed over.
    #[allow(clippy::too_many_arguments)]
    pub fn execute(
        &mut self,
        env: &dyn Env,
        research: &mut Research,
        treasury: &mut [u32],
        deal: &Deal,
        turn: i32,
        dice: &mut dyn Dice,
        board: &mut MessageBoard,
    ) -> Vec<Event> {
        let mut events = vec![];
        let mut timed: [Vec<Clause>; CIV_CAP] = Default::default();
        for (giver, taker, c) in deal.clauses() {
            match c {
                Clause::Gold(n) => {
                    treasury[giver] -= *n as u32;
                    treasury[taker] += *n as u32;
                }
                Clause::GoldPerTurn(_) => timed[giver].push(c.clone()),
                Clause::Tech(t) => {
                    events.extend(research.award(taker, *t, dice));
                    self.rel.rec_mut(slot(giver), slot(taker))[rec::TECH_TRADES] += 1;
                    self.rel.rec_mut(slot(taker), slot(giver))[rec::TECH_TRADES] += 1;
                }
                Clause::Peace => {
                    self.rel.apply(env, slot(giver), slot(taker), c);
                    timed[giver].push(c.clone());
                }
                _ => {
                    let calls = self.rel.apply(env, slot(giver), slot(taker), c);
                    for w in calls {
                        post(
                            board,
                            format!(
                                "The {} have declared war on the {}.",
                                people(civ_of(w.by)),
                                people(civ_of(w.on))
                            ),
                        );
                    }
                }
            }
        }
        for (civ, clauses) in timed.iter().enumerate() {
            if !clauses.is_empty() {
                let other = if civ == deal.a { deal.b } else { deal.a };
                self.rel.open_package(slot(civ), slot(other), clauses, turn);
            }
        }
        events
    }

    // -----------------------------------------------------------------
    // The computer's initiative
    // -----------------------------------------------------------------

    /// The civ `p` would go to war with, if it plans one this turn: the
    /// strongest civ it has met and is at peace with, when the binary's war
    /// decision says so (*clone*: the binary asks it of a provoker).
    pub fn war_plan(
        &self,
        env: &dyn Env,
        p: usize,
        turn: u32,
        dice: &mut dyn Dice,
    ) -> Option<usize> {
        if turn < WAR_PLAN_TURN
            || (turn - WAR_PLAN_TURN) % WAR_PLAN_PERIOD != p as u32 % WAR_PLAN_PERIOD
        {
            return None;
        }
        let target = (0..civ_count())
            .filter(|&q| {
                q != p
                    && self.contact(p, q)
                    && !self.at_war(p, q)
                    && self.rel.in_play >> slot(q) & 1 != 0
                    && !self.rel.package_between(slot(p), slot(q))
            })
            .max_by_key(|&q| (env.score(slot(q)), q))?;
        self.rel
            .wants_war(env, slot(p), slot(target), &mut |n| dice.below(n))
            .then_some(target)
    }

    /// A trade the computer `ai` would propose to the human `human`: an
    /// advance for an advance, when it likes the human and would accept
    /// the swap itself.
    pub fn pitch(
        &self,
        env: &dyn Env,
        research: &Research,
        ai: usize,
        human: usize,
        dice: &mut dyn Dice,
    ) -> Option<Proposal> {
        if !self.contact(ai, human) || self.at_war(ai, human) || self.attitude(env, ai, human) > 2 {
            return None;
        }
        let mut theirs = research.giftable(ai, human);
        let mut ours = research.giftable(human, ai);
        // Offer the dearest advance the human lacks for the dearest the
        // computer lacks.
        theirs.sort_by_key(|&t| std::cmp::Reverse(research.worth(human, t, dice)));
        ours.sort_by_key(|&t| std::cmp::Reverse(research.worth(ai, t, dice)));
        for &give in theirs.iter().take(3) {
            for &get in ours.iter().take(3) {
                let mut deal = Deal::new(human, ai);
                deal.from_a.push(Clause::Tech(get));
                deal.from_b.push(Clause::Tech(give));
                if self.weigh(env, research, ai, &deal, dice) == Verdict::Accept {
                    return Some(Proposal {
                        from: ai,
                        to: human,
                        deal,
                    });
                }
            }
        }
        None
    }
}

/// How the civ's people are named in the news: "the Romans", "the Japanese".
pub fn people(civ: usize) -> String {
    CIVS[civ].noun.to_string()
}

/// The attitude word the Foreign Advisor shows: the five `#ANGER_AT_LEVELS`
/// of `diplomacy.txt` for the binary's five classes.
pub fn attitude_label(class: i32) -> &'static str {
    match class {
        0 => "Gracious",
        1 => "Polite",
        2 => "Cautious",
        3 => "Annoyed",
        _ => "Furious",
    }
}

/// What a human reads of a clause.
pub fn clause_text(c: &Clause) -> String {
    match c {
        Clause::Peace => "Peace treaty".into(),
        Clause::MutualProtection => "Mutual protection pact".into(),
        Clause::RightOfPassage => "Right of passage".into(),
        Clause::MilitaryAlliance(x) => {
            format!("Military alliance against the {}", people(civ_of(*x)))
        }
        Clause::Embargo(x) => format!("Embargo of the {}", people(civ_of(*x))),
        Clause::WorldMap => "World map".into(),
        Clause::Contact(x) => format!("Introduction to the {}", people(civ_of(*x))),
        Clause::Gold(n) => format!("{n} gold"),
        Clause::GoldPerTurn(n) => format!("{n} gold per turn"),
        Clause::Tech(t) => tech_name(*t).to_string(),
        Clause::City(_) => "A city".into(),
    }
}

/// How the computer answers a deal it weighed.
pub fn verdict_text(v: Verdict) -> &'static str {
    match v {
        Verdict::Accept => "We have a deal.",
        Verdict::WeakReject => "That is close, but we cannot accept.",
        Verdict::NeutralReject => "We are not interested in that.",
        Verdict::StrongReject => "Absolutely not!",
        Verdict::Invalid => "We will not discuss that.",
    }
}

/// Continents: connected land gets a label, water 0.
pub fn label_land(map: &GameMap) -> Vec<u16> {
    let mut label = vec![0u16; map.tiles.len()];
    let mut next = 0u16;
    for y in 0..map.h {
        for x in 0..map.w {
            let i = map.idx(x, y);
            if label[i] != 0 || crate::improvements::is_water_base(map.tiles[i].base) {
                continue;
            }
            next += 1;
            label[i] = next;
            let mut stack = vec![(x, y)];
            while let Some((cx, cy)) = stack.pop() {
                for (nx, ny) in map.neighbors(cx, cy) {
                    let j = map.idx(nx, ny);
                    if label[j] == 0 && !crate::improvements::is_water_base(map.tiles[j].base) {
                        label[j] = next;
                        stack.push((nx, ny));
                    }
                }
            }
        }
    }
    label
}

/// Pairs of civs whose people have met: a soldier or settler within two
/// tiles of a foreign unit, or within three of a foreign city. The binary
/// asks whether a unit *sees* a foreign unit or stands beside foreign
/// territory (`turn.md` 3.3); this is that with the clone's sight.
pub fn meetings(
    map: &GameMap,
    units: &[&Unit],
    cities: &[&City],
    met: impl Fn(usize, usize) -> bool,
) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = vec![];
    let mut add = |a: usize, b: usize| {
        let pair = (a.min(b), a.max(b));
        if !met(pair.0, pair.1) && !out.contains(&pair) {
            out.push(pair);
        }
    };
    for (i, u) in units.iter().enumerate() {
        for v in &units[i + 1..] {
            if u.civ != v.civ
                && u.civ < civ_count()
                && v.civ < civ_count()
                && map.distance((u.x, u.y), (v.x, v.y)) <= 2
            {
                add(u.civ, v.civ);
            }
        }
        for c in cities {
            if u.civ != c.civ && u.civ < civ_count() && map.distance((u.x, u.y), (c.x, c.y)) <= 3 {
                add(u.civ, c.civ);
            }
        }
    }
    out
}

/// Soldiers of one civ standing in another's territory, as
/// `(owner, intruder) -> count`.
pub fn trespassers(
    owner: &HashMap<(i32, i32), usize>,
    units: &[&Unit],
) -> HashMap<(usize, usize), i32> {
    let mut out: HashMap<(usize, usize), i32> = HashMap::new();
    for u in units {
        if def(u.utype).attack == 0 {
            continue;
        }
        if let Some(&host) = owner.get(&(u.x, u.y)) {
            if host != u.civ && u.civ < civ_count() {
                *out.entry((host, u.civ)).or_default() += 1;
            }
        }
    }
    out
}

/// The advances each civ knows, for the score.
fn known_counts(research: &Research) -> [i32; CIV_CAP] {
    let mut k = [0; CIV_CAP];
    for (civ, n) in k.iter_mut().enumerate() {
        *n = research.world.players[slot(civ) as usize].known_count;
    }
    k
}

// ---------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------

/// Label the continents once the map exists.
pub fn setup(map: Res<GameMap>, mut diplomacy: ResMut<Diplomacy>) {
    diplomacy.land = label_land(&map);
}

/// Civs that fell are out of the diplomatic world.
pub fn refresh(civs: Res<Civilizations>, mut diplomacy: ResMut<Diplomacy>) {
    if civs.is_changed() {
        diplomacy.sync_in_play(&civs.eliminated);
    }
}

/// First contact.
pub fn detect_contact(
    map: Res<GameMap>,
    cities: Query<&City>,
    units: Query<&Unit>,
    mut diplomacy: ResMut<Diplomacy>,
    mut board: ResMut<MessageBoard>,
) {
    let pairs = (0..civ_count()).flat_map(|a| (a + 1..civ_count()).map(move |b| (a, b)));
    if pairs.clone().all(|(a, b)| diplomacy.contact(a, b)) {
        return;
    }
    let cs: Vec<&City> = cities.iter().collect();
    let us: Vec<&Unit> = units.iter().collect();
    for (a, b) in meetings(&map, &us, &cs, |a, b| diplomacy.contact(a, b)) {
        if diplomacy.meet(a, b) {
            for (me, other) in [(a, b), (b, a)] {
                if !is_ai(me) {
                    post(
                        &mut board,
                        format!("We have made contact with the {}.", people(other)),
                    );
                }
            }
        }
    }
}

/// A civ's turn ends: timed deals run, the computer reacts to soldiers in
/// its borders, plans a war and pitches trades.
#[allow(clippy::too_many_arguments)]
pub fn end_turn(
    mut ended: MessageReader<CivilizationEnded>,
    map: Res<GameMap>,
    cities: Query<&City>,
    units: Query<&Unit>,
    research: Res<Research>,
    turn: Res<Turn>,
    mut diplomacy: ResMut<Diplomacy>,
    mut treasury: ResMut<Treasury>,
    mut rng: ResMut<CombatRng>,
    mut board: ResMut<MessageBoard>,
) {
    for ev in ended.read() {
        let civ = ev.0;
        let cs: Vec<&City> = cities.iter().collect();
        let us: Vec<&Unit> = units.iter().collect();
        let facts = diplomacy.facts(&map, &research, &cs, &us);
        let owner = territory(&map);
        diplomacy.run_turn_end(
            &facts,
            &research,
            civ,
            turn.0,
            &trespassers(&owner, &us),
            &mut treasury.0,
            &mut rng.0,
            &mut board,
        );
    }
}

impl Diplomacy {
    /// Everything that happens to `civ`'s diplomacy when its turn ends.
    #[allow(clippy::too_many_arguments)]
    pub fn run_turn_end(
        &mut self,
        facts: &Facts,
        research: &Research,
        civ: usize,
        turn: u32,
        intruders: &HashMap<(usize, usize), i32>,
        treasury: &mut [u32],
        dice: &mut dyn Dice,
        board: &mut MessageBoard,
    ) {
        let me = slot(civ);
        // Deals that run for turns: the lapsed end first, the rest pay.
        self.rel.expire_packages(turn as i32);
        for k in &self.rel.packages {
            if k.a != me {
                continue;
            }
            for c in &k.clauses {
                if let Clause::GoldPerTurn(g) = c {
                    let paid = (*g as u32).min(treasury[civ]);
                    treasury[civ] -= paid;
                    treasury[civ_of(k.b)] += paid;
                }
            }
        }
        // Soldiers in another civ's territory build pressure on its owner.
        for owner in 0..civ_count() {
            if owner == civ || !self.contact(owner, civ) || self.at_war(owner, civ) {
                continue;
            }
            let (o, i) = (slot(owner), me);
            let n = intruders.get(&(owner, civ)).copied().unwrap_or(0);
            if n == 0 || self.rel.treaty(o, i) & treaty::ROP != 0 {
                let t = self.rel.tension_mut(o, i);
                *t = (*t - PRESSURE_DECAY).max(0);
                if *t < WARNING_AT {
                    *self.rel.rel_mut(o, i) &= !(relbit::BORDER_WARNING | relbit::BORDER_WAR);
                }
                continue;
            }
            *self.rel.tension_mut(o, i) += PRESSURE_PER_UNIT * n;
            let t = self.rel.tension(o, i);
            if t >= WAR_TRACK_AT {
                self.rel.border_pressure(o, i, t);
                // The owner's patience is spent: one more grievance, and
                // the war decision.
                self.rel.rec_mut(o, i)[rec::HOSTILE_ACTS] += 1;
                *self.rel.tension_mut(o, i) = WAR_TRACK_AT / 2;
                if is_ai(owner) && self.rel.wants_war(facts, o, i, &mut |n| dice.below(n)) {
                    self.declare(facts, owner, civ, 0, board);
                } else if !is_ai(owner) || !is_ai(civ) {
                    post(
                        board,
                        format!(
                            "The {} demand that we withdraw our soldiers.",
                            people(owner)
                        ),
                    );
                }
            } else if t >= WARNING_AT && self.rel.rel(o, i) & relbit::BORDER_WARNING == 0 {
                self.rel.border_pressure(o, i, t);
                if !is_ai(civ) {
                    post(
                        board,
                        format!("Our soldiers are in the borders of the {}.", people(owner)),
                    );
                }
            }
        }
        if !is_ai(civ) || self.rel.in_play >> me & 1 == 0 {
            return;
        }
        // The computer's own moves: a war, a pitch to each human.
        if let Some(target) = self.war_plan(facts, civ, turn, dice) {
            self.declare(facts, civ, target, 0, board);
        }
        for human in (0..civ_count()).filter(|&h| !is_ai(h)) {
            if self.pitched[civ] + 3 > turn || self.proposals.iter().any(|p| p.from == civ) {
                continue;
            }
            if dice.below(PITCH_ODDS) != 0 {
                continue;
            }
            if let Some(p) = self.pitch(facts, research, civ, human, dice) {
                self.pitched[civ] = turn;
                self.proposals.push_back(p);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::UnitType;
    use civ3mapgen::rng::Rng;

    fn research() -> Research {
        crate::civs::set_controllers();
        let mut r = Research::new();
        r.begin(&mut Rng::new(9));
        r
    }

    fn met() -> Diplomacy {
        crate::civs::set_controllers();
        let mut d = Diplomacy::new();
        for a in 0..civ_count() {
            for b in a + 1..civ_count() {
                d.meet(a, b);
            }
        }
        d
    }

    fn unit(civ: usize, x: i32, y: i32) -> Unit {
        Unit::new(civ, UnitType::named("Warrior"), x, y)
    }

    fn city(civ: usize, x: i32, y: i32) -> City {
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
            name: format!("C{civ}"),
            x,
            y,
            diseased: false,
            citizens: crate::citizens::new_pool(civ, 3),
            food: 0,
            shields: 0,
            production: crate::cities::Production::named("Warrior"),
            queue: vec![],
            buildings: vec![],
            culture: 0,
            founded: 1,
        }
    }

    #[test]
    fn nobody_is_at_war_until_it_is_declared() {
        let d = met();
        for a in 0..civ_count() {
            for b in 0..civ_count() {
                assert!(!d.at_war(a, b));
            }
        }
        assert!(d.contact(0, 1) && d.contact(2, 1));
        assert!(!d.contact(1, 1));
        // The embassies came with the meeting.
        assert!(d.rel.embassy(slot(0), slot(1)) && d.rel.embassy(slot(1), slot(0)));
    }

    #[test]
    fn a_meeting_happens_once() {
        let mut d = Diplomacy::new();
        assert!(d.meet(0, 1));
        assert!(!d.meet(1, 0));
        assert_eq!(d.contacts(0), 1 << slot(1));
        assert_eq!(d.contacts(2), 0);
    }

    #[test]
    fn units_meet_within_two_tiles_and_cities_within_three() {
        let map = GameMap::generate_with_seed(1);
        let (a, b, c) = (unit(0, 10, 10), unit(1, 12, 10), unit(2, 14, 10));
        let units = [&a, &b, &c];
        let none = |_: usize, _: usize| false;
        assert_eq!(meetings(&map, &units, &[], none), vec![(0, 1), (1, 2)]);
        // Already met pairs are not reported again.
        assert_eq!(
            meetings(&map, &units, &[], |a, b| (a, b) == (0, 1)),
            vec![(1, 2)]
        );
        // A city is noticed from three tiles, but not by another city.
        let town = city(3, 16, 10);
        let far = city(0, 30, 10);
        let out = meetings(&map, &[&c], &[&town, &far], none);
        assert_eq!(out, vec![(2, 3)]);
        assert!(meetings(&map, &[], &[&town, &far], none).is_empty());
        assert!(meetings(&map, &[&unit(3, 40, 10)], &[&town], none).is_empty());
    }

    #[test]
    fn soldiers_in_foreign_territory_are_counted_by_host() {
        // Civ 1 holds two tiles.
        let owner: HashMap<(i32, i32), usize> =
            [((10, 10), 1usize), ((11, 10), 1)].into_iter().collect();
        let own = unit(1, 11, 10);
        let (s1, s2) = (unit(2, 11, 10), unit(2, 10, 10));
        let worker = Unit::new(2, UnitType::named("Worker"), 10, 10);
        let elsewhere = unit(0, 30, 30);
        let out = trespassers(&owner, &[&own, &s1, &s2, &worker, &elsewhere]);
        assert_eq!(
            out.get(&(1, 2)),
            Some(&2),
            "civ 2's two soldiers are in civ 1's land"
        );
        assert_eq!(
            out.len(),
            1,
            "the owner's own soldiers, workers and the far unit are not intruders"
        );
    }

    #[test]
    fn facts_rank_the_civs_by_score() {
        let map = GameMap::generate_with_seed(1);
        let land = label_land(&map);
        let (c0, c1, c2) = (city(0, 5, 5), city(0, 9, 9), city(1, 20, 20));
        let u = unit(2, 1, 1);
        let f = Facts::new(
            &map,
            &land,
            &[&c0, &c1, &c2],
            &[&u],
            crate::civs::pad(&[3, 3, 3, 3]),
        );
        assert_eq!(f.cities[..4], [2, 1, 0, 0]);
        assert_eq!(f.score[0], 29);
        assert_eq!(f.rank[0], 1);
        assert_eq!(f.rank[1], 2);
        assert_eq!(
            f.rank[2], 3,
            "a lone soldier outscores nobody but the empty"
        );
        assert_eq!(f.rank[3], 4);
    }

    #[test]
    fn declaring_war_is_reported_to_the_human() {
        let mut d = met();
        let mut board = MessageBoard::default();
        d.declare(&Facts::even(), 1, 0, 0, &mut board);
        assert!(d.at_war(0, 1) && d.at_war(1, 0));
        assert_eq!(board.text, "The Romans have declared war on us!");
        d.declare(&Facts::even(), 0, 2, 0, &mut board);
        assert_eq!(board.text, "We have declared war on the Egyptians.");
        d.make_peace(0, 1);
        assert!(!d.at_war(0, 1));
    }

    #[test]
    fn nothing_is_traded_in_a_war_but_peace() {
        let r = research();
        let mut d = met();
        let treasury = [100; CIV_CAP];
        let mut gold = Deal::new(0, 1);
        gold.from_a.push(Clause::Gold(10));
        assert_eq!(d.check(&r, &treasury, &gold), Ok(()));
        d.declare(&Facts::even(), 0, 1, 0, &mut MessageBoard::default());
        assert!(d.check(&r, &treasury, &gold).is_err());
        let mut peace = Deal::new(0, 1);
        peace.from_a.push(Clause::Peace);
        assert_eq!(d.check(&r, &treasury, &peace), Ok(()));
        // Peace needs a war.
        assert!(met().check(&r, &treasury, &peace).is_err());
    }

    #[test]
    fn gold_changes_hands_and_cannot_exceed_the_purse() {
        let mut r = research();
        let mut d = met();
        let mut treasury = [50, 20, 0, 0];
        let mut deal = Deal::new(0, 1);
        deal.from_a.push(Clause::Gold(30));
        deal.from_b.push(Clause::Gold(5));
        assert_eq!(d.check(&r, &treasury, &deal), Ok(()));
        d.execute(
            &Facts::even(),
            &mut r,
            &mut treasury,
            &deal,
            1,
            &mut Rng::new(1),
            &mut MessageBoard::default(),
        );
        assert_eq!(treasury, [25, 45, 0, 0]);
        let mut too_much = Deal::new(0, 1);
        too_much.from_a.push(Clause::Gold(26));
        assert!(d.check(&r, &treasury, &too_much).is_err());
        let mut nothing = Deal::new(0, 1);
        assert!(d.check(&r, &treasury, &nothing).is_err());
        nothing.from_a.push(Clause::Gold(0));
        assert!(d.check(&r, &treasury, &nothing).is_err());
    }

    #[test]
    fn an_advance_trade_teaches_the_taker_and_counts() {
        let mut r = research();
        let mut d = met();
        let t = r
            .giftable(1, 0)
            .first()
            .copied()
            .expect("Rome knows something Japan does not");
        let mut deal = Deal::new(1, 0);
        deal.from_a.push(Clause::Tech(t));
        let mut treasury = [0; CIV_CAP];
        assert_eq!(d.check(&r, &treasury, &deal), Ok(()));
        assert!(!r.knows(0, t));
        let events = d.execute(
            &Facts::even(),
            &mut r,
            &mut treasury,
            &deal,
            1,
            &mut Rng::new(1),
            &mut MessageBoard::default(),
        );
        assert!(r.knows(0, t));
        assert!(!events.is_empty());
        assert_eq!(d.rel.rec(slot(0), slot(1))[rec::TECH_TRADES], 1);
        // What Japan already knows cannot be given again.
        assert!(d.check(&r, &treasury, &deal).is_err());
    }

    #[test]
    fn treaties_need_the_advance_on_both_sides() {
        let mut r = research();
        let d = met();
        let treasury = [0; CIV_CAP];
        let mut deal = Deal::new(0, 1);
        deal.from_a.push(Clause::RightOfPassage);
        // Nobody has Map Making at the start.
        assert!(d.check(&r, &treasury, &deal).is_err());
        let mm = crate::ruleset::TECH_NAMES
            .iter()
            .position(|&n| n == "Map Making")
            .unwrap() as i32;
        r.award(0, mm, &mut Rng::new(1));
        assert!(
            d.check(&r, &treasury, &deal).is_err(),
            "Rome still lacks it"
        );
        r.award(1, mm, &mut Rng::new(1));
        assert_eq!(d.check(&r, &treasury, &deal), Ok(()));
    }

    #[test]
    fn peace_is_free_from_a_friend_and_dear_from_an_enemy() {
        let r = research();
        let mut d = met();
        let f = Facts::even();
        d.declare(&f, 0, 1, 0, &mut MessageBoard::default());
        let mut peace = Deal::new(0, 1);
        peace.from_a.push(Clause::Peace);
        let dice = &mut Rng::new(4);
        // A fresh war: Rome is not winning or losing and sees no reason to
        // hate Japan, so it takes peace for nothing.
        let class = d.attitude(&f, 1, 0);
        let verdict = d.weigh(&f, &r, 1, &peace, dice);
        if class <= 1 {
            assert_eq!(verdict, Verdict::Accept);
        }
        // An old grudge raises the price; gold pays it.
        for _ in 0..3 {
            d.rel.rec_mut(slot(1), slot(0))[rec::HOSTILE_ACTS] += 1;
        }
        assert_eq!(d.attitude(&f, 1, 0), 4);
        assert_eq!(d.weigh(&f, &r, 1, &peace, dice), Verdict::StrongReject);
        let mut paid = peace.clone();
        paid.from_a.push(Clause::Gold(200));
        assert_eq!(d.weigh(&f, &r, 1, &paid, dice), Verdict::Accept);
        // Half the price is not enough.
        let mut cheap = peace.clone();
        cheap.from_a.push(Clause::Gold(10));
        assert_ne!(d.weigh(&f, &r, 1, &cheap, dice), Verdict::Accept);
    }

    #[test]
    fn a_gold_gift_is_accepted_and_a_demand_is_not() {
        let r = research();
        let d = met();
        let f = Facts::even();
        let dice = &mut Rng::new(4);
        let mut gift = Deal::new(0, 1);
        gift.from_a.push(Clause::Gold(10));
        assert_eq!(d.weigh(&f, &r, 1, &gift, dice), Verdict::Accept);
        let mut demand = Deal::new(0, 1);
        demand.from_b.push(Clause::Gold(10));
        assert_eq!(d.weigh(&f, &r, 1, &demand, dice), Verdict::StrongReject);
        // Fair is fair: an even swap of gold goes through, a short one does not.
        let mut swap = Deal::new(0, 1);
        swap.from_a.push(Clause::Gold(10));
        swap.from_b.push(Clause::Gold(10));
        assert_eq!(d.weigh(&f, &r, 1, &swap, dice), Verdict::Accept);
        swap.from_b = vec![Clause::Gold(30)];
        assert_eq!(d.weigh(&f, &r, 1, &swap, dice), Verdict::StrongReject);
    }

    #[test]
    fn a_broken_package_makes_the_computer_ask_five_times_more() {
        let r = research();
        let mut d = met();
        let f = Facts::even();
        let dice = &mut Rng::new(4);
        let mut swap = Deal::new(0, 1);
        swap.from_a.push(Clause::Gold(10));
        swap.from_b.push(Clause::Gold(10));
        assert_eq!(d.weigh(&f, &r, 1, &swap, dice), Verdict::Accept);
        // Japan broke a deal with Rome: T = 1 scales the ask by 5.
        d.rel.rec_mut(slot(1), slot(0))[rec::DEALS_CANCELLED] = 1;
        assert_ne!(d.weigh(&f, &r, 1, &swap, dice), Verdict::Accept);
        swap.from_a = vec![Clause::Gold(50)];
        assert_eq!(d.weigh(&f, &r, 1, &swap, dice), Verdict::Accept);
    }

    #[test]
    fn gold_per_turn_is_paid_for_twenty_turns() {
        let mut r = research();
        let mut d = met();
        let f = Facts::even();
        let mut treasury = [100, 0, 0, 0];
        let mut deal = Deal::new(0, 1);
        deal.from_a.push(Clause::GoldPerTurn(4));
        d.execute(
            &f,
            &mut r,
            &mut treasury,
            &deal,
            10,
            &mut Rng::new(1),
            &mut MessageBoard::default(),
        );
        assert_eq!(d.rel.packages.len(), 1);
        assert_eq!(d.rel.packages[0].ends, 30);
        let mut board = MessageBoard::default();
        for turn in 10..=30 {
            d.run_turn_end(
                &f,
                &r,
                0,
                turn,
                &HashMap::new(),
                &mut treasury,
                &mut Rng::new(1),
                &mut board,
            );
        }
        assert_eq!(treasury, [100 - 4 * 20, 4 * 20, 0, 0]);
        assert!(d.rel.packages.is_empty(), "the deal lapsed");
        // Without gold the payment is whatever is left.
        let mut poor = [3, 0, 0, 0];
        d.execute(
            &f,
            &mut r,
            &mut poor,
            &deal,
            1,
            &mut Rng::new(1),
            &mut board,
        );
        d.run_turn_end(
            &f,
            &r,
            0,
            1,
            &HashMap::new(),
            &mut poor,
            &mut Rng::new(1),
            &mut board,
        );
        assert_eq!(poor, [0, 3, 0, 0]);
    }

    #[test]
    fn soldiers_in_borders_raise_the_warning_then_the_war_track() {
        let r = research();
        let mut d = met();
        let f = Facts::even();
        let mut board = MessageBoard::default();
        let mut treasury = [0; CIV_CAP];
        let mut intruders = HashMap::new();
        intruders.insert((1usize, 0usize), 2);
        // Two soldiers add 0x80 a turn: the warning on the first.
        d.run_turn_end(
            &f,
            &r,
            0,
            1,
            &intruders,
            &mut treasury,
            &mut Rng::new(1),
            &mut board,
        );
        let (rome, japan) = (slot(1), slot(0));
        assert_eq!(d.rel.tension(rome, japan), 0x80);
        assert_ne!(d.rel.rel(rome, japan) & relbit::BORDER_WARNING, 0);
        assert!(board.text.contains("borders of the Romans"));
        for t in 2..4 {
            d.run_turn_end(
                &f,
                &r,
                0,
                t,
                &intruders,
                &mut treasury,
                &mut Rng::new(1),
                &mut board,
            );
        }
        assert_eq!(d.rel.tension(rome, japan), 0x180);
        assert_eq!(d.rel.rel(rome, japan) & relbit::BORDER_WAR, 0);
        // The fourth turn reaches 0x200: the war track, a grievance, and the
        // pressure falls back to half the threshold while the host decides.
        d.run_turn_end(
            &f,
            &r,
            0,
            4,
            &intruders,
            &mut treasury,
            &mut Rng::new(1),
            &mut board,
        );
        assert_ne!(d.rel.rel(rome, japan) & relbit::BORDER_WAR, 0);
        assert_eq!(d.rel.rec(rome, japan)[rec::HOSTILE_ACTS], 1);
        assert_eq!(d.rel.tension(rome, japan), 0x100);
        // Without intruders it fades.
        d.run_turn_end(
            &f,
            &r,
            0,
            5,
            &HashMap::new(),
            &mut treasury,
            &mut Rng::new(1),
            &mut board,
        );
        assert_eq!(d.rel.tension(rome, japan), 0x100 - PRESSURE_DECAY);
    }

    #[test]
    fn a_right_of_passage_lets_soldiers_walk_in() {
        let r = research();
        let mut d = met();
        let f = Facts::even();
        d.rel.apply(&f, slot(1), slot(0), &Clause::RightOfPassage);
        let mut intruders = HashMap::new();
        intruders.insert((1usize, 0usize), 5);
        let mut treasury = [0; CIV_CAP];
        d.run_turn_end(
            &f,
            &r,
            0,
            1,
            &intruders,
            &mut treasury,
            &mut Rng::new(1),
            &mut MessageBoard::default(),
        );
        assert_eq!(d.rel.tension(slot(1), slot(0)), 0);
    }

    /// A weak civ 3 that hates everybody and rolls high.
    fn grudge() -> (Diplomacy, Facts) {
        let mut d = met();
        for q in 0..3 {
            d.rel.rec_mut(slot(3), slot(q))[rec::HOSTILE_ACTS] = 3;
        }
        let mut f = Facts::even();
        f.score = crate::civs::pad(&[5, 50, 20, 1]);
        f.rank = crate::civs::pad(&[3, 1, 2, 4]);
        (d, f)
    }

    /// A die that always shows the same face (or the largest it can).
    struct Always(i32);
    impl Dice for Always {
        fn below(&mut self, n: u32) -> i32 {
            self.0.min(n as i32 - 1)
        }
    }

    #[test]
    fn the_computer_plans_wars_only_on_its_turn_and_never_at_a_partner() {
        let (d, f) = grudge();
        // Before the planning turn: never.
        for turn in 0..WAR_PLAN_TURN {
            assert_eq!(d.war_plan(&f, 3, turn, &mut Always(31)), None);
        }
        // Each civ has one turn in the period.
        let turns: Vec<u32> = (WAR_PLAN_TURN..WAR_PLAN_TURN + 2 * WAR_PLAN_PERIOD)
            .filter(|&t| d.war_plan(&f, 3, t, &mut Always(31)).is_some())
            .collect();
        assert_eq!(
            turns,
            vec![WAR_PLAN_TURN + 3, WAR_PLAN_TURN + 3 + WAR_PLAN_PERIOD]
        );
        // A deal in force protects the partner.
        let (mut d, f) = grudge();
        let turn = WAR_PLAN_TURN + 3;
        assert_eq!(d.war_plan(&f, 3, turn, &mut Always(31)), Some(1));
        d.rel
            .open_package(slot(3), slot(1), &[Clause::Peace], turn as i32);
        assert_eq!(
            d.war_plan(&f, 3, turn, &mut Always(31)),
            Some(2),
            "the next strongest"
        );
        d.rel
            .open_package(slot(3), slot(2), &[Clause::Peace], turn as i32);
        d.rel
            .open_package(slot(3), slot(0), &[Clause::Peace], turn as i32);
        assert_eq!(d.war_plan(&f, 3, turn, &mut Always(31)), None);
    }

    #[test]
    fn a_war_plan_needs_the_die_and_aims_at_the_strongest() {
        let (d, f) = grudge();
        let turn = WAR_PLAN_TURN + 3;
        assert_eq!(d.war_plan(&f, 3, turn, &mut Always(31)), Some(1));
        assert_eq!(d.war_plan(&f, 3, turn, &mut Always(0)), None);
        // The leader, rank 1, needs 31: its own plans rarely come to war.
        let mut d = met();
        d.rel.rec_mut(slot(1), slot(2))[rec::HOSTILE_ACTS] = 3;
        let mut f = f;
        f.score = crate::civs::pad(&[5, 50, 60, 1]);
        f.rank = crate::civs::pad(&[3, 2, 1, 4]);
        assert_eq!(d.war_plan(&f, 2, WAR_PLAN_TURN + 2, &mut Always(31)), None);
    }

    #[test]
    fn the_computer_pitches_a_fair_advance_swap_to_a_friend() {
        let r = research();
        let d = met();
        let f = Facts::even();
        // Each side knows something the other lacks, so a swap exists.
        assert!(
            !r.giftable(1, 0).is_empty() && !r.giftable(0, 1).is_empty(),
            "the free advances differ"
        );
        let p = d
            .pitch(&f, &r, 1, 0, &mut Rng::new(2))
            .expect("a swap to propose");
        assert_eq!((p.from, p.to), (1, 0));
        assert_eq!(d.check(&r, &[0; CIV_CAP], &p.deal), Ok(()));
        assert_eq!(
            d.weigh(&f, &r, 1, &p.deal, &mut Rng::new(2)),
            Verdict::Accept
        );
        // It is a swap: one advance each way.
        assert_eq!((p.deal.from_a.len(), p.deal.from_b.len()), (1, 1));
        // Nobody pitches at an enemy.
        let mut d = met();
        d.declare(&f, 0, 1, 0, &mut MessageBoard::default());
        assert!(d.pitch(&f, &r, 1, 0, &mut Rng::new(2)).is_none());
    }

    #[test]
    fn attitude_words_cover_every_class() {
        let words: Vec<_> = (0..5).map(attitude_label).collect();
        assert_eq!(
            words,
            ["Gracious", "Polite", "Cautious", "Annoyed", "Furious"]
        );
        assert_eq!(verdict_text(Verdict::Accept), "We have a deal.");
        assert_eq!(clause_text(&Clause::Gold(7)), "7 gold");
        assert_eq!(clause_text(&Clause::Embargo(3)), "Embargo of the Egyptians");
        assert_eq!(people(0), "Japanese");
        assert_eq!(people(3), "Chinese");
    }

    #[test]
    fn land_is_labelled_by_continent() {
        let map = GameMap::generate_with_seed(1);
        let land = label_land(&map);
        assert_eq!(land.len(), map.tiles.len());
        for (i, t) in map.tiles.iter().enumerate() {
            assert_eq!(land[i] == 0, crate::improvements::is_water_base(t.base));
        }
        assert!(land.iter().any(|&l| l > 0));
    }

    #[test]
    fn a_civ_that_fell_leaves_the_diplomatic_world() {
        let mut d = Diplomacy::new();
        d.sync_in_play(&[false, false, true, false]);
        assert_eq!(d.rel.in_play >> slot(2) & 1, 0);
        assert_eq!(d.rel.in_play >> slot(1) & 1, 1);
    }

    #[test]
    fn the_wars_of_an_alliance_follow_the_pact() {
        let mut d = met();
        let f = Facts::even();
        let mut board = MessageBoard::default();
        d.rel
            .apply(&f, slot(1), slot(2), &Clause::MilitaryAlliance(slot(3)));
        assert!(d.at_war(1, 3) && d.at_war(2, 3));
        // Peace with civ 3 ends the pact against it.
        d.make_peace(1, 3);
        assert_eq!(d.rel.allies_vs(slot(1), slot(3)), 0);
        let _ = &mut board;
    }
}
