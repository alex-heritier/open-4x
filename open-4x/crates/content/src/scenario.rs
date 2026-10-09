//! Scenario document validation. Every cross-reference, bound, and invariant the simulation
//! relies on is checked here, so a bad scenario is rejected with a precise message at load time
//! and never reaches the simulation.
use crate::ContentError;
use fourx_sim::{Domain, Rules, SCENARIO_FORMAT, Scenario, Status, terrain::Coord};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

pub const MAX_NATIONS: usize = 250;
pub const MAX_REGIONS: usize = 1024;
pub const MAX_CITIES: usize = 4096;
pub const MAX_UNITS: usize = 16384;
/// Most units that may start stacked on one square.
pub const MAX_STACK: usize = 64;
pub const MIN_DIMENSION: i32 = 8;

/// 1–48 characters of `a-z`, `0-9`, and interior `-`.
pub fn valid_identifier(id: &str) -> bool {
    (1..=48).contains(&id.len())
        && !id.starts_with('-')
        && !id.ends_with('-')
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn text_ok(text: &str, max: usize) -> bool {
    text.chars().count() <= max && !text.chars().any(char::is_control)
}

pub fn validate_scenario(s: &Scenario, rules: &Rules) -> Result<(), ContentError> {
    let fail = |message: String| -> Result<(), ContentError> {
        Err(ContentError::Invalid(format!(
            "scenario {:?}: {message}",
            s.id
        )))
    };
    if s.format != SCENARIO_FORMAT {
        return fail(format!(
            "unsupported format {} (expected {SCENARIO_FORMAT})",
            s.format
        ));
    }
    if !valid_identifier(&s.id) {
        return fail("id must be 1–48 characters of a-z, 0-9, and '-'".into());
    }
    if s.name.trim().is_empty() || !text_ok(&s.name, 96) {
        return fail("name must be 1–96 characters".into());
    }
    if !text_ok(&s.description, 4000) || !text_ok(&s.intro, 600) {
        return fail("description is limited to 4000 characters and intro to 600".into());
    }
    for (label, value, range) in [
        ("victory_industry", s.rules.victory_industry, 1..=1_000_000),
        ("research_cost", s.rules.research_cost, 1..=100_000),
        ("starting_gold", s.rules.starting_gold, 0..=1_000_000),
    ] {
        if value.is_some_and(|v| !range.contains(&v)) {
            return fail(format!("rules.{label} is outside {range:?}"));
        }
    }

    let map = &s.map;
    if map.width < MIN_DIMENSION || map.height < MIN_DIMENSION {
        return fail(format!(
            "map must be at least {MIN_DIMENSION}×{MIN_DIMENSION} tiles"
        ));
    }

    // Nations.
    if s.nations.is_empty() || s.nations.len() > MAX_NATIONS {
        return fail(format!("needs 1–{MAX_NATIONS} nations"));
    }
    let mut nations: BTreeMap<&str, usize> = BTreeMap::new();
    for (i, n) in s.nations.iter().enumerate() {
        if !valid_identifier(&n.id) || nations.insert(&n.id, i).is_some() {
            return fail(format!("nation id {:?} is invalid or duplicated", n.id));
        }
        if n.name.trim().is_empty()
            || !text_ok(&n.name, 96)
            || !text_ok(&n.adjective, 48)
            || !text_ok(&n.government, 96)
            || !text_ok(&n.leader, 96)
            || !text_ok(&n.notes, 1000)
        {
            return fail(format!(
                "nation {:?} has an empty or oversized text field",
                n.id
            ));
        }
        if n.gold.is_some_and(|g| !(0..=1_000_000).contains(&g)) || n.technology > 100 {
            return fail(format!(
                "nation {:?} has a treasury or technology out of range",
                n.id
            ));
        }
        if (n.status == Status::Dependent) != n.suzerain.is_some() {
            return fail(format!(
                "nation {:?} must name a suzerain exactly when its status is dependent",
                n.id
            ));
        }
    }
    for n in &s.nations {
        if let Some(lord) = &n.suzerain {
            // Following suzerains must terminate; a nation may not be its own overlord.
            let mut seen = BTreeSet::from([n.id.as_str()]);
            let mut cursor = lord.as_str();
            loop {
                let Some(&index) = nations.get(cursor) else {
                    return fail(format!("nation {:?} names unknown suzerain {lord:?}", n.id));
                };
                if !seen.insert(cursor) {
                    return fail(format!("suzerainty of {:?} is circular", n.id));
                }
                match &s.nations[index].suzerain {
                    Some(next) => cursor = next,
                    None => break,
                }
            }
        }
    }
    if !nations.contains_key(s.commander.as_str()) {
        return fail(format!("commander {:?} is not a nation", s.commander));
    }

    // Regions and the region layer.
    if s.regions.len() > MAX_REGIONS {
        return fail(format!("at most {MAX_REGIONS} regions"));
    }
    let mut region_ids = HashSet::new();
    for r in &s.regions {
        if !valid_identifier(&r.id) || !region_ids.insert(r.id.as_str()) {
            return fail(format!("region id {:?} is invalid or duplicated", r.id));
        }
        if r.name.trim().is_empty()
            || !text_ok(&r.name, 96)
            || !nations.contains_key(r.nation.as_str())
        {
            return fail(format!(
                "region {:?} has a bad name or unknown nation",
                r.id
            ));
        }
    }
    if let Some(tile) = map
        .tiles
        .iter()
        .find(|t| usize::from(t.region) > s.regions.len() || t.owner != 0 || t.claim != 0)
    {
        return fail(format!(
            "tile {},{} has an unknown region, an owner, or a city claim (territory derives from cities)",
            tile.position.x, tile.position.y
        ));
    }

    // Cities.
    if s.cities.len() > MAX_CITIES {
        return fail(format!("at most {MAX_CITIES} cities"));
    }
    let mut city_positions = HashSet::new();
    let mut with_city: HashMap<&str, usize> = HashMap::new();
    let mut capitals: HashMap<&str, usize> = HashMap::new();
    for c in &s.cities {
        let Some(tile) = map.get(c.position) else {
            return fail(format!("city {:?} lies outside the map", c.name));
        };
        if !nations.contains_key(c.nation.as_str())
            || c.name.trim().is_empty()
            || !text_ok(&c.name, 64)
            || !(1..=100).contains(&c.population)
            || !(1..=100).contains(&c.industry)
            || c.border.is_some_and(|b| !(1..=6).contains(&b))
        {
            return fail(format!("city {:?} is malformed", c.name));
        }
        if tile.is_water() {
            return fail(format!("city {:?} is on water", c.name));
        }
        if !city_positions.insert(c.position) {
            return fail(format!("city {:?} shares a tile with another city", c.name));
        }
        *with_city.entry(&c.nation).or_default() += 1;
        if c.capital {
            *capitals.entry(&c.nation).or_default() += 1;
        }
    }
    for n in &s.nations {
        if !with_city.contains_key(n.id.as_str()) {
            return fail(format!(
                "nation {:?} needs at least one starting city",
                n.id
            ));
        }
        if capitals.get(n.id.as_str()).is_some_and(|c| *c > 1) {
            return fail(format!("nation {:?} has more than one capital", n.id));
        }
    }

    // Units.
    if s.units.len() > MAX_UNITS {
        return fail(format!("at most {MAX_UNITS} units"));
    }
    let mut stacks: HashMap<Coord, (usize, &str)> = HashMap::new();
    for u in &s.units {
        let Some(tile) = map.get(u.position) else {
            return fail(format!("a {:?} unit lies outside the map", u.kind));
        };
        let Some(design) = rules.units.get(&u.kind) else {
            return fail(format!("unit design {:?} does not exist", u.kind));
        };
        if !nations.contains_key(u.nation.as_str()) || u.level.is_some_and(|l| l > 3) {
            return fail(format!(
                "a {:?} unit has an unknown nation or level",
                u.kind
            ));
        }
        // Land units start on land and ships on water, or in a coastal city of their own nation.
        let in_port = tile.is_land()
            && s.cities
                .iter()
                .any(|c| c.position == u.position && c.nation == u.nation)
            && map
                .neighbors(u.position)
                .into_iter()
                .any(|p| map.get(p).is_some_and(|t| !t.is_land()));
        let misplaced = match design.domain {
            Domain::Sea => tile.is_land() && !in_port,
            Domain::Land => !tile.is_land(),
        };
        if misplaced {
            return fail(format!(
                "a {:?} unit at {},{} must start on {}",
                u.kind,
                u.position.x,
                u.position.y,
                if design.domain == Domain::Sea {
                    "water or in one of its nation's coastal cities"
                } else {
                    "land"
                }
            ));
        }
        if u.fortified && (design.domain != Domain::Land || design.defense <= 0) {
            return fail(format!("a {:?} unit cannot start fortified", u.kind));
        }
        // Nations never share a square. A city's square belongs to the city's nation.
        let entry = stacks.entry(u.position).or_insert((0, &u.nation));
        entry.0 += 1;
        if entry.1 != u.nation || entry.0 > MAX_STACK {
            return fail(format!(
                "square {},{} holds units of two nations or more than {MAX_STACK} units",
                u.position.x, u.position.y
            ));
        }
    }
    for c in &s.cities {
        if stacks
            .get(&c.position)
            .is_some_and(|(_, nation)| *nation != c.nation)
        {
            return fail(format!("city {:?} holds another nation's units", c.name));
        }
    }

    // Wars.
    let mut wars = HashSet::new();
    for w in &s.wars {
        if !nations.contains_key(w.a.as_str())
            || !nations.contains_key(w.b.as_str())
            || w.a == w.b
            || !text_ok(&w.name, 96)
        {
            return fail(format!("war {:?} vs {:?} is malformed", w.a, w.b));
        }
        let key = if w.a < w.b {
            (&w.a, &w.b)
        } else {
            (&w.b, &w.a)
        };
        if !wars.insert(key) {
            return fail(format!(
                "war between {:?} and {:?} is listed twice",
                w.a, w.b
            ));
        }
    }
    Ok(())
}
