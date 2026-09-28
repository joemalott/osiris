//! The game rules window: switches for the optional rule changes ("cheats"). Shown
//! from the main menu, where it sets the rules for every game, and in a running game.

use crate::lang::{n_, tr};
use osiris_render::{Renderer, Space};
use osiris_sim::Rules;
use osiris_ui::{Font, PanelImages, draw_text, font, panel, text_width};

const W_BLOCKS: i32 = 38;
const ROW_H: f32 = 40.0;
const TOP: f32 = 56.0;

type Field = fn(&mut Rules) -> &mut bool;

/// Each switch: its name, what it does, and the rule it sets.
const SWITCHES: [(&str, &str, Field); 9] = [
    (n_("Global labor pool"), n_("Buildings hire from the whole city; no walker needs to find workers."), |r| &mut r.global_labor_pool),
    (n_("Gods"), n_("When off, gods have no moods and houses never need religion."), |r| &mut r.gods_enabled),
    (n_("Nile floods"), n_("When off, the river stays in its banks. Farms still harvest yearly."), |r| &mut r.floods),
    (n_("Disasters"), n_("Earthquakes and other acts of the gods."), |r| &mut r.disasters),
    (n_("Fire"), n_("Buildings can catch fire."), |r| &mut r.fire),
    (n_("Disease"), n_("Plague in unhealthy neighbourhoods."), |r| &mut r.disease),
    (n_("Collapse"), n_("Neglected buildings can fall down."), |r| &mut r.collapse),
    (n_("New storage accepts nothing"), n_("New storage yards and granaries start at Accept None."), |r| &mut r.storage_accepts_none),
    (n_("Ptah speeds carpenters"), n_("Fixes the original: Ptah's altar speeds the carpenters' guild."), |r| &mut r.ptah_speeds_guilds),
];

#[derive(Default)]
pub struct RulesPanel {
    hover: Option<usize>,
    hover_close: bool,
    /// The width in 16-pixel blocks as last drawn: wider than the usual 38 where a
    /// translation needs it.
    blocks: std::cell::Cell<i32>,
}

pub enum RulesClick {
    Toggled,
    Close,
    Inside,
    Outside,
}

impl RulesPanel {
    fn size(&self) -> (f32, f32) {
        (self.blocks.get().max(W_BLOCKS) as f32 * 16.0, TOP + SWITCHES.len() as f32 * ROW_H + 72.0)
    }

    /// Widens the window to the longest line in the current language, as far as
    /// `area_w` allows.
    fn fit(&self, r: &Renderer, area_w: f32, note: &str) {
        let mut need = text_width(r, Font::SmallPlain, note) + 32;
        for (name, what, _) in SWITCHES {
            let line = text_width(r, Font::NormalBlackOnLight, tr(name)).max(text_width(r, Font::SmallPlain, tr(what)));
            need = need.max(24 + line + 16 + 64 + 24);
        }
        let most = ((area_w as i32 - 16) / 16).max(W_BLOCKS);
        self.blocks.set(((need + 15) / 16).clamp(W_BLOCKS, most));
    }

    fn origin(&self, screen: [f32; 2], area_w: f32) -> (f32, f32) {
        let (w, h) = self.size();
        (((area_w - w) / 2.0).max(0.0).floor(), ((screen[1] - h) / 2.0).max(32.0).floor())
    }

    fn close_rect(&self, x: f32, y: f32) -> (f32, f32, f32, f32) {
        let (w, h) = self.size();
        (x + (w - 160.0) / 2.0, y + h - 48.0, 160.0, 25.0)
    }

    fn row_at(&self, screen: [f32; 2], area_w: f32, p: [f32; 2]) -> Option<usize> {
        let (x, y) = self.origin(screen, area_w);
        let (w, _) = self.size();
        if p[0] < x + 16.0 || p[0] > x + w - 16.0 || p[1] < y + TOP {
            return None;
        }
        let i = ((p[1] - y - TOP) / ROW_H) as usize;
        (i < SWITCHES.len()).then_some(i)
    }

    fn in_close(&self, screen: [f32; 2], area_w: f32, p: [f32; 2]) -> bool {
        let (x, y) = self.origin(screen, area_w);
        let (cx, cy, cw, ch) = self.close_rect(x, y);
        p[0] >= cx && p[0] < cx + cw && p[1] >= cy && p[1] < cy + ch
    }

    /// `area_w` is the width to centre in (the screen, or the city view left of the
    /// sidebar).
    pub fn hover(&mut self, screen: [f32; 2], area_w: f32, p: [f32; 2]) {
        self.hover = self.row_at(screen, area_w, p);
        self.hover_close = self.in_close(screen, area_w, p);
    }

    pub fn click(&mut self, rules: &mut Rules, screen: [f32; 2], area_w: f32, p: [f32; 2]) -> RulesClick {
        if self.in_close(screen, area_w, p) {
            return RulesClick::Close;
        }
        if let Some(i) = self.row_at(screen, area_w, p) {
            let v = (SWITCHES[i].2)(rules);
            *v = !*v;
            return RulesClick::Toggled;
        }
        let (x, y) = self.origin(screen, area_w);
        let (w, h) = self.size();
        if p[0] >= x && p[1] >= y && p[0] < x + w && p[1] < y + h { RulesClick::Inside } else { RulesClick::Outside }
    }

    pub fn draw(&self, r: &mut Renderer, panels: &PanelImages, rules: &Rules, area_w: f32, note: &str) {
        self.fit(r, area_w, note);
        let (x, y) = self.origin(r.screen, area_w);
        let (w, h) = self.size();
        panel::outer_panel(r, panels, x, y, self.blocks.get().max(W_BLOCKS), (h / 16.0).ceil() as i32);
        let title = tr("Game rules");
        let tw = text_width(r, Font::LargeBlackOnLight, title) as f32;
        draw_text(r, Font::LargeBlackOnLight, title, x + (w - tw) / 2.0, y + 14.0, font::BLACK);
        let nw = text_width(r, Font::SmallPlain, note) as f32;
        draw_text(r, Font::SmallPlain, note, x + (w - nw) / 2.0, y + 38.0, font::BLACK);
        let mut rules = rules.clone();
        for (i, (name, what, field)) in SWITCHES.iter().enumerate() {
            let ry = y + TOP + i as f32 * ROW_H;
            let on = *field(&mut rules);
            let focus = self.hover == Some(i);
            let f = if focus { Font::NormalBlue } else { Font::NormalBlackOnLight };
            draw_text(r, f, tr(name), x + 24.0, ry + 2.0, font::BLACK);
            draw_text(r, Font::SmallPlain, tr(what), x + 24.0, ry + 20.0, font::BLACK);
            // The switch: a large label reading On or Off.
            let (bx, bw) = (x + w - 24.0 - 64.0, 4);
            panel::large_label(r, panels, bx, ry + 2.0, bw, focus as u32);
            let label = if on { tr("On") } else { tr("Off") };
            let lw = text_width(r, Font::NormalBlackOnLight, label) as f32;
            draw_text(r, Font::NormalBlackOnLight, label, bx + (64.0 - lw) / 2.0, ry + 8.0, font::BLACK);
            if !on {
                r.rect([bx, ry + 2.0], [64.0, 25.0], [0.0, 0.0, 0.0, 0.3], Space::Screen);
            }
        }
        let (cx, cy, cw, _) = self.close_rect(x, y);
        panel::large_label(r, panels, cx, cy, (cw / 16.0) as i32, self.hover_close as u32);
        let done = tr("Done");
        let lw = text_width(r, Font::NormalBlackOnLight, done) as f32;
        draw_text(r, Font::NormalBlackOnLight, done, cx + (cw - lw) / 2.0, cy + 6.0, font::BLACK);
    }
}
