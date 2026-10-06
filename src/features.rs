//! Tile features: goody huts, barbarian camps, and resources.
//!
//! Placement follows the reverse-engineered mapgen stages (`0x5f22a0`,
//! `0x5f21b0`, `0x5f2090`; see `reverse-engineering/NOTES.md` §11.8-11.10):
//! exact stage seeds, the exact quantity math, Fisher-Yates shuffles, the
//! 1-in-3 camp gate, and block-3 acceptance odds. Documented deviations:
//! - The binary's hut stage is a no-op in normal games (`>= 32` guard) and
//!   the camp stage scans a tiny prefix of cells; we implement the evident
//!   intent (24 huts, up to 8 camps) over the whole map.
//! - Our map has no half-resolution cell grid, so hut picks decode over
//!   W*H tiles instead of `(W/2)*H` cells.
//! - `vfunc(0x48)` ("may hold bonus") is unrecovered; suitability is
//!   land + passable + empty + off the start tile.
//! - Camps keep a playability distance from the start (7) and each other
//!   (4); the binary has no such gates.
//! - GOOD frequencies and the TERR allow-matrix are hardcoded (Civ3
//!   knowledge + sheet order as canonical GOOD ids) until BIQ framing
//!   lands; the class predicates' UI meaning is unknown, so bonus
//!   resources cluster (block 1) while luxury/strategic scatter (block 2
//!   with block-3 acceptance odds).
//! - The same-region invariant waits on continent ids (exact-mapgen slice).

use std::collections::HashSet;

use crate::map::{Base, Cover, GameMap, Relief, Tile, move_cost};
use crate::rng::MapRng;

/// Stage seeds from the binary (`water_level + K`).
pub const RESOURCE_SEED: u32 = 0x180E3;
pub const HUT_SEED: u32 = 0x8ACE;
pub const CAMP_SEED: u32 = 0x8CF78;

pub const N_HUTS: u32 = 24;
pub const N_CAMPS: usize = 8;
/// Min Chebyshev distance from the start for camps (playability).
pub const CAMP_START_DIST: i32 = 7;
/// Min Chebyshev distance between camps (playability).
pub const CAMP_SEPARATION: i32 = 4;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GoodKind {
    Bonus,
    Luxury,
    Strategic,
}

/// Canonical GOOD table in `Art/resources.pcx` sheet order (ids 0..22).
/// `freq` is `GOOD+0x40` (0 = roll `rand(26)+rand(26)+50`); `bonus` is the
/// worked-tile (food, shields) delta. Luxury/strategic goods give no yield
/// here, as in Civ3 pre-improvement; they matter for the trade slice.
pub struct Good {
    pub name: &'static str,
    /// Manifest slug (`res_<art>`); matches the civilopedia filenames, which
    /// differ from display names in places (saltpetre, dye, silk, spice).
    pub art: &'static str,
    pub kind: GoodKind,
    pub freq: u8,
    pub bonus: (u8, u8),
}

pub const GOODS: [Good; 22] = [
    Good {
        name: "Horses",
        art: "horse",
        kind: GoodKind::Strategic,
        freq: 80,
        bonus: (0, 0),
    },
    Good {
        name: "Diamonds",
        art: "diamonds",
        kind: GoodKind::Luxury,
        freq: 80,
        bonus: (0, 0),
    },
    Good {
        name: "Saltpeter",
        art: "saltpetre",
        kind: GoodKind::Strategic,
        freq: 60,
        bonus: (0, 0),
    },
    Good {
        name: "Coal",
        art: "coal",
        kind: GoodKind::Strategic,
        freq: 60,
        bonus: (0, 0),
    },
    Good {
        name: "Oil",
        art: "oil",
        kind: GoodKind::Strategic,
        freq: 50,
        bonus: (0, 0),
    },
    Good {
        name: "Iron",
        art: "iron",
        kind: GoodKind::Strategic,
        freq: 80,
        bonus: (0, 0),
    },
    Good {
        name: "Aluminum",
        art: "aluminum",
        kind: GoodKind::Strategic,
        freq: 40,
        bonus: (0, 0),
    },
    Good {
        name: "Uranium",
        art: "uranium",
        kind: GoodKind::Strategic,
        freq: 35,
        bonus: (0, 0),
    },
    Good {
        name: "Wine",
        art: "wine",
        kind: GoodKind::Luxury,
        freq: 70,
        bonus: (0, 0),
    },
    Good {
        name: "Furs",
        art: "furs",
        kind: GoodKind::Luxury,
        freq: 60,
        bonus: (0, 0),
    },
    Good {
        name: "Dyes",
        art: "dye",
        kind: GoodKind::Luxury,
        freq: 70,
        bonus: (0, 0),
    },
    Good {
        name: "Incense",
        art: "incense",
        kind: GoodKind::Luxury,
        freq: 60,
        bonus: (0, 0),
    },
    Good {
        name: "Spices",
        art: "spice",
        kind: GoodKind::Luxury,
        freq: 60,
        bonus: (0, 0),
    },
    Good {
        name: "Ivory",
        art: "ivory",
        kind: GoodKind::Luxury,
        freq: 60,
        bonus: (0, 0),
    },
    Good {
        name: "Silks",
        art: "silk",
        kind: GoodKind::Luxury,
        freq: 70,
        bonus: (0, 0),
    },
    Good {
        name: "Rubber",
        art: "rubber",
        kind: GoodKind::Strategic,
        freq: 50,
        bonus: (0, 0),
    },
    Good {
        name: "Whales",
        art: "whales",
        kind: GoodKind::Bonus,
        freq: 60,
        bonus: (1, 1),
    },
    Good {
        name: "Game",
        art: "game",
        kind: GoodKind::Bonus,
        freq: 90,
        bonus: (1, 0),
    },
    Good {
        name: "Fish",
        art: "fish",
        kind: GoodKind::Bonus,
        freq: 100,
        bonus: (2, 0),
    },
    Good {
        name: "Cattle",
        art: "cattle",
        kind: GoodKind::Bonus,
        freq: 100,
        bonus: (1, 0),
    },
    Good {
        name: "Wheat",
        art: "wheat",
        kind: GoodKind::Bonus,
        freq: 120,
        bonus: (2, 0),
    },
    Good {
        name: "Gold",
        art: "gold",
        kind: GoodKind::Luxury,
        freq: 60,
        bonus: (0, 0),
    },
];

/// Art manifest key for a good id.
pub fn art_key(id: u8) -> String {
    format!("res_{}", GOODS[id as usize].art)
}

/// Hardcoded TERR allow-matrix: may good `id` appear on this tile?
pub fn suitable(id: u8, t: &Tile) -> bool {
    use Base::*;
    let land = matches!(t.base, Grassland | Plains | Desert | Tundra);
    let flat = t.relief == Relief::Flat;
    let hill = t.relief != Relief::Flat;
    let gp = matches!(t.base, Grassland | Plains);
    match id {
        0 => gp && flat && t.cover == Cover::Bare, // horses
        1 => t.base == Desert && flat || hill || t.cover == Cover::Jungle, // diamonds
        2 => matches!(t.base, Desert | Plains | Grassland | Tundra) && flat || hill, // saltpeter
        3 => hill || gp && flat && t.cover == Cover::Bare, // coal
        4 => t.base == Base::Sea || matches!(t.base, Desert | Tundra) && flat, // oil
        5 => land && (hill || flat),               // iron
        6 => matches!(t.base, Desert | Tundra) && flat || hill, // aluminum
        7 => {
            t.relief == Relief::Mountain
                || t.relief == Relief::Hill
                || matches!(t.base, Desert | Tundra) && flat
        } // uranium
        8 => gp && flat || hill && land,           // wine
        9 => t.base == Tundra && flat || t.cover == Cover::Forest || t.cover == Cover::Pine, // furs
        10 => gp && flat || t.cover == Cover::Forest || t.cover == Cover::Jungle, // dyes
        11 => matches!(t.base, Desert | Plains) && flat && t.cover == Cover::Bare, // incense
        12 => t.cover == Cover::Jungle || t.base == Grassland && flat, // spices
        13 => matches!(t.base, Grassland | Plains | Desert) && flat && t.cover == Cover::Bare, // ivory
        14 => gp && flat && t.cover == Cover::Bare, // silks
        15 => t.cover == Cover::Jungle || t.cover == Cover::Forest || t.base == Grassland && flat, // rubber
        16 => matches!(t.base, Base::Sea | Base::Ocean), // whales
        17 => t.cover == Cover::Forest || matches!(t.base, Grassland | Plains | Tundra) && flat, // game
        18 => matches!(t.base, Base::Coast | Base::Sea), // fish
        19 | 20 => gp && flat && t.cover == Cover::Bare, // cattle, wheat
        21 => t.base == Desert && flat || hill,          // gold
        _ => false,
    }
}

/// TERR-row score for the quantity math. Our rows mirror the BIQ shape:
/// 12 land rows (4 bases x flat/hill/mountain) plus 3 water rows
/// (Ocean/Sea/Coast) worth +4 each, mirroring the binary's `t >= 11`
/// water bonus. Probed with bare cover.
pub fn terr_score(id: u8) -> u32 {
    let mut score = 0;
    for base in [Base::Grassland, Base::Plains, Base::Desert, Base::Tundra] {
        for relief in [Relief::Flat, Relief::Hill, Relief::Mountain] {
            let t = Tile {
                base,
                relief,
                cover: Cover::Bare,
                variant: 0,
                seen: false,
                visible: false,
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
            if suitable(id, &t) {
                score += 1;
            }
        }
    }
    for base in [Base::Ocean, Base::Sea, Base::Coast] {
        let t = Tile {
            base,
            relief: Relief::Flat,
            cover: Cover::Bare,
            variant: 0,
            seen: false,
            visible: false,
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
        if suitable(id, &t) {
            score += 1 + 4;
        }
    }
    score
}

/// Exact quantity math from `0x5f22a0` (NOTES §11.8). `area_factor` is the
/// binary's `this->[0x15C]`; we pass `max(1, land_tiles / 800)`.
/// `0.5f`/`0.75f` truncation equals `/2` and `*3/4` for non-negative ints.
pub fn quantity(area_factor: u32, freq: u8, score: u32, rng: &mut MapRng) -> u32 {
    let pct = if freq != 0 {
        freq as u32
    } else {
        (rng.below(26) + rng.below(26) + 50) as u32
    };
    let n1 = (area_factor * pct) / 32;
    let n = if score < 2 {
        n1 / 2
    } else if score < 4 {
        (n1 * 3) / 4
    } else {
        n1
    };
    n.max(if score >= 4 { 2 } else { 1 })
}

fn is_empty(t: &Tile) -> bool {
    !t.hut && !t.camp && t.resource.is_none()
}

fn suitable_tiles(map: &GameMap, id: u8) -> Vec<(i32, i32)> {
    let mut out = vec![];
    for y in 0..map.h {
        for x in 0..map.w {
            let t = &map.tiles[map.idx(x, y)];
            if is_empty(t) && suitable(id, t) {
                out.push((x, y));
            }
        }
    }
    out
}

/// Block 1: a cluster of `n` copies spreading over the 8-neighborhood,
/// refusing a candidate whose neighborhood already holds >= 3 of same.
fn place_cluster(map: &mut GameMap, id: u8, n: u32, rng: &mut MapRng) {
    let cands = suitable_tiles(map, id);
    if cands.is_empty() {
        return;
    }
    let mut frontier = vec![cands[rng.below(cands.len() as u32) as usize]];
    let mut seen = HashSet::new();
    let mut placed = 0;
    while placed < n && !frontier.is_empty() {
        let i = rng.below(frontier.len() as u32) as usize;
        let (x, y) = frontier.swap_remove(i);
        if !seen.insert((x, y)) {
            continue;
        }
        let ok = map
            .get(x, y)
            .is_some_and(|t| is_empty(t) && suitable(id, t));
        if ok {
            let same = map
                .neighbors(x, y)
                .iter()
                .filter(|(nx, ny)| map.get(*nx, *ny).is_some_and(|nb| nb.resource == Some(id)))
                .count();
            if same < 3 {
                let i = map.idx(x, y);
                map.tiles[i].resource = Some(id);
                placed += 1;
            }
        }
        for nb in map.neighbors(x, y) {
            if !seen.contains(&nb) {
                frontier.push(nb);
            }
        }
    }
}

/// Block 2/3: scattered singles over a Fisher-Yates shuffle with the
/// block-3 acceptance coin (33 % / 50 % / 100 % by score).
fn place_scattered(map: &mut GameMap, id: u8, n: u32, score: u32, rng: &mut MapRng) {
    let mut cands = suitable_tiles(map, id);
    rng.shuffle(&mut cands);
    let m = if score < 2 {
        6
    } else if score < 4 {
        4
    } else {
        2
    };
    let mut placed = 0;
    for (x, y) in cands {
        if placed >= n {
            break;
        }
        // `> 1` is the SKIP arm: values {0,1} place, giving the binary's
        // 33 % / 50 % / 100 % acceptance.
        if rng.below(m) > 1 {
            continue;
        }
        let i = map.idx(x, y);
        map.tiles[i].resource = Some(id);
        placed += 1;
    }
}

/// Stage 10 (`0x5f22a0`): resources for every GOOD id in order.
pub fn place_resources(map: &mut GameMap, water_level: u32) {
    let land = map
        .tiles
        .iter()
        .filter(|t| {
            matches!(
                t.base,
                Base::Grassland | Base::Plains | Base::Desert | Base::Tundra
            )
        })
        .count() as u32;
    let area_factor = (land / 800).max(1);
    let mut rng = MapRng::new(water_level.wrapping_add(RESOURCE_SEED));
    for (id, good) in GOODS.iter().enumerate() {
        let id = id as u8;
        let score = terr_score(id);
        if score == 0 {
            continue;
        }
        let n = quantity(area_factor, good.freq, score, &mut rng);
        if n == 0 {
            continue;
        }
        match good.kind {
            GoodKind::Bonus => place_cluster(map, id, n, &mut rng),
            GoodKind::Luxury | GoodKind::Strategic => place_scattered(map, id, n, score, &mut rng),
        }
    }
}

/// Stage 11 (`0x5f21b0`): goody huts. Random picks with replacement over
/// all tiles; the suitability gate stands in for `vfunc(0x48)`.
pub fn place_goody_huts(map: &mut GameMap, water_level: u32) {
    let mut rng = MapRng::new(water_level.wrapping_add(HUT_SEED));
    let total = (map.w * map.h) as u32;
    for _ in 0..N_HUTS {
        let r = rng.below(total) as i32;
        let (x, y) = (r % map.w, r / map.w);
        let ok = map.get(x, y).is_some_and(|t| {
            map.is_land(x, y) && move_cost(t).is_some() && is_empty(t) && (x, y) != map.start
        });
        if ok {
            let i = map.idx(x, y);
            map.tiles[i].hut = true;
        }
    }
}

/// Stage 12 (`0x5f2090`): barbarian camps. Fisher-Yates over eligible
/// land, land + no-feature + 1-in-3 gates.
pub fn place_barbarian_camps(map: &mut GameMap, water_level: u32) {
    let mut rng = MapRng::new(water_level.wrapping_add(CAMP_SEED));
    let (sx, sy) = map.start;
    let mut cands: Vec<(i32, i32)> = vec![];
    for y in 0..map.h {
        for x in 0..map.w {
            let far = (x - sx).abs().max((y - sy).abs()) >= CAMP_START_DIST;
            let ok = map
                .get(x, y)
                .is_some_and(|t| map.is_land(x, y) && move_cost(t).is_some() && is_empty(t));
            if far && ok {
                cands.push((x, y));
            }
        }
    }
    rng.shuffle(&mut cands);
    let mut placed: Vec<(i32, i32)> = vec![];
    for (x, y) in cands {
        if placed.len() >= N_CAMPS {
            break;
        }
        if rng.below(3) != 0 {
            continue;
        }
        if placed
            .iter()
            .any(|(px, py)| (x - px).abs().max((y - py).abs()) < CAMP_SEPARATION)
        {
            continue;
        }
        let i = map.idx(x, y);
        map.tiles[i].camp = true;
        placed.push((x, y));
    }
}

/// One-line hover description for a tile's feature, if any.
pub fn describe(t: &Tile) -> Option<String> {
    if t.hut {
        return Some("Goody hut".to_string());
    }
    if t.camp {
        return Some("Barbarian camp".to_string());
    }
    t.resource.map(|id| GOODS[id as usize].name.to_string())
}

// --- Gameplay ---

/// Transient HUD message.
#[derive(bevy::prelude::Resource, Default)]
pub struct MessageBoard {
    pub text: String,
    pub ttl: f32,
}

pub fn post(board: &mut MessageBoard, msg: impl Into<String>) {
    board.text = msg.into();
    board.ttl = 6.0;
}

#[derive(bevy::prelude::Component, Clone, Copy, PartialEq, Eq)]
pub enum FeatureKind {
    Hut,
    Camp,
    Resource(u8),
}

#[derive(bevy::prelude::Component)]
pub struct FeatureSprite {
    pub x: i32,
    y: i32,
    kind: FeatureKind,
}

fn tile_kind(t: &Tile) -> Option<FeatureKind> {
    if t.hut {
        Some(FeatureKind::Hut)
    } else if t.camp {
        Some(FeatureKind::Camp)
    } else {
        t.resource.map(FeatureKind::Resource)
    }
}

// --- Feature art ---

#[derive(serde::Deserialize)]
struct FeatureEntry {
    file: String,
    size: [u32; 2],
    anchor: [i32; 2],
}

#[derive(bevy::prelude::Resource)]
pub struct FeatureArt {
    defs: std::collections::HashMap<
        String,
        (
            bevy::prelude::Handle<bevy::prelude::Image>,
            bevy::sprite::Anchor,
        ),
    >,
}

impl FeatureArt {
    pub fn load(asset_server: &bevy::prelude::AssetServer) -> Self {
        let text = crate::web::read_text("assets/cache/features/manifest.json")
            .expect("run from the repo root: the art cache (assets/cache) is built at startup");
        let raw: std::collections::HashMap<String, FeatureEntry> =
            serde_json::from_str(&text).expect("features manifest parses");
        let mut defs = std::collections::HashMap::new();
        for (name, e) in raw {
            let anchor = bevy::sprite::Anchor(bevy::prelude::Vec2::new(
                e.anchor[0] as f32 / e.size[0] as f32 - 0.5,
                0.5 - e.anchor[1] as f32 / e.size[1] as f32,
            ));
            defs.insert(
                name.clone(),
                (
                    asset_server.load(format!("cache/features/{}", e.file)),
                    anchor,
                ),
            );
        }
        Self { defs }
    }

    fn get(
        &self,
        name: &str,
    ) -> &(
        bevy::prelude::Handle<bevy::prelude::Image>,
        bevy::sprite::Anchor,
    ) {
        &self.defs[name]
    }
}

/// Deterministic hut variant from coords (8 hut sprites).
fn hut_variant(x: i32, y: i32) -> u8 {
    ((x as i64 * 7 + y as i64 * 13).rem_euclid(8)) as u8
}

pub fn spawn_features(
    mut commands: bevy::prelude::Commands,
    map: bevy::prelude::Res<GameMap>,
    art: bevy::prelude::Res<FeatureArt>,
) {
    use crate::map::tile_to_world;
    use crate::render::feature_z;
    for y in 0..map.h {
        for x in 0..map.w {
            let t = &map.tiles[map.idx(x, y)];
            let (key, layer, kind) = match tile_kind(t) {
                Some(FeatureKind::Hut) => {
                    (format!("hut_{}", hut_variant(x, y)), 2.0, FeatureKind::Hut)
                }
                Some(FeatureKind::Camp) => ("camp".to_string(), 2.0, FeatureKind::Camp),
                Some(FeatureKind::Resource(id)) => (art_key(id), 1.0, FeatureKind::Resource(id)),
                None => continue,
            };
            let (image, anchor) = art.get(&key).clone();
            let pos = tile_to_world(x, y);
            commands.spawn((
                bevy::prelude::Sprite {
                    image,
                    ..Default::default()
                },
                anchor,
                bevy::prelude::Transform::from_xyz(pos.x, pos.y, feature_z(x, y, layer)),
                FeatureSprite { x, y, kind },
            ));
        }
    }
}

/// Despawn overlays whose tile feature changed; show or hide the rest by
/// fog. The art sits under the fog diamonds (`render::feature_z`), which
/// dim it with its tile, so a resource on the frontier fades into the
/// black with the terrain around it instead of showing as a black blot.
pub fn sync_feature_sprites(
    mut commands: bevy::prelude::Commands,
    map: bevy::prelude::Res<GameMap>,
    reveal: bevy::prelude::Res<crate::render::RevealAll>,
    mut q: bevy::prelude::Query<(
        bevy::prelude::Entity,
        &FeatureSprite,
        &mut bevy::prelude::Visibility,
    )>,
) {
    use bevy::prelude::Visibility;
    let mut dead = vec![];
    for (e, fs, mut vis) in q.iter_mut() {
        let t = &map.tiles[map.idx(fs.x, fs.y)];
        if tile_kind(t) != Some(fs.kind) {
            dead.push(e);
            continue;
        }
        // Hidden only under a solid black diamond, so art poking past its
        // tile never shows over unexplored neighbors.
        let lit = crate::render::fog_cell(&map, reveal.0, fs.x, fs.y) != (0, 0);
        *vis = if lit {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    for e in dead {
        commands.entity(e).despawn();
    }
}

/// Pop huts and capture camps under idle units. Runs after `advance_anims`
/// so rewards trigger on arrival, not on step start.
pub fn resolve_features(
    mut commands: bevy::prelude::Commands,
    civs: bevy::prelude::Res<crate::civs::Civilizations>,
    mut map: bevy::prelude::ResMut<GameMap>,
    units: bevy::prelude::Query<&crate::units::Unit>,
    art: bevy::prelude::Res<crate::units::UnitArt>,
    cities: bevy::prelude::Query<(bevy::prelude::Entity, &mut crate::cities::City)>,
    mut board: bevy::prelude::ResMut<MessageBoard>,
    audio: bevy::prelude::Res<crate::audio::GameAudio>,
    mut research: bevy::prelude::ResMut<crate::research::Research>,
    mut dice: bevy::prelude::ResMut<crate::combat::CombatRng>,
    mut treasury: bevy::prelude::ResMut<crate::cities::Treasury>,
    mut barbarians: bevy::prelude::ResMut<crate::barbarians::Barbarians>,
    turn: bevy::prelude::Res<crate::units::Turn>,
) {
    use crate::units::UnitAnim;
    // The computer pops huts too, silently and without lifting the human's
    // fog: its announcements are dropped and its map reveal changes nothing.
    let quiet = crate::civs::is_ai(civs.active).then(|| (board.text.clone(), board.ttl));
    // Collect arrivals first; rewards mutate the map.
    let mut arrivals: Vec<(i32, i32)> = vec![];
    for u in units.iter() {
        if u.civ != civs.active || !matches!(u.anim, UnitAnim::Idle { .. }) {
            continue;
        }
        arrivals.push((u.x, u.y));
    }
    for (x, y) in arrivals {
        let i = map.idx(x, y);
        if map.tiles[i].hut {
            // `0x55C6B0`: the hut is used up before anything is decided.
            map.tiles[i].hut = false;
            let all_cities: Vec<&crate::cities::City> = cities.iter().map(|(_, c)| c).collect();
            let all_units: Vec<&crate::units::Unit> = units.iter().collect();
            let walker = units.iter().find(|u| u.civ == civs.active && (u.x, u.y) == (x, y));
            let civ = civs.active;
            let era = research.era(civ);
            let used = |t: usize| barbarians.tribe_used(t);
            let build = |q: usize, t: crate::units::UnitType| crate::research::can_build(q, crate::cities::Production::from_unit(t));
            let mut advance = |d: &mut crate::rng::MapRng| research.hut_advance(civ, d).map(|(t, _)| t);
            let snapshot = map.clone();
            let mut ctx = crate::huts::Context {
                map: &snapshot,
                civ,
                tile: (x, y),
                unit: walker,
                cities: &all_cities,
                units: &all_units,
                round: turn.0.saturating_sub(1),
                era,
                difficulty: crate::scenario::difficulty() as i32,
                players: crate::civs::civ_count() as i32,
                used_tribe: &used,
                can_build: &build,
                advance: &mut advance,
            };
            let got = crate::huts::pop(&mut ctx, &mut dice.0);
            treasury.0[civ] = treasury.0[civ].saturating_add(got.gold);
            for &(tx, ty) in &got.reveal {
                // The computer's own map is not modelled; only the human's
                // fog lifts.
                if quiet.is_none() {
                    let j = map.idx(tx, ty);
                    map.tiles[j].seen = true;
                }
            }
            for &(t, conscript) in &got.units {
                let level = if conscript { crate::combat::Level::Conscript } else { crate::combat::Level::Regular };
                crate::units::spawn_unit_at_level(&mut commands, &art, t, x, y, civ, level);
            }
            for &(tx, ty) in &got.barbarians {
                let e = crate::units::spawn_unit_at_level(
                    &mut commands,
                    &art,
                    crate::roles::barbarian_basic(),
                    tx,
                    ty,
                    crate::civs::BARBARIANS,
                    crate::combat::Level::Conscript,
                );
                commands.entity(e).insert(crate::barbarians::Tribe(got.tribe));
            }
            post(&mut board, crate::huts::message(&got, civ));
            if quiet.is_none() {
                crate::audio::sfx(&mut commands, &audio, "Hut");
            }
        } else if map.tiles[i].camp {
            // `0x565A00` (`barbarians.md` 8): a flat 25 gold, and the
            // tribe's name is free again.
            map.tiles[i].camp = false;
            let tribe = barbarians.disperse((x, y));
            treasury.0[civs.active] = treasury.0[civs.active].saturating_add(25);
            post(
                &mut board,
                format!(
                    "We dispersed a {} encampment and took 25 gold!",
                    crate::barbarians::TRIBES[tribe as usize]
                ),
            );
            if quiet.is_none() {
                crate::audio::sfx(&mut commands, &audio, "Barbarian Raid");
            }
        }
    }
    if let Some((text, ttl)) = quiet {
        board.text = text;
        board.ttl = ttl;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{Base, Cover, Relief};

    fn tile(base: Base, relief: Relief, cover: Cover) -> Tile {
        Tile {
            base,
            relief,
            cover,
            variant: 0,
            seen: false,
            visible: false,
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
        }
    }

    #[test]
    fn quantity_math_matches_the_binary() {
        let mut rng = MapRng::new(1);
        // n1 = (8*100)/32 = 25
        assert_eq!(quantity(8, 100, 5, &mut rng), 25);
        assert_eq!(quantity(8, 100, 3, &mut rng), 18); // 25*3/4
        assert_eq!(quantity(8, 100, 1, &mut rng), 12); // 25/2
        // floors: n1 = 0
        assert_eq!(quantity(1, 8, 5, &mut rng), 2);
        assert_eq!(quantity(1, 8, 1, &mut rng), 1);
    }

    #[test]
    fn zero_freq_rolls_and_consumes_draws() {
        let a = quantity(4, 0, 9, &mut MapRng::new(7));
        let mut r = MapRng::new(7);
        let pct = (r.below(26) + r.below(26) + 50) as u32;
        assert_eq!(a, (4 * pct) / 32);
    }

    #[test]
    fn block3_acceptance_odds() {
        // `> 1` skips, so {0,1} place: 2/6, 2/4, 2/2 per NOTES §11.8.
        for (m, lo, hi) in [(6, 1900, 2100), (4, 2900, 3100), (2, 6000, 6000)] {
            let mut rng = MapRng::new(0x180E3);
            let mut hits = 0;
            for _ in 0..6000 {
                if rng.below(m) <= 1 {
                    hits += 1;
                }
            }
            assert!((lo..=hi).contains(&hits), "m={m}: {hits}/6000");
        }
    }

    #[test]
    fn suitability_smoke() {
        let flat = Relief::Flat;
        let bare = Cover::Bare;
        assert!(suitable(20, &tile(Base::Grassland, flat, bare))); // wheat
        assert!(!suitable(20, &tile(Base::Ocean, flat, bare)));
        assert!(suitable(18, &tile(Base::Coast, flat, bare))); // fish
        assert!(suitable(16, &tile(Base::Ocean, flat, bare))); // whales
        assert!(!suitable(0, &tile(Base::Grassland, Relief::Hill, bare))); // horses flat only
        assert!(suitable(5, &tile(Base::Desert, Relief::Mountain, bare))); // iron anywhere land
    }

    #[test]
    fn terr_scores() {
        assert_eq!(terr_score(20), 2); // wheat: grass+plains flat
        assert_eq!(terr_score(18), 10); // fish: coast+sea + water bonus
        assert_eq!(terr_score(16), 10); // whales: ocean+sea + water bonus
        assert_eq!(terr_score(5), 12); // iron: all land rows
    }

    #[test]
    fn art_keys_all_exist_in_manifest() {
        // Catches GOOD-slug vs prep-manifest drift (a runtime panic in
        // spawn_features). Skips when assets were never prepped.
        let text = match crate::web::read_text("assets/cache/features/manifest.json") {
            Ok(t) => t,
            Err(_) => {
                eprintln!("skip: assets/cache/features missing (the game builds it at startup)");
                return;
            }
        };
        let raw: std::collections::HashMap<String, serde_json::Value> =
            serde_json::from_str(&text).expect("features manifest parses");
        for n in 0..8 {
            assert!(raw.contains_key(&format!("hut_{n}")), "hut_{n} missing");
        }
        assert!(raw.contains_key("camp"), "camp missing");
        for (id, good) in GOODS.iter().enumerate() {
            assert!(
                raw.contains_key(&art_key(id as u8)),
                "{} missing",
                good.name
            );
        }
    }

    #[test]
    fn hut_variant_covers_all_sprites() {
        let mut seen = HashSet::new();
        for y in 0..16 {
            for x in 0..16 {
                seen.insert(hut_variant(x, y));
            }
        }
        assert_eq!(seen.len(), 8);
    }

    #[test]
    fn placement_is_deterministic_and_valid() {
        let a = GameMap::generate();
        let b = GameMap::generate();
        let feats = |m: &GameMap| {
            m.tiles
                .iter()
                .map(|t| (t.hut, t.camp, t.resource))
                .collect::<Vec<_>>()
        };
        assert_eq!(feats(&a), feats(&b));
        let huts = a.tiles.iter().filter(|t| t.hut).count();
        assert!((1..=(N_HUTS as usize)).contains(&huts), "{huts} huts");
        let camps = a.tiles.iter().filter(|t| t.camp).count();
        assert!(camps <= N_CAMPS, "{camps} camps");
        let res = a.tiles.iter().filter(|t| t.resource.is_some()).count();
        assert!(res > 20, "{res} resources");
        // huts/camps sit on passable land, never on the start
        for y in 0..a.h {
            for x in 0..a.w {
                let t = &a.tiles[a.idx(x, y)];
                if t.hut || t.camp {
                    assert!(a.is_land(x, y));
                    assert!(move_cost(t).is_some());
                    assert_ne!((x, y), a.start);
                }
                if t.camp {
                    let d = (x - a.start.0).abs().max((y - a.start.1).abs());
                    assert!(d >= CAMP_START_DIST, "camp too close: {d}");
                }
            }
        }
    }

    #[test]
    fn camp_capture_clears_camp_and_posts_message() {
        use bevy::prelude::*;
        let mut map = GameMap::generate();
        let (sx, sy) = map.start;
        let si = map.idx(sx, sy);
        map.tiles[si].camp = true;
        let mut app = App::new();
        app.insert_resource(map);
        app.insert_resource(crate::units::UnitArt::default());
        app.insert_resource(crate::audio::GameAudio {
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
        app.insert_resource(MessageBoard::default());
        app.insert_resource(crate::rng::GameRng::new(2));
        app.insert_resource(crate::research::Research::new());
        app.insert_resource(crate::combat::CombatRng(crate::rng::MapRng::new(2)));
        app.init_resource::<crate::civs::Civilizations>();
        app.world_mut().spawn(crate::units::Unit {
            civ: 0,
            utype: crate::units::UnitType::named("Scout"),
            x: sx,
            y: sy,
            moves: 2,
            fortified: false,
            facing: 0,
            path: Default::default(),
            anim: crate::units::UnitAnim::Idle { t: 0.0 },
            work: None,
            sentry: false,
            exploring: false,
            ..crate::units::Unit::new(0, crate::units::UnitType::named("Scout"), sx, sy)
        });
        app.init_resource::<crate::cities::Treasury>();
        app.init_resource::<crate::barbarians::Barbarians>();
        app.insert_resource(crate::units::Turn(1));
        app.add_systems(Update, resolve_features);
        app.update();
        let map = app.world().resource::<GameMap>();
        assert!(!map.tiles[map.idx(sx, sy)].camp);
        assert_eq!(app.world().resource::<crate::cities::Treasury>().0[0], 25);
        let board = app.world().resource::<MessageBoard>();
        assert!(board.text.contains("dispersed"), "got: {}", board.text);
        assert!(board.ttl > 0.0);
    }

    #[test]
    fn some_seed_puts_a_hut_next_to_start() {
        // Property test that doubles as the live-E2E demo seed finder:
        // huts must be able to spawn within one step of the start.
        let mut found = None;
        for seed in 0..2000u64 {
            let map = GameMap::generate_with_seed(seed);
            let (sx, sy) = map.start;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let (x, y) = (map.wrap_x(sx + dx), sy + dy);
                    if y < 0 || y >= map.h {
                        continue;
                    }
                    if map.get(x, y).is_some_and(|t| t.hut) {
                        found = Some((seed, (sx, sy), (x, y)));
                        break;
                    }
                }
                if found.is_some() {
                    break;
                }
            }
            if found.is_some() {
                break;
            }
        }
        let (seed, start, hut) = found.expect("no adjacent hut in 2000 seeds");
        println!("DEMO_SEED seed={seed} start={start:?} hut={hut:?}");
    }

    #[test]
    fn demo_seed_layout_is_stable() {
        // Pins the live-E2E preconditions for MAP_SEED=2.
        let map = GameMap::generate_with_seed(2);
        assert_eq!(map.start, (43, 30));
        assert!(map.get(42, 30).is_some_and(|t| t.hut));
        let camps: Vec<(i32, i32)> = (0..map.h)
            .flat_map(|y| (0..map.w).map(move |x| (x, y)))
            .filter(|(x, y)| map.get(*x, *y).is_some_and(|t| t.camp))
            .collect();
        println!("DEMO_CAMPS seed=2 start={:?} camps={camps:?}", map.start);
        assert!(!camps.is_empty(), "seed 2 should have camps");
    }

    #[test]
    fn resource_counts_respect_quantity() {
        let map = GameMap::generate();
        let land = map
            .tiles
            .iter()
            .filter(|t| {
                matches!(
                    t.base,
                    Base::Grassland | Base::Plains | Base::Desert | Base::Tundra
                )
            })
            .count() as u32;
        let area = (land / 800).max(1);
        // Recompute n per good. All table freqs are nonzero so quantity()
        // consumes no RNG draws and the seed is irrelevant.
        let mut rng = MapRng::new(1);
        for (id, good) in GOODS.iter().enumerate() {
            let n = quantity(area, good.freq, terr_score(id as u8), &mut rng);
            let got = map
                .tiles
                .iter()
                .filter(|t| t.resource == Some(id as u8))
                .count() as u32;
            assert!(got <= n, "{}: placed {got} > quantity {n}", good.name);
        }
    }
}
