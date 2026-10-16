//! The dressing of a panel: a surface (aged paper, slate or felt), a gilt frame with a boss or a
//! scroll at each corner, and a cartouche in the middle of each side. All of it is pictures of the
//! pack's `ui/chrome/` (`art/forge/chrome.py`); the node under them keeps a flat fill of the same
//! colour, so a pack without them still gets plain panels.
//!
//! Call [`dress`] first inside a panel's `with_children`: it spawns absolute layers that sit under
//! whatever the caller adds after it. The panel itself needs its own size, its padding (the frame's
//! band takes the first 20 px or so) and a fill; see `theme::card`.
use super::*;
use bevy::sprite::{BorderRect, SliceScaleMode, TextureSlicer};
use fourx_sim::terrain::{Cover, Relief, Tile};

/// What a panel is made of.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Skin {
    /// Aged paper, with dark ink on it.
    Paper,
    /// Dark slate, with cream and gold on it.
    Slate,
    /// Dark green felt: the day button.
    Felt,
}

impl Skin {
    fn tile(self) -> &'static str {
        match self {
            Skin::Paper => "parchment",
            Skin::Slate => "slate",
            Skin::Felt => "felt",
        }
    }
}

/// How heavy a frame is: the grand one for windows and panes, the small one for what is under
/// about eighty pixels tall.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Frame {
    Grand,
    Small,
}

impl Frame {
    /// The file, and the width of its corners in the picture's own pixels.
    fn picture(self) -> (&'static str, f32) {
        match self {
            Frame::Grand => ("frame", 64.0),
            Frame::Small => ("frame_small", 40.0),
        }
    }
}

/// A file of the pack's chrome.
pub(crate) fn picture(assets: &AssetServer, prefix: &str, name: &str) -> Handle<Image> {
    assets.load(format!("{prefix}ui/chrome/{name}.png"))
}

/// A layer as big as its panel.
fn layer() -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: px(0),
        top: px(0),
        width: percent(100),
        height: percent(100),
        ..default()
    }
}

/// The panel's surface: a tile of its skin, `alpha` opaque over the panel's own fill (which a
/// button repaints as the pointer crosses it).
pub(crate) fn texture(
    parent: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    prefix: &str,
    skin: Skin,
    alpha: f32,
) {
    // The tile is authored at twice the density it is drawn at.
    parent.spawn((
        layer(),
        ImageNode::new(picture(assets, prefix, skin.tile()))
            .with_color(Color::WHITE.with_alpha(alpha))
            .with_mode(NodeImageMode::Tiled {
                tile_x: true,
                tile_y: true,
                stretch_value: 0.5,
            }),
    ));
}

/// The gilt frame over a panel.
pub(crate) fn trim(
    parent: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    prefix: &str,
    frame: Frame,
) {
    let (name, corner) = frame.picture();
    // The frame is authored at twice the density too: its corners are drawn at half size.
    parent.spawn((
        layer(),
        ImageNode::new(picture(assets, prefix, name)).with_mode(NodeImageMode::Sliced(
            TextureSlicer {
                border: BorderRect::all(corner),
                center_scale_mode: SliceScaleMode::Stretch,
                sides_scale_mode: SliceScaleMode::Stretch,
                max_corner_scale: 0.5,
            },
        )),
    ));
}

/// The surface, the frame over it and, if asked, the side ornaments. `ornaments` is for panels
/// big enough that a cartouche on each side does not crowd the corners.
pub(crate) fn dress(
    parent: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    prefix: &str,
    skin: Skin,
    frame: Frame,
    ornaments: bool,
) {
    texture(parent, assets, prefix, skin, 1.0);
    trim(parent, assets, prefix, frame);
    if !ornaments {
        return;
    }
    // Lozenges on the middles of the four sides, over the band (which is about 7 px wide).
    let (long, short) = (36.0, 18.0);
    for bottom in [false, true] {
        let mut node = Node {
            position_type: PositionType::Absolute,
            left: percent(50),
            margin: UiRect::left(px(-long / 2.0)),
            width: px(long),
            height: px(short),
            ..default()
        };
        if bottom {
            node.bottom = px(-2);
        } else {
            node.top = px(-2);
        }
        parent.spawn((node, ImageNode::new(picture(assets, prefix, "cartouche_h"))));
    }
    for right in [false, true] {
        let mut node = Node {
            position_type: PositionType::Absolute,
            top: percent(50),
            margin: UiRect::top(px(-long / 2.0)),
            width: px(short),
            height: px(long),
            ..default()
        };
        if right {
            node.right = px(-2);
        } else {
            node.left = px(-2);
        }
        parent.spawn((node, ImageNode::new(picture(assets, prefix, "cartouche_v"))));
    }
}

/// A painted emblem of the pack, `side` px across.
pub(crate) fn emblem(
    parent: &mut ChildSpawnerCommands,
    assets: &AssetServer,
    prefix: &str,
    name: &str,
    side: f32,
) {
    parent.spawn((
        ImageNode::new(picture(assets, prefix, &format!("ico_{name}"))),
        Node {
            width: px(side),
            height: px(side),
            flex_shrink: 0.0,
            ..default()
        },
    ));
}

/// The landscape that stands for a square in the terrain panel: its relief if it has any, else
/// its cover, else its ground.
pub(crate) fn scene_for(tile: &Tile) -> &'static str {
    use fourx_sim::terrain::Terrain;
    match (tile.relief, tile.cover, tile.terrain) {
        (Relief::Mountains, _, _) => "scene_mountains",
        (Relief::Hills, _, _) => "scene_hills",
        (_, Cover::Forest, _) => "scene_forest",
        (_, Cover::Jungle, _) => "scene_jungle",
        (_, Cover::Marsh, _) => "scene_marsh",
        (_, _, Terrain::Ocean) => "scene_ocean",
        (_, _, Terrain::Sea) => "scene_sea",
        (_, _, Terrain::Coast) => "scene_coast",
        (_, _, Terrain::Grass) => "scene_grass",
        (_, _, Terrain::Plains) => "scene_plains",
        (_, _, Terrain::Desert) => "scene_desert",
        (_, _, Terrain::Tundra) => "scene_tundra",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fourx_sim::terrain::Terrain;

    fn tile(terrain: Terrain, relief: Relief, cover: Cover) -> Tile {
        let mut tile = Tile::new(fourx_sim::terrain::Coord::new(0, 0), terrain);
        tile.relief = relief;
        tile.cover = cover;
        tile
    }

    #[test]
    fn a_square_is_pictured_by_its_relief_then_its_cover_then_its_ground() {
        assert_eq!(
            scene_for(&tile(Terrain::Grass, Relief::Mountains, Cover::Forest)),
            "scene_mountains"
        );
        assert_eq!(
            scene_for(&tile(Terrain::Plains, Relief::Hills, Cover::Bare)),
            "scene_hills"
        );
        assert_eq!(
            scene_for(&tile(Terrain::Desert, Relief::Flat, Cover::Jungle)),
            "scene_jungle"
        );
        assert_eq!(
            scene_for(&tile(Terrain::Coast, Relief::Flat, Cover::Bare)),
            "scene_coast"
        );
    }

    #[test]
    fn every_ground_has_a_scene() {
        for terrain in Terrain::ALL {
            let name = scene_for(&tile(terrain, Relief::Flat, Cover::Bare));
            assert!(name.starts_with("scene_"), "{terrain:?}");
        }
    }
}
