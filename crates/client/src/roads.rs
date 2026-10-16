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
use std::collections::{HashMap, HashSet};

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

/// The links drawn between squares with track, as sheet masks `(road, rail)` keyed by each
/// square's canonical position.
///
/// Rail runs between two squares that both have it; any other pair of neighbouring squares with
/// track is joined by road, so a railway that meets a road leaves a road stub to it. Joining every
/// such pair would turn a patch of track into a mesh of crossings and loops, so only a spanning
/// forest of the links is drawn: every square stays joined to the squares it can reach, by the
/// fewest links. Rail links are taken before road links so the railways form the trunk lines; then
/// links on thin lines before those inside a thick patch of track (counted by the squares with
/// track that touch both ends); then a scatter of the position, so no grid direction is favoured
/// and patches do not comb into parallel strokes. The forest is built over the whole map, so the
/// picture of a square never changes as the camera moves.
fn network(map: &Map, level: impl Fn(Coord) -> u8) -> HashMap<Coord, (u8, u8)> {
    struct Link {
        rank: (bool, u8, u32),
        ends: (usize, usize),
        squares: (Coord, Coord),
        direction: usize,
        rail: bool,
    }
    let mut links = Vec::new();
    for p in map.positions() {
        let own = level(p);
        if own == 0 {
            continue;
        }
        let Some(from) = map.index(p) else { continue };
        for (direction, &(dx, dy)) in Coord::NEIGHBORS.iter().enumerate() {
            let step = p.offset(dx, dy);
            let Some(q) = map.normalize(step) else {
                continue;
            };
            let other = level(q);
            let Some(to) = map.index(q) else { continue };
            if other == 0 || to <= from {
                continue;
            }
            let crowd = Coord::NEIGHBORS
                .iter()
                .map(|&(ex, ey)| p.offset(ex, ey))
                .filter(|&r| r != step && r.distance(step) == 1 && level(r) > 0)
                .count() as u8;
            let rail = own == 2 && other == 2;
            links.push(Link {
                rank: (!rail, crowd, scatter(from, to)),
                ends: (from, to),
                squares: (p, q),
                direction,
                rail,
            });
        }
    }
    links.sort_by_key(|link| link.rank);
    let mut parent: Vec<usize> = (0..map.tiles.len()).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let mut masks: HashMap<Coord, (u8, u8)> = HashMap::new();
    for link in links {
        let (a, b) = (
            root(&mut parent, link.ends.0),
            root(&mut parent, link.ends.1),
        );
        if a == b {
            continue;
        }
        parent[a] = b;
        let (p, q) = link.squares;
        for (square, bit) in [(p, link.direction), (q, (link.direction + 4) % 8)] {
            let entry = masks.entry(square).or_default();
            if link.rail {
                entry.1 |= 1 << bit;
            } else {
                entry.0 |= 1 << bit;
            }
        }
    }
    masks
}

/// An even scatter of a pair of squares, for breaking ties between equal links.
fn scatter(a: usize, b: usize) -> u32 {
    let mut h = (a as u32).wrapping_mul(0x9E37_79B1) ^ (b as u32).wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 12)
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
fn cells(own: u8, city: bool, (road, rail): (u8, u8)) -> (Option<u8>, Option<u8>) {
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
    let network = network(&game.map, |p| track(game, cities, &known, p));
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
            let links = game
                .map
                .normalize(here)
                .and_then(|p| network.get(&p).copied())
                .unwrap_or_default();
            let (road, rail) = cells(own, city, links);
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

    /// The forest over a 6 x 6 map with track only on the squares listed.
    fn forest(track: &[((i32, i32), u8)]) -> HashMap<Coord, (u8, u8)> {
        let map = Map::filled(6, 6, fourx_sim::terrain::Terrain::Grass);
        let level: HashMap<Coord, u8> = track
            .iter()
            .map(|&((x, y), t)| (Coord::new(x, y), t))
            .collect();
        network(&map, |p| level.get(&p).copied().unwrap_or(0))
    }

    fn links(net: &HashMap<Coord, (u8, u8)>) -> u32 {
        net.values()
            .map(|&(road, rail)| road.count_ones() + rail.count_ones())
            .sum::<u32>()
            / 2
    }

    #[test]
    fn railroads_join_railroads_and_roads_join_the_rest() {
        let net = forest(&[((1, 1), 2), ((2, 1), 2), ((3, 1), 1)]);
        // (1, 0) is neighbour 2, and the way back is neighbour 6
        assert_eq!(net[&Coord::new(1, 1)], (0, 1 << 2));
        assert_eq!(net[&Coord::new(2, 1)], (1 << 2, 1 << 6));
        assert_eq!(net[&Coord::new(3, 1)], (1 << 6, 0));
    }

    #[test]
    fn a_patch_of_track_is_drawn_as_a_tree() {
        // a full 3 x 3 block has 20 neighbouring pairs; joining them all draws a mesh of rings
        let block: Vec<_> = (0..3)
            .flat_map(|x| (0..3).map(move |y| ((x, y), 2)))
            .collect();
        let net = forest(&block);
        assert_eq!(links(&net), 8);
        // and every square is still reached from the corner
        let mut seen = HashSet::from([Coord::new(0, 0)]);
        let mut todo = vec![Coord::new(0, 0)];
        while let Some(p) = todo.pop() {
            let (road, rail) = net[&p];
            for (i, &(dx, dy)) in Coord::NEIGHBORS.iter().enumerate() {
                if (road | rail) >> i & 1 == 1 && seen.insert(p.offset(dx, dy)) {
                    todo.push(p.offset(dx, dy));
                }
            }
        }
        assert_eq!(seen.len(), 9);
    }

    #[test]
    fn railways_are_the_trunk_and_lines_stay_whole() {
        // a road square touching all three squares of a railway joins it once, and the railway
        // keeps both of its links
        let net = forest(&[((0, 0), 2), ((1, 0), 2), ((2, 0), 2), ((1, 1), 1)]);
        assert_eq!(links(&net), 3);
        assert_eq!(net[&Coord::new(1, 0)].1, 1 << 2 | 1 << 6);
        assert_eq!(net[&Coord::new(1, 1)].0.count_ones(), 1);
        // a thin diagonal line loses nothing
        let line: Vec<_> = (0..5).map(|i| ((i, i), 2)).collect();
        assert_eq!(links(&forest(&line)), 4);
    }

    #[test]
    fn squares_draw_what_they_carry() {
        // a lone road, and a lone railroad, are drawn as their short track
        assert_eq!(cells(1, false, (0, 0)), (Some(0), None));
        assert_eq!(cells(2, false, (0, 0)), (None, Some(0)));
        // a city alone draws nothing on its own ground
        assert_eq!(cells(2, true, (0, 0)), (None, None));
        // a railway end that meets only a road shows the road stub and its own short track
        assert_eq!(cells(2, false, (4, 0)), (Some(4), Some(0)));
        // a city reached by a road and a railway draws both
        assert_eq!(cells(2, true, (4, 64)), (Some(4), Some(64)));
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
