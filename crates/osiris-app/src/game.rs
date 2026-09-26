//! A running city: the world, the view onto it, the sidebar and the player's tool.

use crate::city_view::{self, CityView, Highlight, Overlay, Sprite};
use crate::message_list::MessageList;
use crate::overlay::{Overlay as View, OverlayImages};
use crate::top_menu::{MenuAction, TopMenu};
use crate::rules_panel::{RulesClick, RulesPanel};
use crate::sidebar::{self, Button, Category, Click, MenuItem, Sidebar, SidebarImages, SidebarState};
use crate::info::InfoPanel;
use crate::minimap::Minimap;
use osiris_audio::Audio;
use osiris_formats::{Message, MessageTable, TextTable};
use osiris_render::{Paint, Renderer};
use osiris_sim::buildings::kind;
use osiris_sim::placement::GhostImage;
use osiris_sim::{Command, Outcome, World};
use osiris_ui::dialog::MessageDialog;
use osiris_ui::{Font, draw_text, font};
use std::sync::Arc;

/// What a placement ghost was worked out for: the tool, where it goes, the statue
/// and gatehouse choices, and the day (the city under it may have changed since).
type GhostKey = (u16, i32, i32, u8, u8, u8, u8, u64);

/// Game speeds in percent. Up to 100% these follow the original's ladder; above it
/// they are multiples of normal speed, up to 200x.
pub const SPEEDS: [u32; 18] = [10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 200, 300, 500, 1000, 2000, 5000, 10000, 20000];

/// Milliseconds per simulation tick at game speeds 100%, 90%, ... 10%.
const MS_PER_TICK: [f32; 10] = [20.0, 35.0, 55.0, 80.0, 110.0, 160.0, 240.0, 350.0, 500.0, 700.0];

/// Longest the simulation may run in one frame, so fast speeds stay responsive.
const TICK_BUDGET_MS: f32 = 12.0;

/// How long after building the action can be undone, in ticks (two days).
const UNDO_TICKS: u64 = 100;

fn ms_per_tick(speed: u32) -> f32 {
    if speed <= 100 {
        MS_PER_TICK[((100 - speed.clamp(10, 100)) / 10) as usize]
    } else {
        MS_PER_TICK[0] * 100.0 / speed as f32
    }
}

pub fn speed_label(speed: u32) -> String {
    if speed <= 100 { format!("{speed}%") } else { format!("{}x", speed / 100) }
}

const CONTROLS: &str = "@PLeft-click a build button, then click or drag on the map to build. Right-click cancels the tool, closes windows, and drags to scroll. The arrow keys scroll the map and the mouse wheel zooms.@PP pauses. [ and ] (or Page Up and Page Down) change the speed, from 10% up to 200 times normal. - opens the Overseer of the Treasury and = the Chief Overseer.@PW, F and D show the water, fire and damage overlays; the Overlays menu has the rest. Space switches between the normal view and the last overlay.@PB builds roads and X clears land. M, N, U, O, T and G pick up a bazaar, granary, storage yard, apothecary, water supply and gardens; Ctrl+H housing, Ctrl+F firehouse and Ctrl+A architect.@PF2 opens the game rules, F5 saves and F9 loads the saved game for this city; the city is also saved each month while Autosave is on (Options menu), and Continue on the main menu picks up the latest save. Escape backs out of whatever is open, and when nothing is, asks whether to leave for the main menu.";

const ABOUT: &str = "@POsiris is an open-source engine for Pharaoh, written in Rust and released under the GNU GPL version 3.@PIt plays the original campaign using your own copy of the game data. Pharaoh and its art, music and text are the work of Impressions Games and Sierra.";

/// Text group with building and menu names, indexed by building type id.
const TEXT_BUILDING_NAMES: usize = 28;
/// Text group with short month names.
const TEXT_MONTHS: usize = 25;

/// Submenu keys and the building-type id whose name labels them.
const SUBMENUS: &[(&str, u16)] = &[
    ("farms", 2),
    ("raw_materials", 3),
    ("consturction_guilds", 4),
    ("monuments", 48),
    ("water_crossings", 52),
    ("forts", 57),
    ("beautification", 91),
    ("temples", 96),
    ("temple_complex", 97),
    ("shrines", 150),
    ("defenses", 176),
];

/// A build menu label in title case, as the original's menus are: each word
/// capitalized but for short joining words after the first.
fn title_case(s: &str) -> String {
    const SMALL: [&str; 6] = ["to", "of", "the", "and", "a", "on"];
    s.split(' ')
        .enumerate()
        .map(|(i, w)| {
            if i > 0 && SMALL.contains(&w) {
                return w.to_owned();
            }
            let mut c = w.chars();
            c.next().map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    None,
    Road,
    Clear,
    Build(u16),
}

/// What an entry of the open build menu does.
#[derive(Debug, Clone)]
enum Entry {
    Building(u16),
    Submenu(String),
}

pub struct Game {
    pub world: World,
    pub view: CityView,
    pub tool: Tool,
    /// Game speed in percent, 10..=100.
    pub speed: u32,
    pub paused: bool,
    pub sidebar: Sidebar,
    cart_images: Option<crate::anims::CartImages>,
    /// The held building's ghost, and what it was worked out for.
    ghost: Option<(GhostKey, Vec<GhostImage>)>,
    images: SidebarImages,
    text: Arc<TextTable>,
    entries: Vec<Entry>,
    hover: Option<(i32, i32)>,
    cursor: [f32; 2],
    drag_start: Option<(i32, i32)>,
    accumulator: f32,
    message: Option<(String, f32)>,
    messages: Arc<MessageTable>,
    /// The phrases of eventmsg.txt, for scenario event messages.
    pub phrases: Arc<osiris_formats::Phrases>,
    /// The governor's name, for the messages.
    pub player_name: String,
    /// The victory speech (text group 147): the campaign's line for its missions,
    /// 37 for any other map, as the original picks it.
    pub victory_text: usize,
    dialog: Option<MessageDialog>,
    pub info: Option<InfoPanel>,
    minimap: Option<Minimap>,
    pub audio: Option<Arc<Audio>>,
    music_timer: f32,
    /// Map-changing actions since the minimap was last rebuilt.
    map_changed: bool,
    /// The world as it was before the last build action, and when that was.
    undo: Option<(Vec<u8>, u64)>,
    pub message_list: Option<MessageList>,
    pub empire: Option<crate::empire_window::EmpireWindow>,
    pub advisors: Option<crate::advisors::Advisors>,
    /// The company awaiting orders: the next map click sends it there.
    pub selected_company: Option<usize>,
    advisor_images: Option<crate::advisors::AdvisorImages>,
    ui_images: Option<crate::widgets::UiImages>,
    empire_images: Option<crate::empire_window::EmpireImages>,
    custom_dialog: Option<Message>,
    pub rules_panel: Option<RulesPanel>,
    /// Set when the player changes the rules, so the caller can store them.
    pub rules_changed: bool,
    /// A yes/no popup open, and what a yes asks for; a click on it waits to be handled.
    confirm: Option<(crate::popup::Confirm, MenuAction)>,
    confirm_click: Option<[f32; 2]>,
    /// The Sound options window, the player's sound settings, and a click on the
    /// window waiting to be handled.
    pub sound_window: Option<crate::sound_options::SoundWindow>,
    pub sound_prefs: crate::sound_options::SoundPrefs,
    sound_click: Option<[f32; 2]>,
    /// Set when the player keeps new sound settings, so the caller can store them.
    pub sound_changed: bool,
    /// The Difficulty window is open, and a click on it waits to be handled.
    pub difficulty_panel: bool,
    difficulty_click: Option<[f32; 2]>,
    /// Set when the player changes the difficulty, so the caller can store it.
    pub difficulty_changed: bool,
    /// The city is saved each month (the original's Autosave option).
    pub autosave: bool,
    /// Set when the player switches autosave, so the caller can store it.
    pub autosave_changed: bool,
    /// Set when the player switches the Options menu's Fullscreen entry, so the
    /// caller (which owns the window) can actually apply it.
    pub fullscreen_changed: bool,
    top_menu: TopMenu,
    /// The overlay being shown, if any.
    pub view_overlay: Option<View>,
    /// Seconds of unpaused play, for building animations.
    anim_clock: f32,
    /// The overlay Space switches back to.
    last_overlay: View,
    overlay_images: Option<OverlayImages>,
    /// A menu choice for the app to carry out (leave the game, load, save, quit).
    pub request: Option<MenuAction>,
    /// A click on the lost-mission screen, for its buttons to take.
    lost_click: Option<[f32; 2]>,
    /// Next entry of the problem list to jump to.
    problem_cursor: usize,
    /// Build categories with nothing to build, refreshed daily.
    empty: Vec<Category>,
    empty_day: Option<(u32, u32)>,
    /// Dust clouds, and the hailstorm's lightning.
    disasters: crate::disaster_view::Disasters,
    /// The plague track playing in place of the music.
    plague_track: Option<osiris_sim::plagues::Track>,
    /// The cheat box opened with Ctrl+Alt+C, and what has been typed into it so far.
    /// See notes/cheats.md.
    cheat_entry: Option<String>,
    /// Set when the "Unlock All Missions" cheat fires, for the app to mark every
    /// campaign mission won in the family's saved progress (not in the original).
    pub cheat_unlock_missions: bool,
    /// The custom map the city was started from, which Replay starts again while
    /// the file is still there.
    pub replay_map: Option<std::path::PathBuf>,
}

impl Game {
    pub fn new(
        world: World,
        images: SidebarImages,
        text: Arc<TextTable>,
        messages: Arc<MessageTable>,
        audio: Option<Arc<Audio>>,
    ) -> Self {
        let top_menu = TopMenu::new(&text);
        Self {
            phrases: Arc::default(),
            player_name: "Governor".to_owned(),
            victory_text: 37,
            world,
            view: CityView::default(),
            tool: Tool::None,
            speed: 70,
            paused: false,
            sidebar: Sidebar::default(),
            cart_images: None,
            ghost: None,
            images,
            text,
            entries: Vec::new(),
            hover: None,
            cursor: [0.0, 0.0],
            drag_start: None,
            accumulator: 0.0,
            message: None,
            messages,
            dialog: None,
            info: None,
            minimap: None,
            audio,
            music_timer: 0.0,
            map_changed: true,
            undo: None,
            message_list: None,
            empire: None,
            advisors: None,
            selected_company: None,
            advisor_images: None,
            ui_images: None,
            empire_images: None,
            custom_dialog: None,
            rules_panel: None,
            confirm: None,
            confirm_click: None,
            difficulty_panel: false,
            sound_window: None,
            sound_prefs: Default::default(),
            sound_click: None,
            sound_changed: false,
            difficulty_click: None,
            difficulty_changed: false,
            autosave: true,
            autosave_changed: false,
            fullscreen_changed: false,
            rules_changed: false,
            top_menu,
            view_overlay: None,
            anim_clock: 0.0,
            last_overlay: View::Water,
            overlay_images: None,
            request: None,
            lost_click: None,
            problem_cursor: 0,
            empty: Vec::new(),
            empty_day: None,
            disasters: Default::default(),
            plague_track: None,
            cheat_entry: None,
            cheat_unlock_missions: false,
            replay_map: None,
        }
    }

    fn sound(&self, name: &str) {
        if let Some(a) = &self.audio {
            a.play_effect(name);
        }
    }

    /// Opens the next queued message, if nothing is showing.
    fn next_dialog(&mut self, r: &Renderer) {
        if self.dialog.is_some() {
            return;
        }
        if let Some(msg) = self.custom_dialog.take() {
            self.dialog = Some(MessageDialog::new(r, &msg, &self.text));
            return;
        }
        let Some(key) = self.world.messages.pop_front() else { return };
        let msg = if key == "out_of_time" {
            Message {
                title: self.text.get(62, 38).unwrap_or("Out of Time!").to_owned(),
                content: format!("@P{}", self.text.get(62, 39).unwrap_or("")),
                size: (30, 16),
                ..Default::default()
            }
        } else if key == "victory" {
            Message {
                title: self.text.get(62, 0).unwrap_or("Victory").to_owned(),
                content: format!("@P{}", self.text.get(147, self.victory_text).unwrap_or("")),
                size: (30, 16),
                ..Default::default()
            }
        } else if matches!(key.as_str(), "message_template_request" | "message_template_general") {
            let Some(t) = self.world.message_texts.pop_front() else { return };
            self.event_message(&t)
        } else {
            let Some(m) = osiris_sim::missions::message_id(&key).and_then(|id| self.messages.get(id as usize)) else {
                return;
            };
            m.clone()
        };
        self.dialog = Some(MessageDialog::new(r, &msg, &self.text));
        self.sound("BUTTON.WAV");
    }


    /// Nothing modal is open and no tool is in hand.
    pub fn idle(&self) -> bool {
        self.dialog.is_none() && self.info.is_none() && self.message_list.is_none() && self.empire.is_none() && self.advisors.is_none() && self.rules_panel.is_none() && !self.difficulty_panel && self.sound_window.is_none() && self.confirm.is_none() && self.world.messages.is_empty() && self.tool == Tool::None && self.sidebar.open.is_none()
    }

    pub fn close_dialog(&mut self) {
        self.dialog = None;
        self.world.messages.clear();
    }

    pub fn scroll_dialog(&mut self, delta: f32, screen: [f32; 2]) -> bool {
        if let Some(a) = &mut self.advisors {
            a.scroll(if delta < 0.0 { 1 } else { -1 });
            return true;
        }
        if let Some(i) = &mut self.info
            && i.contains(self.cursor)
        {
            i.scroll(if delta < 0.0 { 1 } else { -1 });
            return true;
        }
        match &mut self.dialog {
            Some(d) => {
                d.scroll(delta, screen);
                true
            }
            None => match &mut self.message_list {
                Some(l) => {
                    l.scroll(&self.world, if delta < 0.0 { 1 } else { -1 });
                    true
                }
                None => false,
            },
        }
    }

    pub fn faster(&mut self) {
        if let Some(&s) = SPEEDS.iter().find(|&&s| s > self.speed) {
            self.speed = s;
        }
    }

    pub fn slower(&mut self) {
        if let Some(&s) = SPEEDS.iter().rev().find(|&&s| s < self.speed) {
            self.speed = s;
        }
    }

    fn can_undo(&self) -> bool {
        self.undo.as_ref().is_some_and(|(_, t)| self.world.time.total_ticks - t <= UNDO_TICKS)
    }

    fn undo(&mut self) {
        let Some((data, _)) = self.undo.take() else { return };
        match World::load(&data, self.world.defs.clone(), self.world.balance.clone()) {
            Ok(mut w) => {
                w.rules = self.world.rules.clone();
                if let Some(b) = self.world.balances.clone() {
                    w.attach_balances(b);
                }
                self.world = w;
                self.map_changed = true;
                self.ghost = None;
            }
            Err(e) => self.say(&e),
        }
    }

    /// Build categories whose menus hold nothing buildable in this mission.
    fn refresh_empty_categories(&mut self) {
        let day = (self.world.time.day, self.world.time.month);
        if self.empty_day == Some(day) {
            return;
        }
        self.empty_day = Some(day);
        let menus = [
            (Category::Food, "food"),
            (Category::Industry, "industry"),
            (Category::Distribution, "distribution"),
            (Category::Entertainment, "entertainment"),
            (Category::Religion, "religion"),
            (Category::Education, "education"),
            (Category::Health, "health"),
            (Category::Government, "administration"),
            (Category::Security, "security"),
        ];
        self.empty = menus
            .into_iter()
            .filter(|(_, key)| !self.menu_has_buildings(key, 0))
            .map(|(c, _)| c)
            .collect();
    }

    fn menu_has_buildings(&self, key: &str, depth: u32) -> bool {
        let defs = &self.world.defs;
        let Some(menu) = defs.menu(key) else { return false };
        menu.items.iter().any(|item| match item.strip_prefix("menu_") {
            Some(sub) => depth < 4 && self.menu_has_buildings(sub, depth + 1),
            None => defs.building_by_key(item).is_some_and(|d| self.world.is_allowed(d.id)),
        })
    }

    fn building_name(&self, k: u16) -> String {
        // "Altar of Sebek", "Oracle of Min" and the rest, two to a god.
        if let Some((god, bit)) = osiris_sim::temple_complex::upgrade_of(k)
            && let Some(name) = self.text.get(189, god * 2 + usize::from(bit == osiris_sim::temple_complex::ORACLE))
        {
            return name.to_owned();
        }
        self.text
            .get(TEXT_BUILDING_NAMES, k as usize)
            .map(str::to_owned)
            .or_else(|| self.world.defs.building(k).map(|d| d.key.replace('_', " ")))
            .unwrap_or_else(|| format!("#{k}"))
    }

    /// Whether a drop-down menu or a window over the city is open, which holds time
    /// still. Building info windows leave the city running.
    pub fn menu_open(&self) -> bool {
        self.top_menu.open.is_some()
            || self.advisors.is_some()
            || self.empire.is_some()
            || self.message_list.is_some()
            || self.rules_panel.is_some()
            || self.difficulty_panel
            || self.sound_window.is_some()
            || self.confirm.is_some()
            || self.custom_dialog.is_some()
    }

    pub fn update(&mut self, dt: f32) {
        self.sidebar.update(dt);
        if let Some(e) = &mut self.empire {
            e.tick(dt);
        }
        if !self.paused && !self.menu_open() {
            self.anim_clock += dt;
        }
        if let Some((_, t)) = &mut self.message {
            *t -= dt;
            if *t <= 0.0 {
                self.message = None;
            }
        }
        // The city's warnings (text group 19), several at once read as one line.
        if !self.world.warnings.is_empty() {
            let text = &self.text;
            let lines: Vec<String> = self.world.warnings.drain(..).map(|id| text.get(19, id as usize).unwrap_or("").trim().to_owned()).collect();
            self.say(&lines.join(" "));
        }
        // A plague's track plays in place of the music while it lasts.
        if self.plague_track != self.world.plagues.track {
            self.plague_track = self.world.plagues.track;
            if let Some(a) = &self.audio {
                a.play_plague_track(self.plague_track.map(|t| t.file()));
            }
            self.music_timer = 0.0;
        }
        if let Some(a) = &self.audio
            && self.plague_track.is_none()
        {
            self.music_timer -= dt;
            if self.music_timer <= 0.0 {
                a.update_music(self.world.population);
                self.music_timer = 5.0;
            }
        }
        // A lost city stands still behind the lost-mission screen, and the city waits
        // while a menu or a full window (overseers, empire, messages, rules) is open.
        if self.paused || self.dialog.is_some() || self.world.lost || self.menu_open() {
            return;
        }
        let ms = ms_per_tick(self.speed);
        // Don't try to catch up after a long stall (window hidden, debugger).
        self.accumulator = (self.accumulator + dt * 1000.0).min(ms.max(TICK_BUDGET_MS) * 50.0);
        let start = std::time::Instant::now();
        while self.accumulator >= ms {
            self.accumulator -= ms;
            self.world.tick();
            if !self.world.messages.is_empty() {
                // Stop at a new message so it shows at the moment it happened.
                self.accumulator = 0.0;
                break;
            }
            if start.elapsed().as_secs_f32() * 1000.0 > TICK_BUDGET_MS {
                self.accumulator = 0.0;
                break;
            }
        }
    }

    pub fn set_cursor(&mut self, r: &Renderer, screen: [f32; 2]) {
        self.cursor = screen;
        if let Some(d) = &mut self.dialog {
            d.hover(screen, r.screen);
            self.hover = None;
            return;
        }
        if let Some(a) = &mut self.advisors {
            a.hover(screen);
            self.hover = None;
            return;
        }
        if let Some(i) = &mut self.info {
            i.hover(screen);
        }
        if let Some(e) = &mut self.empire {
            e.hover(r.screen, screen);
            self.hover = None;
            return;
        }
        self.top_menu.hover(screen);
        if self.top_menu.contains(screen) {
            self.hover = None;
            return;
        }
        if let Some(p) = &mut self.rules_panel {
            p.hover(r.screen, sidebar::panel_left(r.screen[0]), screen);
            self.hover = None;
            return;
        }
        if let Some(l) = &mut self.message_list {
            l.hover(&self.world, r.screen, screen);
            self.hover = None;
            return;
        }
        self.sidebar.hover(r.screen, screen);
        if self.sidebar.contains(r.screen, screen) {
            self.hover = None;
            return;
        }
        let world = r.screen_to_world(screen);
        self.hover = city_view::world_to_tile(&self.world.map, world);
    }

    /// Holds building tool `k` with the cursor on `tile` (for scripted screenshots).
    pub fn hold_tool(&mut self, k: u16, tile: (i32, i32)) {
        self.tool = Tool::Build(k);
        self.hover = Some(tile);
    }

    fn footprint_origin(&self, k: u16, tile: (i32, i32)) -> (i32, i32) {
        // The cursor sits on the footprint's middle tile (a tomb's anchor block); an
        // upgrade goes where it points.
        if osiris_sim::temple_complex::is_upgrade(k) {
            return tile;
        }
        let (cx, cy) = self.world.cursor_tile(k);
        (tile.0 - cx, tile.1 - cy)
    }

    fn pending_command(&self) -> Option<Command> {
        let end = self.hover?;
        let start = self.drag_start.unwrap_or(end);
        match self.tool {
            Tool::None => None,
            Tool::Road => Some(Command::Road { start, end }),
            Tool::Clear => Some(Command::Clear { x0: start.0, y0: start.1, x1: end.0, y1: end.1 }),
            Tool::Build(k) if k == kind::VACANT_LOT || k == osiris_sim::irrigation::DITCH || k == osiris_sim::defenses::WALL => {
                Some(Command::Build { kind: k, x: start.0, y: start.1, x1: end.0, y1: end.1 })
            }
            Tool::Build(k) => {
                let (x, y) = self.footprint_origin(k, end);
                Some(Command::Build { kind: k, x, y, x1: x, y1: y })
            }
        }
    }

    fn open_menu(&mut self, key: &str, category: Category) {
        let defs = self.world.defs.clone();
        let Some(menu) = defs.menu(key) else { return };
        self.entries.clear();
        self.sidebar.items.clear();
        for item in &menu.items {
            if let Some(sub) = item.strip_prefix("menu_") {
                // Submenus with nothing to build are left out, as in the original.
                if !self.menu_has_buildings(sub, 1) {
                    continue;
                }
                let label_id = SUBMENUS.iter().find(|(k, _)| *k == sub).map(|&(_, id)| id);
                let label = label_id.map_or_else(|| sub.replace('_', " "), |id| self.building_name(id));
                self.entries.push(Entry::Submenu(sub.to_owned()));
                self.sidebar.items.push(MenuItem { label: format!("{} ...", title_case(&label)), cost: 0, enabled: true });
            } else if let Some(d) = defs.building_by_key(item).filter(|d| self.world.is_allowed(d.id)) {
                self.entries.push(Entry::Building(d.id));
                self.sidebar.items.push(MenuItem {
                    label: title_case(&self.building_name(d.id)),
                    cost: self.world.cost_of(d.id as usize),
                    enabled: true,
                });
            }
        }
        self.sidebar.open = Some(category);
    }

    pub fn choose_category(&mut self, c: Category) {
        self.drag_start = None;
        if self.sidebar.open == Some(c) {
            self.sidebar.open = None;
            return;
        }
        self.sidebar.open = None;
        let key = match c {
            Category::Housing => {
                self.tool = Tool::Build(kind::VACANT_LOT);
                return;
            }
            Category::Roads => {
                self.tool = Tool::Road;
                return;
            }
            Category::Clear => {
                self.tool = Tool::Clear;
                return;
            }
            Category::Food => "food",
            Category::Industry => "industry",
            Category::Distribution => "distribution",
            Category::Entertainment => "entertainment",
            Category::Religion => "religion",
            Category::Education => "education",
            Category::Health => "health",
            Category::Government => "administration",
            Category::Security => "security",
        };
        self.open_menu(key, c);
    }

    /// Handles a left click. Returns a tile to centre the view on (minimap clicks).
    pub fn press_at(&mut self, r: &Renderer) -> Option<(i32, i32)> {
        if self.dialog.is_none()
            && let Some(a) = &mut self.advisors
        {
            a.press(self.cursor);
            return None;
        }
        if self.dialog.is_none()
            && let Some(e) = &mut self.empire
        {
            let images = *self.empire_images.get_or_insert_with(|| crate::empire_window::EmpireImages::load(&r.library).expect("empire images"));
            match e.click(r, &self.world, &images, self.cursor) {
                crate::empire_window::EmpireClick::Close => self.empire = None,
                crate::empire_window::EmpireClick::Advisor => self.open_advisor(crate::advisors::Advisor::Trade),
                crate::empire_window::EmpireClick::OpenRoute(c) => match self.world.open_trade_route(c) {
                    Ok(()) => {
                        e.show(Some(crate::empire_window::EmpirePopup::Opened(c)));
                        self.sound("BUTTON.WAV");
                    }
                    Err(why) => self.say(why),
                },
                crate::empire_window::EmpireClick::Nothing => {}
            }
            return None;
        }
        if let Some(i) = &mut self.info
            && self.dialog.is_none()
        {
            if i.contains(self.cursor) {
                i.press(self.cursor);
            } else {
                self.info = None;
            }
            return None;
        }
        self.press(r.screen)
    }

    fn press(&mut self, screen: [f32; 2]) -> Option<(i32, i32)> {
        let screen_w = screen[0];
        if let Some(d) = &mut self.dialog {
            if d.click(self.cursor, screen) {
                self.dialog = None;
                self.sound("BUTTON.WAV");
            }
            return None;
        }
        if self.world.lost {
            self.lost_click = Some(self.cursor);
            return None;
        }
        if self.confirm.is_some() {
            self.confirm_click = Some(self.cursor);
            return None;
        }
        if self.sound_window.is_some() {
            self.sound_click = Some(self.cursor);
            return None;
        }
        if self.difficulty_panel {
            self.difficulty_click = Some(self.cursor);
            return None;
        }
        if let Some(p) = &mut self.rules_panel {
            match p.click(&mut self.world.rules, screen, sidebar::panel_left(screen_w), self.cursor) {
                RulesClick::Toggled => {
                    self.rules_changed = true;
                    self.sound("BUTTON.WAV");
                }
                RulesClick::Close | RulesClick::Outside => self.rules_panel = None,
                RulesClick::Inside => {}
            }
            return None;
        }
        match self.top_menu.click(self.cursor) {
            (true, Some(action)) => {
                self.menu_action(action);
                return None;
            }
            (true, None) => return None,
            _ => {}
        }
        if let Some(l) = &self.message_list {
            if let Some(i) = l.click(&self.world, screen, self.cursor) {
                self.open_notice(i);
            } else if !l.contains(screen, self.cursor) {
                self.message_list = None;
            }
            return None;
        }
        if let Some(m) = &self.minimap
            && !self.sidebar.sliding()
            && let Some(panel_left) = self.sidebar.minimap_left(screen_w)
            && m.contains(panel_left, self.cursor)
        {
            return m.pixel_to_tile(&self.world.map, panel_left, self.cursor);
        }
        match self.sidebar.click(screen, self.cursor) {
            Click::Button(b) => return self.press_button(b),
            Click::Item(i) => match self.entries.get(i).cloned() {
                Some(Entry::Building(k)) => {
                    self.tool = match k {
                        5 => Tool::Road,
                        9 => Tool::Clear,
                        k => Tool::Build(k),
                    };
                    self.pick_statue_look(k);
                    self.sidebar.open = None;
                    self.sound("BUTTON.WAV");
                }
                Some(Entry::Submenu(key)) => {
                    let c = self.sidebar.open.unwrap_or(Category::Food);
                    self.open_menu(&key, c);
                }
                None => {}
            },
            Click::Absorbed => {}
            Click::Outside => {
                self.sidebar.open = None;
                if self.info.is_some() {
                    // Clicks inside the window do nothing; outside it they close it.
                    return None;
                }
                if self.tool != Tool::None {
                    self.drag_start = self.hover;
                } else if let Some((x, y)) = self.hover
                    && self.command_company(x, y)
                {
                } else if let Some((x, y)) = self.hover {
                    let id = self.world.map.building.at_or(x, y, 0);
                    let target = if id != 0 { crate::info::Target::Building(id) } else { crate::info::Target::Tile(x, y) };
                    self.info = Some(InfoPanel::new(target));
                    self.sound("BUTTON.WAV");
                }
            }
        }
        None
    }

    fn press_button(&mut self, b: Button) -> Option<(i32, i32)> {
        let enabled = match b {
            Button::Build(c) => !self.empty.contains(&c),
            Button::Undo => self.can_undo(),
            Button::Messages => !self.world.notices.log.is_empty(),
            Button::Problem => self.world.problems().next().is_some(),
            Button::Briefing => self.briefing().is_some(),
            Button::SpeedDown | Button::SpeedUp => true,
            Button::Empire => !self.world.trade.cities.is_empty(),
            Button::Advisors | Button::Collapse => true,
        };
        if !enabled {
            return None;
        }
        self.sound("BUTTON.WAV");
        match b {
            Button::Build(c) => self.choose_category(c),
            Button::Undo => self.undo(),
            Button::Messages => {
                self.sidebar.open = None;
                self.tool = Tool::None;
                self.message_list = Some(MessageList::default());
            }
            Button::Problem => {
                let problems: Vec<(i32, i32)> = self.world.problems().filter_map(|n| n.tile).take(10).collect();
                let tile = problems[self.problem_cursor % problems.len()];
                self.problem_cursor += 1;
                return Some(tile);
            }
            Button::Briefing => {
                if let Some(key) = self.briefing() {
                    self.world.messages.push_back(key);
                }
            }
            Button::SpeedDown => self.slower(),
            Button::SpeedUp => self.faster(),
            Button::Empire => {
                self.sidebar.open = None;
                self.tool = Tool::None;
                self.empire = Some(Default::default());
            }
            Button::Advisors => self.open_advisor(crate::advisors::Advisor::Chief),
            Button::Collapse => {
                self.sidebar.open = None;
                self.sidebar.toggle();
            }
        }
        None
    }

    /// Opens the information window for whatever is under the cursor.
    pub fn inspect(&mut self) {
        let Some((x, y)) = self.hover else { return };
        self.info = Some(InfoPanel::new(self.info_target(x, y)));
        self.sound("BUTTON.WAV");
    }

    /// What an info click on tile `(x, y)` shows: its walkers, else its building,
    /// else the terrain.
    pub fn info_target(&self, x: i32, y: i32) -> crate::info::Target {
        let id = self.world.map.building.at_or(x, y, 0);
        // Walkers on the tile come first.
        let mut walkers = [0u32; 7];
        let mut n = 0;
        for f in self.world.figures.iter().filter(|f| (f.x, f.y) == (x, y) && !f.dead) {
            if n < walkers.len() {
                walkers[n] = f.id;
                n += 1;
            }
        }
        if let Some(c) = walkers[..n].iter().find_map(|&w| self.world.company_of(w)) {
            crate::info::Target::Company(c)
        } else if n > 0 {
            crate::info::Target::Figures(walkers, n as u8, 0)
        } else if id != 0 {
            crate::info::Target::Building(id)
        } else {
            crate::info::Target::Tile(x, y)
        }
    }

    pub fn open_advisor(&mut self, a: crate::advisors::Advisor) {
        self.sidebar.open = None;
        self.tool = Tool::None;
        self.empire = None;
        self.advisors = Some(crate::advisors::Advisors::new(a));
    }

    fn menu_action(&mut self, action: MenuAction) {
        self.sound("BUTTON.WAV");
        match action {
            MenuAction::Overseer(a) => self.open_advisor(a),
            MenuAction::Rules => self.open_rules(),
            MenuAction::Autosave => {
                let on = !self.autosave;
                self.set_autosave(on);
                self.autosave_changed = true;
            }
            MenuAction::InterfaceSize => {
                crate::gfx::set_ui_size(crate::gfx::ui_size() + 1);
                crate::gfx::save_ui_size();
                self.top_menu.relabel(MenuAction::InterfaceSize, &crate::gfx::ui_size_label());
            }
            MenuAction::Fullscreen => {
                crate::gfx::set_fullscreen(!crate::gfx::fullscreen());
                self.fullscreen_changed = true;
                self.sync_fullscreen_label();
            }
            MenuAction::Sound => self.sound_window = Some(crate::sound_options::SoundWindow),
            MenuAction::Difficulty => {
                self.difficulty_panel = true;
                self.sound("BUTTON.WAV");
            }
            MenuAction::Faster => self.faster(),
            MenuAction::Slower => self.slower(),
            MenuAction::Pause => self.paused = !self.paused,
            MenuAction::Controls => self.show_text("Controls", CONTROLS),
            MenuAction::About => self.show_text("About Osiris", ABOUT),
            MenuAction::Overlay(o) => {
                if let Some(o) = o {
                    self.last_overlay = o;
                }
                self.view_overlay = o;
                self.tool = Tool::None;
                self.sidebar.open = None;
            }
            // Leaving the city asks first, as the original does.
            MenuAction::MainMenu | MenuAction::Quit => self.ask_to_leave(action),
            other => self.request = Some(other),
        }
    }

    /// Switches the monthly autosave, and the Options menu's label for it.
    pub fn set_autosave(&mut self, on: bool) {
        self.autosave = on;
        let label = self.text.get(2, if on { 9 } else { 10 }).unwrap_or(if on { "Autosave - ON" } else { "Autosave - OFF" }).trim().to_owned();
        self.top_menu.relabel(MenuAction::Autosave, &label);
    }

    /// Keeps the Options menu's Fullscreen entry in step with the window, after a
    /// change made outside the menu (Alt+Enter, F11, or Ctrl+Cmd+F on macOS).
    pub fn sync_fullscreen_label(&mut self) {
        self.top_menu.relabel(MenuAction::Fullscreen, &crate::gfx::fullscreen_label(&self.text));
    }

    /// Asks the original's "Leave the Kingdom?" before `then` (back to the main
    /// menu, or quitting).
    pub fn ask_to_leave(&mut self, then: MenuAction) {
        self.confirm = Some((crate::popup::Confirm::from_text(&self.text, 5, 0), then));
    }

    /// Queues a dialog with plain text of our own.
    fn show_text(&mut self, title: &str, body: &str) {
        self.custom_dialog = Some(Message { title: title.to_owned(), content: body.to_owned(), size: (30, 20), ..Default::default() });
    }

    /// Shows overlay `o`, or goes back to the normal view if it is already showing.
    pub fn show_overlay(&mut self, o: View) {
        if self.view_overlay == Some(o) {
            self.view_overlay = None;
        } else {
            self.last_overlay = o;
            self.view_overlay = Some(o);
        }
    }

    /// Switches between the normal view and the last overlay used.
    pub fn toggle_overlay(&mut self) {
        let last = self.last_overlay;
        self.show_overlay(last);
    }

    /// Taking up a statue tool starts at its first look, facing the viewer; a
    /// gatehouse starts at facing 1, as in the original.
    fn pick_statue_look(&mut self, k: u16) {
        if k == osiris_sim::defenses::GATEHOUSE {
            self.world.gatehouse_facing = 1;
        }
        self.world.statue_variant = 0;
        self.world.statue_facing = 1;
        self.world.complex_facing = 0;
    }

    fn holding_statue(&self) -> bool {
        matches!(self.tool, Tool::Build(k) if self.world.defs.building(k).is_some_and(|d| d.has_flag("is_statue")))
    }

    /// R while holding a statue: its next look, and after the last look the first
    /// one turned a quarter (the original's one counter through all sixteen);
    /// while holding a gatehouse or temple complex: turn it across the other way.
    pub fn rotate_statue(&mut self) {
        if let Tool::Build(k) = self.tool
            && let Some(n) = self.world.defs.building(k).filter(|d| d.has_flag("is_statue")).map(|d| d.variants.len().max(1))
        {
            self.world.statue_variant = ((self.world.statue_variant as usize + 1) % n) as u8;
            if self.world.statue_variant == 0 {
                self.world.statue_facing = (self.world.statue_facing + 1) % 4;
            }
        }
        if self.tool == Tool::Build(osiris_sim::defenses::GATEHOUSE) {
            self.world.gatehouse_facing ^= 1;
        }
        if matches!(self.tool, Tool::Build(k) if osiris_sim::temple_complex::is_complex(k)) {
            self.world.complex_facing ^= 1;
        }
    }

    /// Ctrl+R while holding a statue: turn it a quarter, keeping its look. Not in
    /// the original (only R, through every look before each turn).
    pub fn turn_statue(&mut self) {
        if self.holding_statue() {
            self.world.statue_facing = (self.world.statue_facing + 1) % 4;
        }
    }

    /// Picks up building `k` as the tool, if this mission allows it.
    pub fn try_tool(&mut self, k: u16) {
        if self.world.is_allowed(k) {
            self.tool = Tool::Build(k);
            self.pick_statue_look(k);
            self.sidebar.open = None;
        } else {
            self.say("Not available yet");
        }
    }

    pub fn open_top_menu(&mut self, n: usize) {
        self.top_menu.open = (n < self.top_menu.headers.len()).then_some(n);
    }

    pub fn open_rules(&mut self) {
        self.sidebar.open = None;
        self.tool = Tool::None;
        self.drag_start = None;
        self.rules_panel = Some(RulesPanel::default());
        self.sound("BUTTON.WAV");
    }

    fn briefing(&self) -> Option<String> {
        self.world.mission.as_ref()?.start_message.clone()
    }

    /// A scenario event's message: the template's frame, written from its phrases with
    /// the blanks filled in.
    fn event_message(&self, t: &osiris_sim::scenario_events::EventText) -> Message {
        let phrase = |name: &str| self.phrases.get(&format!("PHRASE_{name}")).unwrap_or("").to_owned();
        let item = |r: u16| self.text.get(23, 54 + r as usize).unwrap_or("").to_owned();
        let city = |c: Option<u8>| c.and_then(|c| self.text.get(195, c as usize)).unwrap_or("").to_owned();
        let shown = |r: u16, units: i32| if r == osiris_sim::scenario_events::DEBEN || r == osiris_sim::scenario_events::TROOPS || units < 100 { units } else { units / 100 };

        let fill = |s: &str, (r, amount, c): (u16, i32, Option<u8>), reason: &str| {
            s.replace("[greeting]", self.text.get(32, 11).unwrap_or(""))
                .replace("[player_name]", &self.player_name)
                .replace("[reason_phrase]", reason)
                .replace("[city_name]", &city(c))
                .replace("[amount]", &shown(r, amount).to_string())
                .replace("[amount_granted]", &shown(r, amount).to_string())
                .replace("[item]", &item(r))
                .replace("[time_allotted]", &t.months.to_string())
                .replace("[time_until_attack]", &t.months.to_string())
                .replace("[god]", t.god.and_then(|g| self.text.get(157, g as usize)).unwrap_or(""))
                .replace("[a_foreign_army]", t.army.and_then(|a| self.text.get(37, a)).unwrap_or("an army"))
        };
        let own = (t.resource, t.amount, t.city_name);
        let reason = fill(&phrase(&t.reason), t.cause.unwrap_or(own), "");
        let title = fill(&phrase(&t.title), own, "");
        let body = fill(&phrase(&t.body), own, &reason);
        let mut m = self.messages.get(t.template as usize).cloned().unwrap_or_default();
        m.title = title;
        m.content = format!("@P{}", body.split_whitespace().collect::<Vec<_>>().join(" "));
        m
    }

    /// Opens entry `i` of the message log.
    fn open_notice(&mut self, i: usize) {
        let Some(n) = self.world.notices.log.get_mut(i) else { return };
        n.read = true;
        let key = n.key.clone();
        if let Some(t) = n.text.clone() {
            self.world.message_texts.push_front(t);
        }
        self.message_list = None;
        self.world.messages.push_front(key);
        self.sound("BUTTON.WAV");
    }

    pub fn release(&mut self) {
        self.sidebar.release();
        if let Some(d) = &mut self.dialog {
            d.release();
        }
        if let Some(e) = &mut self.empire {
            e.release();
        }
        if self.drag_start.is_none() {
            return;
        }
        if let Some(cmd) = self.pending_command() {
            let snapshot = self.world.save().ok();
            self.ghost = None;
            match self.world.apply(&cmd) {
                Outcome::NotEnoughMoney => self.say("Out of credit!"),
                Outcome::Blocked => self.say("Can't build there"),
                Outcome::Invalid(why) => self.say(why),
                Outcome::Done { items, .. } => {
                    if items > 0 {
                        self.map_changed = true;
                        self.undo = snapshot.map(|s| (s, self.world.time.total_ticks));
                        self.sound("BUILD.WAV");
                    }
                }
            }
        }
        self.drag_start = None;
    }

    pub fn cancel(&mut self) {
        if let Some(a) = &mut self.advisors
            && self.dialog.is_none()
        {
            if a.back() {
                self.advisors = None;
            }
            return;
        }
        if let Some(i) = &mut self.info
            && self.dialog.is_none()
        {
            if i.back() {
                let command = i.company_on_close(&self.world);
                self.info = None;
                if let Some(c) = command {
                    self.select_company(c);
                }
            }
            return;
        }
        if self.dialog.is_none()
            && let Some(e) = &mut self.empire
            && e.popup().is_some()
        {
            e.right_click();
            return;
        }
        if self.dialog.take().is_some() || self.empire.take().is_some() || self.info.take().is_some() || self.message_list.take().is_some() || self.rules_panel.take().is_some() || std::mem::take(&mut self.difficulty_panel) || self.cancel_sound_window() || self.confirm.take().is_some() {
            return;
        }
        if self.selected_company.take().is_some() {
            return;
        }
        if self.sidebar.open.take().is_some() {
            return;
        }
        if self.drag_start.take().is_none() {
            self.tool = Tool::None;
        }
    }

    /// A map click while commanding: with a company selected, sends it to the tile
    /// (or home, when the tile is its fort), warning when it can't get there or its
    /// morale is too low, and ends the command as the original does; otherwise
    /// clicking one of the city's soldiers or a standard opens his company's
    /// window. True when the click was used.
    fn command_company(&mut self, x: i32, y: i32) -> bool {
        if let Some(c) = self.selected_company.take() {
            let Some(company) = self.world.military.companies.get(c) else { return false };
            let clicked = self.world.map.building.at_or(x, y, 0);
            let low_morale = company.morale < 21;
            if clicked != 0 && (clicked == company.fort || clicked == company.ground) {
                self.world.return_company(c);
            } else if !self.world.move_company(c, (x, y)) {
                self.warn(209);
            } else if low_morale {
                self.warn(49);
            }
            return true;
        }
        let Some(c) = self.company_at(x, y) else { return false };
        self.info = Some(InfoPanel::new(crate::info::Target::Company(c)));
        self.sound("BUTTON.WAV");
        true
    }

    /// The company of a soldier or standard on or beside tile `(x, y)`, those on
    /// the tile first.
    pub fn company_at(&self, x: i32, y: i32) -> Option<usize> {
        let near = |f: &&osiris_sim::figures::Figure, r: i32| !f.dead && f.action != osiris_sim::military::action::CORPSE && (f.x - x).abs() <= r && (f.y - y).abs() <= r;
        let find = |r: i32| self.world.figures.iter().filter(|f| near(f, r)).find_map(|f| self.world.company_of(f.id));
        find(0).or_else(|| find(1))
    }

    /// A city warning from text group 19.
    fn warn(&mut self, id: usize) {
        let text = self.text.get(19, id).unwrap_or("").to_owned();
        self.say(&text);
    }

    pub fn select_company(&mut self, c: usize) {
        self.selected_company = Some(c);
        self.info = None;
        self.advisors = None;
        let name = self.text.get(138, c % 10).unwrap_or("").trim_matches('"').to_owned();
        self.say(&format!("{name}: click where to send them, or their fort to call them home"));
        self.sound("BUTTON.WAV");
    }

    fn say(&mut self, text: &str) {
        self.message = Some((text.to_owned(), 3.0));
    }

    /// The cheat box opened with Ctrl+Alt+C is showing, and typed keys go to it.
    pub fn wants_cheat_text(&self) -> bool {
        self.cheat_entry.is_some()
    }

    /// Ctrl+Alt+C: opens the cheat box, or closes it without effect if already open.
    pub fn toggle_cheat_entry(&mut self) {
        self.cheat_entry = if self.cheat_entry.is_some() { None } else { Some(String::new()) };
        self.sound("BUTTON.WAV");
    }

    /// Escape while the cheat box is open: closes it without effect.
    pub fn cancel_cheat_entry(&mut self) {
        self.cheat_entry = None;
    }

    /// Typing into the open cheat box: a character, backspace, or Enter to submit it.
    pub fn type_cheat_text(&mut self, text: &str) {
        let Some(buf) = &mut self.cheat_entry else { return };
        for c in text.chars() {
            match c {
                '\u{8}' | '\u{7f}' => {
                    buf.pop();
                }
                '\r' | '\n' => {
                    let code = std::mem::take(buf);
                    self.cheat_entry = None;
                    self.run_cheat(&code);
                    return;
                }
                c if !c.is_control() && buf.chars().count() < 40 => buf.push(c),
                _ => {}
            }
        }
    }

    /// Runs a typed cheat code, exactly as spelled (case sensitive) in the original.
    /// See notes/cheats.md for the source list and Osiris's own additions, which are
    /// clearly marked there.
    fn run_cheat(&mut self, code: &str) {
        use osiris_sim::cheats::Outcome;
        match osiris_sim::cheats::apply(&mut self.world, code) {
            Outcome::Applied => {}
            Outcome::NeedsGod(g) => self.say(&format!("{} is not worshipped here", osiris_sim::religion::NAMES[g])),
            Outcome::NotModeled => self.say("Not modeled in Osiris"),
            Outcome::NeedsApp if code == "Unlock All Missions" => self.cheat_unlock_missions = true,
            Outcome::NeedsApp | Outcome::Unknown => self.say("Unknown cheat"),
        }
    }

    /// The tiles to mark under the cursor, the held building's ghost, what the held
    /// tool would cost, and (for a building that can't go there) why not. As in the
    /// original, a building that may go where the cursor is shows its ghost, tinted
    /// green; otherwise its tiles from the simulation's placement preview, each
    /// green where it may go and red where it blocks, so what shows green is what
    /// builds. Houses, roads and the like show only tiles.
    fn highlights(&mut self) -> (Vec<Highlight>, Vec<GhostImage>, Option<i32>, Option<&'static str>) {
        let ok = Paint::Filter(city_view::PLACE_OK);
        let bad = Paint::Filter(city_view::PLACE_BAD);
        let mark = |(x, y): (i32, i32), paint: Paint| Highlight { x, y, color: osiris_render::WHITE, paint };
        let Some(cmd) = self.pending_command() else {
            let tint = |(x, y): (i32, i32), color: [f32; 4]| Highlight { x, y, color, paint: Paint::Silhouette };
            let mut tiles: Vec<Highlight> = self.hover.map(|t| tint(t, [1.0, 1.0, 1.0, 0.25])).into_iter().collect();
            // The selected company's soldiers.
            if let Some(c) = self.selected_company.and_then(|c| self.world.military.companies.get(c)) {
                for f in c.soldiers.iter().filter_map(|&s| self.world.figures.get(s)) {
                    tiles.push(tint((f.x, f.y), [1.0, 0.85, 0.2, 0.35]));
                }
            }
            return (tiles, Vec::new(), None, None);
        };
        let est = self.world.estimate(&cmd);
        let cost = match est {
            Outcome::Done { cost, .. } => Some(cost),
            _ => None,
        };
        let affordable = cost.is_some() && !self.world.out_of_money();
        let paint = if affordable { ok } else { bad };
        let rect = |x0: i32, y0: i32, x1: i32, y1: i32| {
            let mut v = Vec::new();
            for y in y0.min(y1)..=y0.max(y1) {
                for x in x0.min(x1)..=x0.max(x1) {
                    v.push((x, y));
                }
            }
            v
        };
        let tiles: Vec<(i32, i32)> = match cmd {
            Command::Road { start, end } => self.world.road_path(start, end).unwrap_or_else(|| vec![end]),
            Command::Clear { x0, y0, x1, y1 } => rect(x0, y0, x1, y1),
            Command::Build { kind: k, x, y, x1, y1 } if k == kind::VACANT_LOT => rect(x, y, x1, y1),
            Command::Build { kind: k, x, y, x1, y1 } if k == osiris_sim::defenses::WALL => self.world.wall_sites(x, y, x1, y1),
            Command::Build { kind: k, x, y, x1, y1 } if k == osiris_sim::irrigation::DITCH => self.world.ditch_path((x, y), (x1, y1)).unwrap_or_else(|| vec![(x1, y1)]),
            Command::Build { kind: k, x, y, .. } if osiris_sim::temple_complex::is_upgrade(k) => {
                // An altar or oracle goes in its fixed place on the complex under the
                // cursor; anywhere else the original shows three tiles square in red.
                if affordable {
                    return (Vec::new(), self.ghost(k, x, y), cost, None);
                }
                let why = match est {
                    Outcome::Invalid(why) => Some(why),
                    _ => Some("Out of credit!"),
                };
                return (rect(x - 1, y - 1, x + 1, y + 1).into_iter().map(|t| mark(t, bad)).collect(), Vec::new(), cost, why);
            }
            Command::Build { kind: k, x, y, .. } => {
                let preview = self.world.placement_preview(k, x, y);
                // Money only matters once the ground will do.
                let poor = preview.result.is_ok() && !affordable;
                let why = preview.result.err().or(poor.then_some("Out of credit!"));
                if why.is_none() && self.world.has_ghost(k) {
                    let ghost = self.ghost(k, x, y);
                    if !ghost.is_empty() {
                        return (Vec::new(), ghost, cost, None);
                    }
                }
                let marks = preview.tiles.iter().map(|t| mark((t.x, t.y), if t.red || poor { bad } else { ok })).collect();
                return (marks, Vec::new(), cost, why);
            }
        };
        (tiles.into_iter().map(|t| mark(t, paint)).collect(), Vec::new(), cost, None)
    }

    /// The ghost of building `k` placed at `(x, y)`, worked out again only when
    /// something it depends on changes.
    fn ghost(&mut self, k: u16, x: i32, y: i32) -> Vec<GhostImage> {
        let w = &self.world;
        let key = (k, x, y, w.statue_variant, w.statue_facing, w.gatehouse_facing, w.complex_facing, w.time.total_ticks / 51);
        match &self.ghost {
            Some((have, images)) if *have == key => images.clone(),
            _ => {
                let images = w.placement_ghost(k, x, y);
                self.ghost = Some((key, images.clone()));
                images
            }
        }
    }

    fn sprites(&mut self, r: &Renderer) -> Vec<Sprite> {
        for s in self.disasters.take(&mut self.world) {
            self.sound(s);
        }
        let carts = *self.cart_images.get_or_insert_with(|| crate::anims::CartImages::load(&r.library).expect("cart images"));
        let defs = &self.world.defs;
        let mut out = Vec::new();
        for f in self.world.figures.iter().filter(|f| self.view_overlay.is_none_or(|v| v.shows_figure(&self.world, f.kind))) {
            if let Some(s) = crate::tomb_view::figure_sprite(&self.world, f) {
                out.push(s);
                continue;
            }
            // Craftsmen at work on a monument.
            if matches!(f.kind, osiris_sim::monuments::BRICKLAYER | osiris_sim::monuments::STONEMASON | osiris_sim::monuments::CARPENTER)
                && matches!(f.action, 2 | osiris_sim::monuments::AT_SPOT)
                && f.moving
                && let Some(work) = defs.figure(f.kind).and_then(|d| d.anims.get("work"))
            {
                let frame = (self.world.time.total_ticks / work.duration.max(1) as u64 % work.frames.max(1) as u64) as u32;
                out.push(Sprite { behind: false, x: f.x, y: f.y, offset: f.pixel_offset(), image: work.image + f.direction as u32 + 8 * frame });
                continue;
            }
            // The wild beasts, in water or out.
            if osiris_sim::predators::is_predator(f.kind) {
                out.extend(crate::army_view::beast_sprite(&self.world, f));
                continue;
            }
            if let Some(s) = crate::water_view::figure_sprite(&self.world, f) {
                out.push(s);
                continue;
            }
            if f.kind == osiris_sim::military::STANDARD_BEARER {
                out.extend(crate::army_view::standard_sprites(r, &self.world, f, self.world.time.total_ticks));
                continue;
            }
            if let Some(s) = crate::army_view::fighter_sprite(&self.world, f) {
                out.push(s);
                continue;
            }
            if let Some(s) = crate::disaster_view::figure_sprite(&self.world, f).or_else(|| crate::army_view::fallen_sprite(&self.world, f)) {
                out.push(s);
                continue;
            }
            // A sentry at his post stands on top of his tower.
            if f.kind == osiris_sim::defenses::TOWER_SENTRY
                && f.action == osiris_sim::military::action::AT_STANDARD
                && let Some(walk) = defs.figure(f.kind).and_then(|d| d.anims.get("walk"))
            {
                out.push(Sprite { behind: false, x: f.x + 1, y: f.y + 1, offset: (0, -52), image: walk.image + f.direction as u32 });
                continue;
            }
            let Some(walk) = defs.figure(f.kind).and_then(|d| d.anims.get("walk")) else { continue };
            let frame = if f.moving { f.frame(walk.frames.max(1)) } else { 0 };
            let offset = f.pixel_offset();
            let walker = Sprite { behind: false, x: f.x, y: f.y, offset, image: walk.image + f.direction as u32 + 8 * frame };
            if !matches!(f.kind, osiris_sim::economy::CART_PUSHER | osiris_sim::economy::STORAGEYARD_CART | osiris_sim::docks::DOCKER) {
                out.push(walker);
                continue;
            }
            // The cart, drawn behind its pusher when it is on the far side.
            let (image, (cx, cy)) = carts.cart(f.cargo, f.amount, f.direction);
            let cart = Sprite { behind: false, x: f.x, y: f.y, offset: (offset.0 + cx, offset.1 + cy - 7), image };
            if cy < 0 {
                out.extend([cart, walker]);
            } else {
                out.extend([walker, cart]);
            }
        }
        if self.view_overlay.is_none() {
            out.extend(crate::water_view::fishing_points(&self.world));
            out.extend(self.disasters.cloud_sprites(&self.world, self.world.time.total_ticks));
        }
        out
    }

    /// Images drawn over buildings: growing crops on farms.
    fn overlays(&self, r: &Renderer) -> Vec<Overlay> {
        let mut out = Vec::new();
        for b in self.world.buildings.iter() {
            if !self.world.is_farm(b.kind) {
                continue;
            }
            let crops = self.world.crop_overlays(b.id);
            let n = crops.len();
            let top = city_view::tile_to_world(&self.world.map, b.x, b.y);
            let point = [top[0] - (b.size - 1) as f32 * city_view::TILE_W / 2.0, top[1]];
            for (i, image) in crops {
                // Floodplain farms crop every tile; meadow farms only the front edge.
                let (dx, dy) = if n == 9 {
                    ((i % 3) as i32, (i / 3) as i32)
                } else {
                    [(0, 2), (1, 2), (2, 2), (2, 1), (2, 0)][i]
                };
                let (ox, oy) = (((dx - dy) * 30 + (b.size - 1) * 30) as f32, ((dx + dy) * 15) as f32);
                let h = r.record(image).map_or(30.0, |rec| rec.height as f32);
                out.push(Overlay {
                    x: b.x + dx,
                    y: b.y + dy,
                    pos: [point[0] + ox, point[1] + oy + city_view::TILE_H - h],
                    image,
                });
            }
        }
        let cx = crate::anims::AnimContext { world: &self.world, r, ticks: self.world.time.total_ticks, millis: (self.anim_clock * 1000.0) as u64 };
        crate::anims::building_animations(&cx, &mut out);
        out
    }

    pub fn draw(&mut self, r: &mut Renderer) {
        self.next_dialog(r);
        if let Some(a) = &mut self.advisors {
            let images = *self.advisor_images.get_or_insert_with(|| crate::advisors::AdvisorImages::load(&r.library).expect("overseer images"));
            let ui_images = *self.ui_images.get_or_insert_with(|| crate::widgets::UiImages::load(&r.library).expect("ui images"));
            let action = a.draw(r, &self.images.panels, images, ui_images, &mut self.world, &self.text);
            if let Some(d) = &mut self.dialog {
                d.draw(r);
            }
            match action {
                Some(crate::advisors::AdvisorAction::Close) => self.advisors = None,
                Some(crate::advisors::AdvisorAction::OpenEmpire) => {
                    self.advisors = None;
                    self.empire = Some(Default::default());
                }
                Some(crate::advisors::AdvisorAction::GoToCompany(c)) => {
                    self.advisors = None;
                    let at = self.world.military.companies.get(c).and_then(|co| co.soldiers.iter().find_map(|&s| self.world.figures.get(s)).map(|f| (f.x, f.y)));
                    if let Some((x, y)) = at {
                        self.view.center_on(r, &self.world.map, x, y);
                    }
                    self.select_company(c);
                }
                None => {}
            }
            return;
        }
        if let Some(e) = &mut self.empire {
            let images = *self.empire_images.get_or_insert_with(|| crate::empire_window::EmpireImages::load(&r.library).expect("empire images"));
            e.draw(r, &self.images.panels, &self.world, &self.text, &images);
            if let Some(d) = &mut self.dialog {
                d.draw(r);
            }
            return;
        }
        let (marks, ghost, cost, why) = self.highlights();
        let marker = self.world.defs.terrain.empty_land;
        let sprites = self.sprites(r);
        let overlays = if self.view_overlay.is_some() { Vec::new() } else { self.overlays(r) };
        self.view.clamp_camera(r, &self.world.map, sidebar::panel_left(r.screen[0]), sidebar::TOP);
        let images = *self.overlay_images.get_or_insert_with(|| OverlayImages::load(&r.library).expect("overlay images"));
        let world = &self.world;
        let view = self.view_overlay;
        let look = move |x: i32, y: i32| view.map_or(crate::overlay::TileLook::Normal, |v| v.look(world, &images, x, y));
        let draw = view.map(|v| city_view::OverlayDraw {
            look: &look,
            flat: images.flat,
            columns: world
                .buildings
                .iter()
                .filter_map(|b| {
                    let c = v.column(b)?;
                    Some(city_view::ColumnMark { x: b.x, y: b.y + b.size - 1, image: images.column + c.color, height: c.height })
                })
                .collect(),
        });
        self.view.draw(r, &self.world.map, &marks, &ghost, marker, &sprites, &overlays, draw.as_ref());
        self.disasters.draw_hail(r, &self.world, self.anim_clock, sidebar::panel_left(r.screen[0]), sidebar::TOP);
        self.draw_overlay(r, cost, why);
    }

    fn draw_overlay(&mut self, r: &mut Renderer, cost: Option<i32>, why: Option<&str>) {
        let t = &self.world.time;
        let month = self.text.get(TEXT_MONTHS, t.month as usize).unwrap_or("?");
        let year = if t.year < 0 { format!("{} BC", -t.year) } else { format!("{} AD", t.year) };
        let label = |i: usize, fallback: &str| self.text.get(6, i).unwrap_or(fallback).to_owned();
        let status = [
            (label(0, "Db"), self.world.treasury.to_string()),
            (label(1, "Pop"), self.world.population.to_string()),
            (month.to_owned(), year),
        ];
        self.refresh_empty_categories();
        let category = match self.tool {
            Tool::Road => Some(Category::Roads),
            Tool::Clear => Some(Category::Clear),
            Tool::Build(k) if k == kind::VACANT_LOT => Some(Category::Housing),
            _ => self.sidebar.open,
        };
        let flood = match self.world.flood_state() {
            _ if !self.world.has_floodplain() => None,
            osiris_sim::floods::FloodState::Resting | osiris_sim::floods::FloodState::Farmable => Some("Farming"),
            osiris_sim::floods::FloodState::Imminent => Some("Flood soon"),
            osiris_sim::floods::FloodState::Flooding => Some("Rising"),
            osiris_sim::floods::FloodState::Inundated => Some("Flooded"),
            osiris_sim::floods::FloodState::Contracting => Some("Receding"),
        };
        let mut lines = vec![
            ("Unemployed".to_owned(), format!("{}%", self.world.unemployment)),
            ("Workers".to_owned(), format!("{}/{}", self.world.labor.employed, self.world.labor.needed)),
        ];
        if let Some(f) = flood {
            lines.push(("Nile".to_owned(), f.to_owned()));
        }
        let text = self.text.clone();
        let tips = move |i: usize| text.get(68, i).map(str::to_owned);
        let speed = speed_label(self.speed);
        let state = SidebarState {
            empty: &self.empty,
            category,
            unread: self.world.unread_notices(),
            has_messages: !self.world.notices.log.is_empty(),
            has_problems: self.world.problems().next().is_some(),
            has_briefing: self.briefing().is_some(),
            has_empire: !self.world.trade.cities.is_empty(),
            can_undo: self.can_undo(),
            speed: &speed,
            lines: &lines,
            tips: &tips,
        };
        self.sidebar.draw(r, &self.images, &state);
        let minimap = self.minimap.get_or_insert_with(|| Minimap::new(r));
        // The simulation changes terrain too (fires, rubble); refresh about once a second.
        if self.map_changed || self.world.time.tick == 0 {
            minimap.mark_dirty();
            self.map_changed = false;
        }
        let buildings = &self.world.buildings;
        let is_house = |id: u32| buildings.get(id).is_some_and(|b| b.house.is_some());
        if let Some(left) = self.sidebar.minimap_left(r.screen[0]) {
            let clip = self.sidebar.sliding().then_some([0.0, 0.0, r.screen[0] - 24.0, r.screen[1]]);
            r.set_clip(clip);
            minimap.draw(r, &self.world.map, is_house, left);
            r.set_clip(None);
        }
        self.sidebar.draw_tooltip(r, &state);
        let tool = match self.tool {
            Tool::None => String::new(),
            Tool::Road => self.building_name(5),
            Tool::Clear => self.building_name(9),
            Tool::Build(k) => self.building_name(k),
        };
        // Where the held building can't go, say why, as the original's warning would.
        let line = match (why, cost.filter(|&c| c > 0)) {
            (Some(why), _) => format!("{tool}: {why}"),
            (None, Some(c)) => format!("{tool}: {c} Db"),
            (None, None) => tool,
        };
        if !line.is_empty() {
            draw_text(r, Font::SmallOutlined, &line, 10.0, 38.0, font::WHITE);
        }
        if let Some((m, _)) = &self.message {
            let w = r.screen[0] - crate::sidebar::width();
            let mw = osiris_ui::text_width(r, Font::LargeBlackOnDark, m) as f32;
            draw_text(r, Font::LargeBlackOnDark, m, (w - mw) / 2.0, 70.0, font::WHITE);
        }
        // The cheat box: Ctrl+Alt+C, as in the original (see notes/cheats.md).
        if let Some(buf) = &self.cheat_entry {
            let line = format!("Cheat: {buf}_");
            let w = r.screen[0] - crate::sidebar::width();
            let mw = osiris_ui::text_width(r, Font::LargeBlackOnDark, &line) as f32;
            draw_text(r, Font::LargeBlackOnDark, &line, (w - mw) / 2.0, 70.0, font::WHITE);
        }
        let overlay_name = self.view_overlay.and_then(|o| crate::overlay::MENU.iter().find(|(v, _)| *v == o)).and_then(|(_, id)| self.text.get(14, *id));
        let paused = self.paused.then_some("Paused");
        let label = overlay_name.or(paused);
        self.top_menu.draw(r, &self.images.panels, &status, label);
        if let Some(i) = &mut self.info {
            let img = *self.ui_images.get_or_insert_with(|| crate::widgets::UiImages::load(&r.library).expect("ui images"));
            let mut ui = crate::widgets::Ui { r, panels: &self.images.panels, img, text: &self.text, cursor: self.cursor, click: None };
            match i.draw(&mut ui, &mut self.world) {
                Some(crate::info::InfoAction::Close) => self.info = None,
                Some(crate::info::InfoAction::Overseer(a)) => {
                    self.info = None;
                    self.open_advisor(a);
                }
                Some(crate::info::InfoAction::SelectCompany(c)) => self.select_company(c),
                None => {}
            }
        }
        if let Some(p) = &self.rules_panel {
            p.draw(r, &self.images.panels, &self.world.rules, sidebar::panel_left(r.screen[0]), "Changes apply now, and to every game you play.");
        }
        if self.sound_window.is_some() {
            self.draw_sound(r);
        }
        if self.difficulty_panel {
            self.draw_difficulty(r);
        }
        if let Some((c, then)) = &self.confirm {
            let img = *self.ui_images.get_or_insert_with(|| crate::widgets::UiImages::load(&r.library).expect("ui images"));
            let click = self.confirm_click.take();
            match c.draw(r, &self.images.panels, img, &self.text, self.cursor, click) {
                Some(true) => {
                    self.request = Some(*then);
                    self.confirm = None;
                    self.sound("BUTTON.WAV");
                }
                Some(false) => {
                    self.confirm = None;
                    self.sound("BUTTON.WAV");
                }
                None => {}
            }
        }
        if let Some(l) = &self.message_list {
            l.draw(r, &self.images.panels, &self.world, &self.messages, &self.text);
        }
        if self.world.lost && self.dialog.is_none() && self.world.messages.is_empty() {
            self.draw_lost(r);
        }
        if let Some(d) = &mut self.dialog {
            d.draw(r);
        }
    }

    fn draw_sound(&mut self, r: &mut Renderer) {
        let img = *self.ui_images.get_or_insert_with(|| crate::widgets::UiImages::load(&r.library).expect("ui images"));
        let Some(w) = &mut self.sound_window else { return };
        let click = self.sound_click.take();
        match w.draw(r, &self.images.panels, img, &self.text, self.cursor, click, &mut self.sound_prefs, self.audio.as_deref()) {
            crate::sound_options::Outcome::Open => {}
            crate::sound_options::Outcome::Changed => self.sound_changed = true,
            crate::sound_options::Outcome::Closed => self.sound_window = None,
        }
    }

    /// Closes the Sound options window; whether it was open.
    fn cancel_sound_window(&mut self) -> bool {
        self.sound_window.take().is_some()
    }

    /// The original's Difficulty window (Options menu): the level between arrows. A
    /// change applies at once; a right-click or a click outside closes it. Panel,
    /// title, level text, arrows and footer all match the original's own placing
    /// (FUN_00531be0, arrow table at 0x5c8ee8) pixel for pixel.
    fn draw_difficulty(&mut self, r: &mut Renderer) {
        let img = *self.ui_images.get_or_insert_with(|| crate::widgets::UiImages::load(&r.library).expect("ui images"));
        let ox = ((r.screen[0] - 640.0) / 2.0).floor();
        let oy = ((r.screen[1] - 480.0) / 2.0).floor();
        let (x, y) = (ox + 48.0, oy + 80.0);
        osiris_ui::panel::outer_panel(r, &self.images.panels, x, y, 24, 12);
        let click = self.difficulty_click.take();
        let mut ui = crate::widgets::Ui { r, panels: &self.images.panels, img, text: &self.text, cursor: self.cursor, click };
        let t = ui.t(153, 0);
        ui.centred(Font::LargeBlackOnLight, &t, x, oy + 94.0, 384.0);
        let t = ui.t(153, self.world.difficulty as usize + 1);
        ui.centred(Font::NormalBlackOnLight, &t, ox + 80.0, oy + 174.0, 224.0);
        let down = ui.arrow(ox + 288.0, oy + 166.0, false);
        let up = ui.arrow(ox + 312.0, oy + 166.0, true);
        let t = ui.t(153, 8);
        ui.centred(Font::NormalBlackOnLight, &t, x, oy + 246.0, 384.0);
        let outside = click.is_some_and(|[cx, cy]| cx < x || cy < y || cx >= x + 384.0 || cy >= y + 192.0);
        let d = self.world.difficulty;
        let want = if down { d.saturating_sub(1) } else if up { (d + 1).min(osiris_sim::difficulty::IMPOSSIBLE) } else { d };
        if down || up {
            self.sound("BUTTON.WAV");
        }
        if want != d {
            self.world.set_difficulty(want);
            self.difficulty_changed = true;
        }
        if outside {
            self.difficulty_panel = false;
        }
    }

    /// The original's screen for a lost mission (FUN_004194c0, FUN_00419db0): a 34x16
    /// panel at the top of the middle 640x480, "Defeat!" (62:1) and its text (62:16),
    /// then New Game and, for a campaign mission or a map still on disk, Replay mission. When time ran out
    /// on Easy or harder a campaign mission's reads "Out of Time!" (62:38, 62:39) and
    /// offers Lower Difficulty first.
    fn draw_lost(&mut self, r: &mut Renderer) {
        const W: i32 = 34;
        const H: i32 = 16;
        let img = *self.ui_images.get_or_insert_with(|| crate::widgets::UiImages::load(&r.library).expect("ui images"));
        let w = W as f32 * 16.0;
        let x = ((r.screen[0] - 640.0) / 2.0).floor() + 48.0;
        let y = ((r.screen[1] - 480.0) / 2.0).floor() + 9.0;
        osiris_ui::panel::outer_panel(r, &self.images.panels, x, y, W, H);
        let click = self.lost_click.take();
        // A custom map can be replayed while its file is still there, as the original
        // offers it.
        let replay = self.world.mission.is_some() || self.replay_map.as_ref().is_some_and(|p| p.exists());
        let time = self.world.mission.is_some() && self.world.lost_to_time();
        let mut ui = crate::widgets::Ui { r, panels: &self.images.panels, img, text: &self.text, cursor: self.cursor, click };
        let (title, body) = if time { (38, 39) } else { (1, 16) };
        let t = ui.t(62, title);
        ui.centred(Font::LargeBlackOnLight, &t, x, y + 23.0, w);
        let t = ui.t(62, body);
        ui.wrapped(Font::NormalBlackOnLight, &t, x + 25.0, y + 55.0, 512.0);
        let by = y + 215.0;
        let mut choice = None;
        // (offset in the panel, width in blocks, label, choice)
        let buttons: Vec<(f32, i32, usize, MenuAction)> = match (replay, time) {
            (false, _) => vec![(64.0, 26, 6, MenuAction::MainMenu)],
            (true, false) => vec![(64.0, 9, 6, MenuAction::MainMenu), (352.0, 9, 37, MenuAction::Replay)],
            (true, true) => vec![(32.0, 9, 40, MenuAction::LowerDifficulty), (208.0, 9, 6, MenuAction::MainMenu), (384.0, 9, 37, MenuAction::Replay)],
        };
        for (dx, blocks, label, action) in buttons {
            let (bx, bw) = (x + dx, blocks as f32 * 16.0);
            let rect = [bx, by, bw, 25.0];
            osiris_ui::panel::large_label(ui.r, ui.panels, bx, by, blocks, ui.hot(rect) as u32);
            let t = ui.t(62, label);
            ui.centred(Font::NormalBlackOnLight, &t, bx, by + 6.0, bw);
            if ui.clicked(rect) {
                choice = Some(action);
            }
        }
        if let Some(a) = choice {
            self.sound("BUTTON.WAV");
            if a == MenuAction::LowerDifficulty {
                self.world.lower_difficulty_for_time();
                self.difficulty_changed = true;
            } else {
                self.request = Some(a);
            }
        }
    }
}
