//! An original-style message/briefing popup: a raised outer panel with a centered
//! title, an optional subtitle, a sunken body of word-wrapped [`rich_text`] and an OK
//! button. Text too long for the body gets the original scroll bar down its right
//! side: arrow buttons at either end (held down, they repeat) and a stone to drag.
//!
//! [`MessageDialog`] draws itself centered on the current screen size (recomputed every
//! frame, so it stays centered across a resize) and reports clicks/scroll back to the
//! caller; it does not own "is this dialog open" state — callers keep an
//! `Option<MessageDialog>` (or similar) and drop it when [`MessageDialog::click`]
//! reports the OK button was pressed.

use crate::font::{self, Font, draw_text, text_width};
use crate::panel::{self, PanelImages};
use crate::rich_text::{self, RendererMeasure};
use osiris_formats::{Message, TextTable};
use osiris_render::{Renderer, Space, WHITE};
use std::time::{Duration, Instant};

const BLOCK: f32 = 16.0;
/// Padding inside the sunken body panel before text starts, matching the border
/// thickness drawn by `inner_panel`/`outer_panel` plus a little breathing room.
const BODY_PAD_X: f32 = 8.0;
const BODY_PAD_Y: f32 = 6.0;
const OK_W: f32 = 100.0;
const OK_H: f32 = 22.0;
/// Fallback dialog size (in 16px blocks) for messages whose `size` field is `(0, 0)`.
const DEFAULT_WIDTH_BLOCKS: i32 = 22;
const MIN_HEIGHT_BLOCKS: i32 = 8;
const MAX_HEIGHT_BLOCKS: i32 = 26;
/// The scroll bar's arrow buttons (Pharaoh_General group 96) and its stone.
const ARROW_W: f32 = 39.0;
const ARROW_H: f32 = 26.0;
const DOT: f32 = 25.0;
const REPEAT_DELAY: Duration = Duration::from_millis(300);
const REPEAT: Duration = Duration::from_millis(60);

/// What the left button is holding down on the scroll bar.
#[derive(Clone, Copy)]
enum Held {
    /// An arrow (-1 up, 1 down), repeating from the given time.
    Arrow(i32, Instant),
    Dot,
}

struct Geometry {
    x: f32,
    y: f32,
    w: f32,
    body_x: f32,
    body_y: f32,
    body_wb: i32,
    body_hb: i32,
    ok: (f32, f32, f32, f32),
    /// The scroll bar's left edge, top and height.
    bar: (f32, f32, f32),
}

pub struct MessageDialog {
    pub message_id: u16,
    title: String,
    subtitle: String,
    layout: rich_text::Layout,
    width_blocks: i32,
    height_blocks: i32,
    scroll: f32,
    ok_hover: bool,
    panels: PanelImages,
    /// First up-arrow image, or `None` when the text fits without scrolling.
    arrows: Option<u32>,
    held: Option<Held>,
    cursor: [f32; 2],
}

impl MessageDialog {
    /// Builds a dialog for `msg`, wrapping its content text against `renderer`'s fonts.
    ///
    /// `text` is accepted for API symmetry with the rest of the engine (and in case a
    /// future caller wants to resolve cross-message links found in the content) but
    /// isn't otherwise consulted here: `Message`'s title/subtitle/content are already
    /// fully decoded strings, not `TextTable` indices.
    pub fn new(r: &Renderer, msg: &Message, _text: &TextTable) -> Self {
        let width_blocks = if msg.size.0 > 0 { msg.size.0 as i32 } else { DEFAULT_WIDTH_BLOCKS };
        let text_width_px = (width_blocks - 2) * BLOCK as i32 - 2 * BODY_PAD_X as i32;

        let mut measure = RendererMeasure::new(r);
        let mut opts = rich_text::Options {
            font: Font::NormalBlackOnLight,
            width: text_width_px.max(16),
            paragraph_indent: 50,
        };
        let mut layout = rich_text::layout(&msg.content, &opts, &mut measure);

        let height_blocks = if msg.size.1 > 0 {
            msg.size.1 as i32
        } else {
            let title_area = 2;
            let subtitle_area = if msg.subtitle.is_empty() { 0 } else { 1 };
            let body_lines = (layout.height + 10) / BLOCK as i32 + 1;
            let ok_area = 2;
            (title_area + subtitle_area + body_lines + ok_area + 1).clamp(MIN_HEIGHT_BLOCKS, MAX_HEIGHT_BLOCKS)
        };

        // Text taller than the body is laid out again, narrower, beside a scroll bar.
        let body_hb = Self::body_blocks(height_blocks, !msg.subtitle.is_empty());
        let scrolls = layout.height as f32 > body_hb as f32 * BLOCK - 2.0 * BODY_PAD_Y;
        if scrolls {
            opts.width = (text_width_px - ARROW_W as i32 - 4).max(16);
            layout = rich_text::layout(&msg.content, &opts, &mut measure);
        }
        let arrows = if scrolls { r.library.group_id("Pharaoh_General", 96, 8).ok() } else { None };

        Self {
            message_id: msg.id,
            title: msg.title.clone(),
            subtitle: msg.subtitle.clone(),
            layout,
            width_blocks,
            height_blocks,
            scroll: 0.0,
            ok_hover: false,
            panels: PanelImages::load(&r.library).expect("panel art"),
            arrows,
            held: None,
            cursor: [0.0; 2],
        }
    }

    /// Height of the sunken body in blocks: what the title, subtitle, OK row and
    /// margins leave.
    fn body_blocks(height_blocks: i32, subtitle: bool) -> i32 {
        let title_area = 3.0 * BLOCK;
        let subtitle_area = if subtitle { BLOCK } else { 0.0 };
        let bottom_chrome = 2.0 * BLOCK + BLOCK; // OK row + margin
        ((height_blocks as f32 * BLOCK - bottom_chrome - title_area - subtitle_area) / BLOCK).floor().max(1.0) as i32
    }

    fn geometry(&self, screen: [f32; 2]) -> Geometry {
        let w = self.width_blocks as f32 * BLOCK;
        let h = self.height_blocks as f32 * BLOCK;
        let x = ((screen[0] - w) / 2.0).max(0.0);
        let y = ((screen[1] - h) / 2.0).max(0.0);

        let title_area = 3.0 * BLOCK;
        let subtitle_area = if self.subtitle.is_empty() { 0.0 } else { BLOCK };
        let body_y = y + title_area + subtitle_area;
        let body_wb = self.width_blocks - 2;
        let body_hb = Self::body_blocks(self.height_blocks, !self.subtitle.is_empty());
        let body_x = x + BLOCK;

        let ok_x = x + (w - OK_W) / 2.0;
        let ok_y = y + h - 2.0 * BLOCK;
        Geometry {
            x,
            y,
            w,
            body_x,
            body_y,
            body_wb,
            body_hb,
            ok: (ok_x, ok_y, OK_W, OK_H),
            bar: (body_x + body_wb as f32 * BLOCK - ARROW_W - 3.0, body_y + 3.0, body_hb as f32 * BLOCK - 6.0),
        }
    }

    /// Body viewport height in pixels, for clamping scroll.
    fn body_viewport(&self, screen: [f32; 2]) -> f32 {
        let g = self.geometry(screen);
        g.body_hb as f32 * BLOCK - 2.0 * BODY_PAD_Y
    }

    fn max_scroll(&self, screen: [f32; 2]) -> f32 {
        (self.layout.height as f32 - self.body_viewport(screen)).max(0.0)
    }

    /// Updates hover state (for the OK button's focus frame) and drags the scroll
    /// bar's stone. Call from the app's mouse move handler with the same `screen`
    /// passed to `draw`.
    pub fn hover(&mut self, p: [f32; 2], screen: [f32; 2]) {
        self.cursor = p;
        let g = self.geometry(screen);
        let (ox, oy, ow, oh) = g.ok;
        self.ok_hover = p[0] >= ox && p[0] < ox + ow && p[1] >= oy && p[1] < oy + oh;
        if matches!(self.held, Some(Held::Dot)) {
            self.drag_dot(p[1], &g, screen);
        }
    }

    /// Handles a left click at `p`. Returns `true` if it landed on the OK button (the
    /// caller should then close the dialog).
    pub fn click(&mut self, p: [f32; 2], screen: [f32; 2]) -> bool {
        let g = self.geometry(screen);
        let (bx, by, bh) = g.bar;
        if self.arrows.is_some() && p[0] >= bx && p[0] < bx + ARROW_W && p[1] >= by && p[1] < by + bh {
            if p[1] < by + ARROW_H {
                self.step(-1, screen);
                self.held = Some(Held::Arrow(-1, Instant::now() + REPEAT_DELAY));
            } else if p[1] >= by + bh - ARROW_H {
                self.step(1, screen);
                self.held = Some(Held::Arrow(1, Instant::now() + REPEAT_DELAY));
            } else {
                self.held = Some(Held::Dot);
                self.drag_dot(p[1], &g, screen);
            }
            return false;
        }
        let (ox, oy, ow, oh) = g.ok;
        p[0] >= ox && p[0] < ox + ow && p[1] >= oy && p[1] < oy + oh
    }

    /// The left button came up: stops a held arrow or a dragged stone.
    pub fn release(&mut self) {
        self.held = None;
    }

    /// Scrolls one line up (`dir` -1) or down (1).
    fn step(&mut self, dir: i32, screen: [f32; 2]) {
        let line = Font::NormalBlackOnLight.line_height() as f32;
        self.scroll = (self.scroll + dir as f32 * line).clamp(0.0, self.max_scroll(screen));
    }

    /// Room the stone travels in, between the arrows.
    fn track(g: &Geometry) -> f32 {
        (g.bar.2 - 2.0 * ARROW_H - DOT).max(1.0)
    }

    /// Puts the stone's centre under the pointer at `y`.
    fn drag_dot(&mut self, y: f32, g: &Geometry, screen: [f32; 2]) {
        let t = ((y - g.bar.1 - ARROW_H - DOT / 2.0) / Self::track(g)).clamp(0.0, 1.0);
        let max = self.max_scroll(screen);
        let line = Font::NormalBlackOnLight.line_height() as f32;
        // Whole lines, as the original scrolls.
        self.scroll = ((t * max / line).round() * line).min(max);
    }

    /// Applies a mouse wheel step (`delta` in the same sign convention as the window
    /// event's scroll: positive scrolls content up). One step moves about three lines.
    pub fn scroll(&mut self, delta: f32, screen: [f32; 2]) {
        let step = Font::NormalBlackOnLight.line_height() as f32 * 3.0;
        self.scroll = (self.scroll - delta * step).clamp(0.0, self.max_scroll(screen));
    }

    /// True if the dialog's content is taller than its body viewport.
    pub fn scrollable(&self, screen: [f32; 2]) -> bool {
        self.max_scroll(screen) > 0.0
    }

    pub fn draw(&mut self, r: &mut Renderer) {
        let screen = r.screen;
        let g = self.geometry(screen);
        if let Some(Held::Arrow(dir, next)) = self.held
            && Instant::now() >= next
        {
            self.step(dir, screen);
            self.held = Some(Held::Arrow(dir, Instant::now() + REPEAT));
        }

        panel::outer_panel(r, &self.panels, g.x, g.y, self.width_blocks, self.height_blocks);

        let tw = text_width(r, Font::LargeBlackOnLight, &self.title) as f32;
        draw_text(r, Font::LargeBlackOnLight, &self.title, g.x + (g.w - tw) / 2.0, g.y + BLOCK, font::BLACK);

        if !self.subtitle.is_empty() {
            draw_text(r, Font::NormalBlackOnLight, &self.subtitle, g.x + BLOCK, g.y + 2.0 * BLOCK - 2.0, font::BLACK);
        }

        panel::inner_panel(r, &self.panels, g.body_x, g.body_y, g.body_wb, g.body_hb);
        let body_h = g.body_hb as f32 * BLOCK - 2.0 * BODY_PAD_Y;
        r.set_clip(Some([g.body_x + 2.0, g.body_y + 2.0, g.body_wb as f32 * BLOCK - 4.0, g.body_hb as f32 * BLOCK - 4.0]));
        rich_text::draw(
            r,
            &self.layout,
            [g.body_x + BODY_PAD_X, g.body_y + BODY_PAD_Y],
            body_h,
            self.scroll,
            font::BLACK,
        );
        r.set_clip(None);
        if let Some(arrows) = self.arrows {
            self.draw_bar(r, &g, arrows);
        }

        let (ox, oy, ow, oh) = g.ok;
        panel::button_border(r, &self.panels, ox, oy, ow as i32, oh as i32, self.ok_hover);
        let label = "OK";
        let lw = text_width(r, Font::NormalBlackOnLight, label) as f32;
        draw_text(r, Font::NormalBlackOnLight, label, ox + (ow - lw) / 2.0, oy + (oh - 11.0) / 2.0, font::BLACK);
    }

    fn draw_bar(&self, r: &mut Renderer, g: &Geometry, arrows: u32) {
        let (bx, by, bh) = g.bar;
        let over = |y: f32| self.cursor[0] >= bx && self.cursor[0] < bx + ARROW_W && self.cursor[1] >= y && self.cursor[1] < y + ARROW_H;
        for (dir, y, image) in [(-1, by, arrows), (1, by + bh - ARROW_H, arrows + 4)] {
            let state = if matches!(self.held, Some(Held::Arrow(d, _)) if d == dir) {
                2
            } else {
                over(y) as u32
            };
            r.image(image + state, [bx, y], WHITE, Space::Screen);
        }
        let max = self.max_scroll(r.screen);
        let t = if max > 0.0 { self.scroll / max } else { 0.0 };
        let dot_y = by + ARROW_H + (t * Self::track(g)).round();
        r.image(self.panels.panel_button + 39, [bx + ((ARROW_W - DOT) / 2.0).floor(), dot_y], WHITE, Space::Screen);
    }
}
