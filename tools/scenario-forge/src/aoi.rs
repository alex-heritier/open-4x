//! Historical 1895 territory for the bundled Age of Imperialism scenario.
//!
//! ```text
//! scenario-forge aoi [--scenario FILE] [--preview FILE] [--check]
//! ```
//!
//! The Age of Imperialism map is hand-drawn, not projected, so its cities' borders leave the
//! remote interiors of 1895 empires (Siberia, the Canadian north, the Sahara, the Australian
//! outback) to nobody. This command names who held every stretch of land on 1 January 1895 and
//! writes it into the scenario as regions, with `regional_fill` on, so the game hands that land
//! to the nearest city of its holder.
//!
//! Regions are authored in longitude and latitude in `data/aoi-1895/regions.json`, the same
//! paint operations as the 1876 tables, and painted onto Natural Earth's land. The map is tied
//! to the globe by `data/aoi-1895/anchors.json`, the real position of most of its cities: a
//! smooth warp through them gives the longitude and latitude under every tile, and the tile
//! takes the region most of its samples fall in. Tiles the warp puts at sea take the nearest
//! painted land. Cities claim their own land first, so the regions only decide the gaps.
//!
//! Where a filled stretch would lie far past every city's border, `data/aoi-1895/cities.json`
//! adds the real 1895 settlement there (a trading post or district seat), placed on the land
//! tile the warp puts nearest its position. Rerunning replaces them by name, so the command
//! can follow a fresh BIQ import.
use crate::data::RegionSource;
use crate::forge::{Shape, shape};
use crate::geo::{GX, GY, Polygon, lat_to_sample_row, lon_to_sample_col};
use crate::world::{PaintArea, Raster};
use crate::{output, preview::Image, source};
use fourx_content::{Pack, scenario::validate_scenario};
use fourx_sim::{CityStart, Game, Id, RegionStart, Scenario};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Anchor {
    /// A city of the scenario, by name...
    #[serde(default)]
    city: Option<String>,
    /// ...or a native `[column, row]` of a landmark no city stands on.
    #[serde(default)]
    tile: Option<[i32; 2]>,
    lon: f64,
    lat: f64,
}

/// A settlement of 1895 the AoI map lacks, added to fill an empty stretch.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Settlement {
    name: String,
    nation: String,
    lon: f64,
    lat: f64,
    population: i32,
    /// Part of its nation's homeland (Siberia), rather than a colony.
    home: bool,
}

/// A new settlement keeps more than this squared distance from every other city.
const SETTLEMENT_SPACING: i32 = 5;
/// Plane units per wrap of the world: the map is `2 * columns` half-tiles around.
const PERIOD: f64 = 180.0;
/// Painted land is looked for this many raster samples (about 7°) around a tile at sea.
const SEARCH: i32 = 40;
/// An island of at most this many tiles that no region reaches joins the region of the nearest
/// city within [`ISLAND_REACH`] tiles (squared): the colonial port of its archipelago.
const ISLAND_TILES: usize = 8;
const ISLAND_REACH: i32 = 16 * 16;

fn mercator(lat: f64) -> f64 {
    (std::f64::consts::FRAC_PI_4 + lat.to_radians() / 2.0)
        .tan()
        .ln()
}
fn inverse_mercator(m: f64) -> f64 {
    (2.0 * m.exp().atan() - std::f64::consts::FRAC_PI_2).to_degrees()
}

/// Least squares `y = a + b x`.
fn line(points: &[(f64, f64)]) -> (f64, f64) {
    let n = points.len() as f64;
    let (sx, sy) = points
        .iter()
        .fold((0.0, 0.0), |(sx, sy), (x, y)| (sx + x, sy + y));
    let (mx, my) = (sx / n, sy / n);
    let (sxy, sxx) = points.iter().fold((0.0, 0.0), |(sxy, sxx), (x, y)| {
        (sxy + (x - mx) * (y - my), sxx + (x - mx) * (x - mx))
    });
    let b = sxy / sxx;
    (my - b * mx, b)
}

/// Longitude and latitude under any point of the map's plane (`x` the native column, `y` half
/// the native row): a global equirectangular-by-Mercator fit, bent through the anchors by
/// Gaussian-weighted residuals whose reach widens where anchors are sparse.
struct Warp {
    /// `x = a + b * lon`, with `b` one map width per 360°.
    x: (f64, f64),
    /// `y = c + e * mercator(lat)`.
    y: (f64, f64),
    /// Each anchor's plane position and how far the fit misses it: `(x, y, Δlon, Δmercator)`.
    residuals: Vec<(f64, f64, f64, f64)>,
}

impl Warp {
    fn fit(anchors: &[(f64, f64, f64, f64)]) -> Self {
        // Once around the map is once around the world, so the column scale is fixed and only
        // the column of the prime meridian is fitted: the mean over the anchors, each taken
        // within half a world of the first.
        let b = PERIOD / 360.0;
        let offset = |a: &(f64, f64, f64, f64)| a.0 - b * a.2;
        let first = offset(&anchors[0]);
        let near = |o: f64| o + PERIOD * ((first - o) / PERIOD).round();
        let mean = anchors.iter().map(|a| near(offset(a))).sum::<f64>() / anchors.len() as f64;
        let x = (mean, b);
        let y = line(
            &anchors
                .iter()
                .map(|a| (mercator(a.3), a.1))
                .collect::<Vec<_>>(),
        );
        let mut warp = Warp {
            x,
            y,
            residuals: Vec::new(),
        };
        warp.residuals = anchors
            .iter()
            .map(|a| {
                let (glon, gm) = warp.global(a.0, a.1);
                let dl = (a.2 - glon + 180.0).rem_euclid(360.0) - 180.0;
                (a.0, a.1, dl, mercator(a.3) - gm)
            })
            .collect();
        warp
    }

    fn global(&self, x: f64, y: f64) -> (f64, f64) {
        ((x - self.x.0) / self.x.1, (y - self.y.0) / self.y.1)
    }

    fn lonlat(&self, x: f64, y: f64) -> (f64, f64) {
        let (lon, m) = self.global(x, y);
        let mut near: Vec<(f64, f64, f64)> = self
            .residuals
            .iter()
            .map(|&(ax, ay, dl, dm)| {
                let dx = (x - ax + PERIOD / 2.0).rem_euclid(PERIOD) - PERIOD / 2.0;
                (dx * dx + (y - ay) * (y - ay), dl, dm)
            })
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        let reach = near.get(3).map_or(9.0, |n| n.0).max(9.0);
        let (mut w, mut sl, mut sm) = (1e-6, 0.0, 0.0);
        for &(d2, dl, dm) in near.iter().take(24) {
            let k = (-d2 / (2.0 * reach)).exp();
            w += k;
            sl += k * dl;
            sm += k * dm;
        }
        let lon = (lon + sl / w + 180.0).rem_euclid(360.0) - 180.0;
        (lon, inverse_mercator(m + sm / w))
    }
}

/// A raster sample of a longitude and latitude, if the raster covers it.
fn sample(lon: f64, lat: f64) -> Option<(i32, i32)> {
    let (i, j) = (
        lon_to_sample_col(lon).floor() as i32,
        lat_to_sample_row(lat).floor() as i32,
    );
    (0..GY as i32)
        .contains(&j)
        .then_some((i.rem_euclid(GX as i32), j))
}

pub fn run(args: &[String], root: &Path) -> Result<(), String> {
    let mut path = root.join("../../assets/packs/base/scenarios/age-of-imperialism.json");
    let mut preview_path = root.join("cache/aoi-1895.png");
    let mut check = false;
    let mut iter = args.iter();
    while let Some(flag) = iter.next() {
        let mut value = || {
            iter.next()
                .cloned()
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        match flag.as_str() {
            "--scenario" => path = PathBuf::from(value()?),
            "--preview" => preview_path = PathBuf::from(value()?),
            "--check" => check = true,
            other => return Err(format!("unknown option {other}")),
        }
    }
    let read = |path: &Path| {
        std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))
    };
    let text = read(&path)?;
    let mut scenario: Scenario =
        serde_json::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))?;
    let data = root.join("data/aoi-1895");
    let anchors: Vec<Anchor> = serde_json::from_str(&read(&data.join("anchors.json"))?)
        .map_err(|error| format!("anchors.json: {error}"))?;
    let regions: Vec<RegionSource> = serde_json::from_str(&read(&data.join("regions.json"))?)
        .map_err(|error| format!("regions.json: {error}"))?;
    let lattice = scenario.map.lattice.ok_or("the scenario is not upright")?;

    // --- Tie the map to the globe ---------------------------------------------------------------
    let positions: HashMap<&str, (i32, i32)> = scenario
        .cities
        .iter()
        .map(|city| (city.name.as_str(), lattice.native(city.position)))
        .collect();
    let mut points = Vec::new();
    for anchor in &anchors {
        let (cx, cy) = match (&anchor.city, anchor.tile) {
            (Some(city), None) => *positions
                .get(city.as_str())
                .ok_or_else(|| format!("anchor {city} is not a city of the scenario"))?,
            (None, Some([cx, cy])) => (cx, cy),
            _ => return Err("an anchor names exactly one of city and tile".into()),
        };
        points.push((f64::from(cx), f64::from(cy) / 2.0, anchor.lon, anchor.lat));
    }
    let warp = Warp::fit(&points);

    // --- Add the settlements the map lacks -----------------------------------------------------
    let settlements: Vec<Settlement> = serde_json::from_str(&read(&data.join("cities.json"))?)
        .map_err(|error| format!("cities.json: {error}"))?;
    scenario
        .cities
        .retain(|city| settlements.iter().all(|s| s.name != city.name));
    for settlement in &settlements {
        let mut best: Option<(f64, (i32, i32))> = None;
        for cy in 0..lattice.rows {
            for cx in (cy & 1..2 * lattice.columns).step_by(2) {
                let position = lattice.cell(cx, cy);
                let map = &scenario.map;
                if !map.get(position).is_some_and(|tile| tile.is_land())
                    || scenario.cities.iter().any(|city| {
                        map.distance_squared(city.position, position) <= SETTLEMENT_SPACING
                    })
                {
                    continue;
                }
                let (lon, lat) = warp.lonlat(f64::from(cx), f64::from(cy) / 2.0);
                let dlon = (lon - settlement.lon + 540.0).rem_euclid(360.0) - 180.0;
                let d = (dlon * settlement.lat.to_radians().cos()).hypot(lat - settlement.lat);
                if best.is_none_or(|(b, _)| d < b) {
                    best = Some((d, (cx, cy)));
                }
            }
        }
        let (d, (cx, cy)) = best.ok_or_else(|| format!("no room for {}", settlement.name))?;
        if d > 3.0 {
            return Err(format!("{} lands {d:.1}° from its place", settlement.name));
        }
        scenario.cities.push(CityStart {
            home: settlement.home,
            nation: settlement.nation.clone(),
            position: lattice.cell(cx, cy),
            name: settlement.name.clone(),
            population: settlement.population,
            industry: 1,
            capital: false,
            border: None,
        });
    }

    // --- Paint the regions onto Natural Earth's land ----------------------------------------
    let cache = root.join("cache");
    let countries = source::countries(&source::fetch(&cache, &source::COUNTRIES)?)?;
    let lakes = source::lakes(&source::fetch(&cache, &source::LAKES)?)?;
    let nation_ids: Vec<&str> = scenario.nations.iter().map(|n| n.id.as_str()).collect();
    let mut raster = Raster::new(&countries, &lakes);
    for (index, region) in regions.iter().enumerate() {
        if let Some(nation) = &region.nation
            && !nation_ids.contains(&nation.as_str())
        {
            return Err(format!(
                "region {} names unknown nation {nation}",
                region.id
            ));
        }
        let index = u16::try_from(index + 1).map_err(|_| "too many regions")?;
        for paint in &region.paint {
            let polygons: Vec<Polygon> = match shape(paint, &countries, &region.id)? {
                Shape::Polygons(polygons, bbox) => {
                    raster.paint(
                        &polygons,
                        &PaintArea {
                            bbox,
                            land_only: true,
                        },
                        index,
                    );
                    continue;
                }
                // A hand-placed point claims a small square, land or not.
                Shape::Points(points) => points
                    .iter()
                    .map(|&(lon, lat)| {
                        let r = 0.4;
                        vec![vec![
                            (lon - r, lat - r),
                            (lon + r, lat - r),
                            (lon + r, lat + r),
                            (lon - r, lat + r),
                        ]]
                    })
                    .collect(),
            };
            raster.paint(
                &polygons,
                &PaintArea {
                    bbox: None,
                    land_only: false,
                },
                index,
            );
        }
    }
    let painted = |i: i32, j: i32| -> Option<u16> {
        let at = j as usize * GX + i.rem_euclid(GX as i32) as usize;
        let region = raster.region[at];
        (region != 0).then_some(region)
    };

    // --- Give every land tile the region under it --------------------------------------------
    // Raster region index, or 0 where nothing is painted near.
    let mut chosen: HashMap<(i32, i32), u16> = HashMap::new();
    for cy in 0..lattice.rows {
        for cx in (cy & 1..2 * lattice.columns).step_by(2) {
            let tile = scenario
                .map
                .get(lattice.cell(cx, cy))
                .expect("lattice tile");
            if !tile.is_land() {
                continue;
            }
            let (x, y) = (f64::from(cx), f64::from(cy) / 2.0);
            let mut tally: BTreeMap<u16, u32> = BTreeMap::new();
            for dy in [-0.3, 0.0, 0.3] {
                for dx in [-0.6, 0.0, 0.6] {
                    let (lon, lat) = warp.lonlat(x + dx, y + dy);
                    if let Some(region) = sample(lon, lat).and_then(|(i, j)| painted(i, j)) {
                        *tally.entry(region).or_default() += 1;
                    }
                }
            }
            let region = match tally
                .iter()
                .max_by_key(|&(&r, &n)| (n, std::cmp::Reverse(r)))
            {
                Some((&region, _)) => region,
                None => {
                    let (lon, lat) = warp.lonlat(x, y);
                    sample(lon, lat).map_or(0, |(i, j)| {
                        // Nearest painted sample, ring by ring.
                        (1..=SEARCH)
                            .find_map(|r| {
                                let mut best: Option<(i32, u16)> = None;
                                for dj in -r..=r {
                                    for di in -r..=r {
                                        if di.abs().max(dj.abs()) != r
                                            || !(0..GY as i32).contains(&(j + dj))
                                        {
                                            continue;
                                        }
                                        if let Some(region) = painted(i + di, j + dj) {
                                            let d = di * di + dj * dj;
                                            if best.is_none_or(|(b, _)| d < b) {
                                                best = Some((d, region));
                                            }
                                        }
                                    }
                                }
                                best.map(|(_, region)| region)
                            })
                            .unwrap_or(0)
                    })
                }
            };
            chosen.insert((cx, cy), region);
        }
    }

    adopt_islands(&scenario, &mut chosen);
    // --- Write the regions that name a holder --------------------------------------------------
    let mut out_index: HashMap<u16, u16> = HashMap::new();
    scenario.regions.clear();
    for (index, region) in regions.iter().enumerate() {
        let index = index as u16 + 1;
        let Some(nation) = &region.nation else {
            continue;
        };
        if !chosen.values().any(|&r| r == index) {
            continue;
        }
        scenario.regions.push(RegionStart {
            id: region.id.clone(),
            name: region.name.clone(),
            nation: nation.clone(),
        });
        out_index.insert(index, scenario.regions.len() as u16);
    }
    for tile in &mut scenario.map.tiles {
        tile.region = 0;
    }
    for (&(cx, cy), region) in &chosen {
        scenario
            .map
            .get_mut(lattice.cell(cx, cy))
            .expect("lattice tile")
            .region = out_index.get(region).copied().unwrap_or(0);
    }
    scenario.regional_fill = true;

    let pack = Pack::base();
    let rules = pack.rules.with_overrides(&scenario.rules);
    validate_scenario(&scenario, &rules).map_err(|error| error.to_string())?;
    let game = Game::from_scenario(1, &rules, &scenario, None)
        .map_err(|error| format!("scenario does not start: {error}"))?;
    political(&scenario, &game).save(&preview_path)?;
    let land = |owned: bool| {
        game.map
            .tiles
            .iter()
            .filter(|t| t.is_land() && (t.owner != 0) == owned)
            .count()
    };
    println!(
        "{} regions; {} land tiles held, {} unclaimed; preview {}",
        scenario.regions.len(),
        land(true),
        land(false),
        preview_path.display()
    );
    if !check {
        // Only the parts this command owns are replaced, so the rest of the file keeps the
        // form its own tools wrote it in.
        fn json(value: impl serde::Serialize) -> Result<serde_json::Value, String> {
            serde_json::to_value(value).map_err(|error| error.to_string())
        }
        let mut value: serde_json::Value = serde_json::from_str(&text).expect("parsed above");
        let cities = value["cities"]
            .as_array_mut()
            .ok_or("the scenario has no cities")?;
        cities.retain(|city| settlements.iter().all(|s| city["name"] != s.name.as_str()));
        for city in &scenario.cities[scenario.cities.len() - settlements.len()..] {
            // In the order and form the importer writes a city: `home` only when false.
            let mut entry =
                serde_json::json!({"nation": city.nation, "position": json(city.position)?});
            if !city.home {
                entry["home"] = false.into();
            }
            entry["name"] = city.name.clone().into();
            entry["population"] = city.population.into();
            entry["industry"] = city.industry.into();
            cities.push(entry);
        }
        value["regional_fill"] = true.into();
        value["regions"] = json(&scenario.regions)?;
        value["map"]["regions"] = json(&scenario.map)?["regions"].take();
        std::fs::write(&path, output::render_over(&value, &text))
            .map_err(|error| format!("{}: {error}", path.display()))?;
    }
    Ok(())
}

/// Hand small unpainted islands to the region of the nearest city's tile.
fn adopt_islands(scenario: &Scenario, chosen: &mut HashMap<(i32, i32), u16>) {
    let map = &scenario.map;
    let lattice = map.lattice.expect("upright");
    let mut seen = std::collections::HashSet::new();
    let starts: Vec<(i32, i32)> = chosen.keys().copied().collect();
    for start in starts {
        if !seen.insert(start) {
            continue;
        }
        let mut island = vec![start];
        let mut next = 0;
        while next < island.len() {
            let p = lattice.cell(island[next].0, island[next].1);
            next += 1;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let Some(q) = map.normalize(fourx_sim::terrain::Coord::new(p.x + dx, p.y + dy))
                    else {
                        continue;
                    };
                    let key = lattice.native(q);
                    if chosen.contains_key(&key) && seen.insert(key) {
                        island.push(key);
                    }
                }
            }
        }
        if island.len() > ISLAND_TILES || island.iter().any(|key| chosen[key] != 0) {
            continue;
        }
        let first = lattice.cell(island[0].0, island[0].1);
        let nearest = scenario
            .cities
            .iter()
            .map(|city| (map.distance_squared(first, city.position), city.position))
            .filter(|&(d, _)| d <= ISLAND_REACH)
            .min_by_key(|&(d, _)| d);
        if let Some((_, city)) = nearest
            && let Some(&region) = chosen.get(&lattice.native(city))
        {
            for key in &island {
                chosen.insert(*key, region);
            }
        }
    }
}

/// The political map: every tile its holder's colour, unclaimed land pale, cities as dots.
fn political(scenario: &Scenario, game: &Game) -> Image {
    const SCALE: usize = 8;
    let lattice = scenario.map.lattice.expect("upright");
    let (width, height) = ((2 * lattice.columns) as usize, (lattice.rows / 2) as usize);
    let mut image = Image::new(width * SCALE, height * SCALE, [18, 32, 60]);
    for py in 0..height {
        for px in 0..width {
            let p = lattice.cell(px as i32, (2 * py + (px & 1)) as i32);
            let tile = game.map.get(p).expect("lattice tile");
            if !tile.is_land() {
                continue;
            }
            let colour = match tile.owner {
                0 => [236, 232, 220],
                owner => game.factions[&(owner as Id)].color.0,
            };
            image.fill_rect(px * SCALE, py * SCALE, SCALE, colour);
        }
    }
    for city in &scenario.cities {
        let (cx, cy) = lattice.native(city.position);
        let (x, y) = (cx as usize * SCALE + 2, (cy / 2) as usize * SCALE + 2);
        image.fill_rect(x, y, 4, [16, 16, 16]);
    }
    image
}
