//! Combat resolution: defender choice, round odds, damage, retreat, ranged
//! attacks (target choice, the 1 HP floor, city walls), interception, strikes
//! on cities and tiles, and the post-victory rolls (promotion, golden age,
//! great leader, enslavement).
//!
//! See `../combat.md` for the evidence. Every function names the address it
//! mirrors. The dice are the shared `Random` class ([`crate::rng::Rng`]) used
//! through the **global gameplay instance `0xA526B4`**, not MSVC `rand()`.
//!
//! The model is one *duel* at fixed odds:
//!
//! ```text
//! odds      = 0x4A0ED0(A, B, bombard, flag4)        // P(defender wins a round) * 1024
//! each round: roll = next(1024)                     // 0x4A5B3C
//!             roll >= odds  -> attacker wins the round, defender.damage += 1
//!             roll <  odds  -> defender wins the round, attacker.damage += 1
//! a fighter dies when maxHP - damage <= 0           // 0x4A5C44..0x4A5C59
//! ```
//!
//! Percentages are additive: the defender side is `100 + D`, the attacker side
//! `100 + P`, and the odds are `1024 * X / (X + Y)` clamped to `1..=1023`.
//!
//! What is *not* modelled (unread, so no stubs): the tile-improvement
//! destruction chosen by `0x5B4DC0` after a successful tile strike, the kill
//! function `0x5BBBC0`, the stack/army front-unit reselection that re-runs the
//! odds, the order in which the game's unit lists enumerate a tile (callers
//! pass slices in that order), the meaning of unit status bit `0x01` (set at
//! `0x5BEAF6`) and of overlay bit 29, where the Military Academy's army bonus
//! is consumed (it is not among the `0x55AA10` wonder-flag tests), the
//! building removal itself (`0x4ACF40`), the nuke strike `0x5B3D10`, and the
//! AI's own use of the target selector (`0x44C5D8`, `0x44CD5C`).

use crate::rng::Rng;

/// The round die: `push 0x400; mov ecx,0xA526B4; call 0x60BAB0` (`0x4A5B3C`).
pub const ROUND_DIE: u32 = 0x400;
/// Lower clamp of the odds (`0x4A11C9..0x4A11CB`).
pub const ODDS_MIN: i32 = 1;
/// Upper clamp of the odds, `0x3FF` (`0x4A11D3..0x4A11DA`).
pub const ODDS_MAX: i32 = 0x3FF;
/// Radar-tower bonus, a literal `0x19` in both consumers (`0x4A105E`,
/// `0x56D021`). The bit is a per-civ mask in `cell+0xD4`, see `combat.md`.
pub const RADAR_PCT: i32 = 25;
/// Amphibious-assault bonus, literal `0x19` (`0x4A1187`).
pub const AMPHIBIOUS_PCT: i32 = 25;
/// Bonus for owning the barbarian-bonus wonder (the Great Wall in
/// `conquests.biq`), literal `0x64` (`0x4A1025`, `0x4A10C1`).
pub const GREAT_WALL_VS_BARBARIANS_PCT: i32 = 100;
/// Added to the opponent's retreat percentage to form the retreat die:
/// `add edx,0x32` (`0x4A64FA`, `0x4A7003`).
pub const RETREAT_MARGIN: i32 = 50;
/// Base strength of a unit-less, city-less tile in the strike roll:
/// `shl ecx,4` (`0x4A25C5`). City strikes use the RULE words
/// [`Rules::city_strike_base`] instead, which are also 16 in `conquests.biq`.
pub const TILE_DEFENSE_STRENGTH: i32 = 16;

/// RULE-section values the combat code reads, as globals in the RULE object
/// at `0x9C71E4` (`combat.md` has the address-to-file-offset table).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rules {
    /// Fortified-unit bonus, `[0x9C72F0]` (RULE body `+0x128`).
    pub fortify_pct: i32,
    /// River-crossing bonus, `[0x9C72B8]` (body `+0xF0`).
    pub river_pct: i32,
    /// Fortress overlay bonus, `[0x9C726C]` (body `+0xA8`). A barricade is
    /// twice this (`lea ecx,[eax+eax]` at `0x56CFD0`).
    pub fort_pct: i32,
    /// Town / City / Metropolis defense bonus, `[0x9C72D8..0x9C72E0]`
    /// (body `+0x110..+0x118`).
    pub size_bonus_pct: [i32; 3],
    /// Largest town size, `[0x9C72E4]` (body `+0x11C`).
    pub town_max: i32,
    /// Largest city size, `[0x9C72E8]` (body `+0x120`).
    pub city_max: i32,
    /// Internal movement units per full move, `[0x9C72C8]` (body `+0x100`).
    pub move_unit: i32,
    /// Base strength of a city in the ranged city strike `0x4A2650`, indexed
    /// by mode: `[mode 0, mode 1]` = `[[0x9C7298], [0x9C7294]]` (body
    /// `+0xD4`, `+0xD0`).
    pub city_strike_base: [i32; 2],
}

impl Rules {
    /// The values decoded from `conquests.biq` (verified by the file loader's
    /// read order and by the game's own Civilopedia combat table).
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

    /// Size class of a city: 0 town, 1 city, 2 metropolis (`0x56CF2E..0x56CF4E`).
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

/// `DIFF` row field at memory `+0x64` (body `+0x60`) per difficulty level:
/// Chieftain, Warlord, Regent, Monarch, Emperor, Demigod, Deity, Sid. The
/// non-barbarian side of a fight against barbarians gets this percentage
/// (`0x4A1000`, `0x4A1098`). `conquests.biq` values.
pub const DIFF_VS_BARBARIAN_PCT: [i32; 8] = [800, 400, 200, 100, 50, 25, 0, 0];

/// One `EXPR` (experience level) row: `[0x9C40CC]`, stride 44.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Experience {
    /// Base hit points (memory `+0x24`), read by `0x5BE5B0` at `0x5BE6A3`.
    pub base_hp: i32,
    /// Retreat percentage (memory `+0x28`), read by both retreat blocks.
    pub retreat_pct: i32,
}

/// Conscript, Regular, Veteran, Elite (`conquests.biq`).
pub const EXPERIENCE: [Experience; 4] = [
    Experience { base_hp: 2, retreat_pct: 34 },
    Experience { base_hp: 3, retreat_pct: 50 },
    Experience { base_hp: 4, retreat_pct: 58 },
    Experience { base_hp: 5, retreat_pct: 66 },
];

/// `TERR` defense percentage per terrain (memory `+0x58`, body `+0x54`),
/// `conquests.biq` order: Desert, Plains, Grassland, Tundra, Flood Plain,
/// Hills, Mountains, Forest, Jungle, Marsh, Volcano, Coast, Sea, Ocean.
/// Matches the Civilopedia "Defender Combat Bonus" table row for row.
pub const TERRAIN_DEFENSE_PCT: [i32; 14] = [10, 10, 10, 10, 10, 50, 100, 25, 25, 20, 80, 10, 10, 10];

// ---------------------------------------------------------------------------
// Direction and river edge (0x5E6DA0, 0x56CD80)
// ---------------------------------------------------------------------------

/// Direction index of a coordinate delta, `0x5E6DA0(dx, dy)`.
///
/// `N=0 NE=1 E=2 SE=3 S=4 SW=5 W=6 NW=7`, y growing downwards. The routine
/// returns 8 for north (`dy < 0`, mostly vertical); its caller folds 8 to 0
/// (`0x56CE63..0x56CE68`), which this function does as well.
///
/// Bands: mostly horizontal when `|dx| > |dy|` and `2|dx| > 3|dy|`; mostly
/// vertical when `|dx| <= |dy|` and `2|dy| > 3|dx|`; otherwise diagonal.
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

/// Whether the defender's tile has a river edge facing the attacker:
/// `(cell.vfunc_0x94() >> dir) & 1` where `dir` is the direction from the
/// defender toward the attacker (`0x56CE8C..0x56CE9B`). `mask` is
/// `byte[cell+4]`; the bit layout is by direction index, not a 4-bit nibble.
pub fn river_edge(def_mask: u8, dir_def_to_att: u32) -> bool {
    dir_def_to_att < 8 && def_mask & (1u8 << dir_def_to_att) != 0
}

/// Terrain plus river term, `0x56CD80`. `river_crossed` is false for bombard
/// (`ax = ay = -1`) and for out-of-range attacker coordinates (`0x56CDBC..0x56CDE2`).
pub fn terrain_term(terrain_pct: i32, river_crossed: bool, rules: &Rules) -> i32 {
    terrain_pct + if river_crossed { rules.river_pct } else { 0 }
}

// ---------------------------------------------------------------------------
// Tile structure term (0x56CEB0) and city buildings (0x4C10B0)
// ---------------------------------------------------------------------------

/// What stands on the defender's tile, in the precedence `0x56CEB0` uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Structure {
    /// Nothing.
    None,
    /// Overlay bit 4 (cell vtable slot `0x34`): `rules.fort_pct`.
    Fortress,
    /// Overlay bit 28 (slot `0x38`): `2 * rules.fort_pct`.
    Barricade,
    /// A city (`word[cell+0x1A]` resolves in the city pool `0xA52E68`).
    City {
        /// Citizens (`city+0x138`).
        size: i32,
        /// Resisting citizens, `0x4BB2A0(city, -1)`.
        resisters: i32,
        /// Building bonus, [`city_building_bonus`].
        building_pct: i32,
    },
}

impl Structure {
    /// Fortress wins over barricade when both overlay bits are set
    /// (slot `0x34` is tested first, `0x56CF90..0x56CFA0`).
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

/// Defense percentage of the tile contents plus the radar bonus, `0x56CEB0`.
///
/// `radar` is `civ != -1 && 0 <= civ < 32 && (cell+0xD4 >> civ) & 1`. A city
/// with resisters contributes nothing ("Cities with resisters do not give
/// defensive bonuses", Civilopedia). A city tile never consults the overlay.
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
    /// `BLDG` memory `+0xA4` (body `+0xA0`): Walls 50, Civil Defense 50.
    pub pct: i32,
    /// `BLDG` memory `+0x9C` (body `+0x98`): nonzero drops the building for
    /// cities above town size (Walls: 8), and `> 0` makes it eligible for the
    /// doubling below.
    pub town_limited: i32,
    /// `BLDG +0xE0 >= 0` and the owner knows that tech (`0x4C10EE..0x4C1120`).
    pub obsolete: bool,
}

/// `0x4C10B0`: the **maximum** (not the sum) over the city's buildings.
///
/// `doubler` is `0x55A8D0(owner, 0x1000, 0) != 0` (a wonder with ability bit
/// `0x1000`; none exists in `conquests.biq`, so it is false there).
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

// ---------------------------------------------------------------------------
// Fortify, barbarians, amphibious
// ---------------------------------------------------------------------------

/// Fortify bonus, `0x4A0F79..0x4A0FC9` and `0x42B5B0`. All four conditions
/// are required: land kind (`PRTO +0x9C == 0`), tile not water (`vfunc 0x8C`),
/// order fortified (`[unit+0x64] == 1`) and movement left
/// (`maxMove - [unit+0x50] > 0`, `0x467CE0`).
///
/// A unit with no container is tested itself. A unit **inside an army** (or
/// any container) takes the bonus of its **outermost container** instead:
/// `0x4A0F67` calls `0x5BCA90` (container-of) and, when it is non-null,
/// `0x42B5B0`, which loops `0x5BCA90` to the top and applies these same four
/// tests to that top unit. The caller therefore passes the facts of the
/// outermost container as the arguments.
pub fn fortify_term(
    land_kind: bool,
    on_water: bool,
    fortified: bool,
    move_left: i32,
    rules: &Rules,
) -> i32 {
    if land_kind && !on_water && fortified && move_left > 0 {
        rules.fortify_pct
    } else {
        0
    }
}

/// Barbarian term for the **non-barbarian** side: its difficulty percentage
/// plus 100 when its civ owns the barbarian-bonus wonder.
///
/// Added to `D` when the attacker is a barbarian (`0x4A0FCF..0x4A1025`, the
/// defender's civ) and to `P` when the defender is a barbarian
/// (`0x4A1063..0x4A10C1`, the attacker's civ).
pub fn barbarian_term(diff_pct: i32, has_wonder: bool) -> i32 {
    diff_pct + if has_wonder { GREAT_WALL_VS_BARBARIANS_PCT } else { 0 }
}

/// Inputs of the amphibious clause `0x4A10C6..0x4A118C`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AmphibiousCheck {
    /// `0x5BC8B0(A, 6)`: PRTO ability bit 6 (Marine, Berserk).
    pub ability_amphibious: bool,
    /// `0x5BE6E0(A) > 0`.
    pub attack_strength: i32,
    /// `[A+0x48] & 4` (**HYPOTHESIS**: "already attacked this turn").
    pub status_bit2: bool,
    /// `0x5BC8B0(A, 2)`: ability bit 2 (Blitz); only consulted with `status_bit2`.
    pub ability_blitz: bool,
    /// `PRTO[A].+0x9C == 0`.
    pub land_unit: bool,
    /// Defender's tile is water (`vfunc 0x8C`).
    pub target_is_water: bool,
    /// Attacker's tile is water.
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

// ---------------------------------------------------------------------------
// Strength, army and hit points
// ---------------------------------------------------------------------------

/// Rounded average over an army's carried units, `(sum + n/2) / n`
/// (`0x5BE7E8..0x5BE7F9`, `0x5BE928..0x5BE939`); `None` when the army carries
/// nobody and the getter falls back to the unit's own PRTO strength.
pub fn army_average(members: &[i32]) -> Option<i32> {
    if members.is_empty() {
        return None;
    }
    let n = members.len() as i32;
    let sum: i32 = members.iter().sum();
    Some((sum + n / 2) / n)
}

/// Army bonus `0x5BCAE0`: `trunc(sum * 0x3E2AAAAB)`, the float constant
/// `0.16666667` at `0x66FB50`. The sum is over the carried units' attack
/// strengths (attacker side) or defense strengths (defender side). Zero for
/// an empty army.
pub fn army_bonus(members: &[i32]) -> i32 {
    if members.is_empty() {
        return 0;
    }
    let sum: f32 = members.iter().fold(0.0f32, |acc, &m| acc + m as f32);
    (f64::from(sum) * f64::from(f32::from_bits(0x3E2A_AAAB))) as i32
}

/// Attack strength, `0x5BE6E0`: army average, else the PRTO attack (`+0x60`).
pub fn attack_strength(army_members: &[i32], prto_attack: i32) -> i32 {
    army_average(army_members).unwrap_or(prto_attack)
}

/// Defense strength, `0x5BE820`: army average (returned as is), else the PRTO
/// defense (`+0x58`) halved by `sar 1` when `unit+0x1ED` is set and the value
/// is above 1 (`0x5BE953..0x5BE962`).
pub fn defense_strength(army_members: &[i32], prto_defense: i32, bombarded: bool) -> i32 {
    army_average(army_members).unwrap_or(if bombarded && prto_defense > 1 {
        prto_defense >> 1
    } else {
        prto_defense
    })
}

/// Maximum hit points, `0x5BE5B0`: `max(1, base + hp_bonus)` where `base` is
/// the sum of the carried units' maximum HP for an army with members and
/// `EXPR[level].base_hp` otherwise; `hp_bonus` is `PRTO +0xA4`.
pub fn max_hp(member_hp_sum: Option<i32>, level_base_hp: i32, hp_bonus: i32) -> i32 {
    (member_hp_sum.unwrap_or(level_base_hp) + hp_bonus).max(1)
}

// ---------------------------------------------------------------------------
// Odds (0x4A0ED0)
// ---------------------------------------------------------------------------

/// Inputs of the odds formula after all terms are summed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OddsInput {
    /// Attacker strength (`0x5BE6E0`, or PRTO `+0x48` for bombard).
    pub att_strength: i32,
    /// `0x5BCAE0(A, 0)`.
    pub att_army_bonus: i32,
    /// Attacker percentage `P`: radar + barbarian + amphibious.
    pub att_pct: i32,
    /// Defender strength (`0x5BE820`).
    pub def_strength: i32,
    /// `0x5BCAE0(B, 1)`.
    pub def_army_bonus: i32,
    /// Defender percentage `D`: terrain, river, tile, fortify, barbarian.
    pub def_pct: i32,
}

/// `1024 * P(defender wins a round)` clamped to `1..=1023`, `0x4A11AE..0x4A11DF`.
///
/// `X = (def + bonus) * (100 + D)`, `Y = (att + bonus) * (100 + P)`,
/// `1024 * X / (X + Y)` with x86 `imul`/`shl`/`idiv` semantics. Returns `None`
/// when `X + Y == 0`, where the original traps with a divide error.
///
/// This takes the *summed* terms, so it serves every mode of `0x4A0ED0`. The
/// mode only decides which terms the caller sums: `flag4 != 0` (the "raw"
/// mode, `0x4A0F19 jne 0x4A0FCF`) skips terrain, river, tile structure,
/// defender radar and fortify, but keeps army bonuses, both barbarian terms,
/// the attacker's radar and the amphibious clause; `bombard != 0` swaps the
/// attacker strength for `PRTO +0x48` and calls the terrain terms with no
/// attacker position and no civ. See `combat.md` section 3.4.
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

// ---------------------------------------------------------------------------
// Rounds, retreat, bombard
// ---------------------------------------------------------------------------

/// Retreat eligibility flags, `[this+9]` (attacker) and `[this+8]` (defender)
/// of the step object, computed at `0x4A48CA..0x4A493F`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetreatFlags {
    /// `[this+9]`.
    pub attacker: bool,
    /// `[this+8]`.
    pub defender: bool,
}

impl RetreatFlags {
    /// A side may retreat only when its maximum movement exceeds one full
    /// move (`0x5BE470 > [0x9C72C8]`); if **both** sides are fast neither may,
    /// and the defender may not retreat from a tile with a city (`0x5EA6C0`).
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
    /// Maximum hit points ([`max_hp`]).
    pub max_hp: i32,
    /// Damage taken, `unit+0x4C`.
    pub damage: i32,
    /// `EXPR[level].retreat_pct`.
    pub retreat_pct: i32,
    /// `owner != 0`; barbarians never retreat (`0x4A648C`, `0x4A6F95`).
    pub owned: bool,
}

impl Fighter {
    /// `maxHP - damage` (`0x5BE5B0` minus `[unit+0x4C]`).
    pub fn remaining(&self) -> i32 {
        self.max_hp - self.damage
    }

    /// `damage = max(0, damage + 1)` (`0x4A5BAC..0x4A5BB5`).
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

/// Plays one duel at fixed `odds` ([`defender_round_odds`]).
///
/// Order of RNG draws, per round: the die `next(1024)`; then, after a round
/// that leaves the loser at exactly 1 HP and the winner above 1 HP, the
/// loser's retreat die `next(winner.retreat_pct + 50)` when that side may
/// retreat and is not a barbarian. A retreat succeeds when the die is below
/// the loser's own `retreat_pct`. For the defender a success also needs a
/// free tile (`0x5BFB60`, the `defender_can_step_back` input); when none
/// exists the duel simply continues.
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
            // Attacker wins the round (0x4A5B56 setge).
            def.take_hit();
            if def.remaining() <= 0 {
                return Outcome::AttackerWon;
            }
            // Defender retreat block 0x4A647B.
            if flags.defender && def.owned && def.remaining() == 1 && att.remaining() > 1 {
                let die = (att.retreat_pct + RETREAT_MARGIN) as u32;
                if rng.below(die) < def.retreat_pct && defender_can_step_back {
                    return Outcome::DefenderRetreated;
                }
            }
        } else {
            // Defender wins the round (0x4A66B9 -> 0x4A66F9).
            att.take_hit();
            if att.remaining() <= 0 {
                return Outcome::DefenderWon;
            }
            // Attacker retreat block 0x4A6F84.
            if flags.attacker && att.owned && att.remaining() == 1 && def.remaining() > 1 {
                let die = (def.retreat_pct + RETREAT_MARGIN) as u32;
                if rng.below(die) < att.retreat_pct {
                    return Outcome::AttackerRetreated;
                }
            }
        }
    }
}

/// The unit pass of the **legacy** ranged attack `0x4A3320`, `0x4A3621..0x4A3661`.
///
/// `0x4A3320` is what `Unit::attackAt` (`0x5C1464`) calls for a unit whose
/// special-action word has bit 17 (`0x10020000`, the "charm" bit). **No unit
/// in `conquests.biq` has it**, so this is dormant; the bombard every shipped
/// unit uses is [`ranged_volley`] (`0x4A3A70`), which does write damage.
///
/// Draws exactly `rate_of_fire` dice (`PRTO +0x6C`) at the bombard odds
/// (`0x4A0ED0(A, target, 1, 0)`); the loop never exits early. Returns whether
/// any die was `>= odds`, which sets `unit+0x1ED` and so halves that unit's
/// defense in [`defense_strength`] until the flag is cleared. This routine
/// writes no damage.
pub fn bombard_hit(rng: &mut Rng, odds: i32, rate_of_fire: i32) -> bool {
    let mut hit = false;
    for _ in 0..rate_of_fire.max(0) {
        if rng.below(ROUND_DIE) >= odds {
            hit = true;
        }
    }
    hit
}

// ---------------------------------------------------------------------------
// City and tile strikes (0x4A2650, 0x4A2550)
// ---------------------------------------------------------------------------

/// Which part of a city a ranged strike attacks: the `mode` argument of
/// `0x4A2650`. Every other value makes that function return false without
/// drawing a die (`0x4A2684..0x4A2693`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrikeMode {
    /// Mode 0: kill one citizen (`POPDESTROYED`). Needs city size above 1
    /// (`cmp [city+0x138],1; jle` at `0x4A28AD`). Base `[0x9C7298]`.
    Population = 0,
    /// Mode 1: destroy one random non-wonder building (`0x4C1590`,
    /// `FACDESTROYED`). Base `[0x9C7294]`.
    Improvement = 1,
}

impl StrikeMode {
    /// The city's implicit base strength for this mode ([`Rules::city_strike_base`]).
    pub fn base(self, rules: &Rules) -> i32 {
        rules.city_strike_base[self as usize]
    }
}

/// The implicit strength a strike target holds against a bombard shot:
/// `v = ((terrain + tile + 100) * base) / 100` (the `0x51EB851F` multiply is
/// the signed divide by 100). Shared by the city and tile strikes
/// (`0x4A26A8..0x4A2723`, `0x4A25BC..0x4A2611`) and by the wall rolls of the
/// ranged attack (`0x4A3BB5..0x4A3C08`, `0x4A3CDC..0x4A3D29`).
pub fn implicit_strength(base: i32, terrain_pct: i32, tile_pct: i32) -> i32 {
    terrain_pct.wrapping_add(tile_pct).wrapping_add(100).wrapping_mul(base) / 100
}

/// The odds the target holds in a strike roll: `0x4A2650` (`0x4A26A8..0x4A2723`)
/// for a city and the unit-less tile strike `0x4A2550` (`0x4A25BC..0x4A2611`).
///
/// The target's implicit strength ([`implicit_strength`]) is set against the
/// attacker's bombard strength (`PRTO +0x48`):
/// `odds = 1024 * v / (v + strength)`, clamped like [`defender_round_odds`].
/// `base` is [`StrikeMode::base`] for a city and [`TILE_DEFENSE_STRENGTH`] for
/// a tile with no city. `None` when the denominator is zero, where the
/// original traps.
pub fn strike_odds(
    base: i32,
    terrain_pct: i32,
    tile_pct: i32,
    strike_strength: i32,
) -> Option<i32> {
    let v = implicit_strength(base, terrain_pct, tile_pct);
    let denom = v.wrapping_add(strike_strength);
    if denom == 0 {
        return None;
    }
    let odds = (i64::from(v.wrapping_shl(10)) / i64::from(denom)) as i32;
    Some(odds.clamp(ODDS_MIN, ODDS_MAX))
}

/// Number of dice a city strike may throw: `max(PRTO +0x6C, requested)`
/// (`cmp eax,ecx; jg` at `0x4A2671..0x4A2679`). The tile strike and the
/// post-combat hook ask for 1.
pub fn strike_rolls(rate_of_fire: i32, requested: i32) -> i32 {
    rate_of_fire.max(requested)
}

/// Throws up to `rolls` dice and **stops at the first success**, a die at or
/// above the odds (`jge 0x4A2757` at `0x4A2747`). No dice are drawn for
/// `rolls <= 0`, and then the strike fails. Compare [`bombard_hit`], which
/// never exits early.
pub fn strike_hit(rng: &mut Rng, odds: i32, rolls: i32) -> bool {
    for _ in 0..rolls.max(0) {
        if rng.below(ROUND_DIE) >= odds {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Defensive bombard (0x4A1AE0, 0x4A3280) and interception (0x4A4520)
// ---------------------------------------------------------------------------

/// `PRTO +0x9C`: the unit's domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Domain {
    /// `0`: foot and wheeled units.
    Land = 0,
    /// `1`: ships.
    Sea = 1,
    /// `2`: aircraft and missiles.
    Air = 2,
}

/// Can `victim` be the target of a defensive bombard? `0x4A1AE0..0x4A1B6D`.
///
/// It needs defense above zero (`0x5BE820`) and, for land and sea units, at
/// least 2 HP left, so a bombard hit never kills. Air units have no floor.
pub fn defensive_bombard_victim_ok(domain: Domain, defense: i32, remaining_hp: i32) -> bool {
    defense > 0 && (domain == Domain::Air || remaining_hp > 1)
}

/// One unit standing on the defender's tile, as the shooter scan
/// `0x4A1BC6..0x4A1C7B` sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShooterCandidate {
    /// The candidate is the defender itself (`cmp esi,ebx` at `0x4A1C03`).
    pub is_defender: bool,
    /// The candidate is loaded in the defender (`0x5BCA90(esi) == defender`).
    pub carried_by_defender: bool,
    /// `PRTO +0x9C`; must equal the victim's.
    pub domain: Domain,
    /// `PRTO +0x48`; must be positive.
    pub bombard_strength: i32,
    /// `Unit::hasAbility(3)` (`0x4A1C4B`): such units never shoot.
    pub has_ability_3: bool,
    /// `unit +0x48 & 0x40`: already fired this turn (`0x4A1C58`).
    pub already_fired: bool,
}

/// Index of the shooter among the units on the defender's tile, in the
/// order the tile lists them: the **strictly** highest bombard strength wins,
/// so the first of equals stays (`cmp eax,ebp; jle` at `0x4A1C75`).
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
    /// The hit left the victim at exactly 1 HP (`0x4A32E0`). The shooter's
    /// civ then books an incident against the victim's civ, a weight-1 call
    /// to `Player::0x5631B0` (`combat.md` section 8.9).
    pub left_at_one_hp: bool,
}

/// `0x4A3280`: exactly one die against `odds = 0x4A0ED0(shooter, victim, 1, 1)`.
/// A hit adds one point of damage; a miss changes nothing. The caller sets
/// the shooter's status bit `0x40` either way (`0x4A3309`), which keeps it
/// from shooting again until the per-turn reset.
pub fn defensive_bombard(rng: &mut Rng, odds: i32, victim: &mut Fighter) -> DefensiveBombard {
    let hit = rng.below(ROUND_DIE) >= odds;
    if hit {
        victim.take_hit();
    }
    DefensiveBombard { hit, left_at_one_hp: hit && victim.remaining() == 1 }
}

/// `0x4A4520` returns at once when the interceptor has no attack and the
/// aircraft no defense (`0x4A4531..0x4A4547`); otherwise the duel runs.
pub fn interception_runs(interceptor_attack: i32, aircraft_defense: i32) -> bool {
    interceptor_attack != 0 || aircraft_defense != 0
}

/// The interception duel `0x4A4520..0x4A45D2`: `odds = 0x4A0ED0(interceptor,
/// aircraft, 0, 1)` computed once, the ordinary round die and damage rule,
/// and **no retreat**. `AttackerWon` means the interceptor shot the aircraft
/// down.
pub fn interception_duel(
    rng: &mut Rng,
    odds: i32,
    interceptor: &mut Fighter,
    aircraft: &mut Fighter,
) -> Outcome {
    let no_retreat = RetreatFlags { attacker: false, defender: false };
    duel(rng, odds, interceptor, aircraft, no_retreat, false)
}

// ---------------------------------------------------------------------------
// Choosing the defender (0x4A47C0 -> 0x4A1590 / 0x4A1740 / 0x4A1910)
// ---------------------------------------------------------------------------

/// Verdict of the legality filter `0x4A1590`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Eligibility {
    /// The filter returns true.
    Eligible,
    /// The filter returns false and does nothing else.
    Ineligible,
    /// A unit with defense but no HP left. The filter queues a kill event
    /// (`0x474140`) and reports it as not eligible.
    Zombie,
}

/// What `0x4A1590` looks at for one candidate on tile `(x, y)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DefenderFacts {
    /// `0x5BCA90(unit) != 0`: loaded units never defend on their own.
    pub carried: bool,
    /// `PRTO +0x9C`.
    pub domain: Domain,
    /// The tile's water predicate (`vfunc 0x8C`).
    pub tile_is_water: bool,
    /// `0x5BE820`.
    pub defense: i32,
    /// `0x5BE5B0 - [unit+0x4C]`.
    pub remaining_hp: i32,
}

/// `0x4A1590..0x4A1739`.
///
/// Sea units need a water tile, land units a land tile, and **air units are
/// never eligible** (`0x4A1664`); they are only fought through interception.
/// A unit with zero defense is eligible whatever its HP; otherwise it needs
/// HP left, and a unit at or below zero is a [`Eligibility::Zombie`].
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

/// The rating both selection loops use, `0x4A1823..0x4A186B`:
/// `(100 + fortify) * defense * clamp(remaining, 0, 9999) / 100`, where
/// `fortify` is [`Rules::fortify_pct`] when `0x42B5B0` grants it.
pub fn defender_rating(fortify_pct: i32, defense: i32, remaining_hp: i32) -> i32 {
    (fortify_pct + 100).wrapping_mul(defense).wrapping_mul(remaining_hp.clamp(0, 9999)) / 100
}

/// Starting "best rating" of the strongest-defender loop `0x4A1740`.
pub const BEST_DEFENDER_START: i32 = 0;
/// Starting "best rating" of the weakest-defender loop `0x4A1910`
/// (`mov [esp+0x14],0x3E8` at `0x4A1922`).
pub const WEAKEST_DEFENDER_START: i32 = 1000;

/// One eligible candidate as the comparators `0x4A12D0` / `0x4A1430` see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DefenderRank {
    /// [`defender_rating`].
    pub rating: i32,
    /// `Unit::hasAbility(29)`, the King.
    pub is_king: bool,
    /// Loaded units: `0` for an Army (ability 18), else `0x5BE9E0`.
    pub cargo: i32,
    /// `0x5BE6E0`.
    pub attack: i32,
    /// `PRTO +0x48`.
    pub bombard: i32,
    /// `0x5BE5B0`.
    pub max_hp: i32,
}

/// Shared body of `0x4A12D0` (`strongest`) and its mirror `0x4A1430`.
fn replaces_best(
    c: &DefenderRank,
    best: Option<&DefenderRank>,
    best_rating: i32,
    ties_need_positive: bool,
    strongest: bool,
) -> bool {
    // King clause (`0x4A12E0..0x4A1321`): only against a real incumbent and
    // for a candidate that has a positive rating. A King is the worst
    // defender to pick (the last resort) and the best to pick when the
    // attacker wants the weakest.
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
    // Equal zero ratings: only a King candidate takes the slot (`0x4A1339`).
    if c.rating == best_rating && c.rating == 0 && c.is_king {
        return true;
    }
    // `setg` / `setge` on the incumbent's rating, then an exact tie.
    let tie_ok = if ties_need_positive { best_rating > 0 } else { best_rating >= 0 };
    if !tie_ok || c.rating != best_rating {
        return false;
    }
    let Some(b) = best else {
        return true;
    };
    // Tie-breaks, in order. The strongest search keeps the smaller value of
    // each, the weakest search the larger one; equal on all four keeps the
    // incumbent.
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

/// `0x4A12D0`: does `c` replace `best` in the strongest-defender search?
/// The two callers that are understood, the defender choice `0x4A1740` and the
/// bombard target choice `0x4A1FA0` (`0x4A23DB`), both pass
/// `ties_need_positive = true`; the other callers (`0x44AD06`, `0x44EEAA`,
/// `0x4E416E..0x4E41B4`, `0x5BCE25`, `0x5BD13C`, `0x5BD170`) are unread.
pub fn is_better_defender(
    c: &DefenderRank,
    best: Option<&DefenderRank>,
    best_rating: i32,
    ties_need_positive: bool,
) -> bool {
    replaces_best(c, best, best_rating, ties_need_positive, true)
}

/// `0x4A1430`: the mirror comparator of the weakest-defender search.
pub fn is_weaker_defender(
    c: &DefenderRank,
    best: Option<&DefenderRank>,
    best_rating: i32,
    ties_need_positive: bool,
) -> bool {
    replaces_best(c, best, best_rating, ties_need_positive, false)
}

/// The strongest-defender loop `0x4A1740` over already-filtered candidates in
/// tile order. Returns the index of the chosen defender.
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

/// The weakest-defender loop `0x4A1910`. Zero-rated candidates are skipped
/// outright (`test edi,edi; je` at `0x4A1A75`).
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

// ---------------------------------------------------------------------------
// Ranged attacks (0x5C1410 -> 0x4A3A70; cruise missile 0x4A2B40)
// ---------------------------------------------------------------------------

/// `Unit::hasAbility(27)`, **Lethal Land Bombardment** (`0x4A40E8`): the
/// attacker may take land units to zero HP. The Civilopedia lists it for
/// Cruise Missile, Tactical Nuke... and every unit whose PRTO word `+0x88`
/// has bit 27.
pub const ABILITY_LETHAL_LAND_BOMBARD: u32 = 27;
/// `Unit::hasAbility(28)`, **Lethal Sea Bombardment** (`0x4A40DF`).
pub const ABILITY_LETHAL_SEA_BOMBARD: u32 = 28;

/// Size of the die that picks the city mode when a ranged attack finds no
/// unit to shoot at in a city: `push 2; call 0x60BAB0` (`0x4A3EE9`), then
/// `0x4A2650(attacker, city, die, 0)`. Modes 0 and 1 are both real
/// ([`StrikeMode`]), so this is an even pick between killing a citizen and
/// destroying a building.
pub const NO_TARGET_CITY_MODE_DIE: u32 = 2;

impl StrikeMode {
    /// The mode value `0x4A2650` accepts; anything else is not a strike.
    pub fn from_mode(mode: i32) -> Option<StrikeMode> {
        match mode {
            0 => Some(StrikeMode::Population),
            1 => Some(StrikeMode::Improvement),
            _ => None,
        }
    }
}

/// Which domains the attacker may reduce to zero HP, from its abilities 27
/// and 28. Everything else is held at 1 HP by the floor in
/// [`bombard_candidate_ok`] and [`ranged_volley`]. Aircraft have no floor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Lethality {
    /// `hasAbility(27)`.
    pub land: bool,
    /// `hasAbility(28)`.
    pub sea: bool,
}

impl Lethality {
    /// A unit with neither ability.
    pub const NONE: Lethality = Lethality { land: false, sea: false };
    /// What the Cruise Missile has (`conquests.biq`: PRTO word `+0x88` is
    /// `0x18000008`, bits 3, 27 and 28).
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

/// The classes of target a ranged attacker works through, in order, from the
/// list set-up at `0x4A1FA6..0x4A20A7` (the unit counter `0x4A36C0` builds
/// the same lists). The first class that holds a legal target supplies the
/// defender.
///
/// * the **AI cruise-missile flag** (`PRTO +0x8C` bit 5, `shr eax,5` at
///   `0x4A203C`) or a **sea** attacker: sea, air, land;
/// * an **air** attacker: air, sea, land;
/// * any other **land** attacker: sea units if the tile is water, land units
///   if not, and nothing else.
///
/// The shipped Cruise Missile has domain 0 (land) and the flag, so it takes
/// the first list.
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
    /// The cell's water predicate (vfunc `0x8C`).
    pub is_water: bool,
    /// `0x56D2C0(x, y) != 0`.
    pub has_city: bool,
}

/// One unit on the target tile, as the filter `0x4A2113..0x4A22F8` sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BombardCandidate {
    /// `0x5BCA90(unit) != 0`: loaded units are never picked.
    pub carried: bool,
    /// `0x5BE820`.
    pub defense: i32,
    /// `PRTO +0x9C`.
    pub domain: Domain,
    /// `0x5BE5B0 - [unit+0x4C]`.
    pub remaining_hp: i32,
    /// `0x5BB650(unit, attackerOwner, 1)`: hostile to the attacker.
    pub hostile: bool,
}

/// Is `c` a legal target of `class` (one entry of [`bombard_target_order`])?
/// `0x4A2113..0x4A22F8`, in the order of the tests:
///
/// 1. not carried, and defense above zero (a Worker is never shot at);
/// 2. the 1 HP floor: land and sea units with at most 1 HP left are skipped
///    unless the attacker is lethal against that domain ([`Lethality`]);
/// 3. hostile, and of the class being scanned;
/// 4. land and air units must stand on a **land** tile; a sea unit on a land
///    tile (a port) needs a **city** there; an aircraft needs a city too.
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

/// The target selection `0x4A1FA0`: for each class in `order`, rate the legal
/// candidates with the strongest-defender comparator (`0x4A12D0`, called with
/// `ties_need_positive = 1` at `0x4A23DB`) and stop at the first class that
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

/// `0x4A36C0`: how many units on the tile are legal targets, over **all**
/// classes of `order` (it does not stop at the first), never counting the
/// lethal abilities (no flag reaches it, so the 1 HP floor always applies).
pub fn count_bombard_targets(
    order: &[Domain],
    tile: &BombardTile,
    units: &[BombardCandidate],
) -> i32 {
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

/// The die an aircraft throws over a city to choose between hitting the city
/// and shooting its units: `mode = next(k)` with `k = 4`, `+1` above 4
/// targets and `+1` more above 8 (`0x4A3E75..0x4A3E92`, `n` from
/// [`count_bombard_targets`]). Only modes 0 and 1 are city strikes
/// ([`StrikeMode::from_mode`]), so the chance that the city is hit instead of
/// the garrison is `2 / k`: 50%, 40% or 33%. A strike that does not happen
/// falls through to the units.
pub fn air_city_mode_die(targets: i32) -> u32 {
    let mut k = if targets > 8 { 5 } else { 4 };
    if targets > 4 {
        k += 1;
    }
    k
}

/// A sea unit standing in a **city** is harder to hit by the ranged attack
/// `0x4A3A70` (not by the missile): `odds = ftol((odds + 1) * 0.5)`
/// (`0x4A3FF2..0x4A4006`; `fmul` by the `0x3F000000` at `0x6653B4`, then the
/// truncating `_ftol`). `odds` is the *defender's* chance to hold a round, so
/// the port roughly halves it.
pub fn port_odds(odds: i32) -> i32 {
    (odds + 1) / 2
}

/// How a [`ranged_volley`] ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VolleyEnd {
    /// Every shot of the rate of fire was thrown and the target survived.
    Exhausted,
    /// A hit brought the target to zero HP or below (`0x4A41F8`).
    Killed,
    /// A hit left the target at exactly 1 HP and the attacker is not lethal
    /// against its domain: the volley stops there (`0x4A418A`).
    Spared,
}

/// Result of [`ranged_volley`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Volley {
    /// Shots that scored a hit; each is one point of damage.
    pub hits: i32,
    /// Why it stopped.
    pub end: VolleyEnd,
    /// Some hit left the target at exactly 1 HP (`0x4A40B6`). The attacker's
    /// civ then books a weight-1 incident against the target's civ
    /// (`Player::0x5631B0`, `0x4A40DA`).
    pub left_at_one_hp: bool,
}

/// The shot loop of `0x4A3A70` (`0x4A4012..0x4A41F3`) against one chosen
/// target, at `odds` from `0x4A0ED0(attacker, target, 1, 0)` (bombard mode),
/// already passed through [`port_odds`] when it applies.
///
/// Each of up to `rate_of_fire` (`PRTO +0x6C`) shots draws `next(1024)`: a die
/// below `odds` is a miss, otherwise the target takes one damage
/// (`max(0, damage + 1)`). After a hit: **remaining HP exactly 1** ends the
/// volley when [`Lethality::floor_applies`], and **remaining HP of zero or
/// less** kills. Shots are never skipped for the sake of an earlier hit, so a
/// volley draws `rate_of_fire` dice unless it ends early.
///
/// The cruise missile `0x4A2B40` runs the same loop with the floor removed
/// ([`Lethality::BOTH`]) and without [`port_odds`].
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
    /// `0x4ACB50(city, i, 0)`: the city itself holds the building (and the
    /// owner's government meets `BLDG +0xD4`).
    pub in_city: bool,
    /// `0x4ACB50(city, i, 1)`: as above, or the building acts on the city
    /// through a player-wide or continent-wide list.
    pub acts_on_city: bool,
    /// `BLDG +0xE0 >= 0` and the owner knows that tech (`Player::0x561440`).
    pub obsolete: bool,
    /// `BLDG +0x9C` (body `+0x98`): land bombardment defense. Walls: 8.
    pub land_defense: i32,
    /// `BLDG +0xA0` (body `+0x9C`): naval bombardment defense. Coastal
    /// Fortress: 8.
    pub sea_defense: i32,
}

/// `0x4C0C70`: the land bombardment defense of a city. Zero above town size
/// (`pop > [0x9C72E8]` or `pop > [0x9C72E4]`); otherwise the **largest**
/// `land_defense` over the buildings that act on the city and are not
/// obsolete, times `wonder_count + 1`, where `wonder_count` is
/// `Player::0x55A8D0(owner, 0x1000, 0)`, the owner's wonders with flag
/// `0x1000` (none in `conquests.biq`).
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

/// `0x4C1000`: the naval bombardment defense. The largest `sea_defense` over
/// the buildings that act on the city and are not obsolete, at **any** city
/// size, with no wonder multiplier.
pub fn sea_bombard_defense(facilities: &[FacilityFacts]) -> i32 {
    facilities
        .iter()
        .filter(|b| b.acts_on_city && !b.obsolete)
        .map(|b| b.sea_defense)
        .fold(0, i32::max)
}

/// What `0x4C1320` / `0x4C1470` do to the city after a successful wall roll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FacilityHit {
    /// The building was removed (`0x4ACF40(city, i, 0, 0)`); the message is
    /// `FACDESTROYED`.
    Destroyed(usize),
    /// Only a building that acts through a player-wide or continent-wide
    /// list qualified: nothing is removed and the out flag is set, so the
    /// attack goes on to the units (`0x4A3CC2`).
    Reported(usize),
    /// Nothing qualified; the function returns -1.
    Nothing,
}

/// `0x4C1320`, the land variant. Above town size it finds nothing. Pass 0
/// looks only at buildings the city holds and picks the strictly largest
/// `land_defense` (first of equals); the running best starts at **-1**, so
/// even a zero-defense building could be taken, which the caller prevents by
/// requiring [`land_bombard_defense`] above zero first. Only if pass 0 finds
/// no building at all does pass 1 repeat the scan over buildings that act on
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
            return if pass == 0 { FacilityHit::Destroyed(i) } else { FacilityHit::Reported(i) };
        }
    }
    FacilityHit::Nothing
}

/// `0x4C1470`, the sea variant: no size limit, the running best starts at
/// **0** (so only a positive `sea_defense` qualifies), and both of its passes
/// use the same "in the city" test, so it never reports.
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

/// What a land or sea ranged attacker throws against a city: the fields of
/// the wall roll `0x4A3BB5..0x4A3CAA` (land) and `0x4A3CDC..0x4A3DD0` (sea).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WallsAttack {
    /// The attacker's domain; aircraft have no wall roll.
    pub domain: Domain,
    /// `PRTO +0x48`.
    pub bombard: i32,
    /// `PRTO +0x6C`, the number of dice.
    pub rate_of_fire: i32,
    /// `0x56CD80(-1, -1, x, y)`.
    pub terrain_pct: i32,
    /// `0x56CEB0(x, y, -1)`.
    pub tile_pct: i32,
}

/// The city being attacked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CityWalls<'a> {
    /// `[city+0x138]`.
    pub pop: i32,
    /// One entry per BLDG row, in table order.
    pub facilities: &'a [FacilityFacts],
    /// See [`land_bombard_defense`].
    pub wonder_count: i32,
}

/// Outcome of the wall roll of a ranged attack on a city.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WallsStep {
    /// No wall defense applies (none built, town too large, or an aircraft):
    /// the units are attacked at once (`jle 0x4A3EB0`).
    Skipped,
    /// Every die missed. The whole attack ends here: the units in the city
    /// are not touched (`0x4A3CAF`).
    Missed,
    /// A die succeeded; see [`FacilityHit`].
    Hit(FacilityHit),
}

impl WallsStep {
    /// Does the unit pass `0x4A3EB0` still run after this step?
    pub fn attacks_units(self) -> bool {
        matches!(self, WallsStep::Skipped | WallsStep::Hit(FacilityHit::Reported(_)))
    }
}

/// The wall roll `0x4A3BB5..0x4A3DD0`. The walls' implicit strength is
/// `v = ((terrain + tile + 100) * defense) / 100` ([`implicit_strength`]);
/// `v <= 0` skips the roll. Otherwise `odds = clamp((v << 10) / (bombard + v),
/// 1, 1023)` ([`strike_odds`]) and up to `rate_of_fire` dice are thrown,
/// **stopping at the first die at or above the odds** ([`strike_hit`]); that
/// success then removes one building ([`land_facility_hit`] or
/// [`sea_facility_hit`]). A missed roll ends the attack with the units
/// untouched; a roll that is skipped lets the units be shot at.
pub fn walls_step(
    rng: &mut Rng,
    attack: &WallsAttack,
    city: &CityWalls<'_>,
    rules: &Rules,
) -> WallsStep {
    let defense = match attack.domain {
        Domain::Land => {
            land_bombard_defense(city.pop, city.facilities, city.wonder_count, rules)
        }
        Domain::Sea => sea_bombard_defense(city.facilities),
        Domain::Air => return WallsStep::Skipped,
    };
    if implicit_strength(defense, attack.terrain_pct, attack.tile_pct) <= 0 {
        return WallsStep::Skipped;
    }
    // A zero denominator cannot arise with a positive `v` and a bombard
    // strength of zero or more; where it would, the original traps.
    let Some(odds) = strike_odds(defense, attack.terrain_pct, attack.tile_pct, attack.bombard)
    else {
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

// ---------------------------------------------------------------------------
// After a victory (0x5BEF00): promotion, golden age, great leader, enslave
// ---------------------------------------------------------------------------

/// Base promotion die by the winner's current level 0, 1, 2 (`0x5BF01B..0x5BF034`).
/// Level 3 and above never roll for promotion.
pub const PROMOTION_DIE: [u32; 3] = [2, 4, 8];

/// The promotion die: the base for the level, doubled when the loser is a
/// barbarian (`0x5BF03F`), then halved when the winner's civ has the
/// Militaristic trait (`RACE` trait 0, `0x5BF07C`). `None` for levels the
/// roll does not apply to.
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

/// `0x5BF08B..0x5BF0A1`: a unit promotes when `next(die) == 0`. A unit whose
/// status bit 2 is set (it failed earlier this turn) skips the die and is
/// promoted without drawing. A failed roll must set that bit (`0x5BF0D1`).
pub fn promotion_roll(rng: &mut Rng, die: u32, failed_earlier_this_turn: bool) -> bool {
    failed_earlier_this_turn || rng.below(die) == 0
}

/// A level-3 (elite) winner rolls for a Great Leader instead.
pub const LEADER_LEVEL: i32 = 3;
/// Leader die without the Heroic Epic (`setle cl; lea ecx,[ecx*4+0xC]` at
/// `0x5BF581..0x5BF584`).
pub const LEADER_DIE: u32 = 16;
/// Leader die for a civ that owns the Heroic Epic (BLDG `+0xF4` bit 0).
pub const LEADER_DIE_WITH_EPIC: u32 = 12;

/// What the Great Leader branch `0x5BF4DF..0x5BF5A5` tests before it rolls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LeaderCheck {
    /// `unit +0x44`.
    pub winner_level: i32,
    /// `PRTO +0x9C == 0`.
    pub winner_is_land: bool,
    /// `loser +0x34 == 0`.
    pub loser_is_barbarian: bool,
    /// `unit +0x48 & 0x20`: this unit has already produced a leader.
    pub winner_already_made_leader: bool,
    /// The winner is carried by an Army (ability 18 on its container).
    pub winner_in_army: bool,
    /// The civ already has a unit of the Leader prototype (`[0x9C728C]`).
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

/// The leader die: 16, or 12 with the Heroic Epic, doubled when the
/// *defender* won (`flag == 0`, `0x5BF595`).
pub fn leader_die(has_heroic_epic: bool, attacker_won: bool) -> u32 {
    let base = if has_heroic_epic { LEADER_DIE_WITH_EPIC } else { LEADER_DIE };
    if attacker_won {
        base
    } else {
        base * 2
    }
}

/// `0x5BF598..0x5BF5A5`: a leader appears when `next(die) == 0`.
pub fn leader_roll(rng: &mut Rng, die: u32) -> bool {
    rng.below(die) == 0
}

/// Chance in percent that a unit with the Enslave action converts the loser:
/// `cmp ax,0x21; jae` at `0x5BFA15`.
pub const ENSLAVE_PERCENT: u32 = 33;

/// `0x5BFA05..0x5BFA15`: one `next(100)` per victory of an enslaving unit
/// whose prototype names a result unit.
pub fn enslave_roll(rng: &mut Rng) -> bool {
    (rng.below(100) as u32) < ENSLAVE_PERCENT
}

/// The Golden Age trigger `0x5BEF93..0x5BF005`: the winner (or the
/// prototype of the Army it belongs to) has ability 15, the loser is not a
/// barbarian, and the civ has no golden age scheduled (`Player +0x3C == -1`).
pub fn golden_age_triggers(
    unique_unit: bool,
    loser_is_barbarian: bool,
    golden_age_end_turn: i32,
) -> bool {
    unique_unit && !loser_is_barbarian && golden_age_end_turn == -1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(att: i32, att_pct: i32, def: i32, def_pct: i32) -> OddsInput {
        OddsInput {
            att_strength: att,
            att_army_bonus: 0,
            att_pct,
            def_strength: def,
            def_army_bonus: 0,
            def_pct,
        }
    }

    /// Finds a seed whose first die satisfies `want`.
    fn seed_with_first_roll(want: impl Fn(i32) -> bool) -> u32 {
        (0u32..).find(|&s| want(Rng::new(s).below(ROUND_DIE))).unwrap()
    }

    #[test]
    fn equal_strengths_are_even_odds() {
        // X = Y -> 1024 * X / 2X = 512.
        assert_eq!(defender_round_odds(&input(4, 0, 4, 0)), Some(512));
    }

    #[test]
    fn fortified_hill_defender_hand_calc() {
        // D = hills 50 + fortify 25 = 75: X = 4*175 = 700, Y = 4*100 = 400,
        // 1024*700/1100 = 651 (truncated).
        let d = terrain_term(TERRAIN_DEFENSE_PCT[5], false, &Rules::CONQUESTS)
            + fortify_term(true, false, true, 3, &Rules::CONQUESTS);
        assert_eq!(d, 75);
        assert_eq!(defender_round_odds(&input(4, 0, 4, d)), Some(651));
    }

    #[test]
    fn odds_are_clamped_to_1_and_1023() {
        // Defender strength 0 -> X = 0 -> raw 0 -> clamped up: a worker still
        // wins 1 round in 1024.
        assert_eq!(defender_round_odds(&input(4, 0, 0, 0)), Some(ODDS_MIN));
        // Attacker strength 0 -> Y = 0 -> raw exactly 1024 -> clamped down.
        // The upper clamp can only fire this way: for Y > 0 the raw value is
        // below 1024.
        assert_eq!(defender_round_odds(&input(0, 0, 5, 0)), Some(ODDS_MAX));
        // Short of the clamp the formula is plain truncating division.
        assert_eq!(defender_round_odds(&input(1, 0, 1000, 0)), Some(1022));
        assert_eq!(defender_round_odds(&input(1000, 0, 1, 0)), Some(1));
    }

    #[test]
    fn zero_total_is_the_original_divide_error() {
        assert_eq!(defender_round_odds(&input(0, 0, 0, 0)), None);
    }

    #[test]
    fn worked_example_from_combat_md() {
        // Attack 8 against defense 4, fortified (25) on hills (50) inside a
        // size-5 town (base 0) with Walls (50): D = 125.
        let r = Rules::CONQUESTS;
        let walls = BuildingDefense { pct: 50, town_limited: 8, obsolete: false };
        let city = Structure::City {
            size: 5,
            resisters: 0,
            building_pct: city_building_bonus(5, false, &[walls], &r),
        };
        let d = terrain_term(TERRAIN_DEFENSE_PCT[5], false, &r)
            + tile_term(city, false, &r)
            + fortify_term(true, false, true, 3, &r);
        assert_eq!(d, 125);
        // X = 4 * 225 = 900, Y = 8 * 100 = 800: 1024 * 900 / 1700 = 542.
        let odds = defender_round_odds(&input(8, 0, 4, d)).unwrap();
        assert_eq!(odds, 542);
        // The attacker wins a round on 482 of the 1024 equally likely dice.
        let wins = (0..ROUND_DIE as i32).filter(|&roll| roll >= odds).count();
        assert_eq!(wins, 482);
    }

    #[test]
    fn the_round_die_is_exactly_uniform_over_1024_values() {
        // 32768 / 1024 = 32: every die value has exactly 32 preimages among
        // the 15-bit outputs, so P(attacker wins a round) = (1024 - odds)/1024
        // with no rounding bias. (The retreat dice, n = retreat% + 50, are
        // *not* exact: 32768 is not a multiple of n.)
        let mut counts = [0u32; 1024];
        for k in 0..32768u32 {
            counts[((k * ROUND_DIE) >> 15) as usize] += 1;
        }
        assert!(counts.iter().all(|&c| c == 32));
    }

    #[test]
    fn radar_adds_25_to_attacker_and_defender() {
        let rules = Rules::CONQUESTS;
        assert_eq!(tile_term(Structure::None, true, &rules), 25);
        assert_eq!(tile_term(Structure::None, false, &rules), 0);
        // Attacker radar: Y = 4 * 125 = 500 vs X = 4 * 100 = 400.
        assert_eq!(defender_round_odds(&input(4, RADAR_PCT, 4, 0)), Some(455));
    }

    #[test]
    fn tile_term_matches_the_civilopedia_structure_table() {
        let r = Rules::CONQUESTS;
        let city = |size, resisters, building_pct| Structure::City { size, resisters, building_pct };
        assert_eq!(tile_term(Structure::Fortress, false, &r), 50);
        assert_eq!(tile_term(Structure::Barricade, false, &r), 100);
        assert_eq!(tile_term(city(6, 0, 0), false, &r), 0); // town
        assert_eq!(tile_term(city(7, 0, 0), false, &r), 50); // city
        assert_eq!(tile_term(city(12, 0, 0), false, &r), 50);
        assert_eq!(tile_term(city(13, 0, 0), false, &r), 100); // metropolis
        // Resisters switch every city bonus off, buildings included.
        assert_eq!(tile_term(city(13, 1, 50), false, &r), 0);
        // Radar stacks on top of whatever the tile gives.
        assert_eq!(tile_term(city(13, 0, 0), true, &r), 125);
    }

    #[test]
    fn fortress_beats_barricade_when_both_bits_are_set() {
        assert_eq!(Structure::from_overlay(true, true), Structure::Fortress);
        assert_eq!(Structure::from_overlay(false, true), Structure::Barricade);
        assert_eq!(Structure::from_overlay(false, false), Structure::None);
    }

    #[test]
    fn building_bonus_is_a_maximum_and_walls_are_town_only() {
        let r = Rules::CONQUESTS;
        let walls = BuildingDefense { pct: 50, town_limited: 8, obsolete: false };
        let civil = BuildingDefense { pct: 50, town_limited: 0, obsolete: false };
        // Town (size <= 6): walls count. Civil Defense counts at every size.
        assert_eq!(city_building_bonus(6, false, &[walls], &r), 50);
        assert_eq!(city_building_bonus(7, false, &[walls], &r), 0);
        assert_eq!(city_building_bonus(13, false, &[civil], &r), 50);
        // Both present: 50, not 100.
        assert_eq!(city_building_bonus(5, false, &[walls, civil], &r), 50);
        // The doubling wonder applies only to `town_limited > 0` buildings.
        assert_eq!(city_building_bonus(5, true, &[walls], &r), 100);
        assert_eq!(city_building_bonus(5, true, &[civil], &r), 50);
        // Obsolete buildings are skipped.
        let old = BuildingDefense { obsolete: true, ..walls };
        assert_eq!(city_building_bonus(5, false, &[old], &r), 0);
    }

    #[test]
    fn terrain_table_matches_the_civilopedia() {
        // Grassland/Plains/Desert/Floodplains/Tundra/Coast/Sea/Ocean 10, Marsh 20,
        // Forest/Jungle 25, Hills 50, Volcano 80, Mountains 100.
        let t = TERRAIN_DEFENSE_PCT;
        assert_eq!([t[0], t[1], t[2], t[3], t[4]], [10; 5]);
        assert_eq!([t[11], t[12], t[13]], [10; 3]);
        assert_eq!((t[9], t[7], t[8]), (20, 25, 25));
        assert_eq!((t[5], t[10], t[6]), (50, 80, 100));
    }

    #[test]
    fn river_adds_25_only_across_a_marked_edge() {
        let r = Rules::CONQUESTS;
        assert_eq!(terrain_term(10, true, &r), 35);
        assert_eq!(terrain_term(10, false, &r), 10);
        // Defender has a river on its north-east edge (dir 1); attacker is NE.
        assert!(river_edge(1 << 1, 1));
        assert!(!river_edge(1 << 1, 3));
        assert!(!river_edge(0xFF, 8));
    }

    #[test]
    fn direction_classifier_covers_the_eight_iso_neighbours() {
        // Iso neighbour deltas: N (0,-2) NE (1,-1) E (2,0) SE (1,1) S (0,2)
        // SW (-1,1) W (-2,0) NW (-1,-1).
        let deltas = [(0, -2), (1, -1), (2, 0), (1, 1), (0, 2), (-1, 1), (-2, 0), (-1, -1)];
        for (want, (dx, dy)) in deltas.iter().enumerate() {
            assert_eq!(dir_from_delta(*dx, *dy), want as u32, "({dx},{dy})");
        }
    }

    #[test]
    fn direction_band_edges_need_a_strict_three_halves_ratio() {
        // Horizontal band: ax > ay and 2*ax > 3*ay.
        assert_eq!(dir_from_delta(3, 2), 3); // 6 > 6 is false -> diagonal, east+south
        assert_eq!(dir_from_delta(4, 2), 2); // 8 > 6 -> east
        assert_eq!(dir_from_delta(-3, -2), 7); // diagonal, west+north
        assert_eq!(dir_from_delta(-4, -2), 6); // west
        // Vertical band: ax <= ay and 2*ay > 3*ax.
        assert_eq!(dir_from_delta(2, 3), 3); // 6 > 6 is false -> diagonal
        assert_eq!(dir_from_delta(2, 4), 4); // south
        assert_eq!(dir_from_delta(2, -4), 0); // north (the binary's 8, folded to 0)
        assert_eq!(dir_from_delta(-2, 3), 5); // diagonal, west+south
        // Opposite deltas give opposite directions (mod 8) away from the band edges.
        for (dx, dy) in [(1, 1), (2, 0), (0, 2), (1, -1), (5, 1), (1, 5)] {
            assert_eq!(
                (dir_from_delta(dx, dy) + 4) % 8,
                dir_from_delta(-dx, -dy),
                "({dx},{dy})"
            );
        }
    }

    #[test]
    fn army_average_rounds_half_up_with_truncating_division() {
        assert_eq!(army_average(&[]), None);
        assert_eq!(army_average(&[4, 4, 4]), Some(4));
        assert_eq!(army_average(&[4, 5]), Some(5)); // (9 + 1) / 2
        assert_eq!(army_average(&[1, 2, 2]), Some(2)); // (5 + 1) / 3
        assert_eq!(army_average(&[1, 1, 2]), Some(1)); // (4 + 1) / 3
    }

    #[test]
    fn army_bonus_is_a_sixth_of_the_member_sum() {
        assert_eq!(army_bonus(&[]), 0);
        // 0x3E2AAAAB is the float just above 1/6, so exact multiples of 6 do
        // not fall to the next lower integer.
        assert!(f64::from(f32::from_bits(0x3E2A_AAAB)) > 1.0 / 6.0);
        for sum in 0..=200 {
            assert_eq!(army_bonus(&[sum]), sum / 6, "sum {sum}");
        }
        // Three 4-attack members: sum 12 -> +2.
        assert_eq!(army_bonus(&[4, 4, 4]), 2);
    }

    #[test]
    fn army_strength_and_odds_combine() {
        // Three knights (attack 4) vs a lone defender of 6:
        // strength = avg 4, army bonus = 12/6 = 2 -> Y uses 6.
        let members = [4, 4, 4];
        let i = OddsInput {
            att_strength: attack_strength(&members, 0),
            att_army_bonus: army_bonus(&members),
            att_pct: 0,
            def_strength: 6,
            def_army_bonus: 0,
            def_pct: 0,
        };
        assert_eq!(defender_round_odds(&i), Some(512));
    }

    #[test]
    fn bombard_halves_plain_defense_but_not_army_average() {
        assert_eq!(defense_strength(&[], 6, false), 6);
        assert_eq!(defense_strength(&[], 6, true), 3);
        assert_eq!(defense_strength(&[], 1, true), 1); // value > 1 required
        assert_eq!(defense_strength(&[], 0, true), 0);
        assert_eq!(defense_strength(&[6, 6], 6, true), 6); // average path returns as is
    }

    #[test]
    fn max_hp_uses_level_or_member_sum_and_floors_at_one() {
        assert_eq!(max_hp(None, EXPERIENCE[0].base_hp, 0), 2);
        assert_eq!(max_hp(None, EXPERIENCE[3].base_hp, 1), 6);
        assert_eq!(max_hp(Some(9), 5, 1), 10); // members replace the level base
        assert_eq!(max_hp(None, 2, -5), 1);
    }

    #[test]
    fn experience_table_is_the_conquests_one() {
        let hp: Vec<i32> = EXPERIENCE.iter().map(|e| e.base_hp).collect();
        let rt: Vec<i32> = EXPERIENCE.iter().map(|e| e.retreat_pct).collect();
        assert_eq!(hp, [2, 3, 4, 5]);
        assert_eq!(rt, [34, 50, 58, 66]);
    }

    #[test]
    fn barbarian_terms_follow_difficulty() {
        assert_eq!(barbarian_term(DIFF_VS_BARBARIAN_PCT[0], false), 800); // Chieftain
        assert_eq!(barbarian_term(DIFF_VS_BARBARIAN_PCT[3], true), 200); // Monarch + wonder
        assert_eq!(barbarian_term(DIFF_VS_BARBARIAN_PCT[6], false), 0); // Deity
    }

    #[test]
    fn amphibious_requires_sea_to_land_by_a_land_unit() {
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
        assert_eq!(amphibious_term(&AmphibiousCheck { target_is_water: true, ..base }), 0);
        assert_eq!(amphibious_term(&AmphibiousCheck { land_unit: false, ..base }), 0);
        assert_eq!(amphibious_term(&AmphibiousCheck { ability_amphibious: false, ..base }), 0);
        assert_eq!(amphibious_term(&AmphibiousCheck { attack_strength: 0, ..base }), 0);
        // Status bit 2 requires the blitz ability.
        assert_eq!(amphibious_term(&AmphibiousCheck { status_bit2: true, ..base }), 0);
        assert_eq!(
            amphibious_term(&AmphibiousCheck { status_bit2: true, ability_blitz: true, ..base }),
            25
        );
    }

    #[test]
    fn fortify_needs_all_four_conditions() {
        let r = Rules::CONQUESTS;
        assert_eq!(fortify_term(true, false, true, 1, &r), 25);
        assert_eq!(fortify_term(false, false, true, 1, &r), 0); // sea/air kind
        assert_eq!(fortify_term(true, true, true, 1, &r), 0); // on water
        assert_eq!(fortify_term(true, false, false, 1, &r), 0); // not fortified
        assert_eq!(fortify_term(true, false, true, 0, &r), 0); // no movement left
    }

    #[test]
    fn retreat_flags_need_speed_and_cancel_when_both_are_fast() {
        let r = Rules::CONQUESTS;
        let f = |a, d, city| RetreatFlags::new(a, d, city, &r);
        // 1 move = 3 units: exactly 3 is not fast, 4+ is.
        assert_eq!(f(3, 3, false), RetreatFlags { attacker: false, defender: false });
        assert_eq!(f(6, 3, false), RetreatFlags { attacker: true, defender: false });
        assert_eq!(f(3, 6, false), RetreatFlags { attacker: false, defender: true });
        assert_eq!(f(6, 6, false), RetreatFlags { attacker: false, defender: false });
        // A defender never retreats out of a city.
        assert_eq!(f(3, 6, true), RetreatFlags { attacker: false, defender: false });
        assert_eq!(f(6, 3, true), RetreatFlags { attacker: true, defender: false });
    }

    fn fighter(hp: i32, retreat_pct: i32) -> Fighter {
        Fighter { max_hp: hp, damage: 0, retreat_pct, owned: true }
    }

    const NO_RETREAT: RetreatFlags = RetreatFlags { attacker: false, defender: false };

    #[test]
    fn one_hp_duel_is_decided_by_exactly_one_die() {
        let odds = 512;
        let win = seed_with_first_roll(|r| r >= odds);
        let mut rng = Rng::new(win);
        let mut reference = Rng::new(win);
        let (mut a, mut d) = (fighter(1, 50), fighter(1, 50));
        assert_eq!(duel(&mut rng, odds, &mut a, &mut d, NO_RETREAT, true), Outcome::AttackerWon);
        reference.below(ROUND_DIE);
        assert_eq!(rng, reference, "exactly one draw");
        assert_eq!((a.damage, d.damage), (0, 1));

        let lose = seed_with_first_roll(|r| r < odds);
        let mut rng = Rng::new(lose);
        let (mut a, mut d) = (fighter(1, 50), fighter(1, 50));
        assert_eq!(duel(&mut rng, odds, &mut a, &mut d, NO_RETREAT, true), Outcome::DefenderWon);
        assert_eq!((a.damage, d.damage), (1, 0));
    }

    #[test]
    fn extreme_odds_force_the_outcome() {
        // odds 1: the die is >= 1 on all but one value, the attacker takes the round.
        let mut wins = 0;
        for seed in 0..500 {
            let mut rng = Rng::new(seed);
            let (mut a, mut d) = (fighter(3, 50), fighter(3, 50));
            if duel(&mut rng, ODDS_MIN, &mut a, &mut d, NO_RETREAT, true) == Outcome::AttackerWon {
                wins += 1;
            }
        }
        assert!(wins >= 495, "{wins}");
    }

    #[test]
    fn defender_retreat_fires_at_one_hp_and_draws_one_extra_die() {
        // Defender 2 HP, attacker 3 HP, both Regular (retreat 50) but the
        // defender's own retreat value is 100 so the die (< 100) always passes.
        let odds = 512;
        let seed = seed_with_first_roll(|r| r >= odds);
        let mut rng = Rng::new(seed);
        let mut reference = Rng::new(seed);
        let mut a = fighter(3, 50);
        let mut d = fighter(2, 100);
        let flags = RetreatFlags { attacker: false, defender: true };
        assert_eq!(duel(&mut rng, odds, &mut a, &mut d, flags, true), Outcome::DefenderRetreated);
        reference.below(ROUND_DIE);
        reference.below(100); // retreat die: attacker.retreat_pct + 50
        assert_eq!(rng, reference, "round die then retreat die");
        assert_eq!(d.remaining(), 1);
    }

    #[test]
    fn failed_step_back_lets_the_duel_continue_to_a_death() {
        let odds = 512;
        let seed = seed_with_first_roll(|r| r >= odds);
        let mut rng = Rng::new(seed);
        let mut a = fighter(3, 50);
        let mut d = fighter(2, 100);
        let flags = RetreatFlags { attacker: false, defender: true };
        let out = duel(&mut rng, odds, &mut a, &mut d, flags, false);
        assert!(matches!(out, Outcome::AttackerWon | Outcome::DefenderWon), "{out:?}");
    }

    /// Independent oracle: the same duel with only round dice (no retreat).
    fn replay_dice_only(seed: u32, odds: i32, att_hp: i32, def_hp: i32) -> (Rng, Outcome) {
        let (mut att_hp, mut def_hp) = (att_hp, def_hp);
        let mut rng = Rng::new(seed);
        loop {
            if rng.below(ROUND_DIE) >= odds {
                def_hp -= 1;
                if def_hp <= 0 {
                    return (rng, Outcome::AttackerWon);
                }
            } else {
                att_hp -= 1;
                if att_hp <= 0 {
                    return (rng, Outcome::DefenderWon);
                }
            }
        }
    }

    #[test]
    fn without_retreat_the_duel_is_a_pure_dice_race() {
        for seed in 0..300 {
            for (ahp, dhp) in [(1, 1), (2, 3), (4, 4), (5, 2), (6, 6)] {
                let mut rng = Rng::new(seed);
                let (mut a, mut d) = (fighter(ahp, 50), fighter(dhp, 50));
                let out = duel(&mut rng, 600, &mut a, &mut d, NO_RETREAT, true);
                assert_eq!((rng, out), replay_dice_only(seed, 600, ahp, dhp), "seed {seed}");
            }
        }
    }

    #[test]
    fn barbarians_and_one_hp_winners_draw_no_retreat_die() {
        let flags = RetreatFlags { attacker: true, defender: true };
        for seed in 0..300 {
            // Barbarians never retreat, whatever their retreat percentage.
            let mut rng = Rng::new(seed);
            let mut a = Fighter { owned: false, ..fighter(4, 100) };
            let mut d = Fighter { owned: false, ..fighter(4, 100) };
            let out = duel(&mut rng, 512, &mut a, &mut d, flags, true);
            assert_eq!((rng, out), replay_dice_only(seed, 512, 4, 4), "barbarians, seed {seed}");

            // A 1 HP attacker facing a 2 HP owned defender: the only way the
            // defender can drop to 1 HP is by the attacker winning a round,
            // which kills nobody but leaves the winner at 1 HP, and a retreat
            // roll needs the winner above 1 HP. The defender is the eligible
            // retreater here, so no retreat die may ever be drawn.
            let mut rng = Rng::new(seed);
            let mut a = Fighter { owned: false, ..fighter(1, 100) };
            let mut d = fighter(2, 100);
            let only_defender = RetreatFlags { attacker: false, defender: true };
            let out = duel(&mut rng, 512, &mut a, &mut d, only_defender, true);
            assert_eq!((rng, out), replay_dice_only(seed, 512, 1, 2), "1 HP winner, seed {seed}");
        }
    }

    #[test]
    fn attacker_retreat_mirrors_the_defender_block() {
        // The attacker needs the round loss that leaves it at 1 HP while the
        // defender is above 1 HP.
        let odds = 512;
        let seed = seed_with_first_roll(|r| r < odds);
        let mut rng = Rng::new(seed);
        let mut reference = Rng::new(seed);
        let mut a = fighter(2, 100);
        let mut d = fighter(3, 50);
        let flags = RetreatFlags { attacker: true, defender: false };
        assert_eq!(duel(&mut rng, odds, &mut a, &mut d, flags, true), Outcome::AttackerRetreated);
        reference.below(ROUND_DIE);
        reference.below(100); // defender.retreat_pct + 50
        assert_eq!(rng, reference);
    }

    #[test]
    fn bombard_draws_rate_of_fire_dice_and_never_exits_early() {
        let mut rng = Rng::new(9);
        let mut reference = Rng::new(9);
        // odds 1: almost every die hits, so a first-die hit would tempt an
        // early exit; all 5 dice must still be drawn.
        let mut want = false;
        for _ in 0..5 {
            if reference.below(ROUND_DIE) >= 1 {
                want = true;
            }
        }
        assert_eq!(bombard_hit(&mut rng, 1, 5), want);
        assert_eq!(rng, reference);
        // odds 1023 vs a zero-length volley.
        let mut rng = Rng::new(9);
        assert!(!bombard_hit(&mut rng, 1, 0));
        assert_eq!(rng, Rng::new(9));
    }

    #[test]
    fn bombard_hit_is_any_die_at_or_above_the_odds() {
        let odds = 1000;
        let seed = seed_with_first_roll(|r| r < odds);
        // Verify by replaying the dice.
        for rof in 1..6 {
            let mut replay = Rng::new(seed);
            let expect = (0..rof).any(|_| replay.below(ROUND_DIE) >= odds);
            let mut rng = Rng::new(seed);
            assert_eq!(bombard_hit(&mut rng, odds, rof), expect, "rof {rof}");
        }
    }

    #[test]
    fn tile_strike_uses_an_implicit_strength_of_sixteen() {
        // Open ground: (0 + 0 + 100) * 16 / 100 = 16; against bombard 16 -> 512.
        assert_eq!(TILE_DEFENSE_STRENGTH, 16);
        assert_eq!(strike_odds(TILE_DEFENSE_STRENGTH, 0, 0, 16), Some(512));
        // Hills + fortress: (50 + 50 + 100) * 16 / 100 = 32 vs 16 -> 1024*32/48 = 682.
        assert_eq!(strike_odds(TILE_DEFENSE_STRENGTH, 50, 50, 16), Some(682));
        // A zero-strength attacker never gets through (clamped to 1023).
        assert_eq!(strike_odds(TILE_DEFENSE_STRENGTH, 0, 0, 0), Some(ODDS_MAX));
        // Both strengths zero: the original divides by zero.
        assert_eq!(strike_odds(0, 0, 0, 0), None);
    }

    #[test]
    fn city_strike_base_comes_from_the_rules_by_mode() {
        let rules = Rules::CONQUESTS;
        assert_eq!(StrikeMode::Population.base(&rules), 16);
        assert_eq!(StrikeMode::Improvement.base(&rules), 16);
        // Mode 0 reads the first slot, mode 1 the second.
        let other = Rules { city_strike_base: [48, 8], ..rules };
        assert_eq!(strike_odds(StrikeMode::Population.base(&other), 0, 0, 16), Some(768));
        assert_eq!(strike_odds(StrikeMode::Improvement.base(&other), 0, 0, 16), Some(341));
    }

    #[test]
    fn strike_rolls_is_the_larger_of_rate_of_fire_and_the_request() {
        assert_eq!(strike_rolls(0, 1), 1);
        assert_eq!(strike_rolls(3, 1), 3);
        assert_eq!(strike_rolls(2, 5), 5);
    }

    #[test]
    fn strike_stops_at_the_first_success() {
        let odds = 512;
        let seed = seed_with_first_roll(|r| r >= odds);
        let mut rng = Rng::new(seed);
        let mut reference = Rng::new(seed);
        assert!(strike_hit(&mut rng, odds, 6));
        reference.below(ROUND_DIE);
        assert_eq!(rng, reference, "one draw, not six");
    }

    #[test]
    fn strike_draws_every_die_when_none_succeeds() {
        // Odds 1023: only a die of exactly 1023 succeeds.
        let odds = ODDS_MAX;
        let seed = (0u32..)
            .find(|&s| {
                let mut r = Rng::new(s);
                (0..3).all(|_| r.below(ROUND_DIE) < odds)
            })
            .unwrap();
        let mut rng = Rng::new(seed);
        let mut reference = Rng::new(seed);
        assert!(!strike_hit(&mut rng, odds, 3));
        for _ in 0..3 {
            reference.below(ROUND_DIE);
        }
        assert_eq!(rng, reference);
    }

    #[test]
    fn strike_with_no_rolls_fails_without_a_draw() {
        let mut rng = Rng::new(3);
        assert!(!strike_hit(&mut rng, 1, 0));
        assert!(!strike_hit(&mut rng, 1, -2));
        assert_eq!(rng, Rng::new(3));
    }

    #[test]
    fn defensive_bombard_victim_needs_defense_and_hp_to_spare() {
        assert!(defensive_bombard_victim_ok(Domain::Land, 4, 2));
        assert!(!defensive_bombard_victim_ok(Domain::Land, 4, 1));
        assert!(!defensive_bombard_victim_ok(Domain::Sea, 4, 1));
        assert!(!defensive_bombard_victim_ok(Domain::Sea, 4, -3));
        assert!(defensive_bombard_victim_ok(Domain::Sea, 4, 10_000));
        // Air has no HP floor.
        assert!(defensive_bombard_victim_ok(Domain::Air, 1, 1));
        assert!(defensive_bombard_victim_ok(Domain::Air, 1, 0));
        // A unit with no defense is never a victim.
        assert!(!defensive_bombard_victim_ok(Domain::Air, 0, 5));
        assert!(!defensive_bombard_victim_ok(Domain::Land, 0, 5));
    }

    fn shooter(strength: i32) -> ShooterCandidate {
        ShooterCandidate {
            is_defender: false,
            carried_by_defender: false,
            domain: Domain::Land,
            bombard_strength: strength,
            has_ability_3: false,
            already_fired: false,
        }
    }

    #[test]
    fn shooter_is_the_strictly_strongest_eligible_unit() {
        let units = [shooter(2), shooter(4), shooter(4), shooter(3)];
        assert_eq!(pick_defensive_shooter(Domain::Land, &units), Some(1), "first of equals");
        assert_eq!(pick_defensive_shooter(Domain::Land, &[]), None);
        assert_eq!(pick_defensive_shooter(Domain::Land, &[shooter(0)]), None);
    }

    #[test]
    fn shooter_exclusions() {
        let mut units =
            [shooter(9), shooter(8), shooter(7), shooter(6), shooter(5), shooter(1)];
        units[0].is_defender = true;
        units[1].carried_by_defender = true;
        units[2].domain = Domain::Sea;
        units[3].has_ability_3 = true;
        units[4].already_fired = true;
        assert_eq!(pick_defensive_shooter(Domain::Land, &units), Some(5));
        // For a sea victim the only same-domain unit is the third one.
        assert_eq!(pick_defensive_shooter(Domain::Sea, &units), Some(2));
    }

    #[test]
    fn defensive_bombard_is_one_die_and_one_point_of_damage() {
        let odds = 600;
        let hit_seed = seed_with_first_roll(|r| r >= odds);
        let mut rng = Rng::new(hit_seed);
        let mut reference = Rng::new(hit_seed);
        let mut victim = fighter(5, 0);
        let shot = defensive_bombard(&mut rng, odds, &mut victim);
        assert_eq!(shot, DefensiveBombard { hit: true, left_at_one_hp: false });
        reference.below(ROUND_DIE);
        assert_eq!(rng, reference, "exactly one die");
        assert_eq!(victim.damage, 1);

        let miss_seed = seed_with_first_roll(|r| r < odds);
        let mut rng = Rng::new(miss_seed);
        let mut victim = fighter(5, 0);
        let shot = defensive_bombard(&mut rng, odds, &mut victim);
        assert_eq!(shot, DefensiveBombard { hit: false, left_at_one_hp: false });
        assert_eq!(victim.damage, 0);
    }

    #[test]
    fn defensive_bombard_reports_only_a_hit_that_leaves_one_hp() {
        // Two HP left: a hit leaves one and is reported.
        let seed = seed_with_first_roll(|r| r >= 1);
        let mut victim = Fighter { damage: 1, ..fighter(3, 0) };
        let shot = defensive_bombard(&mut Rng::new(seed), 1, &mut victim);
        assert_eq!(shot, DefensiveBombard { hit: true, left_at_one_hp: true });
        assert_eq!(victim.remaining(), 1);

        // A miss against a unit that is already at one HP is not reported.
        let seed = seed_with_first_roll(|r| r < 1000);
        let mut victim = Fighter { damage: 2, ..fighter(3, 0) };
        let shot = defensive_bombard(&mut Rng::new(seed), 1000, &mut victim);
        assert_eq!(shot, DefensiveBombard { hit: false, left_at_one_hp: false });
    }

    #[test]
    fn interception_needs_attack_or_defense() {
        assert!(!interception_runs(0, 0));
        assert!(interception_runs(0, 3));
        assert!(interception_runs(4, 0));
    }

    #[test]
    fn interception_duel_is_a_pure_dice_race() {
        for seed in 0..200 {
            for (ahp, dhp) in [(1, 1), (3, 2), (4, 4), (2, 5)] {
                let mut rng = Rng::new(seed);
                // A retreat percentage of 100 would show up as extra draws.
                let (mut a, mut d) = (fighter(ahp, 100), fighter(dhp, 100));
                let out = interception_duel(&mut rng, 700, &mut a, &mut d);
                assert_eq!((rng, out), replay_dice_only(seed, 700, ahp, dhp), "seed {seed}");
            }
        }
    }

    fn facts(domain: Domain, tile_is_water: bool) -> DefenderFacts {
        DefenderFacts { carried: false, domain, tile_is_water, defense: 4, remaining_hp: 10 }
    }

    #[test]
    fn eligibility_matches_domain_and_tile() {
        use Eligibility::*;
        assert_eq!(defender_eligibility(&facts(Domain::Land, false)), Eligible);
        assert_eq!(defender_eligibility(&facts(Domain::Land, true)), Ineligible);
        assert_eq!(defender_eligibility(&facts(Domain::Sea, true)), Eligible);
        assert_eq!(defender_eligibility(&facts(Domain::Sea, false)), Ineligible);
        // Aircraft never defend on a tile, over land or sea.
        assert_eq!(defender_eligibility(&facts(Domain::Air, false)), Ineligible);
        assert_eq!(defender_eligibility(&facts(Domain::Air, true)), Ineligible);
        // Loaded units are covered by their carrier.
        let carried = DefenderFacts { carried: true, ..facts(Domain::Land, false) };
        assert_eq!(defender_eligibility(&carried), Ineligible);
    }

    #[test]
    fn eligibility_hp_rules() {
        use Eligibility::*;
        let base = facts(Domain::Land, false);
        assert_eq!(defender_eligibility(&DefenderFacts { remaining_hp: 1, ..base }), Eligible);
        assert_eq!(defender_eligibility(&DefenderFacts { remaining_hp: 0, ..base }), Zombie);
        assert_eq!(defender_eligibility(&DefenderFacts { remaining_hp: -2, ..base }), Zombie);
        assert_eq!(defender_eligibility(&DefenderFacts { remaining_hp: 20_000, ..base }), Eligible);
        // No defense: eligible even with no HP (a worker, say).
        let worker = DefenderFacts { defense: 0, remaining_hp: 0, ..base };
        assert_eq!(defender_eligibility(&worker), Eligible);
    }

    #[test]
    fn rating_is_percent_scaled_defense_times_hp() {
        // 4 defense, 10 HP, no fortify: 100 * 4 * 10 / 100 = 40.
        assert_eq!(defender_rating(0, 4, 10), 40);
        // Fortified (+25): 125 * 4 * 10 / 100 = 50.
        assert_eq!(defender_rating(25, 4, 10), 50);
        // Truncates toward zero: 125 * 3 * 3 / 100 = 11.25 -> 11.
        assert_eq!(defender_rating(25, 3, 3), 11);
        // HP is clamped to 0..=9999.
        assert_eq!(defender_rating(0, 4, -5), 0);
        assert_eq!(defender_rating(0, 1, 50_000), 9999);
    }

    fn rank(rating: i32) -> DefenderRank {
        DefenderRank { rating, is_king: false, cargo: 0, attack: 1, bombard: 0, max_hp: 10 }
    }

    #[test]
    fn best_defender_is_the_highest_rating_first_of_equals() {
        let ranks = [rank(30), rank(50), rank(50), rank(40)];
        assert_eq!(pick_best_defender(&ranks), Some(1));
        assert_eq!(pick_best_defender(&[]), None);
        // A lone zero-rated unit (a worker) is not chosen.
        assert_eq!(pick_best_defender(&[rank(0)]), None);
    }

    #[test]
    fn king_defends_last_even_with_a_higher_rating() {
        let king = DefenderRank { is_king: true, ..rank(90) };
        assert_eq!(pick_best_defender(&[king, rank(10)]), Some(1));
        assert_eq!(pick_best_defender(&[rank(10), king]), Some(0));
        // Alone, or against another king, the ordinary rating rule applies.
        assert_eq!(pick_best_defender(&[king]), Some(0));
        let weaker_king = DefenderRank { is_king: true, ..rank(20) };
        assert_eq!(pick_best_defender(&[weaker_king, king]), Some(1));
    }

    #[test]
    fn equal_ratings_prefer_fewer_cargo_then_lower_attack_bombard_and_hp() {
        let base = rank(40);
        let loaded = DefenderRank { cargo: 2, ..base };
        assert_eq!(pick_best_defender(&[loaded, base]), Some(1), "fewer cargo");
        let striker = DefenderRank { attack: 5, ..base };
        assert_eq!(pick_best_defender(&[striker, base]), Some(1), "lower attack");
        let bombarder = DefenderRank { bombard: 3, ..base };
        assert_eq!(pick_best_defender(&[bombarder, base]), Some(1), "lower bombard");
        let tough = DefenderRank { max_hp: 20, ..base };
        assert_eq!(pick_best_defender(&[tough, base]), Some(1), "lower max HP");
        // Fully equal keeps the first.
        assert_eq!(pick_best_defender(&[base, base]), Some(0));
        // The keys are ordered: cargo is compared before attack.
        let low_attack_but_loaded = DefenderRank { cargo: 1, attack: 0, ..base };
        assert_eq!(pick_best_defender(&[low_attack_but_loaded, striker]), Some(1));
    }

    #[test]
    fn zero_rated_candidates_only_win_a_tie_when_ties_need_not_be_positive() {
        let zero = rank(0);
        assert!(!is_better_defender(&zero, None, 0, true));
        assert!(is_better_defender(&zero, None, 0, false));
        // A zero-rated King takes an empty or zero slot either way.
        let king = DefenderRank { is_king: true, ..zero };
        assert!(is_better_defender(&king, None, 0, true));
    }

    #[test]
    fn weakest_defender_is_the_lowest_rating_and_skips_zero() {
        let ranks = [rank(40), rank(0), rank(15), rank(15), rank(60)];
        assert_eq!(pick_weakest_defender(&ranks), Some(2), "first of equals; zero skipped");
        assert_eq!(pick_weakest_defender(&[rank(0)]), None);
        assert_eq!(pick_weakest_defender(&[]), None);
    }

    #[test]
    fn weakest_search_starts_at_a_rating_of_1000() {
        // A rating equal to the start value ties with it and, with no
        // incumbent, wins; anything above never does.
        assert_eq!(WEAKEST_DEFENDER_START, 1000);
        assert_eq!(pick_weakest_defender(&[rank(1000)]), Some(0));
        assert_eq!(pick_weakest_defender(&[rank(1001)]), None);
        assert_eq!(pick_weakest_defender(&[rank(999)]), Some(0));
    }

    #[test]
    fn weakest_search_prefers_a_king() {
        let king = DefenderRank { is_king: true, ..rank(90) };
        assert_eq!(pick_weakest_defender(&[rank(10), king]), Some(1));
        assert_eq!(pick_weakest_defender(&[king, rank(10)]), Some(0));
    }

    #[test]
    fn weakest_ties_prefer_more_cargo_and_higher_attack() {
        let base = rank(40);
        let loaded = DefenderRank { cargo: 2, ..base };
        assert_eq!(pick_weakest_defender(&[base, loaded]), Some(1));
        let striker = DefenderRank { attack: 5, ..base };
        assert_eq!(pick_weakest_defender(&[base, striker]), Some(1));
        assert_eq!(pick_weakest_defender(&[base, base]), Some(0));
    }

    #[test]
    fn promotion_die_by_level_barbarians_and_militaristic() {
        assert_eq!(promotion_die(0, false, false), Some(2));
        assert_eq!(promotion_die(1, false, false), Some(4));
        assert_eq!(promotion_die(2, false, false), Some(8));
        assert_eq!(promotion_die(3, false, false), None);
        assert_eq!(promotion_die(-1, false, false), None);
        // A barbarian victim doubles the die, so a promotion is half as likely.
        assert_eq!(promotion_die(1, true, false), Some(8));
        // Militaristic halves it, after the doubling.
        assert_eq!(promotion_die(1, false, true), Some(2));
        assert_eq!(promotion_die(1, true, true), Some(4));
    }

    #[test]
    fn promotion_roll_draws_one_die_unless_already_failed() {
        let (seed, die) = (17, 4);
        let mut rng = Rng::new(seed);
        let mut reference = Rng::new(seed);
        let expect = reference.below(die) == 0;
        assert_eq!(promotion_roll(&mut rng, die, false), expect);
        assert_eq!(rng, reference);
        // A unit that already failed this turn is promoted without a draw.
        let mut rng = Rng::new(seed);
        assert!(promotion_roll(&mut rng, die, true));
        assert_eq!(rng, Rng::new(seed));
    }

    #[test]
    fn a_die_of_one_always_promotes_but_still_advances_the_generator() {
        // Militaristic civ, green unit, ordinary victim: die 1.
        let die = promotion_die(0, false, true).unwrap();
        assert_eq!(die, 1);
        for seed in 0..100 {
            let mut rng = Rng::new(seed);
            assert!(promotion_roll(&mut rng, die, false));
            assert_ne!(rng, Rng::new(seed), "the draw still happens");
        }
    }

    fn leader_ok() -> LeaderCheck {
        LeaderCheck {
            winner_level: 3,
            winner_is_land: true,
            loser_is_barbarian: false,
            winner_already_made_leader: false,
            winner_in_army: false,
            civ_has_leader: false,
        }
    }

    #[test]
    fn leader_needs_every_condition() {
        assert!(leader_eligible(&leader_ok()));
        assert!(!leader_eligible(&LeaderCheck { winner_level: 2, ..leader_ok() }));
        assert!(!leader_eligible(&LeaderCheck { winner_is_land: false, ..leader_ok() }));
        assert!(!leader_eligible(&LeaderCheck { loser_is_barbarian: true, ..leader_ok() }));
        assert!(!leader_eligible(&LeaderCheck { winner_already_made_leader: true, ..leader_ok() }));
        assert!(!leader_eligible(&LeaderCheck { winner_in_army: true, ..leader_ok() }));
        assert!(!leader_eligible(&LeaderCheck { civ_has_leader: true, ..leader_ok() }));
    }

    #[test]
    fn leader_die_is_16_or_12_and_doubles_when_the_defender_won() {
        assert_eq!(leader_die(false, true), 16);
        assert_eq!(leader_die(true, true), 12);
        assert_eq!(leader_die(false, false), 32);
        assert_eq!(leader_die(true, false), 24);
    }

    #[test]
    fn leader_roll_is_one_draw_and_succeeds_on_zero() {
        for seed in 0..200u32 {
            let mut rng = Rng::new(seed);
            let mut reference = Rng::new(seed);
            assert_eq!(leader_roll(&mut rng, 16), reference.below(16) == 0);
            assert_eq!(rng, reference);
        }
    }

    #[test]
    fn enslave_succeeds_below_33_of_100() {
        let seed_with_value = |v: i32| (0u32..).find(|&s| Rng::new(s).below(100) == v).unwrap();
        assert_eq!(ENSLAVE_PERCENT, 33);
        assert!(enslave_roll(&mut Rng::new(seed_with_value(0))));
        assert!(enslave_roll(&mut Rng::new(seed_with_value(32))));
        assert!(!enslave_roll(&mut Rng::new(seed_with_value(33))));
        assert!(!enslave_roll(&mut Rng::new(seed_with_value(99))));
    }

    #[test]
    fn golden_age_needs_a_unique_unit_a_real_victim_and_no_age_scheduled() {
        assert!(golden_age_triggers(true, false, -1));
        assert!(!golden_age_triggers(false, false, -1));
        assert!(!golden_age_triggers(true, true, -1));
        assert!(!golden_age_triggers(true, false, 42));
    }

    // -----------------------------------------------------------------------
    // Ranged attacks
    // -----------------------------------------------------------------------

    const LAND_TILE: BombardTile = BombardTile { is_water: false, has_city: false };
    const CITY_TILE: BombardTile = BombardTile { is_water: false, has_city: true };
    const SEA_TILE: BombardTile = BombardTile { is_water: true, has_city: false };

    fn cand(domain: Domain, remaining_hp: i32) -> BombardCandidate {
        BombardCandidate { carried: false, defense: 3, domain, remaining_hp, hostile: true }
    }

    /// A seed whose first `n` dice all satisfy `ok`.
    fn seed_where_first(n: usize, ok: impl Fn(i32) -> bool) -> u32 {
        (0u32..)
            .find(|&s| {
                let mut r = Rng::new(s);
                (0..n).all(|_| ok(r.below(ROUND_DIE)))
            })
            .unwrap()
    }

    #[test]
    fn lethal_ability_bits_match_the_cruise_missile_row() {
        // `conquests.biq` Cruise Missile: PRTO word +0x88 = 0x18000008.
        let word = 0x1800_0008u32;
        assert_eq!((word >> ABILITY_LETHAL_LAND_BOMBARD) & 1, 1);
        assert_eq!((word >> ABILITY_LETHAL_SEA_BOMBARD) & 1, 1);
        assert_eq!((word >> 3) & 1, 1, "bit 3 is the Cruise Missile ability itself");
        assert_eq!(Lethality::BOTH, Lethality { land: true, sea: true });
    }

    #[test]
    fn target_class_lists_follow_the_attacker() {
        use Domain::*;
        // Ships, and anything carrying the AI cruise-missile flag: sea, air, land.
        for water in [false, true] {
            assert_eq!(bombard_target_order(Sea, false, water), [Sea, Air, Land]);
            assert_eq!(bombard_target_order(Land, true, water), [Sea, Air, Land]);
            // The flag beats the domain: an aircraft with it also goes sea first.
            assert_eq!(bombard_target_order(Air, true, water), [Sea, Air, Land]);
            // Aircraft: air, sea, land.
            assert_eq!(bombard_target_order(Air, false, water), [Air, Sea, Land]);
        }
        // Land attackers see one class, chosen by the tile.
        assert_eq!(bombard_target_order(Land, false, false), [Land]);
        assert_eq!(bombard_target_order(Land, false, true), [Sea]);
    }

    #[test]
    fn candidate_needs_no_carrier_defense_and_hostility() {
        let ok = |c: BombardCandidate| {
            bombard_candidate_ok(&c, Domain::Land, &LAND_TILE, Lethality::NONE)
        };
        assert!(ok(cand(Domain::Land, 3)));
        assert!(!ok(BombardCandidate { carried: true, ..cand(Domain::Land, 3) }));
        assert!(!ok(BombardCandidate { defense: 0, ..cand(Domain::Land, 3) }));
        assert!(!ok(BombardCandidate { defense: -1, ..cand(Domain::Land, 3) }));
        assert!(!ok(BombardCandidate { hostile: false, ..cand(Domain::Land, 3) }));
        // Wrong class for the scan in progress.
        assert!(!bombard_candidate_ok(&cand(Domain::Land, 3), Domain::Sea, &LAND_TILE, Lethality::NONE));
    }

    #[test]
    fn one_hp_floor_protects_unless_the_attacker_is_lethal_to_that_domain() {
        let land = cand(Domain::Land, 1);
        let sea = cand(Domain::Sea, 1);
        let air = cand(Domain::Air, 1);
        let by = |c: &BombardCandidate, tile: &BombardTile, l: Lethality| {
            bombard_candidate_ok(c, c.domain, tile, l)
        };
        let l27 = Lethality { land: true, sea: false };
        let l28 = Lethality { land: false, sea: true };
        assert!(!by(&land, &LAND_TILE, Lethality::NONE));
        assert!(!by(&land, &LAND_TILE, l28), "ability 28 does not open land units");
        assert!(by(&land, &LAND_TILE, l27));
        assert!(!by(&sea, &SEA_TILE, Lethality::NONE));
        assert!(!by(&sea, &SEA_TILE, l27), "ability 27 does not open ships");
        assert!(by(&sea, &SEA_TILE, l28));
        // Aircraft in a city have no floor at all.
        assert!(by(&air, &CITY_TILE, Lethality::NONE));
        // Two HP is always enough; a unit with no HP left is floored unless lethal.
        assert!(by(&cand(Domain::Land, 2), &LAND_TILE, Lethality::NONE));
        assert!(!by(&cand(Domain::Land, 0), &LAND_TILE, Lethality::NONE));
        assert!(!by(&cand(Domain::Land, -4), &LAND_TILE, Lethality::NONE));
        assert!(by(&cand(Domain::Land, 0), &LAND_TILE, l27));
        // The remaining HP is clamped to 9999 only for the comparison.
        assert!(by(&cand(Domain::Land, 50_000), &LAND_TILE, Lethality::NONE));
    }

    #[test]
    fn tile_rules_for_each_domain() {
        let ok = |d: Domain, tile: &BombardTile| {
            bombard_candidate_ok(&cand(d, 5), d, tile, Lethality::NONE)
        };
        // Land units: land tiles only.
        assert!(ok(Domain::Land, &LAND_TILE));
        assert!(ok(Domain::Land, &CITY_TILE));
        assert!(!ok(Domain::Land, &SEA_TILE));
        // Ships: any water tile; on land only inside a city (a port).
        assert!(ok(Domain::Sea, &SEA_TILE));
        assert!(ok(Domain::Sea, &CITY_TILE));
        assert!(!ok(Domain::Sea, &LAND_TILE));
        // Aircraft: only on a land tile with a city.
        assert!(ok(Domain::Air, &CITY_TILE));
        assert!(!ok(Domain::Air, &LAND_TILE));
        assert!(!ok(Domain::Air, &SEA_TILE));
        let coastal_city = BombardTile { is_water: true, has_city: true };
        assert!(!ok(Domain::Air, &coastal_city), "water beats the city for aircraft");
    }

    fn ranked(c: BombardCandidate, rating: i32) -> (BombardCandidate, DefenderRank) {
        (c, rank(rating))
    }

    #[test]
    fn target_choice_takes_the_first_class_with_a_legal_unit() {
        use Domain::*;
        // A city holding infantry (rating 90), a destroyer (20) and a fighter (5).
        let units = [
            ranked(cand(Land, 9), 90),
            ranked(cand(Sea, 4), 20),
            ranked(cand(Air, 2), 5),
        ];
        let pick = |attacker: Domain, flag: bool, lethal: Lethality| {
            let order = bombard_target_order(attacker, flag, CITY_TILE.is_water);
            pick_bombard_target(order, &CITY_TILE, lethal, &units)
        };
        assert_eq!(pick(Sea, false, Lethality::NONE), Some(1), "ships fire at ships first");
        assert_eq!(pick(Air, false, Lethality::NONE), Some(2), "aircraft at aircraft first");
        assert_eq!(pick(Land, false, Lethality::NONE), Some(0), "land attackers see land units only");
        assert_eq!(pick(Land, true, Lethality::BOTH), Some(1), "the missile flag means sea first");
    }

    #[test]
    fn target_choice_falls_through_empty_classes_and_ignores_rating_across_them() {
        use Domain::*;
        let order = bombard_target_order(Sea, false, false);
        // No ship and no aircraft: the land unit is taken whatever its rating.
        let only_land = [ranked(cand(Land, 5), 1)];
        assert_eq!(pick_bombard_target(order, &CITY_TILE, Lethality::NONE, &only_land), Some(0));
        // A floored ship does not count as a legal ship, so the aircraft is next.
        let floored = [ranked(cand(Sea, 1), 99), ranked(cand(Air, 5), 3), ranked(cand(Land, 5), 80)];
        assert_eq!(pick_bombard_target(order, &CITY_TILE, Lethality::NONE, &floored), Some(1));
        // A lethal attacker may shoot that ship after all.
        let l28 = Lethality { land: false, sea: true };
        assert_eq!(pick_bombard_target(order, &CITY_TILE, l28, &floored), Some(0));
        // A class whose only unit rates zero is passed over.
        let zero_rated = [ranked(cand(Sea, 5), 0), ranked(cand(Land, 5), 7)];
        assert_eq!(pick_bombard_target(order, &CITY_TILE, Lethality::NONE, &zero_rated), Some(1));
        assert_eq!(pick_bombard_target(order, &CITY_TILE, Lethality::NONE, &[]), None);
    }

    #[test]
    fn target_choice_rates_within_a_class_with_the_defender_comparator() {
        use Domain::*;
        let order = bombard_target_order(Land, false, false);
        let units = [ranked(cand(Land, 5), 30), ranked(cand(Land, 5), 50), ranked(cand(Land, 5), 50)];
        assert_eq!(pick_bombard_target(order, &LAND_TILE, Lethality::NONE, &units), Some(1));
        // A King is the last resort even with the best rating.
        let king = DefenderRank { is_king: true, ..rank(90) };
        let with_king = [(cand(Land, 5), king), ranked(cand(Land, 5), 10)];
        assert_eq!(pick_bombard_target(order, &LAND_TILE, Lethality::NONE, &with_king), Some(1));
    }

    #[test]
    fn target_count_spans_every_class_and_never_uses_lethality() {
        use Domain::*;
        let order = bombard_target_order(Air, false, false);
        let units = [
            cand(Land, 5),
            cand(Land, 5),
            cand(Sea, 4),
            cand(Air, 2),
            cand(Land, 1),                                         // floored
            BombardCandidate { carried: true, ..cand(Land, 5) },   // loaded
            BombardCandidate { hostile: false, ..cand(Land, 5) },  // not an enemy
        ];
        assert_eq!(count_bombard_targets(order, &CITY_TILE, &units), 4);
        // Outside a city only land units can be counted.
        assert_eq!(count_bombard_targets(order, &LAND_TILE, &units), 2);
        assert_eq!(count_bombard_targets(order, &CITY_TILE, &[]), 0);
    }

    #[test]
    fn air_strike_die_grows_with_the_garrison() {
        let die = air_city_mode_die;
        for n in [-1, 0, 1, 4] {
            assert_eq!(die(n), 4, "n = {n}");
        }
        for n in [5, 6, 8] {
            assert_eq!(die(n), 5, "n = {n}");
        }
        for n in [9, 10, 40] {
            assert_eq!(die(n), 6, "n = {n}");
        }
        // Modes 0 and 1 are the only city strikes: 2 of 4, 2 of 5, 2 of 6.
        assert_eq!(StrikeMode::from_mode(0), Some(StrikeMode::Population));
        assert_eq!(StrikeMode::from_mode(1), Some(StrikeMode::Improvement));
        assert_eq!(StrikeMode::from_mode(2), None);
        assert_eq!(StrikeMode::from_mode(-1), None);
        assert_eq!(NO_TARGET_CITY_MODE_DIE, 2);
    }

    #[test]
    fn port_halves_the_defenders_odds() {
        assert_eq!(port_odds(1023), 512);
        assert_eq!(port_odds(512), 256);
        assert_eq!(port_odds(511), 256);
        assert_eq!(port_odds(3), 2);
        assert_eq!(port_odds(2), 1);
        assert_eq!(port_odds(1), 1);
        assert_eq!(port_odds(0), 0);
        // It agrees with the x87 sequence: exact multiply by 0.5, then truncate.
        for odds in ODDS_MIN..=ODDS_MAX {
            assert_eq!(port_odds(odds), ((f64::from(odds) + 1.0) * 0.5) as i32);
        }
    }

    fn target(hp: i32) -> Fighter {
        Fighter { max_hp: hp, damage: 0, retreat_pct: 0, owned: true }
    }

    #[test]
    fn floor_stops_the_volley_at_one_hp() {
        // Odds 1: a die of zero would be the only miss, so find dice that hit.
        let seed = seed_where_first(2, |r| r >= 1);
        let mut rng = Rng::new(seed);
        let mut t = target(3);
        let v = ranged_volley(&mut rng, 1, 5, &mut t, Domain::Land, Lethality::NONE);
        assert_eq!(v, Volley { hits: 2, end: VolleyEnd::Spared, left_at_one_hp: true });
        assert_eq!(t.remaining(), 1);
        let mut reference = Rng::new(seed);
        reference.discard(2);
        assert_eq!(rng, reference, "two dice, then it stops");
    }

    #[test]
    fn lethal_volley_goes_through_one_hp_and_kills() {
        let seed = seed_where_first(3, |r| r >= 1);
        for lethal in [Lethality::BOTH, Lethality { land: true, sea: false }] {
            let mut rng = Rng::new(seed);
            let mut t = target(3);
            let v = ranged_volley(&mut rng, 1, 5, &mut t, Domain::Land, lethal);
            assert_eq!(v, Volley { hits: 3, end: VolleyEnd::Killed, left_at_one_hp: true });
            assert_eq!(t.remaining(), 0);
            let mut reference = Rng::new(seed);
            reference.discard(3);
            assert_eq!(rng, reference);
        }
        // Ability 28 does not help against land units.
        let mut t = target(3);
        let v = ranged_volley(
            &mut Rng::new(seed),
            1,
            5,
            &mut t,
            Domain::Land,
            Lethality { land: false, sea: true },
        );
        assert_eq!(v.end, VolleyEnd::Spared);
    }

    #[test]
    fn sea_targets_need_ability_28_and_air_targets_need_nothing() {
        let seed = seed_where_first(5, |r| r >= 1);
        let l27 = Lethality { land: true, sea: false };
        let l28 = Lethality { land: false, sea: true };
        let run = |domain, lethal| {
            let mut t = target(2);
            ranged_volley(&mut Rng::new(seed), 1, 5, &mut t, domain, lethal).end
        };
        assert_eq!(run(Domain::Sea, l27), VolleyEnd::Spared);
        assert_eq!(run(Domain::Sea, l28), VolleyEnd::Killed);
        assert_eq!(run(Domain::Air, Lethality::NONE), VolleyEnd::Killed);
    }

    #[test]
    fn volley_throws_every_shot_and_never_stops_on_a_hit() {
        // Plenty of HP: all five dice are drawn even though the first hits.
        let seed = seed_where_first(5, |r| r >= 1);
        let mut rng = Rng::new(seed);
        let mut t = target(50);
        let v = ranged_volley(&mut rng, 1, 5, &mut t, Domain::Land, Lethality::NONE);
        assert_eq!(v, Volley { hits: 5, end: VolleyEnd::Exhausted, left_at_one_hp: false });
        let mut reference = Rng::new(seed);
        reference.discard(5);
        assert_eq!(rng, reference);
    }

    #[test]
    fn volley_that_always_misses_draws_all_dice_and_changes_nothing() {
        let seed = seed_where_first(4, |r| r < ODDS_MAX);
        let mut rng = Rng::new(seed);
        let mut t = target(3);
        let v = ranged_volley(&mut rng, ODDS_MAX, 4, &mut t, Domain::Land, Lethality::NONE);
        assert_eq!(v, Volley { hits: 0, end: VolleyEnd::Exhausted, left_at_one_hp: false });
        assert_eq!(t.damage, 0);
        let mut reference = Rng::new(seed);
        reference.discard(4);
        assert_eq!(rng, reference);
        // A rate of fire of zero (a Privateer) throws nothing.
        let mut rng = Rng::new(7);
        let v = ranged_volley(&mut rng, 1, 0, &mut t, Domain::Sea, Lethality::NONE);
        assert_eq!(v.hits, 0);
        assert_eq!(rng, Rng::new(7));
    }

    #[test]
    fn volley_matches_a_replay_of_the_dice() {
        for seed in 0..300u32 {
            for (odds, rof, hp, domain) in [
                (300, 3, 4, Domain::Land),
                (700, 2, 2, Domain::Sea),
                (1, 3, 3, Domain::Air),
                (512, 5, 6, Domain::Land),
            ] {
                for lethal in [Lethality::NONE, Lethality::BOTH] {
                    let mut rng = Rng::new(seed);
                    let mut t = target(hp);
                    let got = ranged_volley(&mut rng, odds, rof, &mut t, domain, lethal);

                    let mut replay = Rng::new(seed);
                    let (mut rem, mut hits, mut one, mut end) = (hp, 0, false, VolleyEnd::Exhausted);
                    for _ in 0..rof {
                        if replay.below(ROUND_DIE) < odds {
                            continue;
                        }
                        rem -= 1;
                        hits += 1;
                        if rem == 1 {
                            one = true;
                            if lethal.floor_applies(domain) {
                                end = VolleyEnd::Spared;
                                break;
                            }
                        } else if rem <= 0 {
                            end = VolleyEnd::Killed;
                            break;
                        }
                    }
                    assert_eq!(got, Volley { hits, end, left_at_one_hp: one }, "seed {seed}");
                    assert_eq!(t.remaining(), rem);
                    assert_eq!(rng, replay);
                }
            }
        }
    }

    /// `conquests.biq`: Walls (BLDG row 7, body `+0x98 = 8`) and Coastal Fortress
    /// (row 23, body `+0x9C = 8`).
    fn walls() -> FacilityFacts {
        FacilityFacts {
            in_city: true,
            acts_on_city: true,
            obsolete: false,
            land_defense: 8,
            sea_defense: 0,
        }
    }

    fn fortress() -> FacilityFacts {
        FacilityFacts { land_defense: 0, sea_defense: 8, ..walls() }
    }

    fn barracks() -> FacilityFacts {
        FacilityFacts { land_defense: 0, sea_defense: 0, ..walls() }
    }

    #[test]
    fn walls_defend_towns_only_and_fortresses_every_size() {
        let rules = Rules::CONQUESTS;
        let b = [barracks(), walls(), fortress()];
        for pop in [1, 5, 6] {
            assert_eq!(land_bombard_defense(pop, &b, 0, &rules), 8, "pop {pop}");
        }
        for pop in [7, 12, 13, 40] {
            assert_eq!(land_bombard_defense(pop, &b, 0, &rules), 0, "pop {pop}");
        }
        assert_eq!(sea_bombard_defense(&b), 8, "no size gate on the naval side");
        // Wonders with flag 0x1000 multiply the land value (none ship).
        assert_eq!(land_bombard_defense(5, &b, 1, &rules), 16);
        assert_eq!(land_bombard_defense(5, &b, 2, &rules), 24);
        assert_eq!(sea_bombard_defense(&b), 8, "...and never the naval one");
    }

    #[test]
    fn defense_is_a_maximum_not_a_sum_and_skips_obsolete_and_absent_buildings() {
        let rules = Rules::CONQUESTS;
        let stronger = FacilityFacts { land_defense: 12, ..walls() };
        assert_eq!(land_bombard_defense(3, &[walls(), stronger], 0, &rules), 12);
        assert_eq!(land_bombard_defense(3, &[walls(), walls()], 0, &rules), 8);
        let obsolete = FacilityFacts { obsolete: true, ..walls() };
        assert_eq!(land_bombard_defense(3, &[obsolete], 0, &rules), 0);
        let absent = FacilityFacts { in_city: false, acts_on_city: false, ..walls() };
        assert_eq!(land_bombard_defense(3, &[absent], 0, &rules), 0);
        // A building that only acts on the city (an empire-wide effect) still counts.
        let remote = FacilityFacts { in_city: false, ..walls() };
        assert_eq!(land_bombard_defense(3, &[remote], 0, &rules), 8);
        assert_eq!(sea_bombard_defense(&[]), 0);
        // Negative values never lower the result below zero.
        let negative = FacilityFacts { sea_defense: -3, ..fortress() };
        assert_eq!(sea_bombard_defense(&[negative]), 0);
    }

    #[test]
    fn land_hit_removes_the_strongest_building_of_the_city() {
        let rules = Rules::CONQUESTS;
        let b = [barracks(), walls(), FacilityFacts { land_defense: 8, ..walls() }];
        assert_eq!(land_facility_hit(5, &b, &rules), FacilityHit::Destroyed(1), "first of equals");
        let stronger = [walls(), FacilityFacts { land_defense: 12, ..walls() }];
        assert_eq!(land_facility_hit(5, &stronger, &rules), FacilityHit::Destroyed(1));
        // Above town size nothing happens.
        assert_eq!(land_facility_hit(7, &b, &rules), FacilityHit::Nothing);
        assert_eq!(land_facility_hit(5, &[], &rules), FacilityHit::Nothing);
        // Obsolete buildings are skipped.
        let obsolete = FacilityFacts { obsolete: true, ..walls() };
        assert_eq!(land_facility_hit(5, &[obsolete], &rules), FacilityHit::Nothing);
    }

    #[test]
    fn land_hit_scan_starts_below_zero_so_a_zero_value_building_can_go() {
        // The running best is -1: with no wall at all the first held building
        // is taken. The caller never gets here, because it first requires a
        // positive land_bombard_defense.
        let rules = Rules::CONQUESTS;
        assert_eq!(land_facility_hit(5, &[barracks()], &rules), FacilityHit::Destroyed(0));
        assert_eq!(land_bombard_defense(5, &[barracks()], 0, &rules), 0);
    }

    #[test]
    fn land_hit_only_reports_a_building_that_acts_through_a_list() {
        let rules = Rules::CONQUESTS;
        let remote = FacilityFacts { in_city: false, ..walls() };
        assert_eq!(land_facility_hit(5, &[remote], &rules), FacilityHit::Reported(0));
        // A held building of any value stops pass 0 from falling through.
        assert_eq!(land_facility_hit(5, &[remote, barracks()], &rules), FacilityHit::Destroyed(1));
    }

    #[test]
    fn sea_hit_needs_a_positive_value_in_the_city() {
        assert_eq!(sea_facility_hit(&[barracks(), fortress()]), FacilityHit::Destroyed(1));
        assert_eq!(sea_facility_hit(&[barracks(), walls()]), FacilityHit::Nothing);
        assert_eq!(sea_facility_hit(&[]), FacilityHit::Nothing);
        // No size gate and no "reported" outcome.
        let remote = FacilityFacts { in_city: false, ..fortress() };
        assert_eq!(sea_facility_hit(&[remote]), FacilityHit::Nothing);
        let obsolete = FacilityFacts { obsolete: true, ..fortress() };
        assert_eq!(sea_facility_hit(&[obsolete]), FacilityHit::Nothing);
        let stronger = FacilityFacts { sea_defense: 12, ..fortress() };
        assert_eq!(sea_facility_hit(&[fortress(), stronger]), FacilityHit::Destroyed(1));
    }

    #[test]
    fn implicit_strength_is_the_signed_percentage_product() {
        assert_eq!(implicit_strength(16, 0, 0), 16);
        assert_eq!(implicit_strength(16, 50, 50), 32);
        assert_eq!(implicit_strength(8, 0, 0), 8);
        assert_eq!(implicit_strength(8, 50, 0), 12, "hills");
        assert_eq!(implicit_strength(0, 50, 50), 0);
        // Truncation toward zero, like the 0x51EB851F signed divide.
        assert_eq!(implicit_strength(3, 0, 0), 3);
        assert_eq!(implicit_strength(1, -50, 0), 0);
        assert_eq!(implicit_strength(3, -150, 0), -1, "-150 / 100 -> -1, not -2");
        assert_eq!(implicit_strength(-3, -150, 0), 1);
    }

    fn catapult_vs_walls() -> (WallsAttack, [FacilityFacts; 1]) {
        // Catapult: bombard 4, rate of fire 1, land domain; flat open tile.
        (
            WallsAttack { domain: Domain::Land, bombard: 4, rate_of_fire: 1, terrain_pct: 0, tile_pct: 0 },
            [walls()],
        )
    }

    #[test]
    fn wall_roll_odds_from_shipped_numbers() {
        // Catapult vs Walls (8): v = 8, 1024 * 8 / (4 + 8) = 682.
        assert_eq!(strike_odds(8, 0, 0, 4), Some(682));
        // On hills: v = 12, 1024 * 12 / 16 = 768.
        assert_eq!(strike_odds(8, 50, 0, 4), Some(768));
        // Battleship (bombard 8) vs Coastal Fortress (8): even.
        assert_eq!(strike_odds(8, 0, 0, 8), Some(512));
        // Artillery (12) vs Walls: 1024 * 8 / 20 = 409.
        assert_eq!(strike_odds(8, 0, 0, 12), Some(409));
    }

    #[test]
    fn wall_roll_skips_without_walls_for_aircraft_and_big_cities() {
        let rules = Rules::CONQUESTS;
        let (a, _) = catapult_vs_walls();
        let skipped = |attack: &WallsAttack, city: &CityWalls<'_>| {
            let mut rng = Rng::new(5);
            let step = walls_step(&mut rng, attack, city, &rules);
            assert_eq!(rng, Rng::new(5), "a skipped roll draws no die");
            step
        };
        let none = CityWalls { pop: 5, facilities: &[barracks()], wonder_count: 0 };
        assert_eq!(skipped(&a, &none), WallsStep::Skipped);
        let big = CityWalls { pop: 9, facilities: &[walls()], wonder_count: 0 };
        assert_eq!(skipped(&a, &big), WallsStep::Skipped);
        // Aircraft have no wall roll.
        let air = WallsAttack { domain: Domain::Air, ..a };
        let held = CityWalls { pop: 3, facilities: &[walls(), fortress()], wonder_count: 0 };
        assert_eq!(skipped(&air, &held), WallsStep::Skipped);
        // A ship still faces the fortress in a big city.
        let port = CityWalls { pop: 9, facilities: &[walls(), fortress()], wonder_count: 0 };
        let ship = WallsAttack { domain: Domain::Sea, bombard: 8, rate_of_fire: 2, ..a };
        assert_ne!(walls_step(&mut Rng::new(5), &ship, &port, &rules), WallsStep::Skipped);
    }

    #[test]
    fn wall_roll_hit_destroys_the_walls_with_one_die() {
        let rules = Rules::CONQUESTS;
        let (a, f) = catapult_vs_walls();
        let city = CityWalls { pop: 5, facilities: &f, wonder_count: 0 };
        let seed = seed_where_first(1, |r| r >= 682);
        let mut rng = Rng::new(seed);
        let step = walls_step(&mut rng, &a, &city, &rules);
        assert_eq!(step, WallsStep::Hit(FacilityHit::Destroyed(0)));
        assert!(!step.attacks_units(), "a destroyed wall ends the attack");
        let mut reference = Rng::new(seed);
        reference.discard(1);
        assert_eq!(rng, reference);
    }

    #[test]
    fn wall_roll_miss_ends_the_attack_and_draws_every_die() {
        let rules = Rules::CONQUESTS;
        let (a, f) = catapult_vs_walls();
        let two_shots = WallsAttack { rate_of_fire: 2, ..a };
        let city = CityWalls { pop: 5, facilities: &f, wonder_count: 0 };
        let seed = seed_where_first(2, |r| r < 682);
        let mut rng = Rng::new(seed);
        let step = walls_step(&mut rng, &two_shots, &city, &rules);
        assert_eq!(step, WallsStep::Missed);
        assert!(!step.attacks_units(), "the garrison is untouched when the walls hold");
        let mut reference = Rng::new(seed);
        reference.discard(2);
        assert_eq!(rng, reference);
        // No shots at all is a miss with no dice.
        let none = WallsAttack { rate_of_fire: 0, ..a };
        let mut rng = Rng::new(9);
        assert_eq!(walls_step(&mut rng, &none, &city, &rules), WallsStep::Missed);
        assert_eq!(rng, Rng::new(9));
    }

    #[test]
    fn wall_roll_stops_at_the_first_success_among_several_dice() {
        let rules = Rules::CONQUESTS;
        let (a, f) = catapult_vs_walls();
        let three = WallsAttack { rate_of_fire: 3, ..a };
        let city = CityWalls { pop: 5, facilities: &f, wonder_count: 0 };
        let seed = seed_where_first(1, |r| r >= 682);
        let mut rng = Rng::new(seed);
        walls_step(&mut rng, &three, &city, &rules);
        let mut reference = Rng::new(seed);
        reference.discard(1);
        assert_eq!(rng, reference, "one die, not three");
    }

    #[test]
    fn a_reported_building_lets_the_units_be_shot_at() {
        let rules = Rules::CONQUESTS;
        let (a, _) = catapult_vs_walls();
        let remote = [FacilityFacts { in_city: false, ..walls() }];
        let city = CityWalls { pop: 5, facilities: &remote, wonder_count: 0 };
        let seed = seed_where_first(1, |r| r >= 682);
        let step = walls_step(&mut Rng::new(seed), &a, &city, &rules);
        assert_eq!(step, WallsStep::Hit(FacilityHit::Reported(0)));
        assert!(step.attacks_units());
        assert!(WallsStep::Skipped.attacks_units());
        assert!(!WallsStep::Missed.attacks_units());
        assert!(!WallsStep::Hit(FacilityHit::Nothing).attacks_units());
        assert!(!WallsStep::Hit(FacilityHit::Destroyed(3)).attacks_units());
    }

    #[test]
    fn ship_wall_roll_uses_the_naval_defense_and_removes_the_fortress() {
        let rules = Rules::CONQUESTS;
        let f = [walls(), fortress()];
        let city = CityWalls { pop: 11, facilities: &f, wonder_count: 0 };
        let battleship = WallsAttack {
            domain: Domain::Sea,
            bombard: 8,
            rate_of_fire: 2,
            terrain_pct: 0,
            tile_pct: 0,
        };
        let seed = seed_where_first(1, |r| r >= 512);
        let step = walls_step(&mut Rng::new(seed), &battleship, &city, &rules);
        assert_eq!(step, WallsStep::Hit(FacilityHit::Destroyed(1)), "the Fortress, not the Walls");
    }
}
