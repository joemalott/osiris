//! The building information window opened by clicking a building, with the original
//! game's descriptions and house advice.

use osiris_formats::TextTable;
use osiris_render::Renderer;
use osiris_sim::World;
use osiris_sim::houses::Need;
use osiris_ui::rich_text::{self, Options, RendererMeasure};
use osiris_ui::{Font, PanelImages, draw_text, font, panel, text_width};

const W_BLOCKS: i32 = 29;
const H_BLOCKS: i32 = 18;
const TEXT_HOUSE: usize = 127;
const TEXT_HOUSE_LEVELS: usize = 29;
const TEXT_RESOURCES: usize = 23;

pub struct InfoPanel {
    pub building: u32,
}

/// Group 127 advice line for a house's first unmet need.
fn house_advice(need: Option<Need>, decaying: bool, food_types_have: i32) -> usize {
    let Some(need) = need else { return 101 };
    let (evolve, devolve) = match need {
        Need::Desirability => (70, 40),
        Need::Water => (71, 41),
        Need::WaterSupply => (72, 42),
        Need::Entertainment => (73, 43),
        Need::Education => (84, 54),
        Need::Religion => (90, 60),
        Need::Dentist => (93, 63),
        Need::Physician | Need::Health => (94, 64),
        Need::Food => (79 + food_types_have.clamp(0, 2) as usize, 49 + food_types_have.clamp(0, 2) as usize),
        Need::Pottery => (89, 59),
        Need::Linen => (97, 67),
        Need::Jewelry => (108, 110),
        Need::Beer => (99, 69),
    };
    if decaying { devolve } else { evolve }
}

impl InfoPanel {
    fn origin(r: &Renderer) -> [f32; 2] {
        let w = (W_BLOCKS * 16) as f32;
        let h = (H_BLOCKS * 16) as f32;
        [((r.screen[0] - crate::sidebar::WIDTH - w) / 2.0).max(0.0), ((r.screen[1] - h) / 2.0).max(30.0)]
    }

    pub fn contains(&self, r: &Renderer, p: [f32; 2]) -> bool {
        let o = Self::origin(r);
        p[0] >= o[0] && p[1] >= o[1] && p[0] < o[0] + (W_BLOCKS * 16) as f32 && p[1] < o[1] + (H_BLOCKS * 16) as f32
    }

    pub fn draw(&self, r: &mut Renderer, panels: &PanelImages, world: &World, text: &TextTable) {
        let Some(b) = world.buildings.get(self.building) else { return };
        let def = world.defs.building(b.kind);
        let group = def.and_then(|d| d.text_id).filter(|&g| g > 0).map(|g| g as usize);
        let [x, y] = Self::origin(r);
        panel::outer_panel(r, panels, x, y, W_BLOCKS, H_BLOCKS);
        let w = (W_BLOCKS * 16) as f32;
        let title = match &b.house {
            Some(h) if h.population <= 0 => text.get(28, 10).unwrap_or("Vacant lot").to_owned(),
            Some(h) => text.get(TEXT_HOUSE_LEVELS, h.level as usize).unwrap_or("House").to_owned(),
            None => group
                .and_then(|g| text.get(g, 0))
                .or_else(|| text.get(28, b.kind as usize))
                .unwrap_or("Building")
                .to_owned(),
        };
        let tw = text_width(r, Font::LargeBlackOnLight, &title) as f32;
        draw_text(r, Font::LargeBlackOnLight, &title, x + (w - tw) / 2.0, y + 16.0, font::BLACK);

        let mut lines: Vec<String> = Vec::new();
        if let Some(h) = &b.house {
            let cap = world.balance.house(h.level).max_people * b.size * b.size;
            lines.push(format!("{} {}    {} {}", h.population, text.get(TEXT_HOUSE, 20).unwrap_or("occupants"), text.get(TEXT_HOUSE, 22).unwrap_or("Extra room for"), (cap - h.population).max(0)));
            let food_types = h.foods.iter().filter(|&&f| f > 0).count() as i32;
            if h.population > 0 {
                let advice = if h.level as usize + 1 >= world.balance.houses.len() && h.blocked_by.is_none() {
                    100
                } else {
                    house_advice(h.blocked_by, h.decaying, food_types)
                };
                lines.push(text.get(TEXT_HOUSE, advice).unwrap_or("").to_owned());
            } else if let Some(desc) = text.get(TEXT_HOUSE, 33) {
                lines.push(desc.to_owned());
            }
            let foods: Vec<String> = [(1, 0), (8, 1), (7, 2), (3, 3)]
                .iter()
                .filter(|&&(_, slot)| h.foods[slot] > 0)
                .map(|&(res, slot)| format!("{} {}", text.get(TEXT_RESOURCES, res).unwrap_or("?"), h.foods[slot]))
                .collect();
            if !foods.is_empty() {
                lines.push(foods.join(", "));
            }
            if h.coverage.tax <= 0 && h.population > 0 {
                lines.push(text.get(TEXT_HOUSE, 23).unwrap_or("").to_owned());
            }
            lines.push(format!("{} {}", text.get(TEXT_HOUSE, 3).unwrap_or("Desirability"), b.desirability));
        } else {
            if let Some(desc) = group.and_then(|g| text.get(g, 1)) {
                lines.push(desc.to_owned());
            }
            let needed = world.workers_needed(b.kind);
            if needed > 0 {
                let access = world.has_labor_access(b.id);
                lines.push(if b.road.is_none() {
                    "This building has no road access.".to_owned()
                } else if !access {
                    "No one lives close enough to work here.".to_owned()
                } else {
                    format!("{} of {} workers employed", b.workers, needed)
                });
            }
            let stock: Vec<String> = b
                .stock
                .iter()
                .enumerate()
                .filter(|&(_, &v)| v > 0)
                .map(|(i, &v)| format!("{} {}", text.get(TEXT_RESOURCES, i).unwrap_or("?"), v))
                .collect();
            if !stock.is_empty() {
                lines.push(stock.join(", "));
            }
        }
        let body_x = x + 16.0;
        let body_y = y + 48.0;
        let bw = W_BLOCKS - 2;
        let bh = H_BLOCKS - 5;
        panel::inner_panel(r, panels, body_x, body_y, bw, bh);
        let opts = Options {
            font: Font::NormalBlackOnLight,
            width: bw * 16 - 20,
            ..Default::default()
        };
        let mut cy = body_y + 8.0;
        for line in lines {
            let laid = rich_text::layout(&line, &opts, &mut RendererMeasure::new(r));
            rich_text::draw(r, &laid, [body_x + 10.0, cy], laid.height as f32, 0.0, font::BLACK);
            cy += laid.height as f32 + 10.0;
        }
        draw_text(r, Font::SmallPlain, "Right-click to close", x + 16.0, y + (H_BLOCKS * 16) as f32 - 22.0, font::BLACK);
    }
}
