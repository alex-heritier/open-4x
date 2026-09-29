//! Terrain rendering and fog of war.
//!
//! Fog is a layer of diamond sprites above all terrain, not a per-sprite
//! tint: tall art (canopy, hills, peaks) overlaps neighboring tiles, so a
//! sprite tinted for its own tile would spill brightness (or black) onto
//! tiles with a different fog state.
//!
//! Each diamond is one cell of Civ3's `FogOfWar` sheet, addressed from the
//! diamond's four vertex fog states exactly like the terrain blend sheets,
//! so lit, remembered and never-seen terrain meet with the soft, speckled
//! frontier the game's own art draws.

use bevy::prelude::*;
use bevy::sprite::Anchor;

use crate::blend::{cell_rect, VERTEX_TILES};
use crate::map::*;
use crate::tiles::TileArt;

/// Tree-cover sprite; despawned when the cover is cleared.
#[derive(Component)]
pub struct CoverLayer;

/// Tall art (canopy, hill, peak) drawn over the base diamond. Hidden on
/// unseen tiles instead of tinted black, so it never spills onto visible
/// neighbors; the tile's own black diamond covers what it hides.
#[derive(Component)]
pub struct OverlayLayer;

#[derive(Component)]
pub struct TileSprite {
    pub x: i32,
    pub y: i32,
}

/// Fog-of-war diamond for one tile.
#[derive(Component)]
pub struct FogSprite {
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

/// Z for cultural border ribbons: above every terrain row, so neither the
/// next row's ground nor a forest or hill canopy paints over the ribbon,
/// and below the fog diamonds (`fog_z` is 50 higher, more than a row
/// step), which dim it with the tile.
pub fn border_z(x: i32, y: i32, layer: f32) -> f32 {
    tile_z(x, y, layer) + 200.0
}

/// Z for fog diamonds: above every terrain row and improvement layer, so
/// bright tall art never spills past a fog boundary, and below units and
/// cities, which manage their own visibility.
pub fn fog_z(x: i32, y: i32) -> f32 {
    (x + y) as f32 + 250.0
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fog {
    Clear,
    Dim,
    Black,
}

pub fn fog_for(reveal: bool, t: &Tile) -> Fog {
    if reveal || t.visible {
        Fog::Clear
    } else if t.seen {
        Fog::Dim
    } else {
        Fog::Black
    }
}

/// Civ3's fog sheet, written by the `fog` stage of `tools/prep_assets.py`:
/// 9x9 cells of 128x64 that are black with the terrain's remaining
/// brightness in the alpha channel.
pub const FOG_SHEET: &str = "gen/terrain/fog.png";

/// How much of a tile the fog leaves: 0 never seen, 1 remembered, 2 lit.
fn vertex_fog(reveal: bool, t: &Tile) -> u32 {
    if reveal || t.visible {
        2
    } else if t.seen {
        1
    } else {
        0
    }
}

/// Sheet cell (column, row) of the fog diamond covering a tile: the terrain
/// sheets' per-vertex addressing (`col = 3*W + N`, `row = 3*S + E`), with
/// each vertex taking the most revealed of the four tiles that touch it.
/// Every tile sharing a vertex agrees, so the gradients line up across
/// tiles the way the sheet's dithered corners are drawn.
pub fn fog_cell(map: &GameMap, reveal: bool, x: i32, y: i32) -> (u32, u32) {
    let mut v = [0u32; 4];
    for (i, offs) in VERTEX_TILES.iter().enumerate() {
        v[i] = offs
            .iter()
            .filter_map(|(dx, dy)| map.get(x + dx, y + dy))
            .map(|t| vertex_fog(reveal, t))
            .max()
            .unwrap_or(0);
    }
    // N, E, S, W order, as in `blend::cell_for`.
    (3 * v[3] + v[0], 3 * v[2] + v[1])
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
    let fog = assets.load(FOG_SHEET);
    // Ground: one blended cell per tile corner (`blend::corner_cell`). Row
    // -1 holds the cells over the north half of map row 0. The cells tile
    // the plane without overlap, so a cell sorts with its top tile.
    for y in -1..map.h {
        for x in 0..map.w {
            let (stem, col, row) = crate::blend::corner_cell(&map, x, y);
            let pos = tile_to_world(x, y) + crate::blend::CORNER_OFFSET;
            commands.spawn((
                Sprite {
                    image: assets.load(crate::blend::sheet_path(stem)),
                    rect: Some(cell_rect(col, row)),
                    ..default()
                },
                Transform::from_xyz(pos.x, pos.y, tile_z(x, y, 0.0)),
            ));
        }
    }
    for y in 0..map.h {
        for x in 0..map.w {
            let t = &map.tiles[map.idx(x, y)];
            let pos = tile_to_world(x, y);
            let (fog_col, fog_row) = fog_cell(&map, false, x, y);
            commands.spawn((
                Sprite {
                    image: fog.clone(),
                    rect: Some(cell_rect(fog_col, fog_row)),
                    ..default()
                },
                Transform::from_xyz(pos.x, pos.y, fog_z(x, y)),
                FogSprite { x, y },
            ));
            // Ice keeps its own unblended art over the tundra the cells
            // give it.
            if t.base == Base::Ice {
                let base = base_name(t);
                commands.spawn((
                    Sprite {
                        image: art.defs[&base].image.clone(),
                        ..default()
                    },
                    art.anchor(&base),
                    Transform::from_xyz(pos.x, pos.y, tile_z(x, y, 0.5)),
                    TileSprite { x, y },
                ));
            }
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
                    OverlayLayer,
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
                    OverlayLayer,
                ));
            }
        }
    }
}

pub fn update_fog(
    map: Res<GameMap>,
    reveal: Res<RevealAll>,
    mut terrain: Query<
        (
            &TileSprite,
            Option<&OverlayLayer>,
            &mut Sprite,
            &mut Visibility,
        ),
        Without<FogSprite>,
    >,
    mut fog: Query<
        (&FogSprite, &mut Sprite, &mut Visibility),
        Without<TileSprite>,
    >,
) {
    for (ts, overlay, mut sprite, mut vis) in terrain.iter_mut() {
        let t = &map.tiles[map.idx(ts.x, ts.y)];
        match (fog_for(reveal.0, t), overlay.is_some()) {
            // Tall art on unseen tiles hides; the tile's own black diamond
            // covers it, and nothing spills onto visible neighbors.
            (Fog::Black, true) => *vis = Visibility::Hidden,
            _ => {
                sprite.color = Color::WHITE;
                *vis = Visibility::Visible;
            }
        }
    }
    for (fs, mut sprite, mut vis) in fog.iter_mut() {
        let t = &map.tiles[map.idx(fs.x, fs.y)];
        // A lit tile lights all four of its vertices, so its cell is the
        // sheet's clear one and the sprite can stay off.
        if fog_for(reveal.0, t) == Fog::Clear {
            *vis = Visibility::Hidden;
            continue;
        }
        let (col, row) = fog_cell(&map, reveal.0, fs.x, fs.y);
        sprite.rect = Some(cell_rect(col, row));
        *vis = Visibility::Visible;
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

    fn fog_tile(visible: bool, seen: bool) -> Tile {
        Tile {
            base: Base::Grassland,
            relief: Relief::Flat,
            cover: Cover::Bare,
            variant: 0,
            seen,
            visible,
            hut: false,
            camp: false,
            resource: None,
            road: false,
            irrigation: false,
            mine: false,
        }
    }

    #[test]
    fn fog_states_follow_visibility() {
        assert_eq!(fog_for(false, &fog_tile(true, true)), Fog::Clear);
        assert_eq!(fog_for(false, &fog_tile(false, true)), Fog::Dim);
        assert_eq!(fog_for(false, &fog_tile(false, false)), Fog::Black);
        assert_eq!(fog_for(true, &fog_tile(false, false)), Fog::Clear);
    }

    #[test]
    fn fog_sorts_between_terrain_and_sprites() {
        // Every fog diamond clears the tallest terrain anywhere (mines at
        // layer 1.7 on the farthest row) and every unit/city sprite clears
        // every fog diamond, whatever the rows.
        assert!(fog_z(0, 0) > tile_z(MAP_W - 1, MAP_H - 1, 1.7));
        assert!(sprite_z(0, 0, 0.0) > fog_z(MAP_W - 1, MAP_H - 1));
    }

    fn all_tiles(visible: bool, seen: bool) -> GameMap {
        let mut map = GameMap::generate();
        for t in map.tiles.iter_mut() {
            t.visible = visible;
            t.seen = seen;
        }
        map
    }

    #[test]
    fn fog_cells_follow_the_four_vertex_states() {
        let (x, y) = (4, 4);
        // Uniform sheets: every vertex never seen, remembered, lit.
        assert_eq!(fog_cell(&all_tiles(false, false), false, x, y), (0, 0));
        assert_eq!(fog_cell(&all_tiles(false, true), false, x, y), (4, 4));
        assert_eq!(fog_cell(&all_tiles(true, true), false, x, y), (8, 8));
        // One lit tile lights all four of its vertices, so it takes the
        // clear cell while its neighbours take a cell that is clear on the
        // corners they share with it.
        let mut map = all_tiles(false, true);
        let lit = map.idx(x, y);
        map.tiles[lit].visible = true;
        assert_eq!(fog_cell(&map, false, x, y), (8, 8));
        assert_eq!(fog_cell(&map, false, x + 1, y), (8, 4));
        assert_eq!(fog_cell(&map, false, x, y - 1), (7, 7));
        // Reveal-all lands on the all-lit cell, which the sheet draws clear.
        let unseen = all_tiles(false, false);
        assert_eq!(fog_cell(&unseen, true, x, y), (8, 8));
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
