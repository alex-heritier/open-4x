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
    /// Consume the closing click through its release, which otherwise moves.
    consume_click: bool,
    owner: usize,
}

#[derive(Component)]
pub(crate) struct PickerRow(Entity);

#[derive(Component)]
pub(crate) struct PickerBackdrop;

#[derive(Component)]
pub(crate) struct PickerList;

pub fn inactive(picker: Res<UnitPicker>) -> bool {
    picker.root.is_none() && !picker.consume_click
}

pub fn update(
    mut commands: Commands,
    mut picker: ResMut<UnitPicker>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    pointer: (Res<Hovered>, ResMut<MovePreview>),
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
    let (hovered, mut preview) = pointer;
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
                    if let Ok((_, mut u)) = units.get_mut(row.0) {
                        if u.civ == civs.active && units::selectable(&u) {
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
            picker.consume_click = true;
        }
        return;
    }
    if blocked || !buttons.just_pressed(MouseButton::Right) {
        return;
    }
    let Some((x, y)) = hovered.0 else {
        return;
    };
    let mut stack: Vec<_> = units
        .iter()
        .filter(|(_, u)| u.civ == civs.active && (u.x, u.y) == (x, y))
        .collect();
    if stack.is_empty() {
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
    let height = (36.0 + stack.len() as f32 * 28.0).min(window.height() * 0.75);
    let pos = window
        .cursor_position()
        .unwrap_or(Vec2::new(window.width() / 2.0, window.height() / 2.0));
    let font = assets.load("gen/fonts/lsans.ttf");
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
                    Text::new("Select Unit"),
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
                        for (e, u) in &stack {
                            let enabled = units::selectable(u);
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
                                        units::def(u.utype).name,
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
        let warrior = spawn(UnitType::Warrior);
        let worker = spawn(UnitType::Worker);
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
