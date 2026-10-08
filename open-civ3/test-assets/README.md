# Generated source assets

Written by `python3 tools/make_stub_assets.py`. Every pixel, frame and sample
is drawn or synthesised by `tools/openart/`; nothing is read from, traced from
or copied out of a Civ3 install (an install is consulted only for file formats
and sheet geometry). Dedicated to the public domain under CC0-1.0. The font
is the vendored Arimo (OFL, licence beside it).

What is here: ground sheets with blended shorelines, hill, mountain, forest,
river, road and irrigation overlays, resources, one folder of INI and
8-direction FLC clips per unit, cities with walls, the city, advisor,
diplomacy and wonder screens, a picture per advance and wonder, leader clips
(16 archetypes by era; civs share them through links), sound effects and music.

Run a scenario or save with no install:

    CIV3_DIR=/nonexistent cargo run --release -- FILE.biq --assets test-assets

Regenerate (add `--biq FILE.biq`, repeatable, to merge a scenario's names into
`tools/stub_asset_refs.json`; `--clean` removes files the run did not write):

    python3 tools/make_stub_assets.py --clean

Verify the result against the engine's contracts, and against a Civ3 install
to confirm nothing here is a copy; then boot both fixtures on it:

    python3 tools/check_stub_assets.py test-assets --civ3 ../civ3/civ3-gog/app
    tools/smoke_assets.sh test-assets

For hand-made art from real modders, `python3 tools/fetch_community_assets.py`
builds the git-ignored `test-assets-community/` from this tree plus a slice of
a community archive.

Conversion needs Pillow and ffmpeg. `assets/cache` is shared by every source
root: keep a copy of a valuable cache before running with another `--assets`.
