//! Resource placement, stage 10 (`0x5f22a0`).
//!
//! Implements the `.biq`-data-independent math of `placeResources`: the
//! default frequency roll, the quantity formula, the terrain-score weighting,
//! and the block-3 acceptance probabilities. See `../resources.md`.
//!
//! Fully specified no-data stages `placeGoodyHuts` (`0x5f21b0`) and
//! `placeBarbarianCamps` (`0x5f2090`) live here too.
//!
//! Every constant cites the address it came from.

use crate::rng::Rng;

/// A `GOOD` resource row: frequency from `good->[0x40]`. `0x5f22a0`.
#[derive(Clone, Copy, Debug)]
pub struct Good {
    /// Per-resource frequency. `0` means "roll it". (`0x5f22a0`)
    pub freq: i32,
}

impl Good {
    /// `pct = freq ? freq : rand_int(26) + rand_int(26) + 50`. (`0x5f22a0`)
    pub fn frequency(&self, rng: &mut Rng) -> i32 {
        if self.freq != 0 {
            self.freq
        } else {
            rng.below(26) + rng.below(26) + 50
        }
    }
}

/// Quantity of copies for one resource. (`0x5f22a0`)
///
/// ```text
/// n1 = (area_factor * pct) / 32
/// n  = score<2 ? n1*0.5 : score<4 ? n1*0.75 : n1
/// n  = max(n, score>=4 ? 2 : 1)
/// ```
/// `score` counts `TERR` rows allowing the resource, +4 each for `t >= 11`.
pub fn quantity(area_factor: i32, pct: i32, score: i32) -> i32 {
    let n1 = (area_factor * pct) / 32;
    let n = if score < 2 {
        (n1 as f32 * 0.5) as i32
    } else if score < 4 {
        (n1 as f32 * 0.75) as i32
    } else {
        n1
    };
    n.max(if score >= 4 { 2 } else { 1 })
}

/// Block-3 die sides by score band. (`0x5f22a0`, branch at `0x5F2B3E`)
///
/// The roll **skips** iff `rand_int(sides) > 1` (`cmp ax,1; ja 0x5F2C06`,
/// where `0x5F2C06` advances the loop): acceptance is `roll <= 1`, i.e.
/// 2/6 = 33 %, 2/4 = 50 %, 2/2 = 100 %.
pub fn block3_sides(score: i32) -> u32 {
    if score < 2 {
        6
    } else if score < 4 {
        4
    } else {
        2
    }
}

/// Block-3 skip predicate: `true` means no placement this round. (`0x5F2B62`)
pub fn block3_skip(roll: u32) -> bool {
    roll > 1
}

/// All copies of one resource must share one `vfunc(0xB8)` region id.
pub fn same_region(regions: &[u16]) -> bool {
    regions.iter().all(|&r| r == regions[0])
}

/// `placeGoodyHuts` (`0x5f21b0`): count of hut rolls for a goody count.
///
/// Returns `None` when the stage is a no-op: `count == -1`, or the
/// `>= 32` guard fails (no-op in every normal game, max 31 civs).
pub fn goody_hut_rolls(goody_count: i32) -> Option<i32> {
    if goody_count == -1 {
        return None;
    }
    if (goody_count & 0xFFE0) == 0 {
        return None;
    }
    Some(goody_count >> 5)
}

/// `placeBarbarianCamps` (`0x5f2090`): 1-in-3 per eligible land tile.
pub fn barbarian_camp_lands(rng: &mut Rng) -> bool {
    rng.one_in(3)
}

/// A decoded `GOOD` row: the 92B memory layout after `0x5e3860` reads one
/// file row. Offsets match the live dump (`resources.md`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoodRow {
    /// Display name, NUL-padded. (`+0x04`, 24B fread, `0x5e3886`)
    pub name: [u8; 24],
    /// `GOOD_*` key, NUL-padded. (`+0x1c`, 32B fread, `0x5e38a2`)
    pub key: [u8; 32],
    /// 2 = strategic, 1 = luxury, 0 = bonus. (`+0x3c`)
    pub cls: u32,
    /// Per-resource frequency. (`+0x40`)
    pub freq: u32,
    /// Unmapped second rate: 800/400/200/100 on strategics, 0 elsewhere.
    /// (`+0x44`) The Civ3 editor's Good tab carries both an appearance ratio
    /// and a disappearance probability per resource; `freq` is the first
    /// (the placement math consumes it), this is likely the second.
    pub a: u32,
    /// Icon/ordering id: **the `resources.pcx` cell** for this resource
    /// (`+0x48`). Equals the row index for every row except Sugar (24),
    /// Tropical Fruit (22) and Oasis (23) — verified against the sheet's
    /// 26 icons (`../resources.md`).
    pub b: u32,
    /// Tech that reveals this resource: an index into the `TECH` section
    /// (`u32::MAX` = never revealed, i.e. luxuries and bonus resources).
    /// (`+0x4c`)
    ///
    /// Verified 8/8 on `conquests.biq` and 8/8 on `civ3mod.bic` (`Iron` ->
    /// `Iron Working`, `Saltpeter` -> `Gunpowder`, `Coal` -> `Steam Power`,
    /// `Oil` -> `Refining`, `Rubber` -> `Replaceable Parts`, `Aluminum` ->
    /// `Rocketry`, `Uranium` -> `Fission`, `Horses` -> `The Wheel`, whose
    /// civilopedia entry reads "{New Resource} Horses appear on the map").
    /// Indices are file order, so they follow a scenario's own `TECH` list.
    pub c: u32,
    /// Trailing 12B (`+0x50`): three small u32s.
    pub tail: [u8; 12],
}

impl GoodRow {
    /// Display name up to the first NUL.
    pub fn name_str(&self) -> &str {
        cstr(&self.name)
    }

    /// `GOOD_*` key up to the first NUL.
    pub fn key_str(&self) -> &str {
        cstr(&self.key)
    }
}

/// NUL-terminated view of a fixed-size text field.
fn cstr(bytes: &[u8]) -> &str {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    core::str::from_utf8(&bytes[..end]).unwrap_or("")
}

/// Parse a raw-BIC (PTW-format) `GOOD` section body: the bytes after the
/// tag. Layout: `[u32 count][rows…]`, each row `[u32 len][data…]` with
/// `len >= 88`; the row reader (`0x5e3860`) consumes the fixed 88B core
/// and `fseek`s past the rest (`0x5e396a`). Returns `None` on truncation.
///
/// Ground truth: `Ancient Mediterranean.bix` parses 29/29, and
/// `conquests.biq` parses 26/26 with the same framing (`Horses` …
/// `Tobacco`, matching the live memory table). The earlier belief that
/// Conquests GOOD sections are u16-framed and nameless came from a
/// corrupted DCL decode — see `biq.md` and `resources.md`.
pub fn parse_ptw_good_section(body: &[u8]) -> Option<(u32, Vec<GoodRow>)> {
    if body.len() < 4 {
        return None;
    }
    let count = u32::from_le_bytes(body[0..4].try_into().ok()?);
    let mut rows = Vec::with_capacity(count.min(256) as usize);
    let mut off = 4;
    for _ in 0..count {
        if off + 4 > body.len() {
            return None;
        }
        let len = u32::from_le_bytes(body[off..off + 4].try_into().ok()?) as usize;
        if len < 88 || off + 4 + len > body.len() {
            return None;
        }
        let d = &body[off + 4..off + 4 + 88];
        let u = |i: usize| u32::from_le_bytes(d[i..i + 4].try_into().unwrap());
        rows.push(GoodRow {
            name: d[0..24].try_into().unwrap(),
            key: d[24..56].try_into().unwrap(),
            cls: u(56),
            freq: u(60),
            a: u(64),
            b: u(68),
            c: u(72),
            tail: d[76..88].try_into().unwrap(),
        });
        off += 4 + len;
    }
    Some((count, rows))
}

/// A decoded `TERR` row: the terrain's allow-matrix for resources.
///
/// File layout, in the order the row reader (`0x5e9300`) consumes it:
/// `[u32 len][u32 goods_count][ceil(goods_count/8) allow bytes][32 B name]
/// [32 B key][161 more bytes]` — `len = 233`, `goods_count = 26` for every
/// shipped `conquests.biq` row. In memory the row keeps the remaining length
/// at `+0x04`, a *pointer* to the mask array at `+0x08` (`0x5e9362`) and the
/// count at `+0x60`; placement reads the mask through that pointer
/// (`0x5F2470`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerrRow {
    /// `u32` at the start of the row body: how many `GOOD` rows the mask
    /// covers (= the GOOD count, 26 here).
    pub goods: u32,
    /// Allow bits, little-endian: bit `g` set = resource `g` may appear on
    /// this terrain. Exactly the bytes placement reads through `[row+8]`
    /// (`byte[ptr + (g>>3)] & (1 << (g&7))`, `0x5F2470`).
    pub allows_bits: u32,
    /// Display name (`+0x08`, 32 B, NUL-padded — the GOOD row's is 24 B).
    pub name: [u8; 32],
    /// `TERR_*` key (32 B, NUL-padded).
    pub key: [u8; 32],
    /// Movement cost for this terrain: `u32` at row-body `+0x58` (memory
    /// `+0x5C`) — 1 for open/water terrains, 2 for hills/forest/marsh,
    /// 3 for mountains/jungle/volcano in `conquests.biq`. An 8.8 fixed-point
    /// copy sits at body `+0x94` (`cost << 8`), which is why a byte read at
    /// `+0x95` shows the same number.
    pub cost: u32,
    /// Defense bonus in percent: `u32` at row-body `+0x54` (memory `+0x58`)
    /// — 10 base, 20 marsh, 25 forest/jungle, 50 hills, 80 volcano,
    /// 100 mountains in `conquests.biq`, with the 8.8 copy at body `+0x98`.
    pub defense: u32,
}

impl TerrRow {
    /// Does this terrain permit resource `g`? (`0x5F2470`)
    pub fn allows(&self, good: u32) -> bool {
        self.allows_bits >> good & 1 == 1
    }

    /// Display name up to the first NUL.
    pub fn name_str(&self) -> &str {
        cstr(&self.name)
    }

    /// `TERR_*` key up to the first NUL.
    pub fn key_str(&self) -> &str {
        cstr(&self.key)
    }
}

/// Parse a `TERR` section body (the bytes after the tag): `[u32 count]
/// [rows…]`, each row `[u32 len][body]`.
///
/// Ground truth: `conquests.biq` parses 14/14 (`Desert` … `Ocean`), the last
/// row ending exactly on the next section tag.
pub fn parse_terr_section(body: &[u8]) -> Option<(u32, Vec<TerrRow>)> {
    if body.len() < 4 {
        return None;
    }
    let count = u32::from_le_bytes(body[0..4].try_into().ok()?);
    let mut rows = Vec::with_capacity(count.min(64) as usize);
    let mut off = 4;
    for _ in 0..count {
        let len = u32::from_le_bytes(body.get(off..off + 4)?.try_into().ok()?) as usize;
        let d = body.get(off + 4..off + 4 + len)?;
        let goods = u32::from_le_bytes(d.get(0..4)?.try_into().ok()?);
        let mask_bytes = goods.div_ceil(8) as usize;
        if mask_bytes > 4 {
            return None;
        }
        let mut allows_bits = 0u32;
        for (i, b) in d.get(4..4 + mask_bytes)?.iter().enumerate() {
            allows_bits |= (*b as u32) << (8 * i);
        }
        let name_at = 4 + mask_bytes;
        rows.push(TerrRow {
            goods,
            allows_bits,
            name: d.get(name_at..name_at + 32)?.try_into().ok()?,
            key: d.get(name_at + 32..name_at + 64)?.try_into().ok()?,
            cost: u32::from_le_bytes(d.get(0x58..0x5C)?.try_into().ok()?),
            defense: u32::from_le_bytes(d.get(0x54..0x58)?.try_into().ok()?),
        });
        off += 4 + len;
    }
    Some((count, rows))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_frequency_range() {
        let mut rng = Rng::new(0x180E3); // 0x5f22a0 resource seed
        for _ in 0..50 {
            let p = Good { freq: 0 }.frequency(&mut rng);
            assert!((50..100).contains(&p));
        }
        assert_eq!(Good { freq: 70 }.frequency(&mut rng), 70);
    }

    #[test]
    fn quantity_weighting() {
        // score 0: half, min 1. score 2..3: three quarters. score 4+: full, min 2.
        assert_eq!(quantity(320, 80, 0), 400);
        assert_eq!(quantity(320, 80, 2), 600);
        assert_eq!(quantity(320, 80, 5), 800);
        assert_eq!(quantity(0, 80, 0), 1);
        assert_eq!(quantity(0, 80, 5), 2);
    }

    #[test]
    fn block3_probabilities() {
        assert_eq!(block3_sides(0), 6);
        assert_eq!(block3_sides(3), 4);
        assert_eq!(block3_sides(4), 2);
        // Acceptance is roll <= 1 over rand_int(sides) in 0..sides.
        let rate = |sides: u32| {
            (0..sides).filter(|&r| !block3_skip(r)).count()
        };
        assert_eq!(rate(6), 2); // 33 %
        assert_eq!(rate(4), 2); // 50 %
        assert_eq!(rate(2), 2); // 100 %
    }

    #[test]
    fn goody_guard_is_noop_for_normal_games() {
        assert_eq!(goody_hut_rolls(-1), None);
        assert_eq!(goody_hut_rolls(31), None); // max real civ count
        assert_eq!(goody_hut_rolls(32), Some(1));
    }

    #[test]
    fn same_region_invariant() {
        assert!(same_region(&[7, 7, 7]));
        assert!(!same_region(&[7, 8, 7]));
    }

    /// The decode of `conquests.biq` that the live EGYPT load used: the
    /// file's GOOD rows are the memory rows.
    fn conquests_biq() -> Vec<u8> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let p = root.join("../../../civ3-gog/app/Conquests/conquests.biq");
        let raw = std::fs::read(&p).unwrap_or_else(|_| panic!("missing {}", p.display()));
        crate::dcl::decompress(&raw).expect("biq decodes")
    }

    #[test]
    fn good_section_layout() {
        // Section framing: tag + u32 count, then `count` rows of
        // `[u32 len][len]` (the length word is what the reader consumes,
        // `0x5e3860`). Memory-row stride 0x5C comes from the reader loop
        // (0x5945ac) and writer loop (0x5974b4), not from the file.
        let out = conquests_biq();
        let secs = crate::dcl::sections(&out);
        let good = secs.iter().find(|s| &s.tag == b"GOOD").expect("GOOD section");
        assert_eq!(good.count, 26);
        let (count, rows) = parse_ptw_good_section(&out[good.rows_at - 4..good.end]).expect("parses");
        assert_eq!(count, 26);
        assert_eq!(rows.len(), 26);
        assert_eq!(rows[0].name_str(), "Horses");
        assert_eq!(rows[0].key_str(), "GOOD_Horses");
        assert_eq!(rows[0].cls, 2);
        assert_eq!(rows[0].freq, 160);
        assert_eq!(rows[7].name_str(), "Uranium");
        assert_eq!(rows[8].name_str(), "Wines");
        assert_eq!(rows[8].key_str(), "GOOD_Wine");
        assert_eq!(rows[8].cls, 1);
        assert_eq!(rows[25].name_str(), "Tobacco");
        assert_eq!(rows[25].freq, 0);
        // `b` is the icon/ordering id (the `resources.pcx` cell): the row
        // index everywhere except the three re-ordered tail rows.
        for (i, r) in rows.iter().enumerate() {
            let expect = match i {
                22 => 24, // Sugar
                23 => 22, // Tropical Fruit
                24 => 23, // Oasis
                n => n as u32,
            };
            assert_eq!(r.b, expect, "row {i} ({}): b", r.name_str());
        }
        // File row 0 == the live memory row captured under winedbg.
        assert_eq!(good.row(&out, 0).unwrap()[..88], LIVE_ROW0[4..]);
    }

    /// GOOD `+0x4c` is the `TECH` row that reveals the resource.
    ///
    /// The index names the canonical reveal tech for all 8 strategics in
    /// `conquests.biq`, and the game's own civilopedia text for The Wheel
    /// ("\{New Resource\} Horses appear on the map", `Text/Civilopedia.txt`)
    /// settles the one pairing that looks surprising.
    #[test]
    fn strategic_reveal_tech_indices() {
        let out = conquests_biq();
        let secs = crate::dcl::sections(&out);
        let sec = |t: &[u8; 4]| *secs.iter().find(|s| &s.tag == t).expect("section");
        let tech = sec(b"TECH");
        assert_eq!(tech.count, 83);
        let techs: Vec<String> = (0..tech.count)
            .map(|i| cstr(&tech.row(&out, i).unwrap()[..32]).to_string())
            .collect();
        assert_eq!(techs[4], "The Wheel");
        assert_eq!(techs[30], "Gunpowder");
        assert_eq!(techs[65], "Fission");

        let good = sec(b"GOOD");
        let (_, goods) = parse_ptw_good_section(&out[good.rows_at - 4..good.end]).expect("GOOD");
        let reveal = |name: &str| -> &str {
            let g = goods.iter().find(|g| g.name_str() == name).expect("good");
            &techs[g.c as usize]
        };
        assert_eq!(reveal("Horses"), "The Wheel");
        assert_eq!(reveal("Iron"), "Iron Working");
        assert_eq!(reveal("Saltpeter"), "Gunpowder");
        assert_eq!(reveal("Coal"), "Steam Power");
        assert_eq!(reveal("Oil"), "Refining");
        assert_eq!(reveal("Rubber"), "Replaceable Parts");
        assert_eq!(reveal("Aluminum"), "Rocketry");
        assert_eq!(reveal("Uranium"), "Fission");
        // Luxuries and bonus resources carry `-1`.
        assert!(goods.iter().filter(|g| g.cls != 2).all(|g| g.c == u32::MAX));
    }

    #[test]
    fn terr_section_gives_allow_matrix() {
        // TERR rows: `[u32 len][u32 goods_count][4 allow bytes][24B name]
        // [32B key]`, 14 rows, ending exactly on the next tag (0x596490
        // loads them through 0x5e9300).
        let out = conquests_biq();
        let secs = crate::dcl::sections(&out);
        let terr = secs.iter().find(|s| &s.tag == b"TERR").expect("TERR section");
        assert_eq!(terr.count, 14);
        let (_, rows) = parse_terr_section(&out[terr.rows_at - 4..terr.end]).expect("parses");
        let names: Vec<&str> = rows.iter().map(|r| r.name_str()).collect();
        assert_eq!(names[0], "Desert");
        assert_eq!(names[4], "Flood Plain");
        assert_eq!(names[13], "Ocean");
        assert!(rows.iter().all(|r| r.goods == 26));
        assert!(rows.iter().all(|r| r.key_str().starts_with("TERR_")));
        // Keys normalize spaces to underscores (`Flood Plain` -> `TERR_Flood_Plain`).
        assert_eq!(rows[4].key_str(), "TERR_Flood_Plain");
        assert_eq!(rows[13].key_str(), "TERR_Ocean");

        let goods = parse_ptw_good_section(&out[secs.iter().find(|s| &s.tag == b"GOOD").unwrap().rows_at - 4..])
            .expect("GOOD")
            .1;
        let allowed = |terrain: &str| -> Vec<String> {
            let r = rows.iter().find(|r| r.name_str() == terrain).expect("terrain");
            goods
                .iter()
                .enumerate()
                .filter(|(i, _)| r.allows(*i as u32))
                .map(|(_, g)| g.name_str().to_string())
                .collect()
        };
        assert_eq!(allowed("Desert"), ["Saltpeter", "Oil", "Incense", "Oasis"]);
        assert_eq!(allowed("Flood Plain"), ["Wheat"]);
        assert_eq!(allowed("Coast"), ["Fish"]);
        assert_eq!(allowed("Sea"), ["Whales", "Fish"]);
        assert!(allowed("Ocean").is_empty());
        assert!(allowed("Volcano").is_empty());
        assert_eq!(rows[0].allows_bits, 0x0100_0814);
        // Movement cost (+0x95) and defense bonus (+0x99) per terrain — the
        // values Civ3's own tables give (open/water 1, hills/forest/jungle/
        // marsh 2, mountains/volcano 3; defense 10 base, 20 marsh, 25
        // forest/jungle, 50 hills, 80 volcano, 100 mountains).
        let costs: Vec<(u32, u32)> = rows.iter().map(|r| (r.cost, r.defense)).collect();
        assert_eq!(
            costs,
            vec![
                (1, 10),  // Desert
                (1, 10),  // Plains
                (1, 10),  // Grassland
                (1, 10),  // Tundra
                (1, 10),  // Flood Plain
                (2, 50),  // Hills
                (3, 100), // Mountains
                (2, 25),  // Forest
                (3, 25),  // Jungle
                (2, 20),  // Marsh
                (3, 80),  // Volcano
                (1, 10),  // Coast
                (1, 10),  // Sea
                (1, 10),  // Ocean
            ]
        );
    }

    #[test]
    fn ptw_bix_good_parses() {
        // Raw-BIC GOOD section: 29 len-88 rows with names inline.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let p = root.join("../../../civ3-gog/app/civ3PTW/Scenarios/Ancient Mediterranean.bix");
        let raw = std::fs::read(&p).unwrap_or_else(|_| panic!("missing {}", p.display()));
        let at = raw.windows(4).position(|w| w == b"GOOD").expect("GOOD section");
        let (count, rows) = parse_ptw_good_section(&raw[at + 4..]).expect("parses");
        assert_eq!(count, 29);
        assert_eq!(rows.len(), 29);
        assert_eq!(rows[0].name_str(), "Horses");
        assert_eq!(rows[0].key_str(), "GOOD_Horses");
        assert_eq!(rows[0].freq, 160);
        assert_eq!(rows[1].name_str(), "Iron");
        assert_eq!(rows[1].freq, 200);
        assert_eq!(rows[3].name_str(), "Tin");
        assert_eq!(rows[10].name_str(), "Purple");
        assert_eq!(rows[10].key_str(), "GOOD_Dye");
        assert_eq!(rows[28].name_str(), "Opium");
        assert!(parse_ptw_good_section(&raw[at + 4..at + 7]).is_none());
    }

    /// Live row 0 verbatim (`good_rows.bin`, EGYPT load): `[u32 idx]` +
    /// the 88B file core. The core must parse as a 1-row PTW section.
    const LIVE_ROW0: [u8; 92] = [
        0x00, 0x00, 0x00, 0x00, 0x48, 0x6f, 0x72, 0x73, 0x65, 0x73, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x47, 0x4f, 0x4f, 0x44, 0x5f, 0x48, 0x6f, 0x72,
        0x73, 0x65, 0x73, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x02, 0x00, 0x00, 0x00, 0xa0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
    ];

    #[test]
    fn egypt_sav_holds_one_stream() {
        // EGYPT.SAV decodes to exactly one DCL stream: the 1748113B map
        // data (save0.tmp). The 209222B rules stream the same load reads
        // (save1.tmp) is the game's own decode of `conquests.biq`, not
        // save content — see `biq.md`.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let p = root.join("../../../civ3-gog/app/Conquests/Saves/EGYPT.SAV");
        let raw = std::fs::read(&p).unwrap_or_else(|_| panic!("missing {}", p.display()));
        let (save0, n1) = crate::dcl::decompress_prefix(&raw).expect("stream 1");
        assert_eq!(save0.len(), 1748113);
        assert_eq!(&save0[..4], b"CIV3");
        assert_eq!(n1, raw.len(), "one stream consumes the whole file");
    }

    #[test]
    fn live_row_core_parses_as_ptw() {
        let mut section = vec![1u8, 0, 0, 0, 88, 0, 0, 0];
        section.extend_from_slice(&LIVE_ROW0[4..]);
        let (count, rows) = parse_ptw_good_section(&section).expect("parses");
        assert_eq!(count, 1);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name_str(), "Horses");
        assert_eq!(rows[0].key_str(), "GOOD_Horses");
        assert_eq!(rows[0].cls, 2);
        assert_eq!(rows[0].freq, 160);
        assert_eq!(rows[0].a, 0);
        assert_eq!(rows[0].b, 0);
        assert_eq!(rows[0].c, 4);
    }
}
