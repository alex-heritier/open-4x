//! The soundtrack: the pack's music, one track after another in a loop, as Civ3 plays its
//! `Music.txt`. It holds its place while the diplomacy window plays its own music over it.
use super::*;
use bevy::audio::AudioSinkPlayback;

/// Marks the soundtrack's current track.
#[derive(Component)]
pub(super) struct Track;

/// Start the next track when the last one has finished, and pause the soundtrack while the
/// diplomacy window, or the title screens' own music, plays.
pub(super) fn soundtrack(
    session: Res<Session>,
    menu: Res<menu::Menu>,
    screen: Res<menu::Screen>,
    assets: Res<AssetServer>,
    mut commands: Commands,
    mut next: Local<usize>,
    playing: Query<Option<&AudioSink>, With<Track>>,
) {
    let music = &session.pack.visuals.music;
    if music.is_empty() {
        return;
    }
    if menu::has_own_music(&menu, *screen) && playing.is_empty() {
        return;
    }
    match playing.single() {
        // Each track despawns when it ends, which is the cue for the next.
        Err(_) => {
            let path = &music[*next % music.len()];
            *next = (*next + 1) % music.len();
            commands.spawn((
                AudioPlayer::new(assets.load(format!("{}{path}", session.asset_prefix))),
                PlaybackSettings::DESPAWN,
                Track,
            ));
        }
        Ok(Some(sink)) => {
            let hushed = session.diplomacy.is_some() || menu::has_own_music(&menu, *screen);
            if hushed && !sink.is_paused() {
                sink.pause();
            } else if !hushed && sink.is_paused() {
                sink.play();
            }
        }
        Ok(None) => {}
    }
}
