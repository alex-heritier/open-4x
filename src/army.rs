//! Great Leaders and Armies.
//!
//! A Great Leader comes from an elite land unit's victory (`combat.md` 6.2
//! step 3): one die of 16, 12 for the owner of the Heroic Epic, twice that
//! when the winner was defending, for a unit that has not made a leader
//! before, against a civilization, while its civ has no Leader alive. The
//! Leader is never built (`buildable.md` 3 step 5).
//!
//! In a city a Leader can build an Army or hurry the city's production
//! (`labels.txt` `#UNIT_ACTIONS`: "(B)uild army", "(H)urry city
//! production"); either way the Leader is used up. An Army carries up to
//! its capacity of land soldiers. Its strength is its members' rounded
//! average plus a sixth of their sum, its hit points their sum, and it
//! moves at the slowest member's speed plus one (`combat.md` 5,
//! `movement.md` 3). Civ3 refuses to unload an army (`movement.md` 9), so a
//! member is folded into the Army unit and shares its fate.
//!
//! HYPOTHESIS: the two Leader actions' executors are not decoded (the
//! Leader strategy handler `0x45FED0`, `unit-ai.md` 5). Here building an
//! army needs a city and turns the Leader into an empty Army on the spot;
//! hurrying needs a city and completes its current build, wonders included,
//! as the manual says. Elite armies, the army's own experience and the
//! participant reselection inside an army (`combat.md` 6) are not modelled:
//! the Army fights as one unit with the summed hit points.

use crate::cities::City;
use crate::combat::Level;
use crate::roster::{ability, oth};
use crate::units::{Unit, UnitType, def};

/// `combat.md` 6.2 step 3, the parts read off the winner.
pub fn leader_eligible(u: &Unit, loser_barbarian: bool) -> bool {
    u.level == Level::Elite
        && def(u.utype).class == 0
        && def(u.utype).abilities & ability::ARMY == 0
        && !loser_barbarian
        && !u.made_leader
        && u.civ < crate::civs::civ_count()
}

/// The leader die: 16, or 12 with the Heroic Epic (BLDG `+0xF4` bit 0),
/// doubled when the defender won.
pub fn leader_die(civ: usize, defender_won: bool) -> u32 {
    let epic = crate::realm::read(civ, |r| {
        crate::roster::BLDGS.iter().enumerate().any(|(i, b)| {
            b.small & 1 != 0
                && b.other & oth::SMALL_WONDER != 0
                && r.owned.get(i).copied().unwrap_or(0) > 0
        })
    });
    let die = if epic { 12 } else { 16 };
    if defender_won { die * 2 } else { die }
}

pub fn is_army(t: UnitType) -> bool {
    def(t).abilities & ability::ARMY != 0
}

/// Can `u` join `army`: a land soldier of the same civ on its tile, into an
/// army with room.
pub fn can_join(u: &Unit, army: &Unit) -> bool {
    is_army(army.utype)
        && army.civ == u.civ
        && (army.x, army.y) == (u.x, u.y)
        && army.members.len() < def(army.utype).capacity as usize
        && def(u.utype).class == 0
        && def(u.utype).attack > 0
        && !is_army(u.utype)
        && u.utype != crate::roles::leader()
        && u.carrier.is_none()
}

/// Fold `u` into `army`: its experience and wounds come along.
pub fn join(army: &mut Unit, u: &Unit) {
    army.members.push((u.utype, u.level));
    army.damage += u.damage;
    // The army moves no further this turn than its slowest member could.
    army.moves = army.moves.min(u.moves);
}

/// Whether a Leader on `tile` stands in one of its civ's cities.
pub fn in_city<'a>(u: &Unit, cities: impl IntoIterator<Item = &'a City>) -> bool {
    cities
        .into_iter()
        .any(|c| c.civ == u.civ && (c.x, c.y) == (u.x, u.y))
}

/// The Leader completes the city's build: the box fills to its price.
pub fn hurry(city: &mut City) {
    city.shields = city.price(city.production);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn soldier(t: UnitType, level: Level) -> Unit {
        Unit {
            level,
            ..Unit::new(0, t, 3, 3)
        }
    }

    #[test]
    fn an_army_averages_its_members_and_adds_a_sixth() {
        let mut army = Unit::new(0, UnitType::named("Army"), 3, 3);
        army.moves = army.allowance();
        for t in [
            UnitType::named("Swordsman"),
            UnitType::named("Swordsman"),
            UnitType::named("Spearman"),
        ] {
            let u = soldier(t, Level::Regular);
            assert!(can_join(&u, &army));
            join(&mut army, &u);
        }
        // Attack 3, 3, 1: (7 + 1) / 3 = 2, bonus 7 / 6 = 1.
        assert_eq!((army.attack(), army.army_bonus(false)), (2, 1));
        // Defense 2, 2, 2: 2, bonus 1.
        assert_eq!((army.defense(), army.army_bonus(true)), (2, 1));
        // Three Regulars: 3 + 3 + 3 hit points.
        assert_eq!(army.max_hp(), 9);
        // Full at three.
        assert!(!can_join(
            &soldier(UnitType::named("Warrior"), Level::Regular),
            &army
        ));
    }

    #[test]
    fn an_army_moves_at_its_slowest_member_plus_one() {
        let mut army = Unit::new(0, UnitType::named("Army"), 3, 3);
        join(
            &mut army,
            &soldier(UnitType::named("Horseman"), Level::Regular),
        );
        join(
            &mut army,
            &soldier(UnitType::named("Warrior"), Level::Regular),
        );
        assert_eq!(army.allowance(), 2 * crate::map::MP);
    }

    #[test]
    fn only_an_elite_land_winner_that_never_made_one_can_make_a_leader() {
        let mut u = soldier(UnitType::named("Swordsman"), Level::Elite);
        assert!(leader_eligible(&u, false));
        assert!(!leader_eligible(&u, true));
        u.made_leader = true;
        assert!(!leader_eligible(&u, false));
        assert!(!leader_eligible(
            &soldier(UnitType::named("Swordsman"), Level::Veteran),
            false
        ));
        assert_eq!(leader_die(0, false), 16);
        assert_eq!(leader_die(0, true), 32);
    }

    #[test]
    fn only_a_scientific_leader_can_start_an_age_and_it_cannot_form_an_army() {
        let mut app = App::new();
        app.init_resource::<crate::units::Selected>();
        app.init_resource::<crate::civs::Civilizations>();
        app.init_resource::<MessageBoard>();
        app.insert_resource(crate::units::UnitArt::blank());
        app.insert_resource(crate::research::Research::new());
        app.insert_resource(crate::units::Turn(7));
        app.add_message::<UnitCommand>();
        app.add_systems(Update, commands);
        app.world_mut().spawn(City::new(0, "Kyoto", 3, 3));
        let mut u = Unit::new(0, UnitType::named("Leader"), 3, 3);
        let e = app.world_mut().spawn(u.clone()).id();
        app.world_mut().resource_mut::<crate::units::Selected>().0 = Some(e);
        app.world_mut().write_message(UnitCommand::ScienceAge);
        app.update();
        assert!(
            app.world().get::<Unit>(e).is_some(),
            "military leader is not consumed"
        );
        u.scientific_leader = true;
        u.moves = 0; // The native availability gate does not test movement.
        app.world_mut().entity_mut(e).insert(u);
        app.world_mut().write_message(UnitCommand::BuildArmy);
        app.update();
        assert!(app.world().get::<Unit>(e).is_some());
        app.world_mut().write_message(UnitCommand::ScienceAge);
        app.update();
        assert!(app.world().get::<Unit>(e).is_none());
        let research = app.world().resource::<crate::research::Research>();
        assert!(research.science_age(0, 27));
        assert!(!research.science_age(0, 28));
        // A second scientific leader remains available for later use.
        let mut u = Unit::new(0, UnitType::named("Leader"), 3, 3);
        u.scientific_leader = true;
        let other = app.world_mut().spawn(u).id();
        app.world_mut().resource_mut::<crate::units::Selected>().0 = Some(other);
        app.world_mut().write_message(UnitCommand::ScienceAge);
        app.update();
        assert!(app.world().get::<Unit>(other).is_some());
        app.world_mut().resource_mut::<crate::units::Turn>().0 = 28;
        app.world_mut().write_message(UnitCommand::LeaderHurry);
        app.update();
        assert!(app.world().get::<Unit>(other).is_none());
        let city = app
            .world_mut()
            .query::<&City>()
            .single(app.world())
            .unwrap();
        assert_eq!(city.shields, city.price(city.production));
    }

    #[test]
    fn computer_scientists_finish_a_wonder_then_start_an_age() {
        crate::civs::set_controllers();
        let mut app = App::new();
        app.edit_schedule(Update, |s| {
            s.set_executor_kind(bevy::ecs::schedule::ExecutorKind::SingleThreaded);
        });
        app.init_resource::<crate::civs::Civilizations>();
        app.world_mut()
            .resource_mut::<crate::civs::Civilizations>()
            .active = 1;
        app.insert_resource(crate::research::Research::new());
        app.insert_resource(crate::units::Turn(7));
        app.add_systems(Update, ai_science_leaders);
        let mut city = City::new(1, "Rome", 3, 3);
        city.production = crate::cities::Production::named("The Pyramids");
        app.world_mut().spawn(city);
        let mut scientist = Unit::new(1, UnitType::named("Leader"), 3, 3);
        scientist.scientific_leader = true;
        let first = app.world_mut().spawn(scientist.clone()).id();
        app.update();
        assert!(app.world().get::<Unit>(first).is_none());
        let city = app
            .world_mut()
            .query::<&City>()
            .single(app.world())
            .unwrap();
        assert_eq!(city.shields, city.price(city.production));
        let second = app.world_mut().spawn(scientist.clone()).id();
        app.update();
        assert!(app.world().get::<Unit>(second).is_none());
        assert!(
            app.world()
                .resource::<crate::research::Research>()
                .science_age(1, 27)
        );
        let third = app.world_mut().spawn(scientist).id();
        app.update();
        assert!(
            app.world().get::<Unit>(third).is_some(),
            "another leader waits until the age expires"
        );
    }
}

use bevy::prelude::*;

use crate::actionbar::UnitCommand;
use crate::features::{MessageBoard, post};

/// An Army of `u`'s civ on its tile that `u` may join.
pub fn joinable(u: &Unit, units: &[(Entity, Unit)]) -> Option<Entity> {
    units
        .iter()
        .filter(|(_, a)| can_join(u, a))
        .map(|(e, _)| *e)
        .min()
}

/// The Leader's two orders and Load into an Army.
#[allow(clippy::too_many_arguments)]
pub fn commands(
    mut commands: Commands,
    mut cmds: MessageReader<UnitCommand>,
    mut selected: ResMut<crate::units::Selected>,
    civs: Res<crate::civs::Civilizations>,
    art: Res<crate::units::UnitArt>,
    mut units: Query<(Entity, &mut Unit)>,
    mut cities: Query<&mut City>,
    mut board: ResMut<MessageBoard>,
    mut research: ResMut<crate::research::Research>,
    turn: Res<crate::units::Turn>,
) {
    for cmd in cmds.read().copied() {
        let Some(e) = selected.0 else { continue };
        let Ok((_, u)) = units.get(e) else { continue };
        if u.civ != civs.active || !cmd.relevant(u.utype) {
            continue;
        }
        let u = u.clone();
        match cmd {
            UnitCommand::ScienceAge => {
                if u.scientific_leader
                    && cities.iter().any(|c| (c.x, c.y) == (u.x, u.y))
                    && !research.science_age(u.civ, turn.0 as i32)
                {
                    research.start_science_age(u.civ, turn.0 as i32);
                    commands.entity(e).despawn();
                    selected.0 = None;
                    post(&mut board, "Our civilization has entered a Science Age!");
                }
            }
            UnitCommand::BuildArmy | UnitCommand::LeaderHurry if !in_city(&u, cities.iter()) => {
                post(
                    &mut board,
                    "A Leader must be in one of our cities to do that.",
                );
            }
            UnitCommand::BuildArmy => {
                if u.scientific_leader {
                    continue;
                }
                commands.entity(e).despawn();
                let a = crate::units::spawn_unit(
                    &mut commands,
                    &art,
                    UnitType::named("Army"),
                    u.x,
                    u.y,
                    u.civ,
                );
                commands
                    .entity(a)
                    .entry::<Unit>()
                    .and_modify(|mut a| a.moves = 0);
                selected.0 = None;
                post(&mut board, "Our Great Leader has formed an Army!");
            }
            UnitCommand::LeaderHurry => {
                if let Some(mut c) = cities.iter_mut().find(|c| (c.x, c.y) == (u.x, u.y)) {
                    hurry(&mut c);
                    post(
                        &mut board,
                        format!(
                            "Our Great Leader hurries the {} in {}!",
                            c.production.name(),
                            c.name
                        ),
                    );
                }
                commands.entity(e).despawn();
                selected.0 = None;
            }
            UnitCommand::Load => {
                let snapshot: Vec<(Entity, Unit)> =
                    units.iter().map(|(e, u)| (e, u.clone())).collect();
                if let Some(a) = joinable(&u, &snapshot)
                    && let Ok((_, mut army)) = units.get_mut(a)
                {
                    join(&mut army, &u);
                    commands.entity(e).despawn();
                    selected.0 = None;
                    post(
                        &mut board,
                        format!("Our {} joins the Army.", def(u.utype).name),
                    );
                }
            }
            _ => {}
        }
    }
}

/// HYPOTHESIS: computer policy prefers finishing a great wonder; otherwise
/// it uses a Scientific Leader for the native Science Age in its city.
pub fn ai_science_leaders(
    mut commands: Commands,
    civs: Res<crate::civs::Civilizations>,
    turn: Res<crate::units::Turn>,
    units: Query<(Entity, &Unit)>,
    mut cities: Query<&mut City>,
    mut research: ResMut<crate::research::Research>,
) {
    let civ = civs.active;
    if !crate::civs::is_ai(civ) || civs.outcome.is_some() {
        return;
    }
    for (e, u) in units
        .iter()
        .filter(|(_, u)| u.civ == civ && u.scientific_leader && u.carrier.is_none())
    {
        let Some(mut city) = cities
            .iter_mut()
            .find(|c| c.civ == civ && (c.x, c.y) == (u.x, u.y))
        else {
            continue;
        };
        if city.production.bldg().is_some_and(|b| b.is_great_wonder())
            && city.shields < city.price(city.production)
        {
            hurry(&mut city);
        } else if !research.science_age(civ, turn.0 as i32) {
            research.start_science_age(civ, turn.0 as i32);
        } else {
            continue;
        }
        commands.entity(e).despawn();
    }
}
