//! CONFIRMSWITCH (`0x4AF5D0`): accept shield loss before changing production.
use bevy::prelude::*;
use crate::cities::{City, Production, ScreenButton};
use crate::civs::Civilizations;

struct Choice {
    city: Entity,
    from: Production,
    to: Production,
    shields: u16,
}

#[derive(Resource, Default)]
pub struct BuildSwitch(Option<Choice>);

impl BuildSwitch {
    pub fn is_pending(&self) -> bool { self.0.is_some() }

    pub fn request(&mut self, entity: Entity, city: &mut City, to: Production) {
        if self.is_pending() || to == city.production || !city.buildable().contains(&to) { return; }
        if city.shields > city.price(to) {
            self.0 = Some(Choice { city: entity, from: city.production, to, shields: city.shields });
        } else {
            city.change_build(to);
        }
    }
}

#[derive(Component)]
pub(crate) struct SwitchRoot;

pub fn respond(
    mut switch: ResMut<BuildSwitch>,
    mut cities: Query<&mut City>,
    civs: Res<Civilizations>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Query<(&Interaction, &ScreenButton), Changed<Interaction>>,
) {
    let answer = buttons.iter().find_map(|(i, b)| {
        if *i != Interaction::Pressed { return None; }
        match b { ScreenButton::SwitchYes => Some(true), ScreenButton::SwitchNo => Some(false), _ => None }
    }).or_else(|| {
        if keys.just_pressed(KeyCode::Escape) { Some(false) }
        else if keys.just_pressed(KeyCode::Enter) { Some(true) }
        else { None }
    });
    let Some(accept) = answer else { return; };
    let Some(choice) = switch.0.take() else { return; };
    if accept && let Ok(mut city) = cities.get_mut(choice.city)
        && city.civ == civs.active && city.production == choice.from
        && city.shields == choice.shields && city.buildable().contains(&choice.to)
    {
        city.change_build(choice.to);
    }
}

pub fn show(
    mut commands: Commands,
    mut switch: ResMut<BuildSwitch>,
    cities: Query<&City>,
    civs: Res<Civilizations>,
    assets: Res<AssetServer>,
    roots: Query<Entity, With<SwitchRoot>>,
) {
    if switch.0.as_ref().is_some_and(|c| !cities.get(c.city).is_ok_and(|city|
        city.civ == civs.active && city.production == c.from && city.shields == c.shields)) {
        switch.0 = None;
    }
    let Some(choice) = &switch.0 else {
        for root in &roots { commands.entity(root).despawn(); }
        return;
    };
    if !roots.is_empty() { return; }
    let city = cities.get(choice.city).unwrap();
    let lost = choice.shields - city.price(choice.to);
    let font = assets.load("cache/fonts/lsans.ttf");
    commands.spawn((SwitchRoot, GlobalZIndex(110), Node {
        width: Val::Percent(100.0), height: Val::Percent(100.0),
        justify_content: JustifyContent::Center, align_items: AlignItems::Center,
        ..default()
    }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.35))))
        .with_children(|root| {
            root.spawn((Node {
                width: Val::Px(460.0), max_width: Val::Percent(95.0),
                padding: UiRect::all(Val::Px(24.0)), flex_direction: FlexDirection::Column,
                row_gap: Val::Px(16.0), ..default()
            }, ImageNode::new(assets.load("cache/cityscreen/ProductionQueueBox.png")),
                BackgroundColor(Color::srgb(0.94, 0.91, 0.77))))
                .with_children(|panel| {
                    panel.spawn((Text::new(format!("Switch {} to {}?\nThis will discard {} {}.",
                        city.name, choice.to.name(), lost, if lost == 1 { "shield" } else { "shields" })),
                        TextFont { font: font.clone(), font_size: 22.0, ..default() }, TextColor(Color::BLACK)));
                    for (label, action) in [("Switch production", ScreenButton::SwitchYes), ("Keep current production", ScreenButton::SwitchNo)] {
                        panel.spawn((Button, action, Node {
                            min_height: Val::Px(40.0), padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
                            align_items: AlignItems::Center, ..default()
                        }, BackgroundColor(Color::srgba(0.65, 0.60, 0.40, 0.18))))
                            .with_children(|b| { b.spawn((Text::new(label), TextFont {
                                font: font.clone(), font_size: 20.0, ..default()
                            }, TextColor(Color::srgb(0.05, 0.3, 0.65)))); });
                    }
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (App, Entity) {
        crate::realm::reset();
        crate::realm::write(0, |r| r.known = u128::MAX);
        let mut app = App::new();
        app.init_resource::<BuildSwitch>();
        app.init_resource::<Civilizations>();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.add_systems(Update, respond);
        let mut city = City::new(0, "Town", 5, 5);
        city.production = Production::named("Barracks");
        city.shields = 30;
        let city = app.world_mut().spawn(city).id();
        (app, city)
    }

    fn request(app: &mut App, entity: Entity, to: Production) {
        app.world_mut().resource_scope(|world, mut switch: Mut<BuildSwitch>| {
            switch.request(entity, &mut world.get_mut::<City>(entity).unwrap(), to);
        });
    }

    #[test]
    fn cancel_preserves_production_and_accept_discards_only_the_quoted_surplus() {
        let (mut app, entity) = fixture();
        request(&mut app, entity, Production::named("Warrior"));
        assert!(app.world().resource::<BuildSwitch>().is_pending());
        assert_eq!(app.world().get::<City>(entity).unwrap().shields, 30);
        let cancel = app.world_mut().spawn((Interaction::Pressed, ScreenButton::SwitchNo)).id();
        app.update();
        let city = app.world().get::<City>(entity).unwrap();
        assert_eq!((city.production, city.shields), (Production::named("Barracks"), 30));
        assert!(!app.world().resource::<BuildSwitch>().is_pending());
        app.world_mut().despawn(cancel);
        request(&mut app, entity, Production::named("Warrior"));
        app.world_mut().spawn((Interaction::Pressed, ScreenButton::SwitchYes));
        app.update();
        let city = app.world().get::<City>(entity).unwrap();
        assert_eq!((city.production, city.shields), (Production::named("Warrior"), 10));
    }

    #[test]
    fn affordable_switch_is_immediate_and_stale_or_foreign_choices_cannot_commit() {
        let (mut app, entity) = fixture();
        request(&mut app, entity, Production::named("Granary"));
        assert!(!app.world().resource::<BuildSwitch>().is_pending());
        assert_eq!(app.world().get::<City>(entity).unwrap().shields, 30);
        for foreign in [false, true] {
            request(&mut app, entity, Production::named("Warrior"));
            if foreign { app.world_mut().get_mut::<City>(entity).unwrap().civ = 1; }
            else { app.world_mut().get_mut::<City>(entity).unwrap().shields = 35; }
            app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Enter);
            app.update();
            assert_eq!(app.world().get::<City>(entity).unwrap().production, Production::named("Granary"));
            assert!(!app.world().resource::<BuildSwitch>().is_pending());
            app.world_mut().resource_mut::<ButtonInput<KeyCode>>().reset_all();
        }
    }

    #[test]
    fn escape_keeps_the_current_item_and_shields() {
        let (mut app, entity) = fixture();
        request(&mut app, entity, Production::named("Warrior"));
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Escape);
        app.update();
        assert!(!app.world().resource::<BuildSwitch>().is_pending());
        let city = app.world().get::<City>(entity).unwrap();
        assert_eq!((city.production, city.shields), (Production::named("Barracks"), 30));
    }
}
