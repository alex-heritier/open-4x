//! Turns the curated tables plus Natural Earth geometry into a [`Scenario`].
use crate::classify::{self, Inputs};
use crate::data::{CitySource, NationSource, Paint, Sources};
use crate::geo::{LATTICE_COLUMNS, LATTICE_ROWS, Polygon, Ring, plane, tile_of, tile_plane};
use crate::world::{PaintArea, Raster, Tiles, at, exists};
use fourx_sim::terrain::{Coord, Lattice, Map, Terrain};
use fourx_sim::{CityStart, Date, NationStart, RegionStart, SCENARIO_FORMAT, Scenario, UnitStart};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

pub struct Output {
    pub scenario: Scenario,
    /// Informational lines and warnings for the person running the forge.
    pub report: Vec<String>,
}

fn ring_of_points(points: &[[f64; 2]]) -> Ring {
    points.iter().map(|p| (p[0], p[1])).collect()
}

fn rect_ring([west, south, east, north]: [f64; 4]) -> Ring {
    vec![(west, south), (east, south), (east, north), (west, north)]
}

enum Shape {
    Polygons(Vec<Polygon>, Option<[f64; 4]>),
    Points(Vec<(f64, f64)>),
}

fn shape(
    paint: &Paint,
    countries: &BTreeMap<String, Vec<Polygon>>,
    context: &str,
) -> Result<Shape, String> {
    let given = [
        paint.country.is_some(),
        paint.poly.is_some(),
        paint.rect.is_some(),
        paint.tiles.is_some(),
    ];
    if given.iter().filter(|&&g| g).count() != 1
        || (paint.bbox.is_some() && paint.country.is_none())
    {
        return Err(format!(
            "{context}: a paint op needs exactly one of country/poly/rect/tiles (bbox only goes with country)"
        ));
    }
    Ok(if let Some(code) = &paint.country {
        let polygons = countries
            .get(code)
            .ok_or_else(|| format!("{context}: unknown country code {code}"))?;
        Shape::Polygons(polygons.clone(), paint.bbox)
    } else if let Some(poly) = &paint.poly {
        if poly.len() < 3 {
            return Err(format!("{context}: polygon needs at least three vertices"));
        }
        Shape::Polygons(vec![vec![ring_of_points(poly)]], None)
    } else if let Some(rect) = paint.rect {
        Shape::Polygons(vec![vec![rect_ring(rect)]], None)
    } else {
        Shape::Points(
            paint
                .tiles
                .as_ref()
                .expect("one shape given")
                .iter()
                .map(|p| (p[0], p[1]))
                .collect(),
        )
    })
}

struct Placed {
    nation: usize,
    name: String,
    x: i32,
    y: i32,
    pop: u32,
    capital: bool,
    dev: u32,
}

pub fn build(
    src: &Sources,
    countries: &BTreeMap<String, Vec<Polygon>>,
    lakes: &[Polygon],
    inputs: &Inputs,
) -> Result<Output, String> {
    let mut report = Vec::new();

    // --- Validate references between tables ---------------------------------------------------
    let mut nation_index: HashMap<&str, usize> = HashMap::new();
    for (i, nation) in src.nations.iter().enumerate() {
        if nation_index.insert(&nation.id, i).is_some() {
            return Err(format!("duplicate nation {}", nation.id));
        }
        if nation.dev > 10 || nation.army > 4 {
            return Err(format!("nation {}: dev is 0-10 and army is 0-4", nation.id));
        }
    }
    let mut region_ids = HashSet::new();
    for region in &src.regions {
        if !region_ids.insert(region.id.as_str()) {
            return Err(format!("duplicate region {}", region.id));
        }
        if let Some(nation) = &region.nation
            && !nation_index.contains_key(nation.as_str())
        {
            return Err(format!(
                "region {} names unknown nation {nation}",
                region.id
            ));
        }
    }

    // --- Paint regions onto the supersampled raster -------------------------------------------
    let mut raster = Raster::new(countries, lakes);
    let mut provisional: Vec<u16> = Vec::with_capacity(src.regions.len());
    let mut next = 1u16;
    let mut claims: Vec<(f64, f64, u16, String)> = Vec::new();
    for region in &src.regions {
        let index = if region.nation.is_some() {
            let index = next;
            next += 1;
            index
        } else {
            0
        };
        provisional.push(index);
        for paint in &region.paint {
            match shape(paint, countries, &format!("region {}", region.id))? {
                Shape::Polygons(polygons, bbox) => raster.paint(
                    &polygons,
                    &PaintArea {
                        bbox,
                        land_only: true,
                    },
                    index,
                ),
                Shape::Points(points) => claims.extend(
                    points
                        .into_iter()
                        .map(|(lon, lat)| (lon, lat, index, region.id.clone())),
                ),
            }
        }
    }

    // --- Reduce to tiles and apply hand-placed land and water ---------------------------------
    let mut tiles = Tiles::reduce(&raster);
    let mut forced_water = HashSet::new();
    for [lon, lat] in &src.terrain.water {
        let (x, y) = tile_of(*lon, *lat);
        let i = at(x, y).expect("tile_of is clamped");
        tiles.land[i] = false;
        tiles.region[i] = 0;
        forced_water.insert(i);
    }
    for (lon, lat, index, id) in &claims {
        let (x, y) = tile_of(*lon, *lat);
        let i = at(x, y).expect("tile_of is clamped");
        if forced_water.contains(&i) {
            return Err(format!("region {id}: tile {x},{y} is also forced to water"));
        }
        tiles.land[i] = true;
        tiles.region[i] = *index;
    }
    for [lon, lat] in &src.terrain.land {
        let (x, y) = tile_of(*lon, *lat);
        let i = at(x, y).expect("tile_of is clamped");
        if forced_water.contains(&i) {
            return Err(format!("tile {x},{y} is forced to both land and water"));
        }
        if !tiles.land[i] {
            let neighbour = [(0, -1), (1, 0), (0, 1), (-1, 0)]
                .iter()
                .filter_map(|(dx, dy)| at(x + dx, y + dy))
                .find(|&j| tiles.land[j])
                .map_or(0, |j| tiles.region[j]);
            tiles.land[i] = true;
            tiles.region[i] = neighbour;
        }
    }

    // --- Place cities --------------------------------------------------------------------------
    // A city that stands on a tile its own nation paints by hand (a rock, an enclave) takes it
    // before any bigger neighbour can be nudged onto it.
    let index_nation: HashMap<u16, usize> = src
        .regions
        .iter()
        .zip(&provisional)
        .filter_map(|(region, &index)| {
            let nation = region.nation.as_deref()?;
            Some((index, nation_index[nation]))
        })
        .collect();
    let claimed: HashMap<usize, usize> = claims
        .iter()
        .filter_map(|(lon, lat, index, _)| {
            let (x, y) = tile_of(*lon, *lat);
            Some((at(x, y)?, *index_nation.get(index)?))
        })
        .collect();
    let reserved = |i: usize| -> bool {
        let city = &src.cities[i];
        let (x, y) = tile_of(city.lon, city.lat);
        at(x, y).and_then(|tile| claimed.get(&tile)).copied()
            == nation_index.get(city.nation.as_str()).copied()
            && claimed.contains_key(&at(x, y).unwrap_or(usize::MAX))
    };
    let mut order: Vec<usize> = (0..src.cities.len()).collect();
    order.sort_by_key(|&i| {
        (
            !src.cities[i].capital,
            !reserved(i),
            std::cmp::Reverse(src.cities[i].pop),
            i,
        )
    });
    let mut occupied: HashSet<usize> = HashSet::new();
    let mut placed: Vec<Placed> = Vec::new();
    let (mut moved, mut dropped) = (0usize, Vec::new());
    for &i in &order {
        let city: &CitySource = &src.cities[i];
        let Some(&nation) = nation_index.get(city.nation.as_str()) else {
            return Err(format!(
                "city {} names unknown nation {}",
                city.name, city.nation
            ));
        };
        let (x, y) = tile_of(city.lon, city.lat);
        let exact = plane(city.lon, city.lat);
        // How far, in tiles, a city may be nudged: the old 5x5 and 7x7 squares, as diamonds.
        let reach: f64 = if city.capital { 4.5 } else { 3.0 };
        let mut best: Option<(f64, i32, i32)> = None;
        for dy in -5..=5 {
            for dx in -5..=5 {
                let Some(j) = at(x + dx, y + dy) else {
                    continue;
                };
                if !tiles.land[j] || occupied.contains(&j) {
                    continue;
                }
                // Prefer the home tile, then the nearest one.
                // Tiles are two plane units wide and one tall, so a step to any neighbour
                // costs one; the same metric `tile_of` uses to choose the home tile.
                let centre = tile_plane(x + dx, y + dy);
                let distance = (centre.0 - exact.0).abs() / 2.0 + (centre.1 - exact.1).abs();
                if distance <= reach && best.is_none_or(|(d, _, _)| distance < d) {
                    best = Some((distance, x + dx, y + dy));
                }
            }
        }
        match best {
            Some((_, px, py)) => {
                moved += usize::from((px, py) != (x, y));
                occupied.insert(at(px, py).expect("on map"));
                placed.push(Placed {
                    nation,
                    name: city.name.clone(),
                    x: px,
                    y: py,
                    pop: city.pop,
                    capital: city.capital,
                    dev: city.dev.unwrap_or(src.nations[nation].dev),
                });
            }
            None if city.capital => {
                return Err(format!(
                    "capital {} ({}) has no free land tile nearby",
                    city.name, city.nation
                ));
            }
            None => dropped.push(format!("{} ({})", city.name, city.nation)),
        }
    }
    if moved > 0 {
        report.push(format!(
            "{moved} cities were nudged to the nearest free land tile"
        ));
    }
    if !dropped.is_empty() {
        report.push(format!(
            "dropped {} cities with no free land tile nearby: {}",
            dropped.len(),
            dropped.join(", ")
        ));
    }

    // A city's own tile belongs to its nation's nearest region.
    let nation_regions: HashMap<usize, Vec<u16>> = {
        let mut map: HashMap<usize, Vec<u16>> = HashMap::new();
        for (region, &index) in src.regions.iter().zip(&provisional) {
            if let Some(nation) = &region.nation {
                map.entry(nation_index[nation.as_str()])
                    .or_default()
                    .push(index);
            }
        }
        map
    };
    let region_nation: HashMap<u16, usize> = nation_regions
        .iter()
        .flat_map(|(&nation, list)| list.iter().map(move |&r| (r, nation)))
        .collect();
    for city in &placed {
        let i = at(city.x, city.y).expect("on map");
        if region_nation.get(&tiles.region[i]) == Some(&city.nation) {
            continue;
        }
        let mut nearest: Option<(i32, u16)> = None;
        for dy in -4..=4 {
            for dx in -4..=4 {
                let Some(j) = at(city.x + dx, city.y + dy) else {
                    continue;
                };
                if tiles.land[j] && region_nation.get(&tiles.region[j]) == Some(&city.nation) {
                    let distance = dx.abs().max(dy.abs());
                    if nearest.is_none_or(|(d, _)| distance < d) {
                        nearest = Some((distance, tiles.region[j]));
                    }
                }
            }
        }
        match nearest {
            Some((_, region)) => tiles.region[i] = region,
            None => report.push(format!(
                "warning: {} ({}) has no region of its own nation within 4 tiles; add a `tiles` paint",
                city.name, src.nations[city.nation].id
            )),
        }
    }

    // --- Terrain: climate, elevation, cover, rivers, water depth --------------------------------
    let city_tiles: HashSet<usize> = occupied.clone();
    let layers = classify::classify(&tiles.land, &city_tiles, &src.terrain, inputs, &mut report)?;

    // --- Drop unused regions and renumber -------------------------------------------------------
    let used: BTreeSet<u16> = (0..tiles.land.len())
        .filter(|&i| tiles.land[i] && tiles.region[i] != 0)
        .map(|i| tiles.region[i])
        .collect();
    let mut final_index: HashMap<u16, u16> = HashMap::new();
    let mut regions = Vec::new();
    let mut unused = Vec::new();
    for (region, &index) in src.regions.iter().zip(&provisional) {
        let Some(nation) = &region.nation else {
            continue;
        };
        if used.contains(&index) {
            regions.push(RegionStart {
                id: region.id.clone(),
                name: region.name.clone(),
                nation: nation.clone(),
            });
            final_index.insert(index, regions.len() as u16);
        } else {
            unused.push(region.id.as_str());
        }
    }
    if !unused.is_empty() {
        report.push(format!(
            "warning: regions covering no tile were dropped: {}",
            unused.join(", ")
        ));
    }

    let mut map = Map::embedded(
        Lattice {
            columns: LATTICE_COLUMNS,
            rows: LATTICE_ROWS,
        },
        Terrain::Ocean,
    );
    for (i, tile) in map.tiles.iter_mut().enumerate() {
        tile.terrain = layers.terrain[i];
        tile.relief = layers.relief[i];
        tile.cover = layers.cover[i];
        tile.river = layers.river[i];
        if tiles.land[i] {
            tile.region = final_index.get(&tiles.region[i]).copied().unwrap_or(0);
        }
    }
    if let Some(tile) = map.tiles.iter().find(|t| !t.layers_valid()) {
        return Err(format!(
            "tile {},{} has layers that cannot coexist",
            tile.position.x, tile.position.y
        ));
    }

    // --- Nations, cities, units ------------------------------------------------------------------
    let nations: Vec<NationStart> = src.nations.iter().map(nation_start).collect();
    let mut cities: Vec<CityStart> = Vec::new();
    for city in &placed {
        let pop = (1.0 + (f64::from(city.pop).sqrt() / 6.0).round()).clamp(1.0, 100.0) as i32;
        let industry = (1 + city.dev as i32 / 2 + i32::from(city.capital)).clamp(1, 100);
        cities.push(CityStart {
            nation: src.nations[city.nation].id.clone(),
            position: Coord::new(city.x, city.y),
            name: city.name.clone(),
            population: pop,
            industry,
            capital: city.capital,
            border: None,
        });
    }
    cities.sort_by(|a, b| {
        (
            nation_index[a.nation.as_str()],
            !a.capital,
            std::cmp::Reverse(a.population),
            &a.name,
        )
            .cmp(&(
                nation_index[b.nation.as_str()],
                !b.capital,
                std::cmp::Reverse(b.population),
                &b.name,
            ))
    });

    let units = forces(src, &cities, &map)?;

    let scenario = Scenario {
        format: SCENARIO_FORMAT,
        id: src.meta.id.clone(),
        name: src.meta.name.clone(),
        description: src.meta.description.clone(),
        start_date: Date::parse(&src.meta.start_date)
            .ok_or_else(|| format!("bad start_date {}", src.meta.start_date))?,
        commander: src.meta.commander.clone(),
        intro: src.meta.intro.clone(),
        charted: src.meta.charted,
        rules: src.meta.rules.clone(),
        map,
        nations,
        regions,
        cities,
        units,
        wars: src.meta.wars.clone(),
    };
    report.extend(connectivity(&scenario));
    Ok(Output { scenario, report })
}

fn nation_start(n: &NationSource) -> NationStart {
    NationStart {
        id: n.id.clone(),
        name: n.name.clone(),
        adjective: if n.adjective.is_empty() {
            n.name.clone()
        } else {
            n.adjective.clone()
        },
        color: n.color,
        flavor: n.flavor,
        status: n.status,
        suzerain: n.suzerain.clone(),
        government: n.government.clone(),
        leader: n.leader.clone(),
        gold: Some(60 + 40 * n.dev as i32),
        technology: n.dev / 3,
        notes: n.notes.clone(),
    }
}

/// Starting units by military tier, workers at each capital, and fleets from each nation's
/// `navy` table. Every unit stands alone; a "formation" is just units that start on one square.
fn forces(src: &Sources, cities: &[CityStart], map: &Map) -> Result<Vec<UnitStart>, String> {
    let mut units = Vec::new();
    let mut taken: HashSet<Coord> = HashSet::new();
    for nation in &src.nations {
        let own: Vec<&CityStart> = cities.iter().filter(|c| c.nation == nation.id).collect();
        if own.is_empty() {
            return Err(format!("nation {} has no city", nation.id));
        }
        let tier = nation.army as i32;
        // (units, experience tier). Cities are listed capital first, so the Guard sits there.
        let formations: Vec<(Vec<&str>, i32)> = match nation.army {
            0 => vec![(vec!["infantry"], 0)],
            1 => vec![(vec!["infantry", "cavalry"], 1)],
            2 => vec![(vec!["infantry", "infantry", "cavalry"], 2)],
            3 => vec![
                (vec!["infantry", "infantry", "cavalry", "artillery"], 3),
                (vec!["infantry", "cavalry"], 2),
            ],
            _ => vec![
                (
                    vec!["infantry", "infantry", "infantry", "cavalry", "artillery"],
                    4,
                ),
                (vec!["infantry", "infantry", "artillery"], 3),
                (vec!["infantry", "cavalry"], 2),
            ],
        };
        for (k, (list, experience)) in formations.into_iter().enumerate() {
            let city = own[k.min(own.len() - 1)];
            // Conscripts in the weakest armies, veterans and elites in the strongest.
            let level = match experience.min(tier.max(0)) {
                0 => Some(0),
                1 | 2 => None,
                3 => Some(2),
                _ => Some(3),
            };
            for kind in list {
                units.push(UnitStart {
                    nation: nation.id.clone(),
                    position: city.position,
                    kind: kind.to_string(),
                    level,
                    // The capital's infantry dig in; the rest wait for orders.
                    fortified: k == 0 && kind == "infantry",
                });
            }
        }
        // Workers: one per developed nation tier, at the capital.
        for _ in 0..1 + usize::from(nation.dev >= 6) + usize::from(nation.dev >= 9) {
            units.push(UnitStart {
                nation: nation.id.clone(),
                position: own[0].position,
                kind: "worker".to_string(),
                level: None,
                fortified: false,
            });
        }
        for fleet in &nation.navy {
            let port = own.iter().find(|c| c.name == fleet.port).ok_or_else(|| {
                format!(
                    "nation {}: fleet port {} is not one of its cities",
                    nation.id, fleet.port
                )
            })?;
            let mut best: Option<(f64, Coord)> = None;
            let here = tile_plane(port.position.x, port.position.y);
            for dy in -6..=6 {
                for dx in -6..=6 {
                    let p = Coord::new(port.position.x + dx, port.position.y + dy);
                    if map.get(p).is_some_and(|t| t.terrain.is_water()) && !taken.contains(&p) {
                        let there = tile_plane(p.x, p.y);
                        let d = (there.0 - here.0).hypot(there.1 - here.1);
                        if best.is_none_or(|(b, _)| d < b) {
                            best = Some((d, p));
                        }
                    }
                }
            }
            let (_, position) =
                best.ok_or_else(|| format!("no water near {} for {}", fleet.port, nation.id))?;
            taken.insert(position);
            for _ in 0..fleet.ships {
                units.push(UnitStart {
                    nation: nation.id.clone(),
                    position,
                    kind: "ironclad".to_string(),
                    level: if nation.dev >= 8 { Some(2) } else { None },
                    fortified: false,
                });
            }
        }
    }
    Ok(units)
}

/// Describes the largest seas and landmasses, to catch closed straits and accidental bridges.
/// Units step to any of the eight surrounding tiles, so that is how seas and lands join.
fn connectivity(scenario: &Scenario) -> Vec<String> {
    let map = &scenario.map;
    let side = map.width as usize;
    let label = |water: bool| -> Vec<(usize, usize, usize)> {
        let mut seen = vec![false; map.tiles.len()];
        let mut found = Vec::new();
        for start in 0..map.tiles.len() {
            if seen[start] || !exists(start) || map.tiles[start].terrain.is_water() != water {
                continue;
            }
            let mut stack = vec![start];
            seen[start] = true;
            let mut size = 0;
            while let Some(i) = stack.pop() {
                size += 1;
                let (x, y) = ((i % side) as i32, (i / side) as i32);
                for (dx, dy) in [
                    (-1, -1),
                    (0, -1),
                    (1, -1),
                    (-1, 0),
                    (1, 0),
                    (-1, 1),
                    (0, 1),
                    (1, 1),
                ] {
                    if let Some(j) = at(x + dx, y + dy)
                        && !seen[j]
                        && map.tiles[j].terrain.is_water() == water
                    {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
            found.push((size, start % side, start / side));
        }
        found.sort_by(|a, b| b.cmp(a));
        found
    };
    let describe = |list: &[(usize, usize, usize)], limit: usize| {
        list.iter()
            .take(limit)
            .map(|(size, x, y)| format!("{size}@{x},{y}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let seas = label(true);
    let lands = label(false);
    vec![
        format!("water bodies ({}): {}", seas.len(), describe(&seas, 24)),
        format!("land masses ({}): {}", lands.len(), describe(&lands, 24)),
    ]
}
