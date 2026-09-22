//! The front end: main menu, campaign mission list, custom maps and saved games,
//! drawn over the original's background art.

use osiris_render::{Renderer, Space, WHITE};
use osiris_ui::{Font, PanelImages, draw_text, font, panel, text_width};
use std::path::PathBuf;

/// Background images in Pharaoh_Unloaded (global ids).
const BG_CHOOSE_GAME: u32 = 656;
const BG_CAMPAIGN: u32 = 655;
const BG_CUSTOM: u32 = 657;

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
    saves: Vec<PathBuf>,
    pub selected_mission: Option<usize>,
}

const ROWS: usize = 16;
const ROW_H: f32 = 22.0;
const BOX_W: f32 = 360.0;

impl Menu {
    pub fn new(mission_names: Vec<String>, unlocked: usize, maps: Vec<PathBuf>, saves: Vec<PathBuf>) -> Self {
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
        };
        m.build();
        m
    }

    pub fn show_campaign(&mut self) {
        self.page = Page::Campaign;
        self.scroll = self.unlocked.saturating_sub(ROWS / 2);
        self.build();
    }

    fn build(&mut self) {
        let go = |p| Action::Go(p);
        self.items = match self.page {
            Page::Main => vec![
                Item { label: "Campaign".into(), enabled: true, action: go(Page::Campaign) },
                Item { label: "Custom map".into(), enabled: !self.maps.is_empty(), action: go(Page::Custom) },
                Item { label: "Load saved game".into(), enabled: !self.saves.is_empty(), action: go(Page::Load) },
                Item { label: "Quit".into(), enabled: true, action: Action::Choose(Choice::Quit) },
            ],
            Page::Campaign => {
                let mut v: Vec<Item> = self
                    .mission_names
                    .iter()
                    .enumerate()
                    .map(|(i, name)| Item {
                        label: format!("{}. {}", i + 1, name),
                        enabled: i <= self.unlocked,
                        action: Action::Choose(Choice::Mission(i)),
                    })
                    .collect();
                v.push(Item { label: "Back".into(), enabled: true, action: go(Page::Main) });
                v
            }
            Page::Custom => {
                let mut v: Vec<Item> = self
                    .maps
                    .iter()
                    .map(|p| Item {
                        label: p.file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned()),
                        enabled: true,
                        action: Action::Choose(Choice::Map(p.clone())),
                    })
                    .collect();
                v.push(Item { label: "Back".into(), enabled: true, action: go(Page::Main) });
                v
            }
            Page::Load => {
                let mut v: Vec<Item> = self
                    .saves
                    .iter()
                    .map(|p| Item {
                        label: p.file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned()),
                        enabled: true,
                        action: Action::Choose(Choice::Save(p.clone())),
                    })
                    .collect();
                v.push(Item { label: "Back".into(), enabled: true, action: go(Page::Main) });
                v
            }
        };
        self.scroll = self.scroll.min(self.items.len().saturating_sub(ROWS));
        self.hover = None;
    }

    fn layout(&self, screen: [f32; 2]) -> (f32, f32) {
        let rows = self.items.len().min(ROWS) as f32;
        let h = rows * ROW_H + 64.0;
        ((screen[0] - BOX_W) / 2.0, ((screen[1] - h) / 2.0).max(40.0))
    }

    fn item_at(&self, screen: [f32; 2], p: [f32; 2]) -> Option<usize> {
        let (x, y) = self.layout(screen);
        let top = y + 40.0;
        if p[0] < x + 16.0 || p[0] > x + BOX_W - 16.0 || p[1] < top {
            return None;
        }
        let row = ((p[1] - top) / ROW_H) as usize;
        (row < self.items.len().min(ROWS)).then_some(row + self.scroll).filter(|&i| i < self.items.len())
    }

    pub fn hover(&mut self, screen: [f32; 2], p: [f32; 2]) {
        self.hover = self.item_at(screen, p);
    }

    pub fn scroll(&mut self, lines: i32) {
        let max = self.items.len().saturating_sub(ROWS) as i32;
        self.scroll = (self.scroll as i32 + lines).clamp(0, max) as usize;
    }

    pub fn back(&mut self) {
        if self.page != Page::Main {
            self.page = Page::Main;
            self.scroll = 0;
            self.build();
        }
    }

    pub fn click(&mut self, screen: [f32; 2], p: [f32; 2]) -> Option<Choice> {
        let i = self.item_at(screen, p)?;
        let item = self.items.get(i)?;
        if !item.enabled {
            return None;
        }
        match item.action.clone() {
            Action::Go(page) => {
                self.page = page;
                self.scroll = if page == Page::Campaign { self.unlocked.saturating_sub(ROWS / 2) } else { 0 };
                self.build();
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

    pub fn draw(&self, r: &mut Renderer, panels: &PanelImages) {
        let [sw, sh] = r.screen;
        let bg = match self.page {
            Page::Main | Page::Load => BG_CHOOSE_GAME,
            Page::Campaign => BG_CAMPAIGN,
            Page::Custom => BG_CUSTOM,
        };
        r.rect([0.0, 0.0], [sw, sh], [0.0, 0.0, 0.0, 1.0], Space::Screen);
        if let Some(rec) = r.record(bg) {
            let (w, h) = (rec.width as f32, rec.height as f32);
            let s = (sw / w).max(sh / h);
            r.image_scaled(bg, [(sw - w * s) / 2.0, (sh - h * s) / 2.0], [w * s, h * s], WHITE, Space::Screen);
        }
        let (x, y) = self.layout(r.screen);
        let rows = self.items.len().min(ROWS);
        let hb = ((rows as f32 * ROW_H + 64.0) / 16.0).ceil() as i32;
        panel::outer_panel(r, panels, x, y, (BOX_W / 16.0) as i32, hb);
        let title = match self.page {
            Page::Main => "Osiris",
            Page::Campaign => "Choose a mission",
            Page::Custom => "Choose a map",
            Page::Load => "Load a saved game",
        };
        let tw = text_width(r, Font::LargeBlackOnLight, title) as f32;
        draw_text(r, Font::LargeBlackOnLight, title, x + (BOX_W - tw) / 2.0, y + 12.0, font::BLACK);
        for (row, i) in (self.scroll..self.items.len()).take(ROWS).enumerate() {
            let item = &self.items[i];
            let iy = y + 40.0 + row as f32 * ROW_H;
            let focus = self.hover == Some(i) && item.enabled;
            panel::label(r, panels, x + 16.0, iy, ((BOX_W - 32.0) / 16.0) as i32, focus as u32);
            let f = if !item.enabled {
                Font::NormalBlackOnDark
            } else if focus {
                Font::NormalYellow
            } else {
                Font::NormalWhiteOnDark
            };
            let lw = text_width(r, f, &item.label) as f32;
            draw_text(r, f, &item.label, x + (BOX_W - lw) / 2.0, iy + 4.0, font::WHITE);
        }
        if self.page == Page::Main {
            let note = "An open-source engine for Pharaoh. Your original game data is required.";
            let nw = text_width(r, Font::SmallPlain, note) as f32;
            draw_text(r, Font::SmallPlain, note, (sw - nw) / 2.0, sh - 24.0, font::WHITE);
        }
    }
}
