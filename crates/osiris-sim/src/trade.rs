//! Trade with the cities of the empire.
//!
//! Each trading city lies at the end of a land or sea route. Once the player opens a
//! route, the city sends a trader every few days (up to three at a time). There is no
//! journey along the route: the trader appears at once at the city's entry point as a
//! caravan with two donkeys (or at the river entry as a ship). The caravan stands 20
//! ticks at the entry point, then visits staffed storage yards that can be walked to
//! from there, each eleven ticks buying one load of a good the player exports and
//! selling one load of a good the player imports, up to eight loads each way, until
//! it has nothing left to do or no yard will deal with it, then leaves by the exit.
//! It sells the goods in turn, a load of each (the rotation is shared by every
//! caravan).
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
/// Days between traders on a land route, and on a sea route: the countdown runs from
/// here to 0, so a trader every 5th or 31st day.
const LAND_ENTRY_DELAY: i32 = 4;
const SEA_ENTRY_DELAY: i32 = 30;
/// Most traders a city has in ours at once.
pub const MAX_TRADERS: usize = 3;
/// City-days between reminders that ships cannot come without a dock.
const NO_DOCK_REMINDER: i32 = 384;
/// Ticks a new caravan stands at the entry point before it chooses a yard.
const CREATED_WAIT: i32 = 20;
/// Ticks between a caravan's deals at a storage yard.
const DEAL_TICKS: i32 = 11;
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
    /// Standing at the entry point before choosing a yard.
    pub const CREATED: u16 = 5;
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TradeCity {
    pub name_id: u8,
    pub city_type: u8,
    pub route: u8,
    pub open: bool,
    pub cost: i32,
    pub sea: bool,
    /// What it sells and buys while it trades (or is ours); nothing otherwise.
    pub sells: Vec<bool>,
    pub buys: Vec<bool>,
    /// The goods the scenario lists for it, sold and bought, which it trades once it
    /// is a trading city.
    #[serde(default)]
    pub listed_sells: Vec<bool>,
    #[serde(default)]
    pub listed_buys: Vec<bool>,
    /// Position on the empire map.
    pub pos: (i32, i32),
    /// Where its name sits beside its icon: 0 left, 1 above, 2 right, 3 below.
    #[serde(default)]
    pub text_align: u8,
    /// Days until the next trader sets out.
    pub entry_delay: i32,
    /// Its caravans or ships in the city, by slot (0 for a free slot).
    #[serde(default)]
    pub traders: [FigureId; MAX_TRADERS],
}

impl TradeCity {
    pub fn trades(&self) -> bool {
        city::trades(self.city_type)
    }

    /// Takes up its listed goods if it trades (or is ours), and drops them if not.
    /// Older saved games kept no lists; their cities keep what they had.
    fn refresh_goods(&mut self) {
        if self.listed_sells.is_empty() {
            return;
        }
        let keep = self.trades() || self.city_type == city::OURS;
        self.sells = self.listed_sells.iter().map(|&l| l && keep).collect();
        self.buys = self.listed_buys.iter().map(|&l| l && keep).collect();
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TradeRoute {
    pub points: Vec<(i32, i32)>,
    /// Pixels between the dots of the route on the empire map.
    #[serde(default)]
    pub step: u8,
    pub sea: bool,
    /// Units a year allowed, and traded so far this year, per resource.
    pub limit: Vec<i32>,
    pub traded: Vec<i32>,
}

/// A trader as older saved games kept it, walking its route on the empire map.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct EmpireTrader {
    pub city: usize,
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
    /// Older saved games' traders, whose figures pointed into this list; emptied on
    /// load.
    #[serde(default, skip_serializing)]
    pub traders: Vec<EmpireTrader>,
    /// City-days until the next reminder that ships need a dock.
    #[serde(default)]
    pub no_dock_reminder: i32,
    /// The good caravans sell us next, round the resources in turn.
    #[serde(default)]
    pub next_import: u16,
    /// The goods dockers unload from ships, and fetch for them, in turn.
    #[serde(default)]
    pub docker_import: u16,
    #[serde(default)]
    pub docker_export: u16,
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
                step: if (1..=50).contains(&r.step) { r.step } else { 5 },
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
            let mut c = TradeCity {
                name_id: o.city_name_id,
                city_type: o.city_type,
                route: o.trade_route_id,
                open: o.trade_route_open,
                cost: o.trade_route_cost as i32,
                listed_sells: (0..RESOURCES).map(|r| r > 0 && o.sells.contains(&(r as u8))).collect(),
                listed_buys: (0..RESOURCES).map(|r| r > 0 && o.buys.contains(&(r as u8))).collect(),
                pos: (o.x, o.y),
                text_align: o.text_align,
                entry_delay: LAND_ENTRY_DELAY,
                ..Default::default()
            };
            c.refresh_goods();
            // Every city's route gets its yearly allowances, trading or not, so a city
            // that becomes a trade partner later brings its goods with it.
            if let Some(route) = t.routes.get_mut(o.trade_route_id as usize) {
                c.sea = route.sea;
                for r in 1..RESOURCES {
                    route.limit[r] = match o.demand.get(r).copied().unwrap_or(0) {
                        1 => 1500,
                        2 => 2500,
                        3 => 4000,
                        _ => 0,
                    };
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
        if self.religion.ra_export_months > 0 { price * 150 / 100 } else { price }
    }

    pub fn buy_price(&self, r: u16) -> i32 {
        self.trade.prices.get(r as usize).map_or(0, |p| p.0)
    }

    /// Units of `r` in all storage yards.
    pub fn yards_stored(&self, r: u16) -> i32 {
        self.buildings.iter().filter(|b| b.kind == kind::STORAGE_YARD).map(|b| self.stored(b.id, r)).sum()
    }

    /// A route's yearly allowance of `r`, moved by Ra's blessings and curses.
    pub fn trade_limit(&self, city: usize, r: u16) -> i32 {
        let Some(c) = self.trade.cities.get(city) else { return 0 };
        let base = self.trade.routes.get(c.route as usize).map_or(0, |rt| rt.limit[r as usize]);
        if base <= 0 {
            return 0;
        }
        crate::religion::ra_allowance(self.religion.ra_trade_steps(), base)
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
        // Nothing comes in while the treasury is 5000 or more in the red.
        c.sells[r as usize] && !self.out_of_money() && !self.limit_reached(city, r) && self.yards_stored(r) < want
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

    /// Opens the trade route to `city`, paying its cost as construction. As in the
    /// original, only a treasury 5000 or more in debt stops it; otherwise the cost is
    /// paid even into debt.
    pub fn open_trade_route(&mut self, city: usize) -> Result<(), &'static str> {
        let Some(c) = self.trade.cities.get(city) else { return Err("No such city") };
        if !c.trades() || c.open {
            return Err("This city does not trade with you");
        }
        if self.out_of_money() {
            return Err("You do not have enough debens to open a trade route.");
        }
        let cost = c.cost;
        self.treasury -= cost;
        self.finance.this_year.construction += cost;
        self.trade.cities[city].open = true;
        Ok(())
    }

    /// A scenario event makes `city` a city of kind `city_type`: it trades its listed
    /// goods if the new kind trades and none otherwise, and `close` shuts its route.
    /// Imports and exports no open route still carries are then stopped.
    pub fn change_trade_city(&mut self, city: usize, city_type: u8, close: bool) {
        let Some(c) = self.trade.cities.get_mut(city) else { return };
        c.city_type = city_type;
        if close {
            c.open = false;
        }
        c.refresh_goods();
        for r in 1..RESOURCES as u16 {
            let st = self.trade.status[r as usize];
            let buying = matches!(st, status::IMPORT | status::IMPORT_AS_NEEDED);
            if st == status::NONE || self.open_partner(r, buying) {
                continue;
            }
            self.trade.status[r as usize] = status::NONE;
        }
    }

    /// Whether an open trading city sells (or buys) `r` and its route allows some.
    fn open_partner(&self, r: u16, buying: bool) -> bool {
        self.trade.cities.iter().enumerate().any(|(i, c)| {
            let list = if buying { &c.sells } else { &c.buys };
            c.trades() && c.open && list.get(r as usize).copied().unwrap_or(false) && self.trade_limit(i, r) > 0
        })
    }

    pub fn set_trade(&mut self, r: u16, st: u8, amount: i32) {
        if let Some(s) = self.trade.status.get_mut(r as usize) {
            *s = st;
            self.trade.amount[r as usize] = amount.max(0);
        }
    }

    /// Tick 32: each open city counts down to its next trader, who appears at once in
    /// our city: every fifth day by land, every 31st by sea. A city may have as many
    /// traders in the city as the mean tier of its goods' allowances (1-3), and its
    /// countdown waits while they are all here. Ships wait (countdown and all) until
    /// the city has a dock, of any staffing, and a river entry. The count starts again
    /// even when trouble keeps the trader home. At most one trader enters a day.
    pub(crate) fn update_trade(&mut self) {
        let has_dock = self.buildings.iter().any(|b| b.kind == crate::water::DOCK);
        let river = self.river_entry().is_some();
        for city in 0..self.trade.cities.len() {
            let c = &self.trade.cities[city];
            if !c.open || !c.trades() {
                continue;
            }
            if c.sea && !has_dock {
                if self.trade.no_dock_reminder > 0 {
                    self.trade.no_dock_reminder -= 1;
                } else {
                    self.trade.no_dock_reminder = NO_DOCK_REMINDER;
                    self.post("message_no_working_dock", None, true);
                }
                continue;
            }
            if c.sea && !river {
                continue;
            }
            // Tiers come from the route's own allowances; Ra's favour only decides
            // which goods count.
            let route = self.trade.routes.get(c.route as usize);
            let (mut goods, mut tiers) = (0, 0);
            for r in (1..RESOURCES).filter(|&r| c.sells[r] || c.buys[r]) {
                if self.trade_limit(city, r as u16) <= 0 {
                    continue;
                }
                goods += 1;
                tiers += match route.map_or(0, |rt| rt.limit[r]) {
                    l if l > 2500 => 3,
                    l if l > 1500 => 2,
                    l if l > 0 => 1,
                    _ => 0,
                };
            }
            let slots = if goods > 1 { (tiers + goods - 1) / goods } else { tiers }.min(MAX_TRADERS as i32);
            if slots <= 0 {
                continue;
            }
            let Some(slot) = (0..slots as usize).find(|&s| !self.trader_in_city(city, s)) else { continue };
            // Ra's wrath, storms and sandstorms keep traders away.
            let troubled = if c.sea { self.scenario_events.sea_problem_days > 0 } else { self.scenario_events.land_problem_days > 0 };
            let blocked = troubled || self.religion.ra_no_traders_months > 0;
            let c = &mut self.trade.cities[city];
            if c.entry_delay > 0 {
                c.entry_delay -= 1;
                continue;
            }
            c.entry_delay = if c.sea { SEA_ENTRY_DELAY } else { LAND_ENTRY_DELAY };
            if blocked {
                continue;
            }
            let fid = if c.sea { self.ship_arrives(city) } else { self.caravan_arrives(city) };
            self.trade.cities[city].traders[slot] = fid;
            break;
        }
    }

    /// Whether slot `slot` of `city` holds one of its caravans or ships.
    fn trader_in_city(&self, city: usize, slot: usize) -> bool {
        let id = self.trade.cities[city].traders[slot];
        id != 0
            && self
                .figures
                .get(id)
                .is_some_and(|f| !f.dead && matches!(f.kind, TRADE_CARAVAN | crate::docks::TRADE_SHIP) && f.target as usize == city)
    }

    /// Older saved games' caravans and ships pointed at a list of traders; point them
    /// at their cities instead.
    pub(crate) fn upgrade_traders(&mut self) {
        let old = std::mem::take(&mut self.trade.traders);
        if old.is_empty() {
            return;
        }
        let ids: Vec<FigureId> = self.figures.iter().filter(|f| matches!(f.kind, TRADE_CARAVAN | crate::docks::TRADE_SHIP)).map(|f| f.id).collect();
        for id in ids {
            let f = self.figures.get_mut(id).expect("present");
            match old.get(f.target as usize) {
                Some(t) if t.figure == id => {
                    f.target = t.city as u32;
                    if let Some(c) = self.trade.cities.get_mut(t.city)
                        && let Some(s) = c.traders.iter_mut().find(|s| **s == 0)
                    {
                        *s = id;
                    }
                }
                _ => f.dead = true,
            }
        }
    }

    /// A caravan from `city` appears at the entry point with its two donkeys, and
    /// stands there 20 ticks before it picks a storage yard.
    fn caravan_arrives(&mut self, city: usize) -> FigureId {
        let (x, y) = self.entry_point;
        let fid = self.figures.spawn(TRADE_CARAVAN, x, y, Travel::PreferRoads);
        let capacity = CARAVAN_LOADS * LOAD;
        if let Some(f) = self.figures.get_mut(fid) {
            f.target = city as u32;
            f.roam_left = capacity;
            f.amount = 0;
            f.counter = 0;
            f.action = action::CREATED;
        }
        let mut lead = fid;
        for _ in 0..2 {
            let d = self.figures.spawn(CARAVAN_DONKEY, x, y, Travel::Land);
            if let Some(f) = self.figures.get_mut(d) {
                f.target = lead;
                f.action = action::FOLLOWING;
            }
            lead = d;
        }
        fid
    }

    /// The city a caravan or ship comes from.
    pub(crate) fn trader_city(&self, fid: FigureId) -> Option<usize> {
        let f = self.figures.get(fid)?;
        let city = f.target as usize;
        (city < self.trade.cities.len()).then_some(city)
    }

    /// Steps the rotation of goods caravans sell us on to the next one, 35 after 1.
    fn step_import_turn(&mut self) -> u16 {
        let next = self.trade.next_import.max(1) + 1;
        self.trade.next_import = if next as usize >= RESOURCES { 1 } else { next };
        self.trade.next_import
    }

    /// Whether storage yard `yard` would take a load a caravan from `city`, having
    /// sold us `sold` loads, has to sell: it has one of the goods we import from the
    /// city on its accept or get orders and short of their limits, the good the
    /// rotation stands at (moved on to the next such good the yard doesn't refuse, if
    /// need be) is one of them, and one of its spaces has room for a load, empty or
    /// holding a good we import from the city.
    fn yard_takes_sale(&mut self, yard: BuildingId, city: usize, sold: i32) -> bool {
        use crate::storage::order;
        let Some(b) = self.buildings.get(yard).filter(|b| b.kind == kind::STORAGE_YARD) else { return false };
        if sold >= CARAVAN_LOADS {
            return false;
        }
        let full = |r: u16, more: i32| b.order_cap(r) < self.stored(yard, r) + more;
        let wanted = (1..RESOURCES as u16).any(|r| !matches!(b.order(r), order::REFUSE | order::EMPTY) && !full(r, 0) && !full(r, LOAD) && self.can_import(city, r));
        if !wanted {
            return false;
        }
        let takes = |w: &World, r: u16| w.buildings.get(yard).is_some_and(|b| b.order(r) != order::REFUSE) && w.can_import(city, r);
        if !takes(self, self.trade.next_import.max(1)) && !(1..RESOURCES).any(|_| {
            let r = self.step_import_turn();
            takes(self, r)
        }) {
            return false;
        }
        let Some(b) = self.buildings.get(yard) else { return false };
        b.spaces.iter().any(|&(r, n)| n < crate::storage::SPACE_UNITS && (n == 0 || self.can_import(city, r)))
    }

    /// Whether a caravan from `city`, having bought `bought` units, could buy a load at
    /// yard `yard`: a space holds a good we export to the city and the yard holds a
    /// load of it.
    fn yard_has_sale(&self, yard: BuildingId, city: usize, bought: i32) -> bool {
        let Some(b) = self.buildings.get(yard).filter(|b| b.kind == kind::STORAGE_YARD) else { return false };
        bought < CARAVAN_LOADS * LOAD && b.spaces.iter().any(|&(r, n)| n > 0 && self.can_export(city, r) && self.stored(yard, r) >= LOAD)
    }

    /// Picks the storage yard a caravan deals with next. A yard qualifies when it is
    /// staffed, reached from the entry point, and has something to buy from the
    /// caravan or sell it. Of those, the caravan takes the one nearest after a penalty
    /// of 32, less 4 for each space holding a good it would buy and, while it has
    /// goods to sell that the yard would take, 16 for each empty space and 8 for each
    /// space under 400 of the good the rotation stands at (which each space moves on
    /// to the next good the yard doesn't refuse). Yards left at 32 or more are passed
    /// over, and ties go to the older yard. A caravan choosing at the entry point
    /// (`from_entry`) adds each yard's walking distance from there.
    fn caravan_next_yard(&mut self, fid: FigureId, not: Option<BuildingId>, from_entry: bool) {
        use crate::storage::order;
        let Some(city) = self.trader_city(fid) else { return };
        let Some(f) = self.figures.get(fid) else { return };
        let (from, bought, sold) = ((f.x, f.y), f.amount, f.cargo as i32);
        let exportable: Vec<bool> = (0..RESOURCES as u16).map(|r| r > 0 && bought < CARAVAN_LOADS * LOAD && self.can_export(city, r)).collect();
        let importable: Vec<bool> = (0..RESOURCES as u16).map(|r| r > 0 && sold < CARAVAN_LOADS && self.can_import(city, r)).collect();
        let any_import = importable.iter().any(|&i| i);
        let yards = self.yards_from_entry(not);
        let mut best: Option<(i32, BuildingId)> = None;
        for (id, entry_distance) in yards {
            if !self.yard_takes_sale(id, city, sold) && !self.yard_has_sale(id, city, bought) {
                continue;
            }
            let b = self.buildings.get(id).expect("yard");
            let (x, y, empty_all, spaces) = (b.x, b.y, b.empty_all, b.spaces.clone());
            let accepts_import = (1..RESOURCES as u16).any(|r| !matches!(b.order(r), order::REFUSE | order::EMPTY) && self.can_import(city, r));
            let refuses = |w: &World, r: u16| w.buildings.get(id).is_some_and(|b| b.order(r) == order::REFUSE);
            let mut penalty = 32;
            for &(r, n) in &spaces {
                if n > 0 && exportable[r as usize] {
                    penalty -= 4;
                }
                if any_import && accepts_import && !empty_all {
                    let mut turn = self.step_import_turn();
                    for _ in 2..RESOURCES {
                        if !refuses(self, turn) {
                            break;
                        }
                        turn = self.step_import_turn();
                    }
                    if !refuses(self, turn) {
                        if n == 0 {
                            penalty -= 16;
                        } else if r == turn && importable[r as usize] && n < crate::storage::SPACE_UNITS {
                            penalty -= 8;
                        }
                    }
                }
            }
            if penalty < 32 {
                let d = (x - from.0).abs().max((y - from.1).abs()) + if from_entry { entry_distance } else { 0 } + penalty;
                if best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, id));
                }
            }
        }
        let best = best.map(|(_, id)| id);
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

    /// Staffed storage yards (but `not`) whose road tile can be walked to from the
    /// entry point, with that walking distance, oldest first.
    pub(crate) fn yards_from_entry(&self, not: Option<BuildingId>) -> Vec<(BuildingId, i32)> {
        let entry = crate::figures::route_distances(&self.map, Travel::Land, self.entry_point);
        let w = self.map.width;
        let at = |(x, y): (i32, i32)| if self.map.contains(x, y) { entry[(y * w + x) as usize] } else { 0 };
        self.buildings
            .iter()
            .filter(|b| b.kind == kind::STORAGE_YARD && b.workers > 0 && Some(b.id) != not)
            .filter_map(|b| b.road.map(|r| (b.id, at(r))))
            .filter(|&(_, d)| d > 0)
            .collect()
    }

    pub(crate) fn update_caravan(&mut self, fid: FigureId) {
        let Some(f) = self.figures.get(fid) else { return };
        let (act, yard) = (f.action, f.home);
        match act {
            action::CREATED => {
                let f = self.figures.get_mut(fid).expect("present");
                f.counter += 1;
                if f.counter > CREATED_WAIT {
                    f.counter = 0;
                    self.caravan_next_yard(fid, None, true);
                }
            }
            action::TO_YARD | action::LEAVING => {
                let map = &self.map;
                let f = self.figures.get_mut(fid).expect("present");
                match f.walk(map) {
                    Step::Moving => {}
                    Step::Arrived if act == action::TO_YARD => {
                        // A yard pulled down or left without staff on the way sends the
                        // caravan choosing again, at once.
                        let gone = self.buildings.get(yard).is_none_or(|b| b.kind != kind::STORAGE_YARD || b.workers <= 0);
                        let f = self.figures.get_mut(fid).expect("present");
                        (f.action, f.counter) = if gone { (action::CREATED, CREATED_WAIT) } else { (action::TRADING, DEAL_TICKS) };
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
                    self.caravan_next_yard(fid, Some(yard), false);
                }
            }
            _ => {}
        }
    }

    /// One round of dealing at a yard: buy a load, then sell a load. False if neither
    /// was possible. The caravan buys the good in the last of the yard's spaces holding
    /// one it wants (nothing if any of them is down to its last part load), and sells
    /// (when the yard would take a sale at all) the goods it has in turn, a load of
    /// each, the rotation moving on each sale.
    fn caravan_deal(&mut self, fid: FigureId, yard: BuildingId) -> bool {
        let Some(city) = self.trader_city(fid) else { return false };
        let Some(f) = self.figures.get(fid) else { return false };
        // `amount` counts loads bought from us, `cargo` loads sold to us (x100 units).
        let (capacity, bought, sold) = (f.roam_left, f.amount, f.cargo as i32 * LOAD);
        let route = self.trade.cities[city].route as usize;
        let mut dealt = false;
        let wanted = || {
            let spaces = self.buildings.get(yard).map_or_else(Vec::new, |b| b.spaces.clone());
            let mut pick = None;
            for (r, n) in spaces {
                if n <= 0 || !self.can_export(city, r) {
                    continue;
                }
                if self.stored(yard, r) < LOAD {
                    return None;
                }
                pick = Some(r);
            }
            pick
        };
        if bought + LOAD <= capacity
            && let Some(r) = wanted()
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
        let sells = sold + LOAD <= capacity && self.yard_takes_sale(yard, city, sold / LOAD);
        let start = self.trade.next_import.max(1) as usize;
        let mut turn = (0..RESOURCES - 1).map(|i| (1 + (start - 1 + i) % (RESOURCES - 1)) as u16);
        if sells
            && let Some(r) = turn.find(|&r| self.can_import(city, r) && self.storage_room(yard, r) >= LOAD)
        {
            self.trade.next_import = 1 + r % (RESOURCES as u16 - 1);
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

    /// The caravan (or ship) has left the city, freeing its city's slot.
    pub(crate) fn caravan_gone(&mut self, fid: FigureId) {
        if let Some(f) = self.figures.get_mut(fid) {
            f.dead = true;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::religion::ra_allowance;

    /// The sandbox map with an open land route (allowances 2500 and 4000, so three
    /// traders at a time) and an open sea route, when the game data is present.
    fn sandbox() -> Option<World> {
        let data = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../PharaohData");
        if !data.join("Maps/Sandbox.map").is_file() {
            return None;
        }
        let library = osiris_formats::ImageLibrary::open(&data.join("Data")).ok()?;
        let scenario = osiris_formats::Scenario::load_map(&data.join("Maps/Sandbox.map")).ok()?;
        let defs = std::sync::Arc::new(crate::defs::Defs::load(&library).ok()?);
        let model = osiris_formats::Model::parse(&String::from_utf8_lossy(&std::fs::read(data.join("Pharaoh_Model_Normal.txt")).ok()?)).ok()?;
        let balance = std::sync::Arc::new(crate::balance::Balance::from_model(&model));
        let mut world = World::new(&scenario, defs, balance);
        world.start(&scenario);
        world.messages.clear();
        let mut t = Trade { cities: Vec::new(), routes: Vec::new(), ..world.trade.clone() };
        for sea in [false, true] {
            let mut route = TradeRoute { points: vec![(0, 0), (10, 10)], step: 5, sea, limit: vec![0; RESOURCES], traded: vec![0; RESOURCES] };
            route.limit[5] = 2500;
            route.limit[6] = 4000;
            let mut c = TradeCity { city_type: city::EGYPTIAN_TRADING, route: t.routes.len() as u8, open: true, sea, entry_delay: LAND_ENTRY_DELAY, ..Default::default() };
            c.sells = vec![false; RESOURCES];
            c.buys = vec![false; RESOURCES];
            c.sells[5] = true;
            c.buys[6] = true;
            t.routes.push(route);
            t.cities.push(c);
        }
        world.trade = t;
        Some(world)
    }

    #[test]
    fn a_city_trades_its_listed_goods_only_while_it_is_a_trading_city() {
        let Some(mut w) = sandbox() else { return };
        // Only the land city is open.
        w.trade.cities[1].open = false;
        let c = &mut w.trade.cities[0];
        (c.listed_sells, c.listed_buys) = (c.sells.clone(), c.buys.clone());
        w.set_trade(5, status::IMPORT, 1000);
        w.change_trade_city(0, city::EGYPTIAN, true);
        let c = &w.trade.cities[0];
        assert!(!c.open && !c.sells[5] && !c.buys[6]);
        assert_eq!(w.trade.status[5], status::NONE, "no open route brings it any more");
        w.change_trade_city(0, city::EGYPTIAN_TRADING, false);
        let c = &w.trade.cities[0];
        assert!(!c.open && c.sells[5] && c.buys[6]);
    }

    /// Caravans alive in the city.
    fn caravans(w: &World) -> usize {
        w.figures.iter().filter(|f| f.kind == TRADE_CARAVAN && !f.dead).count()
    }

    #[test]
    fn traders_appear_at_once_every_fifth_day() {
        let Some(mut w) = sandbox() else { return };
        // A day of trade, keeping every caravan but `gone` in the city whatever its way
        // out.
        let day = |w: &mut World, gone: FigureId| {
            w.update_trade();
            let ids: Vec<FigureId> = w.figures.iter().filter(|f| f.kind == TRADE_CARAVAN && f.id != gone).map(|f| f.id).collect();
            for id in ids {
                w.figures.get_mut(id).expect("present").dead = false;
            }
        };
        let mut seen = Vec::new();
        for d in 1..=30 {
            day(&mut w, 0);
            seen.push((d, caravans(&w)));
        }
        let first = |n: usize| seen.iter().find(|s| s.1 >= n).map(|s| s.0);
        assert_eq!((first(1), first(2), first(3), first(4)), (Some(5), Some(10), Some(15), None));
        // The countdown waits while all three are here; once one leaves, the next comes
        // five days later.
        let gone = w.trade.cities[0].traders[1];
        w.caravan_gone(gone);
        let days = (1..=10).find(|_| {
            day(&mut w, gone);
            caravans(&w) == 3
        });
        assert_eq!(days, Some(5));
        assert_eq!(w.trade.cities[0].traders.iter().filter(|&&t| t != 0 && t != gone).count(), 3);
    }

    #[test]
    fn ships_wait_for_a_dock() {
        let Some(mut w) = sandbox() else { return };
        w.trade.cities[0].open = false;
        for _ in 0..40 {
            w.update_trade();
        }
        assert!(w.figures.iter().all(|f| f.kind != crate::docks::TRADE_SHIP));
        assert_eq!(w.trade.cities[1].entry_delay, LAND_ENTRY_DELAY, "the countdown waits for a dock");
        assert_eq!(w.messages.iter().filter(|m| *m == "message_no_working_dock").count(), 1);
    }

    #[test]
    fn routes_open_into_debt_until_5000_owed() {
        let Some(mut w) = sandbox() else { return };
        w.trade.cities[0].open = false;
        w.trade.cities[0].cost = 1000;
        w.treasury = -4999;
        w.finance.this_year.construction = 0;
        assert!(w.open_trade_route(0).is_ok());
        assert_eq!((w.treasury, w.finance.this_year.construction), (-5999, 1000));
        w.trade.cities[0].open = false;
        assert!(w.open_trade_route(0).is_err(), "5000 or more in debt");
        assert_eq!(w.treasury, -5999);
    }

    /// Mission 12 with a road up from the entry point and three storage yards beside
    /// it, and the route to city 8 (which buys bricks, 12) open.
    fn yards_town() -> Option<(World, [BuildingId; 3])> {
        let data = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../PharaohData");
        if !data.join("mission1.pak").is_file() {
            return None;
        }
        let library = osiris_formats::ImageLibrary::open(&data.join("Data")).ok()?;
        let scenario = osiris_formats::MissionPak::open(&data.join("mission1.pak")).ok()?.scenario(12).ok()?;
        let defs = std::sync::Arc::new(crate::defs::Defs::load(&library).ok()?);
        let balances = crate::balance::Balance::load_all(&data).ok()?;
        let mut world = World::new(&scenario, defs, balances[2].clone());
        world.start(&scenario);
        world.load_mission(12);
        world.treasury = 20000;
        let road = world.apply(&crate::world::Command::Road { start: (111, 141), end: (111, 118) });
        assert!(matches!(road, crate::world::Outcome::Done { items: 1.., .. }), "{road:?}");
        let yards = [(112, 125), (112, 130), (108, 125)].map(|(x, y)| world.create_building(kind::STORAGE_YARD, x, y));
        world.trade.cities[8].open = true;
        world.set_trade(12, status::EXPORT, 0);
        Some((world, yards))
    }

    #[test]
    fn caravans_wait_then_choose_a_staffed_yard_with_goods_to_sell() {
        let Some((mut w, [a, b, c])) = yards_town() else { return };
        // A holds a load of bricks, B three spaces of them; C holds a yard full but has
        // no staff.
        w.add_stored(a, 12, 100);
        w.add_stored(b, 12, 1200);
        w.add_stored(c, 12, 3200);
        for (id, staff) in [(a, 6), (b, 6), (c, 0)] {
            w.buildings.get_mut(id).unwrap().workers = staff;
        }
        let fid = w.caravan_arrives(8);
        for _ in 0..CREATED_WAIT {
            w.update_caravan(fid);
        }
        assert_eq!(w.figures.get(fid).unwrap().action, action::CREATED, "it stands 20 ticks at the entry point");
        w.update_caravan(fid);
        let f = w.figures.get(fid).unwrap();
        assert_eq!((f.action, f.home), (action::TO_YARD, b), "B: more bricks than A");
        // Staff gone on the way, it chooses again: A.
        w.buildings.get_mut(b).unwrap().workers = 0;
        w.figures.get_mut(fid).unwrap().action = action::CREATED;
        w.figures.get_mut(fid).unwrap().counter = CREATED_WAIT;
        w.update_caravan(fid);
        assert_eq!(w.figures.get(fid).unwrap().home, a);
        // With no staffed yard holding bricks, it leaves.
        w.buildings.get_mut(a).unwrap().workers = 0;
        w.caravan_next_yard(fid, None, true);
        assert_eq!(w.figures.get(fid).unwrap().action, action::LEAVING);
    }

    #[test]
    fn a_caravan_buys_a_load_every_eleven_ticks_up_to_eight() {
        let Some((mut w, [a, _, _])) = yards_town() else { return };
        w.add_stored(a, 12, 1200);
        w.buildings.get_mut(a).unwrap().workers = 6;
        let fid = w.caravan_arrives(8);
        let f = w.figures.get_mut(fid).unwrap();
        (f.action, f.home, f.counter) = (action::TRADING, a, DEAL_TICKS);
        let treasury = w.treasury;
        for _ in 0..DEAL_TICKS * 10 {
            w.update_caravan(fid);
        }
        assert_eq!(w.figures.get(fid).unwrap().amount, 8 * LOAD);
        assert_eq!(w.stored(a, 12), 400);
        assert_eq!(w.treasury - treasury, 8 * w.sell_price(12));
        assert_eq!(w.figures.get(fid).unwrap().action, action::LEAVING, "full up, it goes");
    }

    #[test]
    fn ra_moves_allowances_a_step() {
        assert_eq!(ra_allowance(1, 1500), 2500);
        assert_eq!(ra_allowance(1, 4000), 4000);
        assert_eq!(ra_allowance(1, 0), 1500);
        assert_eq!(ra_allowance(-1, 4000), 2500);
        assert_eq!(ra_allowance(-1, 1500), 0);
        assert_eq!(ra_allowance(-3, 4000), 1500);
        assert_eq!(ra_allowance(-2, 2500), 0);
        assert_eq!(ra_allowance(-1, 1200), 1200);
    }
}
