//! `TECH` - civilization advances (the tech tree).
//!
//! **Where**: loader arm `0x594742`, row reader `0x5E85B0`, writer `0x5E8440`,
//! constructor `0x5E83E0`. The game keeps the rows in the table at
//! `[0x9C7320]` (stride `0x74`); memory offset = body offset + 4 (the row
//! starts with its length word). Editor: dialog 143 *Civilization Advances*,
//! apply routine `0x44CB52`.
//!
//! | body | mem | field | evidence |
//! |---|---|---|---|
//! | `0x00` | `0x04` | [`name`](Tech::name) | **A** reader |
//! | `0x20` | `0x24` | [`civilopedia_entry`](Tech::civilopedia_entry) | **A** reader |
//! | `0x40` | `0x44` | [`cost`](Tech::cost) | **B** control 1149; the editor clamps it to `0..=1000` (`0x44CC7C`) |
//! | `0x44` | `0x48` | [`era`](Tech::era) | **B** combo 1147 (item 0 is *None*, stored as `-1`); **A** the loader counts the rows whose era is not `-1` into scenario `+0x870` (`0x5947A8`) |
//! | `0x48` | `0x4C` | [`icon`](Tech::icon) | **C** distinct per advance (Bronze Working 6, Masonry 40, ...); no editor control, the constructor leaves it unset |
//! | `0x4C` | `0x50` | [`tree_x`](Tech::tree_x) | **B** control 1158 |
//! | `0x50` | `0x54` | [`tree_y`](Tech::tree_y) | **B** control 1164 |
//! | `0x54` | `0x58` | [`prerequisites`](Tech::prerequisites) | **B** combos 1152-1155 (`-1` = none) |
//! | `0x64` | `0x68` | [`flags`](Tech::flags), see [`flags`] | **B** the 23 checkboxes, **A** consumers listed there |
//! | `0x68` | `0x6C` | [`flavors`](Tech::flavors) (Conquests rows) | **B** list box 1931 stores `1 << flavor` per selected item (`0x44D10B..0x44D12A`) |
//! | `0x6C` | `0x70` | [`flavor_revision`](Tech::flavor_revision) (Conquests rows) | **A** reader fix-up `0x5E8724..0x5E8765` |
//!
//! Rows are 104 bytes in Civ3 1.x and Play the World files (through `flags`)
//! and 112 bytes in Conquests files. **`flags` comes first and the two
//! Conquests dwords are appended after it** - an earlier reading of this
//! format put them before `flags`, which made every Conquests advance look
//! flag-less (the dword at `0x6C` is `1` in nearly every row).
//!
//! # Flavors and the revision word
//!
//! `flavors` is a bit mask over the `FLAV` rows (bit `i` = flavor `i`), the
//! same representation `BLDG` and `RACE` use. The reader reads the revision
//! word last; when it is `0` (the game's value for rows that do not have it)
//! and any of bits `10..=16` of `flavors` is set it moves those bits down to
//! `0..=6`, then stores `1` (`0x5E872A..0x5E8765`); see
//! [`Tech::effective_flavors`]. Every stock row has `flavors == 0` and
//! revision `1`; rows made with the editor's *Add* button keep whatever the
//! uninitialised constructor left there (`0xCCCCCCCC` or leftover pointers
//! such as `0x4C8780`), which the game treats as "current" because it is not
//! `0`.

use crate::io::{Ctx, Field, Reader, Record, Result, Str, Writer};

/// The `flags` dword (body `+0x64`). The bit for each checkbox is what the
/// editor's apply routine ORs into the row (`0x44CCF1..0x44D099`); every bit
/// is also exactly the set of stock advances one would expect (Writing:
/// diplomats, Sanitation: no flood-plain disease, Satellites: reveal map,
/// Smart Weapons: precision bombing, ...).
pub mod flags {
    /// *Enables Diplomats* (control 1160). Stock: Writing.
    pub const DIPLOMATS: u32 = 1 << 0;
    /// *Enables Irrigation Without Fresh Water* (1161). Stock: Electricity.
    /// Consumer: the irrigation test `0x55F275`.
    pub const IRRIGATION_WITHOUT_FRESH_WATER: u32 = 1 << 1;
    /// *Enables Bridges* (1162). Stock: Engineering.
    pub const BRIDGES: u32 = 1 << 2;
    /// *Disables Diseases From Flood Plains* (1166). Stock: Sanitation.
    pub const DISABLE_FLOOD_PLAIN_DISEASE: u32 = 1 << 3;
    /// *Enables Conscription of Units* (1167). Stock: Nationalism.
    pub const CONSCRIPTION: u32 = 1 << 4;
    /// *Enables Mobilization Levels* (1168). Stock: Nationalism.
    pub const MOBILIZATION: u32 = 1 << 5;
    /// *Enables Recycling* (1169). Stock: Recycling.
    pub const RECYCLING: u32 = 1 << 6;
    /// *Enables Precision Bombing* (1170). Stock: Smart Weapons.
    pub const PRECISION_BOMBING: u32 = 1 << 7;
    /// *Enables Mutual Protection Pacts* (1171). Stock: Nationalism.
    pub const MPP: u32 = 1 << 8;
    /// *Enables Right of Passage Treaties* (1172). Stock: Map Making.
    pub const RIGHT_OF_PASSAGE: u32 = 1 << 9;
    /// *Enables Military Alliances* (1173). Stock: Writing.
    pub const MILITARY_ALLIANCE: u32 = 1 << 10;
    /// *Enables Trade Embargoes* (1174). Stock: Nationalism.
    pub const TRADE_EMBARGO: u32 = 1 << 11;
    /// *Doubles Effect of (Wealth) Improvement* (1175). Stock: Economics.
    pub const DOUBLE_WEALTH: u32 = 1 << 12;
    /// *Enables Trade Over Sea Tiles* (1176). Stock: Astronomy. Consumer:
    /// `0x561AEA`.
    pub const TRADE_OVER_SEA: u32 = 1 << 13;
    /// *Enables Trade Over Ocean Tiles* (1177). Stock: Magnetism, Navigation.
    /// Consumer: `0x561AF8`.
    pub const TRADE_OVER_OCEAN: u32 = 1 << 14;
    /// *Enables Map Trading* (1178). Stock: Navigation.
    pub const MAP_TRADING: u32 = 1 << 15;
    /// *Enables Communication Trading* (1179). Stock: Printing Press.
    pub const COMMUNICATION_TRADING: u32 = 1 << 16;
    /// *Not Required for Era Advancement* (1180). Consumers: `0x5616E1`,
    /// `0x5A86C7`.
    pub const NOT_REQUIRED_FOR_ERA: u32 = 1 << 17;
    /// *Doubles Work Rate (of Workers)* (1181). Stock: Replaceable Parts.
    pub const DOUBLE_WORKER_RATE: u32 = 1 << 18;
    /// *Cannot be Traded* (1182). Consumers: `0x437FA1`, `0x4494DB`. No stock
    /// advance has it.
    pub const CANNOT_BE_TRADED: u32 = 1 << 19;
    /// *Permits Sacrifices* (1183). No stock advance has it.
    pub const PERMITS_SACRIFICES: u32 = 1 << 20;
    /// *Bonus Tech* (1184). Stock: Philosophy. Consumer: `0x561BDE`.
    pub const BONUS_TECH: u32 = 1 << 21;
    /// *Reveal Map* (1185). Stock: Satellites. Consumer: `0x56204A`.
    pub const REVEAL_MAP: u32 = 1 << 22;
}

/// One advance.
#[derive(Clone, Debug, PartialEq)]
pub struct Tech {
    /// Advance name.
    pub name: Str<32>,
    /// Civilopedia key (`TECH_<Name>`).
    pub civilopedia_entry: Str<32>,
    /// Research cost in the game's base units (the editor allows `0..=1000`).
    pub cost: i32,
    /// `ERAS` index, `-1` for none. Rows with `-1` are not counted as tech
    /// slots by the loader.
    pub era: i32,
    /// Icon index in the advance icon sheet.
    pub icon: i32,
    /// Horizontal position in the editor's tech-tree diagram.
    pub tree_x: i32,
    /// Vertical position in the editor's tree diagram.
    pub tree_y: i32,
    /// `TECH` indices of the (up to four) prerequisites, `-1` = none.
    pub prerequisites: [i32; 4],
    /// What the advance enables, see [`flags`].
    pub flags: u32,
    /// Flavor mask: bit `i` set when `FLAV` row `i` applies. Absent
    /// (`0`) in Civ3 1.x / PTW rows.
    pub flavors: u32,
    /// Revision of the flavor representation: `0` = legacy bit positions, see
    /// the module docs. `1` for a row the game has read or written.
    pub flavor_revision: u32,
    /// Bytes after the last modelled field (empty in every shipped file).
    pub extra: Vec<u8>,
}

impl Default for Tech {
    /// A new row as the editor's constructor (`0x5E83E0`) makes it; the
    /// fields it leaves unset are `0`, and the revision is the loader's `1`.
    fn default() -> Self {
        Tech {
            name: Str::default(),
            civilopedia_entry: Str::default(),
            cost: 0,
            era: -1,
            icon: 0,
            tree_x: 0,
            tree_y: 0,
            prerequisites: [-1; 4],
            flags: 0,
            flavors: 0,
            flavor_revision: 1,
            extra: Vec::new(),
        }
    }
}

impl Tech {
    /// Conquests (12.06+) rows carry the flavor mask and revision after `flags`.
    fn has_flavor_fields(ctx: &Ctx) -> bool {
        ctx.version >= crate::Version::new(12, 6)
    }

    /// The flavor mask as the game holds it after loading: rows with revision
    /// `0` that use the pre-release bit positions `10..=16` get them moved to
    /// `0..=6` (`0x5E8729..0x5E8765`).
    pub fn effective_flavors(&self) -> u32 {
        const LEGACY: u32 = 0x1FC00;
        if self.flavor_revision == 0 && self.flavors & LEGACY != 0 {
            (self.flavors & LEGACY) >> 10
        } else {
            self.flavors
        }
    }

    /// Whether flavor `index` (a `FLAV` row, `0..=31`) applies, after the
    /// loader's fix-up.
    pub fn has_flavor(&self, index: u32) -> bool {
        index < 32 && (self.effective_flavors() >> index) & 1 != 0
    }

    /// Whether the advance has all of the [`flags`] bits in `mask`.
    pub fn has_flags(&self, mask: u32) -> bool {
        self.flags & mask == mask
    }

    /// Whether the loader counts this row as a tech slot (`era != -1`,
    /// `0x5947A8`).
    pub fn is_slot(&self) -> bool {
        self.era != -1
    }
}

impl Record for Tech {
    const TAG: [u8; 4] = *b"TECH";

    fn read(r: &mut Reader<'_>, ctx: &Ctx) -> Result<Self> {
        let mut t = Tech::default();
        if r.remaining() >= 32 {
            t.name = Str::read(r);
        }
        if r.remaining() >= 32 {
            t.civilopedia_entry = Str::read(r);
        }
        macro_rules! field {
            ($f:ident, $ty:ty) => {
                if r.remaining() >= 4 {
                    t.$f = <$ty>::read(r);
                }
            };
        }
        field!(cost, i32);
        field!(era, i32);
        field!(icon, i32);
        field!(tree_x, i32);
        field!(tree_y, i32);
        if r.remaining() >= 16 {
            t.prerequisites = <[i32; 4] as Field>::read(r);
        }
        field!(flags, u32);
        if Self::has_flavor_fields(ctx) {
            field!(flavors, u32);
            field!(flavor_revision, u32);
        }
        t.extra = r.rest().to_vec();
        Ok(t)
    }

    fn write(&self, w: &mut Writer, ctx: &Ctx) {
        self.name.write(w);
        self.civilopedia_entry.write(w);
        self.cost.write(w);
        self.era.write(w);
        self.icon.write(w);
        self.tree_x.write(w);
        self.tree_y.write(w);
        self.prerequisites.write(w);
        self.flags.write(w);
        if Self::has_flavor_fields(ctx) {
            self.flavors.write(w);
            self.flavor_revision.write(w);
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
    use crate::Biq;
    use crate::corpus::{self, files};

    fn conquests() -> Option<Biq> {
        let f = files()
            .into_iter()
            .find(|f| f.name().ends_with("Conquests/conquests.biq"))?;
        Some(Biq::from_raw(&f.raw).unwrap())
    }

    fn by_name<'a>(b: &'a Biq, name: &str) -> &'a Tech {
        b.rules
            .techs
            .iter()
            .find(|t| t.name.text() == name)
            .unwrap_or_else(|| panic!("no advance {name}"))
    }

    #[test]
    fn corpus_roundtrip() {
        let st = corpus::check_roundtrip::<Tech>();
        assert_eq!(st.with_extra, 0);
        assert_eq!(st.short, 0);
        let mut expect = vec![104, 112];
        expect.retain(|l| st.lengths.contains(l));
        assert_eq!(st.lengths, expect);
    }

    /// The bit positions are the editor's (`0x44CCF1..0x44D099`), one per
    /// checkbox.
    #[test]
    fn flag_bits_are_the_editors() {
        use flags::*;
        let table: [(u32, u32); 23] = [
            (DIPLOMATS, 0x1),
            (IRRIGATION_WITHOUT_FRESH_WATER, 0x2),
            (BRIDGES, 0x4),
            (DISABLE_FLOOD_PLAIN_DISEASE, 0x8),
            (CONSCRIPTION, 0x10),
            (MOBILIZATION, 0x20),
            (RECYCLING, 0x40),
            (PRECISION_BOMBING, 0x80),
            (MPP, 0x100),
            (RIGHT_OF_PASSAGE, 0x200),
            (MILITARY_ALLIANCE, 0x400),
            (TRADE_EMBARGO, 0x800),
            (DOUBLE_WEALTH, 0x1000),
            (TRADE_OVER_SEA, 0x2000),
            (TRADE_OVER_OCEAN, 0x4000),
            (MAP_TRADING, 0x8000),
            (COMMUNICATION_TRADING, 0x1_0000),
            (NOT_REQUIRED_FOR_ERA, 0x2_0000),
            (DOUBLE_WORKER_RATE, 0x4_0000),
            (CANNOT_BE_TRADED, 0x8_0000),
            (PERMITS_SACRIFICES, 0x10_0000),
            (BONUS_TECH, 0x20_0000),
            (REVEAL_MAP, 0x40_0000),
        ];
        let mut seen = 0u32;
        for (bit, want) in table {
            assert_eq!(bit, want);
            seen |= bit;
        }
        assert_eq!(seen, 0x7F_FFFF);
    }

    /// The stock rules: each flag sits on the advance that gives the effect
    /// (this is what exposed the earlier, wrongly ordered bit table).
    #[test]
    fn stock_advances_carry_the_flags_of_their_effects() {
        let Some(b) = conquests() else {
            return;
        };
        use flags::*;
        let cases: [(&str, u32); 14] = [
            ("Writing", DIPLOMATS | MILITARY_ALLIANCE),
            ("Philosophy", BONUS_TECH),
            ("Smart Weapons", PRECISION_BOMBING),
            ("Satellites", REVEAL_MAP),
            (
                "Sanitation",
                DISABLE_FLOOD_PLAIN_DISEASE | NOT_REQUIRED_FOR_ERA,
            ),
            (
                "Nationalism",
                CONSCRIPTION | MOBILIZATION | MPP | TRADE_EMBARGO | NOT_REQUIRED_FOR_ERA,
            ),
            ("Recycling", RECYCLING),
            ("Replaceable Parts", DOUBLE_WORKER_RATE),
            ("Electricity", IRRIGATION_WITHOUT_FRESH_WATER),
            ("Engineering", BRIDGES),
            ("Astronomy", TRADE_OVER_SEA),
            (
                "Navigation",
                TRADE_OVER_OCEAN | MAP_TRADING | NOT_REQUIRED_FOR_ERA,
            ),
            ("Economics", DOUBLE_WEALTH | NOT_REQUIRED_FOR_ERA),
            (
                "Printing Press",
                COMMUNICATION_TRADING | NOT_REQUIRED_FOR_ERA,
            ),
        ];
        for (name, want) in cases {
            assert_eq!(by_name(&b, name).flags, want, "{name}");
        }
        // Nothing else uses a bit: stock rules leave the two bits the
        // scenario designers set by hand alone.
        let union = b.rules.techs.iter().fold(0, |a, t| a | t.flags);
        assert_eq!(union & (CANNOT_BE_TRADED | PERMITS_SACRIFICES), 0);
        assert_eq!(union & !0x7F_FFFF, 0);
        // Bronze Working has no flags at all (the dword at 0x6C is the
        // revision, 1).
        let bw = by_name(&b, "Bronze Working");
        assert_eq!((bw.cost, bw.era, bw.flags), (3, 0, 0));
        assert_eq!((bw.flavors, bw.flavor_revision), (0, 1));
        assert_eq!(bw.icon, 6);
        assert_eq!((bw.tree_x, bw.tree_y), (82, 72));
    }

    #[test]
    fn currency_cost_and_prerequisites() {
        let Some(b) = conquests() else {
            return;
        };
        let t = by_name(&b, "Currency");
        assert_eq!((t.cost, t.era), (16, 0));
        // Mathematics, in the Conquests TECH order.
        assert_eq!(t.prerequisites[0], 10);
        assert_eq!(&t.prerequisites[1..], &[-1, -1, -1]);
        assert_eq!(b.rules.techs[10].name.text(), "Mathematics");
    }

    /// Scenario files use the flavor mask (bit `i` = `FLAV` row `i`); its
    /// bits stay inside the file's `FLAV` table.
    #[test]
    fn flavor_masks_index_the_flav_table() {
        let mut flavored = 0;
        for f in files() {
            let Ok(b) = Biq::from_raw(&f.raw) else {
                continue;
            };
            let nf = b.flavors.as_ref().map_or(0, |f| f.flavors.len() as u32);
            for t in &b.rules.techs {
                if f.version >= crate::Version::new(12, 6) {
                    assert!(
                        t.flavor_revision != 0,
                        "{}: {} would be migrated",
                        f.name(),
                        t.name.text()
                    );
                }
                if t.flavors != 0 {
                    flavored += 1;
                    assert!(
                        nf > 0 && u64::from(t.flavors) < (1u64 << nf),
                        "{}",
                        f.name()
                    );
                }
            }
        }
        if !files().is_empty() {
            assert!(flavored > 20, "flavored advances: {flavored}");
        }
    }

    #[test]
    fn old_rows_have_no_flavor_fields() {
        let (mut ptw, mut c3c) = (0, 0);
        for f in files() {
            let Ok(b) = Biq::from_raw(&f.raw) else {
                continue;
            };
            let n = b.rules.techs.len();
            if n == 0 {
                continue;
            }
            let len = f.raw.section(b"TECH").unwrap().rows[0].len();
            if f.version < crate::Version::new(12, 0) {
                assert_eq!(len, 104, "{}", f.name());
                assert!(b.rules.techs.iter().all(|t| t.flavors == 0));
                ptw += 1;
            } else {
                assert_eq!(len, 112, "{}", f.name());
                c3c += 1;
            }
        }
        if !files().is_empty() {
            assert!(ptw > 10 && c3c > 20, "{ptw} {c3c}");
        }
    }

    /// The reader's fix-up for rows with revision 0 (`0x5E8729..0x5E8765`).
    #[test]
    fn legacy_flavor_bits_move_down_when_the_revision_is_zero() {
        let t = Tech {
            flavors: (1 << 10) | (1 << 12) | (1 << 16),
            flavor_revision: 0,
            ..Tech::default()
        };
        assert_eq!(t.effective_flavors(), 0b100_0101);
        assert!(t.has_flavor(0) && t.has_flavor(2) && t.has_flavor(6));
        assert!(!t.has_flavor(1));
        // Nothing in the legacy range: untouched, even with the revision unset.
        let u = Tech {
            flavors: 0b101,
            flavor_revision: 0,
            ..Tech::default()
        };
        assert_eq!(u.effective_flavors(), 0b101);
        // Revision 1 rows are taken as they are.
        let v = Tech {
            flavors: (1 << 10),
            flavor_revision: 1,
            ..Tech::default()
        };
        assert_eq!(v.effective_flavors(), 1 << 10);
    }

    #[test]
    fn new_rows_follow_the_constructor() {
        let t = Tech::default();
        assert_eq!((t.cost, t.era), (0, -1));
        assert_eq!(t.prerequisites, [-1; 4]);
        assert!(!t.is_slot());
        let mut w = Writer::new();
        let ctx = |major, minor| Ctx {
            version: crate::Version::new(major, minor),
        };
        t.write(&mut w, &ctx(11, 18));
        assert_eq!(w.buf.len(), 104);
        let mut w = Writer::new();
        t.write(&mut w, &ctx(12, 8));
        assert_eq!(w.buf.len(), 112);
    }
}
