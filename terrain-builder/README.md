# terrain-builder

Compiles a terrain pack, made of one ground material per terrain, into the
Civ3-format transition sheets that open-4x renders. You never author the 81
cells of each sheet by hand. They are treated as a compiled format:

```text
materials (AI via fal.ai) ─┐
transition masks ──────────┼─> compositor ─> sheets/xtgc.png xpgc.png xdgc.png xdpc.png
  (from Civ3, or noise)    ┘                        xdgp.png xggc.png wCSO.png wSSS.png wOOO.png
                                            grassland_0..2.png plains_0..2.png ... ocean_0..2.png
```

The output has the same files, sizes and addressing as
`tools/prep_assets.py` writes to `assets/gen/terrain/`: 1152x576 sheets of
9x9 cells of 128x64, with `col = 3*W + N` and `row = 3*S + E`
(`reverse-engineering/blending.md`, `src/blend.rs`). The game needs no
changes.

## Usage

Run from the repo root (`cargo build --release` in this directory first,
or use `cargo run --release --`):

```bash
B=terrain-builder/target/release/terrain-builder
$B init terrain-builder/packs/meiji --style "early-Meiji Japanese landscape painting, ink and soft watercolor"
export FAL_KEY=...                       # https://fal.ai/dashboard/keys
$B generate terrain-builder/packs/meiji  # 8 materials: 7 terrains + beach sand
$B build terrain-builder/packs/meiji     # -> packs/meiji/out/
$B build terrain-builder/packs/meiji --install   # also copy into assets/gen/terrain
cargo run                                 # play on it
$B restore                                # put the Civ3 art back
```

`init --placeholder` writes flat noise materials, so the whole pipeline
works without an API key. `generate --dry-run` prints the prompts, and
`generate --only coast,sea --force` regenerates a subset. You can also drop
any image into the pack as `<terrain>.png` (or set `texture`) instead of
generating it.

`build` writes:

| path | what |
|---|---|
| `out/terrain/sheets/*.png` | the nine sheets |
| `out/terrain/<terrain>_<0-2>.png` | single tiles (city screen) |
| `out/debug/masks/*.png` | the transition masks, flat-coloured |
| `out/preview.png` | a test island rendered with the engine's cell selection |
| `out/report.json` | validation results |

`preview <sheets dir> --out x.png` renders the same island from any sheets,
for example the originals in `assets/gen/terrain/sheets`, for a
side-by-side comparison. `masks --out dir` dumps the masks on their own.

## pack.json

```json
{
  "name": "meiji",
  "style": "early-Meiji Japanese landscape painting",
  "model": "fal-ai/flux/dev",
  "image_size": "square_hd",
  "masks": "civ3",
  "blend": "height",
  "seed": 1,
  "tiles_across": 8.0,
  "shore_width": 2.0,
  "shoreline_darken": 0.3,
  "color_match": false,
  "flatten": true,
  "terrains": {
    "grassland": { "prompt": "lush green grassland ground ..." },
    "coast": { "prompt": "...", "tiles_across": 6, "texture": "my-coast.jpg" },
    ...
  }
}
```

- `terrains`: `grassland plains desert tundra coast sea ocean` are
  required, and `shore` (beach sand) is optional; without it, lightened
  desert is used. Ice keeps its Civ3 art, because the engine draws it as
  an overlay.
- `masks`: `civ3` takes the transition shapes from the original sheets
  (it needs `civ3/civ3-gog/app/Art/Terrain`, `$CIV3_GOG` or `--civ3`).
  `procedural` builds them from noise and needs no Civ3 install.
- `blend`: `height` makes land terrains meet along their texture detail,
  so the brighter tufts of one poke into the other. `smooth` is a plain
  weighted average.
- `tiles_across`: how many map tiles the material image spans. Raise it
  for finer on-screen detail.
- `color_match`: recolours each material to the mean and contrast of the
  Civ3 terrain, so a new set keeps Civ3's palette.
- `flatten`: removes large-scale lighting and vignetting from materials.

## How it works

**Materials → periodic tiles** (`material.rs`). The material image is
viewed through the isometric projection: lattice coordinates
`a = x/128 + y/64`, `b = x/128 - y/64` are ground-plane coordinates in tile
units. Cells sit on the map at integer lattice steps, so a texture with
period 1 in `a` and `b` lines up across every cell boundary, in every
sheet. The periodic tile is the material windowed around one cell. Only
its rim cross-fades with the neighbouring periods, using a
variance-preserving blend so the rim keeps its contrast.

**Masks** (`masks.rs`). For each cell and pixel, the mask gives a weight
for each vertex terrain, plus a beach channel.

- `civ3`: Civ3 cells are individually painted. They do not share a
  texture, so this mode classifies each pixel by colour against the
  sheet's pure cells (0/0, 4/4, 8/8). Water gets a crisp cut, land gets
  soft weights with a vertex prior, and off-colour land next to water is
  beach. Classification specks are then removed. Neighbouring cells can
  come from different sheets, so `harmonize` pulls each edge, within a few
  pixels, onto one canonical profile per (edge axis, start terrain, end
  terrain), averaged over all sheets.
- `procedural`: bilinear vertex weights, pushed around by lattice-periodic
  noise. The noise fades out where a terrain has no weight, so on an edge
  only the edge's two endpoint terrains exist, and both cells along an
  edge agree by construction.

Both modes enforce the **vertex invariant**: near each vertex, the cell is
100% that vertex's terrain, so a tile center always shows its own type.

**Compositing** (`compose.rs`). Land is mixed by height or smooth blend,
beach is laid over it, water is cut in, and a dark line is drawn where
water meets land, like Civ3's coast outline.

**Validation** (`validate.rs`, `preview.rs`). Every build checks all 2916
cell vertices for purity and exact tile colour, and refuses to install if
one fails. It also reports two seam ratios: the colour step across shared
edges between different terrains, compared with the step across
same-terrain edges (`edge_seam_ratio`), and the step across cell
boundaries in the rendered preview, compared with the step inside cells
(`preview_seam_ratio`). A value near 1 means no visible seams. For
reference, the original Civ3 sheets score 1.37 on the preview.
