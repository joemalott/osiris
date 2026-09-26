//! The shipwright, the city's navy, and invasions by water.
//!
//! The shipwright is `case 0x4a` of the original's daily building pass
//! (FUN_00462180, tick 31). With road access, an idle yard first takes the next ship
//! waiting in its repair queue (FUN_00460f70), else the first kind of boat the
//! wharves are short of (FUN_00460fb0): a fishing boat, then a transport, then a
//! warship, counting staffed wharves against the boats they have and the yards
//! already building for them. A transport or warship is taken on only with 100
//! timber in the yard. From the next day the work gains 2, 4, 6, 8 or 10 a day by the
//! yard's staffing (1-24%, 25-49%, 50-74%, 75-99%, 100%; nothing unstaffed), for a
//! repair as for a new hull; a transport or warship gains nothing while the yard
//! holds under 100 timber, and is dropped if a fishing boat is wanted meanwhile. A
//! hull passing 159 is launched on water beside the yard (FUN_004806c0) for the
//! first wharf of its kind still without a boat, the yard giving up 100 timber for a
//! transport or warship and none for a fishing boat; with nowhere to launch, the work
//! is lost. A job whose wharves all have boats is dropped. A repair puts the day's
//! gain back on the hull and uses one timber a day, stalling without timber.
//!
//! A new transport or warship waits 50 ticks, then sails to its wharf. There a
//! warship stands guard (Engage nearby enemies) and a transport keeps out of harm's
//! way (Evade enemies) until given other orders (the warship's action function at
//! 0x4a7030, the transport's at 0x4a4fd0; their numbers are the original's actions).
//! Either goes to the shipwright with the shortest repair queue on its own when its
//! hull falls to 15% or less (FUN_004a45d0), and sinks if its wharf is lost.
//!
//! An invasion from the sea comes in transports, a ship for every sixteen men. They
//! sail from the sea invasion point toward the nearest landing place, put their men
//! ashore and sail away; a transport sunk before it lands takes its men down with it.

use crate::buildings::BuildingId;
use crate::economy::resource::TIMBER;
use crate::figures::{FigureId, Step, Travel};
use crate::fishing::FISHING_BOAT;
use crate::map::{NEIGHBOURS, terrain};
use crate::military::action;
use crate::world::World;

pub const TRANSPORT: u16 = 77;
pub const WARSHIP: u16 = 78;
pub const ENEMY_TRANSPORT: u16 = 92;

const SHIPWRIGHT: u16 = crate::water::SHIPWRIGHT;
const FISHING_WHARF: u16 = crate::water::FISHING_WHARF;
const TRANSPORT_WHARF: u16 = crate::water::TRANSPORT_WHARF;
const WARSHIP_WHARF: u16 = crate::water::WARSHIP_WHARF;
/// A yard's daily gain on a hull or a repair, by its staffing percentage.
const YARD_PROGRESS: [(i32, i32); 5] = [(1, 2), (25, 4), (50, 6), (75, 8), (100, 10)];
/// The work in a hull of any kind: it is launched once its progress passes 159.
pub const HULL_WORK: i32 = 160;
/// Timber a transport or warship needs on hand to be worked on, and takes at launch.
pub const SHIP_TIMBER: i32 = 100;
/// Ticks a new ship, or one just repaired, waits before sailing to its wharf.
const LAUNCH_WAIT: i32 = 50;
/// A ship this badly damaged (hull percentage) heads for repairs by itself.
const REPAIR_AT: i32 = 15;
/// Ticks a sinking ship stays in sight (FUN_004b1d50).
const SINK_TICKS: i32 = 128;
/// Ticks a warship takes to reload: it shoots when its count passes this.
const RELOAD: i32 = 30;
/// How far a warship looks for enemies under Engage nearby enemies, and how far from
/// its post it goes after them (it goes back once 10 tiles out); how far it looks
/// seeking and destroying (the whole map).
const ENGAGE_SEARCH: i32 = 18;
const ENGAGE_LEASH: i32 = 10;
const SEEK_SEARCH: i32 = 228;
/// Ticks between a pursuing warship's fresh routes to its quarry.
const REPATH_TICKS: i32 = 10;
/// Tiles run straight before a warship rams at full force (its model attack; 20
/// otherwise), and the ticks before it can ram again.
const RAM_RUN: i32 = 4;
const RAM_WEAK: i32 = 20;
const RAM_COOLDOWN: i32 = 15;
/// What a ram's force is divided by, by the target's heading (row) and the warship's
/// (column): 1 square on the beam, 5 end on (the table at 0x5dc24c).
const RAM_ANGLE: [[u8; 8]; 8] = [
    [5, 3, 1, 2, 4, 2, 1, 3],
    [3, 5, 3, 1, 2, 4, 2, 1],
    [1, 3, 5, 3, 1, 2, 4, 2],
    [2, 1, 3, 5, 3, 1, 2, 4],
    [4, 2, 1, 3, 5, 3, 1, 2],
    [2, 4, 2, 1, 3, 5, 3, 1],
    [1, 2, 4, 2, 1, 3, 4, 3],
    [3, 1, 2, 4, 3, 1, 3, 5],
];
/// Ticks of pursuit that exhaust a warship's rowers; they then rest this long without
/// moving, and row slowly as long again (FUN_004a5680).
const EXHAUSTING: i32 = 800;
const FATIGUE_TICKS: i32 = 200;
/// A transport evading looks round every 75 ticks for enemy ships within 9 tiles
/// each way, and runs about 10 tiles from them (FUN_004a4a80, FUN_004a4850).
const EVADE_LOOK: i32 = 75;
const EVADE_BOX: i32 = 9;
const EVADE_RUN: i32 = 10;
/// Men an invading transport carries.
const TRANSPORT_LOAD: i32 = 16;

/// Ship actions: the invaders' transports', then the city's ships', numbered as the
/// original's.
pub mod ship {
    pub const LANDING: u16 = 4;
    pub const LEAVING: u16 = 5;
    /// Just launched or repaired: waits, then sails to its wharf.
    pub const LAUNCHED: u16 = 8;
    /// A transport sailing to its wharf (Return to Wharf).
    pub const TRANSPORT_HOME: u16 = 9;
    /// A warship seeking and destroying all enemies.
    pub const SEEK: u16 = 10;
    /// A transport evading enemies.
    pub const EVADE: u16 = 10;
    /// A warship sailing to its wharf (Return to Wharf).
    pub const WARSHIP_HOME: u16 = 11;
    /// A transport sailing to a shipwright for repairs.
    pub const TRANSPORT_TO_YARD: u16 = 11;
    /// A transport picking up a company, and putting it ashore.
    pub const EMBARK: u16 = 12;
    pub const DISEMBARK: u16 = 13;
    /// A warship sailing to a shipwright for repairs.
    pub const WARSHIP_TO_YARD: u16 = 13;
    /// Waiting its turn at the shipwright, and being repaired.
    pub const IN_REPAIR: u16 = 15;
    pub const HOLD: u16 = 20;
    /// A warship guarding its post (Engage nearby enemies).
    pub const ENGAGE: u16 = 23;
    /// A warship going after the enemy it was sent at.
    pub const ATTACK: u16 = 24;
}

/// The orders of the ship window's buttons and hotkeys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ShipOrder {
    Hold,
    Engage,
    Seek,
    Evade,
    Repair,
    Return,
}

/// A city warship's or transport's state beyond its action.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Ship {
    /// Where it holds or guards.
    pub anchor: (i32, i32),
    /// A warship's order before it was sent at an enemy, which it goes back to.
    pub saved: u16,
    /// A warship's rowers: 0 rested, 1 tired, 2 exhausted; ticks of it left; ticks of
    /// pursuit so far.
    pub fatigue: u8,
    pub rest: i32,
    pub chase: i32,
    /// Tiles run in a straight line, and the heading of the last.
    pub straight: i32,
    pub heading: u8,
    pub ram_cool: i32,
    pub reload: i32,
    /// Ticks to its next fresh route to its quarry.
    pub repath: i32,
    /// The shipwright it is going to or waiting at for repairs.
    pub yard: BuildingId,
    /// A transport's company on board, the company it is picking up, and where that
    /// company is to go ashore.
    pub aboard: Option<usize>,
    pub embark: Option<usize>,
    pub landing: Option<(i32, i32)>,
}

fn distance(a: (i32, i32), b: (i32, i32)) -> i32 {
    (a.0 - b.0).abs().max((a.1 - b.1).abs())
}

pub fn is_city_ship(k: u16) -> bool {
    matches!(k, TRANSPORT | WARSHIP)
}

fn wharf_kind(ship: u16) -> u16 {
    match ship {
        FISHING_BOAT => FISHING_WHARF,
        TRANSPORT => TRANSPORT_WHARF,
        _ => WARSHIP_WHARF,
    }
}

impl World {
    fn yard_staffing(&self, id: BuildingId) -> i32 {
        let Some(b) = self.buildings.get(id) else { return 0 };
        b.workers * 100 / self.workers_needed(b.kind).max(1)
    }

    /// Whether wharf `w` has a boat of kind `ship`, afloat and not sinking.
    fn wharf_has_boat(&self, w: BuildingId, ship: u16) -> bool {
        self.figures.iter().any(|f| f.kind == ship && f.home == w && !f.dead && f.action != action::CORPSE)
    }

    /// The kind of boat the wharves are short of, fishing boats first, then
    /// transports, then warships: staffed wharves without one, less the staffed yards
    /// already building one (FUN_00460fb0).
    pub fn ship_wanted(&self) -> Option<u16> {
        [FISHING_BOAT, TRANSPORT, WARSHIP].into_iter().find(|&ship| {
            let wharf = wharf_kind(ship);
            let lacking = self.buildings.iter().filter(|w| w.kind == wharf && w.workers > 0 && !self.wharf_has_boat(w.id, ship)).count();
            let building = self.buildings.iter().filter(|y| y.kind == SHIPWRIGHT && y.workers > 0 && y.boat_kind == ship).count();
            lacking > building
        })
    }

    /// The first wharf for boat kind `ship` without one, staffed or not
    /// (FUN_00460ea0).
    fn free_wharf(&self, ship: u16) -> Option<BuildingId> {
        let wharf = wharf_kind(ship);
        self.buildings.iter().filter(|w| w.kind == wharf && !self.wharf_has_boat(w.id, ship)).map(|w| w.id).min()
    }

    /// Daily (tick 31): the shipwrights repair ships and build boats.
    pub(crate) fn update_shipwrights(&mut self) {
        let yards: Vec<BuildingId> = self.buildings.iter().filter(|b| b.kind == SHIPWRIGHT).map(|b| b.id).collect();
        for id in yards {
            let pct = self.yard_staffing(id);
            let Some(b) = self.buildings.get(id) else { continue };
            if b.road.is_none() {
                continue;
            }
            let timber = b.stock.get(TIMBER as usize).copied().unwrap_or(0);
            if b.boat_kind == 0 && b.repairing == 0 {
                // An idle yard: a ship waiting for repairs first, else a new boat.
                let wanted = if b.repair_queue.is_empty() { self.ship_wanted() } else { None };
                let b = self.buildings.get_mut(id).expect("present");
                if !b.repair_queue.is_empty() {
                    b.repairing = b.repair_queue.remove(0);
                } else if let Some(ship) = wanted
                    && (ship == FISHING_BOAT || timber >= SHIP_TIMBER)
                {
                    b.boat_kind = ship;
                    b.progress = 0;
                }
                continue;
            }
            let gain = YARD_PROGRESS.iter().rev().find(|&&(p, _)| pct >= p).map_or(0, |&(_, g)| g);
            if b.repairing != 0 {
                self.repair_step(id, gain, timber);
                continue;
            }
            let ship = b.boat_kind;
            let Some(wharf) = self.free_wharf(ship) else {
                let b = self.buildings.get_mut(id).expect("present");
                b.boat_kind = 0;
                b.progress = 0;
                continue;
            };
            let mut gain = gain;
            if ship != FISHING_BOAT && timber < SHIP_TIMBER {
                gain = 0;
                if self.ship_wanted() == Some(FISHING_BOAT) {
                    let b = self.buildings.get_mut(id).expect("present");
                    b.boat_kind = 0;
                    b.progress = 0;
                    continue;
                }
            }
            let b = self.buildings.get_mut(id).expect("present");
            b.progress += gain;
            if b.progress >= HULL_WORK {
                b.progress = 0;
                self.launch_ship(id, ship, wharf);
            }
        }
    }

    /// A day's repair on the ship yard `id` is working on.
    fn repair_step(&mut self, id: BuildingId, gain: i32, timber: i32) {
        let Some(fid) = self.buildings.get(id).map(|b| b.repairing) else { return };
        let afloat = self.figures.get(fid).is_some_and(|f| is_city_ship(f.kind) && !f.dead && f.action != action::CORPSE && f.action == ship::IN_REPAIR);
        let b = self.buildings.get_mut(id).expect("present");
        if !afloat {
            b.repairing = 0;
            return;
        }
        if timber > 0 {
            b.stock[TIMBER as usize] -= 1;
            if let Some(f) = self.figures.get_mut(fid) {
                f.damage = (f.damage - gain).max(0);
            }
        }
        if self.figures.get(fid).is_some_and(|f| f.damage <= 0) {
            self.buildings.get_mut(id).expect("present").repairing = 0;
            let f = self.figures.get_mut(fid).expect("present");
            f.action = ship::LAUNCHED;
            f.counter = 0;
            if let Some(s) = &mut f.ship {
                s.yard = 0;
            }
        }
    }

    /// Puts a finished boat in the water beside yard `id` for `wharf`: on the first
    /// tile round the yard that is water, has no building, and has water all round.
    fn launch_ship(&mut self, id: BuildingId, kind: u16, wharf: BuildingId) {
        let Some(b) = self.buildings.get(id) else { return };
        let (x0, y0, size, orientation) = (b.x, b.y, b.size, b.orientation);
        let map = &self.map;
        let open = |x: i32, y: i32| {
            map.terrain_is(x, y, terrain::WATER | terrain::DEEPWATER) && !map.terrain_is(x, y, terrain::BUILDING) && NEIGHBOURS.iter().all(|&(dx, dy)| map.terrain_is(x + dx, y + dy, terrain::WATER))
        };
        let Some((x, y)) = crate::buildings::ring(x0, y0, size).find(|&(x, y)| open(x, y)) else { return };
        let fid = self.figures.spawn(kind, x, y, Travel::Water);
        let Some(f) = self.figures.get_mut(fid) else { return };
        f.home = wharf;
        f.direction = (orientation + 3) % 8;
        if kind == FISHING_BOAT {
            self.fishing_boat_launched(fid);
            return;
        }
        f.action = ship::LAUNCHED;
        f.counter = 0;
        f.speed = 2;
        f.ship = Some(Box::default());
        let b = self.buildings.get_mut(id).expect("present");
        b.stock[TIMBER as usize] -= SHIP_TIMBER;
        b.boat_kind = 0;
    }

    fn ship_state(&mut self, fid: FigureId) -> Option<&mut Ship> {
        self.figures.get_mut(fid).and_then(|f| f.ship.as_deref_mut())
    }

    /// The share of its hull a ship has left, in percent.
    pub fn hull_percent(&self, fid: FigureId) -> i32 {
        let hp = self.fighter_stats(fid).hp.max(1);
        self.figures.get(fid).map_or(0, |f| (hp - f.damage.max(0)).max(0) * 100 / hp)
    }

    /// Whether ship `fid` is moored at its wharf.
    pub fn ship_docked(&self, fid: FigureId) -> bool {
        let Some(f) = self.figures.get(fid) else { return false };
        !f.moving && self.buildings.get(f.home).is_some_and(|w| crate::water::mooring_tiles(w).contains(&(f.x, f.y)))
    }

    /// The transport a company is aboard, if any.
    pub fn company_ship(&self, company: usize) -> Option<FigureId> {
        self.figures.iter().find(|f| f.kind == TRANSPORT && !f.dead && f.ship.as_ref().is_some_and(|s| s.aboard == Some(company))).map(|f| f.id)
    }

    /// A city warship's or transport's turn.
    pub(crate) fn update_ship(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get_mut(fid) else { return };
        if f.action == action::CORPSE {
            self.sinking(fid);
            return;
        }
        if f.ship.is_none() {
            // Launched by an older build: it starts over from its launch.
            f.ship = Some(Box::default());
            f.action = ship::LAUNCHED;
            f.counter = 0;
            f.speed = 2;
            f.route.clear();
        }
        let (kind, home, act) = (f.kind, f.home, f.action);
        // Its wharf lost, it goes down.
        if self.buildings.get(home).is_none_or(|w| w.kind != wharf_kind(kind)) {
            self.sink(fid);
            return;
        }
        let to_yard = if kind == WARSHIP { ship::WARSHIP_TO_YARD } else { ship::TRANSPORT_TO_YARD };
        let pct = self.hull_percent(fid);
        if act != to_yard && act != ship::IN_REPAIR && pct > 0 && pct <= REPAIR_AT {
            self.send_to_repair(fid);
        }
        if kind == WARSHIP {
            self.update_warship(fid);
        } else {
            self.update_transport(fid);
        }
    }

    fn sink(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get_mut(fid) else { return };
        f.action = action::CORPSE;
        f.counter = 0;
        f.route.clear();
        f.moving = false;
        self.figure_sound(fid, 3);
    }

    /// A sinking ship goes under; a transport's company on board goes down with it.
    fn sinking(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get_mut(fid) else { return };
        f.counter += 1;
        let counter = f.counter;
        if counter == 2 {
            self.drown_company(fid);
        }
        if counter >= SINK_TICKS {
            self.figures.get_mut(fid).expect("present").dead = true;
        }
    }

    /// The company aboard transport `fid` is lost with it; its standard goes back to
    /// its fort.
    pub(crate) fn drown_company(&mut self, fid: FigureId) {
        let Some(c) = self.ship_state(fid).and_then(|s| s.aboard.take()) else { return };
        let Some(co) = self.military.companies.get_mut(c) else { return };
        let soldiers = std::mem::take(&mut co.soldiers);
        co.at_fort = true;
        let (standard, ground) = (co.standard, co.ground);
        for s in soldiers {
            if let Some(f) = self.figures.get_mut(s) {
                f.dead = true;
            }
        }
        let home = self.buildings.get(ground).map(|g| (g.x, g.y));
        if let (Some(f), Some((x, y))) = (self.figures.get_mut(standard), home) {
            f.x = x;
            f.y = y;
            f.route.clear();
            f.moving = false;
            f.action = action::AT_REST;
        }
    }

    /// Sails a ship on along its route: a warship's exhausted rowers don't row, tired
    /// ones row at a third of the pace, and in pursuit a warship slows to half pace
    /// once it has run four tiles straight (FUN_004a5680). Keeps count of the tiles
    /// run straight, and finds a new way round anything in its path.
    fn sail(&mut self, fid: FigureId, chasing: bool) -> Step {
        let ticks = self.time.total_ticks;
        let map = &self.map;
        let Some(f) = self.figures.get_mut(fid) else { return Step::Lost };
        let Some((fatigue, straight, heading)) = f.ship.as_deref().map(|s| (s.fatigue, s.straight, s.heading)) else { return Step::Lost };
        if fatigue == 2 || (fatigue == 1 && ticks % 3 != 0) {
            return if f.route.is_empty() && !f.moving { Step::Arrived } else { Step::Moving };
        }
        f.speed = if fatigue == 1 || (chasing && straight >= RAM_RUN) { 1 } else { 2 };
        let before = (f.x, f.y);
        let step = f.walk(map);
        if (f.x, f.y) != before {
            let dir = f.direction;
            let s = f.ship.as_deref_mut().expect("checked");
            s.straight = if dir == heading { straight + 1 } else { 1 };
            s.heading = dir;
        }
        if step == Step::Blocked
            && let Some(d) = f.destination
        {
            f.go_to(map, d);
        }
        step
    }

    /// Heads a ship for its wharf's mooring.
    fn sail_home(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let act = if f.kind == WARSHIP { ship::WARSHIP_HOME } else { ship::TRANSPORT_HOME };
        let berth = self.mooring_for(f.home, (f.x, f.y));
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("present");
        f.action = act;
        f.foe = 0;
        match berth {
            Some(t) if t != (f.x, f.y) => {
                f.go_to(map, t);
            }
            _ => {
                f.route.clear();
                f.destination = Some((f.x, f.y));
            }
        }
    }

    /// Sends a ship to the working shipwright with the fewest ships waiting, the
    /// nearest of those that tie; with none, it waits and goes home (FUN_004a45d0,
    /// FUN_00406a30).
    fn send_to_repair(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (kind, pos) = (f.kind, (f.x, f.y));
        self.leave_yard(fid);
        let yard = self
            .buildings
            .iter()
            .filter(|b| b.kind == SHIPWRIGHT && b.workers > 0 && b.repair_queue.len() < 200)
            .min_by_key(|b| (b.repair_queue.len(), (b.x - pos.0).pow(2) + (b.y - pos.1).pow(2)))
            .map(|b| b.id);
        let berth = yard.and_then(|y| self.yard_berth(y, pos));
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("present");
        f.foe = 0;
        match (yard, berth) {
            (Some(y), Some(t)) => {
                f.action = if kind == WARSHIP { ship::WARSHIP_TO_YARD } else { ship::TRANSPORT_TO_YARD };
                if t == pos {
                    f.route.clear();
                    f.destination = Some(t);
                } else {
                    f.go_to(map, t);
                }
                if let Some(s) = f.ship.as_deref_mut() {
                    s.yard = y;
                }
            }
            _ => {
                f.action = ship::LAUNCHED;
                f.counter = 0;
            }
        }
    }

    /// Open water beside shipwright `id` a ship at `from` can reach.
    fn yard_berth(&self, id: BuildingId, from: (i32, i32)) -> Option<(i32, i32)> {
        let b = self.buildings.get(id)?;
        let reach = crate::water::reachable(&self.map, from);
        let w = self.map.width;
        crate::buildings::ring(b.x - 1, b.y - 1, b.size + 2)
            .chain(crate::buildings::ring(b.x, b.y, b.size))
            .filter(|&(x, y)| self.map.contains(x, y) && reach[(y * w + x) as usize])
            .min_by_key(|&(x, y)| distance((x, y), from))
    }

    /// Takes ship `fid` out of any shipwright's repair queue.
    fn leave_yard(&mut self, fid: FigureId) {
        for b in self.buildings.iter_mut().filter(|b| b.kind == SHIPWRIGHT) {
            b.repair_queue.retain(|&s| s != fid);
            if b.repairing == fid {
                b.repairing = 0;
            }
        }
        if let Some(s) = self.ship_state(fid) {
            s.yard = 0;
        }
    }

    /// On its way to the shipwright, or waiting there: it joins the yard's queue on
    /// arriving; a yard lost meanwhile, it looks for another.
    fn repair_trip(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let act = f.action;
        let yard = f.ship.as_ref().map_or(0, |s| s.yard);
        if self.buildings.get(yard).is_none_or(|b| b.kind != SHIPWRIGHT) {
            self.send_to_repair(fid);
            return;
        }
        if act == ship::IN_REPAIR {
            return;
        }
        if self.sail(fid, false) == Step::Moving {
            return;
        }
        let b = self.buildings.get_mut(yard).expect("present");
        if b.repairing != fid && !b.repair_queue.contains(&fid) {
            b.repair_queue.push(fid);
        }
        self.figures.get_mut(fid).expect("present").action = ship::IN_REPAIR;
    }

    /// A launched or repaired ship waits, then sails for its wharf.
    fn launch_wait(&mut self, fid: FigureId) -> bool {
        let Some(f) = self.figures.get_mut(fid) else { return false };
        f.counter += 1;
        if f.counter < LAUNCH_WAIT {
            return false;
        }
        f.counter = 0;
        self.sail_home(fid);
        true
    }

    /// Enemies a warship at `from` might go after within `reach`, best first by the
    /// original's reckoning (FUN_004bcb10): loaded enemy transports (25), invaders
    /// ashore within its missile range (17), empty transports (10), less the distance
    /// to each; only what scores better than `-reach` counts. `post` limits it to
    /// enemies within 10 tiles of the post it guards.
    fn ship_target(&self, fid: FigureId, reach: i32, post: Option<(i32, i32)>) -> Option<FigureId> {
        let f = self.figures.get(fid)?;
        let from = (f.x, f.y);
        let range = self.fighter_stats(fid).missile_range;
        let near_post = |p: (i32, i32)| post.is_none_or(|a| distance(a, p) < ENGAGE_LEASH);
        let ships = self.figures.iter().filter(|o| o.kind == ENEMY_TRANSPORT && !o.dead && o.action != action::CORPSE).map(|o| (o.id, (o.x, o.y), if o.amount > 0 { 25 } else { 10 }));
        let land = self.combatants.invaders.iter().filter(|o| distance((o.1, o.2), from) <= range).map(|o| (o.0, (o.1, o.2), 17));
        ships
            .chain(land)
            .filter(|&(id, p, _)| near_post(p) && self.figures.get(id).is_some_and(|o| !o.dead && o.action != action::CORPSE))
            .map(|(id, p, pri)| (pri - distance(from, p), id))
            .filter(|&(score, _)| score > -reach)
            .max_by_key(|&(score, id)| (score, std::cmp::Reverse(id)))
            .map(|(_, id)| id)
    }

    /// Whether `fid` is an enemy still afloat or standing.
    fn alive_enemy(&self, fid: FigureId) -> bool {
        self.figures.get(fid).is_some_and(|o| !o.dead && o.action != action::CORPSE && (o.kind == ENEMY_TRANSPORT || self.is_invader(o)))
    }

    /// A warship looses a javelin at `target` if it has reloaded (a shot every 31
    /// ticks). Its damage is 15 x (20 - armour) / 10 (FUN_004b45c0); the javelin
    /// carries twice the ship's missile attack so the city's javelin rule (attack x
    /// (20 - armour) / 20) comes to the same.
    fn ship_fire(&mut self, fid: FigureId, target: FigureId) {
        let stats = self.fighter_stats(fid);
        let Some(to) = self.figures.get(target).map(|t| (t.x, t.y)) else { return };
        let Some(s) = self.ship_state(fid) else { return };
        if s.reload <= RELOAD {
            return;
        }
        s.reload = 0;
        let from = self.figures.get(fid).map(|f| (f.x, f.y)).expect("present");
        self.figure_sound(fid, 2);
        let missile = self.figures.spawn(crate::military::JAVELIN, from.0, from.1, Travel::Land);
        if let Some(m) = self.figures.get_mut(missile) {
            m.foe = target;
            m.amount = 2 * stats.missile_attack;
            m.destination = Some(to);
            m.direction = crate::figures::direction_to(from, to).unwrap_or(0);
        }
    }

    /// A warship under way or holding shoots at whatever it can see (FUN_004a6bf0).
    fn fire_at_will(&mut self, fid: FigureId) {
        if let Some(t) = self.ship_target(fid, ENGAGE_SEARCH, None) {
            self.ship_fire(fid, t);
        }
    }

    /// A warship rams an enemy ship on its tile or the one ahead: its full attack
    /// after a straight run of four tiles, else 20, divided by the angle of the blow;
    /// the two ships are knocked a point off their headings, and the warship, its
    /// quarry forgotten, seeks and destroys (FUN_004a6160). True if it rammed.
    fn ram(&mut self, fid: FigureId) -> bool {
        let attack = self.fighter_stats(fid).attack;
        let Some(f) = self.figures.get(fid) else { return false };
        let Some(s) = f.ship.as_deref() else { return false };
        if s.ram_cool > 0 {
            return false;
        }
        let (dir, straight) = (f.direction, s.straight);
        let ahead = (f.x + NEIGHBOURS[dir as usize].0, f.y + NEIGHBOURS[dir as usize].1);
        let here = (f.x, f.y);
        let Some((target, tdir)) = self.figures.iter().find(|o| o.kind == ENEMY_TRANSPORT && !o.dead && o.action != action::CORPSE && ((o.x, o.y) == here || (o.x, o.y) == ahead)).map(|o| (o.id, o.direction)) else {
            return false;
        };
        let force = if straight >= RAM_RUN { attack } else { RAM_WEAK };
        let angle = RAM_ANGLE[tdir as usize % 8][dir as usize % 8] as i32;
        self.figure_sound(fid, 2);
        self.hurt(target, force / angle.max(1));
        let knock = |w: &mut World, d: u8| -> u8 {
            w.rng.next();
            let turn = if w.rng.byte() & 1 == 0 { -1 } else { 1 };
            (d as i32 + turn).clamp(0, 7) as u8
        };
        let own = knock(self, dir);
        let theirs = knock(self, tdir);
        if let Some(t) = self.figures.get_mut(target) {
            t.direction = theirs;
        }
        let f = self.figures.get_mut(fid).expect("present");
        f.direction = own;
        f.action = ship::SEEK;
        f.foe = 0;
        f.route.clear();
        if let Some(s) = f.ship.as_deref_mut() {
            s.ram_cool = RAM_COOLDOWN;
            s.straight = 0;
        }
        true
    }

    /// A warship's turn.
    fn update_warship(&mut self, fid: FigureId) {
        let Some(s) = self.ship_state(fid) else { return };
        s.reload = (s.reload + 1).min(RELOAD + 1);
        s.ram_cool = (s.ram_cool - 1).max(0);
        if s.fatigue > 0 {
            s.rest -= 1;
            if s.rest <= 0 {
                s.fatigue -= 1;
                s.rest = if s.fatigue > 0 { FATIGUE_TICKS } else { 0 };
            }
        }
        let act = self.figures.get(fid).map_or(0, |f| f.action);
        match act {
            ship::LAUNCHED => {
                self.fire_at_will(fid);
                self.launch_wait(fid);
            }
            ship::WARSHIP_HOME => {
                self.fire_at_will(fid);
                if self.sail(fid, false) != Step::Moving {
                    if self.ship_docked(fid) || self.figures.get(fid).is_some_and(|f| f.destination == Some((f.x, f.y))) {
                        // At its wharf it stands guard there (FUN_004a6890).
                        self.set_post(fid, ship::ENGAGE);
                    } else {
                        self.sail_home(fid);
                    }
                }
            }
            ship::HOLD => {
                self.fire_at_will(fid);
                self.sail(fid, false);
            }
            ship::ENGAGE | ship::SEEK => self.warship_combat(fid, act),
            ship::ATTACK => self.warship_attack(fid),
            ship::WARSHIP_TO_YARD | ship::IN_REPAIR => self.repair_trip(fid),
            _ => {
                let f = self.figures.get_mut(fid).expect("present");
                f.action = ship::LAUNCHED;
                f.counter = 0;
            }
        }
    }

    /// Makes where ship `fid` is its post, under order `act`.
    fn set_post(&mut self, fid: FigureId, act: u16) {
        let Some(f) = self.figures.get_mut(fid) else { return };
        f.action = act;
        f.foe = 0;
        f.counter = 0;
        let at = f.destination.filter(|_| !f.route.is_empty() || f.moving).unwrap_or((f.x, f.y));
        if let Some(s) = f.ship.as_deref_mut() {
            s.anchor = at;
        }
    }

    /// Engage nearby enemies, or seek and destroy all (FUN_004a6300): after the best
    /// enemy in reach, and guarding, back to its post when it has strayed 10 tiles
    /// from it or has no one to fight.
    fn warship_combat(&mut self, fid: FigureId, act: u16) {
        let Some(f) = self.figures.get(fid) else { return };
        let (pos, foe) = ((f.x, f.y), f.foe);
        let anchor = f.ship.as_ref().map_or(pos, |s| s.anchor);
        let guarding = act == ship::ENGAGE;
        if guarding && distance(pos, anchor) >= ENGAGE_LEASH {
            self.back_to_post(fid, anchor);
            return;
        }
        let target = if self.alive_enemy(foe) { Some(foe) } else { self.ship_target(fid, if guarding { ENGAGE_SEARCH } else { SEEK_SEARCH }, guarding.then_some(anchor)) };
        match target {
            Some(t) => {
                self.figures.get_mut(fid).expect("present").foe = t;
                self.fight_target(fid, t);
            }
            None => {
                let f = self.figures.get_mut(fid).expect("present");
                f.foe = 0;
                let idle = f.route.is_empty() && !f.moving;
                if guarding && idle && pos != anchor {
                    self.back_to_post(fid, anchor);
                } else if !idle {
                    self.sail(fid, false);
                }
            }
        }
    }

    fn back_to_post(&mut self, fid: FigureId, anchor: (i32, i32)) {
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("present");
        f.foe = 0;
        if f.destination != Some(anchor) || (f.route.is_empty() && !f.moving) {
            f.go_to(map, anchor);
        }
        self.sail(fid, false);
    }

    /// Fights `target`: javelins within missile range; after an enemy ship to ram it,
    /// finding a fresh way to it every 10 ticks; an enemy ashore out of range is let
    /// go.
    fn fight_target(&mut self, fid: FigureId, target: FigureId) {
        let range = self.fighter_stats(fid).missile_range;
        let (Some(f), Some(t)) = (self.figures.get(fid), self.figures.get(target)) else { return };
        let (pos, at, afloat) = ((f.x, f.y), (t.x, t.y), t.kind == ENEMY_TRANSPORT);
        let d = distance(pos, at);
        if d < range {
            self.ship_fire(fid, target);
        }
        if !afloat {
            if d >= range && self.figures.get(fid).is_some_and(|f| f.action != ship::ATTACK) {
                self.figures.get_mut(fid).expect("present").foe = 0;
            }
            return;
        }
        if self.ram(fid) {
            return;
        }
        self.pursue(fid, at);
    }

    /// Rows after a quarry at `at`, tiring the crew.
    fn pursue(&mut self, fid: FigureId, at: (i32, i32)) {
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("present");
        let idle = f.route.is_empty() && !f.moving;
        let Some(s) = f.ship.as_deref_mut() else { return };
        s.repath -= 1;
        s.chase += 1;
        if s.chase >= EXHAUSTING {
            s.chase = 0;
            s.fatigue = 2;
            s.rest = FATIGUE_TICKS;
        }
        let repath = s.repath <= 0 || idle;
        if repath {
            s.repath = REPATH_TICKS;
            if f.destination != Some(at) || f.route.is_empty() {
                f.go_to(map, at);
            }
        }
        self.sail(fid, true);
    }

    /// Sent at an enemy (FUN_004a6d40): after it until it falls, then after the
    /// nearest other anywhere; with none left, back to its former order.
    fn warship_attack(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let foe = f.foe;
        let target = if self.alive_enemy(foe) { Some(foe) } else { self.ship_target(fid, SEEK_SEARCH, None) };
        let Some(t) = target else {
            let f = self.figures.get_mut(fid).expect("present");
            let saved = f.ship.as_ref().map_or(0, |s| s.saved);
            f.foe = 0;
            let back = if matches!(saved, ship::HOLD | ship::ENGAGE | ship::SEEK) { saved } else { ship::ENGAGE };
            self.set_post(fid, back);
            return;
        };
        self.figures.get_mut(fid).expect("present").foe = t;
        let range = self.fighter_stats(fid).missile_range;
        let (pos, at, afloat) = {
            let (f, o) = (self.figures.get(fid).expect("present"), self.figures.get(t).expect("alive"));
            ((f.x, f.y), (o.x, o.y), o.kind == ENEMY_TRANSPORT)
        };
        if afloat {
            self.fight_target(fid, t);
        } else if distance(pos, at) < range {
            self.ship_fire(fid, t);
        } else if let Some(w) = self.nearest_water(at) {
            self.pursue(fid, w);
        }
    }

    /// A transport's turn.
    fn update_transport(&mut self, fid: FigureId) {
        let act = self.figures.get(fid).map_or(0, |f| f.action);
        match act {
            ship::LAUNCHED => {
                self.launch_wait(fid);
            }
            ship::TRANSPORT_HOME => {
                if self.sail(fid, false) != Step::Moving {
                    if self.ship_docked(fid) || self.figures.get(fid).is_some_and(|f| f.destination == Some((f.x, f.y))) {
                        self.set_post(fid, ship::EVADE);
                    } else {
                        self.sail_home(fid);
                    }
                }
            }
            ship::EVADE => {
                self.sail(fid, false);
                let f = self.figures.get_mut(fid).expect("present");
                f.counter += 1;
                if f.counter >= EVADE_LOOK {
                    f.counter = 0;
                    self.evade(fid);
                }
            }
            ship::HOLD => {
                self.sail(fid, false);
            }
            ship::EMBARK => self.embark_step(fid),
            ship::DISEMBARK => self.disembark_step(fid),
            ship::TRANSPORT_TO_YARD | ship::IN_REPAIR => self.repair_trip(fid),
            _ => {
                let f = self.figures.get_mut(fid).expect("present");
                f.action = ship::LAUNCHED;
                f.counter = 0;
            }
        }
    }

    /// An evading transport with an enemy ship near runs from it: of three points
    /// about 10 tiles off, straight away and to either side, the one farthest from
    /// the enemy that open water leads to.
    fn evade(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let pos = (f.x, f.y);
        let Some(enemy) = self.figures.iter().filter(|o| o.kind == ENEMY_TRANSPORT && !o.dead && o.action != action::CORPSE && (o.x - pos.0).abs() <= EVADE_BOX && (o.y - pos.1).abs() <= EVADE_BOX).map(|o| (o.x, o.y)).min_by_key(|&p| distance(p, pos)) else {
            return;
        };
        let away = crate::figures::direction_to(enemy, pos).unwrap_or(2) as i32;
        let best = [away, away + 7, away + 1]
            .into_iter()
            .map(|d| {
                let (dx, dy) = NEIGHBOURS[(d % 8) as usize];
                self.water_line(pos, (pos.0 + dx * EVADE_RUN, pos.1 + dy * EVADE_RUN))
            })
            .filter(|&p| p != pos)
            .max_by_key(|&p| distance(p, enemy));
        if let Some(p) = best {
            let map = &self.map;
            self.figures.get_mut(fid).expect("present").go_to(map, p);
        }
    }

    /// The farthest open water on the straight line from `from` toward `to` before
    /// anything else (FUN_0040c180).
    fn water_line(&self, from: (i32, i32), to: (i32, i32)) -> (i32, i32) {
        let n = distance(from, to);
        let mut last = from;
        for i in 1..=n {
            let x = from.0 + ((to.0 - from.0) as f64 * i as f64 / n as f64).round() as i32;
            let y = from.1 + ((to.1 - from.1) as f64 * i as f64 / n as f64).round() as i32;
            if !crate::water::navigable(&self.map, x, y) {
                break;
            }
            last = (x, y);
        }
        last
    }

    /// Whether ship `fid` is at the shipwright's with its hull at 15% or less, when
    /// it takes no orders.
    pub fn ship_laid_up(&self, fid: FigureId) -> bool {
        let Some(f) = self.figures.get(fid) else { return true };
        let at_yard = matches!(f.action, ship::IN_REPAIR) || f.action == if f.kind == WARSHIP { ship::WARSHIP_TO_YARD } else { ship::TRANSPORT_TO_YARD };
        at_yard && self.hull_percent(fid) <= REPAIR_AT
    }

    /// An order from the ship's window or a hotkey.
    pub fn order_ship(&mut self, fid: FigureId, order: ShipOrder) -> bool {
        let Some(f) = self.figures.get(fid) else { return false };
        if !is_city_ship(f.kind) || f.action == action::CORPSE || f.ship.is_none() || self.ship_laid_up(fid) {
            return false;
        }
        let (kind, damaged) = (f.kind, f.damage > 0);
        match order {
            ShipOrder::Repair if !damaged => return false,
            ShipOrder::Repair => {
                self.send_to_repair(fid);
                return true;
            }
            ShipOrder::Engage | ShipOrder::Seek if kind != WARSHIP => return false,
            ShipOrder::Evade if kind != TRANSPORT => return false,
            _ => {}
        }
        self.leave_yard(fid);
        self.cancel_embark(fid);
        match order {
            ShipOrder::Return => self.sail_home(fid),
            ShipOrder::Hold | ShipOrder::Engage | ShipOrder::Seek | ShipOrder::Evade => {
                let act = match order {
                    ShipOrder::Hold => ship::HOLD,
                    ShipOrder::Engage => ship::ENGAGE,
                    ShipOrder::Seek => ship::SEEK,
                    _ => ship::EVADE,
                };
                let f = self.figures.get_mut(fid).expect("present");
                f.route.clear();
                f.destination = None;
                self.set_post(fid, act);
            }
            ShipOrder::Repair => {}
        }
        true
    }

    fn cancel_embark(&mut self, fid: FigureId) {
        if let Some(s) = self.ship_state(fid) {
            s.embark = None;
            s.landing = None;
        }
    }

    /// A click on the map with a ship selected: a warship sent at the enemy on that
    /// tile goes after it; otherwise the ship sails as far toward the spot as open
    /// water goes straight, and keeps its order there (FUN_0040b830, FUN_0040ba40).
    pub fn move_ship(&mut self, fid: FigureId, to: (i32, i32)) -> bool {
        let Some(f) = self.figures.get(fid) else { return false };
        if !is_city_ship(f.kind) || f.action == action::CORPSE || f.ship.is_none() || self.ship_laid_up(fid) {
            return false;
        }
        let (kind, act, pos) = (f.kind, f.action, (f.x, f.y));
        let enemy = self.figures.iter().find(|o| (o.x, o.y) == to && self.alive_enemy(o.id)).map(|o| o.id);
        if kind == WARSHIP
            && let Some(enemy) = enemy
        {
            self.leave_yard(fid);
            let f = self.figures.get_mut(fid).expect("present");
            if let Some(s) = f.ship.as_deref_mut() {
                s.saved = if act == ship::ATTACK { s.saved } else { act };
            }
            f.action = ship::ATTACK;
            f.foe = enemy;
            return true;
        }
        let dest = self.water_line(pos, to);
        if dest == pos {
            return false;
        }
        let map = &self.map;
        if !self.figures.get_mut(fid).expect("present").go_to(map, dest) {
            return false;
        }
        let keeps = if kind == WARSHIP { matches!(act, ship::HOLD | ship::ENGAGE | ship::SEEK) } else { matches!(act, ship::HOLD | ship::EVADE) };
        let order = if keeps { act } else if kind == WARSHIP { ship::ENGAGE } else { ship::EVADE };
        self.leave_yard(fid);
        self.cancel_embark(fid);
        let f = self.figures.get_mut(fid).expect("present");
        f.action = order;
        f.foe = 0;
        f.counter = 0;
        if let Some(s) = f.ship.as_deref_mut() {
            s.anchor = dest;
        }
        true
    }

    /// Embark: company `c` marches to the shore nearest it, and transport `fid` sails
    /// to the water beside that spot to take it aboard (FUN_004a4d00).
    pub fn embark(&mut self, fid: FigureId, c: usize) -> bool {
        let Some(f) = self.figures.get(fid) else { return false };
        if f.kind != TRANSPORT || f.action == action::CORPSE || self.ship_laid_up(fid) || f.ship.as_ref().is_none_or(|s| s.aboard.is_some()) {
            return false;
        }
        let pos = (f.x, f.y);
        let Some(co) = self.military.companies.get(c) else { return false };
        if co.soldiers.is_empty() || self.company_ship(c).is_some() {
            return false;
        }
        let from = self.figures.get(co.standard).map_or(co.standard_tile, |s| (s.x, s.y));
        let reach = crate::water::reachable(&self.map, pos);
        let w = self.map.width;
        let water = |x: i32, y: i32| self.map.contains(x, y) && reach[(y * w + x) as usize];
        let shore = (0..40).find_map(|r| {
            (-r..=r).flat_map(move |dy| (-r..=r).map(move |dx| (from.0 + dx, from.1 + dy))).filter(|&(x, y)| distance((x, y), from) == r).find_map(|(x, y)| {
                if !crate::figures::passable(&self.map, Travel::Land, x, y) || self.map.terrain_is(x, y, terrain::WATER) {
                    return None;
                }
                // Open water comes no nearer the bank than two tiles.
                (1..=2).find_map(|d| (-d..=d).flat_map(move |dy| (-d..=d).map(move |dx| (x + dx, y + dy))).find(|&(wx, wy)| water(wx, wy))).map(|p| ((x, y), p))
            })
        });
        let Some((land, berth)) = shore else { return false };
        if !self.move_company(c, land) {
            return false;
        }
        self.leave_yard(fid);
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("present");
        if berth != pos {
            f.go_to(map, berth);
        }
        f.action = ship::EMBARK;
        f.foe = 0;
        if let Some(s) = f.ship.as_deref_mut() {
            s.embark = Some(c);
            s.landing = Some(land);
        }
        true
    }

    /// Picking a company up: once the ship is there and the company stands formed up
    /// at the shore, its men and standard go aboard and the ship sails home
    /// (FUN_004a43f0, FUN_004a4470).
    fn embark_step(&mut self, fid: FigureId) {
        let (c, land) = match self.figures.get(fid).and_then(|f| f.ship.as_deref()) {
            Some(Ship { embark: Some(c), landing: Some(l), .. }) => (*c, *l),
            _ => {
                self.set_post(fid, ship::EVADE);
                return;
            }
        };
        let arrived = self.sail(fid, false) != Step::Moving;
        let Some(co) = self.military.companies.get(c) else {
            self.cancel_embark(fid);
            self.set_post(fid, ship::EVADE);
            return;
        };
        if co.soldiers.is_empty() || co.at_fort || co.standard_tile != land {
            // Sent elsewhere, or gone.
            self.cancel_embark(fid);
            self.set_post(fid, ship::EVADE);
            return;
        }
        let formed = self.figures.get(co.standard).is_none_or(|s| !s.moving && s.route.is_empty())
            && co.soldiers.iter().all(|&s| self.figures.get(s).is_some_and(|f| f.action == action::AT_STANDARD && !f.moving));
        if !arrived || !formed {
            return;
        }
        let men: Vec<FigureId> = co.soldiers.iter().copied().chain(std::iter::once(co.standard)).collect();
        for m in men {
            if let Some(f) = self.figures.get_mut(m) {
                f.action = action::ABOARD;
                f.route.clear();
                f.moving = false;
                f.foe = 0;
            }
        }
        if let Some(s) = self.ship_state(fid) {
            s.aboard = Some(c);
            s.embark = None;
            s.landing = None;
        }
        self.sail_home(fid);
    }

    /// Disembark: transport `fid` sails as far toward land tile `to` as open water
    /// goes straight, to put its company ashore there (FUN_004a4e10).
    pub fn disembark(&mut self, fid: FigureId, to: (i32, i32)) -> bool {
        let Some(f) = self.figures.get(fid) else { return false };
        if f.kind != TRANSPORT || f.action == action::CORPSE || self.ship_laid_up(fid) || f.ship.as_ref().is_none_or(|s| s.aboard.is_none()) {
            return false;
        }
        if !crate::figures::passable(&self.map, Travel::Land, to.0, to.1) || self.map.terrain_is(to.0, to.1, terrain::WATER) {
            return false;
        }
        let pos = (f.x, f.y);
        let berth = self.water_line(pos, to);
        self.leave_yard(fid);
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("present");
        if berth != pos && !f.go_to(map, berth) {
            return false;
        }
        f.action = ship::DISEMBARK;
        if let Some(s) = f.ship.as_deref_mut() {
            s.landing = Some(to);
        }
        true
    }

    /// Putting a company ashore: at the end of its run the men land beside the ship
    /// and march to the chosen spot; the ship sails home. With no shore at hand it
    /// gives up and evades, the men still aboard.
    fn disembark_step(&mut self, fid: FigureId) {
        if self.sail(fid, false) == Step::Moving {
            return;
        }
        let Some(f) = self.figures.get(fid) else { return };
        let pos = (f.x, f.y);
        let (c, to) = match f.ship.as_deref() {
            Some(Ship { aboard: Some(c), landing: Some(l), .. }) => (*c, *l),
            _ => {
                self.set_post(fid, ship::EVADE);
                return;
            }
        };
        let Some(shore) = self.shore_near(pos) else {
            self.cancel_embark(fid);
            self.set_post(fid, ship::EVADE);
            return;
        };
        let Some(co) = self.military.companies.get_mut(c) else { return };
        co.at_fort = false;
        co.standard_tile = shore;
        let men: Vec<FigureId> = co.soldiers.iter().copied().chain(std::iter::once(co.standard)).collect();
        let standard = co.standard;
        for m in men {
            if let Some(f) = self.figures.get_mut(m) {
                (f.x, f.y) = shore;
                f.route.clear();
                f.moving = false;
                f.progress = 0;
                f.destination = None;
                f.action = if m == standard { action::AT_STANDARD } else { action::GOING_TO_STANDARD };
            }
        }
        if let Some(s) = self.ship_state(fid) {
            s.aboard = None;
            s.landing = None;
        }
        if !self.move_company(c, to) {
            self.move_company(c, shore);
        }
        self.sail_home(fid);
    }

    /// Sends an invading army by sea: transports appear at `spot` on the water, each
    /// with up to sixteen of `men` aboard.
    pub(crate) fn launch_sea_invasion(&mut self, army: usize, nation: u16, men: i32, spot: (i32, i32)) {
        let spot = self.nearest_water(spot).unwrap_or(spot);
        let landing = self.landing_place(spot);
        let mut left = men;
        let mut n = 0;
        // Each shipload is a formation afloat: none sails without a free slot.
        while left > 0 && self.enemy_formations() < crate::invasions::ENEMY_FORMATION_SLOTS {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{Command, Outcome};

    /// The sandbox town (see `water.rs`) with a warship wharf and a transport wharf
    /// on the north bank by its shipwright, fully staffed, the yard holding 200
    /// timber.
    fn navy_town() -> Option<World> {
        let mut world = crate::water::tests::sandbox_town()?;
        world.test_full_staff = true;
        world.scenario_allowed = None;
        // The figure models too, for the ships' hulls and weapons.
        let data = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../PharaohData");
        world.balance = std::sync::Arc::new(crate::balance::Balance::load(&data, "Normal").ok()?);
        for cmd in [Command::Road { start: (176, 109), end: (196, 109) }, Command::Build { kind: WARSHIP_WHARF, x: 186, y: 110, x1: 186, y1: 110 }, Command::Build { kind: TRANSPORT_WHARF, x: 172, y: 111, x1: 172, y1: 111 }] {
            assert!(matches!(world.apply(&cmd), Outcome::Done { .. }), "{cmd:?}");
        }
        let yard = world.map.building.at_or(180, 110, 0);
        world.add_stored(yard, TIMBER, 200);
        Some(world)
    }

    #[test]
    fn the_yard_builds_a_hull_in_sixteen_days_and_takes_its_timber_at_launch() {
        let Some(mut world) = navy_town() else { return };
        let yard = world.map.building.at_or(180, 110, 0);
        // Fishing boat first, then the transport, then the warship; each hull 16 full
        // days (10 a day to 160), a fishing boat free, a transport or warship 100
        // timber when it is launched.
        let mut started: Vec<(u16, u64)> = Vec::new();
        let mut launched: Vec<(u16, u64, i32)> = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..(60 * crate::time::TICKS_PER_DAY) {
            world.tick();
            let b = world.buildings.get(yard).expect("yard");
            if b.boat_kind != 0 && started.last().is_none_or(|&(k, _)| k != b.boat_kind) {
                started.push((b.boat_kind, world.time.total_ticks));
            }
            let timber = b.stock[TIMBER as usize];
            for f in world.figures.iter().filter(|f| matches!(f.kind, FISHING_BOAT | TRANSPORT | WARSHIP)) {
                if seen.insert(f.id) {
                    launched.push((f.kind, world.time.total_ticks, timber));
                }
            }
        }
        let kinds: Vec<u16> = launched.iter().map(|l| l.0).collect();
        assert_eq!(kinds, [FISHING_BOAT, TRANSPORT, WARSHIP], "{started:?} {launched:?}");
        for (s, l) in started.iter().zip(&launched) {
            assert_eq!(l.1 - s.1, 16 * crate::time::TICKS_PER_DAY as u64, "{started:?} {launched:?}");
        }
        assert_eq!(launched.iter().map(|l| l.2).collect::<Vec<_>>(), [200, 100, 0]);
    }

    #[test]
    fn a_damaged_ship_is_repaired_with_a_timber_a_day() {
        let Some(mut world) = navy_town() else { return };
        let yard = world.map.building.at_or(180, 110, 0);
        let warship = |w: &World| w.figures.iter().find(|f| f.kind == WARSHIP && f.action == ship::ENGAGE).map(|f| f.id);
        for _ in 0..(80 * crate::time::TICKS_PER_DAY) {
            if warship(&world).is_some() {
                break;
            }
            world.tick();
        }
        let s = warship(&world).expect("warship");
        world.add_stored(yard, TIMBER, 100);
        // Down to 13%: it sails for the yard by itself.
        world.figures.get_mut(s).expect("ship").damage = 260;
        let before = world.buildings.get(yard).expect("yard").stock[TIMBER as usize];
        for _ in 0..(40 * crate::time::TICKS_PER_DAY) {
            world.tick();
        }
        let f = world.figures.get(s).expect("ship");
        let b = world.buildings.get(yard).expect("yard");
        assert_eq!(f.damage, 0, "{} {:?} {:?} yard {} {} {:?} {}", f.action, (f.x, f.y), f.ship, b.boat_kind, b.repairing, b.repair_queue, b.workers);
        // 260 at 10 a day: 26 days, a timber each.
        assert_eq!(before - world.buildings.get(yard).expect("yard").stock[TIMBER as usize], 26);
    }

    #[test]
    fn a_transport_carries_a_company_across() {
        let Some(mut world) = navy_town() else { return };
        let transport = |w: &World| w.figures.iter().find(|f| f.kind == TRANSPORT && f.action == ship::EVADE).map(|f| f.id);
        for _ in 0..(60 * crate::time::TICKS_PER_DAY) {
            if transport(&world).is_some() {
                break;
            }
            world.tick();
        }
        let t = transport(&world).expect("transport");
        // A company of archers by the north bank.
        let standard = world.figures.spawn(crate::military::STANDARD_BEARER, 160, 108, Travel::Land);
        let mut soldiers = Vec::new();
        for i in 0..4 {
            let s = world.figures.spawn(crate::military::ARCHER, 158 + i, 107, Travel::Land);
            let f = world.figures.get_mut(s).expect("soldier");
            f.formation = 1;
            f.action = action::AT_STANDARD;
            soldiers.push(s);
        }
        world.figures.get_mut(standard).expect("standard").formation = 1;
        let mut company = crate::military::Company::default();
        (company.kind, company.soldiers, company.standard, company.standard_tile, company.morale, company.fort) = (crate::military::ARCHER, soldiers.clone(), standard, (160, 108), 60, 1);
        world.military.companies.push(company);
        assert!(matches!(world.apply(&Command::Embark { ship: t, company: 0 }), Outcome::Done { .. }));
        for _ in 0..2000 {
            world.tick();
        }
        assert_eq!(world.company_ship(0), Some(t));
        assert!(soldiers.iter().all(|&s| world.figures.get(s).is_some_and(|f| f.action == action::ABOARD)));
        // Across to the south bank.
        assert!(matches!(world.apply(&Command::Disembark { ship: t, x: 170, y: 127 }), Outcome::Done { .. }));
        for _ in 0..2000 {
            world.tick();
        }
        assert_eq!(world.company_ship(0), None);
        assert!(soldiers.iter().all(|&s| world.figures.get(s).is_some_and(|f| f.y > 120 && f.action != action::ABOARD)), "{:?}", soldiers.iter().map(|&s| world.figures.get(s).map(|f| (f.x, f.y, f.action))).collect::<Vec<_>>());
    }
}
