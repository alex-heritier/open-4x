//! The advisors: the Science Advisor (what to research), the Foreign Advisor
//! (who we have met and where we stand), the talk screen (deals), and the
//! questions that interrupt a turn: a proposal from the computer, and "declare
//! war?" when a soldier is ordered against a civ at peace.
//!
//! One modal is up at a time. `Advisors::screen` says which; the panel is
//! rebuilt whenever the resource changes, so a button only changes state.
use bevy::prelude::*;

use civ3mapgen::diplomacy::Clause;

use crate::cities::{City, CityView, Treasury};
use crate::civs::{CIV_COUNT, Civilizations, is_ai};
use crate::combat::CombatRng;
use crate::diplomacy::{Deal, Diplomacy, attitude_label, clause_text, people, verdict_text};
use crate::features::{MessageBoard, post};
use crate::map::GameMap;
use crate::production_prompt::ProductionPrompts;
use crate::research::{Research, announce, tech_name};
use crate::units::{Turn, Unit};
use civ3mapgen::diplomacy::Verdict;

/// Gold moved by one click of the plus and minus buttons.
const GOLD_STEP: i32 = 10;
/// Advances listed on each side of the talk screen.
const TECHS_SHOWN: usize = 8;
/// Advances listed in the Science Advisor.
const OPTIONS_SHOWN: usize = 12;

/// Which modal is up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Screen {
    #[default]
    Closed,
    Science,
    Foreign,
    /// Talking with this civ.
    Talk(usize),
    /// The computer's proposal at the front of the queue.
    Proposal,
    /// "Declare war?" for the strike in `Diplomacy::war_ask`.
    WarAsk,
}

#[derive(Resource, Default)]
pub struct Advisors {
    pub screen: Screen,
    /// The deal on the table of the talk screen: the human gives `from_a`.
    deal: Deal,
    /// The last thing said on the talk screen.
    note: String,
    /// Declaring war from the talk screen takes two clicks.
    armed: bool,
}

impl Advisors {
    /// A modal is up: orders and the end of the turn wait.
    pub fn is_open(&self) -> bool {
        self.screen != Screen::Closed
    }

    fn open(&mut self, screen: Screen) {
        *self = Advisors { screen, ..Default::default() };
    }

    /// The talk screen with an empty table between `me` and `other`.
    fn talk_with(&mut self, me: usize, other: usize) {
        *self = Advisors { screen: Screen::Talk(other), deal: Deal::new(me, other), ..Default::default() };
    }

    fn close(&mut self) {
        self.open(Screen::Closed);
    }
}

#[derive(Component)]
pub struct AdvisorRoot;

/// What a button does.
#[derive(Component, Clone, Debug, PartialEq)]
pub enum Action {
    Close,
    Science,
    Foreign,
    /// The human picks this advance to research.
    Pick(i32),
    Talk(usize),
    /// A treaty clause on or off the table.
    Treaty(Clause),
    /// An advance on or off the table; `true` is the one we give.
    Tech(bool, i32),
    Gold(bool, i32),
    GoldPerTurn(bool, i32),
    Propose,
    DeclareWar,
    Accept,
    Decline,
    WarYes,
    WarNo,
}

impl Action {
    /// The name `CIV3_SCRIPT`'s `adv` action presses it by.
    pub fn script_name(&self) -> String {
        let side = |give: &bool| if *give { "Give" } else { "Get" };
        match self {
            Action::Close => "Close".into(),
            Action::Science => "Science".into(),
            Action::Foreign => "Foreign".into(),
            Action::Pick(t) => format!("Pick:{}", tech_name(*t)),
            Action::Talk(c) => format!("Talk:{c}"),
            Action::Treaty(c) => format!("Treaty:{}", clause_text(c)),
            Action::Tech(give, t) => format!("{}:{}", side(give), tech_name(*t)),
            Action::Gold(give, d) => format!("{}Gold:{d}", side(give)),
            Action::GoldPerTurn(give, d) => format!("{}GoldPerTurn:{d}", side(give)),
            Action::Propose => "Propose".into(),
            Action::DeclareWar => "DeclareWar".into(),
            Action::Accept => "Accept".into(),
            Action::Decline => "Decline".into(),
            Action::WarYes => "WarYes".into(),
            Action::WarNo => "WarNo".into(),
        }
    }
}

// ---------------------------------------------------------------------
// When a modal opens
// ---------------------------------------------------------------------

/// The human whose chair it is, if a human is playing.
fn human(civs: &Civilizations) -> Option<usize> {
    (!is_ai(civs.active) && civs.outcome.is_none()).then(|| civs.viewer())
}

/// F6 and F4 open the advisors; Escape closes one that is not a question.
pub fn hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    civs: Res<Civilizations>,
    prompts: Res<ProductionPrompts>,
    view: Res<CityView>,
    research: Res<Research>,
    mut advisors: ResMut<Advisors>,
) {
    let Some(me) = human(&civs) else { return };
    if advisors.is_open() {
        let forced = advisors.screen == Screen::Science && research.needs_choice(me);
        let question = matches!(advisors.screen, Screen::Proposal | Screen::WarAsk);
        if keys.just_pressed(KeyCode::Escape) && !forced && !question {
            advisors.close();
        }
        return;
    }
    if prompts.blocks(civs.active) || view.0.is_some() {
        return;
    }
    if keys.just_pressed(KeyCode::F6) {
        advisors.open(Screen::Science);
    } else if keys.just_pressed(KeyCode::F4) {
        advisors.open(Screen::Foreign);
    }
}

/// Things that stop the turn: a strike against a civ at peace, a research
/// target to pick, a proposal to answer.
pub fn interrupt(
    civs: Res<Civilizations>,
    prompts: Res<ProductionPrompts>,
    view: Res<CityView>,
    research: Res<Research>,
    diplomacy: Res<Diplomacy>,
    mut advisors: ResMut<Advisors>,
) {
    let Some(me) = human(&civs) else { return };
    if diplomacy.war_ask.is_some() {
        if advisors.screen != Screen::WarAsk {
            advisors.open(Screen::WarAsk);
        }
        return;
    }
    if advisors.is_open() || prompts.blocks(civs.active) || view.0.is_some() {
        return;
    }
    if research.needs_choice(me) {
        advisors.open(Screen::Science);
    } else if diplomacy.proposals.iter().any(|p| p.to == me) {
        advisors.open(Screen::Proposal);
    }
}

// ---------------------------------------------------------------------
// The panels
// ---------------------------------------------------------------------

const INK: Color = Color::srgb(0.05, 0.3, 0.65);
const SELECTED: Color = Color::srgba(0.25, 0.55, 0.3, 0.4);
const IDLE: Color = Color::srgba(0.65, 0.60, 0.40, 0.18);

fn text(parent: &mut ChildSpawnerCommands, font: &Handle<Font>, s: impl Into<String>, size: f32) {
    parent.spawn((
        Text::new(s),
        TextFont { font: font.clone(), font_size: size, ..default() },
        TextColor(Color::BLACK),
    ));
}

fn button(parent: &mut ChildSpawnerCommands, font: &Handle<Font>, label: String, action: Action, on: bool) {
    parent
        .spawn((
            Button,
            action,
            Node {
                min_height: Val::Px(32.0),
                padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(if on { SELECTED } else { IDLE }),
        ))
        .with_children(|b| {
            b.spawn((
                Text::new(label),
                TextFont { font: font.clone(), font_size: 18.0, ..default() },
                TextColor(INK),
            ));
        });
}

/// A button that is on the table or not.
fn toggle(parent: &mut ChildSpawnerCommands, font: &Handle<Font>, label: String, action: Action, on: bool) {
    let mark = if on { "* " } else { "  " };
    button(parent, font, format!("{mark}{label}"), action, on);
}

fn row(parent: &mut ChildSpawnerCommands, f: impl FnOnce(&mut ChildSpawnerCommands)) {
    parent
        .spawn(Node { column_gap: Val::Px(10.0), align_items: AlignItems::Center, ..default() })
        .with_children(f);
}

fn column(parent: &mut ChildSpawnerCommands, f: impl FnOnce(&mut ChildSpawnerCommands)) {
    parent
        .spawn(Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(4.0), flex_grow: 1.0, flex_basis: Val::Px(0.0), ..default() })
        .with_children(f);
}

/// Everything the panels read.
struct View<'a> {
    me: usize,
    research: &'a Research,
    diplomacy: &'a Diplomacy,
    treasury: &'a Treasury,
    advisors: &'a Advisors,
    facts: &'a crate::diplomacy::Facts,
}

fn turns_text(turns: i32) -> String {
    match turns {
        9999.. => "never".into(),
        1 => "1 turn".into(),
        n => format!("{n} turns"),
    }
}

fn science(p: &mut ChildSpawnerCommands, font: &Handle<Font>, v: &View) {
    let (me, r) = (v.me, v.research);
    let forced = r.needs_choice(me);
    text(p, font, "Science Advisor", 30.0);
    let line = match (r.target(me), r.progress(me)) {
        (Some(t), Some((have, cost))) => {
            format!("Researching {}: {have} of {cost} beakers, {}.", tech_name(t), turns_text(r.turns(me, t)))
        }
        _ if forced => "Excellency, what shall our scientists study?".into(),
        _ => "Our scientists have no target.".into(),
    };
    text(p, font, line, 20.0);
    text(p, font, format!("Our science yields {} beakers a turn.", r.rate(me)), 18.0);
    let options = r.options(me);
    if options.is_empty() {
        text(p, font, "There is nothing left we can study.", 18.0);
    } else if !forced && r.progress(me).is_some_and(|(have, _)| have > 0) {
        text(p, font, "Changing the target forfeits the beakers stored.", 16.0);
    }
    for &t in options.iter().take(OPTIONS_SHOWN) {
        let now = r.target(me) == Some(t);
        button(p, font, format!("{} ({})", tech_name(t), turns_text(r.turns(me, t))), Action::Pick(t), now);
    }
    if options.len() > OPTIONS_SHOWN {
        text(p, font, format!("...and {} more.", options.len() - OPTIONS_SHOWN), 16.0);
    }
    if !forced || options.is_empty() {
        button(p, font, "Close".into(), Action::Close, false);
    }
}

/// What the Foreign Advisor says about the standing with `other`.
fn standing(v: &View, other: usize) -> String {
    let d = v.diplomacy;
    if !d.contact(v.me, other) {
        return "No contact".into();
    }
    let mood = attitude_label(d.attitude(v.facts, other, v.me));
    let status = if d.at_war(v.me, other) { "At war" } else { "At peace" };
    format!("{mood}. {status}{}", d.treaties_text(v.me, other))
}

fn foreign(p: &mut ChildSpawnerCommands, font: &Handle<Font>, v: &View) {
    text(p, font, "Foreign Advisor", 30.0);
    for other in (0..CIV_COUNT).filter(|&o| o != v.me) {
        let met = v.diplomacy.contact(v.me, other);
        row(p, |r| {
            text(r, font, format!("The {}: {}", people(other), standing(v, other)), 18.0);
            if met {
                button(r, font, "Talk".into(), Action::Talk(other), false);
            }
        });
    }
    button(p, font, "Close".into(), Action::Close, false);
}

fn talk(p: &mut ChildSpawnerCommands, font: &Handle<Font>, v: &View, other: usize) {
    let (me, d) = (v.me, v.diplomacy);
    let deal = &v.advisors.deal;
    text(p, font, format!("The {}", people(other)), 30.0);
    text(p, font, standing(v, other), 18.0);
    let gold = |give: bool| {
        let from = if give { &deal.from_a } else { &deal.from_b };
        let once = from.iter().find_map(|c| if let Clause::Gold(n) = c { Some(*n) } else { None }).unwrap_or(0);
        let per = from.iter().find_map(|c| if let Clause::GoldPerTurn(n) = c { Some(*n) } else { None }).unwrap_or(0);
        (once, per)
    };
    row(p, |cols| {
        for give in [true, false] {
            let (giver, taker) = if give { (me, other) } else { (other, me) };
            let from = if give { &deal.from_a } else { &deal.from_b };
            column(cols, |c| {
                text(c, font, if give { "We give" } else { "They give" }, 22.0);
                let techs = v.research.giftable(giver, taker);
                for &t in techs.iter().take(TECHS_SHOWN) {
                    toggle(c, font, tech_name(t).into(), Action::Tech(give, t), from.contains(&Clause::Tech(t)));
                }
                if techs.len() > TECHS_SHOWN {
                    text(c, font, format!("...and {} more.", techs.len() - TECHS_SHOWN), 16.0);
                }
                let (once, per) = gold(give);
                row(c, |g| {
                    button(g, font, "-".into(), Action::Gold(give, -GOLD_STEP), false);
                    text(g, font, format!("{once} of {} gold", v.treasury.0[giver]), 18.0);
                    button(g, font, "+".into(), Action::Gold(give, GOLD_STEP), false);
                });
                row(c, |g| {
                    button(g, font, "-".into(), Action::GoldPerTurn(give, -1), false);
                    text(g, font, format!("{per} gold a turn"), 18.0);
                    button(g, font, "+".into(), Action::GoldPerTurn(give, 1), false);
                });
            });
        }
    });
    // Treaties are offered when the clause alone would stand.
    let mut options = vec![Clause::Peace, Clause::RightOfPassage, Clause::MutualProtection];
    for x in (0..CIV_COUNT).filter(|&x| x != me && x != other) {
        options.push(Clause::MilitaryAlliance(crate::research::slot(x)));
        options.push(Clause::Embargo(crate::research::slot(x)));
    }
    let mut treaties = vec![];
    for c in options {
        let mut alone = Deal::new(me, other);
        alone.from_a.push(c.clone());
        if d.check(v.research, &v.treasury.0, &alone).is_ok() {
            treaties.push(c);
        }
    }
    if !treaties.is_empty() {
        text(p, font, "Treaties", 22.0);
        for c in treaties {
            let on = deal.from_a.contains(&c);
            toggle(p, font, clause_text(&c), Action::Treaty(c), on);
        }
    }
    if !v.advisors.note.is_empty() {
        text(p, font, v.advisors.note.clone(), 20.0);
    }
    row(p, |r| {
        button(r, font, "Propose".into(), Action::Propose, false);
        if !d.at_war(me, other) {
            let label = if v.advisors.armed { "Really declare war?" } else { "Declare war" };
            button(r, font, label.into(), Action::DeclareWar, v.advisors.armed);
        }
        button(r, font, "Foreign Advisor".into(), Action::Foreign, false);
        button(r, font, "Leave".into(), Action::Close, false);
    });
}

fn describe(clauses: &[Clause]) -> String {
    if clauses.is_empty() {
        return "nothing".into();
    }
    clauses.iter().map(clause_text).collect::<Vec<_>>().join(", ")
}

fn proposal(p: &mut ChildSpawnerCommands, font: &Handle<Font>, v: &View) {
    let Some(offer) = v.diplomacy.proposals.iter().find(|q| q.to == v.me) else { return };
    text(p, font, format!("The {} propose a trade", people(offer.from)), 30.0);
    text(p, font, format!("They give: {}.", describe(&offer.deal.from_b)), 20.0);
    text(p, font, format!("We give: {}.", describe(&offer.deal.from_a)), 20.0);
    row(p, |r| {
        button(r, font, "Accept".into(), Action::Accept, false);
        button(r, font, "Decline".into(), Action::Decline, false);
    });
}

fn war_ask(p: &mut ChildSpawnerCommands, font: &Handle<Font>, v: &View) {
    let Some(ask) = v.diplomacy.war_ask else { return };
    text(p, font, "Declare war?", 30.0);
    text(
        p,
        font,
        format!("We are at peace with the {}. Our soldiers cannot strike them unless we declare war.", people(ask.target)),
        20.0,
    );
    row(p, |r| {
        button(r, font, "Declare war".into(), Action::WarYes, false);
        button(r, font, "Cancel".into(), Action::WarNo, false);
    });
}

/// Rebuild the panel when the screen changes.
#[allow(clippy::too_many_arguments)]
pub fn show(
    mut commands: Commands,
    advisors: Res<Advisors>,
    roots: Query<Entity, With<AdvisorRoot>>,
    assets: Res<AssetServer>,
    research: Res<Research>,
    diplomacy: Res<Diplomacy>,
    treasury: Res<Treasury>,
    civs: Res<Civilizations>,
    map: Res<GameMap>,
    cities: Query<&City>,
    units: Query<&Unit>,
) {
    if !advisors.is_changed() {
        return;
    }
    for root in &roots {
        commands.entity(root).despawn();
    }
    if !advisors.is_open() {
        return;
    }
    let font = assets.load("gen/fonts/lsans.ttf");
    let cs: Vec<&City> = cities.iter().collect();
    let us: Vec<&Unit> = units.iter().collect();
    let facts = diplomacy.facts(&map, &research, &cs, &us);
    let v = View { me: civs.viewer(), research: &research, diplomacy: &diplomacy, treasury: &treasury, advisors: &advisors, facts: &facts };
    commands
        .spawn((
            AdvisorRoot,
            GlobalZIndex(100),
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: Val::Px(if matches!(advisors.screen, Screen::Talk(_)) { 760.0 } else { 520.0 }),
                    max_width: Val::Percent(96.0),
                    max_height: Val::Percent(96.0),
                    padding: UiRect::all(Val::Px(20.0)),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(6.0),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                ImageNode::new(assets.load("gen/cityscreen/ProductionQueueBox.png")),
                BackgroundColor(Color::srgb(0.94, 0.91, 0.77)),
                BorderColor::all(Color::srgb(0.25, 0.4, 0.28)),
            ))
            .with_children(|panel| match advisors.screen {
                Screen::Science => science(panel, &font, &v),
                Screen::Foreign => foreign(panel, &font, &v),
                Screen::Talk(other) => talk(panel, &font, &v, other),
                Screen::Proposal => proposal(panel, &font, &v),
                Screen::WarAsk => war_ask(panel, &font, &v),
                Screen::Closed => {}
            });
        });
}

// ---------------------------------------------------------------------
// The answers
// ---------------------------------------------------------------------

/// Put `c` on the table on the human's side (`give`) or the other's, or take
/// it off if it is there.
fn toggle_clause(deal: &mut Deal, give: bool, c: Clause) {
    let side = if give { &mut deal.from_a } else { &mut deal.from_b };
    match side.iter().position(|x| *x == c) {
        Some(i) => {
            side.remove(i);
        }
        None => side.push(c),
    }
}

/// Move the one gold clause of a side by `delta`, dropping it at zero.
fn move_gold(deal: &mut Deal, give: bool, delta: i32, per_turn: bool, purse: u32) {
    let side = if give { &mut deal.from_a } else { &mut deal.from_b };
    let is = |c: &Clause| matches!((c, per_turn), (Clause::Gold(_), false) | (Clause::GoldPerTurn(_), true));
    let now = side
        .iter()
        .find_map(|c| match c {
            Clause::Gold(n) | Clause::GoldPerTurn(n) if is(c) => Some(*n),
            _ => None,
        })
        .unwrap_or(0);
    let next = (now + delta).clamp(0, purse as i32);
    side.retain(|c| !is(c));
    if next > 0 {
        side.push(if per_turn { Clause::GoldPerTurn(next) } else { Clause::Gold(next) });
    }
}

#[allow(clippy::too_many_arguments)]
pub fn respond(
    buttons: Query<(&Interaction, &Action), Changed<Interaction>>,
    civs: Res<Civilizations>,
    map: Res<GameMap>,
    turn: Res<Turn>,
    cities: Query<&City>,
    mut units: Query<&mut Unit>,
    mut advisors: ResMut<Advisors>,
    mut research: ResMut<Research>,
    mut diplomacy: ResMut<Diplomacy>,
    mut treasury: ResMut<Treasury>,
    mut rng: ResMut<CombatRng>,
    mut board: ResMut<MessageBoard>,
) {
    let Some((_, action)) = buttons.iter().find(|(i, _)| **i == Interaction::Pressed) else { return };
    let action = action.clone();
    let me = civs.viewer();
    let screen = advisors.screen;
    let facts = {
        let us: Vec<&Unit> = units.iter().collect();
        let cs: Vec<&City> = cities.iter().collect();
        diplomacy.facts(&map, &research, &cs, &us)
    };
    match action {
        Action::Close => advisors.close(),
        Action::Science => advisors.open(Screen::Science),
        Action::Foreign => advisors.open(Screen::Foreign),
        Action::Talk(other) => advisors.talk_with(me, other),
        Action::Pick(t) => {
            research.pick(me, t, &mut rng.0);
            advisors.close();
        }
        Action::Treaty(c) => {
            advisors.note.clear();
            toggle_clause(&mut advisors.deal, true, c);
        }
        Action::Tech(give, t) => {
            advisors.note.clear();
            toggle_clause(&mut advisors.deal, give, Clause::Tech(t));
        }
        Action::Gold(give, delta) | Action::GoldPerTurn(give, delta) => {
            let Screen::Talk(other) = screen else { return };
            let per_turn = matches!(action, Action::GoldPerTurn(..));
            let giver = if give { me } else { other };
            advisors.note.clear();
            move_gold(&mut advisors.deal, give, delta, per_turn, treasury.0[giver]);
        }
        Action::Propose => {
            let Screen::Talk(other) = screen else { return };
            let deal = advisors.deal.clone();
            match diplomacy.check(&research, &treasury.0, &deal) {
                Err(why) => advisors.note = why.into(),
                Ok(()) => {
                    let verdict = diplomacy.weigh(&facts, &research, other, &deal, &mut rng.0);
                    advisors.note = verdict_text(verdict).into();
                    if verdict == Verdict::Accept {
                        let events = diplomacy.execute(&facts, &mut research, &mut treasury.0, &deal, turn.0 as i32, &mut rng.0, &mut board);
                        announce(&events, &mut board);
                        advisors.deal = Deal::new(me, other);
                    }
                }
            }
        }
        Action::DeclareWar => {
            let Screen::Talk(other) = screen else { return };
            if !advisors.armed {
                advisors.armed = true;
                advisors.note = "Press again to declare war.".into();
                return;
            }
            diplomacy.declare(&facts, me, other, 0, &mut board);
            advisors.talk_with(me, other);
            advisors.note = format!("We are at war with the {}.", people(other));
        }
        Action::Accept | Action::Decline => {
            let Some(i) = diplomacy.proposals.iter().position(|p| p.to == me) else {
                advisors.close();
                return;
            };
            let offer = diplomacy.proposals.remove(i).expect("the proposal was just found");
            if action == Action::Accept && diplomacy.check(&research, &treasury.0, &offer.deal).is_ok() {
                let events = diplomacy.execute(&facts, &mut research, &mut treasury.0, &offer.deal, turn.0 as i32, &mut rng.0, &mut board);
                announce(&events, &mut board);
                post(&mut board, format!("We have a deal with the {}.", people(offer.from)));
            }
            advisors.close();
        }
        Action::WarYes | Action::WarNo => {
            let Some(ask) = diplomacy.war_ask.take() else {
                advisors.close();
                return;
            };
            if action == Action::WarYes {
                diplomacy.declare(&facts, me, ask.target, 0, &mut board);
                // The strike goes ahead as the order was given.
                if let Ok(mut u) = units.get_mut(ask.attacker) {
                    u.path = [ask.to].into();
                }
            }
            advisors.close();
        }
    }
}

// ---------------------------------------------------------------------
// The research line of the info box
// ---------------------------------------------------------------------

/// "Bronze Working 12/38" under the gold line.
pub fn update_science_line(
    civs: Res<Civilizations>,
    research: Res<Research>,
    mut line: Query<&mut Text, With<crate::actionbar::BoxScience>>,
) {
    let Ok(mut text) = line.single_mut() else { return };
    let me = civs.viewer();
    let shown = match (research.target(me), research.progress(me)) {
        (Some(t), Some((have, cost))) => format!("{} {have}/{cost}", tech_name(t)),
        _ if research.needs_choice(me) => "Choose research (F6)".into(),
        _ => "No research".into(),
    };
    if text.0 != shown {
        text.0 = shown;
    }
}

/// Two small buttons for the mouse: Science (F6) and Foreign (F4).
pub fn spawn_buttons(mut commands: Commands, assets: Res<AssetServer>) {
    let font = assets.load("gen/fonts/lsans.ttf");
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(8.0),
                right: Val::Px(8.0),
                column_gap: Val::Px(4.0),
                ..default()
            },
            GlobalZIndex(5),
        ))
        .with_children(|bar| {
            for (label, action) in [("Science (F6)", Action::Science), ("Foreign (F4)", Action::Foreign)] {
                bar.spawn((
                    Button,
                    OpenButton(action),
                    Node { padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)), ..default() },
                    BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
                ))
                .with_children(|b| {
                    b.spawn((
                        Text::new(label),
                        TextFont { font: font.clone(), font_size: 15.0, ..default() },
                        TextColor(Color::srgb(1.0, 0.95, 0.7)),
                    ));
                });
            }
        });
}

#[derive(Component)]
pub struct OpenButton(Action);

/// A click on one of those buttons, when nothing else is up.
pub fn open_buttons(
    buttons: Query<(&Interaction, &OpenButton), Changed<Interaction>>,
    civs: Res<Civilizations>,
    prompts: Res<ProductionPrompts>,
    view: Res<CityView>,
    mut advisors: ResMut<Advisors>,
) {
    if human(&civs).is_none() || advisors.is_open() || prompts.blocks(civs.active) || view.0.is_some() {
        return;
    }
    for (interaction, OpenButton(action)) in &buttons {
        if *interaction == Interaction::Pressed {
            match action {
                Action::Science => advisors.open(Screen::Science),
                Action::Foreign => advisors.open(Screen::Foreign),
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clause_goes_on_and_off_the_table() {
        let mut deal = Deal::new(0, 1);
        toggle_clause(&mut deal, true, Clause::Tech(3));
        toggle_clause(&mut deal, false, Clause::Peace);
        assert_eq!((deal.from_a.clone(), deal.from_b.clone()), (vec![Clause::Tech(3)], vec![Clause::Peace]));
        toggle_clause(&mut deal, true, Clause::Tech(3));
        assert!(deal.from_a.is_empty());
    }

    #[test]
    fn gold_moves_in_steps_within_the_purse_and_vanishes_at_zero() {
        let mut deal = Deal::new(0, 1);
        move_gold(&mut deal, true, 10, false, 25);
        move_gold(&mut deal, true, 10, false, 25);
        move_gold(&mut deal, true, 10, false, 25);
        assert_eq!(deal.from_a, vec![Clause::Gold(25)], "the purse is the limit");
        move_gold(&mut deal, true, 1, true, 25);
        assert_eq!(deal.from_a.len(), 2, "once and per turn are separate clauses");
        move_gold(&mut deal, true, -10, false, 25);
        move_gold(&mut deal, true, -10, false, 25);
        move_gold(&mut deal, true, -10, false, 25);
        assert_eq!(deal.from_a, vec![Clause::GoldPerTurn(1)]);
    }

    #[test]
    fn turns_are_worded_for_the_panel() {
        assert_eq!(turns_text(1), "1 turn");
        assert_eq!(turns_text(12), "12 turns");
        assert_eq!(turns_text(9999), "never");
    }

    #[test]
    fn an_open_screen_blocks_and_a_closed_one_does_not() {
        let mut a = Advisors::default();
        assert!(!a.is_open());
        a.open(Screen::Foreign);
        assert!(a.is_open());
        a.talk_with(0, 2);
        assert_eq!((a.screen, a.deal.a, a.deal.b), (Screen::Talk(2), 0, 2));
        a.close();
        assert!(!a.is_open());
    }

    /// An app with the resources the advisors read; Japan is the human.
    fn app() -> App {
        crate::civs::set_controllers();
        let mut app = App::new();
        app.insert_resource(GameMap::generate());
        app.init_resource::<Civilizations>();
        app.init_resource::<ProductionPrompts>();
        app.init_resource::<CityView>();
        app.init_resource::<Advisors>();
        app.init_resource::<Treasury>();
        app.init_resource::<MessageBoard>();
        app.insert_resource(Turn(1));
        app.insert_resource(Diplomacy::new());
        app.insert_resource(CombatRng(crate::rng::MapRng::new(5)));
        let mut research = Research::new();
        research.begin(&mut crate::rng::MapRng::new(5));
        app.insert_resource(research);
        app
    }

    #[test]
    fn a_research_target_owed_opens_the_science_advisor_and_a_pick_closes_it() {
        let mut app = app();
        app.add_systems(Update, (interrupt, respond).chain());
        app.update();
        assert_eq!(app.world().resource::<Advisors>().screen, Screen::Science);
        let t = app.world().resource::<Research>().options(0)[0];
        app.world_mut().spawn((Interaction::Pressed, Action::Pick(t)));
        app.update();
        assert_eq!(app.world().resource::<Advisors>().screen, Screen::Closed);
        assert_eq!(app.world().resource::<Research>().target(0), Some(t));
        assert!(!app.world().resource::<Research>().needs_choice(0));
    }

    #[test]
    fn a_strike_at_a_civ_at_peace_asks_and_yes_declares_war_and_renews_the_order() {
        let mut app = app();
        app.add_systems(Update, (interrupt, respond).chain());
        // The research choice is not what this is about.
        let t = app.world().resource::<Research>().options(0)[0];
        app.world_mut().resource_scope(|w, mut r: Mut<Research>| {
            r.pick(0, t, &mut w.resource_mut::<CombatRng>().0);
        });
        let strike = app.world_mut().spawn(Unit::new(0, crate::units::UnitType::Horseman, 3, 3)).id();
        app.world_mut().resource_mut::<Diplomacy>().war_ask =
            Some(crate::diplomacy::WarAsk { attacker: strike, to: (4, 3), target: 1 });
        app.update();
        assert_eq!(app.world().resource::<Advisors>().screen, Screen::WarAsk);
        app.world_mut().spawn((Interaction::Pressed, Action::WarYes));
        app.update();
        let d = app.world().resource::<Diplomacy>();
        assert!(d.at_war(0, 1));
        assert!(d.war_ask.is_none());
        assert_eq!(app.world().get::<Unit>(strike).unwrap().path, [(4, 3)]);
        assert_eq!(app.world().resource::<Advisors>().screen, Screen::Closed);
    }

    #[test]
    fn declining_the_strike_leaves_the_peace() {
        let mut app = app();
        app.add_systems(Update, (interrupt, respond).chain());
        let strike = app.world_mut().spawn(Unit::new(0, crate::units::UnitType::Horseman, 3, 3)).id();
        app.world_mut().resource_mut::<Diplomacy>().war_ask =
            Some(crate::diplomacy::WarAsk { attacker: strike, to: (4, 3), target: 1 });
        app.world_mut().spawn((Interaction::Pressed, Action::WarNo));
        app.update();
        assert!(!app.world().resource::<Diplomacy>().at_war(0, 1));
        assert!(app.world().resource::<Diplomacy>().war_ask.is_none());
    }

    #[test]
    fn a_proposal_for_the_human_is_shown_and_accepting_trades() {
        let mut app = app();
        app.add_systems(Update, (interrupt, respond).chain());
        let t = app.world().resource::<Research>().options(0)[0];
        app.world_mut().resource_scope(|w, mut r: Mut<Research>| {
            r.pick(0, t, &mut w.resource_mut::<CombatRng>().0);
        });
        // Rome gives Japan an advance for nothing.
        let t = app.world().resource::<Research>().giftable(1, 0)[0];
        let mut d = Deal::new(0, 1);
        d.from_b.push(Clause::Tech(t));
        let mut diplomacy = app.world_mut().resource_mut::<Diplomacy>();
        diplomacy.meet(0, 1);
        diplomacy.proposals.push_back(crate::diplomacy::Proposal { from: 1, to: 0, deal: d });
        app.update();
        assert_eq!(app.world().resource::<Advisors>().screen, Screen::Proposal);
        app.world_mut().spawn((Interaction::Pressed, Action::Accept));
        app.update();
        assert!(app.world().resource::<Research>().knows(0, t));
        assert!(app.world().resource::<Diplomacy>().proposals.is_empty());
    }
}
