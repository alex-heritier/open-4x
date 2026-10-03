//! The Domestic Advisor (F1): the government and a revolution, the three
//! commerce rates, the books and how each city is faring.
//!
//! The screen is a modal like the other advisors'. It reads the same
//! numbers the turn closes on (`citycalc::totals`, `economy::finance`), so
//! what it shows is what the end of the turn does. Rules and addresses:
//! `reverse-engineering/government.md`.

use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::prelude::*;
use civ3mapgen::government as exe;

use crate::cities::{City, CityView, Treasury};
use crate::citycalc;
use crate::civs::{Civilizations, is_ai};
use crate::economy;
use crate::features::{MessageBoard, post};
use crate::map::GameMap;
use crate::production_prompt::ProductionPrompts;
use crate::realm::{self, GOVT_NAMES, Rate, Rates};
use crate::rng::GameRng;
use crate::rules_data::TECH_NAMES;
use crate::units::Unit;

/// Whether the screen is up, and the revolution the player has asked for
/// once and not yet confirmed.
#[derive(Resource, Default)]
pub struct Domestic {
    open: bool,
    armed: Option<usize>,
    note: String,
}

impl Domestic {
    pub fn is_open(&self) -> bool {
        self.open
    }

    fn close(&mut self) {
        *self = Domestic::default();
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
    size: u8,
    happy: u8,
    content: u8,
    unhappy: u8,
    entertainers: u8,
    disorder: bool,
    corruption: i32,
}

/// The whole screen as data: it is rebuilt only when this changes.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Page {
    govt: String,
    /// Turns of Anarchy left and the government that follows.
    anarchy: Option<(u8, String)>,
    rates: Option<Rates>,
    cap: u8,
    tax: i32,
    sci: i32,
    lux: i32,
    upkeep: i32,
    support: i32,
    net: i32,
    gold: u32,
    governments: Vec<(usize, String, Standing)>,
    armed: Option<usize>,
    note: String,
    cities: Vec<CityLine>,
}

fn page(me: usize, map: &GameMap, cities: &[&City], owned: usize, gold: u32, domestic: &Domestic) -> Page {
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
        ..Page::default()
    };
    if govt == exe::row::ANARCHY {
        p.anarchy = Some((anarchy_left, GOVT_NAMES[then].into()));
    }
    for c in cities {
        let t = citycalc::totals(map, c);
        p.tax += t.tax;
        p.sci += t.sci;
        p.lux += t.lux;
        p.cities.push(CityLine {
            name: c.name.clone(),
            size: c.size,
            happy: t.mood.happy,
            content: t.mood.content,
            unhappy: t.mood.unhappy,
            entertainers: t.mood.entertainers,
            disorder: t.disorder,
            corruption: t.corruption,
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

// ---------------------------------------------------------------------
// The panel
// ---------------------------------------------------------------------

const INK: Color = Color::srgb(0.05, 0.3, 0.65);
const SELECTED: Color = Color::srgba(0.25, 0.55, 0.3, 0.4);
const IDLE: Color = Color::srgba(0.65, 0.60, 0.40, 0.18);
const WARN: Color = Color::srgb(0.7, 0.1, 0.05);

fn text(p: &mut ChildSpawnerCommands, font: &Handle<Font>, s: impl Into<String>, size: f32, color: Color) {
    p.spawn((
        Text::new(s),
        TextFont { font: font.clone(), font_size: size, ..default() },
        TextColor(color),
    ));
}

fn button(p: &mut ChildSpawnerCommands, font: &Handle<Font>, label: &str, click: Click, on: bool) {
    p.spawn((
        Button,
        click,
        Node {
            min_height: Val::Px(26.0),
            padding: UiRect::axes(Val::Px(10.0), Val::Px(2.0)),
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(if on { SELECTED } else { IDLE }),
    ))
    .with_children(|b| {
        text(b, font, label, 16.0, INK);
    });
}

fn row(p: &mut ChildSpawnerCommands, f: impl FnOnce(&mut ChildSpawnerCommands)) {
    p.spawn(Node { column_gap: Val::Px(10.0), align_items: AlignItems::Center, ..default() })
        .with_children(f);
}

fn rate_row(p: &mut ChildSpawnerCommands, font: &Handle<Font>, which: Rate, name: &str, tenths: u8, gives: i32) {
    row(p, |r| {
        r.spawn(Node { width: Val::Px(120.0), ..default() }).with_children(|n| text(n, font, name, 17.0, Color::BLACK));
        button(r, font, "-", Click::Less(which), false);
        r.spawn(Node { width: Val::Px(56.0), justify_content: JustifyContent::Center, ..default() })
            .with_children(|n| text(n, font, format!("{}%", tenths as u32 * 10), 17.0, INK));
        button(r, font, "+", Click::More(which), false);
        text(r, font, format!("{gives} per turn"), 16.0, Color::BLACK);
    });
}

fn body(panel: &mut ChildSpawnerCommands, font: &Handle<Font>, pg: &Page) {
    text(panel, font, "Domestic Advisor", 24.0, INK);
    match &pg.anarchy {
        Some((turns, then)) => text(
            panel,
            font,
            format!("Government: ANARCHY - {turns} more turn{}, then {then}", if *turns == 1 { "" } else { "s" }),
            18.0,
            WARN,
        ),
        None => text(panel, font, format!("Government: {}", pg.govt), 18.0, Color::BLACK),
    }
    if let Some(r) = pg.rates {
        text(panel, font, "Commerce rates", 18.0, INK);
        rate_row(panel, font, Rate::Tax, "Taxes", r.tax, pg.tax);
        rate_row(panel, font, Rate::Sci, "Science", r.sci, pg.sci);
        rate_row(panel, font, Rate::Lux, "Luxuries", r.lux, pg.lux);
        text(
            panel,
            font,
            format!(
                "Treasury {}   Income {:+} (upkeep {}, units {})",
                pg.gold,
                pg.net,
                pg.upkeep,
                pg.support
            ),
            16.0,
            Color::BLACK,
        );
    }
    text(panel, font, "Governments", 18.0, INK);
    panel
        .spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(6.0), row_gap: Val::Px(4.0), ..default() })
        .with_children(|list| {
            for (g, name, standing) in &pg.governments {
                match standing {
                    Standing::Current => button(list, font, &format!("{name} (now)"), Click::Govt(*g), true),
                    Standing::Open => {
                        let label = if pg.armed == Some(*g) { format!("Confirm {name}") } else { name.clone() };
                        button(list, font, &label, Click::Govt(*g), pg.armed == Some(*g));
                    }
                    Standing::Locked(tech) => {
                        list.spawn(Node { padding: UiRect::axes(Val::Px(10.0), Val::Px(2.0)), ..default() })
                            .with_children(|n| text(n, font, format!("{name} (needs {tech})"), 16.0, Color::srgb(0.4, 0.4, 0.4)));
                    }
                }
            }
        });
    if !pg.note.is_empty() {
        text(panel, font, pg.note.clone(), 16.0, WARN);
    }
    text(panel, font, "Cities", 18.0, INK);
    for c in pg.cities.iter().take(14) {
        let mood = format!(
            "{} happy {} content {} unhappy{}",
            c.happy,
            c.content,
            c.unhappy,
            if c.entertainers > 0 { format!(" {} entertaining", c.entertainers) } else { String::new() }
        );
        let line = format!("{}  (size {})  {mood}  lost to corruption {}", c.name, c.size, c.corruption);
        text(panel, font, if c.disorder { format!("{line}  DISORDER") } else { line }, 15.0, if c.disorder { WARN } else { Color::BLACK });
    }
    if pg.cities.is_empty() {
        text(panel, font, "No cities yet.", 16.0, Color::BLACK);
    }
    row(panel, |r| button(r, font, "Close", Click::Close, false));
}

/// Draw the screen (and take it down) when the page changes.
pub fn show(
    mut commands: Commands,
    domestic: Res<Domestic>,
    civs: Res<Civilizations>,
    map: Res<GameMap>,
    treasury: Res<Treasury>,
    cities: Query<&City>,
    units: Query<&Unit>,
    assets: Res<AssetServer>,
    roots: Query<Entity, With<DomesticRoot>>,
    mut shown: Local<Option<Page>>,
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
    let pg = page(me, &map, &mine, owned, treasury.0[me], &domestic);
    if shown.as_ref() == Some(&pg) {
        return;
    }
    for e in &roots {
        commands.entity(e).despawn();
    }
    let font = assets.load("gen/fonts/lsans.ttf");
    commands
        .spawn((
            DomesticRoot,
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
                    width: Val::Px(720.0),
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
            .with_children(|panel| body(panel, &font, &pg));
        });
    *shown = Some(pg);
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
        let pg = page(0, &map, &[], 0, 0, &Domestic::default());
        let monarchy = pg.governments.iter().find(|(g, ..)| *g == exe::row::MONARCHY).unwrap();
        assert!(matches!(monarchy.2, Standing::Locked(_)));
        let despotism = pg.governments.iter().find(|(g, ..)| *g == exe::row::DESPOTISM).unwrap();
        assert_eq!(despotism.2, Standing::Current);
        realm::write(0, |r| r.known |= 1 << exe::SHIPPED[exe::row::MONARCHY].prerequisite_tech);
        let pg = page(0, &map, &[], 0, 0, &Domestic::default());
        let monarchy = pg.governments.iter().find(|(g, ..)| *g == exe::row::MONARCHY).unwrap();
        assert_eq!(monarchy.2, Standing::Open);
    }
}
