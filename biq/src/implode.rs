//! PKWARE DCL **implode**: the compressor half of [`crate::dcl`], ported from the
//! copy embedded in `Civ3Conquests.exe` ("PKWARE Data Compression Library for
//! Win32 ... Version 1.11").
//!
//! The port is *instruction faithful*, not merely format compatible: the game
//! and the editor wrote most shipped scenarios with this routine, and a
//! compressor that makes the same match choices reproduces those files **byte
//! for byte** (`tests::recompresses_every_shipped_file_exactly`). That is the
//! oracle for the port: 83 of 83 DCL-compressed files in the install come back
//! identical. The match finder is the part that cannot be guessed from the
//! format (the stream does not record how matches were chosen), so it is
//! transcribed from the disassembly, including its quirks (see
//! `Imploder::find_rep`).
//!
//! | piece | exe address | here |
//! |---|---|---|
//! | `implode` (table init, dictionary size and mode checks) | `0x648920` | [`compress`], `Imploder::new` |
//! | main loop (`WriteCmpData`) | `0x648AC0` | `Imploder::write_cmp_data` |
//! | match finder (`FindRep`) | `0x648E40` | `Imploder::find_rep` |
//! | bit writer (`OutputBits`) | `0x649180` | `BitWriter` |
//! | output flush | `0x6492C0` | not needed: chunking does not change the bytes |
//! | hash bucket sort (`SortBuffer`) | `0x649340` | `Imploder::sort_buffer` |
//!
//! # Compressor state (offsets in the exe's 36 312-byte work area)
//!
//! ```text
//! +0x0000 distance        last match distance minus 1 (`FindRep` result)
//! +0x0004 out_bytes       +0x0008 out_bits    +0x000C dsize_bits   +0x0010 dsize_mask
//! +0x0014 mode            +0x0018 dsize_bytes (0x400 / 0x800 / 0x1000)
//! +0x001C dist_bits[64]   +0x005C dist_codes[64]
//! +0x009C ch_bits[0x306]  +0x03A2 ch_codes[0x306] (u16)       literals, matches, end
//! +0x09BC fail[0x204]     KMP failure table of the current best match (u16)
//! +0x0DC8 hash_to_index[0x900] (u16)                          bucket starts
//! +0x1FCA out_buff[0x802]
//! +0x27CC work_buff[0x2204]                                   window + look-ahead + new block
//! +0x49D0 hash_offs[0x2204] (u16)                             positions sorted by hash
//! ```
//!
//! `work_buff` is `dsize_bytes` of history, `0x204` bytes of look-ahead
//! (the longest match is `0x204`), then up to `0x1000` bytes of new input.
//! Positions are stored as `u16` offsets into it.
//!
//! # What the match finder does
//!
//! A position is hashed by its first two bytes, `b0 * 4 + b1 * 5` (0x900
//! buckets, so different pairs collide, but equal `b0` with equal hash implies
//! equal `b1`; the comparison loop relies on that and starts at byte 2).
//! `SortBuffer` is a counting sort that leaves every bucket's positions in
//! **ascending** order. `FindRep` then walks the bucket from the oldest
//! position still inside the window:
//!
//! 1. permanently drops positions older than the window from the bucket head
//!    (it rewrites `hash_to_index`);
//! 2. tries each candidate, rejecting early if the byte at `best - 1` differs,
//!    and **accepts a candidate of equal length** (`len >= best`), so among
//!    equal matches the *nearest* wins;
//! 3. a match of 2 bytes is only usable at distance `< 0x100`; length `0x204`
//!    ends the search at once;
//! 4. once the best is longer than 10, it switches to a KMP-style scan: it
//!    builds the failure table of the best match and uses it to skip
//!    candidates that cannot be longer.
//!
//! The main loop adds a one-byte lazy evaluation (shorter matches are retried
//! one byte later, and the later one is taken only if it is longer by 2, or by
//! 1 when the first distance exceeds `0x80`).
//!
//! # Buffer contents past the end of the data
//!
//! At end of input the finder reads up to `0x204` bytes beyond the data. The
//! original never clears the work area, so those bytes are whatever the buffer
//! held: zeros on the first block, the previous block's tail afterwards. This
//! port keeps a real `work_buff` with the same shift (`memcpy` at `0x648CE0`)
//! and starts it zeroed, which reproduces every shipped file (nothing in the
//! corpus depends on uninitialised heap bytes).

use crate::dcl::{
    CH_BITS_ASC, CH_CODE_ASC, DIST_LEN, DIST_LOCODE, DclError, LEN_EXTRA_CNT, LEN_LOCODE, Mode,
    SHORT_EXTRA, TERMINATOR,
};

/// Longest match the format can express (`0x204`).
const MAX_REP: usize = 0x204;
/// Size of the `work_buff` area: dictionary (max `0x1000`) + `0x1000` + look-ahead.
const WORK_SIZE: usize = 0x2204;
/// Input is read in blocks of this many bytes (`0x648B10`: `ebp = 0x1000`).
const BLOCK: usize = 0x1000;
/// Entries of `hash_to_index` (`0x1200` bytes at `+0xDC8`).
const BUCKETS: usize = 0x900;
/// `work_buff` (`0x2204` bytes) then `hash_offs` (`0x2204` u16), `0x8DD8 - 0x27CC` bytes.
const MEM_SIZE: usize = WORK_SIZE + 2 * WORK_SIZE;

/// `hash_offs[i]`: the `i`-th sorted position, a u16 stored right after `work_buff`.
#[inline]
fn offs(mem: &[u8], i: usize) -> usize {
    let at = WORK_SIZE + 2 * i;
    u16::from_le_bytes([mem[at], mem[at + 1]]) as usize
}

/// Compress `input` as one DCL stream, with the exe's implode.
///
/// `dict_bits` is the window: `4`, `5` or `6` for 1, 2 or 4 KiB (`0x648966`
/// accepts exactly `0x400`, `0x800`, `0x1000`). Shipped files all use
/// `Mode::Binary` with `6`; see [`compress_like_the_game`].
pub fn compress(input: &[u8], mode: Mode, dict_bits: u8) -> Result<Vec<u8>, DclError> {
    if !(4..=6).contains(&dict_bits) {
        return Err(DclError::BadDictBits(dict_bits));
    }
    Ok(Imploder::new(mode, dict_bits).run(input))
}

/// The settings every shipped scenario and save uses: binary literals and a
/// 4 KiB window (header bytes `00 06`).
pub fn compress_like_the_game(input: &[u8]) -> Vec<u8> {
    compress(input, Mode::Binary, 6).expect("6 is a valid dictionary size")
}

/// LSB-first bit packer (`OutputBits`, `0x649180`). The exe writes into a
/// 0x802-byte buffer it flushes in 0x800-byte chunks (`0x6492C0`); the bytes
/// are the same as appending to one vector.
struct BitWriter {
    out: Vec<u8>,
    acc: u32,
    used: u32,
}

impl BitWriter {
    /// Low `nbits` bits of `value`, least significant first. The exe splits
    /// anything over 8 bits into a recursive call for the low byte, then the
    /// rest; the packing is identical to a plain bit loop.
    fn put(&mut self, nbits: u32, value: u32) {
        let mut value = value;
        let mut left = nbits;
        while left > 0 {
            let take = left.min(8 - self.used);
            self.acc |= (value & ((1 << take) - 1)) << self.used;
            self.used += take;
            value >>= take;
            left -= take;
            if self.used == 8 {
                self.out.push(self.acc as u8);
                self.acc = 0;
                self.used = 0;
            }
        }
    }

    /// A trailing partial byte counts (`0x648D0F`: `out_bits != 0` bumps `out_bytes`).
    fn finish(mut self) -> Vec<u8> {
        if self.used > 0 {
            self.out.push(self.acc as u8);
        }
        self.out
    }
}

/// The compressor state.
struct Imploder {
    mode: Mode,
    dsize_bits: u32,
    dsize_mask: u32,
    dsize_bytes: usize,
    ch_bits: [u8; 0x306],
    ch_codes: [u16; 0x306],
    /// `+0x0000`: distance minus 1 of the last match `find_rep` returned.
    distance: u32,
    /// `work_buff` immediately followed by `hash_offs`, as in the exe's work
    /// area (`+0x27CC`, `+0x49D0 = +0x27CC + 0x2204`): the match comparisons can
    /// read past the end of `work_buff` and then see the position table.
    mem: Vec<u8>,
    hash_to_index: [u16; BUCKETS],
    fail: [u16; 0x208],
    w: BitWriter,
}

impl Imploder {
    /// `implode` init, `0x648920`..`0x648AAC`.
    fn new(mode: Mode, dict_bits: u8) -> Imploder {
        // `0x648958`: bits = 4, mask = 0xF; each size step adds one bit and one mask bit.
        let dsize_bits = dict_bits as u32;
        let dsize_mask = (1u32 << dsize_bits) - 1;
        let dsize_bytes = 0x40usize << dsize_bits;

        let mut ch_bits = [0u8; 0x306];
        let mut ch_codes = [0u16; 0x306];
        match mode {
            // `0x6489AA`: 9 bits = flag 0 + 8 raw bits; code = 2 * byte (flag is bit 0).
            Mode::Binary => {
                for b in 0..0x100usize {
                    ch_bits[b] = 9;
                    ch_codes[b] = (b as u16) * 2;
                }
            }
            // `0x6489DC`: code length + 1 for the flag, code * 2.
            Mode::Ascii => {
                for b in 0..0x100usize {
                    ch_bits[b] = CH_BITS_ASC[b] + 1;
                    ch_codes[b] = CH_CODE_ASC[b].wrapping_mul(2);
                }
            }
        }
        // `0x648A12`: one entry per (length class, extra value). The code is
        // the flag bit `1`, the class code, then the extra bits.
        let mut idx = 0x100usize;
        for class in 0..16 {
            for extra in 0..(1u32 << SHORT_EXTRA[class]) {
                ch_bits[idx] = SHORT_EXTRA[class] + LEN_EXTRA_CNT[class] + 1;
                ch_codes[idx] = ((extra << (LEN_EXTRA_CNT[class] as u32 + 1))
                    | (LEN_LOCODE[class] as u32 * 2)
                    | 1) as u16;
                idx += 1;
            }
        }
        debug_assert_eq!(idx, TERMINATOR as usize + 1);

        Imploder {
            mode,
            dsize_bits,
            dsize_mask,
            dsize_bytes,
            ch_bits,
            ch_codes,
            distance: 0,
            mem: vec![0; MEM_SIZE],
            hash_to_index: [0; BUCKETS],
            fail: [0; 0x208],
            w: BitWriter {
                out: Vec::new(),
                acc: 0,
                used: 0,
            },
        }
    }

    fn put_symbol(&mut self, sym: usize) {
        self.w
            .put(self.ch_bits[sym] as u32, self.ch_codes[sym] as u32);
    }

    fn put_literal(&mut self, at: usize) {
        self.put_symbol(self.mem[at] as usize);
    }

    /// Emit the pending match of `rep` bytes at `self.distance` (`0x648D5F`).
    fn put_match(&mut self, rep: usize) {
        self.put_symbol(rep + 0xFE);
        let d = self.distance as usize;
        if rep == 2 {
            // Two-byte matches: 6-bit class of `d >> 2`, then two raw bits.
            self.w
                .put(DIST_LEN[d >> 2] as u32, DIST_LOCODE[d >> 2] as u32);
            self.w.put(2, self.distance & 3);
        } else {
            let class = d >> self.dsize_bits;
            self.w
                .put(DIST_LEN[class] as u32, DIST_LOCODE[class] as u32);
            self.w.put(self.dsize_bits, self.dsize_mask & self.distance);
        }
    }

    fn run(mut self, input: &[u8]) -> Vec<u8> {
        self.write_cmp_data(input);
        let mut out = vec![self.mode as u8, self.dsize_bits as u8];
        out.extend(self.w.finish());
        out
    }

    /// The main loop, `0x648AC0`. `pos` stands for the read callback.
    fn write_cmp_data(&mut self, input: &[u8]) {
        let dsize = self.dsize_bytes;
        let first_input = dsize + 0x204;
        let mut input_data = first_input; // edi
        let mut ended = false; // [esp+0x18]
        let mut phase = 0u32; // [esp+0x20]
        let mut pos = 0usize;

        loop {
            // `0x648B10`: fill up to 0x1000 bytes. The exe keeps calling the
            // reader until it has 0x1000 or the reader returns 0.
            let total = (input.len() - pos).min(BLOCK);
            self.mem[first_input..first_input + total].copy_from_slice(&input[pos..pos + total]);
            pos += total;
            if total < BLOCK {
                if total == 0 && phase == 0 {
                    break; // `0x648B52`: empty input, only the end symbol
                }
                ended = true;
            }
            // `0x648B60`: with the end reached, the look-ahead is processed too.
            let end = dsize + total + if ended { 0x204 } else { 0 };

            // Index the new block (and, after the first, the window behind it).
            match phase {
                0 => {
                    self.sort_buffer(input_data, end + 1);
                    phase = if dsize == 0x1000 { 1 } else { 2 };
                }
                1 => {
                    self.sort_buffer(input_data - dsize + 0x204, end + 1);
                    phase = 2;
                }
                _ => self.sort_buffer(input_data - dsize, end + 1),
            }

            while input_data < end {
                input_data = self.step(input_data, end, ended);
            }

            if ended {
                break;
            }
            // `0x648CBC`: slide the window; the stale bytes above stay where they are.
            self.mem.copy_within(BLOCK..BLOCK + dsize + 0x204, 0);
            input_data -= BLOCK;
        }
        self.put_symbol(TERMINATOR as usize);
    }

    /// One iteration of the compression loop, `0x648BE4`..`0x648CB2`: emit a
    /// literal or a match starting at `at`, returning the new position.
    fn step(&mut self, mut at: usize, end: usize, ended: bool) -> usize {
        let mut rep = self.find_rep(at) as usize;
        loop {
            // `0x648BF2`/`0x648BF8`: nothing usable, or a 2-byte match that is too far.
            if rep == 0 || (rep == 2 && self.distance >= 0x100) {
                self.put_literal(at);
                return at + 1;
            }
            // `0x648C09`: at the end of the data a match may not run past it.
            if ended && at + rep > end {
                rep = end - at;
                if rep < 2 || (rep == 2 && self.distance >= 0x100) {
                    self.put_literal(at);
                    return at + 1;
                }
                self.put_match(rep);
                return at + rep;
            }
            // `0x648C1D`: long matches are taken at once.
            if rep >= 8 || at + 1 >= end {
                self.put_match(rep);
                return at + rep;
            }
            // Lazy evaluation: is there a better match one byte later?
            let save_rep = rep;
            let save_distance = self.distance;
            rep = self.find_rep(at + 1) as usize;
            if save_rep >= rep || (save_rep + 1 >= rep && save_distance <= 0x80) {
                // `0x648D59`: keep the first match.
                self.distance = save_distance;
                self.put_match(save_rep);
                return at + save_rep;
            }
            // `0x648C64`: emit one literal and re-examine with the later match
            // (its distance is already in `self.distance`).
            self.put_literal(at);
            at += 1;
        }
    }

    /// `SortBuffer`, `0x649340`: counting sort of the positions `begin..end`
    /// (the exe passes `end` one past the last byte it wants hashed, and reads
    /// one byte beyond `end`) by their two-byte hash. Within a bucket positions
    /// end up ascending.
    fn sort_buffer(&mut self, begin: usize, end: usize) {
        self.hash_to_index.fill(0);
        let hash = |w: &[u8], p: usize| w[p] as usize * 4 + w[p + 1] as usize * 5;
        // `0x649376`: count (a do-while: the first position is always hashed).
        let mut p = begin;
        loop {
            let h = hash(&self.mem, p);
            self.hash_to_index[h] = self.hash_to_index[h].wrapping_add(1);
            p += 1;
            if end <= p {
                break;
            }
        }
        // `0x649399`: running sum, in u16 like the exe.
        let mut sum = 0u16;
        for slot in self.hash_to_index.iter_mut() {
            sum = sum.wrapping_add(*slot);
            *slot = sum;
        }
        // `0x6493AE`: place positions from the last one down, each at the
        // decremented bucket end, so a bucket ends up ascending.
        let mut q = end - 1;
        loop {
            let h = hash(&self.mem, q);
            self.hash_to_index[h] = self.hash_to_index[h].wrapping_sub(1);
            let at = WORK_SIZE + 2 * self.hash_to_index[h] as usize;
            self.mem[at..at + 2].copy_from_slice(&(q as u16).to_le_bytes());
            if q <= begin {
                break;
            }
            q -= 1;
        }
    }

    /// `FindRep`, `0x648E40`: the best earlier occurrence of the bytes at
    /// `src`. Returns its length (0 when none usable; 2..=0x204) and leaves the
    /// distance minus 1 in `self.distance`.
    ///
    /// Control flow follows the disassembly label for label. Names: `best` is
    /// `eax`, `hi` is `[esp+0x18]` (the index into `hash_offs` of the candidate
    /// under test), `limit` is `[esp+0x1C]` (`src - 1`: a candidate must start
    /// at least two bytes before `src`), `cand_end` is `[esp+0x14]`.
    fn find_rep(&mut self, src: usize) -> u32 {
        let dsize = self.dsize_bytes;
        let work = &self.mem;
        let h = work[src] as usize * 4 + work[src + 1] as usize * 5;
        let mut best = 1usize;

        // `0x648E68`..`0x648EA6`: discard positions that left the window; the
        // bucket head is rewritten so the next search starts after them.
        let min_off = src + 1 - dsize;
        let mut hi = self.hash_to_index[h] as usize;
        if (offs(&self.mem, hi)) < min_off {
            loop {
                hi += 1;
                if (offs(&self.mem, hi)) >= min_off {
                    break;
                }
            }
            self.hash_to_index[h] = hi as u16;
        }

        let limit = src - 1;
        let mut cand = offs(&self.mem, hi);
        if limit <= cand {
            return 0; // `0x648ED1`
        }

        // The candidate loop, `0x648EDF`. A later candidate is nearer.
        loop {
            'next: {
                // `0x648EDF`: the byte that would extend the best match must agree.
                if work[cand + best - 1] != work[src + best - 1] || work[cand] != work[src] {
                    break 'next;
                }
                // `0x648EEF`: bytes 0 and 1 are known equal (same hash and byte).
                let mut len = 2usize;
                let mut c = cand + 1;
                let mut s = src + 1;
                loop {
                    let expect = work[s + 1];
                    c += 1;
                    s += 1;
                    if work[c] != expect {
                        break;
                    }
                    len += 1;
                    if len >= MAX_REP {
                        break;
                    }
                }
                // `0x648F0C`: shorter than the best: skip. Equal or longer: take it.
                if len < best {
                    break 'next;
                }
                self.distance = (src as isize - c as isize + len as isize - 1) as u32;
                best = len;
                if len <= 10 {
                    break 'next;
                }
                // `0x648F50`: the longest possible match ends the search. (`c`
                // stopped one short of `cand + len` in this case, hence the fix-up.)
                if len == MAX_REP {
                    self.distance -= 1;
                    return len as u32;
                }
                return self.find_longer(src, hi, best, limit);
            }
            // `0x648F24`: next candidate.
            hi += 1;
            cand = offs(&self.mem, hi);
            if limit <= cand {
                return if best >= 2 { best as u32 } else { 0 }; // `0x648F41`
            }
        }
    }

    /// The tail of `FindRep` from `0x648F68`: the best match so far has length
    /// `best > 10`, found at `hash_offs[hi]`. Builds the KMP failure table of
    /// those `best` bytes and scans the remaining candidates, skipping any that
    /// cannot beat it. `src`, `limit` as in [`Imploder::find_rep`].
    fn find_longer(&mut self, src: usize, mut hi: usize, mut best: usize, limit: usize) -> u32 {
        // `0x648F68`: with no later candidate in range there is nothing to do.
        if (offs(&self.mem, hi + 1)) >= limit {
            return best as u32;
        }
        let work = &self.mem;

        // `0x648F8A`: failure function of `work[src..src + best]`; `fail[0] = -1`.
        // `var12` is `[esp+0x12]` (the string index), `di` the current border.
        self.fail[0] = 0xFFFF;
        self.fail[1] = 0;
        let mut var12 = 1usize;
        let mut di = 0u16;
        let extend = |fail: &mut [u16; 0x208], var12: &mut usize, di: &mut u16, upto: usize| {
            loop {
                // `0x648FA7`: advance on a match; on a mismatch fall back one border,
                // and from the empty border (-1) advance with border 0.
                let advance = if work[src + *var12] == work[src + *di as usize] {
                    true
                } else {
                    *di = fail[*di as usize];
                    *di == 0xFFFF
                };
                if advance {
                    *var12 += 1;
                    *di = di.wrapping_add(1);
                    fail[*var12] = *di;
                }
                if *var12 >= upto {
                    break;
                }
            }
        };
        extend(&mut self.fail, &mut var12, &mut di, best);

        // `0x648FEF`: the best candidate's end in the candidate's own bytes.
        let mut cand_end = offs(&self.mem, hi) + best;
        let mut esi = best; // length matched by the last candidate; then a border

        // `0x64900B`: LOOPB.
        loop {
            // Resume from the border of what matched last (`-1` becomes 0).
            esi = match self.fail[esi] {
                0xFFFF => 0,
                f => f as usize,
            };
            // `0x64902D`: advance to a candidate whose resume point is not
            // before the end of the previous one.
            let mut cand;
            loop {
                hi += 1;
                cand = offs(&self.mem, hi);
                if cand >= limit {
                    return best as u32; // `0x649048`
                }
                if cand + esi >= cand_end {
                    break;
                }
            }
            let c2 = work[src + best - 2]; // [esp+0x20]
            if work[cand + best - 2] == c2 {
                // `0x649069`: restart the comparison unless it lines up.
                if cand + esi != cand_end {
                    esi = 0;
                    cand_end = cand;
                }
            } else {
                // `0x64907A`: skip candidates until two bytes agree at both ends.
                loop {
                    hi += 1;
                    cand = offs(&self.mem, hi);
                    if cand >= limit {
                        return best as u32; // `0x6490A4`
                    }
                    if work[cand + best - 2] == c2 && work[cand] == work[src] {
                        break;
                    }
                }
                esi = 2;
                cand_end = cand + 2;
            }
            // `0x6490CA`: extend the comparison.
            loop {
                if work[src + esi] != work[cand_end] {
                    break;
                }
                esi += 1;
                if esi >= MAX_REP {
                    break;
                }
                cand_end += 1;
            }
            // `0x6490F7`: shorter than the best: keep scanning. Equal: the nearer
            // one replaces it. Longer: it replaces it and the table grows.
            if esi < best {
                continue;
            }
            self.distance = (src - cand - 1) as u32;
            if esi <= best {
                continue;
            }
            best = esi;
            if esi == MAX_REP {
                return best as u32; // `0x64916B`
            }
            extend(&mut self.fail, &mut var12, &mut di, esi);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dcl::decompress;

    /// Deterministic pseudo-random bytes with repeated phrases, so matches of
    /// every length class occur.
    fn sample(len: usize, seed: u32) -> Vec<u8> {
        let mut s = seed | 1;
        let mut next = move || {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            s
        };
        let mut v: Vec<u8> = Vec::new();
        while v.len() < len {
            match next() % 5 {
                0 if v.len() > 4 => {
                    let n = 2 + (next() % 40) as usize;
                    let back = 1 + (next() as usize) % v.len().min(5000);
                    for _ in 0..n {
                        let b = v[v.len() - back.min(v.len())];
                        v.push(b);
                    }
                }
                1 => {
                    let b = (next() % 7) as u8;
                    let n = (next() % 600) as usize;
                    v.extend(std::iter::repeat_n(b, n));
                }
                _ => v.push((next() % 253) as u8),
            }
        }
        v.truncate(len);
        v
    }

    #[test]
    fn round_trips_every_mode_window_and_length() {
        for mode in [Mode::Binary, Mode::Ascii] {
            for dict_bits in 4..=6u8 {
                for len in [
                    0, 1, 2, 3, 7, 100, 0xFFF, 0x1000, 0x1001, 0x2000, 9000, 20000,
                ] {
                    let data = sample(len, len as u32 * 31 + dict_bits as u32);
                    let packed = compress(&data, mode, dict_bits).unwrap();
                    assert_eq!(packed[0], mode as u8);
                    assert_eq!(packed[1], dict_bits);
                    assert_eq!(
                        decompress(&packed).unwrap(),
                        data,
                        "{mode:?} dict {dict_bits} len {len}"
                    );
                }
            }
        }
    }

    #[test]
    fn highly_repetitive_input_hits_the_longest_match() {
        let data = vec![0x5Au8; 100_000];
        let packed = compress_like_the_game(&data);
        assert!(packed.len() < 1000, "{}", packed.len());
        assert_eq!(decompress(&packed).unwrap(), data);
    }

    #[test]
    fn every_byte_value_round_trips_in_ascii_mode() {
        let data: Vec<u8> = (0..=255u8).chain((0..=255u8).rev()).collect();
        let packed = compress(&data, Mode::Ascii, 6).unwrap();
        assert_eq!(decompress(&packed).unwrap(), data);
        let packed = compress(&data, Mode::Binary, 6).unwrap();
        assert_eq!(decompress(&packed).unwrap(), data);
    }

    #[test]
    fn rejects_bad_dictionary_bits() {
        assert_eq!(
            compress(b"x", Mode::Binary, 3),
            Err(DclError::BadDictBits(3))
        );
        assert_eq!(
            compress(b"x", Mode::Binary, 7),
            Err(DclError::BadDictBits(7))
        );
    }

    #[test]
    fn empty_input_is_just_the_end_symbol() {
        // 2 header bytes + the 16-bit end symbol `0xFF01` (flag 1, class 15's code 0, extra 255), LSB first
        assert_eq!(compress_like_the_game(b""), [0, 6, 0x01, 0xFF]);
    }

    /// The oracle: the game's compressor, fed what the game's decompressor
    /// produced, must give back the shipped file. Files are listed so a
    /// failure names the first differing byte.
    #[test]
    fn recompresses_every_shipped_file_exactly() {
        let root = crate::corpus::install_root();
        let mut paths = Vec::new();
        // scenarios, and saved games (written by the same routine)
        crate::corpus::walk_ext(&root, &["biq", "bic", "bix", "sav"], &mut paths);
        let mut checked = 0;
        let mut seen = std::collections::HashSet::new();
        let mut failures = Vec::new();
        for p in paths {
            let Ok(bytes) = std::fs::read(&p) else {
                continue;
            };
            if !crate::dcl::looks_compressed(&bytes) || !seen.insert(bytes.clone()) {
                continue;
            }
            let plain = decompress(&bytes).unwrap();
            let again = compress(&plain, Mode::Binary, bytes[1]).unwrap();
            checked += 1;
            if again != bytes {
                let at = again
                    .iter()
                    .zip(&bytes)
                    .position(|(a, b)| a != b)
                    .unwrap_or(again.len().min(bytes.len()));
                failures.push(format!(
                    "{}: {} -> {} bytes, first difference at {at}",
                    p.display(),
                    bytes.len(),
                    again.len()
                ));
            }
        }
        eprintln!("recompressed {checked} compressed files");
        assert!(failures.is_empty(), "{}", failures.join("\n"));
        // Without an install there is nothing to check; with one, the
        // scenarios alone are 83 files.
        assert!(checked == 0 || checked >= 83, "only {checked} files");
    }
}
