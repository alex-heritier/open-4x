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
use crate::civs::{CIV_CAP, CIVS, CivilizationEnded, Civilizations};
use crate::citycalc;
use crate::economy::{self, food_box, granary_keep};
use crate::features::{MessageBoard, post};
use crate::map::*;
use crate::render::{RevealAll, sprite_z};
use crate::splash::SplashUp;
use crate::roster::{self, bldg_count, BldgDef, unit_count};
use crate::units::{self, Selected, Unit, UnitArt, UnitType};

/// Something a city can build: a unit (`PRTO` row `i`) or an improvement
/// (`BLDG` row `i - unit_count()`). Game logic reaches the rows it means
/// through `roles` or the row's own fields, never by name.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct Production(pub u16);

impl std::fmt::Debug for Production {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// A civ's `RACE` trait mask (`civs::RACES`).
pub fn traits(civ: usize) -> u32 {
    crate::civs::RACES.get(civ).map_or(0, |r| r.traits)
}

/// Shields `p` costs the civ (`0x569FE0`, `economy.md`).
pub fn price_for(civ: usize, p: Production) -> u16 {
    let Some(b) = p.bldg() else { return p.cost() };
    let matched = civ3mapgen::economy::trait_discount(b.other, traits(civ));
    let palace = (b.flags & roster::imp::CENTER_OF_EMPIRE != 0).then(|| {
        let cities = crate::realm::read(civ, |r| r.cities).max(1) as i32;
        civ3mapgen::economy::palace_cost_factor(cities, crate::govern::WORLD_BASE)
    });
    civ3mapgen::economy::improvement_cost(b.cost, 10, matched, palace).clamp(1, i32::from(u16::MAX)) as u16
}

impl Production {
    /// Every unit and improvement the game builds, units first.
    pub fn all() -> impl Iterator<Item = Production> {
        (0..(unit_count() + bldg_count()) as u16)
            .map(Production)
            .filter(|p| p.playable())
    }

    /// Dense index for bit sets and tables.
    pub fn index(self) -> usize {
        self.0 as usize
    }

    /// The first unit or improvement called `name` (case-insensitive), units
    /// first: for scripts and tests.
    pub fn named(name: &str) -> Production {
        Self::find(name).unwrap_or_else(|| panic!("nothing is called {name}"))
    }

    pub fn find(name: &str) -> Option<Production> {
        let name = name.trim();
        UnitType::find(name).map(Production::from_unit).or_else(|| {
            roster::BLDGS.iter().position(|b| b.name.eq_ignore_ascii_case(name)).map(Production::from_building_row)
        })
    }

    pub fn from_building_row(row: usize) -> Production {
        Production((unit_count() + row) as u16)
    }

    pub fn from_unit(t: UnitType) -> Production {
        Production(t.0 as u16)
    }

    pub fn unit(self) -> Option<UnitType> {
        ((self.0 as usize) < unit_count()).then_some(UnitType(self.0))
    }

    pub fn is_building(self) -> bool {
        self.0 as usize >= unit_count()
    }

    /// The `BLDG` row of an improvement.
    pub fn bldg(self) -> Option<&'static BldgDef> {
        self.building_row().map(roster::bldg)
    }

    /// Index of the improvement's `BLDG` row.
    pub fn building_row(self) -> Option<usize> {
        (self.0 as usize).checked_sub(unit_count())
    }

    /// The game offers it for building (it has art or implemented effects).
    pub fn playable(self) -> bool {
        match self.unit() {
            Some(t) => t.row().playable,
            None => self.bldg().is_some_and(|b| b.playable),
        }
    }

    /// Shields a human pays (`city-turn.md` section 4).
    pub fn cost(self) -> u16 {
        match self.unit() {
            Some(t) => t.row().cost.max(1) as u16,
            None => self.bldg().map_or(1, |b| b.shields() as u16),
        }
    }

    pub fn name(self) -> &'static str {
        match self.unit() {
            Some(t) => t.row().name,
            None => self.bldg().map_or("?", |b| b.name),
        }
    }

    /// One-line effect blurb for the build list.
    pub fn blurb(self) -> String {
        if let Some(t) = self.unit() {
            let r = t.row();
            let mut s = if r.bombard > 0 && r.attack == 0 {
                format!("Bombard {}x{} range {}, Defense {}", r.bombard, r.rof, r.bomb_range, r.defense)
            } else if r.attack == 0 && r.defense == 0 {
                String::new()
            } else {
                format!("Attack {}, Defense {}", r.attack, r.defense)
            };
            if r.moves > 1 || s.is_empty() {
                if !s.is_empty() {
                    s.push_str(", ");
                }
                s.push_str(&format!("Moves {}", r.moves));
            }
            if r.pop_cost > 0 {
                s.push_str(&format!(", costs {} pop", r.pop_cost));
            }
            return s;
        }
        let Some(b) = self.bldg() else { return String::new() };
        let mut parts: Vec<String> = vec![];
        let f = b.flags;
        if b.happy > 0 {
            parts.push(format!("+{} content", b.happy));
        }
        if b.happy_all > 0 {
            parts.push(format!("+{} content everywhere", b.happy_all));
        }
        if f & roster::imp::VETERAN_GROUND_UNITS != 0 {
            parts.push("veteran land units".into());
        }
        if f & roster::imp::KEEPS_FOOD != 0 {
            parts.push("keeps half the food box".into());
        }
        if f & roster::imp::RESEARCH_BONUS != 0 {
            parts.push("+50% science".into());
        }
        if f & roster::imp::TAX_BONUS != 0 {
            parts.push("+50% tax".into());
        }
        if f & roster::imp::REDUCES_CORRUPTION != 0 {
            parts.push("less corruption".into());
        }
        if f & roster::imp::ALLOWS_SIZE_LEVEL_2 != 0 {
            parts.push("size above 8".into());
        }
        if f & roster::imp::ALLOWS_SIZE_LEVEL_3 != 0 {
            parts.push("size above 16".into());
        }
        if f & roster::imp::INCREASES_FOOD_IN_WATER != 0 {
            parts.push("+1 food at sea".into());
        }
        if f & roster::imp::CAPITALIZATION != 0 {
            parts.push("shields become gold".into());
        }
        if b.production > 0 {
            parts.push(format!("+{}% shields", b.production * 25));
        }
        if b.defense > 0 {
            parts.push(format!("+{}% defense", b.defense));
        }
        if b.is_great_wonder() {
            parts.push("wonder".into());
        }
        if parts.is_empty() && b.culture > 0 {
            parts.push(format!("{} culture", b.culture));
        }
        parts.join(", ")
    }

    /// Gold upkeep per turn.
    pub fn upkeep(self) -> u8 {
        self.bldg().map_or(0, |b| b.upkeep as u8)
    }

    /// Culture per turn.
    pub fn culture(self) -> u32 {
        self.bldg().map_or(0, |b| b.culture as u32)
    }

    /// Content faces the building adds in its own city.
    pub fn happy(self) -> u8 {
        self.bldg().map_or(0, |b| b.happy as u8)
    }

    /// Cell of the unit icon sheet (`cache/ui/unit_icons.png`, 14 columns of
    /// 32-px icons on a 33-px grid with a 1-px frame).
    pub fn unit_icon_rect(self) -> Option<Rect> {
        let i = self.unit()?.row().icon.max(0) as f32;
        let (c, r) = (i % 14.0, (i / 14.0).floor());
        let (x, y) = (1.0 + c * 33.0, 1.0 + r * 33.0);
        Some(Rect::new(x, y, x + 32.0, y + 32.0))
    }

    /// Sprite rect inside `buildings-small.png`: 32-px icons on a 33-px grid
    /// (1-px green lines), with a label column and a header row; row `i` is
    /// `BLDG` row `i`, the ancient-era column.
    pub fn building_rect(self) -> Option<Rect> {
        let y = 33.0 + self.building_row()? as f32 * 33.0;
        Some(Rect::new(33.0, y, 65.0, y + 32.0))
    }
}

/// Ancient Age specialist jobs (`CTZN`, `yields.md` 5.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Specialist { Entertainer, Scientist, TaxCollector }

impl Specialist {
    pub fn art(self) -> &'static str {
        match self {
            Self::Entertainer => "cache/ui/entertainer.png",
            Self::Scientist => "cache/ui/scientist.png",
            Self::TaxCollector => "cache/ui/tax_collector.png",
        }
    }
    pub(crate) fn next(self) -> Self {
        // The shipped CTZN table lists Entertainer, Tax Collector, Scientist.
        match self { Self::Entertainer => Self::TaxCollector, Self::TaxCollector => Self::Scientist, Self::Scientist => Self::Entertainer }
    }
}

#[derive(Component, Clone, serde::Serialize, serde::Deserialize)]
pub struct City {
    /// Owning civilization, an index into `CIVS`.
    pub civ: usize,
    pub name: String,
    pub x: i32,
    pub y: i32,
    /// Food stored toward the next citizen; a growth-blocked city can keep a full box.
    pub food: u8,
    /// Native terrain-disease flag (+0x68); infection causes citizen loss.
    pub diseased: bool,
    pub shields: u16,
    pub production: Production,
    /// Items queued after the current build.
    pub queue: Vec<Production>,
    pub buildings: Vec<Production>,
    /// Improvements this city gets from wonders (Pyramids' Granary, ...):
    /// they work like real ones but cost no upkeep and cannot be sold.
    pub gifts: Vec<Production>,
    /// Resources (`features::GOODS` ids) that reach the city through its
    /// owner's road network (`City +0x9C`, `trade-network.md` 7.1), refreshed
    /// by `realm::sync`.
    #[serde(default)]
    pub goods: u32,
    /// Borders a water body larger than 20 tiles (Harbor, ship production).
    pub coastal: bool,
    /// On or next to a river (Hydro Plant).
    pub river: bool,
    /// Persistent native citizen slots, including holes and free-list order.
    #[serde(with = "crate::citizens::pool_serde")]
    pub citizens: civ3mapgen::population::Pool,
    /// Culture accumulated: border levels are powers of ten (`economy.md`).
    pub culture: u32,
    /// Turn the city was founded (the clone has no calendar).
    pub founded: u32,
    /// Turns the city has been in civil disorder (0 when calm).
    pub unrest: u8,
    /// Turns of unhappiness left from forced labor (`city +0x1C4`): each
    /// sacrificed citizen adds twenty, one runs out a turn (`hurry.md` 5.2).
    pub hurry_timer: u16,
    /// Culture each civ still holds in this city (`City +0x140[civ]`): the
    /// owner's stock is `culture`, a former owner keeps what it had.
    pub stakes: [u32; CIV_CAP],
    /// Turns before the culture-flip test rolls again (`City +0x58`).
    pub cooldown: u8,
    /// Turns counted by each unit-producing building, by `BLDG` row
    /// (`city +0x37C`, `happiness.md` 10).
    pub unit_clocks: Vec<(u16, u8)>,
}

impl City {
    /// Shields `p` costs this city's civ (`0x569FE0`): a building is halved
    /// once for a civ whose trait matches it (Religious Temples, ...), and
    /// the Palace is dearer the bigger the empire.
    pub fn price(&self, p: Production) -> u16 {
        price_for(self.civ, p)
    }

    /// A size-1 city building a Warrior, with nothing worked yet.
    pub fn new(civ: usize, name: impl Into<String>, x: i32, y: i32) -> City {
        City {
            civ,
            name: name.into(),
            x,
            y,
            food: 0,
            shields: 0,
            production: crate::roles::guard_production(),
            queue: vec![],
            buildings: vec![],
            gifts: vec![],
            goods: 0,
            coastal: false,
            river: false,
            diseased: false,
            citizens: crate::citizens::new_pool(civ, 1),
            culture: 0,
            founded: 1,
            unrest: 0,
            hurry_timer: 0,
            stakes: Default::default(),
            cooldown: 0,
            unit_clocks: Vec::new(),
        }
    }
}

/// How many names each civ has taken from its own list.
#[derive(Resource, Default)]
pub struct CityNamesUsed(pub [usize; CIV_CAP]);

/// Gold in each civ's treasury. A civ's turn adds its tax and pays for its
/// improvements and units (`economy::pay`); it never goes below zero.
#[derive(Resource, Default)]
pub struct Treasury(pub [u32; CIV_CAP]);

#[derive(Resource, Default)]
pub struct CityView(pub Option<Entity>);

#[derive(Deserialize)]
struct CityEntry {
    size: [u32; 2],
    anchor: [i32; 2],
}

/// The map-view city sprites of `Art/Cities`: one sheet per culture group
/// (`RACE.culture_group`), a row per era and a column per size class, and a
/// walled version per group and era (the walls sheets draw the whole city).
#[derive(Resource)]
pub struct CityArt {
    /// `[group][era][town, city, metro]`.
    sprites: Vec<Vec<[Handle<Image>; 3]>>,
    /// `[group][era]`.
    walls: Vec<Vec<Handle<Image>>>,
    pub anchor: Anchor,
}

/// Culture groups of the city sheets (`rAMER`, `rEURO`, `rROMAN`,
/// `rMIDEAST`, `rASIAN`) and eras of their rows.
const CULTURE_GROUPS: usize = 5;
const CITY_ERAS: usize = 4;

impl CityArt {
    pub fn load(asset_server: &AssetServer) -> Self {
        let text = fs::read_to_string("assets/cache/cities/manifest.json")
            .expect("run from the repo root: the art cache (assets/cache) is built at startup");
        let raw: HashMap<String, CityEntry> =
            serde_json::from_str(&text).expect("cities manifest parses");
        let anchor = {
            let e = &raw["town_0_0"];
            Anchor(Vec2::new(
                e.anchor[0] as f32 / e.size[0] as f32 - 0.5,
                0.5 - e.anchor[1] as f32 / e.size[1] as f32,
            ))
        };
        let load = |kind: &str, g: usize, e: usize| asset_server.load(format!("cache/cities/{kind}_{g}_{e}.png"));
        let sprites = (0..CULTURE_GROUPS)
            .map(|g| (0..CITY_ERAS).map(|e| [load("town", g, e), load("city", g, e), load("metro", g, e)]).collect())
            .collect();
        let walls = (0..CULTURE_GROUPS).map(|g| (0..CITY_ERAS).map(|e| load("wall", g, e)).collect()).collect();
        Self { sprites, walls, anchor }
    }

    /// The sprite of a city of `size` in a civ of culture `group` in `era`.
    pub fn graphic(&self, size: u8, group: usize, era: usize) -> Handle<Image> {
        let class = if size >= 13 {
            2
        } else if size >= 7 {
            1
        } else {
            0
        };
        self.sprites[group.min(CULTURE_GROUPS - 1)][era.min(CITY_ERAS - 1)][class].clone()
    }

    pub fn wall(&self, group: usize, era: usize) -> Handle<Image> {
        self.walls[group.min(CULTURE_GROUPS - 1)][era.min(CITY_ERAS - 1)].clone()
    }
}

/// The culture group of a civ's cities.
pub fn culture_group(civ: usize) -> usize {
    crate::civs::RACES[civ].culture_group.max(0) as usize
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

/// Tiles a city cannot work: every city center, the tiles other cities
/// already work, and every tile of its radius outside its own
/// civilization's cultural border, foreign or unclaimed alike, as Civ3's
/// city view shows it. `me` is the asking city's center and must be among
/// `cities`; without it the border rule has no civ to measure against and
/// is skipped.
pub fn taken_tiles<'a>(
    map: &GameMap,
    cities: impl IntoIterator<Item = &'a City>,
    me: (i32, i32),
) -> HashSet<(i32, i32)> {
    let all: Vec<&City> = cities.into_iter().collect();
    let mut out = HashSet::new();
    for c in &all {
        out.insert((c.x, c.y));
        if (c.x, c.y) != me {
            out.extend(c.worked(&map).iter().copied());
        }
    }
    if let Some(mine) = all.iter().find(|c| (c.x, c.y) == me) {
        let land = territory(map);
        out.extend(
            radius_tiles(map, mine.x, mine.y)
                .into_iter()
                .filter(|t| land.get(t) != Some(&mine.civ)),
        );
    }
    out
}

/// A radius tile the city may work: explored, inside its civ's border, and
/// not taken by another city.
pub fn workable(map: &GameMap, taken: &HashSet<(i32, i32)>, tile: (i32, i32)) -> bool {
    !taken.contains(&tile) && map.get(tile.0, tile.1).is_some_and(|t| t.seen)
}

/// `workable` for the city's own civilization: the computer's cities see
/// the whole map, since `Tile::seen` only describes the human's view.
fn can_work(map: &GameMap, taken: &HashSet<(i32, i32)>, tile: (i32, i32), civ: usize) -> bool {
    if crate::civs::is_ai(civ) {
        !taken.contains(&tile) && map.get(tile.0, tile.1).is_some()
    } else {
        workable(map, taken, tile)
    }
}

/// Drop the worked tiles the city may no longer work: its border lost it
/// grew over one, a city was founded on it, or a neighbor took it. True
/// when anything was dropped. Fog is not rechecked: a tile once worked was
/// explored, and `Tile::seen` only describes the civ in play.
pub fn prune_worked(map: &GameMap, city: &mut City, taken: &HashSet<(i32, i32)>) -> bool {
    let dropped: Vec<_> = city.worked(map).into_iter().filter(|t| taken.contains(t)).collect();
    for &tile in &dropped { city.unwork(map, tile, false); }
    !dropped.is_empty()
}

/// The governor's ranking of a tile: food first, then shields, then
/// commerce, as the city would get them under its civ's government.
fn tile_rank(map: &GameMap, city: &City, tile: &(i32, i32)) -> (i32, i32, i32) {
    citycalc::tile_yields(map, city, tile.0, tile.1)
}

/// Governor: work the best `size` tiles, food first then shields.
pub fn governor_assign(map: &GameMap, city: &mut City, taken: &HashSet<(i32, i32)>) {
    city.clear_worked();
    city.clear_specialists();
    governor_fill(map, city, taken);
}

/// Match worked tiles to the population without touching the player's
/// other picks: assign unplaced laborers on growth or after a tile is lost.
/// Population loss already removed its own citizen's tile or specialist.
pub fn governor_fill(map: &GameMap, city: &mut City, taken: &HashSet<(i32, i32)>) {
    let workers = city.size() as usize - city.specialists().len() - crate::resistance::resisters(city) as usize;
    let mut free: Vec<(i32, i32)> = radius_tiles(map, city.x, city.y)
        .into_iter()
        .filter(|t| !city.worked(&map).contains(t) && can_work(map, taken, *t, city.civ))
        .collect();
    free.sort_by_key(|t| {
        let (f, s, c) = tile_rank(map, city, t);
        (
            std::cmp::Reverse(f),
            std::cmp::Reverse(s),
            std::cmp::Reverse(c),
        )
    });
    let want = workers.saturating_sub(city.worked(&map).len());
    for tile in free.into_iter().take(want) { city.work_tile(map, tile); }
    city.entertain_unassigned();
}

/// The computer's governor: start from the food-first picks, then trade
/// food for shields and commerce while the city keeps growing (a surplus of
/// 2 while small, 1 once it is a city). The human's governor stays
/// food-first.
pub fn balanced_governor(map: &GameMap, city: &mut City, taken: &HashSet<(i32, i32)>) {
    governor_assign(map, city, taken);
    let floor: i16 = if city.size() < SETTLER_MIN_SIZE { 2 } else { 1 };
    let value = |c: &City| {
        let (food, shields) = city_income(map, c);
        shields as i32 * 3 + food.clamp(0, 4) as i32 * 2 + city_commerce(map, c) as i32
    };
    let free: Vec<(i32, i32)> = radius_tiles(map, city.x, city.y)
        .into_iter()
        .filter(|t| can_work(map, taken, *t, city.civ))
        .collect();
    for _ in 0..(city.size() as usize * 2).max(2) {
        let base = value(city);
        let mut best: Option<(i32, (i32, i32), (i32, i32))> = None;
        for &out in city.worked(&map).iter() {
            for &into in free.iter().filter(|t| !city.worked(&map).contains(t)) {
                let mut trial = city.clone();
                trial.replace_work(map, out, into);
                let gain = value(&trial) - base;
                if city_income(map, &trial).0 >= floor.min(city_income(map, city).0)
                    && gain > 0
                    && best.is_none_or(|(g, _, _)| gain > g)
                {
                    best = Some((gain, out, into));
                }
            }
        }
        let Some((_, out, into)) = best else { break };
        city.replace_work(map, out, into);
    }
}

/// Toggle a worked tile. When every citizen is busy, the worst tile is
/// replaced. Returns false when the tile cannot be worked.
pub fn toggle_worked(
    map: &GameMap,
    city: &mut City,
    tile: (i32, i32),
    taken: &HashSet<(i32, i32)>,
) -> bool {
    if city.unwork(map, tile, true) {
        return true;
    }
    if !can_work(map, taken, tile, city.civ) || !radius_tiles(map, city.x, city.y).contains(&tile) {
        return false;
    }
    if !city.work_tile(map, tile) {
        let worst = city
            .worked(&map)
            .iter()
            .min_by_key(|t| tile_rank(map, city, t))
            .copied();
        if let Some(worst) = worst {
            city.replace_work(map, worst, tile);
        }
    }
    true
}

/// Gross (food, shields) from the center plus worked tiles, under the
/// owner's government (`citycalc::gross`).
pub fn city_yields(map: &GameMap, city: &City) -> (u8, u8) {
    let (food, shields, _) = citycalc::gross(map, city);
    (food.clamp(0, 255) as u8, shields.clamp(0, 255) as u8)
}

/// (food stored or lost each turn, shields added to the box each turn):
/// the city's real numbers, after waste, the buildings' multiplier and
/// civil disorder (`citycalc::totals`).
pub fn city_income(map: &GameMap, city: &City) -> (i16, u8) {
    let t = citycalc::totals(map, city);
    (t.surplus as i16, t.shields.clamp(0, 255) as u8)
}

/// Commerce from TERR values, road bonus and the native river point.
pub fn tile_commerce(t: &Tile) -> u8 {
    if t.base == Base::Ice { return 0; }
    let terrain = &crate::ruleset::TERRAINS[crate::map::terrain_row(t)];
    terrain.commerce + if t.road { terrain.road } else { 0 } + u8::from(t.river != 0)
}

/// Gross commerce: the center always gives one (Civ3's city tile), plus
/// every worked tile.
pub fn city_commerce(map: &GameMap, city: &City) -> u8 {
    citycalc::gross(map, city).2.clamp(0, 255) as u8
}

/// Culture per turn: the Palace's belongs to the capital.
pub fn culture_per_turn(city: &City, capital: bool) -> u32 {
    let implicit_palace = capital && !city.buildings.iter().any(|b|
        b.bldg().is_some_and(|b| b.flags & roster::imp::CENTER_OF_EMPIRE != 0));
    city.buildings.iter().map(|b| b.culture()).sum::<u32>() + u32::from(implicit_palace)
}

/// Gold a city pays its civ's treasury each turn: the tax share of its
/// commerce, with its buildings' multipliers and any Wealth.
pub fn city_tax(map: &GameMap, city: &City) -> u32 {
    citycalc::totals(map, city).tax.max(0) as u32
}

/// Culture and tax a city adds when its civ's turn ends. Everything comes
/// from the city itself, so one civ's turn never moves another's numbers;
/// `capital` is whether this city is its own civ's capital.
pub fn accrue(map: &GameMap, city: &mut City, capital: bool) -> u32 {
    city.culture += culture_per_turn(city, capital);
    city_tax(map, city)
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

/// Squared reach of each culture level in the exe's doubled grid (table
/// `0x670540`, `borders-culture.md` 4.2): levels 1 to 6 hold 9, 21, 37, 61,
/// 89 and 137 tiles. Index 0 is never a city's level.
pub const CULTURE_REACH2: [i32; 7] = [0, 4, 10, 20, 36, 52, 82];

/// Squared doubled-grid length of a map offset. Map steps `(1, 0)` and
/// `(0, 1)` are the doubled-grid diagonals `(1, 1)` and `(-1, 1)`
/// (`tile_to_world`), so `dx = u - v`, `dy = u + v` and
/// `dx² + dy² = 2(u² + v²)`.
pub fn reach2((u, v): (i32, i32)) -> i32 {
    2 * (u * u + v * v)
}

/// The exe's 169 border offsets `0x5E6E50(k)` in map steps: the tile
/// itself, then rings 1 to 6 outward (`borders-culture.md` 4.1). Nearer
/// rings come first, which is what lets the first city found keep a tile.
pub fn border_offsets() -> &'static [(i32, i32)] {
    static OFFSETS: std::sync::OnceLock<Vec<(i32, i32)>> = std::sync::OnceLock::new();
    OFFSETS.get_or_init(|| {
        let mut out = vec![(0, 0)];
        out.extend([(0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1)]);
        for r in 2..=6 {
            out.extend((-r + 1..r).map(|u| (u, -r)));
            out.extend((-r + 1..r).map(|v| (r, v)));
            out.extend((-r + 1..r).rev().map(|u| (u, r)));
            out.extend((-r + 1..r).rev().map(|v| (-r, v)));
            out.extend([(-r, -r), (r, -r), (r, r), (-r, r)]);
        }
        out
    })
}

/// Owner civ of every tile inside a cultural border, as last written by
/// `recompute_borders`.
pub fn territory(map: &GameMap) -> HashMap<(i32, i32), usize> {
    let mut out = HashMap::new();
    for y in 0..map.h {
        for x in 0..map.w {
            if let Some(o) = map.tiles[map.idx(x, y)].owner {
                out.insert((x, y), o as usize);
            }
        }
    }
    out
}

/// The claimant tie-break `0x5D3850` (`borders-culture.md` 5.4): the city
/// with more culture wins. The exe's per-city words `+0x358..+0x364` are
/// not modelled (they are set in real saves, mode 1 with `+0x364 = 1450`,
/// `economy.md` corruption ties, but their writers are open), so equal
/// culture falls through to the empire ratings, then to the lower civ slot,
/// and a full tie goes to the challenger `c`.
fn border_tie_break(a: &City, c: &City, rating: &[u32]) -> bool {
    if a.culture != c.culture {
        return a.culture > c.culture;
    }
    let (ra, rc) = (rating[a.civ], rating[c.civ]);
    if ra != rc {
        return ra > rc;
    }
    a.civ < c.civ
}

/// Rewrite every tile's owner from the cities' culture levels:
/// `Map::recomputeBorders` `0x5D4830` (`borders-culture.md` 5). The exe
/// runs it only when a city is founded, destroyed or captured or its level
/// changes, never on a plain turn, so a frontier holds still between those
/// events. `rating` is each civ's empire culture (`Player +0x183C`).
///
/// 1. Each tile goes to the first city in offset order whose level reaches
///    it, unless a later one wins the tie-break from inside the same reach
///    bracket. Open ocean only answers to the 21-tile radius.
/// 2. An owned tile no city reaches is marked lapsed; it and every unowned
///    tile then take the owner its two facing neighbours across agree on
///    (`0x5D4370`), repeated until nothing changes. This is what bridges
///    one-tile gaps between cities.
/// 3. Tiles still lapsed fall to nobody.
pub fn recompute_borders(map: &mut GameMap, cities: &[&City], rating: &[u32]) {
    let at: HashMap<(i32, i32), usize> =
        cities.iter().enumerate().map(|(i, c)| ((c.x, c.y), i)).collect();
    let offsets = border_offsets();
    let n = map.tiles.len();
    let mut lapsed = vec![false; n];
    let step = |map: &GameMap, x: i32, y: i32, (u, v): (i32, i32)| {
        let ny = y + v;
        (0..map.h).contains(&ny).then(|| (map.wrap_x(x + u), ny))
    };
    for y in 0..map.h {
        for x in 0..map.w {
            let i = map.idx(x, y);
            let ocean = map.tiles[i].base == Base::Ocean;
            let mut best: Option<(usize, i32)> = None;
            for (k, &off) in offsets.iter().enumerate() {
                if ocean && k >= 21 {
                    break;
                }
                let Some(t) = step(map, x, y, off) else { continue };
                let Some(&ci) = at.get(&t) else { continue };
                let c = cities[ci];
                let d2 = reach2(off);
                if d2 > CULTURE_REACH2[culture_level(c.culture) as usize] {
                    continue;
                }
                let Some((bi, bd2)) = best else {
                    best = Some((ci, d2));
                    continue;
                };
                if border_tie_break(cities[bi], c, rating) {
                    continue;
                }
                let bracket = CULTURE_REACH2[..6].iter().position(|&r| bd2 <= r).unwrap_or(6);
                if d2 <= CULTURE_REACH2[bracket] {
                    best = Some((ci, d2));
                }
            }
            match best {
                Some((ci, _)) => map.tiles[i].owner = Some(cities[ci].civ as u8),
                None => lapsed[i] = map.tiles[i].owner.is_some(),
            }
        }
    }
    loop {
        let mut changed = false;
        for y in 0..map.h {
            for x in 0..map.w {
                let i = map.idx(x, y);
                if map.tiles[i].owner.is_some() && !lapsed[i] {
                    continue;
                }
                let Some(o) = orphan_owner(map, &lapsed, x, y, rating) else { continue };
                lapsed[i] = false;
                map.tiles[i].owner = Some(o);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    for (t, l) in map.tiles.iter_mut().zip(lapsed) {
        if l {
            t.owner = None;
        }
    }
}

/// Orphan resolution `0x5D4370` (`borders-culture.md` 5.5): the owner a
/// tile no city claims takes from its edge neighbours. The pair across
/// `x` and the pair across `y` must each agree; when both pairs name a
/// civ and differ, the higher empire culture wins, ties to the `x` pair.
/// Open ocean and lapsed neighbours never settle anything.
fn orphan_owner(
    map: &GameMap,
    lapsed: &[bool],
    x: i32,
    y: i32,
    rating: &[u32],
) -> Option<u8> {
    if map.tiles[map.idx(x, y)].base == Base::Ocean {
        return None;
    }
    let nb = |u: i32, v: i32| {
        let ny = y + v;
        if !(0..map.h).contains(&ny) {
            return None;
        }
        let j = map.idx(map.wrap_x(x + u), ny);
        if lapsed[j] { None } else { map.tiles[j].owner }
    };
    let pair = |a: Option<u8>, b: Option<u8>| if a == b { a } else { None };
    let across_x = pair(nb(-1, 0), nb(1, 0));
    let across_y = pair(nb(0, -1), nb(0, 1));
    match (across_x, across_y) {
        (Some(a), Some(b)) if a != b => {
            Some(if rating[a as usize] >= rating[b as usize] { a } else { b })
        }
        (Some(a), _) => Some(a),
        (None, b) => b,
    }
}

/// What `recompute_borders` last saw of each city: where it stands, who
/// holds it and its culture level. The exe recomputes on exactly these
/// changes (founding, destruction, capture, level change).
#[derive(Resource, Default)]
pub struct BorderKey(Option<Vec<(i32, i32, usize, u32)>>);

/// Recompute the borders whenever a city appears, disappears, changes
/// hands or changes culture level, and on the first frame of a game.
pub fn update_borders(
    mut key: ResMut<BorderKey>,
    mut map: ResMut<GameMap>,
    cities: Query<&City>,
    flips: Option<Res<crate::flip::Flips>>,
) {
    let mut now: Vec<(i32, i32, usize, u32)> =
        cities.iter().map(|c| (c.x, c.y, c.civ, culture_level(c.culture))).collect();
    now.sort_unstable();
    if key.0.as_ref() == Some(&now) {
        return;
    }
    let rating = flips.map(|f| f.empire).unwrap_or_default();
    let list: Vec<&City> = cities.iter().collect();
    recompute_borders(&mut map, &list, &rating);
    key.0 = Some(now);
}

/// Strategic and luxury goods one civ controls: how many tiles of each lie
/// inside the cultural borders of `civ`'s cities. `cities` holds every
/// civ's cities, so a tile two civs contend for is counted only by the one
/// whose city `territory` awards it.
pub fn resources_owned(
    map: &GameMap,
    civ: usize,
) -> (Vec<(u8, u32)>, Vec<(u8, u32)>) {
    use std::collections::BTreeMap;
    let mut counts: BTreeMap<u8, u32> = BTreeMap::new();
    let owners = territory(map);
    for (&(x, y), &owner) in &owners {
        if owner != civ {
            continue;
        }
        let t = &map.tiles[map.idx(x, y)];
        if let (true, Some(id)) = (t.seen, t.resource) {
            *counts.entry(id).or_insert(0) += 1;
        }
    }
    // A colony outside the borders counts too (`sites.rs`); one inside was
    // counted above.
    for y in 0..map.h {
        for x in 0..map.w {
            let t = &map.tiles[map.idx(x, y)];
            if t.site == Some(crate::sites::Site::Colony(civ as u8))
                && t.seen
                && owners.get(&(x, y)).is_none()
                && let Some(id) = t.resource
            {
                *counts.entry(id).or_insert(0) += 1;
            }
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

/// Normal production keeps a citizen; native ABANDONBASE for nongrowing
/// cities is still pending (`city-turn.md` 7).
pub const SETTLER_MIN_SIZE: u8 = 3;

#[derive(Debug, PartialEq)]
pub enum CityEvent {
    Grew,
    Starved,
    Abandon,
    Disease(Option<usize>),
    Completed(UnitType, usize),
    Built(Production),
    /// A population-cost unit waits for the city to keep a citizen.
    TooSmall,
    /// The city is in civil disorder this turn.
    Disorder,
    /// The disorder is over.
    Calm,
    /// The food box is full but the city cannot grow without an Aqueduct
    /// (or a Hospital); announced once.
    Blocked,
    /// `WONDERCHANGE`: the great wonder was finished elsewhere and the city
    /// switched to the second item (`city-turn.md` 6.1).
    WonderLost(Production, Production),
}

impl City {
    pub fn has(&self, p: Production) -> bool {
        self.buildings.contains(&p)
    }

    /// Items offered in the build list: what the civ has the advance for,
    /// except buildings the city already owns.
    pub fn buildable(&self) -> Vec<Production> {
        Production::all()
            .filter(|&p| self.can_build_here(p))
            .collect()
    }

    /// Can this city build `p` now: the civ may (advance, race, resources,
    /// no wonder built elsewhere) and the city's own conditions hold
    /// (`buildable.md` 2 and 3.1).
    pub fn can_build_here(&self, p: Production) -> bool {
        if !crate::research::can_build(self.civ, p) {
            return false;
        }
        // Every required resource must reach this city (`0x4ADE30`).
        let rows: &[i32] = match (p.unit(), p.bldg()) {
            (Some(t), _) => &t.row().resources,
            (None, Some(b)) => &b.resources,
            _ => &[],
        };
        if !rows.iter().all(|&g| self.resource_usable(g)) {
            return false;
        }
        if p.unit().is_some_and(|t| t.row().class == 1) && !self.coastal {
            return false;
        }
        // An Army needs a small wonder that builds armies, the Military
        // Academy (`buildable.md` 3.1 step 5). HYPOTHESIS-free gap: the
        // empire's `(armies + 1) * 4 <= cities` rule (3 step 6) is not
        // checked; no Ancient Age city can hold the Academy.
        if p.unit().is_some_and(|t| t.row().abilities & roster::ability::ARMY != 0)
            && !self.buildings.iter().any(|b| b.bldg().is_some_and(|d| d.small & 2 != 0))
        {
            return false;
        }
        let Some(b) = p.bldg() else {
            return true;
        };
        if self.has(p) || self.granted(p) {
            return false;
        }
        if !self.civ_may_build(b) {
            return false;
        }
        if b.requires >= 0 && !self.has(Production::from_building_row(b.requires as usize)) {
            return false;
        }
        if b.flags & roster::imp::MUST_BE_NEAR_WATER != 0 && !self.coastal {
            return false;
        }
        if b.flags & roster::imp::MUST_BE_NEAR_RIVER != 0 && !self.river {
            return false;
        }
        true
    }

    /// `City::resourceUsable` `0x4ADE30` for a `GOOD` row: the city's own
    /// mask (for a city joined to the capital the civ's supply records,
    /// which equal the capital's mask while resources are not traded).
    pub fn resource_usable(&self, row: i32) -> bool {
        row < 0 || crate::realm::good_id(row).is_some_and(|id| self.goods >> id & 1 != 0)
    }

    /// The player-level gates of `canBuildImprovement` `0x56A2A0` the build
    /// lists do not cover elsewhere (`buildable.md` 2): a small wonder is
    /// built once per civilization (step 9), and a Reduces-Corruption one
    /// needs at least half the world's optimal number of cities (step 12).
    pub fn civ_may_build(&self, b: &roster::BldgDef) -> bool {
        let row = roster::BLDGS.iter().position(|d| std::ptr::eq(d, b));
        let (owned, cities) = crate::realm::read(self.civ, |r| (row.map_or(0, |i| r.owned[i]), r.cities as i32));
        if b.is_small_wonder() && owned > 0 {
            return false;
        }
        if b.small & 0x20 != 0 && cities < crate::govern::WORLD_BASE / 2 {
            return false;
        }
        true
    }

    /// `0x436BB0`, the human's fallback when a great wonder is lost: the
    /// most expensive building the city can build (not the Palace, no
    /// Reduces-Corruption wonder, and no Courthouse in the capital), a tie
    /// to the later row, then any unit costing at least as much.
    pub fn most_expensive(&self) -> Option<Production> {
        let capital = crate::realm::read(self.civ, |r| r.capital) == Some((self.x, self.y));
        let mut best: Option<(u16, Production)> = None;
        for p in Production::all().filter(|p| p.is_building()) {
            let Some(b) = p.bldg() else { continue };
            if b.flags & roster::imp::CENTER_OF_EMPIRE != 0
                || b.small & 0x20 != 0
                || capital && b.flags & roster::imp::REDUCES_CORRUPTION != 0
                || b.flags & roster::imp::CAPITALIZATION != 0
                || !self.can_build_here(p)
            {
                continue;
            }
            let cost = self.price(p);
            if best.is_none_or(|(c, _)| cost >= c) {
                best = Some((cost, p));
            }
        }
        for p in Production::all().filter(|p| p.unit().is_some() && self.can_build_here(*p)) {
            let cost = self.price(p);
            if best.is_none_or(|(c, _)| cost >= c) {
                best = Some((cost, p));
            }
        }
        best.map(|(_, p)| p)
    }

    /// An improvement the city has without building it: the gifts of
    /// wonders. Set by the turn code each turn (`granted`).
    pub fn granted(&self, p: Production) -> bool {
        self.gifts.contains(&p)
    }

    /// `City::setProduction` (`0x4AFB50`): keep shields up to the new cost.
    pub fn change_build(&mut self, p: Production) {
        if p == self.production {
            return;
        }
        self.production = p;
        self.shields = self.shields.min(self.price(p));
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
    pub(crate) fn advance_queue(&mut self, done: Production) {
        if !self.queue.is_empty() {
            self.production = self.queue.remove(0);
        } else if done.is_building() {
            self.production = crate::roles::guard_production();
        }
    }
}

pub fn process_city_turn(
    map: &GameMap,
    city: &mut City,
    taken: &HashSet<(i32, i32)>,
    rng: &mut crate::rng::MapRng,
) -> Vec<CityEvent> {
    let mut events = vec![];
    // Step 2 of the sequencer (`city-turn.md`, `0x4BE990`): the whip's
    // memory fades a turn before anything is counted.
    city.hurry_timer = city.hurry_timer.saturating_sub(1);
    if let Some(terrain) = crate::disease::step(map, city, rng) {
        events.push(CityEvent::Disease(terrain));
    }
    let totals = citycalc::totals(map, city);
    if totals.disorder {
        city.unrest = city.unrest.saturating_add(1);
        events.push(CityEvent::Disorder);
    } else if city.unrest > 0 {
        city.unrest = 0;
        events.push(CityEvent::Calm);
    }
    let food = city.food as i16 + totals.surplus as i16;
    let full = food_box(city.size()) as i16;
    if food >= full && citycalc::growth_blocked(city, map.fresh_water(city.x, city.y)) {
        // A town or city at its limit keeps a full box until the improvement
        // that lifts the limit arrives (`0x4B1DC0`).
        if (city.food as i16) < full {
            events.push(CityEvent::Blocked);
        }
        city.food = full as u8;
    } else if food >= full {
        // The box that filled is the old size's, and a Granary keeps half
        // of that one; without it the box empties and the overflow is lost.
        city.food = if citycalc::has_flag(city, roster::imp::KEEPS_FOOD) {
            granary_keep(city.size())
        } else {
            0
        };
        city.add_citizens(1, crate::civs::roster_index(city.civ));
        governor_fill(map, city, taken);
        events.push(CityEvent::Grew);
    } else if food < 0 {
        if city.size() > 1 {
            city.lose_population(1, None, rng);
            governor_fill(map, city, taken);
            events.push(CityEvent::Starved);
        }
        city.food = 0;
    } else {
        city.food = food as u8;
    }
    // Wealth turns the shields into gold (paid with the taxes), and the
    // box never fills.
    if city.production.bldg().is_some_and(|b| b.flags & roster::imp::CAPITALIZATION != 0) {
        city.shields = 0;
        if !city.queue.is_empty() {
            city.advance_queue(city.production);
        }
        return events;
    }
    let shields = totals.shields.max(0);
    let before = city.shields;
    if shields > 0 {
        city.shields = city.shields.saturating_add(shields as u16).min(city.price(city.production));
    }
    // `0x4B9270` 6.1: the great wonder was finished elsewhere. HYPOTHESIS
    // for the computer, which takes over a sibling's wonder (`0x436D10`) in
    // the executable: here it falls back like the human.
    if let Some(b) = city.production.bldg()
        && b.is_great_wonder()
        && crate::research::wonder_built(city.production)
    {
        let lost = city.production;
        if let Some(next) = city.most_expensive() {
            city.change_build(next);
            events.push(CityEvent::WonderLost(lost, next));
        }
        return events;
    }
    // 6.2: an improvement the city already has, or the civ may no longer
    // build, never completes; the queue (or the default) takes over.
    if let Some(b) = city.production.bldg()
        && city.shields >= city.price(city.production)
        && (city.has(city.production) || !city.civ_may_build(b))
    {
        let done = city.production;
        city.advance_queue(done);
        return events;
    }
    if city.shields >= city.price(city.production) {
        let done = city.production;
        if done.unit().is_some_and(|u| city.size() as i32 <= units::def(u).pop_cost) {
            // Held at full cost; say so once, when the box first fills.
            city.shields = city.price(done);
            if totals.surplus <= 0 && !crate::civs::is_ai(city.civ) {
                events.push(CityEvent::Abandon);
                return events;
            }
            if before < city.price(done) {
                events.push(CityEvent::TooSmall);
            }
            return events;
        }
        city.shields = 0;
        if let Some(unit) = done.unit() {
            let pop = units::def(unit).pop_cost.max(0) as u8;
            let nationality = city.pay_population_cost(pop, rng);
            if pop > 0 {
                governor_fill(map, city, taken);
            }
            events.push(CityEvent::Completed(unit, nationality));
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
    /// Population box on the left: a dark frame around `BadgeFill`.
    Badge,
    /// Civ-colored inside of the badge; a child of `Badge`.
    BadgeFill,
    /// Slate band behind the name line.
    NameBand,
    /// Brown band behind the build line.
    BuildBand,
    /// The two text lines.
    Text,
    /// Gold capital star on the right; only the capital shows it.
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
pub struct Capital(pub [Option<Entity>; CIV_CAP]);

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
pub const CIV_BADGE_INK: Color = Color::srgb_u8(244, 240, 222);
/// Civ3 draws the label as two bands: a slate one behind the name and a
/// brown one behind the build.
const NAME_BAND: Color = Color::srgba(0.15, 0.19, 0.27, 0.86);
const BUILD_BAND: Color = Color::srgba(0.31, 0.17, 0.08, 0.86);
const LABEL_INK: Color = Color::srgb_u8(236, 232, 218);
const LABEL_FONT: f32 = 11.0;
/// One text line, and one band: the two lines fill the label exactly.
const LINE_H: f32 = 13.0;
const LABEL_H: f32 = 2.0 * LINE_H;
const BADGE_W: f32 = 22.0;
/// Width of the pale frame around the badge's civ color.
const BADGE_FRAME: f32 = 1.0;
const STAR_W: f32 = 20.0;
/// Horizontal padding between the text and the edges of its bands.
const TEXT_PAD: f32 = 6.0;
/// Label center below the tile center: the label hangs under the town art
/// over the tile's lower half, as in Civ3.
const LABEL_DROP: f32 = 34.0;

/// The badge frame: Civ3 rims the civ color in a pale tint of itself.
fn badge_frame(c: Color) -> Color {
    let s = c.to_srgba();
    let pale = |v: f32| v + (1.0 - v) * 0.6;
    Color::srgb(pale(s.red), pale(s.green), pale(s.blue))
}

fn ceil_div(a: impl Into<u16>, b: impl Into<u16>) -> u16 {
    a.into().div_ceil(b.into().max(1))
}

/// Map label, Civ3's two lines: the city name with turns to grow, and the
/// current build with turns left. A stalled line shows `--`, as in Civ3.
///
/// Another civ's city shows its bare name over an empty build band: the
/// label routine `0x4E5580` appends the growth and build only when the
/// owner is the local player (`0x4E5839`; `reverse-engineering/ui.md`).
fn banner_text(map: &GameMap, city: &City, viewer: usize) -> String {
    if city.civ != viewer {
        // A no-break space keeps the second line, so the name stays in the
        // name band.
        return format!("{}\n\u{a0}", city.name);
    }
    let (net_food, shields) = city_income(map, city);
    let name = if net_food > 0 {
        let left = food_box(city.size()).saturating_sub(city.food);
        format!("{} : {}", city.name, ceil_div(left, net_food as u8))
    } else {
        format!("{} : --", city.name)
    };
    let prod = if shields == 0 {
        format!("{} : --", city.production.name())
    } else {
        let left = city.price(city.production).saturating_sub(city.shields);
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
    viewer: usize,
) {
    // Visuals are roots carrying the city entity, like TileSprite and Unit.
    // Cities never move, so nothing needs syncing after spawn.
    let pos = tile_to_world(city.x, city.y);
    commands.spawn((
        Sprite {
            image: art.graphic(city.size(), culture_group(city.civ), 0),
            ..default()
        },
        art.anchor.clone(),
        Transform::from_xyz(pos.x, pos.y, sprite_z(city.x, city.y, 3.0)),
        CitySprite(entity),
    ));
    let font: Handle<Font> = assets.load("cache/fonts/lsans.ttf");
    // Civ3 paints the badge in the owner's civ color.
    let civ_color = CIVS[city.civ].color;
    let origin = label_origin(city);
    let z = sprite_z(city.x, city.y, 5.0);
    // Initial layout from a character-count estimate; the sync lays it out
    // exactly from the measured text on the next frame.
    let est = estimate_text_width(&banner_text(map, city, viewer));
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
                        color: badge_frame(civ_color),
                        custom_size: Some(Vec2::new(BADGE_W, LABEL_H)),
                        ..default()
                    },
                    Transform::from_xyz(layout.badge, 0.0, 0.0),
                    part(LabelPiece::Badge),
                ))
                .with_children(|badge| {
                    let inner = Vec2::new(BADGE_W, LABEL_H) - 2.0 * BADGE_FRAME;
                    badge.spawn((
                        Sprite {
                            color: civ_color,
                            custom_size: Some(inner),
                            ..default()
                        },
                        Transform::from_xyz(0.0, 0.0, 0.01),
                        part(LabelPiece::BadgeFill),
                    ));
                    badge.spawn((
                        Text2d::new(city.size().to_string()),
                        TextFont {
                            font: font.clone(),
                            font_size: 16.0,
                            ..default()
                        },
                        TextColor(CIV_BADGE_INK),
                        Transform::from_xyz(0.0, 0.0, 0.02),
                        CityBadgeText(entity),
                    ));
                });
            for (piece, color, y) in [
                (LabelPiece::NameBand, NAME_BAND, LINE_H / 2.0),
                (LabelPiece::BuildBand, BUILD_BAND, -LINE_H / 2.0),
            ] {
                label.spawn((
                    Sprite {
                        color,
                        custom_size: Some(Vec2::new(layout.box_w, LINE_H)),
                        ..default()
                    },
                    Transform::from_xyz(layout.text, y, 0.0),
                    part(piece),
                ));
            }
            label.spawn((
                Text2d::new(banner_text(map, city, viewer)),
                TextFont {
                    font: font.clone(),
                    font_size: LABEL_FONT,
                    ..default()
                },
                bevy::text::LineHeight::Px(LINE_H),
                TextColor(LABEL_INK),
                // Civ3 sizes the bands to the text and never wraps it.
                TextLayout::new(Justify::Center, LineBreak::NoWrap),
                Transform::from_xyz(layout.text, 0.0, 0.02),
                CityBanner(entity),
                part(LabelPiece::Text),
            ));
            label.spawn((
                Sprite {
                    image: star.0.clone(),
                    custom_size: Some(Vec2::splat(17.0)),
                    ..default()
                },
                Transform::from_xyz(layout.star, 0.0, 0.0),
                if is_capital {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                },
                part(LabelPiece::Star),
            ));
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

/// Badge, text bands and (for the capital) star, abutting, with the whole
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

/// Fallback text width before the first layout lands: ~6 px per
/// character of the longest line at 11 px.
fn estimate_text_width(s: &str) -> f32 {
    s.lines().map(|l| l.chars().count()).max().unwrap_or(0) as f32 * 6.0
}

/// The computer's settler asks to found a city where it stands.
#[derive(Message, Clone, Copy)]
pub struct FoundCityOrder(pub Entity);

/// What founding a city touches besides the map and the units.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Founding<'w> {
    names: ResMut<'w, CityNamesUsed>,
    art: Res<'w, CityArt>,
    assets: Res<'w, AssetServer>,
    star: Res<'w, CapitalStar>,
    capital: ResMut<'w, Capital>,
    audio: Res<'w, GameAudio>,
    turn: Res<'w, units::Turn>,
    flips: Option<Res<'w, crate::flip::Flips>>,
}

/// Put a new city on the map: borders, the governor's first picks, the
/// entity and its sprites. The first city of a civ (or its palace city) is
/// the capital. Returns the city as placed.
pub(crate) fn place_city(
    commands: &mut Commands,
    f: &mut Founding,
    map: &mut GameMap,
    others: &[&City],
    mut city: City,
    viewer: usize,
) -> City {
    let (civ, x, y) = (city.civ, city.x, city.y);
    // Founding recomputes every border at once (`0x4AE5EE`), so the
    // new city works only its own land from the first turn.
    let all: Vec<&City> = others.iter().copied().chain(std::iter::once(&city)).collect();
    let rating = f.flips.as_ref().map(|r| r.empire).unwrap_or_default();
    recompute_borders(map, &all, &rating);
    let taken = taken_tiles(map, all.iter().copied(), (x, y));
    governor_assign(map, &mut city, &taken);
    let entity = commands.spawn(city.clone()).id();
    let palace = crate::roles::palace().is_some_and(|p| city.buildings.contains(&p));
    let is_capital = f.capital.0[civ].is_none() || palace;
    if is_capital {
        f.capital.0[civ] = Some(entity);
    }
    spawn_city_visuals(commands, &f.art, &f.assets, &f.star, map, entity, &city, is_capital, viewer);
    city
}

/// Found a city with the selected settler (B key), or with a settler the
/// computer orders to.
pub fn found_city(
    mut commands: Commands,
    mut cmds: MessageReader<crate::actionbar::UnitCommand>,
    mut orders: MessageReader<FoundCityOrder>,
    selected: Res<Selected>,
    units: Query<(Entity, &Unit)>,
    cities: Query<&City>,
    mut map: ResMut<GameMap>,
    mut f: Founding,
    view: Res<CityView>,
    splash: Res<SplashUp>,
    civs: Res<Civilizations>,
) {
    let asked = cmds
        .read()
        .any(|c| *c == crate::actionbar::UnitCommand::FoundCity);
    let mut requests: Vec<Entity> = orders.read().map(|o| o.0).collect();
    if asked && view.0.is_none() && !splash.0 {
        requests.extend(selected.0);
    }
    // Cities founded in this call are not in the query yet.
    let mut spots: Vec<(i32, i32)> = cities.iter().map(|c| (c.x, c.y)).collect();
    let mut founded: Vec<City> = vec![];
    for s in requests {
        let Ok((e, u)) = units.get(s) else {
            continue;
        };
        // Only the civ whose turn it is may settle, and only with its own
        // settler: the hotseat player never founds for another civ.
        if !crate::roles::founds_cities(u.utype) || u.civ != civs.active {
            continue;
        }
        let civ = u.civ;
        if !can_found(&map, &spots, u.x, u.y) {
            continue;
        }
        let (x, y) = (u.x, u.y);
        commands.entity(e).despawn();
        spots.push((x, y));
        // Settling absorbs any hut or camp on the tile (no reward); a
        // resource underneath stays for future trade.
        let i = map.idx(x, y);
        map.tiles[i].hut = false;
        map.tiles[i].camp = false;
        // Civ3 cities carry a road on their tile.
        map.tiles[i].road = true;
        let city = City {
            coastal: map.coastal_site(x, y),
            river: map.get(x, y).is_some_and(|t| t.river != 0),
            founded: f.turn.0,
            ..City::new(civ, next_name(&mut f.names, civ), x, y)
        };
        let others: Vec<&City> = cities.iter().chain(founded.iter()).collect();
        let city = place_city(&mut commands, &mut f, &mut map, &others, city, civs.viewer());
        // The computer founds quietly unless the human can see it happen.
        if map.get(x, y).is_some_and(|t| t.visible) || !crate::civs::is_ai(civ) {
            commands.spawn(AudioPlayer(f.audio.build.clone()));
        }
        founded.push(city);
    }
}

pub fn end_turn_cities(
    mut commands: Commands,
    mut ended: MessageReader<CivilizationEnded>,
    map: Res<GameMap>,
    mut cities: Query<(Entity, &mut City)>,
    units: Query<(Entity, &Unit)>,
    art: Res<UnitArt>,
    audio: Res<GameAudio>,
    mut board: ResMut<MessageBoard>,
    capital: Res<Capital>,
    mut treasury: ResMut<Treasury>,
    mut prompts: ResMut<crate::production_prompt::ProductionPrompts>,
    mut view: ResMut<CityView>,
    mut menu: ResMut<BuildMenu>,
    mut rng: ResMut<crate::combat::CombatRng>,
    mut abandon: ResMut<crate::abandon::Abandon>,
) {
    // One civ's cities grow, build and pay taxes when that civ hands the
    // hotseat over; the other civs' cities wait for their own turn.
    // Despawns wait for the end of the system, so when several turns end in
    // one frame (a script's `end n`) the units already let go stay listed:
    // they are skipped, not paid for or disbanded twice.
    let mut disbanded: HashSet<Entity> = HashSet::new();
    for ev in ended.read() {
        let civ = ev.0;
        // The computer's books are none of the human's business: whatever
        // it would announce is dropped.
        let quiet = crate::civs::is_ai(civ).then(|| (board.text.clone(), board.ttl));
        // A revolution runs down a turn at a time; its last turn hands the
        // civ the government it chose.
        if crate::realm::write(civ, |r| r.tick()) {
            post(
                &mut board,
                format!(
                    "The revolution is over. Our people now live under {}.",
                    crate::realm::GOVT_NAMES[crate::realm::read(civ, |r| r.govt)]
                ),
            );
        }
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
        let nations = crate::resistance::Nations::current();
        // The books close first, on the cities and units as they stand:
        // the info box showed this very `Finance` all turn, so whatever a
        // city grows or completes below is charged from the next turn on.
        let owned: Vec<(Entity, UnitType)> = units
            .iter()
            .filter(|(e, u)| u.civ == civ && !disbanded.contains(e))
            .map(|(e, u)| (e, u.utype))
            .collect();
        let fin = economy::finance(
            &map,
            cities.iter().filter(|(_, c)| c.civ == civ).map(|(_, c)| c),
            owned.len(),
        );
        // Culture accrues and the tax is collected, city by city.
        let mut collected = 0u32;
        for &e in &order {
            if let Ok((_, mut city)) = cities.get_mut(e) {
                collected += accrue(&map, &mut city, capital.0[civ] == Some(e));
            }
        }
        debug_assert_eq!(collected, fin.tax, "the info box promised another tax");
        // The tax comes in, then the two bills go out: improvements, then
        // units (`economy::pay`). Upkeep the gold cannot cover sells one
        // improvement, whose price joins the treasury before the units are
        // paid; units it cannot cover disband one unit.
        let upkeep = economy::pay(treasury.0[civ].saturating_add(fin.tax), fin.upkeep);
        let mut gold = upkeep.treasury;
        if upkeep.short > 0 {
            let mine = cities.iter().filter(|(_, c)| c.civ == civ);
            if let Some((e, i)) = economy::sale_pick(mine)
                && let Ok((_, mut city)) = cities.get_mut(e)
            {
                let sold = city.buildings.remove(i);
                gold = gold.saturating_add(economy::sale_price(civ, sold));
                post(
                    &mut board,
                    format!(
                        "We can no longer support our {} at {}. We must think more about our treasury!",
                        sold.name(),
                        city.name
                    ),
                );
            }
        }
        let support = economy::pay(gold, fin.unit_cost);
        treasury.0[civ] = support.treasury;
        if support.short > 0
            && let Some((e, kind)) = economy::disband_pick(owned.iter().copied())
        {
            disbanded.insert(e);
            commands.entity(e).despawn();
            post(
                &mut board,
                format!(
                    "We have insufficient gold to continue supporting all our units. One {} unit will be disbanded. (Someone should be looking after our treasury!)",
                    units::def(kind).name
                ),
            );
        }
        for e in order {
            // Later cities see the tiles earlier ones claimed this turn,
            // and any civ's city keeps a tile out of another's radius.
            let me = cities.get(e).map(|(_, c)| (c.x, c.y)).unwrap();
            let taken = taken_tiles(&map, cities.iter().map(|(_, c)| c), me);
            let Ok((_, mut city)) = cities.get_mut(e) else {
                continue;
            };
            // The computer's governor keeps order by putting citizens on
            // entertainment; the human sees the disorder and decides.
            if crate::civs::is_ai(civ) {
                citycalc::keep_order(&map, &mut city);
                citycalc::assign_specialists(&map, &mut city);
            }
            let completed = city.production;
            // The Statue of Zeus and Knights Templar hand out units.
            for unit in citycalc::produced_units(&mut city, |g| crate::research::owns_good(civ, g)) {
                let level = if unit.row().attack > 0 { crate::combat::Level::Veteran } else { crate::combat::Level::Regular };
                units::spawn_unit_at_level(&mut commands, &art, unit, city.x, city.y, civ, level);
                if !crate::civs::is_ai(civ) {
                    post(&mut board, format!("{} produces a free {}.", city.name, units::def(unit).name));
                }
            }
            // Sequencer steps 5 and 6: nationality drift, then the
            // garrison quells resisters (`city-turn.md` 8).
            let police = crate::realm::read(civ, |r| r.garrison.get(&(city.x, city.y)).copied().unwrap_or(0)) as i32;
            let notices = crate::resistance::city_step(&mut city, police, &nations, &mut rng.0);
            use crate::resistance::Notice;
            if notices.iter().any(|n| matches!(n, Notice::Quelled(_) | Notice::Ends)) {
                // Quelled citizens go back to work.
                governor_fill(&map, &mut city, &taken);
            }
            for notice in notices {
                if crate::civs::is_ai(civ) {
                    continue;
                }
                post(&mut board, match notice {
                    Notice::Quelled(n) => format!("Our troops have quelled {n} {} in {}!", if n == 1 { "resister" } else { "resisters" }, city.name),
                    Notice::Ends => format!("The Resistance in {} has ended!", city.name),
                    Notice::Assimilated(n) => format!("{n} {} of {} adopted our nationality.", if n == 1 { "citizen" } else { "citizens" }, city.name),
                });
            }
            let mut disease_notice = None;
            for event in process_city_turn(&map, &mut city, &taken, &mut rng.0) {
                match event {
                    CityEvent::Abandon => abandon.push(e, &city),
                    CityEvent::Completed(unit, nationality) => {
                        // The decision waits for the civ's next turn; the
                        // unit itself joins the civ that built it.
                        if !crate::civs::is_ai(civ) {
                            prompts.push(e, civ, completed);
                        }
                        // Barracks turn out Veterans (`Civilopedia`
                        // #BLDG_Barracks), but only soldiers have a rank.
                        let level = if citycalc::trains_veterans(&city, unit) {
                            crate::combat::Level::Veteran
                        } else {
                            crate::combat::Level::Regular
                        };
                        let produced = units::spawn_unit_at_level(
                            &mut commands,
                            &art,
                            unit,
                            city.x,
                            city.y,
                            civ,
                            level,
                        );
                        commands.entity(produced).queue(move |mut entity: EntityWorldMut| {
                            entity.get_mut::<Unit>().unwrap().nationality = nationality;
                        });
                        if quiet.is_none() {
                            audio::sfx(&mut commands, &audio, "WhatToBuild");
                        }
                        post(
                            &mut board,
                            format!("{} completes {}.", city.name, units::def(unit).name),
                        );
                    }
                    CityEvent::Built(p) => {
                        if !crate::civs::is_ai(civ) {
                            prompts.push(e, civ, p);
                        }
                        if quiet.is_none() {
                            audio::sfx(&mut commands, &audio, "WhatToBuild");
                        }
                        post(&mut board, format!("{} completes {}.", city.name, p.name()));
                    }
                    CityEvent::Disorder => {
                        post(&mut board, format!("Civil disorder in {}! Lower the taxes, add entertainers or build temples.", city.name));
                        let capital_here = capital.0[civ] == Some(e);
                        if city.unrest > 1
                            && let Some(lost) = citycalc::riot(&mut rng.0, &city, capital_here)
                        {
                            let sold = city.buildings.remove(lost);
                            post(&mut board, format!("Rioters destroy the {} in {}!", sold.name(), city.name));
                        }
                    }
                    CityEvent::Disease(terrain) => disease_notice = Some(match terrain {
                        Some(t) => format!("Disease from {} has killed a citizen in {}!", crate::ruleset::TERRAINS[t].name, city.name),
                        None => format!("Disease has killed another citizen in {}!", city.name),
                    }),
                    CityEvent::Calm => post(&mut board, format!("Order is restored in {}.", city.name)),
                    CityEvent::WonderLost(lost, next) => post(
                        &mut board,
                        format!(
                            "Because {} can no longer work on {}, production has been switched to {}.",
                            city.name,
                            lost.name(),
                            next.name()
                        ),
                    ),
                    CityEvent::Blocked => post(
                        &mut board,
                        format!(
                            "{} needs an {} to grow any larger.",
                            city.name,
                            if city.size() as i32 >= civ3mapgen::economy::CITY_MAX { "Hospital" } else { "Aqueduct" }
                        ),
                    ),
                    CityEvent::TooSmall => post(
                        &mut board,
                        format!(
                            "{} must reach size {} to finish its {}.",
                            city.name, completed.unit().map_or(1, |u| units::def(u).pop_cost + 1), completed.name()
                        ),
                    ),
                    CityEvent::Grew => post(
                        &mut board,
                        format!("{} grows to size {}.", city.name, city.size()),
                    ),
                    CityEvent::Starved => post(
                        &mut board,
                        format!("{} starves to size {}.", city.name, city.size()),
                    ),
                }
            }
            if let Some(notice) = disease_notice {
                post(&mut board, notice);
            }
        }
        if let Some((text, ttl)) = quiet {
            board.text = text;
            board.ttl = ttl;
        }
    }
}

/// Keep the active civ's worked tiles legal as the world changes around
/// them. A city founded on a tile, a border that grew over it, or a
/// neighbor's pick takes it from the city; the governor fills the gap.
///
/// It runs after `refresh_visibility`, which keeps `Tile::seen` for the civ
/// in play, so it only ever touches that civ's cities: the others are
/// mended when their turn begins (`civs` changes then). Cities nothing
/// affects are not written, so they do not read as changed.
pub fn reconcile_tiles(
    civs: Res<Civilizations>,
    map: Res<GameMap>,
    mut cities: Query<(Entity, &mut City)>,
) {
    let touched = cities.iter_mut().any(|(_, c)| c.is_changed());
    if !civs.is_changed() && !touched {
        return;
    }
    let order: Vec<Entity> = cities
        .iter()
        .filter(|(_, c)| c.civ == civs.active)
        .map(|(e, _)| e)
        .collect();
    for e in order {
        let Ok((_, city)) = cities.get(e) else {
            continue;
        };
        let taken = taken_tiles(&map, cities.iter().map(|(_, c)| c), (city.x, city.y));
        if !city.worked(&map).iter().any(|t| taken.contains(t)) {
            continue;
        }
        let Ok((_, mut city)) = cities.get_mut(e) else {
            continue;
        };
        prune_worked(&map, &mut city, &taken);
        governor_fill(&map, &mut city, &taken);
    }
}

/// How much light a remembered city keeps: the 60% of a remembered tile
/// (`render::FOG_SHEET`). City art and labels sit above the fog diamonds,
/// so they dim themselves; a game screenshot shows a fogged city's label
/// text at about half the brightness of a lit one.
const FOGGED: f32 = 0.6;

/// True when the viewer sees the city's tile now, not just remembers it.
fn city_lit(map: &GameMap, reveal: &RevealAll, city: &City) -> bool {
    reveal.0 || map.get(city.x, city.y).is_some_and(|t| t.visible)
}

/// `c` with its color scaled by `k`, its alpha kept.
fn shade(c: Color, k: f32) -> Color {
    let s = c.to_srgba();
    Color::srgba(s.red * k, s.green * k, s.blue * k, s.alpha)
}

pub fn sync_city_visuals(
    map: Res<GameMap>,
    art: Res<CityArt>,
    civs: Res<Civilizations>,
    research: Res<crate::research::Research>,
    reveal: Res<RevealAll>,
    cities: Query<&City>,
    capital: Res<Capital>,
    mut sprites: Query<(&CitySprite, &mut Sprite), Without<LabelPart>>,
    mut banners: Query<(&CityBanner, &mut Text2d, &mut TextColor), Without<CityBadgeText>>,
    layouts: Query<(&CityBanner, &TextLayoutInfo)>,
    mut badge_texts: Query<(&CityBadgeText, &mut Text2d, &mut TextColor), Without<CityBanner>>,
    mut parts: Query<
        (&LabelPart, &mut Transform, Option<&mut Sprite>, &mut Visibility),
        Without<CitySprite>,
    >,
) {
    let viewer = civs.viewer();
    let light = |city: &City| if city_lit(&map, &reveal, city) { 1.0 } else { FOGGED };
    for (link, mut sprite) in sprites.iter_mut() {
        if let Ok(city) = cities.get(link.0) {
            let era = crate::tech_tree::era_of(&research, city.civ);
            sprite.image = if crate::roles::walls().is_some_and(|w| city.buildings.contains(&w)) {
                art.wall(culture_group(city.civ), era)
            } else {
                art.graphic(city.size(), culture_group(city.civ), era)
            };
            sprite.color = shade(Color::WHITE, light(city));
        }
    }
    for (link, mut banner, mut ink) in banners.iter_mut() {
        if let Ok(city) = cities.get(link.0) {
            banner.0 = banner_text(&map, city, viewer);
            ink.0 = shade(LABEL_INK, light(city));
        }
    }
    for (link, mut text, mut ink) in badge_texts.iter_mut() {
        if let Ok(city) = cities.get(link.0) {
            text.0 = city.size().to_string();
            ink.0 = shade(CIV_BADGE_INK, light(city));
        }
    }
    // Lay the pieces out around the label center from the measured text.
    for (part, mut tf, mut sprite, mut vis) in parts.iter_mut() {
        let Ok(city) = cities.get(part.city) else {
            continue;
        };
        let w = layouts
            .iter()
            .find(|(b, _)| b.0 == part.city)
            .map(|(_, l)| l.size.x)
            .filter(|w| *w >= 1.0)
            .unwrap_or_else(|| estimate_text_width(&banner_text(&map, city, viewer)));
        let layout = label_layout(w, capital.0[city.civ] == Some(part.city));
        // A captured city wears its new owner's color, and a lost Palace
        // takes its star down.
        let is_capital = capital.0[city.civ] == Some(part.city);
        let k = light(city);
        let color = match part.piece {
            LabelPiece::Badge => {
                tf.translation.x = layout.badge;
                badge_frame(CIVS[city.civ].color)
            }
            LabelPiece::BadgeFill => CIVS[city.civ].color,
            LabelPiece::Star => {
                tf.translation.x = layout.star;
                *vis = if is_capital {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                Color::WHITE
            }
            LabelPiece::Text => {
                tf.translation.x = layout.text;
                continue;
            }
            LabelPiece::NameBand | LabelPiece::BuildBand => {
                tf.translation.x = layout.text;
                if let Some(sprite) = sprite.as_mut() {
                    sprite.custom_size = Some(Vec2::new(layout.box_w, LINE_H));
                }
                if part.piece == LabelPiece::NameBand {
                    NAME_BAND
                } else {
                    BUILD_BAND
                }
            }
        };
        if let Some(mut sprite) = sprite {
            sprite.color = shade(color, k);
        }
    }
}

pub fn city_visibility(
    map: Res<GameMap>,
    reveal: Res<RevealAll>,
    cities: Query<&City>,
    mut sprites: Query<
        (&CitySprite, &mut Visibility),
        (Without<CityBanner>, Without<CityLabelBack>),
    >,
    // Bar roots carry the whole label (badge, text, star) as children, so
    // fog only syncs them; the children inherit.
    mut bars: Query<(&CityLabelBack, &mut Visibility), (Without<CitySprite>, Without<CityBanner>)>,
) {
    for (link, mut vis) in bars.iter_mut() {
        *vis = city_vis(&map, &cities, &reveal, link.0);
    }
    for (link, mut vis) in sprites.iter_mut() {
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
    Governor,
    Specialist(usize),
    /// Build this; a shift-click queues it instead.
    Pick(Production),
    Unqueue(usize),
    CloseMenu,
    /// Turn the build list's page by this many.
    MenuPage(i32),
    /// Step to the previous or next city, as the top bar's arrows do.
    PrevCity,
    NextCity,
    /// Civ3's hurry button: quote the price, then ask.
    Hurry,
    /// The two answers of the hurry question.
    HurryYes,
    HurryNo,
    SwitchYes,
    SwitchNo,
    AbandonYes,
    AbandonNo,
    AbandonZoom,
}

/// A hurry the player is being asked to confirm: the city and its price.
#[derive(Resource, Default)]
pub struct HurryAsk(pub Option<(Entity, crate::hurry::Offer)>);

/// Whether the build-list modal is open over the city screen, and which
/// page of it.
#[derive(Resource, Default)]
pub struct BuildMenu(pub bool, pub usize);

/// Rows on one page of the build list: the roster has dozens of items, and
/// twelve fit between the top bar and the production button.
const MENU_ROWS: usize = 12;

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

/// A text bundle for a flex child: font, size and color, no position.
fn span(font: &Handle<Font>, s: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(s),
        TextFont {
            font: font.clone(),
            font_size: size,
            ..default()
        },
        TextColor(color),
    )
}

/// Single-line text aligned within `w` pixels from `x`. A flex box does
/// the aligning: `Justify` only moves lines inside the text's own measured
/// width, which a single line already fills.
fn txt_aligned(
    parent: &mut ChildSpawnerCommands<'_>,
    font: &Handle<Font>,
    s: &str,
    size: f32,
    (x, y, w): (f32, f32, f32),
    align: JustifyContent,
    color: Color,
) {
    parent
        .spawn(Node {
            position_type: PositionType::Absolute,
            left: Val::Px(x),
            top: Val::Px(y),
            width: Val::Px(w),
            justify_content: align,
            ..default()
        })
        .with_children(|b| {
            b.spawn((
                span(font, s, size, color),
                TextLayout::new(Justify::Left, LineBreak::NoWrap),
            ));
        });
}

/// Centered single-line text across `w` pixels from `x`.
fn txt_centered(
    parent: &mut ChildSpawnerCommands<'_>,
    font: &Handle<Font>,
    s: &str,
    size: f32,
    x: f32,
    y: f32,
    w: f32,
    color: Color,
) {
    txt_aligned(parent, font, s, size, (x, y, w), JustifyContent::Center, color);
}

/// Unit icon or cropped `buildings-small.png` cell for a build item.
fn build_icon(assets: &AssetServer, p: Production) -> ImageNode {
    match p.building_rect() {
        Some(rect) => {
            let mut n = ImageNode::new(assets.load("cache/cityscreen/buildings-small.png"));
            n.rect = Some(rect);
            n
        }
        None => unit_icon_node(assets, p),
    }
}

/// The 32-px icon of a unit production, cropped from the Conquests sheet.
fn unit_icon_node(assets: &AssetServer, p: Production) -> ImageNode {
    let mut n = ImageNode::new(assets.load("cache/ui/unit_icons.png"));
    n.rect = p.unit_icon_rect();
    n
}

fn turns_for(cost: u16, have: u16, rate: u8) -> String {
    if rate == 0 {
        "never".into()
    } else {
        format!("{}t", ceil_div(cost.saturating_sub(have), rate))
    }
}

/// The domestic advisor's hurry question: the line, then the two answers,
/// the second of which cancels (`#HURRY_GOLD`, `#HURRY_PEOPLE`).
fn hurry_question(content: &mut ChildSpawnerCommands<'_>, font: &Handle<Font>, q: (String, &str, &str)) {
    content
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(MENU_X),
                top: Val::Px(MENU_BOTTOM - 120.0),
                width: Val::Px(MENU_W),
                padding: UiRect::all(Val::Px(8.0)),
                row_gap: Val::Px(4.0),
                flex_direction: FlexDirection::Column,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(MENU_FILL),
            BorderColor::all(MENU_EDGE),
        ))
        .with_children(|m| {
            m.spawn(span(font, q.0, 14.0, INK));
            for (label, kind) in [(q.1, ScreenButton::HurryYes), (q.2, ScreenButton::HurryNo)] {
                m.spawn((
                    Button,
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(MENU_ROW_H),
                        padding: UiRect::horizontal(Val::Px(8.0)),
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BackgroundColor(menu_row_color(Interaction::None, false)),
                    MenuRow { current: false },
                    kind,
                ))
                .with_children(|row| {
                    row.spawn(span(font, label.to_string(), 13.0, INK));
                });
            }
        });
}

fn build_menu(
    content: &mut ChildSpawnerCommands<'_>,
    assets: &AssetServer,
    font: &Handle<Font>,
    city: &City,
    rate: u8,
    page: usize,
) {
    let all = city.buildable();
    let pages = all.len().div_ceil(MENU_ROWS).max(1);
    let page = page.min(pages - 1);
    let items: Vec<Production> = all.iter().skip(page * MENU_ROWS).take(MENU_ROWS).copied().collect();
    // Civ3's popup list: a cream box of "Name (N turns)" rows, opening
    // over the view just above the production button.
    let height = 8.0 + items.len() as f32 * MENU_ROW_H + 30.0;
    content
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(MENU_X),
                top: Val::Px(MENU_BOTTOM - height),
                width: Val::Px(MENU_W),
                height: Val::Px(height),
                padding: UiRect::vertical(Val::Px(4.0)),
                flex_direction: FlexDirection::Column,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(MENU_FILL),
            BorderColor::all(MENU_EDGE),
        ))
        .with_children(|m| {
            for p in &items {
                let have = if *p == city.production { city.shields } else { 0 };
                let turns = if rate == 0 {
                    "never".to_string()
                } else {
                    let n = ceil_div(city.price(*p).saturating_sub(have), rate);
                    format!("{n} turn{}", if n == 1 { "" } else { "s" })
                };
                let current = *p == city.production;
                m.spawn((
                    Button,
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(MENU_ROW_H),
                        padding: UiRect::horizontal(Val::Px(8.0)),
                        column_gap: Val::Px(10.0),
                        align_items: AlignItems::Center,
                        flex_shrink: 0.0,
                        ..default()
                    },
                    BackgroundColor(menu_row_color(Interaction::None, current)),
                    MenuRow { current },
                    ScreenButton::Pick(*p),
                ))
                .with_children(|row| {
                    row.spawn((
                        build_icon(assets, *p),
                        Node {
                            width: Val::Px(26.0),
                            height: Val::Px(26.0),
                            flex_shrink: 0.0,
                            ..default()
                        },
                    ));
                    row.spawn((
                        span(font, format!("{} ({turns})", p.name()), 14.0, INK),
                        TextLayout::new(Justify::Left, LineBreak::NoWrap),
                        Node {
                            flex_shrink: 0.0,
                            ..default()
                        },
                    ));
                    row.spawn((
                        span(font, p.blurb(), 11.0, INK_SOFT),
                        TextLayout::new(Justify::Right, LineBreak::NoWrap),
                        Node {
                            flex_grow: 1.0,
                            justify_content: JustifyContent::FlexEnd,
                            overflow: Overflow::clip(),
                            ..default()
                        },
                    ));
                });
            }
            // Footer: the queue hint, the pages, and a way out.
            m.spawn(Node {
                width: Val::Percent(100.0),
                height: Val::Px(26.0),
                margin: UiRect::top(Val::Px(4.0)),
                padding: UiRect::horizontal(Val::Px(8.0)),
                column_gap: Val::Px(6.0),
                align_items: AlignItems::Center,
                border: UiRect::top(Val::Px(1.0)),
                ..default()
            })
            .insert(BorderColor::all(MENU_RULE))
            .with_children(|f| {
                f.spawn((
                    span(font, "Shift-click to queue", 11.0, INK_SOFT),
                    Node {
                        flex_grow: 1.0,
                        ..default()
                    },
                ));
                let link = |f: &mut ChildSpawnerCommands<'_>, label: String, kind: ScreenButton| {
                    f.spawn((
                        Button,
                        Node {
                            padding: UiRect::axes(Val::Px(6.0), Val::Px(2.0)),
                            ..default()
                        },
                        BackgroundColor(Color::NONE),
                        kind,
                    ))
                    .with_children(|b| {
                        b.spawn(span(font, label, 12.0, INK));
                    });
                };
                if pages > 1 {
                    link(f, "<".into(), ScreenButton::MenuPage(-1));
                    f.spawn(span(font, format!("{}/{}", page + 1, pages), 11.0, INK_SOFT));
                    link(f, ">".into(), ScreenButton::MenuPage(1));
                }
                link(f, "Close".into(), ScreenButton::CloseMenu);
            });
        });
}

/// A row of the build list, for its hover highlight.
#[derive(Component)]
pub(crate) struct MenuRow {
    current: bool,
}

/// Civ3 lights the row under the pointer orange; the current build keeps
/// a faint wash.
fn menu_row_color(i: Interaction, current: bool) -> Color {
    match (i, current) {
        (Interaction::Hovered | Interaction::Pressed, _) => Color::srgb(0.91, 0.73, 0.42),
        (Interaction::None, true) => Color::srgba(0.91, 0.73, 0.42, 0.30),
        (Interaction::None, false) => Color::NONE,
    }
}

pub fn highlight_menu_rows(
    mut rows: Query<(&Interaction, &MenuRow, &mut BackgroundColor), Changed<Interaction>>,
) {
    for (i, row, mut bg) in rows.iter_mut() {
        bg.0 = menu_row_color(*i, row.current);
    }
}

/// The build list's box: right-aligned over the view, its foot just above
/// the production button.
const MENU_X: f32 = 660.0;
const MENU_W: f32 = 344.0;
const MENU_BOTTOM: f32 = 500.0;
const MENU_ROW_H: f32 = 30.0;
const MENU_RULE: Color = Color::srgb(0.80, 0.74, 0.60);

/// The build list's cream box and its rim.
const MENU_FILL: Color = Color::srgb(0.99, 0.96, 0.87);
const MENU_EDGE: Color = Color::srgb(0.62, 0.58, 0.50);

const CONTENT_W: f32 = 1024.0;
const CONTENT_H: f32 = 768.0;
const CLUSTER_X: f32 = 192.0;
const CLUSTER_Y: f32 = 140.0;
const PARCHMENT_TEXT: Color = Color::srgb(0.23, 0.14, 0.06);

/// The radius cell (rx, ry in the -2..=2 fat cross) under `v`, a world
/// offset from the city's tile center. Edges belong to the neighbor.
pub fn radius_cell(v: Vec2) -> Option<(i32, i32)> {
    let a = v.x / (TILE_W / 2.0);
    let b = -v.y / (TILE_H / 2.0);
    let rx = ((a + b) / 2.0).round() as i32;
    let ry = ((b - a) / 2.0).round() as i32;
    if rx.abs() > 2 || ry.abs() > 2 || (rx.abs() == 2 && ry.abs() == 2) {
        return None;
    }
    let c = tile_to_world(rx, ry);
    if (v.x - c.x).abs() / (TILE_W / 2.0) + (v.y - c.y).abs() / (TILE_H / 2.0) > 1.0 {
        return None;
    }
    Some((rx, ry))
}

/// The map camera as it was before the city screen took it over, and the
/// city the view is parked on.
#[derive(Resource, Default)]
pub struct CityFrame {
    saved: Option<(Vec3, f32)>,
    city: Option<Entity>,
}

/// A diamond dimming one tile of the city view the city may not work:
/// outside the radius or its civ's border, or unexplored.
#[derive(Component)]
pub(crate) struct CityDim;

/// Above every map sprite (`sprite_z` stays under 700) and inside the 2D
/// camera's +-1000 depth range, so units and labels outside the radius dim
/// with their tile, as in Civ3's city view.
const DIM_Z: f32 = 900.0;

/// Where the camera looks so the city sits at the cluster's center: the
/// cluster center is above the content box's middle, and the content box
/// is centered in the window.
fn view_center(city: &City) -> Vec2 {
    let above = CONTENT_H / 2.0 - (CLUSTER_Y + 160.0);
    tile_to_world(city.x, city.y) - Vec2::new(0.0, above)
}

/// Civ3's city view is the map: park the camera on the city at full size
/// while its screen is open, dim what the city may not work, outline what
/// it may in white, and give the camera back on close.
pub fn frame_city_view(
    mut commands: Commands,
    view: Res<CityView>,
    cities: Query<&City>,
    changed: Query<(), Changed<City>>,
    civs: Res<Civilizations>,
    map: Res<GameMap>,
    assets: Res<AssetServer>,
    mut frame: ResMut<CityFrame>,
    mut cam: Query<(&mut Transform, &mut Projection), With<Camera2d>>,
    dims: Query<Entity, With<CityDim>>,
    mut gizmos: Gizmos,
) {
    let Ok((mut tf, mut proj)) = cam.single_mut() else {
        return;
    };
    let Projection::Orthographic(ortho) = &mut *proj else {
        return;
    };
    let open = view
        .0
        .filter(|e| cities.get(*e).is_ok_and(|c| c.civ == civs.active));
    let Some((e, city)) = open.and_then(|e| cities.get(e).ok().map(|c| (e, c))) else {
        if let Some((pos, scale)) = frame.saved.take() {
            tf.translation = pos;
            ortho.scale = scale;
        }
        if frame.city.take().is_some() {
            dims.iter().for_each(|d| commands.entity(d).despawn());
        }
        return;
    };
    if frame.saved.is_none() {
        frame.saved = Some((tf.translation, ortho.scale));
    }
    tf.translation = view_center(city).extend(tf.translation.z);
    ortho.scale = 1.0;

    // Civ3 lights the part of the radius the city may work: inside its
    // civ's border and explored. Tiles another city works stay lit but get
    // a dark double rim.
    let all: Vec<&City> = cities.iter().collect();
    let land = territory(&map);
    let tile_of = |rx: i32, ry: i32| (map.wrap_x(city.x + rx), city.y + ry);
    let lit: HashSet<(i32, i32)> = radius_offsets()
        .filter(|&(rx, ry)| {
            let t = tile_of(rx, ry);
            land.get(&t) == Some(&city.civ)
                && map.get(t.0, t.1).is_some_and(|t| t.seen)
        })
        .collect();
    let elsewhere: HashSet<(i32, i32)> = all
        .iter()
        .filter(|c| (c.x, c.y) != (city.x, city.y))
        .flat_map(|c| c.worked(&map).into_iter())
        .collect();
    let center = tile_to_world(city.x, city.y);
    let hw = TILE_W / 2.0;
    let hh = TILE_H / 2.0;
    let diamond = |c: Vec2, k: f32| {
        [
            Vec2::new(c.x, c.y + hh * k),
            Vec2::new(c.x + hw * k, c.y),
            Vec2::new(c.x, c.y - hh * k),
            Vec2::new(c.x - hw * k, c.y),
        ]
    };
    for &(rx, ry) in &lit {
        let c = center + tile_to_world(rx, ry);
        let [n, e_, s_, w] = diamond(c, 1.0);
        for (next, a, b) in [
            ((rx + 1, ry), e_, s_),
            ((rx - 1, ry), n, w),
            ((rx, ry + 1), s_, w),
            ((rx, ry - 1), n, e_),
        ] {
            if !lit.contains(&next) {
                gizmos.line_2d(a, b, Color::WHITE);
            }
        }
        if elsewhere.contains(&tile_of(rx, ry)) {
            for k in [0.94, 0.86] {
                let d = diamond(c, k);
                for i in 0..4 {
                    gizmos.line_2d(d[i], d[(i + 1) % 4], WORKED_ELSEWHERE);
                }
            }
        }
    }

    if frame.city == Some(e) && changed.is_empty() {
        return;
    }
    frame.city = Some(e);
    dims.iter().for_each(|d| commands.entity(d).despawn());
    let fog = assets.load(crate::render::FOG_SHEET);
    // Every tile whose diamond reaches the view band and that the city may
    // not work.
    for ry in -7i32..=7 {
        for rx in -7i32..=7 {
            let off = tile_to_world(rx, ry);
            if off.x.abs() > CONTENT_W / 2.0 + hw || off.y.abs() > 220.0 + hh || lit.contains(&(rx, ry)) {
                continue;
            }
            let pos = center + off;
            commands.spawn((
                Sprite {
                    image: fog.clone(),
                    rect: Some(crate::blend::cell_rect(0, 0)),
                    color: Color::srgba(0.0, 0.0, 0.0, 0.62),
                    ..default()
                },
                Transform::from_xyz(pos.x, pos.y, DIM_Z),
                CityDim,
            ));
        }
    }
}

/// Civ3's dark double rim on a tile another city works.
const WORKED_ELSEWHERE: Color = Color::srgb(0.26, 0.16, 0.08);

/// Cell of `CityIcons.png`: 30-px icons on a 31-px stride (1-px green
/// separators). 2 = commerce, 4 = shield, 6 = food.
fn city_icon(assets: &AssetServer, cell: u32) -> ImageNode {
    let mut n = ImageNode::new(assets.load("cache/cityscreen/CityIcons.png"));
    let x = 1.0 + cell as f32 * 31.0;
    n.rect = Some(Rect::new(x, 1.0, x + 30.0, 31.0));
    n
}

/// What the city screen draws from: the city and the map around it.
struct ClusterCtx<'a> {
    assets: &'a AssetServer,
    map: &'a GameMap,
    city: &'a City,
}

/// Radius offsets of the 21 city tiles.
fn radius_offsets() -> impl Iterator<Item = (i32, i32)> {
    (-2i32..=2)
        .flat_map(|ry| (-2i32..=2).map(move |rx| (rx, ry)))
        .filter(|(rx, ry)| !(rx.abs() == 2 && ry.abs() == 2))
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
    if ny < 0 || ny >= map.h || !(center || city.worked(&map).contains(&(nx, ny))) {
        return;
    }
    let (f, s, c) = if center {
        citycalc::center(map, city)
    } else {
        citycalc::tile_yields(map, city, nx, ny)
    };
    let (f, s, c) = (f.max(0) as u8, s.max(0) as u8, c.max(0) as u8);
    let icons: Vec<u32> = std::iter::repeat_n(ICON_FOOD, f as usize)
        .chain(std::iter::repeat_n(ICON_SHIELD, s as usize))
        .chain(std::iter::repeat_n(ICON_COMMERCE, c as usize))
        .collect();
    // Civ3 packs the icons in a row across the tile, overlapping them when
    // there are many. The center's row crosses the city's base, above its
    // map label.
    let size = 22.0;
    let step = (84.0 / icons.len().max(1) as f32).min(size - 2.0);
    let width = step * icons.len().saturating_sub(1) as f32 + size;
    let cx = (rx - ry) as f32 * 64.0 + 320.0;
    let cy = (rx + ry) as f32 * 32.0 + 160.0;
    let top = cy + if center { -4.0 } else { -size / 2.0 };
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

fn build_city_screen(
    commands: &mut Commands,
    ctx: &ClusterCtx,
    p: &PanelCtx,
    menu: bool,
    page: usize,
    ask: Option<crate::hurry::Offer>,
) {
    let ClusterCtx {
        assets, map, city, ..
    } = *ctx;
    let font = assets.load("cache/fonts/lsans.ttf");
    let shields_pt = city_income(map, city).1;
    // Black fills the window around the 1024x768 panel; the panel's own
    // art is open over the city view band, where the map shows.
    let black = |w: Val, h: Val| {
        (
            Node {
                width: w,
                height: h,
                flex_grow: 1.0,
                ..default()
            },
            BackgroundColor(Color::BLACK),
        )
    };
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            CityScreenRoot,
            GlobalZIndex(10),
        ))
        .with_children(|root| {
            root.spawn(black(Val::Percent(100.0), Val::Auto));
            root.spawn(Node {
                width: Val::Percent(100.0),
                height: Val::Px(CONTENT_H),
                flex_shrink: 0.0,
                ..default()
            })
            .with_children(|row| {
            row.spawn(black(Val::Auto, Val::Percent(100.0)));
            row.spawn((
                ImageNode::new(assets.load("cache/cityscreen/background.png")),
                Node {
                    width: Val::Px(CONTENT_W),
                    height: Val::Px(CONTENT_H),
                    flex_shrink: 0.0,
                    ..default()
                },
            ))
            .with_children(|content| {
                city_panel(content, ctx, p, &font);
                // right panel over the black view: production queue
                // Civ3 shows the queue box only while the queue is in use.
                if !city.queue.is_empty() {
                    production_queue(content, assets, &font, city, shields_pt);
                }
                // modal build list
                if menu {
                    build_menu(content, assets, &font, city, shields_pt, page);
                }
                if let Some(offer) = ask {
                    hurry_question(content, &font, offer.question(city.production.name()));
                }
            });
            row.spawn(black(Val::Auto, Val::Percent(100.0)));
            });
            root.spawn(black(Val::Percent(100.0), Val::Auto));
        });
}

/// The queue box on the right of the city view: the current build, then
/// what follows it, each with its icon and turns. A queued row is a button
/// that takes it off the queue.
fn production_queue(
    content: &mut ChildSpawnerCommands<'_>,
    assets: &AssetServer,
    font: &Handle<Font>,
    city: &City,
    shields_pt: u8,
) {
    content.spawn((
        ImageNode::new(assets.load("cache/cityscreen/ProductionQueueBox.png")),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(QUEUE_X),
            top: Val::Px(QUEUE_Y),
            width: Val::Px(203.0),
            height: Val::Px(360.0),
            ..default()
        },
    ));
    txt(
        content,
        font,
        "PRODUCTION QUEUE",
        HEADING_TEXT,
        QUEUE_X + 14.0,
        QUEUE_Y + 10.0,
        INK,
    );
    let first_turns = turns_for(city.price(city.production), city.shields, shields_pt);
    let rows = std::iter::once((city.production, first_turns, None)).chain(
        city.queue
            .iter()
            .enumerate()
            .map(|(i, q)| (*q, String::new(), Some(i))),
    );
    for (n, (item, turns, qi)) in rows.take(10).enumerate() {
        let mut row = content.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(QUEUE_X + 10.0),
                top: Val::Px(QUEUE_Y + 32.0 + n as f32 * 30.0),
                width: Val::Px(183.0),
                height: Val::Px(28.0),
                padding: UiRect::horizontal(Val::Px(4.0)),
                column_gap: Val::Px(6.0),
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(if qi.is_none() {
                Color::srgba(0.42, 0.27, 0.10, 0.22)
            } else {
                Color::NONE
            }),
        ));
        if let Some(i) = qi {
            row.insert((Button, ScreenButton::Unqueue(i)));
        }
        row.with_children(|r| {
            r.spawn((
                build_icon(assets, item),
                Node {
                    width: Val::Px(24.0),
                    height: Val::Px(24.0),
                    flex_shrink: 0.0,
                    ..default()
                },
            ));
            r.spawn((
                span(font, item.name(), 13.0, INK),
                TextLayout::new(Justify::Left, LineBreak::NoWrap),
                Node {
                    flex_grow: 1.0,
                    overflow: Overflow::clip(),
                    ..default()
                },
            ));
            // The build's turns; a queued row offers its removal instead.
            let tail = if qi.is_some() { "remove".to_string() } else { turns };
            r.spawn(span(font, tail, 11.0, INK_SOFT));
        });
    }
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
const BAR_PROD: f32 = 522.0;
const BAR_FOOD: f32 = 568.0;
const BAR_COM: f32 = 618.0;
const COM_ROW: f32 = 46.0;
const INK: Color = PARCHMENT_TEXT;
const INK_SOFT: Color = Color::srgb(0.42, 0.30, 0.16);
const LABEL_TEXT: f32 = 14.0;
const SMALL_TEXT: f32 = 13.0;
/// The bottom panel's row headings: they fit the 13-px strips the
/// background leaves above each bar.
const HEADING_TEXT: f32 = 11.0;
/// Top-left of the production queue box over the city view, below the
/// top fade bar.
const QUEUE_X: f32 = 812.0;
const QUEUE_Y: f32 = 118.0;
/// `CityIcons.png` cell indices, as used across the city screen.
const ICON_COMMERCE: u32 = 2;
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

/// Cells in the city screen's food box grid, whatever the city's box size.
const FOOD_BOX_CELLS: usize = 20;

/// Cells of the food grid a store fills: rounded to the nearest cell, and
/// never the whole grid until the box is full, so a city one food short of
/// growing does not look ready.
fn box_cells(food: u8, box_size: u8) -> usize {
    if food >= box_size {
        return FOOD_BOX_CELLS;
    }
    ((food as usize * FOOD_BOX_CELLS + box_size as usize / 2) / box_size as usize)
        .min(FOOD_BOX_CELLS - 1)
}

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
    let mut n = ImageNode::new(assets.load("cache/cityscreen/CityIcons.png"));
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
    let path = format!("cache/features/{}.png", crate::features::art_key(id));
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
        ImageNode::new(assets.load(format!("cache/ui/{stem}_0.png"))),
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
        img.image = assets.load(format!("cache/ui/{}_{state}.png", button.stem));
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
    let box_size = food_box(city.size());
    let totals = citycalc::totals(map, city);
    let (tax, sci, lux) = (totals.tax, totals.sci, totals.lux);
    let commerce = tax + sci + lux;
    let rates = crate::realm::rates(city.civ);
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
    // The city's own column, between the prev/next arrows.
    txt_centered(content, font, &city.name, 24.0, 412.0, 2.0, 196.0, INK);
    txt_centered(
        content,
        font,
        &format!("Founded: {}", crate::calendar::label(city.founded)),
        12.0,
        412.0,
        31.0,
        196.0,
        INK_SOFT,
    );
    let government = crate::realm::GOVT_NAMES[crate::realm::read(city.civ, |r| r.govt)].to_uppercase();
    for (row, (left, right)) in [
        (format!("{} GOLD", p.treasury), government),
        (format!("POP {}{}", city.size(), if city.diseased { " (DISEASE)" } else { "" }), crate::calendar::label(p.turn).to_uppercase()),
    ]
    .into_iter()
    .enumerate()
    {
        let y = 50.0 + row as f32 * 16.0;
        for (s, x, align) in [
            (left, 412.0, JustifyContent::FlexEnd),
            (right, 516.0, JustifyContent::FlexStart),
        ] {
            txt_aligned(content, font, &s, 12.0, (x, y, 92.0), align, INK);
        }
    }
    // Culture: the rate, the progress to the next border expansion inside
    // the bar `background.png` bakes at (717, 35), and the running total.
    content
        .spawn(Node {
            position_type: PositionType::Absolute,
            left: Val::Px(716.0),
            top: Val::Px(8.0),
            column_gap: Val::Px(8.0),
            align_items: AlignItems::Baseline,
            ..default()
        })
        .with_children(|row| {
            row.spawn(span(font, "CULTURE", 12.0, INK));
            row.spawn(span(font, format!("{culture_pt} per turn"), 10.0, INK));
        });
    let progress = (city.culture as f32 / culture_next.max(1) as f32).clamp(0.0, 1.0);
    content.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(719.0),
            top: Val::Px(37.0),
            width: Val::Px(107.0 * progress),
            height: Val::Px(15.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.96, 0.84, 0.30, 0.85)),
    ));
    let expand = if culture_pt == 0 {
        "No expansion".to_string()
    } else {
        let turns = (culture_next - city.culture).div_ceil(culture_pt);
        format!("Expand in {turns} turn{}", if turns == 1 { "" } else { "s" })
    };
    txt_centered(content, font, &expand, 10.0, 717.0, 39.0, 111.0, INK);
    txt_centered(
        content,
        font,
        &format!("Total: {}/{culture_cur}", city.culture),
        11.0,
        717.0,
        62.0,
        111.0,
        INK,
    );
    // Positions from the button sheet's own layout notes.
    panel_button(
        content,
        assets,
        "mgmt_prev",
        368.0,
        21.0,
        42.0,
        47.0,
        ScreenButton::PrevCity,
    );
    panel_button(
        content,
        assets,
        "mgmt_next",
        609.0,
        21.0,
        42.0,
        47.0,
        ScreenButton::NextCity,
    );
    panel_button(
        content,
        assets,
        "mgmt_x",
        909.0,
        21.0,
        39.0,
        47.0,
        ScreenButton::Close,
    );

    // ---- the city's land, with the citizens who work it ----
    // The band is a window onto the map (`frame_city_view` parks the
    // camera on the city), as in Civ3: only the yields and heads are UI.
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
    // Citizens by mood (happy, content, unhappy), then the entertainers.
    let m = totals.mood;
    let kinds = std::iter::repeat_n(Color::srgb(0.75, 1.0, 0.55), m.happy as usize)
        .chain(std::iter::repeat_n(Color::WHITE, m.content as usize))
        .chain(std::iter::repeat_n(Color::srgb(1.0, 0.55, 0.5), m.unhappy as usize))
        .map(|tint| ("cache/ui/citizen.png", tint, None))
        .chain(city.specialist_jobs().enumerate().map(|(i, s)| (s.art(), Color::WHITE, Some(i))));
    let heads: Vec<_> = kinds.take(16).collect();
    let heads_left = CLUSTER_X + 320.0 - heads.len() as f32 * 50.0 / 2.0;
    for (i, (head, tint, specialist)) in heads.into_iter().enumerate() {
        let mut head = ImageNode::new(assets.load(head));
        head.color = tint;
        let mut entity = content.spawn((
            head,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(heads_left + i as f32 * 50.0),
                top: Val::Px(456.0),
                width: Val::Px(49.0),
                height: Val::Px(49.0),
                ..default()
            },
        ));
        if let Some(i) = specialist { entity.insert((Button, ScreenButton::Specialist(i))); }
    }
    if totals.disorder {
        txt(content, font, "CIVIL DISORDER", 20.0, 176.0, 494.0, Color::srgb(0.75, 0.1, 0.05));
    }
    // ---- improvements: icon, name, then what the building does ----
    txt(content, font, "IMPROVEMENTS", HEADING_TEXT, 8.0, 511.0, INK);
    let mut rows: Vec<(&str, Rect, u32, u8, u8)> = vec![];
    if p.capital && !city.buildings.iter().any(|b|
        b.bldg().is_some_and(|b| b.flags & roster::imp::CENTER_OF_EMPIRE != 0)) {
        // A first capital has an implicit Palace; replacements store its row.
        rows.push(("Palace", Rect::new(33.0, 33.0, 65.0, 65.0), 1, 0, 0));
    }
    for b in city.buildings.iter() {
        if let Some(rect) = b.building_rect() {
            rows.push((b.name(), rect, b.culture(), b.upkeep(), b.happy()));
        }
    }
    for (i, (name, rect, culture, upkeep, happy)) in rows.iter().take(7).enumerate() {
        let y = 528.0 + i as f32 * 32.0;
        let mut n = ImageNode::new(assets.load("cache/cityscreen/buildings-small.png"));
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
        txt(content, font, name, 12.0, 38.0, y, INK);
        let mut mx = 38.0;
        if *upkeep > 0 {
            icon(
                content,
                assets,
                ICON_UPKEEP,
                mx,
                y + 16.0,
                14.0,
                Color::WHITE,
            );
            mx += 16.0;
        }
        for _ in 0..*culture {
            icon(
                content,
                assets,
                ICON_CULTURE,
                mx,
                y + 16.0,
                14.0,
                Color::WHITE,
            );
            mx += 12.0;
        }
        for _ in 0..*happy {
            icon(
                content,
                assets,
                ICON_HAPPY,
                mx,
                y + 16.0,
                14.0,
                Color::WHITE,
            );
            mx += 18.0;
        }
    }
    // The list's scrollbar. The clone owns at most four improvements, so
    // there is never anything to scroll: it is the panel's chrome.
    for (art, y) in [("scroll_up_0", 509.0), ("scroll_down_0", 748.0)] {
        content.spawn((
            ImageNode::new(assets.load(format!("cache/ui/{art}.png"))),
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
            ImageNode::new(assets.load("cache/ui/scroll_track.png")),
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
    txt(content, font, "LUXURIES", HEADING_TEXT, 162.0, 511.0, INK);
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
    txt(content, font, "POLLUTION", HEADING_TEXT, 162.0, 719.0, INK);
    // ---- garrison: the units standing in the city ----
    txt(content, font, "GARRISON", HEADING_TEXT, 288.0, 719.0, INK);
    for (i, u) in p.garrison.iter().take(3).enumerate() {
        content.spawn((
            unit_icon_node(assets, Production::from_unit(*u)),
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
    row_heading(content, font, "PRODUCTION", shields_pt.into(), 290.0, 509.0);
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
    row_heading(content, font, "FOOD", food.into(), 290.0, 557.0);
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
    let need = box_size.saturating_sub(city.food) as u32;
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
            ceil_div(box_size.saturating_sub(city.food), net_food as u8)
        )
    };
    txt_centered(
        content,
        font,
        &format!("Grows in {grow_in}"),
        HEADING_TEXT,
        771.0,
        559.0,
        128.0,
        INK_SOFT,
    );
    row_heading(content, font, "COMMERCE", commerce, 290.0, 608.0);
    // Each row leads with its own badge and counts its points in coins,
    // flasks or faces.
    for (row, (amount, rate, icon_cell, point)) in [
        (tax, rates.tax, ICON_TREASURY, ICON_COMMERCE),
        (sci, rates.sci, ICON_FLASK, ICON_FLASK),
        (lux, rates.lux, ICON_HAPPY, ICON_HAPPY),
    ]
    .iter()
    .enumerate()
    {
        let y = BAR_COM + row as f32 * COM_ROW;
        icons_right(
            content,
            assets,
            *point,
            (*amount).max(0) as u32,
            19.3,
            680.0,
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
    // The grid always has twenty cells; a bigger city's cell holds more.
    let filled = box_cells(city.food, box_size);
    for i in 0..FOOD_BOX_CELLS {
        let stored = i >= FOOD_BOX_CELLS - filled;
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
    if citycalc::has_flag(city, roster::imp::KEEPS_FOOD) {
        let row = (FOOD_BOX_CELLS - filled) / 4;
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
    // Civ3's grid: blue shields for what is stored, grey ones for the rest.
    let (icons, per_icon) = shield_grid(city.price(city.production) as usize);
    let stored = city.shields as usize / per_icon;
    for i in 0..icons {
        icon(
            content,
            assets,
            if i < stored { ICON_SHIELD } else { ICON_SHIELD_BOX },
            908.0 + (i % SHIELD_COLS) as f32 * 15.5,
            SHIELD_GRID_Y + (i / SHIELD_COLS) as f32 * 26.0,
            16.0,
            Color::WHITE,
        );
    }
    // The current build sits in Civ3's production button; clicking it
    // opens the build list.
    content
        .spawn((
            Button,
            ImageNode::new(assets.load("cache/ui/prod_0.png")),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(905.0),
                top: Val::Px(516.0),
                width: Val::Px(115.0),
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
                    left: Val::Px(29.0),
                    top: Val::Px(6.0),
                    width: Val::Px(56.0),
                    height: Val::Px(56.0),
                    ..default()
                },
            ));
            box_.spawn((
                span(font, city.production.name(), 12.0, INK),
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(6.0),
                    top: Val::Px(64.0),
                    width: Val::Px(103.0),
                    ..default()
                },
                TextLayout::new_with_justify(Justify::Center),
            ));
        });
    // Civ3's hurry button beside it (the sheet's note: "draw @ (860, 520)").
    content.spawn((
        Button,
        ImageNode::new(assets.load("cache/ui/hurry_0.png")),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(860.0),
            top: Val::Px(520.0),
            width: Val::Px(28.0),
            height: Val::Px(28.0),
            ..default()
        },
        PanelButton { stem: "hurry" },
        ScreenButton::Hurry,
    ));
    let left = city.price(city.production).saturating_sub(city.shields);
    let prod_in = if shields_pt == 0 {
        "never".to_string()
    } else {
        format!("{} turns", ceil_div(left, shields_pt))
    };
    txt_centered(
        content,
        font,
        &format!("Complete in {prod_in}"),
        HEADING_TEXT,
        901.0,
        614.0,
        123.0,
        INK,
    );
    // The governor sits with the citizens it assigns. The build changes
    // through the production button, as in Civ3.
    content
        .spawn((
            Button,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(16.0),
                top: Val::Px(452.0),
                padding: UiRect::axes(Val::Px(12.0), Val::Px(4.0)),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BorderColor::all(Color::srgb(0.45, 0.33, 0.17)),
            BackgroundColor(Color::srgb(0.86, 0.78, 0.60)),
            ScreenButton::Governor,
        ))
        .with_children(|btn| {
            btn.spawn(span(font, "Governor", 13.0, INK));
        });
}

/// One of the bottom panel's row headings: the label, then the per-turn
/// amount in smaller type, as Civ3 prints them.
fn row_heading(
    content: &mut ChildSpawnerCommands<'_>,
    font: &Handle<Font>,
    label: &str,
    per_turn: i32,
    x: f32,
    y: f32,
) {
    content
        .spawn(Node {
            position_type: PositionType::Absolute,
            left: Val::Px(x),
            top: Val::Px(y),
            column_gap: Val::Px(8.0),
            align_items: AlignItems::Baseline,
            ..default()
        })
        .with_children(|row| {
            row.spawn(span(font, label, 12.0, INK));
            row.spawn(span(font, format!("{per_turn} per turn"), 10.0, INK));
        });
}

/// Top of the build's shield grid, under the production button's caption.
const SHIELD_GRID_Y: f32 = 630.0;
/// The grid's shape: rows of seven, five rows at most.
const SHIELD_COLS: usize = 7;
const SHIELD_ROWS: usize = 5;

/// (icons, shields per icon) of the build's shield grid: one icon a shield
/// while the price fits the 7x5 grid, else each icon stands for several.
fn shield_grid(price: usize) -> (usize, usize) {
    let per_icon = price.div_ceil(SHIELD_COLS * SHIELD_ROWS).max(1);
    (price.div_ceil(per_icon), per_icon)
}

pub fn maintain_city_screen(
    mut commands: Commands,
    view: Res<CityView>,
    cities: Query<&City>,
    changed: Query<(), Changed<City>>,
    roots: Query<Entity, With<CityScreenRoot>>,
    mut last: Local<(Option<Entity>, u64)>,
    assets: Res<AssetServer>,
    map: Res<GameMap>,
    menu: Res<BuildMenu>,
    units: Query<&Unit>,
    treasury: Res<Treasury>,
    capital: Res<Capital>,
    turn: Res<units::Turn>,
    civs: Res<Civilizations>,
    hurry: Res<HurryAsk>,
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
    let (strategic, luxuries) = match open_city {
        Some((_, c)) => resources_owned(&map, c.civ),
        None => (vec![], vec![]),
    };
    let sig = open_city
        .map(|(_, c)| {
            let mut h = radius_signature(&map, c);
            h ^= (treasury.0[c.civ] as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
            h ^= (garrison.len() as u64) << 32;
            for u in &garrison {
                h = h.rotate_left(7) ^ u64::from(u.0);
            }
            h ^= ((strategic.len() as u64) << 16) ^ luxuries.len() as u64;
            h
        })
        .unwrap_or(0);
    if (open, sig) == *last && changed.is_empty() && !menu.is_changed() && !hurry.is_changed() {
        return;
    }
    for r in roots.iter() {
        commands.entity(r).despawn();
    }
    *last = (open, sig);
    let Some((e, city)) = open_city else { return };
    let ctx = ClusterCtx {
        assets: &assets,
        map: &map,
        city,
    };
    let panel = PanelCtx {
        capital: capital.0[city.civ] == Some(e),
        treasury: treasury.0[city.civ],
        turn: turn.0,
        garrison: &garrison,
        strategic: &strategic,
        luxuries: &luxuries,
    };
    let ask = hurry.0.filter(|(c, _)| *c == e).map(|(_, o)| o);
    build_city_screen(&mut commands, &ctx, &panel, menu.0, menu.1, ask);
}

pub fn city_screen_input(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cam: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
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
    let Ok((camera, gt)) = cam.single() else {
        return;
    };
    let Ok(world) = camera.viewport_to_world_2d(gt, cursor) else {
        return;
    };
    let Ok(city) = cities.get(e) else {
        return;
    };
    let Some((rx, ry)) = radius_cell(world - tile_to_world(city.x, city.y)) else {
        return;
    };
    if !click_cluster(&map, &mut cities, e, rx, ry) {
        post(
            &mut board,
            "That tile is unexplored, outside our borders, or worked by another city.",
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
    let taken = taken_tiles(map, cities.iter(), (city.x, city.y));
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
    keys: Res<ButtonInput<KeyCode>>,
    mut treasury: ResMut<Treasury>,
    mut ask: ResMut<HurryAsk>,
    mut rng: ResMut<crate::combat::CombatRng>,
    mut switch: ResMut<crate::build_switch::BuildSwitch>,
    abandon: Res<crate::abandon::Abandon>,
) {
    if switch.is_pending() || abandon.blocks(civs.active) { return; }
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    for (interaction, button) in buttons.iter() {
        if *interaction != Interaction::Pressed {
            continue;
        }
        audio::sfx(&mut commands, &audio, "Button OK");
        match button {
            ScreenButton::Close => {
                views.0 = None;
                menu.0 = false;
                ask.0 = None;
            }
            ScreenButton::HurryNo => ask.0 = None,
            ScreenButton::Change => *menu = BuildMenu(true, 0),
            ScreenButton::CloseMenu => menu.0 = false,
            ScreenButton::MenuPage(d) => {
                let pages = views
                    .0
                    .and_then(|e| cities.get(e).ok())
                    .map_or(1, |(_, c)| c.buildable().len().div_ceil(MENU_ROWS).max(1));
                menu.1 = (menu.1 as i32 + d).clamp(0, pages as i32 - 1) as usize;
            }
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
                    .map(|(_, c)| taken_tiles(&map, cities.iter().map(|(_, c)| c), (c.x, c.y)))
                    .unwrap_or_default();
                let Some((_, mut city)) = views.0.and_then(|e| cities.get_mut(e).ok()) else {
                    continue;
                };
                match button {
                    ScreenButton::Specialist(i) => city.cycle_specialist(*i),
                    ScreenButton::Governor => {
                        governor_assign(&map, &mut city, &taken);
                        // The governor also puts the unhappy to entertain
                        // when the city would riot.
                        citycalc::keep_order(&map, &mut city);
                    }
                    // Shift-click adds to the queue and leaves the list
                    // open, as in Civ3.
                    ScreenButton::Pick(p) if shift => {
                        if !city.enqueue(*p) {
                            post(&mut board, "Cannot queue that item.");
                        }
                    }
                    ScreenButton::Pick(p) => {
                        switch.request(views.0.unwrap(), &mut city, *p);
                        menu.0 = false;
                    }
                    ScreenButton::Unqueue(i) => city.dequeue(*i),
                    ScreenButton::Hurry => {
                        menu.0 = false;
                        let how = crate::realm::govt(city.civ).hurry;
                        match crate::hurry::quote(&city, how, treasury.0[city.civ], crate::hurry::Buyer::Human) {
                            Ok(offer) => ask.0 = views.0.map(|e| (e, offer)),
                            Err(no) => post(&mut board, no.text(city.production.name())),
                        }
                    }
                    ScreenButton::HurryYes => {
                        // The price is quoted again, as the executor does:
                        // the city may have changed since the question.
                        let how = crate::realm::govt(city.civ).hurry;
                        ask.0 = None;
                        match crate::hurry::quote(&city, how, treasury.0[city.civ], crate::hurry::Buyer::Human) {
                            Ok(offer) => {
                                let civ = city.civ;
                                crate::hurry::apply(&map, &mut city, &taken, offer, &mut treasury.0[civ], &mut rng.0);
                                post(&mut board, format!("{} hurried.", city.production.name()));
                            }
                            Err(no) => post(&mut board, no.text(city.production.name())),
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specialist_jobs_cycle_and_survive_growth_and_population_loss() {
        let map = test_map();
        let mut city = test_city(&map);
        city.set_size(2);
        let before = city.worked(&map).clone();
        city.cycle_specialist(0);
        assert_eq!(city.specialists(), [Specialist::TaxCollector]);
        assert_eq!(city.worked(&map), before);
        city.cycle_specialist(0);
        assert_eq!(city.specialists(), [Specialist::Scientist]);
        city.cycle_specialist(0);
        assert_eq!(city.specialists(), [Specialist::Entertainer]);
        city.cycle_specialist(0);
        city.cycle_specialist(0);
        city.set_size(3);
        governor_fill(&map, &mut city, &none());
        assert_eq!(city.specialists(), [Specialist::Scientist]);
        assert_eq!(city.worked(&map).len(), 2, "the new citizen works without replacing the scientist");
        city.set_size(1);
        governor_fill(&map, &mut city, &none());
        assert_eq!(city.worked(&map).len() + city.specialists().len(), 1);
        city.cycle_specialist(99);
        assert_eq!(city.specialists(), [Specialist::Scientist]);
        governor_assign(&map, &mut city, &none());
        assert!(city.specialists().is_empty());
        assert_eq!(city.worked(&map).len(), 1);
    }

    #[test]
    fn working_a_tile_returns_a_specialist_to_work() {
        let map = test_map();
        let mut city = test_city(&map);
        let tile = *city.worked(&map).iter().next().unwrap();
        assert!(toggle_worked(&map, &mut city, tile, &none()));
        assert_eq!(city.specialists(), [Specialist::Entertainer]);
        city.cycle_specialist(0);
        assert!(toggle_worked(&map, &mut city, tile, &none()));
        assert!(city.specialists().is_empty());
        assert_eq!(city.worked(&map).len(), 1);
    }

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

    /// Owners after recomputing the borders of `cities` on a copy of `map`.
    fn claimed(map: &GameMap, cities: &[&City]) -> HashMap<(i32, i32), usize> {
        territory(&bordered(map, cities))
    }

    /// Flat grassland with no owners: nothing but culture decides a tile.
    fn land_map() -> GameMap {
        let mut map = test_map();
        for t in &mut map.tiles {
            t.base = Base::Grassland;
            t.owner = None;
        }
        map
    }

    #[test]
    fn border_offsets_match_the_exe_enumerator() {
        // `borders-culture.md` 4.1 lists them in the doubled grid; map step
        // (u, v) is doubled (u - v, u + v).
        let doubled: Vec<(i32, i32)> = border_offsets().iter().map(|&(u, v)| (u - v, u + v)).collect();
        assert_eq!(doubled.len(), 169);
        assert_eq!(&doubled[..9], &[(0, 0), (1, -1), (2, 0), (1, 1), (0, 2), (-1, 1), (-2, 0), (-1, -1), (0, -2)]);
        assert_eq!(
            &doubled[9..25],
            &[(1, -3), (2, -2), (3, -1), (3, 1), (2, 2), (1, 3), (-1, 3), (-2, 2), (-3, 1), (-3, -1), (-2, -2), (-1, -3), (0, -4), (4, 0), (0, 4), (-4, 0)]
        );
        assert_eq!(doubled[121], (1, -11));
        assert_eq!(doubled[168], (-12, 0));
    }

    #[test]
    fn open_ocean_answers_only_to_the_city_radius() {
        // P2: a level-3 city reaches doubled (4, 0) on land but not on Ocean.
        let mut map = land_map();
        let mut city = test_city(&map);
        (city.x, city.y, city.culture) = (40, 30, 100);
        let far = (42, 32); // doubled (0, 4), k = 23
        assert_eq!(claimed(&map, &[&city]).get(&far), Some(&0));
        let i = map.idx(far.0, far.1);
        map.tiles[i].base = Base::Ocean;
        assert_eq!(claimed(&map, &[&city]).get(&far), None);
    }

    #[test]
    fn the_nearer_city_keeps_a_tile_against_more_culture_farther_out() {
        // A tile one step from A and two from B: B's extra culture cannot
        // take it, since B lies outside the bracket A's distance sets.
        let map = land_map();
        let mut a = test_city_of(&map, 0);
        let mut b = test_city_of(&map, 1);
        (a.x, a.y, a.culture) = (40, 30, 10);
        (b.x, b.y, b.culture) = (43, 30, 90);
        let owner = claimed(&map, &[&a, &b]);
        assert_eq!(owner.get(&(41, 30)), Some(&0));
        assert_eq!(owner.get(&(42, 30)), Some(&1));
        // Equidistant (P3 shape): the higher culture takes it, whichever
        // city the offset order meets first.
        b.x = 42;
        let mid = (41, 30);
        assert_eq!(claimed(&map, &[&a, &b]).get(&mid), Some(&1));
        a.culture = 95;
        assert_eq!(claimed(&map, &[&a, &b]).get(&mid), Some(&0));
    }

    #[test]
    fn a_gap_both_neighbours_across_agree_on_is_bridged() {
        // Two level-1 cities of one civ four steps apart leave the column
        // x = 42 unclaimed; each of its tiles has the civ on both sides
        // across x, so orphan resolution fills it (`0x5D4370`).
        let map = land_map();
        let mut a = test_city_of(&map, 0);
        let mut b = test_city_of(&map, 0);
        (a.x, a.y) = (40, 30);
        (b.x, b.y) = (44, 30);
        let owner = claimed(&map, &[&a, &b]);
        for y in 29..=31 {
            assert_eq!(owner.get(&(42, y)), Some(&0), "gap tile (42, {y})");
        }
        // Rows outside the two claims stay open: only one side is owned.
        assert_eq!(owner.get(&(42, 32)), None);
        // Two civs facing each other across the gap settle nothing.
        b.civ = 1;
        assert_eq!(claimed(&map, &[&a, &b]).get(&(42, 30)), None);
    }

    #[test]
    fn a_tile_no_city_reaches_any_more_is_released() {
        let map = land_map();
        let mut a = test_city_of(&map, 0);
        (a.x, a.y, a.culture) = (40, 30, 10);
        let mut m = map.clone();
        recompute_borders(&mut m, &[&a], &[0; CIV_CAP]);
        assert_eq!(territory(&m).len(), 21);
        recompute_borders(&mut m, &[], &[0; CIV_CAP]);
        assert!(territory(&m).is_empty());
    }

    /// Culture that changes without a level change leaves the frontier
    /// alone: the exe recomputes only on founding, loss, capture and level
    /// changes, so two neighbours trading the culture lead every turn do
    /// not trade tiles every turn.
    #[test]
    fn frontier_holds_until_a_level_changes() {
        let mut world = World::new();
        world.insert_resource(land_map());
        world.init_resource::<BorderKey>();
        let mut a = test_city_of(&land_map(), 0);
        let mut b = test_city_of(&land_map(), 1);
        (a.x, a.y, a.culture) = (40, 30, 11);
        (b.x, b.y, b.culture) = (42, 30, 10);
        let ea = world.spawn(a).id();
        let eb = world.spawn(b).id();
        let run = |world: &mut World| {
            use bevy::ecs::system::RunSystemOnce;
            world.run_system_once(update_borders).unwrap();
            territory(world.resource::<GameMap>()).get(&(41, 30)).copied()
        };
        assert_eq!(run(&mut world), Some(0));
        world.get_mut::<City>(eb).unwrap().culture = 50;
        assert_eq!(run(&mut world), Some(0), "same levels: no recompute");
        world.get_mut::<City>(eb).unwrap().culture = 100;
        assert_eq!(run(&mut world), Some(1), "Rome's level rose: recomputed");
        world.get_mut::<City>(ea).unwrap().culture = 99;
        assert_eq!(run(&mut world), Some(1));
    }

    fn test_city(map: &GameMap) -> City {
        test_city_of(map, 0)
    }

    fn test_city_of(map: &GameMap, civ: usize) -> City {
        let (x, y) = map.start;
        let mut city = City {
            gifts: vec![],
            goods: 0,
            coastal: false,
            river: false,
            unrest: 0,
            hurry_timer: 0,
            stakes: Default::default(),
            cooldown: 0,
            unit_clocks: Vec::new(),
            civ,
            name: CIVS[civ].city_names[0].to_string(),
            x,
            y,
            food: 0,
            shields: 0,
            production: crate::roles::guard_production(),
            queue: vec![],
            buildings: vec![],
            diseased: false,
            citizens: crate::citizens::new_pool(civ, 1),
            culture: 0,
            founded: 1,
        };
        // Work under the real rule: a new city's border is its 3x3 block.
        let alone = city.clone();
        let land = bordered(map, &[&alone]);
        governor_assign(map, &mut city, &taken_tiles(&land, [&alone], (x, y)));
        city
    }

    /// `map` with the borders of `cities` drawn on it.
    fn bordered(map: &GameMap, cities: &[&City]) -> GameMap {
        let mut m = map.clone();
        recompute_borders(&mut m, cities, &[0; CIV_CAP]);
        m
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
            river: 0,
            mine: false,
            site: None,
            fortress: false,
            barricade: false,
            forest_harvested: false,
            owner: None,
        };
        let map = GameMap {
            w: 3,
            h: 3,
            tiles: vec![tile; 9],
            start: (1, 1),
            seed: 1,
        };
        let mut city = City {
            gifts: vec![],
            goods: 0,
            coastal: false,
            river: false,
            unrest: 0,
            hurry_timer: 0,
            stakes: Default::default(),
            cooldown: 0,
            unit_clocks: Vec::new(),
            civ: 0,
            name: "Kyoto".to_string(),
            x: 1,
            y: 1,
            food: 0,
            shields: 0,
            production: crate::roles::guard_production(),
            queue: vec![],
            buildings: vec![],
            diseased: false,
            citizens: crate::citizens::new_pool(0, 1),
            culture: 0,
            founded: 1,
        };
        city.work_tile(&map, (1, 0));
        // Center (2,1 with the free shield) plus one grassland tile: net
        // food 2 grows the 20-box in 10, 1 shield builds the Warrior in 10.
        assert_eq!(banner_text(&map, &city, 0), "Kyoto : 10\nWarrior : 10");
        // Another civ sees the bare name over an empty build line.
        assert_eq!(banner_text(&map, &city, 1), "Kyoto\n\u{a0}");
        // No worked tiles: net food 0 stalls growth, shown as `--`.
        city.clear_worked();
        assert_eq!(banner_text(&map, &city, 0), "Kyoto : --\nWarrior : 10");
    }

    #[test]
    fn governor_assigns_best_first() {
        let map = test_map();
        let mut city = test_city(&map);
        city.set_size(3);
        governor_assign(&map, &mut city, &none());
        assert_eq!(city.worked(&map).len(), 3);
        // What the city gets from a tile is the despotism-trimmed yield,
        // and the governor takes food first, then shields.
        let got_by = |tile: &(i32, i32)| {
            let (f, s, _) = citycalc::tile_yields(&map, &city, tile.0, tile.1);
            (f, s)
        };
        let mut best: Vec<(i32, i32)> = radius_tiles(&map, city.x, city.y)
            .iter()
            .map(got_by)
            .collect();
        best.sort_by_key(|&(f, s)| (std::cmp::Reverse(f), std::cmp::Reverse(s)));
        let mut got: Vec<(i32, i32)> = city.worked(&map).iter().map(got_by).collect();
        got.sort_by_key(|&(f, s)| (std::cmp::Reverse(f), std::cmp::Reverse(s)));
        assert_eq!(got, best[..3]);
    }

    #[test]
    fn toggle_add_remove_replace() {
        let map = test_map();
        let mut city = test_city(&map);
        let tiles = radius_tiles(&map, city.x, city.y);
        // remove the assigned one
        let first = *city.worked(&map).iter().next().unwrap();
        toggle_worked(&map, &mut city, first, &none());
        assert!(city.worked(&map).is_empty());
        // add two with size 1: second replaces first
        toggle_worked(&map, &mut city, tiles[0], &none());
        toggle_worked(&map, &mut city, tiles[1], &none());
        assert_eq!(city.worked(&map).len(), 1);
        assert!(city.worked(&map).contains(&tiles[1]));
    }

    #[test]
    fn growth_and_starvation() {
        let map = test_map();
        let mut city = test_city(&map);
        city.food = food_box(city.size()) - 1;
        // force growth regardless of terrain: rig by size-1978 trick is
        // overkill; instead directly verify the thresholds with income
        let (net, _) = city_income(&map, &city);
        let events = process_city_turn(&map, &mut city, &none(), &mut crate::rng::MapRng::new(1));
        if net >= 1 {
            assert!(events.iter().any(|e| matches!(e, CityEvent::Grew)));
            assert_eq!(city.size(), 2);
            assert_eq!(city.food, 0);
        }
        // starvation: size 3 city that cannot feed itself
        city.set_size(3);
        city.food = 0;
        city.clear_worked();
        let (net, _) = city_income(&map, &city);
        assert!(net < 0, "bare size-3 city must starve");
        let events = process_city_turn(&map, &mut city, &none(), &mut crate::rng::MapRng::new(1));
        assert!(events.iter().any(|e| matches!(e, CityEvent::Starved)));
        assert_eq!(city.size(), 2);
    }

    #[test]
    fn settler_population_cost_uses_native_food_retention_across_size_classes() {
        crate::realm::reset();
        crate::realm::write(0, |r| r.born_content = 7);
        let map = flat_land();
        for granary in [false, true] {
            let mut city = City::new(0, "Test", 10, 10);
            city.set_size(7);
            city.food = 30;
            city.buildings.push(Production::named("Aqueduct"));
            if granary { city.buildings.push(Production::named("Granary")); }
            city.production = Production::named("Settler");
            city.shields = city.price(city.production);
            governor_fill(&map, &mut city, &none());
            let events = process_city_turn(&map, &mut city, &none(), &mut crate::rng::MapRng::new(1));
            assert!(events.iter().any(|e| matches!(e, CityEvent::Completed(t, _) if *t == UnitType::named("Settler"))));
            assert_eq!(city.size(), 5);
            assert_eq!(city.food, if granary { 10 } else { 0 });
        }
    }

    #[test]
    fn a_full_town_without_an_aqueduct_waits_and_says_so_once() {
        crate::realm::reset();
        let map = flat_land();
        let mut city = test_city(&map);
        city.set_size(6);
        governor_fill(&map, &mut city, &none());
        city.food = food_box(6) - 1;
        let events = process_city_turn(&map, &mut city, &none(), &mut crate::rng::MapRng::new(1));
        assert!(events.iter().any(|e| matches!(e, CityEvent::Blocked)));
        assert_eq!((city.size(), city.food), (6, food_box(6)));
        // Announced once: the box stays full and the city stays small.
        let events = process_city_turn(&map, &mut city, &none(), &mut crate::rng::MapRng::new(1));
        assert!(!events.iter().any(|e| matches!(e, CityEvent::Blocked | CityEvent::Grew)));
        assert_eq!(city.size(), 6);
        // The Aqueduct lets it grow on the next turn.
        city.buildings.push(Production::named("Aqueduct"));
        let events = process_city_turn(&map, &mut city, &none(), &mut crate::rng::MapRng::new(1));
        assert!(events.iter().any(|e| matches!(e, CityEvent::Grew)));
        assert_eq!(city.size(), 7);
    }

    #[test]
    fn a_lakeside_town_grows_without_an_aqueduct_but_still_needs_a_hospital() {
        crate::realm::reset();
        let mut map = flat_land();
        let mut city = test_city(&map);
        let lake = map.idx(map.wrap_x(city.x - 1), city.y);
        map.tiles[lake].base = Base::Coast;
        city.set_size(6);
        governor_fill(&map, &mut city, &none());
        city.food = food_box(6);
        let events = process_city_turn(&map, &mut city, &none(), &mut crate::rng::MapRng::new(1));
        assert!(events.iter().any(|e| matches!(e, CityEvent::Grew)));
        assert!(!events.iter().any(|e| matches!(e, CityEvent::Blocked)));
        assert_eq!(city.size(), 7);
        assert!(!city.has(Production::named("Aqueduct")));
        city.set_size(12);
        governor_fill(&map, &mut city, &none());
        city.food = food_box(12) - 1;
        let events = process_city_turn(&map, &mut city, &none(), &mut crate::rng::MapRng::new(1));
        assert!(events.iter().any(|e| matches!(e, CityEvent::Blocked)));
        assert_eq!(city.size(), 12);
        city.buildings.push(Production::named("Hospital"));
        let events = process_city_turn(&map, &mut city, &none(), &mut crate::rng::MapRng::new(1));
        assert!(events.iter().any(|e| matches!(e, CityEvent::Grew)));
        assert_eq!(city.size(), 13);
    }

    #[test]
    fn a_city_of_unhappy_citizens_riots_and_calms_when_they_are_born_content() {
        crate::realm::reset();
        let map = flat_land();
        let mut city = test_city(&map);
        city.set_size(4);
        governor_fill(&map, &mut city, &none());
        city.shields = 5;
        crate::realm::write(0, |r| r.born_content = 0);
        let events = process_city_turn(&map, &mut city, &none(), &mut crate::rng::MapRng::new(1));
        assert!(events.iter().any(|e| matches!(e, CityEvent::Disorder)));
        assert_eq!(city.shields, 5, "a city in disorder builds nothing");
        assert_eq!(city.unrest, 1);
        crate::realm::write(0, |r| r.born_content = 4);
        let events = process_city_turn(&map, &mut city, &none(), &mut crate::rng::MapRng::new(1));
        assert!(events.iter().any(|e| matches!(e, CityEvent::Calm)));
        assert_eq!(city.unrest, 0);
    }

    #[test]
    fn production_discards_surplus_before_advancing_the_queue() {
        crate::realm::reset();
        crate::realm::write(0, |r| r.born_content = 7);
        let mut map = flat_land();
        let (x, y) = map.start;
        let worked = (map.wrap_x(x + 1), y);
        let tile = map.idx(worked.0, worked.1);
        map.tiles[tile].base = Base::Plains;
        map.tiles[tile].mine = true;
        crate::realm::write(0, |r| r.capital = Some((x, y)));
        for done in [Production::named("Warrior"), Production::named("Temple")] {
            let mut city = test_city(&map);
            city.set_worked(&map, HashSet::from([worked]));
            city.production = done;
            city.queue = vec![Production::named("Warrior")];
            let cost = city.price(done);
            city.shields = cost - 1;
            let (_, income) = city_income(&map, &city);
            assert!(income > 1, "fixture must overshoot the price");
            let events = process_city_turn(&map, &mut city, &none(), &mut crate::rng::MapRng::new(1));
            assert_eq!(events.iter().filter(|e| matches!(e, CityEvent::Completed(..) | CityEvent::Built(_))).count(), 1);
            assert_eq!(city.shields, 0);
            assert_eq!(city.production, Production::named("Warrior"));
            assert!(city.queue.is_empty());
            let events = process_city_turn(&map, &mut city, &none(), &mut crate::rng::MapRng::new(1));
            assert!(!events.iter().any(|e| matches!(e, CityEvent::Completed(..))));
            assert_eq!(city.shields, income as u16);
        }
    }

    #[test]
    fn completed_population_unit_offers_abandonment_only_to_nongrowing_humans() {
        crate::civs::set_controllers();
        crate::realm::reset();
        let mut map = flat_land();
        for t in &mut map.tiles { t.base = Base::Plains; }
        for (civ, grow) in [(0, false), (0, true), (1, false)] {
            let mut city = City::new(civ, "Town", 5, 5);
            city.production = Production::named("Worker");
            city.shields = city.price(city.production);
            city.food = 10;
            if grow { governor_fill(&map, &mut city, &none()); }
            let mut rng = crate::rng::MapRng::new(1);
            let events = process_city_turn(&map, &mut city, &none(), &mut rng);
            assert_eq!(events.iter().any(|e| matches!(e, CityEvent::Abandon)), civ == 0 && !grow);
            assert!(!events.iter().any(|e| matches!(e, CityEvent::Completed(..))));
            assert_eq!(city.size(), 1);
            assert_eq!(city.shields, city.price(Production::named("Worker")));
            assert_eq!(rng.state(), 1);
        }
    }

    #[test]
    fn forbidden_palace_needs_half_the_optimal_cities_and_is_built_once() {
        crate::realm::reset();
        let map = flat_land();
        let city = test_city(&map);
        let fp = Production::named("Forbidden Palace");
        let row = fp.building_row().unwrap();
        crate::realm::write(city.civ, |r| r.cities = 9);
        assert!(!city.can_build_here(fp), "9 of 20 optimal cities");
        crate::realm::write(city.civ, |r| r.cities = 10);
        assert!(city.can_build_here(fp));
        crate::realm::write(city.civ, |r| r.owned[row] = 1);
        assert!(!city.can_build_here(fp), "one per civilization");
    }

    #[test]
    fn a_wonder_finished_elsewhere_switches_to_the_most_expensive_item() {
        crate::realm::reset();
        let map = flat_land();
        let mut city = test_city(&map);
        city.production = Production::named("The Pyramids");
        city.shields = 30;
        crate::research::set_wonder_built(Production::named("The Pyramids"), true);
        let events = process_city_turn(&map, &mut city, &none(), &mut crate::rng::MapRng::new(1));
        crate::research::set_wonder_built(Production::named("The Pyramids"), false);
        let next = city.production;
        assert_ne!(next, Production::named("The Pyramids"));
        assert!(events.contains(&CityEvent::WonderLost(Production::named("The Pyramids"), next)));
        assert!(!city.has(Production::named("The Pyramids")));
        assert!(city.shields >= 30 && city.shields <= city.price(next), "the box, with this turn's shields, is kept up to the new cost");
        assert!(city.buildable().iter().all(|&p| p.is_building() && (p.bldg().unwrap().flags & roster::imp::CENTER_OF_EMPIRE != 0 || p.bldg().unwrap().small & 0x20 != 0) || city.price(p) <= city.price(next)));
    }

    #[test]
    fn wealth_clears_stored_shields_and_advances_without_building_the_next_item() {
        crate::realm::reset();
        let map = flat_land();
        let mut city = test_city(&map);
        city.production = Production::named("Wealth");
        city.shields = 30;
        let events = process_city_turn(&map, &mut city, &none(), &mut crate::rng::MapRng::new(1));
        assert!(events.is_empty());
        assert_eq!(city.production, Production::named("Wealth"));
        assert_eq!(city.shields, 0);
        city.queue = vec![Production::named("Warrior")];
        city.shields = 30;
        let events = process_city_turn(&map, &mut city, &none(), &mut crate::rng::MapRng::new(1));
        assert!(events.is_empty());
        assert_eq!(city.production, Production::named("Warrior"));
        assert_eq!(city.shields, 0);
        assert!(city.queue.is_empty());
    }

    #[test]
    fn production_completes_and_settler_costs_pop() {
        let map = test_map();
        let mut city = test_city(&map);
        city.production = Production::named("Warrior");
        city.shields = 9;
        let _ = process_city_turn(&map, &mut city, &none(), &mut crate::rng::MapRng::new(1));
        // warrior may or may not complete depending on shields income;
        // force it:
        city.shields = city.production.cost();
        let events = process_city_turn(&map, &mut city, &none(), &mut crate::rng::MapRng::new(1));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, CityEvent::Completed(t, _) if *t == UnitType::named("Warrior")))
        );
        assert_eq!(city.shields, 0, "completion discards surplus shields");
        // settler costs 2 pop
        city.production = Production::named("Settler");
        city.set_size(4);
        governor_fill(&map, &mut city, &none());
        city.shields = city.production.cost();
        let _ = process_city_turn(&map, &mut city, &none(), &mut crate::rng::MapRng::new(1));
        assert_eq!(city.size(), 2);
        assert!(city.worked(&map).len() <= 2);
        // below size 3 the finished settler waits, announced once
        city.set_size(2);
        city.food = 0;
        city.shields = city.production.cost() - 1;
        let events = process_city_turn(&map, &mut city, &none(), &mut crate::rng::MapRng::new(1));
        assert!(events.iter().any(|e| matches!(e, CityEvent::TooSmall)));
        assert_eq!(city.size(), 2);
        assert_eq!(city.shields, Production::named("Settler").cost());
        let events = process_city_turn(&map, &mut city, &none(), &mut crate::rng::MapRng::new(1));
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, CityEvent::TooSmall | CityEvent::Completed(_, _)))
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
    fn radius_cell_center_and_edges() {
        assert_eq!(radius_cell(Vec2::ZERO), Some((0, 0)));
        // One tile east of the city is (x+1, y): right and down.
        assert_eq!(radius_cell(Vec2::new(64.0, -32.0)), Some((1, 0)));
        assert_eq!(radius_cell(tile_to_world(-2, 1)), Some((-2, 1)));
        // The fat cross has no corners, and nothing past two tiles.
        assert_eq!(radius_cell(tile_to_world(2, 2)), None);
        assert_eq!(radius_cell(tile_to_world(3, 0)), None);
    }

    #[test]
    fn building_completes_queue_advances_and_granary_keeps_food() {
        let map = test_map();
        let mut city = test_city(&map);
        city.production = Production::named("Granary");
        city.queue = vec![Production::named("Temple"), Production::named("Worker")];
        city.shields = Production::named("Granary").cost();
        let events = process_city_turn(&map, &mut city, &none(), &mut crate::rng::MapRng::new(1));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, CityEvent::Built(t) if *t == Production::named("Granary")))
        );
        assert!(city.has(Production::named("Granary")));
        assert_eq!(city.production, Production::named("Temple"));
        assert_eq!(city.queue, vec![Production::named("Worker")]);
        assert!(!city.buildable().contains(&Production::named("Granary")));
        // granary keeps half the food box on growth
        let boxed = food_box(city.size());
        city.food = boxed - 1;
        let (net, _) = city_income(&map, &city);
        if net >= 1 {
            process_city_turn(&map, &mut city, &none(), &mut crate::rng::MapRng::new(1));
            assert_eq!(city.food, boxed / 2);
        }
    }

    #[test]
    fn change_build_keeps_shields_up_to_the_new_cost_and_queue_rules() {
        let map = test_map();
        let mut city = test_city(&map);
        city.production = Production::named("Warrior");
        city.shields = 8;
        city.change_build(Production::named("Worker"));
        assert_eq!(city.shields, 8);
        city.change_build(Production::named("Barracks"));
        assert_eq!(city.shields, 8);
        city.shields = 30;
        city.change_build(Production::named("Warrior"));
        assert_eq!(city.shields, city.price(Production::named("Warrior")));
        city.change_build(Production::named("Barracks"));
        assert!(city.enqueue(Production::named("Temple")));
        assert!(!city.enqueue(Production::named("Temple")));
        assert!(!city.enqueue(Production::named("Barracks")));
        assert!(city.enqueue(Production::named("Warrior")));
        assert!(city.enqueue(Production::named("Warrior")));
        city.dequeue(0);
        assert_eq!(city.queue[0], Production::named("Warrior"));
        // building with empty queue falls back to a Warrior
        city.queue.clear();
        city.production = Production::named("Temple");
        city.shields = Production::named("Temple").cost();
        process_city_turn(&map, &mut city, &none(), &mut crate::rng::MapRng::new(1));
        assert_eq!(city.production, Production::named("Warrior"));
    }

    #[test]
    fn other_cities_tiles_and_unexplored_tiles_are_not_workable() {
        let mut map = test_map();
        let a = test_city(&map);
        let tile = *a.worked(&map).iter().next().unwrap();
        let mut b = test_city(&map);
        b.name = "B".into();
        b.x = map.wrap_x(a.x + 2);
        let taken = taken_tiles(&map, [&a], (b.x, b.y));
        assert!(taken.contains(&(a.x, a.y)), "city centers are taken");
        assert!(taken.contains(&tile), "a's worked tile is taken for b");
        b.clear_worked();
        assert!(!toggle_worked(&map, &mut b, tile, &taken));
        assert!(!b.worked(&map).contains(&tile));
        governor_assign(&map, &mut b, &taken);
        assert!(b.worked(&map).is_disjoint(&a.worked(&map)));
        // a city never counts its own tiles as taken
        assert!(!taken_tiles(&bordered(&map, &[&a]), [&a], (a.x, a.y)).contains(&tile));
        // unexplored tiles cannot be worked
        let (ux, uy) = radius_tiles(&map, b.x, b.y)
            .into_iter()
            .find(|t| !taken.contains(t))
            .unwrap();
        let i = map.idx(ux, uy);
        map.tiles[i].seen = false;
        b.clear_worked();
        assert!(!toggle_worked(&map, &mut b, (ux, uy), &taken));
    }

    #[test]
    fn a_city_works_only_inside_its_own_border() {
        // Civ3 lights only the radius tiles inside the civ's border: a new
        // city's border is its 3x3 block, so the radius's outer ring is
        // unclaimed and cannot be worked until the border grows.
        let map = test_map();
        let mut city = test_city(&map);
        let near = (map.wrap_x(city.x + 1), city.y);
        let far = (map.wrap_x(city.x + 2), city.y);
        let taken = taken_tiles(&bordered(&map, &[&city]), [&city], (city.x, city.y));
        assert!(!taken.contains(&near));
        assert!(taken.contains(&far), "unclaimed land is not workable");
        city.culture = 10;
        let taken = taken_tiles(&bordered(&map, &[&city]), [&city], (city.x, city.y));
        assert!(!taken.contains(&far), "the grown border takes in the ring");
        // A rival's border closes the tile again.
        let mut rival = test_city_of(&map, 1);
        rival.x = map.wrap_x(city.x + 3);
        rival.culture = 100;
        let taken = taken_tiles(&bordered(&map, &[&city, &rival]), [&city, &rival], (city.x, city.y));
        assert!(taken.contains(&far), "a foreign border is not workable");
    }

    #[test]
    fn growth_keeps_manual_picks() {
        let map = test_map();
        let mut city = test_city(&map);
        // pick the worst tile by hand, then grow
        let worst = radius_tiles(&map, city.x, city.y)
            .into_iter()
            .min_by_key(|t| tile_rank(&map, &city, t))
            .unwrap();
        city.clear_worked();
        assert!(toggle_worked(&map, &mut city, worst, &none()));
        city.set_size(2);
        governor_fill(&map, &mut city, &none());
        assert_eq!(city.worked(&map).len(), 2);
        assert!(city.worked(&map).contains(&worst), "manual pick survives growth");
        // Slot zero holds the manual pick: a native seed-zero loss releases it.
        city.lose_population(1, None, &mut crate::rng::MapRng::new(0));
        governor_fill(&map, &mut city, &none());
        assert!(!city.worked(&map).contains(&worst));
    }

    #[test]
    fn worker_completion_pays_a_citizen_and_keeps_foreign_nationality() {
        crate::realm::reset();
        let map = test_map();
        let mut city = test_city(&map);
        city.set_size(3);
        city.set_nationality(1);
        governor_assign(&map, &mut city, &none());
        city.production = Production::named("Worker");
        city.shields = city.price(Production::named("Worker"));
        let mut rng = crate::rng::MapRng::new(1);
        let events = process_city_turn(&map, &mut city, &none(), &mut rng);
        assert!(events.iter().any(|e| matches!(e,
            CityEvent::Completed(t, race) if *t == UnitType::named("Worker") && *race == crate::civs::roster_index(1))));
        assert_eq!(city.size(), 2);
        assert_eq!(city.nationals(1), 2);
        assert_eq!(rng.state(), 2_524_885_223, "owner search fails, foreign search succeeds");
    }

    #[test]
    fn the_city_center_follows_the_exe_whatever_the_terrain() {
        // `yields.md` 4.1 to 4.3: two food whatever the terrain (no free
        // irrigation), the size class adds shields and commerce, and the
        // center never gives less than one shield and one commerce.
        let mut map = test_map();
        let mut city = test_city(&map);
        city.clear_worked();
        let i = map.idx(city.x, city.y);
        for (base, relief) in [
            (Base::Grassland, Relief::Flat),
            (Base::Desert, Relief::Flat),
            (Base::Plains, Relief::Hill),
        ] {
            map.tiles[i].base = base;
            map.tiles[i].relief = relief;
            map.tiles[i].cover = Cover::Bare;
            map.tiles[i].resource = None;
            map.tiles[i].irrigation = false;
            map.tiles[i].mine = false;
            map.tiles[i].road = false;
            let (f, s, c) = citycalc::center(&map, &city);
            assert_eq!(f, 2, "{base:?} {relief:?}");
            assert!(s >= 1 && c >= 1, "{base:?} {relief:?}: {s} shields {c} commerce");
        }
        // The class adds shields and commerce: a Grassland center has none
        // of its own, so a town and a city both sit on the minimum of one.
        map.tiles[i].base = Base::Grassland;
        map.tiles[i].relief = Relief::Flat;
        let at = |size: u8| {
            let mut c = city.clone();
            c.set_size(size);
            citycalc::center(&map, &c)
        };
        assert_eq!(at(1), (2, 1, 1));
        assert_eq!(at(7), (2, 1, 1));
        assert_eq!(at(13), (2, 2, 2));
    }

    #[test]
    fn city_screen_shows_improvements_in_yields() {
        let mut map = test_map();
        let mut city = test_city(&map);
        let tile = *city.worked(&map).iter().next().unwrap();
        let (f0, s0) = city_yields(&map, &city);
        let i = map.idx(tile.0, tile.1);
        map.tiles[i].mine = true;
        let (f1, s1) = city_yields(&map, &city);
        assert_eq!(f1, f0);
        assert!(s1 > s0);
        city.clear_worked();
        let (f, s, _) = citycalc::center(&map, &city);
        assert_eq!(city_yields(&map, &city), (f as u8, s as u8));
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
            river: 0,
            mine: false,
            site: None,
            fortress: false,
            barricade: false,
            forest_harvested: false,
            owner: None,
        };
        assert_eq!(tile_commerce(&bare()), 0);
        let mut road = bare();
        road.road = true;
        assert_eq!(tile_commerce(&road), 1);
        let mut coast = bare();
        coast.base = Base::Coast;
        assert_eq!(tile_commerce(&coast), 2);
        coast.road = true;
        assert_eq!(tile_commerce(&coast), 2, "water has no road bonus");
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
    fn shield_grid_never_outgrows_seven_by_five() {
        // One icon a shield while the price fits; past 35 each icon
        // stands for several, so a dear build never runs off the panel.
        assert_eq!(shield_grid(10), (10, 1));
        assert_eq!(shield_grid(35), (35, 1));
        assert_eq!(shield_grid(60), (30, 2));
        for price in [1, 30, 36, 120, 300, 600] {
            let (icons, per) = shield_grid(price);
            assert!(icons <= SHIELD_COLS * SHIELD_ROWS, "price {price}");
            assert!(icons * per >= price);
        }
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
                claimed(&map, &[&city])
                    .into_keys()
                    .collect::<HashSet<_>>(),
                expected
            );
        }
        city.culture = 10;
        let expanded = claimed(&map, &[&city]);
        assert!(expected.iter().all(|tile| expanded.contains_key(tile)));
        assert!(expanded.contains_key(&(40, 32)));
        assert!(expanded.contains_key(&(42, 31)));
        assert!(!expanded.contains_key(&(42, 32)), "the radius' corners stay out");
        assert!(!expanded.contains_key(&(40, 33)));
    }

    #[test]
    fn borders_grow_like_the_shipped_saves_show() {
        // Tiles claimed at each level: the 3x3 square, the 21-tile city
        // radius, then 37, 61, 89 and 137 (`borders-culture.md` B3). Land
        // everywhere, so open ocean's short reach does not cut any off.
        let mut map = test_map();
        for t in &mut map.tiles {
            t.base = Base::Grassland;
        }
        let mut city = test_city(&map);
        city.x = 40;
        city.y = 30;
        for (culture, tiles) in [(0, 9), (10, 21), (100, 37), (1000, 61), (10_000, 89), (100_000, 137)] {
            city.culture = culture;
            assert_eq!(claimed(&map, &[&city]).len(), tiles, "culture {culture}");
        }
        // Level 3 reaches (3, 1) but not (3, 2).
        city.culture = 100;
        let owned = claimed(&map, &[&city]);
        assert!(owned.contains_key(&(43, 31)));
        assert!(!owned.contains_key(&(43, 32)));
    }

    #[test]
    fn culture_per_turn_counts_the_palace_and_temples() {
        let map = test_map();
        let mut city = test_city(&map);
        assert_eq!(culture_per_turn(&city, false), 0);
        assert_eq!(culture_per_turn(&city, true), 1);
        city.buildings.push(Production::named("Temple"));
        assert_eq!(culture_per_turn(&city, false), 2);
        assert_eq!(culture_per_turn(&city, true), 3);
        assert_eq!(Production::named("Temple").upkeep(), 1);
        assert_eq!(Production::named("Granary").upkeep(), 1);
        assert_eq!(Production::named("Warrior").upkeep(), 0);
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
        assert_eq!(next_name(&mut used, 3), CIVS[3].city_names[0]);
        assert_eq!(used.0[..4], [2, 1, 1, 1], "each civ counts its own foundings");
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
        japan.set_worked(&map, HashSet::from([(jx, jy)]));
        rome.set_worked(&map, HashSet::from([(rx, ry)]));
        let japan_tax = accrue(&map, &mut japan, true);
        let rome_tax = accrue(&map, &mut rome, true);
        assert_eq!(japan.culture, 1, "each civ's own capital earns the palace");
        assert_eq!(rome.culture, 1);
        assert_eq!(city_commerce(&map, &rome), city_commerce(&map, &japan) + 1);
        assert_eq!(japan_tax, 0, "one commerce pays no tax yet");
        assert_eq!(rome_tax, 1, "the road's commerce pays Rome's tax");
        let mut third = test_city_of(&map, 2);
        third.clear_worked();
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
        rome.clear_worked();
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
        recompute_borders(&mut map, &cities, &[0; CIV_CAP]);
        assert_eq!(
            territory(&map).get(&frontier).copied(),
            Some(1),
            "Rome holds the tile; Japan's border does not reach it"
        );
        assert_eq!(resources_owned(&map, 0).0, vec![]);
        assert_eq!(resources_owned(&map, 1).0, vec![(id, 1)]);
    }

    /// Explored, flat grassland everywhere: a worked tile feeds two and
    /// gives nothing else, so a city nets the same two food at any size
    /// (its center gives two and a free shield).
    fn flat_land() -> GameMap {
        let mut map = test_map();
        for t in map.tiles.iter_mut() {
            t.base = Base::Grassland;
            t.relief = Relief::Flat;
            t.cover = Cover::Bare;
            t.river = 0;
            t.resource = None;
            t.road = false;
            t.irrigation = false;
            t.mine = false;
        }
        map
    }

    /// A city of the size, its citizens put to work, on `flat_land`.
    fn sized_city(map: &GameMap, size: u8) -> City {
        let mut city = test_city(map);
        city.set_size(size);
        // Towns past six need an Aqueduct, cities past twelve a Hospital.
        if size >= 6 {
            city.buildings.push(Production::named("Aqueduct"));
        }
        if size >= 12 {
            city.buildings.push(Production::named("Hospital"));
        }
        governor_fill(map, &mut city, &none());
        assert_eq!(city_income(map, &city).0, 2, "flat land nets two at {size}");
        city
    }

    #[test]
    fn a_city_grows_when_the_box_of_its_size_class_fills() {
        let map = flat_land();
        // Towns fill 20 food, cities 40, metropolises 60.
        for (size, boxed) in [(1, 20), (6, 20), (7, 40), (12, 40), (13, 60), (16, 60)] {
            let turn = |food: u8| {
                let mut city = sized_city(&map, size);
                city.food = food;
                let events = process_city_turn(&map, &mut city, &none(), &mut crate::rng::MapRng::new(1));
                (city, events.iter().any(|e| matches!(e, CityEvent::Grew)))
            };
            let (city, grew) = turn(boxed - 3);
            assert!(!grew, "size {size} grew a turn early");
            assert_eq!((city.size(), city.food), (size, boxed - 1));
            let (city, grew) = turn(boxed - 2);
            assert!(grew, "size {size} stayed put with a full box");
            assert_eq!((city.size(), city.food), (size + 1, 0));
            // Without a Granary what overflows the box is lost.
            let (city, _) = turn(boxed - 1);
            assert_eq!((city.size(), city.food), (size + 1, 0));
        }
    }

    #[test]
    fn a_granary_keeps_half_of_the_box_that_filled() {
        let map = flat_land();
        // Growing from 6 to 7 keeps half of the town's box, not the city's.
        for (size, kept) in [(3, 10), (6, 10), (7, 20), (12, 20), (13, 30)] {
            let mut city = sized_city(&map, size);
            city.buildings.push(Production::named("Granary"));
            city.food = food_box(size) - 2;
            process_city_turn(&map, &mut city, &none(), &mut crate::rng::MapRng::new(1));
            assert_eq!(city.size(), size + 1);
            assert_eq!(city.food, kept, "size {size}");
        }
    }

    #[test]
    fn a_settler_leaves_a_smaller_city_with_a_smaller_box() {
        let map = flat_land();
        let mut city = sized_city(&map, 13);
        city.food = 50;
        city.production = Production::named("Settler");
        city.shields = Production::named("Settler").cost();
        let events = process_city_turn(&map, &mut city, &none(), &mut crate::rng::MapRng::new(1));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, CityEvent::Completed(t, _) if *t == UnitType::named("Settler")))
        );
        assert_eq!(city.size(), 11);
        // Crossing metropolis -> city loses the store without a Granary.
        assert_eq!(city.food, 0);
    }

    #[test]
    fn a_city_starves_only_once_its_box_runs_dry() {
        let map = flat_land();
        let mut city = test_city(&map);
        city.set_size(3);
        // Idle citizens: the center's two food against six eaten.
        city.clear_worked();
        city.food = 5;
        assert!(process_city_turn(&map, &mut city, &none(), &mut crate::rng::MapRng::new(1)).is_empty());
        assert_eq!((city.size(), city.food), (3, 1));
        let events = process_city_turn(&map, &mut city, &none(), &mut crate::rng::MapRng::new(1));
        assert!(events.iter().any(|e| matches!(e, CityEvent::Starved)));
        assert_eq!((city.size(), city.food), (2, 0));
    }

    #[test]
    fn worked_tiles_pay_despotism_and_the_center_is_fixed_at_two() {
        let mut map = flat_land();
        let (x, y) = map.start;
        let near = (map.wrap_x(x + 1), y);
        for spot in [(x, y), near] {
            let i = map.idx(spot.0, spot.1);
            map.tiles[i].irrigation = true;
        }
        let tile = map.tiles[map.idx(near.0, near.1)].clone();
        assert_eq!(yields(&tile).0, 3);
        assert_eq!(citycalc::center(&map, &test_city(&map)).0, 2, "the center is fixed at two");
        let city = test_city(&map);
        let worked = |t: &Tile| {
            let mut m = map.clone();
            let i = m.idx(near.0, near.1);
            m.tiles[i] = t.clone();
            citycalc::tile_yields(&m, &city, near.0, near.1)
        };
        assert_eq!(worked(&tile).0, 2, "a worked tile loses one");
        // A forced road on coast has no bonus; worked coast gives two commerce.
        let mut coast = tile.clone();
        coast.base = Base::Coast;
        coast.irrigation = false;
        coast.road = true;
        assert_eq!(tile_commerce(&coast), 2, "water has no road bonus");
        assert_eq!(worked(&coast).2, 2);
        // Small yields pass untouched.
        assert_eq!(citycalc::tile_yields(&map, &city, x, y + 1), (2, 0, 0));
        // The city sums the center's two and the trimmed worked tile.
        let mut city = test_city(&map);
        city.set_worked(&map, HashSet::from([near]));
        assert_eq!(city_yields(&map, &city).0, 2 + 2);
    }

    /// Civ 0's city at the start and civ 1's city two tiles east: each
    /// owns a 3x3 border, and each has in its radius a tile only the other
    /// one's border covers.
    fn neighbors(map: &GameMap) -> (City, City, (i32, i32), (i32, i32)) {
        let (sx, sy) = map.start;
        let mut mine = test_city_of(map, 0);
        let mut theirs = test_city_of(map, 1);
        theirs.x = map.wrap_x(sx + 2);
        mine.clear_worked();
        theirs.clear_worked();
        let theirs_only = (map.wrap_x(sx + 2), sy + 1);
        let mine_only = (sx, sy + 1);
        (mine, theirs, theirs_only, mine_only)
    }

    #[test]
    fn a_foreign_border_keeps_a_tile_out_of_reach() {
        let map = flat_land();
        let (mut mine, mut theirs, theirs_only, mine_only) = neighbors(&map);
        let map = bordered(&map, &[&mine, &theirs]);
        let taken = taken_tiles(&map, [&mine, &theirs], (mine.x, mine.y));
        assert!(taken.contains(&theirs_only));
        assert!(!workable(&map, &taken, theirs_only));
        assert!(!toggle_worked(&map, &mut mine, theirs_only, &taken));
        // A fellow citizen's border is no obstacle.
        theirs.civ = 0;
        let ours = bordered(&map, &[&mine, &theirs]);
        let taken = taken_tiles(&ours, [&mine, &theirs], (mine.x, mine.y));
        assert!(workable(&ours, &taken, theirs_only));
        theirs.civ = 1;
        // The same ground is open to the civ that owns it, and the tile
        // only my border covers is closed to it.
        let taken = taken_tiles(&map, [&mine, &theirs], (theirs.x, theirs.y));
        assert!(workable(&map, &taken, theirs_only));
        assert!(!workable(&map, &taken, mine_only));
        assert!(toggle_worked(&map, &mut theirs, theirs_only, &taken));
    }

    #[test]
    fn a_border_that_grows_over_a_worked_tile_takes_it_back() {
        let map = flat_land();
        let (mut mine, theirs, theirs_only, _) = neighbors(&map);
        let mut map = bordered(&map, &[&mine, &theirs]);
        mine.set_worked(&map, HashSet::from([theirs_only]));
        let taken = taken_tiles(&map, [&mine, &theirs], (mine.x, mine.y));
        assert!(prune_worked(&map, &mut mine, &taken));
        assert!(mine.worked(&map).is_empty());
        assert!(!prune_worked(&map, &mut mine, &taken), "nothing left to drop");
        // Fog is not rechecked: `seen` describes the civ in play, and the
        // pick was explored when it was made.
        let own = (mine.x, mine.y + 1);
        let i = map.idx(own.0, own.1);
        map.tiles[i].seen = false;
        mine.set_worked(&map, HashSet::from([own]));
        let taken = taken_tiles(&map, [&mine, &theirs], (mine.x, mine.y));
        assert!(!prune_worked(&map, &mut mine, &taken));
        assert!(mine.worked(&map).contains(&own));
    }

    #[test]
    fn reconcile_mends_the_active_civs_cities_and_the_rest_in_their_turn() {
        let map = flat_land();
        let (mut mine, mut theirs, theirs_only, mine_only) = neighbors(&map);
        mine.set_worked(&map, HashSet::from([theirs_only]));
        theirs.set_worked(&map, HashSet::from([mine_only]));
        let mut app = App::new();
        app.insert_resource(map);
        app.init_resource::<Civilizations>();
        app.init_resource::<BorderKey>();
        app.add_systems(Update, (update_borders, reconcile_tiles).chain());
        let mine = app.world_mut().spawn(mine).id();
        let theirs = app.world_mut().spawn(theirs).id();
        app.update();
        let worked = |app: &App, e| app.world().get::<City>(e).unwrap().worked(app.world().resource::<GameMap>());
        let fixed = worked(&app, mine);
        assert_eq!(fixed.len(), 1, "the governor found the citizen work");
        assert!(!fixed.contains(&theirs_only));
        assert_eq!(
            worked(&app, theirs),
            HashSet::from([mine_only]),
            "civ 1 waits for its own turn"
        );
        app.world_mut().resource_mut::<Civilizations>().active = 1;
        app.update();
        let fixed = worked(&app, theirs);
        assert_eq!(fixed.len(), 1);
        assert!(!fixed.contains(&mine_only));
        assert_eq!(worked(&app, mine).len(), 1);
    }

    #[test]
    fn a_city_founded_mid_turn_takes_its_site_from_a_neighbor() {
        let map = flat_land();
        let mine = test_city(&map);
        let site = *mine.worked(&map).iter().next().unwrap();
        let mut app = App::new();
        app.insert_resource(map);
        app.init_resource::<Civilizations>();
        app.init_resource::<BorderKey>();
        app.add_systems(Update, (update_borders, reconcile_tiles).chain());
        let mine = app.world_mut().spawn(mine).id();
        app.update();
        assert!(app.world().get::<City>(mine).unwrap().worked(app.world().resource::<GameMap>()).contains(&site));
        // The same civ founds on the worked tile; no hotseat handoff.
        let mut founded = test_city(app.world().resource::<GameMap>());
        (founded.x, founded.y) = site;
        founded.clear_worked();
        app.world_mut().spawn(founded);
        app.update();
        let worked = app.world().get::<City>(mine).unwrap().worked(app.world().resource::<GameMap>());
        assert_eq!(worked.len(), 1, "the citizen found other work");
        assert!(!worked.contains(&site));
    }

    #[test]
    fn the_food_grid_scales_with_the_box_and_is_full_only_when_the_box_is() {
        // A town's box is the grid: one cell a food.
        let cells: Vec<usize> = (0..=20).map(|food| box_cells(food, 20)).collect();
        assert_eq!(cells, (0..=20).collect::<Vec<_>>());
        // A city's cell holds two, a metropolis's three.
        assert_eq!(box_cells(20, 40), 10);
        assert_eq!(box_cells(30, 60), 10);
        assert_eq!(box_cells(0, 60), 0);
        // One short of growing never reads as ready.
        assert_eq!(box_cells(39, 40), FOOD_BOX_CELLS - 1);
        assert_eq!(box_cells(59, 60), FOOD_BOX_CELLS - 1);
        assert_eq!(box_cells(40, 40), FOOD_BOX_CELLS);
    }

    /// A world for `end_turn_cities`: what the system reads, and nothing
    /// else. `flat_land` with a road under each of civ 0's worked tiles, so
    /// its one city makes two commerce and one gold of tax.
    fn books(buildings: &[Production], treasury: u32) -> (App, Entity) {
        let mut map = flat_land();
        let mut city = test_city(&map);
        for &(x, y) in &city.worked(&map) {
            let i = map.idx(x, y);
            map.tiles[i].road = true;
        }
        let map = bordered(&map, &[&city]);
        city.buildings = buildings.to_vec();
        assert_eq!(city_tax(&map, &city), 1);
        let mut app = App::new();
        app.add_message::<CivilizationEnded>();
        app.insert_resource(map);
        app.insert_resource(UnitArt::default());
        app.insert_resource(GameAudio {
            menu: Handle::default(),
            peace: Handle::default(),
            ui: Default::default(),
            run: Default::default(),
            build: Handle::default(),
            fortify: Default::default(),
            work_road: Handle::default(),
            work_irrigate: Handle::default(),
            work_mine: Handle::default(),
            work_clear: Handle::default(),
            music: None,
        });
        app.init_resource::<MessageBoard>();
        app.init_resource::<Capital>();
        app.insert_resource(Treasury(crate::civs::pad(&[treasury, 0, 0, 0])));
        app.init_resource::<crate::production_prompt::ProductionPrompts>();
        app.init_resource::<crate::abandon::Abandon>();
        app.init_resource::<CityView>();
        app.init_resource::<BuildMenu>();
        app.init_resource::<HurryAsk>();
        app.insert_resource(crate::combat::CombatRng(crate::rng::MapRng::new(1)));
        app.add_systems(Update, end_turn_cities);
        let city = app.world_mut().spawn(city).id();
        (app, city)
    }

    fn muster(app: &mut App, civ: usize, kinds: &[UnitType]) {
        let (x, y) = app.world().resource::<GameMap>().start;
        for &kind in kinds {
            app.world_mut().spawn(Unit::new(civ, kind, x, y));
        }
    }

    fn headcount(app: &mut App, civ: usize) -> usize {
        let mut units = app.world_mut().query::<&Unit>();
        units.iter(app.world()).filter(|u| u.civ == civ).count()
    }

    fn end_turn(app: &mut App, civ: usize) {
        app.world_mut().write_message(CivilizationEnded(civ));
        app.update();
    }

    #[test]
    fn the_turn_pays_tax_in_and_support_out_of_the_treasury() {
        let (mut app, _) = books(&[], 10);
        // Six units, four free: two gold of support against one of tax.
        muster(&mut app, 0, &[UnitType::named("Warrior"); 6]);
        // Civ 1 has units but no city, so nothing is charged.
        muster(&mut app, 1, &[UnitType::named("Warrior"); 9]);
        end_turn(&mut app, 0);
        assert_eq!(app.world().resource::<Treasury>().0[0], 10 + 1 - 2);
        assert_eq!(headcount(&mut app, 0), 6, "it could pay, so nobody left");
        end_turn(&mut app, 1);
        assert_eq!(app.world().resource::<Treasury>().0[1], 0);
        assert_eq!(headcount(&mut app, 1), 9);
    }

    #[test]
    fn a_civ_that_cannot_pay_for_its_units_disbands_the_cheapest() {
        let (mut app, _) = books(&[], 0);
        let mut kinds = vec![UnitType::named("Warrior"); 7];
        kinds.push(UnitType::named("Scout"));
        muster(&mut app, 0, &kinds);
        // Eight units: four gold of support against one of tax and none saved.
        end_turn(&mut app, 0);
        assert_eq!(app.world().resource::<Treasury>().0[0], 0);
        assert_eq!(headcount(&mut app, 0), 7);
        let mut units = app.world_mut().query::<&Unit>();
        let scouts = units
            .iter(app.world())
            .filter(|u| u.utype == UnitType::named("Scout"))
            .count();
        assert_eq!(scouts, 1, "a Warrior goes before the Scout that costs the same");
        let board = app.world().resource::<MessageBoard>();
        assert!(board.text.contains("Warrior"), "got: {}", board.text);
        assert!(board.text.contains("disbanded"), "got: {}", board.text);
    }

    #[test]
    fn a_civ_that_cannot_pay_keeps_its_worker_while_warriors_remain() {
        // A Worker costs what a Warrior costs, and still a Warrior goes,
        // whatever id the Worker has. Bevy ranks an id by its generation
        // before its index, so the Worker is tried first, in the middle and
        // last in the muster.
        for at in [0, 4, 8] {
            let (mut app, _) = books(&[], 0);
            let mut kinds = vec![UnitType::named("Warrior"); 8];
            kinds.insert(at, UnitType::named("Worker"));
            muster(&mut app, 0, &kinds);
            end_turn(&mut app, 0);
            assert_eq!(headcount(&mut app, 0), 8, "Worker mustered at {at}");
            let mut units = app.world_mut().query::<&Unit>();
            let workers = units
                .iter(app.world())
                .filter(|u| u.utype == UnitType::named("Worker"))
                .count();
            assert_eq!(workers, 1, "the Worker mustered at {at} was disbanded");
        }
    }

    #[test]
    fn turns_ending_in_one_frame_let_go_of_different_units() {
        let (mut app, _) = books(&[], 0);
        muster(&mut app, 0, &[UnitType::named("Warrior"); 9]);
        // Nine units against four free and one gold of tax: broke twice.
        app.world_mut().write_message(CivilizationEnded(0));
        app.world_mut().write_message(CivilizationEnded(0));
        app.update();
        assert_eq!(headcount(&mut app, 0), 7, "the same unit went twice");
    }

    #[test]
    fn upkeep_the_treasury_cannot_cover_sells_an_improvement_for_gold() {
        // Two upkeep, one tax, an empty treasury: one gold short. The
        // treasury empties, the newest building goes, and its price is
        // what the civ has left.
        let (mut app, city) = books(&[Production::named("Temple"), Production::named("Barracks")], 0);
        end_turn(&mut app, 0);
        let price = economy::sale_price(0, Production::named("Barracks"));
        assert_eq!(app.world().resource::<Treasury>().0[0], price);
        let kept = &app.world().get::<City>(city).unwrap().buildings;
        assert_eq!(kept, &[Production::named("Temple")], "the newest building goes");
        let board = app.world().resource::<MessageBoard>();
        assert!(board.text.contains("Barracks"), "got: {}", board.text);
        // Covered by savings, upkeep sells nothing.
        let (mut app, city) = books(&[Production::named("Temple"), Production::named("Barracks")], 5);
        end_turn(&mut app, 0);
        assert_eq!(app.world().resource::<Treasury>().0[0], 5 + 1 - 2);
        assert_eq!(app.world().get::<City>(city).unwrap().buildings.len(), 2);
    }

    #[test]
    fn a_short_turn_sells_one_improvement_and_no_more() {
        // Three upkeep against one tax: two short, but the binary's sale
        // routine runs once a turn. The price covers the gap for good.
        let all = [Production::named("Temple"), Production::named("Barracks"), Production::named("Granary")];
        let (mut app, city) = books(&all, 0);
        end_turn(&mut app, 0);
        let kept = &app.world().get::<City>(city).unwrap().buildings;
        assert_eq!(kept, &[Production::named("Temple"), Production::named("Barracks")]);
        assert_eq!(
            app.world().resource::<Treasury>().0[0],
            economy::sale_price(0, Production::named("Granary"))
        );
    }

    #[test]
    fn improvements_are_paid_before_units() {
        // One upkeep, one gold of support, one gold of tax: only one of the
        // two bills can be met. Upkeep goes first, so a unit goes and the
        // Temple stays.
        let (mut app, city) = books(&[Production::named("Temple")], 0);
        muster(&mut app, 0, &[UnitType::named("Warrior"); 5]);
        end_turn(&mut app, 0);
        assert_eq!(app.world().resource::<Treasury>().0[0], 0);
        assert_eq!(headcount(&mut app, 0), 4);
        let kept = &app.world().get::<City>(city).unwrap().buildings;
        assert_eq!(kept, &[Production::named("Temple")]);
    }

    #[test]
    fn a_sale_pays_for_the_units_that_follow_it() {
        // Two upkeep, one tax: the Barracks sells, and its price covers the
        // two gold of support, so nobody is disbanded.
        let (mut app, city) = books(&[Production::named("Temple"), Production::named("Barracks")], 0);
        muster(&mut app, 0, &[UnitType::named("Warrior"); 6]);
        end_turn(&mut app, 0);
        let price = economy::sale_price(0, Production::named("Barracks"));
        assert_eq!(app.world().resource::<Treasury>().0[0], price - 2);
        assert_eq!(headcount(&mut app, 0), 6);
        assert_eq!(app.world().get::<City>(city).unwrap().buildings.len(), 1);
    }
}
