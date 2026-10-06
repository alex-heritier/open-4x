//! The rows of the rules the game logic means by a role, found by the BIQ
//! field or flag Civ3 itself reads (`docs/civ3-files.md` section 3), never by
//! name: a mod may rename, replace or drop any stock row.
//!
//! * units: the `RULE` slots (`scout_unit`, `basic_barbarian_unit`,
//!   `battle_created_unit`, ...) and `PRTO` worker-action bits;
//! * buildings: the improvement flag the exe's own test reads (the Granary is
//!   whatever has *Doubles City Growth Rate*), the cheapest such row when
//!   several qualify.
//!
//! Each role is `Option`al where a mod can legitimately lack it.

use std::sync::OnceLock;

use crate::cities::Production;
use crate::roster::{self, BldgDef, UnitRow, imp, oth, worker};
use crate::ruleset::Ruleset;
use crate::units::UnitType;

/// `BLDG.small_wonder_flags` bit: *Reduces Corruption* (Forbidden Palace).
const SMALL_REDUCES_CORRUPTION: u32 = 1 << 5;

/// The worker actions of a land worker.
const WORKS_LAND: u32 = worker::IRRIGATE | worker::BUILD_MINE | worker::BUILD_ROAD;

/// Row indices of the roles, resolved once from a ruleset.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Roles {
    /// The unit with the *Build City* worker action.
    pub settler: Option<usize>,
    /// The cheapest unit that works the land (irrigates, mines, roads) and
    /// cannot found a city; `RULE.captured_unit` when that qualifies.
    pub worker: Option<usize>,
    /// `RULE.scout_unit`.
    pub scout: Option<usize>,
    /// `RULE.battle_created_unit`: the Great Leader.
    pub leader: Option<usize>,
    /// `RULE.build_army_unit`.
    pub army: Option<usize>,
    pub barbarian_basic: Option<usize>,
    pub barbarian_advanced: Option<usize>,
    pub barbarian_sea: Option<usize>,
    /// `RULE.captured_unit`.
    pub captured: Option<usize>,
    /// The cheapest land fighter that needs no advance, the soldier a civ
    /// starts with and falls back on.
    pub guard: Option<usize>,
    /// The cheapest ship that carries land units.
    pub ferry: Option<usize>,
    // Buildings (`BLDG` rows).
    pub palace: Option<usize>,
    pub forbidden_palace: Option<usize>,
    pub wealth: Option<usize>,
    pub temple: Option<usize>,
    pub granary: Option<usize>,
    pub barracks: Option<usize>,
    pub walls: Option<usize>,
    pub library: Option<usize>,
    pub marketplace: Option<usize>,
    pub harbor: Option<usize>,
    pub courthouse: Option<usize>,
}

fn slot(i: i32, n: usize) -> Option<usize> {
    (i >= 0 && (i as usize) < n).then_some(i as usize)
}

/// The cheapest unit satisfying `pred` among the units a civ can have.
fn cheapest_unit(units: &[UnitRow], pred: impl Fn(&UnitRow) -> bool) -> Option<usize> {
    units
        .iter()
        .enumerate()
        .filter(|(_, u)| u.playable && pred(u))
        .min_by_key(|(i, u)| (u.cost, *i))
        .map(|(i, _)| i)
}

/// The cheapest improvement (not a wonder) satisfying `pred`.
fn cheapest_bldg(bldgs: &[BldgDef], pred: impl Fn(&BldgDef) -> bool) -> Option<usize> {
    bldgs
        .iter()
        .enumerate()
        .filter(|(_, b)| b.other & (oth::WONDER | oth::SMALL_WONDER) == 0 && pred(b))
        .min_by_key(|(i, b)| (b.cost, *i))
        .map(|(i, _)| i)
}

/// Resolve every role of `rs`.
pub fn resolve(rs: &Ruleset) -> Roles {
    let (u, b, g) = (&rs.units, &rs.bldgs, &rs.general);
    let nu = u.len();
    let captured = slot(g.captured_unit, nu);
    let is_worker = |r: &UnitRow| r.worker & WORKS_LAND != 0 && r.worker & worker::BUILD_CITY == 0;
    let worker = captured
        .filter(|&i| is_worker(&u[i]))
        .or_else(|| cheapest_unit(u, is_worker));
    // Wonders carry their own flags, so those are looked up with the wonder bits.
    let small_wonder = |pred: &dyn Fn(&BldgDef) -> bool| {
        b.iter()
            .enumerate()
            .filter(|(_, x)| x.other & oth::SMALL_WONDER != 0 && pred(x))
            .min_by_key(|(i, x)| (x.cost, *i))
            .map(|(i, _)| i)
    };
    let flagged = |flag: u32| cheapest_bldg(b, move |x| x.flags & flag != 0);
    Roles {
        settler: cheapest_unit(u, |r| r.worker & worker::BUILD_CITY != 0),
        worker,
        scout: slot(g.scout_unit, nu),
        leader: slot(g.battle_created_unit, nu),
        army: slot(g.build_army_unit, nu),
        barbarian_basic: slot(g.basic_barbarian_unit, nu),
        barbarian_advanced: slot(g.advanced_barbarian_unit, nu),
        barbarian_sea: slot(g.barbarian_sea_unit, nu),
        captured,
        guard: cheapest_unit(u, |r| {
            r.class == 0
                && r.tech < 0
                && r.attack > 0
                && r.pop_cost == 0
                && r.worker == 0
                && r.resources == [-1; 3]
        }),
        ferry: cheapest_unit(u, |r| r.class == 1 && r.capacity > 0 && r.pop_cost == 0),
        palace: b.iter().position(|x| x.flags & imp::CENTER_OF_EMPIRE != 0),
        forbidden_palace: small_wonder(&|x| x.small & SMALL_REDUCES_CORRUPTION != 0),
        wealth: b.iter().position(|x| x.flags & imp::CAPITALIZATION != 0),
        temple: cheapest_bldg(b, |x| x.happy > 0),
        granary: flagged(imp::KEEPS_FOOD),
        barracks: flagged(imp::VETERAN_GROUND_UNITS),
        walls: cheapest_bldg(b, |x| x.defense > 0),
        library: flagged(imp::RESEARCH_BONUS),
        marketplace: flagged(imp::LUXURY_TRADE),
        harbor: flagged(imp::INCREASES_FOOD_IN_WATER),
        courthouse: flagged(imp::REDUCES_CORRUPTION),
    }
}

static ROLES: OnceLock<Roles> = OnceLock::new();

/// The roles of the installed ruleset.
pub fn get() -> &'static Roles {
    ROLES.get_or_init(|| resolve(crate::ruleset::get()))
}

fn unit(i: Option<usize>, what: &str) -> UnitType {
    UnitType(i.unwrap_or_else(|| panic!("the rules have no {what} unit")) as u16)
}

fn bldg(i: Option<usize>) -> Option<Production> {
    i.map(Production::from_building_row)
}

/// The unit with the *Build City* action. Every game needs one.
pub fn settler() -> UnitType {
    unit(get().settler, "settler (Build City)")
}
/// The land worker.
pub fn worker() -> UnitType {
    unit(get().worker, "worker")
}
/// `RULE.scout_unit`; rules without one use the soldier.
pub fn scout() -> UnitType {
    unit(get().scout.or(get().guard), "scout")
}
/// The Great Leader (`RULE.battle_created_unit`).
pub fn leader() -> UnitType {
    unit(get().leader, "leader")
}
/// What a civ's first soldier is.
pub fn guard() -> UnitType {
    unit(get().guard.or(get().barbarian_basic), "starting soldier")
}
/// `RULE.captured_unit`: what the workers of a captured settler become.
pub fn captured() -> UnitType {
    unit(get().captured.or(get().worker), "captured")
}
/// The barbarians' land soldier.
pub fn barbarian_basic() -> UnitType {
    unit(get().barbarian_basic.or(get().guard), "basic barbarian")
}
/// The barbarians' mounted soldier.
pub fn barbarian_advanced() -> UnitType {
    unit(
        get().barbarian_advanced.or(get().barbarian_basic),
        "advanced barbarian",
    )
}
/// The barbarians' ship.
pub fn barbarian_sea() -> Option<UnitType> {
    get()
        .barbarian_sea
        .or(get().ferry)
        .map(|i| UnitType(i as u16))
}
/// The ship the AI ferries settlers with.
pub fn ferry() -> Option<Production> {
    get().ferry.map(|i| Production(i as u16))
}
pub fn settler_production() -> Production {
    Production::from_unit(settler())
}
pub fn worker_production() -> Production {
    Production::from_unit(worker())
}
pub fn guard_production() -> Production {
    Production::from_unit(guard())
}
pub fn palace() -> Option<Production> {
    bldg(get().palace)
}
pub fn forbidden_palace() -> Option<Production> {
    bldg(get().forbidden_palace)
}
/// *Capitalization*: turns shields into gold.
pub fn wealth() -> Option<Production> {
    bldg(get().wealth)
}
pub fn temple() -> Option<Production> {
    bldg(get().temple)
}
pub fn granary() -> Option<Production> {
    bldg(get().granary)
}
pub fn barracks() -> Option<Production> {
    bldg(get().barracks)
}
pub fn walls() -> Option<Production> {
    bldg(get().walls)
}
pub fn library() -> Option<Production> {
    bldg(get().library)
}
pub fn marketplace() -> Option<Production> {
    bldg(get().marketplace)
}
pub fn harbor() -> Option<Production> {
    bldg(get().harbor)
}
pub fn courthouse() -> Option<Production> {
    bldg(get().courthouse)
}

/// The units of a civ's opening party: the two free units of `RULE`, a
/// soldier and a scout.
pub fn start_party() -> Vec<UnitType> {
    let g = &crate::ruleset::get().general;
    let n = crate::ruleset::unit_count();
    let mut party: Vec<UnitType> = [g.start_unit_1, g.start_unit_2]
        .into_iter()
        .filter_map(|i| slot(i, n))
        .map(|i| UnitType(i as u16))
        .collect();
    party.push(guard());
    if get().scout.is_some() {
        party.push(scout());
    }
    party
}

/// Whether a unit works the land (irrigates, mines, builds roads) and cannot
/// found cities.
pub fn is_worker(t: UnitType) -> bool {
    let r = roster::unit(t.0 as usize);
    r.worker & WORKS_LAND != 0 && r.worker & worker::BUILD_CITY == 0
}

/// Whether the Explore order is offered: a land or sea unit that is not a
/// worker, a leader or an army (Civ3's explore is a recon order).
pub fn can_explore(t: UnitType) -> bool {
    let r = roster::unit(t.0 as usize);
    r.playable
        && r.class < 2
        && r.worker == 0
        && r.pop_cost == 0
        && r.abilities & (roster::ability::LEADER | roster::ability::ARMY) == 0
}

/// Whether a unit can found cities (*Build City*).
pub fn founds_cities(t: UnitType) -> bool {
    roster::unit(t.0 as usize).worker & worker::BUILD_CITY != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name_of(rs: &Ruleset, role: Option<usize>, unit: bool) -> &'static str {
        role.map_or("-", |i| {
            if unit {
                rs.units[i].name
            } else {
                rs.bldgs[i].name
            }
        })
    }

    /// In `conquests.biq` every role lands on the row a player would name.
    #[test]
    fn conquests_roles_are_the_familiar_rows() {
        let rs = crate::ruleset::get();
        let r = resolve(rs);
        let n = |x, unit| name_of(rs, x, unit);
        assert_eq!(n(r.settler, true), "Settler");
        assert_eq!(n(r.worker, true), "Worker");
        assert_eq!(n(r.scout, true), "Scout");
        assert_eq!(n(r.leader, true), "Leader");
        assert_eq!(n(r.army, true), "Army");
        assert_eq!(n(r.guard, true), "Warrior");
        assert_eq!(n(r.ferry, true), "Galley");
        assert_eq!(n(r.barbarian_basic, true), "Warrior");
        assert_eq!(n(r.barbarian_advanced, true), "Horseman");
        assert_eq!(n(r.barbarian_sea, true), "Galley");
        assert_eq!(n(r.captured, true), "Worker");
        assert_eq!(n(r.palace, false), "Palace");
        assert_eq!(n(r.forbidden_palace, false), "Forbidden Palace");
        assert_eq!(n(r.wealth, false), "Wealth");
        assert_eq!(n(r.temple, false), "Temple");
        assert_eq!(n(r.granary, false), "Granary");
        assert_eq!(n(r.barracks, false), "Barracks");
        assert_eq!(n(r.walls, false), "Walls");
        assert_eq!(n(r.library, false), "Library");
        assert_eq!(n(r.marketplace, false), "Marketplace");
        assert_eq!(n(r.harbor, false), "Harbor");
        assert_eq!(n(r.courthouse, false), "Courthouse");
    }

    #[test]
    fn the_opening_party() {
        let names: Vec<_> = start_party().iter().map(|t| t.row().name).collect();
        assert_eq!(names, ["Settler", "Worker", "Warrior", "Scout"]);
    }
}
