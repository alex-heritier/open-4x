//! Cultural borders: Civ3's bead-chain ribbon along the edges of a city's
//! plots.
//!
//! Art is `Art/Terrain/Territory.pcx`, a 2x4 sheet of 128x72 cells (the
//! exe slices it at `0x4C6A4B`, x by 128 and y by 72): the row picks the
//! tile edge, the column a straight or a corner variant. Each cell holds
//! one tile diamond inset 4 px top and bottom, so the diamond's center is
//! the cell's center and `Anchor::CENTER` puts it on the tile. The game
//! draws the straight column; the corner column is unused here.
//!
//! A tile draws a ribbon on an edge only when that edge faces a different
//! owner (another city, or nobody) and the tile is owned itself, so the
//! ribbon stays inside its own territory. Plots are per city, so two
//! cities of one civ draw their own ribbons either side of the shared
//! edge, the internal borders the game shows between its cities.

use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

use crate::cities::{territory, City, CIV_COLOR};
use crate::map::{tile_to_world, GameMap};
use crate::render::{border_z, fog_for, Fog, RevealAll};

/// Tie-break inside `render::border_z`'s band, which sits above all
/// terrain and below the fog diamonds and the sprite phase.
const BORDER_LAYER: f32 = 0.0;

/// One edge of a tile, named by the map neighbor it faces. `tile_to_world`
/// is y-up world space and puts `(x+1, y)` at `(+64, -32)`, down and to
/// the right on screen, so that edge is the diamond's lower-right one;
/// `(x, y+1)` is lower-left, `(x-1, y)` upper-left and `(x, y-1)`
/// upper-right. `tools/prep_assets.py borders` names the sheet cells after
/// these.
#[derive(Component, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Side {
    Yp,
    Xp,
    Xm,
    Ym,
}

impl Side {
    pub const ALL: [Side; 4] = [Side::Yp, Side::Xp, Side::Xm, Side::Ym];

    pub fn dir(self) -> (i32, i32) {
        match self {
            Side::Yp => (0, 1),
            Side::Xp => (1, 0),
            Side::Xm => (-1, 0),
            Side::Ym => (0, -1),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Side::Yp => "yp",
            Side::Xp => "xp",
            Side::Xm => "xm",
            Side::Ym => "ym",
        }
    }

    /// Tie-break for sprites sharing a tile, so the painter order is fixed.
    fn order(self) -> f32 {
        Self::ALL.iter().position(|s| *s == self).unwrap() as f32
    }
}

#[derive(Resource)]
pub struct BorderArt {
    defs: HashMap<Side, Handle<Image>>,
}

impl BorderArt {
    pub fn load(asset_server: &AssetServer) -> Self {
        let mut defs = HashMap::new();
        for side in Side::ALL {
            defs.insert(
                side,
                asset_server.load(format!("gen/borders/border_{}.png", side.name())),
            );
        }
        Self { defs }
    }
}

#[derive(Component)]
pub struct BorderSprite {
    x: i32,
    y: i32,
    side: Side,
}

/// True when `(x, y)` is owned and the tile across `side` is not (a
/// different city, or nobody). Off-map north and south count as nobody.
pub fn has_border(
    map: &GameMap,
    owner: &HashMap<(i32, i32), usize>,
    x: i32,
    y: i32,
    side: Side,
) -> bool {
    let Some(here) = owner.get(&(x, y)) else {
        return false;
    };
    let (dx, dy) = side.dir();
    let ny = y + dy;
    if ny < 0 || ny >= map.h {
        return true;
    }
    owner.get(&(map.wrap_x(x + dx), ny)) != Some(here)
}

/// Spawn the ribbons a territory has and drop the ones it lost.
pub fn sync_borders(
    mut commands: Commands,
    map: Res<GameMap>,
    art: Res<BorderArt>,
    reveal: Res<RevealAll>,
    cities: Query<&City>,
    mut q: Query<(Entity, &BorderSprite, &mut Sprite, &mut Visibility)>,
) {
    let list: Vec<&City> = cities.iter().collect();
    let owner = territory(&map, &list);
    let mut have: HashSet<(i32, i32, Side)> = HashSet::new();
    for (e, bs, mut sprite, mut vis) in q.iter_mut() {
        if !has_border(&map, &owner, bs.x, bs.y, bs.side) {
            commands.entity(e).despawn();
            continue;
        }
        have.insert((bs.x, bs.y, bs.side));
        // The fog diamonds above hold the dimming, so the ribbon only has
        // to hide where the tile was never seen.
        sprite.color = CIV_COLOR;
        *vis = match fog_for(reveal.0, &map.tiles[map.idx(bs.x, bs.y)]) {
            Fog::Black => Visibility::Hidden,
            _ => Visibility::Visible,
        };
    }
    for y in 0..map.h {
        for x in 0..map.w {
            for side in Side::ALL {
                if have.contains(&(x, y, side))
                    || !has_border(&map, &owner, x, y, side)
                {
                    continue;
                }
                let vis = match fog_for(reveal.0, &map.tiles[map.idx(x, y)]) {
                    Fog::Black => Visibility::Hidden,
                    _ => Visibility::Visible,
                };
                let pos = tile_to_world(x, y);
                commands.spawn((
                    Sprite {
                        image: art.defs[&side].clone(),
                        color: CIV_COLOR,
                        ..default()
                    },
                    vis,
                    Transform::from_xyz(
                        pos.x,
                        pos.y,
                        border_z(x, y, BORDER_LAYER + side.order() * 0.1),
                    ),
                    BorderSprite { x, y, side },
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cities::City;
    use std::collections::HashSet;

    fn city(name: &str, x: i32, y: i32, culture: u32, founded: u32) -> City {
        City {
            name: name.to_string(),
            x,
            y,
            size: 1,
            food: 0,
            shields: 0,
            production: crate::cities::Production::Warrior,
            queue: vec![],
            buildings: vec![],
            worked: HashSet::new(),
            culture,
            founded,
        }
    }

    #[test]
    fn one_city_borders_the_inside_of_its_own_edge() {
        let map = GameMap::generate();
        let (cx, cy) = (40, 30);
        let c = city("Kyoto", cx, cy, 0, 1);
        let owner = territory(&map, &[&c]);
        // The tile two along y is the edge of the claim, and its edge
        // facing y+1 (lower-left on screen) is unclaimed land: it draws.
        assert!(has_border(&map, &owner, cx, cy + 2, Side::Yp));
        assert!(has_border(&map, &owner, cx + 2, cy, Side::Xp));
        // Edges between its own plots do not.
        assert!(!has_border(&map, &owner, cx, cy, Side::Yp));
        assert!(!has_border(&map, &owner, cx + 1, cy, Side::Xm));
        // A corner is outside the claim, so nothing draws there.
        assert!(!has_border(&map, &owner, cx + 2, cy + 2, Side::Yp));
        // Unowned tiles never draw: the ribbon is inside the territory.
        let far = (cx + 20, cy);
        assert!(!has_border(&map, &owner, far.0, far.1, Side::Xm));
    }

    #[test]
    fn map_edges_count_as_unowned() {
        let map = GameMap::generate();
        let c = city("Kyoto", 5, 0, 0, 1);
        let owner = territory(&map, &[&c]);
        assert!(has_border(&map, &owner, 5, 0, Side::Ym));
    }

    #[test]
    fn neighbours_claim_their_side_of_a_shared_edge() {
        // Two cities of one civ, four tiles apart: every tile belongs to
        // the nearer city, and the edge between them draws from both
        // sides, which is how Civ3 shows internal borders.
        let map = GameMap::generate();
        let a = city("Kyoto", 40, 30, 0, 1);
        let b = city("Osaka", 44, 30, 0, 2);
        let owner = territory(&map, &[&a, &b]);
        assert_eq!(owner[&(42, 30)], 0);
        assert_eq!(owner[&(43, 30)], 1);
        assert!(has_border(&map, &owner, 42, 30, Side::Xp));
        assert!(has_border(&map, &owner, 43, 30, Side::Xm));
        assert!(!has_border(&map, &owner, 42, 30, Side::Xm));
    }

    #[test]
    fn growth_moves_the_border_out() {
        let map = GameMap::generate();
        let small = city("Kyoto", 40, 30, 0, 1);
        let grown = city("Kyoto", 40, 30, 10, 1);
        let before = territory(&map, &[&small]);
        let after = territory(&map, &[&grown]);
        assert!(before.len() < after.len());
        assert!(has_border(&map, &after, 40, 33, Side::Yp));
        assert!(!has_border(&map, &before, 40, 33, Side::Yp));
    }
}
