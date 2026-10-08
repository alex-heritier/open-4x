//! `WCHR` — world creation slider pairs (climate, barbarians, landform, oceans,
//! temperature, age) and the chosen [`crate::sections::wsiz::WorldSize`] preset.
//!
//! Arm `0x594A3E`, worker `0x596EF0`, row reader `0x5EB1E0`, writer `0x5EB040`.
//! Editor dialog **3** (`WCHR`, `editor.md`). Row is **52** bytes (13 dwords).
//!
//! Each generator slider is stored as **selected** then **resolved** dwords
//! (`0x5EB040` fwrite order). When the player chose *Random* in world setup,
//! selected is `3` and actual holds the rolled value (C: `islands.bic` climate
//! `3→2`, barbarian `3→1`; `Intro2_The_Three_Sisters.biq` barbarians `3→2`).

use crate::fixed_record;

/// Climate / aridity slider values (body climate pair). **E+C**
pub mod climate {
    pub const ARID: i32 = 0;
    pub const NORMAL: i32 = 1;
    pub const WET: i32 = 2;
    /// UI *Random*; resolved value is in [`super::WorldCharacteristics::climate_actual`].
    pub const RANDOM: i32 = 3;
}

/// Barbarian activity slider values. **E+C** (corpus selected: `-1..=4`).
pub mod barbarian {
    pub const NONE: i32 = -1;
    pub const SEDENTARY: i32 = 0;
    pub const ROAMING: i32 = 1;
    pub const RESTLESS: i32 = 2;
    pub const RAGING: i32 = 3;
    pub const RANDOM: i32 = 4;
}

/// Landform / landmass balance slider values. **E+C** (globals `WorldLandmass`).
pub mod landform {
    pub const ARCHIPELAGO: i32 = 0;
    pub const CONTINENTS: i32 = 1;
    pub const PANGAEA: i32 = 2;
    pub const RANDOM: i32 = 3;
}

/// Ocean coverage slider values (% of map). **E+C**
pub mod ocean {
    pub const OCEANS_80: i32 = 0;
    pub const OCEANS_70: i32 = 1;
    pub const OCEANS_60: i32 = 2;
    pub const RANDOM: i32 = 3;
}

/// Temperature slider values. **E+C**
pub mod temperature {
    pub const COOL: i32 = 0;
    pub const TEMPERATE: i32 = 1;
    pub const WARM: i32 = 2;
    pub const RANDOM: i32 = 3;
}

/// World age slider values (billions of years). **E+C**
pub mod age {
    pub const THREE_BN: i32 = 0;
    pub const FOUR_BN: i32 = 1;
    pub const FIVE_BN: i32 = 2;
    pub const RANDOM: i32 = 3;
}

fixed_record! {
    /// One row of world characteristics for a scenario or rules set.
    pub struct WorldCharacteristics(b"WCHR") {
        /// Body `+0x00`. Selected climate (`WorldAridity` @ `0x9C7370`; A).
        pub climate_selected: i32,
        /// Body `+0x04`. Resolved climate (`ActualWorldAridity` @ `0x9C7374`; A).
        pub climate_actual: i32,
        /// Body `+0x08`. Selected barbarian level (`BarbarianActivity` @ `0x9C7378`; A).
        pub barbarian_activity_selected: i32,
        /// Body `+0x0C`. Resolved barbarians (`ActualBarbarianActivity` @ `0x9C737C`; A).
        pub barbarian_activity_actual: i32,
        /// Body `+0x10`. Selected landform (`WorldLandmass` @ `0x9C7380`; A).
        pub landform_selected: i32,
        /// Body `+0x14`. Resolved landform (`ActualWorldLandmass` @ `0x9C7384`; A).
        pub landform_actual: i32,
        /// Body `+0x18`. Selected ocean coverage (`WorldOceanCoverage` @ `0x9C7388`; A).
        pub ocean_coverage_selected: i32,
        /// Body `+0x1C`. Resolved oceans (`ActualWorldOceanCoverage` @ `0x9C738C`; A).
        pub ocean_coverage_actual: i32,
        /// Body `+0x20`. Selected temperature (`WorldTemperature` @ `0x9C7390`; A).
        pub temperature_selected: i32,
        /// Body `+0x24`. Resolved temperature (`ActualWorldTemperature` @ `0x9C7394`; A).
        pub temperature_actual: i32,
        /// Body `+0x28`. Selected world age (`WorldAge` @ `0x9C7398`; A).
        pub age_selected: i32,
        /// Body `+0x2C`. Resolved age (`ActualWorldAge` @ `0x9C739C`; A).
        pub age_actual: i32,
        /// Body `+0x30`. Index into [`crate::sections::wsiz::WorldSize`] rows
        /// (`WorldSize` @ `0x9C73A0`; A/C — Mesopotamia `2` → preset *Standard*
        /// while [`crate::sections::wmap::WorldMap::width`] may differ).
        pub world_size_index: i32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::{check_roundtrip, files, load_all, rows};
    use crate::io::{Reader, Record};

    #[test]
    fn corpus_roundtrip() {
        if !crate::corpus::available() {
            return;
        }
        let st = check_roundtrip::<WorldCharacteristics>();
        assert_eq!(st.with_extra, 0);
        assert_eq!(st.short, 0);
        assert_eq!(st.lengths, vec![52]);
    }

    #[test]
    fn mesopotamia_sliders_and_size_index() {
        let files: Vec<_> = load_all().into_iter().filter_map(|f| f.ok()).collect();
        if files.is_empty() {
            return;
        }
        let file = files
            .iter()
            .find(|f| f.name().contains("Mesopotamia") && !f.name().contains("MP_"))
            .expect("Mesopotamia scenario");
        let row = rows::<WorldCharacteristics>(std::slice::from_ref(file))
            .into_iter()
            .next()
            .expect("WCHR row");
        let w = WorldCharacteristics::read(&mut Reader::new(row.body), &file.ctx()).unwrap();
        // Fixed climate/temperature/age; restless barbs rolled down to roaming.
        assert_eq!(w.climate_selected, climate::NORMAL);
        assert_eq!(w.climate_actual, climate::NORMAL);
        assert_eq!(w.barbarian_activity_selected, barbarian::RESTLESS);
        assert_eq!(w.barbarian_activity_actual, barbarian::ROAMING);
        assert_eq!(w.landform_selected, landform::CONTINENTS);
        assert_eq!(w.world_size_index, 2);
    }

    #[test]
    fn rise_of_rome_large_world_preset() {
        let files: Vec<_> = load_all().into_iter().filter_map(|f| f.ok()).collect();
        if files.is_empty() {
            return;
        }
        let Some(file) = files.iter().find(|f| f.name().contains("Rise_of_Rome")) else {
            return;
        };
        let w = WorldCharacteristics::read(
            &mut Reader::new(rows::<WorldCharacteristics>(std::slice::from_ref(file))[0].body),
            &file.ctx(),
        )
        .unwrap();
        assert_eq!(w.world_size_index, 3);
        let preset = crate::sections::wsiz::WorldSize::read(
            &mut Reader::new(
                rows::<crate::sections::wsiz::WorldSize>(std::slice::from_ref(file))
                    [w.world_size_index as usize]
                    .body,
            ),
            &file.ctx(),
        )
        .unwrap();
        assert_eq!(preset.name.text(), "Large");
        assert_eq!(preset.width, 140);
    }

    #[test]
    fn random_barbarians_resolves_in_intro2() {
        let files: Vec<_> = load_all().into_iter().filter_map(|f| f.ok()).collect();
        if files.is_empty() {
            return;
        }
        let Some(file) = files
            .iter()
            .find(|f| f.name().contains("Intro2_The_Three_Sisters"))
        else {
            return;
        };
        let w = WorldCharacteristics::read(
            &mut Reader::new(rows::<WorldCharacteristics>(std::slice::from_ref(file))[0].body),
            &file.ctx(),
        )
        .unwrap();
        assert_eq!(w.barbarian_activity_selected, barbarian::RANDOM);
        assert_eq!(w.barbarian_activity_actual, barbarian::RESTLESS);
    }

    #[test]
    fn world_size_index_matches_wsiz_row() {
        let files = files();
        let Some(file) = files
            .iter()
            .find(|f| f.name().contains("Mesopotamia") && !f.name().contains("MP_"))
        else {
            return;
        };
        let wchr = rows::<WorldCharacteristics>(std::slice::from_ref(file))
            .into_iter()
            .next()
            .unwrap();
        let w = WorldCharacteristics::read(&mut Reader::new(wchr.body), &file.ctx()).unwrap();
        let wsiz = rows::<crate::sections::wsiz::WorldSize>(std::slice::from_ref(file));
        let idx = w.world_size_index as usize;
        assert!(idx < wsiz.len());
        let preset =
            crate::sections::wsiz::WorldSize::read(&mut Reader::new(wsiz[idx].body), &file.ctx())
                .unwrap();
        assert_eq!(preset.name.text(), "Standard");
        assert_eq!(preset.width, 100);
        assert_eq!(preset.height, 100);
    }
}
