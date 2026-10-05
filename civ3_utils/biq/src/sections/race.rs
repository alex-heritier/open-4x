//! `RACE` — playable civilizations (editor dialog **169** “Civilizations”;
//! help overview `HIDD_PROPPAGE_RACES` / `Civilizations Page` in the editor
//! help).
//!
//! Arm `0x594632`, worker `0x5961F0`, row reader `0x5E6300`, writer `0x5E5EE0`
//! (via `0x5979D0`), constructor `0x5E5C00`..`0x5E5DAD`. Disk layout is the
//! writer's sequential `fwrite` order.
//!
//! # Row sizes
//!
//! | era | distinct row lengths |
//! |---|---|
//! | 4.01 | 14 (2880–4224 B) |
//! | 11.18 | 91 |
//! | 12.06–12.08 | 68 (2708–4548 B) |
//!
//! Variable pieces: city-name count, great-leader count, `0`/`4`/`8` era-art
//! paths (260 bytes each; the file stores them in blocks of four, `0x410`
//! bytes), the optional PTW king-unit dword, and the optional Conquests block
//! (flavors, flavor revision, diplomacy text index, `scientific_leader_count`
//! and that many 32-byte names: `16 + 32·n` bytes, so the whole tail is
//! `92 + 32·n`; every shipped Conquests row has `n` between 1 and 10).
//!
//! # The tail (after the era-art block)
//!
//! Every tail field is a little-endian dword. `t` is the offset inside the
//! tail on disk, `mem` the offset inside the in-memory civilization object
//! (the loader's `0x928`/`0x92C` list capacities are not on disk, so `t` and
//! `mem` differ by `0x90C` up to `t = 0x18` and by `0x914` from there on).
//! **A** = executable (reader `0x5E6300`, constructor, consumers), **B** =
//! editor binding (page → row routine `0x4441xx..0x444AB3`, row → page
//! `0x442140..0x4421D0`, controls from dialog 169).
//!
//! | `t` | mem | field | evidence |
//! |---|---|---|---|
//! | `0x00` | `+0x90C` | [`culture_group`](Civilization::culture_group) | B combo 1568 |
//! | `0x04` | `+0x910` | [`leader_gender`](Civilization::leader_gender) | B radios 1586/1587 |
//! | `0x08` | `+0x914` | [`civilization_gender`](Civilization::civilization_gender) | B radios 1570–1572 |
//! | `0x0C` | `+0x918` | [`aggression`](Civilization::aggression) | B slider 1646 |
//! | `0x10` | `+0x91C` | [`civilization_index`](Civilization::civilization_index) | A getter `0x53A060`; B renumbered on add/delete/move |
//! | `0x14` | `+0x920` | [`shunned_government`](Civilization::shunned_government) | B combo 1612 |
//! | `0x18` | `+0x924` | [`favorite_government`](Civilization::favorite_government) | B combo 1611 |
//! | `0x1C` | `+0x930` | [`default_color`](Civilization::default_color) | B combo 1601; A colour allocator `0x5A16FD` |
//! | `0x20` | `+0x934` | [`unique_color`](Civilization::unique_color) | B combo 1844; A colour allocator `0x5A16C2`/`0x5A174B` |
//! | `0x24`–`0x30` | `+0x938`–`+0x944` | [`free_techs`](Civilization::free_techs) | B combos 1604–1607; A getter `0x53A0A0` |
//! | `0x34` | `+0x948` | [`traits`](Civilization::traits) | A `hasTrait` `0x53A080`; B checkboxes 1590–1595, 1630, 1631 |
//! | `0x38` | `+0x94C` | [`governor_settings`](Civilization::governor_settings) | B checkboxes 1614–1620; A copied to the player at `0x4BCF89` |
//! | `0x3C` | `+0x950` | [`build_never`](Civilization::build_never) | B checkboxes 1657–1670; A copied to the player at `0x4BCFB1` |
//! | `0x40` | `+0x954` | [`build_often`](Civilization::build_often) | B checkboxes 1640–1655, 1676; A copied to the player at `0x4BCFD9` |
//! | `0x44` | `+0x958` | [`plurality`](Civilization::plurality) | B radios 1573/1574 |
//! | `0x48` | `+0x95C` | [`king_unit`](Civilization::king_unit) | B combo 1600; A `use_civ_king_unit` `0x5D2E80`, `0x568662` |
//! | `0x4C` | `+0x960` | [`flavors`](CivilizationConquestsExtension::flavors) | B checkboxes 1621–1626, 1629; A AI consumers `0x438009`… |
//! | `0x50` | `+0x964` | [`flavor_revision`](CivilizationConquestsExtension::flavor_revision) | A reader fix-up `0x5E6892`..`0x5E68DB` |
//! | `0x54` | `+0x968` | [`diplomacy_text_index`](CivilizationConquestsExtension::diplomacy_text_index) | B edit 1975 (range −1..40); A consumer `0x515557` |
//! | `0x58` | `+0x96C` | scientific-leader count, then `count × Str<32>` | A reader `0x5E6805`, `0x5E6884` |
//!
//! Civ3 1.x rows end after `plurality` (72 bytes), PTW rows add the king unit
//! (76 bytes), Conquests rows add the rest. The reader takes each dword only if
//! it still fits, exactly like the game.
//!
//! Earlier revisions of this crate read the four free-tech dwords as "bonus"
//! slots plus governor/build masks, and everything from `t = 0x34` to the king
//! unit one dword off (the trait mask as a "unique unit", the governor mask as
//! a "unique building", …). The editor's page routine fixes the real layout.

use crate::io::{Ctx, Field, Reader, Record, Result, Str, Writer};

/// Conquests tail extension: the dwords after [`Civilization::king_unit`]
/// (reader `0x5E6781`..`0x5E6884`, writer `0x5E6224`..`0x5E62B1`). Rows that
/// are too short to hold all of it have no extension at all.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct CivilizationConquestsExtension {
    /// Flavor bit mask (tail `+0x4C`, mem `+0x960`): bit *n* is the civ's
    /// *Flavor (n+1)* check box, an index into the `FLAV` section. Only bits
    /// 0..=6 are meaningful; some scenario authors' rows carry the MSVC
    /// debug-fill pattern `0xCCCCCC00` in the upper bits (kept verbatim).
    pub flavors: u32,
    /// Revision of the traits/flavors split (tail `+0x50`, mem `+0x964`). The
    /// loader treats `0` as "old layout": bits 8..=14 of
    /// [`Civilization::traits`] are moved into [`flavors`](Self::flavors) and
    /// the field becomes `3` (`0x5E6892`..`0x5E68DB`). Every shipped Conquests
    /// row stores `2`.
    pub flavor_revision: i32,
    /// Diplomacy text set (tail `+0x54`, mem `+0x968`; **B** *Diplomacy Text
    /// Index* edit, range `-1..=40`). `-1` (constructor default) makes the AI
    /// use the civilization's own row index minus one (`0x515557`).
    pub diplomacy_text_index: i32,
    /// Scientific leader count dword on disk (**B** `Civilization_Scientific_Leaders_…`).
    pub scientific_leader_count: u32,
    /// `count`×`Str<32>` scientific leader names (**B**; fixed 32-byte slots).
    pub scientific_leaders: Vec<Str<32>>,
}

/// Bit ids of [`Civilization::traits`].
///
/// **A** `RACE.vtable[0]` (`0x53A080`) tests `traits >> id & 1`; **B** the
/// editor assembles the mask from eight check boxes in `0x444475..0x444540`
/// (Militaristic 1590 → bit 0, Commercial 1591 → 1, Expansionist 1592 → 2,
/// Scientific 1593 → 3, Religious 1594 → 4, Industrious 1595 → 5,
/// Agricultural 1630 → 6, Seafaring 1631 → 7). Every shipped playable civ has
/// exactly two bits set.
pub mod trait_id {
    pub const MILITARISTIC: i32 = 0;
    pub const COMMERCIAL: i32 = 1;
    pub const EXPANSIONIST: i32 = 2;
    pub const SCIENTIFIC: i32 = 3;
    pub const RELIGIOUS: i32 = 4;
    pub const INDUSTRIOUS: i32 = 5;
    pub const AGRICULTURAL: i32 = 6;
    pub const SEAFARING: i32 = 7;
}

/// Bits of [`Civilization::governor_settings`] (**B** check boxes of the
/// *Governor → Settings* group, bits assigned in `0x4446D5..0x44477C`; the
/// constructor default is `0x11`, *Manage Citizens* + *Manage Production*).
pub mod governor_setting {
    pub const MANAGE_CITIZENS: u32 = 1 << 0;
    pub const EMPHASIZE_FOOD: u32 = 1 << 1;
    pub const EMPHASIZE_SHIELDS: u32 = 1 << 2;
    pub const EMPHASIZE_TRADE: u32 = 1 << 3;
    pub const MANAGE_PRODUCTION: u32 = 1 << 4;
    pub const NO_WONDERS: u32 = 1 << 5;
    pub const NO_SMALL_WONDERS: u32 = 1 << 6;
}

/// Bits shared by [`Civilization::build_never`] and
/// [`Civilization::build_often`] (**B** the two 15-box groups *Build Never*
/// and *Build Often* of the *Governor* panel, `0x44477C..0x444A9B`). The
/// governor's city-production advisor consults them through the player
/// object (**A** `0x4BCFB1`/`0x4BCFD9` copy them to `player+0x38`/`+0x3C`).
pub mod build_preference {
    pub const OFFENSIVE_LAND_UNITS: u32 = 1 << 0;
    pub const DEFENSIVE_LAND_UNITS: u32 = 1 << 1;
    pub const ARTILLERY_LAND_UNITS: u32 = 1 << 2;
    pub const SETTLERS: u32 = 1 << 3;
    pub const WORKERS: u32 = 1 << 4;
    pub const NAVAL_UNITS: u32 = 1 << 5;
    pub const AIR_UNITS: u32 = 1 << 6;
    pub const GROWTH: u32 = 1 << 7;
    pub const PRODUCTION: u32 = 1 << 8;
    pub const HAPPINESS: u32 = 1 << 9;
    pub const SCIENCE: u32 = 1 << 10;
    pub const WEALTH: u32 = 1 << 11;
    pub const TRADE: u32 = 1 << 12;
    pub const EXPLORE: u32 = 1 << 13;
    pub const CULTURE: u32 = 1 << 14;
}

/// One civilization row.
#[derive(Clone, Debug, PartialEq)]
pub struct Civilization {
    /// City name list (**B** `Civilization_City_Names`): `[u32 count]` then
    /// `count`×[`Str<24>`].
    pub city_names: Vec<Str<24>>,
    /// Great leader names (**B** `Civilization_Great_Leaders`).
    pub great_leaders: Vec<Str<32>>,
    /// Leader name (**B** `Name_Civilization_Leader_`; 32 bytes).
    pub leader_name: Str<32>,
    /// Leader title (**B** `Title_Civilization_Leader_`; 24 bytes).
    pub title: Str<24>,
    /// Civilopedia key (control 1627; 32 bytes).
    pub civilopedia_entry: Str<32>,
    /// Adjective form (**B** `Adjective_Civilization_Description_`; 40 bytes; file order first).
    pub adjective: Str<40>,
    /// Singular civilization name (**B**; 40 bytes; e.g. Romans → `Rome`).
    pub civilization_name: Str<40>,
    /// Plural noun (**B** `Plurality_Civilization_Description_`; 40 bytes; e.g. `Romans`).
    pub noun: Str<40>,
    /// Era animation paths (**B** `Era_Civilization_Animations_`: forward/reverse
    /// pairs). Shipped rows store `0`, `4`, or `8`×`Str<260>` (`0`/`0x410`/`0x820`).
    pub era_art: Vec<Str<260>>,
    /// Culture group (**B** `Culture_Group_Civilization_Properties_`; tail `+0x00`).
    pub culture_group: i32,
    /// Leader gender, `0` male / `1` female (**B** `Gender_Civilization_Leader_`; tail `+0x04`).
    pub leader_gender: i32,
    /// Civilization gender, `0` masculine / `1` feminine / `2` neuter (**B**
    /// `Gender_Civilization_Description_`; tail `+0x08`).
    pub civilization_gender: i32,
    /// Aggression slider, `-2..=2` (**B** `Aggression_Level_Civilization_Personality_`;
    /// tail `+0x0C`).
    pub aggression: i32,
    /// This row's own index (tail `+0x10`, mem `+0x91C`). Equal to the row
    /// number in all 930 shipped rows, so it is `0` only for the Barbarians
    /// row, which the editor uses to recognise it (`0x444BB6`).
    pub civilization_index: i32,
    /// Shunned government (**B** `Shunned_Government_`; `GOVT` index, `-1` none; `+0x14`).
    pub shunned_government: i32,
    /// Favorite government (**B** `Favorite_Government_`; `+0x18`).
    pub favorite_government: i32,
    /// Default team color (**B** `Civilization_Team_Colors`; `ntpNN.pcx` index; `+0x1C`).
    pub default_color: i32,
    /// Unique/alternate team color (**B** + **C** `reverse-engineering/biq.md`; `+0x20`).
    pub unique_color: i32,
    /// Technologies the civilization starts with (`TECH` index, `-1` empty;
    /// tail `+0x24..+0x30`). Stock Conquests civs use two slots: Romans
    /// `[5, 2]` = Warrior Code + Alphabet, Greeks `[2, 0]` = Alphabet + Bronze
    /// Working.
    pub free_techs: [i32; 4],
    /// Trait bit mask (tail `+0x34`, mem `+0x948`; bit ids in [`trait_id`]).
    pub traits: u32,
    /// AI governor settings (tail `+0x38`; bits in [`governor_setting`]).
    pub governor_settings: u32,
    /// Things the governor never builds (tail `+0x3C`; bits in [`build_preference`]).
    pub build_never: u32,
    /// Things the governor builds often (tail `+0x40`; bits in [`build_preference`]).
    pub build_often: u32,
    /// Plurality of the noun, `0` singular / `1` plural (**B** radios 1573/1574; tail `+0x44`).
    pub plurality: i32,
    /// Civilization-specific king unit (`PRTO` index; tail `+0x48`; **B**
    /// *King Unit* combo 1600). `None` for Civ3 1.x rows, which end after
    /// [`plurality`](Self::plurality). Units with `use_civ_king_unit` are
    /// replaced by it (`0x5D2E80`); the Conquests rules put one *Caesar*,
    /// *Cleopatra*… per civ at `PRTO` 77–112.
    pub king_unit: Option<i32>,
    /// Conquests-only tail; requires [`king_unit`](Self::king_unit).
    pub conquests: Option<CivilizationConquestsExtension>,
    /// Bytes after the last modelled field (empty in every shipped file).
    pub extra: Vec<u8>,
}

impl Civilization {
    /// Singular display name (alias for [`Self::civilization_name`]).
    pub fn name(&self) -> &Str<40> {
        &self.civilization_name
    }

    /// Whether the civilization has trait `trait_index` (**A** `0x53A080`,
    /// ids in [`trait_id`]).
    pub fn has_trait(&self, trait_index: i32) -> bool {
        if !(0..32).contains(&trait_index) {
            return false;
        }
        (self.traits >> trait_index as u32) & 1 != 0
    }

    /// The traits as the game sees them after loading: for rows with no
    /// revision dword (all Civ3 1.x and PTW rows) or revision `0`, bits 8..=14
    /// belong to [`effective_flavors`](Self::effective_flavors) and are cleared
    /// (**A** `0x5E6892`..`0x5E68D1`).
    pub fn effective_traits(&self) -> u32 {
        if self.flavor_revision() == 0 {
            self.traits & !0x7F00
        } else {
            self.traits
        }
    }

    /// The flavor mask as the game sees it after loading (see
    /// [`effective_traits`](Self::effective_traits)).
    pub fn effective_flavors(&self) -> u32 {
        let stored = self.conquests.as_ref().map_or(0, |e| e.flavors);
        if self.flavor_revision() == 0 {
            stored | ((self.traits >> 8) & 0x7F)
        } else {
            stored
        }
    }

    /// Whether flavor `flavor_index` (a `FLAV` row, `0..=6`) is set after the
    /// loader's fix-up.
    pub fn has_flavor(&self, flavor_index: i32) -> bool {
        if !(0..32).contains(&flavor_index) {
            return false;
        }
        (self.effective_flavors() >> flavor_index as u32) & 1 != 0
    }

    /// Revision dword as the reader leaves it: `0` when the row is too short.
    fn flavor_revision(&self) -> i32 {
        self.conquests.as_ref().map_or(0, |e| e.flavor_revision)
    }
}

impl Default for Civilization {
    /// The in-memory constructor's state (`0x5E5C00`..`0x5E5DAD`), except that
    /// the version-dependent tail pieces ([`king_unit`](Self::king_unit),
    /// [`conquests`](Self::conquests)) are absent.
    fn default() -> Self {
        Self {
            city_names: Vec::new(),
            great_leaders: Vec::new(),
            leader_name: Str::zero(),
            title: Str::zero(),
            civilopedia_entry: Str::zero(),
            adjective: Str::zero(),
            civilization_name: Str::zero(),
            noun: Str::zero(),
            era_art: Vec::new(),
            culture_group: -1,
            leader_gender: 0,
            civilization_gender: 0,
            aggression: 0,
            civilization_index: 0,
            shunned_government: -1,
            favorite_government: -1,
            default_color: 0,
            unique_color: 0,
            free_techs: [-1; 4],
            traits: 0,
            governor_settings: governor_setting::MANAGE_CITIZENS
                | governor_setting::MANAGE_PRODUCTION,
            build_never: 0,
            build_often: 0,
            plurality: 0,
            king_unit: None,
            conquests: None,
            extra: Vec::new(),
        }
    }
}

fn read_counted_list<T: Field>(
    r: &mut Reader<'_>,
    tag: [u8; 4],
    what: &'static str,
) -> Result<Vec<T>> {
    match r.counted_list::<T>(tag, what)? {
        Some(v) => Ok(v),
        None => Ok(Vec::new()),
    }
}

fn read_era_art(r: &mut Reader<'_>) -> Vec<Str<260>> {
    let mut out = Vec::new();
    if r.remaining() >= 0x410 {
        for _ in 0..4 {
            out.push(r.str::<260>().unwrap_or_default());
        }
    }
    if r.remaining() >= 0x410 {
        for _ in 0..4 {
            out.push(r.str::<260>().unwrap_or_default());
        }
    }
    out
}

fn write_era_art(w: &mut Writer, art: &[Str<260>]) {
    if art.is_empty() {
        return;
    }
    let first = art.len().min(4);
    for p in &art[..first] {
        p.write(w);
    }
    if art.len() > 4 {
        for p in &art[4..art.len().min(8)] {
            p.write(w);
        }
    }
}

fn take_i32(r: &mut Reader<'_>) -> Option<i32> {
    if r.remaining() >= 4 { r.i32() } else { None }
}

fn take_u32(r: &mut Reader<'_>) -> Option<u32> {
    if r.remaining() >= 4 { r.u32() } else { None }
}

fn read_tail(r: &mut Reader<'_>, c: &mut Civilization) {
    let Some(v) = take_i32(r) else {
        return;
    };
    c.culture_group = v;
    c.leader_gender = take_i32(r).unwrap_or(0);
    c.civilization_gender = take_i32(r).unwrap_or(0);
    c.aggression = take_i32(r).unwrap_or(0);
    c.civilization_index = take_i32(r).unwrap_or(0);
    c.shunned_government = take_i32(r).unwrap_or(-1);
    c.favorite_government = take_i32(r).unwrap_or(-1);
    c.default_color = take_i32(r).unwrap_or(0);
    c.unique_color = take_i32(r).unwrap_or(0);

    if r.remaining() >= 16 {
        for slot in &mut c.free_techs {
            *slot = r.i32().unwrap_or(-1);
        }
    }

    c.traits = take_u32(r).unwrap_or(0);
    c.governor_settings = take_u32(r).unwrap_or(c.governor_settings);
    c.build_never = take_u32(r).unwrap_or(0);
    c.build_often = take_u32(r).unwrap_or(0);
    c.plurality = take_i32(r).unwrap_or(0);

    let Some(king) = take_i32(r) else {
        return;
    };
    c.king_unit = Some(king);

    if r.remaining() < 16 {
        return;
    }
    let flavors = r.u32().unwrap_or(0);
    let flavor_revision = r.i32().unwrap_or(0);
    let diplomacy_text_index = r.i32().unwrap_or(-1);
    let scientific_leader_count = r.u32().unwrap_or(0);
    let mut scientific_leaders = Vec::new();
    for _ in 0..scientific_leader_count {
        if r.remaining() < 32 {
            break;
        }
        scientific_leaders.push(r.str::<32>().unwrap_or_default());
    }
    c.conquests = Some(CivilizationConquestsExtension {
        flavors,
        flavor_revision,
        diplomacy_text_index,
        scientific_leader_count,
        scientific_leaders,
    });
}

fn write_tail(w: &mut Writer, c: &Civilization) {
    w.i32(c.culture_group);
    w.i32(c.leader_gender);
    w.i32(c.civilization_gender);
    w.i32(c.aggression);
    w.i32(c.civilization_index);
    w.i32(c.shunned_government);
    w.i32(c.favorite_government);
    w.i32(c.default_color);
    w.i32(c.unique_color);
    for tech in c.free_techs {
        w.i32(tech);
    }
    w.u32(c.traits);
    w.u32(c.governor_settings);
    w.u32(c.build_never);
    w.u32(c.build_often);
    w.i32(c.plurality);

    if let Some(king) = c.king_unit {
        w.i32(king);
        if let Some(ext) = &c.conquests {
            w.u32(ext.flavors);
            w.i32(ext.flavor_revision);
            w.i32(ext.diplomacy_text_index);
            w.u32(ext.scientific_leader_count);
            for name in &ext.scientific_leaders {
                name.write(w);
            }
        }
    }
}

impl Record for Civilization {
    const TAG: [u8; 4] = *b"RACE";

    fn read(r: &mut Reader<'_>, _ctx: &Ctx) -> Result<Self> {
        let city_names = read_counted_list::<Str<24>>(r, Self::TAG, "city_names")?;
        let great_leaders = read_counted_list::<Str<32>>(r, Self::TAG, "great_leaders")?;
        let mut c = Civilization {
            city_names,
            great_leaders,
            ..Civilization::default()
        };

        if r.remaining() >= 32 {
            c.leader_name = r.str().unwrap_or_default();
        }
        if r.remaining() >= 24 {
            c.title = r.str().unwrap_or_default();
        }
        if r.remaining() >= 32 {
            c.civilopedia_entry = r.str().unwrap_or_default();
        }
        if r.remaining() >= 40 {
            c.adjective = r.str().unwrap_or_default();
        }
        if r.remaining() >= 40 {
            c.civilization_name = r.str().unwrap_or_default();
        }
        if r.remaining() >= 40 {
            c.noun = r.str().unwrap_or_default();
        }

        c.era_art = read_era_art(r);
        read_tail(r, &mut c);
        c.extra = r.rest().to_vec();
        Ok(c)
    }

    fn write(&self, w: &mut Writer, _ctx: &Ctx) {
        w.counted_list(&self.city_names);
        w.counted_list(&self.great_leaders);
        self.leader_name.write(w);
        self.title.write(w);
        self.civilopedia_entry.write(w);
        self.adjective.write(w);
        self.civilization_name.write(w);
        self.noun.write(w);
        write_era_art(w, &self.era_art);
        write_tail(w, self);
        w.bytes(&self.extra);
    }

    fn extra(&self) -> &[u8] {
        &self.extra
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::Reader;

    fn conquests() -> Option<crate::corpus::CorpusFile> {
        crate::corpus::files()
            .into_iter()
            .find(|f| f.name().contains("conquests.biq"))
    }

    fn by_key(f: &crate::corpus::CorpusFile) -> std::collections::HashMap<String, Civilization> {
        let ctx = f.ctx();
        let sec = f.raw.section(b"RACE").expect("RACE");
        let mut out = std::collections::HashMap::new();
        for row in &sec.rows {
            let body = f.raw.row(row);
            let civ = Civilization::read(&mut Reader::new(body), &ctx).unwrap();
            out.insert(civ.civilopedia_entry.text().to_string(), civ);
        }
        out
    }

    #[test]
    fn corpus_roundtrip() {
        let st = crate::corpus::check_roundtrip::<Civilization>();
        assert_eq!(st.with_extra, 0, "{}", st.report());
    }

    #[test]
    fn romans_description_strings() {
        let Some(f) = conquests() else {
            return;
        };
        let map = by_key(&f);
        let romans = map.get("RACE_ROMANS").unwrap();
        assert_eq!(romans.adjective.text(), "Roman");
        assert_eq!(romans.civilization_name.text(), "Rome");
        assert_eq!(romans.noun.text(), "Romans");
        assert_eq!(romans.leader_name.text(), "Caesar");
    }

    #[test]
    fn conquests_color_indices() {
        let Some(f) = conquests() else {
            return;
        };
        let map = by_key(&f);
        let want = [
            ("RACE_ROMANS", 1, 1),
            ("RACE_EGYPTIANS", 3, 3),
            ("RACE_CHINESE", 5, 15),
            ("RACE_AMERICAN", 5, 5),
            ("RACE_JAPANESE", 4, 11),
            ("RACE_Vikings", 8, 31),
        ];
        for (k, def, uniq) in want {
            let civ = map.get(k).unwrap();
            assert_eq!(civ.default_color, def, "{k} default ntp index");
            assert_eq!(civ.unique_color, uniq, "{k} unique ntp index");
        }
    }

    #[test]
    fn chinese_city_list_head() {
        let Some(f) = conquests() else {
            return;
        };
        let map = by_key(&f);
        let chinese = map.get("RACE_CHINESE").unwrap();
        assert_eq!(chinese.city_names.len(), 18);
        let head: Vec<_> = chinese
            .city_names
            .iter()
            .take(8)
            .map(|s| s.text().into_owned())
            .collect();
        assert_eq!(
            head,
            [
                "Beijing", "Shanghai", "Canton", "Nanking", "Tsingtao", "Xinjian", "Chengdu",
                "Hangchow",
            ]
        );
    }

    #[test]
    fn conquests_free_techs() {
        let Some(f) = conquests() else {
            return;
        };
        let map = by_key(&f);
        // Rows hold TECH indices; stock order starts Bronze Working, Masonry,
        // Alphabet, Pottery, The Wheel, Warrior Code, Ceremonial Burial.
        let want = [
            ("RACE_ROMANS", [5, 2, -1, -1]),
            ("RACE_EGYPTIANS", [6, 1, -1, -1]),
            ("RACE_GREEKS", [2, 0, -1, -1]),
            ("RACE_JAPANESE", [4, 6, -1, -1]),
            ("RACE_BARBARIANS", [-1, -1, -1, -1]),
        ];
        for (k, techs) in want {
            assert_eq!(map.get(k).unwrap().free_techs, techs, "{k}");
        }
        let romans = map.get("RACE_ROMANS").unwrap();
        let ext = romans.conquests.as_ref().unwrap();
        assert_eq!(ext.scientific_leader_count, 5);
        assert!(ext.scientific_leaders[0].text().contains("Leonardo"));
    }

    #[test]
    fn conquests_governments_and_aggression() {
        let Some(f) = conquests() else {
            return;
        };
        let map = by_key(&f);
        // Romans: shunned Despotism (3), favorite Monarchy (4) in stock GOVT order.
        let romans = map.get("RACE_ROMANS").unwrap();
        assert_eq!(
            (
                romans.shunned_government,
                romans.favorite_government,
                romans.aggression
            ),
            (3, 4, 1)
        );
        let french = map.get("RACE_FRENCH").unwrap();
        assert_eq!(french.aggression, -2);
        let vikings = map.get("RACE_Vikings").unwrap();
        assert_eq!(
            (vikings.shunned_government, vikings.favorite_government),
            (4, 2)
        );
    }

    #[test]
    fn every_playable_civ_has_two_traits() {
        let Some(f) = conquests() else {
            return;
        };
        let map = by_key(&f);
        assert_eq!(map.get("RACE_BARBARIANS").unwrap().traits, 0);
        for (key, civ) in &map {
            if key != "RACE_BARBARIANS" {
                assert_eq!(civ.traits.count_ones(), 2, "{key}: {:#x}", civ.traits);
                assert_eq!(civ.traits >> 8, 0, "{key}");
            }
        }
    }

    #[test]
    fn trait_masks_follow_the_civilopedia() {
        use trait_id::*;
        let Some(f) = conquests() else {
            return;
        };
        let map = by_key(&f);
        let want = [
            ("RACE_ROMANS", [MILITARISTIC, COMMERCIAL]),
            ("RACE_EGYPTIANS", [RELIGIOUS, INDUSTRIOUS]),
            ("RACE_GREEKS", [COMMERCIAL, SCIENTIFIC]),
            ("RACE_GERMANS", [MILITARISTIC, SCIENTIFIC]),
            ("RACE_CHINESE", [MILITARISTIC, INDUSTRIOUS]),
            ("RACE_AMERICAN", [EXPANSIONIST, INDUSTRIOUS]),
            ("RACE_ENGLISH", [COMMERCIAL, SEAFARING]),
            ("RACE_Vikings", [MILITARISTIC, SEAFARING]),
            ("RACE_AZTECS", [MILITARISTIC, AGRICULTURAL]),
        ];
        for (k, bits) in want {
            let civ = map.get(k).unwrap();
            for b in bits {
                assert!(civ.has_trait(b), "{k} lacks trait {b}");
            }
            assert_eq!(civ.traits, (1 << bits[0]) | (1 << bits[1]), "{k}");
        }
        // Spot-check the raw masks quoted in reverse-engineering/capture.md.
        for (k, mask) in [
            ("RACE_ROMANS", 3),
            ("RACE_EGYPTIANS", 48),
            ("RACE_Spanish", 144),
            ("RACE_Carthaginians", 160),
            ("RACE_DUTCH", 192),
        ] {
            assert_eq!(map.get(k).unwrap().traits, mask, "{k}");
        }
        assert!(!map.get("RACE_ROMANS").unwrap().has_trait(-1));
        assert!(!map.get("RACE_ROMANS").unwrap().has_trait(40));
    }

    #[test]
    fn governor_masks_in_conquests() {
        use build_preference::*;
        let Some(f) = conquests() else {
            return;
        };
        let map = by_key(&f);
        for (key, civ) in &map {
            assert_eq!(
                civ.governor_settings,
                governor_setting::MANAGE_CITIZENS | governor_setting::MANAGE_PRODUCTION,
                "{key}"
            );
            assert_eq!(civ.build_never, 0, "{key}");
        }
        let romans = map.get("RACE_ROMANS").unwrap();
        assert_eq!(
            romans.build_often,
            OFFENSIVE_LAND_UNITS | DEFENSIVE_LAND_UNITS | GROWTH | PRODUCTION
        );
        let egypt = map.get("RACE_EGYPTIANS").unwrap();
        assert_eq!(egypt.build_often, GROWTH | PRODUCTION | CULTURE);
    }

    #[test]
    fn kings_are_the_leader_units() {
        let Some(f) = conquests() else {
            return;
        };
        let biq = crate::Biq::from_raw(&f.raw).unwrap();
        let map = by_key(&f);
        for (key, king) in [
            ("RACE_ROMANS", "Caesar"),
            ("RACE_EGYPTIANS", "Cleopatra"),
            ("RACE_GREEKS", "Alexander"),
            ("RACE_GERMANS", "Bismarck"),
            ("RACE_JAPANESE", "Tokugawa"),
            ("RACE_DUTCH", "William of Orange"),
        ] {
            let index = map.get(key).unwrap().king_unit.unwrap();
            assert_eq!(
                biq.rules.unit_types[index as usize].name.text(),
                king,
                "{key}"
            );
        }
        // Every stock civ owns a distinct king unit; plurality is `1` ("Romans").
        for (key, civ) in &map {
            assert_eq!(civ.plurality, 1, "{key}");
        }
    }

    #[test]
    fn conquests_flavor_revision_and_diplomacy_text() {
        let Some(f) = conquests() else {
            return;
        };
        let map = by_key(&f);
        for (key, civ) in &map {
            let ext = civ.conquests.as_ref().unwrap();
            assert_eq!(ext.flavor_revision, 2, "{key}");
        }
        let ext = |k: &str| map.get(k).unwrap().conquests.as_ref().unwrap();
        assert_eq!(ext("RACE_ROMANS").diplomacy_text_index, -1);
        assert_eq!(ext("RACE_DUTCH").diplomacy_text_index, 28);
        assert_eq!(ext("RACE_PORTUGAL").diplomacy_text_index, 27);
    }

    #[test]
    fn civilization_index_is_the_row_number() {
        let Some(f) = conquests() else {
            return;
        };
        let ctx = f.ctx();
        let sec = f.raw.section(b"RACE").unwrap();
        for (i, row) in sec.rows.iter().enumerate() {
            let c = Civilization::read(&mut Reader::new(f.raw.row(row)), &ctx).unwrap();
            assert_eq!(c.civilization_index, i as i32);
        }
    }

    #[test]
    fn legacy_flavor_bits_move_out_of_the_trait_mask() {
        // A PTW-style row: no revision dword, flavor 1 and 3 hidden in bits 8 and 10.
        let civ = Civilization {
            traits: (1 << 8) | (1 << 10) | (1 << 7) | 3,
            ..Civilization::default()
        };
        assert_eq!(civ.effective_traits(), (1 << 7) | 3);
        assert_eq!(civ.effective_flavors(), 0b101);
        assert!(civ.has_flavor(0) && !civ.has_flavor(1) && civ.has_flavor(2));

        // A revision-2 row is taken as stored.
        let civ = Civilization {
            traits: 3,
            king_unit: Some(0),
            conquests: Some(CivilizationConquestsExtension {
                flavors: 0b10,
                flavor_revision: 2,
                ..Default::default()
            }),
            ..Civilization::default()
        };
        assert_eq!(civ.effective_traits(), 3);
        assert_eq!(civ.effective_flavors(), 0b10);
    }

    #[test]
    fn tail_roundtrips_for_every_generation() {
        let ctx = Ctx {
            version: crate::Version::new(12, 8),
        };
        let mut civ = Civilization {
            free_techs: [1, 2, 3, 4],
            traits: 0x81,
            build_often: 0x4001,
            ..Civilization::default()
        };
        for king in [None, Some(7)] {
            civ.king_unit = king;
            for ext in [
                None,
                Some(CivilizationConquestsExtension {
                    flavors: 5,
                    flavor_revision: 2,
                    diplomacy_text_index: 9,
                    scientific_leader_count: 1,
                    scientific_leaders: vec![Str::new("Newton")],
                }),
            ] {
                civ.conquests = if king.is_some() { ext } else { None };
                let mut w = Writer::new();
                civ.write(&mut w, &ctx);
                let back = Civilization::read(&mut Reader::new(&w.buf), &ctx).unwrap();
                assert_eq!(back, civ);
            }
        }
    }

    #[test]
    fn city_names_are_24_bytes() {
        let Some(f) = conquests() else {
            return;
        };
        let ctx = f.ctx();
        let sec = f.raw.section(b"RACE").expect("RACE");
        for row in &sec.rows {
            let body = f.raw.row(row);
            let civ = Civilization::read(&mut Reader::new(body), &ctx).unwrap();
            for name in &civ.city_names {
                assert!(name.0.iter().skip(13).all(|&b| b == 0) || name.text().len() > 12);
            }
        }
    }

    #[test]
    fn romans_eight_era_paths() {
        let Some(f) = conquests() else {
            return;
        };
        let map = by_key(&f);
        let romans = map.get("RACE_ROMANS").unwrap();
        assert_eq!(romans.era_art.len(), 8);
    }
}
