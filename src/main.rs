use bevy::prelude::*;
use bevy::window::WindowResolution;

mod actionbar;
mod abandon;
mod actions;
mod advisor_frame;
mod advisors;
mod ai;
mod army;
mod barbarians;
mod assets;
mod audio;
mod blend;
mod boot;
mod borders;
mod bombard;
mod build_switch;
mod calendar;
mod cli;
mod capital;
mod cities;
mod citizens;
mod disease;
mod citycalc;
mod civs;
mod combat;
mod diplomacy;
mod domestic;
mod economy;
mod features;
mod flip;
mod golden;
mod govern;
mod hurry;
mod huts;
mod naval;
mod improvements;
mod input;
mod install;
mod leaders;
mod map;
mod production_prompt;
mod render;
mod realm;
mod research;
mod resistance;
mod rng;
mod roles;
mod populate;
mod savfile;
mod scenario;
mod rivers;
mod save;
mod roster;
mod ruleset;
mod screenshot;
mod script;
mod speech;
mod splash;
mod stage;
mod tech_tree;
mod tiles;
mod trade;
mod ui;
mod unit_picker;
mod units;
mod upgrades;
mod sites;
mod weariness;
mod web;
mod zoc;
mod wonders;

use map::GameMap;

/// Lift the open-file soft limit to the hard limit (no privilege needed).
///
/// The asset server opens one file per pending load; a unit-heavy save queues
/// thousands at once, and macOS starts a shell's game with `RLIMIT_NOFILE`
/// soft = 256, so those loads fail with `EMFILE` (os error 24) and the sprite
/// keeps its placeholder. The hard limit is the machine's own cap
/// (`kern.maxfilesperproc`), so this only removes the artificial ceiling.
#[cfg(unix)]
fn raise_fd_limit() {
    // SAFETY: `getrlimit`/`setrlimit` read and write a process rlimit, on a
    // value initialized here; failure just leaves the limit alone.
    unsafe {
        let mut limit = std::mem::zeroed::<libc::rlimit>();
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) != 0 || limit.rlim_cur >= limit.rlim_max {
            return;
        }
        // `rlim_max` may be `RLIM_INFINITY`, which is not a request the kernel
        // takes; ask for a million, above `kern.maxfilesperproc`.
        limit.rlim_cur = limit.rlim_max.min(1 << 20);
        libc::setrlimit(libc::RLIMIT_NOFILE, &limit);
    }
}

fn main() {
    #[cfg(unix)]
    raise_fd_limit();

    #[cfg(target_arch = "wasm32")]
    {
        console_error_panic_hook::set_once();
        let console_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            web::show_error(&info.to_string());
            console_hook(info);
        }));
    }
    let boot = match load_boot() {
        Ok(b) => b,
        Err(msg) => fail(&msg),
    };
    #[cfg(target_arch = "wasm32")]
    web::show_progress("rules ready");
    if let Err(msg) = scenario::install(&boot) {
        fail(&msg);
    }
    #[cfg(target_arch = "wasm32")]
    web::show_progress("scenario ready");
    civs::set_controllers();
    let (roster, colors) = assets::in_play();
    let wanted = assets::Wanted { install: &boot.install, plan: &ruleset::get().art, roster: &roster, colors };
    if let Err(msg) = assets::ensure(&wanted) {
        fail(&msg);
    }
    let mut app = App::new();
    app.insert_resource(boot)
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Civ3 Clone".into(),
                        resolution: WindowResolution::new(1280, 800),
                        canvas: Some("#game".into()),
                        fit_canvas_to_parent: true,
                        prevent_default_event_handling: true,
                        ..default()
                    }),
                    ..default()
                })
                .set(asset_plugin())
                .set(ImagePlugin::default_nearest()),
        )
        .add_message::<units::TurnEnded>()
        .add_message::<save::Command>()
        .add_message::<civs::CivilizationEnded>()
        .add_message::<combat::AttackOrder>()
        .add_message::<bombard::Order>()
        .init_resource::<bombard::TargetMode>()
        .init_resource::<cities::BorderKey>()
        .add_message::<cities::FoundCityOrder>()
        .init_resource::<ai::AiState>()
        .init_resource::<combat::ActiveCombat>()
        .init_resource::<combat::CombatSpeed>()
        .insert_resource(civs::Civilizations::start())
        .init_resource::<units::Exploration>()
        .insert_resource(research::Research::new())
        .insert_resource(diplomacy::Diplomacy::new())
        .init_resource::<advisors::Advisors>()
        .init_resource::<domestic::Domestic>()
        .insert_resource(speech::Speech::load())
        .insert_resource(leaders::LeaderArt::load())
        .init_resource::<wonders::Wonders>()
        .add_message::<actionbar::UnitCommand>()
        .init_resource::<actionbar::GotoMode>()
        .insert_resource(GameMap::generate())
        // Debug: CIV3_REVEAL=1 starts with fog off, like the F9 toggle.
        .insert_resource(render::RevealAll(std::env::var("CIV3_REVEAL").is_ok() || scenario::settings().reveal_map))
        .insert_resource(units::Turn(scenario::start_turn()))
        .init_resource::<unit_picker::UnitPicker>()
        .init_resource::<input::Hovered>()
        .init_resource::<input::HoverPin>()
        .init_resource::<input::MovePreview>()
        .init_resource::<cities::CityNamesUsed>()
        .init_resource::<cities::CityView>()
        .init_resource::<cities::CityFrame>()
        .init_resource::<cities::Capital>()
        .init_resource::<cities::Treasury>()
        .init_resource::<cities::BuildMenu>()
        .init_resource::<cities::HurryAsk>()
        .init_resource::<build_switch::BuildSwitch>()
        .init_resource::<barbarians::Barbarians>()
        .init_resource::<flip::Flips>()
        .init_resource::<production_prompt::ProductionPrompts>()
        .init_resource::<abandon::Abandon>()
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
                barbarians::setup_camps,
                units::spawn_selection_ring,
                ui::spawn_hud,
                actionbar::spawn_bar,
                screenshot::setup_shots,
                script::setup_script,
                diplomacy::setup,
                research::begin,
                leaders::preload,
                advisors::spawn_buttons,
                domestic::spawn_button,
            )
                .chain(),
        )
        .add_systems(Startup, populate::populate.after(units::spawn_party).before(units::spawn_selection_ring))
        .add_systems(Startup, units::seed_exploration_from_save.after(populate::populate))
        .add_systems(
            Update,
            (
                (
                    (
                        script::drive_script,
                        diplomacy::refresh,
                        diplomacy::detect_contact,
                        research::refresh,
                        realm::sync,
                        golden::track,
                        advisors::hotkeys,
                        advisors::open_buttons,
                        advisors::interrupt,
                        advisors::respond,
                        domestic::hotkeys,
                        domestic::open_button,
                        domestic::respond,
                    )
                        .chain(),
                    input::camera_control.run_if(unit_picker::inactive),
                    (input::hover, unit_picker::update).chain(),
                    input::hold_preview.run_if(unit_picker::inactive),
                    input::orders
                        .run_if(barbarians::idle)
                        .run_if(production_prompt::inactive)
                        .run_if(unit_picker::inactive)
                        .run_if(combat::idle),
                    (actionbar::run_commands, bombard::arm, naval::cargo_commands, army::commands)
                        .chain()
                        .run_if(barbarians::idle)
                        .run_if(production_prompt::inactive)
                        .run_if(combat::idle),
                    (actions::run, actions::auto_workers)
                        .chain()
                        .run_if(barbarians::idle)
                        .run_if(production_prompt::inactive)
                        .run_if(combat::idle),
                    (
                        barbarians::play,
                barbarians::uprising,
                        (
                            govern::ai_turn,
                            army::ai_science_leaders,
                            upgrades::ai_turn,
                            ai::play_turn,
                            cities::found_city.run_if(production_prompt::inactive),
                            civs::check_elimination,
                        )
                            .chain()
                            .run_if(barbarians::idle),
                    )
                        .chain()
                        .run_if(combat::idle),
                    // Research closes before the cities are processed, so an
                    // advance that arrives this turn can be built this turn.
                    (
                        units::end_turn_units,
                        improvements::end_turn_work,
                        research::end_turn,
                        diplomacy::end_turn,
                        weariness::end_turn,
                        flip::run,
                        civs::check_domination,
                        civs::check_time_limit,
                        cities::end_turn_cities,
                        wonders::track,
                    )
                        .chain(),
                    // Visibility first: it holds `seen` for the civ in play,
                    // which the worked-tile check reads; so do the borders,
                    // recomputed when a city's level, owner or existence
                    // changed this frame.
                    (
                        capital::replace_missing,
                        sites::remove_overrun,
                        units::refresh_visibility,
                        cities::update_borders,
                        cities::reconcile_tiles,
                    )
                        .chain(),
                    units::drive_movement.run_if(combat::idle),
                    units::advance_anims,
                    (features::resolve_features, research::spawn_leaders).chain(),
                    units::restack,
                    units::auto_select,
                    civs::focus_active_civ,
                    splash::dismiss_splash,
                    ui::end_turn_button
                        .run_if(barbarians::idle)
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
                    (production_prompt::respond, production_prompt::show,
                        build_switch::respond, build_switch::show,
                        abandon::respond, abandon::show).chain(),
                    features::sync_feature_sprites,
                    improvements::sync_improvement_sprites,
                    borders::sync_borders,
                    render::update_fog,
                    render::sync_cover,
                    ui::update_hover_label,
                    (
                        actionbar::update_bar,
                        actionbar::update_gold,
                        advisors::update_science_line,
                        advisors::show,
                        domestic::show,
                        leaders::animate,
                        wonders::fanfare,
                    )
                        .chain(),
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
            (naval::unit_hazards, bombard::resolve, combat::start_attacks, combat::run_combat)
                .chain()
                .after(units::drive_movement)
                .before(units::advance_anims),
        )
        .add_systems(Update, naval::sync_cargo.after(combat::run_combat).before(units::advance_anims))
        .add_systems(Update, combat::sync_health_bars.after(units::animate_units))
        .add_systems(Update, units::sync_art_eras.before(units::animate_units))
        // Saving and loading change the whole world, so they run on their own,
        // before the turn's systems look at it.
        .add_systems(First, (save::keys, save::run).chain())
        .add_systems(Update, cities::frame_city_view.after(cities::maintain_city_screen))
        .add_systems(Update, cities::highlight_menu_rows)
        .add_systems(Update, (advisor_frame::hover_art, advisor_frame::switch_tabs));
    #[cfg(target_arch = "wasm32")]
    web::show_progress("app run");
    app.run();
}

fn asset_plugin() -> bevy::asset::AssetPlugin {
    #[cfg(target_arch = "wasm32")]
    {
        let mut plugin = bevy::asset::AssetPlugin::default();
        plugin.meta_check = bevy::asset::AssetMetaCheck::Never;
        plugin
    }
    #[cfg(not(target_arch = "wasm32"))]
    bevy::asset::AssetPlugin::default()
}

fn fail(message: &str) -> ! {
    eprintln!("open-4x: {message}");
    #[cfg(target_arch = "wasm32")]
    panic!("open-4x: {message}");
    #[cfg(not(target_arch = "wasm32"))]
    std::process::exit(1);
}

#[cfg(not(target_arch = "wasm32"))]
fn load_boot() -> Result<boot::Boot, String> {
    boot::load(cli::init())
}

#[cfg(target_arch = "wasm32")]
fn load_boot() -> Result<boot::Boot, String> {
    boot::load_web(cli::options())
}

fn setup_camera(mut commands: Commands, map: Res<GameMap>) {
    #[cfg(target_arch = "wasm32")]
    web::show_ready();
    // Debug: MAP_CENTER=x,y starts the camera over a tile (like MAP_SEED).
    let center = std::env::var("MAP_CENTER")
        .ok()
        .and_then(|s| {
            let (x, y) = s.split_once(',')?;
            Some((x.parse().ok()?, y.parse().ok()?))
        })
        .or_else(populate::human_start)
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
    commands.insert_resource(improvements::ImprovementArt::load());
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
