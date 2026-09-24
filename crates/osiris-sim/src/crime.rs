//! Crime: every household carries a crime risk from 0 to 1000. Each day it moves by
//! (80 - city sentiment) / 5, so it only falls in a very happy city, and twice a month
//! it grows by the house level's model increment (in a city of 300 people or more;
//! below that it is wiped). Every tenth day houses whose risk has reached 1000 send out
//! thieves, no more than one per fifty people, and drop back to their level's base
//! risk. A thief loiters a moment, then makes for a mansion (while the governor has
//! savings) or the palace, a tax office or the courthouse (while the treasury has
//! money) and steals from it. Constables, magistrates and some temple complexes'
//! priests wear the risk down as they pass; a constable who meets a thief on his tile
//! overpowers him.

use crate::figures::{Step, Travel};
use crate::world::World;

pub const PROTESTER: u16 = 22;
pub const ROBBER: u16 = 23;
pub const CONSTABLE: u16 = 88;
pub const MAGISTRATE: u16 = 89;
/// Ticks a criminal roams before slipping away.
const CRIMINAL_ROAM: i32 = 200;
/// A thief loiters until his counter, started at 10 to 25, passes this.
const THIEF_LOITER: i32 = 40;
/// Buildings a thief robs, with their weight in the treasury's share.
const MANSIONS: [u16; 3] = [77, 78, 79];
const TREASURY_WEIGHTS: [(u16, i32); 6] = [(187, 50), (188, 50), (189, 50), (86, 2), (87, 2), (184, 3)];
const PALACES: [u16; 3] = crate::buildings::kind::PALACES;

mod thief_action {
    pub const LOITERING: u16 = 1;
    pub const TO_TARGET: u16 = 2;
}
/// The highest crime risk, at which a house sends out a thief.
pub const MAX_CRIME: i32 = 1000;
/// Days between checks for houses ready to send out a thief.
const CRIMINAL_CHECK_DAYS: u64 = 10;

const POLICE_STATION: u16 = 55;
const COURTHOUSE: u16 = 184;

impl World {
    /// Daily: each household's crime risk moves by (80 - sentiment) / 5, 2 less with
    /// Ma'at's altar in a complex to Ra.
    pub(crate) fn update_crime(&mut self) {
        let maat = if self.complex_blessing(crate::temple_complex::RA, crate::temple_complex::ALTAR) { -2 } else { 0 };
        let step = (80 - self.sentiment) / 5 + maat;
        for b in self.buildings.iter_mut() {
            let Some(h) = b.house.as_mut() else { continue };
            if h.population != 0 {
                h.crime = (h.crime + step).clamp(0, MAX_CRIME);
            }
        }
    }

    /// Days 0 and 8 of the month: crime grows by each house level's model increment;
    /// a town of fewer than 300 people has none.
    pub(crate) fn crime_half_month(&mut self) {
        let small = self.population < 300;
        for b in self.buildings.iter_mut() {
            let Some(h) = b.house.as_mut() else { continue };
            if h.population == 0 {
                continue;
            }
            h.crime = if small { 0 } else { h.crime + self.balance.houses.get(h.level as usize).map_or(0, |m| m.crime_increment) };
        }
    }

    /// Tick 45, every tenth day: houses at the highest crime risk, in turn, send out a
    /// thief (unless theirs is still about) and drop to their level's base risk, up to
    /// one house per fifty people.
    pub(crate) fn release_criminals(&mut self) {
        if !self.time.total_days().is_multiple_of(CRIMINAL_CHECK_DAYS) {
            return;
        }
        let mut cap = (self.population as f64 * 0.02f32 as f64) as i32;
        let ready: Vec<u32> = self
            .buildings
            .iter()
            .filter(|b| b.house.as_ref().is_some_and(|h| h.population != 0 && h.crime >= MAX_CRIME))
            .map(|b| b.id)
            .collect();
        for id in ready {
            if cap <= 0 {
                break;
            }
            cap -= 1;
            let base = self.crime_base(id);
            if let Some(h) = self.buildings.get_mut(id).and_then(|b| b.house.as_mut()) {
                h.crime = base;
            }
            let out = self.figures.iter().any(|f| f.kind == ROBBER && f.home == id && !f.dead);
            if !out {
                self.spawn_criminal(id, ROBBER);
            }
        }
    }

    /// The model's base crime risk for a house's level.
    fn crime_base(&self, house: u32) -> i32 {
        let level = self.buildings.get(house).and_then(|b| b.house.as_ref()).map_or(0, |h| h.level as usize);
        self.balance.houses.get(level).map_or(0, |m| m.crime_base)
    }

    fn spawn_criminal(&mut self, house: u32, kind: u16) {
        let Some(b) = self.buildings.get(house) else { return };
        let Some((x, y)) = crate::buildings::road_within(&self.map, b.x, b.y, b.size, 2) else { return };
        let fid = self.figures.spawn(kind, x, y, Travel::Roads);
        if kind == ROBBER {
            let wait = 10 + (self.map.random.at_or(b.x, b.y, 0) as i32 & 15);
            if let Some(f) = self.figures.get_mut(fid) {
                f.home = house;
                f.action = thief_action::LOITERING;
                f.counter = wait;
            }
            return;
        }
        let turn = self.rng.byte();
        if let Some(f) = self.figures.get_mut(fid) {
            f.home = house;
            f.action = 1;
            f.roam_left = CRIMINAL_ROAM * f.speed.max(1) as i32;
            f.roam_turn = if turn & 1 == 0 { 2 } else { -2 };
        }
    }

    /// Whether the city's palace (the first built) has staff.
    pub fn palace_staffed(&self) -> bool {
        self.buildings.iter().filter(|b| PALACES.contains(&b.kind)).min_by_key(|b| b.id).is_some_and(|b| b.workers > 0)
    }

    /// Money building `id` holds for a thief: the governor's savings at a mansion, or
    /// its weighted share of the treasury.
    fn loot(&self, id: u32) -> i32 {
        let Some(b) = self.buildings.get(id) else { return 0 };
        if MANSIONS.contains(&b.kind) {
            return self.governor.savings.max(0);
        }
        let weight = |k: u16| TREASURY_WEIGHTS.iter().find(|w| w.0 == k).map_or(0, |w| w.1);
        let total: i32 = self.buildings.iter().map(|o| weight(o.kind)).sum();
        if self.treasury <= 0 || total <= 0 {
            return 0;
        }
        (self.treasury as i64 * weight(b.kind) as i64 / total as i64) as i32
    }

    /// A thief loiters, then walks to the nearest building worth robbing, steals from it
    /// and is gone. With nothing to rob, or no way there, he slips away. A constable on
    /// the same tile overpowers him.
    pub(crate) fn update_thief(&mut self, fid: u32) {
        let Some(f) = self.figures.get(fid) else { return };
        let (act, pos) = (f.action, (f.x, f.y));
        if self.figures.iter().any(|c| c.kind == CONSTABLE && !c.dead && (c.x, c.y) == pos) {
            self.figures.get_mut(fid).expect("present").dead = true;
            return;
        }
        match act {
            thief_action::LOITERING => {
                let f = self.figures.get_mut(fid).expect("present");
                f.counter += 1;
                if f.counter <= THIEF_LOITER {
                    return;
                }
                let target = self
                    .buildings
                    .iter()
                    .filter(|b| b.road.is_some() && (MANSIONS.contains(&b.kind) || TREASURY_WEIGHTS.iter().any(|w| w.0 == b.kind)))
                    .filter(|b| self.loot(b.id) > 0)
                    .min_by_key(|b| ((b.x - pos.0).abs().max((b.y - pos.1).abs()), b.id))
                    .and_then(|b| b.road.map(|r| (b.id, r)));
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                match target {
                    Some((id, road)) if f.go_to(map, road) => {
                        f.target = id;
                        f.action = thief_action::TO_TARGET;
                    }
                    _ => f.dead = true,
                }
            }
            _ => {
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                match f.walk(map) {
                    Step::Moving => {}
                    Step::Arrived => {
                        let target = f.target;
                        f.dead = true;
                        self.rob(target, pos);
                    }
                    _ => f.dead = true,
                }
            }
        }
    }

    /// A thief steals 9% of what building `id` holds.
    fn rob(&mut self, id: u32, at: (i32, i32)) {
        let Some(k) = self.buildings.get(id).map(|b| b.kind) else { return };
        let stolen = self.loot(id) * self.by_difficulty(crate::difficulty::THEFT_PCT) / 100;
        if stolen <= 0 {
            return;
        }
        if MANSIONS.contains(&k) {
            self.governor.savings -= stolen;
        } else {
            self.treasury -= stolen;
        }
        self.post_trouble("message_city_crime", at, crate::missions::Condition::Crime);
    }

    /// Takes `amount` off the crime risk of the houses around a passing walker, down
    /// to each house level's base risk.
    pub(crate) fn calm_houses(&mut self, houses: &[u32], amount: i32) {
        for &id in houses {
            let base = self.crime_base(id);
            if let Some(h) = self.buildings.get_mut(id).and_then(|b| b.house.as_mut()) {
                h.crime = (h.crime - amount).max(base).min(MAX_CRIME);
            }
        }
    }

    /// A constable or magistrate passes: the crime risk of the houses around him falls
    /// by his station's model column j (100 for a constable, 500 for a magistrate on
    /// Normal).
    pub(crate) fn patrol(&mut self, kind: u16, houses: &[u32]) {
        let station = if kind == MAGISTRATE { COURTHOUSE } else { POLICE_STATION };
        self.calm_houses(houses, self.balance.stats(station).j);
        if kind == MAGISTRATE {
            for &id in houses {
                if let Some(h) = self.buildings.get_mut(id).and_then(|b| b.house.as_mut()) {
                    h.coverage.magistrate = crate::services::VISIT;
                }
            }
        }
    }

    /// Sick citizens and protesters wander the roads with no home to return
    /// to, and are gone when their wandering is over.
    pub(crate) fn update_wanderer(&mut self, fid: u32) {
        let Some(f) = self.figures.get(fid) else { return };
        let kind = f.kind;
        if !f.moving {
            let (x, y) = (f.x, f.y);
            if kind == crate::health::PLAGUED_CITIZEN {
                self.spread_plague(x, y);
            }
            let done = self.figures.get(fid).is_some_and(|f| f.roam_left <= 0);
            let next = if done { None } else { self.roam_direction(fid) };
            let Some(f) = self.figures.get_mut(fid) else { return };
            match next {
                Some(d) => {
                    f.route.clear();
                    f.route.push_back(d);
                }
                None => {
                    f.dead = true;
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
}
