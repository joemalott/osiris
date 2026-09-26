//! The editor's events: the Event Summary (Options > Events, the original's window
//! 0x27, FUN_00448640) listing the scenario's events with Add event and Delete event,
//! and the event planning window (window 0x18, FUN_0044aaf0) that sets every field of
//! one event, showing only the buttons its type uses. Layouts, texts and the rules
//! for each button are the original's (button tables at 0x5d9a90 and 0x5d9e20, click
//! handlers 0x448560-0x44aa30); notes/editor.md has them. Values are picked from the
//! Options screen's lists or typed on its keypad.

use super::Editor;
use super::options::{Field, Keypad, Options, Page, Picker};
use crate::widgets::{Ui, inside};
use osiris_formats::{EventRecord, EventValue, Scenario};
use osiris_ui::{Font, panel};

/// The editor adds up to 120 events (FUN_00448d10 refuses past 0x77); the game
/// itself may add more while it runs.
pub const MAX_EDITOR_EVENTS: usize = 120;

/// Summary rows shown at once.
const ROWS: usize = 12;

/// Event types (text group 156).
pub mod kind {
    pub const REQUEST: u8 = 1;
    pub const INVASION: u8 = 2;
    pub const EARTHQUAKE: u8 = 3;
    pub const CITY_STATUS: u8 = 19;
    pub const MESSAGE: u8 = 20;
    pub const GIFT: u8 = 23;
}

/// Trigger flags (byte 44): none is a one-time event.
pub mod trigger {
    pub const ONCE: u8 = 0;
    pub const TRIGGERED: u8 = 1;
    pub const RECURRING: u8 = 2;
    pub const FAVOUR: u8 = 0x10;
}

// Per type, what the planning window shows (tables at 0x5d9c18 ... 0x5d9e00).
/// The "cities" (or "from markers") range.
const LOCATION: [u8; 30] = [0, 1, 1, 1, 1, 0, 1, 1, 1, 1, 0, 0, 0, 1, 1, 0, 0, 1, 1, 1, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0];
/// The range is of invasion points ("from markers") rather than cities.
const FROM_MARKERS: [u8; 30] = [0, 0, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
/// The three items.
const ITEMS: [u8; 30] = [0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
/// The items' ids run from here ...
const ITEM_MIN: [i16; 30] = [0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
/// ... to before here ...
const ITEM_END: [i16; 30] = [0, 38, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 36, 36, 36, 36, 0, 0, 0, 0, 0, 0, 38, 0, 0, 0, 0, 0, 0];
/// ... and are named by this text group: resources (23) or invaders (34).
const ITEM_GROUP: [i16; 30] = [-1, 23, 34, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, 23, 23, 23, 23, -1, -1, -1, -1, -1, -1, 23, -1, -1, -1, -1, -1, -1];
/// The amount range.
const AMOUNT: [u8; 30] = [0, 1, 1, 1, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 1, 1, 1, 1, 0, 0, 0, 0, 1, 0, 1, 0, 1, 1, 1];

fn has(table: &[u8; 30], k: u8) -> bool {
    table.get(k as usize).is_some_and(|&v| v != 0)
}

/// What a planning window button sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvField {
    Kind,
    Month,
    /// A request's reason, a city status change or a message.
    Subtype,
    God,
    Item(usize),
    /// The reason given for the follow-up of outcome n (comply, refuse, too late, lose
    /// battle).
    LinkReason(usize),
    /// The low (false) or high (true) end of a range.
    Year(bool),
    Amount(bool),
    Location(bool),
    Route(bool),
    Months,
    Warships,
    Link(usize),
}

/// The two ends a pick shows (FUN_0044aaf0): a range's, a fixed value's twice, or
/// neither when it offers three choices.
pub fn ends(v: EventValue) -> (i16, i16) {
    if v.fixed < 0 {
        (v.min, v.max)
    } else if v.min < 0 {
        (v.fixed, v.fixed)
    } else {
        (-1, -1)
    }
}

/// One end of a pick typed on the keypad (0x449c90 and its twins): the ends put in
/// order, the same twice making a fixed value.
pub fn set_end(v: &mut EventValue, high: bool, typed: i16) {
    let (lo, hi) = ends(*v);
    let fixed = |v: &mut EventValue, n: i16| *v = EventValue { fixed: n, min: -1, max: -1, ..*v };
    let range = |v: &mut EventValue, a: i16, b: i16| *v = EventValue { fixed: -1, min: a.min(b), max: a.max(b), ..*v };
    if high {
        let lo = lo.min(typed);
        if lo < 0 || lo == typed {
            fixed(v, typed);
        } else if typed < 0 {
            fixed(v, lo);
        } else {
            range(v, lo, typed);
        }
    } else {
        let hi = hi.max(typed);
        if typed < 0 || typed == hi {
            fixed(v, hi);
        } else if hi < 0 {
            fixed(v, typed);
        } else {
            range(v, typed, hi);
        }
    }
}

/// The year's pick (bytes 24-30).
fn year_pick(e: &EventRecord) -> EventValue {
    EventValue { value: e.year, fixed: e.year_fixed, min: e.time.min, max: e.time.max }
}

fn set_year_pick(e: &mut EventRecord, v: EventValue) {
    e.year_fixed = v.fixed;
    e.time.min = v.min;
    e.time.max = v.max;
}

fn location_pick(e: &EventRecord) -> EventValue {
    EventValue { value: e.location[0], fixed: e.location[1], min: e.location[2], max: e.location[3] }
}

fn route_pick(e: &EventRecord) -> EventValue {
    EventValue { value: e.route[0], fixed: e.route[1], min: e.route[2], max: e.route[3] }
}

/// The three items an event may pick among (bytes 10-14).
fn items(e: &EventRecord) -> [i16; 3] {
    [e.item.fixed, e.item.min, e.item.max]
}

/// The items with the unset ones moved to the end (as 0x449a69 and 0x44a940 leave
/// them).
fn packed(mut it: [i16; 3]) -> [i16; 3] {
    if it[0] < 0 {
        if it[1] >= 0 {
            it[0] = it[1];
            it[1] = -1;
        } else if it[2] >= 0 {
            it[0] = it[2];
            it[2] = -1;
        }
    }
    if it[1] < 0 && it[2] >= 0 {
        it[1] = it[2];
        it[2] = -1;
    }
    it
}

fn set_items(e: &mut EventRecord, it: [i16; 3]) {
    let [a, b, c] = packed(it);
    e.item.fixed = a;
    e.item.min = b;
    e.item.max = c;
}

/// Whether item `id` suits event `e` (FUN_00449550): in its type's range, and for a
/// request, fitting its reason (FUN_00449440): troops when a city is attacked or a
/// battle is fought, foods and the festival goods for a festival, building goods for
/// construction, foods for a famine, anything else otherwise; never gold or the
/// unused good for demand and price changes, never troops for a gift.
pub fn item_fits(e: &EventRecord, id: i16) -> bool {
    let k = e.kind as usize;
    if k >= 30 || id < ITEM_MIN[k] || id >= ITEM_END[k] {
        return false;
    }
    match e.kind {
        kind::REQUEST => match e.subtype {
            1 | 2 => id == 37,
            3 => matches!(id, 1..=8 | 15 | 17 | 19 | 36),
            4 => matches!(id, 12 | 20 | 24 | 25 | 26 | 30 | 36),
            5 => (1..=8).contains(&id),
            _ => !matches!(id, 37 | 21 | 27),
        },
        kind::GIFT => !matches!(id, 37 | 27),
        13..=16 => !matches!(id, 27 | 21),
        _ => true,
    }
}

/// The items event `e` may be given (FUN_004493f0).
pub fn item_choices(e: &EventRecord) -> Vec<usize> {
    let k = e.kind as usize;
    if k >= 30 {
        return Vec::new();
    }
    (ITEM_MIN[k]..ITEM_END[k]).filter(|&id| item_fits(e, id)).map(|id| id as usize).collect()
}

/// The subtypes' text group and how many there are, for a request's reasons, a city
/// status change or a message (0x449b00, 0x449b30).
fn subtypes(k: u8) -> Option<(usize, usize)> {
    match k {
        kind::REQUEST => Some((149, 7)),
        kind::CITY_STATUS => Some((35, 5)),
        kind::MESSAGE => Some((150, 4)),
        _ => None,
    }
}

fn is_request_or_gift(e: &EventRecord) -> bool {
    matches!(e.kind, kind::REQUEST | kind::GIFT)
}

/// A request for troops, whose battle can be lost.
fn is_troop_request(e: &EventRecord) -> bool {
    e.kind == kind::REQUEST && matches!(e.subtype, 1 | 2)
}

fn is_festival(e: &EventRecord) -> bool {
    e.kind == kind::REQUEST && e.subtype == 3
}

/// An invasion from the sea points (9-16) carries warships.
fn has_warships(e: &EventRecord) -> bool {
    e.kind == kind::INVASION && {
        let (lo, _) = ends(location_pick(e));
        lo > 8
    }
}

/// The follow-up of outcome n: comply (or next), refuse, too late, lose battle.
fn link(e: &EventRecord, n: usize) -> i16 {
    [e.on_completed, e.on_refusal, e.on_too_late, e.on_defeat][n]
}

fn set_link(e: &mut EventRecord, n: usize, v: i16) {
    match n {
        0 => e.on_completed = v,
        1 => e.on_refusal = v,
        2 => e.on_too_late = v,
        _ => e.on_defeat = v,
    }
}

/// The outcomes event `e` has a follow-up for.
fn outcomes(e: &EventRecord) -> impl Iterator<Item = usize> + '_ {
    (0..4).filter(move |&n| match n {
        0 => true,
        1 | 2 => is_request_or_gift(e),
        _ => is_troop_request(e),
    })
}

/// Whether event `to` may follow event `from` (FUN_004496d0): it exists, is not
/// `from` itself, and is a triggered event.
pub fn valid_link(events: &[EventRecord], from: usize, to: i16) -> bool {
    usize::try_from(to).ok().and_then(|t| events.get(t).map(|e| (t, e))).is_some_and(|(t, e)| t != from && e.trigger & trigger::TRIGGERED != 0)
}

/// A new event as Add event makes it (0x448590 over FUN_004489c0): a one-time
/// request from a city for 8 loads of grain, a year after the start, from city 1,
/// with twelve months to send it and no follow-ups.
pub fn new_event() -> EventRecord {
    let mut raw = vec![0u8; osiris_formats::events::RECORD];
    // Bytes 56-57, which FUN_004489c0 sets to -1.
    raw[56] = 0xff;
    raw[57] = 0xff;
    EventRecord {
        kind: kind::REQUEST,
        subtype: 0,
        trigger: trigger::ONCE,
        month: 0,
        item: EventValue { value: 1, fixed: 1, min: -1, max: -1 },
        amount: EventValue { value: 8, fixed: 8, min: -1, max: -1 },
        year: 1,
        year_fixed: 1,
        time: EventValue { value: 1, fixed: 0, min: -1, max: -1 },
        location: [1, 1, -1, -1],
        route: [0, -1, 0, 49],
        months: 12,
        on_completed: -1,
        on_refusal: -1,
        on_too_late: -1,
        on_defeat: -1,
        city: 0,
        // "Auto", as the guide says the reasons start.
        link_reasons: [6; 4],
        raw,
        ..Default::default()
    }
}

/// Whether `v` is a value its pick can give (so the game, which reads it as it
/// stands, sees one the scenario allows).
fn fits(v: EventValue) -> bool {
    if v.fixed == -1 && v.min > -1 && v.max > -1 && v.max == v.min {
        return v.value == v.fixed;
    }
    if v.max == -1 {
        return v.value == v.fixed;
    }
    if v.fixed < 0 {
        return v.max <= v.min || (v.min..v.max).contains(&v.value);
    }
    v.value == v.fixed || v.value == v.min || (v.min >= 0 && v.value == v.max)
}

/// The value a pick first offers.
fn first(v: EventValue) -> i16 {
    if v.fixed >= 0 || v.max == -1 { v.fixed } else { v.min }
}

/// The picks' values made ones they allow, where they aren't: the original rolls them
/// all when a map is loaded (FUN_004d5980 calling FUN_00448a90), which Osiris's game
/// doesn't do for the year, so an edited event carries a year it may have.
pub fn settle_values(e: &mut EventRecord) {
    for v in [&mut e.item, &mut e.amount] {
        if !fits(*v) {
            v.value = first(*v);
        }
    }
    let y = year_pick(e);
    if !fits(y) {
        e.year = first(y);
    }
    let l = location_pick(e);
    if !fits(l) {
        e.location[0] = first(l);
    }
    let r = route_pick(e);
    if !fits(r) {
        e.route[0] = first(r);
    }
}

/// The line the Event Summary shows for event `i` (FUN_00448f60): its number, type,
/// when (the month, or `*n` for the event it follows, `***` when none leads to it,
/// `**` for the one Kingdom's disfavour brings), the years (`+a-b`), the amount and
/// the items.
pub fn summary(text: &osiris_formats::TextTable, events: &[EventRecord], i: usize) -> String {
    let e = &events[i];
    let t = |g: usize, id: usize| text.get(g, id).unwrap_or("").to_owned();
    let when = if e.trigger & trigger::TRIGGERED != 0 {
        let parent = events.iter().enumerate().find(|(_, p)| outcomes(p).any(|n| link(p, n) == i as i16)).map(|(j, _)| j);
        match parent {
            Some(j) => format!("*{j} "),
            None => "*** ".to_owned(),
        }
    } else if e.trigger & trigger::FAVOUR != 0 {
        "** ".to_owned()
    } else {
        format!("{} ", t(25, e.month.clamp(0, 11) as usize))
    };
    let (lo, hi) = ends(year_pick(e));
    let mut s = format!("{when}+{lo}");
    if lo < hi {
        s += &format!("-{hi}");
    }
    if has(&AMOUNT, e.kind) {
        let (a, b) = ends(e.amount);
        s += &if a < b { format!(" {a}-{b}") } else { format!(" {a}") };
    }
    if has(&ITEMS, e.kind) {
        let k = e.kind as usize;
        let names: Vec<String> = items(e)
            .iter()
            .filter(|&&id| id > 0)
            .map(|&id| if id < ITEM_MIN[k] || ITEM_GROUP[k] < 0 { String::new() } else { t(ITEM_GROUP[k] as usize, id as usize) })
            .collect();
        s += &format!(" {}", names.join("/"));
    }
    let name = if e.kind < 30 { t(156, e.kind as usize) } else { String::new() };
    format!("{i:2} {name} {s}")
}

impl Editor {
    /// Add event: a new event at the end of the list (not past 120).
    pub fn add_event(&mut self) -> Option<usize> {
        if self.scenario.events.len() >= MAX_EDITOR_EVENTS {
            return None;
        }
        self.scenario.events.push(new_event());
        self.dirty = true;
        Some(self.scenario.events.len() - 1)
    }

    /// Delete event (FUN_00448e30): the event goes, the ones after it move up, and
    /// every follow-up number past it steps down with them (one naming the deleted
    /// event itself is left as it is, as the original leaves it).
    pub fn delete_event(&mut self, i: usize) -> bool {
        let events = &mut self.scenario.events;
        if i >= events.len() {
            return false;
        }
        events.remove(i);
        for e in events.iter_mut() {
            for n in 0..4 {
                let v = link(e, n);
                if v > i as i16 {
                    set_link(e, n, v - 1);
                }
            }
        }
        self.dirty = true;
        true
    }

    /// The trigger button (0x44a470): one time, recurring, triggered only, then back
    /// to one time, or for an invasion to triggered by favour first.
    pub fn cycle_trigger(&mut self, i: usize) {
        let Some(e) = self.scenario.events.get_mut(i) else { return };
        e.trigger = match e.trigger {
            0 => trigger::RECURRING,
            t if t & trigger::RECURRING != 0 => trigger::TRIGGERED,
            t if t & trigger::TRIGGERED != 0 => {
                if e.kind == kind::INVASION {
                    trigger::FAVOUR
                } else {
                    trigger::ONCE
                }
            }
            _ => trigger::ONCE,
        };
        self.dirty = true;
    }

    /// The Pharaoh / City button of a request or gift.
    pub fn toggle_sender(&mut self, i: usize) {
        if let Some(e) = self.scenario.events.get_mut(i) {
            e.sender = (e.sender == 0) as i8;
            self.dirty = true;
        }
    }

    /// An invasion's target (text group 36), stepped through its five.
    pub fn cycle_target(&mut self, i: usize) {
        if let Some(e) = self.scenario.events.get_mut(i) {
            e.attack_target = (e.attack_target + 1) % 5;
            self.dirty = true;
        }
    }
}

/// A value chosen from a list for event `i`.
pub fn pick(s: &mut Scenario, i: usize, f: EvField, id: usize) {
    let Some(e) = s.events.get_mut(i) else { return };
    let id16 = id as i16;
    match f {
        // A new type (0x449710): the first item, the others cleared; a favour
        // trigger only stays with an invasion.
        EvField::Kind => {
            if e.kind != id as u8 {
                e.kind = id as u8;
                e.item.fixed = 1;
                e.item.min = -1;
                e.item.max = -1;
                if e.trigger & trigger::FAVOUR != 0 && e.kind != kind::INVASION {
                    e.trigger = trigger::ONCE;
                }
            }
        }
        EvField::Month => e.month = id16.clamp(0, 11),
        // A new reason (0x4499c0): items it doesn't take give way to the first it
        // does (the first slot) or go (the others).
        EvField::Subtype => {
            let n = subtypes(e.kind).map_or(0, |(_, n)| n);
            if id < n && e.subtype != id as i8 {
                e.subtype = id as i8;
                if has(&ITEMS, e.kind) {
                    let first = item_choices(e).first().map_or(0, |&v| v as i16);
                    let mut it = items(e);
                    for (slot, v) in it.iter_mut().enumerate() {
                        if !item_fits(e, *v) {
                            *v = if slot == 0 { first } else { -1 };
                        }
                    }
                    set_items(e, it);
                }
            }
        }
        EvField::God => {
            if id < 5 {
                e.god = id as i8;
            }
        }
        // An item (0x44a940): below the type's first id means none.
        EvField::Item(slot) => {
            let k = e.kind as usize;
            let v = if k < 30 && ITEM_MIN[k] > 0 && id16 < ITEM_MIN[k] { -1 } else { id16 };
            let mut it = items(e);
            it[slot.min(2)] = v;
            set_items(e, it);
        }
        EvField::LinkReason(n) => e.link_reasons[n.min(3)] = id.min(6) as u8,
        _ => return,
    }
    settle_values(e);
}

/// A number typed on the keypad for event `i`.
pub fn set_number(s: &mut Scenario, i: usize, f: EvField, v: i32) {
    let Some(e) = s.events.get_mut(i) else { return };
    let v16 = v.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
    match f {
        EvField::Year(high) => {
            let mut p = year_pick(e);
            set_end(&mut p, high, v16);
            set_year_pick(e, p);
        }
        EvField::Amount(high) => set_end(&mut e.amount, high, v16),
        EvField::Location(high) => {
            let mut p = location_pick(e);
            set_end(&mut p, high, v16);
            e.location = [p.value, p.fixed, p.min, p.max];
        }
        EvField::Route(high) => {
            let mut p = route_pick(e);
            set_end(&mut p, high, v16);
            e.route = [p.value, p.fixed, p.min, p.max];
        }
        EvField::Months => e.months = v.clamp(0, 255) as u8,
        EvField::Warships => e.god = v.clamp(0, 127) as i8,
        EvField::Link(n) => set_link(e, n.min(3), v16),
        _ => return,
    }
    settle_values(e);
}

/// The value a keypad for `f` starts from.
pub fn number(s: &Scenario, i: usize, f: EvField) -> i32 {
    let Some(e) = s.events.get(i) else { return 0 };
    let end = |v: EventValue, high: bool| {
        let (lo, hi) = ends(v);
        (if high { hi } else { lo }).max(0) as i32
    };
    match f {
        EvField::Year(h) => end(year_pick(e), h),
        EvField::Amount(h) => end(e.amount, h),
        EvField::Location(h) => end(location_pick(e), h),
        EvField::Route(h) => end(route_pick(e), h),
        EvField::Months => e.months as i32,
        EvField::Warships => e.god.max(0) as i32,
        // The original's keypad for a follow-up starts at 999.
        EvField::Link(_) => 999,
        _ => 0,
    }
}

/// The list a button opens: its text group and entries.
pub fn list(s: &Scenario, i: usize, f: EvField) -> Option<(usize, Vec<usize>)> {
    let e = s.events.get(i)?;
    Some(match f {
        // Every type but the free event, earthquake, revolt and change of
        // Pharaoh (0x4498f0); the mummy the original offers only with Cleopatra
        // installed, which Osiris always has.
        EvField::Kind => (156, (1..30).filter(|&k| !matches!(k, 3 | 4 | 5)).collect()),
        EvField::Month => (25, (0..12).collect()),
        EvField::Subtype => {
            let (g, n) = subtypes(e.kind)?;
            (g, (0..n).collect())
        }
        EvField::God => (157, (0..5).collect()),
        EvField::Item(_) => {
            let g = ITEM_GROUP.get(e.kind as usize).copied().unwrap_or(-1);
            if g < 0 {
                return None;
            }
            (g as usize, item_choices(e))
        }
        // The list shows the reasons' long names (299/0-6); the button the short.
        EvField::LinkReason(_) => (299, (0..7).collect()),
        _ => return None,
    })
}

/// A bordered button with text written as the planning window writes it: centred
/// over the button's width from 4 pixels in, 7 down (FUN_004cd240), or left-aligned
/// there (FUN_004cd1f0).
fn field_button(ui: &mut Ui, rect: [f32; 4], s: &str, centred: bool) -> bool {
    let hot = ui.hot(rect);
    panel::button_border(ui.r, ui.panels, rect[0], rect[1], rect[2] as i32, rect[3] as i32, hot);
    if centred {
        ui.centred(Font::NormalBlackOnLight, s, rect[0] + 4.0, rect[1] + 7.0, rect[2]);
    } else {
        ui.label(Font::NormalBlackOnLight, s, rect[0] + 4.0, rect[1] + 7.0);
    }
    ui.clicked(rect)
}

/// A rectangle of the window's button tables, (x1, y1)-(x2, y2) from its corner.
fn rect(x: f32, y: f32, b: [i32; 4]) -> [f32; 4] {
    [x + b[0] as f32, y + b[1] as f32, (b[2] - b[0]) as f32, (b[3] - b[1]) as f32]
}

impl Editor {
    /// The Event Summary (FUN_00448640): twelve events a page, the one last opened in
    /// white, Add event and Delete event below, and the arrows and bar that scroll.
    pub(super) fn options_events(&mut self, ui: &mut Ui, o: &mut Options, x: f32, y: f32) {
        panel::outer_panel(ui.r, ui.panels, x, y, 40, 27);
        let count = self.scenario.events.len();
        o.event_top = o.event_top.min(count.saturating_sub(1));
        if field_button(ui, rect(x, y, [100, 358, 250, 378]), &ui.t(44, 96), true) {
            self.add_event();
        }
        if field_button(ui, rect(x, y, [260, 358, 410, 378]), &ui.t(44, 97), true)
            && let Some(i) = o.event.take()
        {
            self.delete_event(i);
            let n = self.scenario.events.len();
            if o.event_top >= n {
                o.event_top = o.event_top.saturating_sub(1);
            }
        }
        for row in 0..ROWS {
            let r = rect(x, y, [32, 46 + 24 * row as i32, 500, 66 + 24 * row as i32]);
            let i = o.event_top + row;
            let hot = ui.hot(r);
            if i < count {
                panel::button_border(ui.r, ui.panels, r[0], r[1], r[2] as i32, r[3] as i32, hot);
                let line = summary(ui.text, &self.scenario.events, i);
                let f = if o.event == Some(i) { Font::NormalWhiteOnDark } else { Font::NormalBlackOnDark };
                ui.label(f, &line, r[0] + 4.0, r[1] + 4.0);
            }
            if ui.clicked(r) {
                if i < count {
                    o.event = Some(i);
                    o.page = Page::Event;
                } else {
                    o.event = None;
                }
            }
        }
        // The arrows (table 0x5d9a48: Pharaoh_General 96, frames 8 and 12) step one
        // event; the bar between them (FUN_004483f0) jumps.
        let lib = &ui.r.library;
        let arrows = [lib.group_id("Pharaoh_General", 96, 8).ok(), lib.group_id("Pharaoh_General", 96, 12).ok()];
        for (n, (ay, id)) in [(46.0, arrows[0]), (300.0, arrows[1])].into_iter().enumerate() {
            let r = [x + 508.0, y + ay, 34.0, 34.0];
            if let Some(id) = id {
                let frame = if ui.hot(r) { 1 } else { 0 };
                ui.image(id + frame, r[0], r[1]);
            }
            if ui.clicked(r) {
                if n == 0 {
                    o.event_top = o.event_top.saturating_sub(1);
                } else if o.event_top + 1 < count {
                    o.event_top += 1;
                }
            }
        }
        let over = count as i32 - ROWS as i32;
        if over > 0 || o.event_top > 0 {
            let pct = if over < 1 { 100 } else { (o.event_top as i32 * 100 / over).min(100) };
            if let Ok(id) = ui.r.library.group_id("Pharaoh_General", 15, 39) {
                ui.image(id, x + 516.0, y + 80.0 + (196 * pct / 100) as f32);
            }
        }
        let bar = [x + 508.0, y + 80.0, 32.0, 220.0];
        if let Some(c) = ui.click
            && inside(bar, c)
            && (over > 0 || o.event_top > 0)
        {
            ui.click = None;
            let v = ((c[1] - y - 80.0) as i32).min(196);
            let top = over.max(0) * (v * 100 / 196) / 100;
            if top >= 0 && (top as usize) < count {
                o.event_top = top as usize;
            }
        }
    }

    /// The event planning window (FUN_0044aaf0) for event `o.event`.
    pub(super) fn options_event(&mut self, ui: &mut Ui, o: &mut Options, x: f32, y: f32) {
        let Some(i) = o.event.filter(|&i| i < self.scenario.events.len()) else {
            o.page = Page::Events;
            return;
        };
        // A free event (type 0) is dropped as it is opened.
        if self.scenario.events[i].kind == 0 {
            self.delete_event(i);
            o.event = None;
            o.page = Page::Events;
            return;
        }
        if is_festival(&self.scenario.events[i]) && self.scenario.events[i].god > 4 {
            self.scenario.events[i].god = 0;
        }
        let e = self.scenario.events[i].clone();
        let list = |o: &mut Options, ed: &Editor, f: EvField| {
            if let Some((group, ids)) = list(&ed.scenario, i, f) {
                o.picker = Some(Picker::new(Field::Event(i, f), group, ids));
            }
        };
        let pad = |o: &mut Options, ed: &Editor, f: EvField| {
            o.keypad = Some(Keypad::new(Field::Event(i, f), number(&ed.scenario, i, f)));
        };
        panel::outer_panel(ui.r, ui.panels, x, y, 38, 25);
        let title = ui.t(44, 139);
        let w = ui.width(Font::LargeBlackOnLight, &title);
        ui.label(Font::LargeBlackOnLight, &title, x + 40.0, y + 20.0);
        ui.label(Font::LargeBlackOnLight, &format!(" {i}"), x + 40.0 + w, y + 20.0);
        let kind_name = ui.t(156, e.kind as usize);
        if field_button(ui, rect(x, y, [258, 20, 428, 45]), &kind_name, true) {
            list(o, self, EvField::Kind);
        }
        if is_request_or_gift(&e) && field_button(ui, rect(x, y, [166, 20, 258, 45]), &ui.t(44, if e.sender != 0 { 161 } else { 162 }), true) {
            self.toggle_sender(i);
        }
        let trig = if e.trigger & trigger::TRIGGERED != 0 {
            159
        } else if e.trigger & trigger::RECURRING != 0 {
            158
        } else if e.trigger & trigger::FAVOUR != 0 {
            160
        } else {
            157
        };
        if field_button(ui, rect(x, y, [430, 20, 590, 45]), &ui.t(44, trig), true) {
            self.cycle_trigger(i);
        }
        if let Some((g, _)) = subtypes(e.kind)
            && field_button(ui, rect(x, y, [188, 46, 428, 71]), &ui.t(g, e.subtype.max(0) as usize), true)
        {
            list(o, self, EvField::Subtype);
        }
        let triggered = e.trigger & trigger::TRIGGERED != 0;
        if e.trigger & trigger::FAVOUR == 0 {
            if !triggered {
                let m = if (0..12).contains(&e.month) { ui.t(25, e.month as usize) } else { String::new() };
                if field_button(ui, rect(x, y, [64, 60, 114, 85]), &m, true) {
                    list(o, self, EvField::Month);
                }
            }
            let (lo, hi) = ends(year_pick(&e));
            let start = self.scenario.info.start_year as i32;
            for (label, ly, b, v, high) in [(141, 100.0, [40, 125, 140, 167], lo, false), (142, 173.0, [40, 195, 140, 237], hi, true)] {
                let l = ui.t(44, label);
                ui.centred(Font::NormalBlackOnLight, &l, x + 40.0, y + ly, 100.0);
                let r = rect(x, y, b);
                if field_button(ui, r, &format!("+{v}"), true) {
                    pad(o, self, EvField::Year(high));
                }
                if triggered {
                    let months = ui.t(44, 150);
                    ui.centred(Font::NormalBlackOnLight, &months, r[0] + 4.0, r[1] + 24.0, r[2]);
                } else {
                    let year = super::options::year_text(ui, start + v as i32);
                    ui.label(Font::NormalBlackOnLight, &year, r[0] + 4.0, r[1] + 24.0);
                }
            }
        }
        if has(&LOCATION, e.kind) {
            let (lo, hi) = ends(location_pick(&e));
            let l = ui.t(44, if has(&FROM_MARKERS, e.kind) { 144 } else { 143 });
            ui.label(Font::NormalBlackOnLight, &l, x + 160.0, y + 104.0);
            if field_button(ui, rect(x, y, [270, 100, 370, 125]), &lo.to_string(), true) {
                pad(o, self, EvField::Location(false));
            }
            let to = ui.t(44, 145);
            ui.label(Font::NormalBlackOnLight, &to, x + 384.0, y + 104.0);
            if field_button(ui, rect(x, y, [420, 100, 520, 125]), &hi.to_string(), true) {
                pad(o, self, EvField::Location(true));
            }
        }
        if has(&AMOUNT, e.kind) {
            let (lo, hi) = ends(e.amount);
            let l = ui.t(44, if e.kind == kind::EARTHQUAKE { 147 } else { 146 });
            ui.label(Font::NormalBlackOnLight, &l, x + 339.0, y + 148.0);
            if field_button(ui, rect(x, y, [412, 142, 482, 167]), &lo.to_string(), true) {
                pad(o, self, EvField::Amount(false));
            }
            let to = ui.t(44, 145);
            ui.label(Font::NormalBlackOnLight, &to, x + 488.0, y + 148.0);
            if field_button(ui, rect(x, y, [522, 142, 592, 167]), &hi.to_string(), true) {
                pad(o, self, EvField::Amount(true));
            }
        }
        if e.kind == kind::INVASION {
            let (lo, hi) = ends(route_pick(&e));
            let l = ui.t(44, 32);
            ui.label(Font::NormalBlackOnLight, &l, x + 339.0, y + 186.0);
            if field_button(ui, rect(x, y, [412, 180, 482, 205]), &lo.to_string(), true) {
                pad(o, self, EvField::Route(false));
            }
            let to = ui.t(44, 145);
            ui.label(Font::NormalBlackOnLight, &to, x + 488.0, y + 186.0);
            if field_button(ui, rect(x, y, [522, 180, 592, 205]), &hi.to_string(), true) {
                pad(o, self, EvField::Route(true));
            }
        }
        if has(&ITEMS, e.kind) {
            let group = ITEM_GROUP[e.kind as usize];
            for (slot, id) in items(&e).into_iter().enumerate() {
                let b = [180 + 140 * slot as i32, 72, 310 + 140 * slot as i32, 97];
                let name = if group >= 0 && item_fits(&e, id) { ui.t(group as usize, id as usize) } else { String::new() };
                if field_button(ui, rect(x, y, b), &name, true) {
                    self.item_button(o, i, slot);
                }
            }
            if e.kind == kind::REQUEST {
                let l = ui.t(44, 213);
                ui.label(Font::NormalBlackOnLight, &l, x + 290.0, y + 168.0);
            }
        }
        if matches!(e.kind, kind::REQUEST | kind::INVASION) {
            let r = rect(x, y, [170, 180, 270, 205]);
            let unit = ui.t(44, if e.months == 1 { 140 } else { 150 });
            if field_button(ui, r, &format!("{} {unit}", e.months), false) {
                pad(o, self, EvField::Months);
            }
        }
        if e.kind == kind::INVASION && field_button(ui, rect(x, y, [170, 212, 370, 237]), &ui.t(36, e.attack_target.clamp(0, 4) as usize), false) {
            self.cycle_target(i);
        }
        if has_warships(&e) {
            let l = ui.t(44, 163);
            ui.label(Font::NormalBlackOnLight, &l, x + 388.0, y + 218.0);
            if field_button(ui, rect(x, y, [472, 212, 592, 237]), &e.god.to_string(), true) {
                pad(o, self, EvField::Warships);
            }
        }
        if is_festival(&e) {
            let l = ui.t(44, 212);
            ui.label(Font::NormalBlackOnLight, &l, x + 388.0, y + 218.0);
            if field_button(ui, rect(x, y, [472, 212, 592, 237]), &ui.t(157, e.god.clamp(0, 4) as usize), true) {
                list(o, self, EvField::God);
            }
        }
        // The follow-ups: which event each outcome brings, and the reason it gives.
        for n in outcomes(&e).collect::<Vec<_>>() {
            let ry = 244 + 32 * n as i32;
            let label = match n {
                0 if is_request_or_gift(&e) => 151,
                0 => 156,
                1 => 152,
                2 if e.kind == kind::GIFT => 154,
                2 => 153,
                _ => 155,
            };
            let l = ui.t(44, label);
            ui.label(Font::NormalBlackOnLight, &l, x + 140.0, y + ry as f32 + 4.0);
            let to = link(&e, n);
            if field_button(ui, rect(x, y, [290, ry, 330, ry + 25]), &to.to_string(), false) {
                pad(o, self, EvField::Link(n));
            }
            let what = if valid_link(&self.scenario.events, i, to) { ui.t(156, self.scenario.events[to as usize].kind as usize) } else { ui.t(44, 105) };
            ui.label(Font::NormalBlackOnLight, &what, x + 410.0, y + ry as f32 + 4.0);
            let reason = ui.t(299, e.link_reasons[n].min(6) as usize + 7);
            if field_button(ui, rect(x, y, [340, ry, 400, ry + 25]), &reason, true) {
                list(o, self, EvField::LinkReason(n));
            }
        }
    }

    /// An item button (0x44aa30): the list of the items the event may take, or with
    /// only one, that one straight away.
    fn item_button(&mut self, o: &mut Options, i: usize, slot: usize) {
        let Some((group, ids)) = list(&self.scenario, i, EvField::Item(slot)) else { return };
        match ids.len() {
            0 => {}
            1 => {
                let v = if slot == 0 { ids[0] } else { 0 };
                pick(&mut self.scenario, i, EvField::Item(slot), v);
                self.dirty = true;
            }
            _ => o.picker = Some(Picker::new(Field::Event(i, EvField::Item(slot)), group, ids)),
        }
    }
}
