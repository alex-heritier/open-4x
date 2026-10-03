use civ3mapgen::oracle::{self, PLANE_NAMES};
fn main() {
    for name in oracle::fixture_names() {
        let f = oracle::load(&name).unwrap();
        println!("== {name} ({}x{}, wrap {}, civs {})", f.width, f.height, f.wrap, f.civs);
        for i in 0..f.stages.len() - 1 {
            let a = &f.stages[i].planes;
            let b = &f.stages[i + 1].planes;
            let d = a.diff(b, &PLANE_NAMES);
            let s: Vec<String> = d.iter().map(|p| format!("{}:{}", p.plane, p.count)).collect();
            println!("  {:<18} -> {:<18} {}", f.stages[i].name, f.stages[i + 1].name, s.join(" "));
        }
    }
}
