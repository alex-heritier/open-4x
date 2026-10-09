//! Web-Mercator projection, the tile lattice, and polygon rasterization onto a supersampled grid.
//!
//! The world uses the standard "web map" Mercator: longitude -180..180 maps linearly onto 512
//! plane units, and every unit is as wide as it is tall at the equator, so the full ±85.05°
//! world would be a 512 x 512 square. The map keeps rows `ROW0..ROW0 + ROWS` of that square:
//! from 84.0°N, which clears Greenland and the whole Arctic archipelago, down to 58.0°S, which
//! clears Cape Horn and drops Antarctica. Shapes are Mercator's: east-west distances are true
//! at the equator and stretch toward the poles.
//!
//! The *plane* is that 512 x 342 picture, with `(x, y)` growing east and south. The tiles are
//! laid on it the way Civ3 lays them: diamonds twice as wide as tall, rows alternately shifted
//! half a tile, so the world stands upright on screen. One tile has the area of one plane unit,
//! so the world keeps its 175,104 tiles. The simulation's square grid is that arrangement
//! turned 45°: tile `(u, v)` of the grid sits at native column `cx = u - v + LATTICE_COLUMNS`
//! and native row `cy = u + v - LATTICE_COLUMNS`, which is plane position `(cx, cy / 2)`.
use std::f64::consts::PI;

/// Plane width: 360° of longitude.
pub const COLS: usize = 512;
/// Rows of the full Web-Mercator square the map is cut from.
pub const FULL_ROWS: usize = 512;
/// First kept row of the square (84.0°N).
pub const ROW0: usize = 16;
/// Plane height: rows kept (down to 58.0°S).
pub const ROWS: usize = 342;
/// Tiles in each native row: the lattice is `2 * LATTICE_COLUMNS` half-tile columns wide.
pub const LATTICE_COLUMNS: i32 = (COLS / 2) as i32;
/// Native rows: each is half a plane unit tall.
pub const LATTICE_ROWS: i32 = (ROWS * 2) as i32;
/// Side of the square grid that holds the lattice.
pub const SIDE: usize = LATTICE_COLUMNS as usize + LATTICE_ROWS as usize / 2;
/// Samples per tile edge. Coastlines and borders are decided at this finer resolution.
pub const SUPER: usize = 4;
pub const GX: usize = COLS * SUPER;
pub const GY: usize = ROWS * SUPER;
pub const MAX_LAT: f64 = 85.051_128_779_806_6;

pub type Ring = Vec<(f64, f64)>; // (lon, lat)
/// Outer ring first, then holes.
pub type Polygon = Vec<Ring>;

/// 0 at the northern edge, 1 at the southern edge.
pub fn lat_to_unit(lat: f64) -> f64 {
    let phi = lat.clamp(-MAX_LAT, MAX_LAT).to_radians();
    (1.0 - phi.tan().asinh() / PI) / 2.0
}
pub fn unit_to_lat(unit: f64) -> f64 {
    (PI * (1.0 - 2.0 * unit)).sinh().atan().to_degrees()
}
pub fn lon_to_unit(lon: f64) -> f64 {
    (lon + 180.0) / 360.0
}
/// Whether grid cell `(x, y)` is a tile of the lattice rather than void around it.
pub fn in_lattice(x: i32, y: i32) -> bool {
    let (cx, cy) = (x - y + LATTICE_COLUMNS, x + y - LATTICE_COLUMNS);
    (0..2 * LATTICE_COLUMNS).contains(&cx) && (0..LATTICE_ROWS).contains(&cy)
}
/// Plane position `(x east, y south)` of a longitude/latitude, one unit per Mercator tile.
pub fn plane(lon: f64, lat: f64) -> (f64, f64) {
    (
        lon_to_unit(lon) * COLS as f64,
        lat_to_unit(lat) * FULL_ROWS as f64 - ROW0 as f64,
    )
}
/// Longitude/latitude of a plane position.
pub fn plane_lonlat(x: f64, y: f64) -> (f64, f64) {
    (
        x / COLS as f64 * 360.0 - 180.0,
        unit_to_lat((y + ROW0 as f64) / FULL_ROWS as f64),
    )
}
/// Fractional grid coordinates of a plane position (tile `(u, v)` covers `u..u+1` by `v..v+1`).
pub fn plane_to_grid(x: f64, y: f64) -> (f64, f64) {
    (
        x / 2.0 + y + 0.5,
        y - x / 2.0 + f64::from(LATTICE_COLUMNS) + 0.5,
    )
}
/// Plane position of fractional grid coordinates; the inverse of [`plane_to_grid`].
pub fn grid_to_plane(u: f64, v: f64) -> (f64, f64) {
    (
        u - v + f64::from(LATTICE_COLUMNS),
        (u + v - 1.0 - f64::from(LATTICE_COLUMNS)) / 2.0,
    )
}
/// Plane position of a tile's centre.
pub fn tile_plane(x: i32, y: i32) -> (f64, f64) {
    grid_to_plane(f64::from(x) + 0.5, f64::from(y) + 0.5)
}
/// The lattice tile whose diamond holds a longitude/latitude; the nearest tile when the point
/// lies in the ragged margin the half-tile rows leave at the edge of the plane.
pub fn tile_of(lon: f64, lat: f64) -> (i32, i32) {
    let (px, py) = plane(lon, lat);
    let (u, v) = plane_to_grid(px, py);
    let (x, y) = (u.floor() as i32, v.floor() as i32);
    let mut best: Option<(f64, (i32, i32))> = None;
    for dy in -2..=2 {
        for dx in -2..=2 {
            let (tx, ty) = (x + dx, y + dy);
            if !in_lattice(tx, ty) {
                continue;
            }
            let (cx, cy) = tile_plane(tx, ty);
            // Diamond metric: a tile is 2 wide and 1 tall in plane units.
            let distance = (cx - px).abs() / 2.0 + (cy - py).abs();
            if best.is_none_or(|(d, _)| distance < d) {
                best = Some((distance, (tx, ty)));
            }
        }
    }
    best.expect("a point on the plane is next to a lattice tile")
        .1
}
/// Longitude/latitude of a tile's centre.
pub fn tile_centre(x: i32, y: i32) -> (f64, f64) {
    let (px, py) = tile_plane(x, y);
    plane_lonlat(px, py)
}
/// West, south, east and north (degrees) of the box around a tile's diamond.
pub fn tile_bounds(x: i32, y: i32) -> (f64, f64, f64, f64) {
    let (px, py) = tile_plane(x, y);
    let (west, north) = plane_lonlat(px - 1.0, (py - 0.5).max(0.0));
    let (east, south) = plane_lonlat(px + 1.0, (py + 0.5).min(ROWS as f64));
    (west, south, east, north)
}
/// Fractional grid coordinates of a longitude/latitude, for walking tile edges.
pub fn tile_space(lon: f64, lat: f64) -> (f64, f64) {
    let (px, py) = plane(lon, lat);
    plane_to_grid(px, py)
}

/// Latitude of each sample row's centre.
pub struct Rows(pub Vec<f64>);
impl Rows {
    pub fn new() -> Self {
        Self((0..GY).map(|j| sample_row_lat(j as f64 + 0.5)).collect())
    }
}
/// Latitude of a (fractional) sample row.
fn sample_row_lat(row: f64) -> f64 {
    unit_to_lat((row + (ROW0 * SUPER) as f64) / (FULL_ROWS * SUPER) as f64)
}
/// Fractional sample row of a latitude (may fall outside `0..GY`).
fn lat_to_sample_row(lat: f64) -> f64 {
    lat_to_unit(lat) * (FULL_ROWS * SUPER) as f64 - (ROW0 * SUPER) as f64
}
/// Fractional sample column of a longitude.
fn lon_to_sample_col(lon: f64) -> f64 {
    lon_to_unit(lon) * GX as f64
}
/// Samples rows covering a latitude band: `(first, end)`, clamped to the raster.
pub fn sample_rows(south: f64, north: f64) -> (usize, usize) {
    (
        lat_to_sample_row(north).floor().clamp(0.0, GY as f64) as usize,
        lat_to_sample_row(south).ceil().clamp(0.0, GY as f64) as usize,
    )
}
/// Sample columns covering a longitude band: `(first, end)`.
pub fn sample_cols(west: f64, east: f64) -> (usize, usize) {
    (
        lon_to_sample_col(west).floor().clamp(0.0, GX as f64) as usize,
        lon_to_sample_col(east).ceil().clamp(0.0, GX as f64) as usize,
    )
}

/// Even-odd scanline fill of one polygon (outer ring plus holes). `span(row, start, end)` is
/// called for every horizontal run of samples whose centres lie inside, with `end` exclusive.
pub fn fill_polygon(rows: &Rows, polygon: &[Ring], mut span: impl FnMut(usize, usize, usize)) {
    let mut crossings: std::collections::BTreeMap<usize, Vec<f64>> = Default::default();
    for ring in polygon {
        for k in 0..ring.len() {
            let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
            if a.1 == b.1 {
                continue;
            }
            let (lo, hi) = if a.1 < b.1 { (a.1, b.1) } else { (b.1, a.1) };
            // Rows whose centre latitude is in (lo, hi].
            let first = (lat_to_sample_row(hi) - 0.5).ceil().max(0.0) as usize;
            let end = ((lat_to_sample_row(lo) - 0.5).ceil().max(0.0) as usize).min(GY);
            for j in first..end {
                let lat = rows.0[j];
                let t = (lat - a.1) / (b.1 - a.1);
                crossings.entry(j).or_default().push(a.0 + t * (b.0 - a.0));
            }
        }
    }
    for (j, mut xs) in crossings {
        xs.sort_by(|p, q| p.total_cmp(q));
        for pair in xs.chunks_exact(2) {
            let col = |lon: f64| ((lon_to_sample_col(lon) - 0.5).ceil().max(0.0) as usize).min(GX);
            let (i0, i1) = (col(pair[0]), col(pair[1]));
            if i1 > i0 {
                span(j, i0, i1);
            }
        }
    }
}

/// Even-odd point test against a polygon's rings.
pub fn contains(polygon: &[Ring], lon: f64, lat: f64) -> bool {
    let mut inside = false;
    for ring in polygon {
        for k in 0..ring.len() {
            let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
            if (a.1 > lat) != (b.1 > lat) && lon < a.0 + (lat - a.1) / (b.1 - a.1) * (b.0 - a.0) {
                inside = !inside;
            }
        }
    }
    inside
}

/// Deterministic value in [0, 1) for a tile and purpose.
pub fn hash01(x: i32, y: i32, salt: u64) -> f64 {
    let mut z = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ salt.wrapping_mul(0x1656_67B1_9E37_79F9);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_is_mercator_cut_below_the_pole_and_above_antarctica() {
        assert!((lat_to_unit(MAX_LAT) - 0.0).abs() < 1e-9);
        assert!((lat_to_unit(-MAX_LAT) - 1.0).abs() < 1e-9);
        assert!((lat_to_unit(0.0) - 0.5).abs() < 1e-12);
        for lat in [-80.0, -33.9, 0.0, 35.7, 51.5, 80.0] {
            assert!((unit_to_lat(lat_to_unit(lat)) - lat).abs() < 1e-9);
        }
        // The kept rows run from 84.0°N to 58.0°S.
        let north = unit_to_lat(ROW0 as f64 / FULL_ROWS as f64);
        let south = unit_to_lat((ROW0 + ROWS) as f64 / FULL_ROWS as f64);
        assert!((83.9..84.1).contains(&north), "{north}");
        assert!((-58.1..-57.9).contains(&south), "{south}");
        // Greenland's north cape fits; Cape Horn (56°S) fits; Antarctica's peninsula (63°S) does not.
        assert!(plane(-30.0, 83.6).1 >= 0.0);
        assert!(plane(-67.3, -56.0).1 < ROWS as f64);
        assert!(plane(-60.0, -63.0).1 > ROWS as f64);
    }
    #[test]
    fn the_lattice_is_an_upright_rectangle_of_the_same_number_of_tiles() {
        let tiles = (0..SIDE as i32)
            .flat_map(|y| (0..SIDE as i32).map(move |x| (x, y)))
            .filter(|&(x, y)| in_lattice(x, y))
            .count();
        assert_eq!(tiles, COLS * ROWS);
        // The north-west tile of the rectangle sits at the plane's origin.
        assert!(in_lattice(0, LATTICE_COLUMNS));
        assert_eq!(tile_plane(0, LATTICE_COLUMNS), (0.0, 0.0));
        assert!(!in_lattice(0, 0));
        // Greenwich on the equator is near the middle of the plane.
        let (x, y) = tile_of(0.0, 0.0);
        let (px, py) = tile_plane(x, y);
        assert!(
            (px - 256.0).abs() <= 1.0 && (py - (256 - ROW0) as f64).abs() <= 0.5,
            "{px},{py}"
        );
        // Tokyo is in the east, north of the equator.
        let (tx, ty) = tile_of(139.7, 35.7);
        let (px, py) = tile_plane(tx, ty);
        assert!(
            px > 400.0 && (190.0 - ROW0 as f64..210.0 - ROW0 as f64).contains(&py),
            "{px},{py}"
        );
        // A tile centre maps back into its own tile, and the grid maps are inverses.
        for (x, y) in [(256, 256), (300, 200), (330, 110), (200, 240)] {
            assert!(in_lattice(x, y));
            let (lon, lat) = tile_centre(x, y);
            assert_eq!(tile_of(lon, lat), (x, y));
            let (u, v) = tile_space(lon, lat);
            assert!(
                (u - (f64::from(x) + 0.5)).abs() < 1e-6 && (v - (f64::from(y) + 0.5)).abs() < 1e-6
            );
        }
        // Every point of the plane has a tile, edges and corners included.
        for (lon, lat) in [
            (-180.0, 84.0),
            (179.99, 84.0),
            (-180.0, -57.9),
            (179.99, -57.9),
            (0.0, 0.0),
        ] {
            let (x, y) = tile_of(lon, lat);
            assert!(in_lattice(x, y), "{lon},{lat} -> {x},{y}");
        }
        // North is up: moving north lowers the native row; moving east raises the column.
        let (_, north) = tile_plane(tile_of(10.0, 60.0).0, tile_of(10.0, 60.0).1);
        let (_, south) = tile_plane(tile_of(10.0, 10.0).0, tile_of(10.0, 10.0).1);
        assert!(north < south);
        let (west, _) = tile_plane(tile_of(-30.0, 10.0).0, tile_of(-30.0, 10.0).1);
        let (east, _) = tile_plane(tile_of(30.0, 10.0).0, tile_of(30.0, 10.0).1);
        assert!(west < east);
    }
    #[test]
    fn a_rectangle_fills_the_expected_samples() {
        let rows = Rows::new();
        let ring: Ring = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        let mut samples = 0usize;
        fill_polygon(&rows, std::slice::from_ref(&ring), |_, a, b| {
            samples += b - a
        });
        let width = 10.0 / 360.0 * GX as f64;
        let height = (lat_to_unit(0.0) - lat_to_unit(10.0)) * (FULL_ROWS * SUPER) as f64;
        let expected = width * height;
        assert!(
            (samples as f64 - expected).abs() / expected < 0.03,
            "{samples} vs {expected}"
        );
        assert!(contains(std::slice::from_ref(&ring), 5.0, 5.0) && !contains(&[ring], 11.0, 5.0));
    }
    #[test]
    fn holes_are_left_unfilled() {
        let rows = Rows::new();
        let outer: Ring = vec![(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)];
        let hole: Ring = vec![(5.0, 5.0), (15.0, 5.0), (15.0, 15.0), (5.0, 15.0)];
        let (mut solid, mut ringed) = (0usize, 0usize);
        fill_polygon(&rows, std::slice::from_ref(&outer), |_, a, b| {
            solid += b - a
        });
        fill_polygon(&rows, &[outer.clone(), hole.clone()], |_, a, b| {
            ringed += b - a
        });
        // The hole covers a quarter of the square.
        assert!(
            ringed < solid * 4 / 5 && ringed > solid * 2 / 3,
            "{ringed} of {solid}"
        );
        assert!(!contains(&[outer, hole], 10.0, 10.0));
    }
    #[test]
    fn hash_is_stable_and_uniform_enough() {
        assert_eq!(hash01(3, 4, 1), hash01(3, 4, 1));
        assert_ne!(hash01(3, 4, 1), hash01(4, 3, 1));
        let mean: f64 = (0..4000).map(|i| hash01(i, i / 7, 9)).sum::<f64>() / 4000.0;
        assert!((mean - 0.5).abs() < 0.03, "{mean}");
    }
}
