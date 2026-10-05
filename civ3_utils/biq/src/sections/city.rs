//! `CITY` — pre-placed cities on the scenario map.
//!
//! Arm `0x594E09`, constructor/reader `0x5E0870`, writer `0x5E0AB0` (via
//! `0x597790`). Editor *City* page (dialog **188**): load `0x4295F0`, save
//! `0x4293D0`. The file layout is the `fread` order; the in-memory row (stride
//! `0x48`) has two bytes of padding after the name.
//!
//! | body | mem | field |
//! |---|---|---|
//! | `0`, `1` | `+0`, `+1` | [`has_walls`](City::has_walls), [`has_palace`](City::has_palace) |
//! | `2..26` | `+2` | [`name`](City::name) |
//! | `26` | `+0x1C` | [`owner_type`](City::owner_type) (ctor default `2`) |
//! | `30` | `+0x20` | building count, then `count` `BLDG` indices ([`starting_buildings`](City::starting_buildings)) |
//! | after list | `+0x24` | [`culture`](City::culture) |
//! | | `+0x2C` | [`owner`](City::owner) |
//! | | `+0x30` | [`size`](City::size) |
//! | | `+0x34`, `+0x38` | [`map_x`](City::map_x), [`map_y`](City::map_y) |
//! | | `+0x3C` | [`city_level`](City::city_level) |
//! | | `+0x40` | [`border_level`](City::border_level) |
//! | | `+0x44` | [`use_auto_name`](City::use_auto_name) (file version 3.17 and later) |
//!
//! What the editor derives instead of asking the user (`0x4293D0`, **A/B**):
//!
//! * `has_palace` = any starting building whose `BLDG`
//!   [`improvement_flags`](crate::sections::bldg::Building::improvement_flags)
//!   has bit 0 ([*Center of Empire*](crate::sections::bldg::improvement_flags::CENTER_OF_EMPIRE);
//!   body `+0xE8`, mem `+0xEC`) set; `has_walls` = any starting building with a
//!   positive [`bombard_defense`](crate::sections::bldg::Building::bombard_defense)
//!   (body `+0x98`, mem `+0x9C`, e.g. *Walls* = 8). Both hold for every shipped
//!   city whose file carries its own `BLDG` rows (1 712 of 1 724; `Island Hop.bix`
//!   has no rules sections).
//! * `city_level` = `2` when `size >` [`RULE.city_max_size`], `1` when
//!   `size >` [`RULE.town_max_size`], else `0` (town / city / metropolis);
//!   `size` is clamped to `1..=255`.
//! * `culture` is clamped to `0..=1_000_000`; names hold 23 characters.
//!
//! # Game-start placement (`0x5D2B20`, **A**)
//!
//! The caller picks the owner (`owner_type 2`: the player running that `RACE`, via
//! `0x539D60`; otherwise the player at `LEAD index + 1`) and calls this routine with
//! the row. It reads only:
//!
//! * `+0x44` [`use_auto_name`](City::use_auto_name): non-zero takes the name from the
//!   civilization's city list (`0x565BD0`) instead of `+0x02`. The cities with a fixed
//!   name are created first (loop at `0x5D2619`), the auto-named ones after them.
//! * `+0x34`, `+0x38`: position; the city factory is `0x5663C0(x, y, -1, name, 0)`.
//! * `+0x30` [`size`](City::size): when above 1 the city grows by `size - 1`
//!   (`0x4B9F60`).
//! * `+0x24` [`culture`](City::culture): stored (negative values become 0) in the
//!   city's culture slot for its owner (`city + 0x140 + 4 * owner`) and added to that
//!   player's culture total (`0xA546D4` in the player block).
//! * `+0x20` / `+0x28` the building list: every entry that is not `-1` is added
//!   (`0x4ACF40`, or `0x4AFAB0` for a *Capitalization* building), except wonders,
//!   small wonders and the *Center of Empire* building. A third pass over the `BLDG`
//!   table (`0x5D27F8..0x5D2A11`) places those: it walks the city rows in random
//!   order twice (the first round prefers civilization-owned rows for wonders and
//!   small wonders, player-owned rows for the palace), takes a row that lists the
//!   building (`0x5E0CC0`) and adds it, unless that wonder exists already (`0x538FE0`)
//!   or the owner already has that small wonder. That is the "one wonder of each type,
//!   one small wonder and palace of each type per player" rule of the Civ3Edit 1.33
//!   release notes, and it is why a file may list the same wonder in several rows.
//!
//! `has_walls`, `has_palace`, `city_level` and `border_level` are not read by this
//! routine, and no code that indexes the row array (`[0x9C40B4]`) reads `+0x3C` or
//! `+0x40`. The game presumably recomputes all four from the buildings, size and culture
//! (**HYPOTHESIS**); they are file-format redundancy for the editor.
//!
//! [`RULE.city_max_size`]: crate::sections::rule::GeneralRules::city_max_size
//! [`RULE.town_max_size`]: crate::sections::rule::GeneralRules::town_max_size

use crate::Version;
use crate::io::{Ctx, Field, Reader, Record, Result, Str, Writer};
use crate::owner::Owner;

/// On-disk row is `66 + 4×starting_buildings.len()` in every corpus file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct City {
    /// Body `+0`. `1` when a starting building has a positive bombard-defence rating (derived by the editor, see module docs).
    pub has_walls: u8,
    /// Body `+1`. `1` when a starting building is the palace (derived by the editor).
    pub has_palace: u8,
    /// Body `+2..+25`. City name (≤ 23 characters); ignored by the game when [`use_auto_name`](City::use_auto_name) is set.
    pub name: Str<24>,
    /// Body `+26`. `0` none, `1` barbarian tribe, `2` civilization, `3` player; see [`crate::owner`]. The editor refuses barbarian cities.
    pub owner_type: i32,
    /// Starting `BLDG` rows (*Improvements* list); the count dword precedes the indices.
    pub starting_buildings: Vec<i32>,
    /// *Culture:* starting culture points (`0..=1_000_000`; `Roma` = `800`).
    pub culture: i32,
    /// `RACE` index (`owner_type 2`) or `LEAD` index (`owner_type 3`).
    pub owner: i32,
    /// *Size:* starting population, `1..=255` (`Roma` = `6`).
    pub size: i32,
    /// Map X (staggered grid: `x + y` is even).
    pub map_x: i32,
    /// Map Y.
    pub map_y: i32,
    /// Derived from `size`: `0` town, `1` city, `2` metropolis (see module docs).
    pub city_level: i32,
    /// Culture border band. **C** in the corpus `1` for culture `0..10`, `2` for `10..100`, `3` for `100..1000`, `4` from `1000`; the editor never writes it and the game never reads it (no consumer of `+0x40` in the placement code).
    pub border_level: i32,
    /// The *Auto* button: non-zero lets the game pick the name from the civilization's list (16 rows in the corpus).
    /// Added with file version `3.17` (Civ3Edit 1.46, **B**); older rows end at [`border_level`](City::border_level).
    pub use_auto_name: i32,
    /// Bytes after the last modelled field (empty in every shipped file).
    pub extra: Vec<u8>,
}

/// The file version that added [`City::use_auto_name`] (Civ3Edit 1.46, "Updated BIC file version to v3.17").
pub const AUTO_NAME_SINCE: Version = Version::new(3, 17);

impl City {
    /// The decoded owner, `None` for an unknown `owner_type`.
    pub fn owner(&self) -> Option<Owner> {
        Owner::from_raw(self.owner_type, self.owner)
    }

    /// The level the editor stores for a given `size` (`0x429524..0x4295B4`).
    pub fn level_for_size(size: i32, town_max_size: i32, city_max_size: i32) -> i32 {
        if size > city_max_size {
            2
        } else if size > town_max_size {
            1
        } else {
            0
        }
    }
}

impl Record for City {
    const TAG: [u8; 4] = *b"CITY";

    fn read(r: &mut Reader<'_>, ctx: &Ctx) -> Result<Self> {
        let mut rec = Self::default();
        if r.remaining() >= 1 {
            rec.has_walls = Field::read(r);
        }
        if r.remaining() >= 1 {
            rec.has_palace = Field::read(r);
        }
        if r.remaining() >= 24 {
            rec.name = Field::read(r);
        }
        if r.remaining() >= 4 {
            rec.owner_type = Field::read(r);
        }
        if let Some(list) = r.counted_list::<i32>(Self::TAG, "starting_buildings")? {
            rec.starting_buildings = list;
        }
        if r.remaining() >= 4 {
            rec.culture = Field::read(r);
        }
        if r.remaining() >= 4 {
            rec.owner = Field::read(r);
        }
        if r.remaining() >= 4 {
            rec.size = Field::read(r);
        }
        if r.remaining() >= 4 {
            rec.map_x = Field::read(r);
        }
        if r.remaining() >= 4 {
            rec.map_y = Field::read(r);
        }
        if r.remaining() >= 4 {
            rec.city_level = Field::read(r);
        }
        if r.remaining() >= 4 {
            rec.border_level = Field::read(r);
        }
        if ctx.version >= AUTO_NAME_SINCE && r.remaining() >= 4 {
            rec.use_auto_name = Field::read(r);
        }
        rec.extra = r.rest().to_vec();
        Ok(rec)
    }

    fn write(&self, w: &mut Writer, ctx: &Ctx) {
        Field::write(&self.has_walls, w);
        Field::write(&self.has_palace, w);
        Field::write(&self.name, w);
        Field::write(&self.owner_type, w);
        w.counted_list(&self.starting_buildings);
        Field::write(&self.culture, w);
        Field::write(&self.owner, w);
        Field::write(&self.size, w);
        Field::write(&self.map_x, w);
        Field::write(&self.map_y, w);
        Field::write(&self.city_level, w);
        Field::write(&self.border_level, w);
        if ctx.version >= AUTO_NAME_SINCE {
            Field::write(&self.use_auto_name, w);
        }
        w.bytes(&self.extra);
    }

    fn extra(&self) -> &[u8] {
        &self.extra
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus;
    use crate::io::Record;
    use crate::sections::bldg::Building;

    #[test]
    fn corpus_roundtrip() {
        let st = corpus::check_roundtrip::<City>();
        assert_eq!(st.with_extra, 0);
        assert_eq!(st.short, 0);
        for len in st.lengths {
            assert!(
                len >= 66 && (len - 66) % 4 == 0,
                "unexpected CITY row length {len}"
            );
        }
    }

    /// The *Auto* word came with file version 3.17 (Civ3Edit 1.46); older rows end at the
    /// border level and read back with the flag off.
    #[test]
    fn rows_before_3_17_have_no_auto_name_word() {
        let c = City {
            name: Str::new("Roma"),
            size: 3,
            starting_buildings: vec![1, 2],
            use_auto_name: 1,
            ..City::default()
        };
        for (version, len, auto) in [
            (Version::new(3, 16), 70, 0),
            (Version::new(3, 17), 74, 1),
            (Version::new(12, 8), 74, 1),
        ] {
            let ctx = Ctx { version };
            let mut w = Writer::new();
            c.write(&mut w, &ctx);
            assert_eq!(w.buf.len(), len, "{version}");
            let back = City::read(&mut Reader::new(&w.buf), &ctx).unwrap();
            assert_eq!(back.use_auto_name, auto, "{version}");
            assert_eq!(back.starting_buildings, [1, 2]);
            assert!(back.extra.is_empty());
        }
    }

    #[test]
    fn golden_roma() {
        let files = corpus::files();
        let Some(file) = files
            .iter()
            .find(|f| f.name().contains("Rise_of_Rome") && !f.name().contains("MP"))
        else {
            return;
        };
        let bldg: Vec<Building> = corpus::rows::<Building>(std::slice::from_ref(file))
            .into_iter()
            .map(|r| Building::read(&mut Reader::new(r.body), &file.ctx()).unwrap())
            .collect();
        let sec = file.raw.section(b"CITY").expect("CITY");
        let mut found = false;
        for row in &sec.rows {
            let body = file.raw.row(row);
            let mut r = Reader::new(body);
            let c = City::read(&mut r, &file.ctx()).expect("read");
            if c.name.text() == "Roma" {
                found = true;
                assert_eq!((c.has_walls, c.has_palace), (1, 1));
                // RACE row 1 = Rome.
                assert_eq!(c.owner(), Some(Owner::Civilization(1)));
                assert_eq!(c.size, 6);
                assert_eq!(c.culture, 800);
                assert_eq!(c.starting_buildings, vec![0, 1, 2, 4, 7]);
                let names: Vec<_> = c
                    .starting_buildings
                    .iter()
                    .map(|&i| bldg[i as usize].name.text().to_string())
                    .collect();
                assert_eq!(
                    names,
                    ["Palace", "Barracks", "Granary", "Marketplace", "Walls"]
                );
                assert_eq!((c.map_x, c.map_y), (47, 67));
                assert_eq!(c.border_level, 3);
                assert_eq!(c.use_auto_name, 0);
            }
        }
        assert!(found, "Roma not in Rise_of_Rome CITY rows");
    }

    /// `Intro2` binds its cities to `LEAD` rows (`owner_type 3`).
    #[test]
    fn intro_cities_use_lead_owner() {
        let files = corpus::files();
        let Some(file) = files.iter().find(|f| f.name().contains("Intro2")) else {
            return;
        };
        let sec = file.raw.section(b"CITY").expect("CITY");
        let mut cuzco = false;
        for row in &sec.rows {
            let body = file.raw.row(row);
            let c = City::read(&mut Reader::new(body), &file.ctx()).unwrap();
            if c.name.text().trim_start().starts_with("Cuzco") {
                cuzco = true;
                assert_eq!(c.owner(), Some(Owner::Player(1)));
            }
        }
        assert!(cuzco, "Cuzco city row expected in Intro2 scenario");
    }

    /// The editor's derived fields hold for every shipped city: the level
    /// follows the size thresholds of the same file's `RULE`, and walls/palace
    /// follow the starting buildings.
    #[test]
    fn derived_fields_match_editor_rules() {
        use crate::sections::rule::GeneralRules;
        for file in corpus::files() {
            let rules = corpus::rows::<GeneralRules>(std::slice::from_ref(&file));
            let bldg: Vec<Building> = corpus::rows::<Building>(std::slice::from_ref(&file))
                .into_iter()
                .map(|r| Building::read(&mut Reader::new(r.body), &file.ctx()).unwrap())
                .collect();
            let Some(rule) = rules.first() else { continue };
            let rule = GeneralRules::read(&mut Reader::new(rule.body), &file.ctx()).unwrap();
            for row in corpus::rows::<City>(std::slice::from_ref(&file)) {
                let c = City::read(&mut Reader::new(row.body), &file.ctx()).unwrap();
                assert!(c.owner().is_some());
                assert_ne!(c.owner_type, 1, "the editor refuses barbarian cities");
                assert_eq!(
                    c.city_level,
                    City::level_for_size(c.size, rule.town_max_size, rule.city_max_size),
                    "{} {}",
                    file.name(),
                    c.name.text()
                );
                if !bldg.is_empty() {
                    let has = |p: &dyn Fn(&Building) -> bool| {
                        c.starting_buildings
                            .iter()
                            .any(|&i| bldg.get(i as usize).is_some_and(p))
                    };
                    assert_eq!(c.has_palace != 0, has(&|b| b.is_center_of_empire()));
                    assert_eq!(c.has_walls != 0, has(&|b| b.bombard_defense > 0));
                }
            }
        }
    }
}
