//! HUD text: hover readout and control hints.

use bevy::prelude::*;

use crate::audio::{self, GameAudio};
use crate::cities::CityView;
use crate::features::{self, MessageBoard};
use crate::improvements;
use crate::input::Hovered;
use crate::map::GameMap;
use crate::splash::SplashUp;
use crate::units::{self, Selected, Turn, TurnEnded, Unit};

#[derive(Component)]
pub(crate) struct HoverLabel;

#[derive(Component)]
pub(crate) struct TurnLabel;

#[derive(Component)]
pub(crate) struct SelLabel;

#[derive(Component)]
pub(crate) struct MessageLabel;

#[derive(Component)]
pub(crate) struct EndTurnButton;

pub fn spawn_hud(mut commands: Commands, assets: Res<AssetServer>) {
    let font = assets.load("gen/fonts/lsans.ttf");
    commands.spawn((
        Text::new(""),
        TextFont {
            font: font.clone(),
            font_size: 15.0,
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            left: Val::Px(8.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
        HoverLabel,
    ));
    commands.spawn((
        Text::new("click: select/move/open city | right-click: move | B: found city | F: fortify | R: road | I: irrigate | Space: skip | Tab: cycle | Enter: end turn"),
        TextFont {
            font: font.clone(),
            font_size: 14.0,
            ..default()
        },
        TextColor(Color::srgb(0.8, 0.8, 0.8)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(8.0),
            left: Val::Px(8.0),
            ..default()
        },
    ));
    commands.spawn((
        Text::new(""),
        TextFont {
            font: font.clone(),
            font_size: 16.0,
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(8.0),
            right: Val::Px(12.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
        TurnLabel,
    ));
    commands.spawn((
        Text::new(""),
        TextFont {
            font: font.clone(),
            font_size: 16.0,
            ..default()
        },
        TextColor(Color::srgb(1.0, 1.0, 0.6)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(8.0),
            right: Val::Px(12.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
        SelLabel,
    ));
    commands.spawn((
        Text::new(""),
        TextFont {
            font: font.clone(),
            font_size: 17.0,
            ..default()
        },
        TextColor(Color::srgb(1.0, 0.95, 0.7)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(36.0),
            left: Val::Px(8.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
        MessageLabel,
    ));
    commands
        .spawn((
            Button,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(40.0),
                right: Val::Px(12.0),
                padding: UiRect::axes(Val::Px(18.0), Val::Px(10.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.35, 0.22, 0.1)),
            EndTurnButton,
        ))
        .with_children(|b| {
            b.spawn((
                Text::new("End Turn"),
                TextFont {
                    font,
                    font_size: 18.0,
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.95, 0.8)),
            ));
        });
}

pub fn end_turn_button(
    mut commands: Commands,
    buttons: Query<&Interaction, With<EndTurnButton>>,
    mut turn_end: MessageWriter<TurnEnded>,
    audio: Res<GameAudio>,
    view: Res<CityView>,
    splash: Res<SplashUp>,
) {
    if view.0.is_some() || splash.0 {
        return;
    }
    for pressed in buttons.iter() {
        if *pressed == Interaction::Pressed {
            turn_end.write(TurnEnded);
            audio::sfx(&mut commands, &audio, "EnterTurn");
        }
    }
}

pub fn update_hover_label(
    map: Res<GameMap>,
    hovered: Res<Hovered>,
    mut q: Query<&mut Text, With<HoverLabel>>,
) {
    let Ok(mut text) = q.single_mut() else {
        return;
    };
    text.0 = match hovered.0 {
        Some((x, y)) => {
            let t = &map.tiles[map.idx(x, y)];
            let base = format!("({x},{y}) {:?} {:?} {:?}", t.base, t.relief, t.cover);
            let mut parts = vec![base];
            if let Some(f) = features::describe(t) {
                parts.push(f);
            }
            if let Some(i) = improvements::describe(t) {
                parts.push(i);
            }
            parts.join(" | ")
        }
        None => String::new(),
    };
}

pub fn update_message_label(
    time: Res<Time>,
    mut board: ResMut<MessageBoard>,
    mut q: Query<&mut Text, With<MessageLabel>>,
) {
    let Ok(mut text) = q.single_mut() else {
        return;
    };
    if board.ttl > 0.0 {
        board.ttl -= time.delta_secs();
        text.0 = board.text.clone();
        if board.ttl <= 0.0 {
            board.text.clear();
        }
    } else {
        text.0 = String::new();
    }
}

pub fn update_turn_label(
    turn: Res<Turn>,
    units: Query<&Unit>,
    mut q: Query<&mut Text, With<TurnLabel>>,
) {
    let Ok(mut text) = q.single_mut() else {
        return;
    };
    let active = units.iter().filter(|u| !u.fortified && u.moves > 0).count();
    text.0 = if active == 0 {
        format!("Turn {}  |  ENTER: end turn", turn.0)
    } else {
        format!("Turn {}", turn.0)
    };
}

pub fn update_sel_label(
    selected: Res<Selected>,
    units: Query<&Unit>,
    mut q: Query<&mut Text, With<SelLabel>>,
) {
    let Ok(mut text) = q.single_mut() else {
        return;
    };
    text.0 = match selected.0.and_then(|s| units.get(s).ok()) {
        Some(u) => {
            let d = units::def(u.utype);
            let extra = if u.fortified {
                " (fortified)"
            } else if u.moves == 0 {
                " (done)"
            } else {
                ""
            };
            format!("{}  {}/{} MP{extra}", d.name, u.moves, d.moves)
        }
        None => String::new(),
    };
}
