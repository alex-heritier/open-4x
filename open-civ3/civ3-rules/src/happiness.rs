//! Happiness: citizen moods, civil disorder, riots and "We Love the King Day".
//!
//! The city keeps one mood per citizen; [`recompute`] rebuilds every mood from
//! scratch each time the city's totals change, and the per-turn sequencer then
//! reads the moods: [`is_disorder`]/[`disorder_step`] decide disorder and riots,
//! [`celebrates`]/[`celebration_step`] decide the celebration.
//!
//! Everything the binary reads out of the player records, the BIQ tables and the
//! tile map is a plain field of [`Inputs`], [`Citizen`] and [`BuildingFaces`], so
//! the functions are pure; the dice are the shared gameplay [`Rng`].

use crate::government::city_weariness_unhappy;
use civ3_worldgen::rng::Rng;

/// Citizen mood codes.
pub mod mood {
    /// Happy.
    pub const HAPPY: u8 = 0;
    /// Content.
    pub const CONTENT: u8 = 1;
    /// Unhappy.
    pub const UNHAPPY: u8 = 2;
    /// Resisting (a conquered citizen). [`super::shift`] never moves it.
    pub const RESISTING: u8 = 3;
    /// Specialist. [`super::shift`] never moves it.
    pub const SPECIALIST: u8 = 4;
}

/// The nine bytes of reason counts, then (last step) percentages.
pub mod reason {
    /// Unused.
    pub const UNUSED: usize = 0;
    /// Citizens born unhappy by the difficulty.
    pub const BASE: usize = 1;
    /// War weariness.
    pub const WAR_WEARINESS: usize = 2;
    /// Foreign nationals of a civ at war with the owner.
    pub const FOREIGN: usize = 3;
    /// Propaganda.
    pub const PROPAGANDA: usize = 4;
    /// The draft.
    pub const DRAFT: usize = 5;
    /// The hurry sacrifice.
    pub const HURRY: usize = 6;
    /// Unhappy faces of buildings in the city.
    pub const BUILDING_CITY: usize = 7;
    /// Unhappy faces of buildings in all cities.
    pub const BUILDING_ALL: usize = 8;
}

/// Number of reason bytes.
pub const REASONS: usize = 9;

/// Luxury resources counted at most.
pub const MAX_LUXURY_GOODS: usize = 11;

/// Happy faces given by `n` distinct usable luxury resources in a city with a
/// Marketplace. The Civilopedia's Marketplace entry lists the same numbers for
/// 1 to 8.
pub const FACE_TABLE: [i32; 12] = [0, 1, 2, 4, 6, 9, 12, 16, 20, 24, 28, 32];

/// BLDG `+0xEC` (improvement flags) bit tested by the luxury step.
pub const LUXURY_TRADE_FLAG: u32 = 0x400;

/// BLDG `+0xEC` bit 22, "Reduces War Weariness".
pub const REDUCES_WAR_WEARINESS: u32 = 0x40_0000;

/// What the shipped rules put in the RULE record for this system.
pub mod shipped {
    /// RULE `+0x7C`: `chance_of_rioting`.
    pub const CHANCE_OF_RIOTING: i32 = 20;
    /// RULE `+0x80`: `draft_turn_penalty`.
    pub const DRAFT_TURN_PENALTY: i32 = 20;
    /// RULE `+0x8C`: `citizens_per_happy_face` (divides the luxury commerce).
    pub const CITIZENS_PER_HAPPY_FACE: i32 = 1;
    /// RULE `+0xF0`: `wltk_min_population`.
    pub const WLTK_MIN_POPULATION: i32 = 6;
    /// RULE `+0xD8`: `hurry_sacrifice_turn_penalty`.
    pub const HURRY_SACRIFICE_TURN_PENALTY: i32 = 20;
    /// RULE `+0x100`: `town_max_size`.
    pub const TOWN_MAX_SIZE: i32 = 6;
    /// RULE `+0x104`: `city_max_size`.
    pub const CITY_MAX_SIZE: i32 = 12;
    /// DIFF `+0x44`: `citizens_born_content` by difficulty (Chieftain to Sid).
    pub const BORN_CONTENT: [i32; 8] = [4, 3, 2, 2, 1, 1, 1, 1];
    /// GOVT `+0x1AC`: `military_police_limit` (Anarchy to Feudalism).
    pub const MILITARY_POLICE_LIMIT: [i32; 8] = [0, 2, 3, 4, 0, 0, 4, 3];
}

/// One citizen as the mood code sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Citizen {
    /// The current mood, see [`mood`].
    pub mood: u8,
    /// A resister.
    pub resisting: bool,
    /// The citizen's job is not the default worker.
    pub specialist: bool,
    /// The citizen's race differs from the owner's.
    pub foreign: bool,
    /// [`Citizen::foreign`], and the race's civ is in play and at war with the
    /// owner.
    pub foreign_at_war: bool,
}

impl Citizen {
    /// An ordinary citizen of the owner's race.
    pub fn native(mood: u8) -> Self {
        Citizen {
            mood,
            resisting: false,
            specialist: false,
            foreign: false,
            foreign_at_war: false,
        }
    }
}

/// One BLDG row as the building pass sees it for one city: the table values plus
/// the answers of the helper calls, already resolved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BuildingFaces {
    /// `other_characteristics & 4`: a great wonder.
    pub wonder: bool,
    /// BLDG `+0xD4` is `-1` or the owner's government.
    pub government_ok: bool,
    /// BLDG `+0xE0` is a tech and the owner knows it.
    pub obsolete: bool,
    /// The building is in the city (or granted to it) under the right
    /// government.
    pub present: bool,
    /// How many the owner has.
    pub owned: i32,
    /// The effect is scaled by the owner's cities on this continent instead of
    /// by `owned`.
    pub continental: bool,
    /// Other cities of the owner on this continent that have it. Read only when
    /// [`BuildingFaces::continental`].
    pub on_continent: i32,
    /// The owner holds a live wonder whose `doubles_happiness_of` is this
    /// building.
    pub doubled: bool,
    /// BLDG `+0xB4`: content faces in the city.
    pub happy_city: i32,
    /// BLDG `+0xBC`: unhappy faces in the city.
    pub unhappy_city: i32,
    /// BLDG `+0xB0`: content faces, per counted copy elsewhere.
    pub happy_all: i32,
    /// BLDG `+0xB8`: unhappy faces, per counted copy elsewhere.
    pub unhappy_all: i32,
}

/// Zero the reason bytes, then give every ordinary citizen (not a resister, not
/// a specialist) in list order a base mood: the first `born_content` are
/// content, the rest unhappy. The unhappy ones are added to
/// [`reason::BASE`]. Resisters and specialists keep their mood.
pub fn base_mood(citizens: &mut [Citizen], born_content: i32, reasons: &mut [u8; REASONS]) {
    *reasons = [0; REASONS];
    let (mut content, mut unhappy) = (0i32, 0u8);
    for c in citizens.iter_mut().filter(|c| !c.resisting && !c.specialist) {
        if content < born_content {
            c.mood = mood::CONTENT;
            content += 1;
        } else {
            c.mood = mood::UNHAPPY;
            unhappy = unhappy.wrapping_add(1);
        }
    }
    reasons[reason::BASE] = reasons[reason::BASE].wrapping_add(unhappy);
}

/// The content faces buildings add (+) or take away (-), for one city. Returns
/// the change of the content accumulator and adds the unhappy faces to
/// [`reason::BUILDING_CITY`] / [`reason::BUILDING_ALL`].
pub fn building_pass(buildings: &[BuildingFaces], reasons: &mut [u8; REASONS]) -> i32 {
    let mut content = 0;
    for b in buildings {
        if b.wonder && (!b.government_ok || b.obsolete) {
            continue;
        }
        let k = if b.doubled { 2 } else { 1 };
        let mut n = b.owned;
        if b.present && !b.obsolete {
            n -= 1;
            if b.happy_city > 0 {
                content += b.happy_city * k;
            }
            if b.unhappy_city > 0 {
                let v = b.unhappy_city * k;
                content -= v;
                reasons[reason::BUILDING_CITY] =
                    reasons[reason::BUILDING_CITY].wrapping_add(v as u8);
            }
        }
        if n == 0 {
            continue;
        }
        if b.continental {
            n = b.on_continent;
        }
        let up = b.happy_all * n;
        if up > 0 {
            content += up * k;
        }
        let down = b.unhappy_all * n;
        if down > 0 {
            let v = down * k;
            content -= v;
            reasons[reason::BUILDING_ALL] = reasons[reason::BUILDING_ALL].wrapping_add(v as u8);
        }
    }
    content
}

/// Martial law: the units on the city tile, clamped to `0..=limit`; a negative
/// limit gives 0. Each counted unit is one content face. Only applied when the
/// city has no resister.
pub fn martial_law(units: i32, limit: i32) -> i32 {
    if limit < 0 || units < 0 {
        0
    } else {
        units.min(limit)
    }
}

/// The draft and hurry-sacrifice penalties: a timer `t > 0` costs
/// `(t - 1) / penalty + 1` happy faces. `penalty` is RULE `draft_turn_penalty`
/// or `hurry_sacrifice_turn_penalty`.
pub fn draft_unhappy(timer: i32, penalty: i32) -> i32 {
    if timer > 0 {
        (timer - 1) / penalty + 1
    } else {
        0
    }
}

/// Happy faces from luxury resources: `goods` usable luxury resources (at most
/// [`MAX_LUXURY_GOODS`]), run through [`FACE_TABLE`] when the city has a
/// building with [`LUXURY_TRADE_FLAG`].
pub fn luxury_faces(goods: usize, has_luxury_trade_building: bool) -> i32 {
    let n = goods.min(MAX_LUXURY_GOODS);
    if has_luxury_trade_building {
        FACE_TABLE[n]
    } else {
        n as i32
    }
}

/// Each civ in play and at war with the owner whose war counter is **negative**
/// gives `size / 4` happy faces; the sum is capped at `size`.
pub fn war_enthusiasm(size: i32, counters: impl IntoIterator<Item = i32>) -> i32 {
    let total: i32 = counters.into_iter().filter(|&c| c < 0).map(|_| size / 4).sum();
    total.min(size)
}

/// The citizens who are not resisters and whose race belongs to a civ at war
/// with the owner. Each is one happy face lost; specialists count.
pub fn foreign_at_war(citizens: &[Citizen]) -> i32 {
    citizens
        .iter()
        .filter(|c| !c.resisting && c.foreign_at_war)
        .count() as i32
}

/// Move up to `count` citizens from mood `from` to mood `to`; returns how many
/// moved. `foreign_limit` is the foreign-nationals byte.
///
/// Resisting and specialist moods are neither sources nor targets. Only
/// ordinary citizens with mood `from` move. Pass 1 runs while fewer than
/// `foreign_limit` have moved: it prefers foreign-race citizens when making
/// someone **unhappy** (`to == 2`) and own-race citizens when making someone
/// **less unhappy** (`from == 2`). Pass 2 takes whoever is left, in list order.
pub fn shift(citizens: &mut [Citizen], foreign_limit: u8, count: i32, from: u8, to: u8) -> i32 {
    use mood::{RESISTING, SPECIALIST, UNHAPPY};
    if [from, to].iter().any(|&m| m == RESISTING || m == SPECIALIST) {
        return 0;
    }
    let (mut count, mut moved) = (count, 0i32);
    for c in citizens.iter_mut() {
        if count == 0 || moved >= i32::from(foreign_limit) {
            break;
        }
        let preferred = if c.foreign { to == UNHAPPY } else { from == UNHAPPY };
        if preferred && !c.resisting && !c.specialist && c.mood == from {
            c.mood = to;
            count -= 1;
            moved += 1;
        }
    }
    for c in citizens.iter_mut() {
        if count == 0 {
            break;
        }
        if !c.resisting && !c.specialist && c.mood == from {
            c.mood = to;
            count -= 1;
            moved += 1;
        }
    }
    moved
}

/// Turn the two accumulators into mood changes. `a` is the **happy-face**
/// accumulator and `b` the **content-face** accumulator. "Up" with `n` faces:
/// content to happy for `n`; the `m` that moved leave `n - m`, and each **two**
/// left over lift one unhappy citizen to happy; one face still left over lifts
/// one unhappy citizen to content. "Down" is the mirror image.
pub fn distribute(citizens: &mut [Citizen], foreign_limit: u8, a: i32, b: i32) {
    use mood::{CONTENT, HAPPY, UNHAPPY};
    let mut sh = |count, from, to| shift(citizens, foreign_limit, count, from, to);
    if a >= 0 && b <= 0 {
        let (a, b) = if a > -b { (a + b, 0) } else { (0, a + b) };
        let m = sh(a, CONTENT, HAPPY);
        let m2 = sh((a - m) / 2, UNHAPPY, HAPPY);
        if a > m + 2 * m2 {
            sh(1, UNHAPPY, CONTENT);
        }
        sh(-b, HAPPY, CONTENT);
    } else if a <= 0 && b >= 0 && (a < 0 || b > 0) {
        let (a, b) = if b > -a { (0, a + b) } else { (a + b, 0) };
        let a = -a;
        let m = sh(a, CONTENT, UNHAPPY);
        let m2 = sh((a - m) / 2, HAPPY, UNHAPPY);
        if a > m + 2 * m2 {
            sh(1, HAPPY, CONTENT);
        }
        sh(b, UNHAPPY, CONTENT);
    } else if a > 0 && b > 0 {
        sh(b, UNHAPPY, CONTENT);
        let m = sh(a, CONTENT, HAPPY);
        let m2 = sh((a - m) / 2, UNHAPPY, HAPPY);
        if a > m + 2 * m2 {
            sh(1, UNHAPPY, CONTENT);
        }
    } else {
        sh(-b, HAPPY, CONTENT);
        let a = -a;
        let m = sh(a, CONTENT, UNHAPPY);
        let m2 = sh((a - m) / 2, HAPPY, UNHAPPY);
        if a > m + 2 * m2 {
            sh(1, HAPPY, CONTENT);
        }
    }
}

/// The percentage the binary stores for byte `b` of a total `sum`: truncate
/// `(b / sum) * 100` with every x87 step rounded to a 64-bit mantissa. That
/// differs from the exact `b * 100 / sum` at eight inputs with `b <= 255` and
/// `sum <= 2295`.
pub fn x87_percent(b: u32, sum: u32) -> u8 {
    if b == 0 {
        return 0;
    }
    let (n, d) = (u128::from(b), u128::from(sum));
    let mut s = 63u32;
    while (n << s) / d >= 1u128 << 64 {
        s -= 1;
    }
    while (n << s) / d < 1u128 << 63 {
        s += 1;
    }
    let (mut m, rem) = ((n << s) / d, (n << s) % d);
    if 2 * rem > d || (2 * rem == d && m & 1 == 1) {
        m += 1;
    }
    let mut p = m * 100;
    let mut ps = s;
    let bits = 128 - p.leading_zeros();
    if bits > 64 {
        let t = bits - 64;
        let low = p & ((1u128 << t) - 1);
        p >>= t;
        ps -= t;
        let half = 1u128 << (t - 1);
        if low > half || (low == half && p & 1 == 1) {
            p += 1;
        }
    }
    (p >> ps) as u8
}

/// The last step of the recompute: when no citizen is unhappy the nine bytes
/// are zeroed; otherwise each becomes its percentage of their sum.
pub fn reason_percentages(citizens: &[Citizen], reasons: &mut [u8; REASONS]) {
    if !citizens.iter().any(|c| c.mood == mood::UNHAPPY) {
        *reasons = [0; REASONS];
        return;
    }
    let sum: u32 = reasons.iter().map(|&r| u32::from(r)).sum();
    if sum != 0 {
        for r in reasons.iter_mut() {
            *r = x87_percent(u32::from(*r), sum);
        }
    }
}

/// Everything the recompute reads, resolved. `citizens` is in list order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inputs {
    /// The citizens, in list order.
    pub citizens: Vec<Citizen>,
    /// The city size.
    pub size: i32,
    /// The difficulty's `citizens_born_content`, see [`shipped::BORN_CONTENT`].
    pub born_content: i32,
    /// One entry per BLDG row, in table order.
    pub buildings: Vec<BuildingFaces>,
    /// The draft timer.
    pub draft_timer: i32,
    /// RULE `draft_turn_penalty`.
    pub draft_penalty: i32,
    /// The units counted for martial law.
    pub units_on_tile: i32,
    /// GOVT `military_police_limit`.
    pub police_limit: i32,
    /// Luxury commerce plus the specialists' luxury.
    pub luxury_points: i32,
    /// RULE `citizens_per_happy_face`.
    pub citizens_per_face: i32,
    /// Usable luxury resources.
    pub luxury_goods: usize,
    /// The city has a building with [`LUXURY_TRADE_FLAG`].
    pub luxury_trade_building: bool,
    /// Propaganda unhappiness, in faces.
    pub propaganda: i32,
    /// The hurry-sacrifice timer.
    pub hurry_timer: i32,
    /// RULE `hurry_sacrifice_turn_penalty`.
    pub hurry_penalty: i32,
    /// The war counter against every civ in play the owner is at war with.
    pub war_counters: Vec<i32>,
    /// GOVT `+0x1E4`: the weariness class (0 none, 1 low, 2 high).
    pub weariness_class: i32,
    /// Buildings in the city with [`REDUCES_WAR_WEARINESS`], present and not
    /// obsolete.
    pub police_buildings: i32,
    /// Owned wonders with the "reduces war weariness everywhere" flag.
    pub suffrage: i32,
}

/// What the recompute leaves behind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    /// The citizens with their new moods.
    pub citizens: Vec<Citizen>,
    /// The reason bytes after the percentage step.
    pub reasons: [u8; REASONS],
    /// The happy-face accumulator that was distributed.
    pub happy_faces: i32,
    /// The content-face accumulator that was distributed.
    pub content_faces: i32,
}

/// The recompute, in the order the binary runs it. Resisters present in the city
/// switch martial law off.
pub fn recompute(inp: &Inputs) -> Outcome {
    let mut citizens = inp.citizens.clone();
    let mut reasons = [0u8; REASONS];
    base_mood(&mut citizens, inp.born_content, &mut reasons);
    let mut content = building_pass(&inp.buildings, &mut reasons);
    let mut happy = 0;

    let draft = draft_unhappy(inp.draft_timer, inp.draft_penalty);
    happy -= draft;
    reasons[reason::DRAFT] = reasons[reason::DRAFT].wrapping_add(draft as u8);

    if !citizens.iter().any(|c| c.resisting) {
        content += martial_law(inp.units_on_tile, inp.police_limit);
    }

    happy += inp.luxury_points / inp.citizens_per_face;
    happy += luxury_faces(inp.luxury_goods, inp.luxury_trade_building) - inp.propaganda;
    reasons[reason::PROPAGANDA] = reasons[reason::PROPAGANDA].wrapping_add(inp.propaganda as u8);

    let hurry = draft_unhappy(inp.hurry_timer, inp.hurry_penalty);
    happy -= hurry;
    reasons[reason::HURRY] = reasons[reason::HURRY].wrapping_add(hurry as u8);

    happy += war_enthusiasm(inp.size, inp.war_counters.iter().copied());
    let weary = city_weariness_unhappy(
        inp.size,
        inp.weariness_class,
        inp.war_counters.iter().copied(),
        inp.police_buildings,
        inp.suffrage,
    );
    happy -= weary;
    reasons[reason::WAR_WEARINESS] = reasons[reason::WAR_WEARINESS].wrapping_add(weary as u8);

    let foreign = foreign_at_war(&citizens);
    happy -= foreign;
    reasons[reason::FOREIGN] = reasons[reason::FOREIGN].wrapping_add(foreign as u8);

    distribute(&mut citizens, reasons[reason::FOREIGN], happy, content);
    reason_percentages(&citizens, &mut reasons);
    Outcome {
        citizens,
        reasons,
        happy_faces: happy,
        content_faces: content,
    }
}

fn count(citizens: &[Citizen], m: u8) -> i32 {
    citizens.iter().filter(|c| c.mood == m).count() as i32
}

/// The disorder condition: **strictly more unhappy than happy** citizens.
pub fn is_disorder(citizens: &[Citizen]) -> bool {
    count(citizens, mood::HAPPY) < count(citizens, mood::UNHAPPY)
}

/// What the per-turn disorder check does to the city flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disorder {
    /// Not in disorder, and not now.
    Quiet,
    /// The flag is set this turn.
    Begins,
    /// Still in disorder: the riot roll runs.
    Continues,
    /// The flag is cleared.
    Ends,
}

/// The four-way branch of the disorder step.
pub fn disorder_step(flagged: bool, condition: bool) -> Disorder {
    match (flagged, condition) {
        (false, false) => Disorder::Quiet,
        (false, true) => Disorder::Begins,
        (true, true) => Disorder::Continues,
        (true, false) => Disorder::Ends,
    }
}

/// The RULE values the riot roll reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RiotRules {
    /// `chance_of_rioting`.
    pub chance: i32,
    /// `town_max_size`.
    pub town_max: i32,
    /// `city_max_size`.
    pub city_max: i32,
}

impl RiotRules {
    /// The shipped values.
    pub const SHIPPED: RiotRules = RiotRules {
        chance: shipped::CHANCE_OF_RIOTING,
        town_max: shipped::TOWN_MAX_SIZE,
        city_max: shipped::CITY_MAX_SIZE,
    };
}

/// BLDG `other_characteristics` bits the riot refuses.
const WONDER: u32 = 4;
const SMALL_WONDER: u32 = 8;
/// BLDG `+0xEC` bit 0 (the palace) and bits 11 and 12 (the size-limit
/// buildings): the riot never destroys them.
const NOT_DESTROYED_FLAGS: u32 = 0x1801;

/// Whether a riot may destroy the building `i` of the city: it is in the city's
/// own set, and is neither a wonder, nor a small wonder, nor carries improvement
/// flag bits 0, 11 or 12.
pub fn riot_can_destroy(in_city: bool, other_characteristics: u32, improvement_flags: u32) -> bool {
    in_city
        && other_characteristics & (WONDER | SMALL_WONDER) == 0
        && improvement_flags & NOT_DESTROYED_FLAGS == 0
}

/// The riot roll of a city that stays in disorder. Returns the building row to
/// destroy, if any.
pub fn riot(
    rng: &mut Rng,
    rules: &RiotRules,
    size: i32,
    is_capital: bool,
    n_buildings: u32,
    destroyable: impl Fn(usize) -> bool,
) -> Option<usize> {
    if rng.below(100) >= rules.chance || is_capital {
        return None;
    }
    if size <= rules.city_max && size <= rules.town_max && rng.below(100) >= 2 * rules.chance {
        return None;
    }
    let tries = size + 20;
    if tries < 0 {
        return None;
    }
    (0..=tries).find_map(|_| {
        let row = rng.below(n_buildings) as usize;
        destroyable(row).then_some(row)
    })
}

/// The celebration test: the city has at least `wltk_min_population` citizens,
/// nobody unhappy, strictly more happy than content citizens, no resister, and a
/// food surplus that is not negative.
pub fn celebrates(
    size: i32,
    wltk_min_population: i32,
    citizens: &[Citizen],
    food_surplus: i32,
) -> bool {
    size >= wltk_min_population
        && count(citizens, mood::UNHAPPY) == 0
        && count(citizens, mood::HAPPY) > count(citizens, mood::CONTENT)
        && !citizens.iter().any(|c| c.resisting)
        && food_surplus >= 0
}

/// What the celebration check does to the city flag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Celebration {
    /// Neither before nor now.
    Quiet,
    /// The flag is set and "We Love the King Day" starts.
    Begins,
    /// Already set, still qualifies; nothing happens.
    Continues,
    /// The flag is cleared.
    Ends,
}

/// The branch structure of the celebration step.
pub fn celebration_step(flagged: bool, qualifies: bool) -> Celebration {
    match (flagged, qualifies) {
        (false, false) => Celebration::Quiet,
        (false, true) => Celebration::Begins,
        (true, true) => Celebration::Continues,
        (true, false) => Celebration::Ends,
    }
}

#[cfg(test)]
mod tests {
    use super::mood::{CONTENT, HAPPY, RESISTING, SPECIALIST, UNHAPPY};
    use super::*;

    fn natives(moods: &[u8]) -> Vec<Citizen> {
        moods.iter().map(|&m| Citizen::native(m)).collect()
    }

    fn moods(c: &[Citizen]) -> Vec<u8> {
        c.iter().map(|c| c.mood).collect()
    }

    fn temple() -> BuildingFaces {
        BuildingFaces {
            government_ok: true,
            present: true,
            owned: 1,
            happy_city: 1,
            ..Default::default()
        }
    }

    #[test]
    fn face_table_matches_the_civilopedia_marketplace_list() {
        assert_eq!(&FACE_TABLE[1..=8], &[1, 2, 4, 6, 9, 12, 16, 20]);
        assert_eq!(luxury_faces(3, true), 4);
        assert_eq!(luxury_faces(3, false), 3);
        assert_eq!(luxury_faces(99, true), 32, "saturates at 11 goods");
        assert_eq!(luxury_faces(99, false), 11);
    }

    #[test]
    fn shipped_rules_match_the_dump() {
        assert_eq!(shipped::BORN_CONTENT[0], 4);
        assert_eq!(shipped::CITIZENS_PER_HAPPY_FACE, 1);
        assert_eq!(RiotRules::SHIPPED.chance, 20);
    }

    #[test]
    fn base_mood_gives_the_first_born_content_citizens_content() {
        let mut c = natives(&[0, 0, 0, 0]);
        let mut reasons = [7u8; REASONS];
        base_mood(&mut c, 2, &mut reasons);
        assert_eq!(moods(&c), [CONTENT, CONTENT, UNHAPPY, UNHAPPY]);
        assert_eq!(reasons[reason::BASE], 2);
        assert_eq!(reasons[reason::UNUSED], 0, "the byte is zeroed");
    }

    #[test]
    fn building_pass_counts_city_faces_and_negatives() {
        let mut reasons = [0u8; REASONS];
        let content = building_pass(&[temple()], &mut reasons);
        assert_eq!(content, 1);
        let unhappy = BuildingFaces {
            government_ok: true,
            present: true,
            owned: 1,
            unhappy_city: 2,
            ..Default::default()
        };
        let mut reasons = [0u8; REASONS];
        assert_eq!(building_pass(&[unhappy], &mut reasons), -2);
        assert_eq!(reasons[reason::BUILDING_CITY], 2);
    }

    #[test]
    fn martial_law_clamps_and_draft_penalties_step() {
        assert_eq!(martial_law(5, 2), 2);
        assert_eq!(martial_law(1, 2), 1);
        assert_eq!(martial_law(1, -1), 0);
        assert_eq!(draft_unhappy(0, 20), 0);
        assert_eq!(draft_unhappy(1, 20), 1);
        assert_eq!(draft_unhappy(20, 20), 1);
        assert_eq!(draft_unhappy(21, 20), 2);
    }

    #[test]
    fn war_enthusiasm_and_foreign_faces() {
        assert_eq!(war_enthusiasm(8, [-1, -1, 5]), 4);
        assert_eq!(war_enthusiasm(8, [5, 5]), 0);
        let mut c = natives(&[HAPPY, UNHAPPY, HAPPY]);
        c[1].foreign_at_war = true;
        assert_eq!(foreign_at_war(&c), 1);
        c[2].resisting = true;
        c[2].foreign_at_war = true;
        assert_eq!(foreign_at_war(&c), 1, "resisters do not count");
    }

    #[test]
    fn shift_prefers_foreign_unhappiness_and_skips_special_moods() {
        let mut c = natives(&[CONTENT, CONTENT]);
        c[1].foreign = true;
        assert_eq!(shift(&mut c, 9, 1, CONTENT, UNHAPPY), 1);
        assert_eq!(c[1].mood, UNHAPPY, "the foreign citizen is picked first");
        // Resisting and specialist moods never move.
        let mut c = natives(&[HAPPY]);
        c[0].specialist = true;
        assert_eq!(shift(&mut c, 9, 1, SPECIALIST, UNHAPPY), 0);
        assert_eq!(c[0].mood, HAPPY, "the mood was not special");
        let mut c = natives(&[SPECIALIST]);
        assert_eq!(shift(&mut c, 9, 1, SPECIALIST, UNHAPPY), 0);
        assert_eq!(c[0].mood, SPECIALIST);
        let mut c = natives(&[RESISTING]);
        assert_eq!(shift(&mut c, 9, 1, RESISTING, CONTENT), 0);
    }

    #[test]
    fn distribute_turns_faces_into_moods() {
        // Two content, +2 happy faces: both go happy.
        let mut c = natives(&[CONTENT, CONTENT]);
        distribute(&mut c, 0, 2, 0);
        assert_eq!(moods(&c), [HAPPY, HAPPY]);
        // Two unhappy, one happy face: the leftover lifts one to content.
        let mut c = natives(&[UNHAPPY, UNHAPPY]);
        distribute(&mut c, 0, 1, 0);
        assert_eq!(moods(&c), [CONTENT, UNHAPPY]);
    }

    #[test]
    fn disorder_and_celebration_steps() {
        assert!(is_disorder(&natives(&[UNHAPPY, UNHAPPY, HAPPY])));
        assert!(!is_disorder(&natives(&[UNHAPPY, HAPPY])), "ties are not disorder");
        assert!(!is_disorder(&natives(&[HAPPY, HAPPY])));
        assert_eq!(disorder_step(false, true), Disorder::Begins);
        assert_eq!(disorder_step(true, true), Disorder::Continues);
        assert_eq!(disorder_step(true, false), Disorder::Ends);
        assert_eq!(disorder_step(false, false), Disorder::Quiet);
        let happy = natives(&[HAPPY, HAPPY, HAPPY, CONTENT]);
        assert!(celebrates(6, 6, &happy, 1));
        assert!(!celebrates(6, 6, &happy, -1), "needs a non-negative surplus");
        assert_eq!(celebration_step(false, true), Celebration::Begins);
    }

    #[test]
    fn riot_spares_capitals_and_returns_a_destroyable_row() {
        let rules = RiotRules::SHIPPED;
        let mut rng = Rng::new(1);
        assert_eq!(riot(&mut rng, &rules, 20, true, 5, |_| true), None, "capital");
        let mut rng = Rng::new(2);
        let got = riot(&mut rng, &rules, 30, false, 4, |row| row == 2);
        assert_eq!(got, Some(2));
    }

    #[test]
    fn x87_percent_truncates_a_ratio() {
        assert_eq!(x87_percent(0, 10), 0);
        assert_eq!(x87_percent(1, 2), 50);
        assert_eq!(x87_percent(1, 3), 33);
        assert_eq!(x87_percent(2, 3), 66);
        assert_eq!(x87_percent(5, 5), 100);
    }

    #[test]
    fn reason_percentages_zero_out_when_nobody_is_unhappy() {
        let c = natives(&[HAPPY, HAPPY]);
        let mut reasons = [3u8; REASONS];
        reason_percentages(&c, &mut reasons);
        assert_eq!(reasons, [0; REASONS]);
        let c = natives(&[UNHAPPY, UNHAPPY]);
        let mut reasons = [0u8; REASONS];
        reasons[reason::BASE] = 1;
        reasons[reason::FOREIGN] = 1;
        reason_percentages(&c, &mut reasons);
        assert_eq!(reasons[reason::BASE], 50);
        assert_eq!(reasons[reason::FOREIGN], 50);
    }
}
