//! PKWARE DCL decompressor (`0x649400` family): the `.biq`/`.bic` codec.
//!
//! Recovered instruction-by-instruction from `0x649400` (init), `0x649580`
//! (main loop), `0x649680` (symbol), `0x649830` (distance), `0x6498B0`
//! (bit reader), `0x649940` (peek-table expand), `0x649980` (tree build).
//! See `../biq.md`. Every constant cites its address.
//!
//! Two corrections to `NOTES.md` §15 apply here: the bit order is
//! **LSB-first** (`shr` consumption at `0x6498CA`), not MSB-first, and the
//! stream terminator is symbol **`0x305`**, not `0x30E`.
//!
//! Mode 0 (binary: raw 8-bit literals, short-tree lengths) is exact and
//! verified against both shipped scenario files. Mode 1 (tree literals) is
//! implemented per disassembly but **UNVERIFIED**: no mode-1 file ships
//! locally, and the `0x649980` build writes `0xFF` markers at full-code
//! indices that can overlap the lens copies — mirrored here by using the
//! exe's exact flat table layout, benign or not.
//!
//! The scenario dispatcher (`0x594290`) inventory and magic gate live here
//! too (`SCENARIO_TAGS`, `magic_valid`), since the tag set defines what a
//! decompressed stream may contain.

/// Scenario section tags accepted by the `0x594290` dispatcher, in raw
/// `3D`-scan encounter order (28 tags; `CULT` compares twice).
pub const SCENARIO_TAGS: &[&str] = &[
    "PRTO", "GAME", "GOOD", "VER#", "SLOC", "LEAD", "RACE", "TILE", "RULE",
    "TFRM", "DIFF", "BLDG", "TECH", "ESPN", "CTZN", "CULT", "TERR", "WMAP",
    "WCHR", "EXPR", "ERAS", "UNIT", "CLNY", "CONT", "GOVT", "FLAV", "CITY",
    "WSIZ",
];

/// Scenario magic gate (`0x59432D` ff): exactly `BIC `/`BICX`/`BICQ`.
/// No `BIX `/`BIQ `/`CIV3` compares exist anywhere in `.text`.
pub fn magic_valid(magic: &[u8; 4]) -> bool {
    matches!(magic, b"BIC " | b"BICX" | b"BICQ")
}

/// Per-record stride of the three decoded loader workers
/// (`biq.md`): UNIT `0x7C`, BLDG `0x110`, PRTO `0x138`.
/// All other tags: unknown (`None`).
pub fn loader_stride(tag: &str) -> Option<u32> {
    match tag {
        "UNIT" => Some(0x7C),
        "BLDG" => Some(0x110),
        "PRTO" => Some(0x138),
        _ => None,
    }
}

/// Code lengths for the 256 literal symbols (`0x73A520`).
pub const LIT_LEN: [u8; 256] = [
    11, 12, 12, 12, 12, 12, 12, 12, 12, 8, 7, 12, 12, 7, 12, 12,
    12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 13, 12, 12, 12, 12, 12,
    4, 10, 8, 12, 10, 12, 10, 8, 7, 7, 8, 9, 7, 6, 7, 8,
    7, 6, 7, 7, 7, 7, 8, 7, 7, 8, 8, 12, 11, 7, 9, 11,
    12, 6, 7, 6, 6, 5, 7, 8, 8, 6, 11, 9, 6, 7, 6, 6,
    7, 11, 6, 6, 6, 7, 9, 8, 9, 9, 11, 8, 11, 9, 12, 8,
    12, 5, 6, 6, 6, 5, 6, 6, 6, 5, 11, 7, 5, 6, 5, 5,
    6, 10, 5, 5, 5, 5, 8, 7, 8, 8, 10, 11, 11, 12, 12, 12,
    13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13,
    13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13,
    13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13,
    12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12,
    12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12,
    12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12,
    13, 12, 13, 13, 13, 12, 13, 13, 13, 12, 13, 13, 13, 13, 12, 13,
    13, 13, 12, 12, 12, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13, 13,
];
/// Code lengths for the 64 distance classes (`0x73A450`).
pub const DIST_LEN: [u8; 64] = [
    2, 4, 4, 5, 5, 5, 5, 6, 6, 6, 6, 6, 6, 6, 6, 6,
    6, 6, 6, 6, 6, 6, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
    7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7, 7,
    8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8,
];
/// Extra-bit count per length class (`0x73A500`).
pub const LEN_EXTRA_CNT: [u8; 16] = [
    3, 2, 3, 3, 4, 4, 4, 5, 5, 5, 5, 6, 6, 6, 7, 7,
];
/// Low canonical code per length class (`0x73A510`).
pub const LEN_LOCODE: [u8; 16] = [
    5, 3, 1, 6, 10, 2, 12, 20, 4, 24, 8, 48, 16, 32, 64, 0,
];
/// Extra bits in the short-tree length code (`0x73A4D0`).
pub const SHORT_EXTRA: [u8; 16] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8,
];
/// Short-tree length bases (`0x73A4E0`, 16 u16).
pub const SHORT_BASE: [u16; 16] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 14, 22, 38, 70, 134, 262,
];
/// Low canonical code per distance class (`0x73A490`).
pub const DIST_LOCODE: [u8; 64] = [
    3, 13, 5, 25, 9, 17, 1, 62, 30, 46, 14, 54, 22, 38, 6, 58,
    26, 42, 10, 50, 18, 34, 66, 2, 124, 60, 92, 28, 108, 44, 76, 12,
    116, 52, 84, 20, 100, 36, 68, 4, 120, 56, 88, 24, 104, 40, 72, 8,
    240, 112, 176, 48, 208, 80, 144, 16, 224, 96, 160, 32, 192, 64, 128, 0,
];
/// Canonical codes for the mode-1 literal tree (`0x73A620`, 256 u16).
/// Index pairs with `LIT_LEN` by symbol: `TREE_CODES[sym]` is sym's code.
pub const TREE_CODES: [u16; 256] = [
    1168, 4064, 2016, 3040, 992, 3552, 1504, 2528,
    480, 184, 98, 3808, 1760, 34, 2784, 736,
    3296, 1248, 2272, 224, 3936, 1888, 2912, 864,
    3424, 1376, 4672, 2400, 352, 3680, 1632, 2656,
    15, 592, 56, 608, 80, 3168, 912, 216,
    66, 2, 88, 432, 124, 41, 60, 152,
    92, 9, 28, 108, 44, 76, 24, 12,
    116, 232, 104, 1120, 144, 52, 176, 1808,
    2144, 49, 84, 17, 33, 23, 20, 168,
    40, 1, 784, 304, 62, 100, 30, 46,
    36, 1296, 14, 54, 22, 68, 48, 200,
    464, 208, 272, 72, 1552, 336, 96, 136,
    4000, 7, 38, 6, 58, 27, 26, 42,
    10, 11, 528, 4, 19, 50, 3, 29,
    18, 400, 13, 21, 5, 25, 8, 120,
    240, 112, 656, 1040, 16, 1952, 2976, 928,
    576, 7232, 3136, 5184, 1088, 6208, 2112, 4160,
    64, 8064, 3968, 6016, 1920, 7040, 2944, 4992,
    896, 7552, 3456, 5504, 1408, 6528, 2432, 4480,
    384, 7808, 3712, 5760, 1664, 6784, 2688, 4736,
    640, 7296, 3200, 5248, 1152, 6272, 2176, 4224,
    128, 7936, 3840, 5888, 1792, 6912, 2816, 4864,
    3488, 1440, 2464, 416, 3744, 1696, 2720, 672,
    3232, 1184, 2208, 160, 3872, 1824, 2848, 800,
    3360, 1312, 2336, 288, 3616, 1568, 2592, 544,
    3104, 1056, 2080, 32, 4032, 1984, 3008, 960,
    3520, 1472, 2496, 448, 3776, 1728, 2752, 704,
    3264, 1216, 2240, 192, 3904, 1856, 2880, 832,
    768, 3392, 7424, 3328, 5376, 1344, 1280, 6400,
    2304, 2368, 4352, 256, 7680, 3584, 320, 5632,
    1536, 6656, 3648, 1600, 2624, 2560, 4608, 512,
    7168, 3072, 5120, 1024, 6144, 2048, 4096, 0,
];

/// End-of-stream symbol: class 15 + extra 255 (`0x649703` tolerance check
/// `class + extra == 0x10E`, i.e. `15 + 255`; returned as
/// `0x100 + 262 + 255`). The main loop (`0x6495A9`) ends on `sym >= 0x305`.
pub const TERMINATOR: u32 = 0x305;
/// Match length bias: `len = sym - 0xFE` (`0x6495C1`).
pub const LEN_BIAS: u32 = 0xFE;

/// LSB-first bit reader over the compressed stream (`0x6498B0`).
struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    bitbuf: u32,
    avail: u32,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8]) -> Self {
        Bits { data, pos: 0, bitbuf: 0, avail: 0 }
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

    /// Consume `n` bits; `None` on exhaustion. Values are peeked beforehand —
    /// the exe's call sites all peek-then-consume (`0x6496A9`, `0x649840`).
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

/// Decoder error.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DclError {
    /// Input shorter than the 3-byte header (`0x64943A`).
    TruncatedHeader,
    /// Dict bits outside 4..=6 (`0x649475`).
    BadDictBits(u8),
    /// Mode byte outside {0, 1} (`0x64949A`).
    BadMode(u8),
    /// Bit-stream exhaustion mid-symbol (genuine truncation).
    Truncated,
    /// Match distance beyond emitted output.
    BadDistance,
    /// Mode-1 tree literals: implemented per disassembly but unverified
    /// (no mode-1 file ships locally).
    Mode1Unverified,
}

/// Decompress one DCL stream (header at byte 0).
///
/// `dict_bits` must be 4..=6; `mask` derives from the shift byte as
/// `0xFFFF sar (16 - shift)` with x86 5-bit count masking (`0x64948C`).
pub fn decompress(input: &[u8]) -> Result<Vec<u8>, DclError> {
    if input.len() < 3 {
        return Err(DclError::TruncatedHeader);
    }
    let mode = input[0];
    let dict_bits = input[1];
    let shift = input[2];
    if !(4..=6).contains(&dict_bits) {
        return Err(DclError::BadDictBits(dict_bits));
    }
    if mode > 1 {
        return Err(DclError::BadMode(mode));
    }
    // x86 `sar eax, cl` masks the count to 5 bits (`0x64948E`).
    let count = (16u32.wrapping_sub(shift as u32)) & 31;
    let mask = 0xFFFFu32 >> count;

    let len_class = expand(16, &LEN_EXTRA_CNT, &LEN_LOCODE);
    let dist_class = expand(64, &DIST_LEN, &DIST_LOCODE);

    // The bit buffer is seeded with the shift byte itself (`0x64945C`:
    // `[esi+0x14] = byte2`), so the stream starts at byte 2, not byte 3.
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
        } else if mode == 0 {
            // Binary mode: raw literal byte (`0x64974A`).
            let lit = bits.peek(8);
            bits.get(8).ok_or(DclError::Truncated)?;
            lit
        } else {
            return Err(DclError::Mode1Unverified);
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

    fn crate_root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
    }

    fn read_biq(name: &str) -> Vec<u8> {
        // Workspace GOG install, mirroring the game's own data path.
        let p = crate_root().join("../../../civ3-gog/app/Conquests").join(name);
        std::fs::read(&p).unwrap_or_else(|_| panic!("missing test input {}", p.display()))
    }

    #[test]
    fn header_validation() {
        assert_eq!(decompress(&[]), Err(DclError::TruncatedHeader));
        assert_eq!(decompress(&[0, 3, 0]), Err(DclError::BadDictBits(3)));
        assert_eq!(decompress(&[2, 6, 0]), Err(DclError::BadMode(2)));
    }

    fn assert_scenario(out: &[u8]) {
        // Oracle: the scenario loader (0x594290) dispatches on 4-byte tags;
        // every shipped file opens with a BIC-family magic + VER# section.
        let magic = &out[..4];
        assert!(
            magic == b"BIQ " || magic == b"BIC " || magic == b"BIX " || magic == b"BICX",
            "unexpected magic {magic:?}"
        );
        let has = |tag: &[u8]| out.windows(4).any(|w| w == tag);
        assert!(has(b"VER#"), "no VER# section");
    }

    #[test]
    fn conquests_biq_opens_with_magic() {
        let out = decompress(&read_biq("conquests.biq")).expect("biq decodes");
        assert_scenario(&out);
        let has = |tag: &[u8]| out.windows(4).any(|w| w == tag);
        assert!(has(b"TERR"), "no TERR section");
        assert!(has(b"GOOD"), "no GOOD section");
        assert!(has(b"RULE"), "no RULE section");
    }

    #[test]
    fn compressed_save_opens_with_civ3_magic() {
        // Same 3-byte DCL framing as .biq; decompresses to a CIV3 save
        // stream carrying a full 20 000-tag TILE array.
        let p = crate_root().join("../../../civ3-gog/app/Conquests/Saves/EGYPT.SAV");
        let raw = std::fs::read(&p).unwrap_or_else(|_| panic!("missing {}", p.display()));
        let out = decompress(&raw).expect("sav decodes");
        assert_eq!(&out[..4], b"CIV3", "unexpected save magic {out:?}");
        let tiles = out.windows(4).filter(|w| *w == b"TILE").count();
        assert_eq!(tiles, 20_000, "got {tiles} TILE tags");
    }

    #[test]
    fn civ3mod_bic_opens_with_magic() {
        let raw = {
            let p = crate_root().join("../../../civ3-gog/app/civ3mod.bic");
            std::fs::read(&p).unwrap_or_else(|_| panic!("missing {}", p.display()))
        };
        let out = decompress(&raw).expect("bic decodes");
        assert_scenario(&out);
    }

    #[test]
    fn peek_tables_match_histograms() {
        // The 8-bit peek tables must partition all 256 values; spot-check
        // the two classes NOTES §15.3 pins down.
        let len_class = expand(16, &LEN_EXTRA_CNT, &LEN_LOCODE);
        assert_eq!(len_class.len(), 256);
        let dist_class = expand(64, &DIST_LEN, &DIST_LOCODE);
        assert_eq!(dist_class.len(), 256);
        // Kraft sums from the exe tables are exactly 1 (NOTES §15.3).
        let kraft = |lens: &[u8]| lens.iter().map(|&l| 2f64.powi(-(l as i32))).sum::<f64>();
        assert!((kraft(&LIT_LEN) - 1.0).abs() < 1e-9);
        assert!((kraft(&DIST_LEN) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn dispatcher_inventory_and_magic() {
        assert_eq!(SCENARIO_TAGS.len(), 28);
        assert!(SCENARIO_TAGS.contains(&"UNIT"));
        assert!(SCENARIO_TAGS.contains(&"TERR"));
        assert!(SCENARIO_TAGS.contains(&"FLAV"));
        assert!(magic_valid(b"BIC "));
        assert!(magic_valid(b"BICX"));
        assert!(magic_valid(b"BICQ"));
        assert!(!magic_valid(b"BIX "));
        assert!(!magic_valid(b"BIQ "));
        assert!(!magic_valid(b"CIV3"));
    }

    #[test]
    fn loader_strides_match_workers() {
        // Loop increments from the three worker bodies (biq.md).
        assert_eq!(loader_stride("UNIT"), Some(0x7C));
        assert_eq!(loader_stride("BLDG"), Some(0x110));
        assert_eq!(loader_stride("PRTO"), Some(0x138));
        assert_eq!(loader_stride("TERR"), None);
        assert_eq!(loader_stride("NOPE"), None);
    }
}
