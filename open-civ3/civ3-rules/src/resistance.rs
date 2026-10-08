//! Resistance of captured citizens and nationality drift.
//!
//! A military capture marks every citizen of another race with a pending change
//! to the captor's race and rolls its initial resistance; police units re-roll
//! resisters with the continued chance; and a non-resisting citizen may adopt
//! its pending race over time.

use crate::population::Citizen;

/// `CULT` facts read here: `(culture_ratio_percent, initial %, continued %)`.
pub type CultRow = (i32, i32, i32);

/// `(int)(a * 100.0f / b + 0.5f)` via `_ftol` (truncation; a non-finite or
/// out-of-range value is `i32::MIN`).
pub fn ratio(a: i32, b: i32) -> i32 {
    let v = f64::from(a) * 100.0 / f64::from(b) + 0.5;
    if v.is_finite() && v >= f64::from(i32::MIN) && v < 2_147_483_648.0 {
        v.trunc() as i32
    } else {
        i32::MIN
    }
}

/// The row with the greatest ratio at or below `ratio(a, b)`, the first on ties;
/// otherwise the loader's smallest-ratio row.
pub fn cult_row(rows: &[CultRow], a: i32, b: i32) -> Option<usize> {
    let r = ratio(a, b);
    let mut best: Option<usize> = None;
    for (i, row) in rows.iter().enumerate() {
        if row.0 <= r && best.is_none_or(|k| row.0 > rows[k].0) {
            best = Some(i);
        }
    }
    best.or_else(|| {
        let mut min: Option<usize> = None;
        for (i, row) in rows.iter().enumerate() {
            if min.is_none_or(|k| row.0 < rows[k].0) {
                min = Some(i);
            }
        }
        min
    })
}

/// What the resistance roll reads about the citizen's former nation `S` and its
/// owner.
#[derive(Clone, Copy, Debug)]
pub struct Standing {
    /// `Player[S]` has cities. False also for a race no player holds.
    pub other_has_cities: bool,
    /// The owner is at war with `S`.
    pub at_war: bool,
    /// `Player[owner]`'s culture rating.
    pub owner_rating: i32,
    /// `Player[S]`'s culture rating.
    pub other_rating: i32,
    /// The owner's GOVT row.
    pub owner_govt: usize,
    /// `S`'s GOVT row.
    pub other_govt: usize,
}

/// Entering resistance takes the citizen off its tile and specialist job.
pub fn set_resisting(c: &mut Citizen, resist: bool) {
    if c.resister != resist {
        c.resister = resist;
        if resist {
            c.work = 0;
            c.job = 0;
        }
    }
}

/// Marks a captured citizen: a foreigner gets a pending change to the captor's
/// race; captor kin clears resistance and the pending change.
pub fn mark_capture(c: &mut Citizen, captor_race: i32, turn: i32) {
    if c.race != captor_race {
        c.pending_race = captor_race;
        c.pending_turn = turn;
    } else {
        set_resisting(c, false);
        c.pending_race = -1;
        c.pending_turn = -1;
    }
}

/// Returns whether the citizen resists afterwards. `standing` is consulted only
/// when a race change is pending; the die is drawn only past both gates.
pub fn reroll(
    c: &mut Citizen,
    initial: bool,
    rows: &[CultRow],
    govt_resistance: &[impl AsRef<[i32]>],
    standing: impl FnOnce(i32) -> Standing,
    rand: impl FnOnce(i32) -> i32,
) -> bool {
    let mut result = false;
    if c.pending_turn != -1 && c.pending_race >= 0 {
        let s = standing(c.race);
        if s.other_has_cities && s.at_war {
            let term = cult_row(rows, s.owner_rating, s.other_rating)
                .map(|k| if initial { rows[k].1 } else { rows[k].2 })
                .unwrap_or(0);
            let modifier = govt_resistance[s.owner_govt].as_ref()[s.other_govt];
            result = rand(100) & 0xFFFF < modifier + term;
        }
    }
    set_resisting(c, result);
    c.resister
}

/// Each police try re-rolls the first resister in slot order with the continued
/// chance. Returns the citizens quelled; the caller reports `RESISTANCEQUELLED`
/// only while resisters remain.
pub fn quell(
    citizens: &mut [&mut Citizen],
    police: i32,
    mut reroll_one: impl FnMut(&mut Citizen) -> bool,
) -> i32 {
    let mut left = citizens.iter().filter(|c| c.resister).count() as i32;
    let mut quelled = 0;
    for _ in 0..police.max(0) {
        if left <= 0 {
            break;
        }
        let Some(c) = citizens.iter_mut().find(|c| c.resister) else {
            break;
        };
        if !reroll_one(c) {
            left -= 1;
            quelled += 1;
        }
    }
    quelled
}

/// Returns true when the citizen took its pending race.
pub fn drift(
    c: &mut Citizen,
    turn: i32,
    owner_rating: i32,
    other_rating: i32,
    assimilation: i32,
    rand: impl FnOnce(i32) -> i32,
) -> bool {
    if c.pending_turn == -1 || c.pending_race < 0 || c.resister {
        return false;
    }
    let pending_for = turn - c.pending_turn;
    let lived_as = c.pending_turn - c.since;
    if pending_for <= lived_as || owner_rating <= other_rating {
        return false;
    }
    if rand(100) >= assimilation {
        return false;
    }
    c.race = c.pending_race;
    c.since = turn;
    c.pending_race = -1;
    c.pending_turn = -1;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROWS: [CultRow; 6] = [
        (300, 40, 30),
        (200, 50, 40),
        (100, 60, 50),
        (75, 70, 60),
        (50, 80, 70),
        (33, 90, 80),
    ];

    fn foreign(race: i32) -> Citizen {
        Citizen {
            race,
            work: 4,
            job: 0,
            resister: false,
            since: 0,
            pending_race: -1,
            pending_turn: -1,
        }
    }

    #[test]
    fn ratio_rounds_and_maps_non_finite_to_the_fallback_row() {
        assert_eq!(ratio(1, 3), 33);
        assert_eq!(ratio(2, 3), 67);
        assert_eq!(ratio(5, 0), i32::MIN);
        assert_eq!(cult_row(&ROWS, 300, 100), Some(0));
        assert_eq!(cult_row(&ROWS, 299, 100), Some(1));
        assert_eq!(cult_row(&ROWS, 32, 100), Some(5));
    }

    #[test]
    fn capture_marks_foreigners_and_clears_captor_kin() {
        let mut c = foreign(3);
        mark_capture(&mut c, 5, 40);
        assert_eq!((c.pending_race, c.pending_turn), (5, 40));
        let mut kin = Citizen { resister: true, pending_race: 2, pending_turn: 9, ..foreign(5) };
        mark_capture(&mut kin, 5, 40);
        assert!(!kin.resister);
        assert_eq!((kin.pending_race, kin.pending_turn), (-1, -1));
    }

    fn standing(at_war: bool) -> Standing {
        Standing {
            other_has_cities: true,
            at_war,
            owner_rating: 100,
            other_rating: 100,
            owner_govt: 1,
            other_govt: 2,
        }
    }

    #[test]
    fn initial_roll_uses_culture_row_and_government_modifier() {
        let mut govt = [[0; 8]; 8];
        govt[1][2] = 5;
        let mut c = foreign(3);
        mark_capture(&mut c, 5, 40);
        assert!(reroll(&mut c, true, &ROWS, &govt, |_| standing(true), |_| 64));
        assert_eq!((c.work, c.job), (0, 0));
        let mut c = foreign(3);
        mark_capture(&mut c, 5, 40);
        assert!(!reroll(&mut c, false, &ROWS, &govt, |_| standing(true), |_| 55));
    }

    #[test]
    fn peace_or_no_pending_change_ends_resistance_without_a_draw() {
        let govt = [[0; 8]; 8];
        let mut c = Citizen { resister: true, ..foreign(3) };
        mark_capture(&mut c, 5, 40);
        assert!(!reroll(&mut c, false, &ROWS, &govt, |_| standing(false), |_| panic!("no draw")));
    }

    #[test]
    fn drift_waits_longer_than_the_old_race_lasted_and_needs_more_culture() {
        let mut c = Citizen { since: 10, pending_race: 5, pending_turn: 30, ..foreign(3) };
        assert!(!drift(&mut c, 50, 200, 100, 4, |_| 0));
        assert!(!drift(&mut c, 51, 100, 100, 4, |_| 0));
        assert!(!drift(&mut c, 51, 200, 100, 4, |_| 4));
        assert!(drift(&mut c, 51, 200, 100, 4, |_| 3));
        assert_eq!((c.race, c.since, c.pending_race, c.pending_turn), (5, 51, -1, -1));
    }
}
