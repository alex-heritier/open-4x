//! Runs the stage's real systems in a headless Bevy app with a manual clock.
//!
//! The pure parts (motion, script) are tested on their own. These tests exist for what only a
//! running app shows: systems whose queries conflict panic when the app first updates, and a
//! stage that leaks entities or never lets go of the real units would pass every unit test.
use super::*;
use crate::initial_selection;
use bevy::{audio::AudioSource, time::TimeUpdateStrategy};
use fourx_runtime::Host;
use fourx_sim::{Fighter, Outcome};
use std::time::Duration;

/// Where the test camera looks, and so where `--demo-combat` fights are staged.
const CAMERA: Vec2 = Vec2::new(300.0, -200.0);

fn session() -> Session {
    let host = Host::base_scenario("dawn-straits", 42).expect("the starter scenario starts");
    Session {
        player: host.commander(),
        game: Some(host.game),
        rules: host.rules,
        pack: host.pack,
        selection: initial_selection(),
        sequence: 0,
        pending: false,
        dirty: false,
        built: Some(WorldRect {
            min: Vec2::splat(-1.0e4),
            max: Vec2::splat(1.0e4),
        }),
        centered: true,
        message: String::new(),
        asset_prefix: "packs/base/".into(),
        frames: 0,
        screenshot: None,
        smoke: false,
        previous_pinch: None,
        staging: false,
    }
}

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<Image>()
        .init_asset::<AudioSource>()
        .init_resource::<ButtonInput<KeyCode>>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            16,
        )))
        .init_asset::<TextureAtlasLayout>()
        .insert_resource(session())
        .insert_resource(Stage::new())
        .init_resource::<Walks>()
        .init_resource::<crate::puppet::UnitArt>()
        .init_resource::<crate::puppet::Facings>()
        .add_systems(Startup, setup)
        .add_systems(Update, (crate::march::walk, drive, fx).chain());
    app.world_mut().spawn((Window::default(), PrimaryWindow));
    app.world_mut().spawn((
        Camera2d,
        Transform::from_translation(CAMERA.extend(0.0)),
        Projection::Orthographic(OrthographicProjection::default_2d()),
    ));
    app
}

fn count<T: Component>(app: &mut App) -> usize {
    app.world_mut().query::<&T>().iter(app.world()).count()
}

fn stage(app: &App) -> &Stage {
    app.world().resource::<Stage>()
}

fn walks_of(app: &App) -> &Walks {
    app.world().resource::<Walks>()
}

fn session_of(app: &App) -> &Session {
    app.world().resource::<Session>()
}

/// The two squares `--demo-combat` fights on, next to the camera.
fn squares() -> [Coord; 2] {
    let at = Coord::from_screen(CAMERA.x, CAMERA.y);
    [Coord::new(at.x, at.y + 1), Coord::new(at.x + 1, at.y)]
}

fn fighter(id: Id, at: Coord) -> Fighter {
    Fighter {
        id,
        owner: 1,
        kind: "infantry".into(),
        position: at,
        hp: 3,
        max_hp: 3,
    }
}

/// A duel the attacker wins in three rounds, on `at` and the square beside it.
fn duel(at: Coord) -> Battle {
    Battle::Duel {
        attacker: fighter(7001, at),
        defender: fighter(7002, Coord::new(at.x + 1, at.y)),
        support: None,
        rounds: vec![true, true, true],
        outcome: Outcome::AttackerWon,
        retreat_to: None,
        promoted: false,
    }
}

#[test]
fn every_demo_battle_plays_to_the_end_and_cleans_up() {
    let mut app = app();
    app.world_mut().resource_mut::<Stage>().demo = Some("all".into());
    app.update();
    let [left, right] = squares();
    let (mut frames, mut hid, mut peak_visuals, mut peak_effects) = (0, false, 0, 0);
    while frames == 0 || stage(&app).busy() {
        app.update();
        frames += 1;
        assert!(frames < 20_000, "the stage never finished");
        assert_eq!(
            session_of(&app).staging,
            stage(&app).busy(),
            "frame {frames}"
        );
        hid |= stage(&app).hides(left) && stage(&app).hides(right);
        peak_visuals = peak_visuals.max(count::<StageVisual>(&mut app));
        peak_effects = peak_effects.max(count::<Fx>(&mut app));
    }
    assert!(hid, "the real units were never hidden");
    assert!(
        peak_visuals >= 6,
        "actors, bars and a button: {peak_visuals}"
    );
    assert!(peak_effects > 0, "no effect was ever drawn");
    // Everything comes down, and the map is told to give the real units back.
    app.update();
    assert_eq!(count::<StageVisual>(&mut app), 0);
    assert_eq!(count::<Fx>(&mut app), 0);
    assert!(!stage(&app).hides(left) && !stage(&app).hides(right));
    assert!(!session_of(&app).staging);
    assert!(session_of(&app).built.is_none(), "the map must be rebuilt");
}

#[test]
fn space_skips_everything_at_once() {
    let mut app = app();
    app.world_mut().resource_mut::<Stage>().demo = Some("all".into());
    for _ in 0..120 {
        app.update();
    }
    assert!(stage(&app).busy(), "eight fights outlast two seconds");
    assert!(session_of(&app).staging);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Space);
    app.update();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .clear();
    app.update();
    let [left, right] = squares();
    assert!(!stage(&app).busy());
    assert!(!stage(&app).hides(left) && !stage(&app).hides(right));
    assert_eq!(count::<StageVisual>(&mut app), 0);
    assert!(!session_of(&app).staging);
}

#[test]
fn a_fight_out_of_view_is_dropped_and_one_in_view_is_played() {
    let mut app = app();
    app.update();
    let far = Coord::new(400, 400);
    app.world_mut()
        .resource_mut::<Stage>()
        .enqueue(&[duel(far)]);
    app.update();
    assert!(!stage(&app).busy(), "a fight nobody can see is dropped");
    assert_eq!(count::<StageVisual>(&mut app), 0);
    let here = squares()[0];
    app.world_mut()
        .resource_mut::<Stage>()
        .enqueue(&[duel(far), duel(here)]);
    app.update();
    assert!(
        stage(&app).busy(),
        "the visible one plays after the dropped one"
    );
    assert!(stage(&app).hides(here));
    // Two fighters with a sprite, a frame and a fill each, and the skip button with its text.
    assert!(count::<Part>(&mut app) >= 6);
    assert_eq!(count::<SkipButton>(&mut app), 1);
}

#[test]
fn the_skip_button_is_one_button_however_many_fights_wait() {
    let mut app = app();
    app.update();
    let here = squares()[0];
    app.world_mut()
        .resource_mut::<Stage>()
        .enqueue(&[duel(here), duel(here), duel(here)]);
    for _ in 0..600 {
        app.update();
        assert!(count::<SkipButton>(&mut app) <= 1);
    }
}

/// The whole path of a real fight: an order goes to the host, the snapshot that comes back
/// carries the record, the stage plays it over the real units, orders wait, and then they don't.
#[test]
fn a_real_attack_reaches_the_stage_and_holds_orders_back_until_it_is_over() {
    use crate::{BACKDROP, connection::Connection, issue, receive};
    use fourx_sim::Command;

    let mut host = Host::base_scenario("dawn-straits", 42).expect("the starter scenario starts");
    let me = host.commander();
    // Put one of the enemy's infantrymen next to the player's cavalry, so one order is a fight.
    let (rider, from) = {
        let u = (host.game.units.values())
            .find(|u| u.owner == me && u.kind == "cavalry")
            .expect("the commander has cavalry");
        (u.id, u.position)
    };
    let foe = (host.game.units.values())
        .find(|u| u.owner != me && u.kind == "infantry")
        .map(|u| u.id)
        .expect("the enemy has infantry");
    let target = (host.game.map.neighbors(from).into_iter())
        .find(|c| {
            host.game.city_at(*c).is_none() && host.game.map.get(*c).is_some_and(|t| t.is_land())
        })
        .expect("a free square of land beside the cavalry");
    let enemy = host.game.units[&foe].owner;
    host.game.units.get_mut(&foe).unwrap().position = target;
    host.game.reindex();
    assert!(host.game.at_war(me, enemy), "the two empires are at war");

    let mut start = session();
    start.game = None;
    start.player = me;
    start.rules = host.rules.clone();
    start.pack = host.pack.clone();
    start.centered = true;
    let mut app = app();
    app.insert_resource(start)
        .insert_resource(ClearColor(BACKDROP))
        .insert_non_send_resource(Connection::local(host));
    app.add_systems(Update, receive.before(drive));
    // The camera looks at the fight; one nobody can see is not played.
    let (x, y) = from.screen();
    for mut transform in app
        .world_mut()
        .query_filtered::<&mut Transform, With<Camera2d>>()
        .iter_mut(app.world_mut())
    {
        transform.translation = Vec3::new(x, y, 0.0);
    }

    // The first snapshot is where the game starts, not news.
    app.update();
    assert!(session_of(&app).game.is_some());
    assert!(!stage(&app).busy() && !session_of(&app).staging);

    // The player's order, issued as the client issues it.
    let order = Command::Attack {
        unit: rider,
        target,
    };
    app.world_mut()
        .resource_scope(|world, mut session: Mut<Session>| {
            let mut connection = world.non_send_resource_mut::<Connection>();
            issue(&mut session, &mut connection, order);
        });
    assert!(session_of(&app).pending, "the order went to the host");
    app.update();
    assert!(!session_of(&app).pending, "the host answered");
    assert!(stage(&app).busy(), "the fight is on stage");
    assert!(session_of(&app).staging);
    assert!(stage(&app).hides(from) && stage(&app).hides(target));
    assert!(
        count::<Part>(&mut app) >= 6,
        "a sprite, a frame and a fill each"
    );

    // Orders wait while it plays, and nothing reaches the host.
    let sequence = session_of(&app).sequence;
    app.world_mut()
        .resource_scope(|world, mut session: Mut<Session>| {
            let mut connection = world.non_send_resource_mut::<Connection>();
            issue(
                &mut session,
                &mut connection,
                Command::Fortify { unit: rider },
            );
        });
    assert_eq!(session_of(&app).sequence, sequence);
    assert!(!session_of(&app).pending);
    assert!(session_of(&app).message.starts_with("Orders wait"));

    let mut frames = 0;
    while stage(&app).busy() {
        app.update();
        frames += 1;
        assert!(frames < 5_000, "one fight never ends");
    }
    assert!(!session_of(&app).staging);
    assert!(!stage(&app).hides(from) && !stage(&app).hides(target));
    assert!(session_of(&app).built.is_none(), "the real units come back");

    // And now the same order goes through.
    app.world_mut()
        .resource_scope(|world, mut session: Mut<Session>| {
            let mut connection = world.non_send_resource_mut::<Connection>();
            issue(
                &mut session,
                &mut connection,
                Command::Fortify { unit: rider },
            );
        });
    assert_eq!(session_of(&app).sequence, sequence + 1);
}

/// A real order to march: the snapshot brings the walk, a walker stands in for the unit square
/// by square, and the unit comes back on its new square looking the way it went.
#[test]
fn a_real_march_is_walked_square_by_square() {
    use crate::{BACKDROP, connection::Connection, issue, march::WalkerSprite, receive};
    use fourx_sim::Command;

    let host = Host::base_scenario("dawn-straits", 42).expect("the starter scenario starts");
    let me = host.commander();
    let (unit, from) = {
        let u = (host.game.units.values())
            .find(|u| u.owner == me && u.kind == "infantry")
            .expect("the commander has infantry");
        (u.id, u.position)
    };
    let to = (host.game.map.neighbors(from).into_iter())
        .find(|c| {
            host.game.city_at(*c).is_none()
                && host.game.units_at(*c).is_empty()
                && host.game.map.get(*c).is_some_and(|t| t.is_land())
        })
        .expect("an empty square of land beside the infantry");
    let mut start = session();
    start.game = None;
    start.player = me;
    start.rules = host.rules.clone();
    start.pack = host.pack.clone();
    let mut app = app();
    app.insert_resource(start)
        .insert_resource(ClearColor(BACKDROP))
        .insert_non_send_resource(Connection::local(host));
    app.add_systems(Update, receive.before(crate::march::walk));
    let (x, y) = from.screen();
    for mut transform in app
        .world_mut()
        .query_filtered::<&mut Transform, With<Camera2d>>()
        .iter_mut(app.world_mut())
    {
        transform.translation = Vec3::new(x, y, 0.0);
    }
    app.update();

    app.world_mut()
        .resource_scope(|world, mut session: Mut<Session>| {
            let mut connection = world.non_send_resource_mut::<Connection>();
            issue(
                &mut session,
                &mut connection,
                Command::Move {
                    unit,
                    destination: to,
                },
            );
        });
    app.update();
    assert!(
        walks_of(&app).busy() && walks_of(&app).hides(unit),
        "the walk began"
    );
    assert_eq!(count::<WalkerSprite>(&mut app), 1);
    assert!(!session_of(&app).staging, "a walk never holds back orders");
    let mut frames = 0;
    while walks_of(&app).busy() {
        app.update();
        frames += 1;
        assert!(frames < 1_000, "the walk never ends");
    }
    // One step of 0.4 s at 16 ms a frame.
    assert!((20..40).contains(&frames), "{frames} frames");
    assert_eq!(count::<WalkerSprite>(&mut app), 0);
    assert!(!walks_of(&app).hides(unit));
    assert!(session_of(&app).built.is_none(), "the real unit comes back");
    let (x0, y0) = from.screen();
    let (x1, y1) = to.screen();
    let facing = fourx_content::animation::facing(x1 - x0, y1 - y0);
    assert_eq!(
        app.world().resource::<crate::puppet::Facings>().of(unit),
        facing
    );
}
