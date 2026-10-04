//! terrain-builder: compiles a terrain pack (a handful of AI-generated
//! ground materials) into the Civ3-format 9x9 transition sheets open-4x
//! renders, plus the single terrain tiles the city screen uses.
//!
//! ```text
//! materials (AI, fal.ai) ─┐
//! transition masks ───────┼─> compositor ─> xtgc.png xpgc.png ... wCSO.png
//!   (from Civ3 or noise)  ┘                 grassland_0.png ...
//! ```
//!
//! The sheets are a compiled format: nobody edits them by hand. See
//! README.md.

mod compose;
mod fal;
mod geom;
mod masks;
mod material;
mod noise;
mod pack;
mod pcx;
mod preview;
mod validate;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use compose::{ComposeParams, Palette};
use geom::{Kind, SHEETS, SheetSpec};
use masks::SheetMasks;
use material::Material;
use pack::{MaskSource, Pack};

#[derive(Parser)]
#[command(version, about = "Build Civ3-style terrain transition sheets for open-4x from a few AI-generated materials")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create a pack directory with a pack.json of default prompts.
    Init {
        dir: PathBuf,
        #[arg(long)]
        name: Option<String>,
        /// Look shared by every terrain, appended to each prompt.
        #[arg(long, default_value = "")]
        style: String,
        /// Also write noise placeholder materials, so `build` works without AI.
        #[arg(long)]
        placeholder: bool,
    },
    /// Generate the pack's materials with fal.ai (needs FAL_KEY).
    Generate {
        dir: PathBuf,
        /// Only these terrains (comma separated).
        #[arg(long, value_delimiter = ',')]
        only: Vec<String>,
        /// Regenerate materials that already exist.
        #[arg(long)]
        force: bool,
        /// Print the prompts without calling the API.
        #[arg(long)]
        dry_run: bool,
    },
    /// Compile the pack into sheets, tiles, mask debug views and a preview.
    Build {
        dir: PathBuf,
        /// Output directory (default: <pack>/out).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Civ3 `Art/Terrain` directory (default: $CIV3_GOG/Art/Terrain or
        /// civ3/civ3-gog/app/Art/Terrain in the repo).
        #[arg(long)]
        civ3: Option<PathBuf>,
        /// Copy the result into the game's assets/gen/terrain.
        #[arg(long)]
        install: bool,
        /// Game asset directory for --install (default: <repo>/assets/gen/terrain).
        #[arg(long)]
        assets: Option<PathBuf>,
        /// Render the preview with this map instead of the built-in one
        /// (rows of g p d t c s o).
        #[arg(long)]
        map: Option<PathBuf>,
    },
    /// Dump the transition masks as debug images.
    Masks {
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        civ3: Option<PathBuf>,
        #[arg(long)]
        procedural: bool,
    },
    /// Put back the original art saved by the first `build --install`.
    Restore {
        #[arg(long)]
        assets: Option<PathBuf>,
    },
    /// Render a test map from any directory of sheets (e.g. the Civ3 ones
    /// in assets/gen/terrain/sheets) for side-by-side comparison.
    Preview {
        sheets: PathBuf,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        map: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Init { dir, name, style, placeholder } => init(&dir, name, &style, placeholder),
        Cmd::Generate { dir, only, force, dry_run } => generate(&dir, &only, force, dry_run),
        Cmd::Build { dir, out, civ3, install, assets, map } => build(&dir, out, civ3, install, assets, map),
        Cmd::Masks { out, civ3, procedural } => dump_masks(&out, civ3, procedural),
        Cmd::Restore { assets } => restore(assets),
        Cmd::Preview { sheets, out, map } => {
            let r = preview::render(&load_map(map.as_deref())?, &sheets)?;
            r.image.save(&out)?;
            println!("wrote {} (seam ratio {:.2})", out.display(), r.seam_ratio);
            Ok(())
        }
    }
}

/// Nearest ancestor of the working directory holding the game repo
/// (`assets/` or `civ3/` next to a `Cargo.toml`).
fn repo_root() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    cwd.ancestors()
        .find(|d| d.join("Cargo.toml").exists() && (d.join("civ3").is_dir() || d.join("assets").is_dir()) && d.join("src").join("blend.rs").exists())
        .map(Path::to_path_buf)
}

fn civ3_dir(arg: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(d) = arg {
        return Ok(d);
    }
    if let Ok(gog) = std::env::var("CIV3_GOG") {
        return Ok(PathBuf::from(gog).join("Art").join("Terrain"));
    }
    let root = repo_root().context("can't find the open-4x repo; pass --civ3")?;
    Ok(root.join("civ3/civ3-gog/app/Art/Terrain"))
}

fn load_map(path: Option<&Path>) -> Result<preview::TestMap> {
    match path {
        Some(p) => preview::TestMap::parse(&std::fs::read_to_string(p).with_context(|| format!("reading {}", p.display()))?),
        None => preview::TestMap::parse(preview::DEFAULT_MAP),
    }
}

fn init(dir: &Path, name: Option<String>, style: &str, placeholder: bool) -> Result<()> {
    if dir.join("pack.json").exists() {
        bail!("{} already has a pack.json", dir.display());
    }
    let name = name.unwrap_or_else(|| dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "terrain".into()));
    let pack = Pack::new(&name, style);
    pack.save(dir)?;
    println!("wrote {}", dir.join("pack.json").display());
    if placeholder {
        for (i, n) in pack::all_terrain_names().into_iter().enumerate() {
            let path = pack.texture_path(dir, n);
            material::placeholder(pack::placeholder_color(n), 17 + i as u32, 512).save(&path)?;
            println!("wrote placeholder {}", path.display());
        }
    }
    Ok(())
}

fn generate(dir: &Path, only: &[String], force: bool, dry_run: bool) -> Result<()> {
    let pack = Pack::load(dir)?;
    if !dry_run {
        fal::api_key()?;
    }
    for (name, t) in &pack.terrains {
        if !only.is_empty() && !only.contains(name) {
            continue;
        }
        let path = pack.texture_path(dir, name);
        if path.exists() && !force {
            println!("{name}: {} exists (use --force to regenerate)", path.display());
            continue;
        }
        let prompt = pack.full_prompt(name);
        println!("{name}: {prompt}");
        if dry_run {
            continue;
        }
        let bytes = fal::generate(&fal::Request {
            model: &pack.model,
            prompt: &prompt,
            image_size: &pack.image_size,
            seed: t.seed.or(Some(pack.seed as u64)),
        })
        .with_context(|| format!("generating {name}"))?;
        // Re-encode so the file really is the PNG its name says.
        let img = image::load_from_memory(&bytes).with_context(|| format!("decoding {name} from fal.ai"))?;
        img.save(&path)?;
        println!("  -> {} ({}x{})", path.display(), img.width(), img.height());
    }
    Ok(())
}

/// Civ3 sheet + digit whose pure cell defines each terrain's reference colour.
fn civ3_reference(k: Kind) -> (&'static str, usize) {
    match k {
        Kind::Grassland => ("xggc", 1),
        Kind::Plains => ("xpgc", 0),
        Kind::Desert => ("xdgc", 0),
        Kind::Tundra => ("xtgc", 0),
        Kind::Coast => ("wCSO", 0),
        Kind::Sea => ("wCSO", 1),
        Kind::Ocean => ("wCSO", 2),
    }
}

fn read_civ3_sheets(dir: &Path) -> Result<HashMap<&'static str, pcx::Rgba>> {
    let mut out = HashMap::new();
    for s in &SHEETS {
        let path = dir.join(format!("{}.pcx", s.stem));
        let img = pcx::read(&path).with_context(|| format!("Civ3 sheet missing; pass --civ3 or set masks to \"procedural\""))?;
        if (img.w, img.h) != (geom::SHEET_W, geom::SHEET_H) {
            bail!("{}: expected {}x{}, got {}x{}", path.display(), geom::SHEET_W, geom::SHEET_H, img.w, img.h);
        }
        out.insert(s.stem, img);
    }
    Ok(out)
}

fn make_masks(source: MaskSource, civ3: Option<&HashMap<&'static str, pcx::Rgba>>, seed: u32, shore_px: f32) -> Vec<SheetMasks> {
    match source {
        MaskSource::Civ3 => {
            let civ3 = civ3.expect("civ3 sheets loaded for civ3 masks");
            let mut m: Vec<SheetMasks> = SHEETS.iter().map(|s| masks::extract_civ3(s, &civ3[s.stem], shore_px)).collect();
            masks::harmonize(&mut m);
            m
        }
        MaskSource::Procedural => {
            let pp = masks::ProcParams { seed, amp: 0.45, shore: 0.03 * shore_px };
            SHEETS.iter().map(|s| masks::procedural(s, &pp)).collect()
        }
    }
}

fn dump_masks(out: &Path, civ3: Option<PathBuf>, procedural: bool) -> Result<()> {
    std::fs::create_dir_all(out)?;
    let (source, sheets) = if procedural {
        (MaskSource::Procedural, None)
    } else {
        (MaskSource::Civ3, Some(read_civ3_sheets(&civ3_dir(civ3)?)?))
    };
    let m = make_masks(source, sheets.as_ref(), 1, 2.0);
    for (s, sm) in SHEETS.iter().zip(&m) {
        let path = out.join(format!("{}.png", s.stem));
        masks::visualize(s, sm).save(&path)?;
        println!("wrote {}", path.display());
    }
    Ok(())
}

fn assets_dir(arg: Option<PathBuf>) -> Result<PathBuf> {
    match arg {
        Some(a) => Ok(a),
        None => Ok(repo_root().context("can't find the open-4x repo; pass --assets")?.join("assets/gen/terrain")),
    }
}

fn restore(assets: Option<PathBuf>) -> Result<()> {
    let assets = assets_dir(assets)?;
    let backup = assets.join(".civ3-backup");
    if !backup.is_dir() {
        bail!("no backup in {}; `python3 tools/prep_assets.py terrain` regenerates the originals", backup.display());
    }
    let mut n = 0;
    for sub in ["", "sheets"] {
        for entry in std::fs::read_dir(backup.join(sub))? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                std::fs::copy(entry.path(), assets.join(sub).join(entry.file_name()))?;
                n += 1;
            }
        }
    }
    std::fs::remove_dir_all(&backup)?;
    println!("restored {n} files into {}", assets.display());
    Ok(())
}

fn build(dir: &Path, out: Option<PathBuf>, civ3: Option<PathBuf>, install: bool, assets: Option<PathBuf>, map: Option<PathBuf>) -> Result<()> {
    let pack = Pack::load(dir)?;
    let out = out.unwrap_or_else(|| dir.join("out"));
    let sheets_out = out.join("terrain").join("sheets");
    std::fs::create_dir_all(&sheets_out)?;
    std::fs::create_dir_all(out.join("debug").join("masks"))?;

    let any_color_match = pack.color_match || pack.terrains.values().any(|t| t.color_match == Some(true));
    let civ3 = if pack.masks == MaskSource::Civ3 || any_color_match { Some(read_civ3_sheets(&civ3_dir(civ3)?)?) } else { None };

    // Materials -> periodic tiles.
    let mut tiles: HashMap<Kind, Vec<Vec<[f32; 3]>>> = HashMap::new();
    let mut shore = None;
    for (name, t) in &pack.terrains {
        let path = pack.texture_path(dir, name);
        if !path.exists() {
            bail!("{name}: material {} is missing (run `terrain-builder generate {}`)", path.display(), dir.display());
        }
        let mut m = Material::load(&path, t.tiles_across.unwrap_or(pack.tiles_across), pack.flatten)?;
        let kind = Kind::from_name(name);
        if let (Some(k), true, Some(civ3)) = (kind, t.color_match.unwrap_or(pack.color_match), civ3.as_ref()) {
            let (stem, d) = civ3_reference(k);
            let (mean, std) = masks::pure_stats(&civ3[stem], d);
            m.color_match(mean, std);
        }
        let variants: Vec<Vec<[f32; 3]>> = m.origins(3).into_iter().map(|o| m.periodic_tile(o)).collect();
        match kind {
            Some(k) => {
                tiles.insert(k, variants);
            }
            None => shore = Some(variants.into_iter().next().unwrap()),
        }
        println!("material {name}: mean rgb {:?}", m.mean.map(|v| v.round() as i32));
    }
    let pal = Palette {
        heights: tiles.iter().map(|(k, v)| (*k, material::heights(&v[0]))).collect(),
        shore: shore.unwrap_or_else(|| {
            // No beach material: lightened desert.
            tiles[&Kind::Desert][0].iter().map(|c| c.map(|v| (v * 1.05 + 10.0).min(255.0))).collect()
        }),
        tiles: tiles.iter().map(|(k, v)| (*k, v[0].clone())).collect(),
    };

    let mask_set = make_masks(pack.masks, civ3.as_ref(), pack.seed, pack.shore_width);
    let cp = ComposeParams { blend: pack.blend, shoreline_darken: pack.shoreline_darken, seed: pack.seed };
    let mut built: Vec<(&SheetSpec, &SheetMasks, image::RgbaImage)> = Vec::new();
    for (spec, m) in SHEETS.iter().zip(&mask_set) {
        let img = compose::compose_sheet(spec, m, &pal, &cp);
        img.save(sheets_out.join(format!("{}.png", spec.stem)))?;
        masks::visualize(spec, m).save(out.join("debug").join("masks").join(format!("{}.png", spec.stem)))?;
        built.push((spec, m, img));
    }
    println!("wrote {} sheets to {}", built.len(), sheets_out.display());

    // Single tiles for the city screen (render::base_name). Ice keeps its
    // Civ3 art.
    let mut tile_files = Vec::new();
    for k in Kind::ALL {
        for (i, t) in tiles[&k].iter().enumerate() {
            let name = format!("{}_{i}.png", k.name());
            compose::pure_tile(t).save(out.join("terrain").join(&name))?;
            tile_files.push(name);
        }
    }

    let refs: Vec<(&SheetSpec, &SheetMasks, &image::RgbaImage)> = built.iter().map(|(s, m, i)| (*s, *m, i)).collect();
    let mut report = validate::check(&refs, &pal);
    let test_map = load_map(map.as_deref())?;
    let r = preview::render(&test_map, &sheets_out)?;
    r.image.save(out.join("preview.png"))?;
    report.preview_seam_ratio = Some(r.seam_ratio);
    std::fs::write(out.join("report.json"), serde_json::to_string_pretty(&report)? + "\n")?;
    println!(
        "validate: {}/{} vertex cores pure (worst colour error {:.1}), edge seam ratio {:.2}, preview seam ratio {:.2}",
        report.vertices_checked - report.vertex_failures,
        report.vertices_checked,
        report.worst_vertex_color,
        report.edge_seam_ratio,
        r.seam_ratio
    );
    println!("preview: {}", out.join("preview.png").display());
    if report.vertex_failures > 0 || report.worst_vertex_color > 2.0 {
        bail!("vertex invariant violated; not installing");
    }

    if install {
        let assets = assets_dir(assets)?;
        install_into(&assets, &out.join("terrain"), &tile_files)?;
    }
    Ok(())
}

/// Copy sheets and tiles over the game's generated assets. The first
/// install keeps the Civ3 originals in `.civ3-backup/`;
/// `python3 tools/prep_assets.py terrain` also restores them.
fn install_into(assets: &Path, built: &Path, tile_files: &[String]) -> Result<()> {
    let manifest_path = assets.join("manifest.json");
    if !manifest_path.exists() {
        bail!("{} not found; run `python3 tools/prep_assets.py` in the repo first", manifest_path.display());
    }
    let backup = assets.join(".civ3-backup");
    let mut files: Vec<PathBuf> = SHEETS.iter().map(|s| PathBuf::from("sheets").join(format!("{}.png", s.stem))).collect();
    files.extend(tile_files.iter().map(PathBuf::from));
    if !backup.exists() {
        for f in &files {
            let src = assets.join(f);
            if src.exists() {
                std::fs::create_dir_all(backup.join(f).parent().unwrap())?;
                std::fs::copy(&src, backup.join(f))?;
            }
        }
        println!("backed up the original art to {}", backup.display());
    }
    for f in &files {
        std::fs::create_dir_all(assets.join(f).parent().unwrap())?;
        std::fs::copy(built.join(f), assets.join(f)).with_context(|| format!("installing {}", f.display()))?;
    }
    let mut manifest: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&std::fs::read_to_string(&manifest_path)?)?;
    for f in tile_files {
        let name = f.trim_end_matches(".png");
        manifest.insert(name.into(), serde_json::json!({"file": f, "size": [128, 64], "anchor": [64, 32]}));
    }
    std::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)?;
    println!("installed {} files into {}", files.len(), assets.display());
    Ok(())
}

