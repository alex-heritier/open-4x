//! Roads and railroads, one sprite per square.
//!
//! This is how Civ3 draws them: a square that carries track looks at its eight neighbours and
//! picks, from a sheet with one cell per combination (2^8 = 256), the picture that runs from its
//! centre to each neighbour that is joined. The pictures meet at fixed points on the border
//! between squares, so any two cells join and a network of squares reads as one set of
//! curving roads with real forks, crossings and bends, instead of straight strips between
//! centres.
//!
//! The bits of a cell's number are [`Coord::NEIGHBORS`] in order (NE, E, SE, S, SW, W, NW, N on
//! screen). The pack's sheets are 16 cells wide; cell `m` is mask `m`, and cells past 256 hold
//! further drawings (variants) of the masks with at most two links, which are nearly every
//! square of a road. A square picks its variant from where it lies, so a long straight road
//! does not repeat its wobble. The art is made by `art/forge/roads.py`; `docs/roads.md` has the
//! whole contract.
use super::*;
use std::collections::HashSet;

/// Cells per row of a road or railroad sheet.
pub(super) const COLUMNS: u32 = 16;
/// Masks with at most two links, which have variants: 1 + 8 + 28.
const SIMPLE: usize = 37;
/// Position of each such mask among them, in order of the mask (the order of the sheet).
const RANK: [u8; 256] = {
    let mut rank = [0u8; 256];
    let (mut mask, mut next) = (0usize, 0u8);
    while mask < 256 {
        if (mask as u8).count_ones() <= 2 {
            rank[mask] = next;
            next += 1;
        }
        mask += 1;
    }
    rank
};
/// Roads lie over farms and under every relief; railroads lie on their roads.
const Z_ROAD: f32 = 3.0;
const Z_RAIL: f32 = 3.05;

/// A road or railroad sheet cut into cells.
pub(super) struct Sheet {
    layout: Handle<TextureAtlasLayout>,
    /// The image, held from the first frame so that it loads: nothing is drawn until it has.
    image: Option<Handle<Image>>,
    /// Size of the image the layout was last cut for; zero until it has loaded.
    size: UVec2,
    /// Drawings of each mask with at most two links, the base one included.
    variants: usize,
}
impl Sheet {
    pub(super) fn new(layouts: &mut Assets<TextureAtlasLayout>) -> Self {
        Self {
            layout: layouts.add(TextureAtlasLayout::from_grid(
                UVec2::new(192, 96),
                COLUMNS,
                COLUMNS,
                None,
                None,
            )),
            image: None,
            size: UVec2::ZERO,
            variants: 1,
        }
    }
    /// Load the sheet at `path` (empty for a pack that has none) and, once it is there, cut the
    /// layout for it. True when the layout changed and the map has to be drawn again.
    pub(super) fn fit(
        &mut self,
        prefix: &str,
        path: &str,
        assets: &AssetServer,
        images: &Assets<Image>,
        layouts: &mut Assets<TextureAtlasLayout>,
    ) -> bool {
        if path.is_empty() {
            return false;
        }
        let image = self
            .image
            .get_or_insert_with(|| assets.load(format!("{prefix}{path}")));
        let size = images.get(&*image).map(Image::size);
        size.is_some_and(|size| self.cut(size, layouts))
    }
    /// Cut the layout for an image of `size`. Packs may ship the art at any scale.
    fn cut(&mut self, size: UVec2, layouts: &mut Assets<TextureAtlasLayout>) -> bool {
        if size == self.size {
            return false;
        }
        let cell = UVec2::new(size.x / COLUMNS, size.x / COLUMNS / 2);
        let rows = size.y.checked_div(cell.y).unwrap_or(0);
        // The 256 masks come first, so a sheet with fewer rows than that is not one of ours.
        if cell.y == 0 || rows < 256 / COLUMNS {
            return false;
        }
        if let Some(layout) = layouts.get_mut(&self.layout) {
            *layout = TextureAtlasLayout::from_grid(cell, COLUMNS, rows, None, None);
        }
        self.variants = 1 + ((rows * COLUMNS) as usize).saturating_sub(256) / SIMPLE;
        self.size = size;
        true
    }
    /// Cell of `mask`, in the drawing `pick` selects (any number: it is taken modulo the
    /// variants the sheet has).
    fn cell(&self, mask: u8, pick: usize) -> usize {
        let variant = pick % self.variants;
        if variant == 0 || mask.count_ones() > 2 {
            usize::from(mask)
        } else {
            256 + (variant - 1) * SIMPLE + usize::from(RANK[usize::from(mask)])
        }
    }
}

/// How much track a square carries: 0 none, 1 road, 2 railroad. A city counts as both, as it
/// does for movement, so track that reaches a city is drawn into it.
fn track(game: &Game, cities: &HashSet<Coord>, known: &impl Fn(Coord) -> bool, p: Coord) -> u8 {
    if !known(p) {
        return 0;
    }
    match game.map.get(p) {
        Some(t) if cities.contains(&t.position) || t.has_rail() => 2,
        Some(t) if t.has_road() => 1,
        _ => 0,
    }
}

/// The neighbours a square joins by road and by rail, as sheet masks. Rail runs between two
/// squares that both have it; any other pair of squares with track is joined by road, so a
/// railway that meets a road leaves a road stub to it and the road reaches the railway.
fn links(own: u8, around: [u8; 8]) -> (u8, u8) {
    let (mut road, mut rail) = (0u8, 0u8);
    for (i, &other) in around.iter().enumerate() {
        if other == 2 && own == 2 {
            rail |= 1 << i;
        } else if other > 0 {
            road |= 1 << i;
        }
    }
    (road, rail)
}

/// A scatter of the position that breaks up repetition without touching any join, since every
/// variant of a cell leaves it the same way.
fn pick(p: Coord) -> usize {
    let mut h = (p.x as u32).wrapping_mul(0x9E37_79B1) ^ (p.y as u32).wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h >> 4) as usize
}

/// What a square with track (`own` is 1 or 2) draws: the road cell and the railroad cell, if
/// any. A railroad needs no road drawn under it, only the stubs to neighbours that have just a
/// road; a city draws the stubs that reach it but no lone track on its own ground.
fn cells(own: u8, city: bool, around: [u8; 8]) -> (Option<u8>, Option<u8>) {
    let (road, rail) = links(own, around);
    let road = (own == 1 || road != 0).then_some(road);
    let rail = (own == 2 && (rail != 0 || !city)).then_some(rail);
    (road, rail)
}

/// Spawn the road and railroad sprites of the tiles in `built` that the player knows. A sheet
/// that has not loaded draws nothing yet, and a pack without one has no such track on screen.
pub(super) fn draw(
    commands: &mut Commands,
    game: &Game,
    cities: &HashSet<Coord>,
    built: WorldRect,
    known: impl Fn(Coord) -> bool,
    roads: &Sheet,
    rails: &Sheet,
) {
    let (x0, y0, x1, y1) = built.tile_bounds(game);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let here = Coord::new(x, y);
            let own = track(game, cities, &known, here);
            let (sx, sy) = here.screen();
            if own == 0 || !built.has(Vec2::new(sx, sy)) {
                continue;
            }
            let city = game
                .map
                .get(here)
                .is_some_and(|t| cities.contains(&t.position));
            let around =
                Coord::NEIGHBORS.map(|(dx, dy)| track(game, cities, &known, here.offset(dx, dy)));
            let (road, rail) = cells(own, city, around);
            let pick = pick(here);
            for (mask, sheet, z) in [(road, roads, Z_ROAD), (rail, rails, Z_RAIL)] {
                let (Some(mask), Some(image), true) =
                    (mask, &sheet.image, sheet.size != UVec2::ZERO)
                else {
                    continue;
                };
                let mut sprite = Sprite::from_atlas_image(
                    image.clone(),
                    TextureAtlas {
                        layout: sheet.layout.clone(),
                        index: sheet.cell(mask, pick),
                    },
                );
                sprite.custom_size = Some(Vec2::new(128.0, 64.0));
                commands.spawn((sprite, Transform::from_xyz(sx, sy, z), WorldVisual));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NE: u8 = 1;
    const E: u8 = 2;
    const SW: u8 = 16;

    fn sheet(rows: u32) -> Sheet {
        let mut layouts = Assets::<TextureAtlasLayout>::default();
        let mut sheet = Sheet::new(&mut layouts);
        assert!(sheet.cut(UVec2::new(3072, rows * 96), &mut layouts));
        sheet
    }

    #[test]
    fn bits_are_the_neighbour_order() {
        // The cell numbers of the sheet are Coord::NEIGHBORS: NE, E, SE, S, SW, W, NW, N.
        let screen = |i: usize| {
            let (dx, dy) = Coord::NEIGHBORS[i];
            let (x, y) = Coord::new(0, 0).offset(dx, dy).screen();
            ((x as i32).signum(), (y as i32).signum())
        };
        let expect = [
            (1, 1),
            (1, 0),
            (1, -1),
            (0, -1),
            (-1, -1),
            (-1, 0),
            (-1, 1),
            (0, 1),
        ];
        for (i, want) in expect.into_iter().enumerate() {
            assert_eq!(screen(i), want, "neighbour {i}");
        }
    }

    #[test]
    fn a_road_joins_every_neighbour_that_has_track() {
        let mut around = [0u8; 8];
        around[0] = 1;
        around[4] = 1;
        assert_eq!(links(1, around), (NE | SW, 0));
        // railroads join railroads; a road beside a railway is joined by road
        around[0] = 2;
        assert_eq!(links(2, around), (SW, NE));
        assert_eq!(links(1, around), (NE | SW, 0));
        // and a city, which counts as a railroad, takes both
        assert_eq!(links(2, [2, 1, 0, 0, 0, 0, 0, 0]), (E, NE));
    }

    #[test]
    fn squares_draw_what_they_carry() {
        let none = [0u8; 8];
        // a lone road, and a lone railroad, are drawn as their short track
        assert_eq!(cells(1, false, none), (Some(0), None));
        assert_eq!(cells(2, false, none), (None, Some(0)));
        // a city alone draws nothing on its own ground
        assert_eq!(cells(2, true, none), (None, None));
        // a railway end that meets only a road shows the road stub and its own short track
        let mut around = none;
        around[2] = 1;
        assert_eq!(cells(2, false, around), (Some(4), Some(0)));
        // a city reached by a road and a railway draws both
        around[6] = 2;
        assert_eq!(cells(2, true, around), (Some(4), Some(64)));
    }

    #[test]
    fn masks_with_few_links_have_variants_and_busy_ones_do_not() {
        // 16 rows hold the masks; four variants of the 37 simple masks need 7 rows more.
        let s = sheet(23);
        assert_eq!(s.variants, 4);
        assert_eq!(s.cell(0b0000_0110, 0), 6);
        assert_eq!(s.cell(0b0000_0111, 1), 0b0000_0111);
        assert_eq!(s.cell(0, 1), 256);
        assert_eq!(s.cell(0b0000_1000, 1), 256 + 7);
        assert_eq!(s.cell(0b0000_1000, 3), 256 + 2 * SIMPLE + 7);
        assert_eq!(s.cell(0b0000_1000, 4), 0b0000_1000);
        // the last cell any mask can reach is on the sheet
        let top = (0..=255u8)
            .flat_map(|m| (0..4).map(move |v| (m, v)))
            .map(|(m, v)| s.cell(m, v))
            .max()
            .unwrap();
        assert!(top < 23 * COLUMNS as usize);
        // a sheet of just the 256 masks has no variants
        assert_eq!(sheet(16).cell(0b0000_1000, 3), 0b0000_1000);
    }

    #[test]
    fn variants_are_spread_over_a_road() {
        let counts = (0..40).fold([0usize; 4], |mut n, x| {
            n[pick(Coord::new(x, 3)) % 4] += 1;
            n
        });
        assert!(counts.iter().all(|&n| n >= 3), "{counts:?}");
        // and every square always picks the same one
        assert_eq!(pick(Coord::new(7, 9)), pick(Coord::new(7, 9)));
    }

    #[test]
    fn the_base_pack_sheets_fit_the_layout() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/packs/base/");
        let size = |name: &str| {
            let bytes = std::fs::read(format!("{root}{name}")).unwrap();
            // PNG: width and height are the first fields of the IHDR chunk
            let field = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
            UVec2::new(field(16), field(20))
        };
        let pack = fourx_content::Content::base().pack;
        for (name, variants) in [(&pack.visuals.roads, 4), (&pack.visuals.railroads, 3)] {
            let mut layouts = Assets::<TextureAtlasLayout>::default();
            let mut sheet = Sheet::new(&mut layouts);
            assert!(sheet.cut(size(name), &mut layouts), "{name}");
            assert_eq!(size(name).x % COLUMNS, 0, "{name}");
            assert_eq!(sheet.variants, variants, "{name}");
        }
    }
}
