//! Goods: production, storage and cart pushers.
//!
//! Industries gain one point of progress per worker per day; a raw-material producer
//! completes a load at 200 points and a workshop at 400. A completed load (100 units)
//! is carted to the nearest storage that takes it: food to granaries, gold to the
//! palace, everything else to storage yards.

use crate::buildings::{BuildingId, kind};
use crate::figures::{Step, Travel};
use crate::world::World;

pub mod resource {
    pub const GRAIN: u16 = 1;
    pub const MEAT: u16 = 2;
    pub const LETTUCE: u16 = 3;
    pub const CHICKPEAS: u16 = 4;
    pub const POMEGRANATES: u16 = 5;
    pub const FIGS: u16 = 6;
    pub const FISH: u16 = 7;
    pub const GAMEMEAT: u16 = 8;
    pub const STRAW: u16 = 9;
    pub const GOLD: u16 = 21;
    pub const COUNT: usize = 40;

    pub fn is_food(r: u16) -> bool {
        (GRAIN..=GAMEMEAT).contains(&r)
    }

    /// The house food slot a food fills: grain, meat, fish, fruit and vegetables.
    pub fn food_slot(r: u16) -> Option<usize> {
        match r {
            GRAIN => Some(0),
            MEAT | GAMEMEAT => Some(1),
            FISH => Some(2),
            LETTUCE | CHICKPEAS | POMEGRANATES | FIGS => Some(3),
            _ => None,
        }
    }
}

pub const LOAD: i32 = 100;
pub const GRANARY_CAPACITY: i32 = 3200;
pub const YARD_CAPACITY: i32 = 3200;
const RAW_MAX_PROGRESS: i32 = 200;
const WORKSHOP_MAX_PROGRESS: i32 = 400;
pub const CART_PUSHER: u16 = 4;

mod action {
    pub const DELIVERING: u16 = 1;
    pub const RETURNING: u16 = 2;
    pub const WAITING: u16 = 3;
}

impl World {
    pub fn resource_id(&self, key: &str) -> Option<u16> {
        self.defs.resources.iter().position(|k| k == key).map(|i| i as u16)
    }

    pub fn stored(&self, id: BuildingId, r: u16) -> i32 {
        self.buildings.get(id).and_then(|b| b.stock.get(r as usize).copied()).unwrap_or(0)
    }

    fn total_stored(&self, id: BuildingId) -> i32 {
        self.buildings.get(id).map_or(0, |b| b.stock.iter().sum())
    }

    /// Whether building `id` would accept `amount` of `r` right now.
    fn accepts(&self, id: BuildingId, r: u16, amount: i32) -> bool {
        let Some(b) = self.buildings.get(id) else { return false };
        if b.workers <= 0 || b.road.is_none() {
            return false;
        }
        match b.kind {
            kind::GRANARY => {
                if !resource::is_food(r) {
                    return false;
                }
                let types = b.stock.iter().enumerate().filter(|&(i, &v)| v > 0 && i != r as usize).count();
                self.total_stored(id) + amount <= GRANARY_CAPACITY && (b.stock[r as usize] > 0 || types < 4)
            }
            kind::STORAGE_YARD => !resource::is_food(r) && self.total_stored(id) + amount <= YARD_CAPACITY,
            kind::VILLAGE_PALACE => r == resource::GOLD,
            _ => false,
        }
    }

    /// The nearest building that takes `amount` of `r`, measured from `from`.
    pub fn find_storage(&self, r: u16, amount: i32, from: (i32, i32)) -> Option<BuildingId> {
        self.buildings
            .iter()
            .filter(|b| self.accepts(b.id, r, amount))
            .min_by_key(|b| (b.x - from.0).abs() + (b.y - from.1).abs())
            .map(|b| b.id)
    }

    /// Tick 20: industries make progress.
    pub(crate) fn update_production(&mut self) {
        for id in self.buildings.ids() {
            let Some(b) = self.buildings.get(id) else { continue };
            let Some(def) = self.defs.building(b.kind) else { continue };
            if def.outputs.is_empty() || def.has_flag("is_farm") || b.kind == kind::HUNTING_LODGE || b.workers <= 0 {
                continue;
            }
            let Some(out) = self.resource_id(&def.outputs[0]) else { continue };
            let inputs: Vec<u16> = def.inputs.iter().filter_map(|k| self.resource_id(k)).collect();
            let max = if inputs.is_empty() { RAW_MAX_PROGRESS } else { WORKSHOP_MAX_PROGRESS };
            let Some(b) = self.buildings.get_mut(id) else { continue };
            if b.progress == 0 {
                // A workshop needs a load of each input before it can start.
                if inputs.iter().all(|&r| b.stock[r as usize] >= LOAD) {
                    for &r in &inputs {
                        b.stock[r as usize] -= LOAD;
                    }
                    b.progress = 1;
                } else {
                    continue;
                }
            }
            b.progress = (b.progress + b.workers).min(max);
            if b.progress >= max && b.stock[out as usize] < LOAD {
                b.stock[out as usize] += LOAD;
                b.progress = 0;
            }
        }
    }

    /// Tick 31 (with walkers): full loads leave by cart.
    pub(crate) fn send_carts(&mut self) {
        for id in self.buildings.ids() {
            let Some(b) = self.buildings.get(id) else { continue };
            if b.walkers[2] != 0 || b.road.is_none() || matches!(b.kind, kind::GRANARY | kind::STORAGE_YARD) {
                continue;
            }
            let Some(def) = self.defs.building(b.kind) else { continue };
            let Some(r) = def.outputs.first().and_then(|k| self.resource_id(k)) else { continue };
            let amount = b.stock[r as usize];
            if amount < LOAD && !(def.has_flag("is_farm") && amount > 0) {
                continue;
            }
            self.spawn_cart(id, r, amount.min(LOAD * 8));
        }
    }

    pub fn spawn_cart(&mut self, home: BuildingId, r: u16, amount: i32) {
        let Some(b) = self.buildings.get(home) else { return };
        let Some(road) = b.road else { return };
        let fid = self.figures.spawn(CART_PUSHER, road.0, road.1, Travel::Roads);
        if let Some(f) = self.figures.get_mut(fid) {
            f.home = home;
            f.cargo = r;
            f.amount = amount;
            f.action = action::WAITING;
        }
        if let Some(b) = self.buildings.get_mut(home) {
            b.stock[r as usize] -= amount;
            b.walkers[2] = fid;
        }
    }

    pub(crate) fn update_cart(&mut self, fid: u32) {
        let Some(f) = self.figures.get(fid) else { return };
        let (act, r, amount, pos, home) = (f.action, f.cargo, f.amount, (f.x, f.y), f.home);
        match act {
            action::WAITING => {
                // Look for somewhere to take the goods about once a day.
                if f.counter > 0 {
                    if let Some(f) = self.figures.get_mut(fid) {
                        f.counter -= 1;
                    }
                    return;
                }
                let dest = self.find_storage(r, LOAD.min(amount), pos);
                let target = dest.and_then(|d| self.buildings.get(d)).and_then(|b| b.road.map(|rd| (b.id, rd)));
                let map = &self.map;
                let Some(f) = self.figures.get_mut(fid) else { return };
                match target {
                    Some((d, rd)) if f.go_to(map, rd) => {
                        f.target = d;
                        f.action = action::DELIVERING;
                    }
                    _ => f.counter = 50,
                }
            }
            action::DELIVERING => {
                let map = &self.map;
                let Some(f) = self.figures.get_mut(fid) else { return };
                match f.walk(map) {
                    Step::Moving => {}
                    Step::Arrived => {
                        let target = f.target;
                        // Unload as much as the storage still takes, one load at a time.
                        let mut left = amount;
                        while left > 0 && self.accepts(target, r, LOAD.min(left)) {
                            let n = LOAD.min(left);
                            if let Some(b) = self.buildings.get_mut(target) {
                                b.stock[r as usize] += n;
                            }
                            left -= n;
                            if r == resource::GOLD {
                                self.gold_delivered += n;
                            }
                        }
                        let home_road = self.buildings.get(home).and_then(|b| b.road);
                        let map = &self.map;
                        let Some(f) = self.figures.get_mut(fid) else { return };
                        f.amount = left;
                        if left > 0 {
                            f.action = action::WAITING;
                        } else {
                            f.action = action::RETURNING;
                            match home_road {
                                Some(hr) if f.go_to(map, hr) => {}
                                _ => f.dead = true,
                            }
                        }
                    }
                    Step::Blocked | Step::Lost => {
                        f.action = action::WAITING;
                        f.route.clear();
                    }
                }
            }
            _ => {
                let map = &self.map;
                if let Some(f) = self.figures.get_mut(fid)
                    && f.walk(map) != Step::Moving
                {
                    f.dead = true;
                }
            }
        }
    }
}
