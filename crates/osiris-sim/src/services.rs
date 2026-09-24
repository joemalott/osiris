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
    /// The scribal school's walker (the original's "scriber"; its "teacher" is unused).
    pub const TEACHER: u16 = 29;
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

/// Days a building waits between walkers, at 100%, 75%, 50%, 25% and any staffing.
pub fn walker_delays(k: u16) -> [i32; 5] {
    match k {
        kind::APOTHECARY | kind::MORTUARY | crate::ratings::DENTIST | crate::ratings::PHYSICIAN => [3, 7, 15, 29, 44],
        kind::JUGGLER_SCHOOL | kind::CONSERVATORY | kind::BOOTH | kind::BANDSTAND => [3, 7, 15, 29, 44],
        kind::DANCE_SCHOOL => [5, 10, 20, 35, 60],
        kind::PAVILION => [6, 12, 20, 40, 70],
        kind::SCRIBAL_SCHOOL | kind::LIBRARY => [5, 10, 25, 50, 100],
        60..=69 => [3, 7, 10, 15, 20],
        kind::WATER_SUPPLY => [1, 3, 7, 15, 29],
        _ => [0, 1, 3, 7, 15],
    }
}

/// Days until building `k`'s next walker at its staffing; none when unstaffed.
pub fn spawn_delay_days(k: u16, workers: i32, needed: i32) -> Option<i32> {
    if workers <= 0 || needed <= 0 {
        return None;
    }
    let d = walker_delays(k);
    Some(match workers * 100 / needed {
        p if p >= 100 => d[0],
        p if p >= 75 => d[1],
        p if p >= 50 => d[2],
        p if p >= 25 => d[3],
        _ => d[4],
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
            // Labor seeker: looks for houses while the building's walkers have passed
            // fewer than 51 of them lately (101 for the police, fire, architects',
            // courthouse, recruiter and hunting lodge, whose walkers reach fewer).
            let seek_below = if matches!(b.kind, 55 | 81 | 95 | 115 | 167 | 184) { 101 } else { 51 };
            if needed > 0 && !self.rules.global_labor_pool && b.walkers[1] == 0 && b.houses_covered < seek_below {
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
            // Tax offices send collectors only while the palace has staff.
            if kind == figure_kind::TAX_COLLECTOR && !self.palace_staffed() {
                continue;
            }
            let Some(delay) = spawn_delay_days(b.kind, b.workers, needed.max(1)) else { continue };
            if b.spawn_delay > 0 {
                if let Some(b) = self.buildings.get_mut(id) {
                    b.spawn_delay -= 1;
                }
                continue;
            }
            // Schools, libraries and mortuaries send a walker only with his papyrus or
            // linen in stock, and keep waiting, ready, until they have it.
            if let Some((r, n)) = self.walker_supplies(b.kind) {
                if b.stock.get(r as usize).copied().unwrap_or(0) < n {
                    continue;
                }
                self.take_stored(id, r, n);
            }
            if let Some(b) = self.buildings.get_mut(id) {
                b.spawn_delay = delay;
            }
            self.spawn_roamer(id, kind, 0);
        }
    }

    /// The good a building's walker takes with him and how much: papyrus for a
    /// teacher or librarian, linen for an embalmer. The amount is column i of the
    /// building's model row (10, 20 and 20 on Normal); the Oracle of Thoth cuts the
    /// papyrus and the Altar of Anubis the linen to the Ptah or Seth complex's column
    /// i percent (60 on Normal).
    pub fn walker_supplies(&self, k: u16) -> Option<(u16, i32)> {
        use crate::economy::resource;
        use crate::temple_complex::{ALTAR, ORACLE, PTAH, SETH};
        let (r, god, bit) = match k {
            kind::SCRIBAL_SCHOOL | kind::LIBRARY => (resource::PAPYRUS, PTAH, ORACLE),
            kind::MORTUARY => (resource::LINEN, SETH, ALTAR),
            _ => return None,
        };
        let mut n = self.balance.stats(k).i;
        if self.complex_blessing(god, bit) {
            n = n * self.balance.stats(crate::temple_complex::OSIRIS_COMPLEX + god as u16).i / 100;
        }
        Some((r, n))
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
                self.patrol(kind, &houses);
            }
            figure_kind::PRIEST => self.priest_blessings(home_kind, x, y, &seen),
            _ => {}
        }
        // Firemen and architects take their post's model column j (100 and 50 on
        // Normal) off a building's fire or collapse risk for each of its tiles around
        // them; physicians and herbalists likewise take theirs (25) off a house's
        // disease or malaria risk.
        if matches!(kind, figure_kind::FIREMAN | figure_kind::ARCHITECT | figure_kind::PHYSICIAN | figure_kind::HERBALIST) {
            let relief = self.balance.stats(home_kind).j;
            for yy in y - SERVICE_RADIUS..=y + SERVICE_RADIUS {
                for xx in x - SERVICE_RADIUS..=x + SERVICE_RADIUS {
                    let id = self.map.building.at_or(xx, yy, 0);
                    let Some(b) = self.buildings.get_mut(id) else { continue };
                    let risk = match kind {
                        figure_kind::FIREMAN => &mut b.fire_risk,
                        figure_kind::ARCHITECT => &mut b.damage_risk,
                        figure_kind::PHYSICIAN => match b.house.as_mut() {
                            Some(h) => &mut h.disease_risk,
                            None => continue,
                        },
                        _ => match b.house.as_mut() {
                            Some(h) => &mut h.malaria_risk,
                            None => continue,
                        },
                    };
                    *risk = (*risk - relief).max(0);
                }
            }
        }
        for id in seen {
            let Some(b) = self.buildings.get_mut(id) else { continue };
            let Some(h) = b.house.as_mut() else { continue };
            if h.population <= 0 {
                continue;
            }
            served += 1;
            let c = &mut h.coverage;
            match kind {
                figure_kind::WATER_CARRIER => c.water_supply = VISIT,
                figure_kind::HERBALIST => c.apothecary = VISIT,
                figure_kind::PHYSICIAN => c.physician = VISIT,
                figure_kind::DENTIST => c.dentist = VISIT,
                figure_kind::EMBALMER => c.mortuary = VISIT,
                figure_kind::TEACHER => c.school = VISIT,
                figure_kind::LIBRARIAN => c.library = VISIT,
                figure_kind::TAX_COLLECTOR => c.tax = 50,
                crate::entertainment::JUGGLER => c.juggler = VISIT,
                crate::entertainment::MUSICIAN => c.musician = VISIT,
                crate::entertainment::DANCER => c.dancer = VISIT,
                figure_kind::PRIEST => {
                    // Temples and complexes of each god: Osiris, Ra, Ptah, Seth, Bast.
                    // Shrines send no priests and give houses no access.
                    match home_kind {
                        60..=64 => c.temples[(home_kind - 60) as usize] = VISIT,
                        65..=69 => c.temples[(home_kind - 65) as usize] = VISIT,
                        _ => {}
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
        if calms {
            let calm = self.balance.stats(crate::temple_complex::OSIRIS_COMPLEX + god as u16).j;
            self.calm_houses(seen, calm);
        }
        if cleanses {
            for &id in seen {
                if let Some(h) = self.buildings.get_mut(id).and_then(|b| b.house.as_mut()) {
                    h.quarantine = 0;
                }
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
