//! A running city: the world, the view onto it, and the player's current tool.

use crate::city_view::{self, CityView, Highlight};
use osiris_render::{Renderer, Space};
use osiris_sim::{Command, Outcome, World};
use osiris_ui::{Font, draw_text, font};

/// Milliseconds per simulation tick at game speeds 100%, 90%, ... 10%.
const MS_PER_TICK: [f32; 10] = [20.0, 35.0, 55.0, 80.0, 110.0, 160.0, 240.0, 350.0, 500.0, 700.0];

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    None,
    Road,
    Clear,
}

pub struct Game {
    pub world: World,
    pub view: CityView,
    pub tool: Tool,
    /// Game speed in percent, 10..=100.
    pub speed: u32,
    pub paused: bool,
    hover: Option<(i32, i32)>,
    drag_start: Option<(i32, i32)>,
    accumulator: f32,
    message: Option<(String, f32)>,
}

impl Game {
    pub fn new(world: World) -> Self {
        Self {
            world,
            view: CityView::default(),
            tool: Tool::None,
            speed: 70,
            paused: false,
            hover: None,
            drag_start: None,
            accumulator: 0.0,
            message: None,
        }
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

    pub fn set_hover(&mut self, r: &Renderer, screen: [f32; 2]) {
        let world = r.screen_to_world(screen);
        self.hover = city_view::world_to_tile(&self.world.map, world);
    }

    fn pending_command(&self) -> Option<Command> {
        let end = self.hover?;
        let start = self.drag_start.unwrap_or(end);
        match self.tool {
            Tool::None => None,
            Tool::Road => Some(Command::Road { start, end }),
            Tool::Clear => Some(Command::Clear {
                x0: start.0,
                y0: start.1,
                x1: end.0,
                y1: end.1,
            }),
        }
    }

    pub fn press(&mut self) {
        if self.tool != Tool::None {
            self.drag_start = self.hover;
        }
    }

    pub fn release(&mut self) {
        if let Some(cmd) = self.pending_command() {
            match self.world.apply(&cmd) {
                Outcome::NotEnoughMoney => self.say("Not enough money"),
                Outcome::Blocked => self.say("Can't build there"),
                Outcome::Done { .. } => {}
            }
        }
        self.drag_start = None;
    }

    pub fn cancel(&mut self) {
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
                self.hover
                    .map(|(x, y)| Highlight { x, y, color: [1.0, 1.0, 1.0, 0.25] })
                    .into_iter()
                    .collect(),
                None,
            );
        };
        let cost = match self.world.estimate(&cmd) {
            Outcome::Done { cost, .. } => Some(cost),
            _ => None,
        };
        let affordable = cost.is_some_and(|c| c <= self.world.treasury);
        let color = if affordable { ok } else { bad };
        let tiles: Vec<(i32, i32)> = match cmd {
            Command::Road { start, end } => self.world.road_path(start, end).unwrap_or_else(|| vec![end]),
            Command::Clear { x0, y0, x1, y1 } => {
                let mut v = Vec::new();
                for y in y0.min(y1)..=y0.max(y1) {
                    for x in x0.min(x1)..=x0.max(x1) {
                        v.push((x, y));
                    }
                }
                v
            }
        };
        (
            tiles.into_iter().map(|(x, y)| Highlight { x, y, color }).collect(),
            cost,
        )
    }

    pub fn draw(&mut self, r: &mut Renderer) {
        let (marks, cost) = self.highlights();
        let marker = self.world.defs.terrain.empty_land;
        self.view.draw(r, &self.world.map, &marks, marker);
        self.draw_top_bar(r, cost);
    }

    fn draw_top_bar(&mut self, r: &mut Renderer, cost: Option<i32>) {
        let w = r.screen[0];
        r.rect([0.0, 0.0], [w, 22.0], [0.18, 0.12, 0.06, 0.92], Space::Screen);
        let t = &self.world.time;
        let year = if t.year < 0 {
            format!("{} BC", -t.year)
        } else {
            format!("{} AD", t.year)
        };
        let status = format!(
            "Deben {}    {} {}    Speed {}%{}",
            self.world.treasury,
            MONTHS[t.month as usize],
            year,
            self.speed,
            if self.paused { "  (paused)" } else { "" }
        );
        draw_text(r, Font::NormalWhiteOnDark, &status, 8.0, 5.0, font::WHITE);
        let title = &self.world.scenario_name;
        let tw = osiris_ui::text_width(r, Font::NormalWhiteOnDark, title) as f32;
        draw_text(r, Font::NormalWhiteOnDark, title, w - tw - 8.0, 5.0, font::WHITE);
        let tool = match self.tool {
            Tool::None => "",
            Tool::Road => "Road",
            Tool::Clear => "Clear land",
        };
        let mut line = tool.to_owned();
        if let Some(c) = cost.filter(|&c| c > 0) {
            line = format!("{tool}: {c} Db");
        }
        if !line.is_empty() {
            draw_text(r, Font::SmallOutlined, &line, 8.0, 28.0, font::WHITE);
        }
        if let Some((m, _)) = &self.message {
            let mw = osiris_ui::text_width(r, Font::LargeBlackOnDark, m) as f32;
            draw_text(r, Font::LargeBlackOnDark, m, (w - mw) / 2.0, 60.0, font::WHITE);
        }
    }
}
