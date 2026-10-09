//! The route a marching unit will walk, drawn square by square.
//!
//! A unit with a standing `goto` walks the cheapest route [`Game::path`] finds, one stretch
//! each day. The host sends that route with the unit (`Unit::route`, see
//! `Game::show_routes`), so the line follows every bend the unit will take instead of
//! running straight to the goal: it passes through the centres of the squares in order and,
//! because the grid has eight neighbours and the map is drawn diagonally, it bends wherever
//! the route turns. The host found it on the real map, so it is right even across ground
//! this nation has not charted yet.
use super::*;

/// Own gizmo group, so the route is drawn thicker than the selection marks.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub(super) struct RouteGizmos;

/// How the route is drawn: a heavy gold line with rounded bends.
pub(super) fn gizmo_config() -> GizmoConfig {
    GizmoConfig {
        line: GizmoLineConfig {
            width: 4.0,
            joints: GizmoLineJoint::Round(4),
            ..default()
        },
        ..default()
    }
}

/// Draw a unit's route: the line through the squares, a dot on each, and a mark on the goal.
/// With no route to show the unit is still heading somewhere, so the goal is marked with a
/// straight, fainter line to it.
pub(super) fn draw(
    gizmos: &mut Gizmos<RouteGizmos>,
    from: Coord,
    goal: Coord,
    route: &[Coord],
    colour: Color,
) {
    let centre = |p: Coord| {
        let (x, y) = p.screen();
        Vec2::new(x, y)
    };
    if route.len() < 2 {
        gizmos.line_2d(centre(from), centre(goal), colour.with_alpha(0.45));
    } else {
        gizmos.linestrip_2d(route.iter().map(|&p| centre(p)), colour);
        for &p in &route[1..route.len() - 1] {
            gizmos.circle_2d(centre(p), 4.5, colour);
        }
    }
    gizmos.circle_2d(centre(goal), 14.0, colour);
    gizmos.circle_2d(centre(goal), 7.0, colour);
}
