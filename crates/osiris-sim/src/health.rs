//! Health. Once a month every household's disease and malaria risks grow, by its
//! level's model increments and the city's poor health, malaria more near marsh and
//! water. A house whose risk reaches 1000 is wiped out; malaria then creeps on to the
//! houses around it over the following months. The city's health moves two points a
//! month toward the share of people cared for by mortuaries and physicians and fed,
//! and in a sickly city plague strikes the house most at risk and sends a sick citizen
//! wandering the streets, wiping out the households he passes. A stricken house is
//! quarantined for two months. Physicians and herbalists wear the disease and malaria
//! risks down as they pass.

use crate::map::terrain;
use crate::world::World;

/// The sick citizen who carries plague from house to house.
pub const PLAGUED_CITIZEN: u16 = 98;
/// Tiles a plagued citizen walks.
const PLAGUED_ROAM: i32 = 500;
/// The highest disease or malaria risk, at which the house is wiped out.
pub const MAX_RISK: i32 = 1000;
/// Months a stricken house stays quarantined.
const QUARANTINE_MONTHS: i32 = 2;
/// City health bonus in a small city: below each population, the bonus.
const SMALL_CITY_BONUS: [(i32, i32); 10] =
    [(200, 100), (400, 90), (600, 80), (800, 70), (1000, 60), (1100, 50), (1200, 40), (1300, 30), (1400, 20), (1500, 10)];

/// Messages to post at the end of the month's health update.
#[derive(Default)]
struct Outbreaks {
    malaria: Option<u32>,
    disease: Option<u32>,
    plague: Option<u32>,
}

impl World {
    /// Monthly: household risks and outbreaks, the city's health, and plague.
    pub(crate) fn update_health_month(&mut self) {
        let health = self.ratings.health;
        let desert = self.climate == 2;
        let houses: Vec<u32> = self.buildings.iter().filter(|b| b.house.as_ref().is_some_and(|h| h.population != 0)).map(|b| b.id).collect();
        let marsh = self.tiles_with(terrain::MARSHLAND);
        let water = self.tiles_with(terrain::WATER);
        let far = self.map.width.max(self.map.height);
        let mut out = Outbreaks::default();
        let (mut people, mut cared) = (0, 0);
        for id in houses {
            let Some(b) = self.buildings.get(id) else { continue };
            let Some(h) = &b.house else { continue };
            people += h.population;
            let model = *self.balance.house(h.level);
            let marsh_dist = nearest(&marsh, b.x, b.y).unwrap_or(far);
            let water_dist = if self.climate == 0 { 1 } else { nearest(&water, b.x, b.y).unwrap_or(far) };
            let (k, div) = if desert { (8, 2) } else { (16, 1) };
            let malaria_step = (model.malaria_increment - health + 2 * (55 - marsh_dist) - water_dist + k) / div;
            let Some(h) = self.buildings.get_mut(id).and_then(|b| b.house.as_mut()) else { continue };
            h.disease_risk = (h.disease_risk + model.disease_increment + 2 * (50 - health)).clamp(0, MAX_RISK);
            h.malaria_risk = (h.malaria_risk + malaria_step).clamp(0, MAX_RISK);
            if !self.rules.disease {
                // Outbreaks are switched off.
            } else if h.malaria_risk == MAX_RISK {
                self.malaria_strikes(id);
                out.malaria = Some(id);
            } else if h.disease_risk == MAX_RISK {
                self.wipe_out(id);
                out.disease = Some(id);
            }
            let Some(h) = self.buildings.get(id).and_then(|b| b.house.as_ref()) else { continue };
            cared += care_points(h, h.foods.iter().filter(|&&f| f > 0).count() as i32);
        }
        let mut target = if people > 0 { cared * 100 / people } else { 0 };
        target += SMALL_CITY_BONUS.iter().find(|&&(below, _)| self.population < below).map_or(0, |&(_, bonus)| bonus);
        // Isis's altar (a complex to Bast) makes the city healthier.
        if self.complex_blessing(crate::temple_complex::BAST, crate::temple_complex::ALTAR) {
            target = target * 105 / 100;
        }
        self.rng.next();
        if self.rules.disease && self.rng.short() % 100 < 40 - health {
            out.plague = self.start_plague(false);
        }
        let r = &mut self.ratings;
        r.health_target = target;
        r.health = if r.health < target { (r.health + 2).min(target) } else { (r.health - 2).max(target) }.clamp(0, 100);
        for b in self.buildings.iter_mut() {
            if let Some(h) = b.house.as_mut() {
                h.quarantine = (h.quarantine - 1).max(0);
            }
        }
        let creeping: Vec<u32> = self
            .buildings
            .iter_mut()
            .filter_map(|b| {
                let h = b.house.as_mut().filter(|h| h.population != 0 && h.malaria_countdown != 0)?;
                h.malaria_countdown -= 1;
                (h.malaria_countdown == 0).then_some(b.id)
            })
            .collect();
        for id in creeping {
            self.wipe_out(id);
        }
        for (house, key) in [(out.plague, "message_a_plague"), (out.disease, "message_disease_strikes"), (out.malaria, "message_malaria")] {
            if let Some(tile) = house.and_then(|id| self.buildings.get(id)).map(|b| (b.x, b.y)) {
                self.events.disease = true;
                self.post_trouble(key, tile, crate::missions::Condition::Disease);
            }
        }
    }

    /// The tiles with terrain `mask`.
    fn tiles_with(&self, mask: u32) -> Vec<(i32, i32)> {
        let m = &self.map;
        (0..m.height).flat_map(|y| (0..m.width).map(move |x| (x, y))).filter(|&(x, y)| m.terrain_is(x, y, mask)).collect()
    }

    /// Everyone in a house dies; it stays empty and quarantined for two months.
    pub(crate) fn wipe_out(&mut self, id: u32) {
        let Some(h) = self.buildings.get_mut(id).and_then(|b| b.house.as_mut()) else { return };
        let dead = h.population;
        h.population = 0;
        h.quarantine = QUARANTINE_MONTHS;
        h.disease_risk = 0;
        h.malaria_risk = 0;
        self.population -= dead;
        self.census.remove(&self.rng, dead);
    }

    /// Malaria wipes out a house, and will reach every house around it within
    /// (100 - health) / 20 tiles (one more in the last three months of the year, at
    /// most 4), a month later for each tile away.
    fn malaria_strikes(&mut self, id: u32) {
        self.wipe_out(id);
        let late = matches!(self.time.month, 9..=11) as i32;
        let reach = ((100 - self.ratings.health) / 20 + late).min(4);
        if reach < 1 {
            return;
        }
        let Some(b) = self.buildings.get(id) else { return };
        let (bx, by) = (b.x, b.y);
        for y in by - reach..=by + reach {
            for x in bx - reach..=bx + reach {
                let other = self.map.building.at_or(x, y, 0);
                if let Some(h) = self.buildings.get_mut(other).and_then(|b| b.house.as_mut())
                    && h.population != 0
                {
                    h.malaria_countdown = (x - bx).abs().max((y - by).abs());
                }
            }
        }
    }

    /// Plague strikes the house most at risk of disease (Bast's curse picks any
    /// house when none is) and a sick citizen sets out from it. Returns the house.
    pub(crate) fn start_plague(&mut self, forced: bool) -> Option<u32> {
        let mut house = self
            .buildings
            .iter()
            .filter_map(|b| b.house.as_ref().filter(|h| h.population != 0 && h.disease_risk > 0).map(|h| (h.disease_risk, b.id)))
            .fold(None, |best: Option<(i32, u32)>, c| if best.is_some_and(|b| b.0 >= c.0) { best } else { Some(c) })
            .map(|(_, id)| id);
        if forced && house.is_none() {
            let occupied: Vec<u32> = self.buildings.iter().filter(|b| b.house.as_ref().is_some_and(|h| h.population != 0)).map(|b| b.id).collect();
            if !occupied.is_empty() {
                self.rng.next();
                house = Some(occupied[self.rng.short() as usize % occupied.len()]);
            }
        }
        let id = house?;
        self.wipe_out(id);
        let b = self.buildings.get(id)?;
        if let Some((x, y)) = crate::buildings::road_within(&self.map, b.x, b.y, b.size, 2) {
            let fid = self.figures.spawn(PLAGUED_CITIZEN, x, y, crate::figures::Travel::Roads);
            let turn = self.rng.byte();
            if let Some(f) = self.figures.get_mut(fid) {
                f.home = id;
                f.action = 1;
                f.roam_left = PLAGUED_ROAM;
                f.roam_turn = if turn & 1 == 0 { 2 } else { -2 };
            }
        }
        Some(id)
    }

    /// A sick citizen passes: every inhabited house within two tiles not already
    /// quarantined is wiped out.
    pub(crate) fn spread_plague(&mut self, x: i32, y: i32) {
        for yy in y - 2..=y + 2 {
            for xx in x - 2..=x + 2 {
                let id = self.map.building.at_or(xx, yy, 0);
                if self.buildings.get(id).and_then(|b| b.house.as_ref()).is_some_and(|h| h.population != 0 && h.quarantine == 0) {
                    self.wipe_out(id);
                }
            }
        }
    }

    /// A priest of Bast with Isis's altar passes: sick citizens next to him are cured.
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

/// A household's share of the city's health: 25 for a mortuary, 15 for a physician,
/// 10, 15 or 20 for one, two or three foods, and 40 more for food and a physician,
/// as a percentage of its people.
fn care_points(h: &crate::houses::House, foods: i32) -> i32 {
    let c = &h.coverage;
    let doctor = c.physician > 0;
    let mut points = if c.mortuary > 0 { 25 } else { 0 };
    if doctor {
        points += 15;
    }
    points += match foods {
        0 => 0,
        1 => 10,
        2 => 15,
        _ => 20,
    };
    if foods > 0 && doctor {
        points += 40;
    }
    // The original scales by a single-precision 0.01 and truncates.
    (points as f64 * 0.01f32 as f64 * h.population as f64) as i32
}

/// Whole-tile distance from `(x, y)` to the nearest of `tiles`.
fn nearest(tiles: &[(i32, i32)], x: i32, y: i32) -> Option<i32> {
    tiles.iter().map(|&(tx, ty)| (tx - x) * (tx - x) + (ty - y) * (ty - y)).min().map(|d2| (d2 as f64).sqrt() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_tile_distance_truncates() {
        assert_eq!(nearest(&[(3, 4), (10, 10)], 0, 0), Some(5));
        assert_eq!(nearest(&[(1, 1)], 0, 0), Some(1));
        assert_eq!(nearest(&[], 0, 0), None);
    }

    #[test]
    fn care_points_weigh_mortuary_physician_and_food() {
        let mut h = crate::houses::House { population: 100, ..Default::default() };
        assert_eq!(care_points(&h, 0), 0);
        h.coverage.mortuary = 96;
        h.coverage.physician = 96;
        // 25 + 15 + 20 + 40, truncated after the single-precision 0.01.
        assert_eq!(care_points(&h, 3), 99);
    }
}
