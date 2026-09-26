//! The navy, and invasions by water. A warship wharf or transport wharf without a
//! ship of its own asks the shipwrights for one; a shipwright takes warships first,
//! then transports, once it has a load of timber, and puts a day's work into the hull
//! for each unit of timber it uses (400 for either). The finished ship sails to its
//! wharf and moors there. Warships put out against enemy ships in the city's waters
//! and shoot at them when in range.
//!
//! An invasion from the sea comes in transports, a ship for every sixteen men. They
//! sail from the sea invasion point toward the nearest landing place, put their men
//! ashore and sail away; a transport sunk before it lands takes its men down with it.

use crate::buildings::BuildingId;
use crate::figures::{FigureId, Step, Travel};
use crate::military::action;
use crate::world::World;

pub const TRANSPORT: u16 = 77;
pub const WARSHIP: u16 = 78;
pub const ENEMY_TRANSPORT: u16 = 92;

const TRANSPORT_WHARF: u16 = crate::water::TRANSPORT_WHARF;
const WARSHIP_WHARF: u16 = crate::water::WARSHIP_WHARF;
const TIMBER: u16 = 20;
/// Work a warship or transport takes.
const SHIP_COST: i32 = 400;
/// Daily progress on a warship and on a transport, by the yard's staffing percentage.
const WARSHIP_PROGRESS: [(i32, i32); 5] = [(1, 1), (25, 2), (50, 3), (75, 4), (100, 5)];
const TRANSPORT_PROGRESS: [(i32, i32); 5] = [(1, 0), (25, 1), (50, 2), (75, 2), (100, 3)];
/// Men an invading transport carries.
const TRANSPORT_LOAD: i32 = 16;
/// Tiles within which warships go after enemy ships.
const HUNT_RANGE: i32 = 40;

/// Ship states.
pub mod ship {
    pub const SAILING_HOME: u16 = 1;
    pub const MOORED: u16 = 2;
    pub const HUNTING: u16 = 3;
    pub const LANDING: u16 = 4;
    pub const LEAVING: u16 = 5;
}

impl World {
    fn yard_staffing(&self, id: BuildingId) -> i32 {
        let Some(b) = self.buildings.get(id) else { return 0 };
        b.workers * 100 / self.workers_needed(b.kind).max(1)
    }

    /// Wharves of kind `k` that are staffed and have no ship of their own, afloat or
    /// on the stocks.
    fn wharves_wanting(&self, k: u16, ship: u16) -> Vec<BuildingId> {
        let building: Vec<u16> = self.buildings.iter().filter(|b| b.kind == crate::water::SHIPWRIGHT && b.boat_kind == ship && b.progress > 0).map(|_| ship).collect();
        let afloat: Vec<BuildingId> = self.figures.iter().filter(|f| f.kind == ship).map(|f| f.home).collect();
        let mut wanting: Vec<BuildingId> = self.buildings.iter().filter(|b| b.kind == k && b.workers > 0 && !afloat.contains(&b.id)).map(|b| b.id).collect();
        wanting.truncate(wanting.len().saturating_sub(building.len()));
        wanting
    }

    /// Daily, before the fishing boats: shipwrights take on and build warships and
    /// transports.
    pub(crate) fn update_navy_yards(&mut self) {
        let yards: Vec<BuildingId> = self.buildings.iter().filter(|b| b.kind == crate::water::SHIPWRIGHT).map(|b| b.id).collect();
        for id in yards {
            let pct = self.yard_staffing(id);
            let Some(b) = self.buildings.get(id) else { continue };
            if b.boat_kind == 0 {
                if b.progress != 0 || b.workers <= 0 || b.stock.get(TIMBER as usize).copied().unwrap_or(0) < crate::economy::LOAD {
                    continue;
                }
                let job = if !self.wharves_wanting(WARSHIP_WHARF, WARSHIP).is_empty() {
                    WARSHIP
                } else if !self.wharves_wanting(TRANSPORT_WHARF, TRANSPORT).is_empty() {
                    TRANSPORT
                } else {
                    continue;
                };
                let b = self.buildings.get_mut(id).expect("present");
                b.boat_kind = job;
                b.progress = 1;
                continue;
            }
            let table = if b.boat_kind == WARSHIP { &WARSHIP_PROGRESS } else { &TRANSPORT_PROGRESS };
            let gain = table.iter().rev().find(|&&(p, _)| pct >= p).map_or(0, |&(_, g)| g);
            let timber = b.stock.get(TIMBER as usize).copied().unwrap_or(0);
            let gain = gain.min(timber);
            let b = self.buildings.get_mut(id).expect("present");
            b.stock[TIMBER as usize] -= gain;
            b.progress += gain;
            if b.progress > SHIP_COST {
                self.launch_ship(id);
            }
        }
    }

    /// Puts a finished warship or transport in the water beside yard `id`, bound for
    /// a wharf that wants it.
    fn launch_ship(&mut self, id: BuildingId) {
        let Some(b) = self.buildings.get(id) else { return };
        let (kind, x0, y0, size, orientation) = (b.boat_kind, b.x, b.y, b.size, b.orientation);
        let wharf_kind = if kind == WARSHIP { WARSHIP_WHARF } else { TRANSPORT_WHARF };
        let map = &self.map;
        let open = |x: i32, y: i32| crate::water::navigable(map, x, y) && !map.terrain_is(x, y, crate::map::terrain::BUILDING);
        // Warships need a tile further out.
        let Some((x, y)) = crate::buildings::ring(x0 - 1, y0 - 1, size + 2).chain(crate::buildings::ring(x0, y0, size)).find(|&(x, y)| open(x, y)) else { return };
        let wharf = self.buildings.iter().filter(|w| w.kind == wharf_kind && w.workers > 0).find(|w| !self.figures.iter().any(|f| f.kind == kind && f.home == w.id)).map(|w| w.id);
        let fid = self.figures.spawn(kind, x, y, Travel::Water);
        if let Some(f) = self.figures.get_mut(fid) {
            f.home = wharf.unwrap_or(0);
            f.action = ship::SAILING_HOME;
            f.direction = (orientation + 3) % 8;
        }
        let b = self.buildings.get_mut(id).expect("present");
        b.progress = 0;
        b.boat_kind = 0;
        self.send_ship_home(fid);
    }

    fn send_ship_home(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (home, pos) = (f.home, (f.x, f.y));
        let berth = self.mooring_for(home, pos);
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("present");
        match berth {
            Some(t) if t == pos => f.action = ship::MOORED,
            Some(t) if f.go_to(map, t) => f.action = ship::SAILING_HOME,
            _ => f.action = ship::MOORED,
        }
    }

    /// A warship's turn: at its berth until an enemy ship comes into the city's
    /// waters, then after it, shooting when in range; home again when none are left.
    pub(crate) fn update_warship(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (act, x, y) = (f.action, f.x, f.y);
        if act == action::CORPSE {
            let f = self.figures.get_mut(fid).expect("present");
            f.counter += 1;
            if f.counter > 200 {
                f.dead = true;
            }
            return;
        }
        // Transports stay at their berth.
        let hunts = self.figures.get(fid).is_some_and(|f| f.kind == WARSHIP);
        let prey = self
            .figures
            .iter()
            .filter(|o| hunts && o.kind == ENEMY_TRANSPORT && o.action != action::CORPSE && !o.dead)
            .map(|o| ((o.x - x).abs().max((o.y - y).abs()), o.id, (o.x, o.y)))
            .filter(|&(d, _, _)| d <= HUNT_RANGE)
            .min();
        let map = &self.map;
        match prey {
            Some((d, target, at)) => {
                let range = self.fighter_stats(fid).missile_range.max(1);
                if d <= range {
                    let f = self.figures.get_mut(fid).expect("present");
                    f.route.clear();
                    f.moving = false;
                    f.action = ship::HUNTING;
                    self.ship_shoot(fid, target);
                    return;
                }
                let f = self.figures.get_mut(fid).expect("present");
                f.action = ship::HUNTING;
                if f.destination != Some(at) && !f.moving {
                    f.go_to(map, at);
                }
                f.walk(map);
            }
            None if act == ship::HUNTING => self.send_ship_home(fid),
            None if act == ship::SAILING_HOME => {
                let f = self.figures.get_mut(fid).expect("present");
                if f.walk(map) != Step::Moving {
                    f.action = ship::MOORED;
                }
            }
            None => {}
        }
    }

    /// A warship fires at an enemy ship when it has reloaded.
    fn ship_shoot(&mut self, fid: FigureId, target: FigureId) {
        let stats = self.fighter_stats(fid);
        let Some(f) = self.figures.get_mut(fid) else { return };
        f.attack_tick += 1;
        if (f.attack_tick as i32) < stats.missile_delay.max(1) {
            return;
        }
        f.attack_tick = 0;
        let armor = self.fighter_stats(target).missile_armor;
        self.figure_sound(fid, 2);
        self.hurt(target, (stats.missile_attack - armor).max(1));
    }

    /// Sends an invading army by sea: transports appear at `spot` on the water, each
    /// with up to sixteen of `men` aboard.
    pub(crate) fn launch_sea_invasion(&mut self, army: usize, nation: u16, men: i32, spot: (i32, i32)) {
        let spot = self.nearest_water(spot).unwrap_or(spot);
        let landing = self.landing_place(spot);
        let mut left = men;
        let mut n = 0;
        while left > 0 {
            let fid = self.figures.spawn(ENEMY_TRANSPORT, spot.0, spot.1, Travel::Water);
            let map = &self.map;
            if let Some(f) = self.figures.get_mut(fid) {
                f.cargo = nation;
                f.amount = left.min(TRANSPORT_LOAD);
                f.formation = 1000 + army as u16;
                f.slot = n;
                f.action = ship::LANDING;
                if let Some(l) = landing {
                    f.go_to(map, l);
                }
                f.destination = landing.or(Some(spot));
            }
            left -= TRANSPORT_LOAD;
            n += 1;
        }
    }

    /// The nearest open water to `p`.
    fn nearest_water(&self, p: (i32, i32)) -> Option<(i32, i32)> {
        (0..30).find_map(|r| {
            (-r..=r).flat_map(move |dy| (-r..=r).map(move |dx| (p.0 + dx, p.1 + dy))).find(|&(x, y)| crate::water::navigable(&self.map, x, y))
        })
    }

    /// Where transports put ashore: open water beside land nearest the scenario's
    /// landing places, or nearest the city.
    fn landing_place(&self, from: (i32, i32)) -> Option<(i32, i32)> {
        let aim = self.invasions.landings.first().copied().unwrap_or(self.entry_point);
        let reach = crate::water::reachable(&self.map, from);
        let w = self.map.width;
        let mut best: Option<((i32, i32), i32)> = None;
        for y in 0..self.map.height {
            for x in 0..w {
                if !reach.get((y * w + x) as usize).copied().unwrap_or(false) {
                    continue;
                }
                if self.shore_near((x, y)).is_none() {
                    continue;
                }
                let d = (x - aim.0).abs() + (y - aim.1).abs();
                if best.is_none_or(|(_, bd)| d < bd) {
                    best = Some(((x, y), d));
                }
            }
        }
        best.map(|(p, _)| p)
    }

    /// An enemy transport sails to its landing place and puts its men ashore, then
    /// sails off the map; sunk, it is gone with them.
    pub(crate) fn update_enemy_transport(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let act = f.action;
        let map = &self.map;
        if act == action::CORPSE {
            let f = self.figures.get_mut(fid).expect("present");
            f.counter += 1;
            if f.counter > 200 {
                f.dead = true;
            }
            return;
        }
        let f = self.figures.get_mut(fid).expect("present");
        let step = f.walk(map);
        if step == Step::Moving {
            return;
        }
        if act == ship::LEAVING {
            f.dead = true;
            return;
        }
        // Landed: the men go ashore beside the ship.
        let (x, y, men, nation, army) = (f.x, f.y, f.amount, f.cargo, f.formation.saturating_sub(1000) as usize);
        f.amount = 0;
        f.action = ship::LEAVING;
        let exit = self.river_exit().or(self.river_entry());
        if let (Some(e), Some(f)) = (exit, self.figures.get_mut(fid)) {
            f.go_to(&self.map, e);
        }
        let Some(shore) = self.shore_near((x, y)) else { return };
        self.put_ashore(army, nation, men, shore);
    }

    /// Dry land within two tiles of a ship, where its men can wade ashore.
    fn shore_near(&self, (x, y): (i32, i32)) -> Option<(i32, i32)> {
        (1..=2).find_map(|r| {
            (-r..=r)
                .flat_map(move |dy| (-r..=r).map(move |dx| (x + dx, y + dy)))
                .find(|&(sx, sy)| crate::figures::passable(&self.map, Travel::Land, sx, sy) && !self.map.terrain_is(sx, sy, crate::map::terrain::WATER))
        })
    }
}
