//! Who owns a tile that cities of two civilizations both reach?
use civ3_biq::Save;

fn main() {
    let (mut oldest_ok, mut cult_ok, mut nearest_ok, mut nearest_bad, mut ties, mut tie_cult, mut same_civ, mut unowned, mut total) = (0, 0, 0, 0, 0, 0, 0, 0, 0);
    let mut bad_examples = 0;
    for path in std::env::args().skip(1) {
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let Ok(s) = Save::parse(&bytes) else { continue };
        let (w, h) = (s.map.width() as i32, s.map.height() as i32);
        for y in 0..h {
            for x in (y & 1..w).step_by(2) {
                let Some(t) = s.map.tile(x as u32, y as u32) else { continue };
                // (d2*2 in real units, culture, owner, id)
                let mut cands: Vec<(i32, u32, u8, u32)> = vec![];
                for c in &s.cities {
                    let level = c.block_20.u32(0x5C - 0x20) as i32;
                    let dx = (c.x() as i32 - x).abs();
                    let dx = dx.min(w - dx);
                    let dy = (c.y() as i32 - y).abs();
                    let d = dx * dx + dy * dy; // = 2 * d2
                    if d <= 2 * (level * level + 1) {
                        cands.push((d, c.block_13c.u32(4 + 4 * c.owner() as usize), c.owner(), c.id()));
                    }
                }
                if cands.is_empty() {
                    continue;
                }
                let civs: std::collections::BTreeSet<u8> = cands.iter().map(|c| c.2).collect();
                if civs.len() < 2 {
                    if t.owner() != cands[0].2 { unowned += 1; }
                    same_civ += 1;
                    continue;
                }
                total += 1;
                let old = cands.iter().min_by_key(|c| c.3).unwrap();
                if t.owner() == old.2 { oldest_ok += 1; }
                let hi = cands.iter().max_by_key(|c| c.1).unwrap();
                if t.owner() == hi.2 { cult_ok += 1; }
                cands.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
                let best = cands[0];
                if cands.len() > 1 && cands[1].0 == best.0 && cands[1].2 != best.2 {
                    ties += 1;
                    if t.owner() == best.2 { tie_cult += 1; }
                    continue;
                }
                if t.owner() == best.2 { nearest_ok += 1 } else {
                    nearest_bad += 1;
                    if bad_examples < 6 {
                        bad_examples += 1;
                        println!("{path}: tile ({x},{y}) owner {} cands {:?}", t.owner(), cands);
                    }
                }
            }
        }
    }
    println!("oldest ok {oldest_ok}, most-culture ok {cult_ok}");
    println!("same-civ tiles {same_civ} (not owned by that civ: {unowned}); contested {total}: nearest ok {nearest_ok} bad {nearest_bad}; ties {ties}, tie-by-culture ok {tie_cult}");
}
