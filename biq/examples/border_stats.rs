//! Border shape statistics across saves: for each culture level, how often a
//! tile at an offset from a city belongs to that city's civilization, counting
//! only tiles where no other city is as close as this one (so the claim is
//! this city's own).
use civ3_biq::Save;
use std::collections::HashMap;

fn main() {
    // level -> (dx, dy) -> (owned, total)
    let mut stats: HashMap<u32, HashMap<(i32, i32), (u32, u32)>> = HashMap::new();
    let mut count: HashMap<u32, u32> = HashMap::new();
    for path in std::env::args().skip(1) {
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let Ok(s) = Save::parse(&bytes) else { continue };
        let (w, h) = (s.map.width() as i32, s.map.height() as i32);
        let dist = |ax: i32, ay: i32, bx: i32, by: i32| {
            let dx = (ax - bx).abs();
            let dx = dx.min(w - dx);
            let dy = (ay - by).abs();
            // tile-grid distance: the diamond coordinates halve
            let (dx, dy) = ((dx + dy) / 2, (dy - dx).abs() / 2 + 0);
            let _ = dy;
            dx
        };
        let _ = dist;
        for c in &s.cities {
            let (cx, cy) = (c.x() as i32, c.y() as i32);
            let owner = c.owner();
            let level = c.block_20.u32(0x5C - 0x20);
            *count.entry(level).or_default() += 1;
            for dy in -14..=14 {
                for dx in -14..=14 {
                    if (dx + dy) & 1 != 0 {
                        continue;
                    }
                    let (x, y) = (cx + dx, cy + dy);
                    if y < 0 || y >= h {
                        continue;
                    }
                    let x = (x % w + w) % w;
                    // skip tiles at least as near to some other city
                    let me = dx * dx + dy * dy;
                    let contested = s.cities.iter().any(|o| {
                        if o.id() == c.id() {
                            return false;
                        }
                        let ox = (o.x() as i32 - x).abs();
                        let ox = ox.min(w - ox);
                        let oy = (o.y() as i32 - y).abs();
                        ox * ox + oy * oy <= me + 40
                    });
                    if contested {
                        continue;
                    }
                    let Some(t) = s.map.tile(x as u32, y as u32) else { continue };
                    let e = stats.entry(level).or_default().entry((dx, dy)).or_default();
                    e.1 += 1;
                    if t.owner() == owner {
                        e.0 += 1;
                    }
                }
            }
        }
    }
    // The hypothesis d2 <= level^2 + 1 (d2 in tile coordinates = (dx^2+dy^2)/2).
    for (&l, m) in &stats {
        let t = (l * l + 1) as i32;
        let (mut in_o, mut in_t, mut out_o, mut out_t) = (0, 0, 0, 0);
        for (&(dx, dy), &(o, n)) in m {
            if dx * dx + dy * dy <= 2 * t {
                in_o += o;
                in_t += n;
            } else {
                out_o += o;
                out_t += n;
            }
        }
        println!("level {l}: inside {in_o}/{in_t} owned, outside {out_o}/{out_t} owned");
    }
    let mut levels: Vec<_> = stats.keys().copied().collect();
    levels.sort();
    for l in levels {
        println!("== level {l}: {} cities", count[&l]);
        for dy in -12..=12 {
            let mut line = String::new();
            for dx in -12..=12 {
                if (dx + dy) & 1 != 0 {
                    line.push_str("  ");
                    continue;
                }
                match stats[&l].get(&(dx, dy)) {
                    Some(&(o, t)) if t >= 3 => {
                        let p = o * 100 / t;
                        line.push_str(&format!("{}", if p >= 100 { 'F' } else { (b'0' + (p / 10) as u8) as char }));
                        line.push(' ');
                    }
                    _ => line.push_str("? "),
                }
            }
            println!("{dy:3} {line}");
        }
    }
}
