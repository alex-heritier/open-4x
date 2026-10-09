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
}

impl UnitAnimation {
    pub fn clip(&self, clip: Clip) -> &Sheet {
        &self.clips[&clip]
    }
}

/// Every sheet the animations name.
pub fn paths(units: &BTreeMap<String, UnitAnimation>) -> impl Iterator<Item = &String> {
    units
        .values()
        .flat_map(|unit| unit.clips.values().map(|clip| &clip.sheet))
}

pub fn validate(
    units: &BTreeMap<String, UnitAnimation>,
    rules: &Rules,
) -> Result<(), ContentError> {
    let invalid = |message: String| Err(ContentError::Invalid(message));
    for (id, unit) in units {
        if !rules.units.contains_key(id) {
            return invalid(format!("visuals.units names unknown unit design {id:?}"));
        }
        let [w, h] = unit.frame;
        if !(16..=1024).contains(&w) || !(16..=1024).contains(&h) || h * FACINGS > MAX_SHEET_SIDE {
            return invalid(format!(
                "visuals.units.{id}.frame must be 16-1024 pixels, and eight rows of it at most {MAX_SHEET_SIDE}"
            ));
        }
        for clip in Clip::ALL {
            let Some(sheet) = unit.clips.get(&clip) else {
                return invalid(format!(
                    "visuals.units.{id} needs all five clips (idle, run, attack, victory, death)"
                ));
            };
            if !(1..=32).contains(&sheet.frames)
                || w * sheet.frames > MAX_SHEET_SIDE
                || !(20..=2000).contains(&sheet.ms)
                || !safe_path(&sheet.sheet)
            {
                return invalid(format!(
                    "visuals.units.{id}: the {clip:?} sheet needs 1-32 frames no wider than {MAX_SHEET_SIDE} pixels in all, 20-2000 ms a frame, and a path inside the pack"
                ));
            }
        }
    }
    Ok(())
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
            frames,
            ms: 100,
        }
    }

    fn animation() -> UnitAnimation {
        UnitAnimation {
            frame: [160, 160],
            clips: Clip::ALL.into_iter().map(|c| (c, sheet(8))).collect(),
        }
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
    fn animations_are_checked_against_the_pack() {
        let rules = Pack::base().rules;
        let ok = BTreeMap::from([("infantry".to_string(), animation())]);
        validate(&ok, &rules).unwrap();
        let breaks = |change: &dyn Fn(&mut UnitAnimation)| {
            let mut unit = animation();
            change(&mut unit);
            let units = BTreeMap::from([("infantry".to_string(), unit)]);
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
        let unknown = BTreeMap::from([("dragon".to_string(), animation())]);
        assert!(validate(&unknown, &rules).is_err());
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
            let unit = pack
                .visuals
                .units
                .get(id)
                .unwrap_or_else(|| panic!("{id} has no animation"));
            for (clip, sheet) in &unit.clips {
                let bytes = std::fs::read(root.join(&sheet.sheet)).unwrap();
                let (w, h) = png_size(&bytes);
                assert_eq!(
                    (w, h),
                    (unit.frame[0] * sheet.frames, unit.frame[1] * FACINGS),
                    "{id} {clip:?}"
                );
            }
        }
    }
}
