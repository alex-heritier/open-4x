//! Ship movement, cargo and owner-turn hazards (`movement.md` 2, `unit-turn.md` 3).
use bevy::prelude::*;

use crate::combat::CombatRng;
use crate::features::{MessageBoard, post};
use crate::map::{Base, GameMap, MP};
use crate::units::{Unit, UnitType, def};

/// Every ship on the tile that could carry the passenger, oldest first
/// (`0x5C5F70`: the candidates it lists).
pub fn carriers(passenger: &Unit, at: (i32, i32), units: &[(Entity, Unit)]) -> Vec<Entity> {
    if def(passenger.utype).class != 0 { return vec![]; }
    let mut list: Vec<Entity> = units.iter().filter(|(e, u)| {
        let d = def(u.utype);
        u.civ == passenger.civ && (u.x, u.y) == at && u.carrier.is_none()
            && d.class == 1 && d.capacity > 0 && d.abilities & ((1 << 8) | (1 << 24)) == 0
            && (d.abilities & (1 << 14) == 0 || def(passenger.utype).abilities & (1 << 1) != 0)
            && units.iter().filter(|(_, v)| v.carrier == Some(*e)).count() < d.capacity as usize
    }).map(|(e, _)| *e).collect();
    list.sort();
    list
}

/// The oldest carrier: what the computer takes (`0x5C5F70` with `dialog == 0`
/// returns the first eligible candidate).
pub fn pick_carrier(passenger: &Unit, at: (i32, i32), units: &[(Entity, Unit)]) -> Option<Entity> {
    carriers(passenger, at, units).first().copied()
}

/// What boarding a tile of ships comes to.
pub enum Boarding {
    /// Nothing there can take the unit.
    None,
    Ship(Entity),
    /// A human has to choose: the question is up.
    Asking,
}

/// `0x5C5F70` with the dialog on: a human gets a list when several ships
/// qualify, and one candidate is taken without asking.
pub fn choose_carrier(
    e: Entity,
    passenger: &Unit,
    at: (i32, i32),
    units: &[(Entity, Unit)],
    ask: &mut Option<crate::diplomacy::BoardAsk>,
    load: bool,
) -> Boarding {
    let list = carriers(passenger, at, units);
    match list.len() {
        0 => Boarding::None,
        1 => Boarding::Ship(list[0]),
        _ if crate::civs::is_ai(passenger.civ) => Boarding::Ship(list[0]),
        _ => match ask {
            Some(a) if a.unit == e && a.at == at => match a.answer {
                Some(ship) if list.contains(&ship) => {
                    *ask = None;
                    Boarding::Ship(ship)
                }
                Some(_) => {
                    *ask = None;
                    Boarding::None
                }
                None => Boarding::Asking,
            },
            // Another question is up: this one waits its turn.
            Some(_) => Boarding::Asking,
            None => {
                let options = list
                    .iter()
                    .map(|&s| {
                        let ship = units.iter().find(|(id, _)| *id == s).map(|(_, v)| v);
                        let aboard = units.iter().filter(|(_, v)| v.carrier == Some(s)).count();
                        let name = ship.map_or("Ship", |v| def(v.utype).name);
                        let room = ship.map_or(0, |v| def(v.utype).capacity);
                        (s, format!("{name} ({aboard}/{room})"))
                    })
                    .collect();
                *ask = Some(crate::diplomacy::BoardAsk { unit: e, at, options, load, answer: None });
                Boarding::Asking
            }
        },
    }
}

/// Native refusal gate 0x5B5DDC..0x5B5E22 and cargo landing gate
/// 0x5C539C..0x5C53C5. A port is land even while the unit has a carrier link.
pub fn can_attack_from(map: &GameMap, u: &Unit) -> bool {
    let d = def(u.utype);
    if d.class != 0 || !map.get(u.x, u.y).is_some_and(|t| crate::improvements::is_water_base(t.base)) {
        return true;
    }
    d.abilities & (1 << 6) != 0 && d.attack > 0
        && (!u.attacked || d.abilities & crate::roster::ability::BLITZ != 0)
}

pub fn route(map: &GameMap, u: &Unit, dest: (i32, i32), ports: &[(i32, i32)], units: &[(Entity, Unit)]) -> Option<Vec<(i32, i32)>> {
    // 0x5B91C5..0x5B91EC: a ship's last step onto shore requests disembark.
    let shore = def(u.utype).class == 1 && !ports.contains(&dest)
        && map.get(dest.0, dest.1).is_some_and(|t| crate::map::move_cost(t).is_some());
    if (u.x, u.y) != dest && (shore || pick_carrier(u, dest, units).is_some()) {
        map.find_path_by((u.x, u.y), dest, |from, to| {
            if to == dest { Some(MP) } else { crate::units::entry_cost(map, u, from, to, ports) }
        })
    } else { crate::units::route(map, u, dest, ports) }
}

pub fn order_move(map: &GameMap, u: &mut Unit, dest: (i32, i32), ports: &[(i32, i32)], units: &[(Entity, Unit)]) {
    crate::units::order_move(map, u, dest, ports);
    u.path = route(map, u, dest, ports, units).unwrap_or_default().into();
    if !u.path.is_empty() { u.fortified = false; u.sentry = false; }
}

/// 0x5C5821 / 0x5C5924 -> 0x5C59B0: shore choice goes through the mover;
/// same-tile port choice detaches without charging movement.
pub fn disembark(map: &crate::map::GameMap, u: &mut Unit, shore: Option<(i32, i32)>) -> bool {
    if let Some(dest) = shore {
        if u.moves == 0 || !map.neighbors(u.x, u.y).contains(&dest) { return false; }
        crate::units::order_move(map, u, dest, &[]);
        if u.path.is_empty() { return false; }
    } else {
        u.carrier = None;
        u.path.clear();
    }
    u.fortified = false;
    u.sentry = false;
    u.exploring = false;
    u.auto = false;
    true
}

/// 0x4D8B70 -> 0x5C5110: set carrier/order, no movement charge.
fn load_onto(u: &mut Unit, carrier: Entity) {
    u.carrier = Some(carrier);
    u.sentry = true;
    u.fortified = false;
    u.path.clear();
    u.work = None;
    u.exploring = false;
    u.auto = false;
}

pub fn cargo_commands(
    mut commands: MessageReader<crate::actionbar::UnitCommand>,
    selected: Res<crate::units::Selected>,
    mut picker: ResMut<crate::unit_picker::UnitPicker>,
    civs: Res<crate::civs::Civilizations>,
    map: Res<GameMap>,
    mut units: Query<(Entity, &mut Unit)>,
    mut board: ResMut<MessageBoard>,
    mut diplomacy: ResMut<crate::diplomacy::Diplomacy>,
) {
    use crate::actionbar::UnitCommand;
    // The human's pick for a Load order that found several ships.
    if let Some(ask) = diplomacy.board_ask.clone()
        && ask.load
        && let Some(ship) = ask.answer
    {
        diplomacy.board_ask = None;
        if let Ok((_, mut u)) = units.get_mut(ask.unit)
            && u.carrier.is_none()
        {
            load_onto(&mut u, ship);
        }
    }
    for cmd in commands.read() {
        if !matches!(cmd, UnitCommand::Load | UnitCommand::Unload) { continue; }
        let Some(e) = selected.0 else { continue };
        let Ok((_, u)) = units.get(e) else { continue };
        if u.civ != civs.active || u.moves == 0 || !cmd.relevant(u.utype) { continue; }
        let at = (u.x, u.y);
        if *cmd == UnitCommand::Load {
            if u.carrier.is_some() { continue; }
            let snapshot: Vec<_> = units.iter().map(|(e, u)| (e, u.clone())).collect();
            // An Army on the tile takes the soldier (`army::commands`).
            if crate::army::joinable(u, &snapshot).is_some() { continue; }
            match choose_carrier(e, u, at, &snapshot, &mut diplomacy.board_ask, true) {
                Boarding::Ship(carrier) => load_onto(&mut units.get_mut(e).unwrap().1, carrier),
                Boarding::None => post(&mut board, "No friendly transport has room here."),
                Boarding::Asking => {}
            }
        } else if map.get(at.0, at.1).is_some_and(|t| !crate::improvements::is_water_base(t.base)) {
            // 0x5C1C45: Unload is a port command. The native UI lets the
            // human choose one passenger, including an exhausted passenger.
            if units.iter().any(|(_, u)| u.carrier == Some(e)) { picker.unload = Some(e); }
        }
    }
}

/// 0x5BD54C: cargo follows its carrier. Missing carriers kill cargo at sea;
/// in a city the kill routine detaches it instead (0x5BC041..0x5BC09D).
pub fn sync_cargo(mut commands: Commands, map: Res<GameMap>, mut units: Query<(Entity, &mut Unit)>) {
    let ships: std::collections::HashMap<_, _> = units.iter().filter(|(_, u)| def(u.utype).capacity > 0)
        .map(|(e, u)| (e, (u.x, u.y))).collect();
    for (e, mut u) in units.iter_mut() {
        let Some(carrier) = u.carrier else { continue };
        if let Some(&(x, y)) = ships.get(&carrier) {
            if (u.x, u.y) != (x, y) {
                u.x = x;
                u.y = y;
                u.sentry = true;
                u.fortified = false;
                u.rested = false;
                u.path.clear();
                u.anim = crate::units::UnitAnim::Idle { t: 0.0 };
            }
        } else if map.get(u.x, u.y).is_some_and(|t| crate::improvements::is_water_base(t.base)) {
            commands.entity(e).despawn();
        } else {
            u.carrier = None;
            u.sentry = false;
        }
    }
}

/// The flags of the great wonders `civ` holds in effect (`BLDG +0xF4`).
pub fn wonders(civ: usize) -> u32 {
    crate::realm::read(civ, |r| crate::roster::BLDGS.iter().enumerate()
        .filter(|(i, b)| r.owned.get(*i).copied().unwrap_or(0) > 0
            && (b.obsolete < 0 || !r.knows(b.obsolete))
            && (b.govt < 0 || b.govt == r.govt as i32))
        .fold(0, |flags, (_, b)| flags | b.wonder))
}

/// 0x5CDDF0: Seafaring and the two ship-movement wonder flags are additive.
pub fn moves(t: UnitType, civ: usize) -> u8 {
    let d = def(t);
    let mut m = d.moves * MP;
    if d.class == 1 {
        let w = wonders(civ);
        m += MP * u8::from(w & 8 != 0);
        m += 2 * MP * u8::from(w & 0x4000 != 0);
        m += MP * u8::from(crate::civs::RACES[civ].traits & (1 << 7) != 0);
    }
    m
}

fn sinking_die(base: Base, abilities: u32, tech_flags: u32, safe_sea: bool, seafaring: bool) -> Option<u32> {
    let sea = base == Base::Sea && abilities & (1 << 11) != 0 && tech_flags & 0x2000 == 0 && !safe_sea;
    let ocean = base == Base::Ocean && abilities & (1 << 12) != 0 && tech_flags & 0x4000 == 0;
    (sea || ocean).then_some(if seafaring { 4 } else { 2 })
}

/// Unit::turn's sea and jungle losses at the owner's turn boundary.
pub fn unit_hazards(
    mut ended: MessageReader<crate::civs::CivilizationEnded>,
    mut commands: Commands,
    units: Query<(Entity, &Unit)>,
    map: Res<GameMap>,
    civs: Res<crate::civs::Civilizations>,
    mut rng: ResMut<CombatRng>,
    mut board: ResMut<MessageBoard>,
) {
    // Scripted turn advances can deliver several boundaries in one frame;
    // despawns are deferred, so a lost unit must not roll again.
    let mut lost = std::collections::HashSet::new();
    for event in ended.read() {
        let civ = event.0;
        let rules = crate::rules_data::rules();
        let tech_flags = crate::realm::read(civ, |r| rules.techs.iter().enumerate()
            .filter(|(i, _)| r.knows(*i as i32)).fold(0, |flags, (_, t)| flags | t.flags));
        let safe_sea = wonders(civ) & 1 != 0;
        let seafaring = crate::civs::RACES[civ].traits & (1 << 7) != 0;
        for (e, u) in units.iter().filter(|(_, u)| u.civ == civ && u.carrier.is_none()) {
            if lost.contains(&e) { continue; }
            let Some(t) = map.get(u.x, u.y) else { continue };
            if let Some(die) = sinking_die(t.base, def(u.utype).abilities, tech_flags, safe_sea, seafaring)
                && rng.0.below(die) == 0
            {
                commands.entity(e).despawn();
                lost.insert(e);
                if civ == civs.viewer() { post(&mut board, format!("Our {} has sunk in unsafe waters!", def(u.utype).name)); }
                continue;
            }
            if civ3mapgen::disease::unit_jungle_loss(false, def(u.utype).pop_cost,
                u.fortified, crate::map::terrain_row(t), |n| rng.0.below(n))
            {
                commands.entity(e).despawn();
                lost.insert(e);
                if civ == civs.viewer() { post(&mut board, format!("Our {} has died from disease in the jungle!", def(u.utype).name)); }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cities::{City, Production};

    #[test]
    fn several_ships_make_a_human_choose_and_the_computer_take_the_oldest() {
        crate::civs::set_controllers();
        let mut w = World::new();
        let (a, b, p) = (w.spawn_empty().id(), w.spawn_empty().id(), w.spawn_empty().id());
        let ships = vec![
            (b, Unit::new(0, UnitType::Galley, 5, 5)),
            (a, Unit::new(0, UnitType::Galley, 5, 5)),
        ];
        let passenger = Unit::new(0, UnitType::Warrior, 5, 5);
        let mut ask = None;
        assert!(matches!(choose_carrier(p, &passenger, (5, 5), &ships, &mut ask, false), Boarding::Asking));
        let shown = ask.as_ref().unwrap();
        let first = a.min(b);
        assert_eq!(shown.options.iter().map(|(e, _)| *e).collect::<Vec<_>>(), vec![first, a.max(b)], "the entity order");
        assert!(shown.options[0].1.contains("(0/2)"));
        // No answer yet: still asking, and the same question stands.
        assert!(matches!(choose_carrier(p, &passenger, (5, 5), &ships, &mut ask, false), Boarding::Asking));
        ask.as_mut().unwrap().answer = Some(b);
        assert!(matches!(choose_carrier(p, &passenger, (5, 5), &ships, &mut ask, false), Boarding::Ship(s) if s == b));
        assert!(ask.is_none());
        // One ship needs no question, and the computer never asks.
        assert!(matches!(choose_carrier(p, &passenger, (5, 5), &ships[..1], &mut ask, false), Boarding::Ship(s) if s == b));
        let ai = Unit::new(1, UnitType::Warrior, 5, 5);
        let theirs = vec![(b, Unit::new(1, UnitType::Galley, 5, 5)), (a, Unit::new(1, UnitType::Galley, 5, 5))];
        assert!(matches!(choose_carrier(p, &ai, (5, 5), &theirs, &mut ask, false), Boarding::Ship(s) if s == first));
        assert!(ask.is_none());
    }

    fn cargo_app() -> App {
        crate::realm::reset();
        let mut map = GameMap::generate();
        for t in &mut map.tiles { t.base = Base::Grassland; }
        for x in 2..=4 {
            let i = map.idx(x, 1);
            map.tiles[i].base = Base::Coast;
        }
        let mut app = App::new();
        app.insert_resource(map);
        app.init_resource::<crate::civs::Civilizations>();
        app.init_resource::<crate::units::Selected>();
        app.init_resource::<crate::unit_picker::UnitPicker>();
        app.init_resource::<MessageBoard>();
        app.insert_resource(crate::diplomacy::Diplomacy::new());
        app.init_resource::<crate::cities::Treasury>();
        app.add_message::<crate::combat::AttackOrder>();
        app.add_message::<crate::actionbar::UnitCommand>();
        app.add_systems(Update, (cargo_commands, crate::units::drive_movement, sync_cargo, crate::units::auto_select).chain());
        app
    }

    #[test]
    fn boarding_reserves_capacity_and_cargo_follows_then_lands_without_movement() {
        let mut app = cargo_app();
        assert_eq!(def(UnitType::Galley).capacity, 2);
        let ship = app.world_mut().spawn(Unit::new(0, UnitType::Galley, 2, 1)).id();
        let passengers: Vec<_> = [UnitType::Settler, UnitType::Worker, UnitType::Warrior].into_iter().map(|t| {
            let mut u = Unit::new(0, t, 1, 1);
            u.path.push_back((2, 1));
            app.world_mut().spawn(u).id()
        }).collect();
        app.update();
        let loaded: Vec<_> = passengers.iter().copied().filter(|e| app.world().get::<Unit>(*e).unwrap().carrier == Some(ship)).collect();
        assert_eq!(loaded.len(), 2, "simultaneous orders cannot overfill a ship");
        let refused = passengers.iter().find(|e| !loaded.contains(e)).unwrap();
        assert_eq!(app.world().get::<Unit>(*refused).unwrap().moves, MP);
        assert_eq!(app.world().get::<Unit>(*refused).unwrap().x, 1);
        app.world_mut().get_mut::<Unit>(ship).unwrap().path.push_back((3, 1));
        app.update();
        for &e in &loaded {
            let u = app.world().get::<Unit>(e).unwrap();
            assert_eq!((u.x, u.y, u.moves), (3, 1, 0));
            assert!(u.sentry);
        }
        let cargo = loaded[0];
        {
            let mut u = app.world_mut().get_mut::<Unit>(cargo).unwrap();
            u.moves = MP;
            u.anim = crate::units::UnitAnim::Idle { t: 0.0 };
            u.path.push_back((3, 2));
        }
        app.update();
        let u = app.world().get::<Unit>(cargo).unwrap();
        assert_eq!((u.x, u.y, u.moves, u.carrier), (3, 2, 0, None));
        assert_eq!(app.world().get::<Unit>(loaded[1]).unwrap().carrier, Some(ship));
    }

    #[test]
    fn port_commands_load_and_unload_and_refuse_foreign_transports() {
        use crate::actionbar::UnitCommand;
        let mut app = cargo_app();
        let foreign = app.world_mut().spawn(Unit::new(1, UnitType::Galley, 1, 1)).id();
        let cargo = app.world_mut().spawn(Unit::new(0, UnitType::Settler, 1, 1)).id();
        app.world_mut().resource_mut::<crate::units::Selected>().0 = Some(cargo);
        app.world_mut().write_message(UnitCommand::Load);
        app.update();
        assert_eq!(app.world().get::<Unit>(cargo).unwrap().carrier, None);
        let ship = app.world_mut().spawn(Unit::new(0, UnitType::Galley, 1, 1)).id();
        app.world_mut().write_message(UnitCommand::Load);
        app.update();
        assert_eq!(app.world().get::<Unit>(cargo).unwrap().carrier, Some(ship));
        assert_eq!(app.world().get::<Unit>(cargo).unwrap().moves, MP);
        app.world_mut().resource_mut::<crate::units::Selected>().0 = Some(ship);
        app.world_mut().get_mut::<Unit>(ship).unwrap().moves = 0;
        app.world_mut().write_message(UnitCommand::Unload);
        app.update();
        assert_eq!(app.world().get::<Unit>(cargo).unwrap().carrier, Some(ship));
        app.world_mut().get_mut::<Unit>(ship).unwrap().moves = MP;
        app.world_mut().resource_mut::<crate::units::Selected>().0 = Some(ship);
        app.world_mut().write_message(UnitCommand::Unload);
        app.update();
        assert_eq!(app.world().get::<Unit>(cargo).unwrap().carrier, Some(ship));
        assert_eq!(app.world().resource::<crate::unit_picker::UnitPicker>().unload, Some(ship));
        assert!(app.world().get::<Unit>(foreign).is_some());
    }

    #[test]
    fn sea_unload_is_refused_but_a_manually_selected_passenger_can_land() {
        let mut app = cargo_app();
        app.insert_resource(crate::render::RevealAll(true));
        app.add_systems(Update, crate::units::unit_visibility.after(crate::units::auto_select));
        let ship = app.world_mut().spawn((Unit::new(0, UnitType::Galley, 2, 1), Visibility::Visible)).id();
        let mut u = Unit::new(0, UnitType::Settler, 2, 1);
        u.carrier = Some(ship);
        u.sentry = true;
        let cargo = app.world_mut().spawn((u, Visibility::Visible)).id();
        app.world_mut().resource_mut::<crate::units::Selected>().0 = Some(ship);
        app.world_mut().write_message(crate::actionbar::UnitCommand::Unload);
        app.update();
        assert_eq!(app.world().resource::<crate::units::Selected>().0, Some(ship));
        assert!(app.world().resource::<crate::unit_picker::UnitPicker>().unload.is_none());
        assert_eq!(app.world().get::<Unit>(cargo).unwrap().carrier, Some(ship));
        assert_eq!(*app.world().get::<Visibility>(cargo).unwrap(), Visibility::Hidden);
        assert_eq!(*app.world().get::<Visibility>(ship).unwrap(), Visibility::Visible);
        // The right-click picker wakes this passenger without detaching it.
        app.world_mut().resource_mut::<crate::units::Selected>().0 = Some(cargo);
        app.world_mut().get_mut::<Unit>(cargo).unwrap().sentry = false;
        app.world_mut().get_mut::<Unit>(cargo).unwrap().path.push_back((2, 2));
        app.update();
        assert_eq!(app.world().get::<Unit>(cargo).unwrap().carrier, None);
        assert_eq!(*app.world().get::<Visibility>(cargo).unwrap(), Visibility::Visible);
    }

    #[test]
    fn destroying_a_ship_kills_only_its_sea_cargo_and_releases_port_cargo() {
        for x in [1, 2] {
            let mut app = cargo_app();
            let ship = app.world_mut().spawn(Unit::new(0, UnitType::Galley, x, 1)).id();
            let mut u = Unit::new(0, UnitType::Settler, x, 1);
            u.carrier = Some(ship);
            let cargo = app.world_mut().spawn(u).id();
            let bystander = app.world_mut().spawn(Unit::new(0, UnitType::Worker, x, 1)).id();
            app.world_mut().despawn(ship);
            app.update();
            if x == 2 { assert!(app.world().get::<Unit>(cargo).is_none()); }
            else { assert_eq!(app.world().get::<Unit>(cargo).unwrap().carrier, None); }
            assert!(app.world().get::<Unit>(bystander).is_some());
        }
    }

    #[test]
    fn loaded_units_do_not_defend_or_attack_from_the_ship() {
        let mut app = cargo_app();
        let ship = app.world_mut().spawn(Unit::new(0, UnitType::Galley, 2, 1)).id();
        let mut u = Unit::new(0, UnitType::Warrior, 2, 1);
        u.carrier = Some(ship);
        u.path.push_back((2, 2));
        let cargo = app.world_mut().spawn(u).id();
        app.world_mut().spawn(Unit::new(1, UnitType::Warrior, 2, 2));
        app.update();
        let u = app.world().get::<Unit>(cargo).unwrap();
        assert_eq!((u.x, u.y, u.moves, u.carrier), (2, 1, MP, Some(ship)));
        assert!(u.path.is_empty());
        assert!(!crate::units::needs_orders(u));
        assert_eq!(crate::actions::check(crate::actionbar::UnitCommand::Automate, app.world().resource::<GameMap>(), u, None), Some(false));
        assert_eq!(crate::actions::check(crate::actionbar::UnitCommand::Goto, app.world().resource::<GameMap>(), u, None), None);
        assert_eq!(app.world().resource::<Messages<crate::combat::AttackOrder>>().len(), 0);
        let stack = [(cargo, u.clone())];
        assert!(crate::combat::pick_defender(app.world().resource::<GameMap>(), &Unit::new(1, UnitType::Warrior, 2, 2), None, &stack).is_none());
        assert!(!crate::actionbar::UnitCommand::Work(crate::improvements::WorkAction::Road).enabled(app.world().resource::<GameMap>(), &[], u));
    }

    #[test]
    fn boarding_preview_requires_a_friendly_carrier_with_room() {
        let map = GameMap::generate_with_seed(1);
        let from = (8, 7);
        let board = (8, 6);
        assert_eq!(map.get(from.0, from.1).unwrap().base, Base::Grassland);
        assert_eq!(map.get(board.0, board.1).unwrap().base, Base::Coast);
        let passenger = Unit::new(0, UnitType::Settler, from.0, from.1);
        let ship = Entity::from_bits(1);
        let mut units = vec![(ship, Unit::new(1, UnitType::Galley, board.0, board.1))];
        assert!(route(&map, &passenger, board, &[], &units).is_none());
        units[0].1.civ = 0;
        assert_eq!(route(&map, &passenger, board, &[], &units), Some(vec![board]));
        for id in 2..=3 {
            let mut u = passenger.clone();
            u.carrier = Some(ship);
            units.push((Entity::from_bits(id), u));
        }
        assert!(route(&map, &passenger, board, &[], &units).is_none());
    }

    #[test]
    fn galleys_require_a_coastal_city_and_follow_water_routes_between_ports() {
        crate::realm::reset();
        crate::research::set_trainable(0, UnitType::Galley.0 as usize, true);
        let mut port = City::new(0, "Port", 1, 1);
        assert!(!port.can_build_here(Production::Galley));
        port.coastal = true;
        assert!(port.can_build_here(Production::Galley));
        let mut map = GameMap::generate();
        for t in &mut map.tiles { t.base = Base::Grassland; }
        for x in 1..=20 { let i = map.idx(x, 10); map.tiles[i].base = Base::Coast; }
        assert!(!map.coastal_site(0, 10), "a lake of 20 tiles cannot train ships");
        let i = map.idx(21, 10); map.tiles[i].base = Base::Coast;
        assert!(map.coastal_site(0, 10));
        for (x, base) in [(2, Base::Coast), (3, Base::Sea), (4, Base::Ocean)] {
            let i = map.idx(x, 1);
            map.tiles[i].base = base;
        }
        let mut galley = Unit::new(0, UnitType::Galley, 1, 1);
        let ports = [(1, 1), (5, 1)];
        crate::units::order_move(&map, &mut galley, (5, 1), &ports);
        assert_eq!(galley.path.iter().copied().collect::<Vec<_>>(), [(2, 1), (3, 1), (4, 1), (5, 1)]);
        assert!(crate::units::route(&map, &galley, (6, 1), &ports).is_none());
        let warrior = Unit::new(0, UnitType::Warrior, 1, 1);
        assert!(crate::units::route(&map, &warrior, (2, 1), &ports).is_none());
        for &to in &galley.path {
            assert_eq!(crate::units::entry_cost(&map, &galley, (galley.x, galley.y), to, &ports), Some(MP));
        }
        let mut app = App::new();
        app.insert_resource(map);
        app.init_resource::<crate::civs::Civilizations>();
        app.insert_resource(crate::diplomacy::Diplomacy::new());
        app.init_resource::<crate::cities::Treasury>();
        app.add_message::<crate::combat::AttackOrder>();
        app.init_resource::<crate::unit_picker::UnitPicker>();
        app.add_systems(Update, crate::units::drive_movement);
        app.world_mut().spawn(port);
        let mut destination = City::new(0, "Destination", 5, 1);
        destination.coastal = true;
        app.world_mut().spawn(destination);
        let e = app.world_mut().spawn(galley).id();
        for x in 2..=4 {
            app.update();
            let mut u = app.world_mut().get_mut::<Unit>(e).unwrap();
            assert_eq!((u.x, u.y, u.moves), (x, 1, (4 - x) as u8 * MP));
            u.anim = crate::units::UnitAnim::Idle { t: 0.0 };
        }
        app.update(); // no movement remains, so entering the port waits
        assert_eq!(app.world().get::<Unit>(e).unwrap().x, 4);
        app.world_mut().get_mut::<Unit>(e).unwrap().moves = 3 * MP;
        app.update();
        let u = app.world().get::<Unit>(e).unwrap();
        assert_eq!((u.x, u.y, u.moves), (5, 1, 2 * MP));
    }

    #[test]
    fn jungle_disease_kills_only_eligible_outgoing_units_and_preserves_rng_skips() {
        for seed in [0, 1] {
            crate::realm::reset();
            let mut map = GameMap::generate();
            for x in 10..=16 {
                let i = map.idx(x, 10);
                map.tiles[i].base = Base::Grassland;
                map.tiles[i].relief = crate::map::Relief::Flat;
                map.tiles[i].cover = if x == 15 { crate::map::Cover::Bare } else { crate::map::Cover::Jungle };
            }
            let mut app = App::new();
            app.insert_resource(map);
            app.init_resource::<crate::civs::Civilizations>();
            app.init_resource::<MessageBoard>();
            app.insert_resource(CombatRng(crate::rng::MapRng::new(seed)));
            app.add_message::<crate::civs::CivilizationEnded>();
            app.add_systems(Update, (unit_hazards, sync_cargo).chain());
            let mut warrior = Unit::new(0, UnitType::Warrior, 10, 10);
            warrior.fortified = true;
            let exposed = app.world_mut().spawn(warrior.clone()).id();
            let mut immune = Vec::new();
            for (x, kind) in [(11, UnitType::Settler), (12, UnitType::Worker)] {
                let mut u = Unit::new(0, kind, x, 10);
                assert_ne!(def(kind).pop_cost, 0);
                u.fortified = true;
                immune.push(app.world_mut().spawn(u).id());
            }
            warrior.x = 13;
            warrior.fortified = false;
            immune.push(app.world_mut().spawn(warrior.clone()).id());
            warrior.x = 14;
            warrior.fortified = true;
            let carrier = app.world_mut().spawn(Unit::new(0, UnitType::Galley, 14, 10)).id();
            immune.push(carrier);
            warrior.carrier = Some(carrier);
            immune.push(app.world_mut().spawn(warrior.clone()).id());
            warrior.x = 15;
            warrior.carrier = None;
            immune.push(app.world_mut().spawn(warrior.clone()).id());
            warrior.x = 16;
            warrior.civ = 1;
            immune.push(app.world_mut().spawn(warrior).id());
            let mut reference = crate::rng::MapRng::new(seed);
            let lost = reference.below(1000) == 0;
            assert_eq!(lost, seed == 0);
            app.world_mut().write_message(crate::civs::CivilizationEnded(0));
            if lost { app.world_mut().write_message(crate::civs::CivilizationEnded(0)); }
            app.update();
            assert_eq!(app.world().get::<Unit>(exposed).is_none(), lost);
            for e in immune { assert!(app.world().get::<Unit>(e).is_some()); }
            assert_eq!(app.world().resource::<CombatRng>().0.state(), reference.state());
            assert_eq!(app.world().resource::<MessageBoard>().text.contains("disease"), lost);
        }
    }

    #[test]
    fn unsafe_water_losses_match_the_gameplay_rng_and_only_the_outgoing_owner_rolls() {
        for seed in 0..30 {
            crate::realm::reset();
            let mut map = GameMap::generate();
            for (x, base) in [(10, Base::Sea), (11, Base::Coast)] {
                let i = map.idx(x, 10); map.tiles[i].base = base;
            }
            let mut app = App::new();
            app.insert_resource(map);
            app.init_resource::<crate::civs::Civilizations>();
            app.init_resource::<MessageBoard>();
            app.insert_resource(CombatRng(crate::rng::MapRng::new(seed)));
            app.add_message::<crate::civs::CivilizationEnded>();
            app.add_systems(Update, (unit_hazards, sync_cargo).chain());
            let unsafe_ship = app.world_mut().spawn(Unit::new(0, UnitType::Galley, 10, 10)).id();
            let mut cargo = Unit::new(0, UnitType::Settler, 10, 10);
            cargo.carrier = Some(unsafe_ship);
            let cargo = app.world_mut().spawn(cargo).id();
            let coast = app.world_mut().spawn(Unit::new(0, UnitType::Galley, 11, 10)).id();
            let foreign = app.world_mut().spawn(Unit::new(1, UnitType::Galley, 10, 10)).id();
            let mut reference = crate::rng::MapRng::new(seed);
            let lost = reference.below(2) == 0;
            app.world_mut().write_message(crate::civs::CivilizationEnded(0));
            app.update();
            assert_eq!(app.world().get::<Unit>(unsafe_ship).is_none(), lost);
            assert_eq!(app.world().get::<Unit>(cargo).is_none(), lost);
            assert!(app.world().get::<Unit>(coast).is_some());
            assert!(app.world().get::<Unit>(foreign).is_some());
            assert_eq!(app.world_mut().resource_mut::<CombatRng>().0.below(1024), reference.below(1024));
        }
    }

    #[test]
    fn safe_sea_travel_technology_wonders_and_seafaring_change_the_hazard() {
        let abilities = (1 << 11) | (1 << 12);
        assert_eq!(sinking_die(Base::Sea, abilities, 0, false, true), Some(4));
        assert_eq!(sinking_die(Base::Sea, abilities, 0x2000, false, false), None);
        assert_eq!(sinking_die(Base::Sea, abilities, 0, true, false), None);
        assert_eq!(sinking_die(Base::Ocean, abilities, 0, true, false), Some(2));
        assert_eq!(sinking_die(Base::Ocean, abilities, 0x4000, false, false), None);
        crate::realm::reset();
        let row = Production::TheGreatLighthouse.building_row().unwrap();
        crate::realm::write(0, |r| r.owned[row] = 1);
        assert_eq!(moves(UnitType::Galley, 0), 4 * MP);
        let obsolete = crate::roster::BLDGS[row].obsolete;
        crate::realm::write(0, |r| r.known |= 1 << obsolete);
        assert_eq!(moves(UnitType::Galley, 0), 3 * MP);
        assert_eq!(moves(UnitType::Warrior, 0), MP);
    }
}
