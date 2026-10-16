//! The stack fight on a field of its own.
//!
//! The `X` order resolves a whole stack in one step and the record says what happened in every
//! lane. This module plays that record the way the map plays one duel (see
//! `docs/combat-animation.md`): each side's units stand drawn up in ranks facing the other, the
//! pairs the record fought take the front one duel after another, and every round is the same
//! attack motion the map's fights use, with the blow landing at [`STRIKE_AT`] of the way through
//! it. A beaten unit falls where it stood while the next in line steps up over it.
//!
//! Nothing here writes to the game: the snapshot is final already, so *Skip* (or Escape) simply
//! closes the field, and nothing is lost by watching only part of it.
use super::*;
use crate::puppet::{ClipArt, Puppetry};
use crate::stage::{Motion, Pose, STRIKE_AT};
use crate::theme::{BAD, CARD, GOOD, ON_PLATE, SPENT, Tone, bold, button, hp_color, rounded};
use bevy::image::TRANSPARENT_IMAGE_HANDLE;
use bevy::math::Rot2;
use bevy::ui::{FocusPolicy, UiTransform, Val2};
use fourx_content::animation::{Clip, facing};
use fourx_content::combat::Style;
use fourx_sim::stacks::{Lane as Record, StackBattle};
use fourx_sim::{Fighter, Flavor, Id, Outcome};
use std::collections::HashSet;

/// Seconds the two lines stand facing each other before the first blow: the units fade in on
/// the ground.
const LEAD: f32 = 0.60;
/// One beat: both fighters swing once. It is the length of an attack motion, so no swing is cut
/// short, and the blow lands [`STRIKE_AT`] of the way through it.
const BEAT: f32 = 0.80;
/// The beat a defending gun fires its support shot on, before the first round of a duel.
const SHOT: f32 = 0.75;
/// How long a beaten land unit takes to topple and fade (the death clip's own length).
const FALL: f32 = 0.95;
/// The same for a ship, which lists, goes under and leaves a slick.
const SINK: f32 = 1.20;
/// The winner's cheer after a duel is decided (the victory clip's own length).
const CHEER: f32 = 0.70;
/// A unit crossing between the front and its own place in the ranks.
const STRIDE: f32 = 0.42;
/// A flinch: the short recoil of the fighter that took the blow, as on the map.
const FLINCH: f32 = 0.20;
/// The white-hot flicker of a blow landing.
const FLASH: f32 = 0.16;
/// Hold after the last duel is decided, before the board can be closed.
const TAIL: f32 = 1.20;
/// How large a unit's cell is drawn against the art, and so how large the map's own diamonds
/// are drawn under it (the map draws a unit at about half this).
const DRAW: f32 = 0.95;
/// Where a unit's feet sit inside its cell: land art stands on the tile centre at `(80, 108)`
/// of a 160 px cell, ship art at `(128, 123)` of 256 x 192.
const LAND_ANCHOR: Vec2 = Vec2::new(0.5, 0.675);
const SEA_ANCHOR: Vec2 = Vec2::new(0.5, 0.640625);
/// Room the field leaves past its outermost rank, as a share of a unit's cell.
const MARGIN: f32 = 0.16;
/// The hit-point bar under a unit: how wide it is, and the gap between its pips.
const BAR: f32 = 58.0;
const PIP: f32 = 6.0;
const PIP_GAP: f32 = 3.0;
/// A ship in a fight is turned beam-on, so that its guns bear on the foe (see
/// `docs/combat-animation.md` §7).
const BEAM_ON: u32 = 2;

/// Which of the two lines a unit stands in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Side {
    Attacker,
    Defender,
}

impl Side {
    /// Which way it looks: at its foe, east for the attackers and west for the defenders.
    fn face(self) -> u32 {
        facing(
            match self {
                Side::Attacker => 1.0,
                Side::Defender => -1.0,
            },
            0.0,
        )
    }

    /// Where the foe lies, as a screen direction (`+y` up, as a world).
    fn toward(self) -> Vec2 {
        match self {
            Side::Attacker => Vec2::X,
            Side::Defender => -Vec2::X,
        }
    }

    fn index(self) -> usize {
        match self {
            Side::Attacker => 0,
            Side::Defender => 1,
        }
    }
}

// --- the record, laid out in time --------------------------------------------------------------

/// The defensive shot a gun of the defender's fires at a duel before its first round.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Shot {
    id: Id,
    at: f32,
}

/// One duel of the record, with every moment it plays at, in the field's own seconds.
#[derive(Clone, Debug, PartialEq)]
struct Duel {
    /// The lane record it plays.
    record: usize,
    /// The lane whose front it takes.
    lane: usize,
    /// When its pair steps onto that front (zero for the pair that opens the battle).
    arrive: f32,
    /// The support shot that covers it, if a gun of the defender's is in range.
    shot: Option<Shot>,
    /// When the first round's beat begins.
    first: f32,
    /// When each round's blow lands, in order.
    blows: Vec<f32>,
    /// The blow that decided it.
    decided: f32,
    /// When its pair gives the front up to the next in line.
    relief: f32,
}

impl Duel {
    /// When the support shot's own blow lands.
    fn shot_blow(&self) -> Option<f32> {
        self.shot.map(|shot| shot.at + STRIKE_AT * SHOT)
    }

    /// How many blows this duel has in it: one per round, and one for a support shot.
    fn blows_total(&self) -> usize {
        self.blows.len() + usize::from(self.shot.is_some())
    }
}

/// How long a beaten unit takes to go down: a ship sinks slower than a man falls.
fn drop(sea: bool) -> f32 {
    if sea { SINK } else { FALL }
}

/// Lay the record out in time. The lanes fight side by side; the duels of one lane, one after
/// another, because that is the order the record holds them in. Returns the duels in the order
/// they play, and how long the whole fight takes.
fn schedule(battle: &StackBattle, sea: bool) -> (Vec<Duel>, f32) {
    let mut duels: Vec<Duel> = Vec::new();
    let mut span: f32 = 0.0;
    for lane in 0..battle.width.max(1) {
        let mut records: Vec<usize> = (0..battle.lanes.len())
            .filter(|i| battle.lanes[*i].lane == lane)
            .collect();
        records.sort_by_key(|i| (battle.lanes[*i].start, *i));
        // The pair that opens a lane is standing on its front from the first frame; every later
        // pair steps in at the moment the one before it has finished going down.
        let mut arrive = 0.0;
        let mut at = LEAD;
        let mut last = None;
        for record in records {
            let shot = battle.lanes[record].support.as_ref().map(|support| Shot {
                id: support.shooter.id,
                at,
            });
            if shot.is_some() {
                at += SHOT;
            }
            let first = at;
            let mut blows = Vec::with_capacity(battle.lanes[record].rounds.len());
            for _ in 0..battle.lanes[record].rounds.len() {
                blows.push(at + STRIKE_AT * BEAT);
                at += BEAT;
            }
            let decided = *blows.last().expect("a duel fought has a round");
            let relief = decided + drop(sea);
            last = Some(duels.len());
            duels.push(Duel {
                record,
                lane,
                arrive,
                shot,
                first,
                blows,
                decided,
                relief,
            });
            arrive = relief;
            at = relief + STRIDE;
        }
        if let Some(last) = last {
            span = span.max(duels[last].relief + CHEER + TAIL);
        }
    }
    (duels, span)
}

/// How long the whole replay takes, in seconds: what [`super::campaign::animate`]'s clock runs
/// for.
pub(super) fn span(battle: &StackBattle, rules: &Rules) -> f32 {
    schedule(battle, at_sea(battle, rules)).1
}

// --- the field ---------------------------------------------------------------------------------

/// Where everything stands, in field coordinates: the middle of the field is the origin, `+x`
/// is east and `+y` is down the screen.
#[derive(Clone, Debug)]
struct Field {
    size: Vec2,
    /// The size one unit's cell is drawn at.
    cell: Vec2,
    /// Half the distance between the two front ranks.
    half: f32,
    /// The middle of each lane's front.
    lane_at: Vec<f32>,
    /// The ranks waiting behind each side, in the order its units were first met.
    back: [Vec<Vec2>; 2],
}

impl Field {
    /// Where a lane's attacker or defender stands on the front.
    fn front(&self, lane: usize, side: Side) -> Vec2 {
        let sign = if side == Side::Attacker { -1.0 } else { 1.0 };
        Vec2::new(sign * self.half, self.lane_at[lane])
    }
}

/// The order of battle of one side: every fighter that appears in the record, in the order it
/// is first met, the gun that fires a support shot counted among them.
fn order(battle: &StackBattle, attackers: bool) -> Vec<Fighter> {
    let mut seen = HashSet::new();
    let mut roster = Vec::new();
    let mut push = |fighter: &Fighter| {
        if seen.insert(fighter.id) {
            roster.push(fighter.clone());
        }
    };
    for lane in &battle.lanes {
        if attackers {
            push(&lane.attacker);
        } else {
            push(&lane.defender);
            if let Some(support) = &lane.support {
                push(&support.shooter);
            }
        }
    }
    roster
}

/// Whether a fight is at sea, which decides the ground, the art and how a beaten unit goes down.
fn at_sea(battle: &StackBattle, rules: &Rules) -> bool {
    battle.lanes.iter().any(|lane| {
        rules
            .units
            .get(&lane.attacker.kind)
            .is_some_and(|def| def.is_naval())
    })
}

/// How large a unit's cell is drawn: the art is 160 px square for land units and 256 x 192 for
/// ships, and both are drawn at about twice the map's size, where the art is still crisp.
fn cell_size(sea: bool) -> Vec2 {
    if sea {
        Vec2::new(256.0, 192.0) * DRAW
    } else {
        Vec2::splat(160.0) * DRAW
    }
}

/// Lay the field out, and fit it to the window: the two front ranks facing each other across
/// the middle, the spare ranks behind their own line, and the ground they all need.
fn field(battle: &StackBattle, sea: bool, size: Vec2) -> Field {
    let lanes = battle.width.max(1);
    let ranks = [order(battle, true), order(battle, false)];
    // Spare units stand in columns behind their own front rank. Two columns at the most, so
    // that a long line does not stretch the field off the screen.
    let columns = |n: usize| n.div_ceil(lanes).clamp(1, 2);
    let columns_back = [columns(ranks[0].len()), columns(ranks[1].len())];
    let rows = lanes.max(
        ranks
            .iter()
            .zip(columns_back)
            .map(|(side, columns)| side.len().div_ceil(columns))
            .max()
            .unwrap_or(lanes),
    );
    // Everything is worked out at full size, then shrunk to fit what the window leaves.
    let row = cell_size(sea).y * 0.96;
    let gap = cell_size(sea).x * 0.98;
    let half = cell_size(sea).x * 0.78;
    let rows_f = rows as f32;
    let wide = half + gap * columns_back.iter().copied().max().unwrap_or(1) as f32;
    // The field has to hold a unit's whole cell: most of the art stands above its anchor, and
    // the hit-point bar hangs under it.
    let anchor = if sea { SEA_ANCHOR } else { LAND_ANCHOR }.y;
    let pad = cell_size(sea).y * (anchor.max(1.0 - anchor) + MARGIN);
    let natural = Vec2::new(
        2.0 * (wide + cell_size(sea).x * (0.5 + MARGIN)),
        (rows_f - 1.0) * row + 2.0 * pad,
    );
    let room = (size - Vec2::new(80.0, 200.0)).max(Vec2::splat(240.0));
    let fit = (room / natural).min_element().clamp(0.5, 1.0);
    let (cell, row, gap, half) = (cell_size(sea) * fit, row * fit, gap * fit, half * fit);
    let row_at = |index: usize| (index as f32 - (rows_f - 1.0) * 0.5) * row;
    let mut out = Field {
        size: natural * fit,
        cell,
        half,
        lane_at: (0..lanes).map(row_at).collect(),
        back: [Vec::new(), Vec::new()],
    };
    for (side, (roster, columns)) in ranks.iter().zip(columns_back).enumerate() {
        let sign = if side == 0 { -1.0 } else { 1.0 };
        for index in 0..roster.len() {
            let column = index % columns.max(1);
            let row = index / columns.max(1);
            out.back[side].push(Vec2::new(
                sign * (half + gap * (column + 1) as f32),
                row_at(row),
            ));
        }
    }
    out
}

// --- the units ---------------------------------------------------------------------------------

/// One place a fighter stands for a stretch of time: the front of a lane, while it fights the
/// duel there, or its own place in the ranks.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Stint {
    at: Vec2,
    from: f32,
    to: f32,
    /// The duel it is fighting while it stands here.
    duel: Option<usize>,
}

/// One clip of a unit's art, with the nation-colour sheet that goes over it.
#[derive(Clone)]
struct Layer {
    art: ClipArt,
    accent: Option<Handle<Image>>,
}

/// What a unit draws with: the clips of its design, or the single image of a design drawn
/// without any.
#[derive(Clone)]
enum Art {
    Clips {
        idle: Layer,
        run: Layer,
        attack: Layer,
        victory: Layer,
        death: Layer,
    },
    Still {
        image: Handle<Image>,
        accent: Option<Handle<Image>>,
    },
}

impl Art {
    fn get(&self, clip: Clip) -> Option<&Layer> {
        match self {
            Art::Clips {
                idle,
                run,
                attack,
                victory,
                death,
            } => Some(match clip {
                Clip::Idle => idle,
                Clip::Run => run,
                Clip::Attack => attack,
                Clip::Victory => victory,
                Clip::Death => death,
            }),
            Art::Still { .. } => None,
        }
    }

    /// The single image of a design drawn without clips, and the nation colour sheet that goes
    /// over it, if any.
    fn still(&self) -> Option<(&Handle<Image>, Option<Handle<Image>>)> {
        match self {
            Art::Still { image, accent } => Some((image, accent.clone())),
            Art::Clips { .. } => None,
        }
    }
}

/// Everything the field knows about one fighter: where it stands and when, what it draws, and
/// what the record had it do.
struct Actor {
    owner: Id,
    side: Side,
    art: Art,
    cell: Vec2,
    anchor: Vec2,
    hp: i32,
    max_hp: i32,
    style: Style,
    face: u32,
    /// Every place it stands, in order: the ranks, the fronts it fought on, the ranks again.
    stints: Vec<Stint>,
    /// The support shots it fires, as the window each one is loosed in.
    shots: Vec<(f32, f32)>,
    /// When it goes down, and whether it sinks.
    end: Option<(f32, bool)>,
    /// Seconds woven into its idle loop, so neighbours do not breathe in step.
    phase: f32,
}

impl Actor {
    fn clips(&self) -> bool {
        matches!(self.art, Art::Clips { .. })
    }

    /// The stint it is standing in at `t`.
    fn stint(&self, t: f32) -> Option<&Stint> {
        let index = self.stints.iter().rposition(|stint| stint.from <= t)?;
        self.stints.get(index)
    }

    /// Where it stands at `t`, and the crossing it is making, if it is between two places. The
    /// crossing is the step it takes: the offset it moves by, and how far through it is.
    fn place(&self, t: f32) -> (Vec2, Option<(Vec2, f32)>) {
        let Some(index) = self.stints.iter().rposition(|stint| stint.from <= t) else {
            return (Vec2::ZERO, None);
        };
        let stint = self.stints[index];
        if let Some(previous) = index.checked_sub(1).map(|i| self.stints[i])
            && previous.at != stint.at
        {
            let u = ((t - stint.from) / STRIDE).clamp(0.0, 1.0);
            let by = stint.at - previous.at;
            if u < 1.0 {
                return (previous.at + by * smooth(u), Some((by, u)));
            }
        }
        (stint.at, None)
    }

    /// Whether it lost that round of a duel.
    fn took(&self, record: &Record, round: usize) -> bool {
        let attacker_won = record.rounds.get(round).copied().unwrap_or(true);
        attacker_won == (self.side == Side::Defender)
    }

    /// The hit points it has left at `t`: one for every blow that has landed on it, the support
    /// shot's included.
    fn hp_at(&self, duel: &Duel, record: &Record, t: f32) -> i32 {
        if self.end.is_some_and(|(at, _)| t >= at) {
            return 0;
        }
        let mut taken = 0;
        if self.side == Side::Attacker
            && record.support.as_ref().is_some_and(|shot| shot.hit)
            && duel.shot_blow().is_some_and(|blow| t >= blow)
        {
            taken += 1;
        }
        for (round, blow) in duel.blows.iter().enumerate() {
            if t >= *blow && self.took(record, round) {
                taken += 1;
            }
        }
        (self.hp - taken).max(0)
    }

    /// The frame of a clip to draw at this moment. Looping clips run off the wall clock, so they
    /// keep their own rhythm; the others play once, over the motion's progress.
    fn frame(&self, clip: Clip, progress: f32, clock: f32) -> u32 {
        let Some(layer) = self.art.get(clip) else {
            return 0;
        };
        if clip.loops() {
            layer.art.looping(clock + self.phase)
        } else {
            layer.art.once(progress)
        }
    }
}

fn smooth(u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

/// A whole board, ready to draw: the field, the schedule, and every unit standing on it.
struct Plan {
    sea: bool,
    span: f32,
    /// Where the field's own origin sits on the screen, for the animator to place units.
    corner: Vec2,
    field: Field,
    duels: Vec<Duel>,
    records: Vec<Record>,
    actors: Vec<Actor>,
    /// Every blow of the whole fight, for the count in the header.
    blows: usize,
}

impl Plan {
    /// Blows that have landed by `t`.
    fn blows_at(&self, t: f32) -> usize {
        self.duels
            .iter()
            .map(|duel| {
                duel.blows.iter().filter(|blow| **blow <= t).count()
                    + usize::from(duel.shot_blow().is_some_and(|blow| blow <= t))
            })
            .sum()
    }

    /// Units of a side lost by `t`: a duel is lost by the side its outcome went against.
    fn losses_at(&self, t: f32, side: Side) -> usize {
        self.duels
            .iter()
            .filter(|duel| duel.decided <= t)
            .filter(|duel| match side {
                Side::Attacker => self.records[duel.record].outcome == Outcome::DefenderWon,
                Side::Defender => self.records[duel.record].outcome == Outcome::AttackerWon,
            })
            .count()
    }
}

/// Work the board out of the record: the field, the schedule, and every unit standing on it.
fn plan(
    battle: &StackBattle,
    sea: bool,
    size: Vec2,
    session: &Session,
    assets: &AssetServer,
    puppetry: &mut Puppetry,
) -> Plan {
    let (duels, span) = schedule(battle, sea);
    let field = field(battle, sea, size);
    let ranks = [order(battle, true), order(battle, false)];
    let mut actors = Vec::new();
    for (side, roster) in [Side::Attacker, Side::Defender].into_iter().zip(ranks) {
        for (index, fighter) in roster.iter().enumerate() {
            let back = field.back[side.index()]
                .get(index)
                .copied()
                .unwrap_or(Vec2::ZERO);
            let mut stints: Vec<Stint> = Vec::new();
            let mut shots: Vec<(f32, f32)> = Vec::new();
            let mut end = None;
            let mut cursor = 0.0;
            for (index, duel) in duels.iter().enumerate() {
                let record = &battle.lanes[duel.record];
                if duel.shot.is_some_and(|shot| shot.id == fighter.id) {
                    let at = duel.shot.expect("checked").at;
                    shots.push((at, at + SHOT));
                }
                let holds = match side {
                    Side::Attacker => record.attacker.id == fighter.id,
                    Side::Defender => record.defender.id == fighter.id,
                };
                if !holds {
                    continue;
                }
                if duel.arrive > cursor {
                    stints.push(Stint {
                        at: back,
                        from: cursor,
                        to: duel.arrive,
                        duel: None,
                    });
                }
                // A beaten unit keeps the front only while it goes down; a survivor holds it
                // until the next pair steps up.
                let beaten = match side {
                    Side::Attacker => record.outcome == Outcome::DefenderWon,
                    Side::Defender => record.outcome == Outcome::AttackerWon,
                };
                // A beaten unit keeps the front only while it goes down. A survivor holds it
                // until the next pair steps up — or, if the lane is done, to the end of the
                // fight, so the winners are left standing on the ground they won.
                let taken_over = duels
                    .iter()
                    .any(|other| other.lane == duel.lane && other.arrive > duel.relief - 0.001);
                let to = if beaten {
                    duel.decided
                } else if taken_over {
                    duel.relief
                } else {
                    span
                };
                if beaten {
                    end = Some((duel.decided, sea));
                }
                stints.push(Stint {
                    at: field.front(duel.lane, side),
                    from: duel.arrive.max(cursor),
                    to,
                    duel: Some(index),
                });
                cursor = to;
                if beaten {
                    break;
                }
            }
            if end.is_none() && cursor < span {
                stints.push(Stint {
                    at: back,
                    from: cursor,
                    to: span,
                    duel: None,
                });
            }
            let def = session.rules.units.get(&fighter.kind);
            let style = def.map_or(Style::Volley, |def| {
                session.pack.visuals.combat.style_of(def)
            });
            let flavor = session
                .game
                .as_ref()
                .and_then(|game| game.factions.get(&fighter.owner))
                .map_or(Flavor::Western, |faction| faction.flavor);
            actors.push(Actor {
                owner: fighter.owner,
                side,
                art: unit_art(session, assets, puppetry, &fighter.kind, flavor),
                cell: cell_size(sea),
                anchor: if sea { SEA_ANCHOR } else { LAND_ANCHOR },
                hp: fighter.hp,
                max_hp: fighter.max_hp.max(1),
                style,
                face: if sea {
                    (side.face() + BEAM_ON) % 8
                } else {
                    side.face()
                },
                stints,
                shots,
                end,
                phase: (fighter.id as f32 * 0.618_034).fract() * 9.0,
            });
        }
    }
    let blows = duels.iter().map(Duel::blows_total).sum();
    Plan {
        sea,
        span,
        corner: Vec2::ZERO,
        field,
        duels,
        records: battle.lanes.clone(),
        actors,
        blows,
    }
}

/// A unit's art: the clips the pack draws it with, else its single image.
fn unit_art(
    session: &Session,
    assets: &AssetServer,
    puppetry: &mut Puppetry,
    kind: &str,
    flavor: Flavor,
) -> Art {
    let Some(set) = puppetry
        .art
        .set(session, assets, &mut puppetry.layouts, kind, flavor)
    else {
        let path = session
            .pack
            .visuals
            .unit(kind, flavor)
            .map(|visual| visual.sprite.clone())
            .or_else(|| session.rules.units.get(kind).map(|def| def.sprite.clone()))
            .unwrap_or_default();
        let image: Handle<Image> = assets.load(format!("{}{path}", session.asset_prefix));
        let accent = puppetry.art.accent(assets, &image);
        return Art::Still { image, accent };
    };
    let layer = |clip: Clip| {
        let art: ClipArt = set.get(clip).clone();
        let accent = puppetry.art.accent(assets, &art.image);
        Layer { art, accent }
    };
    Art::Clips {
        idle: layer(Clip::Idle),
        run: layer(Clip::Run),
        attack: layer(Clip::Attack),
        victory: layer(Clip::Victory),
        death: layer(Clip::Death),
    }
}

// --- drawing -----------------------------------------------------------------------------------

/// One piece of the field: a sprite's base, the nation colour over it, the shadow it stands on,
/// a pip of its hit-point bar, or the flash of a blow landing on it. The animator moves and
/// colours them; only the plan says where they belong.
#[derive(Component)]
pub(super) struct Piece {
    actor: usize,
    role: Role,
    /// Where the unit stood when this piece was spawned, so the animator can offset it.
    spawn: Vec2,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Role {
    Shadow,
    Base,
    Accent,
    Flash,
    /// One pip of a unit's hit-point bar, floating under it as in the map's own fights.
    Pip(usize),
}

/// The board on screen: the plan the animator poses it by, and the button's two states.
#[derive(Resource, Default)]
pub(super) struct Board {
    plan: Option<Plan>,
}

/// The line that counts the fight out, the note that closes it, the losses under each line, and
/// the two words the one button wears.
#[derive(Component)]
pub(super) struct Round;
#[derive(Component)]
pub(super) struct Note;
#[derive(Component)]
pub(super) struct Loss(Side);
#[derive(Component)]
pub(super) struct Skip;
#[derive(Component)]
pub(super) struct Carry;

/// Spawn the field, its units and its labels. Called from the map's own refresh, so that the
/// board is rebuilt whenever the interface is.
#[allow(clippy::too_many_arguments)]
pub(super) fn spawn(
    commands: &mut Commands,
    assets: &AssetServer,
    session: &Session,
    game: &Game,
    size: Vec2,
    puppetry: &mut Puppetry,
    board: &mut Board,
    soft: Handle<Image>,
) {
    let Some((battle, _)) = session.stack_replay.as_ref() else {
        board.plan = None;
        return;
    };
    let sea = at_sea(battle, &session.rules);
    let mut plan = plan(battle, sea, size, session, assets, puppetry);
    let field = plan.field.size;
    let corner = Vec2::new(
        ((size.x - field.x) * 0.5).max(10.0),
        ((size.y - field.y) * 0.5 + 36.0).max(78.0),
    );
    plan.corner = corner;
    let city = game
        .city_at(battle.target)
        .and_then(|id| game.cities.get(&id))
        .map(|city| city.name.clone())
        .unwrap_or_else(|| format!("({}, {})", battle.target.x, battle.target.y));
    let (owner, enemy) = (
        game.factions[&battle.owner].name.clone(),
        game.factions[&battle.enemy].name.clone(),
    );
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(0),
                width: percent(100),
                height: percent(100),
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.03, 0.05, 0.74)),
            FocusPolicy::Block,
            GlobalZIndex(64),
            UiRoot,
        ))
        .with_children(|p| {
            // The place being fought over, over the field.
            p.spawn(Node {
                position_type: PositionType::Absolute,
                left: px(0),
                // Clear of the map's own cards at the head of the screen.
                top: px((corner.y - 74.0).clamp(96.0, 240.0)),
                width: percent(100),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(2),
                ..default()
            })
            .with_children(|p| {
                p.spawn((
                    Text::new("ASSAULT ON"),
                    TextFont {
                        font_size: 10.5,
                        ..default()
                    },
                    TextColor(ON_PLATE.with_alpha(0.6)),
                ));
                bold(p, city, 23.0, ON_PLATE);
                p.spawn((
                    Round,
                    Text::new(""),
                    TextFont {
                        font_size: 12.5,
                        ..default()
                    },
                    TextColor(ON_PLATE.with_alpha(0.72)),
                ));
            });
            p.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(corner.x),
                    top: px(corner.y),
                    width: px(field.x),
                    height: px(field.y),
                    overflow: Overflow::clip(),
                    ..default()
                },
                BackgroundColor(CARD),
            ))
            .with_children(|p| {
                floor(
                    p,
                    session,
                    assets,
                    battle,
                    &plan.field,
                    puppetry,
                    soft.clone(),
                );
                // The ranks behind first: the lines in front draw over them.
                let mut order: Vec<usize> = (0..plan.actors.len()).collect();
                order.sort_by(|a, b| {
                    let place = |actor: &Actor| {
                        actor
                            .stints
                            .iter()
                            .map(|stint| stint.at.y)
                            .fold(f32::NEG_INFINITY, f32::max)
                    };
                    place(&plan.actors[*a])
                        .partial_cmp(&place(&plan.actors[*b]))
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                for index in order {
                    unit(p, game, &plan, index, soft.clone());
                }
            });
            // The two orders of battle, named under their own lines.
            for (side, label) in [(Side::Attacker, owner), (Side::Defender, enemy)] {
                let at_left = side == Side::Attacker;
                p.spawn((
                    Loss(side),
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(if at_left {
                            corner.x
                        } else {
                            corner.x + field.x - 320.0
                        }),
                        top: px(corner.y + field.y + 11.0),
                        width: px(320.0),
                        justify_content: if at_left {
                            JustifyContent::FlexStart
                        } else {
                            JustifyContent::FlexEnd
                        },
                        ..default()
                    },
                    Text::new(label),
                    TextFont {
                        font_size: 12.5,
                        ..default()
                    },
                    TextColor(ON_PLATE.with_alpha(0.8)),
                ));
            }
            p.spawn(Node {
                position_type: PositionType::Absolute,
                left: px(0),
                bottom: px(12),
                width: percent(100),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(8),
                ..default()
            })
            .with_children(|p| {
                p.spawn((
                    Note,
                    Text::new(""),
                    TextFont {
                        font_size: 16.0,
                        ..default()
                    },
                    TextColor(ON_PLATE),
                ));
                p.spawn((Skip, Node::default())).with_children(|p| {
                    button(p, "Skip battle [Esc]", Action::CloseBattle, Tone::Primary);
                });
                p.spawn((Carry, Node::default(), Visibility::Hidden))
                    .with_children(|p| {
                        button(p, "Continue", Action::CloseBattle, Tone::Primary);
                    });
            });
        });
    board.plan = Some(plan);
}

/// The field's floor: the interface's own parchment, mottled with soft patches a little
/// darker and a little lighter, so it reads as a card rather than a painted board.
fn floor(
    parent: &mut ChildSpawnerCommands,
    session: &Session,
    assets: &AssetServer,
    battle: &StackBattle,
    field: &Field,
    puppetry: &Puppetry,
    soft: Handle<Image>,
) {
    let ground = CARD;
    let size = field.size;
    // The ground itself: one tone all over, so that nothing drawn on it could be mistaken
    // for part of a unit.
    parent.spawn((
        BackgroundColor(ground),
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            top: px(0),
            width: percent(100),
            height: percent(100),
            ..default()
        },
        ZIndex(0),
    ));
    // Broad soft drifts first, then smaller worn patches over them.
    let drifts = [0.84, 1.18, 0.88, 1.15, 0.86, 1.12, 0.90, 1.16, 0.85, 1.13];
    for (index, tone) in drifts.into_iter().enumerate() {
        blotch(
            parent,
            &soft,
            ground,
            tone,
            0.60,
            size,
            presentation::scatter(Coord::new(index as i32, 11)),
            0.22,
            0.40,
        );
    }
    let worn = [
        0.86, 1.14, 0.89, 1.11, 0.87, 1.13, 0.90, 1.10, 0.88, 1.12, 0.86, 1.14, 0.89, 1.11, 0.87,
        1.10,
    ];
    for (index, tone) in worn.into_iter().enumerate() {
        blotch(
            parent,
            &soft,
            ground,
            tone,
            0.50,
            size,
            presentation::scatter(Coord::new(index as i32, 29)),
            0.07,
            0.15,
        );
    }
    // A fight in the woods is fought at the edge of one: the country's own trees, hung over
    // the far side of the field.
    fringe(parent, session, assets, battle, field);
    let _ = puppetry;
}

/// The pack's own trees along the far edge of the field, when the fight is in woodland: they
/// hang over the edge and are clipped by it, so the ground reads as a clearing.
fn fringe(
    parent: &mut ChildSpawnerCommands,
    session: &Session,
    assets: &AssetServer,
    battle: &StackBattle,
    field: &Field,
) {
    use fourx_sim::terrain::Cover;
    let Some(tile) = session
        .game
        .as_ref()
        .and_then(|game| game.map.get(battle.target))
    else {
        return;
    };
    let woods = match tile.cover {
        Cover::Forest => &session.pack.visuals.forest,
        Cover::Jungle => session
            .pack
            .visuals
            .overlays
            .get("jungle")
            .unwrap_or(&session.pack.visuals.forest),
        _ => return,
    };
    let image: Handle<Image> = assets.load(format!("{}{woods}", session.asset_prefix));
    // The trees are drawn at the map's own scale, so they stand behind the lines rather than
    // over them.
    let size = Vec2::new(128.0, 112.0) * (field.cell.y / 70.0) * 0.62;
    let step = size.x * 0.72;
    let count = (field.size.x / step).ceil() as i32 + 1;
    for k in 0..=count {
        let scatter = presentation::scatter(Coord::new(k, 3));
        let at = Vec2::new(
            k as f32 * step - size.x * 0.2 + (scatter % 23) as f32,
            -size.y * 0.62 + ((scatter / 23) % 13) as f32,
        );
        parent.spawn((
            ImageNode::new(image.clone())
                .with_color(Color::srgba(0.86, 0.9, 0.86, 0.96))
                .with_mode(NodeImageMode::Stretch),
            Node {
                position_type: PositionType::Absolute,
                left: px(at.x),
                top: px(at.y),
                width: px(size.x),
                height: px(size.y),
                ..default()
            },
            ZIndex(2),
        ));
    }
}

/// One soft patch of the field, a little darker or lighter than the ground:
/// where it lies and how large it is come from `scatter`, so the panel is never one flat
/// pour. `small` and `large` bound its size as shares of the field.
#[allow(clippy::too_many_arguments)]
fn blotch(
    parent: &mut ChildSpawnerCommands,
    soft: &Handle<Image>,
    ground: Color,
    tone: f32,
    alpha: f32,
    size: Vec2,
    scatter: usize,
    small: f32,
    large: f32,
) {
    let scatter = scatter as f32;
    let span = (large - small).max(0.01);
    let wide = size.x * (small + scatter % 13.0 / 13.0 * span);
    let high = size.y * (small + (scatter / 13.0) % 11.0 / 11.0 * span);
    let at = Vec2::new(
        scatter % 101.0 / 100.0 * (size.x - wide),
        (scatter / 101.0) % 101.0 / 100.0 * (size.y - high),
    );
    parent.spawn((
        ImageNode::new(soft.clone())
            .with_color(lit(ground, tone).with_alpha(alpha))
            .with_mode(NodeImageMode::Stretch),
        Node {
            position_type: PositionType::Absolute,
            left: px(at.x),
            top: px(at.y),
            width: px(wide),
            height: px(high),
            ..default()
        },
        ZIndex(1),
    ));
}

/// A colour the same as `base`, but a little lighter or darker.
fn lit(base: Color, tone: f32) -> Color {
    let c = base.to_srgba();
    Color::srgb(
        (c.red * tone).clamp(0.0, 1.0),
        (c.green * tone).clamp(0.0, 1.0),
        (c.blue * tone).clamp(0.0, 1.0),
    )
    .with_alpha(c.alpha)
}

/// The depth of a unit standing at `y` on the field: further down the screen is in front.
fn depth(y: f32) -> i32 {
    (60.0 + y * 0.08) as i32
}

/// One unit: the shadow it stands on, its sprite with the nation colour over it, its hit-point
/// bar, and the flash of a blow landing on it.
fn unit(
    parent: &mut ChildSpawnerCommands,
    game: &Game,
    plan: &Plan,
    index: usize,
    soft: Handle<Image>,
) {
    let actor = &plan.actors[index];
    let at = actor.stints.first().map_or(Vec2::ZERO, |stint| stint.at);
    // The field's own coordinates are centred on it; a child node measures from its top left.
    let place = at + plan.field.size * 0.5;
    let nation = presentation::nation_color(game, actor.owner, 1.0);
    let sprite =
        |parent: &mut ChildSpawnerCommands, image: Handle<Image>, color: Color, role: Role| {
            parent.spawn((
                Piece {
                    actor: index,
                    role,
                    spawn: at,
                },
                ImageNode::new(image).with_color(color),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(place.x - actor.cell.x * 0.5),
                    top: px(place.y - actor.cell.y * actor.anchor.y),
                    width: px(actor.cell.x),
                    height: px(actor.cell.y),
                    ..default()
                },
                ZIndex(depth(at.y)),
            ));
        };
    // The shadow it stands on: a soft, flat ellipse just under the feet, so the unit sits on
    // the ground instead of floating over it.
    let shade = actor.cell * Vec2::new(0.30, 0.085);
    parent.spawn((
        Piece {
            actor: index,
            role: Role::Shadow,
            spawn: at,
        },
        ImageNode::new(soft.clone())
            .with_color(Color::srgba(0.02, 0.03, 0.05, 0.34))
            .with_mode(NodeImageMode::Stretch),
        Node {
            position_type: PositionType::Absolute,
            left: px(place.x - shade.x),
            top: px(place.y - shade.y),
            width: px(shade.x * 2.0),
            height: px(shade.y * 2.0),
            ..default()
        },
        ZIndex(depth(at.y) - 1),
    ));
    match &actor.art {
        Art::Clips { idle, .. } => {
            sprite(parent, idle.art.image.clone(), Color::WHITE, Role::Base);
            if let Some(accent) = &idle.accent {
                sprite(parent, accent.clone(), nation, Role::Accent);
            }
        }
        Art::Still { image, accent } => {
            sprite(parent, image.clone(), Color::WHITE, Role::Base);
            if let Some(accent) = accent {
                sprite(parent, accent.clone(), nation, Role::Accent);
            }
        }
    }
    // The hit-point bar, floating under the unit as in the map's own fights: one pip per
    // hit point, and no backing, so nothing boxes the unit in.
    let count = actor.max_hp as usize;
    let width = ((BAR - PIP_GAP * count.saturating_sub(1) as f32) / count as f32).max(2.0);
    for pip in 0..count {
        parent.spawn((
            Piece {
                actor: index,
                role: Role::Pip(pip),
                spawn: at,
            },
            Node {
                position_type: PositionType::Absolute,
                left: px(place.x - BAR * 0.5 + pip as f32 * (width + PIP_GAP)),
                top: px(place.y + actor.cell.y * 0.05),
                width: px(width),
                height: px(PIP),
                border_radius: rounded(1.5),
                ..default()
            },
            BackgroundColor(SPENT),
            ZIndex(depth(at.y) + 4),
        ));
    }
    // The flash of a blow: a small, bright burst on the body, not a wash over the whole unit.
    let glow = actor.cell * 0.15;
    parent.spawn((
        Piece {
            actor: index,
            role: Role::Flash,
            spawn: at,
        },
        ImageNode::new(soft)
            .with_color(Color::srgba(1.0, 0.9, 0.62, 0.0))
            .with_mode(NodeImageMode::Stretch),
        Node {
            position_type: PositionType::Absolute,
            left: px(place.x - glow.x),
            top: px(place.y - actor.cell.y * actor.anchor.y * 0.78 - glow.y),
            width: px(glow.x * 2.0),
            height: px(glow.y * 2.0),
            ..default()
        },
        ZIndex(depth(at.y) + 8),
    ));
}

// --- playing it --------------------------------------------------------------------------------

/// What one unit looks like at the moment the clock has reached.
struct Frame {
    at: Vec2,
    pose: Pose,
    clip: Clip,
    /// The frame of that clip to draw. Looping clips are driven by the clock instead.
    frame: u32,
    hp: i32,
    flash: f32,
    gone: bool,
    /// How far through the lead-in the field is: everything fades up together.
    fade: f32,
}

/// The pose of one motion, cut down to what a unit's own clip cannot show.
fn posed(actor: &Actor, motion: Motion, u: f32) -> Pose {
    let sampled = motion.sample(u, actor.side.toward());
    if actor.clips() {
        motion.under_clip(sampled)
    } else {
        sampled
    }
}

/// How brightly a blow at `blow` is flashing now.
fn glow(now: f32, blow: f32) -> f32 {
    if now < blow || now > blow + FLASH {
        return 0.0;
    }
    1.0 - (now - blow) / FLASH
}

/// Work out one unit's frame: where it stands, what it is doing, and what its bar reads.
fn play(actor: &Actor, plan: &Plan, t: f32, clock: f32) -> Frame {
    let (at, crossing) = actor.place(t);
    let fade = (t / LEAD).clamp(0.0, 1.0);
    // Beaten: it goes down where it stood, and is gone when it has finished going down.
    if let Some((died, sink)) = actor.end
        && t >= died
    {
        let u = (t - died) / drop(sink);
        let motion = if sink { Motion::Sink } else { Motion::Fall };
        return Frame {
            at,
            pose: posed(actor, motion, u),
            clip: Clip::Death,
            frame: actor.frame(Clip::Death, u, clock),
            hp: 0,
            flash: 0.0,
            gone: u >= 1.0,
            fade,
        };
    }
    // Crossing between the front and its own place in the ranks.
    if let Some((by, u)) = crossing {
        return Frame {
            at,
            pose: posed(actor, Motion::Step { by }, u),
            clip: Clip::Run,
            frame: actor.frame(Clip::Run, u, clock),
            hp: actor.hp,
            flash: 0.0,
            gone: false,
            fade,
        };
    }
    let mut motion: Option<(Motion, f32)> = None;
    let mut flinch: Option<f32> = None;
    let mut flash = 0.0;
    let mut hp = actor.hp;
    if let Some(stint) = actor.stint(t)
        && let Some(duel) = stint.duel.map(|index| &plan.duels[index])
    {
        let record = &plan.records[duel.record];
        hp = actor.hp_at(duel, record, t);
        if t >= duel.decided {
            // Decided: the winner cheers, and the loser is already going down above.
            let u = (t - duel.decided) / CHEER;
            return Frame {
                at,
                pose: posed(actor, Motion::Cheer { sea: plan.sea }, u),
                clip: Clip::Victory,
                frame: actor.frame(Clip::Victory, u, clock),
                hp,
                flash: 0.0,
                gone: false,
                fade,
            };
        }
        if t >= duel.first {
            // A round: both swing, and the one that lost it is knocked back at the blow.
            let beat = (t - duel.first) / BEAT;
            let round = (beat.floor() as usize).min(record.rounds.len().saturating_sub(1));
            let local = (beat - round as f32).clamp(0.0, 1.0);
            motion = Some((
                Motion::Attack {
                    style: actor.style,
                    variant: (round % 2) as u8,
                },
                local,
            ));
            if actor.took(record, round)
                && let Some(blow) = duel.blows.get(round)
            {
                flash = glow(t, *blow);
                if t >= *blow {
                    flinch = Some((t - blow) / FLINCH);
                }
            }
        } else if actor.side == Side::Attacker
            && let Some(blow) = duel.shot_blow()
        {
            // Waiting out the defending gun, and taking its shot if it finds the mark.
            flash = glow(t, blow);
            if record.support.as_ref().is_some_and(|shot| shot.hit) && t >= blow {
                flinch = Some((t - blow) / FLINCH);
            }
        }
    } else if let Some((start, end)) = actor
        .shots
        .iter()
        .find(|(start, end)| (*start..*end).contains(&t))
    {
        // The gun behind the line, firing the shot that covers the front.
        motion = Some((
            Motion::Attack {
                style: actor.style,
                variant: 0,
            },
            (t - start) / (end - start),
        ));
    }
    let mut pose = Pose::REST;
    let mut clip = Clip::Idle;
    let mut progress = 0.0;
    if let Some((sample, u)) = motion {
        pose = posed(actor, sample, u);
        clip = match sample {
            Motion::Attack { .. } => Clip::Attack,
            Motion::Step { .. } => Clip::Run,
            Motion::Cheer { .. } => Clip::Victory,
            _ => Clip::Idle,
        };
        progress = u;
    }
    if let Some(u) = flinch {
        pose = pose.and(posed(actor, Motion::Flinch, u));
    }
    Frame {
        at,
        pose,
        clip,
        frame: actor.frame(clip, progress, clock),
        hp,
        flash,
        gone: false,
        fade,
    }
}

/// A colour washed toward white by a blow landing. A sprite carries its own colours, so only
/// the wash shows on it; the nation colour over it can also be lifted.
fn washed(role: Role, base: Color, flash: f32) -> Color {
    let c = base.to_srgba();
    let bright = 1.0 + 1.5 * flash;
    if role == Role::Base {
        return Color::srgba(bright, bright, bright, c.alpha);
    }
    Color::srgba(
        (c.red * bright).min(1.0),
        (c.green * bright).min(1.0),
        (c.blue * bright).min(1.0),
        c.alpha,
    )
}

/// Pose every unit, its sprite, its shadow, its bar and its flash for the clock's moment, keep
/// the labels and the button in step with it, and play the sounds of what has happened.
#[allow(clippy::too_many_arguments)]
pub(super) fn animate(
    mut commands: Commands,
    assets: Res<AssetServer>,
    time: Res<Time>,
    session: Res<Session>,
    board: Res<Board>,
    // The bar pips are plain colour nodes with no image, while the sprites, shadows and
    // flashes are images: both paints are optional, and each role draws with its own.
    mut pieces: Query<
        (
            &Piece,
            &mut UiTransform,
            Option<&mut ImageNode>,
            Option<&mut BackgroundColor>,
            &mut Visibility,
            &mut ZIndex,
        ),
        (Without<Skip>, Without<Carry>),
    >,
    images: Res<Assets<Image>>,
    mut labels: Query<(&mut Text, Option<&Round>, Option<&Note>, Option<&Loss>)>,
    mut gates: Query<(&mut Visibility, Has<Skip>), Or<(With<Skip>, With<Carry>)>>,
    mut heard: Local<Heard>,
) {
    let Some(plan) = &board.plan else {
        return;
    };
    let Some((battle, elapsed)) = &session.stack_replay else {
        return;
    };
    let elapsed = *elapsed;
    let clock = time.elapsed_secs();
    // Sounds are played for what has happened since the last frame looked, and only once.
    if heard.battle.as_ref() != Some(battle) {
        heard.battle = Some(battle.clone());
        heard.at = elapsed;
    }
    if elapsed > heard.at {
        let cue = |commands: &mut Commands, name: &str| {
            if let Some(path) = session.pack.visuals.combat.sounds.get(name) {
                commands.spawn((
                    AudioPlayer::new(assets.load(format!("{}{}", session.asset_prefix, path))),
                    PlaybackSettings::DESPAWN,
                ));
            }
        };
        for duel in &plan.duels {
            if let Some(shot) = duel.shot
                && heard.at < shot.at
                && shot.at <= elapsed
            {
                let style = session
                    .rules
                    .units
                    .get(&plan.records[duel.record].attacker.kind)
                    .map_or(Style::Volley, |def| {
                        session.pack.visuals.combat.style_of(def)
                    });
                cue(
                    &mut commands,
                    match style {
                        Style::Gun => "gun",
                        Style::Charge => "charge",
                        Style::Broadside => "broadside",
                        Style::Volley => "volley",
                    },
                );
            }
            for blow in duel.blows.iter().chain(duel.shot_blow().iter()) {
                if heard.at < *blow && *blow <= elapsed {
                    cue(&mut commands, "hit");
                }
            }
        }
        for actor in &plan.actors {
            if let Some((died, sink)) = actor.end
                && heard.at < died
                && died <= elapsed
            {
                cue(&mut commands, if sink { "sink" } else { "fall" });
            }
        }
        heard.at = elapsed;
    }
    let frames: Vec<Frame> = plan
        .actors
        .iter()
        .map(|actor| play(actor, plan, elapsed, clock))
        .collect();
    for (piece, mut transform, mut image, mut color, mut visibility, mut z) in &mut pieces {
        let (Some(frame), Some(actor)) = (frames.get(piece.actor), plan.actors.get(piece.actor))
        else {
            continue;
        };
        let want = if frame.gone {
            Visibility::Hidden
        } else {
            Visibility::Visible
        };
        if *visibility != want {
            *visibility = want;
        }
        if frame.gone {
            continue;
        }
        let delta = frame.at - piece.spawn;
        let alpha = frame.pose.alpha * frame.fade;
        match piece.role {
            Role::Base | Role::Accent => {
                let Some(image) = image.as_mut() else {
                    continue;
                };
                transform.translation =
                    Val2::px(delta.x + frame.pose.offset.x, delta.y - frame.pose.offset.y);
                transform.scale = Vec2::splat(frame.pose.scale);
                transform.rotation = Rot2::radians(-frame.pose.tilt);
                z.0 = depth(frame.at.y);
                let accent = piece.role == Role::Accent;
                let layer = actor.art.get(frame.clip);
                // The sheet of this clip, and the frame of it to show. A sheet that has not
                // loaded yet is left blank rather than drawn as the placeholder the renderer
                // would put there.
                let sheet = match (layer, accent) {
                    (Some(layer), false) => Some(layer.art.image.clone()),
                    (Some(layer), true) => layer.accent.clone(),
                    (None, _) => None,
                };
                match sheet.filter(|sheet| images.get(sheet).is_some()) {
                    Some(sheet) => {
                        if image.image != sheet {
                            image.image = sheet;
                        }
                        if let Some(layer) = layer {
                            let index = layer.art.index(actor.face, frame.frame);
                            let same = image.texture_atlas.as_ref().is_some_and(|atlas| {
                                atlas.layout == layer.art.layout && atlas.index == index
                            });
                            if !same {
                                image.texture_atlas = Some(TextureAtlas {
                                    layout: layer.art.layout.clone(),
                                    index,
                                });
                            }
                        }
                        image.flip_x = false;
                    }
                    None => {
                        if let Some((still, accent_sheet)) = actor.art.still() {
                            let sheet = if accent {
                                accent_sheet.clone()
                            } else {
                                Some(still.clone())
                            };
                            let blank = sheet
                                .filter(|sheet| images.get(sheet).is_some())
                                .unwrap_or_else(|| TRANSPARENT_IMAGE_HANDLE.clone());
                            if image.image != blank {
                                image.image = blank.clone();
                            }
                            image.texture_atlas = None;
                            image.flip_x = actor.side == Side::Defender;
                        }
                    }
                }
                let base = if accent {
                    session.game.as_ref().map_or(Color::WHITE, |game| {
                        presentation::nation_color(game, actor.owner, 1.0)
                    })
                } else {
                    Color::WHITE
                };
                let lit = washed(piece.role, base, frame.flash);
                image.color = lit.with_alpha(lit.alpha() * alpha);
            }
            Role::Shadow => {
                let Some(image) = image.as_mut() else {
                    continue;
                };
                transform.translation = Val2::px(delta.x, delta.y);
                let lifted = (frame.pose.offset.y.abs() / 12.0).min(1.0);
                transform.scale = Vec2::new(1.0, 1.0 - 0.3 * lifted);
                image.color = Color::srgba(0.02, 0.03, 0.05, 0.34 * alpha);
                z.0 = depth(frame.at.y) - 1;
            }
            Role::Pip(pip) => {
                let Some(color) = color.as_mut() else {
                    continue;
                };
                transform.translation = Val2::px(delta.x, delta.y);
                z.0 = depth(frame.at.y) + 4;
                let fraction = frame.hp.max(0) as f32 / actor.max_hp as f32;
                color.0 = if (pip as i32) < frame.hp {
                    hp_color(fraction).with_alpha(alpha)
                } else {
                    SPENT.with_alpha(SPENT.alpha() * alpha)
                };
            }
            Role::Flash => {
                // The flash is the soft round puff, glowing over the unit's chest as the
                // blow lands and following it as it reels. It paints the image, never the
                // node's own backing, so no box of it ever shows on the ground.
                let Some(image) = image.as_mut() else {
                    continue;
                };
                transform.translation =
                    Val2::px(delta.x + frame.pose.offset.x, delta.y - frame.pose.offset.y);
                z.0 = depth(frame.at.y) + 8;
                image.color = Color::srgba(1.0, 0.92, 0.7, 0.8 * frame.flash * frame.fade);
            }
        }
    }
    // The header, the two lines' losses, the closing note and the button's two words.
    let finished = elapsed >= plan.span - TAIL;
    let round = format!(
        "{} LANES  ·  ROUND {} OF {}",
        plan.field.lane_at.len(),
        plan.blows_at(elapsed).min(plan.blows),
        plan.blows
    );
    let note = if !finished {
        ("", ON_PLATE)
    } else if battle.advanced {
        ("THE LINE BREAKS  ·  THE STACK ADVANCES", GOOD)
    } else {
        ("THE DEFENDERS HOLD THE LINE", BAD)
    };
    let (owner, enemy) = (
        session
            .game
            .as_ref()
            .map(|game| game.factions[&battle.owner].name.to_uppercase())
            .unwrap_or_default(),
        session
            .game
            .as_ref()
            .map(|game| game.factions[&battle.enemy].name.to_uppercase())
            .unwrap_or_default(),
    );
    for (mut text, round_label, note_label, loss) in &mut labels {
        let want = if round_label.is_some() {
            round.clone()
        } else if note_label.is_some() {
            note.0.to_string()
        } else if let Some(side) = loss {
            let (name, lost) = match side.0 {
                Side::Attacker => (&owner, plan.losses_at(elapsed, Side::Attacker)),
                Side::Defender => (&enemy, plan.losses_at(elapsed, Side::Defender)),
            };
            format!("{name}  ·  {lost} LOST")
        } else {
            continue;
        };
        if text.0 != want {
            text.0 = want;
        }
    }
    // The one button says *Skip* while the fight is on and *Continue* once it is decided.
    for (mut visibility, skip) in &mut gates {
        let want = if skip == finished {
            Visibility::Hidden
        } else {
            Visibility::Visible
        };
        if *visibility != want {
            *visibility = want;
        }
    }
}

/// The battle the sounds have already been played for, and how far the clock had run.
#[derive(Default)]
pub(super) struct Heard {
    battle: Option<StackBattle>,
    at: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use fourx_sim::Support;
    use fourx_sim::stacks::Lane;
    use fourx_sim::terrain::Coord;

    fn fighter(id: Id, owner: Id, kind: &str, hp: i32) -> Fighter {
        Fighter {
            id,
            owner,
            kind: kind.into(),
            position: Coord::new(18, 11),
            hp,
            max_hp: 4,
        }
    }

    fn lane(lane: usize, start: u32, rounds: Vec<bool>, outcome: Outcome) -> Lane {
        Lane {
            lane,
            start,
            attacker: fighter(1, 1, "infantry", 4),
            defender: fighter(2, 2, "infantry", 4),
            rounds,
            outcome,
            support: None,
        }
    }

    fn battle(lanes: Vec<Lane>, width: usize, rounds: u32) -> StackBattle {
        StackBattle {
            owner: 1,
            enemy: 2,
            target: Coord::new(18, 11),
            width,
            lanes,
            rounds,
            attacker_losses: 0,
            defender_losses: 0,
            advanced: false,
        }
    }

    #[test]
    fn a_blow_lands_at_the_strike_of_its_beat() {
        let battle = battle(
            vec![lane(0, 0, vec![true, false, true], Outcome::AttackerWon)],
            1,
            3,
        );
        let (duels, span) = schedule(&battle, false);
        let duel = &duels[0];
        assert!((duel.first - LEAD).abs() < 1e-6, "the first beat opens it");
        for (round, blow) in duel.blows.iter().enumerate() {
            let beat = duel.first + round as f32 * BEAT;
            assert!((blow - (beat + STRIKE_AT * BEAT)).abs() < 1e-5, "{round}");
        }
        assert!((span - (duel.decided + FALL + CHEER + TAIL)).abs() < 1e-5);
    }

    #[test]
    fn one_lane_plays_its_duels_turn_and_turn_while_the_lanes_fight_side_by_side() {
        let battle = battle(
            vec![
                lane(0, 0, vec![true, true], Outcome::AttackerWon),
                lane(1, 0, vec![true, true], Outcome::AttackerWon),
                lane(0, 2, vec![true], Outcome::AttackerWon),
            ],
            2,
            3,
        );
        let (duels, span) = schedule(&battle, false);
        let fought = |lane: usize| duels.iter().filter(|duel| duel.lane == lane).count();
        assert_eq!(fought(0), 2, "lane 0 fought twice");
        assert_eq!(fought(1), 1, "lane 1 once");
        for lane in 0..2 {
            let opening = duels
                .iter()
                .filter(|duel| duel.lane == lane)
                .min_by(|a, b| a.arrive.total_cmp(&b.arrive))
                .expect("a lane fights");
            assert_eq!(opening.arrive, 0.0, "the lanes open together");
        }
        let second = duels
            .iter()
            .find(|duel| duel.lane == 0 && duel.arrive > 0.0)
            .expect("lane 0 fights a second time");
        assert!(
            second.arrive >= duels[0].decided + FALL,
            "the next pair waits for the one before it to fall"
        );
        assert!((second.first - (second.arrive + STRIDE)).abs() < 1e-5);
        assert!((span - (second.decided + FALL + CHEER + TAIL)).abs() < 1e-5);
    }

    #[test]
    fn a_support_shot_is_a_beat_of_its_own_before_the_rounds() {
        let mut record = lane(0, 0, vec![true, true], Outcome::AttackerWon);
        record.support = Some(Support {
            shooter: fighter(3, 2, "artillery", 3),
            hit: true,
        });
        let battle = battle(vec![record], 1, 2);
        let (duels, _) = schedule(&battle, false);
        let duel = &duels[0];
        let shot = duel.shot.expect("the gun fires");
        assert!((shot.at - LEAD).abs() < 1e-6);
        assert!((duel.first - (LEAD + SHOT)).abs() < 1e-6);
        assert!((duel.shot_blow().expect("a shot lands") - (LEAD + STRIKE_AT * SHOT)).abs() < 1e-5);
        assert_eq!(duel.blows_total(), 3, "two rounds and the shot");
    }

    #[test]
    fn the_two_orders_of_battle_face_each_other_and_keep_every_fighter_once() {
        let mut second = lane(0, 2, vec![true], Outcome::DefenderWon);
        // The defender that held the front fights again, against a fresh attacker.
        second.attacker = fighter(4, 1, "cavalry", 3);
        let battle = battle(
            vec![lane(0, 0, vec![true, true], Outcome::AttackerWon), second],
            1,
            3,
        );
        let attackers = order(&battle, true);
        let defenders = order(&battle, false);
        assert_eq!(
            attackers.iter().map(|f| f.id).collect::<Vec<_>>(),
            vec![1, 4]
        );
        assert_eq!(defenders.iter().map(|f| f.id).collect::<Vec<_>>(), vec![2]);
        let field = field(&battle, false, Vec2::new(1440.0, 900.0));
        assert!(field.front(0, Side::Attacker).x < 0.0);
        assert!(field.front(0, Side::Defender).x > 0.0);
        // Every fighter has a place in the ranks; the one waiting behind stands further back.
        assert_eq!(field.back[0].len(), 2, "a place for each attacker");
        assert_eq!(field.back[1].len(), 1, "a place for each defender");
        assert!(field.back[0][0].x < field.front(0, Side::Attacker).x);
        assert!(field.back[0][1].x < field.back[0][0].x);
        assert!(field.back[1][0].x > field.front(0, Side::Defender).x);
    }

    #[test]
    fn the_count_of_blows_and_losses_follows_the_record() {
        let battle = battle(
            vec![
                lane(0, 0, vec![true], Outcome::AttackerWon),
                lane(0, 1, vec![true], Outcome::AttackerWon),
            ],
            1,
            2,
        );
        let (duels, span) = schedule(&battle, false);
        let plan = Plan {
            sea: false,
            span,
            corner: Vec2::ZERO,
            field: field(&battle, false, Vec2::new(1440.0, 900.0)),
            duels,
            records: battle.lanes.clone(),
            actors: Vec::new(),
            blows: 2,
        };
        let first = plan.duels[0].decided;
        assert_eq!(plan.blows_at(first), 1);
        assert_eq!(plan.losses_at(first, Side::Defender), 1);
        assert_eq!(plan.losses_at(first, Side::Attacker), 0);
        let last = plan.duels[1].decided;
        assert_eq!(plan.blows_at(last), 2);
        assert_eq!(plan.losses_at(last, Side::Defender), 2);
        assert!(plan.span > last + FALL, "the field holds to the end");
    }

    #[test]
    fn a_unit_waits_in_the_ranks_steps_in_to_fight_and_steps_back_out() {
        let mut second = lane(0, 1, vec![true], Outcome::AttackerWon);
        second.attacker = fighter(4, 1, "cavalry", 3);
        let battle = battle(
            vec![lane(0, 0, vec![true, true], Outcome::AttackerWon), second],
            1,
            3,
        );
        let (duels, span) = schedule(&battle, false);
        let field = field(&battle, false, Vec2::new(1440.0, 900.0));
        let reserve = field.back[0][0];
        let front = field.front(0, Side::Attacker);
        let actor = Actor {
            owner: 1,
            side: Side::Attacker,
            art: Art::Still {
                image: Handle::default(),
                accent: None,
            },
            cell: cell_size(false),
            anchor: LAND_ANCHOR,
            hp: 3,
            max_hp: 3,
            style: Style::Charge,
            face: Side::Attacker.face(),
            stints: vec![
                Stint {
                    at: reserve,
                    from: 0.0,
                    to: duels[1].arrive,
                    duel: None,
                },
                Stint {
                    at: front,
                    from: duels[1].arrive,
                    to: duels[1].relief,
                    duel: Some(1),
                },
                Stint {
                    at: reserve,
                    from: duels[1].relief,
                    to: span,
                    duel: None,
                },
            ],
            shots: Vec::new(),
            end: None,
            phase: 0.0,
        };
        assert_eq!(actor.place(0.0).0, reserve, "it waits in the ranks");
        let (at, crossing) = actor.place(duels[1].arrive + STRIDE * 0.5);
        assert!(crossing.is_some(), "it steps in");
        assert!(
            at.x < front.x && at.x > reserve.x,
            "between the ranks: {at:?}"
        );
        assert_eq!(actor.place(duels[1].first + BEAT * 0.5).0, front);
        let (_, crossing) = actor.place(duels[1].relief + STRIDE * 0.5);
        assert!(crossing.is_some(), "it steps back out");
        assert_eq!(actor.place(span).0, reserve);
    }

    #[test]
    fn a_beaten_unit_keeps_the_front_while_it_falls() {
        let battle = battle(
            vec![lane(0, 0, vec![true, true], Outcome::AttackerWon)],
            1,
            2,
        );
        let (duels, span) = schedule(&battle, false);
        let field = field(&battle, false, Vec2::new(1440.0, 900.0));
        let record = battle.lanes[0].clone();
        let duel = duels[0].clone();
        let defender = Actor {
            owner: 2,
            side: Side::Defender,
            art: Art::Still {
                image: Handle::default(),
                accent: None,
            },
            cell: cell_size(false),
            anchor: LAND_ANCHOR,
            // Two rounds, both lost by the defender: it had two hit points left to lose.
            hp: 2,
            max_hp: 2,
            style: Style::Volley,
            face: Side::Defender.face(),
            stints: vec![Stint {
                at: field.front(0, Side::Defender),
                from: 0.0,
                to: duel.decided,
                duel: Some(0),
            }],
            shots: Vec::new(),
            end: Some((duel.decided, false)),
            phase: 0.0,
        };
        let plan = Plan {
            sea: false,
            span,
            corner: Vec2::ZERO,
            field: field.clone(),
            duels: duels.clone(),
            records: vec![record],
            actors: Vec::new(),
            blows: 2,
        };
        // It takes a wound a round, and is down to nothing when the duel is decided.
        assert_eq!(
            defender.hp_at(&duel, &plan.records[0], duel.blows[0] - 0.01),
            2
        );
        assert_eq!(defender.hp_at(&duel, &plan.records[0], duel.blows[0]), 1);
        assert_eq!(defender.hp_at(&duel, &plan.records[0], duel.decided), 0);
        // It keeps its place on the front while it goes down, and is gone after that.
        let fallen = play(&defender, &plan, duel.decided + drop(false) * 0.5, 0.0);
        assert_eq!(fallen.at, field.front(0, Side::Defender));
        assert_eq!(fallen.clip, Clip::Death);
        assert!(!fallen.gone);
        assert!(play(&defender, &plan, duel.decided + drop(false) + 0.01, 0.0).gone);
    }

    #[test]
    fn a_round_is_one_attack_motion_each_with_the_blow_half_way_through_it() {
        let battle = battle(
            vec![lane(0, 0, vec![false, false], Outcome::DefenderWon)],
            1,
            2,
        );
        let (duels, span) = schedule(&battle, false);
        let duel = duels[0].clone();
        let field = field(&battle, false, Vec2::new(1440.0, 900.0));
        let record = battle.lanes[0].clone();
        let plan = Plan {
            sea: false,
            span,
            corner: Vec2::ZERO,
            field: field.clone(),
            duels: duels.clone(),
            records: vec![record],
            actors: Vec::new(),
            blows: 2,
        };
        let attacker = Actor {
            owner: 1,
            side: Side::Attacker,
            art: Art::Still {
                image: Handle::default(),
                accent: None,
            },
            cell: cell_size(false),
            anchor: LAND_ANCHOR,
            hp: 4,
            max_hp: 4,
            style: Style::Charge,
            face: Side::Attacker.face(),
            stints: vec![Stint {
                at: field.front(0, Side::Attacker),
                from: 0.0,
                to: duel.decided,
                duel: Some(0),
            }],
            shots: Vec::new(),
            end: Some((duel.decided, false)),
            phase: 0.0,
        };
        // The charge closes on the foe and blows land half way through their beat.
        let before = play(&attacker, &plan, duel.blows[0] - 0.05, 0.0)
            .pose
            .offset
            .x;
        let strike = play(&attacker, &plan, duel.blows[0], 0.0).pose.offset.x;
        assert!(strike > before, "{strike} against {before}");
        assert!(strike > 4.0, "the charge reaches the foe");
        let settled = play(&attacker, &plan, duel.first + BEAT - 0.001, 0.0);
        assert!(settled.pose.offset.x < strike, "and comes back to rest");
        assert_eq!(play(&attacker, &plan, duel.blows[0] - 0.01, 0.0).flash, 0.0);
        let blown = play(&attacker, &plan, duel.blows[0] + 0.01, 0.0);
        assert!(blown.flash > 0.0, "the flash of the blow is up");
        assert!(blown.pose.flash > 0.0, "and the wounded unit reels");
    }

    fn duelist(field: &Field, duel: &Duel, side: Side, owner: Id, hp: i32, max_hp: i32) -> Actor {
        Actor {
            owner,
            side,
            art: Art::Still {
                image: Handle::default(),
                accent: None,
            },
            cell: cell_size(false),
            anchor: LAND_ANCHOR,
            hp,
            max_hp,
            style: Style::Volley,
            face: side.face(),
            stints: vec![Stint {
                at: field.front(0, side),
                from: 0.0,
                to: duel.decided,
                duel: Some(0),
            }],
            shots: Vec::new(),
            end: None,
            phase: 0.0,
        }
    }

    #[test]
    fn both_sides_swing_every_round_and_only_the_loser_reels() {
        let battle = battle(
            vec![lane(0, 0, vec![true, false, true], Outcome::AttackerWon)],
            1,
            3,
        );
        let (duels, span) = schedule(&battle, false);
        let duel = duels[0].clone();
        let field = field(&battle, false, Vec2::new(1440.0, 900.0));
        let plan = Plan {
            sea: false,
            span,
            corner: Vec2::ZERO,
            field: field.clone(),
            duels: duels.clone(),
            records: battle.lanes.clone(),
            actors: Vec::new(),
            blows: 3,
        };
        let attacker = duelist(&field, &duel, Side::Attacker, 1, 3, 3);
        let defender = duelist(&field, &duel, Side::Defender, 2, 3, 3);
        // The first round goes the attacker's way: both swing, and only the defender
        // takes the blow, flashing and falling a hit point behind at the strike.
        let mid = duel.first + BEAT * 0.5;
        assert_eq!(play(&attacker, &plan, mid, 0.0).clip, Clip::Attack);
        assert_eq!(play(&defender, &plan, mid, 0.0).clip, Clip::Attack);
        let blow = duel.blows[0];
        assert_eq!(play(&attacker, &plan, blow - 0.01, 0.0).hp, 3);
        assert_eq!(play(&defender, &plan, blow - 0.01, 0.0).hp, 3);
        assert_eq!(play(&attacker, &plan, blow, 0.0).flash, 0.0);
        assert_eq!(play(&defender, &plan, blow, 0.0).flash, 1.0);
        assert_eq!(play(&attacker, &plan, blow, 0.0).hp, 3);
        assert_eq!(play(&defender, &plan, blow, 0.0).hp, 2);
        assert_eq!(play(&defender, &plan, blow + FLASH + 0.01, 0.0).flash, 0.0);
        // The second round goes the other way: now the attacker reels.
        let blow = duel.blows[1];
        assert_eq!(play(&attacker, &plan, blow, 0.0).flash, 1.0);
        assert_eq!(play(&defender, &plan, blow, 0.0).flash, 0.0);
        assert_eq!(play(&attacker, &plan, blow, 0.0).hp, 2);
    }

    #[test]
    fn the_sea_ground_turns_ships_beam_on_and_the_land_leaves_them_facing() {
        let rules = fourx_content::Pack::base().rules;
        let battle = battle(vec![lane(0, 0, vec![true], Outcome::AttackerWon)], 1, 1);
        assert!(
            !at_sea(&battle, &rules),
            "an infantry duel is fought ashore"
        );
        let mut sea = battle.clone();
        sea.lanes[0].attacker = fighter(1, 1, "ironclad", 4);
        assert!(at_sea(&sea, &rules), "two squadrons fight afloat");
        let field = field(&battle, false, Vec2::new(1440.0, 900.0));
        assert_eq!(field.cell, cell_size(false));
        let sea = cell_size(true);
        assert!(sea.x > sea.y, "a ship's cell is wider than it is tall");
        assert_eq!(
            (Side::Attacker.face() + BEAM_ON) % 8,
            5,
            "east turned a quarter turn shows the starboard side"
        );
    }
}
