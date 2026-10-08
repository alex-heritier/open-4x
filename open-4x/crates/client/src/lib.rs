mod connection;
mod presentation;

use bevy::{input::mouse::MouseWheel, prelude::*, window::PrimaryWindow};
use connection::{Connection, Event};
use fourx_content::Pack;
use fourx_runtime::Host;
use fourx_sim::{Command, Game, Id, PROTOCOL_VERSION, Request, Response, Rules, terrain::Coord};

#[derive(Resource)]
struct Session {
    game: Option<Game>,
    rules: Rules,
    pack: Pack,
    player: Id,
    selection: Selection,
    sequence: u64,
    pending: bool,
    dirty: bool,
    message: String,
    asset_prefix: String,
    frames: u32,
    #[cfg(not(target_arch = "wasm32"))]
    screenshot: Option<String>,
    smoke: bool,
    previous_pinch: Option<f32>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Selection {
    #[default]
    None,
    Army(Id),
    City(Id),
    Tile(Coord),
}

#[derive(Component)]
struct WorldVisual;
#[derive(Component)]
struct UiRoot;
#[derive(Component)]
struct Inspector;
#[derive(Component, Clone)]
enum Action {
    EndTurn,
    Found,
    Develop,
    Produce(String),
    Save,
    SelectArmy(Id),
    SelectCity(Id),
    Center,
}

#[cfg(not(target_arch = "wasm32"))]
fn save_path() -> std::path::PathBuf {
    #[cfg(target_os = "android")]
    {
        return bevy::android::ANDROID_APP
            .get()
            .unwrap()
            .internal_data_path()
            .unwrap()
            .join("dawn.save.json");
    }
    #[cfg(target_os = "ios")]
    {
        return std::path::PathBuf::from(std::env::var("HOME").expect("iOS app home"))
            .join("Documents/dawn.save.json");
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        std::path::PathBuf::from("dawn.save.json")
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn default_asset_root() -> String {
    #[cfg(target_os = "android")]
    {
        return "assets".into();
    }
    #[cfg(not(target_os = "android"))]
    {
        let bundled = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join("assets");
        if bundled.is_dir() {
            return bundled.to_string_lossy().into_owned();
        }
        let source = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        if source.is_dir() {
            return source.to_string_lossy().into_owned();
        }
        "assets".into()
    }
}
#[derive(Resource)]
struct Art {
    atlas: Handle<TextureAtlasLayout>,
}

fn option(flag: &str) -> Option<String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::env::args()
            .collect::<Vec<_>>()
            .windows(2)
            .find(|a| a[0] == flag)
            .map(|a| a[1].clone())
    }
    #[cfg(target_arch = "wasm32")]
    {
        let key = flag.trim_start_matches('-');
        let search = web_sys::window()?.location().search().ok()?;
        web_sys::UrlSearchParams::new_with_str(&search)
            .ok()?
            .get(key)
    }
}
fn flag(flag: &str) -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::env::args().any(|a| a == flag)
    }
    #[cfg(target_arch = "wasm32")]
    {
        option(flag).is_some()
    }
}

pub fn run() {
    let server = option("--server");
    let seed = option("--seed").and_then(|s| s.parse().ok()).unwrap_or(42);
    let pack = Pack::base();
    let asset_prefix = server
        .as_ref()
        .map(|s| {
            format!(
                "{}/assets/",
                s.trim_end_matches("/ws")
                    .replacen("ws://", "http://", 1)
                    .replacen("wss://", "https://", 1)
            )
        })
        .unwrap_or("packs/base/".into());
    let connection = if let Some(server) = server {
        Connection::remote(server, option("--token").unwrap_or_default())
    } else {
        #[cfg(not(target_arch = "wasm32"))]
        let host = if let Some(path) = option("--load") {
            Host::load(&std::fs::read_to_string(path).expect("read save")).expect("valid save")
        } else if let Some(path) = option("--pack") {
            let (p, s) = fourx_content::load_directory(std::path::Path::new(&path))
                .expect("valid pack directory");
            Host::new(p, s, seed).expect("valid campaign script")
        } else {
            #[cfg(any(target_os = "android", target_os = "ios"))]
            let saved = std::fs::read_to_string(save_path())
                .ok()
                .and_then(|s| Host::load(&s).ok());
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            let saved: Option<Host> = None;
            saved.unwrap_or_else(|| Host::base(seed).expect("valid bundled campaign"))
        };
        #[cfg(target_arch = "wasm32")]
        let host = if option("--load").as_deref() == Some("browser") {
            web_sys::window()
                .and_then(|w| w.local_storage().ok().flatten())
                .and_then(|s| s.get_item("open-4x-save").ok().flatten())
                .and_then(|s| Host::load(&s).ok())
                .unwrap_or_else(|| Host::base(seed).expect("valid bundled campaign"))
        } else {
            Host::base(seed).expect("valid bundled campaign")
        };
        Connection::local(host)
    };
    #[cfg(not(target_arch = "wasm32"))]
    let (asset_root, asset_prefix) = if let Some(path) = option("--pack") {
        (
            std::fs::canonicalize(path)
                .expect("pack path")
                .to_string_lossy()
                .into_owned(),
            if option("--server").is_none() {
                String::new()
            } else {
                asset_prefix
            },
        )
    } else {
        (default_asset_root(), asset_prefix)
    };
    #[cfg(target_arch = "wasm32")]
    let asset_root = "assets".to_string();
    let mut app = App::new();
    app.insert_resource(ClearColor(Color::srgb(0.055, 0.13, 0.15)))
        .insert_resource(Session {
            game: None,
            rules: pack.rules.clone(),
            pack,
            player: 1,
            selection: Selection::None,
            sequence: 0,
            pending: false,
            dirty: true,
            message: "Connecting to the campaign...".into(),
            asset_prefix,
            frames: 0,
            #[cfg(not(target_arch = "wasm32"))]
            screenshot: option("--screenshot"),
            smoke: flag("--smoke"),
            previous_pinch: None,
        })
        .insert_non_send_resource(connection)
        .add_plugins(
            DefaultPlugins
                .set(AssetPlugin {
                    file_path: asset_root,
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Open 4X | Dawn over the Straits".into(),
                        resolution: (1440, 900).into(),
                        canvas: Some("#game".into()),
                        fit_canvas_to_parent: true,
                        prevent_default_event_handling: true,
                        ..default()
                    }),
                    ..default()
                }),
        )
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                receive,
                buttons,
                scroll_inspector,
                controls,
                presentation::refresh,
                presentation::draw_overlays,
                smoke,
            )
                .chain(),
        );
    app.run();
}

// Android's launcher loads this symbol from the cdylib. iOS calls `run()` from its app wrapper.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: bevy::android::android_activity::AndroidApp) {
    bevy::android::ANDROID_APP
        .set(app)
        .expect("Android app initialized once");
    run();
}

#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn fourx_ios_main() {
    run();
}

fn setup(mut commands: Commands, mut layouts: ResMut<Assets<TextureAtlasLayout>>) {
    commands.spawn((
        Camera2d,
        Transform::from_xyz(240.0, -544.0, 0.0),
        Projection::Orthographic(OrthographicProjection {
            scale: 1.25,
            ..OrthographicProjection::default_2d()
        }),
    ));
    commands.insert_resource(Art {
        atlas: layouts.add(TextureAtlasLayout::from_grid(
            UVec2::new(128, 64),
            9,
            9,
            None,
            None,
        )),
    });
}
fn receive(
    mut session: ResMut<Session>,
    mut connection: NonSendMut<Connection>,
    mut commands: Commands,
    assets: Res<AssetServer>,
) {
    for event in connection.poll() {
        match event {
            Event::Content(pack) => {
                session.pack = pack;
                session.dirty = true;
            }
            Event::Error(error) => {
                session.message = error;
                session.pending = false;
                session.dirty = true;
            }
            Event::Response(Response::Snapshot {
                version,
                player,
                game,
                rules,
            }) => {
                if version != PROTOCOL_VERSION {
                    session.message = "Incompatible server protocol".into();
                    continue;
                }
                let newer = session
                    .game
                    .as_ref()
                    .is_none_or(|old| old.revision != game.revision);
                if session
                    .game
                    .as_ref()
                    .is_some_and(|old| game.turn > old.turn)
                {
                    commands.spawn((
                        AudioPlayer::new(assets.load(format!(
                            "{}{}",
                            session.asset_prefix, session.pack.visuals.turn_sound
                        ))),
                        PlaybackSettings::DESPAWN,
                    ));
                }
                session.player = player;
                session.game = Some(game);
                session.rules = rules;
                session.pending = false;
                if newer {
                    session.dirty = true;
                    session.message = if player == 0 {
                        "Spectator | another client commands this campaign".into()
                    } else {
                        "Select an army; click a destination to march. Space advances one day."
                            .into()
                    };
                }
            }
            Event::Response(Response::Rejected { reason, .. }) => {
                session.pending = false;
                session.message = reason;
                session.dirty = true;
            }
        }
    }
}
fn issue(session: &mut Session, connection: &mut Connection, command: Command) {
    if session.pending || session.game.is_none() {
        return;
    }
    if session.player == 0 {
        session.message = "Spectators cannot issue orders".into();
        session.dirty = true;
        return;
    }
    session.sequence += 1;
    session.pending = true;
    connection.send(Request {
        version: PROTOCOL_VERSION,
        sequence: session.sequence,
        revision: session.game.as_ref().unwrap().revision,
        command,
    });
}
fn act(action: &Action, session: &mut Session, connection: &mut Connection) {
    let command = match action {
        Action::EndTurn => Some(Command::EndTurn),
        Action::Found => {
            if let Selection::Army(army) = session.selection {
                Some(Command::FoundCity {
                    army,
                    name: format!(
                        "New Haven {}",
                        session.game.as_ref().unwrap().cities.len() + 1
                    ),
                })
            } else {
                None
            }
        }
        Action::Develop => {
            if let Selection::City(city) = session.selection {
                Some(Command::Develop { city })
            } else {
                None
            }
        }
        Action::Produce(unit) => {
            if let Selection::City(city) = session.selection {
                Some(Command::Produce {
                    city,
                    unit: unit.clone(),
                })
            } else {
                None
            }
        }
        Action::SelectArmy(id) => {
            session.selection = Selection::Army(*id);
            session.dirty = true;
            None
        }
        Action::SelectCity(id) => {
            session.selection = Selection::City(*id);
            session.dirty = true;
            None
        }
        Action::Center => None,
        Action::Save => {
            match connection.save() {
                Ok(json) => {
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        let path = save_path();
                        session.message = match std::fs::write(&path, json) {
                            Ok(_) => format!("Saved to {}", path.display()),
                            Err(e) => e.to_string(),
                        };
                    }
                    #[cfg(target_arch = "wasm32")]
                    {
                        session.message = match web_sys::window()
                            .and_then(|w| w.local_storage().ok().flatten())
                        {
                            Some(storage) => match storage.set_item("open-4x-save", &json) {
                                Ok(_) => {
                                    "Saved in this browser. Use ?load=browser to resume.".into()
                                }
                                Err(_) => "Browser storage is unavailable".into(),
                            },
                            None => "Browser storage is unavailable".into(),
                        };
                    }
                }
                Err(e) => session.message = e,
            }
            session.dirty = true;
            None
        }
    };
    if let Some(command) = command {
        issue(session, connection, command);
    }
}
fn buttons(
    mut session: ResMut<Session>,
    mut connection: NonSendMut<Connection>,
    mut buttons: Query<(&Interaction, &Action, &mut BackgroundColor), Changed<Interaction>>,
    mut camera: Query<&mut Transform, With<Camera2d>>,
    touches: Res<Touches>,
    mut touch_action: Local<Option<Action>>,
) {
    let mut activated = Vec::new();
    for (interaction, action, mut color) in &mut buttons {
        color.0 = match interaction {
            Interaction::Hovered => Color::srgb(0.32, 0.36, 0.29),
            Interaction::Pressed => Color::srgb(0.45, 0.37, 0.20),
            _ => Color::srgb(0.12, 0.19, 0.20),
        };
        if *interaction == Interaction::Pressed {
            if touches.iter().next().is_some() {
                *touch_action = Some(action.clone());
            } else {
                activated.push(action.clone());
            }
        }
    }
    if let Some(touch) = touches.iter_just_released().next() {
        if let Some(action) = touch_action.take()
            && touch.position().distance(touch.start_position()) < 12.0
        {
            activated.push(action);
        }
    }
    for action in &activated {
        if matches!(action, Action::Center) {
            camera.single_mut().unwrap().translation = Vec3::new(240.0, -544.0, 0.0);
        }
        act(action, &mut session, &mut connection);
        let pos = match action {
            Action::SelectArmy(id) => session
                .game
                .as_ref()
                .and_then(|g| g.armies.get(id))
                .map(|a| a.position),
            Action::SelectCity(id) => session
                .game
                .as_ref()
                .and_then(|g| g.cities.get(id))
                .map(|c| c.position),
            _ => None,
        };
        if let Some(pos) = pos {
            let (x, y) = pos.screen();
            camera.single_mut().unwrap().translation = Vec3::new(x + 160.0, y, 0.0);
        }
    }
}
fn inspector_contains(window: &Window, pos: Vec2) -> bool {
    if window.width() < 900.0 {
        pos.y > window.height() - inspector_height(window.height()) - 52.0
            && pos.y < window.height() - 52.0
    } else {
        pos.x >= 16.0 && pos.x <= 282.0 && pos.y >= 88.0 && pos.y <= window.height() - 166.0
    }
}
fn inspector_height(window_height: f32) -> f32 {
    (window_height * 0.30).clamp(110.0, 240.0)
}
fn scroll_inspector(
    mut wheel: MessageReader<MouseWheel>,
    touches: Res<Touches>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut inspector: Query<(&ComputedNode, &mut ScrollPosition), With<Inspector>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let mut delta = 0.0;
    for event in wheel.read() {
        if window
            .cursor_position()
            .is_some_and(|p| inspector_contains(window, p))
        {
            delta -= event.y * 30.0;
        }
    }
    for touch in touches.iter() {
        if inspector_contains(window, touch.start_position()) {
            delta -= touch.delta().y;
        }
    }
    for (node, mut scroll) in &mut inspector {
        let max = (node.content_size().y - node.size().y) * node.inverse_scale_factor();
        scroll.y = (scroll.y + delta).clamp(0.0, max.max(0.0));
    }
}
fn controls(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    time: Res<Time>,
    mut wheel: MessageReader<MouseWheel>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut camera: Query<(&Camera, &GlobalTransform, &mut Transform, &mut Projection), With<Camera2d>>,
    ui: Query<&Interaction, With<Button>>,
    mut session: ResMut<Session>,
    mut connection: NonSendMut<Connection>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let Ok((camera, global, mut transform, mut projection)) = camera.single_mut() else {
        return;
    };
    let Projection::Orthographic(ref mut ortho) = *projection else {
        return;
    };
    let step = time.delta_secs() * 650.0 * ortho.scale;
    if keys.pressed(KeyCode::ArrowLeft) || keys.pressed(KeyCode::KeyA) {
        transform.translation.x -= step;
    }
    if keys.pressed(KeyCode::ArrowRight) || keys.pressed(KeyCode::KeyD) {
        transform.translation.x += step;
    }
    if keys.pressed(KeyCode::ArrowUp) || keys.pressed(KeyCode::KeyW) {
        transform.translation.y += step;
    }
    if keys.pressed(KeyCode::ArrowDown) || keys.pressed(KeyCode::KeyS) {
        transform.translation.y -= step;
    }
    let over_inspector = window
        .cursor_position()
        .is_some_and(|p| inspector_contains(window, p));
    for scroll in wheel.read() {
        if !over_inspector {
            ortho.scale = (ortho.scale * (1.0 - scroll.y * 0.08)).clamp(0.4, 3.0);
        }
    }
    let fingers: Vec<_> = touches.iter().collect();
    if fingers.len() == 2 {
        let distance = fingers[0].position().distance(fingers[1].position());
        if let Some(last) = session.previous_pinch {
            ortho.scale = (ortho.scale * last / distance.max(1.0)).clamp(0.4, 3.0);
        }
        session.previous_pinch = Some(distance);
        let delta = (fingers[0].delta() + fingers[1].delta()) * 0.5 * ortho.scale;
        transform.translation.x -= delta.x;
        transform.translation.y += delta.y;
    } else {
        session.previous_pinch = None;
    }
    if session.game.is_none() {
        return;
    }
    if keys.just_pressed(KeyCode::Space) {
        act(&Action::EndTurn, &mut session, &mut connection);
    }
    if keys.just_pressed(KeyCode::KeyF) {
        act(&Action::Found, &mut session, &mut connection);
    }
    if keys.just_pressed(KeyCode::KeyE) {
        act(&Action::Develop, &mut session, &mut connection);
    }
    if keys.just_pressed(KeyCode::F5) {
        act(&Action::Save, &mut session, &mut connection);
    }
    if keys.just_pressed(KeyCode::Escape) {
        session.selection = Selection::None;
        session.dirty = true;
    }
    if keys.just_pressed(KeyCode::Tab) {
        let ids: Vec<_> = session
            .game
            .as_ref()
            .unwrap()
            .armies
            .values()
            .filter(|a| a.owner == session.player)
            .map(|a| a.id)
            .collect();
        if !ids.is_empty() {
            let next = if let Selection::Army(id) = session.selection {
                (ids.iter().position(|v| *v == id).unwrap_or(0) + 1) % ids.len()
            } else {
                0
            };
            session.selection = Selection::Army(ids[next]);
            session.dirty = true;
        }
    }
    if keys.just_pressed(KeyCode::KeyC) {
        let ids: Vec<_> = session
            .game
            .as_ref()
            .unwrap()
            .cities
            .values()
            .filter(|c| c.owner == session.player)
            .map(|c| c.id)
            .collect();
        if !ids.is_empty() {
            let next = if let Selection::City(id) = session.selection {
                (ids.iter().position(|v| *v == id).unwrap_or(0) + 1) % ids.len()
            } else {
                0
            };
            session.selection = Selection::City(ids[next]);
            session.dirty = true;
        }
    }
    for (index, key) in [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
    ]
    .into_iter()
    .enumerate()
    {
        if keys.just_pressed(key)
            && let Some(unit) = session.rules.units.keys().nth(index).cloned()
        {
            act(&Action::Produce(unit), &mut session, &mut connection);
        }
    }
    let touch = touches
        .iter_just_released()
        .find(|t| t.position().distance(t.start_position()) < 12.0)
        .map(|t| t.position());
    let clicked = mouse.just_pressed(MouseButton::Left)
        || mouse.just_pressed(MouseButton::Right)
        || touch.is_some();
    if !clicked || fingers.len() > 1 || ui.iter().any(|i| *i != Interaction::None) {
        return;
    }
    let Some(cursor) = touch.or_else(|| window.cursor_position()) else {
        return;
    };
    // Panels also block map clicks in their empty space.
    let narrow = window.width() < 900.0;
    if cursor.y < 76.0
        || cursor.y > window.height() - 56.0
        || (!narrow && cursor.x < 300.0)
        || (narrow && cursor.y > window.height() - inspector_height(window.height()) - 56.0)
    {
        return;
    }
    let Ok(world) = camera.viewport_to_world_2d(global, cursor) else {
        return;
    };
    let position = Coord::from_screen(world.x, world.y);
    let game = session.game.as_ref().unwrap();
    if game.map.get(position).is_none() {
        return;
    }
    let own: Vec<_> = game
        .armies
        .values()
        .filter(|a| a.position == position && a.owner == session.player)
        .map(|a| a.id)
        .collect();
    let city = game
        .cities
        .values()
        .find(|c| c.position == position)
        .map(|c| c.id);
    if mouse.just_pressed(MouseButton::Right) {
        if let Selection::Army(army) = session.selection {
            issue(
                &mut session,
                &mut connection,
                Command::Move {
                    army,
                    destination: position,
                },
            );
        }
        return;
    }
    if !own.is_empty() {
        let next = if let Selection::Army(id) = session.selection {
            (own.iter().position(|v| *v == id).unwrap_or(own.len() - 1) + 1) % own.len()
        } else {
            0
        };
        session.selection = Selection::Army(own[next]);
    } else if let Selection::Army(army) = session.selection {
        issue(
            &mut session,
            &mut connection,
            Command::Move {
                army,
                destination: position,
            },
        );
    } else if let Some(id) = city {
        session.selection = Selection::City(id);
    } else {
        session.selection = Selection::Tile(position);
    }
    session.dirty = true;
}
fn smoke(mut session: ResMut<Session>, mut _commands: Commands, mut exit: MessageWriter<AppExit>) {
    session.frames += 1;
    #[cfg(not(target_arch = "wasm32"))]
    if session.frames == 120
        && let Some(path) = &session.screenshot
    {
        _commands
            .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
            .observe(bevy::render::view::screenshot::save_to_disk(path.clone()));
    }
    if session.smoke && session.frames >= 200 {
        exit.write(AppExit::Success);
    }
}
