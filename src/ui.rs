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

/// The centered banner that says how the game ended.
#[derive(Component)]
pub(crate) struct GameOverLabel;

pub fn spawn_hud(mut commands: Commands, assets: Res<AssetServer>) {
    let font = assets.load("gen/fonts/lsans.ttf");
    commands.spawn((
        Text::new(""),
        TextFont {
            font: font.clone(),
            font_size: 34.0,
            ..default()
        },
        TextColor(Color::srgb(1.0, 0.9, 0.5)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(90.0),
            align_self: AlignSelf::Center,
            justify_self: JustifySelf::Center,
            padding: UiRect::axes(Val::Px(24.0), Val::Px(10.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
        Visibility::Hidden,
        GameOverLabel,
    ));
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
    civs: Res<crate::civs::Civilizations>,
) {
    if view.0.is_some() || splash.0 || crate::civs::is_ai(civs.active) {
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
    units: Query<(Entity, &Unit)>,
    cities: Query<&crate::cities::City>,
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
                if let Some(u) = selected.0.and_then(|s| units.get(s).ok().map(|(_, u)| u)) {
                    let ports: Vec<_> = cities.iter().filter(|c| c.civ == u.civ && c.coastal).map(|c| (c.x, c.y)).collect();
                    let snapshot: Vec<_> = units.iter().map(|(e, u)| (e, u.clone())).collect();
                    match crate::naval::route(&map, u, (x, y), &ports, &snapshot) {
                        Some(p) => {
                            let extra = units::path_turns(
                                &map,
                                (u.x, u.y),
                                u.moves,
                                crate::naval::moves(u.utype, u.civ),
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


/// Show the verdict once the game is decided.
pub fn update_game_over(
    civs: Res<crate::civs::Civilizations>,
    mut q: Query<(&mut Text, &mut Visibility), With<GameOverLabel>>,
) {
    use crate::civs::{CIVS, Outcome};
    let Ok((mut text, mut vis)) = q.single_mut() else {
        return;
    };
    let (shown, v) = match civs.outcome {
        Some(Outcome::Victory(c)) if !crate::civs::is_ai(c) => (
            format!("Victory! The {} stand alone.", CIVS[c].name),
            Visibility::Visible,
        ),
        Some(Outcome::Victory(c)) => (
            format!("The {} have conquered the world.", CIVS[c].name),
            Visibility::Visible,
        ),
        Some(Outcome::Defeat) => (
            "Defeat. Your civilization has been destroyed.".to_string(),
            Visibility::Visible,
        ),
        None => (String::new(), Visibility::Hidden),
    };
    if text.0 != shown {
        text.0 = shown;
    }
    if *vis != v {
        *vis = v;
    }
}
