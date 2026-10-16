//! Hand-made battles, for looking at the stage without playing to a fight:
//! `--demo-combat NAME[@SECONDS]` plays them around the middle of the screen and, with
//! `@SECONDS`, freezes the clock there so a screenshot catches one exact moment.
use fourx_sim::{Battle, Fighter, Id, Outcome, Support, terrain::Coord};

/// Everything `--demo-combat` accepts, besides `all`.
pub const NAMES: [&str; 8] = [
    "duel",
    "charge",
    "support",
    "naval",
    "bombard",
    "broadside",
    "capture",
    "retreat",
];

fn fighter(id: Id, owner: Id, kind: &str, at: Coord, hp: i32) -> Fighter {
    Fighter {
        id,
        owner,
        kind: kind.into(),
        position: at,
        hp,
        max_hp: hp.max(3),
    }
}

/// The battles `name` stands for, fought around `at` between nations `sides.0` (the attacker)
/// and `sides.1`. Unit ids are made up: nothing here touches the game.
pub fn battles(name: &str, at: Coord, sides: (Id, Id)) -> Vec<Battle> {
    // Neighbours on the same screen row: the attacker on the left, the defender on the right.
    let (left, right) = (Coord::new(at.x, at.y + 1), Coord::new(at.x + 1, at.y));
    let (a, d) = sides;
    let duel = |kinds: (&str, &str), rounds: &[bool], outcome| Battle::Duel {
        attacker: fighter(9001, a, kinds.0, left, 4),
        defender: fighter(9002, d, kinds.1, right, 4),
        support: None,
        rounds: rounds.to_vec(),
        outcome,
        retreat_to: None,
        promoted: false,
    };
    match name {
        "all" => NAMES
            .iter()
            .flat_map(|name| battles(name, at, sides))
            .collect(),
        "duel" => {
            let rounds = [true, false, true, true, true];
            let mut battle = duel(("infantry", "infantry"), &rounds, Outcome::AttackerWon);
            if let Battle::Duel { promoted, .. } = &mut battle {
                *promoted = true;
            }
            vec![battle]
        }
        "charge" => vec![duel(
            ("cavalry", "infantry"),
            &[true, true, false, true, true],
            Outcome::AttackerWon,
        )],
        "support" => {
            let rounds = [false, true, true, true, true];
            let mut battle = duel(("infantry", "infantry"), &rounds, Outcome::AttackerWon);
            if let Battle::Duel { support, .. } = &mut battle {
                *support = Some(Support {
                    shooter: fighter(9003, d, "artillery", right, 3),
                    hit: true,
                });
            }
            vec![battle]
        }
        "naval" => vec![duel(
            ("battleship", "ironclad"),
            &[true, false, true, true, true],
            Outcome::AttackerWon,
        )],
        "bombard" => vec![Battle::Bombard {
            shooter: fighter(9001, a, "artillery", left, 3),
            target: fighter(9002, d, "infantry", right, 4),
            shots: vec![true, false, true],
            killed: false,
            promoted: false,
        }],
        "broadside" => vec![Battle::Bombard {
            shooter: fighter(9001, a, "torpedo-boat", left, 3),
            target: fighter(9002, d, "ironclad", right, 2),
            shots: vec![false, true, true],
            killed: true,
            promoted: true,
        }],
        "capture" => vec![Battle::Capture {
            attacker: fighter(9001, a, "infantry", left, 4),
            target: right,
            taken: vec![
                fighter(9002, d, "worker", right, 2),
                fighter(9003, d, "pioneer", right, 2),
            ],
            destroyed: vec![fighter(9004, d, "ironclad", right, 3)],
            city: None,
            advanced: true,
        }],
        "retreat" => {
            let mut battle = duel(
                ("infantry", "cavalry"),
                &[true, true, true],
                Outcome::DefenderRetreated,
            );
            if let Battle::Duel { retreat_to, .. } = &mut battle {
                *retreat_to = Some(Coord::new(at.x + 2, at.y - 1));
            }
            vec![battle]
        }
        _ => Vec::new(),
    }
}

/// Splits `NAME[@SECONDS]`.
pub fn parse(argument: &str) -> (&str, Option<f32>) {
    match argument.split_once('@') {
        Some((name, at)) => (name, at.parse().ok().filter(|t: &f32| *t >= 0.0)),
        None => (argument, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_makes_a_battle_and_unknown_names_make_none() {
        let at = Coord::new(10, 10);
        for name in NAMES {
            assert_eq!(battles(name, at, (1, 2)).len(), 1, "{name}");
        }
        assert_eq!(battles("all", at, (1, 2)).len(), NAMES.len());
        assert!(battles("nonsense", at, (1, 2)).is_empty());
    }

    #[test]
    fn names_may_carry_a_moment_to_freeze_on() {
        assert_eq!(parse("duel"), ("duel", None));
        assert_eq!(parse("naval@1.5"), ("naval", Some(1.5)));
        assert_eq!(parse("duel@soon"), ("duel", None));
        assert_eq!(parse("duel@-2"), ("duel", None));
    }

    #[test]
    fn every_demo_battle_names_known_designs_and_adds_up() {
        let pack = fourx_content::Pack::base();
        for battle in battles("all", Coord::new(10, 10), (1, 2)) {
            for f in battle.fighters() {
                assert!(pack.rules.units.contains_key(&f.kind), "{}", f.kind);
            }
            if let Battle::Duel {
                attacker,
                defender,
                rounds,
                outcome,
                ..
            } = &battle
            {
                // A recorded win means the loser ran out of hit points in the last round.
                let won = rounds.iter().filter(|&&r| r).count() as i32;
                let lost = rounds.len() as i32 - won;
                match outcome {
                    Outcome::AttackerWon => assert!(won >= defender.hp, "{battle:?}"),
                    Outcome::DefenderWon => assert!(lost >= attacker.hp),
                    _ => {}
                }
            }
        }
    }
}
