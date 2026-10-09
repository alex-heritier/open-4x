//! Procedural motion: where a sprite is, and how it looks, part-way through a motion.
//!
//! Every motion is a *pose* (offset, tilt, scale, opacity, a white flash, a tint toward another
//! nation) sampled from a progress `u` in `0..=1`. Poses add up, so a unit can flinch while it
//! swings. A design drawn from a single image moves by its pose alone; a design with clips plays
//! the clip and keeps only the part of the pose the clip cannot show (see [`Motion::under_clip`]).
//! See `docs/combat-animation.md` §7.
use bevy::math::Vec2;
use fourx_content::combat::Style;
use std::f32::consts::PI;

/// Fraction of an attack motion at which a firing style shoots.
pub const FIRE_AT: f32 = 0.30;
/// Fraction of an attack beat at which the blow lands.
pub const STRIKE_AT: f32 = 0.50;
/// How long a unit takes to cross to a neighbouring square.
pub const STEP_SECS: f32 = 0.40;

/// How a sprite looks at one instant. `Pose::REST` is a sprite left alone.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    /// Screen units from where the sprite stands; `+y` is up.
    pub offset: Vec2,
    /// The part of `offset` that is a change of square. The hit-point bar goes with it; it does
    /// not go with a lunge, a recoil or a fall.
    pub travel: Vec2,
    /// Radians, counter-clockwise.
    pub tilt: f32,
    pub scale: f32,
    pub alpha: f32,
    /// 0 to 1: how far the sprite is washed to white.
    pub flash: f32,
    /// 0 to 1: how far the sprite has taken on another nation's colour.
    pub tint: f32,
}

impl Pose {
    pub const REST: Pose = Pose {
        offset: Vec2::ZERO,
        travel: Vec2::ZERO,
        tilt: 0.0,
        scale: 1.0,
        alpha: 1.0,
        flash: 0.0,
        tint: 0.0,
    };

    /// Two poses at once: shifts and tilts add, scales and opacities multiply.
    pub fn and(self, other: Pose) -> Pose {
        Pose {
            offset: self.offset + other.offset,
            travel: self.travel + other.travel,
            tilt: self.tilt + other.tilt,
            scale: self.scale * other.scale,
            alpha: self.alpha * other.alpha,
            flash: self.flash.max(other.flash),
            tint: self.tint.max(other.tint),
        }
    }

    /// The largest difference between two poses, in screen units; the other channels count
    /// thirty times over, so that a tenth of a tilt is as visible as three pixels.
    #[cfg(test)]
    pub fn distance(&self, other: &Pose) -> f32 {
        [
            (self.offset - other.offset).length(),
            (self.travel - other.travel).length(),
            (self.tilt - other.tilt).abs() * 30.0,
            (self.scale - other.scale).abs() * 30.0,
            (self.alpha - other.alpha).abs() * 30.0,
            (self.flash - other.flash).abs() * 30.0,
            (self.tint - other.tint).abs() * 30.0,
        ]
        .into_iter()
        .fold(0.0, f32::max)
    }
}

/// One thing a unit does over a stretch of time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Motion {
    /// A swing at the foe in a style. `variant` (0 or 1) alternates from round to round so that
    /// consecutive rounds do not look the same.
    Attack { style: Style, variant: u8 },
    /// Knocked back by a hit.
    Flinch,
    /// A winner's hops (a gentle bob at sea).
    Cheer { sea: bool },
    /// A land unit topples and fades.
    Fall,
    /// A ship lists, goes under, and fades.
    Sink,
    /// A captured unit settles and takes on its captor's colour.
    Yield,
    /// Slides away from the foe and back.
    Withdraw,
    /// Crosses by `by` screen units to another square.
    Step { by: Vec2 },
}

impl Motion {
    /// Seconds at normal speed.
    pub fn length(&self) -> f32 {
        match self {
            Motion::Attack { style, .. } => match style {
                Style::Volley | Style::Charge => 0.80,
                Style::Gun => 0.90,
                Style::Broadside => 1.00,
            },
            Motion::Flinch => 0.20,
            Motion::Cheer { .. } => 0.70,
            Motion::Fall => 0.90,
            Motion::Sink => 1.80,
            Motion::Yield => 0.90,
            Motion::Withdraw => 0.60,
            Motion::Step { .. } => STEP_SECS,
        }
    }

    /// Whether the pose at the end is kept after the motion is over. Every other motion ends
    /// where it began.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Motion::Fall | Motion::Sink | Motion::Yield | Motion::Step { .. }
        )
    }

    /// What is left of a pose when the unit's own clip plays the motion: the clip draws the
    /// stance, the swing, the cheer and the fall, so the sprite is never tilted, and a fall,
    /// a sinking or a cheer does not move it either. Shifts between and toward squares, flashes,
    /// fading and the captor's colour stay.
    pub fn under_clip(&self, pose: Pose) -> Pose {
        let still = matches!(self, Motion::Fall | Motion::Sink | Motion::Cheer { .. });
        Pose {
            tilt: 0.0,
            offset: if still { Vec2::ZERO } else { pose.offset },
            scale: if still { 1.0 } else { pose.scale },
            ..pose
        }
    }

    /// The pose at progress `u` (clamped to `0..=1`). `toward` is a unit vector pointing at the
    /// foe, which decides which way a unit leans, recoils or lunges.
    pub fn sample(&self, u: f32, toward: Vec2) -> Pose {
        let u = u.clamp(0.0, 1.0);
        // +1 when the foe is to the right: a tilt "forward" turns clockwise.
        let side = if toward.x >= 0.0 { 1.0 } else { -1.0 };
        let toward = toward.normalize_or_zero();
        let mut pose = Pose::REST;
        match *self {
            Motion::Attack { style, variant } => {
                let swap = if variant == 0 { 1.0 } else { -1.0 };
                match style {
                    Style::Volley => {
                        let x = after(u, FIRE_AT);
                        pose.offset = -toward * 6.0 * kick(x);
                        pose.tilt = side * (-0.05 * lean(u, 0.0, FIRE_AT) + swap * 0.04 * kick(x));
                    }
                    Style::Gun => {
                        let thrust = keys(
                            u,
                            &[
                                (0.0, 0.0),
                                (0.3, 0.0),
                                (0.38, -12.0),
                                (0.7, -4.0),
                                (1.0, 0.0),
                            ],
                        );
                        pose.offset = toward * thrust;
                        pose.scale = 1.0 + 0.04 * kick(after(u, FIRE_AT));
                        pose.tilt = side * swap * 0.03 * kick(after(u, FIRE_AT));
                    }
                    Style::Charge => {
                        let reach = if variant == 0 { 34.0 } else { 28.0 };
                        let thrust = keys(
                            u,
                            &[(0.0, 0.0), (0.3, -10.0), (STRIKE_AT, reach), (1.0, 0.0)],
                        );
                        pose.offset = toward * thrust;
                        pose.tilt = side * swap * 0.10 * lean(u, 0.3, STRIKE_AT);
                        pose.scale = 1.0 + 0.05 * lean(u, 0.3, STRIKE_AT);
                    }
                    Style::Broadside => {
                        let roll = keys(
                            u,
                            &[
                                (0.0, 0.0),
                                (0.2, -0.05),
                                (0.45, 0.06),
                                (0.75, -0.03),
                                (1.0, 0.0),
                            ],
                        );
                        pose.tilt = swap * roll;
                        pose.offset = -toward * 5.0 * kick(after(u, FIRE_AT));
                    }
                }
            }
            Motion::Flinch => {
                // (`sin` of pi is a hair below zero in floating point.)
                let hump = (PI * u).sin().max(0.0);
                pose.offset = -toward * 8.0 * hump;
                pose.flash = 0.7 * hump;
            }
            Motion::Cheer { sea } => {
                pose.offset.y = if sea {
                    3.0 * (2.0 * PI * u).sin()
                } else {
                    9.0 * (2.0 * PI * u).sin().abs()
                };
            }
            Motion::Fall => {
                let e = smooth(u);
                // Over backwards, away from the foe, and into the ground.
                pose.tilt = side * 1.45 * e;
                pose.offset = Vec2::new(-side * 10.0 * e, -12.0 * e);
                pose.alpha = 1.0 - smooth(span(u, 0.5, 1.0));
            }
            Motion::Sink => {
                let e = smooth(u) * smooth(u);
                pose.tilt = side * 0.35 * smooth(span(u, 0.0, 0.5));
                pose.offset = Vec2::new(0.0, -36.0 * e);
                pose.scale = 1.0 - 0.1 * e;
                pose.alpha = 1.0 - smooth(span(u, 0.45, 1.0));
            }
            Motion::Yield => {
                let e = smooth(u);
                pose.tint = e;
                pose.scale = 1.0 - 0.04 * e;
                pose.offset.y = -3.0 * e;
            }
            Motion::Withdraw => {
                pose.offset = -toward * 16.0 * (PI * u).sin();
            }
            Motion::Step { by } => {
                let e = smooth(u);
                pose.offset = by * e + Vec2::Y * 7.0 * (PI * u).sin();
                // A hop, but the final pose is exactly the destination.
                if u >= 1.0 {
                    pose.offset = by;
                }
                pose.travel = by * e;
            }
        }
        pose
    }
}

fn smooth(u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

/// Progress through `from..to`, held at 0 before and 1 after.
fn span(u: f32, from: f32, to: f32) -> f32 {
    ((u - from) / (to - from)).clamp(0.0, 1.0)
}

/// Progress after a moment, rescaled to `0..=1` over what is left.
fn after(u: f32, from: f32) -> f32 {
    span(u, from, 1.0)
}

/// A sharp onset that eases away: 0 at both ends and 1 at its peak a third of the way through.
fn kick(x: f32) -> f32 {
    6.75 * x * (1.0 - x) * (1.0 - x)
}

/// A rise and fall over `from..to`: 0 outside, 1 in the middle.
fn lean(u: f32, from: f32, to: f32) -> f32 {
    let x = span(u, from, to);
    if u < from || u > to {
        0.0
    } else {
        (PI * x).sin()
    }
}

/// Values at moments, joined smoothly. `points` run in time order from `0.0` to `1.0`.
fn keys(u: f32, points: &[(f32, f32)]) -> f32 {
    for pair in points.windows(2) {
        let ((t0, v0), (t1, v1)) = (pair[0], pair[1]);
        if u <= t1 {
            let x = if t1 > t0 { (u - t0) / (t1 - t0) } else { 1.0 };
            return v0 + (v1 - v0) * smooth(x);
        }
    }
    points.last().map_or(0.0, |p| p.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    const STYLES: [Style; 4] = [Style::Volley, Style::Charge, Style::Gun, Style::Broadside];
    const RIGHT: Vec2 = Vec2::X;

    fn every_motion() -> Vec<Motion> {
        let mut all = vec![
            Motion::Flinch,
            Motion::Cheer { sea: false },
            Motion::Cheer { sea: true },
            Motion::Fall,
            Motion::Sink,
            Motion::Yield,
            Motion::Withdraw,
            Motion::Step {
                by: Vec2::new(64.0, -32.0),
            },
        ];
        for style in STYLES {
            for variant in 0..2 {
                all.push(Motion::Attack { style, variant });
            }
        }
        all
    }

    #[test]
    fn motions_begin_at_rest_and_the_others_end_there() {
        for motion in every_motion() {
            for toward in [RIGHT, -RIGHT, Vec2::new(-0.6, 0.8)] {
                let start = motion.sample(0.0, toward);
                assert!(
                    start.distance(&Pose::REST) < 1e-3,
                    "{motion:?} starts away from rest"
                );
                let end = motion.sample(1.0, toward);
                if !motion.is_terminal() {
                    assert!(
                        end.distance(&Pose::REST) < 1e-3,
                        "{motion:?} ends away from rest"
                    );
                }
            }
        }
    }

    #[test]
    fn poses_change_smoothly() {
        // No sprite jumps: between two samples a 200th of the way apart, nothing moves by more
        // than a few pixels (or the equivalent in the other channels).
        for motion in every_motion() {
            let mut last = motion.sample(0.0, RIGHT);
            for i in 1..=200 {
                let pose = motion.sample(i as f32 / 200.0, RIGHT);
                assert!(last.distance(&pose) < 5.0, "{motion:?} jumps at step {i}");
                last = pose;
            }
        }
    }

    #[test]
    fn samples_are_always_finite_and_clamped() {
        for motion in every_motion() {
            for u in [-3.0, 0.0, 0.37, 1.0, 9.0] {
                let p = motion.sample(u, Vec2::ZERO);
                assert!(p.offset.is_finite() && p.tilt.is_finite() && p.scale.is_finite());
                assert!((0.0..=1.0).contains(&p.alpha), "{motion:?}");
                assert!((0.0..=1.0).contains(&p.flash) && (0.0..=1.0).contains(&p.tint));
            }
            assert_eq!(motion.sample(-3.0, RIGHT), motion.sample(0.0, RIGHT));
            assert_eq!(motion.sample(9.0, RIGHT), motion.sample(1.0, RIGHT));
        }
    }

    #[test]
    fn a_charge_lunges_at_the_foe_and_makes_contact_at_the_strike_point() {
        let charge = Motion::Attack {
            style: Style::Charge,
            variant: 0,
        };
        for toward in [RIGHT, -RIGHT, Vec2::new(0.0, 1.0)] {
            let wind_up = charge.sample(0.3, toward).offset.dot(toward);
            let contact = charge.sample(STRIKE_AT, toward).offset.dot(toward);
            assert!(wind_up < -5.0, "it draws back first: {wind_up}");
            assert!(contact > 25.0, "it reaches the foe: {contact}");
            // The contact is the furthest the lunge goes.
            for i in 0..=100 {
                let at = charge.sample(i as f32 / 100.0, toward).offset.dot(toward);
                assert!(at <= contact + 1e-3);
            }
        }
    }

    #[test]
    fn firing_styles_kick_back_after_they_fire_not_before() {
        for style in [Style::Volley, Style::Gun] {
            let attack = Motion::Attack { style, variant: 0 };
            let before = attack.sample(FIRE_AT * 0.9, RIGHT).offset.x;
            let after = attack.sample(FIRE_AT + 0.12, RIGHT).offset.x;
            assert!(before > -1.0, "{style:?} recoiled early: {before}");
            assert!(after < -3.0, "{style:?} did not recoil: {after}");
        }
    }

    #[test]
    fn consecutive_variants_look_different() {
        for style in STYLES {
            let a = Motion::Attack { style, variant: 0 };
            let b = Motion::Attack { style, variant: 1 };
            let differs = (1..20).any(|i| {
                let u = i as f32 / 20.0;
                a.sample(u, RIGHT).distance(&b.sample(u, RIGHT)) > 0.05
            });
            assert!(differs, "{style:?}");
        }
    }

    #[test]
    fn the_dead_end_unseen_and_the_captured_end_tinted() {
        for death in [Motion::Fall, Motion::Sink] {
            assert!(death.is_terminal());
            assert!(death.sample(1.0, RIGHT).alpha < 1e-4, "{death:?}");
            assert!(
                death.sample(0.2, RIGHT).alpha > 0.99,
                "{death:?} fades too early"
            );
        }
        let captured = Motion::Yield.sample(1.0, RIGHT);
        assert!(captured.tint > 0.99 && captured.alpha > 0.99);
        // Falling is quicker than sinking, and both are longer than a swing's blow.
        assert!(Motion::Fall.length() < Motion::Sink.length());
    }

    #[test]
    fn a_step_ends_exactly_on_the_next_square() {
        let by = Vec2::new(-64.0, -32.0);
        let step = Motion::Step { by };
        assert_eq!(step.sample(1.0, RIGHT).offset, by);
        assert!(step.sample(0.5, RIGHT).offset.distance(by * 0.5) < 8.0);
    }

    #[test]
    fn only_a_step_carries_the_bar_to_another_square() {
        let by = Vec2::new(62.0, 24.0);
        assert_eq!(Motion::Step { by }.sample(1.0, RIGHT).travel, by);
        assert_eq!(Motion::Step { by }.sample(0.0, RIGHT).travel, Vec2::ZERO);
        let charge = Motion::Attack {
            style: Style::Charge,
            variant: 0,
        };
        let others = [
            charge,
            Motion::Flinch,
            Motion::Fall,
            Motion::Sink,
            Motion::Withdraw,
        ];
        for motion in others {
            for u in [0.0, 0.25, 0.5, 0.75, 1.0] {
                assert_eq!(
                    motion.sample(u, RIGHT).travel,
                    Vec2::ZERO,
                    "{motion:?} at {u}"
                );
            }
        }
        // Travel adds up with the rest, like every other channel.
        let both = Motion::Step { by }
            .sample(1.0, RIGHT)
            .and(Motion::Flinch.sample(0.5, RIGHT));
        assert_eq!(both.travel, by);
    }

    #[test]
    fn a_fall_goes_backwards_away_from_the_foe() {
        let right = Motion::Fall.sample(1.0, RIGHT);
        let left = Motion::Fall.sample(1.0, -RIGHT);
        assert!(right.tilt > 0.0 && left.tilt < 0.0);
        assert!(right.offset.x < 0.0 && left.offset.x > 0.0);
    }

    #[test]
    fn a_clip_keeps_the_fade_and_the_travel_but_not_the_tilt() {
        for motion in every_motion() {
            for i in 0..=20 {
                let u = i as f32 / 20.0;
                let full = motion.sample(u, RIGHT);
                let kept = motion.under_clip(full);
                assert_eq!(kept.tilt, 0.0, "{motion:?}");
                assert_eq!(
                    (kept.alpha, kept.flash, kept.tint),
                    (full.alpha, full.flash, full.tint)
                );
                assert_eq!(kept.travel, full.travel, "{motion:?}");
            }
        }
        // The clip lies down, sinks or hops in place; a lunge and a step still move the sprite.
        assert_eq!(
            Motion::Fall
                .under_clip(Motion::Fall.sample(1.0, RIGHT))
                .offset,
            Vec2::ZERO
        );
        assert_eq!(
            Motion::Sink
                .under_clip(Motion::Sink.sample(0.8, RIGHT))
                .offset,
            Vec2::ZERO
        );
        let charge = Motion::Attack {
            style: Style::Charge,
            variant: 0,
        };
        let lunge = charge.under_clip(charge.sample(STRIKE_AT, RIGHT));
        assert!(lunge.offset.x > 25.0);
    }

    #[test]
    fn poses_combine() {
        let swing = Motion::Attack {
            style: Style::Charge,
            variant: 0,
        }
        .sample(STRIKE_AT, RIGHT);
        let flinch = Motion::Flinch.sample(0.5, RIGHT);
        let both = swing.and(flinch);
        assert!(both.flash > 0.5 && both.offset.x < swing.offset.x);
        assert_eq!(Pose::REST.and(swing), swing);
    }
}
