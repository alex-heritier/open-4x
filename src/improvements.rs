//! Worker improvements: roads and irrigation.
//!
//! Overlay art follows the recovered mask tables
//! (`graphics-terrain.md`): roads are a 256-entry 8-neighbor table,
//! irrigation a 16-entry 4-edge table per base terrain. Mask bit orders
//! below were verified by stub-tile inspection (roads) and edge-contact
//! plus a 2x2 continuity composite (irrigation); see prep tool notes.
//!
//! Labor follows `worker-jobs.md` section 6: each worker stores its own
//! accumulated work; matching jobs on a tile pool that work across owners.
//! Terrain movement cost multiplies the job's required labor; government,
//! Industrious, doubling advances, nationality and PRTO strength set the rate.
//! Irrigation uses small lakes, existing farms and one neighboring city
//! (`worker-jobs.md` 3.2), including rivers on the working tile.

use bevy::prelude::*;
use bevy::sprite::Anchor;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

use crate::audio::GameAudio;
use crate::cities::City;
use crate::features::{MessageBoard, post};
use crate::map::{Base, Cover, GameMap, Tile, tile_to_world};
use crate::render::{Fog, RevealAll, fog_for, tile_z};
use crate::units::{Unit, UnitAnim};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub enum WorkAction {
    Road,
    Irrigate,
    Mine,
    /// Clear forest, jungle or pine cover back to bare ground.
    Clear,
    /// A fortress: +50% defense on the tile (`sites.rs`).
    Fortress,
    Barricade,
    Outpost,
}

/// Native TFRM job row; clearing jungle uses Clear Wetlands rather than Forest.
pub fn work_needed(action: WorkAction, tile: &Tile) -> i32 {
    let row = match action {
        WorkAction::Mine => 0,
        WorkAction::Irrigate => 1,
        WorkAction::Fortress => 2,
        WorkAction::Road => 3,
        WorkAction::Clear if tile.cover == Cover::Jungle => 7,
        WorkAction::Clear => 6,
        WorkAction::Outpost => 11,
        WorkAction::Barricade => 12,
    };
    crate::ruleset::WORK_NEEDED[row]
        * i32::from(crate::ruleset::TERRAINS[crate::map::terrain_row(tile)].movement)
}

/// `0x5B33C0`: single-precision multiplications and truncation in native order.
pub fn work_rate(unit: &Unit) -> i32 {
    let mut rate = crate::realm::govt(unit.civ).worker_steps as f32;
    if crate::civs::RACES[unit.civ].traits & (1 << 5) != 0 {
        rate *= 1.5;
    }
    if crate::realm::read(unit.civ, |r| r.known & crate::ruleset::doubles_work() != 0) {
        rate += rate;
    }
    if unit.nationality != crate::civs::roster_index(unit.civ) {
        rate *= 0.5;
    }
    (rate * crate::units::def(unit.utype).worker_strength)
        .trunc()
        .max(1.0) as i32
}

/// Estimate owner turns left with the workers currently sharing this job.
pub fn work_turns_left<'a>(unit: &Unit, tile: &Tile, units: impl Iterator<Item = &'a Unit>) -> i32 {
    let Some(job) = unit.work else { return 0 };
    let (mut progress, mut rate) = (0, 0);
    for other in units {
        if (other.x, other.y) == (unit.x, unit.y)
            && other.work.is_some_and(|w| w.action == job.action)
        {
            progress += other.work.unwrap().progress;
            if other.civ == unit.civ {
                rate += work_rate(other);
            }
        }
    }
    let left = (work_needed(job.action, tile) - progress).max(0);
    (left + rate - 1) / rate.max(1)
}

fn action_name(a: WorkAction) -> &'static str {
    match a {
        WorkAction::Road => "road",
        WorkAction::Irrigate => "irrigation",
        WorkAction::Mine => "mine",
        WorkAction::Clear => "clearing",
        WorkAction::Fortress => "fortress",
        WorkAction::Barricade => "barricade",
        WorkAction::Outpost => "outpost",
    }
}

pub fn action_slot(a: WorkAction) -> &'static str {
    match a {
        WorkAction::Road => "ROAD",
        WorkAction::Irrigate => "IRRIGATE",
        WorkAction::Mine => "MINE",
        WorkAction::Clear => "FOREST",
        WorkAction::Fortress | WorkAction::Barricade | WorkAction::Outpost => "FORTRESS",
    }
}

/// In-progress worker job on a unit.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct Work {
    pub action: WorkAction,
    pub progress: i32,
}

/// Road mask bit order: the 8 map-neighbors screen-clockwise from map-N.
/// Derived from single-bit stub centroids: bit0 upper-right (N edge),
/// bit1 right (NE corner), bit2 lower-right (E edge), bit3 down (SE
/// corner), bit4 lower-left (S edge), bit5 left (SW corner), bit6
/// upper-left (W edge), bit7 up (NW corner).
pub const ROAD_DIRS: [(i32, i32); 8] = [
    (0, -1),  // bit 0: N
    (1, -1),  // bit 1: NE
    (1, 0),   // bit 2: E
    (1, 1),   // bit 3: SE
    (0, 1),   // bit 4: S
    (-1, 1),  // bit 5: SW
    (-1, 0),  // bit 6: W
    (-1, -1), // bit 7: NW
];

/// Irrigation edge bits: 0=W 1=N 2=S 3=E (map dirs). Derived from
/// single-bit edge contact (bit0 NW/W edge, bit1 NE/N edge, bit2 SW/S
/// edge, bit3 SE/E edge) and forced by multi-bit combos: mask3 touches
/// NW+NE, mask12 SW+SE, mask5 NW+SW, mask10 NE+SE. An earlier W/E/S/N
/// reading had bits 1 and 3 swapped (same mistake as the road order).
pub const IRR_DIRS: [(i32, i32); 4] = [(-1, 0), (0, -1), (0, 1), (1, 0)];

pub fn is_water_base(b: Base) -> bool {
    matches!(b, Base::Ocean | Base::Sea | Base::Coast)
}

/// Native roads require a nonzero TERR road bonus, including mountains.
pub fn can_road(map: &GameMap, x: i32, y: i32) -> bool {
    map.get(x, y).is_some_and(|t| {
        map.is_land(x, y)
            && crate::ruleset::TERRAINS[crate::map::terrain_row(t)].road != 0
            && !t.road
    })
}

/// A mine replaces irrigation on terrain with a nonzero TERR mining bonus.
pub fn can_mine(map: &GameMap, x: i32, y: i32) -> bool {
    map.get(x, y).is_some_and(|t| {
        map.is_land(x, y)
            && !t.mine
            && crate::ruleset::TERRAINS[crate::map::terrain_row(t)].mining != 0
    })
}

/// Clear Forest / Wetlands must be the effective terrain's job, on unowned or own land.
pub fn can_clear(map: &GameMap, civ: usize, x: i32, y: i32) -> bool {
    map.get(x, y).is_some_and(|t| {
        map.is_land(x, y)
            && t.owner.is_none_or(|owner| owner as usize == civ)
            && matches!(
                crate::ruleset::TERRAINS[crate::map::terrain_row(t)].worker_job,
                6 | 7
            )
    })
}

/// Native water-source predicate 0x5D8400. A city can relay water from
/// freshwater or an adjacent farm; consecutive cities cannot relay it.
fn water_source(map: &GameMap, cities: &[(i32, i32)], x: i32, y: i32, chain: bool) -> bool {
    map.fresh_water(x, y)
        || map.neighbors(x, y).into_iter().any(|(nx, ny)| {
            map.get(nx, ny).is_some_and(|nb| nb.irrigation)
                || (chain && cities.contains(&(nx, ny)) && water_source(map, cities, nx, ny, false))
        })
}

/// Irrigation needs a nonzero TERR bonus and a water source. It replaces a mine.
pub fn can_irrigate(map: &GameMap, cities: &[(i32, i32)], x: i32, y: i32) -> bool {
    map.get(x, y).is_some_and(|t| {
        map.is_land(x, y)
            && !t.irrigation
            && crate::ruleset::TERRAINS[crate::map::terrain_row(t)].irrigation != 0
    }) && water_source(map, cities, x, y, true)
}

/// 8-bit road mask. Cities count as connectors (road hubs).
pub fn road_mask(map: &GameMap, cities: &[(i32, i32)], x: i32, y: i32) -> u8 {
    let mut m = 0u8;
    for (i, (dx, dy)) in ROAD_DIRS.iter().enumerate() {
        let nx = map.wrap_x(x + dx);
        let ny = y + dy;
        if ny < 0 || ny >= map.h {
            continue;
        }
        let linked = map.get(nx, ny).is_some_and(|t| t.road) || cities.contains(&(nx, ny));
        if linked {
            m |= 1 << i;
        }
    }
    m
}

/// 4-bit irrigation edge mask over irrigated orthogonal neighbors.
pub fn irr_mask(map: &GameMap, x: i32, y: i32) -> u8 {
    let mut m = 0u8;
    for (i, (dx, dy)) in IRR_DIRS.iter().enumerate() {
        let nx = map.wrap_x(x + dx);
        let ny = y + dy;
        if ny < 0 || ny >= map.h {
            continue;
        }
        if map.get(nx, ny).is_some_and(|t| t.irrigation) {
            m |= 1 << i;
        }
    }
    m
}

fn irr_sheet(t: &Tile) -> &'static str {
    match t.base {
        Base::Grassland => "grass",
        Base::Plains => "plains",
        Base::Desert => "desert",
        _ => "tundra",
    }
}

/// Hover suffix for a tile's improvements.
pub fn describe(t: &Tile) -> Option<String> {
    let parts: Vec<&str> = [
        (t.road, "Road"),
        (t.irrigation, "Irrigated"),
        (t.mine, "Mine"),
    ]
    .iter()
    .filter_map(|(on, name)| on.then_some(*name))
    .collect();
    (!parts.is_empty()).then(|| parts.join(", "))
}

/// Apply a finished job to its tile.
pub fn apply_work(t: &mut Tile, a: WorkAction) {
    match a {
        WorkAction::Road => t.road = true,
        WorkAction::Irrigate => {
            t.irrigation = true;
            t.mine = false;
        }
        WorkAction::Mine => {
            t.mine = true;
            t.irrigation = false;
        }
        WorkAction::Clear => t.cover = Cover::Bare,
        WorkAction::Fortress | WorkAction::Barricade => {
            if matches!(t.site, Some(crate::sites::Site::Outpost(_))) {
                t.site = None;
            }
            if a == WorkAction::Fortress {
                t.fortress = true;
            } else {
                t.barricade = true;
            }
        }
        // The owner is installed by the worker completion system.
        WorkAction::Outpost => t.fortress = false,
    }
}

/// Advance worker jobs on end turn; complete and apply to the map.
pub fn end_turn_work(
    mut commands: Commands,
    mut end: MessageReader<crate::civs::CivilizationEnded>,
    mut map: ResMut<GameMap>,
    mut units: Query<(Entity, &mut Unit)>,
    mut cities: Query<&mut City>,
    mut board: ResMut<MessageBoard>,
    audio: Res<GameAudio>,
) {
    for event in end.read() {
        let mut actors: Vec<_> = units
            .iter()
            .filter(|(_, u)| u.civ == event.0 && u.work.is_some())
            .map(|(e, _)| e)
            .collect();
        actors.sort();
        for actor in actors {
            let (x, y, action) = {
                let (_, mut u) = units.get_mut(actor).unwrap();
                let Some(mut job) = u.work else { continue };
                if cities.iter().any(|c| (c.x, c.y) == (u.x, u.y)) {
                    u.work = None;
                    u.path.clear();
                    continue;
                }
                job.progress += work_rate(&u);
                u.work = Some(job);
                u.moves = 0;
                (u.x, u.y, job.action)
            };
            let progress: i32 = units
                .iter()
                .filter_map(|(_, u)| {
                    u.work
                        .filter(|w| (u.x, u.y) == (x, y) && w.action == action)
                        .map(|w| w.progress)
                })
                .sum();
            if progress < work_needed(action, &map.tiles[map.idx(x, y)]) {
                continue;
            }
            for (_, mut u) in &mut units {
                if (u.x, u.y) == (x, y) && u.work.is_some_and(|w| w.action == action) {
                    u.work = None;
                    u.path.clear();
                    u.anim = UnitAnim::OneShot {
                        slot: action_slot(action),
                        t: 0.0,
                    };
                }
            }
            let i = map.idx(x, y);
            let forest = crate::map::terrain_row(&map.tiles[i]) == 7;
            let mut harvest = None;
            apply_work(&mut map.tiles[i], action);
            if forest
                && crate::map::terrain_row(&map.tiles[i]) != 7
                && !map.tiles[i].forest_harvested
            {
                map.tiles[i].forest_harvested = true;
                for &(dx, dy) in &crate::cities::border_offsets()[1..=20] {
                    let pos = (map.wrap_x(x + dx), y + dy);
                    if let Some(mut city) = cities.iter_mut().find(|c| {
                        c.civ == event.0
                            && (c.x, c.y) == pos
                            && crate::hurry::ordinary(c.production)
                    }) {
                        let before = city.shields;
                        city.shields = city
                            .shields
                            .saturating_add(crate::ruleset::forest_shields())
                            .min(city.price(city.production));
                        harvest = Some(format!(
                            "{} receives {} shields from the forest.",
                            city.name,
                            city.shields.saturating_sub(before)
                        ));
                        break;
                    }
                }
            }
            if action == WorkAction::Outpost {
                map.tiles[i].site = Some(crate::sites::Site::Outpost(event.0 as u8));
                commands.entity(actor).despawn();
            }
            // The computer's workers finish their jobs unannounced.
            if !crate::civs::is_ai(event.0) {
                commands.spawn(AudioPlayer(audio.work_sfx(action)));
                let message = harvest.unwrap_or_else(|| {
                    format!("Workers complete {} ({x},{y}).", action_name(action))
                });
                post(&mut board, message);
            }
        }
    }
}

// --- Improvement art + overlays ---

#[derive(Deserialize)]
struct ImpEntry {
    file: String,
    size: [u32; 2],
    anchor: [i32; 2],
}

/// One picture of the manifest: where it lives and where its anchor sits.
/// The image is asked for the first time something is drawn with it.
struct ImpDef {
    file: String,
    anchor: Anchor,
    image: std::sync::OnceLock<Handle<Image>>,
}

#[derive(Resource)]
pub struct ImprovementArt {
    defs: HashMap<String, ImpDef>,
}

impl ImprovementArt {
    /// The manifest only: a road has 256 neighbor masks and each irrigation
    /// sheet 16, and a game shows a few dozen, so the pictures are fetched as
    /// they are first drawn, not all at startup (in a browser each is a
    /// request).
    pub fn load() -> Self {
        let text = crate::web::read_text("assets/cache/improvements/manifest.json")
            .expect("run from the repo root: the art cache (assets/cache) is built at startup");
        let raw: HashMap<String, ImpEntry> =
            serde_json::from_str(&text).expect("improvements manifest parses");
        let mut defs = HashMap::new();
        for (name, e) in raw {
            let anchor = Anchor(Vec2::new(
                e.anchor[0] as f32 / e.size[0] as f32 - 0.5,
                0.5 - e.anchor[1] as f32 / e.size[1] as f32,
            ));
            defs.insert(
                name,
                ImpDef {
                    file: e.file,
                    anchor,
                    image: Default::default(),
                },
            );
        }
        Self { defs }
    }

    /// The picture of `key` and its anchor, requesting the image if this is
    /// the first use.
    fn get(&self, key: &str, assets: &AssetServer) -> (Handle<Image>, Anchor) {
        let def = &self.defs[key];
        let image = def
            .image
            .get_or_init(|| assets.load(format!("cache/improvements/{}", def.file)));
        (image.clone(), def.anchor)
    }
}

#[derive(Component, Clone, Copy, PartialEq, Eq, Hash)]
enum ImpLayer {
    Road,
    Irrigation,
    Mine,
    Fortress,
    Colony,
    Outpost,
    Barricade,
}

#[derive(Component)]
pub struct ImprovementSprite {
    x: i32,
    y: i32,
    layer: ImpLayer,
    mask: u8,
}

fn sprite_key(t: &Tile, layer: ImpLayer, mask: u8) -> String {
    match layer {
        ImpLayer::Road => format!("road_{mask}"),
        ImpLayer::Irrigation => format!("irr_{}_{mask}", irr_sheet(t)),
        ImpLayer::Mine => "mine".to_string(),
        // The mask of a structure is the era of its art.
        ImpLayer::Fortress => format!("fortress_{mask}"),
        ImpLayer::Colony => format!("colony_{mask}"),
        ImpLayer::Outpost => "outpost".to_string(),
        ImpLayer::Barricade => format!("barricade_{mask}"),
    }
}

impl ImpLayer {
    const ALL: [ImpLayer; 7] = [
        ImpLayer::Irrigation,
        ImpLayer::Road,
        ImpLayer::Mine,
        ImpLayer::Fortress,
        ImpLayer::Colony,
        ImpLayer::Outpost,
        ImpLayer::Barricade,
    ];

    fn on(self, t: &Tile) -> bool {
        match self {
            ImpLayer::Road => t.road,
            ImpLayer::Irrigation => t.irrigation,
            ImpLayer::Mine => t.mine,
            ImpLayer::Fortress => t.fortress && !t.barricade,
            ImpLayer::Barricade => t.barricade,
            ImpLayer::Outpost => matches!(t.site, Some(crate::sites::Site::Outpost(_))),
            ImpLayer::Colony => matches!(t.site, Some(crate::sites::Site::Colony(_))),
        }
    }

    fn mask(self, map: &GameMap, hubs: &[(i32, i32)], x: i32, y: i32, era: u8) -> u8 {
        match self {
            ImpLayer::Road => road_mask(map, hubs, x, y),
            ImpLayer::Irrigation => irr_mask(map, x, y),
            ImpLayer::Mine => 0,
            ImpLayer::Fortress | ImpLayer::Colony | ImpLayer::Barricade => era,
            ImpLayer::Outpost => 0,
        }
    }

    /// Draw order above the base tile: irrigation, roads, then the mine.
    fn z(self) -> f32 {
        match self {
            ImpLayer::Irrigation => 1.5,
            ImpLayer::Road => 1.6,
            ImpLayer::Mine => 1.7,
            ImpLayer::Fortress | ImpLayer::Colony | ImpLayer::Barricade | ImpLayer::Outpost => 1.8,
        }
    }
}

/// What the overlays were last built from besides the map: the viewer's era
/// and where the cities were.
type Basis = (u8, Vec<(i32, i32)>);

/// Spawn overlays for new improvements, refresh masks, tint by fog.
pub fn sync_improvement_sprites(
    mut commands: Commands,
    map: Res<GameMap>,
    art: Res<ImprovementArt>,
    reveal: Res<RevealAll>,
    cities: Query<&City>,
    research: Res<crate::research::Research>,
    civs: Res<crate::civs::Civilizations>,
    mut q: Query<(Entity, &mut ImprovementSprite, &mut Sprite, &mut Visibility)>,
    assets: Res<AssetServer>,
    images: Res<Assets<Image>>,
    mut last: Local<Option<Basis>>,
    mut waiting: Local<bool>,
) {
    // Structures are drawn in the viewer's era (`graphics-terrain.md`).
    let era = crate::tech_tree::era_of(&research, civs.viewer()) as u8;
    let hubs: Vec<(i32, i32)> = cities.iter().map(|c| (c.x, c.y)).collect();
    // The overlays follow the map, the reveal switch, the era and where the
    // cities are (the roads meet there); with all of those as they were,
    // and no picture still on its way, there is nothing to redo.
    let same = last.as_ref().is_some_and(|(e, h)| *e == era && *h == hubs);
    if same && !*waiting && !(map.is_changed() || reveal.is_changed() || art.is_changed()) {
        return;
    }
    *last = Some((era, hubs.clone()));
    *waiting = false;
    let mut have: HashSet<(i32, i32, ImpLayer)> = HashSet::new();
    for (e, mut fs, mut sprite, mut vis) in q.iter_mut() {
        let t = &map.tiles[map.idx(fs.x, fs.y)];
        // A mine replaces irrigation and vice versa: drop stale overlays.
        if !fs.layer.on(t) {
            commands.entity(e).despawn();
            continue;
        }
        have.insert((fs.x, fs.y, fs.layer));
        let mask = fs.layer.mask(&map, &hubs, fs.x, fs.y, era);
        if mask != fs.mask {
            // Neighbor change: swap to the new mask tile, once it is here;
            // until then the old one stays up rather than a gap.
            let (image, _) = art.get(&sprite_key(t, fs.layer, mask), &assets);
            let failed = matches!(assets.load_state(&image), bevy::asset::LoadState::Failed(_));
            if images.contains(&image) || failed {
                sprite.image = image;
                fs.mask = mask;
            } else {
                *waiting = true;
            }
        }
        // Improvements sit below the fog diamonds, which do the dimming;
        // unseen ones hide so nothing spills past the fog edge.
        if sprite.color != Color::WHITE {
            sprite.color = Color::WHITE;
        }
        vis.set_if_neq(match fog_for(reveal.0, t) {
            Fog::Black => Visibility::Hidden,
            _ => Visibility::Visible,
        });
    }
    // Spawn overlays for improvements that lack them.
    for y in 0..map.h {
        for x in 0..map.w {
            let t = &map.tiles[map.idx(x, y)];
            let pos = tile_to_world(x, y);
            for layer in ImpLayer::ALL {
                if !layer.on(t) || have.contains(&(x, y, layer)) {
                    continue;
                }
                let mask = layer.mask(&map, &hubs, x, y, era);
                let (image, anchor) = art.get(&sprite_key(t, layer, mask), &assets);
                let vis = match fog_for(reveal.0, t) {
                    Fog::Black => Visibility::Hidden,
                    _ => Visibility::Visible,
                };
                commands.spawn((
                    Sprite { image, ..default() },
                    anchor,
                    vis,
                    Transform::from_xyz(pos.x, pos.y, tile_z(x, y, layer.z())),
                    ImprovementSprite { x, y, layer, mask },
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{Base, Cover, Relief};

    #[test]
    fn road_mask_bits_follow_verified_order() {
        // 3x3 road block on the default map: center sees all 8.
        let mut map = GameMap::generate();
        let (cx, cy) = (40, 30);
        for dy in -1..=1 {
            for dx in -1..=1 {
                let i = map.idx(cx + dx, cy + dy);
                map.tiles[i].road = true;
            }
        }
        assert_eq!(road_mask(&map, &[], cx, cy), 0xFF);
        // Northwest corner of the block: E, SE, S neighbors only.
        assert_eq!(road_mask(&map, &[], cx - 1, cy - 1), 0b00011100);
        // Cities connect as hubs.
        let map = GameMap::generate();
        assert_eq!(road_mask(&map, &[(cx + 1, cy)], cx, cy), 0b00000100);
    }

    #[test]
    fn irr_mask_bits_are_w_n_s_e() {
        let mut map = GameMap::generate();
        let (cx, cy) = (40, 30);
        for (x, y) in [(cx, cy), (cx - 1, cy), (cx, cy + 1)] {
            let i = map.idx(x, y);
            map.tiles[i].irrigation = true;
        }
        // W (bit0) and S (bit2) neighbors.
        assert_eq!(irr_mask(&map, cx, cy), 0b0101);
        // E neighbor lives on bit3, not bit1.
        assert_eq!(irr_mask(&map, cx - 1, cy), 0b1000); // E only
        // N neighbor lives on bit1.
        assert_eq!(irr_mask(&map, cx, cy + 1), 0b0010); // N only
    }

    #[test]
    fn road_rules() {
        let map = GameMap::generate();
        let (sx, sy) = map.start;
        assert!(can_road(&map, sx, sy));
        let ocean = (0..map.h)
            .flat_map(|y| (0..map.w).map(move |x| (x, y)))
            .find(|(x, y)| matches!(map.get(*x, *y).map(|t| t.base), Some(Base::Ocean)))
            .expect("map has ocean");
        assert!(!can_road(&map, ocean.0, ocean.1));
    }

    fn dry_map() -> GameMap {
        let mut map = GameMap::generate();
        for t in &mut map.tiles {
            t.base = Base::Grassland;
            t.relief = Relief::Flat;
            t.cover = Cover::Bare;
            t.river = 0;
            t.irrigation = false;
        }
        map
    }

    #[test]
    fn irrigation_accepts_small_lakes_but_not_oceans() {
        let mut map = dry_map();
        for x in 10..30 {
            let i = map.idx(x, 10);
            map.tiles[i].base = Base::Coast;
        }
        assert!(map.fresh_water(10, 9));
        assert!(can_irrigate(&map, &[], 10, 9));
        let i = map.idx(30, 10);
        map.tiles[i].base = Base::Sea;
        assert!(!map.fresh_water(10, 9));
        assert!(!can_irrigate(&map, &[], 10, 9));
        assert!(map.coastal_site(10, 9));
    }

    #[test]
    fn water_bodies_wrap_but_do_not_join_at_corners() {
        let mut map = dry_map();
        for x in 0..20 {
            let i = map.idx(x, 10);
            map.tiles[i].base = Base::Coast;
        }
        // Diagonal contact is a separate lake, not the 21st connected tile.
        let corner = map.idx(20, 11);
        map.tiles[corner].base = Base::Coast;
        assert!(map.fresh_water(0, 9));
        assert!(!map.coastal_site(0, 9));
        let wrapped = map.idx(map.w - 1, 10);
        map.tiles[wrapped].base = Base::Ocean;
        assert!(!map.fresh_water(0, 9));
        assert!(map.coastal_site(0, 9));
        assert!(map.fresh_water(21, 12));
    }

    #[test]
    fn irrigation_crosses_one_city_but_not_a_chain_of_cities() {
        let mut map = dry_map();
        let lake = map.idx(10, 10);
        map.tiles[lake].base = Base::Coast;
        let relay = (11, 10);
        assert!(!can_irrigate(&map, &[], 12, 10));
        assert!(can_irrigate(&map, &[relay], 12, 10));
        assert!(!can_irrigate(&map, &[relay, (12, 10)], 13, 10));
        // A city beside a farm works even without a lake, including diagonals.
        map.tiles[lake].base = Base::Grassland;
        map.tiles[lake].irrigation = true;
        assert!(can_irrigate(&map, &[relay], 12, 11));
        let farm = map.idx(12, 11);
        map.tiles[farm].resource = None;
        let worker = Unit::new(0, crate::units::UnitType::named("Worker"), 12, 11);
        assert!(
            crate::actionbar::UnitCommand::Work(WorkAction::Irrigate).enabled(
                &map,
                &[relay],
                &worker
            )
        );
        assert_eq!(
            crate::ai::job_at(&map, &[relay], (12, 11)),
            Some(WorkAction::Irrigate)
        );
        map.tiles[lake].irrigation = false;
        assert!(
            !crate::actionbar::UnitCommand::Work(WorkAction::Irrigate).enabled(
                &map,
                &[relay],
                &worker
            )
        );
        assert_eq!(
            crate::ai::job_at(&map, &[relay], (12, 11)),
            Some(WorkAction::Mine)
        );
    }

    #[test]
    fn irrigation_rules_need_farmable_land_and_water() {
        let map = GameMap::generate();
        // A farmable, water-adjacent tile exists and irrigates.
        let good = (0..map.h)
            .flat_map(|y| (0..map.w).map(move |x| (x, y)))
            .find(|(x, y)| {
                map.get(*x, *y).is_some_and(|t| {
                    matches!(t.base, Base::Grassland | Base::Plains | Base::Desert)
                        && t.relief == Relief::Flat
                        && t.cover == Cover::Bare
                        && map.fresh_water(*x, *y)
                })
            })
            .expect("map has irrigable land");
        assert!(can_irrigate(&map, &[], good.0, good.1));
        // Forest blocks even next to water.
        let woods = (0..map.h)
            .flat_map(|y| (0..map.w).map(move |x| (x, y)))
            .find(|(x, y)| {
                map.get(*x, *y).is_some_and(|t| {
                    t.cover == Cover::Forest
                        && map.neighbors(*x, *y).iter().any(|(nx, ny)| {
                            map.get(*nx, *ny).is_some_and(|nb| is_water_base(nb.base))
                        })
                })
            });
        if let Some((x, y)) = woods {
            assert!(!can_irrigate(&map, &[], x, y));
        }
        // Ocean never irrigates.
        let ocean = (0..map.h)
            .flat_map(|y| (0..map.w).map(move |x| (x, y)))
            .find(|(x, y)| matches!(map.get(*x, *y).map(|t| t.base), Some(Base::Ocean)))
            .expect("map has ocean");
        assert!(!can_irrigate(&map, &[], ocean.0, ocean.1));
    }

    #[test]
    fn irrigation_chains_through_irrigated_neighbors() {
        let mut map = GameMap::generate();
        // A farmable tile with no water neighbor fails until an adjacent
        // tile is irrigated.
        let inland = (0..map.h)
            .flat_map(|y| (0..map.w).map(move |x| (x, y)))
            .find(|(x, y)| {
                map.get(*x, *y).is_some_and(|t| {
                    matches!(t.base, Base::Grassland | Base::Plains | Base::Desert)
                        && t.relief == Relief::Flat
                        && t.cover == Cover::Bare
                        && !map.fresh_water(*x, *y)
                })
            })
            .expect("map has inland farmable land");
        assert!(!can_irrigate(&map, &[], inland.0, inland.1));
        let (nx, ny) = map.neighbors(inland.0, inland.1)[0];
        let i = map.idx(nx, ny);
        map.tiles[i].irrigation = true;
        assert!(can_irrigate(&map, &[], inland.0, inland.1));
    }

    #[test]
    fn some_seed_has_irrigable_tile_near_start() {
        // Live-E2E finder: water-adjacent farmable land within 3 of start.
        let mut found = None;
        for seed in 0..3000u64 {
            let map = GameMap::generate_with_seed(seed);
            let (sx, sy) = map.start;
            'tiles: for dy in -3..=3 {
                for dx in -3..=3 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let (x, y) = (map.wrap_x(sx + dx), sy + dy);
                    if y < 0 || y >= map.h {
                        continue;
                    }
                    if can_irrigate(&map, &[], x, y) && map.find_path((sx, sy), (x, y)).is_some() {
                        found = Some((seed, (sx, sy), (x, y)));
                        break 'tiles;
                    }
                }
            }
            if found.is_some() {
                break;
            }
        }
        let (seed, start, tile) = found.expect("irrigable tile near start");
        println!("IRR_SEED seed={seed} start={start:?} tile={tile:?}");
    }

    #[test]
    fn describe_covers_all_layer_combos() {
        let mut map = GameMap::generate();
        let (sx, sy) = map.start;
        let i = map.idx(sx, sy);
        assert_eq!(describe(&map.tiles[i]), None);
        map.tiles[i].road = true;
        assert_eq!(describe(&map.tiles[i]).as_deref(), Some("Road"));
        map.tiles[i].irrigation = true;
        assert_eq!(describe(&map.tiles[i]).as_deref(), Some("Road, Irrigated"));
        map.tiles[i].road = false;
        assert_eq!(describe(&map.tiles[i]).as_deref(), Some("Irrigated"));
    }

    #[test]
    fn work_completes_and_applies() {
        use bevy::prelude::*;
        let map = GameMap::generate();
        let (sx, sy) = map.start;
        let mut app = App::new();
        app.insert_resource(map);
        app.insert_resource(MessageBoard::default());
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
        app.add_message::<crate::civs::CivilizationEnded>();
        app.world_mut().spawn(Unit {
            civ: 0,
            utype: crate::units::UnitType::named("Worker"),
            x: sx,
            y: sy,
            moves: 1,
            fortified: false,
            facing: 0,
            path: Default::default(),
            anim: UnitAnim::Idle { t: 0.0 },
            sentry: false,
            exploring: false,
            work: Some(Work {
                action: WorkAction::Road,
                progress: 4,
            }),
            ..Unit::new(0, crate::units::UnitType::named("Worker"), sx, sy)
        });
        app.add_systems(Update, end_turn_work);
        app.world_mut()
            .resource_mut::<Messages<crate::civs::CivilizationEnded>>()
            .write(crate::civs::CivilizationEnded(0));
        app.update();
        let map = app.world().resource::<GameMap>();
        assert!(map.tiles[map.idx(sx, sy)].road);
        let board = app.world().resource::<MessageBoard>();
        assert!(board.text.contains("road"), "got: {}", board.text);
    }

    fn test_app(map: GameMap) -> App {
        let mut app = App::new();
        app.insert_resource(map);
        app.insert_resource(MessageBoard::default());
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
        app.add_message::<crate::civs::CivilizationEnded>();
        app.add_systems(Update, end_turn_work);
        app
    }

    #[test]
    fn an_outpost_consumes_one_worker_keeps_sight_and_is_destroyed_by_foreign_entry() {
        crate::civs::set_controllers();
        let mut map = GameMap::generate_with_seed(1);
        let i = map.idx(10, 10);
        map.tiles[i].base = Base::Grassland;
        map.tiles[i].relief = Relief::Mountain;
        map.tiles[i].fortress = true;
        map.tiles[i].owner = None;
        let mut app = test_app(map);
        app.edit_schedule(Update, |s| {
            s.set_executor_kind(bevy::ecs::schedule::ExecutorKind::SingleThreaded);
        });
        app.init_resource::<crate::civs::Civilizations>();
        app.init_resource::<crate::units::Exploration>();
        app.add_systems(
            Update,
            (
                crate::sites::remove_overrun,
                crate::units::refresh_visibility,
            )
                .chain()
                .after(end_turn_work),
        );
        crate::realm::reset();
        let mut actors: Vec<_> = (0..2)
            .map(|_| {
                app.world_mut()
                    .spawn(worker(10, 10, WorkAction::Outpost, 0))
                    .id()
            })
            .collect();
        actors.sort();
        app.world_mut()
            .write_message(crate::civs::CivilizationEnded(0));
        app.update();
        let remaining: Vec<_> = app
            .world_mut()
            .query::<(Entity, &Unit)>()
            .iter(app.world())
            .map(|(e, _)| e)
            .collect();
        assert_eq!(
            remaining,
            vec![actors[0]],
            "the second contribution crosses the mountain threshold and consumes its actor"
        );
        app.world_mut().despawn(remaining[0]);
        app.update();
        let map = app.world().resource::<GameMap>();
        assert_eq!(map.tiles[i].site, Some(crate::sites::Site::Outpost(0)));
        assert!(!map.tiles[i].fortress);
        assert!(
            map.get(13, 13).unwrap().visible,
            "mountain outpost sees the full 7x7 without a unit"
        );
        assert!(!map.get(14, 14).unwrap().visible);
        app.world_mut().spawn(Unit::new(
            1,
            crate::units::UnitType::named("Warrior"),
            10,
            10,
        ));
        app.update();
        let map = app.world().resource::<GameMap>();
        assert!(map.tiles[i].site.is_none());
        assert!(!map.get(13, 13).unwrap().visible);
        assert!(
            map.get(13, 13).unwrap().seen,
            "destruction preserves explored history"
        );
    }

    fn worker(x: i32, y: i32, action: WorkAction, progress: i32) -> Unit {
        Unit {
            civ: 0,
            utype: crate::units::UnitType::named("Worker"),
            x,
            y,
            moves: 3,
            fortified: false,
            facing: 0,
            path: Default::default(),
            anim: UnitAnim::Idle { t: 0.0 },
            sentry: false,
            exploring: false,
            work: Some(Work { action, progress }),
            ..Unit::new(0, crate::units::UnitType::named("Worker"), x, y)
        }
    }

    fn end_turn(app: &mut App) {
        app.world_mut()
            .resource_mut::<Messages<crate::civs::CivilizationEnded>>()
            .write(crate::civs::CivilizationEnded(0));
        app.update();
    }

    #[test]
    fn workers_pool_actual_labor_and_release_foreign_workers() {
        crate::civs::set_controllers();
        crate::realm::reset();
        let mut map = GameMap::generate();
        let (x, y) = map.start;
        for px in [x, x + 1] {
            let i = map.idx(px, y);
            map.tiles[i].relief = Relief::Flat;
            map.tiles[i].cover = Cover::Bare;
        }
        let mut app = test_app(map);
        let a = app
            .world_mut()
            .spawn(worker(x, y, WorkAction::Road, 0))
            .id();
        let b = app
            .world_mut()
            .spawn(worker(x, y, WorkAction::Road, 0))
            .id();
        let c = app
            .world_mut()
            .spawn(worker(x + 1, y, WorkAction::Road, 0))
            .id();
        let mut foreign = worker(x, y, WorkAction::Road, 0);
        foreign.civ = 1;
        let foreign = app.world_mut().spawn(foreign).id();
        let mut query = app.world_mut().query::<&Unit>();
        assert_eq!(
            work_turns_left(
                app.world().get::<Unit>(a).unwrap(),
                app.world().resource::<GameMap>().get(x, y).unwrap(),
                query.iter(app.world())
            ),
            2
        );
        end_turn(&mut app);
        for e in [a, b, c] {
            let u = app.world().get::<Unit>(e).unwrap();
            assert_eq!(u.work.unwrap().progress, 2);
            assert_eq!(u.moves, 0);
        }
        assert_eq!(
            app.world()
                .get::<Unit>(foreign)
                .unwrap()
                .work
                .unwrap()
                .progress,
            0,
            "only the owner advances work"
        );
        // Previous foreign labor participates in completion, but its owner
        // does not gain a contribution during Japan's end turn.
        app.world_mut()
            .get_mut::<Unit>(foreign)
            .unwrap()
            .work
            .as_mut()
            .unwrap()
            .progress = 2;
        end_turn(&mut app);
        for e in [a, b, foreign] {
            assert!(app.world().get::<Unit>(e).unwrap().work.is_none());
        }
        assert_eq!(
            app.world().get::<Unit>(c).unwrap().work.unwrap().progress,
            4
        );
        let map = app.world().resource::<GameMap>();
        assert!(map.get(x, y).unwrap().road);
        assert!(!map.get(x + 1, y).unwrap().road);
        end_turn(&mut app);
        assert!(
            app.world()
                .resource::<GameMap>()
                .get(x + 1, y)
                .unwrap()
                .road,
            "a native non-Industrious worker takes three turns on flat land"
        );
    }

    #[test]
    fn founding_a_city_cancels_work_without_spending_labor() {
        let map = GameMap::generate();
        let (x, y) = map.start;
        let mut app = test_app(map);
        let e = app
            .world_mut()
            .spawn(worker(x, y, WorkAction::Road, 0))
            .id();
        app.world_mut().spawn(City::new(0, "Kyoto", x, y));
        end_turn(&mut app);
        let u = app.world().get::<Unit>(e).unwrap();
        assert!(u.work.is_none());
        assert_eq!(u.moves, 3);
        assert!(!app.world().resource::<GameMap>().get(x, y).unwrap().road);
    }

    #[test]
    fn terrain_and_nationality_set_native_work_duration() {
        crate::civs::set_controllers();
        crate::realm::reset();
        let map = GameMap::generate();
        let mut tile = map.get(map.start.0, map.start.1).unwrap().clone();
        tile.relief = Relief::Flat;
        tile.cover = Cover::Bare;
        tile.road = true; // roads never reduce required labor
        assert_eq!(work_needed(WorkAction::Road, &tile), 6);
        tile.relief = Relief::Hill;
        assert_eq!(work_needed(WorkAction::Mine, &tile), 24);
        tile.relief = Relief::Mountain;
        assert_eq!(work_needed(WorkAction::Barricade, &tile), 48);
        tile.relief = Relief::Flat;
        tile.cover = Cover::Forest;
        assert_eq!(work_needed(WorkAction::Clear, &tile), 8);
        tile.cover = Cover::Jungle;
        assert_eq!(work_needed(WorkAction::Clear, &tile), 48);
        let mut u = worker(0, 0, WorkAction::Road, 0);
        assert_eq!(work_rate(&u), 2);
        u.civ = 1;
        assert_eq!(
            work_rate(&u),
            1,
            "captured Japanese worker under non-Industrious Rome"
        );
        u.civ = 0;
        // Industrious Egypt in the same game slot.
        let egypt = crate::civs::roster_index_named("Egypt").unwrap();
        let mut roster = crate::civs::players();
        roster[0] = egypt;
        crate::civs::set_players_for_test(&roster);
        u.nationality = egypt;
        assert_eq!(work_rate(&u), 3);
        u.nationality = crate::civs::roster_index_named("Japan").unwrap();
        assert_eq!(
            work_rate(&u),
            1,
            "3 * 0.5 truncates after all multiplications"
        );
        u.nationality = egypt;
        crate::realm::write(0, |r| {
            r.govt = civ3mapgen::government::row::FASCISM;
            r.known |= crate::ruleset::doubles_work();
        });
        assert_eq!(work_rate(&u), 12);
        u.nationality = crate::civs::roster_index_named("Japan").unwrap();
        assert_eq!(work_rate(&u), 6);
        u.utype = crate::units::UnitType::named("Warrior");
        assert_eq!(work_rate(&u), 1, "zero PRTO strength still floors at one");
        crate::civs::set_controllers();
    }

    fn chop_app(cover: Cover) -> App {
        crate::civs::set_controllers();
        crate::realm::reset();
        let mut map = GameMap::generate();
        let i = map.idx(0, 10);
        map.tiles[i].base = Base::Grassland;
        map.tiles[i].relief = Relief::Flat;
        map.tiles[i].cover = cover;
        map.tiles[i].owner = Some(0);
        let need = work_needed(WorkAction::Clear, &map.tiles[i]);
        let mut app = test_app(map);
        app.world_mut()
            .spawn(worker(0, 10, WorkAction::Clear, need - 2));
        app
    }

    #[test]
    fn chopping_pays_the_first_eligible_city_and_never_pays_twice() {
        use crate::cities::Production;
        let mut app = chop_app(Cover::Forest);
        let mut wonder = City::new(0, "Wonder", 0, 9); // first spiral tile
        wonder.production = Production::named("The Pyramids");
        let wonder = app.world_mut().spawn(wonder).id();
        let enemy = app.world_mut().spawn(City::new(1, "Enemy", 1, 9)).id();
        let mut first = City::new(0, "First", 1, 10);
        first.production = Production::named("Spearman");
        first.shields = 15;
        let first = app.world_mut().spawn(first).id();
        let later = app.world_mut().spawn(City::new(0, "Later", 0, 11)).id();
        end_turn(&mut app);
        assert_eq!(
            app.world().get::<City>(first).unwrap().shields,
            20,
            "chop is capped at the current item cost"
        );
        assert!(
            app.world()
                .resource::<MessageBoard>()
                .text
                .contains("First receives 5 shields")
        );
        for e in [wonder, enemy, later] {
            assert_eq!(app.world().get::<City>(e).unwrap().shields, 0);
        }
        let map = app.world().resource::<GameMap>();
        assert_eq!(map.get(0, 10).unwrap().cover, Cover::Bare);
        assert!(map.get(0, 10).unwrap().forest_harvested);
        // A replanted forest cannot earn shields again, even after switching.
        let i = app.world().resource::<GameMap>().idx(0, 10);
        app.world_mut().resource_mut::<GameMap>().tiles[i].cover = Cover::Forest;
        app.world_mut().get_mut::<City>(first).unwrap().shields = 0;
        app.world_mut().spawn(worker(0, 10, WorkAction::Clear, 6));
        end_turn(&mut app);
        assert_eq!(app.world().get::<City>(first).unwrap().shields, 0);
    }

    #[test]
    fn chopping_wraps_and_spends_the_bonus_even_without_a_recipient() {
        let mut app = chop_app(Cover::Pine);
        let wrapped = app.world_mut().spawn(City::new(0, "Wrapped", 79, 10)).id();
        end_turn(&mut app);
        assert_eq!(app.world().get::<City>(wrapped).unwrap().shields, 10);
        let mut app = chop_app(Cover::Forest);
        end_turn(&mut app);
        assert!(
            app.world()
                .resource::<GameMap>()
                .get(0, 10)
                .unwrap()
                .forest_harvested
        );
        let mut app = chop_app(Cover::Jungle);
        let near = app.world_mut().spawn(City::new(0, "Nearby", 1, 10)).id();
        end_turn(&mut app);
        assert_eq!(
            app.world().get::<City>(near).unwrap().shields,
            0,
            "wetlands give no harvest"
        );
        assert!(
            !app.world()
                .resource::<GameMap>()
                .get(0, 10)
                .unwrap()
                .forest_harvested
        );
    }

    #[test]
    fn worker_terrain_gates_use_native_bonuses_and_ownership() {
        let mut map = GameMap::generate();
        let i = map.idx(10, 10);
        map.tiles[i].relief = Relief::Flat;
        map.tiles[i].cover = Cover::Bare;
        map.tiles[i].mine = false;
        map.tiles[i].road = false;
        map.tiles[i].irrigation = false;
        for base in [Base::Grassland, Base::Plains, Base::Desert, Base::Tundra] {
            map.tiles[i].base = base;
            assert!(can_mine(&map, 10, 10));
        }
        assert!(
            !can_irrigate(&map, &[], 10, 10),
            "tundra has no irrigation bonus"
        );
        map.tiles[i].relief = Relief::Mountain;
        assert!(can_mine(&map, 10, 10) && can_road(&map, 10, 10));
        map.tiles[i].relief = Relief::Flat;
        for cover in [Cover::Forest, Cover::Pine, Cover::Jungle] {
            map.tiles[i].cover = cover;
            assert!(!can_mine(&map, 10, 10));
            map.tiles[i].owner = None;
            assert!(can_clear(&map, 0, 10, 10));
            map.tiles[i].owner = Some(1);
            assert!(!can_clear(&map, 0, 10, 10));
            assert!(can_clear(&map, 1, 10, 10));
        }
        map.tiles[i].relief = Relief::Hill;
        assert!(
            !can_clear(&map, 1, 10, 10),
            "hills override the underlying cover"
        );
    }

    #[test]
    fn mine_and_irrigation_replace_each_other() {
        let map = GameMap::generate();
        let mut t = map.tiles[map.idx(map.start.0, map.start.1)].clone();
        apply_work(&mut t, WorkAction::Irrigate);
        assert!(t.irrigation && !t.mine);
        apply_work(&mut t, WorkAction::Mine);
        assert!(t.mine && !t.irrigation);
        apply_work(&mut t, WorkAction::Irrigate);
        assert!(t.irrigation && !t.mine);
        apply_work(&mut t, WorkAction::Road);
        assert!(t.irrigation && t.road);
    }

    #[test]
    fn mine_and_clear_rules_and_effects() {
        let mut map = GameMap::generate();
        let find = |map: &GameMap, f: &dyn Fn(&Tile) -> bool| {
            (0..map.h)
                .flat_map(|y| (0..map.w).map(move |x| (x, y)))
                .find(|(x, y)| map.get(*x, *y).is_some_and(|t| f(t)))
        };
        let hill = find(&map, &|t| {
            t.relief == Relief::Hill && !matches!(t.base, Base::Ocean | Base::Sea | Base::Coast)
        })
        .expect("map has hills");
        assert!(can_mine(&map, hill.0, hill.1));
        let before = crate::map::yields(map.get(hill.0, hill.1).unwrap()).1;
        let i = map.idx(hill.0, hill.1);
        map.tiles[i].mine = true;
        assert_eq!(crate::map::yields(&map.tiles[i]).1, before + 2);
        assert!(!can_mine(&map, hill.0, hill.1));
        assert_eq!(describe(&map.tiles[i]).as_deref(), Some("Mine"));
        let wood = find(&map, &|t| {
            t.relief == Relief::Flat
                && t.cover != Cover::Bare
                && !matches!(t.base, Base::Ocean | Base::Sea | Base::Coast | Base::Ice)
        })
        .expect("map has flat cover");
        assert!(can_clear(&map, 0, wood.0, wood.1));
        assert!(
            !can_clear(&map, 0, map.start.0, map.start.1)
                || map.get(map.start.0, map.start.1).unwrap().cover != Cover::Bare
        );
    }
}
