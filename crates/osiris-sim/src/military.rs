//! The army. Each fort holds a company of up to sixteen soldiers of its kind
//! (infantry, archers or charioteers) with a standard bearer, mustered on the parade
//! ground beside it. A recruiter enlists men for the nearest fort short of them,
//! outfitting each infantryman with a load of weapons and each charioteer with a load
//! of chariots; archers bring their own bows. The player sends a company to a spot
//! (it forms up around its standard) or back to its fort.
//!
//! Fighting: a soldier or invader next to an enemy strikes it once his blow comes
//! round (24 ticks); the blow does its attack less the target's armour. Archers shoot
//! at enemies in range every so often; a missile does its attack less the target's
//! armour against missiles. A figure dies when its damage passes its hit points, and
//! lies on the field a while before it is gone.
//!
//! Orders: a company holds its ground in tight or loose formation, fighting only what
//! comes at it: tight, its men fight better but suffer more from missiles; loose, they
//! cover more ground and suffer less from missiles but fight worse. (How much better
//! or worse is not known from the original; these are estimates.) Or it engages
//! enemies that come near, in formation; or breaks ranks to mop up every enemy it can
//! find (charioteers charge the same way).
//!
//! Morale: every man lost shakes his side, the more so the bigger the share of it
//! that fell; a company rests its spirits at the fort month by month and loses heart
//! when kept out long. A company whose morale breaks runs home, and a broken army
//! runs for the edge of the map.

use crate::balance::UnitStats;
use crate::buildings::BuildingId;
use crate::figures::{FigureId, Step, Travel};
use crate::world::World;

pub const ARCHER: u16 = 11;
pub const CHARIOTEER: u16 = 12;
pub const INFANTRY: u16 = 13;
pub const STANDARD_BEARER: u16 = 14;
pub const ARROW: u16 = 59;
pub const JAVELIN: u16 = 60;

pub const FORT_CHARIOTEERS: u16 = 40;
pub const FORT_ARCHERS: u16 = 44;
pub const FORT_INFANTRY: u16 = 45;
pub const FORT_GROUND: u16 = 54;
pub const RECRUITER: u16 = 95;
const ACADEMIES: [u16; 3] = [94, 185, 186];
const WEAPONS: u16 = 10;
const CHARIOTS: u16 = 28;

/// What a recruiter must hand a new soldier of `kind`, if anything.
fn outfit(kind: u16) -> Option<u16> {
    match kind {
        INFANTRY => Some(WEAPONS),
        CHARIOTEER => Some(CHARIOTS),
        _ => None,
    }
}

/// Soldiers a company holds.
pub const COMPANY_SIZE: usize = 16;
/// Ticks a blow takes to land.
const BLOW_TICKS: u16 = 24;
/// Ticks a fallen figure lies on the field.
const CORPSE_TICKS: i32 = 200;
/// Where the parade ground sits beside its fort.
const GROUND_OFFSET: (i32, i32) = (3, -1);
/// Morale at or below which a side breaks and runs.
pub const BROKEN_MORALE: i32 = 20;

/// Morale lost for a death, by the share of the side it was (percent).
pub fn morale_loss(share_pct: i32) -> i32 {
    match share_pct {
        p if p < 8 => 5,
        p if p < 10 => 7,
        p if p < 14 => 10,
        p if p < 20 => 12,
        p if p < 30 => 15,
        _ => 20,
    }
}

/// The highest morale a company of `kind` reaches; training raises it by 20.
fn morale_cap(kind: u16, trained: bool) -> i32 {
    (if kind == INFANTRY { 80 } else { 60 }) + if trained { 20 } else { 0 }
}

/// Action states of fighters.
pub mod action {
    pub const AT_REST: u16 = 80;
    pub const GOING_TO_FORT: u16 = 81;
    pub const GOING_TO_STANDARD: u16 = 83;
    pub const GOING_TO_ACADEMY: u16 = 85;
    pub const GOING_ABROAD: u16 = 87;
    pub const CHASING: u16 = 86;
    pub const AT_STANDARD: u16 = 84;
    pub const ATTACK: u16 = 90;
    pub const CORPSE: u16 = 149;
}

/// A fort's company.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Company {
    pub fort: BuildingId,
    pub ground: BuildingId,
    /// Soldier figure type.
    pub kind: u16,
    pub soldiers: Vec<FigureId>,
    /// Recruits on their way from the recruiter.
    pub recruits: Vec<FigureId>,
    pub standard: FigureId,
    /// Where the company forms up, when it is out of its fort.
    pub standard_tile: (i32, i32),
    pub at_fort: bool,
    pub morale: i32,
    /// Months out of the fort.
    #[serde(default)]
    pub months_away: i32,
    /// Its men have trained at a military academy, which steadies them.
    #[serde(default)]
    pub trained: bool,
    /// Marked for Kingdom service: it answers Pharaoh's calls for troops.
    #[serde(default)]
    pub kingdom_service: bool,
    /// Soldiers away fighting for the Kingdom.
    #[serde(default)]
    pub abroad: i32,
    #[serde(default)]
    pub order: Order,
    /// Charioteers' horses: the ticks of charging they have left in them.
    #[serde(default = "full_wind")]
    pub wind: i32,
    /// The tick the horses last ran or rested, so a company tires once a tick.
    #[serde(default)]
    pub wind_tick: u64,
}

/// Ticks a company of charioteers can charge at top speed before its horses tire,
/// and how many ticks of rest win back one. (The manual only says the horses tire
/// after a great distance and must rest; these amounts are placeholders.)
const WIND: i32 = 300;
const REST_PER_WIND: u64 = 2;

fn full_wind() -> i32 {
    WIND
}

/// A company's standing orders.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Order {
    #[default]
    HoldTight,
    HoldLoose,
    Engage,
    MopUp,
    Charge,
}

impl Order {
    /// How far the company goes after enemies, in tiles (0: only those at hand).
    fn reach(self) -> i32 {
        match self {
            Order::HoldTight | Order::HoldLoose => 0,
            Order::Engage => 6,
            Order::MopUp | Order::Charge => 20,
        }
    }
}

/// Troops sent to fight for the Kingdom, and the request they answer.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DistantBattle {
    pub request: usize,
    pub enemy: i32,
    pub strength: i32,
    pub companies: Vec<usize>,
    /// Months until the battle, or, once fought, until the survivors are home.
    pub months: i32,
    pub fought: bool,
}

/// Who is on the field this tick, gathered once so fighters needn't search every
/// figure: the city's fighters (soldiers, constables, sentries) and the invaders.
#[derive(Debug, Clone, Default)]
pub struct Combatants {
    pub defenders: Vec<(FigureId, i32, i32)>,
    pub invaders: Vec<(FigureId, i32, i32)>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Military {
    pub companies: Vec<Company>,
    #[serde(default)]
    pub battle: Option<DistantBattle>,
}

/// Months troops take to reach a distant battle, and to come home.
const TRAVEL_MONTHS: i32 = 2;

/// Share of the troops lost in a won battle, by how far they outnumbered the enemy
/// (their surplus as a percentage of their strength).
fn battle_losses(advantage_pct: i32) -> i32 {
    match advantage_pct {
        a if a < 10 => 70,
        a if a < 25 => 50,
        a if a < 50 => 25,
        a if a < 75 => 15,
        a if a < 100 => 10,
        a if a < 150 => 5,
        _ => 0,
    }
}

/// The fort kinds and the soldiers they hold.
pub fn fort_soldier(k: u16) -> Option<u16> {
    match k {
        FORT_CHARIOTEERS => Some(CHARIOTEER),
        FORT_ARCHERS => Some(ARCHER),
        FORT_INFANTRY => Some(INFANTRY),
        _ => None,
    }
}

pub fn is_soldier(k: u16) -> bool {
    matches!(k, ARCHER | CHARIOTEER | INFANTRY)
}

/// Where a soldier stands in a company's double line around its standard, or on the
/// parade ground at rest.
fn slot_offset(slot: u8, order: Order) -> (i32, i32) {
    let s = slot as i32;
    match order {
        // Four abreast, four deep.
        Order::HoldTight => (s % 4 - 2, s / 4 - 2),
        // Spread out, a tile between each man.
        Order::HoldLoose => (2 * (s % 4) - 4, 2 * (s / 4) - 4),
        _ => (s % 8 - 4, s / 8),
    }
}

fn ground_slot(ground: (i32, i32), slot: u8) -> (i32, i32) {
    let s = slot as i32;
    (ground.0 + s % 4, ground.1 + s / 4)
}

impl World {
    /// A fighter's stats: soldiers and the city's own from the figure model, invaders
    /// from their nation's rows.
    pub fn fighter_stats(&self, fid: FigureId) -> UnitStats {
        let Some(f) = self.figures.get(fid) else { return UnitStats::default() };
        if let Some(s) = self.invader_stats(f) {
            return s;
        }
        self.balance.unit(f.kind)
    }

    /// Places a fort's parade ground and musters its company.
    pub(crate) fn place_fort(&mut self, id: BuildingId) {
        let Some(b) = self.buildings.get(id) else { return };
        let Some(kind) = fort_soldier(b.kind) else { return };
        let (gx, gy) = (b.x + GROUND_OFFSET.0, b.y + GROUND_OFFSET.1);
        let ground_image = self.defs.building(b.kind).and_then(|d| d.anims.get("ground")).map(|a| a.image);
        let ground = self.create_building(FORT_GROUND, gx, gy);
        if let Some(image) = ground_image {
            self.set_building_image(ground, image);
        }
        // Soldiers drill on the parade ground: it can be walked on.
        for yy in gy..gy + 4 {
            for xx in gx..gx + 4 {
                self.map.terrain.update(xx, yy, |t| t & !crate::map::terrain::BUILDING);
            }
        }
        let standard = self.figures.spawn(STANDARD_BEARER, gx, gy, Travel::Land);
        let company = self.military.companies.len() as u16 + 1;
        if let Some(f) = self.figures.get_mut(standard) {
            f.formation = company;
            f.home = id;
            f.action = action::AT_REST;
        }
        self.military.companies.push(Company { fort: id, ground, kind, standard, standard_tile: (gx, gy), at_fort: true, morale: 50, wind: WIND, ..Default::default() });
    }

    /// Whether a fort's parade ground fits beside it.
    pub(crate) fn fort_ground_clear(&self, x: i32, y: i32) -> bool {
        let (gx, gy) = (x + GROUND_OFFSET.0, y + GROUND_OFFSET.1);
        (gy..gy + 4).all(|yy| (gx..gx + 4).all(|xx| self.map.contains(xx, yy) && !self.map.terrain_is(xx, yy, crate::map::mask::NOT_CLEAR)))
    }

    /// When a fort goes, its ground goes with it and its company disbands.
    pub(crate) fn remove_fort(&mut self, id: BuildingId) {
        let Some(c) = self.military.companies.iter().position(|c| c.fort == id) else { return };
        let company = self.military.companies[c].clone();
        for fid in company.soldiers.iter().chain(&company.recruits).chain(std::iter::once(&company.standard)) {
            if let Some(f) = self.figures.get_mut(*fid) {
                f.dead = true;
            }
        }
        self.military.companies[c].fort = 0;
        self.military.companies[c].soldiers.clear();
        self.military.companies[c].recruits.clear();
        if self.buildings.get(company.ground).is_some() {
            self.demolish(company.ground);
        }
    }

    /// Daily: each staffed recruiter with a hundred weapons turns them into a soldier
    /// for the nearest fort short of men.
    pub(crate) fn update_recruiters(&mut self) {
        let recruiters: Vec<BuildingId> = self.buildings.iter().filter(|b| b.kind == RECRUITER && b.workers > 0 && b.road.is_some()).map(|b| b.id).collect();
        for r in recruiters {
            let Some(b) = self.buildings.get(r) else { continue };
            let (rx, ry) = (b.x, b.y);
            let road = b.road.expect("checked");
            let has = |res: u16| b.stock.get(res as usize).copied().unwrap_or(0) >= crate::economy::LOAD;
            // The nearest fort short of men whom this recruiter can outfit.
            let needing = self
                .military
                .companies
                .iter()
                .enumerate()
                .filter(|(_, c)| c.fort != 0 && c.soldiers.len() + c.recruits.len() + (c.abroad as usize) < COMPANY_SIZE)
                .filter(|(_, c)| outfit(c.kind).is_none_or(has))
                .filter_map(|(i, c)| self.buildings.get(c.fort).map(|f| (i, (f.x - rx).abs() + (f.y - ry).abs())))
                .min_by_key(|&(_, d)| d)
                .map(|(i, _)| i);
            let Some(c) = needing else { continue };
            if let Some(res) = outfit(self.military.companies[c].kind) {
                self.buildings.get_mut(r).expect("present").stock[res as usize] -= crate::economy::LOAD;
            }
            let company = &self.military.companies[c];
            let kind = company.kind;
            let used: Vec<u8> = company.soldiers.iter().chain(&company.recruits).filter_map(|&s| self.figures.get(s).map(|f| f.slot)).collect();
            let slot = (0..COMPANY_SIZE as u8).find(|s| !used.contains(s)).unwrap_or(0);
            let fid = self.figures.spawn(kind, road.0, road.1, Travel::Land);
            if let Some(f) = self.figures.get_mut(fid) {
                f.formation = c as u16 + 1;
                f.slot = slot;
                f.home = self.military.companies[c].fort;
            }
            self.military.companies[c].recruits.push(fid);
            // A recruit trains at a military academy on his way, if one is half staffed.
            let academy = self
                .buildings
                .iter()
                .filter(|a| ACADEMIES.contains(&a.kind) && a.road.is_some() && a.workers * 2 >= self.workers_needed(a.kind).max(1))
                .min_by_key(|a| (a.x - rx).abs() + (a.y - ry).abs())
                .and_then(|a| a.road);
            let map = &self.map;
            let training = match (academy, self.figures.get_mut(fid)) {
                (Some(to), Some(f)) => {
                    let ok = f.go_to(map, to);
                    if ok {
                        f.action = action::GOING_TO_ACADEMY;
                    }
                    ok
                }
                _ => false,
            };
            if !training {
                self.send_to_post(fid);
            }
        }
    }

    /// Where a soldier belongs now: his place on the parade ground, or around the
    /// standard when the company is out.
    fn post_of(&self, fid: FigureId) -> Option<(i32, i32)> {
        let f = self.figures.get(fid)?;
        let c = self.military.companies.get(f.formation.checked_sub(1)? as usize)?;
        if c.at_fort {
            let g = self.buildings.get(c.ground)?;
            Some(ground_slot((g.x, g.y), f.slot))
        } else {
            let (dx, dy) = slot_offset(f.slot, c.order);
            Some((c.standard_tile.0 + dx, c.standard_tile.1 + dy))
        }
    }

    fn send_to_post(&mut self, fid: FigureId) {
        let Some(post) = self.post_of(fid) else { return };
        let at_fort = self.figures.get(fid).and_then(|f| self.military.companies.get(f.formation as usize - 1)).is_some_and(|c| c.at_fort);
        let map = &self.map;
        let Some(f) = self.figures.get_mut(fid) else { return };
        f.action = if at_fort { action::GOING_TO_FORT } else { action::GOING_TO_STANDARD };
        if (f.x, f.y) == post {
            f.action = if at_fort { action::AT_REST } else { action::AT_STANDARD };
        } else if !f.go_to(map, post) {
            // Nowhere to stand there: stay put.
            f.action = if at_fort { action::AT_REST } else { action::AT_STANDARD };
        }
    }

    /// Orders a company out to form up around `tile`.
    pub fn move_company(&mut self, company: usize, tile: (i32, i32)) {
        let Some(c) = self.military.companies.get_mut(company) else { return };
        c.at_fort = false;
        c.standard_tile = tile;
        let (standard, soldiers) = (c.standard, c.soldiers.clone());
        let map = &self.map;
        if let Some(f) = self.figures.get_mut(standard) {
            f.action = action::GOING_TO_STANDARD;
            f.go_to(map, tile);
        }
        for s in soldiers {
            if self.figures.get(s).is_some_and(|f| f.action != action::ATTACK) {
                self.send_to_post(s);
            }
        }
    }

    /// Orders a company back to its fort.
    pub fn return_company(&mut self, company: usize) {
        let Some(c) = self.military.companies.get_mut(company) else { return };
        c.at_fort = true;
        let (standard, soldiers, ground) = (c.standard, c.soldiers.clone(), c.ground);
        let home = self.buildings.get(ground).map(|g| (g.x, g.y));
        let map = &self.map;
        if let (Some(f), Some(home)) = (self.figures.get_mut(standard), home) {
            f.action = action::GOING_TO_FORT;
            f.go_to(map, home);
        }
        for s in soldiers {
            if let Some(f) = self.figures.get_mut(s) {
                f.foe = 0;
            }
            self.send_to_post(s);
        }
    }

    /// The company a figure belongs to, if it is one of the city's soldiers.
    pub fn company_of(&self, fid: FigureId) -> Option<usize> {
        let f = self.figures.get(fid)?;
        (is_soldier(f.kind) || f.kind == STANDARD_BEARER).then(|| f.formation.checked_sub(1).map(|c| c as usize)).flatten()
    }

    /// A soldier's turn: march to his post, stand, and fight what comes near.
    pub(crate) fn update_soldier(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (act, kind) = (f.action, f.kind);
        if act == action::CORPSE {
            self.update_corpse(fid);
            return;
        }
        // Recruits join the company when they reach it.
        if let Some(c) = self.company_of(fid)
            && let Some(i) = self.military.companies[c].recruits.iter().position(|&r| r == fid)
            && matches!(act, action::AT_REST | action::AT_STANDARD)
        {
            self.military.companies[c].recruits.remove(i);
            self.military.companies[c].soldiers.push(fid);
        }
        match act {
            action::GOING_TO_FORT | action::GOING_TO_STANDARD => {
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                match f.walk(map) {
                    Step::Moving => {}
                    _ => f.action = if act == action::GOING_TO_FORT { action::AT_REST } else { action::AT_STANDARD },
                }
                if act == action::GOING_TO_STANDARD {
                    self.engage(fid, 1);
                }
            }
            action::AT_STANDARD => {
                self.breathe(fid, false);
                if kind == ARCHER {
                    self.shoot_at_foes(fid);
                }
                self.engage(fid, 1);
                let reach = self.company_of(fid).and_then(|c| self.military.companies.get(c)).map_or(0, |c| c.order.reach());
                if reach > 0 && self.figures.get(fid).is_some_and(|f| f.action == action::AT_STANDARD) {
                    self.chase(fid, reach);
                }
            }
            action::CHASING => {
                self.engage(fid, 1);
                if self.figures.get(fid).is_some_and(|f| f.action == action::CHASING) {
                    let reach = self.company_of(fid).and_then(|c| self.military.companies.get(c)).map_or(0, |c| c.order.reach());
                    self.chase(fid, reach);
                }
            }
            action::AT_REST => self.breathe(fid, false),
            action::ATTACK => self.fight(fid),
            action::GOING_ABROAD => {
                // Marching out of the city; gone once past its edge.
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                if f.walk(map) != Step::Moving {
                    f.dead = true;
                    for c in &mut self.military.companies {
                        c.soldiers.retain(|&s| s != fid);
                    }
                }
            }
            action::GOING_TO_ACADEMY => {
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                if f.walk(map) != Step::Moving {
                    if let Some(c) = self.company_of(fid) {
                        self.military.companies[c].trained = true;
                    }
                    self.send_to_post(fid);
                }
            }
            _ => self.send_to_post(fid),
        }
    }

    /// Gathers this tick's combatants (at the start of the figures' turn).
    pub(crate) fn gather_combatants(&mut self) {
        let mut c = Combatants::default();
        for f in self.figures.iter().filter(|f| !f.dead && f.action != action::CORPSE) {
            if self.is_invader(f) {
                c.invaders.push((f.id, f.x, f.y));
            } else if is_soldier(f.kind) || matches!(f.kind, crate::crime::CONSTABLE | crate::defenses::TOWER_SENTRY) {
                c.defenders.push((f.id, f.x, f.y));
            }
        }
        self.combatants = c;
    }

    /// The nearest living enemy of a figure on the invaders' side (`invader`) or the
    /// city's, within `range` tiles: its id and tile.
    pub(crate) fn nearest_foe(&self, invader: bool, (x, y): (i32, i32), range: i32) -> Option<(FigureId, i32, i32)> {
        let list = if invader { &self.combatants.defenders } else { &self.combatants.invaders };
        list.iter()
            .filter(|&&(_, ox, oy)| (ox - x).abs() <= range && (oy - y).abs() <= range)
            .filter(|&&(id, _, _)| self.figures.get(id).is_some_and(|o| !o.dead && o.action != action::CORPSE))
            .min_by_key(|&&(_, ox, oy)| (ox - x).abs() + (oy - y).abs())
            .copied()
    }

    /// Takes on an enemy within `range` tiles, if there is one.
    fn engage(&mut self, fid: FigureId, range: i32) {
        let Some(f) = self.figures.get(fid) else { return };
        let (x, y) = (f.x, f.y);
        let mine = self.is_invader(f);
        let foe = self.nearest_foe(mine, (x, y), range).map(|o| o.0);
        if let Some(foe) = foe
            && let Some(f) = self.figures.get_mut(fid)
        {
            f.foe = foe;
            f.action = action::ATTACK;
            f.attack_tick = 0;
            f.route.clear();
        }
    }

    /// Whether a soldier is a charioteer charging with horses still fresh.
    fn charging(&self, fid: FigureId) -> bool {
        self.figures.get(fid).is_some_and(|f| f.kind == CHARIOTEER)
            && self.company_of(fid).and_then(|c| self.military.companies.get(c)).is_some_and(|c| c.order == Order::Charge && c.wind > 0)
    }

    /// Once a tick, a company's horses tire while it charges and recover while it
    /// doesn't.
    fn breathe(&mut self, fid: FigureId, running: bool) {
        let now = self.time.total_ticks;
        let Some(c) = self.company_of(fid).and_then(|c| self.military.companies.get_mut(c)) else { return };
        if c.wind_tick == now {
            return;
        }
        c.wind_tick = now;
        if running {
            c.wind = (c.wind - 1).max(0);
        } else if now.is_multiple_of(REST_PER_WIND) {
            c.wind = (c.wind + 1).min(WIND);
        }
    }

    /// A soldier under orders to go after enemies heads for the nearest within `reach`
    /// tiles, or back to his place when none is left. Charging charioteers go at the
    /// gallop until their horses tire, and then at half pace.
    fn chase(&mut self, fid: FigureId, reach: i32) {
        let Some(f) = self.figures.get(fid) else { return };
        let (x, y) = (f.x, f.y);
        let target = self.nearest_foe(false, (x, y), reach).map(|o| (o.1, o.2));
        let charioteer = f.kind == CHARIOTEER && self.company_of(fid).and_then(|c| self.military.companies.get(c)).is_some_and(|c| c.order == Order::Charge);
        let fresh = self.charging(fid);
        if charioteer && target.is_some() {
            self.breathe(fid, fresh);
        }
        let spent = charioteer && !fresh && self.time.total_ticks % 2 == 1;
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("present");
        match target {
            Some(to) => {
                f.action = action::CHASING;
                if !f.moving && f.destination != Some(to) {
                    f.go_to(map, to);
                }
                f.speed = if fresh { 2 } else { 1 };
                if !spent {
                    f.walk(map);
                }
                f.speed = 1;
            }
            None if f.action == action::CHASING => self.send_to_post(fid),
            None => {}
        }
    }

    /// The formation a soldier is holding, if he stands in his company's line.
    fn holding(&self, fid: FigureId) -> Option<Order> {
        let f = self.figures.get(fid)?;
        if !matches!(f.action, action::AT_STANDARD | action::ATTACK) {
            return None;
        }
        let c = self.military.companies.get(self.company_of(fid)?)?;
        (!c.at_fort && matches!(c.order, Order::HoldTight | Order::HoldLoose)).then_some(c.order)
    }

    /// Changes a company's orders; its men re-form at their new places.
    pub fn set_order(&mut self, company: usize, order: Order) {
        let Some(c) = self.military.companies.get_mut(company) else { return };
        c.order = order;
        if c.at_fort {
            return;
        }
        for s in c.soldiers.clone() {
            if self.figures.get(s).is_some_and(|f| matches!(f.action, action::AT_STANDARD | action::GOING_TO_STANDARD)) {
                self.send_to_post(s);
            }
        }
    }

    /// A blow comes round: the foe takes the attack less his armour.
    pub(crate) fn fight(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (foe, x, y) = (f.foe, f.x, f.y);
        let alive = self.figures.get(foe).filter(|o| !o.dead && o.action != action::CORPSE);
        let Some(o) = alive.filter(|o| (o.x - x).abs() <= 1 && (o.y - y).abs() <= 1) else {
            let f = self.figures.get_mut(fid).expect("present");
            f.foe = 0;
            f.action = 0;
            return;
        };
        let (ox, oy) = (o.x, o.y);
        let f = self.figures.get_mut(fid).expect("present");
        f.direction = crate::figures::direction_to((x, y), (ox, oy)).unwrap_or(f.direction);
        f.attack_tick += 1;
        if f.attack_tick < BLOW_TICKS {
            return;
        }
        f.attack_tick = 0;
        let attack = self.fighter_stats(fid).attack;
        let armor = self.fighter_stats(foe).armor
            + match self.holding(foe) {
                Some(Order::HoldTight) => 4,
                Some(Order::HoldLoose) => -2,
                _ => 0,
            };
        // A charge breaks the enemy's line: his armour does him no good.
        let armor = if self.charging(fid) { 0 } else { armor };
        self.hurt(foe, (attack - armor).max(0));
    }

    /// Adds damage to a figure; it falls when the damage passes its hit points, and
    /// its side's morale suffers.
    pub(crate) fn hurt(&mut self, fid: FigureId, damage: i32) {
        let hp = self.fighter_stats(fid).hp.max(1);
        let Some(f) = self.figures.get_mut(fid) else { return };
        f.damage += damage;
        if f.damage <= hp || f.action == action::CORPSE {
            return;
        }
        f.action = action::CORPSE;
        f.counter = 0;
        f.foe = 0;
        f.route.clear();
        f.moving = false;
        let formation = if f.kind == crate::navy::ENEMY_TRANSPORT { 0 } else { f.formation };
        if let Some(c) = self.company_of(fid).and_then(|c| self.military.companies.get_mut(c)) {
            let size = (c.soldiers.len() + c.recruits.len()).max(1) as i32;
            c.morale = (c.morale - morale_loss(100 / size)).max(0);
            if c.morale <= BROKEN_MORALE && !c.at_fort {
                let company = self.company_of(fid).expect("checked");
                self.return_company(company);
            }
        } else if formation >= 1000 {
            self.army_loses(formation as usize - 1000);
        }
    }

    /// The strength the companies marked for Kingdom service would bring to a distant
    /// battle: one a soldier, two if trained.
    pub fn kingdom_service_strength(&self) -> i32 {
        self.military.companies.iter().filter(|c| c.kingdom_service && c.fort != 0).map(|c| c.soldiers.len() as i32 * if c.trained { 2 } else { 1 }).sum()
    }

    pub fn toggle_kingdom_service(&mut self, company: usize) {
        if let Some(c) = self.military.companies.get_mut(company) {
            c.kingdom_service = !c.kingdom_service;
        }
    }

    /// The companies marked for Kingdom service march off to fight `enemy` for the
    /// request `request`.
    pub(crate) fn send_to_battle(&mut self, request: usize, enemy: i32) {
        let strength = self.kingdom_service_strength();
        let marked: Vec<usize> = (0..self.military.companies.len()).filter(|&c| self.military.companies[c].kingdom_service && !self.military.companies[c].soldiers.is_empty()).collect();
        let exit = self.exit_point;
        for &c in &marked {
            let soldiers = self.military.companies[c].soldiers.clone();
            self.military.companies[c].abroad = soldiers.len() as i32;
            self.military.companies[c].at_fort = false;
            let map = &self.map;
            for s in soldiers {
                if let Some(f) = self.figures.get_mut(s) {
                    f.action = action::GOING_ABROAD;
                    f.foe = 0;
                    if !f.go_to(map, exit) {
                        f.dead = true;
                    }
                }
            }
        }
        self.military.battle = Some(DistantBattle { request, enemy, strength, companies: marked, months: TRAVEL_MONTHS, fought: false });
    }

    /// Monthly: the troops abroad reach their battle and fight it; the survivors come
    /// home. A won battle meets the request; a lost one fails it.
    pub(crate) fn update_distant_battle(&mut self) {
        let Some(b) = self.military.battle.as_mut() else { return };
        b.months -= 1;
        if b.months > 0 {
            return;
        }
        let b = b.clone();
        if b.fought {
            // Home again: the survivors walk back to their forts.
            self.military.battle = None;
            let exit = self.exit_point;
            for &c in &b.companies {
                let n = std::mem::take(&mut self.military.companies[c].abroad);
                self.military.companies[c].at_fort = true;
                for _ in 0..n {
                    let kind = self.military.companies[c].kind;
                    let used: Vec<u8> = self.military.companies[c].soldiers.iter().chain(&self.military.companies[c].recruits).filter_map(|&s| self.figures.get(s).map(|f| f.slot)).collect();
                    let slot = (0..COMPANY_SIZE as u8).find(|s| !used.contains(s)).unwrap_or(0);
                    let fid = self.figures.spawn(kind, exit.0, exit.1, Travel::Land);
                    if let Some(f) = self.figures.get_mut(fid) {
                        f.formation = c as u16 + 1;
                        f.slot = slot;
                        f.home = self.military.companies[c].fort;
                    }
                    self.military.companies[c].recruits.push(fid);
                    self.send_to_post(fid);
                }
            }
            return;
        }
        let won = b.strength >= b.enemy && b.strength > 0;
        let losses = if won { battle_losses((b.strength - b.enemy) * 100 / b.strength.max(1)) } else { 100 };
        for &c in &b.companies {
            let co = &mut self.military.companies[c];
            co.abroad = co.abroad * (100 - losses) / 100;
        }
        let returning = b.companies.iter().any(|&c| self.military.companies[c].abroad > 0);
        if let Some(bb) = self.military.battle.as_mut() {
            bb.fought = true;
            bb.months = TRAVEL_MONTHS;
        }
        if !returning {
            self.military.battle = None;
        }
        self.settle_troop_request(b.request, won);
    }
    pub(crate) fn update_morale_month(&mut self) {
        for c in &mut self.military.companies {
            if c.at_fort {
                c.months_away = 0;
                c.morale = (c.morale + 5).min(morale_cap(c.kind, c.trained));
            } else {
                c.months_away += 1;
                if c.months_away > 3 {
                    c.morale = (c.morale - 5).max(0);
                }
            }
        }
    }

    /// The fallen lie a while, then are gone, and leave their company.
    fn update_corpse(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get_mut(fid) else { return };
        f.counter += 1;
        if f.counter < CORPSE_TICKS {
            return;
        }
        f.dead = true;
        for c in &mut self.military.companies {
            c.soldiers.retain(|&s| s != fid);
            c.recruits.retain(|&s| s != fid);
        }
    }

    /// An archer looses a missile at the nearest enemy in range, when he is ready.
    pub(crate) fn shoot_at_foes(&mut self, fid: FigureId) {
        let stats = self.fighter_stats(fid);
        if stats.missile_range <= 0 {
            return;
        }
        let Some(f) = self.figures.get_mut(fid) else { return };
        f.attack_tick += 1;
        if (f.attack_tick as i32) < stats.missile_delay.max(1) {
            return;
        }
        let (x, y) = (f.x, f.y);
        let mine = self.figures.get(fid).is_some_and(|f| self.is_invader(f));
        let range = stats.missile_range;
        let target = self.nearest_foe(mine, (x, y), range);
        let Some((target, tx, ty)) = target else { return };
        if let Some(f) = self.figures.get_mut(fid) {
            f.attack_tick = 0;
            f.direction = crate::figures::direction_to((x, y), (tx, ty)).unwrap_or(f.direction);
        }
        let missile = self.figures.spawn(if mine { ARROW } else { JAVELIN }, x, y, Travel::Land);
        if let Some(m) = self.figures.get_mut(missile) {
            m.foe = target;
            m.amount = stats.missile_attack;
            m.destination = Some((tx, ty));
            m.direction = crate::figures::direction_to((x, y), (tx, ty)).unwrap_or(0);
        }
    }

    /// A missile flies a tile every few ticks toward where its target stood, and
    /// wounds it if it is still there.
    pub(crate) fn update_missile(&mut self, fid: FigureId) {
        let Some(m) = self.figures.get_mut(fid) else { return };
        m.counter += 1;
        if m.counter % 3 != 0 {
            return;
        }
        let Some((tx, ty)) = m.destination else {
            m.dead = true;
            return;
        };
        if (m.x, m.y) != (tx, ty) {
            m.x += (tx - m.x).signum();
            m.y += (ty - m.y).signum();
            return;
        }
        m.dead = true;
        let (target, attack) = (m.foe, m.amount);
        let hit = self.figures.get(target).is_some_and(|t| !t.dead && t.action != action::CORPSE && (t.x - tx).abs() <= 1 && (t.y - ty).abs() <= 1);
        if hit {
            let armor = self.fighter_stats(target).missile_armor;
            let damage = (attack - armor).max(0);
            let damage = match self.holding(target) {
                Some(Order::HoldTight) => damage * 3 / 2,
                Some(Order::HoldLoose) => damage / 2,
                _ => damage,
            };
            self.hurt(target, damage);
        }
    }

    /// The standard bearer carries the company's flag to where it forms up.
    pub(crate) fn update_standard_bearer(&mut self, fid: FigureId) {
        let map = &self.map;
        let Some(f) = self.figures.get_mut(fid) else { return };
        if f.walk(map) != Step::Moving {
            f.action = if f.action == action::GOING_TO_FORT { action::AT_REST } else { action::AT_STANDARD };
        }
    }
}
