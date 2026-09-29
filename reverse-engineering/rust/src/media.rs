//! Movies and victory media (`0x4E26C2` intro gate, selectors).
//!
//! See `../media.md`. The UI side only: pref gate, movie choice, and the
//! centered-playback geometry.

/// Intro gate (`0x4E26C2`): plays iff the `PlayIntro` pref reads 1.
pub fn intro_plays(pref: i32) -> bool {
    pref == 1
}

/// Centered-playback offset: `(screen - movie) / 2` per axis, matching the
/// `640x280 from [0x9C733C]/[0x9C7338]` branch (integer coordinates).
pub fn center_offset(screen: u32, movie: u32) -> u32 {
    screen.saturating_sub(movie) / 2
}

/// Victory movie choice (`0x4E28C1`): race film vs victory film. The
/// branch predicates (`0x54C8C0`/`0x54C950`) are undecoded; the selection
/// itself is a two-way pick recorded here for shape.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VictoryMovie {
    /// `Art\Movies\race.bik`.
    Race,
    /// `victory_movie.bik`.
    Victory,
}

/// Hall-of-Fame name sanitizer (`0x54076D-0x54078A`): every ASCII space
/// becomes an underscore before the record is appended.
pub fn hof_sanitize(name: &str) -> String {
    name.chars()
        .map(|c| if c == ' ' { '_' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intro_gate_is_equality_with_one() {
        assert!(intro_plays(1));
        assert!(!intro_plays(0));
        assert!(!intro_plays(2));
    }

    #[test]
    fn centering_math() {
        // 640x280 movie on an 800x600 screen centers at (80, 160).
        assert_eq!(center_offset(800, 640), 80);
        assert_eq!(center_offset(600, 280), 160);
        assert_eq!(center_offset(640, 640), 0);
        assert_eq!(center_offset(100, 640), 0); // saturates, never wraps
    }

    #[test]
    fn hof_names_replace_spaces() {
        // 0x540774: 0x20 -> 0x5F per byte; everything else untouched.
        assert_eq!(hof_sanitize("Joan of Arc"), "Joan_of_Arc");
        assert_eq!(hof_sanitize("Shaka"), "Shaka");
        assert_eq!(hof_sanitize("  "), "__");
        assert_eq!(hof_sanitize("a\tb"), "a\tb");
    }
}
