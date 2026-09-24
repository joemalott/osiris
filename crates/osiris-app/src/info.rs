//! The information window opened by clicking a building or a patch of land, laid out
//! as in the original: the building's name, what it is doing, its stock, its workers,
//! and for storage its special orders.
//!
//! Each building type has its own text group (`text_id` in the building data), laid
//! out the same way for most types: 0 name, 1 description, then status lines.

use crate::widgets::{Ui, inside};
use osiris_sim::World;
use osiris_sim::buildings::{Building, kind};
use osiris_sim::economy::resource;
use osiris_sim::houses::Need;
use osiris_sim::storage::order;
use osiris_ui::{Font, panel};

const TEXT_GENERAL: usize = 8;
const TEXT_FRAME: usize = 69;
const TEXT_HOUSE: usize = 127;
const TEXT_HOUSE_LEVELS: usize = 29;
const TEXT_VACANT: usize = 128;
const TEXT_RESOURCES: usize = 23;
const TEXT_TERRAIN: usize = 70;
const TEXT_YARD: usize = 99;
const TEXT_GRANARY: usize = 98;
const TEXT_BAZAAR: usize = 97;

/// What was clicked.
#[derive(Clone, Copy)]
pub enum Target {
    Building(u32),
    Tile(i32, i32),
    /// Up to seven walkers on the clicked tile, and the one shown.
    Figures([u32; 7], u8, u8),
    /// A company, opened from one of its soldiers or its standard.
    Company(usize),
}

/// What the window asks of the game.
pub enum InfoAction {
    Close,
    Overseer(crate::advisors::Advisor),
    /// Take command of a company: the next map click sends it there.
    SelectCompany(usize),
}

pub struct InfoPanel {
    pub target: Target,
    /// The special orders window over it.
    orders: bool,
    click: Option<[f32; 2]>,
    cursor: [f32; 2],
    scroll: usize,
    /// Where the window was last drawn, for clicks.
    rect: [f32; 4],
}

impl InfoPanel {
    pub fn new(target: Target) -> Self {
        Self { target, orders: false, click: None, cursor: [0.0; 2], scroll: 0, rect: [0.0; 4] }
    }

    pub fn open_orders(&mut self) {
        self.orders = true;
    }

    /// Whether `p` is on the window (or its orders window).
    pub fn contains(&self, p: [f32; 2]) -> bool {
        self.orders || inside(self.rect, p)
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

    /// The company to take command of when this window is closed by a right-click:
    /// as in the original, closing a company's window with men in it (unless they
    /// are mopping up) lets the player click where to send them.
    pub fn company_on_close(&self, world: &World) -> Option<usize> {
        let Target::Company(c) = self.target else { return None };
        let co = world.military.companies.get(c)?;
        (world.company_men(c) > 0 && co.order != osiris_sim::military::Order::MopUp).then_some(c)
    }

    /// Right-click: closes the orders window, else the whole window.
    pub fn back(&mut self) -> bool {
        if self.orders {
            self.orders = false;
            return false;
        }
        true
    }

    pub fn draw(&mut self, ui: &mut Ui, world: &mut World) -> Option<InfoAction> {
        ui.cursor = self.cursor;
        ui.click = self.click.take();
        let orders_click = if self.orders { ui.click.take() } else { None };
        let action = match self.target {
            Target::Building(id) => match world.buildings.get(id) {
                Some(b) => {
                    let b = b.clone();
                    self.building_window(ui, world, &b)
                }
                None => Some(InfoAction::Close),
            },
            Target::Tile(x, y) => self.terrain_window(ui, world, x, y),
            Target::Figures(ids, n, selected) => {
                let (action, pick) = self.figure_window(ui, world, &ids[..n as usize], selected as usize);
                if let Some(p) = pick {
                    self.target = Target::Figures(ids, n, p as u8);
                }
                action
            }
            Target::Company(c) => self.company_window(ui, world, c),
        };
        if self.orders {
            ui.click = orders_click;
            if let Target::Building(id) = self.target {
                self.orders_window(ui, world, id);
            }
        }
        action
    }

    /// The window frame: panel, title, and help and close buttons. Returns its origin.
    fn frame(&mut self, ui: &mut Ui, wb: i32, hb: i32, title: &str) -> ([f32; 2], bool) {
        let screen = ui.r.screen;
        let (w, h) = ((wb * 16) as f32, (hb * 16) as f32);
        let x = ((screen[0] - crate::sidebar::width() - w) / 2.0).max(0.0).floor();
        let y = ((screen[1] - h) / 2.0).max(40.0).floor();
        self.rect = [x, y, w, h];
        panel::outer_panel(ui.r, ui.panels, x, y, wb, hb);
        ui.centred(Font::LargeBlackOnLight, title, x, y + 14.0, w);
        let ctx = ui.img.context_icons;
        ui.image(ctx, x + 14.0, y + h - 40.0);
        let closed = ui.image_button(ctx + 4, x + w - 40.0, y + h - 40.0, 27.0, 27.0);
        ([x, y], closed)
    }

    /// A fort (or its parade ground): the original shows only what forts are for,
    /// or that Seth has cursed it. Its company's window opens from its soldiers and
    /// its standard.
    fn fort_window(&mut self, ui: &mut Ui) -> Option<InfoAction> {
        const G: usize = 89;
        let title = ui.t(G, 0);
        let ([x, y], closed) = self.frame(ui, 29, 16, "");
        ui.centred(Font::LargeBlackOnLight, &title, x, y + 10.0, 29.0 * 16.0);
        let text = ui.t(G, 2);
        ui.wrapped(Font::NormalBlackOnLight, &text, x + 32.0, y + 16.0 * 16.0 - 158.0, 25.0 * 16.0);
        closed.then_some(InfoAction::Close)
    }

    /// The recruiter: its weapons and chariots in store, and how fast it trains
    /// recruits, by its staff, its weapons and whether any fort or tower wants men.
    fn recruiter_window(&mut self, ui: &mut Ui, world: &World, b: &Building) -> Option<InfoAction> {
        const G: usize = 136;
        let title = ui.t(G, 0);
        let ([x, y], closed) = self.frame(ui, 29, 16, "");
        ui.centred(Font::LargeBlackOnLight, &title, x, y + 10.0, 29.0 * 16.0);
        let stored = |r: u16| b.stock.get(r as usize).copied().unwrap_or(0);
        let (weapons, chariots) = (osiris_sim::military::WEAPONS, osiris_sim::military::CHARIOTS);
        let mut rows = vec![(weapons, stored(weapons) / resource_load(), if stored(weapons) / resource_load() == 1 { 15 } else { 2 })];
        if stored(chariots) != 0 {
            rows.push((chariots, stored(chariots) / resource_load(), if stored(chariots) / resource_load() == 1 { 14 } else { 13 }));
        }
        for (i, &(r, n, label)) in rows.iter().enumerate() {
            let ry = y + 40.0 + 22.0 * i as f32;
            ui.icon(r, x + 32.0, ry);
            let line = format!("{} {}", n, ui.t(G, label));
            ui.label(Font::NormalBlackOnLight, &line, x + 58.0, ry + 4.0);
        }
        let needed = world.workers_needed(b.kind).max(1);
        let pct = b.workers * 100 / needed;
        let tier = |full, two_thirds, third, least| match pct {
            p if p >= 100 => full,
            p if p >= 66 => two_thirds,
            p if p >= 33 => third,
            _ => least,
        };
        let status = if b.road.is_none() {
            ui.t(TEXT_FRAME, 25)
        } else if b.workers < 1 {
            ui.t(G, 3)
        } else if !world.recruits_wanted(b.id) {
            ui.t(G, 4)
        } else if stored(weapons) < 1 {
            ui.t(G, tier(5, 6, 7, 8))
        } else {
            ui.t(G, tier(9, 10, 11, 12))
        };
        ui.wrapped(Font::NormalBlackOnLight, &status, x + 32.0, y + 90.0, 25.0 * 16.0);
        Self::workers(ui, world, b, x + 16.0, y + 142.0, "");
        closed.then_some(InfoAction::Close)
    }

    /// A company, as the original lays its window out: its name and arm; its
    /// standard (emblem, flag, and the pole with its morale and experience balls);
    /// its strength, health, experience and morale; the four order buttons and
    /// Return to Fort, with what the hovered (or current) one means; and the switch
    /// that turns its line.
    fn company_window(&mut self, ui: &mut Ui, world: &mut World, c: usize) -> Option<InfoAction> {
        use osiris_sim::military::{self as mil, Order};
        const G: usize = 138;
        let Some(co) = world.military.companies.get(c).filter(|co| co.fort != 0).cloned() else { return Some(InfoAction::Close) };
        let (wb, hb) = (29, 22);
        let ([x, y], closed) = self.frame(ui, wb, hb, "");
        let w = wb as f32 * 16.0;
        let name = ui.t(G, c % 10).trim_matches('"').to_owned();
        ui.centred(Font::LargeBlackOnLight, &name, x, y + 10.0, w);
        // The original's window gives charioteers the infantry's banner and infantry
        // the chariots' (its standards in the field have them the right way round).
        let (arm, buttons, banner) = match co.kind {
            mil::CHARIOTEER => (76, 2, 0),
            mil::INFANTRY => (74, 0, 18),
            _ => (75, 1, 9),
        };
        let subtitle = ui.t(G, arm);
        ui.centred(Font::NormalBlackOnLight, &subtitle, x, y + 30.0, w);
        // The standard, top to bottom, each part centred in a 40-pixel column.
        let size = |ui: &Ui, id: u32| ui.r.record(id).map_or((0.0, 0.0), |r| (r.width as f32, r.height as f32));
        let column = |width: f32| x + 16.0 + ((40.0 - width) / 2.0).trunc();
        let emblem = ui.img.company_emblems + (c % 10) as u32;
        let (ew, eh) = size(ui, emblem);
        ui.image(emblem, column(ew), y + 16.0);
        let flag = ui.img.company_flags + banner + if world.company_halted(c) { 8 } else { 0 };
        let (fw, fh) = size(ui, flag);
        ui.image(flag, column(fw), y + 16.0 + eh);
        let pole = ui.img.standard_pole + (20 - co.morale / 5).clamp(0, 20) as u32;
        let ball = ui.img.experience_ball + mil::experience_ball(co.experience);
        let (pw, _) = size(ui, pole);
        let (bw, _) = size(ui, ball);
        ui.image(pole, column(pw), y + 16.0 + eh + fh);
        ui.image(ball, column(bw), y + 16.0 + eh + fh);
        // Strength, health, experience and morale.
        let men = world.company_men(c) - co.abroad.max(0) as usize;
        let label = ui.t(G, 23);
        ui.label(Font::NormalBlackOnLight, &label, x + 100.0, y + 60.0);
        ui.label(Font::NormalBlackOnLight, &men.to_string(), x + 294.0, y + 60.0);
        let label = ui.t(G, 24);
        ui.label(Font::NormalBlackOnLight, &label, x + 100.0, y + 80.0);
        let health = match world.company_wounds(c) {
            p if p < 1 => 26,
            p if p < 21 => 27,
            p if p < 41 => 28,
            p if p < 56 => 29,
            p if p < 71 => 30,
            p if p < 91 => 31,
            _ => 32,
        };
        let health = ui.t(G, health);
        ui.label(Font::NormalBlackOnLight, &health, x + 300.0, y + 80.0);
        let label = format!("{} {}", ui.t(G, 73), ui.t(G, 25));
        ui.label(Font::NormalBlackOnLight, &label, x + 100.0, y + 100.0);
        let rank = mil::experience_rank(co.experience);
        ui.image(ui.img.experience_icons + rank as u32, x + 275.0, y + 100.0);
        let rank = ui.t(G, 60 + rank);
        ui.label(Font::NormalBlackOnLight, &rank, x + 300.0, y + 100.0);
        let label = format!("{} {}", ui.t(G, 73), ui.t(G, 36));
        ui.label(Font::NormalBlackOnLight, &label, x + 100.0, y + 120.0);
        let morale = ui.t(G, 37 + (co.morale / 5).clamp(0, 20) as usize);
        ui.label(Font::NormalBlackOnLight, &morale, x + 300.0, y + 120.0);
        let mut action = closed.then_some(InfoAction::Close);
        if men == 0 {
            // Why the fort stands empty: the men are away or on their way, or there
            // is no recruiter to raise them.
            let recruiter = world.buildings.iter().any(|b| b.kind == mil::RECRUITER && b.workers > 0);
            let why = ui.t(G, if recruiter { 10 } else { 11 });
            ui.wrapped(Font::NormalBlackOnLight, &why, x + 32.0, y + 172.0, (wb - 4) as f32 * 16.0);
            return action;
        }
        // The order buttons: hold tight, hold loose (charioteers: charge), engage,
        // mop up, and return to fort. A held line's picture is mirrored unless the
        // line is turned; Return to Fort's always is.
        let chariots = co.kind == mil::CHARIOTEER;
        let current = match co.order {
            Order::HoldTight => Some(0),
            Order::HoldLoose => Some(1),
            Order::Charge if !co.charged => Some(1),
            Order::Charge => None,
            Order::Engage => Some(2),
            Order::MopUp => Some(3),
        };
        let rect = |i: usize| [x + 19.0 + 85.0 * i as f32, y + 139.0, 84.0, 84.0];
        let hovered = (0..5).find(|&i| ui.hot(rect(i)));
        let base = ui.img.company_orders[buttons];
        for i in 0..5 {
            let r = rect(i);
            let focus = hovered.map_or(current == Some(i), |h| h == i);
            panel::button_border(ui.r, ui.panels, r[0], r[1], 84, 84, focus);
            let (image, mirrored) = match i {
                1 if chariots => (base + if co.charged { 6 } else { 1 }, false),
                4 => (base + if co.at_fort { 5 } else { 4 }, true),
                _ => (base + i as u32, !co.rotate),
            };
            ui.r.image_flipped(image, [x + 21.0 + 85.0 * i as f32, y + 141.0], [1.0; 4], osiris_render::Space::Screen, mirrored);
        }
        let clicked = (0..5).find(|&i| ui.clicked(rect(i)));
        match clicked {
            Some(4) if !co.at_fort => {
                world.return_company(c);
                action = Some(InfoAction::Close);
            }
            Some(4) | None => {}
            Some(1) if chariots && co.charged => {}
            Some(i) => {
                let order = match i {
                    0 => Order::HoldTight,
                    1 if chariots => Order::Charge,
                    1 => Order::HoldLoose,
                    2 => Order::Engage,
                    _ => Order::MopUp,
                };
                world.set_order(c, order);
                // Mopping up, the company goes after the enemy by itself; for the
                // rest the player now clicks where to send it.
                action = Some(if order == Order::MopUp { InfoAction::Close } else { InfoAction::SelectCompany(c) });
            }
        }
        // What the hovered button, or else the current order, means.
        panel::inner_panel(ui.r, ui.panels, x + 16.0, y + 230.0, wb - 2, 5);
        let standing = match co.order {
            Order::HoldTight => 0,
            Order::HoldLoose | Order::Charge => 1,
            Order::Engage => 2,
            Order::MopUp => 3,
        };
        let (title, text) = match hovered.unwrap_or(standing) {
            0 => (12, 13),
            1 if chariots => (20, 21),
            1 => (14, 15),
            2 => (16, 17),
            3 => (18, 19),
            _ => (58, 22),
        };
        let title = ui.t(G, title);
        ui.label(Font::NormalYellow, &title, x + 24.0, y + 236.0);
        let text = ui.t(G, text);
        ui.wrapped(Font::NormalWhiteOnDark, &text, x + 24.0, y + 252.0, (wb - 4) as f32 * 16.0);
        // The switch that turns the line.
        let r = [x + ((wb - 20) * 16 / 2) as f32, y + (hb * 16 - 40) as f32, 320.0, 30.0];
        panel::button_border(ui.r, ui.panels, r[0], r[1], 320, 30, ui.hot(r));
        let rotate = ui.t(G, 77);
        ui.centred(Font::NormalBlackOnLight, &rotate, r[0], y + ((hb - 2) * 16) as f32, 320.0);
        if ui.clicked(r) {
            world.rotate_line(c);
        }
        action
    }

    /// A walker: tabs for each walker on the tile, then the chosen one's portrait,
    /// name, what he is and where from, and what he carries. Returns the tab clicked.
    fn figure_window(&mut self, ui: &mut Ui, world: &World, ids: &[u32], selected: usize) -> (Option<InfoAction>, Option<usize>) {
        const TYPE_NAMES: usize = 64;
        let ([x, y], closed) = self.frame(ui, 29, 22, "");
        panel::inner_panel(ui.r, ui.panels, x + 16.0, y + 40.0, 27, 13);
        let mut pick = None;
        for (i, &fid) in ids.iter().enumerate() {
            let Some(f) = world.figures.get(fid) else { continue };
            let rect = [x + 27.0 + 60.0 * i as f32, y + 45.0, 52.0, 52.0];
            panel::button_border(ui.r, ui.panels, rect[0], rect[1], 52, 52, i == selected);
            ui.image(ui.img.portraits + f.kind as u32, rect[0] + 4.0, rect[1] + 4.0);
            if ui.clicked(rect) {
                pick = Some(i);
            }
        }
        let Some(f) = ids.get(selected).and_then(|&fid| world.figures.get(fid)) else {
            return (Some(InfoAction::Close), None);
        };
        ui.image(ui.img.portraits + f.kind as u32, x + 30.0, y + 108.0);
        // Boats have boat names; everyone else a person's name.
        let boat = matches!(f.kind, 20 | 25 | 76 | 77 | 78 | 92 | 93 | 100 | 101);
        let (group, count) = if boat { (261, 16) } else { (254, 128) };
        // Animals have no names.
        let name = if osiris_sim::animals::is_animal(f.kind) { ui.t(TYPE_NAMES, f.kind as usize) } else { ui.t(group, f.id as usize % count) };
        ui.label(Font::LargeBlackOnDark, &name, x + 90.0, y + 108.0);
        let mut kind = ui.t(TYPE_NAMES, f.kind as usize);
        if let Some(home) = world.buildings.get(f.home) {
            let home_name = world.defs.building(home.kind).and_then(|d| d.text_id).filter(|&g| g > 0).map(|g| ui.t(g as usize, 0)).unwrap_or_default();
            if !home_name.is_empty() {
                kind = format!("{kind} ({home_name})");
            }
        }
        ui.label(Font::NormalBlackOnDark, &kind, x + 92.0, y + 139.0);
        if f.cargo > 0 && f.amount > 0 && f.cargo < 36 {
            let what = format!("{} {}", f.amount, ui.t(TEXT_RESOURCES, f.cargo as usize));
            ui.icon(f.cargo, x + 90.0, y + 160.0);
            ui.label(Font::NormalBlackOnDark, &what, x + 116.0, y + 162.0);
        }
        (closed.then_some(InfoAction::Close), pick)
    }

    /// The employee row: a dark panel with the worker count and a staffing line.
    fn workers(ui: &mut Ui, world: &World, b: &Building, x: f32, y: f32, desc: &str) {
        let needed = world.workers_needed(b.kind);
        if needed <= 0 {
            return;
        }
        panel::inner_panel(ui.r, ui.panels, x, y, 27, 4);
        ui.image(ui.img.context_icons + 14, x + 8.0, y + 8.0);
        let word = ui.t(TEXT_GENERAL, if b.workers == 1 { 12 } else { 13 });
        let line = format!("{} {} ({} {}", b.workers, word, needed, ui.t(TEXT_FRAME, 0));
        ui.label(Font::NormalWhiteOnDark, &line, x + 40.0, y + 12.0);
        let desc = if desc.is_empty() { Self::labor_line(ui, world, b) } else { desc.to_owned() };
        ui.wrapped(Font::NormalWhiteOnDark, &desc, x + 40.0, y + 30.0, 380.0);
    }

    /// Why a building has fewer workers than it needs.
    fn labor_line(ui: &Ui, world: &World, b: &Building) -> String {
        let needed = world.workers_needed(b.kind);
        if b.workers >= needed {
            return String::new();
        }
        let id = if world.population <= 0 {
            16
        } else if !world.has_labor_access(b.id) {
            17
        } else {
            18
        };
        ui.t(TEXT_FRAME, id)
    }

    /// The staffing tier among `n` lines: 0 for the worst-staffed, `n - 1` for full.
    fn tier(world: &World, b: &Building, n: usize) -> usize {
        let needed = world.workers_needed(b.kind).max(1);
        ((b.workers as f32 / needed as f32 * n as f32).floor() as usize).min(n - 1)
    }

    fn building_window(&mut self, ui: &mut Ui, world: &mut World, b: &Building) -> Option<InfoAction> {
        let def = world.defs.building(b.kind).cloned();
        let g = def.as_ref().and_then(|d| d.text_id).filter(|&g| g > 0).map_or(0, |g| g as usize);
        // The shrines share one group: each god's name and words, in pairs.
        let pair = if (kind::SHRINE_OSIRIS..=kind::SHRINE_BAST).contains(&b.kind) { 2 * (b.kind - kind::SHRINE_OSIRIS) as usize } else { 0 };
        let t = |ui: &Ui, i: usize| ui.t(g, if i <= 1 { i + pair } else { i });
        let name = if g > 0 { t(ui, 0) } else { ui.t(28, b.kind as usize) };
        if b.house.is_some() {
            return self.house_window(ui, world, b);
        }
        if b.monument.is_some() {
            return self.monument_window(ui, world, b);
        }
        if osiris_sim::military::fort_soldier(b.kind).is_some() || b.kind == osiris_sim::military::FORT_GROUND {
            return self.fort_window(ui);
        }
        if b.kind == osiris_sim::military::RECRUITER {
            return self.recruiter_window(ui, world, b);
        }
        if world.is_farm(b.kind) {
            return self.farm_window(ui, world, b, g, &name);
        }
        let flags = |f: &str| def.as_ref().is_some_and(|d| d.has_flag(f));
        let no_road = ui.t(TEXT_FRAME, 25);
        // Buildings that need no road (statues, shrines, gardens) never warn of one.
        let roadless = b.road.is_none() && !flags("no_road_access");
        match b.kind {
            kind::STORAGE_YARD => return self.yard_window(ui, world, b),
            kind::GRANARY => return self.granary_window(ui, world, b),
            kind::BAZAAR => return self.bazaar_window(ui, world, b),
            kind::WELL => {
                let ([x, y], closed) = self.frame(ui, 29, 14, &name);
                let line = t(ui, Self::well_line(world, b));
                ui.wrapped(Font::NormalBlackOnLight, &line, x + 32.0, y + 56.0, 27.0 * 16.0);
                return closed.then_some(InfoAction::Close);
            }
            kind::BOOTH | kind::BANDSTAND | kind::PAVILION => return self.venue_window(ui, world, b, g, &name),
            _ => {}
        }
        let outputs = def.as_ref().map_or(0, |d| d.outputs.len());
        let inputs: Vec<u16> = def.as_ref().map_or_else(Vec::new, |d| d.inputs.iter().filter_map(|i| world.resource_id(i)).collect());
        let output = def.as_ref().and_then(|d| d.outputs.first()).and_then(|o| world.resource_id(o));
        if outputs > 0 && flags("is_workshop") {
            // Workshops: progress, input stock and whether production can go on.
            let two = inputs.len() > 1;
            let ([x, y], closed) = self.frame(ui, 29, if two { 18 } else { 16 }, &name);
            if let Some(r) = output {
                ui.icon(r, x + 10.0, y + 10.0);
            }
            let pct = b.progress * 100 / world.max_progress(b.kind).max(1);
            let progress = format!("{} {}% {}", t(ui, 2), pct, t(ui, 3));
            ui.label(Font::NormalBlackOnLight, &progress, x + 32.0, y + 40.0);
            for (i, &r) in inputs.iter().enumerate() {
                let ry = y + 56.0 + 24.0 * i as f32;
                ui.icon(r, x + 32.0, ry);
                let n = b.stock[r as usize];
                let label = if two { t(ui, 13 + i) } else { t(ui, 12) };
                let line = format!("{} {} {}", label, n, ui.t(TEXT_GENERAL, if n == 1 { 10 } else { 11 }));
                ui.label(Font::NormalBlackOnLight, &line, x + 60.0, ry + 4.0);
            }
            let status = if b.road.is_none() {
                no_road
            } else if output.is_some_and(|r| world.is_mothballed(r)) {
                t(ui, 4)
            } else if b.workers <= 0 {
                t(ui, 5)
            } else if let Some(i) = inputs.iter().position(|&r| b.stock[r as usize] < resource_load()) {
                t(ui, 11 + i)
            } else {
                t(ui, 10 - Self::tier(world, b, 5))
            };
            let sy = y + if two { 110.0 } else { 86.0 };
            ui.wrapped(Font::NormalBlackOnLight, &status, x + 32.0, sy, 27.0 * 16.0);
            Self::workers(ui, world, b, x + 16.0, y + if two { 150.0 } else { 136.0 } - 12.0, " ");
            return closed.then_some(InfoAction::Close);
        }
        if outputs > 0 && b.kind != kind::HUNTING_LODGE {
            // Raw materials: quarries, mines, clay pits, wood and reeds.
            let ([x, y], closed) = self.frame(ui, 29, 17, &name);
            if let Some(r) = output {
                ui.icon(r, x + 14.0, y + 14.0);
            }
            let pct = b.progress * 100 / world.max_progress(b.kind).max(1);
            let progress = format!("{} {}% {}", t(ui, 2), pct, t(ui, 3));
            ui.label(Font::NormalBlackOnLight, &progress, x + 32.0, y + 44.0);
            let desc = t(ui, 1);
            ui.wrapped(Font::NormalBlackOnLight, &desc, x + 32.0, y + 66.0, 26.0 * 16.0);
            let status = if b.road.is_none() {
                no_road
            } else if output.is_some_and(|r| world.is_mothballed(r)) {
                t(ui, 4)
            } else if b.workers <= 0 {
                t(ui, 5)
            } else {
                t(ui, 9 - Self::tier(world, b, 4))
            };
            Self::workers(ui, world, b, x + 16.0, y + 140.0, &status);
            return closed.then_some(InfoAction::Close);
        }
        // Everything else: the description, then what the building is doing.
        let temple = (kind::TEMPLE_OSIRIS..=kind::TEMPLE_BAST).contains(&b.kind);
        // Schools and libraries show their papyrus, mortuaries their linen.
        let supply = world.walker_supplies(b.kind).map(|(r, _)| r);
        let ([x, y], closed) = self.frame(ui, 29, if supply.is_some() { 18 } else { 17 }, &name);
        let supplied = supply.map(|r| b.stock.get(r as usize).copied().unwrap_or(0));
        let walker_out = b.walkers[0] != 0;
        let status = if roadless {
            no_road
        } else if temple || world.workers_needed(b.kind) <= 0 {
            String::new()
        } else if b.workers <= 0 {
            let s = t(ui, 9);
            if s.is_empty() { t(ui, 2) } else { s }
        } else if b.kind == osiris_sim::irrigation::WATER_LIFT {
            // "...needs to be adjacent to water, or connected by an Irrigation Ditch..."
            if b.water == 0 { t(ui, 3) } else { String::new() }
        } else if b.kind == kind::MORTUARY && supplied.unwrap_or(0) < 1 {
            // "Without linen, we cannot prepare the dead..." Schools and libraries
            // have no such line: theirs says only whether they are staffed.
            t(ui, 4)
        } else if walker_out && supply.is_none() {
            t(ui, 2)
        } else {
            t(ui, 3)
        };
        let mut text_y = 46.0;
        if let (Some(r), Some(n)) = (supply, supplied) {
            ui.icon(r, x + 32.0, y + 44.0);
            let line = format!("{} {}", n, ui.t(TEXT_RESOURCES, r as usize));
            ui.label(Font::NormalBlackOnLight, &line, x + 60.0, y + 48.0);
            text_y = 72.0;
        }
        let desc = t(ui, 1);
        let line = if status.is_empty() || status == desc { desc } else { format!("{desc} {status}") };
        ui.wrapped(Font::NormalBlackOnLight, &line, x + 20.0, y + text_y, 27.0 * 16.0);
        Self::workers(ui, world, b, x + 16.0, y + if supply.is_some() { 152.0 } else { 136.0 }, "");
        closed.then_some(InfoAction::Close)
    }

    /// The Construction Foreman's report on a monument.
    fn monument_window(&mut self, ui: &mut Ui, world: &World, b: &Building) -> Option<InfoAction> {
        const G: usize = 178;
        use osiris_sim::monuments::{self as mon, Style};
        let def = mon::monument_def(b.kind)?;
        let title_id = def.title;
        let title = ui.t(198, title_id);
        let ([x, y], closed) = self.frame(ui, 29, 20, &title);
        let foreman = ui.t(G, 12);
        ui.centred(Font::NormalBlackOnLight, &foreman, x, y + 40.0, 29.0 * 16.0);
        let Some((phase, finished, needs)) = world.monument_status(b.id) else { return closed.then_some(InfoAction::Close) };
        let m = b.monument.as_ref().expect("monument");
        let has = |k: u16| world.buildings.iter().any(|g| g.kind == k && g.workers > 0);
        let pct = world.monument_percent(b.id);
        if def.style == Style::RoyalTomb {
            use osiris_sim::royal_tombs::{ARTISANS_GUILD, TOMB_ARTISAN};
            let lamps = world.royal_tomb_lamps(b.id);
            let waiting = |k: u16| world.royal_tomb_waiting(b.id, k);
            let coming = |k: u16| world.figures.iter().any(|f| f.kind == k && f.target == b.id);
            let line = if finished {
                146
            } else if pct >= 100 {
                145
            } else if world.royal_tomb_access(b.id, (b.x, b.y)).is_none() {
                148
            } else if waiting(mon::STONEMASON) && !has(kind::STONEMASONS_GUILD) {
                14
            } else if waiting(TOMB_ARTISAN) && !has(ARTISANS_GUILD) {
                105
            } else if lamps < 100 && !m.chambers.iter().any(|c| c.worker != 0) {
                143
            } else if waiting(TOMB_ARTISAN) && !coming(TOMB_ARTISAN) {
                106
            } else if waiting(mon::STONEMASON) && !coming(mon::STONEMASON) {
                18
            } else {
                144
            };
            let text = if line == 144 { format!("{} {} {}% {}", ui.t(G, 144), ui.t(G, 2), pct, ui.t(G, 0)) } else { ui.t(G, line) };
            ui.wrapped(Font::NormalBlackOnLight, &text, x + 32.0, y + 66.0, 26.0 * 16.0);
            if !finished {
                panel::inner_panel(ui.r, ui.panels, x + 16.0, y + 180.0, 27, 5);
                ui.icon(34, x + 32.0, y + 192.0);
                let s = format!("{} {}", ui.t(G, 147), lamps.max(0));
                ui.label(Font::NormalWhiteOnDark, &s, x + 60.0, y + 194.0);
            }
            return closed.then_some(InfoAction::Close);
        }
        let brick = matches!(def.style, Style::Mastaba);
        // (finished, going well) lines.
        let obelisk = matches!(def.style, Style::Obelisk { .. });
        let (done_line, fine) = if brick {
            (41, 40)
        } else if def.style == Style::Sphinx {
            // Rough shape, fine carving, then painting (steps 1-5, 6-13, 14-15).
            (48, match phase {
                p if p < 7 => 45,
                p if p < 15 => 46,
                _ => 47,
            })
        } else if obelisk {
            // Carpenters' scaffolding, then the masons' carving.
            (44, if def.crew(phase).contains(&mon::CARPENTER) { 42 } else { 43 })
        } else {
            (38, if matches!(def.style, Style::Pyramid(mon::Family::Stepped)) { 37 } else { 31 })
        };
        let crew = world.monument_crew(b.id);
        // Each craftsman's guild, the foreman's line when there is none, and when none comes.
        let guild = |k: u16| match k {
            mon::BRICKLAYER => (kind::BRICKLAYERS_GUILD, 15, 19),
            mon::CARPENTER => (kind::CARPENTERS_GUILD, 16, 20),
            _ => (kind::STONEMASONS_GUILD, 14, 18),
        };
        let no_guild = crew.iter().map(|&k| guild(k)).find(|g| !has(g.0)).map(|g| g.1);
        let absent = crew.iter().find(|&&k| !m.has_craftsman(k)).map(|&k| guild(k).2);
        let short = needs.iter().find(|&&(r, got, want)| got < want && world.yards_stored(r) < osiris_sim::economy::LOAD).map(|n| n.0);
        let line = if finished {
            ui.t(G, done_line)
        } else if def.laborers(phase) {
            let work = if !has(kind::WORK_CAMP) {
                ui.t(G, 13)
            } else if !world.figures.iter().any(|f| f.kind == osiris_sim::farms::PEASANT && f.target == b.id) {
                ui.t(G, 17)
            } else {
                ui.t(G, if brick { 4 } else { 3 })
            };
            format!("{} {} {}% {}", work, ui.t(G, 2), pct, ui.t(G, 0))
        } else if let Some(line) = no_guild.or(absent) {
            ui.t(G, line)
        } else if let Some(r) = short {
            ui.t(G, match r {
                12 => 27,
                20 => 28,
                25 => 23,
                _ => 22,
            })
        } else {
            format!("{} {} {}% {}", ui.t(G, fine), ui.t(G, 2), pct, ui.t(G, 0))
        };
        ui.wrapped(Font::NormalBlackOnLight, &line, x + 32.0, y + 66.0, 26.0 * 16.0);
        if !finished && !needs.is_empty() {
            panel::inner_panel(ui.r, ui.panels, x + 16.0, y + 180.0, 27, 5);
            for (i, &(r, got, want)) in needs.iter().enumerate() {
                let ry = y + 192.0 + 24.0 * i as f32;
                ui.icon(r, x + 32.0, ry);
                let s = format!("{} / {} {}", got, want, ui.t(TEXT_RESOURCES, r as usize));
                ui.label(Font::NormalWhiteOnDark, &s, x + 60.0, ry + 2.0);
            }
        }
        closed.then_some(InfoAction::Close)
    }

    fn well_line(world: &World, b: &Building) -> usize {
        let houses: Vec<&Building> = world
            .buildings
            .iter()
            .filter(|h| h.house.as_ref().is_some_and(|h| h.population > 0))
            .filter(|h| (h.x - b.x).abs() <= 2 && (h.y - b.y).abs() <= 2)
            .collect();
        if houses.is_empty() {
            3
        } else if houses.iter().all(|h| h.house.as_ref().is_some_and(|h| h.coverage.water_supply > 0)) {
            2
        } else {
            1
        }
    }

    fn house_window(&mut self, ui: &mut Ui, world: &World, b: &Building) -> Option<InfoAction> {
        let h = b.house.clone().expect("house");
        if h.population <= 0 {
            let title = ui.t(TEXT_VACANT, 0);
            let ([x, y], closed) = self.frame(ui, 29, 21, &title);
            panel::inner_panel(ui.r, ui.panels, x + 16.0, y + 40.0, 27, 13);
            let line = ui.t(TEXT_VACANT, if b.road.is_some() { 1 } else { 2 });
            ui.wrapped(Font::NormalWhiteOnDark, &line, x + 36.0, y + 114.0, 25.0 * 16.0);
            return closed.then_some(InfoAction::Close);
        }
        let title = ui.t(TEXT_HOUSE_LEVELS, h.level as usize);
        let ([x, y], closed) = self.frame(ui, 29, 23, &title);
        let food_types = h.foods.iter().filter(|&&f| f > 0).count() as i32;
        let advice = if h.level as usize + 1 >= world.balance.houses.len() && h.blocked_by.is_none() {
            100
        } else {
            house_advice(h.blocked_by, h.decaying, food_types)
        };
        let line = ui.t(TEXT_HOUSE, advice);
        ui.wrapped(Font::NormalBlackOnLight, &line, x + 32.0, y + 40.0, 27.0 * 16.0);
        // The city's first four foods and the four goods, with this house's stock.
        let foods = city_foods(world);
        let food_stock = |r: u16| resource::food_slot(r).map_or(0, |s| h.foods[s]);
        for (i, &r) in foods.iter().enumerate() {
            let cx = x + 32.0 + 110.0 * i as f32;
            ui.icon(r, cx, y + 95.0);
            ui.label(Font::NormalBlackOnLight, &food_stock(r).to_string(), cx + 32.0, y + 100.0);
        }
        for (i, &r) in resource::HOUSE_GOODS.iter().enumerate() {
            let cx = x + 32.0 + 110.0 * i as f32;
            ui.icon(r, cx, y + 120.0);
            ui.label(Font::NormalBlackOnLight, &h.goods[i].to_string(), cx + 32.0, y + 124.0);
        }
        panel::inner_panel(ui.r, ui.panels, x + 16.0, y + 148.0, 27, 10);
        ui.image(ui.img.context_icons + 13, x + 34.0, y + 154.0);
        let cap = world.house_capacity(b.id);
        let room = if h.population > cap {
            format!("{}{}", h.population - cap, ui.t(TEXT_HOUSE, 21))
        } else {
            format!("{} {}", ui.t(TEXT_HOUSE, 22), cap - h.population)
        };
        let people = format!("{} {} ( {} )", h.population, ui.t(TEXT_HOUSE, 20), room);
        ui.label(Font::NormalWhiteOnDark, &people, x + 64.0, y + 164.0);
        let tax = if h.coverage.tax <= 0 {
            ui.t(TEXT_HOUSE, 23)
        } else {
            let amount = world.house_tax(h.level, h.population);
            format!("{} {} {}", ui.t(TEXT_HOUSE, 24), amount, ui.t(TEXT_HOUSE, 25))
        };
        ui.wrapped(Font::NormalWhiteOnDark, &tax, x + 36.0, y + 194.0, 25.0 * 16.0);
        if world.balance.house(h.level).food_types <= 0 {
            let s = ui.t(TEXT_HOUSE, 33);
            ui.wrapped(Font::NormalWhiteOnDark, &s, x + 36.0, y + 234.0, 25.0 * 16.0);
        }
        closed.then_some(InfoAction::Close)
    }

    fn farm_window(&mut self, ui: &mut Ui, world: &World, b: &Building, g: usize, name: &str) -> Option<InfoAction> {
        let ([x, y], closed) = self.frame(ui, 29, 19, name);
        let t = |ui: &Ui, i: usize| ui.t(g, i);
        let output = world.defs.building(b.kind).and_then(|d| d.outputs.first()).and_then(|o| world.resource_id(o));
        if let Some(r) = output {
            ui.icon(r, x + 10.0, y + 10.0);
        }
        let pct = b.progress * 100 / osiris_sim::farms::PROGRESS_MAX;
        let line = format!("{} {}% {} {} {}% {}", t(ui, 2), pct, t(ui, 3), t(ui, 12), world.fertility(b.id), t(ui, 13));
        ui.wrapped(Font::NormalBlackOnLight, &line, x + 32.0, y + 40.0, 26.0 * 16.0);
        let floodplain = world.is_floodplain_farm(b.id);
        let status = if b.workers <= 0 && !floodplain {
            t(ui, 5)
        } else if b.road.is_none() {
            ui.t(TEXT_FRAME, 25)
        } else if output.is_some_and(|r| world.is_mothballed(r)) {
            t(ui, 4)
        } else if floodplain {
            let days = b.labor_days;
            t(ui, if days > 0 { 6 } else { 5 })
        } else {
            t(ui, 10 - Self::tier(world, b, 5))
        };
        ui.wrapped(Font::NormalBlackOnLight, &status, x + 32.0, y + 66.0, 26.0 * 16.0);
        // "This farmland is irrigated." or "...not irrigated."
        let irrigated = ui.t(177, if world.is_irrigated(b.id) { 0 } else { 1 });
        ui.wrapped(Font::NormalBlackOnLight, &irrigated, x + 32.0, y + 206.0, 26.0 * 16.0);
        let desc = t(ui, 1);
        ui.wrapped(Font::NormalBlackOnLight, &desc, x + 32.0, y + 226.0, 26.0 * 16.0);
        if !floodplain {
            Self::workers(ui, world, b, x + 16.0, y + 100.0, "");
        }
        closed.then_some(InfoAction::Close)
    }

    fn yard_window(&mut self, ui: &mut Ui, world: &World, b: &Building) -> Option<InfoAction> {
        let title = ui.t(TEXT_YARD, 0);
        let ([x, y], closed) = self.frame(ui, 29, 21, &title);
        let total = world.total_stored(b.id);
        let line = if b.road.is_none() {
            ui.t(TEXT_FRAME, 25)
        } else if total >= osiris_sim::storage::CAPACITY {
            ui.t(TEXT_YARD, 13)
        } else if b.spaces.iter().all(|s| s.1 > 0) {
            ui.t(TEXT_YARD, 14)
        } else if total == 0 {
            format!("{} {}", ui.t(TEXT_YARD, 1), ui.t(TEXT_YARD, 22))
        } else {
            ui.t(TEXT_YARD, 1)
        };
        ui.wrapped(Font::NormalBlackOnLight, &line, x + 22.0, y + 36.0, 28.0 * 16.0 - 30.0);
        Self::storage_totals(ui, total, x + 24.0, y + 95.0);
        let mut held: Vec<(u16, i32)> = Vec::new();
        for &(r, n) in &b.spaces {
            if n <= 0 {
                continue;
            }
            match held.iter_mut().find(|h| h.0 == r) {
                Some(h) => h.1 += n,
                None => held.push((r, n)),
            }
        }
        held.sort();
        for (i, (r, n)) in held.iter().take(9).enumerate() {
            let (cx, cy) = (x + [32.0, 172.0, 292.0][i % 3], y + 116.0 + 26.0 * (i / 3) as f32);
            ui.icon(*r, cx, cy);
            let s = format!("{} {}", n, ui.t(TEXT_RESOURCES, *r as usize));
            ui.label(Font::NormalBlackOnLight, &s, cx + 22.0, cy + 2.0);
        }
        let cart = world.figures.get(b.walkers[2]);
        let cart_line = match cart {
            Some(f) if f.amount > 0 && f.home == b.id => ui.t(TEXT_YARD, 16),
            Some(_) => ui.t(TEXT_YARD, 17),
            None => ui.t(TEXT_YARD, 15),
        };
        let staff = if b.workers <= 0 {
            Self::labor_line(ui, world, b)
        } else {
            let needed = world.workers_needed(b.kind).max(1);
            match b.workers * 100 / needed {
                p if p < 50 => ui.t(TEXT_FRAME, 27),
                p if p < 75 => ui.t(TEXT_FRAME, 26),
                _ => cart_line,
            }
        };
        Self::workers(ui, world, b, x + 16.0, y + 198.0, &staff);
        let orders = ui.t(TEXT_YARD, 2);
        if ui.button([x + 100.0, y + 21.0 * 16.0 - 40.0, 270.0, 24.0], &orders, Font::NormalBlackOnLight) {
            self.orders = true;
            self.scroll = 0;
        }
        if self.trade_button(ui) {
            return Some(InfoAction::Overseer(crate::advisors::Advisor::Trade));
        }
        closed.then_some(InfoAction::Close)
    }

    /// The small Overseer of Commerce button at the bottom left.
    fn trade_button(&self, ui: &mut Ui) -> bool {
        let [x, y, _, h] = self.rect;
        // Three frames per overseer; Trade is the fifth.
        let rect = [x + 40.0, y + h - 40.0, 28.0, 28.0];
        let frame = ui.hot(rect) as u32;
        ui.image_button(ui.img.advisor_buttons + 4 * 3 + frame, rect[0], rect[1], rect[2], rect[3])
    }

    fn storage_totals(ui: &mut Ui, total: i32, x: f32, y: f32) {
        let units = ui.t(TEXT_GENERAL, 17);
        let storing = format!("{} {} {}", ui.t(TEXT_GRANARY, 2), total, units);
        ui.label(Font::NormalBlackOnLight, &storing, x, y);
        let free = format!("{} {} {}", ui.t(TEXT_GRANARY, 3), (osiris_sim::storage::CAPACITY - total).max(0), units);
        ui.label(Font::NormalBlackOnLight, &free, x + 196.0, y);
    }

    fn granary_window(&mut self, ui: &mut Ui, world: &World, b: &Building) -> Option<InfoAction> {
        let title = ui.t(TEXT_GRANARY, 0);
        let ([x, y], closed) = self.frame(ui, 29, 17, &title);
        if b.road.is_none() {
            let s = ui.t(TEXT_FRAME, 25);
            ui.wrapped(Font::NormalBlackOnLight, &s, x + 32.0, y + 40.0, 28.0 * 16.0 - 40.0);
        }
        Self::storage_totals(ui, world.total_stored(b.id), x + 34.0, y + 60.0);
        let foods: Vec<u16> = (resource::GRAIN..=resource::GAMEMEAT).filter(|&r| b.stock[r as usize] > 0).take(4).collect();
        for (i, &r) in foods.iter().enumerate() {
            let (cx, cy) = (x + [34.0, 240.0][i % 2], y + 80.0 + 24.0 * (i / 2) as f32);
            ui.icon(r, cx, cy);
            let s = format!("{} {}", b.stock[r as usize], ui.t(TEXT_RESOURCES, r as usize));
            ui.label(Font::NormalBlackOnLight, &s, cx + 34.0, cy + 3.0);
        }
        Self::workers(ui, world, b, x + 16.0, y + 136.0, "");
        let orders = ui.t(TEXT_GRANARY, 5);
        if ui.button([x + (29.0 * 16.0 - 270.0) / 2.0, y + 17.0 * 16.0 - 40.0, 270.0, 24.0], &orders, Font::NormalBlackOnLight) {
            self.orders = true;
            self.scroll = 0;
        }
        closed.then_some(InfoAction::Close)
    }

    fn bazaar_window(&mut self, ui: &mut Ui, world: &World, b: &Building) -> Option<InfoAction> {
        let title = ui.t(TEXT_BAZAAR, 0);
        let ([x, y], closed) = self.frame(ui, 29, 17, &title);
        let has_stock = (resource::GRAIN..=resource::GAMEMEAT).chain(resource::HOUSE_GOODS).any(|r| b.stock[r as usize] > 0);
        let (seller, buyer) = (b.walkers[0] != 0, b.walkers[2] != 0);
        let id = if b.road.is_none() {
            None
        } else if b.workers <= 0 {
            Some(2)
        } else if !has_stock {
            Some(4)
        } else if seller && buyer {
            Some(3)
        } else if buyer {
            Some(10)
        } else if seller {
            Some(11)
        } else {
            Some(3)
        };
        let line = id.map_or_else(|| ui.t(TEXT_FRAME, 25), |i| ui.t(TEXT_BAZAAR, i));
        ui.wrapped(Font::NormalBlackOnLight, &line, x + 32.0, y + 36.0, 26.0 * 16.0);
        for (row, list) in [(0, city_foods(world)), (1, resource::HOUSE_GOODS.to_vec())] {
            for (i, &r) in list.iter().enumerate() {
                let (cx, cy) = (x + 32.0 + 110.0 * i as f32, y + 85.0 + 25.0 * row as f32);
                ui.icon(r, cx, cy);
                let f = if b.bazaar_buys(r) { Font::NormalBlackOnLight } else { Font::NormalYellow };
                ui.label(f, &b.stock[r as usize].to_string(), cx + 32.0, cy + 5.0);
            }
        }
        Self::workers(ui, world, b, x + 16.0, y + 136.0, "");
        let orders = ui.t(TEXT_GRANARY, 5);
        if ui.button([x + 100.0, y + 17.0 * 16.0 - 40.0, 270.0, 24.0], &orders, Font::NormalBlackOnLight) {
            self.orders = true;
            self.scroll = 0;
        }
        closed.then_some(InfoAction::Close)
    }

    fn venue_window(&mut self, ui: &mut Ui, world: &World, b: &Building, g: usize, name: &str) -> Option<InfoAction> {
        let hb = if b.kind == kind::BOOTH { 16 } else { 20 };
        let ([x, y], closed) = self.frame(ui, 29, hb, name);
        let t = |ui: &Ui, i: usize| ui.t(g, i);
        let days = |ui: &Ui, n: i32| format!("{} {}", n, ui.t(TEXT_GENERAL, if n == 1 { 44 } else { 45 }));
        let [j, m, d] = b.shows;
        let no_road = ui.t(TEXT_FRAME, 25);
        let (status, lines): (String, Vec<String>) = match b.kind {
            kind::BOOTH => {
                let s = if b.road.is_none() {
                    no_road
                } else if b.workers <= 0 {
                    t(ui, 4)
                } else if j > 0 {
                    t(ui, 3)
                } else {
                    t(ui, 2)
                };
                let show = if j > 0 { format!("{} {}", t(ui, 6), days(ui, j)) } else { t(ui, 5) };
                (format!("{} {}", t(ui, 1), s), vec![show])
            }
            kind::BANDSTAND => {
                let s = if b.road.is_none() {
                    no_road
                } else if b.workers <= 0 {
                    t(ui, 6)
                } else {
                    t(ui, match (j > 0, m > 0) {
                        (true, true) => 3,
                        (true, false) => 5,
                        (false, true) => 4,
                        _ => 2,
                    })
                };
                let jl = if j > 0 { format!("{} {}", t(ui, 10), days(ui, j)) } else { t(ui, 9) };
                let ml = if m > 0 { format!("{} {}", t(ui, 8), days(ui, m)) } else { t(ui, 7) };
                (s, vec![jl, ml])
            }
            _ => {
                let s = if b.road.is_none() {
                    no_road
                } else {
                    t(ui, match (j > 0, m > 0, d > 0) {
                        (false, false, false) => 2,
                        (true, true, true) => 3,
                        (true, false, false) => 4,
                        (false, true, false) => 5,
                        (false, false, true) => 6,
                        (true, true, false) => 7,
                        (true, false, true) => 8,
                        _ => 9,
                    })
                };
                let line = |ui: &Ui, n: i32, none: usize| if n > 0 { format!("{} {}", t(ui, none + 1), days(ui, n)) } else { t(ui, none) };
                (format!("{} {}", t(ui, 1), s), vec![line(ui, j, 11), line(ui, m, 13), line(ui, d, 15)])
            }
        };
        ui.wrapped(Font::NormalBlackOnLight, &status, x + 20.0, y + 46.0, 27.0 * 16.0);
        Self::workers(ui, world, b, x + 16.0, y + 100.0, "");
        for (i, l) in lines.iter().enumerate() {
            ui.label(Font::NormalBlackOnLight, l, x + 32.0, y + 170.0 + 20.0 * i as f32);
        }
        closed.then_some(InfoAction::Close)
    }

    /// Special orders: per resource, accept (up to a share of the building), get,
    /// empty or refuse; for a bazaar, buy or don't buy.
    fn orders_window(&mut self, ui: &mut Ui, world: &mut World, id: u32) {
        let Some(b) = world.buildings.get(id) else { return };
        let (bazaar, granary) = (b.kind == kind::BAZAAR, b.kind == kind::GRANARY);
        let list: Vec<u16> = if bazaar {
            (resource::GRAIN..=resource::GAMEMEAT).chain(resource::HOUSE_GOODS).collect()
        } else if granary {
            (resource::GRAIN..=resource::GAMEMEAT).filter(|&r| city_foods(world).contains(&r) || b.stock[r as usize] > 0).collect()
        } else {
            scenario_resources(world)
        };
        let screen = ui.r.screen;
        let rows = if bazaar { 10 } else { 8 };
        // The list's panel, then one or two buttons below it.
        let list_blocks = (rows as i32 * 25 + 8 + 15) / 16;
        let hb = (42 + list_blocks * 16 + if bazaar { 50 } else { 76 } + 15) / 16;
        let (w, h) = (29.0 * 16.0, hb as f32 * 16.0);
        let (x, y) = (((screen[0] - crate::sidebar::width() - w) / 2.0).max(0.0).floor(), ((screen[1] - h) / 2.0).max(40.0).floor());
        panel::outer_panel(ui.r, ui.panels, x, y, 29, hb);
        let title = if bazaar { ui.t(TEXT_BAZAAR, 7) } else if granary { ui.t(TEXT_GRANARY, 6) } else { ui.t(TEXT_YARD, 3) };
        ui.centred(Font::LargeBlackOnLight, &title, x, y + 12.0, w);
        panel::inner_panel(ui.r, ui.panels, x + 16.0, y + 42.0, 27, list_blocks);
        self.scroll = self.scroll.min(list.len().saturating_sub(rows));
        for (i, &r) in list.iter().skip(self.scroll).take(rows).enumerate() {
            let ry = y + 50.0 + 25.0 * i as f32;
            let row = [x + 20.0, ry - 2.0, w - 136.0, 22.0];
            ui.icon(r, x + 36.0, ry);
            let name = ui.t(TEXT_RESOURCES, r as usize);
            ui.label(Font::NormalWhiteOnDark, &name, x + 76.0, ry + 2.0);
            let b = world.buildings.get(id).expect("present");
            if bazaar {
                let (s, f) = if b.bazaar_buys(r) { (ui.t(TEXT_BAZAAR, 8), Font::NormalWhiteOnDark) } else { (ui.t(TEXT_BAZAAR, 9), Font::NormalBlackOnLight) };
                ui.label(f, &s, x + 300.0, ry + 2.0);
                if ui.clicked(row) {
                    world.buildings.get_mut(id).expect("present").toggle_bazaar_buys(r);
                }
                continue;
            }
            let o = b.order(r);
            let quarter = |ui: &Ui, tier: u8| if tier >= 4 { ui.t(TEXT_YARD, 28) } else { format!("{} {}", ui.t(TEXT_YARD, 24 + tier as usize), ui.t(TEXT_YARD, if granary { 30 } else { 29 })) };
            let (s, f) = match o {
                order::ACCEPT => (format!("{} {}", ui.t(TEXT_YARD, 18), quarter(ui, b.order_tier(r, false))), Font::NormalWhiteOnDark),
                order::GET => (format!("{} {}", ui.t(TEXT_YARD, 19), quarter(ui, b.order_tier(r, true))), Font::NormalYellow),
                order::EMPTY => (ui.t(TEXT_YARD, 21), Font::NormalBlackOnLight),
                _ => (ui.t(TEXT_YARD, 8), Font::NormalBlackOnLight),
            };
            ui.label(f, &s, x + 196.0, ry + 2.0);
            if matches!(o, order::ACCEPT | order::GET) {
                if ui.arrow(x + w - 112.0, ry - 3.0, false) {
                    world.buildings.get_mut(id).expect("present").change_order_tier(r, false);
                }
                if ui.arrow(x + w - 88.0, ry - 3.0, true) {
                    world.buildings.get_mut(id).expect("present").change_order_tier(r, true);
                }
            }
            if ui.clicked(row) {
                world.buildings.get_mut(id).expect("present").cycle_order(r);
            }
        }
        let b = world.buildings.get(id).expect("present");
        if !bazaar {
            let empty = if granary {
                ui.t(TEXT_GRANARY, if b.empty_all { 8 } else { 7 })
            } else {
                ui.t(TEXT_YARD, if b.empty_all { 5 } else { 4 })
            };
            if ui.button([x + 80.0, y + h - 64.0, 300.0, 24.0], &empty, Font::NormalBlackOnLight) {
                world.buildings.get_mut(id).expect("present").toggle_empty_all();
            }
        }
        let none = ui.t(TEXT_YARD, 7);
        if ui.button([x + 80.0, y + h - 38.0, 300.0, 24.0], &none, Font::NormalBlackOnLight) {
            let b = world.buildings.get_mut(id).expect("present");
            if bazaar {
                b.orders = vec![1; resource::COUNT];
            } else {
                b.accept_none();
            }
        }
        if list.len() > rows {
            if self.scroll > 0 && ui.arrow(x + w - 46.0, y + 46.0, true) {
                self.scroll -= 1;
            }
            if self.scroll + rows < list.len() && ui.arrow(x + w - 46.0, y + 46.0 + 25.0 * (rows - 1) as f32, false) {
                self.scroll += 1;
            }
        }
        if ui.click.take().is_some_and(|c| !inside([x, y, w, h], c)) {
            self.orders = false;
        }
    }

    fn terrain_window(&mut self, ui: &mut Ui, world: &World, x: i32, y: i32) -> Option<InfoAction> {
        use osiris_sim::map::terrain as tr;
        let t = world.map.terrain.at_or(x, y, 0);
        let is = |m: u32| t & m != 0;
        let k = if is(tr::TREE) {
            1
        } else if is(tr::FLOODPLAIN) {
            if is(tr::WATER) { 20 } else { 19 }
        } else if is(tr::MARSHLAND) {
            21
        } else if is(tr::DUNE) {
            22
        } else if is(tr::ROCK) {
            if (x, y) == world.entry_point {
                14
            } else if (x, y) == world.exit_point {
                15
            } else if is(tr::ORE) {
                16
            } else {
                17
            }
        } else if is(tr::WATER) {
            3
        } else if is(tr::SHRUB) {
            4
        } else if is(tr::ROAD) {
            6
        } else if is(tr::CANAL) {
            7
        } else if is(tr::WALL) {
            24
        } else if is(tr::RUBBLE) {
            8
        } else if is(tr::MEADOW) {
            25
        } else {
            10
        };
        let title = ui.t(TEXT_TERRAIN, 10 + k);
        let ([wx, wy], closed) = self.frame(ui, 29, 20, &title);
        panel::inner_panel(ui.r, ui.panels, wx + 16.0, wy + 50.0, 27, 11);
        let desc = ui.t(TEXT_TERRAIN, 36 + k);
        ui.wrapped(Font::NormalWhiteOnDark, &desc, wx + 30.0, wy + 78.0, 26.0 * 16.0);
        closed.then_some(InfoAction::Close)
    }
}

fn resource_load() -> i32 {
    osiris_sim::economy::LOAD
}

/// The city's first four foods: those it can grow or import.
fn city_foods(world: &World) -> Vec<u16> {
    (resource::GRAIN..=resource::GAMEMEAT)
        .filter(|&r| {
            let made = world.defs.buildings.iter().flatten().any(|d| world.is_allowed(d.id) && d.outputs.iter().any(|o| world.resource_id(o) == Some(r)));
            let bought = world.trade.cities.iter().any(|c| c.trades() && c.sells.get(r as usize).copied().unwrap_or(false));
            made || bought
        })
        .take(4)
        .collect()
}

/// Resources that belong in a storage yard's orders: whatever the scenario can make or
/// trade.
fn scenario_resources(world: &World) -> Vec<u16> {
    (1..osiris_sim::trade::RESOURCES as u16)
        .filter(|&r| {
            let made = world.defs.buildings.iter().flatten().any(|d| world.is_allowed(d.id) && d.outputs.iter().any(|o| world.resource_id(o) == Some(r)));
            let traded = world.trade.cities.iter().any(|c| c.trades() && (c.sells[r as usize] || c.buys[r as usize]));
            made || traded
        })
        .collect()
}

/// Group 127 advice line for a house's first unmet need.
fn house_advice(need: Option<Need>, decaying: bool, food_types_have: i32) -> usize {
    let Some(need) = need else { return 101 };
    let (evolve, devolve) = match need {
        Need::Desirability => (70, 40),
        Need::Water => (71, 41),
        Need::WaterSupply => (72, 42),
        Need::Entertainment => (73, 43),
        Need::Education => (84, 54),
        Need::Religion => (90, 60),
        Need::Dentist => (93, 63),
        Need::Magistrate => (88, 58),
        Need::Health => (94, 64),
        Need::Food => (79 + food_types_have.clamp(0, 2) as usize, 49 + food_types_have.clamp(0, 2) as usize),
        Need::Pottery => (89, 59),
        Need::Linen => (97, 67),
        Need::Jewelry => (108, 112),
        Need::SecondLuxury => (114, 110),
        Need::Beer => (99, 69),
    };
    if decaying { devolve } else { evolve }
}
