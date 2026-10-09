//! Deterministic strategy simulation. No engine, clock, filesystem, networking, or Lua VM.
pub mod ai;
pub mod battle;
pub mod borders;
pub mod calendar;
pub mod combat;
pub mod economy;
pub mod improvements;
pub mod movement;
pub mod naval;
pub mod scenario;
pub mod terrain;
pub mod units;

#[cfg(test)]
pub(crate) mod tests;

pub use battle::{Battle, Fighter, Outcome, Support};
pub use calendar::Date;
pub use combat::Estimate;
pub use economy::Yield;
pub use improvements::Job;
pub use movement::March;
pub use scenario::{
    CityStart, Flavor, NationStart, RegionStart, Rgb, RuleOverrides, SCENARIO_FORMAT, Scenario,
    Status, UnitStart, WarStart,
};
pub use units::{Domain, Order, Unit, UnitDef};

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use terrain::{Coord, Explored, Map, Terrain};

pub type Id = u32;
pub const PROTOCOL_VERSION: u32 = 4;
/// Tiles revealed around every city and unit (Chebyshev distance).
pub const VISION_RADIUS: i32 = 5;
/// Dispatches kept for the interface.
const LOG_LIMIT: usize = 24;

/// Lookup tables from squares to the units and cities on them. They are derived from `units`
/// and `cities`, never serialized, and rebuilt by [`Game::reindex`]. A game without them
/// still answers every query, only slowly.
#[derive(Clone, Debug, Default)]
struct Index {
    built: bool,
    units: HashMap<u32, Vec<Id>>,
    cities: HashMap<u32, Id>,
}
impl PartialEq for Index {
    /// Derived data never makes two otherwise identical games differ.
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

/// The unit designs and scenario-adjustable constants of the active content pack.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Rules {
    pub units: BTreeMap<String, UnitDef>,
    pub victory_industry: i32,
    pub research_cost: i32,
    pub starting_gold: i32,
}
impl Rules {
    /// A unit's design. Content validation guarantees every unit kind exists.
    pub fn def(&self, unit: &Unit) -> &UnitDef {
        &self.units[&unit.kind]
    }
}

/// Per-turn adjustments from the content pack's script.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct TickRules {
    pub income_percent: i32,
}
impl Default for TickRules {
    fn default() -> Self {
        Self {
            income_percent: 100,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Faction {
    pub id: Id,
    /// The scenario's stable nation identifier.
    pub tag: String,
    pub name: String,
    pub adjective: String,
    pub color: Rgb,
    /// Which architecture the nation's cities are drawn in.
    pub flavor: Flavor,
    pub status: Status,
    pub suzerain: Option<Id>,
    pub government: String,
    pub leader: String,
    pub gold: i32,
    pub research: i32,
    pub technology: u32,
    pub industry: i32,
    pub explored: Explored,
}
impl Faction {
    /// What another player may learn about this nation: public facts only.
    fn masked(&self) -> Self {
        Self {
            id: self.id,
            tag: self.tag.clone(),
            name: self.name.clone(),
            adjective: self.adjective.clone(),
            color: self.color,
            flavor: self.flavor,
            status: self.status,
            suzerain: self.suzerain,
            government: self.government.clone(),
            leader: self.leader.clone(),
            gold: 0,
            research: 0,
            technology: self.technology,
            industry: self.industry,
            explored: Explored::default(),
        }
    }
}

/// A named area of the map and the nation that held it when the scenario began.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Region {
    pub id: String,
    pub name: String,
    pub nation: Id,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct City {
    pub id: Id,
    pub owner: Id,
    pub name: String,
    pub position: Coord,
    pub population: i32,
    pub industry: i32,
    pub capital: bool,
    /// Cultural border level 1–6. Fixed for the life of the city.
    pub border: u8,
    pub production: Option<String>,
    pub progress: i32,
    /// Food, shields, and gold the land of the city's border region yields each day. Derived
    /// from the map and recounted after every command; saved so that clients, which only see
    /// part of the map, can show it. See [`economy`].
    #[serde(default)]
    pub harvest: Yield,
    /// Surplus food stored toward the next citizen. See [`economy`].
    #[serde(default)]
    pub granary: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Game {
    pub seed: u64,
    pub rng: u64,
    pub revision: u64,
    /// Turn 1 is the scenario's start date; each turn is one day.
    pub turn: u32,
    pub start_date: Date,
    pub scenario: String,
    pub scenario_name: String,
    pub briefing: String,
    /// The nation the human player commands. Every other nation is computer-controlled.
    pub commander: Id,
    pub map: Map,
    pub regions: Vec<Region>,
    #[serde(with = "id_map")]
    pub factions: BTreeMap<Id, Faction>,
    #[serde(with = "id_map")]
    pub cities: BTreeMap<Id, City>,
    #[serde(with = "id_map")]
    pub units: BTreeMap<Id, Unit>,
    /// Pairs of nations at war, stored with the lower ID first.
    pub wars: BTreeSet<(Id, Id)>,
    pub log: Vec<String>,
    /// The fights of the latest command, in order, for the interface to replay. Cleared by the
    /// next [`Game::apply`]. See [`battle`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub battles: Vec<Battle>,
    /// The walks of the latest command, in order, for the interface to replay. Cleared by the
    /// next [`Game::apply`]. See [`movement::March`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub marches: Vec<March>,
    pub winner: Option<Id>,
    pub next_id: Id,
    #[serde(skip)]
    index: Box<Index>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    /// March toward a square, continuing each turn until there. Never attacks. A ship may
    /// sail into its own coastal cities; a land unit boards by moving onto an adjacent ship
    /// and lands by moving from the ship onto the shore.
    Move {
        unit: Id,
        destination: Coord,
    },
    /// Board a ship that shares the unit's square in port. Spends no movement.
    Load {
        unit: Id,
        carrier: Id,
    },
    /// In port, set a passenger ashore, or every passenger when given the ship.
    Unload {
        unit: Id,
    },
    /// Melee attack on an adjacent square, or capture of what is undefended there.
    Attack {
        unit: Id,
        target: Coord,
    },
    /// Ranged attack that wounds but never kills.
    Bombard {
        unit: Id,
        target: Coord,
    },
    Fortify {
        unit: Id,
    },
    /// Drop any march, fortification, or job.
    Cancel {
        unit: Id,
    },
    /// Build an improvement on the unit's own square.
    Work {
        unit: Id,
        job: Job,
    },
    FoundCity {
        unit: Id,
        name: String,
    },
    Disband {
        unit: Id,
    },
    Produce {
        city: Id,
        unit: String,
    },
    Develop {
        city: Id,
    },
    EndTurn,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request {
    pub version: u32,
    pub sequence: u64,
    pub revision: u64,
    pub command: Command,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum Response {
    Snapshot {
        version: u32,
        player: Id,
        game: Game,
        rules: Rules,
    },
    Rejected {
        sequence: u64,
        reason: String,
    },
}

#[derive(Debug, thiserror::Error, PartialEq)]
#[error("{0}")]
pub struct GameError(pub String);
type Result<T> = std::result::Result<T, GameError>;
fn error(message: &str) -> GameError {
    GameError(message.into())
}

// JSON object keys are strings. Tagged enum buffering uses Serde's generic
// deserializer, which cannot coerce those keys to u32 as serde_json normally does.
mod id_map {
    use super::*;
    pub fn serialize<T: Serialize, S: serde::Serializer>(
        value: &BTreeMap<Id, T>,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        value.serialize(serializer)
    }
    pub fn deserialize<'de, T: Deserialize<'de>, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<BTreeMap<Id, T>, D::Error> {
        BTreeMap::<String, T>::deserialize(deserializer)?
            .into_iter()
            .map(|(key, value)| {
                key.parse()
                    .map(|id| (id, value))
                    .map_err(serde::de::Error::custom)
            })
            .collect()
    }
}

fn war_key(a: Id, b: Id) -> (Id, Id) {
    (a.min(b), a.max(b))
}

impl Game {
    /// Build the opening position. `commander` overrides the scenario's default nation.
    ///
    /// Content validation (see `fourx-content`) checks every cross-reference up front; this
    /// still returns an error rather than panicking if handed an inconsistent scenario.
    pub fn from_scenario(
        seed: u64,
        rules: &Rules,
        scenario: &Scenario,
        commander: Option<&str>,
    ) -> Result<Self> {
        let ids: BTreeMap<&str, Id> = scenario
            .nations
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.as_str(), i as Id + 1))
            .collect();
        let nation = |tag: &str| {
            ids.get(tag)
                .copied()
                .ok_or_else(|| GameError(format!("Unknown nation {tag:?}")))
        };
        let commander_tag = commander.unwrap_or(&scenario.commander);
        let commander = nation(commander_tag)?;
        let regions = scenario
            .regions
            .iter()
            .map(|r| {
                Ok(Region {
                    id: r.id.clone(),
                    name: r.name.clone(),
                    nation: nation(&r.nation)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let mut map = scenario.map.clone();
        for tile in &mut map.tiles {
            if usize::from(tile.region) > regions.len() {
                return Err(error("Map references a region that does not exist"));
            }
            // Territory comes from the cities, never from the scenario's regions.
            tile.owner = 0;
            tile.claim = 0;
        }
        let mut factions = BTreeMap::new();
        for (i, n) in scenario.nations.iter().enumerate() {
            let id = i as Id + 1;
            factions.insert(
                id,
                Faction {
                    id,
                    tag: n.id.clone(),
                    name: n.name.clone(),
                    adjective: if n.adjective.is_empty() {
                        n.name.clone()
                    } else {
                        n.adjective.clone()
                    },
                    color: n.color,
                    flavor: n.flavor,
                    status: n.status,
                    suzerain: n.suzerain.as_deref().map(nation).transpose()?,
                    government: n.government.clone(),
                    leader: n.leader.clone(),
                    gold: n.gold.unwrap_or(rules.starting_gold),
                    research: 0,
                    technology: n.technology,
                    industry: 0,
                    explored: Explored::new(map.width, map.height),
                },
            );
        }
        let mut game = Self {
            seed,
            rng: seed.max(1),
            revision: 0,
            turn: 1,
            start_date: scenario.start_date,
            scenario: scenario.id.clone(),
            scenario_name: scenario.name.clone(),
            briefing: scenario.intro.clone(),
            commander,
            map,
            regions,
            factions,
            cities: BTreeMap::new(),
            units: BTreeMap::new(),
            wars: BTreeSet::new(),
            log: Vec::new(),
            battles: Vec::new(),
            marches: Vec::new(),
            winner: None,
            next_id: 1,
            index: Box::default(),
        };
        for start in &scenario.cities {
            let owner = nation(&start.nation)?;
            let id = game.add_city(owner, start.position, start.name.clone());
            let city = game.cities.get_mut(&id).unwrap();
            city.population = start.population;
            city.granary = economy::starting_granary(start.population);
            city.industry = start.industry;
            city.capital = start.capital;
            city.border = start
                .border
                .unwrap_or_else(|| borders::default_level(start.population, start.capital));
        }
        game.assign_start_territory();
        game.refresh_harvest();
        for start in &scenario.units {
            let owner = nation(&start.nation)?;
            let def = rules
                .units
                .get(&start.kind)
                .ok_or_else(|| error("Scenario unit uses an unknown unit design"))?;
            if !game.passable(def.domain, owner, start.position) {
                return Err(error("Scenario places a unit on the wrong terrain"));
            }
            let id = game.add_unit(owner, &start.kind, start.position);
            let unit = game.units.get_mut(&id).unwrap();
            if let Some(level) = start.level {
                unit.level = level.min(3);
            }
            if start.fortified {
                unit.order = Order::Fortified;
            }
        }
        for war in &scenario.wars {
            game.wars.insert(war_key(nation(&war.a)?, nation(&war.b)?));
        }
        if !scenario.intro.is_empty() {
            game.note(scenario.intro.clone());
        }
        if scenario.charted {
            // Every tile that exists; the void around a lattice is not part of the chart.
            let mut chart = Explored::new(game.map.width, game.map.height);
            for p in game.map.positions() {
                chart.insert(p);
            }
            for faction in game.factions.values_mut() {
                faction.explored = chart.clone();
            }
        }
        game.reindex();
        game.reveal();
        Ok(game)
    }

    /// Calendar date of the current turn.
    pub fn date(&self) -> Date {
        self.start_date.add_days(self.turn.saturating_sub(1))
    }
    pub fn faction_by_tag(&self, tag: &str) -> Option<&Faction> {
        self.factions.values().find(|f| f.tag == tag)
    }
    pub fn region_of(&self, position: Coord) -> Option<&Region> {
        let tile = self.map.get(position)?;
        self.regions.get(usize::from(tile.region).checked_sub(1)?)
    }
    pub fn at_war(&self, a: Id, b: Id) -> bool {
        self.wars.contains(&war_key(a, b))
    }
    fn declare_war(&mut self, a: Id, b: Id) {
        if a != b && a != 0 && b != 0 && self.wars.insert(war_key(a, b)) {
            self.note(format!(
                "{} and {} are at war.",
                self.factions[&a].name, self.factions[&b].name
            ));
        }
    }
    /// Append a dispatch prefixed with the current date.
    fn note(&mut self, text: String) {
        let date = self.date();
        self.log.push(format!(
            "{} {} {} | {}",
            date.day(),
            &date.month_name()[..3],
            date.year(),
            text
        ));
    }

    fn id(&mut self) -> Id {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
    fn add_city(&mut self, owner: Id, position: Coord, name: String) -> Id {
        let id = self.id();
        self.cities.insert(
            id,
            City {
                id,
                owner,
                name,
                position,
                population: 3,
                industry: 4,
                capital: false,
                border: borders::default_level(3, false),
                production: None,
                progress: 0,
                harvest: Yield::default(),
                granary: economy::starting_granary(3),
            },
        );
        self.index_city(id);
        id
    }
    fn random(&mut self, max: u32) -> i32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng % u64::from(max)) as i32
    }

    fn own_unit(&self, player: Id, id: Id) -> Result<&Unit> {
        let unit = self.units.get(&id).ok_or_else(|| error("Unknown unit"))?;
        if unit.owner != player {
            return Err(error("You do not command this unit"));
        }
        Ok(unit)
    }
    /// A unit of the player's that is not riding a ship, for the orders that need solid ground.
    fn own_ashore(&self, player: Id, id: Id) -> Result<&Unit> {
        let unit = self.own_unit(player, id)?;
        if unit.carrier.is_some() {
            return Err(error("Come ashore before giving this order"));
        }
        Ok(unit)
    }
    fn own_city(&self, player: Id, id: Id) -> Result<&City> {
        let city = self.cities.get(&id).ok_or_else(|| error("Unknown city"))?;
        if city.owner != player {
            return Err(error("You do not own this city"));
        }
        Ok(city)
    }

    /// Carry out one player's command and keep the shared bookkeeping current.
    pub fn apply(
        &mut self,
        player: Id,
        command: Command,
        rules: &Rules,
        tick: TickRules,
    ) -> Result<()> {
        if self.winner.is_some() {
            return Err(error("The campaign has ended"));
        }
        if !self.factions.contains_key(&player) {
            return Err(error("Unknown player"));
        }
        self.battles.clear();
        self.marches.clear();
        self.execute(player, command, rules, tick)?;
        self.reveal();
        // A finished improvement, a new city, or a turn of computer play may all have changed
        // what a city's land yields; the interface shows the count straight away.
        self.refresh_harvest();
        self.revision += 1;
        self.log.drain(..self.log.len().saturating_sub(LOG_LIMIT));
        Ok(())
    }

    /// The command itself, without the bookkeeping of [`Game::apply`].
    pub(crate) fn execute(
        &mut self,
        player: Id,
        command: Command,
        rules: &Rules,
        tick: TickRules,
    ) -> Result<()> {
        match command {
            Command::Move { unit, destination } => {
                self.move_unit(player, unit, destination, rules, usize::MAX)?
            }
            Command::Load { unit, carrier } => self.load(player, unit, carrier, rules)?,
            Command::Unload { unit } => self.unload(player, unit, rules)?,
            Command::Attack { unit, target } => self.attack(player, unit, target, rules)?,
            Command::Bombard { unit, target } => self.bombard(player, unit, target, rules)?,
            Command::Fortify { unit } => {
                let u = self.own_ashore(player, unit)?;
                let def = rules.def(u);
                if def.domain != Domain::Land || def.defense <= 0 {
                    return Err(error("This unit cannot fortify"));
                }
                let u = self.units.get_mut(&unit).unwrap();
                if u.order != Order::Fortified {
                    u.clear_orders();
                    u.order = Order::Fortifying;
                }
            }
            Command::Cancel { unit } => {
                self.own_unit(player, unit)?;
                self.units.get_mut(&unit).unwrap().clear_orders();
            }
            Command::Work { unit, job } => {
                let u = self.own_ashore(player, unit)?;
                if !rules.def(u).can_work() {
                    return Err(error("This unit cannot build improvements"));
                }
                self.can_improve(u.position, job).map_err(error)?;
                self.start_work(unit, job, rules);
            }
            Command::FoundCity { unit, name } => {
                let u = self.own_ashore(player, unit)?;
                if !rules.def(u).settler {
                    return Err(error("A pioneer is required"));
                }
                if name.trim().is_empty() || name.chars().count() > 32 {
                    return Err(error("Use a city name of 1–32 characters"));
                }
                if self
                    .cities
                    .values()
                    .any(|c| c.position.distance(u.position) < 3)
                {
                    return Err(error("Cities must be at least three tiles apart"));
                }
                let pos = u.position;
                let tile = self.map.get(pos).unwrap();
                if !tile.is_land() {
                    return Err(error("Cannot settle water"));
                }
                if tile.owner != 0 && tile.owner != player {
                    return Err(error("Cannot found a city in foreign territory"));
                }
                self.remove_unit(unit);
                let id = self.add_city(player, pos, name.trim().into());
                self.claim_for_new_city(id);
                self.note(format!(
                    "{} founded a new port.",
                    self.factions[&player].name
                ));
            }
            Command::Disband { unit } => {
                self.own_unit(player, unit)?;
                self.remove_unit(unit);
            }
            Command::Produce { city, unit } => {
                let def = rules
                    .units
                    .get(&unit)
                    .ok_or_else(|| error("Unknown unit design"))?;
                let c = self.own_city(player, city)?;
                if def.is_naval() && !self.coastal(c.position) {
                    return Err(error("Ships require a coastal city"));
                }
                let c = self.cities.get_mut(&city).unwrap();
                c.production = Some(unit);
                c.progress = 0;
            }
            Command::Develop { city } => {
                self.own_city(player, city)?;
                let faction = self.factions.get_mut(&player).unwrap();
                if faction.gold < 40 {
                    return Err(error("Development costs 40 gold"));
                }
                faction.gold -= 40;
                self.cities.get_mut(&city).unwrap().industry += 2;
            }
            Command::EndTurn => {
                if player != self.commander {
                    return Err(error("Only the campaign host can advance time"));
                }
                self.advance(rules, tick);
            }
        }
        Ok(())
    }

    /// Whether a square touches water.
    pub fn coastal(&self, position: Coord) -> bool {
        self.map
            .neighbors(position)
            .iter()
            .any(|p| self.map.get(*p).is_some_and(|t| t.is_water()))
    }

    fn advance(&mut self, rules: &Rules, tick: TickRules) {
        self.turn += 1;
        // Count the land afresh, so the day's income never rests on an earlier count.
        self.refresh_harvest();
        let mut completed = Vec::new();
        let mut gold: BTreeMap<Id, i32> = BTreeMap::new();
        let mut famines = Vec::new();
        for city in self.cities.values_mut() {
            let faction = self.factions.get_mut(&city.owner).unwrap();
            *gold.entry(city.owner).or_default() += city.harvest.gold;
            faction.research += city.population + city.industry / 2;
            faction.industry += city.shields();
            if city.eat() == Some(economy::Change::Starved) && city.owner == self.commander {
                famines.push(format!(
                    "Famine in {}: its population falls to {}.",
                    city.name, city.population
                ));
            }
            if let Some(kind) = &city.production {
                city.progress += city.shields();
                if city.progress >= rules.units[kind].cost {
                    completed.push((city.owner, city.position, kind.clone()));
                    city.progress = 0;
                    city.production = None;
                }
            }
        }
        // The script scales a nation's whole income once, so small cities are not rounded away.
        for (owner, taken) in gold {
            self.factions.get_mut(&owner).unwrap().gold += taken * tick.income_percent / 100;
        }
        for message in famines {
            self.note(message);
        }
        let mut unlocked = Vec::new();
        for faction in self.factions.values_mut() {
            let cost = rules.research_cost * (faction.technology as i32 + 1);
            if faction.research >= cost {
                faction.research -= cost;
                faction.technology += 1;
                unlocked.push(format!(
                    "{} unlocked technology {}.",
                    faction.name, faction.technology
                ));
            }
        }
        for message in unlocked {
            self.note(message);
        }
        // A new ship floats out into its home port; `Produce` checked that the city is coastal.
        for (owner, pos, kind) in completed {
            self.add_unit(owner, &kind, pos);
        }
        self.refresh_units(rules);
        self.continue_marches(rules);
        self.work_turn(rules);
        self.ai(rules);
        let winner = self.factions.values().find_map(|faction| {
            let other_has_city = self.cities.values().any(|c| c.owner != faction.id);
            (faction.industry >= rules.victory_industry || !other_has_city)
                .then(|| (faction.id, faction.name.clone()))
        });
        if let Some((id, name)) = winner {
            self.winner = Some(id);
            self.note(format!("{name} wins the campaign!"));
        }
    }

    /// Start of turn for every unit: heal if it rested, restore movement and the once-a-turn
    /// abilities, and finish digging in.
    fn refresh_units(&mut self, rules: &Rules) {
        let ids: Vec<Id> = self.units.keys().copied().collect();
        for id in ids {
            let unit = &self.units[&id];
            let heal = if unit.damage > 0 && unit.moves_used == 0 {
                self.healing(unit, rules.def(unit))
            } else {
                0
            };
            let unit = self.units.get_mut(&id).unwrap();
            unit.damage = (unit.damage - heal).max(0);
            unit.moves_used = 0;
            unit.attacked = false;
            unit.fired = false;
            unit.promotion_failed = false;
            if unit.order == Order::Fortifying {
                unit.order = Order::Fortified;
            }
        }
    }
    /// Hit points a resting unit recovers: 2 in a city, 1 in the open on friendly or unclaimed
    /// land, none in foreign territory. A ship at sea never heals; it mends in its own port.
    fn healing(&self, unit: &Unit, def: &UnitDef) -> i32 {
        let Some(tile) = self.map.get(unit.position) else {
            return 0;
        };
        if def.is_naval() {
            return if self.is_port(unit.owner, unit.position) {
                2
            } else {
                0
            };
        }
        if self.city_at(unit.position).is_some() {
            2
        } else if tile.owner == 0 || tile.owner == unit.owner {
            1
        } else {
            0
        }
    }

    fn reveal(&mut self) {
        for city in self.cities.values() {
            if let Some(f) = self.factions.get_mut(&city.owner) {
                f.explored.reveal(city.position, VISION_RADIUS);
            }
        }
        for unit in self.units.values() {
            if let Some(f) = self.factions.get_mut(&unit.owner) {
                f.explored.reveal(unit.position, VISION_RADIUS);
            }
        }
    }

    /// A player sees only explored terrain and enemies currently near their units/cities.
    pub fn view(&self, player: Id) -> Self {
        if player == 0 {
            return self.clone();
        }
        let Some(me) = self.factions.get(&player) else {
            return self.clone();
        };
        let sources: Vec<_> = self
            .units
            .values()
            .filter(|u| u.owner == player)
            .map(|u| u.position)
            .chain(
                self.cities
                    .values()
                    .filter(|c| c.owner == player)
                    .map(|c| c.position),
            )
            .collect();
        let visible = |pos: Coord| sources.iter().any(|p| p.distance(pos) <= VISION_RADIUS);
        let mut map = self.map.clone();
        for tile in &mut map.tiles {
            if !me.explored.contains(tile.position) {
                tile.set_terrain(Terrain::Ocean);
                tile.owner = 0;
                tile.claim = 0;
                tile.region = 0;
                tile.improvements = 0;
            }
        }
        let units = self
            .units
            .iter()
            .filter(|(_, u)| u.owner == player || visible(u.position))
            .map(|(id, u)| (*id, u.clone()))
            .collect();
        Self {
            seed: self.seed,
            rng: 0,
            revision: self.revision,
            turn: self.turn,
            start_date: self.start_date,
            scenario: self.scenario.clone(),
            scenario_name: self.scenario_name.clone(),
            briefing: self.briefing.clone(),
            commander: self.commander,
            map,
            regions: self.regions.clone(),
            factions: self
                .factions
                .iter()
                .map(|(id, f)| (*id, if *id == player { f.clone() } else { f.masked() }))
                .collect(),
            cities: self
                .cities
                .iter()
                .filter(|(_, c)| c.owner == player || me.explored.contains(c.position))
                .map(|(id, c)| (*id, c.clone()))
                .collect(),
            units,
            wars: self.wars.clone(),
            log: self.log.clone(),
            battles: self
                .battles
                .iter()
                .filter(|b| Self::witnessed(b, player, visible))
                .cloned()
                .collect(),
            marches: self
                .marches
                .iter()
                .filter_map(|m| m.seen_by(player, visible))
                .collect(),
            winner: self.winner,
            next_id: self.next_id,
            index: Box::default(),
        }
    }
}
