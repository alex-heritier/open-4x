//! Units: definitions, art, spawning, selection, movement, animation.

use bevy::prelude::*;
use bevy::sprite::Anchor;
use serde::Deserialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;

use crate::improvements::{Work, action_slot};
use crate::map::*;
use crate::render::{RevealAll, sprite_z};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum UnitType {
    Settler,
    Worker,
    Warrior,
    Scout,
}

pub struct UnitDef {
    pub moves: u8,
    pub sight: u8,
    pub dir: &'static str,
    pub name: &'static str,
}

pub fn def(t: UnitType) -> UnitDef {
    match t {
        UnitType::Settler => UnitDef {
            moves: 1,
            sight: 1,
            dir: "Settler",
            name: "Settler",
        },
        UnitType::Worker => UnitDef {
            moves: 1,
            sight: 1,
            dir: "Worker",
            name: "Worker",
        },
        UnitType::Warrior => UnitDef {
            moves: 1,
            sight: 1,
            dir: "warrior",
            name: "Warrior",
        },
        UnitType::Scout => UnitDef {
            moves: 2,
            sight: 2,
            dir: "Scout",
            name: "Scout",
        },
    }
}

#[derive(Deserialize)]
struct ClipEntry {
    frame: [u32; 2],
    frames: usize,
    #[allow(dead_code)]
    dirs: usize,
}

pub struct Clip {
    pub strips: [Handle<Image>; 8],
    pub frame_w: f32,
    pub frame_h: f32,
    pub frames: usize,
}

pub struct UnitClipSet {
    pub clips: HashMap<String, Clip>,
}

#[derive(Resource, Default)]
pub struct UnitArt {
    pub sets: HashMap<UnitType, UnitClipSet>,
    pub font: Handle<Font>,
}

impl UnitArt {
    pub fn load(asset_server: &AssetServer) -> Self {
        let mut sets = HashMap::new();
        for t in [
            UnitType::Settler,
            UnitType::Worker,
            UnitType::Warrior,
            UnitType::Scout,
        ] {
            let dir = def(t).dir;
            let text = fs::read_to_string(format!("assets/gen/units/{dir}/manifest.json"))
                .expect("run from civ3-clone/ after tools/prep_assets.py");
            let raw: HashMap<String, ClipEntry> =
                serde_json::from_str(&text).expect("unit manifest parses");
            let mut clips = HashMap::new();
            for (slot, e) in raw {
                let strips: Vec<Handle<Image>> = (0..8)
                    .map(|d| asset_server.load(format!("gen/units/{dir}/{slot}_d{d}.png")))
                    .collect();
                clips.insert(
                    slot,
                    Clip {
                        strips: strips.try_into().unwrap(),
                        frame_w: e.frame[0] as f32,
                        frame_h: e.frame[1] as f32,
                        frames: e.frames,
                    },
                );
            }
            sets.insert(t, UnitClipSet { clips });
        }
        Self {
            sets,
            font: asset_server.load("gen/fonts/lsans.ttf"),
        }
    }

    pub fn clip(&self, t: UnitType, slot: &str) -> Option<&Clip> {
        self.sets.get(&t)?.clips.get(slot)
    }
}

#[derive(Component)]
pub struct Unit {
    pub civ: usize,
    pub utype: UnitType,
    pub x: i32,
    pub y: i32,
    pub moves: u8,
    pub fortified: bool,
    pub facing: usize,
    pub path: VecDeque<(i32, i32)>,
    pub anim: UnitAnim,
    pub work: Option<Work>,
    /// Sentried: skipped by auto-select like fortified, but not dug in.
    pub sentry: bool,
    /// Auto-exploring: each turn the unit routes itself toward unseen tiles
    /// until none are reachable. Any manual order cancels it.
    pub exploring: bool,
}

#[derive(Clone, Copy, PartialEq)]
pub enum UnitAnim {
    Idle {
        t: f32,
    },
    Stepping {
        from: Vec2,
        to: Vec2,
        t: f32,
        dur: f32,
    },
    OneShot {
        slot: &'static str,
        t: f32,
    },
}

#[derive(Resource, Default)]
pub struct Selected(pub Option<Entity>);

#[derive(Resource)]
pub struct Turn(pub u32);

#[derive(Message)]
pub struct TurnEnded;

const STEP_DUR: f32 = 0.38;

/// FLC strip order, read off the warrior sheet: the unit's facing runs
/// d0 = S, d1 = SE, d2 = E, d3 = NE, d4 = N, d5 = NW, d6 = W, d7 = SW, the
/// compass cycle counterclockwise from south. d1 is the wide chest-on view
/// (map-southeast walks straight at the camera) and d3 the right profile
/// (map-northeast walks straight across the screen).
pub fn facing_for_step(dx: i32, dy: i32) -> usize {
    // Compass angle (east 0, north 90), never the isometric screen angle:
    // on screen the map's cardinals run along the diagonals, so rounding
    // the screen angle to the nearest eighth of a turn gave east and north
    // a neighbouring strip.
    let theta = (-(dy as f32)).atan2(dx as f32).to_degrees();
    ((theta - 270.0) / 45.0).round().rem_euclid(8.0) as usize
}

/// Frame sub-rect, inset half a pixel so strip sampling never bleeds
/// the neighboring frame at UV boundaries.
fn frame_rect(clip: &Clip, f: usize) -> Rect {
    Rect::new(
        f as f32 * clip.frame_w + 0.5,
        0.0,
        (f + 1) as f32 * clip.frame_w - 0.5,
        clip.frame_h,
    )
}

pub(crate) fn spawn_unit(
    commands: &mut Commands,
    art: &UnitArt,
    utype: UnitType,
    x: i32,
    y: i32,
    civ: usize,
) -> Entity {
    let clip = art.clip(utype, "DEFAULT").expect("DEFAULT clip");
    let pos = tile_to_world(x, y);
    let h = clip.frame_h;
    commands
        .spawn((
            Sprite {
                image: clip.strips[0].clone(),
                rect: Some(frame_rect(clip, 0)),
                ..default()
            },
            Anchor(Vec2::new(0.0, -0.5 + 6.0 / h)),
            Transform::from_xyz(pos.x, pos.y, sprite_z(x, y, 4.0)),
            Unit {
                civ,
                utype,
                x,
                y,
                moves: def(utype).moves * MP,
                fortified: false,
                facing: 0, // south: a fresh unit faces the viewer
                path: VecDeque::new(),
                anim: UnitAnim::Idle { t: 0.0 },
                work: None,
                sentry: false,
                exploring: false,
            },
        ))
        .with_children(|c| {
            c.spawn((
                Text2d::new(""),
                TextFont {
                    font: art.font.clone(),
                    font_size: 18.0,
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.95, 0.4)),
                Transform::from_xyz(16.0, 26.0, 0.1),
                StackBadge,
            ));
        })
        .id()
}

pub fn spawn_party(mut commands: Commands, map: Res<GameMap>, art: Res<UnitArt>) {
    let party = [
        UnitType::Settler,
        UnitType::Worker,
        UnitType::Warrior,
        UnitType::Scout,
    ];
    let mut first = None;
    for (civ, (sx, sy)) in crate::civs::starting_positions(&map)
        .into_iter()
        .enumerate()
    {
        for (i, t) in party.iter().enumerate() {
            let e = spawn_unit(&mut commands, &art, *t, sx, sy, civ);
            if civ == 0 && i == 0 {
                first = Some(e);
            }
        }
    }
    commands.insert_resource(Selected(first));
}

pub fn order_move(map: &GameMap, u: &mut Unit, dest: (i32, i32)) {
    // A manual move order cancels standing orders: worker jobs, fortify,
    // sentry, and auto-explore.
    u.work = None;
    u.exploring = false;
    if (u.x, u.y) == dest {
        u.path.clear();
        return;
    }
    match map.find_path((u.x, u.y), dest) {
        Some(p) => {
            u.path = p.into();
            u.fortified = false;
            u.sentry = false;
        }
        None => u.path.clear(),
    }
}

/// Route an auto-exploring unit toward its nearest unseen passable tile.
/// Breadth-first over passable tiles, so the first unseen tile found is the
/// closest reachable one and needs a single `find_path`. Returns false when
/// nothing reachable is left unseen, which ends the automation.
pub fn plan_explore(map: &GameMap, u: &mut Unit) -> bool {
    let mut seen_q = VecDeque::from([(u.x, u.y)]);
    let mut done = HashSet::from([(u.x, u.y)]);
    while let Some((x, y)) = seen_q.pop_front() {
        for (nx, ny) in map.neighbors(x, y) {
            if !done.insert((nx, ny)) {
                continue;
            }
            let Some(t) = map.get(nx, ny) else {
                continue;
            };
            if move_cost(t).is_none() {
                continue;
            }
            // Huts and camps are worth the detour: stepping on them pops them.
            if !t.seen || t.hut || t.camp {
                if let Some(p) = map.find_path((u.x, u.y), (nx, ny)) {
                    if !p.is_empty() {
                        u.path = p.into();
                        return true;
                    }
                }
                // Unseen but unreachable from here: keep looking past it.
            }
            seen_q.push_back((nx, ny));
        }
    }
    u.path.clear();
    false
}

/// Extra turns a unit at `start` needs to walk `path` (0 = arrives this
/// turn), under the "any movement left may enter" rule. Moves are thirds.
pub fn path_turns(
    map: &GameMap,
    start: (i32, i32),
    moves_now: u8,
    moves_max: u8,
    path: &[(i32, i32)],
) -> u32 {
    let mut mv = moves_now;
    let mut turns = 0;
    let mut from = start;
    for &(x, y) in path {
        let cost = match (map.get(from.0, from.1), map.get(x, y)) {
            (Some(a), Some(b)) => step_cost(a, b).unwrap_or(MP),
            _ => MP,
        };
        from = (x, y);
        if mv == 0 {
            turns += 1;
            mv = moves_max;
        }
        mv -= cost.min(mv);
    }
    turns
}

/// Moves left for display: whole MP plus any thirds, like Civ3's "1 1/3".
pub fn fmt_moves(thirds: u8) -> String {
    match (thirds / MP, thirds % MP) {
        (w, 0) => w.to_string(),
        (0, f) => format!("{f}/{MP}"),
        (w, f) => format!("{w} {f}/{MP}"),
    }
}

pub fn end_turn_units(
    mut events: MessageReader<TurnEnded>,
    mut turn: ResMut<Turn>,
    mut civs: ResMut<crate::civs::Civilizations>,
    mut ended: MessageWriter<crate::civs::CivilizationEnded>,
    mut selected: ResMut<Selected>,
    mut goto: ResMut<crate::actionbar::GotoMode>,
    mut units: Query<&mut Unit>,
) {
    for _ in events.read() {
        ended.write(crate::civs::CivilizationEnded(civs.active));
        civs.active = (civs.active + 1) % crate::civs::CIV_COUNT;
        if civs.active == 0 {
            turn.0 += 1;
        }
        selected.0 = None;
        goto.0 = false;
        for mut u in units.iter_mut() {
            if u.civ == civs.active {
                u.moves = def(u.utype).moves * MP;
            }
        }
    }
}

pub fn drive_movement(
    map: Res<GameMap>,
    civs: Res<crate::civs::Civilizations>,
    mut units: Query<(Entity, &mut Unit)>,
) {
    for (_, mut u) in units.iter_mut() {
        if u.civ != civs.active {
            continue;
        }
        if !matches!(u.anim, UnitAnim::Idle { .. }) {
            continue;
        }
        if u.fortified || u.moves == 0 {
            continue;
        }
        // Auto-explore plans a fresh leg whenever the last one runs out;
        // with nothing left unseen the automation ends and the unit waits.
        if u.exploring && u.path.is_empty() && !plan_explore(&map, &mut u) {
            u.exploring = false;
            continue;
        }
        let Some(&(nx, ny)) = u.path.front() else {
            continue;
        };
        let cost = match (map.get(u.x, u.y), map.get(nx, ny)) {
            (Some(from), Some(to)) => step_cost(from, to),
            _ => None,
        };
        let Some(cost) = cost else {
            u.path.clear();
            continue;
        };
        // Civ3 rule: any unit with movement left may enter, spending it all
        // when the tile costs more than it has.
        u.moves -= cost.min(u.moves);
        u.facing = facing_for_step(nx - u.x, ny - u.y);
        let from = tile_to_world(u.x, u.y);
        let to = tile_to_world(nx, ny);
        u.x = nx;
        u.y = ny;
        u.path.pop_front();
        u.fortified = false;
        u.anim = UnitAnim::Stepping {
            from,
            to,
            t: 0.0,
            dur: STEP_DUR,
        };
    }
}

pub fn advance_anims(
    time: Res<Time>,
    art: Res<UnitArt>,
    mut q: Query<(&mut Unit, &mut Transform)>,
) {
    for (mut u, mut tf) in q.iter_mut() {
        let utype = u.utype;
        match &mut u.anim {
            UnitAnim::Idle { t } => *t += time.delta_secs(),
            UnitAnim::Stepping { from, to, t, dur } => {
                *t += time.delta_secs();
                let k = (*t / *dur).min(1.0);
                let p = from.lerp(*to, k);
                tf.translation.x = p.x;
                tf.translation.y = p.y;
                tf.translation.z = sprite_z(u.x, u.y, 4.0);
                if k >= 1.0 {
                    u.anim = UnitAnim::Idle { t: 0.0 };
                }
            }
            UnitAnim::OneShot { slot, t } => {
                *t += time.delta_secs();
                let frames = art.clip(utype, *slot).map(|c| c.frames).unwrap_or(8) as f32;
                if *t * 12.0 >= frames {
                    u.anim = UnitAnim::Idle { t: 0.0 };
                }
            }
        }
    }
}

pub fn animate_units(art: Res<UnitArt>, mut q: Query<(&Unit, &mut Sprite)>) {
    for (u, mut sprite) in q.iter_mut() {
        let (slot, t, fps, looped) = match u.anim {
            UnitAnim::Idle { t } => {
                // Standing orders show: a working unit keeps looping its
                // job clip, and a fortified or sentried unit holds its
                // dug-in frame (Civ3 keeps FORTIFY's last frame up; units
                // without that art idle as usual).
                if let Some(w) = u.work {
                    (action_slot(w.action), t, 12.0, true)
                } else if u.fortified {
                    match art.clip(u.utype, "FORTIFY") {
                        Some(clip) => {
                            sprite.image = clip.strips[u.facing].clone();
                            sprite.rect = Some(frame_rect(clip, clip.frames - 1));
                            continue;
                        }
                        None => ("DEFAULT", t, 8.0, true),
                    }
                } else {
                    ("DEFAULT", t, 8.0, true)
                }
            }
            UnitAnim::Stepping { t, dur, .. } => {
                // run cycle syncs to the step duration
                let clip = art.clip(u.utype, "RUN");
                let fps = clip.map(|c| c.frames as f32 / dur).unwrap_or(20.0);
                let Some(clip) = clip.or_else(|| art.clip(u.utype, "DEFAULT")) else {
                    continue;
                };
                let f = ((t * fps) as usize) % clip.frames;
                sprite.image = clip.strips[u.facing].clone();
                sprite.rect = Some(frame_rect(clip, f));
                continue;
            }
            UnitAnim::OneShot { slot, t } => (slot, t, 12.0, false),
        };
        let Some(clip) = art
            .clip(u.utype, slot)
            .or_else(|| art.clip(u.utype, "DEFAULT"))
        else {
            continue;
        };
        let f = if looped {
            ((t * fps) as usize) % clip.frames
        } else {
            ((t * fps) as usize).min(clip.frames - 1)
        };
        sprite.image = clip.strips[u.facing].clone();
        sprite.rect = Some(frame_rect(clip, f));
    }
}

pub fn refresh_visibility(
    mut map: ResMut<GameMap>,
    civs: Res<crate::civs::Civilizations>,
    mut explored: Local<[Vec<bool>; crate::civs::CIV_COUNT]>,
    mut previous: Local<Option<usize>>,
    units: Query<&Unit>,
    cities: Query<&crate::cities::City>,
) {
    if let Some(civ) = *previous {
        for (seen, tile) in explored[civ].iter_mut().zip(&map.tiles) {
            *seen = tile.seen;
        }
    }
    *previous = Some(civs.active);
    let memory = &mut explored[civs.active];
    memory.resize(map.tiles.len(), false);
    for (t, &seen) in map.tiles.iter_mut().zip(memory.iter()) {
        t.visible = false;
        t.seen = seen;
    }
    for u in units.iter() {
        if u.civ != civs.active {
            continue;
        }
        let bonus = match map.get(u.x, u.y).map(|t| t.relief) {
            Some(Relief::Hill) | Some(Relief::Mountain) => 1,
            _ => 0,
        };
        let r = def(u.utype).sight as i32 + bonus;
        for dy in -r..=r {
            for dx in -r..=r {
                let ny = u.y + dy;
                if ny < 0 || ny >= map.h {
                    continue;
                }
                let nx = map.wrap_x(u.x + dx);
                let i = map.idx(nx, ny);
                let t = &mut map.tiles[i];
                t.visible = true;
                t.seen = true;
            }
        }
    }
    // Cities light their workable radius, as in Civ3.
    for c in cities.iter() {
        if c.civ != civs.active {
            continue;
        }
        let mut tiles = crate::cities::radius_tiles(&map, c.x, c.y);
        tiles.push((c.x, c.y));
        for (nx, ny) in tiles {
            let i = map.idx(nx, ny);
            let t = &mut map.tiles[i];
            t.visible = true;
            t.seen = true;
        }
    }
    for (seen, tile) in memory.iter_mut().zip(&map.tiles) {
        *seen = tile.seen;
    }
}

/// Civ3 shows one unit per tile: the selected one, else the best
/// defender, with a count badge for the rest.
fn defender_rank(t: UnitType) -> u8 {
    match t {
        UnitType::Warrior => 3,
        UnitType::Scout => 2,
        UnitType::Settler => 1,
        UnitType::Worker => 0,
    }
}

fn stack_tops(
    units: &Query<(Entity, &Unit)>,
    selected: Option<Entity>,
) -> HashMap<(i32, i32), (Entity, usize)> {
    let mut best: HashMap<(i32, i32), (Entity, usize, (bool, u8))> = HashMap::new();
    for (e, u) in units.iter() {
        let key = (Some(e) == selected, defender_rank(u.utype));
        let entry = best.entry((u.x, u.y)).or_insert((e, 0, key));
        entry.1 += 1;
        if key > entry.2 || (key == entry.2 && e < entry.0) {
            entry.0 = e;
            entry.2 = key;
        }
    }
    best.into_iter().map(|(k, (e, n, _))| (k, (e, n))).collect()
}

/// Keep stacked units on the tile center; the top unit sorts above.
pub fn restack(
    mut q: Query<(Entity, &Unit, &mut Transform)>,
    selected: Res<Selected>,
    stack_q: Query<(Entity, &Unit)>,
) {
    let tops = stack_tops(&stack_q, selected.0);
    for (e, u, mut tf) in q.iter_mut() {
        if !matches!(u.anim, UnitAnim::Idle { .. }) {
            continue;
        }
        let pos = tile_to_world(u.x, u.y);
        tf.translation.x = pos.x;
        tf.translation.y = pos.y;
        let top = tops.get(&(u.x, u.y)).is_some_and(|(t, _)| *t == e);
        tf.translation.z = sprite_z(u.x, u.y, 4.0) + if top { 0.0005 } else { 0.0 };
    }
}

#[derive(Component)]
pub struct StackBadge;

/// Civ3 only lets a unit with movement left be selected.
pub fn selectable(u: &Unit) -> bool {
    u.moves > 0
}

/// A unit "requires attention": it can act and has no standing order
/// (fortify, sentry, explore, a worker job or a go-to route).
pub fn needs_orders(u: &Unit) -> bool {
    selectable(u) && !u.fortified && !u.exploring && u.work.is_none() && u.path.is_empty()
}

/// Keep the selection while the unit can still act (or is still walking a
/// route), otherwise move to the nearest unit that needs orders, or to none:
/// with nothing selected the HUD invites the player to end the turn.
pub fn auto_select(
    units: Query<(Entity, &Unit)>,
    civs: Res<crate::civs::Civilizations>,
    mut selected: ResMut<Selected>,
    mut last_pos: Local<(i32, i32)>,
) {
    if let Some((_, u)) = selected.0.and_then(|s| units.get(s).ok()) {
        *last_pos = (u.x, u.y);
        if u.civ == civs.active && (needs_orders(u) || (selectable(u) && !u.path.is_empty())) {
            return;
        }
    }
    let (lx, ly) = *last_pos;
    selected.0 = units
        .iter()
        .filter(|(_, u)| u.civ == civs.active && needs_orders(u))
        .min_by_key(|(_, u)| (u.x - lx).abs().max((u.y - ly).abs()))
        .map(|(e, _)| e);
}

pub fn unit_visibility(
    map: Res<GameMap>,
    reveal: Res<RevealAll>,
    selected: Res<Selected>,
    stack_q: Query<(Entity, &Unit)>,
    mut q: Query<(Entity, &Unit, &mut Visibility, Option<&Children>)>,
    mut badges: Query<&mut Text2d, With<StackBadge>>,
) {
    let tops = stack_tops(&stack_q, selected.0);
    for (e, u, mut v, children) in q.iter_mut() {
        let seen = reveal.0 || map.get(u.x, u.y).is_some_and(|t| t.visible);
        let (top, n) = tops
            .get(&(u.x, u.y))
            .map(|(t, n)| (*t == e, *n))
            .unwrap_or((true, 1));
        // Moving units stay visible; idle stack members hide behind the top.
        let moving = !matches!(u.anim, UnitAnim::Idle { .. });
        *v = if seen && (top || moving) {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        if let Some(children) = children {
            for c in children.iter() {
                if let Ok(mut t) = badges.get_mut(c) {
                    let want = if top && n > 1 && !moving {
                        format!("x{n}")
                    } else {
                        String::new()
                    };
                    if t.0 != want {
                        t.0 = want;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotseat_refreshes_only_incoming_units_and_counts_full_rounds() {
        let mut app = App::new();
        app.add_message::<TurnEnded>();
        app.add_message::<crate::civs::CivilizationEnded>();
        app.init_resource::<crate::civs::Civilizations>();
        app.init_resource::<Selected>();
        app.init_resource::<crate::actionbar::GotoMode>();
        app.insert_resource(Turn(1));
        app.add_systems(Update, end_turn_units);
        let ids: Vec<_> = (0..crate::civs::CIV_COUNT)
            .map(|civ| {
                let mut unit = scout_at(civ as i32, 0);
                unit.civ = civ;
                unit.moves = 0;
                app.world_mut().spawn(unit).id()
            })
            .collect();
        app.world_mut().resource_mut::<Selected>().0 = Some(ids[0]);
        for incoming in [1, 2, 0] {
            app.world_mut().write_message(TurnEnded);
            app.update();
            assert_eq!(
                app.world().resource::<crate::civs::Civilizations>().active,
                incoming
            );
            assert_eq!(
                app.world().resource::<Turn>().0,
                if incoming == 0 { 2 } else { 1 }
            );
            assert_eq!(
                app.world().get::<Unit>(ids[incoming]).unwrap().moves,
                def(UnitType::Scout).moves * MP
            );
            assert_eq!(app.world().resource::<Selected>().0, None);
            if incoming != 0 {
                assert_eq!(app.world().get::<Unit>(ids[0]).unwrap().moves, 0);
            }
        }
    }

    #[test]
    fn fog_is_private_and_remembers_exploration_after_handoff() {
        let map = GameMap::generate();
        let starts = crate::civs::starting_positions(&map);
        let mut app = App::new();
        app.insert_resource(map);
        app.init_resource::<crate::civs::Civilizations>();
        app.add_systems(Update, refresh_visibility);
        for (civ, (x, y)) in starts.into_iter().enumerate() {
            let mut unit = scout_at(x, y);
            unit.civ = civ;
            app.world_mut().spawn(unit);
        }
        app.update();
        let extra = {
            let map = app.world().resource::<GameMap>();
            let japan = map.idx(starts[0].0, starts[0].1);
            let rome = map.idx(starts[1].0, starts[1].1);
            assert!(map.tiles[japan].visible);
            assert!(!map.tiles[rome].seen);
            map.tiles.iter().position(|t| !t.seen).unwrap()
        };
        app.world_mut().resource_mut::<GameMap>().tiles[extra].seen = true;
        app.world_mut()
            .resource_mut::<crate::civs::Civilizations>()
            .active = 1;
        app.update();
        let map = app.world().resource::<GameMap>();
        assert!(!map.tiles[map.idx(starts[0].0, starts[0].1)].seen);
        assert!(map.tiles[map.idx(starts[1].0, starts[1].1)].visible);
        app.world_mut()
            .resource_mut::<crate::civs::Civilizations>()
            .active = 0;
        app.update();
        assert!(app.world().resource::<GameMap>().tiles[extra].seen);
    }

    #[test]
    fn facing_matches_flc_order() {
        // d0=S d1=SE d2=E d3=NE d4=N d5=NW d6=W d7=SW
        assert_eq!(facing_for_step(0, 1), 0); // south walks at the camera
        assert_eq!(facing_for_step(1, 1), 1);
        assert_eq!(facing_for_step(1, 0), 2); // east is a diagonal on screen
        assert_eq!(facing_for_step(1, -1), 3);
        assert_eq!(facing_for_step(0, -1), 4); // north is a diagonal on screen
        assert_eq!(facing_for_step(-1, -1), 5);
        assert_eq!(facing_for_step(-1, 0), 6);
        assert_eq!(facing_for_step(-1, 1), 7);
    }

    #[test]
    fn roads_make_long_walks_short() {
        let mut map = GameMap::generate();
        let (sx, sy) = map.start;
        // three road steps east of the start, all on passable land
        let run: Vec<(i32, i32)> = (0..=3).map(|d| (map.wrap_x(sx + d), sy)).collect();
        for &(x, y) in &run {
            let i = map.idx(x, y);
            map.tiles[i].base = Base::Grassland;
            map.tiles[i].relief = Relief::Hill;
            map.tiles[i].cover = Cover::Bare;
        }
        let hills = path_turns(&map, (sx, sy), MP, MP, &run[1..]);
        assert!(hills >= 2, "three hills take a 1-MP unit three turns");
        for &(x, y) in &run {
            let i = map.idx(x, y);
            map.tiles[i].road = true;
        }
        // 3 road steps cost 3 thirds: one MP, arriving this turn
        assert_eq!(path_turns(&map, (sx, sy), MP, MP, &run[1..]), 0);
        let path = map.find_path((sx, sy), run[3]).unwrap();
        assert_eq!(path.len(), 3);
        assert_eq!(fmt_moves(4), "1 1/3");
        assert_eq!(fmt_moves(2), "2/3");
        assert_eq!(fmt_moves(6), "2");
    }

    fn scout_at(x: i32, y: i32) -> Unit {
        Unit {
            civ: 0,
            utype: UnitType::Scout,
            x,
            y,
            moves: MP,
            fortified: false,
            facing: 0,
            path: VecDeque::new(),
            anim: UnitAnim::Idle { t: 0.0 },
            work: None,
            sentry: false,
            exploring: false,
        }
    }

    #[test]
    fn explore_routes_to_unseen_tiles_then_ends() {
        let map = GameMap::generate();
        let (sx, sy) = map.start;
        let mut u = scout_at(sx, sy);
        u.exploring = true;
        assert!(plan_explore(&map, &mut u));
        assert!(!u.path.is_empty());
        // A fully seen map with no huts or camps ends the automation.
        let mut done_map = GameMap::generate();
        for t in done_map.tiles.iter_mut() {
            t.seen = true;
            t.hut = false;
            t.camp = false;
        }
        u.path.clear();
        assert!(!plan_explore(&done_map, &mut u));
    }

    #[test]
    fn exploring_units_need_no_orders_but_manual_orders_cancel() {
        let map = GameMap::generate();
        let (sx, sy) = map.start;
        let mut u = scout_at(sx, sy);
        u.exploring = true;
        assert!(!needs_orders(&u));
        order_move(&map, &mut u, (map.wrap_x(sx + 1), sy));
        assert!(!u.exploring);
    }

    #[test]
    fn path_turns_counts_turn_boundaries() {
        let map = GameMap::generate();
        let (sx, sy) = map.start;
        // straight run over land tiles found near the start
        let mut path = vec![];
        for dx in 1..=3 {
            if map.get(sx + dx, sy).and_then(move_cost).is_some() {
                path.push((sx + dx, sy));
            }
        }
        if path.len() == 3 {
            let costs: u32 = path
                .iter()
                .map(|&(x, y)| map.get(x, y).and_then(move_cost).unwrap() as u32)
                .sum();
            assert!(path_turns(&map, (sx, sy), MP, MP, &path) >= 2 || costs > 3);
            assert_eq!(path_turns(&map, (sx, sy), 0, MP, &[]), 0);
        }
        // a full-move unit with nothing left waits a turn for a 1-step path
        assert_eq!(path_turns(&map, (sx, sy), 0, MP, &[(sx + 1, sy)]), 1);
    }

    fn ring_app(selected: Selected) -> (App, Entity, Entity) {
        let mut app = App::new();
        app.insert_resource(SelectionRing {
            image: Handle::default(),
            frame: Vec2::new(93.0, 46.0),
            frames: 31,
            ms: 175.0,
        });
        app.insert_resource(Time::<()>::default());
        let unit = app
            .world_mut()
            .spawn((scout_at(10, 10), Visibility::Visible, Transform::default()))
            .id();
        let ring = app
            .world_mut()
            .spawn((
                Sprite::default(),
                Visibility::Hidden,
                Transform::default(),
                SelectionRingSprite,
            ))
            .id();
        app.insert_resource(selected);
        app.add_systems(Update, ring_follow);
        (app, unit, ring)
    }

    #[test]
    fn the_ring_hides_without_a_selection() {
        let (mut app, _, ring) = ring_app(Selected(None));
        app.update();
        let vis = app.world().entity(ring).get::<Visibility>().unwrap();
        assert_eq!(*vis, Visibility::Hidden);
    }

    #[test]
    fn the_ring_sits_under_the_selected_unit() {
        let (mut app, unit, ring) = ring_app(Selected(None));
        app.world_mut().resource_mut::<Selected>().0 = Some(unit);
        app.update();
        let vis = app.world().entity(ring).get::<Visibility>().unwrap();
        assert_eq!(*vis, Visibility::Visible);
        let at = tile_to_world(10, 10);
        let tf = app.world().entity(ring).get::<Transform>().unwrap();
        assert_eq!((tf.translation.x, tf.translation.y), (at.x, at.y));
    }
}

/// The white ellipse Civ3 draws under the selected unit. The game ships no
/// Civ3's selection marker: the dashed ellipse under the selected unit,
/// which lives in `Art/Animations/Cursor/Cursor.flc` (prep stage `cursor`,
/// 31 frames of a 175 ms crawl, 93x46 each).
#[derive(Resource)]
pub struct SelectionRing {
    image: Handle<Image>,
    frame: Vec2,
    frames: u32,
    ms: f32,
}

#[derive(Deserialize)]
struct RingEntry {
    file: String,
    frame: [u32; 2],
    frames: u32,
    ms: f32,
}

impl SelectionRing {
    pub fn load(asset_server: &AssetServer) -> Self {
        let text = fs::read_to_string("assets/gen/cursor/manifest.json")
            .expect("run from civ3-clone/ after tools/prep_assets.py cursor");
        let raw: HashMap<String, RingEntry> =
            serde_json::from_str(&text).expect("cursor manifest parses");
        let e = &raw["ring"];
        Self {
            image: asset_server.load(format!("gen/cursor/{}", e.file)),
            frame: Vec2::new(e.frame[0] as f32, e.frame[1] as f32),
            frames: e.frames,
            ms: e.ms,
        }
    }

    /// Frame `f` of the horizontal strip.
    fn rect(&self, f: u32) -> Rect {
        let x = f as f32 * self.frame.x;
        Rect::new(x, 0.0, x + self.frame.x, self.frame.y)
    }
}

#[derive(Component)]
pub struct SelectionRingSprite;

/// The ring starts hidden; `ring_follow` shows it under the selection.
pub fn spawn_selection_ring(mut commands: Commands, ring: Res<SelectionRing>) {
    commands.spawn((
        Sprite {
            image: ring.image.clone(),
            rect: Some(ring.rect(0)),
            ..default()
        },
        Visibility::Hidden,
        Transform::default(),
        SelectionRingSprite,
    ));
}

/// Keep the ring under the selected unit's feet, mirroring the unit's own
/// visibility so a unit hidden by fog or under a stack hides its ring too.
pub fn ring_follow(
    time: Res<Time>,
    ring: Res<SelectionRing>,
    selected: Res<Selected>,
    units: Query<(&Unit, &Visibility)>,
    mut q: Query<
        (&mut Sprite, &mut Transform, &mut Visibility),
        (With<SelectionRingSprite>, Without<Unit>),
    >,
    mut crawl: Local<f32>,
) {
    let Ok((mut sprite, mut tf, mut vis)) = q.single_mut() else {
        return;
    };
    let Some((u, unit_vis)) = selected.0.and_then(|s| units.get(s).ok()) else {
        *vis = Visibility::Hidden;
        return;
    };
    // The dashes crawl around the ellipse: advance one frame every
    // `ring.ms`, as the FLC's own timing does.
    *crawl += time.delta_secs() * 1000.0;
    if ring.ms > 0.0 {
        let f = (*crawl / ring.ms) as u32 % ring.frames;
        let rect = ring.rect(f);
        if sprite.rect != Some(rect) {
            sprite.rect = Some(rect);
        }
    }
    let pos = tile_to_world(u.x, u.y);
    // On the unit's feet (the art's ellipse is centered in its cell) and
    // just below the unit sprites, which sit at layer 4.0.
    tf.translation = Vec3::new(pos.x, pos.y, sprite_z(u.x, u.y, 3.5));
    *vis = if *unit_vis == Visibility::Visible {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
}

pub fn selection_gizmo(
    selected: Res<Selected>,
    units: Query<&Unit>,
    map: Res<GameMap>,
    preview: Res<crate::input::MovePreview>,
    mut gizmos: Gizmos,
) {
    let Some(s) = selected.0 else {
        return;
    };
    let Ok(u) = units.get(s) else {
        return;
    };
    for (x, y) in u.path.iter() {
        gizmos.circle_2d(tile_to_world(*x, *y), 4.0, Color::WHITE);
    }
    // The route a Go-to, or a held press, would take. Nothing is drawn for
    // a plain hover: Civ3 only shows the path while it is being aimed.
    let Some(dest) = preview.0 else {
        return;
    };
    if dest == (u.x, u.y) {
        return;
    }
    if let Some(p) = map.find_path((u.x, u.y), dest) {
        let mut prev = tile_to_world(u.x, u.y);
        for (x, y) in p {
            let cur = tile_to_world(x, y);
            gizmos.line_2d(prev, cur, Color::srgba(1.0, 1.0, 0.3, 0.8));
            prev = cur;
        }
        gizmos.circle_2d(prev, 8.0, Color::srgb(1.0, 1.0, 0.3));
    }
}
