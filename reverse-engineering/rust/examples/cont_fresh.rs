use civ3mapgen::continents::number_continents;
use civ3mapgen::oracle;
fn main() {
    for n in oracle::fixture_names() {
        let f = oracle::load(&n).unwrap();
        let mut line = format!("{n:24}");
        for s in &f.stages {
            let Some(rec) = &s.continents else { continue };
            let mut grid = s.planes.to_grid(f.width, f.height, f.wrap, f.options.seed);
            let before: Vec<u16> = grid.cells.iter().map(|c| c.continent).collect();
            let fresh = number_continents(&mut grid);
            let after: Vec<u16> = grid.cells.iter().map(|c| c.continent).collect();
            let same_rec = fresh.len() == rec.len() && fresh.iter().zip(rec).all(|(a, b)| a.is_land == b.0 && a.size == b.1);
            let same_plane = before == after;
            line += &format!(" {}:{}{}", &s.name[..s.name.len().min(6)], if same_rec { 'R' } else { 'r' }, if same_plane { 'P' } else { 'p' });
        }
        println!("{line}");
    }
}
