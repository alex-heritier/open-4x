use bevy::prelude::*;
use bevy::window::WindowResolution;

mod actionbar;
mod audio;
mod blend;
mod cities;
mod features;
mod improvements;
mod input;
mod map;
mod render;
mod rng;
mod screenshot;
mod script;
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
        .add_message::<actionbar::UnitCommand>()
        .init_resource::<actionbar::GotoMode>()
        .insert_resource(GameMap::generate())
        // Debug: CIV3_REVEAL=1 starts with fog off, like the F9 toggle.
        .insert_resource(render::RevealAll(std::env::var("CIV3_REVEAL").is_ok()))
        .insert_resource(units::Turn(1))
        .init_resource::<input::Hovered>()
        .init_resource::<input::DragState>()
        .init_resource::<cities::CityNamesUsed>()
        .init_resource::<cities::CityView>()
        .init_resource::<cities::Capital>()
        .init_resource::<cities::BuildMenu>()
        .init_resource::<splash::SplashUp>()
        .init_resource::<features::MessageBoard>()
        .init_resource::<screenshot::Shots>()
        .add_systems(
            Startup,
            (
                setup_camera,
                setup_camera_zoom,
                setup_art,
                setup_rng,
                audio::setup_audio,
                splash::setup_splash,
                render::spawn_terrain,
                features::spawn_features,
                units::spawn_party,
                ui::spawn_hud,
                actionbar::spawn_bar,
                screenshot::setup_shots,
                script::setup_script,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (
                (
                    script::drive_script,
                    input::camera_control,
                    input::hover,
                    input::orders,
                    actionbar::run_commands,
                    cities::found_city,
                    units::end_turn_units,
                    cities::end_turn_cities,
                    improvements::end_turn_work,
                    units::drive_movement,
                    units::advance_anims,
                    features::resolve_features,
                    units::refresh_visibility,
                    units::restack,
                    units::auto_select,
                    splash::dismiss_splash,
                    ui::end_turn_button,
                    screenshot::drive_shots,
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
                    features::sync_feature_sprites,
                    improvements::sync_improvement_sprites,
                    render::update_fog,
                    render::sync_cover,
                    ui::update_hover_label,
                    actionbar::update_bar,
                    actionbar::blink_next_turn,
                    ui::update_message_label,
                    screenshot::manual_shot,
                )
                    .chain(),
            )
                .chain(),
        )
        .run();
}

fn setup_camera(mut commands: Commands, map: Res<GameMap>) {
    // Debug: MAP_CENTER=x,y starts the camera over a tile (like MAP_SEED).
    let center = std::env::var("MAP_CENTER")
        .ok()
        .and_then(|s| {
            let (x, y) = s.split_once(',')?;
            Some((x.parse().ok()?, y.parse().ok()?))
        })
        .unwrap_or(map.start);
    let c = map::tile_to_world(center.0, center.1);
    commands.spawn((Camera2d, Transform::from_xyz(c.x, c.y, 0.0)));
}

/// Debug: MAP_ZOOM sets the initial camera zoom (like MAP_SEED).
fn setup_camera_zoom(mut cam: Query<&mut Projection, With<Camera2d>>) {
    let zoom = std::env::var("MAP_ZOOM")
        .ok()
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(1.0)
        .clamp(0.35, 2.5);
    if let Ok(mut proj) = cam.single_mut() {
        if let Projection::Orthographic(o) = &mut *proj {
            o.scale = zoom;
        }
    }
}

fn setup_art(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
) {
    commands.insert_resource(tiles::TileArt::load(&assets));
    commands.insert_resource(units::UnitArt::load(&assets));
    commands.insert_resource(cities::CityArt::load(&assets));
    commands.insert_resource(features::FeatureArt::load(&assets));
    commands.insert_resource(improvements::ImprovementArt::load(&assets));
    commands.insert_resource(cities::CapitalStar::generate(&mut images));
}

/// Gameplay RNG, seeded from the map seed (the binary reseeds from the
/// clock at game start; determinism per map is our deviation).
fn setup_rng(mut commands: Commands, map: Res<GameMap>) {
    commands.insert_resource(rng::GameRng::new(map.seed as u32));
}
