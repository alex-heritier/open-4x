//! The Domestic Advisor (F1): the government and a revolution, the three
//! commerce rates, the books and how each city is faring.
//!
//! The screen is Civ3's own (`advisor_frame`): the books in the green and
//! orange boxes, the science and luxury sliders, the government button, and
//! a line a city. It reads the same numbers the turn closes on
//! (`citycalc::totals`, `economy::finance`), so what it shows is what the
//! end of the turn does. Rules and addresses: `reverse-engineering/government.md`.

use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::prelude::*;
use civ3mapgen::government as exe;

use crate::advisor_frame::{self, Frame, Tab, art, art_button};
use crate::cities::{City, CityView, Production, Treasury};
use crate::citycalc;
use crate::civs::{Civilizations, is_ai};
use crate::economy;
use crate::features::{MessageBoard, post};
use crate::map::GameMap;
use crate::production_prompt::ProductionPrompts;
use crate::realm::{self, GOVT_NAMES, Rate, Rates};
use crate::rng::GameRng;
use crate::research::{Research, tech_name};
use crate::rules_data::TECH_NAMES;
use crate::stage::{self, Stage, Ui};
use bevy::window::PrimaryWindow;
use crate::units::Unit;

/// Whether the screen is up, and the revolution the player has asked for
/// once and not yet confirmed.
#[derive(Resource, Default)]
pub struct Domestic {
    open: bool,
    armed: Option<usize>,
    note: String,
    /// The government button's list is down.
    menu: bool,
    /// The first city line shown.
    scroll: usize,
}

impl Domestic {
    pub fn is_open(&self) -> bool {
        self.open
    }

    fn close(&mut self) {
        *self = Domestic::default();
    }

    /// Open from another advisor's tab.
    pub fn show_now(&mut self) {
        self.open = true;
    }

    /// Put away for another advisor's tab.
    pub fn hide(&mut self) {
        if self.open {
            self.close();
        }
    }
}

/// What a button does.
#[derive(Component, Clone, Debug, PartialEq)]
pub enum Click {
    Close,
    /// A tenth more of the rate, taken from the others.
    More(Rate),
    /// A tenth less of the rate.
    Less(Rate),
    /// Start a revolution toward the government (the second click confirms).
    Govt(usize),
    /// The government button: its list down or up.
    Menu,
    /// The city list a line up or down.
    Scroll(i32),
}

impl Click {
    /// The name `CIV3_SCRIPT`'s `dom` action presses it by.
    pub fn script_name(&self) -> String {
        let rate = |r: &Rate| match r {
            Rate::Tax => "Tax",
            Rate::Sci => "Sci",
            Rate::Lux => "Lux",
        };
        match self {
            Click::Close => "Close".into(),
            Click::More(r) => format!("More:{}", rate(r)),
            Click::Less(r) => format!("Less:{}", rate(r)),
            Click::Govt(g) => format!("Govt:{}", GOVT_NAMES[*g]),
            Click::Menu => "Menu".into(),
            Click::Scroll(d) => format!("Scroll:{d}"),
        }
    }
}

#[derive(Component)]
pub struct DomesticRoot;

#[derive(Component)]
pub struct OpenButton;

/// The human whose chair it is.
fn human(civs: &Civilizations) -> Option<usize> {
    (!is_ai(civs.active) && civs.outcome.is_none()).then(|| civs.viewer())
}

/// F1 opens the screen when nothing else is up; F1 and Escape close it.
pub fn hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    civs: Res<Civilizations>,
    prompts: Res<ProductionPrompts>,
    view: Res<CityView>,
    advisors: Res<crate::advisors::Advisors>,
    mut domestic: ResMut<Domestic>,
) {
    if human(&civs).is_none() {
        if domestic.open {
            domestic.close();
        }
        return;
    }
    if domestic.open {
        // Another modal took over (a question from a leader): step aside.
        if advisors.is_open() || prompts.blocks(civs.active) {
            domestic.close();
        } else if keys.any_just_pressed([KeyCode::Escape, KeyCode::F1]) {
            domestic.close();
        }
        return;
    }
    if keys.just_pressed(KeyCode::F1)
        && !advisors.is_open()
        && !prompts.blocks(civs.active)
        && view.0.is_none()
    {
        domestic.open = true;
    }
}

/// The small button for the mouse, under the other advisors'.
pub fn spawn_button(mut commands: Commands, assets: Res<AssetServer>) {
    let font = assets.load("gen/fonts/lsans.ttf");
    commands
        .spawn((
            Node { position_type: PositionType::Absolute, top: Val::Px(36.0), right: Val::Px(8.0), ..default() },
            GlobalZIndex(5),
        ))
        .with_children(|bar| {
            bar.spawn((
                Button,
                OpenButton,
                Node { padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)), ..default() },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
            ))
            .with_children(|b| {
                b.spawn((
                    Text::new("Domestic (F1)"),
                    TextFont { font, font_size: 15.0, ..default() },
                    TextColor(Color::srgb(1.0, 0.95, 0.7)),
                ));
            });
        });
}

pub fn open_button(
    buttons: Query<&Interaction, (Changed<Interaction>, With<OpenButton>)>,
    civs: Res<Civilizations>,
    prompts: Res<ProductionPrompts>,
    view: Res<CityView>,
    advisors: Res<crate::advisors::Advisors>,
    mut domestic: ResMut<Domestic>,
) {
    if human(&civs).is_none() || domestic.open || advisors.is_open() || prompts.blocks(civs.active) || view.0.is_some() {
        return;
    }
    if buttons.iter().any(|i| *i == Interaction::Pressed) {
        domestic.open = true;
    }
}

// ---------------------------------------------------------------------
// What the screen says
// ---------------------------------------------------------------------

/// Where a government stands for the player.
#[derive(Clone, Debug, PartialEq)]
pub enum Standing {
    Current,
    Open,
    /// Needs this advance.
    Locked(&'static str),
}

#[derive(Clone, Debug, PartialEq)]
pub struct CityLine {
    name: String,
    surplus: i32,
    shields: i32,
    commerce: i32,
    lux: i32,
    sci: i32,
    tax: i32,
    happy: u8,
    content: u8,
    unhappy: u8,
    entertainers: u8,
    disorder: bool,
    producing: Production,
    /// Turns to finish the build, `None` when it never will.
    turns: Option<u16>,
}

/// The whole screen as data: it is rebuilt only when this changes.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Page {
    govt: String,
    /// Turns of Anarchy left and the government that follows.
    anarchy: Option<(u8, String)>,
    rates: Option<Rates>,
    cap: u8,
    /// Gross commerce of every city, before corruption.
    from_cities: i32,
    sci: i32,
    lux: i32,
    corruption: i32,
    upkeep: i32,
    support: i32,
    net: i32,
    gold: u32,
    /// The advance being researched and its turns, for under the sliders.
    research: String,
    era: usize,
    governments: Vec<(usize, String, Standing)>,
    armed: Option<usize>,
    note: String,
    menu: bool,
    scroll: usize,
    cities: Vec<CityLine>,
}

#[allow(clippy::too_many_arguments)]
fn page(me: usize, map: &GameMap, cities: &[&City], owned: usize, gold: u32, domestic: &Domestic, research: Option<&Research>) -> Page {
    let (govt, anarchy_left, then, can) = realm::read(me, |r| {
        (r.govt, r.anarchy, r.then, (0..exe::SHIPPED.len()).map(|g| r.can_adopt(g)).collect::<Vec<_>>())
    });
    let books = economy::finance(map, cities.iter().copied(), owned);
    let mut p = Page {
        govt: GOVT_NAMES[govt].into(),
        rates: Some(realm::rates(me)),
        cap: realm::govt(me).rate_cap as u8,
        upkeep: books.upkeep as i32,
        support: books.unit_cost as i32,
        net: books.net(),
        gold,
        armed: domestic.armed,
        note: domestic.note.clone(),
        menu: domestic.menu,
        scroll: domestic.scroll,
        ..Page::default()
    };
    if let Some(r) = research {
        p.era = crate::tech_tree::era_of(r, me);
        if let Some(t) = r.target(me) {
            p.research = format!("{} ({})", tech_name(t), crate::advisors::turns_text(r.turns(me, t)));
        }
    }
    if govt == exe::row::ANARCHY {
        p.anarchy = Some((anarchy_left, GOVT_NAMES[then].into()));
    }
    let mut sorted: Vec<&&City> = cities.iter().collect();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    for c in sorted {
        let t = citycalc::totals(map, c);
        p.from_cities += t.commerce;
        p.sci += t.sci;
        p.lux += t.lux;
        p.corruption += t.corruption;
        let left = c.price(c.production).saturating_sub(c.shields);
        p.cities.push(CityLine {
            name: c.name.clone(),
            surplus: t.surplus,
            shields: t.shields,
            commerce: t.commerce,
            lux: t.lux,
            sci: t.sci,
            tax: t.tax,
            happy: t.mood.happy,
            content: t.mood.content,
            unhappy: t.mood.unhappy,
            entertainers: t.mood.entertainers,
            disorder: t.disorder,
            producing: c.production,
            turns: (t.shields > 0).then(|| left.div_ceil(t.shields as u16)),
        });
    }
    for g in 0..exe::SHIPPED.len() {
        if g == exe::row::ANARCHY {
            continue;
        }
        let standing = if g == govt {
            Standing::Current
        } else if can[g] {
            Standing::Open
        } else {
            let tech = exe::SHIPPED[g].prerequisite_tech;
            Standing::Locked(TECH_NAMES.get(tech.max(0) as usize).copied().unwrap_or("?"))
        };
        p.governments.push((g, GOVT_NAMES[g].into(), standing));
    }
    p
}

/// What the advisor says: a note first, then whatever is most pressing.
fn says(pg: &Page) -> String {
    if !pg.note.is_empty() {
        return pg.note.clone();
    }
    if let Some((turns, then)) = &pg.anarchy {
        return format!("Anarchy! {turns} more turn{} before {then}.", if *turns == 1 { "" } else { "s" });
    }
    if let Some(c) = pg.cities.iter().find(|c| c.disorder) {
        return format!("Civil disorder in {}! We must make our people content.", c.name);
    }
    if pg.net < 0 {
        return format!("We are losing {} gold a turn. Raise taxes, or our improvements will be sold.", -pg.net);
    }
    if pg.cities.len() < 6 {
        return "Build more cities!".into();
    }
    format!("Our treasury grows by {} gold a turn.", pg.net)
}

// ---------------------------------------------------------------------
// The panel, in the pixels of `domestic.pcx`
// ---------------------------------------------------------------------

const WARN: Color = Color::srgb(0.7, 0.1, 0.05);
const GREEN: Color = Color::srgb(0.30, 0.52, 0.18);
const ORANGE: Color = Color::srgb(0.85, 0.36, 0.18);
const KHAKI: Color = Color::srgb(0.52, 0.44, 0.24);
const PURPLE: Color = Color::srgb(0.55, 0.22, 0.60);
const BLUE: Color = Color::srgb(0.15, 0.25, 0.65);

/// City lines on the page, on the background's ruled lines 45 px apart.
const ROWS: usize = 9;
const ROW_Y: f32 = 325.0;
const ROW_STEP: f32 = 45.0;
/// The slider track: 0% at `TRACK_X`, 100% `TRACK_W` further.
const TRACK_X: f32 = 566.0;
const TRACK_W: f32 = 196.0;
/// The column centres: food, shields and commerce under the first
/// header pill; luxury, science and tax under the second.
const COLUMNS: [f32; 6] = [260.0, 302.0, 344.0, 436.0, 486.0, 536.0];

fn line(ui: &Ui, s: &mut ChildSpawnerCommands, x: f32, y: f32, w: f32, text: impl Into<String>, size: f32, color: Color, center: bool) {
    ui.words(s, x, y, w, size + 6.0, text, size, color, center);
}

/// One commerce slider: the knob at the rate and the -/+ under it.
fn slider(s: &mut ChildSpawnerCommands, ui: &Ui, assets: &AssetServer, which: Rate, tenths: u8, y: f32, knob: &str, below: bool) {
    let x = TRACK_X + tenths as f32 / 10.0 * TRACK_W - 10.0;
    ui.picture(s, art(assets, knob), x, y - 13.0, 21.0, 27.0);
    line(ui, s, 762.0, y - 8.0, 32.0, format!("{}%", tenths as u32 * 10), 11.0, Color::BLACK, true);
    let by = if below { y + 12.0 } else { y - 22.0 };
    art_button(s, ui, assets, "less", (563.0, by + 2.0, 12.0, 8.0), Click::Less(which));
    art_button(s, ui, assets, "more", (737.0, by, 12.0, 13.0), Click::More(which));
}

fn header(s: &mut ChildSpawnerCommands, ui: &Ui, assets: &AssetServer) {
    line(ui, s, 91.0, 255.0, 80.0, "Cities", 13.0, Color::BLACK, true);
    let city_icon = |cell: u32| {
        let mut n = ImageNode::new(assets.load("gen/cityscreen/CityIcons.png"));
        let x = 1.0 + cell as f32 * 31.0;
        n.rect = Some(Rect::new(x, 1.0, x + 30.0, 31.0));
        n
    };
    // CityIcons cells: food 6, shield 4, commerce 2.
    for (k, cell) in [6, 4, 2].into_iter().enumerate() {
        ui.picture(s, city_icon(cell), COLUMNS[k] - 12.0, 251.0, 24.0, 24.0);
    }
    for (k, icon) in ["smiley", "flask", "coins"].into_iter().enumerate() {
        ui.picture(s, art(assets, icon), COLUMNS[3 + k] - 10.0, 249.0, 21.0, 27.0);
    }
    line(ui, s, 660.0, 255.0, 135.0, "Population", 13.0, Color::BLACK, true);
    line(ui, s, 870.0, 255.0, 100.0, "Producing", 13.0, Color::BLACK, true);
}

fn city_row(s: &mut ChildSpawnerCommands, ui: &Ui, assets: &AssetServer, c: &CityLine, y: f32) {
    line(ui, s, 110.0, y - 9.0, 130.0, c.name.clone(), 13.0, if c.disorder { WARN } else { Color::BLACK }, false);
    let cols = [
        (format!("{:+}", c.surplus), Color::srgb(0.70, 0.22, 0.10)),
        (c.shields.to_string(), BLUE),
        (c.commerce.to_string(), Color::srgb(0.70, 0.22, 0.10)),
        (c.lux.to_string(), Color::srgb(0.15, 0.55, 0.65)),
        (c.sci.to_string(), PURPLE),
        (c.tax.to_string(), KHAKI),
    ];
    for ((text, color), x) in cols.into_iter().zip(COLUMNS) {
        line(ui, s, x - 20.0, y - 8.0, 40.0, text, 12.0, color, true);
    }
    // The citizens by mood, then a gap and the entertainers.
    let heads: Vec<&str> = std::iter::repeat_n("head_happy", c.happy as usize)
        .chain(std::iter::repeat_n("head_content", c.content as usize))
        .chain(std::iter::repeat_n("head_unhappy", c.unhappy as usize))
        .collect();
    let total = heads.len() + c.entertainers as usize;
    let step = if total == 0 { 30.0 } else { (240.0 / (total as f32 + 1.0)).min(30.0) };
    let mut x = 607.0;
    for head in heads {
        ui.picture(s, art(assets, head), x, y - 19.0, 34.0, 34.0);
        x += step;
    }
    if c.entertainers > 0 {
        x += step;
        for _ in 0..c.entertainers {
            ui.picture(s, ImageNode::new(assets.load("gen/ui/entertainer.png")), x, y - 19.0, 34.0, 34.0);
            x += step;
        }
    }
    ui.picture(s, crate::tech_tree::item_icon(assets, c.producing), 872.0, y - 14.0, 28.0, 28.0);
    let turns = match c.turns {
        Some(1) => "(1 turn)".to_string(),
        Some(n) => format!("({n} turns)"),
        None => "(never)".into(),
    };
    line(ui, s, 903.0, y - 14.0, 82.0, format!("{}\n{turns}", c.producing.name()), 10.0, BLUE, false);
}

fn body(s: &mut ChildSpawnerCommands, ui: &Ui, assets: &AssetServer, pg: &Page) {
    let f = Frame {
        background: "domestic",
        title: "Domestic Advisor",
        portrait: format!("portrait_domestic_{}", pg.era),
        says: says(pg),
        tab: Tab::Domestic,
    };
    advisor_frame::frame(s, ui, assets, &f, Some(Click::Close));
    // The books: income in green, expenses in orange, what is left below.
    for (k, text) in [
        format!("From cities: {:+}", pg.from_cities),
        "From taxmen: +0".into(),
        "From other civs: +0".into(),
        "From interest: +0".into(),
    ]
    .into_iter()
    .enumerate()
    {
        line(ui, s, 80.0, 97.0 + 16.0 * k as f32, 160.0, text, 11.0, Color::BLACK, true);
    }
    line(ui, s, 250.0, 104.0, 120.0, format!("Income: {}", pg.from_cities), 13.0, GREEN, false);
    let expenses = pg.sci + pg.lux + pg.corruption + pg.upkeep + pg.support;
    line(ui, s, 250.0, 153.0, 120.0, format!("Expenses: {expenses}"), 13.0, ORANGE, false);
    for (k, (n, what)) in [
        (pg.sci, "Science"),
        (pg.lux, "Entertainment"),
        (pg.corruption, "Corruption"),
        (pg.upkeep, "Maintenance"),
        (pg.support, "Unit costs"),
        (0, "To other civs"),
    ]
    .into_iter()
    .enumerate()
    {
        line(ui, s, 384.0, 89.0 + 16.0 * k as f32, 160.0, format!("-{n}: {what}"), 11.0, Color::BLACK, false);
    }
    line(ui, s, 80.0, 195.0, 160.0, format!("Treasury: {} Gold", pg.gold), 13.0, KHAKI, true);
    let net_color = if pg.net < 0 { WARN } else { KHAKI };
    line(ui, s, 245.0, 195.0, 115.0, format!("Net Gain:{:+}", pg.net), 13.0, net_color, true);

    // Science on the upper slider, luxury on the lower; taxes take the rest.
    if let Some(r) = pg.rates {
        slider(s, ui, assets, Rate::Sci, r.sci, 99.0, "flask", true);
        slider(s, ui, assets, Rate::Lux, r.lux, 146.0, "smiley", false);
    }
    line(ui, s, 562.0, 166.0, 240.0, pg.research.clone(), 11.0, PURPLE, false);
    line(ui, s, 557.0, 191.0, 90.0, "Government", 13.0, Color::BLACK, false);
    let govt = match &pg.anarchy {
        Some((turns, _)) => format!("Anarchy ({turns})"),
        None => pg.govt.clone(),
    };
    govt_button(s, ui, assets, 187.0, govt, Click::Menu);
    if pg.menu {
        let open = pg.governments.iter().filter(|(_, _, st)| *st == Standing::Open);
        for (k, (g, name, _)) in open.enumerate() {
            let label = if pg.armed == Some(*g) { format!("Confirm {name}") } else { name.clone() };
            govt_button(s, ui, assets, 213.0 + 26.0 * k as f32, label, Click::Govt(*g));
        }
    }

    header(s, ui, assets);
    for (k, c) in pg.cities.iter().skip(pg.scroll).take(ROWS).enumerate() {
        city_row(s, ui, assets, c, ROW_Y + ROW_STEP * k as f32);
    }
    if pg.cities.is_empty() {
        line(ui, s, 110.0, ROW_Y - 9.0, 300.0, "We have no cities yet.", 13.0, Color::BLACK, false);
    }
    if pg.cities.len() > ROWS {
        for (stem, y, d) in [("scroll_up_0", 290.0, -1), ("scroll_down_0", 690.0, 1)] {
            s.spawn((Button, Click::Scroll(d), ImageNode::new(assets.load(format!("gen/ui/{stem}.png"))), ui.st.rect(958.0, y, 18.0, 16.0)));
        }
    }
}

fn govt_button(s: &mut ChildSpawnerCommands, ui: &Ui, assets: &AssetServer, y: f32, label: String, click: Click) {
    s.spawn((
        Button,
        advisor_frame::ArtButton("govt"),
        click,
        art(assets, "govt_0"),
        Node { justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..ui.st.rect(650.0, y, 146.0, 26.0) },
    ))
    .with_children(|b| {
        b.spawn((
            Text::new(label),
            TextFont { font: ui.font.clone(), font_size: ui.st.font(13.0), ..default() },
            TextColor(Color::BLACK),
        ));
    });
}

/// Draw the screen (and take it down) when the page changes.
#[allow(clippy::too_many_arguments)]
pub fn show(
    mut commands: Commands,
    domestic: Res<Domestic>,
    civs: Res<Civilizations>,
    map: Res<GameMap>,
    treasury: Res<Treasury>,
    research: Res<Research>,
    cities: Query<&City>,
    units: Query<&Unit>,
    assets: Res<AssetServer>,
    roots: Query<Entity, With<DomesticRoot>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut shown: Local<Option<(Page, f32)>>,
) {
    if !domestic.open {
        if shown.take().is_some() {
            for e in &roots {
                commands.entity(e).despawn();
            }
        }
        return;
    }
    let me = civs.viewer();
    let mine: Vec<&City> = cities.iter().filter(|c| c.civ == me).collect();
    let owned = units.iter().filter(|u| u.civ == me).count();
    let pg = page(me, &map, &mine, owned, treasury.0[me], &domestic, Some(&research));
    let scale = stage::scale_of(&windows);
    if shown.as_ref().is_some_and(|(p, s)| *p == pg && *s == scale) {
        return;
    }
    for e in &roots {
        commands.entity(e).despawn();
    }
    let font = assets.load("gen/fonts/lsans.ttf");
    let st = Stage(scale);
    let ui = Ui { font: &font, st };
    let root = stage::spawn(&mut commands, DomesticRoot, st, 0.0, 100);
    commands.entity(root).with_children(|s| body(s, &ui, &assets, &pg));
    *shown = Some((pg, scale));
}

// ---------------------------------------------------------------------
// The answers
// ---------------------------------------------------------------------

/// Move a tenth toward (`more`) or away from a rate, keeping the three at
/// ten and under the government's cap.
pub fn nudge(rates: Rates, which: Rate, more: bool, cap: u8) -> Option<Rates> {
    let others: Vec<Rate> = [Rate::Tax, Rate::Sci, Rate::Lux].into_iter().filter(|r| *r != which).collect();
    let get = |r: Rates, w: Rate| match w {
        Rate::Tax => r.tax,
        Rate::Sci => r.sci,
        Rate::Lux => r.lux,
    };
    // Take from, or give to, the biggest other rate first; luxury goes last.
    let mut order = others.clone();
    order.sort_by_key(|w| (matches!(w, Rate::Lux), std::cmp::Reverse(get(rates, *w))));
    if more {
        order.iter().find_map(|&from| rates.shifted(from, which, cap))
    } else {
        let mut give = others;
        give.sort_by_key(|w| (matches!(w, Rate::Lux), get(rates, *w)));
        give.iter().find_map(|&to| rates.shifted(which, to, cap))
    }
}

pub fn respond(
    buttons: Query<(&Interaction, &Click), Changed<Interaction>>,
    civs: Res<Civilizations>,
    mut domestic: ResMut<Domestic>,
    mut dice: ResMut<GameRng>,
    mut board: ResMut<MessageBoard>,
) {
    if !domestic.open {
        return;
    }
    let me = civs.viewer();
    for (interaction, click) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match click {
            Click::Close => domestic.close(),
            Click::Menu => {
                domestic.menu = !domestic.menu;
                domestic.armed = None;
                domestic.note.clear();
            }
            Click::Scroll(d) => domestic.scroll = (domestic.scroll as i32 + d).max(0) as usize,
            Click::More(r) | Click::Less(r) => {
                let more = matches!(click, Click::More(_));
                let now = realm::rates(me);
                let cap = realm::govt(me).rate_cap as u8;
                domestic.note.clear();
                if realm::in_anarchy(me) {
                    domestic.note = "During Anarchy the rates are not ours to set.".into();
                } else if let Some(next) = nudge(now, *r, more, cap) {
                    realm::set_rates(me, next);
                }
            }
            Click::Govt(g) => {
                let current = realm::read(me, |r| r.govt);
                domestic.note.clear();
                if *g == current || !realm::read(me, |r| r.can_adopt(*g)) {
                    continue;
                }
                if domestic.armed != Some(*g) {
                    domestic.armed = Some(*g);
                    domestic.note = format!(
                        "A revolution brings Anarchy, with no taxes or science. Press {} again to begin.",
                        GOVT_NAMES[*g]
                    );
                    continue;
                }
                domestic.armed = None;
                let turns = crate::govern::revolt(me, *g, &mut dice);
                let text = if turns <= 1 {
                    format!("Revolution! Our people now live under {}.", GOVT_NAMES[*g])
                } else {
                    format!("Revolution! {turns} turns of Anarchy before {}.", GOVT_NAMES[*g])
                };
                post(&mut board, text);
                domestic.close();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: Rates = Rates { tax: 5, sci: 5, lux: 0 };

    #[test]
    fn more_science_takes_from_the_larger_other_rate() {
        let r = nudge(R, Rate::Sci, true, 10).unwrap();
        assert_eq!(r, Rates { tax: 4, sci: 6, lux: 0 });
        // Taxes and luxury: the luxury rate takes from the larger of the two.
        assert_eq!(nudge(R, Rate::Lux, true, 10), Some(Rates { tax: 4, sci: 5, lux: 1 }));
    }

    #[test]
    fn less_gives_the_tenth_to_the_smaller_other_rate_and_luxury_last() {
        // Less tax: the tenth goes to science unless luxury is smaller... but
        // luxury is the last resort, so science gets it.
        assert_eq!(nudge(R, Rate::Tax, false, 10), Some(Rates { tax: 4, sci: 6, lux: 0 }));
        let all_tax = Rates { tax: 10, sci: 0, lux: 0 };
        assert_eq!(nudge(all_tax, Rate::Tax, false, 10), Some(Rates { tax: 9, sci: 1, lux: 0 }));
    }

    #[test]
    fn the_cap_and_the_floor_hold() {
        let capped = Rates { tax: 6, sci: 4, lux: 0 };
        assert_eq!(nudge(capped, Rate::Tax, true, 6), None, "sixty percent is the cap");
        assert_eq!(nudge(Rates { tax: 10, sci: 0, lux: 0 }, Rate::Sci, false, 10), None, "nothing to give");
    }

    #[test]
    fn a_government_needs_its_advance_to_be_open() {
        realm::reset();
        let map = GameMap::generate();
        let pg = page(0, &map, &[], 0, 0, &Domestic::default(), None);
        let monarchy = pg.governments.iter().find(|(g, ..)| *g == exe::row::MONARCHY).unwrap();
        assert!(matches!(monarchy.2, Standing::Locked(_)));
        let despotism = pg.governments.iter().find(|(g, ..)| *g == exe::row::DESPOTISM).unwrap();
        assert_eq!(despotism.2, Standing::Current);
        realm::write(0, |r| r.known |= 1 << exe::SHIPPED[exe::row::MONARCHY].prerequisite_tech);
        let pg = page(0, &map, &[], 0, 0, &Domestic::default(), None);
        let monarchy = pg.governments.iter().find(|(g, ..)| *g == exe::row::MONARCHY).unwrap();
        assert_eq!(monarchy.2, Standing::Open);
    }
}
