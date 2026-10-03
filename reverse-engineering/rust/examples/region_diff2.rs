use civ3mapgen::oracle;
fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "tiny_flat".into());
    let cont: u16 = std::env::args().nth(2).unwrap().parse().unwrap();
    let reg: u16 = u16::from_str_radix(&std::env::args().nth(3).unwrap(), 16).unwrap();
    let f = oracle::load(&name).unwrap();
    let grid = f.stage("paint_continents").planes.to_grid(f.width, f.height, f.wrap, f.options.seed);
    let got = civ3mapgen::regions::paint_regions(&grid, f.options.seed);
    let want: Vec<u16> = f.stage("biomes").planes.region.iter().map(|&r| r as u16).collect();
    for (label, m) in [("got", &got), ("exe", &want)] {
        let cells: Vec<_> = (0..m.len()).filter(|&i| grid.cells[i].continent == cont && m[i] == reg).map(|i| grid.coords(i)).collect();
        println!("{label}: region {reg:#x} of continent {cont}: {cells:?}");
    }
}
