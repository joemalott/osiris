//! The front end: main menu, campaign mission list, custom maps, saved games and the
//! game rules, drawn over the original's background art.

use crate::rules_panel::{RulesClick, RulesPanel};
use osiris_render::{Renderer, Space, WHITE};
use osiris_sim::Rules;
use osiris_ui::{Font, PanelImages, draw_text, draw_text_tinted, font, panel, rich_text, text_width};
use std::path::PathBuf;

/// Background images in Pharaoh_Unloaded (global ids).
const BG_TITLE: u32 = 201;
const BG_CHOOSE_GAME: u32 = 656;
const BG_HISTORY: u32 = 658;
const BG_CUSTOM: u32 = 657;
/// The family registry's own background (group 29, "FE_Registry.BMP"); the "create a
/// family" page reuses `BG_CHOOSE_GAME`, as the original does.
const BG_REGISTRY: u32 = 654;
/// Size of the family pages' popups (a notice, or a delete confirmation).
const NOTICE_W: f32 = 340.0;
const NOTICE_H: f32 = 130.0;
const CONFIRM_W: f32 = 360.0;
const CONFIRM_H: f32 = 170.0;
/// The choice of city: the frame, the maps of Egypt (one per choice screen, 640x400,
/// shown at 192,144 in the frame) and the city marker (normal, hover, pressed).
const CHOICE_BACK: u32 = 492;
pub const CHOICE_MAPS: u32 = 493;
const CHOICE_MARKER: u32 = 502;
const CHOICE_MAP_AT: [f32; 2] = [192.0, 144.0];
const MARKER_R: f32 = 23.0;

/// The history plaque in `BG_HISTORY` (1024x768): the dark list panel on the right and
/// the sandstone area under the picture frame on the left.
const PLAQUE_LIST: [f32; 4] = [522.0, 204.0, 808.0, 584.0];
const PLAQUE_INFO: [f32; 4] = [226.0, 366.0, 496.0, 600.0];

/// The campaign as the menu shows it.
#[derive(Debug, Clone, Default)]
pub struct CampaignView {
    /// Missions the player may start, in the order played.
    pub playable: Vec<usize>,
    pub done: Vec<usize>,
    /// The choice of the next city, when one is waiting.
    pub choice: Option<ChoiceView>,
}

#[derive(Debug, Clone)]
pub struct ChoiceView {
    pub map: u32,
    pub title: String,
    pub prompt: String,
    pub points: Vec<ChoicePoint>,
}

/// A city to choose: its place on the map (the marker's centre) and its line.
#[derive(Debug, Clone)]
pub struct ChoicePoint {
    pub x: f32,
    pub y: f32,
    pub label: String,
    pub path: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Choice {
    Mission(usize),
    /// The city on this campaign path.
    Path(u32),
    Map(PathBuf),
    Save(PathBuf),
    /// A family chosen (or freshly created) on the registry page, to make active.
    Family(String),
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Main,
    Campaign,
    /// The map of Egypt with the cities to choose between.
    CityChoice,
    Custom,
    Load,
    Rules,
    /// The family registry: create, delete or choose a family.
    Family,
    /// Typing a new family's name.
    NewFamily,
}

/// The family pages' text, resolved from `Pharaoh_Text.eng` so they read like the
/// original's (group numbers are the original's own, found in its text table).
#[derive(Debug, Clone, Default)]
pub struct FamilyText {
    /// 292.3, the registry's title.
    pub registry_title: String,
    /// 292.0
    pub new_button: String,
    /// 292.1
    pub delete_button: String,
    /// 292.2
    pub proceed_button: String,
    /// 292.4
    pub back_button: String,
    /// 31.0, the "create a family" page's title.
    pub enter_name: String,
    /// 13.5, that page's commit button.
    pub continue_button: String,
    /// 12.0, that page's cancel button.
    pub cancel_button: String,
    /// 5.90
    pub delete_title: String,
    /// 5.91
    pub delete_body: String,
    /// 5.92
    pub exists_title: String,
    /// 5.93
    pub exists_body: String,
    /// 5.94
    pub none_title: String,
    /// 5.95
    pub none_body: String,
    /// 18.1
    pub yes: String,
    /// 18.0
    pub no: String,
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
    campaign: CampaignView,
    hover_point: Option<usize>,
    maps: Vec<PathBuf>,
    /// Newest first.
    saves: Vec<PathBuf>,
    pub selected_mission: Option<usize>,
    pub rules: Rules,
    rules_panel: RulesPanel,
    /// Set when the rules change, so the caller can store them.
    pub rules_changed: bool,
    hover_back: bool,
    /// The active family, whose name is used in messages; empty until one is chosen.
    pub family: String,
    /// Families found in the registry, refreshed whenever it's opened.
    families: Vec<String>,
    /// The registry row awaiting Delete or Proceed.
    family_selected: Option<usize>,
    /// The name being typed on the "create a family" page.
    new_family: String,
    /// An OK-only popup on the family pages: title and body.
    family_notice: Option<(String, String)>,
    /// A delete confirmation, naming the family that would be removed.
    family_confirm: Option<String>,
    /// The hovered button on the family pages: New/Continue is 0, Delete 1, Proceed
    /// 2; a popup's OK or Yes is 0, No is 1.
    family_hover: Option<u8>,
    family_text: FamilyText,
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

fn inside4(p: [f32; 2], r: [f32; 4]) -> bool {
    inside(p, r[0], r[1], r[2], r[3])
}

impl Menu {
    #[allow(clippy::too_many_arguments)]
    pub fn new(mission_names: Vec<String>, campaign: CampaignView, maps: Vec<PathBuf>, mut saves: Vec<PathBuf>, rules: Rules, family: String, family_text: FamilyText) -> Self {
        saves.sort_by_key(|p| std::cmp::Reverse(std::fs::metadata(p).and_then(|m| m.modified()).ok()));
        let mut m = Self {
            page: Page::Main,
            items: Vec::new(),
            hover: None,
            scroll: 0,
            mission_names,
            campaign,
            hover_point: None,
            maps,
            saves,
            selected_mission: None,
            rules,
            rules_panel: RulesPanel::default(),
            rules_changed: false,
            hover_back: false,
            family,
            families: Vec::new(),
            family_selected: None,
            new_family: String::new(),
            family_notice: None,
            family_confirm: None,
            family_hover: None,
            family_text,
        };
        m.build();
        m
    }

    /// Opens a page by name (for scripted screenshots).
    pub fn open_page(&mut self, name: &str) {
        // The registry's own popups, for screenshotting them too.
        if name == "family-confirm" {
            self.go(Page::Family);
            self.family_confirm = self.families.first().cloned();
            return;
        }
        if name == "family-notice" {
            self.go(Page::Family);
            self.family_notice = Some((self.family_text.none_title.clone(), self.family_text.none_body.clone()));
            return;
        }
        let page = match name {
            "campaign" => Page::Campaign,
            "choice" => Page::CityChoice,
            "custom" => Page::Custom,
            "load" => Page::Load,
            "rules" => Page::Rules,
            "family" => Page::Family,
            "newfamily" => Page::NewFamily,
            _ => Page::Main,
        };
        self.go(page);
    }

    /// Rereads the family registry and re-selects the active family's row, if any.
    fn refresh_families(&mut self) {
        self.families = crate::list_families();
        self.family_selected = self.families.iter().position(|f| f.eq_ignore_ascii_case(&self.family));
    }

    /// The campaign after a mission: the choice of the next city when one is waiting,
    /// otherwise the mission list.
    pub fn show_campaign(&mut self) {
        self.go(if self.campaign.choice.is_some() { Page::CityChoice } else { Page::Campaign });
    }

    fn go(&mut self, page: Page) {
        if page == Page::Family {
            self.refresh_families();
        }
        self.page = page;
        self.build();
        self.scroll = if page == Page::Campaign {
            self.items.len().saturating_sub(self.visible_rows() / 2)
        } else {
            0
        };
        self.clamp_scroll();
    }

    fn build(&mut self) {
        let go = |p| Action::Go(p);
        self.items = match self.page {
            Page::Main => {
                let mut v = vec![Item { label: format!("Family: {}", self.family), enabled: true, action: go(Page::Family) }];
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
            Page::Campaign => {
                let mut v: Vec<Item> = self
                    .campaign
                    .playable
                    .iter()
                    .map(|&m| Item {
                        label: format!("{}. {}", m + 1, self.mission_names.get(m).map_or("", |s| s.as_str())),
                        enabled: true,
                        action: Action::Choose(Choice::Mission(m)),
                    })
                    .collect();
                if let Some(c) = &self.campaign.choice {
                    v.push(Item { label: c.title.clone(), enabled: true, action: go(Page::CityChoice) });
                }
                v
            }
            Page::Custom => Self::files(&self.maps, Choice::Map),
            Page::Load => Self::files(&self.saves, Choice::Save),
            Page::Family => self
                .families
                .iter()
                .map(|name| Item { label: name.clone(), enabled: true, action: Action::Choose(Choice::Family(name.clone())) })
                .collect(),
            Page::Rules | Page::CityChoice | Page::NewFamily => Vec::new(),
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
    /// The family registry has an extra row of buttons (New/Delete/Proceed) above its
    /// Back button, so it reserves more height.
    fn list_box(&self, screen: [f32; 2]) -> (f32, f32) {
        let rows = self.items.len().clamp(1, LIST_ROWS) as f32;
        let extra = if self.page == Page::Family { 40.0 } else { 0.0 };
        let h = rows * ROW_H + 64.0 + 40.0 + extra;
        (((screen[0] - BOX_W) / 2.0).floor(), ((screen[1] - h) / 2.0).max(40.0).floor())
    }

    fn back_button(&self, screen: [f32; 2]) -> [f32; 2] {
        match self.page {
            Page::Campaign => {
                let (_, info) = Self::plaque(screen);
                [((info[0] + info[2]) / 2.0 - 80.0).floor(), (info[3] - BUTTON_H).floor()]
            }
            Page::Family => {
                let (x, y) = self.list_box(screen);
                let rows = self.items.len().clamp(1, LIST_ROWS) as f32;
                [x + (BOX_W - 160.0) / 2.0, y + 44.0 + rows * ROW_H + 12.0 + BUTTON_H + 12.0]
            }
            Page::NewFamily => {
                let [x, y, w, h] = self.new_family_box(screen);
                [x + w / 2.0 + 8.0, y + h - 40.0]
            }
            _ => {
                let (x, y) = self.list_box(screen);
                let rows = self.items.len().clamp(1, LIST_ROWS) as f32;
                [x + (BOX_W - 160.0) / 2.0, y + 44.0 + rows * ROW_H + 12.0]
            }
        }
    }

    /// The "create a family" page's panel: a fixed size, since it has no rows.
    fn new_family_box(&self, screen: [f32; 2]) -> [f32; 4] {
        let (w, h) = (BOX_W, 190.0);
        [((screen[0] - w) / 2.0).floor(), ((screen[1] - h) / 2.0).max(40.0).floor(), w, h]
    }

    /// The family registry's New/Delete/Proceed buttons, side by side above Back.
    fn family_buttons(&self, screen: [f32; 2]) -> [[f32; 4]; 3] {
        let (x, y) = self.list_box(screen);
        let rows = self.items.len().clamp(1, LIST_ROWS) as f32;
        let by = y + 44.0 + rows * ROW_H + 12.0;
        // Buttons are drawn in whole 16-pixel pieces.
        let w = 128.0;
        [[x + 4.0, by, w, BUTTON_H], [x + 8.0 + w, by, w, BUTTON_H], [x + 12.0 + 2.0 * w, by, w, BUTTON_H]]
    }

    /// The "create a family" page's commit button (its Back button reuses the generic
    /// `back_button`, next to it).
    fn new_family_ok_button(&self, screen: [f32; 2]) -> [f32; 4] {
        let [x, y, w, h] = self.new_family_box(screen);
        [x + w / 2.0 - 168.0, y + h - 40.0, 160.0, BUTTON_H]
    }

    /// A popup panel, centred on screen.
    fn popup_rect(screen: [f32; 2], w: f32, h: f32) -> [f32; 4] {
        [((screen[0] - w) / 2.0).floor(), ((screen[1] - h) / 2.0).floor(), w, h]
    }

    fn notice_ok_button(screen: [f32; 2]) -> [f32; 4] {
        let [x, y, w, h] = Self::popup_rect(screen, NOTICE_W, NOTICE_H);
        [x + (w - 100.0) / 2.0, y + h - 40.0, 100.0, BUTTON_H]
    }

    fn confirm_buttons(screen: [f32; 2]) -> ([f32; 4], [f32; 4]) {
        let [x, y, w, h] = Self::popup_rect(screen, CONFIRM_W, CONFIRM_H);
        let by = y + h - 40.0;
        ([x + w / 2.0 - 108.0, by, 100.0, BUTTON_H], [x + w / 2.0 + 8.0, by, 100.0, BUTTON_H])
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
            Page::Custom | Page::Load | Page::Family => {
                let (x, y) = self.list_box(screen);
                let top = y + 44.0;
                if p[0] < x + 16.0 || p[0] > x + BOX_W - 16.0 || p[1] < top {
                    return None;
                }
                let row = ((p[1] - top) / ROW_H) as usize;
                (row < self.visible_rows()).then_some(row + self.scroll)
            }
            Page::Rules | Page::CityChoice | Page::NewFamily => None,
        };
        found.filter(|&i| i < self.items.len())
    }

    pub fn hover(&mut self, screen: [f32; 2], p: [f32; 2]) {
        if self.page == Page::Rules {
            self.rules_panel.hover(screen, screen[0], p);
            return;
        }
        if self.family_notice.is_some() {
            self.family_hover = inside4(p, Self::notice_ok_button(screen)).then_some(0);
            return;
        }
        if self.family_confirm.is_some() {
            let (yes, no) = Self::confirm_buttons(screen);
            self.family_hover = if inside4(p, yes) { Some(0) } else if inside4(p, no) { Some(1) } else { None };
            return;
        }
        self.hover = self.item_at(screen, p);
        self.hover_point = self.point_at(screen, p);
        let [bx, by] = self.back_button(screen);
        self.hover_back = !matches!(self.page, Page::Main | Page::CityChoice) && inside(p, bx, by, 160.0, BUTTON_H);
        self.family_hover = match self.page {
            Page::Family => self.family_buttons(screen).iter().position(|&r| inside4(p, r)).map(|i| i as u8),
            Page::NewFamily => inside4(p, self.new_family_ok_button(screen)).then_some(0),
            _ => None,
        };
    }

    pub fn scroll(&mut self, lines: i32) {
        let max = self.items.len().saturating_sub(self.visible_rows()) as i32;
        self.scroll = (self.scroll as i32 + lines).clamp(0, max) as usize;
    }

    pub fn back(&mut self) {
        if self.family_notice.take().is_some() || self.family_confirm.take().is_some() {
            return;
        }
        match self.page {
            Page::CityChoice => self.go(Page::Campaign),
            Page::Main => {}
            // Nothing to fall back to until a family exists: the registry is the
            // only page reachable, and it must stay so.
            Page::Family if self.family.is_empty() => {}
            Page::NewFamily => self.go(Page::Family),
            _ => self.go(Page::Main),
        }
    }

    /// The choice screen's frame: its offset and scale on screen.
    fn choice_frame(screen: [f32; 2]) -> ([f32; 2], f32) {
        cover_screen(screen, [1024.0, 768.0])
    }

    /// The city marker under `p` on the choice screen.
    fn point_at(&self, screen: [f32; 2], p: [f32; 2]) -> Option<usize> {
        if self.page != Page::CityChoice {
            return None;
        }
        let c = self.campaign.choice.as_ref()?;
        let (o, s) = Self::choice_frame(screen);
        c.points.iter().position(|pt| {
            let (cx, cy) = (o[0] + (CHOICE_MAP_AT[0] + pt.x) * s, o[1] + (CHOICE_MAP_AT[1] + pt.y) * s);
            (p[0] - cx).powi(2) + (p[1] - cy).powi(2) <= (MARKER_R * s).powi(2)
        })
    }

    /// Where the active family's name is written, read-only, on the campaign page.
    fn name_rect(screen: [f32; 2]) -> [f32; 4] {
        let (_, info) = Self::plaque(screen);
        [info[0], info[3] - 60.0, info[2] - info[0], 24.0]
    }

    /// Whether the window should forward typed text to [`Self::type_family_name`].
    pub fn wants_text(&self) -> bool {
        self.page == Page::NewFamily
    }

    /// Typing the new family's name: a character, backspace, or Enter to create it.
    pub fn type_family_name(&mut self, text: &str) {
        if self.family_notice.is_some() {
            return; // a popup must be dismissed with a click first
        }
        for c in text.chars() {
            match c {
                '\u{8}' | '\u{7f}' => {
                    self.new_family.pop();
                }
                '\r' | '\n' => {
                    self.commit_new_family();
                    return;
                }
                c if !c.is_control() && self.new_family.chars().count() < 31 => self.new_family.push(c),
                _ => {}
            }
        }
    }

    /// Creates the typed family, after checking it is non-empty and unique, and
    /// returns to the registry with it selected.
    fn commit_new_family(&mut self) {
        let name = self.new_family.trim().to_owned();
        if name.is_empty() {
            return;
        }
        // Compared as folder names would collide (sanitized), not just as typed: two
        // names differing only in punctuation must not silently share a family.
        if self.families.iter().any(|f| crate::sanitize(f).eq_ignore_ascii_case(&crate::sanitize(&name))) {
            self.family_notice = Some((self.family_text.exists_title.clone(), self.family_text.exists_body.clone()));
            return;
        }
        crate::create_family(&name);
        self.new_family.clear();
        self.go(Page::Family);
        self.family_selected = self.families.iter().position(|f| f.eq_ignore_ascii_case(&name));
    }

    /// A click on the family registry: a row selects it; New opens the "create a
    /// family" page; Delete and Proceed act on the selected row, or complain that
    /// none is selected; Back leaves (once a family is already active).
    fn click_family(&mut self, screen: [f32; 2], p: [f32; 2]) -> Option<Choice> {
        if self.family_notice.is_some() {
            if inside4(p, Self::notice_ok_button(screen)) {
                self.family_notice = None;
            }
            return None;
        }
        if let Some(name) = self.family_confirm.clone() {
            let (yes, no) = Self::confirm_buttons(screen);
            if inside4(p, yes) {
                crate::delete_family(&name);
                if self.family.eq_ignore_ascii_case(&name) {
                    self.family.clear();
                }
                self.family_confirm = None;
                self.go(Page::Family);
            } else if inside4(p, no) {
                self.family_confirm = None;
            }
            return None;
        }
        let [new_r, delete_r, proceed_r] = self.family_buttons(screen);
        if inside4(p, new_r) {
            self.go(Page::NewFamily);
            return None;
        }
        let selected = self.family_selected.and_then(|i| self.families.get(i)).cloned();
        if inside4(p, delete_r) {
            match selected {
                Some(name) => self.family_confirm = Some(name),
                None => self.family_notice = Some((self.family_text.none_title.clone(), self.family_text.none_body.clone())),
            }
            return None;
        }
        if inside4(p, proceed_r) {
            return match selected {
                Some(name) => Some(Choice::Family(name)),
                None => {
                    self.family_notice = Some((self.family_text.none_title.clone(), self.family_text.none_body.clone()));
                    None
                }
            };
        }
        let [bx, by] = self.back_button(screen);
        if inside(p, bx, by, 160.0, BUTTON_H) {
            self.back();
            return None;
        }
        if let Some(i) = self.item_at(screen, p) {
            self.family_selected = Some(i);
        }
        None
    }

    /// A click on the "create a family" page: the commit button (validated in
    /// [`Self::commit_new_family`]) or Back, cancelling.
    fn click_new_family(&mut self, screen: [f32; 2], p: [f32; 2]) -> Option<Choice> {
        if self.family_notice.is_some() {
            if inside4(p, Self::notice_ok_button(screen)) {
                self.family_notice = None;
            }
            return None;
        }
        if inside4(p, self.new_family_ok_button(screen)) {
            self.commit_new_family();
            return None;
        }
        let [bx, by] = self.back_button(screen);
        if inside(p, bx, by, 160.0, BUTTON_H) {
            self.back();
        }
        None
    }

    pub fn click(&mut self, screen: [f32; 2], p: [f32; 2]) -> Option<Choice> {
        if self.page == Page::Family {
            return self.click_family(screen, p);
        }
        if self.page == Page::NewFamily {
            return self.click_new_family(screen, p);
        }
        if self.page == Page::CityChoice {
            let i = self.point_at(screen, p)?;
            return self.campaign.choice.as_ref().and_then(|c| c.points.get(i)).map(|pt| Choice::Path(pt.path));
        }
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
            Page::CityChoice => self.draw_choice(r),
            Page::Custom | Page::Load => self.draw_list(r, panels),
            Page::Rules => {
                Self::background(r, BG_TITLE);
                self.rules_panel.draw(r, panels, &self.rules, r.screen[0], "These apply to every game you play.");
            }
            Page::Family => self.draw_family(r, panels),
            Page::NewFamily => self.draw_new_family(r, panels),
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
            if matches!(item.action, Action::Choose(Choice::Mission(m)) if self.campaign.done.contains(&m)) {
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
        let shown = self.hover.or(self.items.len().checked_sub(1));
        let about = shown.and_then(|i| self.items.get(i)).map(|item| match item.action {
            Action::Choose(Choice::Mission(m)) => {
                let status = if self.campaign.done.contains(&m) { "Completed. Click to play it again." } else { "Your next mission. Click to begin." };
                (format!("Mission {}", m + 1), self.mission_names.get(m).cloned().unwrap_or_default(), status.to_string())
            }
            _ => {
                let c = self.campaign.choice.as_ref();
                ("Next".to_string(), item.label.clone(), c.map_or_else(String::new, |c| c.prompt.clone()))
            }
        });
        if let Some((heading, name, status)) = &about {
            let lines = [(Font::NormalBlackOnLight, heading.as_str()), (Font::NormalBlackOnLight, name.as_str()), (Font::SmallPlain, status.as_str())];
            let mut y = info[1] + 40.0;
            for (f, text) in lines {
                let w = text_width(r, f, text) as f32;
                draw_text(r, f, text, (cx - w / 2.0).floor(), y, font::BLACK);
                y += 22.0;
            }
        }
        // The governor, whose name the messages use: the active family, read-only
        // here (switch families from the main menu instead).
        let [nx, ny, nw, _] = Self::name_rect(r.screen);
        let label = format!("Governor: {}", self.family);
        let w = text_width(r, Font::NormalBlackOnLight, &label) as f32;
        draw_text(r, Font::NormalBlackOnLight, &label, (nx + (nw - w) / 2.0).floor(), ny, font::BLACK);
        let [bx, by] = self.back_button(r.screen);
        Self::button(r, panels, "Back", bx, by, 160.0, self.hover_back, true);
    }

    /// The map of Egypt with a marker on each city to choose from; the period's title
    /// below, and the city under the mouse (or the prompt to choose one).
    fn draw_choice(&self, r: &mut Renderer) {
        let [sw, sh] = r.screen;
        r.rect([0.0, 0.0], [sw, sh], [0.0, 0.0, 0.0, 1.0], Space::Screen);
        let Some(c) = &self.campaign.choice else { return };
        let (o, s) = Self::choice_frame(r.screen);
        r.image_scaled(CHOICE_BACK, o, [1024.0 * s, 768.0 * s], WHITE, Space::Screen);
        let at = [o[0] + CHOICE_MAP_AT[0] * s, o[1] + CHOICE_MAP_AT[1] * s];
        r.image_scaled(c.map, at, [640.0 * s, 400.0 * s], WHITE, Space::Screen);
        for (i, pt) in c.points.iter().enumerate() {
            let image = CHOICE_MARKER + (self.hover_point == Some(i)) as u32;
            let (cx, cy) = (at[0] + pt.x * s, at[1] + pt.y * s);
            r.image_scaled(image, [cx - MARKER_R * s, cy - MARKER_R * s], [46.0 * s, 46.0 * s], WHITE, Space::Screen);
        }
        draw_text(r, Font::LargeBlackOnLight, &c.title, (o[0] + 204.0 * s).floor(), (o[1] + 550.0 * s).floor(), font::BLACK);
        let line = self.hover_point.and_then(|i| c.points.get(i)).map_or(c.prompt.as_str(), |pt| pt.label.as_str());
        draw_text(r, Font::NormalBlackOnLight, line, (o[0] + 214.0 * s).floor(), (o[1] + 584.0 * s).floor(), font::BLACK);
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
        // Scroll hints (the mouse wheel scrolls the list).
        if self.scroll > 0 {
            draw_text(r, Font::NormalWhiteOnDark, "^", x + BOX_W - 40.0, y + 46.0, font::WHITE);
        }
        if self.scroll + LIST_ROWS < self.items.len() {
            let last = y + 46.0 + (LIST_ROWS - 1) as f32 * ROW_H;
            draw_text(r, Font::NormalWhiteOnDark, "v", x + BOX_W - 40.0, last, font::WHITE);
            let more = format!("{} more", self.items.len() - self.scroll - LIST_ROWS);
            let mw = text_width(r, Font::SmallPlain, &more) as f32;
            draw_text(r, Font::SmallPlain, &more, x + BOX_W - 46.0 - mw, last + 3.0, font::WHITE);
        }
        let [bx, by] = self.back_button(r.screen);
        Self::button(r, panels, "Back", bx, by, 160.0, self.hover_back, true);
    }

    /// The family registry: a list of existing families, with New/Delete/Proceed
    /// above the usual Back button.
    fn draw_family(&self, r: &mut Renderer, panels: &PanelImages) {
        Self::background(r, BG_REGISTRY);
        let (x, y) = self.list_box(r.screen);
        let rows = self.items.len().clamp(1, LIST_ROWS);
        let hb = ((rows as f32 * ROW_H + 64.0 + 40.0 + 40.0) / 16.0).ceil() as i32;
        panel::outer_panel(r, panels, x, y, (BOX_W / 16.0) as i32, hb);
        let title = &self.family_text.registry_title;
        let tw = text_width(r, Font::LargeBlackOnLight, title) as f32;
        draw_text(r, Font::LargeBlackOnLight, title, x + (BOX_W - tw) / 2.0, y + 12.0, font::BLACK);
        panel::inner_panel(r, panels, x + 16.0, y + 40.0, (BOX_W / 16.0) as i32 - 2, ((rows as f32 * ROW_H + 8.0) / 16.0).ceil() as i32);
        for (row, i) in (self.scroll..self.items.len()).take(LIST_ROWS).enumerate() {
            let item = &self.items[i];
            let iy = y + 46.0 + row as f32 * ROW_H;
            let f = if self.family_selected == Some(i) || self.hover == Some(i) { Font::NormalYellow } else { Font::NormalWhiteOnDark };
            let lw = text_width(r, f, &item.label) as f32;
            draw_text(r, f, &item.label, x + (BOX_W - lw) / 2.0, iy, font::WHITE);
        }
        if self.families.is_empty() {
            let empty = "No families yet - create one below.";
            let ew = text_width(r, Font::NormalWhiteOnDark, empty) as f32;
            draw_text(r, Font::NormalWhiteOnDark, empty, x + (BOX_W - ew) / 2.0, y + 46.0, font::WHITE);
        }
        let [new_r, delete_r, proceed_r] = self.family_buttons(r.screen);
        let labels = [&self.family_text.new_button, &self.family_text.delete_button, &self.family_text.proceed_button];
        for (i, (r_, label)) in [new_r, delete_r, proceed_r].into_iter().zip(labels).enumerate() {
            Self::button(r, panels, label, r_[0], r_[1], r_[2], self.family_hover == Some(i as u8), true);
        }
        let [bx, by] = self.back_button(r.screen);
        Self::button(r, panels, &self.family_text.back_button, bx, by, 160.0, self.hover_back, !self.family.is_empty());
        if let Some((title, body)) = &self.family_notice {
            self.draw_notice(r, panels, title, body);
        } else if self.family_confirm.is_some() {
            self.draw_confirm(r, panels);
        }
    }

    /// The "create a family" page: a title, a text box and Continue/Back buttons.
    fn draw_new_family(&self, r: &mut Renderer, panels: &PanelImages) {
        Self::background(r, BG_CHOOSE_GAME);
        let [x, y, w, h] = self.new_family_box(r.screen);
        panel::outer_panel(r, panels, x, y, (w / 16.0) as i32, (h / 16.0).ceil() as i32);
        let title = &self.family_text.enter_name;
        let tw = text_width(r, Font::LargeBlackOnLight, title) as f32;
        draw_text(r, Font::LargeBlackOnLight, title, x + (w - tw) / 2.0, y + 14.0, font::BLACK);
        panel::inner_panel(r, panels, x + 16.0, y + 48.0, (w / 16.0) as i32 - 2, 2);
        let shown = format!("{}_", self.new_family);
        draw_text(r, Font::NormalWhiteOnDark, &shown, x + 24.0, y + 54.0, font::WHITE);
        let ok = self.new_family_ok_button(r.screen);
        Self::button(r, panels, &self.family_text.continue_button, ok[0], ok[1], ok[2], self.family_hover == Some(0), true);
        let [bx, by] = self.back_button(r.screen);
        Self::button(r, panels, &self.family_text.cancel_button, bx, by, 160.0, self.hover_back, true);
        if let Some((title, body)) = &self.family_notice {
            self.draw_notice(r, panels, title, body);
        }
    }

    /// Wraps `text` to `width` px in the popups' body font.
    fn wrap(r: &Renderer, text: &str, width: f32) -> rich_text::Layout {
        let opts = rich_text::Options { font: Font::NormalBlackOnLight, width: width as i32, paragraph_indent: 0 };
        rich_text::layout(text, &opts, &mut rich_text::RendererMeasure::new(r))
    }

    /// An OK-only popup: a title and a short wrapped message.
    fn draw_notice(&self, r: &mut Renderer, panels: &PanelImages, title: &str, body: &str) {
        let screen = r.screen;
        r.rect([0.0, 0.0], screen, [0.0, 0.0, 0.0, 0.5], Space::Screen);
        let [x, y, w, h] = Self::popup_rect(screen, NOTICE_W, NOTICE_H);
        panel::outer_panel(r, panels, x, y, (w / 16.0) as i32, (h / 16.0).ceil() as i32);
        let tw = text_width(r, Font::LargeBlackOnLight, title) as f32;
        draw_text(r, Font::LargeBlackOnLight, title, x + (w - tw) / 2.0, y + 10.0, font::BLACK);
        let layout = Self::wrap(r, body, w - 32.0);
        r.set_clip(Some([x + 16.0, y + 38.0, w - 32.0, h - 78.0]));
        rich_text::draw(r, &layout, [x + 16.0, y + 38.0], h - 78.0, 0.0, font::BLACK);
        r.set_clip(None);
        let ok = Self::notice_ok_button(screen);
        Self::button(r, panels, "OK", ok[0], ok[1], ok[2], self.family_hover == Some(0), true);
    }

    /// The delete-family Yes/No confirmation.
    fn draw_confirm(&self, r: &mut Renderer, panels: &PanelImages) {
        let Some(name) = &self.family_confirm else { return };
        let screen = r.screen;
        r.rect([0.0, 0.0], screen, [0.0, 0.0, 0.0, 0.5], Space::Screen);
        let [x, y, w, h] = Self::popup_rect(screen, CONFIRM_W, CONFIRM_H);
        panel::outer_panel(r, panels, x, y, (w / 16.0) as i32, (h / 16.0).ceil() as i32);
        let title = &self.family_text.delete_title;
        let tw = text_width(r, Font::LargeBlackOnLight, title) as f32;
        draw_text(r, Font::LargeBlackOnLight, title, x + (w - tw) / 2.0, y + 10.0, font::BLACK);
        let body = format!("{} ({name})", self.family_text.delete_body);
        let layout = Self::wrap(r, &body, w - 32.0);
        r.set_clip(Some([x + 16.0, y + 38.0, w - 32.0, h - 78.0]));
        rich_text::draw(r, &layout, [x + 16.0, y + 38.0], h - 78.0, 0.0, font::BLACK);
        r.set_clip(None);
        let (yes, no) = Self::confirm_buttons(screen);
        Self::button(r, panels, &self.family_text.yes, yes[0], yes[1], yes[2], self.family_hover == Some(0), true);
        Self::button(r, panels, &self.family_text.no, no[0], no[1], no[2], self.family_hover == Some(1), true);
    }
}
