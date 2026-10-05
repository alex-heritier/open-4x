//! `SLOC` — civilization start locations on a scenario map.
//!
//! Arm `0x594412`, constructor `0x5E8270`, row reader `0x5E8290`, writer
//! `0x5E8330`. Editor tool *Player Starting Location* (overlay list entry in
//! editor string 1048; the editor refuses barbarians: "Barbarians cannot have
//! starting locations"). Row is **16** bytes: four dwords in `fread`/`fwrite`
//! order, constructor defaults `(0, 0, -1, -1)`.
//!
//! `owner_type`/`owner` follow [`crate::owner`]: `2` + `RACE` index is the
//! normal form (`Rise_of_Rome`: Rome = RACE 1 at Roma's tile), `3` + `LEAD`
//! index appears in scenarios with player rows, and `0` is an **unassigned**
//! start (27 rules-less `.bix`/`.bic` maps such as `Tower_of_Babel`, `Europe`
//! and `Close_quarters` carry only type-`0` rows; the game gives them to
//! whichever civilizations need a start). **C**
//!
//! The swap-players fix-up `0x599CF0` rewrites `owner` where `owner_type == 3`,
//! which is how the engine knows `3` indexes `LEAD`.

use crate::fixed_record;
use crate::owner::Owner;

fixed_record! {
    /// One start tile for a civilization or player slot.
    pub struct StartLocation(b"SLOC") {
        /// Body `+0x00`. `0` unassigned, `2` civilization, `3` player (`1` is never used here).
        pub owner_type: i32,
        /// Body `+0x04`. `RACE` index for type `2`, `LEAD` index for type `3`, `0` for type `0`.
        pub owner: i32,
        /// Body `+0x08`. Map X (staggered grid: `x + y` is even).
        pub map_x: i32,
        /// Body `+0x0C`. Map Y.
        pub map_y: i32,
    }
}

impl StartLocation {
    /// The decoded owner, `None` for an unknown `owner_type`.
    pub fn owner(&self) -> Option<Owner> {
        Owner::from_raw(self.owner_type, self.owner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::{check_roundtrip, load_all, rows};
    use crate::io::{Reader, Record};
    use crate::sections::terr::TerrainNumbering;
    use crate::sections::tile::{Tile, index_from_coords, terrain_id};
    use crate::sections::wmap::WorldMap;

    #[test]
    fn corpus_roundtrip() {
        if !crate::corpus::available() {
            return;
        }
        let st = check_roundtrip::<StartLocation>();
        assert_eq!(st.with_extra, 0);
        assert_eq!(st.short, 0);
        assert_eq!(st.lengths, vec![16]);
    }

    #[test]
    fn mesopotamia_first_start() {
        let files: Vec<_> = load_all().into_iter().filter_map(|f| f.ok()).collect();
        if files.is_empty() {
            return;
        }
        let file = files
            .iter()
            .find(|f| f.name().contains("Mesopotamia") && !f.name().contains("MP_"))
            .expect("Mesopotamia");
        let row = rows::<StartLocation>(std::slice::from_ref(file))
            .into_iter()
            .next()
            .expect("SLOC");
        let s = StartLocation::read(&mut Reader::new(row.body), &file.ctx()).unwrap();
        assert_eq!(s.owner(), Some(Owner::Civilization(2)));
        assert_eq!((s.map_x, s.map_y), (9, 33));
    }

    /// Rise of Rome: the `RACE` index of each start is the civilization whose
    /// capital stands there (Rome = RACE 1 at Roma, Carthage = RACE 8 at (44,92)).
    #[test]
    fn rise_of_rome_starts_are_race_indices() {
        let files: Vec<_> = load_all().into_iter().filter_map(|f| f.ok()).collect();
        let Some(file) = files
            .iter()
            .find(|f| f.name().contains("Rise_of_Rome") && !f.name().contains("MP_"))
        else {
            return;
        };
        let starts: Vec<StartLocation> = rows::<StartLocation>(std::slice::from_ref(file))
            .into_iter()
            .map(|r| StartLocation::read(&mut Reader::new(r.body), &file.ctx()).unwrap())
            .collect();
        let at = |x, y| starts.iter().find(|s| (s.map_x, s.map_y) == (x, y));
        assert_eq!(at(47, 67).unwrap().owner(), Some(Owner::Civilization(1)));
        assert_eq!(at(44, 92).unwrap().owner(), Some(Owner::Civilization(8)));
    }

    #[test]
    fn starts_are_on_land_tiles() {
        let files: Vec<_> = load_all().into_iter().filter_map(|f| f.ok()).collect();
        if files.is_empty() {
            return;
        }
        for file in &files {
            let wmap_rows = rows::<WorldMap>(std::slice::from_ref(file));
            if wmap_rows.is_empty() {
                continue;
            }
            let wmap = WorldMap::read(&mut Reader::new(wmap_rows[0].body), &file.ctx()).unwrap();
            let tiles: Vec<Tile> = rows::<Tile>(std::slice::from_ref(file))
                .into_iter()
                .map(|r| Tile::read(&mut Reader::new(r.body), &file.ctx()).unwrap())
                .collect();
            if tiles.is_empty() {
                continue;
            }
            for sloc in rows::<StartLocation>(std::slice::from_ref(file)) {
                let s = StartLocation::read(&mut Reader::new(sloc.body), &file.ctx()).unwrap();
                assert!(s.owner().is_some());
                assert_ne!(s.owner_type, 1, "barbarians have no start location");
                let idx = index_from_coords(s.map_x, s.map_y, wmap.width) as usize;
                assert!(idx < tiles.len(), "{} start out of range", file.name());
                let t = &tiles[idx];
                let terr_rows = file.raw.section(b"TERR").map_or(0, |s| s.rows.len());
                let numbering = TerrainNumbering::of_file(terr_rows, file.version);
                assert!(
                    !t.is_water(numbering),
                    "{} start ({},{}) on water",
                    file.name(),
                    s.map_x,
                    s.map_y
                );
                assert!(
                    numbering.to_current(t.terrain_id()) != terrain_id::OCEAN,
                    "{} start in ocean class",
                    file.name()
                );
            }
        }
    }
}
