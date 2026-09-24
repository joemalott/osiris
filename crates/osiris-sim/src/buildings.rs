//! Buildings: placement, storage and the per-building state shared by all types.

use crate::figures::FigureId;
use crate::map::{Map, terrain};

pub type BuildingId = u32;

/// Building type ids of the original game (they index the model files too).
pub mod kind {
    pub const ROAD: u16 = 5;
    pub const IRRIGATION_DITCH: u16 = 8;
    pub const CLEAR_LAND: u16 = 9;
    pub const HOUSE_FIRST: u16 = 10;
    pub const HOUSE_LAST: u16 = 29;
    pub const VACANT_LOT: u16 = HOUSE_FIRST;
    pub const BANDSTAND: u16 = 30;
    pub const BOOTH: u16 = 31;
    pub const PAVILION: u16 = 33;
    pub const GARDENS: u16 = 39;
    pub const CONSERVATORY: u16 = 34;
    pub const DANCE_SCHOOL: u16 = 35;
    pub const JUGGLER_SCHOOL: u16 = 36;
    pub const APOTHECARY: u16 = 46;
    pub const MORTUARY: u16 = 47;
    pub const SCRIBAL_SCHOOL: u16 = 51;
    pub const LIBRARY: u16 = 53;
    pub const TEMPLE_OSIRIS: u16 = 60;
    pub const TEMPLE_BAST: u16 = 64;
    pub const BAZAAR: u16 = 70;
    pub const GRANARY: u16 = 71;
    pub const STORAGE_YARD: u16 = 72;
    pub const ARCHITECT_POST: u16 = 81;
    pub const VILLAGE_PALACE: u16 = 84;
    pub const TAX_COLLECTOR: u16 = 86;
    pub const WELL: u16 = 92;
    pub const BURNING_RUIN: u16 = 99;
    pub const FARM_FIRST: u16 = 100;
    pub const FARM_LAST: u16 = 105;
    pub const JEWELER: u16 = 113;
    pub const HUNTING_LODGE: u16 = 115;
    pub const SHRINE_OSIRIS: u16 = 140;
    pub const SHRINE_BAST: u16 = 144;
    pub const SHIPWRIGHT: u16 = 74;
    pub const CLAY_PIT: u16 = 109;
    pub const GOLD_MINE: u16 = 161;
    pub const FIREHOUSE: u16 = 167;
    pub const WATER_SUPPLY: u16 = 180;
    pub const WORK_CAMP: u16 = 199;
    pub const CARPENTERS_GUILD: u16 = 177;
    pub const BRICKLAYERS_GUILD: u16 = 178;
    pub const STONEMASONS_GUILD: u16 = 179;
    pub const SMALL_MASTABA: u16 = 258;
    pub const MEDIUM_MASTABA: u16 = 259;
    pub const LARGE_MASTABA: u16 = 260;

    pub fn is_house(k: u16) -> bool {
        (HOUSE_FIRST..=HOUSE_LAST).contains(&k)
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Building {
    pub id: BuildingId,
    pub kind: u16,
    /// A temple complex's altar and oracle (bits 1 and 2).
    #[serde(default)]
    pub upgrades: u8,
    pub x: i32,
    pub y: i32,
    pub size: i32,
    /// Road tile the building's walkers start from, if any.
    pub road: Option<(i32, i32)>,
    pub workers: i32,
    /// Walkers belonging to this building, by slot (service walker, labor seeker, cart).
    pub walkers: [FigureId; 3],
    /// Ticks until the next walker may be sent out.
    pub spawn_delay: i32,
    pub fire_risk: i32,
    pub damage_risk: i32,
    /// Desirability at the building's tile, refreshed with the desirability grid.
    pub desirability: i32,
    /// Houses its walkers have served recently (decays daily).
    pub houses_covered: i32,
    /// Production progress, growth or other per-type counter.
    pub progress: i32,
    /// Stored resources, indexed by resource id.
    pub stock: Vec<i32>,
    pub house: Option<crate::houses::House>,
    /// Animation frame counter.
    pub anim: u32,
    /// Days of peasant labor left on a floodplain farm.
    #[serde(default)]
    pub labor_days: i32,
    /// Direction the next roaming walker sets off in; rotates by a quarter turn per walker.
    #[serde(default)]
    pub roam_dir: u8,
    /// Which way a venue faces over its roads (0..4), or a gatehouse runs (0, 1).
    #[serde(default)]
    pub orientation: u8,
    /// A storage yard's eight spaces: resource and units in each.
    #[serde(default)]
    pub spaces: Vec<(u16, i32)>,
    /// A storage building's order per resource; missing entries take the defaults. For a
    /// bazaar, 1 means it doesn't buy the resource.
    #[serde(default)]
    pub orders: Vec<u8>,
    /// A storage building's amount tiers per resource, for accepting and for getting:
    /// 1-4 quarters of its capacity (missing entries mean all of it).
    #[serde(default)]
    pub order_tiers: Vec<(u8, u8)>,
    /// A storage building told to empty itself, and the orders to restore afterwards.
    #[serde(default)]
    pub empty_all: bool,
    #[serde(default)]
    pub saved_orders: Vec<u8>,
    /// A monument's footprint (width, height) when it isn't square.
    #[serde(default)]
    pub dims: Option<(i32, i32)>,
    /// A monument's construction.
    #[serde(default)]
    pub monument: Option<crate::monuments::Monument>,
    /// Damage invaders have done it.
    #[serde(default)]
    pub enemy_damage: i32,
    /// The ship a shipwright is building: warship or transport (0 for none, or a
    /// fishing boat).
    #[serde(default)]
    pub boat_kind: u16,
    /// A venue's days of shows left, per performer (juggler, musician, dancer).
    #[serde(default)]
    pub shows: [i32; 3],
    /// A venue's performers out roaming, per performer.
    #[serde(default)]
    pub performers: [FigureId; 3],
    /// A water lift's water: 0 none, 2 reached but not yet pumped, 1 pumping.
    #[serde(default)]
    pub water: u8,
    /// A bazaar's demand for pottery, luxury goods, linen and beer: raised when houses
    /// nearby want the good, run down by each buyer decision.
    #[serde(default)]
    pub goods_demand: [i32; 4],
    /// The image drawn for this building (global id).
    pub image: u32,
}

impl Building {
    /// Width and height of the footprint.
    pub fn footprint(&self) -> (i32, i32) {
        self.dims.unwrap_or((self.size, self.size))
    }

    pub fn tiles(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        let (w, h) = self.footprint();
        (0..h).flat_map(move |dy| (0..w).map(move |dx| (self.x + dx, self.y + dy)))
    }

    pub fn center(&self) -> (i32, i32) {
        (self.x + self.size / 2, self.y + self.size / 2)
    }

    pub fn is_house(&self) -> bool {
        kind::is_house(self.kind)
    }
}

/// Storage for all buildings, with stable ids (0 is never used).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Buildings {
    slots: Vec<Option<Building>>,
    free: Vec<BuildingId>,
}

impl Buildings {
    pub fn insert(&mut self, mut b: Building) -> BuildingId {
        let id = self.free.pop().unwrap_or_else(|| {
            self.slots.push(None);
            self.slots.len() as BuildingId
        });
        b.id = id;
        self.slots[id as usize - 1] = Some(b);
        id
    }

    pub fn get(&self, id: BuildingId) -> Option<&Building> {
        self.slots.get((id as usize).wrapping_sub(1))?.as_ref()
    }

    pub fn get_mut(&mut self, id: BuildingId) -> Option<&mut Building> {
        self.slots.get_mut((id as usize).wrapping_sub(1))?.as_mut()
    }

    pub fn remove(&mut self, id: BuildingId) -> Option<Building> {
        let b = self.slots.get_mut((id as usize).wrapping_sub(1))?.take();
        if b.is_some() {
            self.free.push(id);
        }
        b
    }

    pub fn iter(&self) -> impl Iterator<Item = &Building> {
        self.slots.iter().flatten()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Building> {
        self.slots.iter_mut().flatten()
    }

    pub fn ids(&self) -> Vec<BuildingId> {
        self.iter().map(|b| b.id).collect()
    }

    pub fn count_of(&self, k: u16) -> usize {
        self.iter().filter(|b| b.kind == k).count()
    }
}

/// A road tile orthogonally adjacent to the `size x size` footprint at `(x, y)`,
/// scanning the perimeter the way the original does (top edge first).
/// A road tile beside a `w` x `h` footprint, going round it from the top.
pub fn road_access_rect(map: &Map, x: i32, y: i32, (w, h): (i32, i32)) -> Option<(i32, i32)> {
    let top = (0..w).map(|i| (x + i, y - 1));
    let right = (0..h).map(|i| (x + w, y + i));
    let bottom = (0..w).map(|i| (x + w - 1 - i, y + h));
    let left = (0..h).map(|i| (x - 1, y + h - 1 - i));
    top.chain(right).chain(bottom).chain(left).find(|&(rx, ry)| map.terrain_is(rx, ry, terrain::ROAD) && !map.terrain_is(rx, ry, terrain::WATER | terrain::BUILDING))
}

pub fn road_access(map: &Map, x: i32, y: i32, size: i32) -> Option<(i32, i32)> {
    let mut candidates = Vec::with_capacity((4 * size) as usize);
    for i in 0..size {
        candidates.push((x + i, y - 1));
    }
    for i in 0..size {
        candidates.push((x + size, y + i));
    }
    for i in 0..size {
        candidates.push((x + size - 1 - i, y + size));
    }
    for i in 0..size {
        candidates.push((x - 1, y + size - 1 - i));
    }
    candidates.into_iter().find(|&(rx, ry)| {
        map.terrain_is(rx, ry, terrain::ROAD) && !map.terrain_is(rx, ry, terrain::WATER | terrain::BUILDING)
    })
}

/// A road tile within `radius` of the footprint (houses reach roads two tiles away).
pub fn road_within(map: &Map, x: i32, y: i32, size: i32, radius: i32) -> Option<(i32, i32)> {
    for r in 1..=radius {
        for yy in (y - r)..(y + size + r) {
            for xx in (x - r)..(x + size + r) {
                let on_ring = xx == x - r || yy == y - r || xx == x + size + r - 1 || yy == y + size + r - 1;
                if on_ring && map.terrain_is(xx, yy, terrain::ROAD) && !map.terrain_is(xx, yy, terrain::WATER) {
                    return Some((xx, yy));
                }
            }
        }
    }
    None
}

/// Neighbouring tiles of a footprint (orthogonal ring), for adjacency checks.
pub fn ring(x: i32, y: i32, size: i32) -> impl Iterator<Item = (i32, i32)> {
    (-1..=size).flat_map(move |dy| {
        (-1..=size).filter_map(move |dx| {
            let inside = dx >= 0 && dy >= 0 && dx < size && dy < size;
            (!inside).then_some((x + dx, y + dy))
        })
    })
}
