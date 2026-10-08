//! Research: beakers, what a technology costs, acquiring one, eras, the Great
//! Library and choosing a target.
//!
//! What is modelled is the *state machine*: the per-player research fields, the
//! `known[t]` bitmasks and the queue ring. Everything that needs the rest of the
//! game — the science rate, whether the player holds the Great Library, the UI —
//! is left to the caller. Effects that touch other systems (trade-network
//! rebuild, resource recompute, wonder reach, the scientific-leader roll, the
//! barbarian landing) are reported as [`Event`]s.

use std::collections::VecDeque;

use civ3_worldgen::rng::Rng;

/// "No technology" in every prerequisite, queue and target slot.
pub const NONE: i32 = -1;
/// Returned by [`World::choose_research`] for an interactive player who has to
/// pick: the binary opens a modal dialog here.
pub const PENDING: i32 = -2;

/// Era numbers run `0..=3`; the last era never advances.
pub const LAST_ERA: i32 = 3;

/// Slots `0..32`; slot 0 is the barbarians.
pub const SLOTS: usize = 32;

/// `TECH.flags` bits read by this module.
pub mod flags {
    /// Enables Diplomats (Writing).
    pub const DIPLOMATS: u32 = 1 << 0;
    /// Enables Irrigation Without Fresh Water.
    pub const IRRIGATION_WITHOUT_FRESH_WATER: u32 = 1 << 1;
    /// Enables Bridges.
    pub const BRIDGES: u32 = 1 << 2;
    /// Disables Diseases From Flood Plains.
    pub const DISABLE_FLOOD_PLAIN_DISEASE: u32 = 1 << 3;
    /// Enables Conscription.
    pub const CONSCRIPTION: u32 = 1 << 4;
    /// Enables Mobilization Levels.
    pub const MOBILIZATION: u32 = 1 << 5;
    /// Enables Recycling.
    pub const RECYCLING: u32 = 1 << 6;
    /// Enables Precision Bombing.
    pub const PRECISION_BOMBING: u32 = 1 << 7;
    /// Enables Mutual Protection Pacts.
    pub const MPP: u32 = 1 << 8;
    /// Enables Right of Passage treaties.
    pub const RIGHT_OF_PASSAGE: u32 = 1 << 9;
    /// Enables Military Alliances.
    pub const MILITARY_ALLIANCE: u32 = 1 << 10;
    /// Enables Trade Embargoes.
    pub const TRADE_EMBARGO: u32 = 1 << 11;
    /// Doubles the effect of the Wealth improvement.
    pub const DOUBLE_WEALTH: u32 = 1 << 12;
    /// Enables trade over sea tiles.
    pub const TRADE_OVER_SEA: u32 = 1 << 13;
    /// Enables trade over ocean tiles.
    pub const TRADE_OVER_OCEAN: u32 = 1 << 14;
    /// Enables map trading.
    pub const MAP_TRADING: u32 = 1 << 15;
    /// Enables communication trading.
    pub const COMMUNICATION_TRADING: u32 = 1 << 16;
    /// Not required for era advancement.
    pub const NOT_REQUIRED_FOR_ERA: u32 = 1 << 17;
    /// Doubles the work rate of Workers.
    pub const DOUBLE_WORKER_RATE: u32 = 1 << 18;
    /// Cannot be traded.
    pub const CANNOT_BE_TRADED: u32 = 1 << 19;
    /// Permits sacrifices.
    pub const PERMITS_SACRIFICES: u32 = 1 << 20;
    /// Bonus tech: the first discoverer gets another (Philosophy).
    pub const BONUS_TECH: u32 = 1 << 21;
    /// Reveals the map (Satellites).
    pub const REVEAL_MAP: u32 = 1 << 22;
}

/// The gameplay `Random`: `rand(n)` is `0..n`.
pub trait Dice {
    /// A value in `0..n`.
    fn below(&mut self, n: u32) -> i32;
}

impl Dice for Rng {
    fn below(&mut self, n: u32) -> i32 {
        Rng::below(self, n)
    }
}

/// One `TECH` row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TechRow {
    /// `cost`.
    pub cost: i32,
    /// `era`; `-1` is "not in the tree".
    pub era: i32,
    /// The four prerequisites, `-1` = none.
    pub prereq: [i32; 4],
    /// `flags`.
    pub flags: u32,
    /// Flavor mask.
    pub flavors: u32,
}

/// The rule constants of research.
#[derive(Clone, Debug, Default)]
pub struct Rules {
    /// `TECH`, `T` rows.
    pub techs: Vec<TechRow>,
    /// `future_tech_cost`.
    pub future_tech_cost: i32,
    /// `max_research_time`.
    pub max_research_turns: i32,
    /// `min_research_time`.
    pub min_research_turns: i32,
}

impl Rules {
    /// `T`, also the index meaning "a future technology".
    pub fn count(&self) -> i32 {
        self.techs.len() as i32
    }
}

/// A research event the caller has to act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// `known[tech]` gained the player. Recompute what the advance enables.
    Acquired {
        /// The slot.
        player: u32,
        /// The advance.
        tech: i32,
        /// Whether research finished it.
        by_research: bool,
    },
    /// A future technology was completed.
    FutureTech {
        /// The slot.
        player: u32,
    },
    /// The player entered an era.
    EnteredEra {
        /// The slot.
        player: u32,
        /// The new era.
        era: i32,
    },
    /// The player was the first to learn the advance and research completed it.
    FirstDiscoverer {
        /// The slot.
        player: u32,
        /// The advance.
        tech: i32,
    },
    /// The scientific-leader die succeeded.
    ScientificLeader {
        /// The slot.
        player: u32,
        /// The advance.
        tech: i32,
    },
    /// The second civ to reach an era: the barbarian landing triggers.
    BarbarianLanding {
        /// The era two civs now share.
        era: i32,
    },
    /// The advance has the Reveal Map flag.
    RevealMap {
        /// The slot.
        player: u32,
    },
    /// The Great Library gave the advance.
    GreatLibrary {
        /// The slot.
        player: u32,
        /// The advance.
        tech: i32,
        /// The first civ that knew it.
        first: u32,
        /// The last civ that knew it.
        second: u32,
    },
}

/// What the player's slot holds on the research side.
#[derive(Clone, Debug)]
pub struct Player {
    /// The slot number.
    pub slot: u32,
    /// `0..=3`.
    pub era: i32,
    /// Beakers toward the current target.
    pub beakers: i32,
    /// The target, [`NONE`], or `T` for a future technology.
    pub current: i32,
    /// Research turns spent on the target.
    pub turns: i32,
    /// Future technologies acquired.
    pub future: i32,
    /// Advances known.
    pub known_count: i32,
    /// The city count. The step does nothing without cities.
    pub cities: i32,
    /// The research rate `s`, with the Science Age factor already applied.
    pub rate: i32,
    /// Slots with the contact bit set.
    pub contact: u32,
    /// The research queue: the human's plan, the AI's one-element buffer.
    pub queue: VecDeque<i32>,
    /// A human at the keyboard picks the target in a dialog.
    pub interactive: bool,
    /// Holds a Great Library-type wonder.
    pub great_library: bool,
    /// Obsoleting advance of that wonder. `NONE` means it never expires.
    pub great_library_obsolete: i32,
    /// `RACE.freeTech(0..4)`.
    pub free_techs: [i32; 4],
    /// The civilization has the Scientific trait.
    pub scientific: bool,
    /// Science Age last turn, inclusive.
    pub science_until: Option<i32>,
    /// Great Library notice: `(tech, first, second)`.
    pub notice: Option<(i32, u32, u32)>,
    /// Eras finished while nobody was ahead.
    pub achievements: u32,
    /// Set when an interactive player has to choose; cleared by [`World::choose`].
    pub awaiting_choice: bool,
}

impl Player {
    fn write(&self, w: &mut crate::words::Writer) {
        w.put(self.slot);
        for v in [
            self.era,
            self.beakers,
            self.current,
            self.turns,
            self.future,
            self.known_count,
            self.cities,
            self.rate,
        ] {
            w.put(v);
        }
        w.put(self.contact);
        w.run(&self.queue.iter().copied().collect::<Vec<i32>>());
        w.flag(self.interactive);
        w.flag(self.great_library);
        w.put(self.great_library_obsolete);
        w.run(&self.free_techs);
        w.flag(self.scientific);
        w.put(self.science_until.unwrap_or(NONE));
        match self.notice {
            Some((t, a, b)) => {
                w.put(1);
                w.put(t);
                w.put(a);
                w.put(b);
            }
            None => w.put(0),
        }
        w.put(self.achievements);
        w.flag(self.awaiting_choice);
    }

    fn read(r: &mut crate::words::Reader) -> Option<Player> {
        let slot = r.u32()?;
        let mut p = Player::new(slot);
        p.era = r.i32()?;
        p.beakers = r.i32()?;
        p.current = r.i32()?;
        p.turns = r.i32()?;
        p.future = r.i32()?;
        p.known_count = r.i32()?;
        p.cities = r.i32()?;
        p.rate = r.i32()?;
        p.contact = r.u32()?;
        p.queue = r.run(|v| i32::try_from(v).ok())?.into();
        p.interactive = r.flag()?;
        p.great_library = r.flag()?;
        p.great_library_obsolete = r.i32()?;
        p.free_techs = r.run(|v| i32::try_from(v).ok())?.try_into().ok()?;
        p.scientific = r.flag()?;
        let until = r.i32()?;
        p.science_until = (until != NONE).then_some(until);
        p.notice = if r.flag()? {
            Some((r.i32()?, r.u32()?, r.u32()?))
        } else {
            None
        };
        p.achievements = r.u32()?;
        p.awaiting_choice = r.flag()?;
        Some(p)
    }

    /// A fresh slot: no target, era 0, nothing known.
    pub fn new(slot: u32) -> Self {
        Player {
            slot,
            era: 0,
            beakers: 0,
            current: NONE,
            turns: 0,
            future: 0,
            known_count: 0,
            cities: 0,
            rate: 0,
            contact: 0,
            queue: VecDeque::new(),
            interactive: false,
            great_library: false,
            great_library_obsolete: NONE,
            free_techs: [NONE; 4],
            scientific: false,
            science_until: None,
            notice: None,
            achievements: 0,
            awaiting_choice: false,
        }
    }
}

/// The valuation that picks a target.
pub trait Brain {
    /// The best researchable advance for `p`, or `T` when nothing is worth
    /// researching. `mode` is 1 for every automatic caller and 0 for the
    /// interactive suggestion.
    fn default_pick(&mut self, w: &World, p: u32, mode: u8, dice: &mut dyn Dice) -> i32;

    /// The plain valuation of one advance.
    fn value(&mut self, w: &World, p: u32, t: i32, dice: &mut dyn Dice) -> i32;
}

/// All research state of a game.
#[derive(Clone, Debug)]
pub struct World {
    /// The rule constants.
    pub rules: Rules,
    /// Bit `p` of `known[t]` is set when slot `p` knows `t`.
    pub known: Vec<u32>,
    /// Slots in play.
    pub in_play: u32,
    /// Human slots.
    pub human: u32,
    /// The difficulty's cost factor.
    pub difficulty_cost_factor: i32,
    /// The world size's tech rate.
    pub size_tech_rate: i32,
    /// The Accelerated Production game flag.
    pub accelerated: bool,
    /// Enable first-discoverer scientific leaders.
    pub scientific_leaders: bool,
    /// The turn number (0 during setup).
    pub turn: i32,
    /// One entry per slot.
    pub players: Vec<Player>,
}

impl World {
    /// Everything that changes in play, as words (the rules are not saved).
    pub fn to_words(&self) -> Vec<i64> {
        let mut w = crate::words::Writer::default();
        w.run(&self.known);
        w.put(self.in_play);
        w.put(self.human);
        w.put(self.difficulty_cost_factor);
        w.put(self.size_tech_rate);
        w.flag(self.accelerated);
        w.flag(self.scientific_leaders);
        w.put(self.turn);
        w.put(self.players.len() as i64);
        for p in &self.players {
            p.write(&mut w);
        }
        w.0
    }

    /// Put saved state back. False, with `self` untouched, when the words do not
    /// fit this world.
    pub fn restore(&mut self, words: &[i64]) -> bool {
        let mut r = crate::words::Reader::new(words);
        let parsed = (|| {
            let known = r.run(|v| u32::try_from(v).ok())?;
            let (in_play, human) = (r.u32()?, r.u32()?);
            let (cost, rate) = (r.i32()?, r.i32()?);
            let accelerated = r.flag()?;
            let scientific_leaders = r.flag()?;
            let turn = r.i32()?;
            let n = usize::try_from(r.get()?).ok()?;
            let players = (0..n).map(|_| Player::read(&mut r)).collect::<Option<Vec<_>>>()?;
            (r.done() && known.len() == self.known.len() && players.len() == self.players.len()).then_some((
                known,
                in_play,
                human,
                cost,
                rate,
                accelerated,
                scientific_leaders,
                turn,
                players,
            ))
        })();
        let Some((known, in_play, human, cost, rate, accelerated, scientific_leaders, turn, players)) = parsed
        else {
            return false;
        };
        self.known = known;
        self.in_play = in_play;
        self.human = human;
        self.difficulty_cost_factor = cost;
        self.size_tech_rate = rate;
        self.accelerated = accelerated;
        self.scientific_leaders = scientific_leaders;
        self.turn = turn;
        self.players = players;
        true
    }
}

/// C integer division (truncates toward zero), which Rust's `/` is.
fn tdiv(a: i32, b: i32) -> i32 {
    a / b
}

impl World {
    /// A world with `SLOTS` players, nothing known.
    pub fn new(rules: Rules) -> Self {
        let t = rules.techs.len();
        World {
            rules,
            known: vec![0; t],
            in_play: 0,
            human: 0,
            difficulty_cost_factor: 10,
            size_tech_rate: 240,
            accelerated: false,
            scientific_leaders: false,
            turn: 0,
            players: (0..SLOTS as u32).map(Player::new).collect(),
        }
    }

    /// `T`.
    pub fn t(&self) -> i32 {
        self.rules.count()
    }

    /// `knowsTech(P, t)`: `-1` is known, `T` is not.
    pub fn knows(&self, p: u32, t: i32) -> bool {
        if t == NONE {
            return true;
        }
        if t < 0 || t >= self.t() {
            return false;
        }
        self.known[t as usize] >> p & 1 != 0
    }

    fn row(&self, t: i32) -> Option<&TechRow> {
        usize::try_from(t).ok().and_then(|i| self.rules.techs.get(i))
    }

    /// `canResearch(P, t)`.
    pub fn can_research(&self, p: u32, t: i32) -> bool {
        let Some(row) = self.row(t) else {
            return false;
        };
        if self.knows(p, t) {
            return false;
        }
        let era = self.players[p as usize].era;
        if row.era == NONE || row.era > era {
            return false;
        }
        row.prereq.iter().all(|&q| self.knows(p, q))
    }

    /// `reachable(P, t)`: can `t` be researched eventually?
    pub fn reachable(&self, p: u32, t: i32) -> bool {
        if t == NONE {
            return true;
        }
        if self.knows(p, t) {
            return true;
        }
        match self.row(t) {
            None => false,
            Some(row) => row.era != NONE && row.prereq.iter().all(|&q| self.reachable(p, q)),
        }
    }

    /// `techDepth(P, t, d)`: the longest prerequisite chain.
    pub fn tech_depth(&self, t: i32, d: i32) -> i32 {
        match self.row(t) {
            None => d,
            Some(row) => row.prereq.iter().map(|&q| self.tech_depth(q, d + 1)).max().unwrap_or(d),
        }
    }

    /// `knowsTechWithFlags(P, mask)`.
    pub fn knows_with_flags(&self, p: u32, mask: u32) -> bool {
        (0..self.t()).any(|t| self.rules.techs[t as usize].flags & mask == mask && self.knows(p, t))
    }

    fn civs_in_play(&self) -> i32 {
        self.in_play.count_ones() as i32
    }

    /// `Player::baseCost(P; t, neutral)`. `neutral` prices any player as a
    /// human. Panics (divide by zero) with one civ in play, as the binary
    /// faults.
    pub fn base_cost(&self, p: u32, t: i32, neutral: bool) -> i32 {
        let human = self.human >> p & 1 != 0;
        let cf = self.difficulty_cost_factor;
        let mut a = if human || neutral { 10 } else { cf };
        if self.accelerated {
            a = tdiv(a, 2);
        }
        let r = a.clamp(1, 10);

        let x = if t == self.t() {
            self.rules.future_tech_cost * r
        } else {
            let n = self.civs_in_play();
            let t1 = tdiv(7 * n - 7, 4);
            let count = (1..SLOTS as u32)
                .filter(|&q| {
                    self.in_play >> q & 1 != 0
                        && self.players[p as usize].contact >> q & 1 != 0
                        && (t == NONE || self.known[t as usize] >> q & 1 != 0)
                })
                .count() as i32;
            let cost = self.rules.techs[t as usize].cost;
            tdiv((t1 - count) * (cost * r), t1)
        };
        let base = tdiv(self.size_tech_rate * x, 10 * cf.min(10));
        base.max(1)
    }

    /// `Player::effectiveCost(P; t, neutral)`: the beaker total the step
    /// compares against.
    pub fn effective_cost(&self, p: u32, t: i32, neutral: bool) -> i32 {
        if t == NONE {
            return 0;
        }
        let me = &self.players[p as usize];
        let base = self.base_cost(p, t, neutral);
        let s = me.rate;
        let (min_t, max_t) = (self.rules.min_research_turns, self.rules.max_research_turns);
        let (b, k) = (me.beakers, me.turns);
        if t == me.current {
            if s == 0 {
                return b + (max_t - k);
            }
            let hi = (max_t - k).max(0) * s;
            let lo = (min_t - k).max(0) * s;
            let need = (base - b).max(0);
            let c = if hi < lo || need < lo { lo } else { need.min(hi) };
            b + c
        } else {
            let lo = if s == 0 { min_t } else { min_t * s };
            let hi = if s == 0 { max_t } else { max_t * s };
            if hi < lo || base < lo {
                lo
            } else if base > hi {
                hi
            } else {
                base
            }
        }
    }

    /// `turnsLeft(P; t, remainingOnly)`: `9999` when the rate is zero.
    pub fn turns_left(&self, p: u32, t: i32, remaining_only: bool) -> i32 {
        if t == NONE {
            return 0;
        }
        let me = &self.players[p as usize];
        let s = me.rate;
        if s == 0 {
            return 9999;
        }
        let mut c = self.effective_cost(p, t, false);
        if remaining_only && t == me.current {
            c -= me.beakers;
        }
        if c <= 0 {
            return 1;
        }
        let mut n = tdiv(c, s);
        if n * s < c {
            n += 1;
        }
        n.max(1)
    }

    /// The research queue with unusable entries dropped from the front.
    pub fn clean_queue(&mut self, p: u32) {
        loop {
            let Some(&v) = self.players[p as usize].queue.front() else {
                return;
            };
            if self.queue_entry_valid(p, v) {
                return;
            }
            self.players[p as usize].queue.pop_front();
        }
    }

    fn queue_entry_valid(&self, p: u32, v: i32) -> bool {
        let era = self.players[p as usize].era;
        match self.row(v) {
            None => false,
            Some(row) => {
                !self.knows(p, v)
                    && row.era != NONE
                    && row.era <= era
                    && row.prereq.iter().all(|&q| self.knows(p, q))
            }
        }
    }

    /// `chooseResearch(P; byResearch)`. Returns the target, or [`PENDING`] when
    /// an interactive player still has to choose.
    pub fn choose_research(
        &mut self,
        p: u32,
        by_research: bool,
        choice: Option<i32>,
        brain: &mut dyn Brain,
        dice: &mut dyn Dice,
    ) -> i32 {
        let t = self.t();
        if !self.players[p as usize].interactive {
            self.clean_queue(p);
            if self.players[p as usize].queue.is_empty() {
                let d = brain.default_pick(self, p, 1, dice);
                self.players[p as usize].queue.push_back(d);
            }
            return *self.players[p as usize].queue.front().unwrap();
        }
        if by_research {
            self.players[p as usize].queue.pop_front();
        }
        self.clean_queue(p);
        let d = match self.players[p as usize].queue.front() {
            Some(&f) => f,
            None => brain.default_pick(self, p, 0, dice),
        };
        if d == t || d == NONE {
            self.players[p as usize].queue.push_back(t);
            return *self.players[p as usize].queue.front().unwrap();
        }
        let Some(c) = choice else {
            self.players[p as usize].awaiting_choice = true;
            return PENDING;
        };
        let q = &mut self.players[p as usize].queue;
        match q.front() {
            Some(&f) if f == c => {}
            Some(_) => {
                q.clear();
                q.push_back(c);
            }
            None => q.push_back(c),
        }
        *q.front().unwrap()
    }

    /// An interactive player answers the research dialog. The target changes
    /// (and the beakers reset) only when it is a different advance.
    pub fn choose(&mut self, p: u32, tech: i32, brain: &mut dyn Brain, dice: &mut dyn Dice) {
        self.players[p as usize].awaiting_choice = false;
        let nt = self.choose_research(p, false, Some(tech), brain, dice);
        self.set_target(p, nt);
    }

    fn set_target(&mut self, p: u32, nt: i32) {
        if nt == PENDING {
            return;
        }
        let me = &mut self.players[p as usize];
        if nt != me.current {
            me.current = nt;
            me.beakers = 0;
            me.turns = 0;
        }
    }

    /// `randomResearchable(P, onlyCurrentEra)`.
    pub fn random_researchable(&self, p: u32, only_current_era: bool, dice: &mut dyn Dice) -> i32 {
        let era = self.players[p as usize].era;
        let list: Vec<i32> = (0..self.t())
            .filter(|&t| {
                let row = &self.rules.techs[t as usize];
                !self.knows(p, t)
                    && row.era != NONE
                    && row.era <= era
                    && row.prereq.iter().all(|&q| self.knows(p, q))
                    && (!only_current_era || row.era == era)
            })
            .collect();
        if list.is_empty() {
            return NONE;
        }
        list[dice.below(list.len() as u32) as usize]
    }

    /// The advance case of the goody hut: the advance `p` is given, or [`NONE`]
    /// when the hut falls back to its default outcome.
    pub fn hut_advance(&self, p: u32, brain: &mut dyn Brain, dice: &mut dyn Dice) -> i32 {
        let me = &self.players[p as usize];
        if me.era > 0 {
            return NONE;
        }
        let (mut current, mut best, mut best_score) = (NONE, NONE, i32::MAX);
        for t in 0..self.t() {
            let row = &self.rules.techs[t as usize];
            if self.knows(p, t) || row.era != 0 || !row.prereq.iter().all(|&q| q == NONE || self.knows(p, q)) {
                continue;
            }
            if t == me.current {
                current = t;
                continue;
            }
            let depth = row.prereq.iter().map(|&q| self.tech_depth(q, 1)).max().unwrap_or(1);
            if depth > 4 {
                continue;
            }
            let noise = dice.below(100) & 0xFFFF;
            let score = brain.value(self, p, t, dice) + noise;
            if score < best_score {
                best = t;
                best_score = score;
            }
        }
        let t = if best != NONE { best } else { current };
        if t != NONE {
            dice.below(15);
        }
        t
    }

    /// `tryAdvanceEra(P)`: true when the player moved to the next era.
    pub fn try_advance_era(&mut self, p: u32) -> bool {
        let era = self.players[p as usize].era;
        for t in 0..self.t() {
            let row = &self.rules.techs[t as usize];
            if row.era == NONE
                || row.era > era
                || row.flags & flags::NOT_REQUIRED_FOR_ERA != 0
                || self.knows(p, t)
            {
                continue;
            }
            if row.prereq.iter().all(|&q| self.reachable(p, q)) {
                return false;
            }
        }
        let higher = (0..SLOTS as u32)
            .any(|q| self.in_play >> q & 1 != 0 && self.players[q as usize].era > era);
        if !higher {
            self.players[p as usize].achievements |= 1 << era;
        }
        if era >= LAST_ERA {
            return false;
        }
        self.players[p as usize].era += 1;
        true
    }

    /// `enterEra(P)`, called after [`Self::try_advance_era`] returned true.
    pub fn enter_era(&mut self, p: u32, events: &mut Vec<Event>, ctx: &mut Ctx<'_>) {
        let era = self.players[p as usize].era;
        events.push(Event::EnteredEra { player: p, era });
        for k in 0..4 {
            let t = self.players[p as usize].free_techs[k];
            if t > NONE && self.row(t).is_some_and(|r| r.era == era) {
                self.acquire(p, t, false, true, false, events, ctx);
            }
        }
        if self.players[p as usize].scientific {
            let t = self.random_researchable(p, true, ctx.dice);
            if t != NONE {
                self.acquire(p, t, false, true, false, events, ctx);
            }
        }
        if self.turn > 0 {
            let n = (1..SLOTS as u32)
                .filter(|&q| self.in_play >> q & 1 != 0 && self.players[q as usize].era == era)
                .count();
            if n == 2 {
                events.push(Event::BarbarianLanding { era });
            }
        }
    }

    /// `Player::acquire(P; t, byResearch, silent, runEras)`.
    #[allow(clippy::too_many_arguments)]
    pub fn acquire(
        &mut self,
        p: u32,
        t: i32,
        by_research: bool,
        silent: bool,
        run_eras: bool,
        events: &mut Vec<Event>,
        ctx: &mut Ctx<'_>,
    ) {
        let total = self.t();
        if t == total {
            let me = &mut self.players[p as usize];
            me.beakers = 0;
            me.turns = 0;
            me.future += 1;
            events.push(Event::FutureTech { player: p });
            return;
        }
        if t == NONE || t < 0 || t > total || self.knows(p, t) {
            return;
        }
        self.known[t as usize] |= 1 << p;
        self.players[p as usize].known_count += 1;
        events.push(Event::Acquired { player: p, tech: t, by_research });
        let flags = self.rules.techs[t as usize].flags;
        let first = !(1..SLOTS as u32).any(|q| q != p && self.known[t as usize] >> q & 1 != 0);
        let bonus = first && flags & flags::BONUS_TECH != 0;
        if first && by_research && self.turn > 0 {
            events.push(Event::FirstDiscoverer { player: p, tech: t });
            if self.scientific_leaders {
                let chance = if self.players[p as usize].scientific { 5 } else { 3 };
                if ctx.dice.below(100) < chance {
                    events.push(Event::ScientificLeader { player: p, tech: t });
                }
            }
        }
        if run_eras {
            while self.try_advance_era(p) {
                self.enter_era(p, events, ctx);
            }
        }
        if self.players[p as usize].current == t {
            let nt = self.choose_research(p, by_research, None, ctx.brain, ctx.dice);
            self.set_target(p, nt);
        }
        if flags & flags::REVEAL_MAP != 0 {
            events.push(Event::RevealMap { player: p });
        }
        let _ = silent;
        if bonus {
            let mut u = self.players[p as usize].current;
            if u == NONE {
                u = ctx.brain.default_pick(self, p, 1, ctx.dice);
            }
            if u != NONE && u != PENDING {
                self.acquire(p, u, true, true, true, events, ctx);
            }
        }
    }

    /// `greatLibrary(P)`: every advance known by two civs that have contact with
    /// `p` is given to the holder.
    pub fn great_library(&mut self, p: u32, events: &mut Vec<Event>, ctx: &mut Ctx<'_>) {
        if !self.players[p as usize].great_library {
            return;
        }
        let obsolete = self.players[p as usize].great_library_obsolete;
        if obsolete >= 0 && self.knows(p, obsolete) {
            return;
        }
        for t in 0..self.t() {
            if self.knows(p, t) {
                continue;
            }
            let row = &self.rules.techs[t as usize];
            let era = self.players[p as usize].era;
            if row.era > era || row.era == NONE {
                continue;
            }
            if !row.prereq.iter().all(|&q| self.knows(p, q)) {
                continue;
            }
            let (mut n, mut first, mut second) = (0, NONE, NONE);
            for q in 1..SLOTS as u32 {
                if self.in_play >> q & 1 != 0
                    && self.players[q as usize].contact >> p & 1 != 0
                    && self.known[t as usize] >> q & 1 != 0
                {
                    n += 1;
                    if first == NONE {
                        first = q as i32;
                    } else {
                        second = q as i32;
                    }
                }
            }
            if n < 2 {
                continue;
            }
            let current = self.players[p as usize].current == t;
            if current {
                self.players[p as usize].notice = Some((t, first as u32, second as u32));
            }
            events.push(Event::GreatLibrary {
                player: p,
                tech: t,
                first: first as u32,
                second: second as u32,
            });
            self.acquire(p, t, false, current, true, events, ctx);
        }
    }

    /// The research step: once per round per player, after the income pass has
    /// added the cities' beakers.
    pub fn step(&mut self, p: u32, events: &mut Vec<Event>, ctx: &mut Ctx<'_>) {
        if self.players[p as usize].cities == 0 {
            return;
        }
        let cur = self.players[p as usize].current;
        if cur != NONE {
            let s = self.players[p as usize].rate;
            if s > 0 {
                self.players[p as usize].turns += 1;
                let need = self.effective_cost(p, cur, false);
                if self.players[p as usize].beakers >= need {
                    let mut t = self.players[p as usize].current;
                    if t == NONE {
                        t = ctx.brain.default_pick(self, p, 1, ctx.dice);
                    }
                    if t != NONE {
                        self.acquire(p, t, true, true, true, events, ctx);
                    }
                }
            }
        }
        self.great_library(p, events, ctx);
        let me = &self.players[p as usize];
        if me.beakers >= 0 && me.current == NONE && !me.awaiting_choice {
            let nt = self.choose_research(p, false, None, ctx.brain, ctx.dice);
            self.set_target(p, nt);
        }
    }

    /// The income pass for one player: add `beakers` from the cities.
    pub fn add_beakers(&mut self, p: u32, beakers: i32) {
        self.players[p as usize].beakers += beakers;
    }

    /// `Player::init` grants: the free era-0 advances, then the era loop.
    pub fn start_grants(&mut self, p: u32, events: &mut Vec<Event>, ctx: &mut Ctx<'_>) {
        for k in 0..4 {
            let t = self.players[p as usize].free_techs[k];
            if t > NONE && !self.knows(p, t) {
                let era = self.row(t).map(|r| r.era);
                if era == Some(0) || era == Some(NONE) {
                    self.acquire(p, t, false, true, false, events, ctx);
                }
            }
        }
        while self.try_advance_era(p) {
            self.enter_era(p, events, ctx);
        }
    }

    /// The advances a scenario or a saved game gives `p` at the start, whatever
    /// their era, then the eras they add up to.
    pub fn grant_known(&mut self, p: u32, techs: &[i32], events: &mut Vec<Event>, ctx: &mut Ctx<'_>) {
        for &t in techs {
            if t > NONE && t < self.t() && !self.knows(p, t) {
                self.acquire(p, t, false, true, false, events, ctx);
            }
        }
        while self.try_advance_era(p) {
            self.enter_era(p, events, ctx);
        }
    }
}

/// The callers' services for research code that picks or rolls.
pub struct Ctx<'a> {
    /// The research picker.
    pub brain: &'a mut dyn Brain,
    /// The gameplay RNG.
    pub dice: &'a mut dyn Dice,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(n: usize) -> Rules {
        Rules {
            techs: (0..n)
                .map(|i| TechRow {
                    cost: 10 * (i as i32 + 1),
                    era: 0,
                    prereq: if i == 0 { [NONE; 4] } else { [i as i32 - 1, NONE, NONE, NONE] },
                    flags: 0,
                    flavors: 0,
                })
                .collect(),
            future_tech_cost: 100,
            max_research_turns: 40,
            min_research_turns: 4,
        }
    }

    struct Plain;
    impl Brain for Plain {
        fn default_pick(&mut self, w: &World, p: u32, _mode: u8, _dice: &mut dyn Dice) -> i32 {
            (0..w.t()).find(|&t| w.can_research(p, t)).unwrap_or_else(|| w.t())
        }
        fn value(&mut self, _w: &World, _p: u32, _t: i32, _dice: &mut dyn Dice) -> i32 {
            0
        }
    }

    fn ctx<'a>(brain: &'a mut Plain, dice: &'a mut Rng) -> Ctx<'a> {
        Ctx { brain, dice }
    }

    #[test]
    fn knows_and_can_research_follow_the_prerequisites() {
        let mut w = World::new(rules(4));
        w.in_play = 1 | 1 << 1;
        w.human = 1;
        assert!(w.can_research(0, 0));
        assert!(!w.can_research(0, 1), "needs tech 0 first");
        w.known[0] |= 1; // player 0 knows tech 0
        assert!(w.knows(0, 0));
        assert!(w.can_research(0, 1));
        assert!(!w.can_research(0, 4), "T is future, not researchable as a row");
    }

    #[test]
    fn cost_rises_with_tech_and_falls_with_knowers() {
        let mut w = World::new(rules(4));
        w.in_play = 0b11;
        w.human = 0b11;
        let c0 = w.base_cost(0, 0, false);
        let c3 = w.base_cost(0, 3, false);
        assert!(c3 > c0, "later techs cost more");
        // Another civ knowing the tech makes it cheaper.
        w.known[0] |= 1 << 1;
        assert!(w.base_cost(0, 0, false) <= c0);
    }

    #[test]
    fn turns_left_is_nine_nines_without_a_rate() {
        let mut w = World::new(rules(4));
        w.in_play = 0b11;
        w.players[0].rate = 0;
        assert_eq!(w.turns_left(0, 0, false), 9999);
        w.players[0].rate = 10;
        assert!(w.turns_left(0, 0, false) >= 1);
    }

    #[test]
    fn an_ai_picks_and_stores_a_target() {
        let mut w = World::new(rules(4));
        w.in_play = 0b11;
        w.players[0].cities = 1;
        let mut brain = Plain;
        let mut dice = Rng::new(1);
        let c = ctx(&mut brain, &mut dice);
        let t = w.choose_research(0, false, None, c.brain, c.dice);
        assert_eq!(t, 0);
        assert_eq!(w.players[0].queue.front(), Some(&0));
    }

    #[test]
    fn acquire_marks_known_and_emits_an_event() {
        let mut w = World::new(rules(2));
        w.in_play = 1;
        let mut brain = Plain;
        let mut dice = Rng::new(1);
        let mut events = Vec::new();
        let mut c = ctx(&mut brain, &mut dice);
        w.acquire(0, 0, false, true, true, &mut events, &mut c);
        assert!(w.knows(0, 0));
        assert!(events.contains(&Event::Acquired { player: 0, tech: 0, by_research: false }));
    }
}
