//! Terrain pack manifest (`pack.json`).

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::compose::Blend;
use crate::geom::Kind;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum MaskSource {
    /// Transition shapes recovered from the original Civ3 sheets.
    #[default]
    Civ3,
    /// Noise-distorted vertex blends; needs no Civ3 install.
    Procedural,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Terrain {
    /// What to ask the image model for (the pack `style` is appended).
    #[serde(default)]
    pub prompt: String,
    /// Material image, relative to the pack. Default `<terrain>.png`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub texture: Option<String>,
    /// How many map tiles the material image spans edge to edge; larger
    /// means finer detail on screen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tiles_across: Option<f32>,
    /// Per-terrain override of the pack's `color_match`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_match: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
}

fn d_model() -> String {
    "fal-ai/flux/dev".into()
}
fn d_size() -> String {
    "square_hd".into()
}
fn d_seed() -> u32 {
    1
}
fn d_shore() -> f32 {
    2.0
}
fn d_darken() -> f32 {
    0.3
}
fn d_tiles() -> f32 {
    8.0
}
fn d_true() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Pack {
    pub name: String,
    /// Appended to every terrain prompt, so the set shares one look.
    #[serde(default)]
    pub style: String,
    /// fal.ai model id.
    #[serde(default = "d_model")]
    pub model: String,
    /// fal.ai `image_size` (`square_hd` is 1024x1024).
    #[serde(default = "d_size")]
    pub image_size: String,
    #[serde(default)]
    pub masks: MaskSource,
    #[serde(default)]
    pub blend: Blend,
    #[serde(default = "d_seed")]
    pub seed: u32,
    /// Default `tiles_across` for every terrain.
    #[serde(default = "d_tiles")]
    pub tiles_across: f32,
    /// Minimum beach width along coastlines, in pixels.
    #[serde(default = "d_shore")]
    pub shore_width: f32,
    /// Darkening of the line where water meets land, 0..1.
    #[serde(default = "d_darken")]
    pub shoreline_darken: f32,
    /// Recolour each material to the mean/contrast of the matching Civ3
    /// terrain, so mixed or AI-varied sets keep Civ3's palette.
    #[serde(default)]
    pub color_match: bool,
    /// Remove large-scale lighting from materials.
    #[serde(default = "d_true")]
    pub flatten: bool,
    /// `grassland plains desert tundra coast sea ocean`, plus optional
    /// `shore` (beach sand along coastlines).
    pub terrains: BTreeMap<String, Terrain>,
}

pub const SHORE: &str = "shore";

pub fn default_prompt(name: &str) -> &'static str {
    match name {
        "grassland" => "lush green grassland ground, short grass with subtle patches of lighter and darker green",
        "plains" => "dry golden plains ground, yellow-ochre short grass with small patches of bare brown earth",
        "desert" => "pale sandy desert ground, fine sand with faint wind ripples",
        "tundra" => "cold tundra ground, pale grey-green moss and lichen with small patches of frost",
        "coast" => "shallow clear turquoise coastal water over a sandy bottom, gentle ripples",
        "sea" => "medium-depth teal sea water surface with small even waves",
        "ocean" => "deep blue-green ocean water surface with small even waves",
        "shore" => "light wet beach sand with fine grain",
        _ => "",
    }
}

/// Civ3-like flat colour per terrain, for placeholder materials.
pub fn placeholder_color(name: &str) -> [f32; 3] {
    match name {
        "grassland" => [159.0, 159.0, 58.0],
        "plains" => [217.0, 184.0, 87.0],
        "desert" => [239.0, 216.0, 142.0],
        "tundra" => [219.0, 225.0, 190.0],
        "coast" => [142.0, 218.0, 177.0],
        "sea" => [66.0, 191.0, 173.0],
        "ocean" => [63.0, 160.0, 143.0],
        _ => [232.0, 216.0, 160.0],
    }
}

pub fn all_terrain_names() -> Vec<&'static str> {
    Kind::ALL.iter().map(|k| k.name()).chain([SHORE]).collect()
}

impl Pack {
    pub fn new(name: &str, style: &str) -> Self {
        let terrains = all_terrain_names()
            .into_iter()
            .map(|n| (n.to_string(), Terrain { prompt: default_prompt(n).into(), texture: None, tiles_across: None, color_match: None, seed: None }))
            .collect();
        Self {
            name: name.into(),
            style: style.into(),
            model: d_model(),
            image_size: d_size(),
            masks: MaskSource::Civ3,
            blend: Blend::Height,
            seed: 1,
            tiles_across: d_tiles(),
            shore_width: d_shore(),
            shoreline_darken: d_darken(),
            color_match: false,
            flatten: true,
            terrains,
        }
    }

    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join("pack.json");
        let text = std::fs::read_to_string(&path).with_context(|| format!("reading {} (run `terrain-builder init` first)", path.display()))?;
        let pack: Pack = serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        for k in Kind::ALL {
            if !pack.terrains.contains_key(k.name()) {
                bail!("{}: terrains.{} is missing", path.display(), k.name());
            }
        }
        for name in pack.terrains.keys() {
            if Kind::from_name(name).is_none() && name != SHORE {
                bail!("{}: unknown terrain {name:?}; expected {:?}", path.display(), all_terrain_names());
            }
        }
        Ok(pack)
    }

    pub fn save(&self, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join("pack.json"), serde_json::to_string_pretty(self)? + "\n")?;
        Ok(())
    }

    pub fn texture_path(&self, dir: &Path, name: &str) -> PathBuf {
        let t = &self.terrains[name];
        dir.join(t.texture.clone().unwrap_or_else(|| format!("{name}.png")))
    }

    pub fn full_prompt(&self, name: &str) -> String {
        let base = &self.terrains[name].prompt;
        let style = if self.style.is_empty() { String::new() } else { format!(" {}.", self.style.trim_end_matches('.')) };
        format!(
            "{base}.{style} Seamless tileable ground texture seen straight from above, orthographic top-down view, \
             evenly lit with soft light from the upper left, uniform detail density across the whole image, \
             no objects, no trees, no rocks, no buildings, no horizon, no perspective, no text, no border."
        )
    }
}
