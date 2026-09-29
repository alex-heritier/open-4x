//! Window screenshots for automated testing and debugging.
//!
//! Capture is in-engine (`Screenshot::primary_window`), so it copies what the
//! renderer drew, not what the desktop shows: the window may be occluded or in
//! the background. Keep the window uncovered anyway: winit stops updating a
//! hidden window, which stalls the frame counter and the capture timing.
//! Env vars, read once at startup:
//!
//! - `CIV3_SHOT=<path>`: schedule captures. `{}` in the path is replaced by
//!   the frame number.
//! - `CIV3_SHOT_FRAME=<frames>`: comma separated frame list, default `90`. With
//!   several frames and no `{}`, `-<frame>` is inserted before the extension.
//! - `CIV3_SHOT_KEEP=1`: stay open after the shots are written. Default is to
//!   exit 0 (1 if a capture never completed), so a test script can just wait
//!   for the process.
//!
//! `P` grabs the window on demand to `shot-<unix seconds>.png` in the working
//! directory. `CIV3_NO_SPLASH=1` (see `splash`) starts on the map; the greeting
//! otherwise eats the first click or key.
//!
//! Example: `CIV3_NO_SPLASH=1 CIV3_SHOT=/tmp/map.png CIV3_SHOT_FRAME=120 cargo run`

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use bevy::winit::WinitSettings;

/// Frames of grace after the last scheduled frame before an unfinished capture
/// is a failure (~4s at 60fps).
const STALL_FRAMES: u32 = 240;

/// Default capture frame: late enough for the map and its art to be ready.
const DEFAULT_FRAME: u32 = 90;

#[derive(Resource, Default)]
pub struct Shots {
    /// Scheduled captures, ascending by frame.
    schedule: Vec<(u32, PathBuf)>,
    next: usize,
    frame: u32,
    pending: Arc<AtomicUsize>,
    keep: bool,
}

fn shot_path(pattern: &str, frame: u32, total: usize) -> PathBuf {
    if pattern.contains("{}") {
        return PathBuf::from(pattern.replace("{}", &frame.to_string()));
    }
    if total == 1 {
        return PathBuf::from(pattern);
    }
    let path = PathBuf::from(pattern);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "shot".into());
    let name = match path.extension().map(|e| e.to_string_lossy().into_owned()) {
        Some(ext) => format!("{stem}-{frame}.{ext}"),
        None => format!("{stem}-{frame}"),
    };
    path.with_file_name(name)
}

pub fn setup_shots(mut commands: Commands, mut shots: ResMut<Shots>) {
    let Ok(pattern) = std::env::var("CIV3_SHOT") else {
        return;
    };
    // Capture runs are unattended. The default winit settings go reactive
    // while the window is unfocused, which stalls the frame counter and leaves
    // the exit message unread, so a run can hang after writing its shots.
    commands.insert_resource(WinitSettings::continuous());
    let mut frames: Vec<u32> = std::env::var("CIV3_SHOT_FRAME")
        .map(|s| {
            s.split(',')
                .filter_map(|f| f.trim().parse().ok())
                .collect()
        })
        .unwrap_or_default();
    if frames.is_empty() {
        frames.push(DEFAULT_FRAME);
    }
    let total = frames.len();
    let mut schedule: Vec<(u32, PathBuf)> = frames
        .iter()
        .map(|f| (*f, shot_path(&pattern, *f, total)))
        .collect();
    schedule.sort_by_key(|(frame, _)| *frame);
    for (frame, path) in &schedule {
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            if let Err(e) = std::fs::create_dir_all(dir) {
                eprintln!("shot: cannot create {}: {e}", dir.display());
            }
        }
        println!("shot: frame {frame} -> {}", path.display());
    }
    shots.pending.store(schedule.len(), Ordering::SeqCst);
    shots.keep = std::env::var("CIV3_SHOT_KEEP").is_ok();
    shots.schedule = schedule;
}

/// Fires the scheduled captures and ends the run once they have all landed.
pub fn drive_shots(mut commands: Commands, mut shots: ResMut<Shots>) {
    if shots.schedule.is_empty() {
        return;
    }
    shots.frame += 1;
    let frame = shots.frame;
    while let Some((at, path)) = shots.schedule.get(shots.next).cloned() {
        if frame < at {
            break;
        }
        shots.next += 1;
        let pending = shots.pending.clone();
        let keep = shots.keep;
        commands
            .spawn(Screenshot::primary_window())
            .observe(move |shot: On<ScreenshotCaptured>| {
                save_to_disk(&path)(shot);
                println!("shot: wrote {}", path.display());
                if !keep && pending.fetch_sub(1, Ordering::SeqCst) == 1 {
                    end_run(0);
                }
            });
    }
    if shots.keep || shots.next < shots.schedule.len() {
        return;
    }
    let last = shots.schedule.last().map(|(frame, _)| *frame).unwrap_or(0);
    if frame > last + STALL_FRAMES && shots.pending.load(Ordering::SeqCst) > 0 {
        eprintln!(
            "shot: capture still pending {} frames after frame {last}",
            frame - last
        );
        end_run(1);
    }
}

/// Leave immediately instead of asking the app to shut down: a window that is
/// occluded or hidden stops being updated, so an exit request would sit unread
/// and an unattended run would hang. Every shot is already on disk here.
fn end_run(code: i32) -> ! {
    let _ = std::io::Write::flush(&mut std::io::stdout());
    std::process::exit(code);
}

/// `P`: save the window as it looks right now.
pub fn manual_shot(mut commands: Commands, keys: Res<ButtonInput<KeyCode>>) {
    if !keys.just_pressed(KeyCode::KeyP) {
        return;
    }
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = PathBuf::from(format!("shot-{seconds}.png"));
    commands
        .spawn(Screenshot::primary_window())
        .observe(move |shot: On<ScreenshotCaptured>| {
            save_to_disk(&path)(shot);
            println!("shot: wrote {}", path.display());
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_frame_keeps_the_path() {
        assert_eq!(shot_path("/tmp/a.png", 90, 1), PathBuf::from("/tmp/a.png"));
    }

    #[test]
    fn placeholder_takes_every_frame() {
        assert_eq!(shot_path("/tmp/a-{}.png", 7, 3), PathBuf::from("/tmp/a-7.png"));
        assert_eq!(shot_path("/tmp/a-{}.png", 7, 1), PathBuf::from("/tmp/a-7.png"));
    }

    #[test]
    fn frame_list_gets_a_suffix_before_the_extension() {
        assert_eq!(shot_path("/tmp/a.png", 40, 2), PathBuf::from("/tmp/a-40.png"));
        assert_eq!(shot_path("/tmp/a", 40, 2), PathBuf::from("/tmp/a-40"));
    }
}
