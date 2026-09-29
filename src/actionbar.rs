//! Unit commands and the bottom action bar.
//!
//! Keys and bar buttons both emit `UnitCommand` messages; `run_commands`
//! is the single executor (found-city lives in `cities::found_city`).

use bevy::prelude::*;

use crate::audio::{self, GameAudio};
use crate::cities::{can_found, City};
use crate::features::{post, MessageBoard};
use crate::improvements::{
    action_slot, can_clear, can_irrigate, can_mine, can_road, work_turns, WorkAction,
};
use crate::map::{Cover, GameMap};
use crate::units::{self, Selected, Unit, UnitAnim, UnitType};

#[derive(Message, Clone, Copy, PartialEq, Eq, Debug)]
pub enum UnitCommand {
    Fortify,
    Sentry,
    Skip,
    Wake,
    Goto,
    Disband,
    FoundCity,
    Work(WorkAction),
}

/// Next left click on the map sends the selected unit there.
#[derive(Resource, Default)]
pub struct GotoMode(pub bool);

impl UnitCommand {
    pub fn label(self) -> &'static str {
        match self {
            UnitCommand::Fortify => "Fortify (F)",
            UnitCommand::Sentry => "Sentry (Q)",
            UnitCommand::Skip => "Skip (Space)",
            UnitCommand::Wake => "Wake (X)",
            UnitCommand::Goto => "Go to (G)",
            UnitCommand::Disband => "Disband (Del)",
            UnitCommand::FoundCity => "Found City (B)",
            UnitCommand::Work(WorkAction::Road) => "Road (R)",
            UnitCommand::Work(WorkAction::Irrigate) => "Irrigate (I)",
            UnitCommand::Work(WorkAction::Mine) => "Mine (M)",
            UnitCommand::Work(WorkAction::Clear) => "Clear (C)",
        }
    }

    /// Does this command apply to the unit type at all (button shown)?
    pub fn relevant(self, t: UnitType) -> bool {
        match self {
            UnitCommand::FoundCity => t == UnitType::Settler,
            UnitCommand::Work(_) => t == UnitType::Worker,
            UnitCommand::Fortify => t != UnitType::Scout,
            _ => true,
        }
    }

    /// Is the command currently possible for this unit?
    pub fn enabled(self, map: &GameMap, cities: &[(i32, i32)], u: &Unit) -> bool {
        let idle = u.work.is_none();
        match self {
            UnitCommand::Wake => u.fortified || u.sentry || u.work.is_some(),
            UnitCommand::Fortify | UnitCommand::Sentry => u.moves > 0 && !u.fortified,
            UnitCommand::Skip | UnitCommand::Goto => u.moves > 0 || !u.path.is_empty(),
            UnitCommand::Disband => true,
            UnitCommand::FoundCity => can_found(map, cities, u.x, u.y),
            UnitCommand::Work(a) => {
                u.moves > 0
                    && idle
                    && match a {
                        WorkAction::Road => can_road(map, u.x, u.y),
                        WorkAction::Irrigate => can_irrigate(map, u.x, u.y),
                        WorkAction::Mine => can_mine(map, u.x, u.y),
                        WorkAction::Clear => can_clear(map, u.x, u.y),
                    }
            }
        }
    }
}

/// Bar order, left to right: Civ3's own order for these actions, which is the
/// order of its action list (`action_cell`) and the order its panel draws them
/// in. `bar_commands_follow_civ3_action_order` pins that.
pub const BAR_COMMANDS: [UnitCommand; 11] = [
    UnitCommand::Skip,
    UnitCommand::Fortify,
    UnitCommand::Disband,
    UnitCommand::Goto,
    UnitCommand::Wake,
    UnitCommand::Sentry,
    UnitCommand::FoundCity,
    UnitCommand::Work(WorkAction::Road),
    UnitCommand::Work(WorkAction::Mine),
    UnitCommand::Work(WorkAction::Irrigate),
    UnitCommand::Work(WorkAction::Clear),
];

/// Key bindings shared with the bar labels.
pub fn key_commands(keys: &ButtonInput<KeyCode>) -> Vec<UnitCommand> {
    let table = [
        (KeyCode::KeyF, UnitCommand::Fortify),
        (KeyCode::KeyQ, UnitCommand::Sentry),
        (KeyCode::Space, UnitCommand::Skip),
        (KeyCode::KeyX, UnitCommand::Wake),
        (KeyCode::KeyG, UnitCommand::Goto),
        (KeyCode::Delete, UnitCommand::Disband),
        (KeyCode::Backspace, UnitCommand::Disband),
        (KeyCode::KeyB, UnitCommand::FoundCity),
        (KeyCode::KeyR, UnitCommand::Work(WorkAction::Road)),
        (KeyCode::KeyI, UnitCommand::Work(WorkAction::Irrigate)),
        (KeyCode::KeyM, UnitCommand::Work(WorkAction::Mine)),
        (KeyCode::KeyC, UnitCommand::Work(WorkAction::Clear)),
    ];
    table
        .iter()
        .filter(|(k, _)| keys.just_pressed(*k))
        .map(|(_, c)| *c)
        .collect()
}

pub fn run_commands(
    mut commands: Commands,
    mut cmds: MessageReader<UnitCommand>,
    selected: Res<Selected>,
    mut units: Query<(Entity, &mut Unit)>,
    cities: Query<&City>,
    map: Res<GameMap>,
    audio: Res<GameAudio>,
    mut goto: ResMut<GotoMode>,
    mut board: ResMut<MessageBoard>,
) {
    let spots: Vec<(i32, i32)> = cities.iter().map(|c| (c.x, c.y)).collect();
    for cmd in cmds.read().copied() {
        let Some(s) = selected.0 else {
            continue;
        };
        let Ok((e, mut u)) = units.get_mut(s) else {
            continue;
        };
        if !cmd.relevant(u.utype) {
            if let UnitCommand::Work(_) = cmd {
                post(&mut board, "Only Workers can build improvements.");
            }
            continue;
        }
        if cmd != UnitCommand::Goto {
            goto.0 = false;
        }
        if !cmd.enabled(&map, &spots, &u) {
            if let UnitCommand::Work(a) = cmd {
                post(
                    &mut board,
                    match a {
                        WorkAction::Road => "A road cannot be built here.",
                        WorkAction::Irrigate => "Irrigation needs fresh water or a chain.",
                        WorkAction::Mine => "Mines need hills, mountains or desert.",
                        WorkAction::Clear => "Nothing to clear here.",
                    },
                );
            } else if cmd == UnitCommand::FoundCity {
                post(&mut board, "Cities need open land, two tiles from another city.");
            }
            continue;
        }
        match cmd {
            UnitCommand::Fortify => {
                let warrior = u.utype == UnitType::Warrior;
                u.fortified = true;
                u.sentry = false;
                u.moves = 0;
                u.path.clear();
                u.work = None;
                u.anim = UnitAnim::OneShot { slot: "FORTIFY", t: 0.0 };
                if warrior {
                    commands.spawn(AudioPlayer(audio.fortify.clone()));
                }
            }
            UnitCommand::Sentry => {
                u.fortified = true;
                u.sentry = true;
                u.moves = 0;
                u.path.clear();
                u.work = None;
            }
            UnitCommand::Skip => {
                u.moves = 0;
                u.path.clear();
                u.work = None;
            }
            UnitCommand::Wake => {
                u.fortified = false;
                u.sentry = false;
                u.work = None;
                u.moves = units::def(u.utype).moves.min(u.moves.max(1));
            }
            UnitCommand::Goto => {
                goto.0 = !goto.0;
                if goto.0 {
                    post(&mut board, "Go to: click a destination (Esc cancels).");
                }
            }
            UnitCommand::Disband => {
                commands.entity(e).despawn();
                post(&mut board, "Unit disbanded.");
            }
            UnitCommand::FoundCity => {} // handled in cities::found_city
            UnitCommand::Work(action) => {
                u.work = Some(crate::improvements::Work {
                    action,
                    turns_left: work_turns(action),
                });
                u.moves = 0;
                u.path.clear();
                u.fortified = false;
                u.sentry = false;
                u.anim = UnitAnim::OneShot { slot: action_slot(action), t: 0.0 };
                commands.spawn(AudioPlayer(audio.work_sfx(action)));
                post(
                    &mut board,
                    match action {
                        WorkAction::Road => "Building road...",
                        WorkAction::Irrigate => "Building irrigation...",
                        WorkAction::Mine => "Digging mine...",
                        WorkAction::Clear => "Clearing land...",
                    },
                );
            }
        }
        let _ = audio::sfx; // silence unused-import churn when SFX list changes
    }
}

// --- bar widgets ---

/// Civ3 draws its unit action buttons as 32-px cells of an 8x10 grid; the
/// row-major cell order is the `#UNIT_ACTIONS` order of
/// `Conquests/Text/labels.txt` (verified against in-game screenshots: a
/// Warrior's seven buttons are cells 0-6). Sheets and the disc alpha come from
/// `Conquests/Art/interface/*.PCX` via `tools/prep_assets.py unitbuttons`.
const BTN_PX: f32 = 32.0;
const BTN_COLS: u32 = 8;
const BTN_ROWS: u32 = 10;

/// Cell index of a command. `cover` selects between Civ3's two clearing
/// buttons, forest (27) and wetlands (28, the jungle art here).
fn action_cell(cmd: UnitCommand, cover: Cover) -> u32 {
    match cmd {
        UnitCommand::Skip => 0,
        UnitCommand::Fortify => 2,
        UnitCommand::Disband => 3,
        UnitCommand::Goto => 4,
        // Civ3 has no Wake button on the map panel (Wake is a right-click
        // entry). Reuse the circular-arrow art, Civ3's Explore, unused here.
        UnitCommand::Wake => 5,
        UnitCommand::Sentry => 6,
        UnitCommand::FoundCity => 21,
        UnitCommand::Work(WorkAction::Road) => 22,
        UnitCommand::Work(WorkAction::Mine) => 25,
        UnitCommand::Work(WorkAction::Irrigate) => 26,
        UnitCommand::Work(WorkAction::Clear) => match cover {
            Cover::Jungle => 28,
            _ => 27,
        },
    }
}

/// Button art per interaction state: idle, hover, held or toggled on.
#[derive(Resource, Clone)]
pub(crate) struct ButtonArt {
    sheets: [Handle<Image>; 3],
}

/// Terrain under a unit; only the clearing art depends on it.
fn cover_at(map: &GameMap, u: &Unit) -> Cover {
    map.get(u.x, u.y).map(|t| t.cover).unwrap_or(Cover::Bare)
}

pub const BAR_H: f32 = 84.0;

#[derive(Component)]
pub struct BarButton(pub UnitCommand);

#[derive(Component)]
pub struct BarInfo;

pub fn spawn_bar(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let font = assets.load("gen/fonts/lsans.ttf");
    let ink = Color::srgb(0.23, 0.14, 0.06);
    let layout = layouts.add(TextureAtlasLayout::from_grid(
        UVec2::splat(BTN_PX as u32),
        BTN_COLS,
        BTN_ROWS,
        None,
        None,
    ));
    let sheets = ["norm", "over", "down"].map(|s| assets.load(format!("gen/ui/unitbtns_{s}.png")));
    commands.insert_resource(ButtonArt { sheets: sheets.clone() });
    commands
        .spawn((
            ImageNode::new(assets.load("gen/cityscreen/BottomFadeBar.png")),
            // `Button` marks the whole strip as UI: the map ignores clicks on
            // it (and on the gaps between discs) the way Civ3's panel does.
            Button,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                bottom: Val::Px(0.0),
                height: Val::Px(BAR_H),
                padding: UiRect::axes(Val::Px(10.0), Val::Px(8.0)),
                column_gap: Val::Px(6.0),
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgb(0.93, 0.83, 0.62)),
            GlobalZIndex(5),
        ))
        .with_children(|bar| {
            bar.spawn((
                Text::new(""),
                TextFont { font: font.clone(), font_size: 16.0, ..default() },
                TextColor(ink),
                Node { width: Val::Px(190.0), ..default() },
                BarInfo,
            ));
            // Civ3 packs the discs edge to edge and centers the row in the
            // panel right of the unit readout.
            bar.spawn(Node {
                flex_grow: 1.0,
                column_gap: Val::Px(0.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|row| {
                for cmd in BAR_COMMANDS {
                    row.spawn((
                        Button,
                        BarButton(cmd),
                        ImageNode::from_atlas_image(
                            sheets[0].clone(),
                            TextureAtlas { layout: layout.clone(), index: 0 },
                        ),
                        Node {
                            width: Val::Px(BTN_PX),
                            height: Val::Px(BTN_PX),
                            ..default()
                        },
                    ));
                }
            });
        });
}

/// Show only applicable buttons, dim unavailable ones, swap art on hover or
/// while armed, name the hovered command in the unit readout, and emit
/// commands on press.
pub fn update_bar(
    art: Res<ButtonArt>,
    selected: Res<Selected>,
    units: Query<&Unit>,
    cities: Query<&City>,
    map: Res<GameMap>,
    goto: Res<GotoMode>,
    mut buttons: Query<(&BarButton, Ref<Interaction>, &mut Node, &mut ImageNode)>,
    mut info: Query<&mut Text, With<BarInfo>>,
    mut out: MessageWriter<UnitCommand>,
) {
    let spots: Vec<(i32, i32)> = cities.iter().map(|c| (c.x, c.y)).collect();
    let unit = selected.0.and_then(|s| units.get(s).ok());
    let mut hint = None;
    for (b, interaction, mut node, mut img) in buttons.iter_mut() {
        let Some(u) = unit else {
            node.display = Display::None;
            continue;
        };
        node.display = if b.0.relevant(u.utype) {
            Display::Flex
        } else {
            Display::None
        };
        let ok = b.0.enabled(&map, &spots, u);
        let active = b.0 == UnitCommand::Goto && goto.0;
        // Civ3's states: gold idle, orange hover, blue held or toggled on.
        let state = match (*interaction, ok) {
            (Interaction::Pressed, true) => 2,
            (Interaction::Hovered, true) => 1,
            _ if active => 2,
            _ => 0,
        };
        img.image = art.sheets[state].clone();
        if let Some(atlas) = img.texture_atlas.as_mut() {
            atlas.index = action_cell(b.0, cover_at(&map, u)) as usize;
        }
        // Civ3's greyed look darkens the disc instead of fading it out.
        img.color = if ok { Color::WHITE } else { Color::srgb(0.62, 0.58, 0.52) };
        if ok && !matches!(*interaction, Interaction::None) {
            hint = Some(b.0.label());
        }
        if interaction.is_changed() && *interaction == Interaction::Pressed && ok {
            out.write(b.0);
        }
    }
    if let Ok(mut t) = info.single_mut() {
        t.0 = hint.map(str::to_string).unwrap_or_else(|| match unit {
            Some(u) => {
                let d = units::def(u.utype);
                let state = if let Some(w) = u.work {
                    format!("working ({} left)", w.turns_left)
                } else if u.sentry {
                    "sentry".to_string()
                } else if u.fortified {
                    "fortified".to_string()
                } else if !u.path.is_empty() {
                    format!("going to ({},{})", u.path.back().unwrap().0, u.path.back().unwrap().1)
                } else {
                    "ready".to_string()
                };
                format!("{}  {}/{} MP\n{}", d.name, u.moves, d.moves, state)
            }
            None => "No unit selected".to_string(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Every bar command draws its own cell of the sheet: no two buttons share
    /// art, and every index is inside the grid.
    #[test]
    fn bar_commands_map_to_distinct_cells() {
        let mut seen = HashSet::new();
        for cmd in BAR_COMMANDS {
            for cover in [Cover::Bare, Cover::Forest, Cover::Jungle, Cover::Pine] {
                let cell = action_cell(cmd, cover);
                assert!(cell < BTN_COLS * BTN_ROWS, "{cmd:?} cell {cell} off sheet");
            }
            assert!(seen.insert(action_cell(cmd, Cover::Bare)), "{cmd:?} reuses a cell");
        }
        assert_eq!(seen.len(), BAR_COMMANDS.len());
    }

    /// The bar lays its buttons out in Civ3's action order, so the row reads
    /// left to right the way a Civ3 unit panel does.
    #[test]
    fn bar_commands_follow_civ3_action_order() {
        let cells: Vec<u32> = BAR_COMMANDS
            .iter()
            .map(|c| action_cell(*c, Cover::Forest))
            .collect();
        assert!(cells.windows(2).all(|w| w[0] < w[1]), "out of order: {cells:?}");
    }

    /// Civ3's clearing art depends on the tile: jungle uses the wetlands cell.
    #[test]
    fn clear_art_follows_cover() {
        let clear = UnitCommand::Work(WorkAction::Clear);
        assert_eq!(action_cell(clear, Cover::Jungle), 28);
        assert_eq!(action_cell(clear, Cover::Forest), action_cell(clear, Cover::Pine));
        assert_ne!(action_cell(clear, Cover::Jungle), action_cell(clear, Cover::Forest));
    }
}
