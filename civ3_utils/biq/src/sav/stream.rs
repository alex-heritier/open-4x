//! The two primitives every saved-game loader is built from, and their
//! mirror images: a tagged, sized chunk (`0x4FCBB0` / `0x4FCB60`) and a raw
//! block that is the bytes of an array with no framing at all.

use super::body::Body;
use crate::io::{Error, Result, Writer};

/// One 12-byte list item (the three-dword records of the player lists).
pub type Rec12 = [u8; 12];

/// Read cursor over the decoded stream.
pub(super) struct Rd<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Rd<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Rd { buf, pos: 0 }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    /// The next `n` bytes (raw block).
    pub fn take(&mut self, n: usize, what: &'static str) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&e| e <= self.buf.len())
            .ok_or(Error::Truncated {
                offset: self.pos,
                what,
            })?;
        let out = &self.buf[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    /// `n` elements of `size` bytes. The product is checked against the data
    /// that is left before anything is allocated, so a corrupt count cannot
    /// size a buffer.
    fn items(&mut self, n: usize, size: usize, what: &'static str) -> Result<&'a [u8]> {
        let bytes = n.checked_mul(size).ok_or(Error::Truncated {
            offset: self.pos,
            what,
        })?;
        self.take(bytes, what)
    }

    pub fn u32(&mut self, what: &'static str) -> Result<u32> {
        let b = self.take(4, what)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn bytes(&mut self, n: usize, what: &'static str) -> Result<Vec<u8>> {
        Ok(self.take(n, what)?.to_vec())
    }

    pub fn u32s(&mut self, n: usize, what: &'static str) -> Result<Vec<u32>> {
        Ok(self
            .items(n, 4, what)?
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect())
    }

    pub fn u16s(&mut self, n: usize, what: &'static str) -> Result<Vec<u16>> {
        Ok(self
            .items(n, 2, what)?
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect())
    }

    /// `u32 count` followed by `count` 12-byte items.
    pub fn list12(&mut self, what: &'static str) -> Result<Vec<Rec12>> {
        let n = self.u32(what)? as usize;
        Ok(self
            .items(n, 12, what)?
            .chunks_exact(12)
            .map(|c| {
                let mut r = [0u8; 12];
                r.copy_from_slice(c);
                r
            })
            .collect())
    }

    /// 32 consecutive [`Rd::list12`] lists, one per player slot.
    pub fn lists12(&mut self, what: &'static str) -> Result<Vec<Vec<Rec12>>> {
        (0..32).map(|_| self.list12(what)).collect()
    }

    /// A chunk: `tag[4]`, `u32 size`, `size` bytes, with `size == N`.
    pub fn chunk<const N: usize>(&mut self, tag: &[u8; 4]) -> Result<Body<N>> {
        let offset = self.pos;
        let head = self.take(8, "chunk header")?;
        if &head[..4] != tag {
            self.pos = offset;
            return Err(Error::BadTag {
                offset,
                found: [head[0], head[1], head[2], head[3]],
            });
        }
        let size = u32::from_le_bytes([head[4], head[5], head[6], head[7]]);
        if size as usize != N {
            self.pos = offset;
            return Err(Error::BadChunkSize {
                offset,
                tag: *tag,
                size,
                expected: N as u32,
            });
        }
        let mut b = [0u8; N];
        b.copy_from_slice(self.take(N, "chunk body")?);
        Ok(Body(b))
    }

    /// `n` chunks with the same tag and size.
    pub fn chunks<const N: usize>(&mut self, n: usize, tag: &[u8; 4]) -> Result<Vec<Body<N>>> {
        // Each chunk is at least its 8-byte header: refuse counts the data
        // cannot hold before reserving anything.
        if n > self.remaining() / (8 + N) {
            return Err(Error::Truncated {
                offset: self.pos,
                what: "chunk list",
            });
        }
        (0..n).map(|_| self.chunk::<N>(tag)).collect()
    }

    /// Everything not yet consumed, without consuming it.
    pub fn rest(&self) -> &'a [u8] {
        &self.buf[self.pos..]
    }

    /// Whether the stream was consumed exactly.
    pub fn at_end(&self) -> bool {
        self.pos == self.buf.len()
    }
}

// --- writing --------------------------------------------------------------

pub(super) fn put_chunk<const N: usize>(w: &mut Writer, tag: &[u8; 4], b: &Body<N>) {
    w.tag(*tag);
    w.u32(N as u32);
    w.bytes(&b.0);
}

pub(super) fn put_chunks<const N: usize>(w: &mut Writer, tag: &[u8; 4], list: &[Body<N>]) {
    for b in list {
        put_chunk(w, tag, b);
    }
}

pub(super) fn put_u32s(w: &mut Writer, v: &[u32]) {
    for &x in v {
        w.u32(x);
    }
}

pub(super) fn put_u16s(w: &mut Writer, v: &[u16]) {
    for &x in v {
        w.u16(x);
    }
}

pub(super) fn put_list12(w: &mut Writer, list: &[Rec12]) {
    w.u32(list.len() as u32);
    for r in list {
        w.bytes(r);
    }
}

pub(super) fn put_lists12(w: &mut Writer, lists: &[Vec<Rec12>]) -> Result<()> {
    want(lists.len(), 32, "a player list set must have 32 lists")?;
    for l in lists {
        put_list12(w, l);
    }
    Ok(())
}

/// `Err(Inconsistent(what))` unless `len == expected`.
pub(super) fn want(len: usize, expected: usize, what: &'static str) -> Result<()> {
    if len == expected {
        Ok(())
    } else {
        Err(Error::Inconsistent(what))
    }
}
