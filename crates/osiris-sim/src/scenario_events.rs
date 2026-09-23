//! Scenario events: what the scenario has planned for the city. Once a month every
//! event whose date has come fires: Pharaoh or a city asks for goods, sends a gift,
//! prices or demand change, a route opens or closes, the flood fails, a mine caves
//! in. An event can instead wait for another: when a request is met, met late or
//! refused, the event it points to for that outcome follows (after its own delay).
//!
//! A request gives the player a number of months to send the goods (from the
//! Political Overseer); a reminder comes six months before the deadline. Sending on
//! time raises the kingdom rating by 3. Missing the deadline costs 3 and starts a
//! 24-month grace period: sending then earns back 1, letting it pass costs 2 more.

use crate::buildings::kind;
use crate::economy::resource;
use crate::world::World;
use osiris_formats::{EventRecord, EventValue};

/// A value the scenario lets the game pick (see `World::roll`).
#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
pub struct Pick {
    pub value: i16,
    pub fixed: i16,
    pub min: i16,
    pub max: i16,
}

impl From<EventValue> for Pick {
    fn from(v: EventValue) -> Self {
        Self { value: v.value, fixed: v.fixed, min: v.min, max: v.max }
    }
}

/// Event types.
pub mod event {
    pub const REQUEST: u8 = 1;
    pub const INVASION: u8 = 2;
    pub const SEA_TRADE_PROBLEM: u8 = 6;
    pub const LAND_TRADE_PROBLEM: u8 = 7;
    pub const WAGE_INCREASE: u8 = 8;
    pub const WAGE_DECREASE: u8 = 9;
    pub const CONTAMINATED_WATER: u8 = 10;
    pub const GOLD_MINE_COLLAPSE: u8 = 11;
    pub const CLAY_PIT_FLOOD: u8 = 12;
    pub const DEMAND_INCREASE: u8 = 13;
    pub const DEMAND_DECREASE: u8 = 14;
    pub const PRICE_INCREASE: u8 = 15;
    pub const PRICE_DECREASE: u8 = 16;
    pub const REPUTATION_INCREASE: u8 = 17;
    pub const REPUTATION_DECREASE: u8 = 18;
    pub const CITY_STATUS_CHANGE: u8 = 19;
    pub const MESSAGE: u8 = 20;
    pub const FAILED_FLOOD: u8 = 21;
    pub const PERFECT_FLOOD: u8 = 22;
    pub const GIFT: u8 = 23;
    pub const LOCUSTS: u8 = 24;
    pub const FROGS: u8 = 25;
    pub const HAILSTORM: u8 = 26;
    pub const BLOOD_RIVER: u8 = 27;
}

/// How an event comes about.
pub mod trigger {
    pub const ONCE: u8 = 0;
    pub const ONLY_VIA_EVENT: u8 = 1;
    pub const RECURRING: u8 = 2;
    pub const FIRED: u8 = 4;
}

/// How a request ended, which decides the event that follows and the reason given.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Outcome {
    Completed,
    TooLate,
    Refused,
}

/// Request states.
pub mod state {
    pub const WAITING: u8 = 0;
    pub const IN_PROGRESS: u8 = 1;
    pub const OVERDUE: u8 = 2;
    pub const RECEIVED: u8 = 3;
    pub const FAILED: u8 = 4;
}

/// Months of grace after a request's deadline.
const GRACE_MONTHS: i32 = 24;
/// Days trade stops for after a storm or sandstorm.
const TRADE_PROBLEM_DAYS: i32 = 48;
/// Months a siege lasts when the event does not say.
const DEFAULT_SIEGE_MONTHS: i32 = 12;
pub const DEBEN: u16 = 36;
pub const TROOPS: u16 = 37;

/// A pop-up written from `eventmsg.txt` phrases, filled in by the app.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct EventText {
    /// Phrase names (without the `PHRASE_` prefix).
    pub title: String,
    pub body: String,
    pub reason: String,
    /// What fills the phrases' blanks.
    pub resource: u16,
    pub amount: i32,
    /// The city's name id (text group 195), if any.
    pub city_name: Option<u8>,
    pub months: i32,
    /// The template message: 130 for requests, 131 otherwise.
    pub template: u16,
    /// For reasons that describe the event before: its resource, amount and city.
    pub cause: Option<(u16, i32, Option<u8>)>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ScenarioEvent {
    pub kind: u8,
    pub trigger: u8,
    pub subtype: i8,
    /// 1 when Pharaoh asks or gives, 0 a city.
    pub pharaoh: bool,
    /// Fires in this month (0 = January) of this many years after the start.
    pub month: i32,
    pub year: i32,
    /// For events that follow others: (fixed, min, max) of the months to wait.
    delay: (i16, i16, i16),
    /// For recurring events: years between occurrences.
    interval: (i16, i16),
    item: Pick,
    amount_pick: Pick,
    /// Route numbers of the cities the event may concern.
    cities: (i16, i16),
    pub on_completed: i16,
    pub on_refusal: i16,
    pub on_too_late: i16,
    pub months_initial: i32,
    /// What this occurrence is about.
    pub resource: u16,
    pub amount: i32,
    pub city: Option<usize>,
    /// Requests: state, months left, and the messages already shown.
    pub state: u8,
    pub months_left: i32,
    pub active: bool,
    pub overdue: bool,
    can_comply_shown: bool,
    announced: bool,
    /// Why this event happened, when another event led to it.
    cause: Option<(usize, Outcome)>,
    /// Months until a follow-up event fires.
    pub wait: i32,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ScenarioEvents {
    pub list: Vec<ScenarioEvent>,
    pub start_year: i32,
    /// Days left of storms at sea and trouble on land routes.
    pub sea_problem_days: i32,
    pub land_problem_days: i32,
}

impl ScenarioEvents {
    pub fn from_records(records: &[EventRecord], start_year: i32) -> Self {
        let list = records
            .iter()
            .map(|e| ScenarioEvent {
                kind: e.kind,
                trigger: e.trigger,
                subtype: e.subtype,
                pharaoh: e.sender == 1,
                month: e.month as i32,
                year: e.year as i32,
                delay: (e.month, e.time.min, e.time.max),
                interval: (e.time.min, e.time.max),
                item: e.item.into(),
                amount_pick: e.amount.into(),
                cities: (e.location[0], e.location[1]),
                on_completed: e.on_completed,
                on_refusal: e.on_refusal,
                on_too_late: e.on_too_late,
                months_initial: e.months as i32,
                ..Default::default()
            })
            .collect();
        Self { list, start_year, ..Default::default() }
    }

    /// Requests the player can still answer, for the Political Overseer.
    pub fn open_requests(&self) -> impl Iterator<Item = (usize, &ScenarioEvent)> {
        self.list.iter().enumerate().filter(|(_, e)| e.kind == event::REQUEST && e.active && e.state <= state::OVERDUE)
    }
}

/// The phrase group a request's messages come from, by its reason.
fn request_group(subtype: i8) -> &'static str {
    match subtype {
        1 => "great_festival",
        2 => "project",
        3 => "famine",
        4 => "threat",
        5 => "egyptian_city_attacked",
        6 => "distant_battle",
        _ => "general_request",
    }
}

impl ScenarioEvent {
    fn side(&self) -> &'static str {
        if self.pharaoh { "P" } else { "C" }
    }

    /// Units the request or gift is for: amounts under 100 are loads of 100.
    pub fn units(&self) -> i32 {
        if self.resource == DEBEN || self.resource == TROOPS || self.amount >= 100 { self.amount } else { self.amount * 100 }
    }
}

impl World {
    /// A pick among the scenario's choices: a fixed value, one of up to three values,
    /// or anything in a range.
    fn roll(&mut self, v: Pick) -> i32 {
        let (fixed, min, max) = (v.fixed as i32, v.min as i32, v.max as i32);
        if fixed == -1 && min > -1 && max > -1 && max == min {
            return fixed;
        }
        if max == -1 {
            return fixed;
        }
        if fixed < 0 {
            let range = max - min;
            return if range <= 0 { v.value as i32 } else { self.rng.below(range) + min };
        }
        let choices = if min < 0 { 1 } else if max > -1 { 3 } else { 2 };
        match self.rng.below(choices) {
            0 => fixed,
            1 => min,
            _ => max,
        }
    }

    fn years_since_start(&self) -> i32 {
        self.time.year - self.scenario_events.start_year
    }

    /// The empire city with route number `n`.
    fn city_by_route(&self, n: i32) -> Option<usize> {
        self.trade.cities.iter().position(|c| c.route as i32 == n && n > 0)
    }

    /// Monthly: events whose time has come fire, and open requests move on.
    pub(crate) fn process_scenario_events(&mut self) {
        let now = (self.years_since_start(), self.time.month as i32);
        for i in 0..self.scenario_events.list.len() {
            let e = &mut self.scenario_events.list[i];
            let due = match e.trigger {
                trigger::ONCE | trigger::RECURRING => !e.active && (e.year, e.month) == now,
                trigger::FIRED if e.cause.is_some() => {
                    e.wait -= 1;
                    e.wait < 0
                }
                _ => false,
            };
            if due {
                self.fire_event(i);
            }
        }
        for i in 0..self.scenario_events.list.len() {
            let e = &self.scenario_events.list[i];
            if e.kind == event::REQUEST && e.active {
                self.update_request(i);
            }
        }
    }

    /// The event `i` follows another: it fires after its delay.
    fn follow(&mut self, i: i16, parent: usize, outcome: Outcome) {
        let Some(e) = usize::try_from(i).ok().and_then(|i| self.scenario_events.list.get(i)) else { return };
        if e.trigger != trigger::ONLY_VIA_EVENT && !(e.trigger == trigger::FIRED && e.cause.is_some()) {
            return;
        }
        let (fixed, min, max) = e.delay;
        let wait = self.roll(Pick { value: 0, fixed, min, max }).max(0);
        let e = &mut self.scenario_events.list[i as usize];
        e.trigger = trigger::FIRED;
        e.cause = Some((parent, outcome));
        e.wait = wait;
    }

    /// Runs event `i`'s effect and message.
    fn fire_event(&mut self, i: usize) {
        let e = self.scenario_events.list[i].clone();
        let resource = self.roll(e.item).max(0) as u16;
        let amount = self.roll(e.amount_pick);
        let route = if e.cities.1 > e.cities.0 { e.cities.0 as i32 + self.rng.below((e.cities.1 - e.cities.0 + 1) as i32) } else { e.cities.0 as i32 };
        let city = self.city_by_route(route);
        {
            let ev = &mut self.scenario_events.list[i];
            ev.resource = resource;
            ev.amount = amount;
            ev.city = city;
            if ev.trigger == trigger::ONCE {
                ev.trigger = trigger::FIRED;
            }
        }
        if e.trigger == trigger::RECURRING {
            let (min, max) = (e.interval.0 as i32, e.interval.1 as i32);
            let years = if min >= 0 && max > min { min + self.rng.below(max - min + 1) } else { min.max(max) };
            self.scenario_events.list[i].year += years.max(1);
        }
        if e.trigger == trigger::FIRED {
            // A follow-up is spent until another event calls on it again.
            self.scenario_events.list[i].trigger = trigger::ONLY_VIA_EVENT;
        }
        let city_name = city.map(|c| self.trade.cities[c].name_id);
        let mut text = EventText { resource, amount, city_name, template: 131, cause: self.cause_of(i), ..Default::default() };
        let up = |name: &str| format!("{name}_I");
        let down = |name: &str| format!("{name}_D");
        let mut phrases: Option<(String, String, String)> = None;
        let mut popup_key: Option<&str> = None;
        match e.kind {
            event::REQUEST => {
                let ev = &mut self.scenario_events.list[i];
                ev.state = state::WAITING;
                ev.months_left = ev.months_initial;
                ev.active = true;
                ev.overdue = false;
                ev.can_comply_shown = false;
                ev.announced = false;
            }
            event::SEA_TRADE_PROBLEM => {
                if self.trade.cities.iter().any(|c| c.open && c.sea) {
                    self.scenario_events.sea_problem_days = TRADE_PROBLEM_DAYS;
                    phrases = Some(("stormy_seas_title".into(), "stormy_seas_initial_announcement".into(), "stormy_seas_no_reason_A".into()));
                }
            }
            event::LAND_TRADE_PROBLEM => {
                if self.trade.cities.iter().any(|c| c.open && !c.sea) {
                    self.scenario_events.land_problem_days = TRADE_PROBLEM_DAYS;
                    let name = if self.climate == 2 { "sandstorm" } else { "landslide" };
                    phrases = Some((format!("{name}_title"), format!("{name}_initial_announcement"), format!("{name}_no_reason_A")));
                }
            }
            event::WAGE_INCREASE | event::WAGE_DECREASE => {
                let raise = e.kind == event::WAGE_INCREASE;
                let delta = amount.max(1);
                self.finance.kingdom_wages = (self.finance.kingdom_wages + if raise { delta } else { -delta }).max(0);
                let f = if raise { up } else { down };
                phrases = Some((f("wage_change_title"), f("wage_change_initial_announcement"), f("wage_change_no_reason") + "_A"));
            }
            event::CONTAMINATED_WATER => {
                if self.population > 200 {
                    let health = self.ratings.health;
                    let change = if health > 80 {
                        -50
                    } else if health > 60 {
                        -40
                    } else {
                        -25
                    };
                    self.ratings.health = (health + change).clamp(0, 100);
                    phrases = Some(("bad_water_title".into(), "bad_water_initial_announcement".into(), "bad_water_no_reason_A".into()));
                }
            }
            event::GOLD_MINE_COLLAPSE | event::CLAY_PIT_FLOOD => {
                let target = if e.kind == event::GOLD_MINE_COLLAPSE { kind::GOLD_MINE } else { kind::CLAY_PIT };
                let ids: Vec<u32> = self.buildings.iter().filter(|b| b.kind == target).map(|b| b.id).collect();
                if !ids.is_empty() {
                    let id = ids[self.rng.below(ids.len() as i32) as usize];
                    let tile = self.buildings.get(id).map(|b| (b.x, b.y));
                    self.wreck(id, false);
                    if e.kind == event::GOLD_MINE_COLLAPSE {
                        phrases = Some(("goldmine_cavein_title".into(), "goldmine_cavein_initial_announcement".into(), "goldmine_cavein_no_reason_A".into()));
                    } else {
                        self.post("message_tutorial_flooded_clay_pit", tile, true);
                    }
                }
            }
            event::DEMAND_INCREASE | event::DEMAND_DECREASE => {
                let raise = e.kind == event::DEMAND_INCREASE;
                let mut changed = None;
                for c in 0..self.trade.cities.len() {
                    let tc = &self.trade.cities[c];
                    let r = resource as usize;
                    if !tc.trades() || !(tc.sells.get(r).copied().unwrap_or(false) || tc.buys.get(r).copied().unwrap_or(false)) {
                        continue;
                    }
                    if city.is_some_and(|x| x != c) {
                        continue;
                    }
                    let route = tc.route as usize;
                    if let Some(rt) = self.trade.routes.get_mut(route) {
                        const TIERS: [i32; 4] = [0, 1500, 2500, 4000];
                        let tier = TIERS.iter().position(|&t| t >= rt.limit[r]).unwrap_or(3) as i32;
                        let next = (tier + if raise { 1 } else { -1 }).clamp(1, 3) as usize;
                        rt.limit[r] = TIERS[next];
                        changed = Some(c);
                    }
                }
                if let Some(c) = changed {
                    text.city_name = Some(self.trade.cities[c].name_id);
                    let f = if raise { up } else { down };
                    phrases = Some((f("demand_change_title"), f("demand_change_initial_announcement"), f("demand_change_no_reason") + "_A"));
                }
            }
            event::PRICE_INCREASE | event::PRICE_DECREASE => {
                let raise = e.kind == event::PRICE_INCREASE;
                let delta = if raise { amount } else { -amount };
                if let Some(p) = self.trade.prices.get_mut(resource as usize) {
                    let before = *p;
                    p.0 = (p.0 + delta).max(2);
                    p.1 = (p.1 + delta).max(0);
                    if *p != before {
                        let f = if raise { up } else { down };
                        phrases = Some((f("price_change_title"), f("price_change_initial_announcement"), f("price_change_no_reason") + "_A"));
                    }
                }
            }
            event::REPUTATION_INCREASE | event::REPUTATION_DECREASE => {
                let raise = e.kind == event::REPUTATION_INCREASE;
                self.ratings.change_kingdom(if raise { amount } else { -amount });
                let f = if raise { up } else { down };
                phrases = Some((f("rating_change_title"), f("rating_change_initial_announcement"), f("rating_change_no_reason") + "_A"));
            }
            event::CITY_STATUS_CHANGE | event::MESSAGE => {
                phrases = self.city_status_change(&e, city);
            }
            event::FAILED_FLOOD => {
                self.adjust_next_flood_quality(-100);
                phrases = Some(("flood_fails_title".into(), "flood_fails_initial_announcement".into(), "flood_fails_no_reason_A".into()));
            }
            event::PERFECT_FLOOD => {
                self.adjust_next_flood_quality(100);
                phrases = Some(("perfect_flood_title".into(), "perfect_flood_initial_announcement".into(), "perfect_flood_no_reason_A".into()));
            }
            event::GIFT => {
                let side = e.side();
                if resource == DEBEN {
                    self.treasury += amount;
                    phrases = Some((format!("gift_title_{side}"), format!("gift_cash_granted_{side}"), format!("gift_no_reason_{side}_A")));
                } else if resource > 0 && amount > 0 {
                    let units = if amount >= 100 { amount } else { amount * 100 };
                    let left = self.deliver_to_storage(resource, units);
                    let body = if left == 0 {
                        "gift_granted"
                    } else if left < units {
                        "gift_partial_space"
                    } else {
                        "gift_insufficient_space"
                    };
                    phrases = Some((format!("gift_title_{side}"), format!("{body}_{side}"), format!("gift_no_reason_{side}_A")));
                }
            }
            event::LOCUSTS => {
                // The swarm eats the crops of as many farms as the event says.
                let farms: Vec<u32> = self.buildings.iter().filter(|b| self.is_farm(b.kind) && b.progress > 0).map(|b| b.id).collect();
                for &id in farms.iter().take(amount.max(1) as usize) {
                    if let Some(b) = self.buildings.get_mut(id) {
                        b.progress = 0;
                    }
                }
                popup_key = Some("message_plague_of_locusts");
            }
            event::FROGS => popup_key = Some("message_plague_of_frogs"),
            event::HAILSTORM => popup_key = Some("message_hailstorm"),
            event::BLOOD_RIVER => popup_key = Some("message_river_of_blood"),
            _ => {}
        }
        if let Some(key) = popup_key {
            self.post(key, None, true);
        }
        if let Some((title, body, no_reason)) = phrases {
            text.title = title;
            text.body = body;
            text.reason = self.reason_phrase(i).unwrap_or(no_reason);
            self.post_event_text(text);
        }
        // Everything but a request leads straight on to what follows it.
        if e.kind != event::REQUEST {
            let on = self.scenario_events.list[i].on_completed;
            self.follow(on, i, Outcome::Completed);
        }
    }

    /// City status changes and messages about other cities.
    fn city_status_change(&mut self, e: &ScenarioEvent, city: Option<usize>) -> Option<(String, String, String)> {
        use osiris_formats::empire::city as ct;
        let names = |n: &str| Some((format!("{n}_title"), format!("{n}_initial_announcement"), format!("{n}_no_reason_A")));
        if e.kind == event::MESSAGE {
            return match e.subtype {
                0 => names("eg_city_saved"),
                1 => names("foreign_city_conquered"),
                2 => names("battle_lost"),
                3 => names("acknowledgement"),
                4 => self.siege(e, city),
                _ => None,
            };
        }
        let c = city?;
        match e.subtype {
            0 => {
                let tc = &mut self.trade.cities[c];
                tc.open = false;
                tc.city_type = ct::FOREIGN;
                names("eg_city_falls")
            }
            1 => names("foreign_city_conquered"),
            2 => {
                let tc = &mut self.trade.cities[c];
                tc.city_type = match tc.city_type {
                    ct::PHARAOH => ct::PHARAOH_TRADING,
                    ct::EGYPTIAN => ct::EGYPTIAN_TRADING,
                    ct::FOREIGN => ct::FOREIGN_TRADING,
                    t => t,
                };
                names("route_opened")
            }
            3 => {
                let tc = &mut self.trade.cities[c];
                tc.open = false;
                tc.city_type = match tc.city_type {
                    ct::PHARAOH_TRADING => ct::PHARAOH,
                    ct::EGYPTIAN_TRADING => ct::EGYPTIAN,
                    ct::FOREIGN_TRADING => ct::FOREIGN,
                    t => t,
                };
                names("route_closed")
            }
            4 => self.siege(e, city),
            _ => None,
        }
    }

    /// A trade city falls under siege: its traders stay home for the months given.
    fn siege(&mut self, e: &ScenarioEvent, city: Option<usize>) -> Option<(String, String, String)> {
        let c = city.or_else(|| self.trade.cities.iter().position(|c| c.open))?;
        let months = if e.months_initial > 0 {
            e.months_initial
        } else if e.amount > 0 {
            e.amount
        } else {
            DEFAULT_SIEGE_MONTHS
        };
        self.trade.cities[c].siege_months = months;
        Some(("siege_title".into(), "siege_initial_announcement".into(), "siege_no_reason_A".into()))
    }

    /// The reason phrase for event `i` when an earlier event led to it: what happened
    /// with that event, in its own words.
    fn reason_phrase(&self, i: usize) -> Option<String> {
        let (parent, outcome) = self.scenario_events.list[i].cause?;
        let p = self.scenario_events.list.get(parent)?;
        if p.kind != event::REQUEST {
            return None;
        }
        let what = match outcome {
            Outcome::Completed => "comply_reason",
            Outcome::TooLate => "too_late_reason",
            Outcome::Refused => "refuse_reason",
        };
        Some(format!("{}_{what}_{}_A", request_group(p.subtype), p.side()))
    }

    fn cause_of(&self, i: usize) -> Option<(u16, i32, Option<u8>)> {
        let (parent, _) = self.scenario_events.list[i].cause?;
        let p = self.scenario_events.list.get(parent)?;
        Some((p.resource, p.units(), p.city.map(|c| self.trade.cities[c].name_id)))
    }

    fn post_event_text(&mut self, text: EventText) {
        let key = if text.template == 130 { "message_template_request" } else { "message_template_general" };
        self.post(key, None, true);
        if let Some(n) = self.notices.log.last_mut() {
            n.text = Some(text.clone());
        }
        self.message_texts.push_back(text);
    }

    /// Monthly: a request's clock runs; the player hears of it, is reminded, and is
    /// told when the deadline passes.
    fn update_request(&mut self, i: usize) {
        let can_send = self.can_send_request(i);
        let e = &mut self.scenario_events.list[i];
        if e.months_left > 0 {
            e.months_left -= 1;
        }
        if e.state == state::WAITING {
            e.state = state::IN_PROGRESS;
        }
        let group = request_group(e.subtype);
        let side = e.side();
        let text = |this: &ScenarioEvent, body: &str| EventText {
            title: format!("{group}_title_{side}"),
            body: format!("{group}_{body}_{side}"),
            reason: format!("{group}_no_reason_{side}_A"),
            resource: this.resource,
            amount: this.units(),
            city_name: None,
            months: this.months_left,
            template: 130,
            cause: None,
        };
        let mut posts: Vec<EventText> = Vec::new();
        let mut comply_ready = false;
        match e.state {
            state::IN_PROGRESS => {
                if !e.can_comply_shown && can_send {
                    e.can_comply_shown = true;
                    comply_ready = true;
                } else if !e.announced {
                    e.announced = true;
                    posts.push(text(e, "initial_announcement"));
                }
                if e.months_left == 6 {
                    posts.push(text(e, "reminder"));
                }
                if e.months_left == 0 {
                    e.state = state::OVERDUE;
                    e.overdue = true;
                    e.months_left = GRACE_MONTHS;
                    posts.push(text(e, "overdue"));
                    self.ratings.change_kingdom(-3);
                }
            }
            state::OVERDUE if e.months_left == 0 => {
                e.state = state::FAILED;
                e.active = false;
                self.ratings.change_kingdom(-2);
                let next = e.on_refusal;
                self.follow(next, i, Outcome::Refused);
            }
            _ => {}
        }
        let city_name = self.scenario_events.list[i].city.map(|c| self.trade.cities[c].name_id);
        if comply_ready {
            let deben = self.scenario_events.list[i].resource == DEBEN;
            self.post(if deben { "message_compliance_now_possible" } else { "message_storage_yards_ready_to_fulfill_request" }, None, true);
        }
        for mut t in posts {
            t.city_name = city_name;
            if let Some(reason) = self.reason_phrase(i) {
                t.reason = reason;
                t.cause = self.cause_of(i);
            }
            self.post_event_text(t);
        }
    }

    /// Whether the city holds what request `i` asks for.
    pub fn can_send_request(&self, i: usize) -> bool {
        let Some(e) = self.scenario_events.list.get(i) else { return false };
        match e.resource {
            DEBEN => self.treasury > e.amount,
            TROOPS => false,
            r => self.city_stored(r) >= e.units(),
        }
    }

    /// Units of `r` in storage yards and, for food, granaries.
    pub fn city_stored(&self, r: u16) -> i32 {
        self.buildings
            .iter()
            .filter(|b| b.kind == kind::STORAGE_YARD || resource::is_food(r) && b.kind == kind::GRANARY)
            .map(|b| self.stored(b.id, r))
            .sum()
    }

    /// The player sends what request `i` asks for. The goods leave storage at once.
    pub fn dispatch_request(&mut self, i: usize) -> bool {
        if !self.can_send_request(i) {
            return false;
        }
        let e = &self.scenario_events.list[i];
        if !(e.kind == event::REQUEST && e.active && e.state <= state::OVERDUE) {
            return false;
        }
        let (r, units, overdue) = (e.resource, e.units(), e.overdue);
        if r == DEBEN {
            self.treasury -= units;
        } else {
            let mut left = units;
            let stores: Vec<u32> = self.buildings.iter().filter(|b| b.kind == kind::STORAGE_YARD || resource::is_food(r) && b.kind == kind::GRANARY).map(|b| b.id).collect();
            for id in stores {
                if left <= 0 {
                    break;
                }
                left -= self.take_stored(id, r, left);
            }
        }
        let e = &mut self.scenario_events.list[i];
        e.state = state::RECEIVED;
        e.active = false;
        let (next, outcome) = if overdue { (e.on_too_late, Outcome::TooLate) } else { (e.on_completed, Outcome::Completed) };
        self.ratings.change_kingdom(if overdue { 1 } else { 3 });
        self.follow(next, i, outcome);
        true
    }

    /// Puts `units` of `r` into storage yards (and granaries, for food) with room;
    /// returns what did not fit.
    fn deliver_to_storage(&mut self, r: u16, units: i32) -> i32 {
        let mut left = units;
        let stores: Vec<u32> = self.buildings.iter().filter(|b| b.kind == kind::STORAGE_YARD || resource::is_food(r) && b.kind == kind::GRANARY).map(|b| b.id).collect();
        for id in stores {
            if left <= 0 {
                break;
            }
            let room = self.storage_room(id, r).min(left);
            if room > 0 {
                left -= self.add_stored(id, r, room);
            }
        }
        left
    }

    /// Daily: storms and sandstorms blow over.
    pub(crate) fn update_trade_problems(&mut self) {
        let s = &mut self.scenario_events;
        s.sea_problem_days = (s.sea_problem_days - 1).max(0);
        s.land_problem_days = (s.land_problem_days - 1).max(0);
    }

    /// Monthly: sieges wear on.
    pub(crate) fn update_sieges(&mut self) {
        for c in &mut self.trade.cities {
            c.siege_months = (c.siege_months - 1).max(0);
        }
    }
}
