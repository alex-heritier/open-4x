//! Happiness: citizen moods, civil disorder, riots and "We Love the King Day".
//!
//! Findings, addresses and the open list are in `../happiness.md`. The city
//! keeps one **mood** per citizen (`[citizen+0x128]`, [`mood`]); the
//! recompute routine `0x4BCFF0` rebuilds every mood from scratch each time
//! the city's totals change, and the per-turn sequencer `0x4BE970` then reads
//! the moods: `0x4BDFF0` decides disorder and riots, `0x4BE440` decides the
//! celebration.
//!
//! | here | binary |
//! |---|---|
//! | [`recompute`] | `0x4BCFF0` (22 call sites and one tail jump from `0x4B10F0`) |
//! | [`base_mood`] | `0x4BDAD0` |
//! | [`building_pass`] | `0x4BD420` |
//! | [`martial_law`], [`draft_unhappy`], [`luxury_faces`] | inline in `0x4BCFF0` |
//! | [`war_enthusiasm`] | `0x4BD6D0` |
//! | war weariness | `0x4BD780`, see [`crate::government::city_weariness_unhappy`] |
//! | [`foreign_at_war`] | `0x4BD9B0` |
//! | [`shift`] | `0x4BDBE0` |
//! | [`distribute`] | `0x4BCFF0` tail, `0x4BD1C8..0x4BD35B` |
//! | [`reason_percentages`] | `0x4BCFF0` tail, `0x4BD35B..0x4BD410` |
//! | [`is_disorder`], [`disorder_step`], [`riot`] | `0x4BDFF0` |
//! | [`celebrates`], [`celebration_step`] | `0x4BE440` |
//!
//! Everything the binary reads out of the player records, the BIQ tables and
//! the tile map is a plain field of [`Inputs`], [`Citizen`] and
//! [`BuildingFaces`], so the functions are pure; the dice are the shared
//! gameplay [`Rng`].
//!
//! [`recompute`], [`is_disorder`] and [`celebrates`] were compared with the real code
//! (an x86 emulator running `0x4BCFF0`, `0x4BDFF0` and `0x4BE440` on random cities, 30 000
//! cases each, identical); what that does and does not cover is in `../happiness.md`
//! section 11. The unit tests below only prove consistency with the reading of the assembly.
//!
//! Not modelled (unread, no stubs): the unit counter `0x5A6060` mode 4
//! (callers pass the count), the luxury-source readers `0x4AC9E0` /
//! `0x4ACAE0` (see `../yields.md`), the messages, sounds and the three
//! `0x61C5A0` effect calls, `[city+0xA4]` and the queue entry at `+0x3BC`.

use crate::government::city_weariness_unhappy;
use crate::rng::Rng;

/// Citizen mood codes, `[citizen+0x128]`.
pub mod mood {
    /// Happy.
    pub const HAPPY: u8 = 0;
    /// Content.
    pub const CONTENT: u8 = 1;
    /// Unhappy.
    pub const UNHAPPY: u8 = 2;
    /// Resisting (a conquered citizen). [`super::shift`] never moves it
    /// (`0x4BDBE9..0x4BDC0B`).
    pub const RESISTING: u8 = 3;
    /// Specialist. [`super::shift`] never moves it.
    pub const SPECIALIST: u8 = 4;
}

/// The nine bytes `[city+0xCC..0xD4]`: first raw counts of what caused
/// unhappiness, then (last step of the recompute) percentages. Index `i` is
/// byte `+0xCC + i`.
pub mod reason {
    /// `+0xCC`. Zeroed by `0x4BDAD0`; **no writer found**.
    pub const UNUSED: usize = 0;
    /// `+0xCD`. Citizens born unhappy by the difficulty (`0x4BDAD0`).
    pub const BASE: usize = 1;
    /// `+0xCE`. War weariness (`0x4BD780`).
    pub const WAR_WEARINESS: usize = 2;
    /// `+0xCF`. Foreign nationals of a civ at war with the owner
    /// (`0x4BD9B0`); also the limit of the first pass of [`super::shift`].
    pub const FOREIGN: usize = 3;
    /// `+0xD0`. Propaganda (`[city+0x1C0]`).
    pub const PROPAGANDA: usize = 4;
    /// `+0xD1`. The draft (`[city+0x70]`).
    pub const DRAFT: usize = 5;
    /// `+0xD2`. The hurry sacrifice (`[city+0x1C4]`).
    pub const HURRY: usize = 6;
    /// `+0xD3`. Unhappy faces of buildings in the city (BLDG `+0xBC`).
    pub const BUILDING_CITY: usize = 7;
    /// `+0xD4`. Unhappy faces of buildings in all cities (BLDG `+0xB8`).
    pub const BUILDING_ALL: usize = 8;
}

/// Number of reason bytes, `[city+0xCC..0xD4]`.
pub const REASONS: usize = 9;

/// Luxury resources counted at most (`0x4BD116: cmp ebx, 0xB; jge`).
pub const MAX_LUXURY_GOODS: usize = 11;

/// Happy faces given by `n` distinct usable luxury resources in a city with a
/// BLDG that has improvement flag `0x400` (the Marketplace): the table at
/// `0x665868`. The Civilopedia's Marketplace entry lists the same numbers
/// for 1 to 8.
pub const FACE_TABLE: [i32; 12] = [0, 1, 2, 4, 6, 9, 12, 16, 20, 24, 28, 32];

/// BLDG `+0xEC` (improvement flags) bit tested by the luxury step
/// (`0x4BD133: push 0x400; call 0x4B1F90`).
pub const LUXURY_TRADE_FLAG: u32 = 0x400;

/// BLDG `+0xEC` bit 22, "Reduces War Weariness" (`0x4BD8E5: test ..., 0x400000`).
pub const REDUCES_WAR_WEARINESS: u32 = 0x40_0000;

/// What the shipped rules put in the RULE record for this system
/// (`conquests.biq` and 26 of the 27 other scenarios that carry a RULE
/// section; one has `draft_turn_penalty` 10).
pub mod shipped {
    /// RULE `+0x7C` (global `0x9C7260`): `chance_of_rioting`, read at `0x4BE0B0`.
    pub const CHANCE_OF_RIOTING: i32 = 20;
    /// RULE `+0x80` (`0x9C7264`): `draft_turn_penalty`, read at `0x4BD028`.
    pub const DRAFT_TURN_PENALTY: i32 = 20;
    /// RULE `+0x8C` (`0x9C7270`): `citizens_per_happy_face`, read at
    /// `0x4BD0D9`. It divides the **luxury commerce**, so 1 point of luxury
    /// is one face.
    pub const CITIZENS_PER_HAPPY_FACE: i32 = 1;
    /// RULE `+0xF0` (`0x9C72D4`): `wltk_min_population`, read at `0x4BE446`.
    pub const WLTK_MIN_POPULATION: i32 = 6;
    /// RULE `+0xD8` (`0x9C72BC`): `hurry_sacrifice_turn_penalty`.
    pub const HURRY_SACRIFICE_TURN_PENALTY: i32 = 20;
    /// RULE `+0x100` (`0x9C72E4`): `town_max_size`.
    pub const TOWN_MAX_SIZE: i32 = 6;
    /// RULE `+0x104` (`0x9C72E8`): `city_max_size`.
    pub const CITY_MAX_SIZE: i32 = 12;
    /// DIFF `+0x44` in memory: `citizens_born_content` by difficulty
    /// (Chieftain to Sid).
    pub const BORN_CONTENT: [i32; 8] = [4, 3, 2, 2, 1, 1, 1, 1];
    /// GOVT `+0x1AC`: `military_police_limit` (Anarchy, Despotism,
    /// Monarchy, Communism, Republic, Democracy, Fascism, Feudalism).
    pub const MILITARY_POLICE_LIMIT: [i32; 8] = [0, 2, 3, 4, 0, 0, 4, 3];
}

/// One citizen as the mood code sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Citizen {
    /// `[citizen+0x128]`, see [`mood`].
    pub mood: u8,
    /// Byte `[citizen+0x20] != 0`: a resister.
    pub resisting: bool,
    /// `0x4ABE70`: the citizen's job `[citizen+0x13C]` is not the default
    /// worker `[0x9C3D64]`.
    pub specialist: bool,
    /// The citizen's race `[citizen+0x140]` differs from the owner's
    /// `Player+0x20`.
    pub foreign: bool,
    /// [`Citizen::foreign`], and the race's civ (`0x539D60`) is in play and
    /// at war with the owner (`Player+0xD30[civ] != 0`).
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

/// One BLDG row as `0x4BD420` sees it for one city: the table values plus
/// the answers of the helper calls, already resolved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BuildingFaces {
    /// `other_characteristics & 4` (BLDG `+0xF0`): a great wonder.
    pub wonder: bool,
    /// BLDG `+0xD4` is `-1` or the owner's government (`Player+0xA0`).
    pub government_ok: bool,
    /// BLDG `+0xE0` is a tech and the owner knows it (`0x561440`).
    pub obsolete: bool,
    /// `0x4ACB50(city, i, 1)`: the building is in the city, or granted to it
    /// by a player-wide or continent-wide effect, under the right government.
    pub present: bool,
    /// `Player+0x15DC[i]` (a word table): how many the owner has.
    pub owned: i32,
    /// `other_characteristics & 0x10`: the effect is scaled by the owner's
    /// cities on this continent instead of by `owned`.
    pub continental: bool,
    /// `0x4BDDE0(city, i)`: other cities of the owner on this continent that
    /// have it. Read only when [`BuildingFaces::continental`].
    pub on_continent: i32,
    /// `0x55A7E0(owner, i)`: the owner holds a live wonder whose
    /// `doubles_happiness_of` (BLDG `+0x84`) is this building.
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

/// `0x4BDAD0`: zero the reason bytes, then give every ordinary citizen (not
/// a resister, not a specialist) in list order a base mood: the first
/// `born_content` are content, the rest unhappy. The unhappy ones are added
/// to [`reason::BASE`]. Resisters and specialists keep their mood.
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

/// `0x4BD420(&content, _)`: the content faces buildings add (+) or take away
/// (-), for one city. Returns the change of the content accumulator and adds
/// the unhappy faces to [`reason::BUILDING_CITY`] / [`reason::BUILDING_ALL`].
///
/// Per building, in table order:
/// 1. a wonder whose government does not fit, or that the owner has made
///    obsolete, is skipped;
/// 2. if the building is [`present`](BuildingFaces::present) and not
///    obsolete, the count `n` of "other copies" is `owned - 1` and the
///    in-city faces count once;
/// 3. with `n != 0` (replaced by `on_continent` for continental effects) the
///    all-cities faces count `n` times.
///
/// A [`doubled`](BuildingFaces::doubled) building counts twice, both signs.
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
                reasons[reason::BUILDING_CITY] = reasons[reason::BUILDING_CITY].wrapping_add(v as u8);
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

/// Martial law (`0x4BD0A0..0x4BD0B6`): the units on the city tile (land
/// units for which `0x5BE6E0 > 0` or `0x5BE820 > 0`, any owner;
/// `0x5A6060(x, y, 4, -1, 0, -1)`), clamped to `0..=limit` (GOVT `+0x1AC`);
/// a negative limit gives 0. Each counted unit is one content face. Only
/// applied when the city has no resister.
pub fn martial_law(units: i32, limit: i32) -> i32 {
    if limit < 0 || units < 0 {
        0
    } else {
        units.min(limit)
    }
}

/// The draft and hurry-sacrifice penalties (`0x4BD019..0x4BD041`,
/// `0x4BD174..0x4BD195`): a timer `t > 0` costs `(t - 1) / penalty + 1`
/// happy faces. `penalty` is RULE `draft_turn_penalty` or
/// `hurry_sacrifice_turn_penalty`; zero would fault (`idiv`) in the binary.
pub fn draft_unhappy(timer: i32, penalty: i32) -> i32 {
    if timer > 0 {
        (timer - 1) / penalty + 1
    } else {
        0
    }
}

/// Happy faces from luxury resources (`0x4BD0E3..0x4BD14C`, the same count
/// as `0x4BAF80`, `../yields.md` section 8): `goods` usable luxury resources
/// (at most [`MAX_LUXURY_GOODS`]), run through [`FACE_TABLE`] when the city
/// has a building with [`LUXURY_TRADE_FLAG`].
pub fn luxury_faces(goods: usize, has_luxury_trade_building: bool) -> i32 {
    let n = goods.min(MAX_LUXURY_GOODS);
    if has_luxury_trade_building {
        FACE_TABLE[n]
    } else {
        n as i32
    }
}

/// `0x4BD6D0`: each civ in play and at war with the owner whose war counter
/// (`Player+0xCB4[civ-1]`) is **negative** gives `size / 4` happy faces; the
/// sum is capped at `size`. `counters` are those of the civs in play and at
/// war.
pub fn war_enthusiasm(size: i32, counters: impl IntoIterator<Item = i32>) -> i32 {
    let total: i32 = counters.into_iter().filter(|&c| c < 0).map(|_| size / 4).sum();
    total.min(size)
}

/// `0x4BD9B0`: the citizens who are not resisters and whose race belongs to a
/// civ at war with the owner. Each is one happy face lost; specialists
/// count.
pub fn foreign_at_war(citizens: &[Citizen]) -> i32 {
    citizens
        .iter()
        .filter(|c| !c.resisting && c.foreign_at_war)
        .count() as i32
}

/// `0x4BDBE0(count, from, to)`: move up to `count` citizens from mood `from`
/// to mood `to`; returns how many moved. `foreign_limit` is the byte
/// `[city+0xCF]` ([`reason::FOREIGN`]).
///
/// Resisting and specialist moods are neither sources nor targets (0). Only
/// ordinary citizens (no resister, no specialist) with mood `from` move.
/// Pass 1 runs while fewer than `foreign_limit` have moved: it prefers
/// foreign-race citizens when making someone **unhappy** (`to == 2`) and
/// own-race citizens when making someone **less unhappy** (`from == 2`).
/// Pass 2 takes whoever is left, in list order.
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

/// The tail of `0x4BCFF0` (`0x4BD1C8..0x4BD35B`): turn the two accumulators
/// into mood changes. `a` is the **happy-face** accumulator (`[esp+0x10]`:
/// luxury, the draft, propaganda, the war terms, foreign nationals) and `b`
/// the **content-face** accumulator (`[esp+0x14]`: buildings, martial law).
///
/// Four regions, in the order the code tests them:
///
/// | region | what happens |
/// |---|---|
/// | `a >= 0, b <= 0` | the smaller of the two cancels the other, then the net happy faces go up (below), and a net content deficit turns happy into content |
/// | `a <= 0, b >= 0` (not both 0) | the net unhappy faces go down (below), then the net content faces turn unhappy into content |
/// | `a > 0, b > 0` | content faces first (unhappy to content), then happy faces up |
/// | `a < 0, b < 0` | happy to content for `-b`, then the unhappy faces go down |
///
/// "Up" with `n` faces: content to happy for `n`; the `m` that moved leave
/// `n - m`, and each **two** left over lift one unhappy citizen to happy
/// (`(n - m) / 2`, truncating); one face still left over (`n > m + 2 m2`)
/// lifts one unhappy citizen to content. "Down" is the mirror image.
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

/// The percentage the binary stores for byte `b` of a total `sum`
/// (`0x4BD3D8..0x4BD3FC`): `fild sum; fild b; fdiv st(1); fmul 100.0f;
/// _ftol`, i.e. truncate `(b / sum) * 100` with **every x87 step rounded to
/// a 64-bit mantissa** (the MSVC default precision). That differs from the
/// exact `b * 100 / sum` at eight inputs with `b <= 255` and `sum <= 2295`
/// (53 or 59 of 100, and the multiples of 100): 0.53 and 0.59 round to just
/// below the exact value and come out one lower.
///
/// **HYPOTHESIS:** the game runs with the default 64-bit precision; at 53
/// bits twenty inputs differ.
pub fn x87_percent(b: u32, sum: u32) -> u8 {
    if b == 0 {
        return 0;
    }
    let (n, d) = (u128::from(b), u128::from(sum));
    // q = n / d rounded to 64 significant bits, as m / 2^s with 2^63 <= m < 2^64
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
    // p = m * 100 / 2^s, rounded to 64 significant bits, then truncated
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

/// The last step of `0x4BCFF0` (`0x4BD35B..0x4BD410`): when no citizen is
/// unhappy the nine bytes are zeroed; otherwise each becomes its percentage
/// of their sum ([`x87_percent`]); an all-zero sum is left alone. The UI's
/// `CITIZEN_UNHAPPY_REASONS` reads the result.
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

/// Everything `0x4BCFF0` reads, resolved. `citizens` is in list order
/// (`[city+0xE0]`, index `0..=[city+0xEC]`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inputs {
    /// The citizens, in list order.
    pub citizens: Vec<Citizen>,
    /// `[city+0x138]`. **HYPOTHESIS:** equals the length of `citizens`.
    pub size: i32,
    /// DIFF `+0x44` for the owner's level (`Player+0x30`): see
    /// [`shipped::BORN_CONTENT`].
    pub born_content: i32,
    /// One entry per BLDG row, in table order.
    pub buildings: Vec<BuildingFaces>,
    /// `[city+0x70]`: the draft timer.
    pub draft_timer: i32,
    /// RULE `draft_turn_penalty`.
    pub draft_penalty: i32,
    /// The units counted for martial law ([`martial_law`]).
    pub units_on_tile: i32,
    /// GOVT `military_police_limit`.
    pub police_limit: i32,
    /// `0x4AC9E0(city,0,0) + 0x4ACAE0(city,0,0)`: luxury commerce plus the
    /// specialists' luxury.
    pub luxury_points: i32,
    /// RULE `citizens_per_happy_face`.
    pub citizens_per_face: i32,
    /// Usable luxury resources ([`luxury_faces`]).
    pub luxury_goods: usize,
    /// The city has a building with [`LUXURY_TRADE_FLAG`] (`0x4B1F90 > 0`).
    pub luxury_trade_building: bool,
    /// `[city+0x1C0]`: propaganda unhappiness, in faces.
    pub propaganda: i32,
    /// `[city+0x1C4]`: the hurry-sacrifice timer.
    pub hurry_timer: i32,
    /// RULE `hurry_sacrifice_turn_penalty`.
    pub hurry_penalty: i32,
    /// `Player+0xCB4[civ-1]` for every civ in play that the owner is at war
    /// with.
    pub war_counters: Vec<i32>,
    /// GOVT `+0x1E4`: the weariness class (0 none, 1 low, 2 high).
    pub weariness_class: i32,
    /// Buildings in the city with [`REDUCES_WAR_WEARINESS`], present and not
    /// obsolete.
    pub police_buildings: i32,
    /// Owned wonders with the "reduces war weariness everywhere" flag
    /// (`0x55A8D0(0x800, 0)`).
    pub suffrage: i32,
}

/// What `0x4BCFF0` leaves behind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    /// The citizens with their new moods.
    pub citizens: Vec<Citizen>,
    /// `[city+0xCC..0xD4]` after the percentage step.
    pub reasons: [u8; REASONS],
    /// The happy-face accumulator that was distributed.
    pub happy_faces: i32,
    /// The content-face accumulator that was distributed.
    pub content_faces: i32,
}

/// `0x4BCFF0`, in the order the binary runs it. Resisters present in the
/// city switch martial law off.
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

/// The disorder condition (`0x4BDFF0`, `0x4BE067`): **strictly more unhappy
/// than happy** citizens, counted over the mood field of every citizen.
pub fn is_disorder(citizens: &[Citizen]) -> bool {
    count(citizens, mood::HAPPY) < count(citizens, mood::UNHAPPY)
}

/// What the per-turn disorder check does to the city flag
/// (`[city+0x30]` bit 0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disorder {
    /// Not in disorder, and not now.
    Quiet,
    /// The flag is set this turn (`0x4BE356..0x4BE41C`).
    Begins,
    /// Still in disorder: the riot roll runs (`0x4BE0B0`).
    Continues,
    /// The flag is cleared (`0x4BE2CD`).
    Ends,
}

/// The four-way branch of `0x4BDFF0` (`0x4BE0A2`, `0x4BE0A8`, `0x4BE2BD`,
/// `0x4BE34E`).
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
    /// `chance_of_rioting` (`0x9C7260`).
    pub chance: i32,
    /// `town_max_size` (`0x9C72E4`).
    pub town_max: i32,
    /// `city_max_size` (`0x9C72E8`).
    pub city_max: i32,
}

impl RiotRules {
    /// The shipped values ([`shipped`]).
    pub const SHIPPED: RiotRules = RiotRules {
        chance: shipped::CHANCE_OF_RIOTING,
        town_max: shipped::TOWN_MAX_SIZE,
        city_max: shipped::CITY_MAX_SIZE,
    };
}

/// BLDG `other_characteristics` bits the riot refuses (`0x4BE17C..0x4BE199`).
const WONDER: u32 = 4;
const SMALL_WONDER: u32 = 8;
/// BLDG `+0xEC` bit 0 (the palace) and bits 11 and 12 (the size-limit
/// buildings): the riot never destroys them (`0x4BE192`, `0x4BE196`).
const NOT_DESTROYED_FLAGS: u32 = 0x1801;

/// Whether a riot may destroy the building `i` of the city: it is in the
/// city's own set (`0x4ACB50(city, i, 0)`), and is neither a wonder, nor a
/// small wonder, nor carries improvement flag bits 0, 11 or 12.
pub fn riot_can_destroy(in_city: bool, other_characteristics: u32, improvement_flags: u32) -> bool {
    in_city
        && other_characteristics & (WONDER | SMALL_WONDER) == 0
        && improvement_flags & NOT_DESTROYED_FLAGS == 0
}

/// The riot roll of a city that stays in disorder (`0x4BE0B0..0x4BE209`).
/// Returns the building row to destroy, if any.
///
/// 1. `rng(100) < chance`, else nothing;
/// 2. the owner's capital is spared;
/// 3. a city of at most `town_max` **and** `city_max` citizens needs a
///    second roll `rng(100) < 2 * chance`;
/// 4. up to `size + 21` draws of `rng(n_buildings)`; the first row for which
///    `destroyable(row)` holds is the victim.
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

/// The celebration test (`0x4BE440`): the city has at least
/// `wltk_min_population` citizens (RULE `+0xF0`), nobody unhappy, strictly
/// more happy than content citizens, no resister, and a food surplus
/// `[city+0x250]` that is not negative (the value of the **previous** recompute:
/// `0x4BE970` refreshes it after this test).
pub fn celebrates(size: i32, wltk_min_population: i32, citizens: &[Citizen], food_surplus: i32) -> bool {
    size >= wltk_min_population
        && count(citizens, mood::UNHAPPY) == 0
        && count(citizens, mood::HAPPY) > count(citizens, mood::CONTENT)
        && !citizens.iter().any(|c| c.resisting)
        && food_surplus >= 0
}

/// What the celebration check does to the city flag (`[city+0x30]` bit 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Celebration {
    /// Neither before nor now.
    Quiet,
    /// The flag is set and "We Love the King Day" starts (`0x4BE56A`).
    Begins,
    /// Already set, still qualifies; nothing happens (`0x4BE564`).
    Continues,
    /// The flag is cleared (`0x4BE6AB`).
    Ends,
}

/// The branch structure of `0x4BE440`.
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

    // --- tables --------------------------------------------------------

    #[test]
    fn face_table_matches_the_civilopedia_marketplace_list() {
        // Civilopedia.txt, #BLDG_Marketplace: 1=1 2=2 3=4 4=6 5=9 6=12 7=16 8=20.
        assert_eq!(&FACE_TABLE[1..=8], &[1, 2, 4, 6, 9, 12, 16, 20]);
        assert_eq!(luxury_faces(3, true), 4);
        assert_eq!(luxury_faces(3, false), 3);
        assert_eq!(luxury_faces(99, true), 32, "saturates at 11 goods");
        assert_eq!(luxury_faces(99, false), 11);
    }

    #[test]
    fn shipped_rules_match_the_dump() {
        use shipped::*;
        assert_eq!(BORN_CONTENT.len(), MILITARY_POLICE_LIMIT.len());
        assert_eq!(BORN_CONTENT[0], 4);
        assert_eq!(CITIZENS_PER_HAPPY_FACE, 1);
        assert_eq!(RiotRules::SHIPPED.chance, 20);
    }

    // --- base mood -------------------------------------------------------

    #[test]
    fn base_mood_gives_the_first_born_content_citizens_content() {
        // Chieftain 4, Emperor 1.
        for (born, want_unhappy) in [(4, 2u8), (1, 5)] {
            let mut c = natives(&[HAPPY; 6]);
            let mut r = [9u8; REASONS];
            base_mood(&mut c, born, &mut r);
            assert_eq!(count(&c, CONTENT), born);
            assert_eq!(count(&c, UNHAPPY), i32::from(want_unhappy));
            assert_eq!(r[reason::BASE], want_unhappy);
            assert_eq!(r[reason::UNUSED], 0, "all nine bytes are zeroed first");
            assert_eq!(&moods(&c)[..born as usize], &vec![CONTENT; born as usize][..]);
        }
    }

    #[test]
    fn base_mood_skips_resisters_and_specialists() {
        let mut c = natives(&[RESISTING, SPECIALIST, HAPPY, HAPPY]);
        c[0].resisting = true;
        c[1].specialist = true;
        let mut r = [0u8; REASONS];
        base_mood(&mut c, 1, &mut r);
        assert_eq!(moods(&c), [RESISTING, SPECIALIST, CONTENT, UNHAPPY]);
    }

    // --- shift ----------------------------------------------------------

    #[test]
    fn shift_refuses_resisting_and_specialist_moods() {
        let mut c = natives(&[CONTENT, CONTENT]);
        assert_eq!(shift(&mut c, 0, 2, CONTENT, SPECIALIST), 0);
        assert_eq!(shift(&mut c, 0, 2, RESISTING, HAPPY), 0);
        assert_eq!(moods(&c), [CONTENT, CONTENT]);
    }

    #[test]
    fn shift_moves_in_list_order_and_stops_at_count() {
        let mut c = natives(&[CONTENT, UNHAPPY, CONTENT, CONTENT]);
        assert_eq!(shift(&mut c, 0, 2, CONTENT, HAPPY), 2);
        assert_eq!(moods(&c), [HAPPY, UNHAPPY, HAPPY, CONTENT]);
    }

    #[test]
    fn shift_leaves_resisters_and_specialists_alone() {
        let mut c = natives(&[CONTENT, CONTENT, CONTENT]);
        c[0].resisting = true;
        c[1].specialist = true;
        assert_eq!(shift(&mut c, 0, 5, CONTENT, HAPPY), 1);
        assert_eq!(moods(&c), [CONTENT, CONTENT, HAPPY]);
    }

    #[test]
    fn first_pass_prefers_foreigners_when_making_unhappy() {
        // Two natives first in the list, a foreigner last; limit 1.
        let mut c = natives(&[CONTENT, CONTENT, CONTENT]);
        c[2].foreign = true;
        assert_eq!(shift(&mut c, 1, 1, CONTENT, UNHAPPY), 1);
        assert_eq!(moods(&c), [CONTENT, CONTENT, UNHAPPY], "the foreigner goes first");
        // Without the limit the list order decides.
        let mut c = natives(&[CONTENT, CONTENT, CONTENT]);
        c[2].foreign = true;
        shift(&mut c, 0, 1, CONTENT, UNHAPPY);
        assert_eq!(moods(&c), [UNHAPPY, CONTENT, CONTENT]);
    }

    #[test]
    fn first_pass_prefers_natives_when_relieving_unhappiness() {
        let mut c = natives(&[UNHAPPY, UNHAPPY, UNHAPPY]);
        c[0].foreign = true;
        assert_eq!(shift(&mut c, 1, 1, UNHAPPY, CONTENT), 1);
        assert_eq!(moods(&c), [UNHAPPY, CONTENT, UNHAPPY]);
    }

    #[test]
    fn first_pass_is_limited_but_the_second_finishes_the_job() {
        let mut c = natives(&[CONTENT, CONTENT, CONTENT]);
        c[1].foreign = true;
        c[2].foreign = true;
        // limit 1: one foreigner in pass 1, the other two in pass 2 by list order.
        assert_eq!(shift(&mut c, 1, 3, CONTENT, UNHAPPY), 3);
        assert_eq!(moods(&c), [UNHAPPY; 3]);
    }

    // --- distribution -------------------------------------------------------

    #[test]
    fn the_civilopedia_carry_over_example() {
        // "a city has the capability to generate 5 happy faces, yet there are
        // only 2 content faces ... those 2 content faces will become happy and
        // the remaining 3 happy faces will carry over to unhappy citizens at
        // reduced efficiency."  2 content + 4 unhappy, a = 5, b = 0:
        // 2 content -> happy; (5 - 2) / 2 = 1 unhappy -> happy; the odd face
        // makes one unhappy citizen content.
        let mut c = natives(&[CONTENT, CONTENT, UNHAPPY, UNHAPPY, UNHAPPY, UNHAPPY]);
        distribute(&mut c, 0, 5, 0);
        assert_eq!(count(&c, HAPPY), 3);
        assert_eq!(count(&c, CONTENT), 1);
        assert_eq!(count(&c, UNHAPPY), 2);
    }

    #[test]
    fn positive_content_faces_come_before_happy_faces() {
        // a > 0, b > 0: unhappy -> content first, then content -> happy.
        let mut c = natives(&[UNHAPPY, UNHAPPY, CONTENT]);
        distribute(&mut c, 0, 1, 1);
        // b=1 turns the first unhappy content; then a=1 makes the first content happy.
        assert_eq!(moods(&c), [HAPPY, UNHAPPY, CONTENT]);
    }

    #[test]
    fn a_content_deficit_cancels_happy_faces_first() {
        // a >= 0, b <= 0 with a > -b: net a' = a + b, nothing left of b.
        let mut c = natives(&[CONTENT, CONTENT, CONTENT]);
        distribute(&mut c, 0, 3, -2);
        assert_eq!(count(&c, HAPPY), 1);
        // a < -b: a' = 0, b' = a + b; happy citizens become content.
        let mut c = natives(&[HAPPY, HAPPY, HAPPY]);
        distribute(&mut c, 0, 1, -3);
        assert_eq!(count(&c, CONTENT), 2, "b' = -2");
    }

    #[test]
    fn negative_faces_make_citizens_unhappy_and_carry_over() {
        // a < 0, b < 0 (region 4): -b happy -> content, then -a content -> unhappy.
        let mut c = natives(&[HAPPY, CONTENT, CONTENT, CONTENT]);
        distribute(&mut c, 0, -2, -1);
        // [H C C C] -> [C C C C] -> the first two content turn unhappy.
        assert_eq!(moods(&c), [UNHAPPY, UNHAPPY, CONTENT, CONTENT]);
        // Not enough content: happy citizens fall two steps per two faces.
        let mut c = natives(&[HAPPY, HAPPY, HAPPY]);
        distribute(&mut c, 0, -4, 0);
        // a<=0,b>=0 region: m = 0 content; (4 - 0)/2 = 2 happy -> unhappy.
        assert_eq!(count(&c, UNHAPPY), 2);
        assert_eq!(count(&c, HAPPY), 1);
    }

    #[test]
    fn zero_zero_changes_nothing() {
        let mut c = natives(&[HAPPY, CONTENT, UNHAPPY]);
        distribute(&mut c, 0, 0, 0);
        assert_eq!(moods(&c), [HAPPY, CONTENT, UNHAPPY]);
    }

    // --- buildings -------------------------------------------------------

    #[test]
    fn a_temple_gives_one_content_face() {
        let mut r = [0u8; REASONS];
        assert_eq!(building_pass(&[temple()], &mut r), 1);
        assert_eq!(r, [0; REASONS]);
    }

    #[test]
    fn a_doubling_wonder_doubles_the_building() {
        let mut r = [0u8; REASONS];
        let cathedral = BuildingFaces {
            happy_city: 3,
            doubled: true,
            ..temple()
        };
        assert_eq!(building_pass(&[cathedral], &mut r), 6);
    }

    #[test]
    fn unhappy_faces_subtract_and_are_recorded() {
        let mut r = [0u8; REASONS];
        let b = BuildingFaces {
            unhappy_city: 2,
            happy_city: 0,
            ..temple()
        };
        assert_eq!(building_pass(&[b], &mut r), -2);
        assert_eq!(r[reason::BUILDING_CITY], 2);
        // doubled: both signs double.
        let mut r = [0u8; REASONS];
        let b = BuildingFaces { doubled: true, ..b };
        assert_eq!(building_pass(&[b], &mut r), -4);
        assert_eq!(r[reason::BUILDING_CITY], 4);
    }

    #[test]
    fn an_obsolete_present_building_gives_nothing_in_the_city() {
        let mut r = [0u8; REASONS];
        let b = BuildingFaces {
            obsolete: true,
            ..temple()
        };
        assert_eq!(building_pass(&[b], &mut r), 0);
    }

    #[test]
    fn wonders_are_skipped_under_the_wrong_government_or_when_obsolete() {
        let mut r = [0u8; REASONS];
        let wonder = BuildingFaces {
            wonder: true,
            happy_city: 2,
            ..temple()
        };
        assert_eq!(building_pass(&[wonder], &mut r), 2);
        let wrong = BuildingFaces {
            government_ok: false,
            ..wonder
        };
        assert_eq!(building_pass(&[wrong], &mut r), 0);
        let old = BuildingFaces {
            obsolete: true,
            ..wonder
        };
        assert_eq!(building_pass(&[old], &mut r), 0);
    }

    #[test]
    fn all_cities_faces_count_the_other_copies() {
        let mut r = [0u8; REASONS];
        // Owned in 3 cities, present here: n = 2 others, 1 face each.
        let b = BuildingFaces {
            owned: 3,
            happy_city: 0,
            happy_all: 1,
            ..temple()
        };
        assert_eq!(building_pass(&[b], &mut r), 2);
        // Not present here: all 3 count.
        let b = BuildingFaces { present: false, ..b };
        assert_eq!(building_pass(&[b], &mut r), 3);
        // Owned only here and present: n = 0, nothing.
        let b = BuildingFaces {
            owned: 1,
            present: true,
            ..b
        };
        assert_eq!(building_pass(&[b], &mut r), 0);
    }

    #[test]
    fn continental_effects_use_the_continent_count() {
        let mut r = [0u8; REASONS];
        let b = BuildingFaces {
            owned: 1,
            present: false,
            happy_city: 0,
            happy_all: 2,
            continental: true,
            on_continent: 3,
            ..temple()
        };
        assert_eq!(building_pass(&[b], &mut r), 6);
        let b = BuildingFaces { on_continent: 0, ..b };
        assert_eq!(building_pass(&[b], &mut r), 0);
    }

    // --- the other terms ---------------------------------------------------

    #[test]
    fn martial_law_is_clamped_by_the_government() {
        assert_eq!(martial_law(5, 3), 3);
        assert_eq!(martial_law(2, 3), 2);
        assert_eq!(martial_law(5, 0), 0);
        assert_eq!(martial_law(5, -1), 0);
        assert_eq!(martial_law(-1, 3), 0);
    }

    #[test]
    fn draft_and_sacrifice_penalties_step_every_penalty_turns() {
        // timer 1..=20 -> 1, 21..=40 -> 2 with the shipped 20.
        assert_eq!(draft_unhappy(0, 20), 0);
        assert_eq!(draft_unhappy(1, 20), 1);
        assert_eq!(draft_unhappy(20, 20), 1);
        assert_eq!(draft_unhappy(21, 20), 2);
        assert_eq!(draft_unhappy(40, 20), 2);
        assert_eq!(draft_unhappy(41, 20), 3);
    }

    #[test]
    fn war_enthusiasm_counts_negative_counters_up_to_the_size() {
        assert_eq!(war_enthusiasm(12, [-5, 10, -1]), 6);
        assert_eq!(war_enthusiasm(12, [10, 0]), 0);
        assert_eq!(war_enthusiasm(8, [-1, -1, -1, -1]), 8);
        assert_eq!(war_enthusiasm(8, [-1, -1, -1, -1, -1]), 8, "capped at the size");
    }

    #[test]
    fn foreign_nationals_at_war_cost_a_face_each_specialists_included() {
        let mut c = natives(&[CONTENT; 4]);
        c[0].foreign_at_war = true;
        c[1].foreign_at_war = true;
        c[1].specialist = true;
        c[2].foreign_at_war = true;
        c[2].resisting = true;
        assert_eq!(foreign_at_war(&c), 2);
    }

    // --- percentages --------------------------------------------------------

    #[test]
    fn x87_percent_matches_exact_division_except_at_eight_inputs() {
        let mut deviating = Vec::new();
        for sum in 1..=9 * 255u32 {
            for b in 0..=sum.min(255) {
                let exact = (b * 100 / sum) as u8;
                let got = x87_percent(b, sum);
                if got != exact {
                    deviating.push((b, sum, got, exact));
                }
            }
        }
        // Found by exact rational emulation of 64-bit x87 rounding.
        assert_eq!(
            deviating,
            [
                (53, 100, 52, 53),
                (59, 100, 58, 59),
                (106, 200, 52, 53),
                (118, 200, 58, 59),
                (159, 300, 52, 53),
                (177, 300, 58, 59),
                (212, 400, 52, 53),
                (236, 400, 58, 59),
            ]
        );
    }

    #[test]
    fn x87_percent_known_values() {
        assert_eq!(x87_percent(1, 2), 50);
        assert_eq!(x87_percent(1, 3), 33);
        assert_eq!(x87_percent(2, 3), 66);
        assert_eq!(x87_percent(7, 7), 100);
        assert_eq!(x87_percent(0, 7), 0);
    }

    #[test]
    fn percentages_are_zeroed_without_an_unhappy_citizen() {
        let mut r = [3, 0, 0, 1, 0, 0, 0, 0, 0];
        reason_percentages(&natives(&[HAPPY, CONTENT]), &mut r);
        assert_eq!(r, [0; REASONS]);
        let mut r = [3, 0, 0, 1, 0, 0, 0, 0, 0];
        reason_percentages(&natives(&[UNHAPPY]), &mut r);
        assert_eq!(r, [75, 0, 0, 25, 0, 0, 0, 0, 0]);
        let mut r = [0; REASONS];
        reason_percentages(&natives(&[UNHAPPY]), &mut r);
        assert_eq!(r, [0; REASONS], "an all-zero sum is left alone");
    }

    // --- the whole recompute ----------------------------------------------------

    fn city(n: usize, born_content: i32) -> Inputs {
        Inputs {
            citizens: natives(&vec![HAPPY; n]),
            size: n as i32,
            born_content,
            buildings: vec![],
            draft_timer: 0,
            draft_penalty: shipped::DRAFT_TURN_PENALTY,
            units_on_tile: 0,
            police_limit: 0,
            luxury_points: 0,
            citizens_per_face: shipped::CITIZENS_PER_HAPPY_FACE,
            luxury_goods: 0,
            luxury_trade_building: false,
            propaganda: 0,
            hurry_timer: 0,
            hurry_penalty: shipped::HURRY_SACRIFICE_TURN_PENALTY,
            war_counters: vec![],
            weariness_class: 0,
            police_buildings: 0,
            suffrage: 0,
        }
    }

    #[test]
    fn a_bare_size_six_city_on_chieftain_has_two_unhappy_citizens() {
        let out = recompute(&city(6, 4));
        assert_eq!(moods(&out.citizens), [CONTENT, CONTENT, CONTENT, CONTENT, UNHAPPY, UNHAPPY]);
        assert_eq!(out.reasons[reason::BASE], 100, "all of it is base unhappiness");
        // 0 happy < 2 unhappy: disorder.
        assert!(is_disorder(&out.citizens));
    }

    #[test]
    fn a_temple_and_martial_law_cure_it() {
        let mut inp = city(6, 4);
        inp.buildings = vec![temple()];
        inp.units_on_tile = 5;
        inp.police_limit = 3; // Monarchy
        let out = recompute(&inp);
        // content faces 1 + 3 = 4 >= 2 unhappy: both become content, 2 spare faces.
        assert_eq!(count(&out.citizens, UNHAPPY), 0);
        assert_eq!(out.content_faces, 4);
        assert_eq!(out.reasons, [0; REASONS], "nobody unhappy: reasons are zeroed");
        assert!(!is_disorder(&out.citizens));
    }

    #[test]
    fn a_resister_switches_martial_law_off() {
        let mut inp = city(6, 4);
        inp.citizens[5].resisting = true;
        inp.units_on_tile = 5;
        inp.police_limit = 3;
        let out = recompute(&inp);
        assert_eq!(out.content_faces, 0);
    }

    #[test]
    fn luxury_makes_content_citizens_happy_and_ends_disorder() {
        let mut inp = city(6, 4);
        inp.luxury_points = 4; // one face per point
        let out = recompute(&inp);
        // a = 4, b = 0: 4 content -> happy (m = 4), (4 - 4)/2 = 0.
        assert_eq!(count(&out.citizens, HAPPY), 4);
        assert_eq!(count(&out.citizens, UNHAPPY), 2);
        assert!(!is_disorder(&out.citizens), "4 happy vs 2 unhappy");
    }

    #[test]
    fn propaganda_draft_and_hurry_each_record_their_reason() {
        let mut inp = city(8, 4);
        inp.propaganda = 2;
        inp.draft_timer = 25; // 2 faces
        inp.hurry_timer = 5; // 1 face
        let out = recompute(&inp);
        assert_eq!(out.happy_faces, -(2 + 2 + 1));
        // base unhappy 4 + propaganda 2 + draft 2 + hurry 1 = 9: 4/9, 2/9, 2/9, 1/9.
        assert_eq!(out.reasons[reason::BASE], 44);
        assert_eq!(out.reasons[reason::PROPAGANDA], 22);
        assert_eq!(out.reasons[reason::DRAFT], 22);
        assert_eq!(out.reasons[reason::HURRY], 11);
    }

    #[test]
    fn war_weariness_and_the_warmonger_bonus_use_the_government_functions() {
        let mut inp = city(8, 8); // everyone born content
        inp.weariness_class = 2; // high: base = size
        inp.war_counters = vec![130]; // above 120: twice the base, capped at the size
        let out = recompute(&inp);
        assert_eq!(out.happy_faces, -8);
        assert_eq!(out.reasons[reason::WAR_WEARINESS], 100);
        // A negative counter gives size / 4 happy faces instead.
        let mut inp = city(8, 8);
        inp.weariness_class = 2;
        inp.war_counters = vec![-3, -3];
        assert_eq!(recompute(&inp).happy_faces, 4);
    }

    #[test]
    fn at_war_foreigners_cost_faces_and_take_the_unhappiness_first() {
        let mut inp = city(4, 4);
        inp.citizens[3].foreign = true;
        inp.citizens[3].foreign_at_war = true;
        let out = recompute(&inp);
        // happy = -1: one content citizen -> unhappy; pass 1 (limit 1) picks the foreigner.
        assert_eq!(out.happy_faces, -1);
        assert_eq!(moods(&out.citizens), [CONTENT, CONTENT, CONTENT, UNHAPPY]);
        assert_eq!(out.reasons[reason::FOREIGN], 100);
    }

    // --- disorder, riots, celebration ----------------------------------------------

    #[test]
    fn disorder_needs_strictly_more_unhappy_than_happy() {
        assert!(!is_disorder(&natives(&[HAPPY, UNHAPPY])), "equal is fine");
        assert!(is_disorder(&natives(&[HAPPY, UNHAPPY, UNHAPPY])));
        assert!(!is_disorder(&natives(&[CONTENT, CONTENT])));
        assert!(is_disorder(&natives(&[UNHAPPY])), "0 < 1");
    }

    #[test]
    fn disorder_flag_transitions() {
        assert_eq!(disorder_step(false, false), Disorder::Quiet);
        assert_eq!(disorder_step(false, true), Disorder::Begins);
        assert_eq!(disorder_step(true, true), Disorder::Continues);
        assert_eq!(disorder_step(true, false), Disorder::Ends);
        assert_eq!(celebration_step(false, true), Celebration::Begins);
        assert_eq!(celebration_step(true, true), Celebration::Continues);
        assert_eq!(celebration_step(true, false), Celebration::Ends);
        assert_eq!(celebration_step(false, false), Celebration::Quiet);
    }

    #[test]
    fn riot_protects_wonders_palace_and_size_buildings() {
        assert!(riot_can_destroy(true, 0, 0));
        assert!(!riot_can_destroy(false, 0, 0));
        assert!(!riot_can_destroy(true, 4, 0), "wonder");
        assert!(!riot_can_destroy(true, 8, 0), "small wonder");
        assert!(!riot_can_destroy(true, 0, 1), "palace");
        assert!(!riot_can_destroy(true, 0, 1 << 11), "allows size level 2");
        assert!(!riot_can_destroy(true, 0, 1 << 12), "allows size level 3");
        assert!(riot_can_destroy(true, 0x10, 1 << 3), "other bits do not matter");
    }

    #[test]
    fn riot_never_hits_the_capital_and_needs_the_first_roll() {
        let rules = RiotRules::SHIPPED;
        for seed in 0..200 {
            let mut rng = Rng::new(seed);
            assert_eq!(riot(&mut rng, &rules, 20, true, 10, |_| true), None);
        }
        // chance 0: never.
        let none = RiotRules { chance: 0, ..rules };
        for seed in 0..200 {
            let mut rng = Rng::new(seed);
            assert_eq!(riot(&mut rng, &none, 20, false, 10, |_| true), None);
        }
    }

    #[test]
    fn riot_frequency_matches_the_chance_and_halves_for_small_cities() {
        let rules = RiotRules::SHIPPED;
        let hits = |size: i32| -> i32 {
            (0..4000)
                .filter(|&seed| {
                    let mut rng = Rng::new(seed);
                    riot(&mut rng, &rules, size, false, 10, |_| true).is_some()
                })
                .count() as i32
        };
        let big = hits(20); // 20 %
        let small = hits(4); // 20 % * 40 % = 8 %
        assert!((650..950).contains(&big), "about 800 of 4000, got {big}");
        assert!((230..410).contains(&small), "about 320 of 4000, got {small}");
    }

    #[test]
    fn riot_gives_up_when_nothing_can_be_destroyed() {
        let mut rules = RiotRules::SHIPPED;
        rules.chance = 100;
        let mut rng = Rng::new(7);
        assert_eq!(riot(&mut rng, &rules, 20, false, 2, |_| false), None);
        // 21 draws of a coin: the chance of never drawing row 1 is 2^-21.
        let mut rng = Rng::new(7);
        let victim = riot(&mut rng, &rules, 20, false, 2, |row| row == 1);
        assert_eq!(victim, Some(1), "the search finds the only candidate");
    }

    #[test]
    fn celebration_conditions() {
        let ok = natives(&[HAPPY, HAPPY, HAPPY, CONTENT, CONTENT, CONTENT, HAPPY]);
        assert!(celebrates(7, 6, &ok, 0));
        assert!(!celebrates(5, 6, &ok, 0), "too small");
        assert!(!celebrates(7, 6, &ok, -1), "starving");
        let mut with_unhappy = ok.clone();
        with_unhappy[0].mood = UNHAPPY;
        assert!(!celebrates(7, 6, &with_unhappy, 0));
        let equal = natives(&[HAPPY, HAPPY, HAPPY, CONTENT, CONTENT, CONTENT]);
        assert!(!celebrates(6, 6, &equal, 0), "happy must exceed content");
        let mut with_resister = ok.clone();
        with_resister[6].resisting = true;
        assert!(!celebrates(7, 6, &with_resister, 0));
    }
}
