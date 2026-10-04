//! What a player may build: the two player-level eligibility predicates
//! `Player::canBuildImprovement` (`0x56A2A0`) and `Player::canBuildUnit`
//! (`0x56A7C0`), plus the key layout of the free-building set `0x55A560`
//! rebuilds.
//!
//! Findings, addresses and the open list are in `../buildable.md`. Both
//! predicates are decision trees over the BLDG or PRTO row and a handful of
//! facts the binary reads from the world; [`Realm`] names those facts so a
//! caller (or a test) supplies them. Whatever a *city* adds on top (the
//! required building in the city, the coast, the resources in the trade
//! network) is not decoded and not modelled.
//!
//! The order of the checks matters only for which check fires first, never for
//! the answer: every branch returns `false` and the single exit is `true`.

/// Bits of the BLDG `improvement_flags` word (memory `+0xEC`) this file reads.
pub mod improvement_flags {
    /// "Center of Empire": the Palace.
    pub const CENTER_OF_EMPIRE: u32 = 1 << 0;
    /// "Capitalization": Wealth (`0x56A3A0`, `0x56A744`).
    pub const CAPITALIZATION: u32 = 1 << 19;
}

/// Bits of the BLDG `other_characteristics` word (memory `+0xF0`).
pub mod characteristics {
    /// The "Militaristic" box: what a mobilized player may still build.
    pub const MILITARISTIC: u32 = 1 << 1;
    /// Category "Wonder" (a great wonder, one per world).
    pub const WONDER: u32 = 1 << 2;
    /// Category "Sm. Wonder" (one per civilization).
    pub const SMALL_WONDER: u32 = 1 << 3;
}

/// Bits of the BLDG `small_wonder_flags` word (memory `+0xF4`).
pub mod small_wonder_flags {
    /// "Build Spaceship Parts" (Apollo Program).
    pub const BUILD_SPACESHIP_PARTS: u32 = 1 << 4;
    /// "Reduces Corruption" (Forbidden Palace, Secret Police HQ).
    pub const REDUCES_CORRUPTION: u32 = 1 << 5;
    /// "Requires a Victorious Army" (Heroic Epic, Military Academy).
    pub const REQUIRES_VICTORIOUS_ARMY: u32 = 1 << 10;
    /// "Requires Elite Naval Units". Never set in the shipped rules.
    pub const REQUIRES_ELITE_NAVAL_UNITS: u32 = 1 << 11;
}

/// Bits of the BLDG `wonder_flags` word (memory `+0xF8`).
pub mod wonder_flags {
    /// "Allows Construction of Nuclear Devices" (Manhattan Project).
    pub const ALLOWS_NUCLEAR_DEVICES: u32 = 1 << 8;
    /// "Allows Diplomatic Victory" (United Nations).
    pub const ALLOWS_DIPLOMATIC_VICTORY: u32 = 1 << 13;
}

/// The BLDG fields `0x56A2A0` reads (memory offsets; stride `0x110`,
/// `conquests.biq` body offset = memory offset - 4). `-1` is "none" for every
/// index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Improvement {
    /// `+0x90`: the building that must already stand (`BLDG` index).
    pub required_improvement: i32,
    /// `+0xC0`: how many of it the player must own; 1 for everything but
    /// Wall Street and its siblings (5).
    pub num_buildings_required: i32,
    /// `+0xD4`: `GOVT` index.
    pub required_government: i32,
    /// `+0xD8`: spaceship part type, `-1` for a building.
    pub spaceship_part: i32,
    /// `+0xDC`: `TECH` index.
    pub required_advance: i32,
    /// `+0xEC`, see [`improvement_flags`].
    pub improvement_flags: u32,
    /// `+0xF0`, see [`characteristics`].
    pub characteristics: u32,
    /// `+0xF4`, see [`small_wonder_flags`].
    pub small_wonder_flags: u32,
    /// `+0xF8`, see [`wonder_flags`].
    pub wonder_flags: u32,
    /// `+0xFC`: armies the player must own (Pentagon 3, others 0).
    pub armies_required: i32,
}

impl Default for Improvement {
    /// A plain building with no requirement at all.
    fn default() -> Self {
        Improvement {
            required_improvement: -1,
            num_buildings_required: 1,
            required_government: -1,
            spaceship_part: -1,
            required_advance: -1,
            improvement_flags: 0,
            characteristics: 0,
            small_wonder_flags: 0,
            wonder_flags: 0,
            armies_required: 0,
        }
    }
}

/// The PRTO fields `0x56A7C0` reads (memory offsets; stride `0x138`, body
/// offset = memory offset - 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitType {
    /// `+0x74`: `TECH` index.
    pub required_tech: i32,
    /// `+0x90`: bit `n` set when the civ with `RACE` index `n` may build it.
    pub available_to_civs: u32,
    /// `+0xA0`: the prototype this one is the AI's alternate of, else `-1`.
    pub alt_strategy_of: i32,
    /// Ability bit 18 (`0x5E4EF0(0x12)`): an Army.
    pub army: bool,
    /// Ability bit 16 (`0x5E4EF0(0x10)`): a nuclear weapon.
    pub nuclear: bool,
    /// Ability bit 29 (`0x5E4EF0(0x1D)`): a king (a leader of a civ).
    pub king: bool,
}

impl Default for UnitType {
    /// A plain unit every civ can build from the start.
    fn default() -> Self {
        UnitType {
            required_tech: -1,
            available_to_civs: u32::MAX,
            alt_strategy_of: -1,
            army: false,
            nuclear: false,
            king: false,
        }
    }
}

/// The facts the two predicates read from the world. Each method names the
/// field or routine it stands for.
pub trait Realm {
    /// The bit of the player's civ in the technology's learned mask
    /// (`[0xA52B4C][tech]`, `1 << Player +0x1C`). Only called for a real
    /// technology index; [`has_tech`] handles the two special values.
    fn knows(&self, tech: i32) -> bool;
    /// `[0x9C3DBC]`, the number of technologies: the "no technology can
    /// ever grant it" value of a prerequisite.
    fn tech_count(&self) -> i32;
    /// `Player +0xA0`.
    fn government(&self) -> i32;
    /// `Player +0xA4 == 1`.
    fn mobilized(&self) -> bool;
    /// `Player +0x15DC[building]` (words): improvements of that type the
    /// player owns.
    fn improvements_owned(&self, building: i32) -> i32;
    /// `Player +0x15FC[part]` (words): spaceship parts of that type built.
    fn parts_built(&self, part: i32) -> i32;
    /// `[0x9C724C][part]`: how many parts of that type the ship takes (the
    /// RULE int array behind the count `[0x9C72A8]`; ten entries, all 1 in
    /// the shipped rules).
    fn part_limit(&self, part: i32) -> i32;
    /// `[0xA5267C] & 2`: the Space Race victory is enabled.
    fn space_race(&self) -> bool;
    /// `[0xA5267C] & 4`: the Diplomatic victory is enabled.
    fn diplomatic_victory(&self) -> bool;
    /// `Player::countSmallWonderFlag(0x10, 0) != 0` (`0x55AA10`): the player
    /// owns a wonder with "Build Spaceship Parts" that its government allows.
    fn owns_spaceship_wonder(&self) -> bool;
    /// `WonderRegistry@0xA52658 +0x4FC[building]` (`0x538FE0`): the great
    /// wonder has been built, by anyone.
    fn great_wonder_built(&self, building: i32) -> bool;
    /// `Player +0x15E8[building] != -1`: the player owns that small wonder.
    fn owns_small_wonder(&self, building: i32) -> bool;
    /// `Player +0x40` bit 0 (HYPOTHESIS: set by a victory of an army).
    fn victorious_army(&self) -> bool;
    /// `Player +0x40` bit 8 (HYPOTHESIS: elite naval units).
    fn elite_navy(&self) -> bool;
    /// `Player +0x188` (word): armies the player owns.
    fn armies(&self) -> i32;
    /// `Player +0x194`: cities the player owns.
    fn cities(&self) -> i32;
    /// `WSIZ[[0x9C73A0]]` memory `+4`: the world size's optimal number of
    /// cities (shipped 14 / 17 / 20 / 28 / 36), not the per-player OCN.
    fn world_size_cities(&self) -> i32;
    /// Membership of `building` in the set at `Player +0x20D0` that
    /// `0x55A560` rebuilds from the player's great wonders ([`grant_key`]).
    fn granted_by_wonder(&self, building: i32) -> bool;
    /// Strict mode only: one of the player's cities is building `building`
    /// (`City +0x4C/+0x50`, via `0x437E30`) or has it in its queue
    /// (`City +0x1F8` count, `+0x1FC` pairs of item and kind).
    fn in_production(&self, building: i32) -> bool;
    /// Strict mode only: how many of the player's cities build or queue
    /// something whose BLDG row has spaceship part type `part`.
    fn parts_in_production(&self, part: i32) -> i32;
}

/// `Player::hasTech` (`0x561440`) for a prerequisite: `-1` is always met, the
/// technology count is never met, anything else is the learned bit.
pub fn has_tech(realm: &impl Realm, tech: i32) -> bool {
    if tech == -1 {
        true
    } else if tech == realm.tech_count() {
        false
    } else {
        realm.knows(tech)
    }
}

/// `Player::canBuildImprovement(building, strict)` (`0x56A2A0`, `ret 8`).
///
/// `strict` is the second argument; with it set the answer also excludes
/// what the player is already building: a Palace, wonder, small wonder or
/// spaceship part that any city builds or has queued, and a spaceship part
/// when parts built plus parts in production already reach the ship's limit.
/// The order below is the order of the code.
pub fn can_build_improvement(realm: &impl Realm, id: i32, b: &Improvement, strict: bool) -> bool {
    use characteristics::{MILITARISTIC, SMALL_WONDER, WONDER};
    use improvement_flags::{CAPITALIZATION, CENTER_OF_EMPIRE};

    // 0x56A2C1: the technology.
    if !has_tech(realm, b.required_advance) {
        return false;
    }
    // 0x56A2FA: N of another improvement owned empire-wide (Wall Street).
    if b.required_improvement != -1
        && b.num_buildings_required > 1
        && realm.improvements_owned(b.required_improvement) < b.num_buildings_required
    {
        return false;
    }
    // 0x56A32C: the government.
    if b.required_government != -1 && realm.government() != b.required_government {
        return false;
    }
    let part = b.spaceship_part;
    let wealth = b.improvement_flags & CAPITALIZATION != 0;
    if strict {
        // 0x56A358..0x56A469: one at a time.
        let unique = b.improvement_flags & CENTER_OF_EMPIRE != 0
            || b.characteristics & (WONDER | SMALL_WONDER) != 0
            || part != -1;
        if unique && !wealth && realm.in_production(id) {
            return false;
        }
        // 0x56A471..0x56A584: the ship's limit counts what is under way.
        if part != -1
            && realm.parts_built(part) + realm.parts_in_production(part) == realm.part_limit(part)
        {
            return false;
        }
    }
    // 0x56A592: spaceship parts.
    if part != -1
        && (!realm.space_race()
            || !realm.owns_spaceship_wonder()
            || realm.parts_built(part) == realm.part_limit(part))
    {
        return false;
    }
    // 0x56A60B: a great wonder exists once.
    if b.characteristics & WONDER != 0 && realm.great_wonder_built(id) {
        return false;
    }
    // 0x56A649: the United Nations needs the Diplomatic victory enabled.
    let diplomatic = b.wonder_flags & wonder_flags::ALLOWS_DIPLOMATIC_VICTORY != 0;
    if diplomatic && !realm.diplomatic_victory() {
        return false;
    }
    // 0x56A676: a small wonder once per civilization.
    if b.characteristics & SMALL_WONDER != 0 && realm.owns_small_wonder(id) {
        return false;
    }
    // 0x56A69E: army conditions.
    if b.small_wonder_flags & small_wonder_flags::REQUIRES_VICTORIOUS_ARMY != 0
        && !realm.victorious_army()
    {
        return false;
    }
    if b.small_wonder_flags & small_wonder_flags::REQUIRES_ELITE_NAVAL_UNITS != 0
        && !realm.elite_navy()
    {
        return false;
    }
    if realm.armies() < b.armies_required {
        return false;
    }
    // 0x56A6F1: half the world size's optimal cities (signed halving).
    if b.small_wonder_flags & small_wonder_flags::REDUCES_CORRUPTION != 0
        && realm.cities() < realm.world_size_cities() / 2
    {
        return false;
    }
    // 0x56A732: mobilization, "may not build peacetime improvements".
    if realm.mobilized()
        && b.characteristics & MILITARISTIC == 0
        && !wealth
        && !diplomatic
        && part == -1
    {
        return false;
    }
    // 0x56A76B: not what a wonder already supplies to every city.
    !realm.granted_by_wonder(id)
}

/// `Player::canBuildUnit(prototype, _, allow_king)` (`0x56A7C0`, `ret 0xC`;
/// the middle argument is unread). `human` is the player's bit in the human
/// mask `[0xA526BC]`; `race` is `Player +0x20`.
pub fn can_build_unit(
    realm: &impl Realm,
    id: i32,
    u: &UnitType,
    human: bool,
    race: u32,
    allow_king: bool,
    rules: &UnitRules,
) -> bool {
    // 0x56A7E8: a human may not build the AI's alternate prototypes.
    if human && u.alt_strategy_of != -1 {
        return false;
    }
    // 0x56A80A: kings only when asked for.
    if !allow_king && u.king {
        return false;
    }
    // 0x56A831: the civ (`shl` masks its count to five bits, as here).
    if u.available_to_civs & 1u32.wrapping_shl(race) == 0 {
        return false;
    }
    // 0x56A85A: the technology.
    if !has_tech(realm, u.required_tech) {
        return false;
    }
    // 0x56A889: the Great Leader (RULE "battle-created unit") is never built.
    if id == rules.battle_created_unit {
        return false;
    }
    // 0x56A89B: one army per `cities_per_army` cities.
    if u.army && (realm.armies() + 1) * rules.cities_per_army > realm.cities() {
        return false;
    }
    // 0x56A8CB: nuclear weapons need the Manhattan Project (or its
    // small-wonder form in one of the player's own cities).
    !u.nuclear || rules.nuclear_devices_allowed
}

/// The RULE words and the one world fact `0x56A7C0` reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitRules {
    /// `[0x9C728C]`: "Battle-Created Unit" (the Great Leader), a `PRTO`
    /// index; 47 (Leader) in the shipped rules.
    pub battle_created_unit: i32,
    /// `[0x9C725C]`: "Cities Needed to Support an Army"; 4 in the shipped
    /// rules.
    pub cities_per_army: i32,
    /// A great wonder with [`wonder_flags::ALLOWS_NUCLEAR_DEVICES`] has been
    /// built by anyone, or the player owns a small wonder with it
    /// (`0x56A8DE..0x56A9B2`).
    pub nuclear_devices_allowed: bool,
}

/// The keys `0x55A560` puts into the set at `Player +0x20D0` for the player's
/// great wonders: for every great wonder that has a "gain in every city"
/// improvement or a "gain in every city on continent" one, whose government
/// requirement (`+0xD4`) is `-1` or the player's government, that is not
/// obsolete for the player (`+0xE0` unknown or `-1`) and whose city belongs
/// to the player, the key `gain_in_every_city` (an improvement index) and the
/// key `(continent + 1) * improvements + gain_in_every_city_on_continent`,
/// `continent` being the continent of the wonder's city. `0x56A76B` rejects a
/// plain improvement index that is in the set, so the global grants are
/// unbuildable; the continental keys sit above every improvement index and
/// are for the city-level check.
pub fn grant_key(continent: Option<i32>, improvement: i32, improvements: i32) -> i32 {
    match continent {
        None => improvement,
        Some(c) => (c + 1) * improvements + improvement,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mock world; every field is the fact the trait method names.
    struct World {
        known: Vec<i32>,
        tech_count: i32,
        government: i32,
        mobilized: bool,
        owned: Vec<(i32, i32)>,
        parts_built: Vec<(i32, i32)>,
        part_limit: i32,
        space_race: bool,
        diplomatic: bool,
        apollo: bool,
        built_wonders: Vec<i32>,
        small_wonders: Vec<i32>,
        victorious: bool,
        elite_navy: bool,
        armies: i32,
        cities: i32,
        world_cities: i32,
        granted: Vec<i32>,
        producing: Vec<i32>,
        parts_producing: i32,
    }

    impl Default for World {
        fn default() -> Self {
            World {
                known: vec![],
                tech_count: 83,
                government: 1,
                mobilized: false,
                owned: vec![],
                parts_built: vec![],
                part_limit: 1,
                space_race: true,
                diplomatic: true,
                apollo: true,
                built_wonders: vec![],
                small_wonders: vec![],
                victorious: false,
                elite_navy: false,
                armies: 0,
                cities: 10,
                world_cities: 20,
                granted: vec![],
                producing: vec![],
                parts_producing: 0,
            }
        }
    }

    impl Realm for World {
        fn knows(&self, tech: i32) -> bool {
            self.known.contains(&tech)
        }
        fn tech_count(&self) -> i32 {
            self.tech_count
        }
        fn government(&self) -> i32 {
            self.government
        }
        fn mobilized(&self) -> bool {
            self.mobilized
        }
        fn improvements_owned(&self, building: i32) -> i32 {
            self.owned
                .iter()
                .find(|(b, _)| *b == building)
                .map_or(0, |(_, n)| *n)
        }
        fn parts_built(&self, part: i32) -> i32 {
            self.parts_built
                .iter()
                .find(|(p, _)| *p == part)
                .map_or(0, |(_, n)| *n)
        }
        fn part_limit(&self, _part: i32) -> i32 {
            self.part_limit
        }
        fn space_race(&self) -> bool {
            self.space_race
        }
        fn diplomatic_victory(&self) -> bool {
            self.diplomatic
        }
        fn owns_spaceship_wonder(&self) -> bool {
            self.apollo
        }
        fn great_wonder_built(&self, building: i32) -> bool {
            self.built_wonders.contains(&building)
        }
        fn owns_small_wonder(&self, building: i32) -> bool {
            self.small_wonders.contains(&building)
        }
        fn victorious_army(&self) -> bool {
            self.victorious
        }
        fn elite_navy(&self) -> bool {
            self.elite_navy
        }
        fn armies(&self) -> i32 {
            self.armies
        }
        fn cities(&self) -> i32 {
            self.cities
        }
        fn world_size_cities(&self) -> i32 {
            self.world_cities
        }
        fn granted_by_wonder(&self, building: i32) -> bool {
            self.granted.contains(&building)
        }
        fn in_production(&self, building: i32) -> bool {
            self.producing.contains(&building)
        }
        fn parts_in_production(&self, _part: i32) -> i32 {
            self.parts_producing
        }
    }

    const PLAIN: Improvement = Improvement {
        required_improvement: -1,
        num_buildings_required: 1,
        required_government: -1,
        spaceship_part: -1,
        required_advance: -1,
        improvement_flags: 0,
        characteristics: 0,
        small_wonder_flags: 0,
        wonder_flags: 0,
        armies_required: 0,
    };

    fn can(w: &World, b: &Improvement) -> bool {
        can_build_improvement(w, 7, b, false)
    }

    #[test]
    fn a_plain_building_needs_only_its_technology() {
        let w = World::default();
        assert!(can(&w, &PLAIN));
        let b = Improvement {
            required_advance: 12,
            ..PLAIN
        };
        assert!(!can(&w, &b));
        let w = World {
            known: vec![12],
            ..World::default()
        };
        assert!(can(&w, &b));
        // The technology count means "no technology grants it".
        let never = Improvement {
            required_advance: 83,
            ..PLAIN
        };
        assert!(!can(&w, &never));
    }

    #[test]
    fn wall_street_wants_five_of_its_prerequisite() {
        let b = Improvement {
            required_improvement: 3,
            num_buildings_required: 5,
            ..PLAIN
        };
        let few = World {
            owned: vec![(3, 4)],
            ..World::default()
        };
        let enough = World {
            owned: vec![(3, 5)],
            ..World::default()
        };
        assert!(!can(&few, &b));
        assert!(can(&enough, &b));
        // With a count of 1 the prerequisite is a city-level matter and the
        // player-level check ignores it, even when the player owns none.
        let bank = Improvement {
            required_improvement: 3,
            ..PLAIN
        };
        assert!(can(&World::default(), &bank));
    }

    #[test]
    fn a_government_requirement_is_exact() {
        let b = Improvement {
            required_government: 3,
            ..PLAIN
        };
        assert!(!can(&World::default(), &b));
        let w = World {
            government: 3,
            ..World::default()
        };
        assert!(can(&w, &b));
    }

    #[test]
    fn a_great_wonder_exists_once_and_a_small_wonder_once_per_civ() {
        let great = Improvement {
            characteristics: characteristics::WONDER,
            ..PLAIN
        };
        let small = Improvement {
            characteristics: characteristics::SMALL_WONDER,
            ..PLAIN
        };
        let built = World {
            built_wonders: vec![7],
            small_wonders: vec![7],
            ..World::default()
        };
        assert!(!can(&built, &great));
        assert!(!can(&built, &small));
        assert!(can(&World::default(), &great));
        assert!(can(&World::default(), &small));
        // The flags are independent: a plain building ignores both lists.
        assert!(can(&built, &PLAIN));
    }

    #[test]
    fn strict_mode_forbids_a_second_wonder_in_production() {
        let wonder = Improvement {
            characteristics: characteristics::WONDER,
            ..PLAIN
        };
        let w = World {
            producing: vec![7],
            ..World::default()
        };
        assert!(can_build_improvement(&w, 7, &wonder, false));
        assert!(!can_build_improvement(&w, 7, &wonder, true));
        // An ordinary building may be built in many cities at once.
        assert!(can_build_improvement(&w, 7, &PLAIN, true));
        // The Palace counts as unique; Wealth never does, even as a wonder.
        let palace = Improvement {
            improvement_flags: improvement_flags::CENTER_OF_EMPIRE,
            ..PLAIN
        };
        assert!(!can_build_improvement(&w, 7, &palace, true));
        let wealth = Improvement {
            improvement_flags: improvement_flags::CAPITALIZATION
                | improvement_flags::CENTER_OF_EMPIRE,
            ..PLAIN
        };
        assert!(can_build_improvement(&w, 7, &wealth, true));
    }

    #[test]
    fn spaceship_parts_need_the_race_the_wonder_and_room() {
        let part = Improvement {
            spaceship_part: 1,
            ..PLAIN
        };
        let w = World::default();
        assert!(can(&w, &part));
        let no_race = World {
            space_race: false,
            ..World::default()
        };
        assert!(!can(&no_race, &part));
        let no_apollo = World {
            apollo: false,
            ..World::default()
        };
        assert!(!can(&no_apollo, &part));
        let full = World {
            parts_built: vec![(1, 1)],
            ..World::default()
        };
        assert!(!can(&full, &part));
        // A part of another type does not count against this one.
        let other = World {
            parts_built: vec![(2, 1)],
            ..World::default()
        };
        assert!(can(&other, &part));
        // Strict: the parts under way count against the limit, and the test
        // is for equality, as in the binary (0x56A584 `jne`).
        let busy = World {
            parts_producing: 1,
            ..World::default()
        };
        assert!(can_build_improvement(&busy, 7, &part, false));
        assert!(!can_build_improvement(&busy, 7, &part, true));
        let over = World {
            parts_producing: 2,
            ..World::default()
        };
        assert!(can_build_improvement(&over, 7, &part, true));
    }

    #[test]
    fn the_united_nations_needs_the_diplomatic_victory() {
        let un = Improvement {
            characteristics: characteristics::WONDER,
            wonder_flags: wonder_flags::ALLOWS_DIPLOMATIC_VICTORY,
            ..PLAIN
        };
        assert!(can(&World::default(), &un));
        let off = World {
            diplomatic: false,
            ..World::default()
        };
        assert!(!can(&off, &un));
    }

    #[test]
    fn army_conditions() {
        let epic = Improvement {
            characteristics: characteristics::SMALL_WONDER,
            small_wonder_flags: small_wonder_flags::REQUIRES_VICTORIOUS_ARMY,
            ..PLAIN
        };
        assert!(!can(&World::default(), &epic));
        let won = World {
            victorious: true,
            ..World::default()
        };
        assert!(can(&won, &epic));
        let navy = Improvement {
            small_wonder_flags: small_wonder_flags::REQUIRES_ELITE_NAVAL_UNITS,
            ..PLAIN
        };
        assert!(!can(&won, &navy));
        let ships = World {
            elite_navy: true,
            ..World::default()
        };
        assert!(can(&ships, &navy));
        // The Pentagon wants three armies.
        let pentagon = Improvement {
            armies_required: 3,
            ..PLAIN
        };
        let two = World {
            armies: 2,
            ..World::default()
        };
        let three = World {
            armies: 3,
            ..World::default()
        };
        assert!(!can(&two, &pentagon));
        assert!(can(&three, &pentagon));
    }

    #[test]
    fn corruption_wonders_need_half_the_worlds_optimal_cities() {
        let fp = Improvement {
            characteristics: characteristics::SMALL_WONDER,
            small_wonder_flags: small_wonder_flags::REDUCES_CORRUPTION,
            ..PLAIN
        };
        // Standard world: 20 optimal cities, so 10 are enough, 9 are not.
        let nine = World {
            cities: 9,
            ..World::default()
        };
        let ten = World {
            cities: 10,
            ..World::default()
        };
        assert!(!can(&nine, &fp));
        assert!(can(&ten, &fp));
        // An odd base halves downwards: 17 -> 8.
        let tiny = World {
            cities: 8,
            world_cities: 17,
            ..World::default()
        };
        assert!(can(&tiny, &fp));
    }

    #[test]
    fn mobilization_allows_only_the_exempt() {
        let w = World {
            mobilized: true,
            ..World::default()
        };
        assert!(!can(&w, &PLAIN));
        let barracks = Improvement {
            characteristics: characteristics::MILITARISTIC,
            ..PLAIN
        };
        assert!(can(&w, &barracks));
        let wealth = Improvement {
            improvement_flags: improvement_flags::CAPITALIZATION,
            ..PLAIN
        };
        assert!(can(&w, &wealth));
        let un = Improvement {
            wonder_flags: wonder_flags::ALLOWS_DIPLOMATIC_VICTORY,
            ..PLAIN
        };
        assert!(can(&w, &un));
        let part = Improvement {
            spaceship_part: 0,
            ..PLAIN
        };
        assert!(can(&w, &part));
        // A wonder is a peacetime improvement like any other.
        let wonder = Improvement {
            characteristics: characteristics::WONDER,
            ..PLAIN
        };
        assert!(!can(&w, &wonder));
    }

    #[test]
    fn what_a_wonder_supplies_everywhere_cannot_be_built() {
        let w = World {
            granted: vec![7],
            ..World::default()
        };
        assert!(!can(&w, &PLAIN));
        assert!(can_build_improvement(&w, 8, &PLAIN, false));
        // Keys for a continent sit above every improvement index.
        assert_eq!(grant_key(None, 5, 70), 5);
        assert_eq!(grant_key(Some(0), 5, 70), 75);
        assert_eq!(grant_key(Some(2), 5, 70), 215);
    }

    /// The shipped rules: the Leader is prototype 47, an army needs 4 cities.
    const RULES: UnitRules = UnitRules {
        battle_created_unit: 47,
        cities_per_army: 4,
        nuclear_devices_allowed: false,
    };

    fn unit_ok(w: &World, u: &UnitType) -> bool {
        can_build_unit(w, 10, u, true, 3, false, &RULES)
    }

    #[test]
    fn a_unit_needs_its_civ_and_its_technology() {
        let w = World::default();
        let u = UnitType::default();
        assert!(unit_ok(&w, &u));
        let only_race_3 = UnitType {
            available_to_civs: 1 << 3,
            ..u
        };
        assert!(unit_ok(&w, &only_race_3));
        let other_race = UnitType {
            available_to_civs: 1 << 4,
            ..u
        };
        assert!(!unit_ok(&w, &other_race));
        let tech = UnitType {
            required_tech: 5,
            ..u
        };
        assert!(!unit_ok(&w, &tech));
        let w = World {
            known: vec![5],
            ..World::default()
        };
        assert!(unit_ok(&w, &tech));
    }

    #[test]
    fn humans_cannot_build_ai_alternates_or_kings_or_the_leader() {
        let w = World::default();
        let alt = UnitType {
            alt_strategy_of: 16,
            ..UnitType::default()
        };
        assert!(!unit_ok(&w, &alt));
        assert!(can_build_unit(&w, 10, &alt, false, 3, false, &RULES));
        let king = UnitType {
            king: true,
            ..UnitType::default()
        };
        assert!(!unit_ok(&w, &king));
        assert!(can_build_unit(&w, 10, &king, true, 3, true, &RULES));
        // The Great Leader: never, for anyone.
        assert!(!can_build_unit(
            &w,
            47,
            &UnitType::default(),
            false,
            3,
            true,
            &RULES
        ));
    }

    #[test]
    fn an_army_needs_four_cities_for_each_one_owned_plus_one() {
        let army = UnitType {
            army: true,
            ..UnitType::default()
        };
        let enough = |armies, cities| World {
            armies,
            cities,
            ..World::default()
        };
        assert!(!unit_ok(&enough(0, 3), &army));
        assert!(unit_ok(&enough(0, 4), &army));
        assert!(!unit_ok(&enough(1, 7), &army));
        assert!(unit_ok(&enough(1, 8), &army));
    }

    #[test]
    fn nuclear_weapons_wait_for_the_manhattan_project() {
        let nuke = UnitType {
            nuclear: true,
            ..UnitType::default()
        };
        let w = World::default();
        assert!(!can_build_unit(&w, 10, &nuke, true, 3, false, &RULES));
        let built = UnitRules {
            nuclear_devices_allowed: true,
            ..RULES
        };
        assert!(can_build_unit(&w, 10, &nuke, true, 3, false, &built));
        assert!(can_build_unit(
            &w,
            10,
            &UnitType::default(),
            true,
            3,
            false,
            &RULES
        ));
    }
}
