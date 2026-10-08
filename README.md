# open-4x

Top-level layout:

- `open-civ3/` — the Civ3 clone: the Bevy game (`src/`), the reverse-engineering
  findings and reference crates (`reverse-engineering/`), the file-format
  crates (`civ3_utils/`), and the asset/tooling scripts (`tools/`, `web/`).
- `open-4x/` — independent Rust/Bevy strategy game, with a portable simulation, Lua mod packs, and a headless server. See [its README](open-4x/README.md).
- `civ3/` — git-ignored Civ3 installs and reverse-engineering scratch
  (`civ3-gog/`, `civ3-complete/`, `re/`, the Wine runtime and prefix). Never
  committed, and present only on a machine that has the install.

See [`open-civ3/README.md`](open-civ3/README.md) to build and run the game.
