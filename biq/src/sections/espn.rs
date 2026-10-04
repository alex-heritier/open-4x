//! `ESPN` - diplomat/spy missions.
//!
//! Loader arm `0x5948FB`, worker `0x5958F0` (in-memory stride `0xEC`, memory
//! offset = body offset + 4; the game keeps the table in global `0x9C40C8`),
//! row reader `0x5E1950`, writer `0x5E1860`. Editor dialog 153 *Diplomats and
//! Spies* (help: *Diplomat/Spy Mission Properties*), apply routine `0x4310B0`.
//! Row body 232 bytes; exactly nine missions (the worker complains when the
//! count is not 9).
//!
//! | body | field | editor control |
//! |------|-------|----------------|
//! | `0x00` | `description` (128 bytes) | edit 1267 *Description*, copy of 127 characters |
//! | `0x80` | `name` (64 bytes) | the mission combo 1260 (renamed with *Rename*), copy of 63 |
//! | `0xC0` | `civilopedia_entry` (32 bytes) | edit 1618, copy of 31 |
//! | `0xE0` | `performed_by` | check boxes 1264 *Diplomats* (bit 0) and 1265 *Spies* (bit 1) |
//! | `0xE4` | `base_cost` | edit 1738 *Base Cost*, clamped to `0..=1000` |
//!
//! `performed_by` is **who may perform the mission**, a bit mask the game ANDs
//! with a class field of the acting unit (`0x4451F5` and the four tests after
//! it, `0x44554C` for the embassy). Earlier readings of this dword as a
//! "target selection" kind are wrong: the stock values are `1` (diplomats only)
//! for *Build an Embassy*, `3` (both) for *Investigate City* and *Steal
//! Technology*, and `2` (spies only) for the other six.
//!
//! # What the base cost means
//!
//! From the editor's help page for *Base Cost*, the formula per mission before
//! the distance, city-population, citizen-ratio and safety-level modifiers:
//!
//! | mission | cost |
//! |---------|------|
//! | Establish Embassy | `base + target city size` |
//! | Investigate City | `base × target city size` |
//! | Steal Technology | `base × target player's techs × tech rate / 100` |
//! | Steal World Map | `base × (map width + map height) / 2` |
//! | Plant Spy | `base` |
//! | Steal Plans | `base × target player's units` |
//! | Initiate Propaganda | `base × size + base × size × culture ratio + base × size / 2 × gold ratio` |
//! | Sabotage Production | `base × target city's shields` |
//! | Expose Mole | `base` |
//!
//! When `major + minor × 0.01 < 2.10` the worker patches default costs onto the
//! nine rows (`0x5959C8`: 20, 10, 10, 1, 60, 10, 100, 10, 80).

use crate::fixed_record;
use crate::io::Str;

/// `performed_by` bits.
pub mod performed_by {
    /// Diplomats may perform the mission.
    pub const DIPLOMATS: u32 = 1;
    /// Spies may perform the mission.
    pub const SPIES: u32 = 2;
    /// Both may.
    pub const BOTH: u32 = DIPLOMATS | SPIES;
}

fixed_record! {
    /// One espionage mission offered to diplomats and spies.
    pub struct EspionageMission(b"ESPN") {
        /// The text shown as the title of the window that opens when a player
        /// starts the mission (editor edit 1267 *Description*; body `+0`,
        /// 128 bytes). For *Build an Embassy*: "Choose the civilization for
        /// your embassy".
        pub description: Str<128>,
        /// Mission name, as listed in the editor's mission combo ("Build an
        /// Embassy"; body `+0x80`).
        pub name: Str<64>,
        /// Civilopedia / text-script key (body `+0xc0`; e.g. `ESPN_Build_an_Embassy`).
        pub civilopedia_entry: Str<32>,
        /// Who may perform the mission, see [`performed_by`] (body `+0xe0`).
        pub performed_by: u32,
        /// "Base Cost" of the mission, `0..=1000` in the editor (body `+0xe4`);
        /// the formula is in the module docs.
        pub base_cost: i32,
    }
}

impl EspionageMission {
    /// Whether diplomats may perform the mission.
    pub fn diplomats_may(&self) -> bool {
        self.performed_by & performed_by::DIPLOMATS != 0
    }

    /// Whether spies may perform the mission.
    pub fn spies_may(&self) -> bool {
        self.performed_by & performed_by::SPIES != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus;
    use crate::io::{Reader, Record};

    fn conquests() -> Vec<EspionageMission> {
        let files = corpus::files();
        let Some(f) = files
            .iter()
            .find(|f| f.name().ends_with("Conquests/conquests.biq"))
        else {
            return Vec::new();
        };
        corpus::rows::<EspionageMission>(std::slice::from_ref(f))
            .into_iter()
            .map(|r| EspionageMission::read(&mut Reader::new(r.body), &f.ctx()).unwrap())
            .collect()
    }

    #[test]
    fn corpus_roundtrip() {
        if !crate::corpus::available() {
            return;
        }
        let st = corpus::check_roundtrip::<EspionageMission>();
        assert_eq!(st.with_extra, 0);
        assert_eq!(st.lengths, vec![232]);
    }

    /// The nine stock missions: name, who performs it, base cost.
    #[test]
    fn conquests_missions() {
        let rows = conquests();
        if rows.is_empty() {
            return;
        }
        let got: Vec<_> = rows
            .iter()
            .map(|m| (m.name.text().into_owned(), m.performed_by, m.base_cost))
            .collect();
        let want = |name: &str, who, cost| (name.to_string(), who, cost);
        assert_eq!(
            got,
            [
                want("Build an Embassy", performed_by::DIPLOMATS, 20),
                want("Investigate City", performed_by::BOTH, 10),
                want("Steal Technology", performed_by::BOTH, 10),
                want("Steal World Map", performed_by::SPIES, 1),
                want("Plant Spy", performed_by::SPIES, 60),
                want("Steal Plans", performed_by::SPIES, 10),
                want("Initiate Propaganda", performed_by::SPIES, 100),
                want("Sabotage Production", performed_by::SPIES, 10),
                want("Expose Enemy Spy", performed_by::SPIES, 80),
            ]
        );
        let embassy = &rows[0];
        assert_eq!(
            embassy.description.text(),
            "Choose the civilization for your embassy"
        );
        assert_eq!(embassy.civilopedia_entry.text(), "ESPN_Build_an_Embassy");
        assert!(embassy.diplomats_may() && !embassy.spies_may());
        assert!(rows[1].diplomats_may() && rows[1].spies_may());
        assert!(!rows[8].diplomats_may() && rows[8].spies_may());
    }

    /// The costs the worker patches into pre-2.10 files are the stock costs.
    #[test]
    fn stock_costs_are_the_pre_2_10_defaults() {
        let rows = conquests();
        if rows.is_empty() {
            return;
        }
        let costs: Vec<_> = rows.iter().map(|m| m.base_cost).collect();
        assert_eq!(costs, [20, 10, 10, 1, 60, 10, 100, 10, 80]);
    }

    /// Whatever the editor wrote, the mask is a nonempty subset of the two
    /// check boxes and the cost is inside the edit box's range.
    #[test]
    fn editor_ranges_hold_in_every_file() {
        let mut rows = 0;
        for f in corpus::files() {
            for r in corpus::rows::<EspionageMission>(std::slice::from_ref(&f)) {
                let m = EspionageMission::read(&mut Reader::new(r.body), &f.ctx()).unwrap();
                let who = format!("{} / {}", f.name(), m.name.text());
                assert!((1..=3).contains(&m.performed_by), "{who}");
                assert!((0..=1000).contains(&m.base_cost), "{who}");
                rows += 1;
            }
        }
        if !corpus::files().is_empty() {
            assert!(rows >= 9 * 20, "{rows} rows");
        }
    }
}
