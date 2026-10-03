//! `VER#` - the file header: format version, scenario title and description.
//!
//! Reader `0x5E8180` (sub-object at `this+0x8CC`, called from the `VER#` arm
//! `0x594487`). Always the first section, always exactly one 720-byte row.
//! The version it carries is what the other readers gate on (see
//! [`crate::Version`]); the loader rejects majors outside `2..=12`
//! (`0x594530`, "Incompatible scenario file version").

use crate::fixed_record;
use crate::io::Str;

fixed_record! {
    /// The single `VER#` row (720 bytes).
    pub struct Header(b"VER#") {
        /// Header padding (body `+0`). Unread in `Civ3Conquests.exe`; often `0`,
        /// sometimes MSVC fill `0xCDCDCDCD` in Conquests scenarios.
        pub reserved_a: u32,
        /// Header padding (body `+4`). Unread; always `0` in the corpus.
        pub reserved_b: u32,
        /// BIQ major version (body `+8`): 2–4 Civ3 1.x, 11 PTW, 12 Conquests.
        pub major: u32,
        /// BIQ minor version (body `+0xc`; e.g. 12.08 → `12`, `8`).
        pub minor: u32,
        /// Scenario description shown in the scenario list (NUL padded, 640 B).
        pub description: Str<640>,
        /// Scenario title (NUL padded, 64 B).
        pub title: Str<64>,
    }
}

impl Header {
    /// The version as a comparable value.
    pub fn version(&self) -> crate::Version {
        crate::Version::new(self.major, self.minor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::Record;

    #[test]
    fn corpus_roundtrip() {
        if !crate::corpus::available() {
            return;
        }
        let st = crate::corpus::check_roundtrip::<Header>();
        assert_eq!(st.with_extra, 0);
        assert_eq!(st.short, 0);
        assert_eq!(st.lengths, vec![720]);
    }

    #[test]
    fn conquests_header_version_and_title() {
        let files: Vec<_> = crate::corpus::load_all()
            .into_iter()
            .filter_map(|f| f.ok())
            .collect();
        if files.is_empty() {
            return;
        }
        let f = files
            .iter()
            .find(|f| f.name().ends_with("Conquests/conquests.biq"))
            .expect("conquests.biq");
        let row = crate::corpus::rows::<Header>(std::slice::from_ref(f))
            .into_iter()
            .next()
            .expect("VER# row");
        let h = Header::read(&mut crate::io::Reader::new(row.body), &f.ctx()).unwrap();
        assert_eq!(h.major, 12);
        assert_eq!(h.minor, 8);
        assert_eq!(h.version(), crate::Version::new(12, 8));
        // Shipped conquests.biq leaves title/description blank in VER# (NUL-only).
        assert_eq!(h.title.text(), "");
    }

    #[test]
    fn civ3mod_header_is_bic_major_4() {
        let files: Vec<_> = crate::corpus::load_all()
            .into_iter()
            .filter_map(|f| f.ok())
            .collect();
        if files.is_empty() {
            return;
        }
        let f = files
            .iter()
            .find(|f| f.name().contains("civ3mod.bic"))
            .expect("civ3mod");
        let row = crate::corpus::rows::<Header>(std::slice::from_ref(f))
            .into_iter()
            .next()
            .expect("VER#");
        let h = Header::read(&mut crate::io::Reader::new(row.body), &f.ctx()).unwrap();
        assert_eq!(h.major, 4);
        assert_eq!(h.minor, 1);
    }
}
