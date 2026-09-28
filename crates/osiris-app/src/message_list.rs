//! The messages window: the city's message log, newest first. Clicking a message
//! opens it.

use crate::lang::tr;
use osiris_formats::{MessageTable, TextTable};
use osiris_render::{Renderer, Space, WHITE};
use osiris_sim::World;
use osiris_ui::{Font, PanelImages, draw_text, font, panel, text_width};

const W_BLOCKS: i32 = 30;
const H_BLOCKS: i32 = 24;
const ROWS: usize = 14;
const ROW_H: f32 = 20.0;
const TEXT_MONTHS: usize = 25;

#[derive(Default)]
pub struct MessageList {
    scroll: usize,
    hover: Option<usize>,
}

impl MessageList {
    fn origin(screen: [f32; 2]) -> (f32, f32) {
        let w = W_BLOCKS as f32 * 16.0;
        let h = H_BLOCKS as f32 * 16.0;
        let area = screen[0] - crate::sidebar::width();
        (((area - w) / 2.0).max(0.0), ((screen[1] - h) / 2.0).max(40.0))
    }

    pub fn contains(&self, screen: [f32; 2], p: [f32; 2]) -> bool {
        let (x, y) = Self::origin(screen);
        p[0] >= x && p[1] >= y && p[0] < x + W_BLOCKS as f32 * 16.0 && p[1] < y + H_BLOCKS as f32 * 16.0
    }

    /// Row under `p`, as an index into the log.
    fn row_at(&self, world: &World, screen: [f32; 2], p: [f32; 2]) -> Option<usize> {
        let (x, y) = Self::origin(screen);
        let top = y + 56.0;
        if p[0] < x + 24.0 || p[0] > x + W_BLOCKS as f32 * 16.0 - 24.0 || p[1] < top {
            return None;
        }
        let row = ((p[1] - top) / ROW_H) as usize;
        let n = world.notices.log.len();
        let i = row + self.scroll;
        (row < ROWS && i < n).then(|| n - 1 - i)
    }

    pub fn hover(&mut self, world: &World, screen: [f32; 2], p: [f32; 2]) {
        self.hover = self.row_at(world, screen, p);
    }

    pub fn scroll(&mut self, world: &World, lines: i32) {
        let max = world.notices.log.len().saturating_sub(ROWS) as i32;
        self.scroll = (self.scroll as i32 + lines).clamp(0, max) as usize;
    }

    pub fn click(&self, world: &World, screen: [f32; 2], p: [f32; 2]) -> Option<usize> {
        self.row_at(world, screen, p)
    }

    pub fn draw(&self, r: &mut Renderer, panels: &PanelImages, world: &World, messages: &MessageTable, text: &TextTable) {
        let (x, y) = Self::origin(r.screen);
        let w = W_BLOCKS as f32 * 16.0;
        panel::outer_panel(r, panels, x, y, W_BLOCKS, H_BLOCKS);
        let title = tr("Messages");
        let tw = text_width(r, Font::LargeBlackOnLight, title) as f32;
        draw_text(r, Font::LargeBlackOnLight, title, x + (w - tw) / 2.0, y + 14.0, font::BLACK);
        panel::inner_panel(r, panels, x + 16.0, y + 48.0, W_BLOCKS - 2, H_BLOCKS - 6);
        let log = &world.notices.log;
        if log.is_empty() {
            draw_text(r, Font::NormalBlackOnDark, tr("No messages"), x + 32.0, y + 60.0, font::WHITE);
        }
        // As the original's list (FUN_004e1a50): a scroll icon, open once the message is
        // read, then the date and the title, yellow under the mouse and light otherwise.
        let icons = r.library.group_id("Pharaoh_General", 90, 14).ok();
        for (row, (i, n)) in log.iter().enumerate().rev().skip(self.scroll).take(ROWS).enumerate() {
            let ry = y + 58.0 + row as f32 * ROW_H;
            if let Some(icon) = icons {
                r.image(icon + n.read as u32, [x + 28.0, ry - 2.0], WHITE, Space::Screen);
            }
            let f = if self.hover == Some(i) { Font::NormalYellow } else { Font::NormalWhiteOnDark };
            let month = text.get(TEXT_MONTHS, n.month as usize).unwrap_or("?");
            let year = crate::game::year_text(text, n.year);
            draw_text(r, f, &format!("{month} {year}"), x + 58.0, ry, font::WHITE);
            let title = osiris_sim::missions::message_id(&n.key)
                .and_then(|id| messages.get(id as usize))
                .map_or_else(|| n.key.clone(), |m| m.title.clone());
            draw_text(r, f, &title, x + 200.0, ry, font::WHITE);
        }
        let hint = tr("Click a message to read it. Right-click or Esc to close.");
        draw_text(r, Font::NormalBlackOnLight, hint, x + 24.0, y + H_BLOCKS as f32 * 16.0 - 30.0, font::BLACK);
    }
}
