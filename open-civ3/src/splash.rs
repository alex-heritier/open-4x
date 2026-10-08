//! Greeting splash for the hotseat game. Shows once at startup over the
//! new game; any click or key dismisses it and starts the peace music
//! loop. The portrait art is still Tokugawa's.

use bevy::prelude::*;

use crate::audio::{self, GameAudio};
use crate::civs::CIVS;

#[derive(Resource, Default)]
pub struct SplashUp(pub bool);

#[derive(Component)]
pub(crate) struct SplashRoot;

/// Hotseat greeting: the chair is shared, so the splash names every
/// civilization that will act rather than speaking for one leader. The
/// portrait art stays Tokugawa's.
fn greeting() -> String {
    let list = CIVS.iter().map(|c| c.name).collect::<Vec<_>>().join(", ");
    format!(
        "Greetings. {} civilizations share this world, and each \
         takes the chair in turn: {list}. Found your first city, explore \
         these lands, and make them yours.",
        crate::civs::civ_count()
    )
}

pub fn setup_splash(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut splash: ResMut<SplashUp>,
    mut audio: Option<ResMut<GameAudio>>,
) {
    // Debug: CIV3_NO_SPLASH=1 starts straight on the map. Any key or click
    // dismisses the greeting, so screenshot runs would otherwise need input.
    if std::env::var("CIV3_NO_SPLASH").is_ok() {
        splash.0 = false;
        if let Some(audio) = audio.as_mut() {
            audio::start_peace_music(&mut commands, audio);
        }
        return;
    }
    splash.0 = true;
    let font = assets.load("fonts/lsans.ttf");
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.75)),
            SplashRoot,
        ))
        .with_children(|root| {
            root.spawn((
                ImageNode::new(assets.load("splash/tokugawa.png")),
                Node {
                    width: Val::Px(400.0),
                    height: Val::Px(480.0),
                    margin: UiRect::right(Val::Px(32.0)),
                    ..default()
                },
            ));
            root.spawn((
                Node {
                    width: Val::Px(420.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(24.0),
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ))
            .with_children(|col| {
                col.spawn((
                    Text::new(greeting()),
                    TextFont {
                        font: font.clone(),
                        font_size: 22.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.95, 0.9, 0.8)),
                ));
                col.spawn((
                    Text::new("Click or press any key to begin."),
                    TextFont {
                        font,
                        font_size: 18.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.7, 0.7, 0.7)),
                ));
            });
        });
}

pub fn dismiss_splash(
    mut commands: Commands,
    splash: Res<SplashUp>,
    roots: Query<Entity, With<SplashRoot>>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut audio: Option<ResMut<GameAudio>>,
) {
    if !splash.0 {
        return;
    }
    let clicked = buttons.get_just_pressed().next().is_some();
    let pressed = keys.get_just_pressed().next().is_some();
    if !clicked && !pressed {
        return;
    }
    for r in roots.iter() {
        commands.entity(r).despawn_related::<Children>().despawn();
    }
    commands.insert_resource(SplashUp(false));
    if let Some(audio) = audio.as_mut() {
        audio::start_peace_music(&mut commands, audio);
    }
}
