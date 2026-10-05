//! `PRTO` — unit prototypes (editor dialog **151** "Units").
//!
//! Game: row reader `0x5E54B0`, writer `0x5E4F20`, constructor `0x5E4D00`, section loader
//! `0x595C40` (counts rows, upgrades old ones). The in-memory row is `0x138` bytes. Editor:
//! page → row `0x45CC80`, row → page `0x45E1C0`.
//!
//! # Row layout
//!
//! Offsets are into the row body (after the length word); the in-memory offset is the body
//! offset `+4` (the first memory dword is the reader's bytes-left counter), and `+6` after the
//! terrain bytes (two bytes of padding in memory that are not in the file). The reader takes a
//! field only when its full size is still left in the row (the five action words together,
//! 20 bytes); a field that is missing keeps the value the constructor gave it (see
//! [`UnitType::default`]), except the handful the reader fills in itself (below).
//!
//! | body offset | size | field | since |
//! |---|---|---|---|
//! | `0x00` | 4 | `zone_of_control` | |
//! | `0x04` / `0x24` | 32 + 32 | `name` / `civilopedia_entry` | |
//! | `0x44`..`0x74` | 13 × 4 | stat block, [`UnitType::bombard_strength`] … [`UnitType::upgrade_to`] | |
//! | `0x78` | 3 × 4 | `required_resource_1..3` | |
//! | `0x84`..`0xA0` | 8 × 4 | `abilities`, `ai_strategies`, `available_to_civs`, 2 legacy words, `unit_class`, `alt_strategy_of`, `hit_point_bonus` | |
//! | | | *end of a Civ3 1.x row: 164 bytes* | |
//! | `0xA4`..`0xB4` | 5 × 4 | `standard_orders`, `special_actions`, `worker_actions`, `air_missions`, `command_mask` | [`ACTION_WORDS_SINCE`] |
//! | `0xB8` | 4 | `bombard_fx` | [`BOMBARD_FX_SINCE`] |
//! | `0xBC` | 12 or 14 | `ignore_move_cost`, one byte per `TERR` row | [`IGNORE_MOVE_COST_SINCE`] |
//! | | 4 | `require_support` | [`REQUIRE_SUPPORT_SINCE`] |
//! | | | *end of a PTW row: 204 bytes* | |
//! | | 4 | `revision` (7) | [`CONQUESTS_SINCE`] |
//! | | 4 | `telepad_range` | |
//! | | 8 + 4n | `legal_unit_telepads` | |
//! | | 4 | `enslave_results_in` | |
//! | | 8 + 4n | `stealth_attack_targets` | |
//! | | 8 + 4n | `legal_building_telepads` | |
//! | | 1 | `create_craters` | |
//! | | 4 | `worker_strength` (float) | |
//! | | 4 | `abilities_extended` | |
//! | | 4 | `air_defense` | |
//!
//! Conquests rows are 255 bytes plus 4 per list entry (a Stealth Fighter has 92 stealth
//! targets: 623 bytes).
//!
//! # Version history
//!
//! The row only ever grew at its end, and the editor's release notes name the version of each
//! step, in the order of the layout: Civ3XEdit 2.00 (`11.00`) "added expansion unit actions"
//! (Explore, Sentry, Build Airfield / Radar Tower / Outpost: the five action words; the game's
//! loader translates Civ3 1.x rows older than `10.0` the same way, [`UnitType::actions_from_legacy`]),
//! 2.05 (`11.05`) the bombard effect flag, 2.09 (`11.09`) the terrain movement bonus ("Ignore
//! Move Cost"), 2.25 (`11.11`) "Req. Support". The Conquests editor added the tail. The crate
//! reads and writes each group only for files of at least its version, and keeps any bytes
//! past the last group of a file's version in [`UnitType::extra`], so every file writes back
//! exactly. The corpus has `PRTO` rows only at `4.01` (164 bytes), `11.18` (204) and `12.06`
//! to `12.08`; the intermediate gates follow the release notes, not observed rows.
//!
//! # Defaults and what the reader fills in
//!
//! [`UnitType::default`] is the game's row constructor (`0x5E4D00`), which is also what the
//! editor's *Add* button creates: movement 1, all `-1` references, every standard order
//! allowed (`0x0FFFFFFF`), *Req. Support* on, revision 7, worker strength 1.0. When a row lacks a
//! tail field the reader (`0x5E54B0`) uses: revision `0`, telepad range `0`, enslave result `-1`,
//! empty lists, no craters, extended abilities `0`, air defense `0`, and a derived worker
//! strength ([`UnitType::derived_worker_strength`]: 1.0 for units that improve terrain, 0
//! otherwise). The crate reproduces all of it.
//!
//! The derived and legacy fields:
//!
//! * `command_mask` is recomputed by the editor from the other action words every time it
//!   saves ([`UnitType::derived_command_mask`], editor `0x45DF03`); it is exact in every
//!   shipped Conquests row.
//! * `legacy_orders_a/b` are the Civ3 1.x packed action words ([`UnitType::actions_from_legacy`]).
//!   For files older than `10.0` the crate fills the five action words from them when it
//!   reads the row (as the game does) and keeps the two words, so the row writes back
//!   unchanged; in newer files they are uninitialised editor memory.
//!
//! # Editor ranges
//!
//! The constructor and the editor's apply routine (`0x45CC80`) clamp the numbers the same way:
//! bombard strength `0..=1000`, bombard range `0..=362`, transport capacity `0..=100`, shield
//! cost `0..=1000`, defense and attack `0..=1000`, operational range `0..=362`, population cost
//! `0..=255`, rate of fire `0..=10`, movement `1..=100`, HP bonus `-20..=20`, telepad range
//! `0..=100`. Three more rules of the apply routine: *Bombard Fx* and *Create Craters* are only
//! kept when the bombard strength is above 0, *Ignore Move Cost* is dropped for air units,
//! and a unit with the Flag Unit AI strategy gets attack and defense 0. *Available to* is a
//! bit set over `RACE` rows (bit `i` for row `i`).
//!
//! # Evidence
//!
//! The stat block, ability/AI/order labels, tail field identities and the 12-versus-14 terrain
//! layout are verified three ways: the reader's `fread` sequence, the editor's page → row copy
//! (control id → row offset), and the stock `conquests.biq` rows (for example the stealth
//! target list is non-empty on exactly the rows that carry the Stealth Attack action, and
//! `enslave_results_in` is set on exactly the rows with Enslave).
//!
//! **Row revision.** `revision` is `7` in every Conquests row. Rows without it (Civ3 1.x, PTW)
//! store `shield_cost` in tens of shields: the reader (`0x5E5A80`) multiplies the cost by 10
//! when the revision is below 7 and then stamps 7. [`UnitType::shield_cost_in_shields`]
//! applies the same rule.
//!
//! **Terrain bytes.** Conquests added Marsh and Volcano, so a PTW row has 12 bytes and a
//! Conquests row 14. The game reads 14 unconditionally. In the shipped Conquests rules,
//! bytes 12 and 13 (Sea, Ocean) are `1, 0` on the 87 rows converted from PTW and `0, 0` on
//! rows made since; that matches PTW's `require_support` low bytes, so they are most likely
//! conversion residue, not designed data (HYPOTHESIS).

use crate::Version;
use crate::io::{Ctx, Field, Reader, Record, Result, Str, Writer};

/// Row revision written by the Conquests editor (reader `0x5E5A80`).
pub const CURRENT_REVISION: i32 = 7;

/// First version whose rows carry the five action words (`standard_orders` ..
/// `command_mask`). The game translates older rows' packed words when the file version is
/// below `10.0` (`0x595CF7`..`0x595D3F` calling `0x5E5AB0`); the editor introduced the new
/// words in `11.00` (Civ3XEdit 2.00).
pub const ACTION_WORDS_SINCE: Version = Version::new(10, 0);
/// First version with the *Bombard Fx* flag (Civ3XEdit 2.05).
pub const BOMBARD_FX_SINCE: Version = Version::new(11, 5);
/// First version with *Ignore Move Cost* (Civ3XEdit 2.09, "terrain movement bonus").
pub const IGNORE_MOVE_COST_SINCE: Version = Version::new(11, 9);
/// First version with *Req. Support* (Civ3XEdit 2.25).
pub const REQUIRE_SUPPORT_SINCE: Version = Version::new(11, 11);
/// First version with the Conquests tail and the 14 terrain bytes. This is the first version
/// observed in a shipped file; the Conquests editor started at `12.00`, and files from
/// `12.00` to `12.05` (none known) are read as PTW rows with their extra bytes kept.
pub const CONQUESTS_SINCE: Version = Version::new(12, 6);

/// Worker actions that count as terrain work for the reader's worker-strength rule: every
/// bit except Build City, Automate and Join City (`0x5E596E`..`0x5E5A20`).
const TERRAIN_WORK_ACTIONS: u32 = 0x1E7FD;

/// Unit ability dword at body `+0x84` (memory `+0x88`). Bit names are the editor's
/// "Unit Abilities" checklist (**B**, label list at editor offset `0x1B77DA`) cross-checked
/// against the stock rows. Bits `>= 32` live in [`UnitType::abilities_extended`]; the
/// game's flag helper is `0x5E4EF0`.
pub mod ability {
    pub const WHEELED: u32 = 1 << 0;
    /// "Foot Unit": what "Transports Only Foot Units" carries (Warrior, Archer, Marine, ...).
    pub const FOOT_UNIT: u32 = 1 << 1;
    /// Blitz (**A** `combat.md`).
    pub const BLITZ: u32 = 1 << 2;
    pub const CRUISE_MISSILE: u32 = 1 << 3;
    /// "All Terrain As Roads" (Explorer).
    pub const ALL_TERRAIN_AS_ROADS: u32 = 1 << 4;
    pub const RADAR: u32 = 1 << 5;
    /// Amphibious assault (**A** `combat.md`).
    pub const AMPHIBIOUS: u32 = 1 << 6;
    /// "Invisible" (Submarines).
    pub const INVISIBLE: u32 = 1 << 7;
    /// "Transports Only Aircraft" (Carrier).
    pub const TRANSPORTS_ONLY_AIRCRAFT: u32 = 1 << 8;
    pub const DRAFT: u32 = 1 << 9;
    pub const IMMOBILE: u32 = 1 << 10;
    pub const SINKS_IN_SEA: u32 = 1 << 11;
    pub const SINKS_IN_OCEAN: u32 = 1 << 12;
    pub const FLAG_UNIT: u32 = 1 << 13;
    pub const TRANSPORTS_ONLY_FOOT_UNITS: u32 = 1 << 14;
    /// A victory with this unit starts a Golden Age (**A** `combat.md`, "ability 15").
    pub const STARTS_GOLDEN_AGE: u32 = 1 << 15;
    pub const NUCLEAR_WEAPON: u32 = 1 << 16;
    /// Tested by the stacking code (**A** `stacking.md`, `0x5E4EF0` index `0x11`).
    pub const HIDDEN_NATIONALITY: u32 = 1 << 17;
    /// The unit is an Army (a container for other units).
    pub const ARMY: u32 = 1 << 18;
    pub const LEADER: u32 = 1 << 19;
    pub const INFINITE_BOMBARD_RANGE: u32 = 1 << 20;
    pub const STEALTH: u32 = 1 << 21;
    pub const DETECT_INVISIBLE: u32 = 1 << 22;
    pub const TACTICAL_MISSILE: u32 = 1 << 23;
    pub const TRANSPORTS_ONLY_TACTICAL_MISSILES: u32 = 1 << 24;
    pub const RANGED_ATTACK_ANIMATION: u32 = 1 << 25;
    pub const ROTATE_BEFORE_ATTACK: u32 = 1 << 26;
    pub const LETHAL_LAND_BOMBARDMENT: u32 = 1 << 27;
    pub const LETHAL_SEA_BOMBARDMENT: u32 = 1 << 28;
    pub const KING: u32 = 1 << 29;
    pub const REQUIRES_ESCORT: u32 = 1 << 30;
}

/// AI strategy dword at body `+0x88` (editor "AI Strategies" boxes, label list at editor
/// offset `0x1B7018`).
///
/// The editor only keeps a strategy ticked when the unit can do what it implies (its apply
/// routine rebuilds this word from the checkboxes, `0x45D423..0x45D9E7`). The release notes
/// (Civ3XEdit 2.07, 2.08, 2.16) state the rules for the newer ones: *Flag Unit* needs attack,
/// defense, bombard strength and transport capacity 0, the Flag unit ability, the Immobile
/// ability and no Disband order; *King* needs the King ability and no Disband order;
/// *Offense* and *Defense* need the Capture action. Units named on the General Settings page
/// (`RULE`) should have at most one strategy.
pub mod ai {
    pub const OFFENSE: u32 = 1 << 0;
    pub const DEFENSE: u32 = 1 << 1;
    pub const ARTILLERY: u32 = 1 << 2;
    pub const EXPLORE: u32 = 1 << 3;
    pub const ARMY: u32 = 1 << 4;
    pub const CRUISE_MISSILE: u32 = 1 << 5;
    pub const AIR_BOMBARD: u32 = 1 << 6;
    pub const AIR_DEFENSE: u32 = 1 << 7;
    pub const NAVAL_POWER: u32 = 1 << 8;
    pub const AIR_TRANSPORT: u32 = 1 << 9;
    pub const NAVAL_TRANSPORT: u32 = 1 << 10;
    pub const NAVAL_CARRIER: u32 = 1 << 11;
    pub const TERRAFORM: u32 = 1 << 12;
    pub const SETTLE: u32 = 1 << 13;
    pub const LEADER: u32 = 1 << 14;
    pub const TACTICAL_NUKE: u32 = 1 << 15;
    pub const ICBM: u32 = 1 << 16;
    pub const NAVAL_MISSILE_TRANSPORT: u32 = 1 << 17;
    pub const FLAG_UNIT: u32 = 1 << 18;
    pub const KING: u32 = 1 << 19;
}

/// Standard orders ([`UnitType::standard_orders`], editor "Standard Orders" box).
pub mod order {
    pub const SKIP_TURN: u32 = 1 << 0;
    pub const WAIT: u32 = 1 << 1;
    pub const FORTIFY: u32 = 1 << 2;
    pub const DISBAND: u32 = 1 << 3;
    pub const GO_TO: u32 = 1 << 4;
    pub const EXPLORE: u32 = 1 << 5;
    pub const SENTRY: u32 = 1 << 6;
    /// What the row constructor (`0x5E4D00`) puts in the word: the low 28 bits all set.
    pub const ALL: u32 = 0x0FFF_FFFF;
}

/// Special actions ([`UnitType::special_actions`], editor "Special Actions" box). Bits
/// 10-13 and 22+ have no label; bit 28 is tested with Telepad and Teleportable by `0x5C4E70`.
pub mod special {
    pub const LOAD: u32 = 1 << 0;
    pub const UNLOAD: u32 = 1 << 1;
    pub const AIRLIFT: u32 = 1 << 2;
    pub const PILLAGE: u32 = 1 << 3;
    pub const BOMBARD: u32 = 1 << 4;
    pub const AIRDROP: u32 = 1 << 5;
    pub const BUILD_ARMY: u32 = 1 << 6;
    pub const FINISH_IMPROVEMENTS: u32 = 1 << 7;
    pub const UPGRADE_UNIT: u32 = 1 << 8;
    pub const CAPTURE: u32 = 1 << 9;
    pub const TELEPAD: u32 = 1 << 14;
    pub const TELEPORTABLE: u32 = 1 << 15;
    /// Stealth Attack: the unit picks its victim from a stack among
    /// [`UnitType::stealth_attack_targets`]. The editor binds control 1855 to this bit
    /// (`0x45E90B`: `byte [row+0xAE] & 1`, memory offsets); it is set on exactly the stock rows
    /// with a non-empty target list.
    pub const STEALTH_ATTACK: u32 = 1 << 16;
    pub const CHARM: u32 = 1 << 17;
    /// Enslave: a won fight may turn the loser into [`UnitType::enslave_results_in`]
    /// (33 % chance, `combat.md`).
    pub const ENSLAVE: u32 = 1 << 18;
    pub const COLLATERAL_DAMAGE: u32 = 1 << 19;
    pub const SACRIFICE: u32 = 1 << 20;
    pub const SCIENCE_AGE: u32 = 1 << 21;
}

/// Worker/Engineer actions ([`UnitType::worker_actions`]).
pub mod worker {
    pub const BUILD_COLONY: u32 = 1 << 0;
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
    pub const BUILD_AIRFIELD: u32 = 1 << 13;
    pub const BUILD_RADAR_TOWER: u32 = 1 << 14;
    pub const BUILD_OUTPOST: u32 = 1 << 15;
    /// Editor control 1262 "Build Barricade" (`0x45EA6C`: `byte [row+0xB2] & 1`, memory
    /// offsets); only the stock Worker has it.
    pub const BUILD_BARRICADE: u32 = 1 << 16;
}

/// Air missions ([`UnitType::air_missions`]).
pub mod air {
    pub const BOMBING: u32 = 1 << 0;
    pub const RECON: u32 = 1 << 1;
    pub const INTERCEPTION: u32 = 1 << 2;
    pub const REBASE: u32 = 1 << 3;
    pub const PRECISION_BOMBING: u32 = 1 << 4;
}

/// [`UnitType::unit_class`] values (**A** `combat.md`; editor "Class" radio buttons).
pub mod class {
    pub const LAND: i32 = 0;
    pub const SEA: i32 = 1;
    pub const AIR: i32 = 2;
}

/// A length-prefixed list of indices (reader `0x5E4B70`): a leading dword that is always `1`
/// in shipped files (the constructor's default), a count, then the entries.
///
/// The game tolerates a count larger than the bytes left in the row (it reads while at least
/// four bytes remain), so `count` and `entries.len()` are kept separately for an exact round
/// trip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexList {
    /// First dword on disk (`1`).
    pub marker: i32,
    /// Count dword on disk.
    pub count: i32,
    /// Entries actually present.
    pub entries: Vec<i32>,
}

impl Default for IndexList {
    fn default() -> Self {
        Self {
            marker: 1,
            count: 0,
            entries: Vec::new(),
        }
    }
}

impl IndexList {
    /// Membership test (`0x5E4CD0`).
    pub fn contains(&self, index: i32) -> bool {
        self.entries.contains(&index)
    }

    fn read(r: &mut Reader<'_>) -> Self {
        let Some(marker) = r.i32() else {
            return Self::default();
        };
        let Some(count) = r.i32() else {
            return Self {
                marker,
                ..Self::default()
            };
        };
        let mut entries = Vec::new();
        for _ in 0..count.max(0) {
            if r.remaining() < 4 {
                break;
            }
            if let Some(v) = r.i32() {
                entries.push(v);
            }
        }
        Self {
            marker,
            count,
            entries,
        }
    }

    fn write(&self, w: &mut Writer) {
        w.i32(self.marker);
        w.i32(self.count);
        for &e in &self.entries {
            w.i32(e);
        }
    }
}

/// One unit prototype (`PRTO` row).
#[derive(Clone, Debug, PartialEq)]
pub struct UnitType {
    /// "Zone of Control" checkbox (editor control 1535; the editor reads it from the first
    /// body dword, `0x45E2B2`). `0` or `1`; set on 16 stock rows (Cavalry, Tank, Mech Infantry,
    /// Modern Armor, Army, Cossack, Keshik, ...). Settles the open question in `editor.md`:
    /// ZOC is this dword, not an ability bit. Which polarity it has in play is not decoded.
    pub zone_of_control: u32,
    pub name: Str<32>,
    pub civilopedia_entry: Str<32>,
    /// `0x44`. Archers have 1, Catapults 4.
    pub bombard_strength: i32,
    /// `0x48`.
    pub bombard_range: i32,
    /// `0x4C`. Galleon 4, Army 3.
    pub transport_capacity: i32,
    /// `0x50`. Shields, or tens of shields when [`Self::revision`] is below 7; see
    /// [`Self::shield_cost_in_shields`].
    pub shield_cost: i32,
    /// `0x54`.
    pub defense: i32,
    /// `0x58`. Icon cell in the unit sheet.
    pub icon: i32,
    /// `0x5C`.
    pub attack: i32,
    /// `0x60`. Air units' sortie range (Fighter 6, Bomber 10).
    pub operational_range: i32,
    /// `0x64`. Citizens consumed (Settler 2).
    pub population_cost: i32,
    /// `0x68`.
    pub rate_of_fire: i32,
    /// `0x6C`. Movement points.
    pub movement: i32,
    /// `0x70`. `TECH` index, `-1` none.
    pub required_tech: i32,
    /// `0x74`. `PRTO` index, `-1` none.
    pub upgrade_to: i32,
    /// `0x78`. `GOOD` index, `-1` none.
    pub required_resource_1: i32,
    /// `0x7C`.
    pub required_resource_2: i32,
    /// `0x80`.
    pub required_resource_3: i32,
    /// `0x84`. See [`ability`].
    pub abilities: u32,
    /// `0x88`. See [`ai`].
    pub ai_strategies: u32,
    /// `0x8C`. "Available to" checklist (control 1233; the editor sets bit `i` for each selected
    /// `RACE` row `i`): bit `i` set when `RACE` row `i` may build the unit. `-2` (every
    /// civilization but row 0, the barbarians) on most stock rows; a new unit starts at `0`.
    pub available_to_civs: i32,
    /// `0x90`. The Civ3 1.x packed unit actions, first word: standard orders in bits 0-4,
    /// special actions in bits 5-13, worker actions in bits 14-26 (see
    /// [`Self::actions_from_legacy`]). The game translates it when the file is older than
    /// `10.0` (`0x5E5AB0`); in newer files it is uninitialised editor memory (`0xCCCCCCCC`)
    /// or a stale value and means nothing.
    pub legacy_orders_a: i32,
    /// `0x94`. Second packed word: air missions in bits 0-4, command mask in bits 5-18.
    pub legacy_orders_b: i32,
    /// `0x98`. See [`class`].
    pub unit_class: i32,
    /// `0x9C`. For the extra rows that give a unit a second AI strategy (stock rows 124..=140:
    /// a second Rifleman with Defense instead of Offense, a second Nuclear Submarine, ...): the
    /// `PRTO` index of the primary row of the same name. `-1` on every ordinary row. The loader
    /// counts the `-1` rows as the scenario's real unit types (`0x595CD6`, scenario `+0x874`).
    pub alt_strategy_of: i32,
    /// `0xA0`. Added to the experience level's base HP; the editor allows -20..=20.
    pub hit_point_bonus: i32,
    /// `0xA4`. See [`order`].
    pub standard_orders: u32,
    /// `0xA8`. See [`special`].
    pub special_actions: u32,
    /// `0xAC`. See [`worker`].
    pub worker_actions: u32,
    /// `0xB0`. See [`air`].
    pub air_missions: u32,
    /// `0xB4`. A "which command buttons apply" mask the editor recomputes from the other
    /// action words every time it saves the unit (`0x45DF03..0x45E15B`, see
    /// [`Self::derived_command_mask`]). Derived, not independent data. The game's own loader
    /// (`0x595D5B..0x595F2A`) builds it with an older, simpler rule for files older than `2.11`,
    /// none of which has a `PRTO` section in the corpus, so that rule is not modelled.
    pub command_mask: u32,
    /// `0xB8`. "Bombard Fx" checkbox (editor control 1562); `1` on units with bombard
    /// animations (Catapult, Cannon, Battleship, ...).
    pub bombard_fx: i32,
    /// `0xBC`. "Ignore Move Cost" checklist: one flag byte per `TERR` row (12 in a PTW row, 14
    /// in a Conquests row); the game's lookup is `0x5BC830`. Chasqui Scout has Hills and
    /// Mountains.
    pub ignore_move_cost: Vec<u8>,
    /// "Req. Support" checkbox (control 1565): the unit costs upkeep. `1` on the 110 ordinary
    /// stock rows, `0` on the 31 named great leaders (**A** `economy.md`, `0x55D030`).
    pub require_support: i32,
    /// Row revision, `7` in Conquests rows (constructor `0x5E4D00`, reader `0x5E5A80`). `0`
    /// here for rows that do not carry it.
    pub revision: i32,
    /// "Telepad Range" (control 1186/1187, the editor keeps `0..=100`). `0` in stock rows.
    pub telepad_range: i32,
    /// "Legal Unit Telepads" (control 1923): `PRTO` indices of the unit types that may serve as
    /// telepad for this unit; tested by `0x5C4E70` when this unit has Teleportable.
    pub legal_unit_telepads: IndexList,
    /// "Enslave Results In" (control 1924): `PRTO` index of the unit a won fight turns the loser
    /// into, `-1` none. Set on the rows with [`special::ENSLAVE`] (Javelin Thrower -> Worker;
    /// Man-O-War and Privateer -> themselves).
    pub enslave_results_in: i32,
    /// "Stealth Attack Targets" (control 1925): `PRTO` indices the unit may pick from a stack
    /// (`0x5B64C0`). 18 entries on Submarine, 92 on Stealth Fighter.
    pub stealth_attack_targets: IndexList,
    /// "Legal Bldg Telepads" (control 1926): `BLDG` indices (label only; empty in all stock
    /// rows).
    pub legal_building_telepads: IndexList,
    /// "Create Craters" checkbox (control 1567). Set on bombarding and air units.
    pub create_craters: u8,
    /// "Worker Strength": work-rate multiplier; the editor shows it times 100. Stock Worker 1.0.
    /// When a row lacks the field the reader derives `1.0` for units with worker actions or the
    /// Terraform AI strategy, else `0`.
    pub worker_strength: f32,
    /// Ability bits 32 and up (`0x5E4EF0`); `0` in stock rows.
    pub abilities_extended: u32,
    /// "Air Defense Str." (control 1193): AEGIS Cruiser 3, Mobile SAM 4.
    pub air_defense: i32,
    /// Bytes after the last modelled field.
    pub extra: Vec<u8>,
}

/// The game's row constructor (`0x5E4D00`): what *Add* creates in the editor, and the value
/// every field of a short row keeps. The two legacy words are not initialised by the
/// constructor; they are `0` here.
impl Default for UnitType {
    fn default() -> Self {
        Self {
            zone_of_control: 0,
            name: Str::default(),
            civilopedia_entry: Str::default(),
            bombard_strength: 0,
            bombard_range: 0,
            transport_capacity: 0,
            shield_cost: 0,
            defense: 0,
            icon: 0,
            attack: 0,
            operational_range: 0,
            population_cost: 0,
            rate_of_fire: 0,
            movement: 1,
            required_tech: -1,
            upgrade_to: -1,
            required_resource_1: -1,
            required_resource_2: -1,
            required_resource_3: -1,
            abilities: 0,
            ai_strategies: 0,
            available_to_civs: 0,
            legacy_orders_a: 0,
            legacy_orders_b: 0,
            unit_class: class::LAND,
            alt_strategy_of: -1,
            hit_point_bonus: 0,
            standard_orders: order::ALL,
            special_actions: 0,
            worker_actions: 0,
            air_missions: 0,
            command_mask: 0,
            bombard_fx: 0,
            ignore_move_cost: vec![0; 14],
            require_support: 1,
            revision: CURRENT_REVISION,
            telepad_range: 0,
            legal_unit_telepads: IndexList::default(),
            enslave_results_in: -1,
            stealth_attack_targets: IndexList::default(),
            legal_building_telepads: IndexList::default(),
            create_craters: 0,
            worker_strength: 1.0,
            abilities_extended: 0,
            air_defense: 0,
            extra: Vec::new(),
        }
    }
}

impl UnitType {
    /// Shield cost in shields. Rows older than revision 7 store tens of shields and the game
    /// multiplies them by 10 on load (`0x5E5A80`).
    pub fn shield_cost_in_shields(&self) -> i32 {
        if self.revision < CURRENT_REVISION {
            self.shield_cost * 10
        } else {
            self.shield_cost
        }
    }

    /// Number of [`Self::ignore_move_cost`] bytes in a row of this file version.
    pub fn terrain_byte_count(version: Version) -> usize {
        if version >= CONQUESTS_SINCE { 14 } else { 12 }
    }

    /// The five action words of a Civ3 1.x row, decoded from its two packed words
    /// `legacy_orders_a`/`b` the way the game does for files older than `10.0` (`0x5E5AB0`):
    /// `[standard_orders, special_actions, worker_actions, air_missions, command_mask]`.
    ///
    /// The packing is `a = orders | special << 5 | worker << 14` (5, 9 and 13 bits) and
    /// `b = air | mask << 5` (5 and 14 bits). The bit meanings of each field are the ones of
    /// [`order`], [`special`], [`worker`] and [`air`]; the Civ3 1.x rows decode into exactly
    /// the stock actions (Settler: Build City and Join City; Catapult: Load, Bombard, Upgrade).
    pub fn actions_from_legacy(a: u32, b: u32) -> [u32; 5] {
        [
            a & 0x1F,
            (a >> 5) & 0x1FF,
            (a >> 14) & 0x1FFF,
            b & 0x1F,
            (b >> 5) & 0x3FFF,
        ]
    }

    /// The `command_mask` the Conquests editor stores for this unit's other fields
    /// (`0x45DF03..0x45E15B`, run on every save). Always has bit `0x10000`; the rest:
    ///
    /// | bit | condition |
    /// |---|---|
    /// | `0x1` | Sentry order |
    /// | `0x2` | Bombard action |
    /// | `0x4` | Build Colony and Build Road |
    /// | `0x8`, `0x20` | Build Road |
    /// | `0x10` | Build Railroad |
    /// | `0x40` | Irrigate |
    /// | `0x80` | Clear Forest |
    /// | `0x100` | Clear Jungle |
    /// | `0x200` | Clear Pollution |
    /// | `0x400`, `0x800` | Automate and the Terraform AI strategy |
    /// | `0x1000` | Bombing mission |
    /// | `0x2000` | Precision Bombing mission |
    /// | `0x4000` | Automate |
    /// | `0x8000` | Go To order or Re-base mission |
    ///
    /// Every shipped Conquests row has exactly this value. Of the Play the World rows, 11 in
    /// four scenarios differ in bits `0x4` and `0xC00` (the PTW editor used the looser tests
    /// "Build Colony or Build Road" and "Automate").
    pub fn derived_command_mask(&self) -> u32 {
        let has = |word: u32, bit: u32| word >> bit & 1 != 0;
        let (orders, special, work, air) = (
            self.standard_orders,
            self.special_actions,
            self.worker_actions,
            self.air_missions,
        );
        let mut mask = 0x10000;
        let mut set = |cond: bool, bits: u32| {
            if cond {
                mask |= bits;
            }
        };
        set(has(orders, 6), 0x1);
        set(has(special, 4), 0x2);
        set(has(work, 0) && has(work, 2), 0x4);
        set(has(work, 2), 0x8 | 0x20);
        set(has(work, 3), 0x10);
        set(has(work, 6), 0x40);
        set(has(work, 7), 0x80);
        set(has(work, 8), 0x100);
        set(has(work, 10), 0x200);
        set(
            has(work, 11) && self.ai_strategies & ai::TERRAFORM != 0,
            0x400 | 0x800,
        );
        set(has(air, 0), 0x1000);
        set(has(air, 4), 0x2000);
        set(has(work, 11), 0x4000);
        set(has(orders, 4) || has(air, 3), 0x8000);
        mask
    }

    /// The worker strength the reader assigns a row that lacks the field (`0x5E596E`):
    /// `1.0` for units with the Terraform AI strategy or any terrain-working action (all
    /// worker actions but Build City, Automate and Join City), `0.0` for the rest.
    pub fn derived_worker_strength(&self) -> f32 {
        if self.ai_strategies & ai::TERRAFORM != 0
            || self.worker_actions & TERRAIN_WORK_ACTIONS != 0
        {
            1.0
        } else {
            0.0
        }
    }

    /// What the reader (`0x5E54B0`) uses for a row without the Conquests tail.
    fn absent_conquests_tail(u: &mut UnitType) {
        u.revision = 0;
        u.telepad_range = 0;
        u.legal_unit_telepads = IndexList::default();
        u.enslave_results_in = -1;
        u.stealth_attack_targets = IndexList::default();
        u.legal_building_telepads = IndexList::default();
        u.create_craters = 0;
        u.worker_strength = u.derived_worker_strength();
        u.abilities_extended = 0;
        u.air_defense = 0;
    }

    fn read_conquests_tail(r: &mut Reader<'_>, u: &mut UnitType) {
        Self::absent_conquests_tail(u);
        if let Some(v) = r.i32() {
            u.revision = v;
        }
        if let Some(v) = r.i32() {
            u.telepad_range = v;
        }
        u.legal_unit_telepads = IndexList::read(r);
        if let Some(v) = r.i32() {
            u.enslave_results_in = v;
        }
        u.stealth_attack_targets = IndexList::read(r);
        u.legal_building_telepads = IndexList::read(r);
        if let Some(v) = r.u8() {
            u.create_craters = v;
        }
        if let Some(v) = r.f32() {
            u.worker_strength = v;
        }
        if let Some(v) = r.u32() {
            u.abilities_extended = v;
        }
        if let Some(v) = r.i32() {
            u.air_defense = v;
        }
    }

    fn write_conquests_tail(&self, w: &mut Writer) {
        w.i32(self.revision);
        w.i32(self.telepad_range);
        self.legal_unit_telepads.write(w);
        w.i32(self.enslave_results_in);
        self.stealth_attack_targets.write(w);
        self.legal_building_telepads.write(w);
        w.u8(self.create_craters);
        w.f32(self.worker_strength);
        w.u32(self.abilities_extended);
        w.i32(self.air_defense);
    }
}

impl Record for UnitType {
    const TAG: [u8; 4] = *b"PRTO";

    fn read(r: &mut Reader<'_>, ctx: &Ctx) -> Result<Self> {
        let mut u = UnitType::default();
        macro_rules! field {
            ($f:ident, $ty:ident) => {
                if r.remaining() >= <$ty as Field>::SIZE {
                    u.$f = r.$ty().unwrap_or(0);
                }
            };
            ($f:ident, str $n:expr) => {
                if r.remaining() >= $n {
                    u.$f = r.str::<$n>().unwrap_or_default();
                }
            };
        }

        field!(zone_of_control, u32);
        field!(name, str 32);
        field!(civilopedia_entry, str 32);
        field!(bombard_strength, i32);
        field!(bombard_range, i32);
        field!(transport_capacity, i32);
        field!(shield_cost, i32);
        field!(defense, i32);
        field!(icon, i32);
        field!(attack, i32);
        field!(operational_range, i32);
        field!(population_cost, i32);
        field!(rate_of_fire, i32);
        field!(movement, i32);
        field!(required_tech, i32);
        field!(upgrade_to, i32);
        field!(required_resource_1, i32);
        field!(required_resource_2, i32);
        field!(required_resource_3, i32);
        field!(abilities, u32);
        field!(ai_strategies, u32);
        field!(available_to_civs, i32);
        field!(legacy_orders_a, i32);
        field!(legacy_orders_b, i32);
        field!(unit_class, i32);
        field!(alt_strategy_of, i32);
        field!(hit_point_bonus, i32);

        let v = ctx.version;
        if v >= ACTION_WORDS_SINCE && r.remaining() >= 20 {
            u.standard_orders = r.u32().unwrap_or(0);
            u.special_actions = r.u32().unwrap_or(0);
            u.worker_actions = r.u32().unwrap_or(0);
            u.air_missions = r.u32().unwrap_or(0);
            u.command_mask = r.u32().unwrap_or(0);
        }
        if v >= BOMBARD_FX_SINCE {
            field!(bombard_fx, i32);
        }

        let terrains = Self::terrain_byte_count(v);
        u.ignore_move_cost = vec![0; terrains];
        if v >= IGNORE_MOVE_COST_SINCE
            && let Some(bytes) = r.take(terrains)
        {
            u.ignore_move_cost.copy_from_slice(bytes);
        }
        if v >= REQUIRE_SUPPORT_SINCE {
            field!(require_support, i32);
        }

        if v >= CONQUESTS_SINCE {
            Self::read_conquests_tail(r, &mut u);
        } else {
            Self::absent_conquests_tail(&mut u);
        }
        if v < ACTION_WORDS_SINCE {
            // Civ3 1.x row: the actions live in the two packed words.
            let words =
                Self::actions_from_legacy(u.legacy_orders_a as u32, u.legacy_orders_b as u32);
            [
                u.standard_orders,
                u.special_actions,
                u.worker_actions,
                u.air_missions,
                u.command_mask,
            ] = words;
        }

        u.extra = r.rest().to_vec();
        Ok(u)
    }

    fn write(&self, w: &mut Writer, ctx: &Ctx) {
        w.u32(self.zone_of_control);
        self.name.write(w);
        self.civilopedia_entry.write(w);
        w.i32(self.bombard_strength);
        w.i32(self.bombard_range);
        w.i32(self.transport_capacity);
        w.i32(self.shield_cost);
        w.i32(self.defense);
        w.i32(self.icon);
        w.i32(self.attack);
        w.i32(self.operational_range);
        w.i32(self.population_cost);
        w.i32(self.rate_of_fire);
        w.i32(self.movement);
        w.i32(self.required_tech);
        w.i32(self.upgrade_to);
        w.i32(self.required_resource_1);
        w.i32(self.required_resource_2);
        w.i32(self.required_resource_3);
        w.u32(self.abilities);
        w.u32(self.ai_strategies);
        w.i32(self.available_to_civs);
        w.i32(self.legacy_orders_a);
        w.i32(self.legacy_orders_b);
        w.i32(self.unit_class);
        w.i32(self.alt_strategy_of);
        w.i32(self.hit_point_bonus);

        let v = ctx.version;
        if v >= ACTION_WORDS_SINCE {
            w.u32(self.standard_orders);
            w.u32(self.special_actions);
            w.u32(self.worker_actions);
            w.u32(self.air_missions);
            w.u32(self.command_mask);
        }
        if v >= BOMBARD_FX_SINCE {
            w.i32(self.bombard_fx);
        }
        if v >= IGNORE_MOVE_COST_SINCE {
            for i in 0..Self::terrain_byte_count(v) {
                w.u8(self.ignore_move_cost.get(i).copied().unwrap_or(0));
            }
        }
        if v >= REQUIRE_SUPPORT_SINCE {
            w.i32(self.require_support);
        }
        if v >= CONQUESTS_SINCE {
            self.write_conquests_tail(w);
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
    use crate::io::Reader;

    /// Every `PRTO` row of the first corpus file whose name contains `needle`.
    fn units_of(needle: &str) -> Option<Vec<UnitType>> {
        let files = crate::corpus::load_all();
        let file = files
            .iter()
            .filter_map(|r| r.as_ref().ok())
            .find(|f| f.name().contains(needle))?;
        let sec = file.raw.section(b"PRTO")?;
        Some(
            sec.rows
                .iter()
                .map(|r| UnitType::read(&mut Reader::new(file.raw.row(r)), &file.ctx()).unwrap())
                .collect(),
        )
    }

    fn conquests_units() -> Option<Vec<UnitType>> {
        units_of("conquests.biq")
    }

    fn get<'a>(units: &'a [UnitType], name: &str) -> &'a UnitType {
        units
            .iter()
            .find(|u| u.name.text() == name)
            .unwrap_or_else(|| panic!("no unit {name}"))
    }

    #[test]
    fn corpus_roundtrip() {
        let st = crate::corpus::check_roundtrip::<UnitType>();
        assert_eq!(st.with_extra, 0, "{}", st.report());
        assert_eq!(st.short, 0, "{}", st.report());
    }

    #[test]
    fn conquests_stock_unit_semantics() {
        let Some(units) = conquests_units() else {
            return;
        };

        let w = get(&units, "Warrior");
        assert_eq!(w.shield_cost, 10);
        assert_eq!(w.attack, 1);
        assert_eq!(w.defense, 1);
        assert_eq!(w.movement, 1);
        assert_eq!(w.upgrade_to, 9);
        assert_ne!(w.abilities & ability::FOOT_UNIT, 0);

        let a = get(&units, "Archer");
        assert_eq!((a.attack, a.defense), (2, 1));
        assert_eq!((a.bombard_strength, a.rate_of_fire), (1, 1));

        let t = get(&units, "Tank");
        assert_eq!(
            (t.attack, t.defense, t.movement, t.shield_cost),
            (16, 8, 2, 100)
        );
        assert_eq!(
            (
                t.required_resource_1,
                t.required_resource_2,
                t.required_resource_3
            ),
            (4, 5, -1)
        );
        assert_eq!(t.upgrade_to, 21);
        assert_ne!(t.abilities & ability::BLITZ, 0);
        assert_ne!(t.abilities & ability::RANGED_ATTACK_ANIMATION, 0);
        assert_eq!(t.zone_of_control, 1);

        let settler = get(&units, "Settler");
        assert_eq!((settler.population_cost, settler.movement), (2, 1));
        assert_ne!(settler.ai_strategies & ai::SETTLE, 0);
        assert_ne!(settler.worker_actions & worker::BUILD_CITY, 0);

        let cat = get(&units, "Catapult");
        assert_eq!(
            (cat.bombard_strength, cat.bombard_range, cat.bombard_fx),
            (4, 1, 1)
        );
        assert_ne!(cat.ai_strategies & ai::ARTILLERY, 0);

        let dest = get(&units, "Destroyer");
        assert_eq!(dest.unit_class, class::SEA);
        assert_eq!((dest.movement, dest.rate_of_fire), (8, 2));

        let f = get(&units, "Fighter");
        assert_eq!(f.unit_class, class::AIR);
        assert_eq!(f.operational_range, 6);
        assert_ne!(f.air_missions & air::INTERCEPTION, 0);

        let army = get(&units, "Army");
        assert_ne!(army.abilities & ability::ARMY, 0);
        assert_eq!(army.transport_capacity, 3);

        let leader = get(&units, "Leader");
        assert_eq!(leader.shield_cost, 0);
        assert_ne!(leader.abilities & ability::LEADER, 0);
        assert_ne!(leader.ai_strategies & ai::LEADER, 0);
        assert_ne!(leader.special_actions & special::BUILD_ARMY, 0);
        assert_ne!(leader.special_actions & special::SCIENCE_AGE, 0);
    }

    #[test]
    fn ability_bits_follow_the_editor_labels() {
        let Some(units) = conquests_units() else {
            return;
        };
        let has = |name: &str, bit: u32| get(&units, name).abilities & bit != 0;
        assert!(has("Chariot", ability::WHEELED));
        assert!(has("Explorer", ability::ALL_TERRAIN_AS_ROADS));
        assert!(has("Submarine", ability::INVISIBLE));
        assert!(has("Carrier", ability::TRANSPORTS_ONLY_AIRCRAFT));
        assert!(has("Leader", ability::LEADER));
        assert!(has("ICBM", ability::NUCLEAR_WEAPON));
        assert!(has("Stealth Fighter", ability::STEALTH));
        // Hidden Nationality and Leader are different bits (the stacking code tests 0x11).
        assert_ne!(ability::HIDDEN_NATIONALITY, ability::LEADER);
    }

    #[test]
    fn tail_fields_are_what_the_editor_says() {
        let Some(units) = conquests_units() else {
            return;
        };
        for u in &units {
            let n = u.name.text();
            assert_eq!(u.revision, CURRENT_REVISION, "{n}");
            assert_eq!(u.ignore_move_cost.len(), 14, "{n}");
            // Stealth Attack action <=> a stealth target list.
            assert_eq!(
                u.special_actions & special::STEALTH_ATTACK != 0,
                !u.stealth_attack_targets.entries.is_empty(),
                "{n}"
            );
            // Enslave action <=> an enslave result unit.
            assert_eq!(
                u.special_actions & special::ENSLAVE != 0,
                u.enslave_results_in >= 0,
                "{n}"
            );
        }

        // Req. Support: ordinary units pay, the named great leaders do not.
        assert_eq!(get(&units, "Worker").require_support, 1);
        assert_eq!(get(&units, "Caesar").require_support, 0);
        assert_eq!(units.iter().filter(|u| u.require_support == 0).count(), 31);

        assert_eq!(get(&units, "Mobile SAM").air_defense, 4);
        assert_eq!(get(&units, "AEGIS Cruiser").air_defense, 3);
        assert_eq!(get(&units, "Worker").worker_strength, 1.0);
        assert_ne!(
            get(&units, "Worker").worker_actions & worker::BUILD_BARRICADE,
            0
        );

        let sf = get(&units, "Stealth Fighter");
        assert_eq!(sf.stealth_attack_targets.entries.len(), 92);
        assert!(
            sf.stealth_attack_targets.contains(
                units
                    .iter()
                    .position(|u| u.name.text() == "Mobile SAM")
                    .unwrap() as i32
            )
        );

        let jt = get(&units, "Javelin Thrower");
        assert_eq!(
            units[jt.enslave_results_in as usize].name.text(),
            "Worker",
            "Javelin Throwers enslave into Workers"
        );

        let chasqui = get(&units, "Chasqui Scout");
        assert_eq!(chasqui.ignore_move_cost[5], 1, "Hills");
        assert_eq!(chasqui.ignore_move_cost[6], 1, "Mountains");
    }

    #[test]
    fn alt_strategy_rows_duplicate_a_primary_row() {
        let Some(units) = conquests_units() else {
            return;
        };
        let mut alternates = 0;
        for u in &units {
            if u.alt_strategy_of < 0 {
                continue;
            }
            alternates += 1;
            let primary = &units[u.alt_strategy_of as usize];
            assert_eq!(primary.name.text(), u.name.text());
            assert_eq!(primary.alt_strategy_of, -1);
            assert_ne!(primary.ai_strategies, u.ai_strategies);
        }
        assert_eq!(alternates, 17);
    }

    #[test]
    fn old_rows_store_tens_of_shields() {
        let Some(ptw) = units_of("Ancient_Mediterranean") else {
            return;
        };
        let w = get(&ptw, "Warrior");
        assert_eq!(w.revision, 0);
        assert_eq!(w.shield_cost, 1);
        assert_eq!(w.shield_cost_in_shields(), 10);
        assert_eq!(w.ignore_move_cost.len(), 12);
        let gallic = get(&ptw, "Gallic Swordsman");
        assert_eq!(gallic.ignore_move_cost[7], 1, "Forest");
        assert_eq!(get(&ptw, "Worker").require_support, 1);

        let Some(conq) = conquests_units() else {
            return;
        };
        let w = get(&conq, "Warrior");
        assert_eq!(w.shield_cost_in_shields(), 10);
    }

    #[test]
    fn sea_dog_enslaves_into_itself() {
        let Some(units) = units_of("Discovery") else {
            return;
        };
        let (i, dog) = units
            .iter()
            .enumerate()
            .find(|(_, u)| u.name.text().contains("Sea Dog"))
            .expect("Elizabethan Sea Dog");
        assert_ne!(dog.special_actions & special::ENSLAVE, 0);
        assert_eq!(dog.enslave_results_in, i as i32);
    }

    /// Every `(file, unit)` of the corpus.
    fn all_units() -> Vec<(crate::corpus::CorpusFile, Vec<UnitType>)> {
        crate::corpus::files()
            .into_iter()
            .filter_map(|f| {
                let sec = f.raw.section(b"PRTO")?;
                let units = sec
                    .rows
                    .iter()
                    .map(|r| UnitType::read(&mut Reader::new(f.raw.row(r)), &f.ctx()).unwrap())
                    .collect();
                Some((f, units))
            })
            .collect()
    }

    /// The constructor `0x5E4D00`: movement 1, nothing required, every standard order,
    /// supported, revision 7, worker strength 1.
    #[test]
    fn default_is_the_game_constructor() {
        let u = UnitType::default();
        assert_eq!(u.movement, 1);
        assert_eq!(
            (
                u.required_tech,
                u.upgrade_to,
                u.alt_strategy_of,
                u.enslave_results_in
            ),
            (-1, -1, -1, -1)
        );
        assert_eq!(
            [
                u.required_resource_1,
                u.required_resource_2,
                u.required_resource_3
            ],
            [-1; 3]
        );
        assert_eq!(u.standard_orders, 0x0FFF_FFFF);
        assert_eq!(u.available_to_civs, 0);
        assert_eq!((u.require_support, u.revision), (1, CURRENT_REVISION));
        assert_eq!(u.worker_strength, 1.0);
        assert_eq!(u.legal_unit_telepads, IndexList::default());
        assert_eq!(u.ignore_move_cost, vec![0; 14]);
    }

    /// Each group of the row appears at the version the editor's release notes give.
    #[test]
    fn row_length_grows_with_the_version() {
        let unit = UnitType {
            command_mask: 0x10000,
            ..UnitType::default()
        };
        let write = |major, minor| {
            let mut w = Writer::new();
            let ctx = Ctx {
                version: Version::new(major, minor),
            };
            unit.write(&mut w, &ctx);
            (w.buf, ctx)
        };
        for (version, len) in [
            ((4, 1), 164),
            ((9, 99), 164),
            ((10, 0), 184),
            ((11, 0), 184),
            ((11, 4), 184),
            ((11, 5), 188),
            ((11, 8), 188),
            ((11, 9), 200),
            ((11, 10), 200),
            ((11, 11), 204),
            ((11, 18), 204),
            ((12, 5), 204),
            ((12, 6), 255),
            ((12, 8), 255),
        ] {
            let (bytes, ctx) = write(version.0, version.1);
            assert_eq!(bytes.len(), len, "{}", ctx.version);
            // What was written reads back and writes the same bytes.
            let back = UnitType::read(&mut Reader::new(&bytes), &ctx).unwrap();
            assert!(back.extra.is_empty(), "{}", ctx.version);
            let mut again = Writer::new();
            back.write(&mut again, &ctx);
            assert_eq!(again.buf, bytes, "{}", ctx.version);
        }
    }

    /// A Civ3 1.x row keeps its actions in two packed words; the reader decodes them the
    /// way `0x5E5AB0` does, and the stock rows decode into the actions the game has.
    #[test]
    fn civ3_1x_rows_decode_their_packed_actions() {
        assert_eq!(
            UnitType::actions_from_legacy(0x0400_803F, 0),
            [0x1F, 0x1, 0x1002, 0, 0]
        );
        let Some(units) = units_of("civ3mod.bic") else {
            return;
        };
        let settler = get(&units, "Settler");
        assert_eq!(settler.standard_orders, 0x1F);
        assert_eq!(settler.special_actions, special::LOAD);
        assert_eq!(
            settler.worker_actions,
            worker::BUILD_CITY | worker::JOIN_CITY
        );

        let worker_unit = get(&units, "Worker");
        assert_eq!(
            worker_unit.worker_actions, 0x1FFD,
            "everything but Build City"
        );
        assert_eq!(worker_unit.command_mask, 0xFFC);

        let catapult = get(&units, "Catapult");
        assert_eq!(
            catapult.special_actions,
            special::LOAD | special::BOMBARD | special::UPGRADE_UNIT
        );
        assert_eq!(catapult.command_mask, 0x2, "Bombard");

        let fighter = get(&units, "Fighter");
        assert_eq!(
            fighter.air_missions,
            air::BOMBING | air::RECON | air::INTERCEPTION | air::REBASE
        );
        assert_eq!(
            fighter.standard_orders, 0xF,
            "no Explore or Sentry for aircraft"
        );
        assert_eq!(fighter.command_mask, 0x1000, "Bombing");

        // A Civ3 1.x row has none of the later fields: constructor defaults apply.
        assert_eq!(settler.require_support, 1);
        assert_eq!(settler.bombard_fx, 0);
        assert_eq!(settler.revision, 0);
        assert_eq!(settler.shield_cost_in_shields(), settler.shield_cost * 10);
        // Every row decodes, none keeps the constructor's all-orders default.
        for u in &units {
            assert_ne!(u.standard_orders, order::ALL, "{}", u.name.text());
        }
    }

    /// The editor derives `command_mask` from the other action words; the table in
    /// [`UnitType::derived_command_mask`] is exact in every Conquests row and in all but 11
    /// Play the World rows.
    #[test]
    fn command_mask_is_derived_from_the_actions() {
        let (mut rows, mut outliers) = (0, 0);
        for (f, units) in all_units() {
            if f.version.major < 11 {
                continue;
            }
            for u in &units {
                rows += 1;
                if u.derived_command_mask() != u.command_mask {
                    assert!(
                        f.version.major == 11,
                        "{} / {}: stored {:#x}, derived {:#x}",
                        f.name(),
                        u.name.text(),
                        u.command_mask,
                        u.derived_command_mask()
                    );
                    // Only the two bits the PTW editor tested more loosely.
                    assert_eq!(
                        (u.derived_command_mask() ^ u.command_mask) & !0xC04,
                        0,
                        "{} / {}",
                        f.name(),
                        u.name.text()
                    );
                    outliers += 1;
                }
            }
        }
        if rows > 0 {
            assert!(rows > 3000, "{rows}");
            assert_eq!(outliers, 11);
        }
    }

    /// Where a row has no worker strength, the reader derives it (`0x5E596E`).
    #[test]
    fn worker_strength_is_derived_for_rows_without_it() {
        let worker_unit = UnitType {
            worker_actions: worker::BUILD_ROAD,
            ..UnitType::default()
        };
        assert_eq!(worker_unit.derived_worker_strength(), 1.0);
        let settler = UnitType {
            worker_actions: worker::BUILD_CITY | worker::AUTOMATE | worker::JOIN_CITY,
            ..UnitType::default()
        };
        assert_eq!(settler.derived_worker_strength(), 0.0);
        let terraformer = UnitType {
            ai_strategies: ai::TERRAFORM,
            ..UnitType::default()
        };
        assert_eq!(terraformer.derived_worker_strength(), 1.0);

        let Some(ptw) = units_of("Ancient_Mediterranean") else {
            return;
        };
        assert_eq!(get(&ptw, "Worker").worker_strength, 1.0);
        assert_eq!(get(&ptw, "Warrior").worker_strength, 0.0);
        // The Conquests editor stores what the rule gives for every stock row.
        if let Some(conq) = conquests_units() {
            for u in &conq {
                assert_eq!(
                    u.worker_strength,
                    u.derived_worker_strength(),
                    "{}",
                    u.name.text()
                );
            }
        }
    }

    /// The constructor and the editor's apply routine clamp the numbers to these ranges, and
    /// the apply routine zeroes a few fields in the situations listed in the module docs.
    #[test]
    fn editor_ranges_hold_in_every_file() {
        let mut rows = 0;
        for (f, units) in all_units() {
            for u in &units {
                let who = format!("{} / {}", f.name(), u.name.text());
                let within = |v: i32, lo: i32, hi: i32| (lo..=hi).contains(&v);
                assert!(within(u.bombard_strength, 0, 1000), "{who}");
                assert!(within(u.bombard_range, 0, 362), "{who}");
                assert!(within(u.transport_capacity, 0, 100), "{who}");
                assert!(within(u.shield_cost, 0, 1000), "{who}");
                assert!(within(u.defense, 0, 1000), "{who}");
                assert!(within(u.attack, 0, 1000), "{who}");
                assert!(within(u.operational_range, 0, 362), "{who}");
                assert!(within(u.population_cost, 0, 255), "{who}");
                assert!(within(u.rate_of_fire, 0, 10), "{who}");
                assert!(within(u.movement, 1, 100), "{who}");
                assert!(within(u.hit_point_bonus, -20, 20), "{who}");
                assert!(within(u.telepad_range, 0, 100), "{who}");
                assert!(within(u.unit_class, 0, 2), "{who}");
                assert!(u.zone_of_control <= 1, "{who}");
                if f.version.major >= 11 {
                    // Rows saved by the Play the World or Conquests editor.
                    if u.bombard_strength == 0 {
                        assert_eq!(u.bombard_fx, 0, "{who}");
                        assert_eq!(u.create_craters, 0, "{who}");
                    }
                    if u.ai_strategies & ai::FLAG_UNIT != 0 {
                        assert_eq!((u.attack, u.defense), (0, 0), "{who}");
                        assert_eq!((u.bombard_strength, u.transport_capacity), (0, 0), "{who}");
                        assert_ne!(u.abilities & ability::FLAG_UNIT, 0, "{who}");
                        assert_eq!(u.standard_orders & order::DISBAND, 0, "{who}");
                    }
                    if u.ai_strategies & ai::KING != 0 {
                        assert_ne!(u.abilities & ability::KING, 0, "{who}");
                        assert_eq!(u.standard_orders & order::DISBAND, 0, "{who}");
                    }
                    // Air units have no Ignore Move Cost (bytes 12 and 13 of a Conquests
                    // row can hold the conversion residue described in the module docs).
                    if u.unit_class == class::AIR && f.version >= IGNORE_MOVE_COST_SINCE {
                        assert!(u.ignore_move_cost[..12].iter().all(|&b| b == 0), "{who}");
                    }
                }
                rows += 1;
            }
        }
        if rows > 0 {
            assert!(rows > 3000, "{rows}");
        }
    }
}
