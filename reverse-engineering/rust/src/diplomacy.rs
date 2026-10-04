//! Diplomacy: relation state, war and peace, the AI's attitude and war
//! decision, and the deal ladder. Specification: `../diplomacy.md`.
//!
//! Every per-pair table of the `Player` object is kept here under the offset
//! the binary uses, so a figure can be compared with the disassembly. `p` is
//! the slot of the player that owns the table and `q` the other civ; slot 0
//! is the barbarians and is never a diplomatic partner.
//!
//! | table | offset | meaning |
//! |---|---|---|
//! | [`Relations::at_war`] | `+0xD30[q]` | at-war byte |
//! | [`Relations::embassy`] | `+0xD50[q]` | communications / embassy byte |
//! | `rel` | `+0xEB0 + 4q` | relation word, see [`relbit`] |
//! | `treaty` | `+0xF30 + 4q` | treaty word, see [`treaty`] |
//! | `allies_vs` | `+0xFB0 + 4q` | civs allied with `p` against `q` |
//! | `embargo` | `+0x1030 + 4q` | civs embargoing `q` together with `p` |
//! | `greeting` | `+0xB30 + 4q` | the AI's greeting timer about a human |
//! | `memory` | `+0xBB0 + 4q` | the AI's war memory |
//! | `war_counter` | `+0xCB0 + 4q` | war counter (negative: winning) |
//! | `tension` | `+0xDB0 + 4q` | border tension |
//! | `rec` | `+0x1B0 + 0x4C q` | the pair record, 19 dwords, see [`rec`] |

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

/// Deal-response selector (`0x517B70`): the `0x440EE0` score maps to a
/// `DIPLOADVICETRADE_DEAL_*` script key; any other value falls through
/// with no dialog.
pub fn deal_response(score: u32) -> Option<&'static str> {
    match score {
        36 => Some("DIPLOADVICETRADE_DEAL_ACCEPT"),
        37 => Some("DIPLOADVICETRADE_DEAL_WEAKREJECT"),
        38 => Some("DIPLOADVICETRADE_DEAL_NEUTRALREJECT"),
        39 => Some("DIPLOADVICETRADE_DEAL_STRONGREJECT"),
        _ => None,
    }
}

/// Threshold ladder (`0x44198B-0x4419D6`): side-A total vs side-B total
/// yields the 36..39 verdict. Divisions are the exact `cdq`-and-shift
/// truncating sequences, which match Rust `/` on `i32`.
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

/// Attitude scaling (`0x441901-0x441931`): side-B total is multiplied
/// by `4*T+1` where `T` is the pair record's deals-cancelled word
/// ([`rec::DEALS_CANCELLED`], `0xA5304C`).
pub fn attitude_scaled(base: i32, t: i32) -> i32 {
    base.wrapping_mul(t.wrapping_mul(4).wrapping_add(1))
}

/// Scorer epilogue deltas (`0x441AA3-0x441AC5`): the `[esp+0x58]`
/// out-param takes offer-minus-ask, `[esp+0x5C]` takes the gated delta
/// (each write null-guarded in the binary; the arithmetic is exact).
pub fn scorer_deltas(offer: i32, ask: i32, gated_a: i32, gated_b: i32) -> (i32, i32) {
    (offer.wrapping_sub(ask), gated_a.wrapping_sub(gated_b))
}

/// Offsets (in dwords) inside the 19-dword pair record `P + 0x1B0 + 0x4C q`.
/// `rec(p, q)` is `p`'s book about `q`; the counters that say what `q` did to
/// `p` are kept in `p`'s book, the ones that say what `p` did to `q` in `q`'s.
pub mod rec {
    /// `+0x00` declarations of war by `q` against `p` (`0x502079` increments the
    /// *victim's* book about the declarer).
    pub const DECLARATIONS: usize = 0;
    /// `+0x04` deals cancelled (declaring war with a package in force); the
    /// scorer's `T` (`4T+1`).
    pub const DEALS_CANCELLED: usize = 1;
    /// `+0x08`.
    pub const C08: usize = 2;
    /// `+0x0C` right-of-passage violations.
    pub const ROP_VIOLATIONS: usize = 3;
    /// `+0x10` hostile acts (provocations).
    pub const HOSTILE_ACTS: usize = 4;
    /// `+0x14` attacks by `q`.
    pub const ATTACKS: usize = 5;
    /// `+0x18` diplomacy-log counter.
    pub const LOG18: usize = 6;
    /// `+0x1C` diplomacy-log counter.
    pub const LOG1C: usize = 7;
    /// `+0x20` diplomacy-log counter.
    pub const LOG20: usize = 8;
    /// `+0x24` goodwill: lowers the attitude by one per ten, at most ten (no
    /// writer located).
    pub const C24: usize = 9;
    /// `+0x28` tech trades.
    pub const TECH_TRADES: usize = 10;
    /// `+0x2C` hidden attacks.
    pub const HIDDEN_ATTACKER: usize = 11;
    /// `+0x30` visible attacks.
    pub const VISIBLE_ATTACKER: usize = 12;
    /// `+0x34` incident accumulator (halved at peace).
    pub const ACC34: usize = 13;
    /// `+0x38` incident accumulator (zeroed at peace).
    pub const ACC38: usize = 14;
    /// `+0x3C`.
    pub const C3C: usize = 15;
    /// `+0x40` razed cities.
    pub const RAZED: usize = 16;
    /// `+0x44`.
    pub const C44: usize = 17;
    /// `+0x48`.
    pub const C48: usize = 18;
    /// Dwords in a record.
    pub const LEN: usize = 19;
}

/// Bits of the relation word `+0xEB0 + 4q`.
pub mod relbit {
    /// Contact, set on both sides by `establishContact` and never cleared.
    pub const CONTACT: u32 = 0x1;
    /// Set by `establishContact` on the word of each **AI** side (never on a
    /// human's): the AI has just met this civ (`0x501E1D`, `0x501E9C`).
    pub const AI_JUST_MET: u32 = 0x2;
    /// Border warning (`0x446C18`).
    pub const BORDER_WARNING: u32 = 0x8;
    /// Border war track (`0x446C3B`).
    pub const BORDER_WAR: u32 = 0x10;
    /// Bits `declareWar` clears (`and 0xFFFFFFC1`).
    pub const CLEARED_BY_WAR: u32 = 0x3E;
}

/// Bits of the treaty word `+0xF30 + 4q`.
pub mod treaty {
    /// Mutual protection pact.
    pub const MPP: u32 = 1;
    /// Right of passage.
    pub const ROP: u32 = 2;
    /// Alliance (tested by the scorer and cleared at peace; no executor arm
    /// sets it).
    pub const ALLIANCE: u32 = 4;
}

/// The AI's greeting timer after meeting a human (`0x501ECF`, `0x501F0D`).
pub const GREETING_TURNS: i32 = 0x20;

/// Turns a timed package lasts (`[0xA526AC] + 0x14`).
pub const PACKAGE_TURNS: i32 = 20;

/// What the attitude and war decisions read from outside the diplomatic
/// state. The game fills it from its own world.
pub trait Env {
    /// `RACE` aggression for the AI (`0x53A0B0`, clamped to `-2..=2`).
    fn aggression(&self, p: u32) -> i32;
    /// The player's government row.
    fn government(&self, p: u32) -> i32;
    /// `RACE.shunned_government` of `p`'s civilization.
    fn shunned(&self, p: u32) -> i32;
    /// `RACE.favorite_government` of `p`'s civilization.
    fn favorite(&self, p: u32) -> i32;
    /// `RACE.culture_group` of `p`'s civilization.
    fn culture_group(&self, p: u32) -> i32;
    /// The player's score (`+0x183C`).
    fn score(&self, p: u32) -> i32;
    /// The player's rank (`+0x24`), 1 for the leader.
    fn rank(&self, p: u32) -> i32;
    /// `0x55E8E0`: `p` and `q` have cities on one continent.
    fn shares_continent(&self, p: u32, q: u32) -> bool;
    /// Units of `p` whose nationality is `q`'s civilization.
    fn nationals(&self, p: u32, q: u32) -> i32;
    /// `GOVT.war_weariness` of a government row.
    fn war_weariness(&self, government: i32) -> i32;
    /// Map width and height (`[0x9C74C0]`, `[0x9C74D4]`).
    fn map_size(&self) -> (i32, i32);
    /// Number of cities of `p` (`+0x194`).
    fn cities(&self, p: u32) -> i32;
}

/// `declareWar` made another civ join (a military-alliance call-in) or
/// started a war.
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

/// A timed group of clauses (`[0xA526AC] + 0x14`).
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

/// One item of a deal (the node's type and sub word, `0x502D90`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Clause {
    /// Type 0, sub 0: a peace treaty.
    Peace,
    /// Type 0, sub 1: mutual protection (tech flag `0x100`).
    MutualProtection,
    /// Type 0, sub 2: right of passage (tech flag `0x200`).
    RightOfPassage,
    /// Type 1: military alliance against a third civ (tech flag `0x400`).
    MilitaryAlliance(u32),
    /// Type 2: embargo of a civ (tech flag `0x800`).
    Embargo(u32),
    /// Type 3: the world map.
    WorldMap,
    /// Type 4: establish contact with a civ.
    Contact(u32),
    /// Type 7, sub 1: gold, once (`0x50331E`: the giver's treasury cells lose
    /// it, the receiver's gain it; the scorer prices it as the literal amount).
    Gold(i32),
    /// Type 7, sub 0: gold per turn for [`PACKAGE_TURNS`] turns (`0x5033E3`
    /// adds the amount to the per-turn deal table `0xA53CC8`).
    GoldPerTurn(i32),
    /// Type 8: an advance.
    Tech(i32),
    /// Type 9: a city, by an id the game understands.
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

    /// Counted in the gated subtotal of the scorer (`0x502D40`): treaties,
    /// alliances, embargoes, contact, gold, advances; not maps or cities.
    pub fn gated(&self) -> bool {
        !matches!(self, Clause::WorldMap | Clause::City(_))
    }

    /// Timed clauses start a package.
    pub fn timed(&self) -> bool {
        matches!(self, Clause::Peace | Clause::GoldPerTurn(_))
    }
}

/// The diplomatic state of every pair.
#[derive(Clone, Debug)]
pub struct Relations {
    /// `[0xA526C0]`: slots in play.
    pub in_play: u32,
    /// `[0xA526BC]`: human slots.
    pub human: u32,
    war: Vec<bool>,
    embassy: Vec<bool>,
    rel: Vec<u32>,
    treaty: Vec<u32>,
    allies_vs: Vec<u32>,
    embargo: Vec<u32>,
    /// The AI's war memory `+0xBB0`.
    memory: Vec<i32>,
    greeting: Vec<i32>,
    war_counter: Vec<i32>,
    tension: Vec<i32>,
    rec: Vec<[i32; rec::LEN]>,
    /// Timed packages in force.
    pub packages: Vec<Package>,
}

impl Clause {
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

impl Relations {
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

    /// Put saved books back. False, with `self` untouched, when the words do
    /// not fit.
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
        let Some((in_play, human, war, embassy, mut us, mut is, rec, packages)) = parsed else { return false };
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

    fn ix(p: u32, q: u32) -> usize {
        p as usize * SLOTS + q as usize
    }

    /// `+0xD30[q]`.
    pub fn at_war(&self, p: u32, q: u32) -> bool {
        self.war[Self::ix(p, q)]
    }

    /// `+0xD50[q]`.
    pub fn embassy(&self, p: u32, q: u32) -> bool {
        self.embassy[Self::ix(p, q)]
    }

    /// `+0xEB0 + 4q`.
    pub fn rel(&self, p: u32, q: u32) -> u32 {
        self.rel[Self::ix(p, q)]
    }

    /// Mutable relation word.
    pub fn rel_mut(&mut self, p: u32, q: u32) -> &mut u32 {
        &mut self.rel[Self::ix(p, q)]
    }

    /// `+0xF30 + 4q`.
    pub fn treaty(&self, p: u32, q: u32) -> u32 {
        self.treaty[Self::ix(p, q)]
    }

    /// `+0xFB0 + 4q`: civs allied with `p` against `q`.
    pub fn allies_vs(&self, p: u32, q: u32) -> u32 {
        self.allies_vs[Self::ix(p, q)]
    }

    /// `+0x1030 + 4q`: civs embargoing `q` together with `p`.
    pub fn embargo(&self, p: u32, q: u32) -> u32 {
        self.embargo[Self::ix(p, q)]
    }

    /// `+0xBB0 + 4q`.
    pub fn memory(&self, p: u32, q: u32) -> i32 {
        self.memory[Self::ix(p, q)]
    }

    /// `+0xCB0 + 4q`.
    pub fn war_counter(&self, p: u32, q: u32) -> i32 {
        self.war_counter[Self::ix(p, q)]
    }

    /// Mutable war counter.
    pub fn war_counter_mut(&mut self, p: u32, q: u32) -> &mut i32 {
        &mut self.war_counter[Self::ix(p, q)]
    }

    /// `+0xDB0 + 4q`.
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

    fn human(&self, p: u32) -> bool {
        self.human >> p & 1 != 0
    }

    fn in_play(&self, p: u32) -> bool {
        self.in_play >> p & 1 != 0
    }

    /// The pair has met (relation bit 0).
    pub fn contact(&self, p: u32, q: u32) -> bool {
        self.rel(p, q) & relbit::CONTACT != 0
    }

    /// `canTalk` (`0x501910`): `(embassy && !war) || espionage channel`
    /// (`+0xD70`, which the clone has none of).
    pub fn can_talk(&self, p: u32, q: u32) -> bool {
        self.embassy(p, q) && !self.at_war(p, q)
    }

    /// `establishContact` (`0x501CD0`): set on both sides, never cleared.
    /// Returns whether the pair had not met. Each AI side also gets
    /// [`relbit::AI_JUST_MET`] (`0x501E1D`, `0x501E9C`), and an AI that meets
    /// a human starts a greeting timer of `0x20` about it (`0x501ECF`,
    /// `0x501F0D`; what reads it is not decoded).
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

    /// `+0xB30 + 4q`: the AI's greeting timer about a human it has met.
    pub fn greeting(&self, p: u32, q: u32) -> i32 {
        self.greeting[Self::ix(p, q)]
    }

    /// `0x56B7D0` stores the embassy byte for one direction.
    pub fn set_embassy(&mut self, p: u32, q: u32) {
        let i = Self::ix(p, q);
        self.embassy[i] = true;
    }

    /// Both directions at once (the first-contact conversation).
    pub fn exchange_embassies(&mut self, p: u32, q: u32) {
        self.set_embassy(p, q);
        self.set_embassy(q, p);
    }

    /// `declareWar(P; q, reason)` (`0x501F20`), including the call-in of
    /// every civ allied through [`Relations::allies_vs`]. `env` supplies the
    /// facts the attitude reads. `rop_violation` is the result of the unit
    /// scan at `0x501FA2` (a `p` unit inside `q`'s territory while a right of
    /// passage or tension `>= 0x200` is in force). Returns every declaration
    /// made, the first being `p` on `q`.
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

    #[allow(clippy::too_many_arguments)]
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
        // 0x501F5A: a right of passage, or tension >= 0x200, makes the unit
        // scan count a violation on the victim's book.
        let rop = self.treaty(p, q) & treaty::ROP != 0;
        if (rop || self.tension(q, p) >= 0x200) && rop_violation {
            self.rec_mut(q, p)[rec::ROP_VIOLATIONS] += 1;
        }
        // 0x502098: the victim counts one more declaration by `p`.
        self.rec_mut(q, p)[rec::DECLARATIONS] += 1;
        self.war[Self::ix(q, p)] = true;
        self.war[Self::ix(p, q)] = true;
        // 0x502111: the war bits of both relation words go, as do the tensions.
        *self.rel_mut(q, p) &= !relbit::CLEARED_BY_WAR;
        *self.rel_mut(p, q) &= !relbit::CLEARED_BY_WAR;
        *self.tension_mut(q, p) = 0;
        *self.tension_mut(p, q) = 0;
        // 0x502167 (reason) and 0x50216F (hostile acts) gate both halves: an
        // unprovoked, plain declaration costs the declarer's mood when the
        // victim was a friend (0x502188, 0x502199), and the victim counts the
        // war as won so far (0x5021BD, `add -30`).
        if reason == 0 && self.rec(p, q)[rec::HOSTILE_ACTS] == 0 {
            match self.attitude_class(env, p, q, 0) {
                0 => *self.war_counter_mut(p, q) += 0x3C,
                1 => *self.war_counter_mut(p, q) += 0x1E,
                _ => {}
            }
            *self.war_counter_mut(q, p) -= 30;
        }
        // The AI's war memory (0x5021E1, 0x502238): 8 * ((a + b + 1) / 2),
        // `a` and `b` the two declaration counters.
        let mem = 8 * ((self.rec(p, q)[rec::DECLARATIONS] + self.rec(q, p)[rec::DECLARATIONS] + 1) / 2);
        if !self.human(q) {
            self.memory[Self::ix(q, p)] = mem;
        }
        if !self.human(p) {
            self.memory[Self::ix(p, q)] = mem;
        }
        // 0x500830: the packages between the two lapse; the victim notes it.
        self.settle_on_war(p, q);
        // 0x5024DB: allies of either side join. `p`'s allies against `q` come
        // in as military allies of `p`; `q`'s allies against `p` likewise.
        for b in 1..SLOTS as u32 {
            if b == p || b == q || !self.in_play(b) {
                continue;
            }
            if self.allies_vs(p, q) >> b & 1 != 0 {
                self.declare_war_inner(env, b, q, 0x22 + p as i32, false, calls);
            } else if self.allies_vs(q, p) >> b & 1 != 0 {
                self.declare_war_inner(env, b, p, 0x22 + q as i32, false, calls);
            } else if self.treaty(q, b) & treaty::MPP != 0 {
                // A pact partner of the victim defends it (reason `2 + q`).
                self.declare_war_inner(env, b, p, 2 + q as i32, false, calls);
            }
        }
    }

    /// `0x500830(P; q)`: cancel what is in force between the two. A package
    /// still running counts against the one who broke it
    /// ([`rec::DEALS_CANCELLED`] on the victim's book).
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

    /// `makePeace(P; q)` (`0x5025B0`). Needs the pair to be at war.
    pub fn make_peace(&mut self, p: u32, q: u32) {
        if q == 0 || p == q || !self.at_war(p, q) {
            return;
        }
        // 0x5025F9: halve the 0x34 accumulators, zero the 0x38 ones.
        for (a, b) in [(q, p), (p, q)] {
            let r = self.rec_mut(a, b);
            r[rec::ACC34] /= 2;
            r[rec::ACC38] = 0;
        }
        self.war[Self::ix(q, p)] = false;
        self.war[Self::ix(p, q)] = false;
        // 0x5026A9: an AI side's war memory is reset to 8, not to zero.
        if !self.human(q) {
            self.memory[Self::ix(q, p)] = 8;
        }
        if !self.human(p) {
            self.memory[Self::ix(p, q)] = 8;
        }
        // 0x5027D5: alliances against the other side end, and so does an
        // alliance treaty between the two.
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

    /// The attitude of `p` towards `q` (`0x440100(P; q, flag)`). Higher is
    /// more hostile. `flag = 0` halves a negative score when `q` ranks below.
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
        s += r[rec::ACC34].min(10) + r[rec::ACC38].min(10) + 16 * r[rec::RAZED] + r[rec::C44] + r[rec::C48]
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
        // The flavor mix: with the default type 4 the blend is the identity
        // (0x440A4F), so the term is the plain sum over third civs.
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

    /// `0x501020`: civs allied with both `p` and `q` (the alliance bit of the
    /// treaty word towards each).
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

    /// `0x440AD0`: 0 friendliest .. 4 most hostile.
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

    /// `0x440B60(P; q)`: whether the AI declares war on a civ that provoked
    /// it (called from the provoke tail `0x502CC0`). `die` is the draw
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

    /// The AI's peace move while at war (`0x43E470`, 3.1): blocked by war
    /// memory and by the most hostile attitude; the caller still draws
    /// `rand(4) == 0`.
    pub fn may_offer_peace(&self, env: &dyn Env, p: u32, q: u32) -> bool {
        self.at_war(p, q)
            && self.memory(p, q) == 0
            && self.memory(q, p) == 0
            && self.attitude_class(env, p, q, 0) != 4
            && self.embassy(p, q)
            && self.treaty(p, q) & treaty::ROP == 0
    }

    /// The gate of the AI's initiative outside war (`0x43E87E`): `rand(T)` must
    /// be zero with `T = max(1, S - 1)`, one less when a game flag in
    /// `0x16` is set.
    pub fn initiative_gate(score: i32, flags_0x16: bool) -> u32 {
        let t = (score - 1).max(1) - i32::from(flags_0x16);
        t.max(1) as u32
    }

    /// `0x446C1B`: a foreign unit's score accumulates as border pressure;
    /// `>= 0x200` sets the war track, otherwise the warning bit. Returns the
    /// relation bit set.
    pub fn border_pressure(&mut self, owner: u32, intruder: u32, pressure: i32) -> u32 {
        let bit = if pressure >= 0x200 { relbit::BORDER_WAR } else { relbit::BORDER_WARNING };
        *self.rel_mut(owner, intruder) |= bit;
        bit
    }

    /// Execute a clause between `a` (giver) and `b` (receiver). Returns the
    /// wars it started. Gold, advances, maps and cities are the caller's
    /// business; the treaty state is done here.
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

    /// Whether a clause may be offered between the pair: `canOffer*`
    /// (`0x501950..0x501B80`) wants an embassy for the treaties, and the
    /// advance flag (`knowsTechWithFlags`) decided by the caller.
    pub fn clause_valid(&self, a: u32, b: u32, clause: &Clause) -> bool {
        match clause {
            Clause::Peace => self.at_war(a, b),
            Clause::MutualProtection => !self.at_war(a, b) && self.embassy(a, b) && self.treaty(a, b) & treaty::MPP == 0,
            Clause::RightOfPassage => !self.at_war(a, b) && self.embassy(a, b) && self.treaty(a, b) & treaty::ROP == 0,
            Clause::MilitaryAlliance(t) => {
                !self.at_war(a, b)
                    && self.embassy(a, b)
                    && *t != a
                    && *t != b
                    && self.allies_vs(a, *t) >> b & 1 == 0
            }
            Clause::Embargo(v) => !self.at_war(a, b) && self.embassy(a, b) && *v != a && *v != b && self.embargo(a, *v) >> b & 1 == 0,
            Clause::Contact(c) => *c != a && *c != b && self.contact(a, *c) && !self.contact(b, *c),
            _ => true,
        }
    }
}

/// The result of weighing a deal for an AI (`0x440EE0`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// 36: accepted.
    Accept,
    /// 37: nearly.
    WeakReject,
    /// 38: not interested.
    NeutralReject,
    /// 39: no.
    StrongReject,
    /// 40: the deal cannot be made.
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
/// against the ask (what `ai` gives), with the ask scaled by the deals the
/// pair has cancelled. `value` prices one clause; a `None` price makes the
/// deal invalid. This is the structure of `0x440EE0`; the prices are the
/// caller's (see `diplomacy.md` for which are decoded).
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
        let Some(v) = value(c, true) else { return Verdict::Invalid };
        sum_a = sum_a.saturating_add(v);
    }
    for c in ask {
        let Some(v) = value(c, false) else { return Verdict::Invalid };
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

    #[test]
    fn relations_round_trip_through_words() {
        let mut r = Relations::new(0b111, 0b1);
        r.war[Relations::ix(0, 1)] = true;
        r.embassy[Relations::ix(1, 2)] = true;
        r.rel[Relations::ix(2, 0)] = 5;
        r.tension[Relations::ix(1, 0)] = -3;
        r.rec[Relations::ix(0, 2)][3] = 7;
        r.packages.push(Package { a: 0, b: 2, ends: 40, clauses: vec![Clause::Peace, Clause::GoldPerTurn(6), Clause::Embargo(1), Clause::City(9)] });
        let words = r.to_words();
        let mut fresh = Relations::new(1, 1);
        assert!(fresh.restore(&words));
        assert_eq!(fresh.to_words(), words);
        assert!(fresh.at_war(0, 1) && fresh.embassy(1, 2) && fresh.rel(2, 0) == 5);
        assert_eq!(fresh.packages, r.packages);
        assert!(!fresh.restore(&words[..words.len() - 2]));
    }

    /// A fixed world: all civs alike unless a test changes the fields.
    struct World {
        aggression: i32,
        gov: [i32; SLOTS],
        rank: [i32; SLOTS],
        score: [i32; SLOTS],
        continent: bool,
        weariness: i32,
    }

    impl World {
        fn new() -> Self {
            World { aggression: 0, gov: [1; SLOTS], rank: [1; SLOTS], score: [100; SLOTS], continent: false, weariness: 0 }
        }
    }

    impl Env for World {
        fn aggression(&self, _: u32) -> i32 {
            self.aggression
        }
        fn government(&self, p: u32) -> i32 {
            self.gov[p as usize]
        }
        fn shunned(&self, _: u32) -> i32 {
            4
        }
        fn favorite(&self, _: u32) -> i32 {
            7
        }
        fn culture_group(&self, p: u32) -> i32 {
            p as i32
        }
        fn score(&self, p: u32) -> i32 {
            self.score[p as usize]
        }
        fn rank(&self, p: u32) -> i32 {
            self.rank[p as usize]
        }
        fn shares_continent(&self, _: u32, _: u32) -> bool {
            self.continent
        }
        fn nationals(&self, _: u32, _: u32) -> i32 {
            0
        }
        fn war_weariness(&self, _: i32) -> i32 {
            self.weariness
        }
        fn map_size(&self) -> (i32, i32) {
            (60, 60)
        }
        fn cities(&self, _: u32) -> i32 {
            4
        }
    }

    fn rel() -> Relations {
        // Civs 1..=4 in play, civ 1 human.
        Relations::new(0b11110, 0b10)
    }

    #[test]
    fn strides_match_disassembly() {
        // Byte strides from the lea/shl immediates.
        assert_eq!(ROW_STRIDE * 4, 8420);
        assert_eq!(COL_STRIDE * 4, 76);
        assert_eq!(relation_index(0, 0), 0);
        assert_eq!(relation_index(1, 0), 2105);
        assert_eq!(relation_index(0, 1), 19);
        assert_eq!(relation_index(2, 3), 2 * 2105 + 3 * 19);
    }

    #[test]
    fn deal_response_words_match_selector() {
        assert_eq!(deal_response(36), Some("DIPLOADVICETRADE_DEAL_ACCEPT"));
        assert_eq!(deal_response(37), Some("DIPLOADVICETRADE_DEAL_WEAKREJECT"));
        assert_eq!(deal_response(38), Some("DIPLOADVICETRADE_DEAL_NEUTRALREJECT"));
        assert_eq!(deal_response(39), Some("DIPLOADVICETRADE_DEAL_STRONGREJECT"));
        assert_eq!(deal_response(35), None);
        assert_eq!(deal_response(40), None);
        assert_eq!(deal_response(0), None);
    }

    #[test]
    fn threshold_ladder_matches_disassembly() {
        // A >= B accepts, even at zero.
        assert_eq!(deal_threshold_verdict(100, 100), 36);
        assert_eq!(deal_threshold_verdict(100, 90), 36);
        assert_eq!(deal_threshold_verdict(0, 0), 36);
        // Boundary trunc(B*7/8) = trunc(700/8) = 87.
        assert_eq!(deal_threshold_verdict(90, 100), 37);
        assert_eq!(deal_threshold_verdict(88, 100), 37);
        assert_eq!(deal_threshold_verdict(87, 100), 38);
        // Boundary trunc(B/2) = 50.
        assert_eq!(deal_threshold_verdict(51, 100), 38);
        assert_eq!(deal_threshold_verdict(50, 100), 39);
        assert_eq!(deal_threshold_verdict(0, 100), 39);
        // Odd B truncates: trunc(7*7/8) = 6.
        assert_eq!(deal_threshold_verdict(7, 7), 36);
        assert_eq!(deal_threshold_verdict(6, 7), 38);
        // Full chain: ladder output selects the dialog word.
        let v = deal_threshold_verdict(90, 100);
        assert_eq!(deal_response(v), Some("DIPLOADVICETRADE_DEAL_WEAKREJECT"));
        assert_eq!(deal_response(40), None);
        assert_eq!(deal_response(44), None);
    }

    #[test]
    fn attitude_scale_is_four_t_plus_one() {
        assert_eq!(attitude_scaled(100, 0), 100);
        assert_eq!(attitude_scaled(100, 2), 900);
        assert_eq!(attitude_scaled(100, 1), 500);
    }

    #[test]
    fn epilogue_writes_both_deltas() {
        // 0x441AA3: [esp+0x58] = offer - ask; 0x441ABB: [esp+0x5C] = gated_a - gated_b.
        assert_eq!(scorer_deltas(120, 100, 30, 10), (20, 20));
        assert_eq!(scorer_deltas(50, 100, 0, 25), (-50, -25));
    }

    #[test]
    fn bump_increments_one_cell() {
        let mut m = vec![0u32; (2 * ROW_STRIDE + COL_STRIDE) as usize];
        assert_eq!(bump_relation(&mut m, 1, 1), 1);
        assert_eq!(bump_relation(&mut m, 1, 1), 2);
        assert_eq!(m[relation_index(1, 0) as usize], 0);
        assert_eq!(m[relation_index(0, 1) as usize], 0);
    }

    #[test]
    fn contact_is_symmetric_and_marks_the_ai_sides() {
        let mut r = rel();
        assert!(!r.contact(1, 2));
        assert!(r.establish_contact(1, 2));
        assert!(r.contact(1, 2) && r.contact(2, 1));
        // The AI side is marked and starts a greeting timer; the human is not.
        assert_ne!(r.rel(2, 1) & relbit::AI_JUST_MET, 0);
        assert_eq!(r.rel(1, 2) & relbit::AI_JUST_MET, 0);
        assert_eq!((r.greeting(2, 1), r.greeting(1, 2)), (GREETING_TURNS, 0));
        // Two AI sides are both marked, with no timer.
        assert!(r.establish_contact(2, 3));
        assert_ne!(r.rel(2, 3) & r.rel(3, 2) & relbit::AI_JUST_MET, 0);
        assert_eq!(r.greeting(2, 3), 0);
        assert!(!r.establish_contact(2, 1), "a second meeting is not new");
        assert!(!r.establish_contact(1, 0), "the barbarians are nobody's contact");
    }

    #[test]
    fn contact_alone_is_not_war_and_talking_needs_an_embassy() {
        let mut r = rel();
        r.establish_contact(1, 2);
        assert!(!r.at_war(1, 2) && !r.at_war(2, 1));
        assert!(!r.can_talk(1, 2));
        r.exchange_embassies(1, 2);
        assert!(r.can_talk(1, 2) && r.can_talk(2, 1));
    }

    #[test]
    fn declaring_war_books_it_on_the_victim() {
        let (mut r, w) = (rel(), World::new());
        let calls = r.declare_war(&w, 2, 3, 0, false);
        assert_eq!(calls, vec![WarCall { by: 2, on: 3, reason: 0 }]);
        assert!(r.at_war(2, 3) && r.at_war(3, 2));
        assert!(r.contact(2, 3), "declaring war makes the contact");
        assert_eq!(r.rec(3, 2)[rec::DECLARATIONS], 1, "the victim counts it");
        assert_eq!(r.rec(2, 3)[rec::DECLARATIONS], 0);
        assert_eq!(r.war_counter(3, 2), -30, "the victim is ahead");
        // The AI victim and the AI declarer both remember: 8 * ((1 + 0 + 1) / 2).
        assert_eq!(r.memory(3, 2), 8);
        assert_eq!(r.memory(2, 3), 8);
        // A human victim keeps no AI memory.
        r.declare_war(&w, 2, 1, 0, false);
        assert_eq!(r.memory(1, 2), 0);
        // A second declaration is a no-op.
        assert!(r.declare_war(&w, 3, 2, 0, false).is_empty());
    }

    #[test]
    fn war_clears_the_relation_bits_and_tension() {
        let (mut r, w) = (rel(), World::new());
        r.establish_contact(1, 2);
        *r.rel_mut(1, 2) |= relbit::BORDER_WAR | relbit::BORDER_WARNING;
        *r.tension_mut(1, 2) = 0x300;
        r.declare_war(&w, 2, 1, 0, false);
        assert_eq!(r.rel(1, 2) & (relbit::BORDER_WAR | relbit::BORDER_WARNING), 0);
        assert_eq!(r.rel(2, 1) & relbit::AI_JUST_MET, 0, "war clears bits 1..5");
        assert_eq!(r.rel(1, 2) & relbit::CONTACT, relbit::CONTACT, "contact is never cleared");
        assert_eq!(r.tension(1, 2), 0);
    }

    #[test]
    fn an_unprovoked_war_on_a_friend_weighs_on_the_declarer() {
        let (mut r, mut w) = (rel(), World::new());
        // Culture-group mismatch (-0) and a continent (-5) and an embassy
        // (-2) put p at a friendly class towards q.
        w.continent = true;
        r.exchange_embassies(2, 3);
        let before = r.attitude_class(&w, 2, 3, 0);
        assert!(before <= 1, "friendly: class {before}");
        r.declare_war(&w, 2, 3, 0, false);
        // The class is read after the war flag is set (0x50217E); the
        // declarer's counter rose by 0x3C or 0x1E only for classes 0 and 1.
        let after_class = r.attitude_class(&w, 2, 3, 0);
        let counter = r.war_counter(2, 3);
        assert!(matches!(counter, 0 | 0x1E | 0x3C), "counter {counter}, class {after_class}");
        // A provoked declaration never pays it.
        let (mut r, w) = (rel(), World::new());
        r.rec_mut(2, 3)[rec::HOSTILE_ACTS] = 1;
        r.declare_war(&w, 2, 3, 0, false);
        assert_eq!(r.war_counter(2, 3), 0);
        // Nor does the victim get its relief of 30 (0x502167 skips both).
        assert_eq!(r.war_counter(3, 2), 0);
        // A call to arms (a reason word) is not a plain declaration either.
        let (mut r, w) = (rel(), World::new());
        r.declare_war(&w, 2, 3, 2 + 4, false);
        assert_eq!((r.war_counter(2, 3), r.war_counter(3, 2)), (0, 0));
    }

    #[test]
    fn a_military_alliance_pulls_the_allies_in() {
        let (mut r, w) = (rel(), World::new());
        // 2 and 4 are allied against 3, then 3 declares war on 2.
        r.apply(&w, 2, 4, &Clause::MilitaryAlliance(1));
        let mut r = rel();
        r.allies_vs[Relations::ix(2, 3)] |= 1 << 4;
        let calls = r.declare_war(&w, 2, 3, 0, false);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[1], WarCall { by: 4, on: 3, reason: 0x22 + 2 });
        assert!(r.at_war(4, 3));
        // The victim's allies against the declarer join too.
        let mut r = rel();
        r.allies_vs[Relations::ix(3, 2)] |= 1 << 1;
        let calls = r.declare_war(&w, 2, 3, 0, false);
        assert_eq!(calls[1], WarCall { by: 1, on: 2, reason: 0x22 + 3 });
        assert!(r.at_war(1, 2));
    }

    #[test]
    fn a_mutual_protection_pact_defends_the_victim() {
        let (mut r, w) = (rel(), World::new());
        r.exchange_embassies(3, 4);
        r.apply(&w, 3, 4, &Clause::MutualProtection);
        let calls = r.declare_war(&w, 2, 3, 0, false);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[1], WarCall { by: 4, on: 2, reason: 2 + 3 });
        assert!(r.at_war(4, 2));
        // The declarer's own partners are not dragged in.
        let (mut r, w) = (rel(), World::new());
        r.apply(&w, 2, 4, &Clause::MutualProtection);
        let calls = r.declare_war(&w, 2, 3, 0, false);
        assert_eq!(calls.len(), 1);
    }

    #[test]
    fn peace_halves_zeroes_and_resets_the_memory() {
        let (mut r, w) = (rel(), World::new());
        r.declare_war(&w, 2, 3, 0, false);
        r.rec_mut(2, 3)[rec::ACC34] = 7;
        r.rec_mut(3, 2)[rec::ACC34] = 9;
        r.rec_mut(2, 3)[rec::ACC38] = 4;
        r.rec_mut(3, 2)[rec::ACC38] = 4;
        r.make_peace(2, 3);
        assert!(!r.at_war(2, 3) && !r.at_war(3, 2));
        assert_eq!((r.rec(2, 3)[rec::ACC34], r.rec(3, 2)[rec::ACC34]), (3, 4));
        assert_eq!((r.rec(2, 3)[rec::ACC38], r.rec(3, 2)[rec::ACC38]), (0, 0));
        assert_eq!((r.memory(2, 3), r.memory(3, 2)), (8, 8));
        // Peace needs a war.
        r.rec_mut(2, 3)[rec::ACC34] = 7;
        r.make_peace(2, 3);
        assert_eq!(r.rec(2, 3)[rec::ACC34], 7);
    }

    #[test]
    fn breaking_a_package_counts_against_the_breaker() {
        let (mut r, w) = (rel(), World::new());
        r.declare_war(&w, 2, 3, 0, false);
        r.open_package(2, 3, &[Clause::Peace], 1);
        r.make_peace(2, 3);
        assert!(r.package_between(2, 3));
        assert_eq!(r.packages[0].ends, 21);
        r.declare_war(&w, 2, 3, 0, false);
        assert!(!r.package_between(2, 3));
        assert_eq!(r.rec(3, 2)[rec::DEALS_CANCELLED], 1);
        // T = 1 scales the ask by 5.
        assert_eq!(attitude_scaled(100, r.rec(3, 2)[rec::DEALS_CANCELLED]), 500);
    }

    #[test]
    fn packages_lapse_after_twenty_turns() {
        let mut r = rel();
        r.open_package(2, 3, &[Clause::Peace, Clause::WorldMap], 10);
        assert_eq!(r.packages[0].clauses, vec![Clause::Peace]);
        assert!(r.expire_packages(29).is_empty());
        assert_eq!(r.expire_packages(30).len(), 1);
        assert!(!r.package_between(2, 3));
        // Untimed deals open nothing.
        r.open_package(2, 3, &[Clause::WorldMap, Clause::Gold(5)], 10);
        assert!(r.packages.is_empty());
    }

    #[test]
    fn the_attitude_accumulates_the_book() {
        let (mut r, w) = (rel(), World::new());
        // Two civs with nothing between them: culture groups differ (0),
        // same government (0), no embassy (0), equal scores: neutral.
        assert_eq!(r.attitude(&w, 2, 3, 0), 0);
        assert_eq!(r.attitude_class(&w, 2, 3, 0), 2);
        r.rec_mut(2, 3)[rec::DECLARATIONS] = 1;
        r.rec_mut(2, 3)[rec::HOSTILE_ACTS] = 1;
        assert_eq!(r.attitude(&w, 2, 3, 0), 4 + 4);
        // Capped terms.
        r.rec_mut(2, 3)[rec::DEALS_CANCELLED] = 5;
        assert_eq!(r.attitude(&w, 2, 3, 0), 4 + 8 + 4, "4*5 is capped at 8");
        // The classes at map 60x60: m = 6.
        assert_eq!(r.attitude_class(&w, 2, 3, 0), 4);
        let mut w = World::new();
        w.aggression = 2;
        let r = rel();
        assert_eq!(r.attitude(&w, 2, 3, 0), 2);
        assert_eq!(r.attitude_class(&w, 2, 3, 0), 3);
    }

    #[test]
    fn friendly_circumstances_lower_the_attitude() {
        let (mut r, mut w) = (rel(), World::new());
        r.exchange_embassies(2, 3);
        assert_eq!(r.attitude(&w, 2, 3, 0), -2, "an embassy in peace");
        w.continent = true;
        assert_eq!(r.attitude(&w, 2, 3, 0), -7);
        // Negative halves for a lower-ranked q only when flag is 0: with
        // rank(q) > rank(p): -7 / 2 = -3 (toward zero).
        w.rank[3] = 2;
        assert_eq!(r.attitude(&w, 2, 3, 0), -3);
        assert_eq!(r.attitude(&w, 2, 3, 1), -7);
        // A treaty of passage: -5 more.
        let i = Relations::ix(2, 3);
        r.treaty[i] |= treaty::ROP;
        assert_eq!(r.attitude(&w, 2, 3, 1), -12);
    }

    #[test]
    fn war_floors_the_attitude_at_zero_and_adds_five() {
        let (mut r, mut w) = (rel(), World::new());
        r.exchange_embassies(2, 3);
        w.continent = true;
        r.declare_war(&w, 2, 3, 0, false);
        // war +5, embassy at war +1, continent -5, same government: 1.
        assert_eq!(r.attitude(&w, 2, 3, 0), 1);
        let mut r2 = rel();
        r2.exchange_embassies(2, 3);
        r2.war[Relations::ix(2, 3)] = true;
        w.continent = true;
        w.gov[3] = 4; // the shunned government: +5
        assert!(r2.attitude(&w, 2, 3, 0) >= 0);
    }

    #[test]
    fn governments_shape_the_attitude() {
        let (r, mut w) = (rel(), World::new());
        w.gov[3] = 2; // different, not shunned: +1
        assert_eq!(r.attitude(&w, 2, 3, 0), 1);
        w.gov[3] = 4; // shunned: +5
        assert_eq!(r.attitude(&w, 2, 3, 0), 5);
        w.gov[2] = 7;
        w.gov[3] = 7; // same and favorite: -5
        assert_eq!(r.attitude(&w, 2, 3, 0), -5, "equal ranks, so no halving");
    }

    #[test]
    fn third_party_enemies_make_friends() {
        let (mut r, w) = (rel(), World::new());
        r.declare_war(&w, 2, 4, 0, false);
        let base = r.attitude(&w, 2, 3, 0);
        r.declare_war(&w, 3, 4, 0, false);
        // Both at war with 4: -3 (and the fresh book of 4 about 3 adds none).
        let now = r.attitude(&w, 2, 3, 0);
        assert_eq!(now - base, -3);
    }

    #[test]
    fn the_war_decision_uses_aggression_class_and_a_die() {
        let (r, mut w) = (rel(), World::new());
        // Aggression 2 makes the attitude 2 (class 3, +1): b = 3 + die, and
        // a rank-1 civ needs 32 - 1 = 31.
        w.aggression = 2;
        assert!(r.wants_war(&w, 2, 3, &mut |_| 28));
        assert!(!r.wants_war(&w, 2, 3, &mut |_| 27));
        // A peaceful leader (-2) needs the die to carry it.
        w.aggression = -2;
        assert!(!r.wants_war(&w, 2, 3, &mut |_| 31));
        // The die is rand(32 - rank(q)).
        let mut asked = 0;
        r.wants_war(&w, 2, 3, &mut |n| {
            asked = n;
            0
        });
        assert_eq!(asked, 31);
    }

    #[test]
    fn war_weariness_of_the_government_holds_the_ai_back() {
        let (r, mut w) = (rel(), World::new());
        w.aggression = 2;
        w.weariness = 2;
        // b = 2 - 2 + 1 (class 3) = 1, so the die must reach 30.
        assert!(r.wants_war(&w, 2, 3, &mut |_| 30));
        assert!(!r.wants_war(&w, 2, 3, &mut |_| 29));
    }

    #[test]
    fn peace_offers_are_blocked_by_memory_and_the_worst_attitude() {
        let (mut r, w) = (rel(), World::new());
        r.declare_war(&w, 2, 3, 0, false);
        r.exchange_embassies(2, 3);
        assert!(!r.can_talk(2, 3), "no talking across a war");
        // The embassy gate (0x5019F0) is only the byte and ROP, not the war.
        assert!(!r.may_offer_peace(&w, 2, 3), "the war memory blocks it");
        r.memory[Relations::ix(2, 3)] = 0;
        r.memory[Relations::ix(3, 2)] = 0;
        assert!(r.may_offer_peace(&w, 2, 3));
        // The most hostile class blocks it again.
        r.rec_mut(2, 3)[rec::HOSTILE_ACTS] = 5;
        assert_eq!(r.attitude_class(&w, 2, 3, 0), 4);
        assert!(!r.may_offer_peace(&w, 2, 3));
    }

    #[test]
    fn border_pressure_picks_the_warning_or_the_war_bit() {
        let mut r = rel();
        assert_eq!(r.border_pressure(2, 3, 0x1FF), relbit::BORDER_WARNING);
        assert_eq!(r.border_pressure(2, 3, 0x200), relbit::BORDER_WAR);
        assert_eq!(r.rel(2, 3), relbit::BORDER_WARNING | relbit::BORDER_WAR);
    }

    #[test]
    fn the_initiative_gate_matches_the_draw() {
        assert_eq!(Relations::initiative_gate(0, false), 1);
        assert_eq!(Relations::initiative_gate(5, false), 4);
        assert_eq!(Relations::initiative_gate(5, true), 3);
        assert_eq!(Relations::initiative_gate(1, true), 1);
    }

    #[test]
    fn clauses_have_their_flags_and_gates() {
        use crate::research::flags;
        assert_eq!(Clause::MutualProtection.tech_flag(), flags::MPP);
        assert_eq!(Clause::RightOfPassage.tech_flag(), flags::RIGHT_OF_PASSAGE);
        assert_eq!(Clause::MilitaryAlliance(3).tech_flag(), flags::MILITARY_ALLIANCE);
        assert_eq!(Clause::Embargo(3).tech_flag(), flags::TRADE_EMBARGO);
        assert_eq!(Clause::Tech(1).tech_flag(), 0);
        assert!(Clause::Gold(1).gated() && Clause::Tech(1).gated());
        assert!(!Clause::WorldMap.gated() && !Clause::City(1).gated());
    }

    #[test]
    fn clause_validity_follows_the_treaty_state() {
        let (mut r, w) = (rel(), World::new());
        assert!(!r.clause_valid(2, 3, &Clause::Peace), "peace needs a war");
        assert!(!r.clause_valid(2, 3, &Clause::RightOfPassage), "treaties need an embassy");
        r.exchange_embassies(2, 3);
        assert!(r.clause_valid(2, 3, &Clause::RightOfPassage));
        r.apply(&w, 2, 3, &Clause::RightOfPassage);
        assert!(!r.clause_valid(2, 3, &Clause::RightOfPassage), "already in force");
        assert_eq!(r.treaty(2, 3) & treaty::ROP, treaty::ROP);
        assert_eq!(r.treaty(3, 2) & treaty::ROP, treaty::ROP);
        r.declare_war(&w, 2, 3, 0, false);
        assert!(r.clause_valid(2, 3, &Clause::Peace));
        assert_eq!(r.treaty(2, 3), 0, "war voids the treaties");
    }

    #[test]
    fn embargo_and_contact_clauses() {
        let (mut r, w) = (rel(), World::new());
        r.exchange_embassies(2, 3);
        assert!(r.clause_valid(2, 3, &Clause::Embargo(4)));
        r.apply(&w, 2, 3, &Clause::Embargo(4));
        assert_eq!(r.embargo(2, 4), 1 << 3);
        assert_eq!(r.embargo(3, 4), 1 << 2);
        assert!(!r.clause_valid(2, 3, &Clause::Embargo(4)));
        // Contact: 2 knows 4, 3 does not.
        r.establish_contact(2, 4);
        assert!(r.clause_valid(2, 3, &Clause::Contact(4)));
        r.apply(&w, 2, 3, &Clause::Contact(4));
        assert!(r.contact(3, 4) && r.contact(4, 3));
    }

    #[test]
    fn a_joint_war_follows_a_military_alliance() {
        let (mut r, w) = (rel(), World::new());
        r.exchange_embassies(2, 3);
        let calls = r.apply(&w, 2, 3, &Clause::MilitaryAlliance(4));
        assert!(r.at_war(2, 4) && r.at_war(3, 4));
        assert_eq!(calls.len(), 2);
        assert_eq!(r.allies_vs(2, 4), 1 << 3);
        // Peace with 4 ends 2's alliance against 4.
        r.make_peace(2, 4);
        assert_eq!(r.allies_vs(2, 4), 0);
    }

    #[test]
    fn the_scorer_follows_the_ladder_and_the_cancelled_deals() {
        let mut r = rel();
        let mut price = |c: &Clause, _offer: bool| match c {
            Clause::Gold(n) => Some(*n),
            Clause::Tech(_) => Some(300),
            _ => Some(0),
        };
        // The AI is offered 300, asked 300: accepted.
        assert_eq!(weigh(&r, 2, 1, &[Clause::Gold(300)], &[Clause::Tech(1)], &mut price), Verdict::Accept);
        // 270 of 300 is above 7/8 (262): weak reject.
        assert_eq!(weigh(&r, 2, 1, &[Clause::Gold(270)], &[Clause::Tech(1)], &mut price), Verdict::WeakReject);
        // A cancelled deal quintuples the ask.
        r.rec_mut(2, 1)[rec::DEALS_CANCELLED] = 1;
        assert_eq!(weigh(&r, 2, 1, &[Clause::Gold(300)], &[Clause::Tech(1)], &mut price), Verdict::StrongReject);
        // A clause that cannot be priced makes the deal invalid.
        let mut none = |_: &Clause, _: bool| None;
        assert_eq!(weigh(&r, 2, 1, &[Clause::Peace], &[], &mut none), Verdict::Invalid);
        assert_eq!(Verdict::Invalid.key(), None);
        assert_eq!(Verdict::Accept.key(), Some("DIPLOADVICETRADE_DEAL_ACCEPT"));
    }
}
