//! `LEAD` — one row per player slot (editor dialog 191 "Players").
//!
//! Loader worker `0x595B80`, row constructor `0x5E4400`, row reader `0x5E4490`,
//! writer `0x5E47B0`. The editor binds the dialog to the row in `0x43F720`
//! (page → row) and `0x43FA40..0x43FDB0` (row → page); the game applies a row
//! when a scenario game starts in `0x5819D0` (`0x58211F..0x58217D`).
//!
//! # On-disk layout
//!
//! Offsets are body offsets; the game's in-memory row (size `0x6C`) is the
//! file layout shuffled, so the *memory* offset (used by the editor and game
//! code above) is given too.
//!
//! | Body | Mem | Field | Editor control |
//! |---|---|---|---|
//! | `0x00` | `+0x04` | [`custom_civ_data`](Player::custom_civ_data) | NOT *Civilization Defaults* (1777) |
//! | `0x04` | `+0x08` | [`human_player`](Player::human_player) | *Human Player* (1778) |
//! | `0x08` | `+0x0C` | [`leader_name`](Player::leader_name) `[40]` | *Leader Name* (1792, max 31 chars) |
//! | `0x30` | `+0x54` | `starting_units` count, then `count × {count, unit}` | *Starting Units* (1779) |
//! | | `+0x38` | [`gender`](Player::gender) | *Male / Female* (1794/1795) |
//! | | `+0x50` | `free_techs` count, then `count × i32` | *Free Techs* (1780) |
//! | | `+0x40` | [`difficulty`](Player::difficulty) | *Difficulty Level* (1565) |
//! | | `+0x44` | [`initial_era`](Player::initial_era) | *Initial Era* (1561) |
//! | | `+0x48` | [`starting_treasury`](Player::starting_treasury) | *Starting Treasury* (1784) |
//! | | `+0x4C` | [`government`](Player::government) | *Government* (1563) |
//! | | `+0x58` | [`civilization`](Player::civilization) | *Civilization* (1559) |
//! | | `+0x5C` | [`team_color`](Player::team_color) | *Team Color* (1790) |
//! | | `+0x60` | [`skip_first_turn`](Player::skip_first_turn) | *Skip 1st Turn* (1796) |
//! | | `+0x64` | [`unused_0x64`](Player::unused_0x64) | none |
//! | | `+0x68` (byte) | [`start_embassies`](Player::start_embassies) | *Starts with Embassies* (1797) |
//!
//! The last three fields exist in 12.06+ files only; older rows end after
//! `team_color` (the reader zeroes `+0x64` before reading and every field is
//! guarded by the bytes left in the row). The writer always emits everything.
//!
//! Row sizes in the corpus: 84 B (PTW, no units, no techs) up to 357 B
//! (Conquests with a full tech list).
//!
//! # Semantics
//!
//! * **Editor save path (`0x43F720`)**: `custom_civ_data = !civ_defaults`.
//!   Team color, leader name, gender and free techs are written only while
//!   *Civilization Defaults* is **off**; with it on they keep their old value.
//!   Starting units are stored with `count` clamped to `1..=25`, team color
//!   to `0..=32`, treasury to `0..=1_000_000`.
//! * **Game start (`0x58211F`)**: the leader name and gender are copied to the
//!   setup screen only if `custom_civ_data != 0`; `difficulty` only if it is
//!   not [`DIFFICULTY_DEFAULT`]; era, treasury and government feed the new
//!   civilization (`0x567D97`, `0x568925`).
//! * **Loader (`0x595B80`)**: the first row with `human_player != 0` becomes
//!   the human slot; if no row has it, row 0 is forced to human.
//! * `difficulty` of old (PTW) files is a raw index (`6` in 128 rows); the game
//!   applies it without translation.

use crate::Version;
use crate::io::{Ctx, Field, Reader, Record, Result, Str, Writer};

/// The first version whose rows carry *Skip 1st Turn* and *Starts with Embassies*: the first
/// Conquests version in the corpus (the Conquests editor's notes give no numbers).
pub const CONQUESTS_SINCE: Version = Version::new(12, 6);

/// [`Player::difficulty`] sentinel: do not override the setup screen's value
/// (constructor default, editor *Difficulty Level* combo item 0).
pub const DIFFICULTY_DEFAULT: i32 = -2;

/// [`Player::civilization`] value for *Any* (constructor default; editor combo item 0).
pub const CIV_ANY: i32 = -3;

/// [`Player::civilization`] value for *Random* (editor combo item 1).
pub const CIV_RANDOM: i32 = -2;

/// [`Player::government`] when unset (constructor default).
pub const GOVERNMENT_UNSET: i32 = -1;

/// One starting-unit stack (editor *Starting Units* list: *Unit* + *How Many*).
///
/// File order is **count first, unit type second** (editor helper `0x4970A0`
/// stores `clamp(count, 1, 25)` at `+0` and the unit at `+4`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct StartingUnit {
    /// *How Many* (1–25). Editor control 1055/1056.
    pub count: i32,
    /// `PRTO` index. Editor control 1566/1567.
    pub unit_type: i32,
}

impl Field for StartingUnit {
    const SIZE: usize = 8;
    fn zero() -> Self {
        Self::default()
    }
    fn read(r: &mut Reader<'_>) -> Self {
        StartingUnit {
            count: r.i32().unwrap_or(0),
            unit_type: r.i32().unwrap_or(0),
        }
    }
    fn write(&self, w: &mut Writer) {
        w.i32(self.count);
        w.i32(self.unit_type);
    }
}

/// One `LEAD` row — one player slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Player {
    /// Body `0x00`. `1` when the row carries its own leader name / gender /
    /// team color / free techs, i.e. the editor's *Civilization Defaults* box is
    /// **unchecked** (the editor stores `!checkbox`). Constructor default `0`.
    pub custom_civ_data: u32,
    /// Body `0x04`. *Human Player* checkbox (constructor argument).
    pub human_player: u32,
    /// Body `0x08`. *Leader Name*, a 40-byte NUL-terminated buffer (the editor
    /// limits it to 31 characters). Used only with [`custom_civ_data`](Self::custom_civ_data).
    pub leader_name: Str<40>,
    /// *Starting Units*.
    pub starting_units: Vec<StartingUnit>,
    /// *Gender*: `0` male, `1` female. Used only with [`custom_civ_data`](Self::custom_civ_data).
    pub gender: u32,
    /// *Free Techs*: `TECH` indices the player starts with.
    pub free_techs: Vec<i32>,
    /// *Difficulty Level*: `DIFF` index, [`DIFFICULTY_DEFAULT`] for no override.
    pub difficulty: i32,
    /// *Initial Era*: `ERAS` index.
    pub initial_era: i32,
    /// *Starting Treasury* (gold, 0–1,000,000).
    pub starting_treasury: i32,
    /// *Government*: `GOVT` index, [`GOVERNMENT_UNSET`] if none.
    pub government: i32,
    /// *Civilization*: `RACE` index, [`CIV_ANY`] or [`CIV_RANDOM`].
    pub civilization: i32,
    /// *Team Color*: palette index (0–32).
    pub team_color: i32,
    /// *Skip 1st Turn* (12.06+).
    pub skip_first_turn: u32,
    /// Memory `+0x64` (12.06+). No editor control and no reader found in the
    /// game; constructor default `1`, `0` in all 245 shipped rows that have it.
    pub unused_0x64: i32,
    /// *Starts with Embassies* (12.06+).
    pub start_embassies: u8,
    /// Bytes after the last modelled field (empty in every shipped file).
    pub extra: Vec<u8>,
}

impl Default for Player {
    /// The constructor's values (`0x5E4400`), with `human_player` 0.
    fn default() -> Self {
        Player {
            custom_civ_data: 0,
            human_player: 0,
            leader_name: Str::default(),
            starting_units: Vec::new(),
            gender: 0,
            free_techs: Vec::new(),
            difficulty: DIFFICULTY_DEFAULT,
            initial_era: 0,
            starting_treasury: 0,
            government: GOVERNMENT_UNSET,
            civilization: CIV_ANY,
            team_color: 0,
            skip_first_turn: 0,
            unused_0x64: 1,
            start_embassies: 0,
            extra: Vec::new(),
        }
    }
}

impl Player {
    /// True when the editor's *Civilization Defaults* box is checked.
    pub fn uses_civ_defaults(&self) -> bool {
        self.custom_civ_data == 0
    }

    /// True for the *Human Player* slot.
    pub fn is_human(&self) -> bool {
        self.human_player != 0
    }
}

impl Record for Player {
    const TAG: [u8; 4] = *b"LEAD";

    fn read(r: &mut Reader<'_>, ctx: &Ctx) -> Result<Self> {
        let mut p = Player {
            custom_civ_data: r.u32().unwrap_or(0),
            human_player: r.u32().unwrap_or(0),
            ..Player::default()
        };
        if r.remaining() >= 40 {
            p.leader_name = r.str::<40>().unwrap_or_default();
        }
        p.starting_units = r
            .counted_list::<StartingUnit>(Self::TAG, "starting_units")?
            .unwrap_or_default();
        p.gender = r.u32().unwrap_or(0);
        p.free_techs = r
            .counted_list::<i32>(Self::TAG, "free_techs")?
            .unwrap_or_default();
        if r.remaining() >= 24 {
            p.difficulty = r.i32().unwrap_or(0);
            p.initial_era = r.i32().unwrap_or(0);
            p.starting_treasury = r.i32().unwrap_or(0);
            p.government = r.i32().unwrap_or(0);
            p.civilization = r.i32().unwrap_or(0);
            p.team_color = r.i32().unwrap_or(0);
        }
        // The reader zeroes `+0x64` before reading, so an absent field is 0.
        p.unused_0x64 = 0;
        if ctx.version >= CONQUESTS_SINCE && r.remaining() >= 9 {
            p.skip_first_turn = r.u32().unwrap_or(0);
            p.unused_0x64 = r.i32().unwrap_or(0);
            p.start_embassies = r.u8().unwrap_or(0);
        }
        p.extra = r.rest().to_vec();
        Ok(p)
    }

    fn write(&self, w: &mut Writer, ctx: &Ctx) {
        w.u32(self.custom_civ_data);
        w.u32(self.human_player);
        w.bytes(&self.leader_name.0);
        w.counted_list(&self.starting_units);
        w.u32(self.gender);
        w.counted_list(&self.free_techs);
        w.i32(self.difficulty);
        w.i32(self.initial_era);
        w.i32(self.starting_treasury);
        w.i32(self.government);
        w.i32(self.civilization);
        w.i32(self.team_color);
        if ctx.version >= CONQUESTS_SINCE {
            w.u32(self.skip_first_turn);
            w.i32(self.unused_0x64);
            w.u8(self.start_embassies);
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

    fn players_of(needle: &str) -> Vec<Player> {
        let files = crate::corpus::files();
        let Some(f) = files.iter().find(|f| f.name().contains(needle)) else {
            return Vec::new();
        };
        crate::corpus::rows::<Player>(std::slice::from_ref(f))
            .into_iter()
            .map(|row| Player::read(&mut Reader::new(row.body), &f.ctx()).unwrap())
            .collect()
    }

    #[test]
    fn corpus_roundtrip() {
        let st = crate::corpus::check_roundtrip::<Player>();
        assert_eq!(st.with_extra, 0, "{}", st.report());
    }

    /// Every field value is in the range the editor can produce.
    #[test]
    fn corpus_values_are_in_editor_range() {
        let files = crate::corpus::files();
        for row in crate::corpus::rows::<Player>(&files) {
            let p = Player::read(&mut Reader::new(row.body), &row.file.ctx()).unwrap();
            assert!(p.custom_civ_data <= 1 && p.human_player <= 1 && p.gender <= 1);
            assert!(p.leader_name.is_empty(), "no shipped row names its leader");
            assert!(p.skip_first_turn <= 1 && p.start_embassies <= 1);
            assert_eq!(p.unused_0x64, 0);
            assert!((0..=32).contains(&p.team_color));
            assert!((0..=1_000_000).contains(&p.starting_treasury));
            assert!(p.difficulty == DIFFICULTY_DEFAULT || p.difficulty >= 0);
            assert!(p.civilization == CIV_ANY || p.civilization >= 0);
            for u in &p.starting_units {
                assert!((1..=25).contains(&u.count), "{u:?}");
            }
        }
    }

    #[test]
    fn intro1_players_are_all_human_with_a_settler_and_worker() {
        let players = players_of("Intro1_Ancient_Treasures");
        if players.is_empty() {
            return;
        }
        assert_eq!(players.len(), 4);
        for p in &players {
            assert!(p.is_human() && !p.uses_civ_defaults());
            assert_eq!(
                p.starting_units,
                vec![
                    StartingUnit {
                        count: 1,
                        unit_type: 0
                    },
                    StartingUnit {
                        count: 1,
                        unit_type: 1
                    }
                ]
            );
            assert_eq!(p.difficulty, DIFFICULTY_DEFAULT);
            assert_eq!(p.starting_treasury, 10);
            assert_eq!(p.government, 1);
            assert_eq!(p.initial_era, 0);
        }
        let civs: Vec<i32> = players.iter().map(|p| p.civilization).collect();
        assert_eq!(civs, vec![1, 2, 6, 7]);
        assert_eq!(players[0].free_techs, vec![2, 5]);
        assert_eq!(players[0].gender, 1);
    }

    /// The loader forces row 0 human only when no row is; stock scenarios
    /// always mark at least one.
    #[test]
    fn stock_scenarios_have_a_human_slot() {
        let files = crate::corpus::files();
        for f in &files {
            let rows = crate::corpus::rows::<Player>(std::slice::from_ref(f));
            if rows.is_empty() {
                continue;
            }
            let any = rows.iter().any(|row| {
                Player::read(&mut Reader::new(row.body), &f.ctx())
                    .unwrap()
                    .is_human()
            });
            assert!(any, "{}", f.name());
        }
    }

    #[test]
    fn rise_of_rome_embassies_and_treasury() {
        let players = players_of("2_Rise_of_Rome.biq");
        if players.is_empty() {
            return;
        }
        assert_eq!(players[0].starting_treasury, 800);
        assert_eq!(players[0].government, 4);
        assert_eq!(players[0].civilization, 1);
        assert_eq!(players[0].team_color, 1);
        assert_eq!(players[0].start_embassies, 1);
        assert_eq!(players[0].starting_units[0].unit_type, 26);
    }

    #[test]
    fn ptw_island_hop_uses_civ_defaults() {
        let players = players_of("Island_Hop");
        if players.is_empty() {
            return;
        }
        for p in &players {
            assert!(p.uses_civ_defaults());
            assert_eq!(p.difficulty, 6, "raw PTW difficulty index");
            assert_eq!(p.initial_era, 4);
            assert_eq!(p.starting_treasury, 100);
            assert_eq!(p.government, 2);
            assert_eq!(p.civilization, CIV_ANY);
            assert!(p.starting_units.is_empty() && p.free_techs.is_empty());
            // Old rows end after the team color; the optional tail is absent.
            assert_eq!(p.unused_0x64, 0);
        }
        assert_eq!(players.iter().filter(|p| p.is_human()).count(), 1);
    }
}
