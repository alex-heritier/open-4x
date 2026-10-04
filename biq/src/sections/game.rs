//! `GAME` — the scenario-wide settings row.
//!
//! Editor pages: **Scenario** (dialog 190), **Locked Alliances** (207), **Victory Point
//! Limits** (208) and **Disasters!** (209). The *Crop Map* page (210) is not stored here.
//!
//! # Where it lives in the exe
//!
//! The scenario manager (instance `0x9C3508`) embeds the game object at `+0xBC8`, which
//! is **`0x9C40D0`** — call it `G`. Every field below is addressed `G+offset` in the exe
//! and the engine reads them through absolute operands (`G+0x7C` is `[0x9C414C]`). The
//! editor keeps the same record at the same offsets (`G = S+0x5C` in the Scenario page
//! save `0x44A5B0`).
//!
//! | code | address |
//! |---|---|
//! | constructor (the defaults in [`Game::default`]) | `0x5E2450` |
//! | row reader | `0x5E26E0`, called from the section dispatcher at `0x5946D2` |
//! | row writer | `0x5E2F90`, called at `0x597951` |
//! | playable-civ list allocator | `0x5E3610` |
//!
//! The engine does not use `G` directly for the victory/rule bits; it merges them into
//! its game-state record (see [`flags`] and [`Game::engine_flags`]).
//!
//! Evidence tags in the docs: **A** exe code, **B** editor dialog/DDX/save code, **C**
//! shipped-corpus values, **E** community documentation (hypothesis only).
//!
//! # Row layout
//!
//! The reader obeys the loader's shared row rule (see `io.rs`): a field is read only if
//! its whole size is still left in the row, and whatever it does not know is skipped.
//! Different file generations therefore just end at different fields. Numbering below is
//! the on-disk order; [`Game::fields_present`] counts how many of these a row had.
//!
//! | # | `G+` | bytes | field | default |
//! |--:|---|--:|---|---|
//! | 1 | `0x00` | 4 | [`use_default_game_rules`](Game::use_default_game_rules) | 1 |
//! | 2 | `0x04` | 4 | [`use_default_victory_conditions`](Game::use_default_victory_conditions) | 1 |
//! | 3 | `0x10` | 4 + 4N | count N, then N × [`PlayableCiv::civilization`] | none |
//! | 4 | `0x14` | 4 | [`rules_flags`](Game::rules_flags) | 0 |
//! | 5–7 | `0x18 0x1C 0x20` | 4 each | auto-place capture units / king units / victory locations | 1 |
//! | 8 | `0x24` | 4 | [`debug_mode`](Game::debug_mode) | 0 |
//! | 9 | `0x28` | 4 | [`use_time_limit`](Game::use_time_limit) | 0 |
//! | 10 | `0x2C` | 4 | [`base_unit_of_time`](Game::base_unit_of_time) | 0 |
//! | 11–13 | `0x30 0x34 0x38` | 4 each | start month, start week, start year | 1, 1, −4000 |
//! | 14–15 | `0x3C 0x40` | 4 each | time limit minutes, time limit turns | 0, 540 |
//! | 16 | `0x44` | 28 | [`time_scale_turns`](Game::time_scale_turns) | 25 25 40 50 100 100 100 |
//! | 17 | `0x60` | 28 | [`time_scale_units`](Game::time_scale_units) | 50 40 25 20 10 5 2 |
//! | 18 | `0xB4` | 5200 | [`search_folders`](Game::search_folders) | empty |
//! | | | | *end of a PTW row (`5316 + 4N` bytes)* | |
//! | 19 | – | 4N | N × [`PlayableCiv::alliance`] (**version > 11.19**) | 0 |
//! | 19′ | → `0x1A71` | 4 | [`legacy_reveal_map`](Game::legacy_reveal_map) (**version ≤ 11.19**, then the row ends) | 0 |
//! | 20–31 | `0x7C`–`0xA8` | 4 each | [`VictoryLimits`] first twelve values | see there |
//! | 32 | `0xB0` | 4 | [`theme`](Game::theme) | 0 |
//! | 33 | `0x1A72` | 1 | [`runtime_flag_1a72`](Game::runtime_flag_1a72) | 0 |
//! | 34 | `0x1504` | 1280 | [`alliance_names`](Game::alliance_names) (5 × 256) | empty |
//! | 35 | `0x1A04` | 100 | [`alliance_war`](Game::alliance_war) (5 × 5 dwords) | 0 |
//! | 36 | `0x1A68` | 4 | [`alliance_victory_type`](Game::alliance_victory_type) | 0 |
//! | 37 | `0x1A73` | 260 | [`DisasterSettings::plague_name`] | `Black Death` |
//! | 38 | `0x1B77` | 1 | [`DisasterSettings::permit_plagues`] | 0 |
//! | 39–44 | `0x1B78`–`0x1B8C` | 4 each | plague earliest start, variance, duration, strength, grace period, max occurrences | 0 0 0 0 1 1 |
//! | 45 | `0x1B94` | 4 | [`campaign_record`](Game::campaign_record) | 0 |
//! | 46 | `0x1B98` | 260 | [`unknown_1b98`](Game::unknown_1b98) | `Unknown` |
//! | 47 | `0x1A6C` | 4 | [`VictoryLimits::respawn_flag_unit_on_capture`] | 1 |
//! | 48 | `0x1A70` | 1 | [`VictoryLimits::allow_anyone_capture_any_flag`] | 0 |
//! | 49 | `0xAC` | 4 | [`VictoryLimits::gold_for_capture`] | 0 |
//! | 50 | `0x1A71` | 1 | [`reveal_entire_map`](Game::reveal_entire_map) | 0 |
//! | 51 | `0x1C9C` | 1 | [`retain_culture_on_capture`](Game::retain_culture_on_capture) | 0 |
//! | 52 | `0x1B90` | 4 | [`DisasterSettings::plague_schedule`] | −1 |
//! | 53 | `0x1CA0` | 4 | [`DisasterSettings::volcano_max_eruption_period`] | 5000 |
//! | 54–56 | `0x30F4 0x30F8 0x30FC` | 4 each | multiplayer timer: base, per city, per unit | 24, 3, 1 |
//!
//! Row sizes (**C**): Civ3 1.x rows are 16 bytes (fields 1–4); the oldest PTW rows are
//! 32, 124 or 128 bytes (cut after a dword); Play the World 11.18 rows are `5316 + 4N`;
//! Conquests rows are `7333 + 8N` (all 56 fields) or `7333 + 8N − 12` (no multiplayer
//! timers; fields 1–53). No shipped row has bytes beyond the last modelled field.
//!
//! ## Version gate
//!
//! The fork after field 18 is the float compare `major + minor * 0.01 > 11.19`
//! (**A** `0x5E29B3`, constant `0x670538`); in this crate `ctx.version > 11.19`. A file at
//! or below 11.19 has at most one more dword (field 19′), which the exe turns into the
//! boolean at `G+0x1A71`. The shipped files never contain it.
//!
//! The loader also logs the version history of this section: `GAME` rows gained their
//! post-PTW part with 12.08 (`"Setting up data added in v12.08"`), where it sets bit
//! `0x40000` of the rule flags for every older file ([`flags::V12_08`]). That is
//! [`Game::effective_rules_flags`].

use crate::Version;
use crate::io::{Ctx, Error, Field, Reader, Record, Result, Str, Writer};

/// Highest version that uses the short (PTW) layout after field 18.
const LAST_SHORT_LAYOUT: Version = Version::new(11, 19);

/// Bits of [`Game::rules_flags`].
///
/// The word mixes two groups the engine treats separately: **victory conditions**
/// ([`VICTORY_MASK`]) and **game rules** ([`RULES_MASK`]).
///
/// * Victory conditions: editor list 1823 (string table id 1281), table `0x4DD4F0`.
/// * Game rules: editor list 1825 (string id 1282), table `0x4DD508`. The editor writes a
///   bit only from these tables, and only when the matching `use_default_*` is 0.
///
/// Labels are the editor's. The engine tests the combined masks `0x24000` / `0x26000` /
/// `0x22000` / `0x25C00` on its runtime copy (flag-capture and city-elimination modes),
/// which agrees with the labels (**A**).
pub mod flags {
    /// Victory condition: Domination.
    pub const DOMINATION: u32 = 0x1;
    /// Victory condition: Space Race.
    pub const SPACE_RACE: u32 = 0x2;
    /// Victory condition: Diplomatic.
    pub const DIPLOMATIC: u32 = 0x4;
    /// Victory condition: Conquest.
    pub const CONQUEST: u32 = 0x8;
    /// Victory condition: Cultural.
    pub const CULTURAL: u32 = 0x10;
    /// Victory condition: Wonder.
    pub const WONDER: u32 = 0x10000;

    /// Game rule: Culturally Linked Start.
    pub const CULTURALLY_LINKED_START: u32 = 0x40;
    /// Game rule: Respawn AI Players.
    pub const RESPAWN_AI_PLAYERS: u32 = 0x80;
    /// Game rule: Preserve Random Seed.
    pub const PRESERVE_RANDOM_SEED: u32 = 0x100;
    /// Game rule: Accelerated Production.
    pub const ACCELERATED_PRODUCTION: u32 = 0x200;
    /// Game rule: City Elimination.
    pub const CITY_ELIMINATION: u32 = 0x400;
    /// Game rule: Regicide.
    pub const REGICIDE: u32 = 0x800;
    /// Game rule: Regicide (all Kings).
    pub const MASS_REGICIDE: u32 = 0x1000;
    /// Game rule: Victory Point Scoring.
    pub const VICTORY_POINT_SCORING: u32 = 0x2000;
    /// Game rule: Capture the Unit.
    pub const CAPTURE_THE_UNIT: u32 = 0x4000;
    /// Game rule: Allow Cultural Conversions.
    pub const ALLOW_CULTURAL_CONVERSIONS: u32 = 0x8000;
    /// Game rule: Reverse Capture the Flag. The engine drops this bit when both
    /// `use_default_*` flags are set (**A** `0x5858ED`).
    pub const REVERSE_CAPTURE_THE_FLAG: u32 = 0x20000;

    /// Game-rule bit 5. It is inside [`RULES_MASK`] but in neither editor table; four
    /// Play the World files carry it (**C**). Community documentation calls it
    /// "civ-specific abilities" (**E**, unverified).
    pub const LEGACY_BIT_5: u32 = 0x20;
    /// Game-rule bit 18, in [`RULES_MASK`] but with no editor label. The loader sets it
    /// for every file older than 12.08 and `conquests.biq` stores it. The only runtime
    /// test (`0x561C0E`) gates a random roll (3 %, 5 % for one class of civilization)
    /// that creates a Leader unit in a city and posts the `NEWSCILEADER` message, so the
    /// rule is most likely *scientific leaders* (**HYPOTHESIS** for the name).
    pub const V12_08: u32 = 0x40000;

    /// Bits that belong to the victory-condition group (**A** `0x585913`).
    pub const VICTORY_MASK: u32 = 0x1001F;
    /// Bits that belong to the game-rule group (**A** `0x585932`).
    pub const RULES_MASK: u32 = 0x6FFE0;
}

/// Value of [`Game::theme`] that selects the Fantasy soundtrack and search path.
pub const THEME_FANTASY: i32 = 1;

/// One playable civilization: an entry of the two parallel lists the row stores.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct PlayableCiv {
    /// `RACE` row index (field 3). Row 0 is the barbarians; shipped lists start at 1.
    pub civilization: i32,
    /// Locked-alliance number, `0` for none and `1..=4` for the four alliance slots
    /// (field 19; absent from files at or below 11.19, where it stays `0`).
    pub alliance: i32,
}

/// Victory-point page (dialog 208) plus the scattered fields that belong to it.
///
/// The twelve first values are consecutive on disk (fields 20–31) and are copied into
/// the game state at `0x585945`–`0x5859C9`. Their defaults are the constructor's (**A**
/// `0x5E2450`); `conquests.biq` stores exactly these.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VictoryLimits {
    /// *Victory Point Limit* (`G+0x7C`, default 50000).
    pub victory_point_limit: i32,
    /// *City Elimination Count* (`G+0x80`, default 1).
    pub city_elimination_count: i32,
    /// *Culture Value for 1 City* (`G+0x84`, default 20000).
    pub culture_one_city: i32,
    /// *Culture Value for Civilization* (`G+0x88`, default 100000).
    pub culture_civilization: i32,
    /// *% Terrain for Domination* (`G+0x8C`, default 66).
    pub domination_terrain_percent: i32,
    /// *% Population for Domination* (`G+0x90`, default 66).
    pub domination_population_percent: i32,
    /// *Wonder \* cost* (`G+0x94`, default 10).
    pub wonder_cost_multiplier: i32,
    /// *Defeating Opposing Unit \* cost* (`G+0x98`, default 10).
    pub defeat_unit_cost_multiplier: i32,
    /// *Advancement \* cost* (`G+0x9C`, default 5).
    pub advancement_cost_multiplier: i32,
    /// *City Conquest \* population* (`G+0xA0`, default 100).
    pub city_conquest_population_multiplier: i32,
    /// *Victory Point Scoring* (`G+0xA4`, default 25).
    pub victory_point_scoring: i32,
    /// *Capturing Special Unit* (`G+0xA8`, default 1000).
    pub capture_special_unit_points: i32,
    /// *Gold for Capture* (`G+0xAC`, default 0, editor range 0..=10000).
    pub gold_for_capture: i32,
    /// *Respawn Flag Unit on Capture* (`G+0x1A6C`, default 1; **A** `0x5B7FF3` compares
    /// it with 1).
    pub respawn_flag_unit_on_capture: u32,
    /// *Allow Anyone to Capture Any Flag* (`G+0x1A70`, default 0).
    pub allow_anyone_capture_any_flag: u8,
}

impl Default for VictoryLimits {
    fn default() -> Self {
        VictoryLimits {
            victory_point_limit: 50000,
            city_elimination_count: 1,
            culture_one_city: 20000,
            culture_civilization: 100000,
            domination_terrain_percent: 66,
            domination_population_percent: 66,
            wonder_cost_multiplier: 10,
            defeat_unit_cost_multiplier: 10,
            advancement_cost_multiplier: 5,
            city_conquest_population_multiplier: 100,
            victory_point_scoring: 25,
            capture_special_unit_points: 1000,
            gold_for_capture: 0,
            respawn_flag_unit_on_capture: 1,
            allow_anyone_capture_any_flag: 0,
        }
    }
}

/// Plague and volcano settings (dialog 209 *Disasters!*), plus the plague scheduler's
/// saved state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisasterSettings {
    /// *Plague Name* (`G+0x1A73`, 260 bytes, default `Black Death`).
    pub plague_name: Str<260>,
    /// *Permit Plagues* (`G+0x1B77`). The plague routine (`0x4F5265`) returns at once
    /// when this is 0.
    pub permit_plagues: u8,
    /// *Earliest Start* (`G+0x1B78`): a calendar year, `-700` in the stock rules,
    /// `1346` in *Middle Ages*.
    pub plague_earliest_start: i32,
    /// *Variance* (`G+0x1B7C`, `5` stock).
    pub plague_variance: i32,
    /// *Duration* (`G+0x1B80`, `100` stock).
    pub plague_duration: i32,
    /// *Strength* (`G+0x1B84`, `80` stock).
    pub plague_strength: i32,
    /// *Grace Period* (`G+0x1B88`, constructor default 1, `1000` stock).
    pub plague_grace_period: i32,
    /// *Max Occurances* [sic] (`G+0x1B8C`, constructor default 1, `3` stock). The plague
    /// routine stops when the running count reaches it (**A** `0x4F5285`).
    pub plague_max_occurrences: i32,
    /// Saved state of the plague scheduler (`G+0x1B90`). `-1` means "not drawn yet"; the
    /// routine then stores a random number below the variance (**A** `0x4F52CD`). Every
    /// shipped file has `-1`. No editor control.
    pub plague_schedule: i32,
    /// *Max Eruption Period* of the Volcanos box (`G+0x1CA0`, default 5000; read at
    /// `0x4F478B`).
    pub volcano_max_eruption_period: i32,
}

impl Default for DisasterSettings {
    fn default() -> Self {
        DisasterSettings {
            plague_name: Str::new("Black Death"),
            permit_plagues: 0,
            plague_earliest_start: 0,
            plague_variance: 0,
            plague_duration: 0,
            plague_strength: 0,
            plague_grace_period: 1,
            plague_max_occurrences: 1,
            plague_schedule: -1,
            volcano_max_eruption_period: 5000,
        }
    }
}

/// The scenario-wide settings row (`GAME`). One per file.
///
/// `Default` is the exe constructor's state (`0x5E2450`), which is also what a field
/// that a short row does not reach keeps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    /// *Use Default Game Rules* (`G+0x00`, default 1). When non-zero the engine ignores
    /// this row's [`RULES_MASK`](flags::RULES_MASK) bits and takes the rules the player
    /// chose in the custom-game screen instead (**A** `0x585909`..`0x585937`).
    pub use_default_game_rules: u32,
    /// *Use Default Victory Conditions* (`G+0x04`, default 1). Same for the
    /// [`VICTORY_MASK`](flags::VICTORY_MASK) bits.
    pub use_default_victory_conditions: u32,
    /// The playable civilizations (field 3 and, when present, field 19). The count word
    /// at `G+0x10` is the length of this list.
    pub playable_civilizations: Vec<PlayableCiv>,
    /// Victory-condition and game-rule bits, see [`flags`] (`G+0x14`, default 0). The
    /// editor only fills the bits whose `use_default_*` is 0.
    pub rules_flags: u32,
    /// *Auto-place Capture Units* (`G+0x18`, default 1; used at `0x56869A`, `0x569091`).
    pub auto_place_capture_units: u32,
    /// *Auto-place King Units* (`G+0x1C`, default 1; used at `0x568609`, `0x56901C`).
    pub auto_place_king_units: u32,
    /// *Auto-place Victory Locations* (`G+0x20`, default 1; used at `0x5686D9`).
    pub auto_place_victory_locations: u32,
    /// *Debug Mode* (`G+0x24`, default 0; tested at `0x4F13D6`, `0x4F6365`).
    pub debug_mode: u32,
    /// *Use Time Limit* (`G+0x28`, default 0; **A** `0x493F40`, `0x582183`).
    pub use_time_limit: u32,
    /// *Base Unit of Time* (`G+0x2C`): 0 Years, 1 Months, 2 Weeks. The calendar code at
    /// `0x5DF03C`..`0x5DF76D` reads it together with the start date and the two time
    /// scale arrays. *WWII in the Pacific* stores 1 with start month 12 and year 1941 (**C**).
    pub base_unit_of_time: u32,
    /// *Start Month* (`G+0x30`, 1..=12, default 1).
    pub start_month: i32,
    /// *Start Week* (`G+0x34`, 1..=52, default 1).
    pub start_week: i32,
    /// *Start Year* (`G+0x38`, negative is BC, default −4000; the editor maps 0 to 1).
    pub start_year: i32,
    /// *Time limit, minutes* (`G+0x3C`, 0..=1440, default 0).
    pub time_limit_minutes: i32,
    /// *Time limit, turns* (`G+0x40`, 1..=1000, default 540). The engine uses 540 when
    /// the stored value is 0 and clamps to 1..=1000 (`0x581800`..`0x581890`).
    pub time_limit_turns: i32,
    /// Time scale, "turns" column (`G+0x44`, seven steps). Step `i` lasts
    /// `time_scale_turns[i]` turns of `time_scale_units[i]` base units each; the last
    /// step carries on until the game ends. Default 25 25 40 50 100 100 100.
    pub time_scale_turns: [i32; 7],
    /// Time scale, "units each" column (`G+0x60`). Default 50 40 25 20 10 5 2.
    pub time_scale_units: [i32; 7],
    /// *Scenario Search Folders* (`G+0xB4`, 5200 bytes): semicolon-separated folders the
    /// game searches for art and sounds, relative to the scenario, e.g. `New Alliances`
    /// or `..\Extras\Medieval Japan;..\Extras\Prehistoric`.
    pub search_folders: Str<5200>,
    /// Field 19′, only on disk at version ≤ 11.19 (none shipped). The exe turns it into
    /// [`reveal_entire_map`](Game::reveal_entire_map) as `value != 0` (`0x5E29BA`).
    pub legacy_reveal_map: u32,
    /// Victory-point page and related fields.
    pub victory: VictoryLimits,
    /// `G+0xB0`, no editor control, 0 in every shipped file. The value 1
    /// ([`THEME_FANTASY`]) switches the background-music tables to the `Fantasy\Fantasy
    /// Mix` / `Fantasy\Fantasy 2 Mix` entries and seeds the live search-path buffer
    /// (`G+0x1CA4`) with `Fantasy` (**A** `0x5361F7`, `0x536263`, `0x536413`,
    /// `0x5987B4`).
    pub theme: i32,
    /// `G+0x1A72`, always 0 in files. The engine overwrites it at game start and reads
    /// it in 23 places (`0x40780F`, `0x41B329`, `0x4E92FC`, ...). It is set to 1 when
    /// the game-mode global `[0x990394]` is 1 (`0x5A2954`) and to `mode == 3` at game
    /// setup (`0x54E6A6`); the mode's meaning is not pinned down, so the name is neutral.
    pub runtime_flag_1a72: u8,
    /// Locked-alliance names (`G+0x1504`): five 256-byte buffers; index 0 is unused and
    /// 1..=4 hold `Alliance 1` .. `Alliance 4` or a custom name.
    pub alliance_names: [Str<256>; 5],
    /// Locked-alliance war matrix (`G+0x1A04`): `alliance_war[a * 5 + b]` is non-zero when
    /// alliance `a` is at war with `b` (dialog 207). Symmetric in every shipped file.
    pub alliance_war: [u32; 25],
    /// *Alliance Victory Type* (`G+0x1A68`): 0 Individual, 1 Coalition. Read in ten
    /// places in the diplomacy and victory code (`0x4F1CF5`, `0x541E07`, ...).
    pub alliance_victory_type: u32,
    /// Plague and volcano settings.
    pub disasters: DisasterSettings,
    /// `G+0x1B94`, no editor control: non-zero in the nine non-multiplayer Conquests
    /// campaign scenarios (value 1). At the end of the human player's game the engine
    /// passes it, with the scenario file name and score, to the campaign-record writer
    /// (`0x4F16FA` -> `0x4A97A0`; strings `CAMPAIGN_RECORD`, `GMRC`, `CMRC`,
    /// `art\interface\campRec.pcx`). The editor zeroes it when it saves.
    pub campaign_record: i32,
    /// `G+0x1B98`, 260 bytes, default `Unknown`, no editor control and no engine reader found.
    pub unknown_1b98: Str<260>,
    /// *Reveal Entire Map* (`G+0x1A71`; read at `0x4950BA`, `0x4F634D`).
    pub reveal_entire_map: u8,
    /// *Retain Culture on Capture* (`G+0x1C9C`; read at `0x5650F9`).
    pub retain_culture_on_capture: u8,
    /// Multiplayer turn timer, constant part (`G+0x30F4`, default 24).
    ///
    /// The three values give `base + per_city * cities + per_unit * units`, which the
    /// engine scales by the game-speed setting and doubles (`0x467FC8`..`0x468027`,
    /// `0x468499`..`0x4684FA`). The unit is presumably seconds (**HYPOTHESIS**).
    pub mp_timer_base: i32,
    /// Multiplayer timer, seconds per city (`G+0x30F8`, default 3).
    pub mp_timer_per_city: i32,
    /// Multiplayer timer, seconds per unit (`G+0x30FC`, default 1).
    pub mp_timer_per_unit: i32,
    /// How many leading fields (numbered in the module docs) the row had when it was
    /// read; [`usize::MAX`] for a record built in memory.
    ///
    /// [`Record::write`] stops after this many fields, which is what makes the old,
    /// shorter rows (16, 32, 124, 128 bytes, ...) round-trip byte for byte. To turn such
    /// a record into a full modern row, set it to `usize::MAX`; the fields the old row did
    /// not reach already hold the constructor defaults.
    pub fields_present: usize,
    /// Bytes after the last modelled field (empty in every shipped file).
    pub extra: Vec<u8>,
}

impl Default for Game {
    fn default() -> Self {
        Game {
            use_default_game_rules: 1,
            use_default_victory_conditions: 1,
            playable_civilizations: Vec::new(),
            rules_flags: 0,
            auto_place_capture_units: 1,
            auto_place_king_units: 1,
            auto_place_victory_locations: 1,
            debug_mode: 0,
            use_time_limit: 0,
            base_unit_of_time: 0,
            start_month: 1,
            start_week: 1,
            start_year: -4000,
            time_limit_minutes: 0,
            time_limit_turns: 540,
            time_scale_turns: [25, 25, 40, 50, 100, 100, 100],
            time_scale_units: [50, 40, 25, 20, 10, 5, 2],
            search_folders: Str::default(),
            legacy_reveal_map: 0,
            victory: VictoryLimits::default(),
            theme: 0,
            runtime_flag_1a72: 0,
            alliance_names: [Str::default(); 5],
            alliance_war: [0; 25],
            alliance_victory_type: 0,
            disasters: DisasterSettings::default(),
            campaign_record: 0,
            unknown_1b98: Str::new("Unknown"),
            reveal_entire_map: 0,
            retain_culture_on_capture: 0,
            mp_timer_base: 24,
            mp_timer_per_city: 3,
            mp_timer_per_unit: 1,
            fields_present: usize::MAX,
            extra: Vec::new(),
        }
    }
}

impl Game {
    /// Rule flags as the loader leaves them for a file of `version`: files older than
    /// 12.08 get [`flags::V12_08`] added (12.08 itself does not).
    pub fn effective_rules_flags(&self, version: Version) -> u32 {
        if version < Version::new(12, 8) {
            self.rules_flags | flags::V12_08
        } else {
            self.rules_flags
        }
    }

    /// The rule word the engine's game state ends up with (**A**
    /// `0x5858D0`..`0x585939`).
    ///
    /// `custom` is what the player chose in the custom-game screen (`[0xA52B7C]`). Each
    /// group is taken from this scenario unless its `use_default_*` flag is set. When
    /// both flags are set the whole custom word is used, minus
    /// [`flags::REVERSE_CAPTURE_THE_FLAG`].
    pub fn engine_flags(&self, version: Version, custom: u32) -> u32 {
        if self.use_default_game_rules != 0 && self.use_default_victory_conditions != 0 {
            return custom & !flags::REVERSE_CAPTURE_THE_FLAG;
        }
        let own = self.effective_rules_flags(version);
        let victory = if self.use_default_victory_conditions != 0 {
            custom
        } else {
            own
        };
        let rules = if self.use_default_game_rules != 0 {
            custom
        } else {
            own
        };
        (victory & flags::VICTORY_MASK) | (rules & flags::RULES_MASK)
    }

    /// True if alliance `a` is at war with alliance `b` (both `1..=4`; 0 is "none").
    pub fn alliances_at_war(&self, a: usize, b: usize) -> bool {
        a < 5 && b < 5 && self.alliance_war[a * 5 + b] != 0
    }

    /// The field list, in file order. The same walk both reads and writes (see
    /// [`Pass`]), so the two directions cannot disagree.
    fn walk(&mut self, p: &mut Pass<'_, '_>, version: Version) -> Step {
        p.field(&mut self.use_default_game_rules)?;
        p.field(&mut self.use_default_victory_conditions)?;
        p.civilizations(&mut self.playable_civilizations)?;
        p.field(&mut self.rules_flags)?;
        p.field(&mut self.auto_place_capture_units)?;
        p.field(&mut self.auto_place_king_units)?;
        p.field(&mut self.auto_place_victory_locations)?;
        p.field(&mut self.debug_mode)?;
        p.field(&mut self.use_time_limit)?;
        p.field(&mut self.base_unit_of_time)?;
        p.field(&mut self.start_month)?;
        p.field(&mut self.start_week)?;
        p.field(&mut self.start_year)?;
        p.field(&mut self.time_limit_minutes)?;
        p.field(&mut self.time_limit_turns)?;
        p.field(&mut self.time_scale_turns)?;
        p.field(&mut self.time_scale_units)?;
        p.field(&mut self.search_folders)?;

        if version <= LAST_SHORT_LAYOUT {
            p.field(&mut self.legacy_reveal_map)?;
            self.reveal_entire_map = u8::from(self.legacy_reveal_map != 0);
            return Ok(());
        }

        p.alliances(&mut self.playable_civilizations)?;
        p.field(&mut self.victory.victory_point_limit)?;
        p.field(&mut self.victory.city_elimination_count)?;
        p.field(&mut self.victory.culture_one_city)?;
        p.field(&mut self.victory.culture_civilization)?;
        p.field(&mut self.victory.domination_terrain_percent)?;
        p.field(&mut self.victory.domination_population_percent)?;
        p.field(&mut self.victory.wonder_cost_multiplier)?;
        p.field(&mut self.victory.defeat_unit_cost_multiplier)?;
        p.field(&mut self.victory.advancement_cost_multiplier)?;
        p.field(&mut self.victory.city_conquest_population_multiplier)?;
        p.field(&mut self.victory.victory_point_scoring)?;
        p.field(&mut self.victory.capture_special_unit_points)?;
        p.field(&mut self.theme)?;
        p.field(&mut self.runtime_flag_1a72)?;
        p.field(&mut self.alliance_names)?;
        p.field(&mut self.alliance_war)?;
        p.field(&mut self.alliance_victory_type)?;
        p.field(&mut self.disasters.plague_name)?;
        p.field(&mut self.disasters.permit_plagues)?;
        p.field(&mut self.disasters.plague_earliest_start)?;
        p.field(&mut self.disasters.plague_variance)?;
        p.field(&mut self.disasters.plague_duration)?;
        p.field(&mut self.disasters.plague_strength)?;
        p.field(&mut self.disasters.plague_grace_period)?;
        p.field(&mut self.disasters.plague_max_occurrences)?;
        p.field(&mut self.campaign_record)?;
        p.field(&mut self.unknown_1b98)?;
        p.field(&mut self.victory.respawn_flag_unit_on_capture)?;
        p.field(&mut self.victory.allow_anyone_capture_any_flag)?;
        p.field(&mut self.victory.gold_for_capture)?;
        p.field(&mut self.reveal_entire_map)?;
        p.field(&mut self.retain_culture_on_capture)?;
        p.field(&mut self.disasters.plague_schedule)?;
        p.field(&mut self.disasters.volcano_max_eruption_period)?;
        p.field(&mut self.mp_timer_base)?;
        p.field(&mut self.mp_timer_per_city)?;
        p.field(&mut self.mp_timer_per_unit)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// One walk, two directions
// ---------------------------------------------------------------------------

/// Why a walk over the fields stopped before the last one.
enum Stop {
    /// Reading: the row has no room for the next field. Writing: the field budget
    /// ([`Game::fields_present`]) is used up. Not an error.
    End,
    /// The data is inconsistent.
    Bad(Error),
}

type Step = std::result::Result<(), Stop>;

enum Io<'a, 'r> {
    Read(&'r mut Reader<'a>),
    Write(&'r mut Writer),
}

/// Cursor for [`Game::walk`]: fills the record from a row, or emits it, one field at
/// a time, and counts how many fields it handled.
struct Pass<'a, 'r> {
    io: Io<'a, 'r>,
    done: usize,
    limit: usize,
}

impl Pass<'_, '_> {
    /// Writing stops once `limit` fields went out; reading has no limit.
    fn gate(&self) -> Step {
        if matches!(self.io, Io::Write(_)) && self.done >= self.limit {
            Err(Stop::End)
        } else {
            Ok(())
        }
    }

    /// One fixed-size field.
    fn field<T: Field>(&mut self, v: &mut T) -> Step {
        self.gate()?;
        match &mut self.io {
            Io::Read(r) => {
                if r.remaining() < T::SIZE {
                    return Err(Stop::End);
                }
                *v = T::read(r);
            }
            Io::Write(w) => v.write(w),
        }
        self.done += 1;
        Ok(())
    }

    /// Field 3: the count word and the `RACE` indices.
    fn civilizations(&mut self, civs: &mut Vec<PlayableCiv>) -> Step {
        self.gate()?;
        match &mut self.io {
            Io::Read(r) => match r.counted_list::<i32>(Game::TAG, "playable civilizations") {
                Ok(Some(list)) => {
                    *civs = list
                        .into_iter()
                        .map(|civilization| PlayableCiv {
                            civilization,
                            alliance: 0,
                        })
                        .collect();
                }
                Ok(None) => return Err(Stop::End),
                Err(e) => return Err(Stop::Bad(e)),
            },
            Io::Write(w) => {
                w.u32(civs.len() as u32);
                for c in civs.iter() {
                    w.i32(c.civilization);
                }
            }
        }
        self.done += 1;
        Ok(())
    }

    /// Field 19: one alliance number per playable civilization, no count of its own.
    fn alliances(&mut self, civs: &mut [PlayableCiv]) -> Step {
        self.gate()?;
        match &mut self.io {
            Io::Read(r) => {
                if r.remaining() < civs.len() * i32::SIZE {
                    return Err(Stop::End);
                }
                for c in civs.iter_mut() {
                    c.alliance = i32::read(r);
                }
            }
            Io::Write(w) => {
                for c in civs.iter() {
                    w.i32(c.alliance);
                }
            }
        }
        self.done += 1;
        Ok(())
    }
}

impl Record for Game {
    const TAG: [u8; 4] = *b"GAME";

    fn read(r: &mut Reader<'_>, ctx: &Ctx) -> Result<Self> {
        let mut g = Game::default();
        let mut pass = Pass {
            io: Io::Read(&mut *r),
            done: 0,
            limit: usize::MAX,
        };
        let outcome = g.walk(&mut pass, ctx.version);
        let done = pass.done;
        match outcome {
            Ok(()) | Err(Stop::End) => {}
            Err(Stop::Bad(e)) => return Err(e),
        }
        g.fields_present = done;
        g.extra = r.rest().to_vec();
        Ok(g)
    }

    fn write(&self, w: &mut Writer, ctx: &Ctx) {
        let mut copy = self.clone();
        let mut pass = Pass {
            io: Io::Write(&mut *w),
            done: 0,
            limit: self.fields_present,
        };
        // `Stop::End` just means the field budget ran out; writing cannot fail otherwise.
        let _ = copy.walk(&mut pass, ctx.version);
        w.bytes(&self.extra);
    }

    fn extra(&self) -> &[u8] {
        &self.extra
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// First `GAME` row of the first corpus file whose short name satisfies `pred`.
    fn load(pred: impl Fn(&str) -> bool) -> Option<(Game, Version)> {
        let files = crate::corpus::files();
        let f = files.iter().find(|f| pred(&f.name()))?;
        let row = crate::corpus::rows::<Game>(std::slice::from_ref(f))
            .into_iter()
            .next()?;
        let g = Game::read(&mut Reader::new(row.body), &f.ctx()).ok()?;
        Some((g, f.version))
    }

    fn named(part: &'static str) -> impl Fn(&str) -> bool {
        move |n| n.contains(part)
    }

    fn civs(g: &Game) -> Vec<i32> {
        g.playable_civilizations
            .iter()
            .map(|c| c.civilization)
            .collect()
    }

    fn alliances(g: &Game) -> Vec<i32> {
        g.playable_civilizations
            .iter()
            .map(|c| c.alliance)
            .collect()
    }

    #[test]
    fn corpus_roundtrip() {
        let st = crate::corpus::check_roundtrip::<Game>();
        assert_eq!(st.with_extra, 0, "{}", st.report());
    }

    /// The Scenario page's save routine (`0x44A5B0`) clamps what the player types: start
    /// month 1-12, start week 1-52, start year within 10000 years of 1 AD and never 0, time
    /// limit 0-1440 minutes and 1-1000 turns, every time scale cell 0-1000, the three
    /// multiplayer timer values 0-100. The Victory Point page clamps its numbers as well.
    #[test]
    fn editor_ranges_hold_in_every_file() {
        let files = crate::corpus::files();
        let mut rows = 0;
        for row in crate::corpus::rows::<Game>(&files) {
            let g = Game::read(&mut Reader::new(row.body), &row.file.ctx()).unwrap();
            let who = row.file.name();
            let within = |v: i32, lo: i32, hi: i32| (lo..=hi).contains(&v);
            assert!(within(g.start_month, 1, 12), "{who}");
            assert!(within(g.start_week, 1, 52), "{who}");
            assert!(
                within(g.start_year, -10000, 10000) && g.start_year != 0,
                "{who}"
            );
            assert!(within(g.time_limit_minutes, 0, 1440), "{who}");
            assert!(within(g.time_limit_turns, 1, 1000), "{who}");
            for v in g.time_scale_turns.iter().chain(&g.time_scale_units) {
                assert!(within(*v, 0, 1000), "{who}");
            }
            for v in [g.mp_timer_base, g.mp_timer_per_city, g.mp_timer_per_unit] {
                assert!(within(v, 0, 100), "{who}");
            }
            assert!(g.base_unit_of_time <= 2, "{who}");
            assert!(within(g.victory.gold_for_capture, 0, 10000), "{who}");
            for flag in [
                g.use_default_game_rules,
                g.use_default_victory_conditions,
                g.auto_place_capture_units,
                g.auto_place_king_units,
                g.auto_place_victory_locations,
                g.debug_mode,
                g.use_time_limit,
                g.alliance_victory_type,
            ] {
                assert!(flag <= 1, "{who}");
            }
            rows += 1;
        }
        if !files.is_empty() {
            assert!(rows >= 40, "{rows}");
        }
    }

    /// The reader hands the rest of the row to `extra`; a row that ends exactly on a
    /// field boundary leaves it empty and records how far it got.
    #[test]
    fn short_rows_are_prefixes_of_the_full_layout() {
        let ctx = Ctx {
            version: Version::new(11, 13),
        };
        // use_default x2, N = 2, civs 3 and 5, rules_flags.
        let mut w = Writer::new();
        for v in [0u32, 1, 2, 3, 5, 0x8001] {
            w.u32(v);
        }
        let g = Game::read(&mut Reader::new(&w.buf), &ctx).unwrap();
        assert_eq!(g.fields_present, 4);
        assert_eq!(g.use_default_game_rules, 0);
        assert_eq!(civs(&g), [3, 5]);
        assert_eq!(g.rules_flags, 0x8001);
        // Fields the row did not reach keep the constructor defaults.
        assert_eq!(g.time_limit_turns, 540);
        assert_eq!(g.start_year, -4000);
        assert!(g.extra.is_empty());
        let mut out = Writer::new();
        g.write(&mut out, &ctx);
        assert_eq!(out.buf, w.buf);
    }

    #[test]
    fn unknown_trailing_bytes_are_kept() {
        let ctx = Ctx {
            version: Version::new(11, 13),
        };
        let mut w = Writer::new();
        for v in [1u32, 1, 0, 0] {
            w.u32(v);
        }
        w.bytes(&[1, 2, 3]);
        let g = Game::read(&mut Reader::new(&w.buf), &ctx).unwrap();
        assert_eq!(g.fields_present, 4);
        assert_eq!(g.extra, [1, 2, 3]);
        let mut out = Writer::new();
        g.write(&mut out, &ctx);
        assert_eq!(out.buf, w.buf);
    }

    #[test]
    fn absurd_civilization_count_is_an_error() {
        let ctx = Ctx {
            version: Version::new(12, 8),
        };
        let mut w = Writer::new();
        for v in [1u32, 1, 1_000_000] {
            w.u32(v);
        }
        assert!(matches!(
            Game::read(&mut Reader::new(&w.buf), &ctx),
            Err(Error::BadCount { .. })
        ));
    }

    /// A default record is a complete modern row of the documented size.
    #[test]
    fn default_row_has_the_documented_size() {
        let ctx = Ctx {
            version: Version::new(12, 8),
        };
        for n in [0usize, 8, 31] {
            let g = Game {
                playable_civilizations: (0..n as i32)
                    .map(|civilization| PlayableCiv {
                        civilization,
                        alliance: 0,
                    })
                    .collect(),
                ..Game::default()
            };
            let mut w = Writer::new();
            g.write(&mut w, &ctx);
            assert_eq!(w.buf.len(), 7333 + 8 * n);
            let back = Game::read(&mut Reader::new(&w.buf), &ctx).unwrap();
            assert_eq!(back.fields_present, 56);
            assert_eq!(
                back,
                Game {
                    fields_present: 56,
                    ..g
                }
            );
        }
    }

    /// At or below 11.19 the row stops after the search folders (plus one optional dword).
    #[test]
    fn short_layout_below_11_19() {
        let ctx = Ctx {
            version: Version::new(11, 18),
        };
        let g = Game {
            playable_civilizations: vec![PlayableCiv::default(); 3],
            legacy_reveal_map: 7,
            ..Game::default()
        };
        let mut w = Writer::new();
        g.write(&mut w, &ctx);
        assert_eq!(w.buf.len(), 5316 + 4 * 3 + 4);
        let back = Game::read(&mut Reader::new(&w.buf), &ctx).unwrap();
        assert_eq!(back.legacy_reveal_map, 7);
        assert_eq!(back.reveal_entire_map, 1);
        assert_eq!(back.fields_present, 19);
    }

    #[test]
    fn conquests_stock_settings() {
        let Some((g, v)) = load(|n| n.ends_with("conquests.biq")) else {
            return;
        };
        assert_eq!(v, Version::new(12, 8));
        assert_eq!(g.fields_present, 56);
        assert_eq!(civs(&g), (1..=31).collect::<Vec<_>>());
        assert!(g.playable_civilizations.iter().all(|c| c.alliance == 0));
        assert_eq!(
            (g.use_default_game_rules, g.use_default_victory_conditions),
            (1, 1)
        );
        assert_eq!(g.rules_flags, flags::V12_08);
        assert_eq!(g.start_year, -4000);
        assert_eq!(g.time_limit_turns, 540);
        assert_eq!(g.time_scale_turns, [25, 25, 40, 50, 100, 100, 100]);
        assert_eq!(g.time_scale_units, [50, 40, 25, 20, 10, 5, 2]);
        assert!(g.search_folders.is_empty());
        let vp = &g.victory;
        assert_eq!(vp.victory_point_limit, 50000);
        assert_eq!(vp.culture_civilization, 100000);
        assert_eq!(
            (
                vp.domination_terrain_percent,
                vp.domination_population_percent
            ),
            (66, 66)
        );
        assert_eq!(vp.capture_special_unit_points, 1000);
        assert_eq!(g.disasters.plague_name.text(), "Plague");
        assert_eq!(g.disasters.plague_earliest_start, -700);
        assert_eq!(g.disasters.plague_max_occurrences, 3);
        assert_eq!(g.disasters.plague_schedule, -1);
        assert_eq!(g.disasters.volcano_max_eruption_period, 5000);
        assert_eq!(g.unknown_1b98.text(), "Unknown");
        assert_eq!(
            (g.mp_timer_base, g.mp_timer_per_city, g.mp_timer_per_unit),
            (24, 3, 1)
        );
    }

    #[test]
    fn rise_of_rome_alliances_and_calendar() {
        let Some((g, _)) = load(|n| n.contains("2 Rise of Rome") && !n.contains("MP")) else {
            return;
        };
        assert_eq!(civs(&g), (1..=8).collect::<Vec<_>>());
        assert_eq!(alliances(&g), [1, 0, 3, 0, 0, 4, 0, 2]);
        assert!(g.alliances_at_war(1, 2) && g.alliances_at_war(2, 1));
        assert!(g.alliances_at_war(3, 4) && g.alliances_at_war(4, 3));
        assert!(!g.alliances_at_war(1, 3));
        assert_eq!(g.alliance_victory_type, 0);
        assert_eq!(g.alliance_names[1].text(), "Alliance 1");
        assert_eq!(g.alliance_names[0].text(), "");
        assert_eq!(
            g.rules_flags,
            flags::DOMINATION | flags::ALLOW_CULTURAL_CONVERSIONS
        );
        assert_eq!((g.base_unit_of_time, g.start_year), (0, -350));
        assert_eq!(g.use_time_limit, 1);
        assert_eq!(g.time_limit_turns, 130);
        assert_eq!(g.time_scale_turns, [130, 40, 35, 50, 100, 100, 100]);
        assert_eq!(g.time_scale_units, [5, 6, 6, 6, 6, 5, 2]);
        assert_eq!(g.campaign_record, 1);
        assert_eq!(g.retain_culture_on_capture, 1);
    }

    #[test]
    fn intro3_locked_alliances() {
        let Some((g, _)) = load(named("Intro3 New Alliances")) else {
            return;
        };
        assert_eq!(civs(&g), [5, 27, 28, 29]);
        assert_eq!(alliances(&g), [1, 2, 2, 1]);
        assert!(g.alliances_at_war(1, 2) && g.alliances_at_war(2, 1));
        assert_eq!(g.alliance_victory_type, 1);
        assert_eq!(g.search_folders.text(), "New Alliances");
        assert_eq!(g.disasters.permit_plagues, 1);
        assert_eq!(g.disasters.plague_earliest_start, 925);
        assert_eq!(
            (g.use_default_game_rules, g.use_default_victory_conditions),
            (0, 0)
        );
        // Written before the multiplayer timers existed.
        assert_eq!(g.fields_present, 53);
        assert_eq!(g.mp_timer_base, 24);
    }

    #[test]
    fn named_alliances_and_monthly_calendars() {
        if let Some((g, _)) = load(named("9 WWII in the Pacific")) {
            assert_eq!(g.alliance_names[1].text(), "Allies");
            assert_eq!(g.alliance_names[2].text(), "Japanese Empire");
            assert_eq!(g.alliance_victory_type, 1);
            assert_eq!(alliances(&g), [1, 1, 2, 1, 1]);
            // 300 turns of one month each, from December 1941.
            assert_eq!(
                (g.base_unit_of_time, g.start_month, g.start_year),
                (1, 12, 1941)
            );
            assert_eq!((g.time_scale_turns[0], g.time_scale_units[0]), (300, 1));
            assert_eq!(g.reveal_entire_map, 1);
        }
        if let Some((g, _)) = load(named("8 Napoleonic Europe")) {
            assert_eq!(g.alliance_names[1].text(), "French Coalition");
            assert_eq!(g.alliance_names[2].text(), "English Coalition");
            assert_eq!(g.start_year, 1800);
        }
        if let Some((g, _)) = load(named("7 Sengoku")) {
            assert_eq!(
                g.rules_flags,
                flags::DOMINATION
                    | flags::DIPLOMATIC
                    | flags::CONQUEST
                    | flags::REGICIDE
                    | flags::ALLOW_CULTURAL_CONVERSIONS
            );
        }
    }

    #[test]
    fn plague_scenarios_and_gold_for_capture() {
        if let Some((g, _)) = load(named("With Plague")) {
            assert_eq!(g.disasters.permit_plagues, 1);
            assert_eq!(g.disasters.plague_name.text(), "Plague");
        }
        if let Some((g, _)) = load(|n| n.contains("4 Middle Ages") && !n.contains("MP")) {
            assert_eq!(g.disasters.plague_name.text(), "Black Death");
            assert_eq!(g.disasters.plague_earliest_start, 1346);
            assert_eq!(g.victory.gold_for_capture, 500);
            assert_eq!(g.victory.respawn_flag_unit_on_capture, 0);
            assert_eq!(g.victory.allow_anyone_capture_any_flag, 1);
            assert_eq!(g.victory.capture_special_unit_points, 10000);
        }
    }

    #[test]
    fn multiplayer_timer_overrides() {
        if let Some((g, _)) = load(named("MPTournament")) {
            assert_eq!(
                (g.mp_timer_base, g.mp_timer_per_city, g.mp_timer_per_unit),
                (16, 2, 1)
            );
            assert_eq!(
                (g.use_default_game_rules, g.use_default_victory_conditions),
                (0, 1)
            );
            assert_eq!(
                g.rules_flags,
                flags::ACCELERATED_PRODUCTION | flags::ALLOW_CULTURAL_CONVERSIONS
            );
        }
    }

    #[test]
    fn ptw_search_folders() {
        let Some((g, v)) = load(|n| n.ends_with("TETurkhan.bix")) else {
            return;
        };
        assert_eq!(v, Version::new(11, 18));
        assert_eq!(g.fields_present, 18);
        assert!(g.search_folders.text().contains("Medieval Japan"));
        assert_eq!(civs(&g).len(), 31);
    }

    /// Every victory/rule bit set anywhere in the corpus is one the engine masks know.
    #[test]
    fn corpus_rule_bits_are_covered_by_the_masks() {
        let known = flags::VICTORY_MASK | flags::RULES_MASK;
        for f in crate::corpus::files() {
            for row in crate::corpus::rows::<Game>(std::slice::from_ref(&f)) {
                let g = Game::read(&mut Reader::new(row.body), &f.ctx()).unwrap();
                assert_eq!(g.rules_flags & !known, 0, "{}", f.name());
            }
        }
    }

    #[test]
    fn engine_flag_merge() {
        let custom = flags::SPACE_RACE | flags::CITY_ELIMINATION | flags::REVERSE_CAPTURE_THE_FLAG;
        let mut g = Game {
            rules_flags: flags::DOMINATION | flags::REGICIDE,
            ..Game::default()
        };
        let v12_8 = Version::new(12, 8);

        // Both defaults: the player's word, minus reverse capture the flag.
        assert_eq!(
            g.engine_flags(v12_8, custom),
            flags::SPACE_RACE | flags::CITY_ELIMINATION
        );

        // Own victory conditions, the player's rules.
        g.use_default_victory_conditions = 0;
        assert_eq!(
            g.engine_flags(v12_8, custom),
            flags::DOMINATION | flags::CITY_ELIMINATION | flags::REVERSE_CAPTURE_THE_FLAG
        );

        // Own rules, the player's victory conditions.
        g.use_default_victory_conditions = 1;
        g.use_default_game_rules = 0;
        assert_eq!(
            g.engine_flags(v12_8, custom),
            flags::SPACE_RACE | flags::REGICIDE
        );

        // Neither default: only the scenario's bits, and the loader's 12.08 bit for older files.
        g.use_default_victory_conditions = 0;
        assert_eq!(
            g.engine_flags(v12_8, custom),
            flags::DOMINATION | flags::REGICIDE
        );
        assert_eq!(
            g.engine_flags(Version::new(12, 7), custom),
            flags::DOMINATION | flags::REGICIDE | flags::V12_08
        );
    }

    #[test]
    fn effective_flags_threshold() {
        let g = Game::default();
        assert_eq!(g.effective_rules_flags(Version::new(12, 7)), flags::V12_08);
        assert_eq!(g.effective_rules_flags(Version::new(12, 8)), 0);
        assert_eq!(g.effective_rules_flags(Version::new(11, 18)), flags::V12_08);
    }
}
