//! The screens before a campaign: the title screen, and the nation selection screen where a
//! scenario and the nation to command are chosen.
//!
//! The title screen is a painting (the pack's `visuals.menu.background`, else the walnut desk
//! with the harbour engraving) under the game's title and a column of plates: Continue (the
//! newest save, when there is one), New Campaign, Load Game, Settings, Credits and Exit. Load
//! Game, Settings and Credits open a parchment sheet over it.
//!
//! The nation selection screen lists the scenario's nations as flags on the left, describes the
//! chosen one at the top right (its profile from the scenario, see [`fourx_sim::scenario::Profile`],
//! and the pack's picture of it), and shows the world at the start date below with that nation's
//! lands lit, where a click on any nation picks it. Along the bottom: Back, the scenario, and Start
//! Game.
//!
//! Nothing here reaches the simulation until a campaign starts: then the embedded host is built
//! and handed to the [`Connection`], and the [`Screen`] becomes the campaign. A client started with
//! campaign flags (`--scenario`, `--nation`, `--load`, `--server`, ...) skips these screens.
use super::*;
use crate::chrome::{Frame, Skin, dress, texture, trim};
use crate::city::DISPLAY;
use crate::theme::{
    BAD, BRONZE, EDGE, GOOD, INK, INK_SOFT, ON_PLATE, RULE, SLATE, TRIM, WELL, font, framed_flag,
    lift, rounded,
};
use bevy::audio::Volume;
use bevy::text::FontWeight;
use bevy::ui::FocusPolicy;
use bevy::window::{MonitorSelection, WindowMode};
use fourx_sim::{Scenario, calendar::Date};

/// Which screen is up. The campaign's systems run only in [`Screen::Campaign`].
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Screen {
    Title,
    Nations,
    Campaign,
}

/// A sheet of parchment over the title screen.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Sheet {
    Saves,
    Settings,
    Credits,
}

/// A save the title screen can resume.
#[derive(Clone, Debug)]
pub(crate) struct SaveFile {
    /// What the list calls it: the file's name without `.save.json`.
    name: String,
    /// When it was written, as far as the platform tells.
    when: String,
    /// Seconds since the epoch it was written, for picking the newest.
    stamp: u64,
    #[cfg(not(target_arch = "wasm32"))]
    path: std::path::PathBuf,
}

/// The state of the screens before a campaign.
#[derive(Resource)]
pub(crate) struct Menu {
    content: Content,
    /// The pack's scenario IDs in the pack's order.
    scenarios: Vec<String>,
    scenario: usize,
    /// The chosen nation: its index in the scenario's list (its ID is one more).
    nation: usize,
    sheet: Option<Sheet>,
    saves: Vec<SaveFile>,
    seed: u64,
    /// Rebuild the screen at the next frame.
    dirty: bool,
    /// The scenario's world on its start date, built once per scenario, which the map is
    /// painted from.
    world: Option<(usize, Game)>,
    /// The painted map, and the scenario and nation it was painted for.
    chart: Option<Handle<Image>>,
    charted: Option<(usize, usize)>,
    /// A line under the plates: why a save would not load, for instance.
    notice: String,
    fullscreen: bool,
}

impl Menu {
    pub(crate) fn new(content: Content, seed: u64) -> Self {
        let scenarios: Vec<String> = content
            .pack
            .scenarios
            .iter()
            .map(|entry| entry.id.clone())
            .collect();
        let scenario = scenarios
            .iter()
            .position(|id| *id == content.pack.default_scenario)
            .unwrap_or(0);
        let mut menu = Self {
            content,
            scenarios,
            scenario,
            nation: 0,
            sheet: None,
            saves: find_saves(),
            seed,
            dirty: true,
            world: None,
            chart: None,
            charted: None,
            notice: String::new(),
            fullscreen: false,
        };
        menu.nation = menu.commander_index();
        menu
    }

    pub(crate) fn pack(&self) -> &Pack {
        &self.content.pack
    }

    fn scenario(&self) -> &Scenario {
        &self.content.scenarios[&self.scenarios[self.scenario]]
    }

    /// Where the scenario's default commander stands in its list of nations.
    fn commander_index(&self) -> usize {
        let scenario = self.scenario();
        scenario
            .nations
            .iter()
            .position(|n| n.id == scenario.commander)
            .unwrap_or(0)
    }

    /// The scenario's world, built on first use.
    fn world(&mut self) -> Option<&Game> {
        if self.world.as_ref().is_none_or(|(i, _)| *i != self.scenario) {
            let scenario = self.scenario();
            let rules = self.content.pack.rules.with_overrides(&scenario.rules);
            let game = Game::from_scenario(self.seed, &rules, scenario, None).ok()?;
            self.world = Some((self.scenario, game));
        }
        self.world.as_ref().map(|(_, game)| game)
    }
}

/// Whether the client goes straight into a campaign: it was told which one, or to join a
/// server, or it is being driven by a tool. `--menu` shows the title screens regardless
/// (`--menu nations` opens the nation selection screen).
pub(crate) fn straight_to_campaign() -> bool {
    if option("--menu").is_some() || flag("--menu") {
        return false;
    }
    [
        "--server",
        "--load",
        "--scenario",
        "--nation",
        "--screenshot",
        "--talk",
        "--select",
        "--panel",
        "--focus",
        "--demo-combat",
    ]
    .iter()
    .any(|f| option(f).is_some())
        || flag("--smoke")
        || flag("--replay-battle")
}

/// The screen the client opens on.
pub(crate) fn first_screen() -> Screen {
    if straight_to_campaign() {
        Screen::Campaign
    } else if option("--menu").as_deref() == Some("nations") {
        Screen::Nations
    } else {
        Screen::Title
    }
}

/// Whether the campaign's systems run.
pub(crate) fn in_campaign(screen: Res<Screen>) -> bool {
    *screen == Screen::Campaign
}

// --- saves ---------------------------------------------------------------------------------------

/// The saves the title screen can offer, newest first: every `*.save.json` beside the save the
/// game writes, or the browser's one save.
fn find_saves() -> Vec<SaveFile> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let main = save_path();
        let folder = main
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map_or_else(|| std::path::PathBuf::from("."), |p| p.to_path_buf());
        let mut saves: Vec<SaveFile> = std::fs::read_dir(&folder)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                let file = path.file_name()?.to_str()?.to_string();
                let name = file.strip_suffix(".save.json")?.to_string();
                let stamp = entry
                    .metadata()
                    .ok()?
                    .modified()
                    .ok()?
                    .duration_since(std::time::UNIX_EPOCH)
                    .ok()?
                    .as_secs();
                Some(SaveFile {
                    name,
                    when: written(stamp),
                    stamp,
                    path,
                })
            })
            .collect();
        saves.sort_by(|a, b| b.stamp.cmp(&a.stamp));
        saves
    }
    #[cfg(target_arch = "wasm32")]
    {
        let stored = web_sys::window()
            .and_then(|w| w.local_storage().ok().flatten())
            .and_then(|s| s.get_item("open-4x-save").ok().flatten())
            .is_some();
        if stored {
            vec![SaveFile {
                name: "Browser save".into(),
                when: "Saved in this browser".into(),
                stamp: 0,
            }]
        } else {
            Vec::new()
        }
    }
}

/// `Saved 14 October 2026, 18:05 UTC` for seconds since the epoch.
fn written(stamp: u64) -> String {
    let days = (stamp / 86_400) as i64;
    let (hour, minute) = ((stamp % 86_400) / 3600, (stamp % 3600) / 60);
    match Date::from_days(days) {
        Some(date) => format!("Saved {date}, {hour:02}:{minute:02} UTC"),
        None => String::new(),
    }
}

/// Open a save as the campaign's host.
fn read_save(save: &SaveFile) -> Result<Host, String> {
    #[cfg(not(target_arch = "wasm32"))]
    let json = std::fs::read_to_string(&save.path).map_err(|e| e.to_string())?;
    #[cfg(target_arch = "wasm32")]
    let json = {
        let _ = save;
        web_sys::window()
            .and_then(|w| w.local_storage().ok().flatten())
            .and_then(|s| s.get_item("open-4x-save").ok().flatten())
            .ok_or_else(|| "The browser holds no save".to_string())?
    };
    Host::load(&json).map_err(|e| e.to_string())
}

// --- leaving the menu --------------------------------------------------------------------------

/// Hand a host to the connection and go to the campaign.
fn begin(host: Host, session: &mut Session, connection: &mut Connection, screen: &mut Screen) {
    *connection = Connection::local(host);
    *screen = Screen::Campaign;
    session.game = None;
    session.centered = false;
    session.dirty = true;
}

/// Start the chosen scenario as the chosen nation.
fn start(menu: &Menu) -> Result<Host, String> {
    let scenario = menu.scenario();
    let nation = scenario
        .nations
        .get(menu.nation)
        .map(|n| n.id.clone())
        .ok_or("No nation chosen")?;
    let content = Content {
        pack: menu.content.pack.clone(),
        scenarios: menu.content.scenarios.clone(),
    };
    Host::start(
        content,
        Start {
            scenario: Some(&menu.scenarios[menu.scenario]),
            nation: Some(&nation),
            seed: menu.seed,
        },
    )
    .map_err(|e| e.to_string())
}

// --- palette -----------------------------------------------------------------------------------
/// The red of the one plate that moves on (Continue, Start Game), with its hover and press.
const CRIMSON: Color = Color::srgb(0.478, 0.106, 0.090);
const CRIMSON_HOVER: Color = Color::srgb(0.569, 0.141, 0.118);
const CRIMSON_PRESSED: Color = Color::srgb(0.384, 0.082, 0.071);
/// Cream on the crimson and on slate.
const CREAM: Color = Color::srgb(0.965, 0.929, 0.816);
/// The darker wells set into slate, and words on slate that are not the main line.
const INSET: Color = Color::srgb(0.082, 0.090, 0.102);
const MUTED: Color = Color::srgba(0.965, 0.957, 0.925, 0.62);
/// Behind everything until the pictures load.
const NIGHT: Color = Color::srgb(0.055, 0.047, 0.043);

// --- components --------------------------------------------------------------------------------

/// Everything the menu spawns hangs from one of these.
#[derive(Component)]
pub(crate) struct MenuRoot;

/// What a control of the menu does.
#[derive(Component, Clone, Debug)]
pub(crate) enum MenuAction {
    Continue,
    NewCampaign,
    Open(Sheet),
    CloseSheet,
    Exit,
    Load(usize),
    Back,
    Start,
    Nation(usize),
    /// Step through the pack's scenarios.
    Scenario(i32),
    Volume(i32),
    Fullscreen,
}

/// How a control is drawn, so [`paint`] can light it under the pointer.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Plate {
    /// Crimson with a gilt edge: the action that moves on.
    Crimson,
    /// Parchment in a small gilt frame.
    Paper,
    /// Slate in a small gilt frame.
    Slate,
    /// A nation's cell in the list; `true` when it is the chosen one.
    Cell(bool),
    /// Cannot be used now.
    Dormant,
}

/// The layer of a plate that brightens under the pointer.
#[derive(Component)]
pub(crate) struct Glow;

/// A picture fitted to the node it is in: covering it (cropped) or contained in it (whole).
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Fit {
    Cover,
    Contain,
}

/// The list of nations, which the wheel scrolls.
#[derive(Component)]
pub(crate) struct NationList;

/// The world map, which a click picks a nation on.
#[derive(Component)]
pub(crate) struct WorldChart;

/// The frame around the world map, sized to the map's own proportions so no sea is added at
/// its sides.
#[derive(Component)]
pub(crate) struct ChartFrame;

// --- systems -------------------------------------------------------------------------------------

/// Rebuild the screen when it changes, and take it down when the campaign starts.
pub(crate) fn show(
    mut commands: Commands,
    mut menu: ResMut<Menu>,
    screen: Res<Screen>,
    session: Res<Session>,
    assets: Res<AssetServer>,
    windows: Query<&Window, With<PrimaryWindow>>,
    roots: Query<Entity, With<MenuRoot>>,
    mut shown: Local<Option<Screen>>,
) {
    if *shown == Some(*screen) && !menu.dirty {
        return;
    }
    *shown = Some(*screen);
    menu.dirty = false;
    for root in &roots {
        commands.entity(root).despawn();
    }
    let width = windows.single().map_or(f32::MAX, |w| w.width());
    let layout = Layout {
        narrow: width < 1000.0,
        pictured: width >= 1550.0,
    };
    let kit = Kit {
        assets: &assets,
        prefix: &session.asset_prefix,
        session: &session,
    };
    match *screen {
        Screen::Title => title_screen(&mut commands, &kit, &menu),
        Screen::Nations => {
            menu.world();
            nation_screen(&mut commands, &kit, &menu, layout);
        }
        Screen::Campaign => {}
    }
}

/// Paint the world map for the chosen nation whenever the choice changes.
pub(crate) fn paint_chart(
    mut menu: ResMut<Menu>,
    screen: Res<Screen>,
    mut images: ResMut<Assets<Image>>,
) {
    if *screen != Screen::Nations || menu.charted == Some((menu.scenario, menu.nation)) {
        return;
    }
    let focus = menu.nation as Id + 1;
    let Some(game) = menu.world() else {
        return;
    };
    let (width, height, data) = presentation::chart_pixels(game, 0, Some(focus));
    // Each tile becomes a 3 x 3 block, so the smooth sampler softens only the blocks' edges.
    const SCALE: u32 = 3;
    let mut big = vec![0u8; (width * height * SCALE * SCALE * 4) as usize];
    for y in 0..height * SCALE {
        for x in 0..width * SCALE {
            let from = (((y / SCALE) * width + x / SCALE) * 4) as usize;
            let to = ((y * width * SCALE + x) * 4) as usize;
            big[to..to + 4].copy_from_slice(&data[from..from + 4]);
        }
    }
    let mut image = Image::new(
        bevy::render::render_resource::Extent3d {
            width: width * SCALE,
            height: height * SCALE,
            depth_or_array_layers: 1,
        },
        bevy::render::render_resource::TextureDimension::D2,
        big,
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::default(),
    );
    // A small map is drawn much larger than it is: keep its squares crisp.
    if width < 128 {
        image.sampler = bevy::image::ImageSampler::nearest();
    }
    match menu.chart.as_ref().and_then(|chart| images.get_mut(chart)) {
        Some(old) => *old = image,
        None => menu.chart = Some(images.add(image)),
    }
    menu.charted = Some((menu.scenario, menu.nation));
    menu.dirty = true;
}

/// Light a plate under the pointer.
pub(crate) fn paint(
    plates: Query<(&Interaction, &Plate, &Children), Changed<Interaction>>,
    mut glows: Query<&mut BackgroundColor, With<Glow>>,
) {
    for (interaction, plate, children) in &plates {
        let lit = match (plate, interaction) {
            (Plate::Dormant, _) | (_, Interaction::None) => 0.0,
            (_, Interaction::Hovered) => 1.0,
            (_, Interaction::Pressed) => 0.6,
        };
        for child in children.iter() {
            if let Ok(mut glow) = glows.get_mut(child) {
                glow.0 = glow_color(*plate, lit);
            }
        }
    }
}

fn glow_color(plate: Plate, lit: f32) -> Color {
    match plate {
        Plate::Crimson => {
            if lit == 0.0 {
                CRIMSON
            } else if lit < 1.0 {
                CRIMSON_PRESSED
            } else {
                CRIMSON_HOVER
            }
        }
        Plate::Paper => Color::srgba(1.0, 0.97, 0.86, 0.30 * lit),
        Plate::Slate | Plate::Cell(_) => Color::srgba(1.0, 0.92, 0.70, 0.10 * lit),
        Plate::Dormant => Color::NONE,
    }
}

/// Do what the pressed control says.
pub(crate) fn press(
    controls: Query<(&Interaction, &MenuAction, &Plate), Changed<Interaction>>,
    mut menu: ResMut<Menu>,
    mut screen: ResMut<Screen>,
    mut session: ResMut<Session>,
    mut connection: NonSendMut<Connection>,
    mut volume: ResMut<GlobalVolume>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(action) = controls
        .iter()
        .find(|(i, _, plate)| **i == Interaction::Pressed && **plate != Plate::Dormant)
        .map(|(_, action, _)| action.clone())
    else {
        return;
    };
    act(
        action,
        &mut menu,
        &mut screen,
        &mut session,
        &mut connection,
        &mut volume,
        windows.single_mut().ok().as_deref_mut(),
        &mut exit,
    );
}

#[allow(clippy::too_many_arguments)]
fn act(
    action: MenuAction,
    menu: &mut Menu,
    screen: &mut Screen,
    session: &mut Session,
    connection: &mut Connection,
    volume: &mut GlobalVolume,
    window: Option<&mut Window>,
    exit: &mut MessageWriter<AppExit>,
) {
    menu.dirty = true;
    match action {
        MenuAction::Continue | MenuAction::Load(_) => {
            let index = match action {
                MenuAction::Load(i) => i,
                _ => 0,
            };
            let Some(save) = menu.saves.get(index) else {
                return;
            };
            match read_save(save) {
                Ok(host) => begin(host, session, connection, screen),
                Err(error) => {
                    menu.notice = format!("{} could not be loaded: {error}", save.name);
                    menu.sheet = None;
                }
            }
        }
        MenuAction::NewCampaign => {
            menu.sheet = None;
            menu.notice.clear();
            *screen = Screen::Nations;
        }
        MenuAction::Open(sheet) => {
            if sheet == Sheet::Saves {
                menu.saves = find_saves();
            }
            menu.sheet = Some(sheet);
        }
        MenuAction::CloseSheet => menu.sheet = None,
        MenuAction::Exit => {
            exit.write(AppExit::Success);
        }
        MenuAction::Back => *screen = Screen::Title,
        MenuAction::Start => match start(menu) {
            Ok(host) => begin(host, session, connection, screen),
            Err(error) => menu.notice = format!("The campaign cannot start: {error}"),
        },
        MenuAction::Nation(i) => menu.nation = i,
        MenuAction::Scenario(step) => {
            let count = menu.scenarios.len() as i32;
            menu.scenario = (menu.scenario as i32 + step).rem_euclid(count) as usize;
            menu.nation = menu.commander_index();
        }
        MenuAction::Volume(step) => {
            let level = (volume.volume.to_linear() + step as f32 * 0.1).clamp(0.0, 1.0);
            volume.volume = Volume::Linear((level * 10.0).round() / 10.0);
        }
        MenuAction::Fullscreen => {
            menu.fullscreen = !menu.fullscreen;
            if let Some(window) = window {
                window.mode = if menu.fullscreen {
                    WindowMode::BorderlessFullscreen(MonitorSelection::Current)
                } else {
                    WindowMode::Windowed
                };
            }
        }
    }
}

/// Keys: Escape steps back, Enter takes the plate that moves on, and the arrows walk the list of
/// nations.
pub(crate) fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut menu: ResMut<Menu>,
    mut screen: ResMut<Screen>,
    mut session: ResMut<Session>,
    mut connection: NonSendMut<Connection>,
    mut volume: ResMut<GlobalVolume>,
    mut exit: MessageWriter<AppExit>,
) {
    let pressed = |k| keys.just_pressed(k);
    let action = match *screen {
        Screen::Campaign => None,
        Screen::Title => {
            if pressed(KeyCode::Escape) && menu.sheet.is_some() {
                Some(MenuAction::CloseSheet)
            } else if pressed(KeyCode::Enter) && menu.sheet.is_none() {
                Some(if menu.saves.is_empty() {
                    MenuAction::NewCampaign
                } else {
                    MenuAction::Continue
                })
            } else {
                None
            }
        }
        Screen::Nations => {
            let count = menu.scenario().nations.len() as i32;
            let step = if pressed(KeyCode::ArrowLeft) {
                -1
            } else if pressed(KeyCode::ArrowRight) {
                1
            } else if pressed(KeyCode::ArrowUp) {
                -GRID
            } else if pressed(KeyCode::ArrowDown) {
                GRID
            } else {
                0
            };
            if pressed(KeyCode::Escape) {
                Some(MenuAction::Back)
            } else if pressed(KeyCode::Enter) {
                Some(MenuAction::Start)
            } else if step != 0 {
                let next = (menu.nation as i32 + step).clamp(0, count - 1);
                Some(MenuAction::Nation(next as usize))
            } else {
                None
            }
        }
    };
    if let Some(action) = action {
        act(
            action,
            &mut menu,
            &mut screen,
            &mut session,
            &mut connection,
            &mut volume,
            None,
            &mut exit,
        );
    }
}

/// Fit pictures to their frames once their sizes are known.
pub(crate) fn fit(
    images: Res<Assets<Image>>,
    mut pictures: Query<(&Fit, &ImageNode, &ChildOf, &mut Node)>,
    frames: Query<&ComputedNode>,
) {
    for (fit, image, parent, mut node) in &mut pictures {
        let (Some(picture), Ok(frame)) = (images.get(&image.image), frames.get(parent.parent()))
        else {
            continue;
        };
        let picture = picture.size().as_vec2();
        let frame = frame.size() * frame.inverse_scale_factor();
        if picture.min_element() <= 0.0 || frame.min_element() <= 0.0 {
            continue;
        }
        let scale = match fit {
            Fit::Cover => (frame / picture).max_element(),
            Fit::Contain => (frame / picture).min_element(),
        };
        let size = picture * scale;
        let corner = (frame - size) / 2.0;
        let wanted = (px(size.x), px(size.y), px(corner.x), px(corner.y));
        if (node.width, node.height, node.left, node.top) != wanted {
            (node.width, node.height, node.left, node.top) = wanted;
            node.position_type = PositionType::Absolute;
        }
    }
}

/// Size the world map's frame to the largest box of the map's proportions that fits the room
/// left for it, its band included.
pub(crate) fn frame_chart(
    menu: Res<Menu>,
    images: Res<Assets<Image>>,
    mut frames: Query<(&ChildOf, &mut Node), With<ChartFrame>>,
    rooms: Query<&ComputedNode>,
) {
    let Some(chart) = menu.chart.as_ref().and_then(|chart| images.get(chart)) else {
        return;
    };
    let chart = chart.size().as_vec2();
    for (parent, mut node) in &mut frames {
        let Ok(room) = rooms.get(parent.parent()) else {
            continue;
        };
        let room = room.size() * room.inverse_scale_factor();
        let inner = room - Vec2::splat(2.0 * CHART_BAND);
        if chart.min_element() <= 0.0 || inner.min_element() <= 0.0 {
            continue;
        }
        let size = chart * (inner / chart).min_element() + Vec2::splat(2.0 * CHART_BAND);
        let wanted = (px(size.x), px(size.y));
        if (node.width, node.height) != wanted {
            (node.width, node.height) = wanted;
        }
    }
}

/// The wheel scrolls the list of nations.
pub(crate) fn scroll(
    mut wheel: MessageReader<MouseWheel>,
    mut list: Query<(&ComputedNode, &mut ScrollPosition), With<NationList>>,
) {
    let delta: f32 = wheel.read().map(|e| -e.y * 40.0).sum();
    if delta == 0.0 {
        return;
    }
    for (node, mut pos) in &mut list {
        let max = (node.content_size().y - node.size().y) * node.inverse_scale_factor();
        pos.y = (pos.y + delta).clamp(0.0, max.max(0.0));
    }
}

/// A click on the world map picks the nation whose land it falls on.
pub(crate) fn pick_on_chart(
    mut menu: ResMut<Menu>,
    chart: Query<(&Interaction, &RelativeCursorPosition), (With<WorldChart>, Changed<Interaction>)>,
) {
    for (interaction, cursor) in &chart {
        let (Interaction::Pressed, Some(at)) = (interaction, cursor.normalized) else {
            continue;
        };
        let Some(game) = menu.world() else {
            return;
        };
        let tile = Chart::of(&game.map).tile_at(at + Vec2::splat(0.5));
        let owner = game.map.get(tile).map_or(0, |t| t.owner);
        if owner > 0 && (owner as usize) <= menu.scenario().nations.len() {
            menu.nation = owner as usize - 1;
            menu.dirty = true;
        }
    }
}

/// The pack's music for these screens, in a loop, while they are up.
pub(crate) fn music(
    menu: Res<Menu>,
    screen: Res<Screen>,
    session: Res<Session>,
    assets: Res<AssetServer>,
    mut commands: Commands,
    playing: Query<Entity, With<Music>>,
) {
    let path = &menu.content.pack.visuals.menu.music;
    let wanted = *screen != Screen::Campaign && !path.is_empty();
    match (wanted, playing.single()) {
        (true, Err(_)) => {
            commands.spawn((
                AudioPlayer::new(assets.load(format!("{}{path}", session.asset_prefix))),
                PlaybackSettings::LOOP,
                Music,
            ));
        }
        (false, Ok(entity)) => commands.entity(entity).despawn(),
        _ => {}
    }
}

/// Marks the music under the title screens.
#[derive(Component)]
pub(crate) struct Music;

/// Whether the title screens play their own music, which the soundtrack waits for.
pub(crate) fn has_own_music(menu: &Menu, screen: Screen) -> bool {
    screen != Screen::Campaign && !menu.content.pack.visuals.menu.music.is_empty()
}

// --- building blocks ---------------------------------------------------------------------------

/// What the screens need to find the pack's pictures.
struct Kit<'a> {
    assets: &'a AssetServer,
    prefix: &'a str,
    session: &'a Session,
}
impl Kit<'_> {
    fn image(&self, path: &str) -> Handle<Image> {
        self.assets.load(format!("{}{path}", self.prefix))
    }
}

/// Text in the display face (Playfair Display), for titles and the words on plates.
fn display(p: &mut ChildSpawnerCommands, value: impl Into<String>, size: f32, color: Color) {
    p.spawn((
        Node {
            flex_shrink: 0.0,
            ..default()
        },
        Text::new(value),
        TextFont {
            font: DISPLAY,
            font_size: size,
            weight: FontWeight(500),
            ..default()
        },
        TextColor(color),
        TextLayout::new_with_justify(Justify::Center),
    ));
}

/// Running text that wraps within its box.
fn prose(p: &mut ChildSpawnerCommands, value: impl Into<String>, size: f32, color: Color) {
    p.spawn((
        Node {
            width: percent(100),
            ..default()
        },
        Text::new(value),
        font(size, FontWeight::NORMAL),
        TextColor(color),
    ));
}

/// Small capitals, spaced out: `A GLOBAL POWER`.
fn spaced(p: &mut ChildSpawnerCommands, value: &str, size: f32, color: Color) {
    let spaced: String = value
        .to_uppercase()
        .chars()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join("\u{2009}");
    p.spawn((
        Node {
            flex_shrink: 0.0,
            ..default()
        },
        Text::new(spaced),
        font(size, FontWeight::SEMIBOLD),
        TextColor(color),
        // Thin spaces sit between its letters: never break a word there.
        TextLayout::new_with_linebreak(LineBreak::NoWrap),
    ));
}

/// A rule with a diamond at its middle, `width` wide.
fn ornament(p: &mut ChildSpawnerCommands, width: Val, color: Color) {
    p.spawn(Node {
        width,
        // Without a width of its own it shares out the room left in its row.
        flex_grow: if width == Val::Auto { 1.0 } else { 0.0 },
        align_items: AlignItems::Center,
        column_gap: px(6),
        flex_shrink: 0.0,
        ..default()
    })
    .with_children(|p| {
        let line = || {
            (
                Node {
                    flex_grow: 1.0,
                    height: px(1),
                    ..default()
                },
                BackgroundColor(color),
            )
        };
        p.spawn(line());
        p.spawn((
            Node {
                width: px(6),
                height: px(6),
                ..default()
            },
            BackgroundColor(color),
            UiTransform::from_rotation(Rot2::degrees(45.0)),
        ));
        p.spawn(line());
    });
}

/// A layer as big as its parent, under what follows it.
fn layer() -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: px(0),
        top: px(0),
        width: percent(100),
        height: percent(100),
        ..default()
    }
}

/// A picture fitted to a frame that clips it.
fn fitted(p: &mut ChildSpawnerCommands, image: Handle<Image>, fit: Fit) {
    p.spawn((Node {
        overflow: Overflow::clip(),
        ..layer()
    },))
        .with_children(|p| {
            p.spawn((
                ImageNode::new(image),
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
                fit,
            ));
        });
}

/// A plate: a button with a surface, a frame and a label.
fn plate(
    p: &mut ChildSpawnerCommands,
    kit: &Kit,
    label: &str,
    action: MenuAction,
    style: Plate,
    node: Node,
    size: f32,
) {
    let (fill, ink) = match style {
        Plate::Crimson => (CRIMSON, CREAM),
        Plate::Paper => (WELL, INK),
        Plate::Slate | Plate::Cell(_) => (SLATE, CREAM),
        Plate::Dormant => (WELL, INK.with_alpha(0.38)),
    };
    p.spawn((
        Button,
        action,
        style,
        Node {
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            flex_shrink: 0.0,
            ..node
        },
        BackgroundColor(fill),
        lift(),
    ))
    .with_children(|p| {
        match style {
            Plate::Paper | Plate::Dormant => {
                texture(p, kit.assets, kit.prefix, Skin::Paper, 1.0);
            }
            Plate::Slate | Plate::Cell(_) => texture(p, kit.assets, kit.prefix, Skin::Slate, 1.0),
            Plate::Crimson => {}
        }
        if style == Plate::Dormant {
            p.spawn((layer(), BackgroundColor(Color::srgba(0.2, 0.15, 0.1, 0.35))));
        }
        p.spawn((layer(), BackgroundColor(glow_color(style, 0.0)), Glow));
        trim(p, kit.assets, kit.prefix, Frame::Small);
        display(p, label, size, ink);
    });
}

/// A small square plate with a chevron pointing `step`'s way.
fn arrow(p: &mut ChildSpawnerCommands, kit: &Kit, step: i32) {
    p.spawn((
        Button,
        MenuAction::Scenario(step),
        Plate::Paper,
        Node {
            width: px(40),
            height: px(40),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            flex_shrink: 0.0,
            ..default()
        },
        BackgroundColor(WELL),
        lift(),
    ))
    .with_children(|p| {
        texture(p, kit.assets, kit.prefix, Skin::Paper, 1.0);
        p.spawn((
            layer(),
            BackgroundColor(glow_color(Plate::Paper, 0.0)),
            Glow,
        ));
        trim(p, kit.assets, kit.prefix, Frame::Small);
        // Two sides of a square turned on its corner.
        p.spawn((
            Node {
                width: px(11),
                height: px(11),
                margin: UiRect::left(px(if step < 0 { 5.0 } else { -5.0 })),
                border: UiRect::new(px(0), px(2.5), px(2.5), px(0)),
                ..default()
            },
            BorderColor::all(INK),
            UiTransform::from_rotation(Rot2::degrees(if step < 0 { -135.0 } else { 45.0 })),
        ));
    });
}

/// The full-window root of a screen.
fn root<'a>(commands: &'a mut Commands<'_, '_>) -> EntityCommands<'a> {
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            ..default()
        },
        BackgroundColor(NIGHT),
        FocusPolicy::Block,
        GlobalZIndex(80),
        MenuRoot,
    ))
}

/// The desk under a screen: the pack's surface, else its walnut.
fn desk(p: &mut ChildSpawnerCommands, kit: &Kit, path: &str) {
    if path.is_empty() {
        p.spawn((
            layer(),
            ImageNode::new(kit.image("margin/desk.png")).with_mode(NodeImageMode::Tiled {
                tile_x: true,
                tile_y: true,
                stretch_value: 1.0,
            }),
        ));
    } else {
        fitted(p, kit.image(path), Fit::Cover);
    }
}

// --- the title screen ----------------------------------------------------------------------------

fn title_screen(commands: &mut Commands, kit: &Kit, menu: &Menu) {
    let art = &menu.content.pack.visuals.menu;
    root(commands).with_children(|p| {
        // The painting, or the desk with the harbour engraving along its foot.
        if art.background.is_empty() {
            desk(p, kit, "");
            p.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(0),
                    bottom: px(0),
                    width: percent(100),
                    height: percent(42),
                    ..default()
                },
                ImageNode::new(kit.image("ui/city_panorama.png"))
                    .with_color(Color::srgba(1.0, 0.96, 0.88, 0.55)),
            ));
        } else {
            fitted(p, kit.image(&art.background), Fit::Cover);
        }
        // Darken the edges so the plates read over any painting.
        p.spawn((
            layer(),
            BackgroundGradient::from(LinearGradient::to_bottom(vec![
                ColorStop::percent(Color::srgba(0.0, 0.0, 0.0, 0.45), 0.0),
                ColorStop::percent(Color::srgba(0.0, 0.0, 0.0, 0.0), 30.0),
                ColorStop::percent(Color::srgba(0.0, 0.0, 0.0, 0.0), 70.0),
                ColorStop::percent(Color::srgba(0.0, 0.0, 0.0, 0.5), 100.0),
            ])),
        ));
        p.spawn(Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            padding: UiRect::vertical(percent(3)),
            row_gap: px(10),
            ..default()
        })
        .with_children(|p| {
            title_plate(p, kit, &art.logo);
            p.spawn(Node {
                flex_grow: 1.0,
                max_height: px(70),
                ..default()
            });
            p.spawn(Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(10),
                ..default()
            })
            .with_children(|p| {
                let wide = |style| (style, 330.0);
                let mut entries: Vec<(&str, MenuAction, Plate, f32)> = Vec::new();
                if !menu.saves.is_empty() {
                    entries.push(("Continue", MenuAction::Continue, Plate::Crimson, 360.0));
                }
                let (style, width) = if menu.saves.is_empty() {
                    (Plate::Crimson, 360.0)
                } else {
                    wide(Plate::Paper)
                };
                entries.push(("New Campaign", MenuAction::NewCampaign, style, width));
                let (style, width) = wide(if menu.saves.is_empty() {
                    Plate::Dormant
                } else {
                    Plate::Paper
                });
                entries.push(("Load Game", MenuAction::Open(Sheet::Saves), style, width));
                let (style, width) = wide(Plate::Paper);
                entries.push(("Settings", MenuAction::Open(Sheet::Settings), style, width));
                entries.push(("Credits", MenuAction::Open(Sheet::Credits), style, width));
                // A browser tab or a phone app is closed by its host, not by the game.
                #[cfg(not(any(
                    target_arch = "wasm32",
                    target_os = "android",
                    target_os = "ios"
                )))]
                entries.push(("Exit", MenuAction::Exit, style, width));
                for (label, action, style, width) in entries {
                    let big = style == Plate::Crimson;
                    plate(
                        p,
                        kit,
                        label,
                        action,
                        style,
                        Node {
                            width: px(width),
                            height: px(if big { 62.0 } else { 54.0 }),
                            ..default()
                        },
                        if big { 28.0 } else { 23.0 },
                    );
                }
            });
            if !menu.notice.is_empty() {
                notice(p, &menu.notice);
            }
            p.spawn(Node {
                flex_grow: 1.0,
                ..default()
            });
            // The version, between two gilt rules.
            p.spawn(Node {
                align_items: AlignItems::Center,
                column_gap: px(10),
                ..default()
            })
            .with_children(|p| {
                ornament(p, px(60), TRIM);
                display(p, format!("v{}", env!("CARGO_PKG_VERSION")), 16.0, TRIM);
                ornament(p, px(60), TRIM);
            });
        });
        if let Some(sheet) = menu.sheet {
            sheet_over(p, kit, menu, sheet);
        }
    });
}

/// The game's name: the pack's lettered logo, else a slate plate in a gilt frame.
fn title_plate(p: &mut ChildSpawnerCommands, kit: &Kit, logo: &str) {
    if !logo.is_empty() {
        p.spawn((
            ImageNode::new(kit.image(logo)),
            Node {
                max_width: percent(70),
                max_height: percent(30),
                ..default()
            },
        ));
        return;
    }
    p.spawn((
        Node {
            padding: UiRect::axes(px(72), px(30)),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(4),
            ..default()
        },
        BackgroundColor(SLATE),
        lift(),
    ))
    .with_children(|p| {
        dress(p, kit.assets, kit.prefix, Skin::Slate, Frame::Grand, true);
        display(p, TITLE, 84.0, TRIM);
        ornament(p, percent(80), TRIM.with_alpha(0.7));
        spaced(p, SUBTITLE, 13.0, MUTED);
    });
}

/// The game's name on the title screen, until the pack brings a logo.
const TITLE: &str = "Open 4X";
const SUBTITLE: &str = "Empires of the industrial age";

/// A line of bad news under the plates.
fn notice(p: &mut ChildSpawnerCommands, message: &str) {
    p.spawn((
        Node {
            margin: UiRect::top(px(8)),
            padding: UiRect::axes(px(14), px(6)),
            max_width: px(560),
            border_radius: rounded(4.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.05, 0.04, 0.03, 0.85)),
    ))
    .with_children(|p| prose(p, message, 13.0, Color::srgb(0.95, 0.62, 0.55)));
}

/// A parchment sheet over the dimmed title screen.
fn sheet_over(p: &mut ChildSpawnerCommands, kit: &Kit, menu: &Menu, sheet: Sheet) {
    p.spawn((
        Node {
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..layer()
        },
        BackgroundColor(Color::srgba(0.02, 0.02, 0.03, 0.62)),
        FocusPolicy::Block,
        // A click beside the sheet closes it.
        Button,
        MenuAction::CloseSheet,
        Plate::Dormant,
    ))
    .with_children(|p| {
        p.spawn((
            Node {
                width: px(560),
                max_width: percent(94),
                max_height: percent(86),
                padding: UiRect::axes(px(40), px(34)),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(12),
                ..default()
            },
            BackgroundColor(WELL),
            lift(),
            FocusPolicy::Block,
            // Swallows the click, so that one on the sheet does not close it.
            Button,
        ))
        .with_children(|p| {
            dress(p, kit.assets, kit.prefix, Skin::Paper, Frame::Grand, true);
            let title = match sheet {
                Sheet::Saves => "Load Game",
                Sheet::Settings => "Settings",
                Sheet::Credits => "Credits",
            };
            display(p, title, 34.0, INK);
            ornament(p, percent(60), INK_SOFT.with_alpha(0.6));
            p.spawn(Node {
                width: percent(100),
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                margin: UiRect::vertical(px(6)),
                overflow: Overflow::clip(),
                ..default()
            })
            .with_children(|p| match sheet {
                Sheet::Saves => saves_list(p, kit, menu),
                Sheet::Settings => settings(p, kit, menu),
                Sheet::Credits => credits(p),
            });
            plate(
                p,
                kit,
                "Close",
                MenuAction::CloseSheet,
                Plate::Slate,
                Node {
                    width: px(180),
                    height: px(46),
                    ..default()
                },
                20.0,
            );
        });
    });
}

fn saves_list(p: &mut ChildSpawnerCommands, kit: &Kit, menu: &Menu) {
    if menu.saves.is_empty() {
        prose(
            p,
            "No saved campaigns yet. Press F5 during a campaign to save it.",
            16.0,
            INK_SOFT,
        );
        return;
    }
    for (i, save) in menu.saves.iter().enumerate().take(8) {
        p.spawn((
            Button,
            MenuAction::Load(i),
            Plate::Paper,
            Node {
                width: percent(100),
                padding: UiRect::axes(px(18), px(10)),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(WELL),
        ))
        .with_children(|p| {
            texture(p, kit.assets, kit.prefix, Skin::Paper, 1.0);
            p.spawn((layer(), BackgroundColor(Color::NONE), Glow));
            trim(p, kit.assets, kit.prefix, Frame::Small);
            // The save F5 writes is the quick save.
            let name = if save.name == "dawn" {
                "Quick save (F5)".to_string()
            } else {
                save.name.clone()
            };
            crate::theme::bold(p, name, 17.0, INK);
            crate::theme::text(p, save.when.clone(), 12.5, INK_SOFT);
        });
    }
}

fn settings(p: &mut ChildSpawnerCommands, kit: &Kit, menu: &Menu) {
    setting_row(p, kit, "Volume", |p, kit| {
        step_plate(p, kit, "\u{2212}", MenuAction::Volume(-1));
        p.spawn((
            Node {
                width: px(70),
                justify_content: JustifyContent::Center,
                ..default()
            },
            VolumeLabel,
            Text::new(""),
            font(17.0, FontWeight::SEMIBOLD),
            TextColor(INK),
        ));
        step_plate(p, kit, "+", MenuAction::Volume(1));
    });
    #[cfg(not(any(target_arch = "wasm32", target_os = "android", target_os = "ios")))]
    setting_row(p, kit, "Display", |p, kit| {
        plate(
            p,
            kit,
            if menu.fullscreen {
                "Fullscreen"
            } else {
                "Windowed"
            },
            MenuAction::Fullscreen,
            Plate::Paper,
            Node {
                width: px(170),
                height: px(42),
                ..default()
            },
            18.0,
        );
    });
    #[cfg(any(target_arch = "wasm32", target_os = "android", target_os = "ios"))]
    let _ = menu;
    prose(p, "Settings last until the game is closed.", 13.0, INK_SOFT);
}

/// A small square plate with a sign on it.
fn step_plate(p: &mut ChildSpawnerCommands, kit: &Kit, sign: &str, action: MenuAction) {
    let node = Node {
        width: px(40),
        height: px(40),
        ..default()
    };
    plate(p, kit, sign, action, Plate::Paper, node, 24.0);
}

/// The figure of the volume setting, kept current by [`volume_label`].
#[derive(Component)]
pub(crate) struct VolumeLabel;

pub(crate) fn volume_label(
    volume: Res<GlobalVolume>,
    mut labels: Query<&mut Text, With<VolumeLabel>>,
) {
    let figure = format!("{}%", (volume.volume.to_linear() * 100.0).round() as i32);
    for mut text in &mut labels {
        if text.0 != figure {
            text.0 = figure.clone();
        }
    }
}

fn setting_row(
    p: &mut ChildSpawnerCommands,
    kit: &Kit,
    label: &str,
    control: impl FnOnce(&mut ChildSpawnerCommands, &Kit),
) {
    p.spawn((
        Node {
            width: percent(100),
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            padding: UiRect::vertical(px(6)),
            border: UiRect::bottom(px(1)),
            ..default()
        },
        BorderColor::all(RULE),
    ))
    .with_children(|p| {
        crate::theme::bold(p, label, 17.0, INK);
        p.spawn(Node {
            align_items: AlignItems::Center,
            column_gap: px(8),
            ..default()
        })
        .with_children(|p| control(p, kit));
    });
}

fn credits(p: &mut ChildSpawnerCommands) {
    for (role, who) in [
        (
            "Game",
            "Open 4X, an independent strategy game in Rust and Bevy",
        ),
        (
            "World map",
            "Converted from the community Civ3 scenario Age of Imperialism 5.0",
        ),
        (
            "Flags and music",
            "From the Age of Imperialism scenario (see assets/PROVENANCE.md)",
        ),
        (
            "Art and sound",
            "Drawn and synthesized in code for this project",
        ),
        (
            "Typefaces",
            "EB Garamond and Playfair Display, SIL Open Font License",
        ),
    ] {
        p.spawn(Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|p| {
            crate::theme::caption(p, role);
            p.spawn((
                Node {
                    width: percent(100),
                    ..default()
                },
                Text::new(who),
                font(15.0, FontWeight::NORMAL),
                TextColor(INK),
                TextLayout::new_with_justify(Justify::Center),
            ));
        });
    }
}

// --- the nation selection screen -----------------------------------------------------------------

/// Nations per row of the list.
const GRID: i32 = 4;
/// Width of the picture beside a profile, a painted scene or the nation's flag alike.
const PROFILE_PICTURE: f32 = 270.0;
/// The sea between the world map and its frame's edge, which the frame's band sits over.
const CHART_BAND: f32 = 16.0;

/// How much of the nation selection screen the window has room for.
#[derive(Clone, Copy)]
struct Layout {
    /// One column: the list above the profile, which drops its strengths and challenges.
    narrow: bool,
    /// The profile has room for its picture beside the text; without it the flag stands by
    /// the nation's name.
    pictured: bool,
}

fn nation_screen(commands: &mut Commands, kit: &Kit, menu: &Menu, layout: Layout) {
    let Layout { narrow, .. } = layout;
    let scenario = menu.scenario();
    let art = &menu.content.pack.visuals.menu;
    root(commands).with_children(|p| {
        desk(p, kit, &art.desk);
        p.spawn(Node {
            width: percent(100),
            height: percent(100),
            padding: UiRect::all(px(14)),
            column_gap: px(14),
            flex_direction: if narrow {
                FlexDirection::Column
            } else {
                FlexDirection::Row
            },
            ..default()
        })
        .with_children(|p| {
            nation_list(p, kit, menu, scenario, narrow);
            p.spawn(Node {
                flex_grow: 1.0,
                flex_basis: px(0),
                min_width: px(0),
                min_height: px(0),
                flex_direction: FlexDirection::Column,
                row_gap: px(14),
                ..default()
            })
            .with_children(|p| {
                profiles(p, kit, menu, scenario, layout);
                world_map(p, kit, menu);
                footer(p, kit, menu, scenario);
            });
        });
    });
}

/// The list of nations, each its flag, number and name.
fn nation_list(
    p: &mut ChildSpawnerCommands,
    kit: &Kit,
    menu: &Menu,
    scenario: &Scenario,
    narrow: bool,
) {
    p.spawn((
        Node {
            width: if narrow { percent(100) } else { percent(31) },
            min_width: px(330),
            max_width: px(520),
            height: if narrow { percent(40) } else { percent(100) },
            padding: UiRect::axes(px(26), px(24)),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(6),
            flex_shrink: 0.0,
            ..default()
        },
        BackgroundColor(SLATE),
        lift(),
    ))
    .with_children(|p| {
        dress(p, kit.assets, kit.prefix, Skin::Slate, Frame::Grand, true);
        display(p, "Select a Nation", 40.0, CREAM);
        p.spawn(Node {
            width: percent(100),
            align_items: AlignItems::Center,
            column_gap: px(10),
            margin: UiRect::bottom(px(8)),
            ..default()
        })
        .with_children(|p| {
            ornament(p, Val::Auto, TRIM.with_alpha(0.5));
            spaced(
                p,
                &format!(
                    "{} nations \u{00b7} {}",
                    scenario.nations.len(),
                    scenario.start_date.year()
                ),
                12.0,
                MUTED,
            );
            ornament(p, Val::Auto, TRIM.with_alpha(0.5));
        });
        p.spawn((
            Node {
                width: percent(100),
                flex_grow: 1.0,
                min_height: px(0),
                flex_wrap: FlexWrap::Wrap,
                align_content: AlignContent::FlexStart,
                column_gap: percent(2),
                row_gap: px(8),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            ScrollPosition::default(),
            NationList,
        ))
        .with_children(|p| {
            let owners = menu.world.as_ref().map(|(_, game)| game);
            for (i, nation) in scenario.nations.iter().enumerate() {
                let chosen = i == menu.nation;
                let color = owners.map_or(Color::srgb(0.5, 0.5, 0.5), |game| {
                    presentation::nation_color(game, i as Id + 1, 1.0)
                });
                p.spawn((
                    Button,
                    MenuAction::Nation(i),
                    Plate::Cell(chosen),
                    Node {
                        width: percent(23),
                        padding: UiRect::new(px(6), px(6), px(7), px(5)),
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: px(2),
                        border: UiRect::all(px(if chosen { 2.0 } else { 1.0 })),
                        border_radius: rounded(3.0),
                        ..default()
                    },
                    BackgroundColor(if chosen {
                        Color::srgb(0.16, 0.15, 0.12)
                    } else {
                        INSET
                    }),
                    BorderColor::all(if chosen { TRIM } else { TRIM.with_alpha(0.18) }),
                ))
                .with_children(|p| {
                    p.spawn((
                        layer(),
                        BackgroundColor(glow_color(Plate::Cell(chosen), 0.0)),
                        Glow,
                    ));
                    flag_cut(p, kit, &nation.id, color, percent(100));
                    crate::theme::text(p, (i + 1).to_string(), 10.0, MUTED);
                    p.spawn((
                        Node {
                            width: percent(100),
                            ..default()
                        },
                        Text::new(nation.name.clone()),
                        font(13.0, FontWeight::NORMAL),
                        TextColor(if chosen { TRIM } else { ON_PLATE }),
                        TextLayout::new_with_justify(Justify::Center),
                    ));
                });
            }
        });
    });
}

/// A nation's flag at 3:2, cut from the middle of the pack's portrait of it, else its colour.
fn flag_cut(p: &mut ChildSpawnerCommands, kit: &Kit, tag: &str, color: Color, width: Val) {
    p.spawn((
        Node {
            width,
            aspect_ratio: Some(1.5),
            border: UiRect::all(px(1)),
            flex_shrink: 0.0,
            overflow: Overflow::clip(),
            ..default()
        },
        BackgroundColor(color),
        BorderColor::all(BRONZE),
    ))
    .with_children(|p| {
        if let Some(path) = kit.session.pack.visuals.nations.get(tag) {
            let mut image = ImageNode::new(kit.image(path));
            image.rect = Some(Rect::new(0.0, 53.0, 200.0, 53.0 + 200.0 / 1.5));
            p.spawn((
                image,
                Node {
                    width: percent(100),
                    height: percent(100),
                    ..default()
                },
            ));
        }
    });
}

/// The chosen nation: its flag and name, its profile, and its picture.
/// Every nation's profile laid in the same cell, only the chosen one shown: the panel is as tall
/// as the tallest of them, so choosing another nation never moves the map below it.
fn profiles(
    p: &mut ChildSpawnerCommands,
    kit: &Kit,
    menu: &Menu,
    scenario: &Scenario,
    layout: Layout,
) {
    p.spawn(Node {
        width: percent(100),
        flex_shrink: 0.0,
        display: Display::Grid,
        grid_template_columns: vec![GridTrack::flex(1.0)],
        ..default()
    })
    .with_children(|p| {
        for (i, nation) in scenario.nations.iter().enumerate() {
            profile(p, kit, menu, i, nation, layout);
        }
    });
}

fn profile(
    p: &mut ChildSpawnerCommands,
    kit: &Kit,
    menu: &Menu,
    index: usize,
    nation: &fourx_sim::NationStart,
    layout: Layout,
) {
    let Layout { narrow, pictured } = layout;
    let id = index as Id + 1;
    let game = menu.world.as_ref().map(|(_, game)| game);
    let color = game.map_or(Color::srgb(0.5, 0.5, 0.5), |g| {
        presentation::nation_color(g, id, 1.0)
    });
    let scene = menu.content.pack.visuals.menu.scenes.get(&nation.id);
    p.spawn((
        Node {
            width: percent(100),
            grid_row: GridPlacement::start(1),
            grid_column: GridPlacement::start(1),
            padding: UiRect::axes(px(34), px(28)),
            column_gap: px(28),
            ..default()
        },
        BackgroundColor(WELL),
        lift(),
        if index == menu.nation {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        },
    ))
    .with_children(|p| {
        dress(p, kit.assets, kit.prefix, Skin::Paper, Frame::Grand, true);
        // Name, epithet and summary, with the facts of the start date under them.
        p.spawn(Node {
            flex_grow: 1.4,
            flex_basis: px(0),
            min_width: px(0),
            flex_direction: FlexDirection::Column,
            row_gap: px(10),
            ..default()
        })
        .with_children(|p| {
            p.spawn(Node {
                align_items: AlignItems::Center,
                column_gap: px(18),
                // As tall as the flag, whether or not it is shown here.
                min_height: px(80),
                ..default()
            })
            .with_children(|p| {
                if scene.is_some() || !pictured {
                    flag_cut(p, kit, &nation.id, color, px(120));
                }
                p.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    min_width: px(0),
                    overflow: Overflow::clip_x(),
                    ..default()
                })
                .with_children(|p| {
                    p.spawn((
                        Text::new(nation.name.clone()),
                        TextFont {
                            font: DISPLAY,
                            font_size: if narrow { 30.0 } else { 46.0 },
                            weight: FontWeight(500),
                            ..default()
                        },
                        TextColor(INK),
                    ));
                    if !nation.profile.epithet.is_empty() {
                        spaced(p, &nation.profile.epithet, 13.0, INK_SOFT);
                    }
                });
            });
            ornament(p, percent(100), INK_SOFT.with_alpha(0.45));
            let summary = if nation.profile.summary.is_empty() {
                format!(
                    "Lead the {} people from {}.",
                    if nation.adjective.is_empty() {
                        &nation.name
                    } else {
                        &nation.adjective
                    },
                    menu.scenario().start_date
                )
            } else {
                nation.profile.summary.clone()
            };
            prose(p, summary, if narrow { 13.0 } else { 15.5 }, INK);
            // What the start date gives the nation.
            p.spawn(Node {
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(8),
                row_gap: px(6),
                margin: UiRect::top(px(2)),
                ..default()
            })
            .with_children(|p| {
                if !nation.leader.trim().is_empty() {
                    chip(p, nation.leader.trim().to_string());
                }
                if !nation.government.is_empty() {
                    chip(p, nation.government.clone());
                }
                if let Some(game) = game {
                    let cities = game.cities.values().filter(|c| c.owner == id).count();
                    let units = game.units.values().filter(|u| u.owner == id).count();
                    chip(p, plural(cities, "city", "cities"));
                    chip(p, plural(units, "unit", "units"));
                    if let Some(faction) = game.factions.get(&id) {
                        chip(p, format!("{} gold", faction.gold));
                    }
                }
            });
        });
        if narrow {
            return;
        }
        // Strengths and challenges.
        if !nation.profile.strengths.is_empty() || !nation.profile.challenges.is_empty() {
            p.spawn((
                Node {
                    width: px(236),
                    flex_shrink: 0.0,
                    flex_direction: FlexDirection::Column,
                    row_gap: px(5),
                    padding: UiRect::left(px(22)),
                    border: UiRect::left(px(1)),
                    ..default()
                },
                BorderColor::all(RULE),
            ))
            .with_children(|p| {
                traits(p, "Strengths", &nation.profile.strengths, GOOD);
                p.spawn(Node {
                    height: px(8),
                    ..default()
                });
                traits(p, "Challenges", &nation.profile.challenges, BAD);
            });
        }
        if !pictured {
            return;
        }
        // The nation's picture, or its leader's portrait (the flag).
        p.spawn((
            Node {
                width: px(PROFILE_PICTURE),
                flex_shrink: 0.0,
                border: UiRect::all(px(3)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(color),
            BorderColor::all(TRIM),
            BoxShadow::new(
                Color::srgba(0.0, 0.0, 0.0, 0.35),
                px(0),
                px(2),
                px(0),
                px(6),
            ),
        ))
        .with_children(|p| match scene {
            Some(path) => fitted(p, kit.image(path), Fit::Cover),
            None => {
                if let Some(path) = kit.session.pack.visuals.nations.get(&nation.id) {
                    fitted(p, kit.image(path), Fit::Cover);
                } else {
                    framed_flag(p, color, &nation.id, None, 164.0, 198.0);
                }
            }
        });
    });
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// A fact in a pill.
fn chip(p: &mut ChildSpawnerCommands, value: String) {
    p.spawn((
        Node {
            padding: UiRect::axes(px(12), px(4)),
            border: UiRect::all(px(1)),
            border_radius: rounded(999.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.42, 0.30, 0.14, 0.10)),
        BorderColor::all(EDGE),
    ))
    .with_children(|p| crate::theme::text(p, value, 12.5, INK));
}

/// A heading and its lines, each behind a diamond of `mark`.
fn traits(p: &mut ChildSpawnerCommands, heading: &str, lines: &[String], mark: Color) {
    if lines.is_empty() {
        return;
    }
    p.spawn((
        Text::new(heading),
        TextFont {
            font: DISPLAY,
            font_size: 21.0,
            weight: FontWeight(500),
            ..default()
        },
        TextColor(INK),
    ));
    for line in lines {
        p.spawn(Node {
            align_items: AlignItems::Center,
            column_gap: px(10),
            ..default()
        })
        .with_children(|p| {
            p.spawn((
                Node {
                    width: px(7),
                    height: px(7),
                    flex_shrink: 0.0,
                    ..default()
                },
                BackgroundColor(mark),
                UiTransform::from_rotation(Rot2::degrees(45.0)),
            ));
            crate::theme::text(p, line.clone(), 14.0, INK);
        });
    }
}

/// The world on the start date, the chosen nation's land lit, in a frame of its own proportions
/// centred in the room left for it.
fn world_map(p: &mut ChildSpawnerCommands, kit: &Kit, menu: &Menu) {
    p.spawn(Node {
        width: percent(100),
        flex_grow: 1.0,
        min_height: px(160),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    })
    .with_children(|p| {
        p.spawn((
            Node {
                width: percent(100),
                height: percent(100),
                padding: UiRect::all(px(CHART_BAND)),
                flex_shrink: 0.0,
                ..default()
            },
            BackgroundColor(Color::srgb(0.086, 0.204, 0.329)),
            lift(),
            ChartFrame,
        ))
        .with_children(|p| {
            // The frame's band sits over the chart's edge.
            p.spawn(Node {
                flex_grow: 1.0,
                overflow: Overflow::clip(),
                ..default()
            })
            .with_children(|p| {
                if let Some(chart) = &menu.chart {
                    p.spawn((
                        ImageNode::new(chart.clone()),
                        Node {
                            position_type: PositionType::Absolute,
                            ..default()
                        },
                        Fit::Contain,
                        Button,
                        WorldChart,
                        RelativeCursorPosition::default(),
                    ));
                }
            });
            trim(p, kit.assets, kit.prefix, Frame::Grand);
            // A hint in the lower left.
            p.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(28),
                    bottom: px(22),
                    padding: UiRect::axes(px(10), px(3)),
                    border_radius: rounded(3.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.05, 0.06, 0.08, 0.72)),
            ))
            .with_children(|p| {
                crate::theme::text(p, "Click a nation on the map to choose it", 12.0, MUTED)
            });
        });
    });
}

/// Back, the scenario, and Start Game.
fn footer(p: &mut ChildSpawnerCommands, kit: &Kit, menu: &Menu, scenario: &Scenario) {
    p.spawn((
        Node {
            width: percent(100),
            flex_shrink: 0.0,
            padding: UiRect::axes(px(26), px(16)),
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            column_gap: px(16),
            ..default()
        },
        BackgroundColor(WELL),
        lift(),
    ))
    .with_children(|p| {
        dress(p, kit.assets, kit.prefix, Skin::Paper, Frame::Small, false);
        plate(
            p,
            kit,
            "Back",
            MenuAction::Back,
            Plate::Slate,
            Node {
                width: px(200),
                height: px(54),
                ..default()
            },
            23.0,
        );
        p.spawn(Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            flex_grow: 1.0,
            min_width: px(0),
            row_gap: px(4),
            ..default()
        })
        .with_children(|p| {
            crate::theme::caption(p, "Campaign");
            p.spawn(Node {
                align_items: AlignItems::Center,
                column_gap: px(14),
                ..default()
            })
            .with_children(|p| {
                let several = menu.scenarios.len() > 1;
                if several {
                    arrow(p, kit, -1);
                }
                p.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    min_width: px(260),
                    ..default()
                })
                .with_children(|p| {
                    display(p, scenario.name.clone(), 21.0, INK);
                    let date = scenario.start_date.to_string();
                    // Most scenarios already carry their date in their name.
                    if !scenario.name.contains(&date) {
                        crate::theme::text(p, format!("Begins {date}"), 12.5, INK_SOFT);
                    }
                });
                if several {
                    arrow(p, kit, 1);
                }
            });
            if !menu.notice.is_empty() {
                crate::theme::text(p, menu.notice.clone(), 12.0, BAD);
            }
        });
        plate(
            p,
            kit,
            "Start Game",
            MenuAction::Start,
            Plate::Crimson,
            Node {
                width: px(260),
                height: px(58),
                ..default()
            },
            26.0,
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bundled_nation_has_a_profile() {
        let content = Content::base();
        for id in ["age-of-imperialism", "russo-japanese-war"] {
            for nation in &content.scenarios[id].nations {
                let profile = &nation.profile;
                assert!(
                    !profile.epithet.is_empty() && !profile.summary.is_empty(),
                    "{id}: {}",
                    nation.id
                );
                assert_eq!(profile.strengths.len(), 3, "{id}: {}", nation.id);
                assert_eq!(profile.challenges.len(), 3, "{id}: {}", nation.id);
            }
        }
    }

    #[test]
    fn the_menu_opens_on_the_default_commander() {
        let menu = Menu::new(Content::base(), 42);
        let scenario = menu.scenario();
        assert_eq!(scenario.nations[menu.nation].id, scenario.commander);
    }

    #[test]
    fn the_chosen_nation_starts_the_campaign_it_was_chosen_for() {
        let mut menu = Menu::new(Content::base(), 42);
        menu.scenario = menu
            .scenarios
            .iter()
            .position(|s| s == "russo-japanese-war")
            .unwrap();
        menu.nation = 1;
        let host = start(&menu).expect("the starter begins");
        assert_eq!(host.game.factions[&host.commander()].name, "Russia");
    }

    #[test]
    fn a_save_is_dated_by_the_clock() {
        assert_eq!(written(0), "Saved 1 January 1970, 00:00 UTC");
        assert_eq!(written(86_400 + 3_660), "Saved 2 January 1970, 01:01 UTC");
    }
}
