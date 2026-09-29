//! HUD text: hover readout and control hints.

use bevy::prelude::*;

use crate::actionbar::{InfoBox, NextTurnDisc};
use crate::audio::{self, GameAudio};
use crate::cities::CityView;
use crate::features::{self, MessageBoard};
use crate::improvements;
use crate::input::Hovered;
use crate::input::MovePreview;
use crate::map::GameMap;
use crate::splash::SplashUp;
use crate::units::{self, Selected, TurnEnded, Unit};

#[derive(Component)]
pub(crate) struct HoverLabel;

#[derive(Component)]
pub(crate) struct MessageLabel;

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
        Text::new(""),
        TextFont {
            font: font.clone(),
            font_size: 17.0,
            ..default()
        },
        TextColor(Color::srgb(1.0, 0.95, 0.7)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(56.0),
            left: Val::Px(8.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
        MessageLabel,
    ));
}

/// The next-turn disc always ends the turn; the rest of Civ3's bottom-right
/// box does once no unit is selected ("click here for next turn").
pub fn end_turn_button(
    mut commands: Commands,
    disc: Query<&Interaction, (Changed<Interaction>, With<NextTurnDisc>)>,
    info_box: Query<&Interaction, (Changed<Interaction>, With<InfoBox>)>,
    selected: Res<Selected>,
    mut turn_end: MessageWriter<TurnEnded>,
    audio: Res<GameAudio>,
    view: Res<CityView>,
    splash: Res<SplashUp>,
) {
    if view.0.is_some() || splash.0 {
        return;
    }
    let pressed = |i: &Interaction| *i == Interaction::Pressed;
    if disc.iter().any(pressed) || (selected.0.is_none() && info_box.iter().any(pressed)) {
        turn_end.write(TurnEnded);
        audio::sfx(&mut commands, &audio, "EnterTurn");
    }
}

pub fn update_hover_label(
    map: Res<GameMap>,
    hovered: Res<Hovered>,
    selected: Res<Selected>,
    preview: Res<MovePreview>,
    units: Query<&Unit>,
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
            // The route readout belongs to the move preview: Civ3 shows it
            // while a Go-to or a held press is being aimed, not on a hover.
            if preview.0 == Some((x, y)) {
                if let Some(u) = selected.0.and_then(|s| units.get(s).ok()) {
                    match map.find_path((u.x, u.y), (x, y)) {
                        Some(p) => {
                            let extra = units::path_turns(
                                &map,
                                (u.x, u.y),
                                u.moves,
                                units::def(u.utype).moves * crate::map::MP,
                                &p,
                            );
                            parts.push(format!(
                                "path {} steps, {}",
                                p.len(),
                                if extra == 0 {
                                    "this turn".to_string()
                                } else {
                                    format!("{} turns", extra + 1)
                                }
                            ));
                        }
                        None => parts.push("no path".to_string()),
                    }
                }
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

