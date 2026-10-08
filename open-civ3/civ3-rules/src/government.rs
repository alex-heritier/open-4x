//! Governments: the GOVT record as the engine reads it, anarchy, the
//! war-weary Democracy and the AI's choice of government.
//!
//! This module holds the decisions and the arithmetic; everything that touches
//! the world (the city list, the unit list, the message dialogs) is supplied by
//! the caller as plain numbers. [`SHIPPED`] is the eight GOVT rows of
//! `conquests.biq` reduced to the fields above, matched against the Civilopedia.

use crate::economy::{self, Support};
use civ3_worldgen::rng::Rng;

/// The war-weariness class that collapses into anarchy (GOVT `+0x1E4` is 2,
/// Civilopedia "High"; 1 is "Low", 0 "None").
pub const WEARINESS_HIGH: i32 = 2;

/// Average war weariness at which the AI stops considering a high-weariness
/// government.
pub const AI_REFUSES_AT: i32 = 90;

/// Average war weariness above which a high-weariness government is thrown into
/// anarchy by the turn processing.
pub const COLLAPSES_ABOVE: i32 = 90;

/// Cooldown the AI adds on top of the anarchy countdown after starting a
/// revolution.
pub const REVOLUTION_COOLDOWN: i32 = 16;

/// Hurry method of a government (GOVT `+0x1A0`, Civilopedia "Hurry Method").
pub mod hurry {
    /// Anarchy: production cannot be hurried.
    pub const NONE: i32 = 0;
    /// "Forced Labor": population is spent.
    pub const FORCED_LABOR: i32 = 1;
    /// "Pay citizens": gold is spent.
    pub const PAY: i32 = 2;
}

/// One GOVT record reduced to the fields a decoded routine reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Govt {
    /// `+0x14`: the government's improvements cost upkeep (0 only for Anarchy).
    pub requires_maintenance: bool,
    /// `+0x1C`: the Despotism tile penalty (a tile giving more than two of
    /// food, shields or commerce gives one less).
    pub tile_penalty: bool,
    /// `+0x20`: the standard trade bonus (Republic, Democracy).
    pub trade_bonus: bool,
    /// `+0x28`: forced resettlement (Fascism).
    pub forced_resettlement: bool,
    /// `+0x18C`: corruption and waste class, 0 Minimal to 5 Communal.
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
    /// The unit-support terms for the given size classes of the player's cities.
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
    /// The transition government.
    pub const ANARCHY: usize = 0;
    /// Despotism (the default type).
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
pub const SHIPPED: [Govt; 8] = [
    // Anarchy
    govt([false, true, false, false], 4, [0, 0, 0, 2, -1, 10, 1], (-1, [0, 0, 0], 1), 0),
    // Despotism
    govt([true, true, false, false], 3, [1, 2, 2, 4, -1, 10, 2], (0, [4, 4, 4], 1), 0),
    // Monarchy
    govt([true, false, false, false], 2, [2, 2, 3, 4, 19, 10, 2], (0, [2, 4, 8], 1), 0),
    // Communism
    govt([true, false, false, false], 5, [1, 2, 4, 1, 46, 10, 2], (0, [6, 6, 6], 1), 0),
    // Republic
    govt([true, false, true, false], 1, [2, 1, 0, 2, 18, 10, 2], (0, [1, 3, 4], 2), 1),
    // Democracy
    govt([true, false, true, false], 0, [2, 1, 0, 3, 34, 10, 3], (0, [0, 0, 0], 1), 2),
    // Fascism
    govt([true, false, false, true], 1, [1, 2, 4, 2, 82, 10, 4], (0, [4, 7, 10], 1), 0),
    // Feudalism
    govt([true, false, false, false], 2, [1, 2, 3, 3, 22, 10, 2], (0, [5, 2, 1], 3), 1),
];

/// Score added per city for a corruption class: 12, 8, 4, 2 for classes 0 to 3,
/// nothing for class 4, 4 for class 5 and nothing beyond.
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

/// Score taken per city for one war, from the war weariness `counter` against
/// that enemy: 8 above 120, 4 above 60, 2 above 30, nothing at 30 or less;
/// doubled for a [`WEARINESS_HIGH`] government. A government without weariness
/// pays nothing.
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

/// The facts the score routine reads about the player for one candidate
/// government.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScoreInputs<'a> {
    /// The number of cities.
    pub cities: i32,
    /// The unit-support gold under the candidate.
    pub support_charge: i32,
    /// The building upkeep under the candidate.
    pub upkeep: i32,
    /// [`average_weariness`].
    pub average_weariness: i32,
    /// The war weariness against every civ in play the player is at war with.
    pub enemy_weariness: &'a [i32],
    /// The candidate is the civ's favorite government.
    pub favorite: bool,
    /// The candidate is the civ's shunned government.
    pub shunned: bool,
}

/// `value * num / 8`, truncating toward zero.
fn eighths(value: i32, num: i32) -> i32 {
    value * num / 8
}

/// The AI's score for a government.
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

/// The AI's choice: the index with the highest score, the lowest index winning
/// a tie. `i32::MIN` never wins, so the answer is `None` when no candidate
/// qualifies.
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

/// One civ as the weariness average and the enemy count see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Relation {
    /// The civ is in play.
    pub in_play: bool,
    /// The civs have met.
    pub met: bool,
    /// The unnamed `+0xEB4` bit `0x20`.
    pub flag_20: bool,
    /// The civs are at war.
    pub at_war: bool,
    /// The war weariness against that civ.
    pub weariness: i32,
}

/// The average war weariness: over the civs in play that the player has met,
/// the weariness of those at war and 0 for the others; truncating division, 0
/// when no civ qualifies.
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

/// The civs in play (other than the player) at war with the player, and with
/// `include_flagged` also those not at war whose `+0xEB4` word has bit `0x20`.
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

/// The unit-support gold the AI expects under a candidate government:
/// [`economy::unit_support_charge`] with the AI's extra free units
/// (`(flat, per_city)`) added for a player outside the human mask.
pub fn candidate_support_charge(
    cities: i32,
    units: i32,
    exempt: i32,
    support: Support,
    ai_bonus: Option<(i32, i32)>,
) -> i32 {
    let extra = ai_bonus.map_or(0, |(flat, per_city)| per_city * cities + flat);
    economy::unit_support_charge(cities, units, exempt, support.free + extra, support.per_unit)
}

/// The facts the revolution gate reads before it rolls the die.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GateInputs<'a> {
    /// The turn is before the end of the player's golden age.
    pub golden_age: bool,
    /// The player has no enemies.
    pub no_enemies: bool,
    /// The wars.
    pub wars: i32,
    /// The civ has [`economy::trait_bit::RELIGIOUS`].
    pub religious: bool,
    /// GOVT `+0x1E4` of the current government.
    pub weariness: i32,
    /// The war weariness against the civs in play at war.
    pub enemy_weariness: &'a [i32],
}

/// The denominator `r` of the AI's per-turn chance to consider a revolution:
/// 64, or 128 during a golden age; halved with no enemies; halved for a
/// Religious civ; then, with war weariness, divided for every war by the figure
/// in the weariness steps (4, 2, 1 above 120, 60, 30, doubled when High);
/// without it multiplied by `wars + 1`; at least 1.
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

/// One roll of the gameplay `Random` against [`revolution_denominator`]: a 0
/// lets the revolution proceed.
pub fn considers_revolution(rng: &mut Rng, denominator: i32) -> bool {
    rng.below(denominator.max(1) as u32) == 0
}

/// The cooldown the AI sets after starting a revolution: none for a Religious
/// civ, else the anarchy countdown plus [`REVOLUTION_COOLDOWN`].
pub fn revolution_cooldown(religious: bool, anarchy_countdown: i32) -> Option<i32> {
    if religious {
        None
    } else {
        Some(anarchy_countdown + REVOLUTION_COOLDOWN)
    }
}

/// How many turns a revolution lasts.
///
/// A Religious civ always gets 2 and draws nothing. Otherwise `2 + next(3) +
/// next(3) + min(3, 3 * cities / ocn)`. A player outside the human mask has the
/// result capped at `ai_cap` when that is non-zero. A count of 1 or less ends
/// the revolution at once.
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

/// The turn-processing rule for a high-weariness government: above
/// [`COLLAPSES_ABOVE`] the average war weariness throws the player into anarchy.
pub fn democracy_collapses(weariness: i32, average: i32) -> bool {
    weariness == WEARINESS_HIGH && average > COLLAPSES_ABOVE
}

/// Citizens a city loses to forced resettlement: three above `city_max` and
/// above 3, two above `town_max` and above 2, one for any other city of at
/// least 2, and none for a city of size 1.
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

/// Floor of the war-weariness counter's slow decay while at war.
pub const WEARINESS_FLOOR: i32 = 30;

/// The per-turn change of one war-weariness counter for a civ at war with the
/// player.
///
/// `incidents` is added in full first. An invasion adds 1; with neither side
/// invading, the counter falls by 1 while it is above [`WEARINESS_FLOOR`]; a
/// foe on the player's land alone changes nothing. A mobilized player adds 1.
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

/// The per-turn change of a counter against a civ not at war with the player:
/// unchanged while mobilized, else `19 * w / 20` for a positive counter
/// (truncating: 5% off per turn).
pub fn weariness_at_peace(counter: i32, mobilized: bool) -> i32 {
    if mobilized || counter <= 0 {
        counter
    } else {
        counter * 19 / 20
    }
}

/// The unhappy citizens one war adds to a city: `base` is the city's size,
/// halved unless the government's weariness class is High; the counter against
/// the enemy scales it by 2 above 120, 1 above 60, 1/2 above 30 and 0 below.
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

/// The citizens shifted towards unhappiness in one city.
///
/// `counters` are the owner's counters against the civs in play it is at war
/// with. `police` counts the city's Police Stations, each taking a quarter of
/// the size off; `suffrage` is the owner's count of active Universal Suffrage
/// wonders, one citizen each. The result never exceeds the size, and a
/// government without weariness gives 0.
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

/// Net incident balance above which the AI mobilizes.
pub const AI_MOBILIZES_ABOVE: i32 = 32;

/// One civ as the AI's mobilization check sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Foe {
    /// The civ is in play.
    pub in_play: bool,
    /// The civs are at war.
    pub at_war: bool,
    /// The civ has at least one city on the continent of the player's capital.
    pub on_capital_continent: bool,
    /// What the civ did to the player.
    pub incidents_against_us: i32,
    /// What the player did to the civ.
    pub incidents_by_us: i32,
}

/// The balance the mobilization check sums over the civs in play that are at
/// war with the player and have a city on the capital's continent.
pub fn incident_balance(foes: impl IntoIterator<Item = Foe>) -> i32 {
    foes.into_iter()
        .filter(|f| f.in_play && f.at_war && f.on_capital_continent)
        .map(|f| f.incidents_against_us - f.incidents_by_us)
        .sum()
}

/// Whether the AI sets its mobilization flag this turn: `eligible` and a
/// balance above [`AI_MOBILIZES_ABOVE`].
pub fn ai_mobilizes(eligible: bool, foes: impl IntoIterator<Item = Foe>) -> bool {
    eligible && incident_balance(foes) > AI_MOBILIZES_ABOVE
}

/// Bits of the treaty word, the player's treaties with a civ.
pub mod treaty {
    /// Mutual protection pact.
    pub const MUTUAL_PROTECTION: u32 = 0x01;
    /// Right of passage.
    pub const RIGHT_OF_PASSAGE: u32 = 0x02;
    /// The alliance that refuses attacks.
    pub const ALLIANCE: u32 = 0x04;
}

/// Whether a civ whose treaty word about the victim is `word` is dragged into
/// the victim's wars: an alliance or a mutual protection pact.
pub fn joins_the_defence(word: u32) -> bool {
    word & (treaty::ALLIANCE | treaty::MUTUAL_PROTECTION) != 0
}

/// The reason code a call to arms passes: the victim's civ id plus 2.
pub fn defence_reason(victim: u32) -> u32 {
    victim + 2
}

/// A unit on a tile as the occupant scan sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileUnit {
    /// The unit's owner.
    pub owner: i32,
    /// Attack or defense strength, bombard strength or the nuclear ability.
    pub military: bool,
    /// Hidden Nationality.
    pub hidden_nationality: bool,
    /// The viewer can see it.
    pub visible: bool,
}

/// Whose military unit is on a tile: -1 when there is none, 0 when only
/// barbarian or disguised units qualify, otherwise the owner of the first unit
/// in the tile's chain that counts.
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

/// One civ as the call-to-arms loop sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Partner {
    /// The civ index (0 is not a real civ).
    pub civ: u32,
    /// Its bit is in the in-play mask.
    pub in_play: bool,
    /// The territory owner's treaty word about this civ.
    pub treaty_with_owner: u32,
    /// The civ is at war with the enemy.
    pub at_war_with_enemy: bool,
}

/// The declarations the call-to-arms pass makes for one tile of the territory
/// owner's land: `(declarer, reason)` pairs in civ order. `enemy` is
/// [`tile_occupant`] of the tile as the owner sees it. Nothing happens unless it
/// is a real civ the owner is at war with.
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

/// The attitude class derived from an attitude score: 0 below `-m`, 1 below
/// zero, 2 at zero, 3 up to `m`, 4 above, with `m = ((width + height) / 2) / 10`.
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

/// What a declaration of war does to the two weariness counters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeclarationWeariness {
    /// Added to the declarer's counter against the victim.
    pub declarer: i32,
    /// Added to the victim's counter against the declarer.
    pub victim: i32,
}

/// The weariness a declaration of war costs. It applies only to a plain
/// declaration (`reason == 0`) by a player that has committed no hostile act
/// against the victim: then the declarer gains 60 (attitude class 0) or 30
/// (class 1) and the victim always loses 30.
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

/// The value an AI victim stores when war is declared on it: 8 times the half,
/// rounded up, of the declarations between the two in either direction.
pub fn victim_war_memory(a: i32, b: i32) -> i32 {
    (a + b + 1) / 2 * 8
}

/// The unit-prototype fields the mobilization check reads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UnitFacts {
    /// Cargo capacity.
    pub transport_capacity: i32,
    /// Attack strength.
    pub attack: i32,
    /// Defense strength.
    pub defense: i32,
    /// Bombard strength.
    pub bombard_strength: i32,
    /// "Transports Only Aircraft".
    pub carries_aircraft_only: bool,
    /// "Transports Only Tactical Missiles".
    pub carries_missiles_only: bool,
    /// "Nuclear Weapon".
    pub nuclear: bool,
}

/// Whether a unit counts as military for mobilization.
///
/// A prototype with cargo capacity qualifies only when it carries aircraft or
/// tactical missiles only; every other transport does not, whatever its attack.
/// The rest qualifies with a positive attack, defense or bombard strength, or
/// the nuclear ability.
pub fn is_military_unit(u: &UnitFacts) -> bool {
    if u.transport_capacity > 0 && !u.carries_aircraft_only && !u.carries_missiles_only {
        return false;
    }
    u.attack > 0 || u.defense > 0 || u.bombard_strength > 0 || u.nuclear
}

/// What a city is building, as the mobilization check sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Build {
    /// An improvement, a wonder or nothing.
    Other,
    /// A unit prototype.
    Unit(UnitFacts),
}

/// The extra shield on every tile that already makes one applies when the owner
/// is mobilized and the city is building a military unit. Workers, settlers and
/// explorers build normally without the bonus.
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
                (g.hurry, g.draft_rate, g.military_police, g.worker_steps * 50, g.war_weariness),
                p
            );
        }
        assert_eq!(SHIPPED[DESPOTISM].support_per_class, [4, 4, 4]);
        assert_eq!(SHIPPED[MONARCHY].support_per_class, [2, 4, 8]);
        assert_eq!(SHIPPED[REPUBLIC].support_per_class, [1, 3, 4]);
        assert_eq!(SHIPPED[FASCISM].support_per_class, [4, 7, 10]);
        assert_eq!(SHIPPED[FEUDALISM].support_per_class, [5, 2, 1]);
        assert_eq!(SHIPPED[REPUBLIC].support_per_unit, 2);
        assert_eq!(SHIPPED[FEUDALISM].support_per_unit, 3);
        let classes: Vec<i32> = SHIPPED.iter().map(|g| g.corruption_class).collect();
        assert_eq!(classes, [4, 3, 2, 5, 1, 0, 1, 2]);
    }

    #[test]
    fn a_quiet_empire_of_ten_cities_ranks_the_governments() {
        let s: Vec<i32> = SHIPPED.iter().map(|g| quiet(g, 10)).collect();
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
        let all: Vec<usize> = (0..90).collect();
        assert_eq!(choose_government(scores(&all)), Some(DEMOCRACY));
        let no_democracy: Vec<usize> = (0..90).filter(|&t| t != 34).collect();
        assert_eq!(choose_government(scores(&no_democracy)), Some(REPUBLIC));
        assert_eq!(choose_government(scores(&[19, 22])), Some(MONARCHY));
        assert_eq!(choose_government(scores(&[])), Some(DESPOTISM));
        assert_eq!(choose_government([None, None]), None);
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
        assert_eq!(at(&SHIPPED[REPUBLIC], 0, &[]), 240);
        assert_eq!(at(&SHIPPED[REPUBLIC], 0, &[31]), 240 - 20);
        assert_eq!(at(&SHIPPED[REPUBLIC], 0, &[61]), 240 - 40);
        assert_eq!(at(&SHIPPED[REPUBLIC], 0, &[121, 30]), 240 - 80);
        assert_eq!(at(&SHIPPED[DEMOCRACY], 0, &[121]), 280 - 160);
        assert_eq!(at(&SHIPPED[DEMOCRACY], 0, &[61, 31]), 280 - 80 - 40);
        assert_eq!(at(&SHIPPED[COMMUNISM], 0, &[200, 200]), 160);
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
        assert_eq!(with(10, false, false, 0), 150);
        assert_eq!(with(10, true, false, 0), 168);
        assert_eq!(with(10, false, true, 0), 131);
        assert_eq!(with(10, false, false, 230), -80);
        assert_eq!(with(10, true, false, 230), -70);
        assert_eq!(with(10, false, true, 230), -90);
        assert_eq!(with(10, true, false, 138), 13);
        assert_eq!(with(10, false, true, 227), -77 * 9 / 8);
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
        let civs = [
            rel(true, true, false, 50),
            rel(true, true, false, 0),
            rel(true, true, true, 120),
            rel(true, false, true, 999),
        ];
        assert_eq!(average_weariness(civs), (0 + 0 + 120) / 3);
        assert_eq!(average_weariness([rel(false, true, true, 999)]), 0);
    }

    #[test]
    fn unit_support_enters_the_score_through_the_governments_terms() {
        let classes = [1; 10];
        let charge = |g: &Govt, ai: Option<(i32, i32)>| {
            candidate_support_charge(10, 30, 0, g.support(classes), ai)
        };
        assert_eq!(charge(&SHIPPED[DEMOCRACY], None), 30);
        assert_eq!(charge(&SHIPPED[REPUBLIC], None), 0);
        assert_eq!(charge(&SHIPPED[FEUDALISM], None), 30);
        assert_eq!(charge(&SHIPPED[ANARCHY], None), 0);
        assert_eq!(charge(&SHIPPED[DEMOCRACY], Some((8, 2))), 2);
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

    #[test]
    fn weariness_and_mobilization_arithmetic() {
        assert_eq!(weariness_at_war(30, 0, false, false, false), 30);
        assert_eq!(weariness_at_war(31, 0, false, false, false), 30);
        assert_eq!(weariness_at_war(0, 5, false, false, false), 5);
        assert_eq!(weariness_at_war(10, 0, true, false, false), 11);
        assert_eq!(weariness_at_war(10, 0, false, true, false), 10);
        assert_eq!(weariness_at_war(10, 0, false, false, true), 11);
        assert_eq!(weariness_at_peace(100, false), 95);
        assert_eq!(weariness_at_peace(100, true), 100);
        assert_eq!(city_weariness_term(121, 10, WEARINESS_HIGH), 20);
        assert_eq!(city_weariness_term(61, 10, 1), 5);
        // The result never exceeds the city size.
        assert_eq!(city_weariness_unhappy(10, WEARINESS_HIGH, [121], 1, 1), 10);
        // size 40, class Low: term = (40/2) = 20, Police Station -10, Suffrage -1.
        assert_eq!(city_weariness_unhappy(40, 1, [61], 1, 1), 9);
    }

    #[test]
    fn mobilization_needs_military_builds_and_a_big_incident_balance() {
        let settler = UnitFacts { transport_capacity: 0, ..UnitFacts::default() };
        let warrior = UnitFacts { attack: 1, ..UnitFacts::default() };
        let carrier = UnitFacts {
            transport_capacity: 4,
            carries_aircraft_only: true,
            defense: 1,
            ..UnitFacts::default()
        };
        let galley = UnitFacts { transport_capacity: 3, attack: 1, ..UnitFacts::default() };
        assert!(!is_military_unit(&settler));
        assert!(is_military_unit(&warrior));
        assert!(is_military_unit(&carrier));
        assert!(!is_military_unit(&galley));
        assert!(mobilization_bonus(true, Build::Unit(warrior)));
        assert!(!mobilization_bonus(false, Build::Unit(warrior)));
        assert!(!mobilization_bonus(true, Build::Other));
        let foe = |against, by| Foe {
            in_play: true,
            at_war: true,
            on_capital_continent: true,
            incidents_against_us: against,
            incidents_by_us: by,
        };
        assert!(!ai_mobilizes(true, [foe(30, 0)]));
        assert!(ai_mobilizes(true, [foe(40, 0)]));
        assert!(!ai_mobilizes(false, [foe(40, 0)]));
    }
}
