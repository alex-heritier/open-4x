//! Governments: the GOVT record as the engine reads it, anarchy, the
//! war-weary Democracy and the AI's choice of government.
//!
//! Findings, addresses and the open list are in `../government.md`. This
//! module holds the decisions and the arithmetic; everything that touches
//! the world (the city list, the unit list, the message dialogs) is described
//! in the markdown only. Callers supply the facts the binary reads from the
//! world as plain numbers.
//!
//! The routines, in the order a player meets them:
//!
//! 1. [`score_government`] (`0x4446C0`) and [`choose_government`]
//!    (`0x4448F0`): the AI's pick, used when a revolution ends
//!    (`0x55CBB0`) and when it considers one (`0x444970`).
//! 2. [`revolution_denominator`] and [`considers_revolution`] (`0x444A10`):
//!    how often an AI player thinks about a revolution at all.
//! 3. [`anarchy_turns`] (`0x53A860`) and [`revolution_cooldown`]
//!    (`0x444970`): what a revolution costs and when the AI may try again.
//! 4. [`democracy_collapses`] (`0x560D18`): the forced revolution of a
//!    war-weary Democracy, for human and AI players alike.
//! 5. [`resettlement_loss`] (`0x55CD00`): what a government with forced
//!    resettlement does to a city.
//! 6. [`weariness_at_war`] and [`weariness_at_peace`] (`0x500AD0`): the
//!    war-weariness counters, and [`city_weariness_unhappy`] (`0x4BD780`):
//!    what a counter does to the mood of a city.
//! 7. [`declaration_weariness`] (`0x501F20`) and [`calls_to_arms`]
//!    (`0x500AD0`, second pass): what a declaration of war costs, and which
//!    allies a hostile unit on the territory calls in.
//! 8. [`ai_mobilizes`] (`0x444B80`) and [`mobilization_bonus`]
//!    (`0x4BFEE0`): mobilization.
//!
//! [`SHIPPED`] is the eight GOVT rows of `conquests.biq` reduced to the
//! fields above, with the Civilopedia values they were matched against.

use crate::economy::{self, Support};
use crate::rng::Rng;

/// The war-weariness class that collapses into anarchy (GOVT `+0x1E4` is 2,
/// Civilopedia "High"; 1 is "Low", 0 "None").
pub const WEARINESS_HIGH: i32 = 2;

/// Average war weariness (`0x5007B0`) at which the AI stops considering a
/// high-weariness government (`0x4446EE`: `>= 0x5A` returns `i32::MIN`).
pub const AI_REFUSES_AT: i32 = 90;

/// Average war weariness above which a high-weariness government is thrown
/// into anarchy by the turn processing (`0x560D28`: `<= 0x5A` is safe).
pub const COLLAPSES_ABOVE: i32 = 90;

/// Cooldown the AI adds on top of the anarchy countdown after starting a
/// revolution (`0x4449F6`: `0x10`).
pub const REVOLUTION_COOLDOWN: i32 = 16;

/// Hurry method of a government (GOVT `+0x1A0`, Civilopedia "Hurry
/// Method"). `0x433EE0`, `0x436A10` and `0x4B5290` compare it with 1 and 2.
pub mod hurry {
    /// Anarchy: production cannot be hurried.
    pub const NONE: i32 = 0;
    /// "Forced Labor": population is spent.
    pub const FORCED_LABOR: i32 = 1;
    /// "Pay citizens": gold is spent.
    pub const PAY: i32 = 2;
}

/// One GOVT record reduced to the fields a decoded routine reads. Offsets
/// are **memory** offsets of the record (stride 488, table `[0x9C71D8]`);
/// the matching `conquests.biq` row offsets are in `../government.md`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Govt {
    /// `+0x14`: the government's improvements cost upkeep (0 only for
    /// Anarchy). `0x55CFB0` returns 0 without it.
    pub requires_maintenance: bool,
    /// `+0x1C`: the Despotism tile penalty (a tile giving more than two of
    /// food, shields or commerce gives one less). Anarchy and Despotism.
    pub tile_penalty: bool,
    /// `+0x20`: the standard trade bonus (Republic, Democracy).
    pub trade_bonus: bool,
    /// `+0x28`: forced resettlement (Fascism): adopting the government and
    /// capturing a city shrink cities, see [`resettlement_loss`].
    pub forced_resettlement: bool,
    /// `+0x18C`: corruption and waste class, 0 Minimal, 1 Nuisance, 2
    /// Problematic, 3 Rampant, 4 Catastrophic, 5 Communal.
    pub corruption_class: i32,
    /// `+0x1A0`: hurry method, see [`hurry`].
    pub hurry: i32,
    /// `+0x1A8`: citizens one city may draft per turn.
    pub draft_rate: i32,
    /// `+0x1AC`: military police limit.
    pub military_police: i32,
    /// `+0x1B4`: how many ruler titles the government has.
    pub title_count: i32,
    /// `+0x1B8`: the prerequisite tech (`-1`: none).
    pub prerequisite_tech: i32,
    /// `+0x1BC`: cap on each of the three rates (10 is 100%).
    pub rate_cap: i32,
    /// `+0x1C0`: worker efficiency in steps of 50% (2 is 100%).
    pub worker_steps: i32,
    /// `+0x1D0`: free units; `-1` means units cost nothing.
    pub support_base: i32,
    /// `+0x1D4 / +0x1D8 / +0x1DC`: free units per town, city, metropolis.
    pub support_per_class: [i32; 3],
    /// `+0x1E0`: gold per unit past the free ones.
    pub support_per_unit: i32,
    /// `+0x1E4`: war weariness, 0 None, 1 Low, [`WEARINESS_HIGH`].
    pub war_weariness: i32,
}

impl Govt {
    /// The unit-support terms (`0x53A960`) for the given size classes of the
    /// player's cities.
    pub fn support(&self, classes: impl IntoIterator<Item = i32>) -> Support {
        economy::support_terms(
            self.support_base,
            self.support_per_class,
            self.support_per_unit,
            classes,
        )
    }
}

/// One shipped row. `flags` is `[requires_maintenance, tile_penalty,
/// trade_bonus, forced_resettlement]`; `traits` is `[hurry, draft_rate,
/// military_police, title_count, prerequisite_tech, rate_cap, worker_steps]`;
/// `support` is `(base, [town, city, metropolis], per_unit)`.
const fn govt(
    flags: [bool; 4],
    corruption_class: i32,
    traits: [i32; 7],
    support: (i32, [i32; 3], i32),
    war_weariness: i32,
) -> Govt {
    Govt {
        requires_maintenance: flags[0],
        tile_penalty: flags[1],
        trade_bonus: flags[2],
        forced_resettlement: flags[3],
        corruption_class,
        hurry: traits[0],
        draft_rate: traits[1],
        military_police: traits[2],
        title_count: traits[3],
        prerequisite_tech: traits[4],
        rate_cap: traits[5],
        worker_steps: traits[6],
        support_base: support.0,
        support_per_class: support.1,
        support_per_unit: support.2,
        war_weariness,
    }
}

/// Row indices of the shipped governments.
pub mod row {
    /// The transition government (GOVT `+0x10` set): `[0x9C3DD0]`.
    pub const ANARCHY: usize = 0;
    /// Despotism (GOVT `+0x0C` set: the default type).
    pub const DESPOTISM: usize = 1;
    /// Monarchy.
    pub const MONARCHY: usize = 2;
    /// Communism.
    pub const COMMUNISM: usize = 3;
    /// Republic.
    pub const REPUBLIC: usize = 4;
    /// Democracy.
    pub const DEMOCRACY: usize = 5;
    /// Fascism.
    pub const FASCISM: usize = 6;
    /// Feudalism.
    pub const FEUDALISM: usize = 7;
}

/// The eight GOVT rows of `conquests.biq`, in file order (see [`row`]).
///
/// Every number is a dword of the shipped rows read through the row reader
/// `0x5E3E80`; the hurry method, draft rate, military police limit, unit
/// support, war weariness, worker efficiency and corruption class equal the
/// Civilopedia page of the government (`GOVT_*`), which is how the
/// field-to-meaning map was fixed.
pub const SHIPPED: [Govt; 8] = [
    // flags [maintenance, tile penalty, trade bonus, resettlement], corruption
    // class, [hurry, draft, police, titles, prerequisite, cap, worker],
    // (base, [town, city, metropolis], per unit), weariness
    // Anarchy
    govt(
        [false, true, false, false],
        4,
        [0, 0, 0, 2, -1, 10, 1],
        (-1, [0, 0, 0], 1),
        0,
    ),
    // Despotism
    govt(
        [true, true, false, false],
        3,
        [1, 2, 2, 4, -1, 10, 2],
        (0, [4, 4, 4], 1),
        0,
    ),
    // Monarchy
    govt(
        [true, false, false, false],
        2,
        [2, 2, 3, 4, 19, 10, 2],
        (0, [2, 4, 8], 1),
        0,
    ),
    // Communism
    govt(
        [true, false, false, false],
        5,
        [1, 2, 4, 1, 46, 10, 2],
        (0, [6, 6, 6], 1),
        0,
    ),
    // Republic
    govt(
        [true, false, true, false],
        1,
        [2, 1, 0, 2, 18, 10, 2],
        (0, [1, 3, 4], 2),
        1,
    ),
    // Democracy
    govt(
        [true, false, true, false],
        0,
        [2, 1, 0, 3, 34, 10, 3],
        (0, [0, 0, 0], 1),
        2,
    ),
    // Fascism
    govt(
        [true, false, false, true],
        1,
        [1, 2, 4, 2, 82, 10, 4],
        (0, [4, 7, 10], 1),
        0,
    ),
    // Feudalism
    govt(
        [true, false, false, false],
        2,
        [1, 2, 3, 3, 22, 10, 2],
        (0, [5, 2, 1], 3),
        1,
    ),
];

/// Score added per city for a corruption class (`0x4446C0`, jump table
/// `0x4448D4`): 12, 8, 4, 2 for classes 0 to 3, nothing for the
/// catastrophic class 4, 4 for the communal class 5 and nothing beyond.
pub fn corruption_points(class: i32) -> i32 {
    match class {
        0 => 12,
        1 => 8,
        2 => 4,
        3 => 2,
        5 => 4,
        _ => 0,
    }
}

/// Score taken per city for one war (`0x4447E5..0x444832`), from the war
/// weariness `counter` against that enemy (`Player +0xCB4[civ - 1]`): 8
/// above 120, 4 above 60, 2 above 30, nothing at 30 or less; doubled for a
/// [`WEARINESS_HIGH`] government. A government without weariness (`0`) skips
/// the loop and pays nothing.
pub fn war_penalty(counter: i32, weariness: i32) -> i32 {
    if weariness == 0 {
        return 0;
    }
    let doubled = if weariness == WEARINESS_HIGH { 2 } else { 1 };
    let base = if counter > 120 {
        8
    } else if counter > 60 {
        4
    } else if counter > 30 {
        2
    } else {
        0
    };
    base * doubled
}

/// The facts `0x4446C0` reads about the player for one candidate
/// government.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScoreInputs<'a> {
    /// `Player +0x194`: the number of cities.
    pub cities: i32,
    /// `0x55D310(player, g)`: the unit-support gold under the candidate,
    /// see [`candidate_support_charge`].
    pub support_charge: i32,
    /// `0x55CFB0(player, g)`: the building upkeep under the candidate
    /// (`GOVT +0x14` set: the sum of the cities' `+0x2C`).
    pub upkeep: i32,
    /// `0x5007B0(player)`, see [`average_weariness`].
    pub average_weariness: i32,
    /// `Player +0xCB4[civ - 1]` for every civ in play that the player is at
    /// war with (`Player +0xD30[civ] != 0`).
    pub enemy_weariness: &'a [i32],
    /// `RACE +0x924 == g`: the candidate is the civ's favorite government.
    pub favorite: bool,
    /// `RACE +0x920 == g`: the candidate is the civ's shunned government.
    pub shunned: bool,
}

/// `value * num / 8`, truncating toward zero (the `cdq / and 7 / sar 3`
/// sequence at `0x44488D`).
fn eighths(value: i32, num: i32) -> i32 {
    value * num / 8
}

/// The AI's score for a government (`0x4446C0`, `ret 4`, `this` the
/// player).
///
/// 1. A [`WEARINESS_HIGH`] government is `i32::MIN` once the average war
///    weariness reaches [`AI_REFUSES_AT`].
/// 2. `8 * cities` without the tile penalty, another `8 * cities` with the
///    trade bonus, [`corruption_points`] `* cities`.
/// 3. `+ military_police * cities - support_charge - upkeep`.
/// 4. A government with weariness loses [`war_penalty`] `* cities` for
///    every war.
/// 5. The civ's favorite scales a positive score by 9/8 and a score that is
///    not positive by 7/8; its shunned government the other way round (7/8
///    and 9/8). The favorite is applied first.
pub fn score_government(g: &Govt, i: &ScoreInputs) -> i32 {
    if g.war_weariness == WEARINESS_HIGH && i.average_weariness >= AI_REFUSES_AT {
        return i32::MIN;
    }
    let cities = i.cities;
    let mut score = 0;
    if !g.tile_penalty {
        score += 8 * cities;
    }
    if g.trade_bonus {
        score += 8 * cities;
    }
    score += corruption_points(g.corruption_class) * cities;
    score += g.military_police * cities - i.support_charge - i.upkeep;
    for &counter in i.enemy_weariness {
        score -= war_penalty(counter, g.war_weariness) * cities;
    }
    if i.favorite {
        score = eighths(score, if score > 0 { 9 } else { 7 });
    }
    if i.shunned {
        score = eighths(score, if score > 0 { 7 } else { 9 });
    }
    score
}

/// The AI's choice (`0x4448F0`): the index with the highest score, the
/// lowest index winning a tie (`0x444942`: `jle` keeps the first). Pass
/// `None` for the transition government (`[0x9C3DD0]`) and for a government
/// whose prerequisite the player lacks (`0x561440`: `-1` is always met, the
/// tech count `[0x9C3DBC]` never is, anything else needs the player's bit in
/// `[0xA52B4C][tech]`). `i32::MIN` never wins, so the answer is `None` when
/// no candidate qualifies (the binary returns `-1`).
pub fn choose_government(scores: impl IntoIterator<Item = Option<i32>>) -> Option<usize> {
    let mut best = None;
    let mut best_score = i32::MIN;
    for (index, score) in scores.into_iter().enumerate() {
        if let Some(score) = score {
            if score > best_score {
                best_score = score;
                best = Some(index);
            }
        }
    }
    best
}

/// One civ as `0x5007B0` and `0x500F50` see it from a player's record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Relation {
    /// The civ is in play (`[0xA526C0]`).
    pub in_play: bool,
    /// `Player +0xEB4[civ - 1]` bit 0: the civs have met (HYPOTHESIS for
    /// the name; the bit gates the average).
    pub met: bool,
    /// `Player +0xEB4[civ - 1]` bit `0x20` (`0x500F9C`, unnamed).
    pub flag_20: bool,
    /// `Player +0xD30[civ] != 0`.
    pub at_war: bool,
    /// `Player +0xCB4[civ - 1]`: the war weariness against that civ.
    pub weariness: i32,
}

/// The average war weariness (`0x5007B0`): over the civs in play that the
/// player has met, the weariness of those at war and 0 for the others;
/// truncating division, 0 when no civ qualifies.
pub fn average_weariness(civs: impl IntoIterator<Item = Relation>) -> i32 {
    let mut count = 0;
    let mut sum = 0;
    for civ in civs {
        if civ.in_play && civ.met {
            count += 1;
            if civ.at_war {
                sum += civ.weariness;
            }
        }
    }
    if count > 0 {
        sum / count
    } else {
        0
    }
}

/// `0x500F50(player, flag)`: the civs in play (other than the player) at
/// war with the player, and with `include_flagged` also those not at war
/// whose `+0xEB4` word has bit `0x20`.
pub fn enemy_count(civs: impl IntoIterator<Item = Relation>, include_flagged: bool) -> i32 {
    let mut count = 0;
    for civ in civs {
        if !civ.in_play {
            continue;
        }
        if civ.at_war || (include_flagged && civ.flag_20) {
            count += 1;
        }
    }
    count
}

/// The unit-support gold the AI expects under a candidate government
/// (`0x55D310`): [`economy::unit_support_charge`] with the AI's extra free
/// units, `DIFF[level] +0x60 * cities + DIFF[level] +0x5C` (memory
/// offsets; shipped per city 0 0 0 1 2 3 4 8 and flat 0 0 0 4 8 12 16 24 from
/// Chieftain to Sid), added for a player outside the human mask.
pub fn candidate_support_charge(
    cities: i32,
    units: i32,
    exempt: i32,
    support: Support,
    ai_bonus: Option<(i32, i32)>,
) -> i32 {
    let extra = ai_bonus.map_or(0, |(flat, per_city)| per_city * cities + flat);
    economy::unit_support_charge(
        cities,
        units,
        exempt,
        support.free + extra,
        support.per_unit,
    )
}

/// The facts `0x444A10` reads before it rolls the die.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GateInputs<'a> {
    /// The turn `[0xA526AC]` is before `Player +0x3C`, the end of the
    /// player's golden age (written by `0x55C8C0` as turn + `[0x9C7308]`).
    pub golden_age: bool,
    /// `0x500F50(player, 1)` is 0.
    pub no_enemies: bool,
    /// `0x500F50(player, 0)`: the wars.
    pub wars: i32,
    /// The civ has [`economy::trait_bit::RELIGIOUS`].
    pub religious: bool,
    /// GOVT `+0x1E4` of the current government.
    pub weariness: i32,
    /// `Player +0xCB4[civ - 1]` for the civs in play at war.
    pub enemy_weariness: &'a [i32],
}

/// The denominator `r` of the AI's per-turn chance to consider a revolution
/// (`0x444A10`, `0x444A2E..0x444B59`): 64, or 128 during a golden age;
/// halved with no enemies; halved for a Religious civ; then, **with** war
/// weariness, divided for every war by the figure in [`war_penalty`]'s steps
/// (4, 2, 1 above 120, 60, 30, doubled when High), **without** it multiplied
/// by `wars + 1`; at least 1. The AI considers a revolution when
/// [`considers_revolution`] rolls 0.
pub fn revolution_denominator(i: &GateInputs) -> i32 {
    let mut r = if i.golden_age { 128 } else { 64 };
    if i.no_enemies {
        r /= 2;
    }
    if i.religious {
        r /= 2;
    }
    if i.weariness == 0 {
        r *= i.wars + 1;
    } else {
        let high = if i.weariness == WEARINESS_HIGH { 2 } else { 1 };
        for &counter in i.enemy_weariness {
            let step = if counter > 120 {
                4
            } else if counter > 60 {
                2
            } else if counter > 30 {
                1
            } else {
                continue;
            };
            r /= step * high;
        }
    }
    r.max(1)
}

/// One roll of the gameplay `Random` against [`revolution_denominator`]
/// (`0x444B5F`: `next(r)`, a 0 lets `0x444970` run).
pub fn considers_revolution(rng: &mut Rng, denominator: i32) -> bool {
    rng.below(denominator.max(1) as u32) == 0
}

/// The cooldown the AI sets after starting a revolution (`0x4449A9..
/// 0x4449FC`): none for a Religious civ, else the anarchy countdown
/// `Player +0x9C` plus [`REVOLUTION_COOLDOWN`]. `Player +0x34` holds it and
/// `0x444A10` counts it down.
pub fn revolution_cooldown(religious: bool, anarchy_countdown: i32) -> Option<i32> {
    if religious {
        None
    } else {
        Some(anarchy_countdown + REVOLUTION_COOLDOWN)
    }
}

/// How many turns a revolution lasts (`0x53A860(civ)`).
///
/// A Religious civ always gets 2 and draws nothing. Otherwise `2 +
/// next(3) + next(3) + min(3, 3 * cities / ocn)` (two draws of the
/// gameplay `Random`, then the city term with `ocn` from
/// [`economy::optimal_city_number`]). A player outside the human mask has the
/// result capped at `DIFF[level] +0x48` when that is non-zero (`ai_cap`;
/// shipped 0 0 0 4 3 2 2 1 from Chieftain to Sid, so the Sid AI never
/// suffers anarchy). A count of 1 or less ends the revolution at once
/// (`0x55CE91`).
pub fn anarchy_turns(
    religious: bool,
    rng: &mut Rng,
    cities: i32,
    ocn: i32,
    ai_cap: Option<i32>,
) -> i32 {
    if religious {
        return 2;
    }
    let mut turns = rng.below(3) + 2;
    turns += rng.below(3);
    turns += (3 * cities / ocn.max(1)).min(3);
    match ai_cap {
        Some(cap) if cap != 0 && cap < turns => cap,
        _ => turns,
    }
}

/// The turn-processing rule for a high-weariness government (`0x560D18..
/// 0x560D2F`, every player): above [`COLLAPSES_ABOVE`] the average war
/// weariness throws the player into anarchy (`0x55CE50`).
pub fn democracy_collapses(weariness: i32, average: i32) -> bool {
    weariness == WEARINESS_HIGH && average > COLLAPSES_ABOVE
}

/// Citizens a city loses to forced resettlement (`0x55CD00`, then
/// `0x4BA230(city, n, -1, 0)`): three above `city_max` and above 3, two above
/// `town_max` and above 2, one for any other city of at least 2, and none
/// for a city of size 1. The limits are the globals `[0x9C72E8]` and
/// `[0x9C72E4]` ([`economy::CITY_MAX`], [`economy::TOWN_MAX`]: 12 and 6); the
/// fixed 3 and 2 only matter for rules that lower them. Applied to every
/// city when the player adopts a government with
/// [`Govt::forced_resettlement`] (`0x55CDF1`) and to every city that player
/// captures (`0x56515B`).
pub fn resettlement_loss(size: i32, town_max: i32, city_max: i32) -> i32 {
    if size > city_max && size > 3 {
        3
    } else if size > town_max && size > 2 {
        2
    } else if size > 1 {
        1
    } else {
        0
    }
}

// ---------------------------------------------------------------------------
// War weariness and mobilization
// ---------------------------------------------------------------------------

/// Floor of the war-weariness counter's slow decay while at war
/// (`0x500CEC`: `cmp eax, 0x1E; jle`).
pub const WEARINESS_FLOOR: i32 = 30;

/// The per-turn change of one war-weariness counter (`Player +0xCB4[civ -
/// 1]`) for a civ **at war** with the player (`0x500B2D..0x500D09`, called
/// once per turn from `0x5604B0` at `0x560D41`).
///
/// `incidents` is the sum of the two "second accumulators" of the pair
/// records (`Player +0x1C4 + 0x4C * civ + 0x24` and the civ's record about
/// the player, written by `0x5631B0`), which is added in full first.
/// `own_unit_abroad` is set when a military unit of the player stands on a
/// tile owned by that civ (`0x500BED..0x500C33`), `foe_unit_at_home` when a
/// visible military unit of that civ stands on a tile owned by the player
/// (`0x500C38..0x500CB3`). An invasion adds 1; with neither side invading,
/// the counter falls by 1 while it is above [`WEARINESS_FLOOR`]; a foe on
/// the player's land alone changes nothing. A mobilized player adds 1 more
/// (`0x500CF4..0x500D07`).
pub fn weariness_at_war(
    counter: i32,
    incidents: i32,
    own_unit_abroad: bool,
    foe_unit_at_home: bool,
    mobilized: bool,
) -> i32 {
    let mut w = counter + incidents;
    if own_unit_abroad {
        w += 1;
    } else if !foe_unit_at_home && w > WEARINESS_FLOOR {
        w -= 1;
    }
    if mobilized {
        w += 1;
    }
    w
}

/// The per-turn change of a counter against a civ **not** at war with the
/// player (`0x500D0B..0x500D30`): unchanged while mobilized, else
/// `19 * w / 20` for a positive counter (truncating: 5 % off per turn).
pub fn weariness_at_peace(counter: i32, mobilized: bool) -> i32 {
    if mobilized || counter <= 0 {
        counter
    } else {
        counter * 19 / 20
    }
}

/// The unhappy citizens one war adds to a city (`0x4BD780`,
/// `0x4BD7F2..0x4BD851`): `base` is the city's size, halved unless the
/// government's weariness class is High; the counter against the enemy
/// scales it by 2 above 120, 1 above 60, 1/2 above 30 and 0 below.
pub fn city_weariness_term(counter: i32, size: i32, weariness_class: i32) -> i32 {
    if counter <= 0 {
        return 0;
    }
    let base = if weariness_class == WEARINESS_HIGH {
        size
    } else {
        size / 2
    };
    if counter > 120 {
        base * 2
    } else if counter > 60 {
        base
    } else if counter > 30 {
        base / 2
    } else {
        0
    }
}

/// The citizens `0x4BD780` shifts towards unhappiness in one city.
///
/// `counters` are the owner's counters against the civs in play it is at
/// war with. `police` counts the city's non-obsolete buildings with the
/// "reduces war weariness" flag (`BLDG +0xEC & 0x400000`, the Police Station);
/// each takes a quarter of the size off. `suffrage` is the owner's count of
/// active wonders with the "reduces war weariness everywhere" flag (`0x800`,
/// Universal Suffrage; `0x55A8D0(0x800, 0)`), one citizen each. The result
/// never exceeds the size, and a government without weariness gives 0.
pub fn city_weariness_unhappy(
    size: i32,
    weariness_class: i32,
    counters: impl IntoIterator<Item = i32>,
    police: i32,
    suffrage: i32,
) -> i32 {
    if weariness_class == 0 {
        return 0;
    }
    let mut total: i32 = counters
        .into_iter()
        .map(|n| city_weariness_term(n, size, weariness_class))
        .sum();
    let reduction = size / 4 * police;
    if reduction > 0 {
        total = (total - reduction).max(0);
    }
    if suffrage > 0 {
        total = (total - suffrage).max(0);
    }
    total.min(size)
}

/// Net incident balance above which the AI mobilizes (`0x444C96`: `cmp
/// ebx, 0x20; jle`).
pub const AI_MOBILIZES_ABOVE: i32 = 32;

/// One civ as the AI's mobilization check `0x444B80` sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Foe {
    /// The civ is in play (its bit in `[0xA526C0]`).
    pub in_play: bool,
    /// `Player +0xD30[civ] != 0`.
    pub at_war: bool,
    /// The civ has at least one city on the continent of the player's
    /// capital: `Player(civ) +0x1610[continent] > 0` (`0x444C62`).
    pub on_capital_continent: bool,
    /// What the civ did to the player: the player's pair record about it,
    /// first accumulator (`+0x1C4 + 0x4C * civ + 0x20`; `0x5631B0` adds to
    /// the **victim's** record).
    pub incidents_against_us: i32,
    /// What the player did to the civ: the civ's pair record about the
    /// player, first accumulator.
    pub incidents_by_us: i32,
}

/// The balance `0x444B80` sums over the civs in play that are at war with
/// the player and have a city on the capital's continent
/// (`0x444C40..0x444C86`): incidents against us minus incidents by us.
pub fn incident_balance(foes: impl IntoIterator<Item = Foe>) -> i32 {
    foes.into_iter()
        .filter(|f| f.in_play && f.at_war && f.on_capital_continent)
        .map(|f| f.incidents_against_us - f.incidents_by_us)
        .sum()
}

/// Whether the AI sets its mobilization flag this turn (`0x444B80`, called
/// from `0x444A10`): `eligible` and a balance above [`AI_MOBILIZES_ABOVE`].
///
/// `eligible` collects the early exits: the player's government is not the
/// transition government `[0x9C3DD0]` (anarchy), it is not already mobilized
/// (`Player +0xA4 != 1`), it knows a technology whose flags contain
/// `MOBILIZATION` (`0x561480(0x20)`, Nationalism) and its capital city
/// `Player +0x2C` exists. A successful check stores `Player +0xA4 = 1` and
/// recomputes every city of the player (`0x561290`, which runs `0x4B0E80`).
pub fn ai_mobilizes(eligible: bool, foes: impl IntoIterator<Item = Foe>) -> bool {
    eligible && incident_balance(foes) > AI_MOBILIZES_ABOVE
}

// ---------------------------------------------------------------------------
// Declarations of war and the calls to arms
// ---------------------------------------------------------------------------

/// Bits of the treaty word `Player +0xF30 + 4 * civ`, the player's treaties
/// with that civ (`0x502D40` sets and clears them).
pub mod treaty {
    /// Mutual protection pact (`0x502DE0`: `or edx, 1`; message
    /// `MUTUALPROTECTIONPACT`; cleared at `0x50363B`).
    pub const MUTUAL_PROTECTION: u32 = 0x01;
    /// Right of passage (`0x502ED9`; cleared at `0x503660`).
    pub const RIGHT_OF_PASSAGE: u32 = 0x02;
    /// The alliance that refuses attacks (`0x5B5790`, `NOALLIANCE_AGGRESSION`).
    /// No store that sets it was found among the direct accesses.
    pub const ALLIANCE: u32 = 0x04;
}

/// Whether a civ whose treaty word about the victim is `word` is dragged
/// into the victim's wars: an alliance or a mutual protection pact
/// (`0x500E46..0x500E51` in the turn update, `0x5B5600` after an attack).
pub fn joins_the_defence(word: u32) -> bool {
    word & (treaty::ALLIANCE | treaty::MUTUAL_PROTECTION) != 0
}

/// The reason code a call to arms passes to `0x501F20`: the victim's civ id
/// plus 2 (`0x500E5C`: `add edx, 2`; reason 0 is a plain declaration).
pub fn defence_reason(victim: u32) -> u32 {
    victim + 2
}

/// A unit on a tile as `0x56D480` sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileUnit {
    /// `unit +0x34`.
    pub owner: i32,
    /// Attack or defense strength (`0x5BE6E0`, `0x5BE820`), bombard strength
    /// (prototype `+0x48`) or the nuclear ability (ability 16).
    pub military: bool,
    /// Ability 17 (`0x5BC8B0(unit, 0x11)`).
    pub hidden_nationality: bool,
    /// `0x5BB650(unit, viewer, 1)`; **HYPOTHESIS:** the viewer can see it.
    pub visible: bool,
}

/// Whose military unit is on a tile (`0x56D480(x, y, viewer,
/// check_visibility)`): -1 when there is none, 0 when only barbarian or
/// disguised units qualify, otherwise the owner of the first unit in the
/// tile's chain that counts.
///
/// A unit with Hidden Nationality that belongs to someone other than the
/// viewer is reported as civ 0 when the viewer is a real civ, and the walk
/// goes on; a barbarian unit (owner 0) also lets the walk go on.
pub fn tile_occupant(units: &[TileUnit], viewer: i32, check_visibility: bool) -> i32 {
    let mut result = -1;
    for u in units {
        if !u.military {
            continue;
        }
        if check_visibility && viewer != -1 && !u.visible {
            continue;
        }
        if u.hidden_nationality && viewer != -1 && viewer != 0 && viewer != u.owner {
            result = 0;
            continue;
        }
        result = u.owner;
        if result != 0 {
            return result;
        }
    }
    result
}

/// One civ as the call-to-arms loop of `0x500AD0` (`0x500E23..0x500E8E`)
/// sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Partner {
    /// The civ index (0 is not a real civ, see [`calls_to_arms`]).
    pub civ: u32,
    /// Its bit is in the in-play mask `[0xA526C0]`.
    pub in_play: bool,
    /// The territory owner's treaty word about this civ
    /// (`Player +0xF34 + 4 * (civ - 1)`).
    pub treaty_with_owner: u32,
    /// `Player(civ) +0xD30[enemy] != 0`.
    pub at_war_with_enemy: bool,
}

/// The declarations the second pass of `0x500AD0` makes for **one** tile of
/// the territory owner's land: `(declarer, reason)` pairs in civ order.
///
/// `enemy` is [`tile_occupant`] of the tile as the owner sees it. Nothing
/// happens unless it is a real civ (above 0) the owner is at war with.
pub fn calls_to_arms(
    owner: u32,
    enemy: i32,
    owner_at_war_with_enemy: bool,
    others: impl IntoIterator<Item = Partner>,
) -> Vec<(u32, u32)> {
    if enemy <= 0 || !owner_at_war_with_enemy {
        return Vec::new();
    }
    others
        .into_iter()
        .filter(|p| {
            p.civ != owner
                && p.civ as i32 != enemy
                && p.in_play
                && joins_the_defence(p.treaty_with_owner)
                && !p.at_war_with_enemy
        })
        .map(|p| (p.civ, defence_reason(owner)))
        .collect()
}

/// The attitude class `0x440AD0` (Player vtable `+0x88`) derives from the
/// attitude score `s` of `0x440100` (vtable `+0x84`): 0 below `-m`, 1 below
/// zero, 2 at zero, 3 up to `m`, 4 above, with `m = ((width + height) / 2) /
/// 10` (`[0x9C74D4]`, `[0x9C74C0]`; truncating divisions; `sar edx, 2` at
/// `0x440B06` and `0x440B2B`).
pub fn attitude_class(score: i32, map_width: i32, map_height: i32) -> i32 {
    let margin = (map_width + map_height) / 2 / 10;
    if score < -margin {
        0
    } else if score > margin {
        4
    } else if score < 0 {
        1
    } else {
        2 + i32::from(score > 0)
    }
}

/// What a declaration of war does to the two weariness counters
/// (`0x502167..0x5021C0`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeclarationWeariness {
    /// Added to the declarer's counter against the victim.
    pub declarer: i32,
    /// Added to the victim's counter against the declarer.
    pub victim: i32,
}

/// The weariness a declaration of war costs. It applies only to a plain
/// declaration (`reason == 0`) by a player that has committed no hostile act
/// against the victim (`Player +0x1C0 + 0x4C * civ == 0`): then the
/// declarer gains 60 (attitude class 0) or 30 (class 1) and the victim
/// always loses 30.
pub fn declaration_weariness(
    reason: u32,
    hostile_acts: i32,
    attitude: i32,
) -> DeclarationWeariness {
    if reason != 0 || hostile_acts != 0 {
        return DeclarationWeariness {
            declarer: 0,
            victim: 0,
        };
    }
    DeclarationWeariness {
        declarer: match attitude {
            0 => 60,
            1 => 30,
            _ => 0,
        },
        victim: -30,
    }
}

/// The value an AI victim stores in `Player +0xBB0[declarer]` when war is
/// declared on it (`0x5021D8..0x50221F`): 8 times the half, rounded up, of
/// the declarations between the two in either direction (`a` and `b` are the
/// two `+0x1B0 + 0x4C * civ` counters, the new declaration included).
pub fn victim_war_memory(a: i32, b: i32) -> i32 {
    (a + b + 1) / 2 * 8
}

/// The unit-prototype fields `0x4BFEE0` reads (memory offsets of the PRTO
/// row, stride `0x138`; `conquests.biq` body offset = memory offset - 4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UnitFacts {
    /// `+0x50` (body `+0x4C`): cargo capacity.
    pub transport_capacity: i32,
    /// `+0x60` (body `+0x5C`).
    pub attack: i32,
    /// `+0x58` (body `+0x54`).
    pub defense: i32,
    /// `+0x48` (body `+0x44`).
    pub bombard_strength: i32,
    /// Ability bit 8, "Transports Only Aircraft" (`0x5E4EF0(8)`).
    pub carries_aircraft_only: bool,
    /// Ability bit 24, "Transports Only Tactical Missiles" (`0x5E4EF0(0x18)`).
    pub carries_missiles_only: bool,
    /// Ability bit 16, "Nuclear Weapon" (`0x5E4EF0(0x10)`).
    pub nuclear: bool,
}

/// Whether a unit counts as military for mobilization (`0x4BFEE0`,
/// `0x4BFEFE..0x4BFF6F`).
///
/// A prototype with cargo capacity qualifies only when it carries aircraft or
/// tactical missiles only (Carrier, Submarine): every other transport (Galley
/// to Transport, Helicopter, Army) does not, whatever its attack. The rest
/// qualifies with a positive attack, defense or bombard strength, or the
/// nuclear ability. Of the 141 shipped prototypes 16 fail it: Settler,
/// Worker, Scout, Explorer, Leader, Princess, Army, Helicopter and the
/// transport ships Galley, Caravel, Galleon, Transport, Carrack and Dromon
/// (the last two again as their AI copies).
pub fn is_military_unit(u: &UnitFacts) -> bool {
    if u.transport_capacity > 0 && !u.carries_aircraft_only && !u.carries_missiles_only {
        return false;
    }
    u.attack > 0 || u.defense > 0 || u.bombard_strength > 0 || u.nuclear
}

/// What a city is building, as `0x4BFEE0` sees it (`City +0x50` is the kind,
/// 2 for a unit; `+0x4C` the prototype).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Build {
    /// An improvement, a wonder or nothing.
    Other,
    /// A unit prototype.
    Unit(UnitFacts),
}

/// `City::mobilizationBonus` (`0x4BFEE0`): the extra shield on every tile
/// that already makes one ([`crate::yields::Working::military_build`]) applies when the
/// owner is mobilized (`Player +0xA4 == 1`) and the city is building a
/// [`is_military_unit`]. Workers, settlers and explorers build normally
/// without the bonus (Civilopedia "Mobilization").
pub fn mobilization_bonus(owner_mobilized: bool, building: Build) -> bool {
    match building {
        Build::Unit(u) => owner_mobilized && is_military_unit(&u),
        Build::Other => false,
    }
}

#[cfg(test)]
mod tests {
    use super::row::*;
    use super::*;

    fn quiet(g: &Govt, cities: i32) -> i32 {
        score_government(
            g,
            &ScoreInputs {
                cities,
                support_charge: 0,
                upkeep: 0,
                average_weariness: 0,
                enemy_weariness: &[],
                favorite: false,
                shunned: false,
            },
        )
    }

    #[test]
    fn the_table_equals_the_civilopedia() {
        // (hurry, draft, military police, worker %, weariness) per page.
        let pages = [
            (hurry::NONE, 0, 0, 50, 0),
            (hurry::FORCED_LABOR, 2, 2, 100, 0),
            (hurry::PAY, 2, 3, 100, 0),
            (hurry::FORCED_LABOR, 2, 4, 100, 0),
            (hurry::PAY, 1, 0, 100, 1),
            (hurry::PAY, 1, 0, 150, 2),
            (hurry::FORCED_LABOR, 2, 4, 200, 0),
            (hurry::FORCED_LABOR, 2, 3, 100, 1),
        ];
        for (g, p) in SHIPPED.iter().zip(pages) {
            assert_eq!(
                (
                    g.hurry,
                    g.draft_rate,
                    g.military_police,
                    g.worker_steps * 50,
                    g.war_weariness
                ),
                p
            );
        }
        // Unit support per town / city / metropolis, and gold per unit.
        assert_eq!(SHIPPED[DESPOTISM].support_per_class, [4, 4, 4]);
        assert_eq!(SHIPPED[MONARCHY].support_per_class, [2, 4, 8]);
        assert_eq!(SHIPPED[REPUBLIC].support_per_class, [1, 3, 4]);
        assert_eq!(SHIPPED[FASCISM].support_per_class, [4, 7, 10]);
        assert_eq!(SHIPPED[FEUDALISM].support_per_class, [5, 2, 1]);
        assert_eq!(SHIPPED[REPUBLIC].support_per_unit, 2);
        assert_eq!(SHIPPED[FEUDALISM].support_per_unit, 3);
        // Corruption: Democracy minimal, Despotism rampant, Anarchy
        // catastrophic, Communism communal.
        let classes: Vec<i32> = SHIPPED.iter().map(|g| g.corruption_class).collect();
        assert_eq!(classes, [4, 3, 2, 5, 1, 0, 1, 2]);
    }

    #[test]
    fn only_anarchy_is_free_of_upkeep_and_fascism_resettles() {
        let upkeep: Vec<bool> = SHIPPED.iter().map(|g| g.requires_maintenance).collect();
        assert_eq!(upkeep, [false, true, true, true, true, true, true, true]);
        let resettles: Vec<usize> = (0..8).filter(|&i| SHIPPED[i].forced_resettlement).collect();
        assert_eq!(resettles, [FASCISM]);
        // Anarchy charges nothing for units (base -1) and has no free ones.
        let s = SHIPPED[ANARCHY].support([0, 1, 2]);
        assert_eq!((s.per_unit, s.free), (0, 0));
    }

    #[test]
    fn a_quiet_empire_of_ten_cities_ranks_the_governments() {
        // 8c without the tile penalty, 8c trade bonus, the corruption table,
        // military police * c.
        let s: Vec<i32> = SHIPPED.iter().map(|g| quiet(g, 10)).collect();
        //          Anarchy Despot Monarch Commun  Republ  Democ  Fascism Feudal
        assert_eq!(s, [0, 40, 150, 160, 240, 280, 200, 150]);
    }

    #[test]
    fn the_chooser_takes_the_best_and_the_lowest_index_on_a_tie() {
        let scores = |techs: &[usize]| -> Vec<Option<i32>> {
            (0..8)
                .map(|i| {
                    if i == ANARCHY
                        || (SHIPPED[i].prerequisite_tech != -1
                            && !techs.contains(&(SHIPPED[i].prerequisite_tech as usize)))
                    {
                        None
                    } else {
                        Some(quiet(&SHIPPED[i], 10))
                    }
                })
                .collect()
        };
        // Everything known: Democracy.
        let all: Vec<usize> = (0..90).collect();
        assert_eq!(choose_government(scores(&all)), Some(DEMOCRACY));
        // Without the Democracy tech (34): Republic.
        let no_democracy: Vec<usize> = (0..90).filter(|&t| t != 34).collect();
        assert_eq!(choose_government(scores(&no_democracy)), Some(REPUBLIC));
        // Only Monarchy (19) and Feudalism (22): both score 150, Monarchy
        // comes first.
        assert_eq!(choose_government(scores(&[19, 22])), Some(MONARCHY));
        // Nothing known: Despotism, the only government without a tech.
        assert_eq!(choose_government(scores(&[])), Some(DESPOTISM));
        // No candidate at all.
        assert_eq!(choose_government([None, None]), None);
        // i32::MIN is never chosen.
        assert_eq!(choose_government([Some(i32::MIN)]), None);
    }

    #[test]
    fn war_wearies_a_republic_and_a_democracy_out_of_the_running() {
        let at = |g: &Govt, average: i32, wars: &[i32]| {
            score_government(
                g,
                &ScoreInputs {
                    cities: 10,
                    support_charge: 0,
                    upkeep: 0,
                    average_weariness: average,
                    enemy_weariness: wars,
                    favorite: false,
                    shunned: false,
                },
            )
        };
        // Low weariness (Republic): -2 / -4 / -8 per city per war.
        assert_eq!(at(&SHIPPED[REPUBLIC], 0, &[]), 240);
        assert_eq!(at(&SHIPPED[REPUBLIC], 0, &[31]), 240 - 20);
        assert_eq!(at(&SHIPPED[REPUBLIC], 0, &[61]), 240 - 40);
        assert_eq!(at(&SHIPPED[REPUBLIC], 0, &[121, 30]), 240 - 80);
        // High (Democracy): double.
        assert_eq!(at(&SHIPPED[DEMOCRACY], 0, &[121]), 280 - 160);
        assert_eq!(at(&SHIPPED[DEMOCRACY], 0, &[61, 31]), 280 - 80 - 40);
        // A government without weariness ignores wars.
        assert_eq!(at(&SHIPPED[COMMUNISM], 0, &[200, 200]), 160);
        // The shut-out: High at an average of 90, but not at 89.
        assert_eq!(at(&SHIPPED[DEMOCRACY], 90, &[]), i32::MIN);
        assert_eq!(at(&SHIPPED[DEMOCRACY], 89, &[]), 280);
        assert_eq!(at(&SHIPPED[REPUBLIC], 200, &[]), 240);
    }

    #[test]
    fn favorite_and_shunned_governments_move_the_score_by_an_eighth() {
        let with = |score_cities: i32, favorite: bool, shunned: bool, support: i32| {
            score_government(
                &SHIPPED[MONARCHY],
                &ScoreInputs {
                    cities: score_cities,
                    support_charge: support,
                    upkeep: 0,
                    average_weariness: 0,
                    enemy_weariness: &[],
                    favorite,
                    shunned,
                },
            )
        };
        // Monarchy with 10 cities scores 150.
        assert_eq!(with(10, false, false, 0), 150);
        assert_eq!(with(10, true, false, 0), 168); // 150 * 9 / 8
        assert_eq!(with(10, false, true, 0), 131); // 150 * 7 / 8
                                                   // A negative score is pulled toward 0 by the favorite and pushed
                                                   // away by the shunned government (support 230 gives -80).
        assert_eq!(with(10, false, false, 230), -80);
        assert_eq!(with(10, true, false, 230), -70);
        assert_eq!(with(10, false, true, 230), -90);
        // Truncation toward zero: 150 - 138 = 12 -> 12 * 9 / 8 = 13.
        assert_eq!(with(10, true, false, 138), 13);
        assert_eq!(with(10, false, true, 227), -77 * 9 / 8);
    }

    #[test]
    fn unit_support_enters_the_score_through_the_governments_terms() {
        // 10 cities (all towns here are not needed: the classes come from the
        // caller), 30 units, none exempt.
        let classes = [1; 10]; // ten cities of the city class
        let charge = |g: &Govt, ai: Option<(i32, i32)>| {
            candidate_support_charge(10, 30, 0, g.support(classes), ai)
        };
        // Democracy: nothing free, 1 gold a unit.
        assert_eq!(charge(&SHIPPED[DEMOCRACY], None), 30);
        // Republic: 3 free per city = 30 free, so nothing.
        assert_eq!(charge(&SHIPPED[REPUBLIC], None), 0);
        // Feudalism: 2 free per city = 20, 10 units at 3 gold.
        assert_eq!(charge(&SHIPPED[FEUDALISM], None), 30);
        // Anarchy never charges.
        assert_eq!(charge(&SHIPPED[ANARCHY], None), 0);
        // The AI gets DIFF +0x5C + 10 * DIFF +0x60 more free units (Emperor:
        // 8 flat, 2 per city = 28): Democracy owes 2.
        assert_eq!(charge(&SHIPPED[DEMOCRACY], Some((8, 2))), 2);
        // No cities, no charge.
        assert_eq!(
            candidate_support_charge(0, 5, 0, SHIPPED[DEMOCRACY].support(Vec::<i32>::new()), None),
            0
        );
        // The charge is subtracted from the score.
        let scored = score_government(
            &SHIPPED[DEMOCRACY],
            &ScoreInputs {
                cities: 10,
                support_charge: 30,
                upkeep: 12,
                average_weariness: 0,
                enemy_weariness: &[],
                favorite: false,
                shunned: false,
            },
        );
        assert_eq!(scored, 280 - 30 - 12);
    }

    fn rel(in_play: bool, met: bool, at_war: bool, weariness: i32) -> Relation {
        Relation {
            in_play,
            met,
            flag_20: false,
            at_war,
            weariness,
        }
    }

    #[test]
    fn the_average_counts_met_civs_and_zeroes_the_peaceful_ones() {
        // Two met civs at peace and one at war with 120: (0 + 0 + 120) / 3.
        let civs = [
            rel(true, true, false, 50),
            rel(true, true, false, 0),
            rel(true, true, true, 120),
            rel(true, false, true, 999), // not met: ignored
            rel(false, true, true, 999), // not in play: ignored
        ];
        assert_eq!(average_weariness(civs), 40);
        assert_eq!(average_weariness([]), 0);
        assert_eq!(average_weariness([rel(true, false, true, 80)]), 0);
        // Truncation: 100 / 3.
        assert_eq!(
            average_weariness([
                rel(true, true, true, 100),
                rel(true, true, false, 0),
                rel(true, true, false, 0)
            ]),
            33
        );
    }

    #[test]
    fn enemies_are_the_wars_plus_optionally_the_flagged() {
        let mut flagged = rel(true, true, false, 0);
        flagged.flag_20 = true;
        let civs = [rel(true, true, true, 0), flagged, rel(false, true, true, 0)];
        assert_eq!(enemy_count(civs, false), 1);
        assert_eq!(enemy_count(civs, true), 2);
        assert_eq!(enemy_count([], true), 0);
    }

    #[test]
    fn the_revolution_gate_follows_the_dice_ladder() {
        let base = GateInputs {
            golden_age: false,
            no_enemies: false,
            wars: 0,
            religious: false,
            weariness: 0,
            enemy_weariness: &[],
        };
        assert_eq!(revolution_denominator(&base), 64);
        // Golden age doubles, no enemies halves, Religious halves.
        let g = GateInputs {
            golden_age: true,
            ..base
        };
        assert_eq!(revolution_denominator(&g), 128);
        let peace = GateInputs {
            no_enemies: true,
            ..base
        };
        assert_eq!(revolution_denominator(&peace), 32);
        let pious = GateInputs {
            no_enemies: true,
            religious: true,
            ..base
        };
        assert_eq!(revolution_denominator(&pious), 16);
        // No weariness: every war makes a change less likely.
        let at_war = GateInputs { wars: 3, ..base };
        assert_eq!(revolution_denominator(&at_war), 64 * 4);
        // Weariness: wars make it likelier. Low: 130 -> /4, 70 -> /2.
        let low = GateInputs {
            weariness: 1,
            wars: 2,
            enemy_weariness: &[130, 70, 20],
            ..base
        };
        assert_eq!(revolution_denominator(&low), 64 / 4 / 2);
        // High: 130 -> /8, 70 -> /4: 64 / 8 / 4 = 2.
        let high = GateInputs {
            weariness: 2,
            enemy_weariness: &[130, 70],
            ..low
        };
        assert_eq!(revolution_denominator(&high), 2);
        // Never below 1.
        let drowning = GateInputs {
            enemy_weariness: &[200, 200, 200],
            ..high
        };
        assert_eq!(revolution_denominator(&drowning), 1);
    }

    #[test]
    fn a_denominator_of_one_always_considers_a_revolution() {
        let mut rng = Rng::new(7);
        assert!((0..20).all(|_| considers_revolution(&mut rng, 1)));
        // 64 does so about once in 64.
        let mut rng = Rng::new(7);
        let hits = (0..6400)
            .filter(|_| considers_revolution(&mut rng, 64))
            .count();
        assert!((60..140).contains(&hits), "{hits}");
    }

    #[test]
    fn the_cooldown_skips_religious_civs() {
        assert_eq!(revolution_cooldown(false, 5), Some(21));
        assert_eq!(revolution_cooldown(false, 0), Some(16));
        assert_eq!(revolution_cooldown(true, 5), None);
    }

    #[test]
    fn anarchy_length_follows_the_two_dice_and_the_city_term() {
        // Religious: 2, and the dice stay untouched.
        let mut rng = Rng::new(3);
        let before = rng;
        assert_eq!(anarchy_turns(true, &mut rng, 99, 10, None), 2);
        assert_eq!(rng, before);
        // Otherwise exactly two draws.
        let mut rng = Rng::new(3);
        let mut reference = Rng::new(3);
        let a = reference.below(3);
        let b = reference.below(3);
        let turns = anarchy_turns(false, &mut rng, 10, 20, None);
        assert_eq!(turns, 2 + a + b + 3 * 10 / 20);
        assert_eq!(rng, reference);
        // The city term saturates at 3: range 2..=9 over many seeds.
        for seed in 0..200 {
            let t = anarchy_turns(false, &mut Rng::new(seed), 100, 10, None);
            assert!((5..=9).contains(&t), "{t}");
            let t = anarchy_turns(false, &mut Rng::new(seed), 0, 10, None);
            assert!((2..=6).contains(&t), "{t}");
        }
        // The AI cap applies only when non-zero and smaller.
        let long = anarchy_turns(false, &mut Rng::new(1), 100, 10, None);
        assert_eq!(
            anarchy_turns(false, &mut Rng::new(1), 100, 10, Some(0)),
            long
        );
        assert_eq!(anarchy_turns(false, &mut Rng::new(1), 100, 10, Some(2)), 2);
        assert_eq!(
            anarchy_turns(false, &mut Rng::new(1), 100, 10, Some(20)),
            long
        );
    }

    #[test]
    fn only_a_high_weariness_government_collapses_and_only_above_ninety() {
        assert!(democracy_collapses(WEARINESS_HIGH, 91));
        assert!(!democracy_collapses(WEARINESS_HIGH, 90));
        assert!(!democracy_collapses(1, 500));
        assert!(!democracy_collapses(0, 500));
        // The AI backs away one point earlier than the collapse.
        assert_eq!(AI_REFUSES_AT, COLLAPSES_ABOVE);
    }

    #[test]
    fn resettlement_shrinks_towns_by_one_cities_by_two_metropolises_by_three() {
        let loss = |size| resettlement_loss(size, economy::TOWN_MAX, economy::CITY_MAX);
        let losses: Vec<i32> = [1, 2, 6, 7, 12, 13, 30].map(loss).to_vec();
        assert_eq!(losses, [0, 1, 1, 2, 2, 3, 3]);
        // The fixed 3 and 2 matter for rules with tiny limits: a size-3 city
        // is above both limits (1 and 2) but not above 3, so it loses 2; a
        // size-4 city loses 3.
        assert_eq!(resettlement_loss(3, 1, 2), 2);
        assert_eq!(resettlement_loss(4, 1, 2), 3);
        assert_eq!(resettlement_loss(2, 1, 1), 1);
    }

    #[test]
    fn weariness_grows_while_invading_and_decays_to_thirty_otherwise() {
        // Own military unit on enemy land: +1, whatever else is going on.
        assert_eq!(weariness_at_war(10, 0, true, false, false), 11);
        assert_eq!(weariness_at_war(10, 0, true, true, false), 11);
        // Nobody invades: -1 per turn, but only above the floor of 30.
        assert_eq!(weariness_at_war(50, 0, false, false, false), 49);
        assert_eq!(weariness_at_war(31, 0, false, false, false), 30);
        assert_eq!(weariness_at_war(30, 0, false, false, false), 30);
        assert_eq!(weariness_at_war(5, 0, false, false, false), 5);
        // A foe on our land alone freezes it.
        assert_eq!(weariness_at_war(50, 0, false, true, false), 50);
        // The incident accumulators are added in full before anything else,
        // and the decay then looks at the new value.
        assert_eq!(weariness_at_war(10, 7, true, false, false), 18);
        assert_eq!(weariness_at_war(25, 5, false, false, false), 30);
        assert_eq!(weariness_at_war(25, 6, false, false, false), 30);
        assert_eq!(weariness_at_war(25, 7, false, false, false), 31);
        // Mobilization adds one more every turn on top of all of it.
        assert_eq!(weariness_at_war(10, 0, true, false, true), 12);
        assert_eq!(weariness_at_war(50, 0, false, true, true), 51);
        assert_eq!(weariness_at_war(50, 0, false, false, true), 50);
    }

    #[test]
    fn weariness_fades_five_percent_a_turn_in_peace_unless_mobilized() {
        assert_eq!(weariness_at_peace(100, false), 95);
        assert_eq!(weariness_at_peace(95, false), 90);
        // Truncating: 19 * 3 / 20 = 2, 19 * 1 / 20 = 0.
        assert_eq!(weariness_at_peace(3, false), 2);
        assert_eq!(weariness_at_peace(1, false), 0);
        assert_eq!(weariness_at_peace(0, false), 0);
        assert_eq!(weariness_at_peace(-4, false), -4);
        assert_eq!(weariness_at_peace(100, true), 100);
        // From 130, above every penalty step of `war_penalty`, it takes one
        // peaceful turn to drop below 125, and 25 to reach 30.
        let mut w = 130;
        let mut turns = 0;
        let mut path = Vec::new();
        while w > WEARINESS_FLOOR {
            w = weariness_at_peace(w, false);
            turns += 1;
            if turns <= 3 {
                path.push(w);
            }
        }
        assert_eq!(path, [123, 116, 110]);
        assert_eq!(turns, 25);
    }

    fn foe(at_war: bool, near: bool, against_us: i32, by_us: i32) -> Foe {
        Foe {
            in_play: true,
            at_war,
            on_capital_continent: near,
            incidents_against_us: against_us,
            incidents_by_us: by_us,
        }
    }

    #[test]
    fn the_ai_mobilizes_above_thirty_two_net_incidents_only() {
        // Only at-war civs with a city on the capital's continent count.
        let foes = [
            foe(true, true, 30, 0),
            foe(true, false, 100, 0),
            foe(false, true, 100, 0),
            Foe {
                in_play: false,
                ..foe(true, true, 100, 0)
            },
        ];
        assert_eq!(incident_balance(foes), 30);
        assert!(!ai_mobilizes(true, foes));
        // Exactly 32 is not enough, 33 is.
        assert!(!ai_mobilizes(true, [foe(true, true, 32, 0)]));
        assert!(ai_mobilizes(true, [foe(true, true, 33, 0)]));
        // What we did to them is subtracted, per foe, and may go negative.
        assert_eq!(
            incident_balance([foe(true, true, 40, 10), foe(true, true, 0, 5)]),
            25
        );
        assert!(!ai_mobilizes(true, [foe(true, true, 40, 10)]));
        // The early exits win over any balance.
        assert!(!ai_mobilizes(false, [foe(true, true, 1000, 0)]));
    }

    fn unit(attack: i32, defense: i32) -> UnitFacts {
        UnitFacts {
            attack,
            defense,
            ..UnitFacts::default()
        }
    }

    #[test]
    fn only_armed_non_transport_units_are_military() {
        // Settler, Worker, Scout, Explorer: attack 0, defense 0.
        assert!(!is_military_unit(&unit(0, 0)));
        // Warrior and Archer.
        assert!(is_military_unit(&unit(1, 1)));
        // A defense-only unit (Spearman-class) is military too.
        assert!(is_military_unit(&unit(0, 2)));
        // Catapult-class bombard alone is enough.
        let artillery = UnitFacts {
            bombard_strength: 4,
            ..unit(0, 0)
        };
        assert!(is_military_unit(&artillery));
        // A nuclear weapon is military whatever its numbers.
        let nuke = UnitFacts {
            nuclear: true,
            ..unit(0, 0)
        };
        assert!(is_military_unit(&nuke));
        // Galleon (1/2, capacity 4), Army (0/0, capacity 3), Helicopter
        // (0/2, capacity 3): transports are not.
        let galleon = UnitFacts {
            transport_capacity: 4,
            ..unit(1, 2)
        };
        assert!(!is_military_unit(&galleon));
        let helicopter = UnitFacts {
            transport_capacity: 3,
            ..unit(0, 2)
        };
        assert!(!is_military_unit(&helicopter));
        // Carriers and submarines carry only aircraft or missiles and are.
        let carrier = UnitFacts {
            transport_capacity: 4,
            carries_aircraft_only: true,
            ..unit(0, 8)
        };
        assert!(is_military_unit(&carrier));
        let submarine = UnitFacts {
            transport_capacity: 2,
            carries_missiles_only: true,
            ..unit(8, 2)
        };
        assert!(is_military_unit(&submarine));
        // A carrier-class transport with no weapons at all still fails the
        // second half.
        let empty = UnitFacts {
            transport_capacity: 4,
            carries_aircraft_only: true,
            ..unit(0, 0)
        };
        assert!(!is_military_unit(&empty));
    }

    #[test]
    fn the_shield_bonus_needs_mobilization_and_a_military_unit() {
        let rifleman = Build::Unit(unit(5, 4));
        let worker = Build::Unit(unit(0, 0));
        assert!(mobilization_bonus(true, rifleman));
        assert!(!mobilization_bonus(false, rifleman));
        assert!(!mobilization_bonus(true, worker));
        assert!(!mobilization_bonus(true, Build::Other));
    }
    fn tile_unit(owner: i32) -> TileUnit {
        TileUnit {
            owner,
            military: true,
            hidden_nationality: false,
            visible: true,
        }
    }

    #[test]
    fn the_first_visible_military_unit_names_the_occupant() {
        assert_eq!(tile_occupant(&[], 3, true), -1);
        // A settler (not military) does not count.
        let settler = TileUnit {
            military: false,
            ..tile_unit(4)
        };
        assert_eq!(tile_occupant(&[settler], 3, true), -1);
        assert_eq!(tile_occupant(&[settler, tile_unit(5)], 3, true), 5);
        // An unseen unit is skipped when visibility is checked, kept when not.
        let unseen = TileUnit {
            visible: false,
            ..tile_unit(4)
        };
        assert_eq!(tile_occupant(&[unseen, tile_unit(5)], 3, true), 5);
        assert_eq!(tile_occupant(&[unseen, tile_unit(5)], 3, false), 4);
        assert_eq!(tile_occupant(&[unseen], -1, true), 4);
    }

    #[test]
    fn disguised_and_barbarian_units_report_civ_zero_and_the_walk_goes_on() {
        let privateer = TileUnit {
            hidden_nationality: true,
            ..tile_unit(4)
        };
        // Seen by a third civ it is nobody's; the next unit decides.
        assert_eq!(tile_occupant(&[privateer], 3, true), 0);
        assert_eq!(tile_occupant(&[privateer, tile_unit(6)], 3, true), 6);
        // Its owner and a viewer of -1 or 0 see through it.
        assert_eq!(tile_occupant(&[privateer], 4, true), 4);
        assert_eq!(tile_occupant(&[privateer], -1, true), 4);
        assert_eq!(tile_occupant(&[privateer], 0, true), 4);
        // A barbarian (owner 0) does not stop the walk either.
        assert_eq!(tile_occupant(&[tile_unit(0), tile_unit(2)], 3, true), 2);
        assert_eq!(tile_occupant(&[tile_unit(0)], 3, true), 0);
    }

    fn partner(civ: u32, word: u32) -> Partner {
        Partner {
            civ,
            in_play: true,
            treaty_with_owner: word,
            at_war_with_enemy: false,
        }
    }

    #[test]
    fn allies_and_protection_partners_are_called_but_passage_alone_is_not() {
        assert!(joins_the_defence(treaty::ALLIANCE));
        assert!(joins_the_defence(treaty::MUTUAL_PROTECTION));
        assert!(joins_the_defence(
            treaty::ALLIANCE | treaty::RIGHT_OF_PASSAGE
        ));
        assert!(!joins_the_defence(treaty::RIGHT_OF_PASSAGE));
        assert!(!joins_the_defence(0));

        let others = [
            partner(2, treaty::ALLIANCE),
            partner(3, treaty::RIGHT_OF_PASSAGE),
            partner(4, treaty::MUTUAL_PROTECTION),
            partner(5, 0),
        ];
        // Civ 1's territory, civ 6 inside it, at war with civ 1.
        assert_eq!(calls_to_arms(1, 6, true, others), vec![(2, 3), (4, 3)]);
        assert_eq!(defence_reason(1), 3);
    }

    #[test]
    fn nobody_is_called_unless_the_intruder_is_a_real_enemy() {
        let others = [partner(2, treaty::ALLIANCE)];
        assert!(calls_to_arms(1, -1, true, others).is_empty());
        assert!(calls_to_arms(1, 0, true, others).is_empty());
        assert!(calls_to_arms(1, 6, false, others).is_empty());
    }

    #[test]
    fn the_caller_the_intruder_the_absent_and_the_already_warring_are_left_out() {
        let at_war = Partner {
            at_war_with_enemy: true,
            ..partner(2, treaty::ALLIANCE)
        };
        let absent = Partner {
            in_play: false,
            ..partner(3, treaty::ALLIANCE)
        };
        let others = [
            partner(1, treaty::ALLIANCE), // the territory owner itself
            partner(6, treaty::ALLIANCE), // the intruder (an ally of both)
            at_war,
            absent,
            partner(7, treaty::MUTUAL_PROTECTION),
        ];
        assert_eq!(calls_to_arms(1, 6, true, others), vec![(7, 3)]);
    }

    #[test]
    fn the_attitude_classes_split_at_a_tenth_of_the_mean_map_size() {
        // A 100 x 60 map: n = 80, m = 8.
        let class = |s| attitude_class(s, 100, 60);
        assert_eq!(class(-9), 0);
        assert_eq!(class(-8), 1);
        assert_eq!(class(-1), 1);
        assert_eq!(class(0), 2);
        assert_eq!(class(1), 3);
        assert_eq!(class(8), 3);
        assert_eq!(class(9), 4);
        // Odd sums and small maps truncate: (5 + 4) / 2 = 4, m = 0.
        assert_eq!(attitude_class(0, 5, 4), 2);
        assert_eq!(attitude_class(1, 5, 4), 4);
        assert_eq!(attitude_class(-1, 5, 4), 0);
    }

    #[test]
    fn a_plain_unprovoked_declaration_on_a_friend_costs_weariness() {
        let w = |reason, acts, class| declaration_weariness(reason, acts, class);
        let none = DeclarationWeariness {
            declarer: 0,
            victim: 0,
        };
        assert_eq!(
            w(0, 0, 0),
            DeclarationWeariness {
                declarer: 60,
                victim: -30
            }
        );
        assert_eq!(
            w(0, 0, 1),
            DeclarationWeariness {
                declarer: 30,
                victim: -30
            }
        );
        // The victim's relief does not depend on the class.
        for class in 2..=4 {
            assert_eq!(
                w(0, 0, class),
                DeclarationWeariness {
                    declarer: 0,
                    victim: -30
                }
            );
        }
        // A reason (a call to arms) or an earlier hostile act removes both.
        assert_eq!(w(3, 0, 0), none);
        assert_eq!(w(0, 1, 0), none);
    }

    #[test]
    fn an_ai_victim_remembers_eight_per_pair_of_declarations() {
        assert_eq!(victim_war_memory(0, 1), 8);
        assert_eq!(victim_war_memory(1, 1), 8);
        assert_eq!(victim_war_memory(1, 2), 16);
        assert_eq!(victim_war_memory(3, 3), 24);
        assert_eq!(victim_war_memory(4, 3), 32);
    }
    #[test]
    fn a_war_makes_a_democratic_city_unhappy_in_steps_of_the_counter() {
        let term = |n| city_weariness_term(n, 8, WEARINESS_HIGH);
        assert_eq!(term(0), 0);
        assert_eq!(term(30), 0);
        assert_eq!(term(31), 4);
        assert_eq!(term(60), 4);
        assert_eq!(term(61), 8);
        assert_eq!(term(120), 8);
        assert_eq!(term(121), 16);
        assert_eq!(term(-5), 0);
        // A Low government halves the base first.
        let low = |n| city_weariness_term(n, 8, 1);
        assert_eq!(low(31), 2);
        assert_eq!(low(61), 4);
        assert_eq!(low(121), 8);
        // An odd size truncates twice.
        assert_eq!(city_weariness_term(40, 7, 1), 1);
        assert_eq!(city_weariness_term(40, 7, WEARINESS_HIGH), 3);
    }

    #[test]
    fn the_unhappy_never_exceed_the_city_and_none_means_none() {
        // Two wars of 100 in a size 8 Democracy: 16, capped at 8.
        assert_eq!(
            city_weariness_unhappy(8, WEARINESS_HIGH, [100, 100], 0, 0),
            8
        );
        assert_eq!(city_weariness_unhappy(8, 0, [200], 0, 0), 0);
        assert_eq!(city_weariness_unhappy(8, WEARINESS_HIGH, [], 0, 0), 0);
        // Wars at peace-time counters cost nothing.
        assert_eq!(city_weariness_unhappy(8, WEARINESS_HIGH, [10, 30], 0, 0), 0);
    }

    #[test]
    fn police_and_suffrage_take_citizens_off_but_not_below_zero() {
        // Size 12, one war at 100 under a Democracy: 12 unhappy.
        assert_eq!(city_weariness_unhappy(12, WEARINESS_HIGH, [100], 0, 0), 12);
        // The Police Station takes size / 4 = 3, Universal Suffrage one.
        assert_eq!(city_weariness_unhappy(12, WEARINESS_HIGH, [100], 1, 0), 9);
        assert_eq!(city_weariness_unhappy(12, WEARINESS_HIGH, [100], 1, 1), 8);
        assert_eq!(city_weariness_unhappy(12, WEARINESS_HIGH, [100], 0, 1), 11);
        // A small counter is wiped out entirely.
        assert_eq!(city_weariness_unhappy(12, WEARINESS_HIGH, [40], 1, 1), 2);
        assert_eq!(city_weariness_unhappy(4, 1, [40], 1, 3), 0);
        // Sizes below 4 get no reduction from a Police Station.
        assert_eq!(city_weariness_unhappy(3, WEARINESS_HIGH, [100], 1, 0), 3);
    }
}
