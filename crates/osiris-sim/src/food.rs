//! Food: bazaar buyers fetch food from granaries, bazaar traders hand it out to houses
//! along their route, and houses eat it every half month.

use crate::buildings::{BuildingId, kind};
use crate::economy::{LOAD, resource};
use crate::figures::{Step, Travel};
use crate::world::World;

pub const MARKET_TRADER: u16 = 26;
pub const MARKET_BUYER: u16 = 39;

/// Bazaar stock caps: grain holds a little more than other foods.
const CAP_GRAIN: i32 = 700;
const CAP_OTHER: i32 = 600;

/// Per house level: food each resident may keep (x population) and the weekly food
/// consumption percentage before the difficulty adjustment.
const FOOD_STORAGE_MULTIPLIER: [i32; 20] = [4, 4, 4, 4, 5, 5, 5, 5, 5, 5, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6];
const FOOD_CONSUMPTION_PCT: [i32; 20] = [30, 32, 34, 36, 38, 40, 42, 44, 46, 48, 50, 52, 54, 56, 58, 60, 62, 64, 66, 68];
/// Consumption reduction on Normal difficulty.
const CONSUMPTION_REDUCTION_PCT: i32 = 30;

mod action {
    pub const TO_GRANARY: u16 = 1;
    pub const HOME: u16 = 2;
}

impl World {
    fn bazaar_cap(r: u16) -> i32 {
        if r == resource::GRAIN { CAP_GRAIN } else { CAP_OTHER }
    }

    /// The food a bazaar most needs, if any: the one it holds least of, below its cap.
    fn bazaar_wants(&self, id: BuildingId) -> Option<u16> {
        let b = self.buildings.get(id)?;
        (resource::GRAIN..=resource::GAMEMEAT)
            .filter(|&r| b.stock[r as usize] < Self::bazaar_cap(r) - LOAD)
            .filter(|&r| self.buildings.iter().any(|g| g.kind == kind::GRANARY && g.stock[r as usize] > 0))
            .min_by_key(|&r| b.stock[r as usize])
    }

    /// Tick 31: bazaars send a trader when stocked and a buyer when short.
    pub(crate) fn bazaar_walkers(&mut self) {
        let bazaars: Vec<BuildingId> = self.buildings.iter().filter(|b| b.kind == kind::BAZAAR).map(|b| b.id).collect();
        for id in bazaars {
            let Some(b) = self.buildings.get(id) else { continue };
            if b.workers <= 0 || b.road.is_none() {
                continue;
            }
            let has_food = (resource::GRAIN..=resource::GAMEMEAT).any(|r| b.stock[r as usize] > 0);
            let (trader, buyer) = (b.walkers[0], b.walkers[2]);
            if trader == 0 && has_food {
                self.spawn_roamer(id, MARKET_TRADER, 0);
            }
            if buyer == 0
                && let Some(r) = self.bazaar_wants(id)
            {
                self.spawn_buyer(id, r);
            }
        }
    }

    fn spawn_buyer(&mut self, bazaar: BuildingId, r: u16) {
        let Some(b) = self.buildings.get(bazaar) else { return };
        let Some(road) = b.road else { return };
        let from = (b.x, b.y);
        let granary = self
            .buildings
            .iter()
            .filter(|g| g.kind == kind::GRANARY && g.stock[r as usize] > 0 && g.road.is_some())
            .min_by_key(|g| (g.x - from.0).abs() + (g.y - from.1).abs())
            .map(|g| (g.id, g.road.unwrap()));
        let Some((gid, groad)) = granary else { return };
        let fid = self.figures.spawn(MARKET_BUYER, road.0, road.1, Travel::Roads);
        let map = &self.map;
        if let Some(f) = self.figures.get_mut(fid) {
            f.home = bazaar;
            f.target = gid;
            f.cargo = r;
            f.action = action::TO_GRANARY;
            if !f.go_to(map, groad) {
                f.dead = true;
            }
        }
        if let Some(b) = self.buildings.get_mut(bazaar) {
            b.walkers[2] = fid;
        }
    }

    pub(crate) fn update_buyer(&mut self, fid: u32) {
        let map = &self.map;
        let Some(f) = self.figures.get_mut(fid) else { return };
        let step = f.walk(map);
        let (act, home, target, r) = (f.action, f.home, f.target, f.cargo);
        match (act, step) {
            (_, Step::Moving) => {}
            (action::TO_GRANARY, Step::Arrived) => {
                let want = self
                    .buildings
                    .get(home)
                    .map_or(0, |b| Self::bazaar_cap(r) - b.stock[r as usize])
                    .max(0);
                let take = self.buildings.get(target).map_or(0, |g| g.stock[r as usize]).min(want).min(4 * LOAD);
                if let Some(g) = self.buildings.get_mut(target) {
                    g.stock[r as usize] -= take;
                }
                let home_road = self.buildings.get(home).and_then(|b| b.road);
                let map = &self.map;
                let Some(f) = self.figures.get_mut(fid) else { return };
                f.amount = take;
                f.action = action::HOME;
                match home_road {
                    Some(hr) if f.go_to(map, hr) => {}
                    _ => f.dead = true,
                }
            }
            (action::HOME, Step::Arrived) => {
                let amount = f.amount;
                f.dead = true;
                if let Some(b) = self.buildings.get_mut(home) {
                    b.stock[r as usize] += amount;
                }
            }
            _ => f.dead = true,
        }
    }

    /// A bazaar trader's delivery to one house.
    pub(crate) fn deliver_food(&mut self, bazaar: BuildingId, house: BuildingId) {
        let Some(h) = self.buildings.get(house).and_then(|b| b.house.clone()) else { return };
        let level = h.level as usize;
        let max_stock = FOOD_STORAGE_MULTIPLIER[level] * h.population.max(1);
        // Houses stock up for the level they are growing into.
        let next = (level + 1).min(self.balance.houses.len() - 1);
        let types_wanted = self.balance.houses[next].food_types.max(self.balance.houses[level].food_types);
        let full = h.foods.iter().filter(|&&f| f >= max_stock).count() as i32;
        if types_wanted <= full {
            return;
        }
        let Some(market) = self.buildings.get(bazaar) else { return };
        let mut give: Option<(u16, usize, i32)> = None;
        for r in resource::GRAIN..=resource::GAMEMEAT {
            let Some(slot) = resource::food_slot(r) else { continue };
            if h.foods[slot] >= max_stock || market.stock[r as usize] <= 0 {
                continue;
            }
            give = Some((r, slot, market.stock[r as usize].min(max_stock)));
            break;
        }
        let Some((r, slot, n)) = give else { return };
        if let Some(m) = self.buildings.get_mut(bazaar) {
            m.stock[r as usize] -= n;
        }
        if let Some(h) = self.buildings.get_mut(house).and_then(|b| b.house.as_mut()) {
            h.foods[slot] += n;
        }
    }

    /// Twice a month: every house eats.
    pub(crate) fn consume_food(&mut self) {
        let houses = self.balance.houses.clone();
        for b in self.buildings.iter_mut() {
            let Some(h) = b.house.as_mut() else { continue };
            let level = h.level as usize;
            let types = houses.get(level).map_or(0, |m| m.food_types);
            if h.population <= 0 || types <= 0 {
                continue;
            }
            let pct = FOOD_CONSUMPTION_PCT[level] * (100 - CONSUMPTION_REDUCTION_PCT) / 100 / 5 * 5;
            let mut per_type = h.population * pct / 100;
            if types > 1 {
                per_type /= types;
            }
            if per_type > 0 {
                per_type = (per_type / 2).max(1);
            }
            let mut eaten = 0;
            for f in h.foods.iter_mut() {
                if eaten >= types {
                    break;
                }
                if *f > 0 {
                    *f = (*f - per_type).max(0);
                    eaten += 1;
                }
            }
        }
    }
}
