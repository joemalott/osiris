//! The overseers: full-screen reports on each part of the city, reached from the
//! sidebar or the menu bar. A 640x432 panel sits on a backdrop above a strip of
//! thirteen overseer buttons and a Back button.
//!
//! The screens are drawn immediate-mode: a click is held until the next draw, and the
//! widget it lands on acts on it, so each screen's layout lives in one place.

use osiris_formats::{ImageLibrary, TextTable};
use osiris_render::{Renderer, Space};
use osiris_sim::World;
use osiris_sim::buildings::kind;
use osiris_sim::trade::status;
use crate::widgets::{Ui, UiImages, inside};
use osiris_ui::{Font, PanelImages, draw_text, draw_text_tinted, font, panel};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Advisor {
    Labor,
    Military,
    Political,
    Ratings,
    Trade,
    Population,
    Health,
    Education,
    Entertainment,
    Religion,
    Financial,
    Chief,
    Monuments,
}

/// Button strip: each overseer's button x offset and width on the strip.
const STRIP: [(Advisor, f32, f32); 13] = [
    (Advisor::Labor, 12.0, 33.0),
    (Advisor::Military, 52.0, 39.0),
    (Advisor::Political, 96.0, 34.0),
    (Advisor::Ratings, 135.0, 38.0),
    (Advisor::Trade, 178.0, 46.0),
    (Advisor::Population, 229.0, 48.0),
    (Advisor::Health, 282.0, 35.0),
    (Advisor::Education, 322.0, 38.0),
    (Advisor::Entertainment, 363.0, 39.0),
    (Advisor::Religion, 406.0, 35.0),
    (Advisor::Financial, 445.0, 40.0),
    (Advisor::Chief, 490.0, 46.0),
    (Advisor::Monuments, 542.0, 40.0),
];
const BACK: (f32, f32) = (588.0, 40.0);

/// Every overseer, in the strip's order.
pub const ALL: [Advisor; 13] = [
    Advisor::Labor,
    Advisor::Military,
    Advisor::Political,
    Advisor::Ratings,
    Advisor::Trade,
    Advisor::Population,
    Advisor::Health,
    Advisor::Education,
    Advisor::Entertainment,
    Advisor::Religion,
    Advisor::Financial,
    Advisor::Chief,
    Advisor::Monuments,
];
const TOOLTIPS: usize = 68;
const RESOURCE_NAMES: usize = 23;

impl Advisor {
    fn index(self) -> usize {
        STRIP.iter().position(|s| s.0 == self).expect("in strip")
    }

    /// Whether Osiris has this screen yet.
    pub fn available(self) -> bool {
        true
    }
}

#[derive(Clone, Copy)]
pub struct AdvisorImages {
    backdrop: u32,
    strip: u32,
    buttons: u32,
    icons: u32,
    lock: u32,
}

impl AdvisorImages {
    pub fn load(lib: &ImageLibrary) -> osiris_formats::Result<Self> {
        Ok(Self {
            backdrop: lib.group_id("Pharaoh_Unloaded", 11, 0)?,
            strip: lib.group_id("Pharaoh_General", 160, 0)?,
            buttons: lib.group_id("Pharaoh_General", 159, 0)?,
            icons: lib.group_id("Pharaoh_General", 128, 0)?,
            lock: lib.group_id("Pharaoh_General", 94, 0)?,
        })
    }
}

/// What the overseers ask of the game.
pub enum AdvisorAction {
    Close,
    OpenEmpire,
    /// Take command of a company and look at it.
    GoToCompany(usize),
}

/// A popup over an overseer screen.
enum Popup {
    /// Trade settings for one resource.
    Resource(u16),
    /// Priority for one labor category.
    Priority(usize),
    /// The price list.
    Prices,
    /// Choosing a festival: its god, then its size.
    Festival(Option<usize>),
    /// Asking before sending what a request wants, or saying there is not enough.
    Request(usize, bool),
    /// Choosing the governor's salary.
    Salary,
    /// Choosing a gift to the Kingdom.
    Gift,
    /// Giving part of the governor's savings to the city: the amount so far.
    Donate(i32),
    /// Dispatching a burial provision: the amount so far, in hundreds.
    Burial(u16, i32),
}

impl Advisors {
    /// Opens a named popup (for scripted screenshots).
    pub fn open_popup(&mut self, name: &str) {
        self.popup = match name {
            "salary" => Some(Popup::Salary),
            "gift" => Some(Popup::Gift),
            "donate" => Some(Popup::Donate(0)),
            "burial" => Some(Popup::Burial(13, 0)),
            _ => None,
        };
    }
}

pub struct Advisors {
    pub current: Advisor,
    popup: Option<Popup>,
    click: Option<[f32; 2]>,
    cursor: [f32; 2],
    scroll: usize,
}

impl Advisors {
    pub fn new(current: Advisor) -> Self {
        Self { current, popup: None, click: None, cursor: [0.0; 2], scroll: 0 }
    }

    fn panel_origin(screen: [f32; 2]) -> [f32; 2] {
        [((screen[0] - 640.0) / 2.0).floor(), ((screen[1] - 480.0) / 2.0).floor()]
    }

    pub fn hover(&mut self, p: [f32; 2]) {
        self.cursor = p;
    }

    pub fn press(&mut self, p: [f32; 2]) {
        self.click = Some(p);
    }

    pub fn scroll(&mut self, lines: i32) {
        self.scroll = (self.scroll as i32 + lines).max(0) as usize;
    }

    /// Right-click: closes a popup, or the overseers.
    pub fn back(&mut self) -> bool {
        self.popup.take().is_none()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw(&mut self, r: &mut Renderer, panels: &PanelImages, img: AdvisorImages, ui_img: UiImages, world: &mut World, text: &TextTable) -> Option<AdvisorAction> {
        let screen = r.screen;
        let mut ui = Ui { r, panels, img: ui_img, text, cursor: self.cursor, click: self.click.take() };
        ui.r.rect([0.0, 0.0], screen, [0.0, 0.0, 0.0, 1.0], Space::Screen);
        ui.image(img.backdrop, ((screen[0] - 1024.0) / 2.0).floor(), ((screen[1] - 768.0) / 2.0).floor());
        // A popup takes every click while it is open.
        let popup_open = self.popup.is_some();
        let held = if popup_open { ui.click.take() } else { None };
        let mut action = None;
        let [px, py] = Self::panel_origin(screen);
        let strip_x = ((screen[0] - 640.0) / 2.0).floor();
        let strip_y = ((screen[1] + 400.0) / 2.0).floor();
        let button_y = ((screen[1] + 418.0) / 2.0).floor();
        ui.image(img.strip, strip_x, strip_y);
        let mut switch_to = None;
        for (i, &(a, x, w)) in STRIP.iter().enumerate() {
            let rect = [strip_x + x, button_y, w, 32.0];
            let id = img.buttons + 4 * i as u32;
            if !a.available() {
                // Not in Osiris yet: the plain button, greyed.
                ui.r.image(id, [rect[0], rect[1]], [0.45, 0.45, 0.45, 1.0], Space::Screen);
            } else {
                let frame = if a == self.current { 2 } else { ui.hot(rect) as u32 };
                ui.image(id + frame, rect[0], rect[1]);
            }
            if a.available() && ui.clicked(rect) {
                switch_to = Some(a);
            }
        }
        let back = [strip_x + BACK.0, button_y, BACK.1, 32.0];
        ui.image(img.buttons + 52 + ui.hot(back) as u32, back[0], back[1]);
        if ui.clicked(back) {
            action = Some(AdvisorAction::Close);
        }
        for &(a, x, w) in &STRIP {
            if ui.hot([strip_x + x, button_y, w, 32.0]) {
                let tip = ui.t(TOOLTIPS, 71 + a.index());
                let tw = ui.width(Font::SmallPlain, &tip);
                let tx = (strip_x + x).min(screen[0] - tw - 12.0);
                ui.r.rect([tx - 4.0, button_y - 22.0], [tw + 8.0, 18.0], [1.0, 1.0, 0.85, 1.0], Space::Screen);
                ui.label(Font::SmallPlain, &tip, tx, button_y - 20.0);
            }
        }
        panel::outer_panel(ui.r, panels, px, py, 40, 27);
        ui.image(img.icons + self.current.index() as u32, px + 10.0, py + 10.0);
        let from_screen = match self.current {
            Advisor::Labor => labor(&mut ui, img.lock, world, [px, py], &mut self.popup),
            Advisor::Trade => trade(&mut ui, world, [px, py], &mut self.popup, &mut self.scroll),
            Advisor::Financial => financial(&mut ui, world, [px, py]),
            Advisor::Chief => chief(&mut ui, world, [px, py]),
            Advisor::Ratings => ratings(&mut ui, world, [px, py], &mut self.scroll),
            Advisor::Religion => religion(&mut ui, world, [px, py], &mut self.popup),
            Advisor::Entertainment => entertainment(&mut ui, world, [px, py]),
            Advisor::Education => education(&mut ui, world, [px, py]),
            Advisor::Health => health(&mut ui, world, [px, py]),
            Advisor::Population => population(&mut ui, world, [px, py], &mut self.scroll),
            Advisor::Political => political(&mut ui, world, [px, py], &mut self.popup),
            Advisor::Military => military(&mut ui, world, [px, py]),
            Advisor::Monuments => monuments(&mut ui, world, [px, py], &mut self.popup),
        };
        action = action.or(from_screen);
        if popup_open {
            ui.click = held;
            let closed = match self.popup {
                Some(Popup::Resource(r)) => resource_popup(&mut ui, world, r),
                Some(Popup::Priority(c)) => priority_popup(&mut ui, world, c),
                Some(Popup::Prices) => prices_popup(&mut ui, world),
                Some(Popup::Festival(god)) => match festival_popup(&mut ui, world, god) {
                    FestivalChoice::Close => true,
                    FestivalChoice::God(g) => {
                        self.popup = Some(Popup::Festival(Some(g)));
                        false
                    }
                    FestivalChoice::Stay => false,
                },
                Some(Popup::Request(i, ok)) => request_popup(&mut ui, world, i, ok),
                Some(Popup::Salary) => salary_popup(&mut ui, world),
                Some(Popup::Gift) => gift_popup(&mut ui, world),
                Some(Popup::Burial(r, n)) => match burial_popup(&mut ui, world, r, n) {
                    Some(n) => {
                        self.popup = Some(Popup::Burial(r, n));
                        false
                    }
                    None => true,
                },
                Some(Popup::Donate(n)) => match donate_popup(&mut ui, world, n) {
                    Some(n) => {
                        self.popup = Some(Popup::Donate(n));
                        false
                    }
                    None => true,
                },
                None => false,
            };
            if closed {
                self.popup = None;
            }
        }
        if let Some(a) = switch_to {
            self.current = a;
            self.popup = None;
            self.scroll = 0;
        }
        action
    }
}

/// Labor categories in the overseer's order: text group 50 ids 1-9 and the sim's
/// category names.
const LABOR_ROWS: [(usize, &str); 9] = [
    (1, "food_production"),
    (2, "industry_commerce"),
    (3, "entertainment"),
    (4, "religion"),
    (5, "education"),
    (6, "water_health"),
    (7, "infrastructure"),
    (8, "government"),
    (9, "military"),
];

fn labor(ui: &mut Ui, lock: u32, world: &mut World, [px, py]: [f32; 2], popup: &mut Option<Popup>) -> Option<AdvisorAction> {
    const G: usize = 50;
    let title = ui.t(G, 0);
    ui.label(Font::LargeBlackOnLight, &title, px + 60.0, py + 12.0);
    for (x, id) in [(60.0, 21), (170.0, 22), (400.0, 23), (500.0, 24)] {
        let s = ui.t(G, id);
        ui.label(Font::SmallPlain, &s, px + x, py + 46.0);
    }
    panel::inner_panel(ui.r, ui.panels, px + 32.0, py + 65.0, 36, 16);
    for (i, &(id, key)) in LABOR_ROWS.iter().enumerate() {
        let y = py + 67.0 + 25.0 * i as f32;
        let row = [px + 40.0, y, 560.0, 25.0];
        let Some(ci) = osiris_sim::labor::CATEGORIES.iter().position(|c| *c == key) else { continue };
        let (need, have) = world.labor.by_category.get(ci).copied().unwrap_or((0, 0));
        let prio = world.labor.priorities.get(ci).copied().unwrap_or(0);
        if prio > 0 {
            ui.image(lock, row[0], y + 4.0);
            draw_text(ui.r, Font::NormalWhiteOnDark, &prio.to_string(), row[0] + 15.0, y + 5.0, font::WHITE);
        }
        let name = ui.t(G, id);
        let f = if ui.hot(row) { Font::NormalYellow } else { Font::NormalWhiteOnDark };
        draw_text(ui.r, f, &name, row[0] + 60.0, y + 5.0, font::WHITE);
        draw_text(ui.r, Font::NormalWhiteOnDark, &need.to_string(), row[0] + 330.0, y + 5.0, font::WHITE);
        let hf = if have == need { Font::NormalWhiteOnDark } else { Font::NormalYellow };
        draw_text(ui.r, hf, &have.to_string(), row[0] + 430.0, y + 5.0, font::WHITE);
        if ui.clicked(row) {
            *popup = Some(Popup::Priority(ci));
        }
    }
    let l = &world.labor;
    let pct = if l.available > 0 { l.unemployed * 100 / l.available } else { 0 };
    let employed = format!("{} {} {} {}{}%)", l.employed, ui.t(G, 12), l.unemployed, ui.t(G, 13), pct);
    ui.label(Font::NormalBlackOnLight, &employed, px + 32.0, py + 325.0);
    panel::inner_panel(ui.r, ui.panels, px + 64.0, py + 350.0, 32, 2);
    let wage_title = ui.t(G, 14);
    draw_text(ui.r, Font::NormalWhiteOnDark, &wage_title, px + 70.0, py + 359.0, font::WHITE);
    if ui.arrow(px + 158.0, py + 354.0, false) {
        world.finance.wages = (world.finance.wages - 1).max(0);
    }
    if ui.arrow(px + 182.0, py + 354.0, true) {
        world.finance.wages = (world.finance.wages + 1).min(100);
    }
    let wages = format!("{} {} {} {})", world.finance.wages, ui.t(G, 15), ui.t(G, 18), world.finance.kingdom_wages);
    draw_text(ui.r, Font::NormalWhiteOnDark, &wages, px + 230.0, py + 359.0, font::WHITE);
    None
}

fn priority_popup(ui: &mut Ui, world: &mut World, category: usize) -> bool {
    const G: usize = 50;
    let screen = ui.r.screen;
    let (w, h) = (416.0, 144.0);
    let (x, y) = (((screen[0] - w) / 2.0).floor(), ((screen[1] - h) / 2.0).floor());
    panel::outer_panel(ui.r, ui.panels, x, y, 26, 9);
    let title = ui.t(G, 25);
    let tw = ui.width(Font::LargeBlackOnLight, &title);
    ui.label(Font::LargeBlackOnLight, &title, x + (w - tw) / 2.0, y + 16.0);
    let ranks = osiris_sim::labor::MAX_PRIORITY as usize;
    let x0 = x + (w - 34.0 * ranks as f32) / 2.0;
    for i in 0..ranks {
        let rect = [x0 + 34.0 * i as f32, y + 50.0, 30.0, 30.0];
        if ui.button(rect, &(i + 1).to_string(), Font::LargeBlackOnLight) {
            world.set_labor_priority(category, i as u8 + 1);
            return true;
        }
    }
    let none = ui.t(G, 26);
    if ui.button([x + (w - 200.0) / 2.0, y + 100.0, 200.0, 25.0], &none, Font::NormalBlackOnLight) {
        world.set_labor_priority(category, 0);
        return true;
    }
    ui.click.take().is_some_and(|c| !inside([x, y, w, h], c))
}

/// Resources that belong on the trade list: anything this city can make, store or
/// trade.
fn trade_resources(world: &World) -> Vec<u16> {
    (1..osiris_sim::trade::RESOURCES as u16)
        .filter(|&r| {
            let traded = world.trade.cities.iter().any(|c| c.trades() && (c.sells[r as usize] || c.buys[r as usize]));
            let made = world.buildings.iter().any(|b| world.defs.building(b.kind).is_some_and(|d| d.outputs.iter().any(|o| world.resource_id(o) == Some(r))));
            let allowed = world.defs.buildings.iter().flatten().any(|d| world.is_allowed(d.id) && d.outputs.iter().any(|o| world.resource_id(o) == Some(r)));
            traded || made || allowed || world.yards_stored(r) > 0
        })
        .collect()
}

fn trade(ui: &mut Ui, world: &mut World, [px, py]: [f32; 2], popup: &mut Option<Popup>, scroll: &mut usize) -> Option<AdvisorAction> {
    const G: usize = 54;
    let title = ui.t(G, 0);
    ui.label(Font::LargeBlackOnLight, &title, px + 60.0, py + 17.0);
    let hint = ui.t(G, 1);
    ui.label(Font::NormalBlackOnLight, &hint, px + 60.0, py + 40.0);
    panel::inner_panel(ui.r, ui.panels, px + 17.0, py + 60.0, 36, 21);
    let list = trade_resources(world);
    const ROWS: usize = 15;
    *scroll = (*scroll).min(list.len().saturating_sub(ROWS));
    for (row, &r) in list.iter().skip(*scroll).take(ROWS).enumerate() {
        let y = py + 68.0 + 22.0 * row as f32;
        let rect = [px + 20.0, y - 2.0, 570.0, 22.0];
        ui.icon(r, px + 28.0, y - 2.0);
        let name = ui.t(RESOURCE_NAMES, r as usize);
        let hot = ui.hot(rect);
        let nf = if world.is_mothballed(r) || hot { Font::NormalYellow } else { Font::NormalWhiteOnDark };
        draw_text(ui.r, nf, &name, px + 52.0, y, font::WHITE);
        draw_text(ui.r, Font::NormalWhiteOnDark, &world.yards_stored(r).to_string(), px + 206.0, y, font::WHITE);
        if world.is_stockpiled(r) {
            let s = ui.t(G, 3);
            draw_text(ui.r, Font::SmallPlain, &s, px + 246.0, y + 2.0, [0.9, 0.85, 0.6, 1.0]);
        } else if world.is_mothballed(r) {
            let s = ui.t(18, 5);
            draw_text(ui.r, Font::NormalYellow, &s, px + 246.0, y, font::WHITE);
        }
        let st = world.trade.status[r as usize];
        let amount = world.trade.amount[r as usize];
        let (s, dull) = match st {
            status::IMPORT => (format!("{} {}", ui.t(G, 5), amount), false),
            status::EXPORT => (format!("{} {}", ui.t(G, 6), amount), false),
            status::IMPORT_AS_NEEDED => (ui.t(G, 37), false),
            status::EXPORT_SURPLUS => (ui.t(G, 38), false),
            _ => {
                let (imp, imp_open) = world.trade_partners(r, true);
                let (exp, exp_open) = world.trade_partners(r, false);
                let id = match (imp_open, exp_open, imp, exp) {
                    (true, true, _, _) => Some(33),
                    (true, false, _, _) => Some(31),
                    (false, true, _, _) => Some(32),
                    (_, _, true, true) => Some(36),
                    (_, _, true, false) => Some(34),
                    (_, _, false, true) => Some(35),
                    _ => None,
                };
                (id.map(|i| ui.t(G, i)).unwrap_or_default(), !(imp_open || exp_open))
            }
        };
        let sx = px + 330.0;
        if dull {
            draw_text_tinted(ui.r, Font::NormalWhiteOnDark, &s, sx, y, [0.65, 0.65, 0.6, 1.0]);
        } else {
            draw_text(ui.r, Font::NormalWhiteOnDark, &s, sx, y, font::WHITE);
        }
        if ui.clicked(rect) {
            *popup = Some(Popup::Resource(r));
        }
    }
    if list.len() > ROWS {
        if *scroll > 0 && ui.arrow(px + 596.0, py + 64.0, true) {
            *scroll -= 1;
        }
        if *scroll + ROWS < list.len() && ui.arrow(px + 596.0, py + 370.0, false) {
            *scroll += 1;
        }
    }
    let goto = ui.t(G, 30);
    if ui.button([px + 48.0, py + 396.0, 200.0, 24.0], &goto, Font::NormalBlackOnLight) {
        return Some(AdvisorAction::OpenEmpire);
    }
    let prices = ui.t(G, 2);
    if ui.button([px + 368.0, py + 396.0, 200.0, 24.0], &prices, Font::NormalBlackOnLight) {
        *popup = Some(Popup::Prices);
    }
    None
}

/// Prices throughout Egypt: what buyers pay and sellers receive for each good.
fn prices_popup(ui: &mut Ui, world: &World) -> bool {
    const G: usize = 54;
    let screen = ui.r.screen;
    let list: Vec<u16> = (1..osiris_sim::trade::RESOURCES as u16).filter(|&r| world.buy_price(r) > 0).collect();
    let per_row = 18;
    let (w, h) = (608.0, 176.0);
    let (x, y) = (((screen[0] - w) / 2.0).floor(), ((screen[1] - h) / 2.0).floor());
    panel::outer_panel(ui.r, ui.panels, x, y, 38, 11);
    let title = ui.t(G, 21);
    let tw = ui.width(Font::LargeBlackOnLight, &title);
    ui.label(Font::LargeBlackOnLight, &title, x + (w - tw) / 2.0, y + 12.0);
    for (row, chunk) in list.chunks(per_row).enumerate() {
        let ry = y + 44.0 + 64.0 * row as f32;
        let (buy, sell) = (ui.t(G, 22), ui.t(G, 23));
        ui.label(Font::SmallPlain, &buy, x + 14.0, ry + 22.0);
        ui.label(Font::SmallPlain, &sell, x + 14.0, ry + 38.0);
        for (i, &r) in chunk.iter().enumerate() {
            let cx = x + 104.0 + 27.0 * i as f32;
            ui.icon(r, cx, ry);
            let (b, s) = (world.buy_price(r).to_string(), world.sell_price(r).to_string());
            ui.label(Font::SmallPlain, &b, cx, ry + 22.0);
            ui.label(Font::SmallPlain, &s, cx, ry + 38.0);
        }
    }
    ui.click.take().is_some()
}

/// The trade settings of one resource: import and export, industry on or off, and
/// stockpiling. True when it closes.
fn resource_popup(ui: &mut Ui, world: &mut World, r: u16) -> bool {
    const G: usize = 54;
    let screen = ui.r.screen;
    let (w, h) = (576.0, 240.0);
    let (x, y) = (((screen[0] - w) / 2.0).floor(), ((screen[1] - h) / 2.0).floor());
    panel::outer_panel(ui.r, ui.panels, x, y, 36, 15);
    ui.icon(r, x + 16.0, y + 18.0);
    let name = ui.t(RESOURCE_NAMES, r as usize);
    let nw = ui.width(Font::LargeBlackOnLight, &name);
    ui.label(Font::LargeBlackOnLight, &name, x + (w - nw) / 2.0, y + 16.0);
    let stored = format!("{} {}", world.yards_stored(r), ui.t(G, 15));
    ui.label(Font::NormalBlackOnLight, &stored, x + 48.0, y + 50.0);
    let st = world.trade.status[r as usize];
    let amount = world.trade.amount[r as usize];
    // Import on the left, export on the right.
    for (col, buying) in [(0.0, true), (w / 2.0, false)] {
        let rect = [x + 32.0 + col, y + 80.0, 256.0, 30.0];
        let (_, open) = world.trade_partners(r, buying);
        let label = if !open {
            ui.t(G, if buying { 41 } else { 42 })
        } else {
            match (buying, st) {
                (true, status::IMPORT) => format!("{} {}", ui.t(G, 19), amount),
                (true, status::IMPORT_AS_NEEDED) => ui.t(G, 37),
                (false, status::EXPORT) => format!("{} {}", ui.t(G, 20), amount),
                (false, status::EXPORT_SURPLUS) => ui.t(G, 38),
                (true, _) => ui.t(G, 39),
                (false, _) => ui.t(G, 40),
            }
        };
        if !open {
            let lw = ui.width(Font::NormalBlackOnLight, &label);
            ui.label(Font::NormalBlackOnLight, &label, rect[0] + (rect[2] - lw) / 2.0, rect[1] + 8.0);
            continue;
        }
        let set_amount = (buying && st == status::IMPORT) || (!buying && st == status::EXPORT);
        let text_rect = if set_amount { [rect[0], rect[1], rect[2] - 56.0, rect[3]] } else { rect };
        if ui.button(text_rect, &label, Font::NormalBlackOnLight) {
            if buying {
                world.cycle_import(r);
            } else {
                world.cycle_export(r);
            }
        }
        if set_amount {
            if ui.arrow(rect[0] + rect[2] - 51.0, rect[1] + 3.0, false) {
                world.change_trade_amount(r, -100);
            }
            if ui.arrow(rect[0] + rect[2] - 28.0, rect[1] + 3.0, true) {
                world.change_trade_amount(r, 100);
            }
        }
    }
    let makes = world.defs.buildings.iter().flatten().any(|d| d.outputs.iter().any(|o| world.resource_id(o) == Some(r)) && world.is_allowed(d.id));
    if makes {
        let on = ui.t(G, if world.is_mothballed(r) { 17 } else { 16 });
        if ui.button([x + (w - 400.0) / 2.0, y + 130.0, 400.0, 30.0], &on, Font::NormalBlackOnLight) {
            world.toggle_mothballed(r);
        }
    }
    let stock = if world.is_stockpiled(r) { format!("{} - {}", ui.t(G, 26), ui.t(G, 27)) } else { format!("{} - {}", ui.t(G, 28), ui.t(G, 29)) };
    if ui.button([x + (w - 400.0) / 2.0, y + 168.0, 400.0, 30.0], &stock, Font::NormalBlackOnLight) {
        world.toggle_stockpiled(r);
    }
    ui.click.take().is_some_and(|c| !inside([x, y, w, h], c))
}

fn financial(ui: &mut Ui, world: &mut World, [px, py]: [f32; 2]) -> Option<AdvisorAction> {
    const G: usize = 60;
    let title = ui.t(G, 0);
    ui.label(Font::LargeBlackOnLight, &title, px + 60.0, py + 17.0);
    let (bx, by) = (px + 64.0, py + 48.0);
    panel::inner_panel(ui.r, ui.panels, bx, by, 34, 5);
    let t = world.treasury;
    let treasury = format!("{} {}", ui.t(G, if t < 0 { 3 } else { 2 }), t.abs());
    let tf = if t < 0 { Font::NormalYellow } else { Font::NormalWhiteOnDark };
    draw_text(ui.r, tf, &treasury, bx + 6.0, by + 10.0, font::WHITE);
    let rate = ui.t(G, 1);
    draw_text(ui.r, Font::NormalWhiteOnDark, &rate, bx + 70.0, by + 30.0, font::WHITE);
    if ui.arrow(bx + 170.0, by + 25.0, false) {
        world.finance.tax_rate = (world.finance.tax_rate - 1).max(0);
    }
    if ui.arrow(bx + 195.0, by + 25.0, true) {
        world.finance.tax_rate = (world.finance.tax_rate + 1).min(25);
    }
    let (covered, uncovered) = world.monthly_tax_estimate();
    let estimate = format!("{}% {} {} Deben", world.finance.tax_rate, ui.t(G, 4), covered * 12);
    draw_text(ui.r, Font::NormalWhiteOnDark, &estimate, bx + 240.0, by + 30.0, font::WHITE);
    let pct = world.percentage_taxed();
    let payers = format!("{}% {} ({} {})", pct, ui.t(G, 5), uncovered * 12, ui.t(G, 23));
    draw_text(ui.r, Font::NormalWhiteOnDark, &payers, bx + 10.0, by + 60.0, font::WHITE);
    let (last, this) = (&world.finance.last_year, &world.finance.this_year);
    let heads = [(ui.t(G, 6), 270.0), (ui.t(G, 7), 400.0)];
    for (h, x) in heads {
        ui.label(Font::NormalBlackOnLight, &h, px + x, py + 128.0);
    }
    let income = |y: &osiris_sim::finance::YearTotals| y.taxes + y.exports + y.gold;
    let expenses = |y: &osiris_sim::finance::YearTotals| y.imports + y.wages + y.construction + y.interest;
    let rows: Vec<(String, i32, i32)> = vec![
        (ui.t(G, 8), last.taxes, this.taxes),
        (ui.t(G, 9), last.exports, this.exports),
        (ui.t(G, 24), last.gold, this.gold),
        (ui.t(G, 10), income(last), income(this)),
        (ui.t(G, 11), last.imports, this.imports),
        (ui.t(G, 12), last.wages, this.wages),
        (ui.t(G, 13), last.construction, this.construction),
        (ui.t(G, 14), last.interest, this.interest),
        (ui.t(G, 17), expenses(last), expenses(this)),
        (ui.t(G, 18), income(last) - expenses(last), income(this) - expenses(this)),
    ];
    for (i, (name, a, b)) in rows.iter().enumerate() {
        // Space before each total and after the income block.
        let gap = [0.0, 0.0, 0.0, 6.0, 14.0, 14.0, 14.0, 14.0, 20.0, 26.0][i];
        let y = py + 150.0 + 18.0 * i as f32 + gap;
        let f = Font::NormalBlackOnLight;
        ui.label(f, name, px + 80.0, y);
        let (aw, bw) = (ui.width(f, &a.to_string()), ui.width(f, &b.to_string()));
        ui.label(f, &a.to_string(), px + 330.0 - aw, y);
        ui.label(f, &b.to_string(), px + 470.0 - bw, y);
    }
    None
}

/// One line of the chief overseer's report: a heading and the finding, in yellow
/// when it needs attention.
fn chief(ui: &mut Ui, world: &mut World, [px, py]: [f32; 2]) -> Option<AdvisorAction> {
    const G: usize = 61;
    let title = ui.t(4, 12);
    ui.label(Font::LargeBlackOnLight, &title, px + 60.0, py + 17.0);
    panel::inner_panel(ui.r, ui.panels, px + 26.0, py + 66.0, 35, 21);
    let mut lines: Vec<(String, String, bool)> = Vec::new();
    let s = world.sentiment.clamp(0, 100);
    lines.push((ui.t(G, 1), ui.t(G, (20 + s * 11 / 100) as usize), s < 40));
    let migration = if world.migration.newcomers_this_month >= 5 {
        (44, false)
    } else if world.housing_room() <= 0 {
        (45, true)
    } else {
        (59, false)
    };
    lines.push((ui.t(G, 2), ui.t(G, migration.0), migration.1));
    let l = &world.labor;
    let pct = if l.available > 0 { l.unemployed * 100 / l.available } else { 0 };
    let short = (l.needed - l.employed).max(0);
    let employment = if pct > 0 {
        let id = match pct {
            p if p > 10 => 76,
            p if p > 5 => 77,
            p if p > 2 => 78,
            _ => 79,
        };
        (format!("{} {}%", ui.t(G, id), pct), id != 79)
    } else if short > 0 {
        let id = match short {
            n if n > 75 => 80,
            n if n > 50 => 81,
            n if n > 25 => 82,
            _ => 83,
        };
        (format!("{} {}", ui.t(G, id), short), id != 83)
    } else {
        (ui.t(G, 84), false)
    };
    lines.push((ui.t(G, 3), employment.0, employment.1));
    let months = world.food_supply_months();
    let food = if months > 0 { (format!("{} {} months", ui.t(G, 98).trim(), months), false) } else { (ui.t(G, 95), true) };
    lines.push((ui.t(G, 4), food.0, food.1));
    let finance = {
        let (last, now) = (world.finance.last_year_balance, world.treasury);
        if now > last {
            (format!("{}{}", ui.t(G, 152), now - last), false)
        } else if now < last {
            (format!("{}{}", ui.t(G, 154), last - now), true)
        } else if world.percentage_taxed() < 75 {
            (ui.t(G, 151), false)
        } else {
            (ui.t(G, 153), false)
        }
    };
    lines.push((ui.t(G, 8), finance.0, finance.1));
    for (i, (head, body, warn)) in lines.iter().enumerate() {
        let y = py + 76.0 + 40.0 * i as f32;
        draw_text(ui.r, Font::NormalWhiteOnDark, head, px + 40.0, y, font::WHITE);
        let f = if *warn { Font::NormalYellow } else { Font::NormalWhiteOnDark };
        draw_text(ui.r, f, body, px + 60.0, y + 18.0, font::WHITE);
    }
    None
}

/// The four ratings as columns, with the goal each must reach; clicking one explains it.
fn ratings(ui: &mut Ui, world: &mut World, [px, py]: [f32; 2], selected: &mut usize) -> Option<AdvisorAction> {
    const G: usize = 53;
    let title = ui.t(G, 0);
    ui.label(Font::LargeBlackOnLight, &title, px + 60.0, py + 17.0);
    let goals = world.mission.as_ref().map(|m| m.goals.clone()).unwrap_or_default();
    let pop_line = if goals.population.enabled { format!("{} {}", ui.t(G, 6), goals.population.value) } else { ui.t(G, 7) };
    ui.label(Font::NormalBlackOnLight, &pop_line, px + 300.0, py + 20.0);
    if let Ok(bg) = ui.r.library.group_id("Pharaoh_Unloaded", 2, 0) {
        ui.image(bg, px + 60.0, py + 38.0);
    }
    let column = ui.r.library.group_id("Pharaoh_General", 189, 0).unwrap_or(0);
    let r = &world.ratings;
    let values = [(r.culture, goals.culture), (r.prosperity, goals.prosperity), (r.monument, goals.monuments), (r.kingdom, goals.kingdom)];
    for (i, (value, goal)) in values.iter().enumerate() {
        let x = px + 80.0 + 120.0 * i as f32;
        let base_y = py + 256.0;
        // The column rises one step per point and a half.
        let steps = 2 * (*value as f32 * 0.75) as i32;
        ui.image(column, x + 45.0, base_y);
        for k in 0..steps {
            ui.image(column + 1, x + 45.0, base_y - 1.0 - k as f32);
        }
        if goal.enabled && *value >= goal.value {
            ui.image(column + 2, x + 45.0, base_y - steps as f32 - 12.0);
        }
        let rect = [x, py + 276.0, 120.0, 60.0];
        let hot = ui.hot(rect) || *selected == i + 1;
        panel::button_border(ui.r, ui.panels, rect[0], rect[1], 120, 60, hot);
        ui.centred(Font::LargeBlackOnLight, &value.to_string(), x, py + 284.0, 120.0);
        let needed = format!("{} {}", if goal.enabled { goal.value } else { 0 }, ui.t(G, 5));
        ui.centred(Font::NormalBlackOnLight, &needed, x, py + 310.0, 120.0);
        if ui.clicked(rect) {
            *selected = i + 1;
        }
    }
    panel::inner_panel(ui.r, ui.panels, px + 40.0, py + 340.0, 35, 5);
    let (head, body) = match *selected {
        0 => (String::new(), ui.t(G, 8)),
        n => {
            let value = values[n - 1].0;
            let best = [65, 66, 67, 68][n - 1];
            let body = if value > 90 {
                ui.t(G, best)
            } else {
                match n {
                    1 => ui.t(G, culture_reason(world)),
                    2 => ui.t(G, if value <= 0 { 23 } else if value >= world.ratings.prosperity_max { 24 } else { 31 }),
                    3 => ui.t(G, 55),
                    _ => ui.t(52, (value / 5) as usize + 22),
                }
            };
            (ui.t(G, n), body)
        }
    };
    if !head.is_empty() {
        ui.label(Font::NormalWhiteOnDark, &head, px + 68.0, py + 344.0);
    }
    ui.wrapped(Font::NormalWhiteOnDark, &body, px + 68.0, py + 364.0, 520.0);
    None
}

/// Which gap in culture to point out: the least-covered of religion, entertainment,
/// schools and libraries (group 53 ids 9-19).
fn culture_reason(world: &World) -> usize {
    let c = &world.ratings.coverage;
    let options = [(world.religion.coverage_common, 14), (c.booth, 9), (c.school, 15), (c.library, 16)];
    options.iter().min_by_key(|o| o.0).map_or(9, |o| o.1)
}

/// The coverage word for a percentage (group 57: 7 None ... 18 Perfect).
fn coverage_word(ui: &Ui, pct: i32) -> String {
    let id = if pct <= 0 {
        7
    } else if pct >= 100 {
        18
    } else {
        8 + (pct / 10) as usize
    };
    ui.t(57, id)
}

fn staffed(world: &World, k: u16) -> (usize, usize) {
    let total = world.buildings.iter().filter(|b| b.kind == k).count();
    let active = world.buildings.iter().filter(|b| b.kind == k && b.workers > 0).count();
    (total, active)
}

fn religion(ui: &mut Ui, world: &mut World, [px, py]: [f32; 2], popup: &mut Option<Popup>) -> Option<AdvisorAction> {
    const G: usize = 59;
    let title = ui.t(G, 0);
    ui.label(Font::LargeBlackOnLight, &title, px + 60.0, py + 12.0);
    for (x, y, g, id) in [(180.0, 32.0, G, 5), (170.0, 46.0, G, 2), (250.0, 46.0, G, 1), (320.0, 46.0, 28, 150), (390.0, 18.0, G, 6), (400.0, 32.0, G, 8), (390.0, 46.0, G, 7), (460.0, 46.0, G, 3)] {
        let s = ui.t(g, id);
        ui.label(Font::SmallPlain, &s, px + x, py + y);
    }
    panel::inner_panel(ui.r, ui.panels, px + 32.0, py + 60.0, 36, 13);
    let icons = ui.r.library.group_id("Pharaoh_General", 129, 0).unwrap_or(0);
    for (i, god) in world.religion.gods.iter().enumerate() {
        let y = py + 68.0 + 40.0 * i as f32;
        let x = px + 40.0;
        let name = ui.t(157, i);
        let st = ui.t(187, god.status as usize);
        if god.status == 0 {
            draw_text(ui.r, Font::NormalYellow, &name, x, y, font::WHITE);
            draw_text(ui.r, Font::NormalYellow, &st, x + 62.0, y, font::WHITE);
        } else {
            draw_text(ui.r, Font::NormalWhiteOnDark, &name, x, y, font::WHITE);
            draw_text(ui.r, Font::NormalWhiteOnDark, &st, x + 62.0, y, font::WHITE);
            let (complexes, _) = staffed(world, 65 + i as u16);
            let (temples, active) = staffed(world, 60 + i as u16);
            let (shrines, _) = staffed(world, 140 + i as u16);
            for (dx, v) in [(162.0, complexes.to_string()), (227.0, format!("{active} ({temples})")), (292.0, shrines.to_string()), (352.0, god.months_since_festival.to_string())] {
                draw_text(ui.r, Font::NormalWhiteOnDark, &v, x + dx, y, font::WHITE);
            }
            let mood = ui.t(G, 20 + (god.mood / 10).clamp(0, 10) as usize);
            draw_text(ui.r, Font::NormalWhiteOnDark, &mood, x + 422.0, y, font::WHITE);
            for k in 0..(god.wrath / 20).min(5) {
                ui.image(icons + 34, x + 500.0 + 12.0 * k as f32, y);
            }
            for k in 0..(god.favour / 20).min(5) {
                ui.image(icons + 33, x + 500.0 + 12.0 * k as f32, y);
            }
        }
        let epithet = ui.t(158, i);
        draw_text(ui.r, Font::NormalBlackOnDark, &epithet, x, y + 18.0, font::BLACK);
    }
    let advice = if !world.rules.gods_enabled {
        ui.t(G, 43)
    } else {
        let least = world.religion.known().min_by_key(|(_, g)| g.mood).map(|(i, g)| (i, g.wrath));
        match least {
            Some((i, wrath)) if wrath > 4 => ui.t(G, 15 + i),
            _ if world.religion.coverage_common >= 100 => ui.t(G, 14),
            _ if world.population < 150 => ui.t(G, 13),
            _ => ui.t(G, 9),
        }
    };
    ui.wrapped(Font::NormalBlackOnLight, &advice, px + 60.0, py + 273.0, 512.0);
    // Festivals.
    panel::inner_panel(ui.r, ui.panels, px + 48.0, py + 330.0, 34, 5);
    let since = world.religion.known().map(|(_, g)| g.months_since_festival).min().unwrap_or(0);
    let last = format!("{} {} {}", since, ui.t(8, 5), ui.t(58, 15));
    draw_text(ui.r, Font::NormalWhiteOnDark, &last, px + 112.0, py + 336.0, font::WHITE);
    match &world.religion.festival {
        Some(f) => {
            let s = format!("{}{} ({} {})", ui.t(58, 34), ui.t(157, f.god), f.months_left, ui.t(8, 5));
            draw_text(ui.r, Font::NormalWhiteOnDark, &s, px + 112.0, py + 360.0, font::WHITE);
        }
        None => {
            let hold = ui.t(58, 16);
            if ui.button([px + 102.0, py + 354.0, 300.0, 24.0], &hold, Font::NormalBlackOnLight)
                && world.buildings.iter().any(|b| b.kind == osiris_sim::religion::FESTIVAL_SQUARE)
            {
                *popup = Some(Popup::Festival(None));
            }
            let advice = ui.t(58, 18 + (since / 4).clamp(0, 6) as usize);
            ui.wrapped(Font::NormalWhiteOnDark, &advice, px + 60.0, py + 384.0, 400.0);
        }
    }
    None
}

enum FestivalChoice {
    Stay,
    Close,
    God(usize),
}

/// Choosing a festival: first the god, then the size, with each size's cost.
fn festival_popup(ui: &mut Ui, world: &mut World, god: Option<usize>) -> FestivalChoice {
    let screen = ui.r.screen;
    let (w, h) = (416.0, 256.0);
    let (x, y) = (((screen[0] - w) / 2.0).floor(), ((screen[1] - h) / 2.0).floor());
    panel::outer_panel(ui.r, ui.panels, x, y, 26, 16);
    let title = ui.t(58, 52);
    ui.centred(Font::LargeBlackOnLight, &title, x, y + 14.0, w);
    match god {
        None => {
            for g in 0..5 {
                if world.religion.gods.get(g).is_none_or(|g| g.status == 0) {
                    continue;
                }
                let label = ui.t(58, 25 + g);
                if ui.button([x + 58.0, y + 50.0 + 34.0 * g as f32, 300.0, 26.0], &label, Font::NormalBlackOnLight) {
                    return FestivalChoice::God(g);
                }
            }
        }
        Some(g) => {
            let name = ui.t(157, g);
            ui.centred(Font::NormalBlackOnLight, &name, x, y + 44.0, w);
            for (i, size) in [osiris_sim::religion::festival::SMALL, osiris_sim::religion::festival::LARGE, osiris_sim::religion::festival::GRAND].into_iter().enumerate() {
                let label = format!("{} - {} {} Deben", ui.t(58, 31 + i), ui.t(58, 30), world.festival_cost(size));
                if ui.button([x + 38.0, y + 76.0 + 40.0 * i as f32, 340.0, 28.0], &label, Font::NormalBlackOnLight) {
                    let _ = world.plan_festival(g, size);
                    return FestivalChoice::Close;
                }
            }
        }
    }
    if ui.click.take().is_some_and(|c| !inside([x, y, w, h], c)) {
        return FestivalChoice::Close;
    }
    FestivalChoice::Stay
}

fn entertainment(ui: &mut Ui, world: &mut World, [px, py]: [f32; 2]) -> Option<AdvisorAction> {
    const G: usize = 58;
    let title = ui.t(G, 0);
    ui.label(Font::LargeBlackOnLight, &title, px + 60.0, py + 12.0);
    for (x, y, id) in [(180.0, 42.0, 1), (180.0, 56.0, 55), (280.0, 56.0, 2), (340.0, 56.0, 3), (470.0, 56.0, 4)] {
        let s = ui.t(G, id);
        ui.label(Font::SmallPlain, &s, px + x, py + y);
    }
    panel::inner_panel(ui.r, ui.panels, px + 32.0, py + 70.0, 36, 9);
    let c = world.ratings.coverage.clone();
    let rows = [
        (47, kind::BOOTH, 400, c.booth, 0usize),
        (48, kind::BANDSTAND, 700, c.bandstand, 1),
        (49, kind::PAVILION, 1200, c.pavilion, 2),
        (50, 32, 0, c.senet, 3),
    ];
    for (i, &(label, k, serves, cov, slot)) in rows.iter().enumerate() {
        let y = py + 80.0 + 25.0 * i as f32;
        let x = px + 40.0;
        let name = ui.t(G, label);
        draw_text(ui.r, Font::NormalWhiteOnDark, &name, x, y, font::WHITE);
        let (total, active) = staffed(world, k);
        let shows = world.buildings.iter().filter(|b| b.kind == k && b.shows.get(slot).copied().unwrap_or(0) > 0).count();
        for (dx, v) in [(140.0, format!("{active} ({total})")), (240.0, shows.to_string()), (310.0, format!("{} {}", serves * active as i32, ui.t(G, 5)))] {
            draw_text(ui.r, Font::NormalWhiteOnDark, &v, x + dx, y, font::WHITE);
        }
        let word = coverage_word(ui, cov);
        draw_text(ui.r, Font::NormalWhiteOnDark, &word, x + 430.0, y, font::WHITE);
    }
    let avg: i32 = {
        let houses: Vec<i32> = world.buildings.iter().filter_map(|b| b.house.as_ref()).filter(|h| h.population > 0).map(|h| h.entertainment).collect();
        if houses.is_empty() { 0 } else { houses.iter().sum::<i32>() / houses.len() as i32 }
    };
    let advice = ui.t(G, if avg > 0 { 8 } else { 7 });
    ui.wrapped(Font::NormalBlackOnLight, &advice, px + 30.0, py + 230.0, 512.0);
    None
}

fn education(ui: &mut Ui, world: &mut World, [px, py]: [f32; 2]) -> Option<AdvisorAction> {
    const G: usize = 57;
    let title = ui.t(G, 0);
    ui.label(Font::LargeBlackOnLight, &title, px + 60.0, py + 17.0);
    let kids: i32 = world.census.at_age[0..14].iter().sum();
    let young: i32 = world.census.at_age[14..21].iter().sum();
    for (x, v) in [(20.0, format!("{} {}", world.population, ui.t(G, 6))), (220.0, format!("{} {}", kids, ui.t(G, 4))), (420.0, format!("{} {}", young, ui.t(G, 5)))] {
        ui.centred(Font::NormalBlackOnLight, &v, px + x, py + 50.0, 200.0);
    }
    for (x, id) in [(180.0, 1), (290.0, 2), (440.0, 3)] {
        let s = ui.t(G, id);
        ui.label(Font::SmallPlain, &s, px + x, py + 86.0);
    }
    panel::inner_panel(ui.r, ui.panels, px + 32.0, py + 108.0, 36, 6);
    let c = world.ratings.coverage.clone();
    let rows = [(18, osiris_sim::ratings::SCHOOL, 75, c.school, 4), (20, osiris_sim::ratings::ACADEMY, 100, c.academy, 5), (22, osiris_sim::ratings::LIBRARY, 800, c.library, 6)];
    for (i, &(name_id, k, serves, cov, who)) in rows.iter().enumerate() {
        let y = py + 118.0 + 25.0 * i as f32;
        let x = px + 40.0;
        let (total, active) = staffed(world, k);
        let name = format!("{} {}", total, ui.t(8, name_id));
        draw_text(ui.r, Font::NormalWhiteOnDark, &name, x, y, font::WHITE);
        draw_text(ui.r, Font::NormalWhiteOnDark, &active.to_string(), x + 140.0, y, font::WHITE);
        let care = format!("{} {}", serves * active as i32, ui.t(G, who));
        draw_text(ui.r, Font::NormalWhiteOnDark, &care, x + 250.0, y, font::WHITE);
        let word = coverage_word(ui, cov);
        draw_text(ui.r, Font::NormalWhiteOnDark, &word, x + 400.0, y, font::WHITE);
    }
    let advice = ui.t(G, if c.school <= 0 && world.population < 300 { 23 } else if c.school >= 100 && c.library >= 100 { 25 } else { 19 });
    ui.wrapped(Font::NormalBlackOnLight, &advice, px + 30.0, py + 250.0, 37.0 * 16.0);
    None
}

fn health(ui: &mut Ui, world: &mut World, [px, py]: [f32; 2]) -> Option<AdvisorAction> {
    const G: usize = 56;
    let title = ui.t(G, 0);
    ui.label(Font::LargeBlackOnLight, &title, px + 60.0, py + 17.0);
    let h = world.ratings.health;
    let state = if world.population >= 200 { ui.t(G, (h / 10).clamp(0, 10) as usize + 16) } else { ui.t(G, 15) };
    ui.wrapped(Font::NormalBlackOnLight, &state, px + 60.0, py + 46.0, 500.0);
    for (x, id) in [(180.0, 3), (290.0, 4), (440.0, 5)] {
        let s = ui.t(G, id);
        ui.label(Font::SmallPlain, &s, px + x, py + 94.0);
    }
    panel::inner_panel(ui.r, ui.panels, px + 32.0, py + 108.0, 36, 6);
    let c = world.ratings.coverage.clone();
    let rows = [(25, osiris_sim::ratings::PHYSICIAN, 1000, c.physician), (27, osiris_sim::ratings::DENTIST, 1000, c.dentist), (29, kind::APOTHECARY, 100, c.apothecary), (31, osiris_sim::ratings::MORTUARY, 1000, c.mortuary)];
    for (i, &(name_id, k, serves, cov)) in rows.iter().enumerate() {
        let y = py + 116.0 + 20.0 * i as f32;
        let x = px + 40.0;
        let (total, active) = staffed(world, k);
        let name = format!("{} {}", total, ui.t(8, name_id));
        draw_text(ui.r, Font::NormalWhiteOnDark, &name, x, y, font::WHITE);
        draw_text(ui.r, Font::NormalWhiteOnDark, &active.to_string(), x + 145.0, y, font::WHITE);
        let care = format!("{} {}", serves * active as i32, ui.t(G, 6));
        draw_text(ui.r, Font::NormalWhiteOnDark, &care, x + 250.0, y, font::WHITE);
        let word = ui.t(G, (cov / 10).clamp(0, 10) as usize + 43);
        draw_text(ui.r, Font::NormalWhiteOnDark, &word, x + 400.0, y, font::WHITE);
    }
    let advice = ui.t(G, if world.population < 200 { 14 } else if c.physician < 100 { 9 } else { 14 });
    ui.wrapped(Font::NormalBlackOnLight, &advice, px + 60.0, py + 218.0, 500.0);
    None
}

/// Population: the history of the city's size, its ages, or its housing, as a bar graph.
fn population(ui: &mut Ui, world: &mut World, [px, py]: [f32; 2], graph: &mut usize) -> Option<AdvisorAction> {
    const G: usize = 55;
    *graph %= 3;
    let title = ui.t(G, *graph);
    ui.label(Font::LargeBlackOnLight, &title, px + 60.0, py + 17.0);
    ui.label(Font::NormalBlackOnLight, &format!("Population {}", world.population), px + 450.0, py + 25.0);
    let data: Vec<i32> = match *graph {
        0 => world.ratings.population_history.clone(),
        1 => world.census.at_age.clone(),
        _ => {
            let mut levels = vec![0; 20];
            for h in world.buildings.iter().filter_map(|b| b.house.as_ref()) {
                levels[(h.level as usize).min(19)] += h.population;
            }
            levels
        }
    };
    let (gx, gy, gw, gh) = (px + 65.0, py + 65.0, 395.0, 195.0);
    panel::inner_panel(ui.r, ui.panels, gx - 8.0, gy - 8.0, 26, 14);
    let steps = [100, 200, 400, 800, 1500, 3000, 6000, 12000, 25000, 50000];
    let max = data.iter().copied().max().unwrap_or(0);
    let top = steps.iter().copied().find(|&s| s >= max).unwrap_or(50000) as f32;
    let n = data.len().max(1);
    let bar = (gw / n as f32).clamp(1.0, 20.0);
    for (i, &v) in data.iter().enumerate() {
        let hgt = (v as f32 / top * gh).max(0.0);
        ui.r.rect([gx + bar * i as f32, gy + gh - hgt], [(bar - 1.0).max(1.0), hgt], [0.72, 0.12, 0.08, 1.0], Space::Screen);
    }
    ui.label(Font::SmallPlain, &(top as i32).to_string(), gx - 4.0, gy - 22.0);
    // The other two graphs, to switch to.
    for (k, dy) in [(1usize, 61.0), (2, 161.0)] {
        let other = (*graph + k) % 3;
        let rect = [px + 503.0, py + dy, 104.0, 55.0];
        let label = ui.t(G, other);
        if ui.button(rect, &label, Font::SmallPlain) {
            *graph = other;
        }
    }
    let lines: Vec<String> = match *graph {
        1 => {
            let avg = if world.population > 0 { (0..100).map(|a| a as i32 * world.census.at_age[a]).sum::<i32>() / world.population } else { 0 };
            vec![format!("Average age {avg}"), format!("{}% of the people can work", if world.population > 0 { world.labor.available * 100 / world.population } else { 0 })]
        }
        2 => vec![format!("Housing prosperity {}", world.ratings.prosperity_max)],
        _ => vec![format!("{} {}", world.food_supply_months(), ui.t(8, 5))],
    };
    for (i, l) in lines.iter().enumerate() {
        ui.label(Font::NormalBlackOnLight, l, px + 60.0, py + 340.0 + 18.0 * i as f32);
    }
    None
}

fn political(ui: &mut Ui, world: &mut World, [px, py]: [f32; 2], popup: &mut Option<Popup>) -> Option<AdvisorAction> {
    const G: usize = 52;
    ui.label(Font::LargeBlackOnLight, "Political Overseer", px + 60.0, py + 17.0);
    let rating = format!("{} {}", ui.t(G, 0), world.ratings.kingdom);
    ui.label(Font::NormalBlackOnLight, &rating, px + 60.0, py + 42.0);
    let advice = ui.t(G, (world.ratings.kingdom / 5).clamp(0, 20) as usize + 22);
    ui.wrapped(Font::NormalBlackOnLight, &advice, px + 60.0, py + 64.0, 35.0 * 16.0);
    panel::inner_panel(ui.r, ui.panels, px + 32.0, py + 110.0, 36, 13);
    // Open requests, five to a screen: what and how much, the months left, what the
    // city holds, and whether it can be sent.
    let requests: Vec<(usize, u16, i32, i32)> = world.scenario_events.open_requests().map(|(i, e)| (i, e.resource, e.units(), e.months_left)).collect();
    if requests.is_empty() {
        let none = ui.t(G, 21);
        ui.centred(Font::NormalWhiteOnDark, &none, px + 32.0, py + 200.0, 36.0 * 16.0);
    }
    for (row, &(i, r, units, months)) in requests.iter().take(5).enumerate() {
        let (rx, ry) = (px + 38.0, py + 116.0 + 45.0 * row as f32);
        let rect = [rx, ry, 35.0 * 16.0, 45.0];
        ui.icon(r, rx + 7.0, ry + 7.0);
        let shown = if r == osiris_sim::scenario_events::DEBEN || r == osiris_sim::scenario_events::TROOPS { units } else { units / 100 };
        let what = format!("{} {}", shown, ui.t(RESOURCE_NAMES, r as usize));
        ui.label(Font::NormalWhiteOnDark, &what, rx + 30.0, ry + 7.0);
        let when = format!("{} {} {}", months, ui.t(8, if months == 1 { 4 } else { 5 }), ui.t(12, 2));
        ui.label(Font::NormalWhiteOnDark, &when, rx + 310.0, ry + 7.0);
        let can = world.can_send_request(i);
        let held = if r == osiris_sim::scenario_events::DEBEN {
            format!("{} {}", world.treasury, ui.t(G, 44))
        } else {
            format!("{} {}", world.city_stored(r) / 100, ui.t(G, 43))
        };
        ui.label(Font::NormalWhiteOnDark, &held, rx + 30.0, ry + 25.0);
        let status = ui.t(G, if can { 47 } else { 48 });
        ui.label(if can { Font::NormalYellow } else { Font::NormalWhiteOnDark }, &status, rx + 310.0, ry + 25.0);
        if ui.hot(rect) {
            panel::button_border(ui.r, ui.panels, rx + 2.0, ry + 2.0, rect[2] as i32 - 4, rect[3] as i32 - 4, false);
        }
        if ui.clicked(rect) {
            *popup = Some(Popup::Request(i, can));
        }
    }
    // The governor: his rank, savings and salary, and what he can do with them.
    panel::inner_panel(ui.r, ui.panels, px + 64.0, py + 324.0, 32, 6);
    let rank = ui.t(32, world.assigned_rank() as usize);
    ui.label(Font::LargeBlackOnDark, &rank, px + 72.0, py + 332.0);
    let give = ui.t(G, 2);
    if ui.button([px + 320.0, py + 330.0, 250.0, 20.0], &give, Font::NormalWhiteOnDark) {
        *popup = Some(Popup::Donate(0));
    }
    let gift = ui.t(G, 49);
    if ui.button([px + 320.0, py + 352.0, 250.0, 20.0], &gift, Font::NormalWhiteOnDark) {
        *popup = Some(Popup::Gift);
    }
    let savings = format!("{} {} Db", ui.t(G, 1), world.governor.savings);
    ui.label(Font::NormalWhiteOnDark, &savings, px + 72.0, py + 374.0);
    let rank = world.governor.salary_rank as usize;
    let salary = format!("{} {} {}", ui.t(G, 4 + rank), osiris_sim::kingdom::SALARIES[rank], ui.t(G, 3));
    if ui.button([px + 70.0, py + 392.0, 500.0, 24.0], &salary, Font::NormalWhiteOnDark) {
        *popup = Some(Popup::Salary);
    }
    None
}

/// A small popup panel centred on the screen, `w` by `h` tiles, with its title.
fn popup_frame(ui: &mut Ui, w: i32, h: i32, title: &str) -> [f32; 2] {
    let screen = ui.r.screen;
    let (pw, ph) = (w as f32 * 16.0, h as f32 * 16.0);
    let (x, y) = (((screen[0] - pw) / 2.0).floor(), ((screen[1] - ph) / 2.0).floor());
    panel::outer_panel(ui.r, ui.panels, x, y, w, h);
    ui.centred(Font::LargeBlackOnLight, title, x, y + 16.0, pw);
    [x, y]
}

/// The eleven salaries, one per rank. True when it closes.
fn salary_popup(ui: &mut Ui, world: &mut World) -> bool {
    const G: usize = 52;
    let title = ui.t(G, 15);
    let [x, y] = popup_frame(ui, 32, 22, &title);
    panel::inner_panel(ui.r, ui.panels, x + 16.0, y + 48.0, 30, 15);
    for rank in 0..osiris_sim::kingdom::SALARIES.len() {
        let rect = [x + 24.0, y + 56.0 + 20.0 * rank as f32, 30.0 * 16.0 - 16.0, 20.0];
        let hot = ui.hot(rect);
        let f = if hot || rank == world.governor.salary_rank as usize { Font::NormalYellow } else { Font::NormalWhiteOnDark };
        let line = format!("{} {} {}", ui.t(G, 4 + rank), osiris_sim::kingdom::SALARIES[rank], ui.t(G, 3));
        ui.label(f, &line, rect[0] + 8.0, rect[1] + 3.0);
        if ui.clicked(rect) {
            world.set_salary_rank(rank as u8);
            return true;
        }
    }
    let note = if world.has_mansion() { ui.t(G, 76) } else { ui.t(G, 78) };
    ui.wrapped(Font::NormalBlackOnLight, &note, x + 24.0, y + 300.0, 28.0 * 16.0);
    ui.button([x + 216.0, y + 324.0, 80.0, 24.0], "Cancel", Font::NormalBlackOnLight)
}

/// Modest, generous and lavish gifts and what they cost. True when it closes.
fn gift_popup(ui: &mut Ui, world: &mut World) -> bool {
    const G: usize = 52;
    let title = ui.t(G, 49);
    let [x, y] = popup_frame(ui, 32, 17, &title);
    let savings = format!("{} {} Db", ui.t(G, 1), world.governor.savings);
    ui.centred(Font::NormalBlackOnLight, &savings, x, y + 46.0, 32.0 * 16.0);
    for size in 0..3 {
        let ry = y + 76.0 + 48.0 * size as f32;
        let cost = world.gift_cost(size);
        let label = format!("{} {} Db", ui.t(G, 63 + size), cost);
        ui.label(Font::NormalBlackOnLight, &label, x + 32.0, ry + 4.0);
        let send = ui.t(G, 66 + size);
        let can = cost <= world.governor.savings;
        if ui.button([x + 240.0, ry, 240.0, 24.0], &send, if can { Font::NormalBlackOnLight } else { Font::SmallPlain }) && can {
            world.send_gift(size);
            return true;
        }
    }
    if world.gift_cost(0) > world.governor.savings {
        let none = ui.t(G, 70);
        ui.wrapped(Font::NormalBlackOnLight, &none, x + 32.0, y + 220.0, 28.0 * 16.0);
    }
    ui.button([x + 216.0, y + 240.0, 80.0, 24.0], "Cancel", Font::NormalBlackOnLight)
}

/// Choosing how much of a burial provision to send: the new amount (in hundreds)
/// while it stays open, `None` when it closes.
fn burial_popup(ui: &mut Ui, world: &mut World, r: u16, amount: i32) -> Option<i32> {
    const G: usize = 199;
    let title = ui.t(G, 10);
    let [x, y] = popup_frame(ui, 26, 12, &title);
    let (need, sent) = world.burial.get(r as usize).copied().unwrap_or((0, 0));
    let most = ((need - sent).min(world.city_stored(r)) / 100).max(0);
    ui.icon(r, x + 40.0, y + 52.0);
    let line = format!("{} {}", ui.t(G, 4), amount);
    ui.label(Font::NormalBlackOnLight, &line, x + 70.0, y + 54.0);
    let mut amount = amount;
    if ui.arrow(x + 260.0, y + 48.0, true) {
        amount = (amount + 1).min(most);
    }
    if ui.arrow(x + 286.0, y + 48.0, false) {
        amount = (amount - 1).max(0);
    }
    let all = ui.t(G, 5);
    if ui.button([x + 320.0, y + 50.0, 60.0, 22.0], &all, Font::NormalBlackOnLight) {
        amount = most;
    }
    let send = ui.t(G, 6);
    if ui.button([x + 40.0, y + 130.0, 160.0, 24.0], &send, Font::NormalBlackOnLight) {
        world.dispatch_burial(r, amount * 100);
        return None;
    }
    let cancel = ui.t(G, 7);
    if ui.button([x + 216.0, y + 130.0, 160.0, 24.0], &cancel, Font::NormalBlackOnLight) {
        return None;
    }
    Some(amount)
}

/// Choosing how much of his savings the governor gives the city. The new amount while
/// it stays open, `None` when it closes.
fn donate_popup(ui: &mut Ui, world: &mut World, amount: i32) -> Option<i32> {
    const G: usize = 52;
    let title = ui.t(G, 16);
    let [x, y] = popup_frame(ui, 26, 12, &title);
    let savings = world.governor.savings;
    let line = format!("{} {} Db", ui.t(G, 17), amount);
    ui.centred(Font::NormalBlackOnLight, &line, x, y + 56.0, 26.0 * 16.0);
    let mut amount = amount;
    for (i, step) in [-100, -10, 10, 100].into_iter().enumerate() {
        let label = if step > 0 { format!("+{step}") } else { step.to_string() };
        if ui.button([x + 40.0 + 70.0 * i as f32, y + 90.0, 60.0, 22.0], &label, Font::NormalBlackOnLight) {
            amount = (amount + step).clamp(0, savings);
        }
    }
    let all = ui.t(G, 19);
    if ui.button([x + 320.0, y + 90.0, 60.0, 22.0], &all, Font::NormalBlackOnLight) {
        amount = savings;
    }
    let give = ui.t(G, 18);
    if ui.button([x + 60.0, y + 140.0, 140.0, 24.0], &give, Font::NormalBlackOnLight) {
        world.donate(amount);
        return None;
    }
    if ui.button([x + 216.0, y + 140.0, 140.0, 24.0], "Cancel", Font::NormalBlackOnLight) {
        return None;
    }
    Some(amount)
}

/// "Dispatch goods?" with Yes and No, or "You do not have enough" with OK. True when
/// it closes.
fn request_popup(ui: &mut Ui, world: &mut World, i: usize, can: bool) -> bool {
    let screen = ui.r.screen;
    let (w, h) = (400.0, 160.0);
    let (x, y) = (((screen[0] - w) / 2.0).floor(), ((screen[1] - h) / 2.0).floor());
    panel::outer_panel(ui.r, ui.panels, x, y, 25, 10);
    let title = ui.t(5, 6);
    ui.centred(Font::LargeBlackOnLight, &title, x, y + 16.0, w);
    let troops = world.scenario_events.list.get(i).is_some_and(|e| e.resource == osiris_sim::scenario_events::TROOPS);
    let line = ui.t(5, match (troops, can) {
        (true, true) => 15,
        (true, false) if world.military.companies.iter().any(|c| c.fort != 0 && !c.soldiers.is_empty()) => 13,
        (true, false) => 11,
        (false, true) => 7,
        (false, false) => 9,
    });
    ui.centred(Font::NormalBlackOnLight, &line, x, y + 60.0, w);
    if can {
        if ui.button([x + 80.0, y + 110.0, 100.0, 24.0], "Yes", Font::NormalBlackOnLight) {
            world.dispatch_request(i);
            return true;
        }
        ui.button([x + 220.0, y + 110.0, 100.0, 24.0], "No", Font::NormalBlackOnLight)
    } else {
        ui.button([x + 150.0, y + 110.0, 100.0, 24.0], "OK", Font::NormalBlackOnLight)
    }
}

fn military(ui: &mut Ui, world: &mut World, [px, py]: [f32; 2]) -> Option<AdvisorAction> {
    const G: usize = 51;
    let title = ui.t(G, 0);
    ui.label(Font::LargeBlackOnLight, &title, px + 60.0, py + 12.0);
    panel::inner_panel(ui.r, ui.panels, px + 32.0, py + 70.0, 36, 17);
    let companies: Vec<(usize, osiris_sim::military::Company)> = world.military.companies.iter().cloned().enumerate().filter(|(_, c)| c.fort != 0).collect();
    if companies.is_empty() {
        let none = ui.t(G, 16);
        ui.wrapped(Font::NormalWhiteOnDark, &none, px + 42.0, py + 70.0 + 128.0, 34.0 * 16.0);
    }
    let mut action = None;
    for (row, (c, co)) in companies.iter().take(6).enumerate() {
        let ry = py + 78.0 + 44.0 * row as f32;
        let name = ui.t(138, c % 10).trim_matches('"').to_owned();
        ui.label(Font::NormalWhiteOnDark, &name, px + 44.0, ry + 4.0);
        let arm = ui.t(138, match co.kind {
            osiris_sim::military::CHARIOTEER => 33,
            osiris_sim::military::ARCHER => 35,
            _ => 34,
        });
        let count = format!("{} {}", co.soldiers.len(), arm);
        ui.label(Font::NormalWhiteOnDark, &count, px + 44.0, ry + 22.0);
        let morale = ui.t(138, 37 + (co.morale / 5).clamp(0, 20) as usize);
        ui.label(Font::NormalWhiteOnDark, &morale, px + 200.0, ry + 22.0);
        let go = format!("{} {}", ui.t(G, 1), ui.t(G, 2));
        if !co.soldiers.is_empty() && ui.button([px + 330.0, ry + 4.0, 110.0, 22.0], &go, Font::NormalWhiteOnDark) {
            action = Some(AdvisorAction::GoToCompany(*c));
        }
        let back = format!("{} {}", ui.t(G, 3), ui.t(G, 4));
        if !co.at_fort && ui.button([px + 450.0, ry + 4.0, 110.0, 22.0], &back, Font::NormalWhiteOnDark) {
            world.return_company(*c);
        }
        // Kingdom service: lit when the company answers Pharaoh's calls for troops.
        let service = format!("{} {}", ui.t(G, 5), ui.t(G, 6));
        let rect = [px + 450.0, ry + 26.0, 110.0, 18.0];
        panel::button_border(ui.r, ui.panels, rect[0], rect[1], rect[2] as i32, rect[3] as i32, co.kingdom_service);
        ui.centred(if co.kingdom_service { Font::NormalYellow } else { Font::NormalWhiteOnDark }, &service, rect[0], rect[1] + 3.0, rect[2]);
        if ui.clicked(rect) {
            world.toggle_kingdom_service(*c);
        }
        if co.abroad > 0 {
            let away = format!("{} {}", co.abroad, ui.t(G, 29));
            ui.label(Font::NormalWhiteOnDark, &away, px + 330.0, ry + 26.0);
        }
    }
    // What the scouts report.
    let invaders = world.figures.iter().any(|f| osiris_sim::invasions::is_invader_kind(f.kind));
    let coming = world.invasions.planned.iter().any(|p| p.announced && !p.done);
    let threat = ui.t(G, if invaders { 10 } else if coming { 9 } else { 8 });
    ui.label(Font::NormalBlackOnLight, &threat, px + 50.0, py + 432.0 - 90.0);
    let troops_wanted = world.scenario_events.open_requests().any(|(_, e)| e.resource == osiris_sim::scenario_events::TROOPS);
    let abroad = ui.t(G, match &world.military.battle {
        Some(b) if b.fought => 15,
        Some(_) => 14,
        None if troops_wanted => 13,
        None => 12,
    });
    ui.label(Font::NormalBlackOnLight, &abroad, px + 50.0, py + 432.0 - 70.0);
    action
}

/// Text lines (group 199) for a monument family: not begun (two lines), under way,
/// just finished, and housing the deceased.
fn monument_lines(k: u16) -> Option<([usize; 2], usize, usize, usize)> {
    use osiris_sim::monuments::{self as mon, Family, Style};
    let style = mon::monument_def(k)?.style;
    Some(match style {
        Style::Pyramid(Family::True) => ([14, 15], 16, 17, 18),
        Style::Pyramid(Family::Mudbrick) => ([19, 20], 21, 22, 23),
        Style::Pyramid(Family::Stepped) => ([24, 25], 26, 27, 28),
        Style::Pyramid(Family::Bent) => ([29, 30], 31, 32, 33),
        Style::Mastaba => ([34, 35], 36, 37, 38),
        Style::Obelisk { .. } => ([43, 44], 45, 46, 46),
        Style::Sphinx => ([39, 40], 41, 42, 42),
        Style::Mausoleum => ([51, 52], 53, 54, 55),
        Style::SunTemple => ([47, 48], 49, 50, 50),
    })
}

fn monuments(ui: &mut Ui, world: &mut World, [px, py]: [f32; 2], popup: &mut Option<Popup>) -> Option<AdvisorAction> {
    const G: usize = 199;
    let title = ui.t(G, 0);
    ui.label(Font::LargeBlackOnLight, &title, px + 60.0, py + 12.0);
    let rating = format!("{} {}", ui.t(G, 11), world.ratings.monument);
    ui.label(Font::NormalBlackOnLight, &rating, px + 60.0, py + 42.0);
    // The scenario's monuments, each with how it stands.
    panel::inner_panel(ui.r, ui.panels, px + 32.0, py + 64.0, 36, 13);
    let slots: Vec<u16> = world.scenario_monuments.iter().copied().filter(|&m| m > 0).collect();
    for (i, &code) in slots.iter().enumerate() {
        let y = py + 70.0 + 66.0 * i as f32;
        let name = ui.t(198, code as usize);
        ui.label(Font::NormalWhiteOnDark, &name, px + 48.0, y);
        let def = osiris_sim::monuments::monument_for_title(code as usize);
        let Some((begin, under_way, done, rests)) = def.and_then(|d| monument_lines(d.kind)) else { continue };
        // The same monument may be listed twice: the second slot is the second one built.
        let nth = slots[..i].iter().filter(|&&c| c == code).count();
        let built = def.and_then(|d| world.buildings.iter().filter(|b| b.kind == d.kind).nth(nth));
        let lines: Vec<String> = match built.and_then(|b| b.monument.as_ref()) {
            // Monuments paid for in stone when placed say how much is needed and stored.
            None => match def.and_then(|d| osiris_sim::monuments::placement_cost(d.kind)) {
                Some((r, units)) => {
                    let have = world.city_stored(r) / 100;
                    vec![format!("{} {} {} {} {}", ui.t(G, begin[0]).trim_end(), units / 100, ui.t(G, begin[1]).trim(), have, ui.t(G, if have == 1 { 57 } else { 58 }))]
                }
                None => vec![ui.t(G, begin[0]), ui.t(G, begin[1])],
            },
            Some(m) if m.finished => vec![ui.t(G, if m.funeral_done { rests } else { done })],
            Some(_) => {
                let pct = built.map_or(0, |b| world.monument_percent(b.id));
                vec![format!("{} {}% {}", ui.t(G, under_way), pct.min(99), ui.t(178, 0))]
            }
        };
        ui.wrapped(Font::NormalWhiteOnDark, &lines.join(" "), px + 60.0, y + 16.0, 33.0 * 16.0);
    }
    // Burial provisions: what is needed, what has been sent, and what is in storage.
    panel::inner_panel(ui.r, ui.panels, px + 32.0, py + 280.0, 36, 8);
    let needs = world.burial_needs();
    if needs.is_empty() {
        let none = ui.t(G, 12);
        ui.centred(Font::NormalWhiteOnDark, &none, px + 32.0, py + 336.0, 36.0 * 16.0);
    } else {
        let hint = ui.t(G, 3);
        ui.label(Font::NormalWhiteOnDark, &hint, px + 48.0, py + 288.0);
    }
    for (i, &(r, need, sent)) in needs.iter().take(6).enumerate() {
        let (cx, cy) = (px + 48.0 + 280.0 * (i % 2) as f32, py + 310.0 + 34.0 * (i / 2) as f32);
        let rect = [cx - 4.0, cy - 4.0, 270.0, 32.0];
        ui.icon(r, cx, cy);
        let line = format!("{} / {} {}", sent / 100, need / 100, ui.t(RESOURCE_NAMES, r as usize));
        let f = if sent >= need { Font::NormalYellow } else { Font::NormalWhiteOnDark };
        ui.label(f, &line, cx + 28.0, cy);
        let stored = format!("{} {}", world.city_stored(r) / 100, ui.t(G, 1));
        ui.label(Font::SmallPlain, &stored, cx + 28.0, cy + 16.0);
        if sent < need && ui.clicked(rect) {
            *popup = Some(Popup::Burial(r, 0));
        }
    }
    None
}
