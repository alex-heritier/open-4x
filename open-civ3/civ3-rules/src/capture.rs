//! City capture and transfer, and razing.
//!
//! This module holds the parts that are pure arithmetic or pure decisions,
//! including the AI's two choices ([`ai_accepts_city`], [`ai_razes_city`]) over
//! facts the caller reads from the world; everything that touches the world
//! (units, tiles, message dialogs) stays outside.
//!
//! The barbarian raid is a separate branch that never transfers the city:
//! [`raid_loss`], [`raid_loot`].

use crate::economy::{size_class, treasury_cells, CITY_MAX, TOWN_MAX};
use civ3_worldgen::rng::Rng;

/// The two boolean stack arguments of `Player::capture`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Flags {
    /// The old owner's units on the city tile are killed, and in the transfer
    /// the buildings may be destroyed by chance ([`surviving_buildings`]).
    pub capture: bool,
    /// A culture conversion; it overrides the plunder branch.
    pub convert: bool,
}

/// Which part of the capture routine runs for a [`Flags`] pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Path {
    /// `(false, false)`: straight to the transfer.
    Silent,
    /// `(true, false)`: plunder, population loss, keep or raze.
    Plunder,
    /// `convert` set: the accept-or-rebuff question.
    Conversion,
}

impl Flags {
    /// The branch of the capture routine these flags select.
    pub fn path(self) -> Path {
        if self.convert {
            Path::Conversion
        } else if self.capture {
            Path::Plunder
        } else {
            Path::Silent
        }
    }

    /// The post-transfer timer argument: 1 for a plain capture, 10 otherwise.
    pub fn post_transfer_timer(self) -> i32 {
        if self.capture && !self.convert {
            1
        } else {
            10
        }
    }
}

/// Incident weight added to the victim's pair record about the capturer for
/// every military capture from a real civ.
pub const CAPTURE_INCIDENT: i32 = 16;

/// Extra incident weight when the capture also kills a citizen.
pub const SHRINK_INCIDENT: i32 = 1;

/// Gold taken in a military capture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plunder {
    /// The loot before the unit bonus; what the victim loses.
    pub base: i32,
    /// What the capturer gains: `base`, doubled when the capturing unit carries
    /// the bonus flag.
    pub gain: i32,
}

/// The per-city share scaled by the city's size class: a town pays half, a city
/// three quarters, a metropolis the whole share. Both divisions truncate toward
/// zero.
pub fn scale_share(share: i32, class: i32) -> i32 {
    match class {
        0 => share / 2,
        1 => share.wrapping_mul(3) / 4,
        _ => share,
    }
}

/// The gold a capture takes. The victim's last city yields the whole treasury;
/// otherwise the loot is `treasury / cities` scaled by [`scale_share`]. The
/// bonus never applies to a last city.
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

/// The victim's treasury cells after the capture: `max(0, total - base)`, split
/// from the clock like every treasury write.
pub fn victim_cells(total: i32, p: Plunder, clock: u32) -> (i32, i32) {
    treasury_cells(total.wrapping_sub(p.base).max(0), clock)
}

/// The capturer's treasury cells after the capture: `total + gain`; a result at
/// or below 0 is stored as a sum of 0.
pub fn capturer_cells(total: i32, p: Plunder, clock: u32) -> (i32, i32) {
    treasury_cells(total.wrapping_add(p.gain), clock)
}

/// What the capture does to the city's population.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Loss {
    /// The city keeps its size.
    None,
    /// One citizen of any race dies.
    Shrink,
    /// The city is destroyed and the keep-or-raze question is skipped.
    Destroy,
}

/// The facts [`capture_loss`] reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CityFacts {
    /// The city size.
    pub size: i32,
    /// Citizens of the capturer's race.
    pub capturer_race_citizens: i32,
    /// The capturer's per-civ culture stake in the city.
    pub capturer_stake: i32,
    /// Citizens of the old owner's race.
    pub owner_race_citizens: i32,
    /// 1 when the city was founded and when the transfer resets it.
    pub style: i32,
}

/// The population loss of a capture.
///
/// Nothing is lost when the city already holds a citizen of the capturer's race,
/// or the capturer has a non-zero stake in it. Otherwise a city of size 1 is
/// destroyed if its one citizen is of the old owner's race and `style == 1`, and
/// kept at size 1 if not; a larger city loses one citizen.
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

/// Workers released when a city is razed: `size / 2` units of the RULE worker
/// prototype.
pub fn raze_workers(size: i32) -> i32 {
    size / 2
}

/// What the capture code reads about one BLDG row, from three dwords of the
/// in-memory row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuildingClass {
    /// Center of Empire, the Palace.
    pub center_of_empire: bool,
    /// A great wonder.
    pub great_wonder: bool,
    /// A small wonder.
    pub small_wonder: bool,
    /// The two city-size gates (Aqueduct, Hospital).
    pub size_gate: bool,
    /// Culture per turn.
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

    /// Not a wonder, not the palace, not a size gate.
    pub fn is_ordinary(&self) -> bool {
        !(self.great_wonder || self.small_wonder || self.center_of_empire || self.size_gate)
    }
}

/// The buildings that survive a transfer, in BLDG index order.
///
/// `held` lists `(BLDG index, class)` for every building the city holds,
/// ascending by index. Four passes run:
///
/// * **A**, only when the city is the old owner's capital: buildings with Center
///   of Empire are destroyed.
/// * **B**, always: every ordinary building with culture above 0 is destroyed.
/// * **C**, only for a capture that is not a conversion, and only when the city
///   holds no citizen of the capturer's race: each remaining ordinary building
///   is destroyed when `next(4) == 0` (one draw per building, in index order).
/// * **D**, always: small wonders are destroyed.
///
/// Great wonders, size gates and every building that none of the passes names
/// survive.
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

/// The loss a barbarian raid inflicts, in the order the game tests them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RaidLoss {
    /// The building with the best land bombard defense is removed.
    Walls,
    /// The shield stock is set to 0.
    Stock,
    /// One citizen of the owner's race dies.
    Citizen,
    /// Gold: [`raid_loot`].
    Gold,
}

/// The first loss that applies to a raided city.
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

/// The gold a barbarian raid takes: `treasury / cities`, and when that is 0, 1
/// if the treasury exceeds 1, else the whole treasury. The barbarians do not
/// receive it.
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

/// Is a great wonder active for a civ? The required government fits and the
/// obsoleting tech is unknown.
pub fn wonder_active(
    required_government: i32,
    government: i32,
    obsolete_by: i32,
    knows_obsoleting_tech: bool,
) -> bool {
    (required_government == -1 || required_government == government)
        && (obsolete_by == -1 || !knows_obsoleting_tech)
}

/// One great wonder standing in the captured city, as the deciding AI player
/// sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wonder {
    /// [`wonder_active`] for the deciding player.
    pub active: bool,
    /// The wonder's builder is the deciding player's civ.
    pub built_by_me: bool,
}

impl Wonder {
    /// Active, or built by the deciding player: either makes the AI keep the
    /// city.
    pub fn is_wanted(self) -> bool {
        self.active || self.built_by_me
    }
}

/// Everything the accept decision reads.
#[derive(Clone, Copy, Debug)]
pub struct AcceptFacts<'a> {
    /// The player is at war with the city's owner.
    pub at_war_with_owner: bool,
    /// The great wonders in the city.
    pub wonders: &'a [Wonder],
    /// The city's citizens of the player's race.
    pub my_race_citizens: i32,
    /// The player's city count.
    pub city_count: i32,
    /// [`crate::economy::optimal_city_number`] of the player.
    pub ocn: i32,
}

/// Does the AI take a city that culture has flipped to it? In order: at war with
/// the owner; a wanted wonder; at least one citizen of the player's race;
/// otherwise only while the player has fewer than twice its optimal city number
/// of cities. There is no random draw.
pub fn ai_accepts_city(f: &AcceptFacts) -> bool {
    f.at_war_with_owner
        || f.wonders.iter().any(|w| w.is_wanted())
        || f.my_race_citizens > 0
        || f.city_count < 2 * f.ocn
}

/// `GOOD` class predicate: luxury (1) or strategic (2), not a bonus resource
/// (0).
pub fn is_scarce_good(class: i32) -> bool {
    class == 1 || class == 2
}

/// Everything the raze decision reads.
#[derive(Clone, Copy, Debug)]
pub struct RazeFacts<'a> {
    /// The owner's city count.
    pub owner_cities: i32,
    /// The owner's plus the capturer's cities on the city's continent.
    pub cities_on_continent: i32,
    /// The scan of the city's radius found a luxury or strategic resource the
    /// capturer's capital cannot reach. False when the capturer has no capital.
    pub capital_lacks_good: bool,
    /// The great wonders in the city.
    pub wonders: &'a [Wonder],
    /// City size.
    pub size: i32,
    /// Citizens of the capturer's race.
    pub my_race_citizens: i32,
    /// Citizens of the owner's race.
    pub owner_race_citizens: i32,
    /// The capturer's culture stake in the city.
    pub my_stake: i32,
    /// The owner's stake.
    pub owner_stake: i32,
    /// The capturer's culture rating.
    pub my_rating: i32,
    /// The owner's culture rating.
    pub owner_rating: i32,
    /// The capturer's city count.
    pub my_cities: i32,
    /// [`crate::economy::optimal_city_number`] of the capturer.
    pub ocn: i32,
}

/// Does the AI raze the city it has just captured? Rules, in the order the code
/// applies them; the first that fires wins:
///
/// 1. the owner's last city: no;
/// 2. the only city of either player on its continent: no;
/// 3. a reachable-blocked scarce resource in the radius: no;
/// 4. a wanted wonder: no;
/// 5. at least `(size + 1) / 2` of the citizens are of the capturer's race: no;
/// 6. any great wonder at all: yes;
/// 7. the owner holds more culture and at least half the citizens: yes, with the
///    two draws made only past the rating test;
/// 8. otherwise yes exactly when the capturer has at least twice its optimal
///    city number of cities.
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
        assert_eq!(PLAIN.post_transfer_timer(), 1);
        assert_eq!(Flags { capture: false, convert: false }.post_transfer_timer(), 10);
    }

    #[test]
    fn last_city_yields_the_whole_treasury_and_share_scales_by_class() {
        for size in [1, 6, 7, 12, 13, 30] {
            assert_eq!(plunder(1234, 1, size, true).base, 1234);
        }
        assert_eq!(plunder(1000, 4, 6, false).base, 125);
        assert_eq!(plunder(1000, 4, 7, false).base, 187);
        assert_eq!(plunder(1000, 4, 13, false).base, 250);
    }

    #[test]
    fn bonus_unit_doubles_the_gain_but_not_the_loss() {
        let p = plunder(1000, 4, 13, true);
        assert_eq!((p.base, p.gain), (250, 500));
        assert_eq!(plunder(1000, 1, 13, true).gain, 1000);
    }

    #[test]
    fn treasury_writes_conserve_the_sums_and_never_go_negative() {
        let p = plunder(1000, 4, 13, false);
        let (a, b) = victim_cells(1000, p, 987_654);
        assert_eq!(a + b, 750);
        let (a, b) = capturer_cells(40, p, 987_654);
        assert_eq!(a + b, 290);
        let p = Plunder { base: 500, gain: 500 };
        assert_eq!(victim_cells(100, p, 55_555), treasury_cells(0, 55_555));
        assert_eq!(capturer_cells(-600, p, 55_555), treasury_cells(0, 55_555));
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
        assert_eq!(capture_loss(&CityFacts { capturer_race_citizens: 1, ..base }), Loss::None);
        assert_eq!(capture_loss(&CityFacts { capturer_stake: 3, ..base }), Loss::None);
        let one = CityFacts {
            size: 1,
            capturer_race_citizens: 0,
            capturer_stake: 0,
            owner_race_citizens: 1,
            style: 1,
        };
        assert_eq!(capture_loss(&one), Loss::Destroy);
        assert_eq!(capture_loss(&CityFacts { style: 2, ..one }), Loss::None);
    }

    #[test]
    fn razing_releases_half_the_population_as_workers() {
        assert_eq!(raze_workers(1), 0);
        assert_eq!(raze_workers(2), 1);
        assert_eq!(raze_workers(16), 8);
    }

    fn palace() -> BuildingClass {
        BuildingClass::from_words(0x1, 0x0, 1)
    }
    fn temple() -> BuildingClass {
        BuildingClass::from_words(0x0, 0x100, 2)
    }
    fn barracks() -> BuildingClass {
        BuildingClass::from_words(0x2, 0x2, 0)
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
        assert!(great_library().great_wonder && !great_library().is_ordinary());
        assert!(wall_street().small_wonder && !wall_street().is_ordinary());
    }

    #[test]
    fn peaceful_transfer_keeps_everything_but_culture_and_small_wonders() {
        let held = [
            (0, palace()),
            (1, barracks()),
            (3, temple()),
            (34, great_library()),
            (59, wall_street()),
        ];
        let silent = Flags { capture: false, convert: false };
        let mut rng = Rng::new(7);
        let before = rng;
        let kept = surviving_buildings(&held, false, silent, false, &mut rng);
        assert_eq!(kept, vec![0, 1, 34]);
        assert_eq!(rng, before, "a silent transfer draws nothing");
    }

    #[test]
    fn accepting_a_flip_needs_a_reason_or_room_for_cities() {
        let none = [];
        let big = Rng::new(1);
        let _ = big;
        let facts = |at_war, citizens, cities, ocn| AcceptFacts {
            at_war_with_owner: at_war,
            wonders: &none,
            my_race_citizens: citizens,
            city_count: cities,
            ocn,
        };
        assert!(ai_accepts_city(&facts(true, 0, 100, 10)));
        assert!(ai_accepts_city(&facts(false, 1, 100, 10)));
        assert!(ai_accepts_city(&facts(false, 0, 19, 10)));
        assert!(!ai_accepts_city(&facts(false, 0, 20, 10)));
    }
}
