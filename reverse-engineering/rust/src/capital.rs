//! Replacement capital score in Player::capitalLost (`0x4482B0`).

/// `0x448353..0x44841F`: population, twice owner nationals, martial-law
/// units, then 1/2/3 for every other owned town/city/metropolis encountered
/// at spiral indices 1..288. The spatial caller preserves wrap duplicates.
pub fn score(size: i32, nationals: i32, police: i32, neighbors: impl IntoIterator<Item = i32>) -> i32 {
    size + 2 * nationals + police + neighbors.into_iter().map(|n|
        1 + i32::from(n > crate::economy::TOWN_MAX) + i32::from(n > crate::economy::CITY_MAX)
    ).sum::<i32>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nationality_and_garrison_can_outweigh_raw_population() {
        assert_eq!(score(3, 3, 2, []), 11);
        assert_eq!(score(10, 0, 0, []), 10);
    }

    #[test]
    fn neighbor_classes_use_strict_six_and_twelve_thresholds() {
        assert_eq!(score(1, 1, 0, [6, 7, 12, 13]), 3 + 1 + 2 + 2 + 3);
    }
}
