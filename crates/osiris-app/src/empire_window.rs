//! The empire map: the empire's cities and trade routes, with the selected city's
//! trade in the panel along the bottom. Cities are opened for trade from here.
//!
//! The 1200x1600 map scrolls inside a frame of bars (Pharaoh_General group 172), 16
//! pixels in from the screen's edges and 130 above its bottom. Each city is drawn
//! with the image its kind and name give it, its name in dark red beside it, and a
//! flag waving over it when a trade route to it can be opened or is open. Only open
//! routes are drawn, a dot every few pixels along the way. Traders do not travel the
//! routes: they appear straight in the city. What the original moves along routes
//! are armies: invaders marching on the city and troops sent to a distant battle
//! (group 178 by land, 179 by sea), which Osiris does not draw yet. Text comes from
//! group 47 and city names from group 195.
//!
//! Opening a route asks first, in the original's yes/no window (text group 5), and
//! then shows the window saying the route is open (group 142).

use osiris_formats::empire::city;
use osiris_formats::{ImageLibrary, TextTable};
use osiris_render::{Renderer, Space, WHITE};
use osiris_sim::World;
use osiris_ui::{Font, PanelImages, draw_text, font, panel, rich_text, text_width};

fn img(r: &mut Renderer, id: u32, x: f32, y: f32) {
    r.image(id, [x, y], [1.0; 4], Space::Screen);
}

const MAP_W: f32 = 1200.0;
const MAP_H: f32 = 1600.0;
/// The frame's bars are 16 pixels thick; the map ends 120 pixels above the bottom
/// of the screen, where the panel's top bar is (Akhenaten's ui_empire_window.js:
/// `start_pos (16,16)`, `finish_pos (32,136)`, so the view's bottom edge sits at
/// `start_pos.y + (screen height - finish_pos.y)` = `16 + height - 136` = `height - 120`).
const BAR: f32 = 16.0;
const PANEL_TOP: f32 = 120.0;
/// The paneling art's own divider bar and corner pieces are drawn 20 pixels above
/// `PANEL_TOP`, so their top edge lands flush with the view boundary once the bar's
/// own height is added (Akhenaten's ui_empire_chrome.js `draw_paneling`: the bottom
/// rows tile at `max_pos.y - 140, -100, -60, -20` and the divider `hbar`/`cross` sit
/// at `max_pos.y - 140`).
const DIVIDER: f32 = 140.0;
const TEXT: usize = 47;
const CITY_NAMES: usize = 195;
/// City names on the map: 0x40e4 in the game's 555 colour.
const NAME_COLOR: [f32; 4] = [16.0 / 31.0, 7.0 / 31.0, 4.0 / 31.0, 1.0];
/// The inset box around a resource icon: dark top and left, white bottom and right.
const BOX_DARK: [f32; 4] = [16.0 / 31.0, 32.0 / 63.0, 16.0 / 31.0, 1.0];
/// Foreign cities' pictures, by city name id: offsets into group 169 (or group 5
/// once they trade). Egyptian cities all share one picture.
const FOREIGN_IMAGE: [u8; 66] = [
    11, 11, 14, 9, 11, 11, 9, 15, 11, 14, 11, 14, 1, 14, 16, 9, 11, 11, 9, 11, 11, 16, 11, 9, 14, 11, 0, 5, 11, 11, 11, 4, 11, 11, 11, 11, 2,
    12, 11, 11, 11, 11, 3, 14, 11, 9, 11, 11, 11, 9, 10, 11, 11, 11, 15, 11, 9, 4, 4, 11, 11, 14, 11, 11, 4, 4,
];
/// Resources counted in blocks or pieces rather than units: their amounts show in
/// loads.
const COUNTED: [usize; 7] = [10, 24, 25, 26, 28, 30, 35];
/// How far right of the original's place the "N of M" amounts are drawn, to clear the
/// widest tier badge (a quality-of-life change: the original overlaps them).
const AMOUNT_SHIFT: f32 = 6.0;

#[derive(Clone, Copy)]
pub struct EmpireImages {
    pub(crate) map: u32,
    pub(crate) bars: u32,
    pub(crate) route_dot: u32,
    /// Cities: group 5 for trading cities and ours, 169 for the rest, 164 and 167 for
    /// Pharaoh's city (167 also for ours once the governor is Pharaoh).
    pub(crate) cities_trading: u32,
    pub(crate) cities: u32,
    pub(crate) pharaoh: u32,
    pub(crate) pharaoh_trading: u32,
    pub(crate) flag: u32,
    pub(crate) tiers: u32,
    pub(crate) icons: u32,
    pub(crate) context: u32,
    pub(crate) advisors: u32,
    /// The OK and cancel buttons of the yes/no window: group 96, offsets 0 and 4.
    pub(crate) ok_cancel: u32,
}

impl EmpireImages {
    pub fn load(lib: &ImageLibrary) -> osiris_formats::Result<Self> {
        let g = |group| lib.group_id("Pharaoh_General", group, 0);
        Ok(Self {
            map: lib.group_id("Empire", 1, 0)?,
            bars: g(172)?,
            route_dot: g(149)? + 1,
            cities_trading: g(5)?,
            cities: g(169)?,
            pharaoh: g(164)?,
            pharaoh_trading: g(167)?,
            flag: g(222)?,
            tiers: g(171)?,
            icons: lib.group_id("Expansion", 3, 0)?,
            context: g(134)?,
            advisors: g(106)?,
            ok_cancel: g(96)?,
        })
    }

    /// The small icon of resource `r`.
    pub fn icon(&self, r: u16) -> u32 {
        self.icons + r as u32
    }

    fn city(&self, c: &osiris_sim::trade::TradeCity, pharaoh_rank: bool) -> u32 {
        self.city_image(c.city_type, c.name_id, pharaoh_rank)
    }

    /// The picture of a city of kind `city_type` named `name_id` (FUN_004425c0).
    pub(crate) fn city_image(&self, city_type: u8, name_id: u8, pharaoh_rank: bool) -> u32 {
        let foreign = FOREIGN_IMAGE.get(name_id as usize).copied().unwrap_or(11) as u32;
        match city_type {
            city::OURS if pharaoh_rank => self.pharaoh_trading,
            city::OURS => self.cities_trading,
            city::PHARAOH_TRADING => self.pharaoh_trading,
            city::PHARAOH => self.pharaoh,
            city::EGYPTIAN_TRADING => self.cities_trading + 6,
            city::EGYPTIAN => self.cities + 7,
            city::FOREIGN_TRADING => self.cities_trading + foreign,
            _ => self.cities + foreign,
        }
    }
}

/// A window over the empire map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmpirePopup {
    /// "Pay to open this land/water trade route?" for a city.
    Confirm(usize),
    /// The treasury is 5000 or more in debt: no route can be opened.
    NoMoney,
    /// "New trade route established." after opening the route to a city.
    Opened(usize),
}

/// What a click in the empire window did.
pub enum EmpireClick {
    Nothing,
    Close,
    /// The button to the Overseer of Commerce.
    Advisor,
    /// Agreed to pay to open the selected city's trade route.
    OpenRoute(usize),
}

#[derive(Default)]
pub struct EmpireWindow {
    /// Map pixel at the top-left of the view.
    scroll: [f32; 2],
    selected: Option<usize>,
    /// Where a drag started, and the scroll then.
    drag: Option<([f32; 2], [f32; 2])>,
    cursor: [f32; 2],
    centred: bool,
    /// Seconds the window has been open, for the flags.
    clock: f32,
    popup: Option<EmpirePopup>,
}

impl EmpireWindow {
    /// How much the map is enlarged: a screen wider than the map's 1200 pixels gets it
    /// stretched to fill the space between the side bars, rather than stone panels
    /// beside it (the original's screens were never wider than the map). Cities,
    /// names, flags and route dots keep their size and sit where the map puts them.
    fn scale(screen: [f32; 2]) -> f32 {
        ((screen[0] - 2.0 * BAR) / MAP_W).max(1.0)
    }

    /// The map's rectangle on screen.
    fn view(screen: [f32; 2]) -> [f32; 4] {
        let s = Self::scale(screen);
        let w = (screen[0] - 2.0 * BAR).min(MAP_W * s);
        let h = (screen[1] - PANEL_TOP - BAR).min(MAP_H * s);
        [((screen[0] - w) / 2.0).floor(), BAR, w, h]
    }

    /// Keeps the scroll, in screen pixels of the enlarged map, inside the map.
    fn clamp(&mut self, screen: [f32; 2]) {
        let (v, s) = (Self::view(screen), Self::scale(screen));
        self.scroll[0] = self.scroll[0].clamp(0.0, (MAP_W * s - v[2]).max(0.0));
        self.scroll[1] = self.scroll[1].clamp(0.0, (MAP_H * s - v[3]).max(0.0));
    }

    /// Centres the view on our city the first time it is shown.
    fn centre_once(&mut self, world: &World, screen: [f32; 2]) {
        if self.centred {
            return;
        }
        self.centred = true;
        let v = Self::view(screen);
        if let Some(c) = world.trade.cities.iter().find(|c| c.city_type == city::OURS) {
            let s = Self::scale(screen);
            self.scroll = [c.pos.0 as f32 * s - v[2] / 2.0, c.pos.1 as f32 * s - v[3] / 2.0];
        }
        self.clamp(screen);
    }

    fn to_screen(&self, screen: [f32; 2], p: (i32, i32)) -> [f32; 2] {
        let (v, s) = (Self::view(screen), Self::scale(screen));
        [(v[0] + p.0 as f32 * s - self.scroll[0]).floor(), (v[1] + p.1 as f32 * s - self.scroll[1]).floor()]
    }

    /// The top-left corner of an image `w` by `h` that the map places at `p`: on an
    /// enlarged map its centre follows the map, so it stays on its spot of the art.
    fn place(&self, screen: [f32; 2], p: (i32, i32), w: f32, h: f32) -> [f32; 2] {
        let grow = Self::scale(screen) - 1.0;
        let at = self.to_screen(screen, p);
        [(at[0] + w / 2.0 * grow).floor(), (at[1] + h / 2.0 * grow).floor()]
    }

    pub fn select(&mut self, city: Option<usize>) {
        self.selected = city;
    }

    pub fn show(&mut self, popup: Option<EmpirePopup>) {
        self.popup = popup;
    }

    pub fn popup(&self) -> Option<EmpirePopup> {
        self.popup
    }

    /// Right-click: the only window it closes is the one saying there is no money
    /// ("Right-click to continue"); the others wait for their buttons.
    pub fn right_click(&mut self) {
        if self.popup == Some(EmpirePopup::NoMoney) {
            self.popup = None;
        }
    }

    pub fn tick(&mut self, dt: f32) {
        self.clock += dt;
    }

    pub fn scroll_by(&mut self, screen: [f32; 2], dx: f32, dy: f32) {
        self.scroll[0] += dx;
        self.scroll[1] += dy;
        self.clamp(screen);
    }

    pub fn hover(&mut self, screen: [f32; 2], p: [f32; 2]) {
        self.cursor = p;
        if let Some((start, from)) = self.drag {
            self.scroll = [from[0] - (p[0] - start[0]), from[1] - (p[1] - start[1])];
            self.clamp(screen);
        }
    }

    pub fn release(&mut self) {
        self.drag = None;
    }

    /// The screen's horizontal centre: the trade panel's buttons and columns are laid
    /// out from here, not from a fixed-width box (Akhenaten's ui_empire_trade_panel.js
    /// `empire_window_layout_ui`, which positions everything from `centerX`).
    fn panel_x(screen: [f32; 2]) -> f32 {
        (screen[0] / 2.0).floor()
    }

    fn open_button(screen: [f32; 2]) -> [f32; 4] {
        [Self::panel_x(screen) - 220.0, screen[1] - 40.0, 440.0, 20.0]
    }

    fn help_button(screen: [f32; 2]) -> [f32; 4] {
        [16.0, screen[1] - 40.0, 27.0, 27.0]
    }

    fn close_button(screen: [f32; 2]) -> [f32; 4] {
        [screen[0] - 40.0, screen[1] - 40.0, 24.0, 24.0]
    }

    fn advisor_button(screen: [f32; 2]) -> [f32; 4] {
        [16.0, screen[1] - 121.0, 28.0, 28.0]
    }

    /// The top-left of the original's 640x480 layout, centred on the screen.
    fn origin(screen: [f32; 2]) -> [f32; 2] {
        [((screen[0] - 640.0) / 2.0).floor(), ((screen[1] - 480.0) / 2.0).floor()]
    }

    /// The yes/no window: 480x160 at (80, 80), yes (the tick) at (256, 100) and no
    /// at (192, 100) in it, each 34 pixels square.
    fn yes_no(screen: [f32; 2]) -> ([f32; 2], [f32; 4], [f32; 4]) {
        let o = Self::origin(screen);
        let at = [o[0] + 80.0, o[1] + 80.0];
        (at, [at[0] + 256.0, at[1] + 100.0, 34.0, 34.0], [at[0] + 192.0, at[1] + 100.0, 34.0, 34.0])
    }

    /// The route-opened window's own top-left: Akhenaten's ui_trade_opened_window.js
    /// centres a 30x14 tile panel on the screen directly, not nested 80 pixels into
    /// the 640x480 layout the way Osiris drew it before.
    fn opened_origin(screen: [f32; 2]) -> [f32; 2] {
        [((screen[0] - 480.0) / 2.0).floor(), ((screen[1] - 224.0) / 2.0).floor()]
    }

    /// The route-opened window's buttons: to the Overseer of Commerce, and back to
    /// the map.
    fn opened_buttons(screen: [f32; 2]) -> ([f32; 4], [f32; 4]) {
        let o = Self::opened_origin(screen);
        ([o[0] + 12.0, o[1] + 184.0, 28.0, 28.0], [o[0] + 442.0, o[1] + 188.0, 24.0, 24.0])
    }

    fn inside(r: [f32; 4], p: [f32; 2]) -> bool {
        p[0] >= r[0] && p[1] >= r[1] && p[0] < r[0] + r[2] && p[1] < r[1] + r[3]
    }

    /// The city under `p`, if any.
    fn city_at(&self, r: &Renderer, world: &World, images: &EmpireImages, p: [f32; 2]) -> Option<usize> {
        let pharaoh = world.assigned_rank() >= 10;
        world.trade.cities.iter().position(|c| {
            let (w, h) = r.record(images.city(c, pharaoh)).map_or((37.0, 34.0), |rec| (rec.width as f32, rec.height as f32));
            let at = self.place(r.screen, c.pos, w, h);
            p[0] >= at[0] && p[1] >= at[1] && p[0] < at[0] + w && p[1] < at[1] + h
        })
    }

    pub fn click(&mut self, r: &Renderer, world: &World, images: &EmpireImages, p: [f32; 2]) -> EmpireClick {
        let screen = r.screen;
        if let Some(popup) = self.popup {
            return self.popup_click(popup, screen, p);
        }
        if Self::inside(Self::close_button(screen), p) {
            return EmpireClick::Close;
        }
        if Self::inside(Self::advisor_button(screen), p) {
            return EmpireClick::Advisor;
        }
        if let Some(c) = self.selected
            && Self::inside(Self::open_button(screen), p)
            && world.trade.cities.get(c).is_some_and(|c| c.trades() && !c.open)
        {
            // The original asks before paying, unless the treasury is 5000 or more in
            // debt; short of that the route opens even into debt.
            self.popup = Some(if world.out_of_money() { EmpirePopup::NoMoney } else { EmpirePopup::Confirm(c) });
            return EmpireClick::Nothing;
        }
        if Self::inside(Self::view(screen), p) {
            match self.city_at(r, world, images, p) {
                Some(c) => self.selected = Some(c),
                None => self.drag = Some((p, self.scroll)),
            }
        }
        EmpireClick::Nothing
    }

    fn popup_click(&mut self, popup: EmpirePopup, screen: [f32; 2], p: [f32; 2]) -> EmpireClick {
        match popup {
            EmpirePopup::Confirm(c) => {
                let (_, yes, no) = Self::yes_no(screen);
                if Self::inside(yes, p) {
                    self.popup = None;
                    return EmpireClick::OpenRoute(c);
                }
                if Self::inside(no, p) {
                    self.popup = None;
                }
            }
            EmpirePopup::NoMoney => {}
            EmpirePopup::Opened(_) => {
                let (advisor, close) = Self::opened_buttons(screen);
                if Self::inside(advisor, p) {
                    self.popup = None;
                    return EmpireClick::Advisor;
                }
                if Self::inside(close, p) {
                    self.popup = None;
                }
            }
        }
        EmpireClick::Nothing
    }

    pub fn draw(&mut self, r: &mut Renderer, panels: &PanelImages, world: &World, text: &TextTable, images: &EmpireImages) {
        let screen = r.screen;
        self.centre_once(world, screen);
        self.clamp(screen);
        let v = Self::view(screen);
        r.rect([0.0, 0.0], screen, [0.0, 0.0, 0.0, 1.0], Space::Screen);
        r.set_clip(Some(v));
        let s = Self::scale(screen);
        r.image_scaled(images.map, [v[0] - self.scroll[0], v[1] - self.scroll[1]], [MAP_W * s, MAP_H * s], WHITE, Space::Screen);
        let pharaoh = world.assigned_rank() >= 10;
        for c in &world.trade.cities {
            let image = images.city(c, pharaoh);
            let (w, h) = r.record(image).map_or((0.0, 0.0), |rec| (rec.width as f32, rec.height as f32));
            let at = self.place(screen, c.pos, w, h);
            img(r, image, at[0], at[1]);
            let name = text.get(CITY_NAMES, c.name_id as usize).unwrap_or("");
            let tw = text_width(r, Font::SmallPlain, name) as f32;
            let (x, y) = match c.text_align {
                0 => (at[0] - tw, at[1] + (h / 2.0).floor()),
                1 => (at[0] + ((w - tw) / 2.0).floor(), at[1] - 10.0),
                2 => (at[0] + w, at[1] + (h / 2.0).floor()),
                _ => (at[0] + ((w - tw) / 2.0).floor(), at[1] + h + 5.0),
            };
            draw_text(r, Font::SmallPlain, name, x, y, NAME_COLOR);
            if c.trades() {
                let frame = self.flag_frame(r, images.flag);
                let fh = r.record(images.flag + frame).map_or(0.0, |rec| rec.height as f32);
                img(r, images.flag + frame, at[0] + (w / 2.0).floor(), at[1] + h - fh);
            }
        }
        for c in world.trade.cities.iter().filter(|c| c.trades() && c.open) {
            let Some(route) = world.trade.routes.get(c.route as usize) else { continue };
            for p in route_dots(route) {
                let at = self.to_screen(screen, p);
                img(r, images.route_dot, at[0], at[1]);
            }
        }
        r.set_clip(None);
        self.draw_frame(r, images);
        self.draw_panel(r, panels, world, text, images);
        if let Some(popup) = self.popup {
            self.draw_popup(r, panels, world, text, images, popup);
        }
    }

    fn draw_popup(&self, r: &mut Renderer, panels: &PanelImages, world: &World, text: &TextTable, images: &EmpireImages, popup: EmpirePopup) {
        let screen = r.screen;
        let t = |g: usize, i: usize| text.get(g, i).unwrap_or("").trim().to_owned();
        let centred = |r: &mut Renderer, f: Font, s: &str, x: f32, y: f32, w: f32| {
            let tw = text_width(r, f, s) as f32;
            draw_text(r, f, s, x + ((w - tw) / 2.0).max(0.0).floor(), y, font::BLACK);
        };
        let button = |r: &mut Renderer, id: u32, b: [f32; 4]| img(r, id + Self::inside(b, self.cursor) as u32, b[0], b[1]);
        match popup {
            EmpirePopup::Confirm(_) | EmpirePopup::NoMoney => {
                // Title and question from group 5: 2 and 3 for a land route, 4 and 5 for
                // a water route, 119 and 120 for no money.
                let id = match popup {
                    EmpirePopup::Confirm(c) if world.trade.cities.get(c).is_some_and(|c| c.sea) => 4,
                    EmpirePopup::Confirm(_) => 2,
                    _ => 119,
                };
                let (at, yes, no) = Self::yes_no(screen);
                panel::outer_panel(r, panels, at[0], at[1], 30, 10);
                centred(r, Font::LargeBlackOnLight, &t(5, id), at[0], at[1] + 20.0, 480.0);
                // A line under 420 pixels is centred; longer text wraps 420 wide from 30 in.
                let body = t(5, id + 1);
                if text_width(r, Font::NormalBlackOnLight, &body) < 420 {
                    centred(r, Font::NormalBlackOnLight, &body, at[0], at[1] + 60.0, 480.0);
                } else {
                    wrapped(r, &body, at[0] + 30.0, at[1] + 60.0, 420.0);
                }
                if popup == EmpirePopup::NoMoney {
                    centred(r, Font::NormalBlackOnLight, &t(13, 1), at[0], at[1] + 128.0, 480.0);
                } else {
                    button(r, images.ok_cancel, yes);
                    button(r, images.ok_cancel + 4, no);
                }
            }
            EmpirePopup::Opened(c) => {
                // Akhenaten's ui_trade_opened_window.js: its own 30x14 tile panel
                // centred on the screen (group 142: the title, the note about the
                // Overseer of Commerce and, for a water route, the reminder that ships
                // need a dock).
                let o = Self::opened_origin(screen);
                panel::outer_panel(r, panels, o[0], o[1], 30, 14);
                centred(r, Font::LargeBlackOnLight, &t(142, 0), o[0], o[1] + 16.0, 480.0);
                if world.trade.cities.get(c).is_some_and(|c| c.sea) {
                    wrapped(r, &t(142, 1), o[0] + 32.0, o[1] + 56.0, 416.0);
                    wrapped(r, &t(142, 3), o[0] + 32.0, o[1] + 120.0, 416.0);
                } else {
                    wrapped(r, &t(142, 1), o[0] + 32.0, o[1] + 88.0, 416.0);
                }
                centred(r, Font::NormalBlackOnLight, &t(142, 2), o[0] + 48.0, o[1] + 192.0, 384.0);
                let (advisor, close) = Self::opened_buttons(screen);
                button(r, images.advisors + 4 * 3, advisor);
                button(r, images.context + 4, close);
            }
        }
    }

    /// The flag's frame, 1 to its frame count, at its animation speed.
    fn flag_frame(&self, r: &Renderer, flag: u32) -> u32 {
        let Some(rec) = r.record(flag) else { return 1 };
        let n = (rec.num_animation_sprites as u64).max(1);
        let step = (self.clock * 1000.0) as u64 / (20 * (rec.animation_speed_id as u64).max(1));
        (step % n) as u32 + 1
    }

    /// Bars around the map and the stone panel below it.
    fn draw_frame(&self, r: &mut Renderer, images: &EmpireImages) {
        let screen = r.screen;
        let (vert, horiz, cross, bottom) = (images.bars, images.bars + 1, images.bars + 2, images.bars + 3);
        let (w, h) = (screen[0], screen[1]);
        for y in [h - DIVIDER, h - DIVIDER + 40.0, h - DIVIDER + 80.0, h - DIVIDER + 120.0] {
            let mut x = 0.0;
            while x < w - 70.0 {
                img(r, bottom, x, y);
                x += 70.0;
            }
            img(r, bottom, w - 70.0, y);
        }
        for y in [0.0, h - DIVIDER, h - BAR] {
            let mut x = 0.0;
            while x < w - 86.0 {
                img(r, horiz, x, y);
                x += 86.0;
            }
            img(r, horiz, w - 86.0, y);
        }
        for (x, end) in [(0.0, h), (w - BAR, h)] {
            let mut y = BAR;
            while y < end - 86.0 {
                img(r, vert, x, y);
                y += 86.0;
            }
            img(r, vert, x, end - 86.0);
            for cy in [0.0, h - DIVIDER] {
                img(r, cross, x, cy);
            }
            if end == h {
                img(r, cross, x, h - BAR);
            }
        }
    }

    fn draw_panel(&self, r: &mut Renderer, panels: &PanelImages, world: &World, text: &TextTable, images: &EmpireImages) {
        let screen = r.screen;
        let (sw, sh) = (screen[0], screen[1]);
        let raw = |i: usize| text.get(TEXT, i).unwrap_or("").to_owned();
        let t = |i: usize| raw(i).trim().to_owned();
        let label = |r: &mut Renderer, f: Font, s: &str, x: f32, y: f32| draw_text(r, f, s, x, y, font::BLACK) as f32;
        let centre = |r: &mut Renderer, f: Font, s: &str, y: f32| {
            let w = text_width(r, f, s) as f32;
            draw_text(r, f, s, ((sw - w) / 2.0).floor(), y, font::BLACK);
        };
        // Each button shows its next frame under the mouse.
        for (b, id) in [(Self::help_button(screen), images.context), (Self::close_button(screen), images.context + 4), (Self::advisor_button(screen), images.advisors + 4 * 3)] {
            img(r, id + Self::inside(b, self.cursor) as u32, b[0], b[1]);
        }

        let Some((i, c)) = self.selected.and_then(|i| world.trade.cities.get(i).map(|c| (i, c))) else {
            centre(r, Font::NormalBlackOnLight, &t(9), sh - 60.0);
            return;
        };
        let name = text.get(CITY_NAMES, c.name_id as usize).unwrap_or("?");
        centre(r, Font::LargeBlackOnLight, name, sh - 122.0);
        if !c.trades() {
            let what = match c.city_type {
                city::OURS => 1,
                city::PHARAOH => 19,
                city::EGYPTIAN => 13,
                _ => 0,
            };
            centre(r, Font::NormalBlackOnLight, &t(what), sh - 60.0);
            return;
        }
        let cx = Self::panel_x(screen);
        let route = world.trade.routes.get(c.route as usize);
        // A good shows only while the route allows some of it this year.
        let goods = |buys: bool| {
            let list = if buys { &c.buys } else { &c.sells };
            (1..list.len()).filter(|&r| list[r] && world.trade_limit(i, r as u16) > 0).collect::<Vec<_>>()
        };
        let tier_badge = |r: &mut Renderer, res: usize, x: f32, y: f32| {
            let loads = world.trade_limit(i, res as u16) / 100;
            let k = if loads > 25 { 2 } else if loads > 15 { 1 } else { 0 };
            img(r, images.tiers + k, x, y);
        };
        let bevel = |r: &mut Renderer, x: f32, y: f32| {
            let (w, h) = (19.0, 17.0);
            r.rect([x, y], [w, 1.0], BOX_DARK, Space::Screen);
            r.rect([x, y], [1.0, h], BOX_DARK, Space::Screen);
            r.rect([x + w - 1.0, y], [1.0, h], font::WHITE, Space::Screen);
            r.rect([x, y + h - 1.0], [w, 1.0], font::WHITE, Space::Screen);
        };
        if !c.open {
            // Not yet open: what the city would sell and buy, stacked (Akhenaten's
            // city_want_sell_title/items at centerX-220/-170, y sellItemsTop = sh-90;
            // city_want_buy_title/items at the same x, y buyItemsTop+20 = sh-70).
            label(r, Font::NormalBlackOnLight, &t(5), cx - 220.0, sh - 90.0);
            for (k, &res) in goods(false).iter().enumerate() {
                let x = cx - 170.0 + 32.0 * k as f32;
                bevel(r, x, sh - 95.0);
                img(r, images.icon(res as u16), x + 1.0, sh - 94.0);
                tier_badge(r, res, x + 13.0, sh - 94.0);
            }
            label(r, Font::NormalBlackOnLight, &t(4), cx - 220.0, sh - 70.0);
            for (k, &res) in goods(true).iter().enumerate() {
                let x = cx - 170.0 + 32.0 * k as f32;
                bevel(r, x, sh - 73.0);
                img(r, images.icon(res as u16), x + 1.0, sh - 73.0);
                tier_badge(r, res, x + 13.0, sh - 73.0);
            }
            let b = Self::open_button(screen);
            panel::button_border(r, panels, b[0], b[1], b[2] as i32, b[3] as i32, Self::inside(b, self.cursor));
            // "300 Deben to open land trade route": the cost after a blank sign slot,
            // then the unit, each followed by a blank.
            let unit = text.get(8, if c.cost == 1 { 0 } else { 1 }).unwrap_or("");
            let cost = c.cost.to_string();
            let x = cx - 220.0 + 10.0;
            let x = x + label(r, Font::NormalBlackOnLight, &cost, x, sh - 35.0) + 6.0;
            let x = x + label(r, Font::NormalBlackOnLight, unit, x, sh - 35.0) + 6.0;
            label(r, Font::NormalBlackOnLight, &raw(if c.sea { 7 } else { 6 }), x, sh - 35.0);
            return;
        }
        // The route is open: what has been sold and bought this year of what it allows,
        // as "N of M". Each number is drawn after a blank sign slot and followed by a
        // blank, 4 pixels each in this font. The original starts the first number 22
        // pixels in, under the tier badge (13 in, up to 13 wide); Osiris moves the text
        // 6 pixels right so the badge covers none of it.
        let amount = |r: &mut Renderer, res: usize, x: f32, y: f32| {
            let traded = route.map_or(0, |rt| rt.traded[res]);
            let limit = world.trade_limit(i, res as u16).max(traded);
            let shown = |n: i32| (if COUNTED.contains(&res) { n / 100 } else { n }).to_string();
            let (a, of, b) = (shown(traded), t(12), shown(limit));
            let wa = text_width(r, Font::SmallPlain, &a) as f32;
            let wof = text_width(r, Font::SmallPlain, &of) as f32;
            let x = x + AMOUNT_SHIFT;
            draw_text(r, Font::SmallPlain, &a, x + 22.0, y, font::BLACK);
            draw_text(r, Font::SmallPlain, &of, x + 24.0 + wa, y, font::BLACK);
            draw_text(r, Font::SmallPlain, &b, x + 26.0 + wa + wof, y, font::BLACK);
        };
        // The city's sales (what we buy from it) sit on the right, its purchases (what
        // we sell it) on the left: Akhenaten's city_sell_title/items (text group 47 id
        // 11, city.city_sells_resource) are at centerX+250/+100, and city_buy_title/
        // items (id 10, city.city_buys_resource) at centerX-300/-430; both titles sit
        // at infoTop = sh-121, both item blocks at sellItemsTop = buyItemsTop = sh-90.
        r.rect([cx, sh - 90.0], [1.0, 76.0], font::BLACK, Space::Screen);
        label(r, Font::NormalBlackOnLight, &t(10), cx - 300.0, sh - 121.0);
        for (k, &res) in goods(true).iter().enumerate() {
            let (x, y) = (cx - 430.0 + 135.0 * (k % 2) as f32, sh - 90.0 + 20.0 * (k / 2) as f32);
            bevel(r, x, y + 1.0);
            img(r, images.icon(res as u16), x + 1.0, y + 2.0);
            tier_badge(r, res, x + 13.0, y);
            amount(r, res, x, y + 8.0);
        }
        label(r, Font::NormalBlackOnLight, &t(11), cx + 250.0, sh - 121.0);
        for (k, &res) in goods(false).iter().enumerate() {
            let (x, y) = (cx + 100.0 + 135.0 * (k % 2) as f32, sh - 90.0 + 20.0 * (k / 2) as f32);
            bevel(r, x, y);
            img(r, images.icon(res as u16), x + 1.0, y + 1.0);
            tier_badge(r, res, x + 13.0, y);
            // A good the city both sells and buys has its amount in the Bought column
            // (the left-hand one, above) only.
            if !c.buys[res] {
                amount(r, res, x, y + 8.0);
            }
        }
    }
}

/// Normal black text wrapped `w` wide, three pixels above `y` like single lines.
fn wrapped(r: &mut Renderer, s: &str, x: f32, y: f32, w: f32) {
    let opts = rich_text::Options { font: Font::NormalBlackOnLight, width: w as i32, paragraph_indent: 0 };
    let laid = rich_text::layout(s, &opts, &mut rich_text::RendererMeasure::new(r));
    rich_text::draw(r, &laid, [x, y - 3.0], laid.height as f32, 0.0, font::BLACK);
}

/// The dots along a route: from each waypoint to the next, one every `step` pixels.
fn route_dots(route: &osiris_sim::trade::TradeRoute) -> Vec<(i32, i32)> {
    let step = if route.step == 0 { 5.0 } else { route.step as f32 };
    let mut dots = Vec::new();
    for pair in route.points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let (dx, dy) = ((b.0 - a.0) as f32, (b.1 - a.1) as f32);
        let len = (dx * dx + dy * dy).sqrt();
        if len == 0.0 {
            continue;
        }
        let mut d = 0.0;
        while d <= len {
            dots.push((a.0 + (dx * d / len) as i32, a.1 + (dy * d / len) as i32));
            d += step;
        }
    }
    dots
}
