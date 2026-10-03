use bevy::prelude::*;
use bevy::window::WindowResolution;

mod actionbar;
mod advisors;
mod ai;
mod audio;
mod blend;
mod borders;
mod cities;
mod civs;
mod combat;
mod diplomacy;
mod economy;
mod features;
mod improvements;
mod input;
mod map;
mod production_prompt;
mod render;
mod research;
mod rng;
mod rules_data;
mod screenshot;
mod script;
mod splash;
mod tiles;
mod ui;
mod unit_picker;
mod units;

use map::GameMap;

fn main() {
    civs::set_controllers();
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
        .add_message::<civs::CivilizationEnded>()
        .add_message::<combat::AttackOrder>()
        .add_message::<cities::FoundCityOrder>()
        .init_resource::<ai::AiState>()
        .init_resource::<combat::ActiveCombat>()
        .init_resource::<combat::CombatSpeed>()
        .init_resource::<civs::Civilizations>()
        .insert_resource(research::Research::new())
        .insert_resource(diplomacy::Diplomacy::new())
        .init_resource::<advisors::Advisors>()
        .add_message::<actionbar::UnitCommand>()
        .init_resource::<actionbar::GotoMode>()
        .insert_resource(GameMap::generate())
        // Debug: CIV3_REVEAL=1 starts with fog off, like the F9 toggle.
        .insert_resource(render::RevealAll(std::env::var("CIV3_REVEAL").is_ok()))
        .insert_resource(units::Turn(1))
        .init_resource::<unit_picker::UnitPicker>()
        .init_resource::<input::Hovered>()
        .init_resource::<input::HoverPin>()
        .init_resource::<input::MovePreview>()
        .init_resource::<cities::CityNamesUsed>()
        .init_resource::<cities::CityView>()
        .init_resource::<cities::Capital>()
        .init_resource::<cities::Treasury>()
        .init_resource::<cities::BuildMenu>()
        .init_resource::<production_prompt::ProductionPrompts>()
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
                units::spawn_selection_ring,
                ui::spawn_hud,
                actionbar::spawn_bar,
                screenshot::setup_shots,
                script::setup_script,
                diplomacy::setup,
                research::begin,
                advisors::spawn_buttons,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (
                (
                    (
                        script::drive_script,
                        diplomacy::refresh,
                        diplomacy::detect_contact,
                        research::refresh,
                        advisors::hotkeys,
                        advisors::open_buttons,
                        advisors::interrupt,
                        advisors::respond,
                    )
                        .chain(),
                    input::camera_control.run_if(unit_picker::inactive),
                    (input::hover, unit_picker::update).chain(),
                    input::hold_preview.run_if(unit_picker::inactive),
                    input::orders
                        .run_if(production_prompt::inactive)
                        .run_if(unit_picker::inactive)
                        .run_if(combat::idle),
                    actionbar::run_commands
                        .run_if(production_prompt::inactive)
                        .run_if(combat::idle),
                    (
                        ai::play_turn,
                        cities::found_city.run_if(production_prompt::inactive),
                        civs::check_elimination,
                    )
                        .chain()
                        .run_if(combat::idle),
                    // Research closes before the cities are processed, so an
                    // advance that arrives this turn can be built this turn.
                    (
                        units::end_turn_units,
                        research::end_turn,
                        diplomacy::end_turn,
                        cities::end_turn_cities,
                    )
                        .chain(),
                    improvements::end_turn_work,
                    // Visibility first: it holds `seen` for the civ in play,
                    // which the worked-tile check reads.
                    (units::refresh_visibility, cities::reconcile_tiles).chain(),
                    units::drive_movement.run_if(combat::idle),
                    units::advance_anims,
                    features::resolve_features,
                    units::restack,
                    units::auto_select,
                    civs::focus_active_civ,
                    splash::dismiss_splash,
                    ui::end_turn_button
                        .run_if(production_prompt::inactive)
                        .run_if(combat::idle),
                    screenshot::drive_shots,
                )
                    .chain(),
                (
                    (unit_picker::style_and_scroll, units::unit_visibility).chain(),
                    units::selection_gizmo,
                    units::ring_follow,
                    units::animate_units,
                    (cities::sync_city_visuals, cities::city_visibility).chain(),
                    cities::maintain_city_screen,
                    cities::city_screen_input.run_if(production_prompt::city_input_allowed),
                    (cities::city_screen_buttons, cities::update_panel_buttons).chain(),
                    (production_prompt::respond, production_prompt::show).chain(),
                    features::sync_feature_sprites,
                    improvements::sync_improvement_sprites,
                    borders::sync_borders,
                    render::update_fog,
                    render::sync_cover,
                    ui::update_hover_label,
                    (actionbar::update_bar, actionbar::update_gold, advisors::update_science_line, advisors::show).chain(),
                    actionbar::blink_next_turn,
                    ui::update_message_label,
                    ui::update_game_over,
                    screenshot::manual_shot,
                )
                    .chain(),
            )
                .chain(),
        )
        // The fight on screen: after movement has asked for it, before the
        // clips advance.
        .add_systems(
            Update,
            (combat::start_attacks, combat::run_combat)
                .chain()
                .after(units::drive_movement)
                .before(units::advance_anims),
        )
        .add_systems(Update, combat::sync_health_bars.after(units::animate_units))
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

fn setup_art(mut commands: Commands, assets: Res<AssetServer>, mut images: ResMut<Assets<Image>>) {
    commands.insert_resource(tiles::TileArt::load(&assets));
    commands.insert_resource(units::UnitArt::load(&assets));
    commands.insert_resource(cities::CityArt::load(&assets));
    commands.insert_resource(features::FeatureArt::load(&assets));
    commands.insert_resource(improvements::ImprovementArt::load(&assets));
    commands.insert_resource(borders::BorderArt::load(&assets));
    commands.insert_resource(units::SelectionRing::load(&assets));
    commands.insert_resource(cities::CapitalStar::generate(&mut images));
}

/// Gameplay RNG, seeded from the map seed (the binary reseeds from the
/// clock at game start; determinism per map is our deviation).
fn setup_rng(mut commands: Commands, map: Res<GameMap>) {
    commands.insert_resource(rng::GameRng::new(map.seed as u32));
    // Combat has its own dice in the binary (the `Random` instance at
    // `0xA526B4`, the map generator's LCG), not the `rand()` of the hut rolls.
    commands.insert_resource(combat::CombatRng(rng::MapRng::new(map.seed as u32)));
}
