//! The command line (`docs/civ3-files.md` section 1).
//!
//! ```text
//! open-4x [FILE] [options]
//!
//!   FILE                  .biq/.bix/.bic scenario or mod, or .sav saved game
//!                         (default: <civ3>/Conquests/conquests.biq, a new random game)
//!   --civ3 <dir>          Civ3 install root (default: $CIV3_DIR, else civ3/civ3-gog/app)
//!   --assets <dir>        original asset root (default: $CIV3_ASSETS, else --civ3)
//!
//! New games (ignored for a .sav):
//!   --civ <name>          the human's civilization (RACE name, case-insensitive)
//!   --opponents <n|names> number of rivals, or a comma list of RACE names
//!   --difficulty <name>   DIFF row (default: RULE.default_difficulty)
//!   --size <name>         WSIZ row (default: Standard)
//!   --land, --water, --climate, --temperature, --age, --barbarians
//!                         WCHR settings, as Civ3's custom-world screen names them
//!   --seed <n>            map seed
//! ```
//!
//! The older environment variables fold into the same options: `CIV3_PLAYER`
//! is `--civ`, `CIV3_CIVS` is `--civ` plus `--opponents`, `MAP_SEED` is
//! `--seed`, `CIV3_GOG` is `--civ3`, `CIV3_ASSETS` is `--assets`. A flag beats
//! its variable. The dev and
//! debug variables (`CIV3_SHOT`, `CIV3_SCRIPT`, `CIV3_AUTOPLAY`, ...) stay
//! environment variables, so the screenshot and script tooling keeps working.
//!
//! The options are parsed once in `main`, before `App::new()`, and read
//! through [`options`].

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Where the Civ3 install is looked for when neither `--civ3` nor the
/// environment names it.
pub const DEFAULT_CIV3_DIR: &str = "civ3/civ3-gog/app";

/// What kind of file the command line named.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FileKind {
    /// `.biq` / `.bix` / `.bic`: rules, and maybe a map and players.
    Biq,
    /// `.sav`: a saved game.
    Sav,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Opponents {
    Count(usize),
    Names(Vec<String>),
}

/// The parsed command line.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Options {
    /// The `.biq` / `.sav` to play; `None` is the stock `conquests.biq`.
    pub file: Option<PathBuf>,
    /// `--civ3`; `None` resolves to `$CIV3_DIR`, `$CIV3_GOG`, then the default.
    pub civ3: Option<PathBuf>,
    /// `--assets`; `None` resolves to `$CIV3_ASSETS`, then `civ3_dir()`.
    pub assets: Option<PathBuf>,
    pub civ: Option<String>,
    pub opponents: Option<Opponents>,
    pub difficulty: Option<String>,
    pub size: Option<String>,
    pub land: Option<String>,
    pub water: Option<String>,
    pub climate: Option<String>,
    pub temperature: Option<String>,
    pub age: Option<String>,
    pub barbarians: Option<String>,
    pub seed: Option<u64>,
}

pub const USAGE: &str = "usage: open-4x [FILE] [options]

  FILE                  .biq/.bix/.bic scenario or mod, or .sav saved game
                        (default: <civ3>/Conquests/conquests.biq, a new random game)
  --civ3 <dir>          Civ3 install root (default: $CIV3_DIR, else civ3/civ3-gog/app)
  --assets <dir>        original asset root (default: $CIV3_ASSETS, else --civ3)

New games (ignored for a .sav):
  --civ <name>          the human's civilization (RACE name, case-insensitive)
  --opponents <n|names> number of rivals, or a comma list of RACE names
  --difficulty <name>   DIFF row (default: the rules' default difficulty)
  --size <name>         WSIZ row (default: Standard)
  --land <name>         landmass: Archipelago, Continents, Pangaea
  --water <name>        ocean coverage: 80%, 70%, 60%
  --climate <name>      Arid, Normal, Wet
  --temperature <name>  Cool, Temperate, Warm
  --age <name>          3 Billion, 4 Billion, 5 Billion
  --barbarians <name>   None, Sedentary, Roaming, Restless, Raging
  --seed <n>            map seed
  -h, --help            this text";

/// Environment variables the older tooling sets, as `(name, getter)`.
pub trait Env {
    fn get(&self, name: &str) -> Option<String>;
}

struct ProcessEnv;
impl Env for ProcessEnv {
    fn get(&self, name: &str) -> Option<String> {
        std::env::var(name).ok().filter(|s| !s.is_empty())
    }
}

#[cfg(test)]
impl Env for std::collections::HashMap<&str, &str> {
    fn get(&self, name: &str) -> Option<String> {
        std::collections::HashMap::get(self, name).map(|s| s.to_string())
    }
}

/// Parse `args` (without the program name) over `env`.
///
/// `Err` carries a message for the user; `Err("")`-style help is spelled
/// `Err(USAGE)`.
pub fn parse<I, S>(args: I, env: &dyn Env) -> Result<Options, String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut o = Options::default();
    let mut args = args.into_iter().map(Into::into);
    let mut positional_only = false;
    while let Some(a) = args.next() {
        if !positional_only && a == "--" {
            positional_only = true;
            continue;
        }
        if positional_only || !a.starts_with('-') || a == "-" {
            if o.file.is_some() {
                return Err(format!("more than one file given ({a})\n\n{USAGE}"));
            }
            o.file = Some(a.into());
            continue;
        }
        if a == "-h" || a == "--help" {
            return Err(USAGE.to_string());
        }
        // `--flag value` and `--flag=value`.
        let (flag, inline) = match a.split_once('=') {
            Some((f, v)) => (f.to_string(), Some(v.to_string())),
            None => (a.clone(), None),
        };
        let value = |args: &mut dyn Iterator<Item = String>| -> Result<String, String> {
            inline
                .clone()
                .or_else(|| args.next())
                .ok_or_else(|| format!("{flag} needs a value\n\n{USAGE}"))
        };
        match flag.as_str() {
            "--civ3" => o.civ3 = Some(value(&mut args)?.into()),
            "--assets" => o.assets = Some(value(&mut args)?.into()),
            "--civ" => o.civ = Some(value(&mut args)?),
            "--opponents" => {
                let v = value(&mut args)?;
                o.opponents = Some(match v.trim().parse::<usize>() {
                    Ok(n) => Opponents::Count(n),
                    Err(_) => Opponents::Names(split_names(&v)),
                });
            }
            "--difficulty" => o.difficulty = Some(value(&mut args)?),
            "--size" => o.size = Some(value(&mut args)?),
            "--land" => o.land = Some(value(&mut args)?),
            "--water" => o.water = Some(value(&mut args)?),
            "--climate" => o.climate = Some(value(&mut args)?),
            "--temperature" => o.temperature = Some(value(&mut args)?),
            "--age" => o.age = Some(value(&mut args)?),
            "--barbarians" => o.barbarians = Some(value(&mut args)?),
            "--seed" => {
                let v = value(&mut args)?;
                o.seed = Some(
                    v.trim()
                        .parse()
                        .map_err(|_| format!("--seed: {v:?} is not a number"))?,
                );
            }
            _ => return Err(format!("unknown option {flag}\n\n{USAGE}")),
        }
    }
    o.fold_env(env);
    Ok(o)
}

fn split_names(list: &str) -> Vec<String> {
    list.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

impl Options {
    /// The older environment variables fill what no flag set.
    fn fold_env(&mut self, env: &dyn Env) {
        if self.seed.is_none() {
            self.seed = env.get("MAP_SEED").and_then(|s| s.trim().parse().ok());
        }
        if self.civ3.is_none() {
            self.civ3 = env
                .get("CIV3_DIR")
                .or_else(|| env.get("CIV3_GOG"))
                .map(PathBuf::from);
        }
        if self.assets.is_none() {
            self.assets = env.get("CIV3_ASSETS").map(PathBuf::from);
        }
        // `CIV3_CIVS` lists the human first; `CIV3_PLAYER` then overrides the human.
        let listed = env
            .get("CIV3_CIVS")
            .map(|l| split_names(&l))
            .unwrap_or_default();
        if self.civ.is_none() {
            self.civ = env
                .get("CIV3_PLAYER")
                .map(|s| s.trim().to_string())
                .or_else(|| listed.first().cloned());
        }
        if self.opponents.is_none() && listed.len() > 1 {
            self.opponents = Some(Opponents::Names(listed[1..].to_vec()));
        }
    }

    /// The Civ3 install root.
    pub fn civ3_dir(&self) -> PathBuf {
        self.civ3.clone().unwrap_or_else(|| DEFAULT_CIV3_DIR.into())
    }

    /// The original asset root; defaults to the Civ3 install root.
    pub fn assets_dir(&self) -> PathBuf {
        self.assets.clone().unwrap_or_else(|| self.civ3_dir())
    }

    /// The file to play: the named one, else the stock `conquests.biq`.
    pub fn file_or_default(&self) -> PathBuf {
        self.file
            .clone()
            .unwrap_or_else(|| self.civ3_dir().join("Conquests").join("conquests.biq"))
    }

    /// `.sav` is a saved game, anything else a BIQ.
    pub fn kind(&self) -> FileKind {
        kind_of(&self.file_or_default())
    }
}

pub fn kind_of(path: &Path) -> FileKind {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("sav") => FileKind::Sav,
        _ => FileKind::Biq,
    }
}

static OPTIONS: OnceLock<Options> = OnceLock::new();

/// Parse the process's command line and environment, exiting with a message
/// on a bad one. Call once, first thing in `main`.
pub fn init() -> &'static Options {
    let parsed = match parse(std::env::args().skip(1), &ProcessEnv) {
        Ok(o) => o,
        Err(msg) => {
            let help = msg == USAGE;
            if help {
                println!("{msg}");
            } else {
                eprintln!("open-4x: {msg}");
            }
            std::process::exit(if help { 0 } else { 2 });
        }
    };
    OPTIONS.get_or_init(|| parsed)
}

/// The command line. Defaults when `init` has not run (tests).
pub fn options() -> &'static Options {
    OPTIONS.get_or_init(|| parse(std::iter::empty::<String>(), &ProcessEnv).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn p(args: &[&str]) -> Result<Options, String> {
        parse(args.iter().copied(), &HashMap::new())
    }

    #[test]
    fn no_arguments_is_the_stock_game() {
        let o = p(&[]).unwrap();
        assert_eq!(o, Options::default());
        assert_eq!(o.kind(), FileKind::Biq);
        assert!(o.file_or_default().ends_with("Conquests/conquests.biq"));
    }

    #[test]
    fn a_file_and_flags() {
        let o = p(&["Game.SAV", "--seed", "7", "--civ=Rome", "--opponents", "3"]).unwrap();
        assert_eq!(o.kind(), FileKind::Sav);
        assert_eq!(o.seed, Some(7));
        assert_eq!(o.civ.as_deref(), Some("Rome"));
        assert_eq!(o.opponents, Some(Opponents::Count(3)));
    }

    #[test]
    fn opponent_names() {
        let o = p(&["--opponents", "Egypt, China"]).unwrap();
        assert_eq!(
            o.opponents,
            Some(Opponents::Names(vec!["Egypt".into(), "China".into()]))
        );
    }

    #[test]
    fn the_world_flags() {
        let o = p(&[
            "x.biq",
            "--size",
            "Huge",
            "--land",
            "Pangaea",
            "--water",
            "60%",
            "--climate",
            "Wet",
            "--temperature",
            "Warm",
            "--age",
            "5 Billion",
            "--barbarians",
            "Raging",
            "--difficulty",
            "Deity",
        ])
        .unwrap();
        assert_eq!(o.size.as_deref(), Some("Huge"));
        assert_eq!(o.land.as_deref(), Some("Pangaea"));
        assert_eq!(o.water.as_deref(), Some("60%"));
        assert_eq!(o.climate.as_deref(), Some("Wet"));
        assert_eq!(o.temperature.as_deref(), Some("Warm"));
        assert_eq!(o.age.as_deref(), Some("5 Billion"));
        assert_eq!(o.barbarians.as_deref(), Some("Raging"));
        assert_eq!(o.difficulty.as_deref(), Some("Deity"));
    }

    #[test]
    fn environment_variables_fold_in_and_flags_win() {
        let env: HashMap<&str, &str> = HashMap::from([
            ("CIV3_CIVS", "Rome, Egypt, China"),
            ("MAP_SEED", "99"),
            ("CIV3_GOG", "/opt/civ3"),
        ]);
        let o = parse(["--seed", "5"], &env).unwrap();
        assert_eq!(o.seed, Some(5));
        assert_eq!(o.civ.as_deref(), Some("Rome"));
        assert_eq!(
            o.opponents,
            Some(Opponents::Names(vec!["Egypt".into(), "China".into()]))
        );
        assert_eq!(o.civ3_dir(), PathBuf::from("/opt/civ3"));
        assert_eq!(o.assets_dir(), o.civ3_dir());
        let env: HashMap<&str, &str> = HashMap::from([
            ("CIV3_PLAYER", "Japan"),
            ("CIV3_DIR", "/a"),
            ("CIV3_GOG", "/b"),
            ("CIV3_ASSETS", "/art"),
        ]);
        let o = parse::<_, &str>([], &env).unwrap();
        assert_eq!(o.civ.as_deref(), Some("Japan"));
        assert_eq!(o.civ3_dir(), PathBuf::from("/a"));
        assert_eq!(o.assets_dir(), PathBuf::from("/art"));
        let o = parse(["--civ", "Rome", "--civ3", "/c", "--assets=/stub"], &env).unwrap();
        assert_eq!(o.civ.as_deref(), Some("Rome"));
        assert_eq!(o.civ3_dir(), PathBuf::from("/c"));
        assert_eq!(o.assets_dir(), PathBuf::from("/stub"));
        assert_eq!(
            p(&["--civ3", "/custom"]).unwrap().assets_dir(),
            PathBuf::from("/custom")
        );
        assert_eq!(p(&[]).unwrap().assets_dir(), p(&[]).unwrap().civ3_dir());
    }

    #[test]
    fn bad_command_lines() {
        assert!(
            p(&["--nope"])
                .unwrap_err()
                .contains("unknown option --nope")
        );
        assert!(p(&["--seed"]).unwrap_err().contains("needs a value"));
        assert!(p(&["--assets"]).unwrap_err().contains("needs a value"));
        assert!(p(&["--seed", "x"]).unwrap_err().contains("not a number"));
        assert!(
            p(&["a.biq", "b.biq"])
                .unwrap_err()
                .contains("more than one file")
        );
        assert_eq!(p(&["--help"]).unwrap_err(), USAGE);
    }
}
