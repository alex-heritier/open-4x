use civ3mapgen::oracle;
use civ3mapgen::spiral::spiral_offset;
fn main() {
    let f = oracle::load("tiny_dry_cool").unwrap();
    let a = f.stage("landmass_fix").planes.to_grid(f.width, f.height, f.wrap, f.options.seed);
    for &(x, y) in &[(28, 40), (27, 41), (26, 42)] {
        let mut hits = vec![];
        for n in 1..49 {
            let (dx, dy) = spiral_offset(n);
            if let Some((nx, ny)) = a.wrap_and_check(x + dx, y + dy) {
                let c = a.cell_at(nx, ny).unwrap();
                hits.push(format!("{n}:({nx},{ny})c{}", c.continent));
            }
        }
        println!("({x},{y}) cont {}: {}", a.cell_at(x, y).unwrap().continent, hits.join(" "));
    }
}
