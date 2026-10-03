//! Rule-set sizes that decide how long the save's raw arrays are.

use crate::Biq;

/// The five rule counts the loaders size their raw arrays with.
///
/// The game keeps them in globals (`0x9C3D80` buildings, `0x9C3DB0` unit
/// types, `0x9C3DBC` advances, `0x9C3DA4` resources, `0x9C72A8` spaceship
/// parts) that the scenario loader fills from the rule sections of the BIQ it
/// is loading. A save embeds the scenario it was started from, so the counts
/// are those of that embedded BIQ; a section the embedded BIQ lacks (a map-only
/// scenario, every random-map game) leaves the count at the shipped
/// `conquests.biq` value ([`RuleCounts::CONQUESTS`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuleCounts {
    /// `BLDG` rows.
    pub buildings: usize,
    /// `PRTO` rows.
    pub unit_types: usize,
    /// `TECH` rows.
    pub techs: usize,
    /// `GOOD` rows.
    pub goods: usize,
    /// Spaceship part types: the length of the `RULE` row's part list.
    pub space_parts: usize,
}

impl RuleCounts {
    /// The shipped Conquests rules (`conquests.biq` 1.22): what a save without
    /// rule sections of its own uses.
    pub const CONQUESTS: RuleCounts = RuleCounts {
        buildings: 83,
        unit_types: 141,
        techs: 83,
        goods: 26,
        space_parts: 10,
    };

    /// The counts a game started from `biq` runs with: each count from the
    /// BIQ when it has rows of that kind, else [`RuleCounts::CONQUESTS`].
    pub fn from_biq(biq: &Biq) -> RuleCounts {
        let d = RuleCounts::CONQUESTS;
        let or = |n: usize, default: usize| if n == 0 { default } else { n };
        RuleCounts {
            buildings: or(biq.rules.buildings.len(), d.buildings),
            unit_types: or(biq.rules.unit_types.len(), d.unit_types),
            techs: or(biq.rules.techs.len(), d.techs),
            goods: or(biq.rules.goods.len(), d.goods),
            space_parts: biq
                .rules
                .general_rules
                .first()
                .map_or(d.space_parts, |g| g.spaceship_parts_needed.len()),
        }
    }
}
