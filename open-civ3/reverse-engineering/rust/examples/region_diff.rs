use civ3mapgen::oracle;
fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "tiny_flat".into());
    let f = oracle::load(&name).unwrap();
    let grid = f.stage("paint_continents").planes.to_grid(f.width, f.height, f.wrap, f.options.seed);
    let got = civ3mapgen::regions::paint_regions(&grid, f.options.seed);
    let want: Vec<u16> = f.stage("biomes").planes.region.iter().map(|&r| r as u16).collect();
    let half = (f.width / 2) as usize;
    for i in 0..got.len() {
        if got[i] != want[i] {
            let (x, y) = grid.coords(i);
            println!("cell {i} ({x},{y}) got {:#x} exe {:#x} class {} cont {}", got[i], want[i], grid.cells[i].class(), grid.cells[i].continent);
        }
    }
    // print a window around first diff
    let first = (0..got.len()).find(|&i| got[i] != want[i]);
    if let Some(i) = first {
        let (_, y0) = grid.coords(i);
        for y in (y0.saturating_sub(3))..(y0 + 4).min(f.height) {
            let mut a = String::new();
            let mut b = String::new();
            for cx in 0..half.min(12) {
                let k = (y as usize) * half + cx;
                a += &format!("{:>5x}", got[k]);
                b += &format!("{:>5x}", want[k]);
            }
            println!("y{y:>3} got {a}   exe {b}");
        }
    }
}
