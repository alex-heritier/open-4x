//! The barbarians (`reverse-engineering/barbarians.md`): the camps found
//! at map setup each start with two basic units (section 7), every round
//! the camps spawn land units under a cap (section 2), the world tops its
//! camps up one at a time (section 4), and dispersing a camp pays 25 gold
//! (section 8, in `features`).
//!
//! The barbarians play first in each round, as the executable's slot 0
//! does: when the round counter ticks, a barbarian phase holds the human's
//! input while their units move and fight through the ordinary mover and
//! combat sequencer.
//!
//! Adaptations: the clone's map is a plain grid rather than Civ3's
//! doubled-x diamond, so the "5x5 raw block" and the site predicate's
//! squares are Chebyshev squares of the same side; the start-location test
//! of the site predicate is covered by its unit test (the start tiles hold
//! the parties). HYPOTHESIS: how barbarian units choose their moves is not
//! decoded (section 10.1); here each one heads for the nearest city or unit
//! of a civilization within [`SIGHT`] tiles and attacks it, and the last
//! unit on a camp stays to guard it.

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;

use crate::cities::City;
use crate::civs::{BARBARIANS, civ_count, is_barbarian};
use crate::combat::{CombatRng, Level};
use crate::map::GameMap;
use crate::units::{Unit, UnitAnim, UnitArt, UnitType, def};

/// The resolved barbarian setting `S` (`[0x9C737C]`). 1 is the editor's
/// "roaming", the game's default (label mapping **H**, section 1.1).
pub fn activity() -> i32 {
    crate::scenario::barbarian_activity()
}

/// Players in play, `N` of section 2: the civilizations and the
/// barbarians.
fn n() -> i32 {
    civ_count() as i32 + 1
}

/// How far a barbarian looks for prey (HYPOTHESIS).
pub const SIGHT: i32 = 8;

/// RACE row 0's tribe names: five culture groups of fifteen, then the
/// default "Barbarian" (section 1.4).
pub const TRIBES: [&str; 76] = [
    "Chanca",
    "Lupaca",
    "Cherokee",
    "Anasazi",
    "Teoihuacan",
    "Olmec",
    "Zapotec",
    "Chehalis",
    "Chinook",
    "Apache",
    "Illinois",
    "Inuit",
    "Navajo",
    "Carib",
    "Saxon",
    "Vandal",
    "Goth",
    "Angle",
    "Magyar",
    "Khazak",
    "Iberian",
    "Bulgar",
    "Alemanni",
    "Burgundian",
    "Gepid",
    "Hun",
    "Jute",
    "Marcomanni",
    "Seljuk",
    "Phoenician",
    "Estruscan",
    "Illuryian",
    "Thracian",
    "Phrygian",
    "Gaul",
    "Minoan",
    "Mycenian",
    "Cimmerian",
    "Ligurian",
    "Numidian",
    "Patzinal",
    "Sarmatian",
    "Scythian",
    "Suren",
    "Assyrian",
    "Harappan",
    "Mauryan",
    "Parthian",
    "Harappan",
    "Nubian",
    "Sarbadar",
    "Bactrian",
    "Circassian",
    "Cuman",
    "Hurrian",
    "Kassite",
    "Bantu",
    "Khoisan",
    "Libyan",
    "Shangian",
    "Yayoi",
    "Zhou",
    "Ainu",
    "Polynesian",
    "Aryan",
    "Avar",
    "Ghuzz",
    "Hsung-Nu",
    "Kushans",
    "Yue-Chi",
    "Sakae",
    "Uzbek",
    "Tartar",
    "Toltec",
    "Kushite",
    "Barbarian",
];

/// The default tribe.
pub const DEFAULT_TRIBE: u8 = 75;

/// A barbarian unit's tribe (`unit +0x3C`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tribe(pub u8);

#[derive(Resource)]
pub struct Barbarians {
    /// The barbarians are moving: the human waits.
    pub phase: bool,
    /// Spawning and camp top-up ran for this phase.
    spawned: bool,
    /// Units already given their orders this phase.
    done: HashSet<Entity>,
    /// Tribe-name table (`0xA526C8`).
    used: [bool; 75],
    /// Tribe of each camp (`cell +0x18`).
    pub camps: HashMap<(i32, i32), u8>,
    /// Frames the phase has run, a guard against a stuck unit.
    frames: u32,
}

impl Default for Barbarians {
    fn default() -> Self {
        Barbarians {
            phase: false,
            spawned: false,
            done: HashSet::new(),
            used: [false; 75],
            camps: HashMap::new(),
            frames: 0,
        }
    }
}

/// What a saved game keeps of the barbarians: their camps and the tribe
/// names in use.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Saved {
    used: Vec<bool>,
    camps: Vec<((i32, i32), u8)>,
}

impl Barbarians {
    pub fn snapshot(&self) -> Saved {
        let mut camps: Vec<_> = self.camps.iter().map(|(&at, &t)| (at, t)).collect();
        camps.sort();
        Saved {
            used: self.used.to_vec(),
            camps,
        }
    }

    /// Back to a quiet start with the saved camps.
    pub fn restore(&mut self, saved: &Saved) {
        *self = Barbarians::default();
        for (i, &u) in saved.used.iter().enumerate().take(self.used.len()) {
            self.used[i] = u;
        }
        self.camps = saved.camps.iter().copied().collect();
    }
}

impl Barbarians {
    pub fn tribe_used(&self, t: usize) -> bool {
        self.used.get(t).copied().unwrap_or(false)
    }

    pub fn tribe_at(&self, tile: (i32, i32)) -> u8 {
        self.camps.get(&tile).copied().unwrap_or(DEFAULT_TRIBE)
    }

    /// Section 8 steps 3-4: the camp is gone and its name is free again.
    pub fn disperse(&mut self, tile: (i32, i32)) -> u8 {
        let tribe = self.camps.remove(&tile).unwrap_or(DEFAULT_TRIBE);
        if let Some(u) = self.used.get_mut(tribe as usize) {
            *u = false;
        }
        tribe
    }

    /// The round begins: the barbarians move first.
    pub fn begin_round(&mut self) {
        self.phase = true;
        self.spawned = false;
        self.done.clear();
        self.frames = 0;
    }
}

static UPRISING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// `0x55FD00` is due: the research step asks, [`uprising`] answers.
pub fn request_uprising() {
    UPRISING.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Section 5, the uprising: up to `N - 1 - E` new camps, then `8 S`
/// advanced units at every camp. The sea unit of each camp is not placed
/// (the barbarians do not sail yet).
#[allow(clippy::too_many_arguments)]
pub fn uprising(
    mut commands: Commands,
    mut barb: ResMut<Barbarians>,
    mut map: ResMut<GameMap>,
    art: Res<UnitArt>,
    mut rng: ResMut<CombatRng>,
    cities: Query<&City>,
    units: Query<(Entity, &Unit)>,
    civs: Res<crate::civs::Civilizations>,
    mut board: ResMut<crate::features::MessageBoard>,
) {
    if !UPRISING.swap(false, std::sync::atomic::Ordering::Relaxed) || activity() <= 0 {
        return;
    }
    let all: Vec<City> = cities.iter().cloned().collect();
    let refs: Vec<&City> = all.iter().collect();
    let snapshot: Vec<(Entity, Unit)> = units.iter().map(|(e, u)| (e, u.clone())).collect();
    let existing = map.tiles.iter().filter(|t| t.camp).count() as i32;
    for _ in 0..(n() - existing - 1).max(0) {
        found_camp(
            &mut commands,
            &art,
            &mut map,
            &mut barb,
            &mut rng,
            &refs,
            &snapshot,
        );
    }
    let camps: Vec<(i32, i32)> = (0..map.h)
        .flat_map(|y| (0..map.w).map(move |x| (x, y)))
        .filter(|&(x, y)| map.tiles[map.idx(x, y)].camp)
        .collect();
    for &tile in &camps {
        for _ in 0..8 * activity() {
            spawn(
                &mut commands,
                &art,
                crate::roles::barbarian_advanced(),
                tile,
                barb.tribe_at(tile),
            );
        }
    }
    // SUMMARY_BARBARIAN_EXPLOSION_CITY: the human's city nearest a camp.
    let viewer = civs.viewer();
    let map = &*map;
    let near = camps
        .iter()
        .flat_map(|&t| {
            refs.iter()
                .filter(|c| c.civ == viewer)
                .map(move |c| (map.distance(t, (c.x, c.y)), c.name.clone()))
        })
        .filter(|(d, _)| *d < 10)
        .min();
    if let Some((_, name)) = near {
        crate::features::post(
            &mut board,
            format!("We have heard reports of a massive barbarian uprising near {name}!"),
        );
    }
}

/// The human may act: no barbarian is moving.
pub fn idle(b: Res<Barbarians>) -> bool {
    !b.phase
}

/// Land units the camps spawn under the cap `2 (N-1) S`: the basic unit,
/// or the advanced one once the world holds more than `4N - 4` cities.
pub fn land_cap() -> i32 {
    2 * (n() - 1) * activity()
}

/// Section 2.1: no activity before the world holds `2N - 2` cities.
pub fn active(world_cities: i32) -> bool {
    activity() > 0 && world_cities >= 2 * n() - 2
}

/// Section 2.1 `many`: the advanced unit and the sea spawns.
pub fn many(world_cities: i32) -> bool {
    world_cities > 4 * n() - 4
}

/// The tribe a new camp takes (section 4): a free name of the nearest
/// civilization's culture group, starting at `s = rand(15)`, else the
/// default.
pub fn pick_tribe(used: &[bool; 75], group: usize, s: u32) -> u8 {
    (0..15)
        .map(|j| 15 * group + (s as usize + j) % 15)
        .find(|&t| !used[t])
        .map_or(DEFAULT_TRIBE, |t| t as u8)
}

/// Section 4.1's squares for a map of raw width `w` and height `h`: owned
/// tiles are forbidden within the first (side `2A + 3`), camps within the
/// second (side `2B + 5`). Returned as Chebyshev radii.
pub fn site_radii(w: i32, h: i32) -> (i32, i32) {
    let m = (w + h) / 2;
    let a = (m / 100).min(4);
    let b = (m / 25).min(4);
    (a + 1, b + 2)
}

pub(crate) fn spawn(
    commands: &mut Commands,
    art: &UnitArt,
    t: UnitType,
    tile: (i32, i32),
    tribe: u8,
) -> Entity {
    let e = crate::units::spawn_unit_at_level(
        commands,
        art,
        t,
        tile.0,
        tile.1,
        BARBARIANS,
        Level::Conscript,
    );
    commands.entity(e).insert(Tribe(tribe));
    e
}

/// Section 7: every camp on the new map gets the default tribe and two
/// basic units.
pub fn setup_camps(
    mut commands: Commands,
    map: Res<GameMap>,
    art: Res<UnitArt>,
    mut barb: ResMut<Barbarians>,
) {
    for y in 0..map.h {
        for x in 0..map.w {
            if map.tiles[map.idx(x, y)].camp {
                barb.camps.insert((x, y), DEFAULT_TRIBE);
                // A scenario places its own barbarians.
                for _ in 0..if crate::scenario::scenario().is_some() {
                    0
                } else {
                    2
                } {
                    spawn(
                        &mut commands,
                        &art,
                        crate::roles::barbarian_basic(),
                        (x, y),
                        DEFAULT_TRIBE,
                    );
                }
            }
        }
    }
}

/// Continent sizes by label.
fn continent_sizes(labels: &[u16]) -> HashMap<u16, i32> {
    let mut out = HashMap::new();
    for &l in labels {
        *out.entry(l).or_insert(0) += 1;
    }
    out
}

/// Section 4.1 for the clone's grid.
fn site_ok(
    map: &GameMap,
    labels: &[u16],
    sizes: &HashMap<u16, i32>,
    owners: &HashMap<(i32, i32), usize>,
    cities: &[&City],
    units: &[(Entity, Unit)],
    (x, y): (i32, i32),
) -> bool {
    let i = map.idx(x, y);
    let t = &map.tiles[i];
    if crate::improvements::is_water_base(t.base)
        || t.hut
        || cities.iter().any(|c| (c.x, c.y) == (x, y))
        || units.iter().any(|(_, u)| (u.x, u.y) == (x, y))
        || sizes.get(&labels[i]).copied().unwrap_or(0) < 75
    {
        return false;
    }
    let (owned_r, camp_r) = site_radii(2 * map.w, map.h);
    for dy in -camp_r..=camp_r {
        for dx in -camp_r..=camp_r {
            let (nx, ny) = (map.wrap_x(x + dx), y + dy);
            if ny < 0 || ny >= map.h {
                continue;
            }
            if dx.abs().max(dy.abs()) < owned_r && owners.contains_key(&(nx, ny)) {
                return false;
            }
            if map.tiles[map.idx(nx, ny)].camp {
                return false;
            }
        }
    }
    true
}

/// `0x55F9F0`: at most one new camp, `T / 16` attempts of `rand(T)`.
#[allow(clippy::too_many_arguments)]
fn found_camp(
    commands: &mut Commands,
    art: &UnitArt,
    map: &mut GameMap,
    barb: &mut Barbarians,
    rng: &mut CombatRng,
    cities: &[&City],
    units: &[(Entity, Unit)],
) -> Option<(i32, i32)> {
    let t = (map.w * map.h) as u32;
    let labels = crate::realm::continents(map);
    let sizes = continent_sizes(&labels);
    let owners = crate::cities::territory(map);
    for _ in 0..t / 16 {
        let r = rng.0.below(t) as i32;
        let (x, y) = (r % map.w, r / map.w);
        if y == 0
            || y >= map.h - 1
            || !site_ok(map, &labels, &sizes, &owners, cities, units, (x, y))
        {
            continue;
        }
        // No civilization's soldier within the 5x5 block (`0x56D340`).
        if units.iter().any(|(_, u)| {
            !is_barbarian(u.civ)
                && map.distance((u.x, u.y), (x, y)) <= 2
                && def(u.utype).attack + def(u.utype).defense > 0
        }) {
            continue;
        }
        // The continent needs a city; its owner names the tribe.
        let land = labels[map.idx(x, y)];
        let near = cities
            .iter()
            .filter(|c| labels[map.idx(c.x, c.y)] == land)
            .min_by_key(|c| map.distance((c.x, c.y), (x, y)))?;
        let i = map.idx(x, y);
        map.tiles[i].camp = true;
        let group = crate::civs::RACES
            .get(near.civ)
            .map_or(0, |r| r.culture_group as usize);
        let s = rng.0.below(15);
        let tribe = pick_tribe(&barb.used, group, s as u32);
        if let Some(u) = barb.used.get_mut(tribe as usize) {
            *u = true;
        }
        barb.camps.insert((x, y), tribe);
        for _ in 0..2 {
            spawn(
                commands,
                art,
                crate::roles::barbarian_basic(),
                (x, y),
                tribe,
            );
        }
        return Some((x, y));
    }
    None
}

/// The first frame of the phase: section 2.2 then 2.3.
#[allow(clippy::too_many_arguments)]
fn spawn_round(
    commands: &mut Commands,
    art: &UnitArt,
    map: &mut GameMap,
    barb: &mut Barbarians,
    rng: &mut CombatRng,
    cities: &[&City],
    units: &[(Entity, Unit)],
) {
    let world = cities.len() as i32;
    if !active(world) {
        return;
    }
    let many = many(world);
    let mut land = units
        .iter()
        .filter(|(_, u)| is_barbarian(u.civ) && def(u.utype).class == 0)
        .count() as i32;
    let mut camps = 0;
    let tiles: Vec<(i32, i32)> = (0..map.h)
        .flat_map(|y| (0..map.w).map(move |x| (x, y)))
        .collect();
    for tile in tiles {
        if !map.tiles[map.idx(tile.0, tile.1)].camp {
            continue;
        }
        camps += 1;
        if land < land_cap() && rng.0.below(8) == 0 {
            // The factory refuses a tile another civ's unit stands on.
            let blocked = units
                .iter()
                .any(|(_, u)| (u.x, u.y) == tile && !is_barbarian(u.civ));
            if !blocked {
                let t = if many {
                    crate::roles::barbarian_advanced()
                } else {
                    crate::roles::barbarian_basic()
                };
                spawn(commands, art, t, tile, barb.tribe_at(tile));
                land += 1;
            }
        }
        // Sea spawns (section 2.2) need Galleys the barbarians can sail and
        // land; HYPOTHESIS-free parts only: the draw is made.
        if many && 0 < (n() - 1) * activity() / 2 {
            let _ = rng.0.below(8);
        }
    }
    if camps < n() - 1 {
        found_camp(commands, art, map, barb, rng, cities, units);
    }
}

/// Where a barbarian goes this turn (HYPOTHESIS, module docs).
fn order(
    map: &GameMap,
    u: &Unit,
    cities: &[&City],
    units: &[(Entity, Unit)],
) -> Option<Vec<(i32, i32)>> {
    if def(u.utype).attack == 0 {
        return None;
    }
    let on_camp = map.get(u.x, u.y).is_some_and(|t| t.camp);
    let mates = units
        .iter()
        .filter(|(_, o)| is_barbarian(o.civ) && (o.x, o.y) == (u.x, u.y))
        .count();
    if on_camp && mates <= 1 {
        return None;
    }
    let prey = cities
        .iter()
        .map(|c| (c.x, c.y))
        .chain(
            units
                .iter()
                .filter(|(_, o)| !is_barbarian(o.civ) && o.carrier.is_none())
                .map(|(_, o)| (o.x, o.y)),
        )
        .filter(|&p| map.distance(p, (u.x, u.y)) <= SIGHT && map.is_land(p.0, p.1))
        .min_by_key(|&p| (map.distance(p, (u.x, u.y)), p));
    let target = prey?;
    crate::units::route(map, u, target, &[])
}

/// The barbarian phase, one frame at a time.
#[allow(clippy::too_many_arguments)]
pub fn play(
    mut commands: Commands,
    mut barb: ResMut<Barbarians>,
    mut map: ResMut<GameMap>,
    art: Res<UnitArt>,
    mut rng: ResMut<CombatRng>,
    cities: Query<&City>,
    mut units: Query<(Entity, &mut Unit)>,
    active_combat: Res<crate::combat::ActiveCombat>,
) {
    if !barb.phase {
        return;
    }
    barb.frames += 1;
    let all_cities: Vec<City> = cities.iter().cloned().collect();
    let city_refs: Vec<&City> = all_cities.iter().collect();
    let snapshot: Vec<(Entity, Unit)> = units.iter().map(|(e, u)| (e, u.clone())).collect();
    if !barb.spawned {
        barb.spawned = true;
        spawn_round(
            &mut commands,
            &art,
            &mut map,
            &mut barb,
            &mut rng,
            &city_refs,
            &snapshot,
        );
        if std::env::var("CIV3_AI_LOG").is_ok() {
            println!(
                "barbarians: {} camps, {} units, {} world cities",
                barb.camps.len(),
                snapshot.iter().filter(|(_, u)| is_barbarian(u.civ)).count(),
                city_refs.len()
            );
        }
        return; // the new units exist from the next frame
    }
    if active_combat.0.is_some() {
        return;
    }
    let mut busy = false;
    for (e, mut u) in units.iter_mut() {
        if !is_barbarian(u.civ) {
            continue;
        }
        if !matches!(u.anim, UnitAnim::Idle { .. }) || (!u.path.is_empty() && u.moves > 0) {
            busy = true;
            continue;
        }
        if u.moves == 0 || barb.done.contains(&e) {
            continue;
        }
        barb.done.insert(e);
        match order(&map, &u, &city_refs, &snapshot) {
            Some(path) if !path.is_empty() => {
                u.path = path.into();
                u.fortified = false;
                busy = true;
            }
            _ => u.fortified = true,
        }
    }
    // A unit that cannot finish its path (blocked) must not hold the game.
    if !busy || barb.frames > 3000 {
        for (_, mut u) in units.iter_mut() {
            if is_barbarian(u.civ) {
                u.path.clear();
            }
        }
        barb.phase = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caps_and_gates_follow_the_worked_table() {
        // `barbarians.md` 2.4, N = 5, S = 1.
        assert_eq!(land_cap(), 8);
        assert!(!active(7));
        assert!(active(8));
        assert!(!many(16));
        assert!(many(17));
    }

    #[test]
    fn tribes_take_the_first_free_name_of_the_group() {
        let mut used = [false; 75];
        assert_eq!(pick_tribe(&used, 4, 3), 63);
        used[63] = true;
        assert_eq!(pick_tribe(&used, 4, 3), 64);
        assert_eq!(pick_tribe(&used, 4, 14), 74);
        used[60..75].iter_mut().for_each(|u| *u = true);
        assert_eq!(pick_tribe(&used, 4, 0), DEFAULT_TRIBE);
        assert_eq!(TRIBES[60], "Yayoi");
    }

    #[test]
    fn site_squares_follow_section_4_1() {
        // m = (W + H) / 2 with W the raw width.
        assert_eq!(site_radii(24, 24), (1, 2));
        assert_eq!(site_radii(160, 60), (2, 6));
        assert_eq!(site_radii(1000, 1000), (5, 6));
    }
}
