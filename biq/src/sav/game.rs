//! The `GAME` chunk with the arrays that follow it, and the three objects
//! near the end of the stream that belong to the game as a whole: the turn
//! history, the replay log and the network queue.

use super::body::Body;
use super::counts::RuleCounts;
use super::stream::{Rd, put_chunk, put_u32s, want};
use crate::io::{Error, Result, Writer};

/// Body offsets of the `GAME` chunk (`Game` object `0xA52658`; the chunk
/// covers object bytes `+0x1C..+0x36C`, so object offset = body offset + 0x1C).
pub mod game_field {
    /// Flag word; `flags & 0x26000` adds a fifth array to every history record.
    pub const FLAGS: usize = 0x08;
    /// Unit count (`[0xA5268C]`): number of `UNIT` records.
    pub const UNITS: usize = 0x18;
    /// City count: number of city records.
    pub const CITIES: usize = 0x1C;
    /// Colony count (`[0xA52694]`): number of `CLNY` records.
    pub const COLONIES: usize = 0x20;
    /// Turn number (`Game+0x54`; consecutive autosaves differ by exactly 1).
    pub const TURN: usize = 0x38;
    /// Continent count; sizes the first array.
    pub const CONTINENTS: usize = 0x124;
    /// Number of `AIBS` records after `PEER`.
    pub const AIBS: usize = 0x12C;
    /// Number of `VLOC` records after `PEER`.
    pub const VLOC: usize = 0x130;
    /// Number of `RADT` records after `PEER`.
    pub const RADT: usize = 0x134;
    /// Number of `OUTP` records after `PEER`.
    pub const OUTP: usize = 0x138;
    /// Version of the game block (`5`); selects how much follows the arrays.
    pub const BLOCK_VERSION: usize = 0x320;
}

/// The only game-block version modelled (the one the game writes).
const BLOCK_VERSION: u32 = 5;

/// `GAME` chunk, its arrays, and the date objects that follow them.
///
/// Loader `0x538960`, writer `0x538D30`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    /// `GAME` 848. Fields: [`game_field`].
    pub body: Body<848>,
    /// Raw block of one dword per continent: the number of cities on the
    /// continent (incremented where a city is founded, `0x566861`, decremented
    /// where one is destroyed, `0x4AF110`; the sum is the city count in every
    /// corpus save).
    pub cities_per_continent: Vec<u32>,
    /// One dword per `TECH` row. Looks like a bit set of the player slots that
    /// know the advance (**HYPOTHESIS**: slot 0 never set, `510` = slots 1-8
    /// for the starting advances of a nine-player game).
    pub tech_known_by: Vec<u32>,
    /// One dword per `BLDG` row: the index of the city that holds the wonder,
    /// `-1` for none (checked against the `BLDG` flag below in the corpus).
    pub wonder_city: Vec<u32>,
    /// One byte per `BLDG` row: `1` once a wonder has been built.
    pub wonder_built: Vec<u8>,
    /// One dword per `BLDG` row, identical in saves that share rules
    /// (**HYPOTHESIS**: a bit set derived from the rules, cached).
    pub building_mask_a: Vec<u32>,
    /// One dword per `BLDG` row; equals `building_mask_a` except for a few
    /// entries (**HYPOTHESIS**: `a` plus runtime bits).
    pub building_mask_b: Vec<u32>,
    /// One dword per `PRTO` row (bit sets; meaning open).
    pub unit_mask_a: Vec<u32>,
    /// One dword per `PRTO` row (bit sets; meaning open).
    pub unit_mask_b: Vec<u32>,
    /// One dword per `TECH` row (bit sets; meaning open).
    pub tech_mask: Vec<u32>,
    /// `DATE` 84. Body offset `0x4C` holds the game year as an `i32`
    /// (`-4000` = 4000 BC), see [`Game::year`]; offset `0x50` is always `1`;
    /// the first 19 dwords are uninitialised heap in most saves.
    pub date: Body<84>,
    /// `PLGI` 4 (a plug-in/mod record; all zero in the corpus).
    pub plgi_head: Body<4>,
    /// `PLGI` 8 (all zero in the corpus).
    pub plgi_body: Body<8>,
    /// `DATE` 84: the start date (`-4000` in every save of the corpus).
    pub date_start: Body<84>,
    /// `DATE` 84: a second copy of the current year in the saves of one
    /// scenario family (RUS), `-4000` in the others.
    pub date_other: Body<84>,
    /// Raw dword after the dates (`Game+0x36C`; `500` in the RUS family, `0`
    /// elsewhere in the corpus).
    pub tail_a: u32,
    /// Raw dword after that (`Game+0x4E8`; `2` in the corpus).
    pub tail_b: u32,
}

impl Game {
    /// Current turn number ([`game_field::TURN`]).
    pub fn turn(&self) -> u32 {
        self.body.u32(game_field::TURN)
    }

    /// The year held by the `GAME` date object (`-4000` = 4000 BC). It is
    /// not the current year: it equals the year of the *previous* history
    /// record in the TETURKAN, RUS and EGYPT saves, and a turn-0 save holds
    /// the default (`-4000`) or a stale value. Use [`super::Save::year`].
    pub fn year(&self) -> i32 {
        self.date.i32(76)
    }

    /// Flag word ([`game_field::FLAGS`]).
    pub fn flags(&self) -> u32 {
        self.body.u32(game_field::FLAGS)
    }

    /// Whether history records carry a fifth array.
    pub fn history_has_fifth_array(&self) -> bool {
        self.flags() & 0x26000 != 0
    }

    pub(super) fn read(rd: &mut Rd, c: &RuleCounts) -> Result<Game> {
        let body = rd.chunk::<848>(b"GAME")?;
        let block = body.u32(game_field::BLOCK_VERSION);
        if block != BLOCK_VERSION {
            return Err(Error::Unsupported {
                what: "game block version",
                value: block,
            });
        }
        // Each array is read only when its length is non-zero.
        Ok(Game {
            cities_per_continent: rd.u32s(
                body.u32(game_field::CONTINENTS) as usize,
                "per-continent city counts",
            )?,
            tech_known_by: rd.u32s(c.techs, "tech array")?,
            wonder_city: rd.u32s(c.buildings, "wonder city array")?,
            wonder_built: rd.bytes(c.buildings, "wonder flag array")?,
            building_mask_a: rd.u32s(c.buildings, "building mask a")?,
            building_mask_b: rd.u32s(c.buildings, "building mask b")?,
            unit_mask_a: rd.u32s(c.unit_types, "unit mask a")?,
            unit_mask_b: rd.u32s(c.unit_types, "unit mask b")?,
            tech_mask: rd.u32s(c.techs, "tech mask")?,
            date: rd.chunk(b"DATE")?,
            plgi_head: rd.chunk(b"PLGI")?,
            plgi_body: rd.chunk(b"PLGI")?,
            date_start: rd.chunk(b"DATE")?,
            date_other: rd.chunk(b"DATE")?,
            tail_a: rd.u32("game tail")?,
            tail_b: rd.u32("game tail")?,
            body,
        })
    }

    pub(super) fn write(&self, w: &mut Writer, c: &RuleCounts) -> Result<()> {
        want(
            self.cities_per_continent.len(),
            self.body.u32(game_field::CONTINENTS) as usize,
            "per-continent city counts must match the continent count",
        )?;
        want(self.tech_known_by.len(), c.techs, "tech array length")?;
        want(self.wonder_city.len(), c.buildings, "wonder city length")?;
        want(self.wonder_built.len(), c.buildings, "wonder flag length")?;
        want(
            self.building_mask_a.len(),
            c.buildings,
            "building mask length",
        )?;
        want(
            self.building_mask_b.len(),
            c.buildings,
            "building mask length",
        )?;
        want(self.unit_mask_a.len(), c.unit_types, "unit mask length")?;
        want(self.unit_mask_b.len(), c.unit_types, "unit mask length")?;
        want(self.tech_mask.len(), c.techs, "tech mask length")?;
        want(
            self.body.u32(game_field::BLOCK_VERSION) as usize,
            BLOCK_VERSION as usize,
            "game block version must be 5",
        )?;
        put_chunk(w, b"GAME", &self.body);
        put_u32s(w, &self.cities_per_continent);
        put_u32s(w, &self.tech_known_by);
        put_u32s(w, &self.wonder_city);
        w.bytes(&self.wonder_built);
        put_u32s(w, &self.building_mask_a);
        put_u32s(w, &self.building_mask_b);
        put_u32s(w, &self.unit_mask_a);
        put_u32s(w, &self.unit_mask_b);
        put_u32s(w, &self.tech_mask);
        put_chunk(w, b"DATE", &self.date);
        put_chunk(w, b"PLGI", &self.plgi_head);
        put_chunk(w, b"PLGI", &self.plgi_body);
        put_chunk(w, b"DATE", &self.date_start);
        put_chunk(w, b"DATE", &self.date_other);
        w.u32(self.tail_a);
        w.u32(self.tail_b);
        Ok(())
    }
}

/// The turn history (`HIST`, loader `0x542450` on object `0xB38C60`): one
/// record per turn from turn 0 to the saved turn.
///
/// Not framed as chunks: a four-byte tag the loader never reads, two dwords,
/// then the records.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct History {
    /// The four tag bytes (`HIST` in the corpus; skipped by the loader).
    pub tag: [u8; 4],
    /// Dword at `+8` of the block: a bit set of the player slots the records
    /// cover, slots `1..=m` (`2^(m+1) - 2`, e.g. `0x1FE` for eight players, in
    /// every save of the corpus).
    pub x: u32,
    /// The records.
    pub records: Vec<HistoryRecord>,
}

/// One history record: the turn, its year and four (five) per-player arrays.
///
/// `m` is the number of players the game tracks (slots `1..=m`, not the
/// barbarians): `18` in `yolo`, `31` in the TETURKAN family.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryRecord {
    /// Turn number; the records of a save are `0..=turn` in order.
    pub a: u32,
    /// The year of that turn as an `i32` (`-4000` = 4000 BC). The last
    /// record's year is the save's current year, [`super::Save::year`].
    pub b: u32,
    /// The four arrays, each of the record's `m` entries, indexed by player
    /// slot minus one. `series[0]` is the slot number itself (`1..=m`);
    /// `series[2]` and `series[3]` never decrease from one record to the
    /// next (cumulative counters); `series[1]` does (a current value). What
    /// they count is open (**HYPOTHESIS**: `series[3]` is accumulated culture,
    /// it grows by one per palace and turn in `yolo`).
    pub series: [Vec<u32>; 4],
    /// The fifth array, present when [`Game::history_has_fifth_array`].
    pub fifth: Option<Vec<u32>>,
}

impl HistoryRecord {
    /// Entries per array (`m`).
    pub fn len(&self) -> usize {
        self.series[0].len()
    }

    /// Whether the arrays are empty.
    pub fn is_empty(&self) -> bool {
        self.series[0].is_empty()
    }
}

impl History {
    pub(super) fn read(rd: &mut Rd, fifth: bool) -> Result<History> {
        let tag = rd.take(4, "HIST tag")?;
        let tag = [tag[0], tag[1], tag[2], tag[3]];
        let n = rd.u32("HIST record count")? as usize;
        let x = rd.u32("HIST header")?;
        // A record is at least three dwords.
        if n > rd.remaining() / 12 {
            return Err(Error::Truncated {
                offset: rd.pos(),
                what: "HIST records",
            });
        }
        let mut records = Vec::with_capacity(n);
        for _ in 0..n {
            let a = rd.u32("HIST record")?;
            let b = rd.u32("HIST record")?;
            let m = rd.u32("HIST record")? as usize;
            let mut next = |what| rd.u32s(m, what);
            let series = [
                next("HIST series")?,
                next("HIST series")?,
                next("HIST series")?,
                next("HIST series")?,
            ];
            let fifth = if fifth {
                Some(next("HIST series")?)
            } else {
                None
            };
            records.push(HistoryRecord {
                a,
                b,
                series,
                fifth,
            });
        }
        Ok(History { tag, x, records })
    }

    pub(super) fn write(&self, w: &mut Writer, fifth: bool) -> Result<()> {
        w.bytes(&self.tag);
        w.u32(self.records.len() as u32);
        w.u32(self.x);
        for r in &self.records {
            let m = r.len();
            for s in &r.series {
                want(
                    s.len(),
                    m,
                    "history arrays of a record must have equal length",
                )?;
            }
            want(
                usize::from(r.fifth.is_some()),
                usize::from(fifth),
                "history fifth array must match the game flags",
            )?;
            if let Some(f) = &r.fifth {
                want(
                    f.len(),
                    m,
                    "history arrays of a record must have equal length",
                )?;
            }
            w.u32(r.a);
            w.u32(r.b);
            w.u32(m as u32);
            for s in &r.series {
                put_u32s(w, s);
            }
            if let Some(f) = &r.fifth {
                put_u32s(w, f);
            }
        }
        Ok(())
    }
}

/// The replay log (`RPLS`, object `0xC88588`, loader `0x58B400`): turns of
/// events, kept for the end-of-game replay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Replay {
    /// Raw dword tag (`RPLS`, ignored by the loader).
    pub tag: u32,
    /// One record per turn.
    pub turns: Vec<ReplayTurn>,
}

/// One replay turn (object `0x34`, loader `0x58AE50`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayTurn {
    /// `RPLT` 5.
    pub head: Body<5>,
    /// The turn's events.
    pub events: Vec<ReplayEvent>,
}

/// One replay event (object `0x28`, loader `0x58AD10`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayEvent {
    /// `RPLE` 10.
    pub head: Body<10>,
    /// The event's text as stored after the chunk: a NUL-terminated string
    /// (bytes without the NUL; a lone NUL byte is the empty string).
    pub text: Vec<u8>,
}

impl Replay {
    pub(super) fn read(rd: &mut Rd) -> Result<Replay> {
        let tag = rd.u32("RPLS tag")?;
        let n = rd.u32("RPLS turn count")? as usize;
        // A turn is a 13-byte chunk plus a count dword.
        if n > rd.remaining() / 17 {
            return Err(Error::Truncated {
                offset: rd.pos(),
                what: "replay turns",
            });
        }
        let mut turns = Vec::with_capacity(n);
        for _ in 0..n {
            let head = rd.chunk::<5>(b"RPLT")?;
            let k = rd.u32("RPLT event count")? as usize;
            if k > rd.remaining() / 19 {
                return Err(Error::Truncated {
                    offset: rd.pos(),
                    what: "replay events",
                });
            }
            let mut events = Vec::with_capacity(k);
            for _ in 0..k {
                let head = rd.chunk::<10>(b"RPLE")?;
                let rest = rd.rest();
                let end = rest.iter().position(|&b| b == 0).ok_or(Error::Truncated {
                    offset: rd.pos(),
                    what: "replay event text",
                })?;
                let text = rd.take(end + 1, "replay event text")?[..end].to_vec();
                events.push(ReplayEvent { head, text });
            }
            turns.push(ReplayTurn { head, events });
        }
        Ok(Replay { tag, turns })
    }

    pub(super) fn write(&self, w: &mut Writer) -> Result<()> {
        w.u32(self.tag);
        w.u32(self.turns.len() as u32);
        for t in &self.turns {
            put_chunk(w, b"RPLT", &t.head);
            w.u32(t.events.len() as u32);
            for e in &t.events {
                if e.text.contains(&0) {
                    return Err(Error::Inconsistent("replay text must not contain NUL"));
                }
                put_chunk(w, b"RPLE", &e.head);
                w.bytes(&e.text);
                w.u8(0);
            }
        }
        Ok(())
    }
}
