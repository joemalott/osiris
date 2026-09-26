//! Goods: production, cart pushers, storage-yard carts and the wood and reed gatherers.
//!
//! Industries gain progress each day at tick 20, one point per worker, while they hold
//! a load (100) of each input. A raw-material producer is done at 200 points and a
//! workshop at 400; at tick 31 a finished building with no cart out sends a cart of 100
//! units, using up its inputs and starting again from nothing. Carts take goods where
//! they're needed: weapons to the recruiter, raw materials to the least stocked
//! workshop using them, then a fully staffed storage yard, then a granary. A cart with
//! nowhere to go waits at home, and the building sends no other until it has gone.

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
    pub const PAPYRUS: u16 = 23;
    pub const COUNT: usize = 40;

    /// The goods houses use, in the order of a house's goods slots.
    pub const HOUSE_GOODS: [u16; 4] = [POTTERY, LUXURY_GOODS, LINEN, BEER];

    pub fn is_food(r: u16) -> bool {
        (GRAIN..=GAMEMEAT).contains(&r)
    }

    /// The house food slot a food fills: one per food, in resource order (grain, meat,
    /// lettuce, chickpeas, pomegranates, figs, fish, game meat).
    pub fn food_slot(r: u16) -> Option<usize> {
        is_food(r).then(|| (r - GRAIN) as usize)
    }
}

pub const LOAD: i32 = 100;
pub const GRANARY_CAPACITY: i32 = storage::CAPACITY;
/// What an industry keeps on site of each input.
const SITE_CAP: i32 = 200;
const RAW_MAX_PROGRESS: i32 = 200;
const WORKSHOP_MAX_PROGRESS: i32 = 400;
/// What a gatherer's building stores before its gatherers stay home.
const GATHER_CAP: i32 = 500;
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
const SENET_HOUSE: u16 = 32;
const PALACES: [u16; 3] = kind::PALACES;
const BRICKWORKS: u16 = 204;
/// Ticks a gatherer spends cutting once it reaches its tree or reeds.
const GATHER_TICKS: i32 = 300;
/// What a lumberjack brings back per trip, and a reed gatherer.
const TIMBER_PER_TRIP: i32 = 25;
const REEDS_PER_TRIP: i32 = 50;

pub mod action {
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
    /// A gatherer waiting by the road before he looks for his tree or reeds.
    pub const LOOKING: u16 = 8;
}

/// What a building that uses goods keeps on site of each: 300 at a scribal school or
/// senet house, 500 at a mortuary or library, 200 anywhere else. A storage yard
/// supplies it while a load more still fits.
fn site_cap(k: u16) -> i32 {
    match k {
        kind::SCRIBAL_SCHOOL | SENET_HOUSE => 300,
        kind::MORTUARY | kind::LIBRARY => 500,
        _ => SITE_CAP,
    }
}

impl World {
    pub fn resource_id(&self, key: &str) -> Option<u16> {
        self.defs.resources.iter().position(|k| k == key).map(|i| i as u16)
    }

    /// Whether building `k` produces through the daily progress of an industry (farms,
    /// the hunting lodge, gatherers and the fishing wharf have their own rules).
    fn is_industry(&self, k: u16) -> bool {
        let Some(def) = self.defs.building(k) else { return false };
        !def.outputs.is_empty()
            && !def.has_flag("is_farm")
            && !matches!(k, kind::HUNTING_LODGE | WOOD_CUTTERS | REED_GATHERERS | FISHING_WHARF)
    }

    /// Progress a load takes: 400 where it is made from inputs, 200 where it is dug up.
    pub fn max_progress(&self, k: u16) -> i32 {
        if self.inputs_of(k).is_empty() { RAW_MAX_PROGRESS } else { WORKSHOP_MAX_PROGRESS }
    }

    fn output_of(&self, k: u16) -> Option<u16> {
        self.defs.building(k)?.outputs.first().and_then(|o| self.resource_id(o))
    }

    fn inputs_of(&self, k: u16) -> Vec<u16> {
        self.defs.building(k).map_or_else(Vec::new, |d| d.inputs.iter().filter_map(|i| self.resource_id(i)).collect())
    }

    /// What building `k` needs of input `r` on hand to work, and uses up per load: a
    /// load of each, but the brickworks only a quarter load of straw.
    fn input_need(k: u16, r: u16) -> i32 {
        if k == BRICKWORKS && r == resource::STRAW { LOAD / 4 } else { LOAD }
    }

    /// Whether Ptah speeds building type `k` by half: his complex the mines, clay pits,
    /// shipwrights, jewelers and weavers; Amon's altar the quarries and brickworks.
    fn ptah_speeds(&self, k: u16) -> bool {
        use crate::temple_complex::{ALTAR, PTAH};
        match k {
            74 | 109 | 111 | 113 | 161 | 162 | 217 => self.complex_blessing(PTAH, 0),
            106 | 107 | 216 | 221 | 204 => self.complex_blessing(PTAH, ALTAR),
            _ => false,
        }
    }

    /// Tick 20: industries holding their inputs make a day's progress, one point per
    /// worker, and stop when done.
    pub(crate) fn update_production(&mut self) {
        for id in self.buildings.ids() {
            let Some(b) = self.buildings.get(id) else { continue };
            let k = b.kind;
            if !self.is_industry(k) || b.workers <= 0 || self.output_of(k).is_some_and(|r| self.is_mothballed(r)) {
                continue;
            }
            if !self.inputs_of(k).iter().all(|&r| b.stock[r as usize] >= Self::input_need(k, r)) {
                continue;
            }
            let gain = b.workers;
            let gain = if self.ptah_speeds(k) { gain + gain / 2 } else { gain };
            let max = self.max_progress(k);
            let b = self.buildings.get_mut(id).expect("present");
            b.progress = (b.progress + gain).min(max);
        }
    }

    /// Tick 31 (with walkers): finished industries and farms with a harvest send a
    /// cart, and storage yards send carts on their errands.
    pub(crate) fn send_carts(&mut self) {
        for id in self.buildings.ids() {
            let Some(b) = self.buildings.get(id) else { continue };
            if b.road.is_none() {
                continue;
            }
            let k = b.kind;
            if k == kind::STORAGE_YARD {
                if b.walkers[2] == 0 {
                    self.yard_errand(id);
                } else {
                    self.second_yard_cart(id);
                }
                continue;
            }
            if self.is_farm(k) {
                self.send_farm_carts(id);
                continue;
            }
            if b.walkers[2] != 0 || matches!(k, WOOD_CUTTERS | REED_GATHERERS) {
                continue;
            }
            let Some(r) = self.output_of(k) else { continue };
            if self.is_industry(k) {
                if b.progress < self.max_progress(k) {
                    continue;
                }
                // The cart takes the load, and the inputs it was made from are used up.
                let inputs = self.inputs_of(k);
                let b = self.buildings.get_mut(id).expect("present");
                b.progress = 0;
                for i in inputs {
                    b.stock[i as usize] = (b.stock[i as usize] - Self::input_need(k, i)).max(0);
                }
                b.stock[r as usize] += LOAD;
                self.spawn_cart(id, r, LOAD);
                continue;
            }
            let amount = b.stock[r as usize];
            if amount < LOAD {
                continue;
            }
            self.spawn_cart(id, r, LOAD);
            // Min's oracle doubles what fishermen and hunters bring in.
            let rich = matches!(k, crate::water::FISHING_WHARF | kind::HUNTING_LODGE) && self.complex_blessing(crate::temple_complex::OSIRIS, crate::temple_complex::ORACLE);
            if rich && let Some(f) = self.buildings.get(id).map(|b| b.walkers[2]).and_then(|c| self.figures.get_mut(c)) {
                f.amount *= 2;
            }
        }
        self.send_gathered();
        self.send_gatherers();
    }

    /// A farm sends its whole harvest in one cart, and a grain farm its straw in another.
    fn send_farm_carts(&mut self, id: BuildingId) {
        let Some(b) = self.buildings.get(id) else { return };
        let outputs: Vec<u16> = self.defs.building(b.kind).map_or_else(Vec::new, |d| d.outputs.iter().filter_map(|o| self.resource_id(o)).collect());
        for (i, r) in outputs.into_iter().enumerate().take(2) {
            let Some(b) = self.buildings.get(id) else { return };
            let amount = b.stock[r as usize];
            let busy = if i == 0 { b.walkers[2] != 0 } else { self.figures.iter().any(|f| f.kind == CART_PUSHER && f.home == id && f.cargo == r && !f.dead) };
            if amount > 0 && !busy {
                self.spawn_cart_in(id, r, amount, (i == 0).then_some(2));
            }
        }
    }

    /// Wood cutters and reed gatherers send a cart of 100 from what their gatherers
    /// brought in: wood cutters once they hold 100, reed gatherers once they hold more
    /// than 50 (the load is made up from what comes in next).
    fn send_gathered(&mut self) {
        for id in self.buildings.ids() {
            let Some(b) = self.buildings.get(id) else { continue };
            let (r, min) = match b.kind {
                WOOD_CUTTERS => (resource::TIMBER, LOAD),
                REED_GATHERERS => (resource::REEDS, REEDS_PER_TRIP + 1),
                _ => continue,
            };
            if b.walkers[2] != 0 || b.road.is_none() || b.workers <= 0 || b.stock[r as usize] < min {
                continue;
            }
            self.spawn_cart(id, r, LOAD);
        }
    }

    pub fn spawn_cart(&mut self, home: BuildingId, r: u16, amount: i32) {
        self.spawn_cart_in(home, r, amount, Some(2));
    }

    /// Sends a cart of `amount` of `r` out of `home`'s stock, recorded in walker slot `slot`.
    fn spawn_cart_in(&mut self, home: BuildingId, r: u16, amount: i32, slot: Option<usize>) {
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
            if let Some(slot) = slot {
                b.walkers[slot] = fid;
            }
        }
    }

    /// Of `candidates`, the one first reached from `from` by road, as the original
    /// chooses between destinations: a road search outward from `from` (north, east,
    /// south, west) that stops at the first tile that is a candidate's road access, the
    /// last listed winning a shared tile. None when no candidate can be reached; a lone
    /// candidate is taken as is.
    fn nearest(&self, candidates: impl Iterator<Item = BuildingId>, from: (i32, i32)) -> Option<BuildingId> {
        let list: Vec<(BuildingId, (i32, i32))> = candidates.filter_map(|id| Some((id, self.buildings.get(id)?.road?))).collect();
        if list.len() <= 1 {
            return list.first().map(|c| c.0);
        }
        let map = &self.map;
        if !map.contains(from.0, from.1) {
            return None;
        }
        let w = map.width;
        let mut seen = vec![false; (w * map.height) as usize];
        seen[(from.1 * w + from.0) as usize] = true;
        let mut queue = std::collections::VecDeque::from([from]);
        while let Some((x, y)) = queue.pop_front() {
            if let Some(&(id, _)) = list.iter().rev().find(|c| c.1 == (x, y)) {
                return Some(id);
            }
            for d in [0, 2, 4, 6] {
                let (nx, ny) = (x + crate::map::NEIGHBOURS[d].0, y + crate::map::NEIGHBOURS[d].1);
                if !map.contains(nx, ny) || std::mem::replace(&mut seen[(ny * w + nx) as usize], true) {
                    continue;
                }
                if crate::figures::passable(map, Travel::Roads, nx, ny) || list.iter().any(|c| c.1 == (nx, ny)) {
                    queue.push_back((nx, ny));
                }
            }
        }
        None
    }

    /// Room building `id` has for `r` as a delivery target (storage by its orders, a
    /// palace for any gold, industries and other users up to what they keep on site).
    fn room_for(&self, id: BuildingId, r: u16) -> i32 {
        let Some(b) = self.buildings.get(id) else { return 0 };
        if storage::is_storage(b.kind) {
            return self.storage_room(id, r);
        }
        if r == resource::GOLD && PALACES.contains(&b.kind) {
            return i32::MAX;
        }
        if b.road.is_none() || !self.inputs_of(b.kind).contains(&r) {
            return 0;
        }
        site_cap(b.kind) - b.stock[r as usize]
    }

    /// What carts already on their way are bringing building `id` of `r`.
    fn incoming(&self, id: BuildingId, r: u16) -> i32 {
        self.figures
            .iter()
            .filter(|f| matches!(f.kind, CART_PUSHER | STORAGEYARD_CART) && f.action == action::DELIVERING && f.target == id && f.cargo == r && !f.dead)
            .map(|f| f.amount)
            .sum()
    }

    /// Whether building `id` has all the staff it can hire.
    fn fully_staffed(&self, id: BuildingId) -> bool {
        self.buildings.get(id).is_some_and(|b| b.workers >= self.workers_needed(b.kind).max(1))
    }

    /// The building using `r` that a load of it should go to: the one holding least,
    /// then with least on its way, then the nearest by road. It must have room for a
    /// whole load beyond what it holds, and more than what is on its way.
    fn user_for(&self, r: u16, from: (i32, i32), staffed: bool) -> Option<BuildingId> {
        let users: Vec<((i32, i32), BuildingId)> = self
            .buildings
            .iter()
            .filter(|b| !storage::is_storage(b.kind) && (!staffed || b.workers > 0) && self.inputs_of(b.kind).contains(&r) && b.road.is_some())
            .filter_map(|b| {
                let (stock, cap) = (b.stock[r as usize], site_cap(b.kind));
                let coming = self.incoming(b.id, r);
                (stock + LOAD <= cap && stock + coming < cap).then_some(((stock, coming), b.id))
            })
            .collect();
        let least = users.iter().map(|u| u.0).min()?;
        self.nearest(users.iter().filter(|u| u.0 == least).map(|u| u.1), from)
    }

    /// Where a producer's cart takes `r`, in the original's order: gold to a staffed
    /// palace; unless the good is stockpiled, a building that uses it; a fully staffed
    /// storage yard that takes it; then, for food, a fully staffed granary with room
    /// for a load (skipped for stockpiled food until nothing else will take it).
    pub fn cart_destination(&self, r: u16, from: (i32, i32)) -> Option<BuildingId> {
        if r == resource::GOLD {
            return self.nearest(
                self.buildings.iter().filter(|b| PALACES.contains(&b.kind) && b.workers > 0 && b.road.is_some()).map(|b| b.id),
                from,
            );
        }
        let stockpiled = self.is_stockpiled(r);
        if !stockpiled && let Some(u) = self.user_for(r, from, false) {
            return Some(u);
        }
        let yards = self.buildings.iter().filter(|b| b.kind == kind::STORAGE_YARD && self.fully_staffed(b.id) && self.storage_room(b.id, r) > 0).map(|b| b.id);
        if let Some(y) = self.nearest(yards, from) {
            return Some(y);
        }
        if !resource::is_food(r) {
            return None;
        }
        let granaries = self
            .buildings
            .iter()
            .filter(|b| b.kind == kind::GRANARY && self.fully_staffed(b.id) && self.storage_room(b.id, r) - self.incoming(b.id, r) >= LOAD)
            .map(|b| b.id);
        self.nearest(granaries, from)
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

    /// A storage yard's next errand for its first cart, in the original's order: fetch the good on "get"
    /// it holds least of (by share of its limit), supply buildings that use goods it
    /// holds, send out goods on "empty", then haul for monuments. Needs half its staff.
    fn yard_errand(&mut self, yard: BuildingId) {
        let Some(b) = self.buildings.get(yard) else { return };
        let needed = self.workers_needed(b.kind).max(1);
        if b.workers * 2 < needed {
            return;
        }
        let from = b.road.unwrap_or((b.x, b.y));
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
        if let Some(&(_, r, src)) = self.yard_gets(yard).first() {
            self.yard_cart(yard, r, 0, src, action::FETCHING);
            return;
        }
        // 2. A load for a staffed building that uses a good it holds: workshops, schools,
        // libraries, mortuaries, venues, guilds, the recruiter.
        for &(r, n) in &held {
            if self.is_stockpiled(r) {
                continue;
            }
            if let Some(user) = self.user_for(r, from, true) {
                let taken = self.take_stored(yard, r, n.min(LOAD));
                self.yard_cart(yard, r, taken, user, action::DELIVERING);
                return;
            }
        }
        // 3. Goods on "empty": a cart of 400 (100 of heavy goods) to a building that uses
        // them, a granary for food, or another storage yard. (Monument material
        // leaves on work-camp laborers' sleds, not the yard's carts.)
        let b = self.buildings.get(yard).expect("present").clone();
        for &(r, n) in &held {
            if b.order(r) != order::EMPTY {
                continue;
            }
            let heavy = matches!(self.defs.resources.get(r as usize).map(String::as_str), Some("stone" | "limestone" | "granite" | "sandstone" | "marble" | "bricks" | "weapons" | "chariots"));
            let load = if heavy { LOAD } else { YARD_DELIVER };
            let granary = || {
                let g = self.buildings.iter().filter(|g| g.kind == kind::GRANARY && self.fully_staffed(g.id) && self.storage_room(g.id, r) >= LOAD);
                self.nearest(g.map(|g| g.id), from)
            };
            let yards = || {
                let y = self.buildings.iter().filter(|s| s.id != yard && s.kind == kind::STORAGE_YARD && self.fully_staffed(s.id) && self.storage_room(s.id, r) > 0);
                self.nearest(y.map(|s| s.id), from)
            };
            let target = self.user_for(r, from, false).or_else(|| if resource::is_food(r) { granary() } else { None }).or_else(yards);
            if let Some(t) = target {
                let taken = self.take_stored(yard, r, n.min(load));
                self.yard_cart(yard, r, taken, t, action::DELIVERING);
                return;
            }
        }
    }

    /// A yard's goods on "get" it has room for and another storage building can supply,
    /// with the source, the one it holds least of (by share of its limit) first.
    fn yard_gets(&self, yard: BuildingId) -> Vec<(i32, u16, BuildingId)> {
        let Some(b) = self.buildings.get(yard) else { return vec![] };
        let from = (b.x, b.y);
        let mut gets: Vec<(i32, u16, BuildingId)> = Vec::new();
        for r in 1..resource::COUNT as u16 {
            if b.order(r) != order::GET || self.is_stockpiled(r) || self.storage_room(yard, r) <= 0 {
                continue;
            }
            let sources = self
                .buildings
                .iter()
                .filter(|s| s.id != yard && storage::is_storage(s.kind) && s.order(r) != order::GET && self.stored(s.id, r) >= LOAD)
                .map(|s| s.id);
            if let Some(src) = self.nearest(sources, from) {
                let share = (self.stored(yard, r) * 100 / b.order_cap(r).max(1)).min(100);
                gets.push((share, r, src));
            }
        }
        gets.sort();
        gets
    }

    /// While its first cart is out, a yard at least half staffed may send a second, only
    /// ever to fetch: the good on "get" it holds second least of, or, when it wants just
    /// one, that good again while it holds under half its limit.
    fn second_yard_cart(&mut self, yard: BuildingId) {
        let Some(b) = self.buildings.get(yard) else { return };
        if b.workers * 2 < self.workers_needed(b.kind).max(1) {
            return;
        }
        let first = b.walkers[2];
        if self.figures.iter().any(|f| f.kind == STORAGEYARD_CART && f.home == yard && f.id != first && !f.dead) {
            return;
        }
        let gets = self.yard_gets(yard);
        let pick = match gets.as_slice() {
            [] => None,
            [(share, r, src)] => (*share < 50).then_some((*r, *src)),
            [_, (_, r, src), ..] => Some((*r, *src)),
        };
        let Some((r, src)) = pick else { return };
        let Some(road) = self.buildings.get(yard).and_then(|b| b.road) else { return };
        let fid = self.figures.spawn(STORAGEYARD_CART, road.0, road.1, Travel::Roads);
        if let Some(f) = self.figures.get_mut(fid) {
            f.home = yard;
            f.cargo = r;
            f.action = action::FETCHING;
        }
        if !self.head_for(fid, src)
            && let Some(f) = self.figures.get_mut(fid)
        {
            f.dead = true;
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

    /// Growth of the tree or reeds at `(x, y)`, 255 when ready to cut. The map file
    /// holds it, as the original's does.
    pub fn vegetation_growth(&self, x: i32, y: i32) -> u8 {
        self.map.vegetation.at_or(x, y, 255)
    }

    /// Tick 20: cut trees and reeds grow back (FUN_004860d0). Each day a cut tree gains
    /// 35 to 44 and cut reeds 25 to 34, a quarter more with Min's oracle; the growth is
    /// a byte that wraps past 255, so a tile is grown again only when it comes to rest
    /// on 255 exactly, on average after some 256 days. The same pass settles whether
    /// any staffed wood cutter, and any staffed reed gatherer, can walk to something
    /// grown: until one can, they send nobody out.
    pub(crate) fn grow_vegetation(&mut self) {
        use crate::map::terrain;
        let pct = if self.complex_blessing(crate::temple_complex::OSIRIS, crate::temple_complex::ORACLE) { 125 } else { 100 };
        let mut changed = Vec::new();
        let (mut trees, mut reeds) = (false, false);
        for y in 0..self.map.height {
            for x in 0..self.map.width {
                let t = self.map.terrain.at_or(x, y, 0);
                let tree = t & terrain::TREE != 0;
                if !tree && t & terrain::MARSHLAND == 0 {
                    continue;
                }
                let g = self.map.vegetation.at_or(x, y, 255);
                if g == 255 {
                    if tree {
                        trees = true;
                    } else {
                        reeds = true;
                    }
                    continue;
                }
                let base = if tree { 35 } else { 25 };
                let gain = (self.rng.below(10) + base) * pct / 100;
                let g = (g as i32 + gain) as u8;
                self.map.vegetation.set(x, y, g);
                if g == 255 {
                    changed.push((x, y));
                }
            }
        }
        if !changed.is_empty() {
            crate::terrain_images::refresh_vegetation(&mut self.map, &self.defs, &changed);
        }
        self.wood_reachable = trees && self.gatherer_can_reach(WOOD_CUTTERS);
        self.reeds_reachable = reeds && self.gatherer_can_reach(REED_GATHERERS);
    }

    /// Whether a staffed building of kind `k` can walk from its road to a grown tree
    /// (wood cutters) or grown marsh (reed gatherers).
    fn gatherer_can_reach(&self, k: u16) -> bool {
        use crate::map::terrain;
        let bit = if k == WOOD_CUTTERS { terrain::TREE } else { terrain::MARSHLAND };
        let grown = |x: i32, y: i32| {
            let t = self.map.terrain.at_or(x, y, 0);
            t & bit != 0 && (bit == terrain::TREE || t & terrain::TREE == 0) && self.vegetation_growth(x, y) == 255
        };
        self.buildings.iter().filter(|b| b.kind == k && b.workers > 0).filter_map(|b| b.road).any(|road| self.first_reached(road, &grown).is_some())
    }

    /// The first tile satisfying `want` that a walker from `from` comes to, searching
    /// outward over the ground people walk (north, east, south, west), as the original
    /// does for gatherers (FUN_005190a0). The tile itself must be walkable ground.
    fn first_reached(&self, from: (i32, i32), want: &dyn Fn(i32, i32) -> bool) -> Option<(i32, i32)> {
        let map = &self.map;
        if !map.contains(from.0, from.1) {
            return None;
        }
        let w = map.width;
        let mut seen = vec![false; (w * map.height) as usize];
        seen[(from.1 * w + from.0) as usize] = true;
        let mut queue = std::collections::VecDeque::from([from]);
        while let Some((x, y)) = queue.pop_front() {
            for d in [0, 2, 4, 6] {
                let (nx, ny) = (x + crate::map::NEIGHBOURS[d].0, y + crate::map::NEIGHBOURS[d].1);
                if !map.contains(nx, ny) || seen[(ny * w + nx) as usize] || !crate::figures::passable(map, Travel::Land, nx, ny) {
                    continue;
                }
                seen[(ny * w + nx) as usize] = true;
                if want(nx, ny) {
                    return Some((nx, ny));
                }
                queue.push_back((nx, ny));
            }
        }
        None
    }

    /// Tick 31: wood cutters and reed gatherers send out gatherers by their staffing,
    /// as the original does (FUN_00462180): a wood cutter three at full staff, two from
    /// half, else one; a reed gatherer five at full staff, four from three quarters,
    /// two from half, else one; no more than the room left under 500 stored allows,
    /// and none while none of their kind can reach anything grown. Each waits a moment
    /// by the road (0 to 29 ticks) before looking for his tree or reeds.
    fn send_gatherers(&mut self) {
        for id in self.buildings.ids() {
            let Some(b) = self.buildings.get(id) else { continue };
            let (figure, r, per_trip, reachable) = match b.kind {
                WOOD_CUTTERS => (LUMBERJACK, resource::TIMBER, TIMBER_PER_TRIP, self.wood_reachable),
                REED_GATHERERS => (REED_GATHERER, resource::REEDS, REEDS_PER_TRIP, self.reeds_reachable),
                _ => continue,
            };
            let Some(road) = b.road else { continue };
            let (k, stock) = (b.kind, b.stock[r as usize]);
            let pct = b.workers * 100 / self.workers_needed(k).max(1);
            let wanted = match (k, pct) {
                (_, p) if p <= 0 => continue,
                (WOOD_CUTTERS, p) if p >= 100 => 3,
                (WOOD_CUTTERS, p) if p >= 50 => 2,
                (REED_GATHERERS, p) if p >= 100 => 5,
                (REED_GATHERERS, p) if p >= 75 => 4,
                (REED_GATHERERS, p) if p >= 50 => 2,
                _ => 1,
            };
            let wanted = wanted.min((GATHER_CAP - stock).max(0) / per_trip);
            if !reachable {
                continue;
            }
            let mut out = self.figures.iter().filter(|f| f.kind == figure && f.home == id && !f.dead).count() as i32;
            while out < wanted {
                let wait = self.rng.below(30);
                let fid = self.figures.spawn(figure, road.0, road.1, Travel::PreferRoads);
                if let Some(f) = self.figures.get_mut(fid) {
                    f.home = id;
                    f.cargo = r;
                    f.action = action::LOOKING;
                    f.counter = wait;
                }
                out += 1;
            }
        }
    }

    /// The grown tree (for a lumberjack) or the grown middle of a 3x3 marsh (for a reed
    /// gatherer) a gatherer at `from` walks to first, passing over any where another of
    /// his kind stands (FUN_00486810).
    fn harvest_spot(&self, figure: u16, from: (i32, i32)) -> Option<(i32, i32)> {
        use crate::map::terrain;
        let standing: Vec<(i32, i32)> = self.figures.iter().filter(|f| f.kind == figure && !f.dead).map(|f| (f.x, f.y)).collect();
        let ok = |x: i32, y: i32| {
            let t = self.map.terrain.at_or(x, y, 0);
            if t & terrain::DIKE != 0 || self.vegetation_growth(x, y) != 255 || standing.contains(&(x, y)) {
                return false;
            }
            if figure == LUMBERJACK {
                t & terrain::TREE != 0
            } else {
                t & terrain::MARSHLAND != 0 && crate::map::NEIGHBOURS.iter().all(|&(dx, dy)| self.map.terrain_is(x + dx, y + dy, terrain::MARSHLAND))
            }
        };
        self.first_reached(from, &ok)
    }

    /// A lumberjack (FUN_004a3070) or reed gatherer (FUN_004ac9f0): he waits by the road,
    /// looks for the nearest grown tree or reeds and walks there, cuts for 300 ticks
    /// (the tile is then bare and grows back), and carries 25 timber or 50 reeds home.
    /// Finding nothing, he goes, and his kind stop going out until the next day's
    /// growth finds something within reach. Reaching a tile another is already
    /// cutting, or one cut meanwhile, he looks again.
    pub(crate) fn update_gatherer(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (act, home, kind) = (f.action, f.home, f.kind);
        if self.buildings.get(home).is_none() {
            self.figures.get_mut(fid).expect("present").dead = true;
            return;
        }
        match act {
            action::LOOKING => {
                let f = self.figures.get_mut(fid).expect("present");
                // A lumberjack counts his wait down, a reed gatherer up past 10.
                let ready = if kind == LUMBERJACK {
                    f.counter -= 1;
                    f.counter <= 0
                } else {
                    f.counter += 1;
                    f.counter > 10
                };
                if !ready {
                    return;
                }
                let pos = (f.x, f.y);
                let spot = self.harvest_spot(kind, pos);
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                match spot {
                    Some(spot) if f.go_to(map, spot) => {
                        f.action = action::TO_HARVEST;
                    }
                    Some(_) => f.dead = true,
                    None => {
                        f.dead = true;
                        if kind == LUMBERJACK {
                            self.wood_reachable = false;
                        } else {
                            self.reeds_reachable = false;
                        }
                    }
                }
            }
            action::TO_HARVEST => {
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                match f.walk(map) {
                    Step::Moving => {}
                    Step::Arrived => {
                        let (x, y) = (f.x, f.y);
                        let taken = self.figures.iter().any(|o| o.id != fid && o.kind == kind && o.action == action::HARVESTING && (o.x, o.y) == (x, y) && !o.dead);
                        let grown = self.vegetation_growth(x, y) == 255;
                        let f = self.figures.get_mut(fid).expect("present");
                        f.counter = 0;
                        f.action = if grown && !taken { action::HARVESTING } else { action::LOOKING };
                    }
                    _ => f.dead = true,
                }
            }
            action::HARVESTING => {
                let f = self.figures.get_mut(fid).expect("present");
                f.counter += 1;
                if f.counter <= GATHER_TICKS {
                    return;
                }
                let (x, y) = (f.x, f.y);
                f.amount = if f.kind == REED_GATHERER { REEDS_PER_TRIP } else { TIMBER_PER_TRIP };
                self.map.vegetation.set(x, y, 0);
                crate::terrain_images::refresh_vegetation(&mut self.map, &self.defs, &[(x, y)]);
                self.head_home(fid, action::RETURNING);
            }
            _ => {
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                match f.walk(map) {
                    Step::Moving => {}
                    Step::Arrived => {
                        let (r, n) = (f.cargo, f.amount);
                        f.dead = true;
                        if let Some(b) = self.buildings.get_mut(home) {
                            b.stock[r as usize] += n;
                        }
                    }
                    // Lost on the way home, he and his load are gone.
                    _ => f.dead = true,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brickworks_use_a_quarter_load_of_straw() {
        assert_eq!(World::input_need(BRICKWORKS, resource::STRAW), 25);
        assert_eq!(World::input_need(BRICKWORKS, 11), LOAD);
        assert_eq!(World::input_need(114, 11), LOAD);
    }

    /// Campaign mission `n` with everything allowed and staffed, and no fires.
    pub(crate) fn mission(n: usize) -> Option<World> {
        let data = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../PharaohData");
        if !data.join("mission1.pak").is_file() {
            return None;
        }
        let library = osiris_formats::ImageLibrary::open(&data.join("Data")).ok()?;
        let defs = std::sync::Arc::new(crate::defs::Defs::load(&library).ok()?);
        let model = osiris_formats::Model::parse(&String::from_utf8_lossy(&std::fs::read(data.join("Pharaoh_Model_Normal.txt")).ok()?)).ok()?;
        let balance = std::sync::Arc::new(crate::balance::Balance::from_model(&model));
        let scenario = osiris_formats::MissionPak::open(&data.join("mission1.pak")).ok()?.scenario(n).ok()?;
        let mut world = World::new(&scenario, defs, balance);
        world.start(&scenario);
        world.load_mission(n as i32);
        world.scenario_allowed = None;
        if let Some(m) = world.mission.as_mut() {
            m.allowed.extend(0..300);
        }
        world.test_full_staff = true;
        world.rules.global_labor_pool = true;
        world.rules.fire = false;
        world.rules.collapse = false;
        Some(world)
    }

    #[test]
    fn lumberjacks_cut_the_nearest_trees_which_grow_back() {
        use crate::world::{Command, Outcome};
        let Some(mut world) = mission(2) else { return };
        for cmd in [Command::Road { start: (58, 86), end: (68, 86) }, Command::Build { kind: WOOD_CUTTERS, x: 60, y: 87, x1: 60, y1: 87 }] {
            assert!(matches!(world.apply(&cmd), Outcome::Done { .. }), "{cmd:?}");
        }
        let tree = |w: &World, x, y| w.map.terrain_is(x, y, crate::map::terrain::TREE);
        assert!(tree(&world, 54, 86) && world.vegetation_growth(54, 86) == 255);
        let grown_image = world.map.images.at_or(54, 86, 0);
        for _ in 0..900 {
            world.tick();
        }
        // The nearest trees by the way a walker goes from the road are down, drawn as
        // stumps, and 25 timber came home for each.
        assert!(world.vegetation_growth(54, 86) < 100, "cut, and a day or so grown");
        assert_ne!(world.map.images.at_or(54, 86, 0), grown_image);
        let young = world.defs.terrain.young_tree;
        assert!((young..young + 24).contains(&world.map.images.at_or(54, 86, 0)));
        let b = world.buildings.iter().find(|b| b.kind == WOOD_CUTTERS).expect("wood cutter");
        assert!(b.stock[resource::TIMBER as usize] >= 50 && b.stock[resource::TIMBER as usize] % 25 == 0);
        // A cut tree gains 35 to 44 a day, wrapping past 255, so it stands again only
        // when it comes to rest on 255 exactly.
        world.map.vegetation.set(54, 86, 250);
        let mut seen = vec![];
        for _ in 0..400 {
            world.grow_vegetation();
            let g = world.vegetation_growth(54, 86);
            seen.push(g);
            if g == 255 {
                break;
            }
        }
        assert!(seen[0] < 40, "250 + 35..44 wraps: {}", seen[0]);
        assert_eq!(*seen.last().unwrap(), 255, "grown within 400 days");
        assert_eq!(world.map.images.at_or(54, 86, 0), grown_image);
    }

    #[test]
    fn reed_gatherers_cut_the_middle_of_the_marsh() {
        use crate::world::{Command, Outcome};
        let Some(mut world) = mission(4) else { return };
        for cmd in [Command::Road { start: (94, 78), end: (94, 95) }, Command::Build { kind: REED_GATHERERS, x: 95, y: 80, x1: 95, y1: 80 }] {
            assert!(matches!(world.apply(&cmd), Outcome::Done { .. }), "{cmd:?}");
        }
        for _ in 0..1300 {
            world.tick();
        }
        // Five went out at full staff; what they cut is the middle of the marsh, drawn
        // bare, and each brought 50 reeds.
        let marsh = |x: i32, y: i32| world.map.terrain_is(x, y, crate::map::terrain::MARSHLAND);
        let cut: Vec<(i32, i32)> = (60..130).flat_map(|x| (60..100).map(move |y| (x, y))).filter(|&(x, y)| marsh(x, y) && world.vegetation_growth(x, y) != 255).collect();
        assert!(cut.len() >= 3, "{cut:?}");
        for &(x, y) in &cut {
            assert!(crate::map::NEIGHBOURS.iter().all(|&(dx, dy)| marsh(x + dx, y + dy)));
            let reeds = world.defs.terrain.reeds;
            assert!((reeds..reeds + 8).contains(&world.map.images.at_or(x, y, 0)));
        }
        let b = world.buildings.iter().find(|b| b.kind == REED_GATHERERS).expect("reed gatherer");
        let carted: i32 = world.figures.iter().filter(|f| f.kind == CART_PUSHER && f.home == b.id).map(|f| f.amount).sum();
        assert_eq!(b.stock[resource::REEDS as usize] + carted, cut.len() as i32 * REEDS_PER_TRIP);
    }
}
