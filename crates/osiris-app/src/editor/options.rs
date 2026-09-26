//! The editor's Options screen (the button under the minimap) and the windows it
//! opens: Starting conditions, the start date, Win criteria with the choice of
//! monuments and their burial provisions, Buildings allowed, Gods settings and the
//! flood plain's settings. Layouts are the original's (FUN_004138d0, FUN_00532920,
//! FUN_00533470, FUN_005374a0, FUN_00532fe0, FUN_00537260, FUN_00530780), in the
//! 640x480 window centred on the screen; values are picked from lists or typed on a
//! keypad. The Events button waits for the events editor.

use super::Editor;
use crate::widgets::{Ui, UiImages, inside};
use osiris_formats::Scenario;
use osiris_render::{Renderer, Space, WHITE};
use osiris_ui::{Font, PanelImages, panel};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Main,
    Starting,
    StartDate,
    Win,
    Monuments,
    Allowed,
    Gods,
    Flood,
}

impl Page {
    fn parent(self) -> Option<Page> {
        match self {
            Page::Main => None,
            Page::StartDate => Some(Page::Starting),
            Page::Monuments => Some(Page::Win),
            _ => Some(Page::Main),
        }
    }
}

/// A value being set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Rank,
    Year,
    Funds,
    Gift,
    Milestone(usize),
    Interest,
    Pharaoh,
    Incarnation,
    Enemy,
    HousingCount,
    HousingLevel,
    Culture,
    Prosperity,
    Kingdom,
    TimeLimit,
    Survival,
    Population,
    Monument(usize),
    Provision(usize),
}

/// A list to pick a value from: its text group and entries.
pub struct Picker {
    field: Field,
    group: usize,
    ids: Vec<usize>,
    page: usize,
}

/// The keypad: the value typed so far, which the first digit typed replaces.
pub struct Keypad {
    field: Field,
    typed: String,
    fresh: bool,
}

impl Keypad {
    fn new(field: Field, value: i32) -> Self {
        Self { field, typed: value.to_string(), fresh: true }
    }

    fn digit(&mut self, d: char) {
        if std::mem::take(&mut self.fresh) {
            self.typed.clear();
        }
        if self.typed.len() < 6 {
            self.typed.push(d);
        }
    }
}

#[derive(Default)]
pub struct Options {
    pub page: Page,
    picker: Option<Picker>,
    keypad: Option<Keypad>,
    /// The brief description's box has the keyboard.
    typing: bool,
}

/// The resources a tomb can be sent (FUN_005374a0), and how many of each count for
/// one point of the monument rating: 32 or 16 (FUN_004f78a0).
const PROVISIONS: [(usize, i32); 15] = [(1, 32), (8, 32), (10, 16), (13, 16), (15, 16), (17, 16), (18, 32), (19, 16), (20, 32), (23, 16), (24, 32), (25, 32), (26, 32), (28, 16), (30, 32)];

/// Each monument's kind (1 tomb, 2 other), family and rating points (the table at
/// 0x5d19c8), by monument id (text group 198).
const MONUMENTS: [(u8, u8, i32); 38] = [
    (0, 0, 0),
    (1, 1, 4),
    (1, 1, 10),
    (1, 2, 4),
    (1, 2, 12),
    (1, 2, 26),
    (1, 2, 48),
    (1, 2, 80),
    (1, 3, 3),
    (1, 3, 9),
    (1, 3, 20),
    (1, 3, 36),
    (1, 3, 60),
    (1, 4, 4),
    (1, 4, 12),
    (1, 4, 26),
    (1, 4, 48),
    (1, 4, 80),
    (1, 5, 2),
    (1, 5, 3),
    (1, 5, 11),
    (2, 1, 10),
    (2, 2, 1),
    (2, 2, 3),
    (2, 3, 4),
    (2, 4, 5),
    (2, 4, 5),
    (2, 4, 5),
    (2, 5, 8),
    (2, 6, 12),
    (2, 7, 10),
    (2, 8, 5),
    (2, 9, 5),
    (2, 10, 4),
    (2, 11, 12),
    (2, 12, 26),
    (2, 13, 48),
    (2, 14, 48),
];

/// Whether monument `m` may be built in era `era` (FUN_0040cb40): the sphinx,
/// obelisks, sun temple and mausoleums in any; the pyramids and mastabas with the
/// Pyramids, royal tombs in the Valley of the Kings, the lighthouse, library and
/// Caesareum at Alexandria, Abu Simbel in its own.
fn fits_era(m: usize, era: u8) -> bool {
    let Some(&(kind, family, _)) = MONUMENTS.get(m) else { return false };
    if era == 0 || kind == 0 {
        return false;
    }
    if kind == 2 && (1..=4).contains(&family) {
        return true;
    }
    match era {
        1 => kind == 1,
        2 => kind == 2 && (10..=13).contains(&family),
        3 => kind == 2 && (5..=7).contains(&family),
        4 => kind == 2 && family == 14,
        // Pharaoh's own maps hold 255 here, from before eras: anything goes.
        _ => true,
    }
}

/// Whether monument `m` is a tomb that takes burial provisions.
fn is_tomb(m: usize) -> bool {
    MONUMENTS.get(m).is_some_and(|&(kind, family, _)| kind == 1 || (kind == 2 && matches!(family, 4 | 8 | 10 | 11 | 12 | 13)))
}

/// The monument rating the goal asks for (FUN_004f78a0 as the editor calls it): the
/// square root of the monuments' points and the provisions' shares, times 6.32.
pub fn monument_rating(info: &osiris_formats::ScenarioInfo) -> i32 {
    if info.monuments.iter().all(|&m| m == 0) {
        return 0;
    }
    let mut sum = 0.0f64;
    for &m in &info.monuments {
        sum += MONUMENTS.get(m as usize).map_or(0, |&(_, _, r)| r) as f64;
    }
    for &(r, per) in &PROVISIONS {
        let a = info.burial_provisions_required.get(r).copied().unwrap_or(0) as i32;
        sum += (a / per) as f64;
    }
    (sum.sqrt() * 6.32 + 0.5) as i32
}

impl Options {
    /// The screen opened at `page`.
    pub fn at(page: Page) -> Self {
        Self { page, ..Default::default() }
    }

    /// The screen at `page` with a value's list or keypad open, for scripted
    /// screenshots: `rank`, `pharaoh`, `incarnation`, `enemy`, `housing`, `monument`,
    /// or a keypad's `funds`.
    pub fn with_chooser(page: Page, what: &str, s: &Scenario) -> Self {
        let mut o = Self::at(page);
        let list = |field, group, ids: Vec<usize>| Some(Picker { field, group, ids, page: 0 });
        o.picker = match what {
            "rank" => list(Field::Rank, 32, (0..=10).collect()),
            "pharaoh" => list(Field::Pharaoh, 151, (0..125).collect()),
            "incarnation" => list(Field::Incarnation, 152, (0..=30).collect()),
            "enemy" => list(Field::Enemy, 37, (0..14).collect()),
            "housing" => list(Field::HousingLevel, 29, (0..20).collect()),
            "monument" => list(Field::Monument(0), 198, std::iter::once(0).chain((1..MONUMENTS.len()).filter(|&m| fits_era(m, 1))).collect()),
            _ => None,
        };
        if what == "funds" {
            o.keypad = Some(Keypad::new(Field::Funds, number(s, Field::Funds)));
        }
        o
    }

    pub fn wants_text(&self) -> bool {
        self.typing || self.keypad.is_some()
    }

    /// Backs out one step; true when the Options screen itself closes.
    pub fn back(&mut self) -> bool {
        if self.picker.take().is_some() || self.keypad.take().is_some() {
            return false;
        }
        if self.typing {
            self.typing = false;
            return false;
        }
        match self.page.parent() {
            Some(p) => {
                self.page = p;
                false
            }
            None => true,
        }
    }

    /// Typed text: the brief description while its box has the keyboard, digits on
    /// the keypad.
    pub fn type_text(&mut self, s: &mut Scenario, text: &str) {
        if let Some(k) = &mut self.keypad {
            for c in text.chars() {
                match c {
                    '0'..='9' => k.digit(c),
                    '\u{8}' => {
                        k.fresh = false;
                        k.typed.pop();
                    }
                    '\n' | '\r' => {
                        let (field, v) = (k.field, k.typed.parse().unwrap_or(0));
                        self.keypad = None;
                        set_number(s, field, v);
                        return;
                    }
                    _ => {}
                }
            }
            return;
        }
        if !self.typing {
            return;
        }
        let d = &mut s.info.subtitle;
        for c in text.chars() {
            match c {
                '\u{8}' => {
                    d.pop();
                }
                '\n' | '\r' => self.typing = false,
                // The guide: "there's a limit of 24 characters".
                c if !c.is_control() && c.is_ascii() && d.len() < 24 => d.push(c),
                _ => {}
            }
        }
    }
}

/// A field's value as a number, for the keypad.
fn number(s: &Scenario, f: Field) -> i32 {
    let i = &s.info;
    match f {
        Field::Funds => i.initial_funds,
        Field::Gift => i.rescue_loan,
        Field::Milestone(n) => i.win.milestone_years[n],
        Field::Interest => i.debt_interest_rate as i32,
        Field::Year => i.start_year.abs() as i32,
        Field::HousingCount => i.win.housing_count.value,
        Field::Culture => i.win.culture.value,
        Field::Prosperity => i.win.prosperity.value,
        Field::Kingdom => i.win.kingdom.value,
        Field::TimeLimit => i.win.time_limit.value,
        Field::Survival => i.win.survival_time.value,
        Field::Population => i.win.population.value,
        Field::Provision(r) => i.burial_provisions_required.get(r).copied().unwrap_or(0) as i32,
        _ => 0,
    }
}

fn set_number(s: &mut Scenario, f: Field, v: i32) {
    let i = &mut s.info;
    match f {
        Field::Funds => i.initial_funds = v,
        Field::Gift => i.rescue_loan = v,
        Field::Milestone(n) => i.win.milestone_years[n] = v,
        Field::Interest => i.debt_interest_rate = v.max(0) as u32,
        Field::Year => i.start_year = if i.start_year < 0 { -(v.min(9999) as i16) } else { v.min(9999) as i16 },
        Field::HousingCount => {
            i.win.housing_count.value = v;
            // Like the monuments goal, "on" follows from the value itself: a count
            // of zero means no housing requirement.
            i.win.housing_count.enabled = v != 0;
        }
        Field::Culture => i.win.culture.value = v,
        Field::Prosperity => i.win.prosperity.value = v,
        Field::Kingdom => i.win.kingdom.value = v,
        Field::TimeLimit => i.win.time_limit.value = v,
        Field::Survival => i.win.survival_time.value = v,
        Field::Population => i.win.population.value = v,
        Field::Provision(r) => {
            if i.burial_provisions_required.len() <= r {
                i.burial_provisions_required.resize(36, 0);
            }
            // The window caps an amount at 3200.
            i.burial_provisions_required[r] = v.clamp(0, 3200) as u32;
            let rating = monument_rating(i);
            i.win.monuments.value = rating;
            i.win.monuments.enabled = rating != 0;
        }
        _ => {}
    }
}

fn pick(s: &mut Scenario, f: Field, id: usize) {
    let i = &mut s.info;
    match f {
        Field::Rank => i.player_rank = id as i16,
        Field::Pharaoh => i.current_pharaoh = id as u32,
        Field::Incarnation => i.player_incarnation = id as u32,
        Field::Enemy => i.enemy_id = id as i16,
        Field::HousingLevel => i.win.housing_level.value = id as i32,
        Field::Monument(n) => {
            i.monuments[n] = id as u16;
            // The original keeps the three in order (qsort at FUN_005374a0).
            let mut m = i.monuments;
            m.sort_by_key(|&v| if v == 0 { u16::MAX } else { v });
            i.monuments = m;
            let rating = monument_rating(i);
            i.win.monuments.value = rating;
            i.win.monuments.enabled = rating != 0;
        }
        _ => {}
    }
}

/// The start year as the original writes it: "2500 BC", "AD 30".
fn year(ui: &Ui, y: i32) -> String {
    if y < 0 { format!("{} {}", -y, ui.t(20, 0).trim()) } else { format!("{} {y}", ui.t(20, 1).trim()) }
}

/// A value button greyed like the Options screen's unavailable Events button: the
/// border, but the text tinted rather than plain black, for a value that means
/// nothing while its switch is off.
fn grey_button(ui: &mut Ui, rect: [f32; 4], s: &str) -> bool {
    let hot = ui.hot(rect);
    panel::button_border(ui.r, ui.panels, rect[0], rect[1], rect[2] as i32, rect[3] as i32, hot);
    let tw = ui.width(Font::NormalWhiteOnDark, s);
    osiris_ui::draw_text_tinted(ui.r, Font::NormalWhiteOnDark, s, rect[0] + ((rect[2] - tw) / 2.0).max(0.0).floor(), rect[1] + ((rect[3] - 12.0) / 2.0).floor(), [0.6, 0.55, 0.5, 1.0]);
    ui.clicked(rect)
}

/// The 640x480 window's corner, centred on the screen.
fn origin(screen: [f32; 2]) -> (f32, f32) {
    (((screen[0] - 640.0) / 2.0).floor(), ((screen[1] - 480.0) / 2.0).floor())
}

impl Editor {
    pub fn draw_options(&mut self, r: &mut Renderer, panels: &PanelImages) {
        let Some(mut o) = self.view.options.take() else { return };
        let img = match UiImages::load(&r.library) {
            Ok(i) => i,
            Err(_) => return,
        };
        let click = self.view.options_click.take();
        let (x, y) = origin(r.screen);
        let cursor = self.view.cursor;
        let text = self.text.clone();
        let mut ui = Ui { r, panels, img, text: &text, cursor, click: if o.picker.is_some() || o.keypad.is_some() { None } else { click } };
        let mut close = false;
        let before = format!("{:?}{:?}", self.scenario.info, self.era);
        match o.page {
            Page::Main => close = self.options_main(&mut ui, &mut o, x, y),
            Page::Starting => self.options_starting(&mut ui, &mut o, x, y),
            Page::StartDate => self.options_start_date(&mut ui, &mut o, x, y),
            Page::Win => self.options_win(&mut ui, &mut o, x, y),
            Page::Monuments => self.options_monuments(&mut ui, &mut o, x, y),
            Page::Allowed => self.options_allowed(&mut ui, x, y),
            Page::Gods => self.options_gods(&mut ui, x, y),
            Page::Flood => self.options_flood(&mut ui, x, y),
        }
        if o.picker.is_some() {
            ui.click = click;
            self.draw_picker(&mut ui, &mut o);
        } else if o.keypad.is_some() {
            ui.click = click;
            self.draw_keypad(&mut ui, &mut o);
        }
        if format!("{:?}{:?}", self.scenario.info, self.era) != before {
            self.dirty = true;
        }
        if !close {
            self.view.options = Some(o);
        }
    }

    /// The Options screen itself (FUN_004138d0).
    fn options_main(&mut self, ui: &mut Ui, o: &mut Options, x: f32, y: f32) -> bool {
        panel::outer_panel(ui.r, ui.panels, x, y + 28.0, 30, 25);
        // The brief description, typed into its box: a hover tint and, while it's
        // empty and not being typed into, a blinking caret, so an empty box still
        // reads as a text field to click into.
        let desc = [x + 109.0, y + 40.0, 288.0, 32.0];
        panel::inner_panel(ui.r, ui.panels, desc[0], desc[1], 18, 2);
        if ui.hot(desc) {
            ui.r.rect([desc[0], desc[1]], [desc[2], desc[3]], [1.0, 1.0, 1.0, 0.12], Space::Screen);
        }
        let blink = (self.view.clock * 2.0) as i32 % 2 == 0;
        let caret = if o.typing || (self.scenario.info.subtitle.is_empty() && blink) { "_" } else { "" };
        let d = format!("{}{caret}", self.scenario.info.subtitle);
        ui.label(Font::NormalWhiteOnDark, &d, x + 112.0, y + 50.0);
        if ui.clicked(desc) {
            o.typing = true;
        }
        let b = |row: f32| [x + 212.0, y + row, 250.0, 30.0];
        if ui.button(b(76.0), &ui.t(44, 88), Font::NormalBlackOnLight) {
            o.page = Page::Starting;
        }
        let t76 = ui.t(44, 76);
        ui.label(Font::NormalBlackOnLight, &t76, x + 32.0, y + 125.0);
        let climate = self.scenario.info.climate.min(2);
        if ui.button(b(116.0), &ui.t(44, 77 + climate as usize), Font::NormalBlackOnLight) {
            self.scenario.info.climate = (climate + 1) % 3;
            self.refresh_map();
        }
        // Events wait for the events editor.
        panel::button_border(ui.r, ui.panels, x + 212.0, y + 156.0, 250, 30, false);
        let events = ui.t(44, 95);
        let ew = ui.width(Font::NormalWhiteOnDark, &events);
        osiris_ui::draw_text_tinted(ui.r, Font::NormalWhiteOnDark, &events, x + 212.0 + ((250.0 - ew) / 2.0).floor(), y + 165.0, [0.6, 0.55, 0.5, 1.0]);
        let faction = self.scenario.info.player_faction == 1;
        if ui.button([x + 17.0, y + 196.0, 175.0, 30.0], &ui.t(44, if faction { 227 } else { 41 }), Font::NormalBlackOnLight) {
            self.scenario.info.player_faction = !faction as u8;
        }
        let enemy = ui.t(37, self.scenario.info.enemy_id.max(0) as usize);
        if ui.button(b(196.0), &enemy, Font::NormalBlackOnLight) {
            o.picker = Some(Picker { field: Field::Enemy, group: 37, ids: (0..14).collect(), page: 0 });
        }
        for (row, label, page) in [(236.0, 164, Page::Gods), (276.0, 44, Page::Allowed), (316.0, 45, Page::Win), (356.0, 178, Page::Flood)] {
            if ui.button(b(row), &ui.t(44, label), Font::NormalBlackOnLight) {
                o.page = page;
            }
        }
        // The scenario's picture, with the arrows that change it.
        panel::button_border(ui.r, ui.panels, x + 18.0, y + 238.0, 184, 144, false);
        let pic = self.scenario.info.image_id.max(0) as usize;
        let id = if pic < 19 { ui.r.library.group_id("Pharaoh_Unloaded", 28, pic) } else { ui.r.library.group_id("Expansion", 38, pic - 19) };
        if let Ok(id) = id {
            ui.image(id, x + 20.0, y + 240.0);
        }
        let count = 19 + (0..40).take_while(|&n| ui.r.library.group_id("Expansion", 38, n).is_ok_and(|id| ui.r.record(id).is_some_and(|rec| rec.width > 0))).count();
        if ui.arrow(x + 20.0, y + 384.0, false) {
            self.scenario.info.image_id = ((pic + count - 1) % count) as i16;
        }
        if ui.arrow(x + 44.0, y + 384.0, true) {
            self.scenario.info.image_id = ((pic + 1) % count) as i16;
        }
        // A click outside closes the screen, as a right-click does.
        ui.click.is_some_and(|c| !inside([x, y + 28.0, 480.0, 400.0], c))
    }

    /// Starting conditions (FUN_00532920): a row per value, its label at the left and
    /// its button at the right.
    fn options_starting(&mut self, ui: &mut Ui, o: &mut Options, x: f32, y: f32) {
        panel::outer_panel(ui.r, ui.panels, x, y, 30, 34);
        let title = ui.t(44, 88);
        ui.label(Font::LargeBlackOnLight, &title, x + 32.0, y + 12.0);
        let foot = ui.t(13, 3);
        ui.centred(Font::NormalBlackOnLight, &foot, x + 12.0, y + 507.0, 480.0);
        let info = self.scenario.info.clone();
        let rows: [(f32, usize, Field, String); 10] = [
            (52.0, 108, Field::Rank, ui.t(32, info.player_rank.max(0) as usize)),
            (92.0, 89, Field::Year, year(ui, info.start_year as i32)),
            (132.0, 39, Field::Funds, info.initial_funds.to_string()),
            (172.0, 68, Field::Gift, info.rescue_loan.to_string()),
            (212.0, 91, Field::Milestone(0), format!("+{} {}", info.win.milestone_years[0], year(ui, info.start_year as i32 + info.win.milestone_years[0]))),
            (252.0, 92, Field::Milestone(1), format!("+{} {}", info.win.milestone_years[1], year(ui, info.start_year as i32 + info.win.milestone_years[1]))),
            (292.0, 93, Field::Milestone(2), format!("+{} {}", info.win.milestone_years[2], year(ui, info.start_year as i32 + info.win.milestone_years[2]))),
            (332.0, 177, Field::Interest, format!("{}%", info.debt_interest_rate)),
            (372.0, 202, Field::Pharaoh, ui.t(151, info.current_pharaoh as usize)),
            (412.0, 203, Field::Incarnation, ui.t(152, info.player_incarnation as usize)),
        ];
        for (row, label, field, value) in rows {
            let l = ui.t(44, label);
            ui.label(Font::NormalBlackOnLight, &l, x + 32.0, y + row + 9.0);
            if ui.button([x + 262.0, y + row, 200.0, 30.0], &value, Font::NormalBlackOnLight) {
                match field {
                    Field::Rank => o.picker = Some(Picker { field, group: 32, ids: (0..=10).collect(), page: 0 }),
                    Field::Pharaoh => o.picker = Some(Picker { field, group: 151, ids: (0..125).collect(), page: 0 }),
                    Field::Incarnation => o.picker = Some(Picker { field, group: 152, ids: (0..=30).collect(), page: 0 }),
                    Field::Year => o.page = Page::StartDate,
                    _ => o.keypad = Some(Keypad::new(field, number(&self.scenario, field))),
                }
            }
        }
    }

    /// The start date: BC or AD, and the year.
    fn options_start_date(&mut self, ui: &mut Ui, o: &mut Options, x: f32, y: f32) {
        let (px, py) = (x + 160.0, y + 160.0);
        panel::outer_panel(ui.r, ui.panels, px, py, 20, 8);
        let title = ui.t(44, 13);
        ui.centred(Font::LargeBlackOnLight, &title, px, py + 14.0, 320.0);
        let y0 = self.scenario.info.start_year;
        let era = ui.t(20, if y0 < 0 { 0 } else { 1 });
        if ui.button([px + 24.0, py + 56.0, 100.0, 30.0], &era, Font::NormalBlackOnLight) {
            self.scenario.info.start_year = -y0;
        }
        if ui.button([px + 140.0, py + 56.0, 156.0, 30.0], &y0.abs().to_string(), Font::NormalBlackOnLight) {
            o.keypad = Some(Keypad::new(Field::Year, y0.abs() as i32));
        }
        let foot = ui.t(13, 3);
        ui.centred(Font::NormalBlackOnLight, &foot, px, py + 100.0, 320.0);
    }

    /// Win criteria (FUN_00533470): each goal's switch and its value.
    fn options_win(&mut self, ui: &mut Ui, o: &mut Options, x: f32, y: f32) {
        let (x16, y32) = (x + 16.0, y + 32.0);
        panel::outer_panel(ui.r, ui.panels, x16, y32, 38, 28);
        let title = ui.t(44, 48);
        ui.label(Font::LargeBlackOnLight, &title, x + 26.0, y + 42.0);
        let foot = ui.t(13, 3);
        ui.centred(Font::NormalBlackOnLight, &foot, x16, y + 456.0, 608.0);
        let yes_no = |ui: &Ui, on: bool| ui.t(18, on as usize);
        let s = &mut self.scenario.info;
        let open = s.is_open_play;
        let w = s.win.clone();
        let row = |ui: &mut Ui, r: f32, label: usize| {
            let l = ui.t(44, label);
            ui.label(Font::NormalBlackOnLight, &l, x + 66.0, y + r + 9.0);
        };
        let a = |r: f32| [x + 316.0, y + r, 80.0, 30.0];
        let b = |r: f32| [x + 416.0, y + r, 180.0, 30.0];
        // Open play, and its number that does nothing (the guide: "just there for
        // show"): shown as N/A rather than the raw byte.
        row(ui, 92.0, 107);
        if ui.button(a(92.0), &yes_no(ui, open), Font::NormalBlackOnLight) {
            s.is_open_play = !open;
        }
        grey_button(ui, b(92.0), &ui.t(18, 6));
        // Housing needs both a count and a level; under open play (or with no count
        // set) it shows "No" like the other goals instead of a count that means
        // nothing.
        row(ui, 132.0, 210);
        let housing_on = w.housing_count.enabled && !open;
        let count_label = if housing_on { w.housing_count.value.to_string() } else { yes_no(ui, false) };
        if ui.button(a(132.0), &count_label, Font::NormalBlackOnLight) {
            o.keypad = Some(Keypad::new(Field::HousingCount, w.housing_count.value));
        }
        let level = w.housing_level.value.clamp(0, 19) as usize + if w.housing_count.value > 1 { 20 } else { 0 };
        if ui.button(b(132.0), &ui.t(29, level), Font::NormalBlackOnLight) {
            o.picker = Some(Picker { field: Field::HousingLevel, group: 29, ids: (0..20).collect(), page: 0 });
        }
        let goals = [(172.0, 50, Field::Culture), (212.0, 51, Field::Prosperity), (292.0, 53, Field::Kingdom)];
        for (r, label, field) in goals {
            row(ui, r, label);
            let g = match field {
                Field::Culture => &mut s.win.culture,
                Field::Prosperity => &mut s.win.prosperity,
                _ => &mut s.win.kingdom,
            };
            if ui.button(a(r), &yes_no(ui, g.enabled && !open), Font::NormalBlackOnLight) {
                g.enabled = !g.enabled;
            }
            if ui.button(b(r), &g.value.to_string(), Font::NormalBlackOnLight) {
                o.keypad = Some(Keypad::new(field, g.value));
            }
        }
        row(ui, 252.0, 52);
        let count = s.monuments.iter().filter(|&&m| m != 0).count();
        ui.button(a(252.0), &count.to_string(), Font::NormalBlackOnLight);
        if ui.button(b(252.0), &ui.t(44, 201), Font::NormalBlackOnLight) {
            o.page = Page::Monuments;
        }
        let start = s.start_year as i32;
        let timed = [(332.0, 54, Field::TimeLimit), (372.0, 55, Field::Survival)];
        for (r, label, field) in timed {
            row(ui, r, label);
            let g = if field == Field::TimeLimit { &mut s.win.time_limit } else { &mut s.win.survival_time };
            let on = g.enabled && !open;
            if ui.button(a(r), &yes_no(ui, on), Font::NormalBlackOnLight) {
                g.enabled = !g.enabled;
            }
            let v = g.value;
            let text = format!("+{v} {}", year(ui, start + v));
            let clicked = if on { ui.button(b(r), &text, Font::NormalBlackOnLight) } else { grey_button(ui, b(r), &text) };
            if clicked {
                o.keypad = Some(Keypad::new(field, v));
            }
        }
        row(ui, 412.0, 56);
        let g = &mut s.win.population;
        if ui.button(a(412.0), &yes_no(ui, g.enabled && !open), Font::NormalBlackOnLight) {
            g.enabled = !g.enabled;
        }
        if ui.button(b(412.0), &g.value.to_string(), Font::NormalBlackOnLight) {
            o.keypad = Some(Keypad::new(Field::Population, g.value));
        }
    }

    /// Select monuments (FUN_005374a0): the era, three monuments that fit it, the
    /// rating they make, and for a tomb the burial provisions it must be sent.
    fn options_monuments(&mut self, ui: &mut Ui, o: &mut Options, x: f32, y: f32) {
        panel::outer_panel(ui.r, ui.panels, x, y, 40, 28);
        let foot = ui.t(13, 3);
        ui.centred(Font::NormalBlackOnLight, &foot, x, y + 422.0, 608.0);
        let era = self.era;
        // Pharaoh's own maps hold 255 here (from before eras), which text group 311
        // has no entry for; show its own "Select Monument Era" line instead of nothing.
        let era_label = ui.t(311, if (1..=4).contains(&era) { era as usize } else { 0 });
        if ui.button([x + 10.0, y + 22.0, 270.0, 20.0], &era_label, Font::NormalBlackOnLight) {
            self.era = era % 4 + 1;
            // Monuments the new era doesn't allow go.
            let e = self.era;
            for m in self.scenario.info.monuments.iter_mut() {
                if !fits_era(*m as usize, e) {
                    *m = 0;
                }
            }
            let first = self.scenario.info.monuments[0] as usize;
            pick(&mut self.scenario, Field::Monument(0), first);
        }
        for n in 0..3 {
            let m = self.scenario.info.monuments[n] as usize;
            if ui.button([x + 10.0, y + 70.0 + 24.0 * n as f32, 270.0, 20.0], &ui.t(198, m), Font::NormalBlackOnLight) {
                let ids = std::iter::once(0).chain((1..MONUMENTS.len()).filter(|&m| fits_era(m, self.era))).collect();
                o.picker = Some(Picker { field: Field::Monument(n), group: 198, ids, page: 0 });
            }
        }
        panel::button_border(ui.r, ui.panels, x + 10.0, y + 166.0, 270, 100, false);
        let label = ui.t(199, 11);
        ui.centred(Font::NormalBlackOnLight, &label, x + 10.0, y + 190.0, 270.0);
        let rating = monument_rating(&self.scenario.info);
        ui.centred(Font::LargeBlackOnLight, &rating.to_string(), x + 10.0, y + 214.0, 270.0);
        if !self.scenario.info.monuments.iter().any(|&m| is_tomb(m as usize)) {
            let p = &mut self.scenario.info.burial_provisions_required;
            p.iter_mut().for_each(|v| *v = 0);
            // The right half is otherwise blank while no tomb is chosen: say why.
            let hint = "Burial provisions appear here once one of the three monuments above is a tomb.";
            ui.wrapped(Font::NormalBlackOnLight, hint, x + 300.0, y + 70.0, 260.0);
            return;
        }
        let head = ui.t(44, 213);
        ui.label(Font::NormalBlackOnLight, &head, x + 290.0, y + 400.0);
        for (i, &(res, _)) in PROVISIONS.iter().enumerate() {
            let ry = y + 25.0 + 25.0 * i as f32;
            let rect = [x + 375.0, ry, 225.0, 25.0];
            if ui.hot(rect) {
                panel::button_border(ui.r, ui.panels, rect[0], rect[1], rect[2] as i32, rect[3] as i32, true);
            }
            ui.icon(res as u16, x + 350.0, ry);
            ui.icon(res as u16, x + 600.0, ry);
            let v = self.scenario.info.burial_provisions_required.get(res).copied().unwrap_or(0);
            ui.label(Font::NormalBlackOnLight, &v.to_string(), x + 385.0, ry + 4.0);
            let name = ui.t(23, res);
            ui.label(Font::NormalBlackOnLight, &name, x + 450.0, ry + 4.0);
            if ui.clicked(rect) {
                o.keypad = Some(Keypad::new(Field::Provision(res), v as i32));
            }
        }
    }

    /// Buildings allowed (FUN_00532fe0): the structures of text group 67 in three
    /// columns; those left out show in yellow.
    fn options_allowed(&mut self, ui: &mut Ui, x: f32, y: f32) {
        let (x16, y32) = (x + 16.0, y + 32.0);
        panel::outer_panel(ui.r, ui.panels, x16, y32, 38, 26);
        let title = ui.t(44, 47);
        ui.label(Font::LargeBlackOnLight, &title, x + 26.0, y + 42.0);
        let foot = ui.t(13, 3);
        ui.centred(Font::NormalBlackOnLight, &foot, x16, y + 424.0, 608.0);
        let reserved = &mut self.scenario.info.reserved;
        if reserved.len() < 114 {
            reserved.resize(114, 0);
        }
        for (i, id) in (2..46usize).enumerate() {
            let (col, row) = if i < 16 { (9.0, i) } else if i < 32 { (208.0, i - 16) } else { (407.0, i - 32) };
            let rect = [x16 + col, y32 + 50.0 + 20.0 * row as f32, 190.0, 18.0];
            if ui.hot(rect) {
                ui.r.rect([rect[0], rect[1]], [rect[2], rect[3]], [1.0, 0.35, 0.03, 0.35], Space::Screen);
            }
            let allowed = reserved[id] != 0;
            let f = if allowed { Font::NormalBlackOnLight } else { Font::NormalYellow };
            let name = ui.t(67, id);
            ui.centred(f, &name, rect[0], rect[1] + 4.0, 190.0);
            if ui.clicked(rect) {
                reserved[id] = !allowed as i16;
            }
        }
    }

    /// Gods settings (FUN_00537260): each god's standing, and whether a temple complex
    /// may be built to it (never for an unknown god).
    fn options_gods(&mut self, ui: &mut Ui, x: f32, y: f32) {
        panel::outer_panel(ui.r, ui.panels, x, y, 21, 14);
        let title = ui.t(44, 164);
        ui.label(Font::LargeBlackOnLight, &title, x + 10.0, y + 10.0);
        let foot = ui.t(13, 3);
        ui.centred(Font::NormalBlackOnLight, &foot, x, y + 200.0, 336.0);
        for (label, hx) in [(174, 20.0), (175, 132.0), (176, 197.0)] {
            let l = ui.t(44, label);
            ui.label(Font::NormalBlackOnLight, &l, x + hx, y + 46.0);
        }
        let info = &mut self.scenario.info;
        if info.reserved.len() < 114 {
            info.reserved.resize(114, 0);
        }
        for g in 0..5 {
            let gy = y + 70.0 + 24.0 * g as f32;
            let name = ui.t(157, g);
            ui.label(Font::NormalWhiteOnDark, &name, x + 20.0, gy + 4.0);
            let rank = info.gods[g].min(2);
            if ui.button([x + 100.0, gy, 100.0, 20.0], &ui.t(187, rank as usize), Font::NormalBlackOnLight) {
                let next = (rank + 1) % 3;
                // One patron god at most.
                if next == 2 {
                    for o in info.gods.iter_mut().filter(|v| **v == 2) {
                        *o = 1;
                    }
                }
                info.gods[g] = next;
                info.gods_known[g] = next != 0;
                if next == 0 {
                    info.reserved[104 + g] = 0;
                }
            }
            let complex = info.reserved[104 + g] != 0;
            if ui.button([x + 205.0, gy, 95.0, 20.0], &ui.t(18, complex as usize), Font::NormalBlackOnLight) && info.gods[g] != 0 {
                info.reserved[104 + g] = !complex as i16;
            }
        }
    }

    /// Flood plain settings (FUN_00530780): when the Nile floods, for how long, and
    /// how well; each button steps through its choices.
    fn options_flood(&mut self, ui: &mut Ui, x: f32, y: f32) {
        let py = y + 80.0;
        panel::outer_panel(ui.r, ui.panels, x, py, 30, 18);
        let title = ui.t(44, 178);
        ui.centred(Font::LargeBlackOnLight, &title, x, y + 100.0, 480.0);
        let foot = ui.t(13, 3);
        ui.centred(Font::NormalBlackOnLight, &foot, x, y + 312.0, 480.0);
        let f = &mut self.scenario.floodplain_settings;
        if f.len() < 12 {
            f.resize(12, 0);
        }
        let get = |f: &[u8], o: usize| i32::from_le_bytes(f[o..o + 4].try_into().unwrap());
        let (season, duration, quality) = (get(f, 0).clamp(150, 255), get(f, 4).clamp(60, 120), get(f, 8).clamp(0, 100));
        let rows = [
            (75.0, 179, 170 + season / 15, 0usize, if season >= 255 { 150 } else { season + 15 }),
            (130.0, 188, 187 + duration / 30, 4, if duration >= 120 { 60 } else { duration + 30 }),
            (185.0, 192, 193 + quality / 20, 8, if quality >= 100 { 0 } else { quality + 20 }),
        ];
        for (ry, label, value, at, next) in rows {
            let l = ui.t(44, label);
            ui.label(Font::NormalBlackOnDark, &l, x + 60.0, py + ry + 8.0);
            if ui.button([x + 220.0, py + ry, 200.0, 30.0], &ui.t(44, value as usize), Font::NormalBlackOnLight) {
                f[at..at + 4].copy_from_slice(&next.to_le_bytes());
            }
        }
    }

    /// A list to pick from: three columns of up to 21 entries a page, the page turned
    /// with the arrows below.
    fn draw_picker(&mut self, ui: &mut Ui, o: &mut Options) {
        let Some(p) = &mut o.picker else { return };
        const PER_PAGE: usize = 63;
        let pages = p.ids.len().div_ceil(PER_PAGE).max(1);
        let shown: Vec<usize> = p.ids.iter().copied().skip(p.page * PER_PAGE).take(PER_PAGE).collect();
        let cols = shown.len().div_ceil(21).max(1);
        let rows = shown.len().min(21);
        let (w, h) = (cols * 12, rows.div_ceil(1) as i32 + 4);
        let (sw, sh) = (ui.r.screen[0], ui.r.screen[1]);
        let pw = w as f32 * 16.0;
        let ph = (rows as f32 * 18.0 + if pages > 1 { 64.0 } else { 32.0 }).max(64.0);
        let (x, y) = (((sw - pw) / 2.0).floor(), ((sh - ph) / 2.0).floor().max(24.0));
        panel::outer_panel(ui.r, ui.panels, x, y, w as i32, (ph / 16.0).ceil() as i32);
        let _ = h;
        let mut chosen = None;
        for (i, &id) in shown.iter().enumerate() {
            let (c, r) = (i / 21, i % 21);
            let rect = [x + 8.0 + 192.0 * c as f32, y + 16.0 + 18.0 * r as f32, 176.0, 18.0];
            let label = ui.t(p.group, id);
            let f = if ui.hot(rect) { Font::NormalYellow } else { Font::NormalBlackOnLight };
            ui.centred(f, label.trim(), rect[0], rect[1] + 3.0, rect[2]);
            if ui.clicked(rect) {
                chosen = Some(id);
            }
        }
        if pages > 1 {
            let by = y + ph - 40.0;
            if ui.arrow(x + pw / 2.0 - 30.0, by, true) {
                p.page = (p.page + pages - 1) % pages;
            }
            if ui.arrow(x + pw / 2.0 + 6.0, by, false) {
                p.page = (p.page + 1) % pages;
            }
        }
        if let Some(id) = chosen {
            let field = p.field;
            o.picker = None;
            pick(&mut self.scenario, field, id);
        } else if ui.click.is_some_and(|c| !inside([x, y, pw, ph], c)) {
            o.picker = None;
        }
    }

    /// The keypad: digits, and Accept or Cancel (text 44/16, 44/17); the keyboard's
    /// digits, Backspace and Enter work too.
    fn draw_keypad(&mut self, ui: &mut Ui, o: &mut Options) {
        let Some(k) = &mut o.keypad else { return };
        let (sw, sh) = (ui.r.screen[0], ui.r.screen[1]);
        let (x, y) = (((sw - 208.0) / 2.0).floor(), ((sh - 256.0) / 2.0).floor());
        panel::outer_panel(ui.r, ui.panels, x, y, 13, 16);
        panel::inner_panel(ui.r, ui.panels, x + 24.0, y + 16.0, 10, 2);
        let tw = ui.width(Font::NormalWhiteOnDark, &k.typed);
        ui.label(Font::NormalWhiteOnDark, &k.typed, x + 176.0 - tw, y + 26.0);
        let keys = ["7", "8", "9", "4", "5", "6", "1", "2", "3", "0"];
        for (i, key) in keys.iter().enumerate() {
            let (c, r) = if i == 9 { (1, 3) } else { (i % 3, i / 3) };
            let rect = [x + 32.0 + 50.0 * c as f32, y + 60.0 + 36.0 * r as f32, 42.0, 30.0];
            if ui.button(rect, key, Font::LargeBlackOnLight) {
                k.digit(key.chars().next().unwrap_or('0'));
            }
        }
        let accept = ui.t(44, 16);
        let cancel = ui.t(44, 17);
        let ok = ui.button([x + 16.0, y + 212.0, 84.0, 25.0], &accept, Font::NormalBlackOnLight);
        let no = ui.button([x + 108.0, y + 212.0, 84.0, 25.0], &cancel, Font::NormalBlackOnLight);
        if ok {
            let (field, v) = (k.field, k.typed.parse().unwrap_or(0));
            o.keypad = None;
            set_number(&mut self.scenario, field, v);
        } else if no {
            o.keypad = None;
        }
        let _ = WHITE;
    }
}
