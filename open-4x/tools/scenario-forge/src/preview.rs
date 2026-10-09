//! PNG previews so a human can check the map by eye.
use fourx_sim::terrain::{Coord, Cover, Relief, Terrain};
use fourx_sim::{Game, Id, Scenario};
use std::path::Path;

fn mix(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    let channel = |i: usize| (f32::from(a[i]) * (1.0 - t) + f32::from(b[i]) * t).round() as u8;
    [channel(0), channel(1), channel(2)]
}

/// The world as the plane the lattice lays out: one `scale`-pixel square per tile, upright, so
/// north is up and the picture is the 512 x 342 Mercator map the forge cut it from. Tiles are
/// tinted by the nation that owns them, with a dark mark where two nations meet, and the cities
/// on top. With `political` off it shows the terrain alone.
pub fn map_image(scenario: &Scenario, game: &Game, scale: usize, political: bool) -> Image {
    let map = &scenario.map;
    let lattice = map.lattice.expect("the world is laid out on a lattice");
    let (width, height) = ((2 * lattice.columns) as usize, (lattice.rows / 2) as usize);
    // Plane pixel (px, py) is native column px, row 2 * py + (px & 1): every tile exactly once.
    let tile_at = |px: usize, py: usize| lattice.cell(px as i32, (2 * py + (px & 1)) as i32);
    let nation_of =
        |p: Coord| -> Option<Id> { game.map.get(p).map(|t| t.owner).filter(|&o| o != 0) };
    let mut image = Image::new(width * scale, height * scale, [16, 34, 80]);
    for py in 0..height {
        for px in 0..width {
            let p = tile_at(px, py);
            let tile = map.get(p).expect("a lattice tile");
            let base = match tile.terrain {
                Terrain::Ocean => [16, 34, 80],
                Terrain::Sea => [26, 62, 118],
                Terrain::Coast => [50, 106, 160],
                Terrain::Grass => [112, 156, 84],
                Terrain::Plains => [186, 176, 100],
                Terrain::Desert => [226, 202, 140],
                Terrain::Tundra => [166, 176, 160],
            };
            if tile.is_water() {
                image.fill_rect(px * scale, py * scale, scale, base);
                continue;
            }
            let mut paint = match nation_of(p).filter(|_| political) {
                Some(id) => mix(base, game.factions[&id].color.0, 0.5),
                None => mix(base, [128, 128, 128], if political { 0.2 } else { 0.0 }),
            };
            match tile.cover {
                Cover::Forest => paint = mix(paint, [10, 70, 20], 0.45),
                Cover::Jungle => paint = mix(paint, [0, 96, 44], 0.6),
                Cover::Marsh => paint = mix(paint, [70, 120, 120], 0.5),
                Cover::Bare => {}
            }
            match tile.relief {
                Relief::Hills => paint = mix(paint, [120, 88, 56], 0.3),
                Relief::Mountains => paint = mix(paint, [236, 232, 226], 0.55),
                Relief::Flat => {}
            }
            // A river runs along an edge; the picture can only say a tile has one.
            if tile.river != 0 {
                paint = mix(paint, [70, 150, 235], 0.8);
            }
            // A dark mark on land tiles that touch another nation's land.
            if political
                && [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|&(dx, dy)| {
                    let q = Coord::new(p.x + dx, p.y + dy);
                    map.get(q).is_some_and(|t| t.is_land()) && nation_of(q) != nation_of(p)
                })
            {
                paint = mix(paint, [20, 20, 20], 0.75);
            }
            image.fill_rect(px * scale, py * scale, scale, paint);
        }
    }
    if !political {
        return image;
    }
    for city in &scenario.cities {
        let (cx, cy) = lattice.native(city.position);
        let (x, y) = (cx as usize * scale, (cy / 2) as usize * scale);
        let size = if city.capital {
            scale.max(3)
        } else {
            (scale / 2).max(2)
        };
        let inset = (scale.saturating_sub(size)) / 2;
        image.fill_rect(
            x + inset,
            y + inset,
            size,
            if city.capital {
                [255, 215, 0]
            } else {
                [255, 255, 255]
            },
        );
    }
    image
}

pub struct Image {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<[u8; 3]>,
}

impl Image {
    pub fn new(width: usize, height: usize, fill: [u8; 3]) -> Self {
        Self {
            width,
            height,
            pixels: vec![fill; width * height],
        }
    }
    pub fn set(&mut self, x: usize, y: usize, colour: [u8; 3]) {
        if x < self.width && y < self.height {
            self.pixels[y * self.width + x] = colour;
        }
    }
    pub fn get(&self, x: usize, y: usize) -> [u8; 3] {
        self.pixels[y * self.width + x]
    }
    pub fn fill_rect(&mut self, x: usize, y: usize, size: usize, colour: [u8; 3]) {
        for dy in 0..size {
            for dx in 0..size {
                self.set(x + dx, y + dy, colour);
            }
        }
    }
    /// Crops to a rectangle and scales it up by an integer factor.
    pub fn crop_scaled(
        &self,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
        scale: usize,
    ) -> Image {
        let mut out = Image::new(width * scale, height * scale, [0, 0, 0]);
        for oy in 0..height * scale {
            for ox in 0..width * scale {
                let (sx, sy) = (x + ox / scale, y + oy / scale);
                if sx < self.width && sy < self.height {
                    out.set(ox, oy, self.get(sx, sy));
                }
            }
        }
        out
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let file =
            std::fs::File::create(path).map_err(|error| format!("{}: {error}", path.display()))?;
        let mut encoder = png::Encoder::new(
            std::io::BufWriter::new(file),
            self.width as u32,
            self.height as u32,
        );
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
        let bytes: Vec<u8> = self.pixels.iter().flatten().copied().collect();
        writer
            .write_image_data(&bytes)
            .map_err(|error| error.to_string())
    }
}
