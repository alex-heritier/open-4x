//! Cities: founding, worked tiles, food and shields, growth, production,
//! map visuals, and the city screen.

use bevy::asset::RenderAssetUsages;
use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::sprite::Anchor;
use bevy::text::TextLayoutInfo;
use bevy::window::PrimaryWindow;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs;

use crate::audio::{self, GameAudio};
use crate::features::{post, MessageBoard};
use crate::improvements::{can_irrigate, tile_overlays, ImprovementArt};
use crate::map::*;
use crate::render::{sprite_z, RevealAll};
use crate::splash::SplashUp;
use crate::tiles::TileArt;
use crate::units::{self, Selected, TurnEnded, Unit, UnitArt, UnitType};

pub const FOOD_BOX: u8 = 20;
pub const CITY_NAMES: [&str; 20] = [
    "Kyoto", "Osaka", "Tokyo", "Edo", "Nagoya", "Kobe", "Yokohama",
    "Hiroshima", "Nagasaki", "Nara", "Sapporo", "Sendai", "Niigata",
    "Okayama", "Fukuoka", "Kagoshima", "Matsuyama", "Kanazawa",
    "Takamatsu", "Oita",
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Production {
    Warrior,
    Scout,
    Settler,
    Worker,
    Barracks,
    Granary,
    Temple,
}

impl Production {
    pub const ALL: [Production; 7] = [
        Production::Warrior,
        Production::Scout,
        Production::Settler,
        Production::Worker,
        Production::Barracks,
        Production::Granary,
        Production::Temple,
    ];

    pub fn cost(self) -> u8 {
        match self {
            Production::Warrior => 10,
            Production::Scout => 20,
            Production::Settler => 30,
            Production::Worker => 10,
            Production::Barracks => 30,
            Production::Granary => 40,
            Production::Temple => 30,
        }
    }

    pub fn unit(self) -> Option<UnitType> {
        match self {
            Production::Warrior => Some(UnitType::Warrior),
            Production::Scout => Some(UnitType::Scout),
            Production::Settler => Some(UnitType::Settler),
            Production::Worker => Some(UnitType::Worker),
            _ => None,
        }
    }

    pub fn is_building(self) -> bool {
        self.unit().is_none()
    }

    pub fn name(self) -> &'static str {
        match self {
            Production::Warrior => "Warrior",
            Production::Scout => "Scout",
            Production::Settler => "Settler",
            Production::Worker => "Worker",
            Production::Barracks => "Barracks",
            Production::Granary => "Granary",
            Production::Temple => "Temple",
        }
    }

    /// One-line effect blurb for the build list.
    pub fn blurb(self) -> &'static str {
        match self {
            Production::Warrior => "Attack 1, Defense 1",
            Production::Scout => "Moves 2, sees 2",
            Production::Settler => "Founds a city (costs 2 pop)",
            Production::Worker => "Builds improvements",
            Production::Barracks => "Trains veteran land units",
            Production::Granary => "Keeps half the food box on growth",
            Production::Temple => "Adds culture; 1 upkeep",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Production::Warrior => "gen/ui/uniticon_warrior.png",
            Production::Scout => "gen/ui/uniticon_scout.png",
            Production::Settler => "gen/ui/uniticon_settler.png",
            Production::Worker => "gen/ui/uniticon_worker.png",
            _ => "",
        }
    }

    /// Row of `buildings-small.png`: 32-px icons on a 33-px grid (1-px green
    /// lines), with a label column and a header row; row 0 is the Palace.
    pub fn building_row(self) -> Option<u32> {
        match self {
            Production::Barracks => Some(1),
            Production::Granary => Some(2),
            Production::Temple => Some(3),
            _ => None,
        }
    }

    /// Sprite rect inside `buildings-small.png`: the ancient-era column.
    pub fn building_rect(self) -> Option<Rect> {
        let y = 33.0 + self.building_row()? as f32 * 33.0;
        Some(Rect::new(33.0, y, 65.0, y + 32.0))
    }
}

#[derive(Component, Clone)]
pub struct City {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub size: u8,
    pub food: u8,
    pub shields: u8,
    pub production: Production,
    /// Items queued after the current build.
    pub queue: Vec<Production>,
    pub buildings: Vec<Production>,
    pub worked: HashSet<(i32, i32)>,
}

#[derive(Resource, Default)]
pub struct CityNamesUsed(pub usize);

#[derive(Resource, Default)]
pub struct CityView(pub Option<Entity>);

#[derive(Deserialize)]
struct CityEntry {
    file: String,
    size: [u32; 2],
    anchor: [i32; 2],
}

#[derive(Resource)]
pub struct CityArt {
    pub town: Handle<Image>,
    pub city: Handle<Image>,
    pub metro: Handle<Image>,
    pub anchor: Anchor,
}

impl CityArt {
    pub fn load(asset_server: &AssetServer) -> Self {
        let text = fs::read_to_string("assets/gen/cities/manifest.json")
            .expect("run from civ3-clone/ after tools/prep_assets.py");
        let raw: HashMap<String, CityEntry> =
            serde_json::from_str(&text).expect("cities manifest parses");
        let anchor = {
            let e = &raw["town"];
            Anchor(Vec2::new(
                e.anchor[0] as f32 / e.size[0] as f32 - 0.5,
                0.5 - e.anchor[1] as f32 / e.size[1] as f32,
            ))
        };
        Self {
            town: asset_server.load("gen/cities/town.png"),
            city: asset_server.load("gen/cities/city.png"),
            metro: asset_server.load("gen/cities/metro.png"),
            anchor,
        }
    }

    pub fn graphic(&self, size: u8) -> Handle<Image> {
        if size >= 13 {
            self.metro.clone()
        } else if size >= 7 {
            self.city.clone()
        } else {
            self.town.clone()
        }
    }
}

/// The 20 workable tiles around a city: 5x5 minus corners minus center.
pub fn radius_tiles(map: &GameMap, x: i32, y: i32) -> Vec<(i32, i32)> {
    let mut out = vec![];
    for dy in -2i32..=2 {
        for dx in -2i32..=2 {
            if dx.abs() == 2 && dy.abs() == 2 {
                continue;
            }
            if dx == 0 && dy == 0 {
                continue;
            }
            let ny = y + dy;
            if ny < 0 || ny >= map.h {
                continue;
            }
            out.push((map.wrap_x(x + dx), ny));
        }
    }
    out
}

/// Tiles a city cannot work: every city center plus the tiles other
/// cities already work (`me` is the asking city's center).
pub fn taken_tiles<'a>(
    cities: impl IntoIterator<Item = &'a City>,
    me: (i32, i32),
) -> HashSet<(i32, i32)> {
    let mut out = HashSet::new();
    for c in cities {
        out.insert((c.x, c.y));
        if (c.x, c.y) != me {
            out.extend(c.worked.iter().copied());
        }
    }
    out
}

/// A radius tile the city may work: explored and not taken by another city.
pub fn workable(map: &GameMap, taken: &HashSet<(i32, i32)>, tile: (i32, i32)) -> bool {
    !taken.contains(&tile) && map.get(tile.0, tile.1).is_some_and(|t| t.seen)
}

fn tile_rank(map: &GameMap, tile: &(i32, i32)) -> (u8, u8) {
    yields(&map.tiles[map.idx(tile.0, tile.1)])
}

/// Governor: work the best `size` tiles, food first then shields.
pub fn governor_assign(map: &GameMap, city: &mut City, taken: &HashSet<(i32, i32)>) {
    city.worked.clear();
    governor_fill(map, city, taken);
}

/// Match worked tiles to the population without touching the player's
/// other picks: add the best free tiles on growth, drop the worst when the
/// city shrinks. Citizens with no workable tile left stay idle
/// (entertainers).
pub fn governor_fill(map: &GameMap, city: &mut City, taken: &HashSet<(i32, i32)>) {
    while city.worked.len() > city.size as usize {
        let worst = city.worked.iter().min_by_key(|t| tile_rank(map, t)).copied();
        if let Some(w) = worst {
            city.worked.remove(&w);
        }
    }
    let mut free: Vec<(i32, i32)> = radius_tiles(map, city.x, city.y)
        .into_iter()
        .filter(|t| !city.worked.contains(t) && workable(map, taken, *t))
        .collect();
    free.sort_by_key(|t| {
        let (f, s) = tile_rank(map, t);
        (std::cmp::Reverse(f), std::cmp::Reverse(s))
    });
    let want = (city.size as usize).saturating_sub(city.worked.len());
    city.worked.extend(free.into_iter().take(want));
}

/// Toggle a worked tile. When every citizen is busy, the worst tile is
/// replaced. Returns false when the tile cannot be worked.
pub fn toggle_worked(
    map: &GameMap,
    city: &mut City,
    tile: (i32, i32),
    taken: &HashSet<(i32, i32)>,
) -> bool {
    if city.worked.remove(&tile) {
        return true;
    }
    if !workable(map, taken, tile) {
        return false;
    }
    if city.worked.len() >= city.size as usize {
        let worst = city.worked.iter().min_by_key(|t| tile_rank(map, t)).copied();
        if let Some(worst) = worst {
            city.worked.remove(&worst);
        }
    }
    city.worked.insert(tile);
    true
}

/// City center yield: the tile's own yield, irrigated for free when the
/// land could be irrigated, and at least one shield (Civ3's center rules).
pub fn center_yields(map: &GameMap, x: i32, y: i32) -> (u8, u8) {
    let mut t = map.tiles[map.idx(x, y)].clone();
    if can_irrigate(map, x, y) {
        t.irrigation = true;
        t.mine = false;
    }
    let (f, s) = yields(&t);
    (f, s.max(1))
}

/// Gross (food, shields) from the center plus worked tiles.
pub fn city_yields(map: &GameMap, city: &City) -> (u8, u8) {
    let (mut food, mut shields) = center_yields(map, city.x, city.y);
    for (x, y) in city.worked.iter() {
        let (f, s) = yields(&map.tiles[map.idx(*x, *y)]);
        food += f;
        shields += s;
    }
    (food, shields)
}

/// (net food after feeding 2 per pop, shields per turn).
pub fn city_income(map: &GameMap, city: &City) -> (i16, u8) {
    let (food, shields) = city_yields(map, city);
    (food as i16 - 2 * city.size as i16, shields)
}

/// Population a city must keep: a Settler (2 pop) needs size 3, since
/// Civ3 never lets a build empty a city.
pub const SETTLER_MIN_SIZE: u8 = 3;

pub enum CityEvent {
    Grew,
    Starved,
    Completed(UnitType),
    Built(Production),
    /// A finished Settler waits for the city to reach `SETTLER_MIN_SIZE`.
    TooSmall,
}

impl City {
    pub fn has(&self, p: Production) -> bool {
        self.buildings.contains(&p)
    }

    /// Items offered in the build list: every unit, plus buildings not
    /// yet owned.
    pub fn buildable(&self) -> Vec<Production> {
        Production::ALL
            .iter()
            .copied()
            .filter(|p| !(p.is_building() && self.has(*p)))
            .collect()
    }

    /// Change the current build. Switching between unit and building
    /// classes forfeits half the stored shields, as in Civ3.
    pub fn change_build(&mut self, p: Production) {
        if p == self.production {
            return;
        }
        if p.is_building() != self.production.is_building() {
            self.shields /= 2;
        }
        self.production = p;
    }

    /// Queue an item (buildings are queued at most once).
    pub fn enqueue(&mut self, p: Production) -> bool {
        if self.queue.len() >= 6
            || (p.is_building()
                && (self.has(p) || self.production == p || self.queue.contains(&p)))
        {
            return false;
        }
        self.queue.push(p);
        true
    }

    pub fn dequeue(&mut self, i: usize) {
        if i < self.queue.len() {
            self.queue.remove(i);
        }
    }

    /// After completing the current build: repeat units, or take the
    /// next queued item. Buildings with an empty queue fall back to a
    /// Warrior.
    fn advance_queue(&mut self, done: Production) {
        if !self.queue.is_empty() {
            self.production = self.queue.remove(0);
        } else if done.is_building() {
            self.production = Production::Warrior;
        }
    }
}

pub fn process_city_turn(
    map: &GameMap,
    city: &mut City,
    taken: &HashSet<(i32, i32)>,
) -> Vec<CityEvent> {
    let mut events = vec![];
    let (net_food, shields) = city_income(map, city);
    let food = city.food as i16 + net_food;
    if food >= FOOD_BOX as i16 {
        city.size += 1;
        city.food = if city.has(Production::Granary) {
            FOOD_BOX / 2
        } else {
            0
        };
        governor_fill(map, city, taken);
        events.push(CityEvent::Grew);
    } else if food < 0 {
        if city.size > 1 {
            city.size -= 1;
            governor_fill(map, city, taken);
            events.push(CityEvent::Starved);
        }
        city.food = 0;
    } else {
        city.food = food as u8;
    }
    let before = city.shields;
    city.shields = city.shields.saturating_add(shields);
    if city.shields >= city.production.cost() {
        let done = city.production;
        if done == Production::Settler && city.size < SETTLER_MIN_SIZE {
            // Held at full cost; say so once, when the box first fills.
            city.shields = done.cost();
            if before < done.cost() {
                events.push(CityEvent::TooSmall);
            }
            return events;
        }
        city.shields -= done.cost();
        if let Some(unit) = done.unit() {
            if done == Production::Settler {
                city.size -= 2;
                governor_fill(map, city, taken);
            }
            events.push(CityEvent::Completed(unit));
        } else {
            city.buildings.push(done);
            events.push(CityEvent::Built(done));
        }
        city.advance_queue(done);
    }
    events
}

pub fn can_found(map: &GameMap, cities: &[(i32, i32)], x: i32, y: i32) -> bool {
    let Some(t) = map.get(x, y) else {
        return false;
    };
    if !matches!(
        t.base,
        Base::Grassland | Base::Plains | Base::Desert | Base::Tundra
    ) || t.relief == Relief::Mountain
    {
        return false;
    }
    for (cx, cy) in cities {
        let dx = (x - cx).abs();
        let dx = dx.min(map.w - dx);
        if dx.max((y - cy).abs()) < 2 {
            return false;
        }
    }
    true
}

fn next_name(used: &mut CityNamesUsed) -> String {
    let i = used.0;
    used.0 += 1;
    let base = CITY_NAMES[i % CITY_NAMES.len()];
    if i < CITY_NAMES.len() {
        base.to_string()
    } else {
        format!("{base} {}", i / CITY_NAMES.len() + 1)
    }
}

#[derive(Component)]
pub(crate) struct CitySprite(pub Entity);

#[derive(Component)]
pub(crate) struct CityBanner(pub Entity);

#[derive(Component)]
pub(crate) struct CityBadgeText(pub Entity);

/// Dark backing bar behind the whole label. Children (fill, badge, text,
/// star) inherit its visibility, so fog only syncs this root.
#[derive(Component)]
pub(crate) struct CityLabelBack(pub Entity);

#[derive(Component)]
pub(crate) struct CityLabelFill(pub Entity);

/// Capital star badge, right of the text. Only the capital spawns one.
#[derive(Component)]
pub(crate) struct CityStar(pub Entity);

/// First founded city. It alone gets the star badge, as in Civ3.
#[derive(Resource, Default)]
pub struct Capital(pub Option<Entity>);

/// Gold capital star, rasterized at startup (no font glyph needed).
#[derive(Resource)]
pub struct CapitalStar(pub Handle<Image>);

impl CapitalStar {
    pub fn generate(images: &mut Assets<Image>) -> Self {
        const S: u32 = 48;
        let mut data = vec![0u8; (S * S * 4) as usize];
        let c = S as f32 / 2.0;
        // 5-point star, vertices alternating outer/inner radius.
        let vert: Vec<(f32, f32)> = (0..10)
            .map(|k| {
                let r = if k % 2 == 0 { 21.0 } else { 8.8 };
                let a = -std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::PI / 5.0;
                (c + r * a.cos(), c + r * a.sin())
            })
            .collect();
        let inside = |px: f32, py: f32| {
            let mut winding = false;
            for i in 0..10 {
                let (x0, y0) = vert[i];
                let (x1, y1) = vert[(i + 1) % 10];
                if (y0 > py) != (y1 > py)
                    && px < (x1 - x0) * (py - y0) / (y1 - y0) + x0
                {
                    winding = !winding;
                }
            }
            winding
        };
        // 2x supersample for smooth edges; gold with a dark outline so it
        // reads on the white badge.
        for y in 0..S {
            for x in 0..S {
                let mut gold = 0u32;
                let mut edge = 0u32;
                for (ox, oy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                    let (px, py) = (x as f32 + ox, y as f32 + oy);
                    if inside(px, py) {
                        gold += 1;
                    } else if inside(px * 0.92 + c * 0.08, py * 0.92 + c * 0.08) {
                        edge += 1;
                    }
                }
                let i = ((y * S + x) * 4) as usize;
                if gold > 0 {
                    let a = gold * 255 / 4;
                    data[i..i + 4].copy_from_slice(&[0xE8, 0xB8, 0x20, a as u8]);
                } else if edge > 0 {
                    let a = edge * 255 / 4;
                    data[i..i + 4].copy_from_slice(&[0x2A, 0x1A, 0x08, a as u8]);
                }
            }
        }
        let image = Image::new(
            Extent3d { width: S, height: S, depth_or_array_layers: 1 },
            TextureDimension::D2,
            data,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        Self(images.add(image))
    }
}

/// Japan's map color. Civ3 paints each civ's badge in its own color; the
/// mood tints on the number (yellow/red) need the happiness model.
pub const CIV_BADGE: Color = Color::WHITE;
pub const CIV_BADGE_INK: Color = Color::BLACK;
/// Dark translucent backing bar, with a dim gold frame.
pub const LABEL_FILL: Color = Color::srgba(0.07, 0.05, 0.03, 0.85);
pub const LABEL_FRAME: Color = Color::srgba(0.72, 0.65, 0.48, 0.9);
const BADGE_W: f32 = 30.0;
const BADGE_H: f32 = 44.0;
const STAR_W: f32 = 30.0;
/// Padding inside the bar, and the gaps badge-text and text-star.
const LABEL_PAD: f32 = 3.0;
const LABEL_GAP: f32 = 5.0;

fn ceil_div(a: u8, b: u8) -> u8 {
    (a + b - 1) / b
}

/// Map label, Civ3's two lines: the city name with turns to grow, and the
/// current build with turns left. A stalled line shows `--`, as in Civ3.
fn banner_text(map: &GameMap, city: &City) -> String {
    let (net_food, shields) = city_income(map, city);
    let name = if net_food > 0 {
        let left = FOOD_BOX.saturating_sub(city.food);
        format!("{} : {}", city.name, ceil_div(left, net_food as u8))
    } else {
        format!("{} : --", city.name)
    };
    let prod = if shields == 0 {
        format!("{} : --", city.production.name())
    } else {
        let left = city.production.cost().saturating_sub(city.shields);
        format!("{} : {}", city.production.name(), ceil_div(left, shields))
    };
    format!("{name}\n{prod}")
}

/// Left edge of the label bar in world units. The bar grows rightward
/// with the text; the badge end stays put, roughly centering short names
/// over the city.
fn label_origin(city: &City) -> Vec2 {
    let pos = tile_to_world(city.x, city.y);
    // Just above the town art, which rises ~31 px over the tile center.
    Vec2::new(pos.x - 55.0, pos.y + 50.0)
}

pub fn spawn_city_visuals(
    commands: &mut Commands,
    art: &CityArt,
    assets: &AssetServer,
    star: &CapitalStar,
    map: &GameMap,
    entity: Entity,
    city: &City,
    is_capital: bool,
) {
    // Visuals are roots carrying the city entity, like TileSprite and Unit.
    // Cities never move, so nothing needs syncing after spawn.
    let pos = tile_to_world(city.x, city.y);
    commands.spawn((
        Sprite {
            image: art.graphic(city.size),
            ..default()
        },
        art.anchor.clone(),
        Transform::from_xyz(pos.x, pos.y, sprite_z(city.x, city.y, 3.0)),
        CitySprite(entity),
    ));
    let font: Handle<Font> = assets.load("gen/fonts/lsans.ttf");
    let origin = label_origin(city);
    let z = sprite_z(city.x, city.y, 5.0);
    // Initial size from a character-count estimate; the sync sizes the bar
    // exactly from the laid-out text on the next frame.
    let est = estimate_text_width(&banner_text(map, city));
    let bar_w = label_width(est, is_capital);
    commands
        .spawn((
            Sprite {
                color: LABEL_FRAME,
                custom_size: Some(Vec2::new(bar_w, BADGE_H + LABEL_PAD * 2.0)),
                ..default()
            },
            Anchor(Vec2::new(-0.5, 0.0)),
            Transform::from_xyz(origin.x, origin.y, z),
            CityLabelBack(entity),
        ))
        .with_children(|bar| {
            bar.spawn((
                Sprite {
                    color: LABEL_FILL,
                    custom_size: Some(Vec2::new(bar_w - 2.0, BADGE_H + LABEL_PAD * 2.0 - 2.0)),
                    ..default()
                },
                Transform::from_xyz(0.0, 0.0, 0.01),
                CityLabelFill(entity),
            ));
            // Population badge on the bar's left end.
            bar.spawn((
                Sprite {
                    color: CIV_BADGE,
                    custom_size: Some(Vec2::new(BADGE_W, BADGE_H)),
                    ..default()
                },
                Transform::from_xyz(LABEL_PAD + BADGE_W / 2.0, 0.0, 0.02),
            ))
            .with_children(|badge| {
                badge.spawn((
                    Text2d::new(city.size.to_string()),
                    TextFont { font: font.clone(), font_size: 22.0, ..default() },
                    TextColor(CIV_BADGE_INK),
                    Transform::from_xyz(0.0, 0.0, 0.01),
                    CityBadgeText(entity),
                ));
            });
            bar.spawn((
                Text2d::new(banner_text(map, city)),
                TextFont { font: font.clone(), font_size: 17.0, ..default() },
                TextColor(Color::WHITE),
                TextLayout::new_with_justify(Justify::Left),
                Anchor(Vec2::new(-0.5, 0.0)),
                Transform::from_xyz(LABEL_PAD + BADGE_W + LABEL_GAP, 0.0, 0.02),
                CityBanner(entity),
            ));
            if is_capital {
                // Star badge on the bar's right end, past the text.
                bar.spawn((
                    Sprite {
                        color: CIV_BADGE,
                        custom_size: Some(Vec2::new(STAR_W, BADGE_H)),
                        ..default()
                    },
                    Transform::from_xyz(
                        LABEL_PAD + BADGE_W + LABEL_GAP + est + LABEL_GAP + STAR_W / 2.0,
                        0.0,
                        0.02,
                    ),
                    CityStar(entity),
                ))
                .with_children(|badge| {
                    badge.spawn((
                        Sprite {
                            image: star.0.clone(),
                            custom_size: Some(Vec2::new(26.0, 26.0)),
                            ..default()
                        },
                        Transform::from_xyz(0.0, 0.0, 0.01),
                    ));
                });
            }
        });
}

/// Width of the bar's content (inside padding): badge, text, and the
/// capital star when present.
fn label_width(text_w: f32, is_capital: bool) -> f32 {
    LABEL_PAD * 2.0
        + BADGE_W
        + LABEL_GAP
        + text_w
        + if is_capital { LABEL_GAP + STAR_W } else { 0.0 }
}

/// Fallback text width before the first layout lands: ~9 px per character
/// of the longest line at 17 px.
fn estimate_text_width(s: &str) -> f32 {
    s.lines().map(|l| l.chars().count()).max().unwrap_or(0) as f32 * 9.0
}

/// Found a city with the selected settler (B key).
pub fn found_city(
    mut commands: Commands,
    mut cmds: MessageReader<crate::actionbar::UnitCommand>,
    selected: Res<Selected>,
    units: Query<(Entity, &Unit)>,
    cities: Query<&City>,
    mut names: ResMut<CityNamesUsed>,
    mut map: ResMut<GameMap>,
    art: Res<CityArt>,
    assets: Res<AssetServer>,
    star: Res<CapitalStar>,
    mut capital: ResMut<Capital>,
    view: Res<CityView>,
    splash: Res<SplashUp>,
    audio: Res<GameAudio>,
) {
    let asked = cmds
        .read()
        .any(|c| *c == crate::actionbar::UnitCommand::FoundCity);
    if !asked || view.0.is_some() || splash.0 {
        return;
    }
    let Some(s) = selected.0 else {
        return;
    };
    let Ok((e, u)) = units.get(s) else {
        return;
    };
    if u.utype != UnitType::Settler {
        return;
    }
    let spots: Vec<(i32, i32)> = cities.iter().map(|c| (c.x, c.y)).collect();
    if !can_found(&map, &spots, u.x, u.y) {
        return;
    }
    let (x, y) = (u.x, u.y);
    commands.entity(e).despawn();
    // Settling absorbs any hut or camp on the tile (no reward); a resource
    // underneath stays for future trade.
    let i = map.idx(x, y);
    map.tiles[i].hut = false;
    map.tiles[i].camp = false;
    // Civ3 cities carry a road on their tile.
    map.tiles[i].road = true;
    let mut city = City {
        name: next_name(&mut names),
        x,
        y,
        size: 1,
        food: 0,
        shields: 0,
        production: Production::Warrior,
        queue: vec![],
        buildings: vec![],
        worked: HashSet::new(),
    };
    let taken = taken_tiles(cities.iter(), (x, y));
    governor_assign(&map, &mut city, &taken);
    let entity = commands.spawn(city.clone()).id();
    let is_capital = capital.0.is_none();
    if is_capital {
        capital.0 = Some(entity);
    }
    spawn_city_visuals(&mut commands, &art, &assets, &star, &map, entity, &city, is_capital);
    commands.spawn(AudioPlayer(audio.build.clone()));
}

pub fn end_turn_cities(
    mut commands: Commands,
    mut events: MessageReader<TurnEnded>,
    map: Res<GameMap>,
    mut cities: Query<(Entity, &mut City)>,
    art: Res<UnitArt>,
    audio: Res<GameAudio>,
    mut board: ResMut<MessageBoard>,
) {
    for _ in events.read() {
        let order: Vec<Entity> = cities.iter().map(|(e, _)| e).collect();
        for e in order {
            // Later cities see the tiles earlier ones claimed this turn.
            let me = cities.get(e).map(|(_, c)| (c.x, c.y)).unwrap();
            let taken = taken_tiles(cities.iter().map(|(_, c)| c), me);
            let Ok((_, mut city)) = cities.get_mut(e) else { continue };
            for event in process_city_turn(&map, &mut city, &taken) {
                match event {
                    CityEvent::Completed(unit) => {
                        units::spawn_unit(&mut commands, &art, unit, city.x, city.y);
                        audio::sfx(&mut commands, &audio, "WhatToBuild");
                        post(&mut board, format!("{} completes {}.", city.name, units::def(unit).name));
                    }
                    CityEvent::Built(p) => {
                        audio::sfx(&mut commands, &audio, "WhatToBuild");
                        post(&mut board, format!("{} completes {}.", city.name, p.name()));
                    }
                    CityEvent::TooSmall => post(
                        &mut board,
                        format!("{} must reach size {SETTLER_MIN_SIZE} to finish its Settler.", city.name),
                    ),
                    CityEvent::Grew => post(&mut board, format!("{} grows to size {}.", city.name, city.size)),
                    CityEvent::Starved => post(&mut board, format!("{} starves to size {}.", city.name, city.size)),
                }
            }
        }
    }
}

pub fn sync_city_visuals(
    map: Res<GameMap>,
    art: Res<CityArt>,
    cities: Query<&City>,
    capital: Res<Capital>,
    mut sprites: Query<
        (&CitySprite, &mut Sprite),
        (Without<CityLabelBack>, Without<CityLabelFill>, Without<CityStar>),
    >,
    mut banners: Query<(&CityBanner, &mut Text2d), Without<CityBadgeText>>,
    mut layouts: Query<(&CityBanner, &TextLayoutInfo)>,
    mut badge_texts: Query<(&CityBadgeText, &mut Text2d), Without<CityBanner>>,
    mut bars: Query<
        (&CityLabelBack, &mut Sprite, &mut Transform),
        (Without<CitySprite>, Without<CityStar>, Without<CityLabelFill>),
    >,
    mut fills: Query<
        (&CityLabelFill, &mut Sprite),
        (Without<CitySprite>, Without<CityLabelBack>, Without<CityStar>),
    >,
    mut stars: Query<
        (&CityStar, &mut Transform),
        (Without<CityLabelBack>, Without<CityLabelFill>, Without<CitySprite>),
    >,
) {
    for (link, mut sprite) in sprites.iter_mut() {
        if let Ok(city) = cities.get(link.0) {
            sprite.image = art.graphic(city.size);
        }
    }
    for (link, mut banner) in banners.iter_mut() {
        if let Ok(city) = cities.get(link.0) {
            banner.0 = banner_text(&map, city);
        }
    }
    for (link, mut text) in badge_texts.iter_mut() {
        if let Ok(city) = cities.get(link.0) {
            text.0 = city.size.to_string();
        }
    }
    // Size the bar from the laid-out text and slide the star past it.
    for (blink, mut bsprite, _) in bars.iter_mut() {
        let Ok(city) = cities.get(blink.0) else {
            continue;
        };
        let is_capital = capital.0 == Some(blink.0);
        let w = layouts
            .iter()
            .find(|(b, _)| b.0 == blink.0)
            .map(|(_, l)| l.size.x)
            .filter(|w| *w >= 1.0)
            .unwrap_or_else(|| estimate_text_width(&banner_text(&map, city)));
        let bar_w = label_width(w, is_capital);
        let bar_h = BADGE_H + LABEL_PAD * 2.0;
        bsprite.custom_size = Some(Vec2::new(bar_w, bar_h));
        if let Some((_, mut fsprite)) =
            fills.iter_mut().find(|(f, _)| f.0 == blink.0)
        {
            fsprite.custom_size =
                Some(Vec2::new((bar_w - 2.0).max(1.0), (bar_h - 2.0).max(1.0)));
        }
        if is_capital {
            if let Some((_, mut stf)) =
                stars.iter_mut().find(|(s, _)| s.0 == blink.0)
            {
                stf.translation.x =
                    LABEL_PAD + BADGE_W + LABEL_GAP + w + LABEL_GAP + STAR_W / 2.0;
            }
        }
    }
}

pub fn city_visibility(
    map: Res<GameMap>,
    reveal: Res<RevealAll>,
    cities: Query<&City>,
    mut sprites: Query<
        (&CitySprite, &mut Visibility, &mut Sprite),
        (Without<CityBanner>, Without<CityLabelBack>),
    >,
    // Bar roots carry the whole label (badge, text, star) as children, so
    // fog only syncs them; the children inherit.
    mut bars: Query<
        (&CityLabelBack, &mut Visibility),
        (Without<CitySprite>, Without<CityBanner>),
    >,
) {
    for (link, mut vis) in bars.iter_mut() {
        *vis = city_vis(&map, &cities, &reveal, link.0);
    }
    for (link, mut vis, mut sprite) in sprites.iter_mut() {
        *vis = city_vis(&map, &cities, &reveal, link.0);
        // City graphics sit above the fog diamonds, so they dim themselves
        // when the city is remembered but not currently visible.
        let lit = reveal.0
            || cities
                .get(link.0)
                .ok()
                .and_then(|c| map.get(c.x, c.y))
                .is_some_and(|t| t.visible);
        sprite.color = if lit {
            Color::WHITE
        } else {
                // City art sits above the fog diamonds, so it dims itself
                // to the same 60% a remembered tile keeps.
                Color::srgb(0.6, 0.6, 0.6)
        };
    }
}

fn city_vis(
    map: &GameMap,
    cities: &Query<&City>,
    reveal: &RevealAll,
    entity: Entity,
) -> Visibility {
    let show = reveal.0
        || cities
            .get(entity)
            .ok()
            .and_then(|c| map.get(c.x, c.y))
            .is_some_and(|t| t.seen);
    if show {
        Visibility::Visible
    } else {
        Visibility::Hidden
    }
}

// ---------------------------------------------------------------------------
// City screen
// ---------------------------------------------------------------------------

#[derive(Component)]
pub(crate) struct CityScreenRoot;

#[derive(Component)]
pub(crate) struct TileCluster;

#[derive(Component)]
pub(crate) enum ScreenButton {
    Close,
    Change,
    Governor,
    Pick(Production),
    Queue(Production),
    Unqueue(usize),
    CloseMenu,
}

/// Whether the build-list modal is open over the city screen.
#[derive(Resource, Default)]
pub struct BuildMenu(pub bool);

fn txt(
    parent: &mut ChildSpawnerCommands<'_>,
    font: &Handle<Font>,
    s: &str,
    size: f32,
    x: f32,
    y: f32,
    color: Color,
) {
    parent.spawn((
        Text::new(s),
        TextFont { font: font.clone(), font_size: size, ..default() },
        TextColor(color),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(x),
            top: Val::Px(y),
            ..default()
        },
    ));
}

/// Unit icon or cropped `buildings-small.png` cell for a build item.
fn build_icon(assets: &AssetServer, p: Production) -> ImageNode {
    match p.building_rect() {
        Some(rect) => {
            let mut n = ImageNode::new(assets.load("gen/cityscreen/buildings-small.png"));
            n.rect = Some(rect);
            n
        }
        None => ImageNode::new(assets.load(p.icon())),
    }
}

fn turns_for(cost: u8, have: u8, rate: u8) -> String {
    if rate == 0 {
        "never".into()
    } else {
        format!("{}t", ceil_div(cost.saturating_sub(have), rate))
    }
}

fn build_menu(
    content: &mut ChildSpawnerCommands<'_>,
    assets: &AssetServer,
    font: &Handle<Font>,
    city: &City,
    rate: u8,
) {
    let items = city.buildable();
    let ink = Color::srgb(0.95, 0.9, 0.7);
    content
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(212.0),
                top: Val::Px(100.0),
                width: Val::Px(600.0),
                height: Val::Px(54.0 + items.len() as f32 * 48.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.08, 0.05, 0.03, 0.96)),
        ))
        .with_children(|m| {
            txt(m, font, &format!("What should {} build?", city.name), 20.0, 16.0, 10.0, ink);
            m.spawn((
                Button,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(520.0), top: Val::Px(8.0),
                    padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.35, 0.22, 0.1)),
                ScreenButton::CloseMenu,
            ))
            .with_children(|b| {
                b.spawn((Text::new("Close"), TextFont { font: font.clone(), font_size: 16.0, ..default() }, TextColor(ink)));
            });
            for (n, p) in items.iter().enumerate() {
                let top = 44.0 + n as f32 * 48.0;
                let have = if *p == city.production { city.shields } else { 0 };
                m.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(12.0), top: Val::Px(top + 2.0),
                        width: Val::Px(42.0), height: Val::Px(42.0),
                        ..default()
                    },
                    build_icon(assets, *p),
                ));
                let mark = if *p == city.production { "  [building]" } else { "" };
                txt(m, font, &format!("{}{}   cost {}  ({})", p.name(), mark, p.cost(), turns_for(p.cost(), have, rate)), 17.0, 64.0, top + 2.0, ink);
                txt(m, font, p.blurb(), 13.0, 64.0, top + 24.0, Color::srgb(0.75, 0.7, 0.55));
                for (label, left, kind) in [
                    ("Build now", 400.0, ScreenButton::Pick(*p)),
                    ("Queue", 500.0, ScreenButton::Queue(*p)),
                ] {
                    m.spawn((
                        Button,
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(left), top: Val::Px(top + 6.0),
                            width: Val::Px(90.0), height: Val::Px(30.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.35, 0.22, 0.1)),
                        kind,
                    ))
                    .with_children(|b| {
                        b.spawn((Text::new(label), TextFont { font: font.clone(), font_size: 15.0, ..default() }, TextColor(ink)));
                    });
                }
            }
        });
}

const CONTENT_W: f32 = 1024.0;
const CONTENT_H: f32 = 768.0;
const CLUSTER_X: f32 = 192.0;
const CLUSTER_Y: f32 = 140.0;
const PARCHMENT_TEXT: Color = Color::srgb(0.23, 0.14, 0.06);

/// Which radius cell (rx, ry in -2..=2 fat cross) sits under a cursor
/// position in window pixels, if any.
pub fn cluster_pick(
    win_w: f32,
    win_h: f32,
    cursor: Vec2,
) -> Option<(i32, i32)> {
    let ox = (win_w - CONTENT_W) / 2.0;
    let oy = (win_h - CONTENT_H) / 2.0;
    let px = cursor.x - ox;
    let py = (win_h - cursor.y) - oy;
    let lx = px - CLUSTER_X - 320.0;
    let ly = py - CLUSTER_Y - 160.0;
    let a = lx / 64.0;
    let b = ly / 32.0;
    let rx = ((a + b) / 2.0).round() as i32;
    let ry = ((b - a) / 2.0).round() as i32;
    if rx.abs() > 2 || ry.abs() > 2 || (rx.abs() == 2 && ry.abs() == 2) {
        return None;
    }
    // verify inside the diamond (edges belong to neighbors)
    let cx = (rx - ry) as f32 * 64.0;
    let cy = (rx + ry) as f32 * 32.0;
    if (lx - cx).abs() / 64.0 + (ly - cy).abs() / 32.0 > 1.0 {
        return None;
    }
    Some((rx, ry))
}

/// Cell of `CityIcons.png`: 30-px icons on a 31-px stride (1-px green
/// separators). 2 = commerce, 4 = shield, 6 = food.
fn city_icon(assets: &AssetServer, cell: u32) -> ImageNode {
    let mut n = ImageNode::new(assets.load("gen/cityscreen/CityIcons.png"));
    let x = 1.0 + cell as f32 * 31.0;
    n.rect = Some(Rect::new(x, 1.0, x + 30.0, 31.0));
    n
}

const ICON_FOOD: u32 = 6;
const ICON_SHIELD: u32 = 4;

/// What the city screen needs to draw one radius tile.
struct ClusterCtx<'a> {
    assets: &'a AssetServer,
    tiles: &'a TileArt,
    imp: &'a ImprovementArt,
    city_art: &'a CityArt,
    map: &'a GameMap,
    city: &'a City,
    taken: &'a HashSet<(i32, i32)>,
    hubs: &'a [(i32, i32)],
}

fn tile_node(cluster: &mut ChildSpawnerCommands<'_>, cx_: &ClusterCtx, rx: i32, ry: i32) {
    let ClusterCtx { assets, tiles, imp, city_art, map, city, taken, hubs } = *cx_;
    let ny = city.y + ry;
    if ny < 0 || ny >= map.h {
        return;
    }
    let nx = map.wrap_x(city.x + rx);
    let Some(t) = map.get(nx, ny) else {
        return;
    };
    // Unexplored tiles stay black, as on the map.
    if !t.seen {
        return;
    }
    let center = rx == 0 && ry == 0;
    // Tiles another city holds are drawn dimmed and cannot be picked.
    let tint = if !center && taken.contains(&(nx, ny)) {
        Color::srgb(0.45, 0.45, 0.45)
    } else {
        Color::WHITE
    };
    let cx = (rx - ry) as f32 * 64.0 + 320.0;
    let cy = (rx + ry) as f32 * 32.0 + 160.0;
    let base = crate::render::base_name(t);
    let full = |image: Handle<Image>| {
        let mut n = ImageNode::new(image);
        n.color = tint;
        (
            n,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Px(128.0),
                height: Val::Px(64.0),
                ..default()
            },
        )
    };
    cluster
        .spawn(Node {
            position_type: PositionType::Absolute,
            left: Val::Px(cx - 64.0),
            top: Val::Px(cy - 32.0),
            width: Val::Px(128.0),
            height: Val::Px(64.0),
            ..default()
        })
        .with_children(|tile| {
            match crate::blend::cell_for(map, nx, ny) {
                Some((stem, col, row)) => {
                    let mut n = ImageNode::new(assets.load(crate::blend::sheet_path(stem)));
                    n.rect = Some(crate::blend::cell_rect(col, row));
                    n.color = tint;
                    tile.spawn(n);
                }
                None => {
                    let (mut n, node) = full(tiles.defs[&base].image.clone());
                    n.color = tint;
                    tile.spawn((n, node));
                }
            }
            if center {
                // cell center on the tile center, as on the map
                tile.spawn((
                    ImageNode::new(city_art.graphic(city.size)),
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(64.0 - 83.0),
                        top: Val::Px(32.0 - 47.0),
                        width: Val::Px(167.0),
                        height: Val::Px(95.0),
                        ..default()
                    },
                ));
            } else if let Some(c) = crate::blend::cover_sprite(map, nx, ny) {
                let size = c.rect.size();
                let mut n = ImageNode::new(assets.load(&c.path));
                n.rect = Some(c.rect);
                n.color = tint;
                tile.spawn((
                    n,
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(64.0 - c.anchor_px.x),
                        top: Val::Px(32.0 - c.anchor_px.y),
                        width: Val::Px(size.x),
                        height: Val::Px(size.y),
                        ..default()
                    },
                ));
            } else if let Some(ov) = crate::render::overlay_name(t) {
                let def = &tiles.defs[&ov];
                let mut n = ImageNode::new(def.image.clone());
                n.color = tint;
                tile.spawn((
                    n,
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(64.0 - def.anchor_px.x),
                        top: Val::Px(32.0 - def.anchor_px.y),
                        width: Val::Px(def.size.x),
                        height: Val::Px(def.size.y),
                        ..default()
                    },
                ));
            }
            // Roads, irrigation and mines, in the map's layer order. The
            // city sprite already shows the center's own road.
            if !center {
                for image in tile_overlays(map, imp, hubs, nx, ny) {
                    tile.spawn(full(image));
                }
            }
        });
}

/// Yield icons of a worked tile. Drawn after every tile so neighboring
/// forests and hills never cover them.
fn yield_node(cluster: &mut ChildSpawnerCommands<'_>, ctx: &ClusterCtx, rx: i32, ry: i32) {
    let ClusterCtx { assets, map, city, .. } = *ctx;
    let ny = city.y + ry;
    let nx = map.wrap_x(city.x + rx);
    let center = rx == 0 && ry == 0;
    if ny < 0 || ny >= map.h || !(center || city.worked.contains(&(nx, ny))) {
        return;
    }
    let (f, s) = if center {
        center_yields(map, nx, ny)
    } else {
        yields(&map.tiles[map.idx(nx, ny)])
    };
    let icons: Vec<u32> = std::iter::repeat_n(ICON_FOOD, f as usize)
        .chain(std::iter::repeat_n(ICON_SHIELD, s as usize))
        .collect();
    // Civ3 packs the icons in a row across the tile, overlapping them when
    // there are many. The center's row sits below the city sprite.
    let size = 22.0;
    let step = (84.0 / icons.len().max(1) as f32).min(size - 2.0);
    let width = step * icons.len().saturating_sub(1) as f32 + size;
    let cx = (rx - ry) as f32 * 64.0 + 320.0;
    let cy = (rx + ry) as f32 * 32.0 + 160.0;
    let top = cy + if center { 12.0 } else { -size / 2.0 };
    for (i, cell) in icons.iter().enumerate() {
        cluster.spawn((
            city_icon(assets, *cell),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(cx - width / 2.0 + i as f32 * step),
                top: Val::Px(top),
                width: Val::Px(size),
                height: Val::Px(size),
                ..default()
            },
        ));
    }
}

fn build_city_screen(commands: &mut Commands, ctx: &ClusterCtx, menu: bool) {
    let ClusterCtx { assets, map, city, .. } = *ctx;
    let font = assets.load("gen/fonts/lsans.ttf");
    let (net_food, shields_pt) = city_income(map, city);
    let grow_in = if net_food <= 0 {
        "never".to_string()
    } else {
        format!(
            "{}t",
            ceil_div(
                FOOD_BOX.saturating_sub(city.food),
                net_food as u8
            )
        )
    };
    let prod_left = city.production.cost().saturating_sub(city.shields);
    let prod_in = if shields_pt == 0 {
        "never".to_string()
    } else {
        format!("{}t", ceil_div(prod_left, shields_pt))
    };
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            CityScreenRoot,
            GlobalZIndex(10),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            ));
            root.spawn((
                ImageNode::new(assets.load("gen/cityscreen/background.png")),
                Node {
                    width: Val::Px(CONTENT_W),
                    height: Val::Px(CONTENT_H),
                    ..default()
                },
            ))
            .with_children(|content| {
                content.spawn((
                    Text::new(format!("{}  (pop {})", city.name, city.size)),
                    TextFont {
                        font: font.clone(),
                        font_size: 30.0,
                        ..default()
                    },
                    TextColor(PARCHMENT_TEXT),
                    Node {
                        position_type: PositionType::Absolute,
                        top: Val::Px(18.0),
                        left: Val::Px(0.0),
                        right: Val::Px(0.0),
                        ..default()
                    },
                    TextLayout::new_with_justify(Justify::Center),
                ));
                // black backing for the background's transparent view rect
                // (full width, y 92..508) + tile cluster + click catcher
                content.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.0),
                        top: Val::Px(92.0),
                        width: Val::Px(CONTENT_W),
                        height: Val::Px(416.0),
                        ..default()
                    },
                    BackgroundColor(Color::BLACK),
                ));
                content
                    .spawn(Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(CLUSTER_X),
                        top: Val::Px(CLUSTER_Y),
                        width: Val::Px(640.0),
                        height: Val::Px(320.0),
                        ..default()
                    })
                    .with_children(|cluster| {
                        for ry in -2i32..=2 {
                            for rx in -2i32..=2 {
                                if rx.abs() == 2 && ry.abs() == 2 {
                                    continue;
                                }
                                tile_node(cluster, ctx, rx, ry);
                            }
                        }
                        for ry in -2i32..=2 {
                            for rx in -2i32..=2 {
                                if rx.abs() == 2 && ry.abs() == 2 {
                                    continue;
                                }
                                yield_node(cluster, ctx, rx, ry);
                            }
                        }
                        cluster.spawn((
                            Button,
                            Node {
                                position_type: PositionType::Absolute,
                                left: Val::Px(0.0),
                                top: Val::Px(0.0),
                                width: Val::Px(640.0),
                                height: Val::Px(320.0),
                                ..default()
                            },
                            BackgroundColor(Color::NONE),
                            TileCluster,
                        ));
                    });
                let bar_cover = Color::srgb(0.98, 0.91, 0.73);
                // food box (blue bar) and shield box (yellow bar): cover
                // the unfilled tail of the baked-in bar art
                let food_frac = city.food as f32 / FOOD_BOX as f32;
                let shield_frac = (city.shields as f32
                    / city.production.cost() as f32)
                    .min(1.0);
                for (x0, x1, y0, h, frac) in [
                    (290.0, 908.0, 522.0, 24.0, food_frac),
                    (290.0, 778.0, 570.0, 24.0, shield_frac),
                ] {
                    let fill = (x1 - x0) * frac;
                    content.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(x0 + fill),
                            top: Val::Px(y0),
                            width: Val::Px(x1 - x0 - fill + 6.0),
                            height: Val::Px(h),
                            ..default()
                        },
                        BackgroundColor(bar_cover),
                    ));
                }
                txt(content, &font, &format!(
                    "Food {}/{}  ({:+}/t)  grows in {}", city.food, FOOD_BOX, net_food, grow_in),
                    17.0, 300.0, 524.0, PARCHMENT_TEXT);
                txt(content, &font, &format!(
                    "{} {}/{}  (+{}/t)  done in {}",
                    city.production.name(), city.shields, city.production.cost(), shields_pt, prod_in),
                    17.0, 300.0, 572.0, PARCHMENT_TEXT);
                // citizens (first green bar)
                content
                    .spawn(Node {
                        position_type: PositionType::Absolute,
                        top: Val::Px(624.0),
                        left: Val::Px(292.0),
                        width: Val::Px(380.0),
                        height: Val::Px(22.0),
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        ..default()
                    })
                    .with_children(|row| {
                        // workers first, then idle citizens as entertainers
                        let busy = city.worked.len().min(city.size as usize);
                        for i in 0..(city.size as usize).min(14) {
                            let head = if i < busy {
                                "gen/ui/citizen.png"
                            } else {
                                "gen/ui/entertainer.png"
                            };
                            row.spawn((
                                ImageNode::new(assets.load(head)),
                                Node {
                                    width: Val::Px(22.0),
                                    height: Val::Px(22.0),
                                    margin: UiRect::right(Val::Px(2.0)),
                                    ..default()
                                },
                            ));
                        }
                    });
                // buildings + tile totals (second and third green bars)
                let owned = if city.buildings.is_empty() {
                    "Buildings: none".to_string()
                } else {
                    format!(
                        "Buildings: {}",
                        city.buildings.iter().map(|b| b.name()).collect::<Vec<_>>().join(", ")
                    )
                };
                txt(content, &font, &owned, 16.0, 296.0, 656.0, PARCHMENT_TEXT);
                let (tf, ts) = city_yields(map, city);
                let idle = (city.size as usize).saturating_sub(city.worked.len());
                let idle = if idle > 0 { format!("; {idle} entertainer(s)") } else { String::new() };
                txt(content, &font, &format!(
                    "Tiles yield {tf} food, {ts} shields; citizens eat {}{idle}", 2 * city.size as i32),
                    16.0, 296.0, 688.0, PARCHMENT_TEXT);
                // left column: current build + change/governor buttons
                content.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(20.0), top: Val::Px(522.0),
                        width: Val::Px(66.0), height: Val::Px(66.0),
                        ..default()
                    },
                    build_icon(assets, city.production),
                ));
                txt(content, &font, city.production.name(), 18.0, 20.0, 594.0, PARCHMENT_TEXT);
                for (label, top, kind) in [
                    ("Change build", 616.0, ScreenButton::Change),
                    ("Governor", 676.0, ScreenButton::Governor),
                ] {
                    content
                        .spawn((
                            Button,
                            ImageNode::new(assets.load("gen/ui/prod_0.png")),
                            Node {
                                position_type: PositionType::Absolute,
                                left: Val::Px(8.0), top: Val::Px(top),
                                width: Val::Px(140.0), height: Val::Px(56.0),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                ..default()
                            },
                            kind,
                        ))
                        .with_children(|btn| {
                            btn.spawn((
                                Text::new(label),
                                TextFont { font: font.clone(), font_size: 16.0, ..default() },
                                TextColor(PARCHMENT_TEXT),
                            ));
                        });
                }
                // right panel over the black view: production queue
                content.spawn((
                    ImageNode::new(assets.load("gen/cityscreen/ProductionQueueBox.png")),
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(812.0), top: Val::Px(98.0),
                        width: Val::Px(203.0), height: Val::Px(360.0),
                        ..default()
                    },
                ));
                txt(content, &font, "Production queue", 15.0, 830.0, 106.0, PARCHMENT_TEXT);
                let mut rows: Vec<(String, Option<usize>)> =
                    vec![(format!("> {}", city.production.name()), None)];
                for (i, q) in city.queue.iter().enumerate() {
                    rows.push((format!("{}. {}", i + 1, q.name()), Some(i)));
                }
                for (n, (label, qi)) in rows.iter().enumerate() {
                    let top = 132.0 + n as f32 * 30.0;
                    match qi {
                        Some(i) => {
                            content
                                .spawn((
                                    Button,
                                    Node {
                                        position_type: PositionType::Absolute,
                                        left: Val::Px(826.0), top: Val::Px(top),
                                        width: Val::Px(176.0), height: Val::Px(26.0),
                                        align_items: AlignItems::Center,
                                        ..default()
                                    },
                                    BackgroundColor(Color::srgba(0.4, 0.25, 0.1, 0.25)),
                                    ScreenButton::Unqueue(*i),
                                ))
                                .with_children(|b| {
                                    b.spawn((
                                        Text::new(format!("{label}  [x]")),
                                        TextFont { font: font.clone(), font_size: 14.0, ..default() },
                                        TextColor(PARCHMENT_TEXT),
                                    ));
                                });
                        }
                        None => txt(content, &font, label, 16.0, 830.0, top + 2.0, PARCHMENT_TEXT),
                    }
                }
                // left panel over the black view: owned buildings
                txt(content, &font, "Buildings", 15.0, 18.0, 104.0, Color::srgb(0.95, 0.9, 0.7));
                for (n, b) in city.buildings.iter().enumerate() {
                    content.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(16.0), top: Val::Px(128.0 + n as f32 * 70.0),
                            width: Val::Px(63.0), height: Val::Px(64.0),
                            ..default()
                        },
                        build_icon(assets, *b),
                    ));
                    txt(content, &font, b.name(), 15.0, 84.0, 150.0 + n as f32 * 70.0, Color::srgb(0.95, 0.9, 0.7));
                }
                // modal build list
                if menu {
                    build_menu(content, assets, &font, city, shields_pt);
                }
                content.spawn((
                    Button,
                    ImageNode::new(assets.load("gen/ui/x_0.png")),
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(960.0),
                        top: Val::Px(14.0),
                        width: Val::Px(36.0),
                        height: Val::Px(37.0),
                        ..default()
                    },
                    ScreenButton::Close,
                ));
            });
        });
}

/// Hash of what the city screen draws from the map around a city.
fn radius_signature(map: &GameMap, city: &City) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let mut tiles = radius_tiles(map, city.x, city.y);
    tiles.push((city.x, city.y));
    for (x, y) in tiles {
        let t = &map.tiles[map.idx(x, y)];
        (t.road, t.irrigation, t.mine, t.seen, t.cover as u8, t.resource).hash(&mut h);
    }
    h.finish()
}

pub fn maintain_city_screen(
    mut commands: Commands,
    view: Res<CityView>,
    cities: Query<&City>,
    changed: Query<(), Changed<City>>,
    roots: Query<Entity, With<CityScreenRoot>>,
    mut last: Local<(Option<Entity>, u64)>,
    assets: Res<AssetServer>,
    tiles: Res<TileArt>,
    imp: Res<ImprovementArt>,
    city_art: Res<CityArt>,
    map: Res<GameMap>,
    menu: Res<BuildMenu>,
) {
    // Worker jobs finishing or fog lifting change the radius without
    // touching the City, so the tiles' own state is part of the key.
    let sig = view
        .0
        .and_then(|e| cities.get(e).ok())
        .map(|c| radius_signature(&map, c))
        .unwrap_or(0);
    if (view.0, sig) == *last && changed.is_empty() && !menu.is_changed() {
        return;
    }
    for r in roots.iter() {
        commands.entity(r).despawn();
    }
    *last = (view.0, sig);
    let Some(e) = view.0 else {
        return;
    };
    let Ok(city) = cities.get(e) else {
        return;
    };
    let taken = taken_tiles(cities.iter(), (city.x, city.y));
    let hubs: Vec<(i32, i32)> = cities.iter().map(|c| (c.x, c.y)).collect();
    let ctx = ClusterCtx {
        assets: &assets,
        tiles: &tiles,
        imp: &imp,
        city_art: &city_art,
        map: &map,
        city,
        taken: &taken,
        hubs: &hubs,
    };
    build_city_screen(&mut commands, &ctx, menu.0);
}

pub fn city_screen_input(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    clusters: Query<&Interaction, With<TileCluster>>,
    mut views: ResMut<CityView>,
    mut cities: Query<&mut City>,
    map: Res<GameMap>,
    mut board: ResMut<MessageBoard>,
) {
    if views.0.is_none() {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        views.0 = None;
        return;
    }
    if !buttons.just_pressed(MouseButton::Left) {
        return;
    }
    let hovered = clusters.iter().any(|i| {
        matches!(i, Interaction::Pressed | Interaction::Hovered)
    });
    if !hovered {
        return;
    }
    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let size = window.size();
    let Some((rx, ry)) = cluster_pick(size.x, size.y, cursor) else {
        return;
    };
    let Some(e) = views.0 else {
        return;
    };
    if !click_cluster(&map, &mut cities, e, rx, ry) {
        post(&mut board, "Another city works that tile, or it is unexplored.");
    }
}

/// A click on radius cell (rx, ry) of city `e`'s screen. False when the
/// tile cannot be worked.
pub fn click_cluster(
    map: &GameMap,
    cities: &mut Query<&mut City>,
    e: Entity,
    rx: i32,
    ry: i32,
) -> bool {
    if rx == 0 && ry == 0 {
        return true;
    }
    let Ok(city) = cities.get(e) else {
        return true;
    };
    let ny = city.y + ry;
    if ny < 0 || ny >= map.h {
        return true;
    }
    let tile = (map.wrap_x(city.x + rx), ny);
    let taken = taken_tiles(cities.iter(), (city.x, city.y));
    let Ok(mut city) = cities.get_mut(e) else {
        return true;
    };
    toggle_worked(map, &mut city, tile, &taken)
}

pub fn city_screen_buttons(
    mut commands: Commands,
    mut views: ResMut<CityView>,
    mut menu: ResMut<BuildMenu>,
    mut cities: Query<&mut City>,
    buttons: Query<(&Interaction, &ScreenButton), Changed<Interaction>>,
    audio: Res<GameAudio>,
    map: Res<GameMap>,
    mut board: ResMut<MessageBoard>,
) {
    for (interaction, button) in buttons.iter() {
        if *interaction != Interaction::Pressed {
            continue;
        }
        audio::sfx(&mut commands, &audio, "Button OK");
        let taken = views
            .0
            .and_then(|e| cities.get(e).ok())
            .map(|c| taken_tiles(cities.iter(), (c.x, c.y)))
            .unwrap_or_default();
        let mut city = views.0.and_then(|e| cities.get_mut(e).ok());
        match button {
            ScreenButton::Close => {
                views.0 = None;
                menu.0 = false;
            }
            ScreenButton::Change => menu.0 = true,
            ScreenButton::CloseMenu => menu.0 = false,
            ScreenButton::Governor => {
                if let Some(c) = city.as_mut() {
                    governor_assign(&map, c, &taken);
                }
            }
            ScreenButton::Pick(p) => {
                if let Some(c) = city.as_mut() {
                    c.change_build(*p);
                }
                menu.0 = false;
            }
            ScreenButton::Queue(p) => {
                if let Some(c) = city.as_mut() {
                    if !c.enqueue(*p) {
                        post(&mut board, "Cannot queue that item.");
                    }
                }
            }
            ScreenButton::Unqueue(i) => {
                if let Some(c) = city.as_mut() {
                    c.dequeue(*i);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Default map with every tile explored (only seen tiles are workable).
    fn test_map() -> GameMap {
        let mut map = GameMap::generate();
        for t in map.tiles.iter_mut() {
            t.seen = true;
        }
        map
    }

    fn none() -> HashSet<(i32, i32)> {
        HashSet::new()
    }

    fn test_city(map: &GameMap) -> City {
        let (x, y) = map.start;
        let mut city = City {
            name: "Test".to_string(),
            x,
            y,
            size: 1,
            food: 0,
            shields: 0,
            production: Production::Warrior,
            queue: vec![],
            buildings: vec![],
            worked: HashSet::new(),
        };
        governor_assign(map, &mut city, &none());
        city
    }

    #[test]
    fn radius_has_20_tiles() {
        let map = test_map();
        let tiles = radius_tiles(&map, map.start.0, map.start.1);
        assert_eq!(tiles.len(), 20);
    }

    /// Map label is Civ3's two lines: name with turns to grow, build with
    /// turns left; a stalled line shows `--`.
    #[test]
    fn banner_uses_civ3_growth_and_production_lines() {
        let tile = Tile {
            base: Base::Grassland,
            relief: Relief::Flat,
            cover: Cover::Bare,
            variant: 0,
            seen: true,
            visible: true,
            hut: false,
            camp: false,
            resource: None,
            road: false,
            irrigation: false,
            mine: false,
        };
        let map = GameMap { w: 3, h: 3, tiles: vec![tile; 9], start: (1, 1), seed: 1 };
        let mut city = City {
            name: "Kyoto".to_string(),
            x: 1,
            y: 1,
            size: 1,
            food: 0,
            shields: 0,
            production: Production::Warrior,
            queue: vec![],
            buildings: vec![],
            worked: HashSet::from([(1, 0)]),
        };
        // Center (2,1 with the free shield) plus one grassland tile: net
        // food 2 grows the 20-box in 10, 1 shield builds the Warrior in 10.
        assert_eq!(banner_text(&map, &city), "Kyoto : 10\nWarrior : 10");
        // No worked tiles: net food 0 stalls growth, shown as `--`.
        city.worked.clear();
        assert_eq!(banner_text(&map, &city), "Kyoto : --\nWarrior : 10");
    }

    #[test]
    fn governor_assigns_best_first() {
        let map = test_map();
        let mut city = test_city(&map);
        city.size = 3;
        governor_assign(&map, &mut city, &none());
        assert_eq!(city.worked.len(), 3);
        let mut all: Vec<(u8, u8)> = radius_tiles(&map, city.x, city.y)
            .iter()
            .map(|(x, y)| yields(&map.tiles[map.idx(*x, *y)]))
            .collect();
        all.sort_by(|a, b| b.cmp(a));
        let mut got: Vec<(u8, u8)> = city
            .worked
            .iter()
            .map(|(x, y)| yields(&map.tiles[map.idx(*x, *y)]))
            .collect();
        got.sort_by(|a, b| b.cmp(a));
        // governor sorts food-first, so food totals must match the top-3
        // food-first ordering, not the tuple ordering; just check count
        // here and food optimality below
        assert_eq!(got.len(), 3);
        let food_got: u8 = got.iter().map(|(f, _)| f).sum();
        let mut by_food: Vec<(u8, u8)> = radius_tiles(&map, city.x, city.y)
            .iter()
            .map(|(x, y)| yields(&map.tiles[map.idx(*x, *y)]))
            .collect();
        by_food.sort_by_key(|(f, s)| (std::cmp::Reverse(*f), std::cmp::Reverse(*s)));
        let food_best: u8 = by_food.iter().take(3).map(|(f, _)| f).sum();
        assert_eq!(food_got, food_best);
        let _ = all;
    }

    #[test]
    fn toggle_add_remove_replace() {
        let map = test_map();
        let mut city = test_city(&map);
        let tiles = radius_tiles(&map, city.x, city.y);
        // remove the assigned one
        let first = *city.worked.iter().next().unwrap();
        toggle_worked(&map, &mut city, first, &none());
        assert!(city.worked.is_empty());
        // add two with size 1: second replaces first
        toggle_worked(&map, &mut city, tiles[0], &none());
        toggle_worked(&map, &mut city, tiles[1], &none());
        assert_eq!(city.worked.len(), 1);
        assert!(city.worked.contains(&tiles[1]));
    }

    #[test]
    fn growth_and_starvation() {
        let map = test_map();
        let mut city = test_city(&map);
        city.food = FOOD_BOX - 1;
        // force growth regardless of terrain: rig by size-1978 trick is
        // overkill; instead directly verify the thresholds with income
        let (net, _) = city_income(&map, &city);
        let events = process_city_turn(&map, &mut city, &none());
        if net >= 1 {
            assert!(events.iter().any(|e| matches!(e, CityEvent::Grew)));
            assert_eq!(city.size, 2);
            assert_eq!(city.food, 0);
        }
        // starvation: size 3 city that cannot feed itself
        city.size = 3;
        city.food = 0;
        city.worked.clear();
        let (net, _) = city_income(&map, &city);
        assert!(net < 0, "bare size-3 city must starve");
        let events = process_city_turn(&map, &mut city, &none());
        assert!(events.iter().any(|e| matches!(e, CityEvent::Starved)));
        assert_eq!(city.size, 2);
    }

    #[test]
    fn production_completes_and_settler_costs_pop() {
        let map = test_map();
        let mut city = test_city(&map);
        city.production = Production::Warrior;
        city.shields = 9;
        let _ = process_city_turn(&map, &mut city, &none());
        // warrior may or may not complete depending on shields income;
        // force it:
        city.shields = city.production.cost();
        let events = process_city_turn(&map, &mut city, &none());
        assert!(events.iter().any(|e| matches!(
            e,
            CityEvent::Completed(UnitType::Warrior)
        )));
        // surplus shields carry into the next build
        let (_, income) = city_income(&map, &city);
        assert_eq!(city.shields, income);
        // settler costs 2 pop
        city.production = Production::Settler;
        city.size = 4;
        governor_fill(&map, &mut city, &none());
        city.shields = city.production.cost();
        let _ = process_city_turn(&map, &mut city, &none());
        assert_eq!(city.size, 2);
        assert!(city.worked.len() <= 2);
        // below size 3 the finished settler waits, announced once
        city.size = 2;
        city.food = 0;
        city.shields = city.production.cost() - 1;
        let events = process_city_turn(&map, &mut city, &none());
        assert!(events.iter().any(|e| matches!(e, CityEvent::TooSmall)));
        assert_eq!(city.size, 2);
        assert_eq!(city.shields, Production::Settler.cost());
        let events = process_city_turn(&map, &mut city, &none());
        assert!(!events.iter().any(|e| matches!(e, CityEvent::TooSmall | CityEvent::Completed(_))));
    }

    #[test]
    fn found_rules() {
        let map = test_map();
        let (x, y) = map.start;
        assert!(can_found(&map, &[], x, y));
        assert!(!can_found(&map, &[(x, y)], x, y));
        assert!(!can_found(&map, &[(x, y)], x + 1, y));
        assert!(!can_found(&map, &[(x, y)], x + 1, y + 1));
        assert!(can_found(&map, &[(x, y)], x + 2, y));
        // water and mountains refuse
        let mut water = None;
        let mut mountain = None;
        for ty in 0..map.h {
            for tx in 0..map.w {
                let t = &map.tiles[map.idx(tx, ty)];
                if matches!(t.base, Base::Ocean) && water.is_none() {
                    water = Some((tx, ty));
                }
                if t.relief == Relief::Mountain && mountain.is_none() {
                    mountain = Some((tx, ty));
                }
            }
        }
        assert!(!can_found(&map, &[], water.unwrap().0, water.unwrap().1));
        if let Some((mx, my)) = mountain {
            assert!(!can_found(&map, &[], mx, my));
        }
    }

    #[test]
    fn cluster_pick_center_and_edges() {
        // window 1280x800, content centered: origin (128, 16)
        // cluster center in content: (192+320, 140+160) = (512, 300)
        // window px (top-left origin): (640, 316); cursor is
        // bottom-left origin: (640, 800-316) = (640, 484)
        let c = cluster_pick(1280.0, 800.0, Vec2::new(640.0, 484.0));
        assert_eq!(c, Some((0, 0)));
        // one tile east: content (576, 332) -> cursor (704, 452)
        let c = cluster_pick(1280.0, 800.0, Vec2::new(704.0, 452.0));
        assert_eq!(c, Some((1, 0)));
        // far corner is outside
        assert_eq!(cluster_pick(1280.0, 800.0, Vec2::new(10.0, 10.0)), None);
    }

    #[test]
    fn building_completes_queue_advances_and_granary_keeps_food() {
        let map = test_map();
        let mut city = test_city(&map);
        city.production = Production::Granary;
        city.queue = vec![Production::Temple, Production::Worker];
        city.shields = Production::Granary.cost();
        let events = process_city_turn(&map, &mut city, &none());
        assert!(events
            .iter()
            .any(|e| matches!(e, CityEvent::Built(Production::Granary))));
        assert!(city.has(Production::Granary));
        assert_eq!(city.production, Production::Temple);
        assert_eq!(city.queue, vec![Production::Worker]);
        assert!(!city.buildable().contains(&Production::Granary));
        // granary keeps half the food box on growth
        city.food = FOOD_BOX - 1;
        let (net, _) = city_income(&map, &city);
        if net >= 1 {
            process_city_turn(&map, &mut city, &none());
            assert_eq!(city.food, FOOD_BOX / 2);
        }
    }

    #[test]
    fn change_build_penalizes_class_switch_and_queue_rules() {
        let map = test_map();
        let mut city = test_city(&map);
        city.production = Production::Warrior;
        city.shields = 8;
        city.change_build(Production::Worker);
        assert_eq!(city.shields, 8);
        city.change_build(Production::Barracks);
        assert_eq!(city.shields, 4);
        assert!(city.enqueue(Production::Temple));
        assert!(!city.enqueue(Production::Temple));
        assert!(!city.enqueue(Production::Barracks));
        assert!(city.enqueue(Production::Warrior));
        assert!(city.enqueue(Production::Warrior));
        city.dequeue(0);
        assert_eq!(city.queue[0], Production::Warrior);
        // building with empty queue falls back to a Warrior
        city.queue.clear();
        city.production = Production::Temple;
        city.shields = Production::Temple.cost();
        process_city_turn(&map, &mut city, &none());
        assert_eq!(city.production, Production::Warrior);
    }

    #[test]
    fn other_cities_tiles_and_unexplored_tiles_are_not_workable() {
        let mut map = test_map();
        let a = test_city(&map);
        let tile = *a.worked.iter().next().unwrap();
        let mut b = test_city(&map);
        b.name = "B".into();
        b.x = map.wrap_x(a.x + 2);
        let taken = taken_tiles([&a], (b.x, b.y));
        assert!(taken.contains(&(a.x, a.y)), "city centers are taken");
        assert!(taken.contains(&tile), "a's worked tile is taken for b");
        b.worked.clear();
        assert!(!toggle_worked(&map, &mut b, tile, &taken));
        assert!(!b.worked.contains(&tile));
        governor_assign(&map, &mut b, &taken);
        assert!(b.worked.is_disjoint(&a.worked));
        // a city never counts its own tiles as taken
        assert!(!taken_tiles([&a], (a.x, a.y)).contains(&tile));
        // unexplored tiles cannot be worked
        let (ux, uy) = radius_tiles(&map, b.x, b.y)
            .into_iter()
            .find(|t| !taken.contains(t))
            .unwrap();
        let i = map.idx(ux, uy);
        map.tiles[i].seen = false;
        b.worked.clear();
        assert!(!toggle_worked(&map, &mut b, (ux, uy), &taken));
    }

    #[test]
    fn growth_keeps_manual_picks() {
        let map = test_map();
        let mut city = test_city(&map);
        // pick the worst tile by hand, then grow
        let worst = radius_tiles(&map, city.x, city.y)
            .into_iter()
            .min_by_key(|t| tile_rank(&map, t))
            .unwrap();
        city.worked.clear();
        assert!(toggle_worked(&map, &mut city, worst, &none()));
        city.size = 2;
        governor_fill(&map, &mut city, &none());
        assert_eq!(city.worked.len(), 2);
        assert!(city.worked.contains(&worst), "manual pick survives growth");
        // shrinking drops the worst tile first
        city.size = 1;
        governor_fill(&map, &mut city, &none());
        assert!(!city.worked.contains(&worst));
    }

    #[test]
    fn center_yield_is_irrigated_and_at_least_one_shield() {
        let mut map = test_map();
        // an irrigable grassland tile: center gets +1 food and 1 shield
        let spot = (0..map.h)
            .flat_map(|y| (0..map.w).map(move |x| (x, y)))
            .find(|&(x, y)| {
                map.get(x, y).unwrap().base == Base::Grassland
                    && map.get(x, y).unwrap().resource.is_none()
                    && can_irrigate(&map, x, y)
            })
            .expect("irrigable grassland");
        assert_eq!(center_yields(&map, spot.0, spot.1), (3, 1));
        // no free irrigation without water access
        for (nx, ny) in map.neighbors(spot.0, spot.1) {
            let i = map.idx(nx, ny);
            map.tiles[i].base = Base::Grassland;
            map.tiles[i].irrigation = false;
        }
        assert_eq!(center_yields(&map, spot.0, spot.1), (2, 1));
    }

    #[test]
    fn city_screen_shows_improvements_in_yields() {
        let mut map = test_map();
        let mut city = test_city(&map);
        let tile = *city.worked.iter().next().unwrap();
        let (f0, s0) = city_yields(&map, &city);
        let i = map.idx(tile.0, tile.1);
        map.tiles[i].mine = true;
        let (f1, s1) = city_yields(&map, &city);
        assert_eq!(f1, f0);
        assert!(s1 > s0);
        city.worked.clear();
        assert_eq!(city_yields(&map, &city), center_yields(&map, city.x, city.y));
    }
}
