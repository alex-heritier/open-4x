//! `EXPR` - combat experience / veteran levels.
//!
//! Row reader `0x5E1AD0` (arm `0x594991`); writer `0x5E1A20`. Editor dialog 147
//! *Combat Experience* (help: *Combat Experience Page*). Row body 40 bytes in every
//! corpus version. In-memory stride 44; consumers use body `+0x20` base HP and
//! `+0x24` retreat bonus (`combat.md`, table `[0x9C40CC]`).

use crate::fixed_record;
use crate::io::Str;

fixed_record! {
    /// One combat experience level (Conscript … Elite).
    pub struct ExperienceLevel(b"EXPR") {
        /// Level name (`Conscript`; body `+0`).
        pub name: Str<32>,
        /// "Base Hit Points" (B+A: dialog 1531; body `+0x20`). Stock `2…5`.
        pub base_hit_points: i32,
        /// "Retreat Bonus" success % (B+A: 1535; body `+0x24`). Stock `34…66`.
        pub retreat_bonus_percent: i32,
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
        let st = crate::corpus::check_roundtrip::<ExperienceLevel>();
        assert_eq!(st.with_extra, 0);
        assert_eq!(st.lengths, vec![40]);
    }

    #[test]
    fn conquests_experience_progression() {
        let files = crate::corpus::files();
        let Some(f) = files
            .iter()
            .find(|f| f.name().ends_with("Conquests/conquests.biq"))
        else {
            return;
        };
        let rows: Vec<ExperienceLevel> =
            crate::corpus::rows::<ExperienceLevel>(std::slice::from_ref(f))
                .into_iter()
                .map(|r| {
                    ExperienceLevel::read(&mut crate::io::Reader::new(r.body), &f.ctx()).unwrap()
                })
                .collect();
        let hp: Vec<i32> = rows.iter().map(|e| e.base_hit_points).collect();
        assert_eq!(hp, vec![2, 3, 4, 5]);
        assert_eq!(rows[0].name.text(), "Conscript");
        assert_eq!(rows[0].retreat_bonus_percent, 34);
        assert_eq!(rows[3].name.text(), "Elite");
        assert_eq!(rows[3].retreat_bonus_percent, 66);
    }
}
