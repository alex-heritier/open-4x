//! Right-click selection of individual units without issuing a move order.
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::actionbar::GotoMode;
use crate::audio::{self, GameAudio};
use crate::cities::CityView;
use crate::civs::Civilizations;
use crate::input::{Hovered, MovePreview};
use crate::production_prompt::ProductionPrompts;
use crate::splash::SplashUp;
use crate::units::{self, Selected, Unit};

#[derive(Resource, Default)]
pub struct UnitPicker {
    root: Option<Entity>,
    /// Native port DISEMBARK dialog, requested by the ship's Unload command.
    pub unload: Option<Entity>,
    /// Destination passed by the ship mover; None means same-tile port unload.
    pub shore: Option<(i32, i32)>,
    /// Consume the closing click through its release, which otherwise moves.
    consume_click: bool,
    owner: usize,
}

#[derive(Component)]
pub(crate) struct PickerRow(pub Entity);

#[derive(Component)]
pub(crate) struct PickerBackdrop;

#[derive(Component)]
pub(crate) struct PickerList;

pub fn inactive(picker: Res<UnitPicker>) -> bool {
    picker.root.is_none() && picker.unload.is_none() && !picker.consume_click
}

pub fn update(
    mut commands: Commands,
    mut picker: ResMut<UnitPicker>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    pointer: (Res<Hovered>, ResMut<MovePreview>, Res<crate::map::GameMap>),
    civs: Res<Civilizations>,
    mut selected: ResMut<Selected>,
    mut units: Query<(Entity, &mut Unit)>,
    window: Single<&Window, With<PrimaryWindow>>,
    assets: Res<AssetServer>,
    audio: Res<GameAudio>,
    view: Res<CityView>,
    splash: Res<SplashUp>,
    prompts: Res<ProductionPrompts>,
    mut goto: ResMut<GotoMode>,
    interactions: Query<
        (&Interaction, Option<&PickerRow>),
        Or<(With<PickerRow>, With<PickerBackdrop>)>,
    >,
) {
    let (hovered, mut preview, map) = pointer;
    if !buttons.pressed(MouseButton::Left) && !buttons.just_released(MouseButton::Left) {
        picker.consume_click = false;
    }
    let blocked = view.0.is_some() || splash.0 || prompts.blocks(civs.active);
    if picker.root.is_some() {
        let pressed = interactions.iter().any(|(i, _)| *i == Interaction::Pressed);
        let close = blocked
            || picker.owner != civs.active
            || pressed
            || keys.just_pressed(KeyCode::Escape)
            || buttons.just_pressed(MouseButton::Right);
        if close {
            if !blocked && picker.owner == civs.active {
                if let Some(row) = interactions
                    .iter()
                    .find_map(|(i, row)| (*i == Interaction::Pressed).then_some(row).flatten())
                {
                    if picker.unload == Some(row.0) {
                        for (_, mut u) in &mut units {
                            if u.civ == civs.active && u.carrier == picker.unload {
                                crate::naval::disembark(&map, &mut u, picker.shore);
                            }
                        }
                    } else if let Ok((_, mut u)) = units.get_mut(row.0) {
                        let unloading = picker.unload.is_some() && u.carrier == picker.unload;
                        if u.civ == civs.active && (unloading || (picker.unload.is_none() && units::selectable(&u)))
                            && (!unloading || crate::naval::disembark(&map, &mut u, picker.shore)) {
                            u.fortified = false;
                            u.sentry = false;
                            u.exploring = false;
                            u.auto = false;
                            selected.0 = Some(row.0);
                            audio::sfx(&mut commands, &audio, "Select");
                        }
                    }
                }
            }
            commands.entity(picker.root.take().unwrap()).despawn();
            picker.unload = None;
            picker.shore = None;
            picker.consume_click = true;
        }
        return;
    }
    if blocked {
        picker.unload = None;
        picker.shore = None;
        return;
    }
    if picker.unload.is_none() && !buttons.just_pressed(MouseButton::Right) {
        return;
    }
    let at = picker.unload.and_then(|e| units.get(e).ok().map(|(_, u)| (u.x, u.y))).or(hovered.0);
    let Some((x, y)) = at else {
        picker.unload = None;
        picker.shore = None;
        return;
    };
    let mut stack: Vec<_> = units
        .iter()
        .filter(|(_, u)| u.civ == civs.active && (u.x, u.y) == (x, y)
            && picker.unload.is_none_or(|ship| u.carrier == Some(ship)))
        .collect();
    if stack.is_empty() {
        picker.unload = None;
        picker.shore = None;
        return;
    }
    stack.sort_by_key(|(e, u)| {
        (
            std::cmp::Reverse(units::stack_priority(*e, u, selected.0)),
            *e,
        )
    });
    goto.0 = false;
    preview.0 = None;
    picker.owner = civs.active;
    let width = 300.0_f32.min(window.width());
    let height = (36.0 + (stack.len() + usize::from(picker.unload.is_some())) as f32 * 28.0).min(window.height() * 0.75);
    let pos = window
        .cursor_position()
        .unwrap_or(Vec2::new(window.width() / 2.0, window.height() / 2.0));
    let font = assets.load("cache/fonts/lsans.ttf");
    let root = commands
        .spawn((
            PickerBackdrop,
            Button,
            GlobalZIndex(90),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(pos.x.clamp(0.0, (window.width() - width).max(0.0))),
                    top: Val::Px(pos.y.clamp(0.0, (window.height() - height).max(0.0))),
                    width: Val::Px(width),
                    max_height: Val::Px(height),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(Val::Px(6.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BackgroundColor(Color::srgb(1.0, 0.98, 0.86)),
                BorderColor::all(Color::srgb(0.45, 0.42, 0.3)),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new(if picker.unload.is_some() { "Disembark" } else { "Select Unit" }),
                    TextFont {
                        font: font.clone(),
                        font_size: 16.0,
                        ..default()
                    },
                    TextColor(Color::BLACK),
                ));
                panel
                    .spawn((
                        PickerList,
                        ScrollPosition::default(),
                        Node {
                            flex_direction: FlexDirection::Column,
                            overflow: Overflow::scroll_y(),
                            min_height: Val::Px(0.0),
                            ..default()
                        },
                    ))
                    .with_children(|list| {
                        if let Some(ship) = picker.unload {
                            list.spawn((Button, PickerRow(ship), Node {
                                min_height: Val::Px(28.0), flex_shrink: 0.0,
                                padding: UiRect::axes(Val::Px(4.0), Val::Px(3.0)), ..default()
                            }, BackgroundColor(Color::NONE)))
                                .with_child((Text::new("Unload all"), TextFont { font: font.clone(), font_size: 15.0, ..default() }, TextColor(Color::BLACK)));
                        }
                        for (e, u) in &stack {
                            let enabled = if picker.unload.is_some() { picker.shore.is_none() || u.moves > 0 } else { units::selectable(u) };
                            let state = if u.fortified {
                                " · Fortified"
                            } else if u.sentry {
                                " · Sentry"
                            } else {
                                ""
                            };
                            list.spawn((
                                Button,
                                PickerRow(*e),
                                Node {
                                    min_height: Val::Px(28.0),
                                    flex_shrink: 0.0,
                                    padding: UiRect::axes(Val::Px(4.0), Val::Px(3.0)),
                                    ..default()
                                },
                                BackgroundColor(Color::NONE),
                            ))
                            .with_children(|row| {
                                row.spawn((
                                    Text::new(format!(
                                        "{}  ({}/{} moves){}",
                                        if u.scientific_leader { "Scientific Leader" } else { units::def(u.utype).name },
                                        units::fmt_moves(u.moves),
                                        units::def(u.utype).moves,
                                        state
                                    )),
                                    TextFont {
                                        font: font.clone(),
                                        font_size: 15.0,
                                        ..default()
                                    },
                                    TextColor(if enabled {
                                        Color::BLACK
                                    } else {
                                        Color::srgb(0.5, 0.5, 0.45)
                                    }),
                                ));
                            });
                        }
                    });
            });
        })
        .id();
    picker.root = Some(root);
}

pub fn style_and_scroll(
    mut rows: Query<(&Interaction, &mut BackgroundColor), With<PickerRow>>,
    mut lists: Query<&mut ScrollPosition, With<PickerList>>,
    mut wheel: MessageReader<MouseWheel>,
) {
    for (interaction, mut color) in &mut rows {
        color.0 = if *interaction != Interaction::None {
            Color::srgb(0.86, 0.84, 0.7)
        } else {
            Color::NONE
        };
    }
    for event in wheel.read() {
        for mut pos in &mut lists {
            let dy = match event.unit {
                bevy::input::mouse::MouseScrollUnit::Line => event.y * 28.0,
                bevy::input::mouse::MouseScrollUnit::Pixel => event.y,
            };
            pos.y = (pos.y - dy).max(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::{TurnEnded, UnitAnim, UnitType};

    fn app() -> (App, Entity, Entity) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default()));
        app.init_asset::<Font>();
        app.init_resource::<UnitPicker>();
        app.init_resource::<Civilizations>();
        app.init_resource::<ButtonInput<MouseButton>>();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_resource::<GotoMode>();
        app.init_resource::<crate::bombard::TargetMode>();
        app.add_message::<crate::bombard::Order>();
        app.init_resource::<MovePreview>();
        app.init_resource::<CityView>();
        app.init_resource::<ProductionPrompts>();
        app.insert_resource(SplashUp(false));
        app.insert_resource(Hovered(Some((10, 10))));
        app.insert_resource(crate::map::GameMap::generate());
        app.insert_resource(crate::render::RevealAll(true));
        app.add_message::<TurnEnded>();
        app.add_message::<crate::actionbar::UnitCommand>();
        app.insert_resource(GameAudio {
            menu: default(),
            peace: default(),
            ui: default(),
            run: default(),
            build: default(),
            fortify: default(),
            work_road: default(),
            work_irrigate: default(),
            work_mine: default(),
            work_clear: default(),
            music: None,
        });
        app.world_mut().spawn((Window::default(), PrimaryWindow));
        let mut spawn = |utype| {
            app.world_mut()
                .spawn(Unit {
                    civ: 0,
                    utype,
                    x: 10,
                    y: 10,
                    moves: 3,
                    fortified: false,
                    facing: 0,
                    path: default(),
                    anim: UnitAnim::Idle { t: 0.0 },
                    work: None,
                    sentry: false,
                    exploring: false,
                    ..Unit::new(0, utype, 10, 10)
                })
                .id()
        };
        let warrior = spawn(UnitType::named("Warrior"));
        let worker = spawn(UnitType::named("Worker"));
        app.insert_resource(Selected(None));
        app.add_systems(
            Update,
            (
                update,
                crate::input::orders.run_if(inactive),
                units::auto_select,
            )
                .chain(),
        );
        (app, warrior, worker)
    }

    fn left_click(app: &mut App) {
        let mut mouse = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
        mouse.clear();
        mouse.press(MouseButton::Left);
        mouse.release(MouseButton::Left);
        app.update();
    }

    #[test]
    fn bombard_key_then_target_click_fires_without_ordering_a_move() {
        let (mut app, a, _) = app();
        app.edit_schedule(Update, |s| { s.set_executor_kind(bevy::ecs::schedule::ExecutorKind::SingleThreaded); });
        app.world_mut().get_mut::<Unit>(a).unwrap().utype = UnitType::named("Catapult");
        app.world_mut().resource_mut::<Selected>().0 = Some(a);
        let mut diplomacy = crate::diplomacy::Diplomacy::new();
        diplomacy.declare(&crate::diplomacy::Facts::even(), 0, 1, 0, &mut crate::features::MessageBoard::default());
        app.insert_resource(diplomacy);
        app.insert_resource(crate::combat::CombatRng(crate::rng::MapRng::new(1)));
        app.init_resource::<crate::features::MessageBoard>();
        app.add_systems(Update, (crate::bombard::arm, crate::bombard::resolve).chain().after(crate::input::orders));
        app.world_mut().spawn(Unit::new(1, UnitType::named("Spearman"), 11, 10));
        {
            let mut map = app.world_mut().resource_mut::<crate::map::GameMap>();
            let i = map.idx(11, 10);
            map.tiles[i].visible = true;
        }
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyB);
        app.update();
        assert_eq!(app.world().resource::<crate::bombard::TargetMode>().0, Some(a));
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().clear();
        app.world_mut().resource_mut::<Hovered>().0 = Some((11, 10));
        left_click(&mut app);
        let u = app.world().get::<Unit>(a).unwrap();
        assert_eq!((u.x, u.y, u.moves, u.attacked), (10, 10, 0, true));
        assert!(u.path.is_empty());
        assert_eq!(app.world().resource::<crate::bombard::TargetMode>().0, None);
    }

    fn open(app: &mut App) {
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Right);
        app.update();
        let mut mouse = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
        mouse.release(MouseButton::Right);
        mouse.clear();
    }

    #[test]
    fn repeated_left_click_selects_displayed_unit_without_cycling() {
        let (mut app, warrior, worker) = app();
        for _ in 0..3 {
            left_click(&mut app);
            assert_eq!(app.world().resource::<Selected>().0, Some(warrior));
        }
        app.world_mut().get_mut::<Unit>(warrior).unwrap().moves = 0;
        left_click(&mut app);
        assert_eq!(app.world().resource::<Selected>().0, Some(worker));
    }

    #[test]
    fn picker_selects_specific_unit_and_consumes_closing_click() {
        let (mut app, warrior, worker) = app();
        app.world_mut().resource_mut::<Selected>().0 = Some(warrior);
        app.world_mut().get_mut::<Unit>(worker).unwrap().fortified = true;
        open(&mut app);
        assert!(app.world().resource::<UnitPicker>().root.is_some());
        assert!(app.world().get::<Unit>(warrior).unwrap().path.is_empty());
        let row = app
            .world_mut()
            .query::<(Entity, &PickerRow)>()
            .iter(app.world())
            .find(|(_, row)| row.0 == worker)
            .unwrap()
            .0;
        *app.world_mut().get_mut::<Interaction>(row).unwrap() = Interaction::Pressed;
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();
        assert_eq!(app.world().resource::<Selected>().0, Some(worker));
        assert!(!app.world().get::<Unit>(worker).unwrap().fortified);
        assert!(app.world().resource::<UnitPicker>().root.is_none());
        app.world_mut().resource_mut::<Hovered>().0 = Some((12, 12));
        let mut mouse = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
        mouse.clear();
        mouse.release(MouseButton::Left);
        app.update();
        assert!(app.world().get::<Unit>(worker).unwrap().path.is_empty());
    }

    #[test]
    fn ship_to_shore_dialog_moves_chosen_cargo_and_leaves_ship_offshore() {
        for choice in [None, Some(false), Some(true)] {
            let (mut app, ship, worker) = app();
            {
                let mut map = app.world_mut().resource_mut::<crate::map::GameMap>();
                for t in &mut map.tiles { t.base = crate::map::Base::Grassland; }
                let i = map.idx(10, 10);
                map.tiles[i].base = crate::map::Base::Coast;
            }
            app.insert_resource(crate::diplomacy::Diplomacy::new());
            app.init_resource::<crate::cities::Treasury>();
            app.add_message::<crate::combat::AttackOrder>();
            app.add_systems(Update, (units::drive_movement, crate::naval::sync_cargo).chain().after(update));
            {
                let mut u = app.world_mut().get_mut::<Unit>(worker).unwrap();
                u.carrier = Some(ship);
                u.sentry = true;
            }
            let mut settler = Unit::new(0, UnitType::named("Settler"), 10, 10);
            settler.carrier = Some(ship);
            settler.sentry = true;
            let other = app.world_mut().spawn(settler).id();
            let mut exhausted = Unit::new(0, UnitType::named("Warrior"), 10, 10);
            exhausted.carrier = Some(ship);
            exhausted.moves = 0;
            let exhausted = app.world_mut().spawn(exhausted).id();
            let map = app.world().resource::<crate::map::GameMap>().clone();
            let snapshot: Vec<_> = app.world_mut().query::<(Entity, &Unit)>().iter(app.world()).map(|(e, u)| (e, u.clone())).collect();
            {
                let mut u = app.world_mut().get_mut::<Unit>(ship).unwrap();
                u.utype = UnitType::named("Galley");
                u.moves = 9;
                crate::naval::order_move(&map, &mut u, (11, 10), &[], &snapshot);
                assert_eq!(u.path.front(), Some(&(11, 10)));
            }
            app.update();
            assert_eq!(app.world().resource::<UnitPicker>().shore, Some((11, 10)));
            app.update();
            assert!(app.world().resource::<UnitPicker>().root.is_some());
            if let Some(all) = choice {
                let target = if all { ship } else { worker };
                let row = app.world_mut().query::<(Entity, &PickerRow)>().iter(app.world()).find(|(_, row)| row.0 == target).unwrap().0;
                *app.world_mut().get_mut::<Interaction>(row).unwrap() = Interaction::Pressed;
            } else {
                app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Escape);
            }
            app.update();
            let u = app.world().get::<Unit>(ship).unwrap();
            assert_eq!((u.x, u.y, u.moves), (10, 10, 9));
            assert!(u.path.is_empty());
            for (cargo, landed) in [(worker, choice.is_some()), (other, choice == Some(true)), (exhausted, false)] {
                let u = app.world().get::<Unit>(cargo).unwrap();
                assert_eq!((u.x, u.y, u.carrier), if landed { (11, 10, None) } else { (10, 10, Some(ship)) });
                if landed { assert_eq!(u.moves, 0); }
            }
            assert!(app.world().resource::<UnitPicker>().unload.is_none());
            assert!(app.world().resource::<UnitPicker>().shore.is_none());
        }
    }

    #[test]
    fn passenger_selection_wakes_at_sea_and_disembarks_in_port_without_movement() {
        for in_port in [false, true] {
            let (mut app, ship, worker) = app();
            app.world_mut().get_mut::<Unit>(ship).unwrap().utype = UnitType::named("Galley");
            {
                let mut u = app.world_mut().get_mut::<Unit>(worker).unwrap();
                u.carrier = Some(ship);
                u.sentry = true;
                u.moves = if in_port { 0 } else { 3 };
            }
            let mut passenger = Unit::new(0, UnitType::named("Settler"), 10, 10);
            passenger.carrier = Some(ship);
            let other = app.world_mut().spawn(passenger).id();
            if in_port {
                app.world_mut().resource_mut::<UnitPicker>().unload = Some(ship);
                app.update();
            } else {
                {
                    let mut map = app.world_mut().resource_mut::<crate::map::GameMap>();
                    let i = map.idx(10, 10);
                    map.tiles[i].base = crate::map::Base::Coast;
                }
                open(&mut app);
            }
            assert!(app.world().resource::<UnitPicker>().root.is_some());
            let rows: Vec<_> = app.world_mut().query::<(Entity, &PickerRow)>().iter(app.world()).map(|(e, row)| (e, row.0)).collect();
            assert_eq!(rows.len(), 3);
            let row = rows.iter().find(|(_, u)| *u == worker).unwrap().0;
            *app.world_mut().get_mut::<Interaction>(row).unwrap() = Interaction::Pressed;
            app.update();
            let u = app.world().get::<Unit>(worker).unwrap();
            assert_eq!((u.carrier, u.moves, u.sentry), (if in_port { None } else { Some(ship) }, if in_port { 0 } else { 3 }, false));
            assert_eq!(app.world().get::<Unit>(other).unwrap().carrier, Some(ship));
            assert!(app.world().resource::<UnitPicker>().root.is_none());
            assert!(app.world().resource::<UnitPicker>().unload.is_none());
        }
    }

    #[test]
    fn single_unit_picker_lists_exhausted_unit_but_does_not_wake_it() {
        let (mut app, warrior, worker) = app();
        app.world_mut().despawn(worker);
        {
            let mut u = app.world_mut().get_mut::<Unit>(warrior).unwrap();
            u.moves = 0;
            u.fortified = true;
        }
        open(&mut app);
        let rows: Vec<_> = app
            .world_mut()
            .query::<(Entity, &PickerRow)>()
            .iter(app.world())
            .map(|(e, row)| (e, row.0))
            .collect();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, warrior);
        *app.world_mut().get_mut::<Interaction>(rows[0].0).unwrap() = Interaction::Pressed;
        app.update();
        assert_eq!(app.world().resource::<Selected>().0, None);
        assert!(app.world().get::<Unit>(warrior).unwrap().fortified);
    }
}
