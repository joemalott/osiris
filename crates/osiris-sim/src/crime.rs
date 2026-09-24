//! Crime: every household carries a crime risk from 0 to 1000. Each day it moves by
//! (80 - city sentiment) / 5, so it only falls in a very happy city, and twice a month
//! it grows by the house level's model increment (in a city of 300 people or more;
//! below that it is wiped). Every tenth day each house whose risk has reached 1000
//! sends out a thief and drops back to its level's base risk. Constables, magistrates
//! and some temple complexes' priests wear the risk down as they pass; constables
//! catch the thieves they meet three times in four.

use crate::figures::{Step, Travel};
use crate::world::World;

pub const PROTESTER: u16 = 22;
pub const ROBBER: u16 = 23;
pub const CONSTABLE: u16 = 88;
pub const MAGISTRATE: u16 = 89;
/// Ticks a criminal roams before slipping away.
const CRIMINAL_ROAM: i32 = 200;
/// Tiles within which a constable catches criminals.
const ARREST_RANGE: i32 = 2;
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

    /// Tick 45, every tenth day: each house at the highest crime risk that has no
    /// thief of its own still about sends one out, and its risk drops to its level's
    /// base.
    pub(crate) fn release_criminals(&mut self) {
        if !self.time.total_days().is_multiple_of(CRIMINAL_CHECK_DAYS) {
            return;
        }
        let ready: Vec<u32> = self
            .buildings
            .iter()
            .filter(|b| b.house.as_ref().is_some_and(|h| h.population != 0 && h.crime >= MAX_CRIME))
            .map(|b| b.id)
            .collect();
        for id in ready {
            let base = self.crime_base(id);
            if let Some(h) = self.buildings.get_mut(id).and_then(|b| b.house.as_mut()) {
                h.crime = base;
            }
            let out = self.figures.iter().any(|f| f.kind == ROBBER && f.home == id);
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
        let turn = self.rng.byte();
        if let Some(f) = self.figures.get_mut(fid) {
            f.home = house;
            f.action = 1;
            f.roam_left = CRIMINAL_ROAM * f.speed.max(1) as i32;
            f.roam_turn = if turn & 1 == 0 { 2 } else { -2 };
        }
        if kind == ROBBER {
            // The thief robs the treasury: a quarter of this year's taxes, up to 400.
            let taxes = self.finance.this_year.taxes;
            let stolen = if taxes > 20 { (taxes / 4).min(400) } else if self.treasury > 0 { self.rng.below(50) } else { 0 };
            self.treasury -= stolen;
            self.post_trouble("message_city_crime", (x, y), crate::missions::Condition::Crime);
        }
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
    /// Normal), and a constable may catch a criminal nearby.
    pub(crate) fn patrol(&mut self, kind: u16, x: i32, y: i32, houses: &[u32]) {
        let station = if kind == MAGISTRATE { COURTHOUSE } else { POLICE_STATION };
        self.calm_houses(houses, self.balance.stats(station).j);
        if kind == MAGISTRATE {
            for &id in houses {
                if let Some(h) = self.buildings.get_mut(id).and_then(|b| b.house.as_mut()) {
                    h.coverage.magistrate = crate::services::VISIT;
                }
            }
        }
        if kind != CONSTABLE {
            return;
        }
        for fid in self.figures.ids() {
            let near = self.figures.get(fid).is_some_and(|f| matches!(f.kind, PROTESTER | ROBBER) && (f.x - x).abs() <= ARREST_RANGE && (f.y - y).abs() <= ARREST_RANGE);
            if near && self.rng.below(100) < 75
                && let Some(f) = self.figures.get_mut(fid)
            {
                f.dead = true;
            }
        }
    }

    /// Sick citizens, protesters and thieves wander the roads with no home to return
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
