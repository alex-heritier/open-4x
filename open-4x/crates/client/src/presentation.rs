use super::*;
use bevy::image::{ImageLoaderSettings, ImageSampler};
use bevy::ui::RelativeCursorPosition;
use fourx_content::Visuals;
use fourx_sim::{
    Date, Flavor, Order, Unit, UnitDef, VISION_RADIUS,
    terrain::{Cover, Relief, Terrain, Tile},
};
use std::collections::{BTreeMap, HashMap, HashSet};

const PAPER: Color = Color::srgb(0.88, 0.83, 0.69);
const GOLD: Color = Color::srgb(0.78, 0.64, 0.38);
const INK: Color = Color::srgb(0.09, 0.15, 0.16);
const RED: Color = Color::srgb(0.75, 0.24, 0.20);

/// Minimap pixel colours.
const MAP_OCEAN: [u8; 3] = [22, 52, 84];
const MAP_SEA: [u8; 3] = [33, 84, 99];
const MAP_COAST: [u8; 3] = [64, 128, 146];
const MAP_GRASS: [u8; 3] = [110, 122, 66];
const MAP_PLAINS: [u8; 3] = [166, 160, 92];
const MAP_DESERT: [u8; 3] = [208, 178, 112];
const MAP_TUNDRA: [u8; 3] = [150, 158, 140];
const MAP_FOG: [u8; 3] = [20, 33, 38];
const MAP_OWN_CITY: [u8; 3] = [191, 61, 51];

fn text(parent: &mut ChildSpawnerCommands, value: impl Into<String>, size: f32, color: Color) {
    parent.spawn((
        Node {
            flex_shrink: 0.0,
            ..default()
        },
        Text::new(value),
        TextFont {
            font_size: size,
            ..default()
        },
        TextColor(color),
    ));
}
fn button(parent: &mut ChildSpawnerCommands, title: impl Into<String>, action: Action) {
    parent
        .spawn((
            Button,
            action,
            Node {
                min_height: px(40),
                flex_shrink: 0.0,
                padding: UiRect::axes(px(12), px(8)),
                margin: UiRect::bottom(px(5)),
                border: UiRect::all(px(1)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(INK),
            BorderColor::all(GOLD),
        ))
        .with_children(|p| text(p, title, 14.0, PAPER));
}
fn panel() -> Node {
    Node {
        position_type: PositionType::Absolute,
        padding: UiRect::all(px(18)),
        border: UiRect::all(px(2)),
        flex_direction: FlexDirection::Column,
        row_gap: px(9),
        ..default()
    }
}
fn image_sprite(
    commands: &mut Commands,
    image: &Handle<Image>,
    position: Vec3,
    size: Vec2,
    color: Color,
) {
    let mut sprite = Sprite::from_image(image.clone());
    sprite.custom_size = Some(size);
    sprite.color = color;
    commands.spawn((sprite, Transform::from_translation(position), WorldVisual));
}

fn short_date(date: Date) -> String {
    format!("{} {} {}", date.day(), &date.month_name()[..3], date.year())
}
fn long_date(date: Date) -> String {
    format!(
        "{}, {} {} {}",
        date.weekday(),
        date.day(),
        date.month_name(),
        date.year()
    )
}

fn nation_rgb(game: &Game, owner: Id) -> [u8; 3] {
    game.factions
        .get(&owner)
        .map_or([128, 128, 128], |f| f.color.0)
}
fn nation_color(game: &Game, owner: Id, alpha: f32) -> Color {
    let [r, g, b] = nation_rgb(game, owner);
    Color::srgba(
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
        alpha,
    )
}
/// A sprite tint that leans toward a nation's colour without hiding the art.
pub(super) fn tinted(game: &Game, owner: Id) -> Color {
    let [r, g, b] = nation_rgb(game, owner);
    let lean = |c: u8| 1.0 - 0.5 * (1.0 - f32::from(c) / 255.0);
    Color::srgb(lean(r), lean(g), lean(b))
}
/// How bright a city looks when it is remembered rather than in sight.
const REMEMBERED_LIGHT: f32 = 0.6;
fn dimmed(color: Color, light: f32) -> Color {
    let c = color.to_srgba();
    Color::srgba(c.red * light, c.green * light, c.blue * light, c.alpha)
}
fn mix(a: [u8; 3], b: [u8; 3], toward_b: f32) -> [u8; 3] {
    let blend = |x: u8, y: u8| (f32::from(x) * (1.0 - toward_b) + f32::from(y) * toward_b) as u8;
    [blend(a[0], b[0]), blend(a[1], b[1]), blend(a[2], b[2])]
}

/// `--focus x,y` starts the camera on a given tile instead of the player's capital.
fn focus_option() -> Option<Coord> {
    let text = option("--focus")?;
    let (x, y) = text.split_once(',')?;
    Some(Coord::new(x.trim().parse().ok()?, y.trim().parse().ok()?))
}

/// Columns of every terrain sheet: the ground and water cells index `(4*S + E)*16 + 4*W + N`
/// (digits of the south, east, west and north vertices) and the river cells are one branch mask
/// each.
pub(super) const SHEET_COLUMNS: u32 = 16;
/// Tonal variants the pack's ground and water sheets hold past the 16 base rows: variant `k`
/// (1..=6) of a pure cell with digit `d` is cell `256 + 4*(k-1) + d`.
const VARIANTS: usize = 6;

/// Columns and rows of the fog sheet.
pub(super) const FOG_COLUMNS: u32 = 9;

/// Draw order, back to front. The bands are ten apart and `depth` stays under one on any map,
/// so a band never reaches into the next: terrain, farms, roads, then relief and cover, then
/// the fog over all of those, the culture borders over the fog, and cities and units over
/// everything, as in Civ3.
const Z_RELIEF: f32 = 10.0;
const Z_FOG: f32 = 15.0;
const Z_BORDER: f32 = 16.0;
const Z_CITY: f32 = 20.0;
const Z_UNIT: f32 = 30.0;
/// Gap that puts one sprite of a tile in front of another of the same tile without reaching
/// past the next tile in line (neighbours differ by at least `depth(32.0)`).
const Z_STEP: f32 = 0.0002;
/// Where a piece standing at screen height `sy` sits within its band: lower on the screen is in
/// front. Bounded by 0.7 for the largest world.
fn depth(sy: f32) -> f32 {
    -sy / 50_000.0
}

/// The cell of the fog sheet for a tile whose north, east, south and west vertices are each
/// 0 never seen, 1 remembered or 2 in sight: row `3 * west + north`, column `3 * south + east`.
const fn fog_index(north: u8, east: u8, south: u8, west: u8) -> usize {
    (3 * west as usize + north as usize) * FOG_COLUMNS as usize + 3 * south as usize + east as usize
}
/// The fog cells with every vertex unseen (solid black) and every vertex in sight (clear).
const FOG_BLACK: usize = fog_index(0, 0, 0, 0);
const FOG_CLEAR: usize = fog_index(2, 2, 2, 2);

/// What the player knows of one tile.
const UNSEEN: u8 = 0;
const REMEMBERED: u8 = 1;
const IN_SIGHT: u8 = 2;

/// The fog sheet. Its diamonds must be sampled without filtering: they tile the plane exactly
/// and a blend at their edges would show seams.
fn fog_image(assets: &AssetServer, prefix: &str, visuals: &Visuals) -> Handle<Image> {
    assets.load_with_settings(
        format!("{prefix}{}", visuals.fog),
        |settings: &mut ImageLoaderSettings| settings.sampler = ImageSampler::nearest(),
    )
}

/// Which tiles a player knows, over a rectangle of the map. Outside the rectangle, off the map
/// and in a lattice's void nothing is known.
struct Sight {
    x0: i32,
    y0: i32,
    width: i32,
    height: i32,
    states: Vec<u8>,
}
impl Sight {
    /// The rectangle `(x0, y0)..=(x1, y1)` of tiles. A spectator (player 0) sees everything.
    fn over(game: &Game, player: Id, (x0, y0, x1, y1): (i32, i32, i32, i32)) -> Sight {
        let (width, height) = ((x1 - x0 + 1).max(0), (y1 - y0 + 1).max(0));
        let mut sight = Sight {
            x0,
            y0,
            width,
            height,
            states: vec![UNSEEN; (width * height) as usize],
        };
        let explored = game.factions.get(&player).map(|f| &f.explored);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let p = Coord::new(x, y);
                if game.map.contains(p) {
                    let state = if player == 0 {
                        IN_SIGHT
                    } else if explored.is_some_and(|e| e.contains(p)) {
                        REMEMBERED
                    } else {
                        UNSEEN
                    };
                    sight.set(p, state);
                }
            }
        }
        if player == 0 {
            return sight;
        }
        // In sight: within the vision radius of one of the player's own cities or units. A
        // piece too far off to reach the rectangle changes nothing.
        let sources = game
            .cities
            .values()
            .filter(|c| c.owner == player)
            .map(|c| c.position)
            .chain(
                game.units
                    .values()
                    .filter(|u| u.owner == player)
                    .map(|u| u.position),
            );
        for source in sources {
            for y in (source.y - VISION_RADIUS).max(y0)..=(source.y + VISION_RADIUS).min(y1) {
                for x in (source.x - VISION_RADIUS).max(x0)..=(source.x + VISION_RADIUS).min(x1) {
                    let p = Coord::new(x, y);
                    if game.map.contains(p) {
                        sight.set(p, IN_SIGHT);
                    }
                }
            }
        }
        sight
    }
    fn slot(&self, p: Coord) -> Option<usize> {
        let (x, y) = (p.x - self.x0, p.y - self.y0);
        (x >= 0 && y >= 0 && x < self.width && y < self.height)
            .then(|| (y * self.width + x) as usize)
    }
    fn set(&mut self, p: Coord, state: u8) {
        if let Some(i) = self.slot(p) {
            self.states[i] = state;
        }
    }
    fn state(&self, p: Coord) -> u8 {
        self.slot(p).map_or(UNSEEN, |i| self.states[i])
    }
    /// The state of the tile corner that tiles `(x - 1, y - 1)` to `(x, y)` meet at: the best
    /// any of them has, so the fog thins out wherever any neighbour is known.
    fn vertex(&self, x: i32, y: i32) -> u8 {
        [(-1, -1), (0, -1), (-1, 0), (0, 0)]
            .into_iter()
            .map(|(dx, dy)| self.state(Coord::new(x + dx, y + dy)))
            .max()
            .unwrap_or(UNSEEN)
    }
    /// The fog sheet cell for a tile: the states of its north, east, south and west corners.
    fn fog_cell(&self, p: Coord) -> usize {
        fog_index(
            self.vertex(p.x, p.y),
            self.vertex(p.x + 1, p.y),
            self.vertex(p.x + 1, p.y + 1),
            self.vertex(p.x, p.y + 1),
        )
    }
}

/// The city sprite for a nation: the one its flavor names, or the pack's western one for a
/// nation the player has no record of.
fn city_sprite<'a>(visuals: &'a Visuals, game: &Game, owner: Id) -> &'a str {
    let flavor = game
        .factions
        .get(&owner)
        .map_or(Flavor::Western, |faction| faction.flavor);
    visuals
        .cities
        .get(flavor.key())
        .or_else(|| visuals.cities.get(Flavor::Western.key()))
        .map_or("", String::as_str)
}

/// Re-cut the terrain layouts once the images are available (and again if the pack swaps them
/// for a different size), then request a redraw so variants and indices pick up the new layout.
/// Nothing else waits on the images: units, cities and the HUD draw immediately.
pub(super) fn fit_terrain_atlas(
    mut session: ResMut<Session>,
    assets: Res<AssetServer>,
    images: Res<Assets<Image>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut art: ResMut<Art>,
) {
    if session.game.is_none() {
        return;
    }
    let ground: Handle<Image> = assets.load(format!(
        "{}{}",
        session.asset_prefix, session.pack.visuals.terrain
    ));
    let rivers: Handle<Image> = assets.load(format!(
        "{}{}",
        session.asset_prefix, session.pack.visuals.rivers
    ));
    let fog = fog_image(&assets, &session.asset_prefix, &session.pack.visuals);
    let borders: Handle<Image> = assets.load(format!(
        "{}{}",
        session.asset_prefix, session.pack.visuals.borders
    ));
    let mut changed = false;
    if let Some(sheet) = images.get(&fog).map(Image::size) {
        let cell = sheet / FOG_COLUMNS;
        if sheet != art.fog_sheet && cell.min_element() > 0 {
            if let Some(layout) = layouts.get_mut(&art.fog) {
                *layout = TextureAtlasLayout::from_grid(cell, FOG_COLUMNS, FOG_COLUMNS, None, None);
            }
            art.fog_sheet = sheet;
            changed = true;
        }
    }
    if let Some(sheet) = images.get(&borders).map(Image::size) {
        let cell = UVec2::new(sheet.x / 4, sheet.y);
        if sheet != art.border_sheet && cell.min_element() > 0 {
            if let Some(layout) = layouts.get_mut(&art.borders) {
                *layout = TextureAtlasLayout::from_grid(cell, 4, 1, None, None);
            }
            art.border_sheet = sheet;
            changed = true;
        }
    }
    if let Some(sheet) = images.get(&ground).map(Image::size) {
        let cell = UVec2::new(sheet.x / SHEET_COLUMNS, sheet.x / SHEET_COLUMNS / 2);
        if sheet != art.ground_sheet && cell.y > 0 {
            let rows = sheet.y / cell.y;
            if let Some(layout) = layouts.get_mut(&art.cells) {
                *layout = TextureAtlasLayout::from_grid(cell, SHEET_COLUMNS, rows, None, None);
            }
            art.ground_sheet = sheet;
            art.variants = if rows > SHEET_COLUMNS { VARIANTS } else { 0 };
            changed = true;
        }
    }
    if let Some(sheet) = images.get(&rivers).map(Image::size) {
        let cell = UVec2::new(sheet.x / SHEET_COLUMNS, sheet.x / SHEET_COLUMNS / 2);
        if sheet != art.river_sheet && cell.y > 0 {
            let rows = (sheet.y / cell.y).max(1);
            if let Some(layout) = layouts.get_mut(&art.rivers) {
                *layout = TextureAtlasLayout::from_grid(cell, SHEET_COLUMNS, rows, None, None);
            }
            art.river_sheet = sheet;
            art.meanders = rows as usize;
            changed = true;
        }
    }
    if changed {
        session.built = None;
        session.dirty = true;
    }
}

/// A position hash that breaks up repetition without touching any border.
fn scatter(p: Coord) -> usize {
    (p.x.wrapping_mul(73_856_093) ^ p.y.wrapping_mul(19_349_663)).unsigned_abs() as usize
}

/// Sheet index for the cell at `p`. Pure cells (all four vertices alike) pick one of the pack's
/// tonal variants; `first_digit` is the first digit that has them (the water sheet's land digit
/// is empty).
fn varied(index: usize, p: Coord, variants: usize, first_digit: usize) -> usize {
    if variants == 0 || index >= 256 || index % 85 != 0 || index / 85 < first_digit {
        return index;
    }
    match scatter(p) % (variants + 1) {
        0 => index,
        k => 256 + 4 * (k - 1) + index / 85,
    }
}

/// The sprite names that draw a tile's relief and cover. Forest on a slope or on tundra is
/// conifers; a pack that lacks a named sprite falls back to its forest or mountain.
fn overlay_names(tile: &Tile) -> (Option<&'static str>, Option<&'static str>) {
    let relief = match (tile.relief, tile.terrain) {
        (Relief::Flat, _) => None,
        (Relief::Mountains, Terrain::Desert) => Some("mountain_dry"),
        (Relief::Mountains, Terrain::Tundra) => Some("mountain_cold"),
        (Relief::Mountains, _) => Some("mountain"),
        (Relief::Hills, Terrain::Desert) => Some("hills_dry"),
        (Relief::Hills, Terrain::Tundra) => Some("hills_cold"),
        (Relief::Hills, _) => Some("hills"),
    };
    let cover = match tile.cover {
        Cover::Bare => None,
        Cover::Forest if tile.relief != Relief::Flat || tile.terrain == Terrain::Tundra => {
            Some("pine")
        }
        Cover::Forest => Some("forest"),
        Cover::Jungle => Some("jungle"),
        Cover::Marsh => Some("marsh"),
    };
    (relief, cover)
}

/// Spawn the sprites and the interface, and respawn them when the game changes or the camera
/// drifts outside what is already built. The map is far too big to draw whole, so tiles,
/// cities and units are only created for the part of the world around the camera.
pub(super) fn refresh(
    mut commands: Commands,
    mut session: ResMut<Session>,
    assets: Res<AssetServer>,
    art: Res<Art>,
    minimap: Res<Minimap>,
    world: Query<Entity, With<WorldVisual>>,
    ui: Query<Entity, With<UiRoot>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut camera: Query<(&mut Transform, &Projection), With<Camera2d>>,
    mut previous_size: Local<Vec2>,
    stage: Res<crate::stage::Stage>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let Ok((mut transform, projection)) = camera.single_mut() else {
        return;
    };
    let size = Vec2::new(window.width(), window.height());
    let narrow = size.x < 900.0;
    if *previous_size != size {
        if previous_size.x == 0.0 || (previous_size.x < 900.0) != narrow {
            session.centered = false;
        }
        session.dirty = true;
        *previous_size = size;
    }
    if !session.centered {
        let target = session
            .game
            .as_ref()
            .map(|game| focus_option().or_else(|| home(game, session.player)));
        if let Some(target) = target {
            if let Some(position) = target {
                focus(&mut transform, position, panel_shift(window, projection));
            }
            session.centered = true;
            session.dirty = true;
        }
    }
    let view = camera_rect(&transform, projection, window);
    let needed = view.grow(280.0);
    let stale =
        session.game.is_some() && session.built.is_none_or(|built| !built.contains(&needed));
    if !session.dirty && !stale {
        return;
    }
    let rebuild_ui = session.dirty;
    session.dirty = false;
    for e in &world {
        commands.entity(e).despawn();
    }
    if rebuild_ui {
        for e in &ui {
            commands.entity(e).despawn();
        }
    }
    if session.game.is_none() {
        commands
            .spawn((
                Node {
                    width: percent(100),
                    height: percent(100),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                UiRoot,
            ))
            .with_children(|p| text(p, &session.message, 24.0, PAPER));
        return;
    }
    // Build a generous margin so a pan does not rebuild every frame.
    let span = (view.max - view.min).max_element();
    let built = view.grow(280.0 + 0.3 * span);
    session.built = Some(built);
    let Some(game) = &session.game else {
        return;
    };
    let prefix = &session.asset_prefix;
    let visuals = &session.pack.visuals;
    let ground: Handle<Image> = assets.load(format!("{prefix}{}", visuals.terrain));
    let water: Handle<Image> = assets.load(format!("{prefix}{}", visuals.water));
    let rivers: Handle<Image> = assets.load(format!("{prefix}{}", visuals.rivers));
    let fog_sheet = fog_image(&assets, prefix, visuals);
    let border_sheet: Handle<Image> = assets.load(format!("{prefix}{}", visuals.borders));
    let mut city_images: HashMap<&str, Handle<Image>> = HashMap::new();
    // Named overlay sprites, each falling back to the pack's forest or mountain.
    let mut overlays: HashMap<&str, Handle<Image>> = HashMap::new();
    let (x0, y0, x1, y1) = built.tile_bounds(game);
    // What the player knows, with a margin for the corners of the outermost tiles.
    let sight = Sight::over(game, session.player, (x0 - 2, y0 - 2, x1 + 2, y1 + 2));
    let is_known = |p: Coord| sight.state(p) > UNSEEN;
    let upright = game.map.lattice.is_some();

    // Terrain cells are dual-grid: the cell at (x, y) blends the corner shared by four tiles,
    // so the ring just outside the map is drawn too. Each cell is up to three sprites: the
    // ground, the water over it, and the rivers over both. A cell shows through the fog as far
    // as the fog of any tile it covers does, which is as far as a tile away from the known
    // world, so a cell with nothing known within that reach has nothing to show.
    for y in (y0 - 1).max(-1)..=y1.min(game.map.height - 1) {
        for x in (x0 - 1).max(-1)..=x1.min(game.map.width - 1) {
            let p = Coord::new(x, y);
            let (wx, wy) = p.screen();
            if !built.has(Vec2::new(wx, wy)) {
                continue;
            }
            if !(-1..=2).any(|dy| (-1..=2).any(|dx| is_known(p.offset(dx, dy)))) {
                continue;
            }
            // Nor is there any past the edge of an upright world: a cell that holds no tile
            // of the map would only spill out over the void.
            if upright
                && ![(0, 0), (1, 0), (0, 1), (1, 1)]
                    .iter()
                    .any(|&(dx, dy)| game.map.contains(p.offset(dx, dy)))
            {
                continue;
            }
            let layers = [
                (
                    game.map
                        .ground_cell(p)
                        .map(|i| varied(i, p, art.variants, 0)),
                    &ground,
                    &art.cells,
                    0.0,
                ),
                (
                    game.map
                        .water_cell(p)
                        .map(|i| varied(i, p, art.variants, 1)),
                    &water,
                    &art.cells,
                    0.05,
                ),
                (
                    Some(game.map.river_cell(p))
                        .filter(|&mask| mask != 0)
                        .map(|mask| scatter(p) % art.meanders.max(1) * 16 + mask),
                    &rivers,
                    &art.rivers,
                    0.1,
                ),
            ];
            for (index, image, layout, z) in layers {
                let Some(index) = index else {
                    continue;
                };
                let mut sprite = Sprite::from_atlas_image(
                    image.clone(),
                    TextureAtlas {
                        layout: layout.clone(),
                        index,
                    },
                );
                sprite.custom_size = Some(Vec2::new(128.0, 64.0));
                commands.spawn((sprite, Transform::from_xyz(wx, wy - 32.0, z), WorldVisual));
            }
        }
    }
    // Relief and cover stand on their tiles; forest on a hill is drawn smaller in front of it.
    for y in y0..=y1 {
        for x in x0..=x1 {
            let Some(tile) = game.map.get(Coord::new(x, y)) else {
                continue;
            };
            let (relief, cover) = overlay_names(tile);
            if (relief.is_none() && cover.is_none()) || !is_known(tile.position) {
                continue;
            }
            let (sx, sy) = tile.position.screen();
            if !built.has(Vec2::new(sx, sy)) {
                continue;
            }
            let z = Z_RELIEF + depth(sy);
            for (name, is_relief) in [(relief, true), (cover, false)] {
                let Some(name) = name else {
                    continue;
                };
                let handle = overlays.entry(name).or_insert_with(|| {
                    let path = match name {
                        "mountain" => &visuals.mountain,
                        "forest" => &visuals.forest,
                        other => visuals.overlays.get(other).unwrap_or(
                            if other.starts_with("mountain") || other.starts_with("hills") {
                                &visuals.mountain
                            } else {
                                &visuals.forest
                            },
                        ),
                    };
                    assets.load(format!("{prefix}{path}"))
                });
                let (size, lift, z) = if is_relief || relief.is_none() {
                    (Vec2::new(128.0, 112.0), 24.0, z)
                } else {
                    (Vec2::new(80.0, 70.0), 8.0, z + Z_STEP)
                };
                image_sprite(
                    &mut commands,
                    handle,
                    Vec3::new(sx, sy + lift, z),
                    size,
                    Color::WHITE,
                );
            }
        }
    }
    // Farms, then roads and rails on top of those, then mines.
    let mut city_tiles = HashSet::new();
    for city in game.cities.values() {
        city_tiles.insert(city.position);
    }
    let farm: Handle<Image> = assets.load(format!("{prefix}{}", session.pack.visuals.farm));
    let mine: Handle<Image> = assets.load(format!("{prefix}{}", session.pack.visuals.mine));
    for y in y0..=y1 {
        for x in x0..=x1 {
            let Some(tile) = game.map.get(Coord::new(x, y)) else {
                continue;
            };
            let (sx, sy) = tile.position.screen();
            if !is_known(tile.position) || !built.has(Vec2::new(sx, sy)) {
                continue;
            }
            if tile.improvements & Tile::FARM != 0 {
                image_sprite(
                    &mut commands,
                    &farm,
                    Vec3::new(sx, sy, 2.0),
                    Vec2::new(128.0, 64.0),
                    Color::WHITE,
                );
            }
            if tile.improvements & Tile::MINE != 0 {
                image_sprite(
                    &mut commands,
                    &mine,
                    Vec3::new(sx, sy + 4.0, Z_RELIEF + depth(sy) + 2.0 * Z_STEP),
                    Vec2::new(128.0, 64.0),
                    Color::WHITE,
                );
            }
        }
    }
    draw_roads(&mut commands, game, &city_tiles, built, (x0, y0, x1, y1));

    // The fog of war lies over the terrain, the relief and the improvements, and under the
    // borders, the cities and the units: Civ3 does not hide a city or an army beneath it.
    // Its tiles are blended across their corners, so the edge of the known world and of what
    // is in sight this turn is a soft gradient. Past the edge of an upright world the cells
    // the terrain spills into are covered solid black.
    for y in (y0 - 1)..=(y1 + 1) {
        for x in (x0 - 1)..=(x1 + 1) {
            let p = Coord::new(x, y);
            let (sx, sy) = p.screen();
            if !built.has(Vec2::new(sx, sy)) {
                continue;
            }
            let index = if game.map.contains(p) {
                sight.fog_cell(p)
            } else if upright
                && Coord::NEIGHBORS
                    .iter()
                    .any(|&(dx, dy)| game.map.contains(p.offset(dx, dy)))
            {
                FOG_BLACK
            } else {
                continue;
            };
            if index == FOG_CLEAR {
                continue;
            }
            let mut sprite = Sprite::from_atlas_image(
                fog_sheet.clone(),
                TextureAtlas {
                    layout: art.fog.clone(),
                    index,
                },
            );
            sprite.custom_size = Some(Vec2::new(128.0, 64.0));
            commands.spawn((sprite, Transform::from_xyz(sx, sy, Z_FOG), WorldVisual));
        }
    }
    // Culture borders: a dashed line in the owner's colour just inside the edge of each owned
    // tile that faces another nation's tile or nobody's, never an overlay on the whole
    // territory. Edges that face the unknown or the edge of the world get none.
    let mut inks: HashMap<Id, Color> = HashMap::new();
    for y in y0..=y1 {
        for x in x0..=x1 {
            let Some(tile) = game.map.get(Coord::new(x, y)) else {
                continue;
            };
            if tile.owner == 0 || !is_known(tile.position) {
                continue;
            }
            let (sx, sy) = tile.position.screen();
            if !built.has(Vec2::new(sx, sy)) {
                continue;
            }
            let ink = *inks
                .entry(tile.owner)
                .or_insert_with(|| nation_color(game, tile.owner, 1.0));
            for (index, &(dx, dy)) in EDGE_NEIGHBORS.iter().enumerate() {
                let beyond = Coord::new(x + dx, y + dy);
                let Some(other) = game.map.get(beyond) else {
                    continue;
                };
                if other.owner == tile.owner || !is_known(beyond) {
                    continue;
                }
                let mut dashes = Sprite::from_atlas_image(
                    border_sheet.clone(),
                    TextureAtlas {
                        layout: art.borders.clone(),
                        index,
                    },
                );
                dashes.custom_size = Some(Vec2::new(128.0, 64.0));
                dashes.color = ink;
                commands.spawn((
                    dashes,
                    Transform::from_xyz(sx, sy, Z_BORDER + depth(sy)),
                    WorldVisual,
                ));
            }
        }
    }

    for city in game.cities.values() {
        let (x, y) = city.position.screen();
        if !built.has(Vec2::new(x, y)) {
            continue;
        }
        let z = Z_CITY + depth(y);
        let path = city_sprite(visuals, game, city.owner);
        let sprite = city_images
            .entry(path)
            .or_insert_with(|| assets.load(format!("{prefix}{path}")));
        let tint = if city.owner == session.player {
            Color::WHITE
        } else {
            tinted(game, city.owner)
        };
        // A city seen once and since out of sight is shown as it was, dimmed.
        let tint = if sight.state(city.position) == IN_SIGHT {
            tint
        } else {
            dimmed(tint, REMEMBERED_LIGHT)
        };
        image_sprite(
            &mut commands,
            sprite,
            Vec3::new(x, y + 24.0, z),
            Vec2::new(128.0, 112.0),
            tint,
        );
        commands.spawn((
            Sprite::from_color(INK, Vec2::new(134.0, 24.0)),
            Transform::from_xyz(x, y - 24.0, z + 0.1),
            WorldVisual,
        ));
        commands.spawn((
            Text2d::new(format!("{}   {}", city.population, city.name)),
            TextFont {
                font_size: 16.0,
                ..default()
            },
            TextColor(PAPER),
            Transform::from_xyz(x, y - 24.0, z + 0.2),
            WorldVisual,
        ));
    }
    // One piece stands for a stack, as in Civ3: the selected unit if it is there, otherwise
    // the best defender, with a count when others stand beneath it.
    let mut stacks: BTreeMap<(i32, i32), Vec<Id>> = BTreeMap::new();
    for unit in game.units.values() {
        let (x, y) = unit.position.screen();
        // A fight is being played out on this square: its units are on the stage instead.
        if built.has(Vec2::new(x, y)) && !stage.hides(unit.position) {
            stacks
                .entry((unit.position.x, unit.position.y))
                .or_default()
                .push(unit.id);
        }
    }
    let mut unit_images: HashMap<String, Handle<Image>> = HashMap::new();
    let selected = match session.selection {
        Selection::Unit(id) => Some(id),
        _ => None,
    };
    for ids in stacks.values() {
        let top = ids
            .iter()
            .copied()
            .find(|id| Some(*id) == selected)
            .unwrap_or_else(|| {
                *ids.iter()
                    .max_by_key(|id| {
                        let unit = &game.units[id];
                        let def = session.rules.def(unit);
                        (
                            def.defense * unit.hp(def),
                            def.attack + def.bombard,
                            std::cmp::Reverse(**id),
                        )
                    })
                    .expect("a stack is never empty")
            });
        let unit = &game.units[&top];
        let Some(def) = session.rules.units.get(&unit.kind) else {
            continue;
        };
        let (mut x, mut y) = unit.position.screen();
        if city_tiles.contains(&unit.position) {
            x += 62.0;
            y += 24.0;
        }
        let z = Z_UNIT + depth(y);
        let sprite = unit_images
            .entry(def.sprite.clone())
            .or_insert_with(|| assets.load(format!("{prefix}{}", def.sprite)));
        let spent = unit.owner == session.player && unit.moves_left(def) == 0;
        image_sprite(
            &mut commands,
            sprite,
            Vec3::new(x, y + 12.0, z),
            if def.is_naval() {
                Vec2::new(112.0, 84.0)
            } else {
                Vec2::splat(70.0)
            },
            if unit.owner != session.player {
                tinted(game, unit.owner)
            } else if spent {
                Color::srgb(0.66, 0.66, 0.70)
            } else {
                Color::WHITE
            },
        );
        draw_unit_badges(&mut commands, unit, def, ids.len(), Vec3::new(x, y, z));
    }
    // Unhurried map typography echoes printed campaign charts.
    if game.scenario == "dawn-straits" {
        // Placed on the chart, where the hand-drawn map's squares are.
        let chart = Chart::of(&game.map);
        for (label, pos) in [
            ("THE JADE STRAITS", chart.tile(14, 2)),
            ("SOUTHERN SEA", chart.tile(4, 15)),
        ] {
            let (x, y) = pos.screen();
            commands.spawn((
                Text2d::new(label),
                TextFont {
                    font_size: 24.0,
                    ..default()
                },
                TextColor(Color::srgba(0.67, 0.81, 0.81, 0.55)),
                Transform::from_xyz(x, y, 2.0),
                WorldVisual,
            ));
        }
    }
    if rebuild_ui {
        build_ui(&mut commands, &session, size, &minimap);
    }
}

/// A straight strip of colour between two world points.
fn strip(commands: &mut Commands, from: Vec2, to: Vec2, width: f32, color: Color, z: f32) {
    let along = to - from;
    commands.spawn((
        Sprite::from_color(color, Vec2::new(along.length(), width)),
        Transform::from_translation(((from + to) * 0.5).extend(z))
            .with_rotation(Quat::from_rotation_z(along.y.atan2(along.x))),
        WorldVisual,
    ));
}

/// Roads and railroads join the centres of neighbouring squares. A city counts as both, as it
/// does for movement, so a road that reaches a city is drawn into it. Each pair of squares is
/// visited once, from the square whose direction is in the first half of the eight.
fn draw_roads(
    commands: &mut Commands,
    game: &Game,
    cities: &HashSet<Coord>,
    built: WorldRect,
    (x0, y0, x1, y1): (i32, i32, i32, i32),
) {
    const ROAD_EDGE: Color = Color::srgb(0.30, 0.22, 0.12);
    const ROAD: Color = Color::srgb(0.72, 0.58, 0.36);
    const RAIL_BED: Color = Color::srgb(0.18, 0.18, 0.20);
    const RAIL: Color = Color::srgb(0.78, 0.78, 0.80);
    const TIE: Color = Color::srgb(0.38, 0.27, 0.16);
    // 0 none, 1 road, 2 rail
    let kind = |p: Coord| -> u8 {
        if cities.contains(&p) {
            return 2;
        }
        match game.map.get(p) {
            Some(t) if t.has_rail() => 2,
            Some(t) if t.has_road() => 1,
            _ => 0,
        }
    };
    let mut segments: Vec<(Vec2, Vec2, u8)> = Vec::new();
    for y in y0..=y1 {
        for x in x0..=x1 {
            let here = Coord::new(x, y);
            let own = kind(here);
            let (sx, sy) = here.screen();
            if own == 0 || !built.has(Vec2::new(sx, sy)) {
                continue;
            }
            let centre = Vec2::new(sx, sy);
            let mut joined = cities.contains(&here);
            for &(dx, dy) in &Coord::NEIGHBORS {
                let there = here.offset(dx, dy);
                joined |= kind(there) > 0;
            }
            if !joined {
                // A lone stretch of road shows as a short track so it can be seen.
                segments.push((
                    centre - Vec2::new(14.0, 0.0),
                    centre + Vec2::new(14.0, 0.0),
                    own,
                ));
            }
            for &(dx, dy) in &Coord::NEIGHBORS[..4] {
                let there = here.offset(dx, dy);
                let shared = own.min(kind(there));
                if shared > 0 {
                    let (tx, ty) = there.screen();
                    segments.push((centre, Vec2::new(tx, ty), shared));
                }
            }
        }
    }
    // Every road's dark edge goes down before any light surface, so junctions read cleanly.
    for &(a, b, kind) in &segments {
        let (color, width) = if kind == 2 {
            (RAIL_BED, 9.0)
        } else {
            (ROAD_EDGE, 8.0)
        };
        strip(commands, a, b, width, color, 3.0);
    }
    for &(a, b, kind) in &segments {
        if kind == 2 {
            let along = b - a;
            let ties = (along.length() / 15.0).floor().max(1.0) as i32;
            let across = Vec2::new(-along.y, along.x).normalize_or_zero() * 5.5;
            for i in 0..=ties {
                let at = a + along * (i as f32 / ties as f32);
                strip(commands, at - across, at + across, 2.5, TIE, 3.1);
            }
            strip(commands, a, b, 2.0, RAIL, 3.2);
        } else {
            strip(commands, a, b, 4.5, ROAD, 3.1);
        }
    }
}

/// A unit's hit-point bar, its standing order, its experience, and the size of its stack.
fn draw_unit_badges(commands: &mut Commands, unit: &Unit, def: &UnitDef, stack: usize, at: Vec3) {
    let (x, y, z) = (at.x, at.y, at.z);
    let (hp, max) = (unit.hp(def).max(0), unit.max_hp(def));
    let fraction = hp as f32 / max as f32;
    let width = 46.0;
    commands.spawn((
        Sprite::from_color(INK, Vec2::new(width + 4.0, 9.0)),
        Transform::from_xyz(x, y - 22.0, z + 0.1),
        WorldVisual,
    ));
    commands.spawn((
        Sprite::from_color(
            if fraction > 0.66 {
                Color::srgb(0.30, 0.72, 0.32)
            } else if fraction > 0.33 {
                Color::srgb(0.88, 0.70, 0.22)
            } else {
                RED
            },
            Vec2::new(width * fraction, 5.0),
        ),
        // anchored to the left edge of the bar
        Transform::from_xyz(x - width * (1.0 - fraction) * 0.5, y - 22.0, z + 0.2),
        WorldVisual,
    ));
    // Experience stars, then the order glyph: F fortified, f digging in, W working, > marching.
    let order = match unit.order {
        Order::Fortified => "F",
        Order::Fortifying => "f",
        Order::Work(_) => "W",
        Order::None if unit.goto.is_some() => ">",
        Order::None => "",
    };
    let stars = "*".repeat(usize::from(unit.level.saturating_sub(1)));
    let mark = format!("{order}{stars}");
    if !mark.is_empty() {
        badge(
            commands,
            &mark,
            Vec3::new(x - 28.0, y + 38.0, z),
            14.0 + 7.0 * mark.len() as f32,
        );
    }
    if stack > 1 {
        badge(
            commands,
            &format!("x{stack}"),
            Vec3::new(x + 28.0, y + 38.0, z),
            30.0,
        );
    }
}

fn badge(commands: &mut Commands, label: &str, at: Vec3, width: f32) {
    commands.spawn((
        Sprite::from_color(INK, Vec2::new(width, 16.0)),
        Transform::from_translation(at + Vec3::Z * 0.3),
        WorldVisual,
    ));
    commands.spawn((
        Text2d::new(label),
        TextFont {
            font_size: 12.0,
            ..default()
        },
        TextColor(GOLD),
        Transform::from_translation(at + Vec3::Z * 0.4),
        WorldVisual,
    ));
}

/// Movement counted in thirds, shown as whole moves and fractions: 7 is "2 1/3".
fn moves_text(thirds: u32) -> String {
    match (thirds / 3, thirds % 3) {
        (whole, 0) => whole.to_string(),
        (0, part) => format!("{part}/3"),
        (whole, part) => format!("{whole} {part}/3"),
    }
}

fn job_key(job: Job) -> &'static str {
    match job {
        Job::Road | Job::Rail => "R",
        Job::Mine => "M",
        Job::Farm => "I",
    }
}

/// Squares a selected unit can hit now: adjacent hostile squares for melee, anything hostile
/// within range for a bombarding unit. Each is paired with whether it would be a bombardment.
fn strike_targets(game: &Game, rules: &Rules, unit: &Unit) -> Vec<(Coord, bool)> {
    let def = rules.def(unit);
    let ranged = def.can_bombard();
    if unit.moves_left(def) == 0 || !(ranged || def.can_attack()) {
        return Vec::new();
    }
    let reach = if ranged { def.range } else { 1 };
    let mut targets = Vec::new();
    for dy in -reach..=reach {
        for dx in -reach..=reach {
            let target = unit.position.offset(dx, dy);
            let Some(tile) = game.map.get(target) else {
                continue;
            };
            if (dx, dy) == (0, 0) || game.hostile_holder(target, unit.owner).is_none() {
                continue;
            }
            // Land units fight on land and ships at sea, whether bombarding or not.
            if tile.is_land() == def.is_naval() && !ranged {
                continue;
            }
            targets.push((target, ranged));
        }
    }
    targets
}

/// Everything about a selected unit: its numbers, its orders, and what it can do from here.
fn unit_panel(p: &mut ChildSpawnerCommands, game: &Game, session: &Session, u: &Unit) {
    let def = session.rules.def(u);
    let mine = u.owner == session.player;
    let owner = game
        .factions
        .get(&u.owner)
        .map_or("Unknown", |f| f.adjective.as_str());
    text(p, def.name.clone(), 22.0, INK);
    let mut lines = vec![
        format!("{owner} | {}", u.level_name()),
        format!("HP {}/{}", u.hp(def), u.max_hp(def)),
    ];
    if def.is_defenseless() && !def.can_attack() {
        lines.push("Cannot attack or defend; captured if caught.".into());
    } else {
        lines.push(format!("Attack {}   Defense {}", def.attack, def.defense));
    }
    if def.can_bombard() {
        lines.push(format!(
            "Bombard {} | range {} | {} shots",
            def.bombard, def.range, def.rate_of_fire
        ));
    }
    if def.blitz {
        lines.push("Attacks again while moves remain.".into());
    }
    if mine {
        lines.push(format!(
            "Moves {} of {}",
            moves_text(u.moves_left(def)),
            def.moves
        ));
        if def.defense > 0 {
            lines.push(format!(
                "Defends at +{}% here",
                game.defender_percent(u, def)
            ));
        }
    }
    lines.push(match u.order {
        Order::Fortified => "Fortified".to_string(),
        Order::Fortifying => "Digging in".to_string(),
        Order::Work(job) => format!(
            "Building a {}: {}/{} work",
            job.name().to_lowercase(),
            u.work,
            game.work_required(u.position, job)
        ),
        Order::None => match u.goto {
            Some(goal) => format!("Marching to {},{}", goal.x, goal.y),
            None if u.attacked => "Has fought this turn".into(),
            None => "Awaiting orders".into(),
        },
    });
    text(p, lines.join("\n"), 14.0, INK);
    if !mine {
        return;
    }
    if def.domain == fourx_sim::Domain::Land && def.defense > 0 && u.order != Order::Fortified {
        button(p, "FORTIFY  (F)", Action::Fortify);
    }
    if !u.order.is_none() || u.goto.is_some() {
        button(p, "CANCEL ORDERS  (DEL)", Action::Cancel);
    }
    if def.settler {
        button(p, "FOUND A CITY  (B)", Action::Found);
    }
    if def.can_work() {
        let options = game.job_options(u.position);
        for (job, option) in &options {
            if let Ok(work) = option {
                button(
                    p,
                    format!(
                        "{}  ({} turns)  ({})",
                        job.name().to_uppercase(),
                        work.div_ceil(def.work),
                        job_key(*job)
                    ),
                    Action::Work(*job),
                );
            }
        }
        if options.values().all(Result::is_err) {
            let why = options
                .get(&Job::Road)
                .and_then(|o| o.as_ref().err())
                .copied()
                .unwrap_or("Nothing to build here");
            text(p, format!("Nothing to build here: {why}."), 13.0, INK);
        }
    }
    for (target, bombard) in strike_targets(game, &session.rules, u) {
        // Say who is there, then offer the blow with its odds.
        let holder = game.hostile_holder(target, u.owner);
        let defender = game
            .best_defender(target, holder.unwrap_or(0), &session.rules)
            .and_then(|id| game.units.get(&id));
        text(
            p,
            match (
                defender,
                game.city_at(target).and_then(|id| game.cities.get(&id)),
            ) {
                (Some(d), Some(city)) => format!(
                    "{},{}: {} in {}",
                    target.x,
                    target.y,
                    session.rules.def(d).name,
                    city.name
                ),
                (Some(d), None) => {
                    format!("{},{}: {}", target.x, target.y, session.rules.def(d).name)
                }
                (None, Some(city)) => {
                    format!("{},{}: {} (no defenders)", target.x, target.y, city.name)
                }
                (None, None) => format!("{},{}: foreign units", target.x, target.y),
            },
            13.0,
            INK,
        );
        let label = if bombard {
            "BOMBARD".to_string()
        } else {
            match game.estimate_attack(u.id, target, &session.rules) {
                Some(e) => format!("ATTACK  |  {:.0}% to win", e.win_chance * 100.0),
                None => "TAKE IT (undefended)".to_string(),
            }
        };
        button(p, label, Action::Strike(target));
    }
    button(p, "DISBAND", Action::Disband);
    text(
        p,
        "Right-click a square to move there, or an enemy to attack it.",
        13.0,
        INK,
    );
}

fn build_ui(commands: &mut Commands, session: &Session, size: Vec2, minimap: &Minimap) {
    let game = session.game.as_ref().unwrap();
    let narrow = size.x < 900.0;
    let me = if session.player == 0 {
        game.commander
    } else {
        session.player
    };
    let faction = &game.factions[&me];
    let date = game.date();
    let top = Node {
        position_type: PositionType::Absolute,
        top: px(0),
        width: percent(100),
        height: px(72),
        padding: UiRect::axes(px(20), px(10)),
        border: UiRect::bottom(px(2)),
        justify_content: JustifyContent::SpaceBetween,
        align_items: AlignItems::Center,
        column_gap: px(12),
        ..default()
    };
    commands
        .spawn((top, BackgroundColor(INK), BorderColor::all(GOLD), UiRoot))
        .with_children(|p| {
            p.spawn(Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                ..default()
            })
            .with_children(|p| {
                text(
                    p,
                    if narrow {
                        short_date(date).to_uppercase()
                    } else {
                        game.scenario_name.to_uppercase()
                    },
                    if narrow { 15.0 } else { 22.0 },
                    PAPER,
                );
                if !narrow {
                    text(
                        p,
                        format!(
                            "{} / {}",
                            faction.name.to_uppercase(),
                            long_date(date).to_uppercase()
                        ),
                        11.0,
                        GOLD,
                    );
                }
            });
            text(
                p,
                if narrow {
                    format!("{}g +{}", faction.gold, game.income(me))
                } else {
                    format!(
                        "TREASURY  {} (+{})     RESEARCH  {}/{}     INDUSTRY  {}",
                        faction.gold,
                        game.income(me),
                        faction.research,
                        session.rules.research_cost * (faction.technology as i32 + 1),
                        faction.industry,
                    )
                },
                if narrow { 13.0 } else { 15.0 },
                PAPER,
            );
            button(
                p,
                if session.pending {
                    "Waiting...".into()
                } else {
                    format!("{}  >", short_date(date))
                },
                Action::EndTurn,
            );
        });
    let mut sidebar = panel();
    if narrow {
        sidebar.left = px(8);
        sidebar.right = px(8);
        sidebar.bottom = px(52);
        sidebar.height = px(inspector_height(size.y));
        sidebar.padding = UiRect::all(px(10));
        sidebar.overflow = Overflow::scroll_y();
    } else {
        sidebar.left = px(16);
        sidebar.top = px(88);
        sidebar.width = px(266);
        sidebar.bottom = px(chart_side(size.y) + 96.0);
        sidebar.overflow = Overflow::scroll_y();
    }
    commands
        .spawn((
            sidebar,
            BackgroundColor(PAPER),
            BorderColor::all(GOLD),
            UiRoot,
            Inspector,
            ScrollPosition::default(),
        ))
        .with_children(|p| {
            text(p, "CAMPAIGN COMMAND", 13.0, INK);
            match session.selection {
                Selection::Unit(id) if game.units.contains_key(&id) => {
                    unit_panel(p, game, session, &game.units[&id]);
                }
                Selection::City(id) if game.cities.contains_key(&id) => {
                    let c = &game.cities[&id];
                    text(p, &c.name, 24.0, INK);
                    text(
                        p,
                        format!(
                            "Population {} | Industry {}\nFood {} - {} eaten = {:+}\nGranary {} / {}\nShields {} ({} land + {} industry)\nGold +{}\nBorder level {} | {} squares of territory\nProduction: {} ({})",
                            c.population,
                            c.industry,
                            c.harvest.food,
                            c.food_eaten(),
                            c.food_surplus(),
                            c.granary,
                            c.granary_size(),
                            c.shields(),
                            c.harvest.shields,
                            c.industry,
                            c.harvest.gold,
                            c.border,
                            game.territory_of(c.id).len(),
                            c.production.as_deref().unwrap_or("idle"),
                            c.progress
                        ),
                        14.0,
                        INK,
                    );
                    if c.owner == session.player {
                        button(p, "DEVELOP INDUSTRY (40g)", Action::Develop);
                        for (i, u) in session.rules.units.values().enumerate() {
                            button(
                                p,
                                format!("{}: {} / {}", i + 1, u.name, u.cost),
                                Action::Produce(u.id.clone()),
                            );
                        }
                    } else if let Some(owner) = game.factions.get(&c.owner) {
                        text(p, format!("Held by {}", owner.name), 13.0, INK);
                    }
                    text(
                        p,
                        "Its border never changes. Whoever holds the city holds the land.",
                        13.0,
                        INK,
                    );
                }
                Selection::Tile(pos) => {
                    let t = game.map.get(pos).unwrap();
                    text(p, t.terrain.name(), 22.0, INK);
                    let region = game.region_of(pos).map(|r| r.name.clone());
                    let owner = game
                        .factions
                        .get(&t.owner)
                        .filter(|_| t.owner != 0)
                        .map(|f| f.name.clone());
                    let border = game.cities.get(&t.claim).map(|c| c.name.clone());
                    let mut features = Vec::new();
                    match t.relief {
                        Relief::Hills => features.push("Hills"),
                        Relief::Mountains => features.push("Mountains"),
                        Relief::Flat => {}
                    }
                    match t.cover {
                        Cover::Forest => features.push("Forest"),
                        Cover::Jungle => features.push("Jungle"),
                        Cover::Marsh => features.push("Marsh"),
                        Cover::Bare => {}
                    }
                    if game.map.river_mask(pos) != 0 {
                        features.push("River");
                    }
                    for (flag, name) in [
                        (Tile::ROAD, "Road"),
                        (Tile::RAIL, "Railroad"),
                        (Tile::MINE, "Mine"),
                        (Tile::FARM, "Farm"),
                    ] {
                        if t.improvements & flag != 0 {
                            features.push(name);
                        }
                    }
                    text(
                        p,
                        format!(
                            "{}\n{}\n{}\nChart {}, {}\n{}\nMovement cost {} | Defense +{}%",
                            region.unwrap_or_else(|| "Open country".into()),
                            owner.map_or_else(|| "Unclaimed".into(), |o| format!("Claimed by {o}")),
                            border.map_or_else(String::new, |c| format!("Border of {c}")),
                            pos.x,
                            pos.y,
                            features.join(", "),
                            if t.is_land() { t.move_cost() } else { 1 },
                            t.defense_percent(),
                        ),
                        14.0,
                        INK,
                    );
                    let gathered = fourx_sim::economy::tile_yield(
                        t,
                        game.map.river_mask(pos) != 0,
                        game.city_at(pos).is_some(),
                    );
                    text(
                        p,
                        format!(
                            "Yields {} food | {} shields | {} gold",
                            gathered.food, gathered.shields, gathered.gold
                        ),
                        14.0,
                        INK,
                    );
                }
                _ => {
                    text(p, faction.name.clone(), 22.0, INK);
                    text(
                        p,
                        format!(
                            "{}\n{} | {:?}",
                            faction.leader, faction.government, faction.status
                        ),
                        13.0,
                        INK,
                    );
                    text(p, &game.briefing, 14.0, INK);
                }
            }
            if !narrow {
                let cities: Vec<_> = game
                    .cities
                    .values()
                    .filter(|c| c.owner == session.player)
                    .collect();
                text(p, format!("YOUR CITIES ({})", cities.len()), 12.0, INK);
                for c in cities.iter().take(12) {
                    button(
                        p,
                        format!(
                            "{}  |  Pop {}  Shields {}  Gold +{}",
                            c.name,
                            c.population,
                            c.shields(),
                            c.harvest.gold
                        ),
                        Action::SelectCity(c.id),
                    );
                }
                if cities.len() > 12 {
                    text(
                        p,
                        format!(
                            "and {} more. Press C to cycle through them.",
                            cities.len() - 12
                        ),
                        12.0,
                        INK,
                    );
                }
                let units: Vec<_> = game
                    .units
                    .values()
                    .filter(|u| u.owner == session.player)
                    .collect();
                text(p, format!("YOUR UNITS ({})", units.len()), 12.0, INK);
                for u in units.iter().take(6) {
                    let def = session.rules.def(u);
                    button(
                        p,
                        format!("{}  |  HP {}/{}", def.name, u.hp(def), u.max_hp(def)),
                        Action::SelectUnit(u.id),
                    );
                }
                if units.len() > 6 {
                    text(
                        p,
                        format!("and {} more. Press Tab to cycle through them.", units.len() - 6),
                        12.0,
                        INK,
                    );
                }
                button(p, "SAVE CAMPAIGN", Action::Save);
                button(p, "CENTER MAP", Action::Center);
                text(
                    p,
                    "WASD / arrows: pan | Wheel: zoom\nTab: next idle unit | Right-click: move or attack\nF: fortify | B: build a city\nR: road, then railroad | M: mine | I: farm\nDel: cancel orders\nC: next city | 1-6: produce\nSpace: next day | E: develop\nF5: save | H: capital | Esc: clear",
                    12.0,
                    INK,
                );
            } else {
                p.spawn(Node {
                    flex_direction: FlexDirection::Row,
                    column_gap: px(5),
                    flex_wrap: FlexWrap::Wrap,
                    ..default()
                })
                .with_children(|p| {
                    for c in game
                        .cities
                        .values()
                        .filter(|c| c.owner == session.player)
                        .take(8)
                    {
                        button(p, &c.name, Action::SelectCity(c.id));
                    }
                    for u in game
                        .units
                        .values()
                        .filter(|u| u.owner == session.player)
                        .take(4)
                    {
                        button(p, session.rules.def(u).name.clone(), Action::SelectUnit(u.id));
                    }
                    button(p, "Save", Action::Save);
                });
            }
        });
    if !narrow {
        let side = chart_side(size.y);
        // keep the map's proportions; the framed panel itself stays square
        let shape = Chart::of(&game.map);
        let longest = shape.width.max(shape.height) as f32;
        let (chart_width, chart_height) = (
            side * shape.width as f32 / longest,
            side * shape.height as f32 / longest,
        );
        let mut chart = panel();
        chart.left = px(16);
        chart.bottom = px(64);
        chart.width = px(side + 16.0);
        chart.height = px(side + 16.0);
        chart.padding = UiRect::all(px(6));
        chart.align_items = AlignItems::Center;
        chart.justify_content = JustifyContent::Center;
        commands
            .spawn((chart, BackgroundColor(INK), BorderColor::all(GOLD), UiRoot))
            .with_children(|p| {
                p.spawn((
                    Button,
                    MinimapSurface,
                    RelativeCursorPosition::default(),
                    ImageNode::new(minimap.image.clone()),
                    Node {
                        width: px(chart_width),
                        height: px(chart_height),
                        ..default()
                    },
                ))
                .with_children(|surface| {
                    surface.spawn((
                        MinimapView,
                        Node {
                            position_type: PositionType::Absolute,
                            border: UiRect::all(px(1)),
                            ..default()
                        },
                        BorderColor::all(Color::WHITE),
                    ));
                });
            });
        let mut report = panel();
        report.right = px(16);
        report.bottom = px(66);
        report.width = px(345);
        report.padding = UiRect::all(px(16));
        commands
            .spawn((
                report,
                BackgroundColor(Color::srgba(0.06, 0.12, 0.14, 0.95)),
                BorderColor::all(GOLD),
                UiRoot,
            ))
            .with_children(|p| {
                text(p, "DISPATCHES FROM THE FRONT", 13.0, GOLD);
                for line in game.log.iter().rev().take(4) {
                    text(p, line, 13.0, PAPER);
                }
            });
    }
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: px(0),
                width: percent(100),
                height: px(48),
                padding: UiRect::axes(px(20), px(8)),
                align_items: AlignItems::Center,
                border: UiRect::top(px(1)),
                ..default()
            },
            BackgroundColor(INK),
            BorderColor::all(GOLD),
            UiRoot,
        ))
        .with_children(|p| text(p, &session.message, if narrow { 12.0 } else { 14.0 }, PAPER));
    if let Some(winner) = game.winner {
        commands
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: percent(100),
                    height: percent(100),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                BackgroundColor(Color::srgba(0.02, 0.08, 0.10, 0.75)),
                UiRoot,
            ))
            .with_children(|p| {
                p.spawn((
                    Node {
                        width: percent(70),
                        max_width: px(600),
                        padding: UiRect::all(px(40)),
                        border: UiRect::all(px(3)),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(16),
                        ..default()
                    },
                    BackgroundColor(PAPER),
                    BorderColor::all(GOLD),
                ))
                .with_children(|p| {
                    text(
                        p,
                        if winner == session.player {
                            "THE CAMPAIGN IS WON"
                        } else {
                            "THE CAMPAIGN IS LOST"
                        },
                        30.0,
                        INK,
                    );
                    text(
                        p,
                        format!(
                            "{} won the campaign on {}.",
                            game.factions[&winner].name,
                            long_date(date)
                        ),
                        20.0,
                        INK,
                    );
                    button(p, "SAVE THIS CAMPAIGN", Action::Save);
                });
            });
    }
}

/// Side length of the square minimap for a window height.
fn chart_side(window_height: f32) -> f32 {
    (window_height * 0.30).clamp(120.0, 266.0)
}

/// Redraw the minimap image whenever the game state changes: one pixel per tile, tinted by
/// the owning nation, with the player's own cities picked out.
pub(super) fn update_minimap(
    session: Res<Session>,
    mut minimap: ResMut<Minimap>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(game) = &session.game else {
        return;
    };
    if minimap.drawn == game.revision {
        return;
    }
    minimap.drawn = game.revision;
    let chart = Chart::of(&game.map);
    let (width, height) = (chart.width, chart.height);
    let explored = game.factions.get(&session.player).map(|f| &f.explored);
    let ink: BTreeMap<Id, [u8; 3]> = game
        .factions
        .keys()
        .map(|&id| (id, nation_rgb(game, id)))
        .collect();
    let mut data = vec![255u8; (width * height * 4) as usize];
    let mut put = |p: Coord, rgb: [u8; 3]| {
        let (x, y) = chart.pixel(p);
        if x >= 0 && y >= 0 && (x as u32) < width && (y as u32) < height {
            let i = ((y as u32 * width + x as u32) * 4) as usize;
            data[i..i + 3].copy_from_slice(&rgb);
        }
    };
    for tile in game
        .map
        .tiles
        .iter()
        .filter(|tile| game.map.contains(tile.position))
    {
        let known = session.player == 0 || explored.is_some_and(|e| e.contains(tile.position));
        let rgb = if !known {
            MAP_FOG
        } else {
            let base = match tile.terrain {
                Terrain::Ocean => MAP_OCEAN,
                Terrain::Sea => MAP_SEA,
                Terrain::Coast => MAP_COAST,
                Terrain::Grass => MAP_GRASS,
                Terrain::Plains => MAP_PLAINS,
                Terrain::Desert => MAP_DESERT,
                Terrain::Tundra => MAP_TUNDRA,
            };
            let base = match tile.cover {
                Cover::Forest => mix(base, [32, 76, 36], 0.45),
                Cover::Jungle => mix(base, [20, 92, 50], 0.55),
                Cover::Marsh => mix(base, [70, 110, 104], 0.45),
                Cover::Bare => base,
            };
            let base = match tile.relief {
                Relief::Hills => mix(base, [110, 84, 52], 0.3),
                Relief::Mountains => mix(base, [226, 222, 214], 0.55),
                Relief::Flat => base,
            };
            match ink.get(&tile.owner) {
                Some(owner) if tile.is_land() => mix(base, *owner, 0.55),
                _ => base,
            }
        };
        put(tile.position, rgb);
    }
    for city in game.cities.values() {
        let (x, y) = chart.pixel(city.position);
        if city.owner == session.player {
            // A 3 x 3 blot of pixels, which on a lattice chart is the tile's own neighbours.
            for dy in -1..=1 {
                for dx in -1..=1 {
                    put(chart.tile(x + dx, y + dy), MAP_OWN_CITY);
                }
            }
        } else {
            put(city.position, [240, 236, 220]);
        }
    }
    let Some(image) = images.get_mut(&minimap.image) else {
        return;
    };
    let mut fresh = Image::new(
        bevy::render::render_resource::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        bevy::render::render_resource::TextureDimension::D2,
        data,
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::default(),
    );
    // Small maps are magnified, so keep their pixels crisp.
    if width < 128 {
        fresh.sampler = bevy::image::ImageSampler::nearest();
    }
    *image = fresh;
}

/// Keep the camera's footprint outlined on the minimap.
pub(super) fn minimap_viewport(
    session: Res<Session>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Transform, &Projection), With<Camera2d>>,
    mut marker: Query<&mut Node, With<MinimapView>>,
) {
    let Some(game) = &session.game else {
        return;
    };
    let (Ok(window), Ok((transform, projection))) = (windows.single(), camera.single()) else {
        return;
    };
    let (lo, hi) = Chart::of(&game.map).view(camera_rect(transform, projection, window));
    let ((x0, y0), (x1, y1)) = ((lo.x, lo.y), (hi.x, hi.y));
    for mut node in &mut marker {
        node.left = percent(x0 * 100.0);
        node.top = percent(y0 * 100.0);
        node.width = percent(((x1 - x0) * 100.0).max(2.0));
        node.height = percent(((y1 - y0) * 100.0).max(2.0));
    }
}

/// The four edges of a square, in the order the four edge-sharing neighbours lie: up-right,
/// down-right, down-left, up-left on screen.
const EDGE_NEIGHBORS: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];

fn diamond(center: Vec2) -> [Vec2; 5] {
    [
        center + Vec2::new(0.0, 32.0),
        center + Vec2::new(64.0, 0.0),
        center + Vec2::new(0.0, -32.0),
        center + Vec2::new(-64.0, 0.0),
        center + Vec2::new(0.0, 32.0),
    ]
}

/// The selection's marks: the chosen city's own region outlined, the chosen square, and what a
/// selected unit is heading for and could strike. A nation's borders are part of the map (see
/// `refresh`); only the tiles near the camera are visited here.
pub(super) fn draw_overlays(
    session: Res<Session>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Transform, &Projection), With<Camera2d>>,
    mut gizmos: Gizmos,
) {
    let Some(game) = &session.game else {
        return;
    };
    let (Ok(window), Ok((transform, projection))) = (windows.single(), camera.single()) else {
        return;
    };
    let view = camera_rect(transform, projection, window).grow(96.0);
    let (x0, y0, x1, y1) = view.tile_bounds(game);
    if let Selection::City(city) = session.selection {
        for y in y0..=y1 {
            for x in x0..=x1 {
                let Some(t) = game.map.get(Coord::new(x, y)).filter(|t| t.claim == city) else {
                    continue;
                };
                let (sx, sy) = t.position.screen();
                if !view.has(Vec2::new(sx, sy)) {
                    continue;
                }
                let centre = Vec2::new(sx, sy);
                let points = diamond(centre);
                for (i, (dx, dy)) in EDGE_NEIGHBORS.iter().enumerate() {
                    let beyond = game.map.get(Coord::new(x + dx, y + dy));
                    if beyond.is_none_or(|other| other.claim != city) {
                        gizmos.line_2d(
                            points[i].lerp(centre, 0.14),
                            points[i + 1].lerp(centre, 0.14),
                            Color::srgb(1.0, 0.92, 0.62),
                        );
                    }
                }
            }
        }
    }
    let gold = Color::srgb(1.0, 0.84, 0.42);
    let pos = match session.selection {
        Selection::Unit(id) => game.units.get(&id).map(|u| u.position),
        Selection::City(id) => game.cities.get(&id).map(|c| c.position),
        Selection::Tile(pos) => Some(pos),
        Selection::None => None,
    };
    if let Some(pos) = pos {
        let (x, y) = pos.screen();
        gizmos.linestrip_2d(diamond(Vec2::new(x, y)), gold);
    }
    // A selected unit shows where it is heading and what it could strike from here.
    if let Selection::Unit(id) = session.selection
        && let Some(unit) = game.units.get(&id).filter(|u| u.owner == session.player)
    {
        if let Some(goal) = unit.goto {
            let (from, to) = (unit.position.screen(), goal.screen());
            let (from, to) = (Vec2::new(from.0, from.1), Vec2::new(to.0, to.1));
            gizmos.line_2d(from, to, gold.with_alpha(0.6));
            gizmos.circle_2d(to, 14.0, gold);
        }
        for (target, bombard) in strike_targets(game, &session.rules, unit) {
            let (x, y) = target.screen();
            let colour = if bombard { gold } else { RED };
            gizmos.linestrip_2d(diamond(Vec2::new(x, y)), colour);
            gizmos.circle_2d(Vec2::new(x, y), 16.0, colour);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(terrain: Terrain, relief: Relief, cover: Cover) -> Tile {
        let mut tile = Tile::new(Coord::new(0, 0), terrain);
        tile.relief = relief;
        tile.cover = cover;
        tile
    }

    #[test]
    fn relief_and_cover_pick_their_sprites_from_the_ground_beneath() {
        use {Cover::*, Relief::*, Terrain::*};
        assert_eq!(overlay_names(&tile(Grass, Flat, Bare)), (None, None));
        assert_eq!(
            overlay_names(&tile(Grass, Flat, Forest)),
            (None, Some("forest"))
        );
        assert_eq!(
            overlay_names(&tile(Tundra, Flat, Forest)),
            (None, Some("pine"))
        );
        assert_eq!(
            overlay_names(&tile(Plains, Flat, Jungle)),
            (None, Some("jungle"))
        );
        assert_eq!(
            overlay_names(&tile(Plains, Flat, Marsh)),
            (None, Some("marsh"))
        );
        assert_eq!(
            overlay_names(&tile(Grass, Hills, Bare)),
            (Some("hills"), None)
        );
        assert_eq!(
            overlay_names(&tile(Desert, Hills, Bare)),
            (Some("hills_dry"), None)
        );
        assert_eq!(
            overlay_names(&tile(Tundra, Hills, Bare)),
            (Some("hills_cold"), None)
        );
        assert_eq!(
            overlay_names(&tile(Plains, Mountains, Bare)),
            (Some("mountain"), None)
        );
        assert_eq!(
            overlay_names(&tile(Desert, Mountains, Bare)),
            (Some("mountain_dry"), None)
        );
        assert_eq!(
            overlay_names(&tile(Tundra, Mountains, Bare)),
            (Some("mountain_cold"), None)
        );
        // Trees on a slope are conifers, drawn with the slope.
        assert_eq!(
            overlay_names(&tile(Grass, Hills, Forest)),
            (Some("hills"), Some("pine"))
        );
    }

    fn sight(states: &[&str]) -> Sight {
        // One string per row of tiles: `.` unseen, `r` remembered, `*` in sight.
        let height = states.len() as i32;
        let width = states[0].len() as i32;
        Sight {
            x0: 0,
            y0: 0,
            width,
            height,
            states: states
                .iter()
                .flat_map(|row| row.chars())
                .map(|c| match c {
                    '*' => IN_SIGHT,
                    'r' => REMEMBERED,
                    _ => UNSEEN,
                })
                .collect(),
        }
    }

    #[test]
    fn the_fog_sheet_is_indexed_by_vertex_states() {
        // Row 3 * west + north, column 3 * south + east, nine to a row.
        assert_eq!(FOG_BLACK, 0);
        assert_eq!(FOG_CLEAR, 80);
        assert_eq!(fog_index(0, 1, 0, 0), 1);
        assert_eq!(fog_index(0, 0, 1, 0), 3);
        assert_eq!(fog_index(1, 0, 0, 0), 9);
        assert_eq!(fog_index(0, 0, 0, 1), 27);
        let mut seen = HashSet::new();
        for n in 0..3 {
            for e in 0..3 {
                for s in 0..3 {
                    for w in 0..3 {
                        assert!(seen.insert(fog_index(n, e, s, w)));
                    }
                }
            }
        }
        assert_eq!(seen.len(), 81);
        assert!(seen.iter().all(|&i| i < 81));
    }

    #[test]
    fn fog_thins_toward_whatever_is_known() {
        let sight = sight(&[
            ".....", //
            ".*r..", //
            ".....", //
            ".....", //
        ]);
        // Every corner of a tile in sight is in sight, so nothing hides it.
        assert_eq!(sight.fog_cell(Coord::new(1, 1)), FOG_CLEAR);
        // A remembered neighbour shares two corners with it, which are in sight; the rest of
        // its corners are only remembered or unseen.
        assert_eq!(sight.fog_cell(Coord::new(2, 1)), fog_index(2, 1, 1, 2));
        // A tile with a known neighbour only diagonally still thins out at that corner.
        assert_eq!(sight.fog_cell(Coord::new(0, 0)), fog_index(0, 0, 2, 0));
        assert_eq!(sight.fog_cell(Coord::new(3, 2)), fog_index(1, 0, 0, 0));
        // Far from anything known the fog is solid; off the rectangle nothing is known.
        assert_eq!(sight.fog_cell(Coord::new(4, 3)), FOG_BLACK);
        assert_eq!(sight.state(Coord::new(-1, 1)), UNSEEN);
        assert_eq!(sight.state(Coord::new(5, 1)), UNSEEN);
        assert_eq!(sight.state(Coord::new(2, 1)), REMEMBERED);
    }

    #[test]
    fn sight_is_what_the_player_has_charted_and_what_their_pieces_can_see() {
        let host = fourx_runtime::Host::base_scenario("dawn-straits", 1).unwrap();
        let game = host.game.clone();
        let capital = game.cities[&1].position;
        let rect = (0, 0, game.map.width - 1, game.map.height - 1);
        let sight = Sight::over(&game, 1, rect);
        assert_eq!(sight.state(capital), IN_SIGHT);
        // Charted but out of sight is only remembered; the radius is the sim's.
        let edge = capital.offset(VISION_RADIUS, 0);
        assert_eq!(sight.state(edge), IN_SIGHT);
        let beyond = capital.offset(VISION_RADIUS + 1, 0);
        assert_eq!(sight.state(beyond), UNSEEN);
        let mut game = game;
        game.factions.get_mut(&1).unwrap().explored.insert(beyond);
        let sight = Sight::over(&game, 1, rect);
        assert_eq!(sight.state(beyond), REMEMBERED);
        // The void around an upright map is never known, to anyone.
        let void = Coord::new(0, 0);
        assert!(!game.map.contains(void));
        assert_eq!(sight.state(void), UNSEEN);
        let spectator = Sight::over(&game, 0, rect);
        assert_eq!(spectator.state(void), UNSEEN);
        assert!(game.map.positions().all(|p| spectator.state(p) == IN_SIGHT));
    }

    #[test]
    fn bands_of_the_draw_order_never_overlap() {
        // The deepest screen position of the largest world still sits within a band.
        let deepest = depth(-30_048.0);
        assert!(deepest > 0.0 && deepest < 1.0, "{deepest}");
        // Lower on screen is in front, by more than the gap used within one tile.
        assert!(depth(-64.0) - depth(-32.0) > Z_STEP * 2.0);
        const { assert!(Z_RELIEF + 1.0 < Z_FOG && Z_FOG < Z_BORDER && Z_BORDER + 1.0 < Z_CITY) };
        const { assert!(Z_CITY + 1.0 < Z_UNIT) };
    }

    #[test]
    fn the_chart_stands_a_lattice_map_upright() {
        let host = fourx_runtime::Host::base_scenario("dawn-straits", 1).unwrap();
        let map = &host.game.map;
        let chart = Chart::of(map);
        // 24 columns of 18 rows: one pixel for every tile, none shared, none left over.
        assert_eq!((chart.width, chart.height), (24, 18));
        let mut seen = HashSet::new();
        for p in map.positions() {
            let (x, y) = chart.pixel(p);
            assert!(
                (0..24).contains(&x) && (0..18).contains(&y),
                "{p:?} -> {x},{y}"
            );
            assert_eq!(chart.tile(x, y), p);
            assert!(seen.insert((x, y)));
        }
        assert_eq!(seen.len(), map.tile_count());
        // North is up and east is right on the chart as on the screen.
        let (nw, ne, sw) = (chart.tile(0, 0), chart.tile(23, 0), chart.tile(0, 17));
        let (nw, ne, sw) = (nw.screen(), ne.screen(), sw.screen());
        assert!(ne.0 > nw.0 && (ne.1 - nw.1).abs() <= 32.0 && sw.1 < nw.1);
        // Clicking in the middle of the image finds a tile near the middle of the map.
        let middle = chart.tile_at(Vec2::splat(0.5)).screen();
        let (left, bottom, right, top) = map.screen_bounds();
        assert!((middle.0 - (left + right) / 2.0).abs() < 100.0);
        assert!((middle.1 - (bottom + top) / 2.0).abs() < 100.0);
        // The camera's footprint is the screen rectangle, not the tile box that holds it.
        let whole = WorldRect {
            min: Vec2::new(left, bottom),
            max: Vec2::new(right, top),
        };
        let (lo, hi) = chart.view(whole);
        assert!(
            lo.x < 0.05 && lo.y < 0.05 && hi.x > 0.95 && hi.y > 0.9,
            "{lo} {hi}"
        );
        let quarter = WorldRect {
            min: Vec2::new(left, (bottom + top) / 2.0),
            max: Vec2::new((left + right) / 2.0, top),
        };
        let (lo, hi) = chart.view(quarter);
        assert!(
            lo.x < 0.05
                && lo.y < 0.05
                && (0.45..0.6).contains(&hi.x)
                && (0.45..0.6).contains(&hi.y),
            "{lo} {hi}"
        );
    }

    #[test]
    fn only_pure_cells_take_tonal_variants() {
        let p = Coord::new(7, 11);
        // Mixed cells and sheets without variants never change.
        assert_eq!(varied(86, p, VARIANTS, 0), 86);
        assert_eq!(varied(85, p, 0, 0), 85);
        // Pure cells stay in their digit across every variant.
        let mut seen = HashSet::new();
        for x in 0..200 {
            let index = varied(85 * 2, Coord::new(x, 3), VARIANTS, 0);
            assert!(
                index == 170 || (256..256 + 4 * VARIANTS).contains(&index),
                "{index}"
            );
            if index != 170 {
                assert_eq!(index % 4, 2);
            }
            seen.insert(index);
        }
        assert_eq!(seen.len(), VARIANTS + 1);
        // The water sheet's land digit has no variants.
        assert!((0..50).all(|x| varied(0, Coord::new(x, 1), VARIANTS, 1) == 0));
    }
}
