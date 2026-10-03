//! `BLDG` — improvements and wonders (city buildings and world wonders).
//!
//! Arm `0x5947D7`, worker `0x594FF0`, row reader `0x5DFF40`, writer `0x5DFAE0`,
//! in-memory stride `0x110`. Editor dialog **157** “Improvements and Wonders”
//! (load into the page `0x423120`, save `0x42206x`).
//!
//! The in-memory row starts with a 4-byte “bytes left” counter, so **mem offset
//! = body offset + 4**; the reader calls `fread` for every field below in file
//! order. Row bodies are **252** bytes (Civ3 4.01, PTW 11.18: up to and
//! including `armies_required`) or **268** bytes (Conquests 12.06+: plus the
//! four dwords from `flavors`). Shorter rows are accepted by the game; fields
//! that are not on disk keep the constructor value, except `revision`,
//! `unit_produced` and `unit_frequency`, which are defaulted to `0`, `-1`, `1`
//! at `0x5E0427`–`0x5E047F`.
//!
//! | body | mem | field | evidence |
//! |---|---|---|---|
//! | `0x00` | `0x04` | [`description`](Building::description) (64) | B |
//! | `0x40` | `0x44` | [`name`](Building::name) (32) | A reader |
//! | `0x60` | `0x64` | [`civilopedia_entry`](Building::civilopedia_entry) (32) | A reader |
//! | `0x80` | `0x84` | [`doubles_happiness_of`](Building::doubles_happiness_of) | A editor load, C (*Sistine Chapel* → *Cathedral*) |
//! | `0x84` | `0x88` | [`gain_in_every_city`](Building::gain_in_every_city) | A editor load |
//! | `0x88` | `0x8C` | [`gain_in_every_city_on_continent`](Building::gain_in_every_city_on_continent) | A editor load, C (*Sun Tzu* → *Barracks*, *Hoover Dam* → *Hydro Plant*) |
//! | `0x8C` | `0x90` | [`required_improvement`](Building::required_improvement) | A editor load, C (*Bank* → *Marketplace*) |
//! | `0x90` | `0x94` | [`cost`](Building::cost) | A |
//! | `0x94` | `0x98` | [`culture`](Building::culture) | A (`0x4F8CE0`) |
//! | `0x98` | `0x9C` | [`bombard_defense`](Building::bombard_defense) | A (`0x4C10B0`, editor *Bombard*) |
//! | `0x9C` | `0xA0` | [`naval_bombard_defense`](Building::naval_bombard_defense) | A (`0x4C11E0`, editor *Naval Bombard Def*) |
//! | `0xA0` | `0xA4` | [`defense_bonus`](Building::defense_bonus) | A (`0x4C10B0`, editor *Defense*) |
//! | `0xA4` | `0xA8` | [`naval_defense_bonus`](Building::naval_defense_bonus) | A (`0x4C11E0`); no editor control |
//! | `0xA8` | `0xAC` | [`maintenance`](Building::maintenance) | A editor load |
//! | `0xAC`..`0xB8` | `0xB0`..`0xBC` | happy / unhappy faces | A editor load |
//! | `0xBC` | `0xC0` | [`num_buildings_required`](Building::num_buildings_required) | A editor load, C (*Wall Street* = 5) |
//! | `0xC0` | `0xC4` | [`air_power`](Building::air_power) | A editor load (*Air*), C (*SAM Missile Battery* = 8) |
//! | `0xC4` | `0xC8` | [`naval_power`](Building::naval_power) | A editor load (*Naval*), C (*Coastal Fortress* = 8) |
//! | `0xC8` | `0xCC` | [`pollution`](Building::pollution) | A editor load, C (Hydro/Solar 0, Coal 2) |
//! | `0xCC` | `0xD0` | [`production`](Building::production) | A editor load |
//! | `0xD0` | `0xD4` | [`required_government`](Building::required_government) | A editor load, C (*Secret Police HQ* = Communism) |
//! | `0xD4` | `0xD8` | [`spaceship_part`](Building::spaceship_part) | A editor load; loader clamps `< -1` to `-1` |
//! | `0xD8` | `0xDC` | [`required_advance`](Building::required_advance) | A editor load, C |
//! | `0xDC` | `0xE0` | [`rendered_obsolete_by`](Building::rendered_obsolete_by) | A editor load, C (*Colossus* → *Flight*) |
//! | `0xE0`, `0xE4` | `0xE4`, `0xE8` | [`required_resource_1`](Building::required_resource_1), [`required_resource_2`](Building::required_resource_2) | A (one 8-byte `fread`), C (*Iron Works* = Iron + Coal) |
//! | `0xE8` | `0xEC` | [`improvement_flags`](Building::improvement_flags) | A |
//! | `0xEC` | `0xF0` | [`other_characteristics`](Building::other_characteristics) | A |
//! | `0xF0` | `0xF4` | [`small_wonder_flags`](Building::small_wonder_flags) | A |
//! | `0xF4` | `0xF8` | [`wonder_flags`](Building::wonder_flags) | A |
//! | `0xF8` | `0xFC` | [`armies_required`](Building::armies_required) | A editor load, C (*Pentagon* = 3) |
//! | `0xFC` | `0x100` | [`flavors`](Building::flavors) (12.06+) | A editor (flavors list box) |
//! | `0x100` | `0x104` | [`revision`](Building::revision) (12.06+) | A reader |
//! | `0x104` | `0x108` | [`unit_produced`](Building::unit_produced) (12.06+) | A editor load |
//! | `0x108` | `0x10C` | [`unit_frequency`](Building::unit_frequency) (12.06+) | A editor load |
//!
//! Every index field is `-1` for “none” and indexes the table named in its docs
//! in all 2 644 shipped rows (a corpus test checks this).
//!
//! # What the loader does (`0x594FF0`, **A**)
//!
//! * Only the **first** row whose [`improvement_flags`] bit 0
//!   ([`CENTER_OF_EMPIRE`](improvement_flags::CENTER_OF_EMPIRE)) is set keeps
//!   it; later rows lose the bit. Every shipped file has exactly row `0`
//!   (*Palace*) flagged.
//! * `spaceship_part` values below `-1` become `-1`.
//! * After the whole array is read the loader stamps `revision = 4` on every
//!   row (`0x5E051F`). Rows read with `revision < 4` are migrated by the reader
//!   (`0x5E0489`..`0x5E0519`): `revision == 0` moves
//!   [`other_characteristics`] bits `10..=16` to `flavors` bits `0..=6`;
//!   `revision < 3` moves [`wonder_flags`] bit 15 to
//!   [`PRODUCES_UNITS`](improvement_flags::PRODUCES_UNITS); `revision < 4` moves
//!   [`small_wonder_flags`] bit 9 to
//!   [`REQUIRED_GOODS_IN_CITY_RADIUS`](improvement_flags::REQUIRED_GOODS_IN_CITY_RADIUS).
//!   Rows shorter than 268 bytes carry no `revision`, so the migrations run for
//!   every PTW/Civ3 row; the shipped PTW rows have none of the legacy bits set.
//! * Scenario versions below **2.08** (*"Setting up data added in v2.08"*,
//!   `0x5950E5`) get `small_wonder_flags` bits 0/1 mirrored into bit 10
//!   (*requires a victorious army*) and `armies_required = 3` for *larger
//!   armies*; versions below **11.18** (*"…removed in v11.18"*, `0x59516A`)
//!   lose [`improvement_flags`] bit 26 (*charm barrier*). No shipped file is
//!   that old, so the crate stores the bytes as they are.
//!
//! [`improvement_flags`]: Building::improvement_flags
//! [`other_characteristics`]: Building::other_characteristics
//! [`small_wonder_flags`]: Building::small_wonder_flags
//! [`wonder_flags`]: Building::wonder_flags

use crate::fixed_record;
use crate::io::Str;

/// Bits of [`Building::improvement_flags`] (body `+0xE8`, mem `+0xEC`): the
/// *Improvements* check-box group of dialog 157. Positions come from the
/// editor's load routine `0x423120` (**A**); every bit except 26 and 28 occurs
/// in the corpus, always on the building the label implies (**C**).
pub mod improvement_flags {
    /// *Center of Empire*: the palace. Loader keeps it on one row only.
    pub const CENTER_OF_EMPIRE: u32 = 1 << 0;
    /// *Veteran Ground Units* (*Barracks*).
    pub const VETERAN_GROUND_UNITS: u32 = 1 << 1;
    /// *+50% Research Output* (*Library*, *University*, *Research Lab*).
    pub const RESEARCH_BONUS: u32 = 1 << 2;
    /// *+50% Luxury Output*.
    pub const LUXURY_BONUS: u32 = 1 << 3;
    /// *+50% Tax Output* (*Marketplace*, *Bank*).
    pub const TAX_BONUS: u32 = 1 << 4;
    /// *Removes Pop. Pollution* (*Mass Transit System*).
    pub const REMOVES_POPULATION_POLLUTION: u32 = 1 << 5;
    /// *Reduces Bldg. Pollution* (*Recycling Center*).
    pub const REDUCES_BUILDING_POLLUTION: u32 = 1 << 6;
    /// *Resistant to Propaganda* (*Courthouse*).
    pub const RESISTANT_TO_PROPAGANDA: u32 = 1 << 7;
    /// *Reduces Corruption* (*Courthouse*, *Police Station*).
    pub const REDUCES_CORRUPTION: u32 = 1 << 8;
    /// *Doubles City Growth Rate* (*Granary*).
    pub const DOUBLES_CITY_GROWTH_RATE: u32 = 1 << 9;
    /// *Increases Luxury Trade* (*Marketplace*).
    pub const INCREASES_LUXURY_TRADE: u32 = 1 << 10;
    /// *Allows City Size Level 2* (*Aqueduct*).
    pub const ALLOWS_CITY_SIZE_LEVEL_2: u32 = 1 << 11;
    /// *Allows City Size Level 3* (*Hospital*, *Sewer*-class buildings).
    pub const ALLOWS_CITY_SIZE_LEVEL_3: u32 = 1 << 12;
    /// *Replaces All Impr. with this Flag Checked* (power plants replace each other).
    pub const REPLACES_ALL_WITH_THIS_FLAG: u32 = 1 << 13;
    /// *Must Be Near Water* (*Harbor*, *Nuclear Plant*).
    pub const MUST_BE_NEAR_WATER: u32 = 1 << 14;
    /// *Must Be Near a River* (*Hydro Plant*, *Hoover Dam*).
    pub const MUST_BE_NEAR_RIVER: u32 = 1 << 15;
    /// *Can Explode or Meltdown* (*Nuclear Plant*).
    pub const CAN_EXPLODE_OR_MELTDOWN: u32 = 1 << 16;
    /// *Veteran Sea Units* (*Harbor*).
    pub const VETERAN_SEA_UNITS: u32 = 1 << 17;
    /// *Veteran Air Units* (*Airport*).
    pub const VETERAN_AIR_UNITS: u32 = 1 << 18;
    /// *Capitalization* (*Wealth*).
    pub const CAPITALIZATION: u32 = 1 << 19;
    /// *Allows Water Trade* (*Harbor*).
    pub const ALLOWS_WATER_TRADE: u32 = 1 << 20;
    /// *Allows Air Trade* (*Airport*).
    pub const ALLOWS_AIR_TRADE: u32 = 1 << 21;
    /// *Reduces War Weariness* (*Police Station*).
    pub const REDUCES_WAR_WEARINESS: u32 = 1 << 22;
    /// *Increases Shields in Water* (*Offshore Platform*).
    pub const INCREASES_SHIELDS_IN_WATER: u32 = 1 << 23;
    /// *Increases Food in Water* (*Harbor*).
    pub const INCREASES_FOOD_IN_WATER: u32 = 1 << 24;
    /// *Increases Trade in Water* (*Commercial Dock*).
    pub const INCREASES_TRADE_IN_WATER: u32 = 1 << 25;
    /// *Charm Barrier*. Absent from the corpus; the loader strips it from
    /// scenarios older than 11.18.
    pub const CHARM_BARRIER: u32 = 1 << 26;
    /// *Stealth Attack Barrier*.
    pub const STEALTH_ATTACK_BARRIER: u32 = 1 << 27;
    /// *Acts as General Telepad*. Absent from the corpus.
    pub const ACTS_AS_GENERAL_TELEPAD: u32 = 1 << 28;
    /// *Doubles Sacrifice*.
    pub const DOUBLES_SACRIFICE: u32 = 1 << 29;
    /// *Produces Units*: the building yields [`Building::unit_produced`]
    /// every [`Building::unit_frequency`] turns (*Statue of Zeus*, *Knights Templar*).
    pub const PRODUCES_UNITS: u32 = 1 << 30;
    /// *Required Goods Must Be Within City Radius* (*Iron Works*).
    pub const REQUIRED_GOODS_IN_CITY_RADIUS: u32 = 1 << 31;
}

/// Bits of [`Building::other_characteristics`] (body `+0xEC`, mem `+0xF0`):
/// the *Other Characteristics* box (civilization traits, installation types)
/// plus the *Category* radio buttons. Bits `12..` are never set in the corpus.
pub mod other_characteristics {
    /// *Coastal Installation*.
    pub const COASTAL_INSTALLATION: u32 = 1 << 0;
    /// *Militaristic* trait.
    pub const MILITARISTIC: u32 = 1 << 1;
    /// *Category: Wonder* (a great wonder; wins over [`SMALL_WONDER`]).
    pub const WONDER: u32 = 1 << 2;
    /// *Category: Sm. Wonder*.
    pub const SMALL_WONDER: u32 = 1 << 3;
    /// *Continental Mood Effects*.
    pub const CONTINENTAL_MOOD_EFFECTS: u32 = 1 << 4;
    /// *Scientific* trait.
    pub const SCIENTIFIC: u32 = 1 << 5;
    /// *Commercial* trait.
    pub const COMMERCIAL: u32 = 1 << 6;
    /// *Expansionist* trait.
    pub const EXPANSIONIST: u32 = 1 << 7;
    /// *Religious* trait.
    pub const RELIGIOUS: u32 = 1 << 8;
    /// *Industrious* trait.
    pub const INDUSTRIOUS: u32 = 1 << 9;
    /// *Agricultural* trait.
    pub const AGRICULTURAL: u32 = 1 << 10;
    /// *Seafaring* trait.
    pub const SEAFARING: u32 = 1 << 11;
}

/// Bits of [`Building::small_wonder_flags`] (body `+0xF0`, mem `+0xF4`): the
/// small-wonder half of the *Improvements, Small and Great Wonders* box.
pub mod small_wonder_flags {
    /// *Increases Chance of Leader Appearance*.
    pub const LEADER_APPEARANCE: u32 = 1 << 0;
    /// *Build Armies Without Leader*.
    pub const ARMIES_WITHOUT_LEADER: u32 = 1 << 1;
    /// *Build Larger Armies* (*Pentagon*; needs `armies_required`).
    pub const LARGER_ARMIES: u32 = 1 << 2;
    /// *Treasury Earns 5%* (*Wall Street*).
    pub const TREASURY_EARNS_5_PERCENT: u32 = 1 << 3;
    /// *Build Spaceship Parts* (*Apollo Program*).
    pub const BUILD_SPACESHIP_PARTS: u32 = 1 << 4;
    /// *Reduces Corruption* (*Forbidden Palace*, *Secret Police HQ*).
    pub const REDUCES_CORRUPTION: u32 = 1 << 5;
    /// *Decreases Success of Missile Attacks by 75%* (*Strategic Missile Defense*).
    pub const DECREASES_MISSILE_SUCCESS: u32 = 1 << 6;
    /// *Allows Spy Missions* (*Intelligence Agency*).
    pub const ALLOWS_SPY_MISSIONS: u32 = 1 << 7;
    /// *Allows Healing in Enemy Territory* (*Battlefield Medicine*).
    pub const ALLOWS_HEALING_IN_ENEMY_TERRITORY: u32 = 1 << 8;
    /// Pre-revision-4 home of
    /// [`REQUIRED_GOODS_IN_CITY_RADIUS`](super::improvement_flags::REQUIRED_GOODS_IN_CITY_RADIUS);
    /// the reader moves it. Never set in the corpus.
    pub const LEGACY_REQUIRED_GOODS: u32 = 1 << 9;
    /// *Requires a Victorious Army* (*Heroic Epic*, *Military Academy*).
    pub const REQUIRES_VICTORIOUS_ARMY: u32 = 1 << 10;
    /// *Requires Elite Naval Units*. Never set in the corpus.
    pub const REQUIRES_ELITE_NAVAL_UNITS: u32 = 1 << 11;
}

/// Bits of [`Building::wonder_flags`] (body `+0xF4`, mem `+0xF8`): the
/// great-wonder half of the *Improvements, Small and Great Wonders* box.
pub mod wonder_flags {
    /// *Safe Sea Travel* (*Great Lighthouse*).
    pub const SAFE_SEA_TRAVEL: u32 = 1 << 0;
    /// *Gain Any Advances Owned by 2 Civs* (*Great Library*).
    pub const GAIN_ADVANCES_OWNED_BY_TWO_CIVS: u32 = 1 << 1;
    /// *Double Combat Strength vs. Barbarians* (*Great Wall*).
    pub const DOUBLE_STRENGTH_VS_BARBARIANS: u32 = 1 << 2;
    /// *+1 Ship Movement* (*Magellan's Voyage*, *Great Lighthouse*).
    pub const PLUS_ONE_SHIP_MOVEMENT: u32 = 1 << 3;
    /// *Doubles Research Output* (*Copernicus' Observatory*, *Newton's*, *SETI*).
    pub const DOUBLES_RESEARCH: u32 = 1 << 4;
    /// *+1 Trade in Each Trade-Producing Tile* (*Colossus*).
    pub const PLUS_ONE_TRADE: u32 = 1 << 5;
    /// *Halves Unit Upgrade Cost* (*Leonardo's Workshop*).
    pub const HALVES_UPGRADE_COST: u32 = 1 << 6;
    /// *Pays Maintenance For Trade Installations* (*Smith's Trading Company*).
    pub const PAYS_TRADE_MAINTENANCE: u32 = 1 << 7;
    /// *Allows Construction of Nuclear Devices* (*Manhattan Project*).
    pub const ALLOWS_NUCLEAR_DEVICES: u32 = 1 << 8;
    /// *City Growth Causes +2 Citizens (instead of +1)* (*Longevity*).
    pub const GROWTH_PLUS_TWO: u32 = 1 << 9;
    /// *+2 Free Advances* (*Theory of Evolution*).
    pub const TWO_FREE_ADVANCES: u32 = 1 << 10;
    /// *Reduces War Weariness in All Cities* (*Universal Suffrage*).
    pub const REDUCES_WAR_WEARINESS_EVERYWHERE: u32 = 1 << 11;
    /// *Doubles City Defenses*.
    pub const DOUBLES_CITY_DEFENSES: u32 = 1 << 12;
    /// *Allows Diplomatic Victory* (*United Nations*).
    pub const ALLOWS_DIPLOMATIC_VICTORY: u32 = 1 << 13;
    /// *+2 Ship Movement*.
    pub const PLUS_TWO_SHIP_MOVEMENT: u32 = 1 << 14;
    /// Pre-revision-3 home of
    /// [`PRODUCES_UNITS`](super::improvement_flags::PRODUCES_UNITS); the reader
    /// moves it, but Conquests-era files still carry a copy.
    pub const LEGACY_PRODUCES_UNITS: u32 = 1 << 15;
    /// *Increased Army Value* (*Military Academy*).
    pub const INCREASED_ARMY_VALUE: u32 = 1 << 16;
    /// *Tourist Attraction* (every ancient wonder in stock Conquests).
    pub const TOURIST_ATTRACTION: u32 = 1 << 17;
}

/// What the editor's *Category* radio group shows for a row, derived from
/// [`other_characteristics`] bits 2 and 3 (`0x4233A..`, **A**).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    /// Ordinary city improvement.
    Improvement,
    /// Great wonder (one per world).
    Wonder,
    /// Small wonder (one per civilization).
    SmallWonder,
}

fixed_record! {
    /// One improvement or wonder row.
    pub struct Building(b"BLDG") {
        /// Scenario notes; unused by the game (editor *Description*).
        pub description: Str<64>,
        /// Display name (`Palace`, …).
        pub name: Str<32>,
        /// Civilopedia key (`BLDG_Palace`).
        pub civilopedia_entry: Str<32>,
        /// *Doubles Happiness Of*: `BLDG` index, `-1` none (*Sistine Chapel* → *Cathedral*).
        pub doubles_happiness_of: i32,
        /// *Gain in Every City*: `BLDG` index, `-1` none.
        pub gain_in_every_city: i32,
        /// *Gain in Every City on Continent*: `BLDG` index, `-1` none
        /// (*Great Wall* → *Walls*, *Sun Tzu* → *Barracks*).
        pub gain_in_every_city_on_continent: i32,
        /// *Required Improvement*: `BLDG` index, `-1` none (*Bank* → *Marketplace*).
        pub required_improvement: i32,
        /// Shield cost (*Palace* = 10).
        pub cost: i32,
        /// Culture points per turn (*Temple* = 2).
        pub culture: i32,
        /// *Bombard* rating tested against bombardment (**A** `0x4C10B0`; *Walls* = 8).
        pub bombard_defense: i32,
        /// *Naval Bombard Def* (**A** `0x4C11E0`; *Coastal Fortress* = 8).
        pub naval_bombard_defense: i32,
        /// *Defense* bonus in percent (**A** `0x4C10B0`; *Walls* = 50).
        pub defense_bonus: i32,
        /// Naval defense bonus in percent (**A** `0x4C11E0`; *Coastal Fortress* = 50).
        /// The editor has no control for it.
        pub naval_defense_bonus: i32,
        /// Gold upkeep per turn (*Temple* = 1).
        pub maintenance: i32,
        /// *Happy (all cities)* (*Hanging Gardens* = 1).
        pub happy_faces_all_cities: i32,
        /// *Happy* faces in the city (*Temple* = 1, *Cathedral* = 3).
        pub happy_faces: i32,
        /// *Unhappy (all cities)*.
        pub unhappy_faces_all_cities: i32,
        /// *Unhappy* faces in the city.
        pub unhappy_faces: i32,
        /// *Number of Buildings Required* (editor range `1..=25`; `1` for
        /// everything but *Wall Street* and its siblings, which use `5`).
        pub num_buildings_required: i32,
        /// *Air* power (*SAM Missile Battery* = 8).
        pub air_power: i32,
        /// *Naval* power (*Coastal Fortress* = 8).
        pub naval_power: i32,
        /// *Pollution* the building causes (*Factory* = 2, *Hydro Plant* = 0).
        pub pollution: i32,
        /// *Production* (*Factory* = 2, power plants 2/4).
        pub production: i32,
        /// *Required Government*: `GOVT` index, `-1` none.
        pub required_government: i32,
        /// *Spaceship Part*: `-1` none, otherwise `0..=9` in stock rules
        /// (*SS Thrusters* = 0 … *SS Exterior Casing* = 9).
        pub spaceship_part: i32,
        /// *Required Advance*: `TECH` index, `-1` none (*Temple* = Ceremonial Burial).
        pub required_advance: i32,
        /// *Rendered Obsolete By*: `TECH` index, `-1` never (*Colossus* → Flight).
        pub rendered_obsolete_by: i32,
        /// First *Required Resource*: `GOOD` index, `-1` none.
        pub required_resource_1: i32,
        /// Second *Required Resource*: `GOOD` index, `-1` none.
        pub required_resource_2: i32,
        /// *Improvements* check boxes, see [`improvement_flags`].
        pub improvement_flags: i32,
        /// *Other Characteristics* and *Category*, see [`other_characteristics`].
        pub other_characteristics: i32,
        /// Small-wonder abilities, see [`small_wonder_flags`].
        pub small_wonder_flags: i32,
        /// Great-wonder abilities, see [`wonder_flags`].
        pub wonder_flags: i32,
        /// *Number of Armies Required* (editor range `0..=50`; *Pentagon* = 3).
        pub armies_required: i32,
    }
    since (12, 6) {
        /// *Flavors*: bit `i` set when `FLAV` flavor `i` applies (bits `0..=6`).
        pub flavors: i32,
        /// Format revision of the row, `4` in every shipped file (see module docs).
        pub revision: i32,
        /// *Units Produced*: `PRTO` index, `-1` none; needs
        /// [`improvement_flags::PRODUCES_UNITS`].
        pub unit_produced: i32,
        /// *Frequency* of the produced unit in turns (`1` default).
        pub unit_frequency: i32,
    }
}

impl Building {
    /// Whether `bit` of [`Building::improvement_flags`] is set
    /// (use the [`improvement_flags`] constants).
    pub fn has_improvement_flag(&self, bit: u32) -> bool {
        self.improvement_flags as u32 & bit != 0
    }

    /// Whether `bit` of [`Building::other_characteristics`] is set.
    pub fn has_characteristic(&self, bit: u32) -> bool {
        self.other_characteristics as u32 & bit != 0
    }

    /// Whether `bit` of [`Building::small_wonder_flags`] is set.
    pub fn has_small_wonder_flag(&self, bit: u32) -> bool {
        self.small_wonder_flags as u32 & bit != 0
    }

    /// Whether `bit` of [`Building::wonder_flags`] is set.
    pub fn has_wonder_flag(&self, bit: u32) -> bool {
        self.wonder_flags as u32 & bit != 0
    }

    /// The *Category* the editor displays (a set wonder bit wins over the
    /// small-wonder bit, as in the load routine at `0x4233A5`).
    pub fn category(&self) -> Category {
        if self.has_characteristic(other_characteristics::WONDER) {
            Category::Wonder
        } else if self.has_characteristic(other_characteristics::SMALL_WONDER) {
            Category::SmallWonder
        } else {
            Category::Improvement
        }
    }

    /// Whether this is the palace-like building (*Center of Empire*).
    pub fn is_center_of_empire(&self) -> bool {
        self.has_improvement_flag(improvement_flags::CENTER_OF_EMPIRE)
    }

    /// The two *Required Resource* slots as `GOOD` indices.
    pub fn required_resources(&self) -> [Option<usize>; 2] {
        [self.required_resource_1, self.required_resource_2].map(|g| usize::try_from(g).ok())
    }

    /// Whether `FLAV` flavor `index` (`0..=31`) is set in [`Building::flavors`].
    pub fn has_flavor(&self, index: u32) -> bool {
        index < 32 && (self.flavors as u32 >> index) & 1 != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Version;
    use crate::io::{Reader, Record, Writer};

    fn conquests_buildings() -> Option<Vec<Building>> {
        let f = crate::corpus::files()
            .into_iter()
            .find(|f| f.name().ends_with("Conquests/conquests.biq"))?;
        let ctx = f.ctx();
        let rows = crate::corpus::rows::<Building>(std::slice::from_ref(&f));
        Some(
            rows.iter()
                .map(|r| Building::read(&mut Reader::new(r.body), &ctx).unwrap())
                .collect(),
        )
    }

    fn by_name<'a>(rows: &'a [Building], name: &str) -> &'a Building {
        rows.iter()
            .find(|b| b.name.text() == name)
            .unwrap_or_else(|| panic!("missing BLDG {name}"))
    }

    #[test]
    fn corpus_roundtrip() {
        if !crate::corpus::available() {
            return;
        }
        let st = crate::corpus::check_roundtrip::<Building>();
        assert_eq!(st.with_extra, 0);
        assert_eq!(st.lengths, vec![252, 268]);
        assert_eq!(
            st.by_version
                .get(&Version::new(11, 18))
                .map(|v| v.as_slice()),
            Some(&[252][..])
        );
        assert_eq!(
            st.by_version
                .get(&Version::new(12, 8))
                .map(|v| v.as_slice()),
            Some(&[268][..])
        );
    }

    #[test]
    fn ptw_row_is_252_bytes_on_write() {
        let files: Vec<_> = crate::corpus::files()
            .into_iter()
            .filter(|f| f.ctx().version == Version::new(11, 18))
            .collect();
        let Some(f) = files.first() else {
            return;
        };
        let rows = crate::corpus::rows::<Building>(std::slice::from_ref(f));
        let Some(row) = rows.first() else {
            return;
        };
        let b = Building::read(&mut Reader::new(row.body), &f.ctx()).unwrap();
        let mut w = Writer::new();
        b.write(&mut w, &f.ctx());
        assert_eq!(
            w.len(),
            252,
            "PTW BLDG rows must not include Conquests tail"
        );
    }

    #[test]
    fn flag_modules_match_the_editor_dialog_count() {
        // 32 improvement bits minus the 26/28 never seen in stock data are all
        // named; the other three dwords name 12 / 12 / 18 bits.
        assert_eq!(
            improvement_flags::REQUIRED_GOODS_IN_CITY_RADIUS,
            0x8000_0000
        );
        assert_eq!(other_characteristics::SEAFARING, 0x800);
        assert_eq!(small_wonder_flags::REQUIRES_ELITE_NAVAL_UNITS, 0x800);
        assert_eq!(wonder_flags::TOURIST_ATTRACTION, 0x2_0000);
    }

    #[test]
    fn palace_temple_barracks_library_granary() {
        let Some(rows) = conquests_buildings() else {
            return;
        };
        let p = by_name(&rows, "Palace");
        assert_eq!(p.cost, 10);
        assert_eq!(p.civilopedia_entry.text(), "BLDG_Palace");
        assert_eq!(p.required_advance, 1);
        assert!(p.is_center_of_empire());
        assert_eq!(p.category(), Category::Improvement);
        assert_eq!(p.revision, 4);

        let temple = by_name(&rows, "Temple");
        assert_eq!(temple.culture, 2);
        assert_eq!(temple.maintenance, 1);
        assert_eq!(temple.happy_faces, 1);
        assert_eq!(temple.happy_faces_all_cities, 0);
        assert!(temple.has_characteristic(other_characteristics::RELIGIOUS));
        assert!(!temple.is_center_of_empire());

        let barracks = by_name(&rows, "Barracks");
        assert!(barracks.has_improvement_flag(improvement_flags::VETERAN_GROUND_UNITS));
        assert!(barracks.has_characteristic(other_characteristics::MILITARISTIC));

        let library = by_name(&rows, "Library");
        assert!(library.has_improvement_flag(improvement_flags::RESEARCH_BONUS));
        assert!(library.has_characteristic(other_characteristics::SCIENTIFIC));

        let granary = by_name(&rows, "Granary");
        assert!(granary.has_improvement_flag(improvement_flags::DOUBLES_CITY_GROWTH_RATE));

        let courthouse = by_name(&rows, "Courthouse");
        assert!(courthouse.has_improvement_flag(improvement_flags::REDUCES_CORRUPTION));
        assert!(courthouse.has_improvement_flag(improvement_flags::RESISTANT_TO_PROPAGANDA));

        let market = by_name(&rows, "Marketplace");
        assert!(market.has_improvement_flag(improvement_flags::TAX_BONUS));
        assert!(market.has_improvement_flag(improvement_flags::INCREASES_LUXURY_TRADE));
        assert!(market.has_characteristic(other_characteristics::COMMERCIAL));
    }

    #[test]
    fn combat_values_and_city_size_gates() {
        let Some(rows) = conquests_buildings() else {
            return;
        };
        let w = by_name(&rows, "Walls");
        assert_eq!(w.bombard_defense, 8);
        assert_eq!(w.defense_bonus, 50);
        assert_eq!(w.naval_bombard_defense, 0);
        assert_eq!(w.naval_defense_bonus, 0);

        let c = by_name(&rows, "Coastal Fortress");
        assert_eq!(c.naval_bombard_defense, 8);
        assert_eq!(c.naval_defense_bonus, 50);
        assert_eq!(c.naval_power, 8);
        assert_eq!(c.air_power, 0);
        assert_eq!(c.required_resources(), [Some(1), Some(2)]);
        assert!(c.has_characteristic(other_characteristics::COASTAL_INSTALLATION));
        assert!(c.has_characteristic(other_characteristics::SEAFARING));

        let sam = by_name(&rows, "SAM Missile Battery");
        assert_eq!(sam.air_power, 8);
        assert_eq!(sam.num_buildings_required, 1);

        let aqueduct = by_name(&rows, "Aqueduct");
        assert!(aqueduct.has_improvement_flag(improvement_flags::ALLOWS_CITY_SIZE_LEVEL_2));
        assert!(!aqueduct.has_improvement_flag(improvement_flags::ALLOWS_CITY_SIZE_LEVEL_3));
        let hospital = by_name(&rows, "Hospital");
        assert!(hospital.has_improvement_flag(improvement_flags::ALLOWS_CITY_SIZE_LEVEL_3));
    }

    #[test]
    fn power_plants_and_pollution() {
        let Some(rows) = conquests_buildings() else {
            return;
        };
        let coal = by_name(&rows, "Coal Plant");
        assert_eq!(coal.pollution, 2);
        assert_eq!(coal.production, 2);
        assert_eq!(coal.required_resources(), [Some(3), None]);
        assert!(coal.has_improvement_flag(improvement_flags::REPLACES_ALL_WITH_THIS_FLAG));

        let hydro = by_name(&rows, "Hydro Plant");
        assert_eq!(hydro.pollution, 0);
        assert!(hydro.has_improvement_flag(improvement_flags::MUST_BE_NEAR_RIVER));

        let nuclear = by_name(&rows, "Nuclear Plant");
        assert_eq!(nuclear.production, 4);
        assert!(nuclear.has_improvement_flag(improvement_flags::CAN_EXPLODE_OR_MELTDOWN));
        assert!(nuclear.has_improvement_flag(improvement_flags::MUST_BE_NEAR_WATER));

        let iron_works = by_name(&rows, "Iron Works");
        assert_eq!(iron_works.required_resources(), [Some(1), Some(3)]);
        assert!(iron_works.has_improvement_flag(improvement_flags::REQUIRED_GOODS_IN_CITY_RADIUS));
        assert_eq!(iron_works.pollution, 4);
    }

    #[test]
    fn spaceship_parts_and_government_gate() {
        let Some(rows) = conquests_buildings() else {
            return;
        };
        let parts: Vec<i32> = rows
            .iter()
            .filter(|b| b.name.text().starts_with("SS "))
            .map(|b| b.spaceship_part)
            .collect();
        assert_eq!(parts, (0..=9).collect::<Vec<_>>());
        assert_eq!(
            by_name(&rows, "SS Stasis Chamber").required_resources(),
            [Some(6), Some(7)]
        );
        assert_eq!(by_name(&rows, "Palace").spaceship_part, -1);

        let police = by_name(&rows, "Secret Police HQ");
        assert_eq!(police.required_government, 3);
        assert_eq!(by_name(&rows, "Library").required_government, -1);
    }

    #[test]
    fn wonders_and_small_wonders() {
        let Some(rows) = conquests_buildings() else {
            return;
        };
        let pyramids = by_name(&rows, "The Pyramids");
        assert_eq!(pyramids.required_advance, 1);
        assert_eq!(pyramids.category(), Category::Wonder);
        assert!(pyramids.has_wonder_flag(wonder_flags::TOURIST_ATTRACTION));

        let colossus = by_name(&rows, "The Colossus");
        assert_eq!(colossus.rendered_obsolete_by, 58); // Flight
        assert!(colossus.has_wonder_flag(wonder_flags::PLUS_ONE_TRADE));
        assert!(colossus.has_characteristic(other_characteristics::COASTAL_INSTALLATION));

        let lighthouse = by_name(&rows, "The Great Lighthouse");
        assert!(lighthouse.has_wonder_flag(wonder_flags::SAFE_SEA_TRAVEL));
        assert!(lighthouse.has_wonder_flag(wonder_flags::PLUS_ONE_SHIP_MOVEMENT));

        let library = by_name(&rows, "The Great Library");
        assert!(library.has_wonder_flag(wonder_flags::GAIN_ADVANCES_OWNED_BY_TWO_CIVS));

        let sistine = by_name(&rows, "Sistine Chapel");
        assert_eq!(sistine.doubles_happiness_of, 10); // Cathedral
        let wall = by_name(&rows, "The Great Wall");
        assert_eq!(wall.gain_in_every_city_on_continent, 7); // Walls

        let un = by_name(&rows, "The United Nations");
        assert!(un.has_wonder_flag(wonder_flags::ALLOWS_DIPLOMATIC_VICTORY));

        let forbidden = by_name(&rows, "Forbidden Palace");
        assert_eq!(forbidden.category(), Category::SmallWonder);
        assert!(forbidden.has_small_wonder_flag(small_wonder_flags::REDUCES_CORRUPTION));
        assert!(forbidden.has_characteristic(other_characteristics::RELIGIOUS));

        let wall_street = by_name(&rows, "Wall Street");
        assert_eq!(wall_street.num_buildings_required, 5);
        assert_eq!(wall_street.required_improvement, 76); // Stock Exchange
        assert!(wall_street.has_small_wonder_flag(small_wonder_flags::TREASURY_EARNS_5_PERCENT));

        let pentagon = by_name(&rows, "The Pentagon");
        assert_eq!(pentagon.armies_required, 3);
        assert!(pentagon.has_small_wonder_flag(small_wonder_flags::LARGER_ARMIES));

        let heroic = by_name(&rows, "Heroic Epic");
        assert!(heroic.has_small_wonder_flag(small_wonder_flags::LEADER_APPEARANCE));
        assert!(heroic.has_small_wonder_flag(small_wonder_flags::REQUIRES_VICTORIOUS_ARMY));

        let academy = by_name(&rows, "Military Academy");
        assert!(academy.has_wonder_flag(wonder_flags::INCREASED_ARMY_VALUE));
        assert!(academy.has_small_wonder_flag(small_wonder_flags::ARMIES_WITHOUT_LEADER));
    }

    #[test]
    fn produced_units() {
        let Some(rows) = conquests_buildings() else {
            return;
        };
        for name in ["The Statue of Zeus", "Knights Templar"] {
            let b = by_name(&rows, name);
            assert!(
                b.has_improvement_flag(improvement_flags::PRODUCES_UNITS),
                "{name}"
            );
            assert!(b.unit_produced >= 0, "{name}");
            assert!(b.unit_frequency >= 1, "{name}");
        }
        assert_eq!(by_name(&rows, "Palace").unit_produced, -1);
        assert_eq!(by_name(&rows, "Palace").unit_frequency, 1);
    }

    /// Cross-table invariants over every shipped row (**C**): each index field
    /// points into the table it is documented to, every flag bit is named,
    /// exactly row 0 is the *Center of Empire*, and the category bits never
    /// overlap.
    #[test]
    fn corpus_fields_index_their_tables() {
        if !crate::corpus::available() {
            return;
        }
        use crate::Biq;
        let mut rows_seen = 0;
        let mut flavored = 0;
        for file in crate::corpus::files() {
            let biq = Biq::from_raw(&file.raw).unwrap();
            let rules = &biq.rules;
            let buildings = &rules.buildings;
            if buildings.is_empty() {
                continue;
            }
            let (nb, nt, ng, nv) = (
                buildings.len() as i32,
                rules.techs.len() as i32,
                rules.goods.len() as i32,
                rules.governments.len() as i32,
            );
            let np = rules.unit_types.len() as i32;
            let nf = biq.flavors.as_ref().map_or(0, |f| f.flavors.len() as u32);
            let ok = |v: i32, n: i32| v == -1 || (0..n).contains(&v);
            let centers: Vec<usize> = buildings
                .iter()
                .enumerate()
                .filter(|(_, b)| b.is_center_of_empire())
                .map(|(i, _)| i)
                .collect();
            assert_eq!(centers, [0], "{}", file.name());
            for b in buildings {
                rows_seen += 1;
                let n = b.name.text();
                assert!(ok(b.doubles_happiness_of, nb), "{n}");
                assert!(ok(b.gain_in_every_city, nb), "{n}");
                assert!(ok(b.gain_in_every_city_on_continent, nb), "{n}");
                assert!(ok(b.required_improvement, nb), "{n}");
                assert!(ok(b.required_government, nv), "{n}");
                assert!(ok(b.required_advance, nt), "{n}");
                assert!(ok(b.rendered_obsolete_by, nt), "{n}");
                assert!(ok(b.required_resource_1, ng), "{n}");
                assert!(ok(b.required_resource_2, ng), "{n}");
                assert!((-1..=9).contains(&b.spaceship_part), "{n}");
                assert_eq!(b.other_characteristics as u32 >> 12, 0, "{n}");
                assert_eq!(b.small_wonder_flags as u32 >> 12, 0, "{n}");
                assert_eq!(b.wonder_flags as u32 >> 18, 0, "{n}");
                assert!(
                    !(b.has_characteristic(other_characteristics::WONDER)
                        && b.has_characteristic(other_characteristics::SMALL_WONDER)),
                    "{n}"
                );
                if file.version >= Version::new(12, 6) {
                    assert!(ok(b.unit_produced, np), "{n}");
                    assert_eq!(b.revision, 4, "{n}");
                    assert!(b.unit_frequency >= 1, "{n}");
                    if b.flavors != 0 {
                        flavored += 1;
                        assert!(
                            nf > 0 && (b.flavors as u32) < (1 << nf),
                            "{n} {}",
                            b.flavors
                        );
                    }
                    if b.has_improvement_flag(improvement_flags::PRODUCES_UNITS) {
                        assert!(b.unit_produced >= 0, "{n}");
                    }
                }
            }
        }
        assert!(rows_seen > 2000, "rows seen: {rows_seen}");
        assert!(flavored > 20, "flavored rows: {flavored}");
    }
}
