//! Goods: production, cart pushers, storage-yard carts and the wood and reed gatherers.
//!
//! Industries gain progress each day at tick 20 (by default one point per worker); a
//! raw-material producer finishes a batch at 200 points and a workshop at 400, and the
//! batch is stored at the next day's start. Workshops need 100 units of each input to
//! begin a batch. Once a building holds a batch, a cart pusher takes it (tick 31): gold
//! to the palace, food to a granary, raw materials to a workshop that needs them, and
//! everything else to a storage yard. A cart with nowhere to go waits at home, and the
//! building sends no other until it has gone.

use crate::buildings::{BuildingId, kind};
use crate::figures::{FigureId, Step, Travel};
use crate::storage::{self, order};
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
    pub const POTTERY: u16 = 13;
    pub const BEER: u16 = 15;
    pub const LINEN: u16 = 17;
    pub const LUXURY_GOODS: u16 = 19;
    pub const TIMBER: u16 = 20;
    pub const GOLD: u16 = 21;
    pub const REEDS: u16 = 22;
    pub const COUNT: usize = 40;

    /// The goods houses use, in the order of a house's goods slots.
    pub const HOUSE_GOODS: [u16; 4] = [POTTERY, LUXURY_GOODS, LINEN, BEER];

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
pub const GRANARY_CAPACITY: i32 = storage::CAPACITY;
/// What an industry keeps on site of each input and of its output.
const SITE_CAP: i32 = 200;
const RAW_MAX_PROGRESS: i32 = 200;
const WORKSHOP_MAX_PROGRESS: i32 = 400;
/// The difficulty used for per-difficulty tables (Normal).
const DIFFICULTY: usize = 2;
/// Ticks a cart with nowhere to go waits before looking again.
const CART_RETRY_TICKS: i32 = 30;
/// Most a storage-yard cart fetches at once, and most it delivers.
const YARD_FETCH: i32 = 200;
const YARD_DELIVER: i32 = 400;

pub const CART_PUSHER: u16 = 4;
pub const STORAGEYARD_CART: u16 = 9;
pub const LUMBERJACK: u16 = 75;
pub const REED_GATHERER: u16 = 90;

const WOOD_CUTTERS: u16 = 108;
const REED_GATHERERS: u16 = 195;
const FISHING_WHARF: u16 = 76;
const PALACES: [u16; 3] = [kind::VILLAGE_PALACE, 85, 189];
/// Ticks a gatherer spends cutting once it reaches its tree or reeds.
const GATHER_TICKS: i32 = 300;
/// What a gatherer brings back per trip.
const GATHER_AMOUNT: i32 = 25;

mod action {
    /// Loaded and walking to its destination.
    pub const DELIVERING: u16 = 1;
    /// Empty and walking home.
    pub const RETURNING: u16 = 2;
    /// Loaded, at home, with nowhere to go yet.
    pub const WAITING: u16 = 3;
    /// A storage-yard cart walking empty to fetch goods.
    pub const FETCHING: u16 = 4;
    /// A storage-yard cart bringing fetched goods home.
    pub const BRINGING_HOME: u16 = 5;
    /// A gatherer walking to its tree or reeds.
    pub const TO_HARVEST: u16 = 6;
    /// A gatherer cutting.
    pub const HARVESTING: u16 = 7;
}

/// Quarries: stone, limestone, granite and sandstone.
fn is_quarry(k: u16) -> bool {
    matches!(k, 106 | 107 | 216 | 221)
}

impl World {
    pub fn resource_id(&self, key: &str) -> Option<u16> {
        self.defs.resources.iter().position(|k| k == key).map(|i| i as u16)
    }

    fn distance(&self, id: BuildingId, from: (i32, i32)) -> i32 {
        self.buildings.get(id).map_or(i32::MAX, |b| (b.x - from.0).abs().max((b.y - from.1).abs()))
    }

    /// The units in one finished batch of building `k`'s output.
    pub fn batch_size(&self, k: u16) -> i32 {
        let Some(def) = self.defs.building(k) else { return LOAD };
        if let Some(toml::Value::Array(table)) = def.extra.get("production_rate_dcy")
            && let Some(v) = table.get(DIFFICULTY).and_then(|v| v.as_integer())
        {
            return v as i32;
        }
        def.int("production_rate").map_or(LOAD, |v| v as i32)
    }

    /// Whether building `k` produces through the daily progress of an industry (farms,
    /// the hunting lodge, gatherers and the fishing wharf have their own rules).
    fn is_industry(&self, k: u16) -> bool {
        let Some(def) = self.defs.building(k) else { return false };
        !def.outputs.is_empty()
            && !def.has_flag("is_farm")
            && !matches!(k, kind::HUNTING_LODGE | WOOD_CUTTERS | REED_GATHERERS | FISHING_WHARF)
    }

    /// Progress a batch takes: 400 in a workshop, 200 in a raw-material producer.
    pub fn max_progress(&self, k: u16) -> i32 {
        if self.defs.building(k).is_some_and(|d| d.has_flag("is_workshop")) { WORKSHOP_MAX_PROGRESS } else { RAW_MAX_PROGRESS }
    }

    fn output_of(&self, k: u16) -> Option<u16> {
        self.defs.building(k)?.outputs.first().and_then(|o| self.resource_id(o))
    }

    fn inputs_of(&self, k: u16) -> Vec<u16> {
        self.defs.building(k).map_or_else(Vec::new, |d| d.inputs.iter().filter_map(|i| self.resource_id(i)).collect())
    }

    /// A day's progress with `workers` staff: quarries and mines dig slower than
    /// workshops work.
    fn daily_progress(k: u16, workers: i32) -> i32 {
        if workers <= 0 {
            return 0;
        }
        match k {
            k if is_quarry(k) => (workers / 2).max(1),
            161 => (workers / 10).max(1),
            217 => (workers / 2).max(1),
            162 => (workers / 3).max(1),
            _ => workers,
        }
    }

    /// Tick 20: industries make progress, starting a new batch when they can.
    pub(crate) fn update_production(&mut self) {
        for id in self.buildings.ids() {
            let Some(b) = self.buildings.get(id) else { continue };
            let k = b.kind;
            if !self.is_industry(k) || b.workers <= 0 || self.output_of(k).is_some_and(|r| self.is_mothballed(r)) {
                continue;
            }
            let inputs = self.inputs_of(k);
            let max = self.max_progress(k);
            let gain = Self::daily_progress(k, b.workers);
            let b = self.buildings.get_mut(id).expect("present");
            if b.progress == 0 {
                // A batch starts with a load of each input.
                if !inputs.iter().all(|&r| b.stock[r as usize] >= LOAD) {
                    continue;
                }
                for &r in &inputs {
                    b.stock[r as usize] -= LOAD;
                }
                b.progress = 1;
            }
            b.progress = (b.progress + gain).min(max);
        }
    }

    /// The start of each day: finished batches go into the building's stock, if it
    /// has room for them.
    pub(crate) fn finish_production(&mut self) {
        for id in self.buildings.ids() {
            let Some(b) = self.buildings.get(id) else { continue };
            let k = b.kind;
            if !self.is_industry(k) {
                continue;
            }
            let max = self.max_progress(k);
            let (Some(out), batch) = (self.output_of(k), self.batch_size(k)) else { continue };
            let b = self.buildings.get_mut(id).expect("present");
            if b.progress >= max && b.stock[out as usize] < SITE_CAP {
                b.stock[out as usize] += batch;
                b.progress = 0;
            }
        }
    }

    /// Tick 31 (with walkers): full batches leave by cart, and storage yards send carts
    /// on their errands.
    pub(crate) fn send_carts(&mut self) {
        for id in self.buildings.ids() {
            let Some(b) = self.buildings.get(id) else { continue };
            if b.walkers[2] != 0 || b.road.is_none() {
                continue;
            }
            if b.kind == kind::STORAGE_YARD {
                self.yard_errand(id);
                continue;
            }
            if b.kind == kind::GRANARY {
                continue;
            }
            let k = b.kind;
            let Some(r) = self.output_of(k) else { continue };
            let amount = b.stock[r as usize];
            let farm = self.defs.building(k).is_some_and(|d| d.has_flag("is_farm"));
            let batch = if self.is_industry(k) { self.batch_size(k) } else { LOAD };
            if amount <= 0 || (!farm && amount < batch) {
                continue;
            }
            // Farms send their whole harvest; everything else one batch at a time.
            let carry = if farm { amount.min(LOAD * 8) } else { batch };
            self.spawn_cart(id, r, carry);
            // Min's oracle doubles what fishermen and hunters bring in.
            let rich = matches!(k, crate::water::FISHING_WHARF | kind::HUNTING_LODGE) && self.complex_blessing(crate::temple_complex::OSIRIS, crate::temple_complex::ORACLE);
            if rich && let Some(f) = self.buildings.get(id).map(|b| b.walkers[2]).and_then(|c| self.figures.get_mut(c)) {
                f.amount *= 2;
            }
        }
        self.send_gatherers();
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

    /// The nearest of `candidates` to `from` whose room for the goods is positive.
    fn nearest(&self, candidates: impl Iterator<Item = BuildingId>, from: (i32, i32)) -> Option<BuildingId> {
        candidates.min_by_key(|&id| (self.distance(id, from), id))
    }

    /// Room building `id` has for `r` as a delivery target (storage by its orders,
    /// industries and other users up to what they keep on site).
    fn room_for(&self, id: BuildingId, r: u16) -> i32 {
        let Some(b) = self.buildings.get(id) else { return 0 };
        if storage::is_storage(b.kind) {
            return self.storage_room(id, r);
        }
        if b.road.is_none() || b.workers <= 0 || !self.inputs_of(b.kind).contains(&r) {
            return 0;
        }
        SITE_CAP - b.stock[r as usize]
    }

    /// Where a producer's cart takes `r`, in the original's order of preference: gold
    /// to the palace, food to a granary, raw materials to a workshop that uses them,
    /// then any storage yard.
    pub fn cart_destination(&self, r: u16, from: (i32, i32)) -> Option<BuildingId> {
        let with_room = |k: u16| self.buildings.iter().filter(move |b| b.kind == k && self.room_for(b.id, r) > 0).map(|b| b.id);
        if r == resource::GOLD {
            return self.nearest(
                self.buildings.iter().filter(|b| PALACES.contains(&b.kind) && b.workers >= 5 && b.road.is_some()).map(|b| b.id),
                from,
            );
        }
        if self.is_stockpiled(r)
            && let Some(y) = self.nearest(with_room(kind::STORAGE_YARD), from)
        {
            return Some(y);
        }
        if resource::is_food(r)
            && let Some(g) = self.nearest(with_room(kind::GRANARY), from)
        {
            return Some(g);
        }
        let workshops = self.buildings.iter().filter(|b| self.is_industry(b.kind) && self.room_for(b.id, r) > 0).map(|b| b.id);
        if let Some(w) = self.nearest(workshops, from) {
            return Some(w);
        }
        self.nearest(with_room(kind::STORAGE_YARD), from)
    }

    /// Unloads what building `target` takes of a cart's goods and returns what is left.
    fn unload(&mut self, target: BuildingId, r: u16, amount: i32) -> i32 {
        let room = self.room_for(target, r).min(amount);
        if room <= 0 {
            return amount;
        }
        let is_storage = self.buildings.get(target).is_some_and(|b| storage::is_storage(b.kind));
        if r == resource::GOLD && self.buildings.get(target).is_some_and(|b| PALACES.contains(&b.kind)) {
            // The palace turns gold straight into deben.
            self.gold_delivered += amount;
            self.treasury += amount;
            self.finance.this_year.gold += amount;
            return 0;
        }
        if is_storage {
            self.add_stored(target, r, room);
        } else if let Some(b) = self.buildings.get_mut(target) {
            b.stock[r as usize] += room;
        }
        amount - room
    }

    /// Sends figure `fid` walking to building `target`'s road; false if it can't get there.
    fn head_for(&mut self, fid: FigureId, target: BuildingId) -> bool {
        let Some(road) = self.buildings.get(target).and_then(|b| b.road) else { return false };
        let map = &self.map;
        let Some(f) = self.figures.get_mut(fid) else { return false };
        f.target = target;
        f.go_to(map, road)
    }

    fn head_home(&mut self, fid: FigureId, act: u16) {
        let Some(home) = self.figures.get(fid).map(|f| f.home) else { return };
        let ok = self.head_for(fid, home);
        if let Some(f) = self.figures.get_mut(fid) {
            f.action = act;
            if !ok {
                f.dead = true;
            }
        }
    }

    pub(crate) fn update_cart(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (act, r, pos, home) = (f.action, f.cargo, (f.x, f.y), f.home);
        if self.buildings.get(home).is_none() {
            self.figures.get_mut(fid).expect("present").dead = true;
            return;
        }
        match act {
            action::WAITING => {
                if f.counter > 0 {
                    self.figures.get_mut(fid).expect("present").counter -= 1;
                    return;
                }
                let dest = self.cart_destination(r, pos);
                let ok = dest.is_some_and(|d| self.head_for(fid, d));
                let f = self.figures.get_mut(fid).expect("present");
                if ok {
                    f.action = action::DELIVERING;
                } else {
                    f.counter = CART_RETRY_TICKS;
                }
            }
            action::DELIVERING | action::FETCHING | action::BRINGING_HOME => {
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                match f.walk(map) {
                    Step::Moving => {}
                    Step::Arrived => self.cart_arrived(fid),
                    Step::Blocked | Step::Lost => {
                        // Try again from here.
                        f.route.clear();
                        f.action = if f.amount > 0 { action::WAITING } else { action::RETURNING };
                        if f.action == action::RETURNING {
                            self.head_home(fid, action::RETURNING);
                        }
                    }
                }
            }
            _ => {
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                if f.walk(map) != Step::Moving {
                    f.dead = true;
                }
            }
        }
    }

    fn cart_arrived(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (act, r, amount, target, home) = (f.action, f.cargo, f.amount, f.target, f.home);
        match act {
            action::DELIVERING => {
                let left = self.unload(target, r, amount);
                let f = self.figures.get_mut(fid).expect("present");
                f.amount = left;
                if left > 0 {
                    // The destination filled up on the way; find another.
                    f.action = action::WAITING;
                } else {
                    self.head_home(fid, action::RETURNING);
                }
            }
            action::FETCHING => {
                let room = self.storage_room(home, r);
                let take = self.take_stored(target, r, YARD_FETCH.min(room));
                let f = self.figures.get_mut(fid).expect("present");
                f.amount = take;
                self.head_home(fid, action::BRINGING_HOME);
            }
            action::BRINGING_HOME => {
                let added = self.add_stored(home, r, amount);
                let left = amount - added;
                let f = self.figures.get_mut(fid).expect("present");
                f.amount = left;
                if left > 0 {
                    f.action = action::WAITING;
                } else {
                    f.dead = true;
                }
            }
            _ => {}
        }
    }

    /// A storage yard's next errand, in the original's order: fetch goods it is set to
    /// get, supply buildings that use its goods, move food to granaries, then empty out
    /// goods it is set to empty. Needs half its staff.
    fn yard_errand(&mut self, yard: BuildingId) {
        let Some(b) = self.buildings.get(yard) else { return };
        let needed = self.workers_needed(b.kind).max(1);
        if b.workers * 2 < needed {
            return;
        }
        let from = (b.x, b.y);
        let held: Vec<(u16, i32)> = {
            let mut v: Vec<(u16, i32)> = Vec::new();
            for &(r, n) in &b.spaces {
                if n <= 0 {
                    continue;
                }
                match v.iter_mut().find(|e| e.0 == r) {
                    Some(e) => e.1 += n,
                    None => v.push((r, n)),
                }
            }
            v
        };
        // 1. Goods on "get": fetch from another storage building.
        for r in 1..resource::COUNT as u16 {
            if b.order(r) != order::GET || self.storage_room(yard, r) <= 0 {
                continue;
            }
            let sources = self
                .buildings
                .iter()
                .filter(|s| s.id != yard && storage::is_storage(s.kind) && s.order(r) != order::GET && self.stored(s.id, r) >= LOAD)
                .map(|s| s.id);
            if let Some(src) = self.nearest(sources, from) {
                self.yard_cart(yard, r, 0, src, action::FETCHING);
                return;
            }
        }
        // 2. Buildings that use goods it holds: workshops, schools, venues, guilds.
        let users: Vec<(BuildingId, u16)> = self
            .buildings
            .iter()
            .filter(|u| !storage::is_storage(u.kind))
            .flat_map(|u| held.iter().filter(|h| h.1 >= LOAD && !self.is_stockpiled(h.0) && self.room_for(u.id, h.0) >= LOAD).map(move |h| (u.id, h.0)))
            .collect();
        if let Some(&(user, r)) = users.iter().min_by_key(|(u, _)| (self.distance(*u, from), *u)) {
            let n = self.room_for(user, r).min(YARD_DELIVER);
            let taken = self.take_stored(yard, r, n);
            self.yard_cart(yard, r, taken, user, action::DELIVERING);
            return;
        }
        // 3. Food to a granary.
        for &(r, n) in &held {
            if !resource::is_food(r) {
                continue;
            }
            let granaries = self.buildings.iter().filter(|g| g.kind == kind::GRANARY && self.storage_room(g.id, r) > 0);
            if let Some(g) = self.nearest(granaries.map(|g| g.id), from) {
                let taken = self.take_stored(yard, r, n.min(YARD_DELIVER));
                self.yard_cart(yard, r, taken, g, action::DELIVERING);
                return;
            }
        }
        // 4. Goods on "empty": to any other storage that takes them.
        let b = self.buildings.get(yard).expect("present").clone();
        let mut emptied = false;
        for &(r, n) in &held {
            if b.order(r) != order::EMPTY {
                continue;
            }
            let others = self.buildings.iter().filter(|s| s.id != yard && storage::is_storage(s.kind) && self.storage_room(s.id, r) > 0);
            if let Some(o) = self.nearest(others.map(|s| s.id), from) {
                let taken = self.take_stored(yard, r, n.min(YARD_DELIVER));
                self.yard_cart(yard, r, taken, o, action::DELIVERING);
                emptied = true;
                break;
            }
        }
        // 5. A sled of material for a monument under construction.
        if !emptied {
            self.yard_monument_errand(yard);
        }
    }

    fn yard_cart(&mut self, yard: BuildingId, r: u16, amount: i32, target: BuildingId, act: u16) {
        let Some(road) = self.buildings.get(yard).and_then(|b| b.road) else { return };
        let fid = self.figures.spawn(STORAGEYARD_CART, road.0, road.1, Travel::Roads);
        if let Some(f) = self.figures.get_mut(fid) {
            f.home = yard;
            f.cargo = r;
            f.amount = amount;
            f.action = act;
        }
        if !self.head_for(fid, target) {
            // Unreachable: put the goods back.
            self.add_stored(yard, r, amount);
            if let Some(f) = self.figures.get_mut(fid) {
                f.dead = true;
            }
            return;
        }
        if let Some(b) = self.buildings.get_mut(yard) {
            b.walkers[2] = fid;
        }
    }

    /// Growth of the tree or reeds at `(x, y)`, 255 when ready to cut.
    pub fn vegetation_growth(&self, x: i32, y: i32) -> u8 {
        self.vegetation.as_ref().map_or(255, |g| g.at_or(x, y, 255))
    }

    fn set_vegetation_growth(&mut self, x: i32, y: i32, v: u8) {
        let (w, h) = (self.map.width, self.map.height);
        self.vegetation.get_or_insert_with(|| crate::grid::Grid::filled(w, h, 255)).set(x, y, v);
    }

    /// Daily: cut trees and reeds grow back, reeds quickly and trees slowly.
    pub(crate) fn grow_vegetation(&mut self) {
        let fast = self.complex_blessing(crate::temple_complex::OSIRIS, crate::temple_complex::ORACLE);
        let Some(grid) = self.vegetation.as_mut() else { return };
        for y in 0..self.map.height {
            for x in 0..self.map.width {
                let g = grid.at_or(x, y, 255);
                if g == 255 {
                    continue;
                }
                let t = self.map.terrain.at_or(x, y, 0);
                let (lo, hi) = if t & crate::map::terrain::MARSHLAND != 0 { (5, 14) } else { (1, 2) };
                let r = lo + (self.rng.byte() % (hi - lo + 1));
                // Min's oracle speeds the regrowth by a quarter.
                let r = if fast { r * 125 / 100 } else { r };
                grid.set(x, y, (g as i32 + r).min(255) as u8);
            }
        }
    }

    /// Tick 31: wood cutters and reed gatherers send their gatherer when they have room.
    fn send_gatherers(&mut self) {
        for id in self.buildings.ids() {
            let Some(b) = self.buildings.get(id) else { continue };
            let (figure, r) = match b.kind {
                WOOD_CUTTERS => (LUMBERJACK, resource::TIMBER),
                REED_GATHERERS => (REED_GATHERER, resource::REEDS),
                _ => continue,
            };
            if b.workers <= 0 || b.walkers[0] != 0 || b.stock[r as usize] >= SITE_CAP {
                continue;
            }
            let Some(road) = b.road else { continue };
            let Some(spot) = self.harvest_spot(b.kind, road) else { continue };
            let fid = self.figures.spawn(figure, road.0, road.1, Travel::Land);
            let map = &self.map;
            if let Some(f) = self.figures.get_mut(fid) {
                f.home = id;
                f.cargo = r;
                f.action = action::TO_HARVEST;
                if !f.go_to(map, spot) {
                    f.dead = true;
                }
            }
            self.buildings.get_mut(id).expect("present").walkers[0] = fid;
        }
    }

    /// The nearest grown tree (for wood cutters) or the centre of a grown 3x3 marsh (for
    /// reed gatherers) that nobody else is cutting.
    fn harvest_spot(&self, k: u16, from: (i32, i32)) -> Option<(i32, i32)> {
        use crate::map::terrain;
        let taken: Vec<(i32, i32)> = self
            .figures
            .iter()
            .filter(|f| matches!(f.kind, LUMBERJACK | REED_GATHERER) && f.action != action::RETURNING)
            .filter_map(|f| f.destination)
            .collect();
        let ok = |x: i32, y: i32| {
            if taken.contains(&(x, y)) || self.vegetation_growth(x, y) != 255 {
                return false;
            }
            if k == WOOD_CUTTERS {
                self.map.terrain_is(x, y, terrain::TREE)
            } else {
                (-1..=1).all(|dy| (-1..=1).all(|dx| self.map.terrain_is(x + dx, y + dy, terrain::MARSHLAND)))
            }
        };
        let mut best: Option<((i32, i32), i32)> = None;
        for y in 0..self.map.height {
            for x in 0..self.map.width {
                let d = (x - from.0).abs().max((y - from.1).abs());
                if best.is_some_and(|(_, bd)| d >= bd) || !ok(x, y) {
                    continue;
                }
                best = Some(((x, y), d));
            }
        }
        best.map(|(p, _)| p)
    }

    pub(crate) fn update_gatherer(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (act, home) = (f.action, f.home);
        if self.buildings.get(home).is_none() {
            self.figures.get_mut(fid).expect("present").dead = true;
            return;
        }
        match act {
            action::TO_HARVEST => {
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                match f.walk(map) {
                    Step::Moving => {}
                    Step::Arrived => {
                        f.action = action::HARVESTING;
                        f.counter = GATHER_TICKS;
                    }
                    _ => f.dead = true,
                }
            }
            action::HARVESTING => {
                let f = self.figures.get_mut(fid).expect("present");
                f.counter -= 1;
                if f.counter > 0 {
                    return;
                }
                let (x, y) = (f.x, f.y);
                f.amount = GATHER_AMOUNT;
                self.set_vegetation_growth(x, y, 0);
                self.head_home(fid, action::RETURNING);
            }
            _ => {
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                if f.walk(map) == Step::Moving {
                    return;
                }
                let (r, n) = (f.cargo, f.amount);
                f.dead = true;
                if let Some(b) = self.buildings.get_mut(home) {
                    b.stock[r as usize] += n;
                }
            }
        }
    }
}
