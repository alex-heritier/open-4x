//! Explicit destination targeting using the simulation's path and movement costs.
use super::*;

const COLOUR: Color = Color::srgb(0.3, 0.95, 1.0);
pub(super) struct Targeting {
    pub unit: Id,
    pub stack: bool,
    pub drag: bool,
    pub preview: Option<Preview>,
    pub hovered: Option<Coord>,
    cached: Option<(Coord, u64)>,
}
pub(super) struct Preview {
    destination: Coord,
    path: Vec<Coord>,
    turns: u32,
}
impl Targeting {
    pub fn new(unit: Id, stack: bool, drag: bool) -> Self {
        Self {
            unit,
            stack,
            drag,
            preview: None,
            hovered: None,
            cached: None,
        }
    }
}

/// Movement phases needed, including this turn. An expensive step uses whatever is left.
fn travel_turns(game: &Game, rules: &Rules, unit: &fourx_sim::Unit, path: &[Coord]) -> u32 {
    let def = rules.def(unit);
    let mut left = unit.moves_left(def);
    let mut turns = u32::from(path.len() > 1);
    for step in path.windows(2) {
        if left == 0 {
            if def.total_moves() == 0 {
                return u32::MAX;
            }
            turns += 1;
            left = def.total_moves();
        }
        let landing = def.domain == fourx_sim::Domain::Land
            && game.map.get(step[0]).is_some_and(|t| !t.is_land());
        let cost = if landing {
            left
        } else {
            game.step_cost(step[0], step[1], def.domain, unit.owner)
        };
        left = left.saturating_sub(cost);
    }
    turns
}
fn preview(
    game: &Game,
    rules: &Rules,
    unit: Id,
    stack: bool,
    destination: Coord,
) -> Option<Preview> {
    let destination = game.map.normalize(destination)?;
    let unit = game.units.get(&unit)?;
    if stack
        && matches!(
            stack_command(game, rules, unit.owner, unit.id, destination),
            Some(Command::AttackStack { .. })
        )
    {
        return Some(Preview {
            destination,
            path: vec![unit.position, destination],
            turns: 1,
        });
    }
    if game.blocked_for(unit.owner, destination) {
        return None;
    }
    let path = game.path(
        unit.owner,
        rules.def(unit).domain,
        unit.position,
        destination,
        usize::MAX,
    )?;
    if path.len() < 2 {
        return None;
    }
    let turns = travel_turns(game, rules, unit, &path);
    (turns != u32::MAX).then_some(Preview {
        destination,
        path,
        turns,
    })
}
pub(super) fn update(session: &mut Session, destination: Coord) {
    let Some(game) = session.game.as_ref() else {
        return;
    };
    let Some(targeting) = session.goto.as_mut() else {
        return;
    };
    targeting.hovered = Some(destination);
    let key = (destination, game.revision);
    if targeting.cached != Some(key) {
        targeting.preview = preview(
            game,
            &session.rules,
            targeting.unit,
            targeting.stack,
            destination,
        );
        targeting.cached = Some(key);
    }
}
#[derive(Component)]
pub(super) struct PreviewVisual;
pub(super) fn draw(
    mut commands: Commands,
    session: Res<Session>,
    shapes: Res<theme::Shapes>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Transform, &Projection), With<Camera2d>>,
    previous: Query<Entity, With<PreviewVisual>>,
    mut gizmos: Gizmos<route::RouteGizmos>,
) {
    for entity in &previous {
        commands.entity(entity).despawn();
    }
    let Some(targeting) = &session.goto else {
        return;
    };
    let Some(destination) = targeting.hovered else {
        return;
    };
    let Some(game) = &session.game else {
        return;
    };
    let Some(unit) = game.units.get(&targeting.unit) else {
        return;
    };
    let (Ok(window), Ok((transform, projection))) = (windows.single(), camera.single()) else {
        return;
    };
    let view = camera_rect(transform, projection, window).grow(96.0);
    if let Some(preview) = &targeting.preview {
        route::draw(
            &mut gizmos,
            &game.map,
            view,
            unit.position,
            preview.destination,
            &preview.path,
            COLOUR,
            false,
        );
    }
    for tile in view.copies(&game.map, destination) {
        let (x, y) = tile.screen();
        let mut sprite = shapes.selected_square();
        sprite.color = if targeting.preview.is_some() {
            COLOUR
        } else {
            Color::srgb(0.95, 0.3, 0.25)
        };
        commands.spawn((sprite, Transform::from_xyz(x, y, 51.0), PreviewVisual));
        if let Some(preview) = &targeting.preview {
            commands.spawn((
                Text2d::new(preview.turns.to_string()),
                TextFont {
                    font_size: 28.0,
                    ..default()
                },
                TextColor(COLOUR),
                Transform::from_xyz(x, y, 95.0),
                PreviewVisual,
            ));
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_foreign_cities_and_counts_movement_phases() {
        let host = Host::base_scenario("russo-japanese-war", 42).unwrap();
        let game = host.game;
        let unit = game
            .units
            .values()
            .find(|u| host.rules.def(u).domain == fourx_sim::Domain::Land)
            .unwrap();
        let foreign = game
            .cities
            .values()
            .find(|c| c.owner != unit.owner)
            .unwrap()
            .position;
        assert!(preview(&game, &host.rules, unit.id, false, foreign).is_none());
        assert!(preview(&game, &host.rules, unit.id, false, unit.position).is_none());
        let route = (0..game.map.tiles.len())
            .filter_map(|i| {
                let to = Coord::new(i as i32 % game.map.width, i as i32 / game.map.width);
                preview(&game, &host.rules, unit.id, false, to)
            })
            .find(|p| p.path.len() >= 4)
            .expect("a walkable destination");
        assert_eq!(route.path.first(), Some(&unit.position));
        assert_eq!(route.path.last(), Some(&route.destination));
        let mut walking = game.clone();
        let mut phases = 0;
        while walking.units[&unit.id].position != route.destination {
            assert!(phases < 100, "the preview route must arrive");
            walking
                .apply(
                    unit.owner,
                    Command::Move {
                        unit: unit.id,
                        destination: route.destination,
                    },
                    &host.rules,
                    fourx_sim::TickRules::default(),
                )
                .unwrap();
            phases += 1;
            walking.units.get_mut(&unit.id).unwrap().moves_used = 0;
        }
        assert_eq!(route.turns, phases);
        let mut exhausted = unit.clone();
        exhausted.moves_used = host.rules.def(unit).total_moves();
        assert!(travel_turns(&game, &host.rules, &exhausted, &route.path) >= route.turns);
    }

    #[test]
    fn stack_targeting_attacks_an_adjacent_enemy_city() {
        let mut host = Host::base_scenario("russo-japanese-war", 42).unwrap();
        let player = host.commander();
        let attacker = host
            .game
            .units
            .values()
            .find(|u| u.owner == player && host.rules.def(u).can_attack())
            .unwrap()
            .id;
        let (enemy, city) = host
            .game
            .cities
            .values()
            .find(|c| c.owner != player)
            .map(|c| (c.owner, c.position))
            .unwrap();
        let approach = city.offset(-1, 0);
        let ground = host.game.map.get_mut(approach).unwrap();
        ground.terrain = fourx_sim::terrain::Terrain::Grass;
        ground.relief = fourx_sim::terrain::Relief::Flat;
        ground.cover = fourx_sim::terrain::Cover::Bare;
        host.game.set_position(attacker, approach);
        if !host.game.at_war(player, enemy) {
            host.command(player, Command::DeclareWar { nation: enemy })
                .unwrap();
        }

        assert!(preview(&host.game, &host.rules, attacker, false, city).is_none());
        let attack = preview(&host.game, &host.rules, attacker, true, city)
            .expect("the stack can strike the adjacent city");
        assert_eq!((attack.destination, attack.turns), (city, 1));
        assert!(matches!(
            stack_command(&host.game, &host.rules, player, attacker, city),
            Some(Command::AttackStack { unit, target })
                if unit == attacker && target == city
        ));
    }
}
