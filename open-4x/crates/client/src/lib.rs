mod bar;
mod city;
mod connection;
mod presentation;
mod route;
mod stage;

use bevy::{
    input::mouse::MouseWheel, prelude::*, ui::RelativeCursorPosition, window::PrimaryWindow,
};
use connection::{Connection, Event};
use fourx_content::{Content, Pack};
use fourx_runtime::{Host, Start};
use fourx_sim::{
    Command, Game, Id, Job, PROTOCOL_VERSION, Request, Response, Rules,
    terrain::{Coord, Lattice, Map},
};

/// Zoom limits. The whole world never fits one screen; the minimap covers overview.
const MIN_ZOOM: f32 = 0.4;
const MAX_ZOOM: f32 = 4.0;
/// What shows behind a map that has no void of its own: deep water.
const BACKDROP: Color = Color::srgb(0.055, 0.13, 0.15);

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
    /// The part of the world the sprites currently cover. Only the map near the camera is
    /// ever spawned, and it is rebuilt when the camera leaves it.
    built: Option<WorldRect>,
    /// The camera starts on the player's capital once the first snapshot arrives.
    centered: bool,
    message: String,
    asset_prefix: String,
    frames: u32,
    #[cfg(not(target_arch = "wasm32"))]
    screenshot: Option<String>,
    smoke: bool,
    previous_pinch: Option<f32>,
    /// A recorded fight is playing or waiting: orders are held back until it is over.
    staging: bool,
}

/// A rectangle of the world in screen units (what the camera shows). Only tiles, cities and
/// units inside one are ever spawned, because a whole world is hundreds of thousands of
/// sprites.
#[derive(Clone, Copy, Debug, PartialEq)]
struct WorldRect {
    min: Vec2,
    max: Vec2,
}
impl WorldRect {
    fn contains(&self, other: &WorldRect) -> bool {
        self.min.x <= other.min.x
            && self.min.y <= other.min.y
            && self.max.x >= other.max.x
            && self.max.y >= other.max.y
    }
    fn has(&self, p: Vec2) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }
    fn grow(&self, by: f32) -> WorldRect {
        WorldRect {
            min: self.min - Vec2::splat(by),
            max: self.max + Vec2::splat(by),
        }
    }
    /// Bounding box in tile space as `(x0, y0, x1, y1)`, fractional and not clamped. The
    /// rectangle is a rotated square there.
    fn tile_box(&self) -> (Vec2, Vec2) {
        let mut lo = Vec2::splat(f32::MAX);
        let mut hi = Vec2::splat(f32::MIN);
        for corner in [
            self.min,
            self.max,
            Vec2::new(self.min.x, self.max.y),
            Vec2::new(self.max.x, self.min.y),
        ] {
            let t = tile_at(corner.x, corner.y);
            lo = lo.min(t);
            hi = hi.max(t);
        }
        (lo, hi)
    }
    /// Inclusive tile ranges clamped to the map: `(x0, y0, x1, y1)`.
    fn tile_bounds(&self, game: &Game) -> (i32, i32, i32, i32) {
        let (lo, hi) = self.tile_box();
        (
            (lo.x.floor() as i32).max(0),
            (lo.y.floor() as i32).max(0),
            (hi.x.ceil() as i32).min(game.map.width - 1),
            (hi.y.ceil() as i32).min(game.map.height - 1),
        )
    }
}

/// Tile-space position of a point in the world (fractional; the inverse of `Coord::screen`).
fn tile_at(x: f32, y: f32) -> Vec2 {
    Vec2::new(x / 128.0 - y / 64.0, -x / 128.0 - y / 64.0)
}

/// What the camera currently shows.
fn camera_rect(camera: &Transform, projection: &Projection, window: &Window) -> WorldRect {
    let scale = match projection {
        Projection::Orthographic(o) => o.scale,
        _ => 1.0,
    };
    let half = Vec2::new(window.width(), window.height()) * 0.5 * scale;
    let centre = camera.translation.truncate();
    WorldRect {
        min: centre - half,
        max: centre + half,
    }
}

/// Where the camera starts for a player: their capital, else any city of theirs.
fn home(game: &Game, player: Id) -> Option<Coord> {
    // Spectators (player 0) watch the nation the campaign is commanded by.
    let owner = if player == 0 { game.commander } else { player };
    let own = |c: &&fourx_sim::City| c.owner == owner;
    game.cities
        .values()
        .filter(own)
        .find(|c| c.capital)
        .or_else(|| game.cities.values().find(own))
        .map(|c| c.position)
}

/// Move the camera onto a tile.
fn focus(transform: &mut Transform, position: Coord) {
    let (x, y) = position.screen();
    transform.translation = Vec3::new(x, y, transform.translation.z);
}

/// The overview image: one pixel per tile, rewritten when the game state changes.
#[derive(Resource)]
struct Minimap {
    image: Handle<Image>,
    /// Revision last drawn (`u64::MAX` before the first draw).
    drawn: u64,
}

/// How the minimap lays a map out, one pixel per tile. A lattice map is drawn as the upright
/// rectangle it is: native column `cx` is pixel column `cx`, and the two rows `2 * py` and
/// `2 * py + 1` share pixel row `py` (the parity of the row follows the column, so no two tiles
/// share a pixel). Any other map is drawn as the square grid it is.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Chart {
    width: u32,
    height: u32,
    lattice: Option<Lattice>,
}
impl Chart {
    fn of(map: &Map) -> Chart {
        match map.lattice {
            Some(lattice) => Chart {
                width: 2 * lattice.columns as u32,
                height: (lattice.rows / 2) as u32,
                lattice: Some(lattice),
            },
            None => Chart {
                width: map.width.max(1) as u32,
                height: map.height.max(1) as u32,
                lattice: None,
            },
        }
    }
    /// The pixel of a tile; outside the image for anything that is not on the map.
    fn pixel(&self, p: Coord) -> (i32, i32) {
        match self.lattice {
            Some(lattice) => {
                let (cx, cy) = lattice.native(p);
                (cx, cy.div_euclid(2))
            }
            None => (p.x, p.y),
        }
    }
    /// The tile a pixel stands for.
    fn tile(&self, px: i32, py: i32) -> Coord {
        let (px, py) = (
            px.clamp(0, self.width as i32 - 1),
            py.clamp(0, self.height as i32 - 1),
        );
        match self.lattice {
            Some(lattice) => lattice.cell(px, 2 * py + (px & 1)),
            None => Coord::new(px, py),
        }
    }
    /// The tile under a point of the image, `0..1` across and down.
    fn tile_at(&self, unit: Vec2) -> Coord {
        self.tile(
            (unit.x * self.width as f32).floor() as i32,
            (unit.y * self.height as f32).floor() as i32,
        )
    }
    /// The part of the image a camera rectangle covers: its corners, `0..1` across and down,
    /// clamped to the image.
    fn view(&self, rect: WorldRect) -> (Vec2, Vec2) {
        let (lo, hi) = match self.lattice {
            Some(lattice) => {
                let columns = lattice.columns as f32;
                let unit = |p: Vec2| {
                    Vec2::new(
                        (p.x / 64.0 + columns + 0.5) / (2.0 * columns),
                        (-p.y / 32.0 - columns + 0.5) / lattice.rows as f32,
                    )
                };
                let (a, b) = (unit(rect.min), unit(rect.max));
                (a.min(b), a.max(b))
            }
            None => {
                let (lo, hi) = rect.tile_box();
                let size = Vec2::new(self.width as f32, self.height as f32);
                (lo / size, hi / size)
            }
        };
        (
            lo.clamp(Vec2::ZERO, Vec2::ONE),
            hi.clamp(Vec2::ZERO, Vec2::ONE),
        )
    }
}
#[derive(Component)]
struct MinimapSurface;
#[derive(Component)]
struct MinimapView;
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Selection {
    #[default]
    None,
    Unit(Id),
    City(Id),
    Tile(Coord),
}

#[derive(Component)]
struct WorldVisual;
#[derive(Component)]
struct UiRoot;
#[derive(Component)]
struct Inspector;
/// The pane at the bottom right that describes the selection (see `presentation::info`). It
/// carries an `Interaction` so that a click on it is not a click on the map under it.
#[derive(Component)]
struct InfoPane;
#[derive(Component, Clone)]
enum Action {
    EndTurn,
    Found,
    Fortify,
    Cancel,
    Disband,
    Work(Job),
    /// Attack or bombard the square, whichever the selected unit does.
    Strike(Coord),
    Develop,
    Produce(String),
    Save,
    SelectUnit(Id),
    SelectCity(Id),
    /// Close the city's detail view.
    CloseCity,
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
/// Terrain sheet geometry. The ground and water sheets are 16 columns of 2:1 diamond cells: 16
/// rows of mixed cells, then (optionally) rows of tonal variants of the pure ones. The river sheet
/// is 16 branch masks wide with one row per meander. Layouts are re-cut from the loaded images,
/// so packs may ship 1x, 2x or finer art.
#[derive(Resource)]
struct Art {
    /// Layout of the ground and water sheets (the same grid).
    cells: Handle<TextureAtlasLayout>,
    /// Layout of the river sheet.
    rivers: Handle<TextureAtlasLayout>,
    /// Layout of the fog sheet: 9 x 9 diamonds, see [`presentation::fog_index`].
    fog: Handle<TextureAtlasLayout>,
    /// Layout of the culture-border sheet: four diamonds, one per edge.
    borders: Handle<TextureAtlasLayout>,
    /// Each surface-border sheet can have its own resolution.
    relief_borders: std::collections::HashMap<String, (UVec2, Handle<TextureAtlasLayout>)>,
    /// Image sizes the layouts were last cut for.
    ground_sheet: UVec2,
    river_sheet: UVec2,
    fog_sheet: UVec2,
    border_sheet: UVec2,
    /// Tonal variants per pure cell (0 when the sheets have only the 16 base rows).
    variants: usize,
    /// Meander variants of the river cells.
    meanders: usize,
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

/// The embedded campaign: a save when asked for, otherwise a fresh start of `--scenario` as
/// `--nation` (the pack's default scenario and that scenario's commander when omitted).
fn local_host(seed: u64) -> Host {
    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Some(path) = option("--load") {
            let json = std::fs::read_to_string(path).expect("read save");
            return Host::load(&json).expect("valid save");
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        if option("--load").as_deref() == Some("browser")
            && let Some(host) = web_sys::window()
                .and_then(|w| w.local_storage().ok().flatten())
                .and_then(|s| s.get_item("open-4x-save").ok().flatten())
                .and_then(|s| Host::load(&s).ok())
        {
            return host;
        }
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        if let Some(host) = std::fs::read_to_string(save_path())
            .ok()
            .and_then(|s| Host::load(&s).ok())
        {
            return host;
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let content = match option("--pack") {
        Some(path) => fourx_content::load_directory(std::path::Path::new(&path))
            .expect("valid pack directory"),
        None => Content::base(),
    };
    #[cfg(target_arch = "wasm32")]
    let content = Content::base();
    let (scenario, nation) = (option("--scenario"), option("--nation"));
    Host::start(
        content,
        Start {
            scenario: scenario.as_deref(),
            nation: nation.as_deref(),
            seed,
        },
    )
    .unwrap_or_else(|error| panic!("cannot start the campaign: {error}"))
}

/// `--select unit:ID`, `--select city:ID` or `--select tile:X,Y` starts with that piece selected, which is how the
/// inspector is captured in screenshots.
fn initial_selection() -> Selection {
    let Some(text) = option("--select") else {
        return Selection::None;
    };
    match text.split_once(':') {
        Some(("unit", id)) => id.parse().map_or(Selection::None, Selection::Unit),
        Some(("city", id)) => id.parse().map_or(Selection::None, Selection::City),
        Some(("tile", at)) => at
            .split_once(',')
            .and_then(|(x, y)| Some(Coord::new(x.parse().ok()?, y.parse().ok()?)))
            .map_or(Selection::None, Selection::Tile),
        _ => Selection::None,
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
        Connection::local(local_host(seed))
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
    app.insert_resource(ClearColor(BACKDROP))
        .insert_resource(Session {
            game: None,
            rules: pack.rules.clone(),
            pack,
            player: 1,
            selection: initial_selection(),
            sequence: 0,
            pending: false,
            dirty: true,
            built: None,
            centered: false,
            message: "Connecting to the campaign...".into(),
            asset_prefix,
            frames: 0,
            #[cfg(not(target_arch = "wasm32"))]
            screenshot: option("--screenshot"),
            smoke: flag("--smoke"),
            previous_pinch: None,
            staging: false,
        })
        .insert_resource(stage::Stage::new())
        .insert_non_send_resource(connection)
        .add_plugins(
            DefaultPlugins
                .set(AssetPlugin {
                    file_path: asset_root,
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Open 4X".into(),
                        resolution: (1440, 900).into(),
                        canvas: Some("#game".into()),
                        fit_canvas_to_parent: true,
                        prevent_default_event_handling: true,
                        ..default()
                    }),
                    ..default()
                }),
        )
        .add_systems(Startup, (setup, stage::setup))
        .add_systems(
            Update,
            (
                receive,
                buttons,
                bar::update,
                city::update,
                city::scroll,
                scroll_inspector,
                minimap_click,
                controls,
                // After the input, so a Space that skips a fight is not also an order; before
                // `refresh`, so the real units are hidden in the frame a fight begins.
                stage::drive,
                stage::fx,
                presentation::fit_terrain_atlas,
                presentation::refresh,
                presentation::update_minimap,
                presentation::minimap_viewport,
                presentation::draw_overlays,
                smoke,
            )
                .chain(),
        );
    app.insert_gizmo_config(route::RouteGizmos, route::gizmo_config());
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

fn setup(
    mut commands: Commands,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut images: ResMut<Assets<Image>>,
) {
    let zoom = option("--zoom")
        .and_then(|z| z.parse::<f32>().ok())
        .unwrap_or(1.25)
        .clamp(MIN_ZOOM, MAX_ZOOM);
    // The camera is moved onto the player's capital when the first snapshot arrives.
    commands.spawn((
        Camera2d,
        Transform::from_xyz(0.0, 0.0, 0.0),
        Projection::Orthographic(OrthographicProjection {
            scale: zoom,
            ..OrthographicProjection::default_2d()
        }),
    ));
    commands.insert_resource(Minimap {
        image: images.add(Image::new_fill(
            bevy::render::render_resource::Extent3d::default(),
            bevy::render::render_resource::TextureDimension::D2,
            &[0, 0, 0, 0],
            bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
            bevy::asset::RenderAssetUsages::default(),
        )),
        drawn: u64::MAX,
    });
    commands.insert_resource(Art {
        relief_borders: Default::default(),
        fog: layouts.add(TextureAtlasLayout::from_grid(
            UVec2::new(256, 128),
            presentation::FOG_COLUMNS,
            presentation::FOG_COLUMNS,
            None,
            None,
        )),
        borders: layouts.add(TextureAtlasLayout::from_grid(
            UVec2::new(256, 128),
            4,
            1,
            None,
            None,
        )),
        fog_sheet: UVec2::ZERO,
        border_sheet: UVec2::ZERO,
        cells: layouts.add(TextureAtlasLayout::from_grid(
            UVec2::new(192, 96),
            presentation::SHEET_COLUMNS,
            presentation::SHEET_COLUMNS,
            None,
            None,
        )),
        rivers: layouts.add(TextureAtlasLayout::from_grid(
            UVec2::new(192, 96),
            presentation::SHEET_COLUMNS,
            1,
            None,
            None,
        )),
        ground_sheet: UVec2::ZERO,
        river_sheet: UVec2::ZERO,
        variants: 0,
        meanders: 1,
    });
}
fn receive(
    mut session: ResMut<Session>,
    mut connection: NonSendMut<Connection>,
    mut stage: ResMut<stage::Stage>,
    mut commands: Commands,
    mut clear: ResMut<ClearColor>,
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
                mut game,
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
                // Snapshots carry no lookup tables; build them so queries stay cheap.
                game.reindex();
                // Beyond the edge of an upright world is nothing, and nothing is black.
                let backdrop = if game.map.lattice.is_some() {
                    Color::BLACK
                } else {
                    BACKDROP
                };
                if clear.0 != backdrop {
                    clear.0 = backdrop;
                }
                // A newer state brings the fights of the command that made it. The first
                // snapshot is only the starting point, so whatever a save held is not replayed.
                if newer && session.game.is_some() && !game.battles.is_empty() {
                    stage.enqueue(&game.battles);
                    session.staging = true;
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
                        "Select a unit; right-click a square to move, an enemy to attack. Space advances one day."
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
    if session.staging {
        session.message = "Orders wait for the fight to end. Space skips it.".into();
        session.dirty = true;
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
/// The selected unit, when it is one of the player's own.
fn selected_unit(session: &Session) -> Option<&fourx_sim::Unit> {
    let Selection::Unit(id) = session.selection else {
        return None;
    };
    session
        .game
        .as_ref()?
        .units
        .get(&id)
        .filter(|u| u.owner == session.player)
}

/// What clicking `target` means for a unit: bombard or attack a hostile square in reach,
/// otherwise march there.
fn order_for(session: &Session, unit: Id, target: Coord) -> Option<Command> {
    let game = session.game.as_ref()?;
    let u = game.units.get(&unit)?;
    let def = session.rules.def(u);
    let distance = u.position.distance(target);
    let hostile = game.hostile_holder(target, session.player).is_some();
    Some(if hostile && def.can_bombard() && distance <= def.range {
        Command::Bombard { unit, target }
    } else if hostile && def.can_attack() && distance == 1 {
        Command::Attack { unit, target }
    } else {
        Command::Move {
            unit,
            destination: target,
        }
    })
}

/// The Road-then-Railroad order a worker should take next on its square.
fn next_road_job(game: &Game, unit: &fourx_sim::Unit) -> Job {
    if game.map.get(unit.position).is_some_and(|t| t.has_road()) {
        Job::Rail
    } else {
        Job::Road
    }
}

fn act(action: &Action, session: &mut Session, connection: &mut Connection) {
    let unit = selected_unit(session).map(|u| u.id);
    let command = match action {
        Action::EndTurn => Some(Command::EndTurn),
        Action::Found => unit.map(|unit| Command::FoundCity {
            unit,
            name: format!(
                "New Haven {}",
                session.game.as_ref().unwrap().cities.len() + 1
            ),
        }),
        Action::Fortify => unit.map(|unit| Command::Fortify { unit }),
        Action::Cancel => unit.map(|unit| Command::Cancel { unit }),
        Action::Disband => unit.map(|unit| Command::Disband { unit }),
        Action::Work(job) => unit.map(|unit| Command::Work { unit, job: *job }),
        Action::Strike(target) => unit.and_then(|unit| order_for(session, unit, *target)),
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
        Action::SelectUnit(id) => {
            session.selection = Selection::Unit(*id);
            session.dirty = true;
            None
        }
        Action::SelectCity(id) => {
            session.selection = Selection::City(*id);
            session.dirty = true;
            None
        }
        Action::CloseCity => {
            if matches!(session.selection, Selection::City(_)) {
                session.selection = Selection::None;
            }
            session.dirty = true;
            None
        }
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

/// A button: what it does, its flat colour (the action bar's wear pictures instead), and
/// whether it is one of the bar's.
type Pressable = (
    &'static Interaction,
    &'static Action,
    Option<&'static mut BackgroundColor>,
    Option<&'static bar::BarButton>,
);

fn buttons(
    mut session: ResMut<Session>,
    mut connection: NonSendMut<Connection>,
    mut buttons: Query<Pressable, Changed<Interaction>>,
    mut camera: Query<&mut Transform, With<Camera2d>>,
    touches: Res<Touches>,
    mut touch_action: Local<Option<Action>>,
) {
    let mut activated = Vec::new();
    for (interaction, action, color, bar_button) in &mut buttons {
        // The day button wears the brass; everything else stays navy and warms as the
        // pointer crosses it. See `presentation` for the materials. The action bar's buttons
        // are pictures and wear their own (see `bar`).
        use crate::presentation::{BRASS, BRASS_HOVER, BRASS_PRESSED};
        use crate::presentation::{NAVY, NAVY_HOVER, NAVY_PRESSED};
        let primary = matches!(action, Action::EndTurn);
        let (rest, hover, pressed) = if primary {
            (BRASS, BRASS_HOVER, BRASS_PRESSED)
        } else {
            (NAVY, NAVY_HOVER, NAVY_PRESSED)
        };
        if let Some(mut color) = color {
            color.0 = match interaction {
                Interaction::Hovered => hover,
                Interaction::Pressed => pressed,
                _ => rest,
            };
        }
        // A button that cannot do anything now still shows what it is, and does nothing.
        if bar_button.is_some_and(|b| b.dormant) {
            continue;
        }
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
        act(action, &mut session, &mut connection);
        let pos = match action {
            Action::SelectUnit(id) => session
                .game
                .as_ref()
                .and_then(|g| g.units.get(id))
                .map(|u| u.position),
            Action::SelectCity(id) => session
                .game
                .as_ref()
                .and_then(|g| g.cities.get(id))
                .map(|c| c.position),
            _ => None,
        };
        if let Some(pos) = pos
            && let Ok(mut transform) = camera.single_mut()
        {
            focus(&mut transform, pos);
        }
    }
}
/// Dragging or clicking on the minimap moves the camera there.
fn minimap_click(
    session: Res<Session>,
    surface: Query<(&Interaction, &RelativeCursorPosition), With<MinimapSurface>>,
    mut camera: Query<&mut Transform, With<Camera2d>>,
) {
    let Some(game) = &session.game else {
        return;
    };
    for (interaction, cursor) in &surface {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let Some(normalized) = cursor.normalized else {
            continue;
        };
        let tile = Chart::of(&game.map).tile_at(normalized + Vec2::splat(0.5));
        if let Ok(mut transform) = camera.single_mut() {
            focus(&mut transform, tile);
        }
    }
}
/// Whether a point of the window is over the inspector, which only a narrow window has (at the
/// bottom); a wide one describes the selection in the pane at the bottom right (`InfoPane`).
fn inspector_contains(window: &Window, pos: Vec2) -> bool {
    window.width() < 900.0
        && pos.x >= 8.0
        && pos.x <= window.width() - 8.0
        && pos.y >= window.height() - inspector_height(window.height()) - 44.0
        && pos.y <= window.height() - 44.0
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
    info: Query<&Interaction, With<InfoPane>>,
    mut session: ResMut<Session>,
    mut connection: NonSendMut<Connection>,
    mut last_click: Local<Option<(Coord, f64)>>,
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
        .is_some_and(|p| inspector_contains(window, p))
        || info.iter().any(|i| *i != Interaction::None)
        // The city view is a window over the map; the wheel scrolls it (see `city::scroll`).
        || matches!(session.selection, Selection::City(_));
    for scroll in wheel.read() {
        if !over_inspector {
            ortho.scale = (ortho.scale * (1.0 - scroll.y * 0.08)).clamp(MIN_ZOOM, MAX_ZOOM);
        }
    }
    let fingers: Vec<_> = touches.iter().collect();
    if fingers.len() == 2 {
        let distance = fingers[0].position().distance(fingers[1].position());
        if let Some(last) = session.previous_pinch {
            ortho.scale = (ortho.scale * last / distance.max(1.0)).clamp(MIN_ZOOM, MAX_ZOOM);
        }
        session.previous_pinch = Some(distance);
        let delta = (fingers[0].delta() + fingers[1].delta()) * 0.5 * ortho.scale;
        transform.translation.x -= delta.x;
        transform.translation.y += delta.y;
    } else {
        session.previous_pinch = None;
    }
    let Some(game) = &session.game else {
        return;
    };
    // Keep the camera over the map: the world is large and empty space is disorienting.
    let (left, bottom, right, top) = game.map.screen_bounds();
    transform.translation.x = transform.translation.x.clamp(left - 200.0, right + 200.0);
    transform.translation.y = transform.translation.y.clamp(bottom - 100.0, top + 100.0);
    if keys.just_pressed(KeyCode::Home) || keys.just_pressed(KeyCode::KeyH) {
        if let Some(position) = home(game, session.player) {
            focus(&mut transform, position);
        }
    }
    // While a fight is on stage Space skips it (see `stage::drive`) rather than ending the day.
    if keys.just_pressed(KeyCode::Space) && !session.staging {
        act(&Action::EndTurn, &mut session, &mut connection);
    }
    if keys.just_pressed(KeyCode::KeyF) {
        act(&Action::Fortify, &mut session, &mut connection);
    }
    if keys.just_pressed(KeyCode::KeyB) {
        act(&Action::Found, &mut session, &mut connection);
    }
    if keys.just_pressed(KeyCode::Delete) || keys.just_pressed(KeyCode::Backspace) {
        act(&Action::Cancel, &mut session, &mut connection);
    }
    // R builds the next transport improvement, M a mine, I a farm (Civ3's irrigation key).
    for (key, job) in [
        (KeyCode::KeyR, None),
        (KeyCode::KeyM, Some(Job::Mine)),
        (KeyCode::KeyI, Some(Job::Farm)),
    ] {
        if keys.just_pressed(key) {
            let job = job.or_else(|| {
                let game = session.game.as_ref()?;
                selected_unit(&session).map(|u| next_road_job(game, u))
            });
            if let Some(job) = job {
                act(&Action::Work(job), &mut session, &mut connection);
            }
        }
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
        // Units still waiting for orders come first; with none idle, cycle through them all.
        let game = session.game.as_ref().unwrap();
        let mine: Vec<_> = game
            .units
            .values()
            .filter(|u| u.owner == session.player)
            .collect();
        let idle: Vec<Id> = mine
            .iter()
            .filter(|u| {
                u.moves_left(session.rules.def(u)) > 0 && u.order.is_none() && u.goto.is_none()
            })
            .map(|u| u.id)
            .collect();
        let ids = if idle.is_empty() {
            mine.iter().map(|u| u.id).collect()
        } else {
            idle
        };
        if !ids.is_empty() {
            let next = if let Selection::Unit(id) = session.selection {
                ids.iter().position(|v| *v > id).unwrap_or(0)
            } else {
                0
            };
            let id = ids[next];
            session.selection = Selection::Unit(id);
            if let Some(unit) = session.game.as_ref().and_then(|g| g.units.get(&id)) {
                focus(&mut transform, unit.position);
            }
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
        KeyCode::Digit6,
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
    if !clicked
        || fingers.len() > 1
        || ui
            .iter()
            .chain(info.iter())
            .any(|i| *i != Interaction::None)
    {
        return;
    }
    // The city's detail view is a modal: nothing behind it takes clicks.
    if matches!(session.selection, Selection::City(_)) {
        return;
    }
    let Some(cursor) = touch.or_else(|| window.cursor_position()) else {
        return;
    };
    // Panels also block map clicks in their empty space.
    if cursor.y < 50.0 || cursor.y > window.height() - 34.0 || inspector_contains(window, cursor) {
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
        .unit_ids_at(position)
        .into_iter()
        .filter(|id| game.units[id].owner == session.player)
        .collect();
    let city = game
        .cities
        .values()
        .find(|c| c.position == position)
        .map(|c| c.id);
    if mouse.just_pressed(MouseButton::Right) {
        if let Selection::Unit(unit) = session.selection
            && let Some(command) = order_for(&session, unit, position)
        {
            issue(&mut session, &mut connection, command);
        }
        return;
    }
    // A second click on the same square soon after the first is a double click, which opens
    // a city's detail view. A single click on a city is just a click on its square: it
    // selects the units standing there and nothing else.
    let now = time.elapsed_secs_f64();
    let double = is_double_click(*last_click, position, now);
    *last_click = if double { None } else { Some((position, now)) };
    if double && let Some(id) = city {
        session.selection = Selection::City(id);
        session.dirty = true;
        return;
    }
    if !own.is_empty() {
        let next = if let Selection::Unit(id) = session.selection {
            (own.iter().position(|v| *v == id).unwrap_or(own.len() - 1) + 1) % own.len()
        } else {
            0
        };
        session.selection = Selection::Unit(own[next]);
    } else if touch.is_some()
        && let Selection::Unit(unit) = session.selection
    {
        // Left-clicking only selects; orders go with the right button. A tap has no right
        // button, so on a touch screen it is the order.
        if let Some(command) = order_for(&session, unit, position) {
            issue(&mut session, &mut connection, command);
        }
    } else if city.is_some() {
        return;
    } else {
        session.selection = Selection::Tile(position);
    }
    session.dirty = true;
}

/// How long after a click another on the same square still makes a double click (seconds).
const DOUBLE_CLICK: f64 = 0.4;

/// Whether a click on `position` at time `now` follows the `last` click closely enough, on the
/// same square, to be its second half.
fn is_double_click(last: Option<(Coord, f64)>, position: Coord, now: f64) -> bool {
    last.is_some_and(|(at, then)| at == position && (0.0..=DOUBLE_CLICK).contains(&(now - then)))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_click_on_the_same_square_soon_after_is_a_double_click() {
        let here = Coord::new(4, 7);
        assert!(!is_double_click(None, here, 10.0));
        assert!(is_double_click(Some((here, 10.0)), here, 10.3));
        assert!(!is_double_click(
            Some((here, 10.0)),
            here,
            10.0 + DOUBLE_CLICK + 0.1
        ));
        assert!(!is_double_click(Some((here, 10.0)), Coord::new(5, 7), 10.2));
    }
}
