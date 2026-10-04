//! Scenario events: what the scenario has planned for the city. Once a month every
//! event whose date has come fires: Pharaoh or a city asks for goods, sends a gift,
//! prices or demand change, a route opens or closes, the flood fails, a mine caves
//! in. An event can instead wait for another: when a request is met, met late or
//! refused, the event it points to for that outcome follows (after its own delay).
//!
//! A request gives the player a number of months to send the goods (from the
//! Political Overseer); a reminder comes six months before the deadline. Missing the
//! deadline costs 3 kingdom rating and starts a 24-month grace period; letting that
//! pass too is a refusal. Goods sent take one to four months to arrive, and only then
//! does the event tied to the outcome follow (FUN_0044c660, FUN_0044c1a0). The rating
//! changes nothing else here: the scenario rewards or punishes through the events it
//! ties to each outcome.

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
    pub const EARTHQUAKE: u8 = 3;
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
    /// Troops sent for a request lost their battle.
    Defeat,
}

/// Request states.
pub mod state {
    pub const WAITING: u8 = 0;
    pub const IN_PROGRESS: u8 = 1;
    pub const OVERDUE: u8 = 2;
    pub const RECEIVED: u8 = 3;
    pub const FAILED: u8 = 4;
    /// Goods sent in time, still on their way (the original's state 2).
    pub const SENT: u8 = 5;
    /// Goods sent during the grace months, on their way (state 3).
    pub const SENT_LATE: u8 = 6;
}

/// Gift states (the original's numbers, FUN_0044c660's gift branch). Each month a
/// gift waiting for room looks again, and offers itself once there is room for at
/// least half; the third look is the last.
pub mod gift_state {
    /// Offered with room for at least half: waiting for the player's answer.
    pub const OFFERED: u8 = 1;
    /// Too little room; looks again next month.
    pub const WAITING: u8 = 2;
    /// Offered on the second look.
    pub const OFFERED_AGAIN: u8 = 3;
    /// Too little room twice; the next look is the last.
    pub const LAST_TRY: u8 = 4;
    /// Offered on the last look: postponing now forfeits it.
    pub const LAST_CHANCE: u8 = 5;
    /// Too little room on the last look: forfeit, and what follows comes next month.
    pub const FORFEIT: u8 = 8;
    /// Accepted, refused or forfeit.
    pub const DONE: u8 = 9;
}

/// The buttons a gift's message offers (the original's flags at 0xd3b186).
pub mod gift_choice {
    pub const REFUSE: u8 = 1;
    pub const POSTPONE: u8 = 2;
    pub const ACCEPT: u8 = 4;
}

/// Event kinds whose item is a resource (the table at 0x5d9c78).
fn item_is_resource(kind: u8) -> bool {
    matches!(kind, event::REQUEST | event::DEMAND_INCREASE | event::DEMAND_DECREASE | event::PRICE_INCREASE | event::PRICE_DECREASE | event::GIFT)
}

/// Scenarios saved before version 152 number deben and troops 31 and 32, from before
/// the five resources Cleopatra added at 31. On loading them the original moves every
/// resource past 30 that an event names up by five, and packs the item's choices so
/// the ones in use come first, which makes a range of items a choice between its two
/// ends (FUN_004d40c0, run for files older than 0x98).
fn upgrade_item(v: EventValue) -> EventValue {
    let up = |r: i16| if r > 30 { r + 5 } else { r };
    let mut choices = [up(v.fixed), up(v.min), up(v.max)].into_iter().filter(|&r| r >= 0);
    let mut next = || choices.next().unwrap_or(-1);
    EventValue { value: up(v.value), fixed: next(), min: next(), max: next() }
}

/// Months of grace after a request's deadline.
const GRACE_MONTHS: i32 = 24;
/// Days trade stops for after a storm or sandstorm.
const TRADE_PROBLEM_DAYS: i32 = 48;
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
    /// An invading army's title (text group 37).
    #[serde(default)]
    pub army: Option<usize>,
    /// The god a festival is for.
    #[serde(default)]
    pub god: Option<u8>,
    /// The gift event whose offer this is, for its Accept, Refuse and Postpone.
    #[serde(default)]
    pub gift: Option<usize>,
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
    /// The year's pick (fixed, min, max): for an event that follows another the months
    /// it waits, for a recurring one the years until it comes again.
    delay: (i16, i16, i16),
    item: Pick,
    amount_pick: Pick,
    /// Route numbers of the cities the event may concern (older saved games).
    cities: (i16, i16),
    /// The route number of the city the event concerns, picked like its resource.
    #[serde(default)]
    location: Option<Pick>,
    /// The route number this occurrence picked.
    #[serde(default)]
    pub route: i32,
    pub on_completed: i16,
    pub on_refusal: i16,
    pub on_too_late: i16,
    #[serde(default)]
    pub on_defeat: i16,
    #[serde(default)]
    pub god: i8,
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
    /// Requests: the month (counted from the start, twelve to a year) the request
    /// falls due: its date plus its months (FUN_0044d750 counts down to it).
    #[serde(default)]
    pub due: Option<i32>,
    /// Troop requests: how many months before the last month troops can set out
    /// and still arrive the first and last reminders come (record bytes 101, 102).
    #[serde(default)]
    pub reminder_leads: [u8; 2],
    /// Why this event happened, when another event led to it.
    cause: Option<(usize, Outcome)>,
    /// Months until a follow-up event fires (games saved before `fire_at`).
    pub wait: i32,
    /// The month (counted as `World::month_count`) a follow-up event fires in.
    #[serde(default)]
    pub fire_at: Option<i32>,
    /// Requests: the month the request opened, which doesn't count against it.
    #[serde(default)]
    pub opened: Option<i32>,
    /// Gifts: the answers the player may give now (`gift_choice` flags).
    #[serde(default)]
    pub choices: u8,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ScenarioEvents {
    pub list: Vec<ScenarioEvent>,
    pub start_year: i32,
    /// Days left of storms at sea and trouble on land routes.
    pub sea_problem_days: i32,
    pub land_problem_days: i32,
    /// Set while events are being run, so a follow-up waits for the pass to reach it.
    #[serde(skip)]
    busy: bool,
}

impl ScenarioEvents {
    /// The scenario's events, from a file of version `version`.
    pub fn from_records(records: &[EventRecord], start_year: i32, version: i32) -> Self {
        let list = records
            .iter()
            .map(|e| ScenarioEvent {
                kind: e.kind,
                trigger: e.trigger,
                subtype: e.subtype,
                pharaoh: e.sender == 1,
                month: e.month as i32,
                year: e.year as i32,
                // The year's pick (bytes 26-30): for a follow-up the months it waits,
                // for a recurring event the years between (FUN_0044bc20 re-rolls it).
                delay: (e.year_fixed, e.time.min, e.time.max),
                item: if version < 152 && item_is_resource(e.kind) { upgrade_item(e.item) } else { e.item }.into(),
                amount_pick: e.amount.into(),
                cities: (e.location[0], e.location[1]),
                location: Some(Pick { value: e.location[0], fixed: e.location[1], min: e.location[2], max: e.location[3] }),
                on_completed: e.on_completed,
                on_refusal: e.on_refusal,
                on_too_late: e.on_too_late,
                on_defeat: e.on_defeat,
                reminder_leads: [e.city as u8, e.defeat_link],
                god: e.god,
                months_initial: e.months as i32,
                ..Default::default()
            })
            .collect();
        Self { list, start_year, ..Default::default() }
    }

    /// Adds a troop request for `enemy` strength, open now (for tests and scripts).
    pub fn request_troops_now(&mut self, enemy: i32) -> usize {
        self.list.push(ScenarioEvent {
            kind: event::REQUEST,
            trigger: trigger::FIRED,
            subtype: 2,
            pharaoh: true,
            resource: TROOPS,
            amount: enemy,
            months_initial: 12,
            months_left: 12,
            state: state::IN_PROGRESS,
            active: true,
            on_completed: -1,
            on_refusal: -1,
            on_too_late: -1,
            on_defeat: -1,
            reminder_leads: [6, 1],
            ..Default::default()
        });
        self.list.len() - 1
    }

    /// Requests the player can still answer, for the Political Overseer.
    pub fn open_requests(&self) -> impl Iterator<Item = (usize, &ScenarioEvent)> {
        self.list.iter().enumerate().filter(|(_, e)| e.kind == event::REQUEST && e.active && e.state <= state::OVERDUE)
    }
}

/// The years between a recurring event's occurrences: the range its author set, or
/// the fixed year when no range is set.
pub fn repeat_interval(e: &EventRecord) -> (i16, i16) {
    if e.time.min < 0 && e.year_fixed >= 0 { (e.year_fixed, e.year_fixed) } else { (e.time.min, e.time.max) }
}

/// A pick among the scenario's choices (FUN_004487f0): with `fixed` set, it or one of
/// `min` and `max` that are set (`min` first); without, anything from `min` up to but
/// not including `max`.
fn pick(rng: &mut crate::rng::Rng, v: Pick) -> i32 {
    let (fixed, min, max) = (v.fixed as i32, v.min as i32, v.max as i32);
    if fixed < 0 {
        if min > -1 && max > -1 && max == min {
            return fixed;
        }
        let range = max - min;
        // The original divides by a range of nothing; no scenario sets one.
        return if range <= 0 { v.value as i32 } else { rng.below(range) + min };
    }
    if min < 0 {
        return fixed;
    }
    match rng.below(if max > -1 { 3 } else { 2 }) {
        0 => fixed,
        1 => min,
        _ => max,
    }
}

/// The phrase group a request's messages come from, by its reason.
fn request_group(subtype: i8) -> &'static str {
    match subtype {
        1 => "egyptian_city_attacked",
        2 => "distant_battle",
        3 => "great_festival",
        4 => "project",
        5 => "famine",
        6 => "threat",
        _ => "general_request",
    }
}

impl ScenarioEvent {
    fn side(&self) -> &'static str {
        if self.pharaoh { "P" } else { "C" }
    }

    /// Units the request is for: deben and troops as they are, goods in loads of 100
    /// (FUN_0044c660 and FUN_0044c1a0 multiply every goods amount).
    pub fn units(&self) -> i32 {
        if self.resource == DEBEN || self.resource == TROOPS { self.amount } else { self.amount * 100 }
    }
}

impl World {
    /// A pick among the scenario's choices (see `pick`).
    pub(crate) fn roll(&mut self, v: Pick) -> i32 {
        pick(&mut self.rng, v)
    }

    /// When the city starts, every event's year is picked from its choices, and a call
    /// for troops picks how many months before the last month to send them its two
    /// reminders come: four to eight, and none or one (FUN_00448a90, run on each event
    /// by FUN_004d5980). The picks come from their own generator, seeded from the
    /// map's, so the rest of the game draws what it did before.
    pub(crate) fn start_scenario_events(&mut self, seed: [u32; 2]) {
        let mut rng = crate::rng::Rng::from_seed(seed[1], seed[0]);
        for e in &mut self.scenario_events.list {
            let (fixed, min, max) = e.delay;
            let year = pick(&mut rng, Pick { value: e.year as i16, fixed, min, max });
            if matches!(e.trigger, trigger::ONCE | trigger::RECURRING) {
                e.year = year;
            }
            e.reminder_leads = [4 + rng.below(5) as u8, rng.below(2) as u8];
        }
    }

    fn years_since_start(&self) -> i32 {
        self.time.year - self.scenario_events.start_year
    }

    /// Months since the start of the scenario's first year.
    pub fn month_count(&self) -> i32 {
        self.years_since_start() * 12 + self.time.month as i32
    }

    /// The empire city with route number `n`.
    fn city_by_route(&self, n: i32) -> Option<usize> {
        self.trade.cities.iter().position(|c| c.route as i32 == n && n > 0)
    }

    /// The route number event `i` concerns this time. One that another event led to
    /// keeps that event's route when its own choices allow it: a single fixed route,
    /// or a range.
    fn event_route(&mut self, i: usize) -> i32 {
        let e = &self.scenario_events.list[i];
        let Some(loc) = e.location else {
            let (a, b) = e.cities;
            return if b > a { a as i32 + self.rng.below((b - a + 1) as i32) } else { a as i32 };
        };
        let parent = e.cause.and_then(|(p, _)| self.scenario_events.list.get(p)).map(|p| p.route);
        let allowed = if loc.fixed < 0 {
            Some((loc.min as i32, loc.max as i32))
        } else if loc.min < 0 {
            Some((loc.fixed as i32, loc.fixed as i32))
        } else {
            None
        };
        if let (Some(r), Some((lo, hi))) = (parent, allowed)
            && lo <= r
            && r <= hi
        {
            return r;
        }
        self.roll(loc)
    }

    /// Whether follow-up event `i` is due this month.
    fn follow_up_due(&mut self, i: usize) -> bool {
        let now = self.month_count();
        let e = &mut self.scenario_events.list[i];
        if e.trigger != trigger::FIRED || e.cause.is_none() {
            return false;
        }
        match e.fire_at {
            Some(at) => at <= now,
            None => {
                e.wait -= 1;
                e.wait < 0
            }
        }
    }

    /// Monthly (FUN_0044ba90): events whose time has come fire, and open requests move
    /// on. A request handles its first month when it opens, so its clock starts the
    /// month after. Follow-ups another event called on with no wait fire this month.
    pub(crate) fn process_scenario_events(&mut self) {
        let now = (self.years_since_start(), self.time.month as i32);
        self.scenario_events.busy = true;
        for i in 0..self.scenario_events.list.len() {
            let e = &self.scenario_events.list[i];
            let due = match e.trigger {
                trigger::ONCE | trigger::RECURRING => !e.active && (e.year, e.month) == now,
                _ => self.follow_up_due(i),
            };
            if due {
                self.fire_event(i);
            }
        }
        self.adopt_old_battle();
        let month = self.month_count();
        for i in 0..self.scenario_events.list.len() {
            let e = &self.scenario_events.list[i];
            if e.kind == event::GIFT && e.active && e.opened != Some(month) {
                self.update_gift(i);
                continue;
            }
            if e.kind != event::REQUEST {
                continue;
            }
            if e.resource == TROOPS {
                if e.active || self.military.battle_for(i).is_some() {
                    self.update_troop_request(i);
                }
            } else if e.active && e.opened != Some(month) {
                self.update_request(i);
            }
        }
        self.scenario_events.busy = false;
        self.fire_due_follow_ups();
    }

    /// Fires the follow-up events whose month has come, and those they lead to with no
    /// wait. Outside the monthly pass this is what makes a follow-up with no wait fire
    /// at once, as the original's FUN_0044c490 does.
    fn fire_due_follow_ups(&mut self) {
        if self.scenario_events.busy {
            return;
        }
        self.scenario_events.busy = true;
        // A chain of events that call on each other with no wait would never end.
        for _ in 0..self.scenario_events.list.len() * 2 {
            let Some(i) = (0..self.scenario_events.list.len()).find(|&i| self.scenario_events.list[i].fire_at.is_some() && self.follow_up_due(i)) else { break };
            self.fire_event(i);
        }
        self.scenario_events.busy = false;
    }

    /// The event `i` follows another (FUN_0044c490): it fires after the months its year
    /// picks, in this month when that is none. Only an event that waits for others
    /// follows, and never itself.
    pub(crate) fn follow(&mut self, i: i16, parent: usize, outcome: Outcome) {
        let Some(e) = usize::try_from(i).ok().filter(|&i| i != parent).and_then(|i| self.scenario_events.list.get(i)) else { return };
        if e.trigger != trigger::ONLY_VIA_EVENT && !(e.trigger == trigger::FIRED && e.cause.is_some()) {
            return;
        }
        let (fixed, min, max) = e.delay;
        let wait = self.roll(Pick { value: 0, fixed, min, max }).max(0);
        let (first, last) = (4 + self.rng.below(5) as u8, self.rng.below(2) as u8);
        let at = self.month_count() + wait;
        let e = &mut self.scenario_events.list[i as usize];
        e.trigger = trigger::FIRED;
        e.cause = Some((parent, outcome));
        e.wait = wait;
        e.fire_at = Some(at);
        e.reminder_leads = [first, last];
        self.fire_due_follow_ups();
    }

    /// Fires event `i` now, whatever its trigger (for scripted tests).
    pub fn fire_event_now(&mut self, i: usize) {
        if i < self.scenario_events.list.len() {
            self.fire_event(i);
        }
    }

    /// Runs event `i`'s effect and message.
    fn fire_event(&mut self, i: usize) {
        let now_month = self.month_count();
        let e = self.scenario_events.list[i].clone();
        let resource = self.roll(e.item).max(0) as u16;
        let amount = self.roll(e.amount_pick);
        let route = self.event_route(i);
        let city = self.city_by_route(route);
        {
            let ev = &mut self.scenario_events.list[i];
            ev.route = route;
            ev.resource = resource;
            ev.amount = amount;
            ev.city = city;
            if ev.trigger == trigger::ONCE {
                ev.trigger = trigger::FIRED;
            }
        }
        // A recurring event comes again the years its year picks after this one
        // (FUN_0044bc20); a request only once it has been met, and a gift once it is
        // accepted, refused or forfeit, counted from then.
        if e.trigger == trigger::RECURRING && e.kind != event::REQUEST && e.kind != event::GIFT {
            self.repeat_event(i);
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
                ev.due = Some(now_month + ev.months_initial);
                ev.active = true;
                ev.overdue = false;
                ev.can_comply_shown = false;
                ev.announced = false;
                ev.opened = Some(now_month);
                if resource != TROOPS {
                    self.open_request(i);
                }
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
                // The Kingdom's wage stays between 5 and 45 (as the city sees it, Ra's
                // oracle taking two off); a change past that is ignored.
                let raise = e.kind == event::WAGE_INCREASE;
                let wage = self.finance.kingdom_wages + if raise { amount } else { -amount };
                let ra = if self.complex_blessing(crate::temple_complex::RA, crate::temple_complex::ORACLE) { -2 } else { 0 };
                if (5..=45).contains(&(wage + ra)) {
                    self.finance.kingdom_wages = wage;
                    let f = if raise { up } else { down };
                    phrases = Some((f("wage_change_title"), f("wage_change_initial_announcement"), f("wage_change_no_reason") + "_A"));
                }
            }
            // However small the city (FUN_0044c660 has no check on its size).
            event::CONTAMINATED_WATER => {
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
            event::GOLD_MINE_COLLAPSE | event::CLAY_PIT_FLOOD => {
                let target = if e.kind == event::GOLD_MINE_COLLAPSE { kind::GOLD_MINE } else { kind::CLAY_PIT };
                let ids: Vec<u32> = self.buildings.iter().filter(|b| b.kind == target).map(|b| b.id).collect();
                if !ids.is_empty() {
                    let id = ids[self.rng.below(ids.len() as i32) as usize];
                    let tile = self.buildings.get(id).map(|b| (b.x, b.y));
                    self.wreck(id, false);
                    self.fx(crate::effects::Fx::Sound(crate::effects::CRASH));
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
                        // A year's quota steps between 0, 1500, 2500 and 4000; one
                        // already at the end of the way it moves stays.
                        let v = rt.limit[r] / 100;
                        let next = match (raise, v) {
                            (true, v) if v >= 40 => None,
                            (true, v) if v >= 25 => Some(40),
                            (true, v) if v >= 15 => Some(25),
                            (true, _) => Some(15),
                            (false, v) if v <= 0 => None,
                            (false, v) if v >= 26 => Some(25),
                            (false, v) if v >= 16 => Some(15),
                            (false, _) => Some(0),
                        };
                        if let Some(n) = next {
                            rt.limit[r] = n * 100;
                            changed = Some(c);
                        }
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
                    if raise {
                        *p = (p.0 + delta, p.1 + delta);
                    } else if p.1 > 0 {
                        // A cut to nothing leaves the good worth 2 to buy and 0 to sell.
                        *p = if amount < p.1 { (p.0 + delta, p.1 + delta) } else { (2, 0) };
                    }
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
                // The next flood is 30 to 100 points worse, in tens.
                let q = (3 + self.rng.below(8)) * 10;
                self.adjust_next_flood_quality(-q);
                phrases = Some(("flood_fails_title".into(), "flood_fails_initial_announcement".into(), "flood_fails_no_reason_A".into()));
            }
            event::PERFECT_FLOOD => {
                let q = (3 + self.rng.below(8)) * 10;
                self.adjust_next_flood_quality(q);
                phrases = Some(("perfect_flood_title".into(), "perfect_flood_initial_announcement".into(), "perfect_flood_no_reason_A".into()));
            }
            // The gift is offered, or waits for room (see `gift_look`).
            event::GIFT => {
                let ev = &mut self.scenario_events.list[i];
                ev.state = 0;
                ev.choices = 0;
                ev.active = true;
                ev.opened = Some(now_month);
                self.gift_look(i);
            }
            // Locusts will come before the next flood, as when Osiris sends them.
            event::LOCUSTS => {
                self.arm_locusts();
                popup_key = Some("message_plague_of_locusts");
            }
            // Invasions keep their own calendar; one another event led to sets out now.
            event::INVASION if e.trigger == trigger::FIRED => self.arm_invasion(i),
            // Frogs for as many months as the event's amount.
            event::FROGS => {
                self.plague_of_frogs(amount);
                popup_key = Some("message_plague_of_frogs");
            }
            event::HAILSTORM => {
                self.hailstorm();
                popup_key = Some("message_hailstorm");
            }
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
        // Everything but a request or a gift leads straight on to what follows it; an
        // invasion does when the army arrives.
        if !matches!(e.kind, event::REQUEST | event::INVASION | event::GIFT) {
            let on = self.scenario_events.list[i].on_completed;
            self.follow(on, i, Outcome::Completed);
        }
    }

    /// City status changes and messages about other cities. A change applies only to
    /// a city of the kinds it names, and says nothing otherwise: an Egyptian city
    /// falling becomes foreign (and keeps trading if it traded), a conquered foreign
    /// city becomes Egyptian, a new trade route makes a city a trading one, and a
    /// route shutting down or a trading city under siege stops its trade and closes
    /// its route until a new route is made. Messages change nothing.
    fn city_status_change(&mut self, e: &ScenarioEvent, city: Option<usize>) -> Option<(String, String, String)> {
        use osiris_formats::empire::city as ct;
        let names = |n: &str| Some((format!("{n}_title"), format!("{n}_initial_announcement"), format!("{n}_no_reason_A")));
        if e.kind == event::MESSAGE {
            return match e.subtype {
                0 => names("eg_city_saved"),
                1 => names("foreign_city_conquered"),
                2 => names("battle_lost"),
                3 => names("acknowledgement"),
                _ => None,
            };
        }
        let c = city?;
        let from = self.trade.cities[c].city_type;
        let to = match (e.subtype, from) {
            (0, ct::EGYPTIAN_TRADING) => ct::FOREIGN_TRADING,
            (0, ct::EGYPTIAN) => ct::FOREIGN,
            (1, ct::FOREIGN_TRADING) => ct::EGYPTIAN_TRADING,
            (1, ct::FOREIGN) => ct::EGYPTIAN,
            (2, ct::FOREIGN) => ct::FOREIGN_TRADING,
            (2, ct::EGYPTIAN) => ct::EGYPTIAN_TRADING,
            (2, ct::PHARAOH) => ct::PHARAOH_TRADING,
            (3 | 4, ct::FOREIGN_TRADING) => ct::FOREIGN,
            (3 | 4, ct::EGYPTIAN_TRADING) => ct::EGYPTIAN,
            (3 | 4, ct::PHARAOH_TRADING) => ct::PHARAOH,
            _ => return None,
        };
        self.change_trade_city(c, to, matches!(e.subtype, 3 | 4));
        names(match e.subtype {
            0 => "eg_city_falls",
            1 => "foreign_city_conquered",
            2 => "route_opened",
            3 => "route_closed",
            _ => "siege",
        })
    }

    /// The reason phrase for event `i` when an earlier event led to it: what happened
    /// with that event, in its own words.
    fn reason_phrase(&self, i: usize) -> Option<String> {
        let (parent, outcome) = self.scenario_events.list[i].cause?;
        let p = self.scenario_events.list.get(parent)?;
        if p.kind == event::GIFT {
            let what = match outcome {
                Outcome::Completed => "accepted",
                Outcome::Refused => "refused",
                Outcome::TooLate | Outcome::Defeat => "forfeited",
            };
            return Some(format!("gift_{what}_reason_{}_A", p.side()));
        }
        if p.kind != event::REQUEST {
            return None;
        }
        let what = match outcome {
            Outcome::Completed => "comply_reason",
            Outcome::TooLate => "too_late_reason",
            Outcome::Refused => "refuse_reason",
            Outcome::Defeat => "lost_battle_reason",
        };
        Some(format!("{}_{what}_{}_A", request_group(p.subtype), p.side()))
    }

    fn cause_of(&self, i: usize) -> Option<(u16, i32, Option<u8>)> {
        let (parent, _) = self.scenario_events.list[i].cause?;
        let p = self.scenario_events.list.get(parent)?;
        Some((p.resource, p.units(), p.city.map(|c| self.trade.cities[c].name_id)))
    }

    pub(crate) fn post_event_text(&mut self, text: EventText) {
        let key = if text.template == 130 { "message_template_request" } else { "message_template_general" };
        self.post(key, None, true);
        if let Some(n) = self.notices.log.last_mut() {
            n.text = Some(text.clone());
        }
        self.message_texts.push_back(text);
    }

    /// A goods request's message (`body` from its reason's phrases), with the months
    /// it has left.
    fn request_text(&self, i: usize, body: &str) -> EventText {
        let e = &self.scenario_events.list[i];
        let group = request_group(e.subtype);
        let side = e.side();
        let mut t = EventText {
            title: format!("{group}_title_{side}"),
            body: format!("{group}_{body}_{side}"),
            reason: format!("{group}_no_reason_{side}_A"),
            resource: e.resource,
            amount: e.units(),
            city_name: e.city.map(|c| self.trade.cities[c].name_id),
            months: e.months_left,
            template: 130,
            cause: None,
            army: None,
            god: (0..5).contains(&e.god).then_some(e.god as u8),
            gift: None,
        };
        if let Some(reason) = self.reason_phrase(i) {
            t.reason = reason;
            t.cause = self.cause_of(i);
        }
        t
    }

    /// Whether the city holds enough for request `i` to say it can be met: stock in
    /// the storage yards, or for deben the treasury.
    fn request_covered(&self, i: usize) -> bool {
        let e = &self.scenario_events.list[i];
        if e.resource == DEBEN { self.treasury >= e.amount } else { self.yards_stored(e.resource) >= e.units() }
    }

    /// A goods request opens (FUN_0044c660): the player hears of it at once, with all
    /// its months to go. A city that already holds enough isn't told again that it can.
    fn open_request(&mut self, i: usize) {
        self.scenario_events.list[i].state = state::IN_PROGRESS;
        self.scenario_events.list[i].announced = true;
        self.scenario_events.list[i].can_comply_shown = self.request_covered(i);
        let t = self.request_text(i, "initial_announcement");
        self.post_event_text(t);
    }

    /// Monthly, from the month after it opens (FUN_0044c660): a request's clock runs
    /// down; the player is reminded six months before the deadline. On it the Kingdom
    /// rating drops by 3 and 24 months of grace start, after which the request is
    /// refused. While it is open the player is told once when the city holds enough.
    /// Goods sent are on their way for a few months; when they arrive the event tied
    /// to sending them in time, or late, follows.
    fn update_request(&mut self, i: usize) {
        let e = &mut self.scenario_events.list[i];
        e.months_left -= 1;
        let left = e.months_left;
        match e.state {
            state::WAITING | state::IN_PROGRESS => {
                e.state = state::IN_PROGRESS;
                if left == 6 {
                    let t = self.request_text(i, "reminder");
                    self.post_event_text(t);
                } else if left < 1 {
                    let e = &mut self.scenario_events.list[i];
                    e.state = state::OVERDUE;
                    e.overdue = true;
                    e.months_left = GRACE_MONTHS;
                    let t = self.request_text(i, "overdue");
                    self.post_event_text(t);
                    self.ratings.change_kingdom(-3);
                }
            }
            state::OVERDUE => {
                if left < 1 {
                    e.state = state::FAILED;
                    e.active = false;
                    let next = e.on_refusal;
                    self.follow(next, i, Outcome::Refused);
                    return;
                }
            }
            state::SENT | state::SENT_LATE => {
                if left > 0 {
                    return;
                }
                let late = e.state == state::SENT_LATE;
                e.state = state::RECEIVED;
                e.active = false;
                let (next, outcome) = if late { (e.on_too_late, Outcome::TooLate) } else { (e.on_completed, Outcome::Completed) };
                let recurring = e.trigger == trigger::RECURRING;
                self.follow(next, i, outcome);
                if recurring {
                    self.repeat_event(i);
                }
                return;
            }
            _ => return,
        }
        if !self.scenario_events.list[i].can_comply_shown && self.request_covered(i) {
            self.scenario_events.list[i].can_comply_shown = true;
            let deben = self.scenario_events.list[i].resource == DEBEN;
            self.post(if deben { "message_compliance_now_possible" } else { "message_storage_yards_ready_to_fulfill_request" }, None, true);
        }
    }

    /// A recurring event sets its next year: the years its year picks after this one
    /// (FUN_0044bc20).
    fn repeat_event(&mut self, i: usize) {
        let (fixed, min, max) = self.scenario_events.list[i].delay;
        let years = self.roll(Pick { value: 1, fixed, min, max });
        let now = self.years_since_start();
        self.scenario_events.list[i].year = now + years;
    }

    /// Whether the city holds what request `i` asks for, as the Political Overseer
    /// checks before sending (FUN_0044d520): the goods in its storage yards, or more
    /// deben in the treasury than asked for.
    pub fn can_send_request(&self, i: usize) -> bool {
        let Some(e) = self.scenario_events.list.get(i) else { return false };
        match e.resource {
            DEBEN => self.treasury > e.amount,
            TROOPS => self.military.battle_for(i).is_none() && self.troops_ready(i),
            r => self.yards_stored(r) >= e.units(),
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

    /// The player sends what request `i` asks for (FUN_0044c1a0). Goods leave the
    /// storage yards and deben the treasury at once; they take one to four months to
    /// arrive.
    pub fn dispatch_request(&mut self, i: usize) -> bool {
        if !self.can_send_request(i) {
            return false;
        }
        let e = &self.scenario_events.list[i];
        if !(e.kind == event::REQUEST && e.active && e.state <= state::OVERDUE) {
            return false;
        }
        let (r, units, overdue) = (e.resource, e.units(), e.overdue);
        if r == TROOPS {
            // The troops march off; the request is settled by their battle.
            self.send_to_battle(i, e.amount);
            let e = &mut self.scenario_events.list[i];
            e.active = false;
            return true;
        }
        if r == DEBEN {
            self.treasury -= units;
        } else {
            let mut left = units;
            let yards: Vec<u32> = self.buildings.iter().filter(|b| b.kind == kind::STORAGE_YARD).map(|b| b.id).collect();
            for id in yards {
                if left <= 0 {
                    break;
                }
                left -= self.take_stored(id, r, left);
            }
        }
        let months = 1 + self.rng.below(4);
        let e = &mut self.scenario_events.list[i];
        e.state = if overdue { state::SENT_LATE } else { state::SENT };
        e.months_left = months;
        true
    }

    /// The months troops take to reach the city request `i` concerns, and to come
    /// back (FUN_0044c110): the length of the city's route on the Kingdom map over
    /// 400, and at least a month (FUN_004477d0).
    pub fn request_travel_months(&self, i: usize) -> i32 {
        let Some(e) = self.scenario_events.list.get(i) else { return 0 };
        let length = usize::try_from(e.route).ok().and_then(|r| self.trade.routes.get(r)).map_or(0, |r| r.length());
        (length / 400).max(1)
    }

    /// The months a call for troops gives to send them (FUN_0044c9a0): the months
    /// left, less the first reminder's lead and the journey, plus six, so the first
    /// reminder itself says six; at least one.
    pub fn troop_months_shown(&self, i: usize) -> i32 {
        let Some(e) = self.scenario_events.list.get(i) else { return 1 };
        let left = e.due.map_or(e.months_left, |d| d - self.month_count());
        let travel = match self.request_travel_months(i) {
            m if m < 1 => 12,
            m => m,
        };
        (left - e.reminder_leads[0] as i32 - travel + 6).max(1)
    }

    /// Monthly, for a request for troops (FUN_0044c9e0, which the request loop at
    /// 0x44cdc0 runs for resource 37 in place of the goods requests' rules). Months
    /// count down to the due month. Until troops are sent the player is told of the
    /// call, then reminded twice, each reminder a set number of months before the
    /// last month troops could leave and still get there. On the due month a call
    /// no one answered is refused; troops on the way fight if they are within two
    /// months of the battle, and otherwise are too late. Whatever the outcome, the
    /// event the scenario ties to it follows; this handler leaves the Kingdom rating
    /// alone.
    fn update_troop_request(&mut self, i: usize) {
        let now = self.month_count();
        let e = &mut self.scenario_events.list[i];
        let due = *e.due.get_or_insert(now + e.months_left.max(0));
        let left = due - now;
        e.months_left = left.max(0);
        let travel = match self.request_travel_months(i) {
            m if m < 1 => 12,
            m => m,
        };
        let sent = self.military.battle_for(i).is_some();
        if left == 0 {
            let e = &mut self.scenario_events.list[i];
            if !sent {
                // Refused: no troops went.
                if e.active && e.state <= state::OVERDUE {
                    e.state = state::FAILED;
                    e.active = false;
                    let next = e.on_refusal;
                    self.follow(next, i, Outcome::Refused);
                }
                return;
            }
            let (next, outcome) = match self.distant_battle_due(i) {
                None => (self.scenario_events.list[i].on_too_late, Outcome::TooLate),
                Some(true) => (self.scenario_events.list[i].on_completed, Outcome::Completed),
                Some(false) => (self.scenario_events.list[i].on_defeat, Outcome::Defeat),
            };
            let e = &mut self.scenario_events.list[i];
            e.active = false;
            e.state = if outcome == Outcome::Completed { state::RECEIVED } else { state::FAILED };
            self.follow(next, i, outcome);
            return;
        }
        if sent {
            self.distant_battle_month(i);
            return;
        }
        let e = &mut self.scenario_events.list[i];
        if !e.active {
            return;
        }
        if e.state == state::WAITING {
            e.state = state::IN_PROGRESS;
        }
        let lead = e.reminder_leads;
        let shown = self.troop_months_shown(i);
        let e = &mut self.scenario_events.list[i];
        let body = if !e.announced {
            e.announced = true;
            Some("initial_announcement")
        } else if left - travel == lead[0] as i32 {
            Some("first_reminder")
        } else if left - travel == lead[1] as i32 {
            Some("last_reminder")
        } else {
            None
        };
        let Some(body) = body else { return };
        let e = &self.scenario_events.list[i];
        // Egyptian cities under attack have their own phrases and picture; every other
        // call for troops reads as a distant battle.
        let group = if e.subtype == 1 { "egyptian_city_attacked" } else { "distant_battle" };
        let side = e.side();
        let mut t = EventText {
            title: format!("{group}_title_{side}"),
            body: format!("{group}_{body}_{side}"),
            reason: format!("{}_no_reason_{side}_A", request_group(e.subtype)),
            resource: e.resource,
            amount: e.units(),
            city_name: e.city.map(|c| self.trade.cities[c].name_id),
            months: shown,
            template: match body {
                "initial_announcement" if e.subtype == 1 => 266,
                "initial_announcement" => 272,
                _ => 130,
            },
            cause: None,
            army: None,
            god: None,
            gift: None,
        };
        if let Some(reason) = self.reason_phrase(i) {
            t.reason = reason;
            t.cause = self.cause_of(i);
        }
        self.post_event_text(t);
    }

    /// The buildings a gift of `r` goes to, in the order it fills them: storage
    /// yards, and for food granaries too (FUN_0045afe0, types 0x48 and 0x47).
    fn gift_stores(&self, r: u16) -> Vec<u32> {
        self.buildings.iter().filter(|b| b.kind == kind::STORAGE_YARD || resource::is_food(r) && b.kind == kind::GRANARY).map(|b| b.id).collect()
    }

    /// Room for `r` in the buildings a gift goes to, in units.
    fn gift_room(&self, r: u16) -> i32 {
        self.gift_stores(r).into_iter().map(|id| self.storage_room(id, r)).sum()
    }

    /// Puts `units` of `r` into storage yards, and food into granaries too, as far
    /// as there is room (FUN_0045afe0); returns what did not fit.
    fn deliver_to_storage(&mut self, r: u16, units: i32) -> i32 {
        let mut left = units;
        for id in self.gift_stores(r) {
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

    /// A gift's message (template 132): its title and `body`, both by who sends it,
    /// offering `choices`.
    fn gift_text(&self, i: usize, body: &str, units: i32, choices: u8) -> EventText {
        let e = &self.scenario_events.list[i];
        let side = e.side();
        let mut t = EventText {
            title: format!("gift_title_{side}"),
            body: format!("{body}_{side}"),
            reason: format!("gift_no_reason_{side}_A"),
            resource: e.resource,
            amount: units,
            city_name: e.city.map(|c| self.trade.cities[c].name_id),
            template: 132,
            gift: (choices != 0).then_some(i),
            ..Default::default()
        };
        if let Some(reason) = self.reason_phrase(i) {
            t.reason = reason;
            t.cause = self.cause_of(i);
        }
        t
    }

    /// Gift `i` looks at the room for it (the gift branch of FUN_0044c660, at
    /// 0x44e890): with room for all of it the player may accept or refuse it; with
    /// room for at least half, also postpone it; with less it waits a month and looks
    /// again. A deben gift always has room. The first look comes when it fires; on
    /// the third, a gift with too little room is forfeit and an offer can no longer
    /// wait.
    fn gift_look(&mut self, i: usize) {
        use gift_choice::*;
        use gift_state::*;
        let e = &self.scenario_events.list[i];
        let (state, amount, resource) = (e.state, e.amount, e.resource);
        let room = if resource == DEBEN { amount } else { self.gift_room(resource) / 100 };
        let last = state == LAST_TRY;
        let (body, choices) = if room >= amount {
            (if resource == DEBEN { "gift_cash_granted" } else { "gift_granted" }, ACCEPT | REFUSE)
        } else if room >= amount / 2 {
            (if last { "gift_last_chance" } else { "gift_partial_space" }, ACCEPT | REFUSE | POSTPONE)
        } else {
            (if last { "gift_forfeited" } else { "gift_insufficient_space" }, 0)
        };
        let offered = choices != 0;
        let next = match state {
            WAITING => if offered { OFFERED_AGAIN } else { LAST_TRY },
            LAST_TRY => if offered { LAST_CHANCE } else { FORFEIT },
            _ => if offered { OFFERED } else { WAITING },
        };
        let e = &mut self.scenario_events.list[i];
        e.state = next;
        e.choices = choices;
        let units = if resource == DEBEN { amount } else { amount * 100 };
        let t = self.gift_text(i, body, units, choices);
        self.post_event_text(t);
    }

    /// Monthly, from the month after it fires: a gift waiting for room looks again,
    /// and a forfeit one leads on to the event for that. An offer waits for the
    /// player's answer however long that takes.
    fn update_gift(&mut self, i: usize) {
        match self.scenario_events.list[i].state {
            gift_state::WAITING | gift_state::LAST_TRY => self.gift_look(i),
            // The original says again here that the gift is forfeit; once is enough.
            gift_state::FORFEIT => self.finish_gift(i, Outcome::TooLate),
            _ => {}
        }
    }

    /// The gift event whose offer is waiting for the player's answer, if `t` is one.
    pub fn gift_offer(&self, t: &EventText) -> Option<(usize, u8)> {
        let i = t.gift?;
        let e = self.scenario_events.list.get(i)?;
        (e.kind == event::GIFT && e.active && e.choices != 0).then_some((i, e.choices))
    }

    /// The player's answer to gift `i`'s offer (a `gift_choice`; the message's
    /// buttons, at 0x44ee30, 0x44eea0 and 0x44eef0). Accepting puts what fits of the
    /// goods into storage, or the deben into the treasury; refusing turns the gift
    /// away; postponing has it look again next month, or forfeits it on its last
    /// chance. False when the gift isn't offering that.
    pub fn answer_gift(&mut self, i: usize, choice: u8) -> bool {
        use gift_choice::*;
        let Some(e) = self.scenario_events.list.get(i) else { return false };
        if e.kind != event::GIFT || !e.active || e.choices & choice == 0 || choice.count_ones() != 1 {
            return false;
        }
        let (state, amount, resource) = (e.state, e.amount, e.resource);
        self.scenario_events.list[i].choices = 0;
        match choice {
            ACCEPT => {
                let t = if resource == DEBEN {
                    self.treasury += amount;
                    self.finance.this_year.donated += amount;
                    self.gift_text(i, "gift_cash_accepted", amount, 0)
                } else {
                    let units = amount * 100;
                    let given = units - self.deliver_to_storage(resource, units);
                    self.gift_text(i, "gift_accepted", given, 0)
                };
                self.post_event_text(t);
                self.finish_gift(i, Outcome::Completed);
            }
            REFUSE => {
                let t = self.gift_text(i, "gift_refused", amount * 100, 0);
                self.post_event_text(t);
                self.finish_gift(i, Outcome::Refused);
            }
            _ => match state {
                gift_state::OFFERED => self.scenario_events.list[i].state = gift_state::WAITING,
                gift_state::OFFERED_AGAIN => self.scenario_events.list[i].state = gift_state::LAST_TRY,
                _ => {
                    let t = self.gift_text(i, "gift_forfeited", amount * 100, 0);
                    self.post_event_text(t);
                    self.finish_gift(i, Outcome::TooLate);
                }
            },
        }
        true
    }

    /// Gift `i` is over: the event the scenario ties to how it ended follows, and a
    /// recurring gift sets the year it comes again (FUN_0044bc20).
    fn finish_gift(&mut self, i: usize, outcome: Outcome) {
        let e = &mut self.scenario_events.list[i];
        e.state = gift_state::DONE;
        e.choices = 0;
        e.active = false;
        let next = match outcome {
            Outcome::Completed => e.on_completed,
            Outcome::Refused => e.on_refusal,
            _ => e.on_too_late,
        };
        let recurring = e.trigger == trigger::RECURRING;
        self.follow(next, i, outcome);
        if recurring {
            self.repeat_event(i);
        }
    }

    /// Daily: storms and sandstorms blow over.
    pub(crate) fn update_trade_problems(&mut self) {
        let s = &mut self.scenario_events;
        s.sea_problem_days = (s.sea_problem_days - 1).max(0);
        s.land_problem_days = (s.land_problem_days - 1).max(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economy::tests::mission;

    /// A request for `amount` of `r` with `months` to send it, waiting to be fired.
    fn add_request(world: &mut World, r: i16, amount: i16, months: i32) -> usize {
        world.scenario_events.list.push(ScenarioEvent {
            kind: event::REQUEST,
            trigger: trigger::ONLY_VIA_EVENT,
            item: Pick { value: r, fixed: r, min: -1, max: -1 },
            amount_pick: Pick { value: amount, fixed: amount, min: -1, max: -1 },
            months_initial: months,
            on_completed: -1,
            on_refusal: -1,
            on_too_late: -1,
            on_defeat: -1,
            ..Default::default()
        });
        world.scenario_events.list.len() - 1
    }

    /// An event that raises the Kingdom rating by `amount` when another leads to it.
    fn add_reward(world: &mut World, amount: i16) -> usize {
        world.scenario_events.list.push(ScenarioEvent {
            kind: event::REPUTATION_INCREASE,
            trigger: trigger::ONLY_VIA_EVENT,
            amount_pick: Pick { value: amount, fixed: amount, min: -1, max: -1 },
            delay: (0, -1, -1),
            on_completed: -1,
            ..Default::default()
        });
        world.scenario_events.list.len() - 1
    }

    /// Runs the monthly event pass for the next month, without the rest of the month.
    fn next_month(world: &mut World) {
        world.time.month += 1;
        if world.time.month >= 12 {
            world.time.month = 0;
            world.time.year += 1;
        }
        world.process_scenario_events();
    }

    #[test]
    fn picks_follow_the_original() {
        let mut rng = crate::rng::Rng::from_seed(1234, 5678);
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..200 {
            seen.insert(pick(&mut rng, Pick { value: 0, fixed: 1, min: 4, max: -1 }));
            assert_eq!(pick(&mut rng, Pick { value: 0, fixed: 5, min: -1, max: 9 }), 5);
            assert_eq!(pick(&mut rng, Pick { value: 0, fixed: -1, min: 2, max: 3 }), 2, "a range stops short of its end");
            assert!((8..12).contains(&pick(&mut rng, Pick { value: 0, fixed: -1, min: 8, max: 12 })));
        }
        assert_eq!(seen.into_iter().collect::<Vec<_>>(), vec![1, 4], "a fixed value and a minimum are two choices");
    }

    #[test]
    fn old_files_number_deben_and_troops_after_cleopatras_goods() {
        let v = |fixed, min, max| EventValue { value: fixed, fixed, min, max };
        assert_eq!(upgrade_item(v(31, -1, -1)), v(36, -1, -1));
        assert_eq!(upgrade_item(EventValue { value: 3, fixed: -1, min: 31, max: 32 }), EventValue { value: 3, fixed: 36, min: 37, max: -1 });
        assert_eq!(upgrade_item(v(1, -1, 4)), EventValue { value: 1, fixed: 1, min: 4, max: -1 });
        // Version 147 and 149 campaign missions: a call for troops, and deben.
        if let Some(mut world) = mission(8) {
            world.fire_event_now(8);
            assert_eq!(world.scenario_events.list[8].resource, TROOPS);
        }
        if let Some(mut world) = mission(14) {
            world.fire_event_now(2);
            assert_eq!(world.scenario_events.list[2].resource, DEBEN);
            assert_eq!(world.scenario_events.list[2].units(), 500);
        }
    }

    /// FUN_0044c660 and FUN_0044c1a0: a request is told with all its months to go, the
    /// reminder comes at six, sending costs or earns no rating, and what follows waits
    /// until the goods arrive.
    #[test]
    fn requests_run_on_the_originals_clock() {
        let Some(mut world) = mission(14) else { return };
        let i = add_request(&mut world, DEBEN as i16, 500, 12);
        let reward = add_reward(&mut world, 4);
        world.scenario_events.list[i].on_completed = reward as i16;
        world.treasury = 100;
        let before = world.notices.log.len();
        world.fire_event_now(i);
        let told: Vec<i32> = world.notices.log[before..].iter().filter_map(|n| n.text.as_ref()).map(|t| t.months).collect();
        assert_eq!(told, vec![12]);
        next_month(&mut world);
        assert_eq!(world.scenario_events.list[i].months_left, 11);
        for _ in 0..5 {
            next_month(&mut world);
        }
        assert!(world.notices.log.iter().filter_map(|n| n.text.as_ref()).any(|t| t.body.ends_with("_reminder_C") && t.months == 6));
        // Short of deben, then exactly enough: the treasury must hold more than asked.
        assert!(!world.can_send_request(i));
        world.treasury = 500;
        assert!(!world.can_send_request(i));
        world.treasury = 1000;
        let kingdom = world.ratings.kingdom;
        assert!(world.dispatch_request(i));
        assert_eq!(world.treasury, 500);
        assert_eq!(world.ratings.kingdom, kingdom);
        assert_eq!(world.scenario_events.open_requests().count(), 0);
        let mut months = 0;
        while world.scenario_events.list[i].state != state::RECEIVED {
            assert_eq!(world.ratings.kingdom, kingdom);
            next_month(&mut world);
            months += 1;
            assert!(months <= 4, "goods arrive within four months");
        }
        assert_eq!(world.ratings.kingdom, kingdom + 4, "only the event the scenario ties to it rewards the city");
    }

    /// Missing the deadline costs 3 once; letting the 24 months of grace pass too
    /// costs nothing more and refuses the request, and what is tied to refusing it
    /// follows in that month.
    #[test]
    fn missed_requests_cost_three() {
        let Some(mut world) = mission(14) else { return };
        let reward = add_reward(&mut world, 2);
        let i = add_request(&mut world, 13, 5, 3);
        world.scenario_events.list[i].on_refusal = reward as i16;
        world.fire_event_now(i);
        let kingdom = world.ratings.kingdom;
        for _ in 0..3 {
            next_month(&mut world);
        }
        assert_eq!(world.scenario_events.list[i].state, state::OVERDUE);
        assert_eq!(world.ratings.kingdom, kingdom - 3);
        for _ in 0..23 {
            next_month(&mut world);
        }
        assert_eq!(world.scenario_events.list[i].state, state::OVERDUE);
        next_month(&mut world);
        assert_eq!(world.scenario_events.list[i].state, state::FAILED);
        assert_eq!(world.ratings.kingdom, kingdom - 3 + 2, "the follow-up, which comes before it in the list, fires at once");
    }

    /// Requests are met from the storage yards only; grain in a granary doesn't count
    /// (FUN_0045aae0, FUN_0045b4d0).
    #[test]
    fn granaries_dont_meet_requests() {
        use crate::world::{Command, Outcome};
        let Some(mut world) = mission(4) else { return };
        assert!(matches!(world.apply(&Command::Build { kind: kind::GRANARY, x: 100, y: 70, x1: 100, y1: 70 }), Outcome::Done { .. }));
        let granary = world.buildings.iter().find(|b| b.kind == kind::GRANARY).expect("granary").id;
        world.buildings.get_mut(granary).unwrap().stock[resource::GRAIN as usize] = 2000;
        let i = add_request(&mut world, resource::GRAIN as i16, 10, 12);
        world.fire_event_now(i);
        assert_eq!(world.scenario_events.list[i].units(), 1000);
        assert!(!world.can_send_request(i));
    }

    /// A recurring request comes again only once it has been met, the years its year
    /// picks after the year it was met.
    #[test]
    fn recurring_requests_wait_until_met() {
        let Some(mut world) = mission(14) else { return };
        let i = add_request(&mut world, DEBEN as i16, 5, 12);
        let now = world.years_since_start();
        let e = &mut world.scenario_events.list[i];
        e.trigger = trigger::RECURRING;
        e.delay = (1, -1, -1);
        e.year = now;
        e.month = world.time.month as i32;
        world.fire_event_now(i);
        assert_eq!(world.scenario_events.list[i].year, now, "not yet");
        world.treasury = 1000;
        for _ in 0..13 {
            next_month(&mut world);
        }
        assert!(world.scenario_events.list[i].active, "still open when its month comes round");
        assert!(world.dispatch_request(i));
        while world.scenario_events.list[i].state != state::RECEIVED {
            next_month(&mut world);
        }
        assert_eq!(world.scenario_events.list[i].year, world.years_since_start() + 1);
    }

    /// A gift of `loads` of `r` from Pharaoh, waiting to be fired, with a reward of
    /// 2, 3 or 4 Kingdom rating tied to accepting, refusing or forfeiting it.
    fn add_gift(world: &mut World, r: u16, loads: i16) -> usize {
        let rewards = [2, 3, 4].map(|n| add_reward(world, n) as i16);
        world.scenario_events.list.push(ScenarioEvent {
            kind: event::GIFT,
            trigger: trigger::ONLY_VIA_EVENT,
            pharaoh: true,
            item: Pick { value: r as i16, fixed: r as i16, min: -1, max: -1 },
            amount_pick: Pick { value: loads, fixed: loads, min: -1, max: -1 },
            on_completed: rewards[0],
            on_refusal: rewards[1],
            on_too_late: rewards[2],
            on_defeat: -1,
            ..Default::default()
        });
        world.scenario_events.list.len() - 1
    }

    /// A staffed storage yard with `full` of its eight spaces full of something else.
    fn yard(world: &mut World, full: usize) -> u32 {
        let id = world.create_building(kind::STORAGE_YARD, 60, 60);
        let b = world.buildings.get_mut(id).unwrap();
        b.road = Some((60, 59));
        b.workers = 6;
        for s in b.spaces.iter_mut().take(full) {
            *s = (12, 400);
        }
        id
    }

    /// The gift's last message.
    fn last_gift_text(world: &World) -> EventText {
        world.notices.log.iter().rev().filter_map(|n| n.text.clone()).find(|t| t.title.starts_with("gift_")).unwrap_or_default()
    }

    fn last_body(world: &World) -> String {
        last_gift_text(world).body
    }

    const BRICKS: u16 = 13;

    /// With no room the gift waits, looks again each month, and on the third look is
    /// forfeit; what is tied to forfeiting it follows the month after.
    #[test]
    fn gifts_with_full_yards_wait_then_are_forfeit() {
        let Some(mut world) = mission(14) else { return };
        let y = yard(&mut world, 8);
        let i = add_gift(&mut world, BRICKS, 10);
        let kingdom = world.ratings.kingdom;
        world.fire_event_now(i);
        assert_eq!(last_body(&world), "gift_insufficient_space_P");
        assert_eq!((world.scenario_events.list[i].state, world.scenario_events.list[i].choices), (gift_state::WAITING, 0));
        assert!(!world.answer_gift(i, gift_choice::ACCEPT), "nothing to accept yet");
        next_month(&mut world);
        assert_eq!(world.scenario_events.list[i].state, gift_state::LAST_TRY);
        assert_eq!(last_body(&world), "gift_insufficient_space_P");
        next_month(&mut world);
        assert_eq!(world.scenario_events.list[i].state, gift_state::FORFEIT);
        assert_eq!(last_body(&world), "gift_forfeited_P");
        assert_eq!(world.ratings.kingdom, kingdom);
        next_month(&mut world);
        assert_eq!(world.scenario_events.list[i].state, gift_state::DONE);
        assert!(!world.scenario_events.list[i].active);
        assert_eq!(world.ratings.kingdom, kingdom + 4);
        assert_eq!(world.stored(y, BRICKS), 0);
    }

    /// Accepted once room is made: room for all of it offers Accept and Refuse, and
    /// accepting fills the yard. Room for half also offers Postpone, which waits a
    /// month; postponing the last chance forfeits it.
    #[test]
    fn gifts_are_accepted_once_there_is_room() {
        let Some(mut world) = mission(14) else { return };
        let y = yard(&mut world, 8);
        let i = add_gift(&mut world, BRICKS, 10);
        let kingdom = world.ratings.kingdom;
        world.fire_event_now(i);
        world.buildings.get_mut(y).unwrap().spaces[..3].fill((0, 0));
        next_month(&mut world);
        assert_eq!(last_body(&world), "gift_granted_P");
        let offer = last_gift_text(&world);
        assert_eq!(world.gift_offer(&offer), Some((i, gift_choice::ACCEPT | gift_choice::REFUSE)));
        assert!(!world.answer_gift(i, gift_choice::POSTPONE));
        // Nothing comes of waiting for an answer.
        next_month(&mut world);
        assert_eq!(world.scenario_events.list[i].state, gift_state::OFFERED_AGAIN);
        assert_eq!(world.apply(&crate::world::Command::AnswerGift { event: i, choice: gift_choice::ACCEPT }), crate::world::Outcome::Done { items: 1, cost: 0 });
        assert_eq!(world.stored(y, BRICKS), 1000);
        assert_eq!(last_body(&world), "gift_accepted_P");
        assert_eq!(world.ratings.kingdom, kingdom + 2);
        assert_eq!(world.gift_offer(&offer), None, "answered");

        // Room for 8 of 10 loads: postponed once, then accepted in part on the last look.
        let mut world = mission(14).unwrap();
        let y = yard(&mut world, 6);
        let i = add_gift(&mut world, BRICKS, 10);
        world.fire_event_now(i);
        assert_eq!(last_body(&world), "gift_partial_space_P");
        assert!(world.answer_gift(i, gift_choice::POSTPONE));
        assert_eq!(world.scenario_events.list[i].state, gift_state::WAITING);
        next_month(&mut world);
        assert_eq!(world.scenario_events.list[i].choices, 7);
        assert!(world.answer_gift(i, gift_choice::POSTPONE));
        next_month(&mut world);
        assert_eq!(last_body(&world), "gift_last_chance_P");
        assert!(world.answer_gift(i, gift_choice::ACCEPT));
        assert_eq!(world.stored(y, BRICKS), 800, "what fits");
        assert_eq!(last_gift_text(&world).amount, 800);

        // Postponing the last chance forfeits the gift at once.
        let mut world = mission(14).unwrap();
        yard(&mut world, 6);
        let i = add_gift(&mut world, BRICKS, 10);
        let kingdom = world.ratings.kingdom;
        world.fire_event_now(i);
        for _ in 0..2 {
            assert!(world.answer_gift(i, gift_choice::POSTPONE));
            next_month(&mut world);
        }
        assert!(world.answer_gift(i, gift_choice::POSTPONE));
        assert_eq!(last_body(&world), "gift_forfeited_P");
        assert_eq!(world.ratings.kingdom, kingdom + 4);
    }

    /// Refused, nothing is delivered and the event tied to refusing it follows; a
    /// deben gift accepted goes into the treasury.
    #[test]
    fn gifts_can_be_refused() {
        let Some(mut world) = mission(14) else { return };
        let y = yard(&mut world, 0);
        let i = add_gift(&mut world, BRICKS, 10);
        let kingdom = world.ratings.kingdom;
        world.fire_event_now(i);
        assert_eq!(last_body(&world), "gift_granted_P");
        assert!(world.answer_gift(i, gift_choice::REFUSE));
        assert_eq!(last_body(&world), "gift_refused_P");
        assert_eq!(world.stored(y, BRICKS), 0);
        assert_eq!(world.ratings.kingdom, kingdom + 3);
        assert!(!world.answer_gift(i, gift_choice::ACCEPT), "once only");

        let i = add_gift(&mut world, DEBEN, 500);
        world.treasury = 100;
        world.fire_event_now(i);
        assert_eq!(last_body(&world), "gift_cash_granted_P");
        assert_eq!(world.treasury, 100, "not until accepted");
        assert!(world.answer_gift(i, gift_choice::ACCEPT));
        assert_eq!(world.treasury, 600);
        assert_eq!(last_body(&world), "gift_cash_accepted_P");
    }
}
