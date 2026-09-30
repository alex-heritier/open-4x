//! Local hotseat civilizations. Every slot is controlled by the player.
use bevy::prelude::*;

use crate::map::GameMap;

pub const CIV_COUNT: usize = 3;

pub struct CivDefinition {
    pub name: &'static str,
    pub adjective: &'static str,
    pub color: Color,
    pub city_names: &'static [&'static str],
}

pub const CIVS: [CivDefinition; CIV_COUNT] = [
    CivDefinition {
        name: "Japan",
        adjective: "Japanese",
        color: Color::srgb_u8(25, 148, 24),
        city_names: &[
            "Kyoto",
            "Osaka",
            "Tokyo",
            "Edo",
            "Nagoya",
            "Kobe",
            "Yokohama",
            "Hiroshima",
            "Nagasaki",
            "Nara",
            "Sapporo",
            "Sendai",
            "Niigata",
            "Okayama",
            "Fukuoka",
            "Kagoshima",
            "Matsuyama",
            "Kanazawa",
            "Takamatsu",
            "Oita",
        ],
    },
    CivDefinition {
        name: "Rome",
        adjective: "Roman",
        color: Color::srgb_u8(190, 48, 48),
        city_names: &[
            "Rome", "Veii", "Antium", "Cumae", "Neapolis", "Pompeii", "Pisae", "Ravenna",
        ],
    },
    CivDefinition {
        name: "Egypt",
        adjective: "Egyptian",
        color: Color::srgb_u8(220, 184, 48),
        city_names: &[
            "Thebes",
            "Memphis",
            "Heliopolis",
            "Elephantine",
            "Alexandria",
            "Pi-Ramesses",
            "Giza",
            "Byblos",
        ],
    },
];

#[derive(Resource, Default)]
pub struct Civilizations {
    pub active: usize,
}

/// Emitted once for each outgoing civilization, before the next player acts.
#[derive(Message)]
pub struct CivilizationEnded(pub usize);

/// Handoffs move the view to the incoming player's unit or capital.
pub fn focus_active_civ(
    civs: Res<Civilizations>,
    selected: Res<crate::units::Selected>,
    units: Query<&crate::units::Unit>,
    cities: Query<&crate::cities::City>,
    mut camera: Query<&mut Transform, With<Camera2d>>,
    mut previous: Local<usize>,
) {
    if *previous == civs.active {
        return;
    }
    *previous = civs.active;
    let position = selected
        .0
        .and_then(|e| units.get(e).ok())
        .map(|u| (u.x, u.y))
        .or_else(|| {
            cities
                .iter()
                .find(|c| c.civ == civs.active)
                .map(|c| (c.x, c.y))
        })
        .or_else(|| {
            units
                .iter()
                .find(|u| u.civ == civs.active)
                .map(|u| (u.x, u.y))
        });
    if let (Some((x, y)), Ok(mut transform)) = (position, camera.single_mut()) {
        let world = crate::map::tile_to_world(x, y);
        transform.translation.x = world.x;
        transform.translation.y = world.y;
    }
}

/// Spread parties over settleable land, using wrapped map distance.
pub fn starting_positions(map: &GameMap) -> [(i32, i32); CIV_COUNT] {
    let mut starts = [map.start; CIV_COUNT];
    for civ in 1..CIV_COUNT {
        let mut best_distance = -1;
        for y in 0..map.h {
            for x in 0..map.w {
                if !crate::cities::can_found(map, &starts[..civ], x, y) {
                    continue;
                }
                let distance = starts[..civ]
                    .iter()
                    .map(|&(sx, sy)| {
                        let dx = (x - sx).abs();
                        dx.min(map.w - dx).max((y - sy).abs())
                    })
                    .min()
                    .unwrap();
                if distance > best_distance {
                    best_distance = distance;
                    starts[civ] = (x, y);
                }
            }
        }
        assert!(
            best_distance >= 0,
            "map must have room for all civilizations"
        );
    }
    starts
}
