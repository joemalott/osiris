//! The front end: main menu, campaign mission list, custom maps, saved games and the
//! game rules, drawn over the original's background art.

use crate::rules_panel::{RulesClick, RulesPanel};
use osiris_render::{Renderer, Space, WHITE};
use osiris_sim::Rules;
use osiris_ui::{Font, PanelImages, draw_text, draw_text_tinted, font, panel, text_width};
use std::path::PathBuf;

/// Background images in Pharaoh_Unloaded (global ids).
const BG_TITLE: u32 = 201;
const BG_CHOOSE_GAME: u32 = 656;
const BG_HISTORY: u32 = 658;
const BG_CUSTOM: u32 = 657;

/// The history plaque in `BG_HISTORY` (1024x768): the dark list panel on the right and
/// the sandstone area under the picture frame on the left.
const PLAQUE_LIST: [f32; 4] = [522.0, 204.0, 808.0, 584.0];
const PLAQUE_INFO: [f32; 4] = [226.0, 366.0, 496.0, 600.0];

#[derive(Debug, Clone, PartialEq)]
pub enum Choice {
    Mission(usize),
    Map(PathBuf),
    Save(PathBuf),
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Main,
    Campaign,
    Custom,
    Load,
    Rules,
}

struct Item {
    label: String,
    enabled: bool,
    action: Action,
}

#[derive(Clone)]
enum Action {
    Go(Page),
    Choose(Choice),
}

pub struct Menu {
    page: Page,
    items: Vec<Item>,
    hover: Option<usize>,
    scroll: usize,
    mission_names: Vec<String>,
    unlocked: usize,
    maps: Vec<PathBuf>,
    /// Newest first.
    saves: Vec<PathBuf>,
    pub selected_mission: Option<usize>,
    pub rules: Rules,
    rules_panel: RulesPanel,
    /// Set when the rules change, so the caller can store them.
    pub rules_changed: bool,
    hover_back: bool,
}

const LIST_ROWS: usize = 16;
const ROW_H: f32 = 22.0;
const BOX_W: f32 = 400.0;
const BUTTON_W: f32 = 256.0;
const BUTTON_H: f32 = 25.0;

/// Where a background image lands when scaled to cover the screen: offset and scale.
fn cover(r: &Renderer, image: u32) -> ([f32; 2], f32) {
    let [sw, sh] = r.screen;
    let Some(rec) = r.record(image) else { return ([0.0, 0.0], 1.0) };
    let (w, h) = (rec.width as f32, rec.height as f32);
    let s = (sw / w).max(sh / h);
    ([(sw - w * s) / 2.0, (sh - h * s) / 2.0], s)
}

fn cover_screen(screen: [f32; 2], size: [f32; 2]) -> ([f32; 2], f32) {
    let s = (screen[0] / size[0]).max(screen[1] / size[1]);
    ([(screen[0] - size[0] * s) / 2.0, (screen[1] - size[1] * s) / 2.0], s)
}

fn inside(p: [f32; 2], x: f32, y: f32, w: f32, h: f32) -> bool {
    p[0] >= x && p[0] < x + w && p[1] >= y && p[1] < y + h
}

impl Menu {
    pub fn new(mission_names: Vec<String>, unlocked: usize, maps: Vec<PathBuf>, mut saves: Vec<PathBuf>, rules: Rules) -> Self {
        saves.sort_by_key(|p| std::cmp::Reverse(std::fs::metadata(p).and_then(|m| m.modified()).ok()));
        let mut m = Self {
            page: Page::Main,
            items: Vec::new(),
            hover: None,
            scroll: 0,
            mission_names,
            unlocked,
            maps,
            saves,
            selected_mission: None,
            rules,
            rules_panel: RulesPanel::default(),
            rules_changed: false,
            hover_back: false,
        };
        m.build();
        m
    }

    /// Opens a page by name (for scripted screenshots).
    pub fn open_page(&mut self, name: &str) {
        let page = match name {
            "campaign" => Page::Campaign,
            "custom" => Page::Custom,
            "load" => Page::Load,
            "rules" => Page::Rules,
            _ => Page::Main,
        };
        self.go(page);
    }

    pub fn show_campaign(&mut self) {
        self.go(Page::Campaign);
    }

    fn go(&mut self, page: Page) {
        self.page = page;
        self.build();
        self.scroll = if page == Page::Campaign {
            self.unlocked.saturating_sub(self.visible_rows() / 2)
        } else {
            0
        };
        self.clamp_scroll();
    }

    fn build(&mut self) {
        let go = |p| Action::Go(p);
        self.items = match self.page {
            Page::Main => {
                let mut v = Vec::new();
                if let Some(latest) = self.saves.first() {
                    let name = latest.file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned());
                    v.push(Item { label: format!("Continue: {name}"), enabled: true, action: Action::Choose(Choice::Save(latest.clone())) });
                }
                v.extend([
                    Item { label: "Campaign".into(), enabled: true, action: go(Page::Campaign) },
                    Item { label: "Custom missions".into(), enabled: !self.maps.is_empty(), action: go(Page::Custom) },
                    Item { label: "Load saved game".into(), enabled: !self.saves.is_empty(), action: go(Page::Load) },
                    Item { label: "Game rules".into(), enabled: true, action: go(Page::Rules) },
                    Item { label: "Quit".into(), enabled: true, action: Action::Choose(Choice::Quit) },
                ]);
                v
            }
            Page::Campaign => self
                .mission_names
                .iter()
                .enumerate()
                .map(|(i, name)| Item {
                    label: format!("{}. {}", i + 1, name),
                    enabled: i <= self.unlocked,
                    action: Action::Choose(Choice::Mission(i)),
                })
                .collect(),
            Page::Custom => Self::files(&self.maps, Choice::Map),
            Page::Load => Self::files(&self.saves, Choice::Save),
            Page::Rules => Vec::new(),
        };
        self.hover = None;
    }

    fn files(paths: &[PathBuf], choice: fn(PathBuf) -> Choice) -> Vec<Item> {
        paths
            .iter()
            .map(|p| Item {
                label: p.file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned()),
                enabled: true,
                action: Action::Choose(choice(p.clone())),
            })
            .collect()
    }

    fn visible_rows(&self) -> usize {
        LIST_ROWS
    }

    fn clamp_scroll(&mut self) {
        self.scroll = self.scroll.min(self.items.len().saturating_sub(self.visible_rows()));
    }

    /// Main page: button `i`'s top-left.
    fn main_button(screen: [f32; 2], i: usize) -> [f32; 2] {
        [(screen[0] / 2.0 - BUTTON_W / 2.0).floor(), (screen[1] / 2.0 - 100.0 + 40.0 * i as f32).floor()]
    }

    /// The campaign list panel and info area in screen space.
    fn plaque(screen: [f32; 2]) -> ([f32; 4], [f32; 4]) {
        let (o, s) = cover_screen(screen, [1024.0, 768.0]);
        let map = |r: [f32; 4]| [o[0] + r[0] * s, o[1] + r[1] * s, o[0] + r[2] * s, o[1] + r[3] * s];
        (map(PLAQUE_LIST), map(PLAQUE_INFO))
    }

    /// List pages other than the campaign: an outer panel in the middle of the screen.
    fn list_box(&self, screen: [f32; 2]) -> (f32, f32) {
        let rows = self.items.len().clamp(1, LIST_ROWS) as f32;
        let h = rows * ROW_H + 64.0 + 40.0;
        (((screen[0] - BOX_W) / 2.0).floor(), ((screen[1] - h) / 2.0).max(40.0).floor())
    }

    fn back_button(&self, screen: [f32; 2]) -> [f32; 2] {
        match self.page {
            Page::Campaign => {
                let (_, info) = Self::plaque(screen);
                [((info[0] + info[2]) / 2.0 - 80.0).floor(), (info[3] - BUTTON_H).floor()]
            }
            _ => {
                let (x, y) = self.list_box(screen);
                let rows = self.items.len().clamp(1, LIST_ROWS) as f32;
                [x + (BOX_W - 160.0) / 2.0, y + 44.0 + rows * ROW_H + 12.0]
            }
        }
    }

    fn item_at(&self, screen: [f32; 2], p: [f32; 2]) -> Option<usize> {
        let found = match self.page {
            Page::Main => (0..self.items.len()).find(|&i| {
                let [x, y] = Self::main_button(screen, i);
                inside(p, x, y, BUTTON_W, BUTTON_H)
            }),
            Page::Campaign => {
                let (list, _) = Self::plaque(screen);
                if p[0] < list[0] || p[0] > list[2] || p[1] < list[1] {
                    return None;
                }
                let row = ((p[1] - list[1] - 4.0) / ROW_H) as usize;
                let rows = ((list[3] - list[1] - 8.0) / ROW_H) as usize;
                (row < rows).then_some(row + self.scroll)
            }
            Page::Custom | Page::Load => {
                let (x, y) = self.list_box(screen);
                let top = y + 44.0;
                if p[0] < x + 16.0 || p[0] > x + BOX_W - 16.0 || p[1] < top {
                    return None;
                }
                let row = ((p[1] - top) / ROW_H) as usize;
                (row < self.visible_rows()).then_some(row + self.scroll)
            }
            Page::Rules => None,
        };
        found.filter(|&i| i < self.items.len())
    }

    pub fn hover(&mut self, screen: [f32; 2], p: [f32; 2]) {
        if self.page == Page::Rules {
            self.rules_panel.hover(screen, screen[0], p);
            return;
        }
        self.hover = self.item_at(screen, p);
        let [bx, by] = self.back_button(screen);
        self.hover_back = self.page != Page::Main && inside(p, bx, by, 160.0, BUTTON_H);
    }

    pub fn scroll(&mut self, lines: i32) {
        let max = self.items.len().saturating_sub(self.visible_rows()) as i32;
        self.scroll = (self.scroll as i32 + lines).clamp(0, max) as usize;
    }

    pub fn back(&mut self) {
        if self.page != Page::Main {
            self.go(Page::Main);
        }
    }

    pub fn click(&mut self, screen: [f32; 2], p: [f32; 2]) -> Option<Choice> {
        if self.page == Page::Rules {
            match self.rules_panel.click(&mut self.rules, screen, screen[0], p) {
                RulesClick::Toggled => self.rules_changed = true,
                RulesClick::Close | RulesClick::Outside => self.back(),
                RulesClick::Inside => {}
            }
            return None;
        }
        if self.page != Page::Main {
            let [bx, by] = self.back_button(screen);
            if inside(p, bx, by, 160.0, BUTTON_H) {
                self.back();
                return None;
            }
        }
        let i = self.item_at(screen, p)?;
        let item = self.items.get(i)?;
        if !item.enabled {
            return None;
        }
        match item.action.clone() {
            Action::Go(page) => {
                self.go(page);
                None
            }
            Action::Choose(c) => {
                if let Choice::Mission(m) = c {
                    self.selected_mission = Some(m);
                }
                Some(c)
            }
        }
    }

    fn background(r: &mut Renderer, image: u32) {
        let [sw, sh] = r.screen;
        r.rect([0.0, 0.0], [sw, sh], [0.0, 0.0, 0.0, 1.0], Space::Screen);
        let Some(rec) = r.record(image) else { return };
        let size = [rec.width as f32, rec.height as f32];
        let (o, s) = cover(r, image);
        r.image_scaled(image, o, [size[0] * s, size[1] * s], WHITE, Space::Screen);
    }

    #[allow(clippy::too_many_arguments)]
    fn button(r: &mut Renderer, panels: &PanelImages, label: &str, x: f32, y: f32, w: f32, focus: bool, enabled: bool) {
        panel::large_label(r, panels, x, y, (w / 16.0) as i32, (focus && enabled) as u32);
        let f = Font::NormalBlackOnLight;
        let tw = text_width(r, f, label) as f32;
        draw_text(r, f, label, x + ((w - tw) / 2.0).floor(), y + 6.0, font::BLACK);
        if !enabled {
            r.rect([x, y], [w, BUTTON_H], [0.0, 0.0, 0.0, 0.45], Space::Screen);
        }
    }

    pub fn draw(&self, r: &mut Renderer, panels: &PanelImages) {
        match self.page {
            Page::Main => self.draw_main(r, panels),
            Page::Campaign => self.draw_campaign(r, panels),
            Page::Custom | Page::Load => self.draw_list(r, panels),
            Page::Rules => {
                Self::background(r, BG_TITLE);
                self.rules_panel.draw(r, panels, &self.rules, r.screen[0], "These apply to every game you play.");
            }
        }
    }

    fn draw_main(&self, r: &mut Renderer, panels: &PanelImages) {
        Self::background(r, BG_TITLE);
        let [sw, sh] = r.screen;
        for (i, item) in self.items.iter().enumerate() {
            let [x, y] = Self::main_button(r.screen, i);
            Self::button(r, panels, &item.label, x, y, BUTTON_W, self.hover == Some(i), item.enabled);
        }
        let note = concat!("Osiris ", env!("CARGO_PKG_VERSION"), " - an open-source engine for Pharaoh");
        draw_text(r, Font::SmallPlain, note, 12.0, sh - 20.0, [0.8, 0.8, 0.8, 1.0]);
        let credit = "Game data (c) Sierra";
        let cw = text_width(r, Font::SmallPlain, credit) as f32;
        draw_text(r, Font::SmallPlain, credit, sw - cw - 12.0, sh - 20.0, [0.8, 0.8, 0.8, 1.0]);
    }

    fn draw_campaign(&self, r: &mut Renderer, panels: &PanelImages) {
        Self::background(r, BG_HISTORY);
        let (list, info) = Self::plaque(r.screen);
        let rows = ((list[3] - list[1] - 8.0) / ROW_H) as usize;
        r.set_clip(Some([list[0], list[1], list[2] - list[0], list[3] - list[1]]));
        for (row, i) in (self.scroll..self.items.len()).take(rows).enumerate() {
            let item = &self.items[i];
            let y = list[1] + 6.0 + row as f32 * ROW_H;
            if !item.enabled {
                draw_text_tinted(r, Font::NormalWhiteOnDark, &item.label, list[0] + 10.0, y, [0.45, 0.4, 0.35, 1.0]);
                continue;
            }
            let f = if self.hover == Some(i) { Font::NormalYellow } else { Font::NormalWhiteOnDark };
            draw_text(r, f, &item.label, list[0] + 10.0, y, font::WHITE);
            if i < self.unlocked {
                draw_text(r, f, "done", list[2] - 50.0, y, font::WHITE);
            }
        }
        r.set_clip(None);
        // Scroll hints.
        if self.scroll > 0 {
            draw_text(r, Font::NormalWhiteOnDark, "^", list[2] - 16.0, list[1] + 2.0, font::WHITE);
        }
        if self.scroll + rows < self.items.len() {
            draw_text(r, Font::NormalWhiteOnDark, "v", list[2] - 16.0, list[3] - 20.0, font::WHITE);
        }

        let title = "The Campaign";
        let tw = text_width(r, Font::LargeBlackOnLight, title) as f32;
        let cx = (info[0] + info[2]) / 2.0;
        draw_text(r, Font::LargeBlackOnLight, title, (cx - tw / 2.0).floor(), info[1], font::BLACK);
        let shown = self.hover.or(Some(self.unlocked.min(self.items.len().saturating_sub(1))));
        if let Some(i) = shown
            && let Some(name) = self.mission_names.get(i)
        {
            let status = if i < self.unlocked {
                "Completed. Click to play it again."
            } else if i == self.unlocked {
                "Your next mission. Click to begin."
            } else {
                "Complete the missions before this one first."
            };
            let heading = format!("Mission {}", i + 1);
            let lines = [(Font::NormalBlackOnLight, heading.as_str()), (Font::NormalBlackOnLight, name.as_str()), (Font::SmallPlain, status)];
            let mut y = info[1] + 40.0;
            for (f, text) in lines {
                let w = text_width(r, f, text) as f32;
                draw_text(r, f, text, (cx - w / 2.0).floor(), y, font::BLACK);
                y += 22.0;
            }
        }
        let [bx, by] = self.back_button(r.screen);
        Self::button(r, panels, "Back", bx, by, 160.0, self.hover_back, true);
    }

    fn draw_list(&self, r: &mut Renderer, panels: &PanelImages) {
        Self::background(r, if self.page == Page::Custom { BG_CUSTOM } else { BG_CHOOSE_GAME });
        let (x, y) = self.list_box(r.screen);
        let rows = self.items.len().clamp(1, LIST_ROWS);
        let hb = ((rows as f32 * ROW_H + 64.0 + 40.0) / 16.0).ceil() as i32;
        panel::outer_panel(r, panels, x, y, (BOX_W / 16.0) as i32, hb);
        let title = if self.page == Page::Custom { "Custom missions" } else { "Load a saved game" };
        let tw = text_width(r, Font::LargeBlackOnLight, title) as f32;
        draw_text(r, Font::LargeBlackOnLight, title, x + (BOX_W - tw) / 2.0, y + 12.0, font::BLACK);
        panel::inner_panel(r, panels, x + 16.0, y + 40.0, (BOX_W / 16.0) as i32 - 2, ((rows as f32 * ROW_H + 8.0) / 16.0).ceil() as i32);
        for (row, i) in (self.scroll..self.items.len()).take(LIST_ROWS).enumerate() {
            let item = &self.items[i];
            let iy = y + 46.0 + row as f32 * ROW_H;
            let f = if self.hover == Some(i) { Font::NormalYellow } else { Font::NormalWhiteOnDark };
            let lw = text_width(r, f, &item.label) as f32;
            draw_text(r, f, &item.label, x + (BOX_W - lw) / 2.0, iy, font::WHITE);
        }
        let [bx, by] = self.back_button(r.screen);
        Self::button(r, panels, "Back", bx, by, 160.0, self.hover_back, true);
    }
}
