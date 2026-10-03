//! Locate the game install and load every scenario in it. Used by the tests
//! and examples; the files are the project's ground truth and are *not* part
//! of the repository (`civ3/` is git-ignored), so everything here degrades to
//! "nothing found" when the install is absent.

use crate::Version;
use crate::io::{Ctx, Reader, Record};
use crate::raw::{Magic, Raw};
use std::path::{Path, PathBuf};

/// One decoded file from the install.
pub struct CorpusFile {
    /// Where it was found.
    pub path: PathBuf,
    /// Decoded and framed stream.
    pub raw: Raw,
    /// The `VER#` version (`0.00` if the header is unreadable).
    pub version: Version,
}

impl CorpusFile {
    /// Short display name (`dir/file`).
    pub fn name(&self) -> String {
        let comps: Vec<_> = self.path.components().rev().take(2).collect();
        let mut s = String::new();
        for c in comps.iter().rev() {
            if !s.is_empty() {
                s.push('/');
            }
            s.push_str(&c.as_os_str().to_string_lossy());
        }
        s
    }

    /// Reader context for this file.
    pub fn ctx(&self) -> Ctx {
        Ctx {
            version: self.version,
        }
    }
}

/// Root of the game install: `$CIV3_DIR`, else `<repo>/civ3`.
pub fn install_root() -> PathBuf {
    if let Ok(p) = std::env::var("CIV3_DIR") {
        return PathBuf::from(p);
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../civ3")
}

/// Whether the install holds any scenario file at all. Tests that assert on
/// corpus statistics return early when it does not, so a checkout without the
/// (git-ignored) install passes instead of failing.
pub fn available() -> bool {
    let mut paths = Vec::new();
    walk(&install_root(), &mut paths);
    !paths.is_empty()
}

pub(crate) fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    walk_ext(dir, &["biq", "bic", "bix"], out);
}

/// Every file under `dir` whose extension (any case) is in `exts`, sorted,
/// skipping hidden directories and the RE scratch tree.
pub(crate) fn walk_ext(dir: &Path, exts: &[&str], out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            // skip the Wine prefix and RE scratch tree
            let n = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if n.starts_with('.') || n == "re" {
                continue;
            }
            walk_ext(&p, exts, out);
        } else if let Some(ext) = p.extension().and_then(|s| s.to_str())
            && exts.contains(&ext.to_ascii_lowercase().as_str())
        {
            out.push(p);
        }
    }
}

/// Every `.biq`/`.bic`/`.bix` in the install, decoded and de-duplicated by
/// content (the GOG and Steam trees ship identical copies). Files that fail to
/// frame are returned as errors so tests can fail loudly.
pub fn load_all() -> Vec<Result<CorpusFile, (PathBuf, crate::io::Error)>> {
    let root = install_root();
    let mut paths = Vec::new();
    walk(&root, &mut paths);
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for p in paths {
        let Ok(bytes) = std::fs::read(&p) else {
            continue;
        };
        match Raw::parse(&bytes) {
            Ok(raw) => {
                if !seen.insert(raw.data.clone()) {
                    continue;
                }
                let version = version_of(&raw);
                out.push(Ok(CorpusFile {
                    path: p,
                    raw,
                    version,
                }));
            }
            Err(e) => out.push(Err((p, e))),
        }
    }
    out
}

/// Like [`load_all`] but only the files that parsed.
pub fn files() -> Vec<CorpusFile> {
    load_all().into_iter().filter_map(|r| r.ok()).collect()
}

/// `VER#` major/minor of a framed stream.
pub fn version_of(raw: &Raw) -> Version {
    if raw.magic == Magic::Civ3 {
        return Version::default();
    }
    let Some(sec) = raw.section(b"VER#") else {
        return Version::default();
    };
    let Some(row) = sec.rows.first() else {
        return Version::default();
    };
    let mut r = Reader::new(raw.row(row));
    r.skip(8);
    let major = r.u32().unwrap_or(0);
    let minor = r.u32().unwrap_or(0);
    Version { major, minor }
}

/// One row of type `R` located in a corpus file.
pub struct RowRef<'a> {
    /// The file.
    pub file: &'a CorpusFile,
    /// Row index within its section.
    pub index: usize,
    /// Row body (no length word).
    pub body: &'a [u8],
}

/// Every row of `R`'s section in `files`.
pub fn rows<'a, R: Record>(files: &'a [CorpusFile]) -> Vec<RowRef<'a>> {
    let mut out = Vec::new();
    for f in files {
        for sec in f.raw.sections.iter().filter(|s| s.tag == R::TAG) {
            for (index, r) in sec.rows.iter().enumerate() {
                out.push(RowRef {
                    file: f,
                    index,
                    body: f.raw.row(r),
                });
            }
        }
    }
    out
}

/// What [`check_roundtrip`] found.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RoundtripStats {
    /// Rows examined.
    pub rows: usize,
    /// Always `0`: kept only so older tests compile. `check_roundtrip` now
    /// requires exact equality, which subsumes it.
    pub short: usize,
    /// Rows with bytes this crate does not model (`extra()` non-empty).
    pub with_extra: usize,
    /// Distinct row lengths seen, ascending.
    pub lengths: Vec<usize>,
    /// Distinct row lengths per file version, e.g. `12.08 -> [1236]`.
    pub by_version: std::collections::BTreeMap<Version, Vec<usize>>,
}

impl RoundtripStats {
    /// One line per version with the row lengths seen (for `--nocapture`
    /// output while working out where a layout grew).
    pub fn report(&self) -> String {
        let mut s = format!(
            "{} rows, {} short, {} with extra\n",
            self.rows, self.short, self.with_extra
        );
        for (v, l) in &self.by_version {
            s.push_str(&format!("  {v}: {l:?}\n"));
        }
        s
    }
}

/// Parse every row of `R` in the corpus and verify the invariants every
/// section must satisfy:
///
/// * decoding never fails;
/// * `write` with the file's own version reproduces the original row **byte
///   for byte** (same length, same content), so no field was mis-sized,
///   mis-ordered or dropped, and every version-dependent layout is modelled;
///
/// Panics with the file and row on the first violation. Callers then assert on
/// the returned stats (typically `with_extra == 0`).
pub fn check_roundtrip<R: Record + std::fmt::Debug>() -> RoundtripStats {
    let files = files();
    let mut st = RoundtripStats::default();
    let mut lens = std::collections::BTreeSet::new();
    let mut per_version: std::collections::BTreeMap<Version, std::collections::BTreeSet<usize>> =
        Default::default();
    for rr in rows::<R>(&files) {
        let ctx = rr.file.ctx();
        let mut r = Reader::new(rr.body);
        let rec = R::read(&mut r, &ctx)
            .unwrap_or_else(|e| panic!("{} row {}: {e}", rr.file.name(), rr.index));
        let mut w = crate::io::Writer::new();
        rec.write(&mut w, &ctx);
        let out = w.buf;
        assert!(
            out == rr.body,
            "{} row {} (v{}): re-encoded row differs ({} -> {} bytes)\n  in : {:02x?}\n  out: {:02x?}",
            rr.file.name(),
            rr.index,
            rr.file.version,
            rr.body.len(),
            out.len(),
            &rr.body[..rr.body.len().min(96)],
            &out[..out.len().min(96)]
        );
        st.rows += 1;
        if !rec.extra().is_empty() {
            st.with_extra += 1;
        }
        lens.insert(rr.body.len());
        per_version
            .entry(rr.file.version)
            .or_default()
            .insert(rr.body.len());
    }
    st.lengths = lens.into_iter().collect();
    st.by_version = per_version
        .into_iter()
        .map(|(v, l)| (v, l.into_iter().collect()))
        .collect();
    st
}
