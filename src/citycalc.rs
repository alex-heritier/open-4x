//! One city's numbers for a turn: what its tiles give, what corruption and
//! waste take, what the buildings multiply, how the citizens feel and whether
//! the city riots. Every screen and every turn reads the same [`Totals`].
//!
//! The rules are the executable's where `reverse-engineering/` could read
//! them: the shield sum and its multiplier (`yields.md` 5.4), the commerce
//! split with its rates, multipliers and Wealth (`yields.md` 5.5, 5.6,
//! `civ3mapgen::city`), the moods (`happiness.md`, `civ3mapgen::happiness`)
//! and the effect of disorder (`yields.md` 5.3 to 5.5). The government and
//! everything else about the owner comes from [`crate::realm`].
//!
//! Corruption and waste are the executable's `0x4B1190` (`economy.md`,
//! "Corruption math"), checked against the binary run in the emulator.

use civ3mapgen::city as exe;
use civ3mapgen::economy as exe_econ;
use civ3mapgen::government::Govt;
use civ3mapgen::happiness::{self as hap, BuildingFaces, Citizen, mood};

use crate::cities::{City, Production, tile_commerce};
use crate::map::{Base, GameMap, Tile, yields};
use crate::realm::{self, Realm};
use crate::roster::{self, BLDG_COUNT, BldgDef, imp, oth, wonder};

/// Food each citizen eats (RULE "Food Consumption per Citizen").
pub const FOOD_PER_CITIZEN: i32 = 2;

/// RULE values the mood code reads (`happiness::shipped`).
const DRAFT_PENALTY: i32 = hap::shipped::DRAFT_TURN_PENALTY;
const HURRY_PENALTY: i32 = hap::shipped::HURRY_SACRIFICE_TURN_PENALTY;

/// The tile penalty (Anarchy, Despotism): a yield above two loses one.
/// Agricultural city-center food with freshwater is the sole exemption
/// (`yields.md` 4.1); shields and commerce are still capped.
fn trim(yield_: u8) -> i32 {
    trim_to(i32::from(yield_))
}

fn trim_to(yield_: i32) -> i32 {
    if yield_ > 2 { yield_ - 1 } else { yield_ }
}

/// Citizens by mood.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mood {
    pub happy: u8,
    pub content: u8,
    pub unhappy: u8,
    /// Idle citizens assigned to entertainment.
    pub entertainers: u8,
    pub scientists: u8,
    pub tax_collectors: u8,
}

/// The city's turn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Totals {
    /// Food the center and the worked tiles give.
    pub food: i32,
    /// Food the citizens eat; in disorder, all of it.
    pub eaten: i32,
    /// Food stored or lost each turn.
    pub surplus: i32,
    /// Shields before waste.
    pub shields_gross: i32,
    /// Shields lost to waste.
    pub waste: i32,
    /// Shields added to the box each turn, after waste and the
    /// buildings' multiplier.
    pub shields: i32,
    /// Commerce before corruption.
    pub commerce: i32,
    /// Commerce lost to corruption.
    pub corruption: i32,
    /// The three streams after the split, the buildings' multipliers and
    /// the specialists.
    pub tax: i32,
    pub sci: i32,
    pub lux: i32,
    /// Gold from shields spent on Wealth.
    pub wealth: i32,
    pub mood: Mood,
    pub disorder: bool,
}

/// The buildings in effect in a city: its own and the ones wonders give it,
/// minus those its owner's advances made obsolete.
pub fn active(city: &City) -> impl Iterator<Item = (Production, &'static BldgDef)> + '_ {
    let owner = realm::read(city.civ, |r| r.known);
    city.buildings
        .iter()
        .chain(city.gifts.iter())
        .filter_map(|&p| p.bldg().map(|b| (p, b)))
        .filter(move |(_, b)| b.obsolete < 0 || !known(owner, b.obsolete))
}

fn known(bits: u128, tech: i32) -> bool {
    tech < 0 || (tech < 128 && bits >> tech & 1 != 0)
}

/// How many active buildings carry an improvement flag.
fn count_flag(city: &City, flag: u32) -> i32 {
    active(city).filter(|(_, b)| b.flags & flag != 0).count() as i32
}

/// The city has an active building with the flag.
pub fn has_flag(city: &City, flag: u32) -> bool {
    active(city).any(|(_, b)| b.flags & flag != 0)
}

/// Does the city hold `p`, built or given.
pub fn holds(city: &City, p: Production) -> bool {
    city.has(p) || city.granted(p)
}

/// What a tile gives a city that works it: the tile's own yield, trimmed by
/// a government with the tile penalty, plus the Harbor's food on water and
/// the republics' extra commerce.
struct Rules {
    govt: &'static Govt,
    agricultural: bool,
    harbor: bool,
    colossus: bool,
    golden: bool,
}

impl Rules {
    fn of(city: &City) -> Rules {
        Rules {
            govt: realm::govt(city.civ),
            agricultural: exe_econ::has_trait(crate::cities::traits(city.civ), exe_econ::trait_bit::AGRICULTURAL),
            harbor: has_flag(city, imp::INCREASES_FOOD_IN_WATER),
            colossus: active(city).any(|(_, b)| b.wonder & wonder::PLUS_ONE_TRADE != 0),
            golden: realm::read(city.civ, |r| r.golden),
        }
    }

    /// `yields.md` 4.1 to 4.3: the bonuses come before the Despotism cap,
    /// and the Golden Age, Colossus and trade bonus only reach a tile that
    /// already yields.
    fn tile(&self, map: &GameMap, x: i32, y: i32) -> (i32, i32, i32) {
        let t = &map.tiles[map.idx(x, y)];
        let (f, s) = yields(t);
        let (mut f, mut s, mut c) = (i32::from(f), i32::from(s), i32::from(tile_commerce(t)));
        let water = matches!(t.base, Base::Coast | Base::Sea | Base::Ocean);
        // `0x5D737A..0x5D73D0`: only effective TERR Desert, not Flood Plain,
        // gets the Agricultural irrigation bonus, before the government cap.
        if self.agricultural && t.irrigation && crate::map::terrain_row(t) == 0 {
            f += 1;
        }
        if water {
            // `0x5D7470`: lakes add one food independently of a Harbor.
            // `0x5D7484`: only larger water bodies receive Harbor food.
            let lake = map.water_body_size((x, y)) <= civ3mapgen::lakes::LAKE_MAX;
            if lake || self.harbor { f += 1; }
        }
        if self.golden && s > 0 {
            s += 1;
        }
        if self.golden && c > 0 {
            c += 1;
        }
        if self.colossus && c > 0 {
            c += 1;
        }
        if self.govt.trade_bonus && c > 0 {
            c += 1;
        }
        if self.govt.tile_penalty {
            (trim_to(f), trim_to(s), trim_to(c))
        } else {
            (f, s, c)
        }
    }

    /// The city tile (`yields.md` 4.1 step 6, 4.2 step 7, 4.3 step 7): food is
    /// the RULE constant of two whatever the terrain, the size class adds
    /// shields and commerce to what the tile gives, the center never gives
    /// less than one of each (four commerce in the capital), and the
    /// government's tile penalty still applies.
    fn center(&self, map: &GameMap, city: &City) -> (i32, i32, i32) {
        let t = &map.tiles[map.idx(city.x, city.y)];
        let class = i32::from(crate::economy::size_class(city.size()));
        let capital = realm::read(city.civ, |r| r.capital) == Some((city.x, city.y));
        let (_, shields) = yields(t);
        let traits = crate::cities::traits(city.civ);
        let has = |bit| exe_econ::has_trait(traits, bit);
        let f = FOOD_PER_CITIZEN + i32::from(self.agricultural);
        // `yields.md` 4.2 step 7: class 2 adds 2 shields, one more for an
        // Industrious civ; class 1 adds 1.
        let s_class = match class {
            2 => 2 + i32::from(has(exe_econ::trait_bit::INDUSTRIOUS)),
            n => n,
        };
        let mut s = (i32::from(shields) + s_class).max(1);
        if self.golden {
            s += 1;
        }
        // 4.3 step 7: class 2 adds 2 commerce (5 for a Commercial civ),
        // class 1 adds 1 (3).
        let c_class = match (class, has(exe_econ::trait_bit::COMMERCIAL)) {
            (2, true) => 5,
            (1, true) => 3,
            (n, _) => n,
        };
        let mut c = (i32::from(tile_commerce(t)) + c_class).max(if capital { 4 } else { 1 });
        // `0x5D7EF4..0x5D7F56`: Seafaring adds commerce on an ocean coast,
        // after the capital floor, before Golden Age and government effects.
        if has(exe_econ::trait_bit::SEAFARING) && map.coastal_site(city.x, city.y) {
            c += 1;
        }
        if self.golden {
            c += 1;
        }
        if self.colossus { c += 1; }
        if self.govt.trade_bonus {
            c += 1;
        }
        if self.govt.tile_penalty {
            let food = if self.agricultural && map.fresh_water(city.x, city.y) { f } else { trim_to(f) };
            (food, trim_to(s), trim_to(c))
        } else {
            (f, s, c)
        }
    }
}

/// Food, shields and commerce one tile gives `city` if it works it.
pub fn tile_yields(map: &GameMap, city: &City, x: i32, y: i32) -> (i32, i32, i32) {
    Rules::of(city).tile(map, x, y)
}

/// What the city center gives.
pub fn center(map: &GameMap, city: &City) -> (i32, i32, i32) {
    Rules::of(city).center(map, city)
}

/// Gross food, shields and commerce: the center and every worked tile.
pub fn gross(map: &GameMap, city: &City) -> (i32, i32, i32) {
    let rules = Rules::of(city);
    let mut sum = rules.center(map, city);
    for &(x, y) in city.worked(&map).iter() {
        let (f, s, c) = rules.tile(map, x, y);
        sum = (sum.0 + f, sum.1 + s, sum.2 + c);
    }
    sum
}

/// Which loss `corruption` computes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Loss {
    /// Corruption of the commerce.
    Commerce,
    /// Waste of the shields.
    Shields,
}

/// The executable's distance between two tiles (`0x4378D0`). The clone's
/// square grid is drawn as the native diamond, so a clone offset `(dx, dy)`
/// is the native `(dx - dy, dx + dy)`; x wraps on the clone's columns.
pub fn native_distance(map: &GameMap, a: (i32, i32), b: (i32, i32)) -> i32 {
    native_distance_on(map.w, a, b)
}

/// [`native_distance`] on a map `w` columns wide.
pub fn native_distance_on(w: i32, a: (i32, i32), b: (i32, i32)) -> i32 {
    let mut dx = a.0.rem_euclid(w) - b.0.rem_euclid(w);
    if dx.abs() > w / 2 {
        dx -= w * dx.signum();
    }
    let dy = a.1 - b.1;
    civ3mapgen::starts::distance((dx - dy).abs(), (dx + dy).abs())
}

/// The native map size the corruption distance clamp reads
/// (`[0x9C74D4]`, `[0x9C74C0]`): the clone plays a Standard world
/// (`govern::WORLD_BASE`), whose shipped map is 100 by 100 (**H**: the
/// clone's own grid is 80 by 60 squares).
const NATIVE_W: i32 = 100;
const NATIVE_H: i32 = 100;

/// Corruption or waste: `City::lostToCorruption` `0x4B1190`
/// (`civ3mapgen::economy::corruption`, executed against the binary). The
/// rank orders the owner's cities by distance from the capital, ties by city
/// order (the native tie words `+0x358..+0x364` are not modelled).
pub fn corruption(map: &GameMap, city: &City, gross: i32, kind: Loss) -> i32 {
    let here = (city.x, city.y);
    let (class, capital, order, connected, palaces, cities) = realm::read(city.civ, |r| {
        (r.govt().corruption_class, r.capital, r.city_order.clone(), r.connected.contains(&here), r.palaces.clone(), r.cities)
    });
    let capital_distance = capital.map_or(i32::MAX, |c| native_distance(map, c, here));
    let rank = if class == exe_econ::COMMUNAL_CLASS {
        cities as i32 / 2
    } else if let Some(cap) = capital {
        let me = order.iter().position(|&o| o == here).unwrap_or(usize::MAX);
        order
            .iter()
            .enumerate()
            .filter(|&(j, &o)| o != here && {
                let d = native_distance(map, cap, o);
                d < capital_distance || d == capital_distance && j < me
            })
            .count() as i32
    } else {
        0
    };
    exe_econ::corruption(&exe_econ::CorruptionInputs {
        gross,
        waste: matches!(kind, Loss::Shields),
        // Disorder is applied by `totals` from this turn's moods.
        disorder: false,
        celebrating: false,
        has_capital: capital.is_some(),
        class,
        courthouses: count_flag(city, imp::REDUCES_CORRUPTION),
        is_capital: capital == Some(here),
        palaces_here: palaces.iter().filter(|&&p| p == here).count() as i32,
        ocn: crate::govern::optimal_cities(city.civ),
        world_base: crate::govern::WORLD_BASE,
        capital_distance,
        palace_distance: palaces.iter().map(|&p| native_distance(map, p, here)).min().unwrap_or(i32::MAX),
        connected: connected || capital == Some(here),
        width: NATIVE_W,
        height: NATIVE_H,
        rank,
        // Only a Policeman (`CTZN +0x78`) cuts corruption; the clone's
        // specialists are entertainers, tax collectors and scientists.
        specialists: 0,
        difficulty_percent: crate::rules_data::DIFF_CORRUPTION[crate::research::DIFFICULTY],
    })
}

/// The shield multiplier of the city's buildings, in quarters
/// (`yields.md` 5.4): 4 plus every ordinary building's bonus plus the
/// largest bonus among the ones that replace the rest, each only while the
/// improvement it needs stands.
pub fn shield_quarters(city: &City) -> i32 {
    let mut a = 4;
    let mut b = 0;
    for (_, d) in active(city) {
        if d.flags & imp::REPLACES_ALL != 0 {
            if d.requires >= 0 && holds(city, Production::from_building_row(d.requires as usize)) {
                b = b.max(d.production);
            }
        } else {
            a += d.production;
        }
    }
    a + b
}

/// Idle citizens assigned to entertainment.
pub fn entertainers(city: &City) -> u8 {
    city.specialist_jobs().filter(|s| *s == crate::cities::Specialist::Entertainer).count() as u8
}

/// Faces and flags of every improvement, as the mood code reads them.
fn faces(city: &City, r: &Realm) -> Vec<BuildingFaces> {
    let land = r.continent.get(&(city.x, city.y)).copied().unwrap_or(0);
    let mut out = Vec::new();
    for row in 0..BLDG_COUNT {
        let b = roster::bldg(row);
        if b.happy == 0 && b.happy_all == 0 && b.unhappy == 0 && b.unhappy_all == 0 {
            continue;
        }
        let p = Production::from_building_row(row);
        let own = city.has(p);
        let continental = b.other & 0x10 != 0;
        let elsewhere = r.on_continent.get(&(land, row as u16)).copied().unwrap_or(0) as i32 - own as i32;
        out.push(BuildingFaces {
            wonder: b.other & oth::WONDER != 0,
            government_ok: b.govt < 0 || b.govt as usize == r.govt,
            obsolete: b.obsolete >= 0 && r.knows(b.obsolete),
            present: own || city.granted(p),
            owned: r.owned[row] as i32,
            continental,
            on_continent: elsewhere.max(0),
            doubled: r.doubled[row],
            happy_city: b.happy,
            unhappy_city: b.unhappy,
            happy_all: b.happy_all,
            unhappy_all: b.unhappy_all,
        });
    }
    out
}

/// The moods of the city's citizens, given the luxury its commerce and
/// entertainers make (`0x4BCFF0`).
pub fn moods(city: &City, luxury: i32) -> Mood {
    let size = city.size() as usize;
    let owner_race = crate::civs::roster_index(city.civ) as i32;
    let citizens = city.citizens.slots().iter().flatten().map(|c| Citizen {
            mood: if c.resister { mood::RESISTING } else if c.job != 0 { mood::SPECIALIST } else { mood::CONTENT },
            resisting: c.resister,
            specialist: c.job != 0,
            foreign: c.race != owner_race,
            foreign_at_war: false,
        }).collect::<Vec<_>>();
    let inputs = realm::read(city.civ, |r| hap::Inputs {
        citizens: citizens.clone(),
        size: size as i32,
        born_content: r.born_content,
        buildings: faces(city, r),
        draft_timer: 0,
        draft_penalty: DRAFT_PENALTY,
        units_on_tile: r.garrison.get(&(city.x, city.y)).copied().unwrap_or(0) as i32,
        police_limit: r.govt().military_police,
        luxury_points: luxury,
        citizens_per_face: hap::shipped::CITIZENS_PER_HAPPY_FACE,
        // Luxuries that reach this city (`City +0x9C`).
        luxury_goods: (0..crate::features::GOODS.len())
            .filter(|&id| city.goods >> id & 1 != 0 && crate::features::GOODS[id].kind == crate::features::GoodKind::Luxury)
            .count(),
        luxury_trade_building: false,
        propaganda: 0,
        hurry_timer: i32::from(city.hurry_timer),
        hurry_penalty: HURRY_PENALTY,
        war_counters: r.war_counters.clone(),
        weariness_class: r.govt().war_weariness,
        police_buildings: 0,
        suffrage: r.suffrage,
    });
    let mut inputs = inputs;
    inputs.luxury_trade_building = has_flag(city, imp::LUXURY_TRADE);
    inputs.police_buildings = count_flag(city, imp::REDUCES_WAR_WEARINESS);
    let out = hap::recompute(&inputs);
    let count = |m| out.citizens.iter().filter(|c| c.mood == m).count() as u8;
    Mood {
        happy: count(mood::HAPPY),
        content: count(mood::CONTENT),
        unhappy: count(mood::UNHAPPY),
        entertainers: entertainers(city),
        scientists: city.specialist_jobs().filter(|s| *s == crate::cities::Specialist::Scientist).count() as u8,
        tax_collectors: city.specialist_jobs().filter(|s| *s == crate::cities::Specialist::TaxCollector).count() as u8,
    }
}

/// Riots when strictly more citizens are unhappy than happy.
pub fn riots(m: Mood) -> bool {
    m.happy < m.unhappy
}

/// Everything about the city's turn.
pub fn totals(map: &GameMap, city: &City) -> Totals {
    let (food, shields_gross, commerce) = gross(map, city);
    let lost_commerce = corruption(map, city, commerce, Loss::Commerce);
    let research_wonders = active(city)
        .filter(|(_, b)| b.wonder & wonder::DOUBLES_RESEARCH != 0)
        .count() as i32;
    let rates = realm::rates(city.civ);
    let double_wealth = realm::read(city.civ, |r| r.double_wealth);
    let waste = corruption(map, city, shields_gross, Loss::Shields);
    let net = shields_gross - waste;
    let shields = shield_quarters(city) * net / 4;
    let wealth = match city.production.bldg() {
        Some(b) if b.flags & imp::CAPITALIZATION != 0 => {
            exe::wealth_gold(true, shields, exe::SHIELDS_PER_GOLD, double_wealth)
        }
        _ => 0,
    };
    let split = exe::commerce_split(&exe::CommerceInput {
        tile_commerce: commerce,
        tourist: 0,
        lost: lost_commerce,
        luxury_rate: rates.lux as i32,
        science_rate: rates.sci as i32,
        luxury_buildings: count_flag(city, imp::LUXURY_BONUS),
        research_buildings: count_flag(city, imp::RESEARCH_BONUS),
        tax_buildings: count_flag(city, imp::TAX_BONUS),
        research_wonders,
        wealth,
    });
    let ent = entertainers(city) as i32;
    // An entertainer makes one luxury (`CTZN` row 1).
    let lux = split.luxury + ent;
    let m = moods(city, lux);
    let disorder = riots(m);
    let eaten = if disorder { food } else { FOOD_PER_CITIZEN * (city.size() as i32 - crate::resistance::resisters(city)) };
    if disorder {
        return Totals {
            food,
            eaten,
            surplus: 0,
            shields_gross,
            waste: shields_gross,
            shields: 0,
            commerce,
            corruption: commerce,
            tax: m.tax_collectors as i32 * 2,
            sci: m.scientists as i32 * 3,
            lux: ent,
            wealth: 0,
            mood: m,
            disorder,
        };
    }
    Totals {
        food,
        eaten,
        surplus: food - eaten,
        shields_gross,
        waste,
        shields,
        commerce,
        corruption: split.lost,
        tax: split.tax + m.tax_collectors as i32 * 2,
        sci: split.science + m.scientists as i32 * 3,
        lux,
        wealth,
        mood: m,
        disorder,
    }
}

/// Make citizens entertainers, worst tile first, until the city keeps order
/// or no one is left to work. Returns whether anything changed.
pub fn keep_order(map: &GameMap, city: &mut City) -> bool {
    let mut changed = false;
    while totals(map, city).disorder {
        let worst = city
            .worked(&map)
            .iter()
            .min_by_key(|&&(x, y)| {
                let (f, s, c) = tile_yields(map, city, x, y);
                (f, s, c, x, y)
            })
            .copied();
        let Some(w) = worst else { break };
        city.unwork(map, w, true);
        changed = true;
    }
    changed
}

/// HYPOTHESIS: the computer uses idle citizens for research (taxes when
/// its science slider is off), keeping entertainers needed for order.
pub fn assign_specialists(map: &GameMap, city: &mut City) {
    use crate::cities::Specialist;
    let idle = (city.size() as usize).saturating_sub(city.worked(&map).len());
    let job = if realm::rates(city.civ).sci == 0 { Specialist::TaxCollector } else { Specialist::Scientist };
    for i in 0..idle {
        if city.idle_job(i) != Some(Specialist::Entertainer) { continue; }
        city.set_specialist(i, job);
        if totals(map, city).disorder { city.set_specialist(i, Specialist::Entertainer); }
    }
}

/// A new unit of this type starts as a Veteran: its city has the building
/// that trains its domain (Barracks for land, Harbor for sea) and the unit
/// can fight (`upgrades` 4: the improvement flags `0x2`, `0x20000`).
pub fn trains_veterans(city: &City, unit: crate::units::UnitType) -> bool {
    let r = unit.row();
    if r.attack <= 0 {
        return false;
    }
    match r.class {
        0 => has_flag(city, imp::VETERAN_GROUND_UNITS),
        1 => has_flag(city, imp::VETERAN_SEA_UNITS),
        _ => false,
    }
}

/// The produces-units tick (`0x4BE730`, `happiness.md` 10) for one city:
/// every active building with improvement flag 30 counts a turn, and when
/// its counter has reached `frequency - 1` and the owner has the resources
/// it hands out its unit and starts over. HYPOTHESIS: the unit appears on
/// the turn after the counter fills, so the period is `frequency` turns.
pub fn produced_units(city: &mut City, has_good: impl Fn(i32) -> bool) -> Vec<crate::units::UnitType> {
    let mut made = vec![];
    let rows: Vec<(u16, i32, i32, [i32; 2], i32)> = active(city)
        .filter(|(_, b)| b.flags & roster::imp::PRODUCES_UNITS != 0 && b.produces >= 0)
        .filter_map(|(p, b)| Some((p.building_row()? as u16, b.produces, b.frequency, b.resources, 0)))
        .collect();
    city.unit_clocks.retain(|(r, _)| rows.iter().any(|x| x.0 == *r));
    for (row, unit, freq, need, _) in rows {
        let at = match city.unit_clocks.iter().position(|(r, _)| *r == row) {
            Some(i) => i,
            None => {
                city.unit_clocks.push((row, 0));
                city.unit_clocks.len() - 1
            }
        };
        let full = (freq - 1).clamp(0, 250) as u8;
        if city.unit_clocks[at].1 < full {
            city.unit_clocks[at].1 += 1;
        } else if need.iter().all(|&g| has_good(g)) {
            city.unit_clocks[at].1 = 0;
            made.push(crate::units::UnitType(unit as u8));
        }
    }
    made
}

/// The riot roll of a city that stays in disorder (`0x4BE0B0`,
/// `happiness::riot`): returns the index into `city.buildings` of the
/// improvement the rioters destroy, if any. The capital is spared, wonders
/// and the size-limit buildings never burn.
pub fn riot(rng: &mut crate::rng::MapRng, city: &City, is_capital: bool) -> Option<usize> {
    use civ3mapgen::economy::{CITY_MAX, TOWN_MAX};
    let chance = hap::shipped::CHANCE_OF_RIOTING;
    if rng.below(100) >= chance || is_capital {
        return None;
    }
    let size = city.size() as i32;
    if size <= CITY_MAX && size <= TOWN_MAX && rng.below(100) >= 2 * chance {
        return None;
    }
    if city.buildings.is_empty() {
        return None;
    }
    (0..=size + 20).find_map(|_| {
        let i = rng.below(city.buildings.len() as u32) as usize;
        let b = city.buildings[i].bldg()?;
        hap::riot_can_destroy(true, b.other, b.flags).then_some(i)
    })
}

/// `0x4B1DC0`: freshwater replaces an Aqueduct, but not a Hospital.
pub fn growth_blocked(city: &City, fresh_water: bool) -> bool {
    city.size() as i32 >= size_limit(city, fresh_water)
}

/// The size allowed by active improvement effects and local freshwater.
pub fn size_limit(city: &City, fresh_water: bool) -> i32 {
    civ3mapgen::economy::growth_limit(fresh_water,
        has_flag(city, imp::ALLOWS_SIZE_LEVEL_2),
        has_flag(city, imp::ALLOWS_SIZE_LEVEL_3))
}

/// Gold the city's improvements cost each turn: nothing under a government
/// without maintenance (Anarchy). Improvements a wonder gives are free.
pub fn upkeep(city: &City) -> i32 {
    if !realm::govt(city.civ).requires_maintenance {
        return 0;
    }
    city.buildings.iter().map(|b| i32::from(b.upkeep())).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specialists_bypass_commerce_multipliers_corruption_and_anarchy() {
        realm::reset();
        let map = flat();
        let mut city = City::new(0, "Kyoto", 5, 5);
        city.set_size(2);
        city.buildings.extend([Production::Library, Production::Marketplace]);
        city.set_specialists(vec![crate::cities::Specialist::Entertainer; 2]);
        let before = totals(&map, &city);
        city.set_specialists(vec![crate::cities::Specialist::Scientist, crate::cities::Specialist::TaxCollector]);
        let after = totals(&map, &city);
        assert_eq!(after.sci, before.sci + 3);
        assert_eq!(after.tax, before.tax + 2);
        assert_eq!(after.commerce, before.commerce);
        assert_eq!(after.lux, before.lux - 2);
        assert_eq!((after.mood.entertainers, after.mood.scientists, after.mood.tax_collectors), (0, 1, 1));
        realm::write(0, |r| { r.govt = civ3mapgen::government::row::ANARCHY; r.capital = Some((5, 5)); });
        let anarchy = totals(&map, &city);
        assert_eq!((anarchy.sci, anarchy.tax), (3, 2));
    }

    #[test]
    fn computer_specialists_keep_required_entertainers_and_use_the_rest() {
        realm::reset();
        let map = flat();
        let mut city = City::new(0, "Kyoto", 5, 5);
        city.set_size(2);
        assign_specialists(&map, &mut city);
        assert_eq!(city.specialists(), [crate::cities::Specialist::Scientist; 2]);
        city.clear_specialists();
        realm::set_rates(0, realm::Rates { tax: 10, sci: 0, lux: 0 });
        assign_specialists(&map, &mut city);
        assert_eq!(city.specialists(), [crate::cities::Specialist::TaxCollector; 2]);
        realm::write(0, |r| r.born_content = 0);
        city.set_size(4);
        city.work_tile(&map, (4, 5));
        city.work_tile(&map, (5, 4));
        city.set_specialists(vec![crate::cities::Specialist::Entertainer; 2]);
        assert!(!totals(&map, &city).disorder);
        assign_specialists(&map, &mut city);
        assert_eq!(city.specialists(), [crate::cities::Specialist::Entertainer; 2]);
        assert!(!totals(&map, &city).disorder);
    }
    use crate::map::{Cover, Relief};
    use crate::realm;

    pub fn flat() -> GameMap {
        let mut map = GameMap::generate();
        for t in map.tiles.iter_mut() {
            t.base = Base::Grassland;
            t.relief = Relief::Flat;
            t.cover = Cover::Bare;
            t.river = 0;
            t.resource = None;
            t.road = false;
            t.irrigation = false;
            t.mine = false;
            t.seen = true;
        }
        map
    }

    pub fn town(size: u8) -> City {
        let mut c = City::new(0, "Tokyo", 10, 10);
        c.set_size(size);
        c
    }

    #[test]
    fn the_statue_of_zeus_hands_out_cavalry_while_the_bronze_lasts() {
        realm::reset();
        let row = roster::BLDGS.iter().position(|b| b.name == "The Statue of Zeus").unwrap();
        let mut c = town(3);
        c.buildings.push(Production(crate::roster::UNIT_COUNT as u16 + row as u16));
        let b = roster::bldg(row);
        assert_eq!((b.produces, b.frequency), (118, 5));
        let mut turns = vec![];
        for t in 1..=12 {
            if !produced_units(&mut c, |_| true).is_empty() {
                turns.push(t);
            }
        }
        assert_eq!(turns, [5, 10]);
        for _ in 0..2 {
            assert!(produced_units(&mut c, |_| true).is_empty());
        }
        for _ in 0..3 {
            assert!(produced_units(&mut c, |_| false).is_empty());
            assert_eq!(c.unit_clocks[0].1, 4, "the clock waits, full, for the resource");
        }
        assert_eq!(produced_units(&mut c, |_| true).len(), 1);
    }

    fn work_all(map: &GameMap, c: &mut City) {
        let taken = Default::default();
        crate::cities::governor_assign(map, c, &taken);
    }

    #[test]
    fn a_plain_city_keeps_the_old_numbers() {
        realm::reset();
        let map = flat();
        let mut c = town(2);
        work_all(&map, &mut c);
        let t = totals(&map, &c);
        // Center 2/1 + two grassland tiles 2/0: 6 food, 1 shield.
        assert_eq!((t.food, t.shields_gross), (6, 1));
        assert_eq!(t.surplus, 6 - 4);
        assert!(!t.disorder);
    }

    #[test]
    fn agricultural_food_follows_freshwater_and_effective_desert_terrain() {
        realm::reset();
        let old = std::array::from_fn(crate::civs::roster_index);
        let dutch = crate::civs::roster_index_named("Netherlands").unwrap();
        crate::civs::set_players_for_test([dutch, 0, 1, 6]);
        let mut map = flat();
        let mut city = town(1);
        work_all(&map, &mut city);
        assert_eq!(center(&map, &city).0, 2, "dry Despotism center is capped");
        let at = map.idx(10, 10);
        map.tiles[at].river = crate::rivers::EAST;
        assert_eq!(center(&map, &city).0, 3);
        assert_eq!(totals(&map, &city).surplus, 3);
        crate::cities::process_city_turn(&map, &mut city, &Default::default(), &mut crate::rng::MapRng::new(1));
        assert_eq!(city.food, 3, "the freshwater bonus reaches turn production");
        map.tiles[at].river = 0;
        let neighbor = map.idx(11, 10);
        map.tiles[neighbor].base = Base::Coast;
        assert_eq!(center(&map, &city).0, 3, "a small lake also exempts food");
        for x in 11..=31 { let i = map.idx(x, 10); map.tiles[i].base = Base::Coast; }
        assert_eq!(center(&map, &city).0, 2, "21 water tiles are ocean-like");
        realm::write(0, |r| r.adopt(realm::govt_row::MONARCHY));
        assert_eq!(center(&map, &city).0, 3, "non-penalty governments need no freshwater");
        map.tiles[neighbor].base = Base::Desert;
        assert_eq!(tile_yields(&map, &city, 11, 10).0, 0);
        map.tiles[neighbor].irrigation = true;
        assert_eq!(tile_yields(&map, &city, 11, 10).0, 2);
        map.tiles[neighbor].river = crate::rivers::EAST;
        assert_eq!(tile_yields(&map, &city, 11, 10).0, 4, "Flood Plain gets no Desert trait bonus");
        crate::civs::set_players_for_test(old);
        assert_eq!(center(&map, &city).0, 2, "the bonus belongs to the Agricultural owner");
    }

    #[test]
    fn despotism_trims_a_rich_tile_and_a_monarchy_does_not() {
        realm::reset();
        let mut map = flat();
        let i = map.idx(11, 10);
        map.tiles[i].resource = Some(20); // wheat: grassland 2 + 2 food
        map.tiles[i].irrigation = true;
        let mut c = town(1);
        c.work_tile(&map, (11, 10));
        let despot = tile_yields(&map, &c, 11, 10).0;
        realm::write(0, |r| r.adopt(realm::govt_row::MONARCHY));
        let monarch = tile_yields(&map, &c, 11, 10).0;
        assert_eq!(monarch - despot, 1, "{despot} under despotism, {monarch} under monarchy");
    }

    #[test]
    fn republics_add_a_commerce_to_tiles_that_have_one() {
        realm::reset();
        let mut map = flat();
        let i = map.idx(11, 10);
        map.tiles[i].road = true;
        let c = town(1);
        let before = tile_yields(&map, &c, 11, 10).2;
        let j = map.idx(12, 10);
        let none = tile_yields(&map, &c, 12, 10).2;
        realm::write(0, |r| r.adopt(realm::govt_row::REPUBLIC));
        assert_eq!(tile_yields(&map, &c, 11, 10).2, before + 1);
        assert_eq!(tile_yields(&map, &c, 12, 10).2, none, "no commerce, no bonus");
        let _ = j;
    }

    #[test]
    fn a_harbor_feeds_the_water_tiles() {
        realm::reset();
        let mut map = flat();
        let i = map.idx(11, 10);
        map.tiles[i].base = Base::Ocean;
        let mut c = town(1);
        let lake = tile_yields(&map, &c, 11, 10).0;
        c.buildings.push(Production::Harbor);
        assert_eq!(tile_yields(&map, &c, 11, 10).0, lake, "Harbors do not boost lakes");
        for x in 12..=31 { let i = map.idx(x, 10); map.tiles[i].base = Base::Ocean; }
        c.buildings.clear();
        let dry = tile_yields(&map, &c, 11, 10).0;
        assert_eq!(lake, dry + 1);
        c.buildings.push(Production::Harbor);
        assert_eq!(tile_yields(&map, &c, 11, 10).0, dry + 1);
    }

    #[test]
    fn seafaring_commerce_requires_an_ocean_coast_and_obeys_the_cap() {
        realm::reset();
        let old = std::array::from_fn(crate::civs::roster_index);
        crate::civs::set_players_for_test([crate::civs::roster_index_named("Netherlands").unwrap(), 0, 1, 6]);
        let mut map = flat();
        let city = town(1);
        assert_eq!(center(&map, &city).2, 1);
        for x in 11..=30 { let i = map.idx(x, 10); map.tiles[i].base = Base::Coast; }
        assert_eq!(center(&map, &city).2, 1, "20-tile lake is not a Seafaring coast");
        let i = map.idx(31, 10); map.tiles[i].base = Base::Coast;
        assert_eq!(center(&map, &city).2, 2);
        realm::write(0, |r| r.capital = Some((10, 10)));
        assert_eq!(center(&map, &city).2, 4, "capital floor then Seafaring then Despotism cap");
        realm::write(0, |r| r.adopt(realm::govt_row::MONARCHY));
        assert_eq!(center(&map, &city).2, 5);
        crate::civs::set_players_for_test(old);
        assert_eq!(center(&map, &city).2, 4, "a non-Seafaring capital gets no bonus");
    }

    #[test]
    fn colossus_boosts_land_coast_and_center_but_requires_existing_commerce() {
        realm::reset();
        realm::write(0, |r| r.adopt(realm::govt_row::MONARCHY));
        let mut map = flat();
        let mut city = town(1);
        let coast = map.idx(11, 10); map.tiles[coast].base = Base::Coast;
        let road = map.idx(10, 11); map.tiles[road].road = true;
        let before = [center(&map, &city).2, tile_yields(&map, &city, 11, 10).2, tile_yields(&map, &city, 10, 11).2];
        city.buildings.push(Production::TheColossus);
        let after = [center(&map, &city).2, tile_yields(&map, &city, 11, 10).2, tile_yields(&map, &city, 10, 11).2];
        assert_eq!(after, before.map(|n| n + 1));
        assert_eq!(tile_yields(&map, &city, 9, 10).2, 0);
        realm::write(0, |r| r.govt = realm::govt_row::DESPOTISM);
        assert_eq!(tile_yields(&map, &city, 11, 10).2, 2, "cap follows the wonder bonus");
    }

    #[test]
    fn a_factory_adds_half_and_a_plant_adds_half_more_only_with_the_factory() {
        let mut c = town(1);
        assert_eq!(shield_quarters(&c), 4);
        c.buildings.push(Production::Factory);
        assert_eq!(shield_quarters(&c), 6);
        c.buildings.push(Production::ManufacturingPlant);
        assert_eq!(shield_quarters(&c), 8);
        c.buildings.clear();
        c.buildings.push(Production::SolarPlant);
        assert_eq!(shield_quarters(&c), 4, "a plant without the Factory does nothing");
        c.buildings.push(Production::Factory);
        assert_eq!(shield_quarters(&c), 8);
    }

    #[test]
    fn corruption_follows_the_executable_distance_rank_courthouse_and_palace_terms() {
        // Despotism (class 3), human Japan on Regent: optimal cities
        // 20 * 90 / 100 = 18; the distance clamp is (100 + 100) / 4 = 50.
        realm::reset();
        let map = flat();
        realm::write(0, |r| r.capital = Some((10, 10)));
        let at = |x: i32| {
            let mut c = town(1);
            c.x = x;
            c
        };
        assert_eq!(corruption(&map, &at(10), 100, Loss::Commerce), 0, "the capital: ten halvings and a cap of 0");
        // One tile away: d = 1, 3d/2 = 1, unconnected 5/4 -> 1, clamped to 2:
        // (2*100*18 + 50*18/2) / (50*18) = 4.
        assert_eq!(corruption(&map, &at(11), 100, Loss::Commerce), 4);
        // Fifteen tiles: 22, unconnected 27: (2700*18 + 450) / 900 = 54.
        let mut far = at(25);
        assert_eq!(corruption(&map, &far, 100, Loss::Commerce), 54);
        // A trade route to the capital drops the 5/4: (2200*18 + 450) / 900.
        realm::write(0, |r| { r.connected.insert((25, 10)); });
        assert_eq!(corruption(&map, &far, 100, Loss::Commerce), 44);
        realm::write(0, |r| r.connected.clear());
        // A Courthouse: one halving (27 -> 14), threshold 18 + 20/4 = 23,
        // (1400*23 + 575) / 1150 = 28, under its 80% cap.
        far.buildings.push(Production::Courthouse);
        assert_eq!(corruption(&map, &far, 100, Loss::Commerce), 28);
        far.buildings.clear();
        // Three cities nearer the capital, the capital itself included,
        // rank ahead: + 50 * ((3*100 + 1)/2).
        realm::write(0, |r| r.city_order = vec![(10, 10), (11, 10), (12, 10), (25, 10)]);
        assert_eq!(corruption(&map, &far, 100, Loss::Commerce), (2700 * 18 + 50 * 150 + 450) / 900);
        // A Forbidden Palace next door stands in for the capital, and its own
        // city takes seven halvings with a 20% cap.
        realm::write(0, |r| { r.city_order.clear(); r.palaces = vec![(24, 10)]; });
        assert_eq!(corruption(&map, &far, 100, Loss::Commerce), 4);
        assert_eq!(corruption(&map, &at(24), 100, Loss::Commerce), (100 * 18 + 450) / 900);
    }

    #[test]
    fn anarchy_loses_everything_and_pays_no_upkeep() {
        realm::reset();
        let map = flat();
        realm::write(0, |r| {
            r.capital = Some((10, 10));
            r.govt = realm::govt_row::ANARCHY;
        });
        let mut c = town(1);
        c.buildings.push(Production::Temple);
        assert_eq!(corruption(&map, &c, 7, Loss::Shields), 7);
        assert_eq!(upkeep(&c), 0);
    }

    #[test]
    fn upkeep_is_the_sum_of_the_improvements() {
        realm::reset();
        let mut c = town(1);
        c.buildings.push(Production::Temple);
        c.buildings.push(Production::Barracks);
        assert_eq!(upkeep(&c), 2);
    }

    #[test]
    fn born_content_citizens_keep_order_until_the_limit() {
        realm::reset();
        realm::write(0, |r| r.born_content = 2);
        let map = flat();
        let mut c = town(2);
        work_all(&map, &mut c);
        assert!(!totals(&map, &c).disorder);
        // Two content, one unhappy, nobody happy.
        let mut c = town(3);
        work_all(&map, &mut c);
        assert!(totals(&map, &c).disorder);
        // A Temple (1 content face) calms the third citizen.
        c.buildings.push(Production::Temple);
        assert!(!totals(&map, &c).disorder);
    }

    #[test]
    fn martial_law_counts_the_soldiers_on_the_city_tile() {
        realm::reset();
        realm::write(0, |r| r.born_content = 2);
        let map = flat();
        let mut c = town(4);
        work_all(&map, &mut c);
        assert!(totals(&map, &c).disorder);
        // Despotism lets two soldiers each calm one citizen.
        realm::write(0, |r| {
            r.garrison.insert((10, 10), 2);
        });
        assert!(!totals(&map, &c).disorder);
        realm::write(0, |r| {
            r.garrison.insert((10, 10), 1);
        });
        assert!(totals(&map, &c).disorder);
    }

    #[test]
    fn disorder_costs_the_surplus_the_shields_and_the_commerce() {
        realm::reset();
        realm::write(0, |r| r.born_content = 2);
        let map = flat();
        let mut c = town(4);
        work_all(&map, &mut c);
        let t = totals(&map, &c);
        assert!(t.disorder);
        assert_eq!((t.surplus, t.shields, t.tax, t.sci), (0, 0, 0, 0));
        assert_eq!(t.eaten, t.food);
        c.add_citizens(2, crate::civs::roster_index(c.civ));
        c.set_specialists(vec![crate::cities::Specialist::Scientist, crate::cities::Specialist::TaxCollector]);
        let t = totals(&map, &c);
        assert!(t.disorder);
        assert_eq!((t.surplus, t.shields, t.tax, t.sci), (0, 0, 2, 3));
    }

    #[test]
    fn entertainers_make_luxury_and_restore_order() {
        realm::reset();
        realm::write(0, |r| r.born_content = 2);
        let map = flat();
        let mut c = town(4);
        work_all(&map, &mut c);
        assert!(totals(&map, &c).disorder);
        assert!(keep_order(&map, &mut c));
        let t = totals(&map, &c);
        assert!(!t.disorder);
        assert!(t.mood.entertainers >= 1);
        assert_eq!(t.lux, t.mood.entertainers as i32);
    }

    #[test]
    fn the_luxury_rate_lifts_citizens() {
        realm::reset();
        realm::write(0, |r| r.born_content = 2);
        let mut map = flat();
        for t in map.tiles.iter_mut() {
            t.road = true;
        }
        let mut c = town(4);
        work_all(&map, &mut c);
        assert!(totals(&map, &c).disorder);
        realm::set_rates(0, realm::Rates { tax: 0, sci: 5, lux: 5 });
        let t = totals(&map, &c);
        assert!(t.lux > 0);
        assert!(!t.disorder, "{t:?}");
    }

    #[test]
    fn the_rates_split_the_commerce() {
        realm::reset();
        let mut map = flat();
        for t in map.tiles.iter_mut() {
            t.road = true;
        }
        let mut c = town(4);
        work_all(&map, &mut c);
        let t = totals(&map, &c);
        assert_eq!(t.tax + t.sci + t.lux, t.commerce - t.corruption);
        realm::set_rates(0, realm::Rates { tax: 10, sci: 0, lux: 0 });
        let t2 = totals(&map, &c);
        assert_eq!(t2.sci, 0);
        assert_eq!(t2.tax, t.commerce);
    }

    #[test]
    fn libraries_and_marketplaces_multiply_their_streams() {
        realm::reset();
        let mut map = flat();
        for t in map.tiles.iter_mut() {
            t.road = true;
        }
        let mut c = town(4);
        work_all(&map, &mut c);
        let base = totals(&map, &c);
        c.buildings.push(Production::Library);
        let lib = totals(&map, &c);
        assert_eq!(lib.sci, base.sci * 3 / 2);
        assert_eq!(lib.tax, base.tax);
        c.buildings.push(Production::Marketplace);
        let mk = totals(&map, &c);
        assert_eq!(mk.tax, base.tax * 3 / 2);
    }

    #[test]
    fn the_aqueduct_lets_a_town_grow_past_six() {
        let mut c = town(5);
        assert!(!growth_blocked(&c, false));
        c.set_size(6);
        assert!(growth_blocked(&c, false));
        c.buildings.push(Production::Aqueduct);
        assert!(!growth_blocked(&c, false));
        c.set_size(12);
        assert!(growth_blocked(&c, false));
        c.buildings.push(Production::Hospital);
        assert!(!growth_blocked(&c, false));
    }

    #[test]
    fn wealth_turns_shields_into_gold() {
        realm::reset();
        let map = flat();
        let mut c = town(3);
        work_all(&map, &mut c);
        c.production = Production::Wealth;
        let t = totals(&map, &c);
        assert_eq!(t.wealth, t.shields / 4 + i32::from(t.shields > 0 && t.shields < 4));
    }
}
