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

/// How the route is drawn: a heavy white line with rounded bends.
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
    map: &Map,
    view: WorldRect,
    from: Coord,
    goal: Coord,
    route: &[Coord],
    colour: Color,
    destination_marker: bool,
) {
    let squares = if route.len() < 2 {
        vec![from, goal]
    } else {
        route.to_vec()
    };
    let path = continuous_path(map, &squares);
    // Every image of the path that can intersect the view, including paths crossing its edge.
    let reach = path
        .iter()
        .map(|p| (p.screen().0 - path[0].screen().0).abs())
        .fold(0.0, f32::max);
    for first in view.grow(reach).copies(map, path[0]) {
        let shift = first.screen().0 - path[0].screen().0;
        let centre = |p: Coord| {
            let (x, y) = p.screen();
            Vec2::new(x + shift, y)
        };
        if route.len() < 2 {
            gizmos.line_2d(centre(path[0]), centre(path[1]), colour.with_alpha(0.45));
        } else {
            let mut points: Vec<_> = path.iter().map(|&p| centre(p)).collect();
            // Targeting puts the turn count at the destination; leave that centre clear.
            if !destination_marker {
                let last = points.len() - 1;
                let back = points[last - 1] - points[last];
                points[last] += back.normalize_or_zero() * 20.0_f32.min(back.length());
            }
            gizmos.linestrip_2d(points, colour);
            for &p in &path[1..path.len() - 1] {
                gizmos.circle_2d(centre(p), 4.5, colour);
            }
        }
        if destination_marker {
            let goal = map.nearest(*path.last().unwrap(), goal);
            gizmos.circle_2d(centre(goal), 14.0, colour);
            gizmos.circle_2d(centre(goal), 7.0, colour);
        }
    }
}
