//! Ownership of map objects: the `(owner_type, owner)` dword pair shared by
//! `UNIT`, `CITY`, `SLOC` and `CLNY` rows.
//!
//! | `owner_type` | meaning | `owner` is |
//! |---|---|---|
//! | `0` | nobody | unused |
//! | `1` | barbarian tribe | index into the **barbarian race's name list** (`RACE` row 0, `city_names`, up to 76 entries; `75` = the constructor default) |
//! | `2` | civilization | `RACE` row index |
//! | `3` | player | `LEAD` row index (0-based) |
//!
//! Evidence:
//! * **A** game scenario→game conversion `0x5D2D4B..0x5D2E0F` (`Scenario` unit
//!   placement): type `1` activates the barbarian player slot and passes
//!   `row[+0x28]` as the tribe id, type `2` resolves the civilization's player
//!   slot, type `3` uses `LEAD index + 1` as the player id (slot `0` is the
//!   barbarians).
//! * **A** swap-two-players fix-up `0x599CF0` rewrites `owner` only where
//!   `owner_type == 3` (`SLOC`, `CITY`, `UNIT`, `CLNY`).
//! * **B** editor *Select Active Player* dialog (id 192): *None* + three combo
//!   boxes (barbarian tribes, civilizations, players) and the placement errors
//!   "Units must be assigned to a civilization, player or barbarians", "Cities
//!   cannot be assigned to barbarians", "Barbarians cannot have starting
//!   locations".
//! * **C** corpus: `TETurkhan.bix` units use `owner_type 1` with owners `0..=75`
//!   against a 76-entry tribe list; `Rise_of_Rome` uses `2` with the `RACE`
//!   index of the unit's civilization.

/// `owner_type` value: not owned.
pub const NONE: i32 = 0;
/// `owner_type` value: barbarian tribe.
pub const BARBARIAN_TRIBE: i32 = 1;
/// `owner_type` value: civilization (`RACE` index).
pub const CIVILIZATION: i32 = 2;
/// `owner_type` value: player (`LEAD` index).
pub const PLAYER: i32 = 3;

/// A decoded `(owner_type, owner)` pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Owner {
    /// `owner_type 0`.
    Nobody,
    /// `owner_type 1`; payload is the tribe index (barbarian race name list).
    BarbarianTribe(i32),
    /// `owner_type 2`; payload is a `RACE` index.
    Civilization(i32),
    /// `owner_type 3`; payload is a `LEAD` index.
    Player(i32),
}

impl Owner {
    /// Decode the raw pair. `None` for an `owner_type` outside `0..=3`.
    pub fn from_raw(owner_type: i32, owner: i32) -> Option<Owner> {
        Some(match owner_type {
            NONE => Owner::Nobody,
            BARBARIAN_TRIBE => Owner::BarbarianTribe(owner),
            CIVILIZATION => Owner::Civilization(owner),
            PLAYER => Owner::Player(owner),
            _ => return None,
        })
    }

    /// The raw `(owner_type, owner)` pair. `Nobody` encodes as `(0, 0)`.
    pub fn to_raw(self) -> (i32, i32) {
        match self {
            Owner::Nobody => (NONE, 0),
            Owner::BarbarianTribe(t) => (BARBARIAN_TRIBE, t),
            Owner::Civilization(c) => (CIVILIZATION, c),
            Owner::Player(p) => (PLAYER, p),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_roundtrip() {
        for (t, o) in [(1, 75), (2, 8), (3, 0)] {
            assert_eq!(Owner::from_raw(t, o).unwrap().to_raw(), (t, o));
        }
        assert_eq!(Owner::from_raw(0, 5), Some(Owner::Nobody));
        assert_eq!(Owner::from_raw(4, 0), None);
        assert_eq!(Owner::from_raw(-1, 0), None);
    }
}
