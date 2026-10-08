//! Terrain art: converted tile images plus anchor metadata.

use bevy::prelude::*;
use bevy::sprite::Anchor;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
struct TileEntry {
    file: String,
    size: [u32; 2],
    anchor: [i32; 2],
}

pub struct TileDef {
    pub image: Handle<Image>,
    pub anchor_px: Vec2,
    pub size: Vec2,
}

#[derive(Resource)]
pub struct TileArt {
    pub defs: HashMap<String, TileDef>,
}

impl TileArt {
    pub fn load(asset_server: &AssetServer) -> Self {
        let text = crate::web::read_text(crate::assets::cache_path("terrain/manifest.json"))
            .expect("run from the repo root: the scenario art cache is built at startup");
        let raw: HashMap<String, TileEntry> =
            serde_json::from_str(&text).expect("terrain manifest parses");
        let mut defs = HashMap::new();
        for (name, e) in raw {
            defs.insert(
                name.clone(),
                TileDef {
                    image: asset_server.load(format!("terrain/{}", e.file)),
                    anchor_px: Vec2::new(e.anchor[0] as f32, e.anchor[1] as f32),
                    size: Vec2::new(e.size[0] as f32, e.size[1] as f32),
                },
            );
        }
        Self { defs }
    }

    pub fn anchor(&self, name: &str) -> Anchor {
        let d = &self.defs[name];
        Anchor(Vec2::new(
            d.anchor_px.x / d.size.x - 0.5,
            0.5 - d.anchor_px.y / d.size.y,
        ))
    }
}
