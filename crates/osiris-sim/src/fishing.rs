//! The shipwright and the fishing fleet.
//!
//! A fishing wharf needs a boat, and boats come from a shipwright (see `navy.rs`,
//! where the yard builds fishing boats, transports and warships alike). The new boat
//! is launched for the first wharf lacking one, waits 50 ticks and sails to it.
//!
//! The boat, as the original's (FUN_004937a0), stays in while its last catch waits at
//! the wharf, then rests `5 x (102 - staffing %)` ticks (10 when fully staffed; an
//! unstaffed wharf keeps it in). It sails to the nearest fishing ground (FUN_00480c60),
//! and finding another boat already there, to the nearest open water beside it
//! (FUN_00480d60); it fishes for 200 ticks and brings back 100 fish, which the wharf
//! sends straight off in a cart (FUN_00462180 for the wharf). A boat whose wharf is
//! gone moves to another wharf without a boat, or is lost.

use crate::buildings::BuildingId;
use crate::economy::resource;
use crate::figures::{FigureId, Step};
use crate::water::FISHING_WHARF;
use crate::world::World;

pub const FISHING_BOAT: u16 = 25;

/// Ticks a new boat waits before sailing for its wharf, and between its tries.
const LAUNCH_WAIT: i32 = 50;
/// What a boat brings back, and the ticks it fishes.
const CATCH: i32 = 100;
const FISHING_TICKS: i32 = 200;
/// Ticks between retries when a boat can't find its way.
const RETRY_TICKS: i32 = 50;

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

    /// The fishing boat belonging to wharf `id`, if it has one.
    pub fn wharf_boat(&self, id: BuildingId) -> Option<FigureId> {
        self.figures.iter().find(|f| f.kind == FISHING_BOAT && f.home == id && !f.dead && f.action != action::CREATED).map(|f| f.id)
    }

    /// A new boat launched by a shipwright for wharf `home` (see `navy.rs`) waits 50
    /// ticks and then sails for it (the boat's action 8, FUN_004937a0).
    pub(crate) fn fishing_boat_launched(&mut self, fid: FigureId) {
        if let Some(f) = self.figures.get_mut(fid) {
            f.action = action::CREATED;
            f.counter = LAUNCH_WAIT;
        }
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

    /// The fishing ground nearest boat `fid`, counting the larger of the two
    /// distances along the axes (FUN_00480c60).
    fn fishing_ground(&self, fid: FigureId) -> Option<(i32, i32)> {
        let f = self.figures.get(fid)?;
        self.water.fishing_points.iter().copied().min_by_key(|&p| (p.0 - f.x).abs().max((p.1 - f.y).abs()))
    }

    /// Where boat `fid`, come to its fishing ground, fishes: right there, or if
    /// another boat is there first, the nearest open water round it with no one on it,
    /// ring by ring (FUN_00480d60).
    fn fishing_spot(&self, fid: FigureId, at: (i32, i32)) -> Option<(i32, i32)> {
        let taken = |x: i32, y: i32| self.figures.iter().any(|o| o.id != fid && !o.dead && (o.x, o.y) == (x, y));
        if !taken(at.0, at.1) {
            return None;
        }
        let reach = self.map.width.max(self.map.height);
        (1..=reach).find_map(|r| {
            (at.1 - r..=at.1 + r).find_map(|y| (at.0 - r..=at.0 + r).find(|&x| self.map.terrain_is(x, y, crate::map::terrain::WATER) && !taken(x, y)).map(|x| (x, y)))
        })
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
            // The wharf the shipwright built it for, while it still wants a boat.
            let own = self.buildings.get(home).is_some_and(|b| b.kind == FISHING_WHARF) && self.wharf_boat(home).is_none();
            if let Some(w) = if own { Some(home) } else { self.wharf_for_boat(pos) } {
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
        match act {
            action::AT_WHARF => {
                // Its last catch must be on its way before it rests for the next trip.
                let fish = self.buildings.get(home).map_or(0, |b| b.stock[resource::FISH as usize]);
                if pct == 0 || fish > 0 {
                    return;
                }
                let base = self.building_int(FISHING_WHARF, "wait_time_base", 102);
                let mult = self.building_int(FISHING_WHARF, "wait_time_multiplier", 5);
                let f = self.figures.get_mut(fid).expect("present");
                f.counter += 1;
                if f.counter < mult * (base - pct) {
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
                    f.destination = None;
                }
            }
            action::FISHING => {
                let f = self.figures.get_mut(fid).expect("present");
                f.counter -= 1;
                if f.counter > 0 {
                    return;
                }
                self.figures.get_mut(fid).expect("present").amount = CATCH;
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
                        let at = (f.x, f.y);
                        let spot = self.fishing_spot(fid, at);
                        let map = &self.map;
                        let f = self.figures.get_mut(fid).expect("present");
                        match spot {
                            Some(p) if f.go_to(map, p) => {}
                            _ => {
                                f.action = action::FISHING;
                                f.counter = FISHING_TICKS;
                            }
                        }
                    }
                    Step::Arrived => {
                        let catch = f.amount;
                        f.amount = 0;
                        f.action = action::AT_WHARF;
                        f.counter = 0;
                        f.direction = self.buildings.get(home).map_or(f.direction, |b| (b.orientation * 2 + 4) % 8);
                        if catch > 0
                            && let Some(b) = self.buildings.get_mut(home)
                        {
                            b.stock[resource::FISH as usize] += catch;
                        }
                    }
                    Step::Blocked | Step::Lost => {
                        f.route.clear();
                        if act == action::GOING_TO_FISH {
                            // The ground can't be reached: head home.
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
