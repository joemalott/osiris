//! Storage buildings: storage yards and granaries, their per-resource orders, and the
//! storage yard's eight spaces.
//!
//! A storage yard is a hut in its north corner and eight spaces around it. Each space
//! holds up to 400 units of one resource, so a yard holds 3200 in all. A granary keeps
//! one pool of up to 3200 units of at most four foods.

use crate::buildings::{BuildingId, kind};
use crate::economy::resource;
use crate::world::World;

/// Units one storage-yard space holds.
pub const SPACE_UNITS: i32 = 400;
/// Units a storage building holds in all.
pub const CAPACITY: i32 = 3200;
/// Foods a granary keeps at once.
const GRANARY_FOOD_TYPES: usize = 4;

/// Where the hut and the eight spaces sit in a yard's 3x3 footprint; the hut takes the first.
pub const YARD_TILES: [(i32, i32); 9] = [(0, 0), (0, 1), (1, 0), (1, 1), (0, 2), (2, 0), (1, 2), (2, 1), (2, 2)];

/// What a storage building does with one resource.
pub mod order {
    /// Takes deliveries.
    pub const ACCEPT: u8 = 0;
    /// Takes nothing.
    pub const REFUSE: u8 = 1;
    /// Sends carts out to fetch it.
    pub const GET: u8 = 2;
    /// Sends it all away to other storage.
    pub const EMPTY: u8 = 3;
}

/// Whether `k` is a storage yard or granary.
pub fn is_storage(k: u16) -> bool {
    matches!(k, kind::STORAGE_YARD | kind::GRANARY)
}

impl crate::buildings::Building {
    /// This storage building's order for resource `r`. New yards refuse food and take
    /// everything else; granaries the reverse.
    pub fn order(&self, r: u16) -> u8 {
        if let Some(&o) = self.orders.get(r as usize) {
            return o;
        }
        let food = resource::is_food(r);
        match self.kind {
            kind::GRANARY if food => order::ACCEPT,
            kind::STORAGE_YARD if !food => order::ACCEPT,
            _ => order::REFUSE,
        }
    }

    /// This storage building's amount tier for `r` (1-4 quarters of its capacity), for
    /// accepting or for getting.
    pub fn order_tier(&self, r: u16, get: bool) -> u8 {
        self.order_tiers.get(r as usize).map_or(4, |t| if get { t.1 } else { t.0 })
    }

    /// How much of `r` it will hold under its orders.
    pub fn order_cap(&self, r: u16) -> i32 {
        match self.order(r) {
            order::ACCEPT => self.order_tier(r, false) as i32 * CAPACITY / 4,
            order::GET => self.order_tier(r, true) as i32 * CAPACITY / 4,
            _ => 0,
        }
    }

    /// Steps the current order's amount tier up or down a quarter.
    pub fn change_order_tier(&mut self, r: u16, up: bool) {
        let get = self.order(r) == order::GET;
        if self.order_tiers.len() <= r as usize {
            self.order_tiers.resize(r as usize + 1, (4, 4));
        }
        let t = &mut self.order_tiers[r as usize];
        let v = if get { &mut t.1 } else { &mut t.0 };
        *v = if up { (*v + 1).min(4) } else { (*v - 1).max(1) };
    }

    /// Clicking a resource's order: accept, get, empty, refuse, and round again.
    pub fn cycle_order(&mut self, r: u16) {
        let next = match self.order(r) {
            order::ACCEPT => order::GET,
            order::GET => order::EMPTY,
            order::EMPTY => order::REFUSE,
            _ => order::ACCEPT,
        };
        self.set_order(r, next);
        self.empty_all = false;
    }

    /// Empties the whole building, or stops and restores the orders it had.
    pub fn toggle_empty_all(&mut self) {
        if self.empty_all {
            self.orders = std::mem::take(&mut self.saved_orders);
            self.empty_all = false;
        } else {
            let all: Vec<u8> = (0..crate::economy::resource::COUNT as u16).map(|r| self.order(r)).collect();
            self.saved_orders = all;
            self.orders = vec![order::EMPTY; crate::economy::resource::COUNT];
            self.empty_all = true;
        }
    }

    /// Refuses everything.
    pub fn accept_none(&mut self) {
        self.orders = vec![order::REFUSE; crate::economy::resource::COUNT];
        self.empty_all = false;
        self.saved_orders.clear();
    }

    /// Whether a bazaar buys `r`.
    pub fn bazaar_buys(&self, r: u16) -> bool {
        self.orders.get(r as usize).copied().unwrap_or(0) == 0
    }

    pub fn toggle_bazaar_buys(&mut self, r: u16) {
        if self.orders.len() <= r as usize {
            self.orders.resize(r as usize + 1, 0);
        }
        self.orders[r as usize] ^= 1;
    }

    pub fn set_order(&mut self, r: u16, o: u8) {
        if self.orders.len() <= r as usize {
            let defaults: Vec<u8> = (self.orders.len()..=r as usize).map(|i| self.order(i as u16)).collect();
            self.orders.extend(defaults);
        }
        self.orders[r as usize] = o;
    }
}

impl World {
    /// Units of `r` held by building `id`.
    pub fn stored(&self, id: BuildingId, r: u16) -> i32 {
        let Some(b) = self.buildings.get(id) else { return 0 };
        if b.kind == kind::STORAGE_YARD {
            b.spaces.iter().filter(|s| s.0 == r).map(|s| s.1).sum()
        } else {
            b.stock.get(r as usize).copied().unwrap_or(0)
        }
    }

    /// Units of everything held by building `id`.
    pub fn total_stored(&self, id: BuildingId) -> i32 {
        let Some(b) = self.buildings.get(id) else { return 0 };
        if b.kind == kind::STORAGE_YARD { b.spaces.iter().map(|s| s.1).sum() } else { b.stock.iter().sum() }
    }

    /// How much more of `r` a storage building would take right now, following its
    /// orders, staff and room.
    pub fn storage_room(&self, id: BuildingId, r: u16) -> i32 {
        let Some(b) = self.buildings.get(id) else { return 0 };
        if b.road.is_none() || !matches!(b.order(r), order::ACCEPT | order::GET) {
            return 0;
        }
        let under_cap = (b.order_cap(r) - self.stored(id, r)).max(0);
        let room = match b.kind {
            kind::STORAGE_YARD => {
                if b.workers <= 0 {
                    return 0;
                }
                b.spaces.iter().map(|&(sr, n)| if sr == r || n == 0 { SPACE_UNITS - n } else { 0 }).sum()
            }
            kind::GRANARY => {
                // Granaries take deliveries from three-quarters staff.
                let needed = self.workers_needed(b.kind).max(1);
                if !resource::is_food(r) || b.workers * 4 < needed * 3 {
                    return 0;
                }
                let types = b.stock.iter().enumerate().filter(|&(i, &v)| v > 0 && i != r as usize).count();
                if b.stock[r as usize] <= 0 && types >= GRANARY_FOOD_TYPES {
                    return 0;
                }
                (CAPACITY - self.total_stored(id)).max(0)
            }
            _ => 0,
        };
        room.min(under_cap)
    }

    /// Puts up to `amount` of `r` into building `id` and returns how much went in.
    /// Yards fill spaces already holding `r` before starting empty ones.
    pub fn add_stored(&mut self, id: BuildingId, r: u16, amount: i32) -> i32 {
        let yard = self.buildings.get(id).is_some_and(|b| b.kind == kind::STORAGE_YARD);
        let Some(b) = self.buildings.get_mut(id) else { return 0 };
        if !yard {
            if b.stock.len() <= r as usize {
                b.stock.resize(r as usize + 1, 0);
            }
            b.stock[r as usize] += amount;
            return amount;
        }
        let mut left = amount;
        for pass in 0..2 {
            for s in b.spaces.iter_mut() {
                let usable = if pass == 0 { s.0 == r && s.1 > 0 } else { s.1 == 0 };
                if left > 0 && usable {
                    let n = left.min(SPACE_UNITS - s.1);
                    *s = (r, s.1 + n);
                    left -= n;
                }
            }
        }
        self.refresh_yard_images(id);
        amount - left
    }

    /// Takes up to `amount` of `r` out of building `id` and returns how much came out.
    pub fn take_stored(&mut self, id: BuildingId, r: u16, amount: i32) -> i32 {
        let yard = self.buildings.get(id).is_some_and(|b| b.kind == kind::STORAGE_YARD);
        let Some(b) = self.buildings.get_mut(id) else { return 0 };
        if !yard {
            let n = b.stock.get(r as usize).copied().unwrap_or(0).min(amount).max(0);
            if n > 0 {
                b.stock[r as usize] -= n;
            }
            return n;
        }
        let mut left = amount;
        for s in b.spaces.iter_mut() {
            if left > 0 && s.0 == r && s.1 > 0 {
                let n = left.min(s.1);
                s.1 -= n;
                left -= n;
                if s.1 == 0 {
                    s.0 = 0;
                }
            }
        }
        self.refresh_yard_images(id);
        amount - left
    }

    /// Lays out a new storage yard: the hut and eight empty spaces.
    pub(crate) fn place_storage_yard(&mut self, id: BuildingId) {
        let Some(b) = self.buildings.get_mut(id) else { return };
        b.spaces = vec![(0, 0); 8];
        let (x, y, image) = (b.x, b.y, b.image);
        let (dx, dy) = YARD_TILES[0];
        self.map.set_single_image(x + dx, y + dy, image);
        self.refresh_yard_images(id);
    }

    /// Each space shows its resource, in four steps of fullness.
    pub(crate) fn refresh_yard_images(&mut self, id: BuildingId) {
        let Some(b) = self.buildings.get(id) else { return };
        let Some(def) = self.defs.building(kind::STORAGE_YARD) else { return };
        let (Some(empty), Some(filled)) = (def.anims.get("space_empty"), def.anims.get("space_filled")) else { return };
        let (x, y) = (b.x, b.y);
        let images: Vec<u32> = b
            .spaces
            .iter()
            .map(|&(r, n)| if n <= 0 { empty.image } else { filled.image + 4 * (r as u32 - 1) + ((n + 99) / 100 - 1) as u32 })
            .collect();
        for (i, image) in images.into_iter().enumerate() {
            let (dx, dy) = YARD_TILES[i + 1];
            self.map.set_single_image(x + dx, y + dy, image);
        }
    }
}
