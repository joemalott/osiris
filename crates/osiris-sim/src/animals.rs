//! Wild animals and hunting. The map's herd points hold the wild: at each prey point
//! a herd of the climate's game (birds in the humid north, antelope in the middle
//! lands, ostriches in the desert), at each predator point a pack of its beasts (see
//! `predators`). Twice a day each herd grows back an animal it has lost, and a pack
//! that is whole moves the spot it prowls around. Game takes fright at hunters and
//! beasts and runs from them. Hunting lodges send hunters after the nearest animal;
//! they spear it from two tiles off, and a kill is carried home as a load of game
//! meat.

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

pub mod action {
    // Game.
    pub const RESTING: u16 = 1;
    pub const WANDERING: u16 = 2;
    /// Game that has seen a hunter or beast near, or a startled herd-mate, and stands
    /// watching.
    pub const ALERT: u16 = 14;
    /// Game running a tile from what frightened it.
    pub const FLEEING: u16 = 16;
    // Hunters.
    pub const CHASING: u16 = 1;
    pub const CARRYING: u16 = 2;
    /// A new hunter waits a moment in the lodge before looking for prey.
    pub const STARTING: u16 = 3;
    /// Heading home empty-handed, looking again now and then.
    pub const GIVING_UP: u16 = 4;
    /// Standing, about to look for prey.
    pub const LOOKING: u16 = 8;
    /// Butchering the kill.
    pub const BUTCHERING: u16 = 10;
    /// Walking to the kill.
    pub const TO_KILL: u16 = 11;
    /// Throwing his spear (and waiting to see it land).
    pub const THROWING: u16 = 15;
}

/// A hunter's spear or arrow in flight.
pub const HUNTER_SPEAR: u16 = 74;

/// Ticks a killed animal lies before it is gone, if no hunter comes for it.
const CARCASS_TICKS: i32 = 500;
/// The most game a hunter's search looks at: the first 30 animals.
const PREY_LIST: usize = 30;

pub fn is_animal(k: u16) -> bool {
    matches!(k, figure_kind::BIRDS | figure_kind::OSTRICH | figure_kind::ANTELOPE)
}

pub fn is_hunter(k: u16) -> bool {
    matches!(k, figure_kind::OSTRICH_HUNTER | figure_kind::ANTELOPE_HUNTER | figure_kind::BIRDS_HUNTER)
}

/// The heading of a spear thrown from `from` at `to`, one of 32 turning clockwise from
/// north (`y - 1`), as its 32 pictures are.
fn spear_heading(from: (i32, i32), to: (i32, i32)) -> u8 {
    let (dx, dy) = ((to.0 - from.0) as f64, (to.1 - from.1) as f64);
    let turn = dx.atan2(-dy).rem_euclid(std::f64::consts::TAU);
    ((turn / std::f64::consts::TAU * 32.0).round() as u8) % 32
}

/// What game runs from: hunters, crocodiles, hyenas and hippos (the first four of the
/// original's list at 0x5e4dd0).
fn frightens(k: u16) -> bool {
    is_hunter(k) || matches!(k, figure_kind::CROCODILE | figure_kind::HYENA | figure_kind::HIPPO)
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
            (true, true) => Travel::Wading,
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

    /// Game grazes and wanders about its herd's point, as before, and watches for
    /// danger as the original's does (FUN_004eb000, from the ostrich's FUN_0049bd40):
    /// grazing, every 8 ticks, and walking, every 12, it is put on its guard by a
    /// hunter or beast within 6 tiles or a herd-mate on its guard within 3. On its
    /// guard it watches every tick, and a hunter or beast within 2 sends it running a
    /// tile away one time in four; after 20 looks it calms down. Game runs with a
    /// running herd-mate beside it, to where that one runs. A kill lies 500 ticks
    /// unless its hunter comes for it.
    pub(crate) fn update_animal(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        if f.action == crate::military::action::CORPSE {
            let f = self.figures.get_mut(fid).expect("present");
            f.counter += 1;
            if f.counter > CARCASS_TICKS {
                f.dead = true;
            }
            return;
        }
        // A lodge gone or unstaffed lets go of the animals it claimed.
        if f.target != 0 && !self.buildings.get(f.target).is_some_and(|b| b.kind == kind::HUNTING_LODGE && b.workers > 0) {
            self.figures.get_mut(fid).expect("present").target = 0;
        }
        let Some(herd) = self.herds.iter().position(|h| h.members.contains(&fid)) else {
            if let Some(f) = self.figures.get_mut(fid) {
                f.dead = true;
            }
            return;
        };
        let (hx, hy, radius) = (self.herds[herd].x, self.herds[herd].y, self.herds[herd].radius);
        let Some(f) = self.figures.get(fid) else { return };
        match f.action {
            action::ALERT => self.watch(fid),
            action::FLEEING => {
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                match f.walk(map) {
                    Step::Moving => {}
                    Step::Lost => f.dead = true,
                    _ => {
                        f.speed = 1;
                        f.action = action::ALERT;
                        f.counter = 0;
                    }
                }
            }
            action::WANDERING => {
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                if f.walk(map) != Step::Moving {
                    f.action = action::RESTING;
                    f.counter = 60;
                    return;
                }
                if f.anim_tick % 12 == 0 {
                    self.sense_danger(fid);
                }
            }
            _ => {
                let f = self.figures.get_mut(fid).expect("present");
                f.anim_tick += 1;
                if f.anim_tick % 8 == 0 && self.sense_danger(fid) {
                    return;
                }
                let f = self.figures.get_mut(fid).expect("present");
                if f.counter > 0 {
                    f.counter -= 1;
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

    /// The first figure within `r` tiles of `at` (other than `me`) that `want` picks.
    fn figure_near(&self, me: FigureId, at: (i32, i32), r: i32, want: impl Fn(&crate::figures::Figure) -> bool) -> Option<FigureId> {
        self.figures.iter().find(|o| o.id != me && !o.dead && (o.x - at.0).abs() <= r && (o.y - at.1).abs() <= r && want(o)).map(|o| o.id)
    }

    /// Game not on its guard looks about: a running herd-mate beside it, a herd-mate
    /// on its guard within 3 tiles or a hunter or beast within 6 puts it on its guard.
    /// True when it is.
    fn sense_danger(&mut self, fid: FigureId) -> bool {
        let Some(f) = self.figures.get(fid) else { return false };
        let at = (f.x, f.y);
        let mate = |r: i32, act: u16| self.figure_near(fid, at, r, |o| is_animal(o.kind) && o.action == act);
        let alarmed = mate(1, action::FLEEING).or_else(|| mate(3, action::ALERT)).is_some() || self.figure_near(fid, at, 6, |o| frightens(o.kind)).is_some();
        if alarmed {
            let f = self.figures.get_mut(fid).expect("present");
            f.route.clear();
            f.moving = false;
            f.action = action::ALERT;
            f.counter = 0;
        }
        alarmed
    }

    /// Game on its guard: it runs with a running herd-mate beside it, or one time in
    /// four from a hunter or beast within 2 tiles; after 20 looks it calms down.
    fn watch(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let at = (f.x, f.y);
        if let Some(mate) = self.figure_near(fid, at, 1, |o| is_animal(o.kind) && o.action == action::FLEEING) {
            let to = self.figures.get(mate).and_then(|m| m.destination).unwrap_or(at);
            self.rng.next();
            let jx = (self.rng.short() & 3) - 2;
            self.rng.next();
            let jy = (self.rng.short() & 3) - 2;
            self.run_to(fid, (to.0 + jx, to.1 + jy));
            return;
        }
        if let Some(threat) = self.figure_near(fid, at, 2, |o| frightens(o.kind)) {
            self.rng.next();
            let from = self.figures.get(threat).map_or(at, |t| (t.x, t.y));
            if self.rng.short() & 3 == 0 && let Some(to) = self.away_from(at, from) {
                self.run_to(fid, to);
                return;
            }
        }
        let f = self.figures.get_mut(fid).expect("present");
        f.counter += 1;
        if f.counter > 20 {
            f.action = action::RESTING;
            f.counter = 0;
        }
    }

    /// The tile beside `at` directly away from `from`, or the next one round (clockwise)
    /// that game can run to (FUN_004eabf0).
    fn away_from(&self, at: (i32, i32), from: (i32, i32)) -> Option<(i32, i32)> {
        let d = crate::figures::direction_to(from, at).unwrap_or(0) as usize;
        (0..7).map(|k| (d + k) % 8).map(|k| (at.0 + crate::map::NEIGHBOURS[k].0, at.1 + crate::map::NEIGHBOURS[k].1)).find(|&(x, y)| {
            crate::figures::passable(&self.map, Travel::Land, x, y) && !self.map.terrain_is(x, y, terrain::DIKE)
        })
    }

    fn run_to(&mut self, fid: FigureId, to: (i32, i32)) {
        let map = &self.map;
        let Some(f) = self.figures.get_mut(fid) else { return };
        f.moving = false;
        if f.go_to(map, to) {
            f.action = action::FLEEING;
            f.speed = 2;
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
        let travel = if row.amphibious { Travel::Wading } else { Travel::Hostile };
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

    /// Tick 31: lodges send hunters while short of game meat (FUN_00462180 for the
    /// lodge): three at full staff, two from three quarters, one from a quarter, no
    /// more than the room under 500 allows. Each waits in the lodge 0 to 31 ticks
    /// before he comes out to look for prey.
    pub(crate) fn lodge_walkers(&mut self) {
        let lodges: Vec<BuildingId> = self.buildings.iter().filter(|b| b.kind == kind::HUNTING_LODGE).map(|b| b.id).collect();
        let hunter = self.hunter_kind();
        for id in lodges {
            let Some(b) = self.buildings.get(id) else { continue };
            let Some(road) = b.road else { continue };
            if b.workers <= 0 || b.stock[resource::GAMEMEAT as usize] >= STOP_HUNTING_AT {
                continue;
            }
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
                self.rng.next();
                let wait = self.rng.short() & 31;
                if let Some(f) = self.figures.get_mut(fid) {
                    f.home = id;
                    f.action = action::STARTING;
                    f.counter = wait;
                }
            }
        }
    }

    /// The game hunter `fid` at `from` goes for (FUN_004a2320): of the first 30 live
    /// animals not claimed by another lodge, the one a walker from `from` comes to
    /// first (searching north, east, south, west over open ground), if it is no more
    /// than `range` steps away.
    fn game_for(&self, fid: FigureId, from: (i32, i32), range: i32) -> Option<FigureId> {
        let home = self.figures.get(fid)?.home;
        let list: Vec<(FigureId, (i32, i32))> = self
            .figures
            .iter()
            .filter(|a| is_animal(a.kind) && !a.dead && a.action != crate::military::action::CORPSE && (a.target == 0 || a.target == home))
            .take(PREY_LIST)
            .map(|a| (a.id, (a.x, a.y)))
            .collect();
        if list.is_empty() {
            return None;
        }
        let map = &self.map;
        if !map.contains(from.0, from.1) {
            return None;
        }
        let w = map.width;
        let mut dist = vec![-1i32; (w * map.height) as usize];
        dist[(from.1 * w + from.0) as usize] = 0;
        let mut queue = std::collections::VecDeque::from([from]);
        while let Some((x, y)) = queue.pop_front() {
            let d = dist[(y * w + x) as usize];
            if d >= range {
                continue;
            }
            for k in [0, 2, 4, 6] {
                let (nx, ny) = (x + crate::map::NEIGHBOURS[k].0, y + crate::map::NEIGHBOURS[k].1);
                if !map.contains(nx, ny) || dist[(ny * w + nx) as usize] >= 0 || !crate::figures::passable(map, Travel::Land, nx, ny) {
                    continue;
                }
                dist[(ny * w + nx) as usize] = d + 1;
                // Of animals sharing a tile, the last listed.
                if let Some(&(id, _)) = list.iter().rev().find(|a| a.1 == (nx, ny)) {
                    return Some(id);
                }
                queue.push_back((nx, ny));
            }
        }
        None
    }

    /// Hunter `fid` makes for animal `aid`, claiming it for his lodge.
    fn go_after(&mut self, fid: FigureId, aid: FigureId) -> bool {
        let Some(home) = self.figures.get(fid).map(|f| f.home) else { return false };
        let Some(at) = self.figures.get_mut(aid).map(|a| {
            a.target = home;
            (a.x, a.y)
        }) else {
            return false;
        };
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("present");
        f.target = aid;
        f.moving = false;
        if f.go_to(map, at) {
            f.action = action::CHASING;
            true
        } else {
            f.dead = true;
            false
        }
    }

    /// Hunter `fid` heads back to his lodge's road, as `act`; gone if he is there.
    fn hunter_home(&mut self, fid: FigureId, act: u16) {
        let Some(f) = self.figures.get(fid) else { return };
        let road = self.buildings.get(f.home).and_then(|b| b.road);
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("present");
        f.action = act;
        match road {
            Some(r) if r == (f.x, f.y) && act == action::GIVING_UP => f.dead = true,
            Some(r) if f.go_to(map, r) => {}
            _ => f.dead = true,
        }
    }

    /// A hunter, as the original's (FUN_004a2430): out of the lodge he looks for the
    /// nearest game and goes after it, looking again every 12 ticks (and when he
    /// gets where he was going) for game within 6 steps; within 2 tiles of it he stops
    /// and throws his spear (on the 9th tick of the throw). If it strikes, he walks to
    /// the kill, butchers it (18 ticks; birds are just picked up) and carries 100 game
    /// meat home; if it misses, he looks again. Finding nothing, he goes home, on the
    /// way looking for game within 100 steps every 12 ticks.
    pub(crate) fn update_hunter(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (home, act, pos) = (f.home, f.action, (f.x, f.y));
        if self.buildings.get(home).is_none() {
            self.figures.get_mut(fid).expect("present").dead = true;
            return;
        }
        // His animation's 12 frames come round every 12 ticks.
        let f = self.figures.get_mut(fid).expect("present");
        f.look = (f.look + 1) % 12;
        let wrapped = f.look == 0;
        match act {
            action::STARTING => {
                f.counter -= 1;
                if f.counter <= 0 {
                    f.action = action::LOOKING;
                    f.counter = 0;
                }
            }
            action::LOOKING => {
                f.counter -= 1;
                if f.counter > 0 {
                    return;
                }
                match self.game_for(fid, pos, 1000) {
                    Some(aid) => {
                        self.go_after(fid, aid);
                    }
                    None => self.hunter_home(fid, action::GIVING_UP),
                }
            }
            action::CHASING | action::GIVING_UP => {
                let map = &self.map;
                let step = f.walk(map);
                match step {
                    Step::Lost => {
                        f.dead = true;
                        return;
                    }
                    Step::Blocked if act == action::CHASING => {
                        f.route.clear();
                        f.action = action::LOOKING;
                        f.counter = 20;
                        return;
                    }
                    Step::Blocked => {
                        self.hunter_home(fid, action::GIVING_UP);
                        return;
                    }
                    _ => {}
                }
                let arrived = step == Step::Arrived;
                // Mid-step he looks when he reaches the next tile.
                if wrapped && self.figures.get(fid).is_some_and(|f| f.moving) {
                    self.figures.get_mut(fid).expect("present").look = 11;
                    return;
                }
                if act == action::GIVING_UP {
                    if wrapped && let Some(aid) = self.game_for(fid, pos, 100) {
                        self.go_after(fid, aid);
                    } else if arrived {
                        self.figures.get_mut(fid).expect("present").dead = true;
                    }
                    return;
                }
                if !wrapped && !arrived {
                    return;
                }
                match self.game_for(fid, pos, 6) {
                    Some(aid) => {
                        let Some(at) = self.figures.get(aid).map(|a| (a.x, a.y)) else { return };
                        if (at.0 - pos.0).abs().max((at.1 - pos.1).abs()) <= 2 {
                            if let Some(a) = self.figures.get_mut(aid) {
                                a.target = home;
                            }
                            let f = self.figures.get_mut(fid).expect("present");
                            f.target = aid;
                            f.route.clear();
                            f.destination = Some(at);
                            f.direction = crate::figures::direction_to(pos, at).unwrap_or(f.direction);
                            f.action = action::THROWING;
                            f.counter = 0;
                        } else {
                            self.go_after(fid, aid);
                        }
                    }
                    None if arrived => self.hunter_home(fid, action::GIVING_UP),
                    None => {}
                }
            }
            action::THROWING => {
                f.counter += 1;
                if f.counter == 9 {
                    let (target, to) = (f.target, f.destination.unwrap_or(pos));
                    self.figure_sound(fid, 2);
                    let spear = self.figures.spawn(HUNTER_SPEAR, pos.0, pos.1, Travel::Any);
                    if let Some(s) = self.figures.get_mut(spear) {
                        s.home = fid;
                        s.foe = target;
                        s.destination = Some(to);
                        s.direction = crate::figures::direction_to(pos, to).unwrap_or(0);
                        s.look = spear_heading(pos, to);
                    }
                } else if f.counter > 300 {
                    f.action = action::LOOKING;
                }
            }
            action::TO_KILL => {
                let map = &self.map;
                match f.walk(map) {
                    Step::Moving => {}
                    Step::Arrived => {
                        let aid = f.target;
                        let kill = self.figures.get(aid).is_some_and(|a| !a.dead && is_animal(a.kind) && a.action == crate::military::action::CORPSE && (a.x, a.y) == pos);
                        if kill {
                            self.figures.get_mut(aid).expect("present").dead = true;
                            let f = self.figures.get_mut(fid).expect("present");
                            f.action = action::BUTCHERING;
                            f.counter = 0;
                        } else {
                            let f = self.figures.get_mut(fid).expect("present");
                            f.action = action::LOOKING;
                            f.counter = 20;
                        }
                    }
                    Step::Blocked => {
                        f.route.clear();
                        f.action = action::LOOKING;
                        f.counter = 0;
                    }
                    Step::Lost => f.dead = true,
                }
            }
            action::BUTCHERING => {
                f.counter += 1;
                let ticks = if f.kind == figure_kind::BIRDS_HUNTER { 0 } else { 18 };
                if f.counter > ticks {
                    f.cargo = resource::GAMEMEAT;
                    f.amount = LOAD;
                    self.hunter_home(fid, action::CARRYING);
                }
            }
            _ => {
                let map = &self.map;
                match f.walk(map) {
                    Step::Moving => {}
                    Step::Arrived => {
                        f.dead = true;
                        let amount = f.amount;
                        if let Some(b) = self.buildings.get_mut(home) {
                            b.stock[resource::GAMEMEAT as usize] += amount;
                        }
                    }
                    Step::Blocked => self.hunter_home(fid, action::CARRYING),
                    Step::Lost => f.dead = true,
                }
            }
        }
    }

    /// A hunter's spear (FUN_004a2e90) flies a tile every 3 ticks toward where his
    /// prey stood, for at most 104 ticks. Game it meets on the way falls, and the
    /// hunter goes to the kill; landing on none, it sends him looking again.
    pub(crate) fn update_hunter_spear(&mut self, fid: FigureId) {
        let Some(s) = self.figures.get_mut(fid) else { return };
        s.counter += 1;
        let hunter = s.home;
        if s.counter > 104 {
            s.dead = true;
            return;
        }
        if s.counter % 3 == 0
            && let Some((tx, ty)) = s.destination
            && (s.x, s.y) != (tx, ty)
        {
            s.x += (tx - s.x).signum();
            s.y += (ty - s.y).signum();
        }
        let at = (s.x, s.y);
        let landed = s.destination.is_none_or(|d| d == at);
        let hit = self.figures.iter().find(|a| is_animal(a.kind) && !a.dead && a.action != crate::military::action::CORPSE && (a.x, a.y) == at).map(|a| a.id);
        match hit {
            Some(aid) => {
                self.figures.get_mut(fid).expect("present").dead = true;
                if let Some(a) = self.figures.get_mut(aid) {
                    a.action = crate::military::action::CORPSE;
                    a.counter = 0;
                    a.route.clear();
                    a.moving = false;
                    a.speed = 1;
                }
                self.figure_sound(aid, 3);
                let map = &self.map;
                if let Some(h) = self.figures.get_mut(hunter).filter(|h| is_hunter(h.kind) && !h.dead) {
                    h.target = aid;
                    h.action = action::TO_KILL;
                    if !h.go_to(map, at) {
                        h.action = action::LOOKING;
                    }
                }
            }
            None if landed && self.figures.get(fid).is_some_and(|s| s.counter % 3 == 0) => {
                self.figures.get_mut(fid).expect("present").dead = true;
                if let Some(h) = self.figures.get_mut(hunter).filter(|h| is_hunter(h.kind) && h.action == action::THROWING) {
                    h.action = action::LOOKING;
                }
            }
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hunters_spear_game_and_carry_it_home() {
        use crate::world::{Command, Outcome};
        let Some(mut world) = crate::economy::tests::mission(1) else { return };
        for cmd in [Command::Road { start: (79, 75), end: (79, 82) }, Command::Build { kind: kind::HUNTING_LODGE, x: 77, y: 80, x1: 77, y1: 80 }] {
            assert!(matches!(world.apply(&cmd), Outcome::Done { .. }), "{cmd:?}");
        }
        let (mut spears, mut alert, mut butchered) = (0, 0, 0);
        for _ in 0..3000 {
            world.tick();
            spears += world.figures.iter().filter(|f| f.kind == HUNTER_SPEAR && f.counter == 1).count();
            alert += world.figures.iter().filter(|f| is_animal(f.kind) && f.action == action::ALERT).count();
            butchered += world.figures.iter().filter(|f| is_hunter(f.kind) && f.action == action::BUTCHERING && f.counter == 1).count();
        }
        // Hunters threw spears, the ostriches took fright, and what was killed came
        // home a hundred at a time.
        assert!(spears > 3 && alert > 0 && butchered > 0, "{spears} {alert} {butchered}");
        let lodge = world.buildings.iter().find(|b| b.kind == kind::HUNTING_LODGE).expect("lodge");
        let carted: i32 = world.figures.iter().filter(|f| f.kind == crate::economy::CART_PUSHER && f.home == lodge.id).map(|f| f.amount).sum();
        let meat = lodge.stock[resource::GAMEMEAT as usize] + carted;
        assert!(meat >= 100 && meat % 100 == 0 && meat <= butchered as i32 * 100, "{meat}");
    }
}
