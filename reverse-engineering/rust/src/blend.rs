//! Terrain display and blending: sprite addressing and neighbor masks.
//!
//! See `../blending.md`. Load-time dicing and the runtime addressing
//! formula are verified instruction-level; the sheet/cell *selection*
//! inputs are marked where still open.

/// Base sheets in load order (`0x4C5A55` loop, stride `0x104` over the
/// 260-byte `.rdata` path entries, 9 entries ending before `0x6699D4`).
pub const BASE_SHEETS: &[&str] = &[
    "xtgc", "xpgc", "xdgc", "xdpc", "xdgp", "xggc", "wCSO", "wSSS", "wOOO",
];

/// Expansion (LM) sheets in load order (`0x4C5B31` loop, same shape).
pub const LM_SHEETS: &[&str] = &[
    "lxtgc", "lxpgc", "lxdgc", "lxdpc", "lxdgp", "lxggc", "lwCSO", "lwSSS", "lwOOO",
];

/// Cells per sheet: every base/LM sheet is diced 9x9 (`0x5F7F90` with
/// 128x64: `esi` 0x80..0x480, `ebp` 0x40..0x240).
pub const CELLS_PER_SHEET: u32 = 81;
/// Bytes per diced cell record (`edi` advances 0x2C per cell).
pub const CELL_STRIDE: u32 = 44;
/// Bytes per sheet block (`ecx` advances 0xDEC per sheet = 81 cells).
pub const SHEET_STRIDE: u32 = CELLS_PER_SHEET * CELL_STRIDE;

/// Sprite-record byte offset for `(sheet, cell)`.
///
/// `0x4C3880`: `(sheet*81 + cell) * 11 * 4` = `(sheet*81 + cell) * 44`.
/// The caller passes the table base in `edi`: normal set at view
/// `+0x191A4`, LM set at `+0x102D0` (switch on `byte[0x9C7340]`).
pub fn sprite_offset(sheet: u32, cell: u32) -> u32 {
    (sheet * CELLS_PER_SHEET + cell) * CELL_STRIDE
}

/// Hills overlay sheet: `xhills.pcx` (512x288) diced 4x4 of 128x72 —
/// 8 px taller than a diamond so ridges overlap the tile above.
/// Loaded to view `+0xBCC`.
pub const HILLS_COLS: u32 = 4;
/// Hills sheet rows.
pub const HILLS_ROWS: u32 = 4;
/// Hills cell height in px (overlaps the tile above by 8 px).
pub const HILLS_CELL_H: u32 = 72;

/// Polar ice sheet: 1024x256 diced 8x4 of 128x64.
pub const POLAR_COLS: u32 = 8;
/// Polar ice sheet rows.
pub const POLAR_ROWS: u32 = 4;

/// Tile-diamond size in px (base sheets, rivers, roads, ice).
pub const TILE_W: u32 = 128;
/// Tile-diamond height in px.
pub const TILE_H: u32 = 64;

/// The four neighbors sampled by the blend-mask builder (`0x4C34CA`):
/// `(x, y-1)`, `(x-1, y)`, `(x+1, y)`, `(x, y+1)`, i.e. N/W/E/S in tile
/// coords. Per direction `edi`: `nx = x + edi/2 - (edi&1)`,
/// `ny = y + (edi+1)/2 - 1`, each wrapped when its `+0x1F0` flag is set.
pub const NEIGHBOR_OFFSETS: [(i32, i32); 4] = [(0, -1), (-1, 0), (1, 0), (0, 1)];

/// Halved-coordinate cell fetch index, shared with the AI step guards:
/// `idx = (x>>1) + (W>>1)*y`, masked to 16 bits (`0x4C3567`, `0x4C35D9`).
pub fn fetch_index(x: i32, y: i32, w: i32) -> u32 {
    (((x >> 1) + ((w >> 1) * y)) & 0xFFFF) as u32
}

/// One direction bit of the blend mask: bit `bitpos` of the neighbor
/// cell's `+0x58` flag word (`test [eax+0x58], 1<<bit`, `setne`).
pub fn direction_bit(flags58: u32, bitpos: u32) -> u32 {
    u32::from(flags58 & (1 << bitpos) != 0)
}

/// Blend-mask accumulation over the four directions (`or [esp+0x54], eax`
/// with `shl eax, edi`): direction `i` contributes bit `i`.
pub fn blend_mask(bits: [u32; 4]) -> u32 {
    bits[0] | (bits[1] << 1) | (bits[2] << 2) | (bits[3] << 3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addressing_matches_loader_strides() {
        // Sheet stride and cell stride reproduce the loader immediates.
        assert_eq!(SHEET_STRIDE, 0xDEC);
        assert_eq!(CELL_STRIDE, 0x2C);
        assert_eq!(BASE_SHEETS.len(), 9);
        assert_eq!(LM_SHEETS.len(), 9);
        // (sheet*81+cell)*44, spot-check the formula boundaries.
        assert_eq!(sprite_offset(0, 0), 0);
        assert_eq!(sprite_offset(0, 80), 80 * 44);
        assert_eq!(sprite_offset(1, 0), 81 * 44);
        assert_eq!(sprite_offset(8, 80), (8 * 81 + 80) * 44);
        // Whole table size: 9 sheets * 81 cells * 44 bytes.
        assert_eq!(sprite_offset(8, 80) + 44, 9 * 81 * 44);
    }

    #[test]
    fn sheet_tables_cover_land_and_water() {
        // Six land sheets, three water sheets, same order as the loader.
        assert_eq!(&BASE_SHEETS[6..], &["wCSO", "wSSS", "wOOO"]);
        assert_eq!(&LM_SHEETS[6..], &["lwCSO", "lwSSS", "lwOOO"]);
    }

    #[test]
    fn neighbor_offsets_match_loop_math() {
        // nx = x + edi/2 - (edi&1); ny = y + (edi+1)/2 - 1.
        for (edi, (ox, oy)) in NEIGHBOR_OFFSETS.iter().enumerate() {
            let edi = edi as i32;
            assert_eq!(edi / 2 - (edi & 1), *ox);
            assert_eq!((edi + 1) / 2 - 1, *oy);
        }
    }

    #[test]
    fn mask_builder() {
        // Bit test + 4-direction accumulation.
        assert_eq!(direction_bit(0xFF, 3), 1);
        assert_eq!(direction_bit(0xF7, 3), 0);
        assert_eq!(blend_mask([1, 0, 1, 1]), 0b1101);
        assert_eq!(blend_mask([0, 0, 0, 0]), 0);
    }

    #[test]
    fn hills_and_polar_geometry() {
        // xhills 512x288 = 4x4 of 128x72; polar 1024x256 = 8x4 of 128x64.
        assert_eq!(HILLS_COLS * TILE_W, 512);
        assert_eq!(HILLS_ROWS * HILLS_CELL_H, 288);
        assert!(HILLS_CELL_H > TILE_H); // overlaps the tile above
        assert_eq!(POLAR_COLS * TILE_W, 1024);
        assert_eq!(POLAR_ROWS * TILE_H, 256);
    }
}
