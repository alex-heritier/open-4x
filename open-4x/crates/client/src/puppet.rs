//! Frame-by-frame unit art: the clips a pack draws its designs with, the way each unit faces,
//! and the idle loop of the units standing on the map.
//!
//! A clip is a sheet of eight facing rows (see [`fourx_content::animation`]). Sheets are big, so
//! they are fetched the first time something needs them. The standing and walking clips stay
//! loaded once fetched; the fighting ones are held only while a fight that uses them is on
//! stage, and the asset server lets them go after.
use super::Session;
use bevy::{ecs::system::SystemParam, prelude::*};
use fourx_content::{
    Visuals,
    animation::{Clip, FACINGS},
};
use fourx_sim::Id;
use std::collections::HashMap;

/// One clip of one design, ready to draw.
#[derive(Clone)]
pub(super) struct ClipArt {
    pub image: Handle<Image>,
    pub layout: Handle<TextureAtlasLayout>,
    pub frames: u32,
    /// Milliseconds per frame.
    pub ms: f32,
}

impl ClipArt {
    /// The frame `seconds` into a looping clip.
    pub fn looping(&self, seconds: f32) -> u32 {
        ((seconds.max(0.0) * 1000.0 / self.ms) as u32) % self.frames
    }

    /// The frame `progress` (0 to 1) of the way through a clip played once; the last frame
    /// holds at the end.
    pub fn once(&self, progress: f32) -> u32 {
        ((progress.clamp(0.0, 1.0) * self.frames as f32) as u32).min(self.frames - 1)
    }

    /// The cell of the sheet for a facing and a frame.
    pub fn index(&self, facing: u32, frame: u32) -> usize {
        (facing.min(FACINGS - 1) * self.frames + frame.min(self.frames - 1)) as usize
    }

    /// A sprite showing one cell.
    pub fn sprite(&self, facing: u32, frame: u32, size: Vec2, color: Color) -> Sprite {
        Sprite {
            image: self.image.clone(),
            texture_atlas: Some(TextureAtlas {
                layout: self.layout.clone(),
                index: self.index(facing, frame),
            }),
            custom_size: Some(size),
            color,
            ..default()
        }
    }

    /// Points `sprite` at a cell of this clip, touching only what changed.
    pub fn show(&self, sprite: &mut Sprite, facing: u32, frame: u32) {
        if sprite.image != self.image {
            sprite.image = self.image.clone();
        }
        let index = self.index(facing, frame);
        match &mut sprite.texture_atlas {
            Some(atlas) if atlas.layout == self.layout => {
                if atlas.index != index {
                    atlas.index = index;
                }
            }
            atlas => {
                *atlas = Some(TextureAtlas {
                    layout: self.layout.clone(),
                    index,
                })
            }
        }
    }
}

/// Every clip of one design, for a fight.
#[derive(Clone)]
pub(super) struct ClipSet {
    clips: [ClipArt; 5],
}

impl ClipSet {
    pub fn get(&self, clip: Clip) -> &ClipArt {
        &self.clips[slot(clip)]
    }
}

fn slot(clip: Clip) -> usize {
    Clip::ALL.iter().position(|c| *c == clip).unwrap_or(0)
}

/// The pack's unit clips, fetched as they are first wanted.
#[derive(Resource, Default)]
pub(super) struct UnitArt {
    /// The pack and asset location the caches below belong to.
    key: String,
    layouts: HashMap<(String, Clip), Handle<TextureAtlasLayout>>,
    /// Standing and walking sheets, kept once fetched.
    resident: HashMap<(String, Clip), Handle<Image>>,
}

impl UnitArt {
    /// One clip of a design, or `None` when the pack draws the design from a single sprite.
    pub fn clip(
        &mut self,
        prefix: &str,
        pack_id: &str,
        visuals: &Visuals,
        assets: &AssetServer,
        layouts: &mut Assets<TextureAtlasLayout>,
        kind: &str,
        clip: Clip,
    ) -> Option<ClipArt> {
        let key = format!("{prefix}|{pack_id}");
        if self.key != key {
            *self = UnitArt { key, ..default() };
        }
        let unit = visuals.units.get(kind)?;
        let sheet = unit.clips.get(&clip)?;
        let id = (kind.to_string(), clip);
        let layout = self
            .layouts
            .entry(id.clone())
            .or_insert_with(|| {
                layouts.add(TextureAtlasLayout::from_grid(
                    UVec2::new(unit.frame[0], unit.frame[1]),
                    sheet.frames,
                    FACINGS,
                    None,
                    None,
                ))
            })
            .clone();
        let path = format!("{prefix}{}", sheet.sheet);
        let image = if clip.loops() {
            self.resident
                .entry(id)
                .or_insert_with(|| assets.load(path))
                .clone()
        } else {
            // While a fight holds this handle the asset server keeps the sheet; after it, the
            // sheet is let go.
            assets.load(path)
        };
        Some(ClipArt {
            image,
            layout,
            frames: sheet.frames,
            ms: sheet.ms as f32,
        })
    }

    /// Every clip of a design, or `None` when it has none.
    pub fn set(
        &mut self,
        session: &Session,
        assets: &AssetServer,
        layouts: &mut Assets<TextureAtlasLayout>,
        kind: &str,
    ) -> Option<ClipSet> {
        let mut clips = Vec::with_capacity(5);
        for clip in Clip::ALL {
            clips.push(self.clip(
                &session.asset_prefix,
                &session.pack.id,
                &session.pack.visuals,
                assets,
                layouts,
                kind,
                clip,
            )?);
        }
        Some(ClipSet {
            clips: clips.try_into().ok()?,
        })
    }
}

/// Which way each unit looks. A unit nobody has turned looks south-west, toward the viewer, as in
/// Civ3; walking and fighting turn it.
#[derive(Resource, Default)]
pub(super) struct Facings(HashMap<Id, u32>);

impl Facings {
    pub fn of(&self, unit: Id) -> u32 {
        self.0.get(&unit).copied().unwrap_or(0)
    }

    pub fn turn(&mut self, unit: Id, facing: u32) {
        self.0.insert(unit, facing.min(FACINGS - 1));
    }
}

/// What the stage and the walks need to draw units with their clips.
#[derive(SystemParam)]
pub(super) struct Puppetry<'w> {
    pub art: ResMut<'w, UnitArt>,
    pub layouts: ResMut<'w, Assets<TextureAtlasLayout>>,
    pub facings: ResMut<'w, Facings>,
    pub images: Res<'w, Assets<Image>>,
}

/// A unit standing on the map, looping its idle clip.
#[derive(Component)]
pub(super) struct Puppet {
    pub frames: u32,
    pub ms: f32,
    pub facing: u32,
    /// Seconds added to the clock, so that neighbours do not breathe in step.
    pub phase: f32,
}

impl Puppet {
    pub fn new(clip: &ClipArt, facing: u32, unit: Id) -> Self {
        Puppet {
            frames: clip.frames,
            ms: clip.ms,
            facing,
            phase: (unit as f32 * 0.618_034).fract() * 8.0,
        }
    }
}

/// Advances the idle loop of every unit on the map.
pub(super) fn animate(time: Res<Time>, mut puppets: Query<(&Puppet, &mut Sprite)>) {
    let now = time.elapsed_secs();
    for (puppet, mut sprite) in &mut puppets {
        let frame = (((now + puppet.phase) * 1000.0 / puppet.ms) as u32) % puppet.frames;
        let index = (puppet.facing * puppet.frames + frame) as usize;
        if sprite
            .texture_atlas
            .as_ref()
            .is_some_and(|a| a.index != index)
            && let Some(atlas) = &mut sprite.texture_atlas
        {
            atlas.index = index;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(frames: u32, ms: f32) -> ClipArt {
        ClipArt {
            image: Handle::default(),
            layout: Handle::default(),
            frames,
            ms,
        }
    }

    #[test]
    fn loops_wrap_and_single_plays_hold_their_last_frame() {
        let c = clip(8, 100.0);
        assert_eq!(c.looping(0.0), 0);
        assert_eq!(c.looping(0.35), 3);
        assert_eq!(c.looping(0.85), 0, "a whole cycle later it starts again");
        assert_eq!(c.once(0.0), 0);
        assert_eq!(c.once(0.5), 4);
        assert_eq!(c.once(1.0), 7);
        assert_eq!(c.once(7.0), 7);
    }

    #[test]
    fn a_facing_is_a_row_of_the_sheet() {
        let c = clip(10, 80.0);
        assert_eq!(c.index(0, 0), 0);
        assert_eq!(c.index(3, 2), 32);
        assert_eq!(c.index(7, 9), 79);
        assert_eq!(c.index(12, 40), 79, "out of range is clamped to the sheet");
    }
}
