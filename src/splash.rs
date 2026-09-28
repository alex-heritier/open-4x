//! Tokugawa greeting splash. Shows once at startup over the new game;
//! any click or key dismisses it and starts the peace music loop.

use bevy::prelude::*;

use crate::audio::{self, GameAudio};

#[derive(Resource, Default)]
pub struct SplashUp(pub bool);

#[derive(Component)]
pub(crate) struct SplashRoot;

const GREETING: &str = "Greetings. I am Tokugawa Ieyasu of the Japanese. \
    Our people have wandered long enough. It is time to build Kyoto, \
    explore these lands, and make them ours.";

pub fn setup_splash(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut splash: ResMut<SplashUp>,
) {
    splash.0 = true;
    let font = assets.load("gen/fonts/lsans.ttf");
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
                ImageNode::new(assets.load("gen/splash/tokugawa.png")),
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
                    Text::new(GREETING),
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
        commands
            .entity(r)
            .despawn_related::<Children>()
            .despawn();
    }
    commands.insert_resource(SplashUp(false));
    if let Some(audio) = audio.as_mut() {
        audio::start_peace_music(&mut commands, audio);
    }
}
