//! Unit commands and the bottom action bar.
//!
//! Keys and bar buttons both emit `UnitCommand` messages; `run_commands`
//! is the single executor (found-city lives in `cities::found_city`).
//! Wake is key-only; every other command has a bar button.

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
    Explore,
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
            UnitCommand::Explore => "Explore (E)",
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
            // Civ3's Explore is a recon order; settlers and workers hold.
            UnitCommand::Explore => matches!(t, UnitType::Warrior | UnitType::Scout),
            _ => true,
        }
    }

    /// Is the command currently possible for this unit?
    pub fn enabled(self, map: &GameMap, cities: &[(i32, i32)], u: &Unit) -> bool {
        let idle = u.work.is_none();
        match self {
            UnitCommand::Wake => {
                u.fortified || u.sentry || u.exploring || u.work.is_some()
            }
            UnitCommand::Explore => idle && (u.moves > 0 || u.exploring),
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

/// Bar order, left to right within each row: Civ3's own order for these
/// actions, which is the order of its action list (`action_cell`) and the
/// order its panel draws them in. `bar_rows_follow_civ3_action_order` pins
/// that. Civ3 stacks two rows: unit-specific actions (found, worker jobs) a
/// row up, shared orders below. Wake has no button (in Civ3 it is a
/// right-click entry), so it stays key-only.
pub const BAR_ROW_MAIN: [UnitCommand; 6] = [
    UnitCommand::Skip,
    UnitCommand::Fortify,
    UnitCommand::Disband,
    UnitCommand::Goto,
    UnitCommand::Explore,
    UnitCommand::Sentry,
];
pub const BAR_ROW_UNIT: [UnitCommand; 5] = [
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
        (KeyCode::KeyE, UnitCommand::Explore),
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
                u.exploring = false;
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
                u.exploring = false;
                // Sentry strikes the same dug-in pose as fortify.
                u.anim = UnitAnim::OneShot { slot: "FORTIFY", t: 0.0 };
            }
            UnitCommand::Skip => {
                u.moves = 0;
                u.path.clear();
                u.work = None;
                u.exploring = false;
            }
            UnitCommand::Wake => {
                u.fortified = false;
                u.sentry = false;
                u.exploring = false;
                u.work = None;
                u.moves = (units::def(u.utype).moves * crate::map::MP)
                    .min(u.moves.max(crate::map::MP));
            }
            UnitCommand::Goto => {
                goto.0 = !goto.0;
                if goto.0 {
                    post(&mut board, "Go to: click a destination (Esc cancels).");
                }
            }
            UnitCommand::Explore => {
                // Toggles: a second press stands the unit down. Movement
                // plans the first leg; the readout shows the automation.
                u.exploring = !u.exploring;
                u.path.clear();
                u.fortified = false;
                u.sentry = false;
                u.work = None;
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
                u.exploring = false;
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
        UnitCommand::Explore => 5,
        // Wake has no button (in Civ3 it is a right-click entry); the cell
        // is unused but the match must stay exhaustive.
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

#[derive(Component)]
pub struct BarButton(pub UnitCommand);

/// Unit readout text inside Civ3's bottom-right box.
#[derive(Component)]
pub struct BarInfo;

/// Civ and turn line at the foot of the box.
#[derive(Component)]
pub struct BoxStatus;

/// Civ3's bottom-right box (`box right`). With no unit selected, clicking
/// it ends the turn ("Press ENTER or click here for next turn").
#[derive(Component)]
pub struct InfoBox;

/// The next-turn disc on the box's top-left knob.
#[derive(Component)]
pub struct NextTurnDisc;

/// Next-turn disc art: gold idle, orange rollover, blue highlighted.
#[derive(Resource)]
pub struct NextTurnArt(pub [Handle<Image>; 3]);

pub const BOX_W: f32 = 294.0;
pub const BOX_H: f32 = 137.0;

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
    let turn_art = [0, 1, 2].map(|i| assets.load(format!("gen/ui/nextturn_{i}.png")));
    commands.insert_resource(NextTurnArt(turn_art.clone()));
    // Civ3's bottom-right box: unit readout, or the end-turn prompt.
    // `Button` keeps map clicks from landing through it.
    commands
        .spawn((
            ImageNode::new(assets.load("gen/ui/box_right.png")),
            Button,
            InfoBox,
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(0.0),
                bottom: Val::Px(0.0),
                width: Val::Px(BOX_W),
                height: Val::Px(BOX_H),
                ..default()
            },
            GlobalZIndex(5),
        ))
        .with_children(|b| {
            b.spawn((
                Text::new(""),
                TextFont { font: font.clone(), font_size: 15.0, ..default() },
                TextColor(ink),
                TextLayout::new_with_justify(Justify::Center),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(40.0),
                    top: Val::Px(22.0),
                    width: Val::Px(220.0),
                    ..default()
                },
                BarInfo,
            ));
            b.spawn((
                Text::new(""),
                TextFont { font: font.clone(), font_size: 14.0, ..default() },
                TextColor(ink),
                TextLayout::new_with_justify(Justify::Center),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(40.0),
                    top: Val::Px(100.0),
                    width: Val::Px(220.0),
                    ..default()
                },
                BoxStatus,
            ));
            b.spawn((
                ImageNode::new(turn_art[0].clone()),
                Button,
                NextTurnDisc,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    width: Val::Px(47.0),
                    height: Val::Px(28.0),
                    ..default()
                },
            ));
        });
    // Civ3 has no bar: the discs float over the bottom of the map, packed
    // edge to edge and centered in two rows, unit-specific actions a row
    // up. Only the discs themselves block map clicks; an empty row holds
    // no space.
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                bottom: Val::Px(14.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(0.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            GlobalZIndex(5),
        ))
        .with_children(|col| {
            for cmds in [BAR_ROW_UNIT.as_slice(), BAR_ROW_MAIN.as_slice()] {
                col.spawn((Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: Val::Px(0.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },))
                .with_children(|row| {
                    for cmd in cmds {
                        row.spawn((
                            Button,
                            BarButton(*cmd),
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
            }
        });
}

/// Seconds per half blink of the end-turn prompt.
const BLINK: f32 = 0.6;

/// Civ3 blinks the next-turn disc (and its prompt) once no unit needs
/// orders; hovering the disc shows its rollover art.
pub fn blink_next_turn(
    time: Res<Time>,
    selected: Res<Selected>,
    art: Res<NextTurnArt>,
    mut disc: Query<(&Interaction, &mut ImageNode), With<NextTurnDisc>>,
) {
    let lit = (time.elapsed_secs() / BLINK) as u32 % 2 == 1;
    for (i, mut img) in disc.iter_mut() {
        let state = match i {
            Interaction::Hovered | Interaction::Pressed => 1,
            _ if selected.0.is_none() && lit => 2,
            _ => 0,
        };
        if img.image != art.0[state] {
            img.image = art.0[state].clone();
        }
    }
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
    mut info: Query<(&mut Text, &mut TextColor), With<BarInfo>>,
    mut status: Query<&mut Text, (With<BoxStatus>, Without<BarInfo>)>,
    mut out: MessageWriter<UnitCommand>,
    time: Res<Time>,
    turn: Res<units::Turn>,
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
        let active = (b.0 == UnitCommand::Goto && goto.0)
            || (b.0 == UnitCommand::Explore && u.exploring);
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
    if let Ok((mut t, mut color)) = info.single_mut() {
        let lit = (time.elapsed_secs() / BLINK) as u32 % 2 == 0;
        color.0 = if unit.is_none() && !lit {
            Color::srgba(0.23, 0.14, 0.06, 0.25)
        } else {
            Color::srgb(0.23, 0.14, 0.06)
        };
        t.0 = hint.map(str::to_string).unwrap_or_else(|| match unit {
            Some(u) => {
                let d = units::def(u.utype);
                let state = if let Some(w) = u.work {
                    format!("working ({} left)", w.turns_left)
                } else if u.sentry {
                    "sentry".to_string()
                } else if u.fortified {
                    "fortified".to_string()
                } else if u.exploring {
                    "exploring".to_string()
                } else if !u.path.is_empty() {
                    format!("going to ({},{})", u.path.back().unwrap().0, u.path.back().unwrap().1)
                } else {
                    "ready".to_string()
                };
                let terrain = match map.get(u.x, u.y) {
                    Some(t) if t.cover != Cover::Bare => format!("{:?}", t.cover),
                    Some(t) if t.relief != crate::map::Relief::Flat => format!("{:?}", t.relief),
                    Some(t) => format!("{:?}", t.base),
                    None => String::new(),
                };
                format!(
                    "{}\nMoves {}/{}  -  {}\n{terrain}",
                    d.name,
                    units::fmt_moves(u.moves),
                    d.moves,
                    state
                )
            }
            None => "Press ENTER or click here\nfor next turn".to_string(),
        });
    }
    if let Ok(mut t) = status.single_mut() {
        t.0 = format!("Japanese  -  Turn {}", turn.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Every bar button draws its own cell of the sheet: no two buttons share
    /// art, and every index is inside the grid.
    #[test]
    fn bar_buttons_map_to_distinct_cells() {
        let mut seen = HashSet::new();
        for cmd in BAR_ROW_MAIN.iter().chain(BAR_ROW_UNIT.iter()) {
            for cover in [Cover::Bare, Cover::Forest, Cover::Jungle, Cover::Pine] {
                let cell = action_cell(*cmd, cover);
                assert!(cell < BTN_COLS * BTN_ROWS, "{cmd:?} cell {cell} off sheet");
            }
            assert!(seen.insert(action_cell(*cmd, Cover::Bare)), "{cmd:?} reuses a cell");
        }
        assert_eq!(seen.len(), BAR_ROW_MAIN.len() + BAR_ROW_UNIT.len());
    }

    /// Each row lays its buttons out in Civ3's action order, so the rows read
    /// left to right the way a Civ3 unit panel does.
    #[test]
    fn bar_rows_follow_civ3_action_order() {
        for row in [BAR_ROW_MAIN.as_slice(), BAR_ROW_UNIT.as_slice()] {
            let cells: Vec<u32> =
                row.iter().map(|c| action_cell(*c, Cover::Forest)).collect();
            assert!(cells.windows(2).all(|w| w[0] < w[1]), "out of order: {cells:?}");
        }
    }

    /// Explore owns cell 5 (the `#UNIT_ACTIONS` Explore slot); Wake stays
    /// key-only, as in Civ3, so no bar button collides with it.
    #[test]
    fn explore_takes_cell_five_and_wake_has_no_button() {
        assert_eq!(action_cell(UnitCommand::Explore, Cover::Bare), 5);
        for cmd in BAR_ROW_MAIN.iter().chain(BAR_ROW_UNIT.iter()) {
            assert_ne!(*cmd, UnitCommand::Wake, "Wake must stay off the bar");
        }
    }

    /// Unit-specific actions sit a row up; shared orders stay below.
    #[test]
    fn specific_actions_go_a_row_up() {
        for cmd in BAR_ROW_UNIT {
            assert!(
                matches!(cmd, UnitCommand::FoundCity | UnitCommand::Work(_)),
                "{cmd:?} is not unit-specific"
            );
        }
        for cmd in BAR_ROW_MAIN {
            assert!(
                !matches!(cmd, UnitCommand::FoundCity | UnitCommand::Work(_)),
                "{cmd:?} belongs a row up"
            );
        }
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
