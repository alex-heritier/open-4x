//! `CONT` — per-continent summary rows (in-use flag + tile count).
//!
//! Arm `0x594D26`, worker `0x5951C0`, row reader `0x5E0FE0`, writer `0x5E0F50`.
//! Row is **8** bytes in every corpus file that carries the section.
//!
//! The reader defaults memory `Continent+0x20` to `1` and `+0x24` to `0` before
//! `fread` (`0x5E1007`, `0x5E101B`). Row index matches [`crate::sections::tile::Tile::continent_id`].

use crate::fixed_record;

/// Values on [`Continent::in_use`] (`Continent+0x20` in memory, `NOTES.md` §3).
pub mod in_use {
    /// Water bodies; 184 such rows in the corpus also hold land tiles (C).
    pub const SECONDARY: i32 = 0;
    /// Land continents. Every tile of such a row is land (C: 332 552 tiles, none
    /// water), and the number of these rows is `WMAP` `land_continent_count` (C: all
    /// files that have both sections).
    pub const PRIMARY: i32 = 1;
}

fixed_record! {
    /// One numbered continent after map finalization (`Map::vfunc(0x7C)` / `0x5EB7D0`).
    pub struct Continent(b"CONT") {
        /// Body `+0x00`. Continent class flag (`Continent+0x20`; A), see [`in_use`]:
        /// 1 marks a land continent, 0 a water body (possibly with stray land tiles).
        pub in_use: i32,
        /// Body `+0x04`. Tiles assigned this continent index (`Continent+0x24`; C).
        pub tile_count: i32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::{check_roundtrip, load_all, rows};
    use crate::io::{Reader, Record};
    use crate::sections::tile::Tile;

    #[test]
    fn corpus_roundtrip() {
        if !crate::corpus::available() {
            return;
        }
        let st = check_roundtrip::<Continent>();
        assert_eq!(st.with_extra, 0);
        assert_eq!(st.short, 0);
        assert_eq!(st.lengths, vec![8]);
    }

    #[test]
    fn mesopotamia_largest_continent() {
        let files: Vec<_> = load_all().into_iter().filter_map(|f| f.ok()).collect();
        if files.is_empty() {
            return;
        }
        let file = files
            .iter()
            .find(|f| f.name().contains("Mesopotamia") && !f.name().contains("MP_"))
            .expect("Mesopotamia");
        let conts: Vec<Continent> = rows::<Continent>(std::slice::from_ref(file))
            .into_iter()
            .map(|r| Continent::read(&mut Reader::new(r.body), &file.ctx()).unwrap())
            .collect();
        let (idx, biggest) = conts
            .iter()
            .enumerate()
            .max_by_key(|(_, c)| c.tile_count)
            .unwrap();
        assert_eq!(biggest.tile_count, 2760);
        assert_eq!(biggest.in_use, in_use::PRIMARY);
        assert_eq!(idx, 0);
    }

    #[test]
    fn tile_count_matches_tile_continent_ids() {
        let files: Vec<_> = load_all().into_iter().filter_map(|f| f.ok()).collect();
        if files.is_empty() {
            return;
        }
        for file in &files {
            let cont_rows = rows::<Continent>(std::slice::from_ref(file));
            if cont_rows.is_empty() {
                continue;
            }
            let conts: Vec<Continent> = cont_rows
                .into_iter()
                .map(|r| Continent::read(&mut Reader::new(r.body), &file.ctx()).unwrap())
                .collect();
            let mut tallies = vec![0i32; conts.len()];
            for row in rows::<Tile>(std::slice::from_ref(file)) {
                let t = Tile::read(&mut Reader::new(row.body), &file.ctx()).unwrap();
                let cid = t.continent_id;
                if cid < 0 || cid as usize >= tallies.len() {
                    continue;
                }
                tallies[cid as usize] += 1;
            }
            for (i, c) in conts.iter().enumerate() {
                assert_eq!(c.tile_count, tallies[i], "{} continent {}", file.name(), i);
            }
        }
    }
}
