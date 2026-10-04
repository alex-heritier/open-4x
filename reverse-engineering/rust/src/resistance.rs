//! Resistance of captured citizens and nationality drift (`city-turn.md` 8).
//!
//! * `0x4BB090` (called by `Player::takeCity` `0x564800`) marks every citizen
//!   of another race with a pending change to the captor's race
//!   (`0x4ABE10`) and, on a military capture that is not a culture
//!   conversion, rolls its initial resistance (`0x4ABE90(c; 1)`).
//! * `0x4B2E10` quells: each police unit re-rolls the first resister with the
//!   continued chance (`0x4ABE90(c; 0)`).
//! * `0x4AC140` lets a non-resisting citizen adopt the pending race.
//!
//! The CULT row comes from `0x4F8C50`. When no row's ratio is at or below
//! the computed ratio, the row of `[0x9C3D6C]` is used: the RULE loader
//! (`0x599570..0x599585`, executed in the emulator over the shipped rules)
//! stores there the index of the row with the smallest ratio (shipped: row
//! 5, ratio 33).

use crate::population::Citizen;

/// `CULT` facts read here: `(culture_ratio_percent, initial %, continued %)`.
pub type CultRow = (i32, i32, i32);

/// `0x4F8C50`: `(int)(a * 100.0f / b + 0.5f)` via `_ftol` (truncation; a
/// non-finite or out-of-range value is `0x80000000`).
pub fn ratio(a: i32, b: i32) -> i32 {
    let v = f64::from(a) * 100.0 / f64::from(b) + 0.5;
    if v.is_finite() && v >= f64::from(i32::MIN) && v < 2_147_483_648.0 { v.trunc() as i32 } else { i32::MIN }
}

/// The row with the greatest ratio at or below `ratio(a, b)`, the first on
/// ties; otherwise the loader's smallest-ratio row (**H** on ties between
/// equal smallest ratios: the shipped rows are distinct).
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
            if min.is_none_or(|k| row.0 < rows[k].0) { min = Some(i); }
        }
        min
    })
}

/// What `0x4ABE90` reads about the citizen's former nation `S` and its owner.
#[derive(Clone, Copy, Debug)]
pub struct Standing {
    /// `Player[S].+0x194 > 0`. False also for a race no player holds
    /// (`0x539D60` returns -1; the binary then reads outside the player
    /// array, **H**).
    pub other_has_cities: bool,
    /// `Player[owner].+0xD30[S]`: the owner is at war with `S`.
    pub at_war: bool,
    /// `Player[owner].+0x183C`.
    pub owner_rating: i32,
    /// `Player[S].+0x183C`.
    pub other_rating: i32,
    /// `Player[owner].+0xA0` (a GOVT row).
    pub owner_govt: usize,
    /// `Player[S].+0xA0` (a GOVT row).
    pub other_govt: usize,
}

/// `0x4AC000(c; resist)`: entering resistance takes the citizen off its tile
/// and specialist job.
pub fn set_resisting(c: &mut Citizen, resist: bool) {
    if c.resister != resist {
        c.resister = resist;
        if resist {
            c.work = 0;
            c.job = 0;
        }
    }
}

/// `0x4ABE10(c; captor race)`.
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

/// `0x4ABE90(c; initial)`: returns whether the citizen resists afterwards.
/// `standing` is consulted only when a race change is pending; `rand(100)`
/// (gameplay `Random`, masked to 16 bits) is drawn only past both gates.
pub fn reroll(
    c: &mut Citizen,
    initial: bool,
    rows: &[CultRow],
    govt_resistance: &[[i32; 8]; 8],
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
            let modifier = govt_resistance[s.owner_govt][s.other_govt];
            result = rand(100) & 0xFFFF < modifier + term;
        }
    }
    set_resisting(c, result);
    c.resister
}

/// `0x4B2E10`: `police` is `0x5A6060(x,y,4,..)` times DIFF
/// `citizens_quelled_by_military`. Each try re-rolls the first resister in
/// slot order with the continued chance. Returns the citizens quelled; the
/// caller reports `RESISTANCEQUELLED` only while resisters remain.
pub fn quell(
    citizens: &mut [&mut Citizen],
    police: i32,
    mut reroll_one: impl FnMut(&mut Citizen) -> bool,
) -> i32 {
    let mut left = citizens.iter().filter(|c| c.resister).count() as i32;
    let mut quelled = 0;
    for _ in 0..police.max(0) {
        if left <= 0 { break; }
        let Some(c) = citizens.iter_mut().find(|c| c.resister) else { break };
        if !reroll_one(c) {
            left -= 1;
            quelled += 1;
        }
    }
    quelled
}

/// `0x4AC140`: returns true when the citizen took its pending race.
/// `other_rating` is `Player[S].+0x183C` for the citizen's current race.
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

    const ROWS: [CultRow; 6] = [(300, 40, 30), (200, 50, 40), (100, 60, 50), (75, 70, 60), (50, 80, 70), (33, 90, 80)];

    fn foreign(race: i32) -> Citizen {
        Citizen { race, work: 4, job: 0, resister: false, since: 0, pending_race: -1, pending_turn: -1 }
    }

    #[test]
    fn ratio_rounds_and_maps_non_finite_to_the_fallback_row() {
        assert_eq!(ratio(1, 3), 33);
        assert_eq!(ratio(2, 3), 67);
        assert_eq!(ratio(5, 0), i32::MIN);
        assert_eq!(ratio(0, 0), i32::MIN);
        assert_eq!(cult_row(&ROWS, 300, 100), Some(0));
        assert_eq!(cult_row(&ROWS, 299, 100), Some(1));
        assert_eq!(cult_row(&ROWS, 2, 3), Some(4));
        assert_eq!(cult_row(&ROWS, 1, 3), Some(5));
        assert_eq!(cult_row(&ROWS, 32, 100), Some(5), "below every row: the smallest-ratio row");
        assert_eq!(cult_row(&ROWS, 7, 0), Some(5));
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
        Standing { other_has_cities: true, at_war, owner_rating: 100, other_rating: 100, owner_govt: 1, other_govt: 2 }
    }

    #[test]
    fn initial_roll_uses_culture_row_and_government_modifier() {
        let mut govt = [[0; 8]; 8];
        govt[1][2] = 5;
        // Equal culture: row 2, initial 60 + 5 = 65.
        for (die, resists) in [(64, true), (65, false)] {
            let mut c = foreign(3);
            mark_capture(&mut c, 5, 40);
            let mut drawn = None;
            assert_eq!(reroll(&mut c, true, &ROWS, &govt, |_| standing(true), |n| { drawn = Some(n); die }), resists);
            assert_eq!(drawn, Some(100));
            if resists { assert_eq!((c.work, c.job), (0, 0)); }
        }
        // Continued chance is 50 + 5.
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
        let mut own = Citizen { resister: true, ..foreign(5) };
        assert!(!reroll(&mut own, false, &ROWS, &govt, |_| panic!("not consulted"), |_| panic!("no draw")));
    }

    #[test]
    fn quelling_retries_the_first_resister_and_counts_successes() {
        let mut a = Citizen { resister: true, ..foreign(3) };
        let mut b = foreign(3);
        let mut d = Citizen { resister: true, ..foreign(3) };
        let mut dice = [true, false, false].into_iter();
        let mut all = [&mut a, &mut b, &mut d];
        let q = quell(&mut all, 3, |c| { let r = dice.next().unwrap(); c.resister = r; r });
        assert_eq!(q, 2);
        assert!(!a.resister && !d.resister);
    }

    #[test]
    fn drift_waits_longer_than_the_old_race_lasted_and_needs_more_culture() {
        let mut c = Citizen { since: 10, pending_race: 5, pending_turn: 30, ..foreign(3) };
        assert!(!drift(&mut c, 50, 200, 100, 4, |_| 0), "20 turns pending, 20 lived as");
        assert!(!drift(&mut c, 51, 100, 100, 4, |_| 0), "equal culture");
        assert!(!drift(&mut c, 51, 200, 100, 4, |_| 4));
        assert!(drift(&mut c, 51, 200, 100, 4, |_| 3));
        assert_eq!((c.race, c.since, c.pending_race, c.pending_turn), (5, 51, -1, -1));
        let mut r = Citizen { resister: true, since: 0, pending_race: 5, pending_turn: 1, ..foreign(3) };
        assert!(!drift(&mut r, 90, 200, 100, 100, |_| panic!("resisters never drift")));
    }
}
