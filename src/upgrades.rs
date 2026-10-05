//! Gold upgrades: one unit (`U`), every unit of its type (`Shift+U`), and
//! the computer's habit of spending spare gold on them
//! (`reverse-engineering/unit-upgrades.md`; the arithmetic is
//! `civ3mapgen::upgrade`).
//!
//! A unit upgrades in a city, with the movement it has left, to the
//! furthest successor on its `upgrade_to` chain that its civ can train. The
//! city must hold the facility of the unit's domain (Barracks, Harbor,
//! Airport). The price is the RULE `upgrade_cost` per shield of difference,
//! halved by Leonardo's Workshop. The new unit keeps the tile, the
//! fortified order and (at most Veteran) the experience, is fully healed,
//! and has no movement left.

use bevy::prelude::*;
use civ3mapgen::upgrade as exe;

use crate::cities::{City, Production, Treasury};
use crate::civs::{Civilizations, is_ai};
use crate::combat::Level;
use crate::features::{MessageBoard, post};
use crate::realm;
use crate::roster::{self, bldg_count, special};
use crate::ruleset::upgrade_cost;
use crate::units::{Unit, UnitType, def};

/// What upgrading a unit would do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Offer {
    pub to: UnitType,
    pub cost: u32,
}

/// The unit a type becomes in a city of `civ`: the furthest type on its
/// upgrade chain the civ can train (`City::replacement`, `0x4C0690`).
pub fn replacement(civ: usize, t: UnitType) -> Option<UnitType> {
    exe::replacement(roster::upgrade_chain(t.0 as usize), |n| crate::research::can_train(civ, n))
        .map(|n| UnitType(n as u16))
}

/// A Halves-Upgrade-Cost wonder (Leonardo's Workshop) stands in one of the
/// civ's cities and is not obsolete (`countWonders(0x40)`).
pub fn halved(civ: usize) -> bool {
    realm::read(civ, |r| {
        (0..bldg_count()).any(|row| {
            let b = roster::bldg(row);
            r.owned[row] > 0
                && b.wonder & roster::wonder::HALVES_UPGRADE_COST != 0
                && !(b.obsolete >= 0 && r.knows(b.obsolete))
        })
    })
}

/// Gold to turn a `from` into a `to` for `civ` (`0x5C04D0`).
pub fn price(civ: usize, from: UnitType, to: UnitType) -> u32 {
    let cost = |t: UnitType| i32::from(Production::from_unit(t).cost());
    let level = is_ai(civ).then_some(crate::scenario::difficulty() as i32);
    exe::price(upgrade_cost(), cost(from), cost(to), halved(civ), level) as u32
}

/// Does the type have a successor and the Upgrade Unit ability at all?
pub fn upgradable(t: UnitType) -> bool {
    let row = t.row();
    row.special & special::UPGRADE_UNIT != 0 && row.upgrade_to >= 0
}

/// What upgrading `u` would do where it stands, in `city` (whoever owns
/// it): the facility of its domain is there and a successor can be
/// trained. Movement and gold are not asked (`0x5C0620` steps 2, 3, 5).
pub fn plan(u: &Unit, city: Option<&City>) -> Option<Offer> {
    if u.carrier.is_some() || !upgradable(u.utype) {
        return None;
    }
    let city = city?;
    let flag = exe::facility_flag(u.utype.row().class)?;
    if !crate::citycalc::has_flag(city, flag) {
        return None;
    }
    let to = replacement(city.civ, u.utype)?;
    Some(Offer { to, cost: price(u.civ, u.utype, to) })
}

/// Can `u` upgrade now, with `gold` in the treasury (`0x5C1AD0`, which
/// wants movement left, and `0x5C0620`)?
pub fn offer(u: &Unit, city: Option<&City>, gold: u32) -> Option<Offer> {
    if u.moves == 0 {
        return None;
    }
    plan(u, city).filter(|o| o.cost <= gold)
}

/// Make `u` the unit `to` (`0x5C0740`, here in place: the exe builds a new
/// unit and kills the old one). It keeps its tile and its fortified order
/// and at most a Veteran's experience; it is healed and out of movement.
pub fn apply(u: &mut Unit, to: UnitType) {
    u.utype = to;
    u.level = match exe::experience_after(u.level as i32) {
        0 => Level::Conscript,
        1 => Level::Regular,
        _ => Level::Veteran,
    };
    u.damage = 0;
    u.moves = 0;
    u.attacked = false;
    u.failed_promotion = false;
    u.sentry = false;
    u.exploring = false;
    u.work = None;
    u.path.clear();
}

/// Upgrade every unit of `civ` of type `t`, in turn and paying as it goes:
/// each is re-judged against what is left of the treasury, so a unit that
/// no longer fits is skipped (`Player::upgradeAll`, `0x56AF00`). Returns
/// how many were upgraded and the gold spent.
pub fn upgrade_all<'a>(
    civ: usize,
    t: UnitType,
    units: impl IntoIterator<Item = &'a mut Unit>,
    cities: &[&City],
    gold: &mut u32,
) -> (u32, u32) {
    let (mut count, mut spent) = (0, 0);
    for u in units.into_iter().filter(|u| u.civ == civ && u.utype == t) {
        let here = cities.iter().find(|c| (c.x, c.y) == (u.x, u.y)).copied();
        if let Some(o) = offer(u, here, *gold) {
            *gold -= o.cost;
            spent += o.cost;
            count += 1;
            apply(u, o.to);
        }
    }
    (count, spent)
}

/// How many units of `t` could upgrade now and what that would cost in all
/// (`Player::upgradeAllPrompt`, `0x56AAE0`), each judged alone.
pub fn tally<'a>(
    civ: usize,
    t: UnitType,
    units: impl IntoIterator<Item = &'a Unit>,
    cities: &[&City],
    gold: u32,
) -> (u32, u32) {
    units
        .into_iter()
        .filter(|u| u.civ == civ && u.utype == t)
        .filter_map(|u| offer(u, cities.iter().find(|c| (c.x, c.y) == (u.x, u.y)).copied(), gold))
        .fold((0, 0), |(n, sum), o| (n + 1, sum + o.cost))
}

/// The unit's name for messages.
fn name(t: UnitType) -> &'static str {
    def(t).name
}

/// Run an `Upgrade` command for the selected unit.
pub fn run_one(
    e: Entity,
    units: &mut Query<(Entity, &mut Unit)>,
    cities: &Query<&City>,
    treasury: &mut Treasury,
    board: &mut MessageBoard,
) -> bool {
    let Ok((_, mut u)) = units.get_mut(e) else { return false };
    let here = cities.iter().find(|c| (c.x, c.y) == (u.x, u.y));
    let Some(o) = offer(&u, here, treasury.0[u.civ]) else {
        post(board, "This unit cannot be upgraded here.");
        return false;
    };
    treasury.0[u.civ] -= o.cost;
    let from = u.utype;
    apply(&mut u, o.to);
    post(board, format!("{} upgraded to {} for {} gold.", name(from), name(o.to), o.cost));
    true
}

/// Run an `UpgradeAll` command for the type of the selected unit.
pub fn run_all(
    e: Entity,
    units: &mut Query<(Entity, &mut Unit)>,
    cities: &Query<&City>,
    treasury: &mut Treasury,
    board: &mut MessageBoard,
) {
    let Ok((_, sel)) = units.get(e) else { return };
    let (civ, t) = (sel.civ, sel.utype);
    let cs: Vec<&City> = cities.iter().collect();
    let (count, sum) = tally(civ, t, units.iter().map(|(_, u)| u), &cs, u32::MAX);
    if count == 0 {
        post(board, format!("We have not a single {} which can be upgraded!", name(t)));
        return;
    }
    if sum > treasury.0[civ] {
        post(board, format!("We would need {sum} gold in order to complete all the upgrades!"));
        return;
    }
    // The pool order of the exe is the entity order here.
    let mut order: Vec<Entity> = units.iter().map(|(e, _)| e).collect();
    order.sort();
    let mut gold = treasury.0[civ];
    let mut done = (0, 0);
    for id in order {
        if let Ok((_, mut u)) = units.get_mut(id) {
            let (n, spent) = upgrade_all(civ, t, std::iter::once(&mut *u), &cs, &mut gold);
            done = (done.0 + n, done.1 + spent);
        }
    }
    treasury.0[civ] = gold;
    post(board, format!("{} {} upgraded for {} gold.", done.0, name(t), done.1));
}

/// Gold the computer keeps back when it upgrades: a few turns' pay for its
/// cities' upkeep.
///
/// HYPOTHESIS: the exe calls `Unit::tryAutoUpgrade` from a planner this
/// project has not read (`unit-upgrades.md` 9), so when and with what
/// reserve is the clone's rule.
const AI_RESERVE: u32 = 60;

/// At the start of an AI turn, upgrade its units that stand in a city with
/// the right facility while the treasury stays above a reserve. As the exe
/// does, a unique unit that starts a Golden Age is left alone unless it is
/// a defender.
pub fn ai_turn(
    civs: Res<Civilizations>,
    turn: Res<crate::units::Turn>,
    splash: Res<crate::splash::SplashUp>,
    mut treasury: ResMut<Treasury>,
    cities: Query<&City>,
    mut units: Query<(Entity, &mut Unit)>,
    mut done: Local<Option<(usize, u32)>>,
) {
    let civ = civs.active;
    if !is_ai(civ) || civs.outcome.is_some() || splash.0 || *done == Some((civ, turn.0)) {
        return;
    }
    *done = Some((civ, turn.0));
    let cs: Vec<&City> = cities.iter().collect();
    let mut order: Vec<Entity> = units.iter().filter(|(_, u)| u.civ == civ).map(|(e, _)| e).collect();
    order.sort();
    for e in order {
        let Ok((_, mut u)) = units.get_mut(e) else { continue };
        let row = u.utype.row();
        let keeps = row.abilities & roster::ability::STARTS_GOLDEN_AGE != 0 && row.ai & 2 == 0;
        if keeps {
            continue;
        }
        let here = cs.iter().find(|c| (c.x, c.y) == (u.x, u.y)).copied();
        let Some(o) = offer(&u, here, treasury.0[civ]) else { continue };
        if treasury.0[civ] < o.cost + AI_RESERVE {
            continue;
        }
        treasury.0[civ] -= o.cost;
        apply(&mut u, o.to);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::research;

    fn city_with(civ: usize, x: i32, y: i32, b: &[Production]) -> City {
        let mut c = City::new(civ, "Rome", x, y);
        c.buildings = b.to_vec();
        c
    }

    /// The furthest type of a chain, as the walk finds it when every type
    /// is trainable.
    fn last(t: UnitType) -> UnitType {
        UnitType(roster::upgrade_chain(t.0 as usize).last().unwrap() as u16)
    }

    #[test]
    fn a_warrior_in_a_city_with_barracks_upgrades_for_the_shield_difference() {
        realm::reset();
        let c = city_with(0, 3, 3, &[Production::named("Barracks")]);
        let u = Unit::new(0, UnitType::named("Warrior"), 3, 3);
        let to = last(UnitType::named("Warrior"));
        let cost = |t: UnitType| i32::from(Production::from_unit(t).cost());
        let want = (upgrade_cost() * (cost(to) - cost(UnitType::named("Warrior")))).max(0) as u32;
        assert!(want > 0);
        assert_eq!(offer(&u, Some(&c), want), Some(Offer { to, cost: want }));
        // One gold short.
        assert_eq!(offer(&u, Some(&c), want - 1), None);
    }

    #[test]
    fn it_needs_a_city_with_the_facility_and_movement_left() {
        realm::reset();
        let barracks = city_with(0, 3, 3, &[Production::named("Barracks")]);
        let bare = city_with(0, 3, 3, &[]);
        let mut u = Unit::new(0, UnitType::named("Warrior"), 3, 3);
        assert!(offer(&u, Some(&barracks), 10_000).is_some());
        assert!(offer(&u, Some(&bare), 10_000).is_none());
        assert!(offer(&u, None, 10_000).is_none());
        // A Harbor is for ships, not for a Warrior.
        let harbor = city_with(0, 3, 3, &[Production::named("Harbor")]);
        assert!(offer(&u, Some(&harbor), 10_000).is_none());
        u.moves = 0;
        assert!(offer(&u, Some(&barracks), 10_000).is_none());
    }

    #[test]
    fn a_unit_without_a_successor_or_the_ability_is_never_offered() {
        realm::reset();
        let c = city_with(0, 3, 3, &[Production::named("Barracks"), Production::named("Harbor")]);
        for t in [UnitType::named("Settler"), UnitType::named("Worker")] {
            assert_eq!(offer(&Unit::new(0, t, 3, 3), Some(&c), 10_000), None);
        }
    }

    #[test]
    fn the_walk_skips_what_the_civ_cannot_train() {
        realm::reset();
        let chain: Vec<usize> = roster::upgrade_chain(UnitType::named("Warrior").0 as usize).collect();
        assert!(chain.len() >= 2, "the Warrior chain is longer than one step");
        // Only the first step is trainable: the Warrior becomes that.
        for &n in &chain[1..] {
            research::set_trainable(0, n, false);
        }
        assert_eq!(replacement(0, UnitType::named("Warrior")), Some(UnitType(chain[0] as u16)));
        // Nothing at all is trainable: no upgrade.
        research::set_trainable(0, chain[0], false);
        assert_eq!(replacement(0, UnitType::named("Warrior")), None);
        for &n in &chain {
            research::set_trainable(0, n, true);
        }
    }

    #[test]
    fn leonardo_halves_the_price() {
        realm::reset();
        let plain = price(0, UnitType::named("Warrior"), last(UnitType::named("Warrior")));
        let row = (0..bldg_count())
            .find(|&r| roster::bldg(r).wonder & roster::wonder::HALVES_UPGRADE_COST != 0)
            .expect("a Halves-Upgrade-Cost wonder exists");
        realm::write(0, |r| r.owned[row] = 1);
        assert!(halved(0));
        assert_eq!(price(0, UnitType::named("Warrior"), last(UnitType::named("Warrior"))), plain / 2);
        // Another civ does not own it.
        assert!(!halved(1));
    }

    #[test]
    fn an_upgrade_keeps_the_tile_and_veteran_rank_and_heals() {
        let mut u = Unit::new(0, UnitType::named("Warrior"), 5, 6);
        u.level = Level::Elite;
        u.damage = 1;
        u.fortified = true;
        u.sentry = true;
        apply(&mut u, UnitType::named("Swordsman"));
        assert_eq!((u.utype, u.x, u.y), (UnitType::named("Swordsman"), 5, 6));
        assert_eq!((u.level, u.damage, u.moves), (Level::Veteran, 0, 0));
        assert!(u.fortified, "the fortified order stays");
        assert!(!u.sentry);
        let mut green = Unit::new(0, UnitType::named("Warrior"), 0, 0);
        green.level = Level::Conscript;
        apply(&mut green, UnitType::named("Swordsman"));
        assert_eq!(green.level, Level::Conscript);
    }

    #[test]
    fn upgrade_all_pays_per_unit_and_skips_what_no_longer_fits() {
        realm::reset();
        let c = city_with(0, 3, 3, &[Production::named("Barracks")]);
        let to = last(UnitType::named("Warrior"));
        let each = price(0, UnitType::named("Warrior"), to);
        let mut units: Vec<Unit> = (0..3).map(|_| Unit::new(0, UnitType::named("Warrior"), 3, 3)).collect();
        // Out in the field, a fourth cannot.
        units.push(Unit::new(0, UnitType::named("Warrior"), 9, 9));
        let mut other = Unit::new(1, UnitType::named("Warrior"), 3, 3);
        other.moves = 3;
        units.push(other);
        let cities = [&c];
        // The tally judges each unit alone.
        assert_eq!(tally(0, UnitType::named("Warrior"), units.iter(), &cities, 1000), (3, 3 * each));
        // Gold for two: the third is skipped.
        let mut gold = 2 * each + each / 2;
        let (n, spent) = upgrade_all(0, UnitType::named("Warrior"), units.iter_mut(), &cities, &mut gold);
        assert_eq!((n, spent, gold), (2, 2 * each, each / 2));
        assert_eq!(units.iter().filter(|u| u.utype == to).count(), 2);
        assert_eq!(units[2].utype, UnitType::named("Warrior"));
        assert_eq!(units[4].utype, UnitType::named("Warrior"), "another civ's unit stays");
    }

    #[test]
    fn the_computer_upgrades_in_a_city_and_keeps_a_reserve() {
        realm::reset();
        let mut app = App::new();
        app.insert_resource(Civilizations::default());
        app.insert_resource(crate::units::Turn(1));
        app.insert_resource(crate::splash::SplashUp(false));
        crate::civs::set_controllers();
        let to = last(UnitType::named("Warrior"));
        let each = price(1, UnitType::named("Warrior"), to);
        let mut t = Treasury::default();
        t.0[1] = each + AI_RESERVE - 1;
        app.insert_resource(t);
        app.world_mut().spawn(city_with(1, 3, 3, &[Production::named("Barracks")]));
        let unit = app.world_mut().spawn(Unit::new(1, UnitType::named("Warrior"), 3, 3)).id();
        app.world_mut().resource_mut::<Civilizations>().active = 1;
        // The controllers are a per-thread setting under test.
        app.edit_schedule(Update, |s| {
            s.set_executor_kind(bevy::ecs::schedule::ExecutorKind::SingleThreaded);
        });
        app.add_systems(Update, ai_turn);
        app.update();
        assert_eq!(app.world().get::<Unit>(unit).unwrap().utype, UnitType::named("Warrior"), "a reserve is kept");
        // Next turn it has the gold.
        app.world_mut().resource_mut::<Treasury>().0[1] += 1;
        app.world_mut().resource_mut::<crate::units::Turn>().0 = 2;
        app.update();
        assert_eq!(app.world().get::<Unit>(unit).unwrap().utype, to);
        assert_eq!(app.world().resource::<Treasury>().0[1], AI_AFTER);
    }

    const AI_AFTER: u32 = AI_RESERVE;
}
