//! Print art-reference metadata for the synthetic asset generator; reads only a BIQ.
use civ3_biq::Biq;
fn main() {
    let path = std::env::args().nth(1).expect("usage: asset_refs FILE.biq");
    let b = Biq::read_file(&path).expect("read BIQ");
    for u in &b.rules.unit_types {
        println!("unit\t{}\t{}", u.civilopedia_entry.text(), u.name.text());
    }
    for c in &b.rules.civilizations {
        for p in &c.era_art {
            if !p.text().is_empty() { println!("leader\t{}", p.text()); }
        }
    }
    for t in &b.rules.techs { println!("tech\t{}", t.civilopedia_entry.text()); }
    for b in &b.rules.buildings { println!("wonder\t{}", b.civilopedia_entry.text()); }
}
