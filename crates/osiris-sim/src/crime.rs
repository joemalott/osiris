//! Crime: unhappy households brew crime. Once a day the unhappiest household in the
//! city may turn to it, the more likely the lower the city's sentiment: a protester
//! takes to the streets, or, when things are bad, a thief who robs the treasury as he
//! goes. Constables and magistrates on their rounds calm the houses they pass, and
//! constables catch the criminals they meet three times in four.

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

impl World {
    /// Daily: crime brews in unhappy homes and fades in happy ones, and the unhappiest
    /// household may act on it.
    pub(crate) fn update_crime(&mut self) {
        for b in self.buildings.iter_mut() {
            let Some(h) = b.house.as_mut() else { continue };
            if h.population <= 0 || h.happiness >= 50 {
                h.criminal_active = (h.criminal_active - 1).max(0);
            } else {
                h.criminal_active = (h.criminal_active + (50 - h.happiness) / 10 + 1).min(100);
            }
        }
        let Some((id, happiness)) = self
            .buildings
            .iter()
            .filter_map(|b| b.house.as_ref().filter(|h| h.population > 0 && h.happiness < 50).map(|h| (b.id, h.happiness)))
            .min_by_key(|&(_, happy)| happy)
        else {
            return;
        };
        let sentiment = self.sentiment;
        self.rng.next();
        let roll = self.rng.byte();
        let (threshold, robbers_below) = match sentiment {
            s if s < 30 => (s + 50, 30),
            s if s < 60 => (s + 40, 30),
            s => (s + 20, 0),
        };
        if roll < threshold {
            return;
        }
        let Some(h) = self.buildings.get(id).and_then(|b| b.house.as_ref()) else { return };
        let active = h.criminal_active;
        let kind = if happiness < robbers_below && active > 60 {
            ROBBER
        } else if active > 30 {
            PROTESTER
        } else {
            return;
        };
        if let Some(h) = self.buildings.get_mut(id).and_then(|b| b.house.as_mut()) {
            h.criminal_active -= if kind == ROBBER { 60 } else { 30 };
        }
        self.spawn_criminal(id, kind);
    }

    fn spawn_criminal(&mut self, house: u32, kind: u16) {
        let Some(b) = self.buildings.get(house) else { return };
        let Some((x, y)) = crate::buildings::road_within(&self.map, b.x, b.y, b.size, 2) else { return };
        let fid = self.figures.spawn(kind, x, y, Travel::Roads);
        let turn = self.rng.byte();
        if let Some(f) = self.figures.get_mut(fid) {
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

    /// A constable or magistrate passes: households around him calm down, and a
    /// constable may catch a criminal nearby.
    pub(crate) fn patrol(&mut self, kind: u16, x: i32, y: i32, houses: &[u32]) {
        for &id in houses {
            if let Some(h) = self.buildings.get_mut(id).and_then(|b| b.house.as_mut()) {
                h.criminal_active = (h.criminal_active - 1).max(0);
                if kind == MAGISTRATE {
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
