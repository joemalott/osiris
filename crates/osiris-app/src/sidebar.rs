//! The right-hand control panel: build category buttons and their submenus, the
//! message and mission buttons, and a status block, drawn with the original sidebar art.
//!
//! The panel art (`Pharaoh_General` group 121) is 162 pixels wide with transparent
//! windows for the minimap, the buttons and the picture of the current build category.
//! A 24-pixel patterned strip (121/2, 24x768) runs down the right edge of the screen
//! from the very top, beside the end of the menu bar (1000 pixels, 121/8). Below the
//! panel's 450 rows the original draws one carved relief (121/4, 162x285) at y 480,
//! which fills a 768-high screen, the tallest it supports.
//!
//! The eye button collapses the panel to a 42-pixel column (121/1, relief 121/5) of
//! build buttons (group 108) with an eye to expand it again (110/10), and the city
//! view gains 120 pixels. The full panel slides off over the column, or back over it,
//! in 47 steps. As in the original the state is a global that lasts for the session,
//! so it carries over into the next city played.

use std::sync::atomic::{AtomicBool, Ordering};

use osiris_formats::ImageLibrary;
use osiris_render::{Renderer, Space, WHITE};
use osiris_ui::{Font, PanelImages, draw_text, draw_text_tinted, font, panel, text_width};

/// The full panel plus the strip beside it.
pub const WIDTH: f32 = 186.0;
/// The collapsed column plus the strip.
pub const COLLAPSED_WIDTH: f32 = 66.0;
pub const TOP: f32 = 30.0;
const STRIP_W: f32 = 24.0;
const STRIP_H: f32 = 768.0;
/// Each copy of the strip below the first starts this far up, under the copy above,
/// so its plain top end (the part that sits beside the menu bar) stays hidden.
const STRIP_OVERLAP: f32 = 32.0;
const PANEL_W: f32 = 162.0;
const COLUMN_W: f32 = 42.0;
const PANEL_H: f32 = 450.0;
const RELIEF_Y: f32 = TOP + PANEL_H;
const RELIEF_H: f32 = 285.0;
/// Steps of the slide between the full panel and the column, about one per frame
/// at 60 frames a second.
const SLIDE_STEPS: f32 = 47.0;

static COLLAPSED: AtomicBool = AtomicBool::new(false);

/// Whether the panel is collapsed to its narrow column.
pub fn collapsed() -> bool {
    COLLAPSED.load(Ordering::Relaxed)
}

pub fn set_collapsed(c: bool) {
    COLLAPSED.store(c, Ordering::Relaxed);
}

/// Screen width the panel takes from the right edge; the city view gets the rest.
pub fn width() -> f32 {
    if collapsed() { COLLAPSED_WIDTH } else { WIDTH }
}

/// Left edge on screen of the panel as it stands, full or collapsed.
pub fn panel_left(screen_w: f32) -> f32 {
    screen_w - width()
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
    pub const ALL: [Category; 12] = [
        Category::Housing,
        Category::Roads,
        Category::Clear,
        Category::Food,
        Category::Industry,
        Category::Distribution,
        Category::Entertainment,
        Category::Religion,
        Category::Education,
        Category::Health,
        Category::Government,
        Category::Security,
    ];

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
    /// The eye: collapses the full panel, or expands the column.
    Collapse,
    SpeedDown,
    SpeedUp,
}

/// Where an image button sits (from the panel's left edge and the screen top), its
/// size, which image group it uses, and its first image offset. Buttons have four
/// frames: normal, hover, pressed, disabled; the eyes have the first three.
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
const G108: u8 = 2;

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

/// The collapsed column, from its left edge: the eye to expand it (tooltip 68/12)
/// and the twelve build buttons stacked down it.
const COLLAPSED_LAYOUT: [Layout; 13] = [
    at(Button::Collapse, 8.0, 30.0, 33.0, 20.0, G110, 10, 12),
    at(Button::Build(Category::Housing), 9.0, 51.0, 27.0, 35.0, G108, 0, 20),
    at(Button::Build(Category::Roads), 9.0, 87.0, 27.0, 34.0, G108, 4, 21),
    at(Button::Build(Category::Clear), 9.0, 122.0, 27.0, 36.0, G108, 8, 22),
    at(Button::Build(Category::Food), 9.0, 159.0, 27.0, 33.0, G108, 12, 23),
    at(Button::Build(Category::Industry), 9.0, 193.0, 27.0, 34.0, G108, 16, 24),
    at(Button::Build(Category::Distribution), 9.0, 228.0, 27.0, 34.0, G108, 20, 25),
    at(Button::Build(Category::Entertainment), 9.0, 263.0, 27.0, 32.0, G108, 24, 26),
    at(Button::Build(Category::Religion), 9.0, 296.0, 27.0, 35.0, G108, 28, 27),
    at(Button::Build(Category::Education), 9.0, 332.0, 27.0, 34.0, G108, 32, 28),
    at(Button::Build(Category::Health), 9.0, 368.0, 27.0, 35.0, G108, 36, 29),
    at(Button::Build(Category::Government), 9.0, 404.0, 27.0, 31.0, G108, 40, 30),
    at(Button::Build(Category::Security), 9.0, 436.0, 27.0, 39.0, G108, 44, 31),
];

fn layout(collapsed: bool) -> &'static [Layout] {
    if collapsed { &COLLAPSED_LAYOUT } else { &LAYOUT }
}

/// The status block over the top of the relief, inside the relief's carved frame
/// (its patterned left border is 7 pixels wide, the frame's right edge 3).
const STATUS_X: f32 = 11.0;
const STATUS_Y: f32 = RELIEF_Y + 4.0;
const STATUS_BLOCKS_W: i32 = 9;
const STATUS_BLOCKS_H: i32 = 8;
const STATUS_TEXT_X: f32 = STATUS_X + 10.0;
const STATUS_RIGHT: f32 = STATUS_X + 16.0 * STATUS_BLOCKS_W as f32 - 10.0;
const ARROW: f32 = 24.0;
const SPEED_DOWN: (f32, f32) = (STATUS_TEXT_X, STATUS_Y + 22.0);
const SPEED_UP: (f32, f32) = (STATUS_TEXT_X + 24.0, STATUS_Y + 22.0);

/// How much of the top bar image carries its patterned border, from the left.
const TOP_BAR_BORDERED: f32 = 845.0;

#[derive(Clone)]
pub struct SidebarImages {
    top_bar: u32,
    panel: u32,
    column: u32,
    strip: u32,
    relief: u32,
    column_relief: u32,
    buttons: u32,
    column_buttons: u32,
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
            column: g(121, 1)?,
            strip: g(121, 2)?,
            relief: g(121, 4)?,
            column_relief: g(121, 5)?,
            buttons: g(136, 0)?,
            column_buttons: g(108, 0)?,
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
    /// Whether the kingdom has cities to show.
    pub has_empire: bool,
    pub can_undo: bool,
    pub speed: &'a str,
    pub lines: &'a [(String, String)],
    pub tips: &'a dyn Fn(usize) -> Option<String>,
}

/// The full panel sliding off the column (collapsing) or back over it.
#[derive(Debug, Clone, Copy)]
struct Slide {
    collapsing: bool,
    step: f32,
}

impl Slide {
    /// How far right of its place the full panel is drawn: it speeds up as it
    /// leaves and slows as it arrives.
    fn offset(self) -> f32 {
        let t = (self.step / SLIDE_STEPS).clamp(0.0, 1.0);
        let t = if self.collapsing { t } else { 1.0 - t };
        (WIDTH * t * t).round()
    }
}

#[derive(Default)]
pub struct Sidebar {
    pub open: Option<Category>,
    pub hover_button: Option<Button>,
    pub hover_item: Option<usize>,
    pub items: Vec<MenuItem>,
    pressed: Option<Button>,
    cursor: [f32; 2],
    slide: Option<Slide>,
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
    /// Starts the slide to the other panel; the state flips when it ends.
    pub fn toggle(&mut self) {
        if self.slide.is_none() {
            self.slide = Some(Slide { collapsing: !collapsed(), step: 0.0 });
        }
    }

    /// Freezes a slide part-way, for screenshots.
    pub fn show_slide(&mut self, collapsing: bool, step: f32) {
        set_collapsed(!collapsing);
        self.slide = Some(Slide { collapsing, step });
    }

    pub fn sliding(&self) -> bool {
        self.slide.is_some()
    }

    pub fn update(&mut self, dt: f32) {
        let Some(s) = &mut self.slide else { return };
        s.step += dt * 60.0;
        if s.step >= SLIDE_STEPS {
            set_collapsed(s.collapsing);
            self.slide = None;
            self.hover_button = None;
        }
    }

    /// Where the full panel's left edge is drawn, if it shows at all.
    fn full_left(&self, screen_w: f32) -> Option<f32> {
        match self.slide {
            Some(s) => Some(screen_w - WIDTH + s.offset()),
            None if collapsed() => None,
            None => Some(screen_w - WIDTH),
        }
    }

    /// Left edge of the full panel for the minimap window, while it shows.
    pub fn minimap_left(&self, screen_w: f32) -> Option<f32> {
        self.full_left(screen_w)
    }

    fn button_at(&self, screen_w: f32, p: [f32; 2]) -> Option<Button> {
        if self.slide.is_some() {
            return None;
        }
        let ox = panel_left(screen_w);
        let inside = |x: f32, y: f32, w: f32, h: f32| p[0] >= ox + x && p[0] < ox + x + w && p[1] >= y && p[1] < y + h;
        if !collapsed() {
            if inside(SPEED_DOWN.0, SPEED_DOWN.1, ARROW, ARROW) {
                return Some(Button::SpeedDown);
            }
            if inside(SPEED_UP.0, SPEED_UP.1, ARROW, ARROW) {
                return Some(Button::SpeedUp);
            }
        }
        layout(collapsed()).iter().find(|l| inside(l.x, l.y, l.w, l.h)).map(|l| l.button)
    }

    /// The build submenu, as in the original: 384-wide strips ending 10 pixels left of
    /// the panel (full or collapsed), the last one at y 432 and the list growing
    /// upwards. Returns the rectangle with 8 pixels above the first strip.
    fn menu_rect(&self, screen: [f32; 2]) -> (f32, f32, f32, f32) {
        let w = 384.0;
        let n = self.items.len().max(1) as f32;
        let first = (432.0 - 24.0 * (n - 1.0)).max(TOP + 8.0).min(screen[1] - 24.0 * n);
        (panel_left(screen[0]) - w - 10.0, first - 8.0, w, 24.0 * n + 16.0)
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
        let left = match self.slide {
            Some(_) => screen[0] - WIDTH,
            None => panel_left(screen[0]),
        };
        p[0] >= left || p[1] < TOP || self.item_at(screen, p).is_some()
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
            Button::Empire => s.has_empire,
            Button::Collapse | Button::Advisors | Button::SpeedDown | Button::SpeedUp => true,
        }
    }

    /// Draws the panel. The tooltip goes on last, over the minimap: see
    /// [`Sidebar::draw_tooltip`].
    pub fn draw(&self, r: &mut Renderer, img: &SidebarImages, s: &SidebarState) {
        let [w, h] = r.screen;
        // Top bar, ending where the strip starts. The image's border stops short of
        // its right end, the part meant to sit over the panel. One copy ends at the
        // strip; more are laid leftward a bordered length apart, each covering the
        // plain end of the one before it.
        let bar_w = r.record(img.top_bar).map_or(1000.0, |rec| rec.width as f32);
        let mut starts = vec![w - STRIP_W - bar_w];
        while starts.last().is_some_and(|&x| x > 0.0) {
            let x = starts.last().copied().unwrap_or(0.0) - TOP_BAR_BORDERED;
            starts.push(x);
        }
        for &x in starts.iter().rev() {
            r.image(img.top_bar, [x, 0.0], WHITE, Space::Screen);
        }

        let column = self.slide.is_some() || collapsed();
        if column {
            self.draw_column(r, img, s, w - COLLAPSED_WIDTH);
        }
        if let Some(ox) = self.full_left(w) {
            // A sliding panel passes under the strip.
            let clip = self.slide.map(|_| [0.0, 0.0, w - STRIP_W, h]);
            r.set_clip(clip);
            self.draw_full(r, img, s, ox, clip);
            r.set_clip(None);
        }

        // The strip: the first copy from the top of the screen, each further copy
        // tucked under the one above so the pattern runs on.
        let copies = ((h - STRIP_H) / (STRIP_H - STRIP_OVERLAP)).ceil().max(0.0) as i32;
        for i in (0..=copies).rev() {
            let y = if i == 0 { 0.0 } else { i as f32 * (STRIP_H - STRIP_OVERLAP) };
            r.image(img.strip, [w - STRIP_W, y], WHITE, Space::Screen);
        }

        if self.open.is_some() && !self.items.is_empty() {
            self.draw_menu(r, img);
        }
    }

    /// Reliefs below the panel. At 768 high one fills the space from y 480, as in
    /// the original; taller screens stack more upwards from the bottom edge, so
    /// the one cut short is the top one, under the status block.
    fn draw_reliefs(r: &mut Renderer, image: u32, x: f32, w: f32, clip: Option<[f32; 4]>) {
        let h = r.screen[1];
        let mut y = if h <= RELIEF_Y + RELIEF_H + 3.0 { RELIEF_Y } else { h - RELIEF_H };
        let area = [x, RELIEF_Y, w, (h - RELIEF_Y).max(0.0)];
        r.set_clip(Some(match clip {
            Some(c) => intersect(c, area),
            None => area,
        }));
        while y + RELIEF_H > RELIEF_Y {
            r.image(image, [x, y], WHITE, Space::Screen);
            y -= RELIEF_H;
        }
        r.set_clip(clip);
    }

    fn draw_buttons(&self, r: &mut Renderer, img: &SidebarImages, s: &SidebarState, ox: f32, collapsed: bool) {
        for l in layout(collapsed) {
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
            let base = match l.group {
                G110 => img.collapse,
                G108 => img.column_buttons,
                _ => img.buttons,
            };
            r.image(base + l.offset + state, [ox + l.x, l.y], WHITE, Space::Screen);
        }
    }

    fn draw_column(&self, r: &mut Renderer, img: &SidebarImages, s: &SidebarState, cx: f32) {
        let h = r.screen[1];
        r.rect([cx, TOP], [COLUMN_W, h - TOP], [0.0, 0.0, 0.0, 1.0], Space::Screen);
        Self::draw_reliefs(r, img.column_relief, cx, COLUMN_W, None);
        r.image(img.column, [cx, TOP], WHITE, Space::Screen);
        if self.slide.is_none() {
            self.draw_buttons(r, img, s, cx, true);
        } else {
            // Mid-slide the buttons show at rest.
            for l in &COLLAPSED_LAYOUT {
                let base = if l.group == G110 { img.collapse } else { img.column_buttons };
                let state = if Self::enabled(l.button, s) { 0 } else { 3 };
                let state = if l.group == G110 { 0 } else { state };
                r.image(base + l.offset + state, [cx + l.x, l.y], WHITE, Space::Screen);
            }
        }
    }

    fn draw_full(&self, r: &mut Renderer, img: &SidebarImages, s: &SidebarState, ox: f32, clip: Option<[f32; 4]>) {
        let h = r.screen[1];
        // Behind the panel's windows.
        r.rect([ox, TOP], [PANEL_W, h - TOP], [0.0, 0.0, 0.0, 1.0], Space::Screen);
        Self::draw_reliefs(r, img.relief, ox, PANEL_W, clip);
        r.image(img.panel, [ox, TOP], WHITE, Space::Screen);
        let picture = s.category.map_or(1, Category::picture);
        r.image(img.pictures + picture, [ox + 11.0, 211.0], WHITE, Space::Screen);
        self.draw_buttons(r, img, s, ox, false);
        if s.unread > 0 {
            draw_text(r, Font::NormalBlackOnDark, &s.unread.to_string(), ox + 52.0, 450.0, font::WHITE);
        }
        self.draw_status(r, img, s, ox);
    }

    fn draw_status(&self, r: &mut Renderer, img: &SidebarImages, s: &SidebarState, ox: f32) {
        panel::inner_panel(r, &img.panels, ox + STATUS_X, STATUS_Y, STATUS_BLOCKS_W, STATUS_BLOCKS_H);
        let f = Font::NormalWhiteOnDark;
        draw_text(r, f, "Speed", ox + STATUS_TEXT_X, STATUS_Y + 6.0, font::WHITE);
        for (b, (bx, by), image) in [
            (Button::SpeedDown, SPEED_DOWN, img.arrow_down),
            (Button::SpeedUp, SPEED_UP, img.arrow_up),
        ] {
            let pressed = self.pressed == Some(b);
            r.image(image + pressed as u32, [ox + bx, by], WHITE, Space::Screen);
        }
        draw_text(r, f, s.speed, ox + STATUS_TEXT_X + 56.0, STATUS_Y + 28.0, font::WHITE);
        let mut y = STATUS_Y + 54.0;
        for (label, value) in s.lines {
            draw_text(r, f, label, ox + STATUS_TEXT_X, y, font::WHITE);
            let vw = text_width(r, f, value) as f32;
            // A long value ("100%") keeps a gap after its label, using the margin.
            let after = ox + STATUS_TEXT_X + text_width(r, f, label) as f32 + 5.0;
            draw_text(r, f, value, (ox + STATUS_RIGHT - vw).max(after), y, font::WHITE);
            y += 18.0;
        }
    }

    /// The hovered button's tooltip; drawn after the minimap so it lies over it.
    pub fn draw_tooltip(&self, r: &mut Renderer, s: &SidebarState) {
        let Some(b) = self.hover_button else { return };
        let Some(tip) = layout(collapsed()).iter().find(|l| l.button == b).and_then(|l| (s.tips)(l.tip)) else { return };
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
            // Panel-button strips: style 2 normally, 1 under the mouse.
            panel::label(r, &img.panels, x, iy, (w / 16.0) as i32, if focus { 1 } else { 2 });
            let f = if focus { Font::NormalBlackOnDark } else { Font::NormalBlackOnLight };
            let tint = if item.enabled { [1.0; 4] } else { [0.55, 0.5, 0.45, 1.0] };
            draw_text_tinted(r, f, &item.label, x + 8.0, iy + 3.0, tint);
            if item.cost > 0 {
                draw_text_tinted(r, f, &format!("{} Deben", item.cost), x + w - 92.0, iy + 3.0, tint);
            }
        }
    }
}

/// Overlap of two `[x, y, w, h]` rectangles.
fn intersect(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let x0 = a[0].max(b[0]);
    let y0 = a[1].max(b[1]);
    let x1 = (a[0] + a[2]).min(b[0] + b[2]);
    let y1 = (a[1] + a[3]).min(b[1] + b[3]);
    [x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0)]
}
