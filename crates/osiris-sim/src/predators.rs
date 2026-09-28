//! Predators, as the original has them. Each predator point on the map holds a pack of
//! the climate's beast: hippos in the humid north, crocodiles in the middle lands and
//! hyenas in the desert, or (as the scenario chooses) asps, lions or scorpions. The
//! pack prowls around a spot near its point; a beast now and then looks about for
//! anyone within reach (anyone but boats and its own kind) and goes after them, and
//! falls on whoever shares its tile, citizen, soldier, invader, criminal or game. A
//! victim trades blows as the fighters do (most of them bare-handed, to no effect)
//! until one of them falls; the beast then goes back to what it was doing. Soldiers
//! and constables go after the beasts like any enemy, and archers and towers shoot
//! them when there is no enemy to shoot. Lost beasts grow back (see `animals`).
//!
//! The land beasts watch, rest and walk to spots around the pack's; crocodiles and
//! hippos idle, and paddle a tile at a time toward the water, going at half pace in
//! open water. A hippo also yawns, and sometimes sets out to hunt at a gallop.

use crate::animals::{HerdRow, NEAR, SPREAD, figure_kind as kind, herd_ground, herd_row};
use crate::figures::{FigureId, Step};
use crate::map::{NEIGHBOURS, terrain};
use crate::military::action::{ATTACK, CORPSE};
use crate::world::World;

pub mod action {
    /// Just put down or grown back: unseen until its wait is up.
    pub const HIDDEN: u16 = 24;
    /// Making up its mind what to do next.
    pub const CHOOSE: u16 = 8;
    /// Walking to a spot around the pack's.
    pub const ROAM: u16 = 9;
    /// On the watch for prey; crocodiles and hippos idle.
    pub const WATCH: u16 = 18;
    /// Resting; crocodiles and hippos paddle a tile toward the water.
    pub const REST: u16 = 19;
    pub const CHASE: u16 = 21;
    /// Heading back to the pack after a chase.
    pub const REGROUP: u16 = 23;
    /// Hippos: out hunting, and yawning.
    pub const HUNT: u16 = 25;
    pub const YAWN: u16 = 26;
}
use action::*;

pub fn is_predator(k: u16) -> bool {
    matches!(k, kind::CROCODILE | kind::HYENA | kind::HIPPO | kind::ASP | kind::LION | kind::SCORPION)
}

fn amphibious(k: u16) -> bool {
    matches!(k, kind::CROCODILE | kind::HIPPO)
}

/// Ticks a fallen beast (or anyone it killed) lies before it is gone.
pub const CORPSE_TICKS: i32 = 128;

/// Figures the beasts leave alone: fishing boats, ships and transports.
const BOATS: [u16; 7] = [25, 77, 78, 92, 93, 100, 101];

use crate::figures::stride;

/// The order in which a beast looks round itself: its own tile, then each square
/// ring out, from the top-left corner along the top, down the right, back along the
/// bottom and up the left.
fn ring_key(dx: i32, dy: i32) -> (i32, i32) {
    let r = dx.abs().max(dy.abs());
    let pos = if dy == -r {
        dx + r
    } else if dx == r {
        2 * r + dy + r
    } else if dy == r {
        4 * r + r - dx
    } else {
        6 * r + r - dy
    };
    (r, pos)
}

/// Frames a beast's picture runs through in each of its states, which also paces
/// what some states do.
fn frames(k: u16, act: u16, deep: bool) -> u8 {
    match k {
        kind::CROCODILE if deep => 11,
        kind::CROCODILE => match act {
            REST => 11,
            ATTACK => 7,
            CORPSE => 8,
            _ => 12,
        },
        kind::HIPPO => match act {
            HUNT => 7,
            CORPSE => 8,
            _ => 12,
        },
        kind::LION => match act {
            CHOOSE => 1,
            WATCH => 10,
            REST => 8,
            CORPSE => 6,
            _ => 11,
        },
        _ => match act {
            CHOOSE | REGROUP => 1,
            ROAM | CHASE => 12,
            _ => 6,
        },
    }
}

/// Whether a figure of kind `k` is one of those the city keeps other accounts of
/// (traders, monument crews, standard bearers...), which the beasts pass by.
fn spared(k: u16) -> bool {
    !crate::religion::hail_strikes(k)
}

/// A picture of a beast: the animation (its key in the figure list), the frame, and
/// whether the animation has a picture for each direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Picture {
    pub anim: &'static str,
    pub frame: u32,
    pub facing: bool,
}

impl World {
    /// Whether the water all round `(x, y)` is open: the tile and its eight neighbours.
    fn open_water(&self, x: i32, y: i32) -> bool {
        (-1..=1).all(|dy| (-1..=1).all(|dx| self.map.terrain_is(x + dx, y + dy, terrain::WATER)))
    }

    /// How a beast looks now, or `None` while it is unseen.
    pub fn predator_picture(&self, fid: FigureId) -> Option<Picture> {
        let f = self.figures.get(fid)?;
        let deep = amphibious(f.kind) && self.open_water(f.x, f.y);
        let frame = f.frame as u32;
        let p = |anim, frame| Some(Picture { anim, frame, facing: true });
        let still = |anim| Some(Picture { anim, frame: 0, facing: true });
        let fallen = |last: i32| f.counter.clamp(0, last) as u32;
        match (f.kind, f.action) {
            (_, HIDDEN) => None,
            (kind::CROCODILE | kind::HIPPO, CORPSE) if deep => Some(Picture { anim: "sink", frame: fallen(22), facing: false }),
            (kind::CROCODILE | kind::HIPPO, CORPSE) => Some(Picture { anim: "death", frame: fallen(7), facing: false }),
            (kind::LION, CORPSE) => p("death", fallen(5)),
            (_, CORPSE) => Some(Picture { anim: "death", frame: fallen(5), facing: false }),
            (kind::CROCODILE, ATTACK) => p("attack", frame),
            (kind::CROCODILE, ROAM | REST | CHASE) => p(if deep { "swim" } else { "walk" }, frame),
            (kind::CROCODILE, REGROUP) => p("walk", frame),
            (kind::CROCODILE, _) if deep => p("swim_idle", frame),
            (kind::CROCODILE, _) => still("walk"),
            (kind::HIPPO, ROAM | REST | CHASE) => p(if deep { "swim" } else { "walk" }, frame),
            (kind::HIPPO, HUNT) => p(if deep { "swim" } else { "hunt" }, frame),
            (kind::HIPPO, CHOOSE | REGROUP) => still(if deep { "swim_idle" } else { "walk" }),
            (kind::HIPPO, YAWN) if deep => p("yawn", frame),
            (kind::HIPPO, YAWN) => still("walk"),
            (kind::HIPPO, _) => p(if deep { "swim_idle" } else { "idle" }, frame),
            (_, ATTACK) => p("attack", frame),
            (_, ROAM | CHASE | REGROUP) => p("walk", frame),
            (_, CHOOSE) => still("walk"),
            (_, WATCH) => p("idle", frame),
            // Lions rest on each picture for two ticks.
            (kind::LION, REST) => p("rest", frame / 2),
            (kind::HYENA, REST) => p("rest", frame),
            (_, REST) => p("idle", frame),
            _ => still("walk"),
        }
    }

    /// A beast's turn.
    pub(crate) fn update_predator(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (k, act) = (f.kind, f.action);
        if act == CORPSE {
            let f = self.figures.get_mut(fid).expect("present");
            f.counter += 1;
            if f.counter >= CORPSE_TICKS {
                f.dead = true;
            }
            return;
        }
        let (Some(herd), Some(row)) = (self.herds.iter().position(|h| h.members.contains(&fid)), herd_row(k)) else {
            self.figures.get_mut(fid).expect("present").dead = true;
            return;
        };
        let deep = row.amphibious && self.open_water(f.x, f.y);
        let n = frames(k, act, deep);
        let f = self.figures.get_mut(fid).expect("present");
        f.frame += 1;
        let wrapped = f.frame >= n;
        if wrapped {
            f.frame = 0;
        }
        match act {
            ATTACK => {
                self.fight(fid);
                let f = self.figures.get_mut(fid).expect("present");
                if f.action == 0 {
                    // The foe is down or gone: back to what it was doing.
                    f.action = if matches!(f.resume, 0 | ATTACK | CORPSE) { CHOOSE } else { f.resume };
                    f.route.clear();
                }
            }
            HIDDEN => {
                f.counter -= 1;
                if f.counter <= 0 {
                    f.action = CHOOSE;
                    f.frame = 0;
                }
            }
            _ if amphibious(k) => self.update_swimmer(fid, herd, &row, deep, wrapped),
            _ => self.update_prowler(fid, herd, &row, wrapped),
        }
        self.pounce_here(fid);
    }

    /// Hyenas, lions, scorpions and asps.
    fn update_prowler(&mut self, fid: FigureId, herd: usize, row: &HerdRow, wrapped: bool) {
        let f = self.figures.get(fid).expect("present");
        let (k, act) = (f.kind, f.action);
        let speed = self.fighter_stats(fid).speed;
        match act {
            CHOOSE => self.settle(fid, k, false),
            WATCH => {
                let f = self.figures.get_mut(fid).expect("present");
                f.counter -= 1;
                if f.counter <= 0 && !self.roam_near_pack(fid, herd, row) {
                    let wait = self.put_off(k);
                    self.figures.get_mut(fid).expect("present").counter = wait;
                }
                self.look_for_prey(fid, row);
            }
            REST => {
                let f = self.figures.get_mut(fid).expect("present");
                f.counter -= 1;
                if f.counter <= 0 {
                    f.action = CHOOSE;
                }
            }
            ROAM => match self.prowl(fid, speed) {
                Step::Arrived => self.settle(fid, k, true),
                Step::Lost => self.figures.get_mut(fid).expect("present").action = CHOOSE,
                _ => {}
            },
            CHASE => {
                let f = self.figures.get_mut(fid).expect("present");
                f.counter -= 1;
                if f.counter <= 0 && wrapped {
                    f.counter = 0;
                    f.look = 0;
                    f.action = REGROUP;
                    return;
                }
                if wrapped {
                    self.eye_prey(fid);
                }
                if matches!(self.prowl(fid, speed), Step::Arrived | Step::Lost) {
                    self.figures.get_mut(fid).expect("present").action = REGROUP;
                }
            }
            REGROUP => {
                if !self.roam_near_pack(fid, herd, row) {
                    let wait = self.put_off(k);
                    let f = self.figures.get_mut(fid).expect("present");
                    f.action = WATCH;
                    f.counter = wait;
                }
            }
            _ => self.figures.get_mut(fid).expect("present").action = CHOOSE,
        }
    }

    /// A land beast settles to watch or to rest, half and half, for a while: a hyena
    /// 150 to 249 ticks, a lion 150 to 349, a scorpion up to 99, an asp 50 to 149 (up
    /// to 99 when it has just walked somewhere).
    fn settle(&mut self, fid: FigureId, k: u16, arrived: bool) {
        self.rng.next();
        let watch = self.rng.short() & 1 != 0;
        self.rng.next();
        let r = self.rng.short();
        let wait = match k {
            kind::HYENA => r % 100 + 150,
            kind::LION => r % 200 + 150,
            kind::ASP if !arrived => r % 100 + 50,
            _ => r % 100,
        };
        let f = self.figures.get_mut(fid).expect("present");
        f.action = if watch { WATCH } else { REST };
        f.counter = wait;
        f.frame = 0;
    }

    /// How long a land beast keeps watch when it finds nowhere to roam to.
    fn put_off(&mut self, k: u16) -> i32 {
        match k {
            kind::HYENA => 100,
            kind::LION => 200,
            _ => {
                self.rng.next();
                self.rng.short() % 100
            }
        }
    }

    /// Crocodiles and hippos.
    fn update_swimmer(&mut self, fid: FigureId, herd: usize, row: &HerdRow, deep: bool, wrapped: bool) {
        let f = self.figures.get(fid).expect("present");
        let (k, act) = (f.kind, f.action);
        let model = self.fighter_stats(fid).speed;
        let speed = if deep { model >> 1 } else { model };
        match act {
            CHOOSE => self.decide(fid),
            WATCH => {
                let f = self.figures.get_mut(fid).expect("present");
                f.counter -= 1;
                let due = f.counter <= 0;
                // A crocodile looks about only as its idling comes round.
                if k == kind::HIPPO || wrapped {
                    self.look_for_prey(fid, row);
                }
                if due && self.figures.get(fid).is_some_and(|f| f.action == WATCH) {
                    if self.roam_near_pack(fid, herd, row) {
                        if k == kind::HIPPO {
                            self.look_for_prey(fid, row);
                        }
                    } else {
                        self.figures.get_mut(fid).expect("present").counter = 100;
                    }
                }
            }
            ROAM | REST => {
                if matches!(self.prowl(fid, speed), Step::Arrived | Step::Lost) {
                    self.decide(fid);
                }
            }
            CHASE => {
                let f = self.figures.get_mut(fid).expect("present");
                f.counter -= 1;
                if f.counter <= 0 {
                    f.look = 0;
                    f.action = REGROUP;
                    return;
                }
                // A crocodile keeps its eye on its prey; a hippo looks up now and then.
                if k == kind::CROCODILE || wrapped {
                    self.eye_prey(fid);
                }
                match self.prowl(fid, speed) {
                    Step::Lost => self.figures.get_mut(fid).expect("present").action = REGROUP,
                    Step::Arrived if !self.eye_prey(fid) => self.figures.get_mut(fid).expect("present").action = REGROUP,
                    _ => {}
                }
            }
            REGROUP => {
                // To a spot around the pack's, or else around its point; with neither to
                // be had the beast is gone.
                let home = (self.herds[herd].x, self.herds[herd].y);
                if !self.roam_near_pack(fid, herd, row) && !self.roam_near(fid, home, row) {
                    self.figures.get_mut(fid).expect("present").dead = true;
                }
            }
            HUNT => {
                let f = self.figures.get_mut(fid).expect("present");
                f.counter -= 1;
                if f.counter <= 0 {
                    f.look = 0;
                    f.action = REGROUP;
                    return;
                }
                let target = f.target;
                let alive = self.figures.get(target).is_some_and(|t| !t.dead && t.action != CORPSE);
                if !alive {
                    self.hunt_for_prey(fid, row);
                } else if wrapped {
                    self.eye_prey(fid);
                }
                if self.figures.get(fid).is_none_or(|f| f.action != HUNT) {
                    return;
                }
                // At a gallop on land.
                match self.prowl(fid, if deep { model } else { model * 2 }) {
                    Step::Lost => {
                        let f = self.figures.get_mut(fid).expect("present");
                        f.look = 0;
                        f.action = REGROUP;
                    }
                    Step::Arrived if !self.eye_prey(fid) => self.hunt_for_prey(fid, row),
                    _ => {}
                }
            }
            YAWN => {
                // The mouth stays open on the middle picture a while.
                let f = self.figures.get_mut(fid).expect("present");
                if f.frame == 6 {
                    f.counter -= 1;
                    if f.counter > 0 {
                        f.frame = 5;
                    }
                }
                if wrapped {
                    self.decide(fid);
                }
            }
            _ => self.figures.get_mut(fid).expect("present").action = CHOOSE,
        }
    }

    /// A crocodile or hippo decides what to do next, by chances that depend on what it
    /// was doing: idle a while, paddle a tile toward the water, and for a hippo yawn,
    /// or (rarely) set out to hunt.
    fn decide(&mut self, fid: FigureId) {
        let f = self.figures.get(fid).expect("present");
        let (k, act) = (f.kind, f.action);
        // Chances in percent of paddling, idling, yawning and hunting.
        let odds: [i32; 4] = match (k, act) {
            (kind::CROCODILE, CHOOSE) => [50, 50, 0, 0],
            (kind::CROCODILE, REST) => [90, 10, 0, 0],
            (kind::CROCODILE, _) => [25, 75, 0, 0],
            (_, CHOOSE | HUNT) => [50, 50, 0, 0],
            (_, REST) => [75, 5, 20, 0],
            (_, YAWN) => [90, 1, 9, 0],
            _ => [29, 70, 0, 1],
        };
        self.rng.next();
        let roll = self.rng.short() % 100;
        let mut sum = 0;
        let pick = odds.iter().position(|&o| {
            sum += o;
            roll < sum
        });
        let next = [REST, WATCH, YAWN, HUNT][pick.unwrap_or(3)];
        self.take_up(fid, next);
    }

    /// A crocodile or hippo takes up what it decided on.
    fn take_up(&mut self, fid: FigureId, next: u16) {
        let f = self.figures.get_mut(fid).expect("present");
        f.action = next;
        f.frame = 0;
        match next {
            WATCH => {
                self.rng.next();
                let wait = self.rng.short() % 100 + 250;
                self.figures.get_mut(fid).expect("present").counter = wait;
            }
            REST => {
                // A tile toward the water: the first watery neighbour round from a random
                // side, else the nearest water within five tiles, else stay.
                let (x, y) = (f.x, f.y);
                self.rng.next();
                let start = (self.rng.short() & 7) as usize;
                let wet = |xx: i32, yy: i32| self.map.terrain_is(xx, yy, terrain::WATER) && !self.map.terrain_is(xx, yy, terrain::DIKE);
                let to = (0..8)
                    .map(|i| NEIGHBOURS[(start + i) % 8])
                    .map(|(dx, dy)| (x + dx, y + dy))
                    .find(|&(xx, yy)| wet(xx, yy))
                    .or_else(|| (2..=5).find_map(|r| crate::animals::ring(r).map(|(dx, dy)| (x + dx, y + dy)).find(|&(xx, yy)| wet(xx, yy))))
                    .unwrap_or((x, y));
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                if !f.go_to(map, to) {
                    f.destination = Some((f.x, f.y));
                }
            }
            YAWN => f.counter = 20,
            _ => {
                // Out to hunt: the prey within twice its sight, if any (and if none, it
                // settles to idle or paddle after all, though for as long as a hunt).
                let row = herd_row(f.kind).expect("a beast");
                self.hunt_for_prey(fid, &row);
                self.figures.get_mut(fid).expect("present").counter = 200;
            }
        }
    }

    /// A hippo on the hunt fixes on the nearest prey within twice its sight; with none,
    /// it decides afresh.
    fn hunt_for_prey(&mut self, fid: FigureId, row: &HerdRow) {
        match self.prey_near(fid, row.scan_radius * 2) {
            Some((t, x, y)) => {
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                f.target = t;
                f.route.clear();
                f.go_to(map, (x, y));
            }
            None => {
                let f = self.figures.get_mut(fid).expect("present");
                f.action = HUNT;
                self.decide(fid);
            }
        }
    }

    /// The beast's eye falls on its prey again: if the prey has moved from where the
    /// beast is heading, it heads for the prey's tile. False if it had nothing to
    /// follow.
    fn eye_prey(&mut self, fid: FigureId) -> bool {
        let f = self.figures.get(fid).expect("present");
        let Some(to) = self.figures.get(f.target).filter(|t| !t.dead).map(|t| (t.x, t.y)) else { return false };
        if f.destination == Some(to) {
            return false;
        }
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("present");
        f.route.clear();
        f.go_to(map, to);
        true
    }

    /// Walks on at the model's `speed` toward the destination, finding a way round
    /// anything in the way.
    fn prowl(&mut self, fid: FigureId, speed: i32) -> Step {
        let tick = self.time.total_ticks;
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("present");
        let Some(to) = f.destination else { return Step::Lost };
        if f.route.is_empty() && !f.moving && (f.x, f.y) != to && !f.go_to(map, to) {
            return Step::Lost;
        }
        f.speed = stride(speed, tick);
        let step = f.walk(map);
        f.speed = 1;
        if step == Step::Blocked {
            f.route.clear();
            return Step::Moving;
        }
        step
    }

    /// Sends the beast to a spot around its pack's.
    fn roam_near_pack(&mut self, fid: FigureId, herd: usize, row: &HerdRow) -> bool {
        let dest = self.herds[herd].dest();
        self.roam_near(fid, dest, row)
    }

    /// Sends the beast to a spot around `c`: the first in the herd's spread, from a
    /// random one on, where its kind may stand, no one does and it can get to.
    /// (Crocodiles and hippos, though, make for the spot of that number right round
    /// `c`, as the original has it.)
    fn roam_near(&mut self, fid: FigureId, c: (i32, i32), row: &HerdRow) -> bool {
        self.rng.next();
        let start = (self.rng.short() % 16) as usize;
        for k in (0..16).map(|i| (start + i) % 16) {
            let (x, y) = (c.0 + SPREAD[k].0, c.1 + SPREAD[k].1);
            if !self.map.contains(x, y) || !herd_ground(row, self.map.terrain.at_or(x, y, 0)) || self.figure_on(x, y) {
                continue;
            }
            let to = if row.amphibious { (c.0 + NEAR[k].0, c.1 + NEAR[k].1) } else { (x, y) };
            let map = &self.map;
            let f = self.figures.get_mut(fid).expect("present");
            f.route.clear();
            if f.go_to(map, to) {
                f.action = ROAM;
                f.frame = 0;
                return true;
            }
        }
        false
    }

    /// Now and then (each time its count comes round to its kind's) a beast on the
    /// watch looks about and goes after the first figure it sees within its sight.
    fn look_for_prey(&mut self, fid: FigureId, row: &HerdRow) {
        let f = self.figures.get_mut(fid).expect("present");
        f.look = f.look.saturating_add(1);
        if (f.look as i32) < row.scan_interval {
            return;
        }
        f.look = 0;
        let Some((t, x, y)) = self.prey_near(fid, row.scan_radius) else { return };
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("present");
        f.target = t;
        f.action = CHASE;
        f.counter = row.scan_radius * 18;
        f.frame = 0;
        f.route.clear();
        f.go_to(map, (x, y));
    }

    /// The first figure a beast sees within `radius` tiles, looking round itself ring
    /// by ring: anyone alive and not a boat or of its own kind (or one of the figures
    /// the city keeps other accounts of).
    fn prey_near(&self, fid: FigureId, radius: i32) -> Option<(FigureId, i32, i32)> {
        let me = self.figures.get(fid)?;
        self.figures
            .iter()
            .filter(|o| o.id != fid && !o.dead && o.kind != me.kind && !matches!(o.action, CORPSE) && !BOATS.contains(&o.kind) && !spared(o.kind))
            .filter(|o| (o.x - me.x).abs() <= radius && (o.y - me.y).abs() <= radius)
            .filter(|o| !(is_predator(o.kind) && o.action == HIDDEN))
            .filter(|o| self.fighter_stats(o.id).class != 0)
            .min_by_key(|o| (ring_key(o.x - me.x, o.y - me.y), o.id))
            .map(|o| (o.id, o.x, o.y))
    }

    /// A beast falls on whoever shares its tile: a citizen, one of the city's armed
    /// men, an invader, a criminal or game (but not while resting or paddling, and not
    /// one already fighting two). It strikes after 12 ticks; the victim, if not already
    /// fighting, turns on it and strikes after 24.
    fn pounce_here(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        if matches!(f.action, ATTACK | CORPSE | HIDDEN | REST) || f.dead {
            return;
        }
        let (x, y, k) = (f.x, f.y, f.kind);
        let victim = self
            .figures
            .iter()
            .filter(|o| (o.x, o.y) == (x, y) && o.id != fid && !o.dead && o.kind != k && o.action != CORPSE && !spared(o.kind))
            .filter(|o| o.kind != crate::defenses::TOWER_SENTRY && !BOATS.contains(&o.kind))
            .filter(|o| matches!(self.fighter_stats(o.id).class, 1 | 2 | 3 | 4 | 6))
            .filter(|o| self.figures.iter().filter(|a| a.action == ATTACK && a.foe == o.id).count() < 2)
            .map(|o| o.id)
            .min();
        let Some(victim) = victim else { return };
        let f = self.figures.get_mut(fid).expect("present");
        f.resume = f.action;
        f.action = ATTACK;
        f.foe = victim;
        f.attack_tick = crate::military::QUICK_BLOW;
        f.route.clear();
        f.moving = false;
        let own = handles_own_fights(self.figures.get(victim).map_or(0, |o| o.kind));
        let o = self.figures.get_mut(victim).expect("present");
        if o.action != ATTACK {
            o.resume = o.action;
            o.action = ATTACK;
            o.attack_tick = 0;
            o.foe = fid;
            // Fighters stop where they are; anyone else stands frozen mid-stride, to
            // walk on if it lives.
            if own {
                o.route.clear();
                o.moving = false;
            }
        }
    }

    /// A figure whose own turn knows nothing of fighting, drawn into a fight by a
    /// beast: it trades blows until one of them falls, then goes on with what it was
    /// doing; fallen, it lies a while and is gone. True if that was its turn.
    pub(crate) fn fight_in_place(&mut self, fid: FigureId) -> bool {
        let Some(f) = self.figures.get(fid) else { return false };
        if !matches!(f.action, ATTACK | CORPSE) || handles_own_fights(f.kind) {
            return false;
        }
        if f.action == CORPSE {
            let f = self.figures.get_mut(fid).expect("present");
            f.counter += 1;
            if f.counter >= CORPSE_TICKS {
                f.dead = true;
            }
            return true;
        }
        self.fight(fid);
        let f = self.figures.get_mut(fid).expect("present");
        if f.action == 0 {
            f.action = f.resume;
        }
        true
    }
}

/// Whether a figure of kind `k` looks after its own fights (and falls its own way).
fn handles_own_fights(k: u16) -> bool {
    use crate::{crime, defenses, invasions, military, navy};
    military::is_soldier(k)
        || invasions::is_invader_kind(k)
        || is_predator(k)
        || navy::is_enemy_ship(k)
        || matches!(k, crime::CONSTABLE | defenses::TOWER_SENTRY | military::STANDARD_BEARER | military::ARROW | military::JAVELIN | navy::WARSHIP | navy::TRANSPORT | crate::tomb_robbers::TOMB_ROBBER)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::figures::Travel;

    #[test]
    fn strides_follow_the_speed_table() {
        let steps = |code| (0..12).map(|t| stride(code, t) as u32).collect::<Vec<_>>();
        assert_eq!(steps(6), vec![1; 12]);
        assert_eq!(&steps(7)[..4], &[1, 1, 1, 2]);
        assert_eq!(&steps(14)[..3], &[2, 2, 3]);
        assert_eq!(steps(5).iter().sum::<u32>(), 9);
        assert_eq!(steps(3).iter().sum::<u32>(), 6);
    }

    #[test]
    fn beasts_look_round_ring_by_ring() {
        assert_eq!(ring_key(0, 0), (0, 0));
        // The first ring from its top-left corner, clockwise.
        let order: Vec<(i32, i32)> = crate::animals::ring(1).collect();
        assert_eq!(order, vec![(-1, -1), (0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0)]);
        let keys: Vec<(i32, i32)> = order.iter().map(|&(dx, dy)| ring_key(dx, dy)).collect();
        assert!(keys.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(crate::animals::ring(3).count(), 24);
        let keys: Vec<(i32, i32)> = crate::animals::ring(3).map(|(dx, dy)| ring_key(dx, dy)).collect();
        assert!(keys.windows(2).all(|w| w[0] < w[1]));
    }

    /// Campaign mission 10 (with the real game data, when present), emptied of its
    /// figures and herds, set to `climate` and the scenario's choice of beast.
    fn world_with(climate: u8, alt: bool) -> Option<World> {
        let data = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../PharaohData");
        if !data.join("mission1.pak").is_file() {
            return None;
        }
        let library = osiris_formats::ImageLibrary::open(&data.join("Data")).ok()?;
        let defs = std::sync::Arc::new(crate::defs::Defs::load(&library).ok()?);
        let model = osiris_formats::Model::parse(&String::from_utf8_lossy(&std::fs::read(data.join("Pharaoh_Model_Normal.txt")).ok()?)).ok()?;
        let mut balance = crate::balance::Balance::from_model(&model);
        balance.set_units(&osiris_formats::model::parse_figures(&String::from_utf8_lossy(&std::fs::read(data.join("Figure_model_normal.txt")).ok()?)).ok()?);
        let balance = std::sync::Arc::new(balance);
        let scenario = osiris_formats::MissionPak::open(&data.join("mission1.pak")).ok()?.scenario(10).ok()?;
        let mut w = World::new(&scenario, defs, balance);
        w.climate = climate;
        w.alt_predator = alt;
        Some(w)
    }

    /// The top-left corner of open dry land fifteen tiles square, and its centre.
    fn open_spot(w: &World) -> (i32, i32) {
        let row = w.predator_row();
        let open = |x: i32, y: i32| {
            (0..15).all(|dy| {
                (0..15).all(|dx| {
                    crate::figures::passable(&w.map, Travel::Land, x + dx, y + dy)
                        && crate::figures::passable(&w.map, Travel::Hostile, x + dx, y + dy)
                        && herd_ground(&row, w.map.terrain.at_or(x + dx, y + dy, 0))
                        && !w.map.terrain_is(x + dx, y + dy, terrain::WATER | terrain::ROAD)
                })
            })
        };
        let (x, y) = (2..w.map.height - 16).find_map(|y| (2..w.map.width - 16).step_by(3).find(|&x| open(x, y)).map(|x| (x, y))).expect("open land");
        (x + 7, y + 7)
    }

    #[test]
    fn the_climate_picks_the_beast() {
        use crate::animals::{predator_row_for, prey_row_for};
        let beast = |c, alt| predator_row_for(c, alt).kind;
        assert_eq!(beast(0, false), kind::HIPPO);
        assert_eq!(beast(0, true), kind::ASP);
        assert_eq!(beast(1, false), kind::CROCODILE);
        assert_eq!(beast(1, true), kind::LION);
        assert_eq!(beast(2, false), kind::HYENA);
        assert_eq!(beast(2, true), kind::SCORPION);
        assert_eq!(prey_row_for(0).kind, kind::BIRDS);
        assert_eq!(prey_row_for(1).kind, kind::ANTELOPE);
        assert_eq!(prey_row_for(2).kind, kind::OSTRICH);
        assert_eq!(predator_row_for(1, false).count, 1);
        assert_eq!(predator_row_for(2, false).count, 7);
    }

    #[test]
    fn packs_are_put_down_whole_and_unseen() {
        let Some(mut w) = world_with(2, false) else { return };
        let (x, y) = open_spot(&w);
        w.create_herds(&[(x, y)], &[(x - 5, y - 5)]);
        assert_eq!(w.herds.len(), 2);
        assert!(w.herds[0].predator && !w.herds[1].predator);
        assert_eq!((w.herds[0].members.len(), w.herds[1].members.len()), (7, 7));
        for &id in &w.herds[0].members {
            let f = w.figures.get(id).unwrap();
            assert_eq!((f.kind, f.action), (kind::HYENA, HIDDEN));
            assert!((f.x - x).abs() <= 1 && (f.y - y).abs() <= 1);
            assert!(w.predator_picture(id).is_none());
        }
        assert!(w.herds[1].members.iter().all(|&id| w.figures.get(id).unwrap().kind == kind::OSTRICH));
    }

    /// Mission 10's world with a pack of one hyena on open land, on the watch and
    /// about to look round, and a homeless man standing six tiles off.
    fn hyena_and_walker() -> Option<(World, FigureId, FigureId)> {
        let mut w = world_with(2, false)?;
        let (x, y) = open_spot(&w);
        w.create_herds(&[(x, y)], &[]);
        let hyena = w.herds[0].members[0];
        for id in w.herds[0].members.split_off(1) {
            w.figures.remove(id);
        }
        w.herds[0].target = 1;
        let walker = w.figures.spawn(crate::people::figure_kind::HOMELESS, x + 6, y, Travel::Land);
        w.figures.get_mut(walker).unwrap().action = 99;
        let f = w.figures.get_mut(hyena).unwrap();
        (f.x, f.y) = (x, y);
        f.action = WATCH;
        f.counter = 500;
        f.look = 15;
        Some((w, hyena, walker))
    }

    #[test]
    fn a_hyena_runs_down_a_walker_and_goes_back_to_its_pack() {
        let Some((mut w, hyena, walker)) = hyena_and_walker() else { return };
        let mut fought = false;
        for _ in 0..600 {
            w.update_predator(hyena);
            if w.figures.get(walker).is_some() {
                fought |= w.figures.get(walker).unwrap().action == ATTACK;
                if w.fight_in_place(walker) && w.figures.get(walker).unwrap().dead {
                    w.figures.remove(walker);
                }
            }
            w.time.total_ticks += 1;
        }
        assert!(fought, "the walker was set upon");
        assert!(w.figures.get(walker).is_none_or(|f| f.action == CORPSE), "a man with no weapon falls");
        let f = w.figures.get(hyena).unwrap();
        assert!(!matches!(f.action, ATTACK | CHASE), "the hyena went back to its pack: {}", f.action);
        assert_eq!(f.damage, 0, "a walker's blows do nothing");
    }

    #[test]
    fn a_walker_who_lives_walks_on() {
        let Some((mut w, hyena, walker)) = hyena_and_walker() else { return };
        let (x, y) = (w.figures.get(hyena).unwrap().x, w.figures.get(hyena).unwrap().y);
        let f = w.figures.get_mut(walker).unwrap();
        (f.x, f.y) = (x, y);
        w.update_predator(hyena);
        assert_eq!(w.figures.get(walker).unwrap().action, ATTACK);
        assert_eq!(w.figures.get(walker).unwrap().resume, 99);
        // The beast is struck down before it can do much.
        w.figures.get_mut(hyena).unwrap().action = CORPSE;
        assert!(w.fight_in_place(walker));
        assert_eq!(w.figures.get(walker).unwrap().action, 99);
    }

    #[test]
    fn packs_grow_back_but_a_wiped_out_hyena_pack_does_not() {
        let Some(mut w) = world_with(2, false) else { return };
        let (x, y) = open_spot(&w);
        w.create_herds(&[(x, y)], &[]);
        for id in w.herds[0].members.split_off(1) {
            w.figures.remove(id);
        }
        for _ in 0..9 {
            w.update_herds();
        }
        assert_eq!(w.herds[0].members.len(), 2, "one hyena back after nine updates");
        for id in std::mem::take(&mut w.herds[0].members) {
            w.figures.remove(id);
        }
        for _ in 0..100 {
            w.update_herds();
        }
        assert!(w.herds[0].members.is_empty());
        // Scorpions come back from none.
        let Some(mut w) = world_with(2, true) else { return };
        w.create_herds(&[(x, y)], &[]);
        for id in std::mem::take(&mut w.herds[0].members) {
            w.figures.remove(id);
        }
        for _ in 0..21 {
            w.update_herds();
        }
        assert_eq!(w.herds[0].members.len(), 1);
    }

    #[test]
    fn soldiers_take_on_beasts() {
        let Some(mut w) = world_with(2, false) else { return };
        let (x, y) = open_spot(&w);
        w.create_herds(&[(x, y)], &[]);
        let beast = w.herds[0].members[0];
        let f = w.figures.get_mut(beast).unwrap();
        f.action = WATCH;
        f.counter = 1000;
        let (bx, by) = (f.x, f.y);
        let soldier = w.figures.spawn(crate::military::INFANTRY, bx + 1, by, Travel::Land);
        w.gather_combatants();
        assert_eq!(w.nearest_foe(false, (bx + 1, by), 1).map(|o| o.0), Some(beast));
        w.engage(soldier, 1);
        let f = w.figures.get(beast).unwrap();
        assert_eq!((f.action, f.foe, f.resume), (ATTACK, soldier, WATCH));
    }
}
