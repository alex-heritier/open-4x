//! `TERR` - terrain types.
//!
//! Dispatcher arm `0x594A97`, section worker `0x596490`, row reader `0x5E9300`,
//! row writer `0x5E8EF0`, row constructor `0x5E8AF0`. The editor edits the rows
//! on the *Terrain* page (dialog 137; its apply routine is `0x4530D0`), the
//! game keeps them in a table of `0xF0`-byte objects (`[0x9C7328]`).
//!
//! Evidence classes used below: **A** the exe (reader, writer, constructor or a
//! consumer), **B** the editor (page control to row field), **C** the shipped
//! files, **D** a value pattern only.
//!
//! # Row layout
//!
//! ```text
//! [u32 goods_count][resource mask][name 32][civilopedia key 32][tail]
//! ```
//!
//! The mask is **exactly `(goods_count + 7) / 8` bytes** (`0x5E9366`: `add 7;
//! sar 3`), so it is 3 bytes for 17..=24 resources and 4 for 25..=32. (A
//! 4-byte minimum, which this format was first read with, splits the rows of
//! `2 Rise of Rome.biq` - 18 resources - one byte too early.) `goods_count`
//! equals the number of `GOOD` rows the file was saved with.
//!
//! The tail, in the order the reader takes it (offsets are tail-relative; the
//! `mem` column is the offset inside the game's object, which has a 4-byte
//! length slot and a mask pointer in front, so it differs from the row offset):
//!
//! | tail | mem | size | field | evidence |
//! |-----:|----:|-----:|---|---|
//! | `0x00` | `0x4C` | 4 | [`Terrain::irrigation_bonus`] | A+B |
//! | `0x04` | `0x50` | 4 | [`Terrain::mining_bonus`] | A+B |
//! | `0x08` | `0x54` | 4 | [`Terrain::road_bonus`] | A+B |
//! | `0x0C` | `0x58` | 4 | [`Terrain::defense_bonus`] | A+B |
//! | `0x10` | `0x5C` | 4 | [`Terrain::movement_cost`] | A+B |
//! | `0x14` | `0x64` | 4 | [`Terrain::food`] | A+B |
//! | `0x18` | `0x68` | 4 | [`Terrain::shields`] | A+B |
//! | `0x1C` | `0x6C` | 4 | [`Terrain::commerce`] | A+B |
//! | `0x20` | `0x70` | 4 | [`Terrain::worker_job`] | A+B+C |
//! | `0x24` | `0x74` | 4 | [`Terrain::pollution_effect`] | B+C |
//! | `0x28` | `0x78` | 8x1 | [`TerrainFlags`] | A+B |
//! | `0x30` | `0x80` | 4 | [`Terrain::reserved_0x80`] (always 3) | A |
//! | `0x34` | `0x84` | 1 | [`Terrain::has_landmark`] | A+B+C |
//! | `0x35` | `0x88` | 8x4 | [`Landmark`] yields and bonuses | A+B |
//! | `0x55` | `0xA8` | 32 | [`Landmark::name`] | A+B |
//! | `0x75` | `0xC8` | 32 | [`Landmark::civilopedia_entry`] | A+B |
//! | `0x95` | - | 4 | copy of the landmark mining bonus (not modelled, see below) | A+C |
//! | `0x99` | `0xE8` | 4 | [`Terrain::disease_flags`] | A+B |
//! | `0x9D` | `0xEC` | 4 | [`Terrain::disease_strength`] | A+B |
//!
//! A Conquests tail is therefore 161 bytes, a PTW tail 48 (the ten dwords and
//! the eight flag bytes) and a Civ3 1.x tail 42 (the dwords and the first two
//! flags). Which fields a row carries is a question of file version, see
//! [Version history](#version-history).
//!
//! ## Writer quirks
//!
//! * `+0x80` is forced to `3` by the constructor (`0x5E8BDA`) and again by the
//!   writer (`0x5E9127`); the reader loads it and nothing uses it.
//! * The dword at tail `0x95` is the writer re-emitting the **landmark mining
//!   bonus** (`+0x98`, through `mov edx,[ebx]` at `0x5E9268`); the reader reads
//!   it into a local and drops it. It equals the real field in all 406 shipped
//!   Conquests rows, so the model does not keep it and [`Terrain::write`]
//!   regenerates it.
//! * The high bits of `disease_flags` are uninitialised memory: the constructor
//!   only clears the low nibble (`and al, 0xF0` at `0x5E8BD8`), which is why
//!   most rows read `0xCCCCCCC0` and the rows created by a different path
//!   (`Sea`, `Ocean`) read `0`. They carry no meaning and are preserved so a
//!   file re-encodes unchanged.
//!
//! # Tile terrain and flags (evidence A+B)
//!
//! * The three **tile values** are the base yields. The **terraform bonuses**
//!   are what irrigation, mines and roads add (`0..=25` in the editor); movement
//!   cost is `0..=1000`, defense bonus `-100..=1000` percent.
//! * The eight flags are bytes `0`/`1` in the order *cities, colonies,
//!   impassable, impassable by wheeled units, airfields, forts, outposts, radar
//!   towers*. The game reads them at `0x44B791` (`+0x7A`), `0x44B7B0` (`+0x7B`)
//!   and `0x55F06A`..`0x55F3A3` (`+0x7C..+0x7F`). Stock Conquests: only
//!   Mountains, Jungle, Marsh and Volcano are impassable to wheeled units,
//!   nothing is impassable to everything.
//! * [`Terrain::pollution_effect`] is the terrain a tile degrades to when
//!   pollution stays on it: a `TERR` index, `-1` for none, or `14` for the
//!   editor's *Base Terrain Type* entry (the map's base terrain). Stock:
//!   Plains becomes Desert, Grassland becomes Plains, Forest and Jungle become
//!   the base terrain (**C**, editor help `Pollution Effect (Terrain
//!   Properties)`).
//! * [`Terrain::worker_job`] is a `TFRM` index, `-1` for none: the job a Worker
//!   performs on this terrain. Stock: Plains, Grassland, Tundra `Plant Forest`
//!   (5), Forest `Clear Forest` (6), Jungle and Marsh `Clear Wetlands` (7). The
//!   game tests `terrain.worker_job == job` at `0x4617EC` and `0x55F5FE`.
//!
//! # Landmarks (evidence A+B+C)
//!
//! A landmark is a map overlay that turns a tile into a named variant of its
//! terrain with its own yields, bonuses, movement cost and defense. Only
//! Desert, Plains, Grassland, Hills, Mountains, Forest and Sea may have one
//! (editor help `Landmark Terrain`): those are exactly the terrains for which
//! the game's default-rules initialiser (`0x5E8E30`, jump table at `0x5E8ED8`)
//! sets [`Terrain::has_landmark`]. That initialiser also fills the landmark
//! block from the main fields (food `+0x64` to `+0x88`, shields, commerce,
//! irrigation, mining, road, movement cost, defense) and names it after the
//! terrain; scenario designers then overwrite it. The game reads the block
//! through small getters (`0x5E8C70`..`0x5E8CE0`) from the tile accessors at
//! `0x5DBDxx`..`0x5DBFxx`: a tile asks `is landmark ? terrain.landmark_x :
//! terrain.x`. The editor never writes `has_landmark` (its apply routine only
//! copies the landmark fields when the byte is set).
//!
//! # Disease
//!
//! [`Terrain::disease_flags`] bit 2 is *Causes Disease* and bit 3 *Cured by
//! Sanitation* (getters `0x5E8D10`, `0x5E8D70`); [`Terrain::disease_strength`]
//! is the chance in percent (default 50). Bit 1 is set on Mountains, Marsh and
//! Volcano in the stock file; nothing in the game or editor reads it (**D**).
//!
//! # Version history
//!
//! The loader (`0x596490`) prints one line per fix-up for files older than the
//! version that added the data and fills the missing fields; the crate records
//! the same boundaries as the points where the writer starts emitting a field:
//!
//! | version | what the older file lacks | loader's fix-up |
//! |---|---|---|
//! | 3.09 | *Allow Cities* | cleared on Mountains, Coast, Sea, Ocean |
//! | 3.13 | *Allow Colonies* | cleared on Coast, Sea, Ocean |
//! | 11.10 | *Impassable*, *Impassable by Wheeled* | wheeled set on Mountains and Jungle (*Impassable* was not logged: HYPOTHESIS that it arrived with its neighbour) |
//! | 11.12 | airfields, forts, outposts, radar towers | cleared on Coast, Sea, Ocean |
//! | 12.00 | the *Marsh* and *Volcano* rows | both rows are synthesised and file rows 9.. move up by two |
//! | 12.02 | the landmark block | `0x5E8E30` on every row |
//! | 12.03 | disease | Marsh, Jungle and Flood Plain get strength 50, only Flood Plain *cured by Sanitation* |
//!
//! Fields a row is too short for keep their constructor defaults (`0x5E8AF0`:
//! all flags set except *Impassable* and *Impassable by Wheeled*, worker job and
//! pollution effect `-1`, strength 50); [`Terrain::default`] reproduces them.
//!
//! ## 12 or 14 rows
//!
//! Civ3 1.x and PTW files carry **12** rows (no Marsh, no Volcano), Conquests
//! files 14 (`0x5964CA` accepts nothing else). The terrain ids stored in the
//! tiles follow the same numbering as the section, so `Rules::terrains[id]`
//! works for either; use [`TerrainNumbering`] to compare ids across generations.

use crate::Version;
use crate::io::{Ctx, Error, Field, Reader, Record, Result, Str, Writer};

/// Version thresholds from the loader's fix-ups (see the module docs).
const CITIES_SINCE: Version = Version::new(3, 9);
const COLONIES_SINCE: Version = Version::new(3, 13);
const IMPASSABLE_SINCE: Version = Version::new(11, 10);
const AIRFIELD_AND_UP_SINCE: Version = Version::new(11, 12);
const LANDMARK_SINCE: Version = Version::new(12, 2);
const DISEASE_SINCE: Version = Version::new(12, 3);

/// `pollution_effect` value of the editor's *Base Terrain Type* entry.
pub const BASE_TERRAIN_TYPE: i32 = 14;

/// Bits of [`Terrain::disease_flags`] (getters `0x5E8D10`, `0x5E8D70`).
pub mod disease {
    /// *Causes Disease*.
    pub const CAUSES_DISEASE: u32 = 1 << 2;
    /// *Cured by Sanitation*.
    pub const CURED_BY_SANITATION: u32 = 1 << 3;
}

/// Bytes of the resource mask for `goods_count` resources: `(n + 7) / 8`.
pub const fn mask_len(goods_count: u32) -> usize {
    (goods_count as usize).div_ceil(8)
}

/// How a file numbers its terrains: the value in a tile's terrain nibbles and
/// the index into the `TERR` section.
///
/// Civ3 1.x and PTW files number twelve terrains; Conquests inserted Marsh and
/// Volcano, which moves the three water terrains up by two. The game keeps the
/// Conquests numbering in memory and converts older files while loading
/// (`Cell::fixup(1)`, `0x5EA460`..`0x5EA4B6`): ids `9`, `10`, `11` become
/// `11`, `12`, `13` in both terrain nibbles, ids below `9` are unchanged. The
/// converter is switched on by a scenario flag (`+0xBA0`) that is `1` when the
/// file has a 12-row `TERR` section (`0x596936`) and, if it has none, when its
/// version is below 12.03 (`0x594517`). All 92 shipped files that have a map
/// agree with this rule: none of the 71 legacy ones has an id above 11 and all
/// 21 others have 12 and 13.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TerrainNumbering {
    /// Civ3 1.x / PTW: Desert 0, Plains 1, Grassland 2, Tundra 3, Flood Plain 4,
    /// Hills 5, Mountains 6, Forest 7, Jungle 8, Coast 9, Sea 10, Ocean 11.
    Legacy,
    /// Conquests: as above with Marsh 9 and Volcano 10, then Coast 11, Sea 12,
    /// Ocean 13. This is the order the game uses in memory.
    #[default]
    Current,
}

impl TerrainNumbering {
    /// The numbering of a file with `terr_rows` `TERR` rows and format
    /// `version`.
    pub fn of_file(terr_rows: usize, version: Version) -> Self {
        let legacy = match terr_rows {
            0 => version < Version::new(12, 3),
            n => n == 12,
        };
        if legacy { Self::Legacy } else { Self::Current }
    }

    /// `id` in the in-memory (Conquests) numbering.
    pub const fn to_current(self, id: u8) -> u8 {
        match self {
            Self::Legacy if id >= 9 => id + 2,
            _ => id,
        }
    }

    /// True for Coast, Sea and Ocean.
    pub const fn is_water(self, id: u8) -> bool {
        matches!(self.to_current(id), 11..=13)
    }
}

/// The eight allow/impassable bytes (`0`/`1`), in file order.
///
/// Which of them a row carries depends on the file version (see the module
/// docs); the others keep the constructor defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainFlags {
    /// *Allow Cities* (`+0x78`, since 3.09).
    pub allow_cities: u8,
    /// *Allow Colonies* (`+0x79`, since 3.13).
    pub allow_colonies: u8,
    /// *Impassable* to every unit (`+0x7A`).
    pub impassable: u8,
    /// *Impassable by Wheeled Units* (`+0x7B`, since 11.10).
    pub impassable_wheeled: u8,
    /// *Allow Airfields* (`+0x7C`, since 11.12).
    pub allow_airfields: u8,
    /// *Allow Forts* (`+0x7D`).
    pub allow_forts: u8,
    /// *Allow Outposts* (`+0x7E`).
    pub allow_outposts: u8,
    /// *Allow Radar Towers* (`+0x7F`).
    pub allow_radar_towers: u8,
}

impl Default for TerrainFlags {
    /// The constructor's values (`0x5E8BA8`..`0x5E8BC5`): everything allowed,
    /// nothing impassable.
    fn default() -> Self {
        TerrainFlags {
            allow_cities: 1,
            allow_colonies: 1,
            impassable: 0,
            impassable_wheeled: 0,
            allow_airfields: 1,
            allow_forts: 1,
            allow_outposts: 1,
            allow_radar_towers: 1,
        }
    }
}

/// The landmark variant of a terrain: eight numbers and two names.
///
/// The numbers mirror the main fields in the same order the editor shows them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Landmark {
    /// Food (`+0x88`).
    pub food: i32,
    /// Shields (`+0x8C`).
    pub shields: i32,
    /// Commerce (`+0x90`).
    pub commerce: i32,
    /// Irrigation bonus (`+0x94`).
    pub irrigation_bonus: i32,
    /// Mining bonus (`+0x98`).
    pub mining_bonus: i32,
    /// Road bonus (`+0x9C`).
    pub road_bonus: i32,
    /// Movement cost (`+0xA0`).
    pub movement_cost: i32,
    /// Defense bonus in percent (`+0xA4`).
    pub defense_bonus: i32,
    /// Name shown for the landmark tile (`+0xA8`, `"LM Desert"`).
    pub name: Str<32>,
    /// Civilopedia key (`+0xC8`, `"TERR_Desert"`).
    pub civilopedia_entry: Str<32>,
}

/// One terrain type.
#[derive(Clone, Debug, PartialEq)]
pub struct Terrain {
    /// Number of `GOOD` rows the resource mask was sized for.
    pub goods_count: u32,
    /// Possible resources: bit `g` of the byte string is set when `GOOD` row `g`
    /// may be found here. Always [`mask_len`]`(goods_count)` bytes.
    pub resource_mask: Vec<u8>,
    /// Display name.
    pub name: Str<32>,
    /// Civilopedia key (`TERR_Desert`).
    pub civilopedia_entry: Str<32>,
    /// Terraform bonus of irrigation (`+0x4C`, editor `0..=25`).
    pub irrigation_bonus: i32,
    /// Terraform bonus of a mine (`+0x50`, `0..=25`).
    pub mining_bonus: i32,
    /// Terraform bonus of a road (`+0x54`, `0..=25`).
    pub road_bonus: i32,
    /// Defense bonus in percent (`+0x58`, `-100..=1000`).
    pub defense_bonus: i32,
    /// Movement cost (`+0x5C`, `0..=1000`).
    pub movement_cost: i32,
    /// Base food (`+0x64`, `0..=25`).
    pub food: i32,
    /// Base shields (`+0x68`, `0..=25`).
    pub shields: i32,
    /// Base commerce (`+0x6C`, `0..=25`).
    pub commerce: i32,
    /// `TFRM` index of the job a Worker performs here, `-1` for none (`+0x70`).
    pub worker_job: i32,
    /// Terrain this one degrades to under pollution: a `TERR` index, `-1`, or
    /// [`BASE_TERRAIN_TYPE`] (`+0x74`).
    pub pollution_effect: i32,
    /// Allow/impassable flags (`+0x78..+0x7F`).
    pub flags: TerrainFlags,
    /// Always `3`: forced by the constructor and the writer, never consumed.
    pub reserved_0x80: i32,
    /// `1` for the seven terrains that may have a landmark (`+0x84`).
    pub has_landmark: u8,
    /// The landmark block (valid when [`Self::has_landmark`] is set).
    pub landmark: Landmark,
    /// Disease bits and uninitialised fill (`+0xE8`), see [`disease`].
    pub disease_flags: u32,
    /// Disease strength in percent (`+0xEC`).
    pub disease_strength: i32,
    /// Bytes after the last known field (empty for every shipped file).
    pub extra: Vec<u8>,
}

impl Default for Terrain {
    /// The constructor's row (`0x5E8AF0`): all numbers zero except worker job
    /// and pollution effect `-1`, `reserved_0x80` 3 and strength 50.
    fn default() -> Self {
        Terrain {
            goods_count: 0,
            resource_mask: Vec::new(),
            name: Str::default(),
            civilopedia_entry: Str::default(),
            irrigation_bonus: 0,
            mining_bonus: 0,
            road_bonus: 0,
            defense_bonus: 0,
            movement_cost: 0,
            food: 0,
            shields: 0,
            commerce: 0,
            worker_job: -1,
            pollution_effect: -1,
            flags: TerrainFlags::default(),
            reserved_0x80: 3,
            has_landmark: 0,
            landmark: Landmark::default(),
            disease_flags: 0,
            disease_strength: 50,
            extra: Vec::new(),
        }
    }
}

impl Terrain {
    /// True if `GOOD` row `good_index` may be found on this terrain.
    pub fn resource_allowed(&self, good_index: u32) -> bool {
        self.resource_mask
            .get((good_index / 8) as usize)
            .is_some_and(|b| (b >> (good_index % 8)) & 1 == 1)
    }

    /// Allow or forbid `GOOD` row `good_index`, growing the mask (and
    /// [`Self::goods_count`]) when the index is past its end.
    pub fn set_resource_allowed(&mut self, good_index: u32, allowed: bool) {
        if good_index >= self.goods_count {
            self.goods_count = good_index + 1;
        }
        self.resource_mask
            .resize(mask_len(self.goods_count).max(self.resource_mask.len()), 0);
        let byte = &mut self.resource_mask[(good_index / 8) as usize];
        let bit = 1u8 << (good_index % 8);
        if allowed {
            *byte |= bit;
        } else {
            *byte &= !bit;
        }
    }

    /// *Causes Disease* (bit 2 of [`Self::disease_flags`]).
    pub fn causes_disease(&self) -> bool {
        self.disease_flags & disease::CAUSES_DISEASE != 0
    }

    /// *Cured by Sanitation* (bit 3 of [`Self::disease_flags`]).
    pub fn cured_by_sanitation(&self) -> bool {
        self.disease_flags & disease::CURED_BY_SANITATION != 0
    }

    /// Set or clear *Causes Disease*, leaving every other bit alone.
    pub fn set_causes_disease(&mut self, on: bool) {
        self.set_disease_bit(disease::CAUSES_DISEASE, on);
    }

    /// Set or clear *Cured by Sanitation*, leaving every other bit alone.
    pub fn set_cured_by_sanitation(&mut self, on: bool) {
        self.set_disease_bit(disease::CURED_BY_SANITATION, on);
    }

    fn set_disease_bit(&mut self, bit: u32, on: bool) {
        if on {
            self.disease_flags |= bit;
        } else {
            self.disease_flags &= !bit;
        }
    }
}

impl Record for Terrain {
    const TAG: [u8; 4] = *b"TERR";

    fn read(r: &mut Reader<'_>, ctx: &Ctx) -> Result<Self> {
        let mut t = Terrain::default();
        // The game's rule: take a field only if the whole of it is left, and
        // otherwise keep the constructor's value (see `Reader`). A group is also
        // taken only from a file of a version that has it; bytes beyond that stay in
        // `extra` and are written back unchanged.
        'fields: {
            macro_rules! since {
                ($version:expr) => {
                    if ctx.version < $version {
                        break 'fields;
                    }
                };
            }
            macro_rules! get {
                ($place:expr, $ty:ty) => {{
                    if r.remaining() < <$ty as Field>::SIZE {
                        break 'fields;
                    }
                    $place = <$ty as Field>::read(r);
                }};
            }
            get!(t.goods_count, u32);
            let Some(mask) = r.take(mask_len(t.goods_count)) else {
                // The count promises more mask than the row has. Padding the
                // mask to `goods_count` would let a corrupt count size a
                // buffer, so this is an error, like any list count that does
                // not fit (`Reader::counted_list`).
                return Err(Error::BadCount {
                    tag: Self::TAG,
                    what: "resource mask",
                    count: t.goods_count,
                });
            };
            t.resource_mask = mask.to_vec();
            get!(t.name, Str<32>);
            get!(t.civilopedia_entry, Str<32>);
            get!(t.irrigation_bonus, i32);
            get!(t.mining_bonus, i32);
            get!(t.road_bonus, i32);
            get!(t.defense_bonus, i32);
            get!(t.movement_cost, i32);
            get!(t.food, i32);
            get!(t.shields, i32);
            get!(t.commerce, i32);
            get!(t.worker_job, i32);
            get!(t.pollution_effect, i32);
            since!(CITIES_SINCE);
            get!(t.flags.allow_cities, u8);
            since!(COLONIES_SINCE);
            get!(t.flags.allow_colonies, u8);
            since!(IMPASSABLE_SINCE);
            get!(t.flags.impassable, u8);
            get!(t.flags.impassable_wheeled, u8);
            since!(AIRFIELD_AND_UP_SINCE);
            get!(t.flags.allow_airfields, u8);
            get!(t.flags.allow_forts, u8);
            get!(t.flags.allow_outposts, u8);
            get!(t.flags.allow_radar_towers, u8);
            since!(LANDMARK_SINCE);
            get!(t.reserved_0x80, i32);
            get!(t.has_landmark, u8);
            get!(t.landmark.food, i32);
            get!(t.landmark.shields, i32);
            get!(t.landmark.commerce, i32);
            get!(t.landmark.irrigation_bonus, i32);
            get!(t.landmark.mining_bonus, i32);
            get!(t.landmark.road_bonus, i32);
            get!(t.landmark.movement_cost, i32);
            get!(t.landmark.defense_bonus, i32);
            get!(t.landmark.name, Str<32>);
            get!(t.landmark.civilopedia_entry, Str<32>);
            // The writer's second copy of `+0x98`; the game drops it too.
            if !r.skip(4) {
                break 'fields;
            }
            since!(DISEASE_SINCE);
            get!(t.disease_flags, u32);
            get!(t.disease_strength, i32);
        }
        t.resource_mask
            .resize(mask_len(t.goods_count).max(t.resource_mask.len()), 0);
        t.extra = r.rest().to_vec();
        Ok(t)
    }

    fn write(&self, w: &mut Writer, ctx: &Ctx) {
        let v = ctx.version;
        w.u32(self.goods_count);
        let mut mask = self.resource_mask.clone();
        mask.resize(mask_len(self.goods_count), 0);
        w.bytes(&mask);
        Field::write(&self.name, w);
        Field::write(&self.civilopedia_entry, w);
        for n in [
            self.irrigation_bonus,
            self.mining_bonus,
            self.road_bonus,
            self.defense_bonus,
            self.movement_cost,
            self.food,
            self.shields,
            self.commerce,
            self.worker_job,
            self.pollution_effect,
        ] {
            w.i32(n);
        }
        let f = &self.flags;
        if v >= CITIES_SINCE {
            w.u8(f.allow_cities);
        }
        if v >= COLONIES_SINCE {
            w.u8(f.allow_colonies);
        }
        if v >= IMPASSABLE_SINCE {
            w.u8(f.impassable);
            w.u8(f.impassable_wheeled);
        }
        if v >= AIRFIELD_AND_UP_SINCE {
            w.u8(f.allow_airfields);
            w.u8(f.allow_forts);
            w.u8(f.allow_outposts);
            w.u8(f.allow_radar_towers);
        }
        if v >= LANDMARK_SINCE {
            let l = &self.landmark;
            w.i32(self.reserved_0x80);
            w.u8(self.has_landmark);
            for n in [
                l.food,
                l.shields,
                l.commerce,
                l.irrigation_bonus,
                l.mining_bonus,
                l.road_bonus,
                l.movement_cost,
                l.defense_bonus,
            ] {
                w.i32(n);
            }
            Field::write(&l.name, w);
            Field::write(&l.civilopedia_entry, w);
            w.i32(l.mining_bonus); // the writer's second copy of `+0x98`
        }
        if v >= DISEASE_SINCE {
            w.u32(self.disease_flags);
            w.i32(self.disease_strength);
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
    use crate::corpus::{self, files};
    use crate::{Biq, Version};

    fn le(b: &[u8], at: usize) -> i32 {
        i32::from_le_bytes(b[at..at + 4].try_into().unwrap())
    }

    fn conquests() -> Option<Biq> {
        let f = files()
            .into_iter()
            .find(|f| f.name().ends_with("conquests.biq"))?;
        Some(Biq::from_raw(&f.raw).unwrap())
    }

    fn by_name<'a>(b: &'a Biq, name: &str) -> &'a Terrain {
        b.rules
            .terrains
            .iter()
            .find(|t| t.name.text() == name)
            .unwrap_or_else(|| panic!("no terrain {name}"))
    }

    #[test]
    fn corpus_roundtrip() {
        let st = corpus::check_roundtrip::<Terrain>();
        assert_eq!(st.with_extra, 0, "{}", st.report());
    }

    #[test]
    fn mask_is_exactly_ceil_goods_over_eight() {
        let files = files();
        let mut short_masks = 0;
        for f in &files {
            let Some(sec) = f.raw.section(b"TERR") else {
                continue;
            };
            let goods = f.raw.section(b"GOOD").map_or(0, |s| s.rows.len());
            for row in &sec.rows {
                let body = f.raw.row(row);
                let t = Terrain::read(&mut Reader::new(body), &f.ctx()).unwrap();
                assert_eq!(t.goods_count as usize, goods, "{}", f.name());
                assert_eq!(t.resource_mask.len(), goods.div_ceil(8), "{}", f.name());
                if t.resource_mask.len() < 4 {
                    short_masks += 1;
                }
            }
        }
        if !files.is_empty() {
            // Rise of Rome has 18 resources: a 3-byte mask, the case a
            // "minimum four bytes" rule gets wrong.
            assert!(short_masks > 0);
        }
    }

    #[test]
    fn tail_length_is_42_48_or_161_by_generation() {
        for f in files() {
            let Some(sec) = f.raw.section(b"TERR") else {
                continue;
            };
            let want = match f.version.major {
                ..=10 => 42,
                11 => 48,
                _ => 161,
            };
            for row in &sec.rows {
                let body = f.raw.row(row);
                let head = 4 + mask_len(le(body, 0) as u32) + 64;
                assert_eq!(body.len() - head, want, "{} v{}", f.name(), f.version);
            }
        }
    }

    #[test]
    fn twelve_rows_before_conquests_fourteen_in_it() {
        for f in files() {
            let Some(sec) = f.raw.section(b"TERR") else {
                continue;
            };
            let want = if f.version.major >= 12 { 14 } else { 12 };
            assert_eq!(sec.rows.len(), want, "{}", f.name());
        }
    }

    #[test]
    fn desert_row_of_conquests_biq() {
        let Some(biq) = conquests() else { return };
        let d = by_name(&biq, "Desert");
        assert_eq!(d.civilopedia_entry.text(), "TERR_Desert");
        assert_eq!(d.goods_count, 26);
        assert_eq!(d.resource_mask.len(), 4);
        assert!(d.resource_allowed(4)); // Oil
        assert_eq!((d.food, d.shields, d.commerce), (0, 1, 0));
        assert_eq!(
            (d.irrigation_bonus, d.mining_bonus, d.road_bonus),
            (1, 1, 1)
        );
        assert_eq!((d.defense_bonus, d.movement_cost), (10, 1));
        assert_eq!((d.worker_job, d.pollution_effect), (-1, -1));
        assert_eq!(d.flags, TerrainFlags::default());
        assert_eq!(d.reserved_0x80, 3);
        assert_eq!(d.has_landmark, 1);
        let lm = &d.landmark;
        assert_eq!(lm.name.text(), "LM Desert");
        assert_eq!(lm.civilopedia_entry.text(), "TERR_Desert");
        assert_eq!((lm.food, lm.shields, lm.commerce), (0, 1, 0));
        assert_eq!(
            (lm.irrigation_bonus, lm.mining_bonus, lm.road_bonus),
            (1, 1, 1)
        );
        assert_eq!((lm.movement_cost, lm.defense_bonus), (1, 10));
        assert!(!d.causes_disease());
        assert_eq!(d.disease_strength, 50);
        assert_eq!(d.disease_flags, 0xCCCC_CCC0);
    }

    #[test]
    fn flags_follow_the_games_terrain_behaviour() {
        let Some(biq) = conquests() else { return };
        // Wheeled units cannot enter these four, nothing is impassable to all.
        let wheeled: Vec<_> = biq
            .rules
            .terrains
            .iter()
            .filter(|t| t.flags.impassable_wheeled == 1)
            .map(|t| t.name.text().into_owned())
            .collect();
        assert_eq!(wheeled, ["Mountains", "Jungle", "Marsh", "Volcano"]);
        assert!(biq.rules.terrains.iter().all(|t| t.flags.impassable == 0));
        let mountains = by_name(&biq, "Mountains");
        assert_eq!(mountains.flags.allow_cities, 0);
        assert_eq!(mountains.flags.allow_colonies, 1);
        assert_eq!(mountains.flags.allow_radar_towers, 1);
        for water in ["Coast", "Sea", "Ocean"] {
            let t = by_name(&biq, water);
            assert_eq!(t.flags.allow_cities, 0, "{water}");
            assert_eq!(t.flags.allow_airfields, 0, "{water}");
            assert_eq!(t.flags.allow_colonies, 0, "{water}");
        }
        let marsh = by_name(&biq, "Marsh");
        assert_eq!(
            (marsh.flags.allow_forts, marsh.flags.allow_outposts),
            (0, 1)
        );
    }

    #[test]
    fn landmarks_exist_for_exactly_the_seven_terrains_the_game_allows() {
        let Some(biq) = conquests() else { return };
        // Jump table at 0x5E8ED8 / editor help "Landmark Terrain".
        let with: Vec<_> = biq
            .rules
            .terrains
            .iter()
            .filter(|t| t.has_landmark == 1)
            .map(|t| t.name.text().into_owned())
            .collect();
        assert_eq!(
            with,
            [
                "Desert",
                "Plains",
                "Grassland",
                "Hills",
                "Mountains",
                "Forest",
                "Sea"
            ]
        );
        for t in &biq.rules.terrains {
            assert_eq!(
                t.landmark.name.text().starts_with("LM "),
                t.has_landmark == 1,
                "{}",
                t.name
            );
        }
    }

    #[test]
    fn the_extra_dword_repeats_the_landmark_mining_bonus() {
        let mut rows = 0;
        for f in files() {
            let Some(sec) = f.raw.section(b"TERR") else {
                continue;
            };
            for row in &sec.rows {
                let body = f.raw.row(row);
                let tail = &body[4 + mask_len(le(body, 0) as u32) + 64..];
                if tail.len() == 161 {
                    assert_eq!(le(tail, 0x95), le(tail, 0x35 + 4 * 4), "{}", f.name());
                    rows += 1;
                }
            }
        }
        if !files().is_empty() {
            assert_eq!(rows, 406);
        }
    }

    #[test]
    fn disease_is_jungle_and_flood_plain() {
        let Some(biq) = conquests() else { return };
        let sick: Vec<_> = biq
            .rules
            .terrains
            .iter()
            .filter(|t| t.causes_disease())
            .map(|t| t.name.text().into_owned())
            .collect();
        assert_eq!(sick, ["Flood Plain", "Jungle", "Marsh"]);
        assert!(by_name(&biq, "Flood Plain").cured_by_sanitation());
        assert!(!by_name(&biq, "Jungle").cured_by_sanitation());
        assert!(biq.rules.terrains.iter().all(|t| t.disease_strength == 50));
    }

    #[test]
    fn worker_jobs_and_pollution_index_other_sections() {
        let Some(biq) = conquests() else { return };
        let job = |t: &str| {
            let j = by_name(&biq, t).worker_job;
            (j >= 0).then(|| biq.rules.worker_jobs[j as usize].name.text().into_owned())
        };
        assert_eq!(job("Plains").as_deref(), Some("Plant Forest"));
        assert_eq!(job("Grassland").as_deref(), Some("Plant Forest"));
        assert_eq!(job("Tundra").as_deref(), Some("Plant Forest"));
        assert_eq!(job("Forest").as_deref(), Some("Clear Forest"));
        assert_eq!(job("Jungle").as_deref(), Some("Clear Wetlands"));
        assert_eq!(job("Marsh").as_deref(), Some("Clear Wetlands"));
        assert_eq!(job("Desert"), None);
        let degrades = |t: &str| {
            let p = by_name(&biq, t).pollution_effect;
            (0..biq.rules.terrains.len() as i32)
                .contains(&p)
                .then(|| biq.rules.terrains[p as usize].name.text().into_owned())
        };
        assert_eq!(degrades("Plains").as_deref(), Some("Desert"));
        assert_eq!(degrades("Grassland").as_deref(), Some("Plains"));
        assert_eq!(by_name(&biq, "Forest").pollution_effect, BASE_TERRAIN_TYPE);
        assert_eq!(by_name(&biq, "Jungle").pollution_effect, BASE_TERRAIN_TYPE);
        assert_eq!(degrades("Marsh").as_deref(), Some("Coast"));
    }

    #[test]
    fn old_rows_keep_the_constructor_defaults_for_missing_fields() {
        // civ3mod.bic (v4.01) rows stop after the first two flag bytes.
        let Some(f) = files().into_iter().find(|f| f.version.major < 11) else {
            return;
        };
        let Some(sec) = f.raw.section(b"TERR") else {
            return;
        };
        let t = Terrain::read(&mut Reader::new(f.raw.row(&sec.rows[0])), &f.ctx()).unwrap();
        let d = Terrain::default();
        assert_eq!(t.flags.allow_airfields, d.flags.allow_airfields);
        assert_eq!(t.reserved_0x80, 3);
        assert_eq!(t.disease_strength, 50);
        assert_eq!(t.has_landmark, 0);
        assert!(t.extra.is_empty());
    }

    #[test]
    fn the_version_decides_how_much_of_the_tail_is_written() {
        let t = Terrain {
            goods_count: 26,
            resource_mask: vec![0; 4],
            ..Terrain::default()
        };
        let len_at = |maj, min| {
            let mut w = Writer::new();
            t.write(
                &mut w,
                &Ctx {
                    version: Version::new(maj, min),
                },
            );
            w.buf.len() - (4 + 4 + 64)
        };
        assert_eq!(len_at(2, 5), 40);
        assert_eq!(len_at(3, 9), 41);
        assert_eq!(len_at(4, 1), 42);
        assert_eq!(len_at(11, 10), 44);
        assert_eq!(len_at(11, 18), 48);
        assert_eq!(len_at(12, 2), 153);
        assert_eq!(len_at(12, 8), 161);
    }

    #[test]
    fn default_row_is_the_constructors() {
        let d = Terrain::default();
        assert_eq!((d.worker_job, d.pollution_effect), (-1, -1));
        assert_eq!((d.reserved_0x80, d.disease_strength), (3, 50));
        assert_eq!(d.flags.allow_cities, 1);
        assert_eq!(d.flags.impassable, 0);
        assert_eq!(d.flags.impassable_wheeled, 0);
    }

    #[test]
    fn resource_mask_helpers() {
        let mut t = Terrain::default();
        t.set_resource_allowed(3, true);
        assert_eq!((t.goods_count, t.resource_mask.len()), (4, 1));
        t.set_resource_allowed(26, true);
        assert_eq!((t.goods_count, t.resource_mask.len()), (27, 4));
        assert!(t.resource_allowed(3) && t.resource_allowed(26));
        assert!(!t.resource_allowed(4) && !t.resource_allowed(500));
        t.set_resource_allowed(3, false);
        assert!(!t.resource_allowed(3));
        assert_eq!(t.resource_mask, [0, 0, 0, 0b100]);
    }

    #[test]
    fn disease_bits_are_set_without_touching_the_rest() {
        let mut t = Terrain {
            disease_flags: 0xCCCC_CCC0,
            ..Terrain::default()
        };
        t.set_causes_disease(true);
        assert_eq!(t.disease_flags, 0xCCCC_CCC4);
        t.set_cured_by_sanitation(true);
        assert_eq!(t.disease_flags, 0xCCCC_CCCC);
        t.set_causes_disease(false);
        t.set_cured_by_sanitation(false);
        assert_eq!(t.disease_flags, 0xCCCC_CCC0);
    }

    #[test]
    fn numbering_follows_the_scenario_flag_rule() {
        use TerrainNumbering::{Current, Legacy};
        assert_eq!(TerrainNumbering::of_file(12, Version::new(11, 18)), Legacy);
        assert_eq!(TerrainNumbering::of_file(12, Version::new(12, 8)), Legacy);
        assert_eq!(TerrainNumbering::of_file(14, Version::new(11, 18)), Current);
        assert_eq!(TerrainNumbering::of_file(0, Version::new(11, 18)), Legacy);
        assert_eq!(TerrainNumbering::of_file(0, Version::new(12, 2)), Legacy);
        assert_eq!(TerrainNumbering::of_file(0, Version::new(12, 6)), Current);
        assert_eq!(Legacy.to_current(8), 8);
        assert_eq!(Legacy.to_current(9), 11);
        assert_eq!(Legacy.to_current(11), 13);
        assert_eq!(Current.to_current(11), 11);
        assert!(Legacy.is_water(9) && Legacy.is_water(11));
        assert!(!Legacy.is_water(8));
        assert!(!Current.is_water(10) && Current.is_water(11));
    }
}
