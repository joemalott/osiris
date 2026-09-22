//! A running city: the world, the view onto it, the sidebar and the player's tool.

use crate::city_view::{self, CityView, Highlight, Sprite};
use crate::sidebar::{Category, Click, MenuItem, Sidebar, SidebarImages};
use osiris_formats::TextTable;
use osiris_render::Renderer;
use osiris_sim::buildings::kind;
use osiris_sim::{Command, Outcome, World};
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
}

impl Game {
    pub fn new(world: World, images: SidebarImages, text: Arc<TextTable>) -> Self {
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
        if self.paused {
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
            } else if let Some(d) = defs.building_by_key(item) {
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

    pub fn press(&mut self, screen_w: f32) {
        match self.sidebar.click(screen_w, self.cursor) {
            Click::Button(c) => self.choose_category(c),
            Click::Item(i) => match self.entries.get(i).cloned() {
                Some(Entry::Building(k)) => {
                    self.tool = match k {
                        5 => Tool::Road,
                        9 => Tool::Clear,
                        k => Tool::Build(k),
                    };
                    self.sidebar.open = None;
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
                if self.tool != Tool::None {
                    self.drag_start = self.hover;
                }
            }
        }
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
                Outcome::Done { .. } => {}
            }
        }
        self.drag_start = None;
    }

    pub fn cancel(&mut self) {
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

    pub fn draw(&mut self, r: &mut Renderer) {
        let (marks, cost) = self.highlights();
        let marker = self.world.defs.terrain.empty_land;
        let sprites = self.sprites();
        self.view.draw(r, &self.world.map, &marks, marker, &sprites);
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
    }
}
