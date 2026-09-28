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
//! Marked warships, and companies aboard transports, answer the Kingdom's calls for
//! troops from cities reached by sea (FUN_004b84b0): they sail off by the river
//! entry and are gone until the battle is over. A warship brings a tenth of its hull
//! to the battle, and the battle's losses come off the warships' hulls, sinking
//! those they use up (FUN_004b8360, FUN_004b8890). The survivors sail home: a
//! warship to its wharf, a transport to put its company ashore by its old post
//! (FUN_004b8690).
//!
//! An invasion from the sea comes in transports, a ship for every sixteen men, with
//! the warships the scenario gives it (FUN_00446a30). The transports sail from the
//! sea invasion point to one of the landing places, put their men ashore and sail
//! away by the river; a transport sunk before it lands takes its men down with it.
//! The warships patrol the river and hunt the city's ships and soldiers, ram and
//! shoot at them, flee when badly holed, and sail off after four months without
//! a fight (their action functions at 0x4aee10 and 0x4af970).

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
pub const ENEMY_WARSHIP: u16 = 93;
/// An Egyptian army's (Pharaoh's, or a rebel city's) warships and transports.
pub const EGYPT_WARSHIP: u16 = 100;
pub const EGYPT_TRANSPORT: u16 = 101;

/// Whether `k` is an invader's ship.
pub fn is_enemy_ship(k: u16) -> bool {
    matches!(k, ENEMY_TRANSPORT | ENEMY_WARSHIP | EGYPT_WARSHIP | EGYPT_TRANSPORT)
}

pub fn is_enemy_warship(k: u16) -> bool {
    matches!(k, ENEMY_WARSHIP | EGYPT_WARSHIP)
}

pub fn is_enemy_transport(k: u16) -> bool {
    matches!(k, ENEMY_TRANSPORT | EGYPT_TRANSPORT)
}

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
/// Why a call for troops can or can't be answered (FUN_0044d520), which decides
/// the Political Overseer's pop-up (text group 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TroopsStatus {
    /// Land call, no companies ashore: 10/11.
    NoCompanies,
    /// Land call, none of them in Kingdom service: 12/13.
    NoneMarked,
    /// Sea call, no warships and no company aboard: 98/99.
    NoShips,
    /// Sea call, none of them in Kingdom service: 100/101.
    NoneMarkedAfloat,
    /// "Dispatch relief force?": 14/15.
    Ready,
}

impl TroopsStatus {
    /// The pop-up's title and line in text group 5.
    pub fn popup(self) -> (usize, usize) {
        match self {
            TroopsStatus::NoCompanies => (10, 11),
            TroopsStatus::NoneMarked => (12, 13),
            TroopsStatus::NoShips => (98, 99),
            TroopsStatus::NoneMarkedAfloat => (100, 101),
            TroopsStatus::Ready => (14, 15),
        }
    }
}

/// Ticks between a pursuing warship's fresh routes to its quarry.
const REPATH_TICKS: i32 = 10;
/// A warship rams at full force (its model attack) after a straight run of the
/// model's k tiles, else at a fifth of it; then it can't ram for 15 ticks.
const RAM_WEAK_PCT: i32 = 20;
const RAM_COOLDOWN: i32 = 15;
/// A warship holding position braces for a ram: it takes 20 less.
const HOLD_BRACE: i32 = 20;
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
/// Ticks of pursuit exhaust a warship's rowers after the model's l (800 for the
/// city's); they then rest this long without moving, and row slowly as long again
/// (FUN_004a5680).
const FATIGUE_TICKS: i32 = 200;
/// A transport evading looks round every 75 ticks for enemy ships within 9 tiles
/// each way, and runs about 10 tiles from them (FUN_004a4a80, FUN_004a4850).
const EVADE_LOOK: i32 = 75;
const EVADE_BOX: i32 = 9;
const EVADE_RUN: i32 = 10;
/// Men an invading transport carries.
const TRANSPORT_LOAD: i32 = 16;
/// An invading transport waits 40 ticks more than the one before it to set out (10
/// for the first), a warship 25 more (FUN_00446a30).
const TRANSPORT_STAGGER: i32 = 40;
const WARSHIP_STAGGER: i32 = 25;
const FIRST_WAIT: i32 = 10;
/// An enemy transport lies off the shore this long after landing its men.
const UNLOAD_WAIT: i32 = 50;
/// An enemy warship on patrol looks for a fight every 75 ticks and up to 29 more; on
/// the attack it looks again every 50 (FUN_004aeb00, FUN_004ae650).
const PATROL_LOOK: i32 = 75;
const PATROL_LOOK_RAND: i32 = 30;
const HUNT_LOOK: i32 = 50;
/// The months an enemy warship stays after its arrival or its last fight.
const STAY_MONTHS: i32 = 4;
/// A ship abroad brings a tenth of its hull to the battle (FUN_004b8330).
const HULL_PER_STRENGTH: i32 = 10;

/// Ship actions: the invaders' transports', then the city's ships', numbered as the
/// original's.
pub mod ship {
    pub const LANDING: u16 = 4;
    pub const LEAVING: u16 = 5;
    /// An invader's transport running from the city's ships, badly holed.
    pub const ENEMY_EVADE: u16 = 10;
    /// An enemy warship hunting (the original's 10), fleeing when badly holed (12),
    /// patrolling the river (17), and sailing off (18).
    pub const HUNT: u16 = 10;
    pub const FLEE: u16 = 12;
    pub const PATROL: u16 = 17;
    pub const DEPART: u16 = 18;
    /// A city ship off to the Kingdom's battle (17), away (18), and coming back (19).
    pub const TO_BATTLE: u16 = 17;
    pub const AWAY: u16 = 18;
    pub const BACK: u16 = 19;
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
    /// A warship marked for Kingdom service.
    pub service: bool,
    /// The Kingdom's request it is away on (the original's +0x132).
    pub abroad: Option<usize>,
    /// Out of sight, away at the Kingdom's battle.
    pub hidden: bool,
    /// An enemy warship's: the month it arrived or last found a fight (it sails off
    /// four months on), its leg of the patrol, whether it is closing with its quarry,
    /// and ticks to its next look round.
    pub month: i32,
    pub leg: u8,
    pub engaged: bool,
    pub look: i32,
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
        let Some(orientation) = self.buildings.get(id).map(|b| b.orientation) else { return };
        let Some((x, y)) = self.launch_tile(id) else { return };
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

    /// The first tile round building `id` that is water, has no building, and has
    /// water all round (FUN_004806c0).
    fn launch_tile(&self, id: BuildingId) -> Option<(i32, i32)> {
        let b = self.buildings.get(id)?;
        let map = &self.map;
        let open = |x: i32, y: i32| {
            map.terrain_is(x, y, terrain::WATER | terrain::DEEPWATER) && !map.terrain_is(x, y, terrain::BUILDING) && NEIGHBOURS.iter().all(|&(dx, dy)| map.terrain_is(x + dx, y + dy, terrain::WATER))
        };
        crate::buildings::ring(b.x, b.y, b.size).find(|&(x, y)| open(x, y))
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
        if self.holed(fid) {
            self.sink(fid);
            return;
        }
        // Away on the Kingdom's service it takes no orders (the action functions put
        // it back on its way whatever it was doing).
        if self.figures.get(fid).and_then(|f| f.ship.as_ref()).is_some_and(|s| s.abroad.is_some()) {
            self.abroad_step(fid);
            return;
        }
        if kind == WARSHIP && act == ship::BACK {
            // Home from the battle: to the water by its wharf, then as if launched.
            if self.sail(fid, false) != Step::Moving {
                let f = self.figures.get_mut(fid).expect("present");
                f.action = ship::LAUNCHED;
                f.counter = 0;
            }
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

    pub(crate) fn sink(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get_mut(fid) else { return };
        f.action = action::CORPSE;
        f.counter = 0;
        f.route.clear();
        f.moving = false;
        self.figure_sound(fid, 3);
    }

    /// A sinking ship goes under; a transport's company on board goes down with it.
    pub(crate) fn sinking(&mut self, fid: FigureId) {
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
    /// once it has run its model's k tiles straight (FUN_004a5680). Keeps count of the tiles
    /// run straight, and finds a new way round anything in its path.
    fn sail(&mut self, fid: FigureId, chasing: bool) -> Step {
        let ticks = self.time.total_ticks;
        let run = self.fighter_stats(fid).ram_run.max(1);
        let map = &self.map;
        let Some(f) = self.figures.get_mut(fid) else { return Step::Lost };
        let Some((fatigue, straight, heading)) = f.ship.as_deref().map(|s| (s.fatigue, s.straight, s.heading)) else { return Step::Lost };
        if fatigue == 2 || (fatigue == 1 && ticks % 3 != 0) {
            return if f.route.is_empty() && !f.moving { Step::Arrived } else { Step::Moving };
        }
        f.speed = if fatigue == 1 || (chasing && straight >= run) { 1 } else { 2 };
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
        let yard = self.repair_yard(fid);
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

    /// The working shipwright with the fewest ships waiting, the nearest of those that
    /// tie (FUN_00406a30).
    fn repair_yard(&self, fid: FigureId) -> Option<BuildingId> {
        let f = self.figures.get(fid)?;
        let pos = (f.x, f.y);
        let own = |b: &crate::buildings::Building| b.repair_queue.iter().filter(|&&s| s != fid).count();
        self.buildings
            .iter()
            .filter(|b| b.kind == SHIPWRIGHT && b.workers > 0 && b.repair_queue.len() < 200)
            .min_by_key(|b| (own(b), (b.x - pos.0).pow(2) + (b.y - pos.1).pow(2)))
            .map(|b| b.id)
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
        // Arriving at a yard with ships already waiting, it goes to another if that is
        // now the better choice (FUN_004a4c40).
        let queued = self.buildings.get(yard).is_some_and(|b| !b.repair_queue.is_empty());
        if queued && self.repair_yard(fid).is_some_and(|y| y != yard) {
            self.send_to_repair(fid);
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
    /// ashore within its missile range (17), enemy warships (15), empty transports
    /// (10), less the distance to each; only what scores better than `-reach` counts.
    /// `post` limits it to enemies within 10 tiles of the post it guards.
    fn ship_target(&self, fid: FigureId, reach: i32, post: Option<(i32, i32)>) -> Option<FigureId> {
        let f = self.figures.get(fid)?;
        let from = (f.x, f.y);
        let range = self.fighter_stats(fid).missile_range;
        let near_post = |p: (i32, i32)| post.is_none_or(|a| distance(a, p) < ENGAGE_LEASH);
        let ships = self.figures.iter().filter(|o| is_enemy_ship(o.kind) && !o.dead && o.action != action::CORPSE).map(|o| (o.id, (o.x, o.y), if is_enemy_warship(o.kind) { 15 } else if o.amount > 0 { 25 } else { 10 }));
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
        self.figures.get(fid).is_some_and(|o| !o.dead && o.action != action::CORPSE && (is_enemy_ship(o.kind) || self.is_invader(o)))
    }

    /// A warship looses a javelin at `target` if it has reloaded (a shot every 31
    /// ticks). It does the warship's missile attack x (20 - armour) / 20, the city's
    /// javelin rule (the javelin's action at 0x49a880).
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
            m.amount = stats.missile_attack;
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

    /// A warship rams a ship of the other side on its tile or the one ahead: its full
    /// attack after a straight run of its model's k tiles, else a fifth of it, less 20
    /// against a city warship holding position, divided by the angle of the blow
    /// (FUN_004a6160, and the enemy's FUN_004ae2c0). The two ships are knocked a
    /// point off their headings. True if it rammed.
    fn ram_blow(&mut self, fid: FigureId, rams: impl Fn(&crate::figures::Figure) -> bool) -> bool {
        let stats = self.fighter_stats(fid);
        let Some(f) = self.figures.get(fid) else { return false };
        let Some(s) = f.ship.as_deref() else { return false };
        if s.ram_cool > 0 {
            return false;
        }
        let (dir, straight) = (f.direction, s.straight);
        let ahead = (f.x + NEIGHBOURS[dir as usize].0, f.y + NEIGHBOURS[dir as usize].1);
        let here = (f.x, f.y);
        let Some((target, tdir, braced)) = self
            .figures
            .iter()
            .find(|o| rams(o) && !o.dead && o.action != action::CORPSE && !o.ship.as_ref().is_some_and(|s| s.hidden) && ((o.x, o.y) == here || (o.x, o.y) == ahead))
            .map(|o| (o.id, o.direction, o.kind == WARSHIP && o.action == ship::HOLD))
        else {
            return false;
        };
        let force = if straight >= stats.ram_run.max(1) { stats.attack } else { stats.attack * RAM_WEAK_PCT / 100 };
        let force = if braced { force - HOLD_BRACE } else { force };
        let angle = RAM_ANGLE[tdir as usize % 8][dir as usize % 8] as i32;
        self.figure_sound(fid, 2);
        if force > 0 {
            self.hurt(target, force / angle.max(1));
        }
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
        f.foe = 0;
        f.route.clear();
        if let Some(s) = f.ship.as_deref_mut() {
            s.ram_cool = RAM_COOLDOWN;
            s.straight = 0;
        }
        true
    }

    /// The city's warship rams an enemy ship, and then, its quarry forgotten, seeks
    /// and destroys (FUN_004a6160).
    fn ram(&mut self, fid: FigureId) -> bool {
        if !self.ram_blow(fid, |o| is_enemy_ship(o.kind)) {
            return false;
        }
        self.figures.get_mut(fid).expect("present").action = ship::SEEK;
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
        let (pos, at, afloat) = ((f.x, f.y), (t.x, t.y), is_enemy_ship(t.kind));
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
        let exhausting = self.fighter_stats(fid).exhaust.max(1);
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("present");
        let idle = f.route.is_empty() && !f.moving;
        let Some(s) = f.ship.as_deref_mut() else { return };
        s.repath -= 1;
        s.chase += 1;
        if s.chase >= exhausting {
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
            ((f.x, f.y), (o.x, o.y), is_enemy_ship(o.kind))
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
            ship::DISEMBARK | ship::BACK => self.disembark_step(fid),
            ship::TRANSPORT_TO_YARD | ship::IN_REPAIR => self.repair_trip(fid),
            _ => {
                let f = self.figures.get_mut(fid).expect("present");
                f.action = ship::LAUNCHED;
                f.counter = 0;
            }
        }
    }

    /// A ship evading, with a ship of the other side near, runs from it: of three
    /// points about 10 tiles off, straight away and to either side, the one farthest
    /// from the enemy that open water leads to (FUN_004a5560, FUN_004a4850). The
    /// city's transports run from any enemy ship; the enemy's fleeing ships from the
    /// city's warships, transports and fishing boats. False if none was near.
    fn evade(&mut self, fid: FigureId) -> bool {
        let Some(f) = self.figures.get(fid) else { return false };
        let pos = (f.x, f.y);
        let city = is_city_ship(f.kind);
        let other = |k: u16| if city { is_enemy_ship(k) } else { is_city_ship(k) || k == FISHING_BOAT };
        let Some(enemy) = self
            .figures
            .iter()
            .filter(|o| other(o.kind) && !o.dead && o.action != action::CORPSE && !o.ship.as_ref().is_some_and(|s| s.hidden) && (o.x - pos.0).abs() <= EVADE_BOX && (o.y - pos.1).abs() <= EVADE_BOX)
            .map(|o| (o.x, o.y))
            .min_by_key(|&p| distance(p, pos))
        else {
            return false;
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
        true
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

    /// Whether ship `fid` is at the shipwright's with its hull at 15% or less, or away
    /// on the Kingdom's service, when it takes no orders.
    pub fn ship_laid_up(&self, fid: FigureId) -> bool {
        let Some(f) = self.figures.get(fid) else { return true };
        if f.ship.as_ref().is_some_and(|s| s.abroad.is_some()) {
            return true;
        }
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

    /// Sends an invading army by sea (FUN_00446a30): first its warships, then
    /// transports with up to sixteen of `men` each. Each warship waits 25 ticks more
    /// than the one before to set out, each transport 40 (the first of either 10).
    /// A foreign army's ships gather on free water by the point, an Egyptian army's
    /// warships on the point itself; the transports all make for one of the
    /// scenario's landing places, drawn at random.
    pub(crate) fn launch_sea_invasion(&mut self, army: usize, nation: u16, men: i32, spot: (i32, i32), warships: i32, egypt: bool) {
        let spot = self.nearest_water(spot).unwrap_or(spot);
        let month = self.time.month as i32;
        for i in 0..warships.max(0) {
            let kind = if egypt { EGYPT_WARSHIP } else { ENEMY_WARSHIP };
            let at = if egypt { spot } else { self.free_water(spot).unwrap_or(spot) };
            let fid = self.figures.spawn(kind, at.0, at.1, Travel::Water);
            if let Some(f) = self.figures.get_mut(fid) {
                f.cargo = nation;
                f.action = ship::LAUNCHED;
                f.counter = i * WARSHIP_STAGGER + FIRST_WAIT;
                f.speed = 2;
                f.ship = Some(Box::new(Ship { month, ..Default::default() }));
            }
        }
        let aim = match self.invasions.landings.len() {
            0 => self.entry_point,
            n => self.invasions.landings[self.rng.below(n as i32) as usize],
        };
        let landing = self.landing_place(spot, aim);
        let mut left = men;
        let mut n = 0;
        // Each shipload is a formation afloat: none sails without a free slot.
        while left > 0 && self.enemy_formations() < crate::invasions::ENEMY_FORMATION_SLOTS {
            let at = self.free_water(spot).unwrap_or(spot);
            let fid = self.figures.spawn(if egypt { EGYPT_TRANSPORT } else { ENEMY_TRANSPORT }, at.0, at.1, Travel::Water);
            if let Some(f) = self.figures.get_mut(fid) {
                f.cargo = nation;
                f.amount = left.min(TRANSPORT_LOAD);
                f.formation = 1000 + army as u16;
                f.slot = n;
                f.action = ship::LAUNCHED;
                f.counter = n as i32 * TRANSPORT_STAGGER + FIRST_WAIT;
                f.speed = 2;
                f.ship = Some(Box::new(Ship { landing: landing.or(Some(spot)), month, ..Default::default() }));
            }
            left -= TRANSPORT_LOAD;
            n += 1;
        }
    }

    /// The nearest open water to `p` with no one on it (FUN_004467a0).
    fn free_water(&self, p: (i32, i32)) -> Option<(i32, i32)> {
        let taken = |x: i32, y: i32| self.figures.iter().any(|f| (f.x, f.y) == (x, y) && !f.dead);
        (0..30).find_map(|r| {
            (-r..=r)
                .flat_map(move |dy| (-r..=r).map(move |dx| (p.0 + dx, p.1 + dy)))
                .filter(|&q| distance(q, p) == r)
                .find(|&(x, y)| crate::water::navigable(&self.map, x, y) && !taken(x, y))
        })
    }

    /// The nearest open water to `p`.
    fn nearest_water(&self, p: (i32, i32)) -> Option<(i32, i32)> {
        (0..30).find_map(|r| {
            (-r..=r).flat_map(move |dy| (-r..=r).map(move |dx| (p.0 + dx, p.1 + dy))).find(|&(x, y)| crate::water::navigable(&self.map, x, y))
        })
    }

    /// Where transports put ashore: open water beside land, reachable from `from`,
    /// nearest `aim`.
    fn landing_place(&self, from: (i32, i32), aim: (i32, i32)) -> Option<(i32, i32)> {
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

    /// An invader's ship's stats: its nation's Warship or Transport row, as the
    /// original copies them into the figure model for the scenario's enemy
    /// (FUN_004bc720); the Egyptian ships' own rows of the figure list.
    pub(crate) fn enemy_ship_stats(&self, f: &crate::figures::Figure) -> Option<crate::balance::UnitStats> {
        if !is_enemy_ship(f.kind) {
            return None;
        }
        if matches!(f.kind, EGYPT_WARSHIP | EGYPT_TRANSPORT) {
            return Some(self.balance.unit(f.kind));
        }
        let row = self.defs.armies.get(f.cargo as usize).and_then(|a| a.stats_row);
        Some(match row {
            Some(r) => self.balance.enemy_units.get(r + if is_enemy_warship(f.kind) { 3 } else { 4 }).copied().unwrap_or_default(),
            None => self.balance.unit(f.kind),
        })
    }

    /// Whether ship `fid` has no hull left, when it goes down (a ship whose model
    /// gives it no hull, as the Assyrians' warships, goes down at once). Without the
    /// figure model loaded, none is.
    pub(crate) fn holed(&self, fid: FigureId) -> bool {
        if self.balance.units.is_empty() {
            return false;
        }
        let hp = self.fighter_stats(fid).hp;
        self.figures.get(fid).is_some_and(|f| f.damage >= hp)
    }

    /// An enemy transport waits its turn, sails to its landing place and puts its
    /// men ashore, lies off the shore for 50 ticks and sails away by the river entry
    /// and exit (the action function at 0x4adf60). Holed to 15% or less before it
    /// lands, it keeps its men aboard and runs from the city's ships (FUN_004add90);
    /// sunk, it goes down with them.
    pub(crate) fn update_enemy_transport(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get_mut(fid) else { return };
        if f.action == action::CORPSE {
            self.sinking(fid);
            return;
        }
        if f.ship.is_none() {
            f.ship = Some(Box::default());
        }
        if self.holed(fid) {
            self.sink(fid);
            return;
        }
        let pct = self.hull_percent(fid);
        let f = self.figures.get_mut(fid).expect("present");
        if pct <= REPAIR_AT && !matches!(f.action, ship::ENEMY_EVADE | ship::LEAVING) {
            f.action = ship::ENEMY_EVADE;
            f.counter = EVADE_LOOK;
        }
        match f.action {
            ship::LAUNCHED => {
                f.counter -= 1;
                if f.counter <= 0 {
                    let to = f.ship.as_ref().and_then(|s| s.landing).unwrap_or((f.x, f.y));
                    f.action = ship::LANDING;
                    let map = &self.map;
                    f.go_to(map, to);
                    f.destination = Some(to);
                }
            }
            ship::LANDING => {
                if self.sail(fid, false) == Step::Moving {
                    return;
                }
                // Landed: the men go ashore beside the ship.
                let f = self.figures.get_mut(fid).expect("present");
                let (x, y, men, nation, army) = (f.x, f.y, f.amount, f.cargo, f.formation.saturating_sub(1000) as usize);
                f.amount = 0;
                f.action = ship::LEAVING;
                f.counter = UNLOAD_WAIT;
                if let Some(shore) = self.shore_near((x, y)) {
                    self.put_ashore(army, nation, men, shore);
                }
            }
            ship::LEAVING => {
                if f.counter > 0 {
                    f.counter -= 1;
                    if f.counter == 0 {
                        self.leave_by_river(fid);
                    }
                    return;
                }
                if self.sail(fid, false) != Step::Moving {
                    self.leave_by_river(fid);
                }
            }
            ship::ENEMY_EVADE => {
                self.sail(fid, false);
                let f = self.figures.get_mut(fid).expect("present");
                f.counter += 1;
                if f.counter >= EVADE_LOOK {
                    f.counter = 0;
                    self.evade(fid);
                }
            }
            _ => {
                f.action = ship::LAUNCHED;
                f.counter = 0;
            }
        }
    }

    /// An enemy ship going home: to the river entry, then on to the river exit, where
    /// it is gone.
    fn leave_by_river(&mut self, fid: FigureId) {
        let (entry, exit) = (self.river_entry(), self.river_exit());
        let map = &self.map;
        let Some(f) = self.figures.get_mut(fid) else { return };
        let Some(s) = f.ship.as_deref_mut() else { return };
        let leg = s.leg;
        s.leg = leg.saturating_add(1);
        let to = match leg {
            0 => entry.or(exit),
            1 => exit,
            _ => None,
        };
        match to {
            Some(t) if t != (f.x, f.y) && f.go_to(map, t) => {}
            _ if leg == 0 && exit.is_some() => self.leave_by_river(fid),
            _ => f.dead = true,
        }
    }

    /// An enemy warship's turn (types 93 and 100, the action functions at 0x4aee10
    /// and 0x4af970). Holed, it sinks; badly holed (15% or less), it flees; four
    /// months after it came or last found a fight it sails off by the river exit. It
    /// waits its turn to set out, then patrols the river between the entry, the first
    /// landing place and the exit, looking for a fight every 75 to 104 ticks. A
    /// foreign warship also shoots at whatever comes in range while it waits,
    /// patrols or flees (FUN_004aed10).
    pub(crate) fn update_enemy_warship(&mut self, fid: FigureId) {
        let stats = self.fighter_stats(fid);
        let month = self.time.month as i32;
        let Some(f) = self.figures.get_mut(fid) else { return };
        if f.action == action::CORPSE {
            self.sinking(fid);
            return;
        }
        if f.ship.is_none() {
            f.ship = Some(Box::new(Ship { month, ..Default::default() }));
        }
        if self.holed(fid) {
            self.sink(fid);
            return;
        }
        let pct = self.hull_percent(fid);
        let f = self.figures.get_mut(fid).expect("present");
        let kind = f.kind;
        let s = f.ship.as_deref_mut().expect("present");
        s.reload = (s.reload + 1).min(stats.missile_delay + 1);
        s.ram_cool = (s.ram_cool - 1).max(0);
        if s.fatigue > 0 {
            s.rest -= 1;
            if s.rest <= 0 {
                s.fatigue -= 1;
                s.rest = if s.fatigue > 0 { FATIGUE_TICKS } else { 0 };
            }
        }
        let leave = (s.month + STAY_MONTHS) % 12 == month;
        if pct <= REPAIR_AT && !matches!(f.action, ship::FLEE | ship::DEPART) {
            f.action = ship::FLEE;
            f.counter = 0;
            f.foe = 0;
        }
        if leave && f.action != ship::DEPART {
            f.action = ship::DEPART;
            f.foe = 0;
            let exit = self.river_exit();
            let map = &self.map;
            let f = self.figures.get_mut(fid).expect("present");
            if !exit.is_some_and(|e| e != (f.x, f.y) && f.go_to(map, e)) {
                f.dead = true;
                return;
            }
        }
        let act = self.figures.get(fid).map_or(0, |f| f.action);
        if kind == ENEMY_WARSHIP
            && matches!(act, ship::LAUNCHED | ship::FLEE | ship::PATROL)
            && let Some(t) = self.enemy_ship_target(fid, Some(stats.missile_range))
        {
            self.enemy_fire(fid, t);
        }
        match act {
            ship::LAUNCHED => {
                let f = self.figures.get_mut(fid).expect("present");
                f.counter -= 1;
                if f.counter <= 0 {
                    self.patrol_leg(fid, 0);
                }
            }
            ship::PATROL => {
                if self.sail(fid, false) != Step::Moving {
                    let leg = self.figures.get(fid).and_then(|f| f.ship.as_ref()).map_or(0, |s| s.leg);
                    self.patrol_leg(fid, (leg + 1) % 3);
                }
                let r = self.rng.below(PATROL_LOOK_RAND);
                let s = self.ship_state(fid).expect("present");
                s.look -= 1;
                if s.look <= 0 {
                    s.look = PATROL_LOOK + r;
                    s.engaged = false;
                    self.figures.get_mut(fid).expect("present").action = ship::HUNT;
                }
            }
            ship::HUNT => self.enemy_hunt(fid),
            ship::FLEE => {
                self.sail(fid, false);
                let f = self.figures.get_mut(fid).expect("present");
                f.counter -= 1;
                if f.counter <= 0 {
                    f.counter = EVADE_LOOK;
                    if !self.evade(fid) {
                        let leg = self.figures.get(fid).and_then(|f| f.ship.as_ref()).map_or(0, |s| s.leg);
                        self.patrol_leg(fid, leg);
                    }
                }
            }
            ship::DEPART => {
                if self.sail(fid, false) != Step::Moving {
                    self.figures.get_mut(fid).expect("present").dead = true;
                }
            }
            _ => self.patrol_leg(fid, 0),
        }
    }

    /// Sets an enemy warship on leg `leg` of its patrol: to the river entry, to the
    /// first landing place (or the exit, with none), or to the exit (FUN_004aeb00).
    fn patrol_leg(&mut self, fid: FigureId, leg: u8) {
        let from = self.figures.get(fid).map_or((0, 0), |f| (f.x, f.y));
        let to = match leg {
            0 => self.river_entry(),
            1 => self.invasions.landings.first().and_then(|&p| self.landing_place(from, p)),
            _ => None,
        }
        .or_else(|| self.river_exit());
        let map = &self.map;
        let Some(f) = self.figures.get_mut(fid) else { return };
        f.action = ship::PATROL;
        f.foe = 0;
        if let Some(s) = f.ship.as_deref_mut() {
            s.leg = leg;
            s.engaged = false;
            if s.look <= 0 {
                s.look = PATROL_LOOK;
            }
        }
        match to {
            Some(t) if t != (f.x, f.y) => {
                f.go_to(map, t);
            }
            _ => {
                f.route.clear();
                f.destination = Some((f.x, f.y));
            }
        }
    }

    /// What an enemy warship goes after, best first by the original's reckoning
    /// (FUN_004bcf70, FUN_004bce90): a loaded transport of the city's (25), the
    /// city's soldiers within its missile range (17), a warship (15), an empty
    /// transport (10), a trade ship or a fishing boat (5), less the distance to each,
    /// over the whole map; never a ship away on the Kingdom's service, nor men aboard
    /// a transport. `within` limits it to what is in missile range.
    fn enemy_ship_target(&self, fid: FigureId, within: Option<i32>) -> Option<FigureId> {
        let f = self.figures.get(fid)?;
        let from = (f.x, f.y);
        let range = self.fighter_stats(fid).missile_range;
        self.figures
            .iter()
            .filter(|o| !o.dead && o.action != action::CORPSE && !o.ship.as_ref().is_some_and(|s| s.abroad.is_some() || s.hidden))
            .filter_map(|o| {
                let d = distance(from, (o.x, o.y));
                let pri = match o.kind {
                    TRANSPORT if o.ship.as_ref().is_some_and(|s| s.aboard.is_some()) => 25,
                    TRANSPORT => 10,
                    WARSHIP => 15,
                    crate::docks::TRADE_SHIP | FISHING_BOAT => 5,
                    k if crate::military::is_soldier(k) && o.action != action::ABOARD && d <= range => 17,
                    _ => return None,
                };
                within.is_none_or(|r| d <= r).then_some((pri - d, o.id))
            })
            .filter(|&(score, _)| score > -SEEK_SEARCH)
            .max_by_key(|&(score, id)| (score, std::cmp::Reverse(id)))
            .map(|(_, id)| id)
    }

    /// An enemy warship looses a spear at `target` once it has reloaded (its model's
    /// ticks between shots), turning to face it (FUN_004b45c0 with the spear, 71).
    /// The spear carries the ship's missile attack for a ship it strikes.
    fn enemy_fire(&mut self, fid: FigureId, target: FigureId) {
        let stats = self.fighter_stats(fid);
        let spear = self.balance.unit(crate::military::SPEAR).missile_attack;
        let Some(to) = self.figures.get(target).map(|t| (t.x, t.y)) else { return };
        let Some(s) = self.ship_state(fid) else { return };
        if s.reload <= stats.missile_delay {
            return;
        }
        s.reload = 0;
        let f = self.figures.get_mut(fid).expect("present");
        let from = (f.x, f.y);
        let dir = crate::figures::direction_to(from, to);
        if !f.moving {
            f.direction = dir.unwrap_or(f.direction);
        }
        self.figure_sound(fid, 2);
        let missile = self.figures.spawn(crate::military::ARROW, from.0, from.1, Travel::Land);
        if let Some(m) = self.figures.get_mut(missile) {
            m.foe = target;
            m.amount = spear;
            m.cargo = stats.missile_attack.max(0) as u16;
            m.destination = Some(to);
            m.direction = dir.unwrap_or(0);
        }
    }

    /// An enemy warship on the attack (FUN_004ae650): it picks its quarry, and with
    /// none goes back to its patrol. Half the time a quarry of soldiers or of the
    /// city's warships and transports keeps it another five months. It holds its
    /// place against soldiers, shooting when in range; a ship it chases, shooting,
    /// and rams (FUN_004ae2c0). It looks again on reaching its quarry, after a ram,
    /// or every 50 ticks.
    fn enemy_hunt(&mut self, fid: FigureId) {
        let stats = self.fighter_stats(fid);
        let Some(f) = self.figures.get(fid) else { return };
        let (foe, pos) = (f.foe, (f.x, f.y));
        let (engaged, look, leg) = f.ship.as_ref().map_or((false, 0, 0), |s| (s.engaged, s.look, s.leg));
        let quarry = self.figures.get(foe).is_some_and(|o| !o.dead && o.action != action::CORPSE && o.action != action::ABOARD && !o.ship.as_ref().is_some_and(|s| s.hidden));
        if !engaged || look <= 0 || !quarry {
            let Some(t) = self.enemy_ship_target(fid, None) else {
                self.patrol_leg(fid, leg);
                return;
            };
            let keeps = self.figures.get(t).is_some_and(|o| crate::military::is_soldier(o.kind) || is_city_ship(o.kind));
            let stays = keeps && self.rng.below(2) == 0;
            let month = self.time.month as i32;
            let f = self.figures.get_mut(fid).expect("present");
            f.foe = t;
            let s = f.ship.as_deref_mut().expect("present");
            s.engaged = true;
            s.look = HUNT_LOOK;
            s.repath = 0;
            if stays {
                s.month = (month + 1) % 12;
            }
        }
        let f = self.figures.get_mut(fid).expect("present");
        let target = f.foe;
        if let Some(s) = f.ship.as_deref_mut() {
            s.look -= 1;
        }
        let Some((at, afloat)) = self.figures.get(target).map(|t| ((t.x, t.y), t.travel == Travel::Water)) else { return };
        if distance(pos, at) <= stats.missile_range {
            self.enemy_fire(fid, target);
        }
        if !afloat {
            let f = self.figures.get_mut(fid).expect("present");
            f.route.clear();
            return;
        }
        if self.ram_blow(fid, |o| matches!(o.kind, TRANSPORT | WARSHIP | FISHING_BOAT)) {
            if let Some(s) = self.ship_state(fid) {
                s.engaged = false;
            }
            return;
        }
        if distance(pos, at) <= 1
            && let Some(s) = self.ship_state(fid)
        {
            s.engaged = false;
        }
        self.pursue(fid, at);
    }

    /// Dry land within two tiles of a ship, where its men can wade ashore.
    fn shore_near(&self, (x, y): (i32, i32)) -> Option<(i32, i32)> {
        (1..=2).find_map(|r| {
            (-r..=r)
                .flat_map(move |dy| (-r..=r).map(move |dx| (x + dx, y + dy)))
                .find(|&(sx, sy)| crate::figures::passable(&self.map, Travel::Land, sx, sy) && !self.map.terrain_is(sx, sy, crate::map::terrain::WATER))
        })
    }

    /// Whether Kingdom request `i` comes from a city reached by sea: its route on
    /// the Kingdom map is a sea route (byte +0x140 of the route record is 2;
    /// FUN_00521aa0, FUN_0044d520).
    pub fn request_by_sea(&self, i: usize) -> bool {
        self.scenario_events.list.get(i).and_then(|e| usize::try_from(e.route).ok()).and_then(|r| self.trade.routes.get(r)).is_some_and(|r| r.sea)
    }

    /// Whether a call for troops from a city reached by sea is open.
    pub fn sea_troops_wanted(&self) -> bool {
        self.troops_wanted(true)
    }

    /// Whether a call for troops is open from a city reached by sea, or by land
    /// (FUN_00521a60).
    pub fn troops_wanted(&self, sea: bool) -> bool {
        self.scenario_events.open_requests().any(|(i, e)| e.resource == crate::scenario_events::TROOPS && self.request_by_sea(i) == sea)
    }

    /// Why a company or ship that would go by sea (`sea`), or by land, can't be
    /// marked for Kingdom service now: the pop-up the overseer shows (text group 5)
    /// and the mark cleared, as 0x40a390 and 0x40a600 do. By sea it needs a call from
    /// a city by sea, else "Land Troops Needed" (38) if a land call is open; by land
    /// a call from a city by land, else "Transport Needed" (36) if a sea call is.
    /// With no call at all, "No Troops Needed" (40).
    pub fn service_refusal(&self, sea: bool) -> Option<usize> {
        if self.troops_wanted(sea) {
            None
        } else if self.troops_wanted(!sea) {
            Some(if sea { 38 } else { 36 })
        } else {
            Some(40)
        }
    }

    /// Marks or unmarks warship `fid` for Kingdom service. It can be marked only while
    /// a city reached by sea is calling for troops; otherwise its mark is cleared
    /// (the navy overseer's switch at 0x40a600).
    pub fn toggle_ship_service(&mut self, fid: FigureId) -> bool {
        let wanted = self.sea_troops_wanted();
        let Some(f) = self.figures.get_mut(fid) else { return false };
        if f.kind != WARSHIP || f.action == action::CORPSE {
            return false;
        }
        let Some(s) = f.ship.as_deref_mut() else { return false };
        if s.abroad.is_some() {
            return false;
        }
        s.service = wanted && !s.service;
        wanted
    }

    /// The city's warships marked for Kingdom service, afloat and at home.
    pub fn service_warships(&self) -> Vec<FigureId> {
        self.figures.iter().filter(|f| f.kind == WARSHIP && !f.dead && f.action != action::CORPSE && f.ship.as_ref().is_some_and(|s| s.service && s.abroad.is_none())).map(|f| f.id).collect()
    }

    /// Whether the troops request `i` wants are ready to go (FUN_0044d520): for a city
    /// reached by land a marked company ashore; for one reached by sea a marked
    /// warship or a marked company aboard a transport.
    pub fn troops_ready(&self, i: usize) -> bool {
        self.troops_status(i) == TroopsStatus::Ready
    }

    /// What stands between request `i` and the troops it wants (FUN_0044d520, whose
    /// codes -4, -3, -6, -5 and -2 these are, in its order): by land, companies
    /// ashore and one of them marked; by sea, warships or companies aboard, and one
    /// of them marked. Companies and ships already away don't count.
    pub fn troops_status(&self, i: usize) -> TroopsStatus {
        if !self.request_by_sea(i) {
            if !self.home_companies(false, false) {
                TroopsStatus::NoCompanies
            } else if !self.home_companies(false, true) {
                TroopsStatus::NoneMarked
            } else {
                TroopsStatus::Ready
            }
        } else if !self.home_warships(false) && !self.home_companies(true, false) {
            TroopsStatus::NoShips
        } else if !self.kingdom_service_marked(true) {
            TroopsStatus::NoneMarkedAfloat
        } else {
            TroopsStatus::Ready
        }
    }

    /// Whether the city has a company with men at home (not away for the Kingdom),
    /// aboard a transport or ashore, and if `marked` in Kingdom service.
    fn home_companies(&self, aboard: bool, marked: bool) -> bool {
        self.military.companies.iter().enumerate().any(|(c, co)| co.fort != 0 && !co.soldiers.is_empty() && !self.company_away(c) && !self.military.sent_away(c) && (!marked || co.kingdom_service) && self.company_ship(c).is_some() == aboard)
    }

    /// Whether the city has a warship at home, and if `marked` in Kingdom service.
    fn home_warships(&self, marked: bool) -> bool {
        self.figures.iter().any(|f| f.kind == WARSHIP && !f.dead && f.action != action::CORPSE && f.ship.as_ref().is_some_and(|s| s.abroad.is_none() && (!marked || s.service)))
    }

    /// Whether anything at home is marked for Kingdom service to go by land (a
    /// company ashore) or by sea (a company aboard, or a warship): the counts at
    /// 0xea5ed6, and 0xea5ed9 with 0xea5ed5, that the dispatch button reads.
    pub fn kingdom_service_marked(&self, sea: bool) -> bool {
        if sea { self.home_companies(true, true) || self.home_warships(true) } else { self.home_companies(false, true) }
    }

    /// Whether company `c` is away at the Kingdom's battle, marched off or aboard a
    /// transport sent there.
    pub fn company_away(&self, c: usize) -> bool {
        self.military.companies.get(c).is_some_and(|co| co.abroad > 0) || self.company_ship(c).and_then(|t| self.figures.get(t)).and_then(|t| t.ship.as_ref()).is_some_and(|s| s.abroad.is_some())
    }

    /// Sends ship `fid` off to the Kingdom's battle for request `request`: it sails
    /// for the river entry and out of sight (actions 0x11 and 0x12).
    pub(crate) fn ship_to_battle(&mut self, fid: FigureId, request: usize) {
        self.leave_yard(fid);
        let entry = self.river_entry();
        let map = &self.map;
        let Some(f) = self.figures.get_mut(fid) else { return };
        f.foe = 0;
        f.action = ship::TO_BATTLE;
        if let Some(s) = f.ship.as_deref_mut() {
            s.abroad = Some(request);
            s.embark = None;
        }
        if !entry.is_some_and(|e| e != (f.x, f.y) && f.go_to(map, e)) {
            f.route.clear();
            f.destination = Some((f.x, f.y));
        }
    }

    /// A ship away on the Kingdom's service: it sails to the river entry and there
    /// passes out of sight until the battle is over.
    fn abroad_step(&mut self, fid: FigureId) {
        let act = self.figures.get(fid).map_or(0, |f| f.action);
        match act {
            ship::TO_BATTLE => {
                if self.sail(fid, false) != Step::Moving {
                    let f = self.figures.get_mut(fid).expect("present");
                    f.action = ship::AWAY;
                    f.route.clear();
                    f.moving = false;
                    if let Some(s) = f.ship.as_deref_mut() {
                        s.hidden = true;
                    }
                }
            }
            ship::AWAY => {}
            _ => {
                let request = self.figures.get(fid).and_then(|f| f.ship.as_ref()).and_then(|s| s.abroad).unwrap_or(0);
                self.ship_to_battle(fid, request);
            }
        }
    }

    /// The strength the ships away for request `request` bring to its battle: each
    /// warship a tenth of its hull (FUN_004b8330).
    pub(crate) fn warship_battle_strength(&self, request: usize) -> i32 {
        self.ships_abroad(request).into_iter().filter(|&s| self.figures.get(s).is_some_and(|f| f.kind == WARSHIP)).map(|s| self.hull_left(s) / HULL_PER_STRENGTH).sum()
    }

    fn hull_left(&self, fid: FigureId) -> i32 {
        let hp = self.fighter_stats(fid).hp;
        self.figures.get(fid).map_or(0, |f| (hp - f.damage).max(0))
    }

    /// The ships away for request `request`, still afloat, in the original's order.
    pub(crate) fn ships_abroad(&self, request: usize) -> Vec<FigureId> {
        let mut ids: Vec<FigureId> = self.figures.iter().filter(|f| is_city_ship(f.kind) && !f.dead && f.action != action::CORPSE && f.ship.as_ref().is_some_and(|s| s.abroad == Some(request))).map(|f| f.id).collect();
        ids.sort_unstable();
        ids
    }

    /// The battle's losses, `pct` of the warships' strength, come off their hulls, a
    /// ship at a time: each whose hull the loss uses up sinks, and the first with more
    /// hull than is left to lose takes the rest; a battle lost sinks them all
    /// (FUN_004b8890).
    pub(crate) fn warships_take_losses(&mut self, request: usize, pct: i32) {
        let ships: Vec<FigureId> = self.ships_abroad(request).into_iter().filter(|&s| self.figures.get(s).is_some_and(|f| f.kind == WARSHIP)).collect();
        let mut pool = ships.iter().map(|&s| self.hull_left(s) / HULL_PER_STRENGTH).sum::<i32>() * HULL_PER_STRENGTH * pct / 100;
        for s in ships {
            let hull = self.hull_left(s);
            if pct != 100 && hull > pool {
                if let Some(f) = self.figures.get_mut(s) {
                    f.damage += pool;
                }
                break;
            }
            pool -= hull;
            self.sink_abroad(s);
        }
    }

    /// A ship away at the Kingdom's battle is lost, out of sight.
    pub(crate) fn sink_abroad(&mut self, fid: FigureId) {
        let hp = self.fighter_stats(fid).hp;
        let Some(f) = self.figures.get_mut(fid) else { return };
        f.damage = hp;
        f.action = action::CORPSE;
        f.counter = 0;
        f.route.clear();
        f.moving = false;
        if let Some(s) = f.ship.as_deref_mut() {
            s.abroad = None;
            s.hidden = true;
        }
    }

    /// The ships away for request `request` come home (FUN_004b8690): a warship to
    /// the water by its wharf, then on to its mooring as if just launched; a
    /// transport to the water nearest its company's old post, to put the company
    /// ashore there and sail home.
    pub(crate) fn ships_come_home(&mut self, request: usize) {
        for fid in self.ships_abroad(request) {
            let Some(f) = self.figures.get_mut(fid) else { continue };
            let (kind, home, pos) = (f.kind, f.home, (f.x, f.y));
            let aboard = f.ship.as_ref().and_then(|s| s.aboard);
            if let Some(s) = f.ship.as_deref_mut() {
                s.abroad = None;
                s.hidden = false;
            }
            let post = aboard.and_then(|c| self.military.companies.get(c)).map(|co| co.standard_tile);
            let to = if kind == WARSHIP { self.launch_tile(home) } else { post.and_then(|p| self.water_near(pos, p)) };
            let map = &self.map;
            let f = self.figures.get_mut(fid).expect("present");
            if kind == TRANSPORT
                && let Some(s) = f.ship.as_deref_mut()
            {
                s.landing = post;
            }
            f.action = if kind == TRANSPORT && post.is_none() { ship::TRANSPORT_HOME } else { ship::BACK };
            if !to.is_some_and(|t| t != pos && f.go_to(map, t)) {
                f.route.clear();
                f.destination = Some(pos);
            }
            if kind == TRANSPORT && post.is_none() {
                self.sail_home(fid);
            }
        }
    }

    /// Open water a ship at `from` can reach, nearest `tile`.
    fn water_near(&self, from: (i32, i32), tile: (i32, i32)) -> Option<(i32, i32)> {
        let reach = crate::water::reachable(&self.map, from);
        let w = self.map.width;
        (0..60).find_map(|r| {
            (-r..=r)
                .flat_map(move |dy| (-r..=r).map(move |dx| (tile.0 + dx, tile.1 + dy)))
                .filter(|&q| distance(q, tile) == r)
                .find(|&(x, y)| self.map.contains(x, y) && reach[(y * w + x) as usize])
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

    /// Ticks the town until a ship of kind `kind` is at its post, and gives it.
    fn ship_at_post(world: &mut World, kind: u16) -> FigureId {
        let post = if kind == WARSHIP { ship::ENGAGE } else { ship::EVADE };
        for _ in 0..(80 * crate::time::TICKS_PER_DAY) {
            if let Some(f) = world.figures.iter().find(|f| f.kind == kind && f.action == post) {
                return f.id;
            }
            world.tick();
        }
        panic!("no ship of kind {kind} at its post");
    }

    /// Puts a company of four archers by the north bank aboard transport `t`.
    fn company_aboard(world: &mut World, t: FigureId) -> Vec<FigureId> {
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
        let c = world.military.companies.len() - 1;
        assert!(matches!(world.apply(&Command::Embark { ship: t, company: c }), Outcome::Done { .. }));
        for _ in 0..2000 {
            world.tick();
        }
        assert_eq!(world.company_ship(c), Some(t));
        soldiers
    }

    /// A call for troops against `enemy` from a city by sea whose route is 805
    /// pixels long (two months' sailing), due in three months.
    fn sea_call(world: &mut World, enemy: i32) -> usize {
        let route = world.trade.routes.len();
        world.trade.routes.push(crate::trade::TradeRoute { points: vec![(0, 0), (800, 0)], step: 5, sea: true, ..Default::default() });
        world.trade.cities.push(crate::trade::TradeCity { sea: true, route: route as u8, ..Default::default() });
        let i = world.scenario_events.request_troops_now(enemy);
        let due = world.month_count() + 3;
        let e = &mut world.scenario_events.list[i];
        e.city = Some(world.trade.cities.len() - 1);
        e.route = route as i32;
        e.due = Some(due);
        assert_eq!(world.request_travel_months(i), 2);
        i
    }

    #[test]
    fn enemy_warships_sail_with_a_sea_invasion_and_fight_the_navy() {
        let Some(mut world) = navy_town() else { return };
        let ours = ship_at_post(&mut world, WARSHIP);
        world.invasions.sea_points = vec![world.river_entry().expect("river")];
        world.invade_by_sea_now(crate::invasions::invader::ENEMY, 16, 9, 2);
        let theirs: Vec<FigureId> = world.figures.iter().filter(|f| f.kind == ENEMY_WARSHIP).map(|f| f.id).collect();
        assert_eq!(theirs.len(), 2);
        // Their hulls are the nation's Warship row.
        let row = world.defs.armies[world.invasions.nation as usize].stats_row.expect("row") + 3;
        assert_eq!(world.fighter_stats(theirs[0]).hp, world.balance.enemy_units[row].hp);
        // The second sets out 25 ticks after the first.
        assert_eq!(theirs.iter().map(|&s| world.figures.get(s).expect("ship").counter).collect::<Vec<_>>(), [10, 35]);
        // They find our warship and trade blows with it.
        let (mut hurt_ours, mut hurt_theirs) = (false, false);
        for _ in 0..(40 * crate::time::TICKS_PER_DAY) {
            world.tick();
            hurt_ours |= world.figures.get(ours).is_none_or(|f| f.kind != WARSHIP || f.damage > 0);
            hurt_theirs |= theirs.iter().any(|&s| world.figures.get(s).is_none_or(|f| f.kind != ENEMY_WARSHIP || f.damage > 0));
            if hurt_ours && hurt_theirs {
                break;
            }
        }
        assert!(hurt_ours && hurt_theirs, "ours hurt {hurt_ours}, theirs hurt {hurt_theirs}");
    }

    #[test]
    fn warships_and_a_company_aboard_answer_a_call_from_a_city_by_sea() {
        let Some(mut world) = navy_town() else { return };
        let warship = ship_at_post(&mut world, WARSHIP);
        let transport = ship_at_post(&mut world, TRANSPORT);
        let soldiers = company_aboard(&mut world, transport);
        let c = world.military.companies.len() - 1;
        let i = sea_call(&mut world, 20);
        // Nothing marked, nothing to send.
        assert!(!world.can_send_request(i));
        world.military.companies[c].kingdom_service = true;
        assert!(matches!(world.apply(&Command::ShipService(warship)), Outcome::Done { .. }));
        assert!(world.can_send_request(i));
        assert!(world.dispatch_request(i));
        // They sail for the river entry and out of sight, taking no orders.
        for _ in 0..600 {
            world.tick();
        }
        for s in [warship, transport] {
            let f = world.figures.get(s).expect("ship");
            assert!(f.action == ship::AWAY && f.ship.as_ref().is_some_and(|s| s.hidden), "{} {:?}", f.action, f.ship);
        }
        assert!(!world.order_ship(warship, ShipOrder::Hold));
        assert!(world.company_away(c));
        // Strength: the archers (4 x 100 / 100) and a tenth of the warship's hull.
        let hull = world.fighter_stats(warship).hp;
        assert_eq!(world.military.battle_for(i).map(|b| b.strength), Some(4 + hull / 10));
        // Fought in the request's due month, three months on, won 34 to 20 (41% ahead:
        // a quarter lost), and two months' sailing home.
        let mut fought = false;
        for _ in 0..(8 * 31 * crate::time::TICKS_PER_DAY) {
            world.tick();
            fought |= world.military.battle_for(i).is_some_and(|b| b.fought && !b.late);
            if world.military.battles.is_empty() {
                break;
            }
        }
        assert!(fought && world.military.battles.is_empty());
        assert_eq!(world.scenario_events.list[i].state, crate::scenario_events::state::RECEIVED);
        assert!(world.notices.log.iter().any(|n| n.key == "message_troops_return_victorious"));
        let f = world.figures.get(warship).expect("warship");
        assert_eq!(f.damage, hull / 10 * 10 * 25 / 100, "the warship takes the battle's losses");
        assert!(!f.ship.as_ref().is_some_and(|s| s.hidden || s.abroad.is_some()));
        assert_eq!(world.military.companies[c].soldiers.len(), 3, "a quarter of four lost");
        assert_eq!(soldiers.iter().filter(|&&s| world.figures.get(s).is_some_and(|f| !f.dead)).count(), 3);
        // The transport puts the company back ashore by its old post.
        for _ in 0..3000 {
            world.tick();
        }
        assert_eq!(world.company_ship(c), None);
        let men: Vec<_> = world.military.companies[c].soldiers.iter().map(|&s| world.figures.get(s).map(|f| (f.x, f.y, f.action))).collect();
        assert!(men.iter().all(|m| m.is_some_and(|(_, y, a)| a != action::ABOARD && y < 112)), "{men:?}");
    }

    #[test]
    fn a_lost_battle_sinks_the_warships_and_the_company_aboard() {
        let Some(mut world) = navy_town() else { return };
        let warship = ship_at_post(&mut world, WARSHIP);
        let transport = ship_at_post(&mut world, TRANSPORT);
        company_aboard(&mut world, transport);
        let c = world.military.companies.len() - 1;
        let i = sea_call(&mut world, 500);
        world.military.companies[c].kingdom_service = true;
        assert!(world.toggle_ship_service(warship));
        assert!(world.dispatch_request(i));
        for _ in 0..(5 * 31 * crate::time::TICKS_PER_DAY) {
            world.tick();
        }
        assert!(world.military.battles.is_empty());
        assert!(world.figures.get(warship).is_none() && world.figures.get(transport).is_none());
        assert!(world.military.companies[c].soldiers.is_empty());
    }

    #[test]
    fn warships_sent_too_late_turn_back_without_fighting() {
        let Some(mut world) = navy_town() else { return };
        let warship = ship_at_post(&mut world, WARSHIP);
        let i = sea_call(&mut world, 1);
        // Five months' sailing (2005 pixels) to a battle due in two.
        let route = world.scenario_events.list[i].route as usize;
        world.trade.routes[route].points = vec![(0, 0), (2000, 0)];
        let due = world.month_count() + 2;
        world.scenario_events.list[i].due = Some(due);
        assert_eq!(world.request_travel_months(i), 5);
        assert!(world.toggle_ship_service(warship));
        assert!(world.dispatch_request(i));
        let mut late = None;
        for _ in 0..(5 * 31 * crate::time::TICKS_PER_DAY) {
            world.tick();
            if late.is_none() {
                late = world.military.battle_for(i).filter(|b| b.late).map(|b| (b.months, world.month_count()));
            }
            if world.military.battles.is_empty() {
                break;
            }
        }
        // At the due month they were still four months out: too late, and a month's
        // sailing back (five less the four still to go).
        assert_eq!(late, Some((1, due)));
        assert!(world.military.battles.is_empty());
        assert_eq!(world.figures.get(warship).expect("warship").damage, 0, "no battle, no losses");
        assert!(world.notices.log.iter().any(|n| n.key == "message_troops_return_failed"));
        assert_eq!(world.scenario_events.list[i].state, crate::scenario_events::state::FAILED);
    }

    #[test]
    fn a_call_no_one_answers_is_refused_on_its_due_month() {
        let Some(mut world) = navy_town() else { return };
        let i = sea_call(&mut world, 1);
        for _ in 0..(4 * 31 * crate::time::TICKS_PER_DAY) {
            world.tick();
        }
        let e = &world.scenario_events.list[i];
        assert!(!e.active);
        assert_eq!(e.state, crate::scenario_events::state::FAILED);
        assert!(!e.overdue, "no grace months for troops");
    }
}
