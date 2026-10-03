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
//! Deviations, all because the game has no such system yet: no governments,
//! wonders, Great Library or scientific-leader rolls; the science rate is
//! the fixed 50% (`cities::SCI_RATE`); four civilizations on a Regent game
//! of a standard-size world; only ten productions are gated by an advance.
use bevy::prelude::*;

use civ3mapgen::research::{Ctx, Dice, Event, NONE, World};
use civ3mapgen::research_ai::{Profile, Tables, Valuer, category_mask_from_flags};

use crate::cities::{City, Production, city_commerce, commerce_split};
use crate::civs::{CIV_COUNT, CivilizationEnded, Civilizations, is_ai};
use crate::combat::CombatRng;
use crate::diplomacy::Diplomacy;
use crate::features::{MessageBoard, post};
use crate::map::GameMap;
use crate::rng::MapRng;
use crate::rules_data::{self, ERA_NAMES, RACES, TECH_NAMES, UNLOCKS};
use crate::units::{Turn, Unit, def};

impl Dice for MapRng {
    fn below(&mut self, n: u32) -> i32 {
        MapRng::below(self, n)
    }
}

/// The slot a civ holds in the research and diplomacy worlds (slot 0 is the
/// barbarians).
pub fn slot(civ: usize) -> u32 {
    civ as u32 + 1
}

/// The civ in a slot.
pub fn civ_of(slot: u32) -> usize {
    slot as usize - 1
}

/// `DIFF[2]` (Regent) and the standard `WSIZ` row.
const DIFFICULTY: usize = 2;
const WORLD_SIZE: usize = 2;

/// Productions each civ cannot build yet, one bit per `Production::ALL`
/// entry. A process-wide setting like `civs::AI_MASK`: `City::buildable` is
/// a plain function. Nothing is locked until the research world says so.
#[cfg(not(test))]
static LOCKED: [std::sync::atomic::AtomicU16; CIV_COUNT] = [const { std::sync::atomic::AtomicU16::new(0) }; CIV_COUNT];

// Under test each thread has locks of its own, as with `civs::AI_MASK`.
#[cfg(test)]
thread_local! {
    static LOCKED: [std::cell::Cell<u16>; CIV_COUNT] = const { [const { std::cell::Cell::new(0) }; CIV_COUNT] };
}

fn locked_bits(civ: usize) -> u16 {
    #[cfg(not(test))]
    return LOCKED[civ].load(std::sync::atomic::Ordering::Relaxed);
    #[cfg(test)]
    return LOCKED.with(|l| l[civ].get());
}

fn set_locked_bits(civ: usize, mask: u16) {
    #[cfg(not(test))]
    LOCKED[civ].store(mask, std::sync::atomic::Ordering::Relaxed);
    #[cfg(test)]
    LOCKED.with(|l| l[civ].set(mask));
}

fn index(p: Production) -> usize {
    Production::ALL.iter().position(|&q| q == p).expect("every production is listed")
}

/// The advance a production needs, `-1` for none.
pub fn required_tech(p: Production) -> i32 {
    UNLOCKS[index(p)].1
}

/// Whether `civ` has the advance for `p`.
pub fn can_build(civ: usize, p: Production) -> bool {
    locked_bits(civ) >> index(p) & 1 == 0
}

/// Everything `civ` cannot build yet.
pub fn locked(civ: usize) -> Vec<Production> {
    Production::ALL.iter().copied().filter(|&p| !can_build(civ, p)).collect()
}

/// Name of an advance.
pub fn tech_name(t: i32) -> &'static str {
    usize::try_from(t).ok().and_then(|i| TECH_NAMES.get(i)).copied().unwrap_or("Future Technology")
}

/// All research state of the game.
#[derive(Resource)]
pub struct Research {
    pub world: World,
    tables: Tables,
    profiles: Vec<Profile>,
    categories: Vec<u32>,
    started: bool,
}

impl Research {
    /// The game's four civilizations, nothing known yet.
    pub fn new() -> Self {
        let mut world = World::new(rules_data::rules());
        world.difficulty_cost_factor = rules_data::DIFFICULTY_COST_FACTOR[DIFFICULTY];
        world.size_tech_rate = rules_data::WORLD_TECH_RATE[WORLD_SIZE];
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
        let categories = world.rules.techs.iter().map(|t| category_mask_from_flags(t.flags)).collect();
        Research { world, tables: rules_data::tables(), profiles, categories, started: false }
    }

    /// Run `f` with the AI valuation as the brain of the research code.
    fn with_brain<R>(&mut self, dice: &mut dyn Dice, f: impl FnOnce(&mut World, &mut Ctx<'_>) -> R) -> R {
        let Research { world, tables, profiles, categories, .. } = self;
        let mut brain = Valuer { tables, profiles, categories, space_race: false, wonder_built: &|_| false };
        let mut ctx = Ctx { brain: &mut brain, dice };
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
        let mut v: Vec<i32> = (0..self.world.t()).filter(|&t| self.world.can_research(s, t)).collect();
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
        Some((self.world.players[s as usize].beakers, self.world.effective_cost(s, t, false)))
    }

    /// Beakers a turn.
    pub fn rate(&self, civ: usize) -> i32 {
        self.world.players[slot(civ) as usize].rate
    }

    /// Refresh the bits `City::buildable` reads.
    fn sync_locks(&self) {
        for civ in 0..CIV_COUNT {
            let mut mask = 0u16;
            for (i, &p) in Production::ALL.iter().enumerate() {
                let need = required_tech(p);
                if need != NONE && !self.knows(civ, need) {
                    mask |= 1 << i;
                }
            }
            set_locked_bits(civ, mask);
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
            for civ in 0..CIV_COUNT {
                w.start_grants(slot(civ), &mut events, ctx);
            }
            for civ in 0..CIV_COUNT {
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
    pub fn finish_turn(&mut self, civ: usize, beakers: i32, turn: i32, dice: &mut dyn Dice) -> Vec<Event> {
        let s = slot(civ);
        let mut events = vec![];
        self.world.turn = turn;
        self.world.add_beakers(s, beakers);
        self.with_brain(dice, |w, ctx| w.step(s, &mut events, ctx));
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
        self.with_brain(dice, |w, ctx| w.acquire(s, t, false, true, true, &mut events, ctx));
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
            Event::Acquired { player, tech, by_research } if !is_ai(civ_of(player)) => {
                let name = tech_name(tech);
                post(
                    board,
                    if by_research {
                        format!("Our scientists have discovered {name}!")
                    } else {
                        format!("We have learned {name}.")
                    },
                );
            }
            Event::EnteredEra { player, era } if !is_ai(civ_of(player)) => {
                post(board, format!("Our civilization enters the {} Age.", ERA_NAMES[era as usize]));
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
    let mut rate = [0i32; CIV_COUNT];
    let mut count = [0i32; CIV_COUNT];
    let mut defenders = [0i32; CIV_COUNT];
    for c in &cities {
        rate[c.civ] += i32::from(commerce_split(city_commerce(&map, c)).1);
        count[c.civ] += 1;
    }
    for u in &units {
        if def(u.utype).defense > 0 && def(u.utype).attack > 0 {
            defenders[u.civ] += 1;
        }
    }
    research.world.turn = turn.0 as i32;
    for civ in 0..CIV_COUNT {
        let s = slot(civ) as usize;
        let me = &mut research.world.players[s];
        me.rate = rate[civ];
        me.cities = count[civ];
        me.contact = diplomacy.contacts(civ);
        research.profiles[s].defenders = defenders[civ];
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
) {
    for ev in ended.read() {
        let civ = ev.0;
        if civs.outcome.is_some() {
            continue;
        }
        let beakers = research.rate(civ);
        let events = research.finish_turn(civ, beakers, turn.0 as i32, &mut rng.0);
        // The computer's discoveries are its own business.
        if is_ai(civ) {
            if std::env::var("CIV3_AI_LOG").is_ok() {
                for e in &events {
                    if let Event::Acquired { tech, .. } = e {
                        println!("research: turn {} {} learns {}", turn.0, crate::civs::CIVS[civ].name, tech_name(*tech));
                    }
                }
            }
            continue;
        }
        announce(&events, &mut board);
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
        for civ in 0..CIV_COUNT {
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

    fn tech(name: &str) -> i32 {
        TECH_NAMES.iter().position(|&n| n == name).unwrap() as i32
    }

    #[test]
    fn the_unlock_table_lists_the_productions_in_order() {
        for (p, (name, _)) in Production::ALL.iter().zip(UNLOCKS.iter()) {
            assert_eq!(p.name(), *name);
        }
        assert_eq!(required_tech(Production::Warrior), NONE);
        assert_eq!(required_tech(Production::Archer), tech("Warrior Code"));
        assert_eq!(required_tech(Production::Spearman), tech("Bronze Working"));
        assert_eq!(required_tech(Production::Horseman), tech("Horseback Riding"));
        assert_eq!(required_tech(Production::Granary), tech("Pottery"));
        assert_eq!(required_tech(Production::Temple), tech("Ceremonial Burial"));
    }

    #[test]
    fn a_new_game_grants_each_civ_its_free_advances() {
        let mut r = game();
        let mut dice = Rng::new(7);
        r.begin(&mut dice);
        for (civ, race) in RACES.iter().enumerate() {
            for &t in race.free_techs.iter().filter(|&&t| t >= 0) {
                assert!(r.knows(civ, t), "{} lacks {}", crate::civs::CIVS[civ].name, tech_name(t));
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
        for civ in 1..CIV_COUNT {
            let t = r.target(civ).unwrap_or_else(|| panic!("civ {civ} has no target"));
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
        let costs: Vec<i32> = o.iter().map(|&t| r.world.effective_cost(slot(0), t, false)).collect();
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
        assert!(!can_build(0, Production::Archer));
        r.pick(0, wc, &mut dice);
        r.world.players[slot(0) as usize].cities = 1;
        r.world.players[slot(0) as usize].rate = 5;
        let mut turns = 0;
        while !r.knows(0, wc) && turns < 60 {
            turns += 1;
            r.finish_turn(0, 5, turns, &mut dice);
        }
        assert!(r.knows(0, wc), "no discovery in 60 turns");
        assert!(can_build(0, Production::Archer));
        // The human is asked again: the finished target is not re-run.
        assert!(r.needs_choice(0));
        // Rome (computer) started with it.
        assert!(can_build(1, Production::Archer));
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
        assert!(!r.knows(0, bw) && !can_build(0, Production::Spearman));
        let ev = r.award(0, bw, &mut dice);
        assert!(ev.iter().any(|e| matches!(e, Event::Acquired { tech, by_research: false, .. } if *tech == bw)));
        assert!(can_build(0, Production::Spearman));
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
        for civ in 1..CIV_COUNT {
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
        assert!(japan.contains(&Production::Spearman));
        assert!(!japan.contains(&Production::Warrior));
        assert!(!japan.contains(&Production::Settler));
        assert!(!locked(1).contains(&Production::Archer));
        // Every locked production names an advance the civ lacks.
        for civ in 0..CIV_COUNT {
            for p in locked(civ) {
                assert!(!r.knows(civ, required_tech(p)));
            }
        }
    }

    #[test]
    fn announcements_go_to_the_human_only() {
        // Japan is the human; the mask is per test thread.
        crate::civs::set_controllers();
        let mut board = MessageBoard::default();
        let ev = [Event::Acquired { player: slot(1), tech: 0, by_research: true }];
        announce(&ev, &mut board);
        assert!(board.text.is_empty(), "the computer's discoveries are not announced");
        let ev = [Event::Acquired { player: slot(0), tech: 0, by_research: true }];
        announce(&ev, &mut board);
        assert_eq!(board.text, "Our scientists have discovered Bronze Working!");
    }

    #[test]
    fn a_hut_teaches_a_first_age_civ_one_advance_and_a_later_age_civ_none() {
        let mut r = game();
        r.begin(&mut Rng::new(1));
        let count = |r: &Research| (0..r.world.t()).filter(|&t| r.knows(0, t)).count();
        let before = count(&r);
        let (t, _) = r.hut_advance(0, &mut Rng::new(2)).expect("a first-age advance");
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
        assert_eq!(known as i32, t, "every advance is reachable, and nothing else is left to research");
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
            [("Pottery", 158), ("Bronze Working", 231), ("Masonry", 257), ("Alphabet", 286), ("Warrior Code", 215), ("Mysticism", 249)]
        );
    }
}
