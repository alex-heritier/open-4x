//! The whole scenario file as one typed value: [`Biq`].
//!
//! A `.biq`/`.bix`/`.bic` stream is a magic followed by independent sections
//! ([`crate::raw`]). The sections fall into three layers, which is also how the
//! editor organises them and how this model is grouped:
//!
//! | layer | sections | type |
//! |---|---|---|
//! | **rules** (the rule set the scenario plays by) | `BLDG CTZN CULT DIFF ERAS ESPN EXPR FLAV GOOD GOVT RULE PRTO RACE TECH TFRM TERR WSIZ` | [`Rules`] |
//! | **map** | `WCHR WMAP TILE CONT SLOC` | [`MapData`] |
//! | **scenario** (what is on the map and who plays) | `CITY UNIT CLNY GAME LEAD` | [`Scenario`] |
//!
//! plus the `VER#` header ([`Biq::header`]). A rules-only file such as
//! `conquests.biq` has no map and no scenario; a scenario carries a full copy
//! of its rules.
//!
//! The model keeps which sections the file had and in which order
//! ([`Biq::section_order`]; the editor and the game write different orders and
//! 11 distinct ones occur in the shipped files), so [`Biq::to_stream`]
//! re-creates the decoded stream byte for byte. Sections the crate does not
//! know are kept verbatim in [`Biq::unknown_sections`] (none exist in the
//! shipped files; the loader skips unknown tags, `0x594E7E`).
//!
//! Compression is a transport detail: [`Biq::parse`] accepts DCL-compressed or
//! plain input and remembers which ([`Biq::storage`]). [`Biq::to_stream`]
//! produces the plain stream, which the game loads just as well (it sniffs the
//! magic); [`Biq::to_bytes`] produces the file as it should sit on disk, running
//! the stream through [`crate::implode`] when the file arrived compressed. That
//! compressor is a port of the game's own, so an unmodified file comes back
//! **byte for byte** (`every_shipped_file_round_trips_as_bytes`).

use crate::Version;
use crate::dcl::Storage;
use crate::io::{Ctx, Error, Reader, Record, Result, Writer};
use crate::raw::{Magic, Raw, RawSection};
use crate::sections::flav::Flavors;
use crate::sections::tile::{self, Tile};
use crate::sections::{
    bldg::Building, city::City, clny::Colony, cont::Continent, ctzn::Citizen, cult::CultureLevel,
    diff::Difficulty, eras::Era, espn::EspionageMission, expr::ExperienceLevel, game::Game,
    good::Good, govt::Government, lead::Player, prto::UnitType, race::Civilization,
    rule::GeneralRules, sloc::StartLocation, tech::Tech, terr::Terrain, terr::TerrainNumbering,
    tfrm::WorkerJob, unit::Unit, ver::Header, wchr::WorldCharacteristics, wmap::WorldMap,
    wsiz::WorldSize,
};
use std::path::Path;

/// Section order of the game's own scenario writer (`0x597070`). Used to place
/// sections that were added to a model that did not have them.
pub const CANONICAL_ORDER: [[u8; 4]; 28] = [
    *b"VER#", *b"BLDG", *b"CTZN", *b"CULT", *b"DIFF", *b"ERAS", *b"ESPN", *b"EXPR", *b"FLAV",
    *b"GOOD", *b"GOVT", *b"RULE", *b"PRTO", *b"RACE", *b"TECH", *b"TFRM", *b"TERR", *b"WSIZ",
    *b"WCHR", *b"WMAP", *b"TILE", *b"CONT", *b"SLOC", *b"CITY", *b"UNIT", *b"CLNY", *b"GAME",
    *b"LEAD",
];

fn parse_rows<T: Record>(raw: &Raw, sec: &RawSection, ctx: &Ctx) -> Result<Vec<T>> {
    sec.rows
        .iter()
        .map(|r| T::read(&mut Reader::new(raw.row(r)), ctx))
        .collect()
}

fn write_rows<T: Record>(w: &mut Writer, rows: &[T], ctx: &Ctx) {
    w.tag(T::TAG);
    w.u32(rows.len() as u32);
    for t in rows {
        w.row(|w| t.write(w, ctx));
    }
}

/// Declares a group of sections as a struct of `Vec<Row>` fields plus the
/// per-tag dispatch used by [`Biq`].
macro_rules! group {
    (
        $(#[$meta:meta])*
        pub struct $name:ident {
            $( $(#[$fmeta:meta])* pub $field:ident : $ty:ty, )*
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Debug, Default)]
        pub struct $name {
            $( $(#[$fmeta])* pub $field: Vec<$ty>, )*
        }

        impl $name {
            /// Parse `sec` into the field for its tag. `Ok(false)` if this
            /// group has no section with that tag.
            fn parse_section(&mut self, raw: &Raw, sec: &RawSection, ctx: &Ctx) -> Result<bool> {
                $( if sec.tag == <$ty as Record>::TAG {
                    self.$field = parse_rows(raw, sec, ctx)?;
                    return Ok(true);
                } )*
                Ok(false)
            }

            /// Write the section with `tag`; `false` if it is not in this group.
            fn write_section(&self, tag: [u8; 4], w: &mut Writer, ctx: &Ctx) -> bool {
                $( if tag == <$ty as Record>::TAG {
                    write_rows(w, &self.$field, ctx);
                    return true;
                } )*
                false
            }

            /// Row count of the section with `tag`, if it belongs to this group.
            fn row_count(&self, tag: [u8; 4]) -> Option<usize> {
                $( if tag == <$ty as Record>::TAG { return Some(self.$field.len()); } )*
                None
            }

            /// Total bytes in rows that the typed model did not consume.
            pub fn unmodelled_bytes(&self) -> usize {
                0 $( + self.$field.iter().map(|r| r.extra().len()).sum::<usize>() )*
            }
        }
    };
}

group! {
    /// The rule set: everything the editor's *Rules* pages edit. Vectors are in
    /// file order; cross-references between sections (a unit's required
    /// resource, a building's required advance, ...) are indices into the
    /// vector of the target section, `-1` meaning none.
    pub struct Rules {
        /// `BLDG` - improvements and wonders.
        pub buildings: Building,
        /// `CTZN` - citizen types.
        pub citizens: Citizen,
        /// `CULT` - culture levels.
        pub cultures: CultureLevel,
        /// `DIFF` - difficulty levels.
        pub difficulties: Difficulty,
        /// `ERAS` - eras.
        pub eras: Era,
        /// `ESPN` - diplomat and spy missions.
        pub espionage_missions: EspionageMission,
        /// `EXPR` - combat experience levels.
        pub experience_levels: ExperienceLevel,
        /// `GOOD` - natural resources.
        pub goods: Good,
        /// `GOVT` - governments.
        pub governments: Government,
        /// `RULE` - general settings (one row).
        pub general_rules: GeneralRules,
        /// `PRTO` - unit types.
        pub unit_types: UnitType,
        /// `RACE` - civilizations.
        pub civilizations: Civilization,
        /// `TECH` - advances.
        pub techs: Tech,
        /// `TFRM` - worker jobs (terraforming).
        pub worker_jobs: WorkerJob,
        /// `TERR` - terrain types.
        pub terrains: Terrain,
        /// `WSIZ` - world sizes.
        pub world_sizes: WorldSize,
    }
}

group! {
    /// The map: world parameters and one [`Tile`] per cell.
    pub struct MapData {
        /// `WCHR` - world characteristics chosen in the generator (one row).
        pub characteristics: WorldCharacteristics,
        /// `WMAP` - dimensions, seeds and wrap flags (one row).
        pub world_map: WorldMap,
        /// `TILE` - the cells, `(width / 2) * height` of them, see [`MapView`].
        pub tiles: Tile,
        /// `CONT` - continents.
        pub continents: Continent,
        /// `SLOC` - start locations.
        pub start_locations: StartLocation,
    }
}

group! {
    /// What is placed on the map and who plays.
    pub struct Scenario {
        /// `CITY` - cities.
        pub cities: City,
        /// `UNIT` - units.
        pub units: Unit,
        /// `CLNY` - colonies and tile improvements that are objects.
        pub colonies: Colony,
        /// `GAME` - scenario-wide settings (one row).
        pub game: Game,
        /// `LEAD` - players.
        pub players: Player,
    }
}

/// A section whose tag this crate does not know, kept verbatim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownSection {
    /// Section tag.
    pub tag: [u8; 4],
    /// Row bodies (without their length words).
    pub rows: Vec<Vec<u8>>,
}

/// A parsed scenario file.
#[derive(Clone, Debug)]
pub struct Biq {
    /// File signature (`BIC ` for Civ3 1.x, `BICX` for PTW/Conquests).
    pub magic: Magic,
    /// The `VER#` row: format version, title and description. Its version
    /// selects the on-disk layout of every other section.
    pub header: Option<Header>,
    /// Rule set.
    pub rules: Rules,
    /// `FLAV` flavors (Conquests files only).
    pub flavors: Option<Flavors>,
    /// Map.
    pub map: MapData,
    /// Scenario objects and settings.
    pub scenario: Scenario,
    /// Sections with unknown tags, in file order.
    pub unknown_sections: Vec<UnknownSection>,
    /// How the file was stored on disk; [`Biq::to_bytes`] writes the same way.
    pub storage: Storage,
    /// Tags of the sections the file had, in file order (including sections
    /// with zero rows and unknown ones). [`Biq::to_stream`] writes exactly
    /// these, then any known section that has rows but is not listed.
    pub section_order: Vec<[u8; 4]>,
}

impl Biq {
    /// An empty file of the given format version (header only).
    pub fn new(version: Version) -> Biq {
        let header = Header {
            major: version.major,
            minor: version.minor,
            ..Header::default()
        };
        Biq {
            magic: if version.major >= 11 {
                Magic::Bicx
            } else {
                Magic::Bic
            },
            header: Some(header),
            rules: Rules::default(),
            flavors: None,
            map: MapData::default(),
            scenario: Scenario::default(),
            unknown_sections: Vec::new(),
            storage: Storage::Plain,
            section_order: vec![*b"VER#"],
        }
    }

    /// Parse a file's bytes (DCL-compressed or plain).
    pub fn parse(input: &[u8]) -> Result<Biq> {
        Biq::from_raw(&Raw::parse(input)?)
    }

    /// Read and parse a file.
    pub fn read_file(path: impl AsRef<Path>) -> Result<Biq> {
        let bytes = std::fs::read(path.as_ref()).map_err(|e| Error::Io(e.to_string()))?;
        Biq::parse(&bytes)
    }

    /// Interpret an already framed stream.
    pub fn from_raw(raw: &Raw) -> Result<Biq> {
        if raw.magic == Magic::Civ3 {
            // A saved game: a different stream layout, not modelled.
            return Err(Error::BadMagic(raw.magic.bytes()));
        }
        // VER# first: its version is the context of every other section.
        let header = match raw.section(b"VER#") {
            None => None,
            Some(sec) if sec.rows.len() == 1 => {
                let mut r = Reader::new(raw.row(&sec.rows[0]));
                Some(Header::read(
                    &mut r,
                    &Ctx {
                        version: Version::default(),
                    },
                )?)
            }
            Some(sec) => {
                return Err(Error::BadCount {
                    tag: *b"VER#",
                    what: "VER# rows",
                    count: sec.count,
                });
            }
        };
        let ctx = Ctx {
            version: header.as_ref().map(Header::version).unwrap_or_default(),
        };
        let mut biq = Biq {
            magic: raw.magic,
            header,
            rules: Rules::default(),
            flavors: None,
            map: MapData::default(),
            scenario: Scenario::default(),
            unknown_sections: Vec::new(),
            storage: raw.storage,
            section_order: Vec::with_capacity(raw.sections.len()),
        };
        for sec in &raw.sections {
            if biq.section_order.contains(&sec.tag) {
                return Err(Error::DuplicateSection(sec.tag));
            }
            biq.section_order.push(sec.tag);
            if &sec.tag == b"VER#" {
                continue;
            }
            if &sec.tag == b"FLAV" {
                biq.flavors = Some(Flavors::from_raw(raw, sec)?);
                continue;
            }
            let known = biq.rules.parse_section(raw, sec, &ctx)?
                || biq.map.parse_section(raw, sec, &ctx)?
                || biq.scenario.parse_section(raw, sec, &ctx)?;
            if !known {
                biq.unknown_sections.push(UnknownSection {
                    tag: sec.tag,
                    rows: sec.rows.iter().map(|r| raw.row(r).to_vec()).collect(),
                });
            }
        }
        Ok(biq)
    }

    /// The format version from the header (`0.00` without one).
    pub fn version(&self) -> Version {
        self.header
            .as_ref()
            .map(Header::version)
            .unwrap_or_default()
    }

    /// How this file numbers its terrains: the ids in [`Tile::terrain_id`] and
    /// the indices into [`Rules::terrains`]. See [`TerrainNumbering`] for the
    /// rule the game applies.
    pub fn terrain_numbering(&self) -> TerrainNumbering {
        TerrainNumbering::of_file(self.rules.terrains.len(), self.version())
    }

    /// Reader/writer context for this file.
    pub fn ctx(&self) -> Ctx {
        Ctx {
            version: self.version(),
        }
    }

    /// Bytes inside rows that no typed field accounts for. `0` for every
    /// shipped file; non-zero means the file carries data newer than this
    /// model (it is still preserved and written back).
    pub fn unmodelled_bytes(&self) -> usize {
        self.rules.unmodelled_bytes()
            + self.map.unmodelled_bytes()
            + self.scenario.unmodelled_bytes()
            + self.header.as_ref().map_or(0, |h| h.extra().len())
    }

    fn row_count(&self, tag: [u8; 4]) -> Option<usize> {
        match &tag {
            b"VER#" => Some(usize::from(self.header.is_some())),
            b"FLAV" => Some(usize::from(self.flavors.is_some())),
            _ => self
                .rules
                .row_count(tag)
                .or_else(|| self.map.row_count(tag))
                .or_else(|| self.scenario.row_count(tag)),
        }
    }

    /// The order sections are written in: [`Biq::section_order`], with any
    /// populated known section missing from it placed after the nearest
    /// preceding section of the game's own order ([`CANONICAL_ORDER`]).
    fn write_order(&self) -> Vec<[u8; 4]> {
        let mut order = self.section_order.clone();
        for (i, tag) in CANONICAL_ORDER.iter().enumerate() {
            if order.contains(tag) || self.row_count(*tag).unwrap_or(0) == 0 {
                continue;
            }
            let at = CANONICAL_ORDER[..i]
                .iter()
                .rev()
                .find_map(|t| order.iter().position(|o| o == t))
                .map_or(0, |p| p + 1);
            order.insert(at, *tag);
        }
        order
    }

    /// The file as it should be saved: [`Biq::to_stream`] stored the way the
    /// file was read ([`Biq::storage`], plain unless it arrived compressed).
    /// Set `storage` to [`Storage::GAME`] to produce a compressed file from a
    /// model built in memory.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        Ok(self.storage.encode(&self.to_stream())?)
    }

    /// Write [`Biq::to_bytes`] to `path`.
    pub fn write_file(&self, path: impl AsRef<Path>) -> Result<()> {
        std::fs::write(path.as_ref(), self.to_bytes()?).map_err(|e| Error::Io(e.to_string()))
    }

    /// Encode the plain (decompressed) stream: magic, then the sections.
    pub fn to_stream(&self) -> Vec<u8> {
        let ctx = self.ctx();
        let mut w = Writer::new();
        w.bytes(&self.magic.bytes());
        let mut unknown = self.unknown_sections.iter();
        for tag in self.write_order() {
            match &tag {
                b"VER#" => {
                    w.tag(tag);
                    w.u32(u32::from(self.header.is_some()));
                    if let Some(h) = &self.header {
                        w.row(|w| h.write(w, &ctx));
                    }
                }
                b"FLAV" => {
                    if let Some(f) = &self.flavors {
                        f.write(&mut w);
                    }
                }
                _ => {
                    let known = self.rules.write_section(tag, &mut w, &ctx)
                        || self.map.write_section(tag, &mut w, &ctx)
                        || self.scenario.write_section(tag, &mut w, &ctx);
                    if !known && let Some(u) = unknown.next() {
                        w.tag(u.tag);
                        w.u32(u.rows.len() as u32);
                        for row in &u.rows {
                            w.row(|w| w.bytes(row));
                        }
                    }
                }
            }
        }
        w.buf
    }

    /// The tile grid, if the file has a map (`WMAP` and a matching number of
    /// `TILE` rows).
    pub fn map_view(&self) -> Option<MapView<'_>> {
        let wm = self.map.world_map.first()?;
        let expect = tile::expected_row_count(wm.width, wm.height) as usize;
        if expect == 0 || self.map.tiles.len() != expect {
            return None;
        }
        Some(MapView {
            width: wm.width,
            height: wm.height,
            wrap_x: wm.wrap_flags & crate::sections::wmap::wrap::X as i32 != 0,
            wrap_y: wm.wrap_flags & crate::sections::wmap::wrap::Y as i32 != 0,
            numbering: self.terrain_numbering(),
            tiles: &self.map.tiles,
        })
    }
}

/// The map as a grid of tiles addressed by `(x, y)`.
///
/// Civilization III maps are *staggered*: a width-`W`, height-`H` map has
/// `(W/2) * H` tiles and only the coordinates with `x + y` even exist. Tiles
/// are stored row by row (`index = (W/2) * y + x/2`), see
/// [`tile::index_from_coords`].
#[derive(Clone, Copy, Debug)]
pub struct MapView<'a> {
    /// Map width in coordinate units (twice the number of tiles per row).
    pub width: i32,
    /// Map height (number of rows).
    pub height: i32,
    /// The map wraps east-west (`WMAP` wrap flags).
    pub wrap_x: bool,
    /// The map wraps north-south.
    pub wrap_y: bool,
    /// How the file numbers terrains ([`Biq::terrain_numbering`]).
    pub numbering: TerrainNumbering,
    tiles: &'a [Tile],
}

impl<'a> MapView<'a> {
    /// True for Coast, Sea and Ocean tiles, whichever numbering the file uses.
    pub fn is_water(&self, tile: &Tile) -> bool {
        tile.is_water(self.numbering)
    }

    /// Tile at `(x, y)`; `None` outside the map or where `x + y` is odd.
    /// Coordinates are wrapped on the axes that wrap.
    pub fn tile(&self, x: i32, y: i32) -> Option<&'a Tile> {
        let x = if self.wrap_x {
            x.rem_euclid(self.width)
        } else {
            x
        };
        let y = if self.wrap_y {
            y.rem_euclid(self.height)
        } else {
            y
        };
        if x < 0 || y < 0 || x >= self.width || y >= self.height || (x + y) & 1 != 0 {
            return None;
        }
        self.tiles
            .get(tile::index_from_coords(x, y, self.width) as usize)
    }

    /// All tiles with their coordinates, in storage order.
    pub fn iter(&self) -> impl Iterator<Item = ((i32, i32), &'a Tile)> + '_ {
        let width = self.width;
        self.tiles
            .iter()
            .enumerate()
            .map(move |(i, t)| (tile::coords_from_index(i as u32, width), t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus;

    #[test]
    fn every_shipped_file_parses_fully_and_reencodes_exactly() {
        let files = corpus::files();
        if files.is_empty() {
            return;
        }
        let mut seen = 0;
        for f in &files {
            if f.raw.magic == Magic::Civ3 {
                continue;
            }
            let biq = Biq::from_raw(&f.raw).unwrap_or_else(|e| panic!("{}: {e}", f.name()));
            assert_eq!(biq.unmodelled_bytes(), 0, "{}", f.name());
            assert!(biq.unknown_sections.is_empty(), "{}", f.name());
            assert_eq!(biq.version(), f.version, "{}", f.name());
            let out = biq.to_stream();
            if out != f.raw.data {
                let at = out
                    .iter()
                    .zip(&f.raw.data)
                    .position(|(a, b)| a != b)
                    .unwrap_or(out.len().min(f.raw.data.len()));
                panic!(
                    "{}: re-encoded stream differs at byte {at} ({} -> {} bytes)",
                    f.name(),
                    f.raw.data.len(),
                    out.len()
                );
            }
            seen += 1;
        }
        assert!(seen > 0);
    }

    /// The whole-file round trip: bytes from disk to model and back to the
    /// same bytes, for every scenario in the install (compressed or not, and
    /// including the duplicate copies the corpus loader de-duplicates).
    #[test]
    fn every_shipped_file_round_trips_as_bytes() {
        let mut paths = Vec::new();
        corpus::walk(&corpus::install_root(), &mut paths);
        let (mut plain, mut dcl) = (0, 0);
        for p in paths {
            let Ok(bytes) = std::fs::read(&p) else {
                continue;
            };
            let biq = Biq::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            let out = biq.to_bytes().unwrap();
            assert!(
                out == bytes,
                "{}: {} -> {} bytes",
                p.display(),
                bytes.len(),
                out.len()
            );
            match biq.storage {
                Storage::Plain => plain += 1,
                Storage::Dcl { .. } => dcl += 1,
            }
        }
        eprintln!("{plain} plain and {dcl} DCL files round-tripped byte for byte");
    }

    #[test]
    fn storage_is_remembered_and_can_be_changed() {
        let mut biq = Biq::new(Version::new(12, 8));
        biq.rules.goods.push(Good::default());
        assert_eq!(biq.storage, Storage::Plain);
        let plain = biq.to_bytes().unwrap();
        assert_eq!(plain, biq.to_stream());
        biq.storage = Storage::GAME;
        let packed = biq.to_bytes().unwrap();
        assert_eq!(&packed[..2], [0, 6]);
        let back = Biq::parse(&packed).unwrap();
        assert_eq!(back.storage, Storage::GAME);
        assert_eq!(back.to_stream(), plain);
        assert_eq!(back.to_bytes().unwrap(), packed);
    }

    #[test]
    fn map_view_matches_tile_storage() {
        let files = corpus::files();
        let Some(f) = files.iter().find(|f| f.name().contains("Mesopotamia")) else {
            return;
        };
        let biq = Biq::from_raw(&f.raw).unwrap();
        let map = biq.map_view().expect("Mesopotamia has a map");
        assert_eq!((map.width, map.height), (90, 84));
        assert_eq!(map.iter().count(), 90 / 2 * 84);
        for ((x, y), t) in map.iter() {
            assert!(std::ptr::eq(map.tile(x, y).unwrap(), t));
        }
        assert!(map.tile(1, 0).is_none(), "x + y odd: no such tile");
        assert!(map.tile(-1, 0).is_none() || map.wrap_x);
    }

    #[test]
    fn added_sections_are_written_in_the_games_order() {
        let mut biq = Biq::new(Version::new(12, 8));
        biq.rules.goods.push(Good::default());
        biq.scenario.players.push(Player::default());
        biq.rules.buildings.push(Building::default());
        let out = biq.to_stream();
        let tags: Vec<_> = Raw::parse(&out)
            .unwrap()
            .sections
            .iter()
            .map(|s| s.tag_str())
            .collect();
        assert_eq!(tags, ["VER#", "BLDG", "GOOD", "LEAD"]);
        let again = Biq::parse(&out).unwrap();
        assert_eq!(again.rules.goods.len(), 1);
        assert_eq!(again.to_stream(), out);
    }
}

/// Corrupt input must produce an `Err`, never a panic, a hang or an allocation
/// bomb. Deterministic mutations of real files: every truncation point near a
/// section boundary, single-byte flips, and hostile values in the `u32` count
/// and length words that the format trusts.
#[cfg(test)]
pub(crate) mod fuzz {
    use super::*;
    use crate::corpus;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next() % n.max(1) as u64) as usize
        }
    }

    /// Parse `bytes` and then walk everything a caller would touch, returning
    /// a description if anything panicked.
    fn poke(bytes: &[u8], what: &str) -> Option<String> {
        let (r, largest) = crate::alloc_probe::largest_during(|| {
            catch_unwind(AssertUnwindSafe(|| {
                if let Ok(biq) = Biq::parse(bytes) {
                    let _ = biq.to_stream();
                    let _ = biq.to_bytes();
                    let _ = biq.map_view().map(|m| m.iter().count());
                    let _ = biq.unmodelled_bytes();
                }
            }))
        });
        // The biggest legitimate allocation is a few MB (the tile vector of the
        // largest map); an input of at most a few MB must not ask for hundreds.
        if largest > MAX_ALLOC {
            return Some(format!("{what}: allocated {} MiB at once", largest >> 20));
        }
        r.err().map(|e| {
            let msg = e
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default();
            format!("{what}: {msg}")
        })
    }

    /// Ceiling for one allocation while handling a corrupt file.
    pub(crate) const MAX_ALLOC: usize = 128 << 20;

    #[test]
    fn corrupt_files_error_instead_of_panicking() {
        let files = corpus::files();
        if files.is_empty() {
            return;
        }
        // one file of each generation, the smallest that has a map and one with rules only
        let mut picks: Vec<&corpus::CorpusFile> = Vec::new();
        for major in [4u32, 11, 12] {
            if let Some(f) = files
                .iter()
                .filter(|f| f.version.major == major && f.raw.magic != crate::raw::Magic::Civ3)
                .min_by_key(|f| f.raw.data.len())
            {
                picks.push(f);
            }
        }
        assert!(!picks.is_empty());
        let mut failures = Vec::new();
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        std::panic::set_hook(Box::new(|_| {}));
        let mut cases = 0;
        for f in picks {
            let data = &f.raw.data;
            let name = f.name();
            // truncations: around every section boundary, plus a spread
            let mut cuts: Vec<usize> = Vec::new();
            for s in &f.raw.sections {
                for d in [-5isize, -1, 0, 1, 3, 4, 7, 8, 9, 12] {
                    cuts.push((s.start as isize + d).max(0) as usize);
                }
                for r in s.rows.iter().take(3) {
                    cuts.push(r.start);
                    cuts.push(r.start.saturating_sub(2));
                    cuts.push((r.start + r.end) / 2);
                }
            }
            for _ in 0..200 {
                cuts.push(rng.below(data.len()));
            }
            for cut in cuts {
                cases += 1;
                if let Some(m) = poke(
                    &data[..cut.min(data.len())],
                    &format!("{name} cut at {cut}"),
                ) {
                    failures.push(m);
                }
            }
            // single-byte flips
            for _ in 0..400 {
                let at = rng.below(data.len());
                let mut copy = data.clone();
                copy[at] ^= 1 << rng.below(8);
                cases += 1;
                if let Some(m) = poke(&copy, &format!("{name} flip at {at}")) {
                    failures.push(m);
                }
            }
            // hostile counts and lengths: overwrite the count word of each section,
            // the length word of some rows, and word-aligned spots inside rows
            let hostile = [
                0u32,
                1,
                2,
                0x7FFF_FFFF,
                0x8000_0000,
                0xFFFF_FFFF,
                0x0100_0000,
            ];
            for s in &f.raw.sections {
                let mut spots = vec![s.start + 4, s.start + 8];
                for r in s.rows.iter().take(2) {
                    spots.push(r.start.saturating_sub(4));
                    spots.push(r.start);
                    spots.push(r.start + 4);
                    spots.push(r.start + 8);
                }
                for at in spots {
                    if at + 4 > data.len() {
                        continue;
                    }
                    for &v in &hostile {
                        let mut copy = data.clone();
                        copy[at..at + 4].copy_from_slice(&v.to_le_bytes());
                        cases += 1;
                        if let Some(m) = poke(
                            &copy,
                            &format!("{name} {} word at {at} = {v:#x}", s.tag_str()),
                        ) {
                            failures.push(m);
                        }
                    }
                }
            }
        }
        let _ = std::panic::take_hook();
        eprintln!("{cases} corrupt inputs, {} failures", failures.len());
        failures.sort();
        failures.dedup();
        assert!(
            failures.is_empty(),
            "{} distinct failures, first:\n{}",
            failures.len(),
            failures
                .iter()
                .take(12)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}
