//! The calendar (`victory.md` 3, `world-events.md` 1.3): turns become
//! time through seven segments of `time_scale_turns` turns of
//! `time_scale_units` base units each (years, months or weeks, `GAME`), from
//! the scenario's start date; there is no year 0.

use crate::scenario::{Settings, settings};

/// `U(t)` (`0x5DF030`): calendar units after `t` rounds (0-based).
pub fn units_in(s: &Settings, mut t: i32) -> i32 {
    let mut acc = 0;
    for i in 0..7 {
        if t < s.scale_turns[i] {
            return acc + s.scale_units[i] * t;
        }
        t -= s.scale_turns[i];
        acc += s.scale_units[i] * s.scale_turns[i];
    }
    acc + t
}

/// `U(t)` of the match's calendar.
pub fn units(t: i32) -> i32 {
    units_in(settings(), t)
}

/// The year, and the month (0-based) when the base unit is smaller than a
/// year, of the clone's turn `turn` (1-based).
fn date(s: &Settings, turn: u32) -> (i32, Option<i32>) {
    let u = units_in(s, turn.saturating_sub(1) as i32);
    // Years run ..., 2 BC, 1 BC, AD 1, ...: count them from 0 and skip the gap.
    let fix = |y: i32| if y == 0 { 1 } else { y };
    match s.base_unit {
        1 => {
            let months = s.start_month - 1 + u;
            (
                fix(s.start_year + months.div_euclid(12)),
                Some(months.rem_euclid(12)),
            )
        }
        2 => {
            let weeks = s.start_week - 1 + u;
            (
                fix(s.start_year + weeks.div_euclid(52)),
                Some(weeks.rem_euclid(52) * 12 / 52),
            )
        }
        _ => (fix(s.start_year + u), None),
    }
}

/// The year of the clone's turn `turn` (1-based), year 0 read as 1.
pub fn year(turn: u32) -> i32 {
    date(settings(), turn).0
}

/// "4000 BC", "AD 10", "Dec 1941".
pub fn label_in(s: &Settings, turn: u32) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let (y, month) = date(s, turn);
    let year = if y < 0 {
        format!("{} BC", -y)
    } else if month.is_some() {
        y.to_string()
    } else {
        format!("{y} AD")
    };
    match month {
        Some(m) => format!("{} {year}", MONTHS[m as usize]),
        None => year,
    }
}

/// The date label of the match's calendar.
pub fn label(turn: u32) -> String {
    label_in(settings(), turn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_segments_match_the_cumulative_values() {
        assert_eq!(
            [
                units(25),
                units(50),
                units(90),
                units(140),
                units(240),
                units(340),
                units(440)
            ],
            [1250, 2250, 3250, 4250, 5250, 5750, 5950]
        );
        assert_eq!(units(441), 5951);
    }

    #[test]
    fn the_first_turn_is_4000_bc_and_there_is_no_year_zero() {
        assert_eq!(label(1), "4000 BC");
        assert_eq!(label(2), "3950 BC");
        assert_eq!(year(141), 250);
        // 4000 = U(t) falls in the fourth segment: 3250 + 20 * 37.5 is not a
        // whole turn, so year 0 is never shown; year(…) maps 0 to 1 anyway.
        assert!((1..600).all(|t| year(t) != 0));
    }

    #[test]
    fn a_scenario_can_count_months() {
        let s = Settings {
            base_unit: 1,
            start_year: 1941,
            start_month: 12,
            scale_turns: [100; 7],
            scale_units: [1; 7],
            ..Settings::default()
        };
        assert_eq!(label_in(&s, 1), "Dec 1941");
        assert_eq!(label_in(&s, 2), "Jan 1942");
        assert_eq!(label_in(&s, 14), "Jan 1943");
    }
}
