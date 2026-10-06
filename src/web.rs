//! Browser-only startup and cached-file access.
//!
//! The desktop build keeps its normal filesystem behavior. WASM cannot run
//! the asset conversion subprocess or inspect the Civ3 install, so it reads
//! the already-converted cache from the web server and resolves source paths
//! through the cache's request manifest.

use std::path::Path;
#[cfg(target_arch = "wasm32")]
use std::path::PathBuf;
#[cfg(target_arch = "wasm32")]
use std::sync::OnceLock;

#[cfg(target_arch = "wasm32")]
use serde_json::Value;

#[cfg(target_arch = "wasm32")]
use web_sys::XmlHttpRequest;

#[cfg(target_arch = "wasm32")]
const PEDIA: &str = include_str!("../civ3/civ3-gog/app/Conquests/Text/PediaIcons.txt");
#[cfg(target_arch = "wasm32")]
const STOCK_BIQ: &[u8] = include_bytes!("../civ3/civ3-gog/app/Conquests/conquests.biq");

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PathKind {
    File,
    Dir,
}

#[cfg(target_arch = "wasm32")]
struct Catalog {
    root: PathBuf,
    search: Vec<PathBuf>,
    paths: Vec<(String, PathKind)>,
}

#[cfg(target_arch = "wasm32")]
static CATALOG: OnceLock<Catalog> = OnceLock::new();

#[cfg(target_arch = "wasm32")]
fn norm(path: &str) -> String {
    let mut out = Vec::new();
    for part in path.replace('\\', "/").split('/') {
        match part {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            part => out.push(part.to_ascii_lowercase()),
        }
    }
    out.join("/")
}

#[cfg(target_arch = "wasm32")]
fn request() -> &'static Value {
    static REQUEST: OnceLock<Value> = OnceLock::new();
    REQUEST.get_or_init(|| {
        #[cfg(target_arch = "wasm32")]
        {
            serde_json::from_str(&read_text("assets/cache/request.json").expect("assets/cache/request.json"))
                .expect("assets/cache/request.json parses")
        }
    })
}

#[cfg(target_arch = "wasm32")]
fn catalog() -> &'static Catalog {
    CATALOG.get_or_init(|| {
        let request = request();
        let root = PathBuf::from(request["root"].as_str().unwrap_or_default());
        let search = request["search"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| p.as_str())
            .map(PathBuf::from)
            .collect::<Vec<_>>();
        let root_text = request["root"].as_str().unwrap_or_default();
        let mut paths = vec![
            (norm(&format!("{root_text}/Conquests/Text/PediaIcons.txt")), PathKind::File),
            (norm(&format!("{root_text}/Text/PediaIcons.txt")), PathKind::File),
        ];
        for color in request["team_colors"].as_array().into_iter().flatten().filter_map(Value::as_u64) {
            paths.push((norm(&format!("{root_text}/Art/Units/Palettes/ntp{color:02}.pcx")), PathKind::File));
        }
        for unit in request["units"].as_array().into_iter().flatten() {
            for dir in unit["dirs"].as_array().into_iter().flatten().filter_map(Value::as_str) {
                paths.push((norm(dir), PathKind::Dir));
            }
        }
        for leader in request["leaders"].as_array().into_iter().flatten() {
            for key in ["forward", "reverse"] {
                if let Some(path) = leader[key].as_str() {
                    paths.push((norm(path), PathKind::File));
                }
            }
        }
        for pic in request["wonders"].as_array().into_iter().flatten().chain(request["techs"].as_array().into_iter().flatten()) {
            if let Some(path) = pic["path"].as_str() {
                paths.push((norm(path), PathKind::File));
            }
        }
        paths.dedup_by(|a, b| a.0 == b.0);
        Catalog { root, search, paths }
    })
}

#[cfg(target_arch = "wasm32")]
pub fn stock_biq() -> &'static [u8] {
    STOCK_BIQ
}

#[cfg(target_arch = "wasm32")]
pub fn install() -> crate::install::Install {
    let catalog = catalog();
    crate::install::Install { root: catalog.root.clone(), search: catalog.search.clone() }
}

#[cfg(target_arch = "wasm32")]
pub fn resolve(root: &Path, rel: &str) -> Option<PathBuf> {
    let root = norm(&root.to_string_lossy());
    let rel = norm(rel);
    catalog().paths.iter().find_map(|(path, _)| {
        let path_rel = path.strip_prefix(&root)?.strip_prefix('/')?;
        (path_rel == rel).then(|| PathBuf::from(path))
    })
}

#[cfg(target_arch = "wasm32")]
pub fn kind(path: &Path) -> Option<PathKind> {
    let path = norm(&path.to_string_lossy());
    catalog().paths.iter().find_map(|(candidate, kind)| (*candidate == path).then_some(*kind))
}

fn cache_url(path: &Path) -> Option<String> {
    let path = path.to_string_lossy().replace('\\', "/");
    path.strip_prefix("assets/cache/").map(|rel| format!("assets/cache/{rel}"))
}

#[cfg(target_arch = "wasm32")]
fn xhr(url: &str, binary: bool) -> std::io::Result<Vec<u8>> {
    let xhr = XmlHttpRequest::new().map_err(|e| std::io::Error::other(format!("{e:?}")))?;
    xhr.open_with_async("GET", url, false).map_err(|e| std::io::Error::other(format!("{e:?}")))?;
    xhr.send().map_err(|e| std::io::Error::other(format!("{e:?}")))?;
    if xhr.status().map_err(|e| std::io::Error::other(format!("{e:?}")))? != 200 {
        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, url));
    }
    if !binary {
        return Ok(xhr
            .response_text()
            .map_err(|e| std::io::Error::other(format!("{e:?}")))?
            .unwrap_or_default()
            .into_bytes());
    }
    let response = xhr.response().map_err(|e| std::io::Error::other(format!("{e:?}")))?;
    let array = js_sys::Uint8Array::new(&response);
    Ok(array.to_vec())
}

#[cfg(not(target_arch = "wasm32"))]
fn xhr(url: &str, _binary: bool) -> std::io::Result<Vec<u8>> {
    std::fs::read(url)
}

pub fn read_text(path: impl AsRef<Path>) -> std::io::Result<String> {
    let path = path.as_ref();
    #[cfg(target_arch = "wasm32")]
    {
        if path.to_string_lossy().replace('\\', "/").to_ascii_lowercase().ends_with("text/pediaicons.txt") {
            return Ok(PEDIA.to_string());
        }
    }
    let url = cache_url(path).unwrap_or_else(|| path.to_string_lossy().into_owned());
    Ok(String::from_utf8_lossy(&xhr(&url, false)?).into_owned())
}

pub fn read_bytes(path: impl AsRef<Path>) -> std::io::Result<Vec<u8>> {
    let path = path.as_ref();
    #[cfg(target_arch = "wasm32")]
    {
        let normalized = path.to_string_lossy().replace('\\', "/").to_ascii_lowercase();
        if normalized.ends_with("text/pediaicons.txt") {
            return Ok(PEDIA.as_bytes().to_vec());
        }
        if normalized.contains("art/units/palettes/") {
            return Ok(vec![128; 768]);
        }
    }
    let url = cache_url(path).unwrap_or_else(|| path.to_string_lossy().into_owned());
    xhr(&url, true)
}

pub fn cache_exists(path: impl AsRef<Path>) -> bool {
    let Some(url) = cache_url(path.as_ref()) else { return false };
    #[cfg(target_arch = "wasm32")]
    {
        let Ok(xhr) = XmlHttpRequest::new() else { return false };
        if xhr.open_with_async("HEAD", &url, false).is_err() || xhr.send().is_err() {
            return false;
        }
        let content_type = xhr
            .get_response_header("content-type")
            .ok()
            .flatten()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let content_length = xhr
            .get_response_header("content-length")
            .ok()
            .flatten()
            .and_then(|length| length.parse::<u64>().ok())
            .unwrap_or(0);
        xhr.status().is_ok_and(|status| status == 200)
            && content_length > 0
            && !content_type.starts_with("text/html")
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Path::new(&url).is_file()
    }
}

#[cfg(target_arch = "wasm32")]
pub fn show_error(message: &str) {
    if let Some(document) = web_sys::window().and_then(|window| window.document()) {
        document.set_title(&format!("Civ3 Clone error: {message}"));
    }
}

#[cfg(target_arch = "wasm32")]
pub fn show_progress(message: &str) {
    if let Some(document) = web_sys::window().and_then(|window| window.document()) {
        document.set_title(&format!("Civ3 Clone: {message}"));
    }
}

#[cfg(target_arch = "wasm32")]
pub fn show_ready() {
    if let Some(document) = web_sys::window().and_then(|window| window.document()) {
        document.set_title("Civ3 Clone");
    }
}
