//! One module per section tag. Each defines the row type for that section,
//! documents the field layout with the evidence behind it, and implements
//! [`crate::io::Record`].

pub mod bldg;
pub mod city;
pub mod clny;
pub mod cont;
pub mod ctzn;
pub mod cult;
pub mod diff;
pub mod eras;
pub mod espn;
pub mod expr;
pub mod flav;
pub mod game;
pub mod good;
pub mod govt;
pub mod lead;
pub mod prto;
pub mod race;
pub mod rule;
pub mod sloc;
pub mod tech;
pub mod terr;
pub mod tfrm;
pub mod tile;
pub mod unit;
pub mod ver;
pub mod wchr;
pub mod wmap;
pub mod wsiz;

#[cfg(test)]
mod tests {
    use crate::Version;
    use crate::corpus;
    use crate::io::{Reader, Record, Writer};

    /// Bytes a reader does not recognise must come back out of the writer, and
    /// show up in `extra()` so `Biq::unmodelled_bytes` can report them. Checked
    /// with the first Conquests row of the type with three bytes appended (fewer
    /// than any field needs).
    fn keeps_trailing_bytes<R: Record>() {
        let files = corpus::files();
        let Some(file) = files.iter().find(|f| {
            f.version >= Version::new(12, 6)
                && !corpus::rows::<R>(std::slice::from_ref(f)).is_empty()
        }) else {
            return;
        };
        let rows = corpus::rows::<R>(std::slice::from_ref(file));
        let mut body = rows[0].body.to_vec();
        body.extend_from_slice(&[0xAB, 0xCD, 0xEF]);
        let ctx = file.ctx();
        let tag = String::from_utf8_lossy(&R::TAG).into_owned();
        let rec = R::read(&mut Reader::new(&body), &ctx).unwrap_or_else(|e| panic!("{tag}: {e}"));
        assert_eq!(rec.extra(), [0xAB, 0xCD, 0xEF], "{tag}");
        let mut w = Writer::new();
        rec.write(&mut w, &ctx);
        assert_eq!(w.buf, body, "{tag}");
    }

    /// A Conquests row read as if the file were far older than any layout change keeps
    /// everything beyond that version's layout in `extra`, so it still writes back
    /// unchanged. (The game reads by row length, not version; the crate's version gates
    /// must never turn that into silent data loss.)
    fn survives_an_older_version<R: Record>() {
        let files = corpus::files();
        let Some(file) = files.iter().find(|f| {
            f.version >= Version::new(12, 6)
                && !corpus::rows::<R>(std::slice::from_ref(f)).is_empty()
        }) else {
            return;
        };
        let old = crate::io::Ctx {
            version: Version::new(2, 5),
        };
        for row in corpus::rows::<R>(std::slice::from_ref(file))
            .iter()
            .take(60)
        {
            let tag = String::from_utf8_lossy(&R::TAG).into_owned();
            let rec = R::read(&mut Reader::new(row.body), &old)
                .unwrap_or_else(|e| panic!("{tag} #{}: {e}", row.index));
            let mut w = Writer::new();
            rec.write(&mut w, &old);
            assert_eq!(w.buf, row.body, "{tag} #{}", row.index);
        }
    }

    #[test]
    fn newer_rows_in_older_files_are_not_lost() {
        survives_an_older_version::<super::bldg::Building>();
        survives_an_older_version::<super::city::City>();
        survives_an_older_version::<super::ctzn::Citizen>();
        survives_an_older_version::<super::diff::Difficulty>();
        survives_an_older_version::<super::eras::Era>();
        survives_an_older_version::<super::game::Game>();
        survives_an_older_version::<super::govt::Government>();
        survives_an_older_version::<super::lead::Player>();
        survives_an_older_version::<super::prto::UnitType>();
        survives_an_older_version::<super::rule::GeneralRules>();
        survives_an_older_version::<super::tech::Tech>();
        survives_an_older_version::<super::terr::Terrain>();
        survives_an_older_version::<super::unit::Unit>();
    }

    /// Every aligned word of real rows replaced by hostile values (counts and
    /// sizes the format trusts), then read, written and read again. A row may be
    /// rejected, but must not panic, hang or allocate on the strength of the
    /// bogus number.
    fn survives_hostile_words<R: Record>() -> usize {
        let files = corpus::files();
        // one file per generation, so each layout of the type is covered
        let mut picked: Vec<&corpus::CorpusFile> = Vec::new();
        for major in [2u32, 3, 4, 11, 12] {
            if let Some(f) = files.iter().find(|f| {
                f.version.major == major && !corpus::rows::<R>(std::slice::from_ref(f)).is_empty()
            }) {
                picked.push(f);
            }
        }
        let tag = String::from_utf8_lossy(&R::TAG).into_owned();
        let hostile = [
            0u32,
            1,
            0x7FFF_FFFF,
            0x8000_0000,
            0xFFFF_FFFF,
            0x10_0000,
            0x100_0000,
        ];
        let mut cases = 0;
        for f in picked {
            let ctx = f.ctx();
            // first rows plus the longest, so variable-length rows are covered
            let mut rows = corpus::rows::<R>(std::slice::from_ref(f));
            rows.sort_by_key(|r| std::cmp::Reverse(r.body.len()));
            let longest = rows.first().map(|r| r.index);
            let rows: Vec<_> = corpus::rows::<R>(std::slice::from_ref(f))
                .into_iter()
                .filter(|r| r.index < 2 || Some(r.index) == longest)
                .collect();
            for row in rows {
                // words at every 4-byte step; large rows (GAME, RACE) sampled by stride
                let words = row.body.len() / 4;
                let stride = (words / 600).max(1);
                for w in (0..words).step_by(stride).chain([words.saturating_sub(1)]) {
                    for &v in &hostile {
                        let mut body = row.body.to_vec();
                        body[w * 4..w * 4 + 4].copy_from_slice(&v.to_le_bytes());
                        cases += 1;
                        let (res, largest) = crate::alloc_probe::largest_during(|| {
                            std::panic::catch_unwind(|| {
                                if let Ok(rec) = R::read(&mut Reader::new(&body), &ctx) {
                                    let mut out = Writer::new();
                                    rec.write(&mut out, &ctx);
                                    let _ = R::read(&mut Reader::new(&out.buf), &ctx);
                                }
                            })
                        });
                        assert!(
                            res.is_ok(),
                            "{tag}: panic with word {w} = {v:#x} in {} row {}",
                            f.name(),
                            row.index
                        );
                        assert!(
                            largest < crate::file::fuzz::MAX_ALLOC,
                            "{tag}: allocated {} MiB with word {w} = {v:#x} in {} row {}",
                            largest >> 20,
                            f.name(),
                            row.index
                        );
                    }
                }
            }
        }
        cases
    }

    #[test]
    fn hostile_words_never_panic_or_allocate_wildly() {
        macro_rules! all {
            ($($t:ty),* $(,)?) => {{ let mut n = 0; $( n += survives_hostile_words::<$t>(); )* n }};
        }
        let cases = all!(
            super::bldg::Building,
            super::city::City,
            super::clny::Colony,
            super::cont::Continent,
            super::ctzn::Citizen,
            super::cult::CultureLevel,
            super::diff::Difficulty,
            super::eras::Era,
            super::espn::EspionageMission,
            super::expr::ExperienceLevel,
            super::game::Game,
            super::good::Good,
            super::govt::Government,
            super::lead::Player,
            super::prto::UnitType,
            super::race::Civilization,
            super::rule::GeneralRules,
            super::sloc::StartLocation,
            super::tech::Tech,
            super::terr::Terrain,
            super::tfrm::WorkerJob,
            super::tile::Tile,
            super::unit::Unit,
            super::wchr::WorldCharacteristics,
            super::wmap::WorldMap,
            super::wsiz::WorldSize,
        );
        eprintln!("{cases} hostile rows");
    }

    #[test]
    fn every_record_type_keeps_unmodelled_trailing_bytes() {
        keeps_trailing_bytes::<super::bldg::Building>();
        keeps_trailing_bytes::<super::city::City>();
        keeps_trailing_bytes::<super::clny::Colony>();
        keeps_trailing_bytes::<super::cont::Continent>();
        keeps_trailing_bytes::<super::ctzn::Citizen>();
        keeps_trailing_bytes::<super::cult::CultureLevel>();
        keeps_trailing_bytes::<super::diff::Difficulty>();
        keeps_trailing_bytes::<super::eras::Era>();
        keeps_trailing_bytes::<super::espn::EspionageMission>();
        keeps_trailing_bytes::<super::expr::ExperienceLevel>();
        keeps_trailing_bytes::<super::game::Game>();
        keeps_trailing_bytes::<super::good::Good>();
        keeps_trailing_bytes::<super::govt::Government>();
        keeps_trailing_bytes::<super::lead::Player>();
        keeps_trailing_bytes::<super::prto::UnitType>();
        keeps_trailing_bytes::<super::race::Civilization>();
        keeps_trailing_bytes::<super::rule::GeneralRules>();
        keeps_trailing_bytes::<super::sloc::StartLocation>();
        keeps_trailing_bytes::<super::tech::Tech>();
        keeps_trailing_bytes::<super::terr::Terrain>();
        keeps_trailing_bytes::<super::tfrm::WorkerJob>();
        keeps_trailing_bytes::<super::tile::Tile>();
        keeps_trailing_bytes::<super::unit::Unit>();
        keeps_trailing_bytes::<super::wchr::WorldCharacteristics>();
        keeps_trailing_bytes::<super::wmap::WorldMap>();
        keeps_trailing_bytes::<super::wsiz::WorldSize>();
    }
}
