//! Physical industrial goods, colonial cargo, fixed-price trade and national budgets.
use crate::{Domain, Game, Id, Result, Rules, UnitDef, borders::reach_percent, error};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Good {
    Coal,
    Iron,
    Food,
    Cotton,
    Steel,
    Supplies,
}
impl Good {
    pub const ALL: [Self; 6] = [
        Self::Coal,
        Self::Iron,
        Self::Food,
        Self::Cotton,
        Self::Steel,
        Self::Supplies,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Coal => "Coal",
            Self::Iron => "Iron",
            Self::Food => "Food",
            Self::Cotton => "Cotton",
            Self::Steel => "Steel",
            Self::Supplies => "Supplies",
        }
    }
    pub fn resource(id: &str) -> Option<Self> {
        match id {
            "coal" => Some(Self::Coal),
            "iron" => Some(Self::Iron),
            "wheat" | "corn" | "rice" | "livestock" => Some(Self::Food),
            "cotton" => Some(Self::Cotton),
            _ => None,
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Stockpile(pub BTreeMap<Good, i32>);
impl Stockpile {
    pub fn get(&self, good: Good) -> i32 {
        self.0.get(&good).copied().unwrap_or(0)
    }
    pub fn add(&mut self, good: Good, amount: i32) {
        *self.0.entry(good).or_default() += amount;
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Building {
    Factory,
    Port,
}
impl Building {
    pub fn name(self) -> &'static str {
        match self {
            Self::Factory => "Factory",
            Self::Port => "Port",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EconomicRules {
    pub enabled: bool,
    pub resource_period: u32,
    pub factory_capacity: i32,
    pub factory_cost: i32,
    pub port_cost: i32,
    pub unit_upkeep: i32,
    pub building_upkeep: i32,
    pub loan_principal: i32,
    pub interest_percent: i32,
    pub max_loans: i32,
    pub prices: Stockpile,
    pub market_spread: i32,
    pub starting_stockpile: Stockpile,
    pub unit_goods: i32,
    pub sale_percent: i32,
    pub demand_cooldown: u32,
    pub peace_quiet_turns: u32,
    pub war_chance_per_thousand: u32,
}
impl Default for EconomicRules {
    fn default() -> Self {
        Self {
            enabled: false,
            resource_period: 4,
            factory_capacity: 2,
            factory_cost: 100,
            port_cost: 60,
            unit_upkeep: 1,
            building_upkeep: 2,
            loan_principal: 200,
            interest_percent: 2,
            max_loans: 3,
            prices: Stockpile(
                [
                    (Good::Coal, 8),
                    (Good::Iron, 8),
                    (Good::Food, 4),
                    (Good::Cotton, 6),
                    (Good::Steel, 24),
                    (Good::Supplies, 16),
                ]
                .into(),
            ),
            market_spread: 25,
            starting_stockpile: Stockpile([(Good::Steel, 8), (Good::Supplies, 12)].into()),
            unit_goods: 1,
            sale_percent: 50,
            demand_cooldown: 12,
            peace_quiet_turns: 20,
            war_chance_per_thousand: 12,
        }
    }
}
impl EconomicRules {
    pub fn building_cost(&self, b: Building) -> i32 {
        match b {
            Building::Factory => self.factory_cost,
            Building::Port => self.port_cost,
        }
    }
    pub fn price(&self, g: Good, buy: bool) -> i32 {
        let p = self.prices.get(g);
        if buy {
            p
        } else {
            (p * (100 - self.market_spread) / 100).max(1)
        }
    }
    pub fn unit_good(&self, d: &UnitDef) -> Option<Good> {
        if d.domain == Domain::Sea || d.can_bombard() {
            Some(Good::Steel)
        } else if d.can_attack() {
            Some(Good::Supplies)
        } else {
            None
        }
    }
}
impl Game {
    /// Homeland ownership is frozen by scenario data; conquering a colony never makes it home.
    pub fn home_city(&self, id: Id) -> bool {
        let c = &self.cities[&id];
        c.home_owner == c.owner
    }
    pub(crate) fn refund_production(&mut self, id: Id, rules: &Rules) {
        if !rules.economy.enabled {
            return;
        }
        let c = &self.cities[&id];
        if let Some(g) = c
            .production
            .as_ref()
            .and_then(|k| rules.units.get(k))
            .and_then(|d| rules.economy.unit_good(d))
        {
            self.factions
                .get_mut(&c.owner)
                .unwrap()
                .stockpile
                .add(g, rules.economy.unit_goods);
        }
    }
    pub(crate) fn reserve_goods(&mut self, player: Id, def: &UnitDef, rules: &Rules) -> Result<()> {
        if let Some(g) = rules.economy.unit_good(def) {
            let s = &mut self.factions.get_mut(&player).unwrap().stockpile;
            if s.get(g) < rules.economy.unit_goods {
                return Err(error(
                    "Insufficient goods: buy inputs at the market or run a factory",
                ));
            }
            s.add(g, -rules.economy.unit_goods);
        }
        Ok(())
    }
    pub(crate) fn build(&mut self, player: Id, id: Id, b: Building, rules: &Rules) -> Result<()> {
        if !rules.economy.enabled {
            return Err(error("Industrial economy is disabled in this pack"));
        }
        let c = self.own_city(player, id)?;
        if c.buildings.contains(&b) {
            return Err(error("This building already exists"));
        }
        if b == Building::Port && !self.coastal(c.position) {
            return Err(error("Ports require a coastal city"));
        }
        if c.construction == Some(b) {
            return Err(error("Already under construction"));
        }
        self.refund_production(id, rules);
        let c = self.cities.get_mut(&id).unwrap();
        c.production = None;
        c.progress = 0;
        c.construction = Some(b);
        c.construction_progress = 0;
        Ok(())
    }
    pub(crate) fn trade(&mut self, player: Id, g: Good, amount: i32, rules: &Rules) -> Result<()> {
        if !rules.economy.enabled || amount == 0 || amount.unsigned_abs() > 1000 {
            return Err(error("Trade between 1 and 1000 goods"));
        }
        let p = rules.economy.price(g, amount > 0);
        let f = self
            .factions
            .get_mut(&player)
            .ok_or_else(|| error("Unknown nation"))?;
        if amount > 0 && f.gold < p * amount {
            return Err(error("Not enough gold"));
        }
        if amount < 0 && f.stockpile.get(g) < -amount {
            return Err(error("Not enough goods to sell"));
        }
        f.gold -= p * amount;
        f.stockpile.add(g, amount);
        if player == self.commander {
            self.note(format!(
                "{} {} {} for {} gold.",
                if amount > 0 { "Bought" } else { "Sold" },
                amount.abs(),
                g.name(),
                p * amount.abs()
            ));
        }
        Ok(())
    }
    pub(crate) fn borrow(&mut self, player: Id, rules: &Rules) -> Result<()> {
        let r = &rules.economy;
        if !r.enabled {
            return Err(error("Loans are disabled"));
        }
        let f = self
            .factions
            .get_mut(&player)
            .ok_or_else(|| error("Unknown nation"))?;
        if f.debt >= r.loan_principal * r.max_loans {
            return Err(error("Credit limit reached"));
        }
        f.debt += r.loan_principal;
        f.gold += r.loan_principal;
        self.note(format!(
            "{} borrowed {} gold.",
            self.factions[&player].name, r.loan_principal
        ));
        Ok(())
    }
    pub(crate) fn repay(&mut self, player: Id, rules: &Rules) -> Result<()> {
        let f = self
            .factions
            .get_mut(&player)
            .ok_or_else(|| error("Unknown nation"))?;
        let amount = f.debt.min(rules.economy.loan_principal);
        if amount <= 0 || f.gold < amount {
            return Err(error("Cannot repay a loan"));
        }
        f.debt -= amount;
        f.gold -= amount;
        self.note(format!(
            "{} repaid {} gold.",
            self.factions[&player].name, amount
        ));
        Ok(())
    }
    pub(crate) fn pay(&mut self, player: Id, nation: Id, amount: i32) -> Result<()> {
        if player == nation
            || !self.factions.contains_key(&nation)
            || amount <= 0
            || self.factions[&player].gold < amount
        {
            return Err(error("Invalid payment or insufficient gold"));
        }
        self.factions.get_mut(&player).unwrap().gold -= amount;
        self.factions.get_mut(&nation).unwrap().gold += amount;
        self.note(format!(
            "{} paid {} {} gold.",
            self.factions[&player].name, self.factions[&nation].name, amount
        ));
        Ok(())
    }
    pub fn budget_upkeep(&self, owner: Id, rules: &Rules) -> i32 {
        let r = &rules.economy;
        self.units
            .values()
            .filter(|u| u.owner == owner && u.cargo.is_none())
            .count() as i32
            * r.unit_upkeep
            + self
                .cities
                .values()
                .filter(|c| c.owner == owner)
                .map(|c| c.buildings.len() as i32 * r.building_upkeep)
                .sum::<i32>()
            + (self.factions[&owner].debt * r.interest_percent + 99) / 100
    }
    pub(crate) fn industry_turn(&mut self, rules: &Rules) {
        let r = &rules.economy;
        if !r.enabled {
            return;
        }
        let ids: Vec<Id> = self.cities.keys().copied().collect();
        for id in ids {
            let c = &mut self.cities.get_mut(&id).unwrap();
            if let Some(b) = c.construction {
                c.construction_progress += c.shields();
                if c.construction_progress >= r.building_cost(b) {
                    c.buildings.insert(b);
                    c.construction = None;
                    c.construction_progress = 0;
                    let name = c.name.clone();
                    if c.owner == self.commander {
                        self.note(format!("{name} completed a {}.", b.name()));
                    }
                }
            }
        }
        // A resource past its city's border reach produces less often, by the same share
        // the harvest takes from it: twice as far out as the border, every fourth period.
        let day = self.turn.saturating_sub(1);
        let period = r.resource_period.max(1);
        if day.is_multiple_of(period) {
            let inputs: Vec<_> = self
                .map
                .tiles
                .iter()
                .filter_map(|t| {
                    let city = self.cities.get(&t.claim)?;
                    if city.owner != t.owner {
                        return None;
                    }
                    let share = reach_percent(
                        city.border,
                        self.map.distance_squared(city.position, t.position),
                    )
                    .max(1) as u32;
                    if !(day / period).is_multiple_of(100_u32.div_ceil(share)) {
                        return None;
                    }
                    Some((
                        city.id,
                        city.owner,
                        city.position,
                        Good::resource(&self.map.resource(t)?.id)?,
                    ))
                })
                .collect();
            let mut cargo: BTreeMap<(Id, Good), i32> = BTreeMap::new();
            for (city, owner, _, g) in inputs {
                if self.home_city(city) {
                    self.factions.get_mut(&owner).unwrap().stockpile.add(g, 1);
                } else {
                    *cargo.entry((city, g)).or_default() += 1;
                }
            }
            for ((city, g), amount) in cargo {
                if !rules.units.contains_key("crate") {
                    continue;
                }
                let c = &self.cities[&city];
                let (owner, pos) = (c.owner, c.position);
                let existing = self
                    .units_at(pos)
                    .into_iter()
                    .find(|u| {
                        u.owner == owner
                            && u.carrier.is_none()
                            && u.cargo.is_some_and(|(good, _)| good == g)
                    })
                    .map(|u| u.id);
                if let Some(id) = existing {
                    let u = self.units.get_mut(&id).unwrap();
                    let (_, n) = u.cargo.unwrap();
                    u.cargo = Some((g, n + amount));
                } else {
                    let id = self.add_unit(owner, "crate", pos);
                    self.units.get_mut(&id).unwrap().cargo = Some((g, amount));
                }
            }
        }
        for c in self
            .cities
            .values()
            .filter(|c| c.buildings.contains(&Building::Factory))
        {
            let s = &mut self.factions.get_mut(&c.owner).unwrap().stockpile;
            for (a, b, out) in [
                (Good::Coal, Good::Iron, Good::Steel),
                (Good::Food, Good::Cotton, Good::Supplies),
            ] {
                let n = r.factory_capacity.min(s.get(a)).min(s.get(b));
                if n > 0 {
                    s.add(a, -n);
                    s.add(b, -n);
                    s.add(out, n);
                }
            }
        }
        let owners: Vec<_> = self.factions.keys().copied().collect();
        for owner in owners {
            let upkeep = self.budget_upkeep(owner, rules);
            let f = self.factions.get_mut(&owner).unwrap();
            f.upkeep = upkeep;
            f.gold -= upkeep;
            if owner != self.commander && self.factions[&owner].gold < 0 {
                let _ = self.borrow(owner, rules);
            }
            while self.factions[&owner].gold < 0 {
                let building = self
                    .cities
                    .values()
                    .filter(|c| c.owner == owner)
                    .find_map(|c| c.buildings.iter().next().map(|b| (c.id, *b)));
                let (name, value) = if let Some((id, b)) = building {
                    let c = self.cities.get_mut(&id).unwrap();
                    c.buildings.remove(&b);
                    (
                        format!("{} in {}", b.name(), c.name),
                        r.building_cost(b) * r.sale_percent / 100,
                    )
                } else if let Some(id) = self
                    .units
                    .values()
                    .filter(|u| u.owner == owner && u.cargo.is_none())
                    .max_by_key(|u| rules.def(u).cost)
                    .map(|u| u.id)
                {
                    let value = rules.def(&self.units[&id]).cost * r.sale_percent / 100;
                    let name = rules.def(&self.units[&id]).name.clone();
                    self.remove_unit(id);
                    (name, value)
                } else {
                    self.factions.get_mut(&owner).unwrap().gold = 0;
                    self.note(format!(
                        "{} defaulted; its treasury is exhausted.",
                        self.factions[&owner].name
                    ));
                    break;
                };
                self.factions.get_mut(&owner).unwrap().gold += value.max(1);
                self.note(format!(
                    "{} bankruptcy: sold {name} for {} gold.",
                    self.factions[&owner].name,
                    value.max(1)
                ));
            }
        }
    }
    pub(crate) fn deliver_crates(&mut self, rules: &Rules) {
        if !rules.economy.enabled {
            return;
        }
        let ids: Vec<_> = self
            .units
            .values()
            .filter(|u| u.cargo.is_some() && u.carrier.is_none())
            .filter_map(|u| {
                let city = self.city_at(u.position)?;
                (self.cities[&city].owner == u.owner && self.home_city(city)).then_some(u.id)
            })
            .collect();
        for id in ids {
            let u = self.remove_unit(id).unwrap();
            let (g, n) = u.cargo.unwrap();
            self.factions.get_mut(&u.owner).unwrap().stockpile.add(g, n);
            if u.owner == self.commander {
                self.note(format!("Delivered {n} {} to the home stockpile.", g.name()));
            }
        }
    }
    pub(crate) fn ai_logistics(&mut self, rules: &Rules) {
        if !rules.economy.enabled {
            return;
        }
        let cities: Vec<_> = self
            .cities
            .values()
            .filter(|c| {
                c.owner != self.commander && c.production.is_none() && c.construction.is_none()
            })
            .map(|c| c.id)
            .collect();
        for id in cities {
            let c = &self.cities[&id];
            let owner = c.owner;
            let threatened = self.units.values().any(|u| {
                self.at_war(owner, u.owner)
                    && rules.def(u).can_attack()
                    && self.map.distance(c.position, u.position) <= 6
            });
            let guard = self
                .units_at(c.position)
                .iter()
                .filter(|u| {
                    u.owner == owner && u.carrier.is_none() && rules.def(u).domain == Domain::Land
                })
                .map(|u| rules.def(u).defense)
                .sum::<i32>();
            let defender = rules
                .units
                .values()
                .filter(|d| d.domain == Domain::Land)
                .map(|d| d.defense)
                .max()
                .unwrap_or(0);
            if threatened || guard < crate::ai::garrison_goal(c, false) * defender {
                continue;
            }
            let s = &self.factions[&owner].stockpile;
            let factories = self
                .cities
                .values()
                .filter(|c| {
                    c.owner == owner
                        && (c.buildings.contains(&Building::Factory)
                            || c.construction == Some(Building::Factory))
                })
                .count() as i32;
            let available_recipes = s.get(Good::Coal).min(s.get(Good::Iron))
                + s.get(Good::Food).min(s.get(Good::Cotton));
            let factory = !c.buildings.contains(&Building::Factory)
                && (factories == 0
                    || available_recipes > factories * rules.economy.factory_capacity * 4)
                && ((s.get(Good::Coal) > 0 && s.get(Good::Iron) > 0)
                    || (s.get(Good::Food) > 0 && s.get(Good::Cotton) > 0));
            let port = self.coastal(c.position) && !c.buildings.contains(&Building::Port);
            if self.factions[&owner].income
                > self.budget_upkeep(owner, rules) + rules.economy.building_upkeep
            {
                if factory {
                    let _ = self.build(owner, id, Building::Factory, rules);
                } else if port {
                    let _ = self.build(owner, id, Building::Port, rules);
                }
            }
        }
        // Recruit a transport where colonial shipments cannot use a land route.
        let owners: Vec<_> = self
            .factions
            .keys()
            .copied()
            .filter(|id| *id != self.commander)
            .collect();
        for owner in owners {
            let cargo = self
                .units
                .values()
                .any(|u| u.owner == owner && u.cargo.is_some());
            let transports = self
                .units
                .values()
                .filter(|u| u.owner == owner && rules.def(u).is_carrier())
                .count()
                + self
                    .cities
                    .values()
                    .filter(|c| {
                        c.owner == owner
                            && c.production
                                .as_ref()
                                .and_then(|k| rules.units.get(k))
                                .is_some_and(UnitDef::is_carrier)
                    })
                    .count();
            let colonies = self
                .cities
                .values()
                .filter(|c| c.owner == owner && !self.home_city(c.id) && self.coastal(c.position))
                .count();
            let needed = (1 + colonies.div_ceil(4)).clamp(2, 16);
            if !cargo
                || transports >= needed
                || self.income(owner)
                    <= self.budget_upkeep(owner, rules) + rules.economy.unit_upkeep
            {
                continue;
            }
            let city = self
                .cities
                .values()
                .find(|c| {
                    c.owner == owner
                        && self.coastal(c.position)
                        && c.production.is_none()
                        && c.construction.is_none()
                })
                .map(|c| c.id);
            let def = rules
                .units
                .values()
                .filter(|d| d.is_carrier())
                .min_by_key(|d| d.cost);
            if let (Some(city), Some(def)) = (city, def) {
                if let Some(g) = rules.economy.unit_good(def) {
                    if self.factions[&owner].stockpile.get(g) < rules.economy.unit_goods {
                        let _ = self.trade(owner, g, rules.economy.unit_goods, rules);
                    }
                }
                let _ = self.execute(
                    owner,
                    crate::Command::Produce {
                        city,
                        unit: def.id.clone(),
                    },
                    rules,
                    crate::TickRules::default(),
                );
            }
        }
        if !self
            .units
            .values()
            .any(|u| u.owner != self.commander && u.cargo.is_some())
        {
            return;
        }
        // Label land masses once: don't exhaust a path search for every overseas crate.
        let mut land = vec![0u32; self.map.tiles.len()];
        let mut label = 0u32;
        for tile in &self.map.tiles {
            let Some(index) = self.map.index(tile.position) else {
                continue;
            };
            if !tile.is_land() || land[index] != 0 {
                continue;
            }
            label += 1;
            land[index] = label;
            let mut pending = vec![tile.position];
            while let Some(at) = pending.pop() {
                for next in self.map.neighbors(at) {
                    if let Some(i) = self.map.index(next) {
                        if land[i] == 0 && self.map.get(next).is_some_and(|t| t.is_land()) {
                            land[i] = label;
                            pending.push(next);
                        }
                    }
                }
            }
        }
        let component = |p: crate::terrain::Coord| self.map.index(p).map_or(0, |i| land[i]);
        let cities: Vec<_> = self
            .cities
            .values()
            .map(|c| {
                (
                    c.owner,
                    c.position,
                    component(c.position),
                    self.home_city(c.id),
                    c.buildings.contains(&Building::Port),
                )
            })
            .collect();
        let crates: Vec<_> = self
            .units
            .values()
            .filter(|u| u.owner != self.commander && u.cargo.is_some())
            .map(|u| u.id)
            .collect();
        let mut dispatched = std::collections::BTreeSet::new();
        for id in crates {
            let u = self.units[&id].clone();
            let current = self.map.index(u.position).map_or(0, |i| land[i]);
            let home = cities
                .iter()
                .filter(|c| c.0 == u.owner && c.3)
                .min_by_key(|c| (c.2 != current, self.map.distance(u.position, c.1)))
                .map(|c| c.1);
            let Some(goal) = home else {
                continue;
            };
            if let Some(ship) = u.carrier {
                let port = self
                    .cities
                    .values()
                    .filter(|c| {
                        c.owner == u.owner
                            && self.home_city(c.id)
                            && c.buildings.contains(&Building::Port)
                    })
                    .min_by_key(|c| self.map.distance(u.position, c.position))
                    .map(|c| c.position);
                if let Some(port) = port {
                    if u.position == port {
                        let _ = self.unload(u.owner, id, rules);
                    } else if dispatched.insert(ship) {
                        let _ = self.move_unit(u.owner, ship, port, rules, 20_000);
                    }
                }
                continue;
            }
            if self.map.index(goal).is_some_and(|i| land[i] == current)
                && self
                    .path(u.owner, Domain::Land, u.position, goal, 2_000)
                    .is_some()
            {
                let _ = self.move_unit(u.owner, id, goal, rules, 2_000);
                continue;
            }
            // Walk cargo to a reachable colonial port and sail an empty transport there.
            let pickup = cities
                .iter()
                .filter(|c| c.0 == u.owner && c.4 && !c.3 && c.2 == current)
                .min_by_key(|c| self.map.distance(u.position, c.1))
                .map(|c| c.1);
            let Some(pickup) = pickup else {
                continue;
            };
            if u.position != pickup {
                let _ = self.move_unit(u.owner, id, pickup, rules, 2_000);
            }
            let ship = self
                .units
                .values()
                .filter(|ship| {
                    ship.owner == u.owner
                        && rules.def(ship).is_carrier()
                        && self.passengers(ship.id).len() < (rules.def(ship).capacity as usize)
                })
                .filter(|ship| {
                    ship.position == pickup
                        || self
                            .passengers(ship.id)
                            .iter()
                            .all(|id| self.units[id].cargo.is_none())
                })
                .min_by_key(|ship| self.map.distance(ship.position, pickup))
                .map(|ship| (ship.id, ship.position));
            if let Some((ship, pos)) = ship {
                if pos == pickup && self.units[&id].position == pickup {
                    let _ = self.load(u.owner, id, ship, rules);
                } else if dispatched.insert(ship) {
                    let _ = self.move_unit(u.owner, ship, pickup, rules, 20_000);
                }
            }
        }
        // Escort land cargo with a nearby spare defender, never strip a city garrison.
        let cargo: Vec<_> = self
            .units
            .values()
            .filter(|u| u.owner != self.commander && u.cargo.is_some() && u.carrier.is_none())
            .map(|u| (u.owner, u.position))
            .collect();
        for (owner, pos) in cargo {
            let escort = self
                .units
                .values()
                .filter(|u| {
                    u.owner == owner
                        && u.cargo.is_none()
                        && u.carrier.is_none()
                        && rules.def(u).domain == Domain::Land
                        && rules.def(u).defense > 0
                        && self.city_at(u.position).is_none()
                        && self.map.distance(u.position, pos) <= 8
                })
                .min_by_key(|u| self.map.distance(u.position, pos))
                .map(|u| u.id);
            if let Some(id) = escort {
                let _ = self.move_unit(owner, id, pos, rules, 2000);
            }
        }
    }
}

impl EconomicRules {
    pub fn validate(&self) -> std::result::Result<(), String> {
        let bounded = |v: i32| (0..=10_000).contains(&v);
        if !(1..=1000).contains(&self.resource_period)
            || !(1..=100).contains(&self.factory_capacity)
            || !(1..=10_000).contains(&self.factory_cost)
            || !(1..=10_000).contains(&self.port_cost)
            || !bounded(self.unit_upkeep)
            || !bounded(self.building_upkeep)
            || !(1..=100_000).contains(&self.loan_principal)
            || !(0..=100).contains(&self.interest_percent)
            || !(1..=100).contains(&self.max_loans)
            || !(0..=99).contains(&self.market_spread)
            || !(1..=100).contains(&self.unit_goods)
            || !(1..=100).contains(&self.sale_percent)
            || !(1..=1000).contains(&self.demand_cooldown)
            || !(1..=1000).contains(&self.peace_quiet_turns)
            || self.war_chance_per_thousand > 1000
            || Good::ALL.iter().any(|g| {
                !(1..=10_000).contains(&self.prices.get(*g))
                    || !bounded(self.starting_stockpile.get(*g))
            })
        {
            return Err("Industrial economy rules out of range".into());
        }
        Ok(())
    }
}
