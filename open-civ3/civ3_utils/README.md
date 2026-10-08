# Civ3 Utilities

Standalone support crates used by `civ3-clone`:

- `biq/` reads and writes Civilization III `.biq`, `.bix`, `.bic`, and `.SAV`
  files, including the PKWARE DCL container.
- `terrain-builder/` compiles terrain materials into the transition sheets used
  by the game.

Each subdirectory remains its own Cargo workspace and can be built or tested
independently. The game reaches the parser through the `civ3-biq` path
dependency in the repository's root `Cargo.toml`; terrain-builder is an
offline tool and is not part of the game build.
