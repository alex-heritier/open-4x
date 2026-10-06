//! Research: the reverse-engineered advance rules (`civ3mapgen::research`,
//! specification `reverse-engineering/research.md`) wired into the game.
//!
//! One `World` holds every civilization's knowledge, target and beakers. A
//! civ's turn adds the science of its cities to the beakers and runs the
//! research step (`0x562200`) before the cities are processed, so an
//! advance that arrives this turn can be built this turn (`turn.md` 3.2).
//! The computer picks its targets with the AI valuation (`research-ai.md`);
//! the human is asked, in the Science Advisor (`advisors.rs`).
//!
//! The Great Library reads its current owner, government and obsolescence
//! before each research step. Scientific leaders appear at the capital after
//! the native first-discoverer roll. The game uses four civilizations on a
//! Regent standard world.
use bevy::prelude::*;

use civ3mapgen::research::{Ctx, Dice, Event, NONE, World};
use civ3mapgen::research_ai::{Profile, Tables, Valuer, category_mask_from_flags};

use crate::cities::{City, Production};
use crate::civs::{CIV_CAP, CivilizationEnded, Civilizations, RACES, civ_count, is_ai};
use crate::combat::CombatRng;
use crate::diplomacy::Diplomacy;
use crate::features::{MessageBoard, post};
use crate::map::GameMap;
use crate::rng::MapRng;
use crate::roster::upgrade_chain;
use crate::ruleset::{self as rules_data, ERA_NAMES, TECH_NAMES};
use crate::units::{Turn, Unit, def};

impl Dice for MapRng {
    fn below(&mut self, n: u32) -> i32 {
        MapRng::below(self, n)
    }
}

/// The slot a civ holds in the research and diplomacy worlds (slot 0 is the
/// barbarians).
pub fn slot(civ: usize) -> u32 {
    if civ == crate::civs::BARBARIANS {
        return 0;
    }
    civ as u32 + 1
}

/// The civ in a slot.
pub fn civ_of(slot: u32) -> usize {
    slot as usize - 1
}

/// `DIFF[2]` (Regent) and the standard `WSIZ` row.

/// A set of productions, one bit per `Production::index`.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Bits(Vec<u64>);

impl Bits {
    const EMPTY: Bits = Bits(Vec::new());

    pub fn get(&self, i: usize) -> bool {
        self.0.get(i / 64).is_some_and(|w| w >> (i % 64) & 1 != 0)
    }

    pub fn set(&mut self, i: usize, on: bool) {
        if i / 64 >= self.0.len() {
            if !on {
                return;
            }
            self.0.resize(i / 64 + 1, 0);
        }
        if on {
            self.0[i / 64] |= 1 << (i % 64);
        } else {
            self.0[i / 64] &= !(1 << (i % 64));
        }
    }
}

/// What the city layer's plain functions (`City::buildable`) have to know
/// about the rest of the game without ECS access. A process-wide setting
/// like `civs::AI_MASK`; nothing is locked until the research world says so.
#[derive(Clone)]
struct Access {
    /// Productions each civ cannot build yet (advance, civ, obsolescence,
    /// strategic resource).
    locked: [Bits; CIV_CAP],
    /// Units each civ could not train even if nothing newer replaced them
    /// (the upgrade walk asks for these: the advance, the race, the goods).
    lacking: [Bits; CIV_CAP],
    /// Strategic and luxury goods each civ owns, one bit per `GOOD` row.
    goods: [u32; CIV_CAP],
    /// Great wonders built anywhere: nobody builds them twice.
    built: Bits,
}

impl Access {
    const NONE: Access = Access {
        locked: [Bits::EMPTY; CIV_CAP],
        lacking: [Bits::EMPTY; CIV_CAP],
        goods: [0; CIV_CAP],
        built: Bits::EMPTY,
    };
}

#[cfg(not(test))]
static ACCESS: std::sync::Mutex<Access> = std::sync::Mutex::new(Access::NONE);

// Under test each thread has an access table of its own, as with
// `civs::AI_MASK`.
#[cfg(test)]
thread_local! {
    static ACCESS: std::cell::RefCell<Access> = const { std::cell::RefCell::new(Access::NONE) };
}

fn with_access<R>(f: impl FnOnce(&mut Access) -> R) -> R {
    #[cfg(not(test))]
    return f(&mut ACCESS.lock().unwrap());
    #[cfg(test)]
    return ACCESS.with(|a| f(&mut a.borrow_mut()));
}

/// The advance a production needs, `-1` for none.
#[cfg(test)]
pub fn required_tech(p: Production) -> i32 {
    match p.unit() {
        Some(u) => u.row().tech,
        None => p.bldg().map_or(NONE, |b| b.tech),
    }
}

/// Whether `civ` may build `p` at all: it has the advance, the production
/// is its own (a unique unit is its race's alone), nothing newer on the
/// upgrade chain replaces it, and the strategic resources are owned.
pub fn can_build(civ: usize, p: Production) -> bool {
    with_access(|a| !a.locked[civ].get(p.index()) && !(p.is_building() && a.built.get(p.index())))
}

/// Whether `civ` could train unit row `u` but for its being obsolete: what
/// the upgrade walk asks of each type on a chain (`unit-upgrades.md` 5).
pub fn can_train(civ: usize, u: usize) -> bool {
    with_access(|a| !a.lacking[civ].get(u))
}

/// Test hook: take a unit away from, or give it back to, a civ.
#[cfg(test)]
pub fn set_trainable(civ: usize, u: usize, on: bool) {
    with_access(|a| a.lacking[civ].set(u, !on));
}

/// Everything `civ` cannot build yet.
pub fn locked(civ: usize) -> Vec<Production> {
    Production::all().filter(|&p| !can_build(civ, p)).collect()
}

/// Some city, anywhere, holds the great wonder (`0x538FE0`).
pub fn wonder_built(p: Production) -> bool {
    with_access(|a| a.built.get(p.index()))
}

/// A great wonder has been built somewhere: nobody builds it again.
pub fn set_wonder_built(p: Production, built: bool) {
    with_access(|a| a.built.set(p.index(), built));
}

/// Goods `civ` owns inside its borders: a bit per `GOOD` row.
pub fn owns_good(civ: usize, good: i32) -> bool {
    good < 0 || with_access(|a| a.goods[civ] >> good & 1 != 0)
}

/// Replace the owned goods of `civ`. True when anything changed (the
/// caller then re-syncs the locks).
pub fn set_goods(civ: usize, goods: u32) -> bool {
    with_access(|a| std::mem::replace(&mut a.goods[civ], goods) != goods)
}

/// Name of an advance.
pub fn tech_name(t: i32) -> &'static str {
    usize::try_from(t)
        .ok()
        .and_then(|i| TECH_NAMES.get(i))
        .copied()
        .unwrap_or("Future Technology")
}

/// All research state of the game.
#[derive(Resource)]
pub struct Research {
    pub world: World,
    tables: Tables,
    profiles: Vec<Profile>,
    categories: Vec<u32>,
    started: bool,
    leaders: Vec<usize>,
}

impl Research {
    /// The game's four civilizations, nothing known yet.
    pub fn new() -> Self {
        let mut world = World::new(rules_data::rules());
        world.scientific_leaders = true;
        world.difficulty_cost_factor =
            rules_data::DIFFICULTY_COST_FACTOR[crate::scenario::difficulty()];
        world.size_tech_rate = rules_data::WORLD_TECH_RATE[crate::scenario::setup().size];
        let mut profiles = vec![Profile::default(); civ3mapgen::research::SLOTS];
        for (civ, race) in RACES.iter().enumerate() {
            let s = slot(civ);
            world.in_play |= 1 << s;
            if !is_ai(civ) {
                world.human |= 1 << s;
            }
            let me = &mut world.players[s as usize];
            me.interactive = !is_ai(civ);
            me.free_techs = race.free_techs;
            // A scenario lead (or a save) names its own advances: they are
            // all granted at the start, not just the first era's.
            if crate::scenario::scenario()
                .and_then(|s| s.leads.get(civ))
                .is_some_and(|l| !l.free_techs.is_empty())
            {
                me.free_techs = [NONE; 4];
            }
            // RACE trait 3 is Scientific.
            me.scientific = race.traits >> 3 & 1 != 0;
            profiles[s as usize] = Profile {
                flavors: race.flavors,
                build_often: race.build_often,
                militaristic: race.traits & 1 != 0,
                race: race.race,
                defenders: 0,
                transports: 0,
            };
        }
        let categories = world
            .rules
            .techs
            .iter()
            .map(|t| category_mask_from_flags(t.flags))
            .collect();
        Research {
            world,
            tables: rules_data::tables(),
            profiles,
            categories,
            started: false,
            leaders: vec![],
        }
    }

    /// Everything that changes in play: the started flag, pending leader
    /// rewards, then the world (`civ3mapgen::research::World::to_words`).
    pub fn snapshot(&self) -> Vec<i64> {
        let mut words = vec![i64::from(self.started)];
        words.push(self.leaders.len() as i64);
        words.extend(self.leaders.iter().map(|&c| c as i64));
        words.extend(self.world.to_words());
        words
    }

    /// Put a snapshot back; false, with nothing changed, if it does not fit.
    pub fn restore(&mut self, words: &[i64]) -> bool {
        let Some((&started, rest)) = words.split_first() else {
            return false;
        };
        let Some((&n, rest)) = rest.split_first() else {
            return false;
        };
        let Ok(n) = usize::try_from(n) else {
            return false;
        };
        if n > rest.len() {
            return false;
        }
        let (leaders, rest) = rest.split_at(n);
        if leaders.iter().any(|&c| c < 0 || c >= civ_count() as i64) || !self.world.restore(rest) {
            return false;
        }
        self.started = started != 0;
        self.leaders = leaders.iter().map(|&c| c as usize).collect();
        true
    }

    /// Run `f` with the AI valuation as the brain of the research code.
    fn with_brain<R>(
        &mut self,
        dice: &mut dyn Dice,
        f: impl FnOnce(&mut World, &mut Ctx<'_>) -> R,
    ) -> R {
        let Research {
            world,
            tables,
            profiles,
            categories,
            ..
        } = self;
        let mut brain = Valuer {
            tables,
            profiles,
            categories,
            space_race: false,
            wonder_built: &|_| false,
        };
        let mut ctx = Ctx {
            brain: &mut brain,
            dice,
        };
        f(world, &mut ctx)
    }

    /// Does `civ` know the advance?
    pub fn knows(&self, civ: usize, t: i32) -> bool {
        self.world.knows(slot(civ), t)
    }

    /// Does `civ` know an advance with one of these flags?
    pub fn knows_flag(&self, civ: usize, mask: u32) -> bool {
        self.world.knows_with_flags(slot(civ), mask)
    }

    /// The advance being researched, if there is one.
    pub fn target(&self, civ: usize) -> Option<i32> {
        let t = self.world.players[slot(civ) as usize].current;
        (t >= 0 && t < self.world.t()).then_some(t)
    }

    /// A human still has to pick a target.
    pub fn needs_choice(&self, civ: usize) -> bool {
        self.world.players[slot(civ) as usize].awaiting_choice
    }

    /// Advances `civ` could research now, cheapest first.
    pub fn options(&self, civ: usize) -> Vec<i32> {
        let s = slot(civ);
        let mut v: Vec<i32> = (0..self.world.t())
            .filter(|&t| self.world.can_research(s, t))
            .collect();
        v.sort_by_key(|&t| (self.world.effective_cost(s, t, false), t));
        v
    }

    /// Turns to finish `t` at the current rate (9999 for no science).
    pub fn turns(&self, civ: usize, t: i32) -> i32 {
        self.world.turns_left(slot(civ), t, true)
    }

    /// Beakers stored toward the target and the cost of it.
    pub fn progress(&self, civ: usize) -> Option<(i32, i32)> {
        let t = self.target(civ)?;
        let s = slot(civ);
        Some((
            self.world.players[s as usize].beakers,
            self.world.effective_cost(s, t, false),
        ))
    }

    /// The era `civ` is in, `0..=3` (`research.md` 7).
    pub fn era(&self, civ: usize) -> i32 {
        self.world.players[slot(civ) as usize].era
    }

    /// Beakers a turn.
    pub fn rate(&self, civ: usize) -> i32 {
        self.world.players[slot(civ) as usize].rate
    }

    pub fn science_age(&self, civ: usize, turn: i32) -> bool {
        self.world.players[slot(civ) as usize]
            .science_until
            .is_some_and(|last| turn <= last)
    }

    pub fn start_science_age(&mut self, civ: usize, turn: i32) {
        self.world.players[slot(civ) as usize].science_until = Some(turn + 20);
    }

    fn apply_rewards(&mut self, events: &[Event]) {
        // Era effects must run even when no human announcement is shown
        // (computer research, hut rewards and computer-to-computer trades).
        if events
            .iter()
            .any(|e| matches!(e, Event::BarbarianLanding { .. }))
        {
            crate::barbarians::request_uprising();
        }
        self.leaders.extend(events.iter().filter_map(|e| match *e {
            Event::ScientificLeader { player, .. } => Some(civ_of(player)),
            _ => None,
        }));
    }

    /// Does `civ` know tech `need` (`-1` is always met; the advance count
    /// means "no advance grants it", never met)?
    fn has_tech(&self, civ: usize, need: i32) -> bool {
        need == NONE || (need < self.world.t() && self.knows(civ, need))
    }

    /// `City::canBuildUnit` without the city: the civ's race may build it,
    /// it has the advance and the strategic resources (`buildable.md` 3).
    fn unit_available(&self, civ: usize, u: usize) -> bool {
        let r = crate::roster::unit(u);
        r.playable
            // Nobody builds the battle-created unit (`buildable.md` 3 step 5).
            && r.abilities & crate::roster::ability::LEADER == 0
            && r.races >> RACES[civ].race & 1 != 0
            && self.has_tech(civ, r.tech)
            && r.resources.iter().all(|&g| owns_good(civ, g))
    }

    /// Whether `p` is hidden from `civ`'s build list for a reason that is
    /// the same in every city.
    fn is_locked(&self, civ: usize, p: Production) -> bool {
        if let Some(u) = p.unit() {
            // A unit with a buildable upgrade is obsolete: "the city must
            // build the upgrade instead" (`buildable.md` 3.1 step 2).
            return !self.unit_available(civ, u.0 as usize)
                || upgrade_chain(u.0 as usize).any(|n| self.unit_available(civ, n));
        }
        let Some(b) = p.bldg() else { return true };
        !self.has_tech(civ, b.tech)
            || (b.obsolete >= 0 && self.has_tech(civ, b.obsolete))
            || b.resources.iter().any(|&g| !owns_good(civ, g))
    }

    /// Refresh the bits `City::buildable` reads.
    fn sync_locks(&self) {
        for civ in 0..civ_count() {
            let mut mask = Bits::EMPTY;
            let mut lacking = Bits::EMPTY;
            for p in Production::all() {
                mask.set(p.index(), self.is_locked(civ, p));
                if let Some(u) = p.unit() {
                    lacking.set(p.index(), !self.unit_available(civ, u.0 as usize));
                }
            }
            with_access(|a| {
                a.locked[civ] = mask;
                a.lacking[civ] = lacking;
            });
        }
    }

    /// The goods of `civ` changed; unit and improvement availability may
    /// have with them.
    pub fn goods_changed(&self) {
        self.sync_locks();
    }

    /// Active Great Library-type wonders (`research.md` 8). Read knowledge
    /// directly, rather than the previous frame's realm snapshot.
    fn refresh_wonders<'a>(&mut self, cities: impl Iterator<Item = &'a City>) {
        let mut library = [false; CIV_CAP];
        let mut obsolete = [NONE; CIV_CAP];
        for city in cities {
            let civ = city.civ;
            let govt = crate::realm::read(civ, |r| r.govt as i32);
            for b in city.buildings.iter().filter_map(|p| p.bldg()) {
                if b.wonder & crate::roster::wonder::GAIN_TECHS_OF_TWO_CIVS != 0
                    && (b.govt < 0 || b.govt == govt)
                    && (b.obsolete < 0 || !self.knows(civ, b.obsolete))
                {
                    library[civ] = true;
                    obsolete[civ] = b.obsolete;
                }
            }
        }
        for (civ, active) in library.into_iter().enumerate() {
            self.world.players[slot(civ) as usize].great_library = active;
            self.world.players[slot(civ) as usize].great_library_obsolete = obsolete[civ];
        }
    }

    /// Game start: the free advances of each civilization (`Player::init`,
    /// `research.md` 7.5), then every target: the computer's by the
    /// valuation, the human's by asking.
    pub fn begin(&mut self, dice: &mut dyn Dice) -> Vec<Event> {
        if self.started {
            return vec![];
        }
        self.started = true;
        let mut events = vec![];
        self.with_brain(dice, |w, ctx| {
            for civ in 0..civ_count() {
                w.start_grants(slot(civ), &mut events, ctx);
                if let Some(lead) = crate::scenario::scenario().and_then(|s| s.leads.get(civ)) {
                    w.grant_known(slot(civ), &lead.free_techs, &mut events, ctx);
                }
            }
            for civ in 0..civ_count() {
                let s = slot(civ);
                if w.players[s as usize].interactive {
                    w.choose_research(s, false, None, ctx.brain, ctx.dice);
                } else {
                    w.choose(s, NONE, ctx.brain, ctx.dice);
                }
            }
        });
        self.sync_locks();
        events
    }

    /// The research step of one civ's turn: `rate` and `cities` are already
    /// set; the beakers of the turn are added and the step runs.
    pub fn finish_turn(
        &mut self,
        civ: usize,
        beakers: i32,
        turn: i32,
        dice: &mut dyn Dice,
    ) -> Vec<Event> {
        let s = slot(civ);
        let mut events = vec![];
        self.world.turn = turn;
        self.world.add_beakers(s, beakers);
        self.with_brain(dice, |w, ctx| w.step(s, &mut events, ctx));
        self.apply_rewards(&events);
        self.sync_locks();
        events
    }

    /// The human answers the research dialog.
    pub fn pick(&mut self, civ: usize, t: i32, dice: &mut dyn Dice) {
        let s = slot(civ);
        if !self.world.can_research(s, t) {
            return;
        }
        self.with_brain(dice, |w, ctx| w.choose(s, t, ctx.brain, ctx.dice));
    }

    /// An advance that comes from outside research: a trade, a hut, theft.
    pub fn award(&mut self, civ: usize, t: i32, dice: &mut dyn Dice) -> Vec<Event> {
        let s = slot(civ);
        let mut events = vec![];
        self.with_brain(dice, |w, ctx| {
            w.acquire(s, t, false, true, true, &mut events, ctx)
        });
        self.apply_rewards(&events);
        self.sync_locks();
        events
    }

    /// The advance case of a goody hut (`research.md` 10.5): the advance
    /// given and the events of learning it, or `None` when the hut has no
    /// advance for this civ.
    pub fn hut_advance(&mut self, civ: usize, dice: &mut dyn Dice) -> Option<(i32, Vec<Event>)> {
        let s = slot(civ);
        let t = self.with_brain(dice, |w, ctx| w.hut_advance(s, ctx.brain, ctx.dice));
        if t == NONE {
            return None;
        }
        Some((t, self.award(civ, t, dice)))
    }

    /// What the AI's valuation says an advance is worth to `civ`
    /// (`0x448BF0`, the trade price basis).
    pub fn worth(&self, civ: usize, t: i32, dice: &mut dyn Dice) -> i32 {
        let valuer = Valuer {
            tables: &self.tables,
            profiles: &self.profiles,
            categories: &self.categories,
            space_race: false,
            wonder_built: &|_| false,
        };
        valuer.value(&self.world, slot(civ), t, false, false, dice)
    }

    /// The advances `giver` could hand to `taker`: known to the giver,
    /// researchable by the taker, and tradeable.
    pub fn giftable(&self, giver: usize, taker: usize) -> Vec<i32> {
        use civ3mapgen::research::flags::CANNOT_BE_TRADED;
        (0..self.world.t())
            .filter(|&t| {
                self.knows(giver, t)
                    && self.world.can_research(slot(taker), t)
                    && self.world.rules.techs[t as usize].flags & CANNOT_BE_TRADED == 0
            })
            .collect()
    }
}

impl Default for Research {
    fn default() -> Self {
        Self::new()
    }
}

/// What a research event says to the human, if anything.
pub fn announce(events: &[Event], board: &mut MessageBoard) {
    for e in events {
        match *e {
            Event::Acquired {
                player,
                tech,
                by_research,
            } if !is_ai(civ_of(player)) => {
                let name = tech_name(tech);
                post(
                    board,
                    if by_research {
                        format!("Our scientists have discovered {name}!")
                    } else if events.iter().any(|e| matches!(e,
                        Event::GreatLibrary { player: p, tech: t, .. } if *p == player && *t == tech
                    )) {
                        format!("The Great Library has taught us {name}.")
                    } else {
                        format!("We have learned {name}.")
                    },
                );
            }
            Event::EnteredEra { player, era } if !is_ai(civ_of(player)) => {
                post(
                    board,
                    format!(
                        "Our civilization enters the {} Age.",
                        ERA_NAMES[era as usize]
                    ),
                );
            }
            _ => {}
        }
    }
}

/// Game start: grants and first targets. The free advances are not news.
pub fn begin(mut research: ResMut<Research>, mut rng: ResMut<CombatRng>) {
    research.begin(&mut rng.0);
}

/// Keep the world's view of the board current: science rates, city counts,
/// contacts, and the defenders the valuation counts.
pub fn refresh(
    mut research: ResMut<Research>,
    map: Res<GameMap>,
    turn: Res<Turn>,
    diplomacy: Res<Diplomacy>,
    cities: Query<&City>,
    units: Query<&Unit>,
) {
    let mut rate = [0i32; CIV_CAP];
    let mut count = [0i32; CIV_CAP];
    let mut defenders = [0i32; CIV_CAP];
    for c in &cities {
        rate[c.civ] += crate::citycalc::totals(&map, c).sci;
        count[c.civ] += 1;
    }
    for u in &units {
        if def(u.utype).defense > 0 && def(u.utype).attack > 0 && u.civ < civ_count() {
            defenders[u.civ] += 1;
        }
    }
    // Work the new figures out first and write only if one differs: this
    // runs every frame, and any write marks `Research` changed for every
    // system (like `realm::sync`) that waits on it.
    let turn_now = turn.0 as i32;
    let figures: Vec<(usize, i32, i32, u32, i32)> = (0..civ_count())
        .map(|civ| {
            let s = slot(civ) as usize;
            let me = &research.world.players[s];
            let rate = if me.science_until.is_some_and(|last| turn_now <= last) {
                (rate[civ] as f32 * 1.25) as i32
            } else {
                rate[civ]
            };
            (s, rate, count[civ], diplomacy.contacts(civ), defenders[civ])
        })
        .collect();
    let current = |s: usize| {
        let me = &research.world.players[s];
        (
            me.rate,
            me.cities,
            me.contact,
            research.profiles[s].defenders,
        )
    };
    if research.world.turn == turn_now
        && figures
            .iter()
            .all(|&(s, rate, cities, contact, defenders)| {
                current(s) == (rate, cities, contact, defenders)
            })
    {
        return;
    }
    research.world.turn = turn_now;
    for (s, rate, cities, contact, defenders) in figures {
        let me = &mut research.world.players[s];
        me.rate = rate;
        me.cities = cities;
        me.contact = contact;
        research.profiles[s].defenders = defenders;
    }
}

/// A civ's turn ends: its cities' science goes into the beakers and the
/// step runs, before the cities are processed.
pub fn end_turn(
    mut ended: MessageReader<CivilizationEnded>,
    mut research: ResMut<Research>,
    mut rng: ResMut<CombatRng>,
    mut board: ResMut<MessageBoard>,
    civs: Res<Civilizations>,
    turn: Res<Turn>,
    cities: Query<&City>,
    map: Res<GameMap>,
) {
    for ev in ended.read() {
        let civ = ev.0;
        if civs.outcome.is_some() {
            continue;
        }
        // The native Science Age changes estimates and cost clamps, but the
        // income pass still adds the raw city science (`research.md` 3.2).
        let beakers = cities
            .iter()
            .filter(|c| c.civ == civ)
            .map(|c| crate::citycalc::totals(&map, c).sci)
            .sum();
        research.refresh_wonders(cities.iter());
        let events = research.finish_turn(civ, beakers, turn.0 as i32, &mut rng.0);
        let player = &mut research.world.players[slot(civ) as usize];
        if player
            .science_until
            .is_some_and(|last| turn.0 as i32 > last)
        {
            player.science_until = None;
            if !is_ai(civ) {
                post(&mut board, "Our Science Age has ended.");
            }
        }
        // The computer's discoveries are its own business.
        if is_ai(civ) {
            if std::env::var("CIV3_AI_LOG").is_ok() {
                for e in &events {
                    if let Event::Acquired { tech, .. } = e {
                        println!(
                            "research: turn {} {} learns {}",
                            turn.0,
                            crate::civs::CIVS[civ].name,
                            tech_name(*tech)
                        );
                    }
                }
            }
            continue;
        }
        announce(&events, &mut board);
    }
}

/// Complete successful native rolls after all research/trade/hut acquisitions.
pub fn spawn_leaders(
    mut commands: Commands,
    mut research: ResMut<Research>,
    capital: Res<crate::cities::Capital>,
    cities: Query<&City>,
    art: Res<crate::units::UnitArt>,
    mut board: ResMut<MessageBoard>,
) {
    // Looking is free; `take` borrows mutably and would flag `Research`
    // changed on every frame, pending leader or not.
    if research.leaders.is_empty() {
        return;
    }
    for civ in std::mem::take(&mut research.leaders) {
        let Some(city) = capital.0[civ]
            .and_then(|e| cities.get(e).ok())
            .filter(|c| c.civ == civ)
        else {
            continue;
        };
        let e = crate::units::spawn_unit(
            &mut commands,
            &art,
            crate::roles::leader(),
            city.x,
            city.y,
            civ,
        );
        commands
            .entity(e)
            .entry::<Unit>()
            .and_modify(|mut u| u.scientific_leader = true);
        if !is_ai(civ) {
            post(
                &mut board,
                format!("A Scientific Leader has emerged in {}!", city.name),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use civ3mapgen::rng::Rng;

    /// A research world with Japan human and the rest computer, regardless
    /// of the process-wide controller mask.
    fn game() -> Research {
        let mut r = Research::new();
        for civ in 0..civ_count() {
            let s = slot(civ);
            let human = civ == 0;
            r.world.players[s as usize].interactive = human;
            if human {
                r.world.human = 1 << s;
            } else {
                r.world.human &= !(1 << s);
            }
        }
        r
    }

    #[test]
    fn a_research_discovery_spawns_a_scientific_leader_at_the_capital() {
        let mut r = game();
        let bw = tech("Bronze Working");
        let p = &mut r.world.players[slot(0) as usize];
        p.current = bw;
        p.rate = 10;
        p.cities = 1;
        p.beakers = 1000;
        p.turns = 3;
        // A seed whose first native roll is below the non-Scientific 3%.
        let seed = (0..10000).find(|&s| MapRng::new(s).below(100) < 3).unwrap();
        let events = r.finish_turn(0, 0, 1, &mut MapRng::new(seed));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::ScientificLeader { player: 1, .. }))
        );
        let mut app = App::new();
        app.insert_resource(r);
        app.init_resource::<crate::cities::Capital>();
        app.init_resource::<MessageBoard>();
        app.insert_resource(crate::units::UnitArt::blank());
        let capital = app.world_mut().spawn(City::new(0, "Kyoto", 10, 11)).id();
        app.world_mut().resource_mut::<crate::cities::Capital>().0[0] = Some(capital);
        app.add_systems(Update, spawn_leaders);
        app.update();
        let leaders: Vec<_> = app.world_mut().query::<&Unit>().iter(app.world()).collect();
        assert_eq!(leaders.len(), 1);
        assert_eq!((leaders[0].civ, leaders[0].x, leaders[0].y), (0, 10, 11));
        assert!(leaders[0].scientific_leader);
        assert_eq!(leaders[0].utype, crate::units::UnitType::named("Leader"));
        app.update();
        assert_eq!(
            app.world_mut().query::<&Unit>().iter(app.world()).count(),
            1
        );
        // Successful rolls still consume their die when no capital exists.
        app.world_mut().resource_mut::<Research>().leaders.push(1);
        app.update();
        assert_eq!(
            app.world_mut().query::<&Unit>().iter(app.world()).count(),
            1
        );
    }

    #[test]
    fn science_age_changes_the_estimate_rate_but_not_beaker_income() {
        crate::realm::reset();
        crate::realm::set_rates(
            0,
            crate::realm::Rates {
                tax: 0,
                sci: 10,
                lux: 0,
            },
        );
        let mut r = game();
        let bw = tech("Bronze Working");
        r.world.players[slot(0) as usize].current = bw;
        r.start_science_age(0, 1);
        let mut app = App::new();
        app.edit_schedule(Update, |s| {
            s.set_executor_kind(bevy::ecs::schedule::ExecutorKind::SingleThreaded);
        });
        app.insert_resource(r);
        app.insert_resource(GameMap::generate());
        app.insert_resource(Diplomacy::new());
        app.insert_resource(CombatRng(MapRng::new(5)));
        app.init_resource::<MessageBoard>();
        app.init_resource::<Civilizations>();
        app.insert_resource(Turn(1));
        app.add_message::<CivilizationEnded>();
        let mut city = City::new(0, "Kyoto", 10, 10);
        city.work_tile(app.world().resource::<GameMap>(), (11, 10));
        city.buildings.push(Production::named("Library"));
        let mut map = app.world_mut().resource_mut::<GameMap>();
        let i = map.idx(11, 10);
        map.tiles[i].base = crate::map::Base::Coast;
        app.world_mut().spawn(city);
        app.add_systems(Update, (refresh, end_turn).chain());
        app.update();
        let raw: i32 = app
            .world_mut()
            .query::<&City>()
            .iter(app.world())
            .map(|c| crate::citycalc::totals(app.world().resource::<GameMap>(), c).sci)
            .sum();
        assert!(raw >= 4);
        assert_eq!(
            app.world().resource::<Research>().rate(0),
            (raw as f32 * 1.25) as i32
        );
        app.world_mut().write_message(CivilizationEnded(0));
        app.update();
        assert_eq!(
            app.world().resource::<Research>().world.players[1].beakers,
            raw
        );
        assert!(app.world().resource::<Research>().science_age(0, 21));
        assert!(!app.world().resource::<Research>().science_age(0, 22));
        app.world_mut().resource_mut::<Turn>().0 = 22;
        app.world_mut().write_message(CivilizationEnded(0));
        app.update();
        assert_eq!(
            app.world().resource::<Research>().world.players[1].science_until,
            None
        );
        assert_eq!(app.world().resource::<Research>().rate(0), raw);
    }

    fn tech(name: &str) -> i32 {
        TECH_NAMES.iter().position(|&n| n == name).unwrap() as i32
    }

    fn library() -> Production {
        Production::from_building_row(
            crate::roster::BLDGS
                .iter()
                .position(|b| b.wonder & crate::roster::wonder::GAIN_TECHS_OF_TWO_CIVS != 0)
                .unwrap(),
        )
    }

    #[test]
    fn the_great_library_teaches_at_turn_end_after_two_contacts_know_an_advance() {
        crate::realm::reset();
        let mut r = game();
        let mut dice = MapRng::new(5);
        r.begin(&mut dice);
        let bw = tech("Bronze Working");
        r.award(1, bw, &mut dice);
        r.award(2, bw, &mut dice);
        r.pick(0, bw, &mut dice);
        r.world.players[slot(0) as usize].beakers = 3;
        let mut city = City::new(0, "Kyoto", 10, 10);
        city.buildings.push(library());
        let mut diplomacy = Diplomacy::new();
        diplomacy.meet(0, 1);
        let mut app = App::new();
        app.edit_schedule(Update, |s| {
            s.set_executor_kind(bevy::ecs::schedule::ExecutorKind::SingleThreaded);
        });
        app.insert_resource(r);
        app.insert_resource(GameMap::generate());
        app.insert_resource(diplomacy);
        app.insert_resource(CombatRng(dice));
        app.insert_resource(MessageBoard::default());
        app.insert_resource(Civilizations::default());
        app.insert_resource(Turn(1));
        app.add_message::<CivilizationEnded>();
        let c = app.world_mut().spawn(city).id();
        app.add_systems(Update, (refresh, end_turn).chain());
        app.world_mut().write_message(CivilizationEnded(0));
        app.update();
        assert!(
            !app.world().resource::<Research>().knows(0, bw),
            "one contact cannot teach it"
        );
        app.world_mut().resource_mut::<Diplomacy>().meet(0, 2);
        app.world_mut().write_message(CivilizationEnded(0));
        app.update();
        let r = app.world().resource::<Research>();
        assert!(r.knows(0, bw));
        assert!(
            can_build(0, Production::named("Spearman")),
            "the free advance unlocks production"
        );
        assert!(
            r.needs_choice(0),
            "choose new research after learning the target"
        );
        assert!(
            app.world()
                .resource::<MessageBoard>()
                .text
                .contains("The Great Library")
        );
        let next = r.options(0)[0];
        app.world_mut()
            .resource_scope(|world, mut r: Mut<Research>| {
                r.pick(0, next, &mut world.resource_mut::<CombatRng>().0);
                assert_eq!(
                    r.world.players[slot(0) as usize].beakers,
                    0,
                    "the next target starts without the old beakers"
                );
            });
        // Losing the wonder changes the recipient at the next research step.
        app.world_mut().get_mut::<City>(c).unwrap().civ = 1;
        app.world_mut().write_message(CivilizationEnded(0));
        app.update();
        let r = app.world().resource::<Research>();
        assert!(!r.world.players[slot(0) as usize].great_library);
        assert!(r.world.players[slot(1) as usize].great_library);
    }

    #[test]
    fn an_obsolete_or_removed_great_library_stops_granting_advances() {
        crate::realm::reset();
        let mut r = game();
        let mut dice = Rng::new(5);
        r.begin(&mut dice);
        let mut city = City::new(0, "Kyoto", 10, 10);
        let p = library();
        city.buildings.push(p);
        r.refresh_wonders(std::iter::once(&city));
        assert!(r.world.players[slot(0) as usize].great_library);
        r.award(0, p.bldg().unwrap().obsolete, &mut dice);
        r.refresh_wonders(std::iter::once(&city));
        assert!(
            !r.world.players[slot(0) as usize].great_library,
            "Education obsoletes it immediately"
        );
        // An owner without Education can still use a captured library.
        city.civ = 1;
        r.refresh_wonders(std::iter::once(&city));
        assert!(r.world.players[slot(1) as usize].great_library);
        city.buildings.clear();
        r.refresh_wonders(std::iter::once(&city));
        assert!(!r.world.players[slot(1) as usize].great_library);
    }

    #[test]
    fn production_prerequisites_come_from_the_roster() {
        assert_eq!(required_tech(Production::named("Warrior")), NONE);
        assert_eq!(
            required_tech(Production::named("Archer")),
            tech("Warrior Code")
        );
        assert_eq!(
            required_tech(Production::named("Spearman")),
            tech("Bronze Working")
        );
        assert_eq!(
            required_tech(Production::named("Horseman")),
            tech("Horseback Riding")
        );
        assert_eq!(required_tech(Production::named("Granary")), tech("Pottery"));
        assert_eq!(
            required_tech(Production::named("Temple")),
            tech("Ceremonial Burial")
        );
    }

    #[test]
    fn a_new_game_grants_each_civ_its_free_advances() {
        let mut r = game();
        let mut dice = Rng::new(7);
        r.begin(&mut dice);
        for (civ, race) in RACES.iter().enumerate() {
            for &t in race.free_techs.iter().filter(|&&t| t >= 0) {
                assert!(
                    r.knows(civ, t),
                    "{} lacks {}",
                    crate::civs::CIVS[civ].name,
                    tech_name(t)
                );
            }
        }
        // Nobody gets more than the free ones plus their era's catch-up.
        assert!(r.world.players[slot(1) as usize].known_count >= 2);
    }

    #[test]
    fn the_computer_picks_a_target_and_the_human_is_asked() {
        let mut r = game();
        let mut dice = Rng::new(7);
        r.begin(&mut dice);
        assert!(r.needs_choice(0), "the human chooses");
        assert_eq!(r.target(0), None);
        for civ in 1..civ_count() {
            let t = r
                .target(civ)
                .unwrap_or_else(|| panic!("civ {civ} has no target"));
            assert!(!r.knows(civ, t));
            assert!(r.world.can_research(slot(civ), t));
        }
        // The answer sets the target and clears the question.
        let pick = r.options(0)[0];
        r.pick(0, pick, &mut dice);
        assert_eq!(r.target(0), Some(pick));
        assert!(!r.needs_choice(0));
        // An advance that cannot be researched yet is refused.
        let before = r.target(0);
        r.pick(0, tech("Philosophy"), &mut dice);
        assert_eq!(r.target(0), before);
    }

    #[test]
    fn options_are_the_researchable_advances_cheapest_first() {
        let mut r = game();
        r.begin(&mut Rng::new(1));
        let o = r.options(0);
        assert!(!o.is_empty());
        for &t in &o {
            assert!(r.world.can_research(slot(0), t));
        }
        let costs: Vec<i32> = o
            .iter()
            .map(|&t| r.world.effective_cost(slot(0), t, false))
            .collect();
        assert!(costs.windows(2).all(|w| w[0] <= w[1]), "{costs:?}");
    }

    #[test]
    fn beakers_buy_the_target_and_unlock_its_production() {
        let mut r = game();
        let mut dice = Rng::new(3);
        r.begin(&mut dice);
        let wc = tech("Warrior Code");
        // Japan starts without Warrior Code, so no Archers.
        assert!(!r.knows(0, wc));
        assert!(!can_build(0, Production::named("Archer")));
        r.pick(0, wc, &mut dice);
        r.world.players[slot(0) as usize].cities = 1;
        r.world.players[slot(0) as usize].rate = 5;
        let mut turns = 0;
        while !r.knows(0, wc) && turns < 60 {
            turns += 1;
            r.finish_turn(0, 5, turns, &mut dice);
        }
        assert!(r.knows(0, wc), "no discovery in 60 turns");
        assert!(can_build(0, Production::named("Archer")));
        // The human is asked again: the finished target is not re-run.
        assert!(r.needs_choice(0));
        // Rome (computer) started with it.
        assert!(can_build(1, Production::named("Archer")));
    }

    #[test]
    fn without_cities_nothing_is_researched() {
        let mut r = game();
        let mut dice = Rng::new(3);
        r.begin(&mut dice);
        let t = r.target(1).unwrap();
        for turn in 1..30 {
            r.finish_turn(1, 50, turn, &mut dice);
        }
        assert!(!r.knows(1, t), "the step does nothing without cities");
    }

    #[test]
    fn a_traded_advance_unlocks_without_research() {
        let mut r = game();
        let mut dice = Rng::new(5);
        r.begin(&mut dice);
        let bw = tech("Bronze Working");
        assert!(!r.knows(0, bw) && !can_build(0, Production::named("Spearman")));
        let ev = r.award(0, bw, &mut dice);
        assert!(
            ev.iter().any(
                |e| matches!(e, Event::Acquired { tech, by_research: false, .. } if *tech == bw)
            )
        );
        assert!(can_build(0, Production::named("Spearman")));
        // Awarding a known advance changes nothing.
        assert!(r.award(0, bw, &mut dice).is_empty());
    }

    #[test]
    fn giftable_is_known_by_the_giver_and_ready_for_the_taker() {
        let mut r = game();
        let mut dice = Rng::new(5);
        r.begin(&mut dice);
        for t in r.giftable(1, 0) {
            assert!(r.knows(1, t) && !r.knows(0, t));
            assert!(r.world.can_research(slot(0), t));
        }
    }

    #[test]
    fn an_advance_costs_less_when_contacts_know_it() {
        let mut r = game();
        let mut dice = Rng::new(5);
        r.begin(&mut dice);
        let t = r.options(0)[0];
        let alone = r.world.base_cost(slot(0), t, false);
        // Contact with every civ that knows it makes it cheaper.
        for civ in 1..civ_count() {
            r.world.players[slot(0) as usize].contact |= 1 << slot(civ);
        }
        let _ = r.award(1, t, &mut dice);
        let met = r.world.base_cost(slot(0), t, false);
        assert!(met < alone, "{met} !< {alone}");
    }

    #[test]
    fn the_production_locks_follow_what_is_known() {
        let mut r = game();
        let mut dice = Rng::new(5);
        r.begin(&mut dice);
        let japan = locked(0);
        assert!(japan.contains(&Production::named("Spearman")));
        assert!(!japan.contains(&Production::named("Warrior")));
        assert!(!japan.contains(&Production::named("Settler")));
        assert!(!locked(1).contains(&Production::named("Archer")));
        // Every locked building lacks its advance, is obsolete, or needs a
        // good the civ does not own.
        for civ in 0..civ_count() {
            for p in locked(civ).into_iter().filter(|p| p.is_building()) {
                let b = p.bldg().unwrap();
                let lacks = b.tech != NONE && !r.knows(civ, b.tech);
                let obsolete = b.obsolete >= 0 && r.knows(civ, b.obsolete);
                let goods = b.resources.iter().any(|&g| !owns_good(civ, g));
                assert!(
                    lacks || obsolete || goods,
                    "{} is locked for no reason",
                    b.name
                );
            }
        }
    }

    #[test]
    fn a_unique_unit_is_its_races_alone_and_a_resource_is_a_requirement() {
        let mut r = game();
        let mut dice = Rng::new(5);
        r.begin(&mut dice);
        for t in ["Bronze Working", "Iron Working"] {
            for civ in [0, 1] {
                if !r.knows(civ, tech(t)) {
                    r.award(civ, tech(t), &mut dice);
                }
            }
        }
        // Without Iron nobody can arm a Swordsman or a Legionary.
        assert!(!can_build(0, Production::named("Swordsman")));
        assert!(!can_build(1, Production::named("Legionary")));
        const IRON: u32 = 1 << 1;
        for civ in [0, 1] {
            assert!(set_goods(civ, IRON));
        }
        r.goods_changed();
        // Japan has the plain Swordsman, Rome its Legionary instead.
        assert!(can_build(0, Production::named("Swordsman")));
        assert!(!can_build(0, Production::named("Legionary")));
        assert!(can_build(1, Production::named("Legionary")));
        assert!(!can_build(1, Production::named("Swordsman")));
        // A wonder is built once.
        assert!(
            !can_build(0, Production::named("The Pyramids"))
                || required_tech(Production::named("The Pyramids")) != NONE
        );
        set_wonder_built(Production::named("The Pyramids"), true);
        assert!(!can_build(0, Production::named("The Pyramids")));
        set_wonder_built(Production::named("The Pyramids"), false);
    }

    #[test]
    fn announcements_go_to_the_human_only() {
        // Japan is the human; the mask is per test thread.
        crate::civs::set_controllers();
        let mut board = MessageBoard::default();
        let ev = [Event::Acquired {
            player: slot(1),
            tech: 0,
            by_research: true,
        }];
        announce(&ev, &mut board);
        assert!(
            board.text.is_empty(),
            "the computer's discoveries are not announced"
        );
        let ev = [Event::Acquired {
            player: slot(0),
            tech: 0,
            by_research: true,
        }];
        announce(&ev, &mut board);
        assert_eq!(board.text, "Our scientists have discovered Bronze Working!");
    }

    #[test]
    fn computer_era_entry_spawns_an_uprising_without_an_announcement() {
        crate::realm::reset();
        crate::civs::set_controllers();
        for researched in [true, false] {
            let mut r = game();
            let last = tech("Construction");
            r.world.turn = 10;
            r.world.players[slot(0) as usize].era = 1;
            for (i, row) in r.world.rules.techs.iter().enumerate() {
                if row.era == 0 && i as i32 != last {
                    r.world.known[i] |= 1 << slot(1);
                }
            }
            let p = &mut r.world.players[slot(1) as usize];
            p.known_count = r
                .world
                .known
                .iter()
                .filter(|&&mask| mask & (1 << slot(1)) != 0)
                .count() as i32;
            p.current = last;
            p.beakers = 10000;
            p.turns = 10;
            p.rate = 10;
            p.cities = 1;
            if !researched {
                // The external-acquisition path used by huts and trades. The
                // caller deliberately does not show research announcements.
                let ev = r.award(1, last, &mut MapRng::new(1));
                assert!(
                    ev.iter()
                        .any(|e| matches!(e, Event::BarbarianLanding { era: 1 }))
                );
            }
            let mut app = App::new();
            app.edit_schedule(Update, |s| {
                s.set_executor_kind(bevy::ecs::schedule::ExecutorKind::SingleThreaded);
            });
            let mut map = GameMap::generate_with_seed(1);
            for t in &mut map.tiles {
                t.camp = false;
            }
            // Four camps fill the native camp quota, so the uprising adds
            // exactly eight advanced units per existing camp.
            for (x, y) in crate::civs::starting_positions(&map) {
                let i = map.idx(x, y);
                map.tiles[i].camp = true;
            }
            app.insert_resource(map);
            app.insert_resource(r);
            app.insert_resource(CombatRng(MapRng::new(1)));
            app.insert_resource(Turn(10));
            app.init_resource::<Civilizations>();
            app.init_resource::<MessageBoard>();
            app.init_resource::<crate::barbarians::Barbarians>();
            app.insert_resource(crate::units::UnitArt::blank());
            app.add_message::<CivilizationEnded>();
            app.add_systems(Update, (end_turn, crate::barbarians::uprising).chain());
            app.world_mut().spawn(City::new(1, "Rome", 33, 33));
            if researched {
                app.world_mut().write_message(CivilizationEnded(1));
            }
            app.update();
            assert_eq!(app.world().resource::<Research>().era(1), 1);
            let horses = app
                .world_mut()
                .query::<&Unit>()
                .iter(app.world())
                .filter(|u| {
                    u.civ == crate::civs::BARBARIANS
                        && u.utype == crate::units::UnitType::named("Horseman")
                })
                .count();
            assert_eq!(horses, 32, "researched={researched}");
        }
    }

    #[test]
    fn a_hut_teaches_a_first_age_civ_one_advance_and_a_later_age_civ_none() {
        let mut r = game();
        r.begin(&mut Rng::new(1));
        let count = |r: &Research| (0..r.world.t()).filter(|&t| r.knows(0, t)).count();
        let before = count(&r);
        let (t, _) = r
            .hut_advance(0, &mut Rng::new(2))
            .expect("a first-age advance");
        assert!(r.knows(0, t));
        assert_eq!(count(&r), before + 1);
        r.world.players[slot(0) as usize].era = 1;
        assert!(r.hut_advance(0, &mut Rng::new(2)).is_none());
    }

    #[test]
    fn the_whole_tree_can_be_climbed_one_advance_at_a_time() {
        let mut r = game();
        let mut dice = Rng::new(3);
        let t = r.world.t();
        loop {
            let options = r.options(0);
            let Some(&next) = options.first() else { break };
            assert!(!r.knows(0, next));
            r.award(0, next, &mut dice);
            assert!(r.knows(0, next));
        }
        let known = (0..t).filter(|&a| r.knows(0, a)).count();
        assert_eq!(
            known as i32, t,
            "every advance is reachable, and nothing else is left to research"
        );
    }

    #[test]
    fn first_age_costs_scale_with_the_row() {
        // Four civilizations at Regent in a Standard world: 24 beakers a
        // point of the row (Bronze Working, row cost 3, is 72, `research.md`
        // 4.4), before the turn clamp of the effective cost.
        let r = game();
        let s = slot(0);
        for (name, cost) in [("Bronze Working", 3), ("Alphabet", 5), ("Masonry", 4)] {
            assert_eq!(r.world.base_cost(s, tech(name), false), 24 * cost, "{name}");
        }
    }

    /// Regression values of this port's valuation (`research-ai.md`), not
    /// numbers read from the executable: they catch an accidental change.
    #[test]
    fn the_valuation_of_the_first_choices_is_stable() {
        let mut r = game();
        r.begin(&mut Rng::new(1));
        let mut dice = Rng::new(1);
        let worth: Vec<(&str, i32)> = r
            .options(0)
            .into_iter()
            .map(|t| (TECH_NAMES[t as usize], r.worth(0, t, &mut dice)))
            .collect();
        assert_eq!(
            worth,
            [
                ("Pottery", 158),
                ("Bronze Working", 231),
                ("Masonry", 257),
                ("Alphabet", 286),
                ("Warrior Code", 215),
                ("Mysticism", 249)
            ]
        );
    }
}
