//! `CULT` - cultural opinion levels: how a civilization's culture compared with
//! another's decides propaganda success and the resistance of captured cities.
//!
//! Row reader `0x5E1180` (arm `0x594BAD`, in-memory stride `0x5C`); writer
//! `0x5E1070`. Editor dialog 165 *Culture* (help: *Culture Page*); apply
//! routine `0x42A80D`. Row body 88 bytes in every corpus version. *Border
//! Factor* and *Level Multiplier* are not here, they live on the General
//! Settings page (`RULE`).
//!
//! | body | mem | field | editor control |
//! |---|---|---|---|
//! | `0x00` | `0x04` | [`opinion`](CultureLevel::opinion) | combo 1496 text (`strncpy` of 63 bytes) |
//! | `0x40` | `0x44` | [`propaganda_success_percent`](CultureLevel::propaganda_success_percent) | edit 1508, `0..=100` |
//! | `0x44` | `0x48` | [`culture_ratio_percent`](CultureLevel::culture_ratio_percent) | derived by the editor from 1502/1505 |
//! | `0x48` | `0x4C` | [`culture_ratio_denominator`](CultureLevel::culture_ratio_denominator) | edit 1505 (right of the colon), `1..=10` |
//! | `0x4C` | `0x50` | [`culture_ratio_numerator`](CultureLevel::culture_ratio_numerator) | edit 1502 (left of the colon), `1..=10` |
//! | `0x50` | `0x54` | [`resistance_initial_percent`](CultureLevel::resistance_initial_percent) | edit 1511 *Initial*, `0..=100` |
//! | `0x54` | `0x58` | [`resistance_continued_percent`](CultureLevel::resistance_continued_percent) | edit 1516 *Continued*, `0..=100` |
//!
//! The editor stores the two ratio spin values *and* their quotient as a
//! percentage: `culture_ratio_percent = round(numerator / denominator * 100)`
//! (`fild; fidiv; fmul 100.0; fadd 0.5; ftol`, `0x42A911..0x42A933`). All 272
//! corpus rows satisfy it. The fields were first read in file order as
//! "numerator 30, denominator 300, propaganda 1, ..." from community field
//! lists; the editor's page-to-row bindings and the help text (*Culture
//! Ratio*, *Chance of Successful Propaganda*, *Resistance Chance*) fix the
//! order above.
//!
//! Loader arms `0x594BAD` and `0x594EA4` skip eight bytes before the row count
//! when `major + minor*0.01 < 4.0`; no pre-4.01 `CULT` in the corpus.

use crate::fixed_record;
use crate::io::Str;

fixed_record! {
    /// One cultural opinion band ("in awe of" ... "disdainful of"). Row 0 is the
    /// most favourable opinion (the other civilization has three times your
    /// culture), the last row the least.
    pub struct CultureLevel(b"CULT") {
        /// Opinion text shown in diplomacy ("in awe of", "respectful of", ...).
        pub opinion: Str<64>,
        /// *Chance of Successful Propaganda* in percent when the target holds
        /// this opinion of the initiating civilization (stock: 30 for "in awe of"
        /// down to 3 for "disdainful of").
        pub propaganda_success_percent: i32,
        /// The culture ratio as a percentage, derived by the editor from the two
        /// spins (see [`CultureLevel::ratio_percent`]): 300 for 3:1, 33 for 1:3.
        pub culture_ratio_percent: i32,
        /// Right spin of *Culture Ratio* (`1..=10`): the civilization's own
        /// culture in "other : own".
        pub culture_ratio_denominator: i32,
        /// Left spin of *Culture Ratio* (`1..=10`): the other civilization's
        /// culture. The help text's example: 3000 against 1000 is 3:1, "in awe
        /// of".
        pub culture_ratio_numerator: i32,
        /// *Resistance Chance / Initial*: percent chance a captured city resists
        /// at first, if its former owner holds this opinion of the conqueror
        /// (stock 40 for "in awe of" up to 90).
        pub resistance_initial_percent: i32,
        /// *Resistance Chance / Continued*: percent chance it keeps resisting
        /// each turn afterwards (stock: always `initial - 10`).
        pub resistance_continued_percent: i32,
    }
}

impl CultureLevel {
    /// What the editor stores in [`culture_ratio_percent`](Self::culture_ratio_percent)
    /// for the two spin values (`0x42A911..0x42A933`): the quotient times 100,
    /// rounded half up.
    pub fn ratio_percent(numerator: i32, denominator: i32) -> i32 {
        (f64::from(numerator) / f64::from(denominator) * 100.0 + 0.5) as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::Record;

    fn conquests_row(index: usize) -> Option<CultureLevel> {
        let files = crate::corpus::files();
        let f = files
            .iter()
            .find(|f| f.name().ends_with("Conquests/conquests.biq"))?;
        let row = crate::corpus::rows::<CultureLevel>(std::slice::from_ref(f))
            .into_iter()
            .find(|r| r.index == index)?;
        Some(CultureLevel::read(&mut crate::io::Reader::new(row.body), &f.ctx()).unwrap())
    }

    #[test]
    fn corpus_roundtrip() {
        if !crate::corpus::available() {
            return;
        }
        let st = crate::corpus::check_roundtrip::<CultureLevel>();
        assert_eq!(st.with_extra, 0);
        assert_eq!(st.lengths, vec![88]);
    }

    #[test]
    fn conquests_top_opinion() {
        let Some(c) = conquests_row(0) else {
            return;
        };
        assert_eq!(c.opinion.text(), "in awe of");
        assert_eq!(c.propaganda_success_percent, 30);
        assert_eq!(c.culture_ratio_percent, 300);
        assert_eq!(
            (c.culture_ratio_numerator, c.culture_ratio_denominator),
            (3, 1)
        );
        assert_eq!(c.resistance_initial_percent, 40);
        assert_eq!(c.resistance_continued_percent, 30);
    }

    #[test]
    fn conquests_weakest_opinion() {
        let Some(c) = conquests_row(5) else {
            return;
        };
        assert_eq!(c.opinion.text(), "disdainful of");
        assert_eq!(c.propaganda_success_percent, 3);
        assert_eq!(c.culture_ratio_percent, 33);
        assert_eq!(
            (c.culture_ratio_numerator, c.culture_ratio_denominator),
            (1, 3)
        );
        assert_eq!(
            (c.resistance_initial_percent, c.resistance_continued_percent),
            (90, 80)
        );
    }

    /// The editor derives the percentage from the spins; the spins and the
    /// percentages keep to the ranges of their controls (1..=10, 0..=100).
    #[test]
    fn editor_invariants_hold_in_every_file() {
        let mut rows = 0;
        for f in crate::corpus::files() {
            for row in crate::corpus::rows::<CultureLevel>(std::slice::from_ref(&f)) {
                let c =
                    CultureLevel::read(&mut crate::io::Reader::new(row.body), &f.ctx()).unwrap();
                let at = format!("{} row {}", f.name(), row.index);
                assert_eq!(
                    c.culture_ratio_percent,
                    CultureLevel::ratio_percent(
                        c.culture_ratio_numerator,
                        c.culture_ratio_denominator
                    ),
                    "{at}"
                );
                assert!((1..=10).contains(&c.culture_ratio_numerator), "{at}");
                assert!((1..=10).contains(&c.culture_ratio_denominator), "{at}");
                for p in [
                    c.propaganda_success_percent,
                    c.resistance_initial_percent,
                    c.resistance_continued_percent,
                ] {
                    assert!((0..=100).contains(&p), "{at}");
                }
                rows += 1;
            }
        }
        if !crate::corpus::files().is_empty() {
            assert!(rows > 200, "{rows}");
        }
    }

    /// Stock rules: the more the other civilization's culture exceeds yours,
    /// the likelier propaganda works and the weaker the resistance; continued
    /// resistance is ten points below the initial one.
    #[test]
    fn stock_levels_are_monotonic() {
        let rows: Vec<CultureLevel> = (0..6).filter_map(conquests_row).collect();
        if rows.is_empty() {
            return;
        }
        assert_eq!(rows.len(), 6);
        for pair in rows.windows(2) {
            assert!(pair[0].culture_ratio_percent > pair[1].culture_ratio_percent);
            assert!(pair[0].propaganda_success_percent > pair[1].propaganda_success_percent);
            assert!(pair[0].resistance_initial_percent < pair[1].resistance_initial_percent);
        }
        for c in &rows {
            assert_eq!(
                c.resistance_continued_percent,
                c.resistance_initial_percent - 10
            );
        }
    }

    #[test]
    fn ratio_percent_rounds_half_up() {
        assert_eq!(CultureLevel::ratio_percent(3, 1), 300);
        assert_eq!(CultureLevel::ratio_percent(1, 3), 33);
        assert_eq!(CultureLevel::ratio_percent(2, 3), 67);
        assert_eq!(CultureLevel::ratio_percent(3, 4), 75);
        assert_eq!(CultureLevel::ratio_percent(1, 8), 13);
    }
}
