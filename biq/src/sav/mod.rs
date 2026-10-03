//! Civilization III Conquests saved games (`.SAV`): reader and writer.
//!
//! A save is the **whole live game state, dumped object by object**: the
//! embedded scenario (BIQ), the `Game` object, the console, the map, 32
//! players, every unit and city, the turn history, the replay log and some
//! network state. [`Save::parse`] decodes the container (DCL when the file is
//! compressed), the header and that stream into the typed tree below;
//! [`Save::to_bytes`] writes it back **byte for byte** (compressor included,
//! [`crate::implode`]) for every save in the corpus.
//!
//! The format is documented in `reverse-engineering/savegame.md` with the exe
//! address of every loader; the stream grammar there was recovered by running
//! the game's own `game_data(load)` routine over each save in an emulator and
//! recording which bytes every instruction consumed.
//!
//! # What is modelled
//!
//! * **Structure**: complete for format version 24 with sub-versions 2..=10
//!   (the game's own `Conquests` saves are 24 / 10; sub-versions below 10 are
//!   implemented from the loader's gates and not exercised by any file).
//!   Everything the loader reads is in the tree, with the exact chunk sizes
//!   and array lengths it expects.
//! * **Fields**: chunks are [`Body`] byte arrays with accessors for the
//!   fields that are decoded ([`Tile`], [`Unit`], [`City`], [`Map`], [`Game`],
//!   [`Player`]). The rest are open; they are copies of live object memory
//!   and contain padding and stale heap bytes, so compare through accessors.
//! * **Counts are data**: a count the stream keeps inside a body (units in
//!   `GAME`, citizens in `POPD`, ...) is the length of the matching list.
//!   [`Save::to_stream`] returns [`Error::Inconsistent`] instead of writing a
//!   save whose lists and counts disagree.
//!
//! Not supported (an [`Error::Unsupported`] or a framing error): other format
//! versions, sub-version 0 and 1 (the game itself rejects 1), a zero
//! continent count, game-block versions other than 5, and the loader's
//! corrupt-save repair (a negative per-player counter at `Player+0x18C`).

mod body;
mod counts;
mod game;
mod objects;
mod stream;
mod world;

pub use body::Body;
pub use counts::RuleCounts;
pub use game::{Game, History, HistoryRecord, Replay, ReplayEvent, ReplayTurn, game_field};
pub use objects::{
    City, CityTail, NetQueue, Overlays, PLAYER_SLOTS, Player, PlayerTables, Unit, UnitIds,
    city_field, city_name_field, lead_field, unit_field,
};
pub use stream::Rec12;
pub use world::{Map, Tile};

use crate::Biq;
use crate::dcl::Storage;
use crate::io::{Error, Result, Writer};
use crate::raw::{Magic, Raw};
use stream::{Rd, put_chunk, put_chunks, want};

/// The format version the game writes (`[0xA32BC4]`).
pub const VERSION: u32 = 24;

/// The newest sub-version (`[0xA32BC8]`), written by every shipped save.
pub const SUB_VERSION: u32 = 10;

/// The header in front of the `game_data` stream (loader `0x592100`).
///
/// ```text
/// "CIV3" 00   magic, a NUL-terminated string (strcpy'd by the loader)
/// 1A          one byte the loader skips (DOS end-of-file marker)
/// u32         version     (>= 14 accepted; this crate reads 24)
/// u32         sub-version (read when version >= 17; 1 is rejected, 0 means none)
/// [16]        GUID, only when sub-version >= 7 (older saves get a fresh one)
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    /// Byte after the magic's NUL (`0x1A`).
    pub marker: u8,
    /// Format version (`24`).
    pub version: u32,
    /// Sub-version (`10`).
    pub sub_version: u32,
    /// Game GUID, present when `sub_version >= 7`.
    pub guid: Option<[u8; 16]>,
}

impl Header {
    /// Size of the header in the decoded stream.
    pub fn len(&self) -> usize {
        6 + 8 + if self.guid.is_some() { 16 } else { 0 }
    }

    /// A header is never empty; provided for clippy's `len` convention.
    pub fn is_empty(&self) -> bool {
        false
    }
}

/// A parsed saved game.
#[derive(Clone, Debug)]
pub struct Save {
    /// How the file was stored on disk; [`Save::to_bytes`] writes it the same
    /// way.
    pub storage: Storage,
    /// The stream header.
    pub header: Header,
    /// The rule counts that size the raw arrays. [`Save::parse`] derives them
    /// from [`Save::biq`]; keep them in step if the embedded BIQ is replaced.
    pub counts: RuleCounts,
    /// `BIC ` 524: dword 0 is the length of [`Save::biq`], then two 260-byte
    /// path buffers ([`Save::scenario_dir`], [`Save::scenario_file`]).
    pub bic: Body<524>,
    /// The scenario the game was started from, embedded verbatim: a `BICQ`
    /// stream readable with [`Biq::parse`] ([`Save::embedded_biq`]).
    pub biq: Vec<u8>,
    /// `GAME` and its arrays.
    pub game: Game,
    /// `CNSL` 228 (the console / message-log object `0x9F8C74`).
    pub console: Body<228>,
    /// The map.
    pub map: Map,
    /// The 32 player slots ([`PLAYER_SLOTS`]).
    pub players: Vec<Player>,
    /// Units, in id order.
    pub units: Vec<Unit>,
    /// Cities, in id order.
    pub cities: Vec<City>,
    /// `CLNY` 16 records (colonies; none in the corpus).
    pub colonies: Vec<Body<16>>,
    /// Raw block copied to `0xA94B20`: 256 bytes, plus 8 more for sub-version < 4.
    pub reserved: Vec<u8>,
    /// 32 `PALV` 148 chunks (objects `0xB71288 + k * 0xB0`).
    pub palv: Vec<Body<148>>,
    /// Turn history.
    pub history: History,
    /// `TUTR` 92 (tutorial state, object `0xC9C440`).
    pub tutorial: Body<92>,
    /// `FAXX` 88 (object `0xA46F38`).
    pub faxx: Body<88>,
    /// Replay log.
    pub replay: Replay,
    /// Network queue (`FNetQueue`).
    pub net_queue: NetQueue,
    /// `PEER` 24 (object `0x74AF60`).
    pub peer: Body<24>,
    /// The object pools that close the stream.
    pub overlays: Overlays,
}

/// Refuse a record count the remaining data cannot hold (`min_each` is a lower
/// bound on one record's bytes) before anything is reserved for it.
fn bounded(rd: &Rd, n: u32, min_each: usize, what: &'static str) -> Result<usize> {
    let n = n as usize;
    if n > rd.remaining() / min_each {
        return Err(Error::Truncated {
            offset: rd.pos(),
            what,
        });
    }
    Ok(n)
}

fn c_string(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    bytes[..end].iter().map(|&b| b as char).collect()
}

impl Save {
    /// Parse a `.SAV` file's bytes (DCL-compressed or plain), sizing the raw
    /// arrays from the embedded scenario.
    pub fn parse(input: &[u8]) -> Result<Save> {
        Save::parse_with(input, None)
    }

    /// Like [`Save::parse`], with the rule counts given instead of derived
    /// (for a game whose default rules are not the shipped `conquests.biq`).
    pub fn parse_with(input: &[u8], counts: Option<RuleCounts>) -> Result<Save> {
        let raw = Raw::parse(input)?;
        if raw.magic != Magic::Civ3 {
            return Err(Error::BadMagic(raw.magic.bytes()));
        }
        Save::from_stream(&raw.data, raw.storage, counts)
    }

    /// Read and parse a file.
    pub fn read_file(path: impl AsRef<std::path::Path>) -> Result<Save> {
        let bytes = std::fs::read(path.as_ref()).map_err(|e| Error::Io(e.to_string()))?;
        Save::parse(&bytes)
    }

    /// Parse an already decoded stream (starting with the `CIV3` magic).
    pub fn from_stream(data: &[u8], storage: Storage, counts: Option<RuleCounts>) -> Result<Save> {
        let mut rd = Rd::new(data);
        let magic = rd.take(6, "save magic")?;
        if &magic[..4] != b"CIV3" || magic[4] != 0 {
            return Err(Error::BadMagic([magic[0], magic[1], magic[2], magic[3]]));
        }
        let marker = magic[5];
        let version = rd.u32("save version")?;
        if version != VERSION {
            return Err(Error::Unsupported {
                what: "save format version",
                value: version,
            });
        }
        let sub_version = rd.u32("save sub-version")?;
        if !(2..=SUB_VERSION).contains(&sub_version) {
            return Err(Error::Unsupported {
                what: "save sub-version",
                value: sub_version,
            });
        }
        let sub = sub_version;
        let guid = if sub >= 7 {
            let mut g = [0u8; 16];
            g.copy_from_slice(rd.take(16, "save GUID")?);
            Some(g)
        } else {
            None
        };
        let header = Header {
            marker,
            version,
            sub_version,
            guid,
        };

        let bic = rd.chunk::<524>(b"BIC ")?;
        let biq = rd.bytes(bic.u32(0) as usize, "embedded scenario")?;
        let counts = match counts {
            Some(c) => c,
            None if biq.is_empty() => RuleCounts::CONQUESTS,
            None => RuleCounts::from_biq(&Biq::parse(&biq)?),
        };

        let game = Game::read(&mut rd, &counts)?;
        let console = rd.chunk(b"CNSL")?;
        // Sub-version 9 moved the map in front of the players (`0x590186`).
        let mut map = if sub >= 9 {
            Some(Map::read(&mut rd, sub, &counts)?)
        } else {
            None
        };
        let players = (0..PLAYER_SLOTS)
            .map(|_| Player::read(&mut rd, sub, &counts))
            .collect::<Result<Vec<_>>>()?;
        if map.is_none() {
            map = Some(Map::read(&mut rd, sub, &counts)?);
        }
        let map = map.expect("map read above");

        let n = bounded(&rd, game.body.u32(game_field::UNITS), 8 + 472, "unit list")?;
        let units = (0..n)
            .map(|_| Unit::read(&mut rd))
            .collect::<Result<Vec<_>>>()?;
        let n = bounded(&rd, game.body.u32(game_field::CITIES), 600, "city list")?;
        let cities = (0..n)
            .map(|_| City::read(&mut rd, sub, &counts))
            .collect::<Result<Vec<_>>>()?;
        let n = bounded(
            &rd,
            game.body.u32(game_field::COLONIES),
            8 + 16,
            "colony list",
        )?;
        let colonies = rd.chunks(n, b"CLNY")?;

        let reserved = rd.bytes(if sub < 4 { 256 + 8 } else { 256 }, "reserved block")?;
        let palv = rd.chunks(32, b"PALV")?;
        let history = History::read(&mut rd, game.history_has_fifth_array())?;
        let tutorial = rd.chunk(b"TUTR")?;
        let faxx = rd.chunk(b"FAXX")?;
        let replay = Replay::read(&mut rd)?;
        let net_queue = objects::read_net_queue(&mut rd)?;
        let peer = rd.chunk(b"PEER")?;
        let overlays = Overlays {
            aibs: {
                let n = bounded(&rd, game.body.u32(game_field::AIBS), 8 + 20, "AIBS list")?;
                rd.chunks(n, b"AIBS")?
            },
            vloc: {
                let n = bounded(&rd, game.body.u32(game_field::VLOC), 8 + 16, "VLOC list")?;
                rd.chunks(n, b"VLOC")?
            },
            radt: {
                let n = bounded(&rd, game.body.u32(game_field::RADT), 8 + 16, "RADT list")?;
                rd.chunks(n, b"RADT")?
            },
            outp: {
                let n = bounded(&rd, game.body.u32(game_field::OUTP), 8 + 16, "OUTP list")?;
                rd.chunks(n, b"OUTP")?
            },
        };
        // The loader stops here; whatever follows is ignored by the game. A
        // save with trailing bytes would not round-trip, so refuse it.
        if !rd.at_end() {
            return Err(Error::Unsupported {
                what: "bytes after the end of the saved game",
                value: rd.remaining() as u32,
            });
        }

        Ok(Save {
            storage,
            header,
            counts,
            bic,
            biq,
            game,
            console,
            map,
            players,
            units,
            cities,
            colonies,
            reserved,
            palv,
            history,
            tutorial,
            faxx,
            replay,
            net_queue,
            peer,
            overlays,
        })
    }

    /// The current game year (`-4000` = 4000 BC): the year of the newest
    /// history record, which is the year a mid-game autosave is named after.
    /// [`Game::year`] lags one turn behind it.
    pub fn year(&self) -> i32 {
        self.history
            .records
            .last()
            .map_or_else(|| self.game.year(), |r| r.b as i32)
    }

    /// The embedded scenario as a [`Biq`].
    pub fn embedded_biq(&self) -> Result<Biq> {
        Biq::parse(&self.biq)
    }

    /// Scenario directory the game recorded (`BIC ` bytes `+4..+264`), empty
    /// for random-map games and most scenarios.
    pub fn scenario_dir(&self) -> String {
        c_string(&self.bic.0[4..264])
    }

    /// Scenario file the game recorded (`BIC ` bytes `+264..+524`).
    pub fn scenario_file(&self) -> String {
        c_string(&self.bic.0[264..524])
    }

    /// This save laid out for another sub-version: the pieces a sub-version
    /// gates are added (zero-filled) or dropped. The result is what the
    /// loader expects of a save of that sub-version, **not** something the
    /// game wrote; it exists to feed the game's own loader in the emulator
    /// and check the gates (`savegame.md`, "How the grammar was verified"),
    /// and to test the gates here.
    pub fn with_sub_version(&self, sub: u32) -> Result<Save> {
        if !(2..=SUB_VERSION).contains(&sub) {
            return Err(Error::Unsupported {
                what: "save sub-version",
                value: sub,
            });
        }
        let mut s = self.clone();
        s.header.sub_version = sub;
        s.header.guid = (sub >= 7).then(|| self.header.guid.unwrap_or([0; 16]));
        for p in &mut s.players {
            let none = || vec![Vec::new(); 32];
            p.list_a = (sub > 2).then(|| p.list_a.take().unwrap_or_else(none));
            p.list_b = (sub >= 6).then(|| p.list_b.take().unwrap_or_else(none));
        }
        for c in &mut s.cities {
            c.date = (sub >= 4).then(|| c.date.take().unwrap_or_default());
        }
        s.reserved.resize(if sub < 4 { 256 + 8 } else { 256 }, 0);
        for t in &mut s.map.tiles {
            let old = t.legacy_12.take();
            t.legacy_12 =
                Tile::has_legacy_12(&t.cell_04, sub).then(|| old.unwrap_or(Body([0xEE; 12])));
        }
        Ok(s)
    }

    /// Write the decoded stream.
    pub fn to_stream(&self) -> Result<Vec<u8>> {
        let h = &self.header;
        let sub = h.sub_version;
        if h.version != VERSION || !(2..=SUB_VERSION).contains(&sub) {
            return Err(Error::Unsupported {
                what: "save sub-version",
                value: sub,
            });
        }
        want(
            usize::from(h.guid.is_some()),
            usize::from(sub >= 7),
            "the GUID is present exactly for sub-version >= 7",
        )?;
        let g = &self.game.body;
        want(
            self.units.len(),
            g.u32(game_field::UNITS) as usize,
            "unit count in GAME must match the unit list",
        )?;
        want(
            self.cities.len(),
            g.u32(game_field::CITIES) as usize,
            "city count in GAME must match the city list",
        )?;
        want(
            self.colonies.len(),
            g.u32(game_field::COLONIES) as usize,
            "colony count in GAME must match the colony list",
        )?;
        want(
            self.overlays.aibs.len(),
            g.u32(game_field::AIBS) as usize,
            "AIBS count in GAME must match the list",
        )?;
        want(
            self.overlays.vloc.len(),
            g.u32(game_field::VLOC) as usize,
            "VLOC count in GAME must match the list",
        )?;
        want(
            self.overlays.radt.len(),
            g.u32(game_field::RADT) as usize,
            "RADT count in GAME must match the list",
        )?;
        want(
            self.overlays.outp.len(),
            g.u32(game_field::OUTP) as usize,
            "OUTP count in GAME must match the list",
        )?;
        want(
            self.bic.u32(0) as usize,
            self.biq.len(),
            "BIC length word must match the embedded scenario",
        )?;
        want(self.players.len(), PLAYER_SLOTS, "a save has 32 players")?;
        want(self.palv.len(), 32, "a save has 32 PALV chunks")?;
        want(
            self.reserved.len(),
            if sub < 4 { 256 + 8 } else { 256 },
            "reserved block length",
        )?;

        let c = &self.counts;
        let mut w = Writer::new();
        w.bytes(b"CIV3");
        w.u8(0);
        w.u8(h.marker);
        w.u32(h.version);
        w.u32(h.sub_version);
        if let Some(guid) = &h.guid {
            w.bytes(guid);
        }
        put_chunk(&mut w, b"BIC ", &self.bic);
        w.bytes(&self.biq);
        self.game.write(&mut w, c)?;
        put_chunk(&mut w, b"CNSL", &self.console);
        if sub >= 9 {
            self.map.write(&mut w, sub, c)?;
        }
        for p in &self.players {
            p.write(&mut w, sub, c)?;
        }
        if sub < 9 {
            self.map.write(&mut w, sub, c)?;
        }
        for u in &self.units {
            u.write(&mut w)?;
        }
        for city in &self.cities {
            city.write(&mut w, sub, c)?;
        }
        put_chunks(&mut w, b"CLNY", &self.colonies);
        w.bytes(&self.reserved);
        put_chunks(&mut w, b"PALV", &self.palv);
        self.history
            .write(&mut w, self.game.history_has_fifth_array())?;
        put_chunk(&mut w, b"TUTR", &self.tutorial);
        put_chunk(&mut w, b"FAXX", &self.faxx);
        self.replay.write(&mut w)?;
        objects::put_net_queue(&mut w, &self.net_queue);
        put_chunk(&mut w, b"PEER", &self.peer);
        put_chunks(&mut w, b"AIBS", &self.overlays.aibs);
        put_chunks(&mut w, b"VLOC", &self.overlays.vloc);
        put_chunks(&mut w, b"RADT", &self.overlays.radt);
        put_chunks(&mut w, b"OUTP", &self.overlays.outp);
        Ok(w.buf)
    }

    /// The file's bytes: [`Save::to_stream`], compressed the way the save
    /// arrived ([`Save::storage`]).
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        Ok(self.storage.encode(&self.to_stream()?)?)
    }

    /// Write the file.
    pub fn write_file(&self, path: impl AsRef<std::path::Path>) -> Result<()> {
        std::fs::write(path.as_ref(), self.to_bytes()?).map_err(|e| Error::Io(e.to_string()))
    }
}

#[cfg(test)]
mod tests;
