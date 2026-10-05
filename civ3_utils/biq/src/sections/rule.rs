//! `RULE` — the scenario's general settings (exactly one row per file).
//!
//! The editor shows this row on the *General Settings* page (dialog 131). The
//! game keeps it in a global object at `0x9C71E4`: the field at memory offset
//! `m` is the dword at `0x9C71E4 + m`, so nearly every consumer is a plain
//! absolute load. The object's initialiser is `0x5E7020` (run before every
//! scenario load), its row reader `0x5E78E0`, its row writer `0x5E7320`. The
//! editor page embeds a copy of the same object at page offset `+0x168`, loads
//! it into the controls at `0x41DDF0` and stores them back at `0x41D880`,
//! clamping each value to the ranges below.
//!
//! # Layout
//!
//! The row is a sequence, not a table: two lists make every later offset
//! depend on their length. The game reads it front to back, a field only if all
//! its bytes are still there, so an older, shorter row leaves the later fields
//! at the initialiser's values ([`GeneralRules::default`]). The writer has no
//! version test at all; the file version decides here only for the last two
//! dwords, see below.
//!
//! | # | memory offset | field | editor control | editor range | game consumers |
//! |---|---------------|-------|----------------|--------------|----------------|
//! | 1 | `+0x08` | [`town_name`](GeneralRules::town_name) (32 bytes) | edit 1519 | 31 chars | |
//! | 2 | `+0x28` | [`city_name`](GeneralRules::city_name) (32) | edit 1520 | 31 chars | |
//! | 3 | `+0x48` | [`metropolis_name`](GeneralRules::metropolis_name) (32) | edit 1521 | 31 chars | |
//! | 4 | `+0xC4` | part-type count: the length of [`spaceship_parts_needed`](GeneralRules::spaceship_parts_needed) | edit 1120 | `0..=100` (the reader clamps) | `0x5A34B3` and 18 more |
//! | 5 | list at `+0x68` | [`spaceship_parts_needed`](GeneralRules::spaceship_parts_needed) (`count` × `i32`) | edit 1123 for the part chosen in combo 1642 | `0..=100` | `0x5A34D5`, `0x56A575` |
//! | 6 | `+0x6C` | [`advanced_barbarian_unit`](GeneralRules::advanced_barbarian_unit) | combo 1061 | a ground unit | `0x55FE4F`, `0x56061B` |
//! | 7 | `+0x70` | [`basic_barbarian_unit`](GeneralRules::basic_barbarian_unit) | combo 1110 | a ground unit | `0x44FBA9`, `0x55C120`, `0x55F9B1` |
//! | 8 | `+0x74` | [`barbarian_sea_unit`](GeneralRules::barbarian_sea_unit) | combo 1112 | a sea unit | `0x55FF97`, `0x5607CA` |
//! | 9 | `+0x78` | [`cities_needed_to_support_army`](GeneralRules::cities_needed_to_support_army) | edit 1070 | `1..=1000` | `0x56A8B6`, `0x5BCA00` |
//! | 10 | `+0x7C` | [`chance_of_rioting`](GeneralRules::chance_of_rioting) | edit 1006 | `0..=100` | `0x4BE0B0` |
//! | 11 | `+0x80` | [`draft_turn_penalty`](GeneralRules::draft_turn_penalty) | edit 1067 | `0..=1000` | `0x4B4FEA`, `0x4BD028` |
//! | 12 | `+0x84` | [`shield_cost_per_gold`](GeneralRules::shield_cost_per_gold) | edit 1088 | `1..=1000` | `0x4B0AFE`, `0x4B3301` |
//! | 13 | `+0x88` | [`fortress_defense_bonus`](GeneralRules::fortress_defense_bonus) | edit 1094 | `0..=1000` | `0x56CF97` |
//! | 14 | `+0x8C` | [`citizens_per_happy_face`](GeneralRules::citizens_per_happy_face) | edit 1024 | `1..=1000` | `0x4BD0D9` |
//! | 15 | `+0x90` | [`unused_0x90`](GeneralRules::unused_0x90) | none | | none |
//! | 16 | `+0x94` | [`unused_0x94`](GeneralRules::unused_0x94) | none | | none |
//! | 17 | `+0x98` | [`forest_value_in_shields`](GeneralRules::forest_value_in_shields) | edit 1056 | `0..=1000` | `0x461AA1` |
//! | 18 | `+0x9C` | [`hurry_shield_value_in_gold`](GeneralRules::hurry_shield_value_in_gold) | edit 1079 | `1..=1000` | `0x4B54E4`, `0x4B5D12` |
//! | 19 | `+0xA0` | [`hurry_citizen_value_in_shields`](GeneralRules::hurry_citizen_value_in_shields) | edit 1082 | `1..=1000` | `0x4B5245`, `0x4B5A2A` |
//! | 20 | `+0xA4` | [`default_difficulty`](GeneralRules::default_difficulty) | combo 1611 | a `DIFF` row | `0x567E06`, `0x567E80` |
//! | 21 | `+0xA8` | [`battle_created_unit`](GeneralRules::battle_created_unit) | combo 1059 | a ground unit | `0x445DBB`, `0x561C21`, `0x5BF54D` |
//! | 22 | `+0xAC` | [`build_army_unit`](GeneralRules::build_army_unit) | combo 1063 | a ground unit | `0x5BCA3A` |
//! | 23 | `+0xB0` | [`building_defense_rating`](GeneralRules::building_defense_rating) | edit 1117 | `0..=1000` | `0x4A269A` |
//! | 24 | `+0xB4` | [`citizen_defense_rating`](GeneralRules::citizen_defense_rating) | edit 1114 | `0..=1000` | `0x4A28BB` |
//! | 25 | `+0xB8` | [`default_money_resource`](GeneralRules::default_money_resource) | combo 1613 | a `GOOD` row | `0x55C1C7`, `0x55C1F9` |
//! | 26 | `+0xBC` | [`intercept_air_missions_chance`](GeneralRules::intercept_air_missions_chance) | edit 1077 | `0..=100` | `0x5C695B`, `0x5C70B7` |
//! | 27 | `+0xC0` | [`intercept_stealth_missions_chance`](GeneralRules::intercept_stealth_missions_chance) | edit 1101 | `0..=100` | `0x5C6953`, `0x5C70F1` |
//! | 28 | `+0xC8` | [`starting_treasury`](GeneralRules::starting_treasury) | edit 1053 | `0..=1_000_000` | `0x567E2C`, `0x569167` |
//! | 29 | `+0xCC` | [`unused_0xcc`](GeneralRules::unused_0xcc) | none | | none |
//! | 30 | `+0xD0` | [`food_per_citizen`](GeneralRules::food_per_citizen) | edit 1035 | `1..=100` | 16, e.g. `0x4ADB94` |
//! | 31 | `+0xD4` | [`river_defense_bonus`](GeneralRules::river_defense_bonus) | edit 1097 | `0..=1000` | `0x436550`, `0x56CEA3` |
//! | 32 | `+0xD8` | [`hurry_sacrifice_turn_penalty`](GeneralRules::hurry_sacrifice_turn_penalty) | edit 1027 | `0..=1000` | `0x43437D`, `0x4B5FCB` |
//! | 33 | `+0xDC` | [`scout_unit`](GeneralRules::scout_unit) | combo 1065 | a ground unit | `0x56872C` |
//! | 34 | `+0xE0` | [`captured_unit`](GeneralRules::captured_unit) | combo 1043 | a `PRTO` row | `0x43D2AD`, `0x5633DF`, `0x5B7B8B` |
//! | 35 | `+0xE4` | [`road_movement_rate`](GeneralRules::road_movement_rate) | edit 1032 | `1..=1000` | 39, e.g. `0x424DF8` |
//! | 36 | `+0xE8` | [`start_unit_1`](GeneralRules::start_unit_1) | combo 1045 | a ground unit | `0x4AE9D1`, `0x55BC30` |
//! | 37 | `+0xEC` | [`start_unit_2`](GeneralRules::start_unit_2) | combo 1047 | a ground unit | `0x4AEA1A`, `0x5687A5` |
//! | 38 | `+0xF0` | [`wltk_min_population`](GeneralRules::wltk_min_population) | edit 1008 | `0..=1000` | `0x4BE446` |
//! | 39 | `+0xF4` | [`town_defense_bonus`](GeneralRules::town_defense_bonus) | edit 1012 | `0..=1000` | `0x56CF4E` (indexed by level) |
//! | 40 | `+0xF8` | [`city_defense_bonus`](GeneralRules::city_defense_bonus) | edit 1015 | `0..=1000` | indexed by level |
//! | 41 | `+0xFC` | [`metropolis_defense_bonus`](GeneralRules::metropolis_defense_bonus) | edit 1091 | `0..=1000` | indexed by level |
//! | 42 | `+0x100` | [`town_max_size`](GeneralRules::town_max_size) | edit 1131 | `0..=1000` | 35, e.g. `0x4123EE` |
//! | 43 | `+0x104` | [`city_max_size`](GeneralRules::city_max_size) | edit 1133 | `0..=1000` | 37, e.g. `0x4123D0` |
//! | 44 | `+0x108` | [`metropolis_max_size`](GeneralRules::metropolis_max_size) | none; the editor stores `1000` | | none |
//! | 45 | `+0x10C` | [`fortification_defense_bonus`](GeneralRules::fortification_defense_bonus) | edit 1104 | `0..=1000` | 9, e.g. `0x42B639` |
//! | 46 | `+0x118` | level count: the length of [`culture_level_names`](GeneralRules::culture_level_names) | list 1677 | the list | `0x5E72D0` |
//! | 47 | list at `+0x110` | [`culture_level_names`](GeneralRules::culture_level_names) (`count` × 64 bytes) | list 1677 | 63 chars each | `0x4DCF72` |
//! | 48 | `+0x114` | [`culture_level_multiplier`](GeneralRules::culture_level_multiplier) | edit 1679 | `1..=1_000_000` | `0x5E72D0` |
//! | 49 | `+0x11C` | [`border_factor`](GeneralRules::border_factor) | edit 1682 | `1..=100` | `0x419899`, `0x4B0C6C` |
//! | 50 | `+0x120` | [`future_tech_cost`](GeneralRules::future_tech_cost) | edit 1039 | `0..=1000` | `0x569C7D`, `0x569CBD` |
//! | 51 | `+0x124` | [`golden_age_duration`](GeneralRules::golden_age_duration) | edit 1108 | `0..=100` | `0x55C8C0` |
//! | 52 | `+0x128` | [`max_research_time`](GeneralRules::max_research_time) | edit 1085 | `1..=1000` | `0x569F1F`, `0x569FB8` |
//! | 53 | `+0x12C` | [`min_research_time`](GeneralRules::min_research_time) | edit 1049 | `1..=1000` | 7, e.g. `0x4355AF` |
//! | 54 | `+0x130` | [`flag_unit`](GeneralRules::flag_unit) | combo 1852 | any unit | `0x4AE5B5`, `0x5686A4`, `0x56914D` |
//! | 55 | `+0x134` | [`upgrade_cost`](GeneralRules::upgrade_cost) | edit 1129 | `0..=1000` | `0x5C0545` |
//!
//! So a row is `288 + 4 × parts + 64 × levels` bytes, plus 4 for the flag unit
//! and 4 for the upgrade cost: **712** bytes in Civ 3 1.x (`4.01`, ten parts,
//! six levels), **716** in Play the World (`11.18`), **720** in Conquests, and
//! **684** in the two Conquests scenarios that define one part type instead of
//! ten.
//!
//! Every binding in the table comes from the editor's DDX table and its two
//! page routines (control → slot → row offset), not from the order of the
//! controls on the page; every consumer address is an absolute load of
//! `0x9C71E4 + offset` in the game. Unit-valued fields are `PRTO` indices and
//! `-1` is "none": the editor's combos show "None" first, the apply routine
//! turns the selection back into the index (`0x41F080` reads the item data).
//! Three of them, `captured_unit`, `build_army_unit` and
//! `default_money_resource`, are stored as *selection − 1*.
//!
//! # Version history
//!
//! The reader tests lengths, never versions, but the editor's release notes
//! date the two trailing fields: *Civ3XEdit 2.07 = file version 11.07* "added a
//! default flag unit to General Settings" ([`FLAG_UNIT_SINCE`]); the upgrade
//! cost arrived with Conquests, whose files are `12.x` ([`UPGRADE_COST_SINCE`];
//! the corpus has 11.18 rows without it and 12.06 rows with it, so the exact
//! step is unobserved). The writer follows those two thresholds.
//!
//! # Unused dwords
//!
//! `+0x90`, `+0x94` and `+0xCC` are read and written but no code touches them
//! (no absolute load of `0x9C7274`, `0x9C7278` or `0x9C72B0` in the game, no
//! control in the editor). They hold `50`, `2` and `16` in every shipped file.
//! `+0x108` is the third city size's maximum, which the editor does not show
//! and re-stores as `1000` whenever the page is applied.

use crate::Version;
use crate::io::{Ctx, Error, Field, Reader, Record, Result, Str, Writer};

/// The file version that added [`GeneralRules::flag_unit`] (Civ3XEdit 2.07).
pub const FLAG_UNIT_SINCE: Version = Version::new(11, 7);

/// The first Conquests version, which has [`GeneralRules::upgrade_cost`].
pub const UPGRADE_COST_SINCE: Version = Version::new(12, 0);

/// The most spaceship part types a row can hold (the reader clamps the count).
pub const MAX_SPACESHIP_PARTS: usize = 100;

/// The `RULE` row. See the [module documentation](self) for the layout; unit
/// fields are `PRTO` indices with `-1` for none.
#[derive(Clone, Debug, PartialEq)]
pub struct GeneralRules {
    /// Name of the smallest city size level, shown as "Town" (*Level 1*).
    pub town_name: Str<32>,
    /// Name of the middle level ("City").
    pub city_name: Str<32>,
    /// Name of the largest level ("Metropolis").
    pub metropolis_name: Str<32>,
    /// How many of each spaceship part type a complete ship needs; its length
    /// is the number of part types (*Spaceship Parts*, `10` in the stock
    /// rules). Entry `i` belongs to the improvements whose
    /// [`spaceship_part`](crate::sections::bldg::Building::spaceship_part) is
    /// `i`, and is also how many of them can be built (`0x56A581`). With ten
    /// types of one each the game offers its standard spaceship screen
    /// (`0x5A34BF`..`0x5A34E9`). A part type needing `0` can never be built.
    pub spaceship_parts_needed: Vec<i32>,
    /// *Advanced Barbarian*: the stronger land unit barbarians field (Horseman).
    pub advanced_barbarian_unit: i32,
    /// *Basic Barbarian*: the land unit barbarians begin with (Warrior).
    pub basic_barbarian_unit: i32,
    /// *Barbarian Sea Unit*: a sea unit (Galley).
    pub barbarian_sea_unit: i32,
    /// *Cities Needed to Support an Army*: stock `4`.
    pub cities_needed_to_support_army: i32,
    /// *Chance of Rioting*, percent of a city in disorder; stock `20`.
    pub chance_of_rioting: i32,
    /// *Turn Penalty for Each Drafted Citizen*: how many turns the mood effect
    /// of a draft lasts; stock `20`.
    pub draft_turn_penalty: i32,
    /// *Shield Cost Per Gold*: shields it takes to make one gold with
    /// *Wealth*; stock `4` in Conquests, `8` in Civ 3 1.x and Play the World.
    pub shield_cost_per_gold: i32,
    /// *Fortress* defence bonus, percent; stock `50`.
    pub fortress_defense_bonus: i32,
    /// *Citizens Affected by Each Happy Face*; stock `1`.
    pub citizens_per_happy_face: i32,
    /// Read and written, never used by either program; `50` in every file.
    pub unused_0x90: i32,
    /// Read and written, never used by either program; `2` in every file.
    pub unused_0x94: i32,
    /// *Forest Value in Shields*: shields a worker's *Clear Forest* gives the
    /// nearest city; stock `10`.
    pub forest_value_in_shields: i32,
    /// *Shield Value in Gold* (hurry production): gold per shield; stock `4`.
    pub hurry_shield_value_in_gold: i32,
    /// *Citizen Value in Shields* (hurry production): shields a sacrificed
    /// citizen is worth; stock `20`.
    pub hurry_citizen_value_in_shields: i32,
    /// *Default Difficulty Level*: a `DIFF` index (`2`, Regent).
    pub default_difficulty: i32,
    /// *Battle-Created Unit*: what an elite victory can spawn (the Leader).
    pub battle_created_unit: i32,
    /// *Build-Army Unit*: what a Leader (or the Military Academy) creates (the
    /// Army).
    pub build_army_unit: i32,
    /// *Building* defence rating (`0..=1000`), used only to decide which
    /// buildings a bombardment destroys: a unit with that bombard strength has
    /// a 50 % chance. `16` in every file (the help page says `4`, its Play the
    /// World value).
    pub building_defense_rating: i32,
    /// *Citizen* defence rating: the same for population losses; `16`.
    pub citizen_defense_rating: i32,
    /// *Default Money Resource*: a `GOOD` index, `-1` none. A goody hut places
    /// that resource on its tile (`0x55C1C7`).
    pub default_money_resource: i32,
    /// *Chance to Intercept Enemy Air Missions*, percent; stock `50`.
    pub intercept_air_missions_chance: i32,
    /// *Chance to Intercept Enemy Stealth Missions*, percent; stock `5`.
    pub intercept_stealth_missions_chance: i32,
    /// *Starting Treasury* of every player that has no treasury of its own
    /// (`LEAD`'s field wins); stock `10`. The setter at `0x569167` adds ten
    /// times this value to the player's gold.
    pub starting_treasury: i32,
    /// Read and written, never used by either program; `16` in every file.
    pub unused_0xcc: i32,
    /// *Food Consumption/Citizen*; stock `2`.
    pub food_per_citizen: i32,
    /// *River* defence bonus, percent; stock `25`.
    pub river_defense_bonus: i32,
    /// *Turn Penalty for Each Hurry Sacrifice*: turns a sacrifice's mood
    /// effect lasts; stock `20`.
    pub hurry_sacrifice_turn_penalty: i32,
    /// *Scout*: the free unit an expansionistic civilization starts with.
    pub scout_unit: i32,
    /// *Captured Unit*: the unit the game creates when a city's workers are
    /// captured (`0x5633DF`, `0x5B7B8B`; Worker).
    pub captured_unit: i32,
    /// *Movement Rate Along Roads*, in thirds of a move; stock `3`.
    pub road_movement_rate: i32,
    /// *Start Unit 1*: the first free unit of every civilization (Settlers).
    pub start_unit_1: i32,
    /// *Start Unit 2*: the second (Worker).
    pub start_unit_2: i32,
    /// *Minimum Population for We Love the King Day*; stock `6`.
    pub wltk_min_population: i32,
    /// *Town* (level 1 city) defence bonus, percent; stock `0`.
    pub town_defense_bonus: i32,
    /// *City* (level 2) defence bonus, percent; stock `50`.
    pub city_defense_bonus: i32,
    /// *Metropolis* (level 3) defence bonus, percent; stock `100`.
    pub metropolis_defense_bonus: i32,
    /// *Maximum Size* of a town: a city of at most this many citizens is level
    /// 1 (stock `6`; `15` and `7` in some scenarios).
    pub town_max_size: i32,
    /// *Maximum Size* of a city: level 2 up to here, level 3 above (stock `12`).
    pub city_max_size: i32,
    /// The third level's maximum: `1000` in every file, not editable.
    pub metropolis_max_size: i32,
    /// *Fortifications* defence bonus, percent; stock `25`.
    pub fortification_defense_bonus: i32,
    /// Names of the culture levels, weakest first (*Cultural Levels*): in the
    /// stock rules *Fledgling*, *Weak*, *Fragile*, *Solid*, *Strong*,
    /// *Glorious*.
    pub culture_level_names: Vec<Str<64>>,
    /// *Lvl. Multiplier*: level `k` ends at this many culture points times
    /// `2^k` (see [`GeneralRules::culture_level`]); stock `1000`.
    pub culture_level_multiplier: i32,
    /// *Border Factor*: how fast borders grow with culture, lower is faster;
    /// stock `10`.
    pub border_factor: i32,
    /// *Future Tech Cost*; stock `400`.
    pub future_tech_cost: i32,
    /// *Duration* of golden ages in turns; `0` disables them; stock `20`.
    pub golden_age_duration: i32,
    /// *Maximum Research Time* in turns; stock `40`.
    pub max_research_time: i32,
    /// *Minimum Research Time* in turns; stock `4`.
    pub min_research_time: i32,
    /// *Flag Unit*: the unit captured in capture-the-flag games (Princess).
    /// Absent before file version 11.07.
    pub flag_unit: i32,
    /// *Upgrade Cost*: multiple of the shield difference between a unit and
    /// its upgrade that upgrading costs in gold; stock `3`. Absent before
    /// Conquests.
    pub upgrade_cost: i32,
    /// Bytes after the last known field (empty in every shipped file).
    pub extra: Vec<u8>,
}

impl Default for GeneralRules {
    /// The object as the game's initialiser (`0x5E7020`) leaves it: the values
    /// a row from an older file keeps for the fields it lacks. Units and the
    /// money resource are `-1`; most scalars are the lowest value the editor
    /// allows.
    fn default() -> Self {
        GeneralRules {
            town_name: Str::default(),
            city_name: Str::default(),
            metropolis_name: Str::default(),
            spaceship_parts_needed: Vec::new(),
            advanced_barbarian_unit: -1,
            basic_barbarian_unit: -1,
            barbarian_sea_unit: -1,
            cities_needed_to_support_army: 1,
            chance_of_rioting: 0,
            draft_turn_penalty: 0,
            shield_cost_per_gold: 1,
            fortress_defense_bonus: 0,
            citizens_per_happy_face: 1,
            unused_0x90: 0,
            unused_0x94: 0,
            forest_value_in_shields: 0,
            hurry_shield_value_in_gold: 1,
            hurry_citizen_value_in_shields: 1,
            default_difficulty: 0,
            battle_created_unit: -1,
            build_army_unit: -1,
            building_defense_rating: 0,
            citizen_defense_rating: 0,
            default_money_resource: -1,
            intercept_air_missions_chance: 0,
            intercept_stealth_missions_chance: 0,
            starting_treasury: 0,
            unused_0xcc: 0,
            food_per_citizen: 1,
            river_defense_bonus: 0,
            hurry_sacrifice_turn_penalty: 0,
            scout_unit: -1,
            captured_unit: -1,
            road_movement_rate: 1,
            start_unit_1: -1,
            start_unit_2: -1,
            wltk_min_population: 0,
            town_defense_bonus: 0,
            city_defense_bonus: 0,
            metropolis_defense_bonus: 0,
            town_max_size: 0,
            city_max_size: 0,
            metropolis_max_size: 0,
            fortification_defense_bonus: 0,
            culture_level_names: Vec::new(),
            culture_level_multiplier: 1,
            border_factor: 1,
            future_tech_cost: 0,
            golden_age_duration: 0,
            max_research_time: 1,
            min_research_time: 1,
            flag_unit: -1,
            upgrade_cost: 1,
            extra: Vec::new(),
        }
    }
}

impl GeneralRules {
    /// The culture level of a civilization with `points` culture points
    /// (`0x5E72D0`): the first level `k` with `points <= multiplier << k`, the
    /// last level if there is none, `0` for a scenario without levels. With
    /// the stock multiplier the levels end at 1000, 2000, 4000, ... 32000.
    pub fn culture_level(&self, points: i32) -> usize {
        let levels = self.culture_level_names.len();
        for k in 0..levels {
            let end = self
                .culture_level_multiplier
                .wrapping_mul(1i32.wrapping_shl(k as u32));
            if points <= end {
                return k;
            }
        }
        levels.saturating_sub(1)
    }
}

impl Record for GeneralRules {
    const TAG: [u8; 4] = *b"RULE";

    fn read(r: &mut Reader<'_>, ctx: &Ctx) -> Result<Self> {
        let mut g = GeneralRules::default();
        // A field is read only if every byte of it is still in the row; the
        // first one that is not ends the row (`0x5E78E0`).
        'fields: {
            macro_rules! dwords {
                ($($f:ident),+ $(,)?) => {$(
                    let Some(v) = r.i32() else { break 'fields };
                    g.$f = v;
                )+};
            }
            if r.remaining() < 3 * <Str<32> as Field>::SIZE {
                break 'fields;
            }
            g.town_name = Str::read(r);
            g.city_name = Str::read(r);
            g.metropolis_name = Str::read(r);

            let Some(parts) = r.i32() else { break 'fields };
            if !(0..=MAX_SPACESHIP_PARTS as i32).contains(&parts)
                || parts as usize * 4 > r.remaining()
            {
                return Err(Error::BadCount {
                    tag: Self::TAG,
                    what: "spaceship parts",
                    count: parts as u32,
                });
            }
            g.spaceship_parts_needed = (0..parts).map(|_| i32::read(r)).collect();

            dwords!(
                advanced_barbarian_unit,
                basic_barbarian_unit,
                barbarian_sea_unit,
                cities_needed_to_support_army,
                chance_of_rioting,
                draft_turn_penalty,
                shield_cost_per_gold,
                fortress_defense_bonus,
                citizens_per_happy_face,
                unused_0x90,
                unused_0x94,
                forest_value_in_shields,
                hurry_shield_value_in_gold,
                hurry_citizen_value_in_shields,
                default_difficulty,
                battle_created_unit,
                build_army_unit,
                building_defense_rating,
                citizen_defense_rating,
                default_money_resource,
                intercept_air_missions_chance,
                intercept_stealth_missions_chance,
                starting_treasury,
                unused_0xcc,
                food_per_citizen,
                river_defense_bonus,
                hurry_sacrifice_turn_penalty,
                scout_unit,
                captured_unit,
                road_movement_rate,
                start_unit_1,
                start_unit_2,
                wltk_min_population,
                town_defense_bonus,
                city_defense_bonus,
                metropolis_defense_bonus,
                town_max_size,
                city_max_size,
                metropolis_max_size,
                fortification_defense_bonus,
            );

            let Some(levels) = r.i32() else { break 'fields };
            if levels < 0
                || (levels as usize)
                    .checked_mul(<Str<64> as Field>::SIZE)
                    .is_none_or(|bytes| bytes > r.remaining())
            {
                return Err(Error::BadCount {
                    tag: Self::TAG,
                    what: "culture levels",
                    count: levels as u32,
                });
            }
            g.culture_level_names = (0..levels).map(|_| Str::read(r)).collect();

            dwords!(
                culture_level_multiplier,
                border_factor,
                future_tech_cost,
                golden_age_duration,
                max_research_time,
                min_research_time,
            );
            // The last two fields are taken only from files new enough to have them; bytes
            // beyond that stay in `extra` and are written back unchanged.
            if ctx.version < FLAG_UNIT_SINCE {
                break 'fields;
            }
            dwords!(flag_unit);
            if ctx.version < UPGRADE_COST_SINCE {
                break 'fields;
            }
            dwords!(upgrade_cost);
        }
        g.extra = r.rest().to_vec();
        Ok(g)
    }

    fn write(&self, w: &mut Writer, ctx: &Ctx) {
        self.town_name.write(w);
        self.city_name.write(w);
        self.metropolis_name.write(w);
        w.i32(self.spaceship_parts_needed.len() as i32);
        for &n in &self.spaceship_parts_needed {
            w.i32(n);
        }
        for v in [
            self.advanced_barbarian_unit,
            self.basic_barbarian_unit,
            self.barbarian_sea_unit,
            self.cities_needed_to_support_army,
            self.chance_of_rioting,
            self.draft_turn_penalty,
            self.shield_cost_per_gold,
            self.fortress_defense_bonus,
            self.citizens_per_happy_face,
            self.unused_0x90,
            self.unused_0x94,
            self.forest_value_in_shields,
            self.hurry_shield_value_in_gold,
            self.hurry_citizen_value_in_shields,
            self.default_difficulty,
            self.battle_created_unit,
            self.build_army_unit,
            self.building_defense_rating,
            self.citizen_defense_rating,
            self.default_money_resource,
            self.intercept_air_missions_chance,
            self.intercept_stealth_missions_chance,
            self.starting_treasury,
            self.unused_0xcc,
            self.food_per_citizen,
            self.river_defense_bonus,
            self.hurry_sacrifice_turn_penalty,
            self.scout_unit,
            self.captured_unit,
            self.road_movement_rate,
            self.start_unit_1,
            self.start_unit_2,
            self.wltk_min_population,
            self.town_defense_bonus,
            self.city_defense_bonus,
            self.metropolis_defense_bonus,
            self.town_max_size,
            self.city_max_size,
            self.metropolis_max_size,
            self.fortification_defense_bonus,
        ] {
            w.i32(v);
        }
        w.i32(self.culture_level_names.len() as i32);
        for name in &self.culture_level_names {
            name.write(w);
        }
        for v in [
            self.culture_level_multiplier,
            self.border_factor,
            self.future_tech_cost,
            self.golden_age_duration,
            self.max_research_time,
            self.min_research_time,
        ] {
            w.i32(v);
        }
        if ctx.version >= FLAG_UNIT_SINCE {
            w.i32(self.flag_unit);
        }
        if ctx.version >= UPGRADE_COST_SINCE {
            w.i32(self.upgrade_cost);
        }
        w.bytes(&self.extra);
    }

    fn extra(&self) -> &[u8] {
        &self.extra
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::{self, files};
    use crate::sections::prto::{UnitType, class};

    /// The one `RULE` row of a corpus file, if it has one.
    fn rules_of(f: &corpus::CorpusFile) -> Option<GeneralRules> {
        let sec = f.raw.section(b"RULE")?;
        let row = sec.rows.first()?;
        Some(GeneralRules::read(&mut Reader::new(f.raw.row(row)), &f.ctx()).unwrap())
    }

    fn rules_in(suffix: &str) -> Option<GeneralRules> {
        files()
            .iter()
            .find(|f| f.name().ends_with(suffix))
            .and_then(rules_of)
    }

    #[test]
    fn corpus_roundtrip() {
        let st = corpus::check_roundtrip::<GeneralRules>();
        assert_eq!(st.with_extra, 0, "{}", st.report());
        if files().is_empty() {
            return;
        }
        assert_eq!(st.lengths, vec![684, 712, 716, 720], "{}", st.report());
        // Civ 3 1.x: no flag unit, no upgrade cost; Play the World: flag unit;
        // Conquests: both, and two scenarios with a single part type.
        assert_eq!(st.by_version[&Version::new(4, 1)], vec![712]);
        assert_eq!(st.by_version[&Version::new(11, 18)], vec![716]);
        assert_eq!(st.by_version[&Version::new(12, 6)], vec![684, 720]);
    }

    /// The editor stores each control clamped to the ranges in the module
    /// table. A field at the wrong offset breaks this at once.
    #[test]
    fn editor_ranges_hold_in_every_file() {
        let mut rows = 0;
        for f in files() {
            let Some(g) = rules_of(&f) else { continue };
            let name = f.name();
            let in_range = |v: i32, lo: i32, hi: i32| (lo..=hi).contains(&v);
            let checks: [(&str, i32, i32, i32); 29] = [
                (
                    "cities_needed_to_support_army",
                    g.cities_needed_to_support_army,
                    1,
                    1000,
                ),
                ("chance_of_rioting", g.chance_of_rioting, 0, 100),
                ("draft_turn_penalty", g.draft_turn_penalty, 0, 1000),
                ("shield_cost_per_gold", g.shield_cost_per_gold, 1, 1000),
                ("fortress_defense_bonus", g.fortress_defense_bonus, 0, 1000),
                (
                    "citizens_per_happy_face",
                    g.citizens_per_happy_face,
                    1,
                    1000,
                ),
                (
                    "forest_value_in_shields",
                    g.forest_value_in_shields,
                    0,
                    1000,
                ),
                (
                    "hurry_shield_value_in_gold",
                    g.hurry_shield_value_in_gold,
                    1,
                    1000,
                ),
                (
                    "hurry_citizen_value_in_shields",
                    g.hurry_citizen_value_in_shields,
                    1,
                    1000,
                ),
                (
                    "building_defense_rating",
                    g.building_defense_rating,
                    0,
                    1000,
                ),
                ("citizen_defense_rating", g.citizen_defense_rating, 0, 1000),
                (
                    "intercept_air_missions_chance",
                    g.intercept_air_missions_chance,
                    0,
                    100,
                ),
                (
                    "intercept_stealth_missions_chance",
                    g.intercept_stealth_missions_chance,
                    0,
                    100,
                ),
                ("starting_treasury", g.starting_treasury, 0, 1_000_000),
                ("food_per_citizen", g.food_per_citizen, 1, 100),
                ("river_defense_bonus", g.river_defense_bonus, 0, 1000),
                (
                    "hurry_sacrifice_turn_penalty",
                    g.hurry_sacrifice_turn_penalty,
                    0,
                    1000,
                ),
                ("road_movement_rate", g.road_movement_rate, 1, 1000),
                ("wltk_min_population", g.wltk_min_population, 0, 1000),
                ("town_defense_bonus", g.town_defense_bonus, 0, 1000),
                ("city_defense_bonus", g.city_defense_bonus, 0, 1000),
                (
                    "metropolis_defense_bonus",
                    g.metropolis_defense_bonus,
                    0,
                    1000,
                ),
                ("town_max_size", g.town_max_size, 0, 1000),
                ("city_max_size", g.city_max_size, 0, 1000),
                (
                    "fortification_defense_bonus",
                    g.fortification_defense_bonus,
                    0,
                    1000,
                ),
                (
                    "culture_level_multiplier",
                    g.culture_level_multiplier,
                    1,
                    1_000_000,
                ),
                ("border_factor", g.border_factor, 1, 100),
                ("future_tech_cost", g.future_tech_cost, 0, 1000),
                ("golden_age_duration", g.golden_age_duration, 0, 100),
            ];
            for (field, v, lo, hi) in checks {
                assert!(in_range(v, lo, hi), "{name}: {field} = {v}");
            }
            assert!(in_range(g.max_research_time, 1, 1000), "{name}");
            assert!(in_range(g.min_research_time, 1, 1000), "{name}");
            assert!(in_range(g.upgrade_cost, 0, 1000), "{name}");
            assert!(g.min_research_time <= g.max_research_time, "{name}");
            assert!(
                g.spaceship_parts_needed
                    .iter()
                    .all(|&n| in_range(n, 0, 100)),
                "{name}"
            );
            assert!(g.culture_level_names.len() <= 6, "{name}");
            // The editor re-stores the third level's maximum as 1000.
            assert_eq!(g.metropolis_max_size, 1000, "{name}");
            // The levels must be ordered or the city size classes make no sense.
            assert!(g.town_max_size <= g.city_max_size, "{name}");
            rows += 1;
        }
        if !files().is_empty() {
            assert!(rows >= 30, "{rows} rows");
        }
    }

    /// The three dwords nothing uses are the same in every file.
    #[test]
    fn unused_dwords_are_constant() {
        for f in files() {
            let Some(g) = rules_of(&f) else { continue };
            assert_eq!(
                (g.unused_0x90, g.unused_0x94, g.unused_0xcc),
                (50, 2, 16),
                "{}",
                f.name()
            );
        }
    }

    /// Conquests' own scenario, field by field against the editor's help pages
    /// ("Default is ...").
    #[test]
    fn conquests_stock_values() {
        let Some(g) = rules_in("Conquests/conquests.biq") else {
            return;
        };
        assert_eq!(g.town_name.text(), "Town");
        assert_eq!(g.city_name.text(), "City");
        assert_eq!(g.metropolis_name.text(), "Metropolis");
        assert_eq!(g.spaceship_parts_needed, [1; 10]);
        assert_eq!(g.cities_needed_to_support_army, 4);
        assert_eq!(g.chance_of_rioting, 20);
        assert_eq!(g.draft_turn_penalty, 20);
        assert_eq!(g.hurry_sacrifice_turn_penalty, 20);
        assert_eq!(g.shield_cost_per_gold, 4);
        assert_eq!(g.fortress_defense_bonus, 50);
        assert_eq!(g.river_defense_bonus, 25);
        assert_eq!(g.fortification_defense_bonus, 25);
        assert_eq!(
            (
                g.town_defense_bonus,
                g.city_defense_bonus,
                g.metropolis_defense_bonus
            ),
            (0, 50, 100)
        );
        assert_eq!(g.citizens_per_happy_face, 1);
        assert_eq!(g.forest_value_in_shields, 10);
        assert_eq!(g.hurry_shield_value_in_gold, 4);
        assert_eq!(g.hurry_citizen_value_in_shields, 20);
        assert_eq!(g.default_difficulty, 2);
        assert_eq!(g.default_money_resource, -1);
        assert_eq!(g.intercept_air_missions_chance, 50);
        assert_eq!(g.intercept_stealth_missions_chance, 5);
        assert_eq!(g.starting_treasury, 10);
        assert_eq!(g.food_per_citizen, 2);
        assert_eq!(g.road_movement_rate, 3);
        assert_eq!((g.start_unit_1, g.start_unit_2), (0, 1));
        assert_eq!(g.wltk_min_population, 6);
        assert_eq!((g.town_max_size, g.city_max_size), (6, 12));
        let names: Vec<_> = g.culture_level_names.iter().map(|n| n.text()).collect();
        assert_eq!(
            names,
            [
                "Fledgling",
                "Weak",
                "Fragile",
                "Solid",
                "Strong",
                "Glorious"
            ]
        );
        assert_eq!(g.culture_level_multiplier, 1000);
        assert_eq!(g.border_factor, 10);
        assert_eq!(g.future_tech_cost, 400);
        assert_eq!(g.golden_age_duration, 20);
        assert_eq!((g.min_research_time, g.max_research_time), (4, 50));
        assert_eq!(g.upgrade_cost, 3);
    }

    /// Civ 3 1.x doubles the wealth price and has no flag unit or upgrade
    /// cost: those fields keep the initialiser's values.
    #[test]
    fn civ3_1x_row_is_a_prefix() {
        let Some(g) = rules_in("civ3mod.bic") else {
            return;
        };
        assert_eq!(g.shield_cost_per_gold, 8);
        assert_eq!(g.future_tech_cost, 400);
        assert_eq!(g.flag_unit, -1);
        assert_eq!(g.upgrade_cost, 1);
        let mut w = Writer::new();
        g.write(
            &mut w,
            &Ctx {
                version: Version::new(4, 1),
            },
        );
        assert_eq!(w.buf.len(), 712);
    }

    /// The two Conquests scenarios with one part type are the 684-byte rows.
    #[test]
    fn single_part_rows_are_684_bytes() {
        let mut seen = 0;
        for f in files() {
            let Some(sec) = f.raw.section(b"RULE") else {
                continue;
            };
            let len = f.raw.row(&sec.rows[0]).len();
            let g = rules_of(&f).unwrap();
            // The module's length formula.
            let optional = 4 * usize::from(f.version >= FLAG_UNIT_SINCE)
                + 4 * usize::from(f.version >= UPGRADE_COST_SINCE);
            assert_eq!(
                len,
                288 + 4 * g.spaceship_parts_needed.len()
                    + 64 * g.culture_level_names.len()
                    + optional,
                "{}",
                f.name()
            );
            if g.spaceship_parts_needed.len() == 1 {
                assert_eq!(len, 684);
                assert_eq!(g.spaceship_parts_needed, [0]);
                seen += 1;
            }
        }
        if !files().is_empty() {
            assert_eq!(seen, 2);
        }
    }

    /// Row length against the module's formula at each version step.
    #[test]
    fn writer_follows_the_version_history() {
        let g = GeneralRules {
            spaceship_parts_needed: vec![1; 10],
            culture_level_names: vec![Str::new("x"); 6],
            ..GeneralRules::default()
        };
        let len = |maj, min| {
            let mut w = Writer::new();
            g.write(
                &mut w,
                &Ctx {
                    version: Version::new(maj, min),
                },
            );
            w.buf.len()
        };
        assert_eq!(len(4, 1), 712);
        assert_eq!(len(11, 6), 712);
        assert_eq!(len(11, 7), 716);
        assert_eq!(len(11, 18), 716);
        assert_eq!(len(12, 0), 720);
        assert_eq!(len(12, 8), 720);
    }

    /// A row shorter than the layout leaves the later fields at the
    /// initialiser's values, as the game's reader does.
    #[test]
    fn short_rows_keep_initialiser_defaults() {
        let mut full = GeneralRules {
            spaceship_parts_needed: vec![1; 3],
            culture_level_names: vec![Str::new("a"), Str::new("b")],
            ..GeneralRules::default()
        };
        full.upgrade_cost = 9;
        full.flag_unit = 5;
        let ctx = Ctx {
            version: Version::new(12, 8),
        };
        let mut w = Writer::new();
        full.write(&mut w, &ctx);
        let cut = &w.buf[..w.buf.len() - 4];
        let g = GeneralRules::read(&mut Reader::new(cut), &ctx).unwrap();
        assert_eq!(g.flag_unit, 5);
        assert_eq!(g.upgrade_cost, 1);
        // Cut inside the label block: nothing is read at all.
        let g = GeneralRules::read(&mut Reader::new(&w.buf[..90]), &ctx).unwrap();
        assert_eq!(g.extra.len(), 90);
        assert_eq!(
            GeneralRules {
                extra: Vec::new(),
                ..g
            },
            GeneralRules::default()
        );
    }

    #[test]
    fn absurd_counts_are_errors() {
        let ctx = Ctx {
            version: Version::new(12, 8),
        };
        let mut body = vec![0u8; 96];
        body.extend_from_slice(&101i32.to_le_bytes());
        body.extend_from_slice(&[0u8; 800]);
        assert!(matches!(
            GeneralRules::read(&mut Reader::new(&body), &ctx),
            Err(Error::BadCount {
                what: "spaceship parts",
                ..
            })
        ));
        let mut body = vec![0u8; 96];
        body.extend_from_slice(&(-1i32).to_le_bytes());
        assert!(GeneralRules::read(&mut Reader::new(&body), &ctx).is_err());
    }

    /// `0x5E72D0`: level `k` ends at `multiplier << k`; the last level absorbs
    /// everything above; no levels means level 0.
    #[test]
    fn culture_level_thresholds() {
        let g = GeneralRules {
            culture_level_names: vec![Str::default(); 6],
            culture_level_multiplier: 1000,
            ..GeneralRules::default()
        };
        let ends = [1000, 2000, 4000, 8000, 16000, 32000];
        for (k, end) in ends.into_iter().enumerate() {
            assert_eq!(g.culture_level(end), k);
            assert_eq!(g.culture_level(end + 1), (k + 1).min(5));
        }
        assert_eq!(g.culture_level(0), 0);
        assert_eq!(g.culture_level(-5), 0);
        assert_eq!(g.culture_level(i32::MAX), 5);
        assert_eq!(GeneralRules::default().culture_level(123), 0);
    }

    /// The unit slots of the two stock scenarios name the units the help pages
    /// call the defaults. This is the strongest check that each slot is the
    /// one the editor labels.
    #[test]
    fn stock_unit_slots_name_the_documented_units() {
        for (suffix, flag) in [("Conquests/conquests.biq", "Princess"), ("civ3mod.bic", "")] {
            let Some(f) = files().into_iter().find(|f| f.name().ends_with(suffix)) else {
                continue;
            };
            let g = rules_of(&f).unwrap();
            let units: Vec<String> = f
                .raw
                .section(b"PRTO")
                .unwrap()
                .rows
                .iter()
                .map(|r| {
                    UnitType::read(&mut Reader::new(f.raw.row(r)), &f.ctx())
                        .unwrap()
                        .name
                        .text()
                        .into_owned()
                })
                .collect();
            let name = |i: i32| units.get(i as usize).map(String::as_str).unwrap_or("");
            assert_eq!(name(g.advanced_barbarian_unit), "Horseman");
            assert_eq!(name(g.basic_barbarian_unit), "Warrior");
            assert_eq!(name(g.barbarian_sea_unit), "Galley");
            assert_eq!(name(g.battle_created_unit), "Leader");
            assert_eq!(name(g.build_army_unit), "Army");
            assert_eq!(name(g.scout_unit), "Scout");
            assert_eq!(name(g.captured_unit), "Worker");
            assert_eq!(name(g.start_unit_1), "Settler");
            assert_eq!(name(g.start_unit_2), "Worker");
            assert_eq!(name(g.flag_unit), flag, "{suffix}");
        }
    }

    /// The unit slots really hold unit indices: each is `-1` or a row of the
    /// file's own `PRTO` section, and the editor's class filters hold (land
    /// units for the land slots, a sea unit for the barbarian ship).
    #[test]
    fn unit_slots_point_at_the_right_unit_classes() {
        let mut checked = 0;
        for f in files() {
            let Some(g) = rules_of(&f) else { continue };
            let Some(sec) = f.raw.section(b"PRTO") else {
                continue;
            };
            let units: Vec<UnitType> = sec
                .rows
                .iter()
                .map(|r| UnitType::read(&mut Reader::new(f.raw.row(r)), &f.ctx()).unwrap())
                .collect();
            let name = f.name();
            let class_of = |i: i32| units.get(i as usize).map(|u| u.unit_class);
            for (what, idx, want) in [
                (
                    "advanced_barbarian_unit",
                    g.advanced_barbarian_unit,
                    Some(class::LAND),
                ),
                (
                    "basic_barbarian_unit",
                    g.basic_barbarian_unit,
                    Some(class::LAND),
                ),
                ("barbarian_sea_unit", g.barbarian_sea_unit, Some(class::SEA)),
                ("scout_unit", g.scout_unit, Some(class::LAND)),
                (
                    "battle_created_unit",
                    g.battle_created_unit,
                    Some(class::LAND),
                ),
                ("build_army_unit", g.build_army_unit, Some(class::LAND)),
                ("start_unit_1", g.start_unit_1, Some(class::LAND)),
                ("start_unit_2", g.start_unit_2, Some(class::LAND)),
                ("captured_unit", g.captured_unit, None),
                ("flag_unit", g.flag_unit, None),
            ] {
                assert!(
                    idx >= -1 && idx < units.len() as i32,
                    "{name}: {what} = {idx}"
                );
                if let (Some(want), Some(got)) = (want, class_of(idx)) {
                    assert_eq!(got, want, "{name}: {what} = {idx}");
                }
            }
            checked += 1;
        }
        if !files().is_empty() {
            assert!(checked >= 30, "{checked}");
        }
    }
}
