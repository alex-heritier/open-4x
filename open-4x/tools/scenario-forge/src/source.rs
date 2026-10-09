//! Pinned source data: download, checksum verification, parsing.
//!
//! * Natural Earth (public domain, https://www.naturalearthdata.com/about/terms-of-use/):
//!   coastlines, lakes, country shapes, and rivers. Who governed what on 1 January 1876 lives
//!   entirely in `data/`.
//! * ETOPO5 (NOAA, public domain): 5-arc-minute land elevation and sea depth, which decide
//!   hills, mountains, and how deep the water is.
//! * Köppen–Geiger climate classes (Kottek et al. 2006, 0.5°, free with attribution):
//!   which land is desert, steppe, jungle, forest, or tundra.
use crate::geo::{Polygon, Ring};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Download {
    pub file: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
}

pub const COUNTRIES: Download = Download {
    file: "ne_50m_admin_0_countries.geojson",
    url: "https://raw.githubusercontent.com/nvkelso/natural-earth-vector/v5.1.2/geojson/ne_50m_admin_0_countries.geojson",
    sha256: "3e458fc036ad0a66411f2c1e6cac49c5d7bfb81cb1123bc513b22511a2b7fdeb",
};
pub const LAKES: Download = Download {
    file: "ne_50m_lakes.geojson",
    url: "https://raw.githubusercontent.com/nvkelso/natural-earth-vector/v5.1.2/geojson/ne_50m_lakes.geojson",
    sha256: "d350b75978b26fe839b797c2c529b2fb8f47fb3983c03f4964e36d5df9378a52",
};
pub const RIVERS: Download = Download {
    file: "ne_50m_rivers_lake_centerlines.geojson",
    url: "https://raw.githubusercontent.com/nvkelso/natural-earth-vector/v5.1.2/geojson/ne_50m_rivers_lake_centerlines.geojson",
    sha256: "f286e0ce978fde999ca2d7a78c764be08542e19b63cded52b05c12d5173ccc51",
};
/// ETOPO5 as distributed for DOS: 2160 rows (90°N to 90°S) of 4320 little-endian `i16`
/// (0°E eastward), metres.
pub const ETOPO5: Download = Download {
    file: "ETOPO5.DOS",
    url: "https://www.ngdc.noaa.gov/mgg/global/relief/ETOPO5/TOPO/ETOPO5/ETOPO5.DOS",
    sha256: "bcb4ed5585e07ffe4244b19cb0d645c0ceb10877a22a71b21370efa19e6cb792",
};
pub const KOEPPEN_ZIP: Download = Download {
    file: "Koeppen-Geiger-ASCII.zip",
    url: "https://koeppen-geiger.vu-wien.ac.at/data/Koeppen-Geiger-ASCII.zip",
    sha256: "4b85ca339b9aba4c6507ed0db9efad99334d56bd7010f417672a0c977b4b2f75",
};
/// The one file inside [`KOEPPEN_ZIP`] and its checksum.
pub const KOEPPEN_MEMBER: Download = Download {
    file: "Koeppen-Geiger-ASCII.txt",
    url: "",
    sha256: "9b00fa69a3168cb5baaa5beb0606eb57c57d05dbd63534361188c64a372f3d46",
};

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Returns the verified bytes of a source file, downloading it into `cache` if needed.
pub fn fetch(cache: &Path, download: &Download) -> Result<Vec<u8>, String> {
    let path: PathBuf = cache.join(download.file);
    if let Ok(bytes) = std::fs::read(&path) {
        if digest(&bytes) == download.sha256 {
            return Ok(bytes);
        }
        eprintln!(
            "cached {} failed its checksum; downloading again",
            download.file
        );
    }
    std::fs::create_dir_all(cache)
        .map_err(|error| format!("create {}: {error}", cache.display()))?;
    let url = download.url;
    eprintln!("downloading {url}");
    let output = Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--max-time",
            "300",
            url,
        ])
        .output()
        .map_err(|error| format!("run curl (needed once to fetch geometry): {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "curl failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let actual = digest(&output.stdout);
    if actual != download.sha256 {
        return Err(format!(
            "{} has checksum {actual}, expected {}",
            download.file, download.sha256
        ));
    }
    std::fs::write(&path, &output.stdout)
        .map_err(|error| format!("write {}: {error}", path.display()))?;
    Ok(output.stdout)
}

fn ring(value: &Value) -> Result<Ring, String> {
    value
        .as_array()
        .ok_or("ring is not an array")?
        .iter()
        .map(|point| {
            let pair = point.as_array().ok_or("point is not an array")?;
            Ok((
                pair.first()
                    .and_then(Value::as_f64)
                    .ok_or("bad longitude")?,
                pair.get(1).and_then(Value::as_f64).ok_or("bad latitude")?,
            ))
        })
        .collect()
}

fn polygons(geometry: &Value) -> Result<Vec<Polygon>, String> {
    let coordinates = &geometry["coordinates"];
    let rings = |value: &Value| -> Result<Polygon, String> {
        value
            .as_array()
            .ok_or("polygon is not an array")?
            .iter()
            .map(ring)
            .collect()
    };
    match geometry["type"].as_str() {
        Some("Polygon") => Ok(vec![rings(coordinates)?]),
        Some("MultiPolygon") => coordinates
            .as_array()
            .ok_or("multipolygon is not an array")?
            .iter()
            .map(rings)
            .collect(),
        other => Err(format!("unsupported geometry {other:?}")),
    }
}

/// Country polygons keyed by `ADM0_A3` code.
pub fn countries(bytes: &[u8]) -> Result<BTreeMap<String, Vec<Polygon>>, String> {
    let document: Value = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    let mut out: BTreeMap<String, Vec<Polygon>> = BTreeMap::new();
    for feature in document["features"].as_array().ok_or("no features")? {
        let code = feature["properties"]["ADM0_A3"]
            .as_str()
            .ok_or("feature without ADM0_A3")?;
        out.entry(code.to_string())
            .or_default()
            .extend(polygons(&feature["geometry"])?);
    }
    Ok(out)
}

pub fn lakes(bytes: &[u8]) -> Result<Vec<Polygon>, String> {
    let document: Value = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    let mut out = Vec::new();
    for feature in document["features"].as_array().ok_or("no features")? {
        out.extend(polygons(&feature["geometry"])?);
    }
    Ok(out)
}

/// The Köppen–Geiger table: unzipped on first use with the system `unzip`.
pub fn fetch_koeppen(cache: &Path) -> Result<Vec<u8>, String> {
    let member = cache.join(KOEPPEN_MEMBER.file);
    if let Ok(bytes) = std::fs::read(&member)
        && digest(&bytes) == KOEPPEN_MEMBER.sha256
    {
        return Ok(bytes);
    }
    fetch(cache, &KOEPPEN_ZIP)?;
    let output = Command::new("unzip")
        .arg("-p")
        .arg(cache.join(KOEPPEN_ZIP.file))
        .arg(KOEPPEN_MEMBER.file)
        .output()
        .map_err(|error| format!("run unzip (needed once for the climate table): {error}"))?;
    if !output.status.success() || digest(&output.stdout) != KOEPPEN_MEMBER.sha256 {
        return Err(format!(
            "unzipping {} did not give the pinned table",
            KOEPPEN_ZIP.file
        ));
    }
    std::fs::write(&member, &output.stdout)
        .map_err(|error| format!("write {}: {error}", member.display()))?;
    Ok(output.stdout)
}

/// ETOPO5 elevation (metres, negative at sea) on its 5-arc-minute grid.
pub struct Elevation {
    samples: Vec<i16>,
}
const ETOPO_COLS: usize = 4320;
const ETOPO_ROWS: usize = 2160;
const ETOPO_PER_DEGREE: f64 = 12.0;

/// Mean, highest, and lowest elevation of the samples in a box.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub mean: f64,
    pub max: f64,
    pub min: f64,
}

impl Elevation {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() != ETOPO_COLS * ETOPO_ROWS * 2 {
            return Err(format!(
                "ETOPO5 has {} bytes, expected {}",
                bytes.len(),
                ETOPO_COLS * ETOPO_ROWS * 2
            ));
        }
        Ok(Self {
            samples: bytes
                .chunks_exact(2)
                .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
                .collect(),
        })
    }
    fn sample(&self, row: usize, col: usize) -> f64 {
        f64::from(self.samples[row.min(ETOPO_ROWS - 1) * ETOPO_COLS + col % ETOPO_COLS])
    }
    /// Statistics of the samples whose centres lie in the box (degrees, longitudes -180..180
    /// without wrapping the antimeridian); a box too small for any sample takes the nearest one.
    pub fn stats(&self, west: f64, south: f64, east: f64, north: f64) -> Stats {
        // columns count east from 0°E, rows south from 90°N; sample centres are at +0.5
        let (w, e) = (
            west.rem_euclid(360.0),
            west.rem_euclid(360.0) + (east - west),
        );
        let first_col = (w * ETOPO_PER_DEGREE - 0.5).ceil().max(0.0) as i64;
        let last_col = (e * ETOPO_PER_DEGREE - 0.5).floor() as i64;
        let first_row = ((90.0 - north) * ETOPO_PER_DEGREE - 0.5).ceil().max(0.0) as i64;
        let last_row =
            (((90.0 - south) * ETOPO_PER_DEGREE - 0.5).floor() as i64).min(ETOPO_ROWS as i64 - 1);
        let (mut sum, mut count) = (0.0, 0usize);
        let (mut max, mut min) = (f64::MIN, f64::MAX);
        for row in first_row..=last_row {
            for col in first_col..=last_col {
                let value = self.sample(row as usize, col.rem_euclid(ETOPO_COLS as i64) as usize);
                sum += value;
                count += 1;
                max = max.max(value);
                min = min.min(value);
            }
        }
        if count == 0 {
            let row = (((90.0 - (south + north) / 2.0) * ETOPO_PER_DEGREE).floor() as usize)
                .min(ETOPO_ROWS - 1);
            let col = ((((west + east) / 2.0).rem_euclid(360.0) * ETOPO_PER_DEGREE).floor()
                as usize)
                % ETOPO_COLS;
            let value = self.sample(row, col);
            return Stats {
                mean: value,
                max: value,
                min: value,
            };
        }
        Stats {
            mean: sum / count as f64,
            max,
            min,
        }
    }
}

/// Köppen–Geiger classes on a 0.5° grid (360 rows from 90°N, 720 columns from 180°W).
pub struct Climate {
    cells: Vec<[u8; 3]>,
}
const CLIMATE_COLS: usize = 720;
const CLIMATE_ROWS: usize = 360;

impl Climate {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let text = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
        let mut cells = vec![[b' '; 3]; CLIMATE_COLS * CLIMATE_ROWS];
        for line in text.lines().skip(1) {
            let mut fields = line.split_whitespace();
            let (Some(lat), Some(lon), Some(class)) = (fields.next(), fields.next(), fields.next())
            else {
                continue; // ocean cells carry no class
            };
            let lat: f64 = lat.parse().map_err(|_| format!("bad latitude {lat:?}"))?;
            let lon: f64 = lon.parse().map_err(|_| format!("bad longitude {lon:?}"))?;
            let (row, col) = (((90.0 - lat) * 2.0).floor(), ((lon + 180.0) * 2.0).floor());
            if !(0.0..CLIMATE_ROWS as f64).contains(&row)
                || !(0.0..CLIMATE_COLS as f64).contains(&col)
                || class.len() > 3
            {
                return Err(format!("bad climate cell {line:?}"));
            }
            let mut code = [b' '; 3];
            code[..class.len()].copy_from_slice(class.as_bytes());
            cells[row as usize * CLIMATE_COLS + col as usize] = code;
        }
        Ok(Self { cells })
    }
    fn class_at(&self, row: i32, col: i32) -> Option<[u8; 3]> {
        if !(0..CLIMATE_ROWS as i32).contains(&row) {
            return None;
        }
        let code =
            self.cells[row as usize * CLIMATE_COLS + col.rem_euclid(CLIMATE_COLS as i32) as usize];
        (code[0] != b' ').then_some(code)
    }
    /// Class at a point, or at the nearest classified cell within a few cells (the table has
    /// none for coastal cells that are mostly sea).
    pub fn class(&self, lon: f64, lat: f64) -> Option<Koeppen> {
        let (row, col) = (
            ((90.0 - lat) * 2.0).floor() as i32,
            ((lon + 180.0) * 2.0).floor() as i32,
        );
        let mut best: Option<(i32, [u8; 3])> = None;
        for radius in 0..=6i32 {
            for dr in -radius..=radius {
                for dc in -radius..=radius {
                    if dr.abs().max(dc.abs()) != radius {
                        continue;
                    }
                    if let Some(code) = self.class_at(row + dr, col + dc) {
                        let d = dr * dr + dc * dc;
                        if best.is_none_or(|(b, _)| d < b) {
                            best = Some((d, code));
                        }
                    }
                }
            }
            if best.is_some() {
                break;
            }
        }
        best.and_then(|(_, code)| Koeppen::parse(code))
    }
}

/// The Köppen climate group, with the letters that matter to the map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Koeppen {
    /// A tropical, B dry, C temperate, D continental, E polar.
    pub group: u8,
    /// Second letter: f, m, w, s (precipitation), W, S (desert, steppe), T, F (tundra, ice).
    pub rain: u8,
    /// Third letter: temperature (a hot summer to d very cold winter, h hot, k cold), or space.
    pub heat: u8,
}
impl Koeppen {
    pub fn parse(code: [u8; 3]) -> Option<Self> {
        b"ABCDE".contains(&code[0]).then_some(Self {
            group: code[0],
            rain: code[1],
            heat: code[2],
        })
    }
}

/// A river from Natural Earth: all its branches as `(lon, lat)` polylines, and its scale rank
/// (1 the biggest).
pub struct River {
    pub rank: u32,
    pub lines: Vec<Vec<(f64, f64)>>,
}

pub fn rivers(bytes: &[u8]) -> Result<Vec<River>, String> {
    let document: Value = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    let mut out = Vec::new();
    for feature in document["features"].as_array().ok_or("no features")? {
        if feature["properties"]["featurecla"].as_str() != Some("River") {
            continue; // lake centrelines run through water tiles
        }
        let geometry = &feature["geometry"];
        let lines: Vec<&Value> = match geometry["type"].as_str() {
            Some("LineString") => vec![&geometry["coordinates"]],
            Some("MultiLineString") => geometry["coordinates"]
                .as_array()
                .ok_or("river is not an array")?
                .iter()
                .collect(),
            other => return Err(format!("unsupported river geometry {other:?}")),
        };
        out.push(River {
            rank: feature["properties"]["scalerank"].as_u64().unwrap_or(6) as u32,
            lines: lines.into_iter().map(ring).collect::<Result<_, _>>()?,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn climate_cells_resolve_to_their_class_or_the_nearest_one() {
        let table = "   Lat      Lon      Cls\n 51.25   -0.25      Cfb\n 51.25    0.25      Cfb\n  0.25   20.25       Af\n-30.25  130.25      BWh\n";
        let climate = Climate::parse(table.as_bytes()).unwrap();
        let at = |lon, lat| climate.class(lon, lat).map(|k| (k.group, k.rain, k.heat));
        assert_eq!(at(-0.1, 51.3), Some((b'C', b'f', b'b')));
        assert_eq!(at(20.3, 0.2), Some((b'A', b'f', b' ')));
        assert_eq!(at(130.1, -30.1), Some((b'B', b'W', b'h')));
        // A coastal cell with no class borrows the nearest one within a few cells.
        assert_eq!(at(1.0, 51.3), Some((b'C', b'f', b'b')));
        // Open ocean has none.
        assert_eq!(at(-150.0, 0.0), None);
        assert!(Climate::parse(b"Lat Lon Cls\n 99.0 0.0 Af\n").is_err());
    }

    #[test]
    fn elevation_statistics_cover_a_box_and_wrap_the_antimeridian_grid() {
        let mut bytes = vec![0u8; ETOPO_COLS * ETOPO_ROWS * 2];
        // A 1000 m peak and a 200 m deep hole near 87°E, 28°N.
        let sample = |lon: f64, lat: f64| {
            let (row, col) = (((90.0 - lat) * 12.0) as usize, (lon * 12.0) as usize);
            (row * ETOPO_COLS + col) * 2
        };
        let peak = sample(87.0, 28.0);
        bytes[peak..peak + 2].copy_from_slice(&1000i16.to_le_bytes());
        let hole = sample(87.5, 28.5);
        bytes[hole..hole + 2].copy_from_slice(&(-200i16).to_le_bytes());
        let elevation = Elevation::parse(&bytes).unwrap();
        let stats = elevation.stats(86.0, 27.0, 88.0, 29.0);
        assert_eq!((stats.max, stats.min), (1000.0, -200.0));
        assert!(stats.mean > 0.0 && stats.mean < 10.0, "{}", stats.mean);
        // A box too small to hold a sample centre still answers with the nearest sample.
        let tiny = elevation.stats(87.001, 28.001, 87.002, 28.002);
        assert_eq!(tiny.max, tiny.min);
        // Negative longitudes read the western hemisphere of the 0-360° grid.
        let west = sample(360.0 - 100.0, 40.0);
        bytes[west..west + 2].copy_from_slice(&500i16.to_le_bytes());
        let elevation = Elevation::parse(&bytes).unwrap();
        assert_eq!(elevation.stats(-100.5, 39.5, -99.5, 40.5).max, 500.0);
        assert!(Elevation::parse(&bytes[..100]).is_err());
    }

    #[test]
    fn rivers_keep_rivers_and_drop_lake_centrelines() {
        let json = br#"{"features":[
            {"properties":{"featurecla":"River","scalerank":2},"geometry":{"type":"MultiLineString","coordinates":[[[0,0],[1,1]],[[2,2],[3,3]]]}},
            {"properties":{"featurecla":"Lake Centerline","scalerank":3},"geometry":{"type":"MultiLineString","coordinates":[[[5,5],[6,6]]]}}
        ]}"#;
        let found = rivers(json).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].rank, found[0].lines.len()), (2, 2));
    }
}
