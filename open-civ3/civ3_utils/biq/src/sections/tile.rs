//! `TILE` - one row per map cell in scenario/save streams that carry a map.
//!
//! Arm `0x594608`, worker `0x596CE0`, row reader `0x5EA1F0`, writer `0x5EA030`.
//! Rows appear in cell-index order, see [`index_from_coords`]. The editor
//! edits them in *Tile Properties* (dialog 187: terrain, owner, resource and
//! sixteen overlay check boxes) and the river tool.
//!
//! # Row layout
//!
//! The reader (`0x5EA1F0`) `fread`s straight into the game's `Cell` object,
//! each field only when enough bytes remain, so a body offset is the `Cell`
//! offset minus 4 (there is no leading length dword in the body):
//!
//! | body | Cell | size | field | evidence |
//! |------|------|------|-------|----------|
//! | `+0x00` | `+0x04` | 1 | [`Tile::river_connection_mask`] | A + C |
//! | `+0x01` | `+0x05` | 1 | [`Tile::owner`] | A + B |
//! | `+0x02` | `+0x08` | 4 | [`Tile::resource`] | A |
//! | `+0x06` | `+0x10` | 2 | [`Tile::terrain_image`], [`Tile::terrain_file`] | C |
//! | `+0x08` | `+0x12` | 2 | `unknown_0x08`, `unknown_0x09` (always 0) | D |
//! | `+0x0A` | `+0x14` | 4 | packed dword, see below | A |
//! | `+0x0E` | `+0x18` | 2 | [`Tile::barbarian_tribe_id`] | C |
//! | `+0x10` | `+0x1A` | 2 | [`Tile::city_id`] | A |
//! | `+0x12` | `+0x1C` | 2 | [`Tile::colony_id`] | A |
//! | `+0x14` | `+0x1E` | 2 | [`Tile::continent_id`] | A |
//! | `+0x16` | `+0x20` | 1 | [`Tile::water_depth_byte`] | A |
//! | `+0x17` | `+0x22` | 2 | [`Tile::victory_point_location_id`] | A + B |
//! | `+0x19` | `+0x24` | 4 | [`Tile::ruin_id`] | A |
//! | `+0x1D` | `+0x28` | 4 | [`Tile::flags_0x28`] = overlay plane | A |
//! | `+0x21` | `+0x2C` | 4 | [`Tile::terrain_class`] = terrain word | A |
//! | `+0x25` | `+0x30` | 4 | [`Tile::flags_0x30`] = feature plane | A |
//! | `+0x29` | `+0x34` | 4 | [`Tile::flags_0x34`] | A (meaning open) |
//! | `+0x2D` | - | 4 | `legacy_padding_0x2d` (never read by the game) | D |
//!
//! Row lengths seen: 22 (through `continent_id`), 23 (+ depth), 29 (+ victory
//! point and ruin), 45 (+ the 12-byte plane blob and `flags_0x34`; every
//! Conquests `.biq`), 49 (+ unread padding; `Intro3_New_Alliances`).
//!
//! # Two encodings of the same planes
//!
//! Civ3 1.x/PTW rows store overlays, terrain and features in the packed dword
//! at `+0x0A` (`Cell+0x14`, one byte each: [`Tile::packed_overlays`],
//! [`Tile::packed_terrain`], [`Tile::packed_features`], [`Tile::packed_high`]).
//! Conquests rows store them in the three expanded dwords at `+0x1D` and leave
//! the packed dword zero. After reading, `Cell::fixup` (vtable slot `0x14`,
//! `0x5EA410`) rebuilds the expanded planes from the packed dword whenever the
//! depth byte is below 4 (it is forced to 2 when the row is too short to carry
//! it) and then stores 6 in it:
//!
//! ```text
//! Cell+0x28 (overlay plane)  = packed & 0xE00000FF
//! Cell+0x2C (terrain word)   = packed & 0x0000FF00
//! Cell+0x30 (feature plane)  = packed & 0x5FFF0000      (0x5FFF8000 for old maps)
//! ```
//!
//! Use [`Tile::overlay_plane`], [`Tile::terrain_word`] and
//! [`Tile::feature_plane`] to get the planes the game works with, whichever
//! encoding the row used.
//!
//! # What the game does with a tile after reading it
//!
//! The worker (`0x596CE0..0x596DC1`) clears four fields of every cell right
//! after `0x5EA1F0` returns: city id (`vfunc 0xE4(-1)`), colony id
//! (`vfunc 0xF8(-1)`), the unsaved `Cell+0x0C` id (`vfunc 0x104(-1)`) and the
//! owner (`vfunc 0x108(0)`, `0x5EAD20`). The city and colony links are rebuilt
//! when the `CITY` and `CLNY` rows are placed, and borders from culture; the
//! values in the file are caches for the editor. Continents are renumbered by
//! `finalizeMap` (`vfunc 0x7C`).

use crate::io::{Ctx, Reader, Record, Result, Writer};
use crate::sections::terr::TerrainNumbering;

/// Direction numbering shared with combat (`combat.md` section 4.1).
///
/// `Cell+0x04` is an eight-direction adjacency set: bit `d` is set when the
/// tile is joined by a river to its neighbour in direction `d`. The combat
/// river bonus (`0x56CD80`) tests `(byte >> d) & 1` with `d` the direction from
/// the defender towards the attacker (**A**).
///
/// Evidence from the corpus (**C**, 143 263 land-to-land edges in 89 maps):
///
/// * All eight bits occur, in near-equal numbers per opposite pair, so the
///   relation is meant to be symmetric: if tile `T` has bit `d`, the neighbour
///   of `T` in direction `d` has bit `(d + 4) % 8`. 98.3% of edges between two
///   land tiles are mirrored.
/// * It is not an invariant. One-sided edges exist in most files (21 of 89
///   maps have more than 2%, 8 more than 10%; `Rome.bic` has 177 of 459). A
///   plausible cause is an editor that clears a tile's own bits when its river
///   is erased without clearing the neighbours' (**HYPOTHESIS**: the missing
///   tile usually has no bits at all). The game reads only the defender's own
///   byte, so for such an edge the river bonus depends on the attack
///   direction. Do not assume the mirror.
/// * No water tile carries bits in any shipped map, and a bit towards a water
///   neighbour (a river mouth, 145 edges) is never mirrored.
pub mod river_connection {
    /// Bit for the neighbour at `(0, -2)`.
    pub const N: u8 = 1 << 0;
    /// Bit for the neighbour at `(1, -1)`.
    pub const NE: u8 = 1 << 1;
    /// Bit for the neighbour at `(2, 0)`.
    pub const E: u8 = 1 << 2;
    /// Bit for the neighbour at `(1, 1)`.
    pub const SE: u8 = 1 << 3;
    /// Bit for the neighbour at `(0, 2)`.
    pub const S: u8 = 1 << 4;
    /// Bit for the neighbour at `(-1, 1)`.
    pub const SW: u8 = 1 << 5;
    /// Bit for the neighbour at `(-2, 0)`.
    pub const W: u8 = 1 << 6;
    /// Bit for the neighbour at `(-1, -1)`.
    pub const NW: u8 = 1 << 7;
    /// `(dx, dy)` of direction `d` (`N0 NE1 E2 SE3 S4 SW5 W6 NW7`).
    pub const DELTAS: [(i32, i32); 8] = [
        (0, -2),
        (1, -1),
        (2, 0),
        (1, 1),
        (0, 2),
        (-1, 1),
        (-2, 0),
        (-1, -1),
    ];
    /// The direction pointing back.
    pub const fn opposite(d: u32) -> u32 {
        (d + 4) % 8
    }
}

/// The overlay plane (`Cell+0x28`, [`Tile::flags_0x28`]); the low byte is the
/// packed `overlays` byte of older rows.
///
/// Every bit is read by a `Cell` accessor that `Cell::hasOverlay(kind)`
/// (vtable slot `0x110`, `0x5E9AD0`, jump table `0x5E9C40`) dispatches the
/// editor's overlay kinds to (**A**). Accessors read the plane through
/// `vfunc 0xA8(0)` (`0x5DC300`), which for player 0 returns `dword[Cell+0x28]`.
/// A civilization that cannot see the tile instead gets a remembered copy of
/// bits `0..=7` only (`byte[Cell+0xAE+civ]`).
pub mod overlay {
    /// Road (`0x5EA8F0`, reached through `vfunc 0x64`).
    pub const ROAD: u32 = 1 << 0;
    /// Railroad (`0x5EA8C0`, `vfunc 0x5C`).
    pub const RAILROAD: u32 = 1 << 1;
    /// Mine (`0x5EA810`, `vfunc 0x48`).
    pub const MINE: u32 = 1 << 2;
    /// Irrigation (`0x5EA7F0`, `vfunc 0x44`).
    pub const IRRIGATION: u32 = 1 << 3;
    /// Fortress (`0x5EA760`, `vfunc 0x34`).
    pub const FORTRESS: u32 = 1 << 4;
    /// Goody hut (`0x5EA7A0`, `vfunc 0x3C`).
    pub const GOODY_HUT: u32 = 1 << 5;
    /// Pollution (`0x5EA880`, `vfunc 0x50`).
    pub const POLLUTION: u32 = 1 << 6;
    /// Barbarian camp (`0x5EA630`, `vfunc 0x1C`).
    pub const BARBARIAN_CAMP: u32 = 1 << 7;
    /// Craters (`0x5EA8A0`, `vfunc 0x54`); no editor check box.
    pub const CRATERS: u32 = 1 << 8;
    /// Barricade (`0x5EA780`, `vfunc 0x38`).
    pub const BARRICADE: u32 = 1 << 28;
    /// Airfield (`0x5EA610`, `vfunc 0x18`); the tile also has a `CLNY` row of kind 1.
    pub const AIRFIELD: u32 = 1 << 29;
    /// Radar tower (`0x5EA9D0`, `vfunc 0x84`); `CLNY` kind 2.
    pub const RADAR_TOWER: u32 = 1 << 30;
    /// Outpost (`0x5EA830`, `vfunc 0x4C`); `CLNY` kind 3.
    pub const OUTPOST: u32 = 1 << 31;
    /// Bits that survive the packed-to-expanded conversion (`0x5EA410`).
    pub const PACKED_MASK: u32 = 0xE000_00FF;
    /// The three colony-class bits; a colony tile with none of them is a plain colony.
    pub const COLONY_KINDS: u32 = AIRFIELD | RADAR_TOWER | OUTPOST;
}

/// The feature plane (`Cell+0x30`, [`Tile::flags_0x30`]); `packed_features` is
/// bits `16..24` of it for older rows.
pub mod feature {
    /// Unnamed flag read by `Cell` slot `0x70` (`0x5EA940`). Present only in
    /// Conquests rows (**D**): spread over every terrain including sea.
    pub const UNKNOWN_15: u32 = 1 << 15;
    /// Bonus grassland. **B** (editor variant list string 1049 "Bonus Grassland /
    /// Snow-Capped Mountains / Pine Forest", "Randomize Bonus Grassland Tiles")
    /// and **C** (88% of the tiles carrying it are Grassland, which then has it
    /// on about 35% of its tiles). Read through `Cell` slot `0x6C` (`0x5EA930`).
    pub const BONUS_GRASSLAND: u32 = 1 << 16;
    /// A player start location lies here (`Cell` slot `0x80`, `0x5EA9C0`,
    /// **A**). Equal to the `SLOC` positions in every shipped file (**C**).
    pub const START_LOCATION: u32 = 1 << 19;
    /// Snow-capped mountains: terrain 6 and this bit (`Cell` slot `0x74`,
    /// `0x5EA950`, **A**).
    pub const SNOW_CAPPED: u32 = 1 << 20;
    /// Pine forest: terrain 7 and this bit (`Cell` slot `0x30`, `0x5EA730`, **A**).
    pub const PINE_FOREST: u32 = 1 << 21;
    /// River corner marks, one bit per tile corner (**A**: bits `24..=27` are
    /// OR-ed by the overlay renderer `0x5F56B3..0x5F5ECB`, and the editor's
    /// "River" overlay is `river_connection_mask != 0 || plane & mask != 0`,
    /// `0x5E9AE9`). Which bit is which corner is not established.
    pub const RIVER_CORNERS: u32 = 0x0F00_0000;
    /// Unnamed flag set by `Cell` slot `0x7C` and read by slot `0x78`
    /// (`0x5EA980`); rare (**D**).
    pub const UNKNOWN_29: u32 = 1 << 29;
    /// Bits that survive the packed-to-expanded conversion (`0x5EA410`).
    pub const PACKED_MASK: u32 = 0x5FFF_0000;
    /// The mask old (`mode == 0`) maps use: also keeps bit 15.
    pub const PACKED_MASK_OLD_MAPS: u32 = 0x5FFF_8000;
}

/// Terrain ids in the terrain word (`vfunc 0xC8` @ `0x5EAB30`:
/// `(word >> 12) & 0xF`), in the **Conquests numbering** the game uses in
/// memory. These are `TERR` row indices; the game's own code assumes the stock
/// order (`Cell` slots `0x30`/`0x74` compare with 7 and 6).
///
/// Civ3 1.x and PTW files number the water terrains 9, 10 and 11 instead of
/// 11, 12 and 13: pass ids through [`TerrainNumbering::to_current`] before
/// comparing them with the constants below.
pub mod terrain_id {
    /// Mountains (the same in both numberings).
    pub const MOUNTAINS: u8 = 6;
    /// Forest (the same in both numberings).
    pub const FOREST: u8 = 7;
    /// Coast.
    pub const COAST: u8 = 11;
    /// Sea.
    pub const SEA: u8 = 12;
    /// Ocean.
    pub const OCEAN: u8 = 13;
}

/// Flat cell index (`0x5DC1C0`, `NOTES.md` §4.2).
#[inline]
pub fn index_from_coords(x: i32, y: i32, width: i32) -> u32 {
    (((width >> 1) * y + (x >> 1)) & 0xffff) as u32
}

/// Inverse (`0x5EB7D0`).
#[inline]
pub fn coords_from_index(index: u32, width: i32) -> (i32, i32) {
    let half = (width >> 1).max(0) as u32;
    if half == 0 {
        return (0, 0);
    }
    let y = (index / half) as i32;
    let x = 2 * (index % half) as i32 + (y & 1);
    (x, y)
}

/// Number of `TILE` rows a `width` x `height` map has.
#[inline]
pub fn expected_row_count(width: i32, height: i32) -> u32 {
    ((width >> 1).max(0) * height.max(0)) as u32
}

/// One map cell row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tile {
    /// Body `+0x00` -> `Cell+0x04`. Eight-direction river adjacency, see [`river_connection`]. **A + C**
    pub river_connection_mask: u8,
    /// Body `+0x01` -> `Cell+0x05`. Civilization (player slot) that owns the
    /// tile; `0` = nobody. **A** (`vfunc 0x98`/`0x108`, `combat.md` 14.3), **B**
    /// (*Tile Properties* "Owner" with a "Change..." button; the paint tool
    /// `0x47B1D0` stores `civ index + 1`). The game zeroes it after reading.
    ///
    /// **C**: files saved by the editor hold `LEAD index + 1` (`1..=4` in
    /// `Intro2`, `Intro3`, `Island_Hop`); fifteen shipped Conquests/PTW files
    /// hold `33 + player id` (`34..=64`) for reasons unknown (**D**).
    pub owner: u8,
    /// Body `+0x02` -> `Cell+0x08`. `GOOD` row index, `-1` = none. **A** (`vfunc 0x9C`)
    pub resource: i32,
    /// Body `+0x06` -> `Cell+0x10` low byte. Terrain art: cell index in the sheet. **C**
    pub terrain_image: u8,
    /// Body `+0x07` -> `Cell+0x11`. Terrain art: sheet id (`0..=7`). **C**
    pub terrain_file: u8,
    /// Body `+0x08`. Always `0` (903 994 rows). **D**
    pub unknown_0x08: u8,
    /// Body `+0x09`. Always `0`. **D**
    pub unknown_0x09: u8,
    /// Body `+0x0A`: packed bits `0..8`, see [`overlay`]. **A**
    pub packed_overlays: u8,
    /// Body `+0x0B`: packed bits `8..16`; low nibble = secondary class
    /// (`vfunc 0xC4`), high nibble = terrain id (`vfunc 0xC8`). **A**
    pub packed_terrain: u8,
    /// Body `+0x0C`: packed bits `16..24`, see [`feature`]. **A**
    pub packed_features: u8,
    /// Body `+0x0D`: packed bits `24..32`. The low nibble is the river corner
    /// marks ([`feature::RIVER_CORNERS`] `>> 24`); the high nibble would map to
    /// barricade/airfield/radar/outpost but is `0` in every packed shipped row. **A + C**
    pub packed_high: u8,
    /// Body `+0x0E` -> `Cell+0x18`. Barbarian tribe at a camp, `-1` = none
    /// (`vfunc 0xB0`, setter `0xDC`). **C**: positive on 3 137 rows only.
    pub barbarian_tribe_id: i16,
    /// Body `+0x10` -> `Cell+0x1A`. `CITY` row index, `-1` = none. Cleared by
    /// the loader and re-linked from `CITY`. **A**
    pub city_id: i16,
    /// Body `+0x12` -> `Cell+0x1C`. `CLNY` row index, `-1` = none. Cleared by
    /// the loader and re-linked from `CLNY`. **A**
    pub colony_id: i16,
    /// Body `+0x14` -> `Cell+0x1E`. `CONT` row index, `-1` = none (`vfunc 0xB8`). **A + C**
    pub continent_id: i16,
    /// Body `+0x16` -> `Cell+0x20`. Water grading; slots `0x24`/`0x28`/`0x2C`
    /// accept `2..=4` and the loader finally stores `6`. Conquests rows hold 6;
    /// packed rows hold arbitrary leftovers (`0`, `255`, ...). **A**
    pub water_depth_byte: Option<u8>,
    /// Body `+0x17` -> `Cell+0x22`. Victory point location marker: `0` when
    /// the tile is one, `-1` otherwise (`vfunc 0xC0`; the editor stores `0` or `-1`
    /// from its check box, `0x457EE0`/`0x457EEC`). **A + B**
    pub victory_point_location_id: Option<i16>,
    /// Body `+0x19` -> `Cell+0x24`. Non-zero = ruins (`vfunc 0x90`, setter `0xF0`);
    /// placing a city or colony clears it. Zero in every shipped file. **A**
    pub ruin_id: Option<i32>,
    /// Body `+0x1D` -> `Cell+0x28`. The overlay plane, see [`overlay`]. **A**
    pub flags_0x28: Option<i32>,
    /// Body `+0x21` -> `Cell+0x2C`. The terrain word: bits `8..12` secondary
    /// class (`vfunc 0xC4`), `12..16` terrain id (`vfunc 0xC8`). **A**
    pub terrain_class: Option<i32>,
    /// Body `+0x25` -> `Cell+0x30`. The feature plane, see [`feature`]. **A**
    pub flags_0x30: Option<i32>,
    /// Body `+0x29` -> `Cell+0x34` (`vfunc 0xFC`/`0x100`). Zeroed by the loader
    /// when the depth byte is below 5; always `0` in the corpus. **A**, meaning open.
    pub flags_0x34: Option<i32>,
    /// Body `+0x2D` (49-byte rows). Never read by `0x5EA1F0`; always zero. **D**
    pub legacy_padding_0x2d: Option<i32>,
    /// Bytes after the last modelled field (empty in every shipped file).
    pub extra: Vec<u8>,
}

impl Default for Tile {
    fn default() -> Self {
        Tile {
            river_connection_mask: 0,
            owner: 0,
            resource: -1,
            terrain_image: 0,
            terrain_file: 0,
            unknown_0x08: 0,
            unknown_0x09: 0,
            packed_overlays: 0,
            packed_terrain: 0,
            packed_features: 0,
            packed_high: 0,
            barbarian_tribe_id: -1,
            city_id: -1,
            colony_id: -1,
            continent_id: -1,
            water_depth_byte: None,
            victory_point_location_id: None,
            ruin_id: None,
            flags_0x28: None,
            terrain_class: None,
            flags_0x30: None,
            flags_0x34: None,
            legacy_padding_0x2d: None,
            extra: Vec::new(),
        }
    }
}

impl Tile {
    /// The packed dword (`Cell+0x14`, `+0x0A..+0x0D` on disk).
    #[inline]
    pub fn packed(&self) -> u32 {
        u32::from_le_bytes([
            self.packed_overlays,
            self.packed_terrain,
            self.packed_features,
            self.packed_high,
        ])
    }

    /// `true` when the game would use the expanded dwords as stored: the row
    /// carries them and the depth byte is at least 4 (`Cell::fixup`, `0x5EA410`).
    #[inline]
    pub fn uses_expanded_planes(&self) -> bool {
        self.flags_0x28.is_some() && self.water_depth_byte.is_some_and(|d| d >= 4)
    }

    /// `Cell+0x28` as the game holds it after loading.
    #[inline]
    pub fn overlay_plane(&self) -> u32 {
        match self.flags_0x28 {
            Some(v) if self.uses_expanded_planes() => v as u32,
            _ => self.packed() & overlay::PACKED_MASK,
        }
    }

    /// `Cell+0x2C` as the game holds it after loading.
    #[inline]
    pub fn terrain_word(&self) -> u32 {
        match self.terrain_class {
            Some(v) if self.uses_expanded_planes() => v as u32,
            _ => self.packed() & 0xFF00,
        }
    }

    /// `Cell+0x30` as the game holds it after loading.
    #[inline]
    pub fn feature_plane(&self) -> u32 {
        match self.flags_0x30 {
            Some(v) if self.uses_expanded_planes() => v as u32,
            _ => self.packed() & feature::PACKED_MASK,
        }
    }

    /// Terrain id (`vfunc 0xC8`): the `TERR` row index **in the numbering of
    /// the file the tile came from** ([`TerrainNumbering`]; the game converts
    /// older files to the Conquests numbering after loading).
    #[inline]
    pub fn terrain_id(&self) -> u8 {
        ((self.terrain_word() >> 12) & 0xF) as u8
    }

    /// Secondary class nibble (`vfunc 0xC4`): for land the terrain the tile
    /// was painted over (Hills 5 over Grassland 2 reads `0x52`), for water the
    /// water class (same numbering as [`Self::terrain_id`]).
    #[inline]
    pub fn terrain_sub_class(&self) -> u8 {
        ((self.terrain_word() >> 8) & 0xF) as u8
    }

    /// `vfunc 0x8C` @ `0x5EAA30`: Coast, Sea or Ocean.
    #[inline]
    pub fn is_water(&self, numbering: TerrainNumbering) -> bool {
        numbering.is_water(self.terrain_id())
    }

    /// All of the given [`overlay`] bits are set.
    #[inline]
    pub fn has_overlay(&self, bits: u32) -> bool {
        self.overlay_plane() & bits == bits
    }

    /// All of the given [`feature`] bits are set.
    #[inline]
    pub fn has_feature(&self, bits: u32) -> bool {
        self.feature_plane() & bits == bits
    }

    /// The editor's "River" overlay (`Cell::hasOverlay(0)`, `0x5E9AE9`).
    #[inline]
    pub fn has_river(&self) -> bool {
        self.river_connection_mask != 0 || self.feature_plane() & feature::RIVER_CORNERS != 0
    }

    /// Is there a river towards the neighbour in direction `d` (`0..8`)?
    #[inline]
    pub fn river_towards(&self, d: u32) -> bool {
        d < 8 && (self.river_connection_mask >> d) & 1 != 0
    }

    /// Plain colony, airfield, radar tower or outpost lives here (`vfunc 0x68`, `0x5EA910`).
    #[inline]
    pub fn has_colony(&self) -> bool {
        self.colony_id != -1
    }

    /// The `GOOD` row of the resource on this tile.
    #[inline]
    pub fn resource_index(&self) -> Option<u32> {
        if self.resource < 0 {
            None
        } else {
            Some(self.resource as u32)
        }
    }
}

impl Record for Tile {
    const TAG: [u8; 4] = *b"TILE";

    fn read(r: &mut Reader<'_>, _ctx: &Ctx) -> Result<Self> {
        let body_len = r.remaining();
        let mut t = Tile::default();
        if body_len < 1 {
            return Ok(t);
        }
        t.river_connection_mask = r.u8().unwrap_or(0);
        if r.remaining() >= 1 {
            t.owner = r.u8().unwrap_or(0);
        }
        if r.remaining() >= 4 {
            t.resource = r.i32().unwrap_or(-1);
        }
        if r.remaining() >= 4 {
            let b = r.array::<4>().unwrap_or([0; 4]);
            t.terrain_image = b[0];
            t.terrain_file = b[1];
            t.unknown_0x08 = b[2];
            t.unknown_0x09 = b[3];
        }
        if r.remaining() >= 4 {
            let b = r.array::<4>().unwrap_or([0; 4]);
            t.packed_overlays = b[0];
            t.packed_terrain = b[1];
            t.packed_features = b[2];
            t.packed_high = b[3];
        }
        if r.remaining() >= 2 {
            t.barbarian_tribe_id = r.i16().unwrap_or(-1);
        }
        if r.remaining() >= 2 {
            t.city_id = r.i16().unwrap_or(-1);
        }
        if r.remaining() >= 2 {
            t.colony_id = r.i16().unwrap_or(-1);
        }
        if r.remaining() >= 2 {
            t.continent_id = r.i16().unwrap_or(-1);
        }
        if r.remaining() >= 1 {
            t.water_depth_byte = Some(r.u8().unwrap_or(0));
        }
        if r.remaining() >= 2 {
            t.victory_point_location_id = Some(r.i16().unwrap_or(-1));
        }
        if r.remaining() >= 4 {
            t.ruin_id = Some(r.i32().unwrap_or(0));
        }
        if r.remaining() >= 4 {
            t.flags_0x28 = Some(r.i32().unwrap_or(0));
        }
        if r.remaining() >= 4 {
            t.terrain_class = Some(r.i32().unwrap_or(0));
        }
        if r.remaining() >= 4 {
            t.flags_0x30 = Some(r.i32().unwrap_or(0));
        }
        if r.remaining() >= 4 {
            t.flags_0x34 = Some(r.i32().unwrap_or(0));
        }
        if body_len >= 49 && r.remaining() >= 4 {
            t.legacy_padding_0x2d = Some(r.i32().unwrap_or(0));
        }
        t.extra = r.rest().to_vec();
        Ok(t)
    }

    fn write(&self, w: &mut Writer, _ctx: &Ctx) {
        w.u8(self.river_connection_mask);
        w.u8(self.owner);
        w.i32(self.resource);
        w.u8(self.terrain_image);
        w.u8(self.terrain_file);
        w.u8(self.unknown_0x08);
        w.u8(self.unknown_0x09);
        w.u8(self.packed_overlays);
        w.u8(self.packed_terrain);
        w.u8(self.packed_features);
        w.u8(self.packed_high);
        w.i16(self.barbarian_tribe_id);
        w.i16(self.city_id);
        w.i16(self.colony_id);
        w.i16(self.continent_id);
        if let Some(v) = self.water_depth_byte {
            w.u8(v);
        }
        if let Some(v) = self.victory_point_location_id {
            w.i16(v);
        }
        if let Some(v) = self.ruin_id {
            w.i32(v);
        }
        if let Some(v) = self.flags_0x28 {
            w.i32(v);
        }
        if let Some(v) = self.terrain_class {
            w.i32(v);
        }
        if let Some(v) = self.flags_0x30 {
            w.i32(v);
        }
        if let Some(v) = self.flags_0x34 {
            w.i32(v);
        }
        if let Some(v) = self.legacy_padding_0x2d {
            w.i32(v);
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
    use crate::sections::city::City;
    use crate::sections::wmap::WorldMap;
    use crate::{Biq, MapView};

    /// Every file that has a map, with its parsed model.
    fn maps() -> Vec<(String, Biq)> {
        corpus::files()
            .into_iter()
            .filter_map(|f| {
                let biq = Biq::from_raw(&f.raw).ok()?;
                biq.map_view()?;
                Some((f.name(), biq))
            })
            .collect()
    }

    fn view(biq: &Biq) -> MapView<'_> {
        biq.map_view().unwrap()
    }

    #[test]
    fn corpus_roundtrip() {
        let st = corpus::check_roundtrip::<Tile>();
        assert_eq!(st.with_extra, 0, "{}", st.report());
        assert_eq!(st.short, 0);
        let mut expect = vec![22, 23, 29, 45, 49];
        expect.retain(|l| st.lengths.contains(l));
        assert_eq!(st.lengths, expect, "{:?}", st.lengths);
    }

    #[test]
    fn coordinate_helpers_roundtrip() {
        let w = 80;
        for y in 0..10 {
            for x in 0..w {
                if (x + y) & 1 != 0 {
                    continue;
                }
                let i = index_from_coords(x, y, w);
                let (rx, ry) = coords_from_index(i, w);
                assert_eq!((rx, ry), (x, y), "index {i}");
            }
        }
        assert_eq!(expected_row_count(80, 80), 3200);
    }

    #[test]
    fn packed_rows_derive_the_expanded_planes() {
        // A Civ3 1.x row: road+mine, ocean terrain (11 in the legacy
        // numbering), start + bonus grassland bits and two river corner marks
        // in the packed dword.
        let t = Tile {
            packed_overlays: 0b101,
            packed_terrain: 0xBB,
            packed_features: 0b0000_1001,
            packed_high: 0b0000_0101,
            water_depth_byte: Some(0),
            ..Tile::default()
        };
        assert!(!t.uses_expanded_planes());
        assert_eq!(t.overlay_plane(), overlay::ROAD | overlay::MINE);
        assert_eq!(t.terrain_word(), 0xBB00);
        assert_eq!(t.terrain_id(), 11);
        assert_eq!(
            TerrainNumbering::Legacy.to_current(t.terrain_id()),
            terrain_id::OCEAN
        );
        assert!(t.is_water(TerrainNumbering::Legacy));
        assert!(t.is_water(TerrainNumbering::Current), "11 is Coast there");
        assert!(t.has_feature(feature::START_LOCATION | feature::BONUS_GRASSLAND));
        assert_eq!(t.feature_plane() & feature::RIVER_CORNERS, 0x0500_0000);
        assert!(t.has_river());
        // Bits outside the conversion masks are dropped like the game does.
        let t = Tile {
            packed_terrain: 0x00,
            packed_high: 0xF0,
            ..Tile::default()
        };
        assert_eq!(t.overlay_plane(), 0xE000_0000);
        assert_eq!(t.feature_plane(), 0x5000_0000);
    }

    #[test]
    fn expanded_rows_win_when_the_depth_byte_allows() {
        let t = Tile {
            packed_overlays: 0xFF,
            flags_0x28: Some(overlay::MINE as i32),
            terrain_class: Some(0xDD00),
            flags_0x30: Some(feature::PINE_FOREST as i32),
            water_depth_byte: Some(6),
            ..Tile::default()
        };
        assert!(t.uses_expanded_planes());
        assert_eq!(t.overlay_plane(), overlay::MINE);
        assert_eq!(t.terrain_id(), terrain_id::OCEAN);
        assert!(t.has_feature(feature::PINE_FOREST));
        let t = Tile {
            water_depth_byte: Some(3),
            ..t
        };
        assert!(!t.uses_expanded_planes());
        assert_eq!(t.overlay_plane(), 0xFF);
    }

    #[test]
    fn golden_island_hop_ocean_packed() {
        // Island Hop (PTW, no TERR section, version below 12.03: legacy
        // numbering): the first tile is deep ocean, `0xBB` = 11/11 (Ocean in
        // that numbering; Coast would be `0x99`). 29-byte row.
        let files = corpus::files();
        let Some(file) = files.iter().find(|f| f.name().contains("Island_Hop")) else {
            return;
        };
        let body = file
            .raw
            .section(b"TILE")
            .unwrap()
            .rows
            .first()
            .map(|r| file.raw.row(r))
            .unwrap();
        let mut r = Reader::new(body);
        let t = Tile::read(&mut r, &file.ctx()).unwrap();
        assert_eq!(body.len(), 29);
        assert_eq!(t.resource, -1);
        assert_eq!(t.terrain_image, 0x36);
        assert_eq!(t.terrain_file, 0x06);
        assert_eq!(t.packed_terrain, 0xBB);
        assert_eq!(t.terrain_id(), 11);
        assert_eq!(t.terrain_sub_class(), 11);
        assert!(file.raw.section(b"TERR").is_none());
        assert_eq!(
            TerrainNumbering::of_file(0, file.version),
            TerrainNumbering::Legacy
        );
        assert!(t.is_water(TerrainNumbering::Legacy));
        assert!(t.terrain_class.is_none());
    }

    #[test]
    fn golden_rise_of_rome_city_indices_on_tiles() {
        // Roma is CITY row 0 at (47,67); the tile stores `city_id == 0` (`Cell+0x1A`, combat.md).
        let files = corpus::files();
        let Some(file) = files.iter().find(|f| f.name().contains("Rise_of_Rome")) else {
            return;
        };
        let wsec = file.raw.section(b"WMAP").unwrap();
        let mut wr = Reader::new(file.raw.row(&wsec.rows[0]));
        let wm = WorldMap::read(&mut wr, &file.ctx()).unwrap();
        let tsec = file.raw.section(b"TILE").unwrap();
        let csec = file.raw.section(b"CITY").unwrap();
        for (ci, crow) in csec.rows.iter().enumerate().take(5) {
            let mut cr = Reader::new(file.raw.row(crow));
            let c = City::read(&mut cr, &file.ctx()).unwrap();
            let idx = index_from_coords(c.map_x, c.map_y, wm.width) as usize;
            let mut tr = Reader::new(file.raw.row(&tsec.rows[idx]));
            let tile = Tile::read(&mut tr, &file.ctx()).unwrap();
            assert_eq!(
                tile.city_id, ci as i16,
                "{} at ({},{})",
                c.name, c.map_x, c.map_y
            );
            assert_eq!(tile.colony_id, -1);
        }
    }

    #[test]
    fn golden_rome_road_overlay_bit() {
        let files = corpus::files();
        let Some(file) = files.iter().find(|f| f.name().contains("Rise_of_Rome")) else {
            return;
        };
        let tsec = file.raw.section(b"TILE").unwrap();
        let mut roads = 0;
        for row in &tsec.rows {
            let mut r = Reader::new(file.raw.row(row));
            let t = Tile::read(&mut r, &file.ctx()).unwrap();
            if t.overlay_plane() == overlay::ROAD {
                roads += 1;
                assert!(t.has_overlay(overlay::ROAD));
                assert!(!t.has_overlay(overlay::RAILROAD));
            }
        }
        assert!(roads > 100, "Rise of Rome should contain road tiles");
    }

    #[test]
    fn golden_mesopotamia_terrain_and_resource() {
        let files = corpus::files();
        let Some(file) = files.iter().find(|f| f.name().contains("Mesopotamia")) else {
            return;
        };
        let sec = file.raw.section(b"TILE").unwrap();
        let good_n = file.raw.section(b"GOOD").map(|s| s.rows.len()).unwrap_or(0);
        let terr_n = file.raw.section(b"TERR").map(|s| s.rows.len()).unwrap_or(0);
        let cont_n = file.raw.section(b"CONT").map(|s| s.rows.len()).unwrap_or(0);
        let mut ocean = 0;
        for row in &sec.rows {
            let mut r = Reader::new(file.raw.row(row));
            let t = Tile::read(&mut r, &file.ctx()).unwrap();
            assert_eq!(file.raw.row(row).len(), 45);
            if let Some(rid) = t.resource_index() {
                assert!(rid < good_n as u32, "resource {rid} >= {good_n}");
            }
            let numbering = TerrainNumbering::of_file(terr_n, file.version);
            assert_eq!(numbering, TerrainNumbering::Current);
            assert!((t.terrain_id() as usize) < terr_n);
            if t.continent_id >= 0 {
                assert!((t.continent_id as usize) < cont_n);
            }
            assert_eq!(t.water_depth_byte, Some(6));
            assert_eq!(t.owner, 0);
            if t.is_water(numbering) {
                ocean += 1;
            }
        }
        assert!(ocean > 900, "expected many water tiles, got {ocean}");
    }

    #[test]
    fn golden_conquests_49_byte_padding() {
        let files = corpus::files();
        let Some(file) = files
            .iter()
            .find(|f| f.name().contains("Intro3_New_Alliances"))
        else {
            return;
        };
        let body = file
            .raw
            .section(b"TILE")
            .unwrap()
            .rows
            .first()
            .map(|r| file.raw.row(r))
            .unwrap();
        assert_eq!(body.len(), 49);
        let mut r = Reader::new(body);
        let t = Tile::read(&mut r, &file.ctx()).unwrap();
        assert_eq!(t.legacy_padding_0x2d, Some(0));
        assert_eq!(t.flags_0x34, Some(0));
        // The first cell is deep ocean: terrain word 0xDD00 (secondary class and id 13).
        assert_eq!(t.terrain_class, Some(0xDD00));
        assert_eq!(t.terrain_id(), terrain_id::OCEAN);
        assert_eq!(t.terrain_sub_class(), terrain_id::OCEAN);
        let sengoku = files
            .iter()
            .find(|f| f.name().contains("8_Napoleonic_Europe"));
        if let Some(f2) = sengoku {
            let b45 = f2
                .raw
                .section(b"TILE")
                .unwrap()
                .rows
                .first()
                .map(|r| f2.raw.row(r))
                .unwrap();
            assert_eq!(b45.len(), 45);
            assert_eq!(&body[..45], b45, "49-byte prefix must match 45-byte layout");
        }
    }

    #[test]
    fn tile_count_matches_wmap_in_corpus() {
        let files = corpus::files();
        if files.is_empty() {
            return;
        }
        let mut mismatches = Vec::new();
        for f in &files {
            let Some(tsec) = f.raw.section(b"TILE") else {
                continue;
            };
            let Some(wsec) = f.raw.section(b"WMAP") else {
                continue;
            };
            let Some(wrow) = wsec.rows.first() else {
                continue;
            };
            let mut r = Reader::new(f.raw.row(wrow));
            let Ok(wm) = WorldMap::read(&mut r, &f.ctx()) else {
                continue;
            };
            let want = expected_row_count(wm.width, wm.height) as usize;
            if tsec.rows.len() != want {
                mismatches.push((f.name(), tsec.rows.len(), want, wm.width, wm.height));
            }
        }
        assert!(mismatches.is_empty(), "{mismatches:?}");
    }

    #[test]
    fn units_stand_on_valid_terrain() {
        let files = corpus::files();
        let Some(file) = files.iter().find(|f| f.name().contains("Mesopotamia")) else {
            return;
        };
        let Some(units) = file.raw.section(b"UNIT") else {
            return;
        };
        let wsec = file.raw.section(b"WMAP").unwrap();
        let mut r = Reader::new(file.raw.row(&wsec.rows[0]));
        let wm = WorldMap::read(&mut r, &file.ctx()).unwrap();
        let w = wm.width;
        let tsec = file.raw.section(b"TILE").unwrap();
        for urow in &units.rows {
            let body = file.raw.row(urow);
            if body.len() < 60 {
                continue;
            }
            let ux = i32::from_le_bytes(body[52..56].try_into().unwrap());
            let uy = i32::from_le_bytes(body[56..60].try_into().unwrap());
            let idx = index_from_coords(ux, uy, w) as usize;
            let tbody = file.raw.row(&tsec.rows[idx]);
            let mut tr = Reader::new(tbody);
            let tile = Tile::read(&mut tr, &file.ctx()).unwrap();
            assert!(
                !tile.is_water(TerrainNumbering::Current) || tile.terrain_id() == terrain_id::COAST,
                "unit at ({ux},{uy}) on deep water class {}",
                tile.terrain_id()
            );
        }
    }

    /// `Scenarios/7 MP Sengoku - Sword of the Shogun.biq` was edited outside the editor (62 x 88
    /// map, 264 of 477 river edges lack their mirror bit, and one start
    /// location sits on an odd-parity cell next to the marked one). It is the
    /// only shipped file that breaks the tile/SLOC/river invariants below, so
    /// those tests skip it; its round trip is still byte-exact.
    fn is_hand_edited_sengoku(name: &str) -> bool {
        name.contains("7 MP Sengoku")
    }

    /// The river set is nearly symmetric: if `T` has bit `d`, its neighbour in
    /// direction `d` has bit `(d + 4) % 8` for 98% of land-to-land edges. This
    /// is what establishes the direction numbering of [`river_connection`]
    /// independently of combat; the exceptions are documented there.
    ///
    /// Water carries no river bits (and a river ends at a water neighbour
    /// without a mirror bit), with one exception: some hand-made Civ3 1.x /
    /// PTW maps put rivers *through* Coast tiles - lakes in a river system -
    /// whose land neighbours do mirror them. Those tiles are neither counted
    /// as land nor as river mouths here.
    #[test]
    fn river_adjacency_is_nearly_symmetric() {
        let (mut land, mut land_mirrored) = (0u64, 0u64);
        let (mut mouths, mut mouths_mirrored) = (0u64, 0u64);
        let mut river_coast_tiles = 0u64;
        for (name, biq) in maps() {
            if is_hand_edited_sengoku(&name) {
                continue;
            }
            let v = view(&biq);
            let river_coast = |t: &Tile| {
                v.numbering == TerrainNumbering::Legacy
                    && v.numbering.to_current(t.terrain_id()) == terrain_id::COAST
            };
            for ((x, y), t) in v.iter() {
                if v.is_water(t) {
                    if river_coast(t) {
                        river_coast_tiles += (t.river_connection_mask != 0) as u64;
                    } else {
                        assert_eq!(t.river_connection_mask, 0, "{name}: ({x},{y}) is water");
                    }
                    continue;
                }
                for d in 0..8u32 {
                    if !t.river_towards(d) {
                        continue;
                    }
                    let (dx, dy) = river_connection::DELTAS[d as usize];
                    let Some(n) = v.tile(x + dx, y + dy) else {
                        continue; // map edge
                    };
                    if river_coast(n) {
                        continue;
                    }
                    let mirrored = n.river_towards(river_connection::opposite(d)) as u64;
                    if v.is_water(n) {
                        mouths += 1;
                        mouths_mirrored += mirrored;
                    } else {
                        land += 1;
                        land_mirrored += mirrored;
                    }
                }
            }
        }
        if land > 0 {
            assert!(land > 100_000, "only {land} river edges examined");
            assert!(land_mirrored * 100 >= land * 98, "{land_mirrored}/{land}");
            assert!(mouths > 100, "only {mouths} river mouths");
            assert_eq!(mouths_mirrored, 0);
            // Coast tiles with rivers: Earth (Huge) 6, Boatless Journey 39, ...
            assert!(river_coast_tiles > 50, "{river_coast_tiles}");
        }
    }

    /// `feature::START_LOCATION` marks exactly the `SLOC` positions in every
    /// file that has both (old `.bic` files have the bit but no `SLOC`).
    #[test]
    fn start_location_bit_matches_sloc_rows() {
        let mut checked = 0;
        for (name, biq) in maps() {
            if biq.map.start_locations.is_empty() || is_hand_edited_sengoku(&name) {
                continue;
            }
            let v = view(&biq);
            let mut marked: Vec<(i32, i32)> = v
                .iter()
                .filter(|(_, t)| t.has_feature(feature::START_LOCATION))
                .map(|(p, _)| p)
                .collect();
            let mut listed: Vec<(i32, i32)> = biq
                .map
                .start_locations
                .iter()
                .map(|s| (s.map_x, s.map_y))
                .collect();
            marked.sort();
            listed.sort();
            listed.dedup();
            assert_eq!(marked, listed, "{name}");
            checked += 1;
        }
        if !corpus::files().is_empty() {
            assert!(checked > 20, "only {checked} files checked");
        }
    }

    /// Bonus grassland, snow-capped mountains and pine forest are variants of
    /// one terrain each; the flags are mostly (not exclusively: the editor
    /// leaves stale bits when terrain is repainted) on that terrain.
    #[test]
    fn terrain_variant_bits_sit_on_their_terrain() {
        // (bit, terrain id, minimum share in percent)
        let cases = [
            (feature::BONUS_GRASSLAND, 2u8, 80u64),
            (feature::SNOW_CAPPED, terrain_id::MOUNTAINS, 95),
            (feature::PINE_FOREST, terrain_id::FOREST, 80),
        ];
        let mut total = [0u64; 3];
        let mut on = [0u64; 3];
        for (_, biq) in maps() {
            for (_, t) in biq.map_view().unwrap().iter() {
                for (i, (bit, terr, _)) in cases.iter().enumerate() {
                    if t.has_feature(*bit) {
                        total[i] += 1;
                        if t.terrain_id() == *terr {
                            on[i] += 1;
                        }
                    }
                }
            }
        }
        for (i, (_, _, share)) in cases.iter().enumerate() {
            if total[i] == 0 {
                continue;
            }
            assert!(
                on[i] * 100 >= total[i] * share,
                "case {i}: {}/{}",
                on[i],
                total[i]
            );
        }
    }

    /// The editor stores the owner as `LEAD index + 1`; in the small
    /// editor-saved files that is the whole range.
    #[test]
    fn tile_owner_is_a_player_slot_in_editor_saved_files() {
        let files = corpus::files();
        let Some(file) = files.iter().find(|f| f.name().contains("Intro2_The_Three")) else {
            return;
        };
        let players = file.raw.section(b"LEAD").unwrap().rows.len();
        let mut seen = std::collections::BTreeSet::new();
        for row in &file.raw.section(b"TILE").unwrap().rows {
            seen.insert(file.raw.row(row)[1]);
        }
        let want: std::collections::BTreeSet<u8> = (0..=players as u8).collect();
        assert_eq!(seen, want);
    }

    #[test]
    fn river_direction_helpers() {
        for d in 0..8u32 {
            assert_eq!(river_connection::opposite(river_connection::opposite(d)), d);
            let (dx, dy) = river_connection::DELTAS[d as usize];
            let (ox, oy) = river_connection::DELTAS[river_connection::opposite(d) as usize];
            assert_eq!((dx + ox, dy + oy), (0, 0));
        }
        assert_eq!(river_connection::NW, 0x80);
        let t = Tile {
            river_connection_mask: river_connection::E | river_connection::W,
            ..Tile::default()
        };
        assert!(t.river_towards(2) && t.river_towards(6) && !t.river_towards(0));
        assert!(t.has_river());
    }
}
