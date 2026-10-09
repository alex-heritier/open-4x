//! A replayable account of each fight, for the interface to play back.
//!
//! The simulation resolves a fight in one step and records what happened here, in order:
//! who fought, which side won each round, how it ended. Records say nothing about timing; see
//! `docs/combat-animation.md`. Recording only observes: it never draws a die, so a game plays
//! out identically with or without it.
use crate::{Game, Id, Rules, terrain::Coord};
use serde::{Deserialize, Serialize};

pub use crate::combat::Outcome;

/// Records kept from one command. A turn of computer play is one command, and the world
/// scenario can hold hundreds of fights in a day, so only the latest are kept.
pub const BATTLE_LIMIT: usize = 128;

/// A unit as it stood when a clash began.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Fighter {
    pub id: Id,
    pub owner: Id,
    /// Key of the design in the pack's unit table.
    pub kind: String,
    pub position: Coord,
    /// Hit points before the clash, and the most the unit could have had.
    pub hp: i32,
    pub max_hp: i32,
}

/// A defensive shot fired at an attacker before the first round.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Support {
    pub shooter: Fighter,
    pub hit: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Battle {
    /// A melee fight, round by round.
    Duel {
        attacker: Fighter,
        defender: Fighter,
        support: Option<Support>,
        /// One entry per round up to the deciding one; `true` when the attacker won it.
        rounds: Vec<bool>,
        outcome: Outcome,
        /// Where a retreating defender went.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        retreat_to: Option<Coord>,
        /// The winner gained a level.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        promoted: bool,
    },
    /// Nothing defended the square, so what stood there was taken or destroyed.
    Capture {
        attacker: Fighter,
        target: Coord,
        /// Units that changed hands.
        taken: Vec<Fighter>,
        /// Units that could not defend and were lost, such as ships tied up in port.
        destroyed: Vec<Fighter>,
        /// The name of a city that fell.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        city: Option<String>,
        /// The attacker moved onto the square.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        advanced: bool,
    },
    /// A ranged attack, one volley after another.
    Bombard {
        shooter: Fighter,
        target: Fighter,
        /// One entry per volley fired; `true` for a hit.
        shots: Vec<bool>,
        killed: bool,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        promoted: bool,
    },
}

impl Battle {
    /// Every unit named in the record.
    pub fn fighters(&self) -> Vec<&Fighter> {
        match self {
            Battle::Duel {
                attacker,
                defender,
                support,
                ..
            } => [
                Some(attacker),
                Some(defender),
                support.as_ref().map(|s| &s.shooter),
            ]
            .into_iter()
            .flatten()
            .collect(),
            Battle::Capture {
                attacker,
                taken,
                destroyed,
                ..
            } => std::iter::once(attacker)
                .chain(taken)
                .chain(destroyed)
                .collect(),
            Battle::Bombard {
                shooter, target, ..
            } => vec![shooter, target],
        }
    }

    /// Every square the fight touched: where units stood and where one ended up.
    pub fn tiles(&self) -> Vec<Coord> {
        let mut tiles: Vec<Coord> = self.fighters().iter().map(|f| f.position).collect();
        match self {
            Battle::Duel {
                retreat_to: Some(to),
                ..
            } => tiles.push(*to),
            Battle::Capture { target, .. } => tiles.push(*target),
            _ => {}
        }
        tiles.sort_by_key(|p| (p.x, p.y));
        tiles.dedup();
        tiles
    }
}

impl Game {
    /// A unit's state as a record shows it, or `None` if it is gone.
    pub(crate) fn fighter(&self, id: Id, rules: &Rules) -> Option<Fighter> {
        let unit = self.units.get(&id)?;
        let def = rules.def(unit);
        Some(Fighter {
            id,
            owner: unit.owner,
            kind: unit.kind.clone(),
            position: unit.position,
            hp: unit.hp(def),
            max_hp: unit.max_hp(def),
        })
    }

    pub(crate) fn record(&mut self, battle: Battle) {
        self.battles.push(battle);
        let excess = self.battles.len().saturating_sub(BATTLE_LIMIT);
        self.battles.drain(..excess);
    }

    /// Whether a player may know about a fight: it involved one of their units, or they can
    /// see where it happened. `sees` is the same sight rule that decides which units they see.
    pub(crate) fn witnessed(battle: &Battle, player: Id, sees: impl Fn(Coord) -> bool) -> bool {
        battle.fighters().iter().any(|f| f.owner == player) || battle.tiles().into_iter().any(sees)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{arena, rules, spawn};
    use crate::{Command, TickRules};

    const RED: Id = 1;
    const BLUE: Id = 2;
    const GREEN: Id = 3;

    fn seeded(mut game: Game, seed: u64) -> Game {
        game.rng = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        game
    }

    fn run(game: &mut Game, player: Id, command: Command) {
        game.apply(player, command, &rules(), TickRules::default())
            .expect("the command applies");
    }

    fn attack(game: &mut Game, player: Id, unit: Id, x: i32, y: i32) {
        let target = Coord::new(x, y);
        run(game, player, Command::Attack { unit, target });
    }

    fn only(game: &Game) -> Battle {
        assert_eq!(game.battles.len(), 1, "{:?}", game.battles);
        game.battles[0].clone()
    }

    /// Checks a recorded duel against what the game says happened, and returns how it ended.
    fn verify_duel(game: &Game, a: Id, d: Id) -> Outcome {
        let Battle::Duel {
            attacker,
            defender,
            support,
            rounds,
            outcome,
            retreat_to,
            promoted,
        } = only(game)
        else {
            panic!("expected a duel");
        };
        assert_eq!((attacker.id, defender.id), (a, d));
        let won = rounds.iter().filter(|&&r| r).count() as i32;
        let lost = rounds.len() as i32 - won;
        let shot = i32::from(support.as_ref().is_some_and(|s| s.hit));
        let (a_left, d_left) = (attacker.hp - lost - shot, defender.hp - won);
        let last = *rounds.last().expect("a duel has at least one round");
        // Only the last round decides, and every earlier one left both sides standing.
        match outcome {
            Outcome::AttackerWon => assert!(last && d_left <= 0 && a_left > 0),
            Outcome::DefenderWon => assert!(!last && a_left <= 0 && d_left > 0),
            Outcome::DefenderRetreated => assert!(last && d_left == 1 && a_left > 1),
            Outcome::AttackerRetreated => assert!(!last && a_left == 1 && d_left > 1),
        }
        // What the record adds up to is what the game holds afterwards.
        if outcome == Outcome::DefenderWon {
            assert!(!game.units.contains_key(&a));
        } else {
            let hurt = attacker.max_hp - attacker.hp + lost + shot;
            assert_eq!(game.units[&a].damage, hurt);
        }
        if outcome == Outcome::AttackerWon {
            assert!(!game.units.contains_key(&d));
        } else {
            let hurt = defender.max_hp - defender.hp + won;
            assert_eq!(game.units[&d].damage, hurt);
        }
        // Only a withdrawal moves anyone, and only the withdrawing side names where to.
        assert_eq!(
            retreat_to,
            (outcome == Outcome::DefenderRetreated).then(|| game.units[&d].position)
        );
        assert!(!promoted || matches!(outcome, Outcome::AttackerWon | Outcome::DefenderWon));
        outcome
    }

    #[test]
    fn a_duel_is_recorded_round_by_round() {
        let mut seen = std::collections::BTreeSet::new();
        for seed in 1..=500u64 {
            // Cavalry (fast) against militia (slow): only the attacker may withdraw.
            let mut game = seeded(arena(10, 10), seed);
            let a = spawn(&mut game, RED, "cavalry", 4, 4);
            let d = spawn(&mut game, BLUE, "militia", 5, 5);
            game.units.get_mut(&a).unwrap().level = (seed % 4) as u8;
            game.units.get_mut(&d).unwrap().level = 3;
            attack(&mut game, RED, a, 5, 5);
            let outcome = verify_duel(&game, a, d);
            assert_ne!(outcome, Outcome::DefenderRetreated);
            seen.insert(format!("{outcome:?}"));
            // Militia (slow) against infantry (fast): only the defender may withdraw.
            let mut game = seeded(arena(10, 10), seed);
            let a = spawn(&mut game, RED, "militia", 4, 4);
            let d = spawn(&mut game, BLUE, "infantry", 5, 5);
            game.units.get_mut(&a).unwrap().level = 3;
            attack(&mut game, RED, a, 5, 5);
            let outcome = verify_duel(&game, a, d);
            assert_ne!(outcome, Outcome::AttackerRetreated);
            if outcome == Outcome::DefenderRetreated {
                let Battle::Duel { retreat_to, .. } = only(&game) else {
                    unreachable!()
                };
                assert_eq!(retreat_to, Some(Coord::new(6, 6)), "straight away");
            }
            seen.insert(format!("{outcome:?}"));
        }
        assert_eq!(seen.len(), 4, "every ending turns up: {seen:?}");
    }

    #[test]
    fn a_support_shot_is_recorded_with_its_result() {
        let (mut hit, mut missed) = (0, 0);
        for seed in 1..=300u64 {
            let mut game = seeded(arena(10, 10), seed);
            let a = spawn(&mut game, RED, "cavalry", 4, 4);
            let d = spawn(&mut game, BLUE, "militia", 5, 5);
            let gun = spawn(&mut game, BLUE, "artillery", 5, 5);
            game.units.get_mut(&d).unwrap().level = 3;
            attack(&mut game, RED, a, 5, 5);
            // The duel check sums the shot into the attacker's damage.
            verify_duel(&game, a, d);
            let Battle::Duel { support, .. } = only(&game) else {
                unreachable!()
            };
            let support = support.expect("the artillery fired");
            assert_eq!((support.shooter.id, support.shooter.owner), (gun, BLUE));
            assert_eq!(support.shooter.kind, "artillery");
            assert!(game.units[&gun].fired);
            if support.hit {
                hit += 1;
            } else {
                missed += 1;
            }
        }
        assert!(hit > 20 && missed > 20, "{hit} hits, {missed} misses");
    }

    #[test]
    fn no_support_shot_without_a_gun_on_the_square() {
        let mut game = seeded(arena(10, 10), 5);
        let a = spawn(&mut game, RED, "cavalry", 4, 4);
        let d = spawn(&mut game, BLUE, "militia", 5, 5);
        // Artillery that is not with the defender does not help it.
        spawn(&mut game, BLUE, "artillery", 6, 6);
        attack(&mut game, RED, a, 5, 5);
        verify_duel(&game, a, d);
        let Battle::Duel { support, .. } = only(&game) else {
            unreachable!()
        };
        assert!(support.is_none());
    }

    #[test]
    fn taking_an_undefended_square_is_recorded() {
        let mut game = arena(10, 10);
        let a = spawn(&mut game, RED, "infantry", 4, 4);
        let w = spawn(&mut game, BLUE, "worker", 5, 5);
        attack(&mut game, RED, a, 5, 5);
        let Battle::Capture {
            attacker,
            target,
            taken,
            destroyed,
            city,
            advanced,
        } = only(&game)
        else {
            panic!("expected a capture");
        };
        assert_eq!((attacker.id, target), (a, Coord::new(5, 5)));
        // The record shows the worker as it was taken: still Blue's.
        assert_eq!(taken.len(), 1);
        assert_eq!((taken[0].id, taken[0].owner), (w, BLUE));
        assert!(destroyed.is_empty() && city.is_none() && !advanced);
        assert_eq!(game.units[&w].owner, RED);
    }

    #[test]
    fn a_fallen_city_and_the_ship_in_its_port_are_recorded() {
        let mut game = arena(10, 10);
        let a = spawn(&mut game, RED, "infantry", 7, 7);
        let ship = spawn(&mut game, BLUE, "ironclad", 8, 8);
        let worker = spawn(&mut game, BLUE, "worker", 8, 8);
        attack(&mut game, RED, a, 8, 8);
        let Battle::Capture {
            taken,
            destroyed,
            city,
            advanced,
            ..
        } = only(&game)
        else {
            panic!("expected a capture");
        };
        assert_eq!(city.as_deref(), Some("Blueton"));
        assert!(advanced);
        assert_eq!(destroyed.iter().map(|f| f.id).collect::<Vec<_>>(), [ship]);
        assert_eq!(taken.iter().map(|f| f.id).collect::<Vec<_>>(), [worker]);
        assert!(!game.units.contains_key(&ship));
        assert_eq!(game.units[&a].position, Coord::new(8, 8));
    }

    #[test]
    fn bombardment_is_recorded_volley_by_volley() {
        let mut full = 0;
        for seed in 1..=300u64 {
            let mut game = seeded(arena(10, 10), seed);
            let gun = spawn(&mut game, RED, "artillery", 4, 4);
            let target = spawn(&mut game, BLUE, "infantry", 5, 5);
            run(
                &mut game,
                RED,
                Command::Bombard {
                    unit: gun,
                    target: Coord::new(5, 5),
                },
            );
            let Battle::Bombard {
                shooter,
                target: victim,
                shots,
                killed,
                promoted,
            } = only(&game)
            else {
                panic!("expected a bombardment");
            };
            assert_eq!((shooter.id, victim.id), (gun, target));
            assert!(!shots.is_empty() && shots.len() <= 2);
            // Plain artillery wounds but never kills, and a volley stops at one hit point.
            assert!(!killed && !promoted);
            let hits = shots.iter().filter(|&&h| h).count() as i32;
            let hurt = victim.max_hp - victim.hp + hits;
            assert_eq!(game.units[&target].damage, hurt);
            full += usize::from(shots.len() == 2);
        }
        assert!(full > 20, "{full} full volleys");
    }

    #[test]
    fn a_lethal_bombardment_that_kills_ends_on_the_killing_shot() {
        let (mut kills, mut escapes) = (0, 0);
        for seed in 1..=300u64 {
            let mut game = seeded(arena(10, 10), seed);
            let boat = spawn(&mut game, RED, "torpedo-boat", 4, 4);
            let target = spawn(&mut game, BLUE, "infantry", 5, 5);
            // One hit point left: any hit is fatal to a lethal gun.
            game.units.get_mut(&target).unwrap().damage = 3;
            run(
                &mut game,
                RED,
                Command::Bombard {
                    unit: boat,
                    target: Coord::new(5, 5),
                },
            );
            let Battle::Bombard {
                shots,
                killed,
                target: victim,
                ..
            } = only(&game)
            else {
                panic!("expected a bombardment");
            };
            assert_eq!(victim.hp, 1);
            assert_eq!(killed, !game.units.contains_key(&target));
            if killed {
                kills += 1;
                assert_eq!(shots.iter().filter(|&&h| h).count(), 1);
                assert_eq!(shots.last(), Some(&true), "the kill is the last shot");
            } else {
                escapes += 1;
                assert_eq!(shots, [false, false]);
            }
        }
        assert!(
            kills > 20 && escapes > 20,
            "{kills} kills, {escapes} escapes"
        );
    }

    #[test]
    fn the_record_covers_only_the_latest_command() {
        let mut game = seeded(arena(10, 10), 3);
        let a = spawn(&mut game, RED, "cavalry", 4, 4);
        spawn(&mut game, BLUE, "militia", 5, 5);
        let idle = spawn(&mut game, RED, "infantry", 2, 2);
        attack(&mut game, RED, a, 5, 5);
        assert_eq!(game.battles.len(), 1);
        run(&mut game, RED, Command::Fortify { unit: idle });
        assert!(game.battles.is_empty());
    }

    #[test]
    fn a_failed_command_leaves_no_record_behind() {
        let mut game = seeded(arena(10, 10), 3);
        let a = spawn(&mut game, RED, "cavalry", 4, 4);
        spawn(&mut game, BLUE, "militia", 5, 5);
        attack(&mut game, RED, a, 5, 5);
        // Nothing stands at (9, 9), so this attack is refused.
        let refused = game.apply(
            RED,
            Command::Attack {
                unit: a,
                target: Coord::new(9, 9),
            },
            &rules(),
            TickRules::default(),
        );
        assert!(refused.is_err());
        assert!(game.battles.is_empty());
    }

    #[test]
    fn the_record_is_trimmed_to_the_newest() {
        let mut game = arena(10, 10);
        let fighter = |id| Fighter {
            id,
            owner: RED,
            kind: "infantry".into(),
            position: Coord::new(1, 1),
            hp: 2,
            max_hp: 2,
        };
        for n in 0..(BATTLE_LIMIT + 20) as Id {
            game.record(Battle::Bombard {
                shooter: fighter(n),
                target: fighter(n),
                shots: vec![true],
                killed: false,
                promoted: false,
            });
        }
        assert_eq!(game.battles.len(), BATTLE_LIMIT);
        let Battle::Bombard { shooter, .. } = &game.battles[0] else {
            unreachable!()
        };
        assert_eq!(shooter.id, 20 as Id);
    }

    /// Red, blue and green on a wide map; green and blue fight far from red's city.
    fn crossfire(red_scout: bool) -> (Game, Id, Id) {
        let mut game = seeded(arena(30, 30), 11);
        let green = game.factions[&RED].clone();
        game.factions.insert(GREEN, green);
        if red_scout {
            spawn(&mut game, RED, "infantry", 17, 17);
        }
        let a = spawn(&mut game, GREEN, "cavalry", 20, 20);
        let d = spawn(&mut game, BLUE, "militia", 21, 21);
        attack(&mut game, GREEN, a, 21, 21);
        (game, a, d)
    }

    #[test]
    fn each_player_hears_only_of_fights_they_can_know_about() {
        let (game, a, d) = crossfire(false);
        verify_duel(&game, a, d);
        // The two sides and the spectator are told; red, who is nowhere near, is not.
        assert_eq!(game.view(BLUE).battles.len(), 1);
        assert_eq!(game.view(GREEN).battles.len(), 1);
        assert_eq!(game.view(0).battles.len(), 1);
        assert!(game.view(RED).battles.is_empty());
        // The same fight, with a red unit within sight of it.
        let (game, ..) = crossfire(true);
        assert_eq!(game.view(RED).battles.len(), 1);
    }

    #[test]
    fn battles_survive_a_save_and_old_saves_load_without_them() {
        let (game, ..) = crossfire(false);
        assert!(!game.battles.is_empty());
        let json = serde_json::to_string(&game).unwrap();
        let loaded: Game = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded, game);
        // Saves from before records existed have no such key.
        let mut value = serde_json::to_value(&game).unwrap();
        value.as_object_mut().unwrap().remove("battles");
        let old: Game = serde_json::from_value(value).unwrap();
        assert!(old.battles.is_empty());
        // And a game with nothing to report does not write the key at all.
        let quiet = serde_json::to_value(arena(10, 10)).unwrap();
        assert!(quiet.get("battles").is_none());
    }

    #[test]
    fn the_same_seed_records_the_same_fights() {
        let play = || {
            let (game, ..) = crossfire(false);
            game.battles
        };
        let (one, two) = (play(), play());
        assert!(!one.is_empty());
        assert_eq!(one, two);
    }
}
