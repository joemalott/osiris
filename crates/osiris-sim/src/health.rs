//! Health: each household's own health drifts toward what its apothecary, physician
//! and dentist give it (a third each). When the city's health falls below 40, plague
//! may break out once a month: 7-10% of the people fall sick, less the city's
//! mortuary workers, starting with the unhealthiest homes. A plagued home loses people
//! day by day for 30 days, and a sick citizen wanders the streets spreading it;
//! apothecaries' herbalists cure the sick they meet.

use crate::world::World;

/// The sick citizen who carries plague from house to house.
pub const PLAGUED_CITIZEN: u16 = 98;
const PLAGUE_DAYS: i32 = 30;
/// City health below which plague can break out.
const OUTBREAK_BELOW: i32 = 40;
/// Tiles a sick citizen infects around him, and how far he wanders.
const INFECT_RADIUS: i32 = 1;
const PLAGUED_ROAM: i32 = 480;

impl World {
    /// Daily: household health drifts, and plague takes its toll.
    pub(crate) fn update_house_health(&mut self) {
        let mut deaths = 0;
        let mut rolls = Vec::new();
        for b in self.buildings.iter() {
            if let Some(h) = &b.house
                && h.population > 0
                && h.plague_days > 0
            {
                rolls.push(b.id);
            }
        }
        let chances: Vec<(u32, i32)> = rolls.into_iter().map(|id| (id, 1 + self.rng.below(99))).collect();
        for b in self.buildings.iter_mut() {
            let Some(h) = b.house.as_mut() else { continue };
            let c = &h.coverage;
            let target = 33 * (c.apothecary > 0) as i32 + 33 * (c.physician > 0) as i32 + 33 * (c.dentist > 0) as i32;
            h.common_health = if h.common_health > target {
                h.common_health - 1
            } else if h.common_health < target {
                h.common_health + 1
            } else {
                h.common_health
            };
            if h.plague_days > 0 {
                h.plague_days -= 1;
                if let Some(&(_, roll)) = chances.iter().find(|c| c.0 == b.id) {
                    let chance = (100 - h.common_health).max(10);
                    if roll < chance && h.population > 0 {
                        h.population -= 1;
                        deaths += 1;
                    }
                }
            }
        }
        if deaths > 0 {
            self.census.remove(&self.rng, deaths);
        }
    }

    /// Monthly, after the health rating: plague may break out in a sickly city.
    pub(crate) fn check_outbreak(&mut self) {
        if !self.rules.disease || self.population < 200 {
            return;
        }
        let health = self.ratings.health;
        if health >= OUTBREAK_BELOW || self.rng.below(64) > OUTBREAK_BELOW - health {
            return;
        }
        self.start_plague(false);
    }

    /// Plague strikes: some of the people fall sick, fewer the more mortuary workers
    /// there are. `forced` is Bast's curse, which strikes whatever the city's health.
    pub(crate) fn start_plague(&mut self, forced: bool) {
        let _ = forced;
        let sick = self.population * (7 + self.rng.below(4)) / 100;
        let embalmers: i32 = self.buildings.iter().filter(|b| b.kind == crate::ratings::MORTUARY).map(|b| b.workers).sum();
        let mut to_infect = sick - embalmers;
        self.ratings.health = (self.ratings.health + 10).min(100);
        if to_infect <= 0 {
            self.post("message_malaria", None, true);
            return;
        }
        // The unhealthiest homes first, then those without a doctor, then huts, then any.
        let mut houses: Vec<(u8, u32, i32)> = self
            .buildings
            .iter()
            .filter_map(|b| {
                let h = b.house.as_ref()?;
                if h.population <= 0 || h.plague_days > 0 {
                    return None;
                }
                let doctor = h.coverage.apothecary > 0 || h.coverage.physician > 0;
                let rank = if h.common_health < 10 {
                    0
                } else if !doctor {
                    1
                } else if h.level <= 3 {
                    2
                } else {
                    3
                };
                Some((rank, b.id, h.population))
            })
            .collect();
        houses.sort();
        for (_, id, people) in houses {
            if to_infect <= 0 {
                break;
            }
            to_infect -= people;
            self.infect_house(id);
        }
        self.events.disease = true;
        let key = if embalmers > 0 { "message_a_plague" } else { "message_disease_strikes" };
        let tile = self.buildings.iter().find(|b| b.house.as_ref().is_some_and(|h| h.plague_days > 0)).map(|b| (b.x, b.y));
        match tile {
            Some(tile) => self.post_trouble(key, tile, crate::missions::Condition::Disease),
            None => self.post(key, None, true),
        }
    }

    /// A house falls to plague for 30 days and sends out a sick citizen.
    fn infect_house(&mut self, id: u32) {
        let Some(b) = self.buildings.get_mut(id) else { return };
        let Some(h) = b.house.as_mut() else { return };
        h.plague_days = PLAGUE_DAYS;
        let road = crate::buildings::road_within(&self.map, b.x, b.y, b.size, 2);
        if let Some((x, y)) = road {
            let fid = self.figures.spawn(PLAGUED_CITIZEN, x, y, crate::figures::Travel::Roads);
            let turn = self.rng.byte();
            if let Some(f) = self.figures.get_mut(fid) {
                f.action = 1;
                f.roam_left = PLAGUED_ROAM;
                f.roam_turn = if turn & 1 == 0 { 2 } else { -2 };
            }
        }
    }

    /// A sick citizen passes: houses around him fall ill.
    pub(crate) fn spread_plague(&mut self, x: i32, y: i32) {
        let mut hit = Vec::new();
        for yy in y - INFECT_RADIUS..=y + INFECT_RADIUS {
            for xx in x - INFECT_RADIUS..=x + INFECT_RADIUS {
                let id = self.map.building.at_or(xx, yy, 0);
                if id != 0 && !hit.contains(&id) {
                    hit.push(id);
                }
            }
        }
        for id in hit {
            if let Some(h) = self.buildings.get_mut(id).and_then(|b| b.house.as_mut())
                && h.population > 0
                && h.plague_days == 0
            {
                h.plague_days = PLAGUE_DAYS;
            }
        }
    }

    /// A herbalist passes: sick citizens next to him are cured.
    pub(crate) fn cure_plagued_near(&mut self, x: i32, y: i32) {
        for fid in self.figures.ids() {
            if let Some(f) = self.figures.get_mut(fid)
                && f.kind == PLAGUED_CITIZEN
                && (f.x - x).abs() <= 1
                && (f.y - y).abs() <= 1
            {
                f.dead = true;
            }
        }
    }
}
