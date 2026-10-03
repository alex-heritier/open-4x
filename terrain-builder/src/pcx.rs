//! Minimal reader for Civ3's 8-bit, single-plane, RLE PCX sheets.
//!
//! Transparency follows `tools/prep_assets.py`: magenta (255,0,255) and
//! palette index 255 are transparent.

use anyhow::{Context, Result, bail};
use std::path::Path;

pub struct Rgba {
    pub w: usize,
    pub h: usize,
    pub px: Vec<[u8; 4]>,
}

impl Rgba {
    pub fn get(&self, x: usize, y: usize) -> [u8; 4] {
        self.px[y * self.w + x]
    }
}

pub fn read(path: &Path) -> Result<Rgba> {
    let d = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    if d.len() < 128 + 769 || d[0] != 0x0A {
        bail!("{}: not a PCX file", path.display());
    }
    let u16_at = |i: usize| u16::from_le_bytes([d[i], d[i + 1]]) as usize;
    let (bpp, planes) = (d[3], d[65]);
    if bpp != 8 || planes != 1 {
        bail!("{}: only 8-bit single-plane PCX is supported", path.display());
    }
    let w = u16_at(8) - u16_at(4) + 1;
    let h = u16_at(10) - u16_at(6) + 1;
    let bpl = u16_at(66);
    let pal_at = d.len() - 769;
    if d[pal_at] != 0x0C {
        bail!("{}: missing 256-color palette", path.display());
    }
    let pal = &d[pal_at + 1..];

    // RLE runs may cross scanlines, so decode into one flat buffer.
    let mut idx = Vec::with_capacity(bpl * h);
    let mut i = 128;
    while idx.len() < bpl * h && i < pal_at {
        let b = d[i];
        if b & 0xC0 == 0xC0 {
            let n = (b & 0x3F) as usize;
            let v = *d.get(i + 1).context("truncated RLE run")?;
            idx.extend(std::iter::repeat_n(v, n));
            i += 2;
        } else {
            idx.push(b);
            i += 1;
        }
    }
    if idx.len() < bpl * h {
        bail!("{}: truncated image data", path.display());
    }
    let mut px = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let c = idx[y * bpl + x] as usize;
            let (r, g, b) = (pal[c * 3], pal[c * 3 + 1], pal[c * 3 + 2]);
            let transparent = c == 255 || (r, g, b) == (255, 0, 255);
            px.push([r, g, b, if transparent { 0 } else { 255 }]);
        }
    }
    Ok(Rgba { w, h, px })
}
