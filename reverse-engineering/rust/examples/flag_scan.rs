use civ3mapgen::oracle;
fn main() {
    let bit: u32 = std::env::args().nth(1).map(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).unwrap()).unwrap_or(0x400000);
    for name in oracle::fixture_names() {
        let f = oracle::load(&name).unwrap();
        let mut row = vec![];
        for st in &f.stages {
            let n = st.planes.feature.iter().filter(|&&v| v & bit != 0).count();
            row.push(format!("{}:{}", st.name, n));
        }
        println!("{name}: {}", row.join(" "));
    }
}
