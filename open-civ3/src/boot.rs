//! Startup: read the file named on the command line, install its rules, and
//! decide what world it describes (`docs/civ3-files.md` sections 1, 2, 5, 6).
//!
//! This runs in `main` before `App::new()`. The rules (`ruleset`) are
//! installed process-wide; the world is handed to the `Startup` systems as the
//! [`Boot`] resource.

use std::path::{Path, PathBuf};

use bevy::prelude::Resource;
use civ3_biq::{Biq, Save};

use crate::cli::{FileKind, Options};
use crate::install::Install;
use crate::ruleset;

/// What the file describes (the table in `docs/civ3-files.md`).
pub enum World {
    /// Rules only (a mod, or `conquests.biq` itself): a random map played by
    /// those rules.
    Random,
    /// A map but no players (`LEAD`): that map, civs picked as for a random game.
    Map(Box<Biq>),
    /// A map and players: the scenario as authored.
    Scenario(Box<Biq>),
    /// A saved game.
    Saved(Box<Save>),
}

#[derive(Resource)]
pub struct Boot {
    pub options: Options,
    pub install: Install,
    /// The file played (the stock `conquests.biq` by default).
    pub file: PathBuf,
    pub world: World,
    /// Shared by games using the same rules and asset search roots.
    pub cache_namespace: String,
}

/// Whether a BIQ carries the rule sections (units, buildings, advances...).
pub fn has_rules(biq: &Biq) -> bool {
    let r = &biq.rules;
    !r.unit_types.is_empty()
        && !r.buildings.is_empty()
        && !r.techs.is_empty()
        && !r.terrains.is_empty()
        && !r.civilizations.is_empty()
        && !r.general_rules.is_empty()
        && !r.governments.is_empty()
        && !r.difficulties.is_empty()
}

/// Whether a BIQ carries a map.
pub fn has_map(biq: &Biq) -> bool {
    biq.map_view().is_some()
}

/// Whether a BIQ carries players (`LEAD` rows).
pub fn has_players(biq: &Biq) -> bool {
    !biq.scenario.players.is_empty()
}

/// Which world a BIQ describes.
pub fn world_of(biq: Biq) -> World {
    match (has_map(&biq), has_players(&biq)) {
        (true, true) => World::Scenario(Box::new(biq)),
        (true, false) => World::Map(Box::new(biq)),
        (false, _) => World::Random,
    }
}

/// `GAME.search_folders` of a BIQ (empty without a `GAME` row).
fn search_folders(biq: &Biq) -> String {
    biq.scenario
        .game
        .first()
        .map(|g| g.search_folders.text().to_string())
        .unwrap_or_default()
}

fn stock_path(install_root: &Path) -> Option<PathBuf> {
    crate::install::resolve_in(install_root, "Conquests/conquests.biq").filter(|p| p.is_file())
}

fn read_biq(path: &Path) -> Result<Biq, String> {
    Biq::read_file(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Read the file the options name and install its rules. `Err` is a message
/// for the user.
pub fn load(options: &Options) -> Result<Boot, String> {
    let root = options.civ3_dir();
    let file = options.file.clone().unwrap_or_else(|| {
        stock_path(&root).unwrap_or_else(|| root.join("Conquests").join("conquests.biq"))
    });
    if !file.is_file() {
        return Err(format!("{}: no such file", file.display()));
    }
    let stock = || -> Result<Biq, String> {
        if !root.is_dir() {
            return Err(format!(
                "no Civ3 install at {} (pass --civ3 <dir> or set CIV3_DIR)",
                root.display()
            ));
        }
        let path = stock_path(&root)
            .ok_or_else(|| format!("no Conquests/conquests.biq under {}", root.display()))?;
        read_biq(&path)
    };

    let (rules_from, world, folders): (Biq, World, Vec<PathBuf>) = match options.kind() {
        FileKind::Sav => {
            let save = Save::read_file(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            let embedded = save
                .embedded_biq()
                .map_err(|e| format!("{}: embedded scenario: {e}", file.display()))?;
            let scenario = save.scenario_file().replace('\\', "/");
            let recorded = PathBuf::from(&scenario);
            let source = recorded
                .is_file()
                .then_some(recorded)
                .or_else(|| crate::install::resolve_in(&root, &scenario))
                .filter(|p| p.is_file())
                .unwrap_or_else(|| file.clone());
            let folders = Install::scenario_folders(&source, &search_folders(&embedded));
            let rules_from = if has_rules(&embedded) {
                embedded
            } else {
                stock()?
            };
            (rules_from, World::Saved(Box::new(save)), folders)
        }
        FileKind::Biq => {
            let biq = read_biq(&file)?;
            let folders = Install::scenario_folders(&file, &search_folders(&biq));
            if has_rules(&biq) {
                let rules_from = biq.clone();
                (rules_from, world_of(biq), folders)
            } else {
                // A scenario without rules uses the defaults, like Civ3 does.
                let rules_from = stock()?;
                (rules_from, world_of(biq), folders)
            }
        }
    };

    let assets = options.assets_dir();
    if !assets.is_dir() {
        return Err(format!(
            "no original assets at {} (pass --assets <dir> or set CIV3_ASSETS)",
            assets.display()
        ));
    }
    let install = Install::new(&assets, folders);
    let stock_biq = stock_path(&root).and_then(|p| read_biq(&p).ok());
    let stock_install = Install::new(&root, vec![]);
    let cache_namespace = crate::assets::scenario_namespace(
        &rules_from,
        &install,
        stock_biq.as_ref().map(|b| (b, &stock_install)),
    );
    ruleset::install(ruleset::build(&rules_from, &install));
    Ok(Boot {
        options: options.clone(),
        install,
        file,
        world,
        cache_namespace,
    })
}

/// Start the stock game from data bundled with the WASM binary and the
/// converted art cache served beside it.
#[cfg(target_arch = "wasm32")]
pub fn load_web(options: &Options) -> Result<Boot, String> {
    let biq =
        Biq::parse(crate::web::stock_biq()).map_err(|e| format!("stock conquests.biq: {e}"))?;
    let install = crate::web::install();
    let file = PathBuf::from("Conquests/conquests.biq");
    let world = world_of(biq);
    let rules_from = match &world {
        World::Map(biq) | World::Scenario(biq) => (**biq).clone(),
        World::Random => {
            Biq::parse(crate::web::stock_biq()).map_err(|e| format!("stock conquests.biq: {e}"))?
        }
        World::Saved(_) => unreachable!("the bundled file is a BIQ"),
    };
    ruleset::install(ruleset::build(&rules_from, &install));
    Ok(Boot {
        options: options.clone(),
        install,
        file,
        world,
        cache_namespace: "civ3".into(),
    })
}
