//! `WMAP` — generated / scenario map parameters (dimensions, resource rolls,
//! start-placement seeds, water level, wrap flags).
//!
//! Arm `0x594A68`, worker `0x596F80`, row reader `0x5F41B0` (+ init `0x5F3A30`),
//! writer `0x5F3FD0`. Editor dialog **8** (`WMAP`, `editor.md`).
//!
//! Row length is **`168 + 4 × resource_count`** bytes where `resource_count` is
//! the first body dword and equals the `GOOD` section row count in every corpus
//! file (`reads.py` on `0x5F3FD0`). There is no separate 124-byte tail beyond
//! the 32-dword block at `+0x16C` (community lists that folded the two Map
//! start arrays into one on-disk array).

use crate::io::{Ctx, Reader, Record, Result, Writer};

/// Bits on [`WorldMap::wrap_flags`] (`Map+0x1F0`, `NOTES.md` §3 / §6).
pub mod wrap {
    /// Wrap east/west (`NOTES.md` §11.2).
    pub const X: u32 = 1 << 0;
    /// Wrap north/south.
    pub const Y: u32 = 1 << 1;
    /// Passed into fractal generation (`FractalMap.flags` bit 2, `NOTES.md` §6).
    pub const FRACTAL_SCALE_BY_100: u32 = 1 << 2;
    /// Ocean poles / polar ice cap behaviour (fractal bit 3, `NOTES.md` §6).
    pub const OCEAN_POLES: u32 = 1 << 3;
}

/// One `WMAP` row (variable length only because of [`WorldMap::resource_rolls`]).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct WorldMap {
    /// Body `+0x00`. Count of [`Self::resource_rolls`]; equals `GOOD` row count (C).
    pub resource_count: i32,
    /// Body `+0x04`, length `resource_count`. Per-resource mapgen rolls (`Map+0x14C`
    /// heap array; reader `0x5F426A`). Values differ from `GOOD` appearance ratios (C).
    pub resource_rolls: Vec<i32>,
    /// Body after rolls, map `+0x150` / `Map+0x40` (`u16` in memory). The number of
    /// land continents: it equals the number of `CONT` rows whose `in_use` flag is 1
    /// in every corpus file that has both sections (C: 92/92; Mesopotamia 13, WWII in
    /// the Pacific 80). Not a player count.
    pub land_continent_count: i32,
    /// Map height in tiles (`Map+0x154`; A).
    pub height: i32,
    /// Start-site search radius (`Map+0x158`; A — halved in late `finalPass` levels).
    pub start_site_radius: i32,
    /// Map `+0x15C`: the editor's *Number of Players* (Scenario page, control 1723; B
    /// `0x44A5B0`). The map generator uses it as the resource-quantity scaler in
    /// `placeResources` (`resources.md`; A) and as the start-slot bound in `finalPass`
    /// (`NOTES.md` §11.11); earlier notes call it "area factor". Equals the `LEAD`
    /// row count whenever both sections exist (C: 25/25 corpus files) and is the
    /// generator's player count (4..=24) in files without scenario data.
    pub number_of_players: i32,
    /// Map `+0x160`. Integer square root of `(width/2)×height` cell count (C: 161/161 maps).
    pub cell_count_isqrt: i32,
    /// Body `+0x164`, map `+0x164`. Always `0` in the corpus (D).
    pub unknown_0x164: i32,
    /// Map width in tiles (`Map+0x168`; A). Cell count `(width/2)*height` equals
    /// `TILE` row count when a map is present (C).
    pub width: i32,
    /// Map `+0x16C`, 32×`i32` (`0x5F438E`). Start candidate / shuffle seeds written
    /// before `finalPass`; `-1` when unused (A/D).
    pub start_candidate_seeds: [i32; 32],
    /// Oceans % seed (`Map+0x1EC`, 0..100; A — `NOTES.md` §3).
    pub water_level: i32,
    /// [`wrap`] flags (`Map+0x1F0`; A). Corpus maps store `0xFFFFFFFF` (all bits set).
    pub wrap_flags: i32,
    /// Trailing bytes when the row is longer than this layout (always empty in corpus).
    pub extra: Vec<u8>,
}

impl WorldMap {
    /// Hex cell count `(width/2)×height` used by [`Self::cell_count_isqrt`].
    #[inline]
    pub fn cell_count(&self) -> i32 {
        (self.width / 2).max(0) * self.height.max(0)
    }
}

impl Record for WorldMap {
    const TAG: [u8; 4] = *b"WMAP";

    fn read(r: &mut Reader<'_>, _ctx: &Ctx) -> Result<Self> {
        let mut rec = Self::default();
        let Some(count) = r.i32() else {
            rec.extra = r.rest().to_vec();
            return Ok(rec);
        };
        rec.resource_count = count;
        if count > 0 {
            let n = count as usize;
            let Some(need) = n.checked_mul(4) else {
                return Err(crate::io::Error::BadCount {
                    tag: Self::TAG,
                    what: "resource_rolls",
                    count: count as u32,
                });
            };
            if r.remaining() < need {
                return Err(crate::io::Error::BadCount {
                    tag: Self::TAG,
                    what: "resource_rolls",
                    count: count as u32,
                });
            }
            rec.resource_rolls = (0..n).map(|_| r.i32().unwrap()).collect();
        }
        macro_rules! dword {
            ($field:ident) => {
                if r.remaining() >= 4 {
                    rec.$field = r.i32().unwrap();
                }
            };
        }
        dword!(land_continent_count);
        dword!(height);
        dword!(start_site_radius);
        dword!(number_of_players);
        dword!(cell_count_isqrt);
        dword!(unknown_0x164);
        dword!(width);
        if r.remaining() >= 128 {
            for slot in &mut rec.start_candidate_seeds {
                *slot = r.i32().unwrap();
            }
        }
        dword!(water_level);
        dword!(wrap_flags);
        rec.extra = r.rest().to_vec();
        Ok(rec)
    }

    fn write(&self, w: &mut Writer, ctx: &Ctx) {
        let _ = ctx;
        let count = self.resource_rolls.len() as i32;
        w.i32(count);
        for v in &self.resource_rolls {
            w.i32(*v);
        }
        w.i32(self.land_continent_count);
        w.i32(self.height);
        w.i32(self.start_site_radius);
        w.i32(self.number_of_players);
        w.i32(self.cell_count_isqrt);
        w.i32(self.unknown_0x164);
        w.i32(self.width);
        for v in &self.start_candidate_seeds {
            w.i32(*v);
        }
        w.i32(self.water_level);
        w.i32(self.wrap_flags);
        w.bytes(&self.extra);
    }

    fn extra(&self) -> &[u8] {
        &self.extra
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::{check_roundtrip, load_all, rows};
    use crate::io::{Reader, Record};
    use crate::sections::good::Good;
    use crate::sections::lead::Player;

    #[test]
    fn corpus_roundtrip() {
        let st = check_roundtrip::<WorldMap>();
        assert_eq!(st.with_extra, 0);
        assert_eq!(st.short, 0);
    }

    #[test]
    fn row_length_is_168_plus_four_per_good() {
        let files: Vec<_> = load_all().into_iter().filter_map(|f| f.ok()).collect();
        if files.is_empty() {
            return;
        }
        for file in &files {
            let Some(wmap) = file.raw.section(b"WMAP") else {
                continue;
            };
            if wmap.rows.len() != 1 {
                continue;
            }
            let body = file.raw.row(&wmap.rows[0]);
            let m = WorldMap::read(&mut Reader::new(body), &file.ctx()).unwrap();
            assert_eq!(
                body.len(),
                168 + 4 * m.resource_rolls.len(),
                "{}",
                file.name()
            );
            assert_eq!(m.resource_count as usize, m.resource_rolls.len());
            if let Some(good) = file.raw.section(b"GOOD") {
                assert_eq!(m.resource_rolls.len(), good.rows.len(), "{}", file.name());
            }
        }
    }

    #[test]
    fn mesopotamia_dimensions_and_tile_count() {
        let files: Vec<_> = load_all().into_iter().filter_map(|f| f.ok()).collect();
        if files.is_empty() {
            return;
        }
        let file = files
            .iter()
            .find(|f| f.name().contains("Mesopotamia") && !f.name().contains("MP_"))
            .expect("Mesopotamia");
        let row = rows::<WorldMap>(std::slice::from_ref(file))
            .into_iter()
            .next()
            .expect("WMAP");
        let m = WorldMap::read(&mut Reader::new(row.body), &file.ctx()).unwrap();
        assert_eq!(m.width, 90);
        assert_eq!(m.height, 84);
        assert_eq!(m.land_continent_count, 13);
        assert_eq!(m.number_of_players, 7);
        assert_eq!(m.cell_count_isqrt, 61);
        assert_eq!(m.cell_count(), 3780);
        let tiles = file.raw.section(b"TILE").unwrap().rows.len();
        assert_eq!((m.width / 2) * m.height, tiles as i32);
        let good_n = rows::<Good>(std::slice::from_ref(file)).len();
        assert_eq!(m.resource_rolls.len(), good_n);
        let lead_n = rows::<Player>(std::slice::from_ref(file)).len();
        assert_eq!(m.number_of_players, lead_n as i32);
    }

    #[test]
    fn cell_count_isqrt_matches_hex_cell_count() {
        let files: Vec<_> = load_all().into_iter().filter_map(|f| f.ok()).collect();
        if files.is_empty() {
            return;
        }
        for file in &files {
            let Some(sec) = file.raw.section(b"WMAP") else {
                continue;
            };
            if sec.rows.len() != 1 {
                continue;
            }
            let m =
                WorldMap::read(&mut Reader::new(file.raw.row(&sec.rows[0])), &file.ctx()).unwrap();
            let cells = m.cell_count();
            if cells <= 0 {
                continue;
            }
            let isqrt = (cells as u64).isqrt() as i32;
            assert_eq!(m.cell_count_isqrt, isqrt, "{}", file.name());
        }
    }

    #[test]
    fn number_of_players_matches_lead_rows_when_present() {
        let files: Vec<_> = load_all().into_iter().filter_map(|f| f.ok()).collect();
        if files.is_empty() {
            return;
        }
        for file in &files {
            if file.raw.section(b"LEAD").is_none() || file.raw.section(b"WMAP").is_none() {
                continue;
            }
            let wmap_rows = rows::<WorldMap>(std::slice::from_ref(file));
            if wmap_rows.is_empty() {
                continue;
            }
            let m = WorldMap::read(&mut Reader::new(wmap_rows[0].body), &file.ctx()).unwrap();
            let lead_n = rows::<Player>(std::slice::from_ref(file)).len();
            assert_eq!(m.number_of_players, lead_n as i32, "{}", file.name());
        }
    }

    /// `land_continent_count` is the number of `CONT` rows flagged `in_use`.
    #[test]
    fn land_continent_count_matches_cont_rows() {
        use crate::sections::cont::{Continent, in_use};
        let files: Vec<_> = load_all().into_iter().filter_map(|f| f.ok()).collect();
        for file in &files {
            let wmap = rows::<WorldMap>(std::slice::from_ref(file));
            let conts = rows::<Continent>(std::slice::from_ref(file));
            if wmap.is_empty() || conts.is_empty() {
                continue;
            }
            let m = WorldMap::read(&mut Reader::new(wmap[0].body), &file.ctx()).unwrap();
            let land = conts
                .iter()
                .filter(|r| {
                    Continent::read(&mut Reader::new(r.body), &file.ctx())
                        .unwrap()
                        .in_use
                        == in_use::PRIMARY
                })
                .count();
            assert_eq!(m.land_continent_count, land as i32, "{}", file.name());
        }
    }
}
