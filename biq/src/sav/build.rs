//! Writing a saved game from nothing: [`Save::blank`] lays out a structurally
//! complete save for a rule set and a map size, and the setters fill the
//! fields that are decoded ([`Tile`], [`Player`], [`Unit`], [`City`]). Every
//! field that is not decoded stays zero (or `-1` where the game keeps ids), so
//! the result reads back through [`Save::parse`] and carries exactly what the
//! accessors name; it is not a save Civ3 itself is known to open.

use super::body::Body;
use super::counts::RuleCounts;
use super::game::{Game, History, HistoryRecord, Replay, game_field};
use super::objects::{
    City, CityTail, Overlays, PLAYER_SLOTS, Player, PlayerTables, Unit, UnitIds, city_field,
    city_name_field, lead_field, unit_field,
};
use super::world::{Map, Tile};
use super::{Header, SUB_VERSION, Save, VERSION};
use crate::Biq;
use crate::dcl::Storage;
use crate::io::Result;

/// Slots in a unit's carried-unit list.
const UNIT_SLOTS: u32 = 10;

impl Tile {
    /// An empty cell: no river, owner, resource, city, colony or continent.
    pub fn blank() -> Tile {
        let mut t = Tile::default();
        t.cell_04.set_i32(0x04, -1);
        t.cell_04.set_i16(0x16, -1);
        t.cell_04.set_i16(0x18, -1);
        // Depth byte: the loader stores 6 for every Conquests cell.
        t.cell_04.set_u8(0x1C, 6);
        t.cell_04.set_i16(0x1E, -1);
        t.cell_58.set_i16(0x14, -1);
        t
    }

    /// Set the river connection mask.
    pub fn set_river_connection_mask(&mut self, mask: u8) {
        self.cell_04.set_u8(0x00, mask);
    }

    /// Set the owning player slot (0 = nobody).
    pub fn set_owner(&mut self, slot: u8) {
        self.cell_04.set_u8(0x01, slot);
        self.cell_58.set_u32(0, if slot == 0 { 0 } else { 1 << slot });
    }

    /// Set the `GOOD` row of the resource (`-1` for none).
    pub fn set_resource(&mut self, row: i32) {
        self.cell_04.set_i32(0x04, row);
    }

    /// Set the id of the city on the tile (`-1` for none).
    pub fn set_city_id(&mut self, id: i16) {
        self.cell_04.set_i16(0x16, id);
    }

    /// Set the continent id.
    pub fn set_continent_id(&mut self, id: i16) {
        self.cell_04.set_i16(0x1A, id);
    }

    /// Set the terrain: its `TERR` row and the secondary class nibble.
    pub fn set_terrain(&mut self, id: u8, sub: u8) {
        let word = (u32::from(id & 0xF) << 12) | (u32::from(sub & 0xF) << 8);
        self.cell_28.set_u32(4, word);
    }

    /// Set the overlay plane.
    pub fn set_overlay_plane(&mut self, bits: u32) {
        self.cell_28.set_u32(0, bits);
    }

    /// Set the feature plane.
    pub fn set_feature_plane(&mut self, bits: u32) {
        self.cell_28.set_u32(8, bits);
    }
}

impl Map {
    /// A map of `width x height` blank cells with one continent.
    pub fn blank(width: u32, height: u32, players: u32, counts: &RuleCounts) -> Map {
        let mut header = Body::<164>::default();
        header.set_u32(0x00, 1);
        header.set_u32(0x04, height);
        header.set_u32(0x0C, players);
        header.set_u32(0x18, width);
        let mut continent_count = Body::<2>::default();
        continent_count.set_u16(0, 1);
        let mut cell_58 = Body::<128>::default();
        cell_58.set_i16(0x14, -1);
        let cell = Tile { cell_58, ..Tile::blank() };
        Map {
            continent_count,
            header,
            header_ext: Body::default(),
            tiles: vec![cell; (width / 2) as usize * height as usize],
            continents: vec![Body::default()],
            goods: vec![0; counts.goods],
        }
    }

    /// The cell at `(x, y)` for writing; `None` outside the map.
    pub fn tile_mut(&mut self, x: u32, y: u32) -> Option<&mut Tile> {
        let (w, h) = (self.width(), self.height());
        if x >= w || y >= h {
            return None;
        }
        self.tiles.get_mut((w / 2 * y + x / 2) as usize)
    }
}

impl Player {
    /// A slot nobody plays: race `-1`, no tables.
    pub fn unused() -> Player {
        let mut lead = Body::<5532>::default();
        lead.set_i32(lead_field::RACE, -1);
        lead.set_i32(lead_field::CAPITAL, -1);
        lead.set_u32(lead_field::VERSION, 4);
        Player {
            lead,
            lists: vec![Vec::new(); 32],
            tables: None,
            arrays: Default::default(),
            culture: Body::default(),
            espionage: Default::default(),
            ring: Vec::new(),
            list_a: Some(vec![Vec::new(); 32]),
            list_b: Some(vec![Vec::new(); 32]),
            tail: vec![0; 9],
        }
    }

    /// A slot in use: its civilization, government and gold, with zeroed
    /// tables sized for `counts`.
    pub fn new(race: i32, government: i32, gold: i32, counts: &RuleCounts) -> Player {
        let mut p = Player::unused();
        p.lead.set_i32(lead_field::RACE, race);
        p.lead.set_i32(lead_field::GOVERNMENT, government);
        p.lead.set_i32(lead_field::TREASURY_A, gold);
        p.lead.set_u8(lead_field::IN_USE, 1);
        let b = counts.buildings;
        let u = counts.unit_types;
        p.tables = Some(PlayerTables {
            building_u16: [vec![0; b], vec![0; b], vec![0; b]],
            building_u32: vec![0; b],
            building_u8: vec![0; b],
            unit_u16: [vec![0; u], vec![0; u], vec![0; u]],
            space_u16: vec![0; counts.space_parts],
            goods_supply: vec![0; counts.goods * 96],
            goods_u8: vec![0; counts.goods],
        });
        p
    }

    /// Set the capital city id (`-1` for none).
    pub fn set_capital_city(&mut self, id: i32) {
        self.lead.set_i32(lead_field::CAPITAL, id);
    }
}

impl Unit {
    /// A unit record with the decoded fields set and the rest zero.
    #[allow(clippy::too_many_arguments)]
    pub fn new(id: u32, x: i32, y: i32, owner: u32, unit_type: u32, experience: u32, damage: u32, order: u32) -> Unit {
        let mut body = Body::<472>::default();
        body.set_u32(unit_field::ID, id);
        body.set_i32(unit_field::X, x);
        body.set_i32(unit_field::Y, y);
        body.set_i32(unit_field::PREV_X, -1);
        body.set_i32(unit_field::PREV_Y, -1);
        body.set_u32(unit_field::OWNER, owner);
        body.set_u32(unit_field::TYPE, unit_type);
        body.set_u32(unit_field::EXPERIENCE, experience);
        body.set_u32(unit_field::DAMAGE, damage);
        body.set_u32(unit_field::ORDER, order);
        body.set_u32(unit_field::VERSION, 2);
        let mut head = Body::<8>::default();
        head.set_u32(0, 1);
        head.set_u32(4, UNIT_SLOTS);
        Unit {
            body,
            ids: Some(UnitIds { head, ids: vec![u32::MAX; UNIT_SLOTS as usize] }),
        }
    }
}

impl City {
    /// A city record with the decoded fields set and the rest zero. `built`
    /// lists the `BLDG` rows in the improvement bit set.
    pub fn new(id: u32, x: u16, y: u16, owner: u8, name: &str, size: u32, built: &[usize], counts: &RuleCounts) -> City {
        let mut block_20 = Body::<136>::default();
        block_20.set_u32(city_field::ID, id);
        block_20.set_u16(city_field::X, x);
        block_20.set_u16(city_field::Y, y);
        block_20.set_u8(city_field::OWNER, owner);
        let mut block_1e0 = Body::<148>::default();
        for (i, b) in name.bytes().take(city_name_field::NAME_LEN - 1).enumerate() {
            block_1e0.set_u8(city_name_field::NAME + i, b);
        }
        let mut popd = Body::<8>::default();
        popd.set_u32(0, 1);
        popd.set_u32(4, size);
        let mut bitm = Body::<40>::default();
        let bits = bitm.0.len() * 8;
        for &row in built.iter().filter(|&&r| r < bits) {
            bitm.0[row / 8] |= 1 << (row % 8);
        }
        let mut version = Body::<8>::default();
        version.set_u32(4, 4);
        City {
            block_20,
            block_cc: Body::default(),
            block_f4: Body::default(),
            block_13c: Body::default(),
            block_1e0,
            popd,
            citizens: vec![Body::default(); size as usize],
            binf: Body::default(),
            buildings: vec![0; counts.buildings * 12],
            bitm,
            date: Some(Body::default()),
            tail: CityTail {
                version,
                list: Some((Body::default(), Vec::new())),
                ctpg: Some((Body::default(), Body::default())),
                last: Some(Body::default()),
            },
        }
    }

    /// The `BLDG` rows in the improvement bit set.
    pub fn improvements(&self) -> Vec<usize> {
        (0..self.bitm.0.len() * 8).filter(|&i| self.bitm.0[i / 8] >> (i % 8) & 1 != 0).collect()
    }
}

impl Save {
    /// An empty game on `biq` (the embedded scenario, as the bytes of a BIQ
    /// file; empty for the shipped Conquests rules) with a map of `width x
    /// height` blank cells, the barbarians in slot 0 and turn `turn`.
    /// Fill it with [`Save::set_player`], [`Save::add_unit`] and
    /// [`Save::add_city`], then call [`Save::finish`].
    pub fn blank(biq: Vec<u8>, width: u32, height: u32, turn: u32, year: i32) -> Result<Save> {
        let counts = if biq.is_empty() { RuleCounts::CONQUESTS } else { RuleCounts::from_biq(&Biq::parse(&biq)?) };
        let mut bic = Body::<524>::default();
        bic.set_u32(0, biq.len() as u32);
        let mut gbody = Body::<848>::default();
        gbody.set_u32(game_field::BLOCK_VERSION, 5);
        gbody.set_u32(game_field::CONTINENTS, 1);
        gbody.set_u32(game_field::TURN, turn);
        let mut date = Body::<84>::default();
        date.set_i32(76, year);
        date.set_u32(80, 1);
        let game = Game {
            body: gbody,
            cities_per_continent: vec![0],
            tech_known_by: vec![0; counts.techs],
            wonder_city: vec![u32::MAX; counts.buildings],
            wonder_built: vec![0; counts.buildings],
            building_mask_a: vec![0; counts.buildings],
            building_mask_b: vec![0; counts.buildings],
            unit_mask_a: vec![0; counts.unit_types],
            unit_mask_b: vec![0; counts.unit_types],
            tech_mask: vec![0; counts.techs],
            date,
            plgi_head: Body::default(),
            plgi_body: Body::default(),
            date_start: Body::default(),
            date_other: Body::default(),
            tail_a: 0,
            tail_b: 2,
        };
        let mut players: Vec<Player> = (0..PLAYER_SLOTS).map(|_| Player::unused()).collect();
        players[0] = Player::new(0, 1, 0, &counts);
        let mut palv = Body::<148>::default();
        palv.0.fill(0xFF);
        Ok(Save {
            storage: Storage::GAME,
            header: Header { marker: 0x1A, version: VERSION, sub_version: SUB_VERSION, guid: Some([0; 16]) },
            counts,
            bic,
            biq,
            game,
            console: Body::default(),
            map: Map::blank(width, height, 0, &counts),
            players,
            units: Vec::new(),
            cities: Vec::new(),
            colonies: Vec::new(),
            reserved: vec![0; 256],
            palv: vec![palv; 32],
            history: History { tag: *b"HIST", x: 0, records: Vec::new() },
            tutorial: Body::default(),
            faxx: Body::default(),
            replay: Replay { tag: u32::from_le_bytes(*b"RPLS"), turns: Vec::new() },
            net_queue: Vec::new(),
            peer: Body::default(),
            overlays: Overlays::default(),
        })
    }

    /// Put a civilization in `slot` (1..=31).
    pub fn set_player(&mut self, slot: usize, race: i32, government: i32, gold: i32) {
        self.players[slot] = Player::new(race, government, gold, &self.counts);
    }

    /// Append a unit (ids ascend in the order added).
    pub fn add_unit(&mut self, unit: Unit) {
        self.units.push(unit);
    }

    /// Append a city and link its tile.
    pub fn add_city(&mut self, city: City) {
        let (x, y, id) = (city.x() as u32, city.y() as u32, city.id());
        if let Some(t) = self.map.tile_mut(x, y) {
            t.set_city_id(id as i16);
        }
        self.cities.push(city);
    }

    /// Bring the counts the stream keeps inside its bodies in line with the
    /// lists, and write the history's last record. Call once, after the last
    /// `add_*`.
    pub fn finish(&mut self) {
        let civs: Vec<usize> = (1..PLAYER_SLOTS).filter(|&k| self.players[k].in_use()).collect();
        let g = &mut self.game.body;
        g.set_u32(game_field::UNITS, self.units.len() as u32);
        g.set_u32(game_field::CITIES, self.cities.len() as u32);
        g.set_u32(game_field::COLONIES, self.colonies.len() as u32);
        self.game.cities_per_continent = vec![self.cities.len() as u32];
        self.map.header.set_u32(0x0C, civs.len() as u32);
        let m = civs.len();
        self.history.x = (1u32 << (m + 1)).wrapping_sub(2);
        self.history.records = vec![HistoryRecord {
            a: self.game.turn(),
            b: self.game.date.i32(76) as u32,
            series: [(1..=m as u32).collect(), vec![0; m], vec![0; m], vec![0; m]],
            fifth: None,
        }];
        self.bic.set_u32(0, self.biq.len() as u32);
    }
}
