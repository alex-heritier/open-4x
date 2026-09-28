//! Rivers: art path (verified) + render-path storage (traced live).
//!
//! Map generation places no rivers (`NOTES.md` §14). This module models the
//! verified art side — the `deltaRivers`/`RiverFore` sprite selection — and
//! the per-tile overlay gate traced live under Wine/winedbg (`0x5EAA80`
//! mask getter, gate table `0xA53BC8`, `[cell+0x2C]` nibble extractors).
//! See `../rivers.md`. Items beyond the traced path are marked
//! `HYPOTHESIS`.

/// Overlay mask byte: `byte[cell+5]`, read via vtable slot 38
/// (`0x5EAA80: mov al,[ecx+5]; ret`), three times per tile by the
/// `0x57Fxxx`/`0x580xxx` renderer. Low nibble shape (16 values) matches
/// the 16-cell river sheets; bit order still **HYPOTHESIS**.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct RiverMask(pub u8);

impl RiverMask {
    /// North edge bit. **HYPOTHESIS**: order unconfirmed.
    pub const NORTH: u8 = 0x1;
    /// East edge bit. **HYPOTHESIS**: order unconfirmed.
    pub const EAST: u8 = 0x2;
    /// South edge bit. **HYPOTHESIS**: order unconfirmed.
    pub const SOUTH: u8 = 0x4;
    /// West edge bit. **HYPOTHESIS**: order unconfirmed.
    pub const WEST: u8 = 0x8;

    /// Mask with no river edges.
    pub fn empty() -> Self {
        RiverMask(0)
    }

    /// True when no river edge is set.
    pub fn is_empty(self) -> bool {
        self.0 & 0xF == 0
    }

    /// Segment continuity: a river entering one edge must leave another.
    /// Single-bit masks are segment ends (sources/sinks).
    pub fn is_through(self) -> bool {
        (self.0 & 0xF).count_ones() >= 2
    }
}

/// Which river sheet to draw a segment from. Observed inventory:
/// `deltaRivers.pcx` (map table entry at `0x4C5FDA`), `mtnRivers.pcx`,
/// `waterfalls.pcx` in `Art/Terrain/`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RiverSheet {
    /// Lowland segments (`deltaRivers.pcx`, loaded at `0x4C5FDA`).
    Delta,
    /// Upland segments (`mtnRivers.pcx`, on disk, no `.text` ref).
    Mountain,
    /// Falls overlay (`waterfalls.pcx`, on disk).
    Falls,
}

/// City-view background river layer. Verified against `0x407D56` (selects
/// `RiverFore.pcx` vs `RiverFore-FP.pcx` on mode 9/10) and the on-disk
/// `D-H2O-*.pcx` inventory.
pub fn city_river_layer(floodplain_mode: bool) -> &'static str {
    if floodplain_mode {
        "RiverFore-FP.pcx"
    } else {
        "RiverFore.pcx"
    }
}

/// `art\city view\Backgrounds\<mode>-H2O-<layer>.pcx` name builder.
/// Observed pattern: `D-H2O-RiverFore.pcx`, `D-H2O-NOTtheRiver.pcx`, ...
pub fn city_background_name(mode_prefix: char, layer: &str) -> String {
    format!("{mode_prefix}-H2O-{layer}")
}

/// Overlay gate table at `0xA53BC8` (row stride 8420). The renderer draws
/// the overlay layer `row` for `mask` only when `gate(row, mask) != 0,
/// after requiring tile flags `0x78` and `mask > 0` (`0x57F8BF..0x57FEEB`).
/// Rows dumped live: 0 → `1..=31`, 1 → `{0, 8}`, 2 → `{0, 20, 21, 22}`.
pub fn overlay_gate(row: usize, mask: u8) -> bool {
    match row {
        0 => (1..32).contains(&mask),
        1 => mask == 0 || mask == 8,
        2 => mask == 0 || (20..=22).contains(&mask),
        _ => false,
    }
}

/// Per-tile overlay gate: flags bits `0x78` all set, mask positive, and
/// the gate table agrees. Mirrors `0x57F8BF..0x57FEEB`.
pub fn overlay_present(tile_flags: u8, row: usize, mask: RiverMask) -> bool {
    tile_flags & 0x78 == 0x78 && mask.0 > 0 && overlay_gate(row, mask.0)
}

/// Nibble selectors over the cell attribute dword `[cell+0x2C]`
/// (slot 0 = `0x5EA4E0`). Which layer each feeds is **HYPOTHESIS**:
/// bits 8–11 come out of the `0x4C31A0` wrapper path (slot 49,
/// `0x5EAB20`), bits 12–15 out of the render overlay path (slot 35
/// chain via `0x5EAB30`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CellAttr(pub u32);

impl CellAttr {
    /// Bits 8–11 (slot 49 / `0xC4` consumer).
    pub fn layer_a(self) -> u8 {
        ((self.0 >> 8) & 0xF) as u8
    }

    /// Bits 12–15 (slot 35 consumer in the `0x57Fxxx` renderer).
    pub fn layer_b(self) -> u8 {
        ((self.0 >> 12) & 0xF) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn through_vs_end() {
        assert!(RiverMask(RiverMask::NORTH | RiverMask::SOUTH).is_through());
        assert!(!RiverMask(RiverMask::NORTH).is_through());
        assert!(RiverMask::empty().is_empty());
    }

    #[test]
    fn overlay_gate_rows() {
        // Row contents dumped live from 0xA53BC8 (+8420 per row).
        assert!(overlay_gate(0, 1) && overlay_gate(0, 31));
        assert!(!overlay_gate(0, 0) && !overlay_gate(0, 32));
        assert!(overlay_gate(1, 0) && overlay_gate(1, 8));
        assert!(!overlay_gate(1, 6));
        assert!(overlay_gate(2, 20) && overlay_gate(2, 22));
        assert!(!overlay_gate(2, 19) && !overlay_gate(2, 23));
        assert!(!overlay_gate(3, 8));
    }

    #[test]
    fn overlay_present_gate() {
        // Live sample: flags 0x09, mask 6, row 1 -> table skipped.
        assert!(!overlay_present(
            0x09,
            1,
            RiverMask(6),
        ));
        // All 0x78 bits + mask 8 + row-1 table hit -> overlay.
        assert!(overlay_present(0x79, 1, RiverMask(8)));
        // Same but mask 6 -> table miss.
        assert!(!overlay_present(0x79, 1, RiverMask(6)));
        // Zero mask never passes (jle SKIP).
        assert!(!overlay_present(0x78, 1, RiverMask(0)));
    }

    #[test]
    fn cell_attr_nibbles() {
        // Live sample: cell 0x0B6BFCD0 [cell+0x2C] = 0x1100.
        let a = CellAttr(0x1100);
        assert_eq!(a.layer_a(), 1);
        assert_eq!(a.layer_b(), 1);
        assert_eq!(CellAttr(0).layer_a(), 0);
        assert_eq!(CellAttr(0xFF000000).layer_b(), 0);
    }

    #[test]
    fn city_layer_selection() {
        // Mode 9/10 branch at 0x407D56 picks the -FP variant.
        assert_eq!(city_river_layer(true), "RiverFore-FP.pcx");
        assert_eq!(city_river_layer(false), "RiverFore.pcx");
        assert_eq!(
            city_background_name('D', city_river_layer(false)),
            "D-H2O-RiverFore.pcx"
        );
    }
}
