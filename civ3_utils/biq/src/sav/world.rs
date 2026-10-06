//! The map of a save: `WRLD` header chunks, one four-chunk record per cell,
//! the continent list and the per-resource array.

use super::body::Body;
use super::counts::RuleCounts;
use super::stream::{Rd, put_chunk, put_chunks, put_u32s, want};
use crate::io::{Error, Result, Writer};

/// One map cell: the four chunks the game writes for a `Cell` object
/// (stride `0xF0`, `NOTES.md` §4), named by the **`Cell` offset** each chunk
/// starts at.
///
/// | field | chunk | `Cell` bytes |
/// |---|---|---|
/// | [`cell_04`](Tile::cell_04) | `TILE` 36 | `+0x04..+0x28` |
/// | [`cell_28`](Tile::cell_28) | `TILE` 12 | `+0x28..+0x34` |
/// | [`cell_34`](Tile::cell_34) | `TILE` 4 | `+0x34..+0x38` |
/// | [`cell_58`](Tile::cell_58) | `TILE` 128 | `+0x58..+0xD8` |
///
/// The first three are the fields the scenario `TILE` row carries
/// ([`crate::sections::tile`] lists them by `Cell` offset); the fourth is the
/// per-game state a scenario does not have (visibility and knowledge masks,
/// the worked-by city, per-civilization remembered overlays). Use
/// [`Tile::cell_u8`], [`Tile::cell_u16`] and [`Tile::cell_u32`] to read any
/// field by its `Cell` offset.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Tile {
    /// `Cell+0x04..0x28`: river mask, owner, resource, terrain image, packed
    /// dword, barbarian tribe, city, colony and continent ids, water depth,
    /// victory point and ruin ids.
    pub cell_04: Body<36>,
    /// `Cell+0x28..0x34`: overlay, terrain and feature planes.
    pub cell_28: Body<12>,
    /// `Cell+0x34..0x38`.
    pub cell_34: Body<4>,
    /// Twelve bytes between the third and fourth chunk that the loader skips
    /// (`0x5D9C98`, `add ebp, 0xC`): present in sub-versions below 8 when the
    /// signed water-depth byte (`Cell+0x20`) is at least 6. Never written by
    /// the current game and never read, so always `None` in a sub-version 10
    /// save.
    pub legacy_12: Option<Body<12>>,
    /// `Cell+0x58..0xD8`: runtime state.
    pub cell_58: Body<128>,
}

impl Tile {
    /// Whether a save of this sub-version carries [`Tile::legacy_12`] for a
    /// cell with this first chunk.
    pub(super) fn has_legacy_12(cell_04: &Body<36>, sub: u32) -> bool {
        sub < 8 && (cell_04.u8(0x1C) as i8) >= 6
    }

    /// Locate `len` bytes at `Cell` offset `off` inside one of the four chunks:
    /// `(bytes, index of the first byte)`.
    fn at(&self, off: usize, len: usize) -> Option<(&[u8], usize)> {
        let (bytes, start): (&[u8], usize) = match off {
            0x04..0x28 => (&self.cell_04.0, 0x04),
            0x28..0x34 => (&self.cell_28.0, 0x28),
            0x34..0x38 => (&self.cell_34.0, 0x34),
            0x58..0xD8 => (&self.cell_58.0, 0x58),
            _ => return None,
        };
        let i = off - start;
        (i + len <= bytes.len()).then_some((bytes, i))
    }

    /// Byte at `Cell` offset `off`, `None` if the save does not store it.
    pub fn cell_u8(&self, off: usize) -> Option<u8> {
        self.at(off, 1).map(|(b, i)| b[i])
    }

    /// Little-endian `u16` at `Cell` offset `off` (must lie inside one chunk).
    pub fn cell_u16(&self, off: usize) -> Option<u16> {
        self.at(off, 2)
            .map(|(b, i)| u16::from_le_bytes([b[i], b[i + 1]]))
    }

    /// Little-endian `u32` at `Cell` offset `off` (must lie inside one chunk).
    pub fn cell_u32(&self, off: usize) -> Option<u32> {
        self.at(off, 4)
            .map(|(b, i)| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]))
    }

    /// `Cell+0x04`: eight-direction river adjacency mask.
    pub fn river_connection_mask(&self) -> u8 {
        self.cell_04.u8(0x00)
    }

    /// `Cell+0x05`: the civilization that owns the tile's border (0 = none).
    pub fn owner(&self) -> u8 {
        self.cell_04.u8(0x01)
    }

    /// `Cell+0x08`: `GOOD` row of the resource, `-1` for none.
    pub fn resource(&self) -> i32 {
        self.cell_04.i32(0x04)
    }

    /// `Cell+0x1A`: index of the city on the tile, `-1` for none.
    pub fn city_id(&self) -> i16 {
        self.cell_04.i16(0x16)
    }

    /// `Cell+0x1C`: index of the colony (airfield, radar tower, outpost,
    /// plain colony) on the tile, `-1` for none.
    pub fn colony_id(&self) -> i16 {
        self.cell_04.i16(0x18)
    }

    /// `Cell+0x1E`: continent id.
    pub fn continent_id(&self) -> i16 {
        self.cell_04.i16(0x1A)
    }

    /// `Cell+0x6C`: id of the city that works the tile, `-1` for none. The
    /// city is within the 21-tile radius of the tile (a tile may be worked by
    /// a city of another civilization than its border owner).
    pub fn worked_by_city(&self) -> i16 {
        self.cell_58.i16(0x14)
    }

    /// `Cell+0x58`: bit set of civilizations; the owner's bit is always set
    /// (**HYPOTHESIS**: the civilizations whose borders reach the tile).
    pub fn claim_mask(&self) -> u32 {
        self.cell_58.u32(0)
    }

    /// `Cell+0x58`: whether the civilization in `slot` has ever seen the
    /// tile (`vision.md` 1.2; map trades and goody huts set this bit far
    /// from the civilization's units, so it is the whole remembered map).
    pub fn discovered(&self, slot: u32) -> bool {
        self.cell_u32(0x58).is_some_and(|m| m & (1 << slot) != 0)
    }

    /// Whether the tile is currently visible to `slot`: seen by a unit
    /// (`+0x5C`), a structure (`+0x60`), territory (`+0x64`) or an air
    /// reveal (`+0xD0`) (`vision.md` 1.3; the radar mask `+0xD4` is not
    /// part of it).
    pub fn visible_to(&self, slot: u32) -> bool {
        [0x5C, 0x60, 0x64, 0xD0]
            .iter()
            .filter_map(|&off| self.cell_u32(off))
            .fold(0, |acc, m| acc | m)
            & (1 << slot)
            != 0
    }

    /// `Cell+0x28`: the overlay plane ([`crate::sections::tile::overlay`]).
    pub fn overlay_plane(&self) -> u32 {
        self.cell_28.u32(0)
    }

    /// `Cell+0x2C`: the terrain word; see [`Tile::terrain_id`].
    pub fn terrain_word(&self) -> u32 {
        self.cell_28.u32(4)
    }

    /// `Cell+0x30`: the feature plane ([`crate::sections::tile::feature`]).
    pub fn feature_plane(&self) -> u32 {
        self.cell_28.u32(8)
    }

    /// Terrain id: bits 12..16 of [`Tile::terrain_word`], the `TERR` row of
    /// the Conquests numbering ([`crate::sections::tile::terrain_id`]).
    pub fn terrain_id(&self) -> u8 {
        ((self.terrain_word() >> 12) & 0xF) as u8
    }
}

/// The map: header, cells, continents and the per-resource array.
///
/// Loader `0x5D9580` (`Map` object `0x9C7560`, base `0x9C736C`), writer
/// `0x5D9410`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Map {
    /// `WRLD` 2: `u16` continent count (`Map+0x230`). The loader falls back to
    /// a derived count when it is zero; that path is not modelled and is an
    /// error here.
    pub continent_count: Body<2>,
    /// `WRLD` 164 = `Map+0x150..0x1F4`: the same 41 dwords as a scenario's
    /// `WMAP` row after its resource rolls ([`crate::sections::wmap`]). See the
    /// accessors. The oceans percentage (`+0x9C`) is not meaningful in a save:
    /// random-map games leave it uninitialised.
    pub header: Body<164>,
    /// `WRLD` 52: further map state (`Map` object, format version >= 13).
    pub header_ext: Body<52>,
    /// `(width / 2) * height` cells in index order
    /// ([`crate::sections::tile::index_from_coords`]).
    pub tiles: Vec<Tile>,
    /// `CONT` 8: one record per continent id.
    pub continents: Vec<Body<8>>,
    /// Raw block of one dword per `GOOD` row (`Map+0x14C`, pc `0x5D971A`).
    pub goods: Vec<u32>,
}

impl Map {
    /// Land continent count (`Map+0x150`, `WMAP` `land_continent_count`).
    pub fn land_continent_count(&self) -> u32 {
        self.header.u32(0x00)
    }

    /// Height in tiles (`Map+0x154`).
    pub fn height(&self) -> u32 {
        self.header.u32(0x04)
    }

    /// Start-site search radius (`Map+0x158`).
    pub fn start_site_radius(&self) -> u32 {
        self.header.u32(0x08)
    }

    /// The scenario's number of players (`Map+0x15C`).
    pub fn number_of_players(&self) -> u32 {
        self.header.u32(0x0C)
    }

    /// Width in tiles (`Map+0x168`).
    pub fn width(&self) -> u32 {
        self.header.u32(0x18)
    }

    /// Wrap flags (`Map+0x1F0`, [`crate::sections::wmap::wrap`]).
    pub fn wrap_flags(&self) -> u32 {
        self.header.u32(0xA0)
    }

    /// Number of cells the header promises: `(width / 2) * height`.
    pub fn cell_count(&self) -> usize {
        (self.width() as usize / 2) * self.height() as usize
    }

    /// The tile at `(x, y)`, `None` outside the map. Coordinates of the
    /// isometric grid have `x + y` even; an odd sum names the cell of
    /// `x - 1` (the game masks the low bit, `0x5DC1C0`).
    pub fn tile(&self, x: u32, y: u32) -> Option<&Tile> {
        let (w, h) = (self.width(), self.height());
        if x >= w || y >= h {
            return None;
        }
        self.tiles.get((w / 2 * y + x / 2) as usize)
    }

    pub(super) fn read(rd: &mut Rd, sub: u32, counts: &RuleCounts) -> Result<Map> {
        let continent_count = rd.chunk::<2>(b"WRLD")?;
        let header = rd.chunk::<164>(b"WRLD")?;
        let header_ext = rd.chunk::<52>(b"WRLD")?;
        let mut map = Map {
            continent_count,
            header,
            header_ext,
            tiles: Vec::new(),
            continents: Vec::new(),
            goods: Vec::new(),
        };
        if map.continent_count.u16(0) == 0 {
            return Err(Error::Unsupported {
                what: "map with a zero continent count",
                value: 0,
            });
        }
        let n = map.cell_count();
        // A cell is four chunk headers plus 180 body bytes.
        if n > rd.remaining() / (4 * 8 + 180) {
            return Err(Error::Truncated {
                offset: rd.pos(),
                what: "TILE records",
            });
        }
        map.tiles.reserve_exact(n);
        for _ in 0..n {
            let cell_04 = rd.chunk(b"TILE")?;
            let cell_28 = rd.chunk(b"TILE")?;
            let cell_34 = rd.chunk(b"TILE")?;
            let legacy_12 = if Tile::has_legacy_12(&cell_04, sub) {
                let mut b = [0u8; 12];
                b.copy_from_slice(rd.take(12, "legacy tile bytes")?);
                Some(Body(b))
            } else {
                None
            };
            map.tiles.push(Tile {
                cell_04,
                cell_28,
                cell_34,
                legacy_12,
                cell_58: rd.chunk(b"TILE")?,
            });
        }
        map.continents = rd.chunks(map.continent_count.u16(0) as usize, b"CONT")?;
        map.goods = rd.u32s(counts.goods, "map resource array")?;
        Ok(map)
    }

    pub(super) fn write(&self, w: &mut Writer, sub: u32, counts: &RuleCounts) -> Result<()> {
        want(
            self.tiles.len(),
            self.cell_count(),
            "tile count must be (width / 2) * height",
        )?;
        want(
            self.continents.len(),
            self.continent_count.u16(0) as usize,
            "continent list must match the continent count",
        )?;
        want(
            self.goods.len(),
            counts.goods,
            "map resource array must have one entry per GOOD row",
        )?;
        put_chunk(w, b"WRLD", &self.continent_count);
        put_chunk(w, b"WRLD", &self.header);
        put_chunk(w, b"WRLD", &self.header_ext);
        for t in &self.tiles {
            put_chunk(w, b"TILE", &t.cell_04);
            put_chunk(w, b"TILE", &t.cell_28);
            put_chunk(w, b"TILE", &t.cell_34);
            match (&t.legacy_12, Tile::has_legacy_12(&t.cell_04, sub)) {
                (Some(b), true) => w.bytes(&b.0),
                (None, false) => {}
                _ => {
                    return Err(Error::Inconsistent(
                        "legacy tile bytes must match the sub-version and water depth",
                    ));
                }
            }
            put_chunk(w, b"TILE", &t.cell_58);
        }
        put_chunks(w, b"CONT", &self.continents);
        put_u32s(w, &self.goods);
        Ok(())
    }
}
