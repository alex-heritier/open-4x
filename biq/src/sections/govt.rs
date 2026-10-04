//! `GOVT` — government types.
//!
//! Dispatcher arm `0x594CFD`, worker `0x595AE0`, row reader `0x5E3E80`, writer
//! `0x5E3B20`, constructor `0x5E3A20`. The editor page is dialog **155**
//! “Governments”; its apply routine (`0x438E30`..`0x439338`) copies every bound
//! control into the row, which is how the labels below were established.
//!
//! The in-memory object is `0x1E8` bytes; on disk the rows are the same fields
//! in a different order. Row length is **`396 + 12·n + tail`** bytes (`n` = number
//! of governments in the file, `tail` = 68 B before Conquests 12.06, 76 B from
//! 12.06).
//!
//! | body offset | memory | field |
//! |---|---|---|
//! | `0x000` | `+0x0C` | [`default_type`](Government::default_type) |
//! | `0x004` | `+0x10` | [`transition_type`](Government::transition_type) |
//! | `0x008` | `+0x14` | [`requires_maintenance`](Government::requires_maintenance) |
//! | `0x00C` | `+0x18` | [`unused_0x0c`](Government::unused_0x0c) |
//! | `0x010` | `+0x1C` | [`standard_tile_penalty`](Government::standard_tile_penalty) |
//! | `0x014` | `+0x20` | [`standard_trade_bonus`](Government::standard_trade_bonus) |
//! | `0x018` | `+0x2C` | name (64 B) |
//! | `0x058` | `+0x6C` | Civilopedia key (32 B) |
//! | `0x078` | `+0x8C` | four male/female ruler-title pairs, 8 × 32 B |
//! | `0x178` | `+0x18C` | [`corruption_and_waste`](Government::corruption_and_waste) |
//! | `0x17C` | `+0x190` | [`immune_to`](Government::immune_to) |
//! | `0x180` | `+0x194` | [`diplomats_are`](Government::diplomats_are) |
//! | `0x184` | `+0x198` | [`spies_are`](Government::spies_are) |
//! | `0x188` | `+0x1B0` | `n` ([`governments_in_file`](Government::governments_in_file)) |
//! | `0x18C` | `[+0x19C]` | `n` × 12 B [`VsGovernment`] |
//! | `V+0x00` | `+0x1A0` | [`hurrying_production`](Government::hurrying_production) |
//! | `V+0x04` | `+0x1A4` | [`assimilation_chance`](Government::assimilation_chance) |
//! | `V+0x08` | `+0x1A8` | [`draft_limit`](Government::draft_limit) |
//! | `V+0x0C` | `+0x1AC` | [`military_police_limit`](Government::military_police_limit) |
//! | `V+0x10` | `+0x1B4` | [`ruler_title_count`](Government::ruler_title_count) |
//! | `V+0x14` | `+0x1B8` | [`prerequisite_tech`](Government::prerequisite_tech) |
//! | `V+0x18` | `+0x1BC` | [`rate_cap`](Government::rate_cap) |
//! | `V+0x1C` | `+0x1C0` | [`worker_rate`](Government::worker_rate) |
//! | `V+0x20` | `+0x1C4` | [`unknown_0x1c4`](Government::unknown_0x1c4) (3 dwords) |
//! | `V+0x2C` | `+0x1D0` | [`free_units`](Government::free_units) |
//! | `V+0x30` | `+0x1D4` | [`free_units_per_town`](Government::free_units_per_town) |
//! | `V+0x34` | `+0x1D8` | [`free_units_per_city`](Government::free_units_per_city) |
//! | `V+0x38` | `+0x1DC` | [`free_units_per_metropolis`](Government::free_units_per_metropolis) |
//! | `V+0x3C` | `+0x1E0` | [`cost_per_unit`](Government::cost_per_unit) |
//! | `V+0x40` | `+0x1E4` | [`war_weariness`](Government::war_weariness) |
//! | `V+0x44` | `+0x24` | [`xenophobic`](Government::xenophobic) (12.06+) |
//! | `V+0x48` | `+0x28` | [`forced_resettlement`](Government::forced_resettlement) (12.06+) |
//!
//! where `V = 0x18C + 12·n` is the end of the relation table.

use crate::Version;
use crate::io::{Ctx, Field, Reader, Record, Result, Str, Writer};

/// The first version whose rows carry [`Government::xenophobic`] and
/// [`Government::forced_resettlement`]: the first Conquests version in the corpus (the
/// Conquests editor's notes give no numbers).
pub const CONQUESTS_SINCE: Version = Version::new(12, 6);

/// One government's relation to another government of the same file (12 bytes).
///
/// The table lives in the *acting* government's row, indexed by the other
/// government's row number. The Governments page edits two of the three
/// dwords through the “Propaganda Modifier vs.” and “Resistance Modifier vs.”
/// controls (apply routine `0x438E30`, **B**).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct VsGovernment {
    /// Never initialised: the constructor (`0x5E39EA`) clears only the other
    /// two dwords, the editor has no control for it and no code reads it. Every
    /// stock file holds the allocator's `0xCDCDCDCD` fill here.
    pub unused: i32,
    /// “Propaganda Modifier vs.” (percentage points; the editor clamps to
    /// ±100). Added to the success chance of a propaganda attempt (**A**:
    /// `0x5280D4`, the `+4` element of the table of the acting player's
    /// government, indexed by the target city owner's government).
    pub propaganda_modifier: i32,
    /// “Resistance Modifier vs.” (clamped to ±100 by the editor). Added to the
    /// percentage tested against `rand(100)` when the game decides whether a
    /// city starts resisting (**A**: `0x4ABFB9`; first index = government of
    /// the city's present owner, second = government of the player resolved
    /// from `city+0x140`).
    pub resistance_modifier: i32,
}

impl Field for VsGovernment {
    const SIZE: usize = 12;
    fn zero() -> Self {
        Self::default()
    }
    fn read(r: &mut Reader<'_>) -> Self {
        Self {
            unused: r.i32().unwrap_or(0),
            propaganda_modifier: r.i32().unwrap_or(0),
            resistance_modifier: r.i32().unwrap_or(0),
        }
    }
    fn write(&self, w: &mut Writer) {
        w.i32(self.unused);
        w.i32(self.propaganda_modifier);
        w.i32(self.resistance_modifier);
    }
}

/// One ruler title: the form of address for a male and for a female leader.
///
/// The editor stores the pair as `male/female` in a single text box
/// (`0x4805C0` splits it at the first `/`); the game picks the pair at random
/// (`rand % ruler_title_count`, **A** `0x53AA50`) and the slot by gender.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct RulerTitle {
    /// Title for a male leader (`Sir`).
    pub male: Str<32>,
    /// Title for a female leader (`Madame`); empty when the title is gender neutral.
    pub female: Str<32>,
}

impl Field for RulerTitle {
    const SIZE: usize = 64;
    fn zero() -> Self {
        Self::default()
    }
    fn read(r: &mut Reader<'_>) -> Self {
        Self {
            male: r.str::<32>().unwrap_or_default(),
            female: r.str::<32>().unwrap_or_default(),
        }
    }
    fn write(&self, w: &mut Writer) {
        Field::write(&self.male, w);
        Field::write(&self.female, w);
    }
}

/// One government type (`Anarchy`, `Despotism`, …).
#[derive(Clone, Debug, PartialEq)]
pub struct Government {
    /// “Default Type” flag (`1` on Despotism only in `conquests.biq`: the
    /// government new civilizations start under; **B**).
    pub default_type: i32,
    /// “Transition Type” flag (`1` on Anarchy only: the government used while
    /// switching; **B**).
    pub transition_type: i32,
    /// “Requires Maintenance” flag: improvements of this government's cities
    /// cost upkeep (`0` on Anarchy; **B**, **A** memory `+0x14`).
    pub requires_maintenance: i32,
    /// Not exposed by the editor, not initialised by the constructor and not
    /// read by any code found. Stock Conquests holds `1` on Anarchy …
    /// Communism, `0` on Republic and Democracy and the debug-heap fill
    /// `0xCCCCCCCC` on Fascism and Feudalism.
    pub unused_0x0c: i32,
    /// “Standard Tile Penalty” flag: tiles producing more than two of anything
    /// lose one (`1` on Anarchy and Despotism; **B**).
    pub standard_tile_penalty: i32,
    /// “Standard Trade Bonus” flag (`1` on Republic and Democracy; **B**).
    pub standard_trade_bonus: i32,
    /// Display name (64 B).
    pub name: Str<64>,
    /// Civilopedia key (`GOVT_Despotism`, 32 B).
    pub civilopedia_entry: Str<32>,
    /// Four ruler titles; only the first [`ruler_title_count`](Self::ruler_title_count)
    /// are used (this is the 256-byte block older notes call “description”).
    pub ruler_titles: [RulerTitle; 4],
    /// “Corruption and Waste” radio: `0` Minimal, `1` Nuisance, `2` Problematic,
    /// `3` Rampant, `4` Catastrophic, `5` Communal (**B**; `3` on Despotism, `0`
    /// on Democracy).
    pub corruption_and_waste: i32,
    /// “Immune to” (Espionage group): row of the [`EspionageMission`] this
    /// government cannot be targeted by, `-1` none. Democracy `6` = *Initiate
    /// Propaganda*. Consumers compare it with the mission being attempted
    /// (**A**: `0x445229`, `0x44529C`, `0x44530E`, …).
    ///
    /// [`EspionageMission`]: crate::sections::espn::EspionageMission
    pub immune_to: i32,
    /// “Diplomats Are” combo: row of the [`ExperienceLevel`] new diplomats
    /// start at (`1` Regular everywhere in stock; **B**).
    ///
    /// [`ExperienceLevel`]: crate::sections::expr::ExperienceLevel
    pub diplomats_are: i32,
    /// “Spies Are” combo: experience level new spies start at (`2` Veteran on
    /// Communism and Fascism, `1` elsewhere; **A** `0x52A062` indexes `EXPR`).
    pub spies_are: i32,
    /// Count of governments in this file; always `n` (section row count).
    pub governments_in_file: u32,
    /// Relations toward each government in the file, by row number.
    pub vs: Vec<VsGovernment>,
    /// “Hurrying Production” radio: `0` Cannot hurry, `1` Forced labor, `2` Paid
    /// labor (**A**: `0x43436D` tests `== 1`, `0x434BE6` tests `== 2`).
    pub hurrying_production: i32,
    /// “Assimilation Chance” in percent (0–100; **A**: `0x4AC23A` compares it
    /// with `rand(100)`).
    pub assimilation_chance: i32,
    /// “Draft Limit” (0–255; **B**).
    pub draft_limit: i32,
    /// “Military Police Limit” (0–100; **B**; `4` on Communism and Fascism).
    pub military_police_limit: i32,
    /// Number of used entries of [`ruler_titles`](Self::ruler_titles) (rebuilt
    /// by the editor from the non-empty “Ruler Titles” boxes; **A** `0x53AA68`
    /// uses it as the modulus when picking a title).
    pub ruler_title_count: i32,
    /// “Prerequisite” advance: row of `TECH`, `-1` for none (the editor forces
    /// `-1` on the default and the transition government; **B**). Monarchy 19 =
    /// *Monarchy*, Democracy 34 = *Democracy*.
    pub prerequisite_tech: i32,
    /// “Rate Cap” in tenths (5–10, `10` = 100%; **B**).
    pub rate_cap: i32,
    /// “Worker Rate” (1–20; **B**; constructor default `2`).
    pub worker_rate: i32,
    /// Three dwords the editor never writes and no code reads. Stock values:
    /// `(0,0,0)`, `(-1,0,0)` (Despotism, Communism), `(1,1,0)` (Republic,
    /// Democracy), and debug-heap fill on Fascism and Feudalism.
    pub unknown_0x1c4: [i32; 3],
    /// “Free Units”: units supported for free; `-1` is the “All Units Free”
    /// checkbox (**B**, clamped to −1…1000).
    pub free_units: i32,
    /// “Free Units Per: Town” (−100…100, **B**).
    pub free_units_per_town: i32,
    /// “Free Units Per: City” (−100…100, **B**).
    pub free_units_per_city: i32,
    /// “Free Units Per: Metropolis” (−100…100, **B**).
    pub free_units_per_metropolis: i32,
    /// “Cost/Unit”: gold per unit above the free allowance (0–1000, **B**).
    pub cost_per_unit: i32,
    /// “War Weariness” radio: `0` None, `1` Low, `2` High (**B**). Present in
    /// every generation.
    pub war_weariness: i32,
    /// “Xenophobic” flag (Conquests 12.06+; `1` on Fascism in the stock file; **B**).
    pub xenophobic: i32,
    /// “Forced Resettlement” flag (Conquests 12.06+; **B**).
    pub forced_resettlement: i32,
    /// Bytes past the modelled tail (none in any shipped file).
    pub extra: Vec<u8>,
}

impl Default for Government {
    /// The constructor `0x5E3A20`.
    fn default() -> Self {
        Self {
            default_type: 0,
            transition_type: 0,
            requires_maintenance: 0,
            unused_0x0c: 0,
            standard_tile_penalty: 0,
            standard_trade_bonus: 0,
            name: Str::default(),
            civilopedia_entry: Str::default(),
            ruler_titles: [RulerTitle::default(); 4],
            corruption_and_waste: 0,
            immune_to: -1,
            diplomats_are: 1,
            spies_are: 1,
            governments_in_file: 0,
            vs: Vec::new(),
            hurrying_production: 0,
            assimilation_chance: 0,
            draft_limit: 0,
            military_police_limit: 0,
            ruler_title_count: 0,
            prerequisite_tech: -1,
            rate_cap: 5,
            worker_rate: 2,
            unknown_0x1c4: [0; 3],
            free_units: 0,
            free_units_per_town: 0,
            free_units_per_city: 0,
            free_units_per_metropolis: 0,
            cost_per_unit: 0,
            war_weariness: 0,
            xenophobic: 0,
            forced_resettlement: 0,
            extra: Vec::new(),
        }
    }
}

impl Government {
    /// Bytes before the `vs` table: six flag dwords, the strings, the four
    /// espionage/corruption dwords and `governments_in_file`.
    pub const PREFIX: usize = 24 + 64 + 32 + 256 + 16 + 4;

    /// Trailing scalar block after `vs` (version-gated tail; not a hidden row length).
    pub fn tail_byte_len(ctx: &Ctx) -> usize {
        if ctx.version >= CONQUESTS_SINCE {
            76
        } else {
            68
        }
    }

    /// Ruler titles that are actually in use.
    pub fn used_ruler_titles(&self) -> &[RulerTitle] {
        let n = self.ruler_title_count.clamp(0, 4) as usize;
        &self.ruler_titles[..n]
    }

    fn read_tail(&mut self, r: &mut Reader<'_>, ctx: &Ctx) {
        macro_rules! i32f {
            ($f:expr) => {
                if r.remaining() >= 4 {
                    $f = r.i32().unwrap_or(0);
                }
            };
        }
        i32f!(self.hurrying_production);
        i32f!(self.assimilation_chance);
        i32f!(self.draft_limit);
        i32f!(self.military_police_limit);
        i32f!(self.ruler_title_count);
        i32f!(self.prerequisite_tech);
        i32f!(self.rate_cap);
        i32f!(self.worker_rate);
        for k in 0..3 {
            i32f!(self.unknown_0x1c4[k]);
        }
        i32f!(self.free_units);
        i32f!(self.free_units_per_town);
        i32f!(self.free_units_per_city);
        i32f!(self.free_units_per_metropolis);
        i32f!(self.cost_per_unit);
        i32f!(self.war_weariness);
        if ctx.version >= CONQUESTS_SINCE {
            i32f!(self.xenophobic);
            i32f!(self.forced_resettlement);
        }
    }

    fn write_tail(&self, w: &mut Writer, ctx: &Ctx) {
        w.i32(self.hurrying_production);
        w.i32(self.assimilation_chance);
        w.i32(self.draft_limit);
        w.i32(self.military_police_limit);
        w.i32(self.ruler_title_count);
        w.i32(self.prerequisite_tech);
        w.i32(self.rate_cap);
        w.i32(self.worker_rate);
        for v in self.unknown_0x1c4 {
            w.i32(v);
        }
        w.i32(self.free_units);
        w.i32(self.free_units_per_town);
        w.i32(self.free_units_per_city);
        w.i32(self.free_units_per_metropolis);
        w.i32(self.cost_per_unit);
        w.i32(self.war_weariness);
        if ctx.version >= CONQUESTS_SINCE {
            w.i32(self.xenophobic);
            w.i32(self.forced_resettlement);
        }
    }
}

impl Record for Government {
    const TAG: [u8; 4] = *b"GOVT";

    fn read(r: &mut Reader<'_>, ctx: &Ctx) -> Result<Self> {
        let mut g = Self::default();
        macro_rules! i32f {
            ($f:expr) => {
                if r.remaining() >= 4 {
                    $f = r.i32().unwrap_or(0);
                }
            };
        }
        i32f!(g.default_type);
        i32f!(g.transition_type);
        i32f!(g.requires_maintenance);
        i32f!(g.unused_0x0c);
        i32f!(g.standard_tile_penalty);
        i32f!(g.standard_trade_bonus);
        if let Some(v) = r.str::<64>() {
            g.name = v;
        }
        if let Some(v) = r.str::<32>() {
            g.civilopedia_entry = v;
        }
        if r.remaining() >= 256 {
            g.ruler_titles = <[RulerTitle; 4] as Field>::read(r);
        }
        i32f!(g.corruption_and_waste);
        i32f!(g.immune_to);
        i32f!(g.diplomats_are);
        i32f!(g.spies_are);
        let Some(count) = r.u32() else {
            g.extra = r.rest().to_vec();
            return Ok(g);
        };
        g.governments_in_file = count;
        let need = count as usize * VsGovernment::SIZE;
        if r.remaining() < need {
            return Err(crate::io::Error::BadCount {
                tag: Self::TAG,
                what: "vs",
                count,
            });
        }
        g.vs = (0..count).map(|_| VsGovernment::read(r)).collect();
        g.read_tail(r, ctx);
        g.extra = r.rest().to_vec();
        Ok(g)
    }

    fn write(&self, w: &mut Writer, ctx: &Ctx) {
        w.i32(self.default_type);
        w.i32(self.transition_type);
        w.i32(self.requires_maintenance);
        w.i32(self.unused_0x0c);
        w.i32(self.standard_tile_penalty);
        w.i32(self.standard_trade_bonus);
        Field::write(&self.name, w);
        Field::write(&self.civilopedia_entry, w);
        Field::write(&self.ruler_titles, w);
        w.i32(self.corruption_and_waste);
        w.i32(self.immune_to);
        w.i32(self.diplomats_are);
        w.i32(self.spies_are);
        w.u32(self.governments_in_file);
        for e in &self.vs {
            e.write(w);
        }
        self.write_tail(w, ctx);
        w.bytes(&self.extra);
    }

    fn extra(&self) -> &[u8] {
        &self.extra
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::files;
    use crate::io::Reader;

    fn conquests() -> Option<crate::Biq> {
        let f = files()
            .into_iter()
            .find(|f| f.name().ends_with("Conquests/conquests.biq"))?;
        crate::Biq::from_raw(&f.raw).ok()
    }

    fn by_name<'a>(rows: &'a [Government], name: &str) -> &'a Government {
        rows.iter()
            .find(|g| g.name.text() == name)
            .unwrap_or_else(|| panic!("missing GOVT {name}"))
    }

    #[test]
    fn corpus_roundtrip() {
        let st = crate::corpus::check_roundtrip::<Government>();
        assert_eq!(st.with_extra, 0, "{}", st.report());
    }

    #[test]
    fn vs_table_len_equals_section() {
        let files = files();
        if files.is_empty() {
            return;
        }
        for f in &files {
            let Some(sec) = f.raw.section(b"GOVT") else {
                continue;
            };
            let n = sec.rows.len();
            for row in &sec.rows {
                let mut r = Reader::new(f.raw.row(row));
                let g = Government::read(&mut r, &f.ctx()).unwrap();
                assert_eq!(g.vs.len(), n);
                assert_eq!(g.governments_in_file as usize, n);
            }
        }
    }

    #[test]
    fn row_length_formula() {
        let files = files();
        if files.is_empty() {
            return;
        }
        for f in &files {
            let Some(sec) = f.raw.section(b"GOVT") else {
                continue;
            };
            let n = sec.rows.len();
            let expect = Government::PREFIX + 12 * n + Government::tail_byte_len(&f.ctx());
            for row in &sec.rows {
                assert_eq!(f.raw.row(row).len(), expect, "{}", f.name());
            }
        }
    }

    #[test]
    fn flags_and_corruption_in_conquests() {
        let Some(biq) = conquests() else { return };
        let rows = &biq.rules.governments;
        assert_eq!(rows.len(), 8);
        // Exactly one default and one transition government.
        let defaults: Vec<_> = rows
            .iter()
            .filter(|g| g.default_type == 1)
            .map(|g| g.name.text().into_owned())
            .collect();
        assert_eq!(defaults, ["Despotism"]);
        let transitions: Vec<_> = rows
            .iter()
            .filter(|g| g.transition_type == 1)
            .map(|g| g.name.text().into_owned())
            .collect();
        assert_eq!(transitions, ["Anarchy"]);

        let anarchy = by_name(rows, "Anarchy");
        assert_eq!(anarchy.requires_maintenance, 0);
        assert_eq!(anarchy.standard_tile_penalty, 1);
        assert_eq!(anarchy.corruption_and_waste, 4);
        let despotism = by_name(rows, "Despotism");
        assert_eq!(despotism.standard_tile_penalty, 1);
        assert_eq!(despotism.requires_maintenance, 1);
        assert_eq!(despotism.corruption_and_waste, 3);
        assert_eq!(by_name(rows, "Monarchy").standard_tile_penalty, 0);
        assert_eq!(by_name(rows, "Communism").corruption_and_waste, 5);
        for n in ["Republic", "Democracy"] {
            assert_eq!(by_name(rows, n).standard_trade_bonus, 1, "{n}");
        }
        assert_eq!(by_name(rows, "Democracy").corruption_and_waste, 0);
        assert_eq!(
            rows.iter().filter(|g| g.standard_trade_bonus == 1).count(),
            2
        );
    }

    #[test]
    fn prerequisites_are_the_governments_own_advances() {
        let Some(biq) = conquests() else { return };
        let rows = &biq.rules.governments;
        for (gov, tech) in [
            ("Monarchy", "Monarchy"),
            ("Communism", "Communism"),
            ("Republic", "The Republic"),
            ("Democracy", "Democracy"),
            ("Fascism", "Fascism"),
            ("Feudalism", "Feudalism"),
        ] {
            let g = by_name(rows, gov);
            let t = &biq.rules.techs[g.prerequisite_tech as usize];
            assert_eq!(t.name.text(), tech, "{gov}");
        }
        assert_eq!(by_name(rows, "Anarchy").prerequisite_tech, -1);
        assert_eq!(by_name(rows, "Despotism").prerequisite_tech, -1);
    }

    #[test]
    fn espionage_settings_index_other_sections() {
        let Some(biq) = conquests() else { return };
        let rows = &biq.rules.governments;
        // Democracy cannot be the target of propaganda.
        let d = by_name(rows, "Democracy");
        let m = &biq.rules.espionage_missions[d.immune_to as usize];
        assert_eq!(m.name.text(), "Initiate Propaganda");
        assert!(rows.iter().filter(|g| g.immune_to >= 0).count() == 1);
        // Experience levels of diplomats and spies.
        let level = |i: i32| {
            biq.rules.experience_levels[i as usize]
                .name
                .text()
                .into_owned()
        };
        for g in rows {
            assert_eq!(level(g.diplomats_are), "Regular");
        }
        assert_eq!(level(by_name(rows, "Communism").spies_are), "Veteran");
        assert_eq!(level(by_name(rows, "Fascism").spies_are), "Veteran");
        assert_eq!(level(by_name(rows, "Republic").spies_are), "Regular");
    }

    #[test]
    fn ruler_titles() {
        let Some(biq) = conquests() else { return };
        let rows = &biq.rules.governments;
        let d = by_name(rows, "Despotism");
        assert_eq!(d.ruler_title_count, 4);
        let t = d.used_ruler_titles();
        assert_eq!(t.len(), 4);
        assert_eq!(t[0].male.text(), "Sir");
        assert_eq!(t[0].female.text(), "Madame");
        assert_eq!(t[2].female.text(), "Mistress");
        assert_eq!(t[3].male.text(), "Great One");
        // Gender neutral titles have an empty female form.
        let c = by_name(rows, "Communism");
        assert_eq!(c.ruler_title_count, 1);
        assert_eq!(c.ruler_titles[0].male.text(), "Comrade");
        assert!(c.ruler_titles[0].female.is_empty());
        // The count never exceeds the pairs actually filled in.
        for g in rows {
            let used = g.used_ruler_titles();
            assert!(used.iter().all(|t| !t.male.is_empty()), "{}", g.name);
            assert!(
                g.ruler_titles[used.len()..]
                    .iter()
                    .all(|t| t.male.is_empty()),
                "{}",
                g.name
            );
        }
    }

    #[test]
    fn support_costs_and_limits() {
        let Some(biq) = conquests() else { return };
        let rows = &biq.rules.governments;
        let a = by_name(rows, "Anarchy");
        assert_eq!(a.free_units, -1, "all units free");
        assert_eq!(a.hurrying_production, 0);
        let d = by_name(rows, "Despotism");
        assert_eq!(d.hurrying_production, 1);
        assert_eq!(
            (
                d.free_units_per_town,
                d.free_units_per_city,
                d.free_units_per_metropolis
            ),
            (4, 4, 4)
        );
        assert_eq!(by_name(rows, "Monarchy").hurrying_production, 2);
        let c = by_name(rows, "Communism");
        assert_eq!(c.military_police_limit, 4);
        assert_eq!(c.assimilation_chance, 4);
        assert_eq!(by_name(rows, "Democracy").war_weariness, 2);
        assert_eq!(by_name(rows, "Republic").war_weariness, 1);
        assert_eq!(by_name(rows, "Despotism").war_weariness, 0);
        for g in rows {
            assert_eq!(g.rate_cap, 10, "{}", g.name);
        }
        assert_eq!(by_name(rows, "Fascism").worker_rate, 4);
        assert_eq!(by_name(rows, "Anarchy").worker_rate, 1);
    }

    #[test]
    fn conquests_only_flags() {
        let Some(biq) = conquests() else { return };
        let rows = &biq.rules.governments;
        let f = by_name(rows, "Fascism");
        assert_eq!((f.xenophobic, f.forced_resettlement), (1, 1));
        assert_eq!(rows.iter().filter(|g| g.xenophobic == 1).count(), 1);
        assert_eq!(
            rows.iter().filter(|g| g.forced_resettlement == 1).count(),
            1
        );
    }

    #[test]
    fn relation_grid() {
        let Some(biq) = conquests() else { return };
        let rows = &biq.rules.governments;
        let d = by_name(rows, "Despotism");
        assert_eq!(d.vs[0].propaganda_modifier, 15);
        assert_eq!(d.vs[3].propaganda_modifier, -15);
        assert_eq!(d.vs[0].resistance_modifier, -5);
        assert_eq!(d.vs[5].resistance_modifier, 5);
        // The first dword of every triple is the allocator's fill pattern.
        for g in rows {
            assert!(
                g.vs.iter().all(|v| v.unused == 0xCDCD_CDCDu32 as i32),
                "{}",
                g.name
            );
        }
    }

    #[test]
    fn uninitialised_dwords_stay_verbatim() {
        let Some(biq) = conquests() else { return };
        let rows = &biq.rules.governments;
        // Fascism and Feudalism were added without the constructor running.
        for n in ["Fascism", "Feudalism"] {
            let g = by_name(rows, n);
            assert_eq!(g.unused_0x0c, 0xCCCC_CCCCu32 as i32, "{n}");
            assert_eq!(g.unknown_0x1c4, [0xCCCC_CCCCu32 as i32; 3], "{n}");
        }
        assert_eq!(by_name(rows, "Republic").unknown_0x1c4, [1, 1, 0]);
    }
}
