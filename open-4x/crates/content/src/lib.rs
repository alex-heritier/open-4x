//! Versioned, portable JSON content. Asset identifiers are pack-relative paths.
use fourx_sim::{Rules, Scenario};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path};

pub const BASE_JSON: &str = include_str!("../../../assets/packs/base/pack.json");
pub const BASE_SCRIPT: &str = include_str!("../../../assets/packs/base/scripts/campaign.lua");

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Visuals {
    pub terrain: String,
    pub forest: String,
    pub mountain: String,
    pub city: String,
    pub turn_sound: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pack {
    pub format: u32,
    pub id: String,
    pub name: String,
    pub rules: Rules,
    pub script: String,
    pub visuals: Visuals,
    #[serde(default)]
    pub scenario: Option<Scenario>,
}
#[derive(Debug, thiserror::Error)]
pub enum ContentError {
    #[error("Invalid content: {0}")]
    Invalid(String),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
pub fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && !path.contains(':')
        && !path.contains('\0')
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
        && !path
            .split('/')
            .any(|c| c == "." || c == ".." || c.is_empty())
}
impl Pack {
    pub fn parse(json: &str) -> Result<Self, ContentError> {
        let pack: Self = serde_json::from_str(json)?;
        pack.validate()?;
        Ok(pack)
    }
    pub fn base() -> Self {
        Self::parse(BASE_JSON).expect("bundled content is validated by tests")
    }
    pub fn validate(&self) -> Result<(), ContentError> {
        let invalid = |s: &str| ContentError::Invalid(s.into());
        if self.format != 1 {
            return Err(invalid("Unsupported pack format"));
        }
        if self.id.is_empty() || self.id.len() > 64 {
            return Err(invalid("Invalid pack id"));
        }
        if self.rules.units.is_empty() || self.rules.units.len() > 128 {
            return Err(invalid("A pack needs 1–128 unit designs"));
        }
        if self.rules.combat_width == 0
            || self.rules.combat_width > 64
            || !(1..=1_000_000).contains(&self.rules.victory_industry)
            || !(1..=100_000).contains(&self.rules.research_cost)
            || !(0..=1_000_000).contains(&self.rules.starting_gold)
        {
            return Err(invalid("Rules values out of range"));
        }
        if !self.rules.units.values().any(|u| u.settler && !u.naval)
            || !self.rules.units.values().any(|u| !u.settler && !u.naval)
        {
            return Err(invalid("A pack needs land pioneers and land combat units"));
        }
        for (key, unit) in &self.rules.units {
            if key != &unit.id
                || !safe_path(&unit.sprite)
                || !(1..=10_000).contains(&unit.cost)
                || !(0..=100).contains(&unit.fire)
                || !(0..=100).contains(&unit.shock)
                || !(1..=20).contains(&unit.speed)
                || unit.settler && unit.naval
            {
                return Err(invalid("Invalid unit design or sprite path"));
            }
        }
        for path in [
            &self.script,
            &self.visuals.terrain,
            &self.visuals.forest,
            &self.visuals.mountain,
            &self.visuals.city,
            &self.visuals.turn_sound,
        ] {
            if !safe_path(path) {
                return Err(invalid("Asset paths must stay inside the pack"));
            }
        }
        if let Some(s) = &self.scenario {
            if !(8..=128).contains(&s.width)
                || !(8..=128).contains(&s.height)
                || s.cities.len() > 64
                || s.armies.len() > 128
                || s.tiles.len() > (s.width * s.height) as usize
            {
                return Err(invalid("Scenario dimensions or entity counts out of range"));
            }
            let inside = |p: fourx_sim::terrain::Coord| {
                p.x >= 0 && p.y >= 0 && p.x < s.width && p.y < s.height
            };
            let owner = |id| id == 1 || id == 2;
            for id in [1, 2] {
                if !s.cities.iter().any(|c| c.owner == id) {
                    return Err(invalid("Both campaign factions need a starting city"));
                }
            }
            for c in &s.cities {
                if !owner(c.owner)
                    || !inside(c.position)
                    || c.name.is_empty()
                    || c.name.len() > 64
                    || !(1..=100).contains(&c.population)
                    || !(1..=100).contains(&c.industry)
                {
                    return Err(invalid("Invalid starting city"));
                }
            }
            for a in &s.armies {
                if !owner(a.owner)
                    || !inside(a.position)
                    || a.kinds.is_empty()
                    || a.kinds.len() > 64
                    || a.name.len() > 64
                    || !(0..=10).contains(&a.general)
                    || a.kinds.iter().any(|k| !self.rules.units.contains_key(k))
                {
                    return Err(invalid("Invalid starting army"));
                }
                let naval = self.rules.units[&a.kinds[0]].naval;
                if a.kinds.iter().any(|k| self.rules.units[k].naval != naval)
                    || naval && s.cities.iter().any(|c| c.position == a.position)
                {
                    return Err(invalid("Mixed land/naval army or fleet in a city"));
                }
                if s.armies.iter().any(|b| {
                    b.position == a.position
                        && b.kinds
                            .first()
                            .and_then(|kind| self.rules.units.get(kind))
                            .is_some_and(|u| u.naval != naval)
                }) {
                    return Err(invalid("Land and naval armies share a starting tile"));
                }
            }
            if s.tiles.iter().any(|t| !inside(t.position)) {
                return Err(invalid("Tile override is outside the scenario"));
            }
            if s.cities
                .iter()
                .enumerate()
                .any(|(i, c)| s.cities[i + 1..].iter().any(|b| b.position == c.position))
            {
                return Err(invalid("Starting cities overlap"));
            }
        }
        Ok(())
    }
}

/// A complete pack replaces the base definitions. Explicit ordering keeps mod loads reproducible.
/// Canonicalize every asset to reject symlinks that escape the pack directory.
pub fn load_directory(directory: &Path) -> Result<(Pack, String), ContentError> {
    let root = directory.canonicalize()?;
    let pack = Pack::parse(&std::fs::read_to_string(root.join("pack.json"))?)?;
    for relative in std::iter::once(&pack.script)
        .chain([
            &pack.visuals.terrain,
            &pack.visuals.forest,
            &pack.visuals.mountain,
            &pack.visuals.city,
            &pack.visuals.turn_sound,
        ])
        .chain(pack.rules.units.values().map(|u| &u.sprite))
    {
        let path = root.join(relative).canonicalize()?;
        if !path.starts_with(&root) || !path.is_file() {
            return Err(ContentError::Invalid(
                "Asset escapes pack directory or is not a file".into(),
            ));
        }
    }
    let script = std::fs::read_to_string(root.join(&pack.script))?;
    if script.len() > 256 * 1024 {
        return Err(ContentError::Invalid("Script exceeds 256 KiB".into()));
    }
    Ok((pack, script))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn base_is_valid() {
        Pack::base().validate().unwrap();
    }
    #[test]
    fn traversal_is_rejected() {
        for p in [
            "../x",
            "/x",
            "x/../y",
            "x\\y",
            "C:/x",
            "./x",
            "x//y",
            "http://host/x",
        ] {
            assert!(!safe_path(p), "{p}");
        }
        assert!(safe_path("sprites/ship.png"));
    }
    #[test]
    fn dangerous_numbers_rejected() {
        let mut p = Pack::base();
        p.rules.combat_width = 0;
        assert!(p.validate().is_err());
    }
    #[test]
    fn malformed_scenarios_rejected_without_panics() {
        let mut pack = Pack::base();
        pack.scenario.as_mut().unwrap().armies[1].kinds.clear();
        assert!(pack.validate().is_err());
        let mut pack = Pack::base();
        pack.scenario.as_mut().unwrap().width = 8;
        assert!(pack.validate().is_err());
    }
    #[test]
    fn scenario_names_and_starting_industry_are_moddable() {
        let mut pack = Pack::base();
        let scenario = pack.scenario.as_mut().unwrap();
        scenario.player_name = "A custom empire".into();
        scenario.cities[0].industry = 9;
        pack.validate().unwrap();
        let game = fourx_sim::Game::from_scenario(5, &pack.rules, pack.scenario.as_ref().unwrap());
        assert_eq!(game.factions[&1].name, "A custom empire");
        assert_eq!(game.cities[&1].industry, 9);
    }
}
