//! Combat resolution: defender choice, round odds, damage, retreat, ranged
//! attacks (target choice, the 1 HP floor, city walls), interception, strikes
//! on cities and tiles, and the post-victory rolls (promotion, golden age,
//! great leader, enslavement).
//!
//! The dice are the shared gameplay [`Rng`]. The model is one *duel* at fixed
//! odds: each round draws `next(1024)`; a roll at or above the odds means the
//! attacker wins the round, else the defender does; the loser takes one point
//! of damage; a fighter dies when `max_hp - damage <= 0`.

use civ3_worldgen::rng::Rng;

/// The round die.
pub const ROUND_DIE: u32 = 0x400;
/// Lower clamp of the odds.
pub const ODDS_MIN: i32 = 1;
/// Upper clamp of the odds, `0x3FF`.
pub const ODDS_MAX: i32 = 0x3FF;
/// Radar-tower bonus.
pub const RADAR_PCT: i32 = 25;
/// Amphibious-assault bonus.
pub const AMPHIBIOUS_PCT: i32 = 25;
/// Bonus for owning the barbarian-bonus wonder (the Great Wall).
pub const GREAT_WALL_VS_BARBARIANS_PCT: i32 = 100;
/// Added to the opponent's retreat percentage to form the retreat die.
pub const RETREAT_MARGIN: i32 = 50;
/// Base strength of a unit-less, city-less tile in the strike roll.
pub const TILE_DEFENSE_STRENGTH: i32 = 16;

/// RULE-section values the combat code reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rules {
    /// Fortified-unit bonus.
    pub fortify_pct: i32,
    /// River-crossing bonus.
    pub river_pct: i32,
    /// Fortress overlay bonus. A barricade is twice this.
    pub fort_pct: i32,
    /// Town / City / Metropolis defense bonus.
    pub size_bonus_pct: [i32; 3],
    /// Largest town size.
    pub town_max: i32,
    /// Largest city size.
    pub city_max: i32,
    /// Internal movement units per full move.
    pub move_unit: i32,
    /// Base strength of a city in the ranged city strike, indexed by mode
    /// (`[mode 0, mode 1]`).
    pub city_strike_base: [i32; 2],
}

impl Rules {
    /// The values decoded from `conquests.biq`.
    pub const CONQUESTS: Rules = Rules {
        fortify_pct: 25,
        river_pct: 25,
        fort_pct: 50,
        size_bonus_pct: [0, 50, 100],
        town_max: 6,
        city_max: 12,
        move_unit: 3,
        city_strike_base: [16, 16],
    };

    /// Size class of a city: 0 town, 1 city, 2 metropolis.
    pub fn size_class(&self, size: i32) -> usize {
        if size > self.city_max {
            2
        } else if size > self.town_max {
            1
        } else {
            0
        }
    }
}

impl Default for Rules {
    fn default() -> Self {
        Rules::CONQUESTS
    }
}

/// The non-barbarian side of a fight against barbarians gets this percentage
/// per difficulty level (Chieftain to Sid).
pub const DIFF_VS_BARBARIAN_PCT: [i32; 8] = [800, 400, 200, 100, 50, 25, 0, 0];

/// One `EXPR` (experience level) row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Experience {
    /// Base hit points.
    pub base_hp: i32,
    /// Retreat percentage.
    pub retreat_pct: i32,
}

/// Conscript, Regular, Veteran, Elite.
pub const EXPERIENCE: [Experience; 4] = [
    Experience { base_hp: 2, retreat_pct: 34 },
    Experience { base_hp: 3, retreat_pct: 50 },
    Experience { base_hp: 4, retreat_pct: 58 },
    Experience { base_hp: 5, retreat_pct: 66 },
];

/// `TERR` defense percentage per terrain: Desert, Plains, Grassland, Tundra,
/// Flood Plain, Hills, Mountains, Forest, Jungle, Marsh, Volcano, Coast, Sea,
/// Ocean.
pub const TERRAIN_DEFENSE_PCT: [i32; 14] = [10, 10, 10, 10, 10, 50, 100, 25, 25, 20, 80, 10, 10, 10];

/// Direction index of a coordinate delta. `N=0 NE=1 E=2 SE=3 S=4 SW=5 W=6
/// NW=7`, y growing downwards; north is folded to 0.
pub fn dir_from_delta(dx: i32, dy: i32) -> u32 {
    let (ax, ay) = (dx.abs(), dy.abs());
    let east = dx >= 0;
    let south = dy >= 0;
    let diagonal = |east: bool, south: bool| match (east, south) {
        (true, false) => 1,
        (true, true) => 3,
        (false, true) => 5,
        (false, false) => 7,
    };
    if ax > ay {
        if 2 * ax > 3 * ay {
            return if east { 2 } else { 6 };
        }
        return diagonal(east, south);
    }
    if 2 * ay > 3 * ax {
        return if south { 4 } else { 0 };
    }
    diagonal(east, south)
}

/// Whether the defender's tile has a river edge facing the attacker.
pub fn river_edge(def_mask: u8, dir_def_to_att: u32) -> bool {
    dir_def_to_att < 8 && def_mask & (1u8 << dir_def_to_att) != 0
}

/// Terrain plus river term.
pub fn terrain_term(terrain_pct: i32, river_crossed: bool, rules: &Rules) -> i32 {
    terrain_pct + if river_crossed { rules.river_pct } else { 0 }
}

/// What stands on the defender's tile, in the precedence the routine uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Structure {
    /// Nothing.
    None,
    /// Overlay bit 4: `rules.fort_pct`.
    Fortress,
    /// Overlay bit 28: `2 * rules.fort_pct`.
    Barricade,
    /// A city.
    City {
        /// Citizens.
        size: i32,
        /// Resisting citizens.
        resisters: i32,
        /// Building bonus, [`city_building_bonus`].
        building_pct: i32,
    },
}

impl Structure {
    /// Fortress wins over barricade when both overlay bits are set.
    pub fn from_overlay(bit4: bool, bit28: bool) -> Structure {
        if bit4 {
            Structure::Fortress
        } else if bit28 {
            Structure::Barricade
        } else {
            Structure::None
        }
    }
}

/// Defense percentage of the tile contents plus the radar bonus. A city with
/// resisters contributes nothing; a city tile never consults the overlay.
pub fn tile_term(structure: Structure, radar: bool, rules: &Rules) -> i32 {
    let base = match structure {
        Structure::None => 0,
        Structure::Fortress => rules.fort_pct,
        Structure::Barricade => 2 * rules.fort_pct,
        Structure::City { size, resisters, building_pct } => {
            if resisters > 0 {
                0
            } else {
                rules.size_bonus_pct[rules.size_class(size)] + building_pct
            }
        }
    };
    base + if radar { RADAR_PCT } else { 0 }
}

/// A building that is present in the city and feeds [`city_building_bonus`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuildingDefense {
    /// Walls 50, Civil Defense 50.
    pub pct: i32,
    /// Nonzero drops the building for cities above town size (Walls: 8), and
    /// `> 0` makes it eligible for the doubling.
    pub town_limited: i32,
    /// The building's obsoleting tech is known.
    pub obsolete: bool,
}

/// The **maximum** (not the sum) over the city's buildings. `doubler` is a
/// wonder with ability bit `0x1000` (none in `conquests.biq`).
pub fn city_building_bonus(
    size: i32,
    doubler: bool,
    buildings: &[BuildingDefense],
    rules: &Rules,
) -> i32 {
    let mut best = 0;
    for b in buildings {
        if b.obsolete {
            continue;
        }
        if (size > rules.city_max || size > rules.town_max) && b.town_limited != 0 {
            continue;
        }
        let mult = if b.town_limited > 0 && doubler { 2 } else { 1 };
        best = best.max(b.pct * mult);
    }
    best
}

/// Fortify bonus. All four conditions are required. For a unit inside a
/// container, the caller passes the facts of the outermost container.
pub fn fortify_term(land_kind: bool, on_water: bool, fortified: bool, move_left: i32, rules: &Rules) -> i32 {
    if land_kind && !on_water && fortified && move_left > 0 {
        rules.fortify_pct
    } else {
        0
    }
}

/// Barbarian term for the **non-barbarian** side: its difficulty percentage
/// plus 100 when its civ owns the barbarian-bonus wonder.
pub fn barbarian_term(diff_pct: i32, has_wonder: bool) -> i32 {
    diff_pct + if has_wonder { GREAT_WALL_VS_BARBARIANS_PCT } else { 0 }
}

/// Inputs of the amphibious clause.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AmphibiousCheck {
    /// PRTO ability bit 6 (Marine, Berserk).
    pub ability_amphibious: bool,
    /// The attacker's attack strength.
    pub attack_strength: i32,
    /// The "already attacked this turn" status bit.
    pub status_bit2: bool,
    /// Ability bit 2 (Blitz); only consulted with `status_bit2`.
    pub ability_blitz: bool,
    /// The attacker is a land unit.
    pub land_unit: bool,
    /// The defender's tile is water.
    pub target_is_water: bool,
    /// The attacker's tile is water.
    pub origin_is_water: bool,
}

/// `+25` for an amphibious land unit attacking a land tile from a water tile.
pub fn amphibious_term(c: &AmphibiousCheck) -> i32 {
    let eligible = c.ability_amphibious
        && c.attack_strength > 0
        && (!c.status_bit2 || c.ability_blitz)
        && c.land_unit
        && !c.target_is_water
        && c.origin_is_water;
    if eligible {
        AMPHIBIOUS_PCT
    } else {
        0
    }
}

/// Rounded average over an army's carried units, `(sum + n/2) / n`; `None` when
/// the army carries nobody.
pub fn army_average(members: &[i32]) -> Option<i32> {
    if members.is_empty() {
        return None;
    }
    let n = members.len() as i32;
    let sum: i32 = members.iter().sum();
    Some((sum + n / 2) / n)
}

/// Army bonus: `trunc(sum * 0.16666667)`. Zero for an empty army.
pub fn army_bonus(members: &[i32]) -> i32 {
    if members.is_empty() {
        return 0;
    }
    let sum: f32 = members.iter().fold(0.0f32, |acc, &m| acc + m as f32);
    (f64::from(sum) * f64::from(f32::from_bits(0x3E2A_AAAB))) as i32
}

/// Attack strength: army average, else the PRTO attack.
pub fn attack_strength(army_members: &[i32], prto_attack: i32) -> i32 {
    army_average(army_members).unwrap_or(prto_attack)
}

/// Defense strength: army average, else the PRTO defense halved when the unit
/// has been bombarded and the value is above 1.
pub fn defense_strength(army_members: &[i32], prto_defense: i32, bombarded: bool) -> i32 {
    army_average(army_members).unwrap_or(if bombarded && prto_defense > 1 {
        prto_defense >> 1
    } else {
        prto_defense
    })
}

/// Maximum hit points: `max(1, base + hp_bonus)` where `base` is the sum of the
/// carried units' maximum HP for an army with members and the experience base
/// otherwise.
pub fn max_hp(member_hp_sum: Option<i32>, level_base_hp: i32, hp_bonus: i32) -> i32 {
    (member_hp_sum.unwrap_or(level_base_hp) + hp_bonus).max(1)
}

/// Inputs of the odds formula after all terms are summed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OddsInput {
    /// Attacker strength.
    pub att_strength: i32,
    /// The attacker's army bonus.
    pub att_army_bonus: i32,
    /// Attacker percentage `P`: radar + barbarian + amphibious.
    pub att_pct: i32,
    /// Defender strength.
    pub def_strength: i32,
    /// The defender's army bonus.
    pub def_army_bonus: i32,
    /// Defender percentage `D`: terrain, river, tile, fortify, barbarian.
    pub def_pct: i32,
}

/// `1024 * P(defender wins a round)` clamped to `1..=1023`. `X = (def + bonus)
/// * (100 + D)`, `Y = (att + bonus) * (100 + P)`, `1024 * X / (X + Y)`.
/// Returns `None` when `X + Y == 0`.
pub fn defender_round_odds(i: &OddsInput) -> Option<i32> {
    let x = i
        .def_strength
        .wrapping_add(i.def_army_bonus)
        .wrapping_mul(i.def_pct.wrapping_add(100));
    let y = i
        .att_pct
        .wrapping_add(100)
        .wrapping_mul(i.att_strength.wrapping_add(i.att_army_bonus));
    let total = y.wrapping_add(x);
    if total == 0 {
        return None;
    }
    let scaled = i64::from(x.wrapping_shl(10));
    let odds = (scaled / i64::from(total)) as i32;
    Some(odds.clamp(ODDS_MIN, ODDS_MAX))
}

/// Retreat eligibility flags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetreatFlags {
    /// The attacker may retreat.
    pub attacker: bool,
    /// The defender may retreat.
    pub defender: bool,
}

impl RetreatFlags {
    /// A side may retreat only when its maximum movement exceeds one full move;
    /// if **both** sides are fast neither may, and the defender may not retreat
    /// from a tile with a city.
    pub fn new(att_max_move: i32, def_max_move: i32, target_has_city: bool, rules: &Rules) -> Self {
        let mut attacker = att_max_move > rules.move_unit;
        let mut defender = def_max_move > rules.move_unit;
        if attacker && defender {
            attacker = false;
            defender = false;
        }
        if defender && target_has_city {
            defender = false;
        }
        RetreatFlags { attacker, defender }
    }
}

/// One side of a duel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fighter {
    /// Maximum hit points.
    pub max_hp: i32,
    /// Damage taken.
    pub damage: i32,
    /// The experience level's retreat percentage.
    pub retreat_pct: i32,
    /// `owner != 0`; barbarians never retreat.
    pub owned: bool,
}

impl Fighter {
    /// `max_hp - damage`.
    pub fn remaining(&self) -> i32 {
        self.max_hp - self.damage
    }

    fn take_hit(&mut self) {
        self.damage = (self.damage + 1).max(0);
    }
}

/// How a duel ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The defender's remaining HP reached zero.
    AttackerWon,
    /// The attacker's remaining HP reached zero.
    DefenderWon,
    /// The defender passed its retreat roll and found a tile to step to.
    DefenderRetreated,
    /// The attacker passed its retreat roll.
    AttackerRetreated,
}

/// Plays one duel at fixed `odds`.
pub fn duel(
    rng: &mut Rng,
    odds: i32,
    att: &mut Fighter,
    def: &mut Fighter,
    flags: RetreatFlags,
    defender_can_step_back: bool,
) -> Outcome {
    loop {
        let roll = rng.below(ROUND_DIE);
        if roll >= odds {
            def.take_hit();
            if def.remaining() <= 0 {
                return Outcome::AttackerWon;
            }
            if flags.defender && def.owned && def.remaining() == 1 && att.remaining() > 1 {
                let die = (att.retreat_pct + RETREAT_MARGIN) as u32;
                if rng.below(die) < def.retreat_pct && defender_can_step_back {
                    return Outcome::DefenderRetreated;
                }
            }
        } else {
            att.take_hit();
            if att.remaining() <= 0 {
                return Outcome::DefenderWon;
            }
            if flags.attacker && att.owned && att.remaining() == 1 && def.remaining() > 1 {
                let die = (def.retreat_pct + RETREAT_MARGIN) as u32;
                if rng.below(die) < att.retreat_pct {
                    return Outcome::AttackerRetreated;
                }
            }
        }
    }
}

/// The legacy ranged attack's unit pass: draws exactly `rate_of_fire` dice, and
/// the loop never exits early. This routine writes no damage.
pub fn bombard_hit(rng: &mut Rng, odds: i32, rate_of_fire: i32) -> bool {
    let mut hit = false;
    for _ in 0..rate_of_fire.max(0) {
        if rng.below(ROUND_DIE) >= odds {
            hit = true;
        }
    }
    hit
}

/// Which part of a city a ranged strike attacks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrikeMode {
    /// Kill one citizen. Needs city size above 1.
    Population = 0,
    /// Destroy one random non-wonder building.
    Improvement = 1,
}

impl StrikeMode {
    /// The city's implicit base strength for this mode.
    pub fn base(self, rules: &Rules) -> i32 {
        rules.city_strike_base[self as usize]
    }

    /// The mode value the routine accepts; anything else is not a strike.
    pub fn from_mode(mode: i32) -> Option<StrikeMode> {
        match mode {
            0 => Some(StrikeMode::Population),
            1 => Some(StrikeMode::Improvement),
            _ => None,
        }
    }
}

/// The implicit strength a strike target holds against a bombard shot:
/// `((terrain + tile + 100) * base) / 100`.
pub fn implicit_strength(base: i32, terrain_pct: i32, tile_pct: i32) -> i32 {
    terrain_pct.wrapping_add(tile_pct).wrapping_add(100).wrapping_mul(base) / 100
}

/// The odds the target holds in a strike roll: `1024 * v / (v + strength)`,
/// clamped. `None` when the denominator is zero.
pub fn strike_odds(base: i32, terrain_pct: i32, tile_pct: i32, strike_strength: i32) -> Option<i32> {
    let v = implicit_strength(base, terrain_pct, tile_pct);
    let denom = v.wrapping_add(strike_strength);
    if denom == 0 {
        return None;
    }
    let odds = (i64::from(v.wrapping_shl(10)) / i64::from(denom)) as i32;
    Some(odds.clamp(ODDS_MIN, ODDS_MAX))
}

/// Number of dice a city strike may throw: `max(rate_of_fire, requested)`.
pub fn strike_rolls(rate_of_fire: i32, requested: i32) -> i32 {
    rate_of_fire.max(requested)
}

/// Throws up to `rolls` dice and **stops at the first success**, a die at or
/// above the odds.
pub fn strike_hit(rng: &mut Rng, odds: i32, rolls: i32) -> bool {
    for _ in 0..rolls.max(0) {
        if rng.below(ROUND_DIE) >= odds {
            return true;
        }
    }
    false
}

/// The unit's domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Domain {
    /// Foot and wheeled units.
    Land = 0,
    /// Ships.
    Sea = 1,
    /// Aircraft and missiles.
    Air = 2,
}

/// Can `victim` be the target of a defensive bombard? Land and sea units need at
/// least 2 HP left, so a bombard hit never kills. Air units have no floor.
pub fn defensive_bombard_victim_ok(domain: Domain, defense: i32, remaining_hp: i32) -> bool {
    defense > 0 && (domain == Domain::Air || remaining_hp > 1)
}

/// One unit standing on the defender's tile, as the shooter scan sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShooterCandidate {
    /// The candidate is the defender itself.
    pub is_defender: bool,
    /// The candidate is loaded in the defender.
    pub carried_by_defender: bool,
    /// Must equal the victim's domain.
    pub domain: Domain,
    /// Must be positive.
    pub bombard_strength: i32,
    /// Such units never shoot.
    pub has_ability_3: bool,
    /// Already fired this turn.
    pub already_fired: bool,
}

/// Index of the shooter among the units on the defender's tile: the **strictly**
/// highest bombard strength wins, so the first of equals stays.
pub fn pick_defensive_shooter(victim: Domain, tile_units: &[ShooterCandidate]) -> Option<usize> {
    let mut best = 0;
    let mut pick = None;
    for (i, u) in tile_units.iter().enumerate() {
        let eligible = !u.is_defender
            && !u.carried_by_defender
            && u.domain == victim
            && u.bombard_strength > 0
            && !u.has_ability_3
            && !u.already_fired;
        if eligible && u.bombard_strength > best {
            best = u.bombard_strength;
            pick = Some(i);
        }
    }
    pick
}

/// Result of one defensive-bombard shot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DefensiveBombard {
    /// The die was at or above the odds.
    pub hit: bool,
    /// The hit left the victim at exactly 1 HP.
    pub left_at_one_hp: bool,
}

/// Exactly one die against `odds`. A hit adds one point of damage; a miss
/// changes nothing.
pub fn defensive_bombard(rng: &mut Rng, odds: i32, victim: &mut Fighter) -> DefensiveBombard {
    let hit = rng.below(ROUND_DIE) >= odds;
    if hit {
        victim.take_hit();
    }
    DefensiveBombard { hit, left_at_one_hp: hit && victim.remaining() == 1 }
}

/// The interception duel runs at once unless the interceptor has no attack and
/// the aircraft no defense.
pub fn interception_runs(interceptor_attack: i32, aircraft_defense: i32) -> bool {
    interceptor_attack != 0 || aircraft_defense != 0
}

/// The interception duel: the ordinary round die and damage rule, and **no
/// retreat**. `AttackerWon` means the interceptor shot the aircraft down.
pub fn interception_duel(
    rng: &mut Rng,
    odds: i32,
    interceptor: &mut Fighter,
    aircraft: &mut Fighter,
) -> Outcome {
    let no_retreat = RetreatFlags { attacker: false, defender: false };
    duel(rng, odds, interceptor, aircraft, no_retreat, false)
}

/// Verdict of the defender legality filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Eligibility {
    /// The filter returns true.
    Eligible,
    /// The filter returns false and does nothing else.
    Ineligible,
    /// A unit with defense but no HP left: a kill is queued.
    Zombie,
}

/// What the legality filter looks at for one candidate on a tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DefenderFacts {
    /// Loaded units never defend on their own.
    pub carried: bool,
    /// The unit's domain.
    pub domain: Domain,
    /// The tile's water predicate.
    pub tile_is_water: bool,
    /// The unit's defense strength.
    pub defense: i32,
    /// Remaining HP.
    pub remaining_hp: i32,
}

/// Sea units need a water tile, land units a land tile, and **air units are
/// never eligible**. A unit with zero defense is eligible whatever its HP;
/// otherwise it needs HP left, and at or below zero it is a
/// [`Eligibility::Zombie`].
pub fn defender_eligibility(f: &DefenderFacts) -> Eligibility {
    if f.carried {
        return Eligibility::Ineligible;
    }
    let tile_ok = match f.domain {
        Domain::Sea => f.tile_is_water,
        Domain::Land => !f.tile_is_water,
        Domain::Air => false,
    };
    if !tile_ok {
        return Eligibility::Ineligible;
    }
    if f.defense == 0 || f.remaining_hp > 0 {
        Eligibility::Eligible
    } else {
        Eligibility::Zombie
    }
}

/// The rating both selection loops use: `(100 + fortify) * defense *
/// clamp(remaining, 0, 9999) / 100`.
pub fn defender_rating(fortify_pct: i32, defense: i32, remaining_hp: i32) -> i32 {
    (fortify_pct + 100).wrapping_mul(defense).wrapping_mul(remaining_hp.clamp(0, 9999)) / 100
}

/// Starting "best rating" of the strongest-defender loop.
pub const BEST_DEFENDER_START: i32 = 0;
/// Starting "best rating" of the weakest-defender loop.
pub const WEAKEST_DEFENDER_START: i32 = 1000;

/// One eligible candidate as the comparators see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DefenderRank {
    /// [`defender_rating`].
    pub rating: i32,
    /// The King.
    pub is_king: bool,
    /// Loaded units: `0` for an Army, else the cargo count.
    pub cargo: i32,
    /// The attack strength.
    pub attack: i32,
    /// The bombard strength.
    pub bombard: i32,
    /// The maximum HP.
    pub max_hp: i32,
}

fn replaces_best(
    c: &DefenderRank,
    best: Option<&DefenderRank>,
    best_rating: i32,
    ties_need_positive: bool,
    strongest: bool,
) -> bool {
    if let Some(b) = best {
        if c.rating > 0 {
            match (c.is_king, b.is_king) {
                (true, false) => return !strongest,
                (false, true) => return strongest,
                _ => {}
            }
        }
    }
    let rating_wins = if strongest { c.rating > best_rating } else { c.rating < best_rating };
    if rating_wins {
        return true;
    }
    if c.rating == best_rating && c.rating == 0 && c.is_king {
        return true;
    }
    let tie_ok = if ties_need_positive { best_rating > 0 } else { best_rating >= 0 };
    if !tie_ok || c.rating != best_rating {
        return false;
    }
    let Some(b) = best else {
        return true;
    };
    for (x, y) in [
        (c.cargo, b.cargo),
        (c.attack, b.attack),
        (c.bombard, b.bombard),
        (c.max_hp, b.max_hp),
    ] {
        if x != y {
            return if strongest { x < y } else { x > y };
        }
    }
    false
}

/// The strongest-defender comparator.
pub fn is_better_defender(
    c: &DefenderRank,
    best: Option<&DefenderRank>,
    best_rating: i32,
    ties_need_positive: bool,
) -> bool {
    replaces_best(c, best, best_rating, ties_need_positive, true)
}

/// The weakest-defender comparator.
pub fn is_weaker_defender(
    c: &DefenderRank,
    best: Option<&DefenderRank>,
    best_rating: i32,
    ties_need_positive: bool,
) -> bool {
    replaces_best(c, best, best_rating, ties_need_positive, false)
}

/// The strongest-defender loop over already-filtered candidates in tile order.
pub fn pick_best_defender(ranks: &[DefenderRank]) -> Option<usize> {
    let mut best: Option<usize> = None;
    let mut best_rating = BEST_DEFENDER_START;
    for (i, r) in ranks.iter().enumerate() {
        if is_better_defender(r, best.map(|b| &ranks[b]), best_rating, true) {
            best = Some(i);
            best_rating = r.rating;
        }
    }
    best
}

/// The weakest-defender loop. Zero-rated candidates are skipped outright.
pub fn pick_weakest_defender(ranks: &[DefenderRank]) -> Option<usize> {
    let mut best: Option<usize> = None;
    let mut best_rating = WEAKEST_DEFENDER_START;
    for (i, r) in ranks.iter().enumerate() {
        if r.rating == 0 {
            continue;
        }
        if is_weaker_defender(r, best.map(|b| &ranks[b]), best_rating, true) {
            best = Some(i);
            best_rating = r.rating;
        }
    }
    best
}

/// **Lethal Land Bombardment**.
pub const ABILITY_LETHAL_LAND_BOMBARD: u32 = 27;
/// **Lethal Sea Bombardment**.
pub const ABILITY_LETHAL_SEA_BOMBARD: u32 = 28;

/// Size of the die that picks the city mode when a ranged attack finds no unit
/// to shoot at in a city.
pub const NO_TARGET_CITY_MODE_DIE: u32 = 2;

/// Which domains the attacker may reduce to zero HP, from its abilities 27 and
/// 28. Everything else is held at 1 HP.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Lethality {
    /// Ability 27.
    pub land: bool,
    /// Ability 28.
    pub sea: bool,
}

impl Lethality {
    /// A unit with neither ability.
    pub const NONE: Lethality = Lethality { land: false, sea: false };
    /// What the Cruise Missile has.
    pub const BOTH: Lethality = Lethality { land: true, sea: true };

    /// Is a unit of this domain protected by the 1 HP floor?
    pub fn floor_applies(self, domain: Domain) -> bool {
        match domain {
            Domain::Land => !self.land,
            Domain::Sea => !self.sea,
            Domain::Air => false,
        }
    }
}

/// The classes of target a ranged attacker works through, in order. The first
/// class that holds a legal target supplies the defender.
pub fn bombard_target_order(
    attacker: Domain,
    cruise_missile_flag: bool,
    tile_is_water: bool,
) -> &'static [Domain] {
    const SEA_AIR_LAND: [Domain; 3] = [Domain::Sea, Domain::Air, Domain::Land];
    const AIR_SEA_LAND: [Domain; 3] = [Domain::Air, Domain::Sea, Domain::Land];
    if cruise_missile_flag {
        return &SEA_AIR_LAND;
    }
    match attacker {
        Domain::Land if tile_is_water => &[Domain::Sea],
        Domain::Land => &[Domain::Land],
        Domain::Sea => &SEA_AIR_LAND,
        Domain::Air => &AIR_SEA_LAND,
    }
}

/// The tile a ranged attack lands on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BombardTile {
    /// The cell's water predicate.
    pub is_water: bool,
    /// The tile has a city.
    pub has_city: bool,
}

/// One unit on the target tile, as the filter sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BombardCandidate {
    /// Loaded units are never picked.
    pub carried: bool,
    /// The unit's defense strength.
    pub defense: i32,
    /// The unit's domain.
    pub domain: Domain,
    /// Remaining HP.
    pub remaining_hp: i32,
    /// Hostile to the attacker.
    pub hostile: bool,
}

/// Is `c` a legal target of `class`? In the order of the tests: not carried and
/// defense above zero; the 1 HP floor; hostile and of the class being scanned;
/// the tile domain rules.
pub fn bombard_candidate_ok(
    c: &BombardCandidate,
    class: Domain,
    tile: &BombardTile,
    lethal: Lethality,
) -> bool {
    if c.carried || c.defense <= 0 {
        return false;
    }
    if lethal.floor_applies(c.domain) && c.remaining_hp.clamp(0, 9999) <= 1 {
        return false;
    }
    if !c.hostile || c.domain != class {
        return false;
    }
    match c.domain {
        Domain::Sea => tile.is_water || tile.has_city,
        Domain::Land => !tile.is_water,
        Domain::Air => !tile.is_water && tile.has_city,
    }
}

/// The target selection: for each class in `order`, rate the legal candidates
/// with the strongest-defender comparator and stop at the first class that
/// yields one. Returns the index into `units`, which are in tile order.
pub fn pick_bombard_target(
    order: &[Domain],
    tile: &BombardTile,
    lethal: Lethality,
    units: &[(BombardCandidate, DefenderRank)],
) -> Option<usize> {
    for &class in order {
        let legal: Vec<usize> = (0..units.len())
            .filter(|&i| bombard_candidate_ok(&units[i].0, class, tile, lethal))
            .collect();
        let ranks: Vec<DefenderRank> = legal.iter().map(|&i| units[i].1).collect();
        if let Some(k) = pick_best_defender(&ranks) {
            return Some(legal[k]);
        }
    }
    None
}

/// How many units on the tile are legal targets, over **all** classes of
/// `order`, never counting the lethal abilities.
pub fn count_bombard_targets(order: &[Domain], tile: &BombardTile, units: &[BombardCandidate]) -> i32 {
    order
        .iter()
        .map(|&class| {
            units
                .iter()
                .filter(|c| bombard_candidate_ok(c, class, tile, Lethality::NONE))
                .count() as i32
        })
        .sum()
}

/// The die an aircraft throws over a city to choose between hitting the city and
/// shooting its units.
pub fn air_city_mode_die(targets: i32) -> u32 {
    let mut k = if targets > 8 { 5 } else { 4 };
    if targets > 4 {
        k += 1;
    }
    k
}

/// A sea unit standing in a **city** is harder to hit by the ranged attack:
/// `odds = ftol((odds + 1) * 0.5)`.
pub fn port_odds(odds: i32) -> i32 {
    (odds + 1) / 2
}

/// How a [`ranged_volley`] ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VolleyEnd {
    /// Every shot was thrown and the target survived.
    Exhausted,
    /// A hit brought the target to zero HP or below.
    Killed,
    /// A hit left the target at exactly 1 HP and the attacker is not lethal
    /// against its domain.
    Spared,
}

/// Result of [`ranged_volley`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Volley {
    /// Shots that scored a hit; each is one point of damage.
    pub hits: i32,
    /// Why it stopped.
    pub end: VolleyEnd,
    /// Some hit left the target at exactly 1 HP.
    pub left_at_one_hp: bool,
}

/// The shot loop against one chosen target.
pub fn ranged_volley(
    rng: &mut Rng,
    odds: i32,
    rate_of_fire: i32,
    target: &mut Fighter,
    domain: Domain,
    lethal: Lethality,
) -> Volley {
    let mut v = Volley { hits: 0, end: VolleyEnd::Exhausted, left_at_one_hp: false };
    for _ in 0..rate_of_fire.max(0) {
        if rng.below(ROUND_DIE) < odds {
            continue;
        }
        target.take_hit();
        v.hits += 1;
        let rem = target.remaining();
        if rem == 1 {
            v.left_at_one_hp = true;
            if lethal.floor_applies(domain) {
                v.end = VolleyEnd::Spared;
                return v;
            }
        } else if rem <= 0 {
            v.end = VolleyEnd::Killed;
            return v;
        }
    }
    v
}

/// One BLDG row as the city-walls routines see it for a particular city.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FacilityFacts {
    /// The city itself holds the building.
    pub in_city: bool,
    /// As above, or the building acts on the city through a wider list.
    pub acts_on_city: bool,
    /// The building's obsoleting tech is known.
    pub obsolete: bool,
    /// Land bombardment defense (Walls: 8).
    pub land_defense: i32,
    /// Naval bombardment defense (Coastal Fortress: 8).
    pub sea_defense: i32,
}

/// The land bombardment defense of a city. Zero above town size; otherwise the
/// **largest** `land_defense` over qualifying buildings, times `wonder_count +
/// 1`.
pub fn land_bombard_defense(
    pop: i32,
    facilities: &[FacilityFacts],
    wonder_count: i32,
    rules: &Rules,
) -> i32 {
    if pop > rules.city_max || pop > rules.town_max {
        return 0;
    }
    facilities
        .iter()
        .filter(|b| b.acts_on_city && !b.obsolete)
        .map(|b| b.land_defense.wrapping_mul(wonder_count + 1))
        .fold(0, i32::max)
}

/// The naval bombardment defense: the largest `sea_defense` over qualifying
/// buildings, at **any** city size, with no wonder multiplier.
pub fn sea_bombard_defense(facilities: &[FacilityFacts]) -> i32 {
    facilities
        .iter()
        .filter(|b| b.acts_on_city && !b.obsolete)
        .map(|b| b.sea_defense)
        .fold(0, i32::max)
}

/// What the wall routines do to the city after a successful wall roll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FacilityHit {
    /// The building was removed.
    Destroyed(usize),
    /// Only a building that acts through a wider list qualified: nothing is
    /// removed and the attack goes on to the units.
    Reported(usize),
    /// Nothing qualified.
    Nothing,
}

/// The land variant. Above town size it finds nothing. Pass 0 looks only at
/// buildings the city holds; pass 1 repeats the scan over buildings that act on
/// the city, and then only reports.
pub fn land_facility_hit(pop: i32, facilities: &[FacilityFacts], rules: &Rules) -> FacilityHit {
    if pop > rules.city_max || pop > rules.town_max {
        return FacilityHit::Nothing;
    }
    for pass in 0..2 {
        let mut best = -1;
        let mut pick = None;
        for (i, b) in facilities.iter().enumerate() {
            let present = if pass == 0 { b.in_city } else { b.acts_on_city };
            if present && !b.obsolete && b.land_defense > best {
                best = b.land_defense;
                pick = Some(i);
            }
        }
        if let Some(i) = pick {
            return if pass == 0 {
                FacilityHit::Destroyed(i)
            } else {
                FacilityHit::Reported(i)
            };
        }
    }
    FacilityHit::Nothing
}

/// The sea variant: no size limit, the running best starts at 0, and both of its
/// passes use the same "in the city" test, so it never reports.
pub fn sea_facility_hit(facilities: &[FacilityFacts]) -> FacilityHit {
    let mut best = 0;
    let mut pick = None;
    for (i, b) in facilities.iter().enumerate() {
        if b.in_city && !b.obsolete && b.sea_defense > best {
            best = b.sea_defense;
            pick = Some(i);
        }
    }
    pick.map_or(FacilityHit::Nothing, FacilityHit::Destroyed)
}

/// What a land or sea ranged attacker throws against a city.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WallsAttack {
    /// The attacker's domain; aircraft have no wall roll.
    pub domain: Domain,
    /// The bombard strength.
    pub bombard: i32,
    /// The number of dice.
    pub rate_of_fire: i32,
    /// The terrain percentage.
    pub terrain_pct: i32,
    /// The tile percentage.
    pub tile_pct: i32,
}

/// The city being attacked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CityWalls<'a> {
    /// The city's population.
    pub pop: i32,
    /// One entry per BLDG row, in table order.
    pub facilities: &'a [FacilityFacts],
    /// See [`land_bombard_defense`].
    pub wonder_count: i32,
}

/// Outcome of the wall roll of a ranged attack on a city.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WallsStep {
    /// No wall defense applies: the units are attacked at once.
    Skipped,
    /// Every die missed: the whole attack ends here.
    Missed,
    /// A die succeeded.
    Hit(FacilityHit),
}

impl WallsStep {
    /// Does the unit pass still run after this step?
    pub fn attacks_units(self) -> bool {
        matches!(self, WallsStep::Skipped | WallsStep::Hit(FacilityHit::Reported(_)))
    }
}

/// The wall roll. `v <= 0` skips the roll; otherwise up to `rate_of_fire` dice
/// are thrown, **stopping at the first die at or above the odds**, and that
/// success removes one building.
pub fn walls_step(
    rng: &mut Rng,
    attack: &WallsAttack,
    city: &CityWalls<'_>,
    rules: &Rules,
) -> WallsStep {
    let defense = match attack.domain {
        Domain::Land => land_bombard_defense(city.pop, city.facilities, city.wonder_count, rules),
        Domain::Sea => sea_bombard_defense(city.facilities),
        Domain::Air => return WallsStep::Skipped,
    };
    if implicit_strength(defense, attack.terrain_pct, attack.tile_pct) <= 0 {
        return WallsStep::Skipped;
    }
    let Some(odds) = strike_odds(defense, attack.terrain_pct, attack.tile_pct, attack.bombard) else {
        return WallsStep::Skipped;
    };
    if !strike_hit(rng, odds, attack.rate_of_fire) {
        return WallsStep::Missed;
    }
    WallsStep::Hit(match attack.domain {
        Domain::Sea => sea_facility_hit(city.facilities),
        _ => land_facility_hit(city.pop, city.facilities, rules),
    })
}

/// Base promotion die by the winner's current level 0, 1, 2. Level 3 and above
/// never roll for promotion.
pub const PROMOTION_DIE: [u32; 3] = [2, 4, 8];

/// The promotion die: the base for the level, doubled when the loser is a
/// barbarian, then halved when the winner's civ is Militaristic.
pub fn promotion_die(level: i32, loser_is_barbarian: bool, militaristic: bool) -> Option<u32> {
    let mut die = *PROMOTION_DIE.get(usize::try_from(level).ok()?)?;
    if loser_is_barbarian {
        die *= 2;
    }
    if militaristic {
        die /= 2;
    }
    Some(die)
}

/// A unit promotes when `next(die) == 0`. A unit that failed earlier this turn
/// skips the die and is promoted without drawing.
pub fn promotion_roll(rng: &mut Rng, die: u32, failed_earlier_this_turn: bool) -> bool {
    failed_earlier_this_turn || rng.below(die) == 0
}

/// A level-3 (elite) winner rolls for a Great Leader instead.
pub const LEADER_LEVEL: i32 = 3;
/// Leader die without the Heroic Epic.
pub const LEADER_DIE: u32 = 16;
/// Leader die for a civ that owns the Heroic Epic.
pub const LEADER_DIE_WITH_EPIC: u32 = 12;

/// What the Great Leader branch tests before it rolls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LeaderCheck {
    /// The winner's level.
    pub winner_level: i32,
    /// The winner is a land unit.
    pub winner_is_land: bool,
    /// The loser is a barbarian.
    pub loser_is_barbarian: bool,
    /// This unit has already produced a leader.
    pub winner_already_made_leader: bool,
    /// The winner is carried by an Army.
    pub winner_in_army: bool,
    /// The civ already has a unit of the Leader prototype.
    pub civ_has_leader: bool,
}

/// All conditions of the Great Leader branch.
pub fn leader_eligible(c: &LeaderCheck) -> bool {
    c.winner_level >= LEADER_LEVEL
        && c.winner_is_land
        && !c.loser_is_barbarian
        && !c.winner_already_made_leader
        && !c.winner_in_army
        && !c.civ_has_leader
}

/// The leader die: 16, or 12 with the Heroic Epic, doubled when the *defender*
/// won.
pub fn leader_die(has_heroic_epic: bool, attacker_won: bool) -> u32 {
    let base = if has_heroic_epic { LEADER_DIE_WITH_EPIC } else { LEADER_DIE };
    if attacker_won {
        base
    } else {
        base * 2
    }
}

/// A leader appears when `next(die) == 0`.
pub fn leader_roll(rng: &mut Rng, die: u32) -> bool {
    rng.below(die) == 0
}

/// Chance in percent that a unit with the Enslave action converts the loser.
pub const ENSLAVE_PERCENT: u32 = 33;

/// One `next(100)` per victory of an enslaving unit.
pub fn enslave_roll(rng: &mut Rng) -> bool {
    (rng.below(100) as u32) < ENSLAVE_PERCENT
}

/// The Golden Age trigger: the winner has ability 15, the loser is not a
/// barbarian, and the civ has no golden age scheduled.
pub fn golden_age_triggers(unique_unit: bool, loser_is_barbarian: bool, golden_age_end_turn: i32) -> bool {
    unique_unit && !loser_is_barbarian && golden_age_end_turn == -1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_bands_and_diagonals() {
        assert_eq!(dir_from_delta(0, -1), 0);
        assert_eq!(dir_from_delta(1, -1), 1);
        assert_eq!(dir_from_delta(1, 0), 2);
        assert_eq!(dir_from_delta(1, 1), 3);
        assert_eq!(dir_from_delta(0, 1), 4);
        assert_eq!(dir_from_delta(-1, 1), 5);
        assert_eq!(dir_from_delta(-1, 0), 6);
        assert_eq!(dir_from_delta(-1, -1), 7);
        // Mostly horizontal, mostly vertical, and the diagonal in between.
        assert_eq!(dir_from_delta(3, 1), 2);
        assert_eq!(dir_from_delta(1, 3), 4);
        assert_eq!(dir_from_delta(2, 2), 3);
    }

    #[test]
    fn river_edge_reads_the_direction_bit() {
        for d in 0u32..8 {
            assert!(river_edge(1 << d, d));
            assert!(!river_edge(0, d));
        }
        assert!(!river_edge(0xFF, 8), "direction 8 is out of range");
    }

    #[test]
    fn odds_clamp_and_split_at_equal_strength() {
        let even = OddsInput {
            att_strength: 10,
            att_army_bonus: 0,
            att_pct: 0,
            def_strength: 10,
            def_army_bonus: 0,
            def_pct: 0,
        };
        assert_eq!(defender_round_odds(&even), Some(512));
        // A defender that cannot lose a round: odds clamp to 1023.
        let strong = OddsInput { att_strength: 0, def_strength: 100, ..even };
        assert_eq!(defender_round_odds(&strong), Some(ODDS_MAX), "clamped up");
        // An attacker that cannot lose a round: odds clamp to 1.
        let weak = OddsInput { att_strength: 100, def_strength: 0, ..even };
        assert_eq!(defender_round_odds(&weak), Some(ODDS_MIN), "clamped down");
        let zero = OddsInput { att_strength: 0, def_strength: 0, ..even };
        assert_eq!(defender_round_odds(&zero), None);
    }

    #[test]
    fn tile_term_reads_structure_and_radar() {
        let rules = Rules::CONQUESTS;
        assert_eq!(tile_term(Structure::None, false, &rules), 0);
        assert_eq!(tile_term(Structure::Fortress, false, &rules), 50);
        assert_eq!(tile_term(Structure::Barricade, false, &rules), 100);
        assert_eq!(tile_term(Structure::Fortress, true, &rules), 75);
        let city = Structure::City { size: 13, resisters: 0, building_pct: 50 };
        assert_eq!(tile_term(city, false, &rules), 150, "metropolis + walls");
        let resisting = Structure::City { size: 13, resisters: 1, building_pct: 50 };
        assert_eq!(tile_term(resisting, false, &rules), 0);
    }

    #[test]
    fn fortify_and_amphibious_terms() {
        let rules = Rules::CONQUESTS;
        assert_eq!(fortify_term(true, false, true, 1, &rules), 25);
        assert_eq!(fortify_term(true, false, true, 0, &rules), 0);
        assert_eq!(fortify_term(true, true, true, 1, &rules), 0);
        let base = AmphibiousCheck {
            ability_amphibious: true,
            attack_strength: 8,
            status_bit2: false,
            ability_blitz: false,
            land_unit: true,
            target_is_water: false,
            origin_is_water: true,
        };
        assert_eq!(amphibious_term(&base), 25);
        assert_eq!(amphibious_term(&AmphibiousCheck { origin_is_water: false, ..base }), 0);
    }

    #[test]
    fn army_average_rounds_and_bonus_is_a_sixth() {
        assert_eq!(army_average(&[]), None);
        assert_eq!(army_average(&[3]), Some(3));
        assert_eq!(army_average(&[1, 2]), Some(2)); // (3 + 1) / 2
        assert_eq!(army_bonus(&[]), 0);
        assert_eq!(army_bonus(&[6]), 1);
        assert_eq!(army_bonus(&[3, 3, 3]), 1);
    }

    #[test]
    fn a_duel_ends_when_a_fighter_dies() {
        let mut rng = Rng::new(1);
        let mut att = Fighter { max_hp: 3, damage: 0, retreat_pct: 0, owned: true };
        let mut def = Fighter { max_hp: 3, damage: 0, retreat_pct: 0, owned: true };
        let out = duel(&mut rng, 512, &mut att, &mut def, RetreatFlags { attacker: false, defender: false }, false);
        assert!(matches!(out, Outcome::AttackerWon | Outcome::DefenderWon));
        assert!(att.remaining() <= 0 || def.remaining() <= 0);
    }

    #[test]
    fn defender_selection_keeps_the_strongest_and_weakest() {
        let strong = DefenderRank { rating: 300, is_king: false, cargo: 0, attack: 5, bombard: 0, max_hp: 4 };
        let weak = DefenderRank { rating: 100, is_king: false, cargo: 0, attack: 1, bombard: 0, max_hp: 2 };
        assert_eq!(pick_best_defender(&[weak, strong]), Some(1));
        assert_eq!(pick_weakest_defender(&[weak, strong]), Some(0));
        let zero = DefenderRank { rating: 0, ..weak };
        assert_eq!(pick_weakest_defender(&[zero]), None, "zero-rated are skipped");
    }

    #[test]
    fn bombard_target_order_switches_by_domain_and_flag() {
        assert_eq!(bombard_target_order(Domain::Sea, false, true), &[Domain::Sea, Domain::Air, Domain::Land]);
        assert_eq!(bombard_target_order(Domain::Air, false, true), &[Domain::Air, Domain::Sea, Domain::Land]);
        assert_eq!(bombard_target_order(Domain::Land, false, true), &[Domain::Sea]);
        assert_eq!(bombard_target_order(Domain::Land, false, false), &[Domain::Land]);
        assert_eq!(bombard_target_order(Domain::Land, true, false), &[Domain::Sea, Domain::Air, Domain::Land]);
    }

    #[test]
    fn the_one_hp_floor_spares_unless_lethal() {
        let mut rng = Rng::new(0);
        let mut target = Fighter { max_hp: 3, damage: 1, retreat_pct: 0, owned: true };
        let v = ranged_volley(&mut rng, 1, 5, &mut target, Domain::Land, Lethality::NONE);
        assert_eq!(v.end, VolleyEnd::Spared);
        assert!(v.left_at_one_hp);
        let mut rng = Rng::new(0);
        let mut target = Fighter { max_hp: 3, damage: 1, retreat_pct: 0, owned: true };
        let v = ranged_volley(&mut rng, 1, 5, &mut target, Domain::Land, Lethality::BOTH);
        assert_eq!(v.end, VolleyEnd::Killed);
    }

    #[test]
    fn promotion_and_leader_dice() {
        assert_eq!(promotion_die(0, false, false), Some(2));
        assert_eq!(promotion_die(0, true, false), Some(4));
        assert_eq!(promotion_die(2, false, true), Some(4));
        assert_eq!(promotion_die(3, false, false), None);
        assert_eq!(leader_die(false, true), 16);
        assert_eq!(leader_die(true, true), 12);
        assert_eq!(leader_die(false, false), 32);
        let mut rng = Rng::new(0);
        assert!(promotion_roll(&mut rng, 8, true), "a failed earlier roll promotes");
    }
}
