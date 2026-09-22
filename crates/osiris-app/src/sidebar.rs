//! The right-hand control panel: build category buttons and their submenus, drawn
//! with the original sidebar art.

use osiris_formats::ImageLibrary;
use osiris_render::{Renderer, Space, WHITE};
use osiris_ui::{Font, PanelImages, draw_text, font, panel, text_width};

pub const WIDTH: f32 = 186.0;
pub const TOP: f32 = 30.0;
const PANEL_W: f32 = 162.0;

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

/// Button positions (x from the sidebar panel's left edge, y from screen top) and
/// image offsets within group 136.
const BUTTONS: [(Category, f32, f32, u32); 12] = [
    (Category::Housing, 9.0, 281.0, 0),
    (Category::Roads, 9.0, 330.0, 4),
    (Category::Clear, 9.0, 381.0, 8),
    (Category::Food, 46.0, 281.0, 12),
    (Category::Industry, 46.0, 330.0, 16),
    (Category::Distribution, 46.0, 381.0, 20),
    (Category::Entertainment, 86.0, 281.0, 24),
    (Category::Religion, 86.0, 330.0, 28),
    (Category::Education, 86.0, 381.0, 32),
    (Category::Health, 125.0, 281.0, 36),
    (Category::Government, 125.0, 330.0, 40),
    (Category::Security, 125.0, 381.0, 44),
];

#[derive(Clone)]
pub struct SidebarImages {
    top_bar: u32,
    panel: u32,
    strip: u32,
    relief: u32,
    buttons: u32,
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

#[derive(Default)]
pub struct Sidebar {
    pub open: Option<Category>,
    pub hover_button: Option<Category>,
    pub hover_item: Option<usize>,
    pub items: Vec<MenuItem>,
}

pub enum Click {
    Button(Category),
    Item(usize),
    /// Inside the sidebar, but on nothing in particular.
    Absorbed,
    /// Not on the sidebar at all.
    Outside,
}

impl Sidebar {
    fn origin(screen_w: f32) -> f32 {
        screen_w - PANEL_W
    }

    fn button_at(&self, screen_w: f32, p: [f32; 2]) -> Option<Category> {
        let ox = Self::origin(screen_w);
        BUTTONS.iter().find_map(|&(c, bx, by, _)| {
            let (x, y) = (ox + bx, by);
            (p[0] >= x && p[0] < x + 36.0 && p[1] >= y && p[1] < y + 48.0).then_some(c)
        })
    }

    fn menu_rect(&self, screen_w: f32) -> (f32, f32, f32, f32) {
        let w = 260.0;
        let h = 24.0 * self.items.len() as f32 + 16.0;
        (Self::origin(screen_w) - WIDTH + 20.0 - w, 280.0, w, h)
    }

    fn item_at(&self, screen_w: f32, p: [f32; 2]) -> Option<usize> {
        self.open?;
        let (x, y, w, _) = self.menu_rect(screen_w);
        (0..self.items.len()).find(|&i| {
            let iy = y + 8.0 + 24.0 * i as f32;
            p[0] >= x && p[0] < x + w && p[1] >= iy && p[1] < iy + 22.0
        })
    }

    pub fn contains(&self, screen_w: f32, p: [f32; 2]) -> bool {
        p[0] >= screen_w - WIDTH || p[1] < TOP || self.item_at(screen_w, p).is_some()
    }

    pub fn hover(&mut self, screen_w: f32, p: [f32; 2]) {
        self.hover_button = self.button_at(screen_w, p);
        self.hover_item = self.item_at(screen_w, p);
    }

    pub fn click(&mut self, screen_w: f32, p: [f32; 2]) -> Click {
        if let Some(i) = self.item_at(screen_w, p) {
            return Click::Item(i);
        }
        if let Some(c) = self.button_at(screen_w, p) {
            return Click::Button(c);
        }
        if self.contains(screen_w, p) { Click::Absorbed } else { Click::Outside }
    }

    pub fn draw(&self, r: &mut Renderer, img: &SidebarImages, status: &str, title: &str) {
        let [w, h] = r.screen;
        // Top bar, tiled across the screen.
        let mut x = 0.0;
        while x < w {
            r.image(img.top_bar, [x, 0.0], WHITE, Space::Screen);
            x += 1000.0;
        }
        draw_text(r, Font::NormalWhiteOnDark, status, 10.0, 8.0, font::WHITE);
        let tw = text_width(r, Font::NormalWhiteOnDark, title) as f32;
        draw_text(r, Font::NormalWhiteOnDark, title, w - WIDTH - tw - 10.0, 8.0, font::WHITE);

        let ox = Self::origin(w);
        // The panel art has windows (minimap, messages); back them with black.
        r.rect([ox - 24.0, TOP], [WIDTH, h - TOP], [0.0, 0.0, 0.0, 1.0], Space::Screen);
        r.image(img.strip, [ox - 24.0, TOP], WHITE, Space::Screen);
        r.image(img.panel, [ox, TOP], WHITE, Space::Screen);
        let mut y = TOP + 450.0;
        while y < h {
            r.image(img.relief, [ox, y], WHITE, Space::Screen);
            y += 285.0;
        }
        for &(c, bx, by, off) in &BUTTONS {
            let state = if self.open == Some(c) {
                2
            } else if self.hover_button == Some(c) {
                1
            } else {
                0
            };
            r.image(img.buttons + off + state, [ox + bx, by], WHITE, Space::Screen);
        }
        if self.open.is_some() && !self.items.is_empty() {
            self.draw_menu(r, img, w);
        }
    }

    fn draw_menu(&self, r: &mut Renderer, img: &SidebarImages, screen_w: f32) {
        let (x, y, w, _) = self.menu_rect(screen_w);
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
