//! `WSIZ` - world sizes (Tiny ... Huge): map dimensions and how many
//! civilizations they hold.
//!
//! Arm `0x594DD7`, worker `0x596FD0`, row reader `0x5F7390`, writer `0x5F7270`.
//! Editor dialog 139 (*World Sizes*); its apply routine `0x4667B2` stores each
//! control, clamped, into the row (the editor's rows are 84 bytes: the length
//! word, then the body, so editor offset = body offset + 4).
//!
//! | body | field | editor control | editor range |
//! |------|-------|----------------|--------------|
//! | `0x00` | `optimal_city_count` | edit 1085 | 1..=1000 |
//! | `0x04` | `tech_rate` | edit 1019 | 0..=1000 |
//! | `0x08` | `reserved_0x08` (24 bytes) | none | |
//! | `0x20` | `name` (32 bytes) | combo 1123 | 31 chars |
//! | `0x40` | `height` | edit 1016 | 16..=362, even |
//! | `0x44` | `distance_between_civs` | edit 1017 | 1..=362 |
//! | `0x48` | `civ_count` | edit 1018 | 1..=31 |
//! | `0x4C` | `width` | edit 1015 | 16..=362, even |
//!
//! The row is 80 bytes in every file. The two small counts sit in the opposite
//! order to the dialog (distance first, then civs); the stock Conquests table
//! settles it: its `civ_count` column is `4, 6, 8, 12, 16`, the civ counts the
//! manual gives for Tiny to Huge, and its distances `11, 11, 12, 18, 24` grow
//! with the map.

use crate::fixed_record;
use crate::io::Str;

fixed_record! {
    /// One world size.
    pub struct WorldSize(b"WSIZ") {
        /// *Optimal Number of Cities (corruption)*: the city count above which
        /// corruption rises steeply, whatever the distance to the capital. The
        /// standard 100 x 100 world uses 16 (Play the World) or 20 (Conquests).
        pub optimal_city_count: i32,
        /// *Tech Rate*: the global research rate on maps of this size; larger
        /// maps have more cities and a higher rate. 240 for the standard world.
        pub tech_rate: i32,
        /// 24 bytes the reader copies (`0x5F73F0`) and nothing uses; zero in
        /// every shipped file.
        pub reserved_0x08: [u8; 24],
        /// Display name ("Tiny", "Small", "Standard", "Large", "Huge").
        pub name: Str<32>,
        /// Map height in tiles (even).
        pub height: i32,
        /// *Distance Between Civs*: the minimum distance in tiles the start
        /// location chooser tries to keep between civilizations.
        pub distance_between_civs: i32,
        /// *Number of Civs*: how many civilizations the size supports (at most
        /// 31).
        pub civ_count: i32,
        /// Map width in tiles (even).
        pub width: i32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::{self, files};
    use crate::io::{Reader, Record};

    fn tables() -> Vec<(String, Vec<WorldSize>)> {
        files()
            .iter()
            .filter_map(|f| {
                let sec = f.raw.section(b"WSIZ")?;
                let rows = sec
                    .rows
                    .iter()
                    .map(|r| WorldSize::read(&mut Reader::new(f.raw.row(r)), &f.ctx()).unwrap())
                    .collect();
                Some((f.name(), rows))
            })
            .collect()
    }

    #[test]
    fn corpus_roundtrip() {
        if !crate::corpus::available() {
            return;
        }
        let st = corpus::check_roundtrip::<WorldSize>();
        assert_eq!(st.with_extra, 0);
        assert_eq!(st.lengths, vec![80]);
    }

    /// The editor stores each control clamped to the ranges in the module
    /// table and forces both dimensions even.
    #[test]
    fn editor_ranges_hold_in_every_file() {
        let mut rows = 0;
        for (file, sizes) in tables() {
            for s in &sizes {
                let who = format!("{file} / {}", s.name.text());
                let within = |v: i32, lo: i32, hi: i32| (lo..=hi).contains(&v);
                assert!(within(s.optimal_city_count, 1, 1000), "{who}");
                assert!(within(s.tech_rate, 0, 1000), "{who}");
                assert!(within(s.height, 16, 362) && s.height % 2 == 0, "{who}");
                assert!(within(s.width, 16, 362) && s.width % 2 == 0, "{who}");
                assert!(within(s.distance_between_civs, 1, 362), "{who}");
                assert!(within(s.civ_count, 1, 31), "{who}");
                assert_eq!(s.reserved_0x08, [0u8; 24], "{who}");
                rows += 1;
            }
        }
        if !files().is_empty() {
            assert!(rows >= 30, "{rows}");
        }
    }

    /// Conquests' own table; the civ counts are the manual's.
    #[test]
    fn conquests_stock_sizes() {
        let tables = tables();
        let Some((_, sizes)) = tables.iter().find(|(f, _)| f.ends_with("conquests.biq")) else {
            return;
        };
        let col = |f: fn(&WorldSize) -> i32| sizes.iter().map(f).collect::<Vec<_>>();
        let names: Vec<_> = sizes.iter().map(|s| s.name.text()).collect();
        assert_eq!(names, ["Tiny", "Small", "Standard", "Large", "Huge"]);
        assert_eq!(col(|s| s.width), [60, 80, 100, 130, 160]);
        assert_eq!(col(|s| s.height), [60, 80, 100, 130, 160]);
        assert_eq!(col(|s| s.civ_count), [4, 6, 8, 12, 16]);
        assert_eq!(col(|s| s.distance_between_civs), [11, 11, 12, 18, 24]);
        assert_eq!(col(|s| s.optimal_city_count), [14, 17, 20, 28, 36]);
        assert_eq!(col(|s| s.tech_rate), [160, 200, 240, 320, 400]);
    }

    /// The 31-civ scenarios put the maximum in the column the editor clamps to
    /// 31 (and never in the distance column).
    #[test]
    fn thirty_one_civ_worlds() {
        let mut found = false;
        for (file, sizes) in tables() {
            if file.ends_with("Balancer.bix") {
                assert!(sizes.iter().all(|s| s.civ_count == 31));
                found = true;
            }
        }
        if !files().is_empty() {
            assert!(found);
        }
    }
}
