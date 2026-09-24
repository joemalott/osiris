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

/// Building kinds whose tiles roamers won't step onto.
const ROADBLOCK: u16 = 138;
const FERRY: u16 = 136;

/// The original's tally of recent walker traffic on each tile: ten 3-bit counters in a
/// word, one per kind of service (see `traffic_shift`), raised by one (up to 7) as
/// such a walker reaches the tile centre and lowered by one every 20 days. Setting a
/// counter keeps only the word's low 16 bits besides it, as the original's mask does,
/// so raising a low counter (or any other walker passing) wipes the high ones.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Traffic {
    grid: Option<crate::grid::Grid<u32>>,
    days: u8,
}

/// Bit offset of kind `k`'s traffic counter: water carriers 0, firemen 1, architects 2,
/// constables and magistrates 3, entertainers 4, scribes and librarians 5, priests 6,
/// tax collectors and labor seekers 7, market traders 8, the four healers 9 (times
/// three bits); every other walker's slot is -1, which lands on the top three bits.
fn traffic_shift(k: u16) -> u32 {
    let slot: i32 = match k {
        87 => 0,
        10 => 1,
        8 => 2,
        40 | 88 | 89 => 3,
        15..=18 | 105 => 4,
        29 | 30 => 5,
        27 | 64 => 6,
        5 | 7 => 7,
        26 | 39 | 66 => 8,
        31..=34 => 9,
        _ => -1,
    };
    (slot * 3) as u32 & 31
}

/// Replaces the 3-bit counter at `shift` in `word` by `v` the original's way.
fn set_counter(word: u32, shift: u32, v: u32) -> u32 {
    (word & ((7 << shift) ^ 0xffff)) | (v << shift)
}

impl Traffic {
    /// Kind `k`'s counter on `(x, y)`.
    pub fn at(&self, k: u16, x: i32, y: i32) -> u32 {
        self.grid.as_ref().map_or(0, |g| g.at_or(x, y, 0) >> traffic_shift(k) & 7)
    }

    /// A walker of kind `k` reaches `(x, y)`.
    pub(crate) fn note(&mut self, map: &crate::map::Map, k: u16, x: i32, y: i32) {
        let g = self.grid.get_or_insert_with(|| crate::grid::Grid::new(map.width, map.height));
        let shift = traffic_shift(k);
        g.update(x, y, |w| {
            let v = w >> shift & 7;
            if v < 7 { set_counter(w, shift, v + 1) } else { w }
        });
    }

    /// Tick 45: every 20th day each tile's ten counters drop by one.
    pub(crate) fn decay(&mut self) {
        self.days += 1;
        if self.days < 20 {
            return;
        }
        self.days = 0;
        let Some(g) = self.grid.as_mut() else { return };
        let (w, h) = (g.width(), g.height());
        for y in 0..h {
            for x in 0..w {
                g.update(x, y, |mut word| {
                    if word != 0 {
                        for shift in (0..30).step_by(3) {
                            let v = word >> shift & 7;
                            if v != 0 {
                                word = set_counter(word, shift, v - 1);
                            }
                        }
                    }
                    word
                });
            }
        }
    }
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

    /// Sends a `kind` walker roaming from `home`'s road. As in the original he first
    /// walks to the road nearest (within six tiles) the spot eight tiles from the
    /// building's corner in its next direction (north, east, south, west in turn), and
    /// only roams from there, his roam length counting from that arrival.
    pub(crate) fn spawn_roaming_figure(&mut self, home: BuildingId, kind: u16) -> Option<FigureId> {
        let b = self.buildings.get(home)?;
        let (rx, ry) = b.road?;
        let dir = b.roam_dir;
        let (dx, dy) = NEIGHBOURS[dir as usize % 8];
        let out = (
            (b.x + 8 * dx).clamp(0, self.map.width - 1),
            (b.y + 8 * dy).clamp(0, self.map.height - 1),
        );
        let out = crate::buildings::road_within(&self.map, out.0, out.1, 1, 6);
        let roam = self.max_roam(kind);
        let fid = self.figures.spawn(kind, rx, ry, Travel::Roads);
        let map = &self.map;
        if let Some(f) = self.figures.get_mut(fid) {
            f.home = home;
            f.action = action::ROAMING;
            f.roam_left = roam;
            f.roam_turn = 2;
            f.roam_wait = -1;
            f.roam_out = out.is_some_and(|t| f.go_to(map, t));
            if !f.roam_out {
                f.destination = Some((rx, ry));
                f.route.clear();
            }
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
            let delay = if b.kind == kind::WATER_SUPPLY { self.water_supply_delay(b) } else { spawn_delay_days(b.kind, b.workers, needed.max(1)) };
            let Some(delay) = delay else { continue };
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
            // The trader serves each house tile around him, so a big house is served
            // once for each of its tiles in reach.
            for yy in y - SERVICE_RADIUS..=y + SERVICE_RADIUS {
                for xx in x - SERVICE_RADIUS..=x + SERVICE_RADIUS {
                    let id = self.map.building.at_or(xx, yy, 0);
                    if self.buildings.get(id).is_some_and(|b| b.house.as_ref().is_some_and(|h| h.population > 0)) {
                        self.deliver_to_house(home, id);
                    }
                }
            }
            served += seen.iter().filter(|&&id| self.buildings.get(id).is_some_and(|b| b.house.as_ref().is_some_and(|h| h.population > 0))).count() as i32;
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

    /// The roads a roamer of kind `k` may take from `(x, y)`, by direction, and how
    /// many: road or ramp tiles in the four directions, not into a roadblock (which
    /// only the plague-stricken pass) or a ferry landing. Where more than two meet,
    /// only those its kind has walked least lately count.
    fn roam_roads(&self, k: u16, x: i32, y: i32) -> ([bool; 8], usize) {
        let mut roads = [false; 8];
        for d in (0..8).step_by(2) {
            let (nx, ny) = (x + NEIGHBOURS[d].0, y + NEIGHBOURS[d].1);
            let blocker = self.buildings.get(self.map.building.at_or(nx, ny, 0)).is_some_and(|b| b.kind == FERRY || b.kind == ROADBLOCK && k != crate::health::PLAGUED_CITIZEN);
            roads[d] = self.map.terrain_is(nx, ny, terrain::ROAD | terrain::ACCESS_RAMP) && crate::figures::passable(&self.map, Travel::Roads, nx, ny) && !blocker;
        }
        let mut n = roads.iter().filter(|&&r| r).count();
        if n > 2 {
            let seen = |d: usize| self.traffic.at(k, x + NEIGHBOURS[d].0, y + NEIGHBOURS[d].1);
            let least = (0..8).filter(|&d| roads[d]).map(seen).min().unwrap_or(0);
            for (d, road) in roads.iter_mut().enumerate() {
                if *road && seen(d) > least {
                    *road = false;
                    n -= 1;
                }
            }
        }
        (roads, n)
    }

    /// Points a roamer at the road nearest in turning order to the direction of its
    /// roam's first destination, turning the way that finds it sooner (clockwise on a
    /// tie), and gives it five junctions before it tries again.
    fn roam_toward_destination(&mut self, fid: u32) {
        let Some(f) = self.figures.get(fid) else { return };
        let (x, y) = (f.x, f.y);
        let start = f.destination.and_then(|d| crate::figures::direction_to((x, y), d)).unwrap_or(0) as usize;
        let road = |d: usize| d.is_multiple_of(2) && self.map.terrain_is(x + NEIGHBOURS[d].0, y + NEIGHBOURS[d].1, terrain::ROAD);
        let cw = (0..8).map(|i| (start + i) % 8).position(road);
        let ccw = (0..8).map(|i| (start + 8 - i) % 8).position(road);
        let cw = cw.map_or((8, 0), |i| (i, (start + i) % 8));
        let ccw = ccw.map_or((8, 4), |i| (i, (start + 8 - i) % 8));
        let f = self.figures.get_mut(fid).expect("present");
        if cw.0 <= ccw.0 {
            f.direction = cw.1 as u8;
            f.roam_turn = 2;
        } else {
            f.direction = ccw.1 as u8;
            f.roam_turn = -2;
        }
        f.roam_wait = 5;
    }

    /// Picks the next road direction for a roamer standing on a tile centre, as the
    /// original does: a dead end turns it round; on a road with two ways it keeps on,
    /// else turns its turning way, but never back; at a junction it tries a way picked
    /// from the tile's random value, and when that fails it counts down to turning
    /// toward its first destination, then turns its way from there. None when no
    /// road leads on.
    pub(crate) fn roam_direction(&mut self, fid: u32) -> Option<u8> {
        let f = self.figures.get_mut(fid)?;
        f.counter += 1;
        let (k, x, y) = (f.kind, f.x, f.y);
        let mut came_from = Some((f.direction as usize + 4) % 8);
        let (roads, n) = self.roam_roads(k, x, y);
        if n == 0 {
            return None;
        }
        if n == 1 {
            return (0..8).step_by(2).find(|&d| roads[d]).map(|d| d as u8);
        }
        if n == 2 {
            if self.figures.get(fid)?.roam_wait == -1 {
                self.roam_toward_destination(fid);
                came_from = None;
            }
        } else {
            let random = self.map.random.at_or(x, y, 0) as i32;
            let f = self.figures.get_mut(fid)?;
            let pick = ((random + f.counter) & 6) as usize;
            f.direction = pick as u8;
            if !roads[pick] || Some(pick) == came_from {
                f.roam_wait = f.roam_wait.saturating_sub(1);
                if f.roam_wait < 1 {
                    self.roam_toward_destination(fid);
                    came_from = None;
                }
            } else {
                return Some(pick as u8);
            }
        }
        let f = self.figures.get(fid)?;
        let mut dir = f.direction as i32;
        for _ in 0..5 {
            if roads[dir as usize] && Some(dir as usize) != came_from {
                return Some(dir as u8);
            }
            dir += f.roam_turn as i32;
            if dir > 6 {
                dir = 0;
            }
            if dir < 0 {
                dir = 6;
            }
        }
        (0..8).step_by(2).find(|&d| roads[d] && Some(d) != came_from).map(|d| d as u8)
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
                // The roam length counts every tick out; when it's spent he heads for a
                // road within two tiles of home.
                let f = self.figures.get_mut(fid).expect("present");
                f.roam_left -= 1;
                if f.roam_left <= 0 {
                    let target = self.buildings.get(home).and_then(|b| crate::buildings::road_within(&self.map, b.x, b.y, b.size, 2).or(b.road));
                    let map = &self.map;
                    let f = self.figures.get_mut(fid).expect("present");
                    f.action = action::RETURNING;
                    f.roam_out = false;
                    match target {
                        Some(t) if f.go_to(map, t) => {}
                        _ => f.dead = true,
                    }
                    return;
                }
                if f.roam_out {
                    // The first leg serves as he goes, like any walk.
                    let (x, y, moving) = (f.x, f.y, f.moving);
                    if !moving {
                        self.provide_service(kind, home, x, y);
                    }
                    let max = self.max_roam(kind);
                    let map = &self.map;
                    let f = self.figures.get_mut(fid).expect("present");
                    match f.walk(map) {
                        Step::Moving => return,
                        Step::Arrived => f.roam_left = max,
                        Step::Blocked | Step::Lost => {}
                    }
                    f.roam_out = false;
                    f.roam_wait = 100;
                    f.route.clear();
                }
                let (x, y, at_centre) = self.figures.get(fid).map(|f| (f.x, f.y, !f.moving)).unwrap_or_default();
                if at_centre {
                    self.provide_service(kind, home, x, y);
                    match self.roam_direction(fid) {
                        Some(d) => {
                            let f = self.figures.get_mut(fid).expect("present");
                            f.route.clear();
                            f.route.push_back(d);
                        }
                        None => {
                            // Nowhere to go: his roam is over.
                            self.figures.get_mut(fid).expect("present").roam_left = 0;
                            return;
                        }
                    }
                }
                let map = &self.map;
                if let Some(f) = self.figures.get_mut(fid)
                    && f.walk(map) == Step::Blocked
                {
                    f.route.clear();
                }
            }
            _ => {
                // Walking home, he still serves at each tile.
                let (x, y, moving) = self.figures.get(fid).map(|f| (f.x, f.y, f.moving)).unwrap_or_default();
                if !moving {
                    self.provide_service(kind, home, x, y);
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traffic_counters_rise_decay_and_low_ones_wipe_high_ones() {
        let map = crate::map::Map {
            width: 2,
            height: 1,
            terrain: crate::grid::Grid::new(2, 1),
            images: crate::grid::Grid::new(2, 1),
            edges: crate::grid::Grid::new(2, 1),
            bitfields: crate::grid::Grid::new(2, 1),
            elevation: crate::grid::Grid::new(2, 1),
            random: crate::grid::Grid::new(2, 1),
            fertility: crate::grid::Grid::new(2, 1),
            moisture: crate::grid::Grid::new(2, 1),
            vegetation: crate::grid::Grid::new(2, 1),
            building: crate::grid::Grid::new(2, 1),
            border: Vec::new(),
        };
        let mut t = Traffic::default();
        for _ in 0..9 {
            t.note(&map, figure_kind::PHYSICIAN, 0, 0);
        }
        assert_eq!(t.at(figure_kind::PHYSICIAN, 0, 0), 7);
        // A water carrier's counter sits in the low bits; setting it clears the rest.
        t.note(&map, figure_kind::WATER_CARRIER, 0, 0);
        assert_eq!((t.at(figure_kind::WATER_CARRIER, 0, 0), t.at(figure_kind::PHYSICIAN, 0, 0)), (1, 0));
        t.note(&map, figure_kind::FIREMAN, 0, 0);
        for _ in 0..19 {
            t.decay();
        }
        assert_eq!(t.at(figure_kind::FIREMAN, 0, 0), 1);
        t.decay();
        assert_eq!((t.at(figure_kind::FIREMAN, 0, 0), t.at(figure_kind::WATER_CARRIER, 0, 0)), (0, 0));
    }
}
