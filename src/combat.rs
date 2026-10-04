//! Combat: odds, the animated fight, capture, conquest, healing, promotion.
//!
//! The numbers come from the reverse-engineered model in
//! `reverse-engineering/rust/src/combat.rs` (`civ3mapgen::combat`, read from
//! `Civ3Conquests.exe`): the round die is `rand(1024)`, the defender wins a
//! round when the roll is below `1024 * X / (X + Y)` (clamped to 1..=1023),
//! `X = defense * (100 + D)`, `Y = attack * (100 + P)`, the loser of a round
//! takes one point of damage, and a unit dies when `max_hp - damage <= 0`.
//! Terrain, structure and fortify terms are that module's; `BIQ` tables
//! (`EXPR`, `TERR`, `PRTO`) are read there too.
//!
//! What the exe model does not cover, and this file decides, is marked
//! **HYPOTHESIS** where it is defined: which defender of a stack fights,
//! promotion odds, healing rates, which clip plays in a round, when the blow
//! lands, capture and conquest details. The attack-refusal and result
//! messages are the game's own strings (`Conquests/Text/script.txt`).

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::sprite::Anchor;
use civ3mapgen::combat as exe;

use crate::cities::{Capital, City, CityView, Production};
use crate::features::{MessageBoard, post};
use crate::map::*;
use crate::production_prompt::ProductionPrompts;
use crate::rng::MapRng;
use crate::units::{Unit, UnitAnim, UnitArt, UnitType, def, spawn_unit, step_to};

// ---------------------------------------------------------------------------
// Experience, hit points, healing
// ---------------------------------------------------------------------------

/// Experience level, the row of `EXPR` in `conquests.biq`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, serde::Serialize, serde::Deserialize)]
pub enum Level {
    /// The lowest `EXPR` row; nothing demotes a unit yet.
    #[allow(dead_code)]
    Conscript,
    Regular,
    Veteran,
    Elite,
}

impl Level {
    /// Base hit points (`EXPR`: 2, 3, 4, 5).
    pub fn base_hp(self) -> i32 {
        exe::EXPERIENCE[self as usize].base_hp
    }

    pub fn name(self) -> &'static str {
        ["Conscript", "Regular", "Veteran", "Elite"][self as usize]
    }

    /// The level a promotion leads to.
    pub fn next(self) -> Option<Level> {
        match self {
            Level::Conscript => Some(Level::Regular),
            Level::Regular => Some(Level::Veteran),
            Level::Veteran => Some(Level::Elite),
            Level::Elite => None,
        }
    }

    /// The game's promotion text (`UNITPROMOTIONREG/VET/ELITE`) for a unit
    /// that just reached this level.
    fn promotion_text(self, unit: &str) -> String {
        match self {
            Level::Regular => format!("Our conscript {unit} becomes a Regular."),
            Level::Veteran => format!("Our Regular {unit} is now a Veteran."),
            _ => format!("Our Veteran {unit} is now Elite!"),
        }
    }
}

/// Maximum hit points, `0x5BE5B0`: the level's base plus the unit's `PRTO`
/// bonus, at least 1.
pub fn max_hp(level: Level, hp_bonus: i32) -> i32 {
    exe::max_hp(None, level.base_hp(), hp_bonus)
}

/// Where a damaged unit rests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rest {
    Field,
    City,
    /// A city with a Barracks.
    Barracks,
}

/// Where `u` is resting: in one of its own cities, with or without Barracks.
pub fn rest_of<'a>(u: &Unit, mut cities: impl Iterator<Item = &'a City>) -> Rest {
    match cities.find(|c| c.civ == u.civ && (c.x, c.y) == (u.x, u.y)) {
        Some(c) if c.buildings.contains(&Production::Barracks) => Rest::Barracks,
        Some(_) => Rest::City,
        None => Rest::Field,
    }
}

/// Damage left after a turn's rest.
///
/// The manual says units heal when they skip a turn, faster in cities, and
/// that a Barracks restores a unit fully in one turn (`Civilopedia`
/// `#BLDG_Barracks`). HYPOTHESIS: the field and city amounts (1 and 2
/// points); no healing number was recovered from the exe.
pub fn heal(damage: i32, rest: Rest) -> i32 {
    match rest {
        Rest::Field => (damage - 1).max(0),
        Rest::City => (damage - 2).max(0),
        Rest::Barracks => 0,
    }
}

// ---------------------------------------------------------------------------
// Odds and the duel
// ---------------------------------------------------------------------------

/// The `TERR` row a tile counts as, for [`exe::TERRAIN_DEFENSE_PCT`]:
/// mountains and hills override forest and jungle, which override the base.
/// HYPOTHESIS for the precedence (the clone layers relief and cover over a
/// base where Civ3 has one terrain type per tile).
pub(crate) use crate::map::terrain_row;

/// What a defending city adds to the odds: its size class and the best
/// defensive improvement in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hold {
    pub size: u8,
    /// Resisting citizens: any cancels the size bonus (`combat.md` 5).
    pub resisters: i32,
    /// Percent from Walls, Civil Defense, the Great Wall's gift
    /// (`exe::city_building_bonus`, the best one, not the sum).
    pub building_pct: i32,
}

impl Hold {
    /// A city with no defensive improvements.
    #[cfg(test)]
    pub fn bare(size: u8) -> Hold {
        Hold { size, resisters: 0, building_pct: 0 }
    }

    pub fn of(city: &City) -> Hold {
        // Walls count only up to a town (`BLDG +0x98` is 8 for them); the
        // others at every size. Obsolete ones and the gifts of wonders are
        // sorted out by `citycalc::active`.
        let list: Vec<exe::BuildingDefense> = crate::citycalc::active(city)
            .filter(|(_, b)| b.defense > 0)
            .map(|(p, b)| exe::BuildingDefense {
                pct: b.defense,
                town_limited: if p == Production::Walls { 8 } else { 0 },
                obsolete: false,
            })
            .collect();
        Hold {
            size: city.size(),
            resisters: crate::resistance::resisters(city),
            building_pct: exe::city_building_bonus(i32::from(city.size()), false, &list, &exe::Rules::CONQUESTS),
        }
    }
}

/// The defender-side percentage `D` of the odds formula: the terrain, a
/// city's size bonus and improvements, and the fortify bonus. A fortified unit counts only
/// with movement left (`0x4A0F79`); a defender's moves refill when its own
/// civ's turn begins, so for the hotseat it counts as having them.
pub fn defense_pct(map: &GameMap, d: &Unit, hold: Option<Hold>) -> i32 {
    let rules = exe::Rules::CONQUESTS;
    let Some(tile) = map.get(d.x, d.y) else {
        return 0;
    };
    let structure = match hold {
        Some(h) => exe::Structure::City {
            size: i32::from(h.size),
            resisters: h.resisters,
            building_pct: h.building_pct,
        },
        None => exe::Structure::from_overlay(tile.fortress, tile.barricade),
    };
    exe::terrain_term(exe::TERRAIN_DEFENSE_PCT[terrain_row(tile)], false, &rules)
        + exe::tile_term(structure, false, &rules)
        + exe::fortify_term(true, false, d.fortified && !d.sentry, 1, &rules)
}

/// `1024 * P(defender wins a round)` for `att` against `dfn`, `0x4A0ED0`.
/// The non-barbarian side's term in a fight with barbarians (`combat.md`
/// 4.5): its difficulty's `DIFF +0x64`, plus 100 with the Great Wall.
fn barbarian_term(att: &Unit, dfn: &Unit, for_attacker: bool) -> i32 {
    use crate::civs::is_barbarian;
    let (me, them) = if for_attacker { (att.civ, dfn.civ) } else { (dfn.civ, att.civ) };
    if !is_barbarian(them) || is_barbarian(me) {
        return 0;
    }
    let wall = crate::naval::wonders(me) & crate::roster::wonder::DOUBLE_VS_BARBARIANS != 0;
    exe::barbarian_term(exe::DIFF_VS_BARBARIAN_PCT[crate::research::DIFFICULTY], wall)
}

pub fn round_odds(map: &GameMap, att: &Unit, dfn: &Unit, hold: Option<Hold>) -> i32 {
    let barb_att = barbarian_term(att, dfn, true);
    let barb_def = barbarian_term(att, dfn, false);
    exe::defender_round_odds(&exe::OddsInput {
        att_strength: exe::attack_strength(&[], att.attack()),
        att_army_bonus: att.army_bonus(false),
        att_pct: exe::amphibious_term(&exe::AmphibiousCheck {
            ability_amphibious: def(att.utype).abilities & (1 << 6) != 0,
            attack_strength: att.attack(),
            status_bit2: att.attacked,
            ability_blitz: def(att.utype).abilities & crate::roster::ability::BLITZ != 0,
            land_unit: def(att.utype).class == 0,
            target_is_water: map.get(dfn.x, dfn.y).is_some_and(|t| crate::improvements::is_water_base(t.base)),
            origin_is_water: map.get(att.x, att.y).is_some_and(|t| crate::improvements::is_water_base(t.base)),
        }) + barb_att,
        def_strength: exe::defense_strength(&[], dfn.defense(), false),
        def_army_bonus: dfn.army_bonus(true),
        def_pct: defense_pct(map, dfn, hold) + barb_def
            + if crate::rivers::crossed(map, (dfn.x, dfn.y), (att.x, att.y)) { exe::Rules::CONQUESTS.river_pct } else { 0 },
    })
    .expect("an attacker has attack above 0")
}

/// The combat dice: the exe's gameplay instance of the shared `Random` class
/// (`0xA526B4`), the same LCG as the map generator. Seeded from the map seed
/// so a given map plays the same way.
#[derive(Resource)]
pub struct CombatRng(pub MapRng);

/// Roll a fight round by round: `true` when the attacker won the round.
///
/// The exe's duel with a round log for animated playback. Retreat draws
/// occur after the hit, including a defender's roll when its escape is blocked.
fn roll_duel(
    rng: &mut MapRng,
    odds: i32,
    mut att: exe::Fighter,
    mut dfn: exe::Fighter,
    flags: exe::RetreatFlags,
    defender_can_step_back: bool,
) -> (Vec<bool>, exe::Outcome) {
    let mut rounds = Vec::new();
    loop {
        let attacker_won = rng.below(exe::ROUND_DIE) >= odds;
        rounds.push(attacker_won);
        if attacker_won {
            dfn.damage += 1;
            if dfn.remaining() <= 0 {
                return (rounds, exe::Outcome::AttackerWon);
            }
            if flags.defender && dfn.owned && dfn.remaining() == 1 && att.remaining() > 1 {
                let die = (att.retreat_pct + exe::RETREAT_MARGIN) as u32;
                if rng.below(die) < dfn.retreat_pct && defender_can_step_back {
                    return (rounds, exe::Outcome::DefenderRetreated);
                }
            }
        } else {
            att.damage += 1;
            if att.remaining() <= 0 {
                return (rounds, exe::Outcome::DefenderWon);
            }
            if flags.attacker && att.owned && att.remaining() == 1 && dfn.remaining() > 1 {
                let die = (dfn.retreat_pct + exe::RETREAT_MARGIN) as u32;
                if rng.below(die) < att.retreat_pct {
                    return (rounds, exe::Outcome::AttackerRetreated);
                }
            }
        }
    }
}

fn fighter(u: &Unit) -> exe::Fighter {
    exe::Fighter {
        max_hp: u.max_hp(),
        damage: u.damage,
        retreat_pct: exe::EXPERIENCE[u.level as usize].retreat_pct,
        // Barbarians never retreat (`combat.md` 7, `D.owner != 0`).
        owned: !crate::civs::is_barbarian(u.civ),
    }
}

/// HYPOTHESIS: `0x5BFB60` is still unread. Step directly away from the
/// attacker if that tile is passable and contains no foreign unit or city.
fn retreat_tile<'a>(
    map: &GameMap,
    att: &Unit,
    dfn: &Unit,
    mut units: impl Iterator<Item = &'a Unit>,
    mut cities: impl Iterator<Item = &'a City>,
) -> Option<(i32, i32)> {
    let mut dx = (dfn.x - att.x).rem_euclid(map.w);
    if dx > map.w / 2 { dx -= map.w; }
    let to = (map.wrap_x(dfn.x + dx.signum()), dfn.y + (dfn.y - att.y).signum());
    if crate::units::entry_cost(map, dfn, (dfn.x, dfn.y), to, &[]).is_none()
        || units.any(|u| (u.x, u.y) == to && u.civ != dfn.civ)
        || cities.any(|c| (c.x, c.y) == to && c.civ != dfn.civ)
    {
        return None;
    }
    Some(to)
}

/// Exact chance that the defender survives, by dynamic programming over both
/// hit-point pools, at `odds / 1024` per round.
pub fn defender_win_chance(odds: i32, att_hp: i32, def_hp: i32) -> f64 {
    let p = odds as f64 / exe::ROUND_DIE as f64;
    let (na, nd) = (att_hp.max(0) as usize, def_hp.max(0) as usize);
    // f[a][d]: chance the defender wins with a and d hit points left.
    let mut f = vec![vec![0.0f64; nd + 1]; na + 1];
    // The attacker is dead: the defender wins if it still stands.
    f[0].iter_mut().skip(1).for_each(|v| *v = 1.0);
    for a in 1..=na {
        for d in 1..=nd {
            f[a][d] = p * f[a - 1][d] + (1.0 - p) * f[a][d - 1];
        }
    }
    f[na][nd]
}

/// The unit of a stack that defends: the one most likely to win this fight.
/// HYPOTHESIS: the exe re-runs the odds function per candidate (`ai.md`
/// names the front-unit reselection) but the selector itself is unread;
/// ranking by exact win chance, then hit points, then age is the natural
/// reading.
pub fn pick_defender(
    map: &GameMap,
    att: &Unit,
    hold: Option<Hold>,
    candidates: &[(Entity, Unit)],
) -> Option<Entity> {
    candidates
        .iter()
        .filter(|(_, u)| u.carrier.is_none() && u.defense() > 0)
        .max_by(|(ea, a), (eb, b)| {
            let rank = |u: &Unit| {
                let odds = round_odds(map, att, u, hold);
                (defender_win_chance(odds, att.hp(), u.hp()), u.hp())
            };
            let (ra, rb) = (rank(a), rank(b));
            ra.0.total_cmp(&rb.0).then(ra.1.cmp(&rb.1)).then(eb.cmp(ea))
        })
        .map(|(e, _)| *e)
}

// ---------------------------------------------------------------------------
// Health bars
// ---------------------------------------------------------------------------

#[derive(Component)]
pub struct HealthBar;

#[derive(Component)]
pub struct HealthFill;

const BAR_H: f32 = 30.0;
const BAR_X: f32 = -26.0;

/// Civ3 draws a vertical bar beside a damaged unit and during a fight:
/// green, yellow below two thirds, red below a third (`MANUAL`).
pub fn spawn_health_bar(commands: &mut Commands, unit: Entity) {
    commands.entity(unit).with_children(|p| {
        p.spawn((
            Sprite::from_color(Color::srgb(0.05, 0.05, 0.05), Vec2::new(6.0, BAR_H + 2.0)),
            Anchor::BOTTOM_CENTER,
            Transform::from_xyz(BAR_X, -3.0, 0.02),
            Visibility::Hidden,
            HealthBar,
        ));
        p.spawn((
            Sprite::from_color(Color::srgb(0.2, 0.85, 0.2), Vec2::new(4.0, BAR_H)),
            Anchor::BOTTOM_CENTER,
            Transform::from_xyz(BAR_X, -2.0, 0.03),
            Visibility::Hidden,
            HealthFill,
        ));
    });
}

/// HYPOTHESIS for the cut-offs ("approximately two-thirds / one-third" in the
/// manual): yellow at two thirds of full or less, red at a third or less.
fn bar_color(hp: i32, max: i32) -> Color {
    if hp * 3 > max * 2 {
        Color::srgb(0.2, 0.85, 0.2)
    } else if hp * 3 > max {
        Color::srgb(0.95, 0.85, 0.1)
    } else {
        Color::srgb(0.9, 0.15, 0.1)
    }
}

pub fn sync_health_bars(
    active: Res<ActiveCombat>,
    units: Query<&Unit>,
    mut backs: Query<(&ChildOf, &mut Visibility), (With<HealthBar>, Without<HealthFill>)>,
    mut fills: Query<(&ChildOf, &mut Visibility, &mut Sprite), With<HealthFill>>,
) {
    let fighting = active
        .0
        .as_ref()
        .map(Sequence::fighters)
        .unwrap_or_default();
    let shown = |e: Entity| {
        units
            .get(e)
            .is_ok_and(|u| u.carrier.is_none() && (u.damage > 0 || fighting.contains(&e)))
    };
    for (parent, mut vis) in backs.iter_mut() {
        *vis = if shown(parent.parent()) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    for (parent, mut vis, mut sprite) in fills.iter_mut() {
        let Ok(u) = units.get(parent.parent()) else {
            continue;
        };
        let max = u.max_hp();
        let hp = u.hp().clamp(0, max);
        *vis = if shown(parent.parent()) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        sprite.color = bar_color(hp, max);
        sprite.custom_size = Some(Vec2::new(4.0, BAR_H * hp as f32 / max as f32));
    }
}

// ---------------------------------------------------------------------------
// The sequencer
// ---------------------------------------------------------------------------

/// A unit steps onto a tile held by another civ: resolve it.
#[derive(Message, Clone, Copy)]
pub struct AttackOrder {
    pub attacker: Entity,
    pub to: (i32, i32),
}

/// Playback speed of the combat clips; 1.0 is the FLC speed.
/// `CIV3_COMBAT_SPEED` raises it for unattended runs.
#[derive(Resource)]
pub struct CombatSpeed(pub f32);

impl Default for CombatSpeed {
    fn default() -> Self {
        let speed = std::env::var("CIV3_COMBAT_SPEED")
            .ok()
            .and_then(|s| s.parse::<f32>().ok())
            .filter(|s| *s > 0.0)
            .unwrap_or(1.0);
        CombatSpeed(speed)
    }
}

/// The fight or capture being played; input and movement wait for it.
#[derive(Resource, Default)]
pub struct ActiveCombat(pub Option<Sequence>);

/// Run condition: nothing is being played.
pub fn idle(active: Res<ActiveCombat>) -> bool {
    active.0.is_none()
}

/// How much faster a fight nobody can see is played.
const UNSEEN_SPEEDUP: f32 = 40.0;

/// A round with no attack clip lasts this long (seconds).
const BARE_ROUND: f32 = 0.5;
/// The death/victory beat is at least this long.
const MIN_FINALE: f32 = 0.5;
/// The blow lands this far into the winning clip. HYPOTHESIS: the exe's
/// timing is unread; the middle of the swing is where the art connects.
const STRIKE_AT: f32 = 0.5;

enum Kind {
    Fight {
        defender: Entity,
        support: Option<SupportShot>,
        /// One per round, `true` when the attacker won it.
        rounds: Vec<bool>,
        outcome: exe::Outcome,
        retreat_to: Option<(i32, i32)>,
        round: usize,
        /// The defender was the tile's only unit.
        alone: bool,
    },
    Capture {
        victims: Vec<Entity>,
        city: Option<Entity>,
    },
}

#[derive(PartialEq)]
enum Phase {
    Support,
    Round,
    Finale,
}

struct SupportShot {
    shooter: Entity,
    result: exe::DefensiveBombard,
}

fn domain(t: UnitType) -> exe::Domain {
    match def(t).class { 1 => exe::Domain::Sea, 2 => exe::Domain::Air, _ => exe::Domain::Land }
}

/// `0x4A1AE0`: one supporting shooter, excluding the actual defender.
pub(crate) fn supporting_shooter(att: &Unit, defender: Entity, stack: &[(Entity, Unit)]) -> Option<Entity> {
    let victim = domain(att.utype);
    if !exe::defensive_bombard_victim_ok(victim, def(att.utype).defense, att.hp()) {
        return None;
    }
    let candidates: Vec<_> = stack.iter().map(|(e, u)| exe::ShooterCandidate {
        is_defender: *e == defender, carried_by_defender: u.carrier.is_some(),
        domain: domain(u.utype), bombard_strength: def(u.utype).bombard,
        has_ability_3: def(u.utype).abilities & (1 << 3) != 0,
        already_fired: u.defensive_fired,
    }).collect();
    exe::pick_defensive_shooter(victim, &candidates).map(|i| stack[i].0)
}

pub(crate) fn support_odds(shooter: &Unit, victim: &Unit) -> i32 {
    // Raw bombard mode has no terrain, tile or fortification terms.
    exe::defender_round_odds(&exe::OddsInput {
        att_strength: def(shooter.utype).bombard, att_army_bonus: 0, att_pct: 0,
        def_strength: def(victim.utype).defense, def_army_bonus: 0, def_pct: 0,
    }).expect("supporting shooter has positive bombard strength")
}

pub struct Sequence {
    attacker: Entity,
    kind: Kind,
    phase: Phase,
    /// Seconds into the phase, at the playback speed.
    t: f32,
    dur: f32,
    struck: bool,
    sounds: Vec<(f32, Handle<AudioSource>)>,
    /// Played at a blur and silently: the computer fighting out of sight.
    fast: bool,
}

impl Sequence {
    /// The two units whose bars show during a fight.
    pub fn fighters(&self) -> Vec<Entity> {
        match &self.kind {
            Kind::Fight { defender, .. } => vec![self.attacker, *defender],
            Kind::Capture { .. } => vec![self.attacker],
        }
    }
}

/// Everything a result touches besides units.
#[derive(SystemParam)]
pub struct Realm<'w, 's> {
    cities: Query<'w, 's, (Entity, &'static mut City)>,
    capital: ResMut<'w, Capital>,
    prompts: ResMut<'w, ProductionPrompts>,
    view: ResMut<'w, CityView>,
    board: ResMut<'w, MessageBoard>,
    civs: Res<'w, crate::civs::Civilizations>,
    diplomacy: ResMut<'w, crate::diplomacy::Diplomacy>,
    treasury: ResMut<'w, crate::cities::Treasury>,
}

impl Realm<'_, '_> {
    /// An enemy city falls to `by`. HYPOTHESIS for the details: the manual
    /// says a captured city can be razed or kept, Small Wonders are always
    /// lost and "plundered gold" comes with it, and `POPDESTROYED` says
    /// citizens die; none of that was read from the exe. The clone keeps the
    /// city, loses one citizen (down to a minimum of 1), loses the build
    /// queue and the Palace, and does not plunder or raze.
    fn conquer(&mut self, e: Entity, by: usize, rng: &mut MapRng) {
        if crate::civs::is_barbarian(by) {
            self.raid(e, rng);
            return;
        }
        let Ok((_, mut city)) = self.cities.get_mut(e) else {
            return;
        };
        let old = city.civ;
        // The victim books 16 incident points per capture (`capture.md` 4
        // step 5): they weigh on both sides' war weariness every turn.
        self.diplomacy.incident(by, old, 16);
        // The human hears of captures that concern them; the computer's
        // wars with itself go unannounced.
        let viewer = self.civs.viewer();
        // A city with a citizen of the taker's race or a stake of its own
        // loses nobody (`capture.md` 4); otherwise one citizen dies.
        let kin = crate::flip::nationals(&city, by) > 0 || city.stakes[by] > 0;
        let shrank = !kin && city.size() > 1;
        let was_capital = self.capital.0[old] == Some(e);
        if shrank {
            // `0x5642F1` removes population before the building/owner
            // transfer, so food retention sees the old owner's Granary.
            city.lose_population(1, None, rng);
        }
        let lost = crate::flip::transfer(&mut city, by, was_capital, true, false, || rng.below(4) == 0);
        let _ = lost;
        drop(city);
        // `0x4BB090`: the foreigners may resist the new owner. City counts
        // are taken after the transfer.
        let mut nations = crate::resistance::Nations::current();
        for civ in 0..crate::civs::CIV_COUNT {
            nations.cities[civ] = self.cities.iter().filter(|(_, c)| c.civ == civ).count() as i32;
        }
        nations.war[by][old] = true;
        let Ok((_, mut city)) = self.cities.get_mut(e) else {
            return;
        };
        let resisting = crate::resistance::seed(&mut city, by, false, &nations, rng);
        if by == viewer {
            let mut text = if shrank {
                format!("We captured {}! Some of its citizens were killed.", city.name)
            } else {
                format!("We captured {}!", city.name)
            };
            // `RESISTERS`
            if resisting > 0 {
                text += &format!(
                    " There {} {} {} in {}. We should garrison {} with strong units to quell the resistance.",
                    if resisting == 1 { "is" } else { "are" },
                    resisting,
                    if resisting == 1 { "resister" } else { "resisters" },
                    city.name,
                    city.name,
                );
            }
            post(&mut self.board, text);
        } else if old == viewer {
            post(
                &mut self.board,
                format!("{} has fallen to the {}!", city.name, crate::civs::name(by)),
            );
        }
        city.queue.clear();
        city.food = 0;
        city.shields = 0;
        if self.capital.0[old] == Some(e) {
            self.capital.0[old] = None;
        }
        self.prompts.forget_city(e);
        if self.view.0 == Some(e) {
            self.view.0 = None;
        }
    }
}

impl Realm<'_, '_> {
    /// A barbarian "capture" is a plunder (`combat.md` 14.2, `0x563410`):
    /// the city keeps its owner and suffers the first loss that applies.
    /// The caller removes the raider.
    fn raid(&mut self, e: Entity, rng: &mut MapRng) {
        use civ3mapgen::capture::{RaidLoss, raid_loot, raid_loss};
        let cities_owned = |civ: usize, q: &Query<(Entity, &mut City)>| q.iter().filter(|(_, c)| c.civ == civ).count() as i32;
        let Ok((_, city)) = self.cities.get(e) else { return };
        let owner = city.civ;
        let n = cities_owned(owner, &self.cities).max(1);
        let walls = crate::economy::size_class(city.size()) == 0 && walls_row(city).is_some();
        let loss = raid_loss(walls, i32::from(city.shields), i32::from(city.nationals(owner)), self.treasury.0[owner] as i32);
        let Ok((_, mut city)) = self.cities.get_mut(e) else { return };
        let text = match loss {
            RaidLoss::Walls => {
                if let Some(i) = walls_row(&city) {
                    city.buildings.remove(i);
                }
                format!("Barbarians have destroyed the walls of {}!", city.name)
            }
            RaidLoss::Stock => {
                city.shields = 0;
                format!("Barbarians raid {}! Our work on {} has been destroyed!", city.name, city.production.name())
            }
            RaidLoss::Citizen => {
                // `0x4BA230(B, 1, race, 0)`: only an owner's national dies.
                city.lose_population(1, Some(crate::civs::roster_index(owner) as i32), rng);
                format!("Barbarians raid {}! Citizens have been killed.", city.name)
            }
            RaidLoss::Gold => {
                let loot = raid_loot(self.treasury.0[owner] as i32, n).max(0) as u32;
                self.treasury.0[owner] = self.treasury.0[owner].saturating_sub(loot);
                format!("Barbarians plunder {}! They take {loot} gold.", city.name)
            }
        };
        if owner == self.civs.viewer() {
            post(&mut self.board, text);
        }
    }
}

/// The building `0x4C1320(city, 0)` removes: the city's best land bombard
/// defense, here the improvement with the largest defense bonus.
fn walls_row(city: &City) -> Option<usize> {
    city.buildings
        .iter()
        .enumerate()
        .filter_map(|(i, b)| b.bldg().filter(|d| d.defense > 0).map(|d| (i, d.defense)))
        .max_by_key(|&(_, d)| d)
        .map(|(i, _)| i)
}

/// Facing from `from` toward the adjacent tile `to`, across the map seam.
pub(crate) fn face(map: &GameMap, from: (i32, i32), to: (i32, i32)) -> usize {
    let mut dx = (to.0 - from.0).rem_euclid(map.w);
    if dx > map.w / 2 {
        dx -= map.w;
    }
    crate::units::facing_for_step(dx, to.1 - from.1)
}

/// Start `slot` on `e`.
fn play(units: &mut Query<(Entity, &mut Unit)>, e: Entity, slot: &'static str, looped: bool) {
    if let Ok((_, mut u)) = units.get_mut(e) {
        u.anim = UnitAnim::Held {
            slot,
            t: 0.0,
            looped,
        };
    }
}

fn utype_of(units: &Query<(Entity, &mut Unit)>, e: Entity) -> Option<UnitType> {
    units.get(e).ok().map(|(_, u)| u.utype)
}

/// The attack clip a unit swings in round `n`: its ATTACK slots in turn.
/// HYPOTHESIS: the exe's slot choice is unread.
fn attack_slot(art: &UnitArt, t: UnitType, n: usize) -> Option<&'static str> {
    let slots: Vec<&'static str> = ["ATTACK1", "ATTACK2", "ATTACK3"]
        .into_iter()
        .filter(|s| art.clip(t, s).is_some())
        .collect();
    (!slots.is_empty()).then(|| slots[n % slots.len()])
}

/// Set up the clips of the round `seq` is on.
fn begin_round(seq: &mut Sequence, art: &UnitArt, units: &mut Query<(Entity, &mut Unit)>) {
    seq.phase = Phase::Round;
    let Kind::Fight {
        defender,
        rounds,
        round,
        ..
    } = &seq.kind
    else {
        return;
    };
    let (winner, loser) = if rounds[*round] {
        (seq.attacker, *defender)
    } else {
        (*defender, seq.attacker)
    };
    let (Some(wt), Some(lt)) = (utype_of(units, winner), utype_of(units, loser)) else {
        return;
    };
    // Both sides keep swinging for the whole fight, whoever wins the round,
    // as in Civ3. The round lasts as long as the longer swing, and the blow
    // lands partway through; the winner only decides who loses the hit
    // point. A unit with no attack clip stands.
    seq.t = 0.0;
    seq.struck = false;
    seq.dur = 0.0;
    seq.sounds.clear();
    for (e, t) in [(winner, wt), (loser, lt)] {
        let slot = attack_slot(art, t, *round);
        let clip = slot.and_then(|s| art.clip(t, s));
        seq.dur = seq.dur.max(clip.map_or(BARE_ROUND, |c| c.duration()));
        if let Some(c) = clip {
            seq.sounds.extend(c.sounds.iter().cloned());
        }
        play(units, e, slot.unwrap_or("DEFAULT"), true);
    }
}

fn begin_support(seq: &mut Sequence, art: &UnitArt, units: &mut Query<(Entity, &mut Unit)>) {
    let Kind::Fight { defender, support: Some(shot), .. } = &seq.kind else { return };
    let mut beat = (BARE_ROUND, Vec::new());
    play(units, seq.attacker, "DEFAULT", true);
    play(units, *defender, "DEFAULT", true);
    cue(art, units, shot.shooter, "ATTACK1", &mut beat);
    seq.phase = Phase::Support;
    seq.t = 0.0;
    seq.struck = false;
    seq.dur = beat.0;
    seq.sounds = beat.1;
}

/// One unit's closing clip: play `slot` if it has one (lengthening the beat
/// to fit and collecting its sounds), else stand and wait.
fn cue(
    art: &UnitArt,
    units: &mut Query<(Entity, &mut Unit)>,
    e: Entity,
    slot: &'static str,
    beat: &mut (f32, Vec<(f32, Handle<AudioSource>)>),
) {
    let Some(t) = utype_of(units, e) else {
        return;
    };
    match art.clip(t, slot) {
        Some(c) => {
            beat.0 = beat.0.max(c.duration());
            beat.1.extend(c.sounds.iter().cloned());
            play(units, e, slot, false);
        }
        None => play(units, e, "DEFAULT", true),
    }
}

/// Set up the closing beat: death and victory, or the captives' surrender.
fn begin_finale(seq: &mut Sequence, art: &UnitArt, units: &mut Query<(Entity, &mut Unit)>) {
    let mut beat = (MIN_FINALE, Vec::new());
    match &seq.kind {
        Kind::Fight {
            defender, outcome, ..
        } => {
            if matches!(outcome, exe::Outcome::AttackerRetreated | exe::Outcome::DefenderRetreated) {
                play(units, seq.attacker, "DEFAULT", true);
                play(units, *defender, "DEFAULT", true);
            } else {
                let (winner, loser) = if *outcome == exe::Outcome::AttackerWon {
                    (seq.attacker, *defender)
                } else {
                    (*defender, seq.attacker)
                };
                cue(art, units, loser, "DEATH", &mut beat);
                cue(art, units, winner, "VICTORY", &mut beat);
            }
        }
        Kind::Capture { victims, .. } => {
            play(units, seq.attacker, "DEFAULT", true);
            for v in victims {
                cue(art, units, *v, "CAPTURE", &mut beat);
            }
        }
    }
    seq.phase = Phase::Finale;
    seq.t = 0.0;
    seq.dur = beat.0;
    seq.sounds = beat.1;
}

/// Look at the tile an attacker stepped toward and decide what happens.
pub fn start_attacks(
    mut orders: MessageReader<AttackOrder>,
    mut active: ResMut<ActiveCombat>,
    map: Res<GameMap>,
    art: Res<UnitArt>,
    mut rng: ResMut<CombatRng>,
    mut units: Query<(Entity, &mut Unit)>,
    mut realm: Realm,
    mut commands: Commands,
) {
    let Some(order) = orders.read().next().copied() else {
        return;
    };
    if active.0.is_some() {
        return;
    }
    let Ok((_, att)) = units.get(order.attacker) else {
        return;
    };
    let mut att = att.clone();
    let from = (att.x, att.y);
    let at = order.to;
    let city = realm
        .cities
        .iter()
        .find(|(_, c)| (c.x, c.y) == at && c.civ != att.civ)
        .map(|(e, c)| (e, Hold::of(c)));
    let stack: Vec<(Entity, Unit)> = units
        .iter()
        .filter(|(_, u)| u.carrier.is_none() && (u.x, u.y) == at && u.civ != att.civ)
        .map(|(e, u)| (e, u.clone()))
        .collect();
    if stack.is_empty() && city.is_none() {
        return; // the tile emptied since the step was planned
    }

    // The exe refuses some attacks before they start (`0x5B5CD0`; the
    // strings are `NONCOMBATANT`, `COMBATCONQUER`, `BLITZLIMIT`).
    let refusal = if att.moves == 0 {
        Some("This unit has no movement left.")
    } else if att.attack() == 0 {
        Some(if stack.is_empty() {
            "Only combat units can capture cities and improvements."
        } else {
            "Non-combat units may not attack."
        })
    } else if def(att.utype).class == 0 && map.get(at.0, at.1).is_some_and(|t| !crate::improvements::is_water_base(t.base))
        && crate::units::entry_cost(&map, &att, from, at, &[]).is_none() {
        Some("This unit cannot enter that terrain.")
    } else if !crate::naval::can_attack_from(&map, &att) {
        Some("This unit cannot attack from water.")
    } else if att.attacked && def(att.utype).abilities & crate::roster::ability::BLITZ == 0 {
        Some("This unit may not attack more than once.")
    } else {
        None
    };
    if let Some(text) = refusal {
        if let Ok((_, mut u)) = units.get_mut(order.attacker) {
            u.path.clear();
            u.exploring = false;
        }
        if att.civ == realm.civs.viewer() {
            post(&mut realm.board, text);
        }
        return;
    }

    // An undefended city is entered like any tile and falls.
    if stack.is_empty() {
        let (city, _) = city.expect("an empty tile with no city returned above");
        let cost = crate::units::entry_cost(&map, &att, from, at, &[]).unwrap_or(MP);
        if crate::civs::is_barbarian(att.civ) {
            // The raider plunders and is removed (`0x5638D2`).
            realm.conquer(city, att.civ, &mut rng.0);
            commands.entity(order.attacker).despawn();
            return;
        }
        if let Ok((_, mut u)) = units.get_mut(order.attacker) {
            u.moves -= cost.min(u.moves);
            u.rested = false;
            advance_attacker(&map, &mut u, at);
        }
        realm.conquer(city, att.civ, &mut rng.0);
        return;
    }

    let hold = city.map(|(_, h)| h);
    let kind = match pick_defender(&map, &att, hold, &stack) {
        Some(d) => {
            let support = supporting_shooter(&att, d, &stack).map(|e| {
                let shooter = &stack.iter().find(|(entity, _)| *entity == e).unwrap().1;
                let mut hp = fighter(&att);
                let result = exe::defensive_bombard(rng.0.reference(), support_odds(shooter, &att), &mut hp);
                // The melee dice must start with the shot's damage already
                // accounted for. Playback applies that damage at the blow.
                att.damage = hp.damage;
                if let Ok((_, mut u)) = units.get_mut(e) {
                    u.defensive_fired = true;
                    u.rested = false;
                }
                SupportShot { shooter: e, result }
            });
            let dfn = &stack
                .iter()
                .find(|(e, _)| *e == d)
                .expect("picked from the stack")
                .1;
            let odds = round_odds(&map, &att, dfn, hold);
            let flags = exe::RetreatFlags::new(
                i32::from(def(att.utype).moves * MP),
                i32::from(def(dfn.utype).moves * MP),
                city.is_some(),
                &exe::Rules::CONQUESTS,
            );
            let retreat_to = if flags.defender {
                retreat_tile(&map, &att, dfn, units.iter().map(|(_, u)| u), realm.cities.iter().map(|(_, c)| c))
            } else { None };
            let (rounds, outcome) = roll_duel(&mut rng.0, odds, fighter(&att), fighter(dfn), flags, retreat_to.is_some());
            Kind::Fight {
                defender: d,
                support,
                rounds,
                outcome,
                retreat_to,
                round: 0,
                alone: stack.len() == 1 && city.is_none(),
            }
        }
        // Nobody there can defend: take what stands there.
        None => Kind::Capture {
            victims: stack.iter().map(|(e, _)| *e).collect(),
            city: city.map(|(e, _)| e),
        },
    };

    // An attack spends one move and ends every standing order.
    if let Ok((_, mut u)) = units.get_mut(order.attacker) {
        u.moves -= MP.min(u.moves);
        u.attacked = true;
        u.rested = false;
        u.fortified = false;
        u.sentry = false;
        u.exploring = false;
        u.work = None;
        u.path.clear();
        u.facing = face(&map, from, at);
    }
    for (e, _) in &stack {
        if let Ok((_, mut u)) = units.get_mut(*e) {
            // `0x4A56F1` preserves the fortified order when attacked.
            if u.sentry { u.fortified = false; }
            u.sentry = false;
            u.facing = face(&map, at, from);
        }
    }

    let mut seq = Sequence {
        attacker: order.attacker,
        kind,
        phase: Phase::Round,
        t: 0.0,
        dur: BARE_ROUND,
        struck: false,
        sounds: Vec::new(),
        fast: crate::civs::is_ai(att.civ) && {
            let lit = |(x, y): (i32, i32)| map.get(x, y).is_some_and(|t| t.visible);
            crate::civs::ai_fast() || (!lit(from) && !lit(at))
        },
    };
    match seq.kind {
        Kind::Fight { support: Some(_), .. } => begin_support(&mut seq, &art, &mut units),
        Kind::Fight { .. } => begin_round(&mut seq, &art, &mut units),
        Kind::Capture { .. } => begin_finale(&mut seq, &art, &mut units),
    }
    active.0 = Some(seq);
}

/// A victor's promotion roll (`0x5BEF00`, `combat.md` 6.2): the die is
/// `[2, 4, 8]` by level, halved for a Militaristic civ; it promotes on a
/// zero. A unit that already failed this turn is promoted without a roll,
/// and a failed roll marks it. Elite never rolls.
pub fn promote(rng: &mut MapRng, u: &mut Unit, loser_is_barbarian: bool) -> bool {
    let Some(next) = u.level.next() else {
        return false;
    };
    let militaristic = civ3mapgen::economy::has_trait(
        crate::cities::traits(u.civ),
        civ3mapgen::economy::trait_bit::MILITARISTIC,
    );
    let Some(die) = exe::promotion_die(u.level as i32, loser_is_barbarian, militaristic) else {
        return false;
    };
    if u.failed_promotion || rng.below(die) == 0 {
        u.level = next;
        return true;
    }
    u.failed_promotion = true;
    false
}

/// Advance the fight or capture on screen; apply its result when it ends.
pub fn run_combat(
    mut commands: Commands,
    map: Res<GameMap>,
    time: Res<Time>,
    speed: Res<CombatSpeed>,
    art: Res<UnitArt>,
    mut active: ResMut<ActiveCombat>,
    mut rng: ResMut<CombatRng>,
    mut units: Query<(Entity, &mut Unit)>,
    mut realm: Realm,
) {
    let Some(seq) = active.0.as_mut() else {
        return;
    };
    let needed = match &seq.kind {
        Kind::Fight { defender, .. } => vec![seq.attacker, *defender],
        Kind::Capture { .. } => vec![seq.attacker],
    };
    if needed.iter().any(|e| units.get(*e).is_err()) {
        active.0 = None; // a unit vanished under the sequence
        return;
    }
    seq.t += time.delta_secs() * speed.0 * if seq.fast { UNSEEN_SPEEDUP } else { 1.0 };
    let now = seq.t;
    let silent = seq.fast;
    seq.sounds.retain(|(at, clip)| {
        if *at > now {
            return true;
        }
        if silent {
            return false;
        }
        commands.spawn((AudioPlayer(clip.clone()), PlaybackSettings::DESPAWN));
        false
    });
    match seq.phase {
        Phase::Support => {
            let Kind::Fight { support: Some(shot), .. } = &seq.kind else { return };
            if !seq.struck && seq.t >= seq.dur * STRIKE_AT {
                let shooter_civ = units.get(shot.shooter).ok().map(|(_, u)| u.civ);
                if let Ok((_, mut u)) = units.get_mut(seq.attacker) {
                    if shot.result.hit { u.damage += 1; }
                    if let Some(shooter_civ) = shooter_civ {
                        if shot.result.left_at_one_hp {
                            realm.diplomacy.note_attack(shooter_civ, 1 << u.civ);
                            realm.diplomacy.incident(shooter_civ, u.civ, 1);
                        }
                        if u.civ == realm.civs.viewer() || shooter_civ == realm.civs.viewer() {
                            post(&mut realm.board, if shot.result.hit {
                                "Defensive bombardment hit the attacker."
                            } else { "Defensive bombardment missed." });
                        }
                    }
                }
                seq.struck = true;
            }
            if seq.t >= seq.dur {
                if let Ok((_, mut u)) = units.get_mut(shot.shooter) {
                    u.anim = UnitAnim::Idle { t: 0.0 };
                }
                begin_round(seq, &art, &mut units);
            }
        }
        Phase::Round => {
            let Kind::Fight {
                defender,
                rounds,
                round,
                ..
            } = &seq.kind
            else {
                return;
            };
            // The blow lands part-way through the swing.
            if !seq.struck && seq.t >= seq.dur * STRIKE_AT {
                let loser = if rounds[*round] {
                    *defender
                } else {
                    seq.attacker
                };
                if let Ok((_, mut u)) = units.get_mut(loser) {
                    u.damage += 1;
                }
                seq.struck = true;
            }
            if seq.t >= seq.dur {
                let next = *round + 1;
                let more = next < rounds.len();
                if let Kind::Fight { round, .. } = &mut seq.kind {
                    *round = next;
                }
                if more {
                    begin_round(seq, &art, &mut units);
                } else {
                    begin_finale(seq, &art, &mut units);
                }
            }
        }
        Phase::Finale => {
            if seq.t < seq.dur {
                return;
            }
            let seq = active.0.take().expect("just borrowed");
            resolve(seq, &map, &mut commands, &art, &mut rng.0, &mut units, &mut realm);
        }
    }
}

/// Apply a finished sequence: the dead go, the victor may be promoted, a
/// lone victor takes the tile, captives change hands.
fn resolve(
    seq: Sequence,
    map: &GameMap,
    commands: &mut Commands,
    art: &UnitArt,
    rng: &mut MapRng,
    units: &mut Query<(Entity, &mut Unit)>,
    realm: &mut Realm,
) {
    let Ok((_, att)) = units.get(seq.attacker) else {
        return;
    };
    let att_civ = att.civ;
    match seq.kind {
        Kind::Fight {
            defender,
            outcome,
            retreat_to,
            alone,
            ..
        } => {
            if matches!(outcome, exe::Outcome::AttackerRetreated | exe::Outcome::DefenderRetreated) {
                let to = units.get(defender).map(|(_, u)| (u.x, u.y)).ok();
                let retreater = if outcome == exe::Outcome::AttackerRetreated { seq.attacker } else { defender };
                for e in [seq.attacker, defender] {
                    if let Ok((_, mut u)) = units.get_mut(e) {
                        u.anim = UnitAnim::Idle { t: 0.0 };
                        if e == retreater {
                            // HYPOTHESIS: a retreat ends the unit's movement.
                            u.moves = 0;
                            u.path.clear();
                            u.exploring = false;
                            u.auto = false;
                            u.work = None;
                            if outcome == exe::Outcome::DefenderRetreated {
                                let (x, y) = retreat_to.expect("a defender retreats only with an escape tile");
                                step_to(&mut u, x, y);
                            }
                            if u.civ == realm.civs.viewer() {
                                post(&mut realm.board, format!("Our {} has retreated!", def(u.utype).name));
                            }
                        } else if e == seq.attacker && alone {
                            // As with a killed lone defender, occupy the emptied tile.
                            if let Some((x, y)) = to { step_to(&mut u, x, y); }
                        }
                    }
                }
                return; // nobody dies or promotes, and the bystanders survive
            }
            let attacker_won = outcome == exe::Outcome::AttackerWon;
            let (winner, loser) = if attacker_won {
                (seq.attacker, defender)
            } else {
                (defender, seq.attacker)
            };
            let to = units.get(defender).map(|(_, u)| (u.x, u.y)).ok();
            let viewer = realm.civs.viewer();
            if let Ok((_, u)) = units.get(loser)
                && u.civ == viewer
            {
                post(
                    &mut realm.board,
                    format!("Our {} has been destroyed!", def(u.utype).name),
                );
            }
            // The melee callers kill only the loser (0x4A63EF / 0x4A6EF2).
            // 0x5BBBC0 recursively removes cargo, not ordinary tile occupants.
            let loser_barbarian = units.get(loser).is_ok_and(|(_, u)| crate::civs::is_barbarian(u.civ));
            let leaders: Vec<usize> = units.iter().filter(|(_, u)| u.utype == UnitType::Leader).map(|(_, u)| u.civ).collect();
            let leader_alive = |civ: usize| leaders.contains(&civ);
            if let Ok((_, w)) = units.get(winner)
                && crate::golden::unit_triggers(w.utype, loser_barbarian, w.civ)
            {
                crate::golden::request(w.civ);
            }
            commands.entity(loser).despawn();
            if let Ok((_, mut u)) = units.get_mut(winner) {
                u.anim = UnitAnim::Idle { t: 0.0 };
                let name = def(u.utype).name;
                if promote(rng, &mut u, loser_barbarian) && u.civ == realm.civs.viewer() {
                    post(&mut realm.board, u.level.promotion_text(name));
                }
                // Step 3: an elite land winner may bring forth a Great Leader.
                if crate::army::leader_eligible(&u, loser_barbarian)
                    && !leader_alive(u.civ)
                    && rng.below(crate::army::leader_die(u.civ, !attacker_won)) == 0
                {
                    u.made_leader = true;
                    let (civ, x, y) = (u.civ, u.x, u.y);
                    spawn_unit(commands, art, UnitType::Leader, x, y, civ);
                    if civ == realm.civs.viewer() {
                        post(&mut realm.board, format!("Our victorious {name} has produced a Great Leader!"));
                    }
                }
                // Alone on its tile, the beaten defender leaves it to the
                // winner (`MANUAL`); a stack or a city has to be walked into.
                if let (true, true, Some((x, y))) = (attacker_won, alone, to) {
                    advance_attacker(&map, &mut u, (x, y));
                }
            }
        }
        Kind::Capture { victims, city } => {
            let mut taken = Vec::new();
            let viewer = realm.civs.viewer();
            let mut lost_one = false;
            for v in victims {
                let Ok((_, mut u)) = units.get_mut(v) else {
                    continue;
                };
                taken.push((def(u.utype).name, u.x, u.y));
                lost_one |= u.civ == viewer;
                if crate::civs::is_barbarian(att_civ) {
                    // HYPOTHESIS: barbarians keep no captives; the
                    // non-combatants are killed.
                    commands.entity(v).despawn();
                    continue;
                }
                if u.utype == UnitType::Settler {
                    // "A captured Settler becomes two Workers" (`MANUAL`).
                    let (x, y) = (u.x, u.y);
                    let nationality = u.nationality;
                    commands.entity(v).despawn();
                    for _ in 0..2 {
                        let w = spawn_unit(commands, art, UnitType::Worker, x, y, att_civ);
                        commands
                            .entity(w)
                            .entry::<Unit>()
                            .and_modify(move |mut u| { u.moves = 0; u.nationality = nationality; });
                    }
                    continue;
                }
                u.anim = UnitAnim::Idle { t: 0.0 };
                u.civ = att_civ;
                u.moves = 0;
                u.fortified = false;
                u.sentry = false;
                u.exploring = false;
                u.work = None;
                u.path.clear();
            }
            if att_civ == viewer {
                post(
                    &mut realm.board,
                    match taken.as_slice() {
                        [(name, ..)] => format!("We captured an enemy {name}!"),
                        _ => "We captured enemy units!".to_string(),
                    },
                );
            } else if lost_one {
                post(
                    &mut realm.board,
                    match taken.as_slice() {
                        [(name, ..)] => format!("Our {name} was captured by the {}!", crate::civs::name(att_civ)),
                        _ => format!("Our units were captured by the {}!", crate::civs::name(att_civ)),
                    },
                );
            }
            let raid = crate::civs::is_barbarian(att_civ) && city.is_some();
            if let Ok((_, mut u)) = units.get_mut(seq.attacker) {
                u.anim = UnitAnim::Idle { t: 0.0 };
                if let (false, Some((_, x, y))) = (raid, taken.first()) {
                    advance_attacker(&map, &mut u, (*x, *y));
                }
            }
            if let Some(city) = city {
                realm.conquer(city, att_civ, rng);
            }
            if raid {
                commands.entity(seq.attacker).despawn();
            }
        }
    }
}

/// Successful shore attacks use the mover's landing cost (0x5B94D0).
fn advance_attacker(map: &GameMap, u: &mut Unit, to: (i32, i32)) {
    if def(u.utype).class == 0
        && map.get(u.x, u.y).is_some_and(|t| crate::improvements::is_water_base(t.base))
        && map.get(to.0, to.1).is_some_and(|t| !crate::improvements::is_water_base(t.base))
    {
        u.moves = 0;
    }
    step_to(u, to.0, to.1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::{Clip, UnitClipSet};
    use std::collections::{HashMap, HashSet};
    use std::time::Duration;

    fn tile(base: Base, relief: Relief, cover: Cover) -> Tile {
        Tile {
            base,
            relief,
            cover,
            variant: 0,
            seen: true,
            visible: true,
            hut: false,
            camp: false,
            resource: None,
            road: false,
            irrigation: false,
            river: 0,
            mine: false,
            site: None,
            fortress: false,
            barricade: false,
            forest_harvested: false,
            owner: None,
        }
    }

    /// A 6x4 map of one terrain.
    fn map_of(t: Tile) -> GameMap {
        GameMap {
            w: 6,
            h: 4,
            tiles: vec![t; 24],
            start: (1, 1),
            seed: 1,
        }
    }

    fn grass() -> GameMap {
        map_of(tile(Base::Grassland, Relief::Flat, Cover::Bare))
    }

    fn warrior(civ: usize, x: i32, y: i32) -> Unit {
        Unit::new(civ, UnitType::Warrior, x, y)
    }

    #[test]
    fn levels_carry_the_expr_hit_points() {
        let hp: Vec<i32> = [
            Level::Conscript,
            Level::Regular,
            Level::Veteran,
            Level::Elite,
        ]
        .iter()
        .map(|l| l.base_hp())
        .collect();
        assert_eq!(hp, [2, 3, 4, 5]);
        assert_eq!(max_hp(Level::Regular, 1), 4); // a War Elephant
        let mut u = warrior(0, 0, 0);
        u.damage = 2;
        assert_eq!((u.max_hp(), u.hp()), (3, 1));
        // A promotion raises the cap and so the hit points left.
        u.level = Level::Veteran;
        assert_eq!(u.hp(), 2);
        assert!(Level::Elite.next().is_none());
    }

    #[test]
    fn unit_factors_match_the_biq() {
        // PRTO dwords +92 attack, +84 defense, +160 extra hit points; the
        // name sits at +4 of the row. Skipped without the GOG install.
        let path = "civ3/civ3-gog/app/Conquests/conquests.biq";
        let Ok(raw) = std::fs::read(path) else {
            eprintln!("skipped: {path} is not installed");
            return;
        };
        let body = civ3mapgen::dcl::decompress(&raw).expect("biq decodes");
        let prto = civ3mapgen::dcl::sections(&body)
            .into_iter()
            .find(|s| s.tag_str() == "PRTO")
            .expect("PRTO section");
        let dword = |r: &[u8], at: usize| i32::from_le_bytes(r[at..at + 4].try_into().unwrap());
        for t in UnitType::all() {
            let row = (0..prto.count)
                .map(|i| prto.row(&body, i).unwrap())
                .find(|r| {
                    let n = &r[4..36];
                    let n = &n[..n.iter().position(|&c| c == 0).unwrap_or(n.len())];
                    n == def(t).name.as_bytes()
                })
                .unwrap_or_else(|| panic!("no PRTO row for {}", def(t).name));
            let d = def(t);
            assert_eq!(
                (d.attack, d.defense, d.hp_bonus),
                (dword(row, 92), dword(row, 84), dword(row, 160)),
                "{}",
                d.name
            );
        }
    }

    #[test]
    fn odds_follow_the_exe_formula_on_each_terrain() {
        let att = warrior(0, 1, 1);
        // Grassland, nothing special: X = 1 * 110, Y = 1 * 100.
        let map = grass();
        let d = warrior(1, 2, 1);
        assert_eq!(round_odds(&map, &att, &d, None), 1024 * 110 / 210);
        // A fortified defender on hills: 50 + 25.
        let hills = map_of(tile(Base::Grassland, Relief::Hill, Cover::Bare));
        let mut d = warrior(1, 2, 1);
        d.fortified = true;
        assert_eq!(defense_pct(&hills, &d, None), 75);
        assert_eq!(round_odds(&hills, &att, &d, None), 1024 * 175 / 275);
        // Sentried is not fortified; mountains beat hills; forest is 25.
        d.sentry = true;
        assert_eq!(defense_pct(&hills, &d, None), 50);
        let mountains = map_of(tile(Base::Grassland, Relief::Mountain, Cover::Bare));
        assert_eq!(defense_pct(&mountains, &warrior(1, 2, 1), None), 100);
        let forest = map_of(tile(Base::Plains, Relief::Flat, Cover::Forest));
        assert_eq!(defense_pct(&forest, &warrior(1, 2, 1), None), 25);
        // A city adds its size class: town 0, city 50, metropolis 100.
        let d = warrior(1, 2, 1);
        assert_eq!(defense_pct(&map, &d, Some(Hold::bare(6))), 10);
        assert_eq!(defense_pct(&map, &d, Some(Hold::bare(7))), 60);
        assert_eq!(defense_pct(&map, &d, Some(Hold::bare(13))), 110);
        // A fortress adds 50 outside a city, and a city's own bonus replaces it.
        let mut fort_tile = tile(Base::Grassland, Relief::Flat, Cover::Bare);
        fort_tile.fortress = true;
        let fort = map_of(fort_tile);
        // Grassland's own 10 plus the fortress's 50.
        assert_eq!(defense_pct(&fort, &d, None), 60);
        assert_eq!(defense_pct(&fort, &d, Some(Hold::bare(6))), 10);
    }

    #[test]
    fn the_odds_are_clamped_like_the_exe() {
        let mut att = Unit::new(0, UnitType::Warrior, 1, 1);
        att.damage = 0;
        // Equal strength with no bonus is an even fight, 512 of 1024.
        let map = map_of(tile(Base::Ocean, Relief::Flat, Cover::Bare));
        let d = warrior(1, 2, 1);
        let odds = round_odds(&map, &att, &d, None);
        assert!((1..=1023).contains(&odds));
    }

    /// The round loop is the exe's `duel`: same winner, same hit points
    /// left, and the same number of dice drawn from the same generator.
    #[test]
    fn rounds_match_the_reverse_engineered_duel() {
        use civ3mapgen::rng::Rng;
        let none = exe::RetreatFlags {
            attacker: false,
            defender: false,
        };
        for seed in 0..300u32 {
            for odds in [1, 200, 536, 651, 900, 1023] {
                for (a_hp, d_hp) in [(3, 3), (2, 4), (4, 2), (5, 1)] {
                    let mut mine = MapRng::new(seed);
                    let mut theirs = Rng::new(seed);
                    let fighter = |hp| exe::Fighter {
                        max_hp: hp,
                        damage: 0,
                        retreat_pct: 0,
                        owned: true,
                    };
                    let (mut a, mut d) = (fighter(a_hp), fighter(d_hp));
                    let (rounds, actual) = roll_duel(&mut mine, odds, a, d, none, false);
                    let outcome = exe::duel(&mut theirs, odds, &mut a, &mut d, none, false);
                    let won = rounds.iter().filter(|&&r| r).count() as i32;
                    let lost = rounds.len() as i32 - won;
                    assert_eq!((a.damage, d.damage), (lost, won), "seed {seed} odds {odds}");
                    assert_eq!(
                        outcome == exe::Outcome::AttackerWon,
                        rounds.last() == Some(&true),
                        "seed {seed} odds {odds}"
                    );
                    assert_eq!(mine.below(1024), theirs.below(1024), "dice drawn differ");
                    assert_eq!(actual, outcome);
                }
            }
        }
    }

    #[test]
    fn retreat_rounds_damage_and_dice_match_the_executable_model() {
        use civ3mapgen::rng::Rng;
        let mut outcomes = HashSet::new();
        for seed in 0..300 {
            for odds in [200, 512, 900] {
                for defender_can_step_back in [false, true] {
                    for (a_type, d_type, city) in [
                        (UnitType::Horseman, UnitType::Warrior, false),
                        (UnitType::Warrior, UnitType::Horseman, false),
                        (UnitType::Horseman, UnitType::Horseman, false),
                        (UnitType::Warrior, UnitType::Horseman, true),
                    ] {
                        let mut att = Unit::new(0, a_type, 1, 1);
                        let mut dfn = Unit::new(1, d_type, 2, 1);
                        att.level = Level::Elite;
                        att.damage = 1;
                        dfn.level = Level::Veteran;
                        let flags = exe::RetreatFlags::new(
                            i32::from(def(a_type).moves * MP), i32::from(def(d_type).moves * MP),
                            city, &exe::Rules::CONQUESTS,
                        );
                        let (mut a, mut d) = (fighter(&att), fighter(&dfn));
                        let mut mine = MapRng::new(seed);
                        let mut theirs = Rng::new(seed);
                        let (rounds, actual) = roll_duel(&mut mine, odds, a, d, flags, defender_can_step_back);
                        let expected = exe::duel(&mut theirs, odds, &mut a, &mut d, flags, defender_can_step_back);
                        let hits = rounds.iter().filter(|&&r| r).count() as i32;
                        assert_eq!((a.damage, d.damage), (att.damage + rounds.len() as i32 - hits, dfn.damage + hits));
                        assert_eq!(actual, expected);
                        assert_eq!(mine.below(1024), theirs.below(1024), "retreat dice sequence differs");
                        outcomes.insert(actual as u8);
                    }
                }
            }
        }
        assert_eq!(outcomes.len(), 4, "both deaths and both retreats were exercised");
    }

    #[test]
    fn the_best_defender_is_the_one_likeliest_to_win() {
        let map = grass();
        let att = warrior(0, 1, 1);
        let mut hurt = warrior(1, 2, 1);
        hurt.damage = 2;
        let healthy = warrior(1, 2, 1);
        let mut dug_in = warrior(1, 2, 1);
        dug_in.fortified = true;
        dug_in.damage = 1;
        let (e1, e2, e3) = (
            Entity::from_raw_u32(1).unwrap(),
            Entity::from_raw_u32(2).unwrap(),
            Entity::from_raw_u32(3).unwrap(),
        );
        let worker = Unit::new(1, UnitType::Worker, 2, 1);
        let e4 = Entity::from_raw_u32(4).unwrap();
        let stack = vec![
            (e1, hurt),
            (e2, healthy.clone()),
            (e3, dug_in),
            (e4, worker.clone()),
        ];
        // Fortified and one point down against healthy and unfortified:
        // 125% at 2 hit points loses to 110% at 3.
        assert_eq!(pick_defender(&map, &att, None, &stack), Some(e2));
        // Workers and Settlers cannot defend, however many there are.
        let workers = vec![(e4, worker.clone()), (e1, worker)];
        assert_eq!(pick_defender(&map, &att, None, &workers), None);
    }

    #[test]
    fn healing_is_slow_in_the_field_and_full_in_barracks() {
        assert_eq!(heal(2, Rest::Field), 1);
        assert_eq!(heal(0, Rest::Field), 0);
        assert_eq!(heal(3, Rest::City), 1);
        assert_eq!(heal(3, Rest::Barracks), 0);
    }

    fn city_at(civ: usize, x: i32, y: i32, size: u8) -> City {
        City {
            gifts: vec![],
            goods: 0,
            coastal: false,
            river: false,
            unrest: 0,
            hurry_timer: 0,
            stakes: Default::default(),
            cooldown: 0,
            unit_clocks: Vec::new(),
            civ,
            name: "Thebes".to_string(),
            x,
            y,
            diseased: false,
            citizens: crate::citizens::new_pool(civ, size),
            food: 3,
            shields: 4,
            production: Production::Warrior,
            queue: vec![Production::Temple],
            buildings: vec![],
            culture: 0,
            founded: 1,
        }
    }

    #[test]
    fn rest_depends_on_the_city_and_its_barracks() {
        let u = warrior(0, 1, 1);
        let mut home = city_at(0, 1, 1, 3);
        assert_eq!(rest_of(&u, [&home].into_iter()), Rest::City);
        home.buildings.push(Production::Barracks);
        assert_eq!(rest_of(&u, [&home].into_iter()), Rest::Barracks);
        // An enemy city heals nobody.
        let foreign = city_at(1, 1, 1, 3);
        assert_eq!(rest_of(&u, [&foreign].into_iter()), Rest::Field);
        assert_eq!(rest_of(&warrior(0, 2, 2), [&home].into_iter()), Rest::Field);
    }

    #[test]
    fn walls_add_half_again_to_a_town_but_not_to_a_city() {
        let mut town = city_at(1, 2, 1, 4);
        assert_eq!(Hold::of(&town).building_pct, 0);
        town.buildings.push(Production::Walls);
        assert_eq!(Hold::of(&town), Hold { size: 4, resisters: 0, building_pct: 50 });
        // Above a town they stop counting (`BLDG +0x98`), Civil Defense does not.
        let mut city = city_at(1, 2, 1, 9);
        city.buildings.push(Production::Walls);
        assert_eq!(Hold::of(&city).building_pct, 0);
        city.buildings.push(Production::CivilDefense);
        assert_eq!(Hold::of(&city).building_pct, 50);
        // The best bonus counts, not the sum.
        let mut both = city_at(1, 2, 1, 4);
        both.buildings.extend([Production::Walls, Production::CivilDefense]);
        assert_eq!(Hold::of(&both).building_pct, 50);
        // And the odds follow: walls make the defender likelier to win a round.
        let map = GameMap::generate();
        let att = warrior(0, 1, 1);
        let d = warrior(1, 2, 1);
        assert!(
            round_odds(&map, &att, &d, Some(Hold::of(&town))) > round_odds(&map, &att, &d, Some(Hold::bare(4)))
        );
    }

    #[test]
    fn promotion_climbs_one_level_and_stops_at_elite() {
        let mut u = warrior(2, 0, 0); // Egypt
        let mut rng = MapRng::new(5);
        let mut climbed = 0;
        for _ in 0..200 {
            u.failed_promotion = false;
            if promote(&mut rng, &mut u, false) {
                climbed += 1;
            }
        }
        assert_eq!((u.level, climbed), (Level::Elite, 2)); // Regular to Elite
        assert!(!promote(&mut rng, &mut u, false));
    }

    #[test]
    fn a_failed_roll_makes_the_next_victory_a_promotion() {
        let mut u = warrior(0, 0, 0);
        let mut rng = MapRng::new(11);
        // Win until a roll fails, then the very next win promotes.
        let mut fails = 0;
        while u.level == Level::Regular && fails < 100 {
            if !promote(&mut rng, &mut u, false) {
                assert!(u.failed_promotion);
                fails += 1;
                assert!(promote(&mut rng, &mut u, false), "the second win this turn");
            }
            u.failed_promotion = false;
        }
        assert_eq!(u.level, Level::Veteran);
    }

    #[test]
    fn a_militaristic_civ_promotes_twice_as_often() {
        let rate = |civ: usize| {
            let mut rng = MapRng::new(3);
            let mut won = 0;
            for _ in 0..4000 {
                let mut u = warrior(civ, 0, 0); // a Regular: die 4, or 2
                if promote(&mut rng, &mut u, false) {
                    won += 1;
                }
            }
            won
        };
        let (egypt, japan) = (rate(2), rate(0));
        assert!((900..1100).contains(&egypt), "{egypt}"); // one in four
        assert!((1900..2100).contains(&japan), "{japan}"); // one in two
    }

    #[test]
    fn the_bar_changes_colour_at_two_thirds_and_one_third() {
        let at = |hp, max| bar_color(hp, max);
        assert_eq!(at(3, 3), at(2, 2));
        assert_ne!(at(3, 3), at(2, 3));
        assert_ne!(at(2, 3), at(1, 3));
        assert_eq!(at(4, 5), at(5, 5)); // above two thirds
        assert_eq!(at(3, 5), at(2, 5)); // yellow
        assert_eq!(at(1, 5), at(1, 3)); // red
    }

    // ---- the sequencer, headless ----------------------------------------

    /// DEFAULT-only art for every unit type, so captures can spawn Workers.
    fn test_clip(frames: usize) -> Clip {
        Clip {
            strips: std::array::from_fn(|_| Handle::default()),
            teams: vec![],
            frame_w: 40.0,
            frame_h: 40.0,
            frames,
            ms: 100.0,
            feet: [0; 8],
            sounds: vec![],
        }
    }

    fn art() -> UnitArt {
        let clip = || test_clip(4);
        let mut art = UnitArt::default();
        for t in [
            UnitType::Settler,
            UnitType::Worker,
            UnitType::Warrior,
            UnitType::Scout,
            UnitType::Leader,
        ] {
            art.insert(
                t,
                UnitClipSet {
                    clips: HashMap::from([("DEFAULT".to_string(), clip())]),
                },
            );
        }
        art
    }

    fn arena(seed: u32) -> App {
        let mut app = App::new();
        app.insert_resource(grass());
        app.insert_resource(art());
        app.insert_resource(Time::<()>::default());
        app.insert_resource(CombatSpeed(1000.0));
        app.insert_resource(CombatRng(MapRng::new(seed)));
        app.init_resource::<ActiveCombat>();
        app.init_resource::<MessageBoard>();
        app.init_resource::<Capital>();
        app.init_resource::<crate::civs::Civilizations>();
        app.init_resource::<crate::diplomacy::Diplomacy>();
        app.init_resource::<crate::cities::Treasury>();
        app.init_resource::<CityView>();
        app.init_resource::<ProductionPrompts>();
        app.add_message::<AttackOrder>();
        app.add_systems(Update, (start_attacks, run_combat).chain());
        app
    }

    fn attack(app: &mut App, attacker: Entity, to: (i32, i32)) {
        app.world_mut().write_message(AttackOrder { attacker, to });
    }

    /// Run until nothing is playing; the clock jumps 50 ms a frame.
    fn settle(app: &mut App) {
        for _ in 0..4000 {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(Duration::from_millis(50));
            app.update();
            if app.world().resource::<ActiveCombat>().0.is_none() {
                return;
            }
        }
        panic!("the sequence never ended");
    }

    fn unit(app: &App, e: Entity) -> Option<Unit> {
        app.world().get::<Unit>(e).cloned()
    }

    #[test]
    fn sea_cargo_cannot_attack_but_port_cargo_can_leave_to_capture() {
        for water in [false, true] {
            let mut app = arena(1);
            app.add_systems(Update, crate::naval::sync_cargo.after(run_combat));
            if water {
                let mut map = app.world_mut().resource_mut::<GameMap>();
                let i = map.idx(1, 1);
                map.tiles[i].base = Base::Coast;
            } else {
                let mut port = city_at(0, 1, 1, 1);
                port.coastal = true;
                app.world_mut().spawn(port);
            }
            let ship = app.world_mut().spawn(Unit::new(0, UnitType::Galley, 1, 1)).id();
            let mut cargo = warrior(0, 1, 1);
            cargo.carrier = Some(ship);
            let a = app.world_mut().spawn(cargo).id();
            let worker = app.world_mut().spawn(Unit::new(1, UnitType::Worker, 2, 1)).id();
            attack(&mut app, a, (2, 1));
            settle(&mut app);
            let u = unit(&app, a).unwrap();
            if water {
                assert_eq!((u.x, u.y, u.moves, u.carrier), (1, 1, MP, Some(ship)));
                assert_eq!(unit(&app, worker).unwrap().civ, 1);
                assert_eq!(app.world().resource::<MessageBoard>().text, "This unit cannot attack from water.");
            } else {
                assert_eq!((u.x, u.y, u.moves, u.carrier), (2, 1, 0, None));
                assert_eq!(unit(&app, worker).unwrap().civ, 0);
            }
            assert!(unit(&app, ship).is_some());
        }
    }

    #[test]
    fn amphibious_city_entry_requires_an_unused_attack_and_spends_the_landing_turn() {
        let marine = UnitType::all().find(|t| def(*t).name == "Marine").unwrap();
        for attacked in [false, true] {
            let mut app = arena(1);
            let mut map = app.world_mut().resource_mut::<GameMap>();
            let i = map.idx(1, 1);
            map.tiles[i].base = Base::Coast;
            drop(map);
            let ship = app.world_mut().spawn(Unit::new(0, UnitType::Galley, 1, 1)).id();
            let mut u = Unit::new(0, marine, 1, 1);
            u.carrier = Some(ship);
            u.attacked = attacked;
            u.moves = 2 * MP;
            let a = app.world_mut().spawn(u).id();
            let city = app.world_mut().spawn(city_at(1, 2, 1, 1)).id();
            attack(&mut app, a, (2, 1));
            settle(&mut app);
            let u = unit(&app, a).unwrap();
            if attacked {
                assert_eq!((u.x, u.y, u.moves, u.carrier), (1, 1, 2 * MP, Some(ship)));
                assert_eq!(app.world().get::<City>(city).unwrap().civ, 1);
            } else {
                assert_eq!((u.x, u.y, u.moves, u.carrier), (2, 1, 0, None));
                assert_eq!(app.world().get::<City>(city).unwrap().civ, 0);
            }
        }
    }

    #[test]
    fn defensive_bombard_precedes_melee_and_matches_reference_damage_and_dice() {
        let mut hits = 0;
        for seed in 0..80 {
            let mut app = arena(seed);
            app.insert_resource(CombatSpeed(1.0));
            let att = warrior(0, 1, 1);
            let mut dfn = warrior(1, 2, 1);
            dfn.fortified = true;
            let shooter = Unit::new(1, UnitType::Catapult, 2, 1);
            let a = app.world_mut().spawn(att.clone()).id();
            let d = app.world_mut().spawn(dfn.clone()).id();
            let s = app.world_mut().spawn(shooter.clone()).id();
            app.world_mut().spawn(City::new(1, "Rome", 2, 1));
            let mut reference = civ3mapgen::rng::Rng::new(seed);
            let mut hp = fighter(&att);
            let shot = exe::defensive_bombard(&mut reference, support_odds(&shooter, &att), &mut hp);
            let odds = round_odds(app.world().resource::<GameMap>(), &att, &dfn, Some(Hold::bare(1)));
            let mut def_hp = fighter(&dfn);
            let expected = exe::duel(&mut reference, odds, &mut hp, &mut def_hp,
                exe::RetreatFlags { attacker: false, defender: false }, false);
            attack(&mut app, a, (2, 1));
            app.update();
            let seq = app.world().resource::<ActiveCombat>().0.as_ref().unwrap();
            assert!(seq.phase == Phase::Support);
            assert!(matches!(seq.kind, Kind::Fight { outcome, .. } if outcome == expected));
            let mut actual_rng = app.world().resource::<CombatRng>().0;
            assert_eq!(actual_rng.below(1024), reference.below(1024));
            assert_eq!(unit(&app, a).unwrap().damage, 0, "the shot is not applied before its animation");
            assert!(unit(&app, s).unwrap().defensive_fired, "a miss also uses the shot");
            app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs_f32(0.3));
            app.update();
            assert_eq!(unit(&app, a).unwrap().damage, i32::from(shot.hit), "only the support shot has landed");
            hits += i32::from(shot.hit);
            app.insert_resource(CombatSpeed(1000.0));
            settle(&mut app);
            if let Some(u) = unit(&app, a) { assert_eq!(u.damage, hp.damage); }
            if let Some(u) = unit(&app, d) {
                assert_eq!(u.damage, def_hp.damage);
                assert!(u.fortified, "attacks preserve the fortified order");
            }
            let u = unit(&app, s).unwrap();
            assert!(matches!(u.anim, UnitAnim::Idle { .. }));
            assert_eq!(u.moves, MP, "defensive fire spends no movement");
            assert!(!u.attacked, "offensive and defensive fire are independent");
        }
        assert!(hits > 0 && hits < 80, "both hits and misses were played");
    }

    #[test]
    fn supporting_fire_chooses_the_strongest_unused_unit_and_excludes_the_defender() {
        let att = warrior(0, 1, 1);
        let defender = Entity::from_bits(1);
        let archer = Entity::from_bits(2);
        let catapult = Entity::from_bits(3);
        let mut stack = vec![
            (defender, Unit::new(1, UnitType::Catapult, 2, 1)),
            (archer, Unit::new(1, UnitType::Archer, 2, 1)),
            (catapult, Unit::new(1, UnitType::Catapult, 2, 1)),
        ];
        assert_eq!(supporting_shooter(&att, defender, &stack), Some(catapult));
        stack[2].1.defensive_fired = true;
        assert_eq!(supporting_shooter(&att, defender, &stack), Some(archer));
        stack[1].1.defensive_fired = true;
        assert_eq!(supporting_shooter(&att, defender, &stack), None);
        stack[2].1.defensive_fired = false;
        let mut hurt = att;
        hurt.damage = 2;
        assert_eq!(supporting_shooter(&hurt, defender, &stack), None, "a shot never kills a land attacker");
    }

    #[test]
    fn one_supporting_unit_fires_only_once_between_its_turns() {
        for seed in 0..20 {
            let mut app = arena(seed);
            let a = app.world_mut().spawn(warrior(0, 1, 1)).id();
            let mut dfn = warrior(1, 2, 1);
            dfn.level = Level::Elite;
            dfn.utype = UnitType::Spearman;
            app.world_mut().spawn(dfn);
            let s = app.world_mut().spawn(Unit::new(1, UnitType::Catapult, 2, 1)).id();
            app.world_mut().spawn(City::new(1, "Rome", 2, 1));
            attack(&mut app, a, (2, 1));
            settle(&mut app);
            assert!(unit(&app, s).unwrap().defensive_fired);
            // Ensure the stack is defended even when the first defender died.
            app.world_mut().spawn(Unit::new(1, UnitType::Spearman, 2, 1));
            let a = app.world_mut().spawn(warrior(0, 1, 1)).id();
            attack(&mut app, a, (2, 1));
            app.update();
            assert!(app.world().resource::<ActiveCombat>().0.as_ref().unwrap().phase == Phase::Round);
            settle(&mut app);
        }
    }

    #[test]
    fn mounted_units_retreat_alive_without_death_clips_or_promotions() {
        for attacker_fast in [false, true] {
            let mut retreats = 0;
            for seed in 0..60 {
                let mut app = arena(seed);
                let fast = UnitType::Horseman;
                let slow = UnitType::Warrior;
                let (a_type, d_type) = if attacker_fast { (fast, slow) } else { (slow, fast) };
                let a = app.world_mut().spawn(Unit::new(0, a_type, 1, 1)).id();
                let d = app.world_mut().spawn(Unit::new(1, d_type, 2, 1)).id();
                let bystander = app.world_mut().spawn(Unit::new(1, UnitType::Worker, 2, 1)).id();
                attack(&mut app, a, (2, 1));
                app.update();
                let seq = app.world().resource::<ActiveCombat>().0.as_ref().unwrap();
                let Kind::Fight { outcome, .. } = seq.kind else { panic!("expected a duel") };
                let want = if attacker_fast { exe::Outcome::AttackerRetreated } else { exe::Outcome::DefenderRetreated };
                if outcome != want { continue; }
                retreats += 1;
                // Stop at the finale so the test checks playback as well as resolution.
                for _ in 0..100 {
                    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(50));
                    app.update();
                    if app.world().resource::<ActiveCombat>().0.as_ref().unwrap().phase == Phase::Finale { break; }
                }
                for e in [a, d] {
                    assert!(matches!(unit(&app, e).unwrap().anim, UnitAnim::Held { slot: "DEFAULT", .. }));
                }
                settle(&mut app);
                let (att, dfn) = (unit(&app, a).unwrap(), unit(&app, d).unwrap());
                assert_eq!((att.level, dfn.level), (Level::Regular, Level::Regular));
                assert_eq!((att.x, att.y), (1, 1), "the bystander still occupies the defended tile");
                let escaped = if attacker_fast { &att } else { &dfn };
                assert_eq!((escaped.hp(), escaped.moves), (1, 0));
                assert_eq!((dfn.x, dfn.y), if attacker_fast { (2, 1) } else { (3, 1) });
                assert!(unit(&app, bystander).is_some(), "a retreat never kills the stack");
            }
            assert!(retreats > 0, "both kinds of retreat must play through");
        }
    }

    #[test]
    fn a_lone_retreating_defender_leaves_the_tile_to_the_attacker() {
        let mut retreats = 0;
        for seed in 0..40 {
            let mut app = arena(seed);
            let a = app.world_mut().spawn(warrior(0, 1, 1)).id();
            let d = app.world_mut().spawn(Unit::new(1, UnitType::Horseman, 2, 1)).id();
            attack(&mut app, a, (2, 1));
            app.update();
            let seq = app.world().resource::<ActiveCombat>().0.as_ref().unwrap();
            if !matches!(seq.kind, Kind::Fight { outcome: exe::Outcome::DefenderRetreated, .. }) { continue; }
            settle(&mut app);
            retreats += 1;
            let (att, dfn) = (unit(&app, a).unwrap(), unit(&app, d).unwrap());
            assert_eq!((att.x, att.y), (2, 1));
            assert_eq!((dfn.x, dfn.y, dfn.hp()), (3, 1, 1));
        }
        assert!(retreats > 0);
    }

    #[test]
    fn defenders_do_not_retreat_from_cities_or_blocked_tiles_or_against_fast_attackers() {
        for seed in 0..40 {
            for condition in ["city", "blocked", "fast attacker"] {
                let mut app = arena(seed);
                let a_type = if condition == "fast attacker" { UnitType::Horseman } else { UnitType::Warrior };
                let a = app.world_mut().spawn(Unit::new(0, a_type, 1, 1)).id();
                let d = app.world_mut().spawn(Unit::new(1, UnitType::Horseman, 2, 1)).id();
                if condition == "city" {
                    app.world_mut().spawn(City::new(1, "Rome", 2, 1));
                } else if condition == "blocked" {
                    app.world_mut().spawn(warrior(2, 3, 1));
                }
                attack(&mut app, a, (2, 1));
                app.update();
                let seq = app.world().resource::<ActiveCombat>().0.as_ref().unwrap();
                assert!(matches!(seq.kind, Kind::Fight {
                    outcome: exe::Outcome::AttackerWon | exe::Outcome::DefenderWon, ..
                }), "{condition} seed {seed}");
                settle(&mut app);
                assert!(unit(&app, a).is_some() != unit(&app, d).is_some());
            }
        }
    }

    #[test]
    fn retreat_destinations_wrap_and_reject_impassable_or_foreign_occupied_tiles() {
        let mut map = grass();
        let att = warrior(0, 4, 1);
        let dfn = Unit::new(1, UnitType::Horseman, 5, 1);
        assert_eq!(retreat_tile(&map, &att, &dfn, std::iter::empty(), std::iter::empty()), Some((0, 1)));
        let friendly = warrior(1, 0, 1);
        assert_eq!(retreat_tile(&map, &att, &dfn, std::iter::once(&friendly), std::iter::empty()), Some((0, 1)));
        let foreign = warrior(2, 0, 1);
        assert_eq!(retreat_tile(&map, &att, &dfn, std::iter::once(&foreign), std::iter::empty()), None);
        let city = City::new(2, "Rome", 0, 1);
        assert_eq!(retreat_tile(&map, &att, &dfn, std::iter::empty(), std::iter::once(&city)), None);
        let i = map.idx(0, 1);
        map.tiles[i].base = Base::Ocean;
        assert_eq!(retreat_tile(&map, &att, &dfn, std::iter::empty(), std::iter::empty()), None);
        let att = warrior(0, 1, 1);
        let dfn = Unit::new(1, UnitType::Horseman, 1, 0);
        assert_eq!(retreat_tile(&map, &att, &dfn, std::iter::empty(), std::iter::empty()), None, "no escape off the map's pole");
    }

    #[test]
    fn a_fight_kills_exactly_one_side_and_the_log_adds_up() {
        let mut wins = [0, 0];
        for seed in 0..40 {
            let mut app = arena(seed);
            let a = app.world_mut().spawn(warrior(0, 1, 1)).id();
            let d = app.world_mut().spawn(warrior(1, 2, 1)).id();
            attack(&mut app, a, (2, 1));
            app.update();
            // The fight has started: the attacker paid a move, both faced up.
            let u = unit(&app, a).unwrap();
            assert_eq!((u.moves, u.attacked, u.rested), (0, true, false));
            assert!(app.world().resource::<ActiveCombat>().0.is_some());
            settle(&mut app);
            let (ua, ud) = (unit(&app, a), unit(&app, d));
            assert!(
                ua.is_some() != ud.is_some(),
                "seed {seed}: one side must die"
            );
            if let Some(w) = ua {
                wins[0] += 1;
                // Alone on the tile, the winning attacker took it.
                assert_eq!((w.x, w.y), (2, 1));
                assert!(w.hp() >= 1 && w.damage <= 2);
            } else {
                wins[1] += 1;
                let w = ud.unwrap();
                assert_eq!((w.x, w.y), (2, 1));
                assert!(w.hp() >= 1);
            }
        }
        // Grassland odds make a fight between equals close, never one-sided.
        assert!(wins[0] > 5 && wins[1] > 5, "{wins:?}");
    }

    #[test]
    fn both_fighters_keep_swinging_whoever_wins_the_round() {
        for seed in 0..6 {
            let mut app = arena(seed);
            {
                let mut art = app.world_mut().resource_mut::<UnitArt>();
                art.clips_mut(UnitType::Warrior)
                    .insert("ATTACK1".to_string(), test_clip(8));
            }
            let a = app.world_mut().spawn(warrior(0, 1, 1)).id();
            let d = app.world_mut().spawn(warrior(1, 2, 1)).id();
            attack(&mut app, a, (2, 1));
            let mut rounds_seen = 0;
            for _ in 0..4000 {
                app.world_mut()
                    .resource_mut::<Time>()
                    .advance_by(Duration::from_millis(50));
                app.update();
                let active = &app.world().resource::<ActiveCombat>().0;
                let Some(seq) = active else {
                    break;
                };
                if seq.phase == Phase::Round {
                    rounds_seen += 1;
                    for e in [a, d] {
                        let u = unit(&app, e).unwrap();
                        assert!(
                            matches!(
                                u.anim,
                                UnitAnim::Held { slot: "ATTACK1", looped: true, .. }
                            ),
                            "seed {seed}: a fighter stopped swinging"
                        );
                    }
                }
            }
            assert!(rounds_seen > 0, "seed {seed}: no round was played");
        }
    }

    #[test]
    fn the_blow_lands_during_the_round_not_at_its_start() {
        let mut app = arena(3);
        app.insert_resource(CombatSpeed(1.0));
        let a = app.world_mut().spawn(warrior(0, 1, 1)).id();
        let d = app.world_mut().spawn(warrior(1, 2, 1)).id();
        attack(&mut app, a, (2, 1));
        app.update();
        let dmg = |app: &App| unit(app, a).unwrap().damage + unit(app, d).unwrap().damage;
        assert_eq!(dmg(&app), 0);
        // No art: a bare round lasts BARE_ROUND, the blow at its middle.
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(BARE_ROUND * 0.4));
        app.update();
        assert_eq!(dmg(&app), 0);
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(BARE_ROUND * 0.2));
        app.update();
        assert_eq!(dmg(&app), 1);
    }

    #[test]
    fn losing_a_defender_preserves_other_units_on_its_tile() {
        for bystander in [UnitType::Warrior, UnitType::Worker, UnitType::Catapult] {
            let mut proved = false;
            for seed in 0..60 {
                let mut app = arena(seed);
                let a = app.world_mut().spawn(warrior(0, 1, 1)).id();
                let mut defender = warrior(1, 2, 1);
                defender.fortified = true; // selects this soldier over the bystander
                let d = app.world_mut().spawn(defender).id();
                let w = app.world_mut().spawn(Unit::new(1, bystander, 2, 1)).id();
                attack(&mut app, a, (2, 1));
                settle(&mut app);
                let u = unit(&app, w).expect("an ordinary tile occupant survives");
                assert_eq!((u.civ, u.x, u.y, u.damage), (1, 2, 1, 0));
                if let Some(u) = unit(&app, a) {
                    assert!(unit(&app, d).is_none());
                    assert_eq!((u.x, u.y), (1, 1));
                    proved = true;
                    break;
                }
                assert!(unit(&app, d).is_some());
            }
            assert!(proved, "no attacker victory for {bystander:?}");
        }
    }

    #[test]
    fn civilians_surviving_a_defenders_death_can_be_captured_on_a_later_move() {
        for seed in 0..60 {
            let mut app = arena(seed);
            let a = app.world_mut().spawn(warrior(0, 1, 1)).id();
            let d = app.world_mut().spawn(warrior(1, 2, 1)).id();
            let w = app.world_mut().spawn(Unit::new(1, UnitType::Worker, 2, 1)).id();
            attack(&mut app, a, (2, 1));
            settle(&mut app);
            if unit(&app, a).is_none() { continue; }
            assert!(unit(&app, d).is_none());
            assert_eq!(unit(&app, w).unwrap().civ, 1);
            // Give the winner another turn's move into the now undefended tile.
            {
                let mut u = app.world_mut().get_mut::<Unit>(a).unwrap();
                u.moves = 3;
                u.attacked = false;
            }
            attack(&mut app, a, (2, 1));
            settle(&mut app);
            assert_eq!(unit(&app, w).unwrap().civ, 0);
            let u = unit(&app, a).unwrap();
            assert_eq!((u.x, u.y), (2, 1));
            return;
        }
        panic!("no attacker victory");
    }

    #[test]
    fn a_stack_of_noncombatants_is_captured_and_a_settler_becomes_two_workers() {
        let mut app = arena(1);
        let a = app.world_mut().spawn(warrior(0, 1, 1)).id();
        let settler = app
            .world_mut()
            .spawn(Unit::new(1, UnitType::Settler, 2, 1))
            .id();
        let worker = app
            .world_mut()
            .spawn(Unit::new(1, UnitType::Worker, 2, 1))
            .id();
        attack(&mut app, a, (2, 1));
        settle(&mut app);
        assert!(unit(&app, settler).is_none(), "the Settler is replaced");
        let w = unit(&app, worker).unwrap();
        assert_eq!((w.civ, w.moves), (0, 0));
        assert_eq!(w.nationality, crate::civs::roster_index(1));
        let workers = app
            .world_mut()
            .query::<&Unit>()
            .iter(app.world())
            .filter(|u| u.civ == 0 && u.utype == UnitType::Worker)
            .inspect(|u| assert_eq!(u.nationality, crate::civs::roster_index(1), "settler captives retain their nationality too"))
            .count();
        assert_eq!(workers, 3); // the captured Worker plus two from the Settler
        let u = unit(&app, a).unwrap();
        assert_eq!((u.x, u.y, u.damage), (2, 1, 0));
        assert!(
            app.world()
                .resource::<MessageBoard>()
                .text
                .contains("captured")
        );
    }

    #[test]
    fn noncombatants_may_not_attack() {
        let mut app = arena(1);
        let scout = app
            .world_mut()
            .spawn(Unit::new(0, UnitType::Scout, 1, 1))
            .id();
        app.world_mut().spawn(warrior(1, 2, 1));
        attack(&mut app, scout, (2, 1));
        app.update();
        assert!(app.world().resource::<ActiveCombat>().0.is_none());
        assert_eq!(
            app.world().resource::<MessageBoard>().text,
            "Non-combat units may not attack."
        );
        assert_eq!(unit(&app, scout).unwrap().moves, 6);
    }

    #[test]
    fn a_unit_may_not_attack_twice_in_a_turn() {
        let mut app = arena(1);
        let mut w = warrior(0, 1, 1);
        w.attacked = true;
        let a = app.world_mut().spawn(w).id();
        app.world_mut().spawn(warrior(1, 2, 1));
        attack(&mut app, a, (2, 1));
        app.update();
        assert!(app.world().resource::<ActiveCombat>().0.is_none());
        assert_eq!(
            app.world().resource::<MessageBoard>().text,
            "This unit may not attack more than once."
        );
    }

    #[test]
    fn an_undefended_city_falls_to_the_next_soldier() {
        let mut app = arena(1);
        let a = app.world_mut().spawn(warrior(0, 1, 1)).id();
        let capital = app.world_mut().spawn(city_at(1, 2, 1, 4)).id();
        app.world_mut().resource_mut::<Capital>().0[1] = Some(capital);
        attack(&mut app, a, (2, 1));
        app.update();
        let c = app.world().get::<City>(capital).unwrap();
        assert_eq!((c.civ, c.size(), c.queue.len(), c.shields), (0, 3, 0, 0));
        assert_eq!(app.world().resource::<Capital>().0[1], None);
        // No fight: the soldier simply walked in, spending the tile's cost.
        assert!(app.world().resource::<ActiveCombat>().0.is_none());
        let u = unit(&app, a).unwrap();
        assert_eq!((u.x, u.y, u.moves, u.attacked), (2, 1, 0, false));
    }

    #[test]
    fn barbarians_raid_an_undefended_city_and_vanish() {
        // `combat.md` 14.2: shields above 10 are lost first, the city keeps
        // its owner and the raider is removed.
        let mut app = arena(1);
        let a = app.world_mut().spawn(warrior(crate::civs::BARBARIANS, 1, 1)).id();
        let mut c = city_at(1, 2, 1, 4);
        c.shields = 15;
        let city = app.world_mut().spawn(c).id();
        attack(&mut app, a, (2, 1));
        app.update();
        let c = app.world().get::<City>(city).unwrap();
        assert_eq!((c.civ, c.size(), c.shields), (1, 4, 0));
        assert!(unit(&app, a).is_none());
        // Next: no stock, so a citizen dies.
        let a = app.world_mut().spawn(warrior(crate::civs::BARBARIANS, 1, 1)).id();
        attack(&mut app, a, (2, 1));
        app.update();
        assert_eq!(app.world().get::<City>(city).unwrap().size(), 3);
        // A size-1 city with gold loses its treasury share.
        app.world_mut().get_mut::<City>(city).unwrap().set_size(1);
        app.world_mut().resource_mut::<crate::cities::Treasury>().0[1] = 30;
        let a = app.world_mut().spawn(warrior(crate::civs::BARBARIANS, 1, 1)).id();
        attack(&mut app, a, (2, 1));
        app.update();
        assert_eq!(app.world().resource::<crate::cities::Treasury>().0[1], 0);
    }

    #[test]
    fn the_civilized_side_gets_the_difficulty_bonus_against_barbarians() {
        let map = GameMap::generate();
        let civ = warrior(0, 1, 1);
        let barb = warrior(crate::civs::BARBARIANS, 2, 1);
        let even = round_odds(&map, &civ, &warrior(1, 2, 1), None);
        // Regent: +200% on the attacker's side lowers the defender's odds.
        assert!(round_odds(&map, &civ, &barb, None) < even);
        assert!(round_odds(&map, &barb, &civ, None) > round_odds(&map, &warrior(1, 1, 1), &civ, None));
    }

    #[test]
    fn a_noncombatant_cannot_take_a_city() {
        let mut app = arena(1);
        let a = app
            .world_mut()
            .spawn(Unit::new(0, UnitType::Worker, 1, 1))
            .id();
        let city = app.world_mut().spawn(city_at(1, 2, 1, 4)).id();
        attack(&mut app, a, (2, 1));
        app.update();
        assert_eq!(app.world().get::<City>(city).unwrap().civ, 1);
        assert_eq!(
            app.world().resource::<MessageBoard>().text,
            "Only combat units can capture cities and improvements."
        );
    }

    #[test]
    fn a_city_defender_does_not_take_its_garrison_with_it() {
        // Two warriors in a size-8 city: the attacker fights one at a time.
        for seed in 0..80 {
            let mut app = arena(seed);
            let a = app.world_mut().spawn(warrior(0, 1, 1)).id();
            let city = app.world_mut().spawn(city_at(1, 2, 1, 8)).id();
            let d1 = app.world_mut().spawn(warrior(1, 2, 1)).id();
            let d2 = app.world_mut().spawn(warrior(1, 2, 1)).id();
            attack(&mut app, a, (2, 1));
            settle(&mut app);
            if unit(&app, a).is_some() {
                let left = [d1, d2]
                    .iter()
                    .filter(|e| unit(&app, **e).is_some())
                    .count();
                assert_eq!(left, 1, "only the defender dies in a city");
                // And the city is still its owner's and nobody walked in.
                assert_eq!(app.world().get::<City>(city).unwrap().civ, 1);
                assert_eq!(unit(&app, a).map(|u| (u.x, u.y)), Some((1, 1)));
                return;
            }
        }
        panic!("the attacker never won in 80 seeds");
    }

    #[test]
    fn healing_runs_when_the_civ_turn_begins() {
        // A rested, damaged unit in the field mends a point; one that moved
        // does not.
        let mut app = App::new();
        app.add_message::<crate::units::TurnEnded>();
        app.add_message::<crate::civs::CivilizationEnded>();
        app.insert_resource(crate::units::Turn(1));
        app.init_resource::<crate::civs::Civilizations>();
        app.insert_resource(crate::units::Selected(None));
        app.insert_resource(crate::actionbar::GotoMode(false));
        app.add_systems(Update, crate::units::end_turn_units);
        let mk = |rested| {
            let mut u = warrior(1, 3, 3);
            u.damage = 2;
            u.rested = rested;
            u.moves = 0;
            u
        };
        let rested = app.world_mut().spawn(mk(true)).id();
        let moved = app.world_mut().spawn(mk(false)).id();
        app.world_mut().write_message(crate::units::TurnEnded);
        app.update();
        assert_eq!(unit(&app, rested).unwrap().damage, 1);
        assert_eq!(unit(&app, moved).unwrap().damage, 2);
        // The new turn refills moves and clears the day's attack.
        assert_eq!(unit(&app, rested).unwrap().moves, 3);
        assert!(unit(&app, moved).unwrap().rested);
    }

    /// Civs 0 and 1 are at war.
    fn at_war() -> crate::diplomacy::Diplomacy {
        let mut d = crate::diplomacy::Diplomacy::new();
        let mut board = crate::features::MessageBoard::default();
        d.declare(&crate::diplomacy::Facts::even(), 0, 1, 0, &mut board);
        d
    }

    #[test]
    fn capturing_a_foreign_city_seeds_resisters_who_lose_the_size_bonus_and_eat_nothing() {
        crate::realm::reset();
        let mut app = arena(1);
        app.insert_resource(at_war());
        let mut target = city_at(1, 2, 1, 1);
        target.set_size(10);
        let city = app.world_mut().spawn(target).id();
        // An empty city: the attacker walks in. Both civs keep cities, so the
        // old nation can still rally its people.
        app.world_mut().spawn(city_at(1, 8, 8, 1));
        app.world_mut().spawn(city_at(0, 12, 12, 1));
        let a = app.world_mut().spawn(warrior(0, 1, 1)).id();
        attack(&mut app, a, (2, 1));
        settle(&mut app);
        let c = app.world().get::<City>(city).unwrap().clone();
        assert_eq!(c.civ, 0);
        assert_eq!(c.size(), 9, "one citizen died in the capture");
        // Equal (zero) culture falls to the smallest-ratio CULT row: 90%
        // initial resistance, Despotism against Despotism adds nothing.
        let n = crate::resistance::resisters(&c);
        assert!(n >= 1, "90% each: at least one of nine resists");
        assert!(c.citizens.slots().iter().flatten().all(|z| z.pending_race == crate::civs::roster_index(0) as i32));
        assert!(c.citizens.slots().iter().flatten().filter(|z| z.resister).all(|z| z.work == 0 && z.job == 0));
        assert_eq!(Hold::of(&c).resisters, n);
        let text = &app.world().resource::<MessageBoard>().text;
        assert!(text.contains("resister"), "{text}");
        // A resisting city has no size bonus, an otherwise identical one does.
        let map = app.world().resource::<GameMap>();
        let d = warrior(0, 2, 1);
        let calm = Hold { resisters: 0, ..Hold::of(&c) };
        assert!(defense_pct(map, &d, Some(Hold::of(&c))) < defense_pct(map, &d, Some(calm)));
        let t = crate::citycalc::totals(map, &c);
        assert_eq!(t.eaten, 2 * (9 - n));
        assert!(matches!(crate::hurry::quote(&c, civ3mapgen::government::hurry::PAY, 9999, crate::hurry::Buyer::Human), Err(crate::hurry::Refusal::Resistance)));
    }

    #[test]
    fn a_pillaged_road_blocks_a_wheeled_attack_before_dispatch() {
        let mut app = arena(1);
        app.insert_resource(at_war());
        app.init_resource::<crate::unit_picker::UnitPicker>();
        app.add_systems(Update, crate::units::drive_movement.before(start_attacks));
        let mut chariot = Unit::new(0, UnitType::Chariot, 1, 1);
        chariot.path = [(2, 1)].into();
        let attacker = app.world_mut().spawn(chariot).id();
        let defender = app.world_mut().spawn(warrior(1, 2, 1)).id();
        {
            let mut map = app.world_mut().resource_mut::<GameMap>();
            let i = map.idx(2, 1);
            map.tiles[i].relief = Relief::Mountain;
            map.tiles[i].road = false;
        }
        app.update();
        assert_eq!(app.world().resource::<Messages<AttackOrder>>().len(), 0);
        assert!(unit(&app, attacker).unwrap().path.is_empty());
        assert_eq!(unit(&app, attacker).unwrap().moves, 2 * MP);
        // Direct orders also pass through the terrain refusal gate.
        attack(&mut app, attacker, (2, 1));
        app.update();
        assert!(app.world().resource::<ActiveCombat>().0.is_none());
        assert_eq!(unit(&app, defender).unwrap().damage, 0);
        assert_eq!(unit(&app, attacker).unwrap().moves, 2 * MP);
    }

    #[test]
    fn a_step_into_a_civ_at_peace_is_not_an_attack_but_a_question() {
        use crate::units::drive_movement;
        let mut app = App::new();
        app.insert_resource(grass());
        app.init_resource::<crate::civs::Civilizations>();
        app.insert_resource(crate::diplomacy::Diplomacy::new());
        app.init_resource::<crate::cities::Treasury>();
        app.add_message::<AttackOrder>();
        app.init_resource::<crate::unit_picker::UnitPicker>();
        app.add_systems(Update, drive_movement);
        let mut w = warrior(0, 1, 1);
        w.path = [(2, 1)].into();
        let a = app.world_mut().spawn(w).id();
        app.world_mut().spawn(warrior(1, 2, 1));
        app.update();
        assert_eq!(app.world().resource::<Messages<AttackOrder>>().len(), 0);
        let u = unit(&app, a).unwrap();
        assert_eq!((u.x, u.y, u.moves), (1, 1, 3));
        assert!(u.path.is_empty());
        let ask = app.world().resource::<crate::diplomacy::Diplomacy>().war_ask;
        // Every chair is a human's here, so the question is asked.
        assert_eq!(ask.map(|q| (q.attacker, q.to, q.target)), Some((a, (2, 1), 1)));
    }

    #[test]
    fn a_route_into_an_enemy_attacks_only_on_its_last_step() {
        use crate::units::drive_movement;
        let mut app = App::new();
        app.insert_resource(grass());
        app.init_resource::<crate::civs::Civilizations>();
        app.insert_resource(at_war());
        app.add_message::<AttackOrder>();
        app.init_resource::<crate::unit_picker::UnitPicker>();
        app.add_systems(Update, drive_movement);
        let mut w = warrior(0, 1, 1);
        w.path = [(2, 1)].into();
        let a = app.world_mut().spawn(w).id();
        app.world_mut().spawn(warrior(1, 2, 1));
        app.update();
        let orders: Vec<_> = app
            .world()
            .resource::<Messages<AttackOrder>>()
            .iter_current_update_messages()
            .map(|o| (o.attacker, o.to))
            .collect();
        assert_eq!(orders, [(a, (2, 1))]);
        // Not a step: the unit stays and keeps its moves.
        let u = unit(&app, a).unwrap();
        assert_eq!((u.x, u.y, u.moves), (1, 1, 3));

        // An enemy in the middle of a longer route stops it.
        let mut app = App::new();
        app.insert_resource(grass());
        app.init_resource::<crate::civs::Civilizations>();
        app.insert_resource(at_war());
        app.add_message::<AttackOrder>();
        app.init_resource::<crate::unit_picker::UnitPicker>();
        app.add_systems(Update, drive_movement);
        let mut w = warrior(0, 1, 1);
        w.path = [(2, 1), (3, 1)].into();
        let a = app.world_mut().spawn(w).id();
        app.world_mut().spawn(warrior(1, 2, 1));
        app.update();
        let u = unit(&app, a).unwrap();
        assert!(u.path.is_empty());
        assert_eq!((u.x, u.y), (1, 1));
        assert_eq!(app.world().resource::<Messages<AttackOrder>>().len(), 0);
    }
}
