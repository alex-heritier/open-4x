//! Coastal settlement for the clone's rule-based opponent, not a recovered
//! Civ3 target evaluator. All orders use the shared boarding/disembark mover.
use super::*;
use crate::map::{Base, MP};

type TilePos = (i32, i32);

fn land(map: &GameMap, board: &Board, civ: usize, from: TilePos) -> HashSet<TilePos> {
    flood(map, from, |p| map.get(p.0, p.1).is_some_and(|t| move_cost(t).is_some()) && !board.held_by_other(p, civ))
}

fn coast(map: &GameMap, board: &Board, civ: usize, p: TilePos) -> bool {
    !board.held_by_other(p, civ) && (map.get(p.0, p.1).is_some_and(|t| t.base == Base::Coast)
        || board.city_at(p).is_some_and(|c| c.civ == civ && c.coastal))
}

fn flood(map: &GameMap, from: TilePos, enter: impl Fn(TilePos) -> bool) -> HashSet<TilePos> {
    let mut seen = HashSet::from([from]);
    let mut queue = VecDeque::from([from]);
    while let Some(p) = queue.pop_front() {
        for next in map.neighbors(p.0, p.1) {
            if !seen.contains(&next) && enter(next) {
                seen.insert(next);
                queue.push_back(next);
            }
        }
    }
    seen
}

fn shores(map: &GameMap, board: &Board, civ: usize, water: &HashSet<TilePos>, claimed: &HashSet<TilePos>) -> HashSet<TilePos> {
    water.iter().flat_map(|&p| map.neighbors(p.0, p.1))
        .filter(|&p| site_ok(map, board, civ, claimed, p)).collect()
}

/// Every land tile joined to `from` by land, whoever stands on it.
fn landmass(map: &GameMap, from: TilePos) -> HashSet<TilePos> {
    flood(map, from, |p| map.get(p.0, p.1).is_some_and(|t| move_cost(t).is_some()))
}

/// The nearest city at war with `civ` that is on the shore of `water` and
/// not on the landmass of `home`: the one a land campaign cannot reach.
fn overseas_enemy<'a>(map: &GameMap, board: &'a Board, civ: usize, water: &HashSet<TilePos>, home: TilePos) -> Option<&'a City> {
    let here = landmass(map, home);
    board.cities.iter()
        .filter(|c| board.at_war(civ, c.civ) && !here.contains(&(c.x, c.y)))
        .filter(|c| map.neighbors(c.x, c.y).iter().any(|p| water.contains(p)))
        .min_by_key(|c| (map.distance(home, (c.x, c.y)), (c.x, c.y)))
}

/// A war the computer can only fight by sea: some enemy city is beyond its
/// land but on the coast its ships can sail to from the city at `from`.
pub(super) fn sea_war(map: &GameMap, board: &Board, civ: usize, from: TilePos) -> bool {
    let water = flood(map, from, |p| coast(map, board, civ, p));
    overseas_enemy(map, board, civ, &water, from).is_some()
}

/// Soldiers a landing party may take: the ones a city can spare, and those
/// in the field.
fn spare_soldiers<'a>(map: &GameMap, board: &'a Board, civ: usize) -> Vec<(&'a Entity, &'a Unit)> {
    let mut out = vec![];
    for (tile, stack) in &board.at {
        let mut soldiers: Vec<_> = stack.iter().filter(|(_, u)| u.civ == civ && super::land_soldier(u)).collect();
        soldiers.sort_by_key(|(e, _)| *e);
        let keep = board.city_at(*tile).filter(|c| c.civ == civ)
            .map_or(0, |c| super::garrison_need(c, super::threatened(map, board, civ, *tile)));
        out.extend(soldiers.into_iter().skip(keep).map(|(e, u)| (e, u)).filter(|(_, u)| u.moves > 0 && u.path.is_empty() && !u.attacked));
    }
    out
}

pub(super) fn room(map: &GameMap, board: &Board, civ: usize, from: TilePos) -> bool {
    let local = land(map, board, civ, from);
    let water = flood(map, from, |p| coast(map, board, civ, p));
    shores(map, board, civ, &water, &HashSet::new()).iter().any(|p| !local.contains(p))
}

fn voyage(map: &GameMap, board: &Board, civ: usize, from: TilePos, dest: TilePos) -> Option<Vec<TilePos>> {
    map.find_path_by(from, dest, |_, to| (to == dest || coast(map, board, civ, to)).then_some(MP))
}

/// Ship and passenger paths. Deterministic ordering reserves each settler and
/// destination once; an existing move finishes before a new plan is assigned.
pub(super) fn orders(map: &GameMap, board: &Board, civ: usize, turn: u32, targets: &HashMap<Entity, TilePos>) -> Vec<(Entity, Vec<TilePos>)> {
    if board.mine(civ).count() >= MAX_CITIES { return vec![]; }
    let mut claimed: HashSet<_> = targets.iter().filter(|(e, _)| board.at.values().flatten().any(|(id, u)| id == *e && u.civ == civ)).map(|(_, &p)| p).collect();
    let mut used = HashSet::new();
    let mut ships: Vec<_> = board.at.values().flatten().filter(|(_, u)| u.civ == civ && def(u.utype).class == 1
        && def(u.utype).capacity > 0 && u.moves > 0 && u.path.is_empty() && matches!(u.anim, UnitAnim::Idle { .. })).collect();
    ships.sort_by_key(|(e, _)| *e);
    let spots = board.city_spots();
    let mut result = vec![];
    for &(ship, u) in &ships {
        let from = (u.x, u.y);
        let water = flood(map, from, |p| coast(map, board, civ, p));
        let cargo: Vec<_> = board.at.values().flatten().filter(|(_, v)| v.carrier == Some(*ship)).collect();
        if cargo.iter().any(|(_, v)| v.utype == UnitType::Settler) {
            if cargo.iter().any(|(_, v)| !v.path.is_empty()) { continue; }
            if !cargo.iter().any(|(_, v)| v.utype == UnitType::Settler && v.moves > 0) { continue; }
            let goal = shores(map, board, civ, &water, &claimed).into_iter()
                .max_by_key(|&p| (site_score(map, &spots, p.0, p.1) - 2 * map.distance(from, p), std::cmp::Reverse(p)));
            if let Some(goal) = goal && let Some(path) = voyage(map, board, civ, from, goal) {
                claimed.insert(goal);
                result.push((*ship, path));
            }
            continue;
        }
        let troops = cargo.iter().filter(|(_, v)| super::land_soldier_aboard(v)).count();
        if troops > 0 {
            if cargo.iter().any(|(_, v)| !v.path.is_empty()) { continue; }
            if troops < def(u.utype).capacity as usize && troops < 2 { continue; }
            // Land units cannot attack from a ship (`naval::can_attack_from`):
            // the band lands on open ground beside the city and strikes
            // from there on a later turn.
            if let Some(target) = overseas_enemy(map, board, civ, &water, from) {
                let beach = map.neighbors(target.x, target.y).into_iter()
                    .filter(|&p| map.get(p.0, p.1).is_some_and(|t| move_cost(t).is_some()) && !board.held_by_other(p, civ)
                        && map.neighbors(p.0, p.1).iter().any(|q| water.contains(q)))
                    .min_by_key(|&p| (map.distance(from, p), p));
                if let Some(beach) = beach && let Some(path) = voyage(map, board, civ, from, beach) {
                    result.push((*ship, path));
                }
            }
            continue;
        }
        if !cargo.is_empty() { continue; }
        if turn >= super::AGGRESSION_TURN {
            let spare = spare_soldiers(map, board, civ);
            if spare.len() >= 2 {
                let mut band: Vec<_> = spare.iter().filter(|(e, _)| !used.contains(*e)).collect();
                band.sort_by_key(|(e, v)| (map.distance(from, (v.x, v.y)), **e));
                if let Some(&&(lead, first)) = band.first() {
                    let home = (first.x, first.y);
                    let local = landmass(map, home);
                    if overseas_enemy(map, board, civ, &water, home).is_some() {
                        let pickup = water.iter().copied().filter(|&p| map.get(p.0, p.1).is_some_and(|t| t.base == Base::Coast)
                            && map.neighbors(p.0, p.1).iter().any(|p| local.contains(p)))
                            .min_by_key(|&p| (map.distance(from, p) + map.distance(home, p), p));
                        if let Some(pickup) = pickup && let Some(path) = voyage(map, board, civ, from, pickup) {
                            if from == pickup {
                                let mut taken = 0;
                                for &&(e, v) in band.iter().filter(|(_, v)| local.contains(&(v.x, v.y))) {
                                    if taken >= def(u.utype).capacity as usize { break; }
                                    let spot = (v.x, v.y);
                                    if let Some(walk) = map.find_path_by(spot, pickup, |a, b| {
                                        if b == pickup { Some(MP) }
                                        else if !board.held_by_other(b, civ) { map.land_cost(a, b, crate::units::bridges(civ)) }
                                        else { None }
                                    }) {
                                        result.push((*e, walk));
                                        used.insert(*e);
                                        taken += 1;
                                    }
                                }
                                let _ = lead;
                            } else {
                                result.push((*ship, path));
                            }
                            continue;
                        }
                    }
                }
            }
        }
        let mut settlers: Vec<_> = board.at.values().flatten().filter(|(e, v)| v.civ == civ && v.utype == UnitType::Settler
            && v.carrier.is_none() && v.moves > 0 && v.path.is_empty() && !used.contains(e)).collect();
        settlers.sort_by_key(|(e, v)| (map.distance(from, (v.x, v.y)), *e));
        for &(passenger, v) in &settlers {
            let home = (v.x, v.y);
            let local = land(map, board, civ, home);
            if local.iter().any(|&p| site_ok(map, board, civ, &claimed, p)) { continue; }
            if !shores(map, board, civ, &water, &claimed).iter().any(|p| !local.contains(p)) { continue; }
            let pickup = water.iter().copied().filter(|&p| map.get(p.0, p.1).is_some_and(|t| t.base == Base::Coast)
                && map.neighbors(p.0, p.1).iter().any(|p| local.contains(p)))
                .min_by_key(|&p| (map.distance(from, p) + map.distance(home, p), p));
            let Some(pickup) = pickup else { continue };
            let Some(path) = voyage(map, board, civ, from, pickup) else { continue };
            if from == pickup {
                // The final water step uses the ordinary friendly-capacity gate.
                if let Some(walk) = map.find_path_by(home, pickup, |a, b| {
                    if b == pickup { Some(MP) }
                    else if !board.held_by_other(b, civ) { map.land_cost(a, b, crate::units::bridges(civ)) }
                    else { None }
                }) { result.push((*passenger, walk)); }
            } else { result.push((*ship, path)); }
            used.insert(*passenger);
            break;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn islands() -> (GameMap, City) {
        let mut map = GameMap::generate_with_seed(1);
        for t in &mut map.tiles { t.base = Base::Ocean; t.hut = false; t.camp = false; }
        for y in 1..=6 { for x in 1..=8 {
            let i = map.idx(x, y); map.tiles[i].base = Base::Coast;
        } }
        for p in [(2, 3), (3, 3), (2, 4), (3, 4), (8, 3), (8, 4)] {
            let i = map.idx(p.0, p.1); map.tiles[i].base = Base::Grassland;
        }
        let mut city = City::new(1, "Home", 2, 3);
        city.coastal = true;
        (map, city)
    }

    #[test]
    fn overseas_sites_require_a_safe_coastal_route_and_unoccupied_land() {
        let (mut map, city) = islands();
        let board = Board::new(vec![city.clone()], vec![], &map);
        assert!(room(&map, &board, 1, (2, 3)));
        let enemies = [(8, 3), (8, 4)].into_iter().enumerate().map(|(i, p)|
            (Entity::from_bits(i as u64 + 1), Unit::new(2, UnitType::Warrior, p.0, p.1))).collect();
        let occupied = Board::new(vec![city], enemies, &map);
        assert!(!room(&map, &occupied, 1, (2, 3)));
        for y in 1..=6 { let i = map.idx(6, y); map.tiles[i].base = Base::Sea; }
        assert!(!room(&map, &board, 1, (2, 3)));
    }

    #[test]
    fn ai_picks_up_a_settler_sails_lands_and_orders_an_island_city() {
        crate::realm::reset();
        crate::civs::set_controllers();
        assert!(is_ai(1));
        let (map, city) = islands();
        let mut app = App::new();
        app.edit_schedule(Update, |s| { s.set_executor_kind(bevy::ecs::schedule::ExecutorKind::SingleThreaded); });
        app.insert_resource(Time::<()>::default());
        app.insert_resource(map);
        app.insert_resource(SplashUp(false));
        app.init_resource::<Civilizations>();
        app.world_mut().resource_mut::<Civilizations>().active = 1;
        app.insert_resource(Turn(1));
        app.init_resource::<AiState>();
        app.insert_resource(crate::combat::CombatRng(crate::rng::MapRng::new(1)));
        app.init_resource::<cities::Treasury>();
        app.init_resource::<crate::unit_picker::UnitPicker>();
        app.insert_resource(crate::diplomacy::Diplomacy::new());
        app.add_message::<FoundCityOrder>();
        app.add_message::<TurnEnded>();
        app.add_message::<crate::bombard::Order>();
        app.add_message::<crate::combat::AttackOrder>();
        app.add_systems(Update, (play_turn, crate::units::drive_movement, crate::naval::sync_cargo).chain());
        app.world_mut().spawn(city);
        let settler = app.world_mut().spawn(Unit::new(1, UnitType::Settler, 3, 3)).id();
        let ship = app.world_mut().spawn(Unit::new(1, UnitType::Galley, 4, 3)).id();
        let mut boarded = false;
        let mut landed = false;
        let mut founded = false;
        for _ in 0..120 {
            app.update();
            let u = app.world().get::<Unit>(settler).unwrap();
            boarded |= u.carrier == Some(ship);
            if u.x == 8 && u.carrier.is_none() && !landed {
                assert_eq!(u.moves, 0);
                landed = true;
            }
            if app.world().resource::<Messages<FoundCityOrder>>().iter_current_update_messages().any(|o| o.0 == settler) {
                founded = true; break;
            }
            // Fast-forward animation and other civilizations' turns. Decisions,
            // boarding, domain gates and movement charges run through the game.
            if !app.world().resource::<Messages<TurnEnded>>().is_empty() {
                app.world_mut().resource_mut::<Messages<TurnEnded>>().clear();
                app.world_mut().resource_mut::<Turn>().0 += 1;
                for mut u in app.world_mut().query::<&mut Unit>().iter_mut(app.world_mut()) {
                    u.moves = crate::naval::moves(u.utype, u.civ);
                }
            }
            for mut u in app.world_mut().query::<&mut Unit>().iter_mut(app.world_mut()) {
                u.anim = UnitAnim::Idle { t: 0.0 };
            }
        }
        assert!(boarded && landed && founded, "boarded={boarded}, landed={landed}, founded={founded}");
        assert_ne!(app.world().get::<Unit>(ship).unwrap().x, 8);
        assert!(app.world().resource::<crate::unit_picker::UnitPicker>().unload.is_none());
    }

    #[test]
    fn ai_ferries_a_band_of_soldiers_to_an_enemy_city_across_the_water() {
        crate::realm::reset();
        crate::civs::set_controllers();
        let (map, city) = islands();
        let mut diplomacy = crate::diplomacy::Diplomacy::new();
        diplomacy.meet(1, 2);
        diplomacy.declare(&crate::diplomacy::Facts::even(), 1, 2, 0, &mut crate::features::MessageBoard::default());
        assert!(diplomacy.at_war(1, 2));
        let mut app = App::new();
        app.edit_schedule(Update, |s| { s.set_executor_kind(bevy::ecs::schedule::ExecutorKind::SingleThreaded); });
        app.insert_resource(Time::<()>::default());
        app.insert_resource(map);
        app.insert_resource(SplashUp(false));
        app.init_resource::<Civilizations>();
        app.world_mut().resource_mut::<Civilizations>().active = 1;
        app.insert_resource(Turn(20));
        app.init_resource::<AiState>();
        app.insert_resource(crate::combat::CombatRng(crate::rng::MapRng::new(1)));
        app.init_resource::<cities::Treasury>();
        app.init_resource::<crate::unit_picker::UnitPicker>();
        app.insert_resource(diplomacy);
        app.add_message::<FoundCityOrder>();
        app.add_message::<TurnEnded>();
        app.add_message::<crate::bombard::Order>();
        app.add_message::<crate::combat::AttackOrder>();
        app.add_systems(Update, (play_turn, crate::units::drive_movement, crate::naval::sync_cargo).chain());
        app.world_mut().spawn(city);
        let mut enemy = City::new(2, "Away", 8, 3);
        enemy.coastal = true;
        app.world_mut().spawn(enemy);
        app.world_mut().spawn(Unit::new(1, UnitType::Warrior, 2, 3));
        let band: Vec<_> = [(3, 3), (3, 4), (2, 4)].into_iter()
            .map(|p| app.world_mut().spawn(Unit::new(1, UnitType::Archer, p.0, p.1)).id()).collect();
        let ship = app.world_mut().spawn(Unit::new(1, UnitType::Galley, 4, 3)).id();
        let mut boarded = 0;
        let mut struck = false;
        for _ in 0..200 {
            app.update();
            boarded = boarded.max(band.iter().filter(|e| app.world().get::<Unit>(**e).unwrap().carrier == Some(ship)).count());
            if app.world().resource::<Messages<crate::combat::AttackOrder>>().iter_current_update_messages().any(|o| band.contains(&o.attacker) && o.to == (8, 3)) {
                struck = true; break;
            }
            if !app.world().resource::<Messages<TurnEnded>>().is_empty() {
                app.world_mut().resource_mut::<Messages<TurnEnded>>().clear();
                app.world_mut().resource_mut::<Turn>().0 += 1;
                for mut u in app.world_mut().query::<&mut Unit>().iter_mut(app.world_mut()) {
                    u.moves = crate::naval::moves(u.utype, u.civ);
                }
            }
            for mut u in app.world_mut().query::<&mut Unit>().iter_mut(app.world_mut()) {
                u.anim = UnitAnim::Idle { t: 0.0 };
            }
        }
        assert!(boarded >= 2, "boarded {boarded}");
        let dbg: Vec<_> = band.iter().map(|e| { let u = app.world().get::<Unit>(*e).unwrap(); (u.x, u.y, u.carrier.is_some(), u.moves, u.path.len()) }).collect();
        assert!(struck, "the landing party never struck the city; ship at {:?} {:?} unload {:?} turn {}", app.world().get::<Unit>(ship).map(|u| (u.x, u.y, u.moves, u.path.len())), dbg, app.world().resource::<crate::unit_picker::UnitPicker>().unload, app.world().resource::<Turn>().0);
    }
}
