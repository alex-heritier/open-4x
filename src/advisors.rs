//! The advisors: the Science Advisor (what to research), the Foreign Advisor
//! (who we have met and where we stand), the diplomacy screens (a leader
//! greets us, we talk and trade, the computer proposes), the wonder screens
//! (the splash and the Wonders of the World window), and the questions that
//! interrupt a turn: a proposal from the computer, and "declare war?" when a
//! soldier is ordered against a civ at peace.
//!
//! One modal is up at a time. `Advisors::screen` says which; the panel is
//! rebuilt whenever the resource changes, so a button only changes state.
//!
//! The diplomacy screens are Civ3's own art (`talk_offer`, `counter`) with
//! the other leader's animation in its frame and the words of
//! `diplomacy.txt` (`speech.rs`); everything on them is placed in the art's
//! 1024 x 768 pixels (`stage.rs`).
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use civ3mapgen::diplomacy::Clause;

use crate::cities::{City, CityView, Treasury};
use crate::civs::{CIV_COUNT, Civilizations, is_ai};
use crate::combat::CombatRng;
use crate::diplomacy::{Deal, Diplomacy, attitude_label, clause_text, people, verdict_text};
use crate::features::{MessageBoard, post};
use crate::leaders::{self, LEADERS, LeaderArt};
use crate::map::GameMap;
use crate::production_prompt::ProductionPrompts;
use crate::research::{Research, announce, civ_of, tech_name};
use crate::speech::{Speech, Who, mood_tone, power_tone};
use crate::stage::{self, Stage, Ui};
use crate::units::{Turn, Unit};
use crate::wonders::{self, Wonders};
use civ3mapgen::diplomacy::Verdict;

/// Gold moved by one click of the plus and minus buttons.
const GOLD_STEP: i32 = 10;
/// Advances listed on each side of the talk screen.
const TECHS_SHOWN: usize = 8;
/// Rows of treaty buttons in the lower box of the trading screen.
const TREATY_ROWS: usize = 4;

/// Which modal is up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Screen {
    #[default]
    Closed,
    /// "Our Sages need direction": the Science Advisor's popup that asks
    /// for the next advance; the big picture behind it is `Science`.
    ResearchAsk,
    Science,
    Foreign,
    /// A leader greets us: first contact with this civ.
    Greeting(usize),
    /// Talking with this civ.
    Talk(usize),
    /// The computer's proposal at the front of the queue.
    Proposal,
    /// "Declare war?" for the strike in `Diplomacy::war_ask`.
    WarAsk,
    /// "Install a new governor?" for the city in `Diplomacy::convert_ask`.
    Convert,
    /// "Select transport" for the boarding in `Diplomacy::board_ask`.
    Transport,
    /// The Wonders of the World window (F7).
    Wonders,
    /// The wonder splash at the front of `Wonders::splash`.
    Splash,
}

#[derive(Resource, Default)]
pub struct Advisors {
    pub screen: Screen,
    /// The deal on the table of the talk screen: the human gives `from_a`.
    deal: Deal,
    /// What the computer's leader answered to the last proposal; empty until
    /// one was made, and the leader greets instead.
    said: String,
    /// What the game itself says on the talk screen: why a deal cannot be
    /// made, that war needs a second click.
    note: String,
    /// Declaring war from the talk screen takes two clicks.
    armed: bool,
    /// The leaders who have greeted us.
    greeted: [bool; CIV_COUNT],
    /// Picks among the phrasings of a line; moves on at every screen, so a
    /// screen that is rebuilt keeps its words and the next one has others.
    seed: u32,
    /// The page of the Wonders window.
    page: usize,
    /// The era page of the Science Advisor; the viewer's era until turned.
    era: Option<usize>,
    /// The advance shown in the research popup's pull-down; the advisor's
    /// suggestion until another is chosen.
    ask: Option<i32>,
    /// The research popup's pull-down is open.
    pulldown: bool,
}

impl Advisors {
    /// A modal is up: orders and the end of the turn wait.
    pub fn is_open(&self) -> bool {
        self.screen != Screen::Closed
    }

    fn open(&mut self, screen: Screen) {
        *self = Advisors { screen, greeted: self.greeted, seed: self.seed.wrapping_add(1), ..Default::default() };
    }

    /// The talk screen with an empty table between `me` and `other`.
    fn talk_with(&mut self, me: usize, other: usize) {
        *self = Advisors {
            screen: Screen::Talk(other),
            deal: Deal::new(me, other),
            greeted: self.greeted,
            seed: self.seed.wrapping_add(1),
            ..Default::default()
        };
    }

    fn close(&mut self) {
        self.open(Screen::Closed);
    }

    /// Open `screen` from another advisor's tab.
    pub fn show(&mut self, screen: Screen) {
        self.open(screen);
    }

    /// Put away what is up, for another advisor's tab.
    pub fn dismiss(&mut self) {
        if self.is_open() {
            self.close();
        }
    }
}

/// The root of the screen up, with the stage scale it was built at: a window
/// resize rebuilds it.
#[derive(Component)]
pub struct AdvisorRoot(f32);

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
    /// A city's culture flip toward us: take it, or rebuff the rebels.
    ConvertYes,
    ConvertNo,
    /// A ship picked from the transport list.
    Transport(Entity),
    /// The Wonders window (F7).
    Wonders,
    /// The page of the Wonders window, a step back or forward.
    Page(i32),
    /// Look at the city on this tile.
    Zoom(i32, i32),
    /// Turn the Science Advisor's era page a step back or forward.
    Era(i32),
    /// The research popup: open or shut its pull-down, put an advance in
    /// it, research the advance in it ("OK."), or see the tree ("What's
    /// the big picture?").
    Pulldown,
    Choose(i32),
    Ok,
    BigPicture,
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
            Action::ConvertYes => "ConvertYes".into(),
            Action::ConvertNo => "ConvertNo".into(),
            Action::Transport(e) => format!("Transport:{}", e.index()),
            Action::Wonders => "Wonders".into(),
            Action::Page(d) => format!("Page:{d}"),
            Action::Zoom(..) => "Zoom".into(),
            Action::Era(d) => format!("Era:{d}"),
            Action::Pulldown => "Pulldown".into(),
            Action::Choose(t) => format!("Choose:{}", tech_name(*t)),
            Action::Ok => "Ok".into(),
            Action::BigPicture => "BigPicture".into(),
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

/// F6, F4 and F7 open the advisors and the Wonders window; Escape closes one
/// that is not a question, and any of Escape, Enter and Space ends a splash.
pub fn hotkeys(
    mut keys: ResMut<ButtonInput<KeyCode>>,
    civs: Res<Civilizations>,
    prompts: Res<ProductionPrompts>,
    view: Res<CityView>,
    mut research: ResMut<Research>,
    mut rng: ResMut<CombatRng>,
    mut advisors: ResMut<Advisors>,
    mut wonders: ResMut<Wonders>,
) {
    let Some(me) = human(&civs) else { return };
    if advisors.is_open() {
        let question = matches!(
            advisors.screen,
            Screen::ResearchAsk | Screen::Proposal | Screen::WarAsk | Screen::Convert | Screen::Transport
        );
        if advisors.screen == Screen::ResearchAsk {
            if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]) {
                if let Some(t) = asked(&advisors, &research, me) {
                    research.pick(me, t, &mut rng.0);
                    advisors.close();
                }
                // The key answered the popup; it must not end the turn too.
                consume(&mut keys, &[KeyCode::Enter, KeyCode::NumpadEnter]);
            }
        } else if advisors.screen == Screen::Splash {
            let ends = [KeyCode::Escape, KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space];
            if keys.any_just_pressed(ends) {
                wonders.dismiss(me);
                advisors.close();
                consume(&mut keys, &ends);
            }
        } else if (keys.just_pressed(KeyCode::Escape) && !question)
            || (keys.just_pressed(KeyCode::F7) && advisors.screen == Screen::Wonders)
        {
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
    } else if keys.just_pressed(KeyCode::F7) {
        advisors.open(Screen::Wonders);
    }
}

/// `me` has a city from an earlier turn: the Sages have something to study
/// with.
fn settled(cities: &Query<&City>, me: usize, turn: u32) -> bool {
    cities.iter().any(|c| c.civ == me && c.founded < turn)
}

/// Spend a key press on a modal, so the map's orders read later in the
/// frame (Enter ends the turn, Space skips) never see it.
fn consume(keys: &mut ButtonInput<KeyCode>, codes: &[KeyCode]) {
    for &k in codes {
        keys.clear_just_pressed(k);
    }
}

/// Things that stop the turn: a strike against a civ at peace, a wonder of
/// ours to look at, a research target to pick, a proposal to answer, a leader
/// we have just met.
pub fn interrupt(
    civs: Res<Civilizations>,
    prompts: Res<ProductionPrompts>,
    view: Res<CityView>,
    research: Res<Research>,
    diplomacy: Res<Diplomacy>,
    wonders: Res<Wonders>,
    domestic: Res<crate::domestic::Domestic>,
    cities: Query<&City>,
    turn: Res<Turn>,
    mut advisors: ResMut<Advisors>,
) {
    let Some(me) = human(&civs) else { return };
    if diplomacy.war_ask.is_some() {
        if advisors.screen != Screen::WarAsk {
            advisors.open(Screen::WarAsk);
        }
        return;
    }
    if diplomacy.convert_ask.is_some_and(|a| a.answer.is_none() && a.to == me)
        && !advisors.is_open()
    {
        advisors.open(Screen::Convert);
    }
    if diplomacy.board_ask.as_ref().is_some_and(|a| a.answer.is_none()) && !advisors.is_open() {
        advisors.open(Screen::Transport);
    }
    if advisors.is_open() || domestic.is_open() || prompts.blocks(civs.active) || view.0.is_some() {
        return;
    }
    if wonders.next_for(me).is_some() {
        advisors.open(Screen::Splash);
    } else if research.needs_choice(me) && !research.options(me).is_empty() && settled(&cities, me, turn.0) {
        // Civ3 asks in the advisor's popup, from the turn after the first
        // city is founded; the tree is a click away.
        advisors.open(Screen::ResearchAsk);
    } else if diplomacy.proposals.iter().any(|p| p.to == me) {
        advisors.open(Screen::Proposal);
    } else if let Some(other) = (0..CIV_COUNT).find(|&o| o != me && diplomacy.contact(me, o) && !advisors.greeted[o]) {
        advisors.greeted[other] = true;
        advisors.open(Screen::Greeting(other));
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

fn row(parent: &mut ChildSpawnerCommands, f: impl FnOnce(&mut ChildSpawnerCommands)) {
    parent
        .spawn(Node { column_gap: Val::Px(10.0), align_items: AlignItems::Center, ..default() })
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
    speech: &'a Speech,
    assets: &'a AssetServer,
    /// The real-time clock, for the frame a portrait starts on.
    now_ms: u128,
}

impl View<'_> {
    /// What the leader of `other` says under `key` to the human: in the tone
    /// of their relative strength and of their attitude.
    fn says(&self, other: usize, key: &str, who: &Who) -> Option<String> {
        says(self.speech, self.diplomacy, self.facts, self.me, other, key, self.advisors.seed as usize, who)
    }
}

/// The line of `key` that the leader of `other` speaks to `me`.
#[allow(clippy::too_many_arguments)]
fn says(speech: &Speech, d: &Diplomacy, facts: &crate::diplomacy::Facts, me: usize, other: usize, key: &str, roll: usize, who: &Who) -> Option<String> {
    let power = power_tone(facts.score[other], facts.score[me]);
    let mood = mood_tone(d.attitude(facts, other, me));
    speech.say(key, power, mood, roll, who)
}

pub fn turns_text(turns: i32) -> String {
    match turns {
        9999.. => "never".into(),
        1 => "1 turn".into(),
        n => format!("{n} turns"),
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
    // Room for the boxed X in the corner.
    p.spawn(Node { height: Val::Px(40.0), ..default() });
}

// ---------------------------------------------------------------------
// The research popup
// ---------------------------------------------------------------------

/// The advance in the research popup's pull-down: the one chosen there,
/// else the advisor's suggestion, the cheapest open advance.
fn asked(advisors: &Advisors, research: &Research, me: usize) -> Option<i32> {
    let options = research.options(me);
    advisors.ask.filter(|t| options.contains(t)).or_else(|| options.first().copied())
}

/// The popup's size, and where it sits below the top of the window, read
/// off Conquests' screen; everything inside is placed from its corner.
const ASK: (f32, f32, f32) = (370.0, 230.0, 110.0);
/// The pull-down: corner and size.
const PULLDOWN: (f32, f32, f32, f32) = (63.0, 98.0, 234.0, 23.0);
const ASK_LINK: Color = Color::srgb(0.1, 0.1, 0.85);
const ASK_FOCUS: Color = Color::srgb(0.25, 0.55, 0.8);

/// "Great One, our Sages need direction." Civ3's small advisor popup at the
/// top of the map, the Science Advisor's head over its corner: an advance
/// to pick from a pull-down, "OK." to research it, and "What's the big
/// picture?" for the tree.
fn research_ask(commands: &mut Commands, ui: &Ui, v: &View, scale: f32) {
    let st = ui.st;
    let (w, h, top) = ASK;
    let options = v.research.options(v.me);
    let chosen = asked(v.advisors, v.research, v.me);
    let label = |t: i32| format!("{} ({})", tech_name(t), turns_text(v.research.turns(v.me, t)));
    let slices = TextureSlicer { border: BorderRect::all(6.0), ..default() };
    let parchment = |assets: &AssetServer| {
        ImageNode::new(assets.load("gen/advisors/popup.png")).with_mode(NodeImageMode::Sliced(slices.clone()))
    };
    commands
        .spawn((
            AdvisorRoot(scale),
            GlobalZIndex(100),
            Node { width: Val::Percent(100.0), height: Val::Percent(100.0), justify_content: JustifyContent::Center, ..default() },
        ))
        .with_children(|root| {
            root.spawn(Node { width: st.px(w), height: st.px(h), margin: UiRect::top(st.px(top)), ..default() })
                .with_children(|s| {
                    // The head rises over the popup, which hides the shoulders.
                    let era = crate::tech_tree::era_of(v.research, v.me);
                    ui.picture(s, ImageNode::new(v.assets.load(format!("gen/advisors/portrait_science_{era}.png"))), 221.0, -132.0, 150.0, 150.0);
                    s.spawn((parchment(v.assets), st.rect(0.0, 0.0, w, h)));
                    ui.words(s, 0.0, 10.0, w, 30.0, "Science Advisor", 24.0, Color::BLACK, true);
                    ui.words(s, 27.0, 59.0, w - 40.0, 44.0, "Great One, our Sages need direction.\nShall we look into the secrets of", 16.0, Color::BLACK, false);
                    let (px, py, pw, ph) = PULLDOWN;
                    s.spawn((
                        Button,
                        Action::Pulldown,
                        Node {
                            border: UiRect::all(Val::Px(1.0)),
                            align_items: AlignItems::Center,
                            padding: UiRect::left(st.px(4.0)),
                            overflow: Overflow::clip(),
                            ..st.rect(px, py, pw, ph)
                        },
                        BackgroundColor(Color::srgb(0.97, 0.95, 0.88)),
                        BorderColor::all(Color::srgb(0.35, 0.35, 0.35)),
                    ))
                    .with_children(|b| {
                        b.spawn((
                            Text::new(chosen.map(label).unwrap_or_default()),
                            TextFont { font: ui.font.clone(), font_size: st.font(15.0), ..default() },
                            TextColor(ASK_LINK),
                            TextLayout::new(Justify::Left, LineBreak::NoWrap),
                            Underline,
                        ));
                        b.spawn((ImageNode::new(v.assets.load("gen/advisors/pulldown.png")), st.rect(pw - 23.0, 0.0, 21.0, 21.0)));
                    });
                    ui.words(s, px + pw + 3.0, py, 12.0, ph, "?", 16.0, Color::BLACK, false);
                    for (k, (text, action, focus)) in
                        [("OK.", Action::Ok, true), ("What's the big picture?", Action::BigPicture, false)].into_iter().enumerate()
                    {
                        let y = 128.0 + 27.0 * k as f32;
                        s.spawn((Button, action, Node { column_gap: st.px(3.0), align_items: AlignItems::Center, ..st.rect(27.0, y, 300.0, 22.0) }))
                            .with_children(|b| {
                                let bullet = if focus { "bullet_2" } else { "bullet_0" };
                                b.spawn((
                                    ImageNode::new(v.assets.load(format!("gen/advisors/{bullet}.png"))),
                                    Node { width: st.px(19.0), height: st.px(20.0), ..default() },
                                ));
                                b.spawn((
                                    Text::new(text),
                                    TextFont { font: ui.font.clone(), font_size: st.font(16.0), ..default() },
                                    TextColor(if focus { ASK_FOCUS } else { Color::BLACK }),
                                ));
                            });
                    }
                    if v.advisors.pulldown {
                        let row = 20.0;
                        s.spawn((
                            parchment(v.assets),
                            GlobalZIndex(101),
                            Node { flex_direction: FlexDirection::Column, padding: UiRect::all(st.px(4.0)), ..st.rect(px, py + ph, pw, row * options.len() as f32 + 8.0) },
                        ))
                        .with_children(|list| {
                            for &t in &options {
                                list.spawn((
                                    Button,
                                    Action::Choose(t),
                                    Node { height: st.px(row), align_items: AlignItems::Center, padding: UiRect::left(st.px(4.0)), ..default() },
                                    BackgroundColor(if Some(t) == chosen { SELECTED } else { Color::NONE }),
                                ))
                                .with_child((
                                    Text::new(label(t)),
                                    TextFont { font: ui.font.clone(), font_size: st.font(15.0), ..default() },
                                    TextColor(ASK_LINK),
                                    TextLayout::new(Justify::Left, LineBreak::NoWrap),
                                ));
                            }
                        });
                    }
                });
        });
}

// ---------------------------------------------------------------------
// The staged screens
// ---------------------------------------------------------------------

/// The hole of the diplomacy frames that the leader shows through.
const PORTRAIT: (f32, f32, f32, f32) = (411.0, 59.0, 200.0, 240.0);

/// The animated leader of `other` behind the frame.
fn portrait(s: &mut ChildSpawnerCommands, ui: &Ui, v: &View, art: &mut LeaderArt, other: usize) {
    if let Some((head, image)) = leaders::head(art, v.assets, v.research, other, v.now_ms) {
        s.spawn((ui.st.rect(PORTRAIT.0, PORTRAIT.1, PORTRAIT.2, PORTRAIT.3), image, head));
    }
}

fn frame(s: &mut ChildSpawnerCommands, ui: &Ui, v: &View, name: &str) {
    ui.picture(s, ImageNode::new(v.assets.load(format!("gen/diplomacy/{name}.png"))), 0.0, 0.0, 1024.0, 768.0);
}

/// "Emperor Caesar of the Romans".
fn leader_title(other: usize) -> String {
    format!("{} {} of the {}", LEADERS[other].title, LEADERS[other].name, people(other))
}

/// The leader's greeting when we sit down: by the state of the world.
fn hello(v: &View, other: usize) -> String {
    let d = v.diplomacy;
    let key = if d.at_war(v.me, other) {
        "AIWARGREETINGS"
    } else if d.treaties_text(v.me, other).contains("alliance") {
        "AIALLIANCEGREETINGS"
    } else {
        "AIPEACEGREETINGS"
    };
    v.says(other, key, &Who::between(other, v.me)).unwrap_or_else(|| format!("Greetings. I am {}.", LEADERS[other].name))
}

/// A leader we have just met, in the small frame.
fn greeting(s: &mut ChildSpawnerCommands, ui: &Ui, v: &View, art: &mut LeaderArt, other: usize) {
    portrait(s, ui, v, art, other);
    frame(s, ui, v, "talk_offer");
    let line = v
        .says(other, "AIFIRSTCONTACT", &Who::between(other, v.me))
        .unwrap_or_else(|| format!("Greetings. We are the {}, and I am {}.", people(other), LEADERS[other].name));
    ui.words(s, 308.0, 335.0, 416.0, 20.0, leader_title(other), 15.0, stage::LABEL, true);
    ui.words(s, 316.0, 372.0, 400.0, 130.0, line, 17.0, stage::INK, true);
    ui.button(s, 322.0, 520.0, 180.0, 28.0, "Let us talk", 16.0, Action::Talk(other), false);
    ui.button(s, 522.0, 520.0, 180.0, 28.0, "Farewell", 16.0, Action::Close, false);
}

/// "Peace treaty" and the like, short enough for a button of the table.
fn treaty_label(c: &Clause) -> String {
    match c {
        Clause::MilitaryAlliance(x) => format!("Alliance vs {}", people(civ_of(*x))),
        Clause::Embargo(x) => format!("Embargo on {}", people(civ_of(*x))),
        other => clause_text(other),
    }
}

/// One side of the table: what `giver` puts on it, in the panel of the frame.
fn side(s: &mut ChildSpawnerCommands, ui: &Ui, v: &View, other: usize, give: bool, x: f32) {
    let deal = &v.advisors.deal;
    let (giver, taker) = if give { (v.me, other) } else { (other, v.me) };
    let from = if give { &deal.from_a } else { &deal.from_b };
    ui.words(s, x, 231.0, 183.0, 22.0, if give { "We give" } else { "They give" }, 19.0, stage::LABEL, true);
    let techs = v.research.giftable(giver, taker);
    for (i, &t) in techs.iter().take(TECHS_SHOWN).enumerate() {
        let on = from.contains(&Clause::Tech(t));
        let mark = if on { "* " } else { "" };
        ui.button(s, x, 258.0 + 23.0 * i as f32, 183.0, 21.0, format!("{mark}{}", tech_name(t)), 13.0, Action::Tech(give, t), on);
    }
    if techs.len() > TECHS_SHOWN {
        ui.words(s, x, 258.0 + 23.0 * TECHS_SHOWN as f32, 183.0, 16.0, format!("...and {} more.", techs.len() - TECHS_SHOWN), 12.0, stage::INK, true);
    }
    if techs.is_empty() {
        ui.words(s, x, 262.0, 183.0, 18.0, "No advances to trade.", 13.0, stage::INK, true);
    }
    let amount = |per_turn: bool| {
        from.iter()
            .find_map(|c| match (c, per_turn) {
                (Clause::Gold(n), false) | (Clause::GoldPerTurn(n), true) => Some(*n),
                _ => None,
            })
            .unwrap_or(0)
    };
    let (once, per) = (amount(false), amount(true));
    let stepper = |s: &mut ChildSpawnerCommands, y: f32, label: String, minus: Action, plus: Action| {
        ui.button(s, x, y, 28.0, 24.0, "-", 16.0, minus, false);
        ui.words(s, x + 30.0, y + 5.0, 123.0, 18.0, label, 13.0, stage::INK, true);
        ui.button(s, x + 155.0, y, 28.0, 24.0, "+", 16.0, plus, false);
    };
    stepper(s, 468.0, format!("{once} of {} gold", v.treasury.0[giver]), Action::Gold(give, -GOLD_STEP), Action::Gold(give, GOLD_STEP));
    stepper(s, 500.0, format!("{per} gold a turn"), Action::GoldPerTurn(give, -1), Action::GoldPerTurn(give, 1));
}

/// The trading screen: the leader above, our offer left, theirs right, the
/// treaties in the lower box and the leader's last words in the upper.
fn talk(s: &mut ChildSpawnerCommands, ui: &Ui, v: &View, art: &mut LeaderArt, other: usize) {
    let (me, d) = (v.me, v.diplomacy);
    portrait(s, ui, v, art, other);
    frame(s, ui, v, "counter");
    // The upper box: who speaks, and what they say.
    let spoken = if v.advisors.said.is_empty() { hello(v, other) } else { v.advisors.said.clone() };
    ui.words(s, 306.0, 330.0, 420.0, 18.0, leader_title(other), 14.0, stage::LABEL, true);
    ui.words(s, 306.0, 350.0, 420.0, 72.0, spoken, 15.0, stage::INK, true);
    side(s, ui, v, other, true, 49.0);
    side(s, ui, v, other, false, 798.0);
    // The bar: where we stand, what the game itself has to say.
    ui.words(s, 320.0, 446.0, 394.0, 34.0, standing(v, other), 14.0, stage::INK, true);
    if !v.advisors.note.is_empty() {
        ui.words(s, 320.0, 482.0, 394.0, 20.0, v.advisors.note.clone(), 14.0, stage::WARN, true);
    }
    if !d.at_war(me, other) {
        let label = if v.advisors.armed { "Really declare war?" } else { "Declare war" };
        ui.button(s, 322.0, 506.0, 190.0, 26.0, label, 15.0, Action::DeclareWar, v.advisors.armed);
    }
    ui.button(s, 522.0, 506.0, 190.0, 26.0, "Foreign Advisor", 15.0, Action::Foreign, false);
    // The lower box: treaties are offered when the clause alone would stand.
    let mut options = vec![Clause::Peace, Clause::RightOfPassage, Clause::MutualProtection];
    for x in (0..CIV_COUNT).filter(|&x| x != me && x != other) {
        options.push(Clause::MilitaryAlliance(crate::research::slot(x)));
        options.push(Clause::Embargo(crate::research::slot(x)));
    }
    let treaties: Vec<Clause> = options
        .into_iter()
        .filter(|c| {
            let mut alone = Deal::new(me, other);
            alone.from_a.push(c.clone());
            d.check(v.research, &v.treasury.0, &alone).is_ok()
        })
        .collect();
    ui.words(s, 306.0, 562.0, 420.0, 20.0, "Treaties", 16.0, stage::LABEL, true);
    if treaties.is_empty() {
        ui.words(s, 306.0, 620.0, 420.0, 20.0, "No treaty is open to us.", 14.0, stage::INK, true);
    }
    for (i, c) in treaties.into_iter().take(2 * TREATY_ROWS).enumerate() {
        let on = v.advisors.deal.from_a.contains(&c);
        let mark = if on { "* " } else { "" };
        let (x, y) = (308.0 + 213.0 * (i % 2) as f32, 586.0 + 28.0 * (i / 2) as f32);
        ui.button(s, x, y, 205.0, 24.0, format!("{mark}{}", treaty_label(&c)), 13.0, Action::Treaty(c), on);
    }
    ui.button(s, 354.0, 727.0, 150.0, 24.0, "Propose", 16.0, Action::Propose, false);
    ui.button(s, 514.0, 727.0, 150.0, 24.0, "Leave", 16.0, Action::Close, false);
}

fn describe(clauses: &[Clause]) -> String {
    if clauses.is_empty() {
        return "nothing".into();
    }
    clauses.iter().map(clause_text).collect::<Vec<_>>().join(", ")
}

/// The block of `diplomacy.txt` that makes a leader's case for `deal`, and
/// the third civ it names.
fn proposal_key(deal: &Deal) -> (&'static str, Option<usize>) {
    let mut best: Option<(u8, &'static str, Option<usize>)> = None;
    for c in deal.from_a.iter().chain(&deal.from_b) {
        let pick = match c {
            Clause::Peace => (0, "AIPEACETREATY", None),
            Clause::MilitaryAlliance(x) => (1, "AIMILITARYALLIANCE", Some(civ_of(*x))),
            Clause::MutualProtection => (2, "AIMUTUALPROTECTION", None),
            Clause::RightOfPassage => (3, "AIRIGHTOFPASSAGE", None),
            Clause::Embargo(x) => (4, "AITRADEEMBARGO", Some(civ_of(*x))),
            Clause::WorldMap => (5, "AIMAPTRADE", None),
            Clause::Contact(x) => (6, "AICOMMUNICATIONS", Some(civ_of(*x))),
            Clause::Tech(_) => (7, "AITECHTRADE", None),
            _ => (8, "AIFIRSTDEAL", None),
        };
        if best.is_none_or(|b| pick.0 < b.0) {
            best = Some(pick);
        }
    }
    best.map_or(("AIFIRSTDEAL", None), |(_, key, third)| (key, third))
}

/// The computer's proposal: the leader makes the case, the terms below it.
fn proposal(s: &mut ChildSpawnerCommands, ui: &Ui, v: &View, art: &mut LeaderArt) {
    let Some(offer) = v.diplomacy.proposals.iter().find(|q| q.to == v.me) else { return };
    let other = offer.from;
    portrait(s, ui, v, art, other);
    frame(s, ui, v, "talk_offer");
    let (give, get) = (describe(&offer.deal.from_b), describe(&offer.deal.from_a));
    let (key, third) = proposal_key(&offer.deal);
    let who = Who { third, give: &give, get: &get, ..Who::between(other, v.me) };
    let case = v
        .says(other, key, &who)
        .unwrap_or_else(|| format!("The {} propose a trade.", people(other)));
    ui.words(s, 308.0, 335.0, 416.0, 20.0, leader_title(other), 15.0, stage::LABEL, true);
    ui.words(s, 316.0, 362.0, 400.0, 100.0, case, 16.0, stage::INK, true);
    ui.words(s, 316.0, 454.0, 400.0, 36.0, format!("They give: {give}."), 15.0, stage::INK, true);
    ui.words(s, 316.0, 490.0, 400.0, 36.0, format!("We give: {get}."), 15.0, stage::INK, true);
    ui.button(s, 322.0, 522.0, 180.0, 28.0, "Accept", 16.0, Action::Accept, false);
    ui.button(s, 522.0, 522.0, 180.0, 28.0, "Decline", 16.0, Action::Decline, false);
}

fn convert_ask(p: &mut ChildSpawnerCommands, font: &Handle<Font>, v: &View, cities: &Query<&City>) {
    let Some(ask) = v.diplomacy.convert_ask else { return };
    let name = cities.get(ask.city).map_or("The city".to_string(), |c| c.name.clone());
    text(p, font, format!("{name} wishes to join us!"), 30.0);
    text(
        p,
        font,
        format!("The people of {name} want to switch their allegiance to our civilization. Shall we install a new governor?"),
        20.0,
    );
    row(p, |r| {
        button(r, font, "Great! Install a new governor.".into(), Action::ConvertYes, false);
        button(r, font, format!("We don't want {name}. Rebuff the rebels."), Action::ConvertNo, false);
    });
}

fn transport_ask(p: &mut ChildSpawnerCommands, font: &Handle<Font>, v: &View) {
    let Some(ask) = v.diplomacy.board_ask.as_ref() else { return };
    text(p, font, "Select transport", 30.0);
    text(p, font, "Several ships here could take the unit aboard. Which one?", 20.0);
    for (ship, line) in &ask.options {
        button(p, font, line.clone(), Action::Transport(*ship), false);
    }
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

/// Rebuild the panel when the screen changes, or the window is resized.
#[allow(clippy::too_many_arguments)]
pub fn show(
    mut commands: Commands,
    advisors: Res<Advisors>,
    roots: Query<(Entity, &AdvisorRoot)>,
    assets: Res<AssetServer>,
    research: Res<Research>,
    diplomacy: Res<Diplomacy>,
    treasury: Res<Treasury>,
    civs: Res<Civilizations>,
    map: Res<GameMap>,
    cities: Query<&City>,
    units: Query<&Unit>,
    speech: Res<Speech>,
    wonders: Res<Wonders>,
    mut art: ResMut<LeaderArt>,
    time: Res<Time<Real>>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let scale = stage::scale_of(&windows);
    let resized = roots.iter().any(|(_, r)| r.0 != scale);
    if !advisors.is_changed() && !resized {
        return;
    }
    for (root, _) in &roots {
        commands.entity(root).despawn();
    }
    if !advisors.is_open() {
        return;
    }
    let font = assets.load("gen/fonts/lsans.ttf");
    let cs: Vec<&City> = cities.iter().collect();
    let us: Vec<&Unit> = units.iter().collect();
    let facts = diplomacy.facts(&map, &research, &cs, &us);
    let v = View {
        me: civs.viewer(),
        research: &research,
        diplomacy: &diplomacy,
        treasury: &treasury,
        advisors: &advisors,
        facts: &facts,
        speech: &speech,
        assets: &assets,
        now_ms: time.elapsed().as_millis(),
    };
    let st = Stage(scale);
    let ui = Ui { font: &font, st };
    if advisors.screen == Screen::ResearchAsk {
        research_ask(&mut commands, &ui, &v, scale);
        return;
    }
    let staged = !matches!(advisors.screen, Screen::Foreign | Screen::WarAsk | Screen::Convert | Screen::Transport);
    if staged {
        // Civ3's advisors sit over the map undimmed.
        let dim = if advisors.screen == Screen::Science { 0.0 } else { 0.55 };
        let stage = stage::spawn(&mut commands, AdvisorRoot(scale), st, dim, 100);
        commands.entity(stage).with_children(|s| match advisors.screen {
            Screen::Greeting(other) => greeting(s, &ui, &v, &mut art, other),
            Screen::Talk(other) => talk(s, &ui, &v, &mut art, other),
            Screen::Proposal => proposal(s, &ui, &v, &mut art),
            Screen::Wonders => {
                let list = wonders::cards(&wonders, &cs, &diplomacy, v.me);
                wonders::window(s, &ui, &assets, &list, advisors.page);
            }
            Screen::Splash => {
                if let Some(splash) = wonders.next_for(v.me) {
                    wonders::splash(s, &ui, &assets, splash);
                }
            }
            Screen::Science => {
                let era = advisors.era.unwrap_or_else(|| crate::tech_tree::era_of(&research, v.me));
                crate::tech_tree::page(s, &ui, &assets, &research, v.me, era);
            }
            Screen::ResearchAsk | Screen::Foreign | Screen::WarAsk | Screen::Convert | Screen::Transport | Screen::Closed => {}
        });
        return;
    }
    commands
        .spawn((
            AdvisorRoot(scale),
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
                    width: Val::Px(520.0),
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
                Screen::Foreign => {
                    foreign(panel, &font, &v);
                    // The advisor's boxed X in the panel's corner.
                    let (_, _, w, h) = crate::advisor_frame::CLOSE_BOX;
                    panel.spawn((
                        Button,
                        crate::advisor_frame::ArtButton("exitbox"),
                        Action::Close,
                        crate::advisor_frame::art(&assets, "exitbox_0"),
                        Node {
                            position_type: PositionType::Absolute,
                            right: Val::Px(0.0),
                            bottom: Val::Px(0.0),
                            width: st.px(w),
                            height: st.px(h),
                            ..default()
                        },
                    ));
                }
                Screen::WarAsk => war_ask(panel, &font, &v),
                Screen::Convert => convert_ask(panel, &font, &v, &cities),
                Screen::Transport => transport_ask(panel, &font, &v),
                _ => {}
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
    speech: Res<Speech>,
    mut wonders: ResMut<Wonders>,
    mut camera: Query<&mut Transform, With<Camera2d>>,
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
        Action::Close => {
            if screen == Screen::Splash {
                wonders.dismiss(me);
            }
            advisors.close();
        }
        Action::Science => advisors.open(Screen::Science),
        Action::Foreign => advisors.open(Screen::Foreign),
        Action::Wonders => advisors.open(Screen::Wonders),
        Action::Era(step) => {
            let now = advisors.era.unwrap_or_else(|| crate::tech_tree::era_of(&research, me));
            advisors.era = Some((now as i32 + step).clamp(0, 3) as usize);
        }
        Action::Page(step) => {
            let cs: Vec<&City> = cities.iter().collect();
            let last = wonders::pages(wonders::cards(&wonders, &cs, &diplomacy, me).len()) - 1;
            advisors.page = (advisors.page as i32 + step).clamp(0, last as i32) as usize;
        }
        Action::Zoom(x, y) => {
            if screen == Screen::Splash {
                wonders.dismiss(me);
            }
            if let Ok(mut eye) = camera.single_mut() {
                let at = crate::map::tile_to_world(x, y);
                eye.translation.x = at.x;
                eye.translation.y = at.y;
            }
            advisors.close();
        }
        Action::Talk(other) => advisors.talk_with(me, other),
        Action::Pick(t) => {
            research.pick(me, t, &mut rng.0);
            advisors.close();
        }
        Action::Pulldown => advisors.pulldown = !advisors.pulldown,
        Action::Choose(t) => {
            advisors.ask = Some(t);
            advisors.pulldown = false;
        }
        Action::Ok => {
            if let Some(t) = asked(&advisors, &research, me) {
                research.pick(me, t, &mut rng.0);
            }
            advisors.close();
        }
        Action::BigPicture => advisors.open(Screen::Science),
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
                Err(why) => {
                    advisors.said.clear();
                    advisors.note = why.into();
                }
                Ok(()) => {
                    let verdict = diplomacy.weigh(&facts, &research, other, &deal, &mut rng.0);
                    // The leader answers in their own words; without the
                    // text the game says it.
                    let key = match verdict {
                        Verdict::Accept if deal.from_a.contains(&Clause::Peace) => "AIACCEPTPEACE",
                        Verdict::Accept => "AITHANKS",
                        Verdict::WeakReject => "AIWEAKREJECT",
                        Verdict::NeutralReject => "AINEUTRALREJECT",
                        Verdict::StrongReject => "AISTRONGREJECT",
                        Verdict::Invalid => "AITOTALREJECT",
                    };
                    advisors.seed = advisors.seed.wrapping_add(1);
                    let (give, get) = (describe(&deal.from_b), describe(&deal.from_a));
                    let who = Who { give: &give, get: &get, ..Who::between(other, me) };
                    let said = says(&speech, &diplomacy, &facts, me, other, key, advisors.seed as usize, &who);
                    advisors.note = if said.is_some() { String::new() } else { verdict_text(verdict).into() };
                    advisors.said = said.unwrap_or_default();
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
        Action::Transport(ship) => {
            if let Some(ask) = diplomacy.board_ask.as_mut() {
                ask.answer = Some(ship);
            }
            advisors.close();
        }
        Action::ConvertYes | Action::ConvertNo => {
            if let Some(ask) = diplomacy.convert_ask.as_mut() {
                ask.answer = Some(action == Action::ConvertYes);
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

/// Small buttons for the mouse: Science (F6), Foreign (F4) and Wonders (F7).
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
            for (label, action) in [("Science (F6)", Action::Science), ("Foreign (F4)", Action::Foreign), ("Wonders (F7)", Action::Wonders)] {
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
                Action::Wonders => advisors.open(Screen::Wonders),
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
        app.init_resource::<crate::domestic::Domestic>();
        app.init_resource::<Treasury>();
        app.init_resource::<MessageBoard>();
        app.insert_resource(Turn(1));
        app.insert_resource(Diplomacy::new());
        app.init_resource::<Wonders>();
        app.insert_resource(Speech::default());
        app.insert_resource(CombatRng(crate::rng::MapRng::new(5)));
        let mut research = Research::new();
        research.begin(&mut crate::rng::MapRng::new(5));
        app.insert_resource(research);
        app
    }

    /// Japan founded Kyoto last turn: the research popup is due.
    fn settle(app: &mut App) {
        let mut kyoto = City::new(0, "Kyoto", 10, 11);
        kyoto.founded = 0;
        app.world_mut().spawn(kyoto);
        // The continents the diplomacy facts place the city on.
        let land = crate::diplomacy::label_land(app.world().resource::<GameMap>());
        app.world_mut().resource_mut::<Diplomacy>().land = land;
    }

    #[test]
    fn a_research_target_owed_asks_in_the_popup_and_ok_researches_the_suggestion() {
        let mut app = app();
        settle(&mut app);
        app.add_systems(Update, (interrupt, respond).chain());
        app.update();
        assert_eq!(app.world().resource::<Advisors>().screen, Screen::ResearchAsk);
        let t = app.world().resource::<Research>().options(0)[0];
        press(&mut app, Action::Ok);
        assert_eq!(app.world().resource::<Advisors>().screen, Screen::Closed);
        assert_eq!(app.world().resource::<Research>().target(0), Some(t));
        assert!(!app.world().resource::<Research>().needs_choice(0));
    }

    #[test]
    fn the_popup_waits_for_the_turn_after_the_first_city() {
        let mut app = app();
        app.add_systems(Update, interrupt);
        app.update();
        assert_eq!(app.world().resource::<Advisors>().screen, Screen::Closed, "no city, nothing to study with");
        let kyoto = City::new(0, "Kyoto", 10, 11);
        app.world_mut().spawn(kyoto);
        app.update();
        assert_eq!(app.world().resource::<Advisors>().screen, Screen::Closed, "founded this turn");
        app.world_mut().resource_mut::<Turn>().0 = 2;
        app.update();
        assert_eq!(app.world().resource::<Advisors>().screen, Screen::ResearchAsk);
    }

    #[test]
    fn the_pulldown_changes_the_advance_ok_researches() {
        let mut app = app();
        settle(&mut app);
        app.add_systems(Update, (interrupt, respond).chain());
        app.update();
        let t = app.world().resource::<Research>().options(0)[1];
        press(&mut app, Action::Pulldown);
        assert!(app.world().resource::<Advisors>().pulldown);
        press(&mut app, Action::Choose(t));
        assert!(!app.world().resource::<Advisors>().pulldown);
        press(&mut app, Action::Ok);
        assert_eq!(app.world().resource::<Research>().target(0), Some(t));
    }

    #[test]
    fn enter_answers_the_popup_and_does_not_also_end_the_turn() {
        let mut app = app();
        settle(&mut app);
        app.add_systems(Update, (hotkeys, interrupt).chain());
        app.init_resource::<ButtonInput<KeyCode>>();
        app.update();
        assert_eq!(app.world().resource::<Advisors>().screen, Screen::ResearchAsk);
        let t = app.world().resource::<Research>().options(0)[0];
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Enter);
        app.update();
        assert_eq!(app.world().resource::<Research>().target(0), Some(t));
        assert_eq!(app.world().resource::<Advisors>().screen, Screen::Closed);
        assert!(!app.world().resource::<ButtonInput<KeyCode>>().just_pressed(KeyCode::Enter), "spent on the popup");
    }

    #[test]
    fn the_big_picture_opens_the_tree_and_closing_it_asks_again() {
        let mut app = app();
        settle(&mut app);
        app.add_systems(Update, (interrupt, respond).chain());
        app.update();
        press(&mut app, Action::BigPicture);
        assert_eq!(app.world().resource::<Advisors>().screen, Screen::Science);
        press(&mut app, Action::Close);
        app.update();
        assert_eq!(app.world().resource::<Advisors>().screen, Screen::ResearchAsk, "the choice is still owed");
        let t = app.world().resource::<Research>().options(0)[0];
        press(&mut app, Action::BigPicture);
        press(&mut app, Action::Pick(t));
        assert_eq!(app.world().resource::<Advisors>().screen, Screen::Closed);
        assert_eq!(app.world().resource::<Research>().target(0), Some(t));
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

    /// The research choice is not what these tests are about.
    fn pick_research(app: &mut App) {
        let t = app.world().resource::<Research>().options(0)[0];
        app.world_mut().resource_scope(|w, mut r: Mut<Research>| {
            r.pick(0, t, &mut w.resource_mut::<CombatRng>().0);
        });
    }

    fn press(app: &mut App, action: Action) {
        let button = app.world_mut().spawn((Interaction::Pressed, action)).id();
        app.update();
        app.world_mut().despawn(button);
    }

    #[test]
    fn a_leader_greets_us_once_when_we_meet() {
        let mut app = app();
        app.add_systems(Update, (interrupt, respond).chain());
        pick_research(&mut app);
        app.update();
        assert_eq!(app.world().resource::<Advisors>().screen, Screen::Closed, "nobody has been met");
        app.world_mut().resource_mut::<Diplomacy>().meet(0, 2);
        app.update();
        assert_eq!(app.world().resource::<Advisors>().screen, Screen::Greeting(2));
        press(&mut app, Action::Close);
        app.update();
        assert_eq!(app.world().resource::<Advisors>().screen, Screen::Closed, "the greeting is not repeated");
        app.world_mut().resource_mut::<Diplomacy>().meet(0, 3);
        app.update();
        assert_eq!(app.world().resource::<Advisors>().screen, Screen::Greeting(3), "the next leader is");
    }

    #[test]
    fn the_greeting_leads_to_the_talk_screen() {
        let mut app = app();
        app.add_systems(Update, (interrupt, respond).chain());
        pick_research(&mut app);
        app.world_mut().resource_mut::<Diplomacy>().meet(0, 1);
        app.update();
        press(&mut app, Action::Talk(1));
        let a = app.world().resource::<Advisors>();
        assert_eq!(a.screen, Screen::Talk(1));
        assert!(a.deal.is_empty() && a.said.is_empty(), "the table is bare and the leader has yet to answer");
    }

    #[test]
    fn the_leader_answers_a_proposal_in_their_own_words() {
        let mut app = app();
        app.add_systems(Update, (interrupt, respond).chain());
        app.insert_resource(Speech::parse("#AITHANKS\n#random 1\n\"Thanks, $PLAYER0.\"\n#AIWEAKREJECT\n#random 1\n\"Not quite.\"\n"));
        pick_research(&mut app);
        app.world_mut().resource_mut::<Diplomacy>().meet(0, 1);
        app.world_mut().resource_mut::<Advisors>().greeted[1] = true;
        // A gift is welcome.
        let t = app.world().resource::<Research>().giftable(0, 1)[0];
        app.world_mut().resource_mut::<Advisors>().talk_with(0, 1);
        app.world_mut().resource_mut::<Advisors>().deal.from_a.push(Clause::Tech(t));
        press(&mut app, Action::Propose);
        let a = app.world().resource::<Advisors>();
        assert_eq!(a.said, "Thanks, Tokugawa.");
        assert!(a.note.is_empty(), "the leader's words are all there is: {}", a.note);
        assert!(a.deal.is_empty(), "the table is cleared after a deal");
        assert!(app.world().resource::<Research>().knows(1, t));
    }

    #[test]
    fn without_the_text_the_game_gives_the_verdict() {
        let mut app = app();
        app.add_systems(Update, (interrupt, respond).chain());
        pick_research(&mut app);
        app.world_mut().resource_mut::<Diplomacy>().meet(0, 1);
        app.world_mut().resource_mut::<Advisors>().greeted[1] = true;
        let t = app.world().resource::<Research>().giftable(0, 1)[0];
        app.world_mut().resource_mut::<Advisors>().talk_with(0, 1);
        app.world_mut().resource_mut::<Advisors>().deal.from_a.push(Clause::Tech(t));
        press(&mut app, Action::Propose);
        let a = app.world().resource::<Advisors>();
        assert!(a.said.is_empty());
        assert_eq!(a.note, verdict_text(Verdict::Accept));
    }

    #[test]
    fn a_proposal_is_made_in_the_words_of_its_clause() {
        let mut deal = Deal::new(0, 1);
        assert_eq!(proposal_key(&deal), ("AIFIRSTDEAL", None));
        deal.from_a.push(Clause::Gold(5));
        deal.from_b.push(Clause::Tech(3));
        assert_eq!(proposal_key(&deal).0, "AITECHTRADE");
        deal.from_a.push(Clause::Embargo(crate::research::slot(2)));
        assert_eq!(proposal_key(&deal), ("AITRADEEMBARGO", Some(2)), "the treaty outranks the trade");
        deal.from_a.push(Clause::Peace);
        assert_eq!(proposal_key(&deal).0, "AIPEACETREATY", "peace outranks all");
    }

    #[test]
    fn a_wonder_of_ours_gets_a_splash_before_anything_else_and_zoom_looks_at_the_city() {
        let mut app = app();
        settle(&mut app);
        app.add_systems(Update, (hotkeys, interrupt, respond).chain());
        app.init_resource::<ButtonInput<KeyCode>>();
        let camera = app.world_mut().spawn((Camera2d, Transform::default())).id();
        // Research is still owed, and a wonder is done in Kyoto at (5, 6).
        let seen = crate::wonders::Seen { row: 0, civ: 0, city: "Kyoto".into(), x: 5, y: 6 };
        app.world_mut().resource_mut::<Wonders>().splash.push_back(crate::wonders::Splash { seen });
        app.update();
        assert_eq!(app.world().resource::<Advisors>().screen, Screen::Splash);
        press(&mut app, Action::Zoom(5, 6));
        assert_eq!(app.world().resource::<Advisors>().screen, Screen::Closed);
        app.update();
        assert_eq!(app.world().resource::<Advisors>().screen, Screen::ResearchAsk, "the splash is over; research is next");
        assert!(app.world().resource::<Wonders>().splash.is_empty());
        let at = crate::map::tile_to_world(5, 6);
        let eye = app.world().get::<Transform>(camera).unwrap().translation;
        assert_eq!((eye.x, eye.y), (at.x, at.y));
    }

    #[test]
    fn a_splash_ends_on_a_key_and_other_civs_splashes_wait_for_their_player() {
        let mut app = app();
        app.add_systems(Update, (hotkeys, interrupt).chain());
        app.init_resource::<ButtonInput<KeyCode>>();
        pick_research(&mut app);
        let seen = |civ| crate::wonders::Seen { row: 0, civ, city: "Kyoto".into(), x: 5, y: 6 };
        let mut wonders = app.world_mut().resource_mut::<Wonders>();
        wonders.splash.push_back(crate::wonders::Splash { seen: seen(1) });
        wonders.splash.push_back(crate::wonders::Splash { seen: seen(0) });
        app.update();
        assert_eq!(app.world().resource::<Advisors>().screen, Screen::Splash);
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Space);
        app.update();
        assert_eq!(app.world().resource::<Advisors>().screen, Screen::Closed);
        let left: Vec<usize> = app.world().resource::<Wonders>().splash.iter().map(|s| s.seen.civ).collect();
        assert_eq!(left, [1], "only ours was seen");
    }

    #[test]
    fn f7_toggles_the_wonders_window_and_the_page_stays_within_the_list() {
        let mut app = app();
        app.add_systems(Update, (hotkeys, interrupt, respond).chain());
        app.init_resource::<ButtonInput<KeyCode>>();
        pick_research(&mut app);
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::F7);
        app.update();
        assert_eq!(app.world().resource::<Advisors>().screen, Screen::Wonders);
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(KeyCode::F7);
        keys.clear();
        press(&mut app, Action::Page(1));
        assert_eq!(app.world().resource::<Advisors>().page, 0, "nothing is built: one page");
        press(&mut app, Action::Page(-1));
        assert_eq!(app.world().resource::<Advisors>().page, 0);
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::F7);
        app.update();
        assert_eq!(app.world().resource::<Advisors>().screen, Screen::Closed);
    }
}
