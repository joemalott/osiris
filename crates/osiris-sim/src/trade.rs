//! Trade with the cities of the empire.
//!
//! Each trading city lies at the end of a land or sea route. Once the player opens a
//! route, the city sends a trader along it every few days (one at a time); after
//! walking the route on the empire map it arrives at the city's entry point as a
//! caravan with two donkeys. The caravan visits storage yards, each ten ticks buying one
//! load of a good the player exports and selling one load of a good the player
//! imports, up to eight loads each way, until it has nothing left to do or no yard
//! will deal with it. It keeps selling one good until it can't, then moves on to the
//! next in turn (the city remembers where it got to for the next caravan).
//! Each route allows only so much of each good a year (1500, 2500 or 4000 units).

use crate::buildings::{BuildingId, kind};
use crate::economy::LOAD;
use crate::figures::{FigureId, Step, Travel};
use crate::world::World;
use osiris_formats::empire::{self as fmt, city};

pub const TRADE_CARAVAN: u16 = 19;
pub const CARAVAN_DONKEY: u16 = 21;
/// Per-resource tables hold this many slots (index 0 unused).
pub const RESOURCES: usize = fmt::RESOURCES;
/// Days between traders on a land route, and on a sea route.
const LAND_ENTRY_DELAY: i32 = 4;
const SEA_ENTRY_DELAY: i32 = 30;
/// Days a trader rests at each point of its route on the empire map (a random
/// delay in this range is chosen per trip).
const LAND_MOVE_DELAY: (i32, i32) = (1, 4);
/// Ticks between a caravan's deals at a storage yard.
const DEAL_TICKS: i32 = 10;
/// Loads a caravan buys at most, and sells at most.
const CARAVAN_LOADS: i32 = 8;
/// Default prices (buy, sell) per load, used when a scenario has none.
const DEFAULT_PRICES: [(i32, i32); RESOURCES] = [
    (0, 0),
    (28, 21),
    (47, 35),
    (33, 25),
    (33, 25),
    (33, 25),
    (33, 25),
    (42, 33),
    (44, 34),
    (21, 16),
    (325, 275),
    (38, 29),
    (150, 120),
    (140, 105),
    (48, 37),
    (185, 140),
    (54, 42),
    (210, 160),
    (120, 92),
    (310, 150),
    (225, 170),
    (0, 0),
    (31, 23),
    (200, 165),
    (38, 29),
    (46, 35),
    (60, 45),
    (0, 0),
    (375, 315),
    (240, 185),
    (40, 32),
    (110, 85),
    (40, 30),
    (155, 116),
    (250, 190),
    (72, 54),
];

/// The luxury good each empire city sells, by city name id: the group 23 text id of
/// its name (38 jewelry, 40 wine, 42 ivory, 44 ebony, 46 incense, 48 olive oil,
/// 50 leopard skins, 52 perfume).
const LUXURY_KIND: [u8; 66] = [
    38, 38, 40, 38, 38, 38, 38, 42, 38, 44, 38, 46, 48, 50, 46, 38, 38, 38, 44, 38, 38, 46, 38, 44, 42, 38, 48, 40, 38, 38, 38, 40, 38, 38,
    38, 38, 46, 42, 38, 38, 38, 38, 38, 44, 46, 42, 52, 38, 52, 38, 46, 38, 52, 40, 46, 38, 38, 48, 40, 38, 38, 44, 38, 38, 40, 40,
];

/// What the player does with a resource.
pub mod status {
    pub const NONE: u8 = 0;
    /// Buy it from traders while the storage yards hold less than the set amount.
    pub const IMPORT: u8 = 1;
    /// Sell it to traders while the storage yards hold more than the set amount.
    pub const EXPORT: u8 = 2;
    /// Import up to a level the overseer sets from the city's size and industry.
    pub const IMPORT_AS_NEEDED: u8 = 3;
    /// Export what is over the level the overseer sets.
    pub const EXPORT_SURPLUS: u8 = 4;
}

mod action {
    pub const TO_YARD: u16 = 1;
    pub const TRADING: u16 = 2;
    pub const LEAVING: u16 = 3;
    pub const FOLLOWING: u16 = 4;
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TradeCity {
    pub name_id: u8,
    pub city_type: u8,
    pub route: u8,
    pub open: bool,
    pub cost: i32,
    pub sea: bool,
    pub sells: Vec<bool>,
    pub buys: Vec<bool>,
    /// Position on the empire map.
    pub pos: (i32, i32),
    /// Days until the next trader sets out.
    pub entry_delay: i32,
    /// Months left of a siege, which keeps its traders home.
    #[serde(default)]
    pub siege_months: i32,
}

impl TradeCity {
    pub fn trades(&self) -> bool {
        city::trades(self.city_type)
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TradeRoute {
    pub points: Vec<(i32, i32)>,
    pub sea: bool,
    /// Units a year allowed, and traded so far this year, per resource.
    pub limit: Vec<i32>,
    pub traded: Vec<i32>,
}

/// A trader on its way along a route, or trading in the city as a caravan.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct EmpireTrader {
    pub city: usize,
    /// Route points walked so far.
    pub step: usize,
    pub delay: i32,
    pub delay_max: i32,
    pub returning: bool,
    /// The caravan while it is in the city.
    pub figure: FigureId,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Trade {
    pub empire_id: i16,
    pub cities: Vec<TradeCity>,
    /// Indexed by route id.
    pub routes: Vec<TradeRoute>,
    /// (buy, sell) price per load.
    pub prices: Vec<(i32, i32)>,
    /// Per resource: the player's trade status and amount.
    pub status: Vec<u8>,
    pub amount: Vec<i32>,
    /// Resources kept in storage: not used by industry or bazaars, nor traded.
    #[serde(default)]
    pub stockpiled: Vec<bool>,
    /// Resources whose industries are shut down.
    #[serde(default)]
    pub mothballed: Vec<bool>,
    pub traders: Vec<EmpireTrader>,
    /// The good caravans sell us next, round the resources in turn.
    #[serde(default)]
    pub next_import: u16,
    /// Map decorations for the empire window: (kind, x, y, image id).
    pub objects: Vec<(u8, i32, i32, u16)>,
}

impl Trade {
    pub fn from_scenario(s: &osiris_formats::Scenario) -> Self {
        let e = &s.empire;
        let routes = e
            .routes
            .iter()
            .map(|r| TradeRoute {
                points: if r.in_use { r.points.clone() } else { Vec::new() },
                sea: r.route_type == 2,
                limit: vec![0; RESOURCES],
                traded: vec![0; RESOURCES],
            })
            .collect();
        let mut t = Trade {
            empire_id: s.info.empire_id,
            routes,
            prices: if e.prices.iter().any(|&p| p != (0, 0)) { e.prices.clone() } else { DEFAULT_PRICES.to_vec() },
            status: vec![status::NONE; RESOURCES],
            amount: vec![0; RESOURCES],
            stockpiled: vec![false; RESOURCES],
            mothballed: vec![false; RESOURCES],
            ..Default::default()
        };
        for o in e.objects.iter().filter(|o| o.in_use) {
            t.objects.push((o.kind, o.x, o.y, o.image_id));
            if o.kind != fmt::object::CITY {
                continue;
            }
            let trades = city::trades(o.city_type);
            let mut c = TradeCity {
                name_id: o.city_name_id,
                city_type: o.city_type,
                route: o.trade_route_id,
                open: o.trade_route_open,
                cost: o.trade_route_cost as i32,
                sells: vec![false; RESOURCES],
                buys: vec![false; RESOURCES],
                pos: (o.x, o.y),
                entry_delay: LAND_ENTRY_DELAY,
                ..Default::default()
            };
            if let Some(route) = t.routes.get_mut(o.trade_route_id as usize) {
                c.sea = route.sea;
                for r in 1..RESOURCES {
                    if !trades {
                        continue;
                    }
                    c.sells[r] = o.sells.contains(&(r as u8));
                    c.buys[r] = o.buys.contains(&(r as u8));
                    let amount = match o.demand.get(r).copied().unwrap_or(0) {
                        1 => 1500,
                        2 => 2500,
                        3 => 4000,
                        _ => 0,
                    };
                    if c.sells[r] || c.buys[r] {
                        route.limit[r] = amount;
                    }
                }
            }
            t.cities.push(c);
        }
        t
    }
}

impl World {
    /// What the city earns for a load it exports; Ra's great blessing adds half again.
    pub fn sell_price(&self, r: u16) -> i32 {
        let price = self.trade.prices.get(r as usize).map_or(0, |p| p.1);
        if self.religion.ra_trade_boost >= 2 { price * 3 / 2 } else { price }
    }

    pub fn buy_price(&self, r: u16) -> i32 {
        self.trade.prices.get(r as usize).map_or(0, |p| p.0)
    }

    /// Units of `r` in all storage yards.
    pub fn yards_stored(&self, r: u16) -> i32 {
        self.buildings.iter().filter(|b| b.kind == kind::STORAGE_YARD).map(|b| self.stored(b.id, r)).sum()
    }

    /// A route's yearly allowance of `r`, moved up or down a step by Ra's favour.
    pub fn trade_limit(&self, city: usize, r: u16) -> i32 {
        let Some(c) = self.trade.cities.get(city) else { return 0 };
        let base = self.trade.routes.get(c.route as usize).map_or(0, |rt| rt.limit[r as usize]);
        if base <= 0 {
            return 0;
        }
        const TIERS: [i32; 4] = [0, 1500, 2500, 4000];
        let tier = TIERS.iter().position(|&t| t >= base).unwrap_or(3) as i32;
        let bonus = match self.religion.ra_trade_boost {
            b if b >= 2 => 0,
            b => b,
        };
        let t = (tier + bonus).clamp(0, 3) as usize;
        if bonus == 0 { base } else { TIERS[t] }
    }

    fn limit_reached(&self, city: usize, r: u16) -> bool {
        let Some(c) = self.trade.cities.get(city) else { return true };
        self.trade.routes.get(c.route as usize).is_none_or(|rt| rt.traded[r as usize] >= self.trade_limit(city, r))
    }

    pub fn is_stockpiled(&self, r: u16) -> bool {
        self.trade.stockpiled.get(r as usize).copied().unwrap_or(false)
    }

    pub fn is_mothballed(&self, r: u16) -> bool {
        self.trade.mothballed.get(r as usize).copied().unwrap_or(false)
    }

    /// The stock the overseer aims for when importing as needed or exporting surpluses:
    /// food and drink by population, raw materials by the industries using them, and
    /// so on.
    pub fn trade_level(&self, r: u16) -> i32 {
        use crate::economy::resource as res;
        let pop = self.population;
        let active = |k: u16| self.buildings.iter().filter(|b| b.kind == k && b.workers > 0).count() as i32;
        let users = |r: u16| {
            self.buildings.iter().filter(|b| b.workers > 0 && self.defs.building(b.kind).is_some_and(|d| d.inputs.iter().any(|i| self.resource_id(i) == Some(r)))).count() as i32
        };
        match r {
            res::GRAIN | res::MEAT | res::LETTUCE | res::GAMEMEAT | 31 | res::BEER => (pop / 100 * 100).max(100),
            35 | 10 => 10,
            11 | res::STRAW | 14 | 29 | 16 => 200 + 200 * users(r),
            res::TIMBER => active(crate::buildings::kind::SHIPWRIGHT) * 200,
            12 => (pop / 100 * 100).max(100),
            res::POTTERY | res::LUXURY_GOODS => (pop / 100 * 50).max(100),
            23 => (active(51) + active(53)).max(1) * 100,
            _ => 100,
        }
    }

    /// Whether traders from `city` would buy `r` from us now.
    pub fn can_export(&self, city: usize, r: u16) -> bool {
        let Some(c) = self.trade.cities.get(city) else { return false };
        let keep = match self.trade.status[r as usize] {
            status::EXPORT => self.trade.amount[r as usize],
            status::EXPORT_SURPLUS => self.trade_level(r),
            _ => return false,
        };
        c.buys[r as usize] && !self.is_stockpiled(r) && !self.limit_reached(city, r) && self.yards_stored(r) > keep
    }

    /// Whether traders from `city` would sell `r` to us now.
    pub fn can_import(&self, city: usize, r: u16) -> bool {
        let Some(c) = self.trade.cities.get(city) else { return false };
        let want = match self.trade.status[r as usize] {
            status::IMPORT => self.trade.amount[r as usize],
            status::IMPORT_AS_NEEDED => self.trade_level(r),
            _ => return false,
        };
        c.sells[r as usize] && !self.limit_reached(city, r) && self.yards_stored(r) < want
    }

    /// How many kinds of luxury good the city can get, for estates that want a second
    /// one: jewelry if any jeweler stands, plus the kind each open route sells when
    /// luxury goods are imported and the route's allowance is not nil. Cities selling
    /// the same kind (and jewelry from a city) count once.
    pub fn luxury_sources(&self) -> i32 {
        let lux = crate::economy::resource::LUXURY_GOODS;
        let mut kinds = [false; 16];
        kinds[0] = self.buildings.iter().any(|b| b.kind == kind::JEWELER);
        if matches!(self.trade.status.get(lux as usize), Some(&status::IMPORT) | Some(&status::IMPORT_AS_NEEDED)) {
            for (i, c) in self.trade.cities.iter().enumerate() {
                if c.open && c.sells.get(lux as usize).copied().unwrap_or(false) && self.trade_limit(i, lux) > 0 {
                    let k = LUXURY_KIND.get(c.name_id as usize).copied().unwrap_or(38);
                    kinds[(k - 38) as usize] = true;
                }
            }
        }
        kinds.iter().filter(|&&k| k).count() as i32
    }

    /// Whether any trading city sells (or buys) `r`, and whether its route is open.
    pub fn trade_partners(&self, r: u16, buying: bool) -> (bool, bool) {
        let mut any = false;
        let mut open = false;
        for c in self.trade.cities.iter().filter(|c| c.trades()) {
            let list = if buying { &c.sells } else { &c.buys };
            if list.get(r as usize).copied().unwrap_or(false) {
                any = true;
                open |= c.open;
            }
        }
        (any, open)
    }

    /// Clicking a resource's import button: as needed, then to a set amount, then off.
    pub fn cycle_import(&mut self, r: u16) {
        if !self.trade_partners(r, true).1 {
            return;
        }
        let s = &mut self.trade.status[r as usize];
        *s = match *s {
            status::IMPORT_AS_NEEDED => status::IMPORT,
            status::IMPORT => status::NONE,
            _ => status::IMPORT_AS_NEEDED,
        };
    }

    /// Clicking a resource's export button: surpluses, then over a set amount, then off.
    pub fn cycle_export(&mut self, r: u16) {
        if !self.trade_partners(r, false).1 {
            return;
        }
        let s = &mut self.trade.status[r as usize];
        *s = match *s {
            status::EXPORT_SURPLUS => status::EXPORT,
            status::EXPORT => status::NONE,
            _ => status::EXPORT_SURPLUS,
        };
        if *s != status::NONE {
            self.trade.stockpiled[r as usize] = false;
        }
    }

    pub fn change_trade_amount(&mut self, r: u16, delta: i32) {
        if let Some(a) = self.trade.amount.get_mut(r as usize) {
            *a = (*a + delta).clamp(0, 10000);
        }
    }

    pub fn toggle_stockpiled(&mut self, r: u16) {
        if let Some(s) = self.trade.stockpiled.get_mut(r as usize) {
            *s = !*s;
        }
    }

    pub fn toggle_mothballed(&mut self, r: u16) {
        if let Some(s) = self.trade.mothballed.get_mut(r as usize) {
            *s = !*s;
        }
    }

    /// Opens the trade route to `city`, paying its cost.
    pub fn open_trade_route(&mut self, city: usize) -> Result<(), &'static str> {
        let Some(c) = self.trade.cities.get(city) else { return Err("No such city") };
        if !c.trades() || c.open {
            return Err("This city does not trade with you");
        }
        if self.treasury < c.cost {
            return Err("Not enough money");
        }
        let cost = c.cost;
        self.treasury -= cost;
        self.finance.this_year.construction += cost;
        self.trade.cities[city].open = true;
        Ok(())
    }

    pub fn set_trade(&mut self, r: u16, st: u8, amount: i32) {
        if let Some(s) = self.trade.status.get_mut(r as usize) {
            *s = st;
            self.trade.amount[r as usize] = amount.max(0);
        }
    }

    /// Tick 32: traders move along their routes and cities send new ones.
    pub(crate) fn update_trade(&mut self) {
        let mut arrived = Vec::new();
        for (i, t) in self.trade.traders.iter_mut().enumerate() {
            if t.figure != 0 {
                continue;
            }
            if t.delay > 0 {
                t.delay -= 1;
                continue;
            }
            let n = self.trade.cities.get(t.city).and_then(|c| self.trade.routes.get(c.route as usize)).map_or(0, |r| r.points.len());
            t.step += 1;
            t.delay = t.delay_max;
            if t.step + 1 >= n {
                if t.returning {
                    t.step = usize::MAX;
                } else {
                    arrived.push(i);
                }
            }
        }
        self.trade.traders.retain(|t| t.step != usize::MAX);
        for i in arrived {
            let sea = self.trade.cities.get(self.trade.traders[i].city).is_some_and(|c| c.sea);
            if sea {
                self.ship_arrives(i);
            } else {
                self.caravan_arrives(i);
            }
        }
        for city in 0..self.trade.cities.len() {
            let c = &self.trade.cities[city];
            // Ships need a working dock. Ra's wrath, storms, sandstorms and sieges keep
            // traders away.
            let troubled = c.siege_months > 0 || if c.sea { self.scenario_events.sea_problem_days > 0 } else { self.scenario_events.land_problem_days > 0 };
            if !c.open || !c.trades() || (c.sea && !self.sea_trade_open()) || troubled || self.religion.ra_no_traders_months > 0 {
                continue;
            }
            let route = &self.trade.routes[c.route as usize];
            let any = (1..RESOURCES).any(|r| (c.sells[r] || c.buys[r]) && route.limit[r] > 0);
            if !any || route.points.is_empty() || self.trade.traders.iter().any(|t| t.city == city) {
                continue;
            }
            let c = &mut self.trade.cities[city];
            if c.entry_delay > 0 {
                c.entry_delay -= 1;
                continue;
            }
            c.entry_delay = if c.sea { SEA_ENTRY_DELAY } else { LAND_ENTRY_DELAY };
            let delay = LAND_MOVE_DELAY.0 + self.rng.byte() % (LAND_MOVE_DELAY.1 - LAND_MOVE_DELAY.0 + 1);
            self.trade.traders.push(EmpireTrader { city, delay, delay_max: delay, ..Default::default() });
        }
    }

    /// Where a trader is on the empire map, for drawing: coming from the city toward us,
    /// or going back.
    pub fn trader_position(&self, t: &EmpireTrader) -> Option<(i32, i32)> {
        let c = self.trade.cities.get(t.city)?;
        let pts = &self.trade.routes.get(c.route as usize)?.points;
        let n = pts.len();
        if n == 0 || t.figure != 0 {
            return None;
        }
        let i = t.step.min(n - 1);
        Some(if t.returning { pts[i] } else { pts[n - 1 - i] })
    }

    fn caravan_arrives(&mut self, trader: usize) {
        let (x, y) = self.entry_point;
        let fid = self.figures.spawn(TRADE_CARAVAN, x, y, Travel::Land);
        let capacity = CARAVAN_LOADS * LOAD;
        if let Some(f) = self.figures.get_mut(fid) {
            f.target = trader as u32;
            f.roam_left = capacity;
            f.amount = 0;
            f.counter = 0;
            f.action = action::TO_YARD;
        }
        self.trade.traders[trader].figure = fid;
        let mut lead = fid;
        for _ in 0..2 {
            let d = self.figures.spawn(CARAVAN_DONKEY, x, y, Travel::Land);
            if let Some(f) = self.figures.get_mut(d) {
                f.target = lead;
                f.action = action::FOLLOWING;
            }
            lead = d;
        }
        self.caravan_next_yard(fid, None);
    }

    fn trader_city(&self, fid: FigureId) -> Option<usize> {
        let f = self.figures.get(fid)?;
        self.trade.traders.get(f.target as usize).map(|t| t.city)
    }

    /// Picks the storage yard a caravan deals with next, preferring near yards that
    /// hold goods it can buy or have room for goods it sells.
    fn caravan_next_yard(&mut self, fid: FigureId, not: Option<BuildingId>) {
        let Some(city) = self.trader_city(fid) else { return };
        let Some(f) = self.figures.get(fid) else { return };
        let from = (f.x, f.y);
        let exports: Vec<u16> = (1..RESOURCES as u16).filter(|&r| self.can_export(city, r)).collect();
        let imports: Vec<u16> = (1..RESOURCES as u16).filter(|&r| self.can_import(city, r)).collect();
        let best = self
            .buildings
            .iter()
            .filter(|b| b.kind == kind::STORAGE_YARD && b.road.is_some() && Some(b.id) != not)
            .filter_map(|b| {
                let mut penalty = 32;
                penalty -= 4 * exports.iter().filter(|&&r| self.stored(b.id, r) >= LOAD).count() as i32;
                if imports.iter().any(|&r| self.storage_room(b.id, r) >= LOAD) {
                    penalty -= 16;
                }
                (penalty < 32).then(|| ((b.x - from.0).abs().max((b.y - from.1).abs()) + penalty, b.id))
            })
            .min()
            .map(|(_, id)| id);
        let road = best.and_then(|id| self.buildings.get(id)).and_then(|b| b.road);
        let exit = self.exit_point;
        let map = &self.map;
        let Some(f) = self.figures.get_mut(fid) else { return };
        match (best, road) {
            (Some(yard), Some(rd)) if f.go_to(map, rd) => {
                f.home = yard;
                f.action = action::TO_YARD;
            }
            _ => {
                f.action = action::LEAVING;
                if !f.go_to(map, exit) {
                    f.dead = true;
                }
            }
        }
    }

    pub(crate) fn update_caravan(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (act, yard) = (f.action, f.home);
        match act {
            action::TO_YARD | action::LEAVING => {
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                match f.walk(map) {
                    Step::Moving => {}
                    Step::Arrived if act == action::TO_YARD => {
                        f.action = action::TRADING;
                        f.counter = DEAL_TICKS;
                    }
                    _ => self.caravan_gone(fid),
                }
            }
            action::TRADING => {
                let f = self.figures.get_mut(fid).expect("present");
                f.counter -= 1;
                if f.counter > 0 {
                    return;
                }
                f.counter = DEAL_TICKS;
                if self.buildings.get(yard).is_none() || !self.caravan_deal(fid, yard) {
                    self.caravan_next_yard(fid, Some(yard));
                }
            }
            _ => {}
        }
    }

    /// One round of dealing at a yard: buy a load, then sell a load. False if neither
    /// was possible.
    fn caravan_deal(&mut self, fid: FigureId, yard: BuildingId) -> bool {
        let Some(city) = self.trader_city(fid) else { return false };
        let Some(f) = self.figures.get(fid) else { return false };
        // `amount` counts loads bought from us, `cargo` loads sold to us (x100 units).
        let (capacity, bought, sold) = (f.roam_left, f.amount, f.cargo as i32 * LOAD);
        let route = self.trade.cities[city].route as usize;
        let mut dealt = false;
        if bought + LOAD <= capacity
            && let Some(r) = (1..RESOURCES as u16).find(|&r| self.can_export(city, r) && self.stored(yard, r) >= LOAD)
        {
            self.take_stored(yard, r, LOAD);
            let price = self.sell_price(r);
            self.treasury += price;
            self.finance.this_year.exports += price;
            if r == crate::economy::resource::LUXURY_GOODS {
                self.ratings.luxury_exported += LOAD;
            }
            self.trade.routes[route].traded[r as usize] += LOAD;
            self.figures.get_mut(fid).expect("present").amount += LOAD;
            dealt = true;
        }
        let start = self.trade.next_import.max(1) as usize;
        let mut turn = (0..RESOURCES - 1).map(|i| (1 + (start - 1 + i) % (RESOURCES - 1)) as u16);
        if sold + LOAD <= capacity
            && let Some(r) = turn.find(|&r| self.can_import(city, r) && self.storage_room(yard, r) >= LOAD)
        {
            self.trade.next_import = r;
            self.add_stored(yard, r, LOAD);
            let price = self.buy_price(r);
            self.treasury -= price;
            self.finance.this_year.imports += price;
            self.trade.routes[route].traded[r as usize] += LOAD;
            self.figures.get_mut(fid).expect("present").cargo += 1;
            dealt = true;
        }
        dealt
    }

    /// The caravan (or ship) has left the city: its trader heads home along the route.
    pub(crate) fn caravan_gone(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get_mut(fid) else { return };
        f.dead = true;
        let t = f.target as usize;
        if let Some(tr) = self.trade.traders.get_mut(t)
            && tr.figure == fid
        {
            tr.figure = 0;
            tr.returning = true;
            tr.step = 0;
        }
    }

    /// Donkeys walk behind whoever they follow and leave with them.
    pub(crate) fn update_donkey(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let lead = f.target;
        let Some(l) = self.figures.get(lead).filter(|l| !l.dead) else {
            self.figures.get_mut(fid).expect("present").dead = true;
            return;
        };
        let to = (l.x, l.y);
        let map = &self.map;
        let f = self.figures.get_mut(fid).expect("present");
        if !f.moving && (f.x, f.y) != to && f.destination != Some(to) {
            f.go_to(map, to);
        }
        f.walk(map);
    }

    /// Yearly: each route's traded amounts start again.
    pub(crate) fn reset_trade_year(&mut self) {
        for r in &mut self.trade.routes {
            r.traded.iter_mut().for_each(|t| *t = 0);
        }
    }
}
