//! Camera control and tile hover picking.
//!
//! Panning is keys and wheel only: Civ3 ties the left button to movement,
//! so a press and drag never scrolls the map.
//!
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::actionbar::{GotoMode, UnitCommand, key_commands};
use crate::audio::{self, GameAudio};
use crate::cities::{City, CityView};
use crate::map::*;
use crate::render::RevealAll;
use crate::splash::SplashUp;
use crate::units::{self, Selected, TurnEnded, Unit};

#[derive(Resource, Default)]
pub struct Hovered(pub Option<(i32, i32)>);

/// How long the left button must stay down on a tile before the move
/// preview appears. Civ3 waits about this long; a quicker click still
/// orders the move, it just never draws the route.
pub const HOLD_SECS: f32 = 0.3;

/// Destination the route is drawn for, or None for no preview. Civ3 shows
/// the path and the destination tile only while the Go-to command is armed
/// or while the button is held on a tile, never on a plain hover.
#[derive(Resource, Default)]
pub struct MovePreview(pub Option<(i32, i32)>);

/// Scripted runs aim the hover without a mouse.
#[derive(Resource, Default)]
pub struct HoverPin(pub Option<(i32, i32)>);

#[derive(Default)]
pub(crate) struct Hold {
    /// A press started while a unit was selected.
    armed: bool,
    secs: f32,
}

pub fn camera_control(
    mut wheel: MessageReader<MouseWheel>,
    keys: Res<ButtonInput<KeyCode>>,
    mut cam: Query<(&mut Transform, &mut Projection), With<Camera2d>>,
    time: Res<Time>,
    view: Res<CityView>,
) {
    // The city screen holds the camera on its city (`frame_city_view`).
    if view.0.is_some() {
        wheel.clear();
        return;
    }
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
        tf.translation += (dir.normalize() * 700.0 * ortho.scale * time.delta_secs()).extend(0.0);
    }
}

pub fn hover(
    window: Single<&Window, With<PrimaryWindow>>,
    cam: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    map: Res<GameMap>,
    mut hovered: ResMut<Hovered>,
    mut gizmos: Gizmos,
    ui: Query<&Interaction, With<Button>>,
    pin: Res<HoverPin>,
    selected: Res<Selected>,
    preview: Res<MovePreview>,
    view: Res<CityView>,
) {
    // The city screen's band shows the map, but the map is not in play.
    if view.0.is_some() {
        hovered.0 = None;
        return;
    }
    // The pointer is over a UI button: map picking is off.
    let tile = if let Some(p) = pin.0 {
        Some(p)
    } else if ui.iter().any(|i| *i != Interaction::None) {
        None
    } else {
        let picked = cam.single().ok().and_then(|(camera, gt)| {
            let cur = window.cursor_position()?;
            let world = camera.viewport_to_world_2d(gt, cur).ok()?;
            world_to_tile(&map, world)
        });
        picked
    };
    hovered.0 = tile;
    let Some((x, y)) = tile else {
        return;
    };
    // With a unit selected the tile cursor is the move preview, so Civ3
    // shows nothing until the route is actually being aimed (Go-to armed
    // or the button held).
    if selected.0.is_some() && preview.0 != Some((x, y)) {
        return;
    }
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

/// Work out whether the route is being aimed, and for which tile: the
/// armed Go-to command previews under the pointer, and otherwise only a
/// press held on a tile with a unit selected does, the way Civ3 shows it
/// before the button comes back up. Sliding the pointer while held just
/// moves the destination.
pub fn hold_preview(
    time: Res<Time>,
    buttons: Res<ButtonInput<MouseButton>>,
    hovered: Res<Hovered>,
    selected: Res<Selected>,
    units: Query<&Unit>,
    goto: Res<GotoMode>,
    bombard: Res<crate::bombard::TargetMode>,
    mut hold: Local<Hold>,
    mut preview: ResMut<MovePreview>,
) {
    if bombard.0.is_some() {
        preview.0 = None;
        hold.armed = false;
        return;
    }
    let from = selected
        .0
        .and_then(|s| units.get(s).ok())
        .map(|u| (u.x, u.y));
    // A press only arms while a unit is selected; the timer keeps running
    // until the button comes up.
    if buttons.just_pressed(MouseButton::Left) && !hold.armed {
        hold.armed = from.is_some();
        hold.secs = 0.0;
    }
    if !buttons.pressed(MouseButton::Left) {
        hold.armed = false;
        hold.secs = 0.0;
    } else if hold.armed {
        hold.secs += time.delta_secs();
    }
    let aiming = goto.0 || (hold.armed && hold.secs >= HOLD_SECS);
    preview.0 = match (aiming, hovered.0, from) {
        (true, Some(dest), Some(here)) if dest != here => Some(dest),
        _ => None,
    };
}

fn move_order(map: &GameMap, units: &mut Query<(Entity, &mut Unit)>, s: Entity, dest: (i32, i32), ports: &[(i32, i32)]) {
    let snapshot: Vec<_> = units.iter().map(|(e, u)| (e, u.clone())).collect();
    if let Ok((_, mut u)) = units.get_mut(s) {
        crate::naval::order_move(map, &mut u, dest, ports, &snapshot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::{UnitAnim, UnitType};
    use std::collections::VecDeque;
    use std::time::Duration;

    fn unit_at(x: i32, y: i32) -> Unit {
        Unit {
            civ: 0,
            utype: UnitType::Scout,
            x,
            y,
            moves: 3,
            fortified: false,
            facing: 0,
            path: VecDeque::new(),
            anim: UnitAnim::Idle { t: 0.0 },
            work: None,
            sentry: false,
            exploring: false,
            ..Unit::new(0, UnitType::Scout, x, y)
        }
    }

    /// A scout at (10, 10) with the pointer pinned to (12, 12).
    fn app(selected: bool) -> App {
        let mut app = App::new();
        app.insert_resource(Hovered(Some((12, 12))));
        app.insert_resource(GotoMode(false));
        app.init_resource::<crate::bombard::TargetMode>();
        app.insert_resource(ButtonInput::<MouseButton>::default());
        app.insert_resource(MovePreview::default());
        app.insert_resource(Time::<()>::default());
        let e = app.world_mut().spawn(unit_at(10, 10)).id();
        app.insert_resource(Selected(if selected { Some(e) } else { None }));
        app.add_systems(Update, hold_preview);
        app
    }

    fn preview(app: &App) -> Option<(i32, i32)> {
        app.world().resource::<MovePreview>().0
    }

    fn press(app: &mut App) {
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
    }

    fn wait(app: &mut App, secs: f32) {
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(secs));
        app.update();
    }

    #[test]
    fn hovering_a_tile_previews_nothing() {
        let mut app = app(true);
        app.update();
        assert_eq!(preview(&app), None);
    }

    #[test]
    fn a_held_press_previews_the_route() {
        let mut app = app(true);
        press(&mut app);
        app.update();
        // Still under the delay: Civ3 waits before it draws the path.
        assert_eq!(preview(&app), None);
        wait(&mut app, HOLD_SECS + 0.05);
        assert_eq!(preview(&app), Some((12, 12)));
    }

    #[test]
    fn a_quick_click_never_previews() {
        let mut app = app(true);
        press(&mut app);
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .release(MouseButton::Left);
        wait(&mut app, HOLD_SECS + 0.05);
        assert_eq!(preview(&app), None);
    }

    #[test]
    fn the_preview_follows_the_pointer_while_held() {
        let mut app = app(true);
        press(&mut app);
        wait(&mut app, HOLD_SECS + 0.05);
        assert_eq!(preview(&app), Some((12, 12)));
        // Sliding to another tile while the button is still down aims there.
        app.world_mut().resource_mut::<Hovered>().0 = Some((11, 13));
        app.update();
        assert_eq!(preview(&app), Some((11, 13)));
    }

    #[test]
    fn holding_with_no_unit_selected_does_nothing() {
        let mut app = app(false);
        press(&mut app);
        wait(&mut app, HOLD_SECS + 0.05);
        assert_eq!(preview(&app), None);
    }

    #[test]
    fn the_armed_goto_previews_on_hover() {
        let mut app = app(true);
        app.world_mut().resource_mut::<GotoMode>().0 = true;
        app.update();
        assert_eq!(preview(&app), Some((12, 12)));
    }

    #[test]
    fn the_units_own_tile_is_no_destination() {
        let mut app = app(true);
        app.world_mut().resource_mut::<Hovered>().0 = Some((10, 10));
        app.world_mut().resource_mut::<GotoMode>().0 = true;
        app.update();
        assert_eq!(preview(&app), None);
    }
}

fn order_with_sfx(
    commands: &mut Commands,
    audio: &GameAudio,
    map: &GameMap,
    units: &mut Query<(Entity, &mut Unit)>,
    s: Entity,
    dest: (i32, i32),
    ports: &[(i32, i32)],
) {
    if let Ok((_, u)) = units.get(s) {
        if u.moves > 0 {
            if let Some(h) = audio.run.get(&u.utype) {
                commands.spawn(AudioPlayer(h.clone()));
            }
        }
    }
    move_order(map, units, s, dest, ports);
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct Targeting<'w> {
    goto: ResMut<'w, GotoMode>,
    bombard: ResMut<'w, crate::bombard::TargetMode>,
    shots: MessageWriter<'w, crate::bombard::Order>,
}

pub fn orders(
    mut commands: Commands,
    civs: Res<crate::civs::Civilizations>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    hovered: Res<Hovered>,
    mut selected: ResMut<Selected>,
    mut units: Query<(Entity, &mut Unit)>,
    map: Res<GameMap>,
    mut turn_end: MessageWriter<TurnEnded>,
    mut reveal: ResMut<RevealAll>,
    cities: Query<(Entity, &City)>,
    mut view: ResMut<CityView>,
    audio: Res<GameAudio>,
    splash: Res<SplashUp>,
    mut targeting: Targeting,
    mut cmds: MessageWriter<UnitCommand>,
) {
    let ports: Vec<_> = cities.iter().filter(|(_, c)| c.civ == civs.active && c.coastal).map(|(_, c)| (c.x, c.y)).collect();
    if view.0.is_some() || splash.0 {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        targeting.goto.0 = false;
        targeting.bombard.0 = None;
    }
    if targeting.bombard.0 != selected.0 { targeting.bombard.0 = None; }
    for c in key_commands(&keys) {
        cmds.write(c);
    }
    // V: open the city under the selected unit.
    if keys.just_pressed(KeyCode::KeyV) {
        let at = selected
            .0
            .and_then(|s| units.get(s).ok())
            .map(|(_, u)| (u.x, u.y));
        if let Some((e, _)) = cities
            .iter()
            .find(|(_, c)| c.civ == civs.active && Some((c.x, c.y)) == at)
        {
            view.0 = Some(e);
            audio::sfx(&mut commands, &audio, "City View");
        }
    }
    if buttons.just_released(MouseButton::Left) {
        if let Some((x, y)) = hovered.0 {
            if let Some(attacker) = targeting.bombard.0.take() {
                targeting.shots.write(crate::bombard::Order { attacker, to: (x, y) });
                return;
            }
            if targeting.goto.0 {
                targeting.goto.0 = false;
                if let Some(s) = selected.0 {
                    order_with_sfx(&mut commands, &audio, &map, &mut units, s, (x, y), &ports);
                }
                return;
            }
            let top = units
                .iter()
                .filter(|(_, u)| u.x == x && u.y == y)
                .max_by_key(|(e, u)| {
                    (
                        units::stack_priority(*e, u, selected.0),
                        std::cmp::Reverse(*e),
                    )
                })
                .filter(|(_, u)| u.civ == civs.active && units::selectable(u))
                .map(|(e, _)| e);
            if let Some((e, _)) = cities
                .iter()
                .find(|(_, c)| c.civ == civs.active && c.x == x && c.y == y)
            {
                // Cities open first; right-click selects garrisoned units.
                view.0 = Some(e);
                audio::sfx(&mut commands, &audio, "City View");
            } else if let Some(next) = top {
                // selecting a fortified unit wakes it, as in Civ3; selecting
                // an explorer stands down auto-explore, or a later click
                // could never redirect it (auto-select gives the tile order
                // to whoever still needs orders).
                if let Ok((_, mut u)) = units.get_mut(next) {
                    u.fortified = false;
                    u.sentry = false;
                    u.exploring = false;
                    u.auto = false;
                }
                selected.0 = Some(next);
                audio::sfx(&mut commands, &audio, "Select");
            } else if let Some(s) = selected.0 {
                order_with_sfx(&mut commands, &audio, &map, &mut units, s, (x, y), &ports);
            }
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
            targeting.bombard.0 = None;
            if let Ok((_, u)) = units.get(s) {
                let dest = (map.wrap_x(u.x + dx), (u.y + dy).clamp(0, map.h - 1));
                order_with_sfx(&mut commands, &audio, &map, &mut units, s, dest, &ports);
            }
        }
    }
    if keys.just_pressed(KeyCode::Tab) {
        let cands: Vec<Entity> = units
            .iter()
            .filter(|(_, u)| u.civ == civs.active && units::needs_orders(u))
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
    if keys.just_pressed(KeyCode::Enter) && !crate::civs::is_ai(civs.active) {
        turn_end.write(TurnEnded);
        audio::sfx(&mut commands, &audio, "EnterTurn");
    }
    if keys.just_pressed(KeyCode::F9) {
        reveal.0 = !reveal.0;
    }
}
