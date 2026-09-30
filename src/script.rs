//! Scripted input for unattended testing (pairs with `screenshot`).
//!
//! `CIV3_SCRIPT` is a `;`-separated list of `<frame>:<action>` steps, run
//! when the frame counter reaches `<frame>`. Actions:
//!
//! - `key <K>`: press a key for one frame (`B`, `V`, `R`, `Enter`, `Esc`,
//!   `Tab`, `Space`, `Up`/`Down`/`Left`/`Right`, any letter).
//! - `end <n>`: end `n` turns at once.
//! - `sel <Settler|Worker|Warrior|Scout>`: select the first unit of a type.
//! - `tp <x>,<y>`: teleport the selected unit (debug placement).
//! - `imp <x>,<y> <road|irr|mine>`: put an improvement on a tile.
//!
//! Coordinates written `@dx,dy` are relative to the first city.
//! - `city`: open the first city's screen.
//! - `btn <name>`: press a city-screen button: `Change`, `Close`,
//!   `Governor`, `CloseMenu`, `Prev`, `Next`, `Pick:<item>`,
//!   `Queue:<item>`, `Unqueue:<i>`.
//! - `hover <x>,<y>` / `unhover`: pin the map hover (no mouse in captures).
//! - `down` / `up`: press and release the left mouse button.
//! - `tile <rx>,<ry>`: click a tile of the open city's radius.
//!
//! Example: `CIV3_SCRIPT='20:key B;40:city;60:btn Change'`.

use bevy::prelude::*;

use crate::cities::{self, City, CityView, ScreenButton};
use crate::map::GameMap;
use crate::units::{Selected, TurnEnded, Unit, UnitType};

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
        "F9" => KeyCode::F9,
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

fn unit_type(name: &str) -> Option<UnitType> {
    Some(match name {
        "Settler" => UnitType::Settler,
        "Worker" => UnitType::Worker,
        "Warrior" => UnitType::Warrior,
        "Scout" => UnitType::Scout,
        _ => return None,
    })
}

fn button_matches(b: &ScreenButton, name: &str) -> bool {
    let (kind, arg) = name.split_once(':').unwrap_or((name, ""));
    match b {
        ScreenButton::Close => kind == "Close",
        ScreenButton::Change => kind == "Change",
        ScreenButton::Governor => kind == "Governor",
        ScreenButton::CloseMenu => kind == "CloseMenu",
        ScreenButton::Pick(p) => kind == "Pick" && p.name() == arg,
        ScreenButton::Queue(p) => kind == "Queue" && p.name() == arg,
        ScreenButton::PrevCity => kind == "Prev",
        ScreenButton::NextCity => kind == "Next",
        ScreenButton::Unqueue(i) => kind == "Unqueue" && arg.parse() == Ok(*i),
    }
}

/// Runs first in `Update`, after input and UI focus have run in `PreUpdate`,
/// so injected keys read as `just_pressed` and pressed buttons as
/// `Changed<Interaction>` for the rest of the frame.
pub fn drive_script(
    script: Option<ResMut<Script>>,
    civs: Res<crate::civs::Civilizations>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut pin: ResMut<crate::input::HoverPin>,
    mut turn_end: MessageWriter<TurnEnded>,
    mut selected: ResMut<Selected>,
    mut units: Query<(Entity, &mut Unit)>,
    mut map: ResMut<GameMap>,
    mut cities: Query<&mut City>,
    city_ids: Query<Entity, With<City>>,
    mut view: ResMut<CityView>,
    mut buttons: Query<(&mut Interaction, &ScreenButton)>,
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
            "key" => match key_code(arg) {
                Some(k) => {
                    keys.press(k);
                    script.held.push(k);
                }
                None => eprintln!("script: unknown key {arg}"),
            },
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
            "imp" => {
                let (pos, kind) = arg.split_once(' ').unwrap_or((arg, ""));
                if let Some((x, y)) = coord(pos) {
                    let i = map.idx(x, y);
                    let t = &mut map.tiles[i];
                    match kind {
                        "road" => t.road = true,
                        "irr" => t.irrigation = true,
                        "mine" => t.mine = true,
                        _ => eprintln!("script: unknown improvement {kind}"),
                    }
                }
            }
            "hover" => match coord(arg) {
                Some(p) => pin.0 = Some(p),
                None => eprintln!("script: bad hover {arg}"),
            },
            "unhover" => pin.0 = None,
            "down" => mouse.press(MouseButton::Left),
            "up" => mouse.release(MouseButton::Left),
            "city" => {
                view.0 = city_ids
                    .iter()
                    .find(|e| cities.get(*e).is_ok_and(|c| c.civ == civs.active))
            }
            "btn" => match buttons.iter_mut().find(|(_, b)| button_matches(b, arg)) {
                Some((mut i, _)) => *i = Interaction::Pressed,
                None => eprintln!("script: no button {arg}"),
            },
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
