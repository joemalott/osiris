//! Service walkers: buildings send roamers along the roads; at each tile centre a
//! roamer serves every house (or building) within two tiles, then heads home once it
//! has walked its maximum roam length.

use crate::buildings::{BuildingId, kind};
use crate::figures::{FigureId, Step, Travel};
use crate::map::{NEIGHBOURS, terrain};
use crate::world::World;

pub mod figure_kind {
    pub const LABOR_SEEKER: u16 = 5;
    pub const TAX_COLLECTOR: u16 = 7;
    pub const ARCHITECT: u16 = 8;
    pub const FIREMAN: u16 = 10;
    pub const PRIEST: u16 = 27;
    pub const TEACHER: u16 = 28;
    pub const LIBRARIAN: u16 = 30;
    pub const DENTIST: u16 = 31;
    pub const PHYSICIAN: u16 = 32;
    pub const HERBALIST: u16 = 33;
    pub const EMBALMER: u16 = 34;
    pub const WATER_CARRIER: u16 = 87;
}

/// Coverage value set on a house when a walker passes.
pub const VISIT: i32 = 96;
/// Radius around a roamer within which it serves.
const SERVICE_RADIUS: i32 = 2;

/// Action states of roamers.
mod action {
    pub const ROAMING: u16 = 1;
    pub const RETURNING: u16 = 2;
}

/// Days until the next walker, by staffing percentage.
fn spawn_delay_days(workers: i32, needed: i32) -> Option<i32> {
    if workers <= 0 || needed <= 0 {
        return None;
    }
    let pct = workers * 100 / needed;
    Some(match pct {
        p if p >= 100 => 0,
        p if p >= 75 => 1,
        p if p >= 50 => 3,
        p if p >= 25 => 7,
        _ => 15,
    })
}

pub fn is_roamer(kind: u16) -> bool {
    use figure_kind::*;
    matches!(
        kind,
        LABOR_SEEKER | TAX_COLLECTOR | ARCHITECT | FIREMAN | PRIEST | TEACHER | LIBRARIAN | DENTIST | PHYSICIAN
            | HERBALIST | EMBALMER | WATER_CARRIER
    ) || matches!(kind, crate::crime::CONSTABLE | crate::crime::MAGISTRATE)
        || kind == crate::food::MARKET_TRADER
        || crate::entertainment::performer_slot(kind).is_some()
}

impl World {
    fn max_roam(&self, kind: u16) -> i32 {
        self.defs
            .figure(kind)
            .and_then(|f| f.int("max_roam_length"))
            .unwrap_or(384) as i32
    }

    pub(crate) fn spawn_roamer(&mut self, home: BuildingId, kind: u16, slot: usize) {
        if let Some(fid) = self.spawn_roaming_figure(home, kind)
            && let Some(b) = self.buildings.get_mut(home)
        {
            b.walkers[slot] = fid;
        }
    }

    /// Sends a `kind` walker roaming from `home`'s road.
    pub(crate) fn spawn_roaming_figure(&mut self, home: BuildingId, kind: u16) -> Option<FigureId> {
        let b = self.buildings.get(home)?;
        let (rx, ry) = b.road?;
        let dir = b.roam_dir;
        let roam = self.max_roam(kind);
        let fid = self.figures.spawn(kind, rx, ry, Travel::Roads);
        let turn_seed = self.rng.byte();
        if let Some(f) = self.figures.get_mut(fid) {
            f.home = home;
            f.action = action::ROAMING;
            f.roam_left = roam;
            f.roam_turn = if turn_seed & 1 == 0 { 2 } else { -2 };
            f.direction = dir;
        }
        if let Some(b) = self.buildings.get_mut(home) {
            b.roam_dir = (b.roam_dir + 2) % 8;
        }
        Some(fid)
    }

    /// Tick 31: buildings send out their walkers.
    pub(crate) fn generate_walkers(&mut self) {
        for id in self.buildings.ids() {
            let Some(b) = self.buildings.get(id) else { continue };
            if b.is_house() || b.road.is_none() {
                continue;
            }
            let needed = self.workers_needed(b.kind);
            // Labor seeker: looks for houses when the building has none nearby.
            if needed > 0 && !self.rules.global_labor_pool && b.walkers[1] == 0 && b.houses_covered < 50 {
                self.spawn_roamer(id, figure_kind::LABOR_SEEKER, 1);
            }
            let Some(b) = self.buildings.get(id) else { continue };
            // These send their walkers through their own rules.
            if matches!(b.kind, kind::BOOTH | kind::BANDSTAND | kind::PAVILION | kind::JUGGLER_SCHOOL | kind::CONSERVATORY | kind::DANCE_SCHOOL | kind::BAZAAR) {
                continue;
            }
            let Some(kind) = self.defs.building(b.kind).and_then(|d| d.figure) else { continue };
            if !is_roamer(kind) || b.walkers[0] != 0 {
                continue;
            }
            let Some(delay) = spawn_delay_days(b.workers, needed.max(1)) else { continue };
            if b.spawn_delay > 0 {
                if let Some(b) = self.buildings.get_mut(id) {
                    b.spawn_delay -= 1;
                }
                continue;
            }
            if let Some(b) = self.buildings.get_mut(id) {
                b.spawn_delay = delay;
            }
            self.spawn_roamer(id, kind, 0);
        }
    }

    /// Applies a roamer's service around `(x, y)`.
    fn provide_service(&mut self, kind: u16, home: BuildingId, x: i32, y: i32) {
        let mut served = 0;
        let mut seen: Vec<BuildingId> = Vec::new();
        for yy in y - SERVICE_RADIUS..=y + SERVICE_RADIUS {
            for xx in x - SERVICE_RADIUS..=x + SERVICE_RADIUS {
                let id = self.map.building.at_or(xx, yy, 0);
                if id == 0 || seen.contains(&id) {
                    continue;
                }
                seen.push(id);
            }
        }
        let home_kind = self.buildings.get(home).map_or(0, |b| b.kind);
        if kind == crate::food::MARKET_TRADER {
            for &id in &seen {
                if self.buildings.get(id).is_some_and(|b| b.house.as_ref().is_some_and(|h| h.population > 0)) {
                    self.deliver_food(home, id);
                    self.deliver_goods(home, id);
                    served += 1;
                }
            }
        }
        match kind {
            crate::crime::CONSTABLE | crate::crime::MAGISTRATE => {
                let houses: Vec<BuildingId> = seen.iter().copied().filter(|&id| self.buildings.get(id).is_some_and(|b| b.house.as_ref().is_some_and(|h| h.population > 0))).collect();
                self.patrol(kind, x, y, &houses);
            }
            figure_kind::HERBALIST => self.cure_plagued_near(x, y),
            figure_kind::PRIEST => self.priest_blessings(home_kind, x, y, &seen),
            _ => {}
        }
        for id in seen {
            let Some(b) = self.buildings.get_mut(id) else { continue };
            match kind {
                figure_kind::FIREMAN => b.fire_risk = 0,
                figure_kind::ARCHITECT => b.damage_risk = 0,
                _ => {}
            }
            let Some(h) = b.house.as_mut() else { continue };
            if h.population <= 0 {
                continue;
            }
            served += 1;
            let c = &mut h.coverage;
            match kind {
                figure_kind::WATER_CARRIER => c.water_supply = VISIT,
                figure_kind::HERBALIST => {
                    c.apothecary = VISIT;
                    h.common_health = h.common_health.max(50);
                }
                figure_kind::PHYSICIAN => {
                    c.physician = VISIT;
                    h.common_health = (h.common_health + 1).min(100);
                }
                figure_kind::DENTIST => c.dentist = VISIT,
                figure_kind::EMBALMER => c.mortuary = VISIT,
                figure_kind::TEACHER => c.school = VISIT,
                figure_kind::LIBRARIAN => c.library = VISIT,
                figure_kind::TAX_COLLECTOR => c.tax = 50,
                crate::entertainment::JUGGLER => c.juggler = VISIT,
                crate::entertainment::MUSICIAN => c.musician = VISIT,
                crate::entertainment::DANCER => c.dancer = VISIT,
                figure_kind::PRIEST => {
                    // Temples and shrines of each god: Osiris, Ra, Ptah, Seth, Bast.
                    let god = match home_kind {
                        60..=64 => Some((home_kind - 60) as usize),
                        65..=69 => Some((home_kind - 65) as usize),
                        140..=144 => None,
                        _ => None,
                    };
                    match god {
                        Some(g) => c.temples[g] = VISIT,
                        None => c.shrine = VISIT,
                    }
                }
                _ => {}
            }
        }
        if let Some(b) = self.buildings.get_mut(home) {
            b.houses_covered = (b.houses_covered + served).min(300);
        }
    }

    /// What a temple complex's upgrades let its god's priests do as they pass: Ma'at's
    /// altar (Ra) and Sekhmet's oracle (Seth) calm would-be criminals like a
    /// constable; Isis's altar (Bast) cures the plague-stricken in the street, and
    /// with Hathor's oracle too, cleanses infected houses.
    fn priest_blessings(&mut self, home_kind: u16, x: i32, y: i32, seen: &[BuildingId]) {
        use crate::temple_complex::{ALTAR, BAST, ORACLE, RA, SETH};
        let god = match home_kind {
            60..=64 => (home_kind - 60) as usize,
            65..=69 => (home_kind - 65) as usize,
            _ => return,
        };
        let calms = god == RA && self.complex_blessing(RA, ALTAR) || god == SETH && self.complex_blessing(SETH, ORACLE);
        let cures = god == BAST && self.complex_blessing(BAST, ALTAR);
        let cleanses = cures && self.complex_blessing(BAST, ALTAR | ORACLE);
        if cures {
            self.cure_plagued_near(x, y);
        }
        if !calms && !cleanses {
            return;
        }
        for &id in seen {
            let Some(h) = self.buildings.get_mut(id).and_then(|b| b.house.as_mut()) else { continue };
            if calms {
                h.criminal_active = (h.criminal_active - 1).max(0);
            }
            if cleanses {
                h.plague_days = 0;
            }
        }
    }

    /// Picks the next road direction for a roamer standing on a tile centre.
    pub(crate) fn roam_direction(&mut self, fid: u32) -> Option<u8> {
        let f = self.figures.get(fid)?;
        let (x, y) = (f.x, f.y);
        let came_from = (f.direction + 4) % 8;
        let roads: Vec<u8> = (0..8u8)
            .step_by(2)
            .filter(|&d| {
                let (dx, dy) = NEIGHBOURS[d as usize];
                self.map.terrain_is(x + dx, y + dy, terrain::ROAD) && !self.map.terrain_is(x + dx, y + dy, terrain::WATER) && self.roamer_may_enter(x + dx, y + dy)
            })
            .collect();
        match roads.len() {
            0 => None,
            1 => Some(roads[0]),
            _ => {
                let turn = f.roam_turn;
                let mut dir = if roads.len() == 2 {
                    f.direction
                } else {
                    ((f.counter + self.map.random.at_or(x, y, 0) as i32) & 6) as u8
                };
                for _ in 0..4 {
                    if roads.contains(&dir) && dir != came_from {
                        return Some(dir);
                    }
                    dir = ((dir as i32 + turn as i32).rem_euclid(8)) as u8;
                }
                roads.iter().copied().find(|&d| d != came_from).or(Some(roads[0]))
            }
        }
    }

    pub(crate) fn update_roamer(&mut self, fid: u32) {
        let Some(f) = self.figures.get(fid) else { return };
        let (kind, home, act) = (f.kind, f.home, f.action);
        // Constables stand and fight invaders who come at them.
        if kind == crate::crime::CONSTABLE {
            match act {
                crate::military::action::CORPSE => {
                    let f = self.figures.get_mut(fid).expect("present");
                    f.counter += 1;
                    if f.counter > 200 {
                        f.dead = true;
                    }
                    return;
                }
                crate::military::action::ATTACK => {
                    self.fight(fid);
                    if self.figures.get(fid).is_some_and(|f| f.action == 0) {
                        // The fight is over: back on the rounds, heading home.
                        let target = self.buildings.get(home).and_then(|b| b.road);
                        let map = &self.map;
                        let f = self.figures.get_mut(fid).expect("present");
                        f.action = action::RETURNING;
                        if !target.is_some_and(|t| f.go_to(map, t)) {
                            f.dead = true;
                        }
                    }
                    return;
                }
                _ => {
                    let (x, y) = (f.x, f.y);
                    let foe = self.figures.iter().find(|o| crate::invasions::is_invader_kind(o.kind) && o.action != crate::military::action::CORPSE && (o.x - x).abs() <= 1 && (o.y - y).abs() <= 1).map(|o| o.id);
                    if let Some(foe) = foe {
                        let f = self.figures.get_mut(fid).expect("present");
                        f.foe = foe;
                        f.action = crate::military::action::ATTACK;
                        f.attack_tick = 0;
                        f.route.clear();
                        return;
                    }
                }
            }
        }
        if self.buildings.get(home).is_none() {
            if let Some(f) = self.figures.get_mut(fid) {
                f.dead = true;
            }
            return;
        }
        match act {
            action::ROAMING => {
                let at_centre = !self.figures.get(fid).is_some_and(|f| f.moving);
                if at_centre {
                    let (x, y) = self.figures.get(fid).map(|f| (f.x, f.y)).unwrap_or_default();
                    self.provide_service(kind, home, x, y);
                    let done = self.figures.get(fid).is_some_and(|f| f.roam_left <= 0);
                    let next = if done { None } else { self.roam_direction(fid) };
                    let map = &self.map;
                    let Some(f) = self.figures.get_mut(fid) else { return };
                    match next {
                        Some(d) => {
                            f.route.clear();
                            f.route.push_back(d);
                            f.counter += 1;
                        }
                        None => {
                            f.action = action::RETURNING;
                            let target = self.buildings.get(home).and_then(|b| b.road);
                            match target {
                                Some(t) if f.go_to(map, t) => {}
                                _ => f.dead = true,
                            }
                            return;
                        }
                    }
                }
                let map = &self.map;
                if let Some(f) = self.figures.get_mut(fid) {
                    f.roam_left -= f.speed as i32;
                    if f.walk(map) == Step::Blocked {
                        f.route.clear();
                    }
                }
            }
            _ => {
                let map = &self.map;
                let Some(f) = self.figures.get_mut(fid) else { return };
                match f.walk(map) {
                    Step::Moving => {}
                    _ => f.dead = true,
                }
            }
        }
    }
}
