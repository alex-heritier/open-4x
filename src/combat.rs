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
use crate::civs::CIVS;
use crate::features::{MessageBoard, post};
use crate::map::*;
use crate::production_prompt::ProductionPrompts;
use crate::rng::MapRng;
use crate::units::{Unit, UnitAnim, UnitArt, UnitType, def, spawn_unit, step_to};

// ---------------------------------------------------------------------------
// Experience, hit points, healing
// ---------------------------------------------------------------------------

/// Experience level, the row of `EXPR` in `conquests.biq`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
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
fn terrain_row(t: &Tile) -> usize {
    match (t.relief, t.cover, t.base) {
        (Relief::Mountain, _, _) => 6,
        (Relief::Hill, _, _) => 5,
        (_, Cover::Forest | Cover::Pine, _) => 7,
        (_, Cover::Jungle, _) => 8,
        (_, _, Base::Desert) => 0,
        (_, _, Base::Plains) => 1,
        (_, _, Base::Grassland) => 2,
        (_, _, Base::Tundra | Base::Ice) => 3,
        (_, _, Base::Coast) => 11,
        (_, _, Base::Sea) => 12,
        (_, _, Base::Ocean) => 13,
    }
}

/// What a defending city adds to the odds: its size class and the best
/// defensive improvement in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hold {
    pub size: u8,
    /// Percent from Walls, Civil Defense, the Great Wall's gift
    /// (`exe::city_building_bonus`, the best one, not the sum).
    pub building_pct: i32,
}

impl Hold {
    /// A city with no defensive improvements.
    #[cfg(test)]
    pub fn bare(size: u8) -> Hold {
        Hold { size, building_pct: 0 }
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
            size: city.size,
            building_pct: exe::city_building_bonus(i32::from(city.size), false, &list, &exe::Rules::CONQUESTS),
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
            resisters: 0,
            building_pct: h.building_pct,
        },
        None => exe::Structure::None,
    };
    exe::terrain_term(exe::TERRAIN_DEFENSE_PCT[terrain_row(tile)], false, &rules)
        + exe::tile_term(structure, false, &rules)
        + exe::fortify_term(true, false, d.fortified && !d.sentry, 1, &rules)
}

/// `1024 * P(defender wins a round)` for `att` against `dfn`, `0x4A0ED0`.
pub fn round_odds(map: &GameMap, att: &Unit, dfn: &Unit, hold: Option<Hold>) -> i32 {
    exe::defender_round_odds(&exe::OddsInput {
        att_strength: exe::attack_strength(&[], def(att.utype).attack),
        att_army_bonus: 0,
        att_pct: 0,
        def_strength: exe::defense_strength(&[], def(dfn.utype).defense, false),
        def_army_bonus: 0,
        def_pct: defense_pct(map, dfn, hold),
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
/// This is the exe's `duel` (`0x4A5B3C` loop) without the retreat blocks:
/// no unit in the clone moves faster than one tile per turn and a Scout
/// cannot fight, so [`exe::RetreatFlags`] never allow a retreat. The test
/// `rounds_match_the_reverse_engineered_duel` pins it to `exe::duel`.
pub fn roll_rounds(rng: &mut MapRng, odds: i32, mut att_hp: i32, mut def_hp: i32) -> Vec<bool> {
    let mut rounds = Vec::new();
    while att_hp > 0 && def_hp > 0 {
        let attacker_won = rng.below(exe::ROUND_DIE) >= odds;
        rounds.push(attacker_won);
        if attacker_won {
            def_hp -= 1;
        } else {
            att_hp -= 1;
        }
    }
    rounds
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
        .filter(|(_, u)| def(u.utype).defense > 0)
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
            .is_ok_and(|u| u.damage > 0 || fighting.contains(&e))
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
        /// One per round, `true` when the attacker won it.
        rounds: Vec<bool>,
        round: usize,
        /// The rest of the defender's stack, killed with it outside cities.
        bystanders: Vec<Entity>,
        city: bool,
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
    Round,
    Finale,
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
}

impl Realm<'_, '_> {
    /// An enemy city falls to `by`. HYPOTHESIS for the details: the manual
    /// says a captured city can be razed or kept, Small Wonders are always
    /// lost and "plundered gold" comes with it, and `POPDESTROYED` says
    /// citizens die; none of that was read from the exe. The clone keeps the
    /// city, loses one citizen (down to a minimum of 1), loses the build
    /// queue and the Palace, and does not plunder or raze.
    fn conquer(&mut self, e: Entity, by: usize) {
        let Ok((_, mut city)) = self.cities.get_mut(e) else {
            return;
        };
        let old = city.civ;
        city.civ = by;
        // The human hears of captures that concern them; the computer's
        // wars with itself go unannounced.
        let viewer = self.civs.viewer();
        let shrank = city.size > 1;
        if shrank {
            city.size -= 1;
            city.worked.clear();
        }
        if by == viewer {
            post(
                &mut self.board,
                if shrank {
                    format!(
                        "We captured {}! Some of its citizens were killed.",
                        city.name
                    )
                } else {
                    format!("We captured {}!", city.name)
                },
            );
        } else if old == viewer {
            post(
                &mut self.board,
                format!("{} has fallen to the {}!", city.name, CIVS[by].name),
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

/// Facing from `from` toward the adjacent tile `to`, across the map seam.
fn face(map: &GameMap, from: (i32, i32), to: (i32, i32)) -> usize {
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
            defender, rounds, ..
        } => {
            let (winner, loser) = if rounds.last() == Some(&true) {
                (seq.attacker, *defender)
            } else {
                (*defender, seq.attacker)
            };
            cue(art, units, loser, "DEATH", &mut beat);
            cue(art, units, winner, "VICTORY", &mut beat);
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
    let att = att.clone();
    let from = (att.x, att.y);
    let at = order.to;
    let city = realm
        .cities
        .iter()
        .find(|(_, c)| (c.x, c.y) == at && c.civ != att.civ)
        .map(|(e, c)| (e, Hold::of(c)));
    let stack: Vec<(Entity, Unit)> = units
        .iter()
        .filter(|(_, u)| (u.x, u.y) == at && u.civ != att.civ)
        .map(|(e, u)| (e, u.clone()))
        .collect();
    if stack.is_empty() && city.is_none() {
        return; // the tile emptied since the step was planned
    }

    // The exe refuses some attacks before they start (`0x5B5CD0`; the
    // strings are `NONCOMBATANT`, `COMBATCONQUER`, `BLITZLIMIT`).
    let refusal = if def(att.utype).attack == 0 {
        Some(if stack.is_empty() {
            "Only combat units can capture cities and improvements."
        } else {
            "Non-combat units may not attack."
        })
    } else if att.attacked {
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
        let cost = match (map.get(from.0, from.1), map.get(at.0, at.1)) {
            (Some(f), Some(t)) => step_cost(f, t).unwrap_or(MP),
            _ => MP,
        };
        if let Ok((_, mut u)) = units.get_mut(order.attacker) {
            u.moves -= cost.min(u.moves);
            u.rested = false;
            step_to(&mut u, at.0, at.1);
        }
        realm.conquer(city, att.civ);
        return;
    }

    let hold = city.map(|(_, h)| h);
    let kind = match pick_defender(&map, &att, hold, &stack) {
        Some(d) => {
            let dfn = &stack
                .iter()
                .find(|(e, _)| *e == d)
                .expect("picked from the stack")
                .1;
            let odds = round_odds(&map, &att, dfn, hold);
            Kind::Fight {
                defender: d,
                rounds: roll_rounds(&mut rng.0, odds, att.hp(), dfn.hp()),
                round: 0,
                bystanders: stack.iter().map(|(e, _)| *e).filter(|e| *e != d).collect(),
                city: city.is_some(),
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
            // Units wake when attacked, and turn to face the blow.
            u.fortified = false;
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
            resolve(seq, &mut commands, &art, &mut rng.0, &mut units, &mut realm);
        }
    }
}

/// Apply a finished sequence: the dead go, the victor may be promoted, a
/// lone victor takes the tile, captives change hands.
fn resolve(
    seq: Sequence,
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
            rounds,
            bystanders,
            city,
            alone,
            ..
        } => {
            let attacker_won = rounds.last() == Some(&true);
            let (winner, loser) = if attacker_won {
                (seq.attacker, defender)
            } else {
                (defender, seq.attacker)
            };
            // The defender's stack dies with it, except in a city.
            let mut dead = vec![loser];
            if attacker_won && !city {
                dead.extend(bystanders);
            }
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
            for e in dead {
                commands.entity(e).despawn();
            }
            if let Ok((_, mut u)) = units.get_mut(winner) {
                u.anim = UnitAnim::Idle { t: 0.0 };
                let name = def(u.utype).name;
                if promote(rng, &mut u, false) && u.civ == realm.civs.viewer() {
                    post(&mut realm.board, u.level.promotion_text(name));
                }
                // Alone on its tile, the beaten defender leaves it to the
                // winner (`MANUAL`); a stack or a city has to be walked into.
                if let (true, true, Some((x, y))) = (attacker_won, alone, to) {
                    step_to(&mut u, x, y);
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
                if u.utype == UnitType::Settler {
                    // "A captured Settler becomes two Workers" (`MANUAL`).
                    let (x, y) = (u.x, u.y);
                    commands.entity(v).despawn();
                    for _ in 0..2 {
                        let w = spawn_unit(commands, art, UnitType::Worker, x, y, att_civ);
                        commands
                            .entity(w)
                            .entry::<Unit>()
                            .and_modify(|mut u| u.moves = 0);
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
                        [(name, ..)] => format!("Our {name} was captured by the {}!", CIVS[att_civ].name),
                        _ => format!("Our units were captured by the {}!", CIVS[att_civ].name),
                    },
                );
            }
            if let Ok((_, mut u)) = units.get_mut(seq.attacker) {
                u.anim = UnitAnim::Idle { t: 0.0 };
                if let Some((_, x, y)) = taken.first() {
                    step_to(&mut u, *x, *y);
                }
            }
            if let Some(city) = city {
                realm.conquer(city, att_civ);
            }
        }
    }
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
            mine: false,
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
                    let rounds = roll_rounds(&mut mine, odds, a_hp, d_hp);
                    let mut theirs = Rng::new(seed);
                    let fighter = |hp| exe::Fighter {
                        max_hp: hp,
                        damage: 0,
                        retreat_pct: 0,
                        owned: true,
                    };
                    let (mut a, mut d) = (fighter(a_hp), fighter(d_hp));
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
                }
            }
        }
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
            coastal: false,
            river: false,
            unrest: 0,
            civ,
            name: "Thebes".to_string(),
            x,
            y,
            size,
            food: 3,
            shields: 4,
            production: Production::Warrior,
            queue: vec![Production::Temple],
            buildings: vec![],
            worked: HashSet::from([(x, y + 1)]),
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
        assert_eq!(Hold::of(&town), Hold { size: 4, building_pct: 50 });
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
    fn a_defender_in_a_stack_takes_the_whole_stack_with_it() {
        // Odds of 1023/1024 for the attacker are not reachable with 1 vs 1,
        // so the stack is the loser's: find a seed where the attacker wins.
        let mut proved = false;
        for seed in 0..60 {
            let mut app = arena(seed);
            let a = app.world_mut().spawn(warrior(0, 1, 1)).id();
            let d = app.world_mut().spawn(warrior(1, 2, 1)).id();
            let w = app
                .world_mut()
                .spawn(Unit::new(1, UnitType::Worker, 2, 1))
                .id();
            attack(&mut app, a, (2, 1));
            settle(&mut app);
            if unit(&app, a).is_some() {
                // Stack killed, and the winner stays where it stood.
                assert!(unit(&app, d).is_none() && unit(&app, w).is_none());
                let u = unit(&app, a).unwrap();
                assert_eq!((u.x, u.y), (1, 1));
                proved = true;
                break;
            }
            assert!(unit(&app, d).is_some() && unit(&app, w).is_some());
        }
        assert!(proved);
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
        let workers = app
            .world_mut()
            .query::<&Unit>()
            .iter(app.world())
            .filter(|u| u.civ == 0 && u.utype == UnitType::Worker)
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
        assert_eq!((c.civ, c.size, c.queue.len(), c.shields), (0, 3, 0, 0));
        assert_eq!(app.world().resource::<Capital>().0[1], None);
        // No fight: the soldier simply walked in, spending the tile's cost.
        assert!(app.world().resource::<ActiveCombat>().0.is_none());
        let u = unit(&app, a).unwrap();
        assert_eq!((u.x, u.y, u.moves, u.attacked), (2, 1, 0, false));
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
    fn a_step_into_a_civ_at_peace_is_not_an_attack_but_a_question() {
        use crate::units::drive_movement;
        let mut app = App::new();
        app.insert_resource(grass());
        app.init_resource::<crate::civs::Civilizations>();
        app.insert_resource(crate::diplomacy::Diplomacy::new());
        app.add_message::<AttackOrder>();
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
