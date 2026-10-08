//! `GOOD` - natural resources (bonus, luxury and strategic).
//!
//! Row reader `0x5E3860` (loop in the `GOOD` arm `0x59454B`, 92-byte in-memory
//! stride `0x5C`; memory offset = body offset + 4); writer `0x5E3740`. The file
//! row is 88 bytes in every version in the corpus (4.01, 11.18, 12.xx).
//!
//! Every field was checked against the editor's apply routine (`0x435760`,
//! dialog 135 *Natural Resources*): name (copy of 23 characters), civilopedia
//! key (copy of 31), the *Type* radio, the two probability edits, the icon spin
//! box, the prerequisite combo (index − 1, so `-1` = none) and the three bonus
//! edits clamped to `-25..=25`. For a *Bonus Resource* the editor stores `0` in
//! both probabilities; otherwise the appearance ratio is clamped to `0..=900`
//! and the disappearance probability to `0..=10000`.

use crate::fixed_record;
use crate::io::Str;

/// `kind` values.
pub mod kind {
    /// Bonus resource (Wheat, Cattle, Fish, ...): placed by terrain odds.
    pub const BONUS: u32 = 0;
    /// Luxury resource (Wines, Furs, Incense, ...): traded between cities.
    pub const LUXURY: u32 = 1;
    /// Strategic resource (Horses, Iron, ...): enables units/buildings.
    pub const STRATEGIC: u32 = 2;
}

fixed_record! {
    /// One natural resource.
    pub struct Good(b"GOOD") {
        /// Display name (`Horses`).
        pub name: Str<24>,
        /// Civilopedia key (`GOOD_Horses`).
        pub civilopedia_entry: Str<32>,
        /// Type radio (dialog 135; body `+0x38`): [`kind`] 0 bonus / 1 luxury / 2 strategic.
        pub kind: u32,
        /// "Appearance Ratio" (edit 1017; body `+0x3c`; `0..=900`, `0` for a
        /// bonus resource): how many instances appear per player ("in an
        /// eight-player game 160 is two of each resource per player"); `0`
        /// distributes the resource at random. Consumer `0x5f22a0`; `0` → roll
        /// `rand(26)+rand(26)+50`.
        pub appearance_ratio: i32,
        /// "Disappearance Probability" (edit 1018; body `+0x40`; `0..=10000`,
        /// `0` for a bonus resource): `N` means a 1 in `N` chance each turn that
        /// the resource is "used up" and re-appears somewhere else; `0` = never.
        /// Stock: `800`/`400`/`200`/`100` on the depletable strategics.
        pub disappearance_probability: i32,
        /// Icon cell in `Art/resources.pcx` (body `+0x44`; editor spin box 1013).
        pub icon: i32,
        /// Tech index (into `TECH`) that must be researched before the resource
        /// appears on the map, `-1` for none (body `+0x48`; editor
        /// "Prerequisite", combo 1074).
        pub prerequisite: i32,
        /// Food bonus to the tile (body `+0x4C`; editor "Bonuses: Food", 1014;
        /// `-25..=25`).
        pub food_bonus: i32,
        /// Shield bonus to the tile (body `+0x50`; edit 1015; `-25..=25`).
        pub shield_bonus: i32,
        /// Commerce bonus to the tile (body `+0x54`; edit 1016; `-25..=25`).
        pub commerce_bonus: i32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::Record;

    #[test]
    fn corpus_roundtrip() {
        if !crate::corpus::available() {
            return;
        }
        let st = crate::corpus::check_roundtrip::<Good>();
        assert_eq!(st.with_extra, 0);
        assert_eq!(st.short, 0);
        assert_eq!(st.lengths, vec![88]);
    }

    /// The editor clamps every control; a field at the wrong offset breaks this.
    #[test]
    fn editor_ranges_hold_in_every_file() {
        let mut rows = 0;
        for f in crate::corpus::files() {
            for r in crate::corpus::rows::<Good>(std::slice::from_ref(&f)) {
                let g = Good::read(&mut crate::io::Reader::new(r.body), &f.ctx()).unwrap();
                let who = format!("{} / {}", f.name(), g.name.text());
                assert!(g.kind <= kind::STRATEGIC, "{who}");
                assert!((0..=900).contains(&g.appearance_ratio), "{who}");
                assert!((0..=10_000).contains(&g.disappearance_probability), "{who}");
                if g.kind == kind::BONUS {
                    assert_eq!((g.appearance_ratio, g.disappearance_probability), (0, 0));
                }
                for bonus in [g.food_bonus, g.shield_bonus, g.commerce_bonus] {
                    assert!((-25..=25).contains(&bonus), "{who}");
                }
                assert!(g.prerequisite >= -1, "{who}");
                rows += 1;
            }
        }
        if !crate::corpus::files().is_empty() {
            assert!(rows > 1000, "{rows} rows");
        }
    }

    #[test]
    fn conquests_horses_strategic() {
        let files = crate::corpus::files();
        let Some(f) = files
            .iter()
            .find(|f| f.name().ends_with("Conquests/conquests.biq"))
        else {
            return;
        };
        let rows: Vec<Good> = crate::corpus::rows::<Good>(std::slice::from_ref(f))
            .into_iter()
            .map(|r| Good::read(&mut crate::io::Reader::new(r.body), &f.ctx()).unwrap())
            .collect();
        let horses = rows.iter().find(|g| g.name.text() == "Horses").unwrap();
        assert_eq!(horses.kind, kind::STRATEGIC);
        assert_eq!(horses.appearance_ratio, 160);
        assert_eq!(horses.prerequisite, 4);
        let iron = rows.iter().find(|g| g.name.text() == "Iron").unwrap();
        assert_eq!(iron.kind, kind::STRATEGIC);
        assert_eq!(iron.shield_bonus, 1);
        assert_eq!(iron.disappearance_probability, 800);
        assert_eq!(iron.prerequisite, 7); // Iron Working
        let wheat = rows.iter().find(|g| g.name.text() == "Wheat").unwrap();
        assert_eq!(wheat.kind, kind::BONUS);
        assert_eq!(wheat.food_bonus, 2);
    }
}
