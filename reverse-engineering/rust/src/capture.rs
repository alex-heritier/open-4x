//! City capture and transfer: `Player::capture` (`0x563410`), the transfer
//! itself (`Player::takeCity`, `0x564800`) and razing (`0x563370`).
//!
//! Findings, addresses and the open list are in `../capture.md`. This module
//! holds the parts that are pure arithmetic or pure decisions, including the
//! AI's two choices ([`ai_accepts_city`], [`ai_razes_city`]) over facts the
//! caller reads from the world; everything that touches the world (units,
//! tiles, message dialogs) is described in the markdown only.
//!
//! The pieces, in the order the game runs them for a military capture
//! (`Flags { capture: true, convert: false }`):
//!
//! 1. [`plunder`] and the two treasury writes ([`victim_cells`],
//!    [`capturer_cells`]), `0x563F54..0x56409E`.
//! 2. [`capture_loss`]: the city shrinks by one citizen or is destroyed
//!    outright, `0x5640A1..0x5642F1`.
//! 3. The keep-or-raze choice (a dialog for a local human, [`ai_razes_city`]
//!    for the AI), then either [`raze_workers`] and the city's removal
//!    (`0x563370`) or the transfer.
//! 4. [`surviving_buildings`]: which buildings the transfer destroys,
//!    `0x56492B..0x564A91`.
//!
//! A culture flip (`convert` set) asks the converting player first; for an
//! AI that answer is [`ai_accepts_city`].
//!
//! The barbarian raid (`Player::capture` with `this` = civ 0) is a separate
//! branch that never transfers the city: [`raid_loss`], [`raid_loot`].

use crate::economy::{size_class, treasury_cells, CITY_MAX, TOWN_MAX};
use crate::rng::Rng;

/// The two boolean stack arguments of `Player::capture` (`0x563410`,
/// arguments 3 and 4 after the city and the capturing unit).
///
/// Call sites: units entering a city pass `(true, false)` (`0x469B3F`,
/// `0x476C76`, `0x5B9D81`, `0x5C4D47`), the culture flip passes
/// `(true, true)` (`0x4B2DC0`, `0x5281D3`), the one silent transfer passes
/// `(false, false)` (`0x5034B9`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Flags {
    /// Argument 3: the old owner's units on the city tile are killed, and in
    /// the transfer the buildings may be destroyed by chance
    /// ([`surviving_buildings`]).
    pub capture: bool,
    /// Argument 4: a culture conversion; it overrides the plunder branch.
    pub convert: bool,
}

/// Which part of `0x563410` runs for a [`Flags`] pair (`0x563916`,
/// `0x563E8E`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Path {
    /// `(false, false)`: straight to the transfer `0x564800`.
    Silent,
    /// `(true, false)`: plunder, population loss, keep or raze (`0x563E9A`).
    Plunder,
    /// `convert` set: the accept-or-rebuff question (`0x563922`).
    Conversion,
}

impl Flags {
    /// The branch of `0x563410` these flags select.
    pub fn path(self) -> Path {
        if self.convert {
            Path::Conversion
        } else if self.capture {
            Path::Plunder
        } else {
            Path::Silent
        }
    }

    /// The argument of `0x4BCDE0(city, n)` near the end of the transfer
    /// (`0x564F8F..0x564FAA`): 1 for a plain capture, 10 otherwise. The
    /// callee stores it in `city +0x58`; what that field counts is open
    /// (**HYPOTHESIS**: a post-transfer timer).
    pub fn post_transfer_timer(self) -> i32 {
        if self.capture && !self.convert {
            1
        } else {
            10
        }
    }
}

/// Incident weight added to the victim's pair record about the capturer
/// (`+0x20` and `+0x24`, see `combat.md` section 14.4) for every military
/// capture from a real civ (`0x563EC7`, `0x563EE3`).
pub const CAPTURE_INCIDENT: i32 = 16;

/// Extra incident weight when the capture also kills a citizen
/// (`0x5642CD`, `0x5642E7`).
pub const SHRINK_INCIDENT: i32 = 1;

/// Gold taken in a military capture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plunder {
    /// The loot before the unit bonus; what the victim loses.
    pub base: i32,
    /// What the capturer gains: `base`, doubled when the capturing unit
    /// carries the bonus flag.
    pub gain: i32,
}

/// The per-city share scaled by the city's size class (`0x563FA0..0x563FE3`):
/// a town pays half, a city three quarters, a metropolis the whole share.
/// Both divisions truncate toward zero (`cdq` / `sub` / `sar`).
pub fn scale_share(share: i32, class: i32) -> i32 {
    match class {
        0 => share / 2,
        1 => share.wrapping_mul(3) / 4,
        _ => share,
    }
}

/// The gold a capture takes (`0x563F54..0x564012`).
///
/// `treasury` is the victim's two treasury cells summed, `cities` its city
/// count (`Player +0x194`, at least 1 because the captured city is one of
/// them; 0 would be the original's divide error), `size` the city's
/// population. The victim's last city yields the whole treasury; otherwise
/// the loot is `treasury / cities` scaled by [`scale_share`]. `bonus_unit` is
/// the capturing unit's special-action word `+0xAC & 0x10080000`; no shipped
/// unit has either bit, and the code that tests it (`0x563FE7`) is skipped
/// for a last city, so the bonus never applies there.
pub fn plunder(treasury: i32, cities: i32, size: i32, bonus_unit: bool) -> Plunder {
    if cities == 1 {
        return Plunder { base: treasury, gain: treasury };
    }
    let base = scale_share(treasury / cities, size_class(size, TOWN_MAX, CITY_MAX));
    Plunder {
        base,
        gain: if bonus_unit { base.wrapping_add(base) } else { base },
    }
}

/// The victim's treasury cells after the capture (`0x564044..0x564095`):
/// `max(0, total - base)`, split from the clock like every treasury write
/// (`economy::treasury_cells`).
pub fn victim_cells(total: i32, p: Plunder, clock: u32) -> (i32, i32) {
    treasury_cells(total.wrapping_sub(p.base).max(0), clock)
}

/// The capturer's treasury cells after the capture (`0x564012..0x564044`):
/// `total + gain`; a result at or below 0 is stored as a sum of 0.
pub fn capturer_cells(total: i32, p: Plunder, clock: u32) -> (i32, i32) {
    treasury_cells(total.wrapping_add(p.gain), clock)
}

/// What the capture does to the city's population (`0x5640A1..0x5642F1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Loss {
    /// The city keeps its size.
    None,
    /// One citizen of any race dies (`0x4BA230(city, 1, -1, 0)`).
    Shrink,
    /// The city is destroyed (`0x4AECC0`) and the keep-or-raze question
    /// is skipped.
    Destroy,
}

/// The facts [`capture_loss`] reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CityFacts {
    /// `city +0x138`.
    pub size: i32,
    /// `0x4BB410(city, capturer race)`: citizens of the capturer's race.
    pub capturer_race_citizens: i32,
    /// `city +0x140 + 4 * capturer civ`: the capturer's per-civ value
    /// (**HYPOTHESIS**: its culture in the city); the transfer copies the
    /// old owner's value to the new owner clamped at 0.
    pub capturer_stake: i32,
    /// `0x4BB410(city, old owner race)`: citizens of the old owner's race.
    pub owner_race_citizens: i32,
    /// `city +0x5C`: 1 when the city was founded (`0x4AE40E`) and when the
    /// transfer resets it (`0x4B0C60`); meaning open.
    pub style: i32,
}

/// The population loss of a capture (`0x5640A1`).
///
/// Nothing is lost when the city already holds a citizen of the capturer's
/// race, or the capturer has a non-zero stake in it. Otherwise a city of
/// size 1 is destroyed if its one citizen is of the old owner's race and
/// `style == 1`, and kept at size 1 if not; a larger city loses one citizen.
pub fn capture_loss(c: &CityFacts) -> Loss {
    if c.capturer_race_citizens != 0 || c.capturer_stake != 0 {
        Loss::None
    } else if c.size == 1 {
        if c.owner_race_citizens == 1 && c.style == 1 {
            Loss::Destroy
        } else {
            Loss::None
        }
    } else {
        Loss::Shrink
    }
}

/// Workers released when a city is razed (`0x563370`): `size / 2` units of
/// the RULE worker prototype `[0x9C72C4]` (shipped 1, `Worker`), created for
/// the capturer at the city's tile, tagged with the old owner's race id.
pub fn raze_workers(size: i32) -> i32 {
    size / 2
}

/// What the capture code reads about one BLDG row.
///
/// Built from three dwords of the in-memory row (`0x9C40AC`, stride
/// `0x110`): `+0xEC` (the file's body offset `+0xE8`), `+0xF0` (body
/// `+0xEC`) and `+0x98` (body `+0x94`). The in-memory row is the file body
/// shifted by 4.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuildingClass {
    /// `+0xEC & 1`: Center of Empire, the Palace.
    pub center_of_empire: bool,
    /// `+0xF0 & 4`: a great wonder.
    pub great_wonder: bool,
    /// `+0xF0 & 8`: a small wonder.
    pub small_wonder: bool,
    /// `+0xEC & 0x1800`: the two city-size gates (shipped `Aqueduct`
    /// `0x800`, `Hospital` `0x1000`).
    pub size_gate: bool,
    /// `+0x98`: culture per turn.
    pub culture: i32,
}

impl BuildingClass {
    /// Decodes the three dwords.
    pub fn from_words(flags_ec: u32, flags_f0: u32, culture: i32) -> Self {
        BuildingClass {
            center_of_empire: flags_ec & 1 != 0,
            great_wonder: flags_f0 & 4 != 0,
            small_wonder: flags_f0 & 8 != 0,
            size_gate: flags_ec & 0x1800 != 0,
            culture,
        }
    }

    /// `0x4B3290` minus the presence test: not a wonder, not the palace, not
    /// a size gate.
    pub fn is_ordinary(&self) -> bool {
        !(self.great_wonder || self.small_wonder || self.center_of_empire || self.size_gate)
    }
}

/// The buildings that survive a transfer, in BLDG index order.
///
/// `held` lists `(BLDG index, class)` for every building the city holds,
/// ascending by index. The game runs four passes over the BLDG table
/// (`0x564800`):
///
/// * **A** (`0x56492B`), only when the city is the old owner's capital:
///   buildings with Center of Empire are destroyed.
/// * **B** (`0x564985`), always: every ordinary building with culture above
///   0 is destroyed (`0x4B3290` and `+0x98 > 0`).
/// * **C** (`0x564A05`), only for a capture that is not a conversion, and
///   only when the city holds no citizen of the capturer's race: each
///   remaining ordinary building is destroyed when `next(4) == 0` on the
///   gameplay `Random` (one draw per building, in index order).
/// * **D** (`0x564A47`), always: small wonders are destroyed.
///
/// Great wonders, size gates and every building that none of the passes
/// names survive.
pub fn surviving_buildings(
    held: &[(usize, BuildingClass)],
    capital_lost: bool,
    flags: Flags,
    capturer_has_race_citizen: bool,
    rng: &mut Rng,
) -> Vec<usize> {
    let mut alive: Vec<(usize, BuildingClass)> = held.to_vec();
    if capital_lost {
        alive.retain(|(_, b)| !b.center_of_empire);
    }
    alive.retain(|(_, b)| !(b.is_ordinary() && b.culture > 0));
    if flags.capture && !flags.convert && !capturer_has_race_citizen {
        alive.retain(|(_, b)| !(b.is_ordinary() && rng.one_in(4)));
    }
    alive.retain(|(_, b)| !b.small_wonder);
    alive.into_iter().map(|(i, _)| i).collect()
}

/// The loss a barbarian raid inflicts (`combat.md` section 14.2), in the
/// order the game tests them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RaidLoss {
    /// The building with the best land bombard defense is removed
    /// (`0x4C1320(city, 0)`).
    Walls,
    /// The shield stock is set to 0 (`0x4ACD70` is the item cost, 0 if
    /// none).
    Stock,
    /// One citizen of the owner's race dies (`0x4BA230`).
    Citizen,
    /// Gold: [`raid_loot`].
    Gold,
}

/// The first loss that applies to a raided city (`0x563495..0x5637C9`).
///
/// `town_with_walls`: size class 0 (`0x427540`) and `0x4C0C70 > 0`;
/// `shield_stock`: `city +0x44`; `owner_race_citizens`:
/// `0x4BB410(city, owner race)`; `treasury`: the owner's two cells summed.
pub fn raid_loss(
    town_with_walls: bool,
    shield_stock: i32,
    owner_race_citizens: i32,
    treasury: i32,
) -> RaidLoss {
    if town_with_walls {
        RaidLoss::Walls
    } else if shield_stock > 10 {
        RaidLoss::Stock
    } else if owner_race_citizens > 1 {
        RaidLoss::Citizen
    } else if treasury != 0 {
        RaidLoss::Gold
    } else {
        RaidLoss::Stock
    }
}

/// The gold a barbarian raid takes (`0x56369C..0x563788`): `treasury /
/// cities`, and when that is 0, 1 if the treasury exceeds 1, else the whole
/// treasury. The barbarians do not receive it.
pub fn raid_loot(treasury: i32, cities: i32) -> i32 {
    let loot = treasury / cities;
    if loot != 0 {
        loot
    } else if treasury > 1 {
        1
    } else {
        treasury
    }
}

// ---------------------------------------------------------------------
// The AI's two decisions: `Player.vtable[+0x14]` and `[+0x18]`
// ---------------------------------------------------------------------

/// Is a great wonder active for a civ? The test both AI decisions share
/// (`0x443AB8..0x443AE8`, `0x443D25..0x443D55`, `0x443DF3..0x443E29`):
/// BLDG `+0xD4` (required government, `-1` none) is `-1` or the civ's
/// government (`Player +0xA0`), and BLDG `+0xE0` (the tech that makes the
/// wonder obsolete, `-1` none) is `-1` or unknown to the civ
/// (`Player::hasTech`, `0x561440`).
pub fn wonder_active(
    required_government: i32,
    government: i32,
    obsolete_by: i32,
    knows_obsoleting_tech: bool,
) -> bool {
    (required_government == -1 || required_government == government)
        && (obsolete_by == -1 || !knows_obsoleting_tech)
}

/// One great wonder (BLDG `+0xF0 & 4`) standing in the captured city
/// (`0x4ACB50(city, b, 0)` is true), as the deciding AI player sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wonder {
    /// [`wonder_active`] for the deciding player.
    pub active: bool,
    /// The wonder's builder (`[city +0xC4]`'s 12-byte record `[+4]`) is the
    /// deciding player's civ.
    pub built_by_me: bool,
}

impl Wonder {
    /// Active, or built by the deciding player: either makes the AI keep
    /// the city in both decisions.
    pub fn is_wanted(self) -> bool {
        self.active || self.built_by_me
    }
}

/// Everything `Player.vtable[+0x14]` (`0x443A60`, `ret 4`) reads.
///
/// `this` is the player the city flips to; the argument is the city.
#[derive(Clone, Copy, Debug)]
pub struct AcceptFacts<'a> {
    /// `Player +0xD30[old owner]` is non-zero: the player is at war with the
    /// city's owner.
    pub at_war_with_owner: bool,
    /// The great wonders in the city.
    pub wonders: &'a [Wonder],
    /// `0x4BB410(city; player race)`: the city's citizens of the player's
    /// race.
    pub my_race_citizens: i32,
    /// `Player +0x194`.
    pub city_count: i32,
    /// [`crate::economy::optimal_city_number`] of the player.
    pub ocn: i32,
}

/// Does the AI take a city that culture has flipped to it? (`0x443A60`;
/// the conversion branch of `Player::capture`, `0x563BE6`: a non-zero
/// result converts, zero tells the old owner `THEY_REFUSED_TO_CONVERT_CITY`
/// and counts a refusal in the pair record's `+0x28`.)
///
/// In order:
///
/// 1. at war with the city's owner: yes (the flip hurts the enemy);
/// 2. a great wonder that is [active](Wonder::active) or built by the
///    player: yes;
/// 3. at least one citizen of the player's race: yes;
/// 4. otherwise only while the player has fewer than twice its optimal
///    city number of cities.
///
/// There is no random draw. In the shipped game 4 is nearly always true,
/// so a flip is refused only by a player that is over the limit and
/// has no wonder, war or citizen reason to want the city.
pub fn ai_accepts_city(f: &AcceptFacts) -> bool {
    f.at_war_with_owner
        || f.wonders.iter().any(|w| w.is_wanted())
        || f.my_race_citizens > 0
        || f.city_count < 2 * f.ocn
}

/// `GOOD` class predicate `0x5E3700`: luxury (1) or strategic (2), not a
/// bonus resource (0).
pub fn is_scarce_good(class: i32) -> bool {
    class == 1 || class == 2
}

/// Everything `Player.vtable[+0x18]` (`0x443B60`, `ret 4`) reads.
///
/// `this` is the capturing player `P`; the argument is the captured city
/// (still owned by `O`; the loss of a citizen has already been applied).
#[derive(Clone, Copy, Debug)]
pub struct RazeFacts<'a> {
    /// `O`'s city count (`Player +0x194`).
    pub owner_cities: i32,
    /// `O`'s plus `P`'s cities on the city's continent (`Player
    /// +0x1610[continent]`, the continent from the cell slot `0xB8`).
    pub cities_on_continent: i32,
    /// The scan of the city's 21 radius tiles (`0x5E6E50`, `0x443C29..
    /// 0x443CDE`) found a luxury or strategic resource ([`is_scarce_good`])
    /// that `0x4ADE30(capital, good)` says `P`'s capital cannot reach. The
    /// first such tile ends the scan. False when `P` has no capital (the
    /// scan is skipped).
    pub capital_lacks_good: bool,
    /// The great wonders in the city.
    pub wonders: &'a [Wonder],
    /// City size (`+0x138`).
    pub size: i32,
    /// `0x4BB410(city; P's race)`.
    pub my_race_citizens: i32,
    /// `0x4BB410(city; O's race)`.
    pub owner_race_citizens: i32,
    /// `[city +0x140 + 4 * P.civ]`: `P`'s culture stake in the city.
    pub my_stake: i32,
    /// `[city +0x140 + 4 * O]`: `O`'s stake.
    pub owner_stake: i32,
    /// `Player +0x183C` of `P`. Written only by the player constructor
    /// (`0x539990`, to 0), so zero in play.
    pub my_rating: i32,
    /// `Player +0x183C` of `O`.
    pub owner_rating: i32,
    /// `P`'s city count.
    pub my_cities: i32,
    /// [`crate::economy::optimal_city_number`] of `P`.
    pub ocn: i32,
}

/// Does the AI raze the city it has just captured? (`0x443B60`; the
/// keep-or-raze choice of `Player::capture`, `0x564558`: a non-zero result
/// destroys the city, section 9 of `../capture.md`.)
///
/// Rules, in the order the code applies them; the first that fires wins:
///
/// 1. `O`'s last city: no.
/// 2. The only city of either player on its continent: no.
/// 3. A luxury or strategic resource in the radius that the capital
///    cannot reach: no.
/// 4. A wonder that is active for `P` or built by `P`: no.
/// 5. At least `(size + 1) / 2` of the citizens are of `P`'s race: no.
/// 6. Any great wonder at all (rule 4 has already excluded the useful
///    ones): **yes**.
/// 7. `O` holds more culture in the city than `P` **and** at least
///    `(size + 1) / 2` of the citizens are of `O`'s race: yes if
///    `O`'s rating exceeds twice `P`'s (dead: both are 0), else yes if
///    `owner_race_citizens - my_race_citizens >= next(15) + next(15)`.
///    The two draws are made only when this branch gets past the rating
///    test.
/// 8. Otherwise yes exactly when `P` has at least twice its optimal city
///    number of cities.
pub fn ai_razes_city(f: &RazeFacts, rng: &mut Rng) -> bool {
    if f.owner_cities == 1 || f.cities_on_continent == 1 || f.capital_lacks_good {
        return false;
    }
    if f.wonders.iter().any(|w| w.is_wanted()) {
        return false;
    }
    let half = (f.size + 1) / 2;
    if f.my_race_citizens >= half {
        return false;
    }
    if f.wonders.iter().any(|w| !w.is_wanted()) {
        return true;
    }
    if f.owner_stake > f.my_stake && f.owner_race_citizens >= half {
        if f.owner_rating > f.my_rating.wrapping_add(f.my_rating) {
            return true;
        }
        let draws = rng.below(15) + rng.below(15);
        if f.owner_race_citizens - f.my_race_citizens >= draws {
            return true;
        }
    }
    f.my_cities >= 2 * f.ocn
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: Flags = Flags { capture: true, convert: false };

    #[test]
    fn flag_pairs_select_the_documented_branch() {
        assert_eq!(Flags { capture: false, convert: false }.path(), Path::Silent);
        assert_eq!(PLAIN.path(), Path::Plunder);
        assert_eq!(Flags { capture: true, convert: true }.path(), Path::Conversion);
        assert_eq!(Flags { capture: false, convert: true }.path(), Path::Conversion);
    }

    #[test]
    fn post_transfer_timer_is_one_only_for_a_plain_capture() {
        assert_eq!(PLAIN.post_transfer_timer(), 1);
        assert_eq!(Flags { capture: true, convert: true }.post_transfer_timer(), 10);
        assert_eq!(Flags { capture: false, convert: false }.post_transfer_timer(), 10);
    }

    #[test]
    fn last_city_yields_the_whole_treasury_at_any_size() {
        for size in [1, 6, 7, 12, 13, 30] {
            let p = plunder(1234, 1, size, false);
            assert_eq!((p.base, p.gain), (1234, 1234));
        }
    }

    #[test]
    fn share_scales_with_the_size_class() {
        // 1000 gold over 4 cities: 250 each.
        assert_eq!(plunder(1000, 4, 6, false).base, 125); // town: half
        assert_eq!(plunder(1000, 4, 7, false).base, 187); // city: 3/4, truncated
        assert_eq!(plunder(1000, 4, 12, false).base, 187);
        assert_eq!(plunder(1000, 4, 13, false).base, 250); // metropolis: all
    }

    #[test]
    fn scaling_truncates_toward_zero_on_negative_shares() {
        assert_eq!(scale_share(-3, 0), -1);
        assert_eq!(scale_share(-3, 1), -2); // -9 / 4
        assert_eq!(scale_share(-3, 2), -3);
        // -7 gold over 2 cities: share -3 (idiv truncates), then scaled.
        assert_eq!(plunder(-7, 2, 3, false).base, -1);
    }

    #[test]
    fn bonus_unit_doubles_the_gain_but_not_the_loss() {
        let p = plunder(1000, 4, 13, true);
        assert_eq!((p.base, p.gain), (250, 500));
    }

    #[test]
    fn bonus_never_applies_to_a_last_city() {
        let p = plunder(1000, 1, 13, true);
        assert_eq!((p.base, p.gain), (1000, 1000));
    }

    #[test]
    fn treasury_writes_conserve_the_sums() {
        let p = plunder(1000, 4, 13, false);
        let (a, b) = victim_cells(1000, p, 987_654);
        assert_eq!(a + b, 750);
        let (a, b) = capturer_cells(40, p, 987_654);
        assert_eq!(a + b, 290);
    }

    #[test]
    fn victim_never_ends_below_zero_and_capturer_debt_is_wiped() {
        let p = Plunder { base: 500, gain: 500 };
        let (a, b) = victim_cells(100, p, 55_555);
        assert_eq!(a + b, 0);
        // A capturer 600 in debt still ends at a stored sum of 0.
        let (a, b) = capturer_cells(-600, p, 55_555);
        assert_eq!(a + b, 0);
    }

    #[test]
    fn loss_needs_a_foreign_city_with_no_stake() {
        let base = CityFacts {
            size: 5,
            capturer_race_citizens: 0,
            capturer_stake: 0,
            owner_race_citizens: 5,
            style: 1,
        };
        assert_eq!(capture_loss(&base), Loss::Shrink);
        assert_eq!(
            capture_loss(&CityFacts { capturer_race_citizens: 1, ..base }),
            Loss::None
        );
        assert_eq!(capture_loss(&CityFacts { capturer_stake: 3, ..base }), Loss::None);
    }

    #[test]
    fn size_one_city_is_destroyed_only_under_all_four_conditions() {
        let one = CityFacts {
            size: 1,
            capturer_race_citizens: 0,
            capturer_stake: 0,
            owner_race_citizens: 1,
            style: 1,
        };
        assert_eq!(capture_loss(&one), Loss::Destroy);
        assert_eq!(capture_loss(&CityFacts { style: 2, ..one }), Loss::None);
        assert_eq!(capture_loss(&CityFacts { owner_race_citizens: 0, ..one }), Loss::None);
        assert_eq!(capture_loss(&CityFacts { capturer_stake: 1, ..one }), Loss::None);
        assert_eq!(capture_loss(&CityFacts { capturer_race_citizens: 1, ..one }), Loss::None);
    }

    #[test]
    fn razing_releases_half_the_population_as_workers() {
        assert_eq!(raze_workers(1), 0);
        assert_eq!(raze_workers(2), 1);
        assert_eq!(raze_workers(7), 3);
        assert_eq!(raze_workers(16), 8);
    }

    // The dwords below are the in-memory words (file body +0xE8, +0xEC and
    // +0x94) of the shipped `conquests.biq` rows.
    fn palace() -> BuildingClass {
        BuildingClass::from_words(0x1, 0x0, 1)
    }
    fn temple() -> BuildingClass {
        BuildingClass::from_words(0x0, 0x100, 2)
    }
    fn barracks() -> BuildingClass {
        BuildingClass::from_words(0x2, 0x2, 0)
    }
    fn courthouse() -> BuildingClass {
        BuildingClass::from_words(0x180, 0x0, 0)
    }
    fn aqueduct() -> BuildingClass {
        BuildingClass::from_words(0x800, 0x400, 0)
    }
    fn hospital() -> BuildingClass {
        BuildingClass::from_words(0x1000, 0x0, 0)
    }
    fn great_library() -> BuildingClass {
        BuildingClass::from_words(0x0, 0x24, 6)
    }
    fn wall_street() -> BuildingClass {
        BuildingClass::from_words(0x0, 0x8, 2)
    }

    #[test]
    fn shipped_rows_classify_as_documented() {
        assert!(palace().center_of_empire && !palace().is_ordinary());
        assert!(temple().is_ordinary() && temple().culture == 2);
        assert!(barracks().is_ordinary() && barracks().culture == 0);
        assert!(courthouse().is_ordinary());
        assert!(aqueduct().size_gate && !aqueduct().is_ordinary());
        assert!(hospital().size_gate && !hospital().is_ordinary());
        assert!(great_library().great_wonder && !great_library().is_ordinary());
        assert!(wall_street().small_wonder && !wall_street().is_ordinary());
    }

    /// The first seed whose first `below(4)` is (or is not) 0.
    fn seed_with_roll(zero: bool) -> u32 {
        (0u32..)
            .find(|&s| (Rng::new(s).below(4) == 0) == zero)
            .expect("a seed exists")
    }

    #[test]
    fn peaceful_transfer_keeps_everything_but_culture_and_small_wonders() {
        let held = [
            (0, palace()),
            (1, barracks()),
            (3, temple()),
            (6, courthouse()),
            (8, aqueduct()),
            (34, great_library()),
            (59, wall_street()),
        ];
        let silent = Flags { capture: false, convert: false };
        let mut rng = Rng::new(7);
        let before = rng;
        let kept = surviving_buildings(&held, false, silent, false, &mut rng);
        // The Palace stays (not the capital), the Temple (culture) and Wall
        // Street (small wonder) go, no draw is made.
        assert_eq!(kept, vec![0, 1, 6, 8, 34]);
        assert_eq!(rng, before);
    }

    #[test]
    fn losing_the_capital_destroys_the_palace() {
        let held = [(0, palace()), (6, courthouse())];
        let silent = Flags { capture: false, convert: false };
        let kept = surviving_buildings(&held, true, silent, false, &mut Rng::new(1));
        assert_eq!(kept, vec![6]);
    }

    #[test]
    fn capture_rolls_one_quarter_per_ordinary_building_in_index_order() {
        let held = [(1, barracks()), (6, courthouse()), (8, aqueduct())];
        // Two draws, one per ordinary building; the Aqueduct draws nothing.
        let mut probe = Rng::new(seed_with_roll(true));
        let first = probe.one_in(4);
        let second = probe.one_in(4);
        let mut rng = Rng::new(seed_with_roll(true));
        let kept = surviving_buildings(&held, false, PLAIN, false, &mut rng);
        let mut expect = Vec::new();
        if !first {
            expect.push(1);
        }
        if !second {
            expect.push(6);
        }
        expect.push(8);
        assert_eq!(kept, expect);
        assert!(first, "the chosen seed destroys the Barracks");
        // Exactly two draws were consumed.
        let mut after = Rng::new(seed_with_roll(true));
        after.discard(2);
        assert_eq!(rng, after);
    }

    #[test]
    fn a_citizen_of_the_capturers_race_or_a_conversion_skips_the_rolls() {
        let held = [(1, barracks()), (6, courthouse())];
        let seed = seed_with_roll(true);
        let mut rng = Rng::new(seed);
        let kept = surviving_buildings(&held, false, PLAIN, true, &mut rng);
        assert_eq!(kept, vec![1, 6]);
        assert_eq!(rng, Rng::new(seed));
        let convert = Flags { capture: true, convert: true };
        let mut rng = Rng::new(seed);
        let kept = surviving_buildings(&held, false, convert, false, &mut rng);
        assert_eq!(kept, vec![1, 6]);
        assert_eq!(rng, Rng::new(seed));
    }

    #[test]
    fn one_in_four_is_what_the_original_draws() {
        // next(4) is `(k * 4) >> 15`, zero for k < 8192: a quarter of the
        // 15-bit outputs.
        let zeros = (0u32..4000).filter(|&s| Rng::new(s).below(4) == 0).count();
        assert!((800..1200).contains(&zeros), "{zeros}");
    }

    #[test]
    fn raid_chain_matches_the_documented_order() {
        assert_eq!(raid_loss(true, 99, 9, 99), RaidLoss::Walls);
        assert_eq!(raid_loss(false, 11, 9, 99), RaidLoss::Stock);
        assert_eq!(raid_loss(false, 10, 2, 99), RaidLoss::Citizen);
        assert_eq!(raid_loss(false, 10, 1, 99), RaidLoss::Gold);
        assert_eq!(raid_loss(false, 0, 0, 0), RaidLoss::Stock);
        assert_eq!(raid_loss(false, 0, 0, -5), RaidLoss::Gold);
    }

    #[test]
    fn raid_loot_floors_at_one_gold_when_there_is_more_than_one() {
        assert_eq!(raid_loot(100, 4), 25);
        assert_eq!(raid_loot(3, 5), 1);
        assert_eq!(raid_loot(2, 5), 1);
        assert_eq!(raid_loot(1, 5), 1);
        assert_eq!(raid_loot(-4, 5), -4);
    }

    // ---- the AI's decisions ---------------------------------------------

    const USEFUL: Wonder = Wonder { active: true, built_by_me: false };
    const MINE_BUT_OBSOLETE: Wonder = Wonder { active: false, built_by_me: true };
    const USELESS: Wonder = Wonder { active: false, built_by_me: false };

    #[test]
    fn a_wonder_is_active_unless_the_government_or_a_tech_rules_it_out() {
        // No requirement and no obsoleting tech: always active.
        assert!(wonder_active(-1, 3, -1, true));
        // The required government must be the civ's own.
        assert!(wonder_active(3, 3, -1, false));
        assert!(!wonder_active(2, 3, -1, false));
        // Known obsoleting tech turns it off; an unknown one does not.
        assert!(!wonder_active(-1, 3, 17, true));
        assert!(wonder_active(-1, 3, 17, false));
        // Both conditions are needed.
        assert!(!wonder_active(3, 3, 17, true));
        assert!(!wonder_active(2, 3, 17, false));
    }

    fn accept(wonders: &[Wonder]) -> AcceptFacts<'_> {
        AcceptFacts {
            at_war_with_owner: false,
            wonders,
            my_race_citizens: 0,
            city_count: 40,
            ocn: 20,
        }
    }

    #[test]
    fn the_ai_refuses_a_flip_only_when_nothing_speaks_for_it_and_it_is_full() {
        // 40 cities, OCN 20: exactly at the limit of 2 * OCN.
        assert!(!ai_accepts_city(&accept(&[])));
        // One city fewer and it takes the city.
        let mut f = accept(&[]);
        f.city_count = 39;
        assert!(ai_accepts_city(&f));
        // The OCN is floored at 1, so the smallest limit is 2 cities.
        f.city_count = 1;
        f.ocn = 1;
        assert!(ai_accepts_city(&f));
        f.city_count = 2;
        assert!(!ai_accepts_city(&f));
    }

    #[test]
    fn war_wonders_and_citizens_each_force_acceptance() {
        let mut f = accept(&[]);
        f.at_war_with_owner = true;
        assert!(ai_accepts_city(&f));
        assert!(ai_accepts_city(&accept(&[USEFUL])));
        assert!(ai_accepts_city(&accept(&[MINE_BUT_OBSOLETE])));
        assert!(ai_accepts_city(&accept(&[USELESS, USEFUL])));
        assert!(!ai_accepts_city(&accept(&[USELESS])));
        let mut f = accept(&[USELESS]);
        f.my_race_citizens = 1;
        assert!(ai_accepts_city(&f));
    }

    /// A city nothing speaks for or against: the verdict is the final
    /// capacity test, which is false here (3 cities, 2 * OCN = 40).
    fn raze(wonders: &[Wonder]) -> RazeFacts<'_> {
        RazeFacts {
            owner_cities: 5,
            cities_on_continent: 6,
            capital_lacks_good: false,
            wonders,
            size: 6,
            my_race_citizens: 0,
            owner_race_citizens: 0,
            my_stake: 0,
            owner_stake: 0,
            my_rating: 0,
            owner_rating: 0,
            my_cities: 3,
            ocn: 20,
        }
    }

    /// Runs the decision and reports (verdict, draws made).
    fn razes(f: &RazeFacts) -> (bool, usize) {
        let seed = 12345;
        let mut rng = Rng::new(seed);
        let verdict = ai_razes_city(f, &mut rng);
        let draws = (0..4)
            .find(|&n| {
                let mut probe = Rng::new(seed);
                probe.discard(n);
                probe == rng
            })
            .expect("at most three draws");
        (verdict, draws)
    }

    #[test]
    fn an_unremarkable_city_is_razed_only_by_a_full_player() {
        assert_eq!(razes(&raze(&[])), (false, 0));
        let mut f = raze(&[]);
        f.my_cities = 39;
        assert_eq!(razes(&f), (false, 0));
        f.my_cities = 40;
        assert_eq!(razes(&f), (true, 0));
        f.my_cities = 41;
        assert_eq!(razes(&f), (true, 0));
    }

    #[test]
    fn the_protections_beat_every_reason_to_raze() {
        let full = |mut f: RazeFacts<'static>| {
            // Over capacity, a foreign useless wonder, the old owner's
            // race, culture and rating all say raze.
            f.my_cities = 99;
            f.owner_stake = 9;
            f.owner_race_citizens = 6;
            f.owner_rating = 100;
            f
        };
        let base = full(raze(&[USELESS]));
        assert_eq!(razes(&base), (true, 0));
        // 1. the owner's last city
        let mut f = base;
        f.owner_cities = 1;
        assert_eq!(razes(&f), (false, 0));
        // 2. alone on the continent (the sum of both players' cities)
        let mut f = base;
        f.cities_on_continent = 1;
        assert_eq!(razes(&f), (false, 0));
        f.cities_on_continent = 2;
        assert_eq!(razes(&f), (true, 0));
        // 3. a resource the capital lacks
        let mut f = base;
        f.capital_lacks_good = true;
        assert_eq!(razes(&f), (false, 0));
        // 4. a wonder that is wanted
        let wanted = [USELESS, USEFUL];
        let mut f = base;
        f.wonders = &wanted;
        assert_eq!(razes(&f), (false, 0));
        let mine = [MINE_BUT_OBSOLETE];
        f.wonders = &mine;
        assert_eq!(razes(&f), (false, 0));
        // 5. half the citizens are P's own race: (6 + 1) / 2 = 3
        let mut f = base;
        f.my_race_citizens = 2;
        assert_eq!(razes(&f), (true, 0));
        f.my_race_citizens = 3;
        assert_eq!(razes(&f), (false, 0));
    }

    #[test]
    fn the_half_size_threshold_rounds_up() {
        let mut f = raze(&[]);
        f.my_cities = 99;
        for (size, need) in [(1, 1), (2, 1), (3, 2), (4, 2), (5, 3), (6, 3), (12, 6)] {
            f.size = size;
            f.my_race_citizens = need - 1;
            assert!(ai_razes_city(&f, &mut Rng::new(1)), "size {size}");
            f.my_race_citizens = need;
            assert!(!ai_razes_city(&f, &mut Rng::new(1)), "size {size}");
        }
    }

    #[test]
    fn a_foreign_wonder_that_is_no_use_razes_the_city_without_a_draw() {
        let mut f = raze(&[USELESS]);
        assert_eq!(razes(&f), (true, 0));
        // Even when the player is nowhere near its limit.
        f.my_cities = 1;
        assert_eq!(razes(&f), (true, 0));
    }

    #[test]
    fn the_old_owners_culture_and_race_open_a_two_draw_contest() {
        // Owner stake above mine and 4 of 6 citizens of the owner's race
        // (threshold 3): rating test (0 > 0) fails, two next(15) draws.
        let mut f = raze(&[]);
        f.owner_stake = 5;
        f.owner_race_citizens = 4;
        let seed = 12345;
        let mut probe = Rng::new(seed);
        let (a, b) = (probe.below(15), probe.below(15));
        // R - A = 4 against the sum of two draws in 0..=14.
        let expected = 4 >= a + b;
        let (verdict, draws) = razes(&f);
        assert_eq!(draws, 2);
        assert_eq!(verdict, expected);
        // Over capacity, the contest losing falls through to the capacity
        // test (true here) and the contest winning is true as well.
        f.my_cities = 40;
        assert_eq!(razes(&f), (true, 2));
    }

    #[test]
    fn the_contest_is_won_when_the_race_margin_reaches_the_draws() {
        let mut f = raze(&[]);
        f.owner_stake = 1;
        f.owner_race_citizens = 6;
        f.my_race_citizens = 0;
        // Find seeds with a small and with a large draw sum, and check the
        // verdict follows `R - A >= a + b` exactly.
        let (mut won, mut lost) = (0, 0);
        for seed in 0..400u32 {
            let mut probe = Rng::new(seed);
            let sum = probe.below(15) + probe.below(15);
            let mut rng = Rng::new(seed);
            let verdict = ai_razes_city(&f, &mut rng);
            assert_eq!(verdict, 6 >= sum, "seed {seed}");
            assert_eq!(rng, probe, "exactly two draws, seed {seed}");
            if verdict {
                won += 1;
            } else {
                lost += 1;
            }
        }
        assert!(won > 0 && lost > 0, "{won} won, {lost} lost");
    }

    #[test]
    fn no_contest_unless_the_owner_has_more_culture_and_enough_citizens() {
        let mut f = raze(&[]);
        f.owner_race_citizens = 6;
        // Equal stakes (0 and 0): no draws.
        assert_eq!(razes(&f), (false, 0));
        // The owner's stake equal to mine is still not "more".
        f.owner_stake = 4;
        f.my_stake = 4;
        assert_eq!(razes(&f), (false, 0));
        // More culture but too few citizens of its race (2 < 3): no draws.
        f.owner_stake = 5;
        f.owner_race_citizens = 2;
        assert_eq!(razes(&f), (false, 0));
        // Both met: the draws happen.
        f.owner_race_citizens = 3;
        assert_eq!(razes(&f).1, 2);
    }

    #[test]
    fn the_rating_test_precedes_the_draws_and_never_fires_in_play() {
        let mut f = raze(&[]);
        f.owner_stake = 5;
        f.owner_race_citizens = 3;
        f.owner_rating = 3;
        f.my_rating = 1;
        // 3 > 2 * 1: razed at once.
        assert_eq!(razes(&f), (true, 0));
        f.my_rating = 2;
        assert_eq!(razes(&f).1, 2);
        // The constructor's value for every player is 0, so 0 > 0 is false.
        f.owner_rating = 0;
        f.my_rating = 0;
        assert_eq!(razes(&f).1, 2);
    }

    #[test]
    fn scarce_goods_are_luxuries_and_strategics() {
        assert!(!is_scarce_good(0));
        assert!(is_scarce_good(1));
        assert!(is_scarce_good(2));
        assert!(!is_scarce_good(3));
        assert!(!is_scarce_good(-1));
    }
}
