//! Byte-level plumbing shared by every section: a tolerant little-endian
//! cursor, a writer, fixed-size text buffers, and the [`fixed_record!`] macro.
//!
//! # The row-reading rule (`Reader`)
//!
//! Every row in a scenario stream is `[u32 len][len bytes]`. The game's row
//! readers (`0x5E3860` GOOD, `0x5E54B0` PRTO, ...) all follow one idiom:
//!
//! ```text
//! remaining = len
//! if remaining >= SIZE_OF_FIELD { fread(field); remaining -= SIZE_OF_FIELD }
//! ... next field ...
//! fseek(remaining)            // skip whatever the reader does not know
//! ```
//!
//! That is how one binary loads Civ3 1.x, PTW and Conquests files whose rows
//! grew by appended fields. [`Reader`] reproduces it: `take`/`u32`/... return
//! `None` (without consuming anything) when the row is too short, and the
//! record readers leave the field at its default.
//!
//! # Version gates and `extra`
//!
//! The game decides by fit alone; this crate also has to *write* rows, and a
//! writer needs to know which fields a file of a given version has. So a field
//! group that the format gained at a known file version is read only if the
//! file's version is at least that and the group fits, and is written under
//! the same condition ([`Ctx::version`]). For every shipped file the two tests
//! agree (the corpus tests assert it). Whatever a row holds beyond the modelled
//! fields is kept in the record's `extra` and written back after them, so a
//! file that carries data this model does not know, or whose rows disagree with
//! its declared version, still round-trips byte for byte.

use std::borrow::Cow;
use std::fmt;

/// A parse error. Row bodies are tolerant by design (see the module docs), so
/// these are limited to broken framing and implausible counts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// Input is neither a recognised scenario magic nor a DCL stream.
    BadMagic([u8; 4]),
    /// A DCL stream failed to decode.
    Dcl(crate::dcl::DclError),
    /// A section tag, count or row length ran past the end of the data.
    Truncated {
        /// Where in the decoded stream the problem is.
        offset: usize,
        /// What was being read.
        what: &'static str,
    },
    /// A byte sequence that should have been a section tag was not.
    BadTag {
        /// Offset in the decoded stream.
        offset: usize,
        /// The four bytes found.
        found: [u8; 4],
    },
    /// A count inside a row was larger than the row could hold.
    BadCount {
        /// Section tag.
        tag: [u8; 4],
        /// Description of the list.
        what: &'static str,
        /// The offending count.
        count: u32,
    },
    /// The same section tag appears twice (the model keeps one per tag).
    DuplicateSection([u8; 4]),
    /// A file could not be read.
    Io(String),
    /// A saved game this crate does not read: a format version or
    /// sub-version whose layout is not modelled, or a field the loader
    /// itself rejects.
    Unsupported {
        /// What is unsupported.
        what: &'static str,
        /// The value found.
        value: u32,
    },
    /// A saved-game chunk whose size is not the one the loader expects.
    BadChunkSize {
        /// Offset of the chunk header in the decoded stream.
        offset: usize,
        /// Chunk tag.
        tag: [u8; 4],
        /// Size found.
        size: u32,
        /// Size the loader copies into the game object.
        expected: u32,
    },
    /// A saved-game model that cannot be written as a valid stream: a list is
    /// not as long as the count the stream keeps for it.
    Inconsistent(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::BadMagic(m) => write!(f, "not a Civ3 scenario (magic {m:?})"),
            Error::Dcl(e) => write!(f, "DCL decompression failed: {e}"),
            Error::Truncated { offset, what } => write!(f, "truncated {what} at offset {offset}"),
            Error::BadTag { offset, found } => {
                write!(
                    f,
                    "expected a section tag at offset {offset}, found {found:?}"
                )
            }
            Error::DuplicateSection(tag) => {
                write!(f, "section {} appears twice", String::from_utf8_lossy(tag))
            }
            Error::Io(e) => write!(f, "{e}"),
            Error::Unsupported { what, value } => write!(f, "unsupported {what} {value}"),
            Error::BadChunkSize {
                offset,
                tag,
                size,
                expected,
            } => write!(
                f,
                "chunk {} at offset {offset} has size {size}, expected {expected}",
                String::from_utf8_lossy(tag)
            ),
            Error::Inconsistent(what) => write!(f, "cannot write: {what}"),
            Error::BadCount { tag, what, count } => write!(
                f,
                "{}: implausible {what} count {count}",
                String::from_utf8_lossy(tag)
            ),
        }
    }
}

impl std::error::Error for Error {}

impl From<crate::dcl::DclError> for Error {
    fn from(e: crate::dcl::DclError) -> Self {
        Error::Dcl(e)
    }
}

/// `Result` alias for this crate.
pub type Result<T> = std::result::Result<T, Error>;

// ---------------------------------------------------------------------------
// Reader
// ---------------------------------------------------------------------------

/// Little-endian cursor over one row body. See the module docs for the
/// "read if it fits" contract.
#[derive(Clone, Debug)]
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    /// Cursor over `buf` (a row body, i.e. without its `u32` length word).
    pub fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }

    /// Bytes not yet consumed. The game keeps this in the row's first word.
    pub fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    /// Bytes consumed so far.
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// Next `n` bytes, or `None` (nothing consumed) if fewer remain.
    pub fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.remaining() < n {
            return None;
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Some(s)
    }

    /// Look at the next `n` bytes without consuming them.
    pub fn peek(&self, n: usize) -> Option<&'a [u8]> {
        self.buf.get(self.pos..self.pos + n)
    }

    /// Consume and return everything that is left.
    pub fn rest(&mut self) -> &'a [u8] {
        let s = &self.buf[self.pos..];
        self.pos = self.buf.len();
        s
    }

    /// Skip `n` bytes; `false` (nothing consumed) if fewer remain.
    pub fn skip(&mut self, n: usize) -> bool {
        self.take(n).is_some()
    }

    /// `u8`, or `None` if the row is exhausted.
    pub fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }
    /// `i8`.
    pub fn i8(&mut self) -> Option<i8> {
        self.u8().map(|v| v as i8)
    }
    /// `u16` (little endian).
    pub fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }
    /// `i16`.
    pub fn i16(&mut self) -> Option<i16> {
        self.u16().map(|v| v as i16)
    }
    /// `u32` (little endian).
    pub fn u32(&mut self) -> Option<u32> {
        self.take(4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    /// `i32`.
    pub fn i32(&mut self) -> Option<i32> {
        self.u32().map(|v| v as i32)
    }
    /// `f32`.
    pub fn f32(&mut self) -> Option<f32> {
        self.u32().map(f32::from_bits)
    }
    /// Fixed-size byte array.
    pub fn array<const N: usize>(&mut self) -> Option<[u8; N]> {
        self.take(N).map(|b| {
            let mut a = [0u8; N];
            a.copy_from_slice(b);
            a
        })
    }
    /// Fixed-size text buffer.
    pub fn str<const N: usize>(&mut self) -> Option<Str<N>> {
        self.array::<N>().map(Str)
    }

    /// A `[u32 count]` followed by `count` elements of `T`.
    ///
    /// `Ok(None)` (nothing consumed) when not even the count word fits, which
    /// is how short rows from older versions leave a list empty. `Err` when the
    /// count promises more bytes than the row has: a count that big means this
    /// layout is not what the data holds, and guessing would mis-frame every
    /// field after it.
    pub fn counted_list<T: Field>(
        &mut self,
        tag: [u8; 4],
        what: &'static str,
    ) -> Result<Option<Vec<T>>> {
        let Some(head) = self.peek(4) else {
            return Ok(None);
        };
        let count = u32::from_le_bytes([head[0], head[1], head[2], head[3]]);
        let fits = (count as usize)
            .checked_mul(T::SIZE)
            .is_some_and(|bytes| bytes <= self.remaining() - 4);
        if !fits {
            return Err(Error::BadCount { tag, what, count });
        }
        self.skip(4);
        Ok(Some((0..count).map(|_| T::read(self)).collect()))
    }
}

// ---------------------------------------------------------------------------
// Writer
// ---------------------------------------------------------------------------

/// Little-endian byte sink, the mirror of [`Reader`].
#[derive(Clone, Debug, Default)]
pub struct Writer {
    /// Everything written so far.
    pub buf: Vec<u8>,
}

impl Writer {
    /// Empty writer.
    pub fn new() -> Self {
        Self::default()
    }
    /// Current length in bytes.
    pub fn len(&self) -> usize {
        self.buf.len()
    }
    /// True if nothing has been written.
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
    /// Raw bytes.
    pub fn bytes(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }
    /// `u8`.
    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }
    /// `i8`.
    pub fn i8(&mut self, v: i8) {
        self.buf.push(v as u8);
    }
    /// `u16`.
    pub fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    /// `i16`.
    pub fn i16(&mut self, v: i16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    /// `u32`.
    pub fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    /// `i32`.
    pub fn i32(&mut self, v: i32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    /// `f32`.
    pub fn f32(&mut self, v: f32) {
        self.buf.extend_from_slice(&v.to_bits().to_le_bytes());
    }
    /// A section tag.
    pub fn tag(&mut self, t: [u8; 4]) {
        self.buf.extend_from_slice(&t);
    }
    /// A `[u32 count]` followed by the elements: the mirror of
    /// [`Reader::counted_list`].
    pub fn counted_list<T: Field>(&mut self, items: &[T]) {
        self.u32(items.len() as u32);
        for item in items {
            item.write(self);
        }
    }
    /// A length-prefixed row: `[u32 len][body]`, where `body` is whatever `f` writes.
    ///
    /// `f` receives a **fresh** writer, so inside a row `w.len()` is the offset
    /// from the start of the row body (the same as in the reader). Row writers
    /// that decide what to emit from their position (`RULE` stops at a fixed
    /// offset) rely on this.
    pub fn row(&mut self, f: impl FnOnce(&mut Writer)) {
        let mut body = Writer::new();
        f(&mut body);
        self.u32(body.buf.len() as u32);
        self.buf.extend_from_slice(&body.buf);
    }
}

// ---------------------------------------------------------------------------
// Fixed-size text buffers
// ---------------------------------------------------------------------------

/// A fixed-size, NUL-padded text buffer as stored in the file.
///
/// The text is the bytes up to the first NUL, in Windows-1252. The bytes
/// *after* the NUL are preserved because the editor leaves stale data there
/// (`conquests.biq` FLAV names carry the tail of an earlier longer string);
/// keeping them makes the round trip byte-exact. Equality compares all `N`
/// bytes; use [`Str::text`] to compare as text.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Str<const N: usize>(pub [u8; N]);

impl<const N: usize> Default for Str<N> {
    fn default() -> Self {
        Str([0u8; N])
    }
}

impl<const N: usize> Str<N> {
    /// Build from text: zero padded, truncated to `N - 1` bytes so a NUL
    /// terminator always remains. Non-Latin-1 characters become `?`.
    pub fn new(s: &str) -> Self {
        let mut a = [0u8; N];
        for (i, c) in s.chars().take(N.saturating_sub(1)).enumerate() {
            a[i] = encode_cp1252(c);
        }
        Str(a)
    }

    /// The bytes before the first NUL.
    pub fn bytes(&self) -> &[u8] {
        let end = self.0.iter().position(|&b| b == 0).unwrap_or(N);
        &self.0[..end]
    }

    /// The text before the first NUL, decoded from Windows-1252.
    pub fn text(&self) -> Cow<'_, str> {
        let b = self.bytes();
        if b.is_ascii() {
            // SAFETY-free fast path: ASCII is valid UTF-8.
            Cow::Borrowed(std::str::from_utf8(b).unwrap())
        } else {
            Cow::Owned(b.iter().map(|&c| decode_cp1252(c)).collect())
        }
    }

    /// Bytes after the first NUL that are not zero (stale editor data), if any.
    pub fn stale_tail(&self) -> &[u8] {
        let end = self.0.iter().position(|&b| b == 0).unwrap_or(N);
        let tail = &self.0[end.min(N)..];
        match tail.iter().rposition(|&b| b != 0) {
            Some(last) => &tail[..=last],
            None => &[],
        }
    }

    /// True if the text is empty.
    pub fn is_empty(&self) -> bool {
        self.0.first().is_none_or(|&b| b == 0)
    }
}

impl<const N: usize> fmt::Debug for Str<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.text())
    }
}

impl<const N: usize> fmt::Display for Str<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text())
    }
}

impl<const N: usize> From<&str> for Str<N> {
    fn from(s: &str) -> Self {
        Str::new(s)
    }
}

const CP1252_HIGH: [char; 32] = [
    '\u{20AC}', '\u{81}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{8D}', '\u{017D}', '\u{8F}',
    '\u{90}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\u{9D}', '\u{017E}', '\u{0178}',
];

/// Windows-1252 byte to `char`.
pub fn decode_cp1252(b: u8) -> char {
    match b {
        0x80..=0x9F => CP1252_HIGH[(b - 0x80) as usize],
        _ => b as char,
    }
}

/// `char` to Windows-1252 byte (`?` if unrepresentable).
pub fn encode_cp1252(c: char) -> u8 {
    let u = c as u32;
    if u < 0x80 || (0xA0..=0xFF).contains(&u) {
        return u as u8;
    }
    CP1252_HIGH
        .iter()
        .position(|&h| h == c)
        .map(|i| 0x80 + i as u8)
        .unwrap_or(b'?')
}

// ---------------------------------------------------------------------------
// Field trait + fixed_record!
// ---------------------------------------------------------------------------

/// A fixed-size on-disk value usable in [`fixed_record!`].
pub trait Field: Sized {
    /// Size on disk in bytes.
    const SIZE: usize;
    /// The all-zero value (what an absent field reads as).
    fn zero() -> Self;
    /// Read a value; the caller has checked that `SIZE` bytes remain.
    fn read(r: &mut Reader<'_>) -> Self;
    /// Write the value.
    fn write(&self, w: &mut Writer);
}

macro_rules! impl_field_int {
    ($t:ty, $size:expr, $read:ident, $write:ident) => {
        impl Field for $t {
            const SIZE: usize = $size;
            fn zero() -> Self {
                0
            }
            fn read(r: &mut Reader<'_>) -> Self {
                r.$read().expect("caller checked remaining()")
            }
            fn write(&self, w: &mut Writer) {
                w.$write(*self)
            }
        }
    };
}
impl_field_int!(u8, 1, u8, u8);
impl_field_int!(i8, 1, i8, i8);
impl_field_int!(u16, 2, u16, u16);
impl_field_int!(i16, 2, i16, i16);
impl_field_int!(u32, 4, u32, u32);
impl_field_int!(i32, 4, i32, i32);

impl Field for f32 {
    const SIZE: usize = 4;
    fn zero() -> Self {
        0.0
    }
    fn read(r: &mut Reader<'_>) -> Self {
        r.f32().expect("caller checked remaining()")
    }
    fn write(&self, w: &mut Writer) {
        w.f32(*self)
    }
}

impl<const N: usize> Field for Str<N> {
    const SIZE: usize = N;
    fn zero() -> Self {
        Str([0u8; N])
    }
    fn read(r: &mut Reader<'_>) -> Self {
        r.str::<N>().expect("caller checked remaining()")
    }
    fn write(&self, w: &mut Writer) {
        w.bytes(&self.0)
    }
}

impl<T: Field, const N: usize> Field for [T; N] {
    const SIZE: usize = T::SIZE * N;
    fn zero() -> Self {
        std::array::from_fn(|_| T::zero())
    }
    fn read(r: &mut Reader<'_>) -> Self {
        std::array::from_fn(|_| T::read(r))
    }
    fn write(&self, w: &mut Writer) {
        for v in self {
            v.write(w);
        }
    }
}

/// Version information every record reader/writer may consult.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ctx {
    /// The file's `VER#` major.minor.
    pub version: crate::Version,
}

/// One row of a section.
pub trait Record: Sized + Default {
    /// Section tag, e.g. `*b"GOOD"`.
    const TAG: [u8; 4];
    /// Decode one row body (without its length word).
    ///
    /// Must follow the game's tolerance rule: fields that do not fit stay at
    /// their default, and bytes after the last known field are kept (as the
    /// record's `extra`) rather than dropped.
    fn read(r: &mut Reader<'_>, ctx: &Ctx) -> Result<Self>;
    /// Encode one row body in the *latest* layout (see crate docs).
    fn write(&self, w: &mut Writer, ctx: &Ctx);
    /// Bytes after the last modelled field in the row this record was read
    /// from. A fully understood layout leaves this empty for every row in
    /// every shipped file; the corpus tests assert exactly that.
    fn extra(&self) -> &[u8];
}

/// Declare a record whose body is a flat run of fixed-size fields.
///
/// ```ignore
/// fixed_record! {
///     /// A citizen type.
///     pub struct Citizen(b"CTZN") {
///         /// Display name.
///         pub name: Str<32>,
///         pub prerequisite: i32,
///     }
///     // Fields appended by a later file version; they exist on disk only
///     // when the file's version is >= the stated one.
///     since (12, 6) {
///         /// Added by Conquests.
///         pub corruption: i32,
///     }
/// }
/// ```
///
/// generates the struct (plus a trailing `extra: Vec<u8>` for unknown bytes),
/// `Default`, and a [`Record`] impl. A field is zero in `Default` unless it
/// says otherwise with `pub field: i32 = 1`: the value the game's row
/// constructor stores, which is what a row from an older file keeps for the
/// fields it lacks.
///
/// * `read` takes fields in order while they fit (the game's row rule), so a
///   short row from an older file leaves the later fields at their defaults;
///   a `since` group is taken only when `ctx.version` is at least the group's
///   version (groups are listed in increasing version order), so bytes a file
///   of an older version carries beyond its layout are kept in `extra`;
/// * `write` emits the base fields, then each `since` group only when
///   `ctx.version` is at least the group's version, then `extra`, so a file
///   re-encodes at its own version with exactly the row it had.
///
/// The `since` boundary is the version the loader's fix-up messages or the
/// editor's release notes give for the field; failing both, the lowest version
/// in the corpus that shows the longer row. (The loader itself keys on row
/// length, not version, so a cut-over between two sampled versions cannot
/// otherwise be observed.)
#[macro_export]
macro_rules! fixed_record {
    (@default $ty:ty) => {
        <$ty as $crate::io::Field>::zero()
    };
    (@default $ty:ty, $value:expr) => {
        $value
    };
    (
        $(#[$smeta:meta])*
        $svis:vis struct $name:ident ( $tag:expr ) {
            $(
                $(#[$fmeta:meta])*
                $fvis:vis $field:ident : $ty:ty $(= $fdef:expr)?
            ),* $(,)?
        }
        $(
            since ( $maj:literal , $min:literal ) {
                $(
                    $(#[$gmeta:meta])*
                    $gvis:vis $gfield:ident : $gty:ty $(= $gdef:expr)?
                ),* $(,)?
            }
        )*
    ) => {
        $(#[$smeta])*
        #[derive(Clone, Debug, PartialEq)]
        $svis struct $name {
            $(
                $(#[$fmeta])*
                $fvis $field: $ty,
            )*
            $($(
                $(#[$gmeta])*
                $gvis $gfield: $gty,
            )*)*
            /// Bytes after the last known field (empty unless the file carries
            /// data this crate does not model).
            pub extra: Vec<u8>,
        }

        impl Default for $name {
            fn default() -> Self {
                $name {
                    $( $field: $crate::fixed_record!(@default $ty $(, $fdef)?), )*
                    $($( $gfield: $crate::fixed_record!(@default $gty $(, $gdef)?), )*)*
                    extra: Vec::new(),
                }
            }
        }

        impl $crate::io::Record for $name {
            const TAG: [u8; 4] = *$tag;

            #[allow(unused_labels, unused_variables)]
            fn read(
                r: &mut $crate::io::Reader<'_>,
                ctx: &$crate::io::Ctx,
            ) -> $crate::io::Result<Self> {
                let mut rec = <$name as Default>::default();
                'fields: {
                    $(
                        if r.remaining() < <$ty as $crate::io::Field>::SIZE {
                            break 'fields;
                        }
                        rec.$field = <$ty as $crate::io::Field>::read(r);
                    )*
                    $(
                        // A group the file's version does not have is not read: its
                        // bytes, if the row has any, stay in `extra` and are written
                        // back unchanged.
                        if ctx.version < $crate::Version::new($maj, $min) {
                            break 'fields;
                        }
                        $(
                            if r.remaining() < <$gty as $crate::io::Field>::SIZE {
                                break 'fields;
                            }
                            rec.$gfield = <$gty as $crate::io::Field>::read(r);
                        )*
                    )*
                }
                rec.extra = r.rest().to_vec();
                Ok(rec)
            }

            #[allow(unused_variables)]
            fn write(&self, w: &mut $crate::io::Writer, ctx: &$crate::io::Ctx) {
                $( $crate::io::Field::write(&self.$field, w); )*
                $(
                    if ctx.version >= $crate::Version::new($maj, $min) {
                        $( $crate::io::Field::write(&self.$gfield, w); )*
                    }
                )*
                w.bytes(&self.extra);
            }

            fn extra(&self) -> &[u8] {
                &self.extra
            }
        }
    };
}
