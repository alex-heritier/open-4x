//! The terrain-art stage: `postProcess` (`Map` vtable slot `0x80`, `0x5EBE80`).
//!
//! After the rivers, `generateMap` calls `postProcess`, which is two passes over
//! every cell and an easter egg (a `GetLocalTime` test that moves a file called
//! `ExtrasJaimo.zip`; it touches no map data):
//!
//! 1. [`fix_borders`] (`0x5EBF30`) removes two shapes the art cannot draw:
//!    tundra beside plains or desert, and deep water beside land.
//! 2. [`assign_art`] (`0x5EC2D0`) chooses the sprite of every cell, the bytes
//!    `Cell+0x10` (*image*) and `Cell+0x11` (*file*) that a `.biq` `TILE` row and
//!    a save keep. In a few places it first edits the terrain.
//!
//! # Sprites
//!
//! The terrain sheets are drawn from the **diamond above a cell**: the cell
//! itself is its bottom corner, the corners left and right are the tiles at
//! `(-1, -1)` and `(+1, -1)` and the top is the tile at `(0, -2)` ([`CORNERS`]).
//! A corner's *base terrain* is the sub-class nibble of its terrain word (0
//! desert, 1 plains, 2 grassland, 3 tundra, and 11, 12, 13 for the three water
//! depths; relief and forest keep the base terrain of the land under them), and
//! a corner off the map counts as 11.
//!
//! The *file* picks a sheet and the *image* is the cell in it. Almost always the
//! image is a base-3 number with one digit per corner, the corner `k` having the
//! weight `3^k` ([`pack`]), so a sheet has up to 81 pictures:
//!
//! | file | sheet | digit 0 | digit 1 | digit 2 |
//! |---|---|---|---|---|
//! | 0 | tundra | tundra | grassland | anything else |
//! | 1 | plains | plains | grassland | anything else |
//! | 2, 4 | desert | desert | grassland | anything else |
//! | 3 | desert-plains | desert | plains | anything else |
//! | 5 | grassland, coast | clear | relief or unclean | water |
//! | 6 | water | coast (11) | sea (12) | ocean (13) |
//! | 7 | all sea | random of 81 | | |
//! | 8 | all ocean | random of 81 | | |
//!
//! Which sheet depends on the set of base terrains in the diamond (`distinct`
//! kinds, and which); several kinds of file are picked at random per cell
//! ([`choose_art`] has the whole tree, with the `0x5EC2D0` address of each case).
//!
//! # Four different corners
//!
//! A diamond whose four corners are four different base terrains has no sprite.
//! The stage then edits the terrain, once per cell, in this order, and looks at
//! the diamond again ([`repair`]):
//!
//! 1. **First try**: the cell takes the base terrain of its upper neighbour on
//!    one side if both are land or both water;
//! 2. **second try**: the same with the upper neighbour on the other side;
//! 3. **third try**: the cell takes the base terrain of the tile two rows up
//!    whatever it is, a water tile two rows down becomes coast, and the
//!    continents will be renumbered.
//!
//! A diamond that is still four-coloured gets the file 6 image `0x50` (an
//! out-of-range picture of the water sheet). The binary prints
//! `Fixed on 1st try` and so on to the debugger.
//!
//! # Randomness
//!
//! Each cell has its own LCG, `seed + 101 * cell + 0x1D9D3`, warmed up by five
//! draws. A second one, seeded the same way for the corner's own cell, decides
//! the "unclean" corners of the coast sheet.
//!
//! The generator clears feature bit `0x400000` (a scratch flag [`fix_borders`]
//! reads) of every cell right after this stage; no earlier stage sets it, so
//! the branches that read it never run during a generation.

use crate::cell::{MapGrid, PLANE_FEATURE};
use crate::continents::number_continents;
use crate::rng::Rng;
use crate::spiral::spiral_offset;

/// The four corners of the diamond above a cell, as `(dx, dy)` from the cell.
pub const CORNERS: [(i32, i32); 4] = [(0, -2), (-1, -1), (1, -1), (0, 0)];
/// Per-cell seed: `seed + STRIDE * cell + SEED_BASE`.
pub const SEED_BASE: u32 = 0x1D9D3;
/// See [`SEED_BASE`].
pub const SEED_STRIDE: u32 = 101;
/// Warm-up draws of the per-cell generator.
pub const WARMUP: usize = 5;
/// Feature-plane scratch bit read by [`fix_borders`] and cleared after the stage.
pub const SCRATCH: u32 = 0x40_0000;
/// The file and image a four-coloured diamond ends up with.
pub const FALLBACK: (i32, i32) = (6, 0x50);

/// The sprite of a cell: `(file, image)`.
pub type Art = (i32, i32);

/// A base-3 image number: `digits[k]` is the digit of corner `k`.
pub fn pack(digits: [i32; 4]) -> i32 {
    digits[0] + 3 * digits[1] + 9 * digits[2] + 27 * digits[3]
}

fn cell_rng(seed: u32, cell: u32) -> Rng {
    let mut rng = Rng::new(seed.wrapping_add(SEED_STRIDE.wrapping_mul(cell)).wrapping_add(SEED_BASE));
    rng.discard(WARMUP);
    rng
}

/// The base terrain of corner `n` of the diamond above `(x, y)`; 11 off the map.
fn corner_sub(grid: &MapGrid, x: i32, y: i32, n: usize) -> i32 {
    let (dx, dy) = CORNERS[n];
    grid.wrap_and_check(x + dx, y + dy)
        .and_then(|(cx, cy)| grid.cell_at(cx, cy))
        .map_or(11, |c| i32::from(c.sub_class()))
}

/// `0x5EBF30`: no tundra beside plains or desert, no deep water beside land.
///
/// Looks at the eight neighbours (`spiral_offset(1..=8)`) of every cell, in
/// order. A cell whose base terrain is tundra (3) and which has a land
/// neighbour that is neither tundra nor grassland is made grassland (a forest
/// stays a forest), and the scan of that cell stops. A sea or ocean cell (base
/// 12 or 13) with a land neighbour is made coast (11) the same way. If the cell
/// has the scratch feature bit [`SCRATCH`], the *neighbour* is changed instead
/// and the scan goes on.
pub fn fix_borders(grid: &mut MapGrid) {
    for i in 0..grid.num_cells() {
        let sub = grid.cells[i].sub_class();
        if sub != 3 && sub != 12 && sub != 13 {
            continue;
        }
        let (x, y) = grid.coords(i);
        for n in 1..=8 {
            let (dx, dy) = spiral_offset(n);
            let Some((nx, ny)) = grid.wrap_and_check(x + dx, y + dy) else { continue };
            let Some(nb) = grid.cell_at(nx, ny) else { continue };
            if nb.is_water() {
                continue;
            }
            if sub == 3 && (nb.sub_class() == 3 || nb.sub_class() == 2) {
                continue;
            }
            let flagged = grid.cells[i].flag(PLANE_FEATURE) & SCRATCH != 0;
            let target = if flagged { grid.index(nx, ny) } else { i };
            let cell = &mut grid.cells[target];
            if sub == 3 {
                let forest = cell.class() == 7;
                cell.set_class(2);
                if forest {
                    cell.set_class(7);
                }
            } else {
                cell.set_class(11);
            }
            if !flagged {
                break;
            }
        }
    }
}

/// `0x5EC2D0`: the sprite of every cell. Returns whether a third-try repair
/// changed the terrain, in which case the continents have been renumbered
/// (`Map` vtable slot `0x7C`, `0x5EC86E`).
pub fn assign_art(grid: &mut MapGrid, seed: i32) -> bool {
    let seed = seed as u32;
    let mut renumber = false;
    for i in 0..grid.num_cells() {
        let (x, y) = grid.coords(i);
        let mut rng = cell_rng(seed, i as u32 & 0xFFFF);
        let mut attempt = 0;
        let (file, image) = loop {
            let sub: [i32; 4] = std::array::from_fn(|n| corner_sub(grid, x, y, n));
            let mut hist = [0u32; 14];
            for &s in &sub {
                hist[(s as usize).min(13)] += 1;
            }
            let distinct = hist.iter().filter(|&&h| h != 0).count();
            if distinct < 4 {
                break choose_art(grid, &mut rng, seed, x, y, sub, &hist, distinct);
            }
            if attempt == 0 {
                renumber |= repair(grid, i, x, y);
            }
            attempt += 1;
            if attempt == 2 {
                break FALLBACK;
            }
        };
        // `vfunc(0xD8)(file << 8 | image)` stores one dword at `Cell+0x10`:
        // the image byte, the file byte, and two bytes that stay zero.
        let word = (file << 8) | (image & 0xFFFF);
        let c = &mut grid.cells[i];
        c.image = (word & 0xFF) as u8;
        c.file = ((word >> 8) & 0xFF) as u8;
    }
    if renumber {
        number_continents(grid);
    }
    renumber
}

/// The terrain edit for a diamond with four different corners (`0x5EC478`).
/// Returns whether the third try ran, the only one that asks for a renumbering.
fn repair(grid: &mut MapGrid, i: usize, x: i32, y: i32) -> bool {
    let parity = y & 1;
    let same_kind = |grid: &MapGrid, nx: i32, ny: i32| -> Option<u8> {
        let nb = grid.wrap_and_check(nx, ny).and_then(|(cx, cy)| grid.cell_at(cx, cy))?;
        (nb.is_water() == grid.cells[i].is_water()).then(|| nb.sub_class())
    };
    // First try, then second try: the upper neighbour on either side.
    let first = x + if parity != 0 { 1 } else { -1 };
    let second = x + if parity == 0 { 1 } else { -1 };
    for nx in [first, second] {
        if let Some(sub) = same_kind(grid, nx, y - 1) {
            grid.cells[i].set_class(sub);
            return false;
        }
    }
    // Third try: the tile two rows up, whatever it is.
    let Some(sub) = grid
        .wrap_and_check(x, y - 2)
        .and_then(|(cx, cy)| grid.cell_at(cx, cy))
        .map(|c| c.sub_class())
    else {
        return false;
    };
    grid.cells[i].set_class(sub);
    if let Some((bx, by)) = grid.wrap_and_check(x, y + 2) {
        if let Some(below) = grid.cell_at_mut(bx, by) {
            if below.is_water() {
                below.set_class(11);
            }
        }
    }
    true
}

/// The sheet digit of a corner with base terrain `sub` in sheet `file`
/// (`0x5ECD7C`): 0 for the sheet's own terrain, 1 for its second, else 2.
fn digit(file: i32, sub: i32) -> i32 {
    let (zero, one) = match file {
        0 => (3, 2),
        1 => (1, 2),
        3 => (0, 1),
        _ => (0, 2),
    };
    if sub == zero {
        0
    } else if sub == one {
        1
    } else {
        2
    }
}

/// The decision tree of `0x5EC88A`: the sprite of a diamond with fewer than four
/// kinds of corner. `hist[k]` counts the corners whose base terrain is `k`.
#[allow(clippy::too_many_arguments)]
fn choose_art(
    grid: &MapGrid,
    rng: &mut Rng,
    seed: u32,
    x: i32,
    y: i32,
    sub: [i32; 4],
    hist: &[u32; 14],
    distinct: usize,
) -> Art {
    let water = hist[11] + hist[12] + hist[13];

    // Four water corners: the water sheet; the pure seas have 81 random pictures.
    if water >= 4 {
        let image = pack(sub.map(|s| match s {
            12 => 1,
            13 => 2,
            _ => 0,
        }));
        return match image {
            0x50 => (8, rng.below(0x51)),
            0x28 => (7, rng.below(0x51)),
            _ => (6, image),
        };
    }

    let file = if hist[3] != 0 {
        0
    } else if distinct == 3 {
        if water == 0 {
            4
        } else if hist[2] == 0 {
            3
        } else if hist[0] != 0 {
            2
        } else {
            1
        }
    } else if distinct == 2 {
        if hist[2] != 0 {
            if hist[0] != 0 {
                2 * rng.below(2) + 2
            } else if hist[1] != 0 {
                3 * rng.below(2) + 1
            } else {
                // Grassland and water: the coast sheet when some corner is clean.
                let fallback = rng.below(3);
                let (flags, any_clear) = coast_flags(grid, seed, x, y, sub);
                if any_clear {
                    return (5, pack(flags));
                }
                fallback
            }
        } else if hist[0] != 0 {
            if hist[1] != 0 {
                rng.below(2) + 3
            } else {
                rng.below(2) + 2
            }
        } else if hist[1] != 0 {
            2 * rng.below(2) + 1
        } else {
            -1
        }
    } else if hist[2] != 0 {
        // Four grassland corners: the grassland sheet, a random picture.
        let r: [i32; 4] = std::array::from_fn(|_| rng.below(2));
        return (5, pack(r));
    } else if hist[0] != 0 {
        rng.below(3) + 2
    } else if hist[1] != 0 {
        // Four plains corners: sheets 1, 3 or 4.
        let f = rng.below(3) + 1;
        if f < 2 {
            f
        } else {
            f + 1
        }
    } else {
        -1
    };

    (file, pack(sub.map(|s| digit(file, s))))
}

/// The corner flags of the coast sheet (file 5, `0x5EC963..0x5ECC84`) for a
/// diamond of grassland and water: 2 for a water corner, 1 for a land corner
/// that is relief (hills, mountains, volcano) or has any non-coast tile within
/// its 3x3 (`spiral_offset(0..9)`) that is relief or not grassland-based, or
/// that a 1-in-4 draw marks anyway; 0 for a clean one. Also whether any corner
/// is clean (a clean corner that the draw marks does not count).
fn coast_flags(grid: &MapGrid, seed: u32, x: i32, y: i32, sub: [i32; 4]) -> ([i32; 4], bool) {
    let relief = |class: u8| matches!(class, 5 | 6 | 10);
    let half = grid.w >> 1;
    let mut flags = [0; 4];
    let mut any_clear = false;
    for n in 0..4 {
        let (dx, dy) = CORNERS[n];
        let Some((cx, cy)) = grid.wrap_and_check(x + dx, y + dy) else { continue };
        let Some(corner) = grid.cell_at(cx, cy) else { continue };
        if sub[n] >= 11 {
            flags[n] = 2;
            continue;
        }
        if relief(corner.class()) {
            flags[n] = 1;
            continue;
        }
        let mut clear = true;
        for k in 0..9 {
            let (ox, oy) = spiral_offset(k);
            let Some((ax, ay)) = grid.wrap_and_check(cx + ox, cy + oy) else { continue };
            let Some(nb) = grid.cell_at(ax, ay) else { continue };
            if nb.class() == 11 {
                continue;
            }
            if nb.sub_class() != 2 || relief(nb.class()) {
                clear = false;
                flags[n] = 1;
            }
        }
        if clear {
            let idx = ((half * cy + i32::from((cx as u16) >> 1)) & 0xFFFF) as u32;
            if cell_rng(seed, idx).below(4) == 0 {
                clear = false;
                flags[n] = 1;
            }
        }
        any_clear |= clear;
    }
    (flags, any_clear)
}

/// `0x5EBE80`: the whole stage, followed by what `generateMap` does next, the
/// clearing of [`SCRATCH`] on every cell.
pub fn post_process(grid: &mut MapGrid, seed: i32) {
    fix_borders(grid);
    assign_art(grid, seed);
    for c in grid.cells.iter_mut() {
        c.clear_flag(PLANE_FEATURE, SCRATCH);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn images_are_base_three_numbers() {
        assert_eq!(pack([0, 0, 0, 0]), 0);
        assert_eq!(pack([1, 1, 1, 1]), 40);
        assert_eq!(pack([2, 2, 2, 2]), 80);
        assert_eq!(pack([0, 0, 0, 1]), 27);
    }

    #[test]
    fn a_sheet_digit_names_its_own_terrain_first() {
        // Tundra sheet: tundra 0, grassland 1, everything else 2.
        assert_eq!([3, 2, 1, 0, 11].map(|s| digit(0, s)), [0, 1, 2, 2, 2]);
        // Plains sheet.
        assert_eq!([1, 2, 0, 11].map(|s| digit(1, s)), [0, 1, 2, 2]);
        // Desert-plains sheet.
        assert_eq!([0, 1, 2, 11].map(|s| digit(3, s)), [0, 1, 2, 2]);
        // Desert sheets 2 and 4.
        assert_eq!([0, 2, 1, 11].map(|s| digit(2, s)), [0, 1, 2, 2]);
        assert_eq!([0, 2, 1, 11].map(|s| digit(4, s)), [0, 1, 2, 2]);
    }

    #[test]
    fn the_diamond_is_the_cell_and_the_three_above() {
        assert_eq!(CORNERS[3], (0, 0));
        assert!(CORNERS[..3].iter().all(|&(_, dy)| dy < 0));
    }
}
