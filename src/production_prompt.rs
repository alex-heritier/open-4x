//! Production completion decisions, presented one city at a time. A prompt
//! belongs to the civ that owns the city: the outgoing civ's decisions wait,
//! unseen, until that civ is the active hotseat player again.
use bevy::prelude::*;
use std::collections::VecDeque;

use crate::cities::{City, CityView, Production, city_yields};
use crate::civs::Civilizations;
use crate::map::GameMap;

#[derive(Resource, Default)]
pub struct ProductionPrompts {
    /// City, owning civ and the item that finished, oldest first.
    pending: VecDeque<(Entity, usize, Production)>,
    /// Civ whose panel is on screen, if any.
    open: Option<usize>,
    expanded: bool,
}

impl ProductionPrompts {
    pub fn push(&mut self, city: Entity, civ: usize, completed: Production) {
        self.pending.push_back((city, civ, completed));
    }

    /// A city changed hands: its previous owner no longer decides for it.
    pub fn forget_city(&mut self, city: Entity) {
        self.pending.retain(|(c, _, _)| *c != city);
    }

    /// Index of `civ`'s oldest pending prompt.
    fn first_for(&self, civ: usize) -> Option<usize> {
        self.pending.iter().position(|(_, c, _)| *c == civ)
    }

    fn has_pending(&self, civ: usize) -> bool {
        self.first_for(civ).is_some()
    }

    /// Whether the panel is up or `civ` still owes a build decision. Other
    /// civs' pending prompts never hold the active player up.
    pub fn blocks(&self, civ: usize) -> bool {
        self.open.is_some() || self.has_pending(civ)
    }

    fn remove(&mut self, i: usize) {
        self.pending.remove(i);
    }
}

/// Neither a build decision nor an advisor's modal is waiting.
pub fn inactive(
    prompts: Res<ProductionPrompts>,
    civs: Res<Civilizations>,
    advisors: Res<crate::advisors::Advisors>,
    domestic: Res<crate::domestic::Domestic>,
    switch: Res<crate::build_switch::BuildSwitch>,
    abandon: Res<crate::abandon::Abandon>,
) -> bool {
    !abandon.blocks(civs.active) && !switch.is_pending() && !prompts.blocks(civs.active) && !advisors.is_open() && !domestic.is_open()
}

pub fn city_input_allowed(
    prompts: Res<ProductionPrompts>,
    view: Res<CityView>,
    civs: Res<Civilizations>,
    switch: Res<crate::build_switch::BuildSwitch>,
    abandon: Res<crate::abandon::Abandon>,
) -> bool {
    !abandon.blocks(civs.active) && !switch.is_pending() && (!prompts.blocks(civs.active) || view.0.is_some())
}

#[derive(Component)]
pub(crate) struct PromptRoot;

#[derive(Component)]
pub enum PromptButton {
    Expand,
    Pick(Production),
    Zoom,
    Ok,
}

fn button(
    parent: &mut ChildSpawnerCommands,
    font: &Handle<Font>,
    label: String,
    action: PromptButton,
) {
    parent
        .spawn((
            Button,
            action,
            Node {
                min_height: Val::Px(40.0),
                padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
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

pub fn show(
    mut commands: Commands,
    mut prompts: ResMut<ProductionPrompts>,
    cities: Query<&City>,
    view: Res<CityView>,
    map: Res<GameMap>,
    assets: Res<AssetServer>,
    civs: Res<Civilizations>,
    advisors: Res<crate::advisors::Advisors>,
    domestic: Res<crate::domestic::Domestic>,
    roots: Query<Entity, With<PromptRoot>>,
    switch: Res<crate::build_switch::BuildSwitch>,
    abandon: Res<crate::abandon::Abandon>,
) {
    // One modal at a time: a build decision waits for the advisor's.
    if abandon.blocks(civs.active) || switch.is_pending() || advisors.is_open() || domestic.is_open() {
        return;
    }
    // A panel left over from the civ that just ended its turn comes down
    // before the next player sees it; its decision stays pending.
    if prompts.open.is_some_and(|civ| civ != civs.active) {
        for root in &roots {
            commands.entity(root).despawn();
        }
        prompts.open = None;
        prompts.expanded = false;
    }
    if prompts.open.is_some() || view.0.is_some() {
        return;
    }
    // Only the active civ's own completions are presented.
    let Some(i) = prompts.first_for(civs.active) else {
        return;
    };
    let Ok(city) = cities.get(prompts.pending[i].0) else {
        prompts.remove(i);
        return;
    };
    let completed = prompts.pending[i].2;
    prompts.open = Some(civs.active);
    let font = assets.load("cache/fonts/lsans.ttf");
    let rate = city_yields(&map, city).1;
    let label = |p: Production| {
        let mut preview = city.clone();
        preview.change_build(p);
        let turns = if rate == 0 {
            "never".into()
        } else {
            let n = u32::from(city.price(p).saturating_sub(preview.shields)).div_ceil(u32::from(rate));
            format!("{n} {}", if n == 1 { "turn" } else { "turns" })
        };
        format!("{} ({turns})", p.name())
    };
    commands
        .spawn((
            PromptRoot,
            GlobalZIndex(100),
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: Val::Px(460.0),
                    max_width: Val::Percent(95.0),
                    padding: UiRect::all(Val::Px(24.0)),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(8.0),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                ImageNode::new(assets.load("cache/cityscreen/ProductionQueueBox.png")),
                BackgroundColor(Color::srgb(0.94, 0.91, 0.77)),
                BorderColor::all(Color::srgb(0.25, 0.4, 0.28)),
            ))
            .with_children(|panel| {
                for (text, size) in [
                    ("Domestic Advisor".into(), 30.0),
                    (
                        format!(
                            "Excellency, {} has produced {}.\nShall we begin work on",
                            city.name,
                            completed.name()
                        ),
                        22.0,
                    ),
                ] {
                    panel.spawn((
                        Text::new(text),
                        TextFont {
                            font: font.clone(),
                            font_size: size,
                            ..default()
                        },
                        TextColor(Color::BLACK),
                    ));
                }
                button(
                    panel,
                    &font,
                    format!("{}  ▾", label(city.production)),
                    PromptButton::Expand,
                );
                if prompts.expanded {
                    for p in city.buildable() {
                        button(panel, &font, label(p), PromptButton::Pick(p));
                    }
                }
                button(
                    panel,
                    &font,
                    format!("Zoom to {}", city.name),
                    PromptButton::Zoom,
                );
                button(panel, &font, "OK".into(), PromptButton::Ok);
            });
        });
}

pub fn respond(
    mut commands: Commands,
    buttons: Query<(&Interaction, &PromptButton), Changed<Interaction>>,
    roots: Query<Entity, With<PromptRoot>>,
    mut prompts: ResMut<ProductionPrompts>,
    mut cities: Query<&mut City>,
    mut view: ResMut<CityView>,
    mut switch: ResMut<crate::build_switch::BuildSwitch>,
    abandon: Res<crate::abandon::Abandon>,
    civs: Res<Civilizations>,
) {
    if switch.is_pending() || abandon.blocks(civs.active) { return; }
    for (interaction, action) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let Some(civ) = prompts.open else { return };
        let Some(i) = prompts.first_for(civ) else {
            return;
        };
        let entity = prompts.pending[i].0;
        match action {
            PromptButton::Expand => prompts.expanded = !prompts.expanded,
            PromptButton::Pick(p) => {
                if let Ok(mut city) = cities.get_mut(entity) {
                    if city.buildable().contains(p) {
                        switch.request(entity, &mut city, *p);
                    }
                }
                prompts.expanded = false;
            }
            PromptButton::Zoom => view.0 = Some(entity),
            PromptButton::Ok => {
                prompts.remove(i);
                prompts.expanded = false;
            }
        }
        for root in &roots {
            commands.entity(root).despawn();
        }
        prompts.open = None;
        break;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advisor_pick_requires_confirmation_before_losing_shields() {
        for accept in [false, true] {
            crate::realm::reset();
            crate::realm::write(0, |r| r.known = u128::MAX);
            let mut app = App::new();
            app.init_resource::<ProductionPrompts>();
            app.init_resource::<crate::abandon::Abandon>();
            app.init_resource::<crate::build_switch::BuildSwitch>();
            app.init_resource::<CityView>();
            app.init_resource::<Civilizations>();
            app.init_resource::<ButtonInput<KeyCode>>();
            app.add_systems(Update, (respond, crate::build_switch::respond).chain());
            let mut city = City::new(0, "Town", 5, 5);
            city.production = Production::named("Barracks");
            city.shields = 30;
            let entity = app.world_mut().spawn(city).id();
            let mut prompts = app.world_mut().resource_mut::<ProductionPrompts>();
            prompts.push(entity, 0, Production::named("Warrior"));
            prompts.open = Some(0);
            app.world_mut().spawn((Interaction::Pressed, PromptButton::Pick(Production::named("Warrior"))));
            app.update();
            assert!(app.world().resource::<crate::build_switch::BuildSwitch>().is_pending());
            let city = app.world().get::<City>(entity).unwrap();
            assert_eq!((city.production, city.shields), (Production::named("Barracks"), 30));
            app.world_mut().spawn((Interaction::Pressed, if accept {
                crate::cities::ScreenButton::SwitchYes
            } else { crate::cities::ScreenButton::SwitchNo }));
            app.update();
            let city = app.world().get::<City>(entity).unwrap();
            assert_eq!((city.production, city.shields), if accept {
                (Production::named("Warrior"), 10)
            } else { (Production::named("Barracks"), 30) });
            assert!(app.world().resource::<ProductionPrompts>().blocks(0), "the advisor decision remains pending");
        }
    }

    fn prompt(city: u64, civ: usize, p: Production) -> (Entity, usize, Production) {
        (Entity::from_bits(city), civ, p)
    }

    #[test]
    fn completions_are_presented_in_order() {
        let mut prompts = ProductionPrompts::default();
        let first = Entity::from_bits(1);
        let second = Entity::from_bits(2);
        prompts.push(first, 0, Production::named("Warrior"));
        prompts.push(second, 0, Production::named("Granary"));
        assert_eq!(
            prompts.pending.pop_front(),
            Some(prompt(1, 0, Production::named("Warrior")))
        );
        assert_eq!(
            prompts.pending.pop_front(),
            Some(prompt(2, 0, Production::named("Granary")))
        );
        assert!(prompts.pending.is_empty());
    }

    /// A civ's prompt waits for that civ's next turn: while another civ is
    /// active the decision neither shows nor holds the player up.
    #[test]
    fn another_civs_prompt_waits_for_its_own_turn() {
        let mut prompts = ProductionPrompts::default();
        prompts.push(Entity::from_bits(1), 0, Production::named("Warrior"));
        prompts.push(Entity::from_bits(2), 1, Production::named("Granary"));
        assert!(prompts.blocks(0), "civ 0 owes a decision");
        assert!(prompts.blocks(1));
        assert!(!prompts.blocks(2), "a third civ is never held up");
        assert_eq!(prompts.first_for(0), Some(0));
        assert_eq!(prompts.first_for(1), Some(1));
        assert_eq!(prompts.first_for(2), None);
        // Civ 0 answers: only its own entry leaves the queue, so civ 1's
        // prompt is still waiting when its turn comes round again.
        prompts.remove(0);
        assert!(!prompts.blocks(0));
        assert!(prompts.blocks(1));
        assert_eq!(
            prompts.pending.front(),
            Some(&prompt(2, 1, Production::named("Granary")))
        );
    }
}
