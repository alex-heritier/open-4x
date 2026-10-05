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
    let want = manifest.join("assets");
    // An existing link is only trusted when it points at this checkout. A
    // checkout that was moved or re-cloned leaves a dangling or foreign link
    // behind (`exists()` follows it, so it reads as absent), and with every
    // converted PNG out of reach the game draws nothing but placeholders.
    match std::fs::read_link(&link) {
        Ok(current) if current == want => return,
        Ok(_) => {
            let _ = std::fs::remove_file(&link);
        }
        // Not a symlink: a real directory someone put there stays theirs.
        Err(_) if link.exists() => return,
        Err(_) => {}
    }
    if let Some(parent) = link.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    #[cfg(unix)]
    let _ = std::os::unix::fs::symlink(&want, &link);
}
