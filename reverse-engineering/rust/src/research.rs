//! Research: beakers, what a technology costs, acquiring one, eras, the Great
//! Library and choosing a target.
//!
//! Reference port of `../research.md` (all addresses are `Civ3Conquests.exe`).
//! The AI's valuation of a technology (`0x448BF0`) is in [`crate::research_ai`];
//! the pick that uses it is [`Brain::default_pick`].
//!
//! What is modelled is the *state machine*: the per-player fields at
//! `Player+0xF4..+0x104`, the `known[t]` bitmasks at `[0xA52B4C]` and the
//! queue ring at `Player+0x18C4..`. What is left to the caller is everything
//! that needs the rest of the game: the science rate `s` (the sum of the city
//! streams, [`Player::rate`]), whether the player holds the Great Library
//! ([`Player::great_library`]), and the UI. Effects that only touch other
//! systems (trade-network rebuild, resource recompute, great-wonder reach
//! set, the scientific-leader roll, the barbarian landing on a second civ's
//! era) are reported as [`Event`]s instead of being carried out.

use std::collections::VecDeque;

use crate::rng::Rng;

/// "No technology" in every prerequisite, queue and target slot.
pub const NONE: i32 = -1;
/// Returned by [`World::choose_research`] for an interactive player who has
/// to pick: the binary opens a modal dialog here (`0x5625D0`, 9.1).
pub const PENDING: i32 = -2;

/// Era numbers run `0..=3`; the last era never advances (`0x561690`).
pub const LAST_ERA: i32 = 3;

/// Slots `0..32`; slot 0 is the barbarians (`primitives.md`).
pub const SLOTS: usize = 32;

/// `TECH.flags` bits read by this module (`biq::sections::tech::flags` names
/// them all).
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
    /// Not required for era advancement (`0x5616E1`).
    pub const NOT_REQUIRED_FOR_ERA: u32 = 1 << 17;
    /// Doubles the work rate of Workers.
    pub const DOUBLE_WORKER_RATE: u32 = 1 << 18;
    /// Cannot be traded (`0x437FA1`, `0x4494DB`).
    pub const CANNOT_BE_TRADED: u32 = 1 << 19;
    /// Permits sacrifices.
    pub const PERMITS_SACRIFICES: u32 = 1 << 20;
    /// Bonus tech: the first discoverer gets another (Philosophy).
    pub const BONUS_TECH: u32 = 1 << 21;
    /// Reveals the map (Satellites).
    pub const REVEAL_MAP: u32 = 1 << 22;
}

/// The gameplay `Random` (`0xA526B4`, `0x60BAB0`): `rand(n)` is `0..n`.
pub trait Dice {
    /// A value in `0..n`.
    fn below(&mut self, n: u32) -> i32;
}

impl Dice for Rng {
    fn below(&mut self, n: u32) -> i32 {
        Rng::below(self, n)
    }
}

/// One `TECH` row (stride `0x74`, `research.md` 1.1).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TechRow {
    /// `cost` (`+0x44`).
    pub cost: i32,
    /// `era` (`+0x48`); `-1` is "not in the tree".
    pub era: i32,
    /// The four prerequisites (`+0x58..+0x64`), `-1` = none.
    pub prereq: [i32; 4],
    /// `flags` (`+0x68`).
    pub flags: u32,
    /// Flavor mask (`+0x6C`).
    pub flavors: u32,
}

/// The rule constants of 1.4.
#[derive(Clone, Debug, Default)]
pub struct Rules {
    /// `TECH`, `T` rows.
    pub techs: Vec<TechRow>,
    /// `future_tech_cost` (`[0x9C7304]`).
    pub future_tech_cost: i32,
    /// `max_research_time` (`[0x9C730C]`).
    pub max_research_turns: i32,
    /// `min_research_time` (`[0x9C7310]`).
    pub min_research_turns: i32,
}

impl Rules {
    /// `T`, also the index meaning "a future technology".
    pub fn count(&self) -> i32 {
        self.techs.len() as i32
    }
}

/// A research event the caller has to act on (the parts of `acquire` and the
/// era code that belong to other systems).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// `known[tech]` gained the player (`acquire` step 2). Recompute what
    /// the advance enables (steps 4, 5, 7, 8).
    Acquired {
        /// The slot.
        player: u32,
        /// The advance.
        tech: i32,
        /// Whether research finished it (`byResearch`).
        by_research: bool,
    },
    /// A future technology was completed (`acquire` step 1).
    FutureTech {
        /// The slot.
        player: u32,
    },
    /// The player entered era `era` (`enterEra`, 7.2).
    EnteredEra {
        /// The slot.
        player: u32,
        /// The new era.
        era: i32,
    },
    /// The player was the first to learn the advance and research completed
    /// it, so the scientific-leader roll (step 9) applies when the game rule
    /// `0x40000` is on: `5%` for a Scientific civilization, else `3%`.
    FirstDiscoverer {
        /// The slot.
        player: u32,
        /// The advance.
        tech: i32,
    },
    /// The scientific-leader die succeeded. The caller creates a Leader
    /// at the capital if that city exists (`research.md` 10.2).
    ScientificLeader { player: u32, tech: i32 },
    /// The second civ to reach an era (`enterEra`, `turn > 0`): the barbarian
    /// landing `0x55FD00` triggers.
    BarbarianLanding {
        /// The era two civs now share.
        era: i32,
    },
    /// The advance has the Reveal Map flag (step 15).
    RevealMap {
        /// The slot.
        player: u32,
    },
    /// The Great Library gave the advance (8).
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

/// What the player's slot holds on the research side (`research.md` 1.3).
#[derive(Clone, Debug)]
pub struct Player {
    /// `+0x1C`.
    pub slot: u32,
    /// `+0xF4`, `0..=3`.
    pub era: i32,
    /// `+0xF8`: beakers toward the current target.
    pub beakers: i32,
    /// `+0xFC`: the target, [`NONE`], or `T` for a future technology.
    pub current: i32,
    /// `+0x100`: research turns spent on the target.
    pub turns: i32,
    /// `+0x104`: future technologies acquired.
    pub future: i32,
    /// `+0x1A0`: advances known.
    pub known_count: i32,
    /// `+0x194`: the city count. The step does nothing without cities.
    pub cities: i32,
    /// The research rate `s` of 3.3, with the Science Age factor already
    /// applied (the caller sums the city streams).
    pub rate: i32,
    /// Slots with the contact bit (`+0xEB0 + 4q`, bit 0) set.
    pub contact: u32,
    /// The research queue (`+0x18C4..`): the human's plan, the AI's
    /// one-element buffer.
    pub queue: VecDeque<i32>,
    /// A human at the keyboard picks the target in a dialog (9.1).
    pub interactive: bool,
    /// Holds a Great Library-type wonder (`0x55A8D0(P; 2, 0) > 0`).
    pub great_library: bool,
    /// Obsoleting advance of that wonder, checked after research completes
    /// and before its grant pass. `NONE` means it never expires.
    pub great_library_obsolete: i32,
    /// `RACE.freeTech(0..4)` (vtable `+0x1C`).
    pub free_techs: [i32; 4],
    /// The civilization has the Scientific trait (`hasTrait(3)`).
    pub scientific: bool,
    /// Science Age last turn, inclusive (`research.md` 11).
    pub science_until: Option<i32>,
    /// Great Library notice (`+0xAC`): `(tech, first, second)`.
    pub notice: Option<(i32, u32, u32)>,
    /// `PALV +0x1C`: eras finished while nobody was ahead (7.4).
    pub achievements: u32,
    /// Set when an interactive player has to choose; cleared by
    /// [`World::choose`].
    pub awaiting_choice: bool,
}

impl Player {
    fn write(&self, w: &mut crate::words::Writer) {
        w.put(self.slot);
        for v in [self.era, self.beakers, self.current, self.turns, self.future, self.known_count, self.cities, self.rate] {
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
        p.notice = if r.flag()? { Some((r.i32()?, r.u32()?, r.u32()?)) } else { None };
        p.achievements = r.u32()?;
        p.awaiting_choice = r.flag()?;
        Some(p)
    }

    /// A fresh slot (`Player::init`, 7.5): no target, era 0, nothing known.
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

/// The valuation that picks a target: vtable `+0x58` of the Player class.
pub trait Brain {
    /// `defaultPick(P; mode)` (`0x449530`): the best researchable advance
    /// for `p`, or `T` when nothing is worth researching. `mode` is 1 for
    /// every automatic caller and 0 for the interactive suggestion.
    fn default_pick(&mut self, w: &World, p: u32, mode: u8, dice: &mut dyn Dice) -> i32;

    /// vtable `+0x54` as the goody hut calls it: `value(p; t, 0, 0)`
    /// (`0x448BF0`), the plain valuation of one advance.
    fn value(&mut self, w: &World, p: u32, t: i32, dice: &mut dyn Dice) -> i32;
}

/// All research state of a game.
#[derive(Clone, Debug)]
pub struct World {
    /// The rule constants.
    pub rules: Rules,
    /// `[0xA52B4C]`: bit `p` of `known[t]` is set when slot `p` knows `t`.
    pub known: Vec<u32>,
    /// `[0xA526C0]`: slots in play.
    pub in_play: u32,
    /// `[0xA526BC]`: human slots.
    pub human: u32,
    /// `DIFF[[0xA52684]].cost_factor`.
    pub difficulty_cost_factor: i32,
    /// `WSIZ[[0x9C73A0]].tech_rate`.
    pub size_tech_rate: i32,
    /// Game flag `0x200` (Accelerated Production).
    pub accelerated: bool,
    /// Game flag `0x40000`: enable first-discoverer scientific leaders.
    pub scientific_leaders: bool,
    /// `[0xA526AC]`: the turn number (0 during setup).
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

    /// Put saved state back. False, with `self` untouched, when the words do
    /// not fit this world.
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
            (r.done() && known.len() == self.known.len() && players.len() == self.players.len())
                .then_some((known, in_play, human, cost, rate, accelerated, scientific_leaders, turn, players))
        })();
        let Some((known, in_play, human, cost, rate, accelerated, scientific_leaders, turn, players)) = parsed else { return false };
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

/// `tdiv`: C integer division (truncates toward zero), which Rust's `/` is.
fn tdiv(a: i32, b: i32) -> i32 {
    a / b
}

impl World {
    /// A world with `slots` players, nothing known.
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

    /// `knowsTech(P, t)` (`0x561440`): `-1` is known, `T` is not.
    pub fn knows(&self, p: u32, t: i32) -> bool {
        if t == NONE {
            return true;
        }
        if t < 0 || t >= self.t() {
            return false;
        }
        self.known[t as usize] >> p & 1 != 0
    }

    /// `t` is a real row.
    fn row(&self, t: i32) -> Option<&TechRow> {
        usize::try_from(t).ok().and_then(|i| self.rules.techs.get(i))
    }

    /// `canResearch(P, t)` (`0x561580`).
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

    /// `reachable(P, t)` (`0x5614E0`): can `t` be researched eventually?
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

    /// `techDepth(P, t, d)` (`0x561620`): the longest prerequisite chain.
    pub fn tech_depth(&self, t: i32, d: i32) -> i32 {
        match self.row(t) {
            None => d,
            Some(row) => row
                .prereq
                .iter()
                .map(|&q| self.tech_depth(q, d + 1))
                .max()
                .unwrap_or(d),
        }
    }

    /// `knowsTechWithFlags(P, mask)` (`0x561480`).
    pub fn knows_with_flags(&self, p: u32, mask: u32) -> bool {
        (0..self.t()).any(|t| {
            self.rules.techs[t as usize].flags & mask == mask && self.knows(p, t)
        })
    }

    /// Civs in play (`popcount [0xA526C0]`, `0x5DF900`).
    fn civs_in_play(&self) -> i32 {
        self.in_play.count_ones() as i32
    }

    /// `Player::baseCost(P; t, neutral)` (`0x569C10`, 4.1). `neutral` prices
    /// any player as a human. Panics (divide by zero) with one civ in play,
    /// as the binary faults.
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

    /// `Player::effectiveCost(P; t, neutral)` (`0x569E80`, 4.2): the beaker
    /// total the step compares against.
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

    /// `turnsLeft(P; t, remainingOnly)` (`0x566140`, 9.4): `9999` when the
    /// rate is zero.
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

    /// The research queue with unusable entries dropped from the front
    /// (`cleanQueue`, 9.1).
    pub fn clean_queue(&mut self, p: u32) {
        loop {
            let Some(&v) = self.players[p as usize].queue.front() else {
                return;
            };
            let keep = self.queue_entry_valid(p, v);
            if keep {
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

    /// `chooseResearch(P; byResearch)` (`0x5625D0`, 9.1). `choice` is the
    /// dialog's answer for an interactive player. Returns the target, or
    /// [`PENDING`] when an interactive player still has to choose.
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
        // Interactive.
        if by_research {
            // The finished front entry is dropped. The binary does this
            // unguarded; an empty queue is left empty here.
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

    /// An interactive player answers the research dialog. The target
    /// changes (and the beakers reset) only when it is a different advance.
    pub fn choose(
        &mut self,
        p: u32,
        tech: i32,
        brain: &mut dyn Brain,
        dice: &mut dyn Dice,
    ) {
        self.players[p as usize].awaiting_choice = false;
        let nt = self.choose_research(p, false, Some(tech), brain, dice);
        self.set_target(p, nt);
    }

    /// `if nt != P.+0xFC { P.+0xFC = nt; P.+0xF8 = 0; P.+0x100 = 0 }`.
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

    /// `randomResearchable(P, onlyCurrentEra)` (`0x55E470`, 7.3).
    pub fn random_researchable(
        &self,
        p: u32,
        only_current_era: bool,
        dice: &mut dyn Dice,
    ) -> i32 {
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

    /// The advance case of the goody hut (`0x55B8B0`, outcome 6, 10.5): the
    /// advance `p` is given, or [`NONE`] when the hut falls back to its
    /// default outcome (`p` is past the first era, or nothing qualifies).
    /// The caller grants it with `acquire(p, t, false, true, true)`.
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
            // The current research is only a fallback.
            if t == me.current {
                current = t;
                continue;
            }
            let depth = row.prereq.iter().map(|&q| self.tech_depth(q, 1)).max().unwrap_or(1);
            if depth > 4 {
                continue;
            }
            // The noise is drawn before the valuation, which draws too.
            let noise = dice.below(100) & 0xFFFF;
            let score = brain.value(self, p, t, dice) + noise;
            if score < best_score {
                best = t;
                best_score = score;
            }
        }
        let t = if best != NONE { best } else { current };
        if t != NONE {
            // Selects the message variant; the draw is part of the stream.
            dice.below(15);
        }
        t
    }

    /// `tryAdvanceEra(P)` (`0x561690`, 7.1): true when the player moved to
    /// the next era.
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

    /// `enterEra(P)` (`0x55E190`, 7.2), called after [`Self::try_advance_era`]
    /// returned true.
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

    /// `Player::acquire(P; t, byResearch, silent, runEras)` (`0x561860`, 6).
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
        // 2.
        self.known[t as usize] |= 1 << p;
        self.players[p as usize].known_count += 1;
        events.push(Event::Acquired { player: p, tech: t, by_research });
        // 9. First discoverer.
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
        // 10.
        if run_eras {
            while self.try_advance_era(p) {
                self.enter_era(p, events, ctx);
            }
        }
        // 11.
        if self.players[p as usize].current == t {
            let nt = self.choose_research(p, by_research, None, ctx.brain, ctx.dice);
            self.set_target(p, nt);
        }
        // 15.
        if flags & flags::REVEAL_MAP != 0 {
            events.push(Event::RevealMap { player: p });
        }
        let _ = silent;
        // 17. Philosophy: the next target (or the default pick) is free.
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

    /// `greatLibrary(P)` (`0x562380`, 8): every advance known by two civs
    /// that have contact with `p` is given to the holder.
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

    /// The research step (`0x562200`, 5): once per round per player, after
    /// the income pass has added the cities' beakers.
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

    /// The income pass (`0x560160`, 3.2) for one player: add `beakers` from
    /// the cities. No cap, no Science Age bonus.
    pub fn add_beakers(&mut self, p: u32, beakers: i32) {
        self.players[p as usize].beakers += beakers;
    }

    /// `Player::init` grants (7.5): the free era-0 advances, then the era
    /// loop. `slot` must be in play.
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
}

/// The callers' services for research code that picks or rolls.
pub struct Ctx<'a> {
    /// vtable `+0x58`.
    pub brain: &'a mut dyn Brain,
    /// The gameplay RNG.
    pub dice: &'a mut dyn Dice,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny tree: Alphabet <- Writing, Bronze Working alone, plus a
    /// medieval tech and a future-tech row for the cost vectors.
    fn rules() -> Rules {
        let row = |cost, era, prereq: [i32; 4], flags| TechRow {
            cost,
            era,
            prereq,
            flags,
            flavors: 0,
        };
        Rules {
            techs: vec![
                row(3, 0, [-1; 4], 0),               // 0 Bronze Working
                row(4, 0, [-1; 4], 0),               // 1 Masonry
                row(5, 0, [-1; 4], 0),               // 2 Alphabet
                row(8, 0, [2, -1, -1, -1], 0),       // 3 Writing
                row(15, 1, [3, -1, -1, -1], 0),      // 4 Literature (middle ages)
                row(360, 3, [4, -1, -1, -1], 0),     // 5 Integrated Defense-ish
            ],
            future_tech_cost: 400,
            max_research_turns: 50,
            min_research_turns: 4,
        }
    }

    /// A pick that always takes the lowest researchable index.
    struct Lowest;
    impl Brain for Lowest {
        fn default_pick(&mut self, w: &World, p: u32, _m: u8, _d: &mut dyn Dice) -> i32 {
            (0..w.t()).find(|&t| w.can_research(p, t)).unwrap_or(w.t())
        }
        /// The advance's cost stands in for its worth.
        fn value(&mut self, w: &World, _p: u32, t: i32, _d: &mut dyn Dice) -> i32 {
            w.rules.techs[t as usize].cost * 100
        }
    }

    #[test]
    fn a_world_round_trips_through_words() {
        let mut w = world(3);
        w.known[2] = 0b101;
        w.turn = 17;
        w.scientific_leaders = true;
        w.players[1].science_until = Some(37);
        w.players[1].beakers = 9;
        w.players[1].current = 3;
        w.players[1].queue = VecDeque::from([4, 5]);
        w.players[2].notice = Some((3, 1, 2));
        w.players[2].free_techs = [0, 2, -1, -1];
        let words = w.to_words();
        let mut fresh = world(3);
        assert!(fresh.restore(&words));
        assert_eq!(fresh.to_words(), words);
        assert_eq!((fresh.known[2], fresh.turn, fresh.players[1].beakers), (0b101, 17, 9));
        assert_eq!(fresh.players[1].queue, VecDeque::from([4, 5]));
        assert_eq!(fresh.players[2].notice, Some((3, 1, 2)));
        assert!(fresh.scientific_leaders);
        assert_eq!(fresh.players[1].science_until, Some(37));
        // A short or foreign stream changes nothing.
        let mut other = world(3);
        assert!(!other.restore(&words[..words.len() - 1]));
        assert!(other.to_words() == world(3).to_words());
    }

    #[test]
    fn scientific_leader_chances_and_gates_use_one_native_die() {
        struct Roll { value: i32, draws: Vec<u32> }
        impl Dice for Roll {
            fn below(&mut self, n: u32) -> i32 { self.draws.push(n); self.value }
        }
        for scientific in [false, true] {
            for value in 0..=5 {
                let mut w = world(3);
                w.scientific_leaders = true;
                w.turn = 1;
                w.players[1].scientific = scientific;
                let mut dice = Roll { value, draws: vec![] };
                let mut events = vec![];
                w.acquire(1, 0, true, true, false, &mut events, &mut Ctx { brain: &mut Lowest, dice: &mut dice });
                assert_eq!(dice.draws, [100]);
                assert_eq!(events.iter().any(|e| matches!(e, Event::ScientificLeader { player: 1, tech: 0 })), value < if scientific { 5 } else { 3 });
                w.acquire(2, 0, true, true, false, &mut events, &mut Ctx { brain: &mut Lowest, dice: &mut dice });
                assert_eq!(dice.draws, [100], "second discoverers do not roll");
            }
        }
        for (enabled, turn, researched) in [(false, 1, true), (true, 0, true), (true, 1, false)] {
            let mut w = world(3);
            w.scientific_leaders = enabled;
            w.turn = turn;
            let mut dice = Roll { value: 0, draws: vec![] };
            w.acquire(1, 0, researched, true, false, &mut vec![], &mut Ctx { brain: &mut Lowest, dice: &mut dice });
            assert!(dice.draws.is_empty());
        }
        // Philosophy's gift is a full acquire: its leader roll happens after
        // the first discovery's roll and after choosing the free advance.
        let mut w = world(3);
        w.scientific_leaders = true;
        w.turn = 1;
        w.rules.techs[0].flags = flags::BONUS_TECH;
        w.players[1].current = 0;
        let mut dice = Roll { value: 0, draws: vec![] };
        let mut events = vec![];
        w.acquire(1, 0, true, true, false, &mut events, &mut Ctx { brain: &mut Lowest, dice: &mut dice });
        assert_eq!(dice.draws, [100, 100]);
        let rewards: Vec<_> = events.iter().filter_map(|e| match e {
            Event::ScientificLeader { tech, .. } => Some(*tech), _ => None,
        }).collect();
        assert_eq!(rewards, [0, 1]);
    }

    fn world(n_civs: u32) -> World {
        let mut w = World::new(rules());
        w.in_play = (1u32 << n_civs) - 1;
        w
    }

    #[test]
    fn base_cost_golden_vectors() {
        // research.md 4.4: n = 8 civs (slots 0..7), Regent (cf 10),
        // Standard (rate 240), the player in slot 1 human.
        let mut w = world(8);
        w.human = 1 << 1;
        // Bronze Working costs 3: 12 * (3 * 10) / 12 = 30; 240 * 30 / 100.
        assert_eq!(w.base_cost(1, 0, false), 72);
        // Three contacted civs (2, 3, 4) that know it.
        for q in 2..=4 {
            w.players[1].contact |= 1 << q;
            w.known[0] |= 1 << q;
        }
        assert_eq!(w.base_cost(1, 0, false), 52);
        // Only two of the three know it.
        w.known[0] &= !(1 << 4);
        assert_eq!(w.base_cost(1, 0, false), 60);
        // Three know it, none is contacted.
        w.known[0] |= 1 << 4;
        w.players[1].contact = 0;
        assert_eq!(w.base_cost(1, 0, false), 72);
        // Tiny and Huge world sizes (Masonry costs 4).
        w.size_tech_rate = 160;
        assert_eq!(w.base_cost(1, 1, false), 64);
        w.size_tech_rate = 400;
        assert_eq!(w.base_cost(1, 1, false), 160);
        w.size_tech_rate = 240;
        // Accelerated Production halves the multiplier.
        w.accelerated = true;
        assert_eq!(w.base_cost(1, 0, false), 36);
        w.accelerated = false;
        // A future technology ignores contacts and the row.
        assert_eq!(w.base_cost(1, w.t(), false), 9600);
    }

    #[test]
    fn base_cost_difficulty_and_players() {
        let mut w = world(8);
        w.human = 1 << 1;
        // The last row costs 360: 8640 at Regent.
        assert_eq!(w.base_cost(1, 5, false), 8640);
        // An AI on Sid (cf 4) pays as a human on Regent; a human on Sid
        // pays 10 / 4 times more.
        w.difficulty_cost_factor = 4;
        assert_eq!(w.base_cost(2, 5, false), 8640);
        assert_eq!(w.base_cost(1, 5, false), 21600);
        // Chieftain (cf 20): the multiplier clamps at 10.
        w.difficulty_cost_factor = 20;
        assert_eq!(w.base_cost(2, 5, false), 8640);
        // Deity (cf 6); `neutral` prices the AI as a human.
        w.difficulty_cost_factor = 6;
        assert_eq!(w.base_cost(2, 5, false), 8640);
        assert_eq!(w.base_cost(2, 5, true), 14400);
    }

    #[test]
    fn base_cost_small_games() {
        // n = 3 (slots 0, 1, 3) and n = 2 leave Bronze Working at 72.
        let mut w = World::new(rules());
        w.human = 1 << 1;
        w.in_play = 0b1011;
        assert_eq!(w.base_cost(1, 0, false), 72);
        w.in_play = 0b11;
        assert_eq!(w.base_cost(1, 0, false), 72);
        // n = 5, Alphabet (5): 120.
        w.in_play = 0b111110;
        assert_eq!(w.base_cost(1, 2, false), 120);
    }

    #[test]
    fn effective_cost_golden_vectors() {
        let mut w = world(8);
        w.human = 1 << 1;
        // Non-current, s = 0: the constants (min 4 ... max 50).
        assert_eq!(w.effective_cost(1, 0, false), 50);
        w.players[1].rate = 30;
        assert_eq!(w.effective_cost(1, 0, false), 120);
        assert_eq!(w.effective_cost(1, 5, false), 1500);
        // Tiny Masonry: base 64, s = 5 and the Science Age's 6.
        w.size_tech_rate = 160;
        w.players[1].rate = 5;
        assert_eq!(w.effective_cost(1, 1, false), 64);
        w.players[1].rate = 6;
        assert_eq!(w.effective_cost(1, 1, false), 64);
        w.size_tech_rate = 240;
        w.players[1].rate = 0;
        assert_eq!(w.effective_cost(1, 5, false), 50);
        // The current research.
        w.players[1].current = 0;
        w.players[1].rate = 10;
        w.players[1].beakers = 20;
        w.players[1].turns = 2;
        assert_eq!(w.effective_cost(1, 0, false), 72);
        w.players[1].rate = 100;
        w.players[1].beakers = 0;
        w.players[1].turns = 0;
        assert_eq!(w.effective_cost(1, 0, false), 400);
        w.players[1].rate = 0;
        w.players[1].beakers = 15;
        w.players[1].turns = 10;
        assert_eq!(w.effective_cost(1, 0, false), 55);
        w.players[1].rate = 10;
        w.players[1].turns = 60;
        assert_eq!(w.effective_cost(1, 0, false), 15);
        w.players[1].beakers = 500;
        w.players[1].turns = 9;
        assert_eq!(w.effective_cost(1, 0, false), 500);
        w.players[1].beakers = 0;
        w.players[1].turns = 0;
        w.players[1].current = 5;
        assert_eq!(w.effective_cost(1, 5, false), 500);
        // A future technology: s = 40, b = 100, k = 3.
        w.players[1].current = w.t();
        w.players[1].rate = 40;
        w.players[1].beakers = 100;
        w.players[1].turns = 3;
        assert_eq!(w.effective_cost(1, w.t(), false), 1980);
        // The Science Age: s = 7 -> 8, base 72.
        w.players[1].current = 0;
        w.players[1].rate = 8;
        w.players[1].beakers = 30;
        w.players[1].turns = 1;
        assert_eq!(w.effective_cost(1, 0, false), 72);
    }

    #[test]
    fn turns_left_rounds_up() {
        let mut w = world(8);
        w.human = 1 << 1;
        assert_eq!(w.turns_left(1, 0, false), 9999);
        w.players[1].rate = 30;
        // Cost 120 at 30 a turn.
        assert_eq!(w.turns_left(1, 0, false), 4);
        w.players[1].rate = 7;
        assert_eq!(w.turns_left(1, 0, false), 11);
        assert_eq!(w.turns_left(1, NONE, false), 0);
    }

    #[test]
    fn predicates() {
        let mut w = world(3);
        assert!(w.can_research(1, 0));
        assert!(!w.can_research(1, 3), "Writing needs Alphabet");
        assert!(!w.can_research(1, 4), "later era");
        assert!(w.reachable(1, 4));
        assert_eq!(w.tech_depth(4, 0), 3);
        w.known[2] |= 1 << 1;
        assert!(w.can_research(1, 3));
        assert!(!w.can_research(1, 2), "already known");
        assert!(w.knows(1, NONE));
        assert!(!w.knows(1, w.t()));
    }

    #[test]
    fn a_completed_target_picks_the_next_and_drops_surplus() {
        let mut w = world(3);
        w.human = 0;
        w.players[1].cities = 1;
        w.players[1].rate = 5;
        w.players[1].current = 0;
        w.players[1].beakers = 200;
        // Three research turns are behind it; this one is the fourth, so
        // the minimum research time (4) no longer holds the advance back.
        w.players[1].turns = 3;
        let mut ev = vec![];
        let (mut brain, mut dice) = (Lowest, Rng::new(1));
        w.step(1, &mut ev, &mut Ctx { brain: &mut brain, dice: &mut dice });
        assert!(w.knows(1, 0));
        // The next target is the lowest researchable one; beakers reset.
        assert_eq!(w.players[1].current, 1);
        assert_eq!(w.players[1].beakers, 0);
        assert_eq!(w.players[1].turns, 0);
        assert!(ev.contains(&Event::Acquired { player: 1, tech: 0, by_research: true }));
    }

    #[test]
    fn no_cities_no_research() {
        let mut w = world(3);
        w.players[1].rate = 5;
        w.players[1].current = 0;
        w.players[1].beakers = 200;
        let mut ev = vec![];
        let (mut brain, mut dice) = (Lowest, Rng::new(1));
        w.step(1, &mut ev, &mut Ctx { brain: &mut brain, dice: &mut dice });
        assert!(!w.knows(1, 0));
        assert!(ev.is_empty());
    }

    #[test]
    fn a_player_without_a_target_discards_beakers_when_one_is_chosen() {
        let mut w = world(3);
        w.players[1].cities = 1;
        w.players[1].beakers = 40;
        let mut ev = vec![];
        let (mut brain, mut dice) = (Lowest, Rng::new(1));
        w.step(1, &mut ev, &mut Ctx { brain: &mut brain, dice: &mut dice });
        assert_eq!(w.players[1].current, 0);
        assert_eq!(w.players[1].beakers, 0);
    }

    #[test]
    fn the_era_advances_when_the_last_required_advance_is_known() {
        let mut w = world(3);
        w.human = 0;
        // Era 0 = Bronze Working, Masonry, Alphabet, Writing.
        let (mut brain, mut dice) = (Lowest, Rng::new(1));
        let mut ev = vec![];
        for t in [0, 1, 2] {
            w.acquire(1, t, true, true, true, &mut ev, &mut Ctx { brain: &mut brain, dice: &mut dice });
        }
        assert_eq!(w.players[1].era, 0, "Writing is still missing");
        w.acquire(1, 3, true, true, true, &mut ev, &mut Ctx { brain: &mut brain, dice: &mut dice });
        assert_eq!(w.players[1].era, 1);
        assert!(ev.contains(&Event::EnteredEra { player: 1, era: 1 }));
        // The first era to complete with nobody ahead records the bit.
        assert_eq!(w.players[1].achievements, 1);
    }

    #[test]
    fn not_required_advances_do_not_block_the_era() {
        let mut r = rules();
        r.techs[3].flags |= flags::NOT_REQUIRED_FOR_ERA;
        let mut w = World::new(r);
        w.in_play = 0b111;
        let (mut brain, mut dice) = (Lowest, Rng::new(1));
        let mut ev = vec![];
        for t in [0, 1, 2] {
            w.acquire(1, t, true, true, true, &mut ev, &mut Ctx { brain: &mut brain, dice: &mut dice });
        }
        assert_eq!(w.players[1].era, 1);
    }

    #[test]
    fn a_second_civ_in_an_era_lands_barbarians() {
        let mut w = world(3);
        w.turn = 5;
        w.players[2].era = 1;
        let (mut brain, mut dice) = (Lowest, Rng::new(1));
        let mut ev = vec![];
        for t in [0, 1, 2, 3] {
            w.acquire(1, t, true, true, true, &mut ev, &mut Ctx { brain: &mut brain, dice: &mut dice });
        }
        assert!(ev.contains(&Event::BarbarianLanding { era: 1 }));
    }

    #[test]
    fn free_techs_pay_out_one_era_at_a_time() {
        let mut w = world(3);
        w.players[1].free_techs = [0, 4, NONE, NONE];
        let (mut brain, mut dice) = (Lowest, Rng::new(1));
        let mut ev = vec![];
        w.start_grants(1, &mut ev, &mut Ctx { brain: &mut brain, dice: &mut dice });
        assert!(w.knows(1, 0), "an era-0 grant at the start");
        assert!(!w.knows(1, 4), "the era-1 one waits for the era");
        for t in [1, 2, 3] {
            w.acquire(1, t, true, true, true, &mut ev, &mut Ctx { brain: &mut brain, dice: &mut dice });
        }
        assert!(w.knows(1, 4), "granted on entering era 1");
        // Literature was era 1's only advance, so the empty eras after it
        // pass at once and the player ends in the last era.
        assert_eq!(w.players[1].era, LAST_ERA);
    }

    #[test]
    fn the_scientific_trait_gets_a_free_advance_each_era() {
        let mut w = world(3);
        w.players[1].scientific = true;
        let (mut brain, mut dice) = (Lowest, Rng::new(7));
        let mut ev = vec![];
        for t in [0, 1, 2, 3] {
            w.acquire(1, t, true, true, true, &mut ev, &mut Ctx { brain: &mut brain, dice: &mut dice });
        }
        // Era 1's only advance (Literature) arrives for free.
        assert!(w.knows(1, 4));
    }

    #[test]
    fn future_technologies_only_reset_the_counters() {
        let mut w = world(3);
        w.players[1].beakers = 5;
        w.players[1].turns = 9;
        let (mut brain, mut dice) = (Lowest, Rng::new(1));
        let mut ev = vec![];
        let t = w.t();
        w.acquire(1, t, true, true, true, &mut ev, &mut Ctx { brain: &mut brain, dice: &mut dice });
        assert_eq!(w.players[1].future, 1);
        assert_eq!((w.players[1].beakers, w.players[1].turns), (0, 0));
        assert_eq!(w.players[1].known_count, 0);
        assert_eq!(ev, vec![Event::FutureTech { player: 1 }]);
    }

    #[test]
    fn the_bonus_tech_gives_the_first_discoverer_another() {
        let mut r = rules();
        r.techs[2].flags |= flags::BONUS_TECH;
        let mut w = World::new(r);
        w.in_play = 0b111;
        w.players[1].cities = 1;
        let (mut brain, mut dice) = (Lowest, Rng::new(1));
        let mut ev = vec![];
        w.acquire(1, 2, true, true, true, &mut ev, &mut Ctx { brain: &mut brain, dice: &mut dice });
        // Alphabet gifted the lowest researchable advance: Bronze Working.
        assert!(w.knows(1, 0));
        // Someone else learning it second gets no gift.
        let mut ev2 = vec![];
        w.acquire(2, 2, true, true, true, &mut ev2, &mut Ctx { brain: &mut brain, dice: &mut dice });
        assert!(!w.knows(2, 0));
    }

    #[test]
    fn the_great_library_gives_what_two_civs_know() {
        let mut w = world(4);
        w.players[1].great_library = true;
        for q in [2u32, 3] {
            w.players[q as usize].contact |= 1 << 1;
            w.known[0] |= 1 << q;
        }
        w.known[1] |= 1 << 2; // only one civ knows Masonry
        let (mut brain, mut dice) = (Lowest, Rng::new(1));
        let mut ev = vec![];
        w.great_library(1, &mut ev, &mut Ctx { brain: &mut brain, dice: &mut dice });
        assert!(w.knows(1, 0));
        assert!(!w.knows(1, 1));
        assert!(ev.iter().any(|e| matches!(e, Event::GreatLibrary { tech: 0, first: 2, second: 3, .. })));
    }

    #[test]
    fn completing_the_obsoleting_advance_stops_the_library_before_its_grant_pass() {
        let mut w = world(4);
        w.players[1].great_library = true;
        w.players[1].great_library_obsolete = 0;
        w.players[1].current = 0;
        w.players[1].cities = 1;
        w.players[1].rate = 100;
        w.players[1].beakers = 10000;
        w.players[1].turns = w.rules.min_research_turns - 1;
        for q in [2u32, 3] {
            w.players[q as usize].contact |= 1 << 1;
            w.known[1] |= 1 << q;
        }
        let (mut brain, mut dice) = (Lowest, Rng::new(1));
        let mut ev = vec![];
        w.step(1, &mut ev, &mut Ctx { brain: &mut brain, dice: &mut dice });
        assert!(w.knows(1, 0), "the research completes first");
        assert!(!w.knows(1, 1), "the obsolete wonder grants nothing");
        assert!(!ev.iter().any(|e| matches!(e, Event::GreatLibrary { .. })));
    }

    #[test]
    fn an_interactive_player_chooses_and_the_queue_is_replaced() {
        let mut w = world(3);
        w.players[1].interactive = true;
        let (mut brain, mut dice) = (Lowest, Rng::new(1));
        // Nothing queued: the dialog is needed.
        assert_eq!(w.choose_research(1, false, None, &mut brain, &mut dice), PENDING);
        assert!(w.players[1].awaiting_choice);
        w.choose(1, 2, &mut brain, &mut dice);
        assert_eq!(w.players[1].current, 2);
        assert!(!w.players[1].awaiting_choice);
        // Picking a different advance replaces the whole queue.
        w.players[1].queue.push_back(1);
        w.players[1].queue.push_back(0);
        w.choose(1, 0, &mut brain, &mut dice);
        assert_eq!(w.players[1].queue.iter().copied().collect::<Vec<_>>(), vec![0]);
    }

    #[test]
    fn cleaning_drops_known_and_unreachable_entries() {
        let mut w = world(3);
        w.known[0] |= 1 << 1;
        for v in [0, 3, 99, NONE, 1] {
            w.players[1].queue.push_back(v);
        }
        w.clean_queue(1);
        assert_eq!(w.players[1].queue.front(), Some(&1));
    }

    #[test]
    fn random_researchable_lists_the_choices() {
        let mut w = world(3);
        let mut dice = Rng::new(3);
        let t = w.random_researchable(1, true, &mut dice);
        assert!([0, 1, 2].contains(&t));
        for k in 0..3 {
            w.known[k] |= 1 << 1;
        }
        w.known[3] |= 1 << 1;
        // The middle-ages row is not researchable in era 0.
        assert_eq!(w.random_researchable(1, false, &mut dice), NONE);
    }

    /// A dice that returns a fixed value and counts the draws.
    struct Fixed(i32, u32);
    impl Dice for Fixed {
        fn below(&mut self, _n: u32) -> i32 {
            self.1 += 1;
            self.0
        }
    }

    #[test]
    fn a_goody_hut_gives_the_cheapest_first_era_advance() {
        let w = world(2);
        let mut dice = Fixed(0, 0);
        // Bronze Working (3) is the lowest valuation; Writing needs Alphabet.
        assert_eq!(w.hut_advance(1, &mut Lowest, &mut dice), 0);
        // One noise draw per candidate (Bronze, Masonry, Alphabet), then
        // the message variant: Writing is not yet eligible.
        assert_eq!(dice.1, 4);
    }

    #[test]
    fn a_goody_hut_noise_can_reorder_and_the_current_research_is_a_fallback() {
        let mut w = world(2);
        // Noise of 99 is the same for each; the order is the valuation's.
        let mut dice = Fixed(99, 0);
        assert_eq!(w.hut_advance(1, &mut Lowest, &mut dice), 0);
        // The current research is skipped while another candidate exists.
        w.players[1].current = 0;
        assert_eq!(w.hut_advance(1, &mut Lowest, &mut Fixed(0, 0)), 1);
        // With every other first-era advance known, the current research is
        // given.
        for t in [1, 2, 3] {
            w.known[t] |= 1 << 1;
        }
        let mut dice = Fixed(0, 0);
        let t = w.hut_advance(1, &mut Lowest, &mut dice);
        assert_eq!(t, 0);
        assert_eq!(dice.1, 1, "only the message draw: nothing else was scored");
    }

    #[test]
    fn a_goody_hut_gives_nothing_to_a_civ_past_the_first_era() {
        let mut w = world(2);
        w.players[1].era = 1;
        let mut dice = Fixed(0, 0);
        assert_eq!(w.hut_advance(1, &mut Lowest, &mut dice), NONE);
        assert_eq!(dice.1, 0);
    }
}
