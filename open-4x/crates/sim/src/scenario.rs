//! The scenario document: everything needed to start a campaign.
//!
//! A scenario is separate from a content pack. The pack supplies rules, art, and scripts;
//! the scenario supplies a calendar date, a map, the nations and their starting cities,
//! units, and wars. Nations, regions, and cities refer to each other by stable string IDs,
//! so documents can be edited by hand and reordered without silently re-pointing references.
//! See `docs/scenario-format.md` for the field-by-field specification.
use crate::{Rules, calendar::Date, terrain::Coord, terrain::Map};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub const SCENARIO_FORMAT: u32 = 3;

/// A nation's colour. Serialized as `#rrggbb`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rgb(pub [u8; 3]);
impl Rgb {
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0[0], self.0[1], self.0[2])
    }
    pub fn parse(text: &str) -> Option<Self> {
        let digits = text.strip_prefix('#')?;
        if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let channel = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).ok();
        Some(Self([channel(0)?, channel(2)?, channel(4)?]))
    }
}
impl Serialize for Rgb {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.hex())
    }
}
impl<'de> Deserialize<'de> for Rgb {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).ok_or_else(|| {
            serde::de::Error::custom(format!("invalid colour {text:?}; use #rrggbb"))
        })
    }
}

/// The cultural look of a nation: which set of city skins the interface draws for it. The
/// simulation never reads it; it is carried so every client shows a Zulu kraal beside a
/// Qing walled town instead of one European skyline for the whole world.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Flavor {
    /// North-western Europe and its settler states: brick and slate, steeples, chimneys.
    #[default]
    Western,
    /// Mediterranean Europe and Latin America: terracotta, whitewash, bell towers.
    Latin,
    /// Orthodox Christendom: onion domes and gilded crosses.
    Orthodox,
    /// The Islamic world from Morocco to the Central Asian khanates: domes and minarets.
    Arab,
    /// China, Japan, Korea and their neighbours: upswept tiled roofs and pagodas.
    EastAsian,
    /// The Indian subcontinent: sandstone, chhatris, and temple spires.
    SouthAsian,
    /// Mainland and island South-East Asia: stilt houses and gilded stupas.
    SoutheastAsian,
    /// Sub-Saharan Africa: thatched round huts inside palisades.
    African,
    /// The steppe and the high plateau: felt tents around a chief's.
    Steppe,
    /// The indigenous Americas: tipis, lodges, and earth mounds.
    Native,
    /// The Pacific: thatched halls on posts under palms.
    Oceanic,
}
impl Flavor {
    pub const ALL: [Flavor; 11] = [
        Self::Western,
        Self::Latin,
        Self::Orthodox,
        Self::Arab,
        Self::EastAsian,
        Self::SouthAsian,
        Self::SoutheastAsian,
        Self::African,
        Self::Steppe,
        Self::Native,
        Self::Oceanic,
    ];
    /// The key used in scenarios and in the pack's `cities` table.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Western => "western",
            Self::Latin => "latin",
            Self::Orthodox => "orthodox",
            Self::Arab => "arab",
            Self::EastAsian => "east_asian",
            Self::SouthAsian => "south_asian",
            Self::SoutheastAsian => "southeast_asian",
            Self::African => "african",
            Self::Steppe => "steppe",
            Self::Native => "native",
            Self::Oceanic => "oceanic",
        }
    }
}

/// How a nation stands in 1876 terms.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Governs itself and conducts its own foreign policy.
    #[default]
    Sovereign,
    /// Governs itself internally but answers to a `suzerain`: a vassal, tributary, protectorate,
    /// or self-governing dominion.
    Dependent,
    /// Holds territory and fields forces but is not recognised as a state (rebels, pretenders).
    Unrecognized,
}

/// Optional replacements for pack rule values, applied for this scenario only.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub victory_industry: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub research_cost: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starting_gold: Option<i32>,
}
impl Rules {
    pub fn with_overrides(&self, overrides: &RuleOverrides) -> Self {
        let mut rules = self.clone();
        if let Some(v) = overrides.victory_industry {
            rules.victory_industry = v;
        }
        if let Some(v) = overrides.research_cost {
            rules.research_cost = v;
        }
        if let Some(v) = overrides.starting_gold {
            rules.starting_gold = v;
        }
        rules
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    /// Always [`SCENARIO_FORMAT`].
    pub format: u32,
    /// Stable lowercase identifier, e.g. `world-1876`.
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Calendar date of turn 1. One turn is one day.
    pub start_date: Date,
    /// Nation ID commanded by the player unless the host chooses another.
    pub commander: String,
    /// First dispatch shown to the player and the idle-panel briefing.
    #[serde(default)]
    pub intro: String,
    /// When set, every nation starts with the whole map charted: all terrain, borders and
    /// cities are known from turn 1. Units are still only seen within sight of a friendly
    /// unit or city. Meant for historical scenarios whose geography is common knowledge.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub charted: bool,
    #[serde(default)]
    pub rules: RuleOverrides,
    /// Terrain glyph rows plus the region layer, and optionally pre-built improvements.
    /// Owners and border claims are derived from the cities.
    pub map: Map,
    pub nations: Vec<NationStart>,
    #[serde(default)]
    pub regions: Vec<RegionStart>,
    pub cities: Vec<CityStart>,
    #[serde(default)]
    pub units: Vec<UnitStart>,
    /// Wars already under way on the start date.
    #[serde(default)]
    pub wars: Vec<WarStart>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NationStart {
    /// Stable lowercase identifier, e.g. `united-kingdom`.
    pub id: String,
    pub name: String,
    /// Used to name raised units and founded cities, e.g. `British`. Defaults to the name.
    #[serde(default)]
    pub adjective: String,
    pub color: Rgb,
    /// Which architecture the nation's cities are drawn in. Defaults to `western`.
    #[serde(default)]
    pub flavor: Flavor,
    #[serde(default)]
    pub status: Status,
    /// Required for `dependent` nations, forbidden otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suzerain: Option<String>,
    #[serde(default)]
    pub government: String,
    /// Head of state or government on the start date, when known.
    #[serde(default)]
    pub leader: String,
    /// Starting treasury. Defaults to the rules' `starting_gold`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gold: Option<i32>,
    /// Starting technology level.
    #[serde(default)]
    pub technology: u32,
    /// Free-form historical context. Not used by the simulation.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegionStart {
    pub id: String,
    pub name: String,
    /// The nation that holds this region on the start date.
    pub nation: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CityStart {
    pub nation: String,
    pub position: Coord,
    pub name: String,
    pub population: i32,
    pub industry: i32,
    /// At most one capital per nation.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub capital: bool,
    /// Cultural border level 1–6, fixed for the whole game. Defaults from population and
    /// capital status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border: Option<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitStart {
    pub nation: String,
    pub position: Coord,
    /// Key into the pack's unit table.
    pub kind: String,
    /// Experience 0–3 (Conscript to Elite). Defaults to Regular.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,
    /// Starts dug in.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fortified: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WarStart {
    pub a: String,
    pub b: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_flavor_has_a_distinct_key_that_serde_agrees_with() {
        let mut keys = std::collections::BTreeSet::new();
        for flavor in Flavor::ALL {
            assert!(keys.insert(flavor.key()));
            let json = serde_json::to_string(&flavor).unwrap();
            assert_eq!(json, format!("\"{}\"", flavor.key()));
            assert_eq!(serde_json::from_str::<Flavor>(&json).unwrap(), flavor);
        }
        assert!(serde_json::from_str::<Flavor>("\"martian\"").is_err());
    }
    #[test]
    fn colours_use_lowercase_hex() {
        let c = Rgb::parse("#C8102E").unwrap();
        assert_eq!(c, Rgb([200, 16, 46]));
        assert_eq!(c.hex(), "#c8102e");
        for bad in ["c8102e", "#c8102", "#c8102ef", "#gggggg", "", "#+81234"] {
            assert!(Rgb::parse(bad).is_none(), "{bad}");
        }
        assert_eq!(serde_json::to_string(&c).unwrap(), "\"#c8102e\"");
    }
}
