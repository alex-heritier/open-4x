//! Stack orders and interleaved Civ3 duels: every active lane advances once per round.
use crate::{
    Battle, Fighter, Game, Id, Outcome, Result, Rules, error,
    terrain::{Coord, Cover},
    units::MOVE_UNIT,
};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StackBattle {
    pub owner: Id,
    pub enemy: Id,
    pub target: Coord,
    pub width: usize,
    pub lanes: Vec<Lane>,
    pub rounds: u32,
    pub attacker_losses: usize,
    pub defender_losses: usize,
    pub advanced: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Lane {
    pub lane: usize,
    pub start: u32,
    pub attacker: Fighter,
    pub defender: Fighter,
    pub rounds: Vec<bool>,
    pub outcome: Outcome,
    #[serde(default)]
    pub support: Option<crate::Support>,
}
struct Active {
    lane: usize,
    start: u32,
    attacker: Fighter,
    defender: Fighter,
    rounds: Vec<bool>,
    support: Option<crate::Support>,
}
impl Game {
    pub(crate) fn move_stack(
        &mut self,
        player: Id,
        id: Id,
        to: Coord,
        rules: &Rules,
    ) -> Result<()> {
        let first = self.own_ashore(player, id)?.clone();
        let domain = rules.def(&first).domain;
        let ids: Vec<_> = self
            .units_at(first.position)
            .into_iter()
            .filter(|u| {
                u.owner == player
                    && u.carrier.is_none()
                    && rules.def(u).domain == domain
                    && u.moves_left(rules.def(u)) > 0
            })
            .map(|u| u.id)
            .collect();
        if ids.is_empty() {
            return Err(error("No movement left"));
        }
        self.move_unit(player, id, to, rules, usize::MAX)?;
        for other in ids.into_iter().filter(|u| *u != id) {
            self.move_unit(player, other, to, rules, usize::MAX)?;
        }
        Ok(())
    }
    pub(crate) fn attack_stack(
        &mut self,
        player: Id,
        id: Id,
        target: Coord,
        rules: &Rules,
    ) -> Result<()> {
        let first = self.own_ashore(player, id)?.clone();
        let domain = rules.def(&first).domain;
        let mut attackers: Vec<_> = self
            .units_at(first.position)
            .into_iter()
            .filter(|u| {
                u.owner == player
                    && u.carrier.is_none()
                    && rules.def(u).domain == domain
                    && rules.def(u).can_attack()
                    && u.moves_left(rules.def(u)) > 0
                    && !u.attacked
            })
            .map(|u| u.id)
            .collect();
        if attackers.is_empty() {
            return Err(error("No fresh attackers in this stack"));
        }
        let holder = self.check_target(player, attackers[0], target, rules, 1, false)?;
        if !self.at_war(player, holder) {
            return Err(error("Declare war before attacking"));
        }
        attackers.sort_by_key(|id| (std::cmp::Reverse(rules.def(&self.units[id]).attack), *id));
        let mut defenders: Vec<_> = self
            .units_at(target)
            .into_iter()
            .filter(|u| {
                u.owner == holder
                    && u.carrier.is_none()
                    && rules.def(u).domain == domain
                    && rules.def(u).defense > 0
            })
            .map(|u| u.id)
            .collect();
        defenders.sort_by_key(|id| (std::cmp::Reverse(rules.def(&self.units[id]).defense), *id));
        if domain == crate::Domain::Sea && self.map.get(target).is_some_and(|t| t.is_land()) {
            return Err(error("Ships cannot capture land"));
        }
        let width = attackers.len().min(defenders.len()).max(1).min(
            if self
                .map
                .get(target)
                .is_some_and(|t| t.is_mountain() || t.cover == Cover::Jungle)
            {
                3
            } else {
                usize::MAX
            },
        );
        let survivors = attackers.clone();
        let mut a: VecDeque<_> = attackers.into();
        let mut d: VecDeque<_> = defenders.into();
        let mut active: Vec<Option<Active>> = (0..width).map(|_| None).collect();
        let mut summary = StackBattle {
            owner: player,
            enemy: holder,
            target,
            width,
            lanes: Vec::new(),
            rounds: 0,
            attacker_losses: 0,
            defender_losses: 0,
            advanced: false,
        };
        loop {
            for (lane, slot) in active.iter_mut().enumerate() {
                if slot.is_some() || a.is_empty() || d.is_empty() {
                    continue;
                }
                let ai = a.pop_front().unwrap();
                let di = d.pop_front().unwrap();
                let attacker = self.fighter(ai, rules).unwrap();
                let defender = self.fighter(di, rules).unwrap();
                let support = self.defensive_bombard(ai, di, rules);
                let u = self.units.get_mut(&ai).unwrap();
                u.attacked = true;
                u.moves_used += MOVE_UNIT.min(u.moves_left(rules.def(u)));
                u.clear_orders();
                *slot = Some(Active {
                    lane,
                    start: summary.rounds,
                    attacker,
                    defender,
                    rounds: Vec::new(),
                    support,
                });
            }
            if active.iter().all(Option::is_none) {
                break;
            }
            summary.rounds += 1;
            for slot in &mut active {
                let Some(clash) = slot else {
                    continue;
                };
                let (ai, di) = (clash.attacker.id, clash.defender.id);
                let au = &self.units[&ai];
                let du = &self.units[&di];
                let odds = crate::combat::round_odds(
                    rules.def(au).attack,
                    rules.def(du).defense,
                    self.defender_percent(du, rules.def(du)),
                );
                let hit = self.random(1024) >= odds;
                clash.rounds.push(hit);
                let loser = if hit { di } else { ai };
                self.units.get_mut(&loser).unwrap().damage += 1;
                if self.units[&loser].hp(rules.def(&self.units[&loser])) > 0 {
                    continue;
                }
                let clash = slot.take().unwrap();
                let outcome = if hit {
                    summary.defender_losses += 1;
                    Outcome::AttackerWon
                } else {
                    summary.attacker_losses += 1;
                    d.push_front(di);
                    Outcome::DefenderWon
                };
                let owner = self.units[&loser].owner;
                self.war_loss(owner, if hit { player } else { holder }, 2);
                self.remove_unit(loser);
                let promoted = self.promote(if hit { ai } else { di });
                self.record(Battle::Duel {
                    attacker: clash.attacker.clone(),
                    defender: clash.defender.clone(),
                    support: clash.support.clone(),
                    rounds: clash.rounds.clone(),
                    outcome,
                    retreat_to: None,
                    promoted,
                });
                summary.lanes.push(Lane {
                    lane: clash.lane,
                    start: clash.start,
                    attacker: clash.attacker,
                    defender: clash.defender,
                    rounds: clash.rounds,
                    outcome,
                    support: clash.support,
                });
            }
        }
        if self.best_defender(target, holder, rules).is_none() {
            if let Some(&lead) = survivors.iter().find(|id| self.units.contains_key(id)) {
                self.capture_tile(player, lead, target, holder, rules);
                for id in survivors
                    .iter()
                    .filter(|id| self.units.contains_key(id))
                    .copied()
                    .collect::<Vec<_>>()
                {
                    self.set_position(id, target);
                }
                summary.advanced = true;
            }
        }
        self.note(format!(
            "Stack battle: {} attackers lost; {} enemy units lost. {}",
            summary.attacker_losses,
            summary.defender_losses,
            if summary.advanced {
                "The stack advanced."
            } else {
                "The defenders held."
            }
        ));
        self.stack_battles.push(summary);
        Ok(())
    }
}
