//! Persistent war pressure, demands and peace. All rolls use the campaign RNG.
use crate::{Game, Good, Id, Result, Rules, error, war_key};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DiplomaticState {
    pub wars: Vec<WarPressure>,
    pub demands: Vec<Demand>,
    pub relations: Vec<Relation>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WarPressure {
    pub a: Id,
    pub b: Id,
    pub since: u32,
    pub last_capture: u32,
    pub a_weariness: i32,
    pub b_weariness: i32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Demand {
    pub from: Id,
    pub to: Id,
    pub gold: i32,
    pub good: Option<Good>,
    pub turn: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Relation {
    pub a: Id,
    pub b: Id,
    pub last_demand: u32,
    pub truce_until: u32,
    pub refused_by: Option<Id>,
}
impl DiplomaticState {
    pub fn view(&self, player: Id) -> Self {
        Self {
            wars: self.wars.clone(),
            demands: self
                .demands
                .iter()
                .filter(|d| d.to == player || d.from == player)
                .cloned()
                .collect(),
            relations: self.relations.clone(),
        }
    }
    fn relation(&mut self, a: Id, b: Id) -> &mut Relation {
        let (a, b) = war_key(a, b);
        let index = match self.relations.iter().position(|r| r.a == a && r.b == b) {
            Some(i) => i,
            None => {
                self.relations.push(Relation {
                    a,
                    b,
                    last_demand: 0,
                    truce_until: 0,
                    refused_by: None,
                });
                self.relations.len() - 1
            }
        };
        &mut self.relations[index]
    }
}
impl Game {
    pub(crate) fn war_loss(&mut self, owner: Id, enemy: Id, weight: i32) {
        let key = war_key(owner, enemy);
        if let Some(w) = self.diplomacy.wars.iter_mut().find(|w| (w.a, w.b) == key) {
            if owner == w.a {
                w.a_weariness += weight;
            } else {
                w.b_weariness += weight;
            }
        }
    }
    pub(crate) fn war_capture(&mut self, old: Id, new: Id) {
        self.war_loss(old, new, 8);
        let key = war_key(old, new);
        if let Some(w) = self.diplomacy.wars.iter_mut().find(|w| (w.a, w.b) == key) {
            w.last_capture = self.turn;
        }
    }
    fn peace_between(&mut self, a: Id, b: Id, rules: &Rules) {
        let key = war_key(a, b);
        self.wars.remove(&key);
        self.diplomacy.wars.retain(|w| (w.a, w.b) != key);
        let until = self.turn + rules.economy.demand_cooldown;
        let r = self.diplomacy.relation(a, b);
        r.truce_until = until;
        r.refused_by = None;
        self.diplomacy
            .demands
            .retain(|d| war_key(d.from, d.to) != key);
        self.note(format!(
            "{} and {} made peace; each keeps its occupied territory.",
            self.factions[&a].name, self.factions[&b].name
        ));
    }
    pub fn peace_willing(&self, player: Id, nation: Id, rules: &Rules) -> bool {
        let key = war_key(player, nation);
        self.diplomacy
            .wars
            .iter()
            .find(|w| (w.a, w.b) == key)
            .is_some_and(|w| {
                let (ours, theirs) = if player == w.a {
                    (w.a_weariness, w.b_weariness)
                } else {
                    (w.b_weariness, w.a_weariness)
                };
                theirs >= ours + 5
                    || self.turn.saturating_sub(w.last_capture) >= rules.economy.peace_quiet_turns
            })
    }
    pub(crate) fn propose_peace(&mut self, player: Id, nation: Id, rules: &Rules) -> Result<()> {
        if !self.at_war(player, nation) {
            return Err(error("You are not at war"));
        }
        if !self.peace_willing(player, nation, rules) {
            return Err(error("Peace refused: their war weariness is too low"));
        }
        self.peace_between(player, nation, rules);
        Ok(())
    }
    pub(crate) fn answer_demand(&mut self, player: Id, nation: Id, accept: bool) -> Result<()> {
        let index = self
            .diplomacy
            .demands
            .iter()
            .position(|d| d.to == player && d.from == nation)
            .ok_or_else(|| error("No pending demand from this nation"))?;
        let d = self.diplomacy.demands[index].clone();
        if accept {
            if let Some(g) = d.good {
                if self.factions[&player].stockpile.get(g) < 1 {
                    return Err(error("You no longer have the demanded good"));
                }
                self.factions.get_mut(&player).unwrap().stockpile.add(g, -1);
                self.factions.get_mut(&nation).unwrap().stockpile.add(g, 1);
            } else {
                self.pay(player, nation, d.gold)?;
            }
        } else {
            self.diplomacy.relation(player, nation).refused_by = Some(player);
        }
        if accept {
            self.diplomacy.relation(player, nation).refused_by = None;
        }
        self.diplomacy.demands.remove(index);
        self.note(format!(
            "{} {} the demand from {}.",
            self.factions[&player].name,
            if accept { "accepted" } else { "refused" },
            self.factions[&nation].name
        ));
        Ok(())
    }
    pub(crate) fn diplomacy_turn(&mut self, rules: &Rules) {
        if !rules.economy.enabled {
            return;
        }
        // Older saves/scenario wars acquire the same pressure model as declared wars.
        for &(a, b) in &self.wars {
            if !self.diplomacy.wars.iter().any(|w| w.a == a && w.b == b) {
                self.diplomacy.wars.push(WarPressure {
                    a,
                    b,
                    since: self.turn,
                    last_capture: self.turn,
                    a_weariness: 0,
                    b_weariness: 0,
                });
            }
        }
        for w in &mut self.diplomacy.wars {
            w.a_weariness += 1;
            w.b_weariness += 1;
        }
        let wars: Vec<_> = self.wars.iter().copied().collect();
        for (a, b) in wars {
            if a != self.commander
                && b != self.commander
                && (self.peace_willing(a, b, rules) || self.peace_willing(b, a, rules))
            {
                self.peace_between(a, b, rules);
            }
        }
        // Border adjacency, including new frontiers created by occupation.
        let mut neighbors = std::collections::BTreeSet::new();
        for t in &self.map.tiles {
            if t.owner == 0 {
                continue;
            }
            for p in self.map.neighbors(t.position) {
                if let Some(n) = self.map.get(p) {
                    if n.owner != 0 && n.owner != t.owner {
                        neighbors.insert(war_key(t.owner, n.owner));
                    }
                }
            }
        }
        for (a, b) in neighbors {
            if self.at_war(a, b) {
                continue;
            }
            let (last, truce, refused) = {
                let r = self.diplomacy.relation(a, b);
                (r.last_demand, r.truce_until, r.refused_by)
            };
            if self.turn < truce {
                continue;
            }
            // Local strength near the shared frontier, not total global army size.
            let border: Vec<_> = self
                .map
                .tiles
                .iter()
                .filter(|t| {
                    t.owner == a
                        && self
                            .map
                            .neighbors(t.position)
                            .iter()
                            .any(|p| self.map.get(*p).is_some_and(|n| n.owner == b))
                })
                .map(|t| t.position)
                .collect();
            let mut sa = 0;
            let mut sb = 0;
            for u in self
                .units
                .values()
                .filter(|u| u.carrier.is_none() && (u.owner == a || u.owner == b))
            {
                if border
                    .iter()
                    .any(|p| self.map.distance(*p, u.position) <= 8)
                {
                    let d = rules.def(u);
                    let s = d.attack + d.defense + d.bombard / 2;
                    if u.owner == a {
                        sa += s;
                    } else {
                        sb += s;
                    }
                }
            }
            let (strong, weak) = if sa >= sb { (a, b) } else { (b, a) };
            let (s, w) = (sa.max(sb), sa.min(sb));
            let mut declared = false;
            for (attacker, target, ours, theirs) in [(a, b, sa, sb), (b, a, sb, sa)] {
                if attacker == self.commander
                    || ours == 0
                    || rules.economy.war_chance_per_thousand == 0
                {
                    continue;
                }
                let extra = 4
                    + if ours > theirs * 3 / 2 { 8 } else { 0 }
                    + if self.wars.iter().any(|&(x, y)| x == target || y == target) {
                        6
                    } else {
                        0
                    }
                    + if refused == Some(target) { 20 } else { 0 };
                if self.random(1000) < (rules.economy.war_chance_per_thousand + extra) as i32 {
                    self.declare_war(attacker, target);
                    declared = true;
                    break;
                }
            }
            if declared {
                continue;
            }
            if strong == self.commander
                || s <= w * 3 / 2
                || self.turn.saturating_sub(last) < rules.economy.demand_cooldown
                || self
                    .diplomacy
                    .demands
                    .iter()
                    .any(|d| war_key(d.from, d.to) == (a, b))
            {
                continue;
            }
            if self.random(4) != 0 {
                continue;
            }
            let gold = (self.factions[&weak].gold / 10).max(1);
            let good = if self.factions[&weak].gold < 10 {
                Good::ALL
                    .into_iter()
                    .find(|g| self.factions[&weak].stockpile.get(*g) > 0)
            } else {
                None
            };
            if good.is_none() && self.factions[&weak].gold < gold {
                continue;
            }
            self.diplomacy.relation(a, b).last_demand = self.turn;
            self.diplomacy.demands.push(Demand {
                from: strong,
                to: weak,
                gold,
                good,
                turn: self.turn,
            });
            self.note(format!(
                "{} demands {} from {}.",
                self.factions[&strong].name,
                good.map(|g| format!("1 {}", g.name()))
                    .unwrap_or(format!("{gold} gold")),
                self.factions[&weak].name
            ));
            if weak != self.commander {
                let accept = self.random(100) < 70;
                let _ = self.answer_demand(weak, strong, accept);
            }
        }
    }
}
