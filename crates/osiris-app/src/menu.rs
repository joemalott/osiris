//! The front end: main menu, campaign mission list, custom maps, saved games and the
//! game rules, drawn over the original's background art.

use crate::mission_brief::Brief;
use crate::rules_panel::{RulesClick, RulesPanel};
use osiris_formats::{MissionPak, Scenario, TextTable};
use osiris_render::{Renderer, Space, WHITE};
use osiris_sim::Rules;
use osiris_sim::ratings::MissionResult;
use osiris_ui::{Font, PanelImages, draw_text, font, panel, rich_text, text_width};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Background images in Pharaoh_Unloaded (global ids).
const BG_TITLE: u32 = 201;
const BG_CHOOSE_GAME: u32 = 656;
const BG_HISTORY: u32 = 658;
const BG_CUSTOM: u32 = 657;
/// The mission briefing's background (Pharaoh_Unloaded group 18, second image).
const BG_BRIEFING: u32 = 507;
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

/// Custom Missions and Explore History, in the 1024x768 background's coordinates
/// (the original's 640x480 window sits at 192,144 in it). The scenario list: its left
/// edge and row width, and the stone's track beside it.
const LIST_X: f32 = 216.0;
const LIST_W: f32 = 256.0;
const TRACK_X: f32 = 472.0;
const TRACK_W: f32 = 32.0;
/// The scroll arrows (Pharaoh_General group 96, frames 8 and 12) and the stone.
const ARROW_X: f32 = 474.0;
const ARROW_DOWN_Y: f32 = 579.0;
const ARROW_SIZE: [f32; 2] = [39.0, 26.0];
const STONE_X: f32 = 481.0;
/// The cancel icon (group 96, frame 4) that leaves, and the arrow (group 192) that
/// starts the mission.
const EXIT_BUTTON: [f32; 4] = [527.0, 584.0, 39.0, 27.0];
const PLAY_BUTTON: [f32; 4] = [792.0, 584.0, 27.0, 27.0];
/// Explore History's "Show Prior Results" button and its tabs.
const RESULTS_BUTTON: [f32; 4] = [542.0, 554.0, 250.0, 23.0];
const TAB_MISSIONS: [f32; 4] = [224.0, 352.0, 160.0, 25.0];
const TAB_CAMPAIGNS: [f32; 4] = [387.0, 352.0, 93.0, 25.0];
/// The difficulty's arrows, beside its line under the objectives.
const DIFFICULTY_UP: [f32; 4] = [560.0, 536.0, 17.0, 17.0];
const DIFFICULTY_DOWN: [f32; 4] = [577.0, 536.0, 17.0, 17.0];

/// The campaign window (FUN_0041be10, button table 0x57be78): the nine periods'
/// buttons, 144x25, Pharaoh's five above Cleopatra's four. The arrow that begins or
/// plays the period is `PLAY_BUTTON`, Explore History's exit is `EXIT_BUTTON`.
const PERIOD_BUTTONS: [[f32; 2]; 9] = [
    [212.0, 435.0],
    [212.0, 465.0],
    [364.0, 405.0],
    [364.0, 435.0],
    [364.0, 465.0],
    [212.0, 535.0],
    [212.0, 565.0],
    [364.0, 535.0],
    [364.0, 565.0],
];
const PERIOD_W: f32 = 144.0;
/// Each period's picture: Pharaoh's are the first five mission pictures, Cleopatra's
/// are these of the expansion's.
const PERIOD_PICTURES: [usize; 4] = [4, 2, 5, 7];

/// The mission briefing (FUN_0041a180): its panel, and the label slots its goals
/// fill (table 0x5797d4, relative to the panel).
const BRIEF_AT: [f32; 2] = [208.0, 160.0];
const GOAL_SLOTS: [[f32; 2]; 6] = [[32.0, 90.0], [288.0, 90.0], [32.0, 112.0], [288.0, 112.0], [32.0, 134.0], [288.0, 134.0]];
/// "To the city", the cancel back to the choice of city, and the difficulty's arrows.
const BRIEF_GO: [f32; 4] = [772.0, 570.0, 27.0, 27.0];
const BRIEF_BACK: [f32; 4] = [218.0, 572.0, 31.0, 20.0];
const BRIEF_UP: [f32; 4] = [318.0, 576.0, 17.0, 17.0];
const BRIEF_DOWN: [f32; 4] = [335.0, 576.0, 17.0, 17.0];
/// The briefing's text: where it is drawn and the band it is clipped to.
const BRIEF_TEXT: [f32; 4] = [240.0, 340.0, 528.0, 234.0];

/// The family's menu (FUN_004cb4d0) in its 640x480 art: the panel, and the first
/// button (225x25, one every 48 pixels).
const FAMILY_PANEL: [f32; 2] = [128.0, 40.0];
const FAMILY_BUTTON: [f32; 2] = [208.0, 128.0];
const FAMILY_BUTTON_W: f32 = 224.0;

/// The campaign as the menu shows it.
#[derive(Debug, Clone, Default)]
pub struct CampaignView {
    pub done: Vec<usize>,
    /// The choice of the next city, when one is waiting.
    pub choice: Option<ChoiceView>,
    /// The best result of each mission won.
    pub results: BTreeMap<usize, MissionResult>,
    /// The period the family history has reached; the only one it may begin.
    pub period: usize,
    /// The family has a city in play to resume.
    pub resume: bool,
}

/// A mission's briefing, shown before its city.
#[derive(Debug, Clone, Default)]
pub struct BriefingView {
    pub mission: usize,
    pub title: String,
    pub subtitle: String,
    pub content: String,
    pub brief: Brief,
    /// The tutorial's first goal, on the first five missions.
    pub tutorial: Option<String>,
    /// The mission was picked on the choice of city, which Cancel goes back to.
    pub back: bool,
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
    /// A mission picked on Explore History's list, to brief and play on its own.
    Mission(usize),
    /// The family history's city in play, loaded again.
    Resume,
    /// The family history goes on in the period it has reached.
    Begin,
    /// A whole period played from Explore History.
    Period(usize),
    /// The briefing is read: on to the mission's city.
    ToCity(usize),
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
    /// Explore History's list of individual missions.
    Campaign,
    /// The campaign window as "Begin Family History" opens it: only the period the
    /// family has reached can be begun.
    Periods,
    /// The same window as Explore History's Campaigns tab, where every period plays.
    HistoryPeriods,
    /// A mission's briefing.
    Briefing,
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
    text: Arc<TextTable>,
    data: PathBuf,
    cursor: [f32; 2],
    /// The row picked on Custom Missions or Explore History, and what the panel beside
    /// the list tells of its scenario.
    picked: Option<usize>,
    brief: Option<Brief>,
    /// Explore History shows the picked mission's prior results, not its objectives.
    show_results: bool,
    /// The list's stone is being dragged.
    dragging: bool,
    /// The difficulty new games start at; set when the arrows change it, so the caller
    /// can store it.
    pub difficulty: u8,
    pub difficulty_changed: bool,
    /// The period picked on the campaign window.
    period_sel: usize,
    /// The campaign window came up because a period was won: as in the original, it
    /// can then only be left by beginning the next.
    periods_locked: bool,
    briefing: Option<BriefingView>,
    /// How far the briefing's text is scrolled, in pixels.
    briefing_scroll: f32,
    /// "No Missions Won By Family": asked before Explore History opens for a family
    /// with no mission won.
    explore_confirm: bool,
    /// How far the briefing's text can scroll, found as it is drawn.
    briefing_max: std::cell::Cell<f32>,
}

const LIST_ROWS: usize = 16;
const ROW_H: f32 = 22.0;
const BOX_W: f32 = 400.0;
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

/// Whether art stretched `s` times lands on a fraction of a device pixel, where it is
/// better filtered smoothly than by nearest pixel.
fn fractional(r: &Renderer, s: f32) -> bool {
    let k = s * r.scale;
    (k - k.round()).abs() > 0.01
}

fn inside(p: [f32; 2], x: f32, y: f32, w: f32, h: f32) -> bool {
    p[0] >= x && p[0] < x + w && p[1] >= y && p[1] < y + h
}

fn inside4(p: [f32; 2], r: [f32; 4]) -> bool {
    inside(p, r[0], r[1], r[2], r[3])
}

/// A 1024x768 background scaled to cover the screen. Drawing happens in its
/// coordinates, where the original draws, with the renderer's screen frame set.
struct Frame {
    o: [f32; 2],
    s: f32,
}

impl Frame {
    fn new(screen: [f32; 2]) -> Self {
        Self::of(screen, [1024.0, 768.0])
    }

    /// Art of another size scaled to cover the screen.
    fn of(screen: [f32; 2], size: [f32; 2]) -> Self {
        let (o, s) = cover_screen(screen, size);
        Self { o, s }
    }

    /// A point on screen in the background's coordinates.
    fn to_bg(&self, p: [f32; 2]) -> [f32; 2] {
        [(p[0] - self.o[0]) / self.s, (p[1] - self.o[1]) / self.s]
    }
}

fn text_color(f: Font) -> [f32; 4] {
    if matches!(f, Font::SmallPlain | Font::NormalBlackOnLight | Font::LargeBlackOnLight) { font::BLACK } else { font::WHITE }
}

/// The original draws plain text three pixels above the y it is given, wrapped
/// descriptions included; `draw_text` does so, the rich-text layout doesn't.
const GLYPH_RISE: f32 = 3.0;

fn bg_text(r: &mut Renderer, f: Font, s: &str, x: f32, y: f32) {
    draw_text(r, f, s, x, y, text_color(f));
}

/// Text centred in a band `w` wide from `x`, as the original centres it: flush left
/// when it is wider.
fn bg_centred(r: &mut Renderer, f: Font, s: &str, x: f32, y: f32, w: f32) {
    let tw = text_width(r, f, s) as f32;
    let dx = ((w - tw) / 2.0).max(0.0).floor();
    draw_text(r, f, s, x + dx, y, text_color(f));
}

fn bg_wrapped(r: &mut Renderer, f: Font, s: &str, x: f32, y: f32, w: f32) {
    let opts = rich_text::Options { font: f, width: w as i32, paragraph_indent: 0 };
    let laid = rich_text::layout(s, &opts, &mut rich_text::RendererMeasure::new(r));
    rich_text::draw(r, &laid, [x, y - GLYPH_RISE], laid.height as f32, 0.0, text_color(f));
}

fn bg_image(r: &mut Renderer, id: u32, x: f32, y: f32) {
    r.image(id, [x, y], WHITE, Space::Screen);
}

impl Menu {
    #[allow(clippy::too_many_arguments)]
    pub fn new(mission_names: Vec<String>, campaign: CampaignView, maps: Vec<PathBuf>, mut saves: Vec<PathBuf>, rules: Rules, family: String, family_text: FamilyText, text: Arc<TextTable>, data: PathBuf) -> Self {
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
            text,
            data,
            cursor: [0.0, 0.0],
            picked: None,
            brief: None,
            show_results: false,
            dragging: false,
            difficulty: osiris_sim::difficulty::NORMAL,
            difficulty_changed: false,
            period_sel: 0,
            periods_locked: false,
            briefing: None,
            briefing_scroll: 0.0,
            explore_confirm: false,
            briefing_max: std::cell::Cell::new(0.0),
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
        if name == "explore-confirm" {
            self.go(Page::Main);
            self.explore_confirm = true;
            return;
        }
        let page = match name {
            "campaign" => Page::Campaign,
            "periods" => Page::Periods,
            "history" => Page::HistoryPeriods,
            "briefing" => Page::Briefing,
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

    /// The choice of the next city, waiting in the campaign being played.
    pub fn show_choice(&mut self) {
        self.go(if self.campaign.choice.is_some() { Page::CityChoice } else { Page::Main });
    }

    /// The campaign window with period `k` picked: from "Begin Family History", or,
    /// `locked`, because the period before it was just won.
    pub fn show_periods(&mut self, k: usize, locked: bool) {
        self.go(Page::Periods);
        self.period_sel = k.min(PERIOD_BUTTONS.len() - 1);
        self.periods_locked = locked;
    }

    /// Picks a period on the campaign window, for scripted screenshots.
    pub fn pick_period(&mut self, k: usize) {
        self.period_sel = k.min(PERIOD_BUTTONS.len() - 1);
    }

    /// A mission's briefing, before its city.
    pub fn show_briefing(&mut self, b: BriefingView) {
        self.briefing = Some(b);
        self.briefing_scroll = 0.0;
        self.go(Page::Briefing);
    }

    fn go(&mut self, page: Page) {
        if page == Page::Family {
            self.refresh_families();
        }
        if matches!(page, Page::Periods | Page::HistoryPeriods) && !matches!(self.page, Page::Periods | Page::HistoryPeriods) {
            self.period_sel = if page == Page::Periods { self.campaign.period.min(PERIOD_BUTTONS.len() - 1) } else { 0 };
        }
        self.periods_locked = false;
        self.explore_confirm = false;
        self.page = page;
        self.build();
        self.scroll = 0;
        self.clamp_scroll();
        self.dragging = false;
        // Both lists start on their first row.
        let first = match page {
            Page::Custom | Page::Campaign => (!self.items.is_empty()).then_some(0),
            _ => None,
        };
        self.picked = None;
        self.brief = None;
        if let Some(i) = first {
            self.pick(i);
        }
    }

    /// Picks row `i` of the list and reads its scenario for the panel.
    fn pick(&mut self, i: usize) {
        self.picked = Some(i);
        let Some(item) = self.items.get(i) else { return };
        self.brief = match &item.action {
            Action::Choose(Choice::Map(p)) => Scenario::load_map(p).ok().map(|s| Brief::new(item.label.clone(), &s)),
            Action::Choose(Choice::Mission(m)) => MissionPak::open(&self.data.join("mission1.pak")).ok().and_then(|pak| pak.scenario(*m).ok()).map(|s| Brief::new(item.label.clone(), &s)),
            _ => None,
        };
    }

    /// Picks a row by number, for scripted screenshots.
    pub fn pick_row(&mut self, i: usize) {
        if i < self.items.len() {
            self.pick(i);
            let rows = self.visible_rows();
            if i < self.scroll || i >= self.scroll + rows {
                self.scroll = i;
                self.clamp_scroll();
            }
        }
    }

    /// Shows the prior results on Explore History, for scripted screenshots; with
    /// `sample`, every mission won gets a made-up result first.
    pub fn show_prior_results(&mut self, sample: bool) {
        self.show_results = true;
        if sample {
            for &m in &self.campaign.done {
                let r = MissionResult { culture: 45, prosperity: 30, kingdom: 52, population: 1450 + 10 * m as i32, funds: 3200, months: 50, score: 6120, difficulty: 2 };
                self.campaign.results.insert(m, r);
            }
        }
    }

    fn build(&mut self) {
        let go = |p| Action::Go(p);
        self.items = match self.page {
            // The family's menu (text group 293): the history's button reads "Resume"
            // while a city of it is in play, and Load Saved Game shows only when there
            // are saves. Game rules and Quit are Osiris's own.
            Page::Main => {
                let t = |i: usize| self.text.get(293, i).unwrap_or("").trim().to_string();
                let history = if self.campaign.resume { Item { label: t(0), enabled: true, action: Action::Choose(Choice::Resume) } } else { Item { label: t(7), enabled: true, action: go(Page::Periods) } };
                vec![
                    history,
                    Item { label: t(1), enabled: true, action: go(Page::HistoryPeriods) },
                    Item { label: t(2), enabled: !self.saves.is_empty(), action: go(Page::Load) },
                    Item { label: t(3), enabled: !self.maps.is_empty(), action: go(Page::Custom) },
                    Item { label: t(4), enabled: true, action: go(Page::Family) },
                    Item { label: "Game rules".into(), enabled: true, action: go(Page::Rules) },
                    Item { label: "Quit".into(), enabled: true, action: Action::Choose(Choice::Quit) },
                ]
            }
            // Every mission of campaign.txt, each playable on its own, as the original
            // lists them (FUN_0041df70).
            Page::Campaign => self
                .mission_names
                .iter()
                .enumerate()
                .map(|(m, name)| Item { label: name.clone(), enabled: true, action: Action::Choose(Choice::Mission(m)) })
                .collect(),
            Page::Custom => Self::files(&self.maps, Choice::Map),
            Page::Load => Self::files(&self.saves, Choice::Save),
            Page::Family => self
                .families
                .iter()
                .map(|name| Item { label: name.clone(), enabled: true, action: Action::Choose(Choice::Family(name.clone())) })
                .collect(),
            Page::Rules | Page::CityChoice | Page::NewFamily | Page::Periods | Page::HistoryPeriods | Page::Briefing => Vec::new(),
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
        match self.page {
            Page::Custom => 15,
            Page::Campaign => 13,
            _ => LIST_ROWS,
        }
    }

    /// The top of the Custom Missions or Explore History list, and the up arrow's.
    fn list_top(&self) -> (f32, f32) {
        if self.page == Page::Campaign { (396.0, 391.0) } else { (364.0, 359.0) }
    }

    /// How far the stone travels.
    fn stone_range(&self) -> f32 {
        (self.visible_rows() * 16) as f32 - 76.0
    }

    /// Scrolls to put the stone at `y` (in the background's coordinates).
    fn drag_stone(&mut self, y: f32) {
        let range = self.stone_range();
        let (top, _) = self.list_top();
        let t = (y - top - 25.0).clamp(0.0, range);
        let max = self.items.len().saturating_sub(self.visible_rows());
        let pct = (t * 100.0 / range) as usize;
        self.scroll = max * pct / 100;
    }

    fn clamp_scroll(&mut self) {
        self.scroll = self.scroll.min(self.items.len().saturating_sub(self.visible_rows()));
    }

    /// Main page: button `i`, in its 640x480 art's coordinates.
    fn main_button(i: usize) -> [f32; 4] {
        [FAMILY_BUTTON[0], FAMILY_BUTTON[1] + 48.0 * i as f32, FAMILY_BUTTON_W, BUTTON_H]
    }

    fn main_frame(screen: [f32; 2]) -> Frame {
        Frame::of(screen, [640.0, 480.0])
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
            Page::Main => {
                let b = Self::main_frame(screen).to_bg(p);
                (0..self.items.len()).find(|&i| self.items[i].enabled && inside4(b, Self::main_button(i)))
            }
            Page::Campaign | Page::Custom => {
                let b = Frame::new(screen).to_bg(p);
                let (top, _) = self.list_top();
                if b[0] < LIST_X || b[0] >= LIST_X + LIST_W || b[1] < top {
                    return None;
                }
                let row = ((b[1] - top) / 16.0) as usize;
                (row < self.visible_rows()).then_some(row + self.scroll)
            }
            Page::Load | Page::Family => {
                let (x, y) = self.list_box(screen);
                let top = y + 44.0;
                if p[0] < x + 16.0 || p[0] > x + BOX_W - 16.0 || p[1] < top {
                    return None;
                }
                let row = ((p[1] - top) / ROW_H) as usize;
                (row < self.visible_rows()).then_some(row + self.scroll)
            }
            Page::Rules | Page::CityChoice | Page::NewFamily | Page::Periods | Page::HistoryPeriods | Page::Briefing => None,
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
        if self.family_confirm.is_some() || self.explore_confirm {
            let (yes, no) = Self::confirm_buttons(screen);
            self.family_hover = if inside4(p, yes) { Some(0) } else if inside4(p, no) { Some(1) } else { None };
            return;
        }
        self.cursor = p;
        if self.dragging {
            self.drag_stone(Frame::new(screen).to_bg(p)[1]);
        }
        self.hover = self.item_at(screen, p);
        self.hover_point = self.point_at(screen, p);
        let [bx, by] = self.back_button(screen);
        self.hover_back = !matches!(self.page, Page::Main | Page::CityChoice | Page::Custom | Page::Campaign | Page::Periods | Page::HistoryPeriods | Page::Briefing) && inside(p, bx, by, 160.0, BUTTON_H);
        self.family_hover = match self.page {
            Page::Family => self.family_buttons(screen).iter().position(|&r| inside4(p, r)).map(|i| i as u8),
            Page::NewFamily => inside4(p, self.new_family_ok_button(screen)).then_some(0),
            _ => None,
        };
    }

    /// The left button came up: the stone is let go.
    pub fn release(&mut self) {
        self.dragging = false;
    }

    pub fn scroll(&mut self, lines: i32) {
        if self.page == Page::Briefing {
            self.briefing_scroll = (self.briefing_scroll + 11.0 * lines as f32).clamp(0.0, self.briefing_max.get());
            return;
        }
        let max = self.items.len().saturating_sub(self.visible_rows()) as i32;
        self.scroll = (self.scroll as i32 + lines).clamp(0, max) as usize;
    }

    pub fn back(&mut self) {
        if self.family_notice.take().is_some() || self.family_confirm.take().is_some() || std::mem::take(&mut self.explore_confirm) {
            return;
        }
        match self.page {
            Page::Main => {}
            Page::Periods if self.periods_locked => {}
            Page::Briefing if self.briefing.as_ref().is_some_and(|b| b.back) => self.go(Page::CityChoice),
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
        if self.explore_confirm {
            let (yes, no) = Self::confirm_buttons(screen);
            if inside4(p, yes) {
                self.go(Page::HistoryPeriods);
            } else if inside4(p, no) {
                self.explore_confirm = false;
            }
            return None;
        }
        if matches!(self.page, Page::Periods | Page::HistoryPeriods) {
            return self.click_periods(screen, p);
        }
        if self.page == Page::Briefing {
            return self.click_briefing(screen, p);
        }
        if self.page == Page::Family {
            return self.click_family(screen, p);
        }
        if self.page == Page::NewFamily {
            return self.click_new_family(screen, p);
        }
        if matches!(self.page, Page::Custom | Page::Campaign) {
            return self.click_scenarios(screen, p);
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
            // "Choose a Mission" asks first when the family has won nothing yet
            // (text 5.141-142).
            Action::Go(Page::HistoryPeriods) if self.campaign.done.is_empty() && self.campaign.results.is_empty() => {
                self.explore_confirm = true;
                None
            }
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

    /// A click on the campaign window: a period's button picks it (a locked one too,
    /// to read why it is locked), the arrow begins or plays the picked period, and on
    /// Explore History the exit and the Individual Missions tab.
    fn click_periods(&mut self, screen: [f32; 2], p: [f32; 2]) -> Option<Choice> {
        let b = Frame::new(screen).to_bg(p);
        let explore = self.page == Page::HistoryPeriods;
        if let Some(k) = PERIOD_BUTTONS.iter().position(|&[x, y]| inside(b, x, y, PERIOD_W, BUTTON_H)) {
            self.period_sel = k;
            return None;
        }
        if explore && inside4(b, TAB_MISSIONS) {
            self.go(Page::Campaign);
            return None;
        }
        if explore && inside4(b, EXIT_BUTTON) {
            self.go(Page::Main);
            return None;
        }
        if inside4(b, PLAY_BUTTON) {
            if explore {
                return Some(Choice::Period(self.period_sel));
            }
            if self.period_sel == self.campaign.period {
                return Some(Choice::Begin);
            }
        }
        None
    }

    /// A click on the briefing: on to the city, back to the choice of city, or the
    /// difficulty's arrows.
    fn click_briefing(&mut self, screen: [f32; 2], p: [f32; 2]) -> Option<Choice> {
        let b = Frame::new(screen).to_bg(p);
        let brief = self.briefing.as_ref()?;
        if inside4(b, BRIEF_GO) {
            return Some(Choice::ToCity(brief.mission));
        }
        if brief.back && inside4(b, BRIEF_BACK) {
            self.go(Page::CityChoice);
            return None;
        }
        let d = self.difficulty;
        let want = if inside4(b, BRIEF_UP) { (d + 1).min(osiris_sim::difficulty::IMPOSSIBLE) } else if inside4(b, BRIEF_DOWN) { d.saturating_sub(1) } else { d };
        if want != d {
            self.difficulty = want;
            self.difficulty_changed = true;
        }
        None
    }

    /// A click on Custom Missions or Explore History: the exit and start buttons, the
    /// results toggle, the scroll arrows and stone, or a row, which is picked (the
    /// choice of the next city opens straight away).
    fn click_scenarios(&mut self, screen: [f32; 2], p: [f32; 2]) -> Option<Choice> {
        let b = Frame::new(screen).to_bg(p);
        let at = |r: [f32; 4]| inside4(b, r);
        if at(EXIT_BUTTON) {
            self.back();
            return None;
        }
        if at(PLAY_BUTTON) {
            let item = self.items.get(self.picked?)?;
            let Action::Choose(c) = item.action.clone() else { return None };
            if let Choice::Mission(m) = c {
                self.selected_mission = Some(m);
            }
            return Some(c);
        }
        // The arrows are there whenever the objectives are.
        if self.brief.is_some() && !(self.page == Page::Campaign && self.show_results) {
            let d = self.difficulty;
            let want = if at(DIFFICULTY_UP) { (d + 1).min(osiris_sim::difficulty::IMPOSSIBLE) } else if at(DIFFICULTY_DOWN) { d.saturating_sub(1) } else { d };
            if want != d {
                self.difficulty = want;
                self.difficulty_changed = true;
                return None;
            }
        }
        if self.page == Page::Campaign && at(TAB_CAMPAIGNS) {
            self.go(Page::HistoryPeriods);
            return None;
        }
        if self.page == Page::Campaign && at(RESULTS_BUTTON) {
            self.show_results = !self.show_results;
            return None;
        }
        let (top, up_y) = self.list_top();
        if at([ARROW_X, up_y, ARROW_SIZE[0], ARROW_SIZE[1]]) {
            self.scroll(-1);
            return None;
        }
        if at([ARROW_X, ARROW_DOWN_Y, ARROW_SIZE[0], ARROW_SIZE[1]]) {
            self.scroll(1);
            return None;
        }
        let range = self.stone_range();
        if self.items.len() > self.visible_rows() && b[0] >= TRACK_X && b[0] < TRACK_X + TRACK_W && b[1] >= top + 25.0 && b[1] <= top + 50.0 + range {
            self.dragging = true;
            self.drag_stone(b[1]);
            return None;
        }
        let i = self.item_at(screen, p)?;
        match self.items.get(i)?.action.clone() {
            Action::Go(page) => self.go(page),
            Action::Choose(_) => self.pick(i),
        }
        None
    }

    fn background(r: &mut Renderer, image: u32) {
        let [sw, sh] = r.screen;
        r.rect([0.0, 0.0], [sw, sh], [0.0, 0.0, 0.0, 1.0], Space::Screen);
        let Some(rec) = r.record(image) else { return };
        let size = [rec.width as f32, rec.height as f32];
        let (o, s) = cover(r, image);
        r.smooth = fractional(r, s);
        r.image_scaled(image, o, [size[0] * s, size[1] * s], WHITE, Space::Screen);
        r.smooth = false;
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
            Page::Campaign | Page::Custom => self.draw_scenarios(r, panels),
            Page::Periods | Page::HistoryPeriods => self.draw_framed(r, panels, BG_HISTORY, Self::draw_periods),
            Page::Briefing => self.draw_framed(r, panels, BG_BRIEFING, Self::draw_briefing),
            Page::CityChoice => self.draw_choice(r),
            Page::Load => self.draw_list(r, panels),
            Page::Rules => {
                Self::background(r, BG_TITLE);
                self.rules_panel.draw(r, panels, &self.rules, r.screen[0], "These apply to every game you play.");
            }
            Page::Family => self.draw_family(r, panels),
            Page::NewFamily => self.draw_new_family(r, panels),
        }
    }

    /// The family's menu: its title and buttons on the FE_ChooseGame art, drawn in that
    /// art's 640x480 coordinates.
    fn draw_main(&self, r: &mut Renderer, panels: &PanelImages) {
        Self::background(r, BG_CHOOSE_GAME);
        let f = Self::main_frame(r.screen);
        r.screen_frame = Some((f.o, f.s));
        r.smooth = fractional(r, f.s);
        // The original's panel holds its five buttons; it is drawn taller here for
        // Osiris's two more.
        let rows = ((FAMILY_BUTTON[1] + 48.0 * self.items.len() as f32 - FAMILY_PANEL[1]) / 16.0).ceil() as i32;
        panel::outer_panel(r, panels, FAMILY_PANEL[0], FAMILY_PANEL[1], 24, rows.max(21));
        let title = self.text.get(293, 5).unwrap_or("").trim().replace("[player_name]", &self.family);
        bg_centred(r, Font::LargeBlackOnLight, &title, 140.0, 60.0, 368.0);
        for (i, item) in self.items.iter().enumerate() {
            if !item.enabled {
                continue;
            }
            let [x, y, w, _] = Self::main_button(i);
            panel::large_label(r, panels, x, y, (w / 16.0) as i32, (self.hover == Some(i)) as u32);
            bg_centred(r, Font::NormalBlackOnLight, &item.label, x + 4.0, y + 7.0, w);
        }
        r.screen_frame = None;
        r.smooth = false;
        let [sw, sh] = r.screen;
        let note = concat!("Osiris ", env!("CARGO_PKG_VERSION"), " - an open-source engine for Pharaoh");
        draw_text(r, Font::SmallPlain, note, 12.0, sh - 20.0, [0.8, 0.8, 0.8, 1.0]);
        let credit = "Game data (c) Sierra";
        let cw = text_width(r, Font::SmallPlain, credit) as f32;
        draw_text(r, Font::SmallPlain, credit, sw - cw - 12.0, sh - 20.0, [0.8, 0.8, 0.8, 1.0]);
        if self.explore_confirm {
            let t = |i: usize| self.text.get(5, i).unwrap_or("").trim().to_string();
            self.draw_yes_no(r, panels, &t(141), &t(142));
        }
    }

    /// A 1024x768 background with a page drawn over it in its coordinates.
    fn draw_framed(&self, r: &mut Renderer, panels: &PanelImages, bg: u32, page: fn(&Self, &mut Renderer, &PanelImages, &Frame)) {
        Self::background(r, bg);
        let f = Frame::new(r.screen);
        r.screen_frame = Some((f.o, f.s));
        r.smooth = fractional(r, f.s);
        page(self, r, panels, &f);
        r.set_clip(None);
        r.screen_frame = None;
        r.smooth = false;
    }

    /// Explore History's two tabs, the one showing pressed in.
    fn draw_tabs(&self, r: &mut Renderer, panels: &PanelImages, cursor: [f32; 2], campaigns: bool) {
        let t = |i: usize| self.text.get(294, i).unwrap_or("").trim().to_string();
        for (tab, label, on) in [(TAB_MISSIONS, t(38), !campaigns), (TAB_CAMPAIGNS, t(39), campaigns)] {
            panel::button_border(r, panels, tab[0], tab[1], tab[2] as i32, tab[3] as i32, on || inside4(cursor, tab));
            bg_centred(r, Font::NormalBlackOnLight, &label, tab[0] + 4.0, tab[1] + 7.0, tab[2]);
        }
    }

    /// The campaign window (FUN_0041be10). "Begin Family History" shows the period the
    /// family has reached as the only one to begin, earlier ones with their short
    /// account and later ones with why they must wait (text 294, four lines a period);
    /// Explore History's Campaigns tab lets every period play.
    fn draw_periods(&self, r: &mut Renderer, panels: &PanelImages, f: &Frame) {
        let explore = self.page == Page::HistoryPeriods;
        let t = |g: usize, i: usize| self.text.get(g, i).unwrap_or("").trim().to_string();
        let cursor = f.to_bg(self.cursor);
        let k = self.period_sel;
        let current = self.campaign.period;
        let picture = match k {
            0..5 => r.library.group_id("Pharaoh_Unloaded", 28, k),
            _ => r.library.group_id("Expansion", 38, PERIOD_PICTURES[k - 5]),
        };
        if let Ok(id) = picture {
            bg_image(r, id, 270.0, if explore { 200.0 } else { 204.0 });
        }
        let title = t(293, if explore { 6 } else { 5 }).replace("[player_name]", &self.family);
        bg_centred(r, Font::LargeBlackOnLight, &title, 212.0, 161.0, 600.0);
        bg_centred(r, Font::LargeBlackOnDark, &t(294, 4 * k), 531.0, 204.0, 283.0);
        let line = if explore || k == current { 2 } else if k < current { 1 } else { 3 };
        bg_wrapped(r, Font::NormalWhiteOnDark, &t(294, 4 * k + line), 539.0, 260.0, 269.0);

        bg_text(r, Font::NormalBlackOnLight, &t(294, 41), 222.0, 410.0);
        bg_text(r, Font::NormalBlackOnLight, &t(294, 42), 222.0, 510.0);
        for (i, &[x, y]) in PERIOD_BUTTONS.iter().enumerate() {
            let enabled = explore || i == current;
            let lit = enabled && inside(cursor, x, y, PERIOD_W, BUTTON_H);
            panel::large_label(r, panels, x, y, (PERIOD_W / 16.0) as i32, lit as u32);
            let font = if enabled { Font::NormalBlackOnLight } else { Font::NormalBlue };
            bg_centred(r, font, &t(27, i), x, y + 6.0, PERIOD_W);
        }
        if explore {
            self.draw_tabs(r, panels, cursor, true);
            bg_text(r, Font::NormalBlackOnLight, &t(294, 37), 742.0, 589.0);
            bg_text(r, Font::NormalBlackOnLight, &t(44, 217), 572.0, 589.0);
            if let Ok(cancel) = r.library.group_id("Pharaoh_General", 96, 4) {
                bg_image(r, cancel + inside4(cursor, EXIT_BUTTON) as u32, EXIT_BUTTON[0], EXIT_BUTTON[1]);
            }
        } else if k == current {
            bg_text(r, Font::NormalBlackOnLight, &t(294, 36), 612.0, 589.0);
        }
        if (explore || k == current)
            && let Ok(go) = r.library.group_id("Pharaoh_General", 192, 0)
        {
            bg_image(r, go + inside4(cursor, PLAY_BUTTON) as u32, PLAY_BUTTON[0], PLAY_BUTTON[1]);
        }
    }

    /// The mission briefing (FUN_0041a180): the briefing's title and subtitle, the
    /// objectives as labels, the briefing itself below, and at the bottom the
    /// difficulty and the way to the city.
    fn draw_briefing(&self, r: &mut Renderer, panels: &PanelImages, f: &Frame) {
        let Some(b) = &self.briefing else { return };
        let t = |g: usize, i: usize| self.text.get(g, i).unwrap_or("").trim().to_string();
        let cursor = f.to_bg(self.cursor);
        let [px, py] = BRIEF_AT;
        panel::outer_panel(r, panels, px, py, 38, 28);
        bg_text(r, Font::LargeBlackOnLight, &b.title, px + 16.0, py + 16.0);
        bg_text(r, Font::NormalBlackOnLight, &b.subtitle, px + 16.0, py + 46.0);

        panel::inner_panel(r, panels, px + 16.0, py + 64.0, 36, 6);
        bg_text(r, Font::NormalYellow, &t(62, 10), px + 32.0, py + 72.0);
        let w = &b.brief.win;
        let mut goals = Vec::new();
        if w.population.enabled {
            goals.push(format!("{} {}", t(62, 11), w.population.value));
        }
        if w.housing_count.value != 0 {
            goals.push(format!("{} {}", w.housing_count.value, t(29, w.housing_level.value.max(0) as usize + 20)));
        }
        for (g, id) in [(&w.culture, 12), (&w.prosperity, 13), (&w.monuments, 14), (&w.kingdom, 15)] {
            if g.enabled {
                goals.push(format!("{} {}", t(62, id), g.value));
            }
        }
        for (line, [x, y]) in goals.iter().zip(GOAL_SLOTS) {
            panel::label(r, panels, px + x, py + y, 15, 0);
            bg_text(r, Font::NormalBlue, line, px + x + 8.0, py + y + 3.0);
        }
        if let Some(line) = &b.tutorial {
            let [x, y] = GOAL_SLOTS[4];
            panel::label(r, panels, px + x, py + y, 34, 0);
            bg_text(r, Font::NormalBlue, line, px + x + 8.0, py + y + 3.0);
        }

        panel::inner_panel(r, panels, px + 16.0, py + 168.0, 34, 15);
        let [tx, ty, tw, th] = BRIEF_TEXT;
        let opts = rich_text::Options { font: Font::NormalWhiteOnDark, width: tw as i32, paragraph_indent: 0 };
        let laid = rich_text::layout(&b.content, &opts, &mut rich_text::RendererMeasure::new(r));
        let max = (laid.height as f32 - th).max(0.0);
        self.briefing_max.set(max);
        let scroll = self.briefing_scroll.min(max);
        r.set_clip(Some([tx - 16.0, py + 171.0, tw + 32.0, th]));
        rich_text::draw(r, &laid, [tx, ty - GLYPH_RISE - scroll], laid.height as f32, 0.0, text_color(Font::NormalWhiteOnDark));
        r.set_clip(None);
        if max > 0.0 {
            panel::inner_panel(r, panels, px + 557.0, py + 192.0, 2, 12);
        }

        bg_text(r, Font::NormalBlackOnLight, &format!("{} {}", t(44, 216), t(153, self.difficulty as usize + 1)), px + 150.0, py + 417.0);
        for (group, rect) in [(212, BRIEF_UP), (16, BRIEF_DOWN)] {
            if let Ok(id) = r.library.group_id("Pharaoh_General", group, 0) {
                bg_image(r, id + inside4(cursor, rect) as u32, rect[0], rect[1]);
            }
        }
        if b.back {
            bg_text(r, Font::NormalBlackOnLight, &t(13, 4), px + 50.0, py + 419.0);
            if let Ok(id) = r.library.group_id("Pharaoh_General", 90, 8) {
                bg_image(r, id + inside4(cursor, BRIEF_BACK) as u32, BRIEF_BACK[0], BRIEF_BACK[1]);
            }
        }
        bg_text(r, Font::NormalBlackOnLight, &t(62, 7), px + 476.0, py + 417.0);
        if let Ok(go) = r.library.group_id("Pharaoh_General", 192, 0) {
            bg_image(r, go + inside4(cursor, BRIEF_GO) as u32, BRIEF_GO[0], BRIEF_GO[1]);
        }
    }

    /// Custom Missions and Explore History, drawn as the original's one window: the
    /// list of scenarios on the left with its scroll bar, the picked scenario's picture
    /// above it, and its details on the dark panel to the right.
    fn draw_scenarios(&self, r: &mut Renderer, panels: &PanelImages) {
        Self::background(r, if self.page == Page::Campaign { BG_HISTORY } else { BG_CUSTOM });
        let f = Frame::new(r.screen);
        r.screen_frame = Some((f.o, f.s));
        r.smooth = fractional(r, f.s);
        self.draw_scenarios_framed(r, panels, &f);
        r.set_clip(None);
        r.screen_frame = None;
        r.smooth = false;
    }

    fn draw_scenarios_framed(&self, r: &mut Renderer, panels: &PanelImages, f: &Frame) {
        let history = self.page == Page::Campaign;
        let t = |g: usize, i: usize| self.text.get(g, i).unwrap_or("").trim().to_string();
        let cursor = f.to_bg(self.cursor);
        if history {
            bg_centred(r, Font::LargeBlackOnLight, &t(293, 6), 212.0, 161.0, 600.0);
            self.draw_tabs(r, panels, cursor, false);
        }

        let rows = self.visible_rows();
        let (top, up_y) = self.list_top();
        panel::inner_panel(r, panels, LIST_X - 9.0, top - 15.0, 16, rows as i32 + 1);
        r.set_clip(Some([LIST_X, top, LIST_W, rows as f32 * 16.0]));
        for (row, i) in (self.scroll..self.items.len()).take(rows).enumerate() {
            let lit = self.hover == Some(i) || (self.hover.is_none() && self.picked == Some(i));
            let font = if lit { Font::NormalYellow } else { Font::NormalWhiteOnDark };
            bg_text(r, font, &self.items[i].label, LIST_X, top + 16.0 * row as f32);
        }
        r.set_clip(None);
        panel::inner_panel(r, panels, TRACK_X + 6.0, top + 22.0, 2, rows as i32 - 3);
        if let Ok(arrows) = r.library.group_id("Pharaoh_General", 96, 8) {
            for (y, image) in [(up_y, arrows), (ARROW_DOWN_Y, arrows + 4)] {
                let over = inside4(cursor, [ARROW_X, y, ARROW_SIZE[0], ARROW_SIZE[1]]) as u32;
                bg_image(r, image + over, ARROW_X, y);
            }
        }
        if self.items.len() > rows {
            let max = self.items.len() - rows;
            let pct = if self.scroll == 0 { 0 } else if self.scroll < max { self.scroll * 100 / max } else { 100 };
            let y = top + 25.0 + (self.stone_range() as usize * pct / 100) as f32;
            bg_image(r, panels.panel_button + 39, STONE_X, y);
        }

        // The toggle's frame goes under the text, which may reach it.
        if history {
            let [x, y, w, h] = RESULTS_BUTTON;
            panel::button_border(r, panels, x, y, w as i32, h as i32, inside4(cursor, RESULTS_BUTTON));
        }
        if let Some(b) = &self.brief {
            self.draw_brief(r, f, b);
        }

        if history {
            let label = t(44, if self.show_results { 221 } else { 220 });
            bg_centred(r, Font::NormalWhiteOnDark, &label, 546.0, 558.0, 250.0);
            bg_text(r, Font::NormalBlackOnLight, &t(44, 217), 572.0, 590.0);
            bg_text(r, Font::NormalBlackOnLight, &t(44, 215), 652.0, 590.0);
        } else {
            bg_text(r, Font::NormalBlackOnLight, &t(44, 136), 697.0, 590.0);
        }
        if let Ok(cancel) = r.library.group_id("Pharaoh_General", 96, 4) {
            bg_image(r, cancel + inside4(cursor, EXIT_BUTTON) as u32, EXIT_BUTTON[0], EXIT_BUTTON[1]);
        }
        if let Ok(go) = r.library.group_id("Pharaoh_General", 192, 0) {
            bg_image(r, go + inside4(cursor, PLAY_BUTTON) as u32, PLAY_BUTTON[0], PLAY_BUTTON[1]);
        }
    }

    /// The picked scenario: picture, name, subtitle and start year, then either its
    /// objectives or, on Explore History, the family's best result in it.
    fn draw_brief(&self, r: &mut Renderer, f: &Frame, b: &Brief) {
        let history = self.page == Page::Campaign;
        let t = |g: usize, i: usize| self.text.get(g, i).unwrap_or("").trim().to_string();
        // Pictures 0-18 are Pharaoh's; Cleopatra's follow in the expansion's pack.
        let image = match b.image.max(0) as usize {
            n @ 0..19 => r.library.group_id("Pharaoh_Unloaded", 28, n),
            n => r.library.group_id("Expansion", 38, n - 19),
        };
        if let Ok(id) = image {
            bg_image(r, id, 270.0, if history { 200.0 } else { 180.0 });
        }
        let white = Font::NormalWhiteOnDark;
        bg_centred(r, white, &b.name, 527.0, 209.0, 260.0);
        bg_centred(r, Font::NormalYellow, &b.subtitle, 527.0, 229.0, 260.0);
        let year = if b.start_year < 0 { format!("{} {}", -b.start_year, t(20, 0)) } else { format!("{} {}", t(20, 1), b.start_year) };
        bg_text(r, white, &year, 602.0, 249.0);

        if history && self.show_results {
            let m = match self.picked.and_then(|i| self.items.get(i)).map(|i| &i.action) {
                Some(Action::Choose(Choice::Mission(m))) => *m,
                _ => return,
            };
            let Some(res) = self.campaign.results.get(&m) else {
                bg_wrapped(r, white, &t(305, 0), 537.0, 269.0, 260.0);
                return;
            };
            bg_wrapped(r, white, &t(297, m), 537.0, 269.0, 270.0);
            let w = &b.win;
            let lines = [
                (w.culture.enabled, 0, res.culture, 429.0),
                (w.prosperity.enabled, 1, res.prosperity, 445.0),
                (w.kingdom.enabled, 3, res.kingdom, 461.0),
                (w.population.enabled, 4, res.population, 477.0),
                (true, 5, res.funds, 493.0),
            ];
            for (shown, id, value, y) in lines {
                if shown {
                    bg_centred(r, white, &format!("{} {value}", t(298, id)), 537.0, y, 270.0);
                }
            }
            bg_centred(r, white, &format!("{} {}", t(298, 7), t(153, res.difficulty as usize + 1)), 527.0, 509.0, 270.0);
            bg_centred(r, white, &format!("{} {} {}", t(298, 6), res.months / 12, t(298, 9)), 537.0, 525.0, 270.0);
            bg_centred(r, Font::NormalYellow, &format!("{} {}", t(298, 8), res.score), 537.0, 541.0, 270.0);
            return;
        }

        bg_centred(r, white, &t(44, 77 + b.climate as usize), 527.0, 269.0, 260.0);
        bg_centred(r, white, &t(44, b.size_text()), 527.0, 289.0, 260.0);
        bg_centred(r, white, &t(44, b.military_text()), 527.0, 309.0, 260.0);
        bg_centred(r, white, &t(32, b.challenge_text()), 527.0, 329.0, 260.0);
        if b.open_play {
            bg_wrapped(r, white, &t(145, 0), 537.0, 369.0, 260.0);
            self.draw_difficulty(r, f);
            return;
        }
        bg_centred(r, Font::NormalYellow, &t(44, 127), 527.0, 361.0, 260.0);
        let w = &b.win;
        let goals = [
            (w.culture.enabled, w.culture.value, 129, 389.0),
            (w.prosperity.enabled, w.prosperity.value, 130, 405.0),
            (w.kingdom.enabled, w.kingdom.value, 132, 421.0),
            (w.population.enabled, w.population.value, 133, 437.0),
            (w.survival_time.enabled, w.survival_time.value, 135, 469.0),
            (w.time_limit.enabled, w.time_limit.value, 134, 469.0),
        ];
        for (on, value, id, y) in goals {
            if on {
                bg_text(r, white, &format!("{value} {}", t(44, id)), 602.0, y);
            }
        }
        let count = w.housing_count.value;
        if count != 0 {
            let level = w.housing_level.value.max(0) as usize + if count >= 2 { 20 } else { 0 };
            bg_text(r, white, &format!("{count} {}", t(29, level)), 602.0, 453.0);
        }
        // The monuments to build, by name; the monument goal's own number is never shown.
        for (i, &m) in b.monuments.iter().enumerate() {
            if m != 0 {
                bg_centred(r, white, &t(198, m as usize), 542.0, 485.0 + 16.0 * i as f32, 260.0);
            }
        }
        self.draw_difficulty(r, f);
    }

    /// The difficulty new games start at, with its arrows (Pharaoh_General groups 212
    /// and 16): up to the left, down to the right.
    fn draw_difficulty(&self, r: &mut Renderer, f: &Frame) {
        let t = |g: usize, i: usize| self.text.get(g, i).unwrap_or("").trim().to_string();
        let line = format!("{} {}", t(44, 216), t(153, self.difficulty as usize + 1));
        bg_text(r, Font::NormalWhiteOnDark, &line, 602.0, 536.0);
        let cursor = f.to_bg(self.cursor);
        for (group, rect) in [(212, DIFFICULTY_UP), (16, DIFFICULTY_DOWN)] {
            if let Ok(id) = r.library.group_id("Pharaoh_General", group, 0) {
                bg_image(r, id + inside4(cursor, rect) as u32, rect[0], rect[1]);
            }
        }
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
        let body = format!("{} ({name})", self.family_text.delete_body);
        self.draw_yes_no(r, panels, &self.family_text.delete_title, &body);
    }

    /// A Yes/No popup: a title and a short wrapped question.
    fn draw_yes_no(&self, r: &mut Renderer, panels: &PanelImages, title: &str, body: &str) {
        let screen = r.screen;
        r.rect([0.0, 0.0], screen, [0.0, 0.0, 0.0, 0.5], Space::Screen);
        let [x, y, w, h] = Self::popup_rect(screen, CONFIRM_W, CONFIRM_H);
        panel::outer_panel(r, panels, x, y, (w / 16.0) as i32, (h / 16.0).ceil() as i32);
        let tw = text_width(r, Font::LargeBlackOnLight, title) as f32;
        draw_text(r, Font::LargeBlackOnLight, title, x + (w - tw) / 2.0, y + 10.0, font::BLACK);
        let layout = Self::wrap(r, body, w - 32.0);
        r.set_clip(Some([x + 16.0, y + 38.0, w - 32.0, h - 78.0]));
        rich_text::draw(r, &layout, [x + 16.0, y + 38.0], h - 78.0, 0.0, font::BLACK);
        r.set_clip(None);
        let (yes, no) = Self::confirm_buttons(screen);
        Self::button(r, panels, &self.family_text.yes, yes[0], yes[1], yes[2], self.family_hover == Some(0), true);
        Self::button(r, panels, &self.family_text.no, no[0], no[1], no[2], self.family_hover == Some(1), true);
    }
}
