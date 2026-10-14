//! Frame-by-frame unit art: how a design stands, walks, attacks, cheers, and dies.
//!
//! A design may list five clips under `visuals.units.<design>`. Each clip is one sheet: a row
//! for each of the eight facings, in the order Civ3 stores its unit strips (south-west, south,
//! south-east, east, north-east, north, north-west, west), and a column for each frame. A design
//! without clips is drawn from its single `sprite`. See `docs/combat-animation.md` §7.
use crate::{ContentError, safe_path};
use fourx_sim::Rules;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Rows in every sheet, one per facing.
pub const FACINGS: u32 = 8;
/// The largest texture side every platform can load (WebGL2's guaranteed minimum).
pub const MAX_SHEET_SIDE: u32 = 2048;

/// What a unit is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Clip {
    /// Standing about. Loops.
    Idle,
    /// Walking or sailing. Loops; the client moves the sprite between squares.
    Run,
    /// A swing at the foe. Played once over the attack motion of its combat style.
    Attack,
    /// A cheer after a win. Played once.
    Victory,
    /// Falling or sinking. Played once; the last frame holds.
    Death,
}

impl Clip {
    pub const ALL: [Clip; 5] = [
        Clip::Idle,
        Clip::Run,
        Clip::Attack,
        Clip::Victory,
        Clip::Death,
    ];

    /// Whether the clip repeats while it lasts. The others hold their last frame.
    pub fn loops(self) -> bool {
        matches!(self, Clip::Idle | Clip::Run)
    }
}

/// One clip's sheet.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sheet {
    /// Pack-relative PNG: `frames` columns by eight facing rows of `frame`-sized cells.
    pub sheet: String,
    /// Optional grayscale nation-color layer, with exactly the same frame layout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent: Option<String>,
    pub frames: u32,
    /// Milliseconds per frame at normal speed.
    pub ms: u32,
}

/// Every clip of one design.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitAnimation {
    /// Width and height of one cell in pixels. The cell is drawn at the design's usual size
    /// and anchored like its sprite.
    pub frame: [u32; 2],
    pub clips: BTreeMap<Clip, Sheet>,
    /// Where a ship's guns fire a broadside from, for each facing (empty for everything else).
    /// The ship is drawn with its starboard side turned to the foe, so each row lists the muzzles
    /// on the starboard beam at the moment of firing, in cell pixels from the top left, with a
    /// weight for how big a blast the gun throws. The client puts the flashes, fireballs and
    /// smoke there; the sheet carries only the hull, the trained turrets and the haze.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub muzzles: Vec<Vec<Muzzle>>,
}

/// One gun's muzzle: `[x, y, weight]`.
pub type Muzzle = [f32; 3];

impl UnitAnimation {
    pub fn clip(&self, clip: Clip) -> &Sheet {
        &self.clips[&clip]
    }
}

/// One design's art: a single sprite and, optional, its five frame-by-frame clips. A culture may
/// name a design of its own for any flavor whose people should not march in another's kit, so
/// `flavor_sprites` and `flavor_animations` override `sprite` and `clips` for that flavor only.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitVisual {
    /// Pack-relative PNG drawn when the design has no clips (or none for this flavor).
    pub sprite: String,
    /// Frame-by-frame clips, when the design is animated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clips: Option<UnitAnimation>,
    /// Per-flavor sprite overrides, keyed by [`fourx_sim::Flavor::key`]: the design each culture
    /// fields. A flavor left out falls back to `sprite`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub flavor_sprites: BTreeMap<String, String>,
    /// Per-flavor animation overrides, keyed the same way, for a culture's own clips.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub flavor_animations: BTreeMap<String, UnitAnimation>,
}

impl UnitVisual {
    /// A design with neither clips nor flavor overrides: one sprite, as a pack may still hold.
    pub fn sprite(sprite: impl Into<String>) -> Self {
        Self {
            sprite: sprite.into(),
            clips: None,
            flavor_sprites: BTreeMap::new(),
            flavor_animations: BTreeMap::new(),
        }
    }
}

/// Every sheet the animations name.
pub fn paths(units: &BTreeMap<String, UnitAnimation>) -> impl Iterator<Item = &String> {
    units.values().flat_map(paths_one)
}

/// Every sprite, flavor sprite and animation sheet the unit visuals name.
pub fn visual_paths(units: &BTreeMap<String, UnitVisual>) -> impl Iterator<Item = &String> {
    units.values().flat_map(|visual| {
        std::iter::once(&visual.sprite)
            .chain(visual.flavor_sprites.values())
            .chain(visual.clips.as_ref().into_iter().flat_map(paths_one))
            .chain(visual.flavor_animations.values().flat_map(paths_one))
    })
}

fn paths_one(unit: &UnitAnimation) -> impl Iterator<Item = &String> {
    unit.clips
        .values()
        .flat_map(|clip| std::iter::once(&clip.sheet).chain(clip.accent.iter()))
}

pub fn validate(units: &BTreeMap<String, UnitVisual>, rules: &Rules) -> Result<(), ContentError> {
    let invalid = |message: String| Err(ContentError::Invalid(message));
    for (id, visual) in units {
        if !rules.units.contains_key(id) {
            return invalid(format!("visuals.units names unknown unit design {id:?}"));
        }
        if !safe_path(&visual.sprite) {
            return invalid(format!(
                "visuals.units.{id}.sprite must stay inside the pack"
            ));
        }
        for (flavor, sprite) in &visual.flavor_sprites {
            if !safe_path(sprite) {
                return invalid(format!(
                    "visuals.units.{id}.flavor_sprites.{flavor} must stay inside the pack"
                ));
            }
        }
        // A design is drawn from clips when it has them; a design with neither clips nor any
        // flavor's clips is drawn from its single sprite.
        let animated = visual.clips.is_some() || !visual.flavor_animations.is_empty();
        if animated && visual.clips.is_none() {
            return invalid(format!(
                "visuals.units.{id} has per-flavor clips but no clips of its own to fall back on"
            ));
        }
        if let Some(clips) = &visual.clips {
            check_clips(id, clips)?;
        }
        for (flavor, clips) in &visual.flavor_animations {
            check_clips(&format!("{id}.flavor_animations.{flavor}"), clips)?;
        }
    }
    Ok(())
}

/// One design's clips: every clip present, each a sheet that fits the grid and names a safe path.
fn check_clips(id: &str, unit: &UnitAnimation) -> Result<(), ContentError> {
    let [w, h] = unit.frame;
    if !(16..=1024).contains(&w) || !(16..=1024).contains(&h) || h * FACINGS > MAX_SHEET_SIDE {
        return Err(ContentError::Invalid(format!(
            "visuals.units.{id}.frame must be 16-1024 pixels, and eight rows of it at most {MAX_SHEET_SIDE}"
        )));
    }
    for clip in Clip::ALL {
        let Some(sheet) = unit.clips.get(&clip) else {
            return Err(ContentError::Invalid(format!(
                "visuals.units.{id} needs all five clips (idle, run, attack, victory, death)"
            )));
        };
        if !(1..=32).contains(&sheet.frames)
            || w * sheet.frames > MAX_SHEET_SIDE
            || !(20..=2000).contains(&sheet.ms)
            || !safe_path(&sheet.sheet)
            || sheet.accent.as_ref().is_some_and(|path| !safe_path(path))
        {
            return Err(ContentError::Invalid(format!(
                "visuals.units.{id}: the {clip:?} sheet needs 1-32 frames no wider than {MAX_SHEET_SIDE} pixels in all, 20-2000 ms a frame, and a path inside the pack"
            )));
        }
    }
    validate_muzzles(id, unit)
}

fn validate_muzzles(id: &str, unit: &UnitAnimation) -> Result<(), ContentError> {
    if unit.muzzles.is_empty() {
        return Ok(());
    }
    let [w, h] = unit.frame;
    let ok = unit.muzzles.len() == FACINGS as usize
        && unit.muzzles.iter().all(|row| {
            !row.is_empty()
                && row.len() <= 32
                && row.iter().all(|&[x, y, weight]| {
                    (0.0..w as f32).contains(&x)
                        && (0.0..h as f32).contains(&y)
                        && (0.1..=4.0).contains(&weight)
                })
        });
    if ok {
        Ok(())
    } else {
        Err(ContentError::Invalid(format!(
            "visuals.units.{id}.muzzles needs one row per facing ({FACINGS}), each listing 1-32 [x, y, weight] guns inside the frame with a weight of 0.1-4"
        )))
    }
}

/// The facing (sheet row) that looks along a screen direction, `+y` up. The map is drawn at
/// half height, so the direction is stretched back to the ground before it is rounded to the
/// nearest eighth of a turn; each of a square's eight neighbours is then exactly one facing.
pub fn facing(dx: f32, dy: f32) -> u32 {
    if dx == 0.0 && dy == 0.0 {
        return 0;
    }
    // Ground angle, counter-clockwise from east; facing 0 looks south-west (225 degrees).
    let degrees = (2.0 * dy).atan2(dx).to_degrees();
    ((degrees - 225.0) / 45.0).round().rem_euclid(8.0) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Pack;
    use fourx_sim::terrain::Coord;

    fn sheet(frames: u32) -> Sheet {
        Sheet {
            sheet: "units/x.png".into(),
            accent: None,
            frames,
            ms: 100,
        }
    }

    fn animation() -> UnitAnimation {
        UnitAnimation {
            frame: [160, 160],
            clips: Clip::ALL.into_iter().map(|c| (c, sheet(8))).collect(),
            muzzles: Vec::new(),
        }
    }

    fn visual() -> UnitVisual {
        UnitVisual {
            sprite: "units/x.png".into(),
            clips: Some(animation()),
            flavor_sprites: BTreeMap::new(),
            flavor_animations: BTreeMap::new(),
        }
    }

    #[test]
    fn accent_sheets_are_optional_pack_assets_and_paths_are_validated() {
        let mut unit = animation();
        assert!(unit.clips.values().all(|sheet| sheet.accent.is_none()));
        unit.clips.get_mut(&Clip::Idle).unwrap().accent = Some("units/idle_accent.png".into());
        assert!(paths_one(&unit).any(|path| path == "units/idle_accent.png"));
        assert!(check_clips("infantry", &unit).is_ok());
        unit.clips.get_mut(&Clip::Idle).unwrap().accent = Some("../outside.png".into());
        assert!(check_clips("infantry", &unit).is_err());
    }

    #[test]
    fn each_neighbour_is_its_own_facing_in_the_civ3_strip_order() {
        let step = |dx: i32, dy: i32| {
            let (x0, y0) = Coord::new(5, 5).screen();
            let (x1, y1) = Coord::new(5 + dx, 5 + dy).screen();
            facing(x1 - x0, y1 - y0)
        };
        // SW, S, SE, E, NE, N, NW, W on screen.
        let order = [
            (0, 1),
            (1, 1),
            (1, 0),
            (1, -1),
            (0, -1),
            (-1, -1),
            (-1, 0),
            (-1, 1),
        ];
        for (k, (dx, dy)) in order.into_iter().enumerate() {
            assert_eq!(step(dx, dy), k as u32, "step ({dx}, {dy})");
        }
        assert_eq!(facing(0.0, 0.0), 0, "no direction faces the default way");
        assert_eq!(facing(1.0, 0.0), 3);
        assert_eq!(facing(0.0, -1.0), 1);
    }

    #[test]
    fn clips_parse_and_only_the_standing_and_walking_ones_loop() {
        let unit: UnitAnimation = serde_json::from_str(
            r#"{"frame": [160, 160], "clips": {
                "idle": {"sheet": "a.png", "frames": 8, "ms": 150},
                "run": {"sheet": "b.png", "frames": 8, "ms": 50},
                "attack": {"sheet": "c.png", "frames": 10, "ms": 80},
                "victory": {"sheet": "d.png", "frames": 8, "ms": 90},
                "death": {"sheet": "e.png", "frames": 10, "ms": 90}}}"#,
        )
        .unwrap();
        assert_eq!(unit.clip(Clip::Attack).frames, 10);
        assert!(Clip::Idle.loops() && Clip::Run.loops());
        assert!(!Clip::Attack.loops() && !Clip::Victory.loops() && !Clip::Death.loops());
        assert!(serde_json::from_str::<Clip>(r#""fortify""#).is_err());
    }

    #[test]
    fn a_culture_fields_its_own_kit_and_everyone_else_keeps_the_base_design() {
        use fourx_sim::Flavor;
        let pack = Pack::base();
        let african = pack.visuals.unit("infantry", Flavor::African).unwrap();
        assert_eq!(african.sprite, "sprites/infantry_african.png");
        let western = pack.visuals.unit("infantry", Flavor::Western).unwrap();
        assert_eq!(western.sprite, "sprites/infantry.png");
        assert_eq!(
            pack.visuals.unit_design("infantry", Flavor::Western),
            "infantry"
        );
        // A design with no flavor art is the same for every culture.
        assert_eq!(
            pack.visuals.unit("ironclad", Flavor::Arab).unwrap().sprite,
            "sprites/ironclad.png"
        );
    }

    #[test]
    fn animations_are_checked_against_the_pack() {
        let rules = Pack::base().rules;
        let ok = BTreeMap::from([("infantry".to_string(), visual())]);
        validate(&ok, &rules).unwrap();
        let breaks = |change: &dyn Fn(&mut UnitAnimation)| {
            let mut unit = animation();
            change(&mut unit);
            let units = BTreeMap::from([(
                "infantry".to_string(),
                UnitVisual {
                    clips: Some(unit),
                    ..visual()
                },
            )]);
            assert!(validate(&units, &rules).is_err());
        };
        breaks(&|u| {
            u.clips.remove(&Clip::Death);
        });
        breaks(&|u| u.clips.get_mut(&Clip::Run).unwrap().frames = 0);
        breaks(&|u| u.clips.get_mut(&Clip::Run).unwrap().frames = 13);
        breaks(&|u| u.clips.get_mut(&Clip::Idle).unwrap().ms = 5);
        breaks(&|u| u.clips.get_mut(&Clip::Idle).unwrap().sheet = "../x.png".into());
        breaks(&|u| u.frame = [160, 300]);
        breaks(&|u| u.muzzles = vec![vec![[10.0, 10.0, 1.0]]; 7]);
        breaks(&|u| u.muzzles = vec![vec![[10.0, 200.0, 1.0]]; 8]);
        breaks(&|u| u.muzzles = vec![Vec::new(); 8]);
        let mut armed = animation();
        armed.muzzles = vec![vec![[10.0, 10.0, 1.0], [20.0, 12.0, 0.5]]; 8];
        validate(
            &BTreeMap::from([(
                "infantry".to_string(),
                UnitVisual {
                    clips: Some(armed),
                    ..visual()
                },
            )]),
            &rules,
        )
        .unwrap();
        let unknown = BTreeMap::from([("dragon".to_string(), visual())]);
        assert!(validate(&unknown, &rules).is_err());
    }

    #[test]
    fn a_flavor_override_stays_in_the_pack() {
        let rules = Pack::base().rules;
        let with = |edit: &dyn Fn(&mut UnitVisual)| {
            let mut unit = visual();
            edit(&mut unit);
            BTreeMap::from([("infantry".to_string(), unit)])
        };
        // A real culture, with its own sprite and its own clips, is fine.
        let good = with(&|u| {
            u.flavor_sprites
                .insert("african".into(), "sprites/infantry_african.png".into());
            u.flavor_animations.insert("african".into(), animation());
        });
        validate(&good, &rules).unwrap();
        // A sprite that leaves the pack is not.
        let outside = with(&|u| {
            u.flavor_sprites
                .insert("african".into(), "../infantry.png".into());
        });
        assert!(validate(&outside, &rules).is_err());
        // A culture's clips carry the same checks as the design's own.
        let mut broken = animation();
        broken.clips.get_mut(&Clip::Run).unwrap().sheet = "../x.png".into();
        let bad_clips = with(&|u| {
            u.flavor_animations.insert("african".into(), broken.clone());
        });
        assert!(validate(&bad_clips, &rules).is_err());
    }

    #[test]
    fn a_flavor_may_field_a_design_of_its_own() {
        // A culture with its own art and another without it draw different things.
        let visuals = Pack::base().visuals;
        let african = visuals
            .unit("infantry", fourx_sim::Flavor::African)
            .unwrap();
        let western = visuals
            .unit("infantry", fourx_sim::Flavor::Western)
            .unwrap();
        assert_eq!(african.sprite, "sprites/infantry_african.png");
        assert_eq!(western.sprite, "sprites/infantry.png");
        assert_eq!(
            visuals.unit_design("infantry", fourx_sim::Flavor::African),
            "sprites/infantry_african.png"
        );
        assert_eq!(
            visuals.unit_design("infantry", fourx_sim::Flavor::Western),
            "infantry"
        );
        // A design a culture does not override keeps the base art and the base clips.
        assert_eq!(
            visuals
                .unit("worker", fourx_sim::Flavor::African)
                .unwrap()
                .sprite,
            "sprites/worker.png"
        );
        // A culture's clips are laid out exactly like the design's own, so the art has to
        // match the base frame and sheet shape.
        let base = western.clips.unwrap();
        let own = african.clips.unwrap();
        assert_eq!(own.frame, base.frame);
        assert_eq!(
            own.clips.len(),
            base.clips.len(),
            "a culture lists the same five clips"
        );
    }

    /// A PNG's size, from its header.
    fn png_size(bytes: &[u8]) -> (u32, u32) {
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        let be = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
        (be(16), be(20))
    }

    #[test]
    fn every_base_design_is_animated_and_its_sheets_fit_their_grids() {
        let pack = Pack::base();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/packs/base");
        for id in pack.rules.units.keys() {
            let visual = pack
                .visuals
                .units
                .get(id)
                .unwrap_or_else(|| panic!("{id} has no animation"));
            let unit = visual
                .clips
                .as_ref()
                .unwrap_or_else(|| panic!("{id} is not animated"));
            for unit in std::iter::once(unit).chain(visual.flavor_animations.values()) {
                for (clip, sheet) in &unit.clips {
                    for path in std::iter::once(&sheet.sheet).chain(sheet.accent.iter()) {
                        let bytes = std::fs::read(root.join(path))
                            .unwrap_or_else(|err| panic!("{path}: {err}"));
                        let (w, h) = png_size(&bytes);
                        assert_eq!(
                            (w, h),
                            (unit.frame[0] * sheet.frames, unit.frame[1] * FACINGS),
                            "{id} {clip:?} {path}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn every_base_ship_says_where_its_broadside_fires_from() {
        let pack = Pack::base();
        for (id, def) in &pack.rules.units {
            let unit = pack.visuals.units[id].clips.as_ref().unwrap();
            assert_eq!(
                !unit.muzzles.is_empty(),
                def.is_naval(),
                "{id}: only ships have muzzles, and every ship has them"
            );
        }
    }
}
