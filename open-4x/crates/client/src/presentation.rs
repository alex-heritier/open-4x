use super::*;
use crate::puppet::Puppet;
use bevy::image::{ImageLoaderSettings, ImageSampler};
use bevy::input::mouse::MouseWheel;
use bevy::ui::RelativeCursorPosition;
use fourx_content::{Visuals, animation::Clip};
use fourx_sim::{
    Command, Date, Flavor, Order, Unit, UnitDef, VISION_RADIUS,
    terrain::{Cover, Relief, Terrain, Tile},
};
use std::collections::{BTreeMap, HashMap, HashSet};

use crate::chrome::{Frame, Skin, dress, texture, trim};
use crate::theme::*;

/// Minimap pixel colours.
const MAP_OCEAN: [u8; 3] = [22, 52, 84];
const MAP_SEA: [u8; 3] = [33, 84, 99];
const MAP_COAST: [u8; 3] = [64, 128, 146];
const MAP_GRASS: [u8; 3] = [110, 122, 66];
const MAP_PLAINS: [u8; 3] = [166, 160, 92];
const MAP_DESERT: [u8; 3] = [208, 178, 112];
const MAP_TUNDRA: [u8; 3] = [150, 158, 140];
const MAP_FOG: [u8; 3] = [20, 33, 38];
const MAP_OWN_CITY: [u8; 3] = [244, 238, 220];
const MAP_OWN_CITY_CORE: [u8; 3] = [24, 28, 30];

/// Which corner of the window a card of the top bar sits in.
#[derive(PartialEq, Eq, Clone, Copy)]
enum Side {
    Left,
    Right,
}

/// One readout of the nation's panel, a row of it: a painted emblem, the figure, and the second
/// figure (a rate, a target) at the far end if there is one, over a gilt hairline.
fn stat(
    parent: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    prefix: &str,
    emblem: &str,
    value: impl Into<String>,
    second: Option<(String, Color)>,
) {
    parent
        .spawn((
            Node {
                width: percent(100),
                align_items: AlignItems::Center,
                column_gap: px(12),
                padding: UiRect::vertical(px(2)),
                border: UiRect::bottom(px(1)),
                flex_shrink: 0.0,
                ..default()
            },
            BorderColor::all(TRIM.with_alpha(0.28)),
        ))
        .with_children(|p| {
            crate::chrome::emblem(p, assets, prefix, emblem, 28.0);
            p.spawn(Node {
                flex_grow: 1.0,
                ..default()
            })
            .with_children(|v| bold(v, value, 17.0, ON_PLATE));
            if let Some((second, ink)) = second {
                bold(p, second, 15.0, ink);
            }
        });
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

/// "January 1, 1895".
fn date_text(date: Date) -> String {
    format!("{} {}, {}", date.month_name(), date.day(), date.year())
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

/// The pack's picture of a nation (its flag and leader art), if it has one.
pub(crate) fn nation_art(
    session: &Session,
    assets: &AssetServer,
    tag: &str,
) -> Option<Handle<Image>> {
    let path = session.pack.visuals.nations.get(tag)?;
    Some(assets.load(format!("{}{path}", session.asset_prefix)))
}

fn nation_rgb(game: &Game, owner: Id) -> [u8; 3] {
    game.factions
        .get(&owner)
        .map_or([128, 128, 128], |f| f.color.0)
}
pub(crate) fn nation_color(game: &Game, owner: Id, alpha: f32) -> Color {
    let [r, g, b] = nation_rgb(game, owner);
    Color::srgba(
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
        alpha,
    )
}
/// A sprite tint that leans toward a nation's colour without hiding the art.
pub(super) fn tint(game: &Game, owner: Id) -> Color {
    let [r, g, b] = nation_rgb(game, owner);
    Color::srgb(lean_channel(r), lean_channel(g), lean_channel(b))
}
/// One channel of [`tint`]: a nation colour washed most of the way to white, so multiplying a
/// sprite by it keeps the paint's shading but turns its hue to the nation's.
fn lean_channel(c: u8) -> f32 {
    let base = f32::from(c) / 255.0;
    // A muted colour (a grey, a uniform) still ends up clearly the nation's hue.
    1.0 - 0.62 * (1.0 - base)
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

/// Columns of the shore-cut farm sheet (rows are the four roles of a cell, see
/// `Map::shore_pieces`).
pub(super) const FARM_SHORE_COLUMNS: u32 = 8;

/// A shore-cut farm sheet: 8 x 4 terrain-sized pieces, one sheet per crop.
pub(super) struct FarmShore {
    /// The sheet, held from the first frame so that it loads.
    image: Handle<Image>,
    layout: Handle<TextureAtlasLayout>,
    /// Size of the image the layout was last cut for; zero until it has loaded.
    sheet: UVec2,
}
impl FarmShore {
    fn new(
        assets: &AssetServer,
        layouts: &mut Assets<TextureAtlasLayout>,
        prefix: &str,
        path: &str,
    ) -> Self {
        Self {
            image: assets.load(format!("{prefix}{path}")),
            layout: layouts.add(TextureAtlasLayout::from_grid(
                UVec2::new(256, 128),
                FARM_SHORE_COLUMNS,
                4,
                None,
                None,
            )),
            sheet: UVec2::ZERO,
        }
    }
}

/// Columns and rows of the fog sheet.
pub(super) const FOG_COLUMNS: u32 = 9;

/// Ground and improvements, then a north-to-south painter band containing relief,
/// cover, mines and borders together. A foreground mountain can obscure a border
/// behind it. Fog covers that band; cities and units stay above fog.
const Z_RELIEF: f32 = 10.0;
const Z_FOG: f32 = 15.0;
const Z_BORDER: f32 = Z_RELIEF + 3.0 * Z_STEP;
const Z_CITY: f32 = 20.0;
pub(super) const Z_UNIT: f32 = 30.0;
/// Corner offsets, from a tile, of the dual cells that have it as their north, east, south and
/// west vertex.
const FARM_CELLS: [(i32, i32); 4] = [(0, 0), (-1, 0), (-1, -1), (0, -1)];
/// Side of a resource picture on the map, in map pixels.
const RESOURCE_ICON: f32 = 60.0;
/// Gap that puts one sprite of a tile in front of another of the same tile without reaching
/// past the next tile in line (neighbours differ by at least `depth(32.0)`).
const Z_STEP: f32 = 0.00008;
/// Where a piece standing at screen height `sy` sits within its band: lower on the screen is in
/// front. Bounded by 0.7 for the largest world.
pub(super) fn depth(sy: f32) -> f32 {
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
                if let Some(canonical) = game.map.normalize(p) {
                    let state = if player == 0 {
                        IN_SIGHT
                    } else if explored.is_some_and(|e| e.contains(canonical)) {
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
            let columns = game.map.lattice.map(|l| l.columns);
            let copies = columns.map_or(0..=0, |c| {
                (x0 - source.x - VISION_RADIUS).div_euclid(c)
                    ..=(x1 - source.x + VISION_RADIUS).div_euclid(c)
            });
            for k in copies {
                let source = columns.map_or(source, |c| source.offset(k * c, -k * c));
                for y in (source.y - VISION_RADIUS).max(y0)..=(source.y + VISION_RADIUS).min(y1) {
                    for x in (source.x - VISION_RADIUS).max(x0)..=(source.x + VISION_RADIUS).min(x1)
                    {
                        let p = Coord::new(x, y);
                        if game.map.normalize(p).is_some() {
                            sight.set(p, IN_SIGHT);
                        }
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
pub(crate) fn city_sprite<'a>(visuals: &'a Visuals, game: &Game, owner: Id) -> &'a str {
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
    for path in session.pack.visuals.relief_borders.values() {
        let image: Handle<Image> = assets.load(format!("{}{path}", session.asset_prefix));
        let Some(sheet) = images.get(&image).map(Image::size) else {
            continue;
        };
        if sheet.x == 0 || sheet.y == 0 || sheet.x % 4 != 0 {
            continue;
        }
        let entry = art.relief_borders.entry(path.clone()).or_insert_with(|| {
            (
                UVec2::ZERO,
                layouts.add(TextureAtlasLayout::from_grid(
                    UVec2::new(sheet.x / 4, sheet.y),
                    4,
                    1,
                    None,
                    None,
                )),
            )
        });
        if entry.0 != sheet {
            if let Some(layout) = layouts.get_mut(&entry.1) {
                *layout = TextureAtlasLayout::from_grid(
                    UVec2::new(sheet.x / 4, sheet.y),
                    4,
                    1,
                    None,
                    None,
                );
            }
            entry.0 = sheet;
            changed = true;
        }
    }
    let visuals = &session.pack.visuals;
    let shores = std::iter::once(visuals.farm_shore.as_str())
        .chain(visuals.farms.values().map(|farm| farm.farm_shore.as_str()))
        .filter(|path| !path.is_empty());
    for path in shores {
        // Kept in `Art`: nothing draws with a sheet until it has loaded, and a handle that
        // is dropped before then would cancel the load.
        let shore = art
            .farm_shores
            .entry(path.to_string())
            .or_insert_with(|| FarmShore::new(&assets, &mut layouts, &session.asset_prefix, path));
        if let Some(sheet) = images.get(&shore.image).map(Image::size) {
            let cell = UVec2::new(sheet.x / FARM_SHORE_COLUMNS, sheet.y / 4);
            if sheet != shore.sheet && cell.min_element() > 0 {
                if let Some(layout) = layouts.get_mut(&shore.layout) {
                    *layout =
                        TextureAtlasLayout::from_grid(cell, FARM_SHORE_COLUMNS, 4, None, None);
                }
                shore.sheet = sheet;
                changed = true;
            }
        }
    }
    let (prefix, visuals) = (&session.asset_prefix, &session.pack.visuals);
    changed |= art
        .roads
        .fit(prefix, &visuals.roads, &assets, &images, &mut layouts)
        | art
            .rails
            .fit(prefix, &visuals.railroads, &assets, &images, &mut layouts);
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

/// Dedicated relief/cover art uses the tile's own layers, never a neighbour's cover.
/// Snow and arid bases retain their climate when trees grow over them.
fn overlay_names(tile: &Tile) -> (Option<&'static str>, Option<&'static str>) {
    let relief = match (tile.relief, tile.terrain, tile.cover) {
        (Relief::Flat, _, _) => None,
        (Relief::Mountains, Terrain::Desert, Cover::Forest) => Some("mountain_dry_forest"),
        (Relief::Mountains, Terrain::Tundra, Cover::Forest) => Some("mountain_cold_forest"),
        (Relief::Mountains, _, Cover::Forest) => Some("mountain_forest"),
        (Relief::Mountains, Terrain::Desert, Cover::Jungle) => Some("mountain_dry_jungle"),
        (Relief::Mountains, Terrain::Tundra, Cover::Jungle) => Some("mountain_cold_jungle"),
        (Relief::Mountains, _, Cover::Jungle) => Some("mountain_jungle"),
        (Relief::Mountains, Terrain::Desert, _) => Some("mountain_dry"),
        (Relief::Mountains, Terrain::Tundra, _) => Some("mountain_cold"),
        (Relief::Mountains, _, _) => Some("mountain"),
        (Relief::Hills, Terrain::Desert, Cover::Forest) => Some("hills_dry_forest"),
        (Relief::Hills, Terrain::Tundra, Cover::Forest) => Some("hills_cold_forest"),
        (Relief::Hills, _, Cover::Forest) => Some("hills_forest"),
        (Relief::Hills, Terrain::Desert, Cover::Jungle) => Some("hills_dry_jungle"),
        (Relief::Hills, Terrain::Tundra, Cover::Jungle) => Some("hills_cold_jungle"),
        (Relief::Hills, _, Cover::Jungle) => Some("hills_jungle"),
        (Relief::Hills, Terrain::Desert, _) => Some("hills_dry"),
        (Relief::Hills, Terrain::Tundra, _) => Some("hills_cold"),
        (Relief::Hills, _, _) => Some("hills"),
    };
    let cover = if relief.is_some() {
        None
    } else {
        cover_name(tile)
    };
    (relief, cover)
}

fn cover_name(tile: &Tile) -> Option<&'static str> {
    match tile.cover {
        Cover::Bare => None,
        Cover::Forest if tile.relief != Relief::Flat || tile.terrain == Terrain::Tundra => {
            Some("pine")
        }
        Cover::Forest => Some("forest"),
        Cover::Jungle => Some("jungle"),
        Cover::Marsh => Some("marsh"),
    }
}

/// Old packs still render both layers if they do not supply combined artwork. Border
/// selection uses this resolved name so a ribbon never targets a different heightfield.
fn resolved_overlay_names(
    tile: &Tile,
    visuals: &Visuals,
) -> (Option<&'static str>, Option<&'static str>) {
    let (mut relief, mut cover) = overlay_names(tile);
    if let Some(name) = relief {
        if !visuals.overlays.contains_key(name) && tile.cover != Cover::Bare {
            let mut bare = tile.clone();
            bare.cover = Cover::Bare;
            relief = overlay_names(&bare).0;
            cover = cover_name(tile);
        }
        if let Some(name) = relief {
            if name != "mountain" && !visuals.overlays.contains_key(name) {
                relief = Some("mountain");
            }
        }
    }
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
    walks: Res<crate::march::Walks>,
    shapes: Res<Shapes>,
    mut puppetry: crate::puppet::Puppetry,
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
                focus(&mut transform, position);
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
            .with_children(|p| text(p, &session.message, 24.0, ON_PLATE));
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
    for y in (y0 - 1)..=y1 {
        for x in (x0 - 1)..=x1 {
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
                    .any(|&(dx, dy)| game.map.normalize(p.offset(dx, dy)).is_some())
            {
                continue;
            }
            let art_position = game.map.nearest(Coord::default(), p);
            let layers = [
                (
                    game.map
                        .ground_cell(p)
                        .map(|i| varied(i, art_position, art.variants, 0)),
                    &ground,
                    &art.cells,
                    0.0,
                ),
                (
                    game.map
                        .water_cell(p)
                        .map(|i| varied(i, art_position, art.variants, 1)),
                    &water,
                    &art.cells,
                    0.05,
                ),
                (
                    Some(game.map.river_cell(p))
                        .filter(|&mask| mask != 0)
                        .map(|mask| scatter(art_position) % art.meanders.max(1) * 16 + mask),
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
    // Combined relief/cover sprites share their surface and anchor. Packs without them
    // retain the separate cover fallback.
    for y in y0..=y1 {
        for x in x0..=x1 {
            let p = Coord::new(x, y);
            let Some(tile) = game.map.get(p) else {
                continue;
            };
            let (relief, cover) = resolved_overlay_names(tile, visuals);
            if (relief.is_none() && cover.is_none()) || !is_known(p) {
                continue;
            }
            let (sx, sy) = p.screen();
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
    // Each crop's whole-tile overlay, loaded once per redraw however many farms use it.
    let mut farm_images: HashMap<&str, Handle<Image>> = HashMap::new();
    let mine: Handle<Image> = assets.load(format!("{prefix}{}", session.pack.visuals.mine));
    // Resource icons, in the order of the map's kinds; a kind the pack has no art for is not drawn.
    let resource_icons: Vec<Option<Handle<Image>>> = game
        .map
        .resources
        .iter()
        .map(|kind| {
            visuals
                .resources
                .get(&kind.id)
                .map(|path| assets.load(format!("{prefix}{path}")))
        })
        .collect();
    for y in y0..=y1 {
        for x in x0..=x1 {
            let p = Coord::new(x, y);
            let Some(tile) = game.map.get(p) else {
                continue;
            };
            let (sx, sy) = p.screen();
            if !is_known(p) || !built.has(Vec2::new(sx, sy)) {
                continue;
            }
            if tile.improvements & Tile::FARM != 0 {
                let pieces = game.map.shore_pieces(tile.position);
                // The crop follows the culture of whoever owns the land.
                let flavor = game
                    .factions
                    .get(&tile.owner)
                    .filter(|_| tile.owner != 0)
                    .map(|faction| faction.flavor);
                let (farm_path, shore_path) = visuals.farm(flavor);
                let farm = farm_images
                    .entry(farm_path)
                    .or_insert_with(|| assets.load(format!("{prefix}{farm_path}")));
                match art.farm_shores.get(shore_path) {
                    // Beside water the farm is drawn as four terrain-cell pieces that stop at
                    // the shore; the sheet is only usable once its layout has been cut.
                    Some(shore)
                        if shore.sheet != UVec2::ZERO
                            && pieces.iter().any(|piece| piece % 8 != 0) =>
                    {
                        for (piece, (dx, dy)) in pieces.into_iter().zip(FARM_CELLS) {
                            let (cx, cy) = p.offset(dx, dy).screen();
                            let mut sprite = Sprite::from_atlas_image(
                                shore.image.clone(),
                                TextureAtlas {
                                    layout: shore.layout.clone(),
                                    index: piece,
                                },
                            );
                            sprite.custom_size = Some(Vec2::new(128.0, 64.0));
                            commands.spawn((
                                sprite,
                                Transform::from_xyz(cx, cy - 32.0, 2.0),
                                WorldVisual,
                            ));
                        }
                    }
                    _ => image_sprite(
                        &mut commands,
                        farm,
                        Vec3::new(sx, sy, 2.0),
                        Vec2::new(128.0, 64.0),
                        Color::WHITE,
                    ),
                }
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
            // A city stands over its own square, so only open squares show their resource.
            if let Some(Some(icon)) = usize::from(tile.resource)
                .checked_sub(1)
                .and_then(|i| resource_icons.get(i))
                .filter(|_| !city_tiles.contains(&tile.position))
            {
                image_sprite(
                    &mut commands,
                    icon,
                    // The picture stands on a point 40/128 of its height below its centre.
                    Vec3::new(sx, sy + 16.0, Z_RELIEF + depth(sy) + 4.0 * Z_STEP),
                    Vec2::splat(RESOURCE_ICON),
                    Color::WHITE,
                );
            }
        }
    }
    roads::draw(
        &mut commands,
        game,
        &city_tiles,
        built,
        is_known,
        &art.roads,
        &art.rails,
    );

    // The fog of war lies over the terrain, the relief and the improvements, and under the
    // cities and units. Borders belong to their terrain surface and dim with it.
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
            let index = if game.map.normalize(p).is_some() {
                sight.fog_cell(p)
            } else if upright
                && Coord::NEIGHBORS
                    .iter()
                    .any(|&(dx, dy)| game.map.normalize(p.offset(dx, dy)).is_some())
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
    // Culture borders: a bold dashed line in the owner's colour just inside the edge of each
    // owned tile that faces another nation's tile or nobody's, never an overlay on the whole
    // territory. Each dash carries a black outline in its texture (see `border_masks`) plus a
    // dark halo spawned beneath it, so it reads on any ground like the reference's frontiers.
    // Edges that face the unknown or the edge of the world get none.
    let mut inks: HashMap<Id, Color> = HashMap::new();
    let mut contour_images: HashMap<&str, Handle<Image>> = HashMap::new();
    for y in y0..=y1 {
        for x in x0..=x1 {
            let p = Coord::new(x, y);
            let Some(tile) = game.map.get(p) else {
                continue;
            };
            if tile.owner == 0 || !is_known(p) {
                continue;
            }
            let (sx, sy) = p.screen();
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
                let surface = resolved_overlay_names(tile, visuals)
                    .0
                    .and_then(|name| visuals.relief_borders.get(name));
                let (mut dashes, lift, size) = if let Some((path, (_, layout))) =
                    surface.and_then(|path| art.relief_borders.get(path).map(|entry| (path, entry)))
                {
                    let image = contour_images
                        .entry(path)
                        .or_insert_with(|| assets.load(format!("{prefix}{path}")))
                        .clone();
                    let mut sprite = Sprite::from_image(image);
                    sprite.texture_atlas = Some(TextureAtlas {
                        layout: layout.clone(),
                        index,
                    });
                    let size = Vec2::new(128.0, 112.0);
                    sprite.custom_size = Some(size);
                    (sprite, 24.0, size)
                } else {
                    let mut sprite = Sprite::from_atlas_image(
                        border_sheet.clone(),
                        TextureAtlas {
                            layout: art.borders.clone(),
                            index,
                        },
                    );
                    let size = Vec2::new(128.0, 64.0);
                    sprite.custom_size = Some(size);
                    (sprite, 0.0, size)
                };
                dashes.color = ink;
                // A dark halo beneath the coloured dash: the same beads slightly enlarged,
                // so the frontier keeps its edge on pale ground.
                let mut halo = dashes.clone();
                halo.color = Color::srgba(0.02, 0.02, 0.02, 0.9);
                halo.custom_size = Some(size * 1.12);
                commands.spawn((
                    halo,
                    Transform::from_xyz(sx, sy + lift, Z_BORDER + depth(sy) - 0.001),
                    WorldVisual,
                ));
                commands.spawn((
                    dashes,
                    Transform::from_xyz(sx, sy + lift, Z_BORDER + depth(sy)),
                    WorldVisual,
                ));
            }
        }
    }

    for city in game.cities.values() {
        for position in built.copies(&game.map, city.position) {
            let (x, y) = position.screen();
            if !built.has(Vec2::new(x, y)) {
                continue;
            }
            let z = Z_CITY + depth(y);
            let path = city_sprite(visuals, game, city.owner);
            let sprite = city_images
                .entry(path)
                .or_insert_with(|| assets.load(format!("{prefix}{path}")))
                .clone();
            // The town keeps its own paint; the nation's colour rides the banner over it, as in Civ3,
            // where one accent ramp colours the flag rather than washing the whole sprite.
            let banner = visuals
                .city_banners
                .get(
                    game.factions
                        .get(&city.owner)
                        .map_or(Flavor::Western, |faction| faction.flavor)
                        .key(),
                )
                .or_else(|| visuals.city_banners.get(Flavor::Western.key()));
            let banner_handle = banner.map(|path| {
                city_images
                    .entry(path)
                    .or_insert_with(|| assets.load(format!("{prefix}{path}")))
                    .clone()
            });
            let lit = sight.state(position) == IN_SIGHT;
            let body = if lit {
                Color::WHITE
            } else {
                dimmed(Color::WHITE, REMEMBERED_LIGHT)
            };
            let flag = if lit {
                nation_color(game, city.owner, 1.0)
            } else {
                dimmed(nation_color(game, city.owner, 1.0), REMEMBERED_LIGHT)
            };
            image_sprite(
                &mut commands,
                &sprite,
                Vec3::new(x, y + 24.0, z),
                Vec2::new(128.0, 112.0),
                body,
            );
            if let Some(banner) = banner_handle {
                image_sprite(
                    &mut commands,
                    &banner,
                    Vec3::new(x, y + 24.0, z + 0.05),
                    Vec2::new(128.0, 112.0),
                    flag,
                );
            }
            // The plate: the city's size in its nation's colour at one end, the days
            // until its current work is done at the other, its name between.
            let width = text_width(city.name.chars().count(), 15.0) + 60.0;
            let plate = Vec3::new(x, y - 26.0, z + 0.1);
            label_plate(&mut commands, &shapes, plate, Vec2::new(width, 24.0));
            let ours = city.owner == session.player;
            let left = plate + Vec3::new(5.0 - width / 2.0, 0.0, 0.1);
            medallion(
                &mut commands,
                &shapes,
                left,
                28.0,
                nation_color(game, city.owner, 1.0),
            );
            map_text(
                &mut commands,
                city.population.to_string(),
                13.0,
                ON_ACCENT,
                left + Vec3::Z * 0.2,
            );
            map_text(
                &mut commands,
                &city.name,
                15.0,
                BRONZE,
                plate + Vec3::Z * 0.3,
            );
            let right = plate + Vec3::new(width / 2.0 - 5.0, 0.0, 0.1);
            medallion(
                &mut commands,
                &shapes,
                right,
                28.0,
                if ours {
                    Color::srgb(0.082, 0.157, 0.122)
                } else {
                    Color::srgb(0.075, 0.082, 0.094)
                },
            );
            map_text(
                &mut commands,
                builds_in(
                    city.production
                        .as_ref()
                        .and_then(|id| session.rules.units.get(id))
                        .map(|def| def.cost),
                    city.progress,
                    city.shields(),
                ),
                13.0,
                ON_ACCENT,
                right + Vec3::Z * 0.2,
            );
        }
    }
    // One piece stands for a stack, as in Civ3: the selected unit if it is there, otherwise
    // the best defender, with a count when others stand beneath it.
    let mut stacks: BTreeMap<(i32, i32), Vec<Id>> = BTreeMap::new();
    for unit in game.units.values() {
        for position in built.copies(&game.map, unit.position) {
            let (x, y) = position.screen();
            // A fight is being played out on this square, or the unit is walking: the stage or
            // a walker stands in for it.
            if built.has(Vec2::new(x, y)) && !stage.hides(unit.position) && !walks.hides(unit.id) {
                stacks
                    .entry((position.x, position.y))
                    .or_default()
                    .push(unit.id);
            }
        }
    }
    let mut unit_images: HashMap<String, Handle<Image>> = HashMap::new();
    let selected = match session.selection {
        Selection::Unit(id) => Some(id),
        _ => None,
    };
    for (&(px, py), ids) in &stacks {
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
        let flavor = game
            .factions
            .get(&unit.owner)
            .map_or(Flavor::Western, |faction| faction.flavor);
        let (x, y) = Coord::new(px, py).screen();
        let z = Z_UNIT + depth(y);
        let spent = unit.owner == session.player && unit.moves_left(def) == 0;
        // Every nation's coat, cap and horse blanket lean their colour, as in Civ3; a unit with
        // its movement spent is greyed so the player can still spot it.
        let color = if spent {
            Color::srgb(0.66, 0.66, 0.70)
        } else {
            tint(game, unit.owner)
        };
        let at = Vec3::new(x, y + 12.0, z);
        let mut clip = |kind: &str, which: Clip| {
            puppetry.art.clip(
                prefix,
                &session.pack.id,
                visuals,
                &assets,
                &mut puppetry.layouts,
                kind,
                flavor,
                which,
            )
        };
        match clip(&unit.kind, Clip::Idle) {
            Some((_, idle)) => {
                // Fetch the walk now, so the unit's first steps are not blank.
                clip(&unit.kind, Clip::Run);
                let facing = puppetry.facings.of(unit.id);
                let puppet = Puppet::new(&idle, facing, unit.id);
                commands.spawn((
                    idle.sprite(facing, 0, unit_size(def), color),
                    Transform::from_translation(at),
                    puppet,
                    WorldVisual,
                ));
            }
            None => {
                let design = visuals.unit_design(&unit.kind, flavor).to_string();
                let sprite = unit_images.entry(design.clone()).or_insert_with(|| {
                    let path = visuals
                        .unit(&unit.kind, flavor)
                        .map_or_else(|| def.sprite.clone(), |visual| visual.sprite);
                    assets.load(format!("{prefix}{path}"))
                });
                image_sprite(&mut commands, sprite, at, unit_size(def), color);
            }
        }
        draw_unit_badges(
            &mut commands,
            &shapes,
            unit,
            def,
            ids.len(),
            Vec3::new(x, y, z),
        );
    }
    // Unhurried map typography echoes printed campaign charts.
    if game.scenario == "russo-japanese-war" {
        // Placed on the chart, where the hand-drawn map's squares are.
        let chart = Chart::of(&game.map);
        for (label, pos) in [
            ("YELLOW SEA", chart.tile(14, 2)),
            ("KOREA BAY", chart.tile(4, 15)),
        ] {
            for position in built.copies(&game.map, pos) {
                let (x, y) = position.screen();
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
    }
    if rebuild_ui {
        build_ui(&mut commands, &session, size, &minimap, &assets);
    }
}

/// How big a design's sprite is drawn: ships are wider than they are tall. Land
/// pieces stand about two thirds of a tile tall, as in the reference, where a
/// garrisoned unit fills its city's square.
pub(super) fn unit_size(def: &UnitDef) -> Vec2 {
    if def.is_naval() {
        Vec2::new(216.0, 162.0)
    } else {
        Vec2::splat(105.0)
    }
}

/// A unit's hit-point bar, its standing order, its experience, and the size of its stack.
fn draw_unit_badges(
    commands: &mut Commands,
    shapes: &Shapes,
    unit: &Unit,
    def: &UnitDef,
    stack: usize,
    at: Vec3,
) {
    let (x, y, z) = (at.x, at.y, at.z);
    let max = unit.max_hp(def).max(1);
    let hp = unit.hp(def).clamp(0, max);
    let fraction = hp as f32 / max as f32;
    let size = unit_size(def);
    // A dark track with one pip per hit point in it, standing left of the piece
    // like the reference's vertical segmented bar. The first pips sit at the
    // bottom, so wounds empty from the top.
    // Centred on the sprite, which stands 12 above the tile's centre.
    let (length, inner) = (28.0, 26.0);
    let bar = Vec3::new(x - size.x / 2.0 + 4.0, y + 12.0, z + 0.1);
    pill(commands, shapes, bar, Vec2::new(6.0, length), PLATE);
    let lit = if fraction > 0.66 {
        MAP_HEALTHY
    } else {
        hp_color(fraction)
    };
    let pips = pip_layout_gapped(hp, max, inner, 1.0);
    // Square-cornered pips, so the segments read as blocks rather than beads.
    let mut pip = |at: Vec3, size: Vec2, color: Color| {
        commands.spawn((
            Sprite::from_color(color, size),
            Transform::from_translation(at),
            WorldVisual,
        ));
    };
    if pips.is_empty() {
        let fill = (inner * fraction).max(4.0);
        pip(
            Vec3::new(bar.x, bar.y - inner / 2.0 + fill / 2.0, z + 0.2),
            Vec2::new(4.0, fill),
            lit,
        );
    } else {
        for p in &pips {
            pip(
                Vec3::new(bar.x, bar.y + p.centre, z + 0.2),
                Vec2::new(4.0, p.width),
                if p.lit { lit } else { SPENT },
            );
        }
    }
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
    // The corners ride the sprite's shoulders, whatever its design's size.
    let corner = Vec2::new(size.x / 2.0 - 7.0, 12.0 + size.y / 2.0 - 9.0);
    if !mark.is_empty() {
        badge(
            commands,
            shapes,
            &mark,
            Vec3::new(x - corner.x, y + corner.y, z),
            text_width(mark.len(), 11.0) + 12.0,
        );
    }
    if stack > 1 {
        badge(
            commands,
            shapes,
            &format!("x{stack}"),
            Vec3::new(x + corner.x, y + corner.y, z),
            text_width(2, 11.0) + 14.0,
        );
    }
}

/// Days until a city's current work of `cost` finishes, for the map plate: "-"
/// when nothing is being built or nothing is gathered toward it. Same count as
/// the city view's "Ready in".
fn builds_in(cost: Option<i32>, progress: i32, shields: i32) -> String {
    let days = cost.and_then(|cost| crate::city::days(cost - progress, shields));
    match days {
        Some(n) => n.max(1).to_string(),
        None => "-".into(),
    }
}

fn badge(commands: &mut Commands, shapes: &Shapes, label: &str, at: Vec3, width: f32) {
    pill(
        commands,
        shapes,
        at + Vec3::Z * 0.3,
        Vec2::new(width, 16.0),
        PLATE,
    );
    map_text(commands, label, 11.0, ON_PLATE, at + Vec3::Z * 0.4);
}

/// Movement counted in thirds, shown as whole moves and fractions: 7 is "2 1/3".
fn moves_text(thirds: u32) -> String {
    match (thirds / 3, thirds % 3) {
        (whole, 0) => whole.to_string(),
        (0, part) => format!("{part}/3"),
        (whole, part) => format!("{whole} {part}/3"),
    }
}

/// Squares a selected unit can hit now: adjacent hostile squares for melee, anything hostile
/// within range for a bombarding unit. Each is paired with whether it would be a bombardment.
pub(super) fn strike_targets(game: &Game, rules: &Rules, unit: &Unit) -> Vec<(Coord, bool)> {
    let def = rules.def(unit);
    let ranged = def.can_bombard();
    if unit.moves_left(def) == 0 || !(ranged || def.can_attack()) {
        return Vec::new();
    }
    let reach = if ranged { def.range } else { 1 };
    let mut targets = Vec::new();
    for dy in -reach..=reach {
        for dx in -reach..=reach {
            let Some(target) = game.map.normalize(unit.position.offset(dx, dy)) else {
                continue;
            };
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

/// A unit's title: its nation's flag, its name, and who it belongs to.
fn title(p: &mut ChildSpawnerCommands, color: Color, name: String, subtitle: String) {
    p.spawn(Node {
        align_items: AlignItems::Center,
        column_gap: px(10),
        flex_shrink: 0.0,
        ..default()
    })
    .with_children(|p| {
        nation_flag(p, color, 26.0);
        p.spawn(Node {
            flex_direction: FlexDirection::Column,
            ..default()
        })
        .with_children(|p| {
            bold(p, name, 17.0, INK);
            text(p, subtitle, 12.0, INK_SOFT);
        });
    });
}

/// Everything about a selected unit: its numbers, its orders, and what it can do from here.
fn unit_panel(p: &mut ChildSpawnerCommands, game: &Game, session: &Session, u: &Unit) {
    let def = session.rules.def(u);
    let mine = u.owner == session.player;
    let owner = game
        .factions
        .get(&u.owner)
        .map_or("Unknown", |f| f.adjective.as_str());
    title(
        p,
        nation_color(game, u.owner, 1.0),
        def.name.clone(),
        format!("{owner} · {}", u.level_name()),
    );
    rule(p);
    if def.is_defenseless() && !def.can_attack() {
        text(
            p,
            "Cannot attack or defend; captured if caught.",
            13.0,
            INK_SOFT,
        );
    } else {
        row(p, "Attack", def.attack.to_string(), INK);
        row(p, "Defense", def.defense.to_string(), INK);
    }
    if def.can_bombard() {
        row(
            p,
            "Bombard",
            format!(
                "{} · range {} · {} shots",
                def.bombard, def.range, def.rate_of_fire
            ),
            INK,
        );
    }
    // Hit points are the reference's morale bar: a figure over a meter of pips.
    let max = u.max_hp(def).max(1);
    let hp = u.hp(def).clamp(0, max);
    row(p, "Hit points", format!("{hp} / {max}"), INK);
    hp_pips(p, hp, max, 8.0);
    if mine {
        row(
            p,
            "Moves",
            format!("{} of {}", moves_text(u.moves_left(def)), def.moves),
            INK,
        );
        if def.defense > 0 {
            heading(p, "Modifiers");
            row(
                p,
                "Terrain and works here",
                format!("+{}%", game.defender_percent(u, def)),
                GOOD,
            );
        }
    }
    if def.blitz {
        text(p, "Attacks again while moves remain.", 12.5, INK_SOFT);
    }
    heading(p, "Orders");
    bold(
        p,
        match u.order {
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
        },
        13.0,
        INK,
    );
    if !mine {
        return;
    }
    // The orders themselves are buttons in the bar at the foot of the map (see `bar`); what
    // can be said in words stays here.
    if def.can_work() {
        let options = game.job_options(u.position);
        if options.values().all(Result::is_err) {
            let why = options
                .get(&Job::Road)
                .and_then(|o| o.as_ref().err())
                .copied()
                .unwrap_or("Nothing to build here");
            text(p, format!("Nothing to build here: {why}."), 12.5, INK_SOFT);
        }
    }
    let strikes = crate::bar::strikes(game, &session.rules, u);
    if !strikes.is_empty() {
        heading(p, "In reach");
        for strike in strikes {
            text(p, strike.line(), 12.5, INK);
        }
    }
    text(
        p,
        "Right-click a square to send it there, or an enemy to attack it.",
        12.0,
        INK_SOFT,
    );
}

/// A row of the terrain panel with the pane's hairline under it: `label` at the left, and
/// whatever `value` adds at the right.
fn tile_row(
    p: &mut ChildSpawnerCommands,
    label: &str,
    value: impl FnOnce(&mut ChildSpawnerCommands),
) {
    p.spawn((
        Node {
            width: percent(100),
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            column_gap: px(10),
            flex_shrink: 0.0,
            padding: UiRect::vertical(px(3)),
            border: UiRect::bottom(px(1)),
            ..default()
        },
        BorderColor::all(RULE),
    ))
    .with_children(|p| {
        text(p, label, 14.0, INK_SOFT);
        p.spawn(Node {
            align_items: AlignItems::Center,
            column_gap: px(8),
            ..default()
        })
        .with_children(value);
    });
}

/// What the player knows of a square: its terrain pictured and named, who holds it, what stands
/// on it, and what it yields.
fn tile_panel(
    p: &mut ChildSpawnerCommands,
    game: &Game,
    session: &Session,
    pos: Coord,
    assets: &AssetServer,
) {
    let prefix = session.asset_prefix.as_str();
    let t = game.map.get(pos).unwrap();
    let region = game.region_of(pos).map(|r| r.name.clone());
    let holder = game.factions.get(&t.owner).filter(|_| t.owner != 0);
    let border = game.cities.get(&t.claim).map(|c| c.name.clone());
    // The head: the name over what kind of country it is, and the landscape fading in behind.
    p.spawn(Node {
        width: percent(100),
        height: px(74),
        flex_shrink: 0.0,
        margin: UiRect::bottom(px(6)),
        ..default()
    })
    .with_children(|h| {
        h.spawn((
            ImageNode::new(crate::chrome::picture(
                assets,
                prefix,
                crate::chrome::scene_for(t),
            )),
            Node {
                position_type: PositionType::Absolute,
                right: px(-4),
                top: px(-4),
                width: px(168),
                height: px(75),
                ..default()
            },
        ));
        h.spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(0),
            ..default()
        })
        .with_children(|c| {
            bold(c, t.terrain.name(), 22.0, INK);
            text(
                c,
                region.unwrap_or_else(|| "Open country".into()),
                14.0,
                INK_SOFT,
            );
        });
    });
    tile_row(p, "Held by", |v| {
        if let Some(faction) = holder {
            let art = nation_art(session, assets, &faction.tag);
            v.spawn((
                Node {
                    width: px(23),
                    height: px(16),
                    border: UiRect::all(px(1)),
                    flex_shrink: 0.0,
                    ..default()
                },
                BackgroundColor(nation_color(game, faction.id, 1.0)),
                BorderColor::all(BRONZE),
            ))
            .with_children(|f| {
                if let Some(art) = art {
                    // The flag's picture is a portrait of a waving cloth: show its middle.
                    let mut flag = ImageNode::new(art);
                    flag.rect = Some(Rect::new(0.0, 48.0, 200.0, 48.0 + 200.0 * 16.0 / 23.0));
                    f.spawn((
                        flag,
                        Node {
                            width: percent(100),
                            height: percent(100),
                            ..default()
                        },
                    ));
                }
            });
            bold(v, faction.name.clone(), 14.5, INK);
        } else {
            bold(v, "Unclaimed", 14.5, INK);
        }
    });
    if let Some(city) = border {
        tile_row(p, "Border of", |v| bold(v, city, 14.5, INK));
    }
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
    if !features.is_empty() {
        tile_row(p, "Features", |v| bold(v, features.join(", "), 14.5, INK));
    }
    if let Some(resource) = game.map.resource(t) {
        tile_row(p, "Resource", |v| {
            bold(
                v,
                format!("{} ({})", resource.name, resource.class.name()),
                14.5,
                INK,
            )
        });
    }
    tile_row(p, "Movement cost", |v| {
        bold(
            v,
            (if t.is_land() { t.move_cost() } else { 1 }).to_string(),
            14.5,
            INK,
        )
    });
    let defense = t.defense_percent();
    tile_row(p, "Defense bonus", |v| {
        bold(
            v,
            format!("+{defense}%"),
            14.5,
            if defense > 0 { GOOD } else { INK },
        )
    });
    let gathered = fourx_sim::economy::square_yield(&game.map, t, game.city_at(pos).is_some());
    heading(p, "Yields");
    for (icon, name, amount) in [
        ("wheat", "Food", gathered.food),
        ("shield", "Shields", gathered.shields),
        ("coins", "Gold", gathered.gold),
    ] {
        p.spawn((
            Node {
                width: percent(100),
                align_items: AlignItems::Center,
                column_gap: px(14),
                flex_shrink: 0.0,
                padding: UiRect::new(px(10), px(0), px(1), px(1)),
                border: UiRect::bottom(px(1)),
                ..default()
            },
            BorderColor::all(RULE.with_alpha(0.14)),
        ))
        .with_children(|y| {
            crate::chrome::emblem(y, assets, prefix, icon, 30.0);
            y.spawn(Node {
                flex_grow: 1.0,
                ..default()
            })
            .with_children(|n| bold(n, name, 15.0, INK));
            bold(y, amount.to_string(), 15.0, INK);
        });
    }
    p.spawn(Node {
        width: percent(100),
        justify_content: JustifyContent::FlexEnd,
        flex_shrink: 0.0,
        margin: UiRect::top(px(2)),
        ..default()
    })
    .with_children(|c| text(c, format!("({}, {})", pos.x, pos.y), 13.0, INK_SOFT));
}

/// Whether anything is selected that the pane has something to say about.
fn has_info(game: &Game, session: &Session) -> bool {
    match session.selection {
        Selection::Unit(id) => game.units.contains_key(&id),
        Selection::Tile(pos) => game.map.get(pos).is_some(),
        _ => false,
    }
}

/// The pane that describes the selection: the unit or the square.
fn info(p: &mut ChildSpawnerCommands, game: &Game, session: &Session, assets: &AssetServer) {
    match session.selection {
        Selection::Unit(id) if game.units.contains_key(&id) => {
            unit_panel(p, game, session, &game.units[&id]);
        }
        Selection::Tile(pos) if game.map.get(pos).is_some() => {
            tile_panel(p, game, session, pos, assets);
        }
        _ => {}
    }
}

/// The face of a floating card: a surface, a hairline, and the shadow that lifts it.
fn floating() -> (BackgroundColor, BorderColor, BoxShadow) {
    (BackgroundColor(CARD), BorderColor::all(EDGE), lift())
}

fn build_ui(
    commands: &mut Commands,
    session: &Session,
    size: Vec2,
    minimap: &Minimap,
    assets: &AssetServer,
) {
    let game = session.game.as_ref().unwrap();
    let prefix = session.asset_prefix.as_str();
    let narrow = size.x < 900.0;
    let me = if session.player == 0 {
        game.commander
    } else {
        session.player
    };
    let faction = &game.factions[&me];
    let date = game.date();
    let margin = if narrow { 6.0 } else { TOP_MARGIN + 4.0 };
    let top_card = |side: Side| Node {
        position_type: PositionType::Absolute,
        top: px(TOP_MARGIN),
        left: if side == Side::Left {
            px(margin)
        } else {
            Val::Auto
        },
        right: if side == Side::Right {
            px(margin)
        } else {
            Val::Auto
        },
        align_items: AlignItems::Center,
        ..default()
    };
    let face = |fill: Color| {
        (
            BackgroundColor(fill),
            lift(),
            GlobalZIndex(5),
            UiRoot,
            TopCard,
            Interaction::default(),
        )
    };
    // Top left: the nation's banner with the number of its cities, and beside it its name over
    // what it holds, one row each: treasury and income, research and its goal, industry.
    let cities = game.cities.values().filter(|c| c.owner == me).count();
    let (banner_width, banner_height) = if narrow { (46.0, 66.0) } else { (66.0, 94.0) };
    let mut nation = top_card(Side::Left);
    nation.padding = UiRect::axes(px(if narrow { 18 } else { 24 }), px(18));
    nation.column_gap = px(if narrow { 12 } else { 18 });
    commands.spawn((nation, face(SLATE))).with_children(|p| {
        dress(p, assets, prefix, Skin::Slate, Frame::Grand, !narrow);
        p.spawn(Node {
            flex_shrink: 0.0,
            margin: UiRect::right(px(4)),
            ..default()
        })
        .with_children(|f| {
            // The flag hangs from a gilt rod.
            f.spawn((
                Node {
                    width: px(banner_width),
                    height: px(banner_height),
                    border: UiRect::new(px(1), px(1), px(0), px(1)),
                    flex_shrink: 0.0,
                    margin: UiRect::top(px(5)),
                    ..default()
                },
                BackgroundColor(nation_color(game, me, 1.0)),
                BorderColor::all(BRONZE),
            ))
            .with_children(|b| {
                if let Some(art) = nation_art(session, assets, &faction.tag) {
                    b.spawn((
                        ImageNode::new(art),
                        Node {
                            width: percent(100),
                            height: percent(100),
                            ..default()
                        },
                    ));
                }
            });
            f.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(-4),
                    top: px(0),
                    width: px(banner_width + 8.0),
                    height: px(7),
                    border: UiRect::all(px(1)),
                    border_radius: rounded(3.0),
                    ..default()
                },
                BackgroundColor(TRIM),
                BorderColor::all(BRONZE),
            ));
            // A round badge on the banner's lower corner.
            f.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    right: px(-17),
                    bottom: px(-9),
                    width: px(32),
                    height: px(32),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    border: UiRect::all(px(3)),
                    border_radius: rounded(999.0),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.075, 0.082, 0.094)),
                BorderColor::all(TRIM),
                BoxShadow::new(Color::BLACK.with_alpha(0.5), px(0), px(2), px(0), px(4)),
            ))
            .with_children(|b| bold(b, cities.to_string(), 14.0, ON_PLATE));
        });
        // A gilt rule between the banner and the figures.
        p.spawn((
            Node {
                width: px(1),
                align_self: AlignSelf::Stretch,
                margin: UiRect::vertical(px(2)),
                flex_shrink: 0.0,
                ..default()
            },
            BackgroundColor(TRIM.with_alpha(0.4)),
        ));
        p.spawn(Node {
            flex_direction: FlexDirection::Column,
            width: px(if narrow { 128 } else { 176 }),
            ..default()
        })
        .with_children(|p| {
            if !narrow {
                bold(p, faction.name.clone(), 19.0, ON_PLATE);
            }
            let income = game.income(me);
            stat(
                p,
                assets,
                prefix,
                "coin",
                faction.gold.to_string(),
                Some((format!("{income:+}"), if income >= 0 { GAIN } else { LOSS })),
            );
            if !narrow {
                stat(
                    p,
                    assets,
                    prefix,
                    "flask",
                    faction.research.to_string(),
                    Some((
                        (session.rules.research_cost * (faction.technology as i32 + 1)).to_string(),
                        ON_PLATE.with_alpha(0.7),
                    )),
                );
                stat(
                    p,
                    assets,
                    prefix,
                    "ingots",
                    faction.industry.to_string(),
                    None,
                );
            }
        });
    });
    // Top right: the calendar and the one button that moves it on, joined by a compass.
    let mut right = top_card(Side::Right);
    right.align_items = AlignItems::Center;
    commands
        .spawn((right, face(Color::NONE)))
        .with_children(|p| {
            p.spawn(Node {
                padding: UiRect::axes(px(26), px(12)),
                min_height: px(48),
                align_items: AlignItems::Center,
                ..default()
            })
            .insert((BackgroundColor(CARD), lift_small()))
            .with_children(|d| {
                dress(d, assets, prefix, Skin::Paper, Frame::Small, false);
                bold(d, date_text(date), 18.0, INK);
            });
            p.spawn((
                ImageNode::new(crate::chrome::picture(assets, prefix, "compass")),
                Node {
                    width: px(38),
                    height: px(38),
                    margin: UiRect::horizontal(px(-19)),
                    flex_shrink: 0.0,
                    ..default()
                },
                ZIndex(2),
            ));
            p.spawn((
                Button,
                Action::EndTurn,
                Tone::Primary,
                Node {
                    min_width: px(132),
                    min_height: px(48),
                    padding: UiRect::axes(px(28), px(10)),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                BackgroundColor(Tone::Primary.fill(Interaction::None)),
                lift_small(),
            ))
            .with_children(|b| {
                texture(b, assets, prefix, Skin::Felt, 0.4);
                trim(b, assets, prefix, Frame::Small);
                bold(
                    b,
                    if session.pending {
                        "Waiting..."
                    } else {
                        "End Day"
                    },
                    19.0,
                    ON_ACCENT,
                );
            });
        });
    if narrow {
        // The inspector sits under the map: what the selection is, then quick ways to pick
        // another city or unit. Its dressing stays put while the contents scroll.
        let mut inspector = card();
        inspector.left = px(8);
        inspector.right = px(8);
        inspector.bottom = px(44);
        inspector.height = px(inspector_height(size.y));
        inspector.padding = UiRect::axes(px(18), px(14));
        commands
            .spawn((inspector, floating(), UiRoot))
            .with_children(|p| {
                dress(p, assets, prefix, Skin::Paper, Frame::Grand, false);
                p.spawn((
                    Node {
                        width: percent(100),
                        height: percent(100),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(4),
                        overflow: Overflow::scroll_y(),
                        ..default()
                    },
                    Inspector,
                    ScrollPosition::default(),
                ))
                .with_children(|p| {
                    info(p, game, session, assets);
                    p.spawn(Node {
                        flex_direction: FlexDirection::Row,
                        column_gap: px(6),
                        row_gap: px(6),
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
                            button(p, &c.name, Action::SelectCity(c.id), Tone::Quiet);
                        }
                        for u in game
                            .units
                            .values()
                            .filter(|u| u.owner == session.player)
                            .take(4)
                        {
                            button(
                                p,
                                session.rules.def(u).name.clone(),
                                Action::SelectUnit(u.id),
                                Tone::Quiet,
                            );
                        }
                        button(p, "Save", Action::Save, Tone::Quiet);
                    });
                });
            });
    } else {
        // The selection is described at the bottom right, over the map.
        let mut pane = card();
        pane.right = px(14);
        pane.bottom = px(CHART_BOTTOM);
        pane.width = px(INFO_WIDTH);
        pane.max_height = px((size.y - TOP_BAR - 12.0 - CHART_BOTTOM).max(120.0));
        pane.overflow = Overflow::clip();
        if has_info(game, session) {
            commands
                .spawn((pane, floating(), UiRoot, InfoPane, Interaction::default()))
                .with_children(|p| {
                    dress(p, assets, prefix, Skin::Paper, Frame::Grand, true);
                    info(p, game, session, assets);
                });
        }
    }
    if !narrow {
        // The minimap is the map's own shape, set in a slate frame.
        let image = chart_size(size.y, &game.map);
        let mut chart = card();
        chart.left = px(CHART_LEFT);
        chart.bottom = px(CHART_BOTTOM);
        chart.width = px(image.x + 2.0 * CHART_FRAME);
        chart.height = px(image.y + 2.0 * CHART_FRAME);
        chart.padding = UiRect::all(px(CHART_FRAME));
        commands
            .spawn((
                chart,
                BackgroundColor(SLATE),
                lift(),
                UiRoot,
                Interaction::default(),
            ))
            .with_children(|p| {
                dress(p, assets, prefix, Skin::Slate, Frame::Grand, false);
                p.spawn((
                    Node {
                        width: px(image.x),
                        height: px(image.y),
                        overflow: Overflow::clip(),
                        border: UiRect::all(px(1)),
                        ..default()
                    },
                    BorderColor::all(Color::BLACK.with_alpha(0.7)),
                ))
                .with_children(|p| {
                    p.spawn((
                        Button,
                        MinimapSurface,
                        RelativeCursorPosition::default(),
                        ImageNode::new(minimap.image.clone()),
                        Node {
                            width: px(image.x),
                            height: px(image.y),
                            ..default()
                        },
                    ))
                    .with_children(|surface| {
                        for i in 0..2 {
                            surface.spawn((
                                MinimapView(i),
                                Node {
                                    position_type: PositionType::Absolute,
                                    border: UiRect::all(px(2)),
                                    border_radius: rounded(1.0),
                                    ..default()
                                },
                                BorderColor::all(SELECT),
                            ));
                        }
                    });
                });
            });
    }
    // The selected unit's orders float over the foot of the map, in the space the minimap and
    // the selection's pane leave between them (the inspector is above the bar on a narrow screen).
    if let Selection::Unit(id) = session.selection
        && let Some(unit) = game.units.get(&id)
    {
        let place = if narrow {
            crate::bar::Placement {
                left: 8.0,
                right: 8.0,
                bottom: 44.0 + inspector_height(size.y) + 8.0,
            }
        } else {
            let chart = chart_size(size.y, &game.map);
            crate::bar::Placement {
                left: CHART_LEFT + chart.x + 2.0 * CHART_FRAME + 16.0,
                right: 16.0 + INFO_WIDTH + 12.0,
                bottom: CHART_BOTTOM + MESSAGE_HEIGHT + 10.0,
            }
        };
        let rows = crate::bar::rows(game, &session.rules, unit, session.player);
        crate::bar::spawn(commands, assets, &session.asset_prefix, rows, place);
    }
    // What the game has to say, in a dark pill at the foot of the map between the minimap and
    // the pane.
    let (left, right, bottom) = if narrow {
        (8.0, 8.0, 6.0)
    } else {
        let chart = chart_size(size.y, &game.map);
        (
            CHART_LEFT + chart.x + 2.0 * CHART_FRAME + 16.0,
            16.0 + INFO_WIDTH + 16.0,
            CHART_BOTTOM,
        )
    };
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(left),
                right: px(right),
                bottom: px(bottom),
                justify_content: JustifyContent::Center,
                ..default()
            },
            UiRoot,
        ))
        .with_children(|p| {
            p.spawn((
                Node {
                    min_height: px(MESSAGE_HEIGHT),
                    padding: UiRect::axes(px(18), px(5)),
                    align_items: AlignItems::Center,
                    border: UiRect::all(px(1.5)),
                    border_radius: rounded(4.0),
                    ..default()
                },
                BackgroundColor(PLATE),
                BorderColor::all(TRIM.with_alpha(0.55)),
                lift_small(),
            ))
            .with_children(|p| {
                text(
                    p,
                    &session.message,
                    if narrow { 11.5 } else { 12.5 },
                    ON_PLATE,
                );
            });
        });
    if let Selection::City(id) = session.selection
        && game.cities.contains_key(&id)
    {
        crate::city::spawn(commands, assets, session, game, id, narrow);
    }
    if let Some(nation) = session.diplomacy {
        crate::diplomacy::spawn(commands, assets, session, game, nation, narrow);
    }
    context_menu(commands, session, size);
    if let Some(city) = session.production {
        crate::production::spawn(commands, assets, session, game, city, narrow);
    }
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
                BackgroundColor(Color::srgba(0.03, 0.06, 0.10, 0.55)),
                GlobalZIndex(60),
                UiRoot,
            ))
            .with_children(|p| {
                let mut dialog = card();
                dialog.position_type = PositionType::Relative;
                dialog.width = percent(70);
                dialog.max_width = px(560);
                dialog.padding = UiRect::all(px(40));
                dialog.row_gap = px(14);
                p.spawn((dialog, floating())).with_children(|p| {
                    dress(p, assets, prefix, Skin::Paper, Frame::Grand, true);
                    bold(
                        p,
                        if winner == session.player {
                            "The campaign is won"
                        } else {
                            "The campaign is lost"
                        },
                        26.0,
                        INK,
                    );
                    text(
                        p,
                        format!(
                            "{} won the campaign on {}.",
                            game.factions[&winner].name,
                            long_date(date)
                        ),
                        15.0,
                        INK_SOFT,
                    );
                    p.spawn(Node {
                        margin: UiRect::top(px(6)),
                        ..default()
                    })
                    .with_children(|p| {
                        button(p, "Save this campaign", Action::Save, Tone::Primary);
                    });
                });
            });
    }
}

/// One row of the right-click menu: a labelled button across the menu's width. It carries
/// the menu's own mark, so the pointer resting on a row still counts as resting on the menu:
/// a button captures the hover (see `FocusPolicy`), hiding the card under it from the map.
fn menu_button(parent: &mut ChildSpawnerCommands, title: String, action: Action, tone: Tone) {
    parent
        .spawn((
            Button,
            action,
            tone,
            ContextMenuPane,
            Node {
                width: percent(100),
                min_height: px(32),
                flex_shrink: 0.0,
                padding: UiRect::axes(px(12), px(6)),
                border_radius: rounded(10.0),
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(tone.fill(Interaction::None)),
        ))
        .with_children(|p| bold(p, title, 13.0, tone.ink()));
}

/// Width of the right-click menu, and how far from the window's edges it stays.
const MENU_WIDTH: f32 = 280.0;
const MENU_MARGIN: f32 = 12.0;

/// The right-click menu over the map: the order a selected unit could take there, what a city
/// there offers, and who stands there. A city and a stack share one menu when a unit stands
/// in a city. Entries that name nothing still on the map are left out.
fn context_menu(commands: &mut Commands, session: &Session, size: Vec2) {
    let Some(menu) = &session.menu else {
        return;
    };
    let Some(game) = &session.game else {
        return;
    };
    let city = menu.city.and_then(|id| game.cities.get(&id));
    let units: Vec<&Unit> = menu
        .units
        .iter()
        .filter_map(|id| game.units.get(id))
        .collect();
    if city.is_none() && units.is_empty() {
        return;
    }
    let left = menu
        .at
        .x
        .min((size.x - MENU_WIDTH - MENU_MARGIN).max(MENU_MARGIN));
    let top = menu.at.y.min((size.y - MENU_MARGIN * 2.0).max(MENU_MARGIN));
    let mut face = card();
    face.left = px(left.max(MENU_MARGIN));
    face.top = px(top.max(MENU_MARGIN));
    face.width = px(MENU_WIDTH);
    face.max_height = px((size.y - top - MENU_MARGIN).max(120.0));
    face.overflow = Overflow::scroll_y();
    face.align_items = AlignItems::Stretch;
    commands
        .spawn((
            face,
            floating(),
            GlobalZIndex(30),
            UiRoot,
            ContextMenuPane,
            Interaction::default(),
            // The wheel scrolls a long menu instead of zooming the map under it (see
            // `controls`).
            ScrollPosition::default(),
        ))
        .with_children(|p| {
            // The selected unit's order for the square stays one click away, so striking an
            // enemy there still goes through the right button.
            if let Selection::Unit(id) = session.selection
                && let Some(command) = order_for(session, id, menu.tile)
            {
                let label = match &command {
                    Command::Move { destination, .. } => {
                        format!("March to {},{}", destination.x, destination.y)
                    }
                    Command::Attack { target, .. } => {
                        format!("Attack {},{}", target.x, target.y)
                    }
                    Command::Bombard { target, .. } => {
                        format!("Bombard {},{}", target.x, target.y)
                    }
                    _ => "Order this unit here".to_string(),
                };
                menu_button(p, label, Action::Strike(menu.tile), Tone::Primary);
            }
            if let Some(c) = city {
                caption(p, "city");
                bold(p, c.name.clone(), 16.0, INK);
                text(
                    p,
                    game.factions
                        .get(&c.owner)
                        .map_or("Unknown".to_string(), |f| f.adjective.clone()),
                    12.0,
                    INK_SOFT,
                );
                menu_button(
                    p,
                    format!("Open {}", c.name),
                    Action::SelectCity(c.id),
                    Tone::Quiet,
                );
                menu_button(
                    p,
                    format!("Zoom to {}", c.name),
                    Action::ZoomCity(c.id),
                    Tone::Quiet,
                );
                if c.owner == session.player {
                    menu_button(
                        p,
                        "Change production".to_string(),
                        Action::OpenProduction(c.id),
                        Tone::Quiet,
                    );
                }
            }
            if !units.is_empty() {
                if city.is_some() {
                    rule(p);
                }
                caption(
                    p,
                    if units.len() == 1 {
                        "unit here"
                    } else {
                        "units here"
                    },
                );
                for u in units {
                    let def = session.rules.def(u);
                    let owner = game
                        .factions
                        .get(&u.owner)
                        .map_or("Unknown".to_string(), |f| f.adjective.clone());
                    menu_button(
                        p,
                        if u.owner == session.player {
                            format!(
                                "{} · {} · {} moves",
                                def.name,
                                owner,
                                moves_text(u.moves_left(def))
                            )
                        } else {
                            format!("{} · {}", def.name, owner)
                        },
                        Action::SelectUnit(u.id),
                        Tone::Quiet,
                    );
                }
            }
            rule(p);
            menu_button(p, "Close".to_string(), Action::CloseMenu, Tone::Quiet);
        });
}

/// Scroll a long right-click menu with the wheel when the pointer is over it. Bevy itself
/// does not scroll; every scrolling view drives its own (see `city::scroll`).
pub(super) fn scroll_menu(
    mut wheel: MessageReader<MouseWheel>,
    hovered: Query<&Interaction, With<ContextMenuPane>>,
    mut scrolled: Query<(&ComputedNode, &mut ScrollPosition), With<ContextMenuPane>>,
) {
    let delta: f32 = wheel.read().map(|e| -e.y * 30.0).sum();
    if delta == 0.0 || hovered.iter().all(|i| *i == Interaction::None) {
        return;
    }
    for (node, mut scroll) in &mut scrolled {
        let max = (node.content_size().y - node.size().y) * node.inverse_scale_factor();
        scroll.y = (scroll.y + delta).clamp(0.0, max.max(0.0));
    }
}

/// Margin of the minimap card from the left edge and of both cards from the foot of the window.
const CHART_LEFT: f32 = 12.0;
const CHART_BOTTOM: f32 = 12.0;
/// The frame's rim and padding around the minimap picture.
const CHART_FRAME: f32 = 14.0;
/// Width of the pane that describes the selection, and height of the pill that carries the
/// game's message (map clicks under both are blocked, see `controls`).
const INFO_WIDTH: f32 = 320.0;
pub(super) const MESSAGE_HEIGHT: f32 = 26.0;

/// Length of the minimap's longer side on screen for a window height.
pub(super) fn chart_side(window_height: f32) -> f32 {
    (window_height * 0.24).clamp(120.0, 220.0)
}

/// The minimap image on screen: the map's own proportions, its longer side `chart_side`.
pub(super) fn chart_size(window_height: f32, map: &Map) -> Vec2 {
    let shape = Chart::of(map);
    let (width, height) = (shape.width as f32, shape.height as f32);
    Vec2::new(width, height) * chart_side(window_height) / width.max(height)
}

/// A nation's territory is painted in its colour, over this much of the terrain's own.
const NATION_FILL: f32 = 0.92;
/// A nation's frontier is its colour pulled toward black by this much.
const NATION_EDGE: f32 = 0.5;

/// The minimap picture: one pixel per tile (see [`Chart`]), RGBA. Land held by a nation is
/// filled with the nation's colour, with a darker line along every frontier it shares with
/// another holder's land, and the player's own cities stand out. Tiles the player has not
/// explored stay dark.
fn minimap_pixels(game: &Game, player: Id) -> (u32, u32, Vec<u8>) {
    let chart = Chart::of(&game.map);
    let (width, height) = (chart.width, chart.height);
    let explored = game.factions.get(&player).map(|f| &f.explored);
    let known = |p: Coord| player == 0 || explored.is_some_and(|e| e.contains(p));
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
        let rgb = if !known(tile.position) {
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
                Some(owner) if tile.is_land() && tile.owner != 0 => {
                    let frontier = EDGE_NEIGHBORS.iter().any(|(dx, dy)| {
                        let beyond = Coord::new(tile.position.x + dx, tile.position.y + dy);
                        game.map
                            .get(beyond)
                            .is_some_and(|other| other.is_land() && other.owner != tile.owner)
                    });
                    if frontier {
                        mix(*owner, [0, 0, 0], NATION_EDGE)
                    } else {
                        mix(base, *owner, NATION_FILL)
                    }
                }
                _ => base,
            }
        };
        put(tile.position, rgb);
    }
    for city in game.cities.values().filter(|c| c.owner == player) {
        let (x, y) = chart.pixel(city.position);
        // A 3 x 3 blot of pixels, which on a lattice chart is the tile's own neighbours.
        for dy in -1..=1 {
            for dx in -1..=1 {
                put(chart.tile(x + dx, y + dy), MAP_OWN_CITY);
            }
        }
        put(city.position, MAP_OWN_CITY_CORE);
    }
    (width, height, data)
}

/// Redraw the minimap image whenever the game state changes.
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
    let (width, height, data) = minimap_pixels(game, session.player);
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
    mut marker: Query<(&MinimapView, &mut Node)>,
) {
    let Some(game) = &session.game else {
        return;
    };
    let (Ok(window), Ok((transform, projection))) = (windows.single(), camera.single()) else {
        return;
    };
    let parts = Chart::of(&game.map).view_parts(camera_rect(transform, projection, window));
    for (part, mut node) in &mut marker {
        let Some(&(lo, hi)) = parts.get(part.0) else {
            node.display = Display::None;
            continue;
        };
        node.display = Display::Flex;
        let ((x0, y0), (x1, y1)) = ((lo.x, lo.y), (hi.x, hi.y));
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
    mut route_gizmos: Gizmos<crate::route::RouteGizmos>,
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
                let Some(_) = game.map.get(Coord::new(x, y)).filter(|t| t.claim == city) else {
                    continue;
                };
                let (sx, sy) = Coord::new(x, y).screen();
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
                            SELECT,
                        );
                    }
                }
            }
        }
    }
    // The chosen square and the road it takes are white; what a blow would land on is red.
    let mark = SELECT;
    let strike = Color::srgb(0.93, 0.31, 0.27);
    let pos = match session.selection {
        Selection::Unit(id) => game.units.get(&id).map(|u| u.position),
        Selection::City(id) => game.cities.get(&id).map(|c| c.position),
        Selection::Tile(pos) => Some(pos),
        Selection::None => None,
    };
    if let Some(pos) = pos {
        for p in view.copies(&game.map, pos) {
            let (x, y) = p.screen();
            gizmos.linestrip_2d(diamond(Vec2::new(x, y)), mark);
        }
    }
    // A selected unit shows where it is heading and what it could strike from here.
    if let Selection::Unit(id) = session.selection
        && let Some(unit) = game.units.get(&id).filter(|u| u.owner == session.player)
    {
        // Where it is heading: the exact squares it will walk (see `route`).
        if let Some(goal) = unit.goto {
            crate::route::draw(
                &mut route_gizmos,
                &game.map,
                view,
                unit.position,
                goal,
                &unit.route,
                mark,
            );
        }
        for (target, bombard) in strike_targets(game, &session.rules, unit) {
            for p in view.copies(&game.map, target) {
                let (x, y) = p.screen();
                let colour = if bombard { AMBER } else { strike };
                gizmos.linestrip_2d(diamond(Vec2::new(x, y)), colour);
                gizmos.circle_2d(Vec2::new(x, y), 16.0, colour);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn starter() -> Game {
        fourx_runtime::Host::base_scenario("russo-japanese-war", 42)
            .expect("the starter scenario starts")
            .game
    }

    #[test]
    fn fog_matches_on_both_images_of_the_seam() {
        let mut game = starter();
        let lattice = game.map.lattice.unwrap();
        let city = game
            .cities
            .values_mut()
            .find(|c| c.owner == game.commander)
            .unwrap();
        city.position = lattice.cell(0, 4);
        let west = city.position;
        let east = game.map.normalize(west.offset(-1, 1)).unwrap();
        let sight = Sight::over(
            &game,
            game.commander,
            (
                -lattice.columns,
                -lattice.columns,
                2 * game.map.width,
                2 * game.map.height,
            ),
        );
        assert_eq!(sight.state(east), IN_SIGHT);
        assert_eq!(sight.state(west.offset(-1, 1)), IN_SIGHT);
        assert_eq!(sight.fog_cell(east), sight.fog_cell(west.offset(-1, 1)));
    }

    #[test]
    fn the_minimap_panel_has_the_maps_proportions_and_fits_its_longer_side() {
        let game = starter();
        let chart = Chart::of(&game.map);
        for height in [400.0, 900.0, 2000.0] {
            let size = chart_size(height, &game.map);
            assert!((size.x.max(size.y) - chart_side(height)).abs() < 1e-3);
            let (w, h) = (chart.width as f32, chart.height as f32);
            assert!((size.x / size.y - w / h).abs() < 1e-3);
        }
        assert!(
            chart_side(900.0) < 266.0,
            "smaller than the old square panel"
        );
    }

    #[test]
    fn the_minimap_fills_a_nations_land_and_darkens_its_frontier() {
        let game = starter();
        let player = game.commander;
        let (width, _, painted) = minimap_pixels(&game, player);
        let mut bare = game.clone();
        for tile in &mut bare.map.tiles {
            tile.owner = 0;
        }
        let (_, _, unowned) = minimap_pixels(&bare, player);
        let chart = Chart::of(&game.map);
        let at = |data: &[u8], p: Coord| {
            let (x, y) = chart.pixel(p);
            let i = ((y as u32 * width + x as u32) * 4) as usize;
            [data[i], data[i + 1], data[i + 2]]
        };
        // The player's cities are drawn over the territory, as a blot of pixels.
        let covered: HashSet<(i32, i32)> = game
            .cities
            .values()
            .filter(|c| c.owner == player)
            .flat_map(|c| {
                let (x, y) = chart.pixel(c.position);
                (-1..=1).flat_map(move |dy| (-1..=1).map(move |dx| (x + dx, y + dy)))
            })
            .collect();
        let mut filled = 0;
        let mut frontier = 0;
        for tile in game
            .map
            .tiles
            .iter()
            .filter(|t| game.map.contains(t.position))
        {
            let (a, b) = (at(&painted, tile.position), at(&unowned, tile.position));
            if covered.contains(&chart.pixel(tile.position)) || b == MAP_FOG {
                continue;
            }
            if tile.owner == 0 || !tile.is_land() {
                assert_eq!(a, b, "{:?} is not a nation's land", tile.position);
                continue;
            }
            let ink = nation_rgb(&game, tile.owner);
            let edge = mix(ink, [0, 0, 0], NATION_EDGE);
            if a == edge {
                frontier += 1;
            } else {
                assert_eq!(a, mix(b, ink, NATION_FILL), "{:?}", tile.position);
                filled += 1;
            }
        }
        assert!(filled > 0, "some land is filled with its nation's colour");
        assert!(frontier > 0, "each nation's land has a frontier line");
    }

    #[test]
    fn only_the_players_own_cities_are_picked_out() {
        let game = starter();
        let player = game.commander;
        let (width, _, data) = minimap_pixels(&game, player);
        let chart = Chart::of(&game.map);
        for city in game.cities.values() {
            let (x, y) = chart.pixel(city.position);
            let i = ((y as u32 * width + x as u32) * 4) as usize;
            let core = [data[i], data[i + 1], data[i + 2]];
            assert_eq!(
                core == MAP_OWN_CITY_CORE,
                city.owner == player,
                "{}",
                city.name
            );
        }
    }

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
        // The cover is part of the relief surface, not a second floating sprite.
        assert_eq!(
            overlay_names(&tile(Grass, Hills, Forest)),
            (Some("hills_forest"), None)
        );
    }

    #[test]
    fn every_relief_context_resolves_to_matching_art_and_surface_borders() {
        let pack = fourx_content::Pack::base();
        let mut names = HashSet::new();
        for ground in [Terrain::Grass, Terrain::Desert, Terrain::Tundra] {
            for relief in [Relief::Hills, Relief::Mountains] {
                for cover in [Cover::Bare, Cover::Forest, Cover::Jungle] {
                    let tile = tile(ground, relief, cover);
                    assert!(tile.layers_valid());
                    let (name, separate_cover) = resolved_overlay_names(&tile, &pack.visuals);
                    let name = name.unwrap();
                    assert!(separate_cover.is_none(), "{name}");
                    assert!(pack.visuals.relief_borders.contains_key(name), "{name}");
                    assert!(name == "mountain" || pack.visuals.overlays.contains_key(name));
                    names.insert(name);
                }
            }
        }
        assert_eq!(names.len(), 18);
    }

    #[test]
    fn older_packs_keep_their_cover_and_use_borders_for_the_actual_relief() {
        let mut visuals = fourx_content::Pack::base().visuals;
        visuals.overlays.remove("mountain_cold_forest");
        let tile = tile(Terrain::Tundra, Relief::Mountains, Cover::Forest);
        assert_eq!(
            resolved_overlay_names(&tile, &visuals),
            (Some("mountain_cold"), Some("pine"))
        );
        visuals.overlays.remove("mountain_cold");
        assert_eq!(
            resolved_overlay_names(&tile, &visuals),
            (Some("mountain"), Some("pine"))
        );
        visuals.relief_borders.clear();
        assert!(visuals.relief_borders.is_empty());
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
        let host = fourx_runtime::Host::base_scenario("russo-japanese-war", 1).unwrap();
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
        assert!(depth(-64.0) - depth(-32.0) > Z_STEP * 3.0);
        assert!(Z_RELIEF + depth(-32.0) > Z_BORDER + depth(0.0));
        const { assert!(Z_BORDER + 1.0 < Z_FOG && Z_FOG < Z_CITY) };
        const { assert!(Z_CITY + 1.0 < Z_UNIT) };
    }

    #[test]
    fn pieces_stand_half_again_as_tall_as_before() {
        let host = fourx_runtime::Host::base_scenario("russo-japanese-war", 42)
            .expect("the starter scenario starts");
        assert_eq!(unit_size(&host.rules.units["infantry"]), Vec2::splat(105.0));
        assert_eq!(
            unit_size(&host.rules.units["ironclad"]),
            Vec2::new(216.0, 162.0)
        );
    }

    #[test]
    fn the_plate_counts_down_the_current_work() {
        assert_eq!(builds_in(Some(30), 0, 10), "3");
        assert_eq!(builds_in(Some(10), 9, 5), "1");
        assert_eq!(builds_in(None, 0, 10), "-");
        assert_eq!(builds_in(Some(30), 0, 0), "-");
    }

    #[test]
    fn vertical_pips_fit_their_track() {
        for (hp, max) in [(4, 4), (2, 4), (3, 3)] {
            for pip in pip_layout(hp, max, 42.0) {
                assert!(
                    pip.centre.abs() + pip.width / 2.0 <= 21.0 + 1e-3,
                    "{hp}/{max}"
                );
            }
        }
    }

    #[test]
    fn the_chart_stands_a_lattice_map_upright() {
        let host = fourx_runtime::Host::base_scenario("russo-japanese-war", 1).unwrap();
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
