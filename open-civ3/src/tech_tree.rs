//! The Science Advisor's tree: one page an era (Ancient Times, the Middle
//! Ages, the Industrial Ages, Modern Times), every advance of the era in a
//! box at its `TECH` position, over Conquests' background with the arrows
//! drawn in.
//!
//! A box shows the advance's name and icon and what it brings: the units
//! the viewer's race may train, then the improvements, then the wonders.
//! Its colour says where the advance stands: blue known, green being
//! researched, yellow open to research, grey out of reach. Pressing an open
//! advance makes it the target.
//!
//! The box sheet (`techboxes.pcx`) holds four sizes an era, and the game
//! blits whole grid cells, transparent margin included: `TECH` x and y are
//! the cell's corner, and the box shows `MARGIN` further in. The size
//! follows what the box must hold (`size_for`), read off the shipped tree:
//! one item, up to three, four, and two rows. Layout inside a box is
//! measured off Conquests' screens: the icon at (11, 27) and 32-px items
//! on a 33-px step from x = 49.

use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::prelude::*;

use crate::advisor_frame::{self, ArtButton, Frame, Tab};
use crate::advisors::Action;
use crate::cities::Production;
use crate::civs::RACES;
use crate::research::{Research, tech_name};
use crate::roster::{self, bldg_count, unit_count};
use crate::ruleset::TECH_TREE;
use crate::stage::Ui;
use crate::units::UnitType;

pub const ERAS: [&str; 4] = [
    "Ancient Times",
    "Middle Ages",
    "Industrial Ages",
    "Modern Times",
];

/// Where an advance stands for the viewer: the column of `techboxes.pcx`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Known = 0,
    Researching = 1,
    Open = 2,
    Unreachable = 3,
}

/// Sizes of a box: the row of its era's band in `techboxes.pcx`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    Small = 0,
    Medium = 1,
    Tall = 2,
    Wide = 3,
}

/// `(width, height, left margin, top margin)` of each box cell, by era and
/// size, measured off `techboxes.pcx` (grid lines at a 189-px column stride
/// and the rows' own lines).
const CELLS: [[(f32, f32, f32, f32); 4]; 4] = [
    [
        (98.0, 64.0, 3.0, 9.0),
        (159.0, 70.0, 1.0, 5.0),
        (161.0, 101.0, 1.0, 4.0),
        (188.0, 70.0, 1.0, 5.0),
    ],
    [
        (98.0, 64.0, 3.0, 9.0),
        (159.0, 70.0, 1.0, 6.0),
        (160.0, 100.0, 1.0, 7.0),
        (188.0, 70.0, 1.0, 7.0),
    ],
    [
        (104.0, 67.0, 3.0, 7.0),
        (158.0, 75.0, 3.0, 3.0),
        (159.0, 101.0, 2.0, 5.0),
        (188.0, 72.0, 1.0, 4.0),
    ],
    [
        (106.0, 69.0, 1.0, 7.0),
        (163.0, 75.0, 1.0, 5.0),
        (163.0, 103.0, 1.0, 4.0),
        (188.0, 72.0, 1.0, 5.0),
    ],
];

const ICON: f32 = 32.0;
const STEP: f32 = 33.0;
const ICON_AT: (f32, f32) = (11.0, 27.0);
const ITEMS_X: f32 = 49.0;

/// The box that holds `n` items, and how many it shows.
pub fn size_for(n: usize) -> (Size, usize) {
    match n {
        0..=1 => (Size::Small, 1),
        2..=3 => (Size::Medium, 3),
        4 => (Size::Wide, 4),
        _ => (Size::Tall, 6),
    }
}

/// What advance `t` brings the viewer `civ`: units its race may train
/// (each once, Conquests repeats some rows), improvements, then wonders.
pub fn brings(civ: usize, t: i32) -> Vec<Production> {
    let race = RACES[civ].race;
    let mut names: Vec<&str> = vec![];
    let mut out = vec![];
    for u in 0..unit_count() {
        let row = UnitType(u as u16).row();
        if row.tech == t && row.races >> race & 1 != 0 && !names.contains(&row.name) {
            names.push(row.name);
            out.push(Production::from_unit(UnitType(u as u16)));
        }
    }
    let wonder = |i: usize| roster::bldg(i).wonder != 0 || roster::bldg(i).small != 0;
    for pass_wonders in [false, true] {
        for i in 0..bldg_count() {
            if roster::bldg(i).tech == t && wonder(i) == pass_wonders {
                out.push(Production::from_building_row(i));
            }
        }
    }
    out
}

pub fn state(r: &Research, civ: usize, t: i32) -> State {
    if r.knows(civ, t) {
        State::Known
    } else if r.target(civ) == Some(t) {
        State::Researching
    } else if r.options(civ).contains(&t) {
        State::Open
    } else {
        State::Unreachable
    }
}

/// The era page to open on: the viewer's own era.
pub fn era_of(r: &Research, civ: usize) -> usize {
    r.era(civ).clamp(0, 3) as usize
}

fn title_color(s: State) -> Color {
    match s {
        State::Known => Color::srgb(0.08, 0.13, 0.45),
        State::Researching => Color::srgb(0.05, 0.35, 0.65),
        State::Open => Color::srgb(0.48, 0.40, 0.22),
        State::Unreachable => Color::srgb(0.30, 0.28, 0.25),
    }
}

/// The 32-px icon of something an advance brings.
pub fn item_icon(assets: &AssetServer, p: Production) -> ImageNode {
    let (path, rect) = match p.unit() {
        Some(_) => ("cache/ui/unit_icons.png", p.unit_icon_rect()),
        None => ("cache/cityscreen/buildings-small.png", p.building_rect()),
    };
    let mut n = ImageNode::new(assets.load(path));
    n.rect = rect;
    n
}

fn tech_box(
    s: &mut ChildSpawnerCommands,
    ui: &Ui,
    assets: &AssetServer,
    r: &Research,
    civ: usize,
    t: i32,
) {
    let (era, _, x, y) = TECH_TREE[t as usize];
    let st = state(r, civ, t);
    let items = brings(civ, t);
    let (size, room) = size_for(items.len());
    let (w, h, mx, my) = CELLS[era as usize][size as usize];
    // The visible box's corner.
    let (bx, by) = (x as f32 + mx, y as f32 + my);
    let cell = format!("techbox_{era}_{}_{}", size as usize, st as usize);
    let node = ui.st.rect(bx, by, w, h);
    if matches!(st, State::Open | State::Researching) {
        s.spawn((
            Button,
            Action::Pick(t),
            advisor_frame::art(assets, &cell),
            node,
        ));
    } else {
        s.spawn((advisor_frame::art(assets, &cell), node));
    }
    let mut name = tech_name(t).to_string();
    if matches!(st, State::Open | State::Researching) {
        name = format!("{name} ({})", crate::advisors::turns_text(r.turns(civ, t)));
    }
    // One line, cut at the box's edge ("Monarchy (15 tu").
    s.spawn((Node {
        overflow: Overflow::clip(),
        ..ui.st.rect(bx + 14.0, by + 9.0, w - 22.0, 16.0)
    },))
        .with_children(|clip| {
            clip.spawn((
                Text::new(name),
                TextFont {
                    font: ui.font.clone(),
                    font_size: ui.st.font(11.0),
                    ..default()
                },
                TextColor(title_color(st)),
                TextLayout::new(Justify::Left, LineBreak::NoWrap),
            ));
        });
    if let Some(icon) = crate::ruleset::get().art.tech_icon(t as usize) {
        ui.picture(
            s,
            ImageNode::new(assets.load(icon)),
            bx + ICON_AT.0,
            by + ICON_AT.1,
            ICON,
            ICON,
        );
    }
    // The tall box's second row runs under the first row's items.
    let per_row = if size == Size::Tall { 3 } else { room };
    for (k, p) in items.iter().take(room).enumerate() {
        let (col, row) = (k % per_row, k / per_row);
        let ix = bx + ITEMS_X + col as f32 * STEP;
        let iy = by + ICON_AT.1 + row as f32 * STEP;
        ui.picture(s, item_icon(assets, *p), ix, iy, ICON, ICON);
    }
    if r.world.rules.techs[t as usize].flags & NOT_REQUIRED_FOR_ERA != 0 {
        ui.picture(
            s,
            advisor_frame::art(assets, "non_required"),
            bx + w - 16.0,
            by - 10.0,
            27.0,
            27.0,
        );
    }
}

/// `TECH` flag `0x20000`: the era can be entered without it
/// (`research.md` 7.1); the box wears Civ3's "not required" badge.
const NOT_REQUIRED_FOR_ERA: u32 = 0x20000;

/// What the Science Advisor says on the tree.
pub fn says(r: &Research, civ: usize) -> String {
    match r.target(civ) {
        _ if r.needs_choice(civ) => {
            "Excellency, what shall our scientists study? Choose an advance in yellow.".into()
        }
        Some(t) => {
            let names: Vec<&str> = brings(civ, t).iter().take(3).map(|p| p.name()).collect();
            let gives = if names.is_empty() {
                String::new()
            } else {
                format!(" {} will give us {}.", tech_name(t), names.join(", "))
            };
            format!(
                "We are researching {} and will learn it in {}.{gives}",
                tech_name(t),
                crate::advisors::turns_text(r.turns(civ, t))
            )
        }
        None if r.options(civ).is_empty() => "We are technologically advanced!".into(),
        None => "Our scientists have no target.".into(),
    }
}

/// The page of `era`.
pub fn page(
    s: &mut ChildSpawnerCommands,
    ui: &Ui,
    assets: &AssetServer,
    r: &Research,
    civ: usize,
    era: usize,
) {
    let background = format!("science_{era}");
    let f = Frame {
        background: &background,
        title: "Science Advisor",
        portrait: format!("portrait_science_{}", era_of(r, civ)),
        says: says(r, civ),
        tab: Tab::Science,
    };
    // Closing with a target still owed brings back the research popup.
    advisor_frame::frame(s, ui, assets, &f, Some(Action::Close));
    for t in 0..TECH_TREE.len() as i32 {
        if TECH_TREE[t as usize].0 == era as i32 {
            tech_box(s, ui, assets, r, civ, t);
        }
    }
    ui.words(
        s,
        300.0,
        728.0,
        424.0,
        30.0,
        ERAS[era],
        22.0,
        Color::BLACK,
        true,
    );
    if era > 0 {
        nav(s, ui, assets, 229.0, -1, era - 1);
        ui.picture(
            s,
            advisor_frame::art(assets, "nav_left"),
            183.0,
            742.0,
            45.0,
            10.0,
        );
    }
    if era < 3 {
        nav(s, ui, assets, 674.0, 1, era + 1);
        ui.picture(
            s,
            advisor_frame::art(assets, "nav_right"),
            806.0,
            742.0,
            45.0,
            10.0,
        );
    }
}

/// "To Middle Ages" and the like.
fn nav(s: &mut ChildSpawnerCommands, ui: &Ui, assets: &AssetServer, x: f32, step: i32, to: usize) {
    let to_name = match to {
        0 => "To Ancient Times",
        1 => "To Middle Ages",
        2 => "To Industrial Ages",
        _ => "To Modern Times",
    };
    s.spawn((
        Button,
        ArtButton("nav"),
        Action::Era(step),
        advisor_frame::art(assets, "nav_0"),
        Node {
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..ui.st.rect(x, 730.0, 129.0, 34.0)
        },
    ))
    .with_children(|b| {
        b.spawn((
            Text::new(to_name),
            TextFont {
                font: ui.font.clone(),
                font_size: ui.st.font(13.0),
                ..default()
            },
            TextColor(Color::BLACK),
        ));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ruleset::TECH_NAMES;

    fn tech(name: &str) -> i32 {
        TECH_NAMES.iter().position(|n| *n == name).unwrap() as i32
    }

    #[test]
    fn a_box_brings_the_viewers_units_then_improvements_then_wonders() {
        // Egypt (civ 2) at Bronze Working: the Spearman and the Colossus,
        // not the Greeks' Hoplite or the Zulus' Impi (Conquests' screen).
        let got: Vec<&str> = brings(2, tech("Bronze Working"))
            .iter()
            .map(|p| p.name())
            .collect();
        assert_eq!(got, ["Spearman", "The Colossus"]);
    }

    #[test]
    fn the_box_size_follows_what_it_holds() {
        assert_eq!(size_for(0).0, Size::Small);
        assert_eq!(size_for(1).0, Size::Small);
        assert_eq!(size_for(3), (Size::Medium, 3));
        assert_eq!(size_for(4), (Size::Wide, 4));
        assert_eq!(size_for(5), (Size::Tall, 6));
    }

    #[test]
    fn every_advance_sits_on_one_of_the_four_pages() {
        for (t, &(era, ..)) in TECH_TREE.iter().enumerate() {
            assert!((0..4).contains(&era), "{} is on era {era}", TECH_NAMES[t]);
        }
    }
}
