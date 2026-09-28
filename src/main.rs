use bevy::prelude::*;
use bevy::window::WindowResolution;

mod audio;
mod cities;
mod input;
mod map;
mod render;
mod splash;
mod tiles;
mod ui;
mod units;

use map::GameMap;

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Civ3 Clone".into(),
                        resolution: WindowResolution::new(1280, 800),
                        ..default()
                    }),
                    ..default()
                })
                .set(ImagePlugin::default_nearest()),
        )
        .add_message::<units::TurnEnded>()
        .insert_resource(GameMap::generate())
        .insert_resource(render::RevealAll(false))
        .insert_resource(units::Turn(1))
        .init_resource::<input::Hovered>()
        .init_resource::<input::DragState>()
        .init_resource::<cities::CityNamesUsed>()
        .init_resource::<cities::CityView>()
        .init_resource::<splash::SplashUp>()
        .add_systems(
            Startup,
            (
                setup_camera,
                setup_art,
                audio::setup_audio,
                splash::setup_splash,
                render::spawn_terrain,
                units::spawn_party,
                ui::spawn_hud,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (
                (
                    input::camera_control,
                    input::hover,
                    input::orders,
                    cities::found_city,
                    units::end_turn_units,
                    cities::end_turn_cities,
                    units::drive_movement,
                    units::advance_anims,
                    units::refresh_visibility,
                    units::restack,
                    units::auto_select,
                    splash::dismiss_splash,
                    ui::end_turn_button,
                )
                    .chain(),
                (
                    units::unit_visibility,
                    units::selection_gizmo,
                    units::animate_units,
                    cities::sync_city_visuals,
                    cities::city_visibility,
                    cities::maintain_city_screen,
                    cities::city_screen_input,
                    cities::city_screen_buttons,
                    render::update_fog,
                    ui::update_hover_label,
                    ui::update_turn_label,
                    ui::update_sel_label,
                )
                    .chain(),
            )
                .chain(),
        )
        .run();
}

fn setup_camera(mut commands: Commands, map: Res<GameMap>) {
    let c = map::tile_to_world(map.start.0, map.start.1);
    commands.spawn((Camera2d, Transform::from_xyz(c.x, c.y, 0.0)));
}

fn setup_art(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(tiles::TileArt::load(&assets));
    commands.insert_resource(units::UnitArt::load(&assets));
    commands.insert_resource(cities::CityArt::load(&assets));
}
