//! Food: bazaar buyers fetch food from granaries, bazaar traders hand it out to houses
//! along their route, and houses eat it once a month.

use crate::buildings::{BuildingId, kind};
use crate::economy::{LOAD, resource};
use crate::figures::{Step, Travel};
use crate::world::World;

pub const MARKET_TRADER: u16 = 26;
pub const MARKET_BUYER: u16 = 39;

/// How far a bazaar sends its buyers, in tiles.
const MAX_SEARCH: i32 = 40;
/// A bazaar buys a food it has less than this of, and a good below 100.
const BUY_FOOD_BELOW: i32 = 600;
const BUY_GOOD_BELOW: i32 = 100;
/// Its food buyer brings its first food up to 800 and the others up to 600, in at
/// most eight loads; its goods buyer a good up to 400 in at most four.
const FIRST_FOOD_FILL: i32 = 800;
const FOOD_FILL: i32 = 600;
const GOODS_FILL: i32 = 400;
/// The goods buyer's order of preference: beer, pottery, linen, luxury goods.
const BAZAAR_GOODS: [u16; 4] = [resource::BEER, resource::POTTERY, resource::LINEN, resource::LUXURY_GOODS];
/// A trader leaves a house enough of each good for this many residents per ten.
const GOODS_PER_TEN: i32 = 8;

/// Per house level: food each resident may keep (x population).
const FOOD_STORAGE_MULTIPLIER: [i32; 20] = [4, 4, 4, 4, 5, 5, 5, 5, 5, 5, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6];
/// Share of its people a household eats each month, in percent, and with a complex to
/// Bast or Osiris's altar.
const EAT_PCT: i32 = 25;
const EAT_LESS_PCT: i32 = 20;
/// Stops a bazaar buyer makes before heading home.
const BUYER_STOPS: i32 = 3;
/// A fancy bazaar (desirability over 30 at its tile) sends a second trader.
const FANCY_DESIRABILITY: i32 = 30;

mod action {
    pub const TO_GRANARY: u16 = 1;
    pub const HOME: u16 = 2;
}

impl World {
    /// Where a bazaar looks for `r`: the nearest storage building within 40 tiles
    /// holding some, unless the good is stockpiled or the bazaar doesn't buy it. Food
    /// comes from granaries, and from storage yards only while it is being imported.
    fn bazaar_source(&self, bazaar: BuildingId, r: u16) -> Option<BuildingId> {
        use crate::trade::status;
        let b = self.buildings.get(bazaar)?;
        if self.is_stockpiled(r) || !b.bazaar_buys(r) {
            return None;
        }
        let imported = matches!(self.trade.status.get(r as usize), Some(&status::IMPORT | &status::IMPORT_AS_NEEDED));
        let yard_ok = !resource::is_food(r) || imported;
        let from = (b.x, b.y);
        self.buildings
            .iter()
            .filter(|s| crate::storage::is_storage(s.kind) && s.road.is_some() && self.stored(s.id, r) > 0)
            .filter(|s| s.kind != kind::STORAGE_YARD || yard_ok)
            .filter(|s| (s.x - from.0).abs().max((s.y - from.1).abs()) < MAX_SEARCH)
            .min_by_key(|s| ((s.x - from.0).abs().max((s.y - from.1).abs()), s.id))
            .map(|s| s.id)
    }

    /// The foods a bazaar deals in: up to four the city has a source of or it holds.
    fn bazaar_foods(&self, id: BuildingId) -> Vec<u16> {
        let Some(b) = self.buildings.get(id) else { return vec![] };
        (resource::GRAIN..=resource::GAMEMEAT).filter(|&r| self.bazaar_source(id, r).is_some() || b.stock[r as usize] > 0).take(4).collect()
    }

    /// What a bazaar's food buyer goes for, and where: a food it has none of, else the
    /// scarcest below 50, else one below 600. Its goods buyer: a good it has none of,
    /// else the scarcest below 100.
    fn bazaar_wants(&self, id: BuildingId, food: bool) -> Option<(u16, BuildingId)> {
        let b = self.buildings.get(id)?;
        let have = |r: u16| b.stock[r as usize];
        let with_source = |r: u16| self.bazaar_source(id, r).map(|s| (r, s));
        let list: Vec<u16> = if food { self.bazaar_foods(id) } else { BAZAAR_GOODS.to_vec() };
        if let Some(w) = list.iter().filter(|&&r| have(r) == 0).find_map(|&r| with_source(r)) {
            return Some(w);
        }
        let scarce = if food { 50 } else { BUY_GOOD_BELOW };
        let lowest = list.iter().filter(|&&r| have(r) < scarce).filter_map(|&r| with_source(r).map(|w| (have(r), w))).min_by_key(|(n, _)| *n).map(|(_, w)| w);
        if lowest.is_some() || !food {
            return lowest;
        }
        list.iter().rev().filter(|&&r| have(r) < BUY_FOOD_BELOW).find_map(|&r| with_source(r))
    }

    /// Tick 31: a bazaar with stock sends its trader after a wait that grows as its
    /// staff shrinks (three days at full staff). A bazaar with desirability over 30 at
    /// its tile looks fancy and sends a second trader too, with no wait. One day in
    /// eight, its food buyer and its goods buyer go shopping if not already out.
    pub(crate) fn bazaar_walkers(&mut self) {
        let bazaars: Vec<BuildingId> = self.buildings.iter().filter(|b| b.kind == kind::BAZAAR).map(|b| b.id).collect();
        let needed = self.workers_needed(kind::BAZAAR).max(1);
        let images = self.defs.building(kind::BAZAAR).map(|d| (d.image, d.anims.get("fancy").map_or(d.image, |a| a.image)));
        for (n, id) in bazaars.into_iter().enumerate() {
            let Some(b) = self.buildings.get(id) else { continue };
            let fancy = self.desirability.at_or(b.x, b.y, 0) as i32 > FANCY_DESIRABILITY;
            if let Some((plain, fancy_image)) = images {
                let image = if fancy { fancy_image } else { plain };
                if b.image != image {
                    self.set_building_image(id, image);
                }
            }
            let Some(b) = self.buildings.get(id) else { continue };
            if b.workers <= 0 || b.road.is_none() {
                continue;
            }
            let wait = match b.workers * 100 / needed {
                p if p >= 100 => 2,
                p if p >= 75 => 3,
                p if p >= 50 => 4,
                p if p >= 25 => 5,
                _ => 10,
            };
            let stocked = (resource::GRAIN..=resource::GAMEMEAT).chain(resource::HOUSE_GOODS).any(|r| b.stock[r as usize] > 0);
            let main = b.walkers[0];
            if fancy && stocked && !self.figures.iter().any(|f| f.kind == MARKET_TRADER && f.home == id && f.id != main && !f.dead) {
                self.spawn_roaming_figure(id, MARKET_TRADER);
            }
            let Some(b) = self.buildings.get(id) else { continue };
            if b.walkers[0] == 0 && stocked {
                let waited = b.spawn_delay + 1;
                let b = self.buildings.get_mut(id).expect("present");
                if waited > wait {
                    b.spawn_delay = 0;
                    self.spawn_roamer(id, MARKET_TRADER, 0);
                } else {
                    b.spawn_delay = waited;
                }
            }
            if self.time.day % 8 != (n as u32 + 1) % 8 {
                continue;
            }
            for food in [true, false] {
                let out = self.figures.iter().any(|f| f.kind == MARKET_BUYER && f.home == id && !f.dead && resource::is_food(f.cargo) == food);
                if !out && let Some((r, source)) = self.bazaar_wants(id, food) {
                    let list = self.buyer_list(id, food);
                    self.spawn_buyer(id, r, source, list);
                }
            }
        }
    }

    /// What a buyer shops for: up to four foods (or goods), first those the bazaar has
    /// none of, then those it is short of (under 600 of a food, 100 of a good).
    fn buyer_list(&self, id: BuildingId, food: bool) -> Vec<u16> {
        let Some(b) = self.buildings.get(id) else { return vec![] };
        let list: Vec<u16> = if food { self.bazaar_foods(id) } else { BAZAAR_GOODS.to_vec() };
        let below = if food { BUY_FOOD_BELOW } else { BUY_GOOD_BELOW };
        let has_source = |r: u16| self.bazaar_source(id, r).is_some();
        let empty = list.iter().copied().filter(|&r| b.stock[r as usize] == 0 && has_source(r));
        let short = list.iter().copied().filter(|&r| b.stock[r as usize] > 0 && b.stock[r as usize] < below && has_source(r));
        empty.chain(short).take(4).collect()
    }

    fn spawn_buyer(&mut self, bazaar: BuildingId, r: u16, source: BuildingId, list: Vec<u16>) {
        let Some(road) = self.buildings.get(bazaar).and_then(|b| b.road) else { return };
        let Some(sroad) = self.buildings.get(source).and_then(|s| s.road) else { return };
        let fid = self.figures.spawn(MARKET_BUYER, road.0, road.1, Travel::Roads);
        let map = &self.map;
        if let Some(f) = self.figures.get_mut(fid) {
            f.home = bazaar;
            f.target = source;
            f.cargo = r;
            f.carried = list.into_iter().map(|r| (r, 0)).collect();
            if !f.carried.iter().any(|c| c.0 == r) {
                f.carried.insert(0, (r, 0));
            }
            f.action = action::TO_GRANARY;
            if !f.go_to(map, sroad) {
                f.dead = true;
            }
        }
    }

    /// How far a bazaar fills `r`, and the most loads a buyer takes from one place:
    /// 800 of its first food and 600 of the others in up to eight loads, 400 of a good
    /// in up to four.
    fn buyer_fill(&self, bazaar: BuildingId, r: u16) -> (i32, i32) {
        if !resource::is_food(r) {
            (GOODS_FILL, 4)
        } else if self.bazaar_foods(bazaar).first() == Some(&r) {
            (FIRST_FOOD_FILL, 8)
        } else {
            (FOOD_FILL, 8)
        }
    }

    /// What a buyer already carrying `carried` of `r` takes at `source`: whole loads (a
    /// part load counts as one) until the bazaar would reach its fill level.
    fn buyer_take(&self, bazaar: BuildingId, source: BuildingId, r: u16, carried: i32) -> i32 {
        let have = self.buildings.get(bazaar).map_or(0, |b| b.stock[r as usize]);
        let (fill, most) = self.buyer_fill(bazaar, r);
        let room = fill - have - carried;
        if room <= 0 {
            return 0;
        }
        let stored = self.stored(source, r);
        let there = if stored >= LOAD { (stored / LOAD).min(most) } else { i32::from(stored > 0) };
        there.min(room / LOAD + 1) * LOAD
    }

    /// A buyer at a granary or storage yard takes what it can of everything on its
    /// list. After fewer than three stops it goes on to the source of whatever it is
    /// still shortest of; otherwise, or with nothing left to buy, it goes home.
    pub(crate) fn update_buyer(&mut self, fid: u32) {
        let map = &self.map;
        let Some(f) = self.figures.get_mut(fid) else { return };
        let step = f.walk(map);
        let (act, home, target) = (f.action, f.home, f.target);
        match (act, step) {
            (_, Step::Moving) => {}
            (action::TO_GRANARY, Step::Arrived) => {
                let mut carried = std::mem::take(&mut f.carried);
                f.counter += 1;
                let stops = f.counter;
                for c in carried.iter_mut() {
                    let want = self.buyer_take(home, target, c.0, c.1);
                    c.1 += self.take_stored(target, c.0, want);
                }
                let next = if stops < BUYER_STOPS {
                    carried
                        .iter()
                        .filter_map(|&(r, n)| {
                            let have = self.buildings.get(home).map_or(0, |b| b.stock[r as usize]);
                            let need = self.buyer_fill(home, r).0 - have - n;
                            let src = self.bazaar_source(home, r).filter(|&s| s != target)?;
                            (need > 0).then_some((need, std::cmp::Reverse(r), src))
                        })
                        .max()
                        .map(|t| t.2)
                } else {
                    None
                };
                let to = match next {
                    Some(src) => self.buildings.get(src).and_then(|b| b.road).map(|r| (src, r)),
                    None => self.buildings.get(home).and_then(|b| b.road).map(|r| (home, r)),
                };
                let map = &self.map;
                let Some(f) = self.figures.get_mut(fid) else { return };
                f.carried = carried;
                f.amount = f.carried.iter().map(|c| c.1).sum();
                f.action = if next.is_some() { action::TO_GRANARY } else { action::HOME };
                match to {
                    Some((t, road)) if f.go_to(map, road) => f.target = t,
                    _ => f.dead = true,
                }
            }
            (action::HOME, Step::Arrived) => {
                let carried = std::mem::take(&mut f.carried);
                f.dead = true;
                if let Some(b) = self.buildings.get_mut(home) {
                    for (r, n) in carried {
                        b.stock[r as usize] += n;
                    }
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

    /// At the start and the middle of each month every house uses up a quarter of each
    /// good its level needs (four times the need for a house over one tile). A need
    /// that doesn't split into quarters leaves a remainder of one, two or three, of
    /// which one unit is used: at the start of the month for one or two, mid-month for
    /// three.
    pub(crate) fn consume_goods(&mut self) {
        let start = self.time.day != crate::time::DAYS_PER_MONTH / 2;
        let remainder = |r: i32| match r {
            1 | 2 => start as i32,
            3 => !start as i32,
            _ => 0,
        };
        let houses = self.balance.houses.clone();
        for b in self.buildings.iter_mut() {
            let big = b.size > 1;
            let Some(h) = b.house.as_mut() else { continue };
            let Some(m) = houses.get(h.level as usize) else { continue };
            for (slot, need) in [m.pottery, m.jewelry, m.linen, m.beer].into_iter().enumerate() {
                let need = need.max(0) * if big { 4 } else { 1 };
                let used = if need % 4 == 0 { need / 4 } else { remainder(need % 4) };
                h.goods[slot] -= used.min(h.goods[slot]);
            }
        }
    }

    /// Months the food in granaries at least half staffed would last the city at the
    /// monthly rate of eating (1 if there is food but nobody to eat it).
    pub fn food_supply_months(&self) -> i32 {
        let food: i32 = self
            .buildings
            .iter()
            .filter(|b| b.kind == kind::GRANARY && b.road.is_some() && b.workers * 2 >= self.workers_needed(b.kind).max(1))
            .map(|b| (resource::GRAIN..=resource::GAMEMEAT).map(|r| b.stock.get(r as usize).copied().unwrap_or(0)).sum::<i32>())
            .sum();
        let per_month = self.population * self.eat_pct() / 100;
        if per_month > 0 { food / per_month } else { i32::from(food > 0) }
    }

    fn eat_pct(&self) -> i32 {
        if self.eats_less() { EAT_LESS_PCT } else { EAT_PCT }
    }

    /// The foods the city knows of, in resource order: those it can grow or catch,
    /// those a trading city sells, and any it holds.
    fn city_foods(&self) -> Vec<u16> {
        (resource::GRAIN..=resource::GAMEMEAT)
            .filter(|&r| {
                let key = self.defs.resources.get(r as usize);
                let made = self.defs.buildings.iter().flatten().any(|d| d.outputs.first() == key && self.is_allowed(d.id));
                let sold = self.trade.cities.iter().any(|c| c.trades() && c.sells.get(r as usize).copied().unwrap_or(false));
                let held = self.buildings.iter().any(|b| matches!(b.kind, kind::GRANARY | kind::BAZAAR) && b.stock.get(r as usize).is_some_and(|&n| n > 0));
                made || sold || held
            })
            .collect()
    }

    /// Monthly, just after the city's health: each household eats a quarter of its
    /// people's worth (a fifth with a complex to Bast or Osiris's altar), split evenly
    /// over the food types its level needs, taken from the city's foods in turn. A food
    /// it has less of than its share is used up; either way it counts as eaten. It
    /// stops once it has eaten as many types as its level needs.
    pub(crate) fn consume_food(&mut self) {
        let houses = self.balance.houses.clone();
        let pct = self.eat_pct();
        let mut slots: Vec<usize> = Vec::new();
        for r in self.city_foods() {
            if let Some(s) = resource::food_slot(r)
                && !slots.contains(&s)
            {
                slots.push(s);
            }
        }
        for b in self.buildings.iter_mut() {
            let Some(h) = b.house.as_mut() else { continue };
            let types = houses.get(h.level as usize).map_or(0, |m| m.food_types);
            h.foods_eaten = 0;
            if types <= 0 {
                continue;
            }
            let mut share = h.population * pct / 100;
            if types > 1 {
                share /= types;
            }
            for &s in &slots {
                let f = &mut h.foods[s];
                if *f == 0 && share > 0 {
                    continue;
                }
                *f = (*f - share).max(0);
                h.foods_eaten += 1;
                if h.foods_eaten >= types {
                    break;
                }
            }
        }
    }
}
