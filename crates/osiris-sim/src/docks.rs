//! Docks, dockers and trade ships.
//!
//! A city on a sea route sends its traders by ship once the player has a staffed dock
//! with open water. The ship sails in at the river entry and, if the city has anything
//! to trade with us (a good it sells that we import, or one it buys that we export),
//! makes for the nearest free dock. A dock moors one ship at a time; a ship finding
//! them all taken waits off the nearest one, and gives up after 25 days.
//!
//! While a ship is moored the dock sends out dockers, one while under half staffed,
//! two from half and three from three quarters. A docker either carts a load of the
//! ship's goods to the nearest storage yard with room for it, where the city pays for
//! them, or fetches a load of a good we export from the nearest yard holding one and
//! sells it to the ship. Unloading comes first. The ship leaves by the river exit once
//! its dockers are back and there is nothing left to trade, or its hold is full both
//! ways; the trader then sails home along its route.

use crate::buildings::{BuildingId, kind};
use crate::economy::LOAD;
use crate::figures::{FigureId, Step, Travel};
use crate::trade::RESOURCES;
use crate::water::DOCK;
use crate::world::World;

pub const TRADE_SHIP: u16 = 20;
pub const DOCKER: u16 = 38;

/// Ticks between a moored ship's calls for dockers.
const DEAL_TICKS: i32 = 10;
/// Ticks a ship waiting for a dock looks again.
const QUEUE_TICKS: i32 = 50;
/// A ship with no dock to go to gives up after this many ticks (25 days).
const IDLE_MAX: i32 = 25 * 50;

pub mod ship_action {
    pub const TO_DOCK: u16 = 1;
    pub const MOORED: u16 = 2;
    pub const TO_QUEUE: u16 = 3;
    pub const QUEUED: u16 = 4;
    pub const LEAVING: u16 = 5;
}

mod docker_action {
    /// Carting the ship's goods to a yard.
    pub const IMPORTING: u16 = 1;
    /// Walking to a yard to fetch goods for the ship.
    pub const FETCHING: u16 = 2;
    /// Walking back to the dock.
    pub const RETURNING: u16 = 3;
}

/// Where ships moor, wait and queue further out, from a dock's corner by facing.
fn dock_tiles(x: i32, y: i32, orientation: u8) -> [(i32, i32); 3] {
    let o = match orientation {
        0 => [(1, -1), (2, -2), (2, -3)],
        1 => [(3, 1), (4, 2), (5, 2)],
        2 => [(1, 3), (2, 4), (2, 5)],
        _ => [(-1, 1), (-2, 2), (-3, 2)],
    };
    o.map(|(dx, dy)| (x + dx, y + dy))
}

impl World {
    /// Docks able to receive ships: staffed, on a road and on open water.
    fn working_docks(&self) -> Vec<BuildingId> {
        self.buildings
            .iter()
            .filter(|b| b.kind == DOCK && b.workers > 0 && b.road.is_some())
            .map(|b| b.id)
            .filter(|&id| self.has_open_water(id))
            .collect()
    }

    /// Whether ships can come: the river has an entry and the city has a working dock.
    pub fn sea_trade_open(&self) -> bool {
        self.water.river_entry.is_some() && self.buildings.iter().any(|b| b.kind == DOCK && b.workers > 0)
    }

    /// The ship moored at dock `id`.
    pub fn moored_ship(&self, id: BuildingId) -> Option<FigureId> {
        self.figures.iter().find(|f| f.kind == TRADE_SHIP && f.home == id && f.action == ship_action::MOORED).map(|f| f.id)
    }

    fn ship_city(&self, fid: FigureId) -> Option<usize> {
        let f = self.figures.get(fid)?;
        self.trade.traders.get(f.target as usize).map(|t| t.city)
    }

    /// What a ship from `city` could do here: 2 for each good it sells that we import,
    /// 1 for each it buys that we export.
    fn dock_score(&self, city: usize) -> i32 {
        (1..RESOURCES as u16).map(|r| if self.can_import(city, r) { 2 } else if self.can_export(city, r) { 1 } else { 0 }).sum()
    }

    /// A trade ship arrives at the river entry for trader `trader`.
    pub(crate) fn ship_arrives(&mut self, trader: usize) {
        let Some((x, y)) = self.river_entry() else {
            if let Some(t) = self.trade.traders.get_mut(trader) {
                t.returning = true;
                t.step = 0;
            }
            return;
        };
        let fid = self.figures.spawn(TRADE_SHIP, x, y, Travel::Water);
        let capacity = self.defs.figure(TRADE_SHIP).and_then(|d| d.int("max_capacity")).unwrap_or(1200) as i32;
        if let Some(f) = self.figures.get_mut(fid) {
            f.target = trader as u32;
            f.roam_left = capacity;
            f.roam_turn = 0;
            f.action = ship_action::QUEUED;
        }
        self.trade.traders[trader].figure = fid;
        self.ship_find_dock(fid);
    }

    /// Sends ship `fid` to the best free dock, or to wait off the best taken one, or
    /// away when no dock suits it.
    fn ship_find_dock(&mut self, fid: FigureId) {
        let Some(city) = self.ship_city(fid) else { return };
        let Some(f) = self.figures.get(fid) else { return };
        let from = (f.x, f.y);
        if self.dock_score(city) == 0 {
            self.ship_leave(fid);
            return;
        }
        let taken = |id: BuildingId| {
            self.figures.iter().any(|o| o.id != fid && o.kind == TRADE_SHIP && o.home == id && matches!(o.action, ship_action::TO_DOCK | ship_action::MOORED))
        };
        let docks = self.working_docks();
        let near = |id: &BuildingId| self.buildings.get(*id).map_or(i32::MAX, |b| (b.x - from.0).abs().max((b.y - from.1).abs()));
        let free = docks.iter().copied().filter(|&id| !taken(id)).min_by_key(|id| (near(id), *id));
        let (dock, act, slot) = match free {
            Some(d) => (d, ship_action::TO_DOCK, 0),
            None => match docks.iter().copied().min_by_key(|id| (near(id), *id)) {
                Some(d) => (d, ship_action::TO_QUEUE, 1),
                None => {
                    let f = self.figures.get_mut(fid).expect("present");
                    f.action = ship_action::QUEUED;
                    f.counter = QUEUE_TICKS;
                    return;
                }
            },
        };
        let b = self.buildings.get(dock).expect("working dock");
        let to = dock_tiles(b.x, b.y, b.orientation)[slot];
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("present");
        if f.go_to(map, to) {
            f.home = dock;
            f.action = act;
        } else {
            f.action = ship_action::QUEUED;
            f.counter = QUEUE_TICKS;
        }
    }

    fn ship_leave(&mut self, fid: FigureId) {
        let exit = self.river_exit();
        let map = &self.map;
        let Some(f) = self.figures.get_mut(fid) else { return };
        f.action = ship_action::LEAVING;
        f.home = 0;
        if !exit.is_some_and(|e| f.go_to(map, e)) {
            self.caravan_gone(fid);
        }
    }

    pub(crate) fn update_trade_ship(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (act, dock) = (f.action, f.home);
        match act {
            ship_action::TO_DOCK | ship_action::TO_QUEUE | ship_action::LEAVING => {
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                match f.walk(map) {
                    Step::Moving => {}
                    Step::Arrived if act == ship_action::TO_DOCK => {
                        f.action = ship_action::MOORED;
                        f.counter = DEAL_TICKS;
                        f.direction = self.buildings.get(dock).map_or(f.direction, |b| (b.orientation * 2 + 4) % 8);
                    }
                    Step::Arrived if act == ship_action::TO_QUEUE => {
                        f.action = ship_action::QUEUED;
                        f.counter = QUEUE_TICKS;
                    }
                    Step::Blocked if act != ship_action::LEAVING => {
                        f.route.clear();
                        f.action = ship_action::QUEUED;
                        f.counter = QUEUE_TICKS;
                    }
                    _ => self.caravan_gone(fid),
                }
            }
            ship_action::QUEUED => {
                let f = self.figures.get_mut(fid).expect("present");
                f.counter -= 1;
                if f.counter > 0 {
                    return;
                }
                // `roam_turn` counts the looks it has had.
                f.roam_turn = f.roam_turn.saturating_add(1);
                if f.roam_turn as i32 * QUEUE_TICKS > IDLE_MAX {
                    self.ship_leave(fid);
                } else {
                    self.ship_find_dock(fid);
                }
            }
            ship_action::MOORED => {
                let f = self.figures.get_mut(fid).expect("present");
                f.counter -= 1;
                if f.counter > 0 {
                    return;
                }
                f.counter = DEAL_TICKS;
                let staffed = self.buildings.get(dock).is_some_and(|b| b.kind == DOCK && b.workers > 0);
                let busy = self.figures.iter().any(|d| d.kind == DOCKER && d.home == dock);
                if !staffed || (!self.send_docker(dock, fid) && !busy) {
                    self.ship_leave(fid);
                }
            }
            _ => {}
        }
    }

    /// Dockers dock `id` may have out, by staffing.
    fn dockers_allowed(&self, id: BuildingId) -> usize {
        let Some(b) = self.buildings.get(id) else { return 0 };
        let pct = b.workers * 100 / self.workers_needed(b.kind).max(1);
        match pct {
            p if p >= 75 => 3,
            p if p >= 50 => 2,
            p if p > 0 => 1,
            _ => 0,
        }
    }

    /// Sends a docker on the next job for ship `ship` at dock `dock`: unloading a good
    /// we import, else fetching one we export. False if there is no job to do.
    fn send_docker(&mut self, dock: BuildingId, ship: FigureId) -> bool {
        let Some(city) = self.ship_city(ship) else { return false };
        let Some(road) = self.buildings.get(dock).and_then(|b| b.road) else { return false };
        let out = self.figures.iter().filter(|d| d.kind == DOCKER && d.home == dock).count();
        let Some(s) = self.figures.get(ship) else { return false };
        let (capacity, bought, sold) = (s.roam_left, s.amount, s.cargo as i32 * LOAD);
        let yards: Vec<BuildingId> = self.buildings.iter().filter(|b| b.kind == kind::STORAGE_YARD && b.road.is_some()).map(|b| b.id).collect();
        let nearest = |ok: &dyn Fn(BuildingId) -> bool| {
            yards.iter().copied().filter(|&y| ok(y)).min_by_key(|&y| (self.buildings.get(y).map_or(i32::MAX, |b| (b.x - road.0).abs().max((b.y - road.1).abs())), y))
        };
        let mut job = None;
        if sold + LOAD <= capacity {
            job = (1..RESOURCES as u16)
                .filter(|&r| self.can_import(city, r))
                .find_map(|r| nearest(&|y| self.storage_room(y, r) >= LOAD).map(|y| (r, y, docker_action::IMPORTING)));
        }
        if job.is_none() && bought + LOAD <= capacity {
            job = (1..RESOURCES as u16)
                .filter(|&r| self.can_export(city, r))
                .find_map(|r| nearest(&|y| self.stored(y, r) >= LOAD).map(|y| (r, y, docker_action::FETCHING)));
        }
        let Some((r, yard, act)) = job else { return false };
        if out >= self.dockers_allowed(dock) {
            // A job waits for a docker to come back.
            return true;
        }
        let Some(yard_road) = self.buildings.get(yard).and_then(|b| b.road) else { return false };
        let fid = self.figures.spawn(DOCKER, road.0, road.1, Travel::Roads);
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("spawned");
        f.home = dock;
        f.target = yard;
        f.cargo = r;
        f.roam_left = city as i32;
        f.action = act;
        f.amount = if act == docker_action::IMPORTING { LOAD } else { 0 };
        if !f.go_to(map, yard_road) {
            f.dead = true;
            return false;
        }
        let s = self.figures.get_mut(ship).expect("present");
        if act == docker_action::IMPORTING {
            s.cargo += 1;
        } else {
            s.amount += LOAD;
        }
        true
    }

    pub(crate) fn update_docker(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (act, dock, yard, r, amount, city) = (f.action, f.home, f.target, f.cargo, f.amount, f.roam_left as usize);
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("present");
        match f.walk(map) {
            Step::Moving => return,
            Step::Arrived => {}
            Step::Blocked | Step::Lost => {
                f.dead = true;
                return;
            }
        }
        let route = self.trade.cities.get(city).map(|c| c.route as usize);
        let ship = self.moored_ship(dock);
        match act {
            docker_action::IMPORTING => {
                let added = if self.buildings.get(yard).is_some() { self.add_stored(yard, r, amount) } else { 0 };
                let price = self.buy_price(r) * added / LOAD;
                self.treasury -= price;
                self.finance.this_year.imports += price;
                if let Some(rt) = route.and_then(|i| self.trade.routes.get_mut(i)) {
                    rt.traded[r as usize] += added;
                }
                if added == 0
                    && let Some(s) = ship.and_then(|s| self.figures.get_mut(s))
                {
                    // Nothing went in: the load goes back aboard.
                    s.cargo = s.cargo.saturating_sub(1);
                }
                self.figures.get_mut(fid).expect("present").amount = 0;
                self.docker_home(fid);
            }
            docker_action::FETCHING => {
                let took = self.take_stored(yard, r, LOAD);
                if let Some(s) = ship.and_then(|s| self.figures.get_mut(s)) {
                    s.amount -= LOAD - took;
                }
                self.figures.get_mut(fid).expect("present").amount = took;
                self.docker_home(fid);
            }
            _ => {
                if amount > 0 {
                    let price = self.sell_price(r) * amount / LOAD;
                    self.treasury += price;
                    self.finance.this_year.exports += price;
                    if r == crate::economy::resource::LUXURY_GOODS {
                        self.ratings.luxury_exported += amount;
                    }
                    if let Some(rt) = route.and_then(|i| self.trade.routes.get_mut(i)) {
                        rt.traded[r as usize] += amount;
                    }
                }
                self.figures.get_mut(fid).expect("present").dead = true;
            }
        }
    }

    fn docker_home(&mut self, fid: FigureId) {
        let Some(home) = self.figures.get(fid).map(|f| f.home) else { return };
        let road = self.buildings.get(home).and_then(|b| b.road);
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("present");
        f.action = docker_action::RETURNING;
        if !road.is_some_and(|r| f.go_to(map, r)) {
            f.dead = true;
        }
    }
}
