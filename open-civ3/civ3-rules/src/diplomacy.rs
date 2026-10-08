//! Diplomacy: relation state, war and peace, the AI's attitude and war
//! decision, and the deal ladder.
//!
//! Every per-pair table is kept here. `p` is the slot of the player that owns
//! the table and `q` the other civ; slot 0 is the barbarians and is never a
//! diplomatic partner.

/// Dwords per matrix row (`esi*2105`, byte stride 8420).
pub const ROW_STRIDE: u32 = 2105;
/// Dwords per matrix column (`eax*19`, byte stride 76).
pub const COL_STRIDE: u32 = 19;

/// Player slots (0 = barbarians).
pub const SLOTS: usize = 32;

/// Dword index of cell `(row, col)` in the relation matrix.
pub fn relation_index(row: u32, col: u32) -> u32 {
    row * ROW_STRIDE + col * COL_STRIDE
}

/// Tech-trade bump: increment the cell and return the new value.
pub fn bump_relation(matrix: &mut [u32], row: u32, col: u32) -> u32 {
    let i = relation_index(row, col) as usize;
    matrix[i] = matrix[i].wrapping_add(1);
    matrix[i]
}

/// Deal-response selector: the score maps to a `DIPLOADVICETRADE_DEAL_*`
/// script key; any other value falls through with no dialog.
pub fn deal_response(score: u32) -> Option<&'static str> {
    match score {
        36 => Some("DIPLOADVICETRADE_DEAL_ACCEPT"),
        37 => Some("DIPLOADVICETRADE_DEAL_WEAKREJECT"),
        38 => Some("DIPLOADVICETRADE_DEAL_NEUTRALREJECT"),
        39 => Some("DIPLOADVICETRADE_DEAL_STRONGREJECT"),
        _ => None,
    }
}

/// Threshold ladder: side-A total vs side-B total yields the 36..39 verdict.
pub fn deal_threshold_verdict(side_a: i32, side_b: i32) -> u32 {
    if side_a >= side_b {
        36
    } else if side_a > side_b.wrapping_mul(7) / 8 {
        37
    } else if side_a > side_b / 2 {
        38
    } else {
        39
    }
}

/// Attitude scaling: side-B total is multiplied by `4*T+1` where `T` is the
/// pair record's deals-cancelled word.
pub fn attitude_scaled(base: i32, t: i32) -> i32 {
    base.wrapping_mul(t.wrapping_mul(4).wrapping_add(1))
}

/// Scorer epilogue deltas: offer-minus-ask and the gated delta.
pub fn scorer_deltas(offer: i32, ask: i32, gated_a: i32, gated_b: i32) -> (i32, i32) {
    (offer.wrapping_sub(ask), gated_a.wrapping_sub(gated_b))
}

/// Offsets (in dwords) inside the 19-dword pair record. `rec(p, q)` is `p`'s
/// book about `q`; the counters that say what `q` did to `p` are kept in `p`'s
/// book, the ones that say what `p` did to `q` in `q`'s.
pub mod rec {
    /// Declarations of war by `q` against `p`.
    pub const DECLARATIONS: usize = 0;
    /// Deals cancelled (declaring war with a package in force).
    pub const DEALS_CANCELLED: usize = 1;
    /// Unnamed counter at `+0x08`.
    pub const C08: usize = 2;
    /// Right-of-passage violations.
    pub const ROP_VIOLATIONS: usize = 3;
    /// Hostile acts (provocations).
    pub const HOSTILE_ACTS: usize = 4;
    /// Attacks by `q`.
    pub const ATTACKS: usize = 5;
    /// Diplomacy-log counter.
    pub const LOG18: usize = 6;
    /// Diplomacy-log counter.
    pub const LOG1C: usize = 7;
    /// Diplomacy-log counter.
    pub const LOG20: usize = 8;
    /// Goodwill: lowers the attitude by one per ten, at most ten.
    pub const C24: usize = 9;
    /// Tech trades.
    pub const TECH_TRADES: usize = 10;
    /// Hidden attacks.
    pub const HIDDEN_ATTACKER: usize = 11;
    /// Visible attacks.
    pub const VISIBLE_ATTACKER: usize = 12;
    /// Incident accumulator (halved at peace).
    pub const ACC34: usize = 13;
    /// Incident accumulator (zeroed at peace).
    pub const ACC38: usize = 14;
    /// Unnamed counter at `+0x3C`.
    pub const C3C: usize = 15;
    /// Razed cities.
    pub const RAZED: usize = 16;
    /// Unnamed counter at `+0x44`.
    pub const C44: usize = 17;
    /// Unnamed counter at `+0x48`.
    pub const C48: usize = 18;
    /// Dwords in a record.
    pub const LEN: usize = 19;
}

/// Bits of the relation word.
pub mod relbit {
    /// Contact, set on both sides when the pair first meets, never cleared.
    pub const CONTACT: u32 = 0x1;
    /// Set on each AI side when it has just met this civ.
    pub const AI_JUST_MET: u32 = 0x2;
    /// Border warning.
    pub const BORDER_WARNING: u32 = 0x8;
    /// Border war track.
    pub const BORDER_WAR: u32 = 0x10;
    /// Bits a declaration of war clears.
    pub const CLEARED_BY_WAR: u32 = 0x3E;
}

/// Bits of the treaty word.
pub mod treaty {
    /// Mutual protection pact.
    pub const MPP: u32 = 1;
    /// Right of passage.
    pub const ROP: u32 = 2;
    /// Alliance.
    pub const ALLIANCE: u32 = 4;
}

/// The AI's greeting timer after meeting a human.
pub const GREETING_TURNS: i32 = 0x20;

/// Turns a timed package lasts.
pub const PACKAGE_TURNS: i32 = 20;

/// What the attitude and war decisions read from outside the diplomatic state.
pub trait Env {
    /// `RACE` aggression for the AI (clamped to `-2..=2`).
    fn aggression(&self, p: u32) -> i32;
    /// The player's government row.
    fn government(&self, p: u32) -> i32;
    /// `RACE.shunned_government` of `p`'s civilization.
    fn shunned(&self, p: u32) -> i32;
    /// `RACE.favorite_government` of `p`'s civilization.
    fn favorite(&self, p: u32) -> i32;
    /// `RACE.culture_group` of `p`'s civilization.
    fn culture_group(&self, p: u32) -> i32;
    /// The player's score.
    fn score(&self, p: u32) -> i32;
    /// The player's rank, 1 for the leader.
    fn rank(&self, p: u32) -> i32;
    /// `p` and `q` have cities on one continent.
    fn shares_continent(&self, p: u32, q: u32) -> bool;
    /// Units of `p` whose nationality is `q`'s civilization.
    fn nationals(&self, p: u32, q: u32) -> i32;
    /// `GOVT.war_weariness` of a government row.
    fn war_weariness(&self, government: i32) -> i32;
    /// Map width and height.
    fn map_size(&self) -> (i32, i32);
    /// Number of cities of `p`.
    fn cities(&self, p: u32) -> i32;
}

/// A war declaration made another civ join, or started a war.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WarCall {
    /// The declaring civ.
    pub by: u32,
    /// The civ it declared war on.
    pub on: u32,
    /// The reason word (`0` plain, `2 + c` mutual protection of `c`,
    /// `0x22 + c` military alliance with `c`).
    pub reason: i32,
}

/// A timed group of clauses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package {
    /// One party.
    pub a: u32,
    /// The other party.
    pub b: u32,
    /// The turn the package lapses.
    pub ends: i32,
    /// What was agreed.
    pub clauses: Vec<Clause>,
}

/// One item of a deal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Clause {
    /// A peace treaty.
    Peace,
    /// Mutual protection.
    MutualProtection,
    /// Right of passage.
    RightOfPassage,
    /// Military alliance against a third civ.
    MilitaryAlliance(u32),
    /// Embargo of a civ.
    Embargo(u32),
    /// The world map.
    WorldMap,
    /// Establish contact with a civ.
    Contact(u32),
    /// Gold, once.
    Gold(i32),
    /// Gold per turn for [`PACKAGE_TURNS`] turns.
    GoldPerTurn(i32),
    /// An advance.
    Tech(i32),
    /// A city.
    City(u32),
}

impl Clause {
    /// The tech flag that unlocks the clause (`0` for none).
    pub fn tech_flag(&self) -> u32 {
        use crate::research::flags;
        match self {
            Clause::MutualProtection => flags::MPP,
            Clause::RightOfPassage => flags::RIGHT_OF_PASSAGE,
            Clause::MilitaryAlliance(_) => flags::MILITARY_ALLIANCE,
            Clause::Embargo(_) => flags::TRADE_EMBARGO,
            _ => 0,
        }
    }

    /// Counted in the gated subtotal of the scorer: treaties, alliances,
    /// embargoes, contact, gold, advances; not maps or cities.
    pub fn gated(&self) -> bool {
        !matches!(self, Clause::WorldMap | Clause::City(_))
    }

    /// Timed clauses start a package.
    pub fn timed(&self) -> bool {
        matches!(self, Clause::Peace | Clause::GoldPerTurn(_))
    }

    fn write(&self, w: &mut crate::words::Writer) {
        let (kind, arg) = match self {
            Clause::Peace => (0, 0),
            Clause::MutualProtection => (1, 0),
            Clause::RightOfPassage => (2, 0),
            Clause::MilitaryAlliance(a) => (3, i64::from(*a)),
            Clause::Embargo(a) => (4, i64::from(*a)),
            Clause::WorldMap => (5, 0),
            Clause::Contact(a) => (6, i64::from(*a)),
            Clause::Gold(a) => (7, i64::from(*a)),
            Clause::GoldPerTurn(a) => (8, i64::from(*a)),
            Clause::Tech(a) => (9, i64::from(*a)),
            Clause::City(a) => (10, i64::from(*a)),
        };
        w.put(kind);
        w.put(arg);
    }

    fn read(r: &mut crate::words::Reader) -> Option<Clause> {
        let (kind, arg) = (r.get()?, r.get()?);
        let u = u32::try_from(arg).ok();
        let i = i32::try_from(arg).ok();
        Some(match kind {
            0 => Clause::Peace,
            1 => Clause::MutualProtection,
            2 => Clause::RightOfPassage,
            3 => Clause::MilitaryAlliance(u?),
            4 => Clause::Embargo(u?),
            5 => Clause::WorldMap,
            6 => Clause::Contact(u?),
            7 => Clause::Gold(i?),
            8 => Clause::GoldPerTurn(i?),
            9 => Clause::Tech(i?),
            10 => Clause::City(u?),
            _ => return None,
        })
    }
}

/// The diplomatic state of every pair.
#[derive(Clone, Debug)]
pub struct Relations {
    /// Slots in play.
    pub in_play: u32,
    /// Human slots.
    pub human: u32,
    war: Vec<bool>,
    embassy: Vec<bool>,
    rel: Vec<u32>,
    treaty: Vec<u32>,
    allies_vs: Vec<u32>,
    embargo: Vec<u32>,
    memory: Vec<i32>,
    greeting: Vec<i32>,
    war_counter: Vec<i32>,
    tension: Vec<i32>,
    rec: Vec<[i32; rec::LEN]>,
    /// Timed packages in force.
    pub packages: Vec<Package>,
}

impl Relations {
    /// No contact, no war, empty books.
    pub fn new(in_play: u32, human: u32) -> Self {
        let n = SLOTS * SLOTS;
        Relations {
            in_play,
            human,
            war: vec![false; n],
            embassy: vec![false; n],
            rel: vec![0; n],
            treaty: vec![0; n],
            allies_vs: vec![0; n],
            embargo: vec![0; n],
            memory: vec![0; n],
            greeting: vec![0; n],
            war_counter: vec![0; n],
            tension: vec![0; n],
            rec: vec![[0; rec::LEN]; n],
            packages: Vec::new(),
        }
    }

    /// Every book, as words (see [`crate::words`]).
    pub fn to_words(&self) -> Vec<i64> {
        let mut w = crate::words::Writer::default();
        w.put(self.in_play);
        w.put(self.human);
        w.flags(&self.war);
        w.flags(&self.embassy);
        for v in [&self.rel, &self.treaty, &self.allies_vs, &self.embargo] {
            w.run(v);
        }
        for v in [&self.memory, &self.greeting, &self.war_counter, &self.tension] {
            w.run(v);
        }
        w.put(self.rec.len() as i64);
        for row in &self.rec {
            for &c in row {
                w.put(c);
            }
        }
        w.put(self.packages.len() as i64);
        for p in &self.packages {
            w.put(p.a);
            w.put(p.b);
            w.put(p.ends);
            w.put(p.clauses.len() as i64);
            for c in &p.clauses {
                c.write(&mut w);
            }
        }
        w.0
    }

    /// Put saved books back. False, with `self` untouched, when the words do not
    /// fit.
    pub fn restore(&mut self, words: &[i64]) -> bool {
        let mut r = crate::words::Reader::new(words);
        let n = self.war.len();
        let parsed = (|| {
            let (in_play, human) = (r.u32()?, r.u32()?);
            let flags = |r: &mut crate::words::Reader| r.run(|v| Some(v != 0)).filter(|v| v.len() == n);
            let war = flags(&mut r)?;
            let embassy = flags(&mut r)?;
            let mut us = vec![];
            for _ in 0..4 {
                us.push(r.run(|v| u32::try_from(v).ok()).filter(|v| v.len() == n)?);
            }
            let mut is = vec![];
            for _ in 0..4 {
                is.push(r.run(|v| i32::try_from(v).ok()).filter(|v| v.len() == n)?);
            }
            if usize::try_from(r.get()?).ok()? != n {
                return None;
            }
            let mut rec = vec![[0i32; rec::LEN]; n];
            for row in &mut rec {
                for c in row.iter_mut() {
                    *c = r.i32()?;
                }
            }
            let count = usize::try_from(r.get()?).ok()?;
            let mut packages = vec![];
            for _ in 0..count {
                let (a, b, ends) = (r.u32()?, r.u32()?, r.i32()?);
                let k = usize::try_from(r.get()?).ok()?;
                let clauses = (0..k).map(|_| Clause::read(&mut r)).collect::<Option<Vec<_>>>()?;
                packages.push(Package { a, b, ends, clauses });
            }
            r.done().then_some((in_play, human, war, embassy, us, is, rec, packages))
        })();
        let Some((in_play, human, war, embassy, mut us, mut is, rec, packages)) = parsed else {
            return false;
        };
        self.in_play = in_play;
        self.human = human;
        self.war = war;
        self.embassy = embassy;
        self.embargo = us.pop().unwrap();
        self.allies_vs = us.pop().unwrap();
        self.treaty = us.pop().unwrap();
        self.rel = us.pop().unwrap();
        self.tension = is.pop().unwrap();
        self.war_counter = is.pop().unwrap();
        self.greeting = is.pop().unwrap();
        self.memory = is.pop().unwrap();
        self.rec = rec;
        self.packages = packages;
        true
    }

    fn ix(p: u32, q: u32) -> usize {
        p as usize * SLOTS + q as usize
    }

    fn human(&self, p: u32) -> bool {
        self.human >> p & 1 != 0
    }

    fn in_play(&self, p: u32) -> bool {
        self.in_play >> p & 1 != 0
    }

    /// The at-war byte.
    pub fn at_war(&self, p: u32, q: u32) -> bool {
        self.war[Self::ix(p, q)]
    }

    /// The communications / embassy byte.
    pub fn embassy(&self, p: u32, q: u32) -> bool {
        self.embassy[Self::ix(p, q)]
    }

    /// The relation word.
    pub fn rel(&self, p: u32, q: u32) -> u32 {
        self.rel[Self::ix(p, q)]
    }

    /// Mutable relation word.
    pub fn rel_mut(&mut self, p: u32, q: u32) -> &mut u32 {
        &mut self.rel[Self::ix(p, q)]
    }

    /// The treaty word.
    pub fn treaty(&self, p: u32, q: u32) -> u32 {
        self.treaty[Self::ix(p, q)]
    }

    /// Civs allied with `p` against `q`.
    pub fn allies_vs(&self, p: u32, q: u32) -> u32 {
        self.allies_vs[Self::ix(p, q)]
    }

    /// Civs embargoing `q` together with `p`.
    pub fn embargo(&self, p: u32, q: u32) -> u32 {
        self.embargo[Self::ix(p, q)]
    }

    /// The AI's war memory.
    pub fn memory(&self, p: u32, q: u32) -> i32 {
        self.memory[Self::ix(p, q)]
    }

    /// The war counter.
    pub fn war_counter(&self, p: u32, q: u32) -> i32 {
        self.war_counter[Self::ix(p, q)]
    }

    /// Mutable war counter.
    pub fn war_counter_mut(&mut self, p: u32, q: u32) -> &mut i32 {
        &mut self.war_counter[Self::ix(p, q)]
    }

    /// The border tension.
    pub fn tension(&self, p: u32, q: u32) -> i32 {
        self.tension[Self::ix(p, q)]
    }

    /// Mutable tension.
    pub fn tension_mut(&mut self, p: u32, q: u32) -> &mut i32 {
        &mut self.tension[Self::ix(p, q)]
    }

    /// `p`'s book about `q`.
    pub fn rec(&self, p: u32, q: u32) -> &[i32; rec::LEN] {
        &self.rec[Self::ix(p, q)]
    }

    /// Mutable book.
    pub fn rec_mut(&mut self, p: u32, q: u32) -> &mut [i32; rec::LEN] {
        &mut self.rec[Self::ix(p, q)]
    }

    /// The pair has met (relation bit 0).
    pub fn contact(&self, p: u32, q: u32) -> bool {
        self.rel(p, q) & relbit::CONTACT != 0
    }

    /// `canTalk`: `embassy && !war`, with no espionage channel.
    pub fn can_talk(&self, p: u32, q: u32) -> bool {
        self.embassy(p, q) && !self.at_war(p, q)
    }

    /// Set the contact bit on both sides, never cleared. Returns whether the
    /// pair had not met. Each AI side also gets [`relbit::AI_JUST_MET`], and an
    /// AI that meets a human starts a greeting timer.
    pub fn establish_contact(&mut self, p: u32, q: u32) -> bool {
        if p == q || p == 0 || q == 0 || self.contact(p, q) {
            return false;
        }
        for (a, b) in [(p, q), (q, p)] {
            *self.rel_mut(a, b) |= relbit::CONTACT;
            if !self.human(a) {
                *self.rel_mut(a, b) |= relbit::AI_JUST_MET;
                if self.human(b) {
                    self.greeting[Self::ix(a, b)] = GREETING_TURNS;
                }
            }
        }
        true
    }

    /// The AI's greeting timer about a human it has met.
    pub fn greeting(&self, p: u32, q: u32) -> i32 {
        self.greeting[Self::ix(p, q)]
    }

    /// Store the embassy byte for one direction.
    pub fn set_embassy(&mut self, p: u32, q: u32) {
        let i = Self::ix(p, q);
        self.embassy[i] = true;
    }

    /// Both directions at once (the first-contact conversation).
    pub fn exchange_embassies(&mut self, p: u32, q: u32) {
        self.set_embassy(p, q);
        self.set_embassy(q, p);
    }

    /// Declare war, including the call-in of every civ allied through
    /// `allies_vs`. Returns every declaration made, the first being `p` on `q`.
    pub fn declare_war(
        &mut self,
        env: &dyn Env,
        p: u32,
        q: u32,
        reason: i32,
        rop_violation: bool,
    ) -> Vec<WarCall> {
        let mut calls = Vec::new();
        self.declare_war_inner(env, p, q, reason, rop_violation, &mut calls);
        calls
    }

    fn declare_war_inner(
        &mut self,
        env: &dyn Env,
        p: u32,
        q: u32,
        reason: i32,
        rop_violation: bool,
        calls: &mut Vec<WarCall>,
    ) {
        if q == 0 || p == q || self.at_war(p, q) {
            return;
        }
        calls.push(WarCall { by: p, on: q, reason });
        self.establish_contact(p, q);
        let rop = self.treaty(p, q) & treaty::ROP != 0;
        if (rop || self.tension(q, p) >= 0x200) && rop_violation {
            self.rec_mut(q, p)[rec::ROP_VIOLATIONS] += 1;
        }
        self.rec_mut(q, p)[rec::DECLARATIONS] += 1;
        self.war[Self::ix(q, p)] = true;
        self.war[Self::ix(p, q)] = true;
        *self.rel_mut(q, p) &= !relbit::CLEARED_BY_WAR;
        *self.rel_mut(p, q) &= !relbit::CLEARED_BY_WAR;
        *self.tension_mut(q, p) = 0;
        *self.tension_mut(p, q) = 0;
        if reason == 0 && self.rec(p, q)[rec::HOSTILE_ACTS] == 0 {
            match self.attitude_class(env, p, q, 0) {
                0 => *self.war_counter_mut(p, q) += 0x3C,
                1 => *self.war_counter_mut(p, q) += 0x1E,
                _ => {}
            }
            *self.war_counter_mut(q, p) -= 30;
        }
        let mem = 8 * ((self.rec(p, q)[rec::DECLARATIONS] + self.rec(q, p)[rec::DECLARATIONS] + 1) / 2);
        if !self.human(q) {
            self.memory[Self::ix(q, p)] = mem;
        }
        if !self.human(p) {
            self.memory[Self::ix(p, q)] = mem;
        }
        self.settle_on_war(p, q);
        for b in 1..SLOTS as u32 {
            if b == p || b == q || !self.in_play(b) {
                continue;
            }
            if self.allies_vs(p, q) >> b & 1 != 0 {
                self.declare_war_inner(env, b, q, 0x22 + p as i32, false, calls);
            } else if self.allies_vs(q, p) >> b & 1 != 0 {
                self.declare_war_inner(env, b, p, 0x22 + q as i32, false, calls);
            } else if self.treaty(q, b) & treaty::MPP != 0 {
                self.declare_war_inner(env, b, p, 2 + q as i32, false, calls);
            }
        }
    }

    /// Cancel what is in force between the two; a package still running counts
    /// against the one who broke it.
    fn settle_on_war(&mut self, p: u32, q: u32) {
        let mut cancelled = 0;
        self.packages.retain(|k| {
            let between = (k.a == p && k.b == q) || (k.a == q && k.b == p);
            if between {
                cancelled += 1;
            }
            !between
        });
        self.rec_mut(q, p)[rec::DEALS_CANCELLED] += cancelled;
        for (a, b) in [(p, q), (q, p)] {
            let i = Self::ix(a, b);
            self.treaty[i] = 0;
        }
    }

    /// Make peace. Needs the pair to be at war.
    pub fn make_peace(&mut self, p: u32, q: u32) {
        if q == 0 || p == q || !self.at_war(p, q) {
            return;
        }
        for (a, b) in [(q, p), (p, q)] {
            let r = self.rec_mut(a, b);
            r[rec::ACC34] /= 2;
            r[rec::ACC38] = 0;
        }
        self.war[Self::ix(q, p)] = false;
        self.war[Self::ix(p, q)] = false;
        if !self.human(q) {
            self.memory[Self::ix(q, p)] = 8;
        }
        if !self.human(p) {
            self.memory[Self::ix(p, q)] = 8;
        }
        for b in 1..SLOTS as u32 {
            if b == p || b == q || !self.in_play(b) {
                continue;
            }
            for (x, y) in [(p, q), (q, p)] {
                let i = Self::ix(x, y);
                self.allies_vs[i] &= !(1 << b);
                let j = Self::ix(b, y);
                self.allies_vs[j] &= !(1 << x);
            }
        }
        for (x, y) in [(p, q), (q, p)] {
            let i = Self::ix(x, y);
            self.treaty[i] &= !treaty::ALLIANCE;
        }
    }

    /// The attitude of `p` towards `q`. Higher is more hostile. `flag = 0` halves
    /// a negative score when `q` ranks below.
    pub fn attitude(&self, env: &dyn Env, p: u32, q: u32, flag: i32) -> i32 {
        let r = self.rec(p, q);
        let war = self.at_war(p, q);
        let mut s = env.aggression(p).clamp(-2, 2);
        s += 4 * r[rec::DECLARATIONS]
            + (4 * r[rec::DEALS_CANCELLED]).min(8)
            + (8 * r[rec::C08]).min(16)
            + (2 * r[rec::ROP_VIOLATIONS]).min(4)
            + 4 * r[rec::HOSTILE_ACTS];
        s += 2 * r[rec::LOG18] - r[rec::LOG1C] + r[rec::LOG20] + r[rec::ATTACKS].min(10);
        s -= (r[rec::C24] / 10).min(10);
        s += 16 * (r[rec::VISIBLE_ATTACKER] + 2 * r[rec::HIDDEN_ATTACKER]) - r[rec::TECH_TRADES].min(1);
        s += r[rec::ACC34].min(10)
            + r[rec::ACC38].min(10)
            + 16 * r[rec::RAZED]
            + r[rec::C44]
            + r[rec::C48]
            - r[rec::C3C].min(10);
        s += env.nationals(p, q);
        if self.allies_vs(q, p) != 0 {
            s += 10;
        }
        if self.embargo(q, p) != 0 {
            s += if war { 2 } else { 10 };
        }
        let rb = self.rel(p, q);
        if war || rb & relbit::BORDER_WAR != 0 {
            s += 5;
        } else if rb & relbit::BORDER_WARNING != 0 {
            s += 1;
        }
        let (gp, gq) = (env.government(p), env.government(q));
        if gq != gp {
            s += if gq == env.shunned(p) { 5 } else { 1 };
        } else if gq == env.favorite(p) {
            s -= 5;
        }
        let (sp, sq) = (env.score(p), env.score(q));
        if sq < sp / 2 {
            s += 1;
        } else if sq > 2 * sp {
            s -= 1;
        }
        if self.embassy(p, q) {
            s += if war { 1 } else { -2 };
        }
        if env.culture_group(p) == env.culture_group(q) {
            s -= 1;
        }
        if env.shares_continent(p, q) {
            s -= 5;
        }
        let tw = self.treaty(p, q);
        if tw & (treaty::MPP | treaty::ALLIANCE) != 0 {
            s -= 10;
        }
        if tw & treaty::ROP != 0 {
            s -= 5;
        }
        s -= 2 * self.mutual_allies(p, q);
        for b in 1..SLOTS as u32 {
            if !self.in_play(b) || b == p || b == q || !self.at_war(p, b) {
                continue;
            }
            if self.at_war(q, b) {
                let rb = self.rec(b, q);
                s -= rb[rec::ACC38].min(1);
                s += -3 - rb[rec::HOSTILE_ACTS].min(2);
                s -= rb[rec::ATTACKS].min(4);
                s -= rb[rec::ACC34].min(5);
            } else if self.embargo(q, b) != 0 {
                s -= 2;
            }
        }
        for b in 1..SLOTS as u32 {
            if !self.in_play(b) || b == p || b == q || !self.contact(p, b) || self.at_war(p, b) {
                continue;
            }
            let rb = self.rec(b, q);
            s += rb[rec::DECLARATIONS].min(2)
                + rb[rec::DEALS_CANCELLED].min(2)
                + rb[rec::C08].min(3)
                + rb[rec::ROP_VIOLATIONS].min(1)
                + rb[rec::RAZED].min(8)
                + rb[rec::C44].min(1);
        }
        if s < 0 && war {
            s = 0;
        }
        if flag == 0 && s < 0 && env.rank(q) > env.rank(p) {
            s /= 2;
        }
        let (w, h) = env.map_size();
        let cap = (w + h) / 2;
        s.clamp(-cap, cap)
    }

    /// Civs allied with both `p` and `q`.
    fn mutual_allies(&self, p: u32, q: u32) -> i32 {
        (1..SLOTS as u32)
            .filter(|&b| {
                b != p
                    && b != q
                    && self.in_play(b)
                    && self.treaty(p, b) & treaty::ALLIANCE != 0
                    && self.treaty(q, b) & treaty::ALLIANCE != 0
            })
            .count() as i32
    }

    /// 0 friendliest .. 4 most hostile.
    pub fn attitude_class(&self, env: &dyn Env, p: u32, q: u32, flag: i32) -> i32 {
        let s = self.attitude(env, p, q, flag);
        let (w, h) = env.map_size();
        let m = (w + h) / 2 / 10;
        if s < -m {
            0
        } else if s > m {
            4
        } else if s < 0 {
            1
        } else if s > 0 {
            3
        } else {
            2
        }
    }

    /// Whether the AI declares war on a civ that provoked it. `die` is the draw
    /// `rand(32 - rank(q))`.
    pub fn wants_war(&self, env: &dyn Env, p: u32, q: u32, die: &mut dyn FnMut(u32) -> i32) -> bool {
        let n = self.in_play.count_ones() as i32;
        let mut b = env.aggression(p).clamp(-2, 2);
        match env.war_weariness(env.government(p)) {
            2 => b -= 2,
            1 => b -= 1,
            _ => {}
        }
        b += match self.attitude_class(env, p, q, 0) {
            0 => 1 - n,
            1 => -1,
            3 => 1,
            4 => n - 1,
            _ => 0,
        };
        let d = self.rec(p, q)[rec::ACC34] - self.rec(q, p)[rec::ACC34];
        if d > 0 {
            b -= d / env.cities(p).max(1);
        }
        b += die((32 - env.rank(q)).max(1) as u32);
        b >= 32 - env.rank(p)
    }

    /// The AI's peace move while at war: blocked by war memory and by the most
    /// hostile attitude.
    pub fn may_offer_peace(&self, env: &dyn Env, p: u32, q: u32) -> bool {
        self.at_war(p, q)
            && self.memory(p, q) == 0
            && self.memory(q, p) == 0
            && self.attitude_class(env, p, q, 0) != 4
            && self.embassy(p, q)
            && self.treaty(p, q) & treaty::ROP == 0
    }

    /// The gate of the AI's initiative outside war: `rand(T)` must be zero with
    /// `T = max(1, S - 1)`, one less when a game flag is set.
    pub fn initiative_gate(score: i32, flags_0x16: bool) -> u32 {
        let t = (score - 1).max(1) - i32::from(flags_0x16);
        t.max(1) as u32
    }

    /// A foreign unit's score accumulates as border pressure; `>= 0x200` sets
    /// the war track, otherwise the warning bit. Returns the relation bit set.
    pub fn border_pressure(&mut self, owner: u32, intruder: u32, pressure: i32) -> u32 {
        let bit = if pressure >= 0x200 {
            relbit::BORDER_WAR
        } else {
            relbit::BORDER_WARNING
        };
        *self.rel_mut(owner, intruder) |= bit;
        bit
    }

    /// Execute a clause between `a` (giver) and `b` (receiver). Returns the wars
    /// it started. Gold, advances, maps and cities are the caller's business.
    pub fn apply(&mut self, env: &dyn Env, a: u32, b: u32, clause: &Clause) -> Vec<WarCall> {
        let mut calls = Vec::new();
        match clause {
            Clause::Peace => self.make_peace(a, b),
            Clause::MutualProtection => {
                for (x, y) in [(a, b), (b, a)] {
                    let i = Self::ix(x, y);
                    self.treaty[i] |= treaty::MPP;
                }
            }
            Clause::RightOfPassage => {
                for (x, y) in [(a, b), (b, a)] {
                    let i = Self::ix(x, y);
                    self.treaty[i] |= treaty::ROP;
                }
            }
            Clause::MilitaryAlliance(t) => {
                let t = *t;
                for (x, y) in [(a, b), (b, a)] {
                    let i = Self::ix(x, t);
                    self.allies_vs[i] |= 1 << y;
                }
                for x in [a, b] {
                    self.declare_war_inner(env, x, t, 0x22 + (a + b - x) as i32, false, &mut calls);
                }
            }
            Clause::Embargo(v) => {
                for (x, y) in [(a, b), (b, a)] {
                    let i = Self::ix(x, *v);
                    self.embargo[i] |= 1 << y;
                }
            }
            Clause::Contact(c) => {
                self.establish_contact(b, *c);
            }
            Clause::WorldMap | Clause::Gold(_) | Clause::GoldPerTurn(_) | Clause::Tech(_) | Clause::City(_) => {}
        }
        calls
    }

    /// Record a package for the timed clauses of an executed deal.
    pub fn open_package(&mut self, a: u32, b: u32, clauses: &[Clause], turn: i32) {
        let timed: Vec<Clause> = clauses.iter().filter(|c| c.timed()).cloned().collect();
        if !timed.is_empty() {
            self.packages.push(Package { a, b, ends: turn + PACKAGE_TURNS, clauses: timed });
        }
    }

    /// Drop the packages that have run out; returns them.
    pub fn expire_packages(&mut self, turn: i32) -> Vec<Package> {
        let (gone, kept): (Vec<_>, Vec<_>) = self.packages.drain(..).partition(|k| k.ends <= turn);
        self.packages = kept;
        gone
    }

    /// Whether a package between the pair is still running.
    pub fn package_between(&self, a: u32, b: u32) -> bool {
        self.packages.iter().any(|k| (k.a == a && k.b == b) || (k.a == b && k.b == a))
    }

    /// Whether a clause may be offered between the pair.
    pub fn clause_valid(&self, a: u32, b: u32, clause: &Clause) -> bool {
        match clause {
            Clause::Peace => self.at_war(a, b),
            Clause::MutualProtection => {
                !self.at_war(a, b) && self.embassy(a, b) && self.treaty(a, b) & treaty::MPP == 0
            }
            Clause::RightOfPassage => {
                !self.at_war(a, b) && self.embassy(a, b) && self.treaty(a, b) & treaty::ROP == 0
            }
            Clause::MilitaryAlliance(t) => {
                !self.at_war(a, b)
                    && self.embassy(a, b)
                    && *t != a
                    && *t != b
                    && self.allies_vs(a, *t) >> b & 1 == 0
            }
            Clause::Embargo(v) => {
                !self.at_war(a, b)
                    && self.embassy(a, b)
                    && *v != a
                    && *v != b
                    && self.embargo(a, *v) >> b & 1 == 0
            }
            Clause::Contact(c) => *c != a && *c != b && self.contact(a, *c) && !self.contact(b, *c),
            _ => true,
        }
    }
}

/// The result of weighing a deal for an AI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Accepted.
    Accept,
    /// Nearly.
    WeakReject,
    /// Not interested.
    NeutralReject,
    /// No.
    StrongReject,
    /// The deal cannot be made.
    Invalid,
}

impl Verdict {
    /// The numeric code the binary returns.
    pub fn code(self) -> u32 {
        match self {
            Verdict::Accept => 36,
            Verdict::WeakReject => 37,
            Verdict::NeutralReject => 38,
            Verdict::StrongReject => 39,
            Verdict::Invalid => 40,
        }
    }

    /// The `DIPLOADVICETRADE_DEAL_*` key.
    pub fn key(self) -> Option<&'static str> {
        deal_response(self.code())
    }
}

/// Weigh a deal for AI `ai` against `other`: the offer (what `other` gives)
/// against the ask (what `ai` gives), with the ask scaled by the deals the pair
/// has cancelled. `value` prices one clause; a `None` price makes the deal
/// invalid.
pub fn weigh(
    rel: &Relations,
    ai: u32,
    other: u32,
    offer: &[Clause],
    ask: &[Clause],
    value: &mut dyn FnMut(&Clause, bool) -> Option<i32>,
) -> Verdict {
    let (mut sum_a, mut sum_b) = (0i32, 0i32);
    for c in offer {
        let Some(v) = value(c, true) else {
            return Verdict::Invalid;
        };
        sum_a = sum_a.saturating_add(v);
    }
    for c in ask {
        let Some(v) = value(c, false) else {
            return Verdict::Invalid;
        };
        sum_b = sum_b.saturating_add(v);
    }
    let t = rel.rec(ai, other)[rec::DEALS_CANCELLED];
    match deal_threshold_verdict(sum_a, attitude_scaled(sum_b, t)) {
        36 => Verdict::Accept,
        37 => Verdict::WeakReject,
        38 => Verdict::NeutralReject,
        _ => Verdict::StrongReject,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Flat;
    impl Env for Flat {
        fn aggression(&self, _: u32) -> i32 {
            0
        }
        fn government(&self, _: u32) -> i32 {
            1
        }
        fn shunned(&self, _: u32) -> i32 {
            -1
        }
        fn favorite(&self, _: u32) -> i32 {
            -1
        }
        fn culture_group(&self, _: u32) -> i32 {
            0
        }
        fn score(&self, _: u32) -> i32 {
            100
        }
        fn rank(&self, _: u32) -> i32 {
            1
        }
        fn shares_continent(&self, _: u32, _: u32) -> bool {
            false
        }
        fn nationals(&self, _: u32, _: u32) -> i32 {
            0
        }
        fn war_weariness(&self, _: i32) -> i32 {
            0
        }
        fn map_size(&self) -> (i32, i32) {
            (100, 100)
        }
        fn cities(&self, _: u32) -> i32 {
            5
        }
    }

    #[test]
    fn threshold_ladder_maps_to_the_verdicts() {
        assert_eq!(deal_threshold_verdict(100, 100), 36);
        assert_eq!(deal_threshold_verdict(90, 100), 37);
        assert_eq!(deal_threshold_verdict(60, 100), 38);
        assert_eq!(deal_threshold_verdict(10, 100), 39);
        assert_eq!(deal_response(36), Some("DIPLOADVICETRADE_DEAL_ACCEPT"));
        assert_eq!(deal_response(99), None);
    }

    #[test]
    fn contact_is_mutual_and_once() {
        let mut r = Relations::new(0b110, 0b010);
        assert!(r.establish_contact(1, 2));
        assert!(r.contact(1, 2) && r.contact(2, 1));
        assert!(!r.establish_contact(1, 2), "already met");
        // Slot 1 is the human, so the AI side (slot 2) marks AI_JUST_MET and
        // starts the greeting timer.
        assert_eq!(r.rel(2, 1) & relbit::AI_JUST_MET, relbit::AI_JUST_MET);
        assert_eq!(r.rel(1, 2) & relbit::AI_JUST_MET, 0);
        assert_eq!(r.greeting(2, 1), GREETING_TURNS, "AI met a human");
    }

    #[test]
    fn war_sets_both_sides_and_counts_a_declaration() {
        let env = Flat;
        let mut r = Relations::new(0b110, 0);
        let calls = r.declare_war(&env, 1, 2, 0, false);
        assert_eq!(calls.first(), Some(&WarCall { by: 1, on: 2, reason: 0 }));
        assert!(r.at_war(1, 2) && r.at_war(2, 1));
        assert_eq!(r.rec(2, 1)[rec::DECLARATIONS], 1);
        assert!(r.memory(1, 2) > 0, "the AI remembers a declaration");
    }

    #[test]
    fn peace_clears_the_war_and_halves_the_acc34() {
        let env = Flat;
        let mut r = Relations::new(0b110, 0);
        r.declare_war(&env, 1, 2, 0, false);
        r.rec_mut(1, 2)[rec::ACC34] = 10;
        r.make_peace(1, 2);
        assert!(!r.at_war(1, 2));
        assert_eq!(r.rec(1, 2)[rec::ACC34], 5);
        assert_eq!(r.memory(1, 2), 8, "AI memory resets to 8");
    }

    #[test]
    fn weighing_a_deal_needs_a_price_for_every_clause() {
        let r = Relations::new(0b110, 0);
        let mut price = |c: &Clause, _offer: bool| match c {
            Clause::Gold(g) => Some(*g),
            _ => None,
        };
        assert_eq!(weigh(&r, 1, 2, &[Clause::Gold(100)], &[Clause::Gold(50)], &mut price), Verdict::Accept);
        assert_eq!(weigh(&r, 1, 2, &[Clause::Gold(10)], &[Clause::Gold(50)], &mut price), Verdict::StrongReject);
        let mut nope = |_: &Clause, _: bool| None;
        assert_eq!(weigh(&r, 1, 2, &[Clause::WorldMap], &[], &mut nope), Verdict::Invalid);
    }
}
