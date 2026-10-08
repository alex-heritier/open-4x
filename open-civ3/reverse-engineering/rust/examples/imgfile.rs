use civ3mapgen::oracle;
use std::collections::BTreeMap;
fn main() {
    let name = std::env::args().nth(1).unwrap_or("tiny_continents".into());
    let f = oracle::load(&name).unwrap();
    for st in ["hills", "post_process", "resources", "end"] {
        let p = &f.stage(st).planes;
        let mut m: BTreeMap<(u32, u32), usize> = BTreeMap::new();
        for i in 0..p.len() {
            *m.entry((p.file[i], p.image[i])).or_default() += 1;
        }
        let top: Vec<String> = m.iter().take(40).map(|((fl, im), n)| format!("({fl:#x},{im:#x}):{n}")).collect();
        println!("{st}: {} distinct (file,image): {}", m.len(), top.join(" "));
    }
}
