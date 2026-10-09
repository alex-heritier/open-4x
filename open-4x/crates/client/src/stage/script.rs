//! The timeline of one battle, built from its record.
//!
//! A [`Script`] is a fixed list of beats laid out once and then sampled by a clock, so the same
//! record always plays the same way. It knows nothing about the engine: the stage reads the
//! actors' poses and the events due, and draws them. The rules this implements are in
//! `docs/combat-animation.md`; keep its constants table in step with the ones below.
use super::motion::{FIRE_AT, Motion, Pose, STEP_SECS, STRIKE_AT};
use bevy::math::Vec2;
use fourx_content::animation::Clip;
use fourx_content::combat::{CombatVisuals, Style};
use fourx_sim::{Battle, Fighter, Id, Outcome, Rules, Support, terrain::Coord};

/// Fade-in before the first beat, and the hold after the last.
pub const LEAD_IN: f32 = 0.30;
pub const TAIL: f32 = 0.20;
/// Shortest finale, even when nobody has anything to do in it.
pub const MIN_FINALE: f32 = 0.60;
/// How fast a bombardment's shell travels (screen units per second), and the flights allowed.
pub const SHELL_SPEED: f32 = 600.0;
pub const SHELL_FLIGHT: (f32, f32) = (0.25, 0.90);
/// Gap between the shots of a broadside.
pub const SALVO_GAP: f32 = 0.05;
/// Where a unit's sprite stands relative to its square's anchor, as in the map's own drawing.
const CITY_SHIFT: Vec2 = Vec2::new(62.0, 24.0);
/// Where a defender's supporting gun stands beside it.
const SUPPORT_SHIFT: Vec2 = Vec2::new(30.0, 8.0);

/// Everything the script needs to know about the world around a battle.
pub struct Setting<'a> {
    pub rules: &'a Rules,
    pub combat: &'a CombatVisuals,
    /// Whether a square holds a city. A unit in a city stands beside it, not on it.
    pub city: &'a dyn Fn(Coord) -> bool,
    /// Whether a unit design has its own clips (see [`Script::showing`]).
    pub animated: &'a dyn Fn(&str) -> bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// The side that began the fight: attacker, besieger, shooter.
    Attacker,
    Defender,
    /// A gun standing beside the defender.
    Support,
    /// A unit taken or lost when its square fell.
    Captive,
}

/// One unit on stage.
#[derive(Clone, Debug)]
pub struct Actor {
    pub fighter: Fighter,
    pub role: Role,
    pub sea: bool,
    pub style: Style,
    /// Where the unit stands, in screen units (the sprite is drawn a little above it).
    pub anchor: Vec2,
    /// A unit vector toward the foe.
    pub toward: Vec2,
    /// The sprite faces left (a single image only; clips have a row for every facing).
    pub flip: bool,
    /// The design plays clips rather than moving one image about.
    pub animated: bool,
    /// Who the unit's colour turns to if it yields.
    pub captor: Option<Id>,
    pub motions: Vec<Segment>,
}

/// Which clip an actor shows at a moment, and where in it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Showing {
    /// A clip played once over a motion, `progress` of the way through; the last frame holds.
    Once { clip: Clip, progress: f32 },
    /// A looping clip, `seconds` after it began.
    Looping { clip: Clip, seconds: f32 },
}

/// A motion on the clock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub start: f32,
    pub motion: Motion,
}

impl Segment {
    #[cfg(test)]
    pub fn end(&self) -> f32 {
        self.start + self.motion.length()
    }
}

/// Something that happens at an instant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    Sound(&'static str),
    /// Muzzle flash and smoke at an actor.
    Flash {
        actor: usize,
        big: bool,
    },
    /// A shell leaves `from` for `to` and lands `flight` seconds later: on target, or short.
    Shell {
        from: usize,
        to: usize,
        flight: f32,
        hits: bool,
    },
    /// A cloud kicked up where a charge meets its mark.
    Dust {
        actor: usize,
    },
    /// The actor loses a hit point.
    Blow {
        target: usize,
    },
    /// A gain in rank.
    Sparkle {
        actor: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Timed {
    pub at: f32,
    pub event: Event,
}

/// A round of a duel or a volley of a bombardment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Beat {
    pub start: f32,
    pub length: f32,
}

#[derive(Clone, Debug)]
pub struct Script {
    pub actors: Vec<Actor>,
    /// In time order.
    pub events: Vec<Timed>,
    /// The script's whole length at normal speed.
    pub length: f32,
    /// The rounds or volleys, and when the finale begins: what the tests check the timing by.
    #[cfg(test)]
    pub beats: Vec<Beat>,
    #[cfg(test)]
    pub finale: f32,
}

/// The speed at which to play with `waiting` more battles behind the current one: faster the
/// longer the queue, so a turn of computer play does not hold the player up.
pub fn pace(waiting: usize, base: f32) -> f32 {
    let extra = waiting.saturating_sub(2) as f32;
    base * (1.0 + 0.25 * extra).min(4.0)
}

impl Script {
    pub fn build(battle: &Battle, setting: &Setting) -> Script {
        let mut b = Builder::new(setting);
        let finale = match battle {
            Battle::Duel {
                attacker,
                defender,
                support,
                rounds,
                outcome,
                retreat_to,
                promoted,
            } => b.duel(
                attacker,
                defender,
                support.as_ref(),
                rounds,
                *outcome,
                *retreat_to,
                *promoted,
            ),
            Battle::Capture {
                attacker,
                target,
                taken,
                destroyed,
                advanced,
                ..
            } => b.capture(attacker, *target, taken, destroyed, *advanced),
            Battle::Bombard {
                shooter,
                target,
                shots,
                killed,
                promoted,
            } => b.bombard(shooter, target, shots, *killed, *promoted),
        };
        b.finish(finale)
    }

    /// How an actor looks `t` seconds in: every motion under way, added together, and the
    /// final pose of every terminal motion that has finished.
    pub fn pose_at(&self, actor: usize, t: f32) -> Pose {
        let a = &self.actors[actor];
        a.motions.iter().fold(Pose::REST, |pose, seg| {
            if t < seg.start {
                return pose;
            }
            let u = (t - seg.start) / seg.motion.length();
            if u >= 1.0 && !seg.motion.is_terminal() {
                return pose;
            }
            let pose_now = seg.motion.sample(u, a.toward);
            pose.and(if a.animated {
                seg.motion.under_clip(pose_now)
            } else {
                pose_now
            })
        })
    }

    /// The clip an actor shows `t` seconds in: a swing while it attacks, the cheer while it
    /// cheers, the walk while it steps, the death from its fall on (holding the last frame), and
    /// otherwise standing. Of two motions under way, the one begun later shows.
    pub fn showing(&self, actor: usize, t: f32) -> Showing {
        let mut best: Option<(f32, Showing)> = None;
        for seg in &self.actors[actor].motions {
            if t < seg.start {
                continue;
            }
            let u = (t - seg.start) / seg.motion.length();
            let shown = match seg.motion {
                Motion::Fall | Motion::Sink => Some(Showing::Once {
                    clip: Clip::Death,
                    progress: u.min(1.0),
                }),
                Motion::Attack { .. } if u < 1.0 => Some(Showing::Once {
                    clip: Clip::Attack,
                    progress: u,
                }),
                Motion::Cheer { .. } if u < 1.0 => Some(Showing::Once {
                    clip: Clip::Victory,
                    progress: u,
                }),
                Motion::Step { .. } if u < 1.0 => Some(Showing::Looping {
                    clip: Clip::Run,
                    seconds: t - seg.start,
                }),
                _ => None,
            };
            // Death outlasts everything after it.
            let dead = matches!(
                best,
                Some((
                    _,
                    Showing::Once {
                        clip: Clip::Death,
                        ..
                    }
                ))
            );
            if let Some(shown) = shown
                && !dead
                && best.is_none_or(|(start, _)| seg.start >= start)
            {
                best = Some((seg.start, shown));
            }
        }
        best.map_or(
            Showing::Looping {
                clip: Clip::Idle,
                seconds: t,
            },
            |(_, shown)| shown,
        )
    }

    /// Which way an actor looks `t` seconds in, in screen units: at its foe, or along the step
    /// it is taking or took.
    pub fn heading_at(&self, actor: usize, t: f32) -> Vec2 {
        let a = &self.actors[actor];
        a.motions
            .iter()
            .filter(|seg| t >= seg.start)
            .filter_map(|seg| match seg.motion {
                Motion::Step { by } if by != Vec2::ZERO => Some(by),
                _ => None,
            })
            .last()
            .unwrap_or(a.toward)
    }

    /// An actor's hit points `t` seconds in: each blow so far costs one.
    pub fn hp_at(&self, actor: usize, t: f32) -> i32 {
        let blows = self
            .events
            .iter()
            .filter(|e| e.at <= t && e.event == Event::Blow { target: actor })
            .count() as i32;
        (self.actors[actor].fighter.hp - blows).max(0)
    }
}

struct Builder<'a> {
    setting: &'a Setting<'a>,
    actors: Vec<Actor>,
    events: Vec<Timed>,
    beats: Vec<Beat>,
}

impl<'a> Builder<'a> {
    fn new(setting: &'a Setting<'a>) -> Self {
        Self {
            setting,
            actors: Vec::new(),
            events: Vec::new(),
            beats: Vec::new(),
        }
    }

    /// Where a unit stands on a square.
    fn anchor(&self, tile: Coord) -> Vec2 {
        let (x, y) = tile.screen();
        let at = Vec2::new(x, y);
        if (self.setting.city)(tile) {
            at + CITY_SHIFT
        } else {
            at
        }
    }

    fn actor(&mut self, fighter: &Fighter, role: Role, shift: Vec2) -> usize {
        let def = self.setting.rules.units.get(&fighter.kind);
        let (sea, style) = def.map_or((false, Style::Volley), |d| {
            (d.is_naval(), self.setting.combat.style_of(d))
        });
        let anchor = self.anchor(fighter.position) + shift;
        self.actors.push(Actor {
            fighter: fighter.clone(),
            role,
            sea,
            style,
            anchor,
            toward: Vec2::X,
            flip: false,
            animated: (self.setting.animated)(&fighter.kind),
            captor: None,
            motions: Vec::new(),
        });
        self.actors.len() - 1
    }

    /// Turns `who` toward `foe`. A foe straight above or below leaves the choice of side to the
    /// role: attackers look right, everyone else looks back at them.
    fn face(&mut self, who: usize, foe: usize) {
        let toward = (self.actors[foe].anchor - self.actors[who].anchor).normalize_or(Vec2::X);
        let a = &mut self.actors[who];
        a.toward = toward;
        a.flip = toward.x < -0.5 || (toward.x.abs() <= 0.5 && a.role != Role::Attacker);
    }

    fn face_each_other(&mut self, a: usize, b: usize) {
        self.face(a, b);
        self.face(b, a);
    }

    fn at(&mut self, at: f32, event: Event) {
        self.events.push(Timed { at, event });
    }

    /// A sound, unless the same cue already plays at about this moment (both sides firing the
    /// same kind of gun are heard once).
    fn sound(&mut self, at: f32, cue: &'static str) {
        let twin = |e: &Timed| e.event == Event::Sound(cue) && (e.at - at).abs() < 0.03;
        if !self.events.iter().any(twin) {
            self.at(at, Event::Sound(cue));
        }
    }

    fn motion(&mut self, actor: usize, start: f32, motion: Motion) {
        self.actors[actor].motions.push(Segment { start, motion });
    }

    fn death(&self, actor: usize) -> Motion {
        if self.actors[actor].sea {
            Motion::Sink
        } else {
            Motion::Fall
        }
    }

    /// An actor's swing in a beat that begins at `start` and whose blow lands at `strike`. A
    /// firing style shoots, shells flying from the shot to the blow; a charge reaches its foe
    /// at the blow. `hits` says whether the swing connects.
    fn swing(
        &mut self,
        actor: usize,
        foe: usize,
        start: f32,
        strike: f32,
        variant: u8,
        hits: bool,
    ) {
        let style = self.actors[actor].style;
        let motion = Motion::Attack { style, variant };
        let length = motion.length();
        if style == Style::Charge {
            let from = strike - STRIKE_AT * length;
            self.motion(actor, from, motion);
            self.sound(from, style.cue());
            self.at(strike, Event::Dust { actor: foe });
            return;
        }
        self.motion(actor, start, motion);
        let fire = start + FIRE_AT * length;
        self.sound(fire, style.cue());
        self.at(
            fire,
            Event::Flash {
                actor,
                big: style != Style::Volley,
            },
        );
        self.volley(actor, foe, fire, strike - fire, hits);
    }

    /// The shells of one shot: one, or three for a broadside.
    fn volley(&mut self, from: usize, to: usize, fire: f32, flight: f32, hits: bool) {
        let shells = if self.actors[from].style == Style::Broadside {
            3
        } else {
            1
        };
        for k in 0..shells {
            let launch = fire + k as f32 * SALVO_GAP;
            self.at(
                launch,
                Event::Shell {
                    from,
                    to,
                    flight,
                    hits,
                },
            );
        }
    }

    /// The target of a blow loses a hit point and is knocked back; the impact is heard.
    fn blow(&mut self, at: f32, target: usize) {
        self.at(at, Event::Blow { target });
        self.sound(at, "hit");
        self.motion(target, at, Motion::Flinch);
    }

    #[allow(clippy::too_many_arguments)]
    fn duel(
        &mut self,
        attacker: &Fighter,
        defender: &Fighter,
        support: Option<&Support>,
        rounds: &[bool],
        outcome: Outcome,
        retreat_to: Option<Coord>,
        promoted: bool,
    ) -> f32 {
        let (att, def) = (
            self.actor(attacker, Role::Attacker, Vec2::ZERO),
            self.actor(defender, Role::Defender, Vec2::ZERO),
        );
        self.face_each_other(att, def);
        let mut t = LEAD_IN;
        if let Some(support) = support {
            let gun = self.actor(&support.shooter, Role::Support, SUPPORT_SHIFT);
            self.face(gun, att);
            let length = Motion::Attack {
                style: self.actors[gun].style,
                variant: 0,
            }
            .length();
            let strike = t + STRIKE_AT * length;
            self.swing(gun, att, t, strike, 0, support.hit);
            if support.hit {
                self.blow(strike, att);
            }
            self.beats.push(Beat { start: t, length });
            t += length;
        }
        for (round, &attacker_won) in rounds.iter().enumerate() {
            let variant = (round % 2) as u8;
            let length = [att, def]
                .map(|a| {
                    let style = self.actors[a].style;
                    Motion::Attack { style, variant }.length()
                })
                .into_iter()
                .fold(0.0f32, f32::max);
            let strike = t + STRIKE_AT * length;
            self.swing(att, def, t, strike, variant, attacker_won);
            self.swing(def, att, t, strike, variant, !attacker_won);
            self.blow(strike, if attacker_won { def } else { att });
            self.beats.push(Beat { start: t, length });
            t += length;
        }
        let mut longest = 0.0f32;
        let mut play = |b: &mut Self, actor: usize, delay: f32, motion: Motion| {
            b.motion(actor, t + delay, motion);
            longest = longest.max(delay + motion.length());
        };
        match outcome {
            Outcome::AttackerWon | Outcome::DefenderWon => {
                let (winner, loser) = if outcome == Outcome::AttackerWon {
                    (att, def)
                } else {
                    (def, att)
                };
                let death = self.death(loser);
                play(self, loser, 0.0, death);
                self.sound(
                    t,
                    if death == Motion::Sink {
                        "sink"
                    } else {
                        "fall"
                    },
                );
                let sea = self.actors[winner].sea;
                play(self, winner, 0.1, Motion::Cheer { sea });
                if promoted {
                    self.at(t + 0.1, Event::Sparkle { actor: winner });
                }
            }
            Outcome::DefenderRetreated => {
                let to = retreat_to.unwrap_or(defender.position);
                let by = self.anchor(to) - self.anchor(defender.position);
                play(self, def, 0.0, Motion::Step { by });
            }
            Outcome::AttackerRetreated => play(self, att, 0.0, Motion::Withdraw),
        }
        t + longest.max(MIN_FINALE)
    }

    fn capture(
        &mut self,
        attacker: &Fighter,
        target: Coord,
        taken: &[Fighter],
        destroyed: &[Fighter],
        advanced: bool,
    ) -> f32 {
        let att = self.actor(attacker, Role::Attacker, Vec2::ZERO);
        let spread =
            |i: usize, n: usize| Vec2::new((i as f32 - (n as f32 - 1.0) / 2.0) * 18.0, 0.0);
        let all = taken.len() + destroyed.len();
        let mut captives = Vec::new();
        for (i, f) in taken.iter().chain(destroyed).enumerate() {
            let mut at = f.clone();
            at.position = target;
            captives.push((
                self.actor(&at, Role::Captive, spread(i, all)),
                i < taken.len(),
            ));
        }
        let aim = captives.first().map(|c| c.0);
        let t = LEAD_IN;
        let mut longest = 0.0f32;
        match aim {
            Some(first) => {
                self.face(att, first);
                for &(captive, _) in &captives {
                    self.face(captive, att);
                }
            }
            None => {
                // Nothing left to show but the attacker's step onto an empty square.
                let toward = (self.anchor(target) - self.actors[att].anchor).normalize_or(Vec2::X);
                self.actors[att].toward = toward;
            }
        }
        for (captive, was_taken) in captives {
            if was_taken {
                self.actors[captive].captor = Some(attacker.owner);
                self.motion(captive, t, Motion::Yield);
                self.sound(t, "yield");
                longest = longest.max(Motion::Yield.length());
            } else {
                let death = self.death(captive);
                self.motion(captive, t, death);
                self.sound(
                    t,
                    if death == Motion::Sink {
                        "sink"
                    } else {
                        "fall"
                    },
                );
                longest = longest.max(death.length());
            }
        }
        if advanced {
            let by = self.anchor(target) - self.actors[att].anchor;
            self.motion(att, t, Motion::Step { by });
            longest = longest.max(STEP_SECS);
        }
        t + longest.max(MIN_FINALE)
    }

    fn bombard(
        &mut self,
        shooter: &Fighter,
        target: &Fighter,
        shots: &[bool],
        killed: bool,
        promoted: bool,
    ) -> f32 {
        let (gun, mark) = (
            self.actor(shooter, Role::Attacker, Vec2::ZERO),
            self.actor(target, Role::Defender, Vec2::ZERO),
        );
        self.face_each_other(gun, mark);
        let distance = (self.actors[mark].anchor - self.actors[gun].anchor).length();
        let style = self.actors[gun].style;
        let mut t = LEAD_IN;
        for (i, &hit) in shots.iter().enumerate() {
            let motion = Motion::Attack {
                style,
                variant: (i % 2) as u8,
            };
            let length = motion.length();
            let (fire, flight) = if style == Style::Charge {
                // A melee style on a ranged attack: the blow is the lunge's contact.
                (t, STRIKE_AT * length)
            } else {
                (
                    t + FIRE_AT * length,
                    (distance / SHELL_SPEED).clamp(SHELL_FLIGHT.0, SHELL_FLIGHT.1),
                )
            };
            // A broadside's last shell trails the first by two gaps.
            let salvo = if style == Style::Broadside {
                2.0 * SALVO_GAP
            } else {
                0.0
            };
            self.motion(gun, t, motion);
            self.sound(fire, style.cue());
            if style != Style::Charge {
                self.at(
                    fire,
                    Event::Flash {
                        actor: gun,
                        big: style != Style::Volley,
                    },
                );
                self.volley(gun, mark, fire, flight, hit);
            }
            if hit {
                self.blow(fire + flight, mark);
            }
            let beat = length.max(fire + flight + salvo - t);
            self.beats.push(Beat {
                start: t,
                length: beat,
            });
            t += beat;
        }
        let mut longest = 0.0f32;
        if killed {
            let death = self.death(mark);
            self.motion(mark, t, death);
            self.sound(
                t,
                if death == Motion::Sink {
                    "sink"
                } else {
                    "fall"
                },
            );
            longest = death.length();
            let sea = self.actors[gun].sea;
            self.motion(gun, t + 0.1, Motion::Cheer { sea });
            longest = longest.max(0.1 + Motion::Cheer { sea }.length());
            if promoted {
                self.at(t + 0.1, Event::Sparkle { actor: gun });
            }
        }
        t + longest.max(MIN_FINALE)
    }

    fn finish(mut self, finale_end: f32) -> Script {
        self.events.sort_by(|a, b| a.at.total_cmp(&b.at));
        Script {
            actors: self.actors,
            events: self.events,
            length: finale_end + TAIL,
            #[cfg(test)]
            finale: self.beats.last().map_or(LEAD_IN, |b| b.start + b.length),
            #[cfg(test)]
            beats: self.beats,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fourx_content::Pack;
    use fourx_sim::Support;

    const RED: Id = 1;
    const BLUE: Id = 2;

    fn fighter(id: Id, owner: Id, kind: &str, x: i32, y: i32, hp: i32) -> Fighter {
        Fighter {
            id,
            owner,
            kind: kind.into(),
            position: Coord::new(x, y),
            hp,
            max_hp: hp.max(3),
        }
    }

    fn build(battle: &Battle) -> Script {
        build_with(battle, &CombatVisuals::default(), &|_| false)
    }

    fn build_with(battle: &Battle, combat: &CombatVisuals, city: &dyn Fn(Coord) -> bool) -> Script {
        let pack = Pack::base();
        Script::build(
            battle,
            &Setting {
                rules: &pack.rules,
                combat,
                city,
                animated: &|_| false,
            },
        )
    }

    fn build_animated(battle: &Battle) -> Script {
        let pack = Pack::base();
        Script::build(
            battle,
            &Setting {
                rules: &pack.rules,
                combat: &CombatVisuals::default(),
                city: &|_| false,
                animated: &|_| true,
            },
        )
    }

    fn duel(kinds: (&str, &str), rounds: &[bool], outcome: Outcome) -> Battle {
        Battle::Duel {
            attacker: fighter(1, RED, kinds.0, 4, 4, 4),
            defender: fighter(2, BLUE, kinds.1, 5, 5, 4),
            support: None,
            rounds: rounds.to_vec(),
            outcome,
            retreat_to: None,
            promoted: false,
        }
    }

    fn blows(script: &Script) -> Vec<(f32, usize)> {
        script
            .events
            .iter()
            .filter_map(|e| match e.event {
                Event::Blow { target } => Some((e.at, target)),
                _ => None,
            })
            .collect()
    }

    fn clip_at(script: &Script, actor: usize, t: f32) -> Clip {
        match script.showing(actor, t) {
            Showing::Once { clip, .. } | Showing::Looping { clip, .. } => clip,
        }
    }

    #[test]
    fn actors_stand_swing_every_round_and_the_loser_dies_while_the_winner_cheers() {
        let script = build_animated(&duel(
            ("infantry", "infantry"),
            &[true, false, true],
            Outcome::AttackerWon,
        ));
        let (att, def) = (0, 1);
        assert_eq!(clip_at(&script, att, 0.1), Clip::Idle, "the lead-in");
        for beat in &script.beats {
            let middle = beat.start + beat.length * 0.5;
            for actor in [att, def] {
                assert_eq!(clip_at(&script, actor, middle), Clip::Attack);
            }
        }
        let after = script.finale + 0.3;
        assert_eq!(clip_at(&script, def, after), Clip::Death);
        assert_eq!(clip_at(&script, att, after), Clip::Victory);
        // The dead hold their last frame to the end; the living stand again.
        assert_eq!(
            script.showing(def, script.length),
            Showing::Once {
                clip: Clip::Death,
                progress: 1.0
            }
        );
        assert_eq!(clip_at(&script, att, script.length), Clip::Idle);
    }

    #[test]
    fn a_swing_runs_through_its_clip_from_the_start_of_its_beat_to_the_end() {
        let script = build_animated(&duel(
            ("infantry", "cavalry"),
            &[true],
            Outcome::AttackerWon,
        ));
        let beat = script.beats[0];
        let progress = |t: f32| match script.showing(0, t) {
            Showing::Once {
                clip: Clip::Attack,
                progress,
            } => progress,
            other => panic!("{other:?}"),
        };
        assert!(progress(beat.start + 0.01) < 0.05);
        assert!((progress(beat.start + beat.length * FIRE_AT) - FIRE_AT).abs() < 0.02);
        assert!(progress(beat.start + beat.length - 0.01) > 0.95);
    }

    #[test]
    fn a_retreat_walks_and_looks_the_way_it_goes() {
        let mut battle = duel(("cavalry", "infantry"), &[true], Outcome::DefenderRetreated);
        if let Battle::Duel { retreat_to, .. } = &mut battle {
            *retreat_to = Some(Coord::new(6, 6));
        }
        let script = build_animated(&battle);
        let during = script.finale + STEP_SECS * 0.5;
        assert_eq!(clip_at(&script, 1, during), Clip::Run);
        let away = script.heading_at(1, during);
        assert!(
            away.dot(script.actors[1].toward) < 0.0,
            "it turns its back on the foe"
        );
        assert_eq!(script.heading_at(1, 0.0), script.actors[1].toward);
    }

    #[test]
    fn clips_replace_the_tilt_and_the_topple_but_not_the_fade() {
        let battle = duel(("infantry", "infantry"), &[true], Outcome::AttackerWon);
        let (plain, animated) = (build(&battle), build_animated(&battle));
        assert!(animated.actors.iter().all(|a| a.animated));
        let late = plain.finale + Motion::Fall.length() * 0.95;
        let fallen = plain.pose_at(1, late);
        let lying = animated.pose_at(1, late);
        assert!(fallen.tilt.abs() > 1.0 && fallen.offset.length() > 5.0);
        assert_eq!((lying.tilt, lying.offset), (0.0, Vec2::ZERO));
        assert_eq!(lying.alpha, fallen.alpha);
        assert!(lying.alpha < 0.5);
    }

    fn motions(script: &Script, pick: impl Fn(&Motion) -> bool) -> usize {
        script
            .actors
            .iter()
            .flat_map(|a| &a.motions)
            .filter(|s| pick(&s.motion))
            .count()
    }

    #[test]
    fn each_round_has_one_blow_at_the_strike_point_of_its_beat() {
        let rounds = [true, false, false, true, true];
        let script = build(&duel(
            ("infantry", "infantry"),
            &rounds,
            Outcome::AttackerWon,
        ));
        let blows = blows(&script);
        assert_eq!(blows.len(), rounds.len());
        assert_eq!(script.beats.len(), rounds.len());
        for ((beat, blow), &attacker_won) in script.beats.iter().zip(&blows).zip(&rounds) {
            let strike = beat.start + STRIKE_AT * beat.length;
            assert!((blow.0 - strike).abs() < 1e-4, "{blow:?} vs {strike}");
            // The loser of the round is the one struck: the defender is actor 1.
            assert_eq!(blow.1, if attacker_won { 1 } else { 0 });
        }
        // Beats follow one another from the lead-in, none overlapping.
        assert!((script.beats[0].start - LEAD_IN).abs() < 1e-5);
        for pair in script.beats.windows(2) {
            assert!((pair[0].start + pair[0].length - pair[1].start).abs() < 1e-5);
        }
    }

    #[test]
    fn no_blow_comes_before_its_beat_and_each_beat_holds_exactly_one() {
        let rounds = [false, true, true, false, true, true, true];
        let script = build(&duel(
            ("cavalry", "infantry"),
            &rounds,
            Outcome::AttackerWon,
        ));
        for beat in &script.beats {
            let inside = blows(&script)
                .into_iter()
                .filter(|(at, _)| *at >= beat.start && *at < beat.start + beat.length)
                .count();
            assert_eq!(inside, 1, "{beat:?}");
        }
    }

    #[test]
    fn both_sides_swing_every_round_whoever_wins() {
        let rounds = [true, false, true];
        let script = build(&duel(
            ("infantry", "infantry"),
            &rounds,
            Outcome::AttackerWon,
        ));
        for actor in 0..2 {
            let swings = script.actors[actor]
                .motions
                .iter()
                .filter(|s| matches!(s.motion, Motion::Attack { .. }))
                .count();
            assert_eq!(swings, rounds.len(), "actor {actor}");
        }
        // Consecutive rounds alternate their variant.
        let variants: Vec<u8> = script.actors[0]
            .motions
            .iter()
            .filter_map(|s| match s.motion {
                Motion::Attack { variant, .. } => Some(variant),
                _ => None,
            })
            .collect();
        assert_eq!(variants, [0, 1, 0]);
    }

    #[test]
    fn a_round_lasts_as_long_as_the_longer_swing() {
        // A broadside (1.0 s) against a volley (0.8 s).
        let script = build(&duel(
            ("ironclad", "infantry"),
            &[true],
            Outcome::AttackerWon,
        ));
        assert!((script.beats[0].length - 1.0).abs() < 1e-5);
    }

    #[test]
    fn the_winners_shell_hits_and_the_losers_falls_short() {
        for attacker_won in [true, false] {
            let script = build(&duel(
                ("infantry", "infantry"),
                &[attacker_won],
                Outcome::AttackerWon,
            ));
            let shells: Vec<_> = script
                .events
                .iter()
                .filter_map(|e| match e.event {
                    Event::Shell { from, hits, .. } => Some((from, hits)),
                    _ => None,
                })
                .collect();
            assert_eq!(shells.len(), 2);
            for (from, hits) in shells {
                assert_eq!(hits, (from == 0) == attacker_won, "actor {from}");
            }
        }
    }

    #[test]
    fn a_shell_lands_exactly_at_the_blow() {
        let script = build(&duel(
            ("infantry", "artillery"),
            &[true, false],
            Outcome::AttackerWon,
        ));
        for shell in script
            .events
            .iter()
            .filter(|e| matches!(e.event, Event::Shell { .. }))
        {
            let Event::Shell { flight, .. } = shell.event else {
                unreachable!()
            };
            assert!(flight > 0.0);
            let lands = shell.at + flight;
            assert!(
                blows(&script)
                    .iter()
                    .any(|(at, _)| (at - lands).abs() < 1e-4),
                "a shell lands at {lands}, away from any blow"
            );
        }
    }

    #[test]
    fn a_charge_makes_contact_at_the_blow() {
        let mut combat = CombatVisuals::default();
        combat
            .styles
            .insert("cavalry".into(), fourx_content::combat::Style::Charge);
        let script = build_with(
            &duel(("cavalry", "infantry"), &[true, true], Outcome::AttackerWon),
            &combat,
            &|_| false,
        );
        let charges: Vec<_> = script.actors[0]
            .motions
            .iter()
            .filter(|s| {
                matches!(
                    s.motion,
                    Motion::Attack {
                        style: Style::Charge,
                        ..
                    }
                )
            })
            .collect();
        assert_eq!(charges.len(), 2);
        for (seg, (blow, _)) in charges.iter().zip(blows(&script)) {
            let contact = seg.start + STRIKE_AT * seg.motion.length();
            assert!((contact - blow).abs() < 1e-4);
            assert!(seg.start >= LEAD_IN - 1e-5);
        }
    }

    #[test]
    fn a_support_shot_comes_first_and_hits_only_if_recorded() {
        for hit in [true, false] {
            let battle = Battle::Duel {
                attacker: fighter(1, RED, "infantry", 4, 4, 4),
                defender: fighter(2, BLUE, "infantry", 5, 5, 4),
                support: Some(Support {
                    shooter: fighter(3, BLUE, "artillery", 5, 5, 3),
                    hit,
                }),
                rounds: vec![true, true, true, true],
                outcome: Outcome::AttackerWon,
                retreat_to: None,
                promoted: false,
            };
            let script = build(&battle);
            assert_eq!(script.actors.len(), 3);
            assert_eq!(script.actors[2].role, Role::Support);
            // One beat for the shot, then four rounds.
            assert_eq!(script.beats.len(), 5);
            let struck = blows(&script);
            assert_eq!(struck.len(), 4 + usize::from(hit));
            let attacker_blows: Vec<_> = struck.iter().filter(|(_, t)| *t == 0).collect();
            assert_eq!(attacker_blows.len(), usize::from(hit));
            if hit {
                assert!(
                    attacker_blows[0].0 < script.beats[1].start,
                    "before round one"
                );
            }
            // The attacker takes no swing at the gun.
            assert_eq!(
                script.actors[0]
                    .motions
                    .iter()
                    .filter(|s| matches!(s.motion, Motion::Attack { .. }))
                    .count(),
                4
            );
            // The gun fires once, and its shell hits or falls short as recorded.
            let shells: Vec<_> = script
                .events
                .iter()
                .filter_map(|e| match e.event {
                    Event::Shell { from: 2, hits, .. } => Some(hits),
                    _ => None,
                })
                .collect();
            assert_eq!(shells, [hit]);
        }
    }

    #[test]
    fn a_kill_has_exactly_one_death_and_it_is_the_loser() {
        for (outcome, loser, rounds) in [
            (Outcome::AttackerWon, 1, [true, true, true, true]),
            (Outcome::DefenderWon, 0, [false, false, false, false]),
        ] {
            let script = build(&duel(("infantry", "infantry"), &rounds, outcome));
            assert_eq!(
                motions(&script, |m| matches!(m, Motion::Fall | Motion::Sink)),
                1
            );
            let dies = script.actors[loser]
                .motions
                .iter()
                .any(|s| s.motion == Motion::Fall);
            assert!(dies, "{outcome:?}");
            // The winner cheers instead.
            let winner = 1 - loser;
            assert!(
                script.actors[winner]
                    .motions
                    .iter()
                    .any(|s| matches!(s.motion, Motion::Cheer { .. }))
            );
            // The death comes after the last blow.
            let death = script.actors[loser].motions.last().unwrap();
            assert!(death.start >= script.finale - 1e-5);
        }
    }

    #[test]
    fn ships_sink_and_men_fall() {
        let script = build(&duel(
            ("battleship", "ironclad"),
            &[true, true, true],
            Outcome::AttackerWon,
        ));
        assert_eq!(motions(&script, |m| *m == Motion::Sink), 1);
        assert_eq!(motions(&script, |m| *m == Motion::Fall), 0);
        assert!(
            script
                .events
                .iter()
                .any(|e| e.event == Event::Sound("sink"))
        );
        assert!(
            script.actors[0]
                .motions
                .iter()
                .any(|s| s.motion == Motion::Cheer { sea: true })
        );
    }

    #[test]
    fn a_withdrawal_shows_no_death() {
        let mut battle = duel(
            ("infantry", "infantry"),
            &[true, true],
            Outcome::DefenderRetreated,
        );
        if let Battle::Duel { retreat_to, .. } = &mut battle {
            *retreat_to = Some(Coord::new(6, 6));
        }
        let script = build(&battle);
        assert_eq!(
            motions(&script, |m| matches!(m, Motion::Fall | Motion::Sink)),
            0
        );
        assert_eq!(motions(&script, |m| matches!(m, Motion::Cheer { .. })), 0);
        let step = script.actors[1].motions.last().unwrap();
        let (a, b) = (Coord::new(5, 5).screen(), Coord::new(6, 6).screen());
        let Motion::Step { by } = step.motion else {
            panic!("the defender steps back: {step:?}");
        };
        assert!(by.distance(Vec2::new(b.0 - a.0, b.1 - a.1)) < 1e-3);
        // At the end the defender stands on the square it withdrew to.
        let end = script.pose_at(1, script.length);
        assert!(end.offset.distance(by) < 1e-3 && end.alpha > 0.99);

        let mut battle = duel(
            ("infantry", "infantry"),
            &[false, false],
            Outcome::AttackerRetreated,
        );
        if let Battle::Duel { retreat_to, .. } = &mut battle {
            *retreat_to = None;
        }
        let script = build(&battle);
        assert_eq!(
            motions(&script, |m| matches!(m, Motion::Fall | Motion::Sink)),
            0
        );
        assert_eq!(motions(&script, |m| *m == Motion::Withdraw), 1);
        assert_eq!(script.pose_at(0, script.length), Pose::REST);
    }

    #[test]
    fn a_promotion_sparkles_on_the_winner_only() {
        let mut battle = duel(
            ("infantry", "infantry"),
            &[true, true, true, true],
            Outcome::AttackerWon,
        );
        if let Battle::Duel { promoted, .. } = &mut battle {
            *promoted = true;
        }
        let script = build(&battle);
        let sparkles: Vec<_> = script
            .events
            .iter()
            .filter_map(|e| match e.event {
                Event::Sparkle { actor } => Some(actor),
                _ => None,
            })
            .collect();
        assert_eq!(sparkles, [0]);
        let plain = build(&duel(
            ("infantry", "infantry"),
            &[true; 4],
            Outcome::AttackerWon,
        ));
        assert!(
            !plain
                .events
                .iter()
                .any(|e| matches!(e.event, Event::Sparkle { .. }))
        );
    }

    #[test]
    fn hit_points_fall_by_one_with_each_blow() {
        let rounds = [true, false, true, true, true];
        let script = build(&duel(
            ("infantry", "infantry"),
            &rounds,
            Outcome::AttackerWon,
        ));
        let blow_times = blows(&script);
        // Before the first blow, both are as recorded.
        assert_eq!(script.hp_at(0, 0.0), 4);
        assert_eq!(script.hp_at(1, 0.0), 4);
        for (i, (at, target)) in blow_times.iter().enumerate() {
            let before = script.hp_at(*target, at - 0.001);
            let after = script.hp_at(*target, *at);
            assert_eq!(before - after, 1, "blow {i}");
        }
        assert_eq!(script.hp_at(1, script.length), 0);
        assert_eq!(script.hp_at(0, script.length), 3);
    }

    #[test]
    fn length_is_lead_in_plus_beats_plus_finale_plus_tail() {
        let rounds = [true, false, true, true];
        let script = build(&duel(
            ("infantry", "infantry"),
            &rounds,
            Outcome::AttackerWon,
        ));
        let beats: f32 = script.beats.iter().map(|b| b.length).sum();
        assert!((script.finale - (LEAD_IN + beats)).abs() < 1e-4);
        let finale = Motion::Fall
            .length()
            .max(0.1 + Motion::Cheer { sea: false }.length());
        assert!((script.length - (LEAD_IN + beats + finale.max(MIN_FINALE) + TAIL)).abs() < 1e-4);
        // Everything scheduled sits inside the script.
        for actor in &script.actors {
            for seg in &actor.motions {
                assert!(
                    seg.start >= 0.0 && seg.end() <= script.length + 1e-4,
                    "{seg:?}"
                );
            }
        }
        assert!(
            script
                .events
                .iter()
                .all(|e| e.at >= LEAD_IN - 1e-5 && e.at <= script.length)
        );
        assert!(script.events.windows(2).all(|w| w[0].at <= w[1].at));
    }

    #[test]
    fn a_long_queue_speeds_play_up_but_never_beyond_four_times() {
        let script = build(&duel(
            ("infantry", "infantry"),
            &[true; 4],
            Outcome::AttackerWon,
        ));
        assert!(script.length > 0.0);
        assert_eq!(pace(0, 1.0), 1.0);
        assert_eq!(pace(2, 1.0), 1.0);
        assert!((pace(4, 1.0) - 1.5).abs() < 1e-6);
        assert!((pace(6, 2.0) - 4.0).abs() < 1e-6);
        assert_eq!(pace(500, 1.0), 4.0, "never more than four times");
    }

    #[test]
    fn actors_start_at_rest_and_a_loser_ends_unseen() {
        let script = build(&duel(
            ("infantry", "infantry"),
            &[true; 4],
            Outcome::AttackerWon,
        ));
        for actor in 0..script.actors.len() {
            assert_eq!(script.pose_at(actor, 0.0), Pose::REST);
        }
        assert!(script.pose_at(1, script.length).alpha < 1e-3);
        assert_eq!(script.pose_at(0, script.length), Pose::REST);
    }

    #[test]
    fn sounds_are_only_the_cues_a_pack_can_declare() {
        for battle in [
            duel(
                ("ironclad", "infantry"),
                &[true, false, true, true],
                Outcome::AttackerWon,
            ),
            duel(("infantry", "artillery"), &[false; 3], Outcome::DefenderWon),
        ] {
            let script = build(&battle);
            for e in &script.events {
                if let Event::Sound(cue) = e.event {
                    assert!(fourx_content::combat::CUES.contains(&cue), "{cue}");
                }
            }
        }
    }

    #[test]
    fn the_same_cue_is_not_played_twice_at_once() {
        let script = build(&duel(
            ("infantry", "infantry"),
            &[true],
            Outcome::AttackerWon,
        ));
        let volleys = script
            .events
            .iter()
            .filter(|e| e.event == Event::Sound("volley"))
            .count();
        assert_eq!(volleys, 1);
    }

    #[test]
    fn attackers_face_their_foe_and_a_straight_line_is_broken_by_role() {
        // Defender to the right (5,5 is to the up-left? decided by the real geometry).
        let script = build(&duel(
            ("infantry", "infantry"),
            &[true],
            Outcome::AttackerWon,
        ));
        let (a, d) = (&script.actors[0], &script.actors[1]);
        assert!(a.toward.dot(d.toward) < -0.99, "they look at each other");
        assert_eq!(a.flip, a.toward.x < -0.5);
        // Diagonal neighbours sit straight above one another: the attacker looks right and the
        // defender left, rather than both looking the same way.
        let (a, d) = (&script.actors[0], &script.actors[1]);
        assert!(a.toward.x.abs() < 0.5, "(4,4) and (5,5) are in one column");
        assert!(!a.flip && d.flip);
    }

    #[test]
    fn units_in_a_city_stand_beside_it() {
        let in_city = |c: Coord| c == Coord::new(5, 5);
        let script = build_with(
            &duel(("infantry", "infantry"), &[true], Outcome::AttackerWon),
            &CombatVisuals::default(),
            &in_city,
        );
        let (x, y) = Coord::new(5, 5).screen();
        assert!(
            script.actors[1]
                .anchor
                .distance(Vec2::new(x + 62.0, y + 24.0))
                < 1e-3
        );
        let (x, y) = Coord::new(4, 4).screen();
        assert!(script.actors[0].anchor.distance(Vec2::new(x, y)) < 1e-3);
    }

    fn capture(taken: usize, destroyed: usize, advanced: bool) -> Battle {
        Battle::Capture {
            attacker: fighter(1, RED, "infantry", 7, 7, 4),
            target: Coord::new(8, 8),
            taken: (0..taken)
                .map(|i| fighter(10 + i as Id, BLUE, "worker", 8, 8, 2))
                .collect(),
            destroyed: (0..destroyed)
                .map(|i| fighter(20 + i as Id, BLUE, "ironclad", 8, 8, 3))
                .collect(),
            city: advanced.then(|| "Blueton".to_string()),
            advanced,
        }
    }

    #[test]
    fn a_capture_yields_the_taken_sinks_the_lost_and_steps_in() {
        let script = build(&capture(2, 1, true));
        assert_eq!(script.actors.len(), 4);
        assert_eq!(motions(&script, |m| *m == Motion::Yield), 2);
        assert_eq!(motions(&script, |m| *m == Motion::Sink), 1);
        assert_eq!(motions(&script, |m| matches!(m, Motion::Step { .. })), 1);
        assert!(blows(&script).is_empty() && script.beats.is_empty());
        for actor in &script.actors {
            if actor.role == Role::Captive
                && actor.motions.iter().any(|s| s.motion == Motion::Yield)
            {
                assert_eq!(actor.captor, Some(RED));
            }
        }
        // The attacker ends on the square, the taken in the captor's colour, the lost gone.
        let end = script.length;
        let (a, b) = (Coord::new(7, 7).screen(), Coord::new(8, 8).screen());
        assert!(
            script
                .pose_at(0, end)
                .offset
                .distance(Vec2::new(b.0 - a.0, b.1 - a.1))
                < 1e-3
        );
        assert!(script.pose_at(1, end).tint > 0.99);
        assert!(script.pose_at(3, end).alpha < 1e-3);
        assert!(script.length >= LEAD_IN + MIN_FINALE + TAIL - 1e-5);
    }

    #[test]
    fn capturing_a_lone_worker_has_no_step() {
        let script = build(&capture(1, 0, false));
        assert_eq!(motions(&script, |m| matches!(m, Motion::Step { .. })), 0);
        assert_eq!(motions(&script, |m| *m == Motion::Yield), 1);
        assert!(
            script
                .events
                .iter()
                .any(|e| e.event == Event::Sound("yield"))
        );
    }

    #[test]
    fn captives_are_spread_across_the_square() {
        let script = build(&capture(3, 0, false));
        let xs: Vec<f32> = script.actors[1..].iter().map(|a| a.anchor.x).collect();
        assert!(xs[0] < xs[1] && xs[1] < xs[2]);
    }

    fn bombard(shots: &[bool], killed: bool, kind: &str, target: (i32, i32)) -> Battle {
        Battle::Bombard {
            shooter: fighter(1, RED, kind, 4, 4, 3),
            target: fighter(2, BLUE, "infantry", target.0, target.1, 2),
            shots: shots.to_vec(),
            killed,
            promoted: killed,
        }
    }

    #[test]
    fn a_bombardment_has_a_beat_per_volley_and_a_blow_per_hit() {
        let shots = [true, false, true];
        let script = build(&bombard(&shots, false, "artillery", (5, 5)));
        assert_eq!(script.beats.len(), 3);
        let struck = blows(&script);
        assert_eq!(struck.len(), 2);
        assert!(struck.iter().all(|(_, t)| *t == 1));
        // Each hit's blow falls inside the beat of the volley that fired it.
        for (beat, &hit) in script.beats.iter().zip(&shots) {
            let inside = struck
                .iter()
                .filter(|(at, _)| *at >= beat.start && *at < beat.start + beat.length)
                .count();
            assert_eq!(inside, usize::from(hit), "{beat:?}");
        }
        // The target does nothing: it only takes what comes.
        assert!(
            script.actors[1]
                .motions
                .iter()
                .all(|s| s.motion == Motion::Flinch)
        );
        // A target left standing means no death and no cheer.
        assert_eq!(
            motions(&script, |m| matches!(
                m,
                Motion::Fall | Motion::Sink | Motion::Cheer { .. }
            )),
            0
        );
    }

    #[test]
    fn a_shell_lands_where_the_target_takes_the_blow() {
        let script = build(&bombard(&[true], false, "artillery", (6, 6)));
        let shell = script
            .events
            .iter()
            .find_map(|e| match e.event {
                Event::Shell { flight, .. } => Some((e.at, flight)),
                _ => None,
            })
            .unwrap();
        assert!((shell.0 + shell.1 - blows(&script)[0].0).abs() < 1e-4);
        assert!((SHELL_FLIGHT.0..=SHELL_FLIGHT.1).contains(&shell.1));
    }

    #[test]
    fn shell_flight_grows_with_distance_within_its_limits() {
        let flight = |at: (i32, i32)| {
            let script = build(&bombard(&[true], false, "artillery", at));
            script
                .events
                .iter()
                .find_map(|e| match e.event {
                    Event::Shell { flight, .. } => Some(flight),
                    _ => None,
                })
                .unwrap()
        };
        // Two and three squares out are both inside the limits; next door and far away are not.
        let (two, three) = (flight((8, 4)), flight((10, 4)));
        assert!(
            three > two && two > SHELL_FLIGHT.0 && three < SHELL_FLIGHT.1,
            "{two} {three}"
        );
        assert_eq!(flight((6, 4)), SHELL_FLIGHT.0);
        assert_eq!(flight((4, 30)), SHELL_FLIGHT.1);
    }

    #[test]
    fn a_bombardment_that_kills_ends_with_the_death_and_a_cheer() {
        let script = build(&bombard(&[false, true], true, "artillery", (5, 5)));
        assert_eq!(motions(&script, |m| *m == Motion::Fall), 1);
        assert_eq!(
            script.hp_at(1, script.length),
            1,
            "one point lost before it fell"
        );
        assert!(script.pose_at(1, script.length).alpha < 1e-3);
        assert!(
            script
                .events
                .iter()
                .any(|e| matches!(e.event, Event::Sparkle { actor: 0 }))
        );
        let death = script.actors[1].motions.last().unwrap();
        assert!(death.start >= script.finale - 1e-5);
    }

    #[test]
    fn a_broadside_fires_three_shells_but_costs_one_hit_point() {
        let script = build(&bombard(&[true], false, "protected-cruiser", (5, 5)));
        let shells = script
            .events
            .iter()
            .filter(|e| matches!(e.event, Event::Shell { .. }))
            .count();
        assert_eq!(shells, 3);
        assert_eq!(blows(&script).len(), 1);
        // The beat is long enough for the last shell to land.
        let last = script
            .events
            .iter()
            .filter_map(|e| match e.event {
                Event::Shell { flight, .. } => Some(e.at + flight),
                _ => None,
            })
            .fold(0.0, f32::max);
        let beat = script.beats[0];
        assert!(last <= beat.start + beat.length + 1e-4);
    }

    #[test]
    fn a_pack_style_changes_the_motion() {
        let mut combat = CombatVisuals::default();
        combat.styles.insert("infantry".into(), Style::Gun);
        let script = build_with(
            &duel(("infantry", "militia"), &[true], Outcome::AttackerWon),
            &combat,
            &|_| false,
        );
        assert!(script.actors[0].motions.iter().any(|s| matches!(
            s.motion,
            Motion::Attack {
                style: Style::Gun,
                ..
            }
        )));
        assert!(script.events.iter().any(|e| e.event == Event::Sound("gun")));
    }

    #[test]
    fn an_unknown_design_still_plays() {
        let battle = Battle::Duel {
            attacker: fighter(1, RED, "mystery", 4, 4, 3),
            defender: fighter(2, BLUE, "infantry", 5, 5, 3),
            support: None,
            rounds: vec![true, true, true],
            outcome: Outcome::AttackerWon,
            retreat_to: None,
            promoted: false,
        };
        let script = build(&battle);
        assert_eq!(blows(&script).len(), 3);
    }

    #[test]
    fn every_battle_is_deterministic() {
        let battle = duel(
            ("cavalry", "infantry"),
            &[true, false, true, true, true],
            Outcome::AttackerWon,
        );
        let (one, two) = (build(&battle), build(&battle));
        assert_eq!(one.events, two.events);
        assert_eq!(one.beats, two.beats);
        assert_eq!(one.length, two.length);
    }
}
