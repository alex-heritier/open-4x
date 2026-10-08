//! Deterministic strategy simulation. No engine, clock, filesystem, networking, or Lua VM.
pub mod terrain;

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use terrain::{Coord, Map, Terrain};

pub type Id = u32;
pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Scenario {
    pub width: i32,
    pub height: i32,
    pub player_name: String,
    pub enemy_name: String,
    pub cities: Vec<CityStart>,
    pub armies: Vec<ArmyStart>,
    #[serde(default)]
    pub tiles: Vec<terrain::Tile>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CityStart {
    pub owner: Id,
    pub position: Coord,
    pub name: String,
    pub population: i32,
    pub industry: i32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArmyStart {
    pub owner: Id,
    pub position: Coord,
    pub name: String,
    pub kinds: Vec<String>,
    pub general: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct UnitDef {
    pub id: String,
    pub name: String,
    pub cost: i32,
    pub fire: i32,
    pub shock: i32,
    pub speed: u32,
    pub naval: bool,
    pub settler: bool,
    pub sprite: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Rules {
    pub units: BTreeMap<String, UnitDef>,
    pub combat_width: usize,
    pub victory_industry: i32,
    pub research_cost: i32,
    pub starting_gold: i32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct TickRules {
    pub income_percent: i32,
    pub fire_percent: i32,
    pub shock_percent: i32,
}
impl Default for TickRules {
    fn default() -> Self {
        Self {
            income_percent: 100,
            fire_percent: 100,
            shock_percent: 100,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Faction {
    pub id: Id,
    pub name: String,
    pub gold: i32,
    pub research: i32,
    pub technology: u32,
    pub industry: i32,
    pub explored: BTreeSet<Coord>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct City {
    pub id: Id,
    pub owner: Id,
    pub name: String,
    pub position: Coord,
    pub population: i32,
    pub industry: i32,
    pub production: Option<String>,
    pub progress: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Regiment {
    pub kind: String,
    pub strength: i32,
    pub morale: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Army {
    pub id: Id,
    pub owner: Id,
    pub name: String,
    pub position: Coord,
    pub regiments: Vec<Regiment>,
    pub movement: u32,
    pub general: i32,
}
impl Army {
    pub fn strength(&self) -> i32 {
        self.regiments.iter().map(|r| r.strength).sum()
    }
    pub fn morale(&self) -> i32 {
        if self.regiments.is_empty() {
            0
        } else {
            self.regiments.iter().map(|r| r.morale).sum::<i32>() / self.regiments.len() as i32
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Battle {
    pub attacker: Id,
    pub defender: Id,
    pub day: u32,
    pub position: Coord,
    pub phase: String,
    pub attacker_losses: i32,
    pub defender_losses: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Game {
    pub seed: u64,
    pub rng: u64,
    pub revision: u64,
    pub turn: u32,
    pub map: Map,
    #[serde(with = "id_map")]
    pub factions: BTreeMap<Id, Faction>,
    #[serde(with = "id_map")]
    pub cities: BTreeMap<Id, City>,
    #[serde(with = "id_map")]
    pub armies: BTreeMap<Id, Army>,
    pub battles: Vec<Battle>,
    pub log: Vec<String>,
    pub winner: Option<Id>,
    pub next_id: Id,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    Move { army: Id, destination: Coord },
    FoundCity { army: Id, name: String },
    Produce { city: Id, unit: String },
    Develop { city: Id },
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

impl Game {
    pub fn from_scenario(seed: u64, rules: &Rules, scenario: &Scenario) -> Self {
        // Content validation guarantees dimensions, owners, kinds, and positions.
        let mut game = Self::new(seed, rules);
        game.map = Map::archipelago(scenario.width, scenario.height, seed);
        game.cities.clear();
        game.armies.clear();
        game.next_id = 1;
        for tile in &scenario.tiles {
            *game.map.get_mut(tile.position).unwrap() = tile.clone();
        }
        game.factions.get_mut(&1).unwrap().name = scenario.player_name.clone();
        game.factions.get_mut(&2).unwrap().name = scenario.enemy_name.clone();
        for faction in game.factions.values_mut() {
            faction.explored.clear();
        }
        for start in &scenario.cities {
            game.map.get_mut(start.position).unwrap().terrain = Terrain::Grass;
            game.add_city(start.owner, start.position, start.name.clone());
            let city = game.cities.get_mut(&(game.next_id - 1)).unwrap();
            city.population = start.population;
            city.industry = start.industry;
        }
        for start in &scenario.armies {
            let naval = rules.units[&start.kinds[0]].naval;
            game.map.get_mut(start.position).unwrap().terrain = if naval {
                Terrain::Water
            } else {
                Terrain::Grass
            };
            game.add_army(start.owner, start.position, start.kinds.clone(), rules);
            let army = game.armies.get_mut(&(game.next_id - 1)).unwrap();
            army.name = start.name.clone();
            army.general = start.general;
        }
        game.reveal();
        game
    }
    pub fn new(seed: u64, rules: &Rules) -> Self {
        let map = Map::archipelago(24, 18, seed);
        let factions = [(1, "The Dawn Empire"), (2, "The Northern League")]
            .into_iter()
            .map(|(id, name)| {
                (
                    id,
                    Faction {
                        id,
                        name: name.into(),
                        gold: rules.starting_gold,
                        research: 0,
                        technology: 0,
                        industry: 0,
                        explored: BTreeSet::new(),
                    },
                )
            })
            .collect();
        let mut game = Self {
            seed,
            rng: seed.max(1),
            revision: 0,
            turn: 1,
            map,
            factions,
            cities: BTreeMap::new(),
            armies: BTreeMap::new(),
            battles: Vec::new(),
            log: vec!["1892 | The straits are open. An empire awaits your orders.".into()],
            winner: None,
            next_id: 1,
        };
        for (owner, pos, name) in [
            (1, Coord::new(8, 9), "Akatsuki"),
            (2, Coord::new(16, 8), "Northwatch"),
        ] {
            game.map.get_mut(pos).unwrap().terrain = Terrain::Grass;
            game.add_city(owner, pos, name.into());
            let kinds: Vec<_> = rules
                .units
                .values()
                .filter(|u| !u.naval && !u.settler)
                .map(|u| u.id.clone())
                .collect();
            game.add_army(owner, pos, kinds, rules);
            if let Some(settler) = rules.units.values().find(|u| u.settler) {
                game.add_army(owner, pos, vec![settler.id.clone()], rules);
            }
        }
        if let Some(ship) = rules.units.values().find(|u| u.naval) {
            for (owner, pos) in [(1, Coord::new(4, 13)), (2, Coord::new(4, 16))] {
                game.map.get_mut(pos).unwrap().terrain = Terrain::Water;
                game.add_army(owner, pos, vec![ship.id.clone()], rules);
            }
        }
        game.reveal();
        game
    }
    fn id(&mut self) -> Id {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
    fn add_city(&mut self, owner: Id, position: Coord, name: String) {
        let id = self.id();
        self.cities.insert(
            id,
            City {
                id,
                owner,
                position,
                name,
                population: 3,
                industry: 4,
                production: None,
                progress: 0,
            },
        );
    }
    fn add_army(&mut self, owner: Id, position: Coord, kinds: Vec<String>, rules: &Rules) {
        let id = self.id();
        let regiments: Vec<_> = kinds
            .into_iter()
            .map(|kind| Regiment {
                kind,
                strength: 1000,
                morale: 100,
            })
            .collect();
        let movement = regiments
            .iter()
            .map(|r| rules.units[&r.kind].speed)
            .min()
            .unwrap_or(0);
        self.armies.insert(
            id,
            Army {
                id,
                owner,
                position,
                name: format!("{} {}", if owner == 1 { "Imperial" } else { "League" }, id),
                regiments,
                movement,
                general: 2,
            },
        );
    }
    pub fn in_battle(&self, army: Id) -> bool {
        self.battles
            .iter()
            .any(|b| b.attacker == army || b.defender == army)
    }
    fn random(&mut self, max: u32) -> i32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng % u64::from(max)) as i32
    }
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
        match command {
            Command::Move { army, destination } => {
                self.move_army(player, army, destination, rules)?
            }
            Command::FoundCity { army, name } => {
                let a = self
                    .armies
                    .get(&army)
                    .ok_or_else(|| error("Unknown army"))?;
                if a.owner != player {
                    return Err(error("You do not command this army"));
                }
                if self.in_battle(army) {
                    return Err(error("Army is engaged in battle"));
                }
                if !a.regiments.iter().any(|r| rules.units[&r.kind].settler) {
                    return Err(error("A pioneer regiment is required"));
                }
                if name.trim().is_empty() || name.chars().count() > 32 {
                    return Err(error("Use a city name of 1–32 characters"));
                }
                if self
                    .cities
                    .values()
                    .any(|c| c.position.distance(a.position) < 3)
                {
                    return Err(error("Cities must be at least three tiles apart"));
                }
                let pos = a.position;
                if self.map.get(pos).unwrap().terrain == Terrain::Water {
                    return Err(error("Cannot settle water"));
                }
                self.armies
                    .get_mut(&army)
                    .unwrap()
                    .regiments
                    .retain(|r| !rules.units[&r.kind].settler);
                if self.armies[&army].regiments.is_empty() {
                    self.armies.remove(&army);
                }
                self.add_city(player, pos, name.trim().into());
                self.log.push(format!(
                    "{} founded a new port.",
                    self.factions[&player].name
                ));
            }
            Command::Produce { city, unit } => {
                let def = rules
                    .units
                    .get(&unit)
                    .ok_or_else(|| error("Unknown unit design"))?;
                let c = self
                    .cities
                    .get_mut(&city)
                    .ok_or_else(|| error("Unknown city"))?;
                if c.owner != player {
                    return Err(error("You do not own this city"));
                }
                if def.naval
                    && !self
                        .map
                        .neighbors(c.position)
                        .iter()
                        .any(|p| self.map.get(*p).unwrap().terrain == Terrain::Water)
                {
                    return Err(error("Ships require a coastal city"));
                }
                c.production = Some(unit);
                c.progress = 0;
            }
            Command::Develop { city } => {
                let c = self
                    .cities
                    .get_mut(&city)
                    .ok_or_else(|| error("Unknown city"))?;
                if c.owner != player {
                    return Err(error("You do not own this city"));
                }
                let faction = self.factions.get_mut(&player).unwrap();
                if faction.gold < 40 {
                    return Err(error("Development costs 40 gold"));
                }
                faction.gold -= 40;
                c.industry += 2;
            }
            Command::EndTurn => {
                if player != 1 {
                    return Err(error("Only the campaign host can advance time"));
                }
                self.advance(rules, tick);
            }
        }
        self.reveal();
        self.revision += 1;
        self.log.drain(..self.log.len().saturating_sub(12));
        Ok(())
    }
    pub fn path(&self, start: Coord, goal: Coord, naval: bool) -> Option<Vec<Coord>> {
        self.map.get(goal)?;
        let mut queue = VecDeque::from([start]);
        let mut previous = BTreeMap::from([(start, start)]);
        while let Some(p) = queue.pop_front() {
            if p == goal {
                let mut path = vec![p];
                let mut cursor = p;
                while cursor != start {
                    cursor = previous[&cursor];
                    path.push(cursor);
                }
                path.reverse();
                return Some(path);
            }
            for neighbor in self.map.neighbors(p) {
                if previous.contains_key(&neighbor) {
                    continue;
                }
                if (self.map.get(neighbor)?.terrain == Terrain::Water) != naval {
                    continue;
                }
                previous.insert(neighbor, p);
                queue.push_back(neighbor);
            }
        }
        None
    }
    fn move_army(&mut self, player: Id, id: Id, destination: Coord, rules: &Rules) -> Result<()> {
        let a = self.armies.get(&id).ok_or_else(|| error("Unknown army"))?;
        if a.owner != player {
            return Err(error("You do not command this army"));
        }
        if self.in_battle(id) {
            return Err(error("Army is engaged in battle"));
        }
        if a.position == destination {
            return Err(error("Army is already there"));
        }
        let naval = rules.units[&a.regiments[0].kind].naval;
        let path = self
            .path(a.position, destination, naval)
            .ok_or_else(|| error("No passable route"))?;
        let mut spent = 0;
        let mut target = a.position;
        let mut enemy = None;
        for pos in path.into_iter().skip(1) {
            let tile = self.map.get(pos).unwrap();
            let cost = if tile.mountain { 2 } else { 1 };
            if spent + cost > a.movement {
                break;
            }
            let defender = self
                .armies
                .values()
                .find(|b| b.owner != player && b.position == pos);
            if defender.is_some_and(|b| self.in_battle(b.id)) {
                break;
            }
            spent += cost;
            target = pos;
            if let Some(b) = defender {
                enemy = Some(b.id);
                break;
            }
        }
        if spent == 0 {
            return Err(error("Not enough movement"));
        }
        let a = self.armies.get_mut(&id).unwrap();
        a.position = target;
        a.movement -= spent;
        if let Some(defender) = enemy {
            self.battles.push(Battle {
                attacker: id,
                defender,
                position: target,
                day: 0,
                phase: "Fire".into(),
                attacker_losses: 0,
                defender_losses: 0,
            });
            self.log
                .push("Battle joined. Fire and shock phases resolve as days advance.".into());
        } else {
            self.capture(player, target);
        }
        Ok(())
    }
    fn capture(&mut self, owner: Id, pos: Coord) {
        for city in self
            .cities
            .values_mut()
            .filter(|c| c.position == pos && c.owner != owner)
        {
            city.owner = owner;
            city.production = None;
            city.progress = 0;
            self.log.push(format!("{} has been occupied.", city.name));
        }
    }
    fn advance(&mut self, rules: &Rules, tick: TickRules) {
        self.turn += 1;
        let mut completed = Vec::new();
        for city in self.cities.values_mut() {
            let faction = self.factions.get_mut(&city.owner).unwrap();
            faction.gold += (city.population * 3 + city.industry) * tick.income_percent / 100;
            faction.research += city.population + city.industry / 2;
            faction.industry += city.industry;
            if self.turn.is_multiple_of(8) {
                city.population += 1;
            }
            if let Some(kind) = &city.production {
                city.progress += city.industry * 3;
                if city.progress >= rules.units[kind].cost {
                    completed.push((city.owner, city.position, kind.clone()));
                    city.progress = 0;
                    city.production = None;
                }
            }
        }
        for faction in self.factions.values_mut() {
            let cost = rules.research_cost * (faction.technology as i32 + 1);
            if faction.research >= cost {
                faction.research -= cost;
                faction.technology += 1;
                self.log.push(format!(
                    "{} unlocked technology {}.",
                    faction.name, faction.technology
                ));
            }
        }
        for (owner, mut pos, kind) in completed {
            if rules.units[&kind].naval {
                pos = self
                    .map
                    .neighbors(pos)
                    .into_iter()
                    .find(|p| self.map.get(*p).unwrap().terrain == Terrain::Water)
                    .unwrap();
            }
            self.add_army(owner, pos, vec![kind], rules);
        }
        self.resolve_battles(rules, tick);
        let engaged: BTreeSet<_> = self
            .battles
            .iter()
            .flat_map(|b| [b.attacker, b.defender])
            .collect();
        for a in self.armies.values_mut() {
            a.movement = a
                .regiments
                .iter()
                .map(|r| rules.units[&r.kind].speed)
                .min()
                .unwrap_or(0);
            if !engaged.contains(&a.id) {
                for r in &mut a.regiments {
                    r.morale = (r.morale + 5).min(100);
                    r.strength = (r.strength + 15).min(1000);
                }
            }
        }
        self.ai(rules);
        for faction in self.factions.values() {
            let other_has_city = self.cities.values().any(|c| c.owner != faction.id);
            if faction.industry >= rules.victory_industry || !other_has_city {
                self.winner = Some(faction.id);
                self.log
                    .push(format!("{} wins the campaign!", faction.name));
                break;
            }
        }
    }
    fn resolve_battles(&mut self, rules: &Rules, tick: TickRules) {
        let mut remaining = Vec::new();
        for mut battle in std::mem::take(&mut self.battles) {
            battle.day += 1;
            let fire = ((battle.day - 1) / 3).is_multiple_of(2);
            battle.phase = if fire { "Fire" } else { "Shock" }.into();
            let attack = self.armies[&battle.attacker].clone();
            let defense = self.armies[&battle.defender].clone();
            let roll_a = self.random(10);
            let roll_d = self.random(10);
            let power = |army: &Army, roll: i32| -> i32 {
                let tech = self.factions[&army.owner].technology as i32;
                army.regiments
                    .iter()
                    .take(rules.combat_width)
                    .map(|r| {
                        let unit = &rules.units[&r.kind];
                        let stat = if fire {
                            unit.fire * tick.fire_percent
                        } else {
                            unit.shock * tick.shock_percent
                        };
                        (stat * (roll + army.general + tech + 2) * r.strength / 1000 / 100).max(1)
                    })
                    .sum::<i32>()
                    .max(1)
                    * 4
            };
            let terrain_defense = if self.map.get(battle.position).unwrap().mountain {
                140
            } else {
                100
            };
            let losses_d = power(&attack, roll_a) * 100 / terrain_defense;
            let losses_a = power(&defense, roll_d);
            let damage = |army: &mut Army, total: i32| {
                let count = army.regiments.len().max(1) as i32;
                for r in &mut army.regiments {
                    r.strength = (r.strength - total / count).max(0);
                    r.morale = (r.morale - 4 - total / count / 12).max(0);
                }
                army.regiments.retain(|r| r.strength > 0);
            };
            damage(self.armies.get_mut(&battle.attacker).unwrap(), losses_a);
            damage(self.armies.get_mut(&battle.defender).unwrap(), losses_d);
            battle.attacker_losses += losses_a;
            battle.defender_losses += losses_d;
            let a = &self.armies[&battle.attacker];
            let d = &self.armies[&battle.defender];
            let loser = if a.morale() <= 15 || a.strength() < 100 {
                Some(battle.attacker)
            } else if d.morale() <= 15 || d.strength() < 100 {
                Some(battle.defender)
            } else {
                None
            };
            if let Some(loser) = loser {
                let winner = if loser == battle.attacker {
                    battle.defender
                } else {
                    battle.attacker
                };
                let defeated = self.armies.remove(&loser).unwrap();
                let naval = defeated
                    .regiments
                    .first()
                    .is_some_and(|r| rules.units[&r.kind].naval);
                // Rout away from hostile cities/armies. No valid retreat means surrender.
                let retreat = self.map.neighbors(battle.position).into_iter().find(|p| {
                    (self.map.get(*p).unwrap().terrain == Terrain::Water) == naval
                        && !self
                            .armies
                            .values()
                            .any(|a| a.owner != defeated.owner && a.position == *p)
                        && !self
                            .cities
                            .values()
                            .any(|c| c.owner != defeated.owner && c.position == *p)
                });
                if defeated.strength() >= 100
                    && let Some(pos) = retreat
                {
                    let mut army = defeated;
                    army.position = pos;
                    army.movement = 0;
                    for r in &mut army.regiments {
                        r.morale = 25;
                    }
                    self.armies.insert(loser, army);
                }
                let owner = self.armies[&winner].owner;
                self.capture(owner, battle.position);
                self.log.push(format!(
                    "Battle ended after {} days. {} holds the field.",
                    battle.day, self.factions[&owner].name
                ));
            } else {
                remaining.push(battle);
            }
        }
        self.battles = remaining;
    }
    fn ai(&mut self, rules: &Rules) {
        let ids: Vec<_> = self
            .armies
            .values()
            .filter(|a| a.owner == 2 && !self.in_battle(a.id))
            .map(|a| a.id)
            .collect();
        for id in ids {
            let army = self.armies[&id].clone();
            if army.regiments.iter().any(|r| rules.units[&r.kind].settler) {
                let site = self
                    .map
                    .tiles
                    .iter()
                    .filter(|t| {
                        t.terrain != Terrain::Water
                            && self
                                .cities
                                .values()
                                .all(|c| c.position.distance(t.position) >= 3)
                    })
                    .min_by_key(|t| t.position.distance(army.position))
                    .map(|t| t.position);
                if let Some(site) = site {
                    if army.position == site {
                        let _ = self.apply(
                            2,
                            Command::FoundCity {
                                army: id,
                                name: format!("League Port {}", self.next_id),
                            },
                            rules,
                            TickRules::default(),
                        );
                    } else {
                        let _ = self.move_army(2, id, site, rules);
                    }
                }
            } else if army.morale() > 40 {
                let naval = rules.units[&army.regiments[0].kind].naval;
                let target = if naval {
                    self.armies
                        .values()
                        .filter(|a| a.owner == 1 && rules.units[&a.regiments[0].kind].naval)
                        .min_by_key(|a| a.position.distance(army.position))
                        .map(|a| a.position)
                } else {
                    self.cities
                        .values()
                        .filter(|c| c.owner == 1)
                        .min_by_key(|c| c.position.distance(army.position))
                        .map(|c| c.position)
                };
                if let Some(target) = target {
                    let _ = self.move_army(2, id, target, rules);
                }
            }
        }
        let infantry = rules
            .units
            .values()
            .find(|u| !u.naval && !u.settler)
            .map(|u| u.id.clone());
        if let Some(kind) = infantry {
            for c in self
                .cities
                .values_mut()
                .filter(|c| c.owner == 2 && c.production.is_none())
            {
                c.production = Some(kind.clone());
            }
        }
    }
    fn reveal(&mut self) {
        for owner in self.factions.keys().copied().collect::<Vec<_>>() {
            let sources: Vec<_> = self
                .cities
                .values()
                .filter(|c| c.owner == owner)
                .map(|c| c.position)
                .chain(
                    self.armies
                        .values()
                        .filter(|a| a.owner == owner)
                        .map(|a| a.position),
                )
                .collect();
            let explored = &mut self.factions.get_mut(&owner).unwrap().explored;
            for t in &self.map.tiles {
                if sources.iter().any(|p| p.distance(t.position) <= 5) {
                    explored.insert(t.position);
                }
            }
        }
    }
    /// A player sees only explored terrain and enemies currently near their units/cities.
    pub fn view(&self, player: Id) -> Self {
        if player == 0 {
            return self.clone();
        }
        let mut view = self.clone();
        let sources: Vec<_> = self
            .armies
            .values()
            .filter(|a| a.owner == player)
            .map(|a| a.position)
            .chain(
                self.cities
                    .values()
                    .filter(|c| c.owner == player)
                    .map(|c| c.position),
            )
            .collect();
        let visible = |pos: Coord| sources.iter().any(|p| p.distance(pos) <= 5);
        view.armies
            .retain(|_, a| a.owner == player || visible(a.position));
        view.cities.retain(|_, c| {
            c.owner == player || self.factions[&player].explored.contains(&c.position)
        });
        view.battles.retain(|b| {
            view.armies.contains_key(&b.attacker) && view.armies.contains_key(&b.defender)
        });
        for tile in &mut view.map.tiles {
            if !self.factions[&player].explored.contains(&tile.position) {
                tile.terrain = Terrain::Water;
                tile.forest = false;
                tile.mountain = false;
            }
        }
        for f in view.factions.values_mut().filter(|f| f.id != player) {
            f.gold = 0;
            f.research = 0;
            f.explored.clear();
        }
        view.rng = 0;
        view
    }
}
