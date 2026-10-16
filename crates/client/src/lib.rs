mod arena;
mod bar;
mod campaign;
mod chrome;
mod city;
mod connection;
mod diplomacy;
mod goto;
mod march;
mod margin;
mod menu;
mod music;
mod presentation;
mod production;
mod puppet;
mod roads;
mod route;
mod stage;
mod theme;

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
/// How much faster the camera pans while Shift is held.
const PAN_BOOST: f32 = 3.0;
/// The camera's scale when no `--zoom` is given.
const DEFAULT_ZOOM: f32 = 0.84;
/// What shows behind a map that has no void of its own: deep water.
const BACKDROP: Color = Color::srgb(0.055, 0.13, 0.15);
/// What shows past the margin of an upright world: dark walnut.
const DESK: Color = Color::srgb(0.13, 0.08, 0.05);

#[derive(Resource)]
struct Session {
    game: Option<Game>,
    rules: Rules,
    pack: Pack,
    player: Id,
    selection: Selection,
    goto: Option<goto::Targeting>,
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
    /// The nation whose leader the diplomacy window shows, if it is open.
    diplomacy: Option<Id>,
    /// The city whose production window is open, if any.
    production: Option<Id>,
    /// The right-click menu over the map, if one is open.
    menu: Option<ContextMenu>,
    bureau: Option<campaign::Panel>,
    stack_replay: Option<(fourx_sim::stacks::StackBattle, f32)>,
    stack_queue: std::collections::VecDeque<fourx_sim::stacks::StackBattle>,
}

/// A right-click menu over the map: where the pointer was, what tile it names, and the city
/// and units standing there when it opened.
#[derive(Clone, Debug)]
struct ContextMenu {
    /// The pointer in window pixels, with the origin at the top left.
    at: Vec2,
    tile: Coord,
    city: Option<Id>,
    units: Vec<Id>,
}

/// What sits on a right-clicked tile: the city there, if any, and every unit there.
fn context_targets(game: &Game, tile: Coord) -> (Option<Id>, Vec<Id>) {
    (game.city_at(tile), game.unit_ids_at(tile))
}

/// Whether a right click on a tile with this city and these units opens the menu. A click on
/// a city or on any unit does; a click on an empty square selects that tile.
fn should_open_menu(city: Option<Id>, units: &[Id]) -> bool {
    city.is_some() || !units.is_empty()
}

/// Whether a right click on `tile` puts the map down again instead of choosing the square: a
/// right click on the square that is already chosen clears the selection, the way Escape does,
/// so the tile's card and outline are dismissed with the same button that raised them.
fn right_click_clears(selection: Selection, tile: Coord) -> bool {
    matches!(selection, Selection::Tile(chosen) if chosen == tile)
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
    /// Images of a canonical tile within this horizontal view.
    fn copies(&self, map: &Map, p: Coord) -> Vec<Coord> {
        let Some(lattice) = map.lattice else {
            return vec![p];
        };
        let (x, _) = p.screen();
        let period = lattice.columns as f32 * 128.0;
        let first = ((self.min.x - x) / period).ceil() as i32;
        let last = ((self.max.x - x) / period).floor() as i32;
        (first..=last)
            .map(|k| p.offset(k * lattice.columns, -k * lattice.columns))
            .collect()
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
        if game.map.lattice.is_some() {
            return (
                lo.x.floor() as i32,
                lo.y.floor() as i32,
                hi.x.ceil() as i32,
                hi.y.ceil() as i32,
            );
        }
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

/// Keep consecutive route squares adjacent on screen across a world seam.
fn continuous_path(map: &Map, path: &[Coord]) -> Vec<Coord> {
    let mut result = Vec::with_capacity(path.len());
    for &p in path {
        result.push(
            result
                .last()
                .map_or(p, |&previous| map.nearest(previous, p)),
        );
    }
    result
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
            if self.lattice.is_some() {
                px.rem_euclid(self.width as i32)
            } else {
                px.clamp(0, self.width as i32 - 1)
            },
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
    /// A seam-crossing viewport occupies two rectangles on the overview.
    fn view_parts(&self, rect: WorldRect) -> Vec<(Vec2, Vec2)> {
        let Some(lattice) = self.lattice else {
            return vec![self.view(rect)];
        };
        let period = lattice.columns as f32 * 128.0;
        let left = -(lattice.columns as f32) * 64.0 - 32.0;
        let width = rect.max.x - rect.min.x;
        let start = (rect.min.x - left).rem_euclid(period) + left;
        let rect = WorldRect {
            min: Vec2::new(start, rect.min.y),
            max: Vec2::new(start + width.min(period), rect.max.y),
        };
        let mut parts = vec![self.view(rect)];
        if rect.max.x > left + period {
            parts.push(self.view(WorldRect {
                min: rect.min - Vec2::X * period,
                max: rect.max - Vec2::X * period,
            }));
        }
        if width >= period {
            parts[0].0.x = 0.0;
            parts[0].1.x = 1.0;
            parts.truncate(1);
        }
        parts
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
struct MinimapView(usize);
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Selection {
    #[default]
    None,
    Unit(Id),
    City(Id),
    Tile(Coord),
}

#[derive(Component, Clone)]
struct WorldVisual;
#[derive(Component)]
struct UiRoot;
#[derive(Component)]
struct Inspector;
/// The pane at the bottom right that describes the selection (see `presentation::info`). It
/// carries an `Interaction` so that a click on it is not a click on the map under it.
#[derive(Component)]
struct InfoPane;
/// Either card of the top bar. Like `InfoPane` it carries an `Interaction`, so that a click on it
/// is not a click on the map under it.
#[derive(Component)]
struct TopCard;
/// The menu's own card. Like `InfoPane` it carries an `Interaction`, so that the pointer
/// resting on its empty space is not a click on the map under it.
#[derive(Component)]
struct ContextMenuPane;
/// The cards that block clicks to the map under them.
type Panels = (With<InfoPane>, With<TopCard>, With<ContextMenuPane>);
#[derive(Component, Clone)]
enum Action {
    EndTurn,
    Found,
    Fortify,
    GoTo,
    StackGoTo,
    SelectTile(Coord),
    Cancel,
    Disband,
    Work(Job),
    /// Attack or bombard the square, whichever the selected unit does.
    Strike(Coord),
    Develop,
    Produce(String),
    Save,
    SelectUnit(Id),
    /// Open one of the player's own cities.
    SelectCity(Id),
    /// Build a design in a city without opening its detail view.
    ProduceAt {
        city: Id,
        unit: String,
    },
    /// Open the window for choosing what a city builds.
    OpenProduction(Id),
    CloseProduction,
    /// Close the city's detail view.
    CloseCity,
    /// Open the diplomacy window with a nation's leader.
    Talk(Id),
    /// Declare war on a nation from the diplomacy window.
    DeclareWar(Id),
    Peace(Id),
    AnswerDemand(Id, bool),
    Pay(Id),
    OpenBureau(campaign::Panel),
    CloseBureau,
    CloseBattle,
    Trade(fourx_sim::Good, i32),
    Borrow,
    Repay,
    Build(Id, fourx_sim::Building),
    StackOrder(Coord),
    Load(Id),
    Unload,
    CloseTalk,
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
    /// The shore-cut farm sheets, by pack path: the plain farm's and one for each crop.
    farm_shores: std::collections::HashMap<String, presentation::FarmShore>,
    /// Road and railroad sheets: one cell per set of joined neighbours, see [`roads`].
    roads: roads::Sheet,
    rails: roads::Sheet,
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
    let (scenario, nation) = (option("--scenario"), option("--nation"));
    Host::start(
        content(),
        Start {
            scenario: scenario.as_deref(),
            nation: nation.as_deref(),
            seed,
        },
    )
    .unwrap_or_else(|error| panic!("cannot start the campaign: {error}"))
}

/// The pack and its scenarios: `--pack`'s directory, else the bundled base pack.
fn content() -> Content {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(path) = option("--pack") {
        return fourx_content::load_directory(std::path::Path::new(&path))
            .expect("valid pack directory");
    }
    Content::base()
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
    let screen = menu::first_screen();
    // The title screens offer the pack's scenarios; a remote server brings its own.
    let menu = menu::Menu::new(content(), seed);
    let pack = if server.is_none() {
        menu.pack().clone()
    } else {
        Pack::base()
    };
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
    } else if screen == menu::Screen::Campaign {
        Connection::local(local_host(seed))
    } else {
        Connection::idle()
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
            goto: None,
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
            menu: None,
            staging: false,
            bureau: match option("--panel").as_deref() {
                Some("treasury") => Some(campaign::Panel::Treasury),
                Some("dispatches") => Some(campaign::Panel::Dispatches),
                _ => None,
            },
            stack_replay: None,
            stack_queue: std::collections::VecDeque::new(),
            diplomacy: option("--talk").and_then(|id| id.parse().ok()),
            production: None,
        })
        .insert_resource(screen)
        .insert_resource(menu)
        .insert_resource(stage::Stage::new())
        .init_resource::<march::Walks>()
        .init_resource::<puppet::UnitArt>()
        .init_resource::<puppet::Facings>()
        .init_resource::<arena::Board>()
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
        .add_systems(Update, (production::scroll, music::soundtrack, smoke))
        .add_systems(
            Update,
            (
                menu::press,
                menu::keys,
                menu::pick_on_chart,
                menu::paint_chart,
                menu::show,
                menu::paint,
                menu::fit,
                menu::frame_chart,
                menu::scroll,
                menu::volume_label,
                menu::music,
            )
                .chain()
                .run_if(not(menu::in_campaign)),
        )
        .add_systems(
            Update,
            (
                (receive, buttons).chain(),
                (theme::paint, city::paint, bar::update).chain(),
                city::scroll,
                diplomacy::music,
                scroll_inspector,
                presentation::scroll_menu,
                minimap_click,
                controls,
                // Before the stage, which waits for the walks; before `refresh`, so the walking
                // units are hidden in the frame their walk begins.
                march::walk,
                // After the input, so a Space that skips a fight is not also an order; before
                // `refresh`, so the real units are hidden in the frame a fight begins.
                stage::drive,
                (campaign::animate, campaign::scroll, campaign::budget_hint).chain(),
                stage::fx,
                presentation::fit_terrain_atlas,
                (
                    presentation::refresh,
                    presentation::animate_selection_top,
                    presentation::place_selected_square,
                    // After the refresh, so the field poses the units in the frame they are
                    // spawned in.
                    arena::animate,
                )
                    .chain(),
                presentation::update_minimap,
                presentation::minimap_viewport,
                (presentation::draw_overlays, goto::draw).chain(),
                puppet::animate,
                puppet::accents,
            )
                .chain()
                .run_if(menu::in_campaign),
        )
        // Takes the title screens down in the frame the campaign starts.
        .add_systems(Update, menu::show.run_if(menu::in_campaign));
    app.insert_gizmo_config(route::RouteGizmos, route::gizmo_config());
    theme::install_font(&mut app);
    city::install_fonts(&mut app);
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
        .unwrap_or(DEFAULT_ZOOM)
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
    let shapes = theme::Shapes::new(&mut images);
    commands.spawn((
        shapes.selected_square(),
        Transform::default(),
        Visibility::Hidden,
        presentation::SelectedSquare,
    ));
    commands.spawn((
        shapes.selection_top(),
        Transform::default(),
        Visibility::Hidden,
        presentation::SelectionMarker,
    ));
    commands.insert_resource(shapes);
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
        farm_shores: Default::default(),
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
        roads: roads::Sheet::new(&mut layouts),
        rails: roads::Sheet::new(&mut layouts),
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
    mut walks: ResMut<march::Walks>,
    mut commands: Commands,
    mut clear: ResMut<ClearColor>,
    assets: Res<AssetServer>,
) {
    for event in connection.poll() {
        match event {
            Event::Content(pack) => {
                session.pack = *pack;
                session.dirty = true;
            }
            Event::Error(error) => {
                session.message = error;
                session.pending = false;
                session.dirty = true;
            }
            Event::Response(response) => match *response {
                Response::Snapshot {
                    version,
                    player,
                    mut game,
                    rules,
                } => {
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
                    // Beyond the edge of an upright world lies the desk the chart is spread on
                    // (see `margin`); its walnut shows wherever its tiles are not yet drawn.
                    let backdrop = if game.map.lattice.is_some() {
                        DESK
                    } else {
                        BACKDROP
                    };
                    if clear.0 != backdrop {
                        clear.0 = backdrop;
                    }
                    // A newer state brings the fights of the command that made it. The first
                    // snapshot is only the starting point, so whatever a save held is not replayed.
                    if newer
                        && (session.game.is_some() || flag("--replay-battle"))
                        && !game.stack_battles.is_empty()
                    {
                        session.stack_queue.extend(
                            game.stack_battles
                                .iter()
                                .filter(|b| b.owner == player || b.enemy == player)
                                .cloned(),
                        );
                        if session.stack_replay.is_none() {
                            session.stack_replay =
                                session.stack_queue.pop_front().map(|b| (b, 0.0));
                        }
                    }
                    if newer
                        && session.game.is_some()
                        && game.stack_battles.is_empty()
                        && !game.battles.is_empty()
                    {
                        stage.enqueue(&game.battles);
                        session.staging = true;
                    }
                    // Likewise its walks, which play before its fights.
                    if newer && session.game.is_some() && !game.marches.is_empty() {
                        walks.enqueue(&game.marches);
                    }
                    if newer && session.diplomacy.is_none() {
                        session.diplomacy = game
                            .diplomacy
                            .demands
                            .iter()
                            .find(|d| d.to == player)
                            .map(|d| d.from);
                    }
                    session.player = player;
                    session.game = Some(game);
                    session.rules = rules;
                    session.pending = false;
                    if newer {
                        // A newer state may have moved what the menu named.
                        session.menu = None;
                        session.dirty = true;
                        // Nothing to announce while play is going normally: the strip at the
                        // foot of the map is for what the host has to say, and it stays away
                        // until it does.
                        session.message = if player == 0 {
                            "Spectator | another client commands this campaign".into()
                        } else {
                            String::new()
                        };
                    }
                }
                Response::Rejected { reason, .. } => {
                    session.pending = false;
                    session.message = reason;
                    session.dirty = true;
                }
            },
        }
    }
}
/// Hand an order to the host. Returns whether it went: a control that finishes with a unit
/// only moves on once the order it gave has actually been sent.
fn issue(session: &mut Session, connection: &mut Connection, command: Command) -> bool {
    if session.pending || session.game.is_none() {
        return false;
    }
    if session.staging {
        session.message = "Orders wait for the fight to end. Space skips it.".into();
        session.dirty = true;
        return false;
    }
    if session.player == 0 {
        session.message = "Spectators cannot issue orders".into();
        session.dirty = true;
        return false;
    }
    session.sequence += 1;
    session.pending = true;
    connection.send(Request {
        version: PROTOCOL_VERSION,
        sequence: session.sequence,
        revision: session.game.as_ref().unwrap().revision,
        command,
    });
    true
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

/// What clicking `target` means for a unit: bombard or attack a hostile square in reach when
/// the two nations are at war, otherwise march there. A nation at peace must be declared on
/// before its units may be attacked (see the diplomacy window).
fn order_for(session: &Session, unit: Id, target: Coord) -> Option<Command> {
    let game = session.game.as_ref()?;
    order_for_in(game, &session.rules, session.player, unit, target)
}

fn order_for_in(
    game: &Game,
    rules: &Rules,
    player: Id,
    unit: Id,
    target: Coord,
) -> Option<Command> {
    let u = game.units.get(&unit)?;
    let def = rules.def(u);
    let distance = game.map.distance(u.position, target);
    let enemy = game
        .hostile_holder(target, player)
        .is_some_and(|holder| game.at_war(player, holder));
    Some(if enemy && def.can_bombard() && distance <= def.range {
        Command::Bombard { unit, target }
    } else if enemy && def.can_attack() && distance == 1 {
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

/// The unit Tab selects next: units still waiting for orders come first; with none idle, every
/// unit of the player's is in turn.
/// The unit after `after` that is still waiting for orders, or none when every one of the
/// player's units has been given something to do. Units are visited in id order, the same
/// order Tab walks them in, so a control that finishes with a unit hands over the way Tab
/// would.
fn next_active_unit(game: &Game, rules: &Rules, player: Id, after: Id) -> Selection {
    let active = game
        .units
        .values()
        .filter(|u| u.owner == player && u.id != after)
        .filter(|u| u.moves_left(rules.def(u)) > 0 && u.order.is_none() && u.goto.is_none())
        .map(|u| u.id);
    // The map is ordered by id, so the first unit past `after` is the next in the rotation;
    // with none past it the rotation starts again at the first unit still waiting.
    let mut first = None;
    for id in active {
        if id > after {
            return Selection::Unit(id);
        }
        first = first.or(Some(id));
    }
    first.map_or(Selection::None, Selection::Unit)
}

/// Whether `unit` could be told to fortify at all: a land unit that can defend itself and is
/// not aboard a ship, the same condition the action bar shows the button under. A unit that
/// fails it is left to the host to refuse, which is what happens when the key is pressed
/// rather than the button.
fn can_fortify(session: &Session, unit: Id) -> bool {
    session
        .game
        .as_ref()
        .and_then(|game| game.units.get(&unit))
        .is_some_and(|u| {
            let def = session.rules.def(u);
            def.domain == fourx_sim::Domain::Land && def.defense > 0 && u.carrier.is_none()
        })
}

fn next_unit(session: &Session) -> Option<Id> {
    let game = session.game.as_ref()?;
    let mine: Vec<_> = game
        .units
        .values()
        .filter(|u| u.owner == session.player)
        .collect();
    let idle: Vec<Id> = mine
        .iter()
        .filter(|u| u.moves_left(session.rules.def(u)) > 0 && u.order.is_none() && u.goto.is_none())
        .map(|u| u.id)
        .collect();
    let ids: Vec<Id> = if idle.is_empty() {
        mine.iter().map(|u| u.id).collect()
    } else {
        idle
    };
    let next = if let Selection::Unit(id) = session.selection {
        ids.iter().position(|v| *v > id).unwrap_or(0)
    } else {
        0
    };
    ids.get(next).copied()
}

/// The city C opens next, after the selected one.
fn next_city(session: &Session) -> Option<Id> {
    let ids: Vec<Id> = session
        .game
        .as_ref()?
        .cities
        .values()
        .filter(|c| c.owner == session.player)
        .map(|c| c.id)
        .collect();
    if ids.is_empty() {
        return None;
    }
    let next = if let Selection::City(id) = session.selection {
        (ids.iter().position(|v| *v == id).unwrap_or(0) + 1) % ids.len()
    } else {
        0
    };
    ids.get(next).copied()
}

fn stack_command(
    game: &Game,
    rules: &Rules,
    player: Id,
    unit: Id,
    target: Coord,
) -> Option<Command> {
    let selected = game.units.get(&unit)?;
    let enemy = game
        .hostile_holder(target, player)
        .is_some_and(|holder| game.at_war(player, holder));
    let fresh_attacker = game.units_at(selected.position).into_iter().any(|u| {
        u.owner == player
            && u.carrier.is_none()
            && rules.def(u).domain == rules.def(selected).domain
            && rules.def(u).can_attack()
            && u.moves_left(rules.def(u)) > 0
            && !u.attacked
    });
    if enemy && game.map.distance(selected.position, target) == 1 && fresh_attacker {
        return Some(Command::AttackStack { unit, target });
    }
    match order_for_in(game, rules, player, unit, target)? {
        Command::Move { destination, .. } => Some(Command::MoveStack { unit, destination }),
        Command::Attack { target, .. } => Some(Command::AttackStack { unit, target }),
        _ => None,
    }
}

fn stack_order_for(session: &Session, unit: Id, target: Coord) -> Option<Command> {
    let game = session.game.as_ref()?;
    stack_command(game, &session.rules, session.player, unit, target)
}

fn act(action: &Action, session: &mut Session, connection: &mut Connection) {
    // Every control dismisses the right-click menu, including the menu's own: its choice is
    // carried out below, and anything else replaces it.
    if session.menu.take().is_some() {
        session.dirty = true;
    }
    let unit = selected_unit(session).map(|u| u.id);
    session.goto = None;
    let command = match action {
        Action::GoTo => {
            if let Some(unit) = unit {
                session.goto = Some(goto::Targeting::new(unit, false, false));
                session.dirty = true;
            }
            None
        }
        Action::StackGoTo => {
            if let Some(unit) = unit {
                session.goto = Some(goto::Targeting::new(unit, true, false));
                session.dirty = true;
            }
            None
        }
        Action::SelectTile(tile) => {
            session.selection = Selection::Tile(*tile);
            session.dirty = true;
            None
        }
        Action::OpenBureau(panel) => {
            session.bureau = Some(*panel);
            session.dirty = true;
            None
        }
        Action::CloseBureau => {
            session.bureau = None;
            session.dirty = true;
            None
        }
        Action::CloseBattle => {
            session.stack_replay = session.stack_queue.pop_front().map(|b| (b, 0.0));
            session.dirty = true;
            None
        }
        Action::Trade(g, n) => Some(Command::Trade {
            good: *g,
            amount: *n,
        }),
        Action::Load(carrier) => unit.map(|unit| Command::Load {
            unit,
            carrier: *carrier,
        }),
        Action::Unload => unit.map(|unit| Command::Unload { unit }),
        Action::Borrow => Some(Command::Borrow),
        Action::Repay => Some(Command::Repay),
        Action::Build(city, b) => {
            session.production = None;
            Some(Command::Build {
                city: *city,
                building: *b,
            })
        }
        Action::Peace(nation) => Some(Command::Peace { nation: *nation }),
        Action::AnswerDemand(nation, accept) => Some(Command::AnswerDemand {
            nation: *nation,
            accept: *accept,
        }),
        Action::Pay(nation) => Some(Command::Pay {
            nation: *nation,
            amount: 25,
        }),
        Action::StackOrder(target) => unit.and_then(|unit| stack_order_for(session, unit, *target)),
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
        Action::OpenProduction(city) => {
            session.production = Some(*city);
            session.dirty = true;
            None
        }
        Action::CloseProduction => {
            session.production = None;
            session.dirty = true;
            None
        }
        Action::ProduceAt { city, unit } => {
            // The choice is made; the window has done its work.
            session.production = None;
            session.dirty = true;
            let own = session
                .game
                .as_ref()
                .and_then(|g| g.cities.get(city))
                .is_some_and(|c| c.owner == session.player);
            if own && session.rules.units.contains_key(unit) {
                Some(Command::Produce {
                    city: *city,
                    unit: unit.clone(),
                })
            } else {
                None
            }
        }
        Action::Talk(nation) => {
            session.diplomacy = Some(*nation);
            session.selection = Selection::None;
            session.dirty = true;
            None
        }
        Action::DeclareWar(nation) => {
            // The words have been said; the window closes on the state of war.
            session.diplomacy = None;
            session.dirty = true;
            Some(Command::DeclareWar { nation: *nation })
        }
        Action::CloseTalk => {
            session.diplomacy = None;
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
        let ordered = issue(session, connection, command);
        // Fortifying takes a unit out of the rotation, so the player is handed the next unit
        // still waiting for orders, or is left with nothing selected when there is no such
        // unit. A refused order leaves the selection where it was.
        if ordered
            && matches!(action, Action::Fortify)
            && let Some(unit) = unit
            && can_fortify(session, unit)
        {
            let next = session.game.as_ref().map_or(Selection::None, |game| {
                next_active_unit(game, &session.rules, session.player, unit)
            });
            session.selection = next;
            session.dirty = true;
        }
    }
}

/// A button: what it does, and whether it is one of the bar's, which may be dormant. Its fill
/// follows the pointer (see `theme::paint`).
type Pressable = (
    &'static Interaction,
    &'static Action,
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
    for (interaction, action, bar_button) in &mut buttons {
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
    if let Some(touch) = touches.iter_just_released().next()
        && let Some(action) = touch_action.take()
        && touch.position().distance(touch.start_position()) < 12.0
    {
        activated.push(action);
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
/// The input system's reads and writes, bundled so the system takes one argument.
#[derive(bevy::ecs::system::SystemParam)]
struct Controls<'w, 's> {
    keys: Res<'w, ButtonInput<KeyCode>>,
    mouse: Res<'w, ButtonInput<MouseButton>>,
    touches: Res<'w, Touches>,
    time: Res<'w, Time>,
    wheel: MessageReader<'w, 's, MouseWheel>,
    windows: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
    camera: Query<
        'w,
        's,
        (
            &'static Camera,
            &'static GlobalTransform,
            &'static mut Transform,
            &'static mut Projection,
        ),
        With<Camera2d>,
    >,
    ui: Query<'w, 's, &'static Interaction, With<Button>>,
    info: Query<'w, 's, &'static Interaction, Or<Panels>>,
    session: ResMut<'w, Session>,
    connection: NonSendMut<'w, Connection>,
    last_click: Local<'s, Option<(Coord, f64)>>,
    hold: Local<'s, Option<(Id, f64)>>,
}
fn controls(params: Controls) {
    let Controls {
        keys,
        mouse,
        touches,
        time,
        mut wheel,
        windows,
        mut camera,
        ui,
        info,
        mut session,
        mut connection,
        mut last_click,
        mut hold,
    } = params;
    if let Some(targeting) = session.goto.as_mut() {
        targeting.hovered = None;
    }
    if session.goto.as_ref().is_some_and(|g| g.drag)
        && !mouse.pressed(MouseButton::Left)
        && !mouse.just_released(MouseButton::Left)
    {
        session.goto = None;
    }
    if !mouse.pressed(MouseButton::Left) && !mouse.just_released(MouseButton::Left) {
        *hold = None;
    }
    let Ok(window) = windows.single() else {
        return;
    };
    let Ok((camera, global, mut transform, mut projection)) = camera.single_mut() else {
        return;
    };
    let Projection::Orthographic(ref mut ortho) = *projection else {
        return;
    };
    // Holding either Shift key pans faster.
    let boost = if keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]) {
        PAN_BOOST
    } else {
        1.0
    };
    let step = time.delta_secs() * 650.0 * boost * ortho.scale;
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
        || matches!(session.selection, Selection::City(_))
        || session.diplomacy.is_some()
        || session.production.is_some();
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
    if session.bureau.is_some() || session.stack_replay.is_some() {
        if keys.just_pressed(KeyCode::Escape) {
            session.bureau = None;
            session.stack_replay = session.stack_queue.pop_front().map(|b| (b, 0.0));
            session.dirty = true;
        }
        return;
    }
    // The production window is a modal too, and sits over the others.
    if session.production.is_some() {
        if keys.just_pressed(KeyCode::Escape) {
            session.production = None;
            session.dirty = true;
        }
        return;
    }
    // The diplomacy window is a modal: Escape leaves it, and nothing behind it takes keys.
    if session.diplomacy.is_some() {
        if keys.just_pressed(KeyCode::Escape) {
            session.diplomacy = None;
            session.dirty = true;
        }
        return;
    }
    let Some(game) = &session.game else {
        return;
    };
    // Keep the camera near the map, but let it travel far enough past an edge that the last
    // squares can be brought to the middle of the screen and beyond (see `margin::reach`).
    let (left, bottom, right, top) = game.map.screen_bounds();
    let reach = margin::reach(window.height() * 0.5 * ortho.scale);
    transform.translation.x = if let Some(period) = game.map.screen_period() {
        (transform.translation.x - left).rem_euclid(period) + left
    } else {
        let reach_x = margin::reach(window.width() * 0.5 * ortho.scale);
        transform
            .translation
            .x
            .clamp(left - reach_x, right + reach_x)
    };
    transform.translation.y = transform.translation.y.clamp(bottom - reach, top + reach);
    if (keys.just_pressed(KeyCode::Home) || keys.just_pressed(KeyCode::KeyH))
        && let Some(position) = home(game, session.player)
    {
        focus(&mut transform, position);
    }
    // While a fight is on stage Space skips it (see `stage::drive`) rather than ending the day.
    if keys.just_pressed(KeyCode::Space) && !session.staging {
        act(&Action::EndTurn, &mut session, &mut connection);
    }
    if keys.just_pressed(KeyCode::KeyG) {
        act(&Action::GoTo, &mut session, &mut connection);
    }
    if keys.just_pressed(KeyCode::KeyX) {
        act(&Action::StackGoTo, &mut session, &mut connection);
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
        if session.goto.take().is_none() {
            session.selection = Selection::None;
        }
        *hold = None;
        session.menu = None;
        session.dirty = true;
    }
    if keys.just_pressed(KeyCode::Tab)
        && let Some(id) = next_unit(&session)
    {
        session.selection = Selection::Unit(id);
        if let Some(unit) = session.game.as_ref().and_then(|g| g.units.get(&id)) {
            focus(&mut transform, unit.position);
        }
        session.dirty = true;
    }
    if keys.just_pressed(KeyCode::KeyC)
        && let Some(id) = next_city(&session)
    {
        session.selection = Selection::City(id);
        session.dirty = true;
    }
    for (index, key) in [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
        KeyCode::Digit0,
    ]
    .into_iter()
    .enumerate()
    {
        if keys.just_pressed(key)
            && let Some(unit) = session
                .rules
                .units
                .keys()
                .filter(|k| k.as_str() != "crate")
                .nth(index)
                .cloned()
        {
            act(&Action::Produce(unit), &mut session, &mut connection);
        }
    }
    if mouse.just_pressed(MouseButton::Right) && session.goto.is_some() {
        session.goto = None;
        *hold = None;
        session.dirty = true;
        return;
    }
    // A right click with the menu open puts the menu down and nothing else, the way a left click
    // away from it does: the square under the pointer is not chosen by the click that dismisses.
    if mouse.just_pressed(MouseButton::Right) && session.menu.take().is_some() {
        session.dirty = true;
        return;
    }
    let touch = touches
        .iter_just_released()
        .find(|t| t.position().distance(t.start_position()) < 12.0)
        .map(|t| t.position());
    let blocked = fingers.len() > 1
        || ui
            .iter()
            .chain(info.iter())
            .any(|i| *i != Interaction::None);
    if blocked {
        *hold = None;
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
    if cursor.y > window.height() - 38.0 || inspector_contains(window, cursor) {
        return;
    }
    let Ok(world) = camera.viewport_to_world_2d(global, cursor) else {
        return;
    };
    let game = session.game.as_ref().unwrap();
    let Some(position) = game.map.normalize(Coord::from_screen(world.x, world.y)) else {
        return;
    };
    let (city, present) = context_targets(game, position);
    let own: Vec<Id> = present
        .iter()
        .filter(|id| game.units[*id].owner == session.player)
        .copied()
        .collect();
    // Only a city of the player's own can be opened; a foreign one is reached from the menu.
    let own_city = city.filter(|id| {
        game.cities
            .get(id)
            .is_some_and(|c| c.owner == session.player)
    });
    if let Some((id, started)) = *hold {
        if !matches!(session.selection, Selection::Unit(selected) if selected == id) {
            *hold = None;
        } else if mouse.pressed(MouseButton::Left) && time.elapsed_secs_f64() - started >= 0.3 {
            session.goto = Some(goto::Targeting::new(id, false, true));
            *hold = None;
        }
    }
    if session.goto.as_ref().is_some_and(|g| {
        !matches!(session.selection, Selection::Unit(id) if id == g.unit)
            || selected_unit(&session).is_none()
    }) {
        session.goto = None;
    }
    if session.goto.is_some() {
        goto::update(&mut session, position);
        let confirm = session.goto.as_ref().is_some_and(|g| {
            if g.drag {
                mouse.just_released(MouseButton::Left)
            } else {
                mouse.just_pressed(MouseButton::Left) || touch.is_some()
            }
        });
        if confirm {
            let targeting = session.goto.take().unwrap();
            let command = targeting
                .preview
                .is_some()
                .then(|| {
                    if targeting.stack {
                        stack_order_for(&session, targeting.unit, position)
                    } else {
                        Some(Command::Move {
                            unit: targeting.unit,
                            destination: position,
                        })
                    }
                })
                .flatten();
            if let Some(command) = command {
                issue(&mut session, &mut connection, command);
            }
            session.dirty = true;
        }
        return;
    }
    if mouse.just_pressed(MouseButton::Left)
        && selected_unit(&session).is_some_and(|u| u.position == position)
    {
        *hold = selected_unit(&session).map(|u| (u.id, time.elapsed_secs_f64()));
        return;
    }
    let released_click = mouse.just_released(MouseButton::Left) && hold.take().is_some();
    if !mouse.just_pressed(MouseButton::Left)
        && !mouse.just_pressed(MouseButton::Right)
        && !released_click
        && touch.is_none()
    {
        return;
    }
    if mouse.just_pressed(MouseButton::Right) {
        // A second right click on the chosen square puts it down again, and takes any menu over
        // it along: the tile's card and outline go with the same button that raised them.
        if right_click_clears(session.selection, position) {
            session.selection = Selection::None;
            session.menu = None;
            session.dirty = true;
            return;
        }
        session.menu = should_open_menu(city, &present).then_some(ContextMenu {
            at: cursor,
            tile: position,
            city,
            units: present,
        });
        // Keep a selected unit for explicit combat menu actions; empty ground selects a tile.
        if session.menu.is_none() {
            session.selection = Selection::Tile(position);
        }
        session.dirty = true;
        return;
    }
    // A left click anywhere else dismisses the menu instead of selecting under it.
    if session.menu.take().is_some() {
        session.dirty = true;
        return;
    }
    // A second click on the same square soon after the first is a double click, which opens
    // the detail view of one of the player's own cities. A single click on a city is just a
    // click on its square: it selects the units standing there and nothing else. A foreign
    // city is never opened; its leader is reached from the right-click menu.
    let now = time.elapsed_secs_f64();
    let double = is_double_click(*last_click, position, now);
    *last_click = if double { None } else { Some((position, now)) };
    if double && let Some(id) = own_city {
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
    } else {
        return;
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
    fn the_minimap_splits_the_view_at_the_horizontal_seam() {
        let map = Map::embedded(
            Lattice {
                columns: 4,
                rows: 6,
            },
            fourx_sim::terrain::Terrain::Grass,
        );
        let chart = Chart::of(&map);
        let rect = WorldRect {
            min: Vec2::new(-320.0, -220.0),
            max: Vec2::new(-192.0, -160.0),
        };
        let parts = chart.view_parts(rect);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].1.x, 1.0);
        assert_eq!(parts[1].0.x, 0.0);
        assert!(
            ((parts[0].1.x - parts[0].0.x) + (parts[1].1.x - parts[1].0.x) - 0.25).abs() < 1e-6
        );
        let whole = chart.view_parts(WorldRect {
            max: Vec2::new(300.0, -160.0),
            ..rect
        });
        assert_eq!(whole.len(), 1);
        assert_eq!((whole[0].0.x, whole[0].1.x), (0.0, 1.0));
    }

    #[test]
    fn seam_paths_are_short_and_visible_images_resolve_to_the_same_tile() {
        let lattice = Lattice {
            columns: 4,
            rows: 6,
        };
        let map = Map::embedded(lattice, fourx_sim::terrain::Terrain::Grass);
        let west = lattice.cell(0, 2);
        let east = lattice.cell(6, 2);
        let path = continuous_path(&map, &[west, east, west]);
        assert_eq!(path, vec![west, west.offset(-1, 1), west]);
        let (x, y) = west.screen();
        let view = WorldRect {
            min: Vec2::new(x - 140.0, y - 100.0),
            max: Vec2::new(x + 140.0, y + 100.0),
        };
        let copies = view.copies(&map, east);
        assert_eq!(copies, vec![west.offset(-1, 1)]);
        assert_eq!(map.normalize(copies[0]), Some(east));
    }

    #[test]
    fn the_menu_opens_for_cities_and_stacks_but_not_empty_squares() {
        assert!(!should_open_menu(None, &[]));
        assert!(should_open_menu(Some(1), &[]));
        assert!(should_open_menu(None, &[7]));
        assert!(should_open_menu(Some(1), &[7, 8]));
    }

    #[test]
    fn a_second_right_click_on_the_chosen_square_puts_it_down() {
        let here = Coord::new(4, 7);
        assert!(!right_click_clears(Selection::None, here));
        assert!(!right_click_clears(Selection::Unit(3), here));
        assert!(!right_click_clears(Selection::City(2), here));
        assert!(right_click_clears(Selection::Tile(here), here));
        // Another square is chosen, not put down.
        assert!(!right_click_clears(Selection::Tile(Coord::new(5, 7)), here));
    }

    #[test]
    fn a_unit_that_fortifies_hands_over_the_next_one_waiting_for_orders() {
        use fourx_sim::Order;

        let host =
            Host::base_scenario("russo-japanese-war", 42).expect("the starter scenario starts");
        let player = host.commander();
        let rules = host.rules;
        let mut game = host.game;
        let mine: Vec<Id> = game
            .units
            .values()
            .filter(|u| u.owner == player)
            .map(|u| u.id)
            .collect();
        assert!(mine.len() >= 2, "Japan starts with a stack");
        // Only the first two units are waiting for orders; the rest are dug in and spent.
        for (id, unit) in game.units.iter_mut() {
            if mine[..2].contains(id) {
                unit.order = Order::None;
                unit.moves_used = 0;
            } else {
                unit.order = Order::Fortified;
                unit.moves_used = rules.def(unit).total_moves();
            }
        }
        let (first, second) = (mine[0], mine[1]);
        assert_eq!(
            next_active_unit(&game, &rules, player, first),
            Selection::Unit(second)
        );
        // Fortifying the last one still waiting leaves nothing to select.
        game.units.get_mut(&second).unwrap().order = Order::Fortified;
        assert_eq!(
            next_active_unit(&game, &rules, player, first),
            Selection::None
        );
    }

    #[test]
    fn right_click_names_the_city_and_every_unit_on_its_square() {
        let host = fourx_runtime::Host::base_scenario("russo-japanese-war", 42)
            .expect("the starter scenario starts");
        let game = &host.game;
        let city = game.cities.values().next().expect("a city");
        let (found, units) = context_targets(game, city.position);
        assert_eq!(found, Some(city.id));
        for id in &units {
            assert_eq!(game.units[id].position, city.position);
        }
        let unit = game.units.values().next().expect("a unit");
        let (at_unit, stack) = context_targets(game, unit.position);
        assert!(stack.contains(&unit.id));
        assert_eq!(at_unit, game.city_at(unit.position));
    }

    #[test]
    fn a_noncombatant_can_launch_the_stack_attack_beside_an_enemy_city() {
        let mut host =
            Host::base_scenario("russo-japanese-war", 42).expect("the starter scenario starts");
        let player = host.commander();
        let noncombatant = host
            .game
            .units
            .values()
            .find(|u| {
                u.owner == player
                    && u.carrier.is_none()
                    && host.rules.def(u).domain == fourx_sim::Domain::Land
                    && !host.rules.def(u).can_attack()
                    && !host.rules.def(u).can_bombard()
            })
            .map(|u| u.id)
            .expect("the scenario has a civilian for this stack");
        let attacker = host
            .game
            .units
            .values()
            .find(|u| u.owner == player && host.rules.def(u).can_attack())
            .map(|u| u.id)
            .expect("the scenario has an attacker");
        let (enemy, city) = host
            .game
            .cities
            .values()
            .find(|c| c.owner != player)
            .map(|c| (c.owner, c.position))
            .unwrap();
        let approach = city.offset(-1, 0);
        let ground = host.game.map.get_mut(approach).unwrap();
        ground.terrain = fourx_sim::terrain::Terrain::Grass;
        ground.relief = fourx_sim::terrain::Relief::Flat;
        ground.cover = fourx_sim::terrain::Cover::Bare;
        host.game.set_position(noncombatant, approach);
        host.game.set_position(attacker, approach);
        if !host.game.at_war(player, enemy) {
            host.command(player, Command::DeclareWar { nation: enemy })
                .unwrap();
        }

        assert!(matches!(
            stack_command(&host.game, &host.rules, player, noncombatant, city),
            Some(Command::AttackStack { unit, target })
                if unit == noncombatant && target == city
        ));
    }

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
