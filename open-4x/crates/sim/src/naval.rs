//! Civ3's naval rules: harbours, passengers, boarding and landing.
//!
//! * A ship enters a coastal city of its own nation (a *port*) and may sail through one to
//!   reach another sea. In port it heals, but cannot defend against a melee attack.
//! * A land unit boards by stepping from the shore onto a friendly ship with room, or with
//!   [`Command::Load`](crate::Command::Load) when both stand in the same port. A passenger
//!   shares its ship's square, never defends on its own, and cannot attack while aboard.
//! * Landing from the sea uses up the rest of the unit's movement. Stepping out of a ship
//!   in port is an ordinary move, and so is [`Command::Unload`](crate::Command::Unload).
//! * A ship lost at sea takes its passengers down with it; one lost in port sets them ashore.
use crate::{Game, Id, Result, Rules, error, terrain::Coord, units::*};

impl Game {
    /// A coastal city of `owner`, where that nation's ships come into harbour.
    pub fn is_port(&self, owner: Id, p: Coord) -> bool {
        self.city_at(p)
            .is_some_and(|c| self.cities[&c].owner == owner)
            && self.coastal(p)
    }

    /// Whether a unit of `domain` and nation `owner` can stand on a square: land units need
    /// land, and ships need water or one of their own ports.
    pub fn passable(&self, domain: Domain, owner: Id, p: Coord) -> bool {
        self.map.get(p).is_some_and(|t| match domain {
            Domain::Land => t.is_land(),
            Domain::Sea => !t.is_land() || self.is_port(owner, p),
        })
    }

    /// The units riding `ship`, oldest first.
    pub fn passengers(&self, ship: Id) -> Vec<Id> {
        let Some(ship_unit) = self.units.get(&ship) else {
            return Vec::new();
        };
        self.unit_ids_at(ship_unit.position)
            .into_iter()
            .filter(|id| self.units[id].carrier == Some(ship))
            .collect()
    }

    /// Whether `ship` could take `passenger` aboard: same nation, a carrier with a free berth,
    /// and a land unit that is not already aboard something.
    fn has_berth_for(&self, ship: &Unit, passenger: &Unit, rules: &Rules) -> bool {
        let def = rules.def(ship);
        ship.owner == passenger.owner
            && def.is_carrier()
            && ship.carrier.is_none()
            && rules.def(passenger).domain == Domain::Land
            && (self.passengers(ship.id).len() as u32) < def.capacity
    }

    /// The first of the nation's ships on a square that has room for `passenger`.
    fn ship_with_room(&self, at: Coord, passenger: &Unit, rules: &Rules) -> Option<Id> {
        self.units_at(at)
            .into_iter()
            .find(|ship| self.has_berth_for(ship, passenger, rules))
            .map(|ship| ship.id)
    }

    /// Step from the shore onto a friendly ship with room. Costs one step and, like any move
    /// onto a ship, leaves the unit with nothing to do but ride or land.
    pub(crate) fn board(&mut self, id: Id, at: Coord, rules: &Rules) -> Result<()> {
        let unit = self.units.get(&id).ok_or_else(|| error("Unknown unit"))?;
        if unit.carrier.is_some() {
            return Err(error("Already aboard a ship"));
        }
        if unit.position.distance(at) != 1 {
            return Err(error("A ship must be next to you before you can board it"));
        }
        let left = unit.moves_left(rules.def(unit));
        if left == 0 {
            return Err(error("No movement left"));
        }
        let ship = self
            .ship_with_room(at, unit, rules)
            .ok_or_else(|| error("No friendly ship with room is there"))?;
        let from = unit.position;
        self.set_position(id, at);
        self.record_march(id, vec![from, at]);
        let unit = self.units.get_mut(&id).unwrap();
        unit.moves_used += MOVE_UNIT.min(left);
        unit.clear_orders();
        unit.carrier = Some(ship);
        Ok(())
    }

    /// The `Load` command: a land unit and a ship sharing a port. Spends no movement.
    pub(crate) fn load(&mut self, player: Id, id: Id, ship: Id, rules: &Rules) -> Result<()> {
        let unit = self.own_unit(player, id)?;
        let carrier = self.own_unit(player, ship)?;
        if unit.carrier.is_some() {
            return Err(error("Already aboard a ship"));
        }
        if unit.position != carrier.position {
            return Err(error("The unit and the ship must be on the same square"));
        }
        if unit.moves_left(rules.def(unit)) == 0 {
            return Err(error("No movement left"));
        }
        if !self.has_berth_for(carrier, unit, rules) {
            return Err(error("That ship cannot take this unit aboard"));
        }
        let unit = self.units.get_mut(&id).unwrap();
        unit.clear_orders();
        unit.carrier = Some(ship);
        Ok(())
    }

    /// The `Unload` command: set one passenger, or all of a ship's, ashore in port.
    pub(crate) fn unload(&mut self, player: Id, id: Id, rules: &Rules) -> Result<()> {
        let unit = self.own_unit(player, id)?;
        let (ship, position) = match unit.carrier {
            Some(ship) => (ship, unit.position),
            None if rules.def(unit).is_carrier() => (id, unit.position),
            None => return Err(error("Nothing to unload")),
        };
        if self.city_at(position).is_none() {
            return Err(error(
                "Passengers can be unloaded only in port; at sea they land by moving ashore",
            ));
        }
        if ship == id {
            self.release_passengers(ship);
        } else {
            self.release(id);
        }
        Ok(())
    }

    /// Take a unit off its ship where it stands.
    pub(crate) fn release(&mut self, id: Id) {
        if let Some(unit) = self.units.get_mut(&id) {
            unit.carrier = None;
            unit.clear_orders();
        }
    }
    /// Take everyone off a ship where it stands.
    pub(crate) fn release_passengers(&mut self, ship: Id) {
        for id in self.passengers(ship) {
            self.release(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::tests::{arena, arena_rows, rules, spawn};
    use crate::{Command, Game, Id, Rules, TickRules, terrain::Coord, units::*};

    const RED: Id = 1;
    const BLUE: Id = 2;

    /// A west shore, with Redville inland at (1,1), and Blueton on a distant spit at (10,4).
    const COAST: [&str; 6] = [
        "....~~~~~~~~",
        "....~~~~~~~~",
        "....~~~~~~~~",
        "....~~~~~~~~",
        "....~~~~~...",
        "~~~~~~~~~~~~",
    ];
    /// Two islands: Redville (1,1) and Blueton (10,4) are both harbours.
    const HARBOUR: [&str; 6] = [
        "~~~~~~~~~~~~",
        "~...~~~~~~~~",
        "~...~~~~~~~~",
        "~...~~~~~~~~",
        "~~~~~~~~~...",
        "~~~~~~~~~~~~",
    ];
    /// A one-tile-wide wall of land with Redville in the middle: the only way from the western
    /// sea to the eastern one is through the city.
    const ISTHMUS: [&str; 4] = ["~.~~~~", "~.~~~~", "~.~~.~", "~.~~~~"];

    fn map(rows: &[&str]) -> Game {
        arena_rows(&rows.iter().map(|r| r.to_string()).collect::<Vec<_>>())
    }
    fn at(x: i32, y: i32) -> Coord {
        Coord::new(x, y)
    }
    fn run(game: &mut Game, player: Id, command: Command) -> crate::Result<()> {
        game.apply(player, command, &rules(), TickRules::default())
    }
    fn end_turn(game: &mut Game) {
        run(game, RED, Command::EndTurn).expect("turn ends");
    }
    fn unit(game: &Game, id: Id) -> &Unit {
        &game.units[&id]
    }
    fn board(game: &mut Game, passenger: Id, ship: Id) {
        game.units.get_mut(&passenger).unwrap().carrier = Some(ship);
    }
    fn seeded(game: &mut Game, seed: u64) {
        game.rng = seed.wrapping_mul(0x2545_F491_4F6C_DD1D) | 1;
    }
    fn error(result: crate::Result<()>) -> String {
        result.expect_err("the order should be refused").0
    }

    #[test]
    fn ships_enter_their_own_coastal_cities_and_nobody_elses() {
        let mut game = map(&HARBOUR);
        let ship = spawn(&mut game, RED, "ironclad", 1, 0);
        assert!(game.is_port(RED, at(1, 1)));
        assert!(!game.is_port(BLUE, at(1, 1)), "Redville is not Blue's");
        run(
            &mut game,
            RED,
            Command::Move {
                unit: ship,
                destination: at(1, 1),
            },
        )
        .unwrap();
        assert_eq!(unit(&game, ship).position, at(1, 1));
        let def = &rules().units["ironclad"];
        assert_eq!(
            unit(&game, ship).moves_left(def),
            def.total_moves() - MOVE_UNIT
        );
        // From port it sails out again, but cannot march across the island.
        run(
            &mut game,
            RED,
            Command::Move {
                unit: ship,
                destination: at(0, 0),
            },
        )
        .unwrap();
        assert_eq!(unit(&game, ship).position, at(0, 0));
        for land in [at(2, 2), at(10, 4)] {
            let refusal = error(run(
                &mut game,
                RED,
                Command::Move {
                    unit: ship,
                    destination: land,
                },
            ));
            assert!(refusal.contains("own coastal cities"), "{refusal}");
        }
        // A city with no water beside it is no harbour.
        let inland = arena(8, 8);
        assert!(!inland.is_port(RED, at(1, 1)));
        assert!(!inland.passable(Domain::Sea, RED, at(1, 1)));
    }

    #[test]
    fn a_ship_can_cross_between_two_seas_through_its_own_port() {
        let mut game = map(&ISTHMUS);
        let (west, east) = (at(0, 1), at(2, 1));
        assert_eq!(
            game.path(RED, Domain::Sea, west, east, 1000),
            Some(vec![west, at(1, 1), east])
        );
        assert!(
            game.path(BLUE, Domain::Sea, west, east, 1000).is_none(),
            "Redville is not Blue's to sail through"
        );
        let ship = spawn(&mut game, RED, "ironclad", 0, 1);
        run(
            &mut game,
            RED,
            Command::Move {
                unit: ship,
                destination: east,
            },
        )
        .unwrap();
        assert_eq!(unit(&game, ship).position, east);
    }

    #[test]
    fn new_ships_launch_into_their_port_and_mend_only_there() {
        let mut inland = arena(8, 8);
        let city = inland.city_at(at(1, 1)).unwrap();
        let refusal = error(run(
            &mut inland,
            RED,
            Command::Produce {
                city,
                unit: "ironclad".into(),
            },
        ));
        assert!(refusal.contains("coastal"), "{refusal}");

        let mut game = map(&HARBOUR);
        let city = game.city_at(at(1, 1)).unwrap();
        run(
            &mut game,
            RED,
            Command::Produce {
                city,
                unit: "ironclad".into(),
            },
        )
        .unwrap();
        // Production comes from the city's shields, so wait for the ship rather than for a day.
        for _ in 0..40 {
            if game
                .units
                .values()
                .any(|u| u.owner == RED && u.kind == "ironclad")
            {
                break;
            }
            end_turn(&mut game);
        }
        let ship = game
            .units
            .values()
            .find(|u| u.owner == RED && u.kind == "ironclad")
            .expect("the ironclad was built")
            .id;
        assert_eq!(
            unit(&game, ship).position,
            at(1, 1),
            "built in the city, not beside it"
        );
        // Beside the port is still the open sea.
        let outside = spawn(&mut game, RED, "ironclad", 0, 0);
        for id in [ship, outside] {
            game.units.get_mut(&id).unwrap().damage = 3;
        }
        end_turn(&mut game);
        assert_eq!(unit(&game, ship).damage, 1, "two hit points mended in port");
        assert_eq!(unit(&game, outside).damage, 3, "a ship at sea never heals");
    }

    #[test]
    fn passengers_board_ride_and_land() {
        let mut game = map(&COAST);
        let rules = rules();
        let ship = spawn(&mut game, RED, "transport", 4, 2);
        let a = spawn(&mut game, RED, "infantry", 3, 2);
        let b = spawn(&mut game, RED, "infantry", 3, 3);
        let c = spawn(&mut game, RED, "infantry", 3, 1);
        run(
            &mut game,
            RED,
            Command::Move {
                unit: a,
                destination: at(4, 2),
            },
        )
        .unwrap();
        assert_eq!(
            (unit(&game, a).position, unit(&game, a).carrier),
            (at(4, 2), Some(ship))
        );
        assert_eq!(
            unit(&game, a).moves_left(&rules.units["infantry"]),
            3,
            "boarding costs one step"
        );
        run(
            &mut game,
            RED,
            Command::Move {
                unit: b,
                destination: at(4, 2),
            },
        )
        .unwrap();
        let full = error(run(
            &mut game,
            RED,
            Command::Move {
                unit: c,
                destination: at(4, 2),
            },
        ));
        assert!(full.contains("room"), "{full}");
        assert_eq!(game.passengers(ship), vec![a, b]);

        // The passengers go wherever the ship goes.
        run(
            &mut game,
            RED,
            Command::Move {
                unit: ship,
                destination: at(6, 2),
            },
        )
        .unwrap();
        for id in [a, b] {
            assert_eq!(
                (unit(&game, id).position, unit(&game, id).carrier),
                (at(6, 2), Some(ship))
            );
        }
        // At sea they can neither fight nor dig in nor wade ashore from open water.
        let refusal = error(run(&mut game, RED, Command::Fortify { unit: a }));
        assert!(refusal.contains("ashore"), "{refusal}");
        let refusal = error(run(
            &mut game,
            RED,
            Command::Attack {
                unit: a,
                target: at(7, 2),
            },
        ));
        assert!(refusal.contains("Passengers"), "{refusal}");
        assert!(
            run(
                &mut game,
                RED,
                Command::Move {
                    unit: a,
                    destination: at(3, 2)
                }
            )
            .is_err()
        );

        // Back at the shore one lands; landing uses up the rest of the turn.
        end_turn(&mut game);
        run(
            &mut game,
            RED,
            Command::Move {
                unit: ship,
                destination: at(4, 2),
            },
        )
        .unwrap();
        run(
            &mut game,
            RED,
            Command::Move {
                unit: a,
                destination: at(3, 2),
            },
        )
        .unwrap();
        assert_eq!(
            (unit(&game, a).position, unit(&game, a).carrier),
            (at(3, 2), None)
        );
        assert_eq!(unit(&game, a).moves_left(&rules.units["infantry"]), 0);
        assert_eq!(
            unit(&game, b).carrier,
            Some(ship),
            "the other is still aboard"
        );
        assert_eq!(game.passengers(ship), vec![b]);
    }

    #[test]
    fn only_a_friendly_ship_alongside_can_be_boarded() {
        let mut game = map(&COAST);
        let walker = spawn(&mut game, RED, "infantry", 3, 2);
        let bare = error(run(
            &mut game,
            RED,
            Command::Move {
                unit: walker,
                destination: at(4, 2),
            },
        ));
        assert!(bare.contains("No friendly ship"), "{bare}");
        let far = error(run(
            &mut game,
            RED,
            Command::Move {
                unit: walker,
                destination: at(6, 2),
            },
        ));
        assert!(far.contains("next to you"), "{far}");
        spawn(&mut game, BLUE, "transport", 4, 2);
        let foreign = error(run(
            &mut game,
            RED,
            Command::Move {
                unit: walker,
                destination: at(4, 2),
            },
        ));
        assert!(foreign.contains("No friendly ship"), "{foreign}");
        // Only land units ride, and warships carry nobody.
        let warship = spawn(&mut game, RED, "ironclad", 4, 3);
        let diagonal = error(run(
            &mut game,
            RED,
            Command::Move {
                unit: walker,
                destination: at(4, 3),
            },
        ));
        assert!(diagonal.contains("No friendly ship"), "{diagonal}");
        assert_eq!(unit(&game, warship).carrier, None);
    }

    #[test]
    fn in_port_passengers_load_unload_and_step_ashore() {
        let mut game = map(&HARBOUR);
        let (ship, a, b) = (
            spawn(&mut game, RED, "transport", 1, 1),
            spawn(&mut game, RED, "infantry", 1, 1),
            spawn(&mut game, RED, "infantry", 1, 1),
        );
        let elsewhere = spawn(&mut game, RED, "transport", 0, 0);
        run(
            &mut game,
            RED,
            Command::Load {
                unit: a,
                carrier: ship,
            },
        )
        .unwrap();
        assert_eq!(unit(&game, a).carrier, Some(ship));
        assert_eq!(unit(&game, a).moves_used, 0, "loading is free");
        let apart = error(run(
            &mut game,
            RED,
            Command::Load {
                unit: b,
                carrier: elsewhere,
            },
        ));
        assert!(apart.contains("same square"), "{apart}");

        // Sailing out takes the passenger along, and at sea it can only wait for a port.
        run(
            &mut game,
            RED,
            Command::Move {
                unit: ship,
                destination: at(1, 0),
            },
        )
        .unwrap();
        assert_eq!(unit(&game, a).position, at(1, 0));
        let at_sea = error(run(&mut game, RED, Command::Unload { unit: ship }));
        assert!(at_sea.contains("port"), "{at_sea}");
        run(
            &mut game,
            RED,
            Command::Move {
                unit: ship,
                destination: at(1, 1),
            },
        )
        .unwrap();
        run(&mut game, RED, Command::Unload { unit: a }).unwrap();
        assert_eq!(unit(&game, a).carrier, None);
        assert!(game.passengers(ship).is_empty());

        // Unloading the ship sets everyone ashore, and a passenger can also just walk out.
        for id in [a, b] {
            run(
                &mut game,
                RED,
                Command::Load {
                    unit: id,
                    carrier: ship,
                },
            )
            .unwrap();
        }
        assert_eq!(game.passengers(ship), vec![a, b]);
        run(&mut game, RED, Command::Unload { unit: ship }).unwrap();
        assert!(game.passengers(ship).is_empty());
        run(
            &mut game,
            RED,
            Command::Load {
                unit: a,
                carrier: ship,
            },
        )
        .unwrap();
        run(
            &mut game,
            RED,
            Command::Move {
                unit: a,
                destination: at(2, 2),
            },
        )
        .unwrap();
        assert_eq!(
            (unit(&game, a).position, unit(&game, a).carrier),
            (at(2, 2), None)
        );
        assert_eq!(
            unit(&game, a).moves_left(&rules().units["infantry"]),
            3,
            "leaving a ship in port is an ordinary step, not a landing"
        );
    }

    #[test]
    fn a_ship_lost_at_sea_takes_its_passengers_but_one_lost_in_port_does_not() {
        let mut game = map(&COAST);
        let ship = spawn(&mut game, RED, "transport", 5, 2);
        let rider = spawn(&mut game, RED, "infantry", 5, 2);
        board(&mut game, rider, ship);
        game.remove_unit(ship);
        assert!(!game.units.contains_key(&rider), "drowned with the ship");

        let mut game = map(&HARBOUR);
        let ship = spawn(&mut game, RED, "transport", 1, 1);
        let rider = spawn(&mut game, RED, "infantry", 1, 1);
        board(&mut game, rider, ship);
        game.remove_unit(ship);
        assert_eq!(unit(&game, rider).carrier, None, "walks ashore");
        // Disbanding a ship in harbour does the same.
        let ship = spawn(&mut game, RED, "transport", 1, 1);
        board(&mut game, rider, ship);
        run(&mut game, RED, Command::Disband { unit: ship }).unwrap();
        assert!(game.units.contains_key(&rider));
    }

    #[test]
    fn a_ship_sunk_in_battle_takes_its_passengers_with_it() {
        let (mut sunk, mut survived) = (0, 0);
        for seed in 1..=40u64 {
            let mut game = map(&COAST);
            seeded(&mut game, seed);
            let ship = spawn(&mut game, BLUE, "transport", 6, 2);
            let rider = spawn(&mut game, BLUE, "infantry", 6, 2);
            board(&mut game, rider, ship);
            // An ironclad against a full-strength transport: either side may win.
            let attacker = spawn(&mut game, RED, "ironclad", 5, 2);
            run(
                &mut game,
                RED,
                Command::Attack {
                    unit: attacker,
                    target: at(6, 2),
                },
            )
            .unwrap();
            if game.units.contains_key(&ship) {
                survived += 1;
                assert_eq!(unit(&game, rider).carrier, Some(ship));
            } else {
                sunk += 1;
                assert!(
                    !game.units.contains_key(&rider),
                    "seed {seed}: the passenger survived"
                );
            }
        }
        assert!(sunk > 0 && survived > 0, "{sunk} sunk, {survived} survived");
    }

    #[test]
    fn passengers_never_defend_and_ships_in_port_cannot_defend_the_city() {
        let rules = rules();
        let mut game = map(&COAST);
        let ship = spawn(&mut game, BLUE, "transport", 5, 2);
        let rider = spawn(&mut game, BLUE, "infantry", 5, 2);
        board(&mut game, rider, ship);
        // The infantryman is the better soldier, but it only rides.
        assert_eq!(game.best_defender(at(5, 2), BLUE, &rules), Some(ship));

        let mut game = map(&HARBOUR);
        let port = at(10, 4);
        let ship = spawn(&mut game, BLUE, "transport", 10, 4);
        let rider = spawn(&mut game, BLUE, "infantry", 10, 4);
        board(&mut game, rider, ship);
        spawn(&mut game, BLUE, "ironclad", 10, 4);
        assert_eq!(game.best_defender(port, BLUE, &rules), None);
        let raider = spawn(&mut game, RED, "cavalry", 9, 4);
        run(
            &mut game,
            RED,
            Command::Attack {
                unit: raider,
                target: port,
            },
        )
        .unwrap();
        let city = game.city_at(port).unwrap();
        assert_eq!(game.cities[&city].owner, RED, "taken by walking in");
        assert!(
            game.units.values().all(|u| u.owner == RED),
            "the ships and their passengers were lost with the city"
        );
    }

    #[test]
    fn only_a_lethal_bombardment_can_kill() {
        let rules = rules();
        // A militiaman on the shore and an ironclad at sea, each down to one hit point.
        let targets = [("militia", at(3, 2)), ("ironclad", at(5, 2))];
        for (victim, place) in targets {
            let mut kills = 0;
            for seed in 1..=60u64 {
                for gun in ["gunboat", "torpedo-boat"] {
                    let mut game = map(&COAST);
                    seeded(&mut game, seed);
                    let target = spawn(&mut game, BLUE, victim, place.x, place.y);
                    let hull = unit(&game, target).max_hp(&rules.units[victim]);
                    game.units.get_mut(&target).unwrap().damage = hull - 1;
                    let boat = spawn(&mut game, RED, gun, 4, 2);
                    let result = run(
                        &mut game,
                        RED,
                        Command::Bombard {
                            unit: boat,
                            target: place,
                        },
                    );
                    if gun == "gunboat" {
                        let refusal = error(result);
                        assert!(refusal.contains("Nothing"), "{refusal}");
                        assert_eq!(unit(&game, target).hp(&rules.units[victim]), 1);
                    } else {
                        result.unwrap();
                        if !game.units.contains_key(&target) {
                            kills += 1;
                            assert!(game.log.last().unwrap().contains("destroyed"));
                        }
                    }
                }
            }
            // Two shots at 88% (shore) or 65% (sea) a shot.
            assert!(kills >= 40, "{victim}: {kills} kills in 60");
        }
    }

    #[test]
    fn lethality_is_per_domain_and_a_gun_that_is_not_lethal_stops_at_one_hit_point() {
        let rules = rules();
        let mut game = map(&COAST);
        let militia = spawn(&mut game, BLUE, "militia", 3, 2);
        let ironclad = spawn(&mut game, BLUE, "ironclad", 5, 2);
        for id in [militia, ironclad] {
            let hull = rules.def(unit(&game, id)).hp + 3;
            game.units.get_mut(&id).unwrap().damage = hull - 1;
        }
        let mut shore_only = rules.units["torpedo-boat"].clone();
        shore_only.lethal_sea = false;
        let mut sea_only = rules.units["torpedo-boat"].clone();
        sea_only.lethal_land = false;
        assert_eq!(
            game.bombard_victim(&shore_only, at(3, 2), BLUE, &rules),
            Some(militia)
        );
        assert_eq!(
            game.bombard_victim(&shore_only, at(5, 2), BLUE, &rules),
            None
        );
        assert_eq!(
            game.bombard_victim(&sea_only, at(5, 2), BLUE, &rules),
            Some(ironclad)
        );
        assert_eq!(game.bombard_victim(&sea_only, at(3, 2), BLUE, &rules), None);
    }

    #[test]
    fn ships_in_port_are_easy_targets_and_are_shot_first_by_other_ships() {
        let rules = rules();
        let mut game = map(&HARBOUR);
        let port = at(10, 4);
        let docked = spawn(&mut game, BLUE, "ironclad", 10, 4);
        let infantry = spawn(&mut game, BLUE, "infantry", 10, 4);
        let sailing = spawn(&mut game, BLUE, "ironclad", 9, 3);
        let def = &rules.units["ironclad"];
        let afloat = game.bombard_odds(8, unit(&game, sailing), def);
        let moored = game.bombard_odds(8, unit(&game, docked), def);
        assert_eq!(
            moored,
            (afloat + 1) / 2,
            "the defender's odds are halved in port"
        );

        let torpedo = &rules.units["torpedo-boat"];
        let gun = &rules.units["artillery"];
        assert_eq!(
            game.bombard_victim(torpedo, port, BLUE, &rules),
            Some(docked)
        );
        assert_eq!(game.bombard_victim(gun, port, BLUE, &rules), Some(infantry));
        game.remove_unit(infantry);
        assert_eq!(
            game.bombard_victim(gun, port, BLUE, &rules),
            None,
            "guns do not reach ships"
        );
        // A gun cannot be turned on the open sea either.
        assert_eq!(game.bombard_victim(gun, at(9, 3), BLUE, &rules), None);
        assert_eq!(
            game.bombard_victim(torpedo, at(9, 3), BLUE, &rules),
            Some(sailing)
        );
    }

    /// Passenger links and the square index must stay whole however the ships are handled.
    fn assert_manifests_hold(game: &Game, rules: &Rules) {
        for u in game.units.values() {
            let def = rules.def(u);
            assert!(
                game.unit_ids_at(u.position).contains(&u.id),
                "{u:?} is not indexed"
            );
            if let Some(ship) = u.carrier {
                let s = &game.units[&ship];
                assert_eq!(
                    (s.position, s.owner),
                    (u.position, u.owner),
                    "{u:?} vs {s:?}"
                );
                assert!(rules.def(s).is_carrier());
                assert_eq!(def.domain, Domain::Land);
            }
            let wet = !game.map.get(u.position).unwrap().is_land();
            match def.domain {
                Domain::Land => assert!(!wet || u.carrier.is_some(), "{u:?} is swimming"),
                Domain::Sea => {
                    assert!(wet || game.is_port(u.owner, u.position), "{u:?} is aground")
                }
            }
        }
        for ship in game.units.values().filter(|s| rules.def(s).is_carrier()) {
            assert!(game.passengers(ship.id).len() as u32 <= rules.def(ship).capacity);
        }
    }

    #[test]
    fn passenger_links_survive_random_play() {
        let rules = rules();
        for seed in 1..=30u64 {
            let mut game = map(&HARBOUR);
            seeded(&mut game, seed);
            for (owner, kind, x, y) in [
                (RED, "transport", 1, 1),
                (RED, "transport", 0, 0),
                (RED, "infantry", 1, 1),
                (RED, "infantry", 1, 1),
                (RED, "infantry", 2, 2),
                (RED, "cavalry", 3, 3),
                (RED, "battleship", 1, 0),
                (BLUE, "transport", 10, 4),
                (BLUE, "infantry", 10, 4),
                (BLUE, "infantry", 9, 4),
                (BLUE, "torpedo-boat", 10, 4),
                (BLUE, "cruiser", 9, 3),
                (BLUE, "ironclad", 8, 3),
            ] {
                spawn(&mut game, owner, kind, x, y);
            }
            for _ in 0..150 {
                let ids: Vec<Id> = game.units.keys().copied().collect();
                if ids.is_empty() {
                    break;
                }
                let id = ids[game.random(ids.len() as u32) as usize];
                let other = ids[game.random(ids.len() as u32) as usize];
                let owner = game.units[&id].owner;
                let position = game.units[&id].position;
                let target = match game.random(3) {
                    0 => Coord::new(game.random(12), game.random(6)),
                    _ => position.offset(game.random(3) - 1, game.random(3) - 1),
                };
                let command = match game.random(9) {
                    0..=2 => Command::Move {
                        unit: id,
                        destination: target,
                    },
                    3 => Command::Load {
                        unit: id,
                        carrier: other,
                    },
                    4 => Command::Unload { unit: id },
                    5 => Command::Attack { unit: id, target },
                    6 => Command::Bombard { unit: id, target },
                    7 => Command::Disband { unit: id },
                    _ => Command::EndTurn,
                };
                let player = if matches!(command, Command::EndTurn) {
                    RED
                } else {
                    owner
                };
                let _ = game.apply(player, command, &rules, TickRules::default());
                assert_manifests_hold(&game, &rules);
            }
        }
    }
}
