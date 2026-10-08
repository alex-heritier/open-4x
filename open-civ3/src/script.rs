//! Scripted input for unattended testing (pairs with `screenshot`).
//!
//! `CIV3_SCRIPT` is a `;`-separated list of `<frame>:<action>` steps, run
//! when the frame counter reaches `<frame>`. Actions:
//!
//! - `key <K>`: press a key for one frame (`B`, `V`, `R`, `Enter`, `Esc`,
//!   `Tab`, `Space`, `Up`/`Down`/`Left`/`Right`, any letter); modifiers can
//!   be combined, for example `key Ctrl+O`.
//! - `end <n>`: end `n` turns at once.
//! - `sel <Settler|Worker|Warrior|Scout>`: select the first unit of a type.
//! - `tp <x>,<y>`: teleport the selected unit (debug placement).
//! - `spawn <Settler|Worker|Warrior|Scout> <civ> <x>,<y>`: add a unit of
//!   civ index `<civ>` (0 Japan, 1 Egypt, ..., 4 the barbarians).
//! - `go <x>,<y>`: order the selected unit to walk there (attacking whatever
//!   enemy holds the last tile).
//! - `report`: print nearby units; `report all` prints every unit.
//! - `pick <unit name|All>`: press a unit-picker or disembark row.
//! - `imp <x>,<y> <road|irr|mine|fort|colony>`: put an improvement on a tile.
//!
//! Coordinates written `@dx,dy` are relative to the first city.
//! - `city`: open the first city's screen.
//! - `btn <name>`: press a city-screen button: `Change`, `Close`,
//!   `Governor`, `CloseMenu`, `PageNext`, `PagePrev`, `Prev`, `Next`, `Pick:<item>`,
//!   `Queue:<item>` (a shift-click on the item), `Unqueue:<i>`, `Hurry`,
//!   `HurryYes`, `HurryNo`, `Specialist:<i>` (zero-based idle citizen index).
//! - `adv <name>`: press an advisor button: `Science`, `Foreign`, `Close`,
//!   `Pick:<advance>`, `Talk:<civ>`, `Treaty:<clause>`, `Give:<advance>`,
//!   `Get:<advance>`, `GiveGold:<delta>`, `GetGold:<delta>`, `Propose`,
//!   `DeclareWar`, `Accept`, `Decline`, `WarYes`, `WarNo`, `Wonders`,
//!   `Page:<1|-1>`, `Zoom`. `key F6`, `key F4` and `key F7` open the Science
//!   and Foreign Advisors and the Wonders window.
//! - `meet <a> <b>`: the civs (indices as for `spawn`) make contact, which
//!   brings up the leader's greeting.
//! - `wonder <civ> <building name>`: put a wonder (or any improvement) into
//!   the first city of that civ, as if it had just been completed.
//! - `propose <civ>`: that civ offers the human an advance for a peace treaty.
//! - `build <civ> <building name>`: set what the first city of that civ builds.
//! - `hover <x>,<y>` / `unhover`: pin the map hover (no mouse in captures).
//! - `down` / `up`: press and release the left mouse button.
//! - `tile <rx>,<ry>`: click a tile of the open city's radius.
//! - `size <n> [shields]`: set the active civ's first city's size and box.
//!
//! Example: `CIV3_SCRIPT='20:key B;40:city;60:btn Change'`.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::cities::{self, City, CityView, Production, ScreenButton};
use crate::map::GameMap;
use crate::units::{self, Selected, TurnEnded, Unit, UnitArt, UnitType};

#[derive(Resource, Default)]
pub struct Script {
    steps: Vec<(u32, String)>,
    next: usize,
    frame: u32,
    held: Vec<KeyCode>,
}

pub fn setup_script(mut commands: Commands) {
    let Ok(text) = std::env::var("CIV3_SCRIPT") else {
        return;
    };
    let mut steps: Vec<(u32, String)> = text
        .split(';')
        .filter_map(|s| {
            let (f, a) = s.trim().split_once(':')?;
            Some((f.trim().parse().ok()?, a.trim().to_string()))
        })
        .collect();
    steps.sort_by_key(|(f, _)| *f);
    commands.insert_resource(Script { steps, ..default() });
}

fn key_code(name: &str) -> Option<KeyCode> {
    Some(match name {
        "Enter" => KeyCode::Enter,
        "Esc" => KeyCode::Escape,
        "Tab" => KeyCode::Tab,
        "Space" => KeyCode::Space,
        "Up" => KeyCode::ArrowUp,
        "Down" => KeyCode::ArrowDown,
        "Left" => KeyCode::ArrowLeft,
        "Right" => KeyCode::ArrowRight,
        "F1" => KeyCode::F1,
        "F4" => KeyCode::F4,
        "F5" => KeyCode::F5,
        "F8" => KeyCode::F8,
        "F6" => KeyCode::F6,
        "F7" => KeyCode::F7,
        "F9" => KeyCode::F9,
        "Ctrl" => KeyCode::ControlLeft,
        "Shift" => KeyCode::ShiftLeft,
        _ => {
            let c = name.chars().next()?;
            if name.len() != 1 || !c.is_ascii_alphabetic() {
                return None;
            }
            let keys = [
                KeyCode::KeyA,
                KeyCode::KeyB,
                KeyCode::KeyC,
                KeyCode::KeyD,
                KeyCode::KeyE,
                KeyCode::KeyF,
                KeyCode::KeyG,
                KeyCode::KeyH,
                KeyCode::KeyI,
                KeyCode::KeyJ,
                KeyCode::KeyK,
                KeyCode::KeyL,
                KeyCode::KeyM,
                KeyCode::KeyN,
                KeyCode::KeyO,
                KeyCode::KeyP,
                KeyCode::KeyQ,
                KeyCode::KeyR,
                KeyCode::KeyS,
                KeyCode::KeyT,
                KeyCode::KeyU,
                KeyCode::KeyV,
                KeyCode::KeyW,
                KeyCode::KeyX,
                KeyCode::KeyY,
                KeyCode::KeyZ,
            ];
            keys[(c.to_ascii_uppercase() as u8 - b'A') as usize]
        }
    })
}

fn pair(s: &str) -> Option<(i32, i32)> {
    let (a, b) = s.split_once(',')?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}

/// A unit type by its roster name ("Settler", "Spearman", "Three-Man
/// Chariot").
fn unit_type(name: &str) -> Option<UnitType> {
    UnitType::all().find(|u| u.row().playable && u.row().name.eq_ignore_ascii_case(name))
}

fn button_matches(b: &ScreenButton, name: &str) -> bool {
    let (kind, arg) = name.split_once(':').unwrap_or((name, ""));
    match b {
        ScreenButton::Close => kind == "Close",
        ScreenButton::Change => kind == "Change",
        ScreenButton::Governor => kind == "Governor",
        ScreenButton::Specialist(i) => kind == "Specialist" && arg.parse() == Ok(*i),
        ScreenButton::CloseMenu => kind == "CloseMenu",
        ScreenButton::MenuPage(d) => kind == if *d > 0 { "PageNext" } else { "PagePrev" },
        ScreenButton::Pick(p) => (kind == "Pick" || kind == "Queue") && p.name() == arg,
        ScreenButton::PrevCity => kind == "Prev",
        ScreenButton::NextCity => kind == "Next",
        ScreenButton::Unqueue(i) => kind == "Unqueue" && arg.parse() == Ok(*i),
        ScreenButton::Hurry => kind == "Hurry",
        ScreenButton::HurryYes => kind == "HurryYes",
        ScreenButton::HurryNo => kind == "HurryNo",
        ScreenButton::SwitchYes => kind == "SwitchYes",
        ScreenButton::SwitchNo => kind == "SwitchNo",
        ScreenButton::AbandonYes => kind == "AbandonYes",
        ScreenButton::AbandonNo => kind == "AbandonNo",
        ScreenButton::AbandonZoom => kind == "AbandonZoom",
    }
}

/// What the script presses and reaches into besides the cities and the units
/// (a system takes 16 parameters at most).
#[derive(SystemParam)]
pub struct Reach<'w, 's> {
    picker: Res<'w, crate::unit_picker::UnitPicker>,
    picker_buttons: Query<
        'w,
        's,
        (
            &'static mut Interaction,
            &'static crate::unit_picker::PickerRow,
        ),
        (
            Without<ScreenButton>,
            Without<crate::advisors::Action>,
            Without<crate::domestic::Click>,
        ),
    >,
    mouse: ResMut<'w, ButtonInput<MouseButton>>,
    pin: ResMut<'w, crate::input::HoverPin>,
    diplomacy: ResMut<'w, crate::diplomacy::Diplomacy>,
    research: Res<'w, crate::research::Research>,
    domestic_buttons: Query<
        'w,
        's,
        (&'static mut Interaction, &'static crate::domestic::Click),
        (Without<ScreenButton>, Without<crate::advisors::Action>),
    >,
}

/// Runs first in `Update`, after input and UI focus have run in `PreUpdate`,
/// so injected keys read as `just_pressed` and pressed buttons as
/// `Changed<Interaction>` for the rest of the frame.
pub fn drive_script(
    mut commands: Commands,
    art: Res<UnitArt>,
    script: Option<ResMut<Script>>,
    civs: Res<crate::civs::Civilizations>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut reach: Reach,
    mut turn_end: MessageWriter<TurnEnded>,
    mut selected: ResMut<Selected>,
    mut units: Query<(Entity, &mut Unit)>,
    mut map: ResMut<GameMap>,
    mut cities: Query<&mut City>,
    city_ids: Query<Entity, With<City>>,
    mut view: ResMut<CityView>,
    mut buttons: Query<
        (&mut Interaction, &ScreenButton),
        (
            Without<crate::advisors::Action>,
            Without<crate::domestic::Click>,
        ),
    >,
    mut advisor_buttons: Query<
        (&mut Interaction, &crate::advisors::Action),
        (Without<ScreenButton>, Without<crate::domestic::Click>),
    >,
) {
    let Some(mut script) = script else {
        return;
    };
    let origin = cities
        .iter()
        .find(|c| c.civ == civs.active)
        .map(|c| (c.x, c.y));
    let w = map.w;
    let coord = |s: &str| match s.strip_prefix('@') {
        Some(rel) => {
            let (dx, dy) = pair(rel)?;
            let (ox, oy) = origin?;
            Some(((ox + dx).rem_euclid(w), oy + dy))
        }
        None => pair(s),
    };
    for k in std::mem::take(&mut script.held) {
        keys.release(k);
    }
    script.frame += 1;
    while let Some((at, action)) = script.steps.get(script.next).cloned() {
        if script.frame < at {
            break;
        }
        script.next += 1;
        println!("script: frame {at}: {action}");
        let (verb, arg) = action.split_once(' ').unwrap_or((&action, ""));
        let arg = arg.trim();
        match verb {
            "key" => {
                for part in arg.split('+') {
                    let Some(k) = key_code(part) else {
                        eprintln!("script: unknown key {part}");
                        continue;
                    };
                    keys.press(k);
                    script.held.push(k);
                }
            }
            "end" => {
                for _ in 0..arg.parse().unwrap_or(1) {
                    turn_end.write(TurnEnded);
                }
            }
            "sel" => {
                let t = unit_type(arg);
                selected.0 = units
                    .iter()
                    .find(|(_, u)| u.civ == civs.active && Some(u.utype) == t)
                    .map(|(e, _)| e);
                if let Some((_, mut u)) = selected.0.and_then(|e| units.get_mut(e).ok()) {
                    u.fortified = false;
                    u.sentry = false;
                    u.exploring = false;
                    u.auto = false;
                }
            }
            "tp" => {
                if let (Some((x, y)), Some(s)) = (coord(arg), selected.0) {
                    if let Ok((_, mut u)) = units.get_mut(s) {
                        u.x = x;
                        u.y = y;
                        u.path.clear();
                    }
                }
            }
            "spawn" => {
                let mut words = arg.split_whitespace();
                let spec = (
                    words.next().and_then(unit_type),
                    words.next().and_then(|c| c.parse::<usize>().ok()),
                    words.next().and_then(coord),
                );
                match spec {
                    (Some(t), Some(civ), Some((x, y))) if civ <= crate::civs::BARBARIANS => {
                        units::spawn_unit(&mut commands, &art, t, x, y, civ);
                    }
                    _ => eprintln!("script: bad spawn {arg}"),
                }
            }
            "go" => {
                let snapshot: Vec<_> = units.iter().map(|(e, u)| (e, u.clone())).collect();
                if let (Some(dest), Some(s)) = (coord(arg), selected.0) {
                    if let Ok((_, mut u)) = units.get_mut(s) {
                        let ports: Vec<_> = cities
                            .iter()
                            .filter(|c| c.civ == u.civ && c.coastal)
                            .map(|c| (c.x, c.y))
                            .collect();
                        crate::naval::order_move(&map, &mut u, dest, &ports, &snapshot);
                    }
                }
            }
            "report" => {
                // Ground truth for captures: every unit near the first city.
                let (ox, oy) = origin.unwrap_or((0, 0));
                for (_, u) in units
                    .iter()
                    .filter(|(_, u)| arg == "all" || (u.x - ox).abs() <= 3 && (u.y - oy).abs() <= 3)
                {
                    println!(
                        "script: unit {:?} civ {} @({},{}) {:?} hp {}/{} moves {} carrier {:?}",
                        u.utype,
                        u.civ,
                        u.x - ox,
                        u.y - oy,
                        u.level,
                        u.hp(),
                        u.max_hp(),
                        u.moves,
                        u.carrier
                    );
                }
            }
            "imp" => {
                let (pos, kind) = arg.split_once(' ').unwrap_or((arg, ""));
                if let Some((x, y)) = coord(pos) {
                    let i = map.idx(x, y);
                    let t = &mut map.tiles[i];
                    match kind {
                        "road" => t.road = true,
                        "irr" => t.irrigation = true,
                        "mine" => t.mine = true,
                        "fort" => t.fortress = true,
                        "barricade" => {
                            t.fortress = true;
                            t.barricade = true;
                        }
                        "outpost" => t.site = Some(crate::sites::Site::Outpost(0)),
                        "colony" => t.site = Some(crate::sites::Site::Colony(0)),
                        _ => eprintln!("script: unknown improvement {kind}"),
                    }
                }
            }
            // `size <n> [shields]`: set the first city's size (and box).
            "size" => {
                let mut it = arg.split_whitespace().map(|v| v.parse::<u16>().ok());
                if let Some(c) = cities.iter_mut().find(|c| c.civ == civs.active).as_mut() {
                    if let Some(Some(n)) = it.next() {
                        c.set_size(n as u8);
                    }
                    if let Some(Some(sh)) = it.next() {
                        c.shields = sh;
                    }
                }
            }
            "hover" => match coord(arg) {
                Some(p) => reach.pin.0 = Some(p),
                None => eprintln!("script: bad hover {arg}"),
            },
            "unhover" => reach.pin.0 = None,
            "right" => reach.mouse.press(MouseButton::Right),
            "right-up" => reach.mouse.release(MouseButton::Right),
            "down" => reach.mouse.press(MouseButton::Left),
            "up" => reach.mouse.release(MouseButton::Left),
            "city" => {
                view.0 = city_ids
                    .iter()
                    .find(|e| cities.get(*e).is_ok_and(|c| c.civ == civs.active))
            }
            "pick" => match reach.picker_buttons.iter_mut().find(|(_, row)| {
                if arg == "All" {
                    reach.picker.unload == Some(row.0)
                } else {
                    units
                        .get(row.0)
                        .is_ok_and(|(_, u)| units::def(u.utype).name.eq_ignore_ascii_case(arg))
                }
            }) {
                Some((mut i, _)) => *i = Interaction::Pressed,
                None => eprintln!("script: no picker row {arg}"),
            },
            "btn" => match buttons.iter_mut().find(|(_, b)| button_matches(b, arg)) {
                Some((mut i, _)) => {
                    // `Queue:<item>` is a shift-click on the item's row.
                    if arg.starts_with("Queue:") {
                        keys.press(KeyCode::ShiftLeft);
                        script.held.push(KeyCode::ShiftLeft);
                    }
                    *i = Interaction::Pressed;
                }
                None => eprintln!("script: no button {arg}"),
            },
            "adv" => match advisor_buttons
                .iter_mut()
                .find(|(_, a)| a.script_name() == arg)
            {
                Some((mut i, _)) => *i = Interaction::Pressed,
                None => eprintln!("script: no advisor button {arg}"),
            },
            "dom" => {
                match reach
                    .domestic_buttons
                    .iter_mut()
                    .find(|(_, c)| c.script_name() == arg)
                {
                    Some((mut i, _)) => *i = Interaction::Pressed,
                    None => eprintln!("script: no domestic button {arg}"),
                }
            }
            "meet" => {
                let mut civs = arg
                    .split_whitespace()
                    .filter_map(|c| c.parse::<usize>().ok());
                match (civs.next(), civs.next()) {
                    (Some(a), Some(b)) if a != b && a.max(b) < crate::civs::civ_count() => {
                        reach.diplomacy.meet(a, b);
                    }
                    _ => eprintln!("script: bad meet {arg}"),
                }
            }
            "propose" => {
                // The civ offers the first advance it can spare, for peace.
                use crate::diplomacy::{Deal, Proposal};
                use civ3_rules::diplomacy::Clause;
                let from = arg
                    .parse::<usize>()
                    .ok()
                    .filter(|&c| c != civs.viewer() && c < crate::civs::civ_count());
                let spare =
                    from.and_then(|c| reach.research.giftable(c, civs.viewer()).first().copied());
                match (from, spare) {
                    (Some(from), Some(tech)) => {
                        let mut deal = Deal::new(civs.viewer(), from);
                        deal.from_b.push(Clause::Tech(tech));
                        deal.from_a.push(Clause::Peace);
                        reach.diplomacy.meet(civs.viewer(), from);
                        reach.diplomacy.proposals.push_back(Proposal {
                            from,
                            to: civs.viewer(),
                            deal,
                        });
                    }
                    _ => eprintln!("script: bad propose {arg}"),
                }
            }
            "wonder" | "build" => {
                let (civ, name) = arg.split_once(' ').unwrap_or((arg, ""));
                let row = (0..crate::roster::bldg_count()).find(|&r| {
                    crate::roster::bldg(r)
                        .name
                        .eq_ignore_ascii_case(name.trim())
                });
                let city = civ
                    .parse::<usize>()
                    .ok()
                    .and_then(|civ| cities.iter_mut().find(|c| c.civ == civ));
                match (row, city) {
                    (Some(row), Some(mut city)) if verb == "wonder" => {
                        city.buildings.push(Production::from_building_row(row))
                    }
                    (Some(row), Some(mut city)) => {
                        city.production = Production::from_building_row(row)
                    }
                    _ => eprintln!("script: bad {verb} {arg}"),
                }
            }
            "tile" => {
                if let (Some((rx, ry)), Some(e)) = (pair(arg), view.0) {
                    if !cities::click_cluster(&map, &mut cities, e, rx, ry) {
                        println!("script: tile {rx},{ry} is not workable");
                    }
                }
            }
            _ => eprintln!("script: unknown action {action}"),
        }
    }
}
