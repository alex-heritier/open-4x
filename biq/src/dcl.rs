//! PKWARE DCL ("implode"/"explode") codec: the transport the game wraps around
//! compressed `.biq`/`.bic`/`.bix`/`.sav` files. [`decompress`] reads both
//! literal modes; [`crate::implode::compress`] is the matching writer.
//!
//! Recovered instruction by instruction from `Civ3Conquests.exe` (it embeds
//! "PKWARE Data Compression Library for Win32 ... Version 1.11", `0x73A388`).
//! Explode: init `0x649400`, main loop `0x649580`, symbol decode `0x649680`,
//! distance decode `0x649830`, bit reader `0x6498B0`, peek-table expansion
//! `0x649940`, ASCII-table builder `0x649980`. Implode lives in
//! [`crate::implode`]. The scenario loader reaches explode through `0x5F76C0`
//! (see `reverse-engineering/biq.md`).
//!
//! # Stream framing
//!
//! ```text
//! byte 0   mode        0 = binary (raw 8-bit literals), 1 = ASCII (variable-length literal codes)
//! byte 1   dict bits   4..=6 (window 1 KiB / 2 KiB / 4 KiB); distance mask = (1 << bits) - 1
//! byte 2.. bitstream   LSB-first; byte 2 is *also* part of the stream (the init
//!                      seeds the bit buffer with it, `0x64945C`), it is not a header field
//! ```
//!
//! Symbols: `< 0x100` literal, `0x100..0x305` a match of length `sym - 0xFE`,
//! `>= 0x305` end of stream (`0x6495A9`).
//!
//! # Mode 1
//!
//! A literal is the flag bit `0` followed by the byte's code from the 256-entry
//! table [`CH_BITS_ASC`] / [`CH_CODE_ASC`] (`0x73A088` / `0x73A188`, 4 to 13 bits,
//! packed LSB-first like everything else). The exe holds the lengths twice (the
//! explode state is seeded from `0x73A520`, byte-identical to the implode copy)
//! and the codes once, shared by both directions. Verified by: the lengths
//! satisfy Kraft's equality exactly (`sum 2^-len = 1`, so the table is a
//! complete prefix code and a bad byte would break it), every code fits its
//! length, and encode followed by decode returns every byte value. **No shipped
//! file uses mode 1** (every compressed file starts `00 06`), so unlike mode 0
//! there is no stream from the game to compare against; the unit tests pin
//! the table instead.

use std::fmt;

/// Extra-bit count per length class (`0x73A500`; the implode copy is `0x73A068`);
/// also the code length of the class in the 8-bit peek table.
pub(crate) const LEN_EXTRA_CNT: [u8; 16] = [3, 2, 3, 3, 4, 4, 4, 5, 5, 5, 5, 6, 6, 6, 7, 7];
/// Low canonical code per length class (`0x73A510`; implode `0x73A078`).
pub(crate) const LEN_LOCODE: [u8; 16] = [5, 3, 1, 6, 10, 2, 12, 20, 4, 24, 8, 48, 16, 32, 64, 0];
/// Extra bits in the short-tree length code (`0x73A4D0`; implode `0x73A058`).
pub(crate) const SHORT_EXTRA: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8];
/// Short-tree length bases (`0x73A4E0`, 16 u16).
pub(crate) const SHORT_BASE: [u16; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 14, 22, 38, 70, 134, 262];
/// Code lengths for the 64 distance classes (`0x73A450`; implode `0x739FD8`).
pub(crate) const DIST_LEN: [u8; 64] = [
    2, 4, 4, 5, 5, 5, 5, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
    7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
];
/// Low canonical code per distance class (`0x73A490`; implode `0x73A018`).
pub(crate) const DIST_LOCODE: [u8; 64] = [
    3, 13, 5, 25, 9, 17, 1, 62, 30, 46, 14, 54, 22, 38, 6, 58, 26, 42, 10, 50, 18, 34, 66, 2, 124,
    60, 92, 28, 108, 44, 76, 12, 116, 52, 84, 20, 100, 36, 68, 4, 120, 56, 88, 24, 104, 40, 72, 8,
    240, 112, 176, 48, 208, 80, 144, 16, 224, 96, 160, 32, 192, 64, 128, 0,
];

/// End-of-stream symbol (`0x6495A9`: `sym >= 0x305` ends the loop).
pub(crate) const TERMINATOR: u32 = 0x305;
/// Match length bias: `len = sym - 0xFE` (`0x6495C1`).
pub(crate) const LEN_BIAS: u32 = 0xFE;

/// Mode-1 literal code length in bits for each byte value (`0x73A088`; the explode
/// copy at `0x73A520` is identical). Does not include the leading flag bit.
pub const CH_BITS_ASC: [u8; 256] = [
    0x0B, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x08, 0x07, 0x0C, 0x0C, 0x07, 0x0C, 0x0C,
    0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0D, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C,
    0x04, 0x0A, 0x08, 0x0C, 0x0A, 0x0C, 0x0A, 0x08, 0x07, 0x07, 0x08, 0x09, 0x07, 0x06, 0x07, 0x08,
    0x07, 0x06, 0x07, 0x07, 0x07, 0x07, 0x08, 0x07, 0x07, 0x08, 0x08, 0x0C, 0x0B, 0x07, 0x09, 0x0B,
    0x0C, 0x06, 0x07, 0x06, 0x06, 0x05, 0x07, 0x08, 0x08, 0x06, 0x0B, 0x09, 0x06, 0x07, 0x06, 0x06,
    0x07, 0x0B, 0x06, 0x06, 0x06, 0x07, 0x09, 0x08, 0x09, 0x09, 0x0B, 0x08, 0x0B, 0x09, 0x0C, 0x08,
    0x0C, 0x05, 0x06, 0x06, 0x06, 0x05, 0x06, 0x06, 0x06, 0x05, 0x0B, 0x07, 0x05, 0x06, 0x05, 0x05,
    0x06, 0x0A, 0x05, 0x05, 0x05, 0x05, 0x08, 0x07, 0x08, 0x08, 0x0A, 0x0B, 0x0B, 0x0C, 0x0C, 0x0C,
    0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D,
    0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D,
    0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D,
    0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C,
    0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C,
    0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C,
    0x0D, 0x0C, 0x0D, 0x0D, 0x0D, 0x0C, 0x0D, 0x0D, 0x0D, 0x0C, 0x0D, 0x0D, 0x0D, 0x0D, 0x0C, 0x0D,
    0x0D, 0x0D, 0x0C, 0x0C, 0x0C, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D, 0x0D,
];
/// Mode-1 literal code for each byte value (`0x73A188`, u16), LSB-first, `CH_BITS_ASC[b]`
/// bits wide. Does not include the leading flag bit.
#[rustfmt::skip]
pub const CH_CODE_ASC: [u16; 256] = [
    0x0490, 0x0FE0, 0x07E0, 0x0BE0, 0x03E0, 0x0DE0, 0x05E0, 0x09E0,
    0x01E0, 0x00B8, 0x0062, 0x0EE0, 0x06E0, 0x0022, 0x0AE0, 0x02E0,
    0x0CE0, 0x04E0, 0x08E0, 0x00E0, 0x0F60, 0x0760, 0x0B60, 0x0360,
    0x0D60, 0x0560, 0x1240, 0x0960, 0x0160, 0x0E60, 0x0660, 0x0A60,
    0x000F, 0x0250, 0x0038, 0x0260, 0x0050, 0x0C60, 0x0390, 0x00D8,
    0x0042, 0x0002, 0x0058, 0x01B0, 0x007C, 0x0029, 0x003C, 0x0098,
    0x005C, 0x0009, 0x001C, 0x006C, 0x002C, 0x004C, 0x0018, 0x000C,
    0x0074, 0x00E8, 0x0068, 0x0460, 0x0090, 0x0034, 0x00B0, 0x0710,
    0x0860, 0x0031, 0x0054, 0x0011, 0x0021, 0x0017, 0x0014, 0x00A8,
    0x0028, 0x0001, 0x0310, 0x0130, 0x003E, 0x0064, 0x001E, 0x002E,
    0x0024, 0x0510, 0x000E, 0x0036, 0x0016, 0x0044, 0x0030, 0x00C8,
    0x01D0, 0x00D0, 0x0110, 0x0048, 0x0610, 0x0150, 0x0060, 0x0088,
    0x0FA0, 0x0007, 0x0026, 0x0006, 0x003A, 0x001B, 0x001A, 0x002A,
    0x000A, 0x000B, 0x0210, 0x0004, 0x0013, 0x0032, 0x0003, 0x001D,
    0x0012, 0x0190, 0x000D, 0x0015, 0x0005, 0x0019, 0x0008, 0x0078,
    0x00F0, 0x0070, 0x0290, 0x0410, 0x0010, 0x07A0, 0x0BA0, 0x03A0,
    0x0240, 0x1C40, 0x0C40, 0x1440, 0x0440, 0x1840, 0x0840, 0x1040,
    0x0040, 0x1F80, 0x0F80, 0x1780, 0x0780, 0x1B80, 0x0B80, 0x1380,
    0x0380, 0x1D80, 0x0D80, 0x1580, 0x0580, 0x1980, 0x0980, 0x1180,
    0x0180, 0x1E80, 0x0E80, 0x1680, 0x0680, 0x1A80, 0x0A80, 0x1280,
    0x0280, 0x1C80, 0x0C80, 0x1480, 0x0480, 0x1880, 0x0880, 0x1080,
    0x0080, 0x1F00, 0x0F00, 0x1700, 0x0700, 0x1B00, 0x0B00, 0x1300,
    0x0DA0, 0x05A0, 0x09A0, 0x01A0, 0x0EA0, 0x06A0, 0x0AA0, 0x02A0,
    0x0CA0, 0x04A0, 0x08A0, 0x00A0, 0x0F20, 0x0720, 0x0B20, 0x0320,
    0x0D20, 0x0520, 0x0920, 0x0120, 0x0E20, 0x0620, 0x0A20, 0x0220,
    0x0C20, 0x0420, 0x0820, 0x0020, 0x0FC0, 0x07C0, 0x0BC0, 0x03C0,
    0x0DC0, 0x05C0, 0x09C0, 0x01C0, 0x0EC0, 0x06C0, 0x0AC0, 0x02C0,
    0x0CC0, 0x04C0, 0x08C0, 0x00C0, 0x0F40, 0x0740, 0x0B40, 0x0340,
    0x0300, 0x0D40, 0x1D00, 0x0D00, 0x1500, 0x0540, 0x0500, 0x1900,
    0x0900, 0x0940, 0x1100, 0x0100, 0x1E00, 0x0E00, 0x0140, 0x1600,
    0x0600, 0x1A00, 0x0E40, 0x0640, 0x0A40, 0x0A00, 0x1200, 0x0200,
    0x1C00, 0x0C00, 0x1400, 0x0400, 0x1800, 0x0800, 0x1000, 0x0000,
];

/// How literals are coded: byte 0 of the stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    /// `0`: every literal is its 8 raw bits (what every shipped file uses).
    Binary = 0,
    /// `1`: literals use the variable-length [`CH_BITS_ASC`] / [`CH_CODE_ASC`] code.
    Ascii = 1,
}

/// LSB-first bit reader over the compressed stream (`0x6498B0`).
struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    bitbuf: u32,
    avail: u32,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8]) -> Self {
        Bits {
            data,
            pos: 0,
            bitbuf: 0,
            avail: 0,
        }
    }

    /// Ensure at least `n` valid low bits, loading whole bytes LSB-first.
    /// Returns `false` when the stream is exhausted first.
    fn fill(&mut self, n: u32) -> bool {
        while self.avail < n {
            let Some(&b) = self.data.get(self.pos) else {
                return false;
            };
            self.pos += 1;
            self.bitbuf |= (b as u32) << self.avail;
            self.avail += 8;
        }
        true
    }

    /// Consume `n` bits; `None` on exhaustion.
    fn get(&mut self, n: u32) -> Option<()> {
        if !self.fill(n) {
            return None;
        }
        self.bitbuf >>= n;
        self.avail -= n;
        Some(())
    }

    /// Low `n` bits of the stream, loading input as needed.
    fn peek(&mut self, n: u32) -> u32 {
        self.fill(n);
        self.bitbuf & (u32::MAX >> (32 - n.min(32)))
    }
}

/// `expand(count, lens, bases)` (`0x649940`): 256-entry 8-bit peek table.
/// `table[v] = class` for `v` in `bases[class]..256` stepping `1 << lens[class]`.
fn expand(count: usize, lens: &[u8], bases: &[u8]) -> [u8; 256] {
    let mut table = [0u8; 256];
    for class in (0..count).rev() {
        let stride = 1usize << lens[class];
        let mut v = bases[class] as usize;
        while v < 0x100 {
            table[v] = class as u8;
            v += stride;
        }
    }
    table
}

/// How a scenario or save is stored on disk.
///
/// The loader sniffs the magic (`0x59433B`): a stream that starts with an
/// ASCII magic is read as is, anything else goes through the DCL decoder
/// (`0x5F76C0`). So both forms load, and [`Storage`] only records which one a
/// file arrived in, so that it can be written back the same way.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Storage {
    /// The bare stream (36 of the 119 shipped scenarios).
    #[default]
    Plain,
    /// PKWARE DCL, with the header's mode and dictionary size (83 shipped
    /// scenarios, all `00 06`: see [`Storage::GAME`]).
    Dcl {
        /// Literal coding (header byte 0).
        mode: Mode,
        /// Dictionary bits, `4..=6` (header byte 1).
        dict_bits: u8,
    },
}

impl Storage {
    /// What every shipped compressed file uses: binary literals, 4 KiB window.
    pub const GAME: Storage = Storage::Dcl {
        mode: Mode::Binary,
        dict_bits: 6,
    };

    /// The storage of `input` as it sits on disk: [`Storage::Dcl`] when it
    /// looks like a DCL header ([`looks_compressed`]) and does not start with
    /// a known magic, else [`Storage::Plain`].
    pub fn sniff(input: &[u8]) -> Storage {
        if crate::raw::Magic::from_bytes(input).is_none() && looks_compressed(input) {
            Storage::Dcl {
                mode: if input[0] == 1 {
                    Mode::Ascii
                } else {
                    Mode::Binary
                },
                dict_bits: input[1],
            }
        } else {
            Storage::Plain
        }
    }

    /// The on-disk bytes for the decoded `stream`: a copy for `Plain`, else the
    /// output of the game's compressor ([`crate::implode::compress`]).
    pub fn encode(self, stream: &[u8]) -> Result<Vec<u8>, DclError> {
        match self {
            Storage::Plain => Ok(stream.to_vec()),
            Storage::Dcl { mode, dict_bits } => crate::implode::compress(stream, mode, dict_bits),
        }
    }
}

/// Longest mode-1 literal code (`max(CH_BITS_ASC)`): the peek width.
const ASCII_PEEK: u32 = 13;

/// Peek table for mode-1 literals: for every 13-bit window `(byte, code length)`
/// of the code that is a prefix of it (the code is prefix-free and complete, so
/// exactly one matches). The exe builds the equivalent tables in `0x649980`.
fn ascii_decoder() -> Vec<(u8, u8)> {
    let mut table = vec![(0u8, 0u8); 1 << ASCII_PEEK];
    for byte in 0..256usize {
        let len = CH_BITS_ASC[byte] as u32;
        let code = CH_CODE_ASC[byte] as usize;
        let mut window = code;
        while window < table.len() {
            table[window] = (byte as u8, len as u8);
            window += 1 << len;
        }
    }
    table
}

/// Why a DCL stream failed to decode.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DclError {
    /// Input shorter than the 3-byte header (`0x64943A`).
    TruncatedHeader,
    /// Dictionary bits outside 4..=6 (`0x649475`).
    BadDictBits(u8),
    /// Mode byte outside {0, 1} (`0x64949A`).
    BadMode(u8),
    /// The bit stream ended in the middle of a symbol.
    Truncated,
    /// A match referred to data before the start of the output.
    BadDistance,
}

impl fmt::Display for DclError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DclError::TruncatedHeader => write!(f, "DCL stream shorter than its 3-byte header"),
            DclError::BadDictBits(b) => write!(f, "DCL dictionary bits {b} outside 4..=6"),
            DclError::BadMode(m) => write!(f, "DCL mode byte {m} is neither 0 nor 1"),
            DclError::Truncated => write!(f, "DCL bit stream ended inside a symbol"),
            DclError::BadDistance => {
                write!(f, "DCL match distance reaches before the output start")
            }
        }
    }
}

impl std::error::Error for DclError {}

/// Whether `input` plausibly starts with a DCL header: mode 0/1 and
/// dictionary bits 4..=6 (the check the exe performs at `0x649475`).
///
/// Every raw scenario starts with an ASCII magic (`BIC `, `BICX`, `BICQ`,
/// `CIV3`), whose first byte is far above 1, so this cannot misfire on them.
pub fn looks_compressed(input: &[u8]) -> bool {
    input.len() >= 3 && input[0] <= 1 && (4..=6).contains(&input[1])
}

/// Decompress one DCL stream (header at byte 0).
pub fn decompress(input: &[u8]) -> Result<Vec<u8>, DclError> {
    if input.len() < 3 {
        return Err(DclError::TruncatedHeader);
    }
    let mode = input[0];
    let dict_bits = input[1];
    if !(4..=6).contains(&dict_bits) {
        return Err(DclError::BadDictBits(dict_bits));
    }
    if mode > 1 {
        return Err(DclError::BadMode(mode));
    }
    // `mov eax,0xffff` / `mov cl,0x10` / `sub cl,dl` (dl = dict bits) /
    // `sar eax,cl` (`0x649487`-`0x649490`): (1 << dict_bits) - 1.
    let mask = 0xFFFFu32 >> (16 - dict_bits as u32);

    let len_class = expand(16, &LEN_EXTRA_CNT, &LEN_LOCODE);
    let dist_class = expand(64, &DIST_LEN, &DIST_LOCODE);
    let ascii = (mode == 1).then(ascii_decoder);

    // The bit buffer is seeded with the third header byte (`0x64945C`), so
    // the stream starts at byte 2, not byte 3.
    let mut bits = Bits::new(&input[2..]);
    let mut out: Vec<u8> = Vec::new();

    loop {
        let select = bits.peek(1);
        bits.get(1).ok_or(DclError::Truncated)?;
        let sym: u32 = if select != 0 {
            // Short tree (`0x6496A9`): 8-bit peek, length class, extra bits.
            let class = len_class[bits.peek(8) as usize] as u32;
            bits.get(LEN_EXTRA_CNT[class as usize] as u32)
                .ok_or(DclError::Truncated)?;
            let extra = SHORT_EXTRA[class as usize] as u32;
            let e = if extra != 0 {
                let e = bits.peek(extra);
                if bits.get(extra).is_none() {
                    // Tolerated only for the terminator (`0x649703`).
                    if class + e != 0x10E {
                        return Err(DclError::Truncated);
                    }
                }
                e
            } else {
                0
            };
            0x100 + SHORT_BASE[class as usize] as u32 + e
        } else if let Some(table) = &ascii {
            // ASCII mode: the byte's variable-length code (`0x64977F`..).
            let (lit, len) = table[bits.peek(ASCII_PEEK) as usize];
            bits.get(len as u32).ok_or(DclError::Truncated)?;
            lit as u32
        } else {
            // Binary mode: raw literal byte (`0x64974A`).
            let lit = bits.peek(8);
            bits.get(8).ok_or(DclError::Truncated)?;
            lit
        };

        if sym >= TERMINATOR {
            break;
        }
        if sym < 0x100 {
            out.push(sym as u8);
        } else {
            let len = (sym - LEN_BIAS) as usize;
            let dist = decode_distance(&mut bits, &dist_class, len, dict_bits, mask)? as usize;
            if dist == 0 || dist > out.len() {
                return Err(DclError::BadDistance);
            }
            for _ in 0..len {
                let b = out[out.len() - dist];
                out.push(b);
            }
        }
    }
    Ok(out)
}

/// Match distance (`0x649830`): peek class, consume its code bits, then
/// 2 extra bits for `len == 2`, else `dict_bits` bits masked by `mask`.
fn decode_distance(
    bits: &mut Bits<'_>,
    dist_class: &[u8; 256],
    len: usize,
    dict_bits: u8,
    mask: u32,
) -> Result<u32, DclError> {
    let class = dist_class[bits.peek(8) as usize] as u32;
    bits.get(DIST_LEN[class as usize] as u32)
        .ok_or(DclError::Truncated)?;
    let d = if len == 2 {
        let e = bits.peek(2);
        bits.get(2).ok_or(DclError::Truncated)?;
        (class << 2) | e
    } else {
        let e = bits.peek(dict_bits as u32) & mask;
        bits.get(dict_bits as u32).ok_or(DclError::Truncated)?;
        (class << dict_bits) | e
    };
    Ok(d + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal DCL encoder for binary-literal mode, built from the same code
    /// tables the decoder uses. It exists to exercise the decoder without the
    /// game install (the real files are checked in `golden_real_files`).
    struct Enc {
        out: Vec<u8>,
        acc: u32,
        used: u32,
        dict_bits: u8,
    }

    impl Enc {
        fn new(dict_bits: u8) -> Enc {
            Enc {
                out: vec![0, dict_bits],
                acc: 0,
                used: 0,
                dict_bits,
            }
        }
        fn bit(&mut self, b: u32) {
            self.acc |= (b & 1) << self.used;
            self.used += 1;
            if self.used == 8 {
                self.out.push(self.acc as u8);
                self.acc = 0;
                self.used = 0;
            }
        }
        /// `count` bits of `value`, least significant first.
        fn bits(&mut self, value: u32, count: u32) {
            for i in 0..count {
                self.bit(value >> i);
            }
        }
        fn literal(&mut self, b: u8) {
            self.bit(0);
            self.bits(b as u32, 8);
        }
        /// Length symbol `0x100 + v` through the short tree.
        fn length_symbol(&mut self, v: u32) {
            let class = (0..16)
                .rev()
                .find(|&c| SHORT_BASE[c] as u32 <= v)
                .expect("class");
            assert!(v - (SHORT_BASE[class] as u32) < (1 << SHORT_EXTRA[class]));
            self.bit(1);
            self.bits(LEN_LOCODE[class] as u32, LEN_EXTRA_CNT[class] as u32);
            self.bits(v - SHORT_BASE[class] as u32, SHORT_EXTRA[class] as u32);
        }
        fn matched(&mut self, len: u32, dist: u32) {
            self.length_symbol(len - 2);
            let d = dist - 1;
            let (class, extra, n) = if len == 2 {
                (d >> 2, d & 3, 2)
            } else {
                (
                    d >> self.dict_bits,
                    d & ((1 << self.dict_bits) - 1),
                    self.dict_bits as u32,
                )
            };
            self.bits(
                DIST_LOCODE[class as usize] as u32,
                DIST_LEN[class as usize] as u32,
            );
            self.bits(extra, n);
        }
        fn finish(mut self) -> Vec<u8> {
            self.length_symbol(TERMINATOR - 0x100);
            if self.used > 0 {
                self.out.push(self.acc as u8);
            }
            self.out
        }
    }

    #[test]
    fn literals_only() {
        let text = b"Civilization III: Conquests";
        let mut e = Enc::new(6);
        for &b in text {
            e.literal(b);
        }
        assert_eq!(decompress(&e.finish()).unwrap(), text);
    }

    #[test]
    fn overlapping_and_two_byte_matches() {
        let mut e = Enc::new(6);
        for &b in b"abc" {
            e.literal(b);
        }
        e.matched(9, 3); // overlapping copy: abcabcabcabc
        e.literal(b'x');
        e.literal(b'y');
        e.matched(2, 2); // two-byte match uses the 2-bit distance form
        let out = decompress(&e.finish()).unwrap();
        assert_eq!(out, b"abcabcabcabcxyxy");
    }

    #[test]
    fn every_dictionary_size_and_length_class() {
        // Deterministic pseudo-random program of literals and matches.
        for dict_bits in 4u8..=6 {
            let window = 64usize << dict_bits;
            let mut state = 0x1234_5678u32 ^ dict_bits as u32;
            let mut rnd = move || {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state
            };
            let mut e = Enc::new(dict_bits);
            let mut expect: Vec<u8> = Vec::new();
            for _ in 0..4000 {
                if expect.len() < 8 || rnd() % 3 == 0 {
                    let b = (rnd() % 251) as u8;
                    e.literal(b);
                    expect.push(b);
                } else {
                    // lengths 2..=518 (every class incl. the longest), any distance in window
                    let len = match rnd() % 4 {
                        0 => 2,
                        1 => 3 + rnd() % 8,
                        2 => 10 + rnd() % 60,
                        _ => 2 + rnd() % 517,
                    };
                    // two-byte matches have only a 6-bit class + 2 raw bits: distance <= 256
                    let reach = if len == 2 { 256 } else { window };
                    let max_dist = expect.len().min(reach) as u32;
                    let dist = 1 + rnd() % max_dist;
                    e.matched(len, dist);
                    for _ in 0..len {
                        let b = expect[expect.len() - dist as usize];
                        expect.push(b);
                    }
                }
            }
            let enc = e.finish();
            assert_eq!(decompress(&enc).unwrap(), expect, "dict_bits {dict_bits}");
        }
    }

    #[test]
    fn malformed_streams_are_errors_not_panics() {
        assert_eq!(decompress(&[0, 6]), Err(DclError::TruncatedHeader));
        assert_eq!(decompress(&[0, 3, 0]), Err(DclError::BadDictBits(3)));
        assert_eq!(decompress(&[0, 7, 0]), Err(DclError::BadDictBits(7)));
        assert_eq!(decompress(&[2, 6, 0]), Err(DclError::BadMode(2)));
        // mode 1 with nothing after the header: the first symbol cannot be read
        assert_eq!(decompress(&[1, 6, 0]), Err(DclError::Truncated));
        // a match before any output
        let mut e = Enc::new(6);
        e.matched(4, 1);
        assert_eq!(decompress(&e.finish()), Err(DclError::BadDistance));
        // every proper prefix of a valid stream must fail cleanly
        let mut e = Enc::new(5);
        for &b in b"hello hello hello" {
            e.literal(b);
        }
        let full = e.finish();
        for n in 0..full.len() {
            let _ = decompress(&full[..n]); // Err or Ok, never a panic
        }
    }

    /// Decoded size and byte sum of three real files (one is a saved game);
    /// the sums were computed independently with the Python prototype of the
    /// decoder. Skipped when the install is absent.
    #[test]
    fn golden_real_files() {
        let root = crate::corpus::install_root();
        let cases: [(&str, usize, u64); 3] = [
            ("conquests.biq", 209_222, 0x56_866f),
            ("civ3mod.bic", 111_308, 0x31_0158),
            ("EGYPT.SAV", 1_748_113, 0x8d5_b8cf),
        ];
        fn find(dir: &std::path::Path, name: &str) -> Option<std::path::PathBuf> {
            for e in std::fs::read_dir(dir).ok()?.flatten() {
                let p = e.path();
                let n = p.file_name()?.to_str()?.to_string();
                if p.is_dir() {
                    if !n.starts_with('.')
                        && n != "re"
                        && let Some(f) = find(&p, name)
                    {
                        return Some(f);
                    }
                } else if n.eq_ignore_ascii_case(name) {
                    return Some(p);
                }
            }
            None
        }
        for (name, len, sum) in cases {
            let Some(path) = find(&root, name) else {
                continue;
            };
            let input = std::fs::read(&path).unwrap();
            assert!(looks_compressed(&input), "{name}");
            let out = decompress(&input).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(out.len(), len, "{name}");
            assert_eq!(out.iter().map(|&b| b as u64).sum::<u64>(), sum, "{name}");
        }
    }
}

#[cfg(test)]
mod ascii_tests {
    use super::*;

    /// Minimal PE reader: the bytes at virtual address `va` of the exe image.
    fn exe_bytes(exe: &[u8], va: u32, len: usize) -> Vec<u8> {
        let u16at = |o: usize| u16::from_le_bytes([exe[o], exe[o + 1]]) as usize;
        let u32at = |o: usize| u32::from_le_bytes([exe[o], exe[o + 1], exe[o + 2], exe[o + 3]]);
        let pe = u32at(0x3C) as usize;
        let sections = u16at(pe + 6);
        let opt_size = u16at(pe + 20);
        let image_base = u32at(pe + 24 + 28);
        for i in 0..sections {
            let at = pe + 24 + opt_size + 40 * i;
            let virt_size = u32at(at + 8);
            let section_va = image_base + u32at(at + 12);
            let raw_size = u32at(at + 16);
            let raw = u32at(at + 20) as usize;
            if (section_va..section_va + virt_size.max(raw_size)).contains(&va) {
                let start = raw + (va - section_va) as usize;
                return exe[start..start + len].to_vec();
            }
        }
        panic!("va {va:#x} is in no section");
    }

    /// Every table constant against the bytes of `Civ3Conquests.exe`, in *both*
    /// copies the exe keeps (the implode tables used by the compressor at
    /// `0x648920`, the explode tables used by `0x649400`). Skipped without the
    /// install.
    #[test]
    fn tables_are_the_exes_bytes_in_both_copies() {
        let root = crate::corpus::install_root();
        let Some(exe) = [
            "civ3-gog/app/Conquests/Civ3Conquests.exe",
            "re/Civ3Conquests.exe",
        ]
        .iter()
        .find_map(|p| std::fs::read(root.join(p)).ok()) else {
            return;
        };
        let words = |b: Vec<u8>| -> Vec<u16> {
            b.chunks(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect()
        };
        // (constant, explode copy, implode copy)
        let bytes: [(&[u8], u32, u32); 6] = [
            (&LEN_EXTRA_CNT, 0x73A500, 0x73A068),
            (&LEN_LOCODE, 0x73A510, 0x73A078),
            (&SHORT_EXTRA, 0x73A4D0, 0x73A058),
            (&DIST_LEN, 0x73A450, 0x739FD8),
            (&DIST_LOCODE, 0x73A490, 0x73A018),
            (&CH_BITS_ASC, 0x73A520, 0x73A088),
        ];
        for (table, explode, implode) in bytes {
            assert_eq!(
                table,
                exe_bytes(&exe, explode, table.len()),
                "explode {explode:#x}"
            );
            assert_eq!(
                table,
                exe_bytes(&exe, implode, table.len()),
                "implode {implode:#x}"
            );
        }
        assert_eq!(SHORT_BASE.to_vec(), words(exe_bytes(&exe, 0x73A4E0, 32)));
        assert_eq!(
            CH_CODE_ASC.to_vec(),
            words(exe_bytes(&exe, 0x73A620, 512)),
            "explode 0x73A620"
        );
        assert_eq!(
            CH_CODE_ASC.to_vec(),
            words(exe_bytes(&exe, 0x73A188, 512)),
            "implode 0x73A188"
        );
    }

    /// The mode-1 literal table is an exact complete prefix code: every length
    /// fits `ASCII_PEEK`, Kraft's sum is exactly 1, every code fits its length,
    /// and every 13-bit window decodes to exactly one byte.
    #[test]
    fn ascii_table_is_a_complete_prefix_code() {
        let kraft: u64 = CH_BITS_ASC
            .iter()
            .map(|&b| 1u64 << (ASCII_PEEK - b as u32))
            .sum();
        assert_eq!(kraft, 1 << ASCII_PEEK, "Kraft sum must be exactly 1");
        assert_eq!(*CH_BITS_ASC.iter().max().unwrap() as u32, ASCII_PEEK);
        for b in 0..256 {
            assert!((CH_CODE_ASC[b] as u32) < 1 << CH_BITS_ASC[b], "byte {b}");
        }
        let table = ascii_decoder();
        let mut hits = vec![0u32; 256];
        for (window, &(byte, len)) in table.iter().enumerate() {
            assert!(len > 0, "window {window:#x} decodes to nothing");
            assert_eq!(
                window & ((1 << len) - 1),
                CH_CODE_ASC[byte as usize] as usize,
                "window {window:#x}"
            );
            assert_eq!(len, CH_BITS_ASC[byte as usize]);
            hits[byte as usize] += 1;
        }
        // a code of `len` bits owns 2^(13 - len) windows
        for b in 0..256 {
            assert_eq!(
                hits[b],
                1 << (ASCII_PEEK - CH_BITS_ASC[b] as u32),
                "byte {b}"
            );
        }
    }
}
