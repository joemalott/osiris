//! The right-hand control panel: build category buttons and their submenus, the
//! message and mission buttons, and a status block, drawn with the original sidebar art.
//!
//! The panel art (`Pharaoh_General` group 121) is 162 pixels wide with transparent
//! windows for the minimap, the buttons and the picture of the current build category;
//! a 24-pixel decorative strip runs down the right edge of the screen beside it.

use osiris_formats::ImageLibrary;
use osiris_render::{Renderer, Space, WHITE};
use osiris_ui::{Font, PanelImages, draw_text, font, panel, text_width};

/// Panel plus the strip beside it.
pub const WIDTH: f32 = 186.0;
pub const TOP: f32 = 30.0;
const STRIP_W: f32 = 24.0;
const PANEL_H: f32 = 450.0;

/// Left edge of the panel on screen.
pub fn panel_left(screen_w: f32) -> f32 {
    screen_w - WIDTH
}

/// The twelve build categories, in the original's button order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Housing,
    Roads,
    Clear,
    Food,
    Industry,
    Distribution,
    Entertainment,
    Religion,
    Education,
    Health,
    Government,
    Security,
}

impl Category {
    /// Offset in group 117 of the picture shown for this category.
    fn picture(self) -> u32 {
        match self {
            Category::Housing => 1,
            Category::Roads => 5,
            Category::Clear => 9,
            Category::Food => 10,
            Category::Industry => 6,
            Category::Distribution => 2,
            Category::Entertainment => 3,
            Category::Religion => 7,
            Category::Education => 11,
            Category::Health => 4,
            Category::Government => 8,
            Category::Security => 12,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Build(Category),
    Undo,
    Messages,
    Problem,
    Briefing,
    Advisors,
    Empire,
    Collapse,
    SpeedDown,
    SpeedUp,
}

/// Where an image button sits (from the panel's left edge and the screen top), its
/// size, which image group it uses, and its first image offset. Buttons have four
/// frames: normal, hover, pressed, disabled.
struct Layout {
    button: Button,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    group: u8,
    offset: u32,
    /// Tooltip in text group 68.
    tip: usize,
}

const G136: u8 = 0;
const G110: u8 = 1;

#[allow(clippy::too_many_arguments)]
const fn at(button: Button, x: f32, y: f32, w: f32, h: f32, group: u8, offset: u32, tip: usize) -> Layout {
    Layout { button, x, y, w, h, group, offset, tip }
}

const LAYOUT: [Layout; 19] = [
    at(Button::Collapse, 128.0, 30.0, 33.0, 20.0, G110, 7, 10),
    at(Button::Advisors, 16.0, 173.0, 60.0, 36.0, G136, 64, 41),
    at(Button::Empire, 90.0, 173.0, 62.0, 36.0, G136, 68, 42),
    at(Button::Build(Category::Housing), 9.0, 281.0, 34.0, 48.0, G136, 0, 20),
    at(Button::Build(Category::Roads), 9.0, 330.0, 34.0, 50.0, G136, 4, 21),
    at(Button::Build(Category::Clear), 9.0, 381.0, 34.0, 49.0, G136, 8, 22),
    at(Button::Build(Category::Food), 46.0, 281.0, 36.0, 48.0, G136, 12, 23),
    at(Button::Build(Category::Industry), 46.0, 330.0, 36.0, 50.0, G136, 16, 24),
    at(Button::Build(Category::Distribution), 46.0, 381.0, 36.0, 49.0, G136, 20, 25),
    at(Button::Build(Category::Entertainment), 86.0, 281.0, 34.0, 48.0, G136, 24, 26),
    at(Button::Build(Category::Religion), 86.0, 330.0, 34.0, 50.0, G136, 28, 27),
    at(Button::Build(Category::Education), 86.0, 381.0, 34.0, 49.0, G136, 32, 28),
    at(Button::Build(Category::Health), 125.0, 281.0, 34.0, 48.0, G136, 36, 29),
    at(Button::Build(Category::Government), 125.0, 330.0, 34.0, 50.0, G136, 40, 30),
    at(Button::Build(Category::Security), 125.0, 381.0, 34.0, 49.0, G136, 44, 31),
    at(Button::Undo, 9.0, 434.0, 35.0, 45.0, G136, 48, 32),
    at(Button::Messages, 46.0, 434.0, 38.0, 45.0, G136, 52, 33),
    at(Button::Problem, 86.0, 434.0, 28.0, 45.0, G136, 56, 34),
    at(Button::Briefing, 116.0, 434.0, 43.0, 45.0, G136, 60, 35),
];

/// The status block under the panel.
const STATUS_Y: f32 = TOP + PANEL_H + 4.0;
const STATUS_BLOCKS_H: i32 = 8;
const ARROW: f32 = 24.0;
const SPEED_DOWN: (f32, f32) = (11.0, STATUS_Y + 22.0);
const SPEED_UP: (f32, f32) = (35.0, STATUS_Y + 22.0);

#[derive(Clone)]
pub struct SidebarImages {
    top_bar: u32,
    panel: u32,
    strip: u32,
    relief: u32,
    buttons: u32,
    collapse: u32,
    pictures: u32,
    arrow_up: u32,
    arrow_down: u32,
    pub panels: PanelImages,
}

impl SidebarImages {
    pub fn load(lib: &ImageLibrary) -> osiris_formats::Result<Self> {
        let g = |group, off| lib.group_id("Pharaoh_General", group, off);
        Ok(Self {
            top_bar: g(121, 8)?,
            panel: g(121, 0)?,
            strip: g(121, 2)?,
            relief: g(121, 4)?,
            buttons: g(136, 0)?,
            collapse: g(110, 0)?,
            pictures: g(117, 0)?,
            arrow_up: lib.group_id("Pharaoh_Unloaded", 0, 16)?,
            arrow_down: lib.group_id("Pharaoh_Unloaded", 0, 18)?,
            panels: PanelImages::load(lib)?,
        })
    }
}

/// One entry of an open build submenu.
#[derive(Debug, Clone)]
pub struct MenuItem {
    pub label: String,
    pub cost: i32,
    pub enabled: bool,
}

/// Everything the sidebar shows that comes from the game.
pub struct SidebarState<'a> {
    /// Build categories with nothing to build yet.
    pub empty: &'a [Category],
    /// Picture for the tool in hand, or `None` for the default.
    pub category: Option<Category>,
    pub unread: usize,
    pub has_messages: bool,
    pub has_problems: bool,
    pub has_briefing: bool,
    pub can_undo: bool,
    pub speed: &'a str,
    pub lines: &'a [(String, String)],
    pub tips: &'a dyn Fn(usize) -> Option<String>,
}

#[derive(Default)]
pub struct Sidebar {
    pub open: Option<Category>,
    pub hover_button: Option<Button>,
    pub hover_item: Option<usize>,
    pub items: Vec<MenuItem>,
    pressed: Option<Button>,
    cursor: [f32; 2],
}

pub enum Click {
    Button(Button),
    Item(usize),
    /// Inside the sidebar, but on nothing in particular.
    Absorbed,
    /// Not on the sidebar at all.
    Outside,
}

impl Sidebar {
    fn button_at(&self, screen_w: f32, p: [f32; 2]) -> Option<Button> {
        let ox = panel_left(screen_w);
        let inside = |x: f32, y: f32, w: f32, h: f32| p[0] >= ox + x && p[0] < ox + x + w && p[1] >= y && p[1] < y + h;
        if inside(SPEED_DOWN.0, SPEED_DOWN.1, ARROW, ARROW) {
            return Some(Button::SpeedDown);
        }
        if inside(SPEED_UP.0, SPEED_UP.1, ARROW, ARROW) {
            return Some(Button::SpeedUp);
        }
        LAYOUT.iter().find(|l| inside(l.x, l.y, l.w, l.h)).map(|l| l.button)
    }

    fn menu_rect(&self, screen: [f32; 2]) -> (f32, f32, f32, f32) {
        let w = 260.0;
        let h = 24.0 * self.items.len() as f32 + 16.0;
        let y = (281.0f32).min(screen[1] - h - 8.0).max(TOP + 8.0);
        (panel_left(screen[0]) - w - 6.0, y, w, h)
    }

    fn item_at(&self, screen: [f32; 2], p: [f32; 2]) -> Option<usize> {
        self.open?;
        let (x, y, w, _) = self.menu_rect(screen);
        (0..self.items.len()).find(|&i| {
            let iy = y + 8.0 + 24.0 * i as f32;
            p[0] >= x && p[0] < x + w && p[1] >= iy && p[1] < iy + 22.0
        })
    }

    pub fn contains(&self, screen: [f32; 2], p: [f32; 2]) -> bool {
        p[0] >= panel_left(screen[0]) || p[1] < TOP || self.item_at(screen, p).is_some()
    }

    pub fn hover(&mut self, screen: [f32; 2], p: [f32; 2]) {
        self.cursor = p;
        self.hover_button = self.button_at(screen[0], p);
        self.hover_item = self.item_at(screen, p);
    }

    pub fn click(&mut self, screen: [f32; 2], p: [f32; 2]) -> Click {
        if let Some(i) = self.item_at(screen, p) {
            return Click::Item(i);
        }
        if let Some(b) = self.button_at(screen[0], p) {
            self.pressed = Some(b);
            return Click::Button(b);
        }
        if self.contains(screen, p) { Click::Absorbed } else { Click::Outside }
    }

    pub fn release(&mut self) {
        self.pressed = None;
    }

    fn enabled(b: Button, s: &SidebarState) -> bool {
        match b {
            Button::Build(c) => !s.empty.contains(&c),
            Button::Undo => s.can_undo,
            Button::Messages => s.has_messages,
            Button::Problem => s.has_problems,
            Button::Briefing => s.has_briefing,
            Button::Advisors | Button::Empire | Button::Collapse => false,
            Button::SpeedDown | Button::SpeedUp => true,
        }
    }

    pub fn draw(&self, r: &mut Renderer, img: &SidebarImages, s: &SidebarState) {
        let [w, h] = r.screen;
        // Top bar, tiled across the screen.
        let mut x = 0.0;
        while x < w {
            r.image(img.top_bar, [x, 0.0], WHITE, Space::Screen);
            x += 1000.0;
        }

        let ox = panel_left(w);
        // Behind the panel's windows.
        r.rect([ox, TOP], [WIDTH, h - TOP], [0.0, 0.0, 0.0, 1.0], Space::Screen);
        let mut y = TOP;
        while y < h {
            r.image(img.strip, [w - STRIP_W, y], WHITE, Space::Screen);
            y += 768.0;
        }
        let mut y = TOP + PANEL_H;
        while y < h {
            r.image(img.relief, [ox, y], WHITE, Space::Screen);
            y += 285.0;
        }
        r.image(img.panel, [ox, TOP], WHITE, Space::Screen);
        let picture = s.category.map_or(1, Category::picture);
        r.image(img.pictures + picture, [ox + 11.0, 211.0], WHITE, Space::Screen);

        for l in &LAYOUT {
            let enabled = Self::enabled(l.button, s);
            let state = if !enabled {
                3
            } else if self.pressed == Some(l.button) || matches!(l.button, Button::Build(c) if self.open == Some(c)) {
                2
            } else if self.hover_button == Some(l.button) {
                1
            } else {
                0
            };
            let base = if l.group == G110 { img.collapse } else { img.buttons };
            // The collapse button has no disabled frame.
            let state = if l.group == G110 { 0 } else { state };
            r.image(base + l.offset + state, [ox + l.x, l.y], WHITE, Space::Screen);
        }
        if s.unread > 0 {
            draw_text(r, Font::NormalBlackOnDark, &s.unread.to_string(), ox + 52.0, 450.0, font::WHITE);
        }
        self.draw_status(r, img, s, ox);
        if self.open.is_some() && !self.items.is_empty() {
            self.draw_menu(r, img);
        }
        self.draw_tooltip(r, s);
    }

    fn draw_status(&self, r: &mut Renderer, img: &SidebarImages, s: &SidebarState, ox: f32) {
        panel::inner_panel(r, &img.panels, ox + 1.0, STATUS_Y, 10, STATUS_BLOCKS_H);
        let f = Font::NormalWhiteOnDark;
        draw_text(r, f, "Speed", ox + 11.0, STATUS_Y + 6.0, font::WHITE);
        for (b, (bx, by), image) in [
            (Button::SpeedDown, SPEED_DOWN, img.arrow_down),
            (Button::SpeedUp, SPEED_UP, img.arrow_up),
        ] {
            let pressed = self.pressed == Some(b);
            r.image(image + pressed as u32, [ox + bx, by], WHITE, Space::Screen);
        }
        draw_text(r, f, s.speed, ox + 67.0, STATUS_Y + 28.0, font::WHITE);
        let mut y = STATUS_Y + 54.0;
        for (label, value) in s.lines {
            draw_text(r, f, label, ox + 11.0, y, font::WHITE);
            let vw = text_width(r, f, value) as f32;
            draw_text(r, f, value, ox + 150.0 - vw, y, font::WHITE);
            y += 18.0;
        }
    }

    fn draw_tooltip(&self, r: &mut Renderer, s: &SidebarState) {
        let Some(b) = self.hover_button else { return };
        let Some(tip) = LAYOUT.iter().find(|l| l.button == b).and_then(|l| (s.tips)(l.tip)) else { return };
        let f = Font::SmallPlain;
        let tw = text_width(r, f, &tip) as f32;
        let (bw, bh) = (tw + 10.0, 20.0);
        let x = (self.cursor[0] - bw - 4.0).max(4.0);
        let y = self.cursor[1] + 18.0;
        r.rect([x, y], [bw, bh], [0.0, 0.0, 0.0, 1.0], Space::Screen);
        r.rect([x + 1.0, y + 1.0], [bw - 2.0, bh - 2.0], [1.0, 1.0, 0.94, 1.0], Space::Screen);
        draw_text(r, f, &tip, x + 5.0, y + 5.0, font::BLACK);
    }

    fn draw_menu(&self, r: &mut Renderer, img: &SidebarImages) {
        let (x, y, w, _) = self.menu_rect(r.screen);
        for (i, item) in self.items.iter().enumerate() {
            let iy = y + 8.0 + 24.0 * i as f32;
            let focus = self.hover_item == Some(i) && item.enabled;
            panel::label(r, &img.panels, x, iy, (w / 16.0) as i32, focus as u32);
            let f = if !item.enabled {
                Font::NormalBlackOnDark
            } else if focus {
                Font::NormalYellow
            } else {
                Font::NormalWhiteOnDark
            };
            draw_text(r, f, &item.label, x + 10.0, iy + 4.0, font::WHITE);
            if item.cost > 0 {
                let c = format!("{} Db", item.cost);
                let cw = text_width(r, f, &c) as f32;
                draw_text(r, f, &c, x + w - cw - 12.0, iy + 4.0, font::WHITE);
            }
        }
    }
}
