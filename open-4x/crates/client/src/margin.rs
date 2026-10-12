//! The chart's margin: what the camera finds past the top and bottom of an upright world.
//!
//! The map is drawn like a Victorian atlas plate. A neatline frame (a gilt cable, a meander
//! frieze and a chequered degree scale) closes off the last row of tiles with a straight edge,
//! the plate's paper margin beyond it carries engravings (the campaign's title in a cartouche,
//! compass roses, motto ribbons, ships and a sea serpent), and past the margin the torn end of
//! the sheet lies on the cartographer's walnut desk among his instruments. Everything repeats
//! with the world's horizontal period, so it wraps with the map.

use bevy::prelude::*;
use fourx_sim::Game;

use crate::WorldRect;

/// Draw order: over the terrain, its fog and its borders, under cities and units, so a city on
/// the last row still stands in front of the frame.
const Z_DESK: f32 = 16.0;
const Z_INSTRUMENTS: f32 = 16.02;
const Z_PAPER: f32 = 16.04;
const Z_EDGE: f32 = 16.06;
const Z_BAND: f32 = 16.08;
const Z_ENGRAVING: f32 = 16.1;
const Z_LETTERING: f32 = 16.12;

/// Outward distances from the inside of the frame (the centres of the void row just past the
/// last row of tiles), in world units.
const BAND: f32 = 112.0;
/// The frame tile also carries a shadow this deep over the map.
const BAND_SHADOW: f32 = 8.0;
const BAND_WIDTH: f32 = 128.0;
/// The paper tile is drawn at its own size; two rows of it make the margin.
const PAPER: f32 = 256.0;
const PAPER_ROWS: i32 = 2;
/// Where the paper ends and the desk begins.
const SHEET_END: f32 = BAND - 12.0 + PAPER_ROWS as f32 * PAPER;
const EDGE: Vec2 = Vec2::new(256.0, 64.0);
const DESK: f32 = 512.0;
/// How far out the engravings and the instruments are centred.
const ENGRAVINGS: f32 = 330.0;
const INSTRUMENTS: f32 = SHEET_END + 320.0;
/// About how far apart along the edge the engravings stand.
const SPACING: f32 = 1150.0;

/// How far past the last row the camera may travel: the edge of the world can come up to a
/// quarter of the way from the far side of the screen, whatever the zoom.
pub(crate) fn reach(view_half_height: f32) -> f32 {
    32.0 + 0.5 * view_half_height
}

#[derive(Clone, Copy)]
enum Engraving {
    Cartouche,
    Compass,
    Ribbon,
    Ship,
    Steamer,
    Serpent,
}
impl Engraving {
    fn file(self) -> &'static str {
        match self {
            Engraving::Cartouche => "cartouche.png",
            Engraving::Compass => "compass.png",
            Engraving::Ribbon => "ribbon.png",
            Engraving::Ship => "ship.png",
            Engraving::Steamer => "steamer.png",
            Engraving::Serpent => "serpent.png",
        }
    }
    /// Drawn size in world units (the files are twice this).
    fn size(self) -> Vec2 {
        match self {
            Engraving::Cartouche => Vec2::new(512.0, 200.0),
            Engraving::Compass => Vec2::new(280.0, 280.0),
            Engraving::Ribbon => Vec2::new(560.0, 120.0),
            Engraving::Ship => Vec2::new(420.0, 230.0),
            Engraving::Steamer | Engraving::Serpent => Vec2::new(440.0, 220.0),
        }
    }
}

/// What stands along the north margin and along the south one, in turn.
const NORTH: [Engraving; 6] = [
    Engraving::Cartouche,
    Engraving::Ship,
    Engraving::Compass,
    Engraving::Ribbon,
    Engraving::Serpent,
    Engraving::Steamer,
];
const SOUTH: [Engraving; 6] = [
    Engraving::Ribbon,
    Engraving::Steamer,
    Engraving::Compass,
    Engraving::Serpent,
    Engraving::Cartouche,
    Engraving::Ship,
];
const MOTTOES: [&str; 4] = [
    "PLUS ULTRA",
    "TERRA INCOGNITA",
    "HIC SUNT DRACONES",
    "MARE TENEBRARUM",
];
const INK: Color = Color::srgb(0.16, 0.11, 0.07);
const RED_INK: Color = Color::srgb(0.47, 0.13, 0.09);

/// Upper-case Roman numerals, as a plate is dated.
fn roman(mut n: i32) -> String {
    const NUMERALS: [(i32, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut text = String::new();
    for (value, numeral) in NUMERALS {
        while n >= value {
            text.push_str(numeral);
            n -= value;
        }
    }
    text
}

/// The cartouche's two lines: the campaign's name without the date it often carries, and the
/// year it starts in.
fn title(game: &Game) -> (String, String) {
    let name = game
        .scenario_name
        .split(", ")
        .next()
        .unwrap_or_default()
        .to_uppercase();
    (name, format!("Anno Domini {}", roman(game.start_date.year())))
}

/// The positions `first + k * step` inside `lo..=hi`.
fn steps(first: f32, step: f32, lo: f32, hi: f32) -> impl Iterator<Item = f32> {
    let k0 = ((lo - first) / step).floor() as i32;
    let k1 = ((hi - first) / step).ceil() as i32;
    (k0..=k1).map(move |k| first + k as f32 * step)
}

/// Spawn the margin's sprites inside `built`. `folder` is the pack's margin folder; a pack
/// without one, or a world that is not upright, has no margin.
pub(crate) fn spawn<M: Component + Clone>(
    commands: &mut Commands,
    assets: &AssetServer,
    game: &Game,
    built: &WorldRect,
    prefix: &str,
    folder: &str,
    marker: M,
) {
    let Some(period) = game.map.screen_period().filter(|_| !folder.is_empty()) else {
        return;
    };
    let image = |name: &str| -> Handle<Image> { assets.load(format!("{prefix}{folder}{name}")) };
    let (left, bottom, _, top) = game.map.screen_bounds();
    let (title, subtitle) = title(game);
    let sprite = |commands: &mut Commands, handle: &Handle<Image>, size: Vec2, at: Vec3, flip: bool| {
        let mut sprite = Sprite::from_image(handle.clone());
        sprite.custom_size = Some(size);
        sprite.flip_y = flip;
        commands.spawn((sprite, Transform::from_translation(at), marker.clone()));
    };
    let (band, paper, edge, desk, instruments) = (
        image("band.png"),
        // The margin is the same aged paper as the city view's sheet.
        assets.load::<Image>(format!("{prefix}ui/paper.png")),
        image("edge.png"),
        image("desk.png"),
        image("instruments.png"),
    );
    let (x0, x1) = (built.min.x, built.max.x);
    for (side, line, row) in [(1.0, top + 32.0, &NORTH), (-1.0, bottom - 32.0, &SOUTH)] {
        // Outward distance `d` from the inside of the frame to world y.
        let y = |d: f32| line + side * d;
        // The span of outward distances the built area covers on this side.
        let outward = |world_y: f32| side * (world_y - line);
        let (lo, hi) = if side > 0.0 {
            (outward(built.min.y), outward(built.max.y))
        } else {
            (outward(built.max.y), outward(built.min.y))
        };
        if hi < -BAND_SHADOW {
            continue;
        }
        let flip = side < 0.0;
        let anchor = left - 64.0;
        // The frame.
        let band_height = BAND + BAND_SHADOW;
        for x in steps(anchor + BAND_WIDTH / 2.0, BAND_WIDTH, x0 - BAND_WIDTH, x1 + BAND_WIDTH) {
            let at = Vec3::new(x, y(BAND - band_height / 2.0), Z_BAND);
            sprite(commands, &band, Vec2::new(BAND_WIDTH, band_height), at, flip);
        }
        if hi < BAND {
            continue;
        }
        // The paper, then the torn end of the sheet over its last rows.
        for row_index in 0..PAPER_ROWS {
            let d = BAND - 12.0 + (row_index as f32 + 0.5) * PAPER;
            if d + PAPER / 2.0 < lo || d - PAPER / 2.0 > hi {
                continue;
            }
            for x in steps(anchor + PAPER / 2.0, PAPER, x0 - PAPER, x1 + PAPER) {
                sprite(commands, &paper, Vec2::splat(PAPER), Vec3::new(x, y(d), Z_PAPER), flip);
            }
        }
        if hi > SHEET_END - EDGE.y {
            for x in steps(anchor + EDGE.x / 2.0, EDGE.x, x0 - EDGE.x, x1 + EDGE.x) {
                let at = Vec3::new(x, y(SHEET_END), Z_EDGE);
                sprite(commands, &edge, EDGE, at, flip);
            }
        }
        // The desk, out to the end of what is built.
        if hi > SHEET_END {
            let mut d = SHEET_END + DESK / 2.0;
            while d - DESK / 2.0 < hi {
                if d + DESK / 2.0 > lo {
                    for x in steps(anchor + DESK / 2.0, DESK, x0 - DESK, x1 + DESK) {
                        sprite(commands, &desk, Vec2::splat(DESK), Vec3::new(x, y(d), Z_DESK), false);
                    }
                }
                d += DESK;
            }
        }
        // The engravings, evenly spaced round the world so they wrap with it; the south row is
        // set half a step along from the north one.
        let slots = (period / SPACING).round().max(1.0) as usize;
        let spacing = period / slots as f32;
        let shift = if side > 0.0 { 0.0 } else { spacing / 2.0 };
        for slot in 0..slots {
            let kind = row[slot % row.len()];
            let size = kind.size();
            let first = left + shift + (slot as f32 + 0.5) * spacing;
            let d = ENGRAVINGS;
            if d + size.y / 2.0 >= lo && d - size.y / 2.0 <= hi {
                for x in steps(first, period, x0 - size.x, x1 + size.x) {
                    let at = Vec3::new(x, y(d), Z_ENGRAVING);
                    sprite(commands, &image(kind.file()), size, at, false);
                    let mut letter = |text: &str, offset: f32, font_size: f32, color: Color| {
                        commands.spawn((
                            Text2d::new(text),
                            TextFont {
                                font_size,
                                ..default()
                            },
                            TextColor(color),
                            Transform::from_xyz(x, y(d) + offset, Z_LETTERING),
                            marker.clone(),
                        ));
                    };
                    match kind {
                        Engraving::Cartouche => {
                            letter(&title, 22.0, 26.0, INK);
                            letter(&subtitle, -25.0, 17.0, RED_INK);
                        }
                        Engraving::Ribbon => {
                            let motto = MOTTOES[(slot + if flip { 1 } else { 0 }) % MOTTOES.len()];
                            letter(motto, 1.0, 26.0, RED_INK);
                        }
                        _ => {}
                    }
                }
            }
            // Every other slot, an instrument lies on the desk beyond.
            if slot % 2 == 0 && INSTRUMENTS + 160.0 >= lo && INSTRUMENTS - 160.0 <= hi {
                let size = Vec2::new(640.0, 320.0);
                for x in steps(first + spacing * 0.3, period, x0 - size.x, x1 + size.x) {
                    let at = Vec3::new(x, y(INSTRUMENTS), Z_INSTRUMENTS);
                    sprite(commands, &instruments, size, at, false);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plates_are_dated_in_roman_numerals() {
        assert_eq!(roman(1895), "MDCCCXCV");
        assert_eq!(roman(1876), "MDCCCLXXVI");
        assert_eq!(roman(1914), "MCMXIV");
    }

    #[test]
    fn steps_cover_the_range() {
        let xs: Vec<f32> = steps(64.0, 128.0, 0.0, 300.0).collect();
        assert!(xs.first().is_some_and(|&x| x <= 0.0 + 64.0));
        assert!(xs.last().is_some_and(|&x| x >= 300.0));
    }
}
