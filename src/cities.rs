//! Cities: founding, worked tiles, food and shields, growth, production,
//! map visuals, and the city screen.

use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy::window::PrimaryWindow;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs;

use crate::audio::{self, GameAudio};
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
    Settler,
    Worker,
}

impl Production {
    pub fn cost(self) -> u8 {
        match self {
            Production::Warrior => 10,
            Production::Settler => 30,
            Production::Worker => 10,
        }
    }

    pub fn unit(self) -> UnitType {
        match self {
            Production::Warrior => UnitType::Warrior,
            Production::Settler => UnitType::Settler,
            Production::Worker => UnitType::Worker,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Production::Warrior => "Warrior",
            Production::Settler => "Settler",
            Production::Worker => "Worker",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Production::Warrior => "gen/ui/uniticon_warrior.png",
            Production::Settler => "gen/ui/uniticon_settler.png",
            Production::Worker => "gen/ui/uniticon_worker.png",
        }
    }

    pub fn cycle(self) -> Self {
        match self {
            Production::Warrior => Production::Settler,
            Production::Settler => Production::Worker,
            Production::Worker => Production::Warrior,
        }
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

/// Governor: work the best `size` tiles, food first then shields.
pub fn governor_assign(map: &GameMap, city: &mut City) {
    let mut tiles = radius_tiles(map, city.x, city.y);
    tiles.sort_by_key(|(x, y)| {
        let (f, s) = yields(&map.tiles[map.idx(*x, *y)]);
        (std::cmp::Reverse(f), std::cmp::Reverse(s))
    });
    city.worked = tiles.into_iter().take(city.size as usize).collect();
}

/// Toggle a worked tile. When full, the worst tile is replaced.
pub fn toggle_worked(map: &GameMap, city: &mut City, tile: (i32, i32)) {
    if city.worked.remove(&tile) {
        return;
    }
    if city.worked.len() < city.size as usize {
        city.worked.insert(tile);
        return;
    }
    let worst = city.worked.iter().min_by_key(|(x, y)| {
        yields(&map.tiles[map.idx(*x, *y)])
    });
    if let Some(worst) = worst.copied() {
        city.worked.remove(&worst);
    }
    city.worked.insert(tile);
}

/// (net food after feeding 2 per pop, shields per turn).
/// The city center always yields 2 food and 1 shield for free.
pub fn city_income(map: &GameMap, city: &City) -> (i16, u8) {
    let mut food = 2i16;
    let mut shields = 1u8;
    for (x, y) in city.worked.iter() {
        let (f, s) = yields(&map.tiles[map.idx(*x, *y)]);
        food += f as i16;
        shields += s;
    }
    (food - 2 * city.size as i16, shields)
}

pub enum CityEvent {
    Grew,
    Starved,
    Completed(UnitType),
}

pub fn process_city_turn(map: &GameMap, city: &mut City) -> Vec<CityEvent> {
    let mut events = vec![];
    let (net_food, shields) = city_income(map, city);
    let food = city.food as i16 + net_food;
    if food >= FOOD_BOX as i16 {
        city.size += 1;
        city.food = 0;
        governor_assign(map, city);
        events.push(CityEvent::Grew);
    } else if food < 0 {
        if city.size > 1 {
            city.size -= 1;
            governor_assign(map, city);
            events.push(CityEvent::Starved);
        }
        city.food = 0;
    } else {
        city.food = food as u8;
    }
    city.shields += shields;
    if city.shields >= city.production.cost() {
        city.shields = 0;
        let unit = city.production.unit();
        if city.production == Production::Settler {
            city.size = (city.size.saturating_sub(2)).max(1);
            governor_assign(map, city);
        }
        events.push(CityEvent::Completed(unit));
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

fn ceil_div(a: u8, b: u8) -> u8 {
    (a + b - 1) / b
}

fn banner_text(map: &GameMap, city: &City) -> String {
    let (_, shields) = city_income(map, city);
    let turns = if shields == 0 {
        "never".to_string()
    } else {
        let left = city.production.cost().saturating_sub(city.shields);
        format!("{}t", ceil_div(left, shields))
    };
    format!("{} ({})\n{} ({})", city.name, city.size, city.production.name(), turns)
}

pub fn spawn_city_visuals(
    commands: &mut Commands,
    art: &CityArt,
    assets: &AssetServer,
    map: &GameMap,
    entity: Entity,
    city: &City,
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
    commands.spawn((
        Text2d::new(banner_text(map, city)),
        TextFont {
            font: assets.load("gen/fonts/lsans.ttf"),
            font_size: 17.0,
            ..default()
        },
        TextColor(Color::WHITE),
        Transform::from_xyz(pos.x, pos.y + 78.0, sprite_z(city.x, city.y, 5.0)),
        CityBanner(entity),
    ));
}

/// Found a city with the selected settler (B key).
pub fn found_city(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    selected: Res<Selected>,
    units: Query<(Entity, &Unit)>,
    cities: Query<&City>,
    mut names: ResMut<CityNamesUsed>,
    map: Res<GameMap>,
    art: Res<CityArt>,
    assets: Res<AssetServer>,
    view: Res<CityView>,
    splash: Res<SplashUp>,
    audio: Res<GameAudio>,
) {
    if view.0.is_some() || splash.0 || !keys.just_pressed(KeyCode::KeyB) {
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
    let mut city = City {
        name: next_name(&mut names),
        x,
        y,
        size: 1,
        food: 0,
        shields: 0,
        production: Production::Warrior,
        worked: HashSet::new(),
    };
    governor_assign(&map, &mut city);
    let entity = commands.spawn(city.clone()).id();
    spawn_city_visuals(&mut commands, &art, &assets, &map, entity, &city);
    commands.spawn(AudioPlayer(audio.build.clone()));
}

pub fn end_turn_cities(
    mut commands: Commands,
    mut events: MessageReader<TurnEnded>,
    map: Res<GameMap>,
    mut cities: Query<&mut City>,
    art: Res<UnitArt>,
    audio: Res<GameAudio>,
) {
    for _ in events.read() {
        for mut city in cities.iter_mut() {
            for event in process_city_turn(&map, &mut city) {
                if let CityEvent::Completed(unit) = event {
                    units::spawn_unit(&mut commands, &art, unit, city.x, city.y);
                    audio::sfx(&mut commands, &audio, "WhatToBuild");
                }
            }
        }
    }
}

pub fn sync_city_visuals(
    map: Res<GameMap>,
    art: Res<CityArt>,
    cities: Query<&City>,
    mut sprites: Query<(&CitySprite, &mut Sprite)>,
    mut banners: Query<(&CityBanner, &mut Text2d)>,
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
}

pub fn city_visibility(
    map: Res<GameMap>,
    reveal: Res<RevealAll>,
    cities: Query<&City>,
    mut sprites: Query<(&CitySprite, &mut Visibility), Without<CityBanner>>,
    mut banners: Query<(&CityBanner, &mut Visibility)>,
) {
    for (link, mut vis) in sprites.iter_mut() {
        *vis = city_vis(&map, &cities, &reveal, link.0);
    }
    for (link, mut vis) in banners.iter_mut() {
        *vis = city_vis(&map, &cities, &reveal, link.0);
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

fn tile_node(
    cluster: &mut ChildSpawnerCommands<'_>,
    _assets: &AssetServer,
    tiles: &TileArt,
    map: &GameMap,
    city: &City,
    city_art: &CityArt,
    rx: i32,
    ry: i32,
) {
    let nx = map.wrap_x(city.x + rx);
    let ny = (city.y + ry).clamp(0, map.h - 1);
    let Some(t) = map.get(nx, ny) else {
        return;
    };
    let cx = (rx - ry) as f32 * 64.0 + 320.0;
    let cy = (rx + ry) as f32 * 32.0 + 160.0;
    let base = crate::render::base_name(t);
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
            tile.spawn(ImageNode::new(tiles.defs[&base].image.clone()));
            if rx == 0 && ry == 0 {
                tile.spawn((
                    ImageNode::new(city_art.graphic(city.size)),
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px((128.0 - 167.0) / 2.0),
                        top: Val::Px(64.0 - 95.0 - 4.0),
                        width: Val::Px(167.0),
                        height: Val::Px(95.0),
                        ..default()
                    },
                ));
            } else if let Some(ov) = crate::render::overlay_name(t) {
                let def = &tiles.defs[&ov];
                tile.spawn((
                    ImageNode::new(def.image.clone()),
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(
                            64.0 - def.anchor_px.x,
                        ),
                        top: Val::Px(32.0 - def.anchor_px.y),
                        width: Val::Px(def.size.x),
                        height: Val::Px(def.size.y),
                        ..default()
                    },
                ));
            }
            let worked = (rx == 0 && ry == 0)
                || city.worked.contains(&(nx, ny));
            if worked {
                let (f, s) = if rx == 0 && ry == 0 {
                    (2, 1)
                } else {
                    yields(t)
                };
                let n = f as usize + s as usize;
                for i in 0..n {
                    let color = if i < f as usize {
                        Color::srgb(0.2, 0.8, 0.2)
                    } else {
                        Color::srgb(0.9, 0.6, 0.1)
                    };
                    tile.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(
                                64.0 - n as f32 * 6.0 + i as f32 * 12.0,
                            ),
                            top: Val::Px(46.0),
                            width: Val::Px(10.0),
                            height: Val::Px(10.0),
                            ..default()
                        },
                        BackgroundColor(color),
                    ));
                }
            }
        });
}

fn build_city_screen(
    commands: &mut Commands,
    assets: &AssetServer,
    tiles: &TileArt,
    city_art: &CityArt,
    map: &GameMap,
    city: &City,
) {
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
                content.spawn((
                    Text::new(format!(
                        "Food {}/{} ({:+}) grows in {}    Shields {}/{} ({:+}/t)  {} in {}",
                        city.food,
                        FOOD_BOX,
                        net_food,
                        grow_in,
                        city.shields,
                        city.production.cost(),
                        shields_pt,
                        city.production.name(),
                        prod_in,
                    )),
                    TextFont {
                        font: font.clone(),
                        font_size: 17.0,
                        ..default()
                    },
                    TextColor(PARCHMENT_TEXT),
                    Node {
                        position_type: PositionType::Absolute,
                        top: Val::Px(58.0),
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
                                tile_node(
                                    cluster, assets, tiles, map, city,
                                    city_art, rx, ry,
                                );
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
                // citizens row
                content
                    .spawn(Node {
                        position_type: PositionType::Absolute,
                        top: Val::Px(520.0),
                        left: Val::Px(60.0),
                        width: Val::Px(560.0),
                        height: Val::Px(60.0),
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        ..default()
                    })
                    .with_children(|row| {
                        row.spawn((
                            Text::new("Citizens: "),
                            TextFont {
                                font: font.clone(),
                                font_size: 18.0,
                                ..default()
                            },
                            TextColor(PARCHMENT_TEXT),
                        ));
                        for _ in 0..city.size.min(12) {
                            row.spawn((
                                ImageNode::new(
                                    assets.load("gen/ui/citizen.png"),
                                ),
                                Node {
                                    width: Val::Px(34.0),
                                    height: Val::Px(34.0),
                                    margin: UiRect::left(Val::Px(2.0)),
                                    ..default()
                                },
                            ));
                        }
                        if city.size > 12 {
                            row.spawn((
                                Text::new(format!(" +{}", city.size - 12)),
                                TextFont {
                                    font: font.clone(),
                                    font_size: 18.0,
                                    ..default()
                                },
                                TextColor(PARCHMENT_TEXT),
                            ));
                        }
                    });
                // production row
                content.spawn((
                    ImageNode::new(
                        assets.load(city.production.icon()),
                    ),
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(660.0),
                        top: Val::Px(520.0),
                        width: Val::Px(48.0),
                        height: Val::Px(48.0),
                        ..default()
                    },
                ));
                content.spawn((
                    Text::new(format!(
                        "{} ({}t)",
                        city.production.name(),
                        prod_in
                    )),
                    TextFont {
                        font: font.clone(),
                        font_size: 20.0,
                        ..default()
                    },
                    TextColor(PARCHMENT_TEXT),
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(716.0),
                        top: Val::Px(528.0),
                        ..default()
                    },
                ));
                content
                    .spawn((
                        Button,
                        ImageNode::new(assets.load("gen/ui/prod_0.png")),
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(660.0),
                            top: Val::Px(580.0),
                            width: Val::Px(200.0),
                            height: Val::Px(60.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        ScreenButton::Change,
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new("Change build"),
                            TextFont {
                                font: font.clone(),
                                font_size: 18.0,
                                ..default()
                            },
                            TextColor(PARCHMENT_TEXT),
                        ));
                    });
                content.spawn((
                    Text::new("Click tiles to assign workers.  ESC closes."),
                    TextFont {
                        font: font.clone(),
                        font_size: 16.0,
                        ..default()
                    },
                    TextColor(PARCHMENT_TEXT),
                    Node {
                        position_type: PositionType::Absolute,
                        top: Val::Px(700.0),
                        left: Val::Px(0.0),
                        right: Val::Px(0.0),
                        ..default()
                    },
                    TextLayout::new_with_justify(Justify::Center),
                ));
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

pub fn maintain_city_screen(
    mut commands: Commands,
    view: Res<CityView>,
    cities: Query<&City>,
    changed: Query<(), Changed<City>>,
    roots: Query<Entity, With<CityScreenRoot>>,
    mut last: Local<Option<Entity>>,
    assets: Res<AssetServer>,
    tiles: Res<TileArt>,
    city_art: Res<CityArt>,
    map: Res<GameMap>,
) {
    if view.0 == *last && changed.is_empty() {
        return;
    }
    for r in roots.iter() {
        commands.entity(r).despawn_related::<Children>();
    }
    *last = view.0;
    let Some(e) = view.0 else {
        return;
    };
    let Ok(city) = cities.get(e) else {
        return;
    };
    build_city_screen(&mut commands, &assets, &tiles, &city_art, &map, city);
}

pub fn city_screen_input(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    clusters: Query<&Interaction, With<TileCluster>>,
    mut views: ResMut<CityView>,
    mut cities: Query<&mut City>,
    map: Res<GameMap>,
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
    if rx == 0 && ry == 0 {
        return;
    }
    let Some(e) = views.0 else {
        return;
    };
    let Ok(mut city) = cities.get_mut(e) else {
        return;
    };
    let tile = (map.wrap_x(city.x + rx), (city.y + ry).clamp(0, map.h - 1));
    toggle_worked(&map, &mut city, tile);
}

pub fn city_screen_buttons(
    mut commands: Commands,
    mut views: ResMut<CityView>,
    mut cities: Query<&mut City>,
    buttons: Query<(&Interaction, &ScreenButton), Changed<Interaction>>,
    audio: Res<GameAudio>,
) {
    for (interaction, button) in buttons.iter() {
        if *interaction != Interaction::Pressed {
            continue;
        }
        audio::sfx(&mut commands, &audio, "Button OK");
        match button {
            ScreenButton::Close => views.0 = None,
            ScreenButton::Change => {
                if let Some(e) = views.0 {
                    if let Ok(mut city) = cities.get_mut(e) {
                        city.production = city.production.cycle();
                        city.shields = 0;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_map() -> GameMap {
        GameMap::generate()
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
            worked: HashSet::new(),
        };
        governor_assign(map, &mut city);
        city
    }

    #[test]
    fn radius_has_20_tiles() {
        let map = test_map();
        let tiles = radius_tiles(&map, map.start.0, map.start.1);
        assert_eq!(tiles.len(), 20);
    }

    #[test]
    fn governor_assigns_best_first() {
        let map = test_map();
        let mut city = test_city(&map);
        city.size = 3;
        governor_assign(&map, &mut city);
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
        toggle_worked(&map, &mut city, first);
        assert!(city.worked.is_empty());
        // add two with size 1: second replaces first
        toggle_worked(&map, &mut city, tiles[0]);
        toggle_worked(&map, &mut city, tiles[1]);
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
        let events = process_city_turn(&map, &mut city);
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
        let events = process_city_turn(&map, &mut city);
        assert!(events.iter().any(|e| matches!(e, CityEvent::Starved)));
        assert_eq!(city.size, 2);
    }

    #[test]
    fn production_completes_and_settler_costs_pop() {
        let map = test_map();
        let mut city = test_city(&map);
        city.production = Production::Warrior;
        city.shields = 9;
        let _ = process_city_turn(&map, &mut city);
        // warrior may or may not complete depending on shields income;
        // force it:
        city.shields = city.production.cost();
        let events = process_city_turn(&map, &mut city);
        assert!(events.iter().any(|e| matches!(
            e,
            CityEvent::Completed(UnitType::Warrior)
        )));
        assert_eq!(city.shields, 0);
        // settler costs 2 pop, clamped at 1
        city.production = Production::Settler;
        city.size = 4;
        city.shields = city.production.cost();
        let _ = process_city_turn(&map, &mut city);
        assert_eq!(city.size, 2);
        city.size = 2;
        city.shields = city.production.cost();
        let _ = process_city_turn(&map, &mut city);
        assert_eq!(city.size, 1);
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
}
