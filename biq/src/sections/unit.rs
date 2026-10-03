//! `UNIT` — pre-placed units on the scenario map.
//!
//! Loader worker `0x596DE0` (arm `0x594AD3`), constructor `0x5EAD40`, row
//! reader `0x5EADA0`, writer `0x5EAF00` (via `0x597CA0`). Game-start placement
//! `0x5D2D4B..0x5D3024` (see below). Editor *Units* page (dialog **189**, class
//! at `0x458200`): load `0x458BE0`, save `0x458A70`, DDX `0x458290`.
//!
//! Present from PTW `11.xx` as fixed **121**-byte rows (`name[32]`, five
//! dwords, `x`, `y`, `custom_name[57]`, one dword). The loader reads each field
//! only while bytes remain in the row. The corpus has `UNIT` rows from `11.09`
//! on, all 121 bytes; the two last fields are gated by the versions that added
//! them (`11.07` the custom name, `11.08` the king flag), so a file older than
//! that writes a 60- or 117-byte row (HYPOTHESIS: never observed).
//!
//! # Row layout
//!
//! | body | mem | field | evidence |
//! |---|---|---|---|
//! | `0..32` | `+0x00` | [`legacy_name`](Unit::legacy_name) | **A** `0x596E96`: for files older than `11.07` the loader moves this string into `custom_name` and blanks it. Zero in all 5877 corpus rows. |
//! | `32` | `+0x20` | [`owner_type`](Unit::owner_type) | **A+B** see [`crate::owner`]; ctor default `1` |
//! | `36` | `+0x24` | [`experience_level`](Unit::experience_level) | **A** `0x5D2FEF`: copied to the created unit's experience (`unit+0x44`). **B** editor *Experience:* combo 1768 lists `EXPR`; ctor default `1` |
//! | `40` | `+0x28` | [`owner`](Unit::owner) | **A+B** see [`crate::owner`]; ctor default `75` |
//! | `44` | `+0x2C` | [`unit_type`](Unit::unit_type) | **A** `0x5D2E8A` `PRTO` index (`-1` skips the row); editor shows the type in the group-box title; ctor default `0` |
//! | `48` | `+0x30` | [`ai_strategy`](Unit::ai_strategy) | **A** `0x5D2E96`: `-1` lets the game pick, else the bit index tested against the `PRTO` strategy masks of the type's variants (see below); **B** *AI Strategy:* combo 1775 item data; ctor `-1` |
//! | `52`, `56` | `+0x34`, `+0x38` | [`map_x`](Unit::map_x), [`map_y`](Unit::map_y) | **A** `0x5D2FA3..0x5D2FB0`; ctor `-1` |
//! | `60..117` | `+0x3C` | [`custom_name`](Unit::custom_name) | **A** `0x5D2FD4` copies it to the unit's name (`unit+0x74`); **B** *Name:* edit 1773, limit 56 |
//! | `117` | `+0x78` | [`use_civ_king_unit`](Unit::use_civ_king_unit) | **A** `0x5D2E4C`: when set, `unit_type` is replaced by the owner civilization's [`king_unit`](super::race::Civilization::king_unit) (`RACE` tail `+0x48`, memory `+0x95C`, read at `0x5D2E80`); **B** checkbox 1853 *Use Civ-Specific King Unit* (enabled only for types with the *King* ability, `PRTO` bit 29) |
//!
//! # Game-start placement (`0x5D2D4B`)
//!
//! The game shuffles the unit list, then for each row resolves the owner by
//! [`owner_type`](Unit::owner_type) (barbarians: player slot 0 plus tribe
//! `owner`; civilization: the player running that `RACE`; player: slot
//! `LEAD index + 1`), skips owners that are not active, and calls the unit
//! factory `0x5694D0(player, type, x, y, tribe, ...)`.
//!
//! # Strategy variants (`0x5D2E96..0x5D2F8D`, **A**)
//!
//! `unit_type` names the *primary* prototype. A prototype whose
//! [`alt_strategy_of`](crate::sections::prto::UnitType::alt_strategy_of) names
//! it is an *alternative strategy* variant of the same unit (for example the
//! defensive twin of an offensive unit). With `ai_strategy == -1` the game
//! picks uniformly among the primary and its variants (`0x60BAB0`, index `0` =
//! primary, `k` = the `k`-th variant by index). With a strategy bit set it
//! scans prototypes from `unit_type` upward and creates the first one that is
//! the primary or a variant of it *and* whose
//! [`ai_strategies`](crate::sections::prto::UnitType::ai_strategies) has that
//! bit; if there is none the row is silently skipped.
//! [`Unit::instantiated_types`] reproduces this; every shipped unit with a
//! strategy resolves (5 386 of 5 386).
//!
//! Earlier revisions of this crate mislabelled body `+36` as the owner and
//! `+44`/`+48` as hit points; the corpus (`Rise_of_Rome` Hannibal =
//! `(2, 2, 8, 14, 4)` = civilization, Veteran, Carthage, *Army*, *Army* AI) and the code
//! above settle the order.

use crate::fixed_record;
use crate::io::Str;
use crate::owner::Owner;
use crate::sections::prto::UnitType;

/// Editor *AI Strategy* combo value: let the game choose (`0x5D2E96`).
pub const AI_STRATEGY_AUTO: i32 = -1;
/// `owner` value the constructor stores for new rows (`0x5EAD78`).
pub const DEFAULT_TRIBE: i32 = 75;

fixed_record! {
    /// One pre-placed unit. Row body **121** bytes.
    pub struct Unit(b"UNIT") {
        /// Body `+0`. Legacy 32-byte name, superseded by [`custom_name`](Unit::custom_name) in `11.07`; always empty.
        pub legacy_name: Str<32>,
        /// Body `+32`. `0` none, `1` barbarian tribe, `2` civilization, `3` player; see [`crate::owner`].
        pub owner_type: i32,
        /// Body `+36`. `EXPR` row index (`1` = Regular in the stock rules).
        pub experience_level: i32,
        /// Body `+40`. Tribe / `RACE` / `LEAD` index according to [`owner_type`](Unit::owner_type).
        pub owner: i32,
        /// Body `+44`. `PRTO` row index.
        pub unit_type: i32,
        /// Body `+48`. AI strategy bit index (`0` Offense ... `19` King, see `prto::ai`), `-1` = automatic.
        pub ai_strategy: i32,
        /// Body `+52`. Map X (staggered grid: `x + y` is even).
        pub map_x: i32,
        /// Body `+56`. Map Y.
        pub map_y: i32,
    }
    since (11, 7) {
        /// Body `+60..117`. Name given to the unit (≤ 56 characters). Added with `11.07`
        /// (**A** the loader fix-up `0x596E96` for older files, **B** Civ3XEdit 2.07).
        pub custom_name: Str<57>,
    }
    since (11, 8) {
        /// Body `+117`. Non-zero: replace `unit_type` with the owner's king unit at game start.
        /// Added with `11.08` (**B** Civ3XEdit 2.08, "civ-specific king" check box). Always `0`
        /// in the corpus.
        pub use_civ_king_unit: u32,
    }
}

impl Unit {
    /// The decoded owner, `None` for an unknown `owner_type`.
    pub fn owner(&self) -> Option<Owner> {
        Owner::from_raw(self.owner_type, self.owner)
    }

    /// A new row with the constructor defaults of `0x5EAD40` and the given type, owner and position.
    pub fn new(unit_type: i32, owner: Owner, map_x: i32, map_y: i32) -> Unit {
        let (owner_type, owner) = owner.to_raw();
        Unit {
            owner_type,
            experience_level: 1,
            owner,
            unit_type,
            ai_strategy: AI_STRATEGY_AUTO,
            map_x,
            map_y,
            ..Unit::default()
        }
    }

    /// The name the unit gets in the game: [`custom_name`](Unit::custom_name), or for a
    /// file older than `11.07` the [`legacy_name`](Unit::legacy_name) the loader moves
    /// there (`0x596E96`). Empty when the unit is unnamed.
    pub fn name(&self) -> std::borrow::Cow<'_, str> {
        let custom = self.custom_name.text();
        if custom.is_empty() {
            self.legacy_name.text()
        } else {
            custom
        }
    }

    /// `true` when the game replaces the unit type by the owner's king unit.
    pub fn uses_civ_king_unit(&self) -> bool {
        self.use_civ_king_unit != 0
    }

    /// The `PRTO` rows the game may create for this row (`0x5D2E96..0x5D2F8D`,
    /// see the module docs): the first primary-or-variant prototype offering a
    /// forced [`ai_strategy`](Unit::ai_strategy) (empty when none does, in
    /// which case the game skips the unit), or the primary followed by all its
    /// variants for the automatic strategy (the game picks one at random).
    pub fn instantiated_types(&self, prototypes: &[UnitType]) -> Vec<usize> {
        let Ok(primary) = usize::try_from(self.unit_type) else {
            return Vec::new();
        };
        if primary >= prototypes.len() {
            return Vec::new();
        }
        let is_variant = |p: usize| p == primary || prototypes[p].alt_strategy_of == self.unit_type;
        let mut candidates = (primary..prototypes.len()).filter(|&p| is_variant(p));
        if self.ai_strategy == AI_STRATEGY_AUTO {
            return candidates.collect();
        }
        let bit = 1u32 << (self.ai_strategy as u32 & 31);
        candidates
            .find(|&p| prototypes[p].ai_strategies & bit != 0)
            .into_iter()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus;
    use crate::owner;

    #[test]
    fn corpus_roundtrip() {
        if !crate::corpus::available() {
            return;
        }
        let st = corpus::check_roundtrip::<Unit>();
        assert_eq!(st.with_extra, 0);
        assert_eq!(st.short, 0);
        assert_eq!(st.lengths, vec![121]);
    }

    /// The two last fields exist only in files new enough to have them, and a row
    /// of the older shapes reads and writes back unchanged.
    #[test]
    fn older_files_write_shorter_rows() {
        use crate::Version;
        use crate::io::{Ctx, Reader, Record, Writer};
        let mut u = Unit::new(3, Owner::Civilization(2), 4, 6);
        u.custom_name = Str::new("Hannibal");
        u.use_civ_king_unit = 1;
        for (version, len) in [
            (Version::new(11, 5), 60),
            (Version::new(11, 7), 117),
            (Version::new(11, 8), 121),
            (Version::new(12, 8), 121),
        ] {
            let ctx = Ctx { version };
            let mut w = Writer::new();
            u.write(&mut w, &ctx);
            assert_eq!(w.buf.len(), len, "{version}");
            let back = Unit::read(&mut Reader::new(&w.buf), &ctx).unwrap();
            let mut again = Writer::new();
            back.write(&mut again, &ctx);
            assert_eq!(again.buf, w.buf, "{version}");
            assert!(back.extra.is_empty());
            assert_eq!(back.use_civ_king_unit, u32::from(len == 121), "{version}");
            assert_eq!(back.name(), if len == 60 { "" } else { "Hannibal" });
        }
        // A pre-11.07 row names the unit in the first 32 bytes.
        let mut old = Unit::new(3, Owner::Civilization(2), 4, 6);
        old.legacy_name = Str::new("Scipio");
        assert_eq!(old.name(), "Scipio");
    }

    #[test]
    fn new_matches_constructor_defaults() {
        let u = Unit::new(3, Owner::BarbarianTribe(DEFAULT_TRIBE), 4, 6);
        assert_eq!(u.owner_type, owner::BARBARIAN_TRIBE);
        assert_eq!(u.owner, 75);
        assert_eq!((u.experience_level, u.ai_strategy), (1, -1));
        assert_eq!(u.owner(), Some(Owner::BarbarianTribe(75)));
    }

    /// Every stored field is within its table in every corpus file, and every
    /// unit with a strategy resolves to a prototype the way the game's
    /// placement code does (**A** `0x5D2E96`).
    #[test]
    fn corpus_fields_index_their_tables() {
        if !crate::corpus::available() {
            return;
        }
        use crate::Biq;
        let mut resolved = 0;
        let mut variants_used = 0;
        for file in corpus::files() {
            let biq = Biq::from_raw(&file.raw).unwrap();
            let units = &biq.scenario.units;
            let protos = &biq.rules.unit_types;
            let n_expr = biq.rules.experience_levels.len() as i32;
            let n_prto = protos.len() as i32;
            for u in units {
                assert!(
                    u.owner().is_some(),
                    "{} owner_type {}",
                    file.name(),
                    u.owner_type
                );
                assert!(u.legacy_name.text().is_empty());
                assert!(u.map_x >= 0 && u.map_y >= 0, "{}", file.name());
                assert!((-1..=19).contains(&u.ai_strategy), "{}", file.name());
                // Rules-less scenarios index the built-in rules.
                if n_expr > 0 {
                    assert!((0..n_expr).contains(&u.experience_level), "{}", file.name());
                }
                if n_prto == 0 || u.use_civ_king_unit != 0 {
                    continue;
                }
                assert!((0..n_prto).contains(&u.unit_type), "{}", file.name());
                let made = u.instantiated_types(protos);
                assert!(
                    !made.is_empty(),
                    "{} {}: strategy {} has no variant of the type",
                    file.name(),
                    protos[u.unit_type as usize].name.text(),
                    u.ai_strategy
                );
                if u.ai_strategy >= 0 {
                    resolved += 1;
                    assert_eq!(made.len(), 1);
                    if made[0] != u.unit_type as usize {
                        variants_used += 1;
                        assert_eq!(protos[made[0]].alt_strategy_of, u.unit_type);
                    }
                }
            }
        }
        assert!(resolved > 5000, "{resolved} units with a strategy");
        assert!(variants_used > 100, "{variants_used} resolved to a variant");
    }
}
