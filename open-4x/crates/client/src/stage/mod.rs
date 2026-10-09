//! Plays recorded battles on the map.
//!
//! The simulation resolves a fight in one step and the snapshot the client receives is already
//! final. The *stage* replays the record: it draws actors built from it, hides the real units
//! on the squares in play so each unit shows once, and gives the real units back when it is
//! done. It never writes to the game. The rules are in `docs/combat-animation.md`.
mod demo;
#[cfg(all(test, not(target_arch = "wasm32")))]
mod headless;
mod motion;
mod script;

use super::march::Walks;
use super::presentation::{tinted, unit_size};
use super::puppet::{ClipSet, Puppetry};
use super::{Session, WorldRect, camera_rect, option};
use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    window::PrimaryWindow,
};
use fourx_content::animation::facing;
use fourx_sim::{Battle, Id, terrain::Coord};
use motion::Pose;
use script::{Event as Moment, LEAD_IN, Script, Setting, Showing};
use std::collections::VecDeque;
use std::f32::consts::PI;

const PAPER: Color = Color::srgb(0.88, 0.83, 0.69);
const INK: Color = Color::srgb(0.09, 0.15, 0.16);
const RED: Color = Color::srgb(0.75, 0.24, 0.20);
/// Sprites on stage stand above the map's own units (drawn at about 30).
const STAGE_Z: f32 = 40.0;
const FX_Z: f32 = 60.0;
/// How far a unit's sprite is drawn above its square's anchor, as on the map.
const LIFT: f32 = 12.0;
const BAR_WIDTH: f32 = 46.0;
/// The most a frame may advance the clock, so a stalled frame does not skip a fight.
const LONGEST_FRAME: f32 = 0.1;
/// A fight is played when it is in the camera's view grown by this much (about a tile).
const VIEW_MARGIN: f32 = 128.0;

const SMOKE: Srgba = Srgba::new(0.82, 0.82, 0.80, 1.0);
const FLASH: Srgba = Srgba::new(1.0, 0.88, 0.50, 1.0);
const BURST: Srgba = Srgba::new(1.0, 0.94, 0.74, 1.0);
const SPARK: Srgba = Srgba::new(1.0, 0.62, 0.20, 1.0);
const DUST: Srgba = Srgba::new(0.76, 0.66, 0.48, 1.0);
const SPLASH: Srgba = Srgba::new(0.86, 0.95, 1.0, 1.0);
const SHELL: Srgba = Srgba::new(0.10, 0.09, 0.08, 1.0);
const GLINT: Srgba = Srgba::new(1.0, 0.85, 0.35, 1.0);

/// Marks everything the stage has put on screen, so it can all be taken down at once.
#[derive(Component)]
pub(super) struct StageVisual;
/// A unit's sprite or its bar.
#[derive(Component)]
pub(super) struct Part;
/// The pieces `drive` poses each frame: apart from the camera, which it only reads.
type Sprites<'w, 's> =
    Query<'w, 's, (&'static mut Transform, &'static mut Sprite), (With<Part>, Without<Camera2d>)>;
#[derive(Component)]
pub(super) struct SkipButton;

/// The queue of battles and the one being played.
#[derive(Resource)]
pub(super) struct Stage {
    queue: VecDeque<Battle>,
    playing: Option<Playing>,
    /// Scales the clock (`--combat-speed`).
    speed: f32,
    /// Script seconds that passed this frame, for the effects.
    step: f32,
    /// Squares whose real units are hidden while a battle plays on them.
    hidden: Vec<Coord>,
    /// `--demo-combat`: battles to play once the camera has settled, and a moment to freeze on.
    demo: Option<String>,
    freeze: Option<f32>,
    /// Whether the skip button is on screen.
    button: bool,
}

struct Playing {
    script: Script,
    clock: f32,
    next: usize,
    parts: Vec<Parts>,
    looks: Vec<Look>,
    /// Each actor's clips, when its design has them. Holding them keeps the fighting sheets
    /// loaded for as long as the fight plays.
    arts: Vec<Option<ClipSet>>,
}

struct Parts {
    sprite: Entity,
    frame: Entity,
    fill: Entity,
}

/// What colour an actor is at the start, and what it turns to if it yields.
struct Look {
    start: Color,
    captor: Color,
}

impl Stage {
    pub(super) fn new() -> Self {
        let speed = option("--combat-speed")
            .or_else(|| {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    std::env::var("FOURX_COMBAT_SPEED").ok()
                }
                #[cfg(target_arch = "wasm32")]
                {
                    None
                }
            })
            .and_then(|s| s.parse::<f32>().ok())
            .filter(|s| s.is_finite() && *s > 0.0)
            .unwrap_or(1.0);
        let (demo, freeze) = match option("--demo-combat") {
            Some(argument) => {
                let (name, freeze) = demo::parse(&argument);
                (Some(name.to_string()), freeze)
            }
            None => (None, None),
        };
        Self {
            queue: VecDeque::new(),
            playing: None,
            speed,
            step: 0.0,
            hidden: Vec::new(),
            demo,
            freeze,
            button: false,
        }
    }

    /// Whether a battle is playing or waiting, which holds back the player's orders.
    pub(super) fn busy(&self) -> bool {
        self.playing.is_some() || !self.queue.is_empty()
    }

    /// Whether the real units on a square are out of sight because it is being played on.
    pub(super) fn hides(&self, at: Coord) -> bool {
        self.hidden.contains(&at)
    }

    /// Takes the battles a new snapshot carries.
    pub(super) fn enqueue(&mut self, battles: &[Battle]) {
        self.queue.extend(battles.iter().cloned());
    }
}

/// The soft shapes the effects are drawn with.
#[derive(Resource)]
pub(super) struct FxArt {
    soft: Handle<Image>,
    star: Handle<Image>,
}

pub(super) fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    commands.insert_resource(FxArt {
        // A round puff, solid in the middle and clear at the edge.
        soft: images.add(shape(48, |x, y| {
            let edge = (1.0 - (x * x + y * y).sqrt()).clamp(0.0, 1.0);
            edge * edge * (3.0 - 2.0 * edge)
        })),
        // A four-pointed star.
        star: images.add(shape(48, |x, y| {
            (1.0 - (x.abs().sqrt() + y.abs().sqrt())).max(0.0).powf(0.8)
        })),
    });
}

/// A white image whose opacity is `opacity(x, y)` for `x` and `y` in `-1..1`.
fn shape(size: u32, opacity: impl Fn(f32, f32) -> f32) -> Image {
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    for row in 0..size {
        for column in 0..size {
            let x = (column as f32 + 0.5) / size as f32 * 2.0 - 1.0;
            let y = (row as f32 + 0.5) / size as f32 * 2.0 - 1.0;
            let alpha = opacity(x, y).clamp(0.0, 1.0);
            data.extend_from_slice(&[255, 255, 255, (alpha * 255.0) as u8]);
        }
    }
    Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

/// Runs the stage for one frame: takes a skip, starts the next battle when idle, advances the
/// clock, fires what is due, and poses the actors. It runs after the player's input (so a
/// Space that skips is not also an order) and before `refresh` (so the real units are hidden in
/// the very frame a battle begins).
#[allow(clippy::too_many_arguments)]
pub(super) fn drive(
    mut commands: Commands,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    skip_buttons: Query<&Interaction, With<SkipButton>>,
    mut session: ResMut<Session>,
    mut stage: ResMut<Stage>,
    art: Res<FxArt>,
    assets: Res<AssetServer>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Transform, &Projection), With<Camera2d>>,
    mut parts: Sprites,
    visuals: Query<Entity, (With<StageVisual>, Without<SkipButton>)>,
    buttons: Query<Entity, With<SkipButton>>,
    mut puppetry: Puppetry,
    walks: Res<Walks>,
) {
    stage.step = 0.0;
    if session.game.is_none() {
        session.staging = false;
        return;
    }
    // Start the demo once the camera has settled on the player's home.
    if session.centered
        && stage.demo.is_some()
        && let Ok((camera, _)) = cameras.single()
    {
        let name = stage.demo.take().unwrap_or_default();
        let at = Coord::from_screen(camera.translation.x, camera.translation.y);
        let game = session.game.as_ref().unwrap();
        let attacker = if session.player == 0 {
            game.commander
        } else {
            session.player
        };
        let other = game
            .factions
            .keys()
            .copied()
            .find(|id| *id != attacker)
            .unwrap_or(attacker);
        stage.enqueue(&demo::battles(&name, at, (attacker, other)));
    }
    let wants_skip = keys.any_just_pressed([KeyCode::Space, KeyCode::Enter, KeyCode::NumpadEnter])
        || skip_buttons.iter().any(|i| *i == Interaction::Pressed);
    if stage.busy() && wants_skip {
        stage.queue.clear();
        stage.playing = None;
        take_down(&mut commands, &mut stage, &mut session, &visuals, &buttons);
    }
    // Fights wait for the walks that brought the fighters together.
    if stage.playing.is_none() && !walks.busy() {
        start_next(
            &mut commands,
            &mut stage,
            &mut session,
            &assets,
            &mut puppetry,
            &windows,
            &cameras,
        );
    }
    if let Some(mut playing) = stage.playing.take() {
        let pace = script::pace(stage.queue.len(), stage.speed);
        let mut step = time.delta_secs().min(LONGEST_FRAME) * pace;
        if let Some(at) = stage.freeze {
            // Hold the clock at the moment a demo was asked to freeze on.
            step = step.min((at - playing.clock).max(0.0));
        }
        playing.clock = (playing.clock + step).min(playing.script.length);
        while let Some(due) = playing.script.events.get(playing.next).copied() {
            if due.at > playing.clock {
                break;
            }
            playing.next += 1;
            fire(
                &mut commands,
                &art,
                &session,
                &assets,
                &playing.script,
                due.at,
                due.event,
            );
        }
        paint(&playing, &mut parts, &puppetry.images);
        stage.step = step;
        if playing.clock < playing.script.length {
            stage.playing = Some(playing);
        } else if stage.queue.is_empty() {
            take_down(&mut commands, &mut stage, &mut session, &visuals, &buttons);
        } else {
            // On to the next, with this one's actors and effects gone.
            for entity in &visuals {
                commands.entity(entity).despawn();
            }
            start_next(
                &mut commands,
                &mut stage,
                &mut session,
                &assets,
                &mut puppetry,
                &windows,
                &cameras,
            );
            if stage.playing.is_none() {
                take_down(&mut commands, &mut stage, &mut session, &visuals, &buttons);
            }
        }
    }
    session.staging = stage.busy();
}

/// Ends all playing: the stage's entities go, the real units show, the map is rebuilt.
fn take_down(
    commands: &mut Commands,
    stage: &mut Stage,
    session: &mut Session,
    visuals: &Query<Entity, (With<StageVisual>, Without<SkipButton>)>,
    buttons: &Query<Entity, With<SkipButton>>,
) {
    for entity in visuals.iter().chain(buttons.iter()) {
        commands.entity(entity).despawn();
    }
    stage.button = false;
    if !stage.hidden.is_empty() {
        stage.hidden.clear();
        session.built = None;
    }
}

/// Whether any square a fight touches is in the camera's view, give or take a tile.
fn in_view(battle: &Battle, view: WorldRect) -> bool {
    let view = view.grow(VIEW_MARGIN);
    battle.tiles().iter().any(|tile| {
        let (x, y) = tile.screen();
        view.has(Vec2::new(x, y))
    })
}

/// Begins the next battle in the queue that the player can see. The others are dropped.
fn start_next(
    commands: &mut Commands,
    stage: &mut Stage,
    session: &mut Session,
    assets: &AssetServer,
    puppetry: &mut Puppetry,
    windows: &Query<&Window, With<PrimaryWindow>>,
    cameras: &Query<(&Transform, &Projection), With<Camera2d>>,
) {
    let (Ok(window), Ok((camera, projection))) = (windows.single(), cameras.single()) else {
        return;
    };
    let view = camera_rect(camera, projection, window);
    while let Some(battle) = stage.queue.pop_front() {
        if !in_view(&battle, view) {
            continue;
        }
        let game = session.game.as_ref().unwrap();
        let city = |at: Coord| game.city_at(at).is_some();
        let animated = |kind: &str| session.pack.visuals.units.contains_key(kind);
        let script = Script::build(
            &battle,
            &Setting {
                rules: &session.rules,
                combat: &session.pack.visuals.combat,
                city: &city,
                animated: &animated,
            },
        );
        let (parts, looks, arts) = spawn_actors(commands, session, assets, puppetry, &script);
        // Whoever survives keeps looking the way the fight left it.
        for (i, actor) in script.actors.iter().enumerate() {
            let heading = script.heading_at(i, script.length);
            puppetry
                .facings
                .turn(actor.fighter.id, facing(heading.x, heading.y));
        }
        if !stage.button {
            spawn_skip_button(commands);
            stage.button = true;
        }
        stage.hidden = battle.tiles();
        session.built = None;
        stage.playing = Some(Playing {
            script,
            clock: 0.0,
            next: 0,
            parts,
            looks,
            arts,
        });
        return;
    }
    // Nothing left that the player can see.
    if stage.playing.is_none() && !stage.hidden.is_empty() {
        stage.hidden.clear();
        session.built = None;
    }
}

/// The button that skips fights, for players without a keyboard.
fn spawn_skip_button(commands: &mut Commands) {
    commands
        .spawn((
            Button,
            Node {
                position_type: PositionType::Absolute,
                top: px(84),
                left: percent(50),
                margin: UiRect::left(px(-110)),
                width: px(220),
                height: px(32),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(6)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(crate::presentation::NAVY),
            BorderColor::all(crate::presentation::GOLD_LINE),
            SkipButton,
            StageVisual,
        ))
        .with_children(|p| {
            p.spawn((
                Text::new("Skip fight  (Space)"),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(PAPER),
                TextLayout::new_with_no_wrap(),
            ));
        });
}

/// How a unit is coloured on the map: its nation's tint, or plain when it is the player's own.
fn unit_color(session: &Session, owner: Id, spent: bool) -> Color {
    let Some(game) = &session.game else {
        return Color::WHITE;
    };
    if owner != session.player {
        tinted(game, owner)
    } else if spent {
        Color::srgb(0.66, 0.66, 0.70)
    } else {
        Color::WHITE
    }
}

fn spawn_actors(
    commands: &mut Commands,
    session: &Session,
    assets: &AssetServer,
    puppetry: &mut Puppetry,
    script: &Script,
) -> (Vec<Parts>, Vec<Look>, Vec<Option<ClipSet>>) {
    let game = session.game.as_ref().unwrap();
    let mut parts = Vec::new();
    let mut looks = Vec::new();
    let mut arts = Vec::new();
    for (i, actor) in script.actors.iter().enumerate() {
        let fighter = &actor.fighter;
        let size = match session.rules.units.get(&fighter.kind) {
            Some(def) => unit_size(def),
            None if actor.sea => Vec2::new(112.0, 84.0),
            None => Vec2::splat(70.0),
        };
        // A unit that is still there looks as the map will draw it when the fight is over, so
        // the hand-over is seamless.
        let now = game.units.get(&fighter.id).map(|u| {
            let spent = u.owner == session.player
                && session
                    .rules
                    .units
                    .get(&u.kind)
                    .is_some_and(|def| u.moves_left(def) == 0);
            unit_color(session, u.owner, spent)
        });
        let recorded = unit_color(session, fighter.owner, false);
        let start = if actor.captor.is_some() {
            recorded
        } else {
            now.unwrap_or(recorded)
        };
        let captor = actor
            .captor
            .map(|id| now.unwrap_or_else(|| unit_color(session, id, false)))
            .unwrap_or(start);
        looks.push(Look { start, captor });
        let art = actor
            .animated
            .then(|| {
                puppetry
                    .art
                    .set(session, assets, &mut puppetry.layouts, &fighter.kind)
            })
            .flatten();
        let mut sprite = match &art {
            Some(art) => {
                let toward = actor.toward;
                art.get(fourx_content::animation::Clip::Idle).sprite(
                    facing(toward.x, toward.y),
                    0,
                    size,
                    start.with_alpha(0.0),
                )
            }
            None => Sprite {
                custom_size: Some(size),
                color: start.with_alpha(0.0),
                flip_x: actor.flip,
                ..default()
            },
        };
        if art.is_none()
            && let Some(def) = session.rules.units.get(&fighter.kind)
        {
            sprite.image = assets.load(format!("{}{}", session.asset_prefix, def.sprite));
        }
        arts.push(art);
        let z = STAGE_Z - actor.anchor.y / 1000.0 + i as f32 * 0.01;
        let place = |dz: f32| Transform::from_xyz(actor.anchor.x, actor.anchor.y + LIFT, z + dz);
        let sprite = commands.spawn((sprite, place(0.0), Part, StageVisual)).id();
        let frame = commands
            .spawn((
                Sprite::from_color(INK.with_alpha(0.0), Vec2::new(BAR_WIDTH + 4.0, 9.0)),
                place(0.1),
                Part,
                StageVisual,
            ))
            .id();
        let fill = commands
            .spawn((
                Sprite::from_color(Color::NONE, Vec2::new(BAR_WIDTH, 5.0)),
                place(0.2),
                Part,
                StageVisual,
            ))
            .id();
        parts.push(Parts {
            sprite,
            frame,
            fill,
        });
    }
    (parts, looks, arts)
}

fn mix(from: f32, to: f32, by: f32) -> f32 {
    from + (to - from) * by
}

/// An actor's colour: its own, turned toward its captor's, washed toward white by a flash, and
/// faded by the pose and by the lead-in.
fn shade(look: &Look, pose: &Pose, fade: f32) -> Color {
    let (a, b) = (look.start.to_srgba(), look.captor.to_srgba());
    let wash = |own: f32, other: f32| {
        let c = mix(own, other, pose.tint);
        mix(c, 1.0, 0.5 * pose.flash) * (1.0 + 1.2 * pose.flash)
    };
    Color::srgba(
        wash(a.red, b.red),
        wash(a.green, b.green),
        wash(a.blue, b.blue),
        a.alpha * pose.alpha * fade,
    )
}

fn bar_color(fraction: f32) -> Color {
    if fraction > 0.66 {
        Color::srgb(0.30, 0.72, 0.32)
    } else if fraction > 0.33 {
        Color::srgb(0.88, 0.70, 0.22)
    } else {
        RED
    }
}

/// Poses every actor, and its hit-point bar, for the clock's moment.
fn paint(playing: &Playing, sprites: &mut Sprites, images: &Assets<Image>) {
    let (script, t) = (&playing.script, playing.clock);
    let fade = (t / LEAD_IN).clamp(0.0, 1.0);
    for (i, actor) in script.actors.iter().enumerate() {
        let pose = script.pose_at(i, t);
        let parts = &playing.parts[i];
        let z = STAGE_Z - actor.anchor.y / 1000.0 + i as f32 * 0.01;
        if let Ok((mut transform, mut sprite)) = sprites.get_mut(parts.sprite) {
            if let Some(art) = &playing.arts[i] {
                show_clip(art, script, i, t, &mut sprite, images);
            }
            let at = actor.anchor + pose.offset + Vec2::Y * LIFT;
            *transform = Transform {
                translation: at.extend(z),
                rotation: Quat::from_rotation_z(pose.tilt),
                scale: Vec3::splat(pose.scale),
            };
            sprite.color = shade(&playing.looks[i], &pose, fade);
        }
        let visible = pose.alpha * fade;
        let hp = script.hp_at(i, t);
        let fraction = (hp as f32 / actor.fighter.max_hp.max(1) as f32).clamp(0.0, 1.0);
        // A unit that crosses to another square takes its bar along; a lunge or a fall does not.
        let base = Vec2::new(actor.anchor.x, actor.anchor.y - 22.0) + pose.travel;
        if let Ok((mut transform, mut sprite)) = sprites.get_mut(parts.frame) {
            transform.translation = base.extend(z + 0.1);
            sprite.color = INK.with_alpha(visible);
        }
        if let Ok((mut transform, mut sprite)) = sprites.get_mut(parts.fill) {
            // The fill shrinks toward its left edge.
            let width = BAR_WIDTH * fraction;
            transform.translation = Vec3::new(base.x - (BAR_WIDTH - width) * 0.5, base.y, z + 0.2);
            sprite.custom_size = Some(Vec2::new(width.max(0.01), 5.0));
            sprite.color = bar_color(fraction).with_alpha(visible);
        }
    }
}

/// Puts the frame of the clip an actor is playing on its sprite, turned the way it looks. A
/// sheet still on its way leaves the actor standing.
fn show_clip(
    art: &ClipSet,
    script: &Script,
    actor: usize,
    t: f32,
    sprite: &mut Sprite,
    images: &Assets<Image>,
) {
    let heading = script.heading_at(actor, t);
    let face = facing(heading.x, heading.y);
    let (clip, frame) = match script.showing(actor, t) {
        Showing::Once { clip, progress } => (art.get(clip), art.get(clip).once(progress)),
        Showing::Looping { clip, seconds } => (art.get(clip), art.get(clip).looping(seconds)),
    };
    if images.contains(&clip.image) {
        clip.show(sprite, face, frame);
    } else {
        let idle = art.get(fourx_content::animation::Clip::Idle);
        if images.contains(&idle.image) {
            idle.show(sprite, face, idle.looping(t));
        }
    }
}

#[derive(Clone, Copy)]
enum Landing {
    Hit,
    /// Falls short: a splash on water, a puff of dust on land.
    Short {
        sea: bool,
    },
}

#[derive(Clone, Copy)]
enum FxKind {
    /// A soft cloud that swells from `from` to `to` across, drifts, and fades from `peak`.
    Puff {
        from: f32,
        to: f32,
        rise: Vec2,
        peak: f32,
    },
    /// A shell in flight.
    Shell { to: Vec2, landing: Landing },
    /// A star that drifts upward and spins away.
    Star { rise: Vec2 },
}

#[derive(Component)]
pub(super) struct Fx {
    age: f32,
    life: f32,
    from: Vec3,
    color: Srgba,
    kind: FxKind,
}

fn spawn_fx(
    commands: &mut Commands,
    art: &FxArt,
    from: Vec3,
    color: Srgba,
    life: f32,
    kind: FxKind,
) {
    let (image, size) = match kind {
        FxKind::Puff { from: size, .. } => (art.soft.clone(), Vec2::splat(size)),
        FxKind::Star { .. } => (art.star.clone(), Vec2::splat(18.0)),
        FxKind::Shell { .. } => (art.soft.clone(), Vec2::splat(9.0)),
    };
    commands.spawn((
        Sprite {
            image,
            custom_size: Some(size),
            color: Color::from(color).with_alpha(0.0),
            ..default()
        },
        Transform::from_translation(from),
        Fx {
            age: 0.0,
            life: life.max(0.01),
            from,
            color,
            kind,
        },
        StageVisual,
    ));
}

/// Does what an event of the script calls for at the moment `at`.
fn fire(
    commands: &mut Commands,
    art: &FxArt,
    session: &Session,
    assets: &AssetServer,
    script: &Script,
    at: f32,
    event: Moment,
) {
    let centre =
        |i: usize, t: f32| script.actors[i].anchor + script.pose_at(i, t).offset + Vec2::Y * LIFT;
    let muzzle =
        |i: usize, reach: f32| centre(i, at) + script.actors[i].toward * reach + Vec2::Y * 6.0;
    match event {
        Moment::Sound(cue) => {
            if let Some(path) = session.pack.visuals.combat.sounds.get(cue) {
                commands.spawn((
                    AudioPlayer::new(assets.load(format!("{}{}", session.asset_prefix, path))),
                    PlaybackSettings::DESPAWN,
                ));
            }
        }
        Moment::Flash { actor, big } => {
            let at_muzzle = muzzle(actor, if big { 38.0 } else { 26.0 }).extend(FX_Z);
            let (to, life) = if big { (64.0, 0.14) } else { (38.0, 0.10) };
            let flash = FxKind::Puff {
                from: 12.0,
                to,
                rise: Vec2::ZERO,
                peak: 1.0,
            };
            spawn_fx(commands, art, at_muzzle, FLASH, life, flash);
            let puffs = if big { 3 } else { 1 };
            for n in 0..puffs {
                let sideways = (n as f32 - 1.0) * 8.0 + script.actors[actor].toward.x * 10.0;
                let smoke = FxKind::Puff {
                    from: 14.0,
                    to: 38.0 + 8.0 * n as f32,
                    rise: Vec2::new(sideways, 16.0 + 6.0 * n as f32),
                    peak: 0.55,
                };
                let at_smoke = at_muzzle - Vec3::Z * 0.1;
                spawn_fx(commands, art, at_smoke, SMOKE, 0.7 + 0.1 * n as f32, smoke);
            }
        }
        Moment::Shell {
            from,
            to,
            flight,
            hits,
        } => {
            let start = muzzle(from, 30.0);
            let aim = centre(to, at + flight);
            let (end, landing) = if hits {
                (aim, Landing::Hit)
            } else {
                // Short of the mark, a little off to one side (the same for the same record).
                let side = ((at * 977.0).fract() - 0.5) * 24.0;
                (
                    start.lerp(aim, 0.55) + Vec2::new(0.0, side),
                    Landing::Short {
                        sea: script.actors[to].sea,
                    },
                )
            };
            let shell = FxKind::Shell { to: end, landing };
            spawn_fx(
                commands,
                art,
                start.extend(FX_Z + 0.1),
                SHELL,
                flight,
                shell,
            );
        }
        Moment::Dust { actor } => {
            let feet = script.actors[actor].anchor + Vec2::Y * 4.0;
            for (dx, delay) in [(-12.0, 0.0), (8.0, 0.05), (22.0, 0.1)] {
                let cloud = FxKind::Puff {
                    from: 16.0,
                    to: 56.0,
                    rise: Vec2::new(dx * 0.5, 12.0),
                    peak: 0.7,
                };
                let at_feet = Vec3::new(feet.x + dx, feet.y, FX_Z);
                spawn_fx(commands, art, at_feet, DUST, 0.5 + delay, cloud);
            }
        }
        Moment::Blow { target } => {
            let at_target = centre(target, at).extend(FX_Z + 0.2);
            let burst = FxKind::Puff {
                from: 20.0,
                to: 66.0,
                rise: Vec2::ZERO,
                peak: 1.0,
            };
            spawn_fx(commands, art, at_target, BURST, 0.22, burst);
            let sparks = FxKind::Puff {
                from: 12.0,
                to: 44.0,
                rise: Vec2::new(0.0, 10.0),
                peak: 0.9,
            };
            spawn_fx(
                commands,
                art,
                at_target - Vec3::Z * 0.1,
                SPARK,
                0.32,
                sparks,
            );
        }
        Moment::Sparkle { actor } => {
            let around = centre(actor, at);
            for (dx, dy) in [
                (-18.0, 20.0),
                (16.0, 26.0),
                (-4.0, 38.0),
                (24.0, 8.0),
                (-26.0, 4.0),
            ] {
                let star = FxKind::Star {
                    rise: Vec2::new(0.0, 22.0),
                };
                let at_star = Vec3::new(around.x + dx, around.y + dy, FX_Z + 0.3);
                spawn_fx(commands, art, at_star, GLINT, 0.9, star);
            }
        }
    }
}

/// Advances every effect on the stage: puffs swell and fade, shells fly and land, stars rise.
pub(super) fn fx(
    mut commands: Commands,
    stage: Res<Stage>,
    art: Res<FxArt>,
    mut effects: Query<(Entity, &mut Fx, &mut Transform, &mut Sprite)>,
) {
    for (entity, mut fx, mut transform, mut sprite) in &mut effects {
        fx.age += stage.step;
        let u = (fx.age / fx.life).clamp(0.0, 1.0);
        match fx.kind {
            FxKind::Puff {
                from,
                to,
                rise,
                peak,
            } => {
                let swell = 1.0 - (1.0 - u) * (1.0 - u);
                sprite.custom_size = Some(Vec2::splat(mix(from, to, swell)));
                transform.translation = fx.from + rise.extend(0.0) * u;
                sprite.color = Color::from(fx.color).with_alpha(peak * (1.0 - u).powf(1.5));
            }
            FxKind::Shell { to, landing } => {
                let start = fx.from.truncate();
                // A shallow arc over the straight line.
                let here = start.lerp(to, u) + Vec2::Y * 18.0 * (PI * u).sin();
                transform.translation = here.extend(fx.from.z);
                sprite.color = Color::from(fx.color);
                if fx.age >= fx.life
                    && let Landing::Short { sea } = landing
                {
                    let (color, from, to_size, rise, life) = if sea {
                        (SPLASH, 10.0, 36.0, Vec2::new(0.0, 26.0), 0.45)
                    } else {
                        (DUST, 14.0, 46.0, Vec2::new(0.0, 10.0), 0.5)
                    };
                    let cloud = FxKind::Puff {
                        from,
                        to: to_size,
                        rise,
                        peak: 0.8,
                    };
                    let at = to.extend(FX_Z);
                    spawn_fx(&mut commands, &art, at, color, life, cloud);
                }
            }
            FxKind::Star { rise } => {
                transform.translation = fx.from + rise.extend(0.0) * u;
                transform.rotation = Quat::from_rotation_z(u * 2.0);
                sprite.custom_size = Some(Vec2::splat(18.0 * (1.0 - 0.5 * u)));
                sprite.color = Color::from(fx.color).with_alpha(1.0 - u * u);
            }
        }
        if fx.age >= fx.life {
            commands.entity(entity).despawn();
        }
    }
}
