//! How a pack dresses combat: the sounds the stage plays and how each design fights.
//!
//! See `docs/combat-animation.md` §7. Both parts are optional. A design without a listed
//! style fights the way its stats suggest, and a cue without a sound plays silently.
use crate::{ContentError, safe_path};
use fourx_sim::{Rules, UnitDef};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Every sound cue the stage can ask for.
pub const CUES: [&str; 8] = [
    "volley",
    "charge",
    "gun",
    "broadside",
    "hit",
    "fall",
    "sink",
    "yield",
];

/// The motion and effects of a design's attack.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Style {
    /// Rifles: a recoil, a flash, and a small shell.
    Volley,
    /// Closes with the enemy: a wind-up, a thrust, and a dust cloud.
    Charge,
    /// A heavy gun: a long recoil, a big flash, and a shell.
    Gun,
    /// Ships: a roll and several staggered shots.
    Broadside,
}

impl Style {
    /// What a design does when its pack does not say: ships fire broadsides, guns fire guns,
    /// everything else that attacks fires volleys.
    pub fn default_for(def: &UnitDef) -> Self {
        if def.is_naval() {
            Style::Broadside
        } else if def.can_bombard() {
            Style::Gun
        } else {
            Style::Volley
        }
    }

    /// The sound cue this style's attack plays.
    pub fn cue(self) -> &'static str {
        match self {
            Style::Volley => "volley",
            Style::Charge => "charge",
            Style::Gun => "gun",
            Style::Broadside => "broadside",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatVisuals {
    /// Pack-relative sound paths by cue name (see [`CUES`]).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sounds: BTreeMap<String, String>,
    /// Attack style by unit design id, for designs that should not use their default.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub styles: BTreeMap<String, Style>,
}

impl CombatVisuals {
    /// How a design attacks on screen.
    pub fn style_of(&self, def: &UnitDef) -> Style {
        self.styles
            .get(&def.id)
            .copied()
            .unwrap_or_else(|| Style::default_for(def))
    }

    /// Every sound file the pack names.
    pub fn paths(&self) -> impl Iterator<Item = &String> {
        self.sounds.values()
    }

    pub fn validate(&self, rules: &Rules) -> Result<(), ContentError> {
        let invalid = |message: String| ContentError::Invalid(message);
        for (cue, path) in &self.sounds {
            if !CUES.contains(&cue.as_str()) {
                return Err(invalid(format!(
                    "visuals.combat.sounds has no cue {cue:?}; the cues are {}",
                    CUES.join(", ")
                )));
            }
            if !safe_path(path) {
                return Err(invalid(format!(
                    "visuals.combat.sounds.{cue} must stay inside the pack"
                )));
            }
        }
        for id in self.styles.keys() {
            if !rules.units.contains_key(id) {
                return Err(invalid(format!(
                    "visuals.combat.styles names unknown unit design {id:?}"
                )));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Pack;

    fn parse(json: &str) -> Result<CombatVisuals, serde_json::Error> {
        serde_json::from_str(json)
    }

    #[test]
    fn designs_fight_the_way_their_stats_suggest() {
        let pack = Pack::base();
        let combat = CombatVisuals::default();
        let style = |id: &str| combat.style_of(&pack.rules.units[id]);
        assert_eq!(style("infantry"), Style::Volley);
        assert_eq!(style("cavalry"), Style::Volley);
        assert_eq!(style("artillery"), Style::Gun);
        assert_eq!(style("ironclad"), Style::Broadside);
        assert_eq!(style("torpedo-boat"), Style::Broadside);
    }

    #[test]
    fn a_pack_may_override_a_style() {
        let pack = Pack::base();
        let combat = parse(r#"{"styles": {"cavalry": "charge"}}"#).unwrap();
        assert_eq!(combat.style_of(&pack.rules.units["cavalry"]), Style::Charge);
        assert_eq!(
            combat.style_of(&pack.rules.units["infantry"]),
            Style::Volley
        );
        assert_eq!(Style::Charge.cue(), "charge");
    }

    #[test]
    fn every_style_has_a_declared_cue() {
        for style in [Style::Volley, Style::Charge, Style::Gun, Style::Broadside] {
            assert!(CUES.contains(&style.cue()), "{style:?}");
        }
    }

    #[test]
    fn unknown_styles_and_fields_are_rejected() {
        assert!(parse(r#"{"styles": {"cavalry": "teleport"}}"#).is_err());
        assert!(parse(r#"{"musik": {}}"#).is_err());
        assert_eq!(parse("{}").unwrap(), CombatVisuals::default());
    }

    #[test]
    fn sounds_and_styles_are_checked_against_the_pack() {
        let rules = Pack::base().rules;
        let good = parse(
            r#"{"sounds": {"hit": "audio/hit.wav", "sink": "audio/sink.wav"},
                "styles": {"cavalry": "charge"}}"#,
        )
        .unwrap();
        good.validate(&rules).unwrap();
        for bad in [
            r#"{"sounds": {"thunder": "audio/hit.wav"}}"#,
            r#"{"sounds": {"hit": "../hit.wav"}}"#,
            r#"{"sounds": {"hit": "/etc/passwd"}}"#,
            r#"{"styles": {"dragon": "volley"}}"#,
        ] {
            let combat = parse(bad).unwrap();
            assert!(combat.validate(&rules).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_pack_rejects_bad_combat_visuals() {
        let mut pack = Pack::base();
        pack.validate().unwrap();
        pack.visuals
            .combat
            .sounds
            .insert("thunder".into(), "audio/hit.wav".into());
        assert!(pack.validate().is_err());
    }

    #[test]
    fn the_base_pack_dresses_every_cue() {
        let pack = Pack::base();
        for cue in CUES {
            assert!(pack.visuals.combat.sounds.contains_key(cue), "{cue}");
        }
        assert_eq!(
            pack.visuals.combat.style_of(&pack.rules.units["cavalry"]),
            Style::Charge
        );
    }
}
