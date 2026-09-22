//! A running city: the world, the view onto it, the sidebar and the player's tool.

use crate::city_view::{self, CityView, Highlight, Overlay, Sprite};
use crate::sidebar::{Category, Click, MenuItem, Sidebar, SidebarImages};
use crate::info::InfoPanel;
use crate::minimap::Minimap;
use osiris_audio::Audio;
use osiris_formats::{Message, MessageTable, TextTable};
use osiris_render::Renderer;
use osiris_sim::buildings::kind;
use osiris_sim::{Command, Outcome, World};
use osiris_ui::dialog::MessageDialog;
use osiris_ui::{Font, draw_text, font};
use std::sync::Arc;

/// Milliseconds per simulation tick at game speeds 100%, 90%, ... 10%.
const MS_PER_TICK: [f32; 10] = [20.0, 35.0, 55.0, 80.0, 110.0, 160.0, 240.0, 350.0, 500.0, 700.0];

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
    images: SidebarImages,
    text: Arc<TextTable>,
    entries: Vec<Entry>,
    hover: Option<(i32, i32)>,
    cursor: [f32; 2],
    drag_start: Option<(i32, i32)>,
    accumulator: f32,
    message: Option<(String, f32)>,
    messages: Arc<MessageTable>,
    dialog: Option<MessageDialog>,
    pub info: Option<InfoPanel>,
    minimap: Option<Minimap>,
    pub audio: Option<Arc<Audio>>,
    music_timer: f32,
    /// Map-changing actions since the minimap was last rebuilt.
    map_changed: bool,
}

impl Game {
    pub fn new(
        world: World,
        images: SidebarImages,
        text: Arc<TextTable>,
        messages: Arc<MessageTable>,
        audio: Option<Arc<Audio>>,
    ) -> Self {
        Self {
            world,
            view: CityView::default(),
            tool: Tool::None,
            speed: 70,
            paused: false,
            sidebar: Sidebar::default(),
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
        let Some(key) = self.world.messages.pop_front() else { return };
        let msg = if key == "victory" {
            Message {
                title: "Victory!".to_owned(),
                content: format!(
                    "@PYou have met every goal set for {}. The people of Egypt rejoice at your success.",
                    self.world.scenario_name
                ),
                size: (30, 16),
                ..Default::default()
            }
        } else {
            let Some(m) = osiris_sim::missions::message_id(&key).and_then(|id| self.messages.get(id as usize)) else {
                return;
            };
            m.clone()
        };
        self.dialog = Some(MessageDialog::new(r, &msg, &self.text));
        self.sound("BUTTON.WAV");
    }


    pub fn close_dialog(&mut self) {
        self.dialog = None;
        self.world.messages.clear();
    }

    pub fn scroll_dialog(&mut self, delta: f32, screen: [f32; 2]) -> bool {
        match &mut self.dialog {
            Some(d) => {
                d.scroll(delta, screen);
                true
            }
            None => false,
        }
    }

    fn building_name(&self, k: u16) -> String {
        self.text
            .get(TEXT_BUILDING_NAMES, k as usize)
            .map(str::to_owned)
            .or_else(|| self.world.defs.building(k).map(|d| d.key.replace('_', " ")))
            .unwrap_or_else(|| format!("#{k}"))
    }

    pub fn update(&mut self, dt: f32) {
        if let Some((_, t)) = &mut self.message {
            *t -= dt;
            if *t <= 0.0 {
                self.message = None;
            }
        }
        if let Some(a) = &self.audio {
            self.music_timer -= dt;
            if self.music_timer <= 0.0 {
                a.update_music(self.world.population);
                self.music_timer = 5.0;
            }
        }
        if self.paused || self.dialog.is_some() {
            return;
        }
        let ms = MS_PER_TICK[((100 - self.speed.clamp(10, 100)) / 10) as usize];
        self.accumulator += dt * 1000.0;
        let mut ticks = 0;
        while self.accumulator >= ms && ticks < 50 {
            self.accumulator -= ms;
            self.world.tick();
            ticks += 1;
        }
        if ticks == 50 {
            self.accumulator = 0.0;
        }
    }

    pub fn set_cursor(&mut self, r: &Renderer, screen: [f32; 2]) {
        self.cursor = screen;
        if let Some(d) = &mut self.dialog {
            d.hover(screen, r.screen);
            self.hover = None;
            return;
        }
        self.sidebar.hover(r.screen[0], screen);
        if self.sidebar.contains(r.screen[0], screen) {
            self.hover = None;
            return;
        }
        let world = r.screen_to_world(screen);
        self.hover = city_view::world_to_tile(&self.world.map, world);
    }

    fn footprint_origin(&self, k: u16, tile: (i32, i32)) -> (i32, i32) {
        // The cursor sits on the footprint's middle tile.
        let s = self.world.size_of(k);
        (tile.0 - (s - 1) / 2, tile.1 - (s - 1) / 2)
    }

    fn pending_command(&self) -> Option<Command> {
        let end = self.hover?;
        let start = self.drag_start.unwrap_or(end);
        match self.tool {
            Tool::None => None,
            Tool::Road => Some(Command::Road { start, end }),
            Tool::Clear => Some(Command::Clear { x0: start.0, y0: start.1, x1: end.0, y1: end.1 }),
            Tool::Build(k) if k == kind::VACANT_LOT => {
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
                let label_id = SUBMENUS.iter().find(|(k, _)| *k == sub).map(|&(_, id)| id);
                let label = label_id.map_or_else(|| sub.replace('_', " "), |id| self.building_name(id));
                self.entries.push(Entry::Submenu(sub.to_owned()));
                self.sidebar.items.push(MenuItem { label: format!("{label} ..."), cost: 0, enabled: true });
            } else if let Some(d) = defs.building_by_key(item).filter(|d| self.world.is_allowed(d.id)) {
                self.entries.push(Entry::Building(d.id));
                self.sidebar.items.push(MenuItem {
                    label: self.building_name(d.id),
                    cost: self.world.cost_of(d.id as usize),
                    enabled: true,
                });
            }
        }
        self.sidebar.open = Some(category);
    }

    fn choose_category(&mut self, c: Category) {
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
        if let Some(i) = &self.info
            && !i.contains(r, self.cursor)
        {
            self.info = None;
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
        let panel_left = screen_w - 162.0;
        if let Some(m) = &self.minimap
            && m.contains(panel_left, self.cursor)
        {
            return m.pixel_to_tile(&self.world.map, panel_left, self.cursor);
        }
        match self.sidebar.click(screen_w, self.cursor) {
            Click::Button(c) => {
                self.sound("BUTTON.WAV");
                self.choose_category(c);
            }
            Click::Item(i) => match self.entries.get(i).cloned() {
                Some(Entry::Building(k)) => {
                    self.tool = match k {
                        5 => Tool::Road,
                        9 => Tool::Clear,
                        k => Tool::Build(k),
                    };
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
                } else if let Some((x, y)) = self.hover {
                    let id = self.world.map.building.at_or(x, y, 0);
                    if id != 0 {
                        self.info = Some(InfoPanel { building: id });
                        self.sound("BUTTON.WAV");
                    }
                }
            }
        }
        None
    }

    pub fn release(&mut self) {
        if self.drag_start.is_none() {
            return;
        }
        if let Some(cmd) = self.pending_command() {
            match self.world.apply(&cmd) {
                Outcome::NotEnoughMoney => self.say("Not enough money"),
                Outcome::Blocked => self.say("Can't build there"),
                Outcome::Invalid(why) => self.say(why),
                Outcome::Done { items, .. } => {
                    if items > 0 {
                        self.map_changed = true;
                        self.sound(if matches!(cmd, Command::Clear { .. }) { "BUILD.WAV" } else { "BUILD.WAV" });
                    }
                }
            }
        }
        self.drag_start = None;
    }

    pub fn cancel(&mut self) {
        if self.dialog.take().is_some() || self.info.take().is_some() {
            return;
        }
        if self.sidebar.open.take().is_some() {
            return;
        }
        if self.drag_start.take().is_none() {
            self.tool = Tool::None;
        }
    }

    fn say(&mut self, text: &str) {
        self.message = Some((text.to_owned(), 3.0));
    }

    fn highlights(&mut self) -> (Vec<Highlight>, Option<i32>) {
        let ok = [0.3, 1.0, 0.3, 0.45];
        let bad = [1.0, 0.2, 0.2, 0.45];
        let Some(cmd) = self.pending_command() else {
            return (
                self.hover.map(|(x, y)| Highlight { x, y, color: [1.0, 1.0, 1.0, 0.25] }).into_iter().collect(),
                None,
            );
        };
        let est = self.world.estimate(&cmd);
        let cost = match est {
            Outcome::Done { cost, .. } => Some(cost),
            _ => None,
        };
        let affordable = cost.is_some_and(|c| c <= self.world.treasury);
        let color = if affordable { ok } else { bad };
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
            Command::Build { kind: k, x, y, .. } => {
                let s = self.world.size_of(k);
                rect(x, y, x + s - 1, y + s - 1)
            }
        };
        (tiles.into_iter().map(|(x, y)| Highlight { x, y, color }).collect(), cost)
    }

    fn sprites(&self) -> Vec<Sprite> {
        let defs = &self.world.defs;
        self.world
            .figures
            .iter()
            .filter_map(|f| {
                let walk = defs.figure(f.kind)?.anims.get("walk")?;
                let frame = if f.moving { f.frame(walk.frames.max(1)) } else { 0 };
                Some(Sprite {
                    x: f.x,
                    y: f.y,
                    offset: f.pixel_offset(),
                    image: walk.image + f.direction as u32 + 8 * frame,
                })
            })
            .collect()
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
        out
    }

    pub fn draw(&mut self, r: &mut Renderer) {
        self.next_dialog(r);
        let (marks, cost) = self.highlights();
        let marker = self.world.defs.terrain.empty_land;
        let sprites = self.sprites();
        let overlays = self.overlays(r);
        self.view.draw(r, &self.world.map, &marks, marker, &sprites, &overlays);
        self.draw_overlay(r, cost);
    }

    fn draw_overlay(&mut self, r: &mut Renderer, cost: Option<i32>) {
        let t = &self.world.time;
        let month = self.text.get(TEXT_MONTHS, t.month as usize).unwrap_or("?");
        let year = if t.year < 0 { format!("{} BC", -t.year) } else { format!("{} AD", t.year) };
        let status = format!(
            "Deben {}      Pop {}      {} {}      Speed {}%{}",
            self.world.treasury,
            self.world.population,
            month,
            year,
            self.speed,
            if self.paused { " (paused)" } else { "" }
        );
        self.sidebar.draw(r, &self.images, &status, &self.world.scenario_name);
        let minimap = self.minimap.get_or_insert_with(|| Minimap::new(r));
        // The simulation changes terrain too (fires, rubble); refresh about once a second.
        if self.map_changed || self.world.time.tick == 0 {
            minimap.mark_dirty();
            self.map_changed = false;
        }
        let buildings = &self.world.buildings;
        let is_house = |id: u32| buildings.get(id).is_some_and(|b| b.house.is_some());
        minimap.draw(r, &self.world.map, is_house, r.screen[0] - 162.0);
        let tool = match self.tool {
            Tool::None => String::new(),
            Tool::Road => self.building_name(5),
            Tool::Clear => self.building_name(9),
            Tool::Build(k) => self.building_name(k),
        };
        let line = match cost.filter(|&c| c > 0) {
            Some(c) => format!("{tool}: {c} Db"),
            None => tool,
        };
        if !line.is_empty() {
            draw_text(r, Font::SmallOutlined, &line, 10.0, 38.0, font::WHITE);
        }
        if let Some((m, _)) = &self.message {
            let w = r.screen[0] - crate::sidebar::WIDTH;
            let mw = osiris_ui::text_width(r, Font::LargeBlackOnDark, m) as f32;
            draw_text(r, Font::LargeBlackOnDark, m, (w - mw) / 2.0, 70.0, font::WHITE);
        }
        if let Some(i) = &self.info {
            i.draw(r, &self.images.panels, &self.world, &self.text);
        }
        if let Some(d) = &self.dialog {
            d.draw(r);
        }
    }
}
