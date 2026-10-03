//! Scratch tool: decompress every scenario/save under the given roots into
//! `/tmp/biq/<mangled-name>.raw` for ad-hoc analysis.
use std::path::{Path, PathBuf};

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if let Some(ext) = p.extension().and_then(|s| s.to_str()) {
            let ext = ext.to_ascii_lowercase();
            if matches!(ext.as_str(), "biq" | "bic" | "bix" | "sav") {
                out.push(p);
            }
        }
    }
}

fn main() {
    let roots: Vec<String> = std::env::args().skip(1).collect();
    let mut files = Vec::new();
    for r in &roots {
        walk(Path::new(r), &mut files);
    }
    files.sort();
    std::fs::create_dir_all("/tmp/biq").unwrap();
    let mut seen = std::collections::HashSet::new();
    for f in files {
        let raw = std::fs::read(&f).unwrap();
        let comp = civ3_biq::dcl::looks_compressed(&raw);
        let dec = if comp {
            match civ3_biq::dcl::decompress(&raw) {
                Ok(d) => d,
                Err(e) => {
                    println!("FAIL {} ({e})", f.display());
                    continue;
                }
            }
        } else {
            raw.clone()
        };
        // dedupe byte-identical decoded streams (Steam and GOG copies)
        let h = {
            use std::hash::{Hash, Hasher};
            let mut s = std::collections::hash_map::DefaultHasher::new();
            dec.hash(&mut s);
            s.finish()
        };
        let dup = !seen.insert(h);
        let name = f
            .strip_prefix("/Users/alex/code/project/open-4x/civ3/")
            .unwrap_or(&f)
            .to_string_lossy()
            .replace('/', "__")
            .replace(' ', "_");
        let magic = String::from_utf8_lossy(&dec[..4.min(dec.len())]).into_owned();
        println!(
            "{} {:>9} -> {:>9} {} {:?} {}",
            if comp { "DCL" } else { "RAW" },
            raw.len(),
            dec.len(),
            if dup { "dup" } else { "   " },
            magic,
            f.display()
        );
        if !dup {
            std::fs::write(format!("/tmp/biq/{name}.raw"), &dec).unwrap();
        }
    }
}
