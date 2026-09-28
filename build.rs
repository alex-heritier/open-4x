//! Link `target/<profile>/assets` to the package assets dir so Bevy's
//! exe-relative asset loader finds converted art when running from cargo.

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let target = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| manifest.join("target"));
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());
    let link = target.join(profile).join("assets");
    if link.exists() {
        return;
    }
    if let Some(parent) = link.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    #[cfg(unix)]
    let _ = std::os::unix::fs::symlink(manifest.join("assets"), &link);
}
