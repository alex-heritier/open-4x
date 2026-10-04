//! A flat integer stream for saving the state structs of this crate without
//! a serialization dependency: every field becomes one `i64`, vectors carry
//! their length first. `research::World` and `diplomacy::Relations` use it
//! to save and restore a game in progress.

/// Appends words.
#[derive(Default)]
pub struct Writer(pub Vec<i64>);

impl Writer {
    /// One word.
    pub fn put(&mut self, v: impl Into<i64>) {
        self.0.push(v.into());
    }

    /// One flag.
    pub fn flag(&mut self, v: bool) {
        self.0.push(i64::from(v));
    }

    /// A length-prefixed run of words.
    pub fn run<T: Copy + Into<i64>>(&mut self, v: &[T]) {
        self.0.push(v.len() as i64);
        self.0.extend(v.iter().map(|&x| x.into()));
    }

    /// A length-prefixed run of flags.
    pub fn flags(&mut self, v: &[bool]) {
        self.0.push(v.len() as i64);
        self.0.extend(v.iter().map(|&x| i64::from(x)));
    }
}

/// Reads words back; every read fails with `None` once the stream is short.
pub struct Reader<'a> {
    words: &'a [i64],
    at: usize,
}

impl<'a> Reader<'a> {
    /// Start at the first word.
    pub fn new(words: &'a [i64]) -> Self {
        Reader { words, at: 0 }
    }

    /// The next word.
    pub fn get(&mut self) -> Option<i64> {
        let v = *self.words.get(self.at)?;
        self.at += 1;
        Some(v)
    }

    /// The next word as an `i32`.
    pub fn i32(&mut self) -> Option<i32> {
        i32::try_from(self.get()?).ok()
    }

    /// The next word as a `u32`.
    pub fn u32(&mut self) -> Option<u32> {
        u32::try_from(self.get()?).ok()
    }

    /// The next word as a flag.
    pub fn flag(&mut self) -> Option<bool> {
        Some(self.get()? != 0)
    }

    /// A length-prefixed run, each word mapped through `f`.
    pub fn run<T>(&mut self, mut f: impl FnMut(i64) -> Option<T>) -> Option<Vec<T>> {
        let n = usize::try_from(self.get()?).ok()?;
        if n > self.words.len() - self.at {
            return None;
        }
        (0..n).map(|_| f(self.get()?)).collect()
    }

    /// True when every word has been read.
    pub fn done(&self) -> bool {
        self.at == self.words.len()
    }
}
