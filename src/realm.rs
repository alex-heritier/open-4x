//! A civilization's standing policy, and the facts about it that the city
//! math reads: its government, the three commerce rates, the anarchy that
//! follows a revolution, the capital, the advances it knows, how many cities
//! hold each improvement and where its soldiers stand.
//!
//! The reverse-engineered rules are `civ3mapgen::government` (the eight
//! shipped GOVT rows and the AI's choice, specification
//! `reverse-engineering/government.md`); this module is the state they act
//! on. The city layer's plain functions (`citycalc`) have no ECS access, so
//! the state lives in a process-wide table like `research::Access`, and
//! [`sync`] refreshes it from the world once a frame. Under test each thread
//! has a table of its own.
//!
//! Deviations, all marked **HYPOTHESIS** where they stand: a revolution
//! names the next government when it starts (the executable asks when
//! anarchy ends); war weariness is not tracked, so the governments that
//! suffer from it are only ever held back by the AI's score.

use std::collections::HashMap;

use bevy::prelude::*;
#[cfg(test)]
pub use civ3mapgen::government::row as govt_row;
use civ3mapgen::government::{Govt, SHIPPED, row};

use crate::cities::{Capital, City, Production, territory};
use crate::civs::{CIV_CAP, civ_count};
use crate::map::GameMap;
use crate::research::{self, Research};
use crate::roster::{self, bldg_count};
use crate::units::{Unit, def};

/// The three commerce rates in tenths of the commerce; they add up to 10.
/// Every shipped government caps each at 100% (`government.md` 1.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rates {
    pub tax: u8,
    pub sci: u8,
    pub lux: u8,
}

impl Rates {
    /// Civ3's start: half taxes, half science.
    pub const DEFAULT: Rates = Rates {
        tax: 5,
        sci: 5,
        lux: 0,
    };

    /// The rates add up to 100% and stay under the cap (10 is 100%).
    pub fn valid(self, cap: u8) -> bool {
        self.tax + self.sci + self.lux == 10 && self.tax.max(self.sci).max(self.lux) <= cap
    }

    /// Move one tenth from one rate to another: the sliders of the Domestic
    /// Advisor. `None` when the move is not allowed.
    pub fn shifted(self, from: Rate, to: Rate, cap: u8) -> Option<Rates> {
        let mut r = self;
        let take = r.get_mut(from);
        if *take == 0 {
            return None;
        }
        *take -= 1;
        *r.get_mut(to) += 1;
        r.valid(cap).then_some(r)
    }

    fn get_mut(&mut self, which: Rate) -> &mut u8 {
        match which {
            Rate::Tax => &mut self.tax,
            Rate::Sci => &mut self.sci,
            Rate::Lux => &mut self.lux,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rate {
    Tax,
    Sci,
    Lux,
}

/// Everything about one civilization the city math needs.
#[derive(Clone, Debug)]
pub struct Realm {
    /// GOVT row (`government::row`).
    pub govt: usize,
    pub rates: Rates,
    /// Turns of anarchy still to run (`Player +0x9C`); the government is
    /// Anarchy meanwhile.
    pub anarchy: u8,
    /// The government to take when anarchy ends.
    pub then: usize,
    /// Turns before the computer thinks about another revolution
    /// (`Player +0x34`).
    pub cooldown: u8,
    /// Where the Palace stands.
    pub capital: Option<(i32, i32)>,
    /// One bit per advance known.
    pub known: u128,
    /// The civ knows an advance that doubles Wealth (Economics).
    pub double_wealth: bool,
    /// How many cities hold each improvement (`Player +0x15DC`).
    pub owned: Vec<u16>,
    /// A live wonder doubles this improvement's happiness (the Oracle's
    /// Temples, the Sistine Chapel's Cathedrals).
    pub doubled: Vec<bool>,
    /// Which continent each city stands on, and how many cities of each
    /// continent hold each improvement.
    pub continent: HashMap<(i32, i32), u16>,
    pub on_continent: HashMap<(u16, u16), u16>,
    /// Distinct luxury goods inside the borders.
    pub luxuries: usize,
    /// Soldiers on each tile (martial law).
    pub garrison: HashMap<(i32, i32), u8>,
    /// Number of cities, for unit support and the AI's score.
    pub cities: usize,
    /// Wonders that ease war weariness everywhere (Universal Suffrage).
    pub suffrage: i32,
    /// The war weariness against every civ at war with this one
    /// (`Player +0xCB4`), the figures the happiness routine reads.
    pub war_counters: Vec<i32>,
    /// The average weariness (`0x5007B0`), which brings down a Democracy.
    pub average_weariness: i32,
    /// Turn the Golden Age ends (`Player +0x3C`), `None` while the civ has
    /// never had one: a civilization has one Golden Age.
    pub golden_end: Option<u32>,
    /// The current turn falls inside the Golden Age (`turn < +0x3C`).
    pub golden: bool,
    /// A unique unit won: start the age at the next check.
    pub golden_due: bool,
    /// Citizens born content (`DIFF +0x44`, `happiness.md` 1). Under test
    /// everyone is born content, so the tests that do not study happiness
    /// stay out of it.
    pub born_content: i32,
    /// Empire culture (`Player +0x183C`), as `flip::Flips` totals it.
    pub rating: i32,
    /// At war with each civ (`Player +0xD30`).
    pub at_war: [bool; CIV_CAP],
    /// The civ's cities in city order (the native pool order is not kept:
    /// **H**, the clone's query order stands in for city ids).
    pub city_order: Vec<(i32, i32)>,
    /// Cities trade-connected to the capital (`trade.rs`, `0x57F0A0`).
    pub connected: std::collections::HashSet<(i32, i32)>,
    /// Cities holding an owned Reduces-Corruption wonder the government
    /// allows (`BLDG +0xF4 & 0x20`: the Forbidden Palace), one entry each.
    pub palaces: Vec<(i32, i32)>,
}

/// What a saved game keeps of a realm: the standing policy. Everything
/// else is read off the world again by [`sync`].
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Saved {
    govt: usize,
    rates: (u8, u8, u8),
    anarchy: u8,
    then: usize,
    cooldown: u8,
    war_counters: Vec<i32>,
    average_weariness: i32,
    golden_end: Option<u32>,
    golden_due: bool,
}

impl Realm {
    pub fn snapshot(&self) -> Saved {
        Saved {
            govt: self.govt,
            rates: (self.rates.tax, self.rates.sci, self.rates.lux),
            anarchy: self.anarchy,
            then: self.then,
            cooldown: self.cooldown,
            war_counters: self.war_counters.clone(),
            average_weariness: self.average_weariness,
            golden_end: self.golden_end,
            golden_due: self.golden_due,
        }
    }

    pub fn restore(&mut self, s: &Saved) {
        self.govt = s.govt;
        self.rates = Rates {
            tax: s.rates.0,
            sci: s.rates.1,
            lux: s.rates.2,
        };
        self.anarchy = s.anarchy;
        self.then = s.then;
        self.cooldown = s.cooldown;
        self.war_counters = s.war_counters.clone();
        self.average_weariness = s.average_weariness;
        self.golden_end = s.golden_end;
        self.golden_due = s.golden_due;
    }

    pub fn new() -> Realm {
        Realm {
            govt: row::DESPOTISM,
            rates: Rates::DEFAULT,
            anarchy: 0,
            then: row::DESPOTISM,
            cooldown: 0,
            capital: None,
            known: 0,
            double_wealth: false,
            owned: vec![0; bldg_count()],
            doubled: vec![false; bldg_count()],
            continent: HashMap::new(),
            on_continent: HashMap::new(),
            luxuries: 0,
            garrison: HashMap::new(),
            cities: 0,
            suffrage: 0,
            war_counters: vec![],
            average_weariness: 0,
            golden_end: None,
            golden: false,
            golden_due: false,
            rating: 0,
            at_war: [false; CIV_CAP],
            city_order: Vec::new(),
            connected: std::collections::HashSet::new(),
            palaces: Vec::new(),
            born_content: if cfg!(test) {
                99
            } else {
                civ3mapgen::happiness::shipped::BORN_CONTENT[crate::scenario::difficulty()]
            },
        }
    }

    pub fn govt(&self) -> &'static Govt {
        &SHIPPED[self.govt]
    }

    pub fn knows(&self, tech: i32) -> bool {
        tech < 0 || (tech < 128 && self.known >> tech & 1 != 0)
    }

    /// The government may be adopted: not the transition one, and its
    /// prerequisite advance is known (`government.md` 3.5).
    pub fn can_adopt(&self, g: usize) -> bool {
        g != row::ANARCHY && self.knows(SHIPPED[g].prerequisite_tech)
    }

    /// Governments the civ may choose, in GOVT order.
    #[cfg(test)]
    pub fn choices(&self) -> Vec<usize> {
        (0..SHIPPED.len()).filter(|&g| self.can_adopt(g)).collect()
    }

    /// Switch government at once. The rates are kept when the new cap
    /// allows them (every shipped cap is 100%).
    pub fn adopt(&mut self, g: usize) {
        self.govt = g;
        self.anarchy = 0;
        if !self.rates.valid(SHIPPED[g].rate_cap as u8) {
            self.rates = Rates::DEFAULT;
        }
    }

    /// Start a revolution toward `then` (`0x55CE50`): Anarchy at once; a
    /// countdown of 1 turn or less ends it on the spot (`0x55CE91`).
    pub fn revolt(&mut self, then: usize, turns: u8) {
        if self.govt == row::ANARCHY {
            self.then = then;
            return;
        }
        if then == self.govt {
            return;
        }
        self.govt = row::ANARCHY;
        self.then = then;
        self.anarchy = turns;
        if turns <= 1 {
            self.end_anarchy();
        }
    }

    /// The end of the revolution (`0x55CBB0`).
    pub fn end_anarchy(&mut self) {
        let g = if self.can_adopt(self.then) {
            self.then
        } else {
            row::DESPOTISM
        };
        self.adopt(g);
    }

    /// One turn of anarchy has passed. True when it just ended
    /// (`0x560CD4..0x560D06`).
    pub fn tick(&mut self) -> bool {
        if self.cooldown > 0 {
            self.cooldown -= 1;
        }
        if self.govt != row::ANARCHY {
            return false;
        }
        self.anarchy = self.anarchy.saturating_sub(1);
        if self.anarchy == 0 {
            self.end_anarchy();
            return true;
        }
        false
    }
}

impl Default for Realm {
    fn default() -> Self {
        Realm::new()
    }
}

fn fresh() -> Vec<Realm> {
    // One more for the barbarians, whose units ask about their owner's
    // government and advances like anyone's.
    (0..CIV_CAP).map(|_| Realm::new()).collect()
}

#[cfg(not(test))]
static REALMS: std::sync::LazyLock<std::sync::Mutex<Vec<Realm>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(fresh()));

#[cfg(test)]
thread_local! {
    static REALMS: std::cell::RefCell<Vec<Realm>> = std::cell::RefCell::new(fresh());
}

/// Read a civ's realm. Keep `f` short and do not call back into this module
/// from it.
pub fn read<R>(civ: usize, f: impl FnOnce(&Realm) -> R) -> R {
    #[cfg(not(test))]
    return f(&REALMS.lock().unwrap()[civ]);
    #[cfg(test)]
    return REALMS.with(|r| f(&r.borrow()[civ]));
}

/// Change a civ's realm.
pub fn write<R>(civ: usize, f: impl FnOnce(&mut Realm) -> R) -> R {
    #[cfg(not(test))]
    return f(&mut REALMS.lock().unwrap()[civ]);
    #[cfg(test)]
    return REALMS.with(|r| f(&mut r.borrow_mut()[civ]));
}

/// Put every realm back to its start (tests).
#[cfg(test)]
pub fn reset() {
    REALMS.with(|r| *r.borrow_mut() = fresh());
}

#[cfg(not(test))]
static TURN: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
#[cfg(test)]
thread_local! {
    static TURN: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// The game turn (`[0xA526AC]`), refreshed by [`sync`].
pub fn turn() -> u32 {
    #[cfg(not(test))]
    return TURN.load(std::sync::atomic::Ordering::Relaxed);
    #[cfg(test)]
    return TURN.with(|t| t.get());
}

pub fn set_turn(turn: u32) {
    #[cfg(not(test))]
    TURN.store(turn, std::sync::atomic::Ordering::Relaxed);
    #[cfg(test)]
    TURN.with(|t| t.set(turn));
}

pub fn govt(civ: usize) -> &'static Govt {
    read(civ, |r| r.govt())
}

pub fn rates(civ: usize) -> Rates {
    read(civ, |r| r.rates)
}

pub fn in_anarchy(civ: usize) -> bool {
    read(civ, |r| r.govt == row::ANARCHY)
}

/// Government names by GOVT row.
pub const GOVT_NAMES: [&str; 8] = [
    "Anarchy",
    "Despotism",
    "Monarchy",
    "Communism",
    "Republic",
    "Democracy",
    "Fascism",
    "Feudalism",
];

/// Set the rates when they are legal under the civ's government.
pub fn set_rates(civ: usize, rates: Rates) -> bool {
    write(civ, |r| {
        let ok = rates.valid(r.govt().rate_cap as u8);
        if ok {
            r.rates = rates;
        }
        ok
    })
}

/// The `GOOD` row of a placed resource, for the eight strategic ones. The
/// map numbers its resources in the order of `features::GOODS`, the file in
/// its own.
pub fn strategic_row(id: u8) -> Option<usize> {
    Some(match crate::features::GOODS[id as usize].name {
        "Horses" => 0,
        "Iron" => 1,
        "Saltpeter" => 2,
        "Coal" => 3,
        "Oil" => 4,
        "Rubber" => 5,
        "Aluminum" => 6,
        "Uranium" => 7,
        _ => return None,
    })
}

/// The placed resource (`features::GOODS` id) of a `GOOD` row, matched by
/// name; the clone calls two of them by their Civilopedia names.
pub fn good_id(row: i32) -> Option<u8> {
    let name = *crate::ruleset::GOOD_NAMES.get(usize::try_from(row).ok()?)?;
    let name = match name {
        "Wines" => "Wine",
        "Gems" => "Diamonds",
        n => n,
    };
    crate::features::GOODS
        .iter()
        .position(|g| g.name == name)
        .map(|i| i as u8)
}

/// Land connected to `at` by king moves, wrapping in x (a continent).
fn flood(map: &GameMap, at: (i32, i32), label: &mut [u16], id: u16) {
    let mut stack = vec![at];
    while let Some((x, y)) = stack.pop() {
        let i = map.idx(x, y);
        if label[i] != 0 {
            continue;
        }
        label[i] = id;
        for (nx, ny) in map.neighbors(x, y) {
            if map.is_land(nx, ny) && label[map.idx(nx, ny)] == 0 {
                stack.push((nx, ny));
            }
        }
    }
}

/// A label per tile: 0 for water, 1.. for each landmass.
pub fn continents(map: &GameMap) -> Vec<u16> {
    let mut label = vec![0u16; map.tiles.len()];
    let mut next = 1;
    for y in 0..map.h {
        for x in 0..map.w {
            if map.is_land(x, y) && label[map.idx(x, y)] == 0 {
                flood(map, (x, y), &mut label, next);
                next += 1;
            }
        }
    }
    label
}

/// What [`sync`] last worked from, for the inputs that carry no change flag:
/// the governments live in the process-wide table, and the soldiers move
/// every frame for the animation's sake, so a flag on `Unit` says nothing.
#[derive(Default)]
pub struct Basis {
    govts: Vec<usize>,
    cities: usize,
    /// `(civ, x, y)` of every soldier of a playing civ, sorted.
    soldiers: Vec<(usize, i32, i32)>,
    scratch: Vec<(usize, i32, i32)>,
}

/// Soldiers of `civ` on each tile, from the sorted list of [`Basis`].
fn garrison_of(soldiers: &[(usize, i32, i32)], civ: usize) -> HashMap<(i32, i32), u8> {
    let mut garrison = HashMap::new();
    for &(_, x, y) in soldiers.iter().filter(|s| s.0 == civ) {
        *garrison.entry((x, y)).or_insert(0u8) += 1;
    }
    garrison
}

/// Refresh the process-wide tables from the world: the advances known, the
/// improvements held, the grants of wonders, the strategic resources inside
/// the borders (which unlock units and improvements), the wonders that are
/// built, the capital, the luxuries and the soldiers on each tile.
///
/// The pass reads every city, the map and the trade network, so it runs
/// only when something it reads has changed: the resources and cities it
/// takes (their change flags), the governments and the number of cities
/// (the [`Basis`]). Where the soldiers stand feeds the garrison tables
/// alone, and those are refreshed on their own.
pub fn sync(
    map: Res<GameMap>,
    research_state: Res<Research>,
    capital: Res<Capital>,
    mut cities: Query<(Entity, &mut City)>,
    units: Query<&Unit>,
    diplomacy: Option<Res<crate::diplomacy::Diplomacy>>,
    extra: (
        Option<Res<crate::flip::Flips>>,
        Option<Res<crate::units::Turn>>,
    ),
    mut labels: Local<Vec<u16>>,
    mut basis: Local<Basis>,
) {
    let (flips, turn) = extra;
    if let Some(turn) = turn.as_ref() {
        set_turn(turn.0);
    }
    let governments: Vec<usize> = (0..civ_count()).map(|civ| read(civ, |r| r.govt)).collect();
    let mut soldiers = std::mem::take(&mut basis.scratch);
    soldiers.clear();
    soldiers.extend(
        units
            .iter()
            .filter(|u| u.civ < civ_count() && def(u.utype).attack > 0)
            .map(|u| (u.civ, u.x, u.y)),
    );
    soldiers.sort_unstable();
    let (count, touched) = cities
        .iter_mut()
        .fold((0, false), |(n, t), (_, c)| (n + 1, t || c.is_changed()));
    let changed = governments != basis.govts || count != basis.cities;
    let moved = soldiers != basis.soldiers;
    if changed {
        basis.govts = governments;
        basis.cities = count;
    }
    if moved {
        std::mem::swap(&mut basis.soldiers, &mut soldiers);
    }
    basis.scratch = soldiers;
    let flagged = map.is_changed()
        || research_state.is_changed()
        || capital.is_changed()
        || diplomacy.as_ref().is_some_and(|d| d.is_changed())
        || flips.as_ref().is_some_and(|f| f.is_changed())
        || turn.as_ref().is_some_and(|t| t.is_changed());
    if !(changed || touched || flagged) {
        // Soldiers walking about change the garrison tables and nothing else
        // the pass computes.
        if moved {
            for civ in 0..civ_count() {
                let garrison = garrison_of(&basis.soldiers, civ);
                write(civ, |r| r.garrison = garrison);
            }
        }
        return;
    }
    if labels.is_empty() {
        *labels = continents(&map);
    }
    let all: Vec<(Entity, City)> = cities.iter().map(|(e, c)| (e, c.clone())).collect();
    let owners = territory(&map);
    let tech_count = crate::ruleset::TECH_NAMES.len() as i32;

    let mut built = vec![false; bldg_count()];
    // The trade network of every civ, over every city (`trade.rs`).
    let war = |a: usize, b: usize| {
        a < civ_count()
            && b < civ_count()
            && a != b
            && diplomacy.as_ref().is_some_and(|d| d.at_war(a, b))
    };
    let trade_flag = |c: &City, flag: u32| {
        c.buildings
            .iter()
            .filter_map(|b| b.building_row())
            .any(|row| {
                let b = roster::bldg(row);
                b.flags & flag != 0 && !(b.obsolete >= 0 && research_state.knows(c.civ, b.obsolete))
            })
    };
    let nodes: Vec<crate::trade::Node> = all
        .iter()
        .map(|(_, c)| crate::trade::Node {
            at: (c.x, c.y),
            owner: c.civ,
            water: trade_flag(c, crate::trade::WATER_TRADE),
            air: trade_flag(c, crate::trade::AIR_TRADE),
        })
        .collect();
    for civ in 0..civ_count() {
        let mut known = 0u128;
        for t in 0..tech_count {
            if research_state.knows(civ, t) {
                known |= 1 << t;
            }
        }
        let mut owned = vec![0u16; bldg_count()];
        let mut on_continent: HashMap<(u16, u16), u16> = HashMap::new();
        let mut continent = HashMap::new();
        let mine: Vec<&(Entity, City)> = all.iter().filter(|(_, c)| c.civ == civ).collect();
        let view = crate::trade::View {
            p: civ,
            at_war: &war,
            sea: research_state.knows_flag(civ, crate::trade::TRADE_OVER_SEA),
            ocean: research_state.knows_flag(civ, crate::trade::TRADE_OVER_OCEAN),
            seen: &|_, _| true,
        };
        let network = crate::trade::components(&map, &nodes, &view);
        let component = &network.comp;
        let capital_pos = capital.0[civ]
            .and_then(|e| all.iter().find(|(id, _)| *id == e))
            .map(|(_, c)| (c.x, c.y));
        let capital_component = capital_pos
            .and_then(|p| nodes.iter().position(|n| n.at == p))
            .map(|i| component[i]);
        let connected: std::collections::HashSet<(i32, i32)> = nodes
            .iter()
            .enumerate()
            .filter(|(i, n)| n.owner == civ && Some(component[*i]) == capital_component)
            .map(|(_, n)| n.at)
            .collect();
        let govt_now = read(civ, |r| r.govt);
        let palaces: Vec<(i32, i32)> = mine
            .iter()
            .flat_map(|(_, c)| {
                c.buildings
                    .iter()
                    .filter_map(|b| b.building_row())
                    .filter(move |&row| {
                        let b = roster::bldg(row);
                        b.small & 0x20 != 0 && (b.govt < 0 || b.govt as usize == govt_now)
                    })
                    .map(move |_| (c.x, c.y))
            })
            .collect();
        let city_order: Vec<(i32, i32)> = mine.iter().map(|(_, c)| (c.x, c.y)).collect();
        for (_, c) in &mine {
            let land = labels[map.idx(c.x, c.y)];
            continent.insert((c.x, c.y), land);
            for b in &c.buildings {
                if let Some(row) = b.building_row() {
                    owned[row] += 1;
                    *on_continent.entry((land, row as u16)).or_insert(0) += 1;
                    built[row] = true;
                }
            }
        }
        let obsolete = |row: usize| {
            let b = roster::bldg(row);
            b.obsolete >= 0 && b.obsolete < 128 && known >> b.obsolete & 1 != 0
        };
        // A live wonder doubles the happiness of another improvement.
        let mut doubled = vec![false; bldg_count()];
        for row in 0..bldg_count() {
            let b = roster::bldg(row);
            if owned[row] > 0 && b.doubles >= 0 && !obsolete(row) {
                doubled[b.doubles as usize] = true;
            }
        }
        // What wonders give to the cities of their owner.
        let mut gifts: HashMap<Entity, Vec<Production>> = HashMap::new();
        for (_, wc) in &mine {
            let land = labels[map.idx(wc.x, wc.y)];
            for b in &wc.buildings {
                let Some(row) = b.building_row() else {
                    continue;
                };
                let w = roster::bldg(row);
                if obsolete(row) {
                    continue;
                }
                for (grant, everywhere) in [(w.grant_all, true), (w.grant_continent, false)] {
                    if grant < 0 {
                        continue;
                    }
                    let gift = Production::from_building_row(grant as usize);
                    for (e, c) in &mine {
                        if (everywhere || labels[map.idx(c.x, c.y)] == land) && !c.has(gift) {
                            let list = gifts.entry(*e).or_default();
                            if !list.contains(&gift) {
                                list.push(gift);
                            }
                        }
                    }
                }
            }
        }
        for (e, mut c) in cities.iter_mut() {
            if c.civ != civ {
                continue;
            }
            let want = gifts.remove(&e).unwrap_or_default();
            if c.gifts != want {
                c.gifts = want;
            }
        }
        // Resources reach a city through its owner's road network
        // (`trade-network.md` 7: `0x57E450` pass 2, `0x57EDD0` step 1): a
        // tile the civ owns (or its colony), carrying a luxury or a
        // strategic resource whose advance is known, labelled in the civ's
        // road network, serves every city of the civ joined to that label.
        let border = |x: i32, y: i32| owners.get(&(x, y)).copied();
        let colonies: std::collections::HashSet<(i32, i32)> = (0..map.h)
            .flat_map(|y| (0..map.w).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                map.get(x, y)
                    .is_some_and(|t| t.site == Some(crate::sites::Site::Colony(civ as u8)))
                    && border(x, y).is_none_or(|o| o == civ)
            })
            .collect();
        let mut masks: HashMap<(i32, i32), u32> =
            mine.iter().map(|(_, c)| ((c.x, c.y), 0u32)).collect();
        for y in 0..map.h {
            for x in 0..map.w {
                let Some(id) = map.tiles[map.idx(x, y)].resource else {
                    continue;
                };
                if border(x, y) != Some(civ) && !colonies.contains(&(x, y)) {
                    continue;
                }
                let usable = match crate::features::GOODS[id as usize].kind {
                    crate::features::GoodKind::Strategic => strategic_row(id).is_some_and(|row| {
                        let need = crate::ruleset::GOOD[row];
                        need < 0 || known >> need & 1 != 0
                    }),
                    crate::features::GoodKind::Luxury => true,
                    crate::features::GoodKind::Bonus => false,
                };
                let Some(reach) = usable.then(|| network.reaches((x, y))).flatten() else {
                    continue;
                };
                for (i, n) in nodes.iter().enumerate() {
                    if n.owner == civ && component[i] == reach {
                        *masks.entry(n.at).or_insert(0) |= 1 << id;
                    }
                }
            }
        }
        let all_goods = masks.values().fold(0u32, |a, m| a | m);
        let mut goods = 0u32;
        let mut luxuries = std::collections::BTreeSet::new();
        for id in (0..crate::features::GOODS.len()).filter(|&id| all_goods >> id & 1 != 0) {
            match crate::features::GOODS[id].kind {
                crate::features::GoodKind::Strategic => {
                    if let Some(row) = strategic_row(id as u8) {
                        goods |= 1 << row;
                    }
                }
                crate::features::GoodKind::Luxury => {
                    luxuries.insert(id as u8);
                }
                crate::features::GoodKind::Bonus => {}
            }
        }
        for (_, mut c) in cities.iter_mut().filter(|(_, c)| c.civ == civ) {
            let m = masks.get(&(c.x, c.y)).copied().unwrap_or(0);
            if c.goods != m {
                c.goods = m;
            }
        }
        let changed = research::set_goods(civ, goods);
        let garrison = garrison_of(&basis.soldiers, civ);
        let capital_at = capital.0[civ]
            .and_then(|e| all.iter().find(|(id, _)| *id == e))
            .map(|(_, c)| (c.x, c.y));
        let double_wealth = research_state.knows_flag(civ, 0x1000);
        // Universal Suffrage's relief: one citizen per live wonder with the flag.
        let suffrage = mine
            .iter()
            .flat_map(|(_, c)| c.buildings.iter())
            .filter_map(|b| b.building_row())
            .filter(|&row| {
                let b = roster::bldg(row);
                b.wonder & roster::wonder::SUFFRAGE != 0 && !obsolete(row)
            })
            .count() as i32;
        let (war_counters, average_weariness) = diplomacy.as_ref().map_or((vec![], 0), |d| {
            (d.enemy_weariness(civ), d.average_weariness(civ))
        });
        write(civ, |r| {
            r.known = known;
            r.owned = owned;
            r.doubled = doubled;
            r.continent = continent;
            r.on_continent = on_continent;
            r.luxuries = luxuries.len();
            r.garrison = garrison;
            r.cities = mine.len();
            r.capital = capital_at;
            r.double_wealth = double_wealth;
            r.suffrage = suffrage;
            r.war_counters = war_counters;
            r.average_weariness = average_weariness;
            r.rating = flips.as_ref().map_or(0, |f| f.empire[civ] as i32);
            r.city_order = city_order;
            r.connected = connected;
            r.palaces = palaces;
            r.at_war = std::array::from_fn(|q| {
                diplomacy
                    .as_ref()
                    .is_some_and(|d| q != civ && d.at_war(civ, q))
            });
        });
        if changed {
            research_state.goods_changed();
        }
    }
    for row in 0..bldg_count() {
        let b = roster::bldg(row);
        if b.is_great_wonder() {
            research::set_wonder_built(Production::from_building_row(row), built[row]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn knowing(_civ: usize) -> Realm {
        let mut r = Realm::new();
        r.known = u128::MAX;
        r
    }

    #[test]
    fn rates_add_up_to_ten_and_shift_one_tenth() {
        let r = Rates::DEFAULT;
        assert!(r.valid(10));
        assert!(
            !Rates {
                tax: 6,
                sci: 5,
                lux: 0
            }
            .valid(10)
        );
        assert!(
            !Rates {
                tax: 11,
                sci: 0,
                lux: 0
            }
            .valid(10)
        );
        let moved = r.shifted(Rate::Sci, Rate::Lux, 10).unwrap();
        assert_eq!(
            moved,
            Rates {
                tax: 5,
                sci: 4,
                lux: 1
            }
        );
        // Nothing to take from.
        assert_eq!(
            Rates {
                tax: 10,
                sci: 0,
                lux: 0
            }
            .shifted(Rate::Sci, Rate::Tax, 10),
            None
        );
        // A cap of 60% refuses a seventh tenth.
        assert_eq!(
            Rates {
                tax: 6,
                sci: 4,
                lux: 0
            }
            .shifted(Rate::Sci, Rate::Tax, 6),
            None
        );
    }

    #[test]
    fn only_known_governments_can_be_chosen() {
        let mut r = Realm::new();
        assert_eq!(r.choices(), vec![row::DESPOTISM]);
        // Monarchy needs advance 19.
        r.known |= 1 << 19;
        assert_eq!(r.choices(), vec![row::DESPOTISM, row::MONARCHY]);
        assert!(!r.can_adopt(row::ANARCHY));
        assert!(knowing(0).can_adopt(row::DEMOCRACY));
    }

    #[test]
    fn a_revolution_runs_through_anarchy() {
        let mut r = knowing(0);
        r.revolt(row::REPUBLIC, 3);
        assert_eq!(r.govt, row::ANARCHY);
        assert!(!r.tick());
        assert!(!r.tick());
        assert_eq!(r.govt, row::ANARCHY);
        assert!(r.tick(), "the third turn ends it");
        assert_eq!(r.govt, row::REPUBLIC);
        assert_eq!(r.anarchy, 0);
    }

    #[test]
    fn a_one_turn_anarchy_is_no_anarchy() {
        let mut r = knowing(0);
        r.revolt(row::MONARCHY, 1);
        assert_eq!(r.govt, row::MONARCHY);
        r.revolt(row::MONARCHY, 0);
        assert_eq!(r.govt, row::MONARCHY);
    }

    #[test]
    fn anarchy_falls_back_to_despotism_when_the_choice_is_not_available() {
        let mut r = Realm::new();
        r.revolt(row::DEMOCRACY, 2);
        assert_eq!(r.govt, row::ANARCHY);
        r.tick();
        assert!(r.tick());
        assert_eq!(r.govt, row::DESPOTISM, "Democracy is not known");
    }

    #[test]
    fn the_choice_can_change_during_anarchy() {
        let mut r = knowing(0);
        r.revolt(row::REPUBLIC, 3);
        r.revolt(row::MONARCHY, 3);
        assert_eq!(r.then, row::MONARCHY);
        assert_eq!(r.anarchy, 3, "the countdown is not restarted");
    }

    #[test]
    fn the_process_wide_table_is_per_thread_under_test() {
        reset();
        assert_eq!(
            govt(0).corruption_class,
            SHIPPED[row::DESPOTISM].corruption_class
        );
        write(1, |r| r.adopt(row::MONARCHY));
        assert_eq!(read(1, |r| r.govt), row::MONARCHY);
        assert_eq!(read(0, |r| r.govt), row::DESPOTISM);
        assert!(set_rates(
            0,
            Rates {
                tax: 3,
                sci: 7,
                lux: 0
            }
        ));
        assert!(!set_rates(
            0,
            Rates {
                tax: 3,
                sci: 3,
                lux: 3
            }
        ));
        assert_eq!(rates(0).sci, 7);
    }

    /// `sync` reads the whole world, so it waits for a change: a figure the
    /// table has wrong stays wrong until something the pass reads moves.
    #[test]
    fn sync_runs_only_when_its_inputs_change() {
        reset();
        let mut app = App::new();
        app.insert_resource(GameMap::generate());
        app.insert_resource(Research::new());
        app.init_resource::<Capital>();
        app.insert_resource(crate::units::Turn(1));
        // The tables are per thread under test: run the pass on this one.
        app.edit_schedule(Update, |s| {
            s.set_executor_kind(bevy::ecs::schedule::ExecutorKind::SingleThreaded);
        });
        app.add_systems(Update, sync);
        let town = app.world_mut().spawn(City::new(0, "Kyoto", 10, 10)).id();
        app.update();
        assert_eq!(read(0, |r| r.cities), 1);

        // Spoil a figure: nothing changed, so the pass leaves it alone.
        write(0, |r| r.cities = 7);
        app.update();
        app.update();
        assert_eq!(
            read(0, |r| r.cities),
            7,
            "an idle world is not scanned again"
        );

        // A city that changed is noticed.
        app.world_mut().get_mut::<City>(town).unwrap().food += 1;
        app.update();
        assert_eq!(read(0, |r| r.cities), 1);

        // So is a government, which carries no change flag of its own.
        write(0, |r| r.cities = 7);
        write(0, |r| r.adopt(row::MONARCHY));
        app.update();
        assert_eq!(read(0, |r| r.cities), 1);

        // A soldier arriving or walking about moves the garrison tables and
        // nothing else: the rest of the pass is not run for it.
        write(0, |r| r.cities = 7);
        let guard = app
            .world_mut()
            .spawn(Unit::new(
                0,
                crate::units::UnitType::named("Warrior"),
                11,
                10,
            ))
            .id();
        app.update();
        assert_eq!(
            read(0, |r| (r.cities, r.garrison.get(&(11, 10)).copied())),
            (7, Some(1))
        );
        app.world_mut().get_mut::<Unit>(guard).unwrap().x = 12;
        app.update();
        assert_eq!(
            read(0, |r| (
                r.cities,
                r.garrison.get(&(11, 10)).copied(),
                r.garrison.get(&(12, 10)).copied()
            )),
            (7, None, Some(1))
        );
        // A civilian is no garrison.
        app.world_mut().spawn(Unit::new(
            0,
            crate::units::UnitType::named("Worker"),
            13,
            10,
        ));
        app.update();
        assert_eq!(read(0, |r| r.garrison.get(&(13, 10)).copied()), None);
        reset();
    }

    #[test]
    fn continents_split_land_by_water() {
        let mut map = GameMap::generate();
        for t in map.tiles.iter_mut() {
            t.base = crate::map::Base::Ocean;
        }
        let a = map.idx(3, 3);
        let b = map.idx(10, 10);
        map.tiles[a].base = crate::map::Base::Grassland;
        let a2 = map.idx(4, 4);
        map.tiles[a2].base = crate::map::Base::Plains;
        map.tiles[b].base = crate::map::Base::Plains;
        let l = continents(&map);
        assert_eq!(l[map.idx(3, 3)], l[map.idx(4, 4)]);
        assert_ne!(l[map.idx(3, 3)], l[map.idx(10, 10)]);
        assert_eq!(l[map.idx(0, 0)], 0);
    }
}
