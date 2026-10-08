//! Reproducible original starter art. No Civ3 files or reference-image pixels are used.
use image::{Rgba, RgbaImage};
use std::{f32::consts::PI, path::PathBuf};

type Color = [u8; 4];
const INK: Color = [30, 38, 35, 255];
const GOLD: Color = [204, 167, 93, 255];
fn pixel(im: &mut RgbaImage, x: i32, y: i32, c: Color) {
    if x >= 0 && y >= 0 && (x as u32) < im.width() && (y as u32) < im.height() {
        im.put_pixel(x as u32, y as u32, Rgba(c));
    }
}
fn rect(im: &mut RgbaImage, x: i32, y: i32, w: i32, h: i32, c: Color) {
    for yy in y..y + h {
        for xx in x..x + w {
            pixel(im, xx, yy, c);
        }
    }
}
fn ellipse(im: &mut RgbaImage, cx: i32, cy: i32, rx: i32, ry: i32, c: Color) {
    for y in cy - ry..=cy + ry {
        for x in cx - rx..=cx + rx {
            if ((x - cx) as f32 / rx as f32).powi(2) + ((y - cy) as f32 / ry as f32).powi(2) <= 1.0
            {
                pixel(im, x, y, c);
            }
        }
    }
}
fn poly(im: &mut RgbaImage, points: &[(i32, i32)], c: Color) {
    let min_y = points.iter().map(|p| p.1).min().unwrap();
    let max_y = points.iter().map(|p| p.1).max().unwrap();
    for y in min_y..=max_y {
        let mut xs = Vec::new();
        for i in 0..points.len() {
            let (x1, y1) = points[i];
            let (x2, y2) = points[(i + 1) % points.len()];
            if (y1 <= y && y2 > y) || (y2 <= y && y1 > y) {
                xs.push(x1 + (y - y1) * (x2 - x1) / (y2 - y1));
            }
        }
        xs.sort();
        for pair in xs.chunks_exact(2) {
            for x in pair[0]..=pair[1] {
                pixel(im, x, y, c);
            }
        }
    }
}
fn line(im: &mut RgbaImage, x1: i32, y1: i32, x2: i32, y2: i32, c: Color) {
    let steps = (x2 - x1).abs().max((y2 - y1).abs()).max(1);
    for i in 0..=steps {
        pixel(
            im,
            x1 + (x2 - x1) * i / steps,
            y1 + (y2 - y1) * i / steps,
            c,
        );
    }
}
fn noise(x: u32, y: u32, seed: u32) -> f32 {
    let mut v = x
        .wrapping_mul(374761393)
        .wrapping_add(y.wrapping_mul(668265263))
        .wrapping_add(seed.wrapping_mul(1274126177));
    v = (v ^ (v >> 13)).wrapping_mul(1274126177);
    (v ^ (v >> 16)) as f32 / u32::MAX as f32
}
fn tree(im: &mut RgbaImage, x: i32, y: i32, size: i32) {
    ellipse(im, x + 2, y + 2, size, size / 3, [20, 40, 26, 80]);
    rect(im, x - 1, y - size / 2, 3, size / 2, [96, 76, 46, 255]);
    for (i, c) in [[27, 62, 38, 255], [47, 83, 43, 255], [76, 111, 48, 255]]
        .into_iter()
        .enumerate()
    {
        let sy = y - size / 2 - i as i32 * size / 3;
        poly(
            im,
            &[(x - size / 2, sy), (x + size / 2, sy), (x, sy - size)],
            c,
        );
    }
    line(
        im,
        x,
        y - size * 2,
        x - size / 3,
        y - size,
        [112, 130, 61, 255],
    );
}
fn soldier(im: &mut RgbaImage, x: i32, y: i32, coat: Color) {
    ellipse(im, x, y + 1, 6, 2, [12, 20, 20, 100]);
    line(im, x - 2, y - 5, x - 3, y, INK);
    line(im, x + 2, y - 5, x + 3, y, INK);
    rect(im, x - 3, y - 15, 7, 10, coat);
    line(im, x - 3, y - 13, x + 3, y - 8, GOLD);
    rect(im, x - 2, y - 19, 5, 5, [208, 173, 133, 255]);
    rect(im, x - 3, y - 22, 7, 4, INK);
    line(im, x + 5, y - 20, x + 5, y - 3, [89, 66, 42, 255]);
    line(im, x + 5, y - 25, x + 5, y - 20, [184, 191, 177, 255]);
}
fn building(im: &mut RgbaImage, x: i32, y: i32, w: i32, h: i32) {
    poly(
        im,
        &[(x, y), (x + w, y - 5), (x + w, y - h - 5), (x, y - h)],
        [155, 141, 108, 255],
    );
    poly(
        im,
        &[
            (x, y),
            (x - w / 2, y - 5),
            (x - w / 2, y - h - 5),
            (x, y - h),
        ],
        [86, 91, 75, 255],
    );
    poly(
        im,
        &[
            (x - w / 2 - 2, y - h - 5),
            (x, y - h - 12),
            (x + w + 2, y - h - 5),
            (x, y - h + 1),
        ],
        [49, 61, 57, 255],
    );
    for xx in (x + 3..x + w - 2).step_by(5) {
        for yy in (y - h + 4..y - 2).step_by(7) {
            rect(im, xx, yy, 2, 3, [46, 50, 40, 255]);
        }
    }
    rect(im, x + 2, y - 7, 4, 7, [54, 46, 34, 255]);
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/packs/base")
        });
    for dir in ["terrain", "sprites", "audio", "ui"] {
        std::fs::create_dir_all(root.join(dir))?;
    }
    let mut atlas = RgbaImage::new(128 * 9, 64 * 9);
    let palette = [
        [112.0, 130.0, 67.0],
        [192.0, 170.0, 109.0],
        [36.0, 104.0, 123.0],
    ];
    for cell in 0..81usize {
        let row = cell / 9;
        let col = cell % 9;
        let digits = [col % 3, row % 3, row / 3, col / 3];
        for y in 0..64u32 {
            for x in 0..128u32 {
                let sx = (x as f32 + 0.5 - 64.0) / 64.0;
                let sy = (y as f32 + 0.5 - 32.0) / 32.0;
                if sx.abs() + sy.abs() > 1.025 {
                    continue;
                }
                // Linear interpolation in four center-to-edge triangles: exact vertex colors.
                let weights = [(-sy).max(0.0), sx.max(0.0), sy.max(0.0), (-sx).max(0.0)];
                let center = (1.0 - sx.abs() - sy.abs()).max(0.0);
                let mut type_weight = [0.0f32; 3];
                for i in 0..4 {
                    type_weight[digits[i]] += weights[i] + center / 4.0;
                }
                let water = type_weight[2];
                let sample = noise(x, y, cell as u32);
                let mut rgb = [0u8; 4];
                rgb[3] = 255;
                for channel in 0..3 {
                    let base = (0..3)
                        .map(|t| palette[t][channel] * type_weight[t])
                        .sum::<f32>();
                    let surf = if water > 0.12 && water < 0.65 {
                        (1.0 - (water - 0.35).abs() * 3.0) * 22.0
                    } else {
                        0.0
                    };
                    let ripples = if water > 0.5 {
                        ((x as f32 * 0.2 + y as f32 * 0.8).sin() + 1.0) * 4.0
                    } else {
                        0.0
                    };
                    rgb[channel] =
                        (base + (sample - 0.5) * 22.0 + surf + ripples).clamp(0.0, 255.0) as u8;
                }
                atlas.put_pixel(col as u32 * 128 + x, row as u32 * 64 + y, Rgba(rgb));
            }
        }
    }
    atlas.save(root.join("terrain/temperate.png"))?;
    let mut forest = RgbaImage::new(128, 112);
    for (x, y, s) in [
        (38, 79, 18),
        (75, 78, 20),
        (53, 67, 19),
        (87, 63, 17),
        (30, 60, 17),
        (65, 49, 15),
    ] {
        tree(&mut forest, x, y, s);
    }
    forest.save(root.join("sprites/forest.png"))?;
    let mut mountain = RgbaImage::new(128, 112);
    ellipse(&mut mountain, 66, 89, 51, 12, [27, 38, 27, 80]);
    poly(
        &mut mountain,
        &[(15, 88), (53, 25), (98, 89)],
        [98, 108, 92, 255],
    );
    poly(
        &mut mountain,
        &[(53, 25), (61, 74), (98, 89)],
        [63, 76, 67, 255],
    );
    poly(
        &mut mountain,
        &[(49, 33), (53, 25), (68, 48), (59, 44), (55, 49)],
        [222, 223, 199, 255],
    );
    poly(
        &mut mountain,
        &[(45, 91), (88, 47), (119, 87)],
        [136, 137, 109, 255],
    );
    poly(
        &mut mountain,
        &[(88, 47), (96, 82), (119, 87)],
        [76, 90, 78, 255],
    );
    for i in 0..24 {
        let x = 23 + i * 3;
        let y = 84 - (i % 5) * 2;
        line(&mut mountain, x, y, x + 4, y - 4, [109, 113, 83, 255]);
    }
    mountain.save(root.join("sprites/mountain.png"))?;
    let mut city = RgbaImage::new(128, 112);
    ellipse(&mut city, 64, 91, 48, 15, [43, 49, 35, 100]);
    poly(
        &mut city,
        &[(15, 83), (64, 63), (116, 83), (64, 107)],
        [153, 139, 96, 255],
    );
    for (x, y, w, h) in [
        (41, 74, 14, 19),
        (65, 71, 15, 23),
        (83, 78, 18, 18),
        (27, 85, 14, 19),
        (54, 92, 20, 25),
        (85, 93, 19, 26),
    ] {
        building(&mut city, x, y, w, h);
    }
    rect(&mut city, 77, 42, 7, 37, [79, 74, 63, 255]);
    rect(&mut city, 75, 40, 11, 4, [47, 53, 48, 255]);
    for i in 0..4 {
        ellipse(
            &mut city,
            79 - i * 4,
            34 - i * 6,
            5 + i,
            3 + i,
            [147, 152, 144, 90],
        );
    }
    building(&mut city, 53, 60, 16, 27);
    rect(&mut city, 53, 15, 2, 18, GOLD);
    rect(&mut city, 55, 16, 15, 9, [243, 225, 191, 255]);
    ellipse(&mut city, 61, 20, 3, 3, [170, 54, 44, 255]);
    city.save(root.join("sprites/city.png"))?;
    for (kind, coat) in [
        ("infantry", [34, 51, 65, 255]),
        ("pioneer", [137, 104, 55, 255]),
    ] {
        let mut im = RgbaImage::new(80, 80);
        for (x, y) in [(28, 60), (50, 57), (40, 43)] {
            soldier(&mut im, x, y, coat);
        }
        im.save(root.join(format!("sprites/{kind}.png")))?;
    }
    let mut cavalry = RgbaImage::new(80, 80);
    ellipse(&mut cavalry, 41, 58, 24, 6, [20, 29, 23, 90]);
    ellipse(&mut cavalry, 38, 48, 18, 7, [95, 65, 44, 255]);
    poly(
        &mut cavalry,
        &[(48, 49), (51, 30), (59, 29), (62, 35), (57, 44), (56, 51)],
        [112, 79, 51, 255],
    );
    for x in [25, 33, 44, 51] {
        line(&mut cavalry, x, 49, x - 2, 62, INK);
        line(&mut cavalry, x + 1, 49, x - 1, 62, INK);
    }
    soldier(&mut cavalry, 39, 43, [42, 63, 83, 255]);
    cavalry.save(root.join("sprites/cavalry.png"))?;
    let mut gun = RgbaImage::new(80, 80);
    ellipse(&mut gun, 42, 57, 25, 7, [20, 27, 22, 80]);
    poly(
        &mut gun,
        &[(20, 46), (55, 38), (66, 44), (32, 54)],
        [68, 79, 65, 255],
    );
    ellipse(&mut gun, 33, 53, 8, 8, INK);
    ellipse(&mut gun, 33, 53, 5, 5, GOLD);
    ellipse(&mut gun, 57, 45, 7, 7, INK);
    ellipse(&mut gun, 57, 45, 4, 4, GOLD);
    poly(
        &mut gun,
        &[(35, 39), (62, 25), (66, 30), (42, 45)],
        [90, 101, 92, 255],
    );
    soldier(&mut gun, 17, 57, [42, 55, 66, 255]);
    gun.save(root.join("sprites/artillery.png"))?;
    let mut ship = RgbaImage::new(128, 96);
    ellipse(&mut ship, 65, 75, 56, 10, [126, 184, 184, 90]);
    poly(
        &mut ship,
        &[(10, 65), (94, 43), (119, 55), (42, 82), (24, 78)],
        [35, 49, 52, 255],
    );
    poly(
        &mut ship,
        &[(13, 61), (93, 41), (117, 52), (42, 73)],
        [150, 153, 127, 255],
    );
    poly(
        &mut ship,
        &[(42, 55), (80, 45), (94, 50), (59, 63)],
        [60, 77, 77, 255],
    );
    for x in [51, 66, 80] {
        rect(&mut ship, x, 25 - (x - 50) / 4, 7, 28, INK);
        rect(
            &mut ship,
            x - 1,
            24 - (x - 50) / 4,
            9,
            4,
            [174, 159, 118, 255],
        );
        ellipse(&mut ship, x - 4, 15 - (x - 50) / 4, 9, 5, [86, 99, 97, 115]);
    }
    line(&mut ship, 97, 46, 97, 14, GOLD);
    rect(&mut ship, 98, 14, 17, 11, [239, 222, 189, 255]);
    ellipse(&mut ship, 106, 19, 3, 3, [169, 57, 43, 255]);
    ship.save(root.join("sprites/ironclad.png"))?;
    let mut parchment = RgbaImage::new(256, 256);
    for y in 0..256 {
        for x in 0..256 {
            let n = (noise(x, y, 99) - 0.5) * 12.0;
            parchment.put_pixel(
                x,
                y,
                Rgba([(221.0 + n) as u8, (206.0 + n) as u8, (166.0 + n) as u8, 255]),
            );
        }
    }
    parchment.save(root.join("ui/parchment.png"))?;
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 22050,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut wav = hound::WavWriter::create(root.join("audio/bell.wav"), spec)?;
    for i in 0..11025 {
        let t = i as f32 / 22050.0;
        let value = ((t * PI * 2.0 * 660.0).sin() * 0.55 + (t * PI * 2.0 * 990.0).sin() * 0.2)
            * (-t * 8.0).exp();
        wav.write_sample((value * 16000.0) as i16)?;
    }
    wav.finalize()?;
    println!(
        "Generated original terrain, unit, city, relief, parchment, and WAV assets in {}",
        root.display()
    );
    Ok(())
}
