//! ABANDONBASE (`0x4B8C1D`): population-cost production can empty a city.
use crate::cities::{City, Production, ScreenButton};
use crate::civs::Civilizations;
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Clone, Copy)]
struct Choice {
    city: Entity,
    civ: usize,
    done: Production,
    size: u8,
}

#[derive(Resource, Default)]
pub struct Abandon(VecDeque<Choice>);

impl Abandon {
    pub fn push(&mut self, city: Entity, c: &City) {
        if !self.0.iter().any(|p| p.city == city) {
            self.0.push_back(Choice {
                city,
                civ: c.civ,
                done: c.production,
                size: c.size(),
            });
        }
    }
    pub fn blocks(&self, civ: usize) -> bool {
        self.0.iter().any(|p| p.civ == civ)
    }
}

fn valid(choice: &Choice, city: &City) -> bool {
    city.civ == choice.civ
        && city.production == choice.done
        && city.size() == choice.size
        && city.shields >= city.price(choice.done)
        && choice
            .done
            .unit()
            .is_some_and(|u| i32::from(city.size()) <= crate::units::def(u).pop_cost)
}

#[derive(Component)]
pub(crate) struct AbandonRoot(pub(crate) Entity);

pub fn respond(
    mut commands: Commands,
    buttons: Query<(&Interaction, &ScreenButton), Changed<Interaction>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut abandon: ResMut<Abandon>,
    civs: Res<Civilizations>,
) {
    let answer = buttons
        .iter()
        .find_map(|(i, b)| {
            if *i != Interaction::Pressed {
                return None;
            }
            match b {
                ScreenButton::AbandonYes => Some(2),
                ScreenButton::AbandonNo => Some(0),
                ScreenButton::AbandonZoom => Some(1),
                _ => None,
            }
        })
        .or_else(|| {
            (keys.just_pressed(KeyCode::Escape) || keys.just_pressed(KeyCode::Enter)).then_some(0)
        });
    let Some(answer) = answer else {
        return;
    };
    let Some(i) = abandon.0.iter().position(|p| p.civ == civs.active) else {
        return;
    };
    let choice = abandon.0.remove(i).unwrap();
    if answer == 1 {
        commands.queue(move |w: &mut World| {
            if w.get::<City>(choice.city)
                .is_some_and(|c| valid(&choice, c))
            {
                w.resource_mut::<crate::cities::CityView>().0 = Some(choice.city);
            }
        });
    } else if answer == 2 {
        commands.queue(move |w: &mut World| complete(w, choice));
    }
}

fn complete(world: &mut World, choice: Choice) {
    let Some(city) = world
        .get::<City>(choice.city)
        .filter(|c| valid(&choice, c))
        .cloned()
    else {
        return;
    };
    if world.resource::<Civilizations>().active != choice.civ {
        return;
    }
    let map = world.resource::<crate::map::GameMap>();
    if crate::citycalc::totals(map, &city).surplus > 0 {
        return;
    }
    let unit = choice.done.unit().unwrap();
    let level = if crate::citycalc::trains_veterans(&city, unit) {
        crate::combat::Level::Veteran
    } else {
        crate::combat::Level::Regular
    };
    // The native factory runs before population payment and removal.
    let produced = world.resource_scope(|w, art: Mut<crate::units::UnitArt>| {
        crate::units::spawn_unit_at_level(
            &mut w.commands(),
            &art,
            unit,
            city.x,
            city.y,
            city.civ,
            level,
        )
    });
    world.flush();
    let nationality = world.resource_scope(|w, mut rng: Mut<crate::combat::CombatRng>| {
        let mut c = w.get_mut::<City>(choice.city).unwrap();
        c.shields = 0;
        c.pay_population_cost(crate::units::def(unit).pop_cost.max(0) as u8, &mut rng.0)
    });
    world
        .get_mut::<crate::units::Unit>(produced)
        .unwrap()
        .nationality = nationality;
    if world.get::<City>(choice.city).unwrap().size() == 0 {
        // The final foreign race taken records this as a razed city.
        if nationality != crate::civs::roster_index(city.civ)
            && let Some(victim) =
                (0..crate::civs::civ_count()).find(|&c| crate::civs::roster_index(c) == nationality)
        {
            let mut dip = world.resource_mut::<crate::diplomacy::Diplomacy>();
            let record = dip.rel.rec_mut(
                crate::research::slot(victim),
                crate::research::slot(city.civ),
            );
            record[civ3_rules::diplomacy::rec::RAZED] += 1;
        }
        remove_empty(world, choice.city, &city);
        crate::features::post(
            &mut world.resource_mut::<crate::features::MessageBoard>(),
            format!(
                "{} produces {} and is abandoned.",
                city.name,
                choice.done.name()
            ),
        );
    } else {
        // Native payment cannot take a race belonging to no player in play.
        world
            .get_mut::<City>(choice.city)
            .unwrap()
            .advance_queue(choice.done);
        world
            .resource_mut::<crate::production_prompt::ProductionPrompts>()
            .push(choice.city, city.civ, choice.done);
    }
}

fn remove_empty(world: &mut World, entity: Entity, city: &City) {
    let mut q = world.query::<(
        Entity,
        Option<&crate::cities::CitySprite>,
        Option<&crate::cities::CityLabelBack>,
    )>();
    let visuals: Vec<_> = q
        .iter(world)
        .filter(|(_, s, b)| s.is_some_and(|s| s.0 == entity) || b.is_some_and(|b| b.0 == entity))
        .map(|(e, _, _)| e)
        .collect();
    for e in visuals {
        world.despawn(e);
    }
    world.despawn(entity);
    world
        .resource_mut::<crate::production_prompt::ProductionPrompts>()
        .forget_city(entity);
    if world.resource::<crate::cities::CityView>().0 == Some(entity) {
        world.resource_mut::<crate::cities::CityView>().0 = None;
        world.resource_mut::<crate::cities::BuildMenu>().0 = false;
    }
    if world
        .resource::<crate::cities::HurryAsk>()
        .0
        .is_some_and(|(e, _)| e == entity)
    {
        world.resource_mut::<crate::cities::HurryAsk>().0 = None;
    }
    // Stock Road's enabling advance is -1. A city's implicit road survives
    // removal (`0x4AF1B2`); railroads are not represented by this game yet.
    let mut map = world.resource_mut::<crate::map::GameMap>();
    let tile = map.idx(city.x, city.y);
    map.tiles[tile].road = true;
    // Border, visibility, capital, trade and wonder watchers observe removal
    // on their next pass; worked-tile claims are derived from live citizens.
}

pub fn show(
    mut commands: Commands,
    mut abandon: ResMut<Abandon>,
    cities: Query<&City>,
    civs: Res<Civilizations>,
    assets: Res<AssetServer>,
    roots: Query<(Entity, &AbandonRoot)>,
    advisors: Res<crate::advisors::Advisors>,
    domestic: Res<crate::domestic::Domestic>,
) {
    abandon
        .0
        .retain(|p| cities.get(p.city).is_ok_and(|c| valid(p, c)));
    let choice = abandon.0.iter().find(|p| p.civ == civs.active);
    for (e, root) in &roots {
        if choice.is_none_or(|p| p.city != root.0) {
            commands.entity(e).despawn();
        }
    }
    if advisors.is_open() || domestic.is_open() {
        return;
    }
    let Some(choice) = choice else {
        return;
    };
    if roots.iter().any(|(_, r)| r.0 == choice.city) {
        return;
    }
    let city = cities.get(choice.city).unwrap();
    let font = assets.load("fonts/lsans.ttf");
    commands
        .spawn((
            AbandonRoot(choice.city),
            GlobalZIndex(110),
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.35)),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: Val::Px(460.0),
                    max_width: Val::Percent(95.0),
                    padding: UiRect::all(Val::Px(24.0)),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(16.0),
                    ..default()
                },
                ImageNode::new(assets.load("cityscreen/ProductionQueueBox.png")),
                BackgroundColor(Color::srgb(0.94, 0.91, 0.77)),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new(format!(
                        "Produce {} and abandon {}?\nThis will use the city's last {} {}.",
                        choice.done.name(),
                        city.name,
                        choice.size,
                        if choice.size == 1 {
                            "citizen"
                        } else {
                            "citizens"
                        }
                    )),
                    TextFont {
                        font: font.clone(),
                        font_size: 22.0,
                        ..default()
                    },
                    TextColor(Color::BLACK),
                ));
                for (label, action) in [
                    ("Keep the city", ScreenButton::AbandonNo),
                    ("Zoom to the city", ScreenButton::AbandonZoom),
                    ("Produce unit and abandon city", ScreenButton::AbandonYes),
                ] {
                    panel
                        .spawn((
                            Button,
                            action,
                            Node {
                                min_height: Val::Px(40.0),
                                padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
                                align_items: AlignItems::Center,
                                ..default()
                            },
                            BackgroundColor(Color::srgba(0.65, 0.60, 0.40, 0.18)),
                        ))
                        .with_children(|b| {
                            b.spawn((
                                Text::new(label),
                                TextFont {
                                    font: font.clone(),
                                    font_size: 20.0,
                                    ..default()
                                },
                                TextColor(Color::srgb(0.05, 0.3, 0.65)),
                            ));
                        });
                }
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cities::{BuildMenu, Capital, CityView, HurryAsk};
    use crate::units::{Unit, UnitType};

    fn pending(production: Production, size: u8) -> (App, Entity) {
        crate::realm::reset();
        let mut map = crate::map::GameMap::generate();
        for tile in &mut map.tiles {
            tile.base = crate::map::Base::Plains;
            tile.relief = crate::map::Relief::Flat;
            tile.cover = crate::map::Cover::Bare;
            tile.resource = None;
            tile.river = 0;
        }
        let mut app = App::new();
        app.insert_resource(map);
        app.insert_resource(crate::combat::CombatRng(crate::rng::MapRng::new(1)));
        app.insert_resource(crate::units::UnitArt::blank());
        app.insert_resource(crate::diplomacy::Diplomacy::new());
        app.init_resource::<Civilizations>();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_resource::<Abandon>();
        app.init_resource::<CityView>();
        app.init_resource::<BuildMenu>();
        app.init_resource::<HurryAsk>();
        app.init_resource::<Capital>();
        app.init_resource::<crate::features::MessageBoard>();
        app.init_resource::<crate::production_prompt::ProductionPrompts>();
        app.add_systems(Update, (respond, crate::capital::replace_missing).chain());
        let mut city = City::new(0, "Town", 5, 5);
        city.set_size(size);
        city.production = production;
        city.shields = city.price(production);
        city.food = 10;
        let entity = app.world_mut().spawn(city.clone()).id();
        app.world_mut()
            .resource_mut::<Abandon>()
            .push(entity, &city);
        app.world_mut().resource_mut::<Capital>().0[0] = Some(entity);
        (app, entity)
    }

    fn answer(app: &mut App, button: ScreenButton) {
        app.world_mut().spawn((Interaction::Pressed, button));
        app.update();
    }

    #[test]
    fn keeping_or_zooming_preserves_population_stock_and_random_state() {
        for button in [ScreenButton::AbandonNo, ScreenButton::AbandonZoom] {
            let (mut app, entity) = pending(Production::named("Worker"), 1);
            let before = app.world().get::<City>(entity).unwrap().citizens.clone();
            let zoom = matches!(button, ScreenButton::AbandonZoom);
            answer(&mut app, button);
            let city = app.world().get::<City>(entity).unwrap();
            assert_eq!(city.citizens, before);
            assert_eq!(
                (city.food, city.shields),
                (10, city.price(Production::named("Worker")))
            );
            assert_eq!(
                app.world().resource::<crate::combat::CombatRng>().0.state(),
                1
            );
            assert!(!app.world().resource::<Abandon>().blocks(0));
            assert_eq!(app.world().resource::<CityView>().0, zoom.then_some(entity));
            assert_eq!(
                app.world_mut().query::<&Unit>().iter(app.world()).count(),
                0
            );
        }
    }

    #[test]
    fn acceptance_produces_the_unit_removes_city_and_replaces_capital() {
        for (production, size, expected) in [
            (Production::named("Worker"), 1, UnitType::named("Worker")),
            (Production::named("Settler"), 2, UnitType::named("Settler")),
        ] {
            let (mut app, entity) = pending(production, size);
            let replacement = app.world_mut().spawn(City::new(0, "Home", 30, 30)).id();
            let visual = app
                .world_mut()
                .spawn(crate::cities::CitySprite(entity))
                .id();
            let unrelated = app
                .world_mut()
                .spawn(crate::cities::CitySprite(replacement))
                .id();
            app.world_mut().resource_mut::<CityView>().0 = Some(entity);
            app.world_mut().resource_mut::<BuildMenu>().0 = true;
            answer(&mut app, ScreenButton::AbandonYes);
            assert!(app.world().get::<City>(entity).is_none());
            assert!(app.world().get_entity(visual).is_err());
            assert!(app.world().get_entity(unrelated).is_ok());
            let units: Vec<_> = app.world_mut().query::<&Unit>().iter(app.world()).collect();
            assert_eq!(units.len(), 1);
            assert_eq!(
                (units[0].utype, units[0].nationality),
                (expected, crate::civs::roster_index(0))
            );
            assert_eq!(app.world().resource::<CityView>().0, None);
            assert!(!app.world().resource::<BuildMenu>().0);
            assert_eq!(app.world().resource::<Capital>().0[0], Some(replacement));
            let map = app.world().resource::<crate::map::GameMap>();
            assert!(map.tiles[map.idx(5, 5)].road);
            assert!(
                !app.world()
                    .resource::<crate::production_prompt::ProductionPrompts>()
                    .blocks(0)
            );
        }
    }

    #[test]
    fn final_foreign_citizen_sets_unit_nationality_and_razed_counter() {
        let (mut app, entity) = pending(Production::named("Worker"), 1);
        app.world_mut()
            .get_mut::<City>(entity)
            .unwrap()
            .set_nationality(1);
        answer(&mut app, ScreenButton::AbandonYes);
        assert!(app.world().get::<City>(entity).is_none());
        let unit = app
            .world_mut()
            .query::<&Unit>()
            .single(app.world())
            .unwrap();
        assert_eq!(unit.nationality, crate::civs::roster_index(1));
        assert_eq!(
            app.world().resource::<crate::combat::CombatRng>().0.state(),
            2524885223
        );
        let dip = app.world().resource::<crate::diplomacy::Diplomacy>();
        assert_eq!(
            dip.rel
                .rec(crate::research::slot(1), crate::research::slot(0))
                [civ3_rules::diplomacy::rec::RAZED],
            1
        );
    }

    #[test]
    fn enter_keeps_the_city_as_the_first_dialog_choice() {
        let (mut app, entity) = pending(Production::named("Worker"), 1);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Enter);
        app.update();
        assert_eq!(app.world().get::<City>(entity).unwrap().size(), 1);
        assert_eq!(app.world().get::<City>(entity).unwrap().shields, 10);
        assert_eq!(
            app.world().resource::<crate::combat::CombatRng>().0.state(),
            1
        );
        assert!(!app.world().resource::<Abandon>().blocks(0));
    }

    #[test]
    fn citizens_of_an_absent_race_survive_native_payment() {
        let (mut app, entity) = pending(Production::named("Worker"), 1);
        let absent = (0..31)
            .find(|r| !crate::civs::players().contains(r))
            .unwrap();
        app.world_mut()
            .get_mut::<City>(entity)
            .unwrap()
            .citizens
            .get_mut(0)
            .unwrap()
            .race = absent as i32;
        answer(&mut app, ScreenButton::AbandonYes);
        let city = app.world().get::<City>(entity).unwrap();
        assert_eq!(city.size(), 1);
        assert_eq!(city.shields, 0);
        assert_eq!(
            city.citizens.slots()[0].as_ref().unwrap().race,
            absent as i32
        );
        assert_eq!(
            app.world_mut().query::<&Unit>().iter(app.world()).count(),
            1
        );
        assert!(
            app.world()
                .resource::<crate::production_prompt::ProductionPrompts>()
                .blocks(0)
        );
    }

    #[test]
    fn stale_choice_cannot_consume_a_changed_city() {
        let (mut app, entity) = pending(Production::named("Worker"), 1);
        app.world_mut().get_mut::<City>(entity).unwrap().production = Production::named("Warrior");
        answer(&mut app, ScreenButton::AbandonYes);
        assert_eq!(app.world().get::<City>(entity).unwrap().size(), 1);
        assert_eq!(
            app.world().resource::<crate::combat::CombatRng>().0.state(),
            1
        );
        assert_eq!(
            app.world_mut().query::<&Unit>().iter(app.world()).count(),
            0
        );
    }
}
