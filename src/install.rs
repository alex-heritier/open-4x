//! The Civ3 install and its file search path (`docs/civ3-files.md` section 7).
//!
//! Civ3's references (`Art\Units\Warrior`, `PediaIcons.txt` entries, a
//! scenario's search folders) and the real file names disagree in case, which
//! only Windows forgives. Everything that turns a Civ3 reference into a file
//! goes through [`Install::resolve`], which matches case-insensitively.

use std::path::{Path, PathBuf};

/// A Civ3 install root plus the folders searched before it.
#[derive(Clone, Debug)]
pub struct Install {
    /// The source asset root (the folder holding `Art/`, `Conquests/`, ...).
    pub root: PathBuf,
    /// Roots searched in order: the scenario's own folder and its search
    /// folders, then `Conquests/`, `civ3PTW/`, then the base install.
    pub search: Vec<PathBuf>,
}

impl Install {
    /// The stock search order of an install, with `extra` (a scenario's own
    /// folder and `GAME.search_folders`) in front.
    pub fn new(root: impl Into<PathBuf>, extra: Vec<PathBuf>) -> Install {
        let root = root.into();
        let mut search = extra;
        for sub in ["Conquests", "civ3PTW"] {
            if let Some(dir) = child(&root, sub) {
                search.push(dir);
            }
        }
        search.push(root.clone());
        Install { root, search }
    }

    /// The first file named `rel` (a Civ3 relative path, either slash, any
    /// case) along the search path.
    pub fn resolve(&self, rel: &str) -> Option<PathBuf> {
        self.search.iter().find_map(|root| resolve_in(root, rel))
    }

    /// Every file `rel` along the search path, nearest first.
    pub fn resolve_all(&self, rel: &str) -> Vec<PathBuf> {
        self.search.iter().filter_map(|root| resolve_in(root, rel)).collect()
    }

    /// Whether the folder `rel` exists somewhere on the path.
    pub fn has_dir(&self, rel: &str) -> bool {
        self.resolve(rel).is_some_and(|p| self.is_dir(&p))
    }

    pub fn is_dir(&self, path: &Path) -> bool {
        #[cfg(target_arch = "wasm32")]
        return crate::web::kind(path) == Some(crate::web::PathKind::Dir);
        #[cfg(not(target_arch = "wasm32"))]
        return path.is_dir();
    }

    pub fn is_file(&self, path: &Path) -> bool {
        #[cfg(target_arch = "wasm32")]
        return crate::web::kind(path) == Some(crate::web::PathKind::File);
        #[cfg(not(target_arch = "wasm32"))]
        return path.is_file();
    }

    /// The scenario folders of a BIQ at `file` with the given
    /// `GAME.search_folders` (semicolon separated, relative to the scenario).
    pub fn scenario_folders(file: &Path, search_folders: &str) -> Vec<PathBuf> {
        let dir = file.parent().map(Path::to_path_buf).unwrap_or_default();
        let dir = if dir.as_os_str().is_empty() { PathBuf::from(".") } else { dir };
        let mut out = vec![dir.clone()];
        for f in search_folders.split(';').map(str::trim).filter(|f| !f.is_empty()) {
            if let Some(p) = resolve_in(&dir, f) {
                out.push(p);
            }
        }
        out
    }
}

/// The entry of `dir` named `name`, ignoring case, with the spelling the
/// file system has (a case-insensitive file system would otherwise echo ours).
fn child(dir: &Path, name: &str) -> Option<PathBuf> {
    let mut fold = None;
    for e in std::fs::read_dir(dir).ok()?.filter_map(Result::ok) {
        let n = e.file_name();
        let n = n.to_string_lossy();
        if n == name {
            return Some(e.path());
        }
        if fold.is_none() && n.eq_ignore_ascii_case(name) {
            fold = Some(e.path());
        }
    }
    fold
}

/// `rel` under `root`, matching each component case-insensitively. `..` and
/// `.` are honored; either slash separates.
pub fn resolve_in(root: &Path, rel: &str) -> Option<PathBuf> {
    #[cfg(target_arch = "wasm32")]
    return crate::web::resolve(root, rel);
    #[cfg(not(target_arch = "wasm32"))]
    let mut at = root.to_path_buf();
    #[cfg(not(target_arch = "wasm32"))]
    for part in rel.split(['/', '\\']).filter(|p| !p.is_empty()) {
        match part {
            "." => {}
            ".." => {
                at = at.parent()?.to_path_buf();
            }
            _ => at = child(&at, part)?,
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    Some(at)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("open4x-install-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn references_match_case_insensitively() {
        let d = dir("case");
        std::fs::create_dir_all(d.join("Art/Units/Warrior")).unwrap();
        std::fs::write(d.join("Art/Units/Warrior/Warrior.INI"), "x").unwrap();
        let hit = resolve_in(&d, "art\\units\\warrior\\warrior.ini").unwrap();
        assert!(hit.ends_with("Art/Units/Warrior/Warrior.INI"));
        assert!(resolve_in(&d, "art/units/archer").is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_scenarios_own_art_is_found_before_the_stock_art() {
        let d = dir("order");
        std::fs::create_dir_all(d.join("Conquests/Art/Units/Warrior")).unwrap();
        std::fs::create_dir_all(d.join("Scenarios/X/Art/Units/Warrior")).unwrap();
        let inst = Install::new(&d, vec![d.join("Scenarios/X")]);
        let hit = inst.resolve("Art/Units/Warrior").unwrap();
        assert!(hit.starts_with(d.join("Scenarios/X")), "{hit:?}");
        // Without the scenario's copy the Conquests one wins.
        let stock = Install::new(&d, vec![]);
        assert!(stock.resolve("Art/Units/Warrior").unwrap().starts_with(d.join("Conquests")));
        assert_eq!(inst.resolve_all("art/units/warrior").len(), 2);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn assets_flag_moves_resolution_without_moving_rules() {
        let d = dir("assets-flag");
        let rules = d.join("install");
        let assets = d.join("subset");
        for root in [&rules, &assets] {
            std::fs::create_dir_all(root.join("Art/Units/Warrior")).unwrap();
        }
        std::fs::write(rules.join("Art/Units/Warrior/Warrior.ini"), "install").unwrap();
        std::fs::write(assets.join("Art/Units/Warrior/Warrior.ini"), "override").unwrap();
        let options = crate::cli::parse(
            ["--civ3", rules.to_str().unwrap(), "--assets", assets.to_str().unwrap()],
            &std::collections::HashMap::<&str, &str>::new(),
        ).unwrap();
        assert_eq!(options.civ3_dir(), rules);
        let install = Install::new(options.assets_dir(), vec![]);
        let picked = install.resolve("art/units/warrior/warrior.INI").unwrap();
        assert!(picked.starts_with(&assets));
        assert_eq!(std::fs::read_to_string(&picked).unwrap(), "override");
        println!("--assets resolution: {} -> override; rules root {}", picked.display(), options.civ3_dir().display());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn scenario_search_folders_are_relative_to_the_scenario() {
        let d = dir("folders");
        std::fs::create_dir_all(d.join("Scenarios/Extras/Medieval Japan")).unwrap();
        std::fs::create_dir_all(d.join("Scenarios/X")).unwrap();
        let file = d.join("Scenarios/X/a.biq");
        let f = Install::scenario_folders(&file, "..\\extras\\medieval japan; nowhere");
        assert_eq!(f.len(), 2);
        assert!(f[1].ends_with("Extras/Medieval Japan"), "{:?}", f[1]);
        let _ = std::fs::remove_dir_all(&d);
    }
}
