//! `CLNY` - colonies, airfields, radar towers and outposts.
//!
//! These four *Tile Properties* check boxes (dialog 187) are objects with an
//! owner, unlike fortresses, barricades and the other overlays which are bits
//! of the tile (editor string 2312: "Colonies, Airfields, Radar Towers, and
//! Outposts must be assigned to a civilization or player").
//!
//! Arm `0x594D50`, constructor `0x5E0CF0`, row reader `0x5E0D10`, writer
//! `0x5E0DD0`. Fixed 20-byte rows of five dwords, read straight into the
//! object; the scenario keeps them in a `0x14`-byte-stride array
//! (`scenario+0xBB0`, count at `+0x884`).
//!
//! | body | field | constructor default |
//! |------|-------|---------------------|
//! | `+0x00` | [`Colony::owner_type`], see [`crate::owner`] | `2` (civilization) |
//! | `+0x04` | [`Colony::owner`] | `0` |
//! | `+0x08` | [`Colony::map_x`] | `-1` |
//! | `+0x0C` | [`Colony::map_y`] | `-1` |
//! | `+0x10` | [`Colony::kind`], see [`kind`] | `0` |
//!
//! The tile under a colony stores the row index in [`Tile::colony_id`]
//! (**C**: 44 of 44 colonies in the corpus) and the colony's kind as an
//! overlay bit ([`kind::overlay_bit`]). The loader clears the tile's
//! `colony_id` while reading `TILE` (`0x596DA3`), so the link is rebuilt from
//! these rows.
//!
//! [`Tile::colony_id`]: crate::sections::tile::Tile::colony_id

use crate::fixed_record;
use crate::owner::Owner;
use crate::sections::tile::overlay;

/// [`Colony::kind`] values.
///
/// **A**: `Cell::hasOverlay(10)` (`0x5E9BA3`) is true for a tile that has a
/// colony id and *none* of the airfield (`0x5EA610`), radar tower (`0x5EA9D0`)
/// and outpost (`0x5EA830`) bits (the "plain colony" test `0x5EA6E0`). The
/// editor lists the check boxes in this order (**B**, dialog 187), and the
/// corpus agrees on every row (**C**).
pub mod kind {
    use super::overlay;

    /// A plain colony.
    pub const COLONY: i32 = 0;
    /// An airfield; the tile has [`overlay::AIRFIELD`].
    pub const AIRFIELD: i32 = 1;
    /// A radar tower; the tile has [`overlay::RADAR_TOWER`].
    pub const RADAR_TOWER: i32 = 2;
    /// An outpost; the tile has [`overlay::OUTPOST`].
    pub const OUTPOST: i32 = 3;

    /// The overlay bit a tile carries for `kind` (`None` for a plain colony
    /// or an unknown kind).
    pub const fn overlay_bit(kind: i32) -> Option<u32> {
        match kind {
            AIRFIELD => Some(overlay::AIRFIELD),
            RADAR_TOWER => Some(overlay::RADAR_TOWER),
            OUTPOST => Some(overlay::OUTPOST),
            _ => None,
        }
    }
}

fixed_record! {
    /// One colony, airfield, radar tower or outpost.
    pub struct Colony(b"CLNY") {
        /// Body `+0x00`. Owner type, see [`crate::owner`]. **A** (constructor default `2`);
        /// `2` in `Fall_of_Rome` and `WWII_in_the_Pacific`, `3` in `Island_Hop`.
        pub owner_type: i32,
        /// Body `+0x04`. Race index or `LEAD` index according to [`Colony::owner_type`]. **A**
        pub owner: i32,
        /// Body `+0x08`. Map x; `x + y` is even. **A**
        pub map_x: i32,
        /// Body `+0x0C`. Map y. **A**
        pub map_y: i32,
        /// Body `+0x10`. What it is, see [`kind`]. **A + B + C**
        pub kind: i32,
    }
}

impl Colony {
    /// A colony row as the editor creates it.
    pub fn new(kind: i32, owner: Owner, map_x: i32, map_y: i32) -> Colony {
        let (owner_type, owner) = owner.to_raw();
        Colony {
            owner_type,
            owner,
            map_x,
            map_y,
            kind,
            ..Colony::default()
        }
    }

    /// Decoded owner; `None` for an unknown owner type.
    pub fn owner(&self) -> Option<Owner> {
        Owner::from_raw(self.owner_type, self.owner)
    }

    /// The overlay bit the tile under this colony must carry.
    pub fn overlay_bit(&self) -> Option<u32> {
        kind::overlay_bit(self.kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus;
    use crate::io::{Reader, Record};
    use crate::{Biq, owner};

    #[test]
    fn corpus_roundtrip() {
        if !crate::corpus::available() {
            return;
        }
        let st = corpus::check_roundtrip::<Colony>();
        assert_eq!(st.with_extra, 0);
        assert_eq!(st.short, 0);
        assert_eq!(st.lengths, vec![20]);
    }

    #[test]
    fn owner_roundtrip_and_new() {
        let c = Colony::new(kind::AIRFIELD, Owner::Player(2), 6, 4);
        assert_eq!((c.owner_type, c.owner), (owner::PLAYER, 2));
        assert_eq!(c.owner(), Some(Owner::Player(2)));
        assert_eq!(c.overlay_bit(), Some(overlay::AIRFIELD));
        assert_eq!(
            Colony::new(kind::COLONY, Owner::Nobody, 0, 0).overlay_bit(),
            None
        );
        assert_eq!(kind::overlay_bit(kind::OUTPOST), Some(overlay::OUTPOST));
        assert_eq!(kind::overlay_bit(99), None);
    }

    /// Every row is a plausible object on a land or coast tile whose own
    /// record points back at it.
    #[test]
    fn rows_agree_with_their_tiles() {
        let mut rows = 0;
        let mut kinds = [0usize; 4];
        for f in corpus::files() {
            let Ok(biq) = Biq::from_raw(&f.raw) else {
                continue;
            };
            let Some(map) = biq.map_view() else {
                continue;
            };
            for (i, c) in biq.scenario.colonies.iter().enumerate() {
                let name = f.name();
                let owner = c.owner().unwrap_or_else(|| panic!("{name}: owner type"));
                assert_ne!(owner, Owner::Nobody, "{name} row {i}");
                assert!((0..4).contains(&c.kind), "{name} row {i}: kind {}", c.kind);
                let t = map
                    .tile(c.map_x, c.map_y)
                    .unwrap_or_else(|| panic!("{name} row {i}: off map"));
                assert_eq!(t.colony_id as usize, i, "{name} row {i}: tile back-link");
                assert_eq!(t.city_id, -1, "{name} row {i}: colony inside a city");
                // The tile carries exactly the bit for this kind.
                let bits = t.overlay_plane() & overlay::COLONY_KINDS;
                assert_eq!(bits, c.overlay_bit().unwrap_or(0), "{name} row {i}");
                assert!(!map.is_water(t), "{name} row {i}: colony on water");
                kinds[c.kind as usize] += 1;
                rows += 1;
            }
        }
        if !corpus::files().is_empty() {
            // Island_Hop 24, Fall_of_Rome 1 + 1 (MP), WWII 9 + 9 (MP).
            assert_eq!(rows, 44, "colonies in the corpus");
            assert_eq!(kinds, [26, 4, 14, 0]);
        }
    }

    #[test]
    fn island_hop_colonies_are_player_owned() {
        let files = corpus::files();
        let Some(file) = files.iter().find(|f| f.name().contains("Island_Hop")) else {
            return;
        };
        let sec = file.raw.section(b"CLNY").expect("CLNY");
        assert_eq!(sec.rows.len(), 24);
        let mut per_player = [0; 4];
        for row in &sec.rows {
            let c = Colony::read(&mut Reader::new(file.raw.row(row)), &file.ctx()).unwrap();
            assert_eq!(c.owner_type, owner::PLAYER);
            assert_eq!(c.kind, kind::COLONY);
            per_player[c.owner as usize] += 1;
        }
        // Six colonies for each of the four players.
        assert_eq!(per_player, [6, 6, 6, 6]);
    }

    #[test]
    fn fall_of_rome_has_one_colony() {
        let files = corpus::files();
        let Some(file) = files
            .iter()
            .find(|f| f.name().ends_with("3_Fall_of_Rome.biq") && f.name().contains("Conquests"))
        else {
            return;
        };
        let sec = file.raw.section(b"CLNY").expect("CLNY");
        assert_eq!(sec.rows.len(), 1);
        let c = Colony::read(&mut Reader::new(file.raw.row(&sec.rows[0])), &file.ctx()).unwrap();
        assert_eq!(
            c,
            Colony::new(kind::COLONY, Owner::Civilization(5), 135, 101)
        );
    }

    #[test]
    fn wwii_pacific_has_airfields_and_radars() {
        let files = corpus::files();
        let Some(file) = files
            .iter()
            .find(|f| f.name().contains("Conquests/9_WWII_in_the_Pacific"))
        else {
            return;
        };
        let biq = Biq::from_raw(&file.raw).unwrap();
        let count = |k| biq.scenario.colonies.iter().filter(|c| c.kind == k).count();
        assert_eq!(count(kind::AIRFIELD), 2);
        assert_eq!(count(kind::RADAR_TOWER), 7);
        // Barricades are plain overlay bits, not objects.
        let v = biq.map_view().unwrap();
        let barricades = v
            .iter()
            .filter(|(_, t)| t.has_overlay(overlay::BARRICADE))
            .count();
        assert!(barricades > 0);
        assert!(v.iter().all(|(_, t)| !t.has_overlay(overlay::OUTPOST)));
    }
}
