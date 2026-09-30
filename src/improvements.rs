//! Worker improvements: roads and irrigation.
//!
//! Overlay art follows the recovered mask tables
//! (`graphics-terrain.md`): roads are a 256-entry 8-neighbor table,
//! irrigation a 16-entry 4-edge table per base terrain. Mask bit orders
//! below were verified by stub-tile inspection (roads) and edge-contact
//! plus a 2x2 continuity composite (irrigation); see prep tool notes.
//!
//! Rules are Civ3-shaped with two sandbox deviations: irrigation water
//! access accepts any water (no rivers exist yet) and work times are
//! shortened (road 4, irrigation 5). Roads give no yield (commerce
//! arrives with the trade slice) but cut road-to-road moves to 1/3 MP;
//! irrigation is +1 food. A mine and irrigation exclude each other: each
//! replaces the other, as in Civ3. Workers on one tile doing the same job
//! pool their labor.

use bevy::prelude::*;
use bevy::sprite::Anchor;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

use crate::audio::GameAudio;
use crate::cities::City;
use crate::features::{MessageBoard, post};
use crate::map::{Base, Cover, GameMap, Relief, Tile, tile_to_world};
use crate::render::{Fog, RevealAll, fog_for, tile_z};
use crate::units::{Unit, UnitAnim};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum WorkAction {
    Road,
    Irrigate,
    Mine,
    /// Clear forest, jungle or pine cover back to bare ground.
    Clear,
}

pub fn work_turns(a: WorkAction) -> u8 {
    match a {
        WorkAction::Road => 4,
        WorkAction::Irrigate => 5,
        WorkAction::Mine => 6,
        WorkAction::Clear => 4,
    }
}

fn action_name(a: WorkAction) -> &'static str {
    match a {
        WorkAction::Road => "road",
        WorkAction::Irrigate => "irrigation",
        WorkAction::Mine => "mine",
        WorkAction::Clear => "clearing",
    }
}

pub fn action_slot(a: WorkAction) -> &'static str {
    match a {
        WorkAction::Road => "ROAD",
        WorkAction::Irrigate => "IRRIGATE",
        WorkAction::Mine => "MINE",
        WorkAction::Clear => "FOREST",
    }
}

/// In-progress worker job on a unit.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Work {
    pub action: WorkAction,
    pub turns_left: u8,
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

/// Roads go on passable land (mountains excluded, forest/jungle kept).
pub fn can_road(map: &GameMap, x: i32, y: i32) -> bool {
    map.get(x, y)
        .is_some_and(|t| map.is_land(x, y) && t.relief != Relief::Mountain && !t.road)
}

/// Mines go on hills, mountains and bare desert (Civ3 mine terrain). A
/// mine replaces irrigation.
pub fn can_mine(map: &GameMap, x: i32, y: i32) -> bool {
    map.get(x, y).is_some_and(|t| {
        !t.mine
            && map.is_land(x, y)
            && (t.relief != Relief::Flat || (t.base == Base::Desert && t.cover == Cover::Bare))
    })
}

/// Forest, jungle and pine cover on flat land can be cleared.
pub fn can_clear(map: &GameMap, x: i32, y: i32) -> bool {
    map.get(x, y)
        .is_some_and(|t| map.is_land(x, y) && t.relief == Relief::Flat && t.cover != Cover::Bare)
}

/// Irrigation needs flat, clear, farmable land plus water access: an
/// adjacent water tile or an adjacent irrigated tile (chains). It replaces
/// a mine.
pub fn can_irrigate(map: &GameMap, x: i32, y: i32) -> bool {
    let Some(t) = map.get(x, y) else {
        return false;
    };
    if !matches!(
        t.base,
        Base::Grassland | Base::Plains | Base::Desert | Base::Tundra
    ) || t.relief != Relief::Flat
        || t.cover != Cover::Bare
        || t.irrigation
    {
        return false;
    }
    map.neighbors(x, y).iter().any(|(nx, ny)| {
        map.get(*nx, *ny)
            .is_some_and(|nb| is_water_base(nb.base) || nb.irrigation)
    })
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
    }
}

/// One turn of labor. Workers sharing a tile and job pool it: the group
/// continues from its most advanced member and gains one turn per worker.
/// Returns the jobs that finished, once per (tile, action).
pub fn advance_work(units: &mut [Mut<Unit>]) -> Vec<(i32, i32, WorkAction)> {
    let mut groups: HashMap<(i32, i32, WorkAction), (u8, u8)> = HashMap::new();
    for u in units.iter() {
        if let Some(w) = u.work {
            let g = groups.entry((u.x, u.y, w.action)).or_insert((u8::MAX, 0));
            g.0 = g.0.min(w.turns_left);
            g.1 += 1;
        }
    }
    let mut done = vec![];
    for u in units.iter_mut() {
        let Some(w) = u.work else { continue };
        // Working units never bank moves.
        u.moves = 0;
        let key = (u.x, u.y, w.action);
        let (left, workers) = groups[&key];
        let left = left.saturating_sub(workers);
        if left > 0 {
            u.work = Some(Work {
                turns_left: left,
                ..w
            });
            continue;
        }
        u.work = None;
        u.anim = UnitAnim::OneShot {
            slot: action_slot(w.action),
            t: 0.0,
        };
        if !done.contains(&key) {
            done.push(key);
        }
    }
    done
}

/// Advance worker jobs on end turn; complete and apply to the map.
pub fn end_turn_work(
    mut commands: Commands,
    mut end: MessageReader<crate::civs::CivilizationEnded>,
    mut map: ResMut<GameMap>,
    mut units: Query<&mut Unit>,
    mut board: ResMut<MessageBoard>,
    audio: Res<GameAudio>,
) {
    for event in end.read() {
        let mut all: Vec<Mut<Unit>> = units.iter_mut().filter(|u| u.civ == event.0).collect();
        for (x, y, action) in advance_work(&mut all) {
            let i = map.idx(x, y);
            apply_work(&mut map.tiles[i], action);
            commands.spawn(AudioPlayer(audio.work_sfx(action)));
            post(
                &mut board,
                format!("Workers complete {} ({x},{y}).", action_name(action)),
            );
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

#[derive(Resource)]
pub struct ImprovementArt {
    defs: HashMap<String, (Handle<Image>, Anchor)>,
}

impl ImprovementArt {
    pub fn load(asset_server: &AssetServer) -> Self {
        let text = std::fs::read_to_string("assets/gen/improvements/manifest.json")
            .expect("run from the repo root after tools/prep_assets.py improvements");
        let raw: HashMap<String, ImpEntry> =
            serde_json::from_str(&text).expect("improvements manifest parses");
        let mut defs = HashMap::new();
        for (name, e) in raw {
            let anchor = Anchor(Vec2::new(
                e.anchor[0] as f32 / e.size[0] as f32 - 0.5,
                0.5 - e.anchor[1] as f32 / e.size[1] as f32,
            ));
            defs.insert(
                name.clone(),
                (
                    asset_server.load(format!("gen/improvements/{}", e.file)),
                    anchor,
                ),
            );
        }
        Self { defs }
    }
}

#[derive(Component, Clone, Copy, PartialEq, Eq, Hash)]
enum ImpLayer {
    Road,
    Irrigation,
    Mine,
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
    }
}

impl ImpLayer {
    const ALL: [ImpLayer; 3] = [ImpLayer::Irrigation, ImpLayer::Road, ImpLayer::Mine];

    fn on(self, t: &Tile) -> bool {
        match self {
            ImpLayer::Road => t.road,
            ImpLayer::Irrigation => t.irrigation,
            ImpLayer::Mine => t.mine,
        }
    }

    fn mask(self, map: &GameMap, hubs: &[(i32, i32)], x: i32, y: i32) -> u8 {
        match self {
            ImpLayer::Road => road_mask(map, hubs, x, y),
            ImpLayer::Irrigation => irr_mask(map, x, y),
            ImpLayer::Mine => 0,
        }
    }

    /// Draw order above the base tile: irrigation, roads, then the mine.
    fn z(self) -> f32 {
        match self {
            ImpLayer::Irrigation => 1.5,
            ImpLayer::Road => 1.6,
            ImpLayer::Mine => 1.7,
        }
    }
}

/// Improvement overlays for one tile, bottom to top, as (image, anchor
/// in pixels from the 128x64 cell's top-left). Shared by the city screen.
pub fn tile_overlays(
    map: &GameMap,
    art: &ImprovementArt,
    hubs: &[(i32, i32)],
    x: i32,
    y: i32,
) -> Vec<Handle<Image>> {
    let Some(t) = map.get(x, y) else {
        return vec![];
    };
    ImpLayer::ALL
        .iter()
        .filter(|l| l.on(t))
        .filter_map(|l| art.defs.get(&sprite_key(t, *l, l.mask(map, hubs, x, y))))
        .map(|(image, _)| image.clone())
        .collect()
}

/// Spawn overlays for new improvements, refresh masks, tint by fog.
pub fn sync_improvement_sprites(
    mut commands: Commands,
    map: Res<GameMap>,
    art: Res<ImprovementArt>,
    reveal: Res<RevealAll>,
    cities: Query<&City>,
    mut q: Query<(Entity, &mut ImprovementSprite, &mut Sprite, &mut Visibility)>,
) {
    let hubs: Vec<(i32, i32)> = cities.iter().map(|c| (c.x, c.y)).collect();
    let mut have: HashSet<(i32, i32, ImpLayer)> = HashSet::new();
    for (e, mut fs, mut sprite, mut vis) in q.iter_mut() {
        let t = &map.tiles[map.idx(fs.x, fs.y)];
        // A mine replaces irrigation and vice versa: drop stale overlays.
        if !fs.layer.on(t) {
            commands.entity(e).despawn();
            continue;
        }
        have.insert((fs.x, fs.y, fs.layer));
        let mask = fs.layer.mask(&map, &hubs, fs.x, fs.y);
        if mask != fs.mask {
            // Neighbor change: swap to the new mask tile.
            let key = sprite_key(t, fs.layer, mask);
            sprite.image = art.defs[&key].0.clone();
            fs.mask = mask;
        }
        // Improvements sit below the fog diamonds, which do the dimming;
        // unseen ones hide so nothing spills past the fog edge.
        sprite.color = Color::WHITE;
        *vis = match fog_for(reveal.0, t) {
            Fog::Black => Visibility::Hidden,
            _ => Visibility::Visible,
        };
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
                let mask = layer.mask(&map, &hubs, x, y);
                let (image, anchor) = art.defs[&sprite_key(t, layer, mask)].clone();
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

    #[test]
    fn irrigation_rules_need_farmable_land_and_water() {
        let map = GameMap::generate();
        // A farmable, water-adjacent tile exists and irrigates.
        let good = (0..map.h)
            .flat_map(|y| (0..map.w).map(move |x| (x, y)))
            .find(|(x, y)| {
                map.get(*x, *y).is_some_and(|t| {
                    matches!(
                        t.base,
                        Base::Grassland | Base::Plains | Base::Desert | Base::Tundra
                    ) && t.relief == Relief::Flat
                        && t.cover == Cover::Bare
                        && map.neighbors(*x, *y).iter().any(|(nx, ny)| {
                            map.get(*nx, *ny).is_some_and(|nb| is_water_base(nb.base))
                        })
                })
            })
            .expect("map has irrigable land");
        assert!(can_irrigate(&map, good.0, good.1));
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
            assert!(!can_irrigate(&map, x, y));
        }
        // Ocean never irrigates.
        let ocean = (0..map.h)
            .flat_map(|y| (0..map.w).map(move |x| (x, y)))
            .find(|(x, y)| matches!(map.get(*x, *y).map(|t| t.base), Some(Base::Ocean)))
            .expect("map has ocean");
        assert!(!can_irrigate(&map, ocean.0, ocean.1));
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
                    matches!(
                        t.base,
                        Base::Grassland | Base::Plains | Base::Desert | Base::Tundra
                    ) && t.relief == Relief::Flat
                        && t.cover == Cover::Bare
                        && !map.neighbors(*x, *y).iter().any(|(nx, ny)| {
                            map.get(*nx, *ny).is_some_and(|nb| is_water_base(nb.base))
                        })
                })
            })
            .expect("map has inland farmable land");
        assert!(!can_irrigate(&map, inland.0, inland.1));
        let (nx, ny) = map.neighbors(inland.0, inland.1)[0];
        let i = map.idx(nx, ny);
        map.tiles[i].irrigation = true;
        assert!(can_irrigate(&map, inland.0, inland.1));
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
                    if can_irrigate(&map, x, y) && map.find_path((sx, sy), (x, y)).is_some() {
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
            fortify: Handle::default(),
            work_road: Handle::default(),
            work_irrigate: Handle::default(),
            work_mine: Handle::default(),
            work_clear: Handle::default(),
            music: None,
        });
        app.add_message::<crate::civs::CivilizationEnded>();
        app.world_mut().spawn(Unit {
            civ: 0,
            utype: crate::units::UnitType::Worker,
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
                turns_left: 1,
            }),
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
            fortify: Handle::default(),
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

    fn worker(x: i32, y: i32, action: WorkAction, turns_left: u8) -> Unit {
        Unit {
            civ: 0,
            utype: crate::units::UnitType::Worker,
            x,
            y,
            moves: 3,
            fortified: false,
            facing: 0,
            path: Default::default(),
            anim: UnitAnim::Idle { t: 0.0 },
            sentry: false,
            exploring: false,
            work: Some(Work { action, turns_left }),
        }
    }

    fn end_turn(app: &mut App) {
        app.world_mut()
            .resource_mut::<Messages<crate::civs::CivilizationEnded>>()
            .write(crate::civs::CivilizationEnded(0));
        app.update();
    }

    #[test]
    fn workers_on_one_tile_pool_labor() {
        let map = GameMap::generate();
        let (sx, sy) = map.start;
        let mut app = test_app(map);
        let n = work_turns(WorkAction::Road);
        let a = app
            .world_mut()
            .spawn(worker(sx, sy, WorkAction::Road, n))
            .id();
        let b = app
            .world_mut()
            .spawn(worker(sx, sy, WorkAction::Road, n))
            .id();
        // a lone worker elsewhere is not helped
        let c = app
            .world_mut()
            .spawn(worker(sx + 1, sy, WorkAction::Road, n))
            .id();
        let mut foreign_worker = worker(sx, sy, WorkAction::Road, n);
        foreign_worker.civ = 1;
        let foreign = app.world_mut().spawn(foreign_worker).id();
        end_turn(&mut app);
        let left = |app: &App, e| {
            app.world()
                .get::<Unit>(e)
                .unwrap()
                .work
                .map(|w| w.turns_left)
        };
        assert_eq!(left(&app, a), Some(n - 2));
        assert_eq!(left(&app, b), Some(n - 2));
        assert_eq!(left(&app, c), Some(n - 1));
        assert_eq!(
            left(&app, foreign),
            Some(n),
            "other civ workers neither contribute nor progress"
        );
        end_turn(&mut app);
        assert_eq!(left(&app, a), None, "two workers halve a 4-turn road");
        assert_eq!(left(&app, b), None);
        let map = app.world().resource::<GameMap>();
        assert!(map.tiles[map.idx(sx, sy)].road);
        assert!(!map.tiles[map.idx(sx + 1, sy)].road);
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
        assert!(can_clear(&map, wood.0, wood.1));
        assert!(
            !can_clear(&map, map.start.0, map.start.1)
                || map.get(map.start.0, map.start.1).unwrap().cover != Cover::Bare
        );
    }
}
