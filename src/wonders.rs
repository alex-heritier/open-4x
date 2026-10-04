//! The wonders as the player sees them: the splash when one of ours is
//! completed, the news of a rival's, and the Wonders of the World window
//! (F7). What a wonder does is the city's business (`cities.rs`); this only
//! watches the cities and shows what happened.
//!
//! `track` compares the wonders standing in the cities with the ledger from
//! frame to frame, so it needs nothing from the code that completes them.
//!
//! The splash is the exe's wonder screen (`0x5CF640`): the 1024 x 768
//! `wonderBackground.pcx` with the wonder's 320 x 320 picture in the hole at
//! (351, 109) and `script.txt #WONDERSPLASH` under it. It is shown for great
//! and small wonders, to their owner only; everybody else gets the
//! `#WONDERPRODUCE` line for a great wonder.
use std::collections::{HashSet, VecDeque};

use bevy::prelude::*;

use crate::advisors::Action;
use crate::cities::City;
use crate::civs::{CIV_COUNT, CIVS, is_ai};
use crate::diplomacy::{Diplomacy, people};
use crate::features::{MessageBoard, post};
use crate::leaders::LEADERS;
use crate::roster;
use crate::stage::{INK, Ui, WARN};
use crate::units::Turn;

/// Cards on a page of the window.
pub const PER_PAGE: usize = 6;

/// A wonder standing in a city.
#[derive(Clone, Debug, PartialEq)]
pub struct Seen {
    pub row: usize,
    pub civ: usize,
    pub city: String,
    pub x: i32,
    pub y: i32,
}

/// A great wonder in the ledger.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Built {
    pub row: usize,
    pub civ: usize,
    pub city: String,
    pub x: i32,
    pub y: i32,
    /// The turn it was completed; 0 for one that stood when play began.
    pub turn: u32,
    /// Its city is gone.
    pub lost: bool,
}

/// What happened to the wonders between two looks.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Completed { great: bool, seen: Seen },
    /// A great wonder's city changed hands.
    Captured { from: usize, seen: Seen },
    Lost { row: usize },
}

/// A wonder splash waiting for its owner.
#[derive(Clone, Debug, PartialEq)]
pub struct Splash {
    pub seen: Seen,
}

#[derive(Resource, Default)]
pub struct Wonders {
    /// Every great wonder built, oldest first.
    pub built: Vec<Built>,
    /// Splashes to show, in order.
    pub splash: VecDeque<Splash>,
    /// Small wonders standing, by row and owner.
    small: HashSet<(usize, usize)>,
    primed: bool,
}

/// What a saved game keeps of the wonders: the ledger and the small
/// wonders standing.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Saved {
    built: Vec<Built>,
    small: Vec<(usize, usize)>,
}

impl Wonders {
    pub fn snapshot(&self) -> Saved {
        let mut small: Vec<_> = self.small.iter().copied().collect();
        small.sort();
        Saved { built: self.built.clone(), small }
    }

    /// The ledger as saved, with no splash waiting.
    pub fn restore(&mut self, saved: &Saved) {
        self.built = saved.built.clone();
        self.small = saved.small.iter().copied().collect();
        self.splash.clear();
        self.primed = true;
    }
}

fn is_great(row: usize) -> bool {
    roster::bldg(row).is_great_wonder()
}

impl Wonders {
    /// Look at the wonders standing now (`turn` is the turn it is).
    pub fn update(&mut self, now: &[Seen], turn: u32) -> Vec<Event> {
        let mut events = vec![];
        if !self.primed {
            // What stands when we first look was there all along.
            self.primed = true;
            for s in now {
                if is_great(s.row) {
                    self.built.push(Built { row: s.row, civ: s.civ, city: s.city.clone(), x: s.x, y: s.y, turn: 0, lost: false });
                } else {
                    self.small.insert((s.row, s.civ));
                }
            }
            return events;
        }
        self.small.retain(|&(row, civ)| now.iter().any(|s| s.row == row && s.civ == civ));
        for s in now {
            if !is_great(s.row) {
                if self.small.insert((s.row, s.civ)) {
                    events.push(Event::Completed { great: false, seen: s.clone() });
                }
                continue;
            }
            match self.built.iter_mut().find(|b| b.row == s.row) {
                Some(b) if !b.lost => {
                    if b.civ != s.civ {
                        events.push(Event::Captured { from: b.civ, seen: s.clone() });
                    }
                    (b.civ, b.city, b.x, b.y) = (s.civ, s.city.clone(), s.x, s.y);
                }
                found => {
                    // New, or standing again after its city was lost.
                    let entry = Built { row: s.row, civ: s.civ, city: s.city.clone(), x: s.x, y: s.y, turn, lost: false };
                    match found {
                        Some(b) => *b = entry,
                        None => self.built.push(entry),
                    }
                    events.push(Event::Completed { great: true, seen: s.clone() });
                }
            }
        }
        for b in self.built.iter_mut().filter(|b| !b.lost) {
            if !now.iter().any(|s| s.row == b.row) {
                b.lost = true;
                events.push(Event::Lost { row: b.row });
            }
        }
        events
    }
}

impl Wonders {
    /// The splash `civ` has yet to see.
    pub fn next_for(&self, civ: usize) -> Option<&Splash> {
        self.splash.iter().find(|s| s.seen.civ == civ)
    }

    /// `civ` has seen its splash.
    pub fn dismiss(&mut self, civ: usize) {
        if let Some(i) = self.splash.iter().position(|s| s.seen.civ == civ) {
            self.splash.remove(i);
        }
    }
}

/// Every wonder standing in the cities.
fn standing<'a>(cities: impl Iterator<Item = &'a City>) -> Vec<Seen> {
    let mut now = vec![];
    for c in cities {
        for p in &c.buildings {
            let Some(row) = p.building_row() else { continue };
            let b = roster::bldg(row);
            if b.is_great_wonder() || b.is_small_wonder() {
                now.push(Seen { row, civ: c.civ, city: c.name.clone(), x: c.x, y: c.y });
            }
        }
    }
    now
}

/// Watch the cities: queue a splash for a human's wonder, tell the humans of
/// a rival's.
pub fn track(turn: Res<Turn>, cities: Query<&City>, mut wonders: ResMut<Wonders>, mut board: ResMut<MessageBoard>) {
    let now = standing(cities.iter());
    for event in wonders.update(&now, turn.0) {
        match event {
            Event::Completed { great, seen } => {
                if !is_ai(seen.civ) {
                    wonders.splash.push_back(Splash { seen: seen.clone() });
                }
                if great && (0..CIV_COUNT).any(|h| !is_ai(h) && h != seen.civ) {
                    post(&mut board, produced(&seen));
                }
            }
            Event::Captured { from, seen } => {
                let wonder = roster::bldg(seen.row).name;
                if !is_ai(seen.civ) {
                    post(&mut board, format!("We captured {}; we now control {wonder}!", seen.city));
                } else if !is_ai(from) {
                    post(&mut board, format!("The {} have captured {} -- along with {wonder}!", people(seen.civ), seen.city));
                }
            }
            Event::Lost { row } => post(&mut board, format!("{} is lost with its city.", roster::bldg(row).name)),
        }
    }
}

/// `script.txt #WONDERPRODUCE`.
pub fn produced(seen: &Seen) -> String {
    format!(
        "We have information that the {} city of {} has completed a great project, {}.",
        CIVS[seen.civ].adjective,
        seen.city,
        roster::bldg(seen.row).name
    )
}

/// `script.txt #WONDERSPLASH`.
pub fn splash_text(seen: &Seen) -> String {
    format!("{}, we have completed {} in {}.", LEADERS[seen.civ].title, roster::bldg(seen.row).name, seen.city)
}

/// File stem of a wonder's art: `Sun Tzu's Art of War` is `sun_tzu_s_art_of_war`
/// (`slug` in `tools/prep_assets.py`).
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_end_matches('_').to_string()
}

fn art_exists(dir: &str, row: usize) -> bool {
    std::path::Path::new(&format!("assets/gen/wonders/{dir}/{}.png", slug(roster::bldg(row).name))).is_file()
}

// ---------------------------------------------------------------------
// The splash
// ---------------------------------------------------------------------

/// Where the art goes in `wonderBackground.pcx`: a 320 x 320 hole.
const ART: (f32, f32, f32) = (351.0, 109.0, 320.0);

pub fn splash(stage: &mut ChildSpawnerCommands, ui: &Ui, assets: &AssetServer, s: &Splash) {
    let seen = &s.seen;
    if art_exists("splash", seen.row) {
        let path = format!("gen/wonders/splash/{}.png", slug(roster::bldg(seen.row).name));
        ui.picture(stage, ImageNode::new(assets.load(path)), ART.0, ART.1, ART.2, ART.2);
    }
    ui.picture(stage, ImageNode::new(assets.load("gen/wonders/frame.png")), 0.0, 0.0, 1024.0, 768.0);
    ui.words(stage, 285.0, 450.0, 454.0, 90.0, splash_text(seen), 24.0, INK, true);
    ui.button(stage, 300.0, 590.0, 200.0, 34.0, "Zoom to City.", 18.0, Action::Zoom(seen.x, seen.y), false);
    ui.button(stage, 524.0, 590.0, 200.0, 34.0, "Sounds Good.", 18.0, Action::Close, false);
}

/// Play the wonder fanfare once as a splash comes up.
pub fn fanfare(
    mut commands: Commands,
    advisors: Res<crate::advisors::Advisors>,
    assets: Res<AssetServer>,
    mut played: Local<bool>,
) {
    let up = advisors.screen == crate::advisors::Screen::Splash;
    if up && !*played {
        commands.spawn(AudioPlayer::<AudioSource>(assets.load("gen/audio/ui/Wonder.wav")));
    }
    *played = up;
}

// ---------------------------------------------------------------------
// The Wonders of the World window
// ---------------------------------------------------------------------

/// One card of the window.
#[derive(Clone, Debug, PartialEq)]
pub struct Card {
    pub row: usize,
    pub civ: usize,
    pub city: String,
    pub x: i32,
    pub y: i32,
    /// The turn it was completed; `None` while it is being built.
    pub turn: Option<u32>,
    pub lost: bool,
}

/// The wonders the viewer can list: every great wonder built, then the ones
/// under construction in the viewer's cities and in those of civs met.
pub fn cards(wonders: &Wonders, cities: &[&City], diplomacy: &Diplomacy, viewer: usize) -> Vec<Card> {
    let mut out: Vec<Card> = wonders
        .built
        .iter()
        .map(|b| Card { row: b.row, civ: b.civ, city: b.city.clone(), x: b.x, y: b.y, turn: Some(b.turn), lost: b.lost })
        .collect();
    out.sort_by_key(|c| (c.turn, c.row));
    let mut building: Vec<Card> = vec![];
    for c in cities {
        let Some(row) = c.production.building_row() else { continue };
        if !is_great(row) || !(c.civ == viewer || diplomacy.contact(viewer, c.civ)) {
            continue;
        }
        if wonders.built.iter().any(|b| b.row == row) || building.iter().any(|b| b.row == row && b.civ == c.civ) {
            continue;
        }
        building.push(Card { row, civ: c.civ, city: c.name.clone(), x: c.x, y: c.y, turn: None, lost: false });
    }
    building.sort_by_key(|c| (c.row, c.civ));
    out.extend(building);
    out
}

/// Pages in a list of `n` cards (at least one).
pub fn pages(n: usize) -> usize {
    n.div_ceil(PER_PAGE).max(1)
}

/// Top left of the card in `slot` of a page: two columns, three rows, in the
/// 914 x 645 field of `wonders_background.pcx` (x 56..969, y 69..713).
fn slot_at(slot: usize) -> (f32, f32) {
    (133.0 + 390.0 * (slot % 2) as f32, 79.0 + 212.0 * (slot / 2) as f32)
}

/// The eye button on `wondersEye.pcx`: three 66 x 47 states, stacked at x = 1.
fn eye_state(i: u32) -> Rect {
    Rect::new(1.0, (1 + 48 * i) as f32, 67.0, (48 + 48 * i) as f32)
}

pub fn window(stage: &mut ChildSpawnerCommands, ui: &Ui, assets: &AssetServer, cards: &[Card], page: usize) {
    ui.picture(stage, ImageNode::new(assets.load("gen/wonders/window.png")), 0.0, 0.0, 1024.0, 768.0);
    ui.words(stage, 174.0, 18.0, 682.0, 34.0, "WONDERS OF THE WORLD", 28.0, INK, true);
    let page = page.min(pages(cards.len()) - 1);
    if cards.is_empty() {
        ui.words(stage, 133.0, 360.0, 760.0, 40.0, "No wonder has been built yet.", 22.0, INK, true);
    }
    for (i, card) in cards.iter().skip(page * PER_PAGE).take(PER_PAGE).enumerate() {
        let (x, y) = slot_at(i);
        let name = roster::bldg(card.row).name;
        ui.picture(stage, ImageNode::new(assets.load("gen/wonders/card.png")), x, y, 370.0, 200.0);
        ui.words(stage, x + 12.0, y + 10.0, 146.0, 40.0, name, 17.0, INK, false);
        let when = match card.turn {
            _ if card.lost => "Destroyed".to_string(),
            Some(0) => "Before the game".to_string(),
            Some(t) => crate::calendar::label(t),
            None => "Under construction".to_string(),
        };
        let owner = if card.lost { "-" } else { CIVS[card.civ].name };
        let place = if card.lost { "-".to_string() } else { card.city.clone() };
        for (k, (label, value)) in [("Owned by:", owner.to_string()), ("Constructed in:", when), ("Located in:", place)]
            .into_iter()
            .enumerate()
        {
            let top = y + 52.0 + 46.0 * k as f32;
            ui.words(stage, x + 12.0, top, 146.0, 18.0, label, 13.0, INK, false);
            ui.words(stage, x + 12.0, top + 16.0, 146.0, 22.0, value, 16.0, if card.lost { WARN } else { INK }, false);
        }
        let built = card.turn.is_some() && !card.lost;
        if built && art_exists("thumb", card.row) {
            let path = format!("gen/wonders/thumb/{}.png", slug(name));
            ui.picture(stage, ImageNode::new(assets.load(path)), x + 162.0, y + 47.0, 190.0, 132.0);
        }
        if !built {
            // Not built: the plate over the picture.
            ui.picture(stage, ImageNode::new(assets.load("gen/wonders/card_hidden.png")), x, y, 370.0, 200.0);
        } else {
            let mut eye = ImageNode::new(assets.load("gen/wonders/eye.png"));
            eye.rect = Some(eye_state(0));
            stage
                .spawn((Button, Action::Zoom(card.x, card.y), ui.st.rect(x + 286.0, y + 47.0, 66.0, 47.0), eye));
        }
    }
    let n = pages(cards.len());
    if n > 1 {
        ui.words(stage, 940.0, 150.0, 70.0, 20.0, format!("{}/{n}", page + 1), 14.0, INK, true);
        if page > 0 {
            ui.button(stage, 980.0, 100.0, 36.0, 36.0, "^", 20.0, Action::Page(-1), false);
        }
        if page + 1 < n {
            ui.button(stage, 980.0, 180.0, 36.0, 36.0, "v", 20.0, Action::Page(1), false);
        }
    }
    crate::advisor_frame::close_box(stage, ui, assets, Action::Close);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cities::Production;

    fn row_named(name: &str) -> usize {
        roster::BLDGS.iter().position(|b| b.name == name).unwrap_or_else(|| panic!("no BLDG {name}"))
    }

    fn city(civ: usize, name: &str) -> City {
        City {
            gifts: vec![],
            coastal: false,
            river: false,
            unrest: 0,
            hurry_timer: 0,
            stakes: Default::default(),
            cooldown: 0,
            unit_clocks: Vec::new(),
            civ,
            name: name.into(),
            x: 3,
            y: 4,
            diseased: false,
            citizens: crate::citizens::new_pool(civ, 3),
            food: 0,
            shields: 0,
            production: Production::Warrior,
            queue: vec![],
            buildings: vec![],
            culture: 0,
            founded: 1,
        }
    }

    fn seen(name: &str, civ: usize, city: &str) -> Seen {
        Seen { row: row_named(name), civ, city: city.into(), x: 3, y: 4 }
    }

    #[test]
    fn what_stands_at_the_first_look_was_always_there() {
        let mut w = Wonders::default();
        let events = w.update(&[seen("The Pyramids", 0, "Kyoto"), seen("Heroic Epic", 1, "Rome")], 5);
        assert!(events.is_empty());
        assert_eq!(w.built.len(), 1);
        assert_eq!(w.built[0].turn, 0);
        assert!(w.update(&[seen("The Pyramids", 0, "Kyoto"), seen("Heroic Epic", 1, "Rome")], 6).is_empty());
    }

    #[test]
    fn a_new_great_wonder_is_completed_once_and_recorded_with_its_turn() {
        let mut w = Wonders::default();
        w.update(&[], 1);
        let pyramids = seen("The Pyramids", 0, "Kyoto");
        assert_eq!(w.update(std::slice::from_ref(&pyramids), 9), [Event::Completed { great: true, seen: pyramids.clone() }]);
        assert!(w.update(std::slice::from_ref(&pyramids), 10).is_empty(), "no second announcement");
        assert_eq!((w.built[0].civ, w.built[0].turn, w.built[0].city.as_str()), (0, 9, "Kyoto"));
    }

    #[test]
    fn a_small_wonder_is_completed_per_owner() {
        let mut w = Wonders::default();
        w.update(&[], 1);
        let epic = seen("Heroic Epic", 1, "Rome");
        let events = w.update(std::slice::from_ref(&epic), 2);
        assert_eq!(events, [Event::Completed { great: false, seen: epic.clone() }]);
        assert!(w.built.is_empty(), "small wonders are not in the ledger of the window");
        let both = [epic.clone(), seen("Heroic Epic", 2, "Thebes")];
        assert_eq!(w.update(&both, 3).len(), 1, "another civ's own Heroic Epic is news again");
    }

    #[test]
    fn a_captured_wonder_changes_owner_and_a_razed_one_is_lost() {
        let mut w = Wonders::default();
        w.update(&[], 1);
        let a = seen("The Oracle", 1, "Rome");
        w.update(std::slice::from_ref(&a), 4);
        let b = Seen { civ: 0, city: "Kyoto".into(), ..a.clone() };
        assert_eq!(w.update(std::slice::from_ref(&b), 8), [Event::Captured { from: 1, seen: b.clone() }]);
        assert_eq!((w.built[0].civ, w.built[0].turn), (0, 4), "the turn it was built stays");
        assert_eq!(w.update(&[], 9), [Event::Lost { row: a.row }]);
        assert!(w.built[0].lost);
        assert!(w.update(&[], 10).is_empty());
        // Built again, it is news again.
        assert_eq!(w.update(std::slice::from_ref(&a), 12).len(), 1);
        assert_eq!((w.built[0].lost, w.built[0].turn), (false, 12));
    }

    #[test]
    fn the_wonders_in_the_cities_are_found_by_their_rows() {
        let row = row_named("The Colossus");
        let mut city = city(1, "Veii");
        city.buildings = vec![Production::from_building_row(row_named("Temple")), Production::from_building_row(row)];
        let now = standing([&city].into_iter());
        assert_eq!(now, [Seen { row, civ: 1, city: "Veii".into(), x: city.x, y: city.y }]);
    }

    #[test]
    fn the_texts_are_the_games() {
        let s = seen("The Great Library", 0, "Kyoto");
        assert_eq!(splash_text(&s), "Shogun, we have completed The Great Library in Kyoto.");
        let r = seen("The Pyramids", 1, "Veii");
        assert_eq!(
            produced(&r),
            "We have information that the Roman city of Veii has completed a great project, The Pyramids."
        );
    }

    #[test]
    fn art_names_follow_the_prep_script() {
        assert_eq!(slug("Sun Tzu's Art of War"), "sun_tzu_s_art_of_war");
        assert_eq!(slug("The Pyramids"), "the_pyramids");
        assert_eq!(slug("SETI program"), "seti_program");
        assert_eq!(slug("JS Bach's Cathedral"), "js_bach_s_cathedral");
        assert_eq!(slug("Copernicus' Observatory"), "copernicus_observatory");
    }

    #[test]
    fn every_wonder_but_the_internet_has_its_art() {
        // Skipped without the converted art (`tools/prep_assets.py wonders`).
        if !std::path::Path::new("assets/gen/wonders/splash").is_dir() {
            eprintln!("skipped: assets/gen/wonders is not built");
            return;
        }
        for (row, b) in roster::BLDGS.iter().enumerate().filter(|(_, b)| b.is_great_wonder() || b.is_small_wonder()) {
            let has = art_exists("splash", row) && art_exists("thumb", row);
            assert_eq!(has, b.name != "The Internet", "{}", b.name);
        }
    }

    #[test]
    fn the_window_lists_what_was_built_then_what_is_building() {
        let mut w = Wonders::default();
        w.update(&[], 1);
        let rome = seen("The Oracle", 1, "Rome");
        let kyoto = seen("The Pyramids", 0, "Kyoto");
        w.update(std::slice::from_ref(&rome), 3);
        w.update(&[rome, kyoto], 7);
        let d = Diplomacy::new();
        let mut mine = city(0, "Osaka");
        mine.production = Production::from_building_row(row_named("The Great Library"));
        let mut theirs = city(2, "Thebes");
        theirs.production = Production::from_building_row(row_named("The Colossus"));
        let list = cards(&w, &[&mine, &theirs], &d, 0);
        let names: Vec<_> = list.iter().map(|c| (roster::bldg(c.row).name, c.turn)).collect();
        assert_eq!(
            names,
            [("The Oracle", Some(3)), ("The Pyramids", Some(7)), ("The Great Library", None)],
            "Egypt's Colossus is not listed: the civs have not met"
        );
        assert_eq!([pages(0), pages(6), pages(7), pages(13)], [1, 1, 2, 3]);
    }

    #[test]
    fn cards_sit_in_two_columns_and_three_rows() {
        assert_eq!(slot_at(0), (133.0, 79.0));
        assert_eq!(slot_at(1), (523.0, 79.0));
        assert_eq!(slot_at(5), (523.0, 503.0));
        let (x, y) = slot_at(5);
        assert!(x + 370.0 <= 969.0 && y + 200.0 <= 713.0, "inside the field of the background");
    }
}
