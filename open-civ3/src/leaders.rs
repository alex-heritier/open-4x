//! The leaders of the civilizations and their animated portraits.
//!
//! Each civilization has one 200 x 240 clip per era (`RACE.era_art`, the
//! forward clips; `tools/prep_assets.py` lays the frames out in a grid
//! sheet under the clip's cache key). The game plays a clip and then its reverse twin, which is a
//! ping-pong over the one clip, and that is all this does: the frame shown is
//! a function of the clock, so a screen that is rebuilt (the diplomacy
//! panels are, on every click) carries the animation on without a restart.
use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageLoaderSettings;
use bevy::prelude::*;
use serde::Deserialize;

use crate::civs::{civ_count, is_ai};
use crate::research::{Research, slot};

use crate::assets::{CACHE, CACHE_URL};

pub use crate::ruleset::Leader;

/// The rulers of this match, by game slot (like `CIVS`).
pub struct LeaderTable;
pub const LEADERS: LeaderTable = LeaderTable;

impl LeaderTable {
    #[allow(dead_code)] // used by the tests
    pub fn iter(&self) -> impl Iterator<Item = &'static Leader> {
        (0..civ_count()).map(|i| &crate::ruleset::LEADER_ROSTER[crate::civs::players()[i]])
    }
}

impl std::ops::Index<usize> for LeaderTable {
    type Output = Leader;
    fn index(&self, i: usize) -> &Leader {
        &crate::ruleset::LEADER_ROSTER[crate::civs::roster_index(i)]
    }
}

/// One clip of the manifest.
#[derive(Clone, Debug, Deserialize)]
pub struct Clip {
    pub file: String,
    /// The cache folder of the clip (its key); set on load.
    #[serde(skip)]
    pub dir: String,
    /// Width and height of one frame.
    pub frame: [u32; 2],
    /// Frames per row of the sheet.
    pub cols: u32,
    pub frames: u32,
    /// Milliseconds a frame stays up.
    pub ms: u32,
}

impl Clip {
    /// Where frame `i` sits on the sheet.
    pub fn cell(&self, i: u32) -> Rect {
        let (w, h) = (self.frame[0] as f32, self.frame[1] as f32);
        let (col, row) = ((i % self.cols) as f32, (i / self.cols) as f32);
        Rect::new(col * w, row * h, (col + 1.0) * w, (row + 1.0) * h)
    }

    /// The frame at `t_ms` on the clock: forward, then back, then forward...
    pub fn frame_at(&self, t_ms: u128) -> u32 {
        pingpong(t_ms / u128::from(self.ms.max(1)), self.frames)
    }
}

/// Position `tick` of a play over `n` frames that runs forward and then
/// backward without repeating the end frames.
pub fn pingpong(tick: u128, n: u32) -> u32 {
    if n < 2 {
        return 0;
    }
    let period = u128::from(2 * (n - 1));
    let t = (tick % period) as u32;
    if t < n { t } else { 2 * (n - 1) - t }
}

/// The clips and the sheets that are loaded.
#[derive(Resource, Default)]
pub struct LeaderArt {
    /// By `RACE` roster index, then era.
    clips: HashMap<usize, Vec<Clip>>,
    held: HashMap<(usize, usize), Handle<Image>>,
}

impl LeaderArt {
    /// The clips the cache holds, for every civ of the roster.
    pub fn load() -> LeaderArt {
        #[cfg(target_arch = "wasm32")]
        crate::web::show_progress("leader art");
        let plan = &crate::ruleset::get().art;
        let mut clips = HashMap::new();
        #[cfg(target_arch = "wasm32")]
        let active = crate::civs::players();
        for (race, eras) in plan.leaders.iter().enumerate() {
            #[cfg(target_arch = "wasm32")]
            if !active.contains(&race) {
                continue;
            }
            let list: Vec<Clip> = eras
                .iter()
                .flatten()
                .filter_map(|item| {
                    let text =
                        crate::web::read_text(format!("{CACHE}/{}/clip.json", item.key)).ok()?;
                    let mut clip: Clip = serde_json::from_str(&text).ok()?;
                    clip.dir = item.key.clone();
                    Some(clip)
                })
                .collect();
            if !list.is_empty() {
                clips.insert(race, list);
            }
        }
        #[cfg(target_arch = "wasm32")]
        crate::web::show_progress("leader art ready");
        LeaderArt {
            clips,
            held: HashMap::new(),
        }
    }

    /// The clip of `civ` in `era` (the last one when the civ has fewer).
    pub fn clip(&self, civ: usize, era: usize) -> Option<&Clip> {
        let clips = self.clips.get(&crate::civs::roster_index(civ))?;
        clips.get(era.min(clips.len().saturating_sub(1)))
    }

    /// The sheet of `civ` in `era`, loaded and kept loaded. The pixels live
    /// on the GPU only: a sheet is 23 MB.
    pub fn sheet(&mut self, assets: &AssetServer, civ: usize, era: usize) -> Option<Handle<Image>> {
        let clip = self.clip(civ, era)?;
        let path = format!("{CACHE_URL}/{}/{}", clip.dir, clip.file);
        // One era of a civ at a time: an older sheet is let go.
        let era = self
            .clips
            .get(&crate::civs::roster_index(civ))
            .map_or(0, |c| era.min(c.len() - 1));
        self.held.retain(|&(c, e), _| c != civ || e == era);
        let handle = self.held.entry((civ, era)).or_insert_with(|| {
            assets.load_with_settings(path, |s: &mut ImageLoaderSettings| {
                s.asset_usage = RenderAssetUsages::RENDER_WORLD;
            })
        });
        Some(handle.clone())
    }
}

/// The era of a civilization's research (0 ancient .. 3 modern).
pub fn era_of(research: &Research, civ: usize) -> usize {
    research.world.players[slot(civ) as usize].era.clamp(0, 3) as usize
}

/// A leader's portrait on the stage: the sheet, cut to the frame on show.
#[derive(Component, Clone, Copy, Debug)]
pub struct LeaderHead {
    pub civ: usize,
    pub era: usize,
}

/// A portrait node for `civ` in its era, showing the frame of the clock.
/// `None` without the art.
pub fn head(
    art: &mut LeaderArt,
    assets: &AssetServer,
    research: &Research,
    civ: usize,
    now_ms: u128,
) -> Option<(LeaderHead, ImageNode)> {
    let era = era_of(research, civ);
    let sheet = art.sheet(assets, civ, era)?;
    let clip = art.clip(civ, era)?;
    let mut image = ImageNode::new(sheet);
    image.rect = Some(clip.cell(clip.frame_at(now_ms)));
    Some((LeaderHead { civ, era }, image))
}

/// Advance every portrait on screen to the frame of the clock.
pub fn animate(
    time: Res<Time<Real>>,
    art: Res<LeaderArt>,
    mut heads: Query<(&LeaderHead, &mut ImageNode)>,
) {
    let now = time.elapsed().as_millis();
    for (head, mut image) in &mut heads {
        let Some(clip) = art.clip(head.civ, head.era) else {
            continue;
        };
        let cell = clip.cell(clip.frame_at(now));
        if image.rect != Some(cell) {
            image.rect = Some(cell);
        }
    }
}

/// Start loading the sheets the human will meet first: each rival in its
/// current era (a sheet takes a moment to decode).
pub fn preload(assets: Res<AssetServer>, research: Res<Research>, mut art: ResMut<LeaderArt>) {
    for civ in (0..civ_count()).filter(|&c| is_ai(c)) {
        art.sheet(&assets, civ, era_of(&research, civ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_play_runs_forward_and_back_without_repeating_the_ends() {
        let n = 5;
        let ticks: Vec<u32> = (0..10).map(|t| pingpong(t, n)).collect();
        assert_eq!(ticks, [0, 1, 2, 3, 4, 3, 2, 1, 0, 1]);
        assert_eq!(pingpong(0, 1), 0);
        assert_eq!(pingpong(7, 0), 0);
        // The 121 frames of a clip: 240 ticks to come back to the start.
        assert_eq!(
            (pingpong(0, 121), pingpong(120, 121), pingpong(240, 121)),
            (0, 120, 0)
        );
    }

    fn clip() -> Clip {
        Clip {
            file: "x.png".into(),
            dir: String::new(),
            frame: [200, 240],
            cols: 11,
            frames: 121,
            ms: 71,
        }
    }

    #[test]
    fn frames_are_cut_from_the_grid_and_follow_the_clock() {
        let c = clip();
        assert_eq!(c.cell(0), Rect::new(0.0, 0.0, 200.0, 240.0));
        assert_eq!(c.cell(11), Rect::new(0.0, 240.0, 200.0, 480.0));
        assert_eq!(
            c.cell(120),
            Rect::new(2000.0, 2400.0, 2200.0, 2640.0),
            "frame 120 is the last of row 10"
        );
        assert_eq!(c.frame_at(0), 0);
        assert_eq!(c.frame_at(70), 0, "a frame stays up for 71 ms");
        assert_eq!(c.frame_at(71), 1);
        assert_eq!(c.frame_at(71 * 120), 120);
        assert_eq!(c.frame_at(71 * 121), 119);
    }

    #[test]
    fn each_leader_speaks_a_text_set_of_their_own() {
        let sets: Vec<usize> = LEADERS.iter().map(|l| l.text_set).collect();
        assert_eq!(
            sets,
            [8, 0, 1, 6],
            "Japan row 9, Rome 1, Egypt 2, China 7, each minus one"
        );
        assert!(
            LEADERS
                .iter()
                .all(|l| l.text_set < crate::speech::TEXT_SETS)
        );
    }

    #[test]
    fn the_converted_sheets_match_their_clips() {
        // Skipped without the converted art (the game converts it at startup).
        let art = LeaderArt::load();
        if art.clips.is_empty() {
            eprintln!("skipped: no leader clips in {CACHE}");
            return;
        }
        for clips in art.clips.values() {
            for clip in clips {
                let path = format!("{CACHE}/{}/{}", clip.dir, clip.file);
                let (w, h) = image_size(&path);
                assert_eq!(w, clip.frame[0] * clip.cols, "{path}");
                assert!(
                    h >= clip.frame[1] * clip.frames.div_ceil(clip.cols),
                    "{path}"
                );
                assert!(
                    (100..=130).contains(&clip.frames),
                    "{path}: {} frames",
                    clip.frames
                );
            }
        }
    }

    /// Width and height from the PNG header.
    fn image_size(path: &str) -> (u32, u32) {
        let head = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let at = |i: usize| u32::from_be_bytes(head[i..i + 4].try_into().unwrap());
        assert_eq!(&head[1..4], b"PNG", "{path}");
        (at(16), at(20))
    }
}
