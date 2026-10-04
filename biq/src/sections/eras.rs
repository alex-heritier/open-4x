//! `ERAS` - technology eras (Ancient Times … Modern Times).
//!
//! Loader arm `0x594AFC`, row reader `0x5E1790`, row writer `0x5E16A0`, table
//! allocator and row constructor `0x59BB20` (stride `0x10C`; memory offset =
//! body offset + 4). Editor dialog 141 (*Eras Page*), apply routine `0x42F3B0`.
//!
//! | body | field | evidence |
//! |------|-------|----------|
//! | `0x00` | `name` (64 bytes) | editor combo 1132, copy of 63 characters |
//! | `0x40` | `civilopedia_entry` (32 bytes) | editor edit 1616, copy of 31 |
//! | `0x60` | `researcher_titles` (5 × 32 bytes) | editor edits 1508, 1510, 1512, 1514, 1361 |
//! | `0x100` | `researcher_title_count` | counted by the editor's apply, read at `0x5995BC` |
//! | `0x104` | `unknown_0x104` (Conquests only) | the constructor stores `1`; nothing reads it |
//!
//! The editor's apply routine **compacts** the researcher list: it walks the
//! five edit boxes and appends each non-empty text to the next free title slot,
//! bumping the count, so the titles `count..5` are empty. The game's
//! researcher-title picker (`0x5995A0`) returns title `rand() % count`
//! (title 0 when the count is 0).

use crate::fixed_record;
use crate::io::Str;

fixed_record! {
    /// One technology era.
    pub struct Era(b"ERAS") {
        /// Era name (`Ancient Times`; body `+0`).
        pub name: Str<64>,
        /// Civilopedia key (`ERAS_Ancient_Times`; body `+0x40`).
        pub civilopedia_entry: Str<32>,
        /// Researcher titles shown on the science advisor; one is chosen at
        /// random in-game (body `+0x60`, five × 32 bytes). Only the first
        /// [`researcher_title_count`](Self::researcher_title_count) are in use.
        pub researcher_titles: [Str<32>; 5],
        /// How many of the titles are used (`3` on Ancient/Middle/Industrial
        /// in stock rules, `5` on Modern Times; body `+0x100`).
        pub researcher_title_count: i32,
    }
    since (12, 6) {
        /// Conquests tail dword (body `+0x104`). Always `1` in the corpus; the
        /// row constructor sets `1`, the editor's Eras page has no control for
        /// it and no game code reads it (HYPOTHESIS: a reserved/format flag).
        pub unknown_0x104: i32 = 1,
    }
}

impl Era {
    /// The researcher titles in use (the first `researcher_title_count`).
    pub fn researchers(&self) -> &[Str<32>] {
        let n = usize::try_from(self.researcher_title_count).unwrap_or(0);
        &self.researcher_titles[..n.min(self.researcher_titles.len())]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Version;
    use crate::corpus;
    use crate::io::Record;

    #[test]
    fn corpus_roundtrip() {
        if !crate::corpus::available() {
            return;
        }
        let st = corpus::check_roundtrip::<Era>();
        assert_eq!(st.with_extra, 0);
        assert_eq!(st.lengths, vec![260, 264]);
        assert_eq!(
            st.by_version
                .get(&Version::new(11, 18))
                .map(|v| v.as_slice()),
            Some(&[260][..])
        );
        assert_eq!(
            st.by_version
                .get(&Version::new(12, 8))
                .map(|v| v.as_slice()),
            Some(&[264][..])
        );
    }

    #[test]
    fn conquests_eras() {
        let files = corpus::files();
        let Some(f) = files.iter().find(|f| f.name().contains("conquests")) else {
            return;
        };
        let rows: Vec<Era> = corpus::rows::<Era>(std::slice::from_ref(f))
            .into_iter()
            .map(|r| Era::read(&mut crate::io::Reader::new(r.body), &f.ctx()).unwrap())
            .collect();
        assert_eq!(rows[0].name.text(), "Ancient Times");
        assert_eq!(rows[0].researcher_titles[0].text(), "Mystics");
        assert_eq!(rows[0].researcher_titles[1].text(), "Sages");
        assert_eq!(rows[0].researcher_title_count, 3);
        assert_eq!(rows[0].researchers().len(), 3);
        assert_eq!(rows[3].name.text(), "Modern Times");
        assert_eq!(rows[3].researcher_title_count, 5);
        assert_eq!(rows[3].researcher_titles[3].text(), "Researchers");
        assert_eq!(rows[3].unknown_0x104, 1);
    }

    /// What the editor's apply routine guarantees: the count is the number of
    /// titles and they are packed at the front.
    #[test]
    fn titles_are_packed_and_counted() {
        let mut rows = 0;
        for f in corpus::files() {
            for r in corpus::rows::<Era>(std::slice::from_ref(&f)) {
                let e = Era::read(&mut crate::io::Reader::new(r.body), &f.ctx()).unwrap();
                let used = e
                    .researcher_titles
                    .iter()
                    .take_while(|t| !t.text().is_empty())
                    .count();
                let all = e
                    .researcher_titles
                    .iter()
                    .filter(|t| !t.text().is_empty())
                    .count();
                let ctx = format!("{} / {}", f.name(), e.name.text());
                assert_eq!(used, all, "{ctx}: titles are packed");
                assert_eq!(e.researcher_title_count as usize, used, "{ctx}");
                rows += 1;
            }
        }
        if !corpus::files().is_empty() {
            assert!(rows > 100, "{rows} rows");
        }
    }

    #[test]
    fn constructor_default_of_the_tail_dword_is_one() {
        assert_eq!(Era::default().unknown_0x104, 1);
        assert_eq!(Era::default().researchers().len(), 0);
    }
}
