//! The empire map: the empire's cities and trade routes, with the selected city's
//! trade in the panel along the bottom. Cities are opened for trade from here.
//!
//! The 1200x1600 map scrolls inside a frame of bars (Pharaoh_General group 172); open
//! routes are dotted (group 149) and cities show as ours, Egyptian or foreign (group
//! 169). As in the original game, no traders travel the routes here: they appear
//! straight in the city. Text comes from group 47 and city names from group 195.

use osiris_formats::{ImageLibrary, TextTable};
use osiris_render::{Renderer, Space};
use osiris_sim::World;
use osiris_ui::{Font, PanelImages, draw_text, font, panel, text_width};

fn img(r: &mut Renderer, id: u32, x: f32, y: f32) {
    r.image(id, [x, y], [1.0; 4], Space::Screen);
}

const MAP_W: f32 = 1200.0;
const MAP_H: f32 = 1600.0;
/// The frame's margins: the map sits 16 pixels in, above a 120-pixel panel.
const MARGIN: f32 = 16.0;
const PANEL_H: f32 = 120.0;
const TEXT: usize = 47;
const CITY_NAMES: usize = 195;
/// Tier badges for 1500, 2500 and 4000 units a year, and where they sit on an icon.
const TIER_BADGE_X: [f32; 3] = [21.0, 17.0, 13.0];

#[derive(Clone, Copy)]
pub struct EmpireImages {
    map: u32,
    bars: u32,
    route_dots: u32,
    cities: u32,
    tiers: u32,
    icons: u32,
}

impl EmpireImages {
    pub fn load(lib: &ImageLibrary) -> osiris_formats::Result<Self> {
        Ok(Self {
            map: lib.group_id("Empire", 1, 0)?,
            bars: lib.group_id("Pharaoh_General", 172, 0)?,
            route_dots: lib.group_id("Pharaoh_General", 149, 0)?,
            cities: lib.group_id("Pharaoh_General", 169, 0)?,
            tiers: lib.group_id("Pharaoh_General", 171, 0)?,
            icons: lib.group_id("Pharaoh_General", 129, 0)?,
        })
    }

    /// The small icon of resource `r`.
    pub fn icon(&self, r: u16) -> u32 {
        self.icons + r as u32
    }

    fn city(&self, city_type: u8) -> u32 {
        use osiris_formats::empire::city;
        self.cities
            + match city_type {
                city::OURS => 0,
                city::FOREIGN | city::FOREIGN_TRADING => 9,
                _ => 6,
            }
    }
}

/// What a click in the empire window did.
pub enum EmpireClick {
    Nothing,
    Close,
    /// Asked to open the selected city's trade route.
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
}

impl EmpireWindow {
    /// The map's rectangle on screen, centred when the screen is wider than the map.
    fn view(screen: [f32; 2]) -> [f32; 4] {
        let w = (screen[0] - 2.0 * MARGIN).min(MAP_W);
        let h = (screen[1] - PANEL_H - 2.0 * MARGIN).min(MAP_H);
        [((screen[0] - w) / 2.0).floor(), MARGIN, w, h]
    }

    fn clamp(&mut self, screen: [f32; 2]) {
        let v = Self::view(screen);
        self.scroll[0] = self.scroll[0].clamp(0.0, (MAP_W - v[2]).max(0.0));
        self.scroll[1] = self.scroll[1].clamp(0.0, (MAP_H - v[3]).max(0.0));
    }

    /// Centres the view on our city the first time it is shown.
    fn centre_once(&mut self, world: &World, screen: [f32; 2]) {
        if self.centred {
            return;
        }
        self.centred = true;
        let v = Self::view(screen);
        if let Some(c) = world.trade.cities.iter().find(|c| c.city_type == osiris_formats::empire::city::OURS) {
            self.scroll = [c.pos.0 as f32 - v[2] / 2.0, c.pos.1 as f32 - v[3] / 2.0];
        }
        self.clamp(screen);
    }

    fn to_screen(&self, screen: [f32; 2], p: (i32, i32)) -> [f32; 2] {
        let v = Self::view(screen);
        [v[0] + p.0 as f32 - self.scroll[0], v[1] + p.1 as f32 - self.scroll[1]]
    }

    pub fn select(&mut self, city: Option<usize>) {
        self.selected = city;
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

    fn open_button(screen: [f32; 2]) -> [f32; 4] {
        [screen[0] / 2.0 - 220.0, screen[1] - MARGIN - 40.0, 440.0, 24.0]
    }

    fn close_button(screen: [f32; 2]) -> [f32; 4] {
        [screen[0] - MARGIN - 110.0, screen[1] - MARGIN - 40.0, 96.0, 24.0]
    }

    fn inside(r: [f32; 4], p: [f32; 2]) -> bool {
        p[0] >= r[0] && p[1] >= r[1] && p[0] < r[0] + r[2] && p[1] < r[1] + r[3]
    }

    /// The city under `p`, if any.
    fn city_at(&self, r: &Renderer, world: &World, images: &EmpireImages, p: [f32; 2]) -> Option<usize> {
        world.trade.cities.iter().position(|c| {
            let at = self.to_screen(r.screen, c.pos);
            let (w, h) = r.record(images.city(c.city_type)).map_or((37.0, 34.0), |rec| (rec.width as f32, rec.height as f32));
            p[0] >= at[0] && p[1] >= at[1] && p[0] < at[0] + w && p[1] < at[1] + h
        })
    }

    pub fn click(&mut self, r: &Renderer, world: &World, images: &EmpireImages, p: [f32; 2]) -> EmpireClick {
        let screen = r.screen;
        if Self::inside(Self::close_button(screen), p) {
            return EmpireClick::Close;
        }
        if let Some(c) = self.selected
            && Self::inside(Self::open_button(screen), p)
            && world.trade.cities.get(c).is_some_and(|c| c.trades() && !c.open)
        {
            return EmpireClick::OpenRoute(c);
        }
        if Self::inside(Self::view(screen), p) {
            match self.city_at(r, world, images, p) {
                Some(c) => self.selected = Some(c),
                None => self.drag = Some((p, self.scroll)),
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
        img(r, images.map, v[0] - self.scroll[0], v[1] - self.scroll[1]);
        // Routes: open ones always, the selected city's also when closed.
        for (i, c) in world.trade.cities.iter().enumerate() {
            let selected = self.selected == Some(i);
            if !c.trades() || !(c.open || selected) {
                continue;
            }
            let dot = images.route_dots + match (c.open, selected) {
                (true, false) => 201,
                (true, true) => 186,
                _ => 211,
            };
            if let Some(route) = world.trade.routes.get(c.route as usize) {
                for &pt in &route.points {
                    let at = self.to_screen(screen, pt);
                    img(r, dot, at[0], at[1]);
                }
            }
        }
        for c in &world.trade.cities {
            let at = self.to_screen(screen, c.pos);
            img(r, images.city(c.city_type), at[0], at[1]);
        }
        r.set_clip(None);
        self.draw_frame(r, images, v);
        self.draw_panel(r, panels, world, text, images);
    }

    /// Bars around the map and the stone panel below it.
    fn draw_frame(&self, r: &mut Renderer, images: &EmpireImages, v: [f32; 4]) {
        let screen = r.screen;
        let (vert, horiz, cross, bottom) = (images.bars, images.bars + 1, images.bars + 2, images.bars + 3);
        let mut y = v[1] + v[3];
        while y < screen[1] {
            let mut x = 0.0;
            while x < screen[0] {
                img(r, bottom, x, y);
                x += 70.0;
            }
            y += 40.0;
        }
        let (left, right) = (v[0] - MARGIN, v[0] + v[2]);
        let mut x = left;
        while x < right {
            img(r, horiz, x, 0.0);
            img(r, horiz, x, v[1] + v[3]);
            x += 86.0;
        }
        let mut y = 0.0;
        while y < v[1] + v[3] {
            img(r, vert, left, y);
            img(r, vert, right, y);
            y += 86.0;
        }
        for (cx, cy) in [(left, 0.0), (right, 0.0), (left, v[1] + v[3]), (right, v[1] + v[3])] {
            img(r, cross, cx, cy);
        }
    }

    fn draw_panel(&self, r: &mut Renderer, panels: &PanelImages, world: &World, text: &TextTable, images: &EmpireImages) {
        let screen = r.screen;
        let raw = |i: usize| text.get(TEXT, i).unwrap_or("").to_owned();
        let t = |i: usize| raw(i).trim().to_owned();
        let centre = |r: &mut Renderer, f: Font, s: &str, y: f32| {
            let w = text_width(r, f, s) as f32;
            draw_text(r, f, s, ((screen[0] - w) / 2.0).floor(), y, font::BLACK);
        };
        let top = screen[1] - PANEL_H + 4.0;
        let close = Self::close_button(screen);
        panel::label(r, panels, close[0], close[1], (close[2] / 16.0) as i32, if Self::inside(close, self.cursor) { 1 } else { 2 });
        draw_text(r, Font::NormalBlackOnLight, "Close", close[0] + 30.0, close[1] + 3.0, font::BLACK);
        let Some(c) = self.selected.and_then(|i| world.trade.cities.get(i)) else {
            centre(r, Font::NormalBlackOnLight, &t(9), top + 30.0);
            return;
        };
        let name = text.get(CITY_NAMES, c.name_id as usize).unwrap_or("?");
        centre(r, Font::LargeBlackOnLight, name, top);
        use osiris_formats::empire::city;
        if c.city_type == city::OURS {
            centre(r, Font::NormalBlackOnLight, &t(1), top + 40.0);
            return;
        }
        if !c.trades() {
            let what = match c.city_type {
                city::PHARAOH => 19,
                city::EGYPTIAN => 13,
                _ => 0,
            };
            centre(r, Font::NormalBlackOnLight, &t(what), top + 40.0);
            return;
        }
        let route = &world.trade.routes[c.route as usize];
        let x0 = Self::view(screen)[0] + 60.0;
        let mut y = top + 28.0;
        for (label, list) in [(if c.open { 11 } else { 5 }, &c.sells), (if c.open { 10 } else { 4 }, &c.buys)] {
            draw_text(r, Font::NormalBlackOnLight, &t(label), x0, y + 4.0, font::BLACK);
            let mut x = x0 + 90.0;
            for res in (1..list.len()).filter(|&i| list[i]) {
                let limit = route.limit[res];
                img(r, images.icon(res as u16), x, y + 2.0);
                let tier = match limit {
                    1500 => Some(0),
                    2500 => Some(1),
                    4000 => Some(2),
                    _ => None,
                };
                if let Some(k) = tier {
                    img(r, images.tiers + k as u32, x + TIER_BADGE_X[k], y + 1.0);
                }
                let amount = if c.open { format!("{} {} {}", route.traded[res].min(limit) / 100, t(12), limit / 100) } else { format!("{}", limit / 100) };
                draw_text(r, Font::SmallPlain, &amount, x + 34.0, y + 6.0, font::BLACK);
                x += if c.open { 110.0 } else { 70.0 };
            }
            y += 24.0;
        }
        if !c.open {
            let b = Self::open_button(screen);
            panel::label(r, panels, b[0], b[1], (b[2] / 16.0) as i32, if Self::inside(b, self.cursor) { 1 } else { 2 });
            let label = format!("{} Db{}", c.cost, raw(if c.sea { 7 } else { 6 }));
            let w = text_width(r, Font::NormalBlackOnLight, &label) as f32;
            draw_text(r, Font::NormalBlackOnLight, &label, b[0] + (b[2] - w) / 2.0, b[1] + 3.0, font::BLACK);
        }
    }
}
