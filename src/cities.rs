//! Cities: founding, worked tiles, food and shields, growth, production,
//! map visuals, and the city screen.

use bevy::asset::RenderAssetUsages;
use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::sprite::Anchor;
use bevy::text::TextLayoutInfo;
use bevy::window::PrimaryWindow;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs;

use crate::audio::{self, GameAudio};
use crate::civs::{CIV_COUNT, CIVS, CivilizationEnded, Civilizations};
use crate::features::{MessageBoard, post};
use crate::improvements::{ImprovementArt, can_irrigate, tile_overlays};
use crate::map::*;
use crate::render::{RevealAll, sprite_z};
use crate::splash::SplashUp;
use crate::tiles::TileArt;
use crate::units::{self, Selected, Unit, UnitArt, UnitType};

pub const FOOD_BOX: u8 = 20;

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

    /// Gold upkeep per turn (Civ3: one for each of these).
    pub fn upkeep(self) -> u8 {
        match self {
            Production::Barracks | Production::Granary | Production::Temple => 1,
            _ => 0,
        }
    }

    /// Culture per turn (Civ3's per-building values).
    pub fn culture(self) -> u32 {
        match self {
            Production::Temple => 2,
            _ => 0,
        }
    }

    /// Happy faces the building adds, for the improvements list.
    pub fn happy(self) -> u8 {
        match self {
            Production::Temple => 1,
            _ => 0,
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
    /// Owning civilization, an index into `CIVS`.
    pub civ: usize,
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
    /// Culture accumulated: border levels are powers of ten (`economy.md`).
    pub culture: u32,
    /// Turn the city was founded (the clone has no calendar).
    pub founded: u32,
}

/// How many names each civ has taken from its own list.
#[derive(Resource, Default)]
pub struct CityNamesUsed(pub [usize; CIV_COUNT]);

/// Gold in each civ's treasury. Nothing spends it yet; taxes accumulate.
#[derive(Resource, Default)]
pub struct Treasury(pub [u32; CIV_COUNT]);

#[derive(Resource, Default)]
pub struct CityView(pub Option<Entity>);

#[derive(Deserialize)]
struct CityEntry {
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
            .expect("run from the repo root after tools/prep_assets.py");
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
        let worst = city
            .worked
            .iter()
            .min_by_key(|t| tile_rank(map, t))
            .copied();
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
        let worst = city
            .worked
            .iter()
            .min_by_key(|t| tile_rank(map, t))
            .copied();
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

/// Commerce a tile carries: water yields it, a road adds one. Rivers and
/// trade are not modelled yet, so inland land without a road gives nothing.
pub fn tile_commerce(t: &Tile) -> u8 {
    let base = match t.base {
        Base::Coast => 2,
        Base::Sea | Base::Ocean => 1,
        _ => 0,
    };
    base + u8::from(t.road)
}

/// Gross commerce: the center always gives one (Civ3's city tile), plus
/// every worked tile.
pub fn city_commerce(map: &GameMap, city: &City) -> u8 {
    let center = tile_commerce(&map.tiles[map.idx(city.x, city.y)]).max(1);
    city.worked
        .iter()
        .map(|(x, y)| tile_commerce(&map.tiles[map.idx(*x, *y)]))
        .fold(center, u8::saturating_add)
}

/// Tax, science and luxury rates in tenths: Civ3's default 50/50/0.
pub const TAX_RATE: u8 = 5;
pub const SCI_RATE: u8 = 5;
pub const LUX_RATE: u8 = 0;

/// (tax, science, luxury) from gross commerce. Each share rounds down, so
/// the remainder is lost, as Civ3 does (27 -> 13/13/0).
pub fn commerce_split(commerce: u8) -> (u8, u8, u8) {
    (
        commerce * TAX_RATE / 10,
        commerce * SCI_RATE / 10,
        commerce * LUX_RATE / 10,
    )
}

/// Culture per turn: the Palace's belongs to the capital.
pub fn culture_per_turn(city: &City, capital: bool) -> u32 {
    city.buildings.iter().map(|b| b.culture()).sum::<u32>() + u32::from(capital)
}

/// Culture and tax a city adds when its civ's turn ends. Everything comes
/// from the city itself, so one civ's turn never moves another's numbers;
/// `capital` is whether this city is its own civ's capital.
pub fn accrue(map: &GameMap, city: &mut City, capital: bool) -> u32 {
    city.culture += culture_per_turn(city, capital);
    commerce_split(city_commerce(map, city)).0 as u32
}

/// (current level, next border expansion) in culture. Levels are powers of
/// ten (`economy.md`); the first level shows 10 as Civ3 does.
pub fn culture_thresholds(culture: u32) -> (u32, u32) {
    let mut next = 10u32;
    while next <= culture {
        next *= 10;
    }
    ((next / 10).max(10), next)
}

/// Culture level: 1 until the first expansion threshold (10), then one
/// level per power of ten, as the city screen's culture bar counts them
/// (`economy.md`), capped at 6 the way the game caps its own counter.
pub fn culture_level(culture: u32) -> u32 {
    let mut level = 1;
    let mut next = 10u32;
    while next <= culture && level < 6 {
        level += 1;
        next = next.saturating_mul(10);
    }
    level
}

/// Civ3's largest cultural border: five tiles from the city center.
pub const CULTURE_RADIUS_MAX: i32 = 5;

/// The initial claim is the 3x3 neighborhood, a diamond on the map.
/// Higher levels retain their distance-based rings, capped at five tiles.
pub fn culture_radius(level: u32) -> i32 {
    if level == 1 {
        1
    } else {
        (level as i32 + 1).min(CULTURE_RADIUS_MAX)
    }
}

/// Owner of every claimed tile, as an index into `cities`.
///
/// The nearest city claims the tile; when two are equally close the one
/// with more culture wins, then the older one — `cities` is walked in
/// that order and only strict improvements replace an owner, so the
/// result is stable frame to frame.
pub fn territory(map: &GameMap, cities: &[&City]) -> HashMap<(i32, i32), usize> {
    let mut order: Vec<usize> = (0..cities.len()).collect();
    order.sort_by(|&a, &b| {
        cities[b]
            .culture
            .cmp(&cities[a].culture)
            .then(cities[a].founded.cmp(&cities[b].founded))
            .then(cities[a].name.cmp(&cities[b].name))
    });
    let mut out: HashMap<(i32, i32), usize> = HashMap::new();
    let mut best: HashMap<(i32, i32), i32> = HashMap::new();
    for i in order {
        let c = cities[i];
        let level = culture_level(c.culture);
        let r = culture_radius(level);
        for dy in -r..=r {
            let y = c.y + dy;
            if y < 0 || y >= map.h {
                continue;
            }
            for dx in -r..=r {
                let d2 = dx * dx + dy * dy;
                if level > 1 && d2 > r * r {
                    continue;
                }
                let key = (map.wrap_x(c.x + dx), y);
                if best.get(&key).is_none_or(|b| d2 < *b) {
                    best.insert(key, d2);
                    out.insert(key, i);
                }
            }
        }
    }
    out
}

/// Strategic and luxury goods one civ controls: how many tiles of each lie
/// inside the cultural borders of `civ`'s cities. `cities` holds every
/// civ's cities, so a tile two civs contend for is counted only by the one
/// whose city `territory` awards it.
pub fn resources_owned(
    map: &GameMap,
    cities: &[&City],
    civ: usize,
) -> (Vec<(u8, u32)>, Vec<(u8, u32)>) {
    use std::collections::BTreeMap;
    let mut counts: BTreeMap<u8, u32> = BTreeMap::new();
    for ((x, y), owner) in territory(map, cities) {
        if cities[owner].civ != civ {
            continue;
        }
        let t = &map.tiles[map.idx(x, y)];
        if let (true, Some(id)) = (t.seen, t.resource) {
            *counts.entry(id).or_insert(0) += 1;
        }
    }
    let mut strategic = vec![];
    let mut luxury = vec![];
    for (id, n) in counts {
        match crate::features::GOODS[id as usize].kind {
            crate::features::GoodKind::Strategic => strategic.push((id, n)),
            crate::features::GoodKind::Luxury => luxury.push((id, n)),
            crate::features::GoodKind::Bonus => {}
        }
    }
    (strategic, luxury)
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
            || (p.is_building() && (self.has(p) || self.production == p || self.queue.contains(&p)))
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

/// Next unused city name of `civ`. Each civ walks its own list, so one
/// civ's foundings never consume another's names.
fn next_name(used: &mut CityNamesUsed, civ: usize) -> String {
    let names = CIVS[civ].city_names;
    let i = used.0[civ];
    used.0[civ] += 1;
    let base = names[i % names.len()];
    if i < names.len() {
        base.to_string()
    } else {
        format!("{base} {}", i / names.len() + 1)
    }
}

#[derive(Component)]
pub(crate) struct CitySprite(pub Entity);

#[derive(Component)]
pub(crate) struct CityBanner(pub Entity);

#[derive(Component)]
pub(crate) struct CityBadgeText(pub Entity);

/// Root of a city's map label: an unsized anchor centered under the city.
/// Its parts are children and inherit its visibility, so fog only syncs
/// this root.
#[derive(Component)]
pub(crate) struct CityLabelBack(pub Entity);

/// The pieces of a label, laid out left to right around the root by
/// `sync_city_visuals` once the text has a measured width.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum LabelPiece {
    /// Civ-colored population box on the left.
    Badge,
    /// Dark translucent box behind the two text lines.
    Box,
    /// The two text lines.
    Text,
    /// Civ-colored capital star box on the right; only the capital has it.
    Star,
}

#[derive(Component)]
pub(crate) struct LabelPart {
    pub city: Entity,
    pub piece: LabelPiece,
}

/// Each civ's first founded city: it alone gets the star badge, as in
/// Civ3. Indexed by civ.
#[derive(Resource, Default)]
pub struct Capital(pub [Option<Entity>; CIV_COUNT]);

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
                if (y0 > py) != (y1 > py) && px < (x1 - x0) * (py - y0) / (y1 - y0) + x0 {
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
            Extent3d {
                width: S,
                height: S,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            data,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        Self(images.add(image))
    }
}

/// Population number on the badge.
pub const CIV_BADGE_INK: Color = Color::srgb_u8(236, 240, 226);
/// Dark translucent box behind the name and build lines.
pub const LABEL_FILL: Color = Color::srgba(0.06, 0.05, 0.04, 0.62);
const LABEL_INK: Color = Color::srgb_u8(236, 232, 218);
const LABEL_FONT: f32 = 14.0;
const BADGE_W: f32 = 26.0;
/// Height of the whole label: two text lines plus a little air.
const LABEL_H: f32 = 34.0;
const STAR_W: f32 = 26.0;
/// Horizontal padding between the text and the edges of its box.
const TEXT_PAD: f32 = 7.0;
/// Label center below the tile center: the label hangs under the town art
/// over the tile's lower half, as in Civ3.
const LABEL_DROP: f32 = 28.0;

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

/// Center of the label in world units: horizontally on the city, just
/// under the tile center.
fn label_origin(city: &City) -> Vec2 {
    let pos = tile_to_world(city.x, city.y);
    Vec2::new(pos.x, pos.y - LABEL_DROP)
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
    // Civ3 paints the badge and the star box in the owner's civ color.
    let civ_color = CIVS[city.civ].color;
    let origin = label_origin(city);
    let z = sprite_z(city.x, city.y, 5.0);
    // Initial layout from a character-count estimate; the sync lays it out
    // exactly from the measured text on the next frame.
    let est = estimate_text_width(&banner_text(map, city));
    let layout = label_layout(est, is_capital);
    let part = |piece| LabelPart {
        city: entity,
        piece,
    };
    commands
        .spawn((
            Transform::from_xyz(origin.x, origin.y, z),
            Visibility::default(),
            CityLabelBack(entity),
        ))
        .with_children(|label| {
            label
                .spawn((
                    Sprite {
                        color: civ_color,
                        custom_size: Some(Vec2::new(BADGE_W, LABEL_H)),
                        ..default()
                    },
                    Transform::from_xyz(layout.badge, 0.0, 0.0),
                    part(LabelPiece::Badge),
                ))
                .with_children(|badge| {
                    badge.spawn((
                        Text2d::new(city.size.to_string()),
                        TextFont {
                            font: font.clone(),
                            font_size: 22.0,
                            ..default()
                        },
                        TextColor(CIV_BADGE_INK),
                        Transform::from_xyz(0.0, 0.0, 0.01),
                        CityBadgeText(entity),
                    ));
                });
            label.spawn((
                Sprite {
                    color: LABEL_FILL,
                    custom_size: Some(Vec2::new(layout.box_w, LABEL_H)),
                    ..default()
                },
                Transform::from_xyz(layout.text, 0.0, 0.0),
                part(LabelPiece::Box),
            ));
            label.spawn((
                Text2d::new(banner_text(map, city)),
                TextFont {
                    font: font.clone(),
                    font_size: LABEL_FONT,
                    ..default()
                },
                TextColor(LABEL_INK),
                TextLayout::new_with_justify(Justify::Center),
                Transform::from_xyz(layout.text, 0.0, 0.02),
                CityBanner(entity),
                part(LabelPiece::Text),
            ));
            if is_capital {
                label
                    .spawn((
                        Sprite {
                            color: civ_color,
                            custom_size: Some(Vec2::new(STAR_W, LABEL_H)),
                            ..default()
                        },
                        Transform::from_xyz(layout.star, 0.0, 0.0),
                        part(LabelPiece::Star),
                    ))
                    .with_children(|badge| {
                        badge.spawn((
                            Sprite {
                                image: star.0.clone(),
                                custom_size: Some(Vec2::new(20.0, 20.0)),
                                ..default()
                            },
                            Transform::from_xyz(0.0, 0.0, 0.01),
                        ));
                    });
            }
        });
}

/// Piece centers (x, relative to the label center) and the text box width.
#[derive(Debug, PartialEq)]
struct LabelLayout {
    badge: f32,
    text: f32,
    star: f32,
    box_w: f32,
}

/// Badge, text box and (for the capital) star box, abutting, with the whole
/// row centered on the city.
fn label_layout(text_w: f32, is_capital: bool) -> LabelLayout {
    let box_w = text_w + 2.0 * TEXT_PAD;
    let star_w = if is_capital { STAR_W } else { 0.0 };
    let left = -(BADGE_W + box_w + star_w) / 2.0;
    LabelLayout {
        badge: left + BADGE_W / 2.0,
        text: left + BADGE_W + box_w / 2.0,
        star: left + BADGE_W + box_w + STAR_W / 2.0,
        box_w,
    }
}

/// Fallback text width before the first layout lands: ~7.5 px per
/// character of the longest line at 14 px.
fn estimate_text_width(s: &str) -> f32 {
    s.lines().map(|l| l.chars().count()).max().unwrap_or(0) as f32 * 7.5
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
    turn: Res<units::Turn>,
    civs: Res<Civilizations>,
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
    // Only the civ whose turn it is may settle, and only with its own
    // settler: the hotseat player never founds for another civ.
    if u.utype != UnitType::Settler || u.civ != civs.active {
        return;
    }
    let civ = u.civ;
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
        civ,
        name: next_name(&mut names, civ),
        x,
        y,
        size: 1,
        food: 0,
        shields: 0,
        production: Production::Warrior,
        queue: vec![],
        buildings: vec![],
        worked: HashSet::new(),
        culture: 0,
        founded: turn.0,
    };
    let taken = taken_tiles(cities.iter(), (x, y));
    governor_assign(&map, &mut city, &taken);
    let entity = commands.spawn(city.clone()).id();
    let is_capital = capital.0[civ].is_none();
    if is_capital {
        capital.0[civ] = Some(entity);
    }
    spawn_city_visuals(
        &mut commands,
        &art,
        &assets,
        &star,
        &map,
        entity,
        &city,
        is_capital,
    );
    commands.spawn(AudioPlayer(audio.build.clone()));
}

pub fn end_turn_cities(
    mut commands: Commands,
    mut ended: MessageReader<CivilizationEnded>,
    map: Res<GameMap>,
    mut cities: Query<(Entity, &mut City)>,
    art: Res<UnitArt>,
    audio: Res<GameAudio>,
    mut board: ResMut<MessageBoard>,
    capital: Res<Capital>,
    mut treasury: ResMut<Treasury>,
    mut prompts: ResMut<crate::production_prompt::ProductionPrompts>,
    mut view: ResMut<CityView>,
    mut menu: ResMut<BuildMenu>,
) {
    // One civ's cities grow, build and pay taxes when that civ hands the
    // hotseat over; the other civs' cities wait for their own turn.
    for ev in ended.read() {
        let civ = ev.0;
        // The outgoing civ's city screen closes with its turn.
        if view
            .0
            .is_some_and(|e| cities.get(e).is_ok_and(|(_, c)| c.civ == civ))
        {
            view.0 = None;
            menu.0 = false;
        }
        let order: Vec<Entity> = cities
            .iter()
            .filter(|(_, c)| c.civ == civ)
            .map(|(e, _)| e)
            .collect();
        for e in order {
            // Later cities see the tiles earlier ones claimed this turn,
            // and any civ's city keeps a tile out of another's radius.
            let me = cities.get(e).map(|(_, c)| (c.x, c.y)).unwrap();
            let taken = taken_tiles(cities.iter().map(|(_, c)| c), me);
            let Ok((_, mut city)) = cities.get_mut(e) else {
                continue;
            };
            // Culture and taxes accrue every turn, like the rest of the
            // turn's bookkeeping, each into its own civ's coffers.
            let tax = accrue(&map, &mut city, capital.0[civ] == Some(e));
            treasury.0[civ] = treasury.0[civ].saturating_add(tax);
            let completed = city.production;
            for event in process_city_turn(&map, &mut city, &taken) {
                match event {
                    CityEvent::Completed(unit) => {
                        // The decision waits for the civ's next turn; the
                        // unit itself joins the civ that built it.
                        prompts.push(e, civ, completed);
                        units::spawn_unit(&mut commands, &art, unit, city.x, city.y, civ);
                        audio::sfx(&mut commands, &audio, "WhatToBuild");
                        post(
                            &mut board,
                            format!("{} completes {}.", city.name, units::def(unit).name),
                        );
                    }
                    CityEvent::Built(p) => {
                        prompts.push(e, civ, p);
                        audio::sfx(&mut commands, &audio, "WhatToBuild");
                        post(&mut board, format!("{} completes {}.", city.name, p.name()));
                    }
                    CityEvent::TooSmall => post(
                        &mut board,
                        format!(
                            "{} must reach size {SETTLER_MIN_SIZE} to finish its Settler.",
                            city.name
                        ),
                    ),
                    CityEvent::Grew => post(
                        &mut board,
                        format!("{} grows to size {}.", city.name, city.size),
                    ),
                    CityEvent::Starved => post(
                        &mut board,
                        format!("{} starves to size {}.", city.name, city.size),
                    ),
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
    mut sprites: Query<(&CitySprite, &mut Sprite), Without<LabelPart>>,
    mut banners: Query<(&CityBanner, &mut Text2d), Without<CityBadgeText>>,
    layouts: Query<(&CityBanner, &TextLayoutInfo)>,
    mut badge_texts: Query<(&CityBadgeText, &mut Text2d), Without<CityBanner>>,
    mut parts: Query<(&LabelPart, &mut Transform, Option<&mut Sprite>), Without<CitySprite>>,
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
    // Lay the pieces out around the label center from the measured text.
    for (part, mut tf, sprite) in parts.iter_mut() {
        let Ok(city) = cities.get(part.city) else {
            continue;
        };
        let w = layouts
            .iter()
            .find(|(b, _)| b.0 == part.city)
            .map(|(_, l)| l.size.x)
            .filter(|w| *w >= 1.0)
            .unwrap_or_else(|| estimate_text_width(&banner_text(&map, city)));
        let layout = label_layout(w, capital.0[city.civ] == Some(part.city));
        match part.piece {
            LabelPiece::Badge => tf.translation.x = layout.badge,
            LabelPiece::Star => tf.translation.x = layout.star,
            LabelPiece::Text => tf.translation.x = layout.text,
            LabelPiece::Box => {
                tf.translation.x = layout.text;
                if let Some(mut sprite) = sprite {
                    sprite.custom_size = Some(Vec2::new(layout.box_w, LABEL_H));
                }
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
    mut bars: Query<(&CityLabelBack, &mut Visibility), (Without<CitySprite>, Without<CityBanner>)>,
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
    /// Step to the previous or next city, as the top bar's arrows do.
    PrevCity,
    NextCity,
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
        TextFont {
            font: font.clone(),
            font_size: size,
            ..default()
        },
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
            txt(
                m,
                font,
                &format!("What should {} build?", city.name),
                20.0,
                16.0,
                10.0,
                ink,
            );
            m.spawn((
                Button,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(520.0),
                    top: Val::Px(8.0),
                    padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.35, 0.22, 0.1)),
                ScreenButton::CloseMenu,
            ))
            .with_children(|b| {
                b.spawn((
                    Text::new("Close"),
                    TextFont {
                        font: font.clone(),
                        font_size: 16.0,
                        ..default()
                    },
                    TextColor(ink),
                ));
            });
            for (n, p) in items.iter().enumerate() {
                let top = 44.0 + n as f32 * 48.0;
                let have = if *p == city.production {
                    city.shields
                } else {
                    0
                };
                m.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(12.0),
                        top: Val::Px(top + 2.0),
                        width: Val::Px(42.0),
                        height: Val::Px(42.0),
                        ..default()
                    },
                    build_icon(assets, *p),
                ));
                let mark = if *p == city.production {
                    "  [building]"
                } else {
                    ""
                };
                txt(
                    m,
                    font,
                    &format!(
                        "{}{}   cost {}  ({})",
                        p.name(),
                        mark,
                        p.cost(),
                        turns_for(p.cost(), have, rate)
                    ),
                    17.0,
                    64.0,
                    top + 2.0,
                    ink,
                );
                txt(
                    m,
                    font,
                    p.blurb(),
                    13.0,
                    64.0,
                    top + 24.0,
                    Color::srgb(0.75, 0.7, 0.55),
                );
                for (label, left, kind) in [
                    ("Build now", 400.0, ScreenButton::Pick(*p)),
                    ("Queue", 500.0, ScreenButton::Queue(*p)),
                ] {
                    m.spawn((
                        Button,
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(left),
                            top: Val::Px(top + 6.0),
                            width: Val::Px(90.0),
                            height: Val::Px(30.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.35, 0.22, 0.1)),
                        kind,
                    ))
                    .with_children(|b| {
                        b.spawn((
                            Text::new(label),
                            TextFont {
                                font: font.clone(),
                                font_size: 15.0,
                                ..default()
                            },
                            TextColor(ink),
                        ));
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
pub fn cluster_pick(win_w: f32, win_h: f32, cursor: Vec2) -> Option<(i32, i32)> {
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

/// Radius offsets of the 21 city tiles.
fn radius_offsets() -> impl Iterator<Item = (i32, i32)> {
    (-2i32..=2)
        .flat_map(|ry| (-2i32..=2).map(move |rx| (rx, ry)))
        .filter(|(rx, ry)| !(rx.abs() == 2 && ry.abs() == 2))
}

/// Ground under the radius: the blended corner cells, as on the map
/// (`blend::corner_cell`), then black diamonds over everything the cells
/// spill onto outside the radius and over unexplored tiles, and a dimming
/// diamond over tiles another city works. The cells straddle tiles, so a
/// per-tile tint cannot do this.
fn ground_nodes(cluster: &mut ChildSpawnerCommands<'_>, ctx: &ClusterCtx) {
    let ClusterCtx {
        assets,
        map,
        city,
        taken,
        ..
    } = *ctx;
    let inside: HashSet<(i32, i32)> = radius_offsets().collect();
    let box_at = |rx: i32, ry: i32, dy: f32| Node {
        position_type: PositionType::Absolute,
        left: Val::Px((rx - ry) as f32 * 64.0 + 320.0 - 64.0),
        top: Val::Px((rx + ry) as f32 * 32.0 + 160.0 - 32.0 + dy),
        width: Val::Px(128.0),
        height: Val::Px(64.0),
        ..default()
    };
    // Cells with a radius tile on a vertex: a tile is the N, E, W and S
    // vertex of cells (x, y), (x-1, y), (x, y-1) and (x-1, y-1). Sorted
    // north to south, though the cells never overlap.
    let mut keys: Vec<(i32, i32)> = inside
        .iter()
        .flat_map(|&(rx, ry)| [(rx - 1, ry - 1), (rx, ry - 1), (rx - 1, ry), (rx, ry)])
        .collect();
    keys.sort_by_key(|&(rx, ry)| (rx + ry, rx));
    keys.dedup();
    for (rx, ry) in keys {
        let (stem, col, row) = crate::blend::corner_cell(map, city.x + rx, city.y + ry);
        let mut n = ImageNode::new(assets.load(crate::blend::sheet_path(stem)));
        n.rect = Some(crate::blend::cell_rect(col, row));
        // Cell (x, y) is centered on the tile's south corner.
        cluster.spawn((n, box_at(rx, ry, 32.0)));
    }
    let fog = assets.load(crate::render::FOG_SHEET);
    let mut mask = |rx: i32, ry: i32, color: Color| {
        let mut n = ImageNode::new(fog.clone());
        n.rect = Some(crate::blend::cell_rect(0, 0));
        n.color = color;
        cluster.spawn((n, box_at(rx, ry, 0.0)));
    };
    for ry in -3i32..=3 {
        for rx in -3i32..=3 {
            let ny = city.y + ry;
            let nx = map.wrap_x(city.x + rx);
            let tile = map.get(nx, ny);
            if !inside.contains(&(rx, ry)) || !tile.is_some_and(|t| t.seen) {
                mask(rx, ry, Color::BLACK);
            } else if (rx, ry) != (0, 0) && taken.contains(&(nx, ny)) {
                mask(rx, ry, Color::srgba(0.0, 0.0, 0.0, 0.55));
            }
        }
    }
}

fn tile_node(cluster: &mut ChildSpawnerCommands<'_>, cx_: &ClusterCtx, rx: i32, ry: i32) {
    let ClusterCtx {
        assets,
        tiles,
        imp,
        city_art,
        map,
        city,
        taken,
        hubs,
    } = *cx_;
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
            if t.base == crate::map::Base::Ice {
                let (mut n, node) = full(tiles.defs[&base].image.clone());
                n.color = tint;
                tile.spawn((n, node));
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
    let ClusterCtx {
        assets, map, city, ..
    } = *ctx;
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

fn build_city_screen(commands: &mut Commands, ctx: &ClusterCtx, p: &PanelCtx, menu: bool) {
    let ClusterCtx {
        assets, map, city, ..
    } = *ctx;
    let font = assets.load("gen/fonts/lsans.ttf");
    let shields_pt = city_income(map, city).1;
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
                city_panel(content, ctx, p, &font);
                // right panel over the black view: production queue
                content.spawn((
                    ImageNode::new(assets.load("gen/cityscreen/ProductionQueueBox.png")),
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(812.0),
                        top: Val::Px(98.0),
                        width: Val::Px(203.0),
                        height: Val::Px(360.0),
                        ..default()
                    },
                ));
                txt(
                    content,
                    &font,
                    "Production queue",
                    15.0,
                    830.0,
                    106.0,
                    PARCHMENT_TEXT,
                );
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
                                        left: Val::Px(826.0),
                                        top: Val::Px(top),
                                        width: Val::Px(176.0),
                                        height: Val::Px(26.0),
                                        align_items: AlignItems::Center,
                                        ..default()
                                    },
                                    BackgroundColor(Color::srgba(0.4, 0.25, 0.1, 0.25)),
                                    ScreenButton::Unqueue(*i),
                                ))
                                .with_children(|b| {
                                    b.spawn((
                                        Text::new(format!("{label}  [x]")),
                                        TextFont {
                                            font: font.clone(),
                                            font_size: 14.0,
                                            ..default()
                                        },
                                        TextColor(PARCHMENT_TEXT),
                                    ));
                                });
                        }
                        None => txt(
                            content,
                            &font,
                            label,
                            16.0,
                            830.0,
                            top + 2.0,
                            PARCHMENT_TEXT,
                        ),
                    }
                }
                // modal build list
                if menu {
                    build_menu(content, assets, &font, city, shields_pt);
                }
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
        (
            t.road,
            t.irrigation,
            t.mine,
            t.seen,
            t.cover as u8,
            t.resource,
        )
            .hash(&mut h);
    }
    h.finish()
}

/// What the city panel shows that is not part of the city itself.
struct PanelCtx<'a> {
    capital: bool,
    treasury: u32,
    turn: u32,
    garrison: &'a [UnitType],
    strategic: &'a [(u8, u32)],
    luxuries: &'a [(u8, u32)],
}

/// Panel geometry, in the 1024x768 content box: the city view band runs
/// from the top bar down to the bottom panel, whose bar rows sit where
/// `background.png` bakes them.
const BAND_TOP: f32 = 92.0;
const BAR_PROD: f32 = 522.0;
const BAR_FOOD: f32 = 568.0;
const BAR_COM: f32 = 618.0;
const COM_ROW: f32 = 46.0;
const INK: Color = PARCHMENT_TEXT;
const INK_SOFT: Color = Color::srgb(0.42, 0.30, 0.16);
const LABEL_TEXT: f32 = 14.0;
const SMALL_TEXT: f32 = 13.0;
/// The clone has no government choice: every civ reads despotism on the
/// city screen's top bar.
const GOVERNMENT: &str = "DESPOTISM";

/// `CityIcons.png` cell indices, as used across the city screen.
const ICON_SHIELD: u32 = 4;
const ICON_FOOD: u32 = 6;
const ICON_BOX: u32 = 9;
const ICON_UPKEEP: u32 = 17;
const ICON_CULTURE: u32 = 18;
const ICON_HAPPY: u32 = 19;
const ICON_SHIELD_BOX: u32 = 8;
const ICON_FOOD_BOX: u32 = 23;
const ICON_TREASURY: u32 = 24;
const ICON_FLASK: u32 = 16;

/// A `CityIcons.png` cell (25 cells of 31 px) drawn at a pixel size.
fn icon(
    parent: &mut ChildSpawnerCommands<'_>,
    assets: &AssetServer,
    cell: u32,
    x: f32,
    y: f32,
    w: f32,
    tint: Color,
) {
    let mut n = ImageNode::new(assets.load("gen/cityscreen/CityIcons.png"));
    let cx = 1.0 + cell as f32 * 31.0;
    n.rect = Some(Rect::new(cx, 1.0, cx + 30.0, 31.0));
    n.color = tint;
    parent.spawn((
        n,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(x),
            top: Val::Px(y),
            width: Val::Px(w),
            height: Val::Px(w),
            ..default()
        },
    ));
}

/// A good's terrain icon, at any size.
fn good_icon(
    parent: &mut ChildSpawnerCommands<'_>,
    assets: &AssetServer,
    id: u8,
    x: f32,
    y: f32,
    w: f32,
) {
    let path = format!("gen/features/{}.png", crate::features::art_key(id));
    parent.spawn((
        ImageNode::new(assets.load(path)),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(x),
            top: Val::Px(y),
            width: Val::Px(w),
            height: Val::Px(w),
            ..default()
        },
    ));
}

/// A row of per-turn icons, the last one ending at `right`.
fn icons_right(
    content: &mut ChildSpawnerCommands<'_>,
    assets: &AssetServer,
    cell: u32,
    count: u32,
    pitch: f32,
    right: f32,
    y: f32,
    w: f32,
) {
    for i in 0..count {
        let x = right - w - (count - 1 - i) as f32 * pitch;
        icon(content, assets, cell, x, y, w, Color::WHITE);
    }
}

/// A row of per-turn icons packed from `left`, overlapping when there are
/// more than `pitch` allows (Civ3 packs its food row the same way).
fn icons_left(
    content: &mut ChildSpawnerCommands<'_>,
    assets: &AssetServer,
    cell: u32,
    count: u32,
    pitch: f32,
    left: f32,
    y: f32,
    w: f32,
) {
    for i in 0..count {
        icon(
            content,
            assets,
            cell,
            left + i as f32 * pitch,
            y,
            w,
            Color::WHITE,
        );
    }
}

/// The 32-px build and garrison icon of a unit type.
fn unit_icon(t: UnitType) -> String {
    let stem = match t {
        UnitType::Warrior => "warrior",
        UnitType::Scout => "scout",
        UnitType::Settler => "settler",
        UnitType::Worker => "worker",
    };
    format!("gen/ui/uniticon_{stem}.png")
}

#[derive(Component)]
pub(crate) struct PanelButton {
    stem: &'static str,
}

/// The city `step` places from `current`, so the top bar's arrows walk the
/// civ's cities and wrap at either end.
fn step_city(all: &[Entity], current: Entity, step: i32) -> Entity {
    let at = all.iter().position(|c| *c == current).unwrap_or(0) as i32;
    all[(at + step).rem_euclid(all.len() as i32) as usize]
}

fn panel_button(
    content: &mut ChildSpawnerCommands<'_>,
    assets: &AssetServer,
    stem: &'static str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    kind: ScreenButton,
) {
    content.spawn((
        Button,
        ImageNode::new(assets.load(format!("gen/ui/{stem}_0.png"))),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(x),
            top: Val::Px(y),
            width: Val::Px(w),
            height: Val::Px(h),
            ..default()
        },
        PanelButton { stem },
        kind,
    ));
}

/// Hover and press art for the panel's stateful buttons.
pub fn update_panel_buttons(
    assets: Res<AssetServer>,
    mut buttons: Query<(&Interaction, &PanelButton, &mut ImageNode), Changed<Interaction>>,
) {
    for (interaction, button, mut img) in buttons.iter_mut() {
        let state = match interaction {
            Interaction::Pressed => 2,
            Interaction::Hovered => 1,
            Interaction::None => 0,
        };
        img.image = assets.load(format!("gen/ui/{}_{state}.png", button.stem));
    }
}

/// Civ3's city screen: the top bar's civ and city readouts, the city's own
/// land in the middle, and the bottom panel's improvements, luxuries,
/// garrison and production, food and commerce rows.
fn city_panel(
    content: &mut ChildSpawnerCommands<'_>,
    ctx: &ClusterCtx,
    p: &PanelCtx,
    font: &Handle<Font>,
) {
    let ClusterCtx {
        assets, map, city, ..
    } = *ctx;
    let (net_food, shields_pt) = city_income(map, city);
    let (food, _) = city_yields(map, city);
    let commerce = city_commerce(map, city);
    let (tax, sci, lux) = commerce_split(commerce);
    let culture_pt = culture_per_turn(city, p.capital);
    let (culture_cur, culture_next) = culture_thresholds(city.culture);

    // ---- top bar: the civ's resources, then the city's own readout ----
    txt(
        content,
        font,
        "STRATEGIC RESOURCES",
        LABEL_TEXT,
        14.0,
        6.0,
        INK,
    );
    for (i, (id, n)) in p.strategic.iter().take(6).enumerate() {
        let x = 16.0 + i as f32 * 40.0;
        good_icon(content, assets, *id, x, 24.0, 32.0);
        content.spawn((
            Text::new(n.to_string()),
            TextFont {
                font: font.clone(),
                font_size: SMALL_TEXT,
                ..default()
            },
            TextColor(INK),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(x),
                top: Val::Px(56.0),
                width: Val::Px(32.0),
                ..default()
            },
            TextLayout::new_with_justify(Justify::Center),
        ));
    }
    for (text, size, y) in [
        (city.name.clone(), 26.0, 4.0),
        (format!("Founded: Turn {}", city.founded), SMALL_TEXT, 32.0),
    ] {
        content.spawn((
            Text::new(text),
            TextFont {
                font: font.clone(),
                font_size: size,
                ..default()
            },
            TextColor(INK),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(305.0),
                top: Val::Px(y),
                width: Val::Px(400.0),
                ..default()
            },
            TextLayout::new_with_justify(Justify::Center),
        ));
    }
    let gold = format!("{} GOLD", p.treasury);
    let pop = format!("POP {}", city.size);
    let date = format!("Turn {}", p.turn);
    for (i, s) in [gold.as_str(), GOVERNMENT, pop.as_str(), date.as_str()]
        .iter()
        .enumerate()
    {
        let left_col = i % 2 == 0;
        content.spawn((
            Text::new(*s),
            TextFont {
                font: font.clone(),
                font_size: SMALL_TEXT,
                ..default()
            },
            TextColor(INK),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(if left_col { 305.0 } else { 525.0 }),
                top: Val::Px(46.0 + (i / 2) as f32 * 16.0),
                width: Val::Px(if left_col { 173.0 } else { 160.0 }),
                ..default()
            },
            TextLayout::new_with_justify(if left_col {
                Justify::Right
            } else {
                Justify::Left
            }),
        ));
    }
    txt(content, font, "CULTURE", 15.0, 712.0, 4.0, INK);
    icon(
        content,
        assets,
        ICON_CULTURE,
        786.0,
        4.0,
        16.0,
        Color::WHITE,
    );
    txt(
        content,
        font,
        &format!("{culture_pt} per turn"),
        SMALL_TEXT,
        806.0,
        7.0,
        INK,
    );
    let expand = if culture_pt == 0 {
        "never".to_string()
    } else {
        format!(
            "{} turns",
            (culture_next - city.culture).div_ceil(culture_pt)
        )
    };
    content.spawn((
        Text::new(format!("Expand in {expand}")),
        TextFont {
            font: font.clone(),
            font_size: 12.0,
            ..default()
        },
        TextColor(INK),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(724.0),
            top: Val::Px(35.0),
            width: Val::Px(110.0),
            ..default()
        },
        TextLayout::new_with_justify(Justify::Center),
    ));
    txt(
        content,
        font,
        &format!("Total: {}/{culture_cur}", city.culture),
        SMALL_TEXT,
        720.0,
        55.0,
        INK,
    );
    panel_button(
        content,
        assets,
        "mgmt_prev",
        359.0,
        26.0,
        49.0,
        60.0,
        ScreenButton::PrevCity,
    );
    panel_button(
        content,
        assets,
        "mgmt_next",
        604.0,
        26.0,
        49.0,
        60.0,
        ScreenButton::NextCity,
    );
    panel_button(
        content,
        assets,
        "x",
        946.0,
        18.0,
        44.0,
        46.0,
        ScreenButton::Close,
    );

    // ---- the city's land, with the citizens who work it ----
    content.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(BAND_TOP),
            width: Val::Px(CONTENT_W),
            height: Val::Px(413.0),
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
            ground_nodes(cluster, ctx);
            for (rx, ry) in radius_offsets() {
                tile_node(cluster, ctx, rx, ry);
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
    // Citizens: the ones on worked tiles first, then idle entertainers.
    let busy = city.worked.len().min(city.size as usize);
    for i in 0..(city.size as usize).min(16) {
        let head = if i < busy {
            "gen/ui/citizen.png"
        } else {
            "gen/ui/entertainer.png"
        };
        content.spawn((
            ImageNode::new(assets.load(head)),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(176.0 + i as f32 * 52.0),
                top: Val::Px(440.0),
                width: Val::Px(50.0),
                height: Val::Px(50.0),
                ..default()
            },
        ));
    }
    content.spawn((
        ImageNode::new(assets.load("gen/ui/TopFadeBar.png")),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(BAND_TOP),
            width: Val::Px(CONTENT_W),
            ..default()
        },
    ));
    content.spawn((
        ImageNode::new(assets.load("gen/ui/BottomFadeBar.png")),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(485.0),
            width: Val::Px(CONTENT_W),
            ..default()
        },
    ));

    // ---- improvements: icon, name, then what the building does ----
    txt(content, font, "IMPROVEMENTS", LABEL_TEXT, 12.0, 511.0, INK);
    let mut rows: Vec<(&str, Rect, u32, u8, u8)> = vec![];
    if p.capital {
        // The Palace is the capital's; the clone does not build one.
        rows.push(("Palace", Rect::new(33.0, 33.0, 65.0, 65.0), 1, 0, 0));
    }
    for b in city.buildings.iter() {
        if let Some(rect) = b.building_rect() {
            rows.push((b.name(), rect, b.culture(), b.upkeep(), b.happy()));
        }
    }
    for (i, (name, rect, culture, upkeep, happy)) in rows.iter().enumerate() {
        let y = 522.0 + i as f32 * 32.0;
        let mut n = ImageNode::new(assets.load("gen/cityscreen/buildings-small.png"));
        n.rect = Some(*rect);
        content.spawn((
            n,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(4.0),
                top: Val::Px(y),
                width: Val::Px(32.0),
                height: Val::Px(32.0),
                ..default()
            },
        ));
        txt(content, font, name, LABEL_TEXT, 38.0, y + 4.0, INK);
        let mut mx = 104.0;
        if *upkeep > 0 {
            icon(
                content,
                assets,
                ICON_UPKEEP,
                mx,
                y + 8.0,
                16.0,
                Color::WHITE,
            );
            mx += 18.0;
        }
        for _ in 0..*culture {
            icon(
                content,
                assets,
                ICON_CULTURE,
                mx,
                y + 8.0,
                16.0,
                Color::WHITE,
            );
            mx += 14.0;
        }
        for _ in 0..*happy {
            icon(
                content,
                assets,
                ICON_HAPPY,
                mx + 2.0,
                y + 8.0,
                16.0,
                Color::WHITE,
            );
            mx += 18.0;
        }
    }
    // The list's scrollbar. The clone owns at most four improvements, so
    // there is never anything to scroll: it is the panel's chrome.
    for (art, y) in [("scroll_up_0", 509.0), ("scroll_down_0", 748.0)] {
        content.spawn((
            ImageNode::new(assets.load(format!("gen/ui/{art}.png"))),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(140.0),
                top: Val::Px(y),
                width: Val::Px(18.0),
                height: Val::Px(16.0),
                ..default()
            },
        ));
    }
    for i in 0..11 {
        content.spawn((
            ImageNode::new(assets.load("gen/ui/scroll_track.png")),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(143.0),
                top: Val::Px(528.0 + i as f32 * 20.0),
                width: Val::Px(12.0),
                height: Val::Px(20.0),
                ..default()
            },
        ));
    }

    // ---- luxuries, and the rest of the left column ----
    txt(content, font, "LUXURIES", LABEL_TEXT, 154.0, 511.0, INK);
    for (i, (id, n)) in p.luxuries.iter().take(6).enumerate() {
        let y = 524.0 + i as f32 * 31.0;
        txt(
            content,
            font,
            &format!("({n})"),
            SMALL_TEXT,
            154.0,
            y + 4.0,
            INK,
        );
        good_icon(content, assets, *id, 186.0, y, 22.0);
        // Civ3 makes a citizen happy per pair of sources; the mood engine
        // is not modelled yet, so this is the display rule for now.
        for j in 0..(n / 2).min(4) {
            icon(
                content,
                assets,
                ICON_HAPPY,
                216.0 + j as f32 * 24.0,
                y,
                22.0,
                Color::WHITE,
            );
        }
    }
    txt(content, font, "POLLUTION", LABEL_TEXT, 154.0, 716.0, INK);
    // ---- garrison: the units standing in the city ----
    txt(content, font, "GARRISON", LABEL_TEXT, 300.0, 716.0, INK);
    for (i, u) in p.garrison.iter().take(3).enumerate() {
        content.spawn((
            ImageNode::new(assets.load(unit_icon(*u))),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(300.0 + i as f32 * 40.0),
                top: Val::Px(732.0),
                width: Val::Px(32.0),
                height: Val::Px(32.0),
                ..default()
            },
        ));
    }

    // ---- production, food and commerce ----
    txt(content, font, "PRODUCTION", LABEL_TEXT, 296.0, 504.0, INK);
    icon(
        content,
        assets,
        ICON_SHIELD,
        374.0,
        506.0,
        16.0,
        Color::WHITE,
    );
    txt(
        content,
        font,
        &format!("{shields_pt} per turn"),
        SMALL_TEXT,
        394.0,
        507.0,
        INK,
    );
    icons_right(
        content,
        assets,
        ICON_SHIELD,
        shields_pt as u32,
        30.0,
        832.0,
        BAR_PROD,
        24.0,
    );
    txt(content, font, "FOOD", LABEL_TEXT, 296.0, 550.0, INK);
    icon(
        content,
        assets,
        ICON_FOOD_BOX,
        332.0,
        552.0,
        16.0,
        Color::WHITE,
    );
    txt(
        content,
        font,
        &format!("{food} per turn"),
        SMALL_TEXT,
        352.0,
        553.0,
        INK,
    );
    icons_left(
        content,
        assets,
        ICON_FOOD,
        food as u32,
        9.0,
        300.0,
        BAR_FOOD + 6.0,
        14.0,
    );
    // The growth box: one icon per ten food still needed.
    let need = FOOD_BOX.saturating_sub(city.food) as u32;
    icons_right(
        content,
        assets,
        ICON_FOOD_BOX,
        need.div_ceil(10),
        30.0,
        767.0,
        BAR_FOOD,
        20.0,
    );
    let grow_in = if net_food <= 0 {
        "never".to_string()
    } else {
        format!(
            "{} turns",
            ceil_div(FOOD_BOX.saturating_sub(city.food), net_food as u8)
        )
    };
    txt(
        content,
        font,
        &format!("Growth in {grow_in}"),
        12.0,
        790.0,
        554.0,
        INK_SOFT,
    );
    txt(content, font, "COMMERCE", LABEL_TEXT, 296.0, 600.0, INK);
    icon(content, assets, ICON_FOOD, 378.0, 602.0, 16.0, Color::WHITE);
    txt(
        content,
        font,
        &format!("{commerce} per turn"),
        SMALL_TEXT,
        396.0,
        603.0,
        INK,
    );
    for (row, (amount, rate, icon_cell)) in [
        (tax, TAX_RATE, ICON_TREASURY),
        (sci, SCI_RATE, ICON_FLASK),
        (lux, LUX_RATE, ICON_HAPPY),
    ]
    .iter()
    .enumerate()
    {
        let y = BAR_COM + row as f32 * COM_ROW;
        icons_right(
            content,
            assets,
            ICON_FOOD,
            *amount as u32,
            19.3,
            720.0,
            y + 4.0,
            20.0,
        );
        icon(
            content,
            assets,
            *icon_cell,
            692.0,
            y + 4.0,
            22.0,
            Color::WHITE,
        );
        txt(
            content,
            font,
            &format!("{amount} ({}%)", rate * 10),
            SMALL_TEXT,
            722.0,
            y + 6.0,
            INK_SOFT,
        );
    }

    // ---- the boxes: food, the granary's store, and the build's shields ----
    for i in 0..FOOD_BOX as usize {
        let stored = i >= FOOD_BOX as usize - city.food as usize;
        icon(
            content,
            assets,
            if stored { ICON_FOOD_BOX } else { ICON_BOX },
            799.0 + (i % 4) as f32 * 18.0,
            574.0 + (i / 4) as f32 * 18.0,
            18.0,
            Color::WHITE,
        );
    }
    if city.has(Production::Granary) {
        let row = (FOOD_BOX as usize - city.food as usize) / 4;
        txt(
            content,
            font,
            "GRANARY",
            SMALL_TEXT,
            792.0,
            576.0 + row as f32 * 18.0,
            INK,
        );
    }
    for i in 0..city.production.cost() as usize {
        let tint = if i < city.shields as usize {
            Color::srgb(1.0, 0.82, 0.36)
        } else {
            Color::WHITE
        };
        icon(
            content,
            assets,
            ICON_SHIELD_BOX,
            918.0 + (i % 7) as f32 * 12.0,
            624.0 + (i / 7) as f32 * 12.0,
            12.0,
            tint,
        );
    }
    // The current build sits in Civ3's production button; clicking it
    // opens the build list.
    content
        .spawn((
            Button,
            ImageNode::new(assets.load("gen/ui/prod_0.png")),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(890.0),
                top: Val::Px(514.0),
                width: Val::Px(133.0),
                height: Val::Px(95.0),
                ..default()
            },
            PanelButton { stem: "prod" },
            ScreenButton::Change,
        ))
        .with_children(|box_| {
            box_.spawn((
                build_icon(assets, city.production),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(28.0),
                    top: Val::Px(2.0),
                    width: Val::Px(76.0),
                    height: Val::Px(76.0),
                    ..default()
                },
            ));
            box_.spawn((
                Text::new(city.production.name()),
                TextFont {
                    font: font.clone(),
                    font_size: 16.0,
                    ..default()
                },
                TextColor(INK),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(6.0),
                    top: Val::Px(70.0),
                    width: Val::Px(121.0),
                    ..default()
                },
                TextLayout::new_with_justify(Justify::Center),
            ));
        });
    let left = city.production.cost().saturating_sub(city.shields);
    let prod_in = if shields_pt == 0 {
        "never".to_string()
    } else {
        format!("{} turns", ceil_div(left, shields_pt))
    };
    txt(
        content,
        font,
        &format!("Complete in {prod_in}"),
        SMALL_TEXT,
        898.0,
        612.0,
        INK,
    );
    // ---- the city's own buttons ----
    for (label, y, kind) in [
        ("Change build", 668.0, ScreenButton::Change),
        ("Governor", 668.0, ScreenButton::Governor),
    ] {
        let x = if matches!(kind, ScreenButton::Governor) {
            85.0
        } else {
            0.0
        };
        content
            .spawn((
                Button,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(2.0 + x),
                    top: Val::Px(y),
                    width: Val::Px(80.0),
                    height: Val::Px(26.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(Color::srgb(0.72, 0.63, 0.44)),
                kind,
            ))
            .with_children(|btn| {
                btn.spawn((
                    Text::new(label),
                    TextFont {
                        font: font.clone(),
                        font_size: 14.0,
                        ..default()
                    },
                    TextColor(INK),
                ));
            });
    }
}

/// Asset resources the city screen draws with: bundled so the system stays
/// inside Bevy's parameter limit.
#[derive(SystemParam)]
pub(crate) struct ScreenArt<'w> {
    assets: Res<'w, AssetServer>,
    tiles: Res<'w, TileArt>,
    imp: Res<'w, ImprovementArt>,
    city_art: Res<'w, CityArt>,
}

pub fn maintain_city_screen(
    mut commands: Commands,
    view: Res<CityView>,
    cities: Query<&City>,
    changed: Query<(), Changed<City>>,
    roots: Query<Entity, With<CityScreenRoot>>,
    mut last: Local<(Option<Entity>, u64)>,
    art: ScreenArt,
    map: Res<GameMap>,
    menu: Res<BuildMenu>,
    units: Query<&Unit>,
    treasury: Res<Treasury>,
    capital: Res<Capital>,
    turn: Res<units::Turn>,
    civs: Res<Civilizations>,
) {
    // Only the active civ's city may be on screen: a view left over from
    // the previous hotseat player never shows their city to the next one.
    let open = view
        .0
        .filter(|e| cities.get(*e).is_ok_and(|c| c.civ == civs.active));
    // Worker jobs finishing or fog lifting change the radius without
    // touching the City, so the tiles' own state is part of the key.
    // The panel also draws the treasury, the garrison and the civ's
    // resources, so those are part of the rebuild key too.
    let open_city = open.and_then(|e| cities.get(e).ok().map(|c| (e, c)));
    let garrison: Vec<UnitType> = open_city
        .map(|(_, c)| {
            units
                .iter()
                .filter(|u| u.x == c.x && u.y == c.y && u.civ == c.civ)
                .map(|u| u.utype)
                .collect()
        })
        .unwrap_or_default();
    let owned: Vec<&City> = cities.iter().collect();
    let (strategic, luxuries) = match open_city {
        Some((_, c)) => resources_owned(&map, &owned, c.civ),
        None => (vec![], vec![]),
    };
    let sig = open_city
        .map(|(_, c)| {
            let mut h = radius_signature(&map, c);
            h ^= (treasury.0[c.civ] as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
            h ^= (garrison.len() as u64) << 32;
            for u in &garrison {
                h = h.rotate_left(7) ^ (*u as u64);
            }
            h ^= ((strategic.len() as u64) << 16) ^ luxuries.len() as u64;
            h
        })
        .unwrap_or(0);
    if (open, sig) == *last && changed.is_empty() && !menu.is_changed() {
        return;
    }
    for r in roots.iter() {
        commands.entity(r).despawn();
    }
    *last = (open, sig);
    let Some((e, city)) = open_city else { return };
    let taken = taken_tiles(cities.iter(), (city.x, city.y));
    let hubs: Vec<(i32, i32)> = cities.iter().map(|c| (c.x, c.y)).collect();
    let ctx = ClusterCtx {
        assets: &art.assets,
        tiles: &art.tiles,
        imp: &art.imp,
        city_art: &art.city_art,
        map: &map,
        city,
        taken: &taken,
        hubs: &hubs,
    };
    let panel = PanelCtx {
        capital: capital.0[city.civ] == Some(e),
        treasury: treasury.0[city.civ],
        turn: turn.0,
        garrison: &garrison,
        strategic: &strategic,
        luxuries: &luxuries,
    };
    build_city_screen(&mut commands, &ctx, &panel, menu.0);
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
    civs: Res<Civilizations>,
) {
    let Some(e) = views.0 else {
        return;
    };
    // Only the active civ's city may be managed: a view left over from
    // the previous hotseat player closes instead of answering clicks.
    if !cities.get(e).is_ok_and(|c| c.civ == civs.active) {
        views.0 = None;
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        views.0 = None;
        return;
    }
    if !buttons.just_pressed(MouseButton::Left) {
        return;
    }
    let hovered = clusters
        .iter()
        .any(|i| matches!(i, Interaction::Pressed | Interaction::Hovered));
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
    if !click_cluster(&map, &mut cities, e, rx, ry) {
        post(
            &mut board,
            "Another city works that tile, or it is unexplored.",
        );
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
    mut cities: Query<(Entity, &mut City)>,
    buttons: Query<(&Interaction, &ScreenButton), Changed<Interaction>>,
    audio: Res<GameAudio>,
    map: Res<GameMap>,
    mut board: ResMut<MessageBoard>,
    civs: Res<Civilizations>,
) {
    for (interaction, button) in buttons.iter() {
        if *interaction != Interaction::Pressed {
            continue;
        }
        audio::sfx(&mut commands, &audio, "Button OK");
        match button {
            ScreenButton::Close => {
                views.0 = None;
                menu.0 = false;
            }
            ScreenButton::Change => menu.0 = true,
            ScreenButton::CloseMenu => menu.0 = false,
            // The arrows walk the active civ's own cities, never another
            // hotseat player's.
            ScreenButton::PrevCity | ScreenButton::NextCity => {
                let all: Vec<Entity> = cities
                    .iter()
                    .filter(|(_, c)| c.civ == civs.active)
                    .map(|(e, _)| e)
                    .collect();
                let step = if matches!(button, ScreenButton::NextCity) {
                    1
                } else {
                    -1
                };
                if let Some(e) = views.0.filter(|_| !all.is_empty()) {
                    menu.0 = false;
                    views.0 = Some(step_city(&all, e, step));
                }
            }
            _ => {
                let taken = views
                    .0
                    .and_then(|e| cities.get(e).ok())
                    .map(|(_, c)| taken_tiles(cities.iter().map(|(_, c)| c), (c.x, c.y)))
                    .unwrap_or_default();
                let Some((_, mut city)) = views.0.and_then(|e| cities.get_mut(e).ok()) else {
                    continue;
                };
                match button {
                    ScreenButton::Governor => governor_assign(&map, &mut city, &taken),
                    ScreenButton::Pick(p) => {
                        city.change_build(*p);
                        menu.0 = false;
                    }
                    ScreenButton::Queue(p) => {
                        if !city.enqueue(*p) {
                            post(&mut board, "Cannot queue that item.");
                        }
                    }
                    ScreenButton::Unqueue(i) => city.dequeue(*i),
                    _ => {}
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
        test_city_of(map, 0)
    }

    fn test_city_of(map: &GameMap, civ: usize) -> City {
        let (x, y) = map.start;
        let mut city = City {
            civ,
            name: CIVS[civ].city_names[0].to_string(),
            x,
            y,
            size: 1,
            food: 0,
            shields: 0,
            production: Production::Warrior,
            queue: vec![],
            buildings: vec![],
            worked: HashSet::new(),
            culture: 0,
            founded: 1,
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
        let map = GameMap {
            w: 3,
            h: 3,
            tiles: vec![tile; 9],
            start: (1, 1),
            seed: 1,
        };
        let mut city = City {
            civ: 0,
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
            culture: 0,
            founded: 1,
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
        assert!(
            events
                .iter()
                .any(|e| matches!(e, CityEvent::Completed(UnitType::Warrior)))
        );
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
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, CityEvent::TooSmall | CityEvent::Completed(_)))
        );
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
        assert!(
            events
                .iter()
                .any(|e| matches!(e, CityEvent::Built(Production::Granary)))
        );
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
        assert_eq!(
            city_yields(&map, &city),
            center_yields(&map, city.x, city.y)
        );
    }

    #[test]
    fn commerce_splits_like_civ3() {
        // Each share rounds down and the remainder is lost, as Civ3's own
        // screen shows: 27 commerce becomes 13 tax and 13 science.
        assert_eq!(commerce_split(27), (13, 13, 0));
        assert_eq!(commerce_split(20), (10, 10, 0));
        // Below ten nothing reaches the treasury yet.
        assert_eq!(commerce_split(9), (4, 4, 0));
        assert_eq!(commerce_split(0), (0, 0, 0));
    }

    #[test]
    fn commerce_comes_from_roads_and_water() {
        let bare = || Tile {
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
        assert_eq!(tile_commerce(&bare()), 0);
        let mut road = bare();
        road.road = true;
        assert_eq!(tile_commerce(&road), 1);
        let mut coast = bare();
        coast.base = Base::Coast;
        assert_eq!(tile_commerce(&coast), 2);
        coast.road = true;
        assert_eq!(tile_commerce(&coast), 3);
        // A city's own tile always pays at least one, as Civ3's does.
        let map = GameMap {
            w: 3,
            h: 3,
            tiles: vec![bare(); 9],
            start: (1, 1),
            seed: 1,
        };
        let city = test_city(&map);
        assert_eq!(city_commerce(&map, &city), 1);
    }

    #[test]
    fn culture_levels_are_powers_of_ten() {
        // Civ3 shows the current level next to the running total and
        // expands the borders at the next power of ten.
        assert_eq!(culture_thresholds(0), (10, 10));
        assert_eq!(culture_thresholds(1118), (1000, 10000));
        assert_eq!(culture_thresholds(10000), (10000, 100000));
    }

    #[test]
    fn label_row_is_centered_on_the_city() {
        // Badge, text box and star abut, and the row's middle is the
        // city: the capital's star pushes the text box left, not the row.
        let plain = label_layout(100.0, false);
        let left = plain.badge - BADGE_W / 2.0;
        let right = plain.text + plain.box_w / 2.0;
        assert!((left + right).abs() < 1e-4);
        assert!((plain.badge + BADGE_W / 2.0 - (plain.text - plain.box_w / 2.0)).abs() < 1e-4);
        let cap = label_layout(100.0, true);
        let left = cap.badge - BADGE_W / 2.0;
        let right = cap.star + STAR_W / 2.0;
        assert!((left + right).abs() < 1e-4);
        assert!((cap.text + cap.box_w / 2.0 - (cap.star - STAR_W / 2.0)).abs() < 1e-4);
    }

    #[test]
    fn initial_territory_is_a_diamond_until_first_culture_expansion() {
        assert_eq!(culture_level(0), 1);
        assert_eq!(culture_level(9), 1);
        assert_eq!(culture_level(10), 2);
        assert_eq!(culture_level(100), 3);
        assert_eq!(culture_level(u32::MAX), 6);
        let map = test_map();
        let mut city = test_city(&map);
        city.x = 40;
        city.y = 30;
        let expected: HashSet<_> = (-1..=1)
            .flat_map(|dy| (-1..=1).map(move |dx| (40 + dx, 30 + dy)))
            .collect();
        for culture in [0, 9] {
            city.culture = culture;
            assert_eq!(
                territory(&map, &[&city])
                    .into_keys()
                    .collect::<HashSet<_>>(),
                expected
            );
        }
        city.culture = 10;
        let expanded = territory(&map, &[&city]);
        assert!(expected.iter().all(|tile| expanded.contains_key(tile)));
        assert!(expanded.contains_key(&(40, 33)));
    }

    #[test]
    fn culture_per_turn_counts_the_palace_and_temples() {
        let map = test_map();
        let mut city = test_city(&map);
        assert_eq!(culture_per_turn(&city, false), 0);
        assert_eq!(culture_per_turn(&city, true), 1);
        city.buildings.push(Production::Temple);
        assert_eq!(culture_per_turn(&city, false), 2);
        assert_eq!(culture_per_turn(&city, true), 3);
        assert_eq!(Production::Temple.upkeep(), 1);
        assert_eq!(Production::Granary.upkeep(), 1);
        assert_eq!(Production::Warrior.upkeep(), 0);
    }

    #[test]
    fn city_arrows_wrap_through_the_city_list() {
        let a = Entity::from_bits(1);
        let b = Entity::from_bits(2);
        assert_eq!(step_city(&[a, b], a, 1), b);
        assert_eq!(step_city(&[a, b], b, 1), a);
        assert_eq!(step_city(&[a, b], a, -1), b);
        // A lone city stays put, and a city missing from the list starts
        // the walk at the first one.
        assert_eq!(step_city(&[a], a, 1), a);
        assert_eq!(step_city(&[a, b], Entity::from_bits(9), 1), b);
    }

    /// Every civ walks its own name list: one civ's foundings never
    /// consume another's names.
    #[test]
    fn city_names_are_per_civ() {
        let mut used = CityNamesUsed::default();
        assert_eq!(next_name(&mut used, 0), CIVS[0].city_names[0]);
        assert_eq!(next_name(&mut used, 0), CIVS[0].city_names[1]);
        assert_eq!(next_name(&mut used, 1), CIVS[1].city_names[0]);
        assert_eq!(next_name(&mut used, 2), CIVS[2].city_names[0]);
        assert_eq!(used.0, [2, 1, 1], "each civ counts its own foundings");
        // Past the end of its list a civ numbers its own first name
        // instead of borrowing a neighbour's still-unused one.
        let mut used = CityNamesUsed::default();
        let mut names: Vec<String> = (0..CIVS[1].city_names.len() + 2)
            .map(|_| next_name(&mut used, 1))
            .collect();
        let mut expected: Vec<String> = CIVS[1].city_names.iter().map(|n| n.to_string()).collect();
        expected.push(format!("{} 2", CIVS[1].city_names[0]));
        expected.push(format!("{} 2", CIVS[1].city_names[1]));
        assert_eq!(names, expected);
        names.sort();
        assert!(
            CIVS[0]
                .city_names
                .iter()
                .all(|n| !names.iter().any(|m| m == n)),
            "a civ never takes another civ's names"
        );
        assert_eq!(used.0[0], 0);
    }

    /// Accrual reads the city it is handed and that civ's capital flag:
    /// each civ's capital earns the palace's culture, and a road on one
    /// civ's tile pays only that civ's tax.
    #[test]
    fn accrue_is_per_city_and_marks_each_civs_capital() {
        let mut map = test_map();
        let mut japan = test_city_of(&map, 0);
        let mut rome = test_city_of(&map, 1);
        let mut picked = radius_tiles(&map, rome.x, rome.y).into_iter();
        let (jx, jy) = picked.next().unwrap();
        let (rx, ry) = picked.next().unwrap();
        // Land with no road, so commerce comes only from the craft below.
        for (x, y) in [(japan.x, japan.y), (jx, jy), (rx, ry)] {
            let i = map.idx(x, y);
            map.tiles[i].base = Base::Grassland;
            map.tiles[i].relief = Relief::Flat;
            map.tiles[i].cover = Cover::Bare;
            map.tiles[i].road = false;
        }
        let ri = map.idx(rx, ry);
        map.tiles[ri].road = true;
        japan.worked = HashSet::from([(jx, jy)]);
        rome.worked = HashSet::from([(rx, ry)]);
        let japan_tax = accrue(&map, &mut japan, true);
        let rome_tax = accrue(&map, &mut rome, true);
        assert_eq!(japan.culture, 1, "each civ's own capital earns the palace");
        assert_eq!(rome.culture, 1);
        assert_eq!(city_commerce(&map, &rome), city_commerce(&map, &japan) + 1);
        assert_eq!(japan_tax, 0, "one commerce pays no tax yet");
        assert_eq!(rome_tax, 1, "the road's commerce pays Rome's tax");
        let mut third = test_city_of(&map, 2);
        third.worked.clear();
        assert_eq!(accrue(&map, &mut third, false), 0);
        assert_eq!(third.culture, 0, "a non-capital earns no palace culture");
    }

    /// The resource tally counts the selected civ's cities only: a good on
    /// a tile the other civ's city owns is never claimed twice.
    #[test]
    fn resource_tally_follows_city_ownership() {
        let mut map = test_map();
        for t in map.tiles.iter_mut() {
            t.resource = None;
        }
        let japan = test_city_of(&map, 0);
        let mut rome = test_city_of(&map, 1);
        rome.x = map.wrap_x(japan.x + 4);
        rome.y = japan.y;
        rome.worked.clear();
        governor_assign(&map, &mut rome, &none());
        let id = crate::features::GOODS
            .iter()
            .position(|g| matches!(g.kind, crate::features::GoodKind::Strategic))
            .expect("a strategic good") as u8;
        let frontier = (map.wrap_x(rome.x + 1), rome.y);
        let i = map.idx(frontier.0, frontier.1);
        map.tiles[i].base = Base::Grassland;
        map.tiles[i].resource = Some(id);
        map.tiles[i].seen = true;
        let cities = [&japan, &rome];
        assert_eq!(
            territory(&map, &cities).get(&frontier).copied(),
            Some(1),
            "Rome holds the tile; Japan's border does not reach it"
        );
        assert_eq!(resources_owned(&map, &cities, 0).0, vec![]);
        assert_eq!(resources_owned(&map, &cities, 1).0, vec![(id, 1)]);
    }
}
