//! `FLAV` - the *Flavors* table: seven named AI research "flavors" and the
//! relationship percentage between every pair (editor *Flavors* page, dialog
//! 206; help pages *Flavor Page*, *Relationship Percentage (Flavors)*,
//! *Change Flavor Name (Flavors Page)*).
//!
//! **Where**: loader `0x594290` arm for `FLAV`; section reader `0x52D2C0`,
//! per-flavor reader `0x52CCC0`, writer `0x52D530`. Present in every Conquests
//! (12.x) file and in no earlier version. The editor page (dialog 206) holds
//! two list boxes (1911 the flavor whose researching is being steered, 1912 the
//! flavor of the advance), the percentage edit 1913 with its spin 1914, and the
//! rename button 1916. It keeps the matrix as one heap array of 7 ints per
//! flavor (accessors `0x47EEC0` set, `0x47EEF0` get) and the help
//! (*Relationship Percentage*) calls the value a 0-100% chance; every shipped
//! value lies in `0..=100`.
//!
//! **Framing differs from every other section** - there are no row length
//! words:
//!
//! ```text
//! "FLAV"  u32 version (=1)  u32 numFlavors
//! numFlavors x {
//!     u32 version (=1)
//!     char name[256]            NUL padded, stale editor bytes after the NUL
//!     u32 numRelations          (=7 in every shipped file = numFlavors)
//!     i32 relation[numRelations]
//! }
//! ```
//!
//! so a stock record is `4 + 256 + 4 + 7*4 = 292` bytes. [`crate::raw::frame`]
//! delimits the records; this module decodes and encodes them.
//!
//! **Meaning** (help *Flavor Page*, confirmed by the stock data): flavor `i`
//! `relations[j]` is the percentage chance that an AI civilization carrying
//! flavor `i` researches advances of flavor `j`. The diagonal is `100` and the
//! default off-diagonal is `50`. Example, Rise of Rome: `Barbarian` row is
//! `[80, 20, 20, 20, 50, 50, 50]`, i.e. 20% for `Med Civ`, exactly the figure
//! the help text uses. Civilizations pick their flavors in `RACE`, and advances
//! and buildings carry flavor masks (`TECH`, `BLDG`).
//!
//! The "number of relations" word is stored per flavor; the loader does not
//! require it to equal `numFlavors`, so it is kept as the length of
//! [`Flavor::relations`].

use crate::io::{Error, Reader, Result, Str, Writer};

/// One flavor: its name and its relationship row.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Flavor {
    /// Per-record format word. Always `1` in the corpus.
    pub version: u32,
    /// Flavor name (`Flavor 1`.. by default; `Barbarian`, `Axis Power`, ...
    /// in scenarios). 256-byte buffer; the editor leaves the tail of a
    /// previous longer name after the NUL, which is preserved.
    pub name: Str<256>,
    /// Relationship percentage towards each flavor (`relations[j]` is the
    /// chance, in percent, that a civ of this flavor researches advances of
    /// flavor `j`). Its length is the on-disk `numRelations`.
    pub relations: Vec<i32>,
}

impl Flavor {
    /// Decode one record (the bytes `raw::frame` assigned to it).
    pub fn read(r: &mut Reader<'_>) -> Result<Flavor> {
        fn truncated(r: &Reader<'_>, what: &'static str) -> Error {
            Error::Truncated {
                offset: r.pos(),
                what,
            }
        }
        let version = r.u32().ok_or_else(|| truncated(r, "FLAV record version"))?;
        let name = r.str::<256>().ok_or_else(|| truncated(r, "FLAV name"))?;
        let n = r.u32().ok_or_else(|| truncated(r, "FLAV relation count"))?;
        let mut relations = Vec::new();
        for _ in 0..n {
            relations.push(r.i32().ok_or_else(|| truncated(r, "FLAV relation"))?);
        }
        Ok(Flavor {
            version,
            name,
            relations,
        })
    }

    /// Encode one record.
    pub fn write(&self, w: &mut Writer) {
        w.u32(self.version);
        w.bytes(&self.name.0);
        w.u32(self.relations.len() as u32);
        for v in &self.relations {
            w.i32(*v);
        }
    }
}

/// The whole `FLAV` section.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Flavors {
    /// Section format word after the tag. Always `1` in the corpus.
    pub version: u32,
    /// The flavors, in order (seven in every shipped file).
    pub flavors: Vec<Flavor>,
}

impl Flavors {
    /// Section tag.
    pub const TAG: [u8; 4] = *b"FLAV";

    /// Decode the section from its framed records.
    pub fn from_raw(raw: &crate::raw::Raw, sec: &crate::raw::RawSection) -> Result<Flavors> {
        let mut flavors = Vec::with_capacity(sec.rows.len());
        for rec in &sec.rows {
            let mut r = Reader::new(raw.row(rec));
            flavors.push(Flavor::read(&mut r)?);
        }
        Ok(Flavors {
            version: sec.count,
            flavors,
        })
    }

    /// Encode the section including its tag and header words.
    pub fn write(&self, w: &mut Writer) {
        w.tag(Self::TAG);
        w.u32(self.version);
        w.u32(self.flavors.len() as u32);
        for f in &self.flavors {
            f.write(w);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus;
    use crate::raw::Framing;

    fn parse_all() -> Vec<(String, Flavors, Vec<u8>)> {
        let mut out = Vec::new();
        for f in corpus::files() {
            let Some(sec) = f.raw.section(b"FLAV") else {
                continue;
            };
            assert_eq!(sec.framing, Framing::Flav);
            let fl = Flavors::from_raw(&f.raw, sec).unwrap();
            out.push((f.name(), fl, f.raw.data[sec.start..sec.end].to_vec()));
        }
        out
    }

    #[test]
    fn corpus_roundtrip_is_byte_exact() {
        for (name, fl, original) in parse_all() {
            let mut w = Writer::new();
            fl.write(&mut w);
            assert_eq!(w.buf, original, "{name}");
            assert_eq!(fl.version, 1, "{name}");
            assert_eq!(fl.flavors.len(), 7, "{name}");
            for f in &fl.flavors {
                assert_eq!(f.version, 1, "{name}");
                assert_eq!(f.relations.len(), 7, "{name}");
                // 292-byte records, as in the row-length survey
                let mut w = Writer::new();
                f.write(&mut w);
                assert_eq!(w.len(), 292);
            }
        }
    }

    /// `FLAV` appears with the Conquests format and never before; the editor
    /// shows a percentage, so every relation is within 0-100.
    #[test]
    fn present_exactly_in_conquests_files_and_percentages_are_in_range() {
        let mut conquests = 0;
        for f in corpus::files() {
            let has = f.raw.section(b"FLAV").is_some();
            assert_eq!(has, f.version.major >= 12, "{}", f.name());
            conquests += usize::from(has);
            if let Some(sec) = f.raw.section(b"FLAV") {
                let fl = Flavors::from_raw(&f.raw, sec).unwrap();
                for fv in &fl.flavors {
                    assert!(fv.name.text().len() < 32, "{}", f.name());
                    assert!(
                        fv.relations.iter().all(|v| (0..=100).contains(v)),
                        "{}",
                        f.name()
                    );
                }
            }
        }
        if !corpus::files().is_empty() {
            assert!(conquests >= 20, "{conquests}");
        }
    }

    #[test]
    fn stock_relationship_matrix() {
        // conquests.biq: identity-ish matrix, 100 on the diagonal, 50 elsewhere.
        // Rise of Rome overrides the Barbarian row (the example in the help).
        for (name, fl, _) in parse_all() {
            if name.contains("Rise_of_Rome") {
                assert_eq!(fl.flavors[0].name.text(), "Barbarian");
                assert_eq!(fl.flavors[0].relations, vec![80, 20, 20, 20, 50, 50, 50]);
                assert_eq!(fl.flavors[3].name.text(), "Med Civ");
                assert_eq!(fl.flavors[3].relations[3], 100);
            }
            if name.ends_with("conquests.biq") {
                assert_eq!(fl.flavors[0].name.text(), "Flavor1");
                for (i, f) in fl.flavors.iter().enumerate() {
                    for (j, v) in f.relations.iter().enumerate() {
                        assert_eq!(*v, if i == j { 100 } else { 50 }, "{name} {i},{j}");
                    }
                }
            }
        }
    }
}
