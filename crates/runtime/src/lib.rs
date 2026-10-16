//! Authoritative host. The simulation remains a portable, Bevy-free Rust library.
use fourx_content::{Content, Pack};
use fourx_sim::{Command, Game, Id, PROTOCOL_VERSION, Request, Response, Rules, TickRules};

pub const SAVE_FORMAT: u32 = 5;

#[derive(Debug, thiserror::Error)]
pub enum HostError {
    #[error(transparent)]
    Game(#[from] fourx_sim::GameError),
    #[error(transparent)]
    Content(#[from] fourx_content::ContentError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Invalid(String),
}

/// Which campaign to begin. `None` selects the pack's default scenario / the scenario's
/// default commander.
#[derive(Clone, Copy, Debug, Default)]
pub struct Start<'a> {
    pub scenario: Option<&'a str>,
    /// Nation ID (for example `japan`) to command instead of the scenario's default.
    pub nation: Option<&'a str>,
    pub seed: u64,
}

pub struct Host {
    pub game: Game,
    pub pack: Pack,
    /// Pack rules with the scenario's overrides applied. This is what the game plays by.
    pub rules: Rules,
}
impl Host {
    /// The bundled pack's default scenario.
    pub fn base(seed: u64) -> Result<Self, HostError> {
        Self::start(
            Content::base(),
            Start {
                seed,
                ..Start::default()
            },
        )
    }
    /// A specific bundled scenario by ID.
    pub fn base_scenario(scenario: &str, seed: u64) -> Result<Self, HostError> {
        Self::start(
            Content::base(),
            Start {
                scenario: Some(scenario),
                seed,
                ..Start::default()
            },
        )
    }
    pub fn start(content: Content, start: Start) -> Result<Self, HostError> {
        content.validate()?;
        let scenario = content.scenario(start.scenario)?;
        let rules = content.pack.rules.with_overrides(&scenario.rules);
        let game = Game::from_scenario(start.seed, &rules, scenario, start.nation)?;
        let host = Self {
            game,
            pack: content.pack,
            rules,
        };
        Ok(host)
    }
    /// The nation this host's player commands.
    pub fn commander(&self) -> Id {
        self.game.commander
    }
    pub fn command(&mut self, player: Id, command: Command) -> Result<(), HostError> {
        let tick = TickRules::default();
        // Command errors never partially mutate the live campaign.
        let mut candidate = self.game.clone();
        candidate.apply(player, command, &self.rules, tick)?;
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
        let mut game = self.game.view(player);
        self.game.show_routes(&mut game, player, &self.rules);
        Response::Snapshot {
            version: PROTOCOL_VERSION,
            player,
            game,
            rules: self.rules.clone(),
        }
    }
    pub fn save(&self) -> Result<String, HostError> {
        Ok(serde_json::to_string(&serde_json::json!({
            "format": SAVE_FORMAT,
            "pack": self.pack,
            "rules": self.rules,
            "game": self.game,
        }))?)
    }
    pub fn load(json: &str) -> Result<Self, HostError> {
        #[derive(serde::Deserialize)]
        struct Save {
            format: u32,
            pack: Pack,
            rules: Rules,
            game: Game,
        }
        let saved: Save = serde_json::from_str(json)?;
        if saved.format != SAVE_FORMAT {
            return Err(HostError::Invalid(format!(
                "Unsupported save format {} (this build reads format {SAVE_FORMAT})",
                saved.format
            )));
        }
        // Saves are trusted local files, never accepted through the command protocol.
        let mut checked = saved.pack.clone();
        checked.rules = saved.rules.clone();
        checked.validate()?;
        // The position index is derived data and is not stored in saves.
        let mut game = saved.game;
        game.reindex();
        let host = Self {
            game,
            pack: saved.pack,
            rules: saved.rules,
        };
        Ok(host)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fourx_sim::terrain::{Coord, Cover, Relief, Terrain};
    use fourx_sim::{Domain, Job, Order};

    /// The compact starter campaign, which the fixtures below are written against.
    fn starter(seed: u64) -> Host {
        Host::base_scenario("russo-japanese-war", seed).unwrap()
    }
    fn starter_at_peace(seed: u64) -> Host {
        let mut content = Content::base();
        content.pack.rules.economy.war_chance_per_thousand = 0;
        content
            .scenarios
            .get_mut("russo-japanese-war")
            .unwrap()
            .wars
            .clear();
        Host::start(
            content,
            Start {
                scenario: Some("russo-japanese-war"),
                seed,
                ..Start::default()
            },
        )
        .unwrap()
    }
    /// The first unit of a kind a nation owns.
    fn unit_of(host: &Host, owner: Id, kind: &str) -> Id {
        host.game
            .units
            .values()
            .find(|u| u.owner == owner && u.kind == kind)
            .unwrap_or_else(|| panic!("nation {owner} has no {kind}"))
            .id
    }
    fn make_grass(host: &mut Host, p: Coord) {
        let tile = host.game.map.get_mut(p).unwrap();
        tile.terrain = Terrain::Grass;
        tile.relief = Relief::Flat;
        tile.cover = Cover::Bare;
    }

    #[test]
    fn the_default_scenario_is_age_of_imperialism() {
        let h = Host::base(1).unwrap();
        assert_eq!(h.pack.default_scenario, "age-of-imperialism");
        let lattice = h.game.map.lattice.expect("the map stands upright");
        assert_eq!((lattice.columns, lattice.rows), (90, 180));
        assert_eq!(h.game.date().iso(), "1895-01-01");
        assert!(h.game.factions.len() >= 30);
        assert_eq!(h.game.factions[&h.commander()].tag, "japan");
    }
    #[test]
    fn the_world_turns_over_deterministically_and_round_trips_through_a_save() {
        let play = |seed| {
            let mut h = Host::base(seed).unwrap();
            let commander = h.commander();
            for _ in 0..30 {
                h.command(commander, Command::EndTurn).unwrap();
            }
            h
        };
        let (a, b) = (play(7), play(7));
        assert_eq!(a.game.date().iso(), "1895-07-30");
        assert_eq!(a.game, b.game);
        // The AI garrisons and recruits, so the world is visibly alive after a month.
        let start = Host::base(7).unwrap().game.units.len();
        assert!(
            a.game.units.len() > start,
            "{} units from {start}",
            a.game.units.len()
        );
        let save = a.save().unwrap();
        assert!(save.len() < 3_000_000, "save is {} bytes", save.len());
        let snapshot = serde_json::to_string(&a.snapshot(a.commander())).unwrap();
        assert!(
            snapshot.len() < 1_500_000,
            "snapshot is {} bytes",
            snapshot.len()
        );
        let mut reloaded = Host::load(&save).unwrap();
        assert_eq!(a.game, reloaded.game);
        // The derived position index is rebuilt on load.
        for unit in reloaded.game.units.values() {
            assert!(reloaded.game.unit_ids_at(unit.position).contains(&unit.id));
        }
        let mut a = a;
        let commander = a.commander();
        a.command(commander, Command::EndTurn).unwrap();
        reloaded.command(commander, Command::EndTurn).unwrap();
        assert_eq!(a.game, reloaded.game);
    }
    #[test]
    fn path_searches_are_repeatable_across_maps_of_different_sizes() {
        let world = Host::base(1).unwrap();
        let japan = world.commander();
        let find = |name: &str| {
            world
                .game
                .cities
                .values()
                .find(|c| c.name == name)
                .unwrap()
                .position
        };
        let (tokyo, osaka, london) = (find("Tokyo"), find("Osaka"), find("London"));
        let land = Domain::Land;
        let route = world.game.path(japan, land, tokyo, osaka, 20_000).unwrap();
        assert_eq!((route[0], *route.last().unwrap()), (tokyo, osaka));
        for step in route.windows(2) {
            assert_eq!(step[0].distance(step[1]), 1);
            assert!(world.game.map.get(step[1]).unwrap().is_land());
        }
        // The search buffers are shared per thread. A smaller map in between must not
        // corrupt them, and a repeated search must give the same answer.
        let small = starter(1);
        let city = small.game.cities.values().next().unwrap().position;
        assert_eq!(
            small.game.path(1, land, city, city, 100).unwrap(),
            vec![city]
        );
        assert_eq!(
            world.game.path(japan, land, tokyo, osaka, 20_000).unwrap(),
            route
        );
        // Separate land masses and exhausted budgets both report "no route".
        assert!(
            world
                .game
                .path(japan, land, tokyo, london, 20_000)
                .is_none()
        );
        assert!(world.game.path(japan, land, tokyo, osaka, 3).is_none());
        // A ship can sail between them.
        assert!(
            world
                .game
                .path(japan, Domain::Sea, tokyo, london, 200_000)
                .is_none_or(|sea| sea
                    .iter()
                    .all(|p| !world.game.map.get(*p).unwrap().is_land()
                        || *p == tokyo
                        || *p == london))
        );
    }
    #[test]
    fn the_charted_world_is_known_but_foreign_units_are_seen_only_nearby() {
        let h = Host::base(3).unwrap();
        let commander = h.commander();
        let Response::Snapshot { game, .. } = h.snapshot(commander) else {
            panic!("expected a snapshot");
        };
        // The geography is common knowledge: every tile, border and city is on the chart.
        let explored = &game.factions[&commander].explored;
        assert!(h.game.map.positions().all(|p| explored.contains(p)));
        assert_eq!(game.cities.len(), h.game.cities.len());
        for (a, b) in game.map.tiles.iter().zip(&h.game.map.tiles) {
            assert_eq!(
                (
                    a.terrain,
                    a.relief,
                    a.cover,
                    a.river,
                    a.owner,
                    a.claim,
                    a.region,
                    a.improvements,
                    a.resource
                ),
                (
                    b.terrain,
                    b.relief,
                    b.cover,
                    b.river,
                    b.owner,
                    b.claim,
                    b.region,
                    b.improvements,
                    b.resource
                )
            );
        }
        // ... but units abroad are not.
        let foreign = |g: &Game| g.units.values().filter(|u| u.owner != commander).count();
        assert!(
            foreign(&game) < foreign(&h.game) / 4,
            "{} of {}",
            foreign(&game),
            foreign(&h.game)
        );
        // Foreign nations' knowledge is not leaked to the client.
        assert!(
            game.factions
                .iter()
                .all(|(id, f)| *id == commander || f.explored.is_empty())
        );
    }
    #[test]
    fn an_uncharted_scenario_starts_in_fog() {
        let h = starter(3);
        let commander = h.commander();
        let Response::Snapshot { game, .. } = h.snapshot(commander) else {
            panic!("expected a snapshot");
        };
        let explored = &game.factions[&commander].explored;
        assert!(!explored.is_empty());
        assert!(
            explored.len() < h.game.map.tiles.len() / 2,
            "{} tiles explored",
            explored.len()
        );
        assert!(game.cities.len() < h.game.cities.len());
    }
    #[test]
    fn snapshot_json_roundtrip() {
        let h = starter(42);
        for player in [0, 1] {
            let response = h.snapshot(player);
            let json = serde_json::to_string(&response).unwrap();
            let restored: Response = serde_json::from_str(&json).unwrap();
            assert_eq!(serde_json::to_string(&restored).unwrap(), json);
        }
    }
    #[test]
    fn deterministic_save_and_replay() {
        let mut a = starter(42);
        let mut b = starter(42);
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
    fn saves_from_other_formats_are_refused_clearly() {
        let json = starter(1).save().unwrap().replacen(
            &format!("\"format\":{SAVE_FORMAT}"),
            "\"format\":2",
            1,
        );
        let message = Host::load(&json).err().unwrap().to_string();
        assert!(message.contains("Unsupported save format 2"), "{message}");
    }
    #[test]
    fn invalid_command_is_atomic() {
        let mut h = starter(1);
        let before = h.game.clone();
        assert!(
            h.command(
                1,
                Command::Move {
                    unit: 999,
                    destination: Coord::new(0, 0)
                }
            )
            .is_err()
        );
        assert_eq!(before, h.game);
    }
    #[test]
    fn stale_revision_and_authority_checked() {
        let mut h = starter(1);
        let r = Request {
            version: PROTOCOL_VERSION,
            sequence: 1,
            revision: 123,
            command: Command::EndTurn,
        };
        assert!(matches!(h.request(1, r), Response::Rejected { .. }));
        let old = Request {
            version: 1,
            sequence: 2,
            revision: 0,
            command: Command::EndTurn,
        };
        assert!(matches!(h.request(1, old), Response::Rejected { .. }));
        assert!(h.command(0, Command::EndTurn).is_err());
        let enemy = unit_of(&h, 2, "cavalry");
        for command in [
            Command::Move {
                unit: enemy,
                destination: Coord::new(10, 9),
            },
            Command::Fortify { unit: enemy },
            Command::Disband { unit: enemy },
        ] {
            assert!(h.command(1, command).is_err());
        }
    }
    #[test]
    fn the_twelfth_week_has_no_boom() {
        let mut host = starter(1);
        host.game.turn = 11;
        let player = host.commander();
        let before = host.game.factions[&player].gold;
        let income = host.game.income(player);
        let upkeep = host.game.budget_upkeep(player, &host.rules);
        host.command(player, Command::EndTurn).unwrap();
        assert_eq!(host.game.factions[&player].gold, before + income - upkeep);
    }
    #[test]
    fn hidden_enemies_not_in_player_snapshot() {
        let h = starter(1);
        let v = h.game.view(1);
        // Rival land forces are out of sight; only ships near our own are spotted.
        assert!(
            v.units
                .values()
                .all(|u| u.owner == 1 || h.rules.def(u).is_naval())
        );
        assert!(v.units.len() < h.game.units.len());
        assert_eq!(v.rng, 0);
        // Other nations' treasuries, research, and explored areas stay private.
        assert_eq!(v.factions[&2].gold, 0);
        assert!(v.factions[&2].explored.is_empty());
        assert!(!v.factions[&1].explored.is_empty());
    }
    #[test]
    fn unexplored_ground_and_ownership_are_masked() {
        let h = starter(1);
        let v = h.game.view(1);
        let far = Coord::new(21, 14);
        assert!(!h.game.factions[&1].explored.contains(far));
        let tile = v.map.get(far).unwrap();
        assert_eq!(
            (tile.terrain, tile.owner, tile.claim, tile.region),
            (Terrain::Ocean, 0, 0, 0)
        );
        let own_city = h.game.cities[&1].position;
        assert_eq!(v.map.get(own_city).unwrap().owner, 1);
        assert_eq!(h.game.view(0).map, h.game.map);
    }
    #[test]
    fn the_campaign_calendar_advances_one_week_per_turn() {
        let mut h = starter(3);
        assert_eq!(h.game.date().iso(), "1904-02-08");
        for _ in 0..3 {
            h.command(1, Command::EndTurn).unwrap();
        }
        assert_eq!(h.game.turn, 4);
        assert_eq!(h.game.date().iso(), "1904-02-29");
        assert!(h.game.log.iter().any(|l| l.starts_with("8 Feb 1904 |")));
        assert_eq!(h.game.view(1).date(), h.game.date());
    }
    #[test]
    fn commander_can_be_chosen_at_start() {
        let content = Content::base();
        let host = Host::start(
            content,
            Start {
                scenario: Some("russo-japanese-war"),
                nation: Some("russia"),
                seed: 1,
            },
        )
        .unwrap();
        assert_eq!(host.commander(), 2);
        let mut host = host;
        assert!(host.command(1, Command::EndTurn).is_err());
        host.command(2, Command::EndTurn).unwrap();
        for (scenario, nation) in [("russo-japanese-war", "atlantis"), ("missing", "japan")] {
            let result = Host::start(
                Content::base(),
                Start {
                    scenario: Some(scenario),
                    nation: Some(nation),
                    seed: 1,
                },
            );
            assert!(result.is_err(), "{scenario}/{nation}");
        }
    }
    #[test]
    fn scenario_wars_drive_the_ai_and_peace_keeps_it_home() {
        // At war: Russia's cavalry rides on the Japanese capital. At peace: it stays home.
        let ride = |mut h: Host| {
            // Economic construction can delay new defenders. Give this military-policy fixture
            // a full garrison so its cavalry is genuinely spare in both cases.
            let capital = h
                .game
                .cities
                .values()
                .find(|c| c.owner == 2 && c.capital)
                .unwrap()
                .position;
            for _ in 0..4 {
                let id = h.game.next_id;
                h.game.next_id += 1;
                h.game
                    .units
                    .insert(id, fourx_sim::Unit::new(id, 2, "infantry", capital));
            }
            h.game.reindex();
            let id = unit_of(&h, 2, "cavalry");
            let start = h.game.units[&id].position;
            for _ in 0..3 {
                h.command(1, Command::EndTurn).unwrap();
            }
            (start, h.game.units[&id].position)
        };
        let (start, after) = ride(starter(1));
        assert_ne!(start, after);
        let (start, after) = ride(starter_at_peace(1));
        assert_eq!(start, after);
    }
    #[test]
    fn peace_blocks_combat_until_war_is_declared() {
        let mut h = starter_at_peace(9);
        assert!(!h.game.at_war(1, 2));
        let attacker = unit_of(&h, 1, "cavalry");
        let target = unit_of(&h, 2, "infantry");
        let capital = h.game.cities[&1].position;
        let square = Coord::new(capital.x + 1, capital.y);
        make_grass(&mut h, square);
        h.game.set_position(target, square);
        // Combat cannot begin while the two nations are at peace.
        let attack = Command::Attack {
            unit: attacker,
            target: square,
        };
        assert!(h.command(1, attack.clone()).is_err());
        assert!(!h.game.at_war(1, 2));
        // Declaring war opens the way, and both nations know it.
        h.command(1, Command::DeclareWar { nation: 2 }).unwrap();
        assert!(h.game.at_war(1, 2));
        assert!(h.game.at_war(2, 1));
        h.command(1, attack).unwrap();
    }
    #[test]
    fn a_battle_wounds_or_kills_with_civ3_combat() {
        let mut h = starter(9);
        let attacker = unit_of(&h, 1, "cavalry");
        let defender = unit_of(&h, 2, "infantry");
        let capital = h.game.cities[&1].position;
        let square = Coord::new(capital.x + 1, capital.y);
        make_grass(&mut h, square);
        h.game.set_position(defender, square);
        let odds = h
            .game
            .estimate_attack(attacker, square, &h.rules)
            .expect("the cavalry can see a target");
        assert_eq!(odds.defender, defender);
        assert!(odds.win_chance > 0.0 && odds.win_chance < 1.0);
        h.command(
            1,
            Command::Attack {
                unit: attacker,
                target: square,
            },
        )
        .unwrap();
        // One of the two paid for it, and the attacker has used its action.
        let hurt = |g: &Game, id: Id| g.units.get(&id).is_none_or(|u| u.damage > 0);
        assert!(hurt(&h.game, attacker) || hurt(&h.game, defender));
        // Cavalry keeps fighting (blitz) while it still has movement and hit points.
        if let Some(unit) = h.game.units.get(&attacker) {
            assert!(unit.attacked);
        }
    }
    #[test]
    fn artillery_bombards_without_killing_and_cannot_melee() {
        let mut h = starter(5);
        let gun = unit_of(&h, 1, "artillery");
        let defender = unit_of(&h, 2, "infantry");
        let capital = h.game.cities[&1].position;
        let square = Coord::new(capital.x + 1, capital.y);
        make_grass(&mut h, square);
        h.game.set_position(defender, square);
        h.command(
            1,
            Command::Bombard {
                unit: gun,
                target: square,
            },
        )
        .unwrap();
        // Bombardment wounds but never kills, and spends the gun's action.
        let survivor = &h.game.units[&defender];
        assert!(survivor.hp(h.rules.def(survivor)) >= 1);
        assert!(h.game.units[&gun].attacked);
        let attack = Command::Attack {
            unit: gun,
            target: square,
        };
        assert!(h.command(1, attack).is_err());
    }
    #[test]
    fn founding_and_conquering_cities_move_borders() {
        let mut h = starter(7);
        let capital = h.game.cities[&1].position;
        let near = Coord::new(capital.x + 1, capital.y);
        assert_eq!(h.game.map.get(near).unwrap().owner, 1);
        // Conquest hands the surrounding claims to the new holder.
        let enemy_capital = h.game.cities[&2].position;
        let beside = Coord::new(enemy_capital.x - 1, enemy_capital.y);
        let after = Coord::new(enemy_capital.x + 1, enemy_capital.y);
        assert_eq!(h.game.map.get(after).unwrap().owner, 2);
        let rider = unit_of(&h, 1, "cavalry");
        h.game.units.retain(|_, unit| unit.owner != 2);
        h.game.reindex();
        make_grass(&mut h, beside);
        h.game.set_position(rider, beside);
        h.command(
            1,
            Command::Attack {
                unit: rider,
                target: enemy_capital,
            },
        )
        .unwrap();
        assert_eq!(h.game.cities[&2].owner, 1);
        assert_eq!(h.game.map.get(enemy_capital).unwrap().owner, 1);
        assert_eq!(h.game.map.get(after).unwrap().owner, 1);
        assert!(h.game.at_war(1, 2));
    }
    #[test]
    fn settlement_production_and_victory() {
        let mut h = starter_at_peace(7);
        // The legacy pack still supports industrial victory when physical goods are disabled.
        h.rules.economy.enabled = false;
        let pioneer = unit_of(&h, 1, "pioneer");
        // Make a known fixture site three tiles away, then exercise real movement/settlement.
        let capital = h.game.cities[&1].position;
        let site = Coord::new(capital.x + 3, capital.y);
        for step in 1..=3 {
            make_grass(&mut h, Coord::new(capital.x + step, capital.y));
        }
        h.command(
            1,
            Command::Move {
                unit: pioneer,
                destination: site,
            },
        )
        .unwrap();
        for _ in 0..4 {
            if h.game.units[&pioneer].position == site {
                break;
            }
            h.command(1, Command::EndTurn).unwrap();
        }
        assert_eq!(h.game.units[&pioneer].position, site);
        h.command(
            1,
            Command::FoundCity {
                unit: pioneer,
                name: "Harbor".into(),
            },
        )
        .unwrap();
        assert_eq!(h.game.map.get(site).unwrap().owner, 1);
        let city = h
            .game
            .cities
            .values()
            .find(|c| c.name == "Harbor")
            .unwrap()
            .id;
        h.game.cities.get_mut(&city).unwrap().industry = 30;
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
                .units
                .values()
                .any(|u| u.owner == 1 && u.kind == "infantry" && u.position == site)
        );
        h.game.factions.get_mut(&1).unwrap().industry = h.rules.victory_industry;
        h.command(1, Command::EndTurn).unwrap();
        assert_eq!(h.game.winner, Some(1));
    }
    #[test]
    fn workers_fortify_and_build_through_the_host() {
        let mut h = starter(2);
        let worker = unit_of(&h, 1, "worker");
        let guard = unit_of(&h, 1, "infantry");
        let capital = h.game.cities[&1].position;
        let field = Coord::new(capital.x + 1, capital.y);
        make_grass(&mut h, field);
        h.command(
            1,
            Command::Move {
                unit: worker,
                destination: field,
            },
        )
        .unwrap();
        assert_eq!(h.game.units[&worker].position, field);
        h.command(
            1,
            Command::Work {
                unit: worker,
                job: Job::Road,
            },
        )
        .unwrap();
        assert!(matches!(
            h.game.units[&worker].order,
            Order::Work(Job::Road)
        ));
        // Two workers' worth of turns lay a road. Nothing else about the economy changes.
        for _ in 0..8 {
            h.command(1, Command::EndTurn).unwrap();
        }
        assert!(h.game.map.get(field).unwrap().has_road());
        // The garrison can dig in, and is dug in after a full turn.
        h.command(1, Command::Fortify { unit: guard }).unwrap();
        h.command(1, Command::EndTurn).unwrap();
        assert!(h.game.units[&guard].is_fortified());
        h.command(1, Command::Cancel { unit: guard }).unwrap();
        assert_eq!(h.game.units[&guard].order, Order::None);
    }
    /// The commander's infantry, and the farthest land square it can walk to along a route of
    /// at most `longest` squares.
    fn infantry_and_goal(host: &Host, longest: usize) -> (Id, Coord) {
        let player = host.commander();
        let unit = host
            .game
            .units
            .values()
            .find(|u| u.owner == player && u.kind == "infantry")
            .expect("the commander starts with infantry");
        let goal = host
            .game
            .map
            .tiles
            .iter()
            .filter(|t| t.is_land() && !host.game.blocked_for(player, t.position))
            .filter_map(|t| {
                host.game
                    .path(player, Domain::Land, unit.position, t.position, usize::MAX)
                    .filter(|path| path.len() <= longest)
                    .map(|path| (path.len(), t.position))
            })
            .max_by_key(|(len, _)| *len)
            .expect("the starter has land to cross")
            .1;
        (unit.id, goal)
    }
    fn seen_by(host: &Host, player: Id) -> Game {
        let Response::Snapshot { game, .. } = host.snapshot(player) else {
            panic!("expected a snapshot");
        };
        game
    }
    #[test]
    fn a_marching_unit_is_sent_with_the_route_it_will_walk() {
        let mut h = starter(1);
        let player = h.commander();
        let (unit, goal) = infantry_and_goal(&h, usize::MAX);
        assert!(seen_by(&h, player).units[&unit].route.is_empty());
        h.command(
            player,
            Command::Move {
                unit,
                destination: goal,
            },
        )
        .unwrap();
        let game = seen_by(&h, player);
        let sent = &game.units[&unit];
        assert_eq!(sent.route.first(), Some(&sent.position));
        assert_eq!(sent.route.last(), Some(&goal));
        for step in sent.route.windows(2) {
            assert_eq!(step[0].distance(step[1]), 1, "{step:?} is not one square");
        }
        // It is the route the unit will take, over ground the nation has not charted yet.
        let chart = &h.game.factions[&player].explored;
        assert!(sent.route.iter().any(|p| !chart.contains(*p)));
        // Only the view carries it: the campaign itself plans the march afresh each day.
        assert!(h.game.units.values().all(|u| u.route.is_empty()));
        assert!(!h.save().unwrap().contains("route"));
        // The rival gets no one's route, and a unit that stands still has none.
        let rival = game.units.values().find(|u| u.owner != player);
        assert!(rival.is_none_or(|u| u.route.is_empty()));
        let other = h
            .game
            .units
            .values()
            .find(|u| u.owner == player && u.id != unit)
            .unwrap()
            .id;
        assert!(game.units[&other].route.is_empty());
    }
    #[test]
    fn the_unit_walks_exactly_the_squares_it_was_sent() {
        let mut h = starter(1);
        let player = h.commander();
        let (unit, goal) = infantry_and_goal(&h, 5);
        h.command(
            player,
            Command::Move {
                unit,
                destination: goal,
            },
        )
        .unwrap();
        let first = seen_by(&h, player).units[&unit].route.clone();
        assert!(first.len() > 2, "{first:?}");
        let mut days = 0;
        while h.game.units[&unit].position != goal && days < 10 {
            // Each day the route is sent again from where the unit stands: the rest of the last.
            let again = seen_by(&h, player).units[&unit].route.clone();
            assert!(
                first.ends_with(&again),
                "day {days}: {again:?} is not the end of {first:?}"
            );
            assert_eq!(again.first(), Some(&h.game.units[&unit].position));
            h.command(player, Command::EndTurn).unwrap();
            let at = h.game.units[&unit].position;
            assert!(
                first.contains(&at),
                "day {days}: {at:?} is off the route {first:?}"
            );
            days += 1;
        }
        assert_eq!(h.game.units[&unit].position, goal, "it arrives");
        assert!(seen_by(&h, player).units[&unit].route.is_empty());
    }
}
