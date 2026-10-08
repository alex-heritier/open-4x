//! Dump the border shape around cities of a save.
//!
//! `border_probe FILE [MIN_GAP] [MIN_LEVEL]`: only cities with no other city
//! within `MIN_GAP` (wrapped x distance + y distance) and at least
//! `MIN_LEVEL` culture level are drawn. `.` is unowned, `#` the city's own
//! civ, `x` another civ's.
use civ3_biq::Save;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("save");
    let gap: i32 = args.next().map_or(24, |s| s.parse().unwrap());
    let min_level: u32 = args.next().map_or(0, |s| s.parse().unwrap());
    let bytes = std::fs::read(&path).unwrap();
    let s = Save::parse(&bytes).unwrap();
    let (w, h) = (s.map.width() as i32, s.map.height() as i32);
    for c in &s.cities {
        let (cx, cy) = (c.x() as i32, c.y() as i32);
        let owner = c.owner();
        let level = c.block_20.u32(0x5C - 0x20);
        let near = s.cities.iter().any(|o| {
            if o.id() == c.id() {
                return false;
            }
            let dx = (o.x() as i32 - cx).abs();
            let dx = dx.min(w - dx);
            dx + (o.y() as i32 - cy).abs() <= gap
        });
        if near || level < min_level {
            continue;
        }
        let culture = c.block_13c.u32(4 + 4 * owner as usize);
        println!(
            "== {} owner {owner} size {} culture {culture} level {level} at ({cx},{cy})",
            c.name(),
            c.size()
        );
        for dy in -12..=12 {
            let y = cy + dy;
            if y < 0 || y >= h {
                continue;
            }
            let mut line = String::new();
            for dx in -12..=12 {
                let x = cx + dx;
                let ch = if (x + y) & 1 != 0 {
                    ' '
                } else if let Some(t) = s.map.tile((x % w + w) as u32 % w as u32, y as u32) {
                    if dx == 0 && dy == 0 {
                        '@'
                    } else if t.owner() == owner {
                        '#'
                    } else if t.owner() != 0 {
                        'x'
                    } else {
                        '.'
                    }
                } else {
                    '?'
                };
                line.push(ch);
            }
            println!("{dy:3} {line}");
        }
    }
}
