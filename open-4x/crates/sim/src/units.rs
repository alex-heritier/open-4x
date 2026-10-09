//! Individual units, Civ3-style: each soldier, worker, and ship is its own piece with hit
//! points, movement, an experience level, and a standing order.
use crate::{Id, improvements::Job, terrain::Coord};
use serde::{Deserialize, Serialize};

/// Movement is counted in thirds of a full move, so a road step (1/3) is exact.
pub const MOVE_UNIT: u32 = 3;
/// Base hit points by experience level: Conscript, Regular, Veteran, Elite.
pub const BASE_HP: [i32; 4] = [2, 3, 4, 5];
/// Chance weight to withdraw from a fight at one hit point, by level.
pub const RETREAT_PERCENT: [i32; 4] = [34, 50, 58, 66];
pub const LEVEL_NAMES: [&str; 4] = ["Conscript", "Regular", "Veteran", "Elite"];
/// New and scenario units are Regular.
pub const STARTING_LEVEL: u8 = 1;
/// Defender bonus for a fortified land unit, in percent.
pub const FORTIFY_PERCENT: i32 = 25;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Domain {
    #[default]
    Land,
    Sea,
}

/// A unit design from the content pack. Strengths use Civ3's scale.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UnitDef {
    pub id: String,
    pub name: String,
    pub cost: i32,
    pub sprite: String,
    pub domain: Domain,
    pub attack: i32,
    pub defense: i32,
    /// Ranged strength. Zero means the unit cannot bombard.
    #[serde(default)]
    pub bombard: i32,
    /// Farthest tile a bombardment can reach.
    #[serde(default)]
    pub range: i32,
    /// Shots fired in one bombardment.
    #[serde(default)]
    pub rate_of_fire: i32,
    /// Civ3's Lethal Land Bombardment: bombardment may kill land units instead of stopping
    /// at one hit point.
    #[serde(default)]
    pub lethal_land: bool,
    /// Civ3's Lethal Sea Bombardment: the same against ships, at sea or in port.
    #[serde(default)]
    pub lethal_sea: bool,
    /// Hit points added to the experience level's base.
    #[serde(default)]
    pub hp: i32,
    /// Whole tiles of movement per turn.
    pub moves: u32,
    /// May attack again after attacking, for as long as movement lasts.
    #[serde(default)]
    pub blitz: bool,
    /// Terrain work per turn. Zero means the unit cannot build improvements.
    #[serde(default)]
    pub work: u32,
    #[serde(default)]
    pub settler: bool,
    /// Land units a ship can carry. Zero means it carries none.
    #[serde(default)]
    pub capacity: u32,
}
impl UnitDef {
    pub fn is_naval(&self) -> bool {
        self.domain == Domain::Sea
    }
    /// Units that can start a fight or take a city.
    pub fn can_attack(&self) -> bool {
        self.attack > 0
    }
    pub fn can_bombard(&self) -> bool {
        self.bombard > 0 && self.range > 0 && self.rate_of_fire > 0
    }
    /// Ships that carry land units.
    pub fn is_carrier(&self) -> bool {
        self.capacity > 0
    }
    /// Whether this design's bombardment can kill a unit of `target`'s domain.
    pub fn lethal_against(&self, target: Domain) -> bool {
        match target {
            Domain::Land => self.lethal_land,
            Domain::Sea => self.lethal_sea,
        }
    }
    /// Units with no defense are captured rather than fought.
    pub fn is_defenseless(&self) -> bool {
        self.defense <= 0
    }
    pub fn can_work(&self) -> bool {
        self.work > 0
    }
    pub fn total_moves(&self) -> u32 {
        self.moves * MOVE_UNIT
    }
    /// Only units with more than one full move can withdraw from a fight.
    pub fn is_fast(&self) -> bool {
        self.total_moves() > MOVE_UNIT
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Order {
    #[default]
    None,
    /// Digging in. Becomes [`Order::Fortified`] when the next turn begins.
    Fortifying,
    Fortified,
    /// Building an improvement on the current tile.
    Work(Job),
}
impl Order {
    pub fn is_none(&self) -> bool {
        *self == Self::None
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Unit {
    pub id: Id,
    pub owner: Id,
    /// Key into the pack's unit table.
    pub kind: String,
    pub position: Coord,
    /// Experience: 0 Conscript, 1 Regular, 2 Veteran, 3 Elite.
    pub level: u8,
    /// Hit points lost.
    pub damage: i32,
    /// Movement spent this turn, in thirds of a move.
    pub moves_used: u32,
    #[serde(default, skip_serializing_if = "Order::is_none")]
    pub order: Order,
    /// Work accumulated toward the current [`Order::Work`] job.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub work: u32,
    /// Has attacked or bombarded this turn.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub attacked: bool,
    /// Has already fired its defensive bombard this turn.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fired: bool,
    /// Failed a promotion roll this turn; the next victory promotes without a roll.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub promotion_failed: bool,
    /// Destination the unit keeps marching toward at the start of each turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goto: Option<Coord>,
    /// The squares the standing march will step on, from the unit's own to its goal. Never
    /// kept in the game itself (the march is planned again each turn): [`Game::show_routes`]
    /// fills it in on a player's view so the client can draw the route the unit will take.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub route: Vec<Coord>,
    /// The ship this land unit is aboard. A passenger shares its ship's square, never defends
    /// on its own, and goes wherever the ship goes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub carrier: Option<Id>,
}
fn is_zero(value: &u32) -> bool {
    *value == 0
}
impl Unit {
    pub fn new(id: Id, owner: Id, kind: &str, position: Coord) -> Self {
        Self {
            id,
            owner,
            kind: kind.into(),
            position,
            level: STARTING_LEVEL,
            damage: 0,
            moves_used: 0,
            order: Order::None,
            work: 0,
            attacked: false,
            fired: false,
            promotion_failed: false,
            goto: None,
            route: Vec::new(),
            carrier: None,
        }
    }
    pub fn max_hp(&self, def: &UnitDef) -> i32 {
        (BASE_HP[usize::from(self.level.min(3))] + def.hp).max(1)
    }
    /// Hit points remaining.
    pub fn hp(&self, def: &UnitDef) -> i32 {
        self.max_hp(def) - self.damage
    }
    pub fn moves_left(&self, def: &UnitDef) -> u32 {
        def.total_moves().saturating_sub(self.moves_used)
    }
    pub fn level_name(&self) -> &'static str {
        LEVEL_NAMES[usize::from(self.level.min(3))]
    }
    pub fn retreat_percent(&self) -> i32 {
        RETREAT_PERCENT[usize::from(self.level.min(3))]
    }
    pub fn is_fortified(&self) -> bool {
        self.order == Order::Fortified
    }
    /// Any standing order or march is dropped.
    pub fn clear_orders(&mut self) {
        self.order = Order::None;
        self.work = 0;
        self.goto = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(moves: u32, hp: i32) -> UnitDef {
        UnitDef {
            id: "test".into(),
            name: "Test".into(),
            cost: 10,
            sprite: "test".into(),
            domain: Domain::Land,
            attack: 1,
            defense: 1,
            bombard: 0,
            range: 0,
            rate_of_fire: 0,
            hp,
            moves,
            blitz: false,
            work: 0,
            settler: false,
            capacity: 0,
            lethal_land: false,
            lethal_sea: false,
        }
    }

    #[test]
    fn hit_points_follow_experience_and_design() {
        let d = def(1, 1);
        let mut u = Unit::new(1, 1, "test", Coord::new(0, 0));
        assert_eq!((u.level, u.max_hp(&d)), (STARTING_LEVEL, 4));
        u.level = 3;
        assert_eq!(u.max_hp(&d), 6);
        u.damage = 2;
        assert_eq!(u.hp(&d), 4);
        // a design can never drop below one hit point
        assert_eq!(
            Unit::new(2, 1, "test", Coord::new(0, 0)).max_hp(&def(1, -9)),
            1
        );
    }

    #[test]
    fn movement_is_counted_in_thirds() {
        let d = def(2, 0);
        let mut u = Unit::new(1, 1, "test", Coord::new(0, 0));
        assert_eq!(u.moves_left(&d), 6);
        u.moves_used = 5;
        assert_eq!(u.moves_left(&d), 1);
        u.moves_used = 9;
        assert_eq!(u.moves_left(&d), 0);
        assert!(def(2, 0).is_fast() && !def(1, 0).is_fast());
    }

    #[test]
    fn orders_serialize_compactly() {
        let mut u = Unit::new(1, 1, "test", Coord::new(0, 0));
        let plain = serde_json::to_string(&u).unwrap();
        assert!(
            !plain.contains("order") && !plain.contains("goto"),
            "{plain}"
        );
        u.order = Order::Work(Job::Road);
        let json = serde_json::to_string(&u).unwrap();
        assert!(json.contains(r#""order":{"work":"road"}"#), "{json}");
        assert_eq!(serde_json::from_str::<Unit>(&json).unwrap(), u);
    }
}
