//! Authoritative host. The simulation remains a portable, Bevy-free Rust library.
use fourx_content::{BASE_SCRIPT, Pack};
use fourx_sim::{Command, Game, Id, PROTOCOL_VERSION, Request, Response, TickRules};
use omnilua::{Lua, LuaSerdeExt, SandboxConfig};

#[derive(Debug, thiserror::Error)]
pub enum HostError {
    #[error(transparent)]
    Script(#[from] omnilua::Error),
    #[error(transparent)]
    Game(#[from] fourx_sim::GameError),
    #[error(transparent)]
    Content(#[from] fourx_content::ContentError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Invalid(String),
}

pub struct Host {
    pub game: Game,
    pub pack: Pack,
    script: String,
}
impl Host {
    pub fn base(seed: u64) -> Result<Self, HostError> {
        Self::new(Pack::base(), BASE_SCRIPT.into(), seed)
    }
    pub fn new(pack: Pack, script: String, seed: u64) -> Result<Self, HostError> {
        pack.validate()?;
        let game = pack.scenario.as_ref().map_or_else(
            || Game::new(seed, &pack.rules),
            |s| Game::from_scenario(seed, &pack.rules, s),
        );
        let host = Self { game, pack, script };
        host.tick_rules()?;
        Ok(host)
    }
    fn tick_rules(&self) -> Result<TickRules, HostError> {
        // A fresh sandbox avoids hidden VM state in saves and replays.
        let mut config = SandboxConfig::strict();
        config.instruction_limit = Some(200_000);
        config.memory_limit_bytes = Some(8 * 1024 * 1024);
        config.check_interval = 100;
        config.remove_globals.extend([
            b"math.random".to_vec(),
            b"math.randomseed".to_vec(),
            b"collectgarbage".to_vec(),
            b"os".to_vec(),
            b"print".to_vec(),
            b"warn".to_vec(),
        ]);
        let (lua, _sandbox) = Lua::sandboxed(config)?;
        lua.load(&self.script).set_name(&self.pack.script).exec()?;
        let function: omnilua::Function = lua.globals().get("on_turn")?;
        let context = lua.create_table()?;
        context.set("turn", self.game.turn + 1)?;
        context.set("seed", self.game.seed.to_string())?;
        context.set("city_count", self.game.cities.len())?;
        context.set("campaign", lua.to_value(&serde_json::json!({"turn":self.game.turn,"cities":self.game.cities,"armies":self.game.armies,"factions":self.game.factions}))?)?;
        let value: omnilua::Value = function.call(context)?;
        let tick: TickRules = lua.from_value(value)?;
        if [tick.income_percent, tick.fire_percent, tick.shock_percent]
            .iter()
            .any(|n| !(25..=400).contains(n))
        {
            return Err(HostError::Invalid(
                "Script percentages must be between 25 and 400".into(),
            ));
        }
        Ok(tick)
    }
    pub fn command(&mut self, player: Id, command: Command) -> Result<(), HostError> {
        let tick = if matches!(command, Command::EndTurn) {
            self.tick_rules()?
        } else {
            TickRules::default()
        };
        // Validation and script errors never partially mutate the live campaign.
        let mut candidate = self.game.clone();
        candidate.apply(player, command, &self.pack.rules, tick)?;
        self.game = candidate;
        Ok(())
    }
    pub fn request(&mut self, player: Id, request: Request) -> Response {
        let reason = if request.version != PROTOCOL_VERSION {
            Some("Incompatible protocol version".into())
        } else if request.revision != self.game.revision {
            Some("Stale state; wait for the latest snapshot".into())
        } else {
            self.command(player, request.command)
                .err()
                .map(|e| e.to_string())
        };
        if let Some(reason) = reason {
            Response::Rejected {
                sequence: request.sequence,
                reason,
            }
        } else {
            self.snapshot(player)
        }
    }
    pub fn snapshot(&self, player: Id) -> Response {
        Response::Snapshot {
            version: PROTOCOL_VERSION,
            player,
            game: self.game.view(player),
            rules: self.pack.rules.clone(),
        }
    }
    pub fn save(&self) -> Result<String, HostError> {
        Ok(serde_json::to_string_pretty(
            &serde_json::json!({"format":1,"pack":self.pack,"script":self.script,"game":self.game}),
        )?)
    }
    pub fn load(json: &str) -> Result<Self, HostError> {
        #[derive(serde::Deserialize)]
        struct Save {
            format: u32,
            pack: Pack,
            script: String,
            game: Game,
        }
        let saved: Save = serde_json::from_str(json)?;
        if saved.format != 1 {
            return Err(HostError::Invalid("Unsupported save format".into()));
        }
        // Saves are trusted local files, never accepted through the command protocol.
        let mut host = Self::new(saved.pack, saved.script, saved.game.seed)?;
        host.game = saved.game;
        host.tick_rules()?;
        Ok(host)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshot_json_roundtrip() {
        let h = Host::base(42).unwrap();
        for player in [0, 1] {
            let response = h.snapshot(player);
            let json = serde_json::to_string(&response).unwrap();
            let restored: Response = serde_json::from_str(&json).unwrap();
            assert_eq!(serde_json::to_string(&restored).unwrap(), json);
        }
    }
    #[test]
    fn deterministic_save_and_replay() {
        let mut a = Host::base(42).unwrap();
        let mut b = Host::base(42).unwrap();
        for _ in 0..2 {
            a.command(1, Command::EndTurn).unwrap();
            b.command(1, Command::EndTurn).unwrap();
        }
        assert_eq!(a.game, b.game);
        let mut c = Host::load(&a.save().unwrap()).unwrap();
        a.command(1, Command::EndTurn).unwrap();
        c.command(1, Command::EndTurn).unwrap();
        assert_eq!(a.game, c.game);
    }
    #[test]
    fn invalid_command_is_atomic() {
        let mut h = Host::base(1).unwrap();
        let before = h.game.clone();
        assert!(
            h.command(
                1,
                Command::Move {
                    army: 999,
                    destination: fourx_sim::terrain::Coord::new(0, 0)
                }
            )
            .is_err()
        );
        assert_eq!(before, h.game);
    }
    #[test]
    fn stale_revision_and_authority_checked() {
        let mut h = Host::base(1).unwrap();
        let r = Request {
            version: 1,
            sequence: 1,
            revision: 123,
            command: Command::EndTurn,
        };
        assert!(matches!(h.request(1, r), Response::Rejected { .. }));
        assert!(h.command(0, Command::EndTurn).is_err());
        let enemy = h.game.armies.values().find(|a| a.owner == 2).unwrap().id;
        assert!(
            h.command(
                1,
                Command::Move {
                    army: enemy,
                    destination: fourx_sim::terrain::Coord::new(10, 9)
                }
            )
            .is_err()
        );
    }
    #[test]
    fn scripts_change_economy() {
        let mut a = Host::base(1).unwrap();
        let script = "function on_turn(c) return {income_percent=200,fire_percent=100,shock_percent=100} end";
        let mut b = Host::new(Pack::base(), script.into(), 1).unwrap();
        a.command(1, Command::EndTurn).unwrap();
        b.command(1, Command::EndTurn).unwrap();
        assert!(b.game.factions[&1].gold > a.game.factions[&1].gold);
    }
    #[test]
    fn failed_turn_script_preserves_state() {
        let script = "function on_turn(c) if c.turn >= 3 then error('a scripted failure') end return {income_percent=100,fire_percent=100,shock_percent=100} end";
        let mut host = Host::new(Pack::base(), script.into(), 42).unwrap();
        host.command(1, Command::EndTurn).unwrap();
        let before = host.game.clone();
        assert!(host.command(1, Command::EndTurn).is_err());
        assert_eq!(before, host.game);
    }
    #[test]
    fn infinite_script_and_unsafe_apis_rejected() {
        assert!(Host::new(Pack::base(), "while true do end".into(), 1).is_err());
        assert!(Host::new(Pack::base(), "os.execute('echo unsafe')".into(), 1).is_err());
        assert!(Host::new(Pack::base(), "print('pollute headless stdout')".into(), 1).is_err());
    }
    #[test]
    fn hidden_enemies_not_in_player_snapshot() {
        let h = Host::base(1).unwrap();
        let v = h.game.view(1);
        assert!(
            v.armies
                .values()
                .all(|a| a.owner == 1 || h.pack.rules.units[&a.regiments[0].kind].naval)
        );
        assert_eq!(v.rng, 0);
    }
    #[test]
    fn settlement_production_and_victory() {
        use fourx_sim::terrain::Coord;
        let mut h = Host::base(7).unwrap();
        let pioneer = h
            .game
            .armies
            .values()
            .find(|a| a.owner == 1 && a.regiments[0].kind == "pioneer")
            .unwrap()
            .id;
        // Make a known fixture site three tiles away, then exercise real movement/settlement.
        for x in 8..=11 {
            h.game.map.get_mut(Coord::new(x, 9)).unwrap().terrain =
                fourx_sim::terrain::Terrain::Grass;
            h.game.map.get_mut(Coord::new(x, 9)).unwrap().mountain = false;
        }
        h.command(
            1,
            Command::Move {
                army: pioneer,
                destination: Coord::new(11, 9),
            },
        )
        .unwrap();
        h.command(
            1,
            Command::FoundCity {
                army: pioneer,
                name: "Harbor".into(),
            },
        )
        .unwrap();
        let city = h
            .game
            .cities
            .values()
            .find(|c| c.name == "Harbor")
            .unwrap()
            .id;
        h.command(
            1,
            Command::Produce {
                city,
                unit: "infantry".into(),
            },
        )
        .unwrap();
        for _ in 0..3 {
            h.command(1, Command::EndTurn).unwrap();
        }
        assert!(
            h.game
                .armies
                .values()
                .any(|a| a.owner == 1 && a.position == Coord::new(11, 9))
        );
        h.game.factions.get_mut(&1).unwrap().industry = 1000;
        h.command(1, Command::EndTurn).unwrap();
        assert_eq!(h.game.winner, Some(1));
    }
    #[test]
    fn battle_has_fire_shock_losses_and_resolution() {
        let mut h = Host::base(9).unwrap();
        let a = h
            .game
            .armies
            .values()
            .find(|a| a.owner == 1 && a.regiments.len() > 1)
            .unwrap()
            .id;
        let d = h
            .game
            .armies
            .values()
            .find(|a| a.owner == 2 && a.regiments.len() > 1)
            .unwrap()
            .id;
        h.game.armies.get_mut(&d).unwrap().position = fourx_sim::terrain::Coord::new(9, 9);
        h.game
            .map
            .get_mut(fourx_sim::terrain::Coord::new(9, 9))
            .unwrap()
            .terrain = fourx_sim::terrain::Terrain::Grass;
        h.command(
            1,
            Command::Move {
                army: a,
                destination: fourx_sim::terrain::Coord::new(9, 9),
            },
        )
        .unwrap();
        assert_eq!(h.game.battles.len(), 1);
        for _ in 0..4 {
            h.command(1, Command::EndTurn).unwrap();
        }
        assert_eq!(h.game.battles[0].phase, "Shock");
        assert!(h.game.battles[0].attacker_losses > 0);
        for _ in 0..30 {
            if h.game.battles.is_empty() || h.game.winner.is_some() {
                break;
            }
            h.command(1, Command::EndTurn).unwrap();
        }
        assert!(h.game.battles.is_empty());
    }
}
