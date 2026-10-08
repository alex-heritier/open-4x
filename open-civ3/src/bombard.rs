//! Land artillery: units, cities and improvements (`combat.md` 8, `colonies.md` 8).
use bevy::prelude::*;
use civ3_rules::combat as exe;

use crate::cities::{City, Production};
use crate::combat::CombatRng;
use crate::diplomacy::Diplomacy;
use crate::features::{MessageBoard, post};
use crate::map::{GameMap, MP, Tile};
use crate::roster;
use crate::units::{Unit, UnitAnim, UnitType, def};

#[derive(Resource, Default)]
pub struct TargetMode(pub Option<Entity>);

#[derive(Message, Clone, Copy)]
pub struct Order {
    pub attacker: Entity,
    pub to: (i32, i32),
}

pub fn capable(t: UnitType) -> bool {
    let d = def(t);
    d.class == 0
        && d.bombard > 0
        && d.bomb_range > 0
        && d.rof > 0
        && d.special & roster::special::BOMBARD != 0
        && d.abilities & ((1 << 27) | (1 << 28)) == 0
}

pub fn improved(t: &Tile) -> bool {
    t.road || t.mine || t.irrigation || t.fortress || t.barricade
}

fn terrain_pct(map: &GameMap, at: (i32, i32)) -> i32 {
    let t = map.get(at.0, at.1).expect("validated bombard destination");
    // Same terrain precedence as melee combat.
    exe::TERRAIN_DEFENSE_PCT[crate::combat::terrain_row(t)]
}

fn tile_pct(city: Option<&City>) -> i32 {
    city.map_or(0, |c| {
        exe::tile_term(
            exe::Structure::City {
                size: i32::from(c.size()),
                resisters: crate::resistance::resisters(c),
                building_pct: crate::combat::Hold::of(c).building_pct,
            },
            false,
            &exe::Rules::CONQUESTS,
        )
    })
}

pub fn arm(
    mut commands: MessageReader<crate::actionbar::UnitCommand>,
    selected: Res<crate::units::Selected>,
    units: Query<&Unit>,
    map: Res<GameMap>,
    cities: Query<&City>,
    civs: Res<crate::civs::Civilizations>,
    mut mode: ResMut<TargetMode>,
    mut goto: ResMut<crate::actionbar::GotoMode>,
    mut board: ResMut<MessageBoard>,
) {
    let spots: Vec<_> = cities.iter().map(|c| (c.x, c.y)).collect();
    for cmd in commands.read() {
        let Some(e) = selected.0 else {
            mode.0 = None;
            continue;
        };
        let Ok(u) = units.get(e) else {
            mode.0 = None;
            continue;
        };
        if u.civ != civs.active || !cmd.relevant(u.utype) {
            continue;
        }
        if *cmd == crate::actionbar::UnitCommand::Bombard && cmd.enabled(&map, &spots, u) {
            mode.0 = if mode.0 == Some(e) { None } else { Some(e) };
            goto.0 = false;
            if mode.0.is_some() {
                post(&mut board, "Bombard: click a target (Esc cancels).");
            }
        } else {
            mode.0 = None;
        }
    }
}

fn target(
    map: &GameMap,
    att: &Unit,
    at: (i32, i32),
    city: Option<&City>,
    units: &[(Entity, Unit)],
    diplomacy: &Diplomacy,
) -> Option<Entity> {
    let tile = exe::BombardTile {
        is_water: map.get(at.0, at.1).is_some_and(|t| {
            matches!(
                t.base,
                crate::map::Base::Coast | crate::map::Base::Sea | crate::map::Base::Ocean
            )
        }),
        has_city: city.is_some(),
    };
    let ranks: Vec<_> = units
        .iter()
        .map(|(_, u)| {
            let d = def(u.utype);
            let domain = match d.class {
                1 => exe::Domain::Sea,
                2 => exe::Domain::Air,
                _ => exe::Domain::Land,
            };
            let fortify = exe::fortify_term(
                true,
                false,
                u.fortified && !u.sentry,
                1,
                &exe::Rules::CONQUESTS,
            );
            (
                exe::BombardCandidate {
                    carried: u.carrier.is_some(),
                    defense: d.defense,
                    domain,
                    remaining_hp: u.hp(),
                    hostile: diplomacy.at_war(att.civ, u.civ),
                },
                exe::DefenderRank {
                    rating: exe::defender_rating(fortify, d.defense, u.hp()),
                    is_king: d.abilities >> 29 & 1 != 0,
                    cargo: 0,
                    attack: d.attack,
                    bombard: d.bombard,
                    max_hp: u.max_hp(),
                },
            )
        })
        .collect();
    let order = exe::bombard_target_order(exe::Domain::Land, false, tile.is_water);
    exe::pick_bombard_target(order, &tile, exe::Lethality::NONE, &ranks).map(|i| units[i].0)
}

/// Land artillery is nonlethal in the Ancient Age. Later lethal artillery
/// and aircraft need their own entry paths; they are not handled here.
pub fn resolve(
    mut orders: MessageReader<Order>,
    mut units: Query<(Entity, &mut Unit)>,
    mut cities: Query<(Entity, &mut City)>,
    mut map: ResMut<GameMap>,
    mut diplomacy: ResMut<Diplomacy>,
    mut rng: ResMut<CombatRng>,
    civs: Res<crate::civs::Civilizations>,
    mut board: ResMut<MessageBoard>,
) {
    for order in orders.read() {
        let Ok((_, att)) = units.get(order.attacker) else {
            continue;
        };
        let att = att.clone();
        let at = (map.wrap_x(order.to.0), order.to.1);
        if !capable(att.utype)
            || att.civ != civs.active
            || att.moves == 0
            || att.attacked
            || map.get(at.0, at.1).is_none()
            || map.distance((att.x, att.y), at) == 0
            || map.distance((att.x, att.y), at) > def(att.utype).bomb_range
            || (!crate::civs::is_ai(att.civ) && !map.get(at.0, at.1).is_some_and(|t| t.visible))
        {
            if att.civ == civs.viewer() {
                post(&mut board, "This tile cannot be bombarded by this unit.");
            }
            continue;
        }
        let city = cities
            .iter()
            .find(|(_, c)| (c.x, c.y) == at)
            .map(|(e, c)| (e, c.clone()));
        let owner = map.get(at.0, at.1).and_then(|t| t.owner).map(usize::from);
        let stack: Vec<_> = units
            .iter()
            .filter(|(_, u)| u.carrier.is_none() && (u.x, u.y) == at)
            .map(|(e, u)| (e, u.clone()))
            .collect();
        if city
            .as_ref()
            .is_some_and(|(_, c)| !diplomacy.at_war(att.civ, c.civ))
            || stack.iter().any(|(_, u)| !diplomacy.at_war(att.civ, u.civ))
        {
            if att.civ == civs.viewer() {
                post(
                    &mut board,
                    "We must be at war to bombard these units or this city.",
                );
            }
            continue;
        }
        let chosen = target(
            &map,
            &att,
            at,
            city.as_ref().map(|(_, c)| c),
            &stack,
            &diplomacy,
        );
        let terrain_target =
            chosen.is_none() && city.is_none() && improved(map.get(at.0, at.1).unwrap());
        if terrain_target && owner.is_some_and(|civ| !diplomacy.at_war(att.civ, civ)) {
            if att.civ == civs.viewer() {
                post(
                    &mut board,
                    "We must be at war to bombard these improvements.",
                );
            }
            continue;
        }
        if chosen.is_none() && city.is_none() && !terrain_target {
            if att.civ == civs.viewer() {
                post(&mut board, "There is no bombardment target here.");
            }
            continue;
        }
        if let Ok((_, mut u)) = units.get_mut(order.attacker) {
            u.moves = u.moves.saturating_sub(MP);
            u.attacked = true;
            u.rested = false;
            u.fortified = false;
            u.sentry = false;
            u.path.clear();
            u.exploring = false;
            u.work = None;
            u.facing = crate::combat::face(&map, (u.x, u.y), at);
            u.anim = UnitAnim::OneShot {
                slot: "ATTACK1",
                t: 0.0,
            };
        }
        let terrain = terrain_pct(&map, at);
        let tile = tile_pct(city.as_ref().map(|(_, c)| c));
        let d = def(att.utype);
        let mut result = "Bombardment failed.".to_string();
        let mut shoot_units = true;
        if let Some((e, c)) = &city {
            let facilities: Vec<_> = roster::BLDGS
                .iter()
                .enumerate()
                .map(|(row, b)| {
                    let p = Production::from_building_row(row);
                    exe::FacilityFacts {
                        in_city: c.has(p)
                            && (b.govt < 0
                                || crate::realm::read(c.civ, |r| r.govt as i32) == b.govt),
                        acts_on_city: crate::citycalc::active(c).any(|(owned, _)| owned == p),
                        obsolete: b.obsolete >= 0
                            && crate::realm::read(c.civ, |r| r.knows(b.obsolete)),
                        // Shipped Ancient Age bombard defense is Walls' 8.
                        land_defense: b.bombard_defense,
                        sea_defense: 0,
                    }
                })
                .collect();
            let wall = exe::walls_step(
                rng.0.reference(),
                &exe::WallsAttack {
                    domain: exe::Domain::Land,
                    bombard: d.bombard,
                    rate_of_fire: d.rof,
                    terrain_pct: terrain,
                    tile_pct: tile,
                },
                &exe::CityWalls {
                    pop: i32::from(c.size()),
                    facilities: &facilities,
                    wonder_count: 0,
                },
                &exe::Rules::CONQUESTS,
            );
            shoot_units = wall.attacks_units();
            if let exe::WallsStep::Hit(hit) = wall {
                let row = match hit {
                    exe::FacilityHit::Destroyed(row) | exe::FacilityHit::Reported(row) => Some(row),
                    _ => None,
                };
                if let Some(row) = row {
                    let p = Production::from_building_row(row);
                    if matches!(hit, exe::FacilityHit::Destroyed(_)) {
                        cities.get_mut(*e).unwrap().1.buildings.retain(|&b| b != p);
                    }
                    result = format!("Bombardment struck {} in {}.", p.name(), c.name);
                }
            }
        }
        if shoot_units {
            if let Some(e) = chosen {
                let mut u = units.get_mut(e).unwrap().1;
                let odds = exe::defender_round_odds(&exe::OddsInput {
                    att_strength: d.bombard,
                    att_army_bonus: 0,
                    att_pct: 0,
                    def_strength: exe::defense_strength(&[], def(u.utype).defense, false),
                    def_army_bonus: 0,
                    def_pct: crate::combat::defense_pct(
                        &map,
                        &u,
                        city.as_ref().map(|(_, c)| crate::combat::Hold::of(c)),
                    ),
                })
                .expect("positive bombard and defense");
                let mut hp = exe::Fighter {
                    max_hp: u.max_hp(),
                    damage: u.damage,
                    retreat_pct: 0,
                    owned: true,
                };
                let domain = if def(u.utype).class == 1 {
                    exe::Domain::Sea
                } else {
                    exe::Domain::Land
                };
                let odds = if domain == exe::Domain::Sea && city.is_some() {
                    exe::port_odds(odds)
                } else {
                    odds
                };
                let volley = exe::ranged_volley(
                    rng.0.reference(),
                    odds,
                    d.rof,
                    &mut hp,
                    domain,
                    exe::Lethality::NONE,
                );
                u.damage = hp.damage;
                if volley.hits > 0 {
                    u.rested = false;
                    result = format!(
                        "Bombardment hit {} for {} damage.",
                        def(u.utype).name,
                        volley.hits
                    );
                    diplomacy.note_attack(att.civ, 1 << u.civ);
                    // A target left at exactly one point books an incident.
                    if u.max_hp() - u.damage == 1 {
                        diplomacy.incident(att.civ, u.civ, 1);
                    }
                }
            } else if let Some((e, c)) = &city {
                let mode =
                    exe::StrikeMode::from_mode(rng.0.below(exe::NO_TARGET_CITY_MODE_DIE)).unwrap();
                if mode != exe::StrikeMode::Population || c.size() > 1 {
                    let odds = exe::strike_odds(
                        mode.base(&exe::Rules::CONQUESTS),
                        terrain,
                        tile,
                        d.bombard,
                    )
                    .unwrap();
                    if exe::strike_hit(rng.0.reference(), odds, d.rof) {
                        let mut c = cities.get_mut(*e).unwrap().1;
                        if mode == exe::StrikeMode::Population {
                            c.lose_population(1, None, &mut rng.0);
                            result = format!("Bombardment killed a citizen in {}.", c.name);
                        } else {
                            let buildings: Vec<_> = c
                                .buildings
                                .iter()
                                .copied()
                                .filter(|p| {
                                    p.bldg().is_some_and(|b| {
                                        !b.is_great_wonder() && !b.is_small_wonder()
                                    })
                                })
                                .collect();
                            if !buildings.is_empty() {
                                let p = buildings[rng.0.below(buildings.len() as u32) as usize];
                                c.buildings.retain(|&b| b != p);
                                result =
                                    format!("Bombardment destroyed {} in {}.", p.name(), c.name);
                            }
                        }
                        diplomacy.note_attack(att.civ, 1 << c.civ);
                        if mode == exe::StrikeMode::Population {
                            diplomacy.incident(att.civ, c.civ, 1);
                        }
                    }
                }
            } else if terrain_target {
                let odds =
                    exe::strike_odds(exe::TILE_DEFENSE_STRENGTH, terrain, 0, d.bombard).unwrap();
                // The improvement roll is one die (0x4A2460), independent of rate of fire.
                if exe::strike_hit(rng.0.reference(), odds, 1) {
                    let i = map.idx(at.0, at.1);
                    let t = &mut map.tiles[i];
                    // 0x5B4DC0 clears all four low overlay bits together.
                    if let Some(l) = crate::actions::loot(t) {
                        crate::actions::strip(t, l);
                    }
                    result = "Bombardment destroyed the tile's improvements.".to_string();
                    if let Some(civ) = owner {
                        diplomacy.note_attack(att.civ, 1 << civ);
                    }
                }
            }
        }
        if att.civ == civs.viewer()
            || stack.iter().any(|(_, u)| u.civ == civs.viewer())
            || city.as_ref().is_some_and(|(_, c)| c.civ == civs.viewer())
            || terrain_target && owner == Some(civs.viewer())
        {
            post(&mut board, result);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{Base, Cover, Relief};

    fn arena(seed: u32, war: bool) -> (App, Entity, Entity) {
        crate::realm::reset();
        let mut map = GameMap::generate();
        for t in &mut map.tiles {
            t.base = Base::Grassland;
            t.relief = Relief::Flat;
            t.cover = Cover::Bare;
            t.river = 0;
            t.visible = true;
        }
        let mut diplomacy = Diplomacy::new();
        if war {
            diplomacy.declare(
                &crate::diplomacy::Facts::even(),
                0,
                1,
                0,
                &mut MessageBoard::default(),
            );
        }
        let mut app = App::new();
        app.edit_schedule(Update, |s| {
            s.set_executor_kind(bevy::ecs::schedule::ExecutorKind::SingleThreaded);
        });
        app.insert_resource(map);
        app.insert_resource(diplomacy);
        app.insert_resource(CombatRng(crate::rng::MapRng::new(seed)));
        app.init_resource::<crate::civs::Civilizations>();
        app.init_resource::<MessageBoard>();
        app.add_message::<Order>();
        app.init_resource::<crate::cities::BorderKey>();
        app.add_systems(Update, (crate::cities::update_borders, resolve).chain());
        let a = app
            .world_mut()
            .spawn(Unit::new(0, UnitType::named("Catapult"), 10, 10))
            .id();
        let d = app
            .world_mut()
            .spawn(Unit::new(1, UnitType::named("Spearman"), 11, 10))
            .id();
        (app, a, d)
    }

    fn fire(app: &mut App, attacker: Entity, to: (i32, i32)) {
        app.world_mut().write_message(Order { attacker, to });
        app.update();
    }

    #[test]
    fn terrain_strikes_match_reference_dice_and_clear_improvements_together() {
        let mut hits = 0;
        for seed in 0..80 {
            for occupied in [false, true] {
                let (mut app, a, d) = arena(seed, true);
                if occupied {
                    // A defender already at the nonlethal HP floor cannot absorb the shot.
                    app.world_mut().get_mut::<Unit>(d).unwrap().damage = 2;
                } else {
                    app.world_mut().despawn(d);
                }
                {
                    let mut map = app.world_mut().resource_mut::<GameMap>();
                    let i = map.idx(11, 10);
                    let t = &mut map.tiles[i];
                    t.road = true;
                    t.mine = true;
                    t.irrigation = true;
                }
                let mut reference = crate::rng::MapRng::new(seed);
                let odds = exe::strike_odds(exe::TILE_DEFENSE_STRENGTH, 10, 0, 4).unwrap();
                let hit = exe::strike_hit(reference.reference(), odds, 1);
                fire(&mut app, a, (11, 10));
                let t = app.world().resource::<GameMap>().get(11, 10).unwrap();
                assert_eq!((t.road, t.mine, t.irrigation), (!hit, !hit, !hit));
                let u = app.world().get::<Unit>(a).unwrap();
                assert_eq!((u.x, u.y, u.moves, u.attacked), (10, 10, 0, true));
                if occupied {
                    assert_eq!(app.world().get::<Unit>(d).unwrap().hp(), 1);
                }
                assert_eq!(
                    app.world_mut().resource_mut::<CombatRng>().0.below(1024),
                    reference.below(1024)
                );
                hits += i32::from(hit);
            }
        }
        assert!(hits > 0 && hits < 160);
    }

    #[test]
    fn terrain_strikes_refuse_own_peaceful_and_empty_tiles_without_spending_dice_or_moves() {
        for case in ["ours", "peace", "empty"] {
            let (mut app, a, d) = arena(1, false);
            app.world_mut().despawn(d);
            if case != "empty" {
                let civ = if case == "ours" { 0 } else { 1 };
                app.world_mut().spawn(City::new(civ, "Town", 12, 10));
                let mut map = app.world_mut().resource_mut::<GameMap>();
                let i = map.idx(11, 10);
                map.tiles[i].road = true;
            }
            fire(&mut app, a, (11, 10));
            assert_eq!(app.world().get::<Unit>(a).unwrap().moves, MP, "{case}");
            let mut expected = crate::rng::MapRng::new(1);
            assert_eq!(
                app.world_mut().resource_mut::<CombatRng>().0.below(1024),
                expected.below(1024)
            );
        }
    }

    #[test]
    fn catapult_damage_and_dice_match_the_reference_without_moving_or_killing() {
        let mut hits = 0;
        for seed in 0..80 {
            let (mut app, a, d) = arena(seed, true);
            {
                let mut map = app.world_mut().resource_mut::<GameMap>();
                let i = map.idx(11, 10);
                map.tiles[i].road = true;
            }
            let mut reference = civ3_worldgen::rng::Rng::new(seed);
            let mut hp = exe::Fighter {
                max_hp: 3,
                damage: 0,
                retreat_pct: 0,
                owned: true,
            };
            let odds = exe::defender_round_odds(&exe::OddsInput {
                att_strength: 4,
                att_army_bonus: 0,
                att_pct: 0,
                def_strength: 2,
                def_army_bonus: 0,
                def_pct: 10,
            })
            .unwrap();
            let v = exe::ranged_volley(
                &mut reference,
                odds,
                1,
                &mut hp,
                exe::Domain::Land,
                exe::Lethality::NONE,
            );
            fire(&mut app, a, (11, 10));
            let att = app.world().get::<Unit>(a).unwrap();
            let dfn = app.world().get::<Unit>(d).unwrap();
            assert_eq!((att.x, att.y, att.moves, att.attacked), (10, 10, 0, true));
            assert_eq!(dfn.damage, v.hits);
            assert!(
                app.world().resource::<GameMap>().get(11, 10).unwrap().road,
                "a legal unit target takes precedence over its improvements"
            );
            assert_eq!(
                app.world_mut().resource_mut::<CombatRng>().0.below(1024),
                reference.below(1024)
            );
            hits += v.hits;
            // Repeated orders cannot fire again this turn.
            fire(&mut app, a, (11, 10));
            assert_eq!(app.world().get::<Unit>(d).unwrap().damage, v.hits);
        }
        assert!(hits > 20 && hits < 75, "hits and misses both exercised");
    }

    #[test]
    fn bombard_rejects_peace_range_fog_and_one_hp_targets_without_spending_a_move() {
        for case in ["peace", "range", "fog", "one hp", "worker"] {
            let (mut app, a, d) = arena(1, case != "peace");
            let at = if case == "range" { (12, 10) } else { (11, 10) };
            if case == "fog" {
                let mut map = app.world_mut().resource_mut::<GameMap>();
                let i = map.idx(11, 10);
                map.tiles[i].visible = false;
            } else if case == "one hp" {
                app.world_mut().get_mut::<Unit>(d).unwrap().damage = 2;
            } else if case == "worker" {
                app.world_mut().get_mut::<Unit>(d).unwrap().utype = UnitType::named("Worker");
            }
            let before = app.world().get::<Unit>(d).unwrap().damage;
            fire(&mut app, a, at);
            assert_eq!(app.world().get::<Unit>(a).unwrap().moves, MP, "{case}");
            assert_eq!(app.world().get::<Unit>(d).unwrap().damage, before, "{case}");
            let mut expected = crate::rng::MapRng::new(1);
            assert_eq!(
                app.world_mut().resource_mut::<CombatRng>().0.below(1024),
                expected.below(1024)
            );
        }
    }

    #[test]
    fn walls_absorb_a_city_bombardment_and_leave_the_garrison_intact() {
        let mut destroyed = 0;
        for seed in 0..80 {
            let (mut app, a, d) = arena(seed, true);
            let mut city = City::new(1, "Rome", 11, 10);
            city.buildings.push(Production::named("Walls"));
            let c = app.world_mut().spawn(city).id();
            fire(&mut app, a, (11, 10));
            assert_eq!(app.world().get::<Unit>(d).unwrap().damage, 0);
            if !app
                .world()
                .get::<City>(c)
                .unwrap()
                .has(Production::named("Walls"))
            {
                destroyed += 1;
            }
        }
        assert!(destroyed > 0 && destroyed < 80);
    }

    #[test]
    fn undefended_cities_lose_population_or_buildings_but_keep_wonders() {
        let (mut citizens, mut buildings) = (0, 0);
        for seed in 0..200 {
            let (mut app, a, d) = arena(seed, true);
            app.world_mut().despawn(d);
            let mut city = City::new(1, "Rome", 11, 10);
            city.set_size(3);
            city.buildings.extend([
                Production::named("Temple"),
                Production::named("The Pyramids"),
            ]);
            let c = app.world_mut().spawn(city).id();
            fire(&mut app, a, (11, 10));
            let c = app.world().get::<City>(c).unwrap();
            assert!(c.has(Production::named("The Pyramids")));
            citizens += i32::from(c.size() == 2);
            buildings += i32::from(!c.has(Production::named("Temple")));
        }
        assert!(citizens > 0 && buildings > 0);
    }
}
