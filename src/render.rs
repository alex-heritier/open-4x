//! Terrain rendering and fog tinting.

use bevy::prelude::*;
use bevy::sprite::Anchor;

use crate::map::*;
use crate::tiles::TileArt;

/// Tree-cover sprite; despawned when the cover is cleared.
#[derive(Component)]
pub struct CoverLayer;

#[derive(Component)]
pub struct TileSprite {
    pub x: i32,
    pub y: i32,
}

/// Debug reveal: everything counts as visible. Real fog arrives with units.
#[derive(Resource, Default)]
pub struct RevealAll(pub bool);

pub fn tile_z(x: i32, y: i32, layer: f32) -> f32 {
    // Painter's order runs north to south: southern rows sort above
    // northern ones so tall overlays (canopy, hills, peaks) overlap the
    // tiles above them instead of being clipped behind them.
    (x + y) as f32 + layer * 0.001
}

/// Z for tall sprites (units, cities, banners). Terrain rows are 1.0 apart,
/// so within-row layers can never clear northern tiles; sprites live in a
/// separate phase above all terrain and sort among themselves by row.
pub fn sprite_z(x: i32, y: i32, layer: f32) -> f32 {
    tile_z(x, y, layer) + 500.0
}

pub(crate) fn base_name(t: &Tile) -> String {
    let b = match t.base {
        Base::Ocean => "ocean",
        Base::Sea => "sea",
        Base::Coast => "coast",
        Base::Ice => "ice",
        Base::Grassland => "grassland",
        Base::Plains => "plains",
        Base::Desert => "desert",
        Base::Tundra => "tundra",
    };
    format!("{b}_{}", t.variant % 3)
}

pub(crate) fn overlay_name(t: &Tile) -> Option<String> {
    match t.relief {
        Relief::Mountain => {
            let snow = matches!(t.base, Base::Tundra);
            let v = t.variant % 2;
            Some(if snow {
                format!("mtnsnow_{v}")
            } else {
                format!("mtn_{v}")
            })
        }
        Relief::Hill => Some(format!("hill_{}", t.variant % 3)),
        Relief::Flat => match t.cover {
            Cover::Forest => Some(format!("forest_{}", t.variant % 2)),
            Cover::Jungle => Some(format!("jungle_{}", t.variant % 2)),
            Cover::Pine => Some(format!("pine_{}", t.variant % 2)),
            Cover::Bare => None,
        },
    }
}

pub fn spawn_terrain(
    mut commands: Commands,
    map: Res<GameMap>,
    art: Res<TileArt>,
    assets: Res<AssetServer>,
) {
    for y in 0..map.h {
        for x in 0..map.w {
            let t = &map.tiles[map.idx(x, y)];
            let pos = tile_to_world(x, y);
            let base = base_name(t);
            let def = &art.defs[&base];
            // Blended cell from the transition sheets; ice keeps its own art.
            let (image, rect) = match crate::blend::cell_for(&map, x, y) {
                Some((stem, col, row)) => (
                    assets.load(crate::blend::sheet_path(stem)),
                    Some(crate::blend::cell_rect(col, row)),
                ),
                None => (def.image.clone(), None),
            };
            commands.spawn((
                Sprite {
                    image,
                    rect,
                    ..default()
                },
                art.anchor(&base),
                Transform::from_xyz(pos.x, pos.y, tile_z(x, y, 0.0)),
                TileSprite { x, y },
            ));
            if let Some(c) = crate::blend::cover_sprite(&map, x, y) {
                let size = c.rect.size();
                commands.spawn((
                    Sprite {
                        image: assets.load(&c.path),
                        rect: Some(c.rect),
                        ..default()
                    },
                    Anchor(Vec2::new(
                        c.anchor_px.x / size.x - 0.5,
                        0.5 - c.anchor_px.y / size.y,
                    )),
                    Transform::from_xyz(pos.x, pos.y, tile_z(x, y, 1.0)),
                    TileSprite { x, y },
                    CoverLayer,
                ));
            } else if let Some(ov) = overlay_name(t) {
                let odef = &art.defs[&ov];
                commands.spawn((
                    Sprite {
                        image: odef.image.clone(),
                        ..default()
                    },
                    art.anchor(&ov),
                    Transform::from_xyz(pos.x, pos.y, tile_z(x, y, 1.0)),
                    TileSprite { x, y },
                ));
            }
        }
    }
}

pub fn update_fog(
    map: Res<GameMap>,
    reveal: Res<RevealAll>,
    mut q: Query<(&TileSprite, &mut Sprite)>,
) {
    for (ts, mut sprite) in q.iter_mut() {
        let t = &map.tiles[map.idx(ts.x, ts.y)];
        sprite.color = if reveal.0 || t.visible {
            Color::WHITE
        } else if t.seen {
            Color::srgb(0.45, 0.45, 0.5)
        } else {
            Color::BLACK
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn painter_order_runs_north_to_south() {
        // Southern rows sort above northern ones; a tile's overlay sorts
        // above its own base and above every northern base it overlaps,
        // so tall canopy/hill/peak art is never clipped behind them.
        let (x, y) = (40, 30);
        assert!(tile_z(x, y, 0.0) > tile_z(x - 1, y, 0.0));
        assert!(tile_z(x, y, 0.0) > tile_z(x, y - 1, 0.0));
        assert!(tile_z(x, y, 1.0) > tile_z(x, y, 0.0));
        assert!(tile_z(x, y, 1.0) > tile_z(x - 1, y - 1, 0.0));
        assert!(tile_z(x, y + 1, 0.0) > tile_z(x, y, 1.0));
        assert!(sprite_z(x, y, 4.0) > tile_z(x, y + 1, 1.6));
    }
}

/// Remove cover sprites whose tile has been cleared by a worker.
pub fn sync_cover(
    mut commands: Commands,
    map: Res<GameMap>,
    q: Query<(Entity, &TileSprite), With<CoverLayer>>,
) {
    if !map.is_changed() {
        return;
    }
    for (e, ts) in q.iter() {
        if map.tiles[map.idx(ts.x, ts.y)].cover == Cover::Bare {
            commands.entity(e).despawn();
        }
    }
}
