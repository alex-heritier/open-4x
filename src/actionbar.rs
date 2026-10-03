//! Unit commands and the bottom action bar.
//!
//! Keys and bar buttons both emit `UnitCommand` messages; `run_commands`
//! is the single executor (found-city lives in `cities::found_city`).
//! Wake is key-only; every other command has a bar button.

use bevy::prelude::*;

use crate::audio::{self, GameAudio};
use crate::cities::{City, Treasury, can_found};
use crate::civs::{CIVS, Civilizations};
use crate::economy;
use crate::features::{MessageBoard, post};
use crate::improvements::{
    WorkAction, action_slot, can_clear, can_irrigate, can_mine, can_road, work_turns,
};
use crate::map::{Cover, GameMap};
use crate::units::{self, Selected, Unit, UnitAnim, UnitType};

#[derive(Message, Clone, Copy, PartialEq, Eq, Debug)]
pub enum UnitCommand {
    Bombard,
    Load,
    Unload,
    Fortify,
    Sentry,
    Skip,
    Wake,
    Goto,
    Explore,
    Disband,
    FoundCity,
    Work(WorkAction),
    /// Gold upgrade of this unit (`U`).
    Upgrade,
    /// Gold upgrade of every unit of its type (`Shift+U`).
    UpgradeAll,
    /// A Settler or Worker adds its citizens to the city it stands in (`J`).
    JoinCity,
    /// Tear an improvement off the tile (`Shift+P`).
    Pillage,
    /// A Worker chooses its own jobs until told otherwise (`Z`).
    Automate,
    /// A Great Leader in a city forms an Army (`B`).
    BuildArmy,
    /// A Great Leader in a city completes its build (`H`).
    LeaderHurry,
}

/// Next left click on the map sends the selected unit there.
#[derive(Resource, Default)]
pub struct GotoMode(pub bool);

impl UnitCommand {
    pub fn label(self) -> &'static str {
        match self {
            UnitCommand::Bombard => "Bombard (B)",
            UnitCommand::Load => "Load (L)",
            UnitCommand::Unload => "Unload (L)",
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
            UnitCommand::Upgrade => "Upgrade (U)",
            UnitCommand::UpgradeAll => "Upgrade all (Shift+U)",
            UnitCommand::JoinCity => "Join City (J)",
            UnitCommand::Pillage => "Pillage (Shift+P)",
            UnitCommand::Automate => "Automate (Z)",
            UnitCommand::BuildArmy => "Build army (B)",
            UnitCommand::LeaderHurry => "Hurry city production (H)",
        }
    }

    /// Does this command apply to the unit type at all (button shown)?
    pub fn relevant(self, t: UnitType) -> bool {
        match self {
            UnitCommand::Load => units::def(t).class == 0 && units::def(t).special & 1 != 0,
            UnitCommand::BuildArmy | UnitCommand::LeaderHurry => t == UnitType::Leader,
            UnitCommand::Unload => units::def(t).capacity > 0 && units::def(t).special & 2 != 0,
            UnitCommand::Bombard => crate::bombard::capable(t),
            UnitCommand::FoundCity => t == UnitType::Settler,
            UnitCommand::Work(_) => t == UnitType::Worker,
            UnitCommand::Fortify => t != UnitType::Scout,
            // Only a type with a successor and the ability has the button.
            UnitCommand::Upgrade => crate::upgrades::upgradable(t),
            UnitCommand::UpgradeAll => crate::upgrades::upgradable(t),
            UnitCommand::JoinCity => crate::actions::join_pop(t).is_some(),
            UnitCommand::Pillage => crate::actions::can_pillage_type(t),
            UnitCommand::Automate => crate::actions::can_automate(t),
            // Civ3's Explore is a recon order; settlers and workers hold.
            UnitCommand::Explore => {
                matches!(t, UnitType::Warrior | UnitType::Scout | UnitType::Horseman | UnitType::Galley)
            }
            _ => true,
        }
    }

    /// Is the command currently possible for this unit?
    pub fn enabled(self, map: &GameMap, cities: &[(i32, i32)], u: &Unit) -> bool {
        if u.carrier.is_some() && !matches!(self, UnitCommand::Goto | UnitCommand::Skip | UnitCommand::Disband) {
            return false;
        }
        let idle = u.work.is_none();
        match self {
            UnitCommand::Load => u.moves > 0 && u.carrier.is_none(),
            UnitCommand::Unload => u.moves > 0 && map.get(u.x, u.y).is_some_and(|t| !crate::improvements::is_water_base(t.base)),
            UnitCommand::Bombard => idle && u.moves > 0 && !u.attacked,
            UnitCommand::Wake => u.fortified || u.sentry || u.exploring || u.work.is_some(),
            UnitCommand::Explore => idle && (u.moves > 0 || u.exploring),
            UnitCommand::Fortify | UnitCommand::Sentry => u.moves > 0 && !u.fortified,
            UnitCommand::Skip | UnitCommand::Goto => u.moves > 0 || !u.path.is_empty(),
            UnitCommand::Disband => true,
            // Needs the city and the treasury: the caller asks `upgrades`.
            UnitCommand::Upgrade | UnitCommand::UpgradeAll => u.moves > 0,
            // The city and the tile decide: `actions::check`.
            UnitCommand::JoinCity | UnitCommand::Pillage => u.moves > 0,
            UnitCommand::Automate => u.auto || u.moves > 0,
            UnitCommand::BuildArmy | UnitCommand::LeaderHurry => u.moves > 0,
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
pub const BAR_ROW_MAIN: [UnitCommand; 10] = [
    UnitCommand::Skip,
    UnitCommand::Fortify,
    UnitCommand::Disband,
    UnitCommand::Goto,
    UnitCommand::Explore,
    UnitCommand::Sentry,
    UnitCommand::Load,
    UnitCommand::Unload,
    UnitCommand::Pillage,
    UnitCommand::Bombard,
];
pub const BAR_ROW_UNIT: [UnitCommand; 10] = [
    UnitCommand::BuildArmy,
    UnitCommand::LeaderHurry,
    UnitCommand::Upgrade,
    UnitCommand::FoundCity,
    UnitCommand::Work(WorkAction::Road),
    UnitCommand::Work(WorkAction::Mine),
    UnitCommand::Work(WorkAction::Irrigate),
    UnitCommand::Work(WorkAction::Clear),
    UnitCommand::Automate,
    UnitCommand::JoinCity,
];

/// Key bindings shared with the bar labels.
pub fn key_commands(keys: &ButtonInput<KeyCode>) -> Vec<UnitCommand> {
    let table = [
        (KeyCode::KeyL, UnitCommand::Load),
        (KeyCode::KeyL, UnitCommand::Unload),
        (KeyCode::KeyF, UnitCommand::Fortify),
        (KeyCode::KeyQ, UnitCommand::Sentry),
        (KeyCode::Space, UnitCommand::Skip),
        (KeyCode::KeyX, UnitCommand::Wake),
        (KeyCode::KeyG, UnitCommand::Goto),
        (KeyCode::KeyE, UnitCommand::Explore),
        (KeyCode::Delete, UnitCommand::Disband),
        (KeyCode::Backspace, UnitCommand::Disband),
        (KeyCode::KeyB, UnitCommand::FoundCity),
        (KeyCode::KeyB, UnitCommand::Bombard),
        (KeyCode::KeyB, UnitCommand::BuildArmy),
        (KeyCode::KeyH, UnitCommand::LeaderHurry),
        (KeyCode::KeyU, UnitCommand::Upgrade),
        (KeyCode::KeyJ, UnitCommand::JoinCity),
        (KeyCode::KeyP, UnitCommand::Pillage),
        (KeyCode::KeyZ, UnitCommand::Automate),
        (KeyCode::KeyR, UnitCommand::Work(WorkAction::Road)),
        (KeyCode::KeyI, UnitCommand::Work(WorkAction::Irrigate)),
        (KeyCode::KeyM, UnitCommand::Work(WorkAction::Mine)),
        (KeyCode::KeyC, UnitCommand::Work(WorkAction::Clear)),
    ];
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    table
        .iter()
        .filter(|(k, c)| keys.just_pressed(*k) && (*c != UnitCommand::Pillage || shift))
        // Shift+U is Upgrade All, not Upgrade.
        .map(|(_, c)| match c {
            UnitCommand::Upgrade if shift => UnitCommand::UpgradeAll,
            c => *c,
        })
        .collect()
}

pub fn run_commands(
    mut commands: Commands,
    mut cmds: MessageReader<UnitCommand>,
    selected: Res<Selected>,
    mut units: Query<(Entity, &mut Unit)>,
    civs: Res<Civilizations>,
    cities: Query<&City>,
    map: Res<GameMap>,
    audio: Res<GameAudio>,
    mut goto: ResMut<GotoMode>,
    mut board: ResMut<MessageBoard>,
    mut treasury: ResMut<Treasury>,
) {
    let spots: Vec<(i32, i32)> = cities.iter().map(|c| (c.x, c.y)).collect();
    for cmd in cmds.read().copied() {
        let Some(s) = selected.0 else {
            continue;
        };
        // Upgrades need the cities and the treasury as well as the unit.
        if matches!(cmd, UnitCommand::Upgrade | UnitCommand::UpgradeAll) {
            let mine = units
                .get(s)
                .is_ok_and(|(_, u)| u.civ == civs.active && u.carrier.is_none() && cmd.relevant(u.utype));
            if mine {
                goto.0 = false;
                if cmd == UnitCommand::Upgrade {
                    crate::upgrades::run_one(s, &mut units, &cities, &mut treasury, &mut board);
                } else {
                    crate::upgrades::run_all(s, &mut units, &cities, &mut treasury, &mut board);
                }
            }
            continue;
        }
        let Ok((e, mut u)) = units.get_mut(s) else {
            continue;
        };
        // Hotseat: only the chair in play gives orders. A stale selection
        // left from the previous civilization is ignored, so a queued key
        // can never move the other side's unit.
        if u.civ != civs.active {
            continue;
        }
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
                post(
                    &mut board,
                    "Cities need open land, two tiles from another city.",
                );
            }
            continue;
        }
        match cmd {
            UnitCommand::Load | UnitCommand::Unload => {} // `naval::cargo_commands`, `army`
            UnitCommand::BuildArmy | UnitCommand::LeaderHurry => {} // `army::commands`
            UnitCommand::Bombard => {} // `bombard::arm`
            UnitCommand::Fortify => {
                let warrior = u.utype == UnitType::Warrior;
                u.fortified = true;
                u.sentry = false;
                u.moves = 0;
                u.path.clear();
                u.work = None;
                u.exploring = false;
                u.anim = UnitAnim::OneShot {
                    slot: "FORTIFY",
                    t: 0.0,
                };
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
                u.anim = UnitAnim::OneShot {
                    slot: "FORTIFY",
                    t: 0.0,
                };
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
                u.moves =
                    (units::def(u.utype).moves * crate::map::MP).min(u.moves.max(crate::map::MP));
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
            UnitCommand::Upgrade | UnitCommand::UpgradeAll => {} // `upgrades`, above
            UnitCommand::JoinCity | UnitCommand::Pillage | UnitCommand::Automate => {} // `actions::run`
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
                u.anim = UnitAnim::OneShot {
                    slot: action_slot(action),
                    t: 0.0,
                };
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
        UnitCommand::Load => 7,
        UnitCommand::Unload => 8,
        UnitCommand::Bombard => 11,
        UnitCommand::Skip => 0,
        UnitCommand::Fortify => 2,
        UnitCommand::Disband => 3,
        UnitCommand::Goto => 4,
        UnitCommand::Explore => 5,
        // Wake has no button (in Civ3 it is a right-click entry); the cell
        // is unused but the match must stay exhaustive.
        UnitCommand::Wake => 5,
        UnitCommand::Sentry => 6,
        // "(U)pgrade unit" is the 16th label of `#UNIT_ACTIONS`.
        UnitCommand::Upgrade | UnitCommand::UpgradeAll => 15,
        // "(P)illage Improvement", "(A)utomate" and "Join City" are the 11th,
        // 32nd and 33rd labels.
        UnitCommand::Pillage => 10,
        UnitCommand::Automate => 31,
        UnitCommand::JoinCity => 32,
        // "(B)uild army" and "(H)urry city production", the 14th and 15th.
        UnitCommand::BuildArmy => 13,
        UnitCommand::LeaderHurry => 14,
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

/// Treasury and its change per turn, between the readout and the status.
#[derive(Component)]
pub struct BoxGold;

/// The research target and its beakers, under the status line.
#[derive(Component)]
pub struct BoxScience;

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
    commands.insert_resource(ButtonArt {
        sheets: sheets.clone(),
    });
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
                TextFont {
                    font: font.clone(),
                    font_size: 15.0,
                    ..default()
                },
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
                TextFont {
                    font: font.clone(),
                    font_size: 14.0,
                    ..default()
                },
                TextColor(ink),
                TextLayout::new_with_justify(Justify::Center),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(40.0),
                    top: Val::Px(80.0),
                    width: Val::Px(220.0),
                    ..default()
                },
                BoxGold,
            ));
            b.spawn((
                Text::new(""),
                TextFont {
                    font: font.clone(),
                    font_size: 14.0,
                    ..default()
                },
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
                Text::new(""),
                TextFont {
                    font: font.clone(),
                    font_size: 13.0,
                    ..default()
                },
                TextColor(ink),
                TextLayout::new_with_justify(Justify::Center),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(40.0),
                    top: Val::Px(116.0),
                    width: Val::Px(220.0),
                    ..default()
                },
                BoxScience,
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
                                    TextureAtlas {
                                        layout: layout.clone(),
                                        index: 0,
                                    },
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
    treasury: Res<Treasury>,
    map: Res<GameMap>,
    goto: Res<GotoMode>,
    bombard: Res<crate::bombard::TargetMode>,
    mut buttons: Query<(&BarButton, Ref<Interaction>, &mut Node, &mut ImageNode)>,
    mut info: Query<(&mut Text, &mut TextColor), With<BarInfo>>,
    mut status: Query<&mut Text, (With<BoxStatus>, Without<BarInfo>)>,
    mut out: MessageWriter<UnitCommand>,
    time: Res<Time>,
    turn: Res<units::Turn>,
    civs: Res<Civilizations>,
) {
    let spots: Vec<(i32, i32)> = cities.iter().map(|c| (c.x, c.y)).collect();
    // The bar belongs to the chair in play: another civilization's unit
    // (a stale selection) gets no buttons and the end-turn prompt.
    let unit = selected
        .0
        .and_then(|s| units.get(s).ok())
        .filter(|u| u.civ == civs.active);
    // What an upgrade would do here: shown in a city with the facility,
    // lit when the unit has movement left and the gold is there.
    let here = unit.and_then(|u| cities.iter().find(|c| (c.x, c.y) == (u.x, u.y)));
    let plan = unit.and_then(|u| crate::upgrades::plan(u, here));
    let offer = unit.and_then(|u| crate::upgrades::offer(u, here, treasury.0[u.civ]));
    let mut hint: Option<String> = None;
    for (b, interaction, mut node, mut img) in buttons.iter_mut() {
        let Some(u) = unit else {
            node.display = Display::None;
            continue;
        };
        node.display = if b.0.relevant(u.utype) && (b.0 != UnitCommand::Upgrade || plan.is_some()) {
            Display::Flex
        } else {
            Display::None
        };
        let ok = if b.0 == UnitCommand::Upgrade {
            offer.is_some()
        } else {
            crate::actions::check(b.0, &map, u, here).unwrap_or_else(|| b.0.enabled(&map, &spots, u))
        };
        let active =
            (b.0 == UnitCommand::Goto && goto.0) || (b.0 == UnitCommand::Explore && u.exploring)
            || (b.0 == UnitCommand::Bombard && bombard.0 == selected.0);
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
        img.color = if ok {
            Color::WHITE
        } else {
            Color::srgb(0.62, 0.58, 0.52)
        };
        if !matches!(*interaction, Interaction::None) && b.0 == UnitCommand::Upgrade {
            hint = plan.map(|p| format!("Upgrade to {} for {} gold (U)", units::def(p.to).name, p.cost));
        } else if ok && !matches!(*interaction, Interaction::None) {
            hint = Some(b.0.label().to_string());
        }
        if interaction.is_changed() && *interaction == Interaction::Pressed && ok {
            out.write(b.0);
        }
    }
    if let Ok((mut t, mut color)) = info.single_mut() {
        let lit = (time.elapsed_secs() / BLINK) as u32 % 2 == 0;
        color.0 = if unit.is_none() && !lit && !crate::civs::is_ai(civs.active) {
            Color::srgba(0.23, 0.14, 0.06, 0.25)
        } else {
            Color::srgb(0.23, 0.14, 0.06)
        };
        t.0 = hint.unwrap_or_else(|| match unit {
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
                    format!(
                        "going to ({},{})",
                        u.path.back().unwrap().0,
                        u.path.back().unwrap().1
                    )
                } else {
                    "ready".to_string()
                };
                let terrain = match map.get(u.x, u.y) {
                    Some(t) if t.cover != Cover::Bare => format!("{:?}", t.cover),
                    Some(t) if t.relief != crate::map::Relief::Flat => format!("{:?}", t.relief),
                    Some(t) => format!("{:?}", t.base),
                    None => String::new(),
                };
                // Soldiers show their rank and hit points, as in Civ3.
                let title = if d.attack > 0 {
                    format!("{} ({})  HP {}/{}", d.name, u.level.name(), u.hp(), u.max_hp())
                } else {
                    d.name.to_string()
                };
                format!(
                    "{}\nMoves {}/{}  -  {}\n{terrain}",
                    title,
                    units::fmt_moves(u.moves),
                    units::fmt_moves(crate::naval::moves(u.utype, u.civ)),
                    state
                )
            }
            None if crate::civs::is_ai(civs.active) => {
                format!("{} is\nmaking its move...", CIVS[civs.active].name)
            }
            None => "Press ENTER or click here\nfor next turn".to_string(),
        });
    }
    if let Ok(mut t) = status.single_mut() {
        t.0 = if civs.active == civs.viewer() {
            format!("{}  -  {}", CIVS[civs.active].adjective, crate::calendar::label(turn.0))
        } else {
            format!(
                "{}  -  {}  -  {} moving",
                CIVS[civs.viewer()].adjective,
                crate::calendar::label(turn.0),
                CIVS[civs.active].name
            )
        };
    }
}

/// The treasury line: gold on hand and what the next turn changes it by.
fn gold_line(treasury: u32, net: i32) -> String {
    format!("{treasury} Gold ({net:+} per turn)")
}

/// Keep the box's treasury line current: the active civ's gold and the
/// change its coming turn brings. The change is `economy::finance`, the
/// same books the turn end settles, so what is shown is what is applied.
/// A deficit shows in red.
pub fn update_gold(
    civs: Res<Civilizations>,
    treasury: Res<Treasury>,
    map: Res<GameMap>,
    cities: Query<&City>,
    units: Query<&Unit>,
    mut line: Query<(&mut Text, &mut TextColor), With<BoxGold>>,
) {
    let Ok((mut text, mut color)) = line.single_mut() else {
        return;
    };
    let civ = civs.viewer();
    let net = economy::finance(
        &map,
        cities.iter().filter(|c| c.civ == civ),
        units.iter().filter(|u| u.civ == civ).count(),
    )
    .net();
    let shown = gold_line(treasury.0[civ], net);
    if text.0 != shown {
        text.0 = shown;
    }
    let ink = if net < 0 {
        Color::srgb(0.6, 0.1, 0.08)
    } else {
        Color::srgb(0.23, 0.14, 0.06)
    };
    if color.0 != ink {
        color.0 = ink;
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
            assert!(
                seen.insert(action_cell(*cmd, Cover::Bare)),
                "{cmd:?} reuses a cell"
            );
        }
        assert_eq!(seen.len(), BAR_ROW_MAIN.len() + BAR_ROW_UNIT.len());
    }

    /// Each row lays its buttons out in Civ3's action order, so the rows read
    /// left to right the way a Civ3 unit panel does.
    #[test]
    fn bar_rows_follow_civ3_action_order() {
        for row in [BAR_ROW_MAIN.as_slice(), BAR_ROW_UNIT.as_slice()] {
            let cells: Vec<u32> = row.iter().map(|c| action_cell(*c, Cover::Forest)).collect();
            assert!(
                cells.windows(2).all(|w| w[0] < w[1]),
                "out of order: {cells:?}"
            );
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
                matches!(
                    cmd,
                    UnitCommand::Upgrade
                        | UnitCommand::BuildArmy
                        | UnitCommand::LeaderHurry
                        | UnitCommand::FoundCity
                        | UnitCommand::Work(_)
                        | UnitCommand::Automate
                        | UnitCommand::JoinCity
                ),
                "{cmd:?} is not unit-specific"
            );
        }
        for cmd in BAR_ROW_MAIN {
            assert!(
                !matches!(
                    cmd,
                    UnitCommand::Upgrade
                        | UnitCommand::FoundCity
                        | UnitCommand::Work(_)
                        | UnitCommand::Automate
                        | UnitCommand::JoinCity
                ),
                "{cmd:?} belongs a row up"
            );
        }
    }

    /// Civ3's clearing art depends on the tile: jungle uses the wetlands cell.
    #[test]
    fn clear_art_follows_cover() {
        let clear = UnitCommand::Work(WorkAction::Clear);
        assert_eq!(action_cell(clear, Cover::Jungle), 28);
        assert_eq!(
            action_cell(clear, Cover::Forest),
            action_cell(clear, Cover::Pine)
        );
        assert_ne!(
            action_cell(clear, Cover::Jungle),
            action_cell(clear, Cover::Forest)
        );
    }

    #[test]
    fn the_gold_line_shows_the_treasury_and_its_signed_change() {
        assert_eq!(gold_line(50, 3), "50 Gold (+3 per turn)");
        assert_eq!(gold_line(0, 0), "0 Gold (+0 per turn)");
        assert_eq!(gold_line(7, -2), "7 Gold (-2 per turn)");
    }

    /// The box shows `economy::finance` for the civ in play: its own gold,
    /// its own cities and units, and red ink once it is losing gold.
    #[test]
    fn the_gold_line_follows_the_active_civ_and_reddens_in_deficit() {
        use crate::cities::Production;
        let mut map = GameMap::generate();
        for t in map.tiles.iter_mut() {
            t.base = crate::map::Base::Grassland;
            t.relief = crate::map::Relief::Flat;
            t.cover = Cover::Bare;
            t.resource = None;
            t.road = false;
            t.seen = true;
        }
        let (x, y) = map.start;
        let mut city = City {
            gifts: vec![],
            coastal: false,
            river: false,
            unrest: 0,
            hurry_timer: 0,
            civ: 0,
            name: "Kyoto".to_string(),
            x,
            y,
            size: 1,
            food: 0,
            shields: 0,
            production: Production::Warrior,
            queue: vec![],
            buildings: vec![],
            worked: Default::default(),
            culture: 0,
            founded: 1,
        };
        crate::cities::governor_assign(&map, &mut city, &Default::default());
        // A road under the worked tile: two commerce, one gold of tax.
        for &(wx, wy) in &city.worked {
            let i = map.idx(wx, wy);
            map.tiles[i].road = true;
        }
        let mut app = App::new();
        app.insert_resource(map);
        app.init_resource::<Civilizations>();
        app.insert_resource(Treasury([12, 3, 0, 0]));
        app.add_systems(Update, update_gold);
        let line = app
            .world_mut()
            .spawn((Text::new(""), TextColor(Color::WHITE), BoxGold))
            .id();
        app.world_mut().spawn(city);
        // Five units, four free: one gold of support against one of tax.
        for _ in 0..5 {
            app.world_mut().spawn(Unit::new(0, UnitType::Warrior, x, y));
        }
        app.update();
        let shown = |app: &App| app.world().get::<Text>(line).unwrap().0.clone();
        assert_eq!(shown(&app), "12 Gold (+0 per turn)");
        // A sixth unit tips it into the red.
        app.world_mut().spawn(Unit::new(0, UnitType::Warrior, x, y));
        app.update();
        assert_eq!(shown(&app), "12 Gold (-1 per turn)");
        assert_ne!(
            app.world().get::<TextColor>(line).unwrap().0,
            Color::srgb(0.23, 0.14, 0.06)
        );
        // Civ 1 has its own purse, and no cities to pay for units.
        app.world_mut().resource_mut::<Civilizations>().active = 1;
        app.update();
        assert_eq!(shown(&app), "3 Gold (+0 per turn)");
        assert_eq!(
            app.world().get::<TextColor>(line).unwrap().0,
            Color::srgb(0.23, 0.14, 0.06)
        );
    }
}
