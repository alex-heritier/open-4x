//! Units: definitions, art, spawning, selection, movement, animation.

use bevy::prelude::*;
use bevy::sprite::Anchor;
use serde::Deserialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::OnceLock;
use std::fs;

use crate::combat::Level;
use crate::improvements::{Work, action_slot};
use crate::map::*;
use crate::render::{RevealAll, sprite_z};

/// A unit type: the `PRTO` row of `conquests.biq` (see `roster.rs`). The
/// named constants (`UnitType::Warrior`, ...) are generated with the rows.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UnitType(pub u8);

impl std::fmt::Debug for UnitType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.row().name)
    }
}

impl UnitType {
    /// Every unit the game plays with, for loading art and iterating rules.
    pub fn all() -> impl Iterator<Item = UnitType> {
        (0..crate::roster::UNIT_COUNT as u8)
            .map(UnitType)
            .filter(|t| t.row().playable)
    }

    /// The type's `PRTO` row.
    pub fn row(self) -> &'static crate::roster::UnitRow {
        crate::roster::unit(self.0 as usize)
    }
}

pub type UnitDef = crate::roster::UnitRow;

pub fn def(t: UnitType) -> &'static UnitDef {
    t.row()
}

#[derive(Deserialize)]
struct ClipEntry {
    frame: [u32; 2],
    frames: usize,
    #[allow(dead_code)]
    dirs: usize,
    /// FLC header speed: milliseconds per frame.
    ms: f32,
    /// Per direction, rows of padding under the lowest solid pixel.
    feet: [i32; 8],
    /// The slot's sound cues, `[seconds into the clip, wav]`.
    #[serde(default)]
    sounds: Vec<(f32, String)>,
}

pub struct Clip {
    pub strips: [Handle<Image>; 8],
    /// The strips in each owner's team color (`TEAM_COLORS` order), when
    /// the converted art has them.
    pub teams: Vec<[Handle<Image>; 8]>,
    pub frame_w: f32,
    pub frame_h: f32,
    pub frames: usize,
    /// Milliseconds per frame, from the FLC header.
    pub ms: f32,
    pub feet: [i32; 8],
    /// Sound cues of the clip, in seconds from its first frame.
    pub sounds: Vec<(f32, Handle<AudioSource>)>,
}

/// Team color (`ntpNN.pcx`) of each unit owner, `CIVS` order then the
/// barbarians (RACE `default_color` in `conquests.biq`).
pub const TEAM_COLORS: [u8; crate::civs::CIV_COUNT + 1] = [4, 1, 3, 5, 0];

impl Clip {
    /// The strip for facing `dir` in `civ`'s colors.
    pub fn strip(&self, dir: usize, civ: usize) -> Handle<Image> {
        self.teams.get(civ).map_or_else(|| self.strips[dir].clone(), |t| t[dir].clone())
    }

    /// Playing time in seconds.
    pub fn duration(&self) -> f32 {
        self.frames as f32 * self.ms / 1000.0
    }

    /// Frame to show `t` seconds in; a looping clip wraps, others hold the
    /// last frame.
    pub fn frame_at(&self, t: f32, looped: bool) -> usize {
        let f = (t.max(0.0) * 1000.0 / self.ms) as usize;
        if looped { f % self.frames } else { f.min(self.frames - 1) }
    }
}

pub struct UnitClipSet {
    pub clips: HashMap<String, Clip>,
}

/// Unit art, loaded the first time a unit type is drawn: the roster has
/// dozens of types and the clips are large, so only the types in play are
/// ever read from disk.
#[derive(Resource)]
pub struct UnitArt {
    server: Option<AssetServer>,
    sets: Vec<OnceLock<Option<UnitClipSet>>>,
}

impl Default for UnitArt {
    fn default() -> Self {
        Self {
            server: None,
            sets: (0..crate::roster::UNIT_COUNT).map(|_| OnceLock::new()).collect(),
        }
    }
}

impl UnitArt {
    pub fn load(asset_server: &AssetServer) -> Self {
        Self { server: Some(asset_server.clone()), ..Self::default() }
    }

    /// Install a clip set by hand (tests).
    #[cfg(test)]
    pub fn insert(&mut self, t: UnitType, set: UnitClipSet) {
        self.sets[t.0 as usize] = OnceLock::from(Some(set));
    }

    /// The clips of an installed set (tests).
    #[cfg(test)]
    pub fn clips_mut(&mut self, t: UnitType) -> &mut HashMap<String, Clip> {
        &mut self.sets[t.0 as usize].get_mut().unwrap().as_mut().unwrap().clips
    }

    fn read(asset_server: &AssetServer, t: UnitType) -> Option<UnitClipSet> {
        let dir = def(t).art;
        if dir.is_empty() {
            return None;
        }
        let text = match fs::read_to_string(format!("assets/gen/units/{dir}/manifest.json")) {
            Ok(text) => text,
            Err(e) => {
                warn!("{}: no converted art ({e}); run tools/prep_assets.py", def(t).name);
                return None;
            }
        };
        let raw: HashMap<String, ClipEntry> =
            serde_json::from_str(&text).expect("unit manifest parses");
        let mut clips = HashMap::new();
        for (slot, e) in raw {
            let strips: Vec<Handle<Image>> = (0..8)
                .map(|d| asset_server.load(format!("gen/units/{dir}/{slot}_d{d}.png")))
                .collect();
            let tinted = std::path::Path::new(&format!("assets/gen/units/{dir}/{slot}_d0_c{}.png", TEAM_COLORS[0])).exists();
            let teams: Vec<[Handle<Image>; 8]> = if tinted {
                TEAM_COLORS
                    .iter()
                    .map(|c| {
                        let v: Vec<Handle<Image>> = (0..8)
                            .map(|d| asset_server.load(format!("gen/units/{dir}/{slot}_d{d}_c{c}.png")))
                            .collect();
                        v.try_into().unwrap()
                    })
                    .collect()
            } else {
                vec![]
            };
            clips.insert(
                slot,
                Clip {
                    strips: strips.try_into().unwrap(),
                    teams,
                    frame_w: e.frame[0] as f32,
                    frame_h: e.frame[1] as f32,
                    frames: e.frames,
                    ms: e.ms,
                    feet: e.feet,
                    sounds: e
                        .sounds
                        .iter()
                        .map(|(at, wav)| {
                            (*at, asset_server.load(format!("gen/audio/units/{dir}/{wav}")))
                        })
                        .collect(),
                },
            );
        }
        Some(UnitClipSet { clips })
    }

    fn set(&self, t: UnitType) -> Option<&UnitClipSet> {
        self.sets[t.0 as usize]
            .get_or_init(|| self.server.as_ref().and_then(|s| Self::read(s, t)))
            .as_ref()
    }

    pub fn clip(&self, t: UnitType, slot: &str) -> Option<&Clip> {
        self.set(t)?.clips.get(slot)
    }

    /// Where a clip's origin sits for facing `dir`: Civ3 frames pad their
    /// feet by different amounts per clip, so each is lifted by its
    /// difference to DEFAULT and the feet stay planted on the tile.
    pub fn anchor(&self, t: UnitType, clip: &Clip, dir: usize) -> Anchor {
        let base = self.clip(t, "DEFAULT").map_or(0, |d| d.feet[dir]);
        let lift = FEET_PX + (clip.feet[dir] - base) as f32;
        Anchor(Vec2::new(0.0, -0.5 + lift / clip.frame_h))
    }
}

/// Rows between a standing unit's lowest solid pixel and the tile center.
const FEET_PX: f32 = 6.0;

#[derive(Component, Clone)]
pub struct Unit {
    pub civ: usize,
    /// Native Unit +0x60 carrier link; cargo shares the carrier tile.
    pub carrier: Option<Entity>,
    pub utype: UnitType,
    pub x: i32,
    pub y: i32,
    pub moves: u8,
    pub fortified: bool,
    pub facing: usize,
    pub path: VecDeque<(i32, i32)>,
    pub anim: UnitAnim,
    pub work: Option<Work>,
    /// Sentried: skipped by auto-select like fortified, but not dug in.
    pub sentry: bool,
    /// Auto-exploring: each turn the unit routes itself toward unseen tiles
    /// until none are reachable. Any manual order cancels it.
    pub exploring: bool,
    /// Automated worker: the unit picks its own jobs and routes
    /// (`actions::auto_workers`). Any manual order cancels it.
    pub auto: bool,
    /// Experience level: sets the hit points (`EXPR`).
    pub level: Level,
    /// Hit points lost, as the exe keeps them (`unit+0x4C`): the unit dies
    /// when `max_hp - damage` reaches 0, and a promotion heals by itself.
    pub damage: i32,
    /// Has not moved or attacked since its civ's last turn began: only a
    /// rested unit heals.
    pub rested: bool,
    /// Attacked this turn (a unit without Blitz may not attack twice).
    pub attacked: bool,
    /// Fired its supporting bombard shot since its own turn began (bit 0x40).
    pub defensive_fired: bool,
    /// Failed a promotion roll this turn (status bit 2): the next victory
    /// promotes without one (`combat.md` 6.2).
    pub failed_promotion: bool,
    /// An Army's members. Civ3 never unloads an army, so a unit that joins
    /// is folded into it and lives and dies with it (`army`).
    pub members: Vec<(UnitType, Level)>,
    /// Has produced a Great Leader (status bit `0x20`, `combat.md` 6.2).
    pub made_leader: bool,
}

impl Unit {
    /// A fresh, rested, undamaged Regular facing the viewer.
    pub fn new(civ: usize, utype: UnitType, x: i32, y: i32) -> Self {
        Unit {
            civ,
            carrier: None,
            utype,
            x,
            y,
            moves: crate::naval::moves(utype, civ),
            fortified: false,
            facing: 0, // south: a fresh unit faces the viewer
            path: VecDeque::new(),
            anim: UnitAnim::Idle { t: 0.0 },
            work: None,
            sentry: false,
            exploring: false,
            auto: false,
            level: Level::Regular,
            damage: 0,
            rested: true,
            attacked: false,
            defensive_fired: false,
            failed_promotion: false,
            members: vec![],
            made_leader: false,
        }
    }

    /// `0x5BE5B0`: an army with members has their hit points summed.
    pub fn max_hp(&self) -> i32 {
        if self.members.is_empty() {
            return crate::combat::max_hp(self.level, def(self.utype).hp_bonus);
        }
        let sum: i32 = self.members.iter().map(|&(t, l)| crate::combat::max_hp(l, def(t).hp_bonus)).sum();
        (sum + def(self.utype).hp_bonus).max(1)
    }

    /// `0x5BE6E0`: an army attacks with its members' rounded average.
    pub fn attack(&self) -> i32 {
        self.strength(|t| def(t).attack)
    }

    /// `0x5BE820`: the same for defense.
    pub fn defense(&self) -> i32 {
        self.strength(|t| def(t).defense)
    }

    fn strength(&self, of: impl Fn(UnitType) -> i32) -> i32 {
        let n = self.members.len() as i32;
        if n == 0 {
            return of(self.utype);
        }
        (self.members.iter().map(|&(t, _)| of(t)).sum::<i32>() + n / 2) / n
    }

    /// `0x5BCAE0`: an army adds a sixth of its members' summed strength.
    pub fn army_bonus(&self, defending: bool) -> i32 {
        let sum: i32 = self.members.iter().map(|&(t, _)| if defending { def(t).defense } else { def(t).attack }).sum();
        sum / 6
    }

    /// `0x5BE470`: an army moves at its slowest member's speed plus one;
    /// anyone else at its own allowance.
    pub fn allowance(&self) -> u8 {
        match self.members.iter().map(|&(t, _)| crate::naval::moves(t, self.civ)).min() {
            Some(m) if def(self.utype).abilities & crate::roster::ability::ARMY != 0 => m + MP,
            _ => crate::naval::moves(self.utype, self.civ),
        }
    }

    /// Hit points left.
    pub fn hp(&self) -> i32 {
        self.max_hp() - self.damage
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum UnitAnim {
    Idle {
        t: f32,
    },
    Stepping {
        from: Vec2,
        to: Vec2,
        t: f32,
        dur: f32,
    },
    OneShot {
        slot: &'static str,
        t: f32,
    },
    /// Played by the combat sequencer, at the clip's own FLC speed: it
    /// holds the last frame (or loops) until the sequencer lets go.
    Held {
        slot: &'static str,
        t: f32,
        looped: bool,
    },
}

#[derive(Resource, Default)]
pub struct Selected(pub Option<Entity>);

#[derive(Resource)]
pub struct Turn(pub u32);

#[derive(Message)]
pub struct TurnEnded;

const STEP_DUR: f32 = 0.38;

/// FLC strip order, read off the warrior sheet: the unit's facing runs
/// d0 = S, d1 = SE, d2 = E, d3 = NE, d4 = N, d5 = NW, d6 = W, d7 = SW, the
/// compass cycle counterclockwise from south. d1 is the wide chest-on view
/// (map-southeast walks straight at the camera) and d3 the right profile
/// (map-northeast walks straight across the screen).
pub fn facing_for_step(dx: i32, dy: i32) -> usize {
    // Compass angle (east 0, north 90), never the isometric screen angle:
    // on screen the map's cardinals run along the diagonals, so rounding
    // the screen angle to the nearest eighth of a turn gave east and north
    // a neighbouring strip.
    let theta = (-(dy as f32)).atan2(dx as f32).to_degrees();
    ((theta - 270.0) / 45.0).round().rem_euclid(8.0) as usize
}

/// Frame sub-rect, inset half a pixel so strip sampling never bleeds
/// the neighboring frame at UV boundaries.
fn frame_rect(clip: &Clip, f: usize) -> Rect {
    Rect::new(
        f as f32 * clip.frame_w + 0.5,
        0.0,
        (f + 1) as f32 * clip.frame_w - 0.5,
        clip.frame_h,
    )
}

pub(crate) fn spawn_unit(
    commands: &mut Commands,
    art: &UnitArt,
    utype: UnitType,
    x: i32,
    y: i32,
    civ: usize,
) -> Entity {
    spawn_unit_at_level(commands, art, utype, x, y, civ, Level::Regular)
}

/// `spawn_unit` with an experience level (Barracks turn out Veterans).
pub(crate) fn spawn_unit_at_level(
    commands: &mut Commands,
    art: &UnitArt,
    utype: UnitType,
    x: i32,
    y: i32,
    civ: usize,
    level: Level,
) -> Entity {
    let clip = art.clip(utype, "DEFAULT").expect("DEFAULT clip");
    let pos = tile_to_world(x, y);
    let e = commands
        .spawn((
            Sprite {
                image: clip.strip(0, civ),
                rect: Some(frame_rect(clip, 0)),
                ..default()
            },
            art.anchor(utype, clip, 0),
            Transform::from_xyz(pos.x, pos.y, sprite_z(x, y, 4.0)),
            Unit {
                level,
                ..Unit::new(civ, utype, x, y)
            },
        ))
        .id();
    crate::combat::spawn_health_bar(commands, e);
    e
}

pub fn spawn_party(mut commands: Commands, map: Res<GameMap>, art: Res<UnitArt>) {
    let party = [
        UnitType::Settler,
        UnitType::Worker,
        UnitType::Warrior,
        UnitType::Scout,
    ];
    let mut first = None;
    for (civ, (sx, sy)) in crate::civs::starting_positions(&map)
        .into_iter()
        .enumerate()
    {
        for (i, t) in party.iter().enumerate() {
            let e = spawn_unit(&mut commands, &art, *t, sx, sy, civ);
            if civ == 0 && i == 0 {
                first = Some(e);
            }
        }
    }
    commands.insert_resource(Selected(first));
}

/// Sea units enter water or a friendly port; land units use terrain and roads.
pub fn entry_cost(map: &GameMap, u: &Unit, from: (i32, i32), to: (i32, i32), ports: &[(i32, i32)]) -> Option<u8> {
    let t = map.get(to.0, to.1)?;
    if def(u.utype).class == 1 {
        (matches!(t.base, Base::Coast | Base::Sea | Base::Ocean) || ports.contains(&to)).then_some(MP)
    } else {
        step_cost(map.get(from.0, from.1)?, t)
    }
}

pub fn route(map: &GameMap, u: &Unit, dest: (i32, i32), ports: &[(i32, i32)]) -> Option<Vec<(i32, i32)>> {
    map.find_path_by((u.x, u.y), dest, |from, to| entry_cost(map, u, from, to, ports))
}

pub fn order_move(map: &GameMap, u: &mut Unit, dest: (i32, i32), ports: &[(i32, i32)]) {
    // A manual move order cancels standing orders: worker jobs, fortify,
    // sentry, and auto-explore.
    u.work = None;
    u.exploring = false;
    u.auto = false;
    if (u.x, u.y) == dest {
        u.path.clear();
        return;
    }
    match route(map, u, dest, ports) {
        Some(p) => {
            u.path = p.into();
            u.fortified = false;
            u.sentry = false;
        }
        None => u.path.clear(),
    }
}

/// Route an auto-exploring unit toward its nearest unseen passable tile.
/// Breadth-first over passable tiles, so the first unseen tile found is the
/// closest reachable one and needs a single `find_path`. Returns false when
/// nothing reachable is left unseen, which ends the automation.
pub fn plan_explore(map: &GameMap, u: &mut Unit) -> bool {
    let mut seen_q = VecDeque::from([(u.x, u.y)]);
    let mut done = HashSet::from([(u.x, u.y)]);
    while let Some((x, y)) = seen_q.pop_front() {
        for (nx, ny) in map.neighbors(x, y) {
            if !done.insert((nx, ny)) {
                continue;
            }
            let Some(t) = map.get(nx, ny) else {
                continue;
            };
            if entry_cost(map, u, (x, y), (nx, ny), &[]).is_none() {
                continue;
            }
            // Huts and camps are worth the detour: stepping on them pops them.
            if !t.seen || t.hut || t.camp {
                if let Some(p) = route(map, u, (nx, ny), &[]) {
                    if !p.is_empty() {
                        u.path = p.into();
                        return true;
                    }
                }
                // Unseen but unreachable from here: keep looking past it.
            }
            seen_q.push_back((nx, ny));
        }
    }
    u.path.clear();
    false
}

/// Extra turns a unit at `start` needs to walk `path` (0 = arrives this
/// turn), under the "any movement left may enter" rule. Moves are thirds.
pub fn path_turns(
    map: &GameMap,
    start: (i32, i32),
    moves_now: u8,
    moves_max: u8,
    path: &[(i32, i32)],
) -> u32 {
    let mut mv = moves_now;
    let mut turns = 0;
    let mut from = start;
    for &(x, y) in path {
        let cost = match (map.get(from.0, from.1), map.get(x, y)) {
            (Some(a), Some(b)) => step_cost(a, b).unwrap_or(MP),
            _ => MP,
        };
        from = (x, y);
        if mv == 0 {
            turns += 1;
            mv = moves_max;
        }
        mv -= cost.min(mv);
    }
    turns
}

/// Moves left for display: whole MP plus any thirds, like Civ3's "1 1/3".
pub fn fmt_moves(thirds: u8) -> String {
    match (thirds / MP, thirds % MP) {
        (w, 0) => w.to_string(),
        (0, f) => format!("{f}/{MP}"),
        (w, f) => format!("{w} {f}/{MP}"),
    }
}

pub fn end_turn_units(
    mut events: MessageReader<TurnEnded>,
    mut turn: ResMut<Turn>,
    mut civs: ResMut<crate::civs::Civilizations>,
    mut ended: MessageWriter<crate::civs::CivilizationEnded>,
    mut selected: ResMut<Selected>,
    mut goto: ResMut<crate::actionbar::GotoMode>,
    mut units: Query<&mut Unit>,
    cities: Query<&crate::cities::City>,
    mut barbarians: Option<ResMut<crate::barbarians::Barbarians>>,
) {
    for _ in events.read() {
        // Once the game is decided there are no more turns to play.
        if civs.outcome.is_some() {
            continue;
        }
        ended.write(crate::civs::CivilizationEnded(civs.active));
        let next = civs.next_active();
        // The round counter ticks when the order wraps past the end.
        let round = next <= civs.active;
        if round {
            turn.0 += 1;
            // The barbarians open the round (`barbarians.md` 2).
            if let Some(b) = barbarians.as_mut() {
                b.begin_round();
            }
        }
        civs.active = next;
        if !crate::civs::is_ai(next) {
            civs.last_human = next;
        }
        selected.0 = None;
        goto.0 = false;
        for mut u in units.iter_mut() {
            if u.civ == civs.active || round && crate::civs::is_barbarian(u.civ) {
                u.moves = u.allowance();
                u.attacked = false;
                u.defensive_fired = false;
                u.failed_promotion = false;
                // A unit that sat out its last turn heals; one that moved
                // or fought did not.
                if u.rested {
                    let rest = crate::combat::rest_of(&u, cities.iter());
                    u.damage = crate::combat::heal(u.damage, rest);
                }
                u.rested = true;
            }
        }
    }
}

pub fn drive_movement(
    map: Res<GameMap>,
    mut picker: ResMut<crate::unit_picker::UnitPicker>,
    civs: Res<crate::civs::Civilizations>,
    cities: Query<&crate::cities::City>,
    mut diplomacy: ResMut<crate::diplomacy::Diplomacy>,
    mut attacks: MessageWriter<crate::combat::AttackOrder>,
    mut units: Query<(Entity, &mut Unit)>,
    barbarians: Option<Res<crate::barbarians::Barbarians>>,
) {
    if picker.unload.is_some() { return; }
    // During the barbarian phase only barbarians move.
    let mover = if barbarians.is_some_and(|b| b.phase) { crate::civs::BARBARIANS } else { civs.active };
    // Which civs have units on each tile: a step onto a tile held by
    // another civ is an attack, not a move, and only against a civ at war.
    let mut snapshot: Vec<_> = units.iter().map(|(e, u)| (e, u.clone())).collect();
    let mut held: HashMap<(i32, i32), u32> = HashMap::new();
    for (_, u) in units.iter().filter(|(_, u)| u.carrier.is_none()) {
        *held.entry((u.x, u.y)).or_default() |= 1 << u.civ;
    }
    for (e, mut u) in units.iter_mut() {
        if u.civ != mover {
            continue;
        }
        if !matches!(u.anim, UnitAnim::Idle { .. }) {
            continue;
        }
        if u.fortified || u.moves == 0 {
            continue;
        }
        // Auto-explore plans a fresh leg whenever the last one runs out;
        // with nothing left unseen the automation ends and the unit waits.
        if u.exploring && u.path.is_empty() && !plan_explore(&map, &mut u) {
            u.exploring = false;
            continue;
        }
        let Some(&(nx, ny)) = u.path.front() else {
            continue;
        };
        // Native mover stage 2 (0x5B91EC) keeps the ship offshore and
        // forwards the intended land tile to the passenger dialog.
        if def(u.utype).class == 1 && map.get(nx, ny).is_some_and(|t| !crate::improvements::is_water_base(t.base))
            && !cities.iter().any(|c| (c.x, c.y) == (nx, ny)) {
            u.path.clear();
            if snapshot.iter().any(|(_, cargo)| cargo.carrier == Some(e)) {
                picker.unload = Some(e);
                picker.shore = Some((nx, ny));
            }
            break;
        }
        let mut others = held.get(&(nx, ny)).copied().unwrap_or(0) & !(1 << u.civ);
        if let Some(c) = cities.iter().find(|c| (c.x, c.y) == (nx, ny) && c.civ != u.civ) {
            others |= 1 << c.civ;
        }
        if others != 0 {
            if !crate::naval::can_attack_from(&map, &u) { u.path.clear(); continue; }
            // Routes stop short of another civ; only the last step attacks,
            // and only a civ at war: a human is asked whether to declare it.
            if u.path.len() == 1 {
                match diplomacy.first_at_peace(u.civ, others) {
                    None => {
                        diplomacy.note_attack(u.civ, others);
                        attacks.write(crate::combat::AttackOrder {
                            attacker: e,
                            to: (nx, ny),
                        });
                        // One fight at a time: the rest wait for the sequencer.
                        break;
                    }
                    Some(target) => {
                        if !crate::civs::is_ai(u.civ) && diplomacy.war_ask.is_none() {
                            diplomacy.war_ask =
                                Some(crate::diplomacy::WarAsk { attacker: e, to: (nx, ny), target });
                        }
                    }
                }
            }
            u.path.clear();
            u.exploring = false;
            continue;
        }
        let ports: Vec<_> = cities.iter().filter(|c| c.civ == u.civ && c.coastal).map(|c| (c.x, c.y)).collect();
        let boarding = def(u.utype).class == 0 && map.get(nx, ny).is_some_and(|t| crate::improvements::is_water_base(t.base));
        let carrier = if boarding { crate::naval::pick_carrier(&u, (nx, ny), &snapshot) } else { None };
        let cost = if boarding { carrier.map(|_| MP) } else { entry_cost(&map, &u, (u.x, u.y), (nx, ny), &ports) };
        let Some(cost) = cost else {
            u.path.clear();
            continue;
        };
        // Civ3 rule: any unit with movement left may enter, spending it all
        // when the tile costs more than it has.
        u.moves -= cost.min(u.moves);
        // Native mover 0x5B94D0: a land unit leaving water spends its turn.
        if def(u.utype).class == 0 && map.get(u.x, u.y).is_some_and(|t| crate::improvements::is_water_base(t.base)) && !boarding {
            u.moves = 0;
        }
        let from = (u.x, u.y);
        step_to(&mut u, nx, ny);
        u.carrier = carrier;
        if boarding {
            u.sentry = true;
            u.path.clear();
            u.exploring = false;
            u.auto = false;
            u.work = None;
        }
        if let Some((_, old)) = snapshot.iter_mut().find(|(id, _)| *id == e) { *old = u.clone(); }
        // The computer's moves out of the human's sight are not worth
        // watching: they hop, so its turn does not drag.
        let lit = |(x, y): (i32, i32)| map.get(x, y).is_some_and(|t| t.visible);
        if crate::civs::is_ai(u.civ) && !lit(from) && !lit((nx, ny)) {
            if let UnitAnim::Stepping { dur, .. } = &mut u.anim {
                *dur = HIDDEN_STEP_DUR;
            }
        }
    }
    // 0x5C5835..0x5C5924: AI uses the same passenger mover without a dialog.
    if crate::civs::is_ai(civs.active) {
        if let Some(ship) = picker.unload.take() {
            let shore = picker.shore.take();
            for (_, mut u) in &mut units {
                if u.carrier == Some(ship) { crate::naval::disembark(&map, &mut u, shore); }
            }
        }
    }
}

/// Seconds a step takes when nobody is looking.
const HIDDEN_STEP_DUR: f32 = 0.03;

/// Walk `u` onto the adjacent tile: face it, take the tile and start the
/// stepping animation. Movement points are the caller's business.
pub fn step_to(u: &mut Unit, nx: i32, ny: i32) {
    // Native setPosition 0x5BD5A8: leaving the carrier tile detaches cargo.
    // Boarding movement assigns the new carrier after the step commits.
    if (u.x, u.y) != (nx, ny) { u.carrier = None; }
    u.facing = facing_for_step(nx - u.x, ny - u.y);
    let from = tile_to_world(u.x, u.y);
    let to = tile_to_world(nx, ny);
    u.x = nx;
    u.y = ny;
    u.path.pop_front();
    u.fortified = false;
    u.rested = false;
    u.anim = UnitAnim::Stepping {
        from,
        to,
        t: 0.0,
        dur: STEP_DUR,
    };
}

pub fn advance_anims(
    time: Res<Time>,
    speed: Res<crate::combat::CombatSpeed>,
    art: Res<UnitArt>,
    mut q: Query<(&mut Unit, &mut Transform)>,
) {
    for (mut u, mut tf) in q.iter_mut() {
        let utype = u.utype;
        match &mut u.anim {
            UnitAnim::Idle { t } => *t += time.delta_secs(),
            UnitAnim::Stepping { from, to, t, dur } => {
                *t += time.delta_secs();
                let k = (*t / *dur).min(1.0);
                let p = from.lerp(*to, k);
                tf.translation.x = p.x;
                tf.translation.y = p.y;
                tf.translation.z = sprite_z(u.x, u.y, 4.0);
                if k >= 1.0 {
                    u.anim = UnitAnim::Idle { t: 0.0 };
                }
            }
            UnitAnim::OneShot { slot, t } => {
                *t += time.delta_secs();
                let frames = art.clip(utype, *slot).map(|c| c.frames).unwrap_or(8) as f32;
                if *t * 12.0 >= frames {
                    u.anim = UnitAnim::Idle { t: 0.0 };
                }
            }
            // The sequencer ends it; the clock runs at its speed.
            UnitAnim::Held { t, .. } => *t += time.delta_secs() * speed.0,
        }
    }
}

/// Put `frame` of `clip` on the sprite, facing `u.facing`, feet on the tile.
fn show(
    art: &UnitArt,
    u: &Unit,
    clip: &Clip,
    frame: usize,
    sprite: &mut Sprite,
    anchor: &mut Anchor,
) {
    sprite.image = clip.strip(u.facing, u.civ);
    sprite.rect = Some(frame_rect(clip, frame));
    *anchor = art.anchor(u.utype, clip, u.facing);
}

pub fn animate_units(art: Res<UnitArt>, mut q: Query<(&Unit, &mut Sprite, &mut Anchor)>) {
    for (u, mut sprite, mut anchor) in q.iter_mut() {
        let (slot, t, fps, looped) = match u.anim {
            UnitAnim::Idle { t } => {
                // Standing orders show: a working unit keeps looping its
                // job clip, and a fortified or sentried unit holds its
                // dug-in frame (Civ3 keeps FORTIFY's last frame up; units
                // without that art idle as usual).
                if let Some(w) = u.work {
                    (action_slot(w.action), t, 12.0, true)
                } else if u.fortified {
                    match art.clip(u.utype, "FORTIFY") {
                        Some(clip) => {
                            show(&art, u, clip, clip.frames - 1, &mut sprite, &mut anchor);
                            continue;
                        }
                        None => ("DEFAULT", t, 8.0, true),
                    }
                } else {
                    ("DEFAULT", t, 8.0, true)
                }
            }
            UnitAnim::Stepping { t, dur, .. } => {
                // run cycle syncs to the step duration
                let clip = art.clip(u.utype, "RUN");
                let fps = clip.map(|c| c.frames as f32 / dur).unwrap_or(20.0);
                let Some(clip) = clip.or_else(|| art.clip(u.utype, "DEFAULT")) else {
                    continue;
                };
                let f = ((t * fps) as usize) % clip.frames;
                show(&art, u, clip, f, &mut sprite, &mut anchor);
                continue;
            }
            UnitAnim::OneShot { slot, t } => (slot, t, 12.0, false),
            UnitAnim::Held { slot, t, looped } => {
                // Combat clips run at their FLC speed.
                let clip = art.clip(u.utype, slot).or_else(|| art.clip(u.utype, "DEFAULT"));
                let Some(clip) = clip else {
                    continue;
                };
                show(&art, u, clip, clip.frame_at(t, looped), &mut sprite, &mut anchor);
                continue;
            }
        };
        let Some(clip) = art
            .clip(u.utype, slot)
            .or_else(|| art.clip(u.utype, "DEFAULT"))
        else {
            continue;
        };
        let f = if looped {
            ((t * fps) as usize) % clip.frames
        } else {
            ((t * fps) as usize).min(clip.frames - 1)
        };
        show(&art, u, clip, f, &mut sprite, &mut anchor);
    }
}

pub fn refresh_visibility(
    mut map: ResMut<GameMap>,
    civs: Res<crate::civs::Civilizations>,
    mut explored: Local<[Vec<bool>; crate::civs::CIV_COUNT]>,
    mut previous: Local<Option<usize>>,
    units: Query<&Unit>,
    cities: Query<&crate::cities::City>,
) {
    if let Some(civ) = *previous {
        for (seen, tile) in explored[civ].iter_mut().zip(&map.tiles) {
            *seen = tile.seen;
        }
    }
    // The screen shows the human's view, even while the computer plays.
    let viewer = civs.viewer();
    *previous = Some(viewer);
    let memory = &mut explored[viewer];
    memory.resize(map.tiles.len(), false);
    for (t, &seen) in map.tiles.iter_mut().zip(memory.iter()) {
        t.visible = false;
        t.seen = seen;
    }
    for u in units.iter() {
        if u.civ != viewer || u.carrier.is_some() {
            continue;
        }
        let bonus = match map.get(u.x, u.y).map(|t| t.relief) {
            Some(Relief::Hill) | Some(Relief::Mountain) => 1,
            _ => 0,
        };
        let r = def(u.utype).sight as i32 + bonus;
        for dy in -r..=r {
            for dx in -r..=r {
                let ny = u.y + dy;
                if ny < 0 || ny >= map.h {
                    continue;
                }
                let nx = map.wrap_x(u.x + dx);
                let i = map.idx(nx, ny);
                let t = &mut map.tiles[i];
                t.visible = true;
                t.seen = true;
            }
        }
    }
    // Cities light their workable radius, as in Civ3.
    for c in cities.iter() {
        if c.civ != viewer {
            continue;
        }
        let mut tiles = crate::cities::radius_tiles(&map, c.x, c.y);
        tiles.push((c.x, c.y));
        for (nx, ny) in tiles {
            let i = map.idx(nx, ny);
            let t = &mut map.tiles[i];
            t.visible = true;
            t.seen = true;
        }
    }
    for (seen, tile) in memory.iter_mut().zip(&map.tiles) {
        *seen = tile.seen;
    }
}

fn defender_rank(t: UnitType) -> u8 {
    let d = def(t);
    (d.defense * 4 + d.attack.min(3)).clamp(0, 255) as u8
}

/// Display units with moves left first, then the selection and best defender.
/// The original view-side sprite priority remains open in reverse-engineering/stacking.md.
pub(crate) fn stack_priority(e: Entity, u: &Unit, selected: Option<Entity>) -> (bool, bool, u8) {
    (u.moves > 0, Some(e) == selected, defender_rank(u.utype))
}

fn stack_tops(
    units: &Query<(Entity, &Unit)>,
    selected: Option<Entity>,
) -> HashMap<(i32, i32), Entity> {
    let mut best: HashMap<(i32, i32), (Entity, (bool, bool, u8))> = HashMap::new();
    for (e, u) in units.iter().filter(|(_, u)| u.carrier.is_none()) {
        let key = stack_priority(e, u, selected);
        let entry = best.entry((u.x, u.y)).or_insert((e, key));
        if key > entry.1 || (key == entry.1 && e < entry.0) {
            entry.0 = e;
            entry.1 = key;
        }
    }
    best.into_iter().map(|(k, (e, _))| (k, e)).collect()
}

/// Keep stacked units on the tile center; the top unit sorts above.
pub fn restack(
    mut q: Query<(Entity, &Unit, &mut Transform)>,
    selected: Res<Selected>,
    stack_q: Query<(Entity, &Unit)>,
) {
    let tops = stack_tops(&stack_q, selected.0);
    for (e, u, mut tf) in q.iter_mut() {
        if !matches!(u.anim, UnitAnim::Idle { .. }) {
            continue;
        }
        let pos = tile_to_world(u.x, u.y);
        tf.translation.x = pos.x;
        tf.translation.y = pos.y;
        let top = tops.get(&(u.x, u.y)).is_some_and(|t| *t == e);
        tf.translation.z = sprite_z(u.x, u.y, 4.0) + if top { 0.0005 } else { 0.0 };
    }
}

/// Civ3 only lets a unit with movement left be selected.
pub fn selectable(u: &Unit) -> bool {
    u.moves > 0
}

/// A unit "requires attention": it can act and has no standing order
/// (fortify, sentry, explore, a worker job or a go-to route).
pub fn needs_orders(u: &Unit) -> bool {
    selectable(u) && u.carrier.is_none() && !u.fortified && !u.exploring && !u.auto && u.work.is_none() && u.path.is_empty()
}

/// Keep the selection while the unit can still act (or is still walking a
/// route), otherwise move to the nearest unit that needs orders, or to none:
/// with nothing selected the HUD invites the player to end the turn.
pub fn auto_select(
    units: Query<(Entity, &Unit)>,
    civs: Res<crate::civs::Civilizations>,
    mut selected: ResMut<Selected>,
    mut last_pos: Local<(i32, i32)>,
) {
    // Nothing is selected while the computer plays.
    if crate::civs::is_ai(civs.active) {
        selected.0 = None;
        return;
    }
    if let Some((_, u)) = selected.0.and_then(|s| units.get(s).ok()) {
        *last_pos = (u.x, u.y);
        if u.civ == civs.active && (needs_orders(u) || (selectable(u) && (!u.path.is_empty() || (u.carrier.is_some() && !u.sentry)))) {
            return;
        }
    }
    let (lx, ly) = *last_pos;
    selected.0 = units
        .iter()
        .filter(|(_, u)| u.civ == civs.active && needs_orders(u))
        .min_by_key(|(_, u)| (u.x - lx).abs().max((u.y - ly).abs()))
        .map(|(e, _)| e);
}

pub fn unit_visibility(
    map: Res<GameMap>,
    reveal: Res<RevealAll>,
    selected: Res<Selected>,
    stack_q: Query<(Entity, &Unit)>,
    mut q: Query<(Entity, &Unit, &mut Visibility)>,
) {
    let tops = stack_tops(&stack_q, selected.0);
    for (e, u, mut v) in q.iter_mut() {
        let seen = reveal.0 || map.get(u.x, u.y).is_some_and(|t| t.visible);
        let top = tops.get(&(u.x, u.y)).is_none_or(|t| *t == e);
        // Moving units stay visible; idle stack members hide behind the top.
        let moving = !matches!(u.anim, UnitAnim::Idle { .. });
        *v = if u.carrier.is_none() && seen && (top || moving) {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack_displays_movable_unit_before_exhausted_selection() {
        let mut app = App::new();
        let mut exhausted = scout_at(10, 10);
        exhausted.utype = UnitType::Warrior;
        exhausted.moves = 0;
        let exhausted = app.world_mut().spawn((exhausted, Visibility::Visible)).id();
        let movable = app
            .world_mut()
            .spawn((scout_at(10, 10), Visibility::Visible))
            .id();
        let mut map = GameMap::generate();
        let i = map.idx(10, 10);
        map.tiles[i].visible = true;
        app.insert_resource(map);
        app.insert_resource(RevealAll(true));
        app.insert_resource(Selected(Some(exhausted)));
        app.add_systems(Update, unit_visibility);
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(movable).unwrap(),
            Visibility::Visible
        );
        assert_eq!(
            *app.world().get::<Visibility>(exhausted).unwrap(),
            Visibility::Hidden
        );
    }

    #[test]
    fn hotseat_refreshes_only_incoming_units_and_counts_full_rounds() {
        let mut app = App::new();
        app.add_message::<TurnEnded>();
        app.add_message::<crate::civs::CivilizationEnded>();
        app.init_resource::<crate::civs::Civilizations>();
        app.init_resource::<Selected>();
        app.init_resource::<crate::actionbar::GotoMode>();
        app.insert_resource(Turn(1));
        app.add_systems(Update, end_turn_units);
        let ids: Vec<_> = (0..crate::civs::CIV_COUNT)
            .map(|civ| {
                let mut unit = scout_at(civ as i32, 0);
                unit.civ = civ;
                unit.moves = 0;
                unit.defensive_fired = true;
                app.world_mut().spawn(unit).id()
            })
            .collect();
        app.world_mut().resource_mut::<Selected>().0 = Some(ids[0]);
        for incoming in (1..crate::civs::CIV_COUNT).chain([0]) {
            app.world_mut().write_message(TurnEnded);
            app.update();
            assert_eq!(
                app.world().resource::<crate::civs::Civilizations>().active,
                incoming
            );
            assert_eq!(
                app.world().resource::<Turn>().0,
                if incoming == 0 { 2 } else { 1 }
            );
            assert_eq!(
                app.world().get::<Unit>(ids[incoming]).unwrap().moves,
                def(UnitType::Scout).moves * MP
            );
            assert_eq!(app.world().resource::<Selected>().0, None);
            assert!(!app.world().get::<Unit>(ids[incoming]).unwrap().defensive_fired);
            if incoming != 0 {
                assert_eq!(app.world().get::<Unit>(ids[0]).unwrap().moves, 0);
                assert!(app.world().get::<Unit>(ids[0]).unwrap().defensive_fired);
            }
        }
    }

    #[test]
    fn fog_is_private_and_remembers_exploration_after_handoff() {
        let map = GameMap::generate();
        let starts = crate::civs::starting_positions(&map);
        let mut app = App::new();
        app.insert_resource(map);
        app.init_resource::<crate::civs::Civilizations>();
        app.add_systems(Update, refresh_visibility);
        for (civ, (x, y)) in starts.into_iter().enumerate() {
            let mut unit = scout_at(x, y);
            unit.civ = civ;
            app.world_mut().spawn(unit);
        }
        app.update();
        let extra = {
            let map = app.world().resource::<GameMap>();
            let japan = map.idx(starts[0].0, starts[0].1);
            let rome = map.idx(starts[1].0, starts[1].1);
            assert!(map.tiles[japan].visible);
            assert!(!map.tiles[rome].seen);
            map.tiles.iter().position(|t| !t.seen).unwrap()
        };
        app.world_mut().resource_mut::<GameMap>().tiles[extra].seen = true;
        app.world_mut()
            .resource_mut::<crate::civs::Civilizations>()
            .active = 1;
        app.update();
        let map = app.world().resource::<GameMap>();
        assert!(!map.tiles[map.idx(starts[0].0, starts[0].1)].seen);
        assert!(map.tiles[map.idx(starts[1].0, starts[1].1)].visible);
        app.world_mut()
            .resource_mut::<crate::civs::Civilizations>()
            .active = 0;
        app.update();
        assert!(app.world().resource::<GameMap>().tiles[extra].seen);
    }

    #[test]
    fn facing_matches_flc_order() {
        // d0=S d1=SE d2=E d3=NE d4=N d5=NW d6=W d7=SW
        assert_eq!(facing_for_step(0, 1), 0); // south walks at the camera
        assert_eq!(facing_for_step(1, 1), 1);
        assert_eq!(facing_for_step(1, 0), 2); // east is a diagonal on screen
        assert_eq!(facing_for_step(1, -1), 3);
        assert_eq!(facing_for_step(0, -1), 4); // north is a diagonal on screen
        assert_eq!(facing_for_step(-1, -1), 5);
        assert_eq!(facing_for_step(-1, 0), 6);
        assert_eq!(facing_for_step(-1, 1), 7);
    }

    #[test]
    fn roads_make_long_walks_short() {
        let mut map = GameMap::generate();
        let (sx, sy) = map.start;
        // three road steps east of the start, all on passable land
        let run: Vec<(i32, i32)> = (0..=3).map(|d| (map.wrap_x(sx + d), sy)).collect();
        for &(x, y) in &run {
            let i = map.idx(x, y);
            map.tiles[i].base = Base::Grassland;
            map.tiles[i].relief = Relief::Hill;
            map.tiles[i].cover = Cover::Bare;
        }
        let hills = path_turns(&map, (sx, sy), MP, MP, &run[1..]);
        assert!(hills >= 2, "three hills take a 1-MP unit three turns");
        for &(x, y) in &run {
            let i = map.idx(x, y);
            map.tiles[i].road = true;
        }
        // 3 road steps cost 3 thirds: one MP, arriving this turn
        assert_eq!(path_turns(&map, (sx, sy), MP, MP, &run[1..]), 0);
        let path = map.find_path((sx, sy), run[3]).unwrap();
        assert_eq!(path.len(), 3);
        assert_eq!(fmt_moves(4), "1 1/3");
        assert_eq!(fmt_moves(2), "2/3");
        assert_eq!(fmt_moves(6), "2");
    }

    fn scout_at(x: i32, y: i32) -> Unit {
        Unit {
            civ: 0,
            utype: UnitType::Scout,
            x,
            y,
            moves: MP,
            fortified: false,
            facing: 0,
            path: VecDeque::new(),
            anim: UnitAnim::Idle { t: 0.0 },
            work: None,
            sentry: false,
            exploring: false,
            ..Unit::new(0, UnitType::Scout, x, y)
        }
    }

    #[test]
    fn explore_routes_to_unseen_tiles_then_ends() {
        let map = GameMap::generate();
        let (sx, sy) = map.start;
        let mut u = scout_at(sx, sy);
        u.exploring = true;
        assert!(plan_explore(&map, &mut u));
        assert!(!u.path.is_empty());
        // A fully seen map with no huts or camps ends the automation.
        let mut done_map = GameMap::generate();
        for t in done_map.tiles.iter_mut() {
            t.seen = true;
            t.hut = false;
            t.camp = false;
        }
        u.path.clear();
        assert!(!plan_explore(&done_map, &mut u));
    }

    #[test]
    fn exploring_units_need_no_orders_but_manual_orders_cancel() {
        let map = GameMap::generate();
        let (sx, sy) = map.start;
        let mut u = scout_at(sx, sy);
        u.exploring = true;
        assert!(!needs_orders(&u));
        order_move(&map, &mut u, (map.wrap_x(sx + 1), sy), &[]);
        assert!(!u.exploring);
    }

    #[test]
    fn path_turns_counts_turn_boundaries() {
        let map = GameMap::generate();
        let (sx, sy) = map.start;
        // straight run over land tiles found near the start
        let mut path = vec![];
        for dx in 1..=3 {
            if map.get(sx + dx, sy).and_then(move_cost).is_some() {
                path.push((sx + dx, sy));
            }
        }
        if path.len() == 3 {
            let costs: u32 = path
                .iter()
                .map(|&(x, y)| map.get(x, y).and_then(move_cost).unwrap() as u32)
                .sum();
            assert!(path_turns(&map, (sx, sy), MP, MP, &path) >= 2 || costs > 3);
            assert_eq!(path_turns(&map, (sx, sy), 0, MP, &[]), 0);
        }
        // a full-move unit with nothing left waits a turn for a 1-step path
        assert_eq!(path_turns(&map, (sx, sy), 0, MP, &[(sx + 1, sy)]), 1);
    }

    fn ring_app(selected: Selected) -> (App, Entity, Entity) {
        let mut app = App::new();
        app.insert_resource(SelectionRing {
            image: Handle::default(),
            frame: Vec2::new(93.0, 46.0),
            frames: 31,
            ms: 175.0,
        });
        app.insert_resource(Time::<()>::default());
        let unit = app
            .world_mut()
            .spawn((scout_at(10, 10), Visibility::Visible, Transform::default()))
            .id();
        let ring = app
            .world_mut()
            .spawn((
                Sprite::default(),
                Visibility::Hidden,
                Transform::default(),
                SelectionRingSprite,
            ))
            .id();
        app.insert_resource(selected);
        app.add_systems(Update, ring_follow);
        (app, unit, ring)
    }

    #[test]
    fn the_ring_hides_without_a_selection() {
        let (mut app, _, ring) = ring_app(Selected(None));
        app.update();
        let vis = app.world().entity(ring).get::<Visibility>().unwrap();
        assert_eq!(*vis, Visibility::Hidden);
    }

    #[test]
    fn the_ring_sits_under_the_selected_unit() {
        let (mut app, unit, ring) = ring_app(Selected(None));
        app.world_mut().resource_mut::<Selected>().0 = Some(unit);
        app.update();
        let vis = app.world().entity(ring).get::<Visibility>().unwrap();
        assert_eq!(*vis, Visibility::Visible);
        let at = tile_to_world(10, 10);
        let tf = app.world().entity(ring).get::<Transform>().unwrap();
        assert_eq!((tf.translation.x, tf.translation.y), (at.x, at.y));
    }
}

/// The white ellipse Civ3 draws under the selected unit. The game ships no
/// Civ3's selection marker: the dashed ellipse under the selected unit,
/// which lives in `Art/Animations/Cursor/Cursor.flc` (prep stage `cursor`,
/// 31 frames of a 175 ms crawl, 93x46 each).
#[derive(Resource)]
pub struct SelectionRing {
    image: Handle<Image>,
    frame: Vec2,
    frames: u32,
    ms: f32,
}

#[derive(Deserialize)]
struct RingEntry {
    file: String,
    frame: [u32; 2],
    frames: u32,
    ms: f32,
}

impl SelectionRing {
    pub fn load(asset_server: &AssetServer) -> Self {
        let text = fs::read_to_string("assets/gen/cursor/manifest.json")
            .expect("run from the repo root after tools/prep_assets.py cursor");
        let raw: HashMap<String, RingEntry> =
            serde_json::from_str(&text).expect("cursor manifest parses");
        let e = &raw["ring"];
        Self {
            image: asset_server.load(format!("gen/cursor/{}", e.file)),
            frame: Vec2::new(e.frame[0] as f32, e.frame[1] as f32),
            frames: e.frames,
            ms: e.ms,
        }
    }

    /// Frame `f` of the horizontal strip.
    fn rect(&self, f: u32) -> Rect {
        let x = f as f32 * self.frame.x;
        Rect::new(x, 0.0, x + self.frame.x, self.frame.y)
    }
}

#[derive(Component)]
pub struct SelectionRingSprite;

/// The ring starts hidden; `ring_follow` shows it under the selection.
pub fn spawn_selection_ring(mut commands: Commands, ring: Res<SelectionRing>) {
    commands.spawn((
        Sprite {
            image: ring.image.clone(),
            rect: Some(ring.rect(0)),
            ..default()
        },
        Visibility::Hidden,
        Transform::default(),
        SelectionRingSprite,
    ));
}

/// Keep the ring under the selected unit's feet, mirroring the unit's own
/// visibility so a unit hidden by fog or under a stack hides its ring too.
pub fn ring_follow(
    time: Res<Time>,
    ring: Res<SelectionRing>,
    selected: Res<Selected>,
    units: Query<(&Unit, &Visibility)>,
    mut q: Query<
        (&mut Sprite, &mut Transform, &mut Visibility),
        (With<SelectionRingSprite>, Without<Unit>),
    >,
    mut crawl: Local<f32>,
) {
    let Ok((mut sprite, mut tf, mut vis)) = q.single_mut() else {
        return;
    };
    let Some((u, unit_vis)) = selected.0.and_then(|s| units.get(s).ok()) else {
        *vis = Visibility::Hidden;
        return;
    };
    // The dashes crawl around the ellipse: advance one frame every
    // `ring.ms`, as the FLC's own timing does.
    *crawl += time.delta_secs() * 1000.0;
    if ring.ms > 0.0 {
        let f = (*crawl / ring.ms) as u32 % ring.frames;
        let rect = ring.rect(f);
        if sprite.rect != Some(rect) {
            sprite.rect = Some(rect);
        }
    }
    let pos = tile_to_world(u.x, u.y);
    // On the unit's feet (the art's ellipse is centered in its cell) and
    // just below the unit sprites, which sit at layer 4.0.
    tf.translation = Vec3::new(pos.x, pos.y, sprite_z(u.x, u.y, 3.5));
    *vis = if *unit_vis == Visibility::Visible {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
}

pub fn selection_gizmo(
    selected: Res<Selected>,
    units: Query<(Entity, &Unit)>,
    map: Res<GameMap>,
    preview: Res<crate::input::MovePreview>,
    cities: Query<&crate::cities::City>,
    mut gizmos: Gizmos,
) {
    let Some(s) = selected.0 else {
        return;
    };
    let Ok((_, u)) = units.get(s) else {
        return;
    };
    for (x, y) in u.path.iter() {
        gizmos.circle_2d(tile_to_world(*x, *y), 4.0, Color::WHITE);
    }
    // The route a Go-to, or a held press, would take. Nothing is drawn for
    // a plain hover: Civ3 only shows the path while it is being aimed.
    let Some(dest) = preview.0 else {
        return;
    };
    if dest == (u.x, u.y) {
        return;
    }
    let ports: Vec<_> = cities.iter().filter(|c| c.civ == u.civ && c.coastal).map(|c| (c.x, c.y)).collect();
    let snapshot: Vec<_> = units.iter().map(|(e, u)| (e, u.clone())).collect();
    if let Some(p) = crate::naval::route(&map, u, dest, &ports, &snapshot) {
        let mut prev = tile_to_world(u.x, u.y);
        for (x, y) in p {
            let cur = tile_to_world(x, y);
            gizmos.line_2d(prev, cur, Color::srgba(1.0, 1.0, 0.3, 0.8));
            prev = cur;
        }
        gizmos.circle_2d(prev, 8.0, Color::srgb(1.0, 1.0, 0.3));
    }
}
