//! Save and load: F5 writes the game to `saves/quicksave.json` (or the file
//! named by `CIV3_SAVE`), F8 reads it back.
//!
//! A save holds what play has changed: the map's tiles, every city and unit,
//! the books of the civilizations (treasury, research, diplomacy, realm,
//! wonders, barbarian camps, flip ratings) and the dice. What the game
//! rebuilds from those each frame is left out: visibility, borders, tile
//! improvement sprites, the realm's derived tables.
//!
//! Not kept: trades on offer, questions waiting for an answer, a combat in
//! progress (a save is refused while one runs), what the computer was
//! planning mid-turn (it plans again). Exploration history is kept for every
//! civilization that has viewed the map.

use bevy::ecs::world::CommandQueue;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::barbarians::{self, Barbarians, Tribe};
use crate::cities::{self, Capital, City, CityNamesUsed, Treasury};
use crate::civs::{CIV_COUNT, Civilizations, Outcome};
use crate::diplomacy::{self, Diplomacy};
use crate::features::{MessageBoard, post};
use crate::flip::Flips;
use crate::map::{GameMap, Tile};
use crate::research::Research;
use crate::units::{Exploration, Selected, Turn, Unit};
use crate::wonders::{self, Wonders};
use crate::{realm, rng};

const VERSION: u32 = 11;

/// A request from the keyboard or a script.
#[derive(Message, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Command {
    Save,
    Load,
}

/// Where the quick save lives.
pub fn path() -> std::path::PathBuf {
    std::env::var("CIV3_SAVE").map(Into::into).unwrap_or_else(|_| "saves/quicksave.json".into())
}

#[derive(Serialize, Deserialize)]
struct UnitSave {
    unit: Unit,
    /// Index into `units` of the ship carrying it.
    carrier: Option<usize>,
    tribe: Option<u8>,
}

#[derive(Serialize, Deserialize)]
struct CivsSave {
    active: usize,
    last_human: usize,
    eliminated: Vec<bool>,
    outcome: Option<(u8, usize)>,
}

/// A whole game.
#[derive(Serialize, Deserialize)]
pub struct Save {
    version: u32,
    turn: u32,
    seed: u64,
    start: (i32, i32),
    width: i32,
    height: i32,
    tiles: Vec<Tile>,
    players: [usize; CIV_COUNT],
    exploration: Exploration,
    cities: Vec<City>,
    units: Vec<UnitSave>,
    /// Index into `cities` of each civ's capital.
    capital: Vec<Option<usize>>,
    names: Vec<usize>,
    treasury: Vec<u32>,
    flips: Vec<u32>,
    civs: CivsSave,
    research: Vec<i64>,
    diplomacy: diplomacy::Saved,
    realms: Vec<realm::Saved>,
    wonders: wonders::Saved,
    barbarians: barbarians::Saved,
    game_rng: u32,
    combat_rng: u32,
}

fn outcome_code(o: Outcome) -> (u8, usize) {
    match o {
        Outcome::Victory(c) => (0, c),
        Outcome::Domination(c) => (1, c),
        Outcome::Defeat => (2, 0),
    }
}

fn outcome_from(code: (u8, usize)) -> Option<Outcome> {
    Some(match code.0 {
        0 => Outcome::Victory(code.1),
        1 => Outcome::Domination(code.1),
        2 => Outcome::Defeat,
        _ => return None,
    })
}

impl Save {
    /// The game as it stands.
    pub fn capture(world: &mut World) -> Save {
        let mut q = world.query::<(Entity, &City)>();
        let mut city_list: Vec<(Entity, City)> = q.iter(world).map(|(e, c)| (e, c.clone())).collect();
        // The world's iteration order is an accident; the file should not be.
        city_list.sort_by_key(|(_, c)| (c.founded, c.civ, c.x, c.y));
        let mut q = world.query::<(Entity, &Unit, Option<&Tribe>)>();
        let mut unit_list: Vec<(Entity, Unit, Option<u8>)> =
            q.iter(world).map(|(e, u, t)| (e, u.clone(), t.map(|t| t.0))).collect();
        unit_list.sort_by_key(|(e, u, _)| (u.civ, u.x, u.y, u.utype.0, e.to_bits()));
        let unit_at = |e: Entity| unit_list.iter().position(|(o, _, _)| *o == e);
        let units = unit_list
            .iter()
            .map(|(_, u, tribe)| UnitSave { unit: u.clone(), carrier: u.carrier.and_then(unit_at), tribe: *tribe })
            .collect();
        let capital_res = world.resource::<Capital>();
        let capital = capital_res.0.iter().map(|c| c.and_then(|e| city_list.iter().position(|(o, _)| *o == e))).collect();
        let map = world.resource::<GameMap>();
        let civs = world.resource::<Civilizations>();
        Save {
            version: VERSION,
            turn: world.resource::<Turn>().0,
            seed: map.seed,
            start: map.start,
            width: map.w,
            height: map.h,
            tiles: map.tiles.clone(),
            players: crate::civs::players(),
            exploration: world.resource::<Exploration>().snapshot(map),
            cities: city_list.into_iter().map(|(_, c)| c).collect(),
            units,
            capital,
            names: world.resource::<CityNamesUsed>().0.to_vec(),
            treasury: world.resource::<Treasury>().0.to_vec(),
            flips: world.resource::<Flips>().empire.to_vec(),
            civs: CivsSave {
                active: civs.active,
                last_human: civs.last_human,
                eliminated: civs.eliminated.to_vec(),
                outcome: civs.outcome.map(outcome_code),
            },
            research: world.resource::<Research>().snapshot(),
            diplomacy: world.resource::<Diplomacy>().snapshot(),
            realms: (0..CIV_COUNT).map(|c| realm::read(c, |r| r.snapshot())).collect(),
            wonders: world.resource::<Wonders>().snapshot(),
            barbarians: world.resource::<Barbarians>().snapshot(),
            game_rng: world.resource::<rng::GameRng>().state(),
            combat_rng: world.resource::<crate::combat::CombatRng>().0.state(),
        }
    }

    /// Why this save cannot be loaded into the running game, if it cannot.
    fn check(&self, map: &GameMap) -> Result<(), String> {
        if self.version != VERSION {
            return Err(format!("save version {} is not {VERSION}", self.version));
        }
        if self.players != crate::civs::players() {
            return Err("saved with different civilizations".into());
        }
        if (self.seed, self.width, self.height) != (map.seed, map.w, map.h) {
            return Err("saved on a different map".into());
        }
        let per_civ = [self.names.len(), self.treasury.len(), self.flips.len(), self.realms.len(), self.capital.len(), self.civs.eliminated.len()];
        if self.tiles.len() != map.tiles.len() || per_civ.iter().any(|&n| n != CIV_COUNT) {
            return Err("the save does not fit this game".into());
        }
        if self.civs.active >= CIV_COUNT || self.civs.last_human >= CIV_COUNT {
            return Err("the save names a civilization that does not exist".into());
        }
        if self.exploration.seen.iter().any(|s| s.len() != map.tiles.len())
            || self.exploration.viewer.is_some_and(|c| c >= CIV_COUNT)
        {
            return Err("the exploration history does not fit this map".into());
        }
        if self.cities.iter().any(|c| c.civ >= CIV_COUNT)
            || self.units.iter().any(|u| u.unit.civ > CIV_COUNT || u.carrier.is_some_and(|i| i >= self.units.len()))
            || self.capital.iter().flatten().any(|&i| i >= self.cities.len())
        {
            return Err("the save names something that does not exist".into());
        }
        Ok(())
    }

    /// Put the game back as saved, replacing every city and unit. Nothing
    /// changes when the save does not fit.
    pub fn apply(&self, world: &mut World) -> Result<(), String> {
        self.check(world.resource::<GameMap>())?;
        // Decode both books before replacing anything. A malformed second
        // book must not leave the first one partially loaded.
        let mut research = Research::new();
        if !research.restore(&self.research) {
            return Err("the research books do not fit".into());
        }
        let mut diplomacy = Diplomacy::new();
        if !diplomacy.restore(&self.diplomacy) {
            return Err("the diplomatic books do not fit".into());
        }
        diplomacy.land = world.resource::<Diplomacy>().land.clone();
        world.insert_resource(research);
        world.resource::<Research>().goods_changed();
        world.insert_resource(diplomacy);
        world.insert_resource(self.exploration.clone());

        // The old cast goes: cities, units and everything drawn for them.
        let mut doomed: Vec<Entity> = vec![];
        let mut q = world.query_filtered::<Entity, Or<(With<City>, With<Unit>, With<cities::CitySprite>, With<cities::CityLabelBack>, With<crate::production_prompt::PromptRoot>, With<crate::build_switch::SwitchRoot>, With<crate::abandon::AbandonRoot>)>>();
        doomed.extend(q.iter(world));
        for e in doomed {
            if let Ok(e) = world.get_entity_mut(e) {
                e.despawn();
            }
        }
        let mut q = world.query::<(Entity, &crate::features::FeatureSprite)>();
        let sprites: Vec<Entity> = q.iter(world).map(|(e, _)| e).collect();
        for e in sprites {
            world.despawn(e);
        }
        let mut q = world.query_filtered::<Entity, With<crate::unit_picker::PickerBackdrop>>();
        let pickers: Vec<Entity> = q.iter(world).collect();
        for e in pickers {
            world.despawn(e);
        }
        let mut q = world.query_filtered::<Entity, Or<(With<crate::advisors::AdvisorRoot>, With<crate::domestic::DomesticRoot>, With<cities::CityScreenRoot>)>>();
        let panels: Vec<Entity> = q.iter(world).collect();
        for e in panels {
            world.despawn(e);
        }
        let mut q = world.query_filtered::<Entity, With<crate::render::TerrainLayer>>();
        let terrain: Vec<Entity> = q.iter(world).collect();
        for e in terrain {
            world.despawn(e);
        }

        {
            let mut map = world.resource_mut::<GameMap>();
            map.tiles = self.tiles.clone();
            map.start = self.start;
        }
        // The loader recomputes every border (`0x5D2A96`), which also
        // settles saves written before tiles carried an owner.
        world.insert_resource(crate::cities::BorderKey::default());
        world.insert_resource(Turn(self.turn));
        world.insert_resource(Civilizations {
            active: self.civs.active,
            last_human: self.civs.last_human,
            eliminated: std::array::from_fn(|i| self.civs.eliminated[i]),
            outcome: self.civs.outcome.and_then(outcome_from),
        });
        world.insert_resource(CityNamesUsed(std::array::from_fn(|i| self.names[i])));
        world.insert_resource(Treasury(std::array::from_fn(|i| self.treasury[i])));
        world.insert_resource(Flips { empire: std::array::from_fn(|i| self.flips[i]) });
        world.resource_mut::<Wonders>().restore(&self.wonders);
        world.resource_mut::<Barbarians>().restore(&self.barbarians);
        world.insert_resource(rng::GameRng::new(self.game_rng));
        world.insert_resource(crate::combat::CombatRng(rng::MapRng::new(self.combat_rng)));
        for (civ, saved) in self.realms.iter().enumerate() {
            realm::write(civ, |r| r.restore(saved));
        }
        // Whatever was on screen or in mid-plan belonged to the old game.
        world.insert_resource(Selected(None));
        world.insert_resource(cities::CityView::default());
        world.insert_resource(cities::BuildMenu::default());
        world.insert_resource(cities::HurryAsk::default());
        world.insert_resource(crate::build_switch::BuildSwitch::default());
        world.insert_resource(crate::abandon::Abandon::default());
        world.insert_resource(crate::ai::AiState::default());
        world.insert_resource(crate::combat::ActiveCombat::default());
        world.insert_resource(crate::production_prompt::ProductionPrompts::default());
        world.insert_resource(crate::unit_picker::UnitPicker::default());
        world.insert_resource(crate::advisors::Advisors::default());
        world.insert_resource(crate::domestic::Domestic::default());
        world.insert_resource(crate::actionbar::GotoMode::default());
        world.insert_resource(crate::bombard::TargetMode::default());
        world.insert_resource(crate::input::MovePreview::default());

        // The new cast. Entities are reserved up front so the capital and
        // the carriers can name them before the commands run.
        let viewer = world.resource::<Civilizations>().viewer();
        let mut queue = CommandQueue::default();
        let (city_ids, unit_ids) = {
            let mut commands = Commands::new(&mut queue, world);
            let map = world.resource::<GameMap>();
            let city_art = world.get_resource::<cities::CityArt>();
            let assets = world.get_resource::<AssetServer>();
            let star = world.get_resource::<cities::CapitalStar>();
            let mut city_ids = vec![];
            for (i, city) in self.cities.iter().enumerate() {
                let e = commands.spawn(city.clone()).id();
                city_ids.push(e);
                let is_capital = self.capital[city.civ] == Some(i);
                if let (Some(art), Some(assets), Some(star)) = (city_art, assets, star) {
                    cities::spawn_city_visuals(&mut commands, art, assets, star, map, e, city, is_capital, viewer);
                }
            }
            let unit_art = world.get_resource::<crate::units::UnitArt>();
            let mut unit_ids = vec![];
            for saved in &self.units {
                let u = &saved.unit;
                let e = match unit_art {
                    Some(art) => crate::units::spawn_unit_at_level(&mut commands, art, u.utype, u.x, u.y, u.civ, u.level),
                    None => commands.spawn_empty().id(),
                };
                unit_ids.push(e);
            }
            for (saved, &e) in self.units.iter().zip(&unit_ids) {
                let mut unit = saved.unit.clone();
                unit.carrier = saved.carrier.map(|i| unit_ids[i]);
                commands.entity(e).insert(unit);
                if let Some(t) = saved.tribe {
                    commands.entity(e).insert(Tribe(t));
                }
            }
            (city_ids, unit_ids)
        };
        queue.apply(world);
        let _ = unit_ids;
        world.insert_resource(Capital(std::array::from_fn(|i| self.capital[i].map(|c| city_ids[c]))));
        if world.contains_resource::<crate::features::FeatureArt>() {
            let _ = world.run_system_cached(crate::features::spawn_features);
        }
        if world.contains_resource::<crate::tiles::TileArt>() && world.contains_resource::<AssetServer>() {
            let _ = world.run_system_cached(crate::render::spawn_terrain);
        }
        Ok(())
    }
}

/// F5 saves, F8 loads; scripts send the same commands.
pub fn keys(keys: Res<ButtonInput<KeyCode>>, mut out: MessageWriter<Command>) {
    if keys.just_pressed(KeyCode::F5) {
        out.write(Command::Save);
    }
    if keys.just_pressed(KeyCode::F8) {
        out.write(Command::Load);
    }
}

/// Carry out the requests.
pub fn run(world: &mut World) {
    let requests: Vec<Command> = world.resource_mut::<Messages<Command>>().drain().collect();
    for request in requests {
        let busy = world.resource::<crate::combat::ActiveCombat>().0.is_some() || world.resource::<Barbarians>().phase;
        let note = if busy {
            "Wait for the fighting to end.".to_string()
        } else {
            match request {
                Command::Save => match write(world) {
                    Ok(p) => format!("Game saved to {}.", p.display()),
                    Err(e) => format!("Could not save: {e}"),
                },
                Command::Load => match read(world) {
                    Ok(p) => format!("Game loaded from {}.", p.display()),
                    Err(e) => format!("Could not load: {e}"),
                },
            }
        };
        println!("{note}");
        post(&mut world.resource_mut::<MessageBoard>(), note);
    }
}

fn write(world: &mut World) -> Result<std::path::PathBuf, String> {
    let path = path();
    let text = serde_json::to_string(&Save::capture(world)).map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(path)
}

fn read(world: &mut World) -> Result<std::path::PathBuf, String> {
    let path = path();
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let save: Save = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    save.apply(world)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cities::Production;
    use crate::units::{UnitArt, UnitType};

    /// A game with the resources a save touches and no art.
    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(GameMap::generate());
        app.init_resource::<Civilizations>();
        app.init_resource::<Exploration>();
        app.insert_resource(Turn(1));
        app.init_resource::<CityNamesUsed>();
        app.init_resource::<Treasury>();
        app.init_resource::<Flips>();
        app.init_resource::<Capital>();
        app.init_resource::<Wonders>();
        app.init_resource::<Barbarians>();
        app.insert_resource(Research::new());
        app.insert_resource(Diplomacy::new());
        app.insert_resource(rng::GameRng::new(7));
        app.insert_resource(crate::combat::CombatRng(rng::MapRng::new(9)));
        app.insert_resource(UnitArt::blank());
        app.add_message::<Command>();
        app
    }

    #[test]
    fn a_saved_game_comes_back_the_same() {
        crate::civs::set_controllers();
        let mut app = app();
        let w = app.world_mut();
        let start = w.resource::<GameMap>().start;
        // Two cities, a capital, a ship with a passenger, a barbarian.
        let mut city = City::new(0, "Kyoto", start.0, start.1);
        city.set_size(4);
        city.diseased = true;
        city.set_specialists(vec![cities::Specialist::Scientist, cities::Specialist::TaxCollector]);
        city.lose_population(1, None, &mut w.resource_mut::<crate::combat::CombatRng>().0);
        assert!(city.citizens.slots().iter().any(Option::is_none));
        city.shields = 12;
        city.buildings.push(Production::Walls);
        city.queue.push(Production::Warrior);
        let capital = w.spawn(city).id();
        w.spawn(City::new(1, "Memphis", start.0 + 5, start.1));
        w.resource_mut::<Capital>().0[0] = Some(capital);
        let ship = w.spawn(Unit::new(0, UnitType::Settler, start.0, start.1)).id();
        let mut cargo = Unit::new(0, UnitType::Warrior, start.0, start.1);
        cargo.carrier = Some(ship);
        cargo.fortified = true;
        cargo.damage = 1;
        cargo.scientific_leader = true;
        w.spawn(cargo);
        let mut worker = Unit::new(1, UnitType::Worker, start.0 + 1, start.1);
        worker.civ = 0;
        worker.work = Some(crate::improvements::Work { action: crate::improvements::WorkAction::Mine, progress: 7 });
        w.spawn(worker);
        w.spawn((Unit::new(4, UnitType::Warrior, start.0 + 2, start.1), Tribe(3)));
        w.resource_mut::<Turn>().0 = 42;
        w.resource_mut::<Treasury>().0[0] = 321;
        w.resource_mut::<Civilizations>().eliminated[2] = true;
        w.resource_mut::<Flips>().empire[1] = 77;
        w.resource_mut::<GameMap>().tiles[30].road = true;
        w.resource_mut::<GameMap>().tiles[30].forest_harvested = true;
        w.resource_mut::<GameMap>().tiles[30].river = crate::rivers::EAST;
        w.resource_mut::<GameMap>().tiles[31].site = Some(crate::sites::Site::Colony(2));
        w.resource_mut::<GameMap>().tiles[31].fortress = true;
        w.resource_mut::<GameMap>().tiles[31].barricade = true;
        w.resource_mut::<GameMap>().tiles[32].site = Some(crate::sites::Site::Outpost(0));
        w.resource_mut::<Barbarians>().camps.insert((5, 6), 9);
        realm::write(0, |r| r.golden_end = Some(60));
        w.resource_mut::<Diplomacy>().meet(0, 1);
        w.resource_mut::<Research>().world.players[0].beakers = 11;
        w.resource_mut::<Research>().start_science_age(0, 42);

        let before = serde_json::to_string(&Save::capture(w)).unwrap();

        // Wreck the game, then load the save.
        let save: Save = serde_json::from_str(&before).unwrap();
        w.resource_mut::<Turn>().0 = 1;
        w.resource_mut::<Treasury>().0[0] = 0;
        w.resource_mut::<GameMap>().tiles[30].road = false;
        w.resource_mut::<Research>().world.players[0].beakers = 0;
        *w.resource_mut::<Diplomacy>() = Diplomacy::new();
        realm::write(0, |r| r.golden_end = None);
        let mut q = w.query::<Entity>();
        let all: Vec<Entity> = q.iter(w).collect();
        for e in all {
            if w.get::<Unit>(e).is_some() || w.get::<City>(e).is_some() {
                w.despawn(e);
            }
        }
        w.spawn(Unit::new(2, UnitType::Scout, 1, 1));
        let mut advisors = crate::advisors::Advisors::default();
        advisors.show(crate::advisors::Screen::Science);
        w.insert_resource(advisors);
        let stale_panel = w.spawn(crate::domestic::DomesticRoot).id();
        w.init_resource::<crate::build_switch::BuildSwitch>();
        let mut choice_city = City::new(0, "Old game", start.0, start.1);
        choice_city.shields = 30;
        let choice_entity = w.spawn(choice_city).id();
        w.resource_scope(|w, mut choice: Mut<crate::build_switch::BuildSwitch>| {
            choice.request(choice_entity, &mut w.get_mut::<City>(choice_entity).unwrap(), Production::Worker);
        });
        assert!(w.resource::<crate::build_switch::BuildSwitch>().is_pending());
        let stale_switch = w.spawn(crate::build_switch::SwitchRoot).id();
        w.init_resource::<crate::abandon::Abandon>();
        let old_city = w.get::<City>(choice_entity).unwrap().clone();
        w.resource_mut::<crate::abandon::Abandon>().push(choice_entity, &old_city);
        let stale_abandon = w.spawn(crate::abandon::AbandonRoot(choice_entity)).id();
        save.apply(w).unwrap();
        assert!(!w.resource::<crate::advisors::Advisors>().is_open());
        assert!(w.get_entity(stale_panel).is_err());
        assert!(!w.resource::<crate::build_switch::BuildSwitch>().is_pending());
        assert!(w.get_entity(stale_switch).is_err());
        assert!(!w.resource::<crate::abandon::Abandon>().blocks(0));
        assert!(w.get_entity(stale_abandon).is_err());

        let after = serde_json::to_string(&Save::capture(w)).unwrap();
        assert_eq!(before, after);
        assert_eq!(w.resource::<Turn>().0, 42);
        assert!(w.resource::<Diplomacy>().contact(0, 1));
        assert_eq!(w.resource::<Research>().world.players[0].beakers, 11);
        assert_eq!(realm::read(0, |r| r.golden_end), Some(60));
        assert!(w.resource::<GameMap>().tiles[30].road);
        assert_eq!(w.resource::<GameMap>().tiles[30].river, crate::rivers::EAST);
        let mut q = w.query::<(Entity, &Unit)>();
        let units: Vec<(Entity, Unit)> = q.iter(w).map(|(e, u)| (e, u.clone())).collect();
        assert_eq!(units.len(), 4, "the stray scout is gone");
        let worker = units.iter().find(|(_, u)| u.utype == UnitType::Worker).unwrap();
        assert_eq!(worker.1.nationality, crate::civs::roster_index(1));
        assert_eq!(worker.1.work.unwrap().progress, 7);
        let cargo = units.iter().find(|(_, u)| u.utype == UnitType::Warrior && u.civ == 0).unwrap();
        let ship = units.iter().find(|(_, u)| u.utype == UnitType::Settler).unwrap();
        assert_eq!(cargo.1.carrier, Some(ship.0), "the passenger is aboard the same ship");
        assert!(cargo.1.scientific_leader);
        assert!(w.resource::<Research>().science_age(0, 62));
        assert!(!w.resource::<Research>().science_age(0, 63));
        let capital = w.resource::<Capital>().0[0].expect("a capital");
        assert_eq!(w.get::<City>(capital).unwrap().name, "Kyoto");
    }

    #[test]
    fn a_save_for_another_map_is_refused() {
        let mut app = app();
        let w = app.world_mut();
        let mut save = Save::capture(w);
        save.seed += 1;
        assert!(save.apply(w).is_err());
        let mut save = Save::capture(w);
        save.civs.active = CIV_COUNT;
        assert!(save.apply(w).is_err());
        let mut save = Save::capture(w);
        save.research.pop();
        let words = w.resource::<Research>().snapshot();
        assert!(save.apply(w).is_err());
        assert_eq!(w.resource::<Research>().snapshot(), words, "a refused load changes nothing");
    }

    #[test]
    fn invalid_diplomacy_does_not_replace_valid_research() {
        let mut app = app();
        let w = app.world_mut();
        let mut file = serde_json::to_value(Save::capture(w)).unwrap();
        file["diplomacy"]["rel"] = serde_json::json!([]);
        let save: Save = serde_json::from_value(file).unwrap();
        w.resource_mut::<Research>().world.players[1].beakers = 73;
        let before = serde_json::to_string(&Save::capture(w)).unwrap();
        assert!(save.apply(w).is_err());
        assert_eq!(serde_json::to_string(&Save::capture(w)).unwrap(), before);
    }

    #[test]
    fn loading_an_older_game_relocks_units_unlocked_after_the_save() {
        let mut app = app();
        let w = app.world_mut();
        let bronze = crate::rules_data::TECH_NAMES.iter().position(|&t| t == "Bronze Working").unwrap() as i32;
        let save = Save::capture(w);
        w.resource_mut::<Research>().award(0, bronze, &mut rng::MapRng::new(7));
        assert!(crate::research::can_build(0, Production::Spearman));
        save.apply(w).unwrap();
        assert!(!crate::research::can_build(0, Production::Spearman));
    }

    #[test]
    fn loading_restores_private_exploration_without_later_discoveries() {
        // The test's default controllers are hotseat: switch viewers through
        // the same system that presents the map in gameplay.
        let mut app = app();
        app.add_systems(Update, crate::units::refresh_visibility);
        app.update();
        app.world_mut().resource_mut::<GameMap>().tiles[30].seen = true;
        app.world_mut().resource_mut::<Civilizations>().active = 1;
        app.update();
        assert!(!app.world().resource::<GameMap>().tiles[30].seen);
        app.world_mut().resource_mut::<GameMap>().tiles[31].seen = true;
        let save = Save::capture(app.world_mut());
        app.world_mut().resource_mut::<GameMap>().tiles[32].seen = true;
        app.update();
        save.apply(app.world_mut()).unwrap();
        app.update();
        let map = app.world().resource::<GameMap>();
        assert!(!map.tiles[30].seen);
        assert!(map.tiles[31].seen);
        assert!(!map.tiles[32].seen, "later exploration must be rolled back");
        app.world_mut().resource_mut::<Civilizations>().active = 0;
        app.update();
        let map = app.world().resource::<GameMap>();
        assert!(map.tiles[30].seen);
        assert!(!map.tiles[31].seen, "another civ's discoveries remain private");

        // Starting a fresh process has no previous system-local fog memory.
        let mut fresh = self::app();
        fresh.add_systems(Update, crate::units::refresh_visibility);
        save.apply(fresh.world_mut()).unwrap();
        fresh.update();
        assert!(fresh.world().resource::<GameMap>().tiles[31].seen);
    }

    #[test]
    fn a_different_roster_or_invalid_exploration_is_refused() {
        let mut app = app();
        let w = app.world_mut();
        let before = serde_json::to_string(&Save::capture(w)).unwrap();
        let mut save = Save::capture(w);
        save.players.swap(0, 1);
        assert!(save.apply(w).is_err());
        let mut save = Save::capture(w);
        save.exploration.seen[0].pop();
        assert!(save.apply(w).is_err());
        let mut save = Save::capture(w);
        save.exploration.viewer = Some(CIV_COUNT);
        assert!(save.apply(w).is_err());
        assert_eq!(serde_json::to_string(&Save::capture(w)).unwrap(), before);
    }
}
