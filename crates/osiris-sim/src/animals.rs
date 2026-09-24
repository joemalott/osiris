//! Wild animals and hunting. The map's herd points hold the wild: at each prey point
//! a herd of the climate's game (birds in the humid north, antelope in the middle
//! lands, ostriches in the desert), at each predator point a pack of its beasts (see
//! `predators`). Twice a day each herd grows back an animal it has lost, and a pack
//! that is whole moves the spot it prowls around. Hunting lodges send hunters after
//! the nearest animal; a kill is carried home as a load of game meat.

use crate::buildings::{BuildingId, kind};
use crate::economy::{LOAD, resource};
use crate::figures::{FigureId, Step, Travel};
use crate::map::terrain;
use crate::world::World;

pub mod figure_kind {
    pub const BIRDS: u16 = 68;
    pub const OSTRICH: u16 = 69;
    pub const ANTELOPE: u16 = 70;
    pub const OSTRICH_HUNTER: u16 = 73;
    pub const CROCODILE: u16 = 82;
    pub const HYENA: u16 = 83;
    pub const HIPPO: u16 = 84;
    pub const ASP: u16 = 102;
    pub const LION: u16 = 103;
    pub const SCORPION: u16 = 104;
    pub const ANTELOPE_HUNTER: u16 = 178;
    pub const BIRDS_HUNTER: u16 = 180;
}

const MAX_HUNTERS: usize = 3;
const STOP_HUNTING_AT: i32 = 500;

/// One row of the original's herd tables, by climate (and for predators by the
/// scenario's choice of beast).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HerdRow {
    pub kind: u16,
    /// Of water and marsh, the ground this kind may also be put down on.
    pub wet: u32,
    /// Animals in a whole herd.
    pub count: i32,
    /// A herd killed off to the last animal never comes back.
    pub needs_one: bool,
    /// Herd updates (two a day) between one animal grown back and the next.
    pub regrow_delay: i32,
    /// Herd updates between moves of the spot a whole pack prowls around, and how far.
    pub roam_delay: i32,
    pub roam_distance: i32,
    /// Crocodiles and hippos go by water as well as land.
    pub amphibious: bool,
    /// Predators: every how many looks one sees prey, and how far.
    pub scan_interval: i32,
    pub scan_radius: i32,
}

#[allow(clippy::too_many_arguments)]
const fn row(kind: u16, wet: u32, count: i32, needs_one: bool, regrow_delay: i32, roam_delay: i32, roam_distance: i32, amphibious: bool, scan_interval: i32, scan_radius: i32) -> HerdRow {
    HerdRow { kind, wet, count, needs_one, regrow_delay, roam_delay, roam_distance, amphibious, scan_interval, scan_radius }
}

/// The game of each climate: humid, normal, arid.
const PREY: [HerdRow; 3] = [
    row(figure_kind::BIRDS, terrain::MARSHLAND, 7, false, 4, 8, 5, false, 0, 0),
    row(figure_kind::ANTELOPE, 0, 7, false, 4, 8, 10, false, 0, 0),
    row(figure_kind::OSTRICH, 0, 7, false, 4, 8, 8, false, 0, 0),
];

const WET: u32 = terrain::WATER | terrain::MARSHLAND;

/// The beasts of each climate: the usual one, and the scenario's other choice.
const PREDATORS: [[HerdRow; 2]; 3] = [
    [row(figure_kind::HIPPO, WET, 3, false, 16, 0, 0, true, 90, 4), row(figure_kind::ASP, 0, 6, false, 16, 30, 2, false, 30, 6)],
    [row(figure_kind::CROCODILE, WET, 1, false, 32, 0, 0, true, 50, 2), row(figure_kind::LION, 0, 4, true, 12, 18, 10, false, 18, 14)],
    [row(figure_kind::HYENA, terrain::DUNE, 7, true, 8, 16, 10, false, 16, 10), row(figure_kind::SCORPION, 0, 2, false, 20, 20, 7, false, 20, 6)],
];

/// The game of climate `climate` (0 humid, 1 normal, 2 arid).
pub fn prey_row_for(climate: u8) -> HerdRow {
    PREY[(climate as usize).min(2)]
}

/// The beast of climate `climate`, or its other one when the scenario chooses `alt`.
pub fn predator_row_for(climate: u8, alt: bool) -> HerdRow {
    PREDATORS[(climate as usize).min(2)][alt as usize]
}

/// The herd row of a kind of animal.
pub fn herd_row(k: u16) -> Option<HerdRow> {
    PREY.iter().chain(PREDATORS.iter().flatten()).find(|r| r.kind == k).copied()
}

/// Where a herd puts its animals down, member by member: right around its point for
/// predators, and in a wider spread for game (the spread is also the herd's layout).
pub(crate) const NEAR: [(i32, i32); 16] =
    [(0, 0), (0, -1), (1, 0), (0, 1), (-1, 0), (1, -1), (1, 1), (-1, 1), (-1, -1), (1, -1), (1, 1), (-1, 1), (-1, -1), (1, 0), (-1, 0), (0, 0)];
pub(crate) const SPREAD: [(i32, i32); 16] =
    [(0, 0), (2, 1), (-1, -1), (1, 1), (1, 0), (-1, 1), (3, 1), (-2, -1), (0, 2), (-4, 0), (-1, 3), (0, 3), (1, 4), (4, 0), (2, 3), (-3, 2)];

/// Whether an animal of `row` may be put down on (or roam to) terrain `t`: never on
/// rock, gardens, rubble, walls, gatehouses, dikes, ore, bridges or cliffs, and on
/// water or marsh only if its row allows.
pub fn herd_ground(row: &HerdRow, t: u32) -> bool {
    const REFUSED: u32 = 0x3038_d022;
    t & REFUSED == 0 && !(t & terrain::WATER != 0 && row.wet & terrain::WATER == 0) && !(t & terrain::MARSHLAND != 0 && row.wet & terrain::MARSHLAND == 0)
}

/// The tiles of the square ring `r` out from a centre, as the original goes round it:
/// the top row left to right, down the right side, the bottom row right to left and
/// up the left side.
pub(crate) fn ring(r: i32) -> impl Iterator<Item = (i32, i32)> {
    let top = (-r..=r).map(move |dx| (dx, -r));
    let right = (-r + 1..=r).map(move |dy| (r, dy));
    let bottom = (-r..r).rev().map(move |dx| (dx, r));
    let left = (-r + 1..r).rev().map(move |dy| (-r, dy));
    top.chain(right).chain(bottom).chain(left)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Herd {
    pub x: i32,
    pub y: i32,
    pub kind: u16,
    /// Animals in the herd when whole.
    pub target: i32,
    pub radius: i32,
    pub members: Vec<FigureId>,
    #[serde(default)]
    pub predator: bool,
    /// The spot a pack prowls around (its point, to begin with).
    #[serde(default)]
    pub dest: Option<(i32, i32)>,
    /// Herd updates counted toward the next animal grown back, and toward the next
    /// move of the pack's spot.
    #[serde(default)]
    pub regrow: i32,
    #[serde(default)]
    pub roam: i32,
}

impl Herd {
    pub fn dest(&self) -> (i32, i32) {
        self.dest.unwrap_or((self.x, self.y))
    }
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
    fn prey_row(&self) -> HerdRow {
        prey_row_for(self.climate)
    }

    /// The beasts of the scenario's climate.
    pub fn predator_row(&self) -> HerdRow {
        predator_row_for(self.climate, self.alt_predator)
    }

    pub fn prey_kind(&self) -> u16 {
        self.prey_row().kind
    }

    fn hunter_kind(&self) -> u16 {
        match self.prey_kind() {
            figure_kind::ANTELOPE => figure_kind::ANTELOPE_HUNTER,
            figure_kind::BIRDS => figure_kind::BIRDS_HUNTER,
            _ => figure_kind::OSTRICH_HUNTER,
        }
    }

    /// Places the scenario's herds: a pack of the climate's beasts at each predator
    /// point, then a herd of its game at each prey point, each whole.
    pub fn create_herds(&mut self, predators: &[(i32, i32)], prey: &[(i32, i32)]) {
        let points: Vec<((i32, i32), HerdRow)> =
            predators.iter().map(|&p| (p, self.predator_row())).chain(prey.iter().map(|&p| (p, self.prey_row()))).collect();
        for ((x, y), row) in points {
            if x <= 0 || !self.map.contains(x, y) {
                continue;
            }
            let predator = row.scan_radius > 0;
            let mut herd = Herd { x, y, kind: row.kind, target: row.count, radius: 8, members: Vec::new(), predator, dest: None, regrow: 0, roam: 0 };
            let offsets = if predator { &NEAR } else { &SPREAD };
            for i in 0..row.count as usize {
                self.rng.next();
                // The first spot from this member's own onward that the kind may stand
                // on, or its own if there is none.
                let slot = (0..16).map(|k| (i + k) % 16).find(|&k| {
                    let (xx, yy) = (x + offsets[k].0, y + offsets[k].1);
                    self.map.contains(xx, yy) && herd_ground(&row, self.map.terrain.at_or(xx, yy, 0))
                });
                let (dx, dy) = offsets[slot.unwrap_or(i % 16)];
                self.rng.next();
                let facing = (self.rng.short() & 7) as u8;
                self.rng.next();
                let wait = if predator { self.rng.short() % 25 * 4 } else { self.rng.short() % 25 };
                if let Some(id) = self.put_down(&row, (x + dx, y + dy), facing, wait) {
                    herd.members.push(id);
                }
            }
            self.herds.push(herd);
        }
    }

    /// An animal of `row` appears at `at`, facing `facing`, and waits `wait` ticks
    /// before it stirs (a predator unseen until then).
    fn put_down(&mut self, row: &HerdRow, at: (i32, i32), facing: u8, wait: i32) -> Option<FigureId> {
        if !self.map.contains(at.0, at.1) {
            return None;
        }
        let predator = row.scan_radius > 0;
        let travel = match (predator, row.amphibious) {
            (false, _) => Travel::Land,
            (true, false) => Travel::Hostile,
            (true, true) => Travel::Amphibious,
        };
        let id = self.figures.spawn(row.kind, at.0, at.1, travel);
        let f = self.figures.get_mut(id)?;
        f.direction = facing;
        f.counter = wait;
        f.action = if predator { crate::predators::action::HIDDEN } else { action::RESTING };
        Some(id)
    }

    /// Whether anyone stands on `(x, y)`.
    pub(crate) fn figure_on(&self, x: i32, y: i32) -> bool {
        self.figures.iter().any(|f| !f.dead && (f.x, f.y) == (x, y))
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

    /// Twice a day (ticks 5 and 29): a herd short of animals counts toward growing one
    /// back beside its point: game every fifth time (every third under Min's oracle),
    /// a pack by its kind, and a hyena or lion pack only while one of it lives. A pack
    /// that isn't growing counts toward moving the spot it prowls around.
    pub(crate) fn update_herds(&mut self) {
        let oracle = self.complex_blessing(crate::temple_complex::OSIRIS, crate::temple_complex::ORACLE);
        for i in 0..self.herds.len() {
            let figures = &self.figures;
            self.herds[i].members.retain(|&id| figures.get(id).is_some_and(|f| !f.dead && f.action != crate::military::action::CORPSE));
            let h = &self.herds[i];
            let Some(row) = herd_row(h.kind) else { continue };
            let (n, predator) = (h.members.len() as i32, h.predator);
            let growing = n < h.target && (!row.needs_one || n > 0);
            if growing {
                let delay = if !predator && oracle { row.regrow_delay / 2 } else { row.regrow_delay };
                self.herds[i].regrow += 1;
                if self.herds[i].regrow > delay {
                    self.herds[i].regrow = 0;
                    self.regrow(i, &row);
                }
            } else if predator {
                self.move_pack(i, &row);
            }
        }
    }

    /// An animal grows back beside the herd's point: at the first of the spread's
    /// spots, from a random one on, where its kind may stand and no one does. (A
    /// pack's newcomer, though, appears at the spot of that number right around the
    /// point: the original checks one table and places by the other.)
    fn regrow(&mut self, i: usize, row: &HerdRow) {
        let (x, y, predator) = (self.herds[i].x, self.herds[i].y, self.herds[i].predator);
        self.rng.next();
        let start = (self.rng.short() & 15) as usize;
        let Some(k) = (0..16).map(|k| (start + k) % 16).find(|&k| {
            let (xx, yy) = (x + SPREAD[k].0, y + SPREAD[k].1);
            self.map.contains(xx, yy) && herd_ground(row, self.map.terrain.at_or(xx, yy, 0)) && !self.figure_on(xx, yy)
        }) else {
            return;
        };
        let (dx, dy) = if predator { NEAR[k] } else { SPREAD[k] };
        self.rng.next();
        let facing = (self.rng.short() & 7) as u8;
        if let Some(id) = self.put_down(row, (x + dx, y + dy), facing, 10) {
            self.herds[i].members.push(id);
        }
    }

    /// A pack moves the spot it prowls around when its count comes round: half its
    /// roaming distance and up to as much again away, from its point when it is there,
    /// otherwise from where it is (seven times in eight) or back to its point.
    fn move_pack(&mut self, i: usize, row: &HerdRow) {
        self.herds[i].roam += 1;
        if self.herds[i].roam <= row.roam_delay {
            return;
        }
        self.herds[i].roam = 0;
        let half = row.roam_distance / 2;
        let d = if half > 0 {
            self.rng.next();
            half + self.rng.short() % half
        } else {
            0
        };
        let home = (self.herds[i].x, self.herds[i].y);
        let dest = self.herds[i].dest();
        let from = if dest == home {
            Some(home)
        } else {
            self.rng.next();
            (self.rng.short() & 7 != 0).then_some(dest)
        };
        let to = match from {
            Some(c) => self.ring_spot(row, c, d),
            None => Some(home),
        };
        if let Some(to) = to.and_then(|t| self.fit_pack(i, t)) {
            self.herds[i].dest = Some(to);
        }
    }

    /// The first tile its kind may stand on `d` tiles out from `c` (or nearer, ring by
    /// ring inward) that it can get to from `c`.
    fn ring_spot(&self, row: &HerdRow, c: (i32, i32), d: i32) -> Option<(i32, i32)> {
        let travel = if row.amphibious { Travel::Amphibious } else { Travel::Hostile };
        // Where it can get to, searched no further than a good way round the ring.
        let reach = d + 16;
        let side = 2 * reach + 1;
        let at = |x: i32, y: i32| ((y - c.1 + reach) * side + x - c.0 + reach) as usize;
        let mut seen = vec![false; (side * side) as usize];
        let mut queue = std::collections::VecDeque::from([c]);
        seen[at(c.0, c.1)] = true;
        while let Some((x, y)) = queue.pop_front() {
            for (dx, dy) in crate::map::NEIGHBOURS {
                let (nx, ny) = (x + dx, y + dy);
                if (nx - c.0).abs() > reach || (ny - c.1).abs() > reach || seen[at(nx, ny)] || !crate::figures::passable(&self.map, travel, nx, ny) {
                    continue;
                }
                seen[at(nx, ny)] = true;
                queue.push_back((nx, ny));
            }
        }
        (0..=d).rev().flat_map(|r| ring(r).map(move |(dx, dy)| (c.0 + dx, c.1 + dy))).find(|&(x, y)| {
            self.map.contains(x, y) && herd_ground(row, self.map.terrain.at_or(x, y, 0)) && seen[at(x, y)]
        })
    }

    /// Where a pack can gather at or near `at` (up to ten tiles off): the first spot,
    /// row by row in widening squares, where each of its animals' places in the herd's
    /// layout is dry, level ground it can walk, with no other herd's animal on it.
    fn fit_pack(&self, i: usize, at: (i32, i32)) -> Option<(i32, i32)> {
        let members = &self.herds[i].members;
        let n = members.len().min(16);
        if n == 0 {
            return Some(at);
        }
        let others: Vec<(i32, i32)> = self.figures.iter().filter(|f| !f.dead && herd_row(f.kind).is_some() && !members.contains(&f.id)).map(|f| (f.x, f.y)).collect();
        let ok = |x: i32, y: i32| {
            SPREAD[..n].iter().all(|&(dx, dy)| {
                let (xx, yy) = (x + dx, y + dy);
                self.map.contains(xx, yy)
                    && !self.map.terrain_is(xx, yy, terrain::WATER | terrain::ELEVATION | terrain::ACCESS_RAMP)
                    && crate::figures::passable(&self.map, Travel::Hostile, xx, yy)
                    && !others.contains(&(xx, yy))
            })
        };
        (0..=10).find_map(|r| (at.1 - r..=at.1 + r).find_map(|y| (at.0 - r..=at.0 + r).find(|&x| ok(x, y)).map(|x| (x, y))))
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
            .filter(|a| is_animal(a.kind) && !a.dead && a.action != crate::military::action::CORPSE && !claimed.contains(&a.id))
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
