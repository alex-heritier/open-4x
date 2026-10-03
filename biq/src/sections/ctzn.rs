//! `CTZN` - citizen types (the default worker plus the specialists).
//!
//! Row reader `0x5E06D0` (arm `0x594864`, in-memory stride `0x80`); writer
//! `0x5E05B0`. Editor page 161 "Citizens" (help: "Citizens Page"). Row body 116
//! bytes up to PTW (4.01, 11.18), 124 bytes in Conquests (12.06+), which
//! appended `corruption` and `construction`.
//!
//! Field order and names are the editor page's: *Default Citizen* checkbox,
//! *Plural name*, *Civilopedia Entry*, *Prerequisite* (a tech), and the
//! *Bonuses* group *Luxuries / Research / Taxes / Corruption / Construction*.
//! Each bonus is "per population point of this citizen type" (help pages
//! "Luxury Bonus", "Research Bonus", "Taxation Bonus", "Corruption Bonus",
//! "Construction Bonus (Citizen Properties)").

use crate::fixed_record;
use crate::io::Str;

fixed_record! {
    /// One citizen type.
    pub struct Citizen(b"CTZN") {
        /// "Default Citizen" checkbox (dialog 161; body `+0`). `1` only on Laborer.
        pub default_citizen: i32,
        /// Singular name (`Laborer`; body `+4`).
        pub name: Str<32>,
        /// Civilopedia key (`CTZN_Laborer`; body `+0x24`). Policeman and Civil
        /// Engineer reuse `CTZN_Laborer` in the shipped Conquests rules.
        pub civilopedia_entry: Str<32>,
        /// Plural name (`Laborers`; body `+0x44`).
        pub plural_name: Str<32>,
        /// "Prerequisite": index into `TECH` that makes this citizen type
        /// available, `-1` for none (body `+0x64`). Conquests: Policeman 43,
        /// Civil Engineer 57.
        pub prerequisite: i32,
        /// "Luxuries" bonus per population (help *Luxury Bonus*; body `+0x68`;
        /// Entertainer `1`).
        pub luxuries: i32,
        /// "Research" bonus (help *Research Bonus*; body `+0x6c`; Scientist `3`
        /// in Conquests).
        pub research: i32,
        /// "Taxes" bonus (help *Taxation Bonus*; body `+0x70`; Tax Collector `2`).
        pub taxes: i32,
    }
    since (12, 6) {
        /// Corruption bonus per citizen (body `+0x74`; Policeman 1). Conquests.
        pub corruption: i32,
        /// Construction bonus per citizen: extra shields while building
        /// improvements and wonders, never units (body `+0x78`; Civil Engineer
        /// 2). Conquests.
        pub construction: i32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Version;
    use crate::io::{Reader, Record};

    #[test]
    fn corpus_roundtrip() {
        if !crate::corpus::available() {
            return;
        }
        let st = crate::corpus::check_roundtrip::<Citizen>();
        assert_eq!(st.with_extra, 0);
        assert_eq!(st.lengths, vec![116, 124]);
        assert_eq!(
            st.by_version.get(&Version::new(4, 1)).map(|v| v.as_slice()),
            Some(&[116][..])
        );
        assert_eq!(
            st.by_version
                .get(&Version::new(12, 8))
                .map(|v| v.as_slice()),
            Some(&[124][..])
        );
    }

    #[test]
    fn conquests_citizens() {
        let files = crate::corpus::files();
        let Some(f) = files
            .iter()
            .find(|f| f.name().ends_with("Conquests/conquests.biq"))
        else {
            return;
        };
        let rows: Vec<Citizen> = crate::corpus::rows::<Citizen>(std::slice::from_ref(f))
            .into_iter()
            .map(|r| Citizen::read(&mut Reader::new(r.body), &f.ctx()).unwrap())
            .collect();
        let by_name = |n: &str| rows.iter().find(|c| c.name.text() == n).unwrap();
        assert_eq!(by_name("Laborer").default_citizen, 1);
        assert_eq!(by_name("Entertainer").luxuries, 1);
        assert_eq!(
            (
                by_name("Tax Collector").taxes,
                by_name("Scientist").research
            ),
            (2, 3)
        );
        assert_eq!(
            (
                by_name("Policeman").prerequisite,
                by_name("Policeman").corruption
            ),
            (43, 1)
        );
        let engineer = by_name("Civil Engineer");
        assert_eq!((engineer.prerequisite, engineer.construction), (57, 2));
    }
}
