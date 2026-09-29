//! Units: definitions, art, spawning, selection, movement, animation.

use bevy::prelude::*;
use bevy::sprite::Anchor;
use serde::Deserialize;
use std::collections::{HashMap, VecDeque};
use std::fs;

use crate::improvements::Work;
use crate::map::*;
use crate::render::{sprite_z, RevealAll};

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
    pub can_found: bool,
    pub dir: &'static str,
    pub name: &'static str,
}

pub fn def(t: UnitType) -> UnitDef {
    match t {
        UnitType::Settler => UnitDef {
            moves: 1,
            sight: 1,
            can_found: true,
            dir: "Settler",
            name: "Settler",
        },
        UnitType::Worker => UnitDef {
            moves: 1,
            sight: 1,
            can_found: false,
            dir: "Worker",
            name: "Worker",
        },
        UnitType::Warrior => UnitDef {
            moves: 1,
            sight: 1,
            can_found: false,
            dir: "warrior",
            name: "Warrior",
        },
        UnitType::Scout => UnitDef {
            moves: 2,
            sight: 2,
            can_found: false,
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
            let text = fs::read_to_string(format!(
                "assets/gen/units/{dir}/manifest.json"
            ))
            .expect("run from civ3-clone/ after tools/prep_assets.py");
            let raw: HashMap<String, ClipEntry> =
                serde_json::from_str(&text).expect("unit manifest parses");
            let mut clips = HashMap::new();
            for (slot, e) in raw {
                let strips: Vec<Handle<Image>> = (0..8)
                    .map(|d| {
                        asset_server.load(format!("gen/units/{dir}/{slot}_d{d}.png"))
                    })
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
}

#[derive(Clone, Copy, PartialEq)]
pub enum UnitAnim {
    Idle { t: f32 },
    Stepping { from: Vec2, to: Vec2, t: f32, dur: f32 },
    OneShot { slot: &'static str, t: f32 },
}

#[derive(Resource, Default)]
pub struct Selected(pub Option<Entity>);

#[derive(Resource)]
pub struct Turn(pub u32);

#[derive(Message)]
pub struct TurnEnded;

const STEP_DUR: f32 = 0.38;

/// FLC strip order runs counterclockwise: d0=W, d1=SW, d2=S, d3=SE,
/// d4=E, d5=NE, d6=N, d7=NW (verified from the warrior sheet).
pub fn facing_for_step(dx: i32, dy: i32) -> usize {
    let dsx = (dx - dy) as f32;
    let dsy = -((dx + dy) as f32) * 0.5;
    let theta = dsy.atan2(dsx).to_degrees();
    ((theta - 180.0) / 45.0).round().rem_euclid(8.0) as usize
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
) -> Entity {
    let clip = art.clip(utype, "DEFAULT").expect("DEFAULT clip");
    let pos = tile_to_world(x, y);
    let h = clip.frame_h;
    commands
        .spawn((
            Sprite {
                image: clip.strips[2].clone(),
                rect: Some(frame_rect(clip, 0)),
                ..default()
            },
            Anchor(Vec2::new(0.0, -0.5 + 6.0 / h)),
            Transform::from_xyz(pos.x, pos.y, sprite_z(x, y, 4.0)),
            Unit {
                utype,
                x,
                y,
                moves: def(utype).moves,
                fortified: false,
                facing: 2,
                path: VecDeque::new(),
                anim: UnitAnim::Idle { t: 0.0 },
                work: None,
                sentry: false,
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
    let (sx, sy) = map.start;
    let party = [
        UnitType::Settler,
        UnitType::Worker,
        UnitType::Warrior,
        UnitType::Scout,
    ];
    let mut first = None;
    for (i, t) in party.iter().enumerate() {
        let e = spawn_unit(&mut commands, &art, *t, sx, sy);
        if i == 0 {
            first = Some(e);
        }
    }
    commands.insert_resource(Selected(first));
}

pub fn order_move(map: &GameMap, u: &mut Unit, dest: (i32, i32)) {
    // Moving cancels any worker job in progress.
    u.work = None;
    if (u.x, u.y) == dest {
        u.path.clear();
        return;
    }
    match map.find_path((u.x, u.y), dest) {
        Some(p) => {
            u.path = p.into();
            u.fortified = false;
        }
        None => u.path.clear(),
    }
}

/// Extra turns a unit needs to walk `path` (0 = arrives this turn),
/// under the "any movement left may enter" rule.
pub fn path_turns(
    map: &GameMap,
    moves_now: u8,
    moves_max: u8,
    path: &[(i32, i32)],
) -> u32 {
    let mut mv = moves_now;
    let mut turns = 0;
    for &(x, y) in path {
        let cost = map.get(x, y).and_then(move_cost).unwrap_or(1);
        if mv == 0 {
            turns += 1;
            mv = moves_max;
        }
        mv -= cost.min(mv);
    }
    turns
}

pub fn end_turn_units(
    mut events: MessageReader<TurnEnded>,
    mut turn: ResMut<Turn>,
    mut units: Query<&mut Unit>,
) {
    for _ in events.read() {
        turn.0 += 1;
        for mut u in units.iter_mut() {
            u.moves = def(u.utype).moves;
        }
    }
}

pub fn drive_movement(map: Res<GameMap>, mut units: Query<(Entity, &mut Unit)>) {
    for (_, mut u) in units.iter_mut() {
        if !matches!(u.anim, UnitAnim::Idle { .. }) {
            continue;
        }
        if u.fortified || u.moves == 0 {
            continue;
        }
        let Some(&(nx, ny)) = u.path.front() else {
            continue;
        };
        let cost = map.get(nx, ny).and_then(move_cost);
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
                let frames =
                    art.clip(utype, *slot).map(|c| c.frames).unwrap_or(8) as f32;
                if *t * 12.0 >= frames {
                    u.anim = UnitAnim::Idle { t: 0.0 };
                }
            }
        }
    }
}

pub fn animate_units(art: Res<UnitArt>, mut q: Query<(&Unit, &mut Sprite)>) {
    for (u, mut sprite) in q.iter_mut() {
        let (slot, t, looped) = match u.anim {
            UnitAnim::Idle { t } => ("DEFAULT", t, true),
            UnitAnim::Stepping { t, dur, .. } => {
                // run cycle syncs to the step duration
                let clip = art.clip(u.utype, "RUN");
                let fps = clip.map(|c| c.frames as f32 / dur).unwrap_or(20.0);
                let Some(clip) = clip.or_else(|| art.clip(u.utype, "DEFAULT"))
                else {
                    continue;
                };
                let f = ((t * fps) as usize) % clip.frames;
                sprite.image = clip.strips[u.facing].clone();
                sprite.rect = Some(frame_rect(clip, f));
                continue;
            }
            UnitAnim::OneShot { slot, t } => (slot, t, false),
        };
        let Some(clip) =
            art.clip(u.utype, slot).or_else(|| art.clip(u.utype, "DEFAULT"))
        else {
            continue;
        };
        let fps = if looped { 8.0 } else { 12.0 };
        let f = if looped {
            ((t * fps) as usize) % clip.frames
        } else {
            ((t * fps) as usize).min(clip.frames - 1)
        };
        sprite.image = clip.strips[u.facing].clone();
        sprite.rect = Some(frame_rect(clip, f));
    }
}

pub fn refresh_visibility(mut map: ResMut<GameMap>, units: Query<&Unit>) {
    for t in map.tiles.iter_mut() {
        t.visible = false;
    }
    for u in units.iter() {
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

pub fn auto_select(
    units: Query<(Entity, &Unit)>,
    mut selected: ResMut<Selected>,
) {
    if selected.0.is_some_and(|s| {
        units
            .get(s)
            .is_ok_and(|(_, u)| !u.fortified && u.moves > 0 && u.work.is_none())
    }) {
        return;
    }
    if let Some((e, _)) = units
        .iter()
        .find(|(_, u)| !u.fortified && u.moves > 0 && u.work.is_none())
    {
        selected.0 = Some(e);
    } else if !selected.0.is_some_and(|s| units.get(s).is_ok()) {
        selected.0 = units.iter().next().map(|(e, _)| e);
    }
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
    fn facing_matches_flc_order() {
        // d0=W d1=SW d2=S d3=SE d4=E d5=NE d6=N d7=NW
        assert_eq!(facing_for_step(1, 0), 3); // map-east runs screen-SE
        assert_eq!(facing_for_step(-1, 0), 7); // map-west runs screen-NW
        assert_eq!(facing_for_step(0, -1), 5); // map-north runs screen-NE
        assert_eq!(facing_for_step(0, 1), 1); // map-south runs screen-SW
        assert_eq!(facing_for_step(1, 1), 2); // diagonal S
        assert_eq!(facing_for_step(-1, -1), 6); // diagonal N
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
            assert!(path_turns(&map, 1, 1, &path) >= 2 || costs > 3);
            assert_eq!(path_turns(&map, 0, 1, &[]), 0);
        }
        // a full-move unit with nothing left waits a turn for a 1-step path
        assert_eq!(path_turns(&map, 0, 1, &[(sx, sy)]), 1);
    }
}

pub fn selection_gizmo(
    selected: Res<Selected>,
    units: Query<&Unit>,
    map: Res<GameMap>,
    hovered: Res<crate::input::Hovered>,
    mut gizmos: Gizmos,
) {
    let Some(s) = selected.0 else {
        return;
    };
    let Ok(u) = units.get(s) else {
        return;
    };
    let c = tile_to_world(u.x, u.y);
    let hw = TILE_W / 2.0;
    let hh = TILE_H / 2.0;
    let corners = [
        Vec2::new(c.x, c.y + hh),
        Vec2::new(c.x + hw, c.y),
        Vec2::new(c.x, c.y - hh),
        Vec2::new(c.x - hw, c.y),
    ];
    for i in 0..4 {
        gizmos.line_2d(corners[i], corners[(i + 1) % 4], Color::srgb(1.0, 1.0, 0.0));
    }
    for (x, y) in u.path.iter() {
        gizmos.circle_2d(tile_to_world(*x, *y), 4.0, Color::WHITE);
    }
    // Preview of the route a click would take.
    if let Some(dest) = hovered.0 {
        if dest != (u.x, u.y) {
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
    }
}
