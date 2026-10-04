//! The unit and building rosters of `conquests.biq`.
//!
//! The rows are generated (`rules_data.rs`, see
//! `biq/examples/gen_game_rules.rs`): `UnitType(i)` is `PRTO` row `i`,
//! `Production(UNIT_COUNT + i)` is `BLDG` row `i`, so every cross-reference
//! the file makes (upgrade chains, required improvements, wonder grants) is a
//! plain index here too. This module holds the row types, the flag bits the
//! game reads, and the lookups that walk those references.

pub use crate::rules_data::{BLDG_COUNT, BLDGS, UNIT_COUNT, UNITS};

/// One `PRTO` row, the fields the game reads.
#[derive(Debug)]
pub struct UnitRow {
    pub name: &'static str,
    /// `Art/Units` folder, empty for a unit the game has no art or rules for.
    pub art: &'static str,
    /// Cell of the 14-column `units_32` icon sheet.
    pub icon: i32,
    pub attack: i32,
    pub defense: i32,
    pub moves: u8,
    pub sight: u8,
    pub hp_bonus: i32,
    /// Shields, as a human pays (`city-turn.md` section 4.2).
    pub cost: i32,
    /// Citizens consumed when the unit is completed.
    pub pop_cost: i32,
    pub tech: i32,
    /// Next unit of the upgrade chain, -1 for none.
    pub upgrade_to: i32,
    /// Strategic resources needed, -1 for none (`GOOD` rows).
    pub resources: [i32; 3],
    pub abilities: u32,
    pub special: u32,
    pub worker: u32,
    pub worker_strength: f32,
    pub bombard: i32,
    pub bomb_range: i32,
    pub rof: i32,
    pub capacity: i32,
    /// 0 land, 1 sea, 2 air.
    pub class: i32,
    /// One bit per `RACE` row.
    pub races: u32,
    pub ai: u32,
    pub zoc: bool,
    /// Has art and is a unit the game plays with.
    pub playable: bool,
}

/// One `BLDG` row, the fields the game reads.
#[derive(Debug)]
pub struct BldgDef {
    pub name: &'static str,
    /// In tens of shields (a human pays `cost * 10`).
    pub cost: i32,
    pub upkeep: i32,
    pub culture: i32,
    pub tech: i32,
    pub obsolete: i32,
    /// `BLDG` row that must stand in the city, -1 for none.
    pub requires: i32,
    pub govt: i32,
    pub resources: [i32; 2],
    pub happy: i32,
    pub happy_all: i32,
    pub unhappy: i32,
    pub unhappy_all: i32,
    /// Percent added to the city's defense.
    pub defense: i32,
    /// `BLDG +0xD0`: shield bonus in quarters (Factory 2 = +50%).
    pub production: i32,
    /// Improvement every city gets from this wonder (`+0x88`), -1 for none.
    pub grant_all: i32,
    /// Improvement every city on the wonder's continent gets (`+0x8C`).
    pub grant_continent: i32,
    /// Improvement whose happiness this one doubles, -1 for none.
    pub doubles: i32,
    pub flags: u32,
    pub other: u32,
    pub small: u32,
    pub wonder: u32,
    /// `PRTO` row this building produces on a timer (`flags` bit 30), -1 for none.
    pub produces: i32,
    /// Turns between two of those units.
    pub frequency: i32,
    /// The game implements the building's effects.
    pub playable: bool,
}

/// `PRTO` ability bits (`biq::sections::prto::ability`).
pub mod ability {
    pub const WHEELED: u32 = 1 << 0;
    pub const FOOT_UNIT: u32 = 1 << 1;
    pub const BLITZ: u32 = 1 << 2;
    pub const IMMOBILE: u32 = 1 << 10;
    /// A victory with this unit starts a Golden Age.
    pub const STARTS_GOLDEN_AGE: u32 = 1 << 15;
    pub const ARMY: u32 = 1 << 18;
    /// The Great Leader (`research.md` 11: ability 19).
    pub const LEADER: u32 = 1 << 19;
}

/// `PRTO` special-action bits.
pub mod special {
    pub const PILLAGE: u32 = 1 << 3;
    pub const BOMBARD: u32 = 1 << 4;
    pub const UPGRADE_UNIT: u32 = 1 << 8;
    pub const CAPTURE: u32 = 1 << 9;
}

/// `PRTO` worker-action bits.
pub mod worker {
    pub const BUILD_CITY: u32 = 1 << 1;
    pub const BUILD_ROAD: u32 = 1 << 2;
    pub const BUILD_RAILROAD: u32 = 1 << 3;
    pub const BUILD_FORT: u32 = 1 << 4;
    pub const BUILD_MINE: u32 = 1 << 5;
    pub const IRRIGATE: u32 = 1 << 6;
    pub const CLEAR_FOREST: u32 = 1 << 7;
    pub const CLEAR_JUNGLE: u32 = 1 << 8;
    pub const PLANT_FOREST: u32 = 1 << 9;
    pub const CLEAR_POLLUTION: u32 = 1 << 10;
    pub const AUTOMATE: u32 = 1 << 11;
    pub const JOIN_CITY: u32 = 1 << 12;
}

/// `BLDG.improvement_flags` bits.
pub mod imp {
    pub const CENTER_OF_EMPIRE: u32 = 1 << 0;
    pub const VETERAN_GROUND_UNITS: u32 = 1 << 1;
    pub const RESEARCH_BONUS: u32 = 1 << 2;
    pub const VETERAN_SEA_UNITS: u32 = 1 << 17;
    /// *Produces Units*: `BldgDef.produces` every `frequency` turns.
    pub const PRODUCES_UNITS: u32 = 1 << 30;
    pub const LUXURY_BONUS: u32 = 1 << 3;
    pub const TAX_BONUS: u32 = 1 << 4;
    pub const REDUCES_CORRUPTION: u32 = 1 << 8;
    /// "Increases luxury trade": the Marketplace (`happiness.md` 3, step 6).
    pub const LUXURY_TRADE: u32 = 1 << 10;
    /// "Doubles City Growth Rate": in `conquests.biq` the Granary's flag.
    pub const KEEPS_FOOD: u32 = 1 << 9;
    pub const ALLOWS_SIZE_LEVEL_2: u32 = 1 << 11;
    pub const ALLOWS_SIZE_LEVEL_3: u32 = 1 << 12;
    pub const REPLACES_ALL: u32 = 1 << 13;
    pub const MUST_BE_NEAR_WATER: u32 = 1 << 14;
    pub const MUST_BE_NEAR_RIVER: u32 = 1 << 15;
    pub const CAPITALIZATION: u32 = 1 << 19;
    pub const REDUCES_WAR_WEARINESS: u32 = 1 << 22;
    pub const INCREASES_FOOD_IN_WATER: u32 = 1 << 24;
}

/// `BLDG.other_characteristics` bits.
pub mod oth {
    pub const COASTAL: u32 = 1 << 0;
    pub const MILITARISTIC: u32 = 1 << 1;
    pub const WONDER: u32 = 1 << 2;
    pub const SMALL_WONDER: u32 = 1 << 3;
    pub const SCIENTIFIC: u32 = 1 << 5;
    pub const COMMERCIAL: u32 = 1 << 6;
    pub const RELIGIOUS: u32 = 1 << 8;
    pub const AGRICULTURAL: u32 = 1 << 10;
    pub const SEAFARING: u32 = 1 << 11;
}

/// `BLDG.wonder_flags` bits.
pub mod wonder {
    pub const GAIN_TECHS_OF_TWO_CIVS: u32 = 1 << 1;
    pub const SUFFRAGE: u32 = 1 << 11;
    pub const DOUBLE_VS_BARBARIANS: u32 = 1 << 2;
    pub const DOUBLES_RESEARCH: u32 = 1 << 4;
    pub const PLUS_ONE_TRADE: u32 = 1 << 5;
    pub const HALVES_UPGRADE_COST: u32 = 1 << 6;
    pub const PAYS_TRADE_MAINTENANCE: u32 = 1 << 7;
    pub const TWO_FREE_ADVANCES: u32 = 1 << 10;
}

/// A unit row; panics on an out-of-range index.
pub fn unit(i: usize) -> &'static UnitRow {
    &UNITS[i]
}

/// A building row; panics on an out-of-range index.
pub fn bldg(i: usize) -> &'static BldgDef {
    &BLDGS[i]
}

impl BldgDef {
    pub fn is_great_wonder(&self) -> bool {
        self.other & oth::WONDER != 0
    }

    pub fn is_small_wonder(&self) -> bool {
        self.other & oth::SMALL_WONDER != 0
    }

    /// Shields a human pays (`0x569FE0`, base factor 10).
    pub fn shields(&self) -> i32 {
        (self.cost * 10).max(1)
    }
}

/// The units after `u` on its upgrade chain, nearest first. Stops at the
/// end of the chain or on a cycle.
pub fn upgrade_chain(u: usize) -> impl Iterator<Item = usize> {
    let mut next = UNITS[u].upgrade_to;
    let mut steps = 0;
    std::iter::from_fn(move || {
        if next < 0 || steps > UNIT_COUNT {
            return None;
        }
        steps += 1;
        let here = next as usize;
        next = UNITS[here].upgrade_to;
        Some(here)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cities::Production;
    use crate::units::UnitType;

    #[test]
    fn named_constants_point_at_their_rows() {
        assert_eq!(unit(UnitType::Warrior.0 as usize).name, "Warrior");
        assert_eq!(unit(UnitType::Settler.0 as usize).name, "Settler");
        assert_eq!(unit(UnitType::ThreeManChariot.0 as usize).name, "Three-Man Chariot");
        assert_eq!(Production::Temple.name(), "Temple");
        assert_eq!(Production::Barracks.name(), "Barracks");
        assert_eq!(Production::Warrior.name(), "Warrior");
        assert_eq!(Production::SunTzusArtOfWar.name(), "Sun Tzu's Art of War");
    }

    #[test]
    fn the_cross_references_resolve() {
        for (i, u) in UNITS.iter().enumerate() {
            assert!(u.upgrade_to < UNIT_COUNT as i32, "{}", u.name);
            // No upgrade chain loops back on itself.
            assert!(upgrade_chain(i).count() < UNIT_COUNT, "{}", u.name);
        }
        for b in BLDGS.iter() {
            for r in [b.requires, b.grant_all, b.grant_continent, b.doubles] {
                assert!(r < BLDG_COUNT as i32, "{}", b.name);
            }
        }
    }

    #[test]
    fn unit_art_exists_for_every_playable_unit() {
        // Skipped without the converted art (`tools/prep_assets.py`).
        if !std::path::Path::new("assets/gen/units").is_dir() {
            eprintln!("skipped: assets/gen/units is not built");
            return;
        }
        for u in UNITS.iter().filter(|u| u.playable) {
            let m = format!("assets/gen/units/{}/manifest.json", u.art);
            assert!(std::path::Path::new(&m).is_file(), "{}: no {m}", u.name);
        }
    }

    #[test]
    fn leaders_and_armies_have_art_for_every_era() {
        if !std::path::Path::new("assets/gen/units").is_dir() {
            return;
        }
        for u in UNITS.iter().filter(|u| u.playable && u.art.ends_with("Ancient Times")) {
            for era in ["Ancient Times", "Middle Ages", "Industrial Ages", "Modern Times"] {
                let m = format!("assets/gen/units/{}/manifest.json", u.art.replace("Ancient Times", era));
                assert!(std::path::Path::new(&m).is_file(), "{}: no {m}", u.name);
            }
        }
    }

    #[test]
    fn the_roster_has_what_the_game_needs() {
        let playable: Vec<_> = UNITS.iter().filter(|u| u.playable).map(|u| u.name).collect();
        for n in ["Settler", "Worker", "Scout", "Warrior", "Archer", "Spearman", "Horseman"] {
            assert!(playable.contains(&n), "{n}");
        }
        // The costs the exe charges: Barracks 4 is 40 shields.
        assert_eq!(bldg(Production::Barracks.0 as usize - UNIT_COUNT).shields(), 40);
    }
}
