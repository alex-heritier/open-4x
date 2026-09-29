//! Camera control and tile hover picking.

use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::audio::{self, GameAudio};
use crate::cities::{City, CityView};
use crate::actionbar::{key_commands, GotoMode, UnitCommand};
use crate::map::*;
use crate::render::RevealAll;
use crate::splash::SplashUp;
use crate::units::{self, Selected, TurnEnded, Unit};

#[derive(Resource, Default)]
pub struct Hovered(pub Option<(i32, i32)>);

#[derive(Resource, Default)]
pub struct DragState {
    start: Vec2,
    last: Vec2,
    active: bool,
    moved: bool,
}

pub fn camera_control(
    mut drag: ResMut<DragState>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut wheel: MessageReader<MouseWheel>,
    keys: Res<ButtonInput<KeyCode>>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut cam: Query<(&mut Transform, &mut Projection), With<Camera2d>>,
    time: Res<Time>,
) {
    let Ok((mut tf, mut proj)) = cam.single_mut() else {
        return;
    };
    let Projection::Orthographic(ortho) = &mut *proj else {
        return;
    };
    for ev in wheel.read() {
        ortho.scale = (ortho.scale * (1.0 - ev.y * 0.08)).clamp(0.35, 2.5);
    }
    let mut dir = Vec2::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        dir.y += 1.0;
    }
    if keys.pressed(KeyCode::KeyS) {
        dir.y -= 1.0;
    }
    if keys.pressed(KeyCode::KeyA) {
        dir.x -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) {
        dir.x += 1.0;
    }
    if dir != Vec2::ZERO {
        tf.translation +=
            (dir.normalize() * 700.0 * ortho.scale * time.delta_secs()).extend(0.0);
    }
    let cur = window.cursor_position();
    if buttons.just_pressed(MouseButton::Left) {
        if let Some(p) = cur {
            *drag = DragState {
                start: p,
                last: p,
                active: true,
                moved: false,
            };
        }
    }
    if buttons.pressed(MouseButton::Left) && drag.active {
        if let Some(p) = cur {
            if (p - drag.start).length() > 6.0 {
                drag.moved = true;
            }
            if drag.moved {
                let d = (p - drag.last) * ortho.scale;
                tf.translation.x -= d.x;
                tf.translation.y += d.y;
                drag.last = p;
            }
        }
    }
    if buttons.just_released(MouseButton::Left) {
        drag.active = false;
    }
}

pub fn hover(
    window: Single<&Window, With<PrimaryWindow>>,
    cam: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    map: Res<GameMap>,
    mut hovered: ResMut<Hovered>,
    mut gizmos: Gizmos,
    ui: Query<&Interaction, With<Button>>,
) {
    // The pointer is over a UI button: map picking is off.
    if ui.iter().any(|i| *i != Interaction::None) {
        hovered.0 = None;
        return;
    }
    let Ok((camera, gt)) = cam.single() else {
        return;
    };
    let Some(cur) = window.cursor_position() else {
        hovered.0 = None;
        return;
    };
    let Ok(world) = camera.viewport_to_world_2d(gt, cur) else {
        hovered.0 = None;
        return;
    };
    let Some((x, y)) = world_to_tile(&map, world) else {
        hovered.0 = None;
        return;
    };
    hovered.0 = Some((x, y));
    let c = tile_to_world(x, y);
    let hw = TILE_W / 2.0;
    let hh = TILE_H / 2.0;
    let corners = [
        Vec2::new(c.x, c.y + hh),
        Vec2::new(c.x + hw, c.y),
        Vec2::new(c.x, c.y - hh),
        Vec2::new(c.x - hw, c.y),
    ];
    for i in 0..4 {
        gizmos.line_2d(corners[i], corners[(i + 1) % 4], Color::WHITE);
    }
}

fn move_order(map: &GameMap, units: &mut Query<(Entity, &mut Unit)>, s: Entity, dest: (i32, i32)) {
    if let Ok((_, mut u)) = units.get_mut(s) {
        units::order_move(map, &mut u, dest);
    }
}

fn order_with_sfx(
    commands: &mut Commands,
    audio: &GameAudio,
    map: &GameMap,
    units: &mut Query<(Entity, &mut Unit)>,
    s: Entity,
    dest: (i32, i32),
) {
    if let Ok((_, u)) = units.get(s) {
        if u.moves > 0 {
            if let Some(h) = audio.run.get(&u.utype) {
                commands.spawn(AudioPlayer(h.clone()));
            }
        }
    }
    move_order(map, units, s, dest);
}

pub fn orders(
    mut commands: Commands,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    hovered: Res<Hovered>,
    drag: Res<DragState>,
    mut selected: ResMut<Selected>,
    mut units: Query<(Entity, &mut Unit)>,
    map: Res<GameMap>,
    mut turn_end: MessageWriter<TurnEnded>,
    mut reveal: ResMut<RevealAll>,
    cities: Query<(Entity, &City)>,
    mut view: ResMut<CityView>,
    audio: Res<GameAudio>,
    splash: Res<SplashUp>,
    mut goto: ResMut<GotoMode>,
    mut cmds: MessageWriter<UnitCommand>,
) {
    if view.0.is_some() || splash.0 {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        goto.0 = false;
    }
    for c in key_commands(&keys) {
        cmds.write(c);
    }
    // V: open the city under the selected unit.
    if keys.just_pressed(KeyCode::KeyV) {
        let at = selected.0.and_then(|s| units.get(s).ok()).map(|(_, u)| (u.x, u.y));
        if let Some((e, _)) = cities.iter().find(|(_, c)| Some((c.x, c.y)) == at) {
            view.0 = Some(e);
            audio::sfx(&mut commands, &audio, "City View");
        }
    }
    if buttons.just_released(MouseButton::Left) && !drag.moved {
        if let Some((x, y)) = hovered.0 {
            if goto.0 {
                goto.0 = false;
                if let Some(s) = selected.0 {
                    order_with_sfx(&mut commands, &audio, &map, &mut units, s, (x, y));
                }
                return;
            }
            // Units that spent their moves cannot be selected, as in Civ3.
            let stack: Vec<Entity> = units
                .iter()
                .filter(|(_, u)| u.x == x && u.y == y && units::selectable(u))
                .map(|(e, _)| e)
                .collect();
            if let Some((e, _)) = cities.iter().find(|(_, c)| c.x == x && c.y == y)
            {
                // cities open first, as in Civ3; garrisoned units cycle via Tab
                view.0 = Some(e);
                audio::sfx(&mut commands, &audio, "City View");
            } else if !stack.is_empty() {
                // selecting a fortified unit wakes it, as in Civ3; selecting
                // an explorer stands down auto-explore, or a later click
                // could never redirect it (auto-select gives the tile order
                // to whoever still needs orders).
                let next = match selected.0 {
                    Some(s) if stack.contains(&s) => {
                        let i = stack.iter().position(|&e| e == s).unwrap();
                        stack[(i + 1) % stack.len()]
                    }
                    _ => stack[0],
                };
                if let Ok((_, mut u)) = units.get_mut(next) {
                    u.fortified = false;
                    u.sentry = false;
                    u.exploring = false;
                }
                selected.0 = Some(next);
                audio::sfx(&mut commands, &audio, "Select");
            } else if let Some(s) = selected.0 {
                order_with_sfx(&mut commands, &audio, &map, &mut units, s, (x, y));
            }
        }
    }
    if buttons.just_pressed(MouseButton::Right) {
        if let (Some(dest), Some(s)) = (hovered.0, selected.0) {
            order_with_sfx(&mut commands, &audio, &map, &mut units, s, dest);
        }
    }
    if let Some(s) = selected.0 {
        let step = if keys.just_pressed(KeyCode::ArrowUp) {
            Some((0, -1))
        } else if keys.just_pressed(KeyCode::ArrowDown) {
            Some((0, 1))
        } else if keys.just_pressed(KeyCode::ArrowLeft) {
            Some((-1, 0))
        } else if keys.just_pressed(KeyCode::ArrowRight) {
            Some((1, 0))
        } else {
            None
        };
        if let Some((dx, dy)) = step {
            if let Ok((_, u)) = units.get(s) {
                let dest = (map.wrap_x(u.x + dx), (u.y + dy).clamp(0, map.h - 1));
                order_with_sfx(&mut commands, &audio, &map, &mut units, s, dest);
            }
        }
    }
    if keys.just_pressed(KeyCode::Tab) {
        let cands: Vec<Entity> = units
            .iter()
            .filter(|(_, u)| units::needs_orders(u))
            .map(|(e, _)| e)
            .collect();
        if !cands.is_empty() {
            let next = match selected.0 {
                Some(s) if cands.contains(&s) => {
                    let i = cands.iter().position(|&e| e == s).unwrap();
                    cands[(i + 1) % cands.len()]
                }
                _ => cands[0],
            };
            selected.0 = Some(next);
        }
    }
    if keys.just_pressed(KeyCode::Enter) {
        turn_end.write(TurnEnded);
        audio::sfx(&mut commands, &audio, "EnterTurn");
    }
    if keys.just_pressed(KeyCode::F9) {
        reveal.0 = !reveal.0;
    }
}
