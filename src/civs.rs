//! Local hotseat civilizations. Every slot is controlled by the player.
use bevy::prelude::*;

use std::collections::VecDeque;

use crate::map::{GameMap, move_cost};

pub const CIV_COUNT: usize = 4;

pub struct CivDefinition {
    pub name: &'static str,
    pub adjective: &'static str,
    pub color: Color,
    pub city_names: &'static [&'static str],
}

pub const CIVS: [CivDefinition; CIV_COUNT] = [
    CivDefinition {
        name: "Japan",
        adjective: "Japanese",
        color: Color::srgb_u8(25, 148, 24),
        city_names: &[
            "Kyoto",
            "Osaka",
            "Tokyo",
            "Edo",
            "Nagoya",
            "Kobe",
            "Yokohama",
            "Hiroshima",
            "Nagasaki",
            "Nara",
            "Sapporo",
            "Sendai",
            "Niigata",
            "Okayama",
            "Fukuoka",
            "Kagoshima",
            "Matsuyama",
            "Kanazawa",
            "Takamatsu",
            "Oita",
        ],
    },
    CivDefinition {
        name: "Rome",
        adjective: "Roman",
        color: Color::srgb_u8(190, 48, 48),
        city_names: &[
            "Rome", "Veii", "Antium", "Cumae", "Neapolis", "Pompeii", "Pisae", "Ravenna",
        ],
    },
    CivDefinition {
        name: "Egypt",
        adjective: "Egyptian",
        color: Color::srgb_u8(220, 184, 48),
        city_names: &[
            "Thebes",
            "Memphis",
            "Heliopolis",
            "Elephantine",
            "Alexandria",
            "Pi-Ramesses",
            "Giza",
            "Byblos",
        ],
    },
    // The BIQ gives the Chinese default color 5 (cyan, `ntp05`); this is that
    // hue toned down like Rome's and Egypt's. City list is the BIQ's, in order.
    CivDefinition {
        name: "China",
        adjective: "Chinese",
        color: Color::srgb_u8(40, 180, 190),
        city_names: &[
            "Beijing", "Shanghai", "Canton", "Nanking", "Tsingtao", "Xinjian", "Chengdu",
            "Hangchow", "Tientsin", "Tatung", "Macao", "Anyang", "Shantung", "Chinan", "Kaifeng",
            "Ningpo", "Paoting", "Yangchow",
        ],
    },
];

/// Which civilizations the computer plays, one bit per `CIVS` index. A
/// process-wide setting rather than a resource because the rules code that
/// has to know (a city's fog check) is plain functions without ECS access.
#[cfg(not(test))]
static AI_MASK: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

// Tests run in parallel threads and some choose their own controllers, so
// under test each thread has a mask of its own.
#[cfg(test)]
thread_local! {
    static AI_MASK: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

fn ai_mask() -> u8 {
    #[cfg(not(test))]
    return AI_MASK.load(std::sync::atomic::Ordering::Relaxed);
    #[cfg(test)]
    return AI_MASK.get();
}

fn set_ai_mask(mask: u8) {
    #[cfg(not(test))]
    AI_MASK.store(mask, std::sync::atomic::Ordering::Relaxed);
    #[cfg(test)]
    AI_MASK.set(mask);
}

/// The computer plays this civilization.
pub fn is_ai(civ: usize) -> bool {
    ai_mask() & (1 << civ) != 0
}

/// Choose who plays whom: Japan is the human and everyone else is the
/// computer, unless `CIV3_HOTSEAT` asks for the old four-chair sandbox or
/// `CIV3_AUTOPLAY` for a game the computer plays alone.
pub fn set_controllers() {
    let mask = if std::env::var("CIV3_HOTSEAT").is_ok() {
        0
    } else if std::env::var("CIV3_AUTOPLAY").is_ok() {
        // A spectator game: the computer plays every civilization.
        (1u8 << CIV_COUNT) - 1
    } else {
        (1u8 << CIV_COUNT) - 2
    };
    set_ai_mask(mask);
}

/// `CIV3_AI_FAST=1`: the computer's moves and fights are never animated,
/// even in the human's sight. For unattended runs.
pub fn ai_fast() -> bool {
    static FAST: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FAST.get_or_init(|| std::env::var("CIV3_AI_FAST").is_ok())
}

/// How the game ended.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    /// Only this civilization is left.
    Victory(usize),
    /// Every human civilization is gone.
    Defeat,
}

#[derive(Resource, Default)]
pub struct Civilizations {
    /// The civilization whose turn it is.
    pub active: usize,
    /// The last human to hold the chair: while the computer plays, the
    /// screen keeps showing this civilization's view.
    pub last_human: usize,
    /// Civilizations that lost their last city and settler.
    pub eliminated: [bool; CIV_COUNT],
    pub outcome: Option<Outcome>,
}

impl Civilizations {
    /// Whose fog and treasury the screen shows.
    pub fn viewer(&self) -> usize {
        self.viewer_with(is_ai)
    }

    fn viewer_with(&self, ai: impl Fn(usize) -> bool) -> usize {
        if ai(self.active) {
            self.last_human
        } else {
            self.active
        }
    }

    /// The civilization after `active` that is still in the game.
    pub fn next_active(&self) -> usize {
        (1..=CIV_COUNT)
            .map(|k| (self.active + k) % CIV_COUNT)
            .find(|&c| !self.eliminated[c])
            .unwrap_or(self.active)
    }
}

/// Whether the game is decided: the humans are all gone (a defeat), or one
/// civilization stands alone (its victory).
pub fn outcome_of(eliminated: &[bool; CIV_COUNT], ai: impl Fn(usize) -> bool) -> Option<Outcome> {
    let alive: Vec<usize> = (0..CIV_COUNT).filter(|&c| !eliminated[c]).collect();
    let humans = (0..CIV_COUNT).any(|c| !ai(c));
    if humans && !alive.iter().any(|&c| !ai(c)) {
        Some(Outcome::Defeat)
    } else if alive.len() == 1 {
        Some(Outcome::Victory(alive[0]))
    } else {
        None
    }
}

/// A civilization with no city and no Settler is out of the game: its
/// remaining units disband. The game ends when no human is left, or when
/// one civilization stands alone.
pub fn check_elimination(
    mut commands: Commands,
    mut civs: ResMut<Civilizations>,
    units: Query<(Entity, &crate::units::Unit)>,
    cities: Query<&crate::cities::City>,
    mut board: ResMut<crate::features::MessageBoard>,
    mut strikes: Local<[u8; CIV_COUNT]>,
) {
    if civs.outcome.is_some() {
        return;
    }
    for civ in 0..CIV_COUNT {
        if civs.eliminated[civ] {
            continue;
        }
        let rooted = cities.iter().any(|c| c.civ == civ)
            || units
                .iter()
                .any(|(_, u)| u.civ == civ && u.utype == crate::units::UnitType::Settler);
        // A founding or a capture lands over a frame or two; only a civ
        // that stays empty is gone.
        strikes[civ] = if rooted { 0 } else { strikes[civ].saturating_add(1) };
        if strikes[civ] < 3 {
            continue;
        }
        civs.eliminated[civ] = true;
        for (e, u) in &units {
            if u.civ == civ {
                commands.entity(e).despawn();
            }
        }
        crate::features::post(
            &mut board,
            format!("The {} have been destroyed!", CIVS[civ].name),
        );
        if std::env::var("CIV3_AI_LOG").is_ok() {
            println!("ai: the {} are eliminated", CIVS[civ].name);
        }
    }
    civs.outcome = outcome_of(&civs.eliminated, is_ai);
    if civs.outcome.is_some() && std::env::var("CIV3_AI_LOG").is_ok() {
        println!("ai: game over: {:?}", civs.outcome);
    }
}

/// Emitted once for each outgoing civilization, before the next player acts.
#[derive(Message)]
pub struct CivilizationEnded(pub usize);

/// Handoffs move the view to the incoming player's unit or capital.
pub fn focus_active_civ(
    civs: Res<Civilizations>,
    selected: Res<crate::units::Selected>,
    units: Query<&crate::units::Unit>,
    cities: Query<&crate::cities::City>,
    mut camera: Query<&mut Transform, With<Camera2d>>,
    mut previous: Local<usize>,
) {
    // The camera follows the human's chair only: the computer's turns play
    // out wherever they are.
    let viewer = civs.viewer();
    if *previous == viewer {
        return;
    }
    *previous = viewer;
    let position = selected
        .0
        .and_then(|e| units.get(e).ok())
        .map(|u| (u.x, u.y))
        .or_else(|| {
            cities
                .iter()
                .find(|c| c.civ == viewer)
                .map(|c| (c.x, c.y))
        })
        .or_else(|| {
            units
                .iter()
                .find(|u| u.civ == viewer)
                .map(|u| (u.x, u.y))
        });
    if let (Some((x, y)), Ok(mut transform)) = (position, camera.single_mut()) {
        let world = crate::map::tile_to_world(x, y);
        transform.translation.x = world.x;
        transform.translation.y = world.y;
    }
}

/// The preset scenario crowds everyone around Japan: the other civilizations
/// start within this many tiles (king moves) of Japan's capital site.
pub const START_RADIUS: i32 = 10;

/// No two starts closer than this, so every civ has room for a first city.
const MIN_START_SPACING: i32 = 4;

/// Where Rome, Egypt, and China would like to start, as offsets from Japan's
/// start (`CIVS[1..]` in order): about 7 tiles out, spread evenly around it.
const NEIGHBOR_OFFSETS: [(i32, i32); CIV_COUNT - 1] = [(-6, 4), (0, -7), (6, 4)];

/// Capital sites. Japan keeps `map.start`; each other civ takes the open site
/// nearest its preferred spot in `NEIGHBOR_OFFSETS`. No ships exist, so a site
/// must be walkable from Japan's. Terrain that can't be settled, huts, camps
/// (units pop them on the spot), and sites crowding another start are out.
/// A site beyond `START_RADIUS` or on poor ground is taken only when
/// nothing better is left, so a cramped map degrades instead of failing.
pub fn starting_positions(map: &GameMap) -> [(i32, i32); CIV_COUNT] {
    let mut starts = [map.start; CIV_COUNT];
    let walkable = walkable_from(map, map.start);
    for civ in 1..CIV_COUNT {
        let (dx, dy) = NEIGHBOR_OFFSETS[civ - 1];
        let wanted = (map.start.0 + dx, map.start.1 + dy);
        starts[civ] = (0..map.h)
            .flat_map(|y| (0..map.w).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                let t = &map.tiles[map.idx(x, y)];
                walkable[map.idx(x, y)]
                    && !t.hut
                    && !t.camp
                    && crate::cities::can_found(map, &starts[..civ], x, y)
                    && starts[..civ]
                        .iter()
                        .all(|&s| map.distance(s, (x, y)) >= MIN_START_SPACING)
            })
            .min_by_key(|&site| {
                (
                    map.distance(map.start, site) > START_RADIUS,
                    !map.tiles[map.idx(site.0, site.1)].good_start(),
                    // King-move distance leaves ring-shaped ties that scan
                    // order would break toward the north-west.
                    straight_distance_sq(map, wanted, site),
                )
            })
            .expect("map must have room for all civilizations");
    }
    starts
}

/// Squared straight-line distance in tiles, wrapping in x.
fn straight_distance_sq(map: &GameMap, a: (i32, i32), b: (i32, i32)) -> i32 {
    let dx = (map.wrap_x(a.0) - map.wrap_x(b.0)).abs();
    let dx = dx.min(map.w - dx);
    let dy = a.1 - b.1;
    dx * dx + dy * dy
}

/// Tiles a land unit can walk to from `from`.
fn walkable_from(map: &GameMap, from: (i32, i32)) -> Vec<bool> {
    let mut reached = vec![false; map.tiles.len()];
    reached[map.idx(from.0, from.1)] = true;
    let mut queue = VecDeque::from([from]);
    while let Some((x, y)) = queue.pop_front() {
        for (nx, ny) in map.neighbors(x, y) {
            let i = map.idx(nx, ny);
            if !reached[i] && move_cost(&map.tiles[i]).is_some() {
                reached[i] = true;
                queue.push_back((nx, ny));
            }
        }
    }
    reached
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roster_is_japan_rome_egypt_china_with_distinct_colors() {
        let names: Vec<_> = CIVS.iter().map(|c| c.name).collect();
        assert_eq!(names, ["Japan", "Rome", "Egypt", "China"]);
        for (i, a) in CIVS.iter().enumerate() {
            assert!(!a.city_names.is_empty(), "{} has no city names", a.name);
            for b in &CIVS[i + 1..] {
                assert_ne!(a.color, b.color, "{} and {} share a color", a.name, b.name);
            }
        }
    }

    /// Sites every start must satisfy, whatever the seed.
    fn assert_sound_starts(map: &GameMap, starts: &[(i32, i32); CIV_COUNT], what: &str) {
        assert_eq!(starts[0], map.start, "{what}: Japan keeps the map's start");
        for (civ, &(x, y)) in starts.iter().enumerate() {
            let t = map.get(x, y).unwrap();
            assert!(
                !t.hut && !t.camp,
                "{what}: {} starts on a hut or camp",
                CIVS[civ].name
            );
            if civ > 0 {
                assert!(
                    map.find_path(map.start, (x, y)).is_some(),
                    "{what}: {} cannot walk to Japan",
                    CIVS[civ].name
                );
            }
            for (other, &o) in starts.iter().enumerate().skip(civ + 1) {
                assert!(
                    map.distance((x, y), o) >= MIN_START_SPACING,
                    "{what}: {} and {} start too close",
                    CIVS[civ].name,
                    CIVS[other].name
                );
            }
        }
    }

    /// The preset scenario: Rome, Egypt, and China start on good ground
    /// within ten tiles of Japan.
    #[test]
    fn default_map_puts_everyone_within_ten_tiles_of_japan() {
        let map = GameMap::generate();
        let starts = starting_positions(&map);
        assert_sound_starts(&map, &starts, "default map");
        for civ in 1..CIV_COUNT {
            let d = map.distance(map.start, starts[civ]);
            assert!(d <= 10, "{} starts {d} tiles from Japan", CIVS[civ].name);
            let (x, y) = starts[civ];
            assert!(
                map.get(x, y).unwrap().good_start(),
                "{} starts on poor ground",
                CIVS[civ].name
            );
        }
    }

    #[test]
    fn starts_stay_close_and_sound_across_seeds() {
        for seed in 0..40 {
            let map = GameMap::generate_with_seed(seed);
            let starts = starting_positions(&map);
            assert_sound_starts(&map, &starts, &format!("seed {seed}"));
            for civ in 1..CIV_COUNT {
                let d = map.distance(map.start, starts[civ]);
                assert!(
                    d <= START_RADIUS,
                    "seed {seed}: {} starts {d} tiles away",
                    CIVS[civ].name
                );
            }
        }
    }

    #[test]
    fn the_turn_order_skips_civilizations_that_are_gone() {
        let mut civs = Civilizations::default();
        assert_eq!(civs.next_active(), 1);
        civs.eliminated[1] = true;
        assert_eq!(civs.next_active(), 2);
        civs.active = 3;
        civs.eliminated[0] = true;
        assert_eq!(civs.next_active(), 2);
        // Alone, a civ is its own successor.
        civs.active = 2;
        civs.eliminated = [true, true, false, true];
        assert_eq!(civs.next_active(), 2);
    }

    #[test]
    fn the_screen_stays_with_the_last_human_while_the_computer_plays() {
        let ai = |c: usize| c != 0;
        let mut civs = Civilizations::default();
        assert_eq!(civs.viewer_with(ai), 0);
        civs.active = 2;
        assert_eq!(civs.viewer_with(ai), 0, "Egypt's turn is watched as Japan");
        // Hotseat: every chair is human, so the view follows the turn.
        assert_eq!(civs.viewer_with(|_| false), 2);
    }

    #[test]
    fn the_game_ends_when_the_human_falls_or_one_civ_is_left() {
        let ai = |c: usize| c != 0;
        assert_eq!(outcome_of(&[false; CIV_COUNT], ai), None);
        assert_eq!(
            outcome_of(&[true, false, false, false], ai),
            Some(Outcome::Defeat)
        );
        assert_eq!(
            outcome_of(&[false, true, true, true], ai),
            Some(Outcome::Victory(0))
        );
        // Computer civs fighting on do not end the game while Japan stands.
        assert_eq!(outcome_of(&[false, true, false, true], ai), None);
        // Four chairs of humans: the last one standing wins.
        assert_eq!(
            outcome_of(&[true, true, false, true], |_| false),
            Some(Outcome::Victory(2))
        );
        // A game with no humans ends only on a lone survivor.
        assert_eq!(outcome_of(&[true, false, false, false], |_| true), None);
        assert_eq!(
            outcome_of(&[true, true, false, true], |_| true),
            Some(Outcome::Victory(2))
        );
    }

    #[test]
    fn a_civ_with_no_city_and_no_settler_is_eliminated_and_its_units_go() {
        use crate::cities::City;
        use crate::units::{Unit, UnitType};
        let mut app = App::new();
        app.init_resource::<Civilizations>();
        app.init_resource::<crate::features::MessageBoard>();
        app.add_systems(Update, check_elimination);
        let settler = |civ| Unit::new(civ, UnitType::Settler, civ as i32 * 5, 0);
        app.world_mut().spawn(settler(0));
        let doomed_settler = app.world_mut().spawn(settler(1)).id();
        let straggler = app
            .world_mut()
            .spawn(Unit::new(1, UnitType::Warrior, 6, 0))
            .id();
        // Civ 2 holds a city and no units; civ 3 has nothing at all.
        app.world_mut().spawn(City {
            civ: 2,
            name: "Thebes".into(),
            x: 10,
            y: 0,
            size: 1,
            food: 0,
            shields: 0,
            production: crate::cities::Production::Warrior,
            queue: vec![],
            buildings: vec![],
            worked: Default::default(),
            culture: 0,
            founded: 1,
        });
        for _ in 0..2 {
            app.update();
        }
        let civs = app.world().resource::<Civilizations>();
        assert_eq!(civs.eliminated, [false; CIV_COUNT], "grace period");
        app.update();
        let civs = app.world().resource::<Civilizations>();
        assert_eq!(civs.eliminated, [false, false, false, true]);
        assert_eq!(civs.outcome, None);
        // Civ 1 loses its settler: its warrior disbands with it.
        app.world_mut().despawn(doomed_settler);
        for _ in 0..4 {
            app.update();
        }
        assert!(app.world().get_entity(straggler).is_err());
        let civs = app.world().resource::<Civilizations>();
        assert_eq!(civs.eliminated, [false, true, false, true]);
        assert_eq!(civs.outcome, None);
    }
}
