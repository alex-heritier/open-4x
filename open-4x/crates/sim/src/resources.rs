//! Natural resources: wheat, fish, gold, oil, and the like, lying on single squares.
//!
//! A map declares the kinds it uses (`map.resource_types`) and paints them on a run-length layer
//! (`map.resources`) whose values index that list, 1-based, with `0` for none. The kind says
//! what the square yields on top of its ground, so the economy needs no table of its own: a
//! scenario can introduce any resource by naming it and its bonus.
use crate::economy::Yield;
use serde::{Deserialize, Serialize};

/// Most kinds one map may declare. A square stores its kind in a byte, `0` meaning none.
pub const MAX_KINDS: usize = 255;
/// Largest bonus, up or down, a resource may add to a square's food, shields, or gold.
pub const MAX_BONUS: i32 = 25;

/// How a resource is meant to be read: a bonus resource sweetens a square, a luxury is traded for
/// its gold, and a strategic one is what units and buildings are made from.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ResourceClass {
    #[default]
    Bonus,
    Luxury,
    Strategic,
}
impl ResourceClass {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Bonus => "Bonus",
            Self::Luxury => "Luxury",
            Self::Strategic => "Strategic",
        }
    }
}

/// One kind of resource a map declares.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResourceKind {
    /// Stable identifier: `a-z`, `0-9`, and interior `-`, e.g. `big-game`. Pack art is named by it.
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub class: ResourceClass,
    /// Added to the square's food, shields, and gold.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub food: i32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub shields: i32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub gold: i32,
}
fn is_zero(value: &i32) -> bool {
    *value == 0
}
impl ResourceKind {
    /// What this resource adds to the square it lies on.
    pub const fn bonus(&self) -> Yield {
        Yield::new(self.food, self.shields, self.gold)
    }
    /// Whether the kind is well formed: a valid identifier, a name, and bonuses in range.
    pub fn check(&self) -> Result<(), String> {
        let id_ok = (1..=48).contains(&self.id.len())
            && !self.id.starts_with('-')
            && !self.id.ends_with('-')
            && self
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        if !id_ok {
            return Err(format!(
                "resource id {:?} must be 1-48 characters of a-z, 0-9, and '-'",
                self.id
            ));
        }
        if self.name.trim().is_empty() || self.name.chars().count() > 48 {
            return Err(format!(
                "resource {:?} needs a name of 1-48 characters",
                self.id
            ));
        }
        for bonus in [self.food, self.shields, self.gold] {
            if !(-MAX_BONUS..=MAX_BONUS).contains(&bonus) {
                return Err(format!(
                    "resource {:?} bonuses must lie within +/-{MAX_BONUS}",
                    self.id
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wheat() -> ResourceKind {
        ResourceKind {
            id: "wheat".into(),
            name: "Wheat".into(),
            class: ResourceClass::Bonus,
            food: 2,
            shields: 0,
            gold: 1,
        }
    }
    #[test]
    fn a_kind_reports_its_bonus_and_rejects_bad_ids_and_extremes() {
        assert_eq!(wheat().bonus(), Yield::new(2, 0, 1));
        assert!(wheat().check().is_ok());
        for id in ["", "-x", "x-", "Wheat", "big game"] {
            assert!(
                ResourceKind {
                    id: id.into(),
                    ..wheat()
                }
                .check()
                .is_err(),
                "{id}"
            );
        }
        assert!(
            ResourceKind {
                food: 26,
                ..wheat()
            }
            .check()
            .is_err()
        );
        assert!(
            ResourceKind {
                gold: -26,
                ..wheat()
            }
            .check()
            .is_err()
        );
    }
    #[test]
    fn kinds_serialize_without_their_zero_bonuses() {
        let json = serde_json::to_string(&wheat()).unwrap();
        assert_eq!(
            json,
            r#"{"id":"wheat","name":"Wheat","class":"bonus","food":2,"gold":1}"#
        );
        assert_eq!(
            serde_json::from_str::<ResourceKind>(&json).unwrap(),
            wheat()
        );
    }
}
