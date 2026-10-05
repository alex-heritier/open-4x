//! Players, units, cities and the small object lists of a save.

use super::body::Body;
use super::counts::RuleCounts;
use super::stream::{Rd, Rec12, put_chunk, put_chunks, put_lists12, put_u16s, put_u32s, want};
use crate::io::{Error, Result, Writer};

// ---------------------------------------------------------------------------
// Players
// ---------------------------------------------------------------------------

/// Number of player slots in the game (`Player` objects at `0xA52E98`,
/// stride `0x20E4`). Slot 0 is the barbarians.
pub const PLAYER_SLOTS: usize = 32;

/// Body offsets of the `LEAD` chunk (object bytes `+0x1C..+0x15B8`, so object
/// offset = body offset + 0x1C).
pub mod lead_field {
    /// `i32`: the civilization's `RACE` row (object `+0x20`), `-1` for an
    /// unused slot.
    pub const RACE: usize = 0x04;
    /// `i32`: id of the capital city, `-1` for none (object `+0x2C`).
    pub const CAPITAL: usize = 0x10;
    /// `i32`: first share of the treasury (object `+0x44`).
    pub const TREASURY_A: usize = 0x28;
    /// `i32`: second share of the treasury (object `+0x48`); gold is the sum
    /// of the two (`economy.md`).
    pub const TREASURY_B: usize = 0x2C;
    /// `i32`: the government's `GOVT` row (object `+0xA0`).
    pub const GOVERNMENT: usize = 0x84;
    /// Dword; when non-zero, five more arrays of this many dwords follow
    /// the raw blocks (object `+0x19C`).
    pub const ARRAY_LEN: usize = 0x180;
    /// Byte: the slot is in use (object `+0x11B4`). It gates the large raw
    /// blocks; every slot of an autosave of a full game has it set.
    pub const IN_USE: usize = 0x1198;
    /// Dword: player block version (object `+0x15B0`, `4` in the corpus);
    /// sizes the dword tail.
    pub const VERSION: usize = 0x1594;
}

/// Raw blocks stored for a slot that is in use (the part of the `Player`
/// object beyond the `LEAD` range, sized by the rule counts).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PlayerTables {
    /// `BLDG` x `u16` (three tables).
    pub building_u16: [Vec<u16>; 3],
    /// `BLDG` x `u32`.
    pub building_u32: Vec<u32>,
    /// `BLDG` x `u8`.
    pub building_u8: Vec<u8>,
    /// `PRTO` x `u16` (three tables).
    pub unit_u16: [Vec<u16>; 3],
    /// Spaceship parts x `u16` (the `RULE` part count).
    pub space_u16: Vec<u16>,
    /// `GOOD` x 96 bytes: per resource, 32 three-byte supply records, one per
    /// civilization (`Player+0x1614[(good * 32 + civ) * 3]`; the first two
    /// bytes both non-zero mean civ supplies the resource, see
    /// `primitives.md` 4.1).
    pub goods_supply: Vec<u8>,
    /// `GOOD` x `u8`.
    pub goods_u8: Vec<u8>,
}

/// One `Player` object (loader `0x5595E0`, writer `0x559040`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Player {
    /// `LEAD` 5532. Fields: [`lead_field`].
    pub lead: Body<5532>,
    /// 32 lists (`u32 n` + `n` 12-byte items), one per other slot; empty in
    /// the corpus.
    pub lists: Vec<Vec<Rec12>>,
    /// Present exactly when [`Player::in_use`].
    pub tables: Option<PlayerTables>,
    /// Five arrays of [`lead_field::ARRAY_LEN`] dwords (`Player+0x1600..0x1610`);
    /// all empty when that length is zero.
    pub arrays: [Vec<u32>; 5],
    /// `CULT` 16.
    pub culture: Body<16>,
    /// Two `ESPN` 32 chunks.
    pub espionage: [Body<32>; 2],
    /// Dword list (`u32 n` + `n` dwords).
    pub ring: Vec<u32>,
    /// 32 lists of 12-byte items (sub-version >= 3); empty items in the corpus.
    pub list_a: Option<Vec<Vec<Rec12>>>,
    /// 32 lists of 12-byte items (sub-version >= 6); empty items in the corpus.
    pub list_b: Option<Vec<Vec<Rec12>>>,
    /// Dword tail whose length depends on [`Player::version`]: 4, 6, 8, 9
    /// dwords for versions 1, 2, 3, 4 (0 for version 0).
    pub tail: Vec<u32>,
}

impl Player {
    /// Whether the slot is in use ([`lead_field::IN_USE`]).
    pub fn in_use(&self) -> bool {
        self.lead.u8(lead_field::IN_USE) != 0
    }

    /// Player block version ([`lead_field::VERSION`]).
    pub fn version(&self) -> u32 {
        self.lead.u32(lead_field::VERSION)
    }

    /// The civilization's `RACE` row, `-1` when the slot is unused.
    pub fn race(&self) -> i32 {
        self.lead.i32(lead_field::RACE)
    }

    /// Id of the capital city, `-1` for none.
    pub fn capital_city(&self) -> i32 {
        self.lead.i32(lead_field::CAPITAL)
    }

    /// `GOVT` row of the current government.
    pub fn government(&self) -> i32 {
        self.lead.i32(lead_field::GOVERNMENT)
    }

    /// Gold in the treasury: the sum of its two shares.
    pub fn gold(&self) -> i32 {
        self.lead
            .i32(lead_field::TREASURY_A)
            .wrapping_add(self.lead.i32(lead_field::TREASURY_B))
    }

    fn array_len(&self) -> usize {
        self.lead.u32(lead_field::ARRAY_LEN) as usize
    }

    fn tail_len(version: u32) -> usize {
        usize::from(version >= 1) * 4
            + usize::from(version >= 2) * 2
            + usize::from(version >= 3) * 2
            + usize::from(version >= 4)
    }

    pub(super) fn read(rd: &mut Rd, sub: u32, c: &RuleCounts) -> Result<Player> {
        let lead = rd.chunk::<5532>(b"LEAD")?;
        let lists = rd.lists12("player list")?;
        let tables = if lead.u8(lead_field::IN_USE) != 0 {
            let b = c.buildings;
            let p = c.unit_types;
            Some(PlayerTables {
                building_u16: [
                    rd.u16s(b, "player building table")?,
                    rd.u16s(b, "player building table")?,
                    rd.u16s(b, "player building table")?,
                ],
                building_u32: rd.u32s(b, "player building table")?,
                building_u8: rd.bytes(b, "player building table")?,
                unit_u16: [
                    rd.u16s(p, "player unit table")?,
                    rd.u16s(p, "player unit table")?,
                    rd.u16s(p, "player unit table")?,
                ],
                space_u16: rd.u16s(c.space_parts, "player space table")?,
                goods_supply: rd.bytes(
                    c.goods.checked_mul(96).ok_or(Error::Truncated {
                        offset: rd.pos(),
                        what: "player supply table",
                    })?,
                    "player supply table",
                )?,
                goods_u8: rd.bytes(c.goods, "player resource flags")?,
            })
        } else {
            None
        };
        let n = lead.u32(lead_field::ARRAY_LEN) as usize;
        let arrays = [
            rd.u32s(n, "player array")?,
            rd.u32s(n, "player array")?,
            rd.u32s(n, "player array")?,
            rd.u32s(n, "player array")?,
            rd.u32s(n, "player array")?,
        ];
        let culture = rd.chunk(b"CULT")?;
        let espionage = [rd.chunk(b"ESPN")?, rd.chunk(b"ESPN")?];
        let ring_len = rd.u32("player ring length")? as usize;
        let ring = rd.u32s(ring_len, "player ring")?;
        let list_a = if sub > 2 {
            Some(rd.lists12("player list")?)
        } else {
            None
        };
        let list_b = if sub >= 6 {
            Some(rd.lists12("player list")?)
        } else {
            None
        };
        let tail = rd.u32s(
            Player::tail_len(lead.u32(lead_field::VERSION)),
            "player tail",
        )?;
        Ok(Player {
            lead,
            lists,
            tables,
            arrays,
            culture,
            espionage,
            ring,
            list_a,
            list_b,
            tail,
        })
    }

    pub(super) fn write(&self, w: &mut Writer, sub: u32, c: &RuleCounts) -> Result<()> {
        put_chunk(w, b"LEAD", &self.lead);
        put_lists12(w, &self.lists)?;
        match (&self.tables, self.in_use()) {
            (Some(t), true) => {
                let (b, p) = (c.buildings, c.unit_types);
                for v in &t.building_u16 {
                    want(v.len(), b, "player building table length")?;
                }
                want(t.building_u32.len(), b, "player building table length")?;
                want(t.building_u8.len(), b, "player building table length")?;
                for v in &t.unit_u16 {
                    want(v.len(), p, "player unit table length")?;
                }
                want(
                    t.space_u16.len(),
                    c.space_parts,
                    "player space table length",
                )?;
                want(
                    t.goods_supply.len(),
                    c.goods * 96,
                    "player supply table length",
                )?;
                want(t.goods_u8.len(), c.goods, "player resource flag length")?;
                for v in &t.building_u16 {
                    put_u16s(w, v);
                }
                put_u32s(w, &t.building_u32);
                w.bytes(&t.building_u8);
                for v in &t.unit_u16 {
                    put_u16s(w, v);
                }
                put_u16s(w, &t.space_u16);
                w.bytes(&t.goods_supply);
                w.bytes(&t.goods_u8);
            }
            (None, false) => {}
            _ => {
                return Err(Error::Inconsistent(
                    "player tables must be present exactly when the slot is in use",
                ));
            }
        }
        for a in &self.arrays {
            want(a.len(), self.array_len(), "player array length")?;
            put_u32s(w, a);
        }
        put_chunk(w, b"CULT", &self.culture);
        for e in &self.espionage {
            put_chunk(w, b"ESPN", e);
        }
        w.u32(self.ring.len() as u32);
        put_u32s(w, &self.ring);
        match (&self.list_a, sub > 2) {
            (Some(l), true) => put_lists12(w, l)?,
            (None, false) => {}
            _ => return Err(Error::Inconsistent("list_a must match the sub-version")),
        }
        match (&self.list_b, sub >= 6) {
            (Some(l), true) => put_lists12(w, l)?,
            (None, false) => {}
            _ => return Err(Error::Inconsistent("list_b must match the sub-version")),
        }
        want(
            self.tail.len(),
            Player::tail_len(self.version()),
            "player tail length",
        )?;
        put_u32s(w, &self.tail);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Units
// ---------------------------------------------------------------------------

/// Body offsets of the `UNIT` chunk (object bytes `+0x20..+0x1F8`; object
/// offset = body offset + 0x20).
pub mod unit_field {
    /// Dword: the unit's id (object `+0x20`). Records are in ascending id
    /// order; ids of units that no longer exist leave gaps.
    pub const ID: usize = 0x00;
    /// `i32` tile x (object `+0x24`).
    pub const X: usize = 0x04;
    /// `i32` tile y (object `+0x28`).
    pub const Y: usize = 0x08;
    /// `i32` previous tile x (object `+0x2C`; `-1` before the first move).
    pub const PREV_X: usize = 0x0C;
    /// `i32` previous tile y (object `+0x30`).
    pub const PREV_Y: usize = 0x10;
    /// Dword: owning player slot (object `+0x34`).
    pub const OWNER: usize = 0x14;
    /// Dword: `PRTO` row of the unit type (object `+0x40`).
    pub const TYPE: usize = 0x20;
    /// Dword: experience level index, `0..=3` (object `+0x44`).
    pub const EXPERIENCE: usize = 0x24;
    /// Dword: status bits (object `+0x48`; bit 2 is **HYPOTHESIS** "attacked
    /// this turn", `combat.md`).
    pub const STATUS: usize = 0x28;
    /// Dword: damage taken (object `+0x4C`); remaining HP is the type's
    /// maximum minus this.
    pub const DAMAGE: usize = 0x2C;
    /// Dword: movement consumed this turn (object `+0x50`).
    pub const MOVES_SPENT: usize = 0x30;
    /// Dword: current order (object `+0x64`; `1` is the fortified order,
    /// the rest of the enumeration is not decoded).
    pub const ORDER: usize = 0x44;
    /// Dword: unit record version (object `+0x1F0`, `2` in the corpus); `>= 2`
    /// adds the [`super::UnitIds`] record.
    pub const VERSION: usize = 0x1D0;
}

/// The `IDLS` record that follows a version >= 2 unit: a head (`1`, slot
/// count) and that many dwords, each a unit id or `-1`.
///
/// Every unit of the corpus has ten slots; unit ids are the records' indices.
/// The slots look like a carried-unit list (**HYPOTHESIS**: one example has a
/// transport whose first slot names a unit).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitIds {
    /// `IDLS` 8: dword 0 is `1` (version), dword 1 the slot count.
    pub head: Body<8>,
    /// The slots.
    pub ids: Vec<u32>,
}

/// One unit (loader `0x5CD0A0`, object size `0x404`, vtable `0x66DCF0`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unit {
    /// `UNIT` 472. Fields: [`unit_field`].
    pub body: Body<472>,
    /// Present when the record version is at least 2.
    pub ids: Option<UnitIds>,
}

impl Unit {
    /// Id (see [`unit_field::ID`]).
    pub fn id(&self) -> u32 {
        self.body.u32(unit_field::ID)
    }
    /// Tile x.
    pub fn x(&self) -> i32 {
        self.body.i32(unit_field::X)
    }
    /// Tile y.
    pub fn y(&self) -> i32 {
        self.body.i32(unit_field::Y)
    }
    /// Previous tile x.
    pub fn prev_x(&self) -> i32 {
        self.body.i32(unit_field::PREV_X)
    }
    /// Previous tile y.
    pub fn prev_y(&self) -> i32 {
        self.body.i32(unit_field::PREV_Y)
    }
    /// Owning player slot.
    pub fn owner(&self) -> u32 {
        self.body.u32(unit_field::OWNER)
    }
    /// `PRTO` row of the unit type.
    pub fn unit_type(&self) -> u32 {
        self.body.u32(unit_field::TYPE)
    }
    /// Experience level index.
    pub fn experience_level(&self) -> u32 {
        self.body.u32(unit_field::EXPERIENCE)
    }
    /// Damage taken.
    pub fn damage(&self) -> u32 {
        self.body.u32(unit_field::DAMAGE)
    }
    /// Movement consumed this turn.
    pub fn moves_spent(&self) -> u32 {
        self.body.u32(unit_field::MOVES_SPENT)
    }
    /// Current order.
    pub fn order(&self) -> u32 {
        self.body.u32(unit_field::ORDER)
    }
    /// Record version.
    pub fn version(&self) -> u32 {
        self.body.u32(unit_field::VERSION)
    }

    pub(super) fn read(rd: &mut Rd) -> Result<Unit> {
        let body = rd.chunk::<472>(b"UNIT")?;
        let ids = if body.u32(unit_field::VERSION) >= 2 {
            let head = rd.chunk::<8>(b"IDLS")?;
            let n = head.u32(4) as usize;
            Some(UnitIds {
                ids: rd.u32s(n, "unit id list")?,
                head,
            })
        } else {
            None
        };
        Ok(Unit { body, ids })
    }

    pub(super) fn write(&self, w: &mut Writer) -> Result<()> {
        put_chunk(w, b"UNIT", &self.body);
        match (&self.ids, self.version() >= 2) {
            (Some(i), true) => {
                want(
                    i.ids.len(),
                    i.head.u32(4) as usize,
                    "unit id list must match its slot count",
                )?;
                put_chunk(w, b"IDLS", &i.head);
                put_u32s(w, &i.ids);
            }
            (None, false) => {}
            _ => {
                return Err(Error::Inconsistent(
                    "unit id list must be present exactly for record versions >= 2",
                ));
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Cities
// ---------------------------------------------------------------------------

/// Body offsets inside [`City::block_20`] (object bytes `+0x20..+0xA8`; object
/// offset = body offset + 0x20).
pub mod city_field {
    /// Dword: the city's id (object `+0x20`), the value `Cell+0x1A` holds for
    /// its tile. Records are in ascending id order.
    pub const ID: usize = 0x00;
    /// `u16` tile x (object `+0x24`).
    pub const X: usize = 0x04;
    /// `u16` tile y (object `+0x26`).
    pub const Y: usize = 0x06;
    /// Byte: owning player slot (object `+0x28`); the three bytes after it
    /// are uninitialised memory.
    pub const OWNER: usize = 0x08;
}

/// Body offsets inside [`City::block_1e0`] (object bytes `+0x1E0..+0x274`).
pub mod city_name_field {
    /// `char[24]`: the city name, NUL terminated (object `+0x1E0`).
    pub const NAME: usize = 0x00;
    /// Capacity of the name buffer.
    pub const NAME_LEN: usize = 24;
}

/// The trailing records of a city, whose presence depends on the city
/// version (`CITY` 8, object `+0x370`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CityTail {
    /// `CITY` 8: dword 1 is the city record version (`4` in the corpus).
    pub version: Body<8>,
    /// Version >= 2: `CITY` 4 whose dword is the entry count, then that many
    /// `CITY` 4 chunks.
    pub list: Option<(Body<4>, Vec<Body<4>>)>,
    /// Version >= 3: `CTPG` 4 and `CTPG` 16.
    pub ctpg: Option<(Body<4>, Body<16>)>,
    /// Version >= 4: `CITY` 4.
    pub last: Option<Body<4>>,
}

/// One city (loader `0x4BBED0`, object size `0x544`).
///
/// The object is saved as five disjoint ranges plus the citizen list, the
/// per-building array and the improvement bit set. Body offset = object
/// offset - the range start in the field name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct City {
    /// `CITY` 136: object `+0x20..+0xA8`. Fields: [`city_field`].
    pub block_20: Body<136>,
    /// `CITY` 16: object `+0xCC..+0xDC`.
    pub block_cc: Body<16>,
    /// `CITY` 36: object `+0xF4..+0x118`.
    pub block_f4: Body<36>,
    /// `CITY` 164: object `+0x13C..+0x1E0` (`+0x140 + 4 * civ` is the per-civ
    /// culture stake, `capture.md`).
    pub block_13c: Body<164>,
    /// `CITY` 148: object `+0x1E0..+0x274`.
    pub block_1e0: Body<148>,
    /// `POPD` 8: dword 1 is the citizen count, which is the city size
    /// (object `+0x138`).
    pub popd: Body<8>,
    /// One `CTZN` 300 chunk per citizen.
    pub citizens: Vec<Body<300>>,
    /// `BINF` 4.
    pub binf: Body<4>,
    /// Raw block of 12 bytes per `BLDG` row (pc `0x4BCC0F`).
    pub buildings: Vec<u8>,
    /// `BITM` 40.
    pub bitm: Body<40>,
    /// `DATE` 84 (sub-version >= 4).
    pub date: Option<Body<84>>,
    /// The version-dependent trailer.
    pub tail: CityTail,
}

impl City {
    /// Id (see [`city_field::ID`]).
    pub fn id(&self) -> u32 {
        self.block_20.u32(city_field::ID)
    }
    /// Tile x.
    pub fn x(&self) -> u16 {
        self.block_20.u16(city_field::X)
    }
    /// Tile y.
    pub fn y(&self) -> u16 {
        self.block_20.u16(city_field::Y)
    }
    /// Owning player slot.
    pub fn owner(&self) -> u8 {
        self.block_20.u8(city_field::OWNER)
    }
    /// The city name (Windows-1252, up to 23 characters).
    pub fn name(&self) -> String {
        let b = &self.block_1e0.0[city_name_field::NAME..city_name_field::NAME_LEN];
        let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
        b[..end].iter().map(|&c| c as char).collect()
    }
    /// City size (citizen count).
    pub fn size(&self) -> u32 {
        self.popd.u32(4)
    }
    /// City record version.
    pub fn version(&self) -> u32 {
        self.tail.version.u32(4)
    }

    pub(super) fn read(rd: &mut Rd, sub: u32, c: &RuleCounts) -> Result<City> {
        let block_20 = rd.chunk(b"CITY")?;
        let block_cc = rd.chunk(b"CITY")?;
        let block_f4 = rd.chunk(b"CITY")?;
        let block_13c = rd.chunk(b"CITY")?;
        let block_1e0 = rd.chunk(b"CITY")?;
        let popd = rd.chunk::<8>(b"POPD")?;
        let citizens = rd.chunks(popd.u32(4) as usize, b"CTZN")?;
        let binf = rd.chunk(b"BINF")?;
        let per_building = c.buildings.checked_mul(12).ok_or(Error::Truncated {
            offset: rd.pos(),
            what: "city building array",
        })?;
        let buildings = rd.bytes(per_building, "city building array")?;
        let bitm = rd.chunk(b"BITM")?;
        let date = if sub >= 4 {
            Some(rd.chunk(b"DATE")?)
        } else {
            None
        };
        let version = rd.chunk::<8>(b"CITY")?;
        let v = version.u32(4);
        let list = if v >= 2 {
            let head = rd.chunk::<4>(b"CITY")?;
            let items = rd.chunks(head.u32(0) as usize, b"CITY")?;
            Some((head, items))
        } else {
            None
        };
        let ctpg = if v >= 3 {
            Some((rd.chunk(b"CTPG")?, rd.chunk(b"CTPG")?))
        } else {
            None
        };
        let last = if v >= 4 {
            Some(rd.chunk(b"CITY")?)
        } else {
            None
        };
        Ok(City {
            block_20,
            block_cc,
            block_f4,
            block_13c,
            block_1e0,
            popd,
            citizens,
            binf,
            buildings,
            bitm,
            date,
            tail: CityTail {
                version,
                list,
                ctpg,
                last,
            },
        })
    }

    pub(super) fn write(&self, w: &mut Writer, sub: u32, c: &RuleCounts) -> Result<()> {
        want(
            self.citizens.len(),
            self.popd.u32(4) as usize,
            "citizen list must match the POPD count",
        )?;
        want(
            self.buildings.len(),
            c.buildings * 12,
            "city building array length",
        )?;
        put_chunk(w, b"CITY", &self.block_20);
        put_chunk(w, b"CITY", &self.block_cc);
        put_chunk(w, b"CITY", &self.block_f4);
        put_chunk(w, b"CITY", &self.block_13c);
        put_chunk(w, b"CITY", &self.block_1e0);
        put_chunk(w, b"POPD", &self.popd);
        put_chunks(w, b"CTZN", &self.citizens);
        put_chunk(w, b"BINF", &self.binf);
        w.bytes(&self.buildings);
        put_chunk(w, b"BITM", &self.bitm);
        match (&self.date, sub >= 4) {
            (Some(d), true) => put_chunk(w, b"DATE", d),
            (None, false) => {}
            _ => return Err(Error::Inconsistent("city date must match the sub-version")),
        }
        let t = &self.tail;
        let v = self.version();
        put_chunk(w, b"CITY", &t.version);
        match (&t.list, v >= 2) {
            (Some((head, items)), true) => {
                want(
                    items.len(),
                    head.u32(0) as usize,
                    "city list must match its count",
                )?;
                put_chunk(w, b"CITY", head);
                put_chunks(w, b"CITY", items);
            }
            (None, false) => {}
            _ => return Err(Error::Inconsistent("city list must match the city version")),
        }
        match (&t.ctpg, v >= 3) {
            (Some((a, b)), true) => {
                put_chunk(w, b"CTPG", a);
                put_chunk(w, b"CTPG", b);
            }
            (None, false) => {}
            _ => return Err(Error::Inconsistent("CTPG must match the city version")),
        }
        match (&t.last, v >= 4) {
            (Some(l), true) => put_chunk(w, b"CITY", l),
            (None, false) => {}
            _ => return Err(Error::Inconsistent("city tail must match the city version")),
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Small lists
// ---------------------------------------------------------------------------

/// Objects that follow `PEER`, each a single fixed-size chunk. Counts are
/// kept in the `GAME` chunk.
///
/// `AIBS`, `VLOC`, `RADT` and `OUTP` are tile-object pools of Conquests. The
/// names are only inferred from the tags and the tile overlay bits (**HYPOTHESIS**:
/// airfield-/victory-location-/radar-tower-/outpost-style objects; only
/// `VLOC` occurs in the corpus, and `Tile::victory_point_location_id`
/// supports that reading). The first dword of each record is its pool index.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Overlays {
    /// `AIBS` 20 records (never seen; the size is the loader's range size).
    pub aibs: Vec<Body<20>>,
    /// `VLOC` 16 records.
    pub vloc: Vec<Body<16>>,
    /// `RADT` 16 records (never seen).
    pub radt: Vec<Body<16>>,
    /// `OUTP` 16 records (never seen).
    pub outp: Vec<Body<16>>,
}

/// The network queue (`FNetQueue`, object `0x74D0CC`, loader `0x4842E0`): a
/// count, then per message a length and that many bytes.
pub type NetQueue = Vec<Vec<u8>>;

/// Write a `u32`-counted list of length-prefixed byte strings.
pub(super) fn put_net_queue(w: &mut Writer, q: &NetQueue) {
    w.u32(q.len() as u32);
    for m in q {
        w.u32(m.len() as u32);
        w.bytes(m);
    }
}

/// Read a [`NetQueue`].
pub(super) fn read_net_queue(rd: &mut Rd) -> Result<NetQueue> {
    let n = rd.u32("network queue length")? as usize;
    // Every message is at least its length dword.
    if n > rd.remaining() / 4 {
        return Err(Error::Truncated {
            offset: rd.pos(),
            what: "network queue",
        });
    }
    let mut q = Vec::with_capacity(n);
    for _ in 0..n {
        let len = rd.u32("network message length")? as usize;
        q.push(rd.bytes(len, "network message")?);
    }
    Ok(q)
}
