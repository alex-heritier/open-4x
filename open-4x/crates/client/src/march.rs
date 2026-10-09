//! Shows units walking.
//!
//! A snapshot says where units ended up; its marches say which squares they crossed to get
//! there. While a march plays, the real unit is hidden and a walker steps along the path with
//! its run clip, turned toward each step, then hands back to the real unit facing the way it
//! last walked. The walks of one snapshot play together, and fights wait for them. They never
//! hold back orders: a newer snapshot cuts them short and starts its own.
use super::presentation::{Z_UNIT, depth, tinted, unit_size};
use super::puppet::{ClipArt, Puppetry};
use super::{Session, camera_rect};
use bevy::{prelude::*, window::PrimaryWindow};
use fourx_content::animation::{Clip, facing};
use fourx_sim::{Id, March, terrain::Coord};
use std::collections::HashSet;

/// How long one step takes, unhurried.
pub const STEP_SECS: f32 = 0.4;
/// The longest a walk may take: a long railway journey speeds up to fit.
pub const LONGEST_WALK: f32 = 2.4;
/// Where a unit stands in a city square, as the map draws it.
const CITY_SHIFT: Vec2 = Vec2::new(62.0, 24.0);
/// How far a unit's sprite is drawn above its square.
const LIFT: f32 = 12.0;
/// A walk is shown when it passes within this much of the camera's view (about a tile).
const VIEW_MARGIN: f32 = 128.0;

#[derive(Resource, Default)]
pub(super) struct Walks {
    waiting: Vec<March>,
    walking: Vec<Walker>,
    clock: f32,
    hidden: HashSet<Id>,
}

struct Walker {
    unit: Id,
    /// Where the sprite stands on each square (beside the city on a city square).
    path: Vec<Vec2>,
    /// The facing of each step, from the squares themselves: the city shift must not turn it.
    faces: Vec<u32>,
    step: f32,
    art: Option<ClipArt>,
    entity: Entity,
}

impl Walker {
    fn length(&self) -> f32 {
        self.step * (self.path.len() - 1) as f32
    }

    /// Where the walker is `t` seconds in and which way it looks.
    fn at(&self, t: f32) -> (Vec2, u32) {
        let steps = self.path.len() - 1;
        let s = (t / self.step).clamp(0.0, steps as f32);
        let i = (s.floor() as usize).min(steps - 1);
        let (a, b) = (self.path[i], self.path[i + 1]);
        (a.lerp(b, s - i as f32), self.faces[i])
    }
}

impl Walks {
    /// Takes the marches a new snapshot carries.
    pub(super) fn enqueue(&mut self, marches: &[March]) {
        self.waiting.extend(marches.iter().cloned());
    }

    /// Whether a unit is out of sight because a walker stands in for it.
    pub(super) fn hides(&self, unit: Id) -> bool {
        self.hidden.contains(&unit)
    }

    /// Whether walks are playing or about to.
    pub(super) fn busy(&self) -> bool {
        !self.waiting.is_empty() || !self.walking.is_empty()
    }
}

/// Seconds a step takes in a walk of `steps` steps.
pub(super) fn step_secs(steps: usize) -> f32 {
    STEP_SECS.min(LONGEST_WALK / steps.max(1) as f32)
}

/// Starts the walks of a new snapshot, advances the ones under way, and ends them.
#[allow(clippy::too_many_arguments)]
pub(super) fn walk(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<AssetServer>,
    mut session: ResMut<Session>,
    mut walks: ResMut<Walks>,
    mut puppetry: Puppetry,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Transform, &Projection), With<Camera2d>>,
    mut sprites: Query<(&mut Transform, &mut Sprite), (With<WalkerSprite>, Without<Camera2d>)>,
) {
    if !walks.waiting.is_empty() {
        finish(&mut commands, &mut walks, &mut puppetry, &mut session);
        let marches = std::mem::take(&mut walks.waiting);
        let (Ok(window), Ok((camera, projection))) = (windows.single(), cameras.single()) else {
            return;
        };
        let view = camera_rect(camera, projection, window).grow(VIEW_MARGIN);
        for march in marches {
            if let Some(walker) = start(
                &mut commands,
                &session,
                &assets,
                &mut puppetry,
                &march,
                view,
            ) {
                walks.hidden.insert(walker.unit);
                walks.walking.push(walker);
            }
        }
        walks.clock = 0.0;
        if !walks.walking.is_empty() {
            session.built = None;
        }
    }
    if walks.walking.is_empty() {
        return;
    }
    walks.clock += time.delta_secs().min(0.1);
    let t = walks.clock;
    for walker in &walks.walking {
        let (at, face) = walker.at(t);
        if let Ok((mut transform, mut sprite)) = sprites.get_mut(walker.entity) {
            transform.translation = Vec3::new(at.x, at.y + LIFT, Z_UNIT + depth(at.y));
            if let Some(art) = &walker.art
                && puppetry.images.contains(&art.image)
            {
                art.show(&mut sprite, face, art.looping(t));
            }
        }
    }
    // The walkers that arrive first wait on their squares, so the map is rebuilt once.
    if walks.walking.iter().all(|w| t >= w.length()) {
        finish(&mut commands, &mut walks, &mut puppetry, &mut session);
    }
}

#[derive(Component)]
pub(super) struct WalkerSprite;

/// A walker for a march, if any of it is in view.
fn start(
    commands: &mut Commands,
    session: &Session,
    assets: &AssetServer,
    puppetry: &mut Puppetry,
    march: &March,
    view: super::WorldRect,
) -> Option<Walker> {
    let game = session.game.as_ref()?;
    let def = session.rules.units.get(&march.kind)?;
    if march.path.len() < 2 {
        return None;
    }
    let spot = |p: Coord| {
        let (x, y) = p.screen();
        let at = Vec2::new(x, y);
        if game.city_at(p).is_some() {
            at + CITY_SHIFT
        } else {
            at
        }
    };
    let path: Vec<Vec2> = march.path.iter().map(|p| spot(*p)).collect();
    let faces = steps_facing(&march.path);
    if !path.iter().any(|p| view.has(*p)) {
        return None;
    }
    let art = puppetry.art.clip(
        &session.asset_prefix,
        &session.pack.id,
        &session.pack.visuals,
        assets,
        &mut puppetry.layouts,
        &march.kind,
        Clip::Run,
    );
    let color = if march.owner == session.player {
        Color::WHITE
    } else {
        tinted(game, march.owner)
    };
    let size = unit_size(def);
    let first = path[0];
    let sprite = match &art {
        Some(clip) => clip.sprite(faces[0], 0, size, color),
        None => {
            let mut sprite =
                Sprite::from_image(assets.load(format!("{}{}", session.asset_prefix, def.sprite)));
            sprite.custom_size = Some(size);
            sprite.color = color;
            sprite
        }
    };
    let entity = commands
        .spawn((
            sprite,
            Transform::from_xyz(first.x, first.y + LIFT, Z_UNIT + depth(first.y)),
            WalkerSprite,
        ))
        .id();
    Some(Walker {
        unit: march.unit,
        step: step_secs(path.len() - 1),
        path,
        faces,
        art,
        entity,
    })
}

/// The facing of each step of a path of squares.
fn steps_facing(path: &[Coord]) -> Vec<u32> {
    path.windows(2)
        .map(|w| {
            let ((x0, y0), (x1, y1)) = (w[0].screen(), w[1].screen());
            facing(x1 - x0, y1 - y0)
        })
        .collect()
}

/// Ends every walk: the walkers go, each unit keeps the way it last looked, the map shows the
/// real units again.
fn finish(
    commands: &mut Commands,
    walks: &mut Walks,
    puppetry: &mut Puppetry,
    session: &mut Session,
) {
    for walker in walks.walking.drain(..) {
        let (_, face) = walker.at(walker.length());
        puppetry.facings.turn(walker.unit, face);
        commands.entity(walker.entity).despawn();
    }
    if !walks.hidden.is_empty() {
        walks.hidden.clear();
        session.built = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn walker(squares: &[Coord]) -> Walker {
        Walker {
            unit: 1,
            path: squares.iter().map(|p| Vec2::from(p.screen())).collect(),
            faces: steps_facing(squares),
            step: step_secs(squares.len() - 1),
            art: None,
            entity: Entity::PLACEHOLDER,
        }
    }

    #[test]
    fn a_walker_crosses_square_by_square_facing_each_step() {
        let (a, b, c) = (Coord::new(0, 0), Coord::new(1, 1), Coord::new(2, 1));
        let screen = |p: Coord| p.screen();
        let w = walker(&[a, b, c]);
        assert_eq!(w.length(), 2.0 * STEP_SECS);
        let (start, face) = w.at(0.0);
        assert_eq!(start, Vec2::from(screen(a)));
        assert_eq!(face, 1, "a (1, 1) step walks south, toward the viewer");
        let (mid, _) = w.at(STEP_SECS * 0.5);
        assert!(mid.distance(Vec2::from(screen(a)).lerp(Vec2::from(screen(b)), 0.5)) < 1e-3);
        let (end, face) = w.at(10.0);
        assert_eq!(end, Vec2::from(screen(c)));
        assert_eq!(face, 2, "and the (1, 0) step south-east");
    }

    #[test]
    fn long_journeys_speed_up_to_fit() {
        assert_eq!(step_secs(1), STEP_SECS);
        assert_eq!(step_secs(3), STEP_SECS);
        assert!((step_secs(40) * 40.0 - LONGEST_WALK).abs() < 1e-4);
    }
}
