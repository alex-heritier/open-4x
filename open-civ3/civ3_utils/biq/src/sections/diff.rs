//! `DIFF` — difficulty levels: the AI's bonuses and the human's handicaps.
//!
//! Section loader arm `0x5947FB`, worker `0x5952C0`, row reader `0x5E1460`,
//! row writer `0x5E1290`, table allocator and row constructor `0x59BA50`
//! (stride `0x7C`; the row's first memory dword is its length, so memory
//! offset = body offset + 4). Editor dialog 163 (*Difficulty Levels Page*).
//!
//! **The file order is the memory order, not the dialog's grouping**: the
//! reader fills memory front to back and the editor's apply routine (`0x42CB62`)
//! stores each control to the same offsets.
//!
//! | body | field | editor control | editor range | game consumer |
//! |------|-------|----------------|--------------|---------------|
//! | `0x00` | `name` (64 bytes) | combo 1483 | 63 chars | |
//! | `0x40` | `citizens_born_content` | 1487 | 0..=255 | `0x4BDB6B` |
//! | `0x44` | `max_government_transition_time` | 1514 | 0..=100 | `0x53A93F` |
//! | `0x48` | `defensive_land_units` | 1496 | 0..=100 | `0x4AE927` |
//! | `0x4C` | `offensive_land_units` | 1499 | 0..=100 | `0x4AE95F` |
//! | `0x50` | `start_unit_type_1` | 1502 | 0..=100 | `0x4AE9AA` |
//! | `0x54` | `start_unit_type_2` | 1505 | 0..=100 | `0x4AE9F3` |
//! | `0x58` | `additional_free_support` | 1511 | 0..=100 | `0x55D201` |
//! | `0x5C` | `bonus_for_each_city` | 1508 | 0..=100 | `0x55D104` |
//! | `0x60` | `attack_bonus_vs_barbarians` | 1490 | 0..=1000 | `0x4A1000` |
//! | `0x64` | `cost_factor` | 1493 | 1..=100 | `0x5660FF` |
//! | `0x68` | `optimal_cities_percent` | 1517 | 0..=1000 | `0x5677EF` |
//! | `0x6C` | `ai_to_ai_trade_rate` | 1522 | 100..=1000 | getter `0x443A43` |
//! | `0x70` | `corruption_percent` | trackbar 1796 | 0..=200 | `0x4B19BC` |
//! | `0x74` | `citizens_quelled_by_military` | 1525 | 0..=255 | `0x4B2B8F` |
//!
//! Every binding above was read from the editor's DDX table and apply routine
//! (control → slot → row store), and every game consumer from the executable;
//! the per-field help pages say the same.
//!
//! # Version history
//!
//! `Scenario::loadDIFF` logs three steps of "data added" for old files
//! (`0x5952C0`): *v2.06* the AI bonus group (transition time, extra starting
//! units, unit support), *v2.07* the optimal-cities percentage, *v3.06* the
//! AI-to-AI trade rate (the corruption modifier arrived with it, Civ3Edit 1.32;
//! its default needs no fix-up because the row constructor already sets it).
//! The editor's release notes date the last field: *Civ3XEdit 2.15 = file
//! version 11.10* added the citizens quelled by military ("defaults to 1 for all
//! levels"). So the shipped Civ 3 1.x scenario (`4.01`) has 116-byte rows and
//! every `BICX` file with this section has 120. Nothing older than 4.01 is in
//! the corpus, and the reader keys on the row length alone.

use crate::Version;
use crate::io::{Ctx, Field, Reader, Record, Result, Str, Writer};

/// The file version that added `ai_to_ai_trade_rate` and `corruption_percent`
/// (the exe's `loadDIFF` step, and Civ3Edit 1.32).
const TRADE_RATE_SINCE: Version = Version::new(3, 6);
/// The file version that added `citizens_quelled_by_military` (Civ3XEdit 2.15).
const QUELLED_SINCE: Version = Version::new(11, 10);

/// One difficulty level (`Chieftain` … `Deity`, plus custom names).
///
/// "AI" fields are bonuses the computer players get; the others shape the
/// human's game. Conquests ships eight levels (the stock names are
/// `Chieftain, Warlord, Regent, Monarch, Emperor, Demigod, Deity, Sid`),
/// Civ 3 1.x and PTW six.
#[derive(Clone, Debug, PartialEq)]
pub struct Difficulty {
    /// Level name (editor combo 1483).
    pub name: Str<64>,
    /// How many citizens of a city are born content (the fewer, the harder).
    /// Chieftain `4`, Regent `2`, Deity `1`. At `0x4BDB6B` the city owner's
    /// level decides: the first this-many citizens get the content mood, the
    /// rest start unhappy (before luxuries, buildings and martial law).
    pub citizens_born_content: i32,
    /// AI only: the longest the AI stays in the transition government
    /// (anarchy) when it changes government, in turns; `0` = no limit. Stock
    /// Monarch `4`, Emperor `3`, Demigod/Deity `2`, Sid `1`. `0x53A93F` caps the
    /// computed duration with it for every player *not* in the human mask
    /// (`0xA526BC`).
    pub max_government_transition_time: i32,
    /// AI only: extra starting units of the AI's best defender (chosen among the
    /// units it can build). Stock Monarch `2`, Deity `8`, Sid `12`.
    pub defensive_land_units: i32,
    /// AI only: extra starting units of its best attacker. Monarch `1`, Deity `4`.
    pub offensive_land_units: i32,
    /// AI only: extra copies of the first default start unit (`RULE`'s start
    /// unit 1, normally the Worker). Demigod `1`, Sid `2`.
    pub start_unit_type_1: i32,
    /// AI only: extra copies of the second default start unit (`RULE`'s start
    /// unit 2, normally the Settler). Emperor `1`, Sid `4`.
    pub start_unit_type_2: i32,
    /// AI only: units supported for free on top of the government's. Stock
    /// Monarch `4`, Deity `16`. Free support is
    /// `additional_free_support + cities × bonus_for_each_city`
    /// (`0x55D1E9`..`0x55D204`).
    pub additional_free_support: i32,
    /// AI only: further free support per city. Monarch `1`, Deity `4`, Sid `8`.
    pub bonus_for_each_city: i32,
    /// Percent attack bonus that *any* player has against barbarians (`0` = the
    /// barbarians fight on equal terms). Stock Chieftain `800`, Warlord `400`,
    /// Regent `200`, Monarch `100`, Emperor `50`, Demigod `25`, Deity `0`.
    /// Added to the combat strength modifiers at `0x4A1000` and `0x4A1098`.
    pub attack_bonus_vs_barbarians: i32,
    /// AI only: the factor applied to the AI's growth, shields and research.
    /// The human's is always `10`: `0x5660E0` returns `10` for a player in the
    /// human mask (halved with scenario flag `0x200`, never below `1`). Stock
    /// Chieftain `20` (the AI is slowed), Regent `10`, Deity `6`, Sid `4`.
    pub cost_factor: i32,
    /// What percentage of a world size's `WSIZ` optimal city count counts as
    /// optimal at this level (`100` = the `WSIZ` figure; the government's
    /// corruption modifies it too). `0x5677EF` multiplies by it and divides by
    /// 100. Stock Chieftain `100`, Deity `60`, Sid `50`.
    pub optimal_cities_percent: i32,
    /// AI only: the percentage by which one AI values what another AI offers in
    /// a trade (`120` = a 100 gold offer is worth 120). Stock `110` (Chieftain)
    /// to `200` (Sid). Read through the getter at `0x443A43`.
    pub ai_to_ai_trade_rate: i32,
    /// The percentage of normal corruption (and waste) at this level; `100` is
    /// stock for every level. Applied at `0x4B19BC` (× value ÷ 100); catastrophic
    /// corruption, cities in disorder and capitals ignore it.
    pub corruption_percent: i32,
    /// How many rioting citizens one military unit quells (martial law
    /// multiplier). `1` in every stock level. `0x4B2B8F`, `0x4B2BF7`, `0x4B2E95`
    /// multiply the unit count with it. Absent (and `1`) before file version 11.10.
    pub citizens_quelled_by_military: i32,
    /// Bytes after the last known field (empty in every shipped file).
    pub extra: Vec<u8>,
}

impl Default for Difficulty {
    /// A fresh row as the table allocator (`0x59BA50`) makes it: everything
    /// zero except cost factor `1`, optimal cities `100 %`, AI trade rate
    /// `100 %`, corruption `100 %` and quelled citizens `1`. Those are also the
    /// values a row from an older file keeps for the fields it lacks.
    fn default() -> Self {
        Difficulty {
            name: Str::default(),
            citizens_born_content: 0,
            max_government_transition_time: 0,
            defensive_land_units: 0,
            offensive_land_units: 0,
            start_unit_type_1: 0,
            start_unit_type_2: 0,
            additional_free_support: 0,
            bonus_for_each_city: 0,
            attack_bonus_vs_barbarians: 0,
            cost_factor: 1,
            optimal_cities_percent: 100,
            ai_to_ai_trade_rate: 100,
            corruption_percent: 100,
            citizens_quelled_by_military: 1,
            extra: Vec::new(),
        }
    }
}

impl Record for Difficulty {
    const TAG: [u8; 4] = *b"DIFF";

    fn read(r: &mut Reader<'_>, ctx: &Ctx) -> Result<Self> {
        let mut d = Difficulty::default();
        'fields: {
            if r.remaining() < <Str<64> as Field>::SIZE {
                break 'fields;
            }
            d.name = Str::read(r);
            macro_rules! dword {
                ($($f:ident),+ $(,)?) => {$(
                    if r.remaining() < 4 {
                        break 'fields;
                    }
                    d.$f = i32::read(r);
                )+};
            }
            dword!(
                citizens_born_content,
                max_government_transition_time,
                defensive_land_units,
                offensive_land_units,
                start_unit_type_1,
                start_unit_type_2,
                additional_free_support,
                bonus_for_each_city,
                attack_bonus_vs_barbarians,
                cost_factor,
                optimal_cities_percent,
            );
            // Newer groups are taken only from files of a version that has them; bytes
            // beyond that stay in `extra` and are written back unchanged.
            if ctx.version < TRADE_RATE_SINCE {
                break 'fields;
            }
            dword!(ai_to_ai_trade_rate, corruption_percent);
            if ctx.version < QUELLED_SINCE {
                break 'fields;
            }
            dword!(citizens_quelled_by_military);
        }
        d.extra = r.rest().to_vec();
        Ok(d)
    }

    fn write(&self, w: &mut Writer, ctx: &Ctx) {
        self.name.write(w);
        self.citizens_born_content.write(w);
        self.max_government_transition_time.write(w);
        self.defensive_land_units.write(w);
        self.offensive_land_units.write(w);
        self.start_unit_type_1.write(w);
        self.start_unit_type_2.write(w);
        self.additional_free_support.write(w);
        self.bonus_for_each_city.write(w);
        self.attack_bonus_vs_barbarians.write(w);
        self.cost_factor.write(w);
        self.optimal_cities_percent.write(w);
        if ctx.version >= TRADE_RATE_SINCE {
            self.ai_to_ai_trade_rate.write(w);
            self.corruption_percent.write(w);
        }
        if ctx.version >= QUELLED_SINCE {
            self.citizens_quelled_by_military.write(w);
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
    use crate::Version;
    use crate::corpus::{self, files};
    use crate::io::{Reader, Record};

    fn conquests_rows() -> Vec<Difficulty> {
        let files = files();
        let Some(f) = files
            .iter()
            .find(|f| f.name().ends_with("Conquests/conquests.biq"))
        else {
            return Vec::new();
        };
        let sec = f.raw.section(b"DIFF").expect("DIFF");
        sec.rows
            .iter()
            .map(|row| Difficulty::read(&mut Reader::new(f.raw.row(row)), &f.ctx()).unwrap())
            .collect()
    }

    #[test]
    fn corpus_roundtrip() {
        if !crate::corpus::available() {
            return;
        }
        let st = corpus::check_roundtrip::<Difficulty>();
        assert_eq!(st.with_extra, 0, "{}", st.report());
        assert_eq!(st.lengths, vec![116, 120]);
        // Civ 3 1.x rows end after the corruption modifier ...
        assert_eq!(
            st.by_version.get(&Version::new(4, 1)).map(|v| v.as_slice()),
            Some(&[116][..])
        );
        // ... and everything from Play the World on has the quelled dword.
        assert_eq!(
            st.by_version
                .get(&Version::new(11, 18))
                .map(|v| v.as_slice()),
            Some(&[120][..])
        );
    }

    /// The editor stores every control clamped to the ranges documented in the
    /// module table; scenarios made with it must respect them. A field put at
    /// the wrong offset breaks this at once.
    #[test]
    fn editor_ranges_hold_in_every_file() {
        let mut rows = 0;
        for f in files() {
            let Some(sec) = f.raw.section(b"DIFF") else {
                continue;
            };
            for row in &sec.rows {
                let d = Difficulty::read(&mut Reader::new(f.raw.row(row)), &f.ctx()).unwrap();
                let name = format!("{} / {}", f.name(), d.name.text());
                let in_range = |v: i32, lo: i32, hi: i32| (lo..=hi).contains(&v);
                assert!(in_range(d.citizens_born_content, 0, 255), "{name}");
                assert!(in_range(d.max_government_transition_time, 0, 100), "{name}");
                assert!(in_range(d.defensive_land_units, 0, 100), "{name}");
                assert!(in_range(d.offensive_land_units, 0, 100), "{name}");
                assert!(in_range(d.start_unit_type_1, 0, 100), "{name}");
                assert!(in_range(d.start_unit_type_2, 0, 100), "{name}");
                assert!(in_range(d.additional_free_support, 0, 100), "{name}");
                assert!(in_range(d.bonus_for_each_city, 0, 100), "{name}");
                assert!(in_range(d.attack_bonus_vs_barbarians, 0, 1000), "{name}");
                assert!(in_range(d.cost_factor, 1, 100), "{name}");
                assert!(in_range(d.optimal_cities_percent, 0, 1000), "{name}");
                assert!(in_range(d.ai_to_ai_trade_rate, 100, 1000), "{name}");
                assert!(in_range(d.corruption_percent, 0, 200), "{name}");
                assert!(in_range(d.citizens_quelled_by_military, 0, 255), "{name}");
                rows += 1;
            }
        }
        if !files().is_empty() {
            assert!(rows > 100, "{rows} rows");
        }
    }

    /// Conquests' eight stock levels, column by column.
    #[test]
    fn conquests_stock_levels() {
        let rows = conquests_rows();
        if rows.is_empty() {
            return;
        }
        let names: Vec<_> = rows.iter().map(|d| d.name.text()).collect();
        assert_eq!(
            names,
            [
                "Chieftain",
                "Warlord",
                "Regent",
                "Monarch",
                "Emperor",
                "Demigod",
                "Deity",
                "Sid"
            ]
        );
        let col = |f: fn(&Difficulty) -> i32| rows.iter().map(f).collect::<Vec<_>>();
        assert_eq!(col(|d| d.citizens_born_content), [4, 3, 2, 2, 1, 1, 1, 1]);
        assert_eq!(
            col(|d| d.max_government_transition_time),
            [0, 0, 0, 4, 3, 2, 2, 1]
        );
        assert_eq!(col(|d| d.defensive_land_units), [0, 0, 0, 2, 4, 6, 8, 12]);
        assert_eq!(col(|d| d.offensive_land_units), [0, 0, 0, 1, 2, 3, 4, 6]);
        assert_eq!(col(|d| d.start_unit_type_1), [0, 0, 0, 0, 0, 1, 1, 2]);
        assert_eq!(col(|d| d.start_unit_type_2), [0, 0, 0, 0, 1, 2, 2, 4]);
        assert_eq!(
            col(|d| d.additional_free_support),
            [0, 0, 0, 4, 8, 12, 16, 24]
        );
        assert_eq!(col(|d| d.bonus_for_each_city), [0, 0, 0, 1, 2, 3, 4, 8]);
        assert_eq!(
            col(|d| d.attack_bonus_vs_barbarians),
            [800, 400, 200, 100, 50, 25, 0, 0]
        );
        assert_eq!(col(|d| d.cost_factor), [20, 12, 10, 9, 8, 7, 6, 4]);
        assert_eq!(
            col(|d| d.optimal_cities_percent),
            [100, 95, 90, 85, 80, 70, 60, 50]
        );
        assert_eq!(
            col(|d| d.ai_to_ai_trade_rate),
            [110, 120, 130, 140, 150, 160, 170, 200]
        );
        assert_eq!(col(|d| d.corruption_percent), [100; 8]);
        assert_eq!(col(|d| d.citizens_quelled_by_military), [1; 8]);
    }

    /// The human's cost factor is fixed at 10 and Regent, the default level,
    /// is the one that matches it (editor help, `0x5660E0`).
    #[test]
    fn regent_is_the_neutral_level() {
        let rows = conquests_rows();
        if rows.is_empty() {
            return;
        }
        let regent = &rows[2];
        assert_eq!(regent.cost_factor, 10);
        assert_eq!(regent.name.text(), "Regent");
    }

    /// A Civ 3 1.x row is 116 bytes: the quelled dword is absent and stays at
    /// the constructor's `1`.
    #[test]
    fn old_rows_keep_constructor_defaults() {
        let mut checked = 0;
        for f in files() {
            if f.ctx().version >= QUELLED_SINCE {
                continue;
            }
            let Some(sec) = f.raw.section(b"DIFF") else {
                continue;
            };
            for row in &sec.rows {
                assert_eq!(row.len(), 116, "{}", f.name());
                let d = Difficulty::read(&mut Reader::new(f.raw.row(row)), &f.ctx()).unwrap();
                assert_eq!(d.citizens_quelled_by_military, 1);
                assert_eq!(d.corruption_percent, 100);
                checked += 1;
            }
        }
        if !files().is_empty() {
            assert!(checked >= 6);
        }
    }

    /// Versions on either side of the quelled field's introduction.
    #[test]
    fn writer_follows_the_version_history() {
        let d = Difficulty::default();
        let len = |maj, min| {
            let mut w = Writer::new();
            d.write(
                &mut w,
                &Ctx {
                    version: Version::new(maj, min),
                },
            );
            w.buf.len()
        };
        assert_eq!(len(3, 5), 64 + 4 * 11);
        assert_eq!(len(4, 1), 116);
        assert_eq!(len(11, 9), 116);
        assert_eq!(len(11, 10), 120);
        assert_eq!(len(12, 8), 120);
    }

    #[test]
    fn constructor_defaults_match_the_game() {
        let d = Difficulty::default();
        assert_eq!(
            (
                d.cost_factor,
                d.optimal_cities_percent,
                d.ai_to_ai_trade_rate,
                d.corruption_percent,
                d.citizens_quelled_by_military
            ),
            (1, 100, 100, 100, 1)
        );
    }
}
