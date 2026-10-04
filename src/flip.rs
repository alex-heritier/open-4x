//! The culture flip: a city whose people and neighbours belong to another
//! civ may defect to it, and the transfer shared with military capture.
//!
//! Rules and addresses: `reverse-engineering/borders-culture.md` section 9
//! (the flip test `0x4B28D0`, golden vectors F1-F9), `capture.md` sections
//! 6-8 (the conversion branch and the transfer `0x564800`). The pure rules
//! are plain functions with tests; [`run`] applies them once per city turn.
//!
//! Deviations, marked **HYPOTHESIS**: the clone has no resisting citizens
//! (`B` = 0) and no celebration; a citizen's race is the owner's unless the
//! city was taken or converted, which `City::foreign` records; the empire
//! culture total of `0x4F8E20` is summed here from the city culture added
//! each turn; a unit may be quelled one citizen at a time (`DIFF +0x78`
//! defaults to 1); the grid distance of `0x4B28D0` is read on the doubled
//! isometric grid.

use bevy::prelude::*;
use civ3mapgen::capture::{AcceptFacts, Wonder, ai_accepts_city};

use crate::cities::{self, Capital, City, Production, radius_tiles, territory};
use crate::civs::{CIV_COUNT, CivilizationEnded, is_ai};
use crate::combat::CombatRng;
use crate::diplomacy::{ConvertAsk, Diplomacy};
use crate::features::{MessageBoard, post};
use crate::map::GameMap;
use crate::roster::{imp, oth};
use crate::units::{Unit, def};

/// The single-precision constant at `0x666AB8`, widened: 1.39999997615814.
const LOST_CAPITAL_FACTOR: f64 = 1.399_999_976_158_14;

/// What the flip test reads about one rival civ `j` and a city of owner `o`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Contender {
    /// Citizens of `j`'s race (`A`).
    pub nationals: i32,
    /// Resisting citizens of `j`'s race, counted a second time (`B`).
    pub resisters: i32,
    /// Of the 20 tiles around the city, those `j` owns (`C`).
    pub tiles: i32,
    /// The city is in disorder (`City +0x30` bit 0).
    pub disorder: bool,
    /// The city is celebrating (bit 1).
    pub celebrating: bool,
    /// `j` has the larger culture stock in the city.
    pub larger_stake: bool,
    /// `Player +0x183C` of the owner and of `j`.
    pub owner_rating: i32,
    pub rival_rating: i32,
    /// Martial-law units on the city tile times `citizens_quelled_by_military`.
    pub quelled: i32,
}

/// `S` of the flip test after every deduction (`borders-culture.md` 9); a
/// result of 0 or less means no roll is made for this civ.
pub fn strength(c: &Contender) -> i32 {
    let t = c.nationals + c.resisters;
    let mut s = t + (c.tiles - 2).max(0);
    if s >= 10 {
        s = 10;
    } else if s <= 0 {
        return 0;
    }
    if c.disorder {
        s *= 2;
    }
    if c.larger_stake {
        s *= 2;
    }
    let (ro, rj) = (c.owner_rating + 1, c.rival_rating + 1);
    if rj > 4 * ro {
        s *= 4;
    } else {
        s = s * rj / ro;
    }
    if c.celebrating {
        s /= 2;
    }
    s - c.quelled
}

/// The factor 1.4 applied for the round after a civ loses its capital.
pub fn capital_lost_boost(s: i32) -> i32 {
    (f64::from(s) * LOST_CAPITAL_FACTOR) as i32
}

/// `D`, the size of the die: 2000, scaled by how much nearer the rival's
/// capital is than the owner's (`500..=8000`).
pub fn denominator(to_rival_capital: Option<i32>, to_own_capital: Option<i32>) -> i32 {
    match (to_rival_capital, to_own_capital) {
        (Some(rival), Some(own)) if own > 0 => (2000 * rival / own).clamp(500, 8000),
        _ => 2000,
    }
}

/// The game's own distance between two cities (`borders-culture.md` 9):
/// not the move count. The clone's squares are put on the doubled grid the
/// executable uses (`x * 2 + (y & 1)`), wrapping in x.
pub fn dist(map: &GameMap, a: (i32, i32), b: (i32, i32)) -> i32 {
    let wx = 2 * map.w;
    let ax = 2 * map.wrap_x(a.0) + (a.1 & 1);
    let bx = 2 * map.wrap_x(b.0) + (b.1 & 1);
    let mut dx = (ax - bx).abs();
    if dx > wx / 2 {
        dx = wx - dx;
    }
    let dy = (a.1 - b.1).abs();
    let (mx, mn) = (dx.max(dy), dx.min(dy));
    mx - (((dx + dy) / 2 - mn + 1) / 2)
}

/// A building the transfer never destroys: great wonders and the size
/// gates stay; ordinary means none of Palace, wonder, gate (`0x4B3290`).
fn ordinary(p: Production) -> bool {
    p.bldg().is_some_and(|b| {
        b.other & (oth::WONDER | oth::SMALL_WONDER) == 0
            && b.flags & (imp::CENTER_OF_EMPIRE | imp::ALLOWS_SIZE_LEVEL_2 | imp::ALLOWS_SIZE_LEVEL_3) == 0
    })
}

/// Buildings the transfer destroys (`capture.md` 8): the Palace of a lost
/// capital (A), every ordinary building with culture (B), a quarter of the
/// rest on a capture into a city with no citizen of the taker's race (C,
/// one draw per building in index order), and small wonders (D).
pub fn destroyed_by_transfer(
    city: &City,
    was_capital: bool,
    capture_without_kin: bool,
    mut quarter: impl FnMut() -> bool,
) -> Vec<Production> {
    let mut lost = vec![];
    let mut rest = vec![];
    for &b in &city.buildings {
        let Some(def) = b.bldg() else { continue };
        if was_capital && def.flags & imp::CENTER_OF_EMPIRE != 0 {
            lost.push(b);
        } else if ordinary(b) && def.culture > 0 {
            lost.push(b);
        } else if ordinary(b) {
            rest.push(b);
        } else if def.other & oth::SMALL_WONDER != 0 {
            lost.push(b);
        }
    }
    if capture_without_kin {
        for b in rest {
            if quarter() {
                lost.push(b);
            }
        }
    }
    lost
}

/// Hand a city to `to` (`Player::takeCity`, `capture.md` 7): buildings are
/// lost, the culture stock follows the city while the old owner keeps what
/// it had, the people stay of their own race, and the flip test sleeps for
/// 1 turn after a capture, 10 after a conversion. Returns what was lost.
pub fn transfer(
    city: &mut City,
    to: usize,
    was_capital: bool,
    capture: bool,
    convert: bool,
    quarter: impl FnMut() -> bool,
) -> Vec<Production> {
    let from = city.civ;
    let kin = i32::from(city.nationals(to));
    let lost = destroyed_by_transfer(city, was_capital, capture && !convert && kin == 0, quarter);
    for p in &lost {
        if let Some(i) = city.buildings.iter().position(|b| b == p) {
            city.buildings.remove(i);
        }
    }
    // Citizen races and slot identity survive ownership changes.
    // `City +0x140[P] = max(+0x140[O], 0)`: the stock moves over, the old
    // owner's entry stays.
    city.stakes[from] = city.culture;
    city.civ = to;
    city.shields = 0;
    city.unrest = 0;
    city.clear_worked();
    city.queue.clear();
    city.cooldown = if convert { 10 } else { 1 };
    lost
}

/// Citizens of `civ`'s race in the city.
pub fn nationals(city: &City, civ: usize) -> i32 {
    i32::from(city.nationals(civ))
}

/// The civ's total culture, the "rating" the flip compares (`0x4F8E20`).
#[derive(Resource, Default)]
pub struct Flips {
    pub empire: [u32; CIV_COUNT],
}

/// The flip test of one city (`0x4B28D0`): the civ it flips to, if any.
#[allow(clippy::too_many_arguments)]
pub fn test_city(
    map: &GameMap,
    city: &City,
    capital_of: &[Option<(i32, i32)>; CIV_COUNT],
    owner_rating: &[u32; CIV_COUNT],
    tile_owner: &dyn Fn(i32, i32) -> Option<usize>,
    martial: i32,
    capital_lost: &[bool; CIV_COUNT],
    in_play: &[bool; CIV_COUNT],
    mut die: impl FnMut(i32) -> i32,
) -> Option<usize> {
    let owner = city.civ;
    let own_capital = capital_of[owner];
    for j in (0..CIV_COUNT).filter(|&j| j != owner && in_play[j]) {
        let tiles = radius_tiles(map, city.x, city.y)
            .into_iter()
            .filter(|&(x, y)| tile_owner(x, y) == Some(j))
            .count() as i32;
        let mut s = strength(&Contender {
            nationals: nationals(city, j),
            resisters: city.citizens.slots().iter().flatten()
                .filter(|c| c.resister && c.race == crate::civs::roster_index(j) as i32).count() as i32,
            tiles,
            disorder: city.unrest > 0,
            celebrating: false,
            larger_stake: city.stakes[j] > city.culture,
            owner_rating: owner_rating[owner] as i32,
            rival_rating: owner_rating[j] as i32,
            quelled: martial,
        });
        if s <= 0 {
            continue;
        }
        let d = denominator(
            capital_of[j].map(|c| dist(map, (city.x, city.y), c)),
            own_capital.map(|c| dist(map, (city.x, city.y), c)),
        );
        if capital_lost[owner] {
            s = capital_lost_boost(s);
        }
        if die(d) & 0xFFFF >= s {
            continue;
        }
        return Some(j);
    }
    None
}

/// Run the flip test for the cities of the civ whose turn ended, before
/// they take their city turn (`city-turn.md` step 1): a flipped city's turn
/// is skipped, because it no longer belongs to that civ.
#[allow(clippy::too_many_arguments)]
pub fn run(
    mut commands: Commands,
    mut ended: MessageReader<CivilizationEnded>,
    map: Res<GameMap>,
    mut cities: Query<(Entity, &mut City)>,
    mut units: Query<(Entity, &mut Unit)>,
    capital: Res<Capital>,
    mut diplomacy: ResMut<Diplomacy>,
    civs: Res<crate::civs::Civilizations>,
    art: Res<crate::units::UnitArt>,
    mut rng: ResMut<CombatRng>,
    mut flips: ResMut<Flips>,
    mut board: ResMut<MessageBoard>,
    mut prompts: ResMut<crate::production_prompt::ProductionPrompts>,
    mut view: ResMut<cities::CityView>,
) {
    // An answered question comes first.
    if let Some(ask) = diplomacy.convert_ask
        && let Some(yes) = ask.answer
    {
        diplomacy.convert_ask = None;
        if yes {
            convert(&mut commands, &map, ask.city, ask.to, &mut cities, &mut units, &capital, &art, &mut rng, &mut board, &mut prompts, &mut view, &civs, &diplomacy);
        } else if let Ok((_, c)) = cities.get(ask.city) {
            post(&mut board, format!("We rebuffed the rebels of {}.", c.name));
        }
    }
    for ev in ended.read() {
        let civ = ev.0;
        if civ >= CIV_COUNT {
            continue;
        }
        // The empire total grows by the culture this turn's cities make.
        let anarchy = crate::realm::in_anarchy(civ);
        if !anarchy {
            for (e, c) in cities.iter().filter(|(_, c)| c.civ == civ) {
                flips.empire[civ] += cities::culture_per_turn(c, capital.0[civ] == Some(e));
            }
        }
        // The cooldown runs down; a city that is cooling does not roll.
        let order: Vec<Entity> = cities.iter().filter(|(_, c)| c.civ == civ).map(|(e, _)| e).collect();
        for e in order {
            {
                let Ok((_, mut city)) = cities.get_mut(e) else { continue };
                if city.civ != civ {
                    continue;
                }
                if city.cooldown > 0 {
                    city.cooldown -= 1;
                    continue;
                }
            }
            if capital.0[civ] == Some(e) || diplomacy.convert_ask.is_some_and(|a| a.city == e) {
                continue;
            }
            let Ok((_, city_ref)) = cities.get(e) else { continue };
            let city_now = city_ref.clone();
            let owners = territory(&map);
            let tile_owner = |x: i32, y: i32| owners.get(&(x, y)).copied();
            let mut capital_of = [None; CIV_COUNT];
            for (k, slot) in capital_of.iter_mut().enumerate() {
                *slot = capital.0[k].and_then(|ce| cities.get(ce).ok()).map(|(_, c)| (c.x, c.y));
            }
            let mut in_play = [false; CIV_COUNT];
            for (k, p) in in_play.iter_mut().enumerate() {
                *p = !civs.eliminated[k];
            }
            let martial = units
                .iter()
                .filter(|(_, u)| u.x == city_now.x && u.y == city_now.y && def(u.utype).attack > 0)
                .count() as i32;
            // The capital-lost counter (`Player +0x15C8`) is not modelled.
            let target = test_city(
                &map,
                &city_now,
                &capital_of,
                &flips.empire,
                &tile_owner,
                martial,
                &[false; CIV_COUNT],
                &in_play,
                |d| rng.0.below(d as u32) as i32,
            );
            let Some(to) = target else { continue };
            if !is_ai(to) {
                if diplomacy.convert_ask.is_none() {
                    diplomacy.convert_ask = Some(ConvertAsk { city: e, to, answer: None });
                }
                continue;
            }
            let facts = accept_facts(&city_now, to, &diplomacy);
            if !ai_accepts_city(&facts.facts(&facts.wonders)) {
                continue;
            }
            convert(&mut commands, &map, e, to, &mut cities, &mut units, &capital, &art, &mut rng, &mut board, &mut prompts, &mut view, &civs, &diplomacy);
        }
    }
}

struct Accept {
    at_war: bool,
    wonders: Vec<Wonder>,
    kin: i32,
    cities: i32,
    ocn: i32,
}

impl Accept {
    fn facts<'a>(&self, wonders: &'a [Wonder]) -> AcceptFacts<'a> {
        AcceptFacts {
            at_war_with_owner: self.at_war,
            wonders,
            my_race_citizens: self.kin,
            city_count: self.cities,
            ocn: self.ocn,
        }
    }
}

fn accept_facts(city: &City, to: usize, diplomacy: &Diplomacy) -> Accept {
    let known = crate::realm::read(to, |r| r.known);
    let wonders = city
        .buildings
        .iter()
        .filter_map(|b| b.bldg())
        .filter(|b| b.is_great_wonder())
        .map(|b| Wonder {
            active: b.obsolete < 0 || b.obsolete >= 128 || known >> b.obsolete & 1 == 0,
            built_by_me: false,
        })
        .collect();
    Accept {
        at_war: diplomacy.at_war(to, city.civ),
        wonders,
        kin: nationals(city, to),
        cities: crate::realm::read(to, |r| r.cities as i32),
        ocn: crate::govern::optimal_cities(to),
    }
}

/// The conversion itself: the transfer, the units on the tile sent home, a
/// new defender, the announcements.
#[allow(clippy::too_many_arguments)]
fn convert(
    commands: &mut Commands,
    map: &GameMap,
    e: Entity,
    to: usize,
    cities: &mut Query<(Entity, &mut City)>,
    units: &mut Query<(Entity, &mut Unit)>,
    capital: &Capital,
    art: &crate::units::UnitArt,
    rng: &mut CombatRng,
    board: &mut MessageBoard,
    prompts: &mut crate::production_prompt::ProductionPrompts,
    view: &mut cities::CityView,
    civs: &crate::civs::Civilizations,
    _diplomacy: &Diplomacy,
) {
    let Ok((_, city)) = cities.get(e) else { return };
    let from = city.civ;
    if from == to {
        return;
    }
    let (x, y, name) = (city.x, city.y, city.name.clone());
    let was_capital = capital.0[from] == Some(e);
    let home = capital.0[from]
        .filter(|&ce| ce != e)
        .and_then(|ce| cities.get(ce).ok())
        .map(|(_, c)| (c.x, c.y));
    let Ok((_, mut city)) = cities.get_mut(e) else { return };
    let lost = transfer(&mut city, to, was_capital, false, true, || rng.0.below(4) == 0);
    // A conversion only records the pending nationality (`0x4BB090`).
    crate::resistance::seed(&mut city, to, true, &crate::resistance::Nations::current(), &mut rng.0);
    drop(city);
    // Units of the old owner on the tile go to its capital, or are lost.
    for (ue, mut u) in units.iter_mut().filter(|(_, u)| u.civ == from && (u.x, u.y) == (x, y)) {
        match home {
            Some((hx, hy)) => {
                u.x = hx;
                u.y = hy;
                u.path.clear();
            }
            None => commands.entity(ue).despawn(),
        }
    }
    // A new defender for the new owner (`takeCity` step 18).
    if let Some(t) = best_defender(to) {
        crate::units::spawn_unit(commands, art, t, x, y, to);
    }
    prompts.forget_city(e);
    if view.0 == Some(e) {
        view.0 = None;
    }
    let viewer = civs.viewer();
    let _ = map;
    if to == viewer {
        post(board, format!("{name} has converted to our civilization!"));
    } else if from == viewer {
        let gone: Vec<&str> = lost.iter().map(|p| p.name()).collect();
        post(
            board,
            format!(
                "The citizens of {name} have switched to the {}!{}",
                crate::civs::name(to),
                if gone.is_empty() { String::new() } else { format!(" We lost: {}.", gone.join(", ")) }
            ),
        );
    }
}

/// The land defender the civ would build first (the best `PRTO` defense).
fn best_defender(civ: usize) -> Option<crate::units::UnitType> {
    Production::all()
        .filter_map(|p| p.unit())
        .filter(|t| {
            let r = t.row();
            r.playable && r.class == 0 && r.pop_cost == 0 && r.defense > 0 && r.attack > 0
                && r.ai & 0x2 != 0
                && crate::research::can_build(civ, Production::from_unit(*t))
        })
        .max_by_key(|t| (t.row().defense, -t.row().cost))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Contender {
        Contender { owner_rating: 10, rival_rating: 10, ..Contender::default() }
    }

    #[test]
    fn golden_f1_f2_the_count_of_people_and_borders() {
        assert_eq!(strength(&Contender { nationals: 1, ..base() }), 1);
        // A = 2, B = 1 (a resister counts twice), C = 5: S = 3 + 3.
        assert_eq!(strength(&Contender { nationals: 2, resisters: 1, tiles: 5, ..base() }), 6);
    }

    #[test]
    fn golden_f3_f4_f5_disorder_stake_and_rating() {
        // 6 doubles for disorder, doubles again for the larger stake.
        let c = Contender { nationals: 6, disorder: true, larger_stake: true, ..base() };
        assert_eq!(strength(&c), 24);
        // Ro = 11, Rj = 12: trunc(12 * 24 / 11) = 26.
        let c = Contender { nationals: 6, disorder: true, larger_stake: true, owner_rating: 10, rival_rating: 11, ..base() };
        assert_eq!(strength(&c), 26);
        // Rj = 50 > 4 * Ro: times four.
        let c = Contender { nationals: 6, owner_rating: 10, rival_rating: 49, ..base() };
        assert_eq!(strength(&c), 24);
    }

    #[test]
    fn s_is_capped_at_ten_and_a_celebration_and_martial_law_cut_it() {
        assert_eq!(strength(&Contender { nationals: 40, ..base() }), 10);
        assert_eq!(strength(&Contender { nationals: 10, celebrating: true, ..base() }), 5);
        assert_eq!(strength(&Contender { nationals: 10, quelled: 3, ..base() }), 7);
        assert_eq!(strength(&Contender { tiles: 2, ..base() }), 0, "two tiles are nothing");
    }

    #[test]
    fn golden_f6_f7_f8_f9_capital_loss_and_the_die() {
        assert_eq!(capital_lost_boost(5), 6);
        assert_eq!(capital_lost_boost(80), 111);
        assert_eq!(
            [5, 10, 15, 20, 80, 160].map(capital_lost_boost),
            [6, 13, 20, 27, 111, 223]
        );
        assert_eq!(denominator(Some(3), Some(12)), 500);
        assert_eq!(denominator(Some(20), Some(2)), 8000);
        assert_eq!(denominator(None, Some(2)), 2000);
        assert_eq!(denominator(Some(5), None), 2000);
    }

    #[test]
    fn golden_d1_the_executables_distance() {
        let map = GameMap::generate_with_seed(1);
        // (dx, dy) on the doubled grid, y parity 0 on both ends.
        let d = |dx: i32, dy: i32| dist(&map, (0, 0), (dx / 2, dy));
        assert_eq!(d(0, 0), 0);
        assert_eq!(d(2, 0), 1);
        assert_eq!(d(4, 0), 3);
        assert_eq!(d(8, 0), 6);
        assert_eq!(d(0, 2), 1);
    }

    #[test]
    fn a_transfer_burns_culture_buildings_and_small_wonders_but_keeps_gates_and_wonders() {
        let mut city = City::new(1, "Veii", 3, 3);
        city.set_size(4);
        city.culture = 77;
        city.buildings = vec![
            Production::Temple,
            Production::Granary,
            Production::Barracks,
            Production::Palace,
            Production::ThePyramids,
        ];
        let lost = transfer(&mut city, 2, true, false, true, || false);
        assert!(lost.contains(&Production::Temple), "culture buildings go");
        assert!(lost.contains(&Production::Palace), "a lost capital's Palace goes");
        assert!(city.buildings.contains(&Production::Granary));
        assert!(city.buildings.contains(&Production::Barracks), "no quarter rule on a conversion");
        assert!(city.buildings.contains(&Production::ThePyramids), "great wonders always stay");
        assert_eq!(city.civ, 2);
        assert_eq!(city.stakes[1], 77, "the old owner keeps its stake");
        assert_eq!(city.nationals(1), 4, "the people stay Roman");
        assert_eq!(city.cooldown, 10);
        assert_eq!(nationals(&city, 2), 0);
        assert_eq!(nationals(&city, 1), 4);
    }

    #[test]
    fn a_capture_without_kin_loses_a_quarter_of_the_rest_by_the_dice() {
        let mut city = City::new(1, "Veii", 3, 3);
        city.set_size(3);
        city.buildings = vec![Production::Granary, Production::Barracks];
        let mut draws = [true, false].into_iter();
        let lost = transfer(&mut city, 0, false, true, false, || draws.next().unwrap());
        assert_eq!(lost, vec![Production::Granary]);
        assert_eq!(city.cooldown, 1, "a plain capture sleeps for one turn");
    }

    #[test]
    fn the_test_flips_only_for_a_rival_with_people_or_land_and_obeys_the_gates() {
        let map = GameMap::generate_with_seed(1);
        let mut city = City::new(1, "Veii", 10, 10);
        city.set_size(8);
        city.set_nationality(2);
        let none = &|_: i32, _: i32| None;
        let ratings = [0, 100, 1, 0];
        let play = [true; CIV_COUNT];
        let caps = [None; CIV_COUNT];
        // A die that always comes up 0 flips: S is 10, ratio 2/101 -> 0 first.
        let flip = |city: &City, ratings: &[u32; CIV_COUNT]| {
            test_city(&map, city, &caps, ratings, none, 0, &[false; CIV_COUNT], &play, |_| 0)
        };
        assert_eq!(flip(&city, &[0, 5, 5, 0]), Some(2));
        // The die at the maximum never does.
        let never = test_city(&map, &city, &caps, &[0, 5, 5, 0], none, 0, &[false; CIV_COUNT], &play, |d| d - 1);
        assert_eq!(never, None);
        // Nobody of that race and no land: nothing to roll.
        city.set_nationality(city.civ);
        assert_eq!(flip(&city, &ratings), None);
    }

    // ---- the system, headless ------------------------------------------

    fn world() -> App {
        // Japan is the human, the rest are the computer.
        crate::civs::set_controllers();
        let mut app = App::new();
        app.insert_resource(GameMap::generate_with_seed(1));
        app.insert_resource(crate::units::UnitArt::blank());
        app.insert_resource(CombatRng(crate::rng::MapRng::new(7)));
        app.init_resource::<Capital>();
        app.init_resource::<Diplomacy>();
        app.init_resource::<crate::civs::Civilizations>();
        app.init_resource::<Flips>();
        app.init_resource::<MessageBoard>();
        app.init_resource::<crate::production_prompt::ProductionPrompts>();
        app.init_resource::<cities::CityView>();
        app.add_message::<CivilizationEnded>();
        // The controller table is per test thread.
        app.edit_schedule(Update, |s| { s.set_executor_kind(bevy::ecs::schedule::ExecutorKind::SingleThreaded); });
        app.add_systems(Update, run);
        app
    }

    /// A Roman city that is nearly all Egyptian, with no capital to keep it.
    fn restless_city(app: &mut App) -> Entity {
        let mut city = City::new(1, "Veii", 10, 10);
        city.set_size(8);
        city.set_nationality(2);
        city.buildings = vec![Production::Temple, Production::Granary];
        city.culture = 40;
        app.world_mut().spawn(city).id()
    }

    #[test]
    fn a_restless_city_eventually_flips_to_the_computer_civ_that_its_people_belong_to() {
        let mut app = world();
        let e = restless_city(&mut app);
        // Egypt's culture rivals Rome's, or the ratio would smother the flip.
        app.world_mut().resource_mut::<Flips>().empire[2] = 100_000;
        for turn in 0..6000 {
            app.world_mut().write_message(CivilizationEnded(1));
            app.update();
            if app.world().get::<City>(e).unwrap().civ != 1 {
                let city = app.world().get::<City>(e).unwrap();
                assert_eq!(city.civ, 2);
                assert!(!city.buildings.contains(&Production::Temple));
                assert_eq!(city.cooldown, 10);
                assert!(turn >= 1, "the first roll is at 8/2000 a turn");
                return;
            }
        }
        panic!("a city of 8 foreign citizens never flipped in 6000 turns");
    }

    #[test]
    fn a_capital_never_flips_and_a_cooling_city_does_not_roll() {
        let mut app = world();
        let e = restless_city(&mut app);
        app.world_mut().resource_mut::<Capital>().0[1] = Some(e);
        for _ in 0..3000 {
            app.world_mut().write_message(CivilizationEnded(1));
            app.update();
        }
        assert_eq!(app.world().get::<City>(e).unwrap().civ, 1);
        app.world_mut().resource_mut::<Capital>().0[1] = None;
        app.world_mut().get_mut::<City>(e).unwrap().cooldown = 200;
        for _ in 0..200 {
            app.world_mut().write_message(CivilizationEnded(1));
            app.update();
        }
        assert_eq!(app.world().get::<City>(e).unwrap().civ, 1);
        assert_eq!(app.world().get::<City>(e).unwrap().cooldown, 0, "one tick per city turn");
    }

    #[test]
    fn a_human_is_asked_and_may_rebuff_or_accept() {
        let mut app = world();
        let e = restless_city(&mut app);
        app.world_mut().get_mut::<City>(e).unwrap().set_nationality(0);
        // Civ 0 is a human in the test's default controller mask.
        app.world_mut().resource_mut::<Diplomacy>().convert_ask =
            Some(ConvertAsk { city: e, to: 0, answer: Some(false) });
        app.update();
        assert_eq!(app.world().get::<City>(e).unwrap().civ, 1, "rebuffed");
        assert!(app.world().resource::<Diplomacy>().convert_ask.is_none());
        app.world_mut().resource_mut::<Diplomacy>().convert_ask =
            Some(ConvertAsk { city: e, to: 0, answer: Some(true) });
        app.update();
        assert_eq!(app.world().get::<City>(e).unwrap().civ, 0, "accepted");
    }
}
