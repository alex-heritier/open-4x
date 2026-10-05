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
//! civilization (or nobody) and the tile is owned itself, so the ribbon
//! stays inside its own territory. Cities of one civ share their territory:
//! their borders merge into one outline, with no ribbon between them.

use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

use crate::cities::territory;
use crate::civs::CIVS;
use crate::map::{GameMap, tile_to_world};
use crate::render::{Fog, RevealAll, border_z, fog_for};

/// Tie-break inside `render::border_z`'s band, which sits above all
/// terrain and the fog diamonds and below the sprite phase.
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
                asset_server.load(format!("cache/borders/border_{}.png", side.name())),
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

/// True when `(x, y)` is owned and the tile across `side` is held by
/// another civ or nobody. `owner` holds civ ids (`territory`). Off-map
/// north and south count as nobody.
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

/// Ribbon tint for an owned tile: the color of the civilization that owns
/// it, so two civs meeting at a border draw their own colors on either side.
fn owner_color(owner: &HashMap<(i32, i32), usize>, x: i32, y: i32) -> Color {
    CIVS[owner[&(x, y)]].color
}

/// Spawn the ribbons a territory has and drop the ones it lost.
pub fn sync_borders(
    mut commands: Commands,
    map: Res<GameMap>,
    art: Res<BorderArt>,
    reveal: Res<RevealAll>,
    mut q: Query<(Entity, &BorderSprite, &mut Sprite, &mut Visibility)>,
) {
    let owner = territory(&map);
    let mut have: HashSet<(i32, i32, Side)> = HashSet::new();
    for (e, bs, mut sprite, mut vis) in q.iter_mut() {
        if !has_border(&map, &owner, bs.x, bs.y, bs.side) {
            commands.entity(e).despawn();
            continue;
        }
        have.insert((bs.x, bs.y, bs.side));
        // Civ3 draws a border at full strength on every tile ever seen, so
        // the ribbon sits above the fog and only hides where it is black.
        sprite.color = owner_color(&owner, bs.x, bs.y);
        *vis = match fog_for(reveal.0, &map.tiles[map.idx(bs.x, bs.y)]) {
            Fog::Black => Visibility::Hidden,
            _ => Visibility::Visible,
        };
    }
    for y in 0..map.h {
        for x in 0..map.w {
            for side in Side::ALL {
                if have.contains(&(x, y, side)) || !has_border(&map, &owner, x, y, side) {
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
                        color: owner_color(&owner, x, y),
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
    use crate::cities::{City, recompute_borders};
    use crate::civs::{CIV_CAP, civ_count};

    fn city(name: &str, x: i32, y: i32, culture: u32, founded: u32) -> City {
        City {
            gifts: vec![],
            goods: 0,
            coastal: false,
            river: false,
            unrest: 0,
            hurry_timer: 0,
            stakes: Default::default(),
            cooldown: 0,
            unit_clocks: Vec::new(),
            name: name.to_string(),
            x,
            y,
            civ: 0,
            diseased: false,
            citizens: crate::citizens::new_pool(0, 1),
            food: 0,
            shields: 0,
            production: crate::cities::Production::named("Warrior"),
            queue: vec![],
            buildings: vec![],
            culture,
            founded,
        }
    }

    /// Owners after a fresh recompute over a generated map.
    fn owners(map: &mut GameMap, cities: &[&City]) -> HashMap<(i32, i32), usize> {
        for t in &mut map.tiles {
            t.owner = None;
        }
        recompute_borders(map, cities, &[0; CIV_CAP]);
        territory(map)
    }

    #[test]
    fn one_city_borders_the_inside_of_its_own_edge() {
        let mut map = GameMap::generate();
        let (cx, cy) = (40, 30);
        let c = city("Kyoto", cx, cy, 0, 1);
        let owner = owners(&mut map, &[&c]);
        // The tile one along y is the edge of the claim, and its edge
        // facing y+1 (lower-left on screen) is unclaimed land: it draws.
        assert!(has_border(&map, &owner, cx, cy + 1, Side::Yp));
        assert!(has_border(&map, &owner, cx + 1, cy, Side::Xp));
        assert!(has_border(&map, &owner, cx + 1, cy + 1, Side::Yp));
        // Edges between its own plots do not.
        assert!(!has_border(&map, &owner, cx, cy, Side::Yp));
        assert!(!has_border(&map, &owner, cx + 1, cy, Side::Xm));
        // Tiles outside the 3x3 neighborhood do not draw borders.
        assert!(!has_border(&map, &owner, cx + 2, cy + 2, Side::Yp));
        // Unowned tiles never draw: the ribbon is inside the territory.
        let far = (cx + 20, cy);
        assert!(!has_border(&map, &owner, far.0, far.1, Side::Xm));
    }

    #[test]
    fn map_edges_count_as_unowned() {
        let mut map = GameMap::generate();
        let c = city("Kyoto", 5, 0, 0, 1);
        let owner = owners(&mut map, &[&c]);
        assert!(has_border(&map, &owner, 5, 0, Side::Ym));
    }

    #[test]
    fn cities_of_one_civ_share_one_outline() {
        // Two cities of one civ, three tiles apart, claim touching ground:
        // no ribbon runs between them, only around the pair.
        let mut map = GameMap::generate();
        let a = city("Kyoto", 40, 30, 0, 1);
        let b = city("Osaka", 43, 30, 0, 2);
        let owner = owners(&mut map, &[&a, &b]);
        assert!(!has_border(&map, &owner, 41, 30, Side::Xp));
        assert!(!has_border(&map, &owner, 42, 30, Side::Xm));
        // The outer edges still draw.
        assert!(has_border(&map, &owner, 39, 30, Side::Xm));
        assert!(has_border(&map, &owner, 44, 30, Side::Xp));
    }

    #[test]
    fn cities_of_two_civs_meet_in_a_frontier() {
        let mut map = GameMap::generate();
        let a = city("Kyoto", 40, 30, 0, 1);
        let mut b = city("Rome", 43, 30, 0, 2);
        b.civ = 1;
        let owner = owners(&mut map, &[&a, &b]);
        assert!(has_border(&map, &owner, 41, 30, Side::Xp));
        assert!(has_border(&map, &owner, 42, 30, Side::Xm));
        assert!(!has_border(&map, &owner, 41, 30, Side::Xm));
    }

    #[test]
    fn growth_moves_the_border_out() {
        let mut map = GameMap::generate();
        let small = city("Kyoto", 40, 30, 0, 1);
        let grown = city("Kyoto", 40, 30, 10, 1);
        let before = owners(&mut map, &[&small]);
        let after = owners(&mut map, &[&grown]);
        assert!(before.len() < after.len());
        assert!(has_border(&map, &after, 40, 32, Side::Yp));
        assert!(!has_border(&map, &before, 40, 32, Side::Yp));
    }

    /// Two civs meeting along a frontier each wear their own color, so a
    /// tile's ribbon tint follows `CIVS` through its owner.
    #[test]
    fn borders_take_the_owning_civs_color() {
        let mut map = GameMap::generate();
        let a = city("Kyoto", 40, 30, 0, 1);
        let mut b = city("Tenochtitlan", 43, 30, 0, 2);
        b.civ = 1;
        let owner = owners(&mut map, &[&a, &b]);
        assert_eq!(owner[&(41, 30)], a.civ);
        assert_eq!(owner[&(42, 30)], b.civ);
        assert_eq!(owner_color(&owner, 41, 30), CIVS[a.civ].color);
        assert_eq!(owner_color(&owner, 42, 30), CIVS[b.civ].color);
        // Distinct civs must not share a tint, or the frontier vanishes.
        assert_ne!(CIVS[a.civ].color, CIVS[b.civ].color);
    }
}
