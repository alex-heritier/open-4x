# Starter asset provenance

All PNG/WAV files under `packs/base/` are generated from original Rust drawing/synthesis code in `tools/asset-forge/`. Regenerate with `cargo run -p asset-forge`. The generator does not read either reference image, the Civ3 installation, or any open-civ3 asset.

The two reference images in the workspace root are user-supplied inspiration and are not bundled into the game or web distribution. Terrain geometry follows the documented Civ3 tiling layout; pixel art, JSON definitions, and Lua scripts are independent.

Bevy supplies its default embedded font under its own upstream license. Game source and generated starter art use the workspace license declaration (MIT OR Apache-2.0). No proprietary Civ3 content is needed to build or run this project.
