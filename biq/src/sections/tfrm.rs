//! `TFRM` - worker jobs (Road, Mine, Irrigation, Clear Forest, ...): what
//! workers can be told to do and what it takes.
//!
//! Arm `0x594824`, worker `0x596990`, row reader `0x5E88F0`, writer `0x5E8800`.
//! Editor dialog 132 (*Worker Jobs*); its apply routine `0x44FBA2` stores the
//! controls into the row (the editor's rows are 116 bytes: the length word,
//! then the 112-byte body, so editor offset = body offset + 4). The row is 112
//! bytes in every file.
//!
//! | body | field | editor control | editor range |
//! |------|-------|----------------|--------------|
//! | `0x00` | `name` (32 bytes) | combo 1011 (the text) | 31 chars |
//! | `0x20` | `civilopedia_entry` (32) | edit 1632 | 31 chars |
//! | `0x40` | `turns_to_complete` | edit 1013 | 1..=1000 |
//! | `0x44` | `required_tech` | combo 1012, selection - 1 | a `TECH` row, `-1` none |
//! | `0x48` | `required_resource_1` | combo 1015 | a `GOOD` row, `-1` none |
//! | `0x4C` | `required_resource_2` | combo 1016 | a `GOOD` row, `-1` none |
//! | `0x50` | `order` (32) | edit 1636 | 31 chars |
//!
//! The resource combos keep the resource index as item data (`0x4512A0` reads
//! it back with `CB_GETITEMDATA`, and answers `-1` for the *None* entry).

use crate::fixed_record;
use crate::io::Str;

fixed_record! {
    /// One worker job.
    pub struct WorkerJob(b"TFRM") {
        /// Job name ("Mine", "Road", "Clear Forest", ...), the text of the
        /// job selector.
        pub name: Str<32>,
        /// Civilopedia key (`TFRM_MINE`).
        pub civilopedia_entry: Str<32>,
        /// *Turns to complete*: the base number of turns one worker needs;
        /// the government modifies it. Never below 1.
        pub turns_to_complete: i32,
        /// *Prerequisite*: the `TECH` row a civilization must know, `-1` none.
        pub required_tech: i32,
        /// First *Required Resource*: a `GOOD` row the civilization must have
        /// access to, `-1` none.
        pub required_resource_1: i32,
        /// Second *Required Resource*, `-1` none.
        pub required_resource_2: i32,
        /// *Order*: the words describing the job in the worker's orders ("Build
        /// Mine").
        pub order: Str<32>,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::{self, files};
    use crate::io::{Reader, Record};

    fn jobs_of(suffix: &str) -> Vec<WorkerJob> {
        let Some(f) = files().into_iter().find(|f| f.name().ends_with(suffix)) else {
            return Vec::new();
        };
        f.raw
            .section(b"TFRM")
            .unwrap()
            .rows
            .iter()
            .map(|r| WorkerJob::read(&mut Reader::new(f.raw.row(r)), &f.ctx()).unwrap())
            .collect()
    }

    #[test]
    fn corpus_roundtrip() {
        if !crate::corpus::available() {
            return;
        }
        let st = corpus::check_roundtrip::<WorkerJob>();
        assert_eq!(st.with_extra, 0);
        assert_eq!(st.lengths, vec![112]);
    }

    /// The editor clamps the turns to 1..=1000 and the other three references
    /// to indices of the file's own tables.
    #[test]
    fn editor_ranges_hold_in_every_file() {
        use crate::sections::{good::Good, tech::Tech};
        let mut rows = 0;
        for f in files() {
            let Some(sec) = f.raw.section(b"TFRM") else {
                continue;
            };
            let count = |tag: &[u8; 4]| f.raw.section(tag).map_or(0, |s| s.rows.len() as i32);
            let (techs, goods) = (count(&Tech::TAG), count(&Good::TAG));
            for r in &sec.rows {
                let j = WorkerJob::read(&mut Reader::new(f.raw.row(r)), &f.ctx()).unwrap();
                let who = format!("{} / {}", f.name(), j.name.text());
                assert!((1..=1000).contains(&j.turns_to_complete), "{who}");
                // Files without the tables (jobs only) cannot be checked.
                if techs > 0 {
                    assert!((-1..techs).contains(&j.required_tech), "{who}");
                }
                if goods > 0 {
                    assert!((-1..goods).contains(&j.required_resource_1), "{who}");
                    assert!((-1..goods).contains(&j.required_resource_2), "{who}");
                }
                rows += 1;
            }
        }
        if !files().is_empty() {
            assert!(rows > 300, "{rows}");
        }
    }

    #[test]
    fn conquests_mine_job() {
        let jobs = jobs_of("Conquests/conquests.biq");
        let Some(job) = jobs.iter().find(|j| j.name.text() == "Mine") else {
            return;
        };
        assert_eq!(job.order.text(), "Build Mine");
        assert_eq!(job.turns_to_complete, 12);
        assert_eq!(job.required_tech, -1);
    }

    #[test]
    fn conquests_railroad_needs_steam_power_and_iron() {
        let jobs = jobs_of("Conquests/conquests.biq");
        let Some(job) = jobs.iter().find(|j| j.name.text() == "Railroad") else {
            return;
        };
        assert_eq!(job.turns_to_complete, 12);
        assert_eq!(job.required_tech, 44); // Steam Power
        assert_eq!(job.required_resource_1, 1); // Iron
        assert_eq!(job.required_resource_2, 3); // Coal
    }
}
