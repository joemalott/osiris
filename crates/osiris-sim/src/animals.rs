//! Wild animals and hunting. Each prey herd point on the map holds a herd (ostriches in
//! the desert, antelope in central Egypt, birds in the north) that wanders near its
//! home and slowly regrows. Hunting lodges send hunters after the nearest animal; a
//! kill is carried home as a load of game meat.

use crate::buildings::{BuildingId, kind};
use crate::economy::{LOAD, resource};
use crate::figures::{FigureId, Step, Travel};
use crate::world::World;

pub mod figure_kind {
    pub const BIRDS: u16 = 68;
    pub const OSTRICH: u16 = 69;
    pub const ANTELOPE: u16 = 70;
    pub const OSTRICH_HUNTER: u16 = 73;
    pub const ANTELOPE_HUNTER: u16 = 178;
    pub const BIRDS_HUNTER: u16 = 180;
}

const MAX_HUNTERS: usize = 3;
const STOP_HUNTING_AT: i32 = 500;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Herd {
    pub x: i32,
    pub y: i32,
    pub kind: u16,
    pub target: i32,
    pub radius: i32,
    pub members: Vec<FigureId>,
}

mod action {
    pub const RESTING: u16 = 1;
    pub const WANDERING: u16 = 2;
    pub const CHASING: u16 = 1;
    pub const CARRYING: u16 = 2;
    /// A new hunter waits a moment before looking for prey.
    pub const STARTING: u16 = 3;
    /// Lost its prey: heading home, looking again now and then.
    pub const GIVING_UP: u16 = 4;
}

/// Ticks between a hunter's looks for new prey on its way home empty-handed.
const RESCAN_TICKS: i32 = 12;

pub fn is_animal(k: u16) -> bool {
    matches!(k, figure_kind::BIRDS | figure_kind::OSTRICH | figure_kind::ANTELOPE)
}

pub fn is_hunter(k: u16) -> bool {
    matches!(k, figure_kind::OSTRICH_HUNTER | figure_kind::ANTELOPE_HUNTER | figure_kind::BIRDS_HUNTER)
}

impl World {
    pub fn prey_kind(&self) -> u16 {
        match self.climate {
            0 => figure_kind::ANTELOPE,
            1 => figure_kind::BIRDS,
            _ => figure_kind::OSTRICH,
        }
    }

    fn hunter_kind(&self) -> u16 {
        match self.prey_kind() {
            figure_kind::ANTELOPE => figure_kind::ANTELOPE_HUNTER,
            figure_kind::BIRDS => figure_kind::BIRDS_HUNTER,
            _ => figure_kind::OSTRICH_HUNTER,
        }
    }

    /// Places the scenario's herds. `points` are prey herd points with an optional count.
    pub fn create_herds(&mut self, points: &[(i32, i32, i32)]) {
        let kind = self.prey_kind();
        for &(x, y, count) in points {
            if !self.map.contains(x, y) {
                continue;
            }
            self.rng.next();
            let n = if count > 0 { count } else { 1 + self.rng.short() % 12 };
            let mut herd = Herd { x, y, kind, target: n, radius: 8, members: Vec::new() };
            for _ in 0..n {
                if let Some(id) = self.spawn_animal(&herd) {
                    herd.members.push(id);
                }
            }
            self.herds.push(herd);
        }
    }

    fn spawn_animal(&mut self, herd: &Herd) -> Option<FigureId> {
        let (x, y) = self.free_land_near(herd.x, herd.y, 3)?;
        let id = self.figures.spawn(herd.kind, x, y, Travel::Land);
        self.rng.next();
        let wait = self.rng.byte() & 0x1f;
        if let Some(f) = self.figures.get_mut(id) {
            f.action = action::RESTING;
            f.counter = wait;
        }
        Some(id)
    }

    fn free_land_near(&self, x: i32, y: i32, radius: i32) -> Option<(i32, i32)> {
        for r in 0..=radius {
            for yy in y - r..=y + r {
                for xx in x - r..=x + r {
                    if crate::figures::passable(&self.map, Travel::Land, xx, yy)
                        && !self.map.terrain_is(xx, yy, crate::map::terrain::ROAD)
                    {
                        return Some((xx, yy));
                    }
                }
            }
        }
        None
    }

    pub(crate) fn update_animal(&mut self, fid: FigureId) {
        let Some(herd) = self.herds.iter().position(|h| h.members.contains(&fid)) else {
            if let Some(f) = self.figures.get_mut(fid) {
                f.dead = true;
            }
            return;
        };
        let (hx, hy, radius) = (self.herds[herd].x, self.herds[herd].y, self.herds[herd].radius);
        let Some(f) = self.figures.get(fid) else { return };
        match f.action {
            action::WANDERING => {
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                if f.walk(map) != Step::Moving {
                    f.action = action::RESTING;
                    f.counter = 60;
                }
            }
            _ => {
                if f.counter > 0 {
                    self.figures.get_mut(fid).expect("present").counter -= 1;
                    return;
                }
                self.rng.next();
                let dx = self.rng.byte() % (2 * radius + 1) - radius;
                let dy = self.rng.byte_alt() % (2 * radius + 1) - radius;
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                if f.go_to(map, (hx + dx, hy + dy)) {
                    f.action = action::WANDERING;
                } else {
                    f.counter = 20;
                }
            }
        }
    }

    /// Monthly: herds that lost animals grow back by one (by two under Min's oracle,
    /// which halves the time they take to breed).
    pub(crate) fn regrow_herds(&mut self) {
        let births = if self.complex_blessing(crate::temple_complex::OSIRIS, crate::temple_complex::ORACLE) { 2 } else { 1 };
        for i in 0..self.herds.len() {
            self.herds[i].members.retain(|&id| self.figures.get(id).is_some());
            for _ in 0..births {
                let herd = self.herds[i].clone();
                if !herd.members.is_empty() && (herd.members.len() as i32) < herd.target
                    && let Some(id) = self.spawn_animal(&herd)
                {
                    self.herds[i].members.push(id);
                }
            }
        }
    }

    /// Tick 31: lodges send hunters while short of game meat.
    pub(crate) fn lodge_walkers(&mut self) {
        let lodges: Vec<BuildingId> = self.buildings.iter().filter(|b| b.kind == kind::HUNTING_LODGE).map(|b| b.id).collect();
        let hunter = self.hunter_kind();
        for id in lodges {
            let Some(b) = self.buildings.get(id) else { continue };
            let Some(road) = b.road else { continue };
            if b.workers <= 0 || b.stock[resource::GAMEMEAT as usize] >= STOP_HUNTING_AT {
                continue;
            }
            // Hunters out at once by staffing: three at full staff, two from three
            // quarters, one from a quarter; no more than the room under 500 allows.
            let wanted = match b.workers * 100 / self.workers_needed(b.kind).max(1) {
                p if p >= 100 => MAX_HUNTERS,
                p if p >= 75 => 2,
                p if p >= 25 => 1,
                _ => 0,
            };
            let wanted = wanted.min(((STOP_HUNTING_AT - b.stock[resource::GAMEMEAT as usize]) / 100).max(0) as usize);
            let out = self.figures.iter().filter(|f| f.home == id && is_hunter(f.kind) && !f.dead).count();
            for _ in out..wanted {
                let fid = self.figures.spawn(hunter, road.0, road.1, Travel::Land);
                let wait = self.rng.byte() & 31;
                if let Some(f) = self.figures.get_mut(fid) {
                    f.home = id;
                    f.action = action::STARTING;
                    f.counter = wait;
                }
            }
        }
    }

    /// The nearest live animal no other lodge's hunter is after.
    fn prey_for(&self, fid: FigureId, pos: (i32, i32)) -> Option<(FigureId, i32, i32)> {
        let home = self.figures.get(fid)?.home;
        let claimed: Vec<FigureId> =
            self.figures.iter().filter(|h| is_hunter(h.kind) && h.home != home && !h.dead && h.action == action::CHASING).map(|h| h.target).collect();
        self.figures
            .iter()
            .filter(|a| is_animal(a.kind) && !a.dead && !claimed.contains(&a.id))
            .min_by_key(|a| ((a.x - pos.0).abs().max((a.y - pos.1).abs()), a.id))
            .map(|a| (a.id, a.x, a.y))
    }

    pub(crate) fn update_hunter(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (home, act, pos) = (f.home, f.action, (f.x, f.y));
        if self.buildings.get(home).is_none() {
            self.figures.get_mut(fid).expect("present").dead = true;
            return;
        }
        match act {
            action::STARTING => {
                let f = self.figures.get_mut(fid).expect("present");
                if f.counter > 0 {
                    f.counter -= 1;
                    return;
                }
                // With nothing to hunt the hunter goes straight back in.
                match self.prey_for(fid, pos) {
                    Some((aid, ..)) => {
                        let f = self.figures.get_mut(fid).expect("present");
                        f.action = action::CHASING;
                        f.target = aid;
                    }
                    None => self.figures.get_mut(fid).expect("present").dead = true,
                }
            }
            action::GIVING_UP => {
                let f = self.figures.get_mut(fid).expect("present");
                f.counter -= 1;
                if f.counter <= 0 {
                    f.counter = RESCAN_TICKS;
                    if let Some((aid, ..)) = self.prey_for(fid, pos) {
                        let f = self.figures.get_mut(fid).expect("present");
                        f.action = action::CHASING;
                        f.target = aid;
                        f.route.clear();
                        return;
                    }
                }
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                if f.walk(map) != Step::Moving {
                    f.dead = true;
                }
            }
            action::CHASING => {
                // Re-aim at the nearest animal each time we reach a tile centre.
                let moving = f.moving;
                if !moving {
                    let Some((aid, ax, ay)) = self.prey_for(fid, pos) else {
                        // The prey is gone: head home, looking for more on the way.
                        let road = self.buildings.get(home).and_then(|b| b.road);
                        let map = &self.map;
                        let f = self.figures.get_mut(fid).expect("present");
                        f.action = action::GIVING_UP;
                        f.counter = RESCAN_TICKS;
                        match road {
                            Some(r) if f.go_to(map, r) => {}
                            _ => f.dead = true,
                        }
                        return;
                    };
                    self.figures.get_mut(fid).expect("present").target = aid;
                    if (ax - pos.0).abs() <= 1 && (ay - pos.1).abs() <= 1 {
                        // The kill: the animal falls and the hunter carries it home.
                        if let Some(a) = self.figures.get_mut(aid) {
                            a.dead = true;
                        }
                        let road = self.buildings.get(home).and_then(|b| b.road);
                        let map = &self.map;
                        let f = self.figures.get_mut(fid).expect("present");
                        f.action = action::CARRYING;
                        f.cargo = resource::GAMEMEAT;
                        f.amount = LOAD;
                        match road {
                            Some(r) if f.go_to(map, r) => {}
                            _ => f.dead = true,
                        }
                        return;
                    }
                    let map = &self.map;
                    let f = self.figures.get_mut(fid).expect("present");
                    if !f.go_to(map, (ax, ay)) {
                        f.dead = true;
                        return;
                    }
                }
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                if matches!(f.walk(map), Step::Blocked | Step::Lost) {
                    f.route.clear();
                }
            }
            _ => {
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                match f.walk(map) {
                    Step::Moving => {}
                    Step::Arrived => {
                        f.dead = true;
                        let amount = f.amount;
                        if let Some(b) = self.buildings.get_mut(home) {
                            b.stock[resource::GAMEMEAT as usize] += amount;
                        }
                    }
                    _ => f.dead = true,
                }
            }
        }
    }
}
