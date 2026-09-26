//! Docks, dockers and trade ships.
//!
//! A city on a sea route sends its traders by ship once the player has built a dock
//! (staffed or not). The ship sails in at the river entry and, if the city has anything
//! to trade with us (a good it sells that we import, or one it buys that we export),
//! makes for the nearest free dock. A dock moors one ship at a time; a ship finding
//! them all taken waits off the nearest one, and gives up after 25 days.
//!
//! While a ship is moored the dock sends out dockers, one while under half staffed,
//! two from half and three from three quarters. A docker either carts up to four
//! loads of the ship's goods to a storage yard that will take them, where the city
//! pays for each load that goes in, or fetches up to four loads of a good we export
//! from a yard holding it, where the ship pays for them. Unloading comes first, and
//! each way the goods are taken in turn. A ship carries twelve loads each way. It
//! leaves by the river exit once its dockers are back and there is nothing left to
//! trade, or its hold is full both ways.

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
/// Loads a docker carries at once.
const DOCKER_LOADS: i32 = 4;

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

    /// The ship moored at dock `id`.
    pub fn moored_ship(&self, id: BuildingId) -> Option<FigureId> {
        self.figures.iter().find(|f| f.kind == TRADE_SHIP && f.home == id && f.action == ship_action::MOORED).map(|f| f.id)
    }

    fn ship_city(&self, fid: FigureId) -> Option<usize> {
        self.trader_city(fid)
    }

    /// What a ship from `city` could do here: 2 for each good it sells that we import,
    /// 1 for each it buys that we export.
    fn dock_score(&self, city: usize) -> i32 {
        (1..RESOURCES as u16).map(|r| if self.can_import(city, r) { 2 } else if self.can_export(city, r) { 1 } else { 0 }).sum()
    }

    /// A trade ship from `city` appears at the river entry.
    pub(crate) fn ship_arrives(&mut self, city: usize) -> FigureId {
        let Some((x, y)) = self.river_entry() else { return 0 };
        let fid = self.figures.spawn(TRADE_SHIP, x, y, Travel::Water);
        let capacity = self.defs.figure(TRADE_SHIP).and_then(|d| d.int("max_capacity")).unwrap_or(1200) as i32;
        if let Some(f) = self.figures.get_mut(fid) {
            f.target = city as u32;
            f.roam_left = capacity;
            f.roam_turn = 0;
            f.action = ship_action::QUEUED;
        }
        self.ship_find_dock(fid);
        fid
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

    /// Moves a docker rotation (`export` or import) on a good, then on to the first
    /// good from there that `ok` allows; None (the rotation left where it stopped) if
    /// none does.
    fn step_docker_turn(&mut self, export: bool, ok: impl Fn(&World, u16) -> bool) -> Option<u16> {
        let step = |t: u16| if t as usize + 1 >= RESOURCES { 1 } else { t + 1 };
        let mut r = step(if export { self.trade.docker_export } else { self.trade.docker_import }.max(1));
        let mut found = ok(self, r);
        for _ in 2..RESOURCES {
            if found {
                break;
            }
            r = step(r);
            found = ok(self, r);
        }
        *(if export { &mut self.trade.docker_export } else { &mut self.trade.docker_import }) = r;
        found.then_some(r)
    }

    /// How many loads (of `amount` units offered) of `r` storage yard `yard` would take
    /// from a ship: none if its orders' limit for `r` is reached or wouldn't hold all
    /// of `amount`; otherwise what fits in its free room, whole loads, and no more
    /// than brings the city's stock up to the level it imports to.
    fn yard_import_loads(&self, yard: BuildingId, r: u16, amount: i32) -> i32 {
        use crate::trade::status;
        let Some(b) = self.buildings.get(yard).filter(|b| b.kind == kind::STORAGE_YARD) else { return 0 };
        let full = |more: i32| b.order_cap(r) < self.stored(yard, r) + more;
        if full(0) || full(amount) {
            return 0;
        }
        let free = crate::storage::CAPACITY - self.total_stored(yard);
        let amount = if free <= amount { if free < LOAD { return 0 } else { free } } else { amount };
        let loads = amount / LOAD;
        let target = match self.trade.status[r as usize] {
            status::IMPORT => self.trade.amount[r as usize],
            _ => self.trade_level(r),
        };
        let have = self.yards_stored(r);
        if target <= have + loads * LOAD { ((target - have) / LOAD).max(0) } else { loads }
    }

    /// Where a docker from the dock with road tile `road` takes an import of `r`: of
    /// the staffed yards reached from the entry point and by road from the dock whose
    /// orders take `r` and that would take some of 400 units, the nearest (counting the
    /// difference of the two walks from the entry point) after a penalty of 32, less 8
    /// for each empty space and 4 for each space holding under 400 of `r`.
    fn import_yard(&self, road: (i32, i32), r: u16) -> Option<BuildingId> {
        use crate::storage::order;
        let (network, dock_entry) = self.dock_reach(road);
        let mut best: Option<(i32, BuildingId)> = None;
        for (id, entry) in self.yards_from_entry(None) {
            let Some(b) = self.buildings.get(id) else { continue };
            if !b.road.is_some_and(|rd| network(rd)) || !matches!(b.order(r), order::ACCEPT | order::GET) || self.stored(id, r) >= b.order_cap(r) || self.yard_import_loads(id, r, 4 * LOAD) == 0 {
                continue;
            }
            let penalty = 32 - b.spaces.iter().map(|&(sr, n)| if n == 0 { 8 } else if sr == r && n < crate::storage::SPACE_UNITS { 4 } else { 0 }).sum::<i32>();
            if penalty < 32 {
                let d = (b.x - road.0).abs().max((b.y - road.1).abs()) + (entry - dock_entry).abs() + penalty;
                if best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, id));
                }
            }
        }
        best.map(|(_, id)| id)
    }

    /// Where a docker fetches an export of `r` from: of the yards reached from the
    /// entry point and by road from the dock holding a load of `r`, the nearest
    /// (counted as for imports) after a penalty of 32, less 1 for each space holding
    /// `r`.
    fn export_yard(&self, road: (i32, i32), r: u16) -> Option<BuildingId> {
        let (network, dock_entry) = self.dock_reach(road);
        let entry = crate::figures::route_distances(&self.map, Travel::Land, self.entry_point);
        let w = self.map.width;
        let mut best: Option<(i32, BuildingId)> = None;
        for b in self.buildings.iter().filter(|b| b.kind == kind::STORAGE_YARD) {
            let Some(rd) = b.road.filter(|&rd| network(rd)) else { continue };
            let e = if self.map.contains(rd.0, rd.1) { entry[(rd.1 * w + rd.0) as usize] } else { 0 };
            if e <= 0 || self.stored(b.id, r) < LOAD {
                continue;
            }
            let penalty = 32 - b.spaces.iter().filter(|&&(sr, n)| sr == r && n > 0).count() as i32;
            if penalty < 32 {
                let d = (b.x - road.0).abs().max((b.y - road.1).abs()) + (e - dock_entry).abs() + penalty;
                if best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, b.id));
                }
            }
        }
        best.map(|(_, id)| id)
    }

    /// For a dock with road tile `road`: whether a tile is on its road network, and the
    /// dock's walk from the entry point.
    fn dock_reach(&self, road: (i32, i32)) -> (impl Fn((i32, i32)) -> bool + use<>, i32) {
        let roads = crate::figures::route_distances(&self.map, Travel::Roads, road);
        let entry = crate::figures::route_distances(&self.map, Travel::Land, self.entry_point);
        let w = self.map.width;
        let (mw, mh) = (self.map.width, self.map.height);
        let inside = move |(x, y): (i32, i32)| x >= 0 && y >= 0 && x < mw && y < mh;
        let dock_entry = if inside(road) { entry[(road.1 * w + road.0) as usize] } else { 0 };
        (move |p: (i32, i32)| inside(p) && roads[(p.1 * w + p.0) as usize] > 0, dock_entry)
    }

    /// Sends a docker on the next job for ship `ship` at dock `dock`: unloading up to
    /// four loads of a good we import, else fetching up to four of one we export (the
    /// goods taken in turn, a turn each way). False if there is no job to do.
    fn send_docker(&mut self, dock: BuildingId, ship: FigureId) -> bool {
        let Some(city) = self.ship_city(ship) else { return false };
        let Some(road) = self.buildings.get(dock).and_then(|b| b.road) else { return false };
        let out = self.figures.iter().filter(|d| d.kind == DOCKER && d.home == dock).count();
        let Some(s) = self.figures.get(ship) else { return false };
        let (capacity, bought, sold) = (s.roam_left / LOAD, s.amount / LOAD, s.cargo as i32);
        if out >= self.dockers_allowed(dock) {
            // A job waits for a docker to come back.
            return true;
        }
        let mut job = None;
        if sold < capacity
            && let Some(r) = self.step_docker_turn(false, |w, r| w.can_import(city, r))
            && let Some(yard) = self.import_yard(road, r)
        {
            job = Some((r, yard, docker_action::IMPORTING, (capacity - sold).min(DOCKER_LOADS)));
        }
        if job.is_none()
            && bought < capacity
            && let Some(r) = self.step_docker_turn(true, |w, r| w.can_export(city, r))
            && let Some(yard) = self.export_yard(road, r)
        {
            job = Some((r, yard, docker_action::FETCHING, 0));
        }
        let Some((r, yard, act, loads)) = job else { return false };
        let Some(yard_road) = self.buildings.get(yard).and_then(|b| b.road) else { return false };
        let fid = self.figures.spawn(DOCKER, road.0, road.1, Travel::Roads);
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("spawned");
        f.home = dock;
        f.target = yard;
        f.cargo = r;
        f.roam_left = city as i32;
        f.action = act;
        f.amount = loads * LOAD;
        if !f.go_to(map, yard_road) {
            f.dead = true;
            return false;
        }
        // The loads a docker carries off the ship count as sold until he finds the
        // yard won't take them all.
        self.figures.get_mut(ship).expect("present").cargo += loads as u16;
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
                // The yard takes what it will of 400 units, or of what the route still
                // allows this year, and the city pays for each load.
                let allowed = route.and_then(|i| self.trade.routes.get(i)).map_or(0, |rt| self.trade_limit(city, r) - rt.traded[r as usize]);
                let loads = self.yard_import_loads(yard, r, allowed.clamp(0, DOCKER_LOADS * LOAD)).min(amount / LOAD);
                let mut added = 0;
                for _ in 0..loads {
                    if self.add_stored(yard, r, LOAD) < LOAD {
                        break;
                    }
                    added += 1;
                    let price = self.buy_price(r);
                    self.treasury -= price;
                    self.finance.this_year.imports += price;
                    if let Some(rt) = route.and_then(|i| self.trade.routes.get_mut(i)) {
                        rt.traded[r as usize] += LOAD;
                    }
                }
                if let Some(s) = ship.and_then(|s| self.figures.get_mut(s)) {
                    // What didn't go in goes back aboard.
                    s.cargo = s.cargo.saturating_sub((amount / LOAD - added) as u16);
                }
                self.figures.get_mut(fid).expect("present").amount = 0;
                self.docker_home(fid);
            }
            docker_action::FETCHING => {
                let loads = self.yard_export_loads(yard, r);
                let mut took = 0;
                for _ in 0..loads {
                    let bought = ship.and_then(|s| self.figures.get(s)).map_or(0, |s| s.amount);
                    let capacity = ship.and_then(|s| self.figures.get(s)).map_or(0, |s| s.roam_left);
                    if bought + LOAD > capacity || self.take_stored(yard, r, LOAD) < LOAD {
                        break;
                    }
                    took += LOAD;
                    if let Some(s) = ship.and_then(|s| self.figures.get_mut(s)) {
                        s.amount += LOAD;
                    }
                    let price = self.sell_price(r);
                    self.treasury += price;
                    self.finance.this_year.exports += price;
                    if r == crate::economy::resource::LUXURY_GOODS {
                        self.ratings.luxury_exported += LOAD;
                    }
                    if let Some(rt) = route.and_then(|i| self.trade.routes.get_mut(i)) {
                        rt.traded[r as usize] += LOAD;
                    }
                }
                self.figures.get_mut(fid).expect("present").amount = took;
                self.docker_home(fid);
            }
            _ => self.figures.get_mut(fid).expect("present").dead = true,
        }
    }

    /// Loads of `r` a docker takes from yard `yard` for a ship: what it holds, up to
    /// four, but none that would take the city's stock below the level kept back.
    fn yard_export_loads(&self, yard: BuildingId, r: u16) -> i32 {
        use crate::trade::status;
        if self.buildings.get(yard).is_none_or(|b| b.kind != kind::STORAGE_YARD) {
            return 0;
        }
        let stored = self.stored(yard, r);
        if stored < LOAD {
            return 0;
        }
        let loads = (stored / LOAD).min(DOCKER_LOADS);
        let keep = match self.trade.status[r as usize] {
            status::EXPORT => self.trade.amount[r as usize],
            _ => self.trade_level(r),
        };
        let have = self.yards_stored(r);
        if have - loads * LOAD < keep { ((have - keep) / LOAD).max(0) } else { loads }
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
