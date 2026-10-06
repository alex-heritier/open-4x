//! Unit actions beyond orders and the four worker jobs: join a city,
//! pillage, and automate a worker.
//!
//! * Join City (`J`): a unit whose type has the Join City worker action
//!   (`PRTO` worker bit 12) adds its `pop_cost` citizens to a city of its
//!   civ, up to the size the city's improvements allow, and is used up.
//! * Pillage (`Shift+P`): a unit with the Pillage special action (bit 3)
//!   follows the native destruction classes (`colonies.md` 8) outside a city.
//!   HYPOTHESIS: the diplomatic gate remains the clone's.
//! * Automate (`Z`): a Worker picks its own jobs with the computer's
//!   worker brain (`ai::worker_act`) until a manual order stops it.

use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

use crate::actionbar::UnitCommand;
use crate::ai::{self, Act, Board};
use crate::cities::{self, City};
use crate::citycalc;
use crate::civs::{Civilizations, is_ai};
use crate::diplomacy::Diplomacy;
use crate::features::{MessageBoard, post};
use crate::improvements::{Work, action_slot};
use crate::map::{GameMap, MP, Tile};
use crate::roster::{special, worker};
use crate::units::{Turn, Unit, UnitAnim, UnitType};

/// Citizens a unit adds when it joins a city, for a type that can.
pub fn join_pop(t: UnitType) -> Option<u8> {
    let r = t.row();
    (r.worker & worker::JOIN_CITY != 0 && r.pop_cost > 0).then_some(r.pop_cost as u8)
}

/// The unit stands in a city of its own civ that has room for its
/// citizens, and has movement left.
pub fn can_join(map: &GameMap, u: &Unit, city: Option<&City>) -> bool {
    let (Some(pop), Some(c)) = (join_pop(u.utype), city) else {
        return false;
    };
    u.moves > 0
        && c.civ == u.civ
        && (c.x, c.y) == (u.x, u.y)
        && i32::from(c.size()) + i32::from(pop)
            <= citycalc::size_limit(c, map.fresh_water(c.x, c.y))
}

pub fn can_pillage_type(t: UnitType) -> bool {
    t.row().special & special::PILLAGE != 0
}

pub fn can_automate(t: UnitType) -> bool {
    t.row().worker & worker::AUTOMATE != 0
}

/// What a pillage takes off a tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Loot {
    Barricade,
    Terrain,
    Fortifications,
}

impl Loot {
    fn name(self) -> &'static str {
        match self {
            Loot::Barricade => "barricade",
            Loot::Terrain => "tile improvements",
            Loot::Fortifications => "fortifications",
        }
    }
}

/// Native destruction order (`colonies.md` 8): downgrade a barricade,
/// then remove all low terrain bits, then fortifications / outposts.
pub fn loot(t: &Tile) -> Option<Loot> {
    if t.barricade {
        Some(Loot::Barricade)
    } else if t.mine || t.irrigation || t.road {
        Some(Loot::Terrain)
    } else if t.fortress || matches!(t.site, Some(crate::sites::Site::Outpost(_))) {
        Some(Loot::Fortifications)
    } else {
        None
    }
}

/// Why a pillage is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    Nothing,
    City,
    Ours,
    Peace(usize),
}

/// May `civ` pillage the tile: it has an improvement, no city stands on
/// it, and its owner is nobody or a civ at war with the pillager.
pub fn pillage_check(
    t: &Tile,
    civ: usize,
    city_here: bool,
    owner: Option<usize>,
    at_war: impl Fn(usize, usize) -> bool,
) -> Result<Loot, Refusal> {
    if city_here {
        return Err(Refusal::City);
    }
    let Some(l) = loot(t) else {
        return Err(Refusal::Nothing);
    };
    match owner {
        Some(o) if o == civ => Err(Refusal::Ours),
        Some(o) if !at_war(civ, o) => Err(Refusal::Peace(o)),
        _ => Ok(l),
    }
}

/// Take `l` off the tile.
pub fn strip(t: &mut Tile, l: Loot) {
    match l {
        Loot::Barricade => {
            t.barricade = false;
            t.fortress = true;
        }
        Loot::Terrain => {
            t.mine = false;
            t.irrigation = false;
            t.road = false;
        }
        Loot::Fortifications => {
            t.fortress = false;
            t.barricade = false;
            if matches!(t.site, Some(crate::sites::Site::Outpost(_))) {
                t.site = None;
            }
        }
    }
}

/// The bar's verdict for the commands this module owns; `None` for the
/// rest. Cheap: no border lookup (a pillage on foreign land is refused
/// with a message when it is ordered).
pub fn check(cmd: UnitCommand, map: &GameMap, u: &Unit, city: Option<&City>) -> Option<bool> {
    let enabled = match cmd {
        UnitCommand::JoinCity => can_join(map, u, city),
        UnitCommand::Pillage => {
            u.moves > 0 && city.is_none() && map.get(u.x, u.y).and_then(loot).is_some()
        }
        UnitCommand::Automate => u.auto || u.moves > 0,
        _ => return None,
    };
    Some(enabled && u.carrier.is_none())
}

/// Execute `JoinCity`, `Pillage` and `Automate` for the selected unit.
pub fn run(
    mut commands: Commands,
    mut cmds: MessageReader<UnitCommand>,
    selected: Res<crate::units::Selected>,
    civs: Res<Civilizations>,
    diplomacy: Res<Diplomacy>,
    mut map: ResMut<GameMap>,
    mut units: Query<(Entity, &mut Unit)>,
    mut cities: Query<(Entity, &mut City)>,
    mut board: ResMut<MessageBoard>,
) {
    for cmd in cmds.read().copied() {
        if !matches!(
            cmd,
            UnitCommand::JoinCity | UnitCommand::Pillage | UnitCommand::Automate
        ) {
            continue;
        }
        let Some(e) = selected.0 else { continue };
        let Ok((_, mut u)) = units.get_mut(e) else {
            continue;
        };
        if u.civ != civs.active || u.carrier.is_some() {
            continue;
        }
        match cmd {
            UnitCommand::Automate => {
                if !can_automate(u.utype) {
                    continue;
                }
                u.auto = !u.auto;
                if u.auto {
                    u.fortified = false;
                    u.sentry = false;
                    u.exploring = false;
                    u.path.clear();
                    post(&mut board, "Worker automated: it will choose its own jobs.");
                } else {
                    post(&mut board, "Automation off.");
                }
            }
            UnitCommand::JoinCity => {
                let all: Vec<City> = cities.iter().map(|(_, c)| c.clone()).collect();
                let here = all.iter().find(|c| (c.x, c.y) == (u.x, u.y));
                if !can_join(&map, &u, here) {
                    post(&mut board, "This unit cannot join a city here.");
                    continue;
                }
                let pop = join_pop(u.utype).expect("can_join checked the type");
                let taken = cities::taken_tiles(&map, all.iter(), (u.x, u.y));
                for (_, mut c) in cities.iter_mut() {
                    if (c.x, c.y) == (u.x, u.y) {
                        c.add_citizens(pop, u.nationality);
                        let box_ = crate::economy::food_box(c.size());
                        c.food = c.food.min(box_.saturating_sub(1));
                        cities::governor_fill(&map, &mut c, &taken);
                        post(
                            &mut board,
                            format!("{} grows to size {}.", c.name, c.size()),
                        );
                    }
                }
                commands.entity(e).despawn();
            }
            UnitCommand::Pillage => {
                if !can_pillage_type(u.utype) || u.moves == 0 {
                    continue;
                }
                let refs: Vec<City> = cities.iter().map(|(_, c)| c.clone()).collect();
                let city_here = refs.iter().any(|c| (c.x, c.y) == (u.x, u.y));
                let owner = map.tiles[map.idx(u.x, u.y)].owner.map(usize::from);
                let i = map.idx(u.x, u.y);
                match pillage_check(&map.tiles[i], u.civ, city_here, owner, |a, b| {
                    diplomacy.at_war(a, b)
                }) {
                    Ok(l) => {
                        strip(&mut map.tiles[i], l);
                        u.moves = u.moves.saturating_sub(MP);
                        u.path.clear();
                        u.fortified = false;
                        u.sentry = false;
                        post(&mut board, format!("Pillaged the {}.", l.name()));
                    }
                    Err(Refusal::Nothing) => post(&mut board, "There is nothing here to pillage."),
                    Err(Refusal::City) => post(&mut board, "A city cannot be pillaged."),
                    Err(Refusal::Ours) => post(&mut board, "We do not pillage our own land."),
                    Err(Refusal::Peace(o)) => post(
                        &mut board,
                        format!("We are not at war with the {}.", crate::civs::CIVS[o].name),
                    ),
                }
            }
            _ => {}
        }
    }
}

/// Bookkeeping of the human's automated workers within a turn.
#[derive(Default)]
pub struct AutoState {
    turn: Option<(usize, u32)>,
    /// Workers with nothing more to do this turn.
    done: HashSet<Entity>,
    /// Where each is headed.
    targets: HashMap<Entity, (i32, i32)>,
}

/// Move the human's automated workers: every worker with movement left
/// and no standing job gets an order from the computer's worker brain.
pub fn auto_workers(
    civs: Res<Civilizations>,
    turn: Res<Turn>,
    splash: Res<crate::splash::SplashUp>,
    map: Res<GameMap>,
    diplomacy: Res<Diplomacy>,
    cities: Query<&City>,
    mut units: Query<(Entity, &mut Unit)>,
    mut state: Local<AutoState>,
) {
    let civ = civs.active;
    if is_ai(civ) || splash.0 || civs.outcome.is_some() {
        return;
    }
    if state.turn != Some((civ, turn.0)) {
        state.turn = Some((civ, turn.0));
        state.done.clear();
    }
    state
        .targets
        .retain(|e, _| units.get(*e).is_ok_and(|(_, u)| u.auto));
    let idle = |e: Entity, u: &Unit| {
        u.civ == civ
            && u.auto
            && u.moves > 0
            && u.work.is_none()
            && u.path.is_empty()
            && matches!(u.anim, UnitAnim::Idle { .. })
            && !state.done.contains(&e)
    };
    let mut ready: Vec<Entity> = units
        .iter()
        .filter(|(e, u)| idle(*e, u))
        .map(|(e, _)| e)
        .collect();
    if ready.is_empty() {
        return;
    }
    ready.sort();
    let board = Board::new(
        cities.iter().cloned().collect(),
        units.iter().map(|(e, u)| (e, u.clone())).collect(),
        &map,
    )
    .with_war(diplomacy.war_matrix());
    for e in ready {
        let Ok((_, mut u)) = units.get_mut(e) else {
            continue;
        };
        let own = state.targets.get(&e).copied();
        let claimed: HashSet<(i32, i32)> = state
            .targets
            .iter()
            .filter(|&(&other, _)| other != e)
            .map(|(_, &t)| t)
            .collect();
        let (act, target) = ai::worker_act(&map, &board, &u, own, &claimed);
        match target {
            Some(t) => {
                state.targets.insert(e, t);
            }
            None => {
                state.targets.remove(&e);
            }
        }
        match act {
            Act::Go(path) => u.path = path.into(),
            Act::Work(action) => {
                u.work = Some(Work {
                    action,
                    progress: 0,
                });
                u.moves = 0;
                u.path.clear();
                u.anim = UnitAnim::OneShot {
                    slot: action_slot(action),
                    t: 0.0,
                };
                state.done.insert(e);
            }
            _ => {
                state.done.insert(e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{Base, Cover, Relief};
    use crate::realm;

    fn tile() -> Tile {
        let mut map = GameMap::generate();
        let t = &mut map.tiles[0];
        t.base = Base::Grassland;
        t.relief = Relief::Flat;
        t.cover = Cover::Bare;
        t.river = 0;
        t.road = false;
        t.irrigation = false;
        t.mine = false;
        t.clone()
    }

    #[test]
    fn pillage_downgrades_a_barricade_then_clears_terrain_then_fortifications() {
        let mut t = tile();
        t.road = true;
        t.irrigation = true;
        t.mine = true;
        t.fortress = true;
        t.barricade = true;
        let mut taken = vec![];
        while let Some(l) = loot(&t) {
            taken.push(l);
            strip(&mut t, l);
        }
        assert_eq!(
            taken,
            [Loot::Barricade, Loot::Terrain, Loot::Fortifications]
        );
    }

    #[test]
    fn pillaging_needs_something_to_take_outside_a_city_and_a_war() {
        let mut t = tile();
        let war = |a: usize, b: usize| a != b && (a, b) == (0, 2);
        assert_eq!(
            pillage_check(&t, 0, false, None, war),
            Err(Refusal::Nothing)
        );
        t.road = true;
        assert_eq!(pillage_check(&t, 0, false, None, war), Ok(Loot::Terrain));
        assert_eq!(pillage_check(&t, 0, true, None, war), Err(Refusal::City));
        assert_eq!(
            pillage_check(&t, 0, false, Some(0), war),
            Err(Refusal::Ours)
        );
        assert_eq!(
            pillage_check(&t, 0, false, Some(1), war),
            Err(Refusal::Peace(1))
        );
        assert_eq!(pillage_check(&t, 0, false, Some(2), war), Ok(Loot::Terrain));
    }

    #[test]
    fn settlers_and_workers_join_by_their_population_cost() {
        assert_eq!(join_pop(UnitType::named("Settler")), Some(2));
        assert_eq!(join_pop(UnitType::named("Worker")), Some(1));
        assert_eq!(join_pop(UnitType::named("Warrior")), None);
    }

    #[test]
    fn a_unit_joins_only_its_own_city_with_room() {
        realm::reset();
        let map = flat_map();
        let mut city = City::new(0, "Kyoto", 3, 3);
        city.set_size(4);
        let u = Unit::new(0, UnitType::named("Settler"), 3, 3);
        assert!(can_join(&map, &u, Some(&city)));
        // A town of six holds no more without an Aqueduct.
        city.set_size(5);
        assert!(
            !can_join(&map, &u, Some(&city)),
            "5 + 2 passes the town limit"
        );
        assert!(can_join(
            &map,
            &Unit::new(0, UnitType::named("Worker"), 3, 3),
            Some(&city)
        ));
        city.buildings.push(cities::Production::named("Aqueduct"));
        assert!(can_join(&map, &u, Some(&city)));
        // Somebody else's city, an empty tile, or no movement left.
        let foreign = City::new(1, "Rome", 3, 3);
        assert!(!can_join(&map, &u, Some(&foreign)));
        assert!(!can_join(&map, &u, None));
        let mut spent = u.clone();
        spent.moves = 0;
        assert!(!can_join(&map, &spent, Some(&city)));
    }

    #[test]
    fn joining_obeys_the_lake_exemption_and_hospital_limit() {
        realm::reset();
        let mut map = flat_map();
        let mut city = City::new(0, "Kyoto", 3, 3);
        let lake = map.idx(2, 3);
        map.tiles[lake].base = crate::map::Base::Coast;
        let worker = Unit::new(0, UnitType::named("Worker"), 3, 3);
        city.set_size(6);
        assert!(can_join(&map, &worker, Some(&city)));
        assert!(check(UnitCommand::JoinCity, &map, &worker, Some(&city)).unwrap());
        city.set_size(12);
        assert!(!can_join(&map, &worker, Some(&city)));
        city.buildings.push(cities::Production::named("Hospital"));
        assert!(can_join(&map, &worker, Some(&city)));
        // Irrigation is not freshwater for city growth.
        city.buildings.clear();
        city.set_size(6);
        map.tiles[lake].base = crate::map::Base::Grassland;
        map.tiles[lake].irrigation = true;
        assert!(!can_join(&map, &worker, Some(&city)));
    }

    fn flat_map() -> GameMap {
        let mut map = GameMap::generate();
        for t in map.tiles.iter_mut() {
            t.base = Base::Grassland;
            t.relief = Relief::Flat;
            t.cover = Cover::Bare;
            t.river = 0;
            t.resource = None;
            t.road = false;
            t.irrigation = false;
            t.mine = false;
            t.hut = false;
            t.camp = false;
            t.seen = true;
            t.visible = true;
        }
        map
    }

    fn world() -> App {
        let mut app = App::new();
        app.insert_resource(flat_map());
        app.insert_resource(Civilizations::default());
        app.insert_resource(Turn(1));
        app.insert_resource(crate::splash::SplashUp(false));
        app.insert_resource(Diplomacy::new());
        app.init_resource::<crate::cities::Treasury>();
        app.insert_resource(MessageBoard::default());
        app.insert_resource(crate::units::Selected(None));
        app.add_message::<UnitCommand>();
        app.edit_schedule(Update, |s| {
            s.set_executor_kind(bevy::ecs::schedule::ExecutorKind::SingleThreaded);
        });
        app
    }

    #[test]
    fn an_automated_worker_walks_to_a_job_and_does_it() {
        realm::reset();
        let mut app = world();
        let (cx, cy) = (app.world().resource::<GameMap>().w / 2, 10);
        app.world_mut().spawn(City::new(0, "Kyoto", cx, cy));
        let mut worker = Unit::new(0, UnitType::named("Worker"), cx, cy);
        worker.auto = true;
        let w = app.world_mut().spawn(worker).id();
        app.add_systems(Update, auto_workers);
        app.update();
        let u = app.world().get::<Unit>(w).unwrap().clone();
        assert!(
            u.work.is_none(),
            "the city tile has no job: the worker walks first"
        );
        let goal = *u.path.back().expect("a route to a job");
        // Arrived with movement left: it starts work.
        {
            let mut u = app.world_mut().get_mut::<Unit>(w).unwrap();
            (u.x, u.y) = goal;
            u.path.clear();
        }
        app.update();
        let u = app.world().get::<Unit>(w).unwrap();
        assert!(u.work.is_some(), "{:?}", u.work);
        assert_eq!(u.moves, 0);
        // A worker that is not automated is left alone.
        app.world_mut().get_mut::<Unit>(w).unwrap().work = None;
        app.world_mut().get_mut::<Unit>(w).unwrap().auto = false;
        app.world_mut().get_mut::<Unit>(w).unwrap().moves = MP;
        app.update();
        assert!(app.world().get::<Unit>(w).unwrap().work.is_none());
    }

    #[test]
    fn a_settler_joins_its_city_and_is_used_up() {
        realm::reset();
        let mut app = world();
        let (cx, cy) = (app.world().resource::<GameMap>().w / 2, 10);
        let mut city = City::new(0, "Kyoto", cx, cy);
        city.set_size(3);
        let c = app.world_mut().spawn(city).id();
        let mut settler = Unit::new(0, UnitType::named("Settler"), cx, cy);
        settler.nationality = crate::civs::roster_index(1);
        let s = app.world_mut().spawn(settler).id();
        app.insert_resource(crate::units::Selected(Some(s)));
        app.init_resource::<crate::cities::BorderKey>();
        app.add_systems(Update, (crate::cities::update_borders, run).chain());
        app.world_mut().write_message(UnitCommand::JoinCity);
        app.update();
        assert_eq!(app.world().get::<City>(c).unwrap().size(), 5);
        assert_eq!(app.world().get::<City>(c).unwrap().nationals(1), 2);
        assert_eq!(
            app.world()
                .get::<City>(c)
                .unwrap()
                .worked(app.world().resource::<GameMap>())
                .len(),
            5
        );
        assert!(app.world().get_entity(s).is_err(), "the settler is gone");
    }

    #[test]
    fn pillaging_is_refused_in_peace_and_allowed_in_war() {
        realm::reset();
        let mut app = world();
        let (cx, cy) = (app.world().resource::<GameMap>().w / 2, 10);
        // Rome's city owns the ground; Japan's Warrior stands on its road.
        app.world_mut().spawn(City::new(1, "Rome", cx, cy));
        let (x, y) = (cx + 1, cy);
        {
            let mut map = app.world_mut().resource_mut::<GameMap>();
            let i = map.idx(x, y);
            map.tiles[i].road = true;
        }
        let u = app
            .world_mut()
            .spawn(Unit::new(0, UnitType::named("Warrior"), x, y))
            .id();
        app.insert_resource(crate::units::Selected(Some(u)));
        app.init_resource::<crate::cities::BorderKey>();
        app.add_systems(Update, (crate::cities::update_borders, run).chain());
        let at_war = app.world().resource::<Diplomacy>().at_war(0, 1);
        app.world_mut().write_message(UnitCommand::Pillage);
        app.update();
        let map = app.world().resource::<GameMap>();
        assert_eq!(map.tiles[map.idx(x, y)].road, !at_war, "war decides");
    }

    #[test]
    fn only_workers_automate_and_only_fighters_pillage() {
        assert!(can_automate(UnitType::named("Worker")));
        assert!(!can_automate(UnitType::named("Warrior")));
        assert!(can_pillage_type(UnitType::named("Warrior")));
        assert!(!can_pillage_type(UnitType::named("Worker")));
    }
}
