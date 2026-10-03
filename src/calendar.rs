//! The calendar (`victory.md` 3, `world-events.md` 1.3): turns become
//! years through seven segments of `time_scale_turns` turns of
//! `time_scale_units` years each, from 4000 BC; there is no year 0.

/// `time_scale_turns` and `time_scale_units` of `conquests.biq`.
const TURNS: [i32; 7] = [25, 25, 40, 50, 100, 100, 100];
const UNITS: [i32; 7] = [50, 40, 25, 20, 10, 5, 2];
/// The scenario's start year (`[0x9C4108]`).
const START: i32 = -4000;

/// `U(t)` (`0x5DF030`): calendar units after `t` rounds (0-based).
pub fn units(mut t: i32) -> i32 {
    let mut acc = 0;
    for i in 0..7 {
        if t < TURNS[i] {
            return acc + UNITS[i] * t;
        }
        t -= TURNS[i];
        acc += UNITS[i] * TURNS[i];
    }
    acc + t
}

/// The year of the clone's turn `turn` (1-based), year 0 read as 1.
pub fn year(turn: u32) -> i32 {
    let y = START + units(turn.saturating_sub(1) as i32);
    if y == 0 { 1 } else { y }
}

/// "4000 BC", "AD 10".
pub fn label(turn: u32) -> String {
    let y = year(turn);
    if y < 0 { format!("{} BC", -y) } else { format!("{y} AD") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_segments_match_the_cumulative_values() {
        assert_eq!([units(25), units(50), units(90), units(140), units(240), units(340), units(440)],
            [1250, 2250, 3250, 4250, 5250, 5750, 5950]);
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
}
