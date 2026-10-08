//! Map, unit, and city graphics: sprite selection from observed assets.
//!
//! Covers `../graphics-terrain.md`, `../graphics-units.md`, and
//! `../graphics-city.md`. Filename tables are observed inventory from
//! `civ3/civ3-gog/app/Art/`; engine call sites (`0x4C5F9F` art table,
//! `0x598580` loader, `0x407C30` city view) are cited where verified.

/// Base land/water tile filename: `Art\Terrain\[l]<x|w><base>.pcx`.
/// Observed: `xtgc`, `wCSO`, and the alternate set keeps the base prefix
/// (`lxtgc`, `lwCSO` — the `l` prepends, verified by `lwCSO/lwOOO/lwSSS`).
pub fn terrain_sprite(base: &str, water: bool, alternate: bool) -> String {
    let prefix = if water { 'w' } else { 'x' };
    format!(
        "Art\\Terrain\\{}{prefix}{base}.pcx",
        if alternate { "l" } else { "" }
    )
}

/// Overlay stacking order, lowest drawn first. Inferred from file roles.
pub const OVERLAY_ORDER: &[&str] = &[
    "base",
    "forest",
    "irrigation",
    "road",
    "railroad",
    "pollution",
    "goodyhut",
    "FogOfWar",
];

/// Unit animation slots: exact key universe over all 76 shipped
/// `Art/Units/*/*.ini` files (case-insensitive scan, 30 keys).
/// Empty value = unit lacks the action.
pub const UNIT_ANIM_SLOTS: &[&str] = &[
    "BLANK",
    "DEFAULT",
    "WALK",
    "RUN",
    "ATTACK1",
    "ATTACK2",
    "ATTACK3",
    "DEFEND",
    "DEATH",
    "DEAD",
    "FORTIFY",
    "FORTIFYHOLD",
    "FIDGET",
    "VICTORY",
    "TURNLEFT",
    "TURNRIGHT",
    "BUILD",
    "ROAD",
    "MINE",
    "IRRIGATE",
    "FORTRESS",
    "CAPTURE",
    "STOP_AT_LAST_FRAME",
    "JUNGLE",
    "FOREST",
    "PLANT",
    "PauseROAD",
    "PauseMINE",
    "PauseIRRIGATE",
    "PauseFOREST",
];

/// Slots present as keys but never filled in any shipped ini:
/// engine-supported actions the data never uses.
pub const NEVER_FILLED_SLOTS: &[&str] = &[
    "ATTACK3",
    "DEFEND",
    "WALK",
    "FORTIFYHOLD",
    "BLANK",
    "STOP_AT_LAST_FRAME",
    "TURNLEFT",
    "TURNRIGHT",
    "PauseROAD",
    "PauseMINE",
    "PauseIRRIGATE",
    "PauseFOREST",
];

/// City-view background prefix: the city's base-terrain class.
/// Observed on disk as `Art\City View\Backgrounds\<P>-...` (48 files).
pub const CITY_BG_PREFIXES: &[&str] = &["D", "G", "P", "T"];

/// City-view background filename: `<P>-<feature>.pcx` for water/feature
/// layers, `<P>-<SML|MED|LRG|UNCLEAR>.pcx` for plain size backgrounds.
/// Only the `D-` row ships a `-FP` floodplain variant.
pub fn city_background(prefix: &str, feature: &str) -> String {
    format!("Art\\City View\\Backgrounds\\{prefix}-{feature}.pcx")
}

/// BLDG resolver table shape (`0x407070`): ids `0..=78` dispatch through
/// the jump table; holes (29, 64–73) and anything above 78 fall through to
/// the `BLDG_Empty` default. Idx 78 is a valid duplicate of the idx-0
/// Palace case. Returns false for ids that resolve to `BLDG_Empty`.
pub fn bldg_key_valid(id: u32) -> bool {
    id <= 78 && id != 29 && !(64..=73).contains(&id)
}

/// Minimal parse of a unit `.ini` `[Animations]` section: slot -> file.
/// Returns the filled (non-empty) slots.
pub fn parse_anim_slots(ini: &str) -> Vec<(String, String)> {
    let mut in_anims = false;
    let mut out = Vec::new();
    for line in ini.lines() {
        let line = line.trim().trim_matches('\r');
        if line.starts_with('[') {
            in_anims = line.eq_ignore_ascii_case("[Animations]");
            continue;
        }
        if !in_anims || line.is_empty() || !line.contains('=') {
            continue;
        }
        let (k, v) = line.split_once('=').unwrap();
        let (k, v) = (k.trim().to_string(), v.trim().to_string());
        if !v.is_empty() {
            out.push((k, v));
        }
    }
    out
}

/// Map diamond size in pixels: 128x64 (measured from every grid sheet).
pub const TILE_PX_W: u32 = 128;
/// Map diamond height in pixels.
pub const TILE_PX_H: u32 = 64;
/// Base land/water sheets are 9x9 transition matrices (1152x576 px).
pub const BASE_GRID: u32 = 9;
/// River sheets are 4x4 edge-mask tables (512x256 px): 16 = 2^4.
/// Matches the `RiverMask` nibble in [`crate::rivers`]. Bit order unverified.
pub const RIVER_GRID: u32 = 4;
/// Irrigation overlays are 4x4 edge-masks; magenta centers are empty cells.
pub const IRRIGATION_GRID: u32 = 4;
/// `roads.pcx` is a 16x16 table (2048x1024 px): 256 = 2^8 neighbour-mask.
/// **HYPOTHESIS**: index-to-mask-bit mapping unverified.
pub const ROAD_GRID: u32 = 16;

/// Tile `(col, row)` cell count of a square grid sheet.
pub const fn grid_tiles(grid: u32) -> u32 {
    grid * grid
}

/// City size class for `D-<LRG|MED|SML>.pcx` selection.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CitySize {
    /// `D-SML.pcx` background class.
    Small,
    /// `D-MED.pcx` background class.
    Medium,
    /// `D-LRG.pcx` background class.
    Large,
}

impl CitySize {
    /// Filename fragment for the size class (`SML`/`MED`/`LRG`).
    pub fn file_fragment(self) -> &'static str {
        match self {
            CitySize::Small => "SML",
            CitySize::Medium => "MED",
            CitySize::Large => "LRG",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terrain_names_match_inventory() {
        assert_eq!(terrain_sprite("tgc", false, false), "Art\\Terrain\\xtgc.pcx");
        assert_eq!(terrain_sprite("CSO", true, false), "Art\\Terrain\\wCSO.pcx");
        assert_eq!(terrain_sprite("tgc", false, true), "Art\\Terrain\\lxtgc.pcx");
    }

    #[test]
    fn settler_slots() {
        // Settler fills 6 animation slots; ATTACK*/DEFEND/FORTIFY are empty.
        let ini = "[Animations]\nDEFAULT=settDefault.flc\nRUN=settRun.flc\n\
                   ATTACK1=\nDEATH=settDeath.flc\nFIDGET=settFidget.flc\n\
                   BUILD=settBuild.flc\nCAPTURE=SettlerCaptured.flc\n";
        let slots = parse_anim_slots(ini);
        assert_eq!(slots.len(), 6);
        assert!(slots.iter().all(|(k, _)| UNIT_ANIM_SLOTS.contains(&k.as_str())));
        assert!(!slots.iter().any(|(k, _)| k == "ATTACK1"));
    }

    #[test]
    fn sheet_geometry_matches_disk() {
        // Measured PCX geometry: 1152x576 bases, 512x256 overlays/rivers.
        assert_eq!(TILE_PX_W / TILE_PX_H, 2);
        assert_eq!(grid_tiles(BASE_GRID), 81);
        assert_eq!(grid_tiles(RIVER_GRID), 16);
        assert_eq!(grid_tiles(IRRIGATION_GRID), 16);
        assert_eq!(grid_tiles(ROAD_GRID), 256);
        assert_eq!(1152 / TILE_PX_W, BASE_GRID);
        assert_eq!(512 / TILE_PX_W, RIVER_GRID);
    }

    #[test]
    fn city_size_fragments() {
        assert_eq!(CitySize::Small.file_fragment(), "SML");
        assert_eq!(CitySize::Large.file_fragment(), "LRG");
    }

    #[test]
    fn slot_universe_is_thirty_keys() {
        assert_eq!(UNIT_ANIM_SLOTS.len(), 30);
        assert!(UNIT_ANIM_SLOTS.contains(&"PLANT"));
        assert!(UNIT_ANIM_SLOTS.contains(&"PauseFOREST"));
        // Every never-filled slot is still a known key.
        assert!(NEVER_FILLED_SLOTS
            .iter()
            .all(|s| UNIT_ANIM_SLOTS.contains(s)));
    }

    #[test]
    fn full_settler_ini_parses_to_six_filled() {
        // Mirrors Settler/settler.ini [Animations]: 30 keys, 6 filled.
        let ini = "[Animations]\nBLANK=\nDEFAULT=settDefault.flc\nWALK=\n\
                   RUN=settRun.flc\nATTACK1=\nATTACK2=\nATTACK3=\nDEFEND=\n\
                   DEATH=settDeath.flc\nDEAD=\nFORTIFY=\nFORTIFYHOLD=\n\
                   FIDGET=settFidget.flc\nVICTORY=\nTURNLEFT=\nTURNRIGHT=\n\
                   BUILD=settBuild.flc\nROAD=\nMINE=\nIRRIGATE=\nFORTRESS=\n\
                   CAPTURE=SettlerCaptured.flc\nSTOP_AT_LAST_FRAME=\n\
                   PauseROAD=\nPauseMINE=\nPauseIRRIGATE=\nJUNGLE=\nFOREST=\n\
                   PLANT=\nPauseFOREST=\n";
        let slots = parse_anim_slots(ini);
        assert_eq!(slots.len(), 6);
        assert!(slots.iter().all(|(k, _)| UNIT_ANIM_SLOTS.contains(&k.as_str())));
        assert!(slots
            .iter()
            .all(|(k, _)| !NEVER_FILLED_SLOTS.contains(&k.as_str())));
    }

    #[test]
    fn bldg_table_shape() {
        assert!(bldg_key_valid(0)); // Palace
        assert!(!bldg_key_valid(29)); // hole
        assert!(!bldg_key_valid(64));
        assert!(!bldg_key_valid(73));
        assert!(bldg_key_valid(74));
        assert!(bldg_key_valid(78)); // Palace duplicate
        assert!(!bldg_key_valid(79)); // above 0x4E -> default
    }

    #[test]
    fn city_background_names_match_disk() {
        assert_eq!(
            city_background("D", "H2O-RiverFore-FP"),
            "Art\\City View\\Backgrounds\\D-H2O-RiverFore-FP.pcx"
        );
        assert_eq!(
            city_background("G", "SML"),
            "Art\\City View\\Backgrounds\\G-SML.pcx"
        );
        assert_eq!(CITY_BG_PREFIXES.len(), 4);
    }
}
