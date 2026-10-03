//! Container layer: optional DCL compression, the 4-byte magic, and the
//! generic section framing. Nothing here knows what any section *means*.
//!
//! ```text
//! [magic "BIC " | "BICX" | "BICQ"]
//! ( [tag 4][u32 count] ( [u32 len][len bytes] ) * count ) *      until EOF
//! ```
//!
//! This is exactly what the loader `0x594290` does: it reads a tag, hands the
//! section to the arm for that tag, and for tags it does not want falls into
//! the shared skip path (`0x594E7E`..`0x594F3E`) that reads the `u32` count and
//! then `fseek`s over `count` length-prefixed rows. There is no end marker; the
//! loop stops when the next 4-byte tag read comes back short (`0x59437B`).
//!
//! One section does not follow the row framing: `FLAV` (see [`Framing::Flav`]).

use crate::dcl::{self, Storage};
use crate::io::{Error, Result};
use std::ops::Range;

/// The 4-byte file signature.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Magic {
    /// `BIC ` - Civ3 / PTW era (`.bic`, BIQ versions 2.x-4.x).
    Bic,
    /// `BICX` - Play the World / Conquests (`.bix`, `.biq`, versions 11.x-12.x).
    Bicx,
    /// `BICQ` - written by the game itself for in-game saves; accepted by the
    /// loader (`0x59433B`) but not seen in any shipped file.
    Bicq,
    /// `CIV3` - a saved game (`.sav`); a different stream, parsed by [`crate::sav`], not framed here.
    Civ3,
}

impl Magic {
    /// Classify the first four bytes.
    pub fn from_bytes(b: &[u8]) -> Option<Magic> {
        match b.get(..4)? {
            b"BIC " => Some(Magic::Bic),
            b"BICX" => Some(Magic::Bicx),
            b"BICQ" => Some(Magic::Bicq),
            b"CIV3" => Some(Magic::Civ3),
            _ => None,
        }
    }

    /// The four signature bytes.
    pub fn bytes(self) -> [u8; 4] {
        match self {
            Magic::Bic => *b"BIC ",
            Magic::Bicx => *b"BICX",
            Magic::Bicq => *b"BICQ",
            Magic::Civ3 => *b"CIV3",
        }
    }
}

/// How a section's rows are delimited.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Framing {
    /// `[tag][u32 count]` then `count` rows of `[u32 len][len bytes]`.
    Rows,
    /// `FLAV`: `[tag][u32 version = 1][u32 numFlavors]` then, per flavor,
    /// `[u32 version = 1][char name[256]][u32 numRelations][i32 x numRelations]`.
    /// No length prefixes (reader `0x52D2C0`, per-flavor `0x52CCC0`).
    Flav,
}

/// One section located in the decoded stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawSection {
    /// 4-byte tag (`GOOD`, `TERR`, `VER#`, ...).
    pub tag: [u8; 4],
    /// The header word after the tag: row count for [`Framing::Rows`], the
    /// format version for [`Framing::Flav`].
    pub count: u32,
    /// How rows are delimited.
    pub framing: Framing,
    /// Offset of the tag.
    pub start: usize,
    /// Offset just past the last row.
    pub end: usize,
    /// Row bodies: for [`Framing::Rows`] the bytes after each `u32 len`; for
    /// [`Framing::Flav`] each whole flavor record.
    pub rows: Vec<Range<usize>>,
}

impl RawSection {
    /// Tag as text.
    pub fn tag_str(&self) -> String {
        String::from_utf8_lossy(&self.tag).into_owned()
    }
}

/// A decoded scenario stream with its sections located but not interpreted.
#[derive(Clone, Debug)]
pub struct Raw {
    /// File signature.
    pub magic: Magic,
    /// How the input was stored on disk (plain, or DCL with which settings).
    pub storage: Storage,
    /// The decoded stream (starts with the magic).
    pub data: Vec<u8>,
    /// Sections in file order.
    pub sections: Vec<RawSection>,
}

fn rd_u32(data: &[u8], at: usize, what: &'static str) -> Result<u32> {
    data.get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or(Error::Truncated { offset: at, what })
}

fn is_tag(t: &[u8]) -> bool {
    t.len() == 4
        && t.iter()
            .all(|&b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'#')
}

impl Raw {
    /// Decode (if compressed) and frame a scenario file.
    ///
    /// `CIV3` saves are decoded and identified but not framed
    /// (`sections` stays empty): their stream is not a plain section list.
    pub fn parse(input: &[u8]) -> Result<Raw> {
        let storage = Storage::sniff(input);
        let data = if storage != Storage::Plain {
            dcl::decompress(input)?
        } else if Magic::from_bytes(input).is_some() {
            input.to_vec()
        } else {
            let mut m = [0u8; 4];
            let n = input.len().min(4);
            m[..n].copy_from_slice(&input[..n]);
            return Err(Error::BadMagic(m));
        };
        let magic = Magic::from_bytes(&data).ok_or_else(|| {
            let mut m = [0u8; 4];
            let n = data.len().min(4);
            m[..n].copy_from_slice(&data[..n]);
            Error::BadMagic(m)
        })?;
        let sections = if magic == Magic::Civ3 {
            Vec::new()
        } else {
            frame(&data)?
        };
        Ok(Raw {
            magic,
            storage,
            data,
            sections,
        })
    }

    /// Whether the input was DCL-compressed on disk.
    pub fn compressed(&self) -> bool {
        self.storage != Storage::Plain
    }

    /// First section with `tag`.
    pub fn section(&self, tag: &[u8; 4]) -> Option<&RawSection> {
        self.sections.iter().find(|s| &s.tag == tag)
    }

    /// Row body bytes.
    pub fn row(&self, r: &Range<usize>) -> &[u8] {
        &self.data[r.clone()]
    }
}

/// Locate every section of a decoded stream (starting after the magic).
pub fn frame(data: &[u8]) -> Result<Vec<RawSection>> {
    let mut out = Vec::new();
    let mut p = 4usize;
    while p < data.len() {
        // The game's loop stops when the 4-byte tag read comes back short.
        let Some(tag) = data.get(p..p + 4) else { break };
        if !is_tag(tag) {
            return Err(Error::BadTag {
                offset: p,
                found: [tag[0], tag[1], tag[2], tag[3]],
            });
        }
        let tag: [u8; 4] = tag.try_into().unwrap();
        let count = rd_u32(data, p + 4, "section count")?;
        let mut q = p + 8;
        let mut rows = Vec::new();
        let framing;
        if &tag == b"FLAV" {
            framing = Framing::Flav;
            // count word = format version; next word = number of flavors
            let n = rd_u32(data, q, "FLAV flavor count")?;
            q += 4;
            for _ in 0..n {
                let start = q;
                rd_u32(data, q, "FLAV record version")?;
                // name[256] follows, then numRelations
                let nrel = rd_u32(data, q + 4 + 256, "FLAV relation count")? as usize;
                q += 4 + 256 + 4;
                let bytes = nrel.checked_mul(4).filter(|b| q + b <= data.len()).ok_or(
                    Error::Truncated {
                        offset: q,
                        what: "FLAV relations",
                    },
                )?;
                q += bytes;
                rows.push(start..q);
            }
        } else {
            framing = Framing::Rows;
            // a row is at least its length word, so a larger count is bogus; the
            // loop below fails on it, but must not first reserve for it
            rows.reserve((count as usize).min(data.len().saturating_sub(q) / 4));
            for _ in 0..count {
                let len = rd_u32(data, q, "row length")? as usize;
                let body = q + 4;
                if body + len > data.len() {
                    return Err(Error::Truncated {
                        offset: q,
                        what: "row body",
                    });
                }
                rows.push(body..body + len);
                q = body + len;
            }
        }
        out.push(RawSection {
            tag,
            count,
            framing,
            start: p,
            end: q,
            rows,
        });
        p = q;
    }
    Ok(out)
}
