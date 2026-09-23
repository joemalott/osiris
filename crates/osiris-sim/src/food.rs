//! Food: bazaar buyers fetch food from granaries, bazaar traders hand it out to houses
//! along their route, and houses eat it every half month.

use crate::buildings::{BuildingId, kind};
use crate::economy::{LOAD, resource};
use crate::figures::{Step, Travel};
use crate::world::World;

pub const MARKET_TRADER: u16 = 26;
pub const MARKET_BUYER: u16 = 39;

/// How far a bazaar sends its buyer, in tiles.
const MAX_SEARCH: i32 = 40;
/// A bazaar restocks a food below these amounts, by the food's place in its list.
const PICK_FOOD_BELOW: [i32; 4] = [600, 400, 200, 100];
/// ...and pottery, luxury goods, linen and beer below these.
const PICK_GOOD_BELOW: [i32; 4] = [150, 100, 50, 25];
/// A trader leaves a house enough of each good for this many residents per ten.
const GOODS_PER_TEN: i32 = 8;

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
    /// Where a bazaar looks for stock: storage buildings within 40 tiles holding at least
    /// a load of `r`, nearest first.
    fn bazaar_source(&self, bazaar: BuildingId, r: u16) -> Option<BuildingId> {
        let b = self.buildings.get(bazaar)?;
        let from = (b.x, b.y);
        self.buildings
            .iter()
            .filter(|s| crate::storage::is_storage(s.kind) && s.road.is_some() && !self.is_stockpiled(r) && self.stored(s.id, r) >= LOAD)
            .filter(|s| (s.x - from.0).abs().max((s.y - from.1).abs()) <= MAX_SEARCH)
            .min_by_key(|s| ((s.x - from.0).abs().max((s.y - from.1).abs()), s.id))
            .map(|s| s.id)
    }

    /// What a bazaar's buyer should fetch next, in the original's order: a food it has
    /// none of, a good it has none of, then whatever is furthest below its restock level.
    fn bazaar_wants(&self, id: BuildingId) -> Option<(u16, BuildingId)> {
        let b = self.buildings.get(id)?;
        let foods: Vec<u16> = (resource::GRAIN..=resource::GAMEMEAT).filter(|&r| self.bazaar_source(id, r).is_some() || b.stock[r as usize] > 0).collect();
        let mut wanted: Vec<(u16, i32)> = foods.iter().enumerate().take(4).map(|(i, &r)| (r, PICK_FOOD_BELOW[i])).collect();
        wanted.extend(resource::HOUSE_GOODS.iter().zip(PICK_GOOD_BELOW).map(|(&r, t)| (r, t)));
        wanted.retain(|w| b.bazaar_buys(w.0));
        let have = |r: u16| b.stock[r as usize];
        let with_source = |r: u16| self.bazaar_source(id, r).map(|s| (r, s));
        let empty_food = wanted.iter().filter(|w| resource::is_food(w.0) && have(w.0) == 0).find_map(|w| with_source(w.0));
        if empty_food.is_some() {
            return empty_food;
        }
        let empty_good = wanted.iter().filter(|w| !resource::is_food(w.0) && have(w.0) == 0).find_map(|w| with_source(w.0));
        if empty_good.is_some() {
            return empty_good;
        }
        wanted.iter().filter(|w| have(w.0) < w.1).filter_map(|w| with_source(w.0).map(|s| (have(w.0), s))).min_by_key(|(n, _)| *n).map(|(_, s)| s)
    }

    /// Tick 31: bazaars send a trader when stocked and a buyer when short.
    pub(crate) fn bazaar_walkers(&mut self) {
        let bazaars: Vec<BuildingId> = self.buildings.iter().filter(|b| b.kind == kind::BAZAAR).map(|b| b.id).collect();
        for id in bazaars {
            let Some(b) = self.buildings.get(id) else { continue };
            if b.workers <= 0 || b.road.is_none() {
                continue;
            }
            let stocked = (resource::GRAIN..=resource::GAMEMEAT).chain(resource::HOUSE_GOODS).any(|r| b.stock[r as usize] > 0);
            let (trader, buyer) = (b.walkers[0], b.walkers[2]);
            if trader == 0 && stocked {
                self.spawn_roamer(id, MARKET_TRADER, 0);
            }
            if buyer == 0
                && let Some((r, source)) = self.bazaar_wants(id)
            {
                self.spawn_buyer(id, r, source);
            }
        }
    }

    fn spawn_buyer(&mut self, bazaar: BuildingId, r: u16, source: BuildingId) {
        let Some(road) = self.buildings.get(bazaar).and_then(|b| b.road) else { return };
        let Some(sroad) = self.buildings.get(source).and_then(|s| s.road) else { return };
        let fid = self.figures.spawn(MARKET_BUYER, road.0, road.1, Travel::Roads);
        let map = &self.map;
        if let Some(f) = self.figures.get_mut(fid) {
            f.home = bazaar;
            f.target = source;
            f.cargo = r;
            f.action = action::TO_GRANARY;
            if !f.go_to(map, sroad) {
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
                // Food comes back in up to four loads, goods in up to two.
                let most = if resource::is_food(r) { 4 * LOAD } else { 2 * LOAD };
                let take = self.take_stored(target, r, most);
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

    /// A bazaar trader tops up the goods a house needs now or for its next level.
    pub(crate) fn deliver_goods(&mut self, bazaar: BuildingId, house: BuildingId) {
        let Some(h) = self.buildings.get(house).and_then(|b| b.house.clone()) else { return };
        let level = h.level as usize;
        let next = (level + 1).min(self.balance.houses.len() - 1);
        let (cur, nxt) = (&self.balance.houses[level], &self.balance.houses[next]);
        let needs = [cur.pottery.max(nxt.pottery), cur.jewelry.max(nxt.jewelry), cur.linen.max(nxt.linen), cur.beer.max(nxt.beer)];
        for (slot, &r) in resource::HOUSE_GOODS.iter().enumerate() {
            if needs[slot] <= 0 {
                continue;
            }
            let target = GOODS_PER_TEN * (h.population / 10).max(1) * needs[slot];
            let have = self.buildings.get(house).and_then(|b| b.house.as_ref()).map_or(0, |h| h.goods[slot]);
            let n = (target - have).min(self.buildings.get(bazaar).map_or(0, |m| m.stock[r as usize]));
            if n <= 0 {
                continue;
            }
            if let Some(m) = self.buildings.get_mut(bazaar) {
                m.stock[r as usize] -= n;
            }
            if let Some(h) = self.buildings.get_mut(house).and_then(|b| b.house.as_mut()) {
                h.goods[slot] += n;
            }
        }
    }

    /// Weekly: each house uses up as much of each good as its level requires.
    pub(crate) fn consume_goods(&mut self) {
        let houses = self.balance.houses.clone();
        for b in self.buildings.iter_mut() {
            let Some(h) = b.house.as_mut() else { continue };
            let Some(m) = houses.get(h.level as usize) else { continue };
            for (slot, need) in [m.pottery, m.jewelry, m.linen, m.beer].into_iter().enumerate() {
                h.goods[slot] -= need.max(0).min(h.goods[slot]);
            }
        }
    }

    /// Months the food in granaries and bazaars would last at the current rate of eating.
    pub fn food_supply_months(&self) -> i32 {
        let food: i32 = self
            .buildings
            .iter()
            .filter(|b| matches!(b.kind, kind::GRANARY | kind::BAZAAR))
            .map(|b| (resource::GRAIN..=resource::GAMEMEAT).map(|r| b.stock.get(r as usize).copied().unwrap_or(0)).sum::<i32>())
            .sum();
        // Houses eat about half their people's worth a month (two meals of the weekly share).
        let per_month: i32 = self
            .buildings
            .iter()
            .filter_map(|b| b.house.as_ref())
            .map(|h| h.population * FOOD_CONSUMPTION_PCT[h.level as usize] * (100 - CONSUMPTION_REDUCTION_PCT) / 100 / 100 * 2)
            .sum();
        if per_month > 0 { food / per_month } else { 0 }
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
