//! The shipwright and the fishing fleet.
//!
//! A fishing wharf needs a boat, and boats come from a shipwright. While a staffed
//! wharf with open water has no boat and none is on its way, a shipwright starts one.
//! Its progress grows daily by its staffing: 1 a day under a quarter staffed, 2 under
//! half, 4 under three quarters, 6 below full and 8 when full; a fishing boat takes
//! 100 and no timber. The finished boat is launched on open water beside the yard,
//! waits a moment, and sails to the first wharf still lacking one.
//!
//! At its wharf the boat rests `5 x (102 - staffing %)` ticks between trips (10 when
//! fully staffed), and stays in while the wharf holds 100 fish or more. It sails to the
//! nearest fishing ground (keeping to the last one it used while that is still good,
//! and avoiding grounds another boat is at), fishes for `200 - staffing %` ticks, and
//! brings back 50 fish. The wharf keeps up to 400; its cart pusher takes them to a
//! granary a load at a time. An unstaffed wharf calls its boat home. A boat whose wharf
//! is gone moves to another wharf without a boat, or is lost.

use crate::buildings::BuildingId;
use crate::economy::resource;
use crate::figures::{FigureId, Step, Travel};
use crate::water::{FISHING_WHARF, SHIPWRIGHT};
use crate::world::World;

pub const FISHING_BOAT: u16 = 25;

/// Progress a fishing boat takes when the building says nothing.
const BOAT_COST: i32 = 100;
/// (staffing %, daily progress) steps for building a fishing boat.
const BOAT_PROGRESS: [(i32, i32); 5] = [(1, 1), (25, 2), (50, 4), (75, 6), (100, 8)];
/// Ticks a new boat waits before looking for a wharf.
const LAUNCH_WAIT: i32 = 50;
/// Fish at the wharf that keep its boat in.
const FULL_ENOUGH: i32 = 100;
/// Ticks between retries when a boat can't find its way.
const RETRY_TICKS: i32 = 50;
/// Extra distance counted for a ground another boat is using.
const BUSY_GROUND: i32 = 100;

pub mod action {
    pub const CREATED: u16 = 0;
    pub const GOING_TO_FISH: u16 = 1;
    pub const FISHING: u16 = 2;
    pub const GOING_TO_WHARF: u16 = 3;
    pub const AT_WHARF: u16 = 4;
    pub const RETURNING_WITH_FISH: u16 = 5;
}

impl World {
    fn staffing(&self, id: BuildingId) -> i32 {
        let Some(b) = self.buildings.get(id) else { return 0 };
        let needed = self.workers_needed(b.kind).max(1);
        (b.workers * 100 / needed).clamp(0, 100)
    }

    fn building_int(&self, k: u16, key: &str, default: i32) -> i32 {
        self.defs.building(k).and_then(|d| d.int(key)).map_or(default, |v| v as i32)
    }

    fn boat_int(&self, key: &str, default: i32) -> i32 {
        self.defs.figure(FISHING_BOAT).and_then(|d| d.int(key)).map_or(default, |v| v as i32)
    }

    /// The fishing boat belonging to wharf `id`, if it has one.
    pub fn wharf_boat(&self, id: BuildingId) -> Option<FigureId> {
        self.figures.iter().find(|f| f.kind == FISHING_BOAT && f.home == id && !f.dead && f.action != action::CREATED).map(|f| f.id)
    }

    /// Wharves able to fish that have no boat.
    fn wharves_wanting_boats(&self) -> Vec<BuildingId> {
        self.buildings
            .iter()
            .filter(|b| b.kind == FISHING_WHARF && b.workers > 0 && self.wharf_boat(b.id).is_none())
            .map(|b| b.id)
            .filter(|&id| self.has_open_water(id))
            .collect()
    }

    /// Daily: shipwrights start and build fishing boats, and launch finished ones.
    pub(crate) fn update_shipwrights(&mut self) {
        let yards: Vec<BuildingId> = self.buildings.iter().filter(|b| b.kind == SHIPWRIGHT).map(|b| b.id).collect();
        if yards.is_empty() {
            return;
        }
        let cost = self.building_int(SHIPWRIGHT, "fishingboat_progress_cost", BOAT_COST);
        let mut wanted = self.wharves_wanting_boats().len() as i32;
        // Boats already launched or on the stocks cover some of that.
        wanted -= self.figures.iter().filter(|f| f.kind == FISHING_BOAT && f.action == action::CREATED).count() as i32;
        wanted -= yards.iter().filter(|&&y| self.buildings.get(y).is_some_and(|b| b.progress > 0 && b.boat_kind == 0)).count() as i32;
        for id in yards {
            let pct = self.staffing(id);
            let Some(b) = self.buildings.get_mut(id) else { continue };
            // Busy with a warship or transport.
            if b.boat_kind != 0 {
                continue;
            }
            if b.progress == 0 {
                // A new boat is laid down; work starts the next day.
                if wanted > 0 && b.workers > 0 {
                    b.progress = 1;
                    wanted -= 1;
                }
                continue;
            }
            if b.progress <= cost {
                let gain = BOAT_PROGRESS.iter().rev().find(|&&(p, _)| pct >= p).map_or(0, |&(_, g)| g);
                b.progress = (b.progress + gain).min(cost + 1);
            }
            if b.progress > cost {
                self.launch_boat(id);
            }
        }
    }

    /// Puts a finished boat in the water beside shipwright `id`, if there is a tile
    /// with water all round it and the yard has road access.
    fn launch_boat(&mut self, id: BuildingId) {
        let Some(b) = self.buildings.get(id) else { return };
        if b.road.is_none() {
            return;
        }
        let map = &self.map;
        let open = |x: i32, y: i32| {
            map.terrain_is(x, y, crate::map::terrain::WATER | crate::map::terrain::DEEPWATER)
                && !map.terrain_is(x, y, crate::map::terrain::BUILDING)
                && crate::map::NEIGHBOURS.iter().all(|&(dx, dy)| map.terrain_is(x + dx, y + dy, crate::map::terrain::WATER))
        };
        let Some((x, y)) = crate::buildings::ring(b.x, b.y, b.size).find(|&(x, y)| open(x, y)) else { return };
        let direction = (b.orientation + 3) % 8;
        let fid = self.figures.spawn(FISHING_BOAT, x, y, Travel::Water);
        if let Some(f) = self.figures.get_mut(fid) {
            f.home = id;
            f.action = action::CREATED;
            f.counter = LAUNCH_WAIT;
            f.direction = direction;
        }
        self.buildings.get_mut(id).expect("present").progress = 0;
    }

    /// The nearest wharf without a boat to `from`, for a new or homeless boat.
    fn wharf_for_boat(&self, from: (i32, i32)) -> Option<BuildingId> {
        self.buildings
            .iter()
            .filter(|b| b.kind == FISHING_WHARF && self.wharf_boat(b.id).is_none())
            .min_by_key(|b| ((b.x - from.0).pow(2) + (b.y - from.1).pow(2), b.id))
            .map(|b| b.id)
    }

    /// Sends boat `fid` to its wharf's mooring; false if it can't get there.
    fn boat_to_wharf(&mut self, fid: FigureId, act: u16) -> bool {
        let Some(f) = self.figures.get(fid) else { return false };
        let Some(to) = self.mooring_for(f.home, (f.x, f.y)) else { return false };
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("present");
        f.action = act;
        f.counter = 0;
        f.go_to(map, to)
    }

    /// The fishing ground boat `fid` sails to: the one it used last while it is still
    /// good, else the nearest, avoiding those other boats are using.
    fn fishing_ground(&self, fid: FigureId) -> Option<(i32, i32)> {
        let f = self.figures.get(fid)?;
        let points = &self.water.fishing_points;
        let last = usize::try_from(f.roam_left - 1).ok().and_then(|i| points.get(i).copied());
        if let Some(p) = last {
            return Some(p);
        }
        let busy = |p: (i32, i32)| {
            self.figures.iter().any(|o| {
                o.id != fid && o.kind == FISHING_BOAT && (o.destination == Some(p) || ((o.x - p.0).abs() <= 1 && (o.y - p.1).abs() <= 1))
            })
        };
        points
            .iter()
            .copied()
            .min_by_key(|&p| (p.0 - f.x).abs().max((p.1 - f.y).abs()) + if busy(p) { BUSY_GROUND } else { 0 })
    }

    pub(crate) fn update_fishing_boat(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (act, home, pos) = (f.action, f.home, (f.x, f.y));
        if act == action::CREATED {
            let f = self.figures.get_mut(fid).expect("present");
            f.counter -= 1;
            if f.counter > 0 {
                return;
            }
            f.counter = LAUNCH_WAIT;
            if let Some(w) = self.wharf_for_boat(pos) {
                self.figures.get_mut(fid).expect("present").home = w;
                if !self.boat_to_wharf(fid, action::GOING_TO_WHARF) {
                    let f = self.figures.get_mut(fid).expect("present");
                    f.home = home;
                    f.action = action::CREATED;
                }
            }
            return;
        }
        if self.buildings.get(home).is_none_or(|b| b.kind != FISHING_WHARF) {
            // Its wharf is gone: another without a boat takes it in, or it is lost.
            match self.wharf_for_boat(pos) {
                Some(w) => {
                    self.figures.get_mut(fid).expect("present").home = w;
                    if !self.boat_to_wharf(fid, action::GOING_TO_WHARF) {
                        self.figures.get_mut(fid).expect("present").dead = true;
                    }
                }
                None => self.figures.get_mut(fid).expect("present").dead = true,
            }
            return;
        }
        let pct = self.staffing(home);
        if pct == 0 && matches!(act, action::GOING_TO_FISH | action::FISHING) {
            self.figures.get_mut(fid).expect("present").amount = 0;
            if !self.boat_to_wharf(fid, action::GOING_TO_WHARF) {
                self.figures.get_mut(fid).expect("present").counter = RETRY_TICKS;
            }
            return;
        }
        match act {
            action::AT_WHARF => {
                let f = self.figures.get_mut(fid).expect("present");
                f.counter += 1;
                let base = self.building_int(FISHING_WHARF, "wait_time_base", 102);
                let mult = self.building_int(FISHING_WHARF, "wait_time_multiplier", 5);
                let max = self.building_int(FISHING_WHARF, "max_storage", 400);
                let f = self.figures.get(fid).expect("present");
                let fish = self.buildings.get(home).map_or(0, |b| b.stock[resource::FISH as usize]);
                if pct == 0 || f.counter < mult * (base - pct) || fish >= max.min(FULL_ENOUGH) {
                    return;
                }
                let ground = self.fishing_ground(fid);
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                f.counter = 0;
                if let Some(p) = ground
                    && f.go_to(map, p)
                {
                    f.action = action::GOING_TO_FISH;
                } else {
                    f.roam_left = 0;
                    f.destination = None;
                }
            }
            action::FISHING => {
                let f = self.figures.get_mut(fid).expect("present");
                f.counter -= 1;
                if f.counter > 0 {
                    return;
                }
                let catch = self.boat_int("fish_per_trip", 50);
                self.figures.get_mut(fid).expect("present").amount = catch;
                if !self.boat_to_wharf(fid, action::RETURNING_WITH_FISH) {
                    self.figures.get_mut(fid).expect("present").counter = RETRY_TICKS;
                }
            }
            _ => {
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                if f.counter > 0 && f.route.is_empty() {
                    // Waiting to try its route again.
                    f.counter -= 1;
                    if f.counter == 0 {
                        let back = act != action::GOING_TO_FISH;
                        if back && !self.boat_to_wharf(fid, act) {
                            self.figures.get_mut(fid).expect("present").counter = RETRY_TICKS;
                        }
                    }
                    return;
                }
                match f.walk(map) {
                    Step::Moving => {}
                    Step::Arrived if act == action::GOING_TO_FISH => {
                        let dest = f.destination;
                        f.action = action::FISHING;
                        let base = self.boat_int("fishing_time_base", 200);
                        let mult = self.boat_int("fishing_time_multiplier", 1);
                        let f = self.figures.get_mut(fid).expect("present");
                        f.counter = (base - mult * pct).max(50);
                        f.roam_left = dest.and_then(|d| self.water.fishing_points.iter().position(|&p| p == d)).map_or(0, |i| i as i32 + 1);
                    }
                    Step::Arrived => {
                        let catch = f.amount;
                        f.amount = 0;
                        f.action = action::AT_WHARF;
                        f.counter = 0;
                        f.direction = self.buildings.get(home).map_or(f.direction, |b| (b.orientation * 2 + 4) % 8);
                        let max = self.building_int(FISHING_WHARF, "max_storage", 400);
                        if catch > 0
                            && let Some(b) = self.buildings.get_mut(home)
                        {
                            let stock = &mut b.stock[resource::FISH as usize];
                            *stock += catch.min(max - *stock).max(0);
                        }
                    }
                    Step::Blocked | Step::Lost => {
                        f.route.clear();
                        if act == action::GOING_TO_FISH {
                            // The ground can't be reached: forget it and head home.
                            f.roam_left = 0;
                            if !self.boat_to_wharf(fid, action::GOING_TO_WHARF) {
                                self.figures.get_mut(fid).expect("present").counter = RETRY_TICKS;
                            }
                        } else {
                            f.counter = RETRY_TICKS;
                        }
                    }
                }
            }
        }
    }
}
