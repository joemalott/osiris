//! The building windows, one layout per building type as the original draws them:
//! the window's height, its title, what it holds, which of its type's lines says
//! what it is doing (and under which conditions), and where its employee panel sits.
//!
//! Positions are pixels from the window's top left. Text lines start at x 32 and wrap
//! at 400 pixels unless noted.

use super::{InfoAction, InfoPanel, TEXT_FRAME, TEXT_GENERAL, TEXT_RESOURCES, TEXT_YARD, city_foods};
use crate::widgets::Ui;
use osiris_sim::{Command, World};
use osiris_sim::buildings::Building;
use osiris_sim::economy::resource;
use osiris_ui::{Font, panel};

/// Building types by the original's numbering.
mod k {
    pub const WATER_LIFT: u16 = 7;
    pub const BANDSTAND: u16 = 30;
    pub const BOOTH: u16 = 31;
    pub const SENET_HOUSE: u16 = 32;
    pub const PAVILION: u16 = 33;
    pub const CONSERVATORY: u16 = 34;
    pub const SENET_MASTER_SCHOOL: u16 = 37;
    pub const STATUE_SMALL: u16 = 41;
    pub const STATUE_LARGE: u16 = 43;
    pub const APOTHECARY: u16 = 46;
    pub const MORTUARY: u16 = 47;
    pub const DENTIST: u16 = 49;
    pub const SCRIBAL_SCHOOL: u16 = 51;
    pub const LIBRARY: u16 = 53;
    pub const POLICE: u16 = 55;
    pub const MUD_GATEHOUSE: u16 = 58;
    pub const MUD_TOWER: u16 = 59;
    pub const TEMPLE_FIRST: u16 = 60;
    pub const COMPLEX_LAST: u16 = 69;
    pub const BAZAAR: u16 = 70;
    pub const GRANARY: u16 = 71;
    pub const STORAGE_YARD: u16 = 72;
    pub const SHIPWRIGHT: u16 = 74;
    pub const DOCK: u16 = 75;
    pub const FISHING_WHARF: u16 = 76;
    pub const MANSION_FIRST: u16 = 77;
    pub const MANSION_LAST: u16 = 79;
    pub const ARCHITECT: u16 = 81;
    pub const VILLAGE_PALACE: u16 = 187;
    pub const TOWN_PALACE: u16 = 188;
    pub const TAX_COLLECTOR: u16 = 86;
    pub const TAX_COLLECTOR_2: u16 = 87;
    pub const WELL: u16 = 92;
    pub const ACADEMY: u16 = 94;
    pub const BURNING_RUIN: u16 = 99;
    pub const HUNTING_LODGE: u16 = 115;
    pub const FERRY: u16 = 136;
    pub const ROADBLOCK: u16 = 138;
    pub const SHRINE_FIRST: u16 = 140;
    pub const SHRINE_LAST: u16 = 144;
    pub const FIREHOUSE: u16 = 167;
    pub const CLAY_GATEHOUSE: u16 = 170;
    pub const BRICK_GATEHOUSE: u16 = 171;
    pub const BRICK_TOWER: u16 = 172;
    pub const CLAY_TOWER: u16 = 173;
    pub const CARPENTERS: u16 = 177;
    pub const BRICKLAYERS: u16 = 178;
    pub const STONEMASONS: u16 = 179;
    pub const WATER_SUPPLY: u16 = 180;
    pub const TRANSPORT_WHARF: u16 = 181;
    pub const WARSHIP_WHARF: u16 = 182;
    pub const COURTHOUSE: u16 = 184;
    pub const ACADEMY_2: u16 = 185;
    pub const ACADEMY_3: u16 = 186;
    pub const CITY_PALACE: u16 = 189;
    pub const CATTLE_RANCH: u16 = 194;
    pub const REED_GATHERER: u16 = 195;
    pub const WORK_CAMP: u16 = 199;
    pub const GATEHOUSE_2: u16 = 200;
    pub const CLAY_GATEHOUSE_2: u16 = 201;
    pub const DECORATIVE_GATEHOUSE: u16 = 202;
    pub const PHYSICIAN: u16 = 206;
    pub const FESTIVAL_SQUARE: u16 = 209;
    pub const ZOO: u16 = 226;
    pub const ARTISANS: u16 = 231;
    pub const TOWER_GATEHOUSE: u16 = 302;
}

/// Resources by the original's numbering, where the windows name them.
mod r {
    pub const BARLEY: u16 = 14;
    pub const FLAX: u16 = 16;
    pub const WEAPONS: u16 = 10;
    pub const CLAY: u16 = 11;
    pub const BRICKS: u16 = 12;
    pub const GEMS: u16 = 18;
    pub const STONE: u16 = 24;
    pub const LIMESTONE: u16 = 25;
    pub const GRANITE: u16 = 26;
    pub const CHARIOTS: u16 = 28;
    pub const COPPER: u16 = 29;
    pub const SANDSTONE: u16 = 30;
    pub const OIL: u16 = 31;
    pub const HENNA: u16 = 32;
    pub const PAINT: u16 = 33;
    pub const LAMPS: u16 = 34;
    pub const MARBLE: u16 = 35;
}

/// A workshop: its text group, what it makes, and what it makes it from.
fn workshop(kind: u16) -> Option<(usize, u16, &'static [u16])> {
    use resource::*;
    Some(match kind {
        110 => (122, BEER, &[r::BARLEY]),
        111 => (123, LINEN, &[r::FLAX]),
        112 => (124, r::WEAPONS, &[r::COPPER]),
        113 => (125, LUXURY_GOODS, &[r::GEMS]),
        114 => (126, POTTERY, &[r::CLAY]),
        203 => (190, PAPYRUS, &[REEDS]),
        205 => (185, r::CHARIOTS, &[TIMBER]),
        233 => (313, r::PAINT, &[r::HENNA]),
        204 => (180, r::BRICKS, &[r::CLAY, STRAW]),
        232 => (314, r::LAMPS, &[r::OIL, POTTERY]),
        _ => return None,
    })
}

/// A quarry, mine or wood cutter: its text group and what it digs or cuts.
fn raw_material(kind: u16) -> Option<(usize, u16)> {
    Some(match kind {
        106 => (118, r::STONE),
        107 => (119, r::LIMESTONE),
        108 => (120, resource::TIMBER),
        109 => (121, r::CLAY),
        161 => (162, resource::GOLD),
        162 => (163, r::GEMS),
        216 => (192, r::GRANITE),
        217 => (193, r::COPPER),
        221 => (194, r::SANDSTONE),
        _ => return None,
    })
}

/// A farm: its text group and its crop. The meadow farms take their crop's.
fn farm(kind: u16) -> Option<(usize, u16)> {
    use resource::*;
    let kind = match kind {
        311..=318 => [100, 101, 102, 103, 104, 105, 224, 196][(kind - 311) as usize],
        k => k,
    };
    Some(match kind {
        100 => (181, r::BARLEY),
        101 => (115, r::FLAX),
        102 => (112, GRAIN),
        103 => (113, LETTUCE),
        104 => (114, POMEGRANATES),
        105 => (182, CHICKPEAS),
        196 => (183, FIGS),
        224 => (306, r::HENNA),
        _ => return None,
    })
}

/// The pick among five lines by staffing: full, three quarters, half, a quarter, less.
fn by_staff(pct: i32, [full, most, half, quarter, least]: [usize; 5]) -> usize {
    match pct {
        p if p >= 100 => full,
        p if p >= 75 => most,
        p if p >= 50 => half,
        p if p >= 25 => quarter,
        _ => least,
    }
}

/// The original's labor line for a building's employee panel (group 69): no people,
/// no houses in reach, too few houses in reach, or ask the overseer. A fully staffed
/// building only warns of poor access. None when there is nothing to say.
fn labor_line(workers: i32, needed: i32, population: i32, houses: i32) -> Option<usize> {
    if workers < needed {
        Some(if population <= 0 {
            16
        } else if houses < 1 {
            17
        } else if houses >= 40 {
            18
        } else {
            20
        })
    } else if houses >= 40 {
        None
    } else {
        Some(20)
    }
}

/// A stock row: the resource's icon and a line beside it.
struct Row {
    resource: u16,
    icon: [f32; 2],
    text: String,
    at: [f32; 2],
    /// Drawn in yellow: a bazaar's goods it has been told not to buy.
    yellow: bool,
}

/// A building's window.
#[derive(Default)]
struct Page {
    /// Height in 16-pixel blocks (all are 29 wide).
    hb: i32,
    title: String,
    /// The output's icon at the top left.
    icon: Option<u16>,
    rows: Vec<Row>,
    /// Text lines: y, text and wrap width, from x 32; `line` defaults the width to 400
    /// pixels, `line_narrow` is for text that shares its row with a picture.
    lines: Vec<(f32, String, f32)>,
    /// The dark panel: its y and height in blocks.
    panel: Option<(f32, i32)>,
    /// The employee row's y.
    staff: Option<f32>,
    /// Lines on the dark panel: x, y and text.
    dark: Vec<(f32, f32, String)>,
    /// Single black lines: x, y and text.
    labels: Vec<(f32, f32, String)>,
    /// A picture and where it goes.
    picture: Option<(u32, [f32; 2])>,
}

impl Page {
    fn new(hb: i32, title: String) -> Self {
        Self { hb, title, ..Default::default() }
    }

    fn line(&mut self, y: f32, text: String) {
        if !text.is_empty() {
            self.lines.push((y, text, 400.0));
        }
    }

    /// A line sharing its row with a picture, wrapped to `width` pixels instead of 400.
    fn line_narrow(&mut self, y: f32, text: String, width: f32) {
        if !text.is_empty() {
            self.lines.push((y, text, width));
        }
    }

    /// The standard dark panel under the text, 4 blocks at y 136, employee row at 142.
    fn staffed(&mut self) {
        self.panel = Some((136.0, 4));
        self.staff = Some(142.0);
    }

    /// The bottom text, `from_bottom` pixels above the window's bottom edge.
    fn bottom(&mut self, from_bottom: f32, text: String) {
        let y = self.hb as f32 * 16.0 - from_bottom;
        self.line(y, text);
    }
}

impl InfoPanel {
    /// Draws building `b`'s window, if the original has a layout for its type.
    pub(super) fn building_page(&mut self, ui: &mut Ui, world: &mut World, b: &Building) -> Option<Option<InfoAction>> {
        let page = page(ui, world, b)?;
        let ([x, y], closed) = self.frame(ui, 29, page.hb, &page.title);
        if let Some(r) = page.icon {
            ui.icon(r, x + 10.0, y + 10.0);
        }
        for row in &page.rows {
            ui.icon(row.resource, x + row.icon[0], y + row.icon[1]);
            let font = if row.yellow { Font::NormalYellow } else { Font::NormalBlackOnLight };
            ui.label(font, &row.text, x + row.at[0], y + row.at[1]);
        }
        for (lx, ly, text) in &page.labels {
            ui.label(Font::NormalBlackOnLight, text, x + lx, y + ly);
        }
        for (ly, text, width) in &page.lines {
            ui.wrapped(Font::NormalBlackOnLight, text, x + 32.0, y + ly, *width);
        }
        if let Some((py, blocks)) = page.panel {
            panel::inner_panel(ui.r, ui.panels, x + 16.0, y + py, 27, blocks);
        }
        if let Some(sy) = page.staff {
            staff_row(ui, world, b, x, y + sy, None);
        }
        for (dx, dy, text) in &page.dark {
            ui.wrapped(Font::NormalBlackOnDark, text, x + dx, y + dy, 368.0);
        }
        if let Some((image, [px, py])) = page.picture {
            ui.image(image, x + px, y + py);
        }
        let mut action = closed.then_some(InfoAction::Close);
        match b.kind {
            // The palace and the tax office set the tax rate.
            k::VILLAGE_PALACE | k::TOWN_PALACE | k::CITY_PALACE | k::TAX_COLLECTOR | k::TAX_COLLECTOR_2 => {
                let rate = format!("{} {}%", ui.t(60, 1), world.finance.tax_rate);
                ui.label(Font::NormalBlackOnLight, &rate, x + 260.0, y + 43.0);
                if ui.arrow(x + 406.0, y + 33.0, false) {
                    world.apply(&Command::TaxRate(world.finance.tax_rate - 1));
                }
                if ui.arrow(x + 430.0, y + 33.0, true) {
                    world.apply(&Command::TaxRate(world.finance.tax_rate + 1));
                }
            }
            k::STORAGE_YARD | k::GRANARY | k::BAZAAR => {
                let label = ui.t(TEXT_YARD, 2);
                if ui.button([x + 100.0, y + page.hb as f32 * 16.0 - 34.0, 272.0, 20.0], &label, Font::NormalBlackOnLight) {
                    self.orders = true;
                    self.scroll = 0;
                }
                if b.kind == k::STORAGE_YARD && self.trade_button(ui) {
                    action = Some(InfoAction::Overseer(crate::advisors::Advisor::Trade));
                }
            }
            _ => {}
        }
        Some(action)
    }
}

/// The employee row at `y`: the worker glyph, "N Employees (M needed)", and the
/// labor line (or `note`) under it.
pub(super) fn staff_row(ui: &mut Ui, world: &World, b: &Building, x: f32, y: f32, note: Option<String>) {
    let needed = world.workers_needed(b.kind);
    if needed <= 0 {
        return;
    }
    let houses = if world.rules.global_labor_pool && b.road.is_some() { 40 } else if world.has_labor_access(b.id) { b.houses_covered.max(1) } else { 0 };
    let desc = note.or_else(|| labor_line(b.workers, needed, world.population, houses).map(|i| ui.t(TEXT_FRAME, i)));
    ui.image(ui.img.context_icons + 14, x + 40.0, y + 6.0);
    let word = ui.t(TEXT_GENERAL, if b.workers == 1 { 12 } else { 13 });
    let line = format!("{} {} ({} {}", b.workers, word, needed, ui.t(TEXT_FRAME, 0));
    let ly = if desc.is_some() { y + 10.0 } else { y + 16.0 };
    // Both lines are in the dark font (FUN_004fc5c0).
    ui.label(Font::NormalBlackOnDark, &line, x + 60.0, ly);
    if let Some(d) = desc {
        ui.wrapped(Font::NormalBlackOnDark, &d, x + 70.0, y + 26.0, 380.0);
    }
}

/// "N Units".
fn units(ui: &Ui, n: i32) -> String {
    format!("{} {}", n.max(0), ui.t(TEXT_GENERAL, if n == 1 { 10 } else { 11 }))
}

/// "N Days".
fn days(ui: &Ui, n: i32) -> String {
    format!("{} {}", n, ui.t(TEXT_GENERAL, if n == 1 { 44 } else { 45 }))
}

fn stock(b: &Building, r: u16) -> i32 {
    b.stock.get(r as usize).copied().unwrap_or(0)
}

/// Building `b`'s window, or None for the types drawn elsewhere.
fn page(ui: &Ui, world: &World, b: &Building) -> Option<Page> {
    let t = |g: usize, i: usize| ui.t(g, i);
    let needed = world.workers_needed(b.kind).max(1);
    let pct = b.workers * 100 / needed;
    let no_road = b.road.is_none();
    let road_line = || ui.t(TEXT_FRAME, 25);
    let walker_out = b.walkers[0] != 0;
    let mothballed = |r: u16| world.is_mothballed(r);
    let def_group = world.defs.building(b.kind).and_then(|d| d.text_id).filter(|&g| g > 0).map(|g| g as usize);
    let kind = b.kind;

    if let Some((g, crop)) = farm(kind) {
        return Some(farm_page(ui, world, b, g, crop));
    }
    if let Some((g, out, inputs)) = workshop(kind) {
        // Progress, the input in store, and whether work can go on.
        let mut p = Page::new(16, t(g, 0));
        p.icon = Some(out);
        let progress = b.progress * 100 / world.max_progress(kind).max(1);
        p.line(40.0, format!("{} {}% {}", t(g, 2), progress, t(g, 3)));
        let two = inputs.len() > 1;
        for (i, &inp) in inputs.iter().enumerate() {
            let label = t(g, if two { 13 + i } else { 12 });
            let dx = 198.0 * i as f32;
            p.rows.push(Row { resource: inp, icon: [32.0 + dx, 56.0], text: format!("{} {}", label, units(ui, stock(b, inp))), at: [60.0 + dx, 60.0], yellow: false });
        }
        let missing = inputs.iter().position(|&inp| stock(b, inp) <= 0);
        let line = if no_road {
            road_line()
        } else if mothballed(out) {
            t(g, 4)
        } else if b.workers <= 0 {
            t(g, 5)
        } else if let Some(i) = missing {
            t(g, 11 + i)
        } else {
            t(g, by_staff(pct, [6, 7, 8, 9, 10]))
        };
        p.line(86.0, line);
        p.staffed();
        return Some(p);
    }
    if let Some((g, out)) = raw_material(kind) {
        let mut p = Page::new(22, t(g, 0));
        p.icon = Some(out);
        let line = if kind == 108 {
            // The wood cutter says what it holds.
            format!("{} {} {}", t(g, 2), stock(b, out), t(g, 3))
        } else {
            format!("{} {}% {}", t(g, 2), b.progress * 100 / world.max_progress(kind).max(1), t(g, 3))
        };
        p.line(44.0, line);
        let status = if no_road {
            road_line()
        } else if mothballed(out) {
            t(g, 4)
        } else if b.workers <= 0 {
            t(g, 5)
        } else {
            t(g, by_staff(pct, [6, 7, 8, 9, 10]))
        };
        p.line(70.0, status);
        p.staffed();
        p.bottom(113.0, t(g, 1));
        return Some(p);
    }
    let service = |g: usize, working: usize, idle: usize| {
        let mut p = Page::new(16, t(g, 0));
        p.line(56.0, if no_road { road_line() } else if b.workers <= 0 { t(g, idle) } else { t(g, working) });
        p.staffed();
        p
    };
    let page = match kind {
        k::APOTHECARY => service(81, 3, 2),
        k::PHYSICIAN => service(83, 3, 2),
        k::DENTIST => service(84, 3, 2),
        k::MORTUARY | k::SCRIBAL_SCHOOL | k::LIBRARY => {
            // What its walkers hand out: linen, or papyrus.
            let (g, r) = match kind {
                k::MORTUARY => (82, resource::LINEN),
                k::SCRIBAL_SCHOOL => (85, resource::PAPYRUS),
                _ => (87, resource::PAPYRUS),
            };
            let mut p = service(g, 3, 2);
            let n = stock(b, r);
            p.rows.push(Row { resource: r, icon: [20.0, 30.0], text: format!("{} {}", n, t(TEXT_RESOURCES, r as usize)), at: [40.0, 35.0], yellow: false });
            if kind == k::MORTUARY && !no_road && b.workers > 0 && n <= 0 {
                p.lines[0].1 = t(g, 4);
            }
            p
        }
        k::CONSERVATORY..=k::SENET_MASTER_SCHOOL => {
            let g = 75 + (kind - k::CONSERVATORY) as usize;
            let mut p = service(g, by_staff(pct, [2, 3, 4, 5, 6]), 7);
            p.staffed();
            p
        }
        k::ACADEMY | k::ACADEMY_2 | k::ACADEMY_3 => service(135, if b.workers < world.workers_needed(kind) { 1 } else { 3 }, 2),
        k::POLICE | k::ARCHITECT | k::FIREHOUSE => {
            // Whether its walker is out, then how well staffed it is.
            let g = match kind {
                k::POLICE => 88,
                k::ARCHITECT => 104,
                _ => 164,
            };
            let mut p = service(g, if walker_out { 2 } else { 3 }, 9);
            if !no_road && b.workers > 0 {
                p.line(72.0, t(g, by_staff(pct, [4, 5, 6, 7, 8])));
            }
            if kind == k::FIREHOUSE {
                p.hb = 22;
                p.bottom(113.0, t(g, 1));
            }
            p
        }
        k::TAX_COLLECTOR | k::TAX_COLLECTOR_2 => {
            const G: usize = 106;
            let mut p = Page::new(16, t(G, 0));
            let palaces = world.buildings.iter().filter(|p| matches!(p.kind, k::VILLAGE_PALACE | k::TOWN_PALACE | k::CITY_PALACE));
            let (any, working) = palaces.fold((false, false), |(_, w), p| (true, w || p.workers > 0));
            if no_road {
                p.line(56.0, road_line());
            } else {
                let id = if !any {
                    11
                } else if !working {
                    12
                } else if b.workers <= 0 {
                    10
                } else {
                    by_staff(pct, [5, 6, 7, 8, 9])
                };
                p.line(72.0, t(G, id));
            }
            p.staffed();
            p
        }
        k::COURTHOUSE => {
            const G: usize = 176;
            let mut p = Page::new(22, t(G, 0));
            if no_road {
                p.line(63.0, road_line());
            } else if b.workers <= 0 {
                p.line(63.0, t(G, 2));
            } else {
                p.line(63.0, t(G, if walker_out { 7 } else { 8 }));
                p.line(83.0, t(G, by_staff(pct, [6, 5, 4, 3, 2])));
            }
            p.staffed();
            p.bottom(113.0, t(G, 1));
            p
        }
        k::VILLAGE_PALACE | k::TOWN_PALACE | k::CITY_PALACE => {
            let mut p = Page::new(if kind == k::CITY_PALACE { 16 } else { 18 }, t(41, kind as usize));
            if no_road {
                p.line(56.0, road_line());
            }
            p.staffed();
            p
        }
        k::MANSION_FIRST..=k::MANSION_LAST => {
            let mut p = Page::new(16, t(41, kind as usize));
            p.bottom(143.0, t(103, 1));
            p
        }
        k::MUD_TOWER | k::BRICK_TOWER | k::CLAY_TOWER => {
            let g = match kind {
                k::MUD_TOWER => 91,
                k::BRICK_TOWER => 169,
                _ => 170,
            };
            let clay = kind == k::CLAY_TOWER;
            let mut p = Page::new(if clay { 22 } else { 16 }, t(g, 0));
            let id = if b.workers <= 0 {
                2
            } else if walker_out {
                3
            } else {
                4
            };
            p.line(if clay { 50.0 } else { 56.0 }, if no_road { road_line() } else { t(g, id) });
            p.staffed();
            if clay {
                p.line(220.0, t(g, 1));
            }
            p
        }
        k::MUD_GATEHOUSE
        | k::GATEHOUSE_2
        | k::CLAY_GATEHOUSE
        | k::CLAY_GATEHOUSE_2
        | k::BRICK_GATEHOUSE
        | k::DECORATIVE_GATEHOUSE
        | k::TOWER_GATEHOUSE
        | k::STATUE_SMALL..=k::STATUE_LARGE
        | k::ROADBLOCK => {
            // Only what it is for.
            let g = match kind {
                k::MUD_GATEHOUSE | k::GATEHOUSE_2 | k::TOWER_GATEHOUSE => 90,
                k::CLAY_GATEHOUSE | k::CLAY_GATEHOUSE_2 => 167,
                k::BRICK_GATEHOUSE | k::DECORATIVE_GATEHOUSE => 168,
                k::ROADBLOCK => 155,
                _ => 80,
            };
            let mut p = Page::new(if kind == k::ROADBLOCK { 22 } else { 16 }, t(g, 0));
            p.bottom(158.0, t(g, 1));
            p
        }
        k::WELL => {
            // A well has no employee row, just its one line of text at y+56 (notes/
            // building_info.md 9.1, from Akhenaten's ui_well_info_window.js).
            let mut p = Page::new(14, t(109, 0));
            p.line(56.0, t(109, super::InfoPanel::well_line(world, b)));
            p
        }
        k::WATER_SUPPLY => {
            // 17 blocks (notes/building_info.md 1.4: "water supply, water lift | 29x17",
            // a direct citation from Akhenaten; the "Decompile facts" class list is
            // admittedly incomplete, so it doesn't override this).
            let mut p = Page::new(17, t(108, 0));
            // One line per worker short, from all five down to none.
            p.line(63.0, if no_road { road_line() } else { t(108, (7 - b.workers.clamp(0, 5)) as usize) });
            p.staffed();
            p
        }
        k::WATER_LIFT => {
            let mut p = Page::new(17, t(107, 0));
            // A lift with no water beside it or ditch to feed it says so.
            p.line(63.0, if no_road { road_line() } else if b.workers <= 0 { t(107, 2) } else if b.water == 0 { t(107, 3) } else { t(107, 1) });
            p.staffed();
            p
        }
        k::WORK_CAMP => {
            const G: usize = 179;
            let mut p = Page::new(22, t(G, 0));
            let out: Vec<u16> = world
                .figures
                .iter()
                .filter(|f| !f.dead && f.home == b.id && (f.kind == osiris_sim::farms::PEASANT || f.kind == osiris_sim::monuments::SLED))
                .map(|f| world.buildings.get(f.target).map_or(0, |t| t.kind))
                .collect();
            let farms = out.iter().any(|&t| world.is_farm(t));
            let monuments = out.iter().any(|&t| t != 0 && !world.is_farm(t));
            let line = if no_road {
                road_line()
            } else if b.workers <= 0 {
                t(G, 2)
            } else if out.is_empty() {
                t(G, 3)
            } else {
                t(G, 4 + farms as usize + 2 * monuments as usize)
            };
            p.line(56.0, line);
            p.panel = Some((136.0, 6));
            p.staff = Some(138.0);
            p.bottom(105.0, t(G, 1));
            p
        }
        k::BURNING_RUIN => {
            let mut p = Page::new(16, t(111, 0));
            p.bottom(143.0, t(111, 1));
            p
        }
        k::BOOTH | k::BANDSTAND | k::PAVILION | k::SENET_HOUSE => venue_page(ui, world, b),
        k::TEMPLE_FIRST..=k::COMPLEX_LAST => {
            // 18 blocks tall (notes/building_info.md "Decompile facts": class 2 = 18
            // blocks). Quoting notes/building_info.md 7.1 verbatim, since this line has
            // been miscopied before: "Title, employee panel at [16,56] size [27,4],
            // overlay, mothball, help, close, and a god picture at [190,134]... Temple
            // text groups 92-96 have only name and description, so there is no priest
            // status line. Show the description (id 1) as the text line." The
            // description sits in a narrower column than usual so it doesn't run under
            // the picture.
            let god = ((kind - k::TEMPLE_FIRST) % 5) as u32;
            let mut p = Page::new(18, t(92 + god as usize, 0));
            p.panel = Some((56.0, 4));
            p.staff = Some(62.0);
            p.picture = Some((ui.img.gods + 21 + god, [190.0, 134.0]));
            p.line_narrow(128.0, if no_road { road_line() } else { t(92 + god as usize, 1) }, 150.0);
            p
        }
        k::SHRINE_FIRST..=k::SHRINE_LAST => {
            // 14 blocks tall, god picture at (190,94) (notes/building_info.md: Akhenaten
            // ui_shrine_info_window.js, background size[29,14], god_image pos[190,94]).
            let god = (kind - k::SHRINE_FIRST) as u32;
            let mut p = Page::new(14, t(161, 2 * god as usize));
            p.picture = Some((ui.img.gods + 21 + god, [190.0, 94.0]));
            p
        }
        k::DOCK => {
            const G: usize = 101;
            let mut p = Page::new(16, t(G, 0));
            let bands = |base: usize| match pct {
                p if p <= 0 => base,
                p if p < 50 => base + 1,
                p if p < 75 => base + 2,
                _ => base + 3,
            };
            let id = if world.moored_ship(b.id).is_some() { bands(2) } else { bands(6) };
            p.line(56.0, if no_road { road_line() } else { t(G, id) });
            p.staffed();
            p
        }
        k::FISHING_WHARF => {
            use osiris_sim::fishing::action;
            const G: usize = 102;
            let mut p = Page::new(16, t(G, 0));
            p.icon = Some(resource::FISH);
            let id = if mothballed(resource::FISH) {
                9
            } else {
                match world.wharf_boat(b.id).and_then(|f| world.figures.get(f)).map(|f| f.action) {
                    None => 2,
                    Some(action::GOING_TO_FISH) => 3,
                    Some(action::FISHING) => 4,
                    Some(action::GOING_TO_WHARF) => 5,
                    Some(action::AT_WHARF) => 6,
                    Some(action::RETURNING_WITH_FISH) => 7,
                    Some(_) => 8,
                }
            };
            p.line(56.0, if no_road { road_line() } else { t(G, id) });
            p.staffed();
            p
        }
        k::TRANSPORT_WHARF | k::WARSHIP_WHARF => {
            let g = if kind == k::TRANSPORT_WHARF { 174 } else { 175 };
            let mut p = Page::new(16, t(g, 0));
            if no_road {
                p.line(40.0, road_line());
            }
            p.line(76.0, t(g, 1));
            p.staffed();
            p
        }
        k::FERRY => {
            const G: usize = 159;
            let mut p = Page::new(22, t(G, 0));
            let link = world.ferry_link(b.id, false).and_then(|l| world.buildings.get(l));
            let id = match link {
                _ if no_road => None,
                None => Some(1),
                Some(l) if l.road.is_none() => Some(2),
                Some(l) if l.workers <= 0 => Some(3),
                Some(_) => None,
            };
            p.line(60.0, if no_road { road_line() } else { id.map_or_else(String::new, |i| t(G, i)) });
            p.staffed();
            p
        }
        k::SHIPWRIGHT => shipwright_page(ui, world, b),
        k::HUNTING_LODGE => {
            const G: usize = 154;
            let mut p = Page::new(22, t(G, 0));
            let meat = stock(b, resource::GAMEMEAT);
            p.rows.push(Row { resource: resource::GAMEMEAT, icon: [32.0, 56.0], text: format!("{} {}", t(G, 13), units(ui, meat)), at: [60.0, 60.0], yellow: false });
            let line = if no_road {
                road_line()
            } else if mothballed(resource::GAMEMEAT) {
                t(G, 4)
            } else if b.workers <= 0 {
                t(G, 5)
            } else if meat <= 0 {
                t(G, 11)
            } else {
                t(G, by_staff(pct, [6, 7, 8, 9, 10]))
            };
            p.line(86.0, line);
            p.staffed();
            p
        }
        k::REED_GATHERER => {
            const G: usize = 116;
            let mut p = Page::new(22, t(G, 0));
            p.icon = Some(resource::REEDS);
            p.rows.push(Row { resource: resource::REEDS, icon: [32.0, 50.0], text: format!("{} {}", t(G, 2), units(ui, stock(b, resource::REEDS))), at: [60.0, 54.0], yellow: false });
            let line = if no_road {
                road_line()
            } else if mothballed(resource::REEDS) {
                t(G, 4)
            } else if b.workers <= 0 {
                t(G, 5)
            } else {
                t(G, by_staff(pct, [6, 7, 8, 9, 10]))
            };
            p.line(70.0, line);
            p.staffed();
            p.bottom(113.0, t(G, 1));
            p
        }
        k::CATTLE_RANCH => {
            const G: usize = 117;
            let mut p = Page::new(22, t(G, 0));
            p.icon = Some(resource::MEAT);
            let progress = b.progress * 100 / world.max_progress(kind).max(1);
            p.line(44.0, format!("{} {}% {}", t(112, 2), progress, t(112, 3)));
            let straw = stock(b, resource::STRAW);
            p.rows.push(Row { resource: resource::STRAW, icon: [32.0, 96.0], text: format!("{} {}", t(TEXT_RESOURCES, resource::STRAW as usize), units(ui, straw)), at: [60.0, 100.0], yellow: false });
            let line = if no_road {
                road_line()
            } else if mothballed(resource::MEAT) {
                t(G, 4)
            } else if b.workers <= 0 {
                t(G, 5)
            } else {
                t(G, by_staff(pct, [6, 7, 8, 9, 10]))
            };
            p.line(70.0, line);
            p.staffed();
            p.bottom(113.0, t(G, 1));
            p
        }
        k::CARPENTERS | k::BRICKLAYERS | k::STONEMASONS => {
            let (g, icon) = match kind {
                k::CARPENTERS => (171, resource::TIMBER),
                k::BRICKLAYERS => (172, r::BRICKS),
                _ => (173, r::STONE),
            };
            let mut p = Page::new(16, t(g, 0));
            p.icon = Some(icon);
            // Only the carpenters keep a store of their own, of wood.
            let wood = stock(b, resource::TIMBER);
            if kind == k::CARPENTERS {
                p.line(40.0, format!("{} {}% {}", t(g, 2), b.progress * 100 / world.max_progress(kind).max(1), t(g, 3)));
                p.rows.push(Row { resource: resource::TIMBER, icon: [32.0, 56.0], text: format!("{} {}", t(g, 12), units(ui, wood)), at: [60.0, 60.0], yellow: false });
            }
            let line = if no_road {
                road_line()
            } else if b.workers <= 0 {
                t(g, 5)
            } else if kind == k::CARPENTERS && wood <= 0 {
                t(g, 11)
            } else {
                t(g, by_staff(pct, [6, 7, 8, 9, 10]))
            };
            p.line(86.0, line);
            p.staffed();
            p
        }
        k::ARTISANS => {
            const G: usize = 312;
            let mut p = Page::new(16, t(G, 0));
            for (i, (res, label)) in [(r::PAINT, 10), (r::CLAY, 11)].into_iter().enumerate() {
                let dx = 198.0 * i as f32;
                p.rows.push(Row { resource: res, icon: [32.0 + dx, 56.0], text: format!("{} {}", t(G, label), units(ui, stock(b, res))), at: [60.0 + dx, 60.0], yellow: false });
            }
            let line = if no_road {
                road_line()
            } else if b.workers <= 0 {
                t(G, 2)
            } else if stock(b, r::PAINT) <= 0 {
                t(G, 8)
            } else if stock(b, r::CLAY) <= 0 {
                t(G, 9)
            } else {
                t(G, by_staff(pct, [3, 4, 5, 6, 7]))
            };
            p.line(86.0, line);
            p.staffed();
            p
        }
        k::ZOO => {
            // Game meat and straw in store, and whether it has animals to show.
            const G: usize = 308;
            let mut p = Page::new(18, t(G, 0));
            let (meat, straw) = (stock(b, resource::GAMEMEAT), stock(b, resource::STRAW));
            p.rows.push(Row { resource: resource::GAMEMEAT, icon: [32.0, 36.0], text: format!("{} {}", t(G, 6), units(ui, meat)), at: [60.0, 40.0], yellow: false });
            p.rows.push(Row { resource: resource::STRAW, icon: [255.0, 36.0], text: format!("{} {}", t(G, 7), units(ui, straw)), at: [280.0, 40.0], yellow: false });
            let line = if no_road {
                road_line()
            } else if b.workers <= 0 {
                t(G, 4)
            } else if meat <= 0 {
                t(G, 2)
            } else if straw <= 0 {
                t(G, 3)
            } else {
                t(G, 1)
            };
            p.line(72.0, line);
            p.panel = Some((136.0, 6));
            p.staff = Some(138.0);
            p
        }
        k::FESTIVAL_SQUARE => {
            let mut p = Page::new(16, t(188, 0));
            p.line(55.0, t(188, 1));
            p
        }
        k::STORAGE_YARD => yard_page(ui, world, b),
        k::GRANARY => granary_page(ui, world, b),
        k::BAZAAR => bazaar_page(ui, world, b),
        _ => {
            // Anything else: its name and what it is for.
            let mut p = Page::new(16, def_group.map_or_else(|| t(28, kind as usize), |g| t(g, 0)));
            if let Some(g) = def_group {
                p.line(56.0, t(g, 1));
            }
            if world.workers_needed(kind) > 0 {
                p.staffed();
            }
            p
        }
    };
    Some(page)
}

fn farm_page(ui: &Ui, world: &World, b: &Building, g: usize, crop: u16) -> Page {
    let t = |g: usize, i: usize| ui.t(g, i);
    let mut p = Page::new(24, t(g, 0));
    p.icon = Some(crop);
    let progress = b.progress * 100 / osiris_sim::farms::PROGRESS_MAX;
    p.line(44.0, format!("{} {}% {} {} {}% {}", t(g, 2), progress, t(g, 3), t(112, 12), world.fertility(b.id).min(99), t(112, 13)));
    let needed = world.workers_needed(b.kind).max(1);
    let floodplain = world.is_floodplain_farm(b.id);
    // Floodplain farms are worked by laborers from a work camp, counted in days.
    let pct = if floodplain { if b.labor_days > 0 { 100 } else { 0 } } else { b.workers * 100 / needed };
    let working = if floodplain { b.labor_days > 0 } else { b.workers > 0 };
    let line = if b.road.is_none() && !floodplain {
        ui.t(TEXT_FRAME, 25)
    } else if world.is_mothballed(crop) {
        t(g, 4)
    } else if !working {
        t(g, 5)
    } else {
        t(g, by_staff(pct, [6, 7, 8, 9, 10]))
    };
    p.line(70.0, line);
    p.panel = Some((136.0, 4));
    if floodplain {
        // The laborers' line in place of the employees.
        p.dark.push((65.0, 152.0, t(177, if working { 6 } else { 5 })));
        p.picture = Some((ui.img.context_icons + 14, [40.0, 148.0]));
        let month = world.flood_start_month() as usize;
        p.bottom(158.0, format!("{} {}", t(177, 2), t(160, month)));
    } else {
        p.staff = Some(142.0);
    }
    // "This farmland is irrigated." or "...not irrigated.", at h-143 whether or not the
    // farm is on a floodplain (notes/building_info.md "Decompile facts": irrigation line
    // at h-143, checked against the exe).
    p.bottom(143.0, t(177, if world.is_irrigated(b.id) { 0 } else { 1 }));
    p.bottom(113.0, t(g, 1));
    p
}

fn venue_page(ui: &Ui, world: &World, b: &Building) -> Page {
    let t = |g: usize, i: usize| ui.t(g, i);
    let no_road = b.road.is_none();
    let [j, m, d] = b.shows;
    let (hb, g) = match b.kind {
        k::BANDSTAND => (19, 71),
        k::BOOTH => (18, 72),
        k::SENET_HOUSE => (18, 73),
        _ => (18, 74),
    };
    let mut p = Page::new(hb, t(g, 0));
    let shows = (j > 0) as usize + (m > 0) as usize + (d > 0) as usize;
    let status = match b.kind {
        k::BOOTH => match () {
            _ if b.workers <= 0 => Some(4),
            _ if shows == 0 => Some(2),
            _ if j > 0 => Some(3),
            _ => None,
        },
        k::BANDSTAND => match () {
            _ if b.workers <= 0 => Some(6),
            _ if shows == 0 => Some(2),
            _ if shows == 2 => Some(3),
            _ if m > 0 => Some(4),
            _ if j > 0 => Some(5),
            _ => None,
        },
        k::SENET_HOUSE => (b.workers <= 0).then_some(4),
        _ => Some(match () {
            _ if b.workers <= 0 => 10,
            _ if shows == 0 => 2,
            _ if shows == 3 => 3,
            _ if d > 0 && m > 0 => 4,
            _ if d > 0 && j > 0 => 5,
            _ if m > 0 && j > 0 => 6,
            _ if j > 0 => 7,
            _ if m > 0 => 8,
            _ => 9,
        }),
    };
    if b.kind == k::SENET_HOUSE {
        // Its beer in store.
        let beer = stock(b, resource::BEER);
        p.rows.push(Row { resource: resource::BEER, icon: [32.0, 36.0], text: format!("{} {}", t(g, 7), units(ui, beer)), at: [60.0, 40.0], yellow: false });
    }
    p.line(56.0, if no_road { ui.t(TEXT_FRAME, 25) } else { status.map_or_else(String::new, |i| t(g, i)) });
    p.panel = Some((136.0, if b.kind == k::BANDSTAND { 7 } else { 6 }));
    p.staff = Some(138.0);
    // The acts playing, and for how long.
    let act = |n: i32, none: usize, playing: usize| if n > 0 { format!("{} {}", t(g, playing), days(ui, n)) } else { t(g, none) };
    let acts: Vec<(f32, String)> = match b.kind {
        k::BOOTH => vec![(182.0, act(j, 5, 6))],
        k::BANDSTAND => vec![(182.0, act(j, 9, 10)), (202.0, act(m, 7, 8))],
        k::PAVILION => vec![(182.0, act(j, 11, 12)), (197.0, act(m, 13, 14)), (212.0, act(d, 15, 16))],
        _ => Vec::new(),
    };
    p.dark.extend(acts.into_iter().map(|(y, s)| (32.0, y, s)));
    let _ = world;
    p
}

fn shipwright_page(ui: &Ui, world: &World, b: &Building) -> Page {
    const G: usize = 100;
    let t = |i: usize| ui.t(G, i);
    let mut p = Page::new(16, t(0));
    let wood = stock(b, resource::TIMBER);
    p.rows.push(Row { resource: resource::TIMBER, icon: [30.0, 38.0], text: format!("{} {}", t(7), units(ui, wood)), at: [58.0, 44.0], yellow: false });
    // What the original's window says (its case 0x4a): the work done on a hull out
    // of 160, and whether there is the wood for it; a repair, and whether there is
    // wood; or, idle, whether any wharf wants a boat.
    let fishing = osiris_sim::fishing::FISHING_BOAT;
    if b.road.is_none() {
        p.line(62.0, ui.t(TEXT_FRAME, 25));
    } else if b.boat_kind != 0 {
        p.line(62.0, format!("{} {}% {}", t(2), b.progress * 100 / osiris_sim::navy::HULL_WORK, t(3)));
        p.line_narrow(86.0, t(if wood < osiris_sim::navy::SHIP_TIMBER && b.boat_kind != fishing { 9 } else { 5 }), 368.0);
    } else if b.repairing != 0 {
        p.line_narrow(86.0, t(if wood < 1 { 10 } else { 6 }), 368.0);
    } else {
        match world.ship_wanted() {
            None => p.line_narrow(86.0, t(4), 368.0),
            Some(k) if k == fishing || wood >= osiris_sim::navy::SHIP_TIMBER => {}
            Some(_) => p.line_narrow(86.0, t(9), 368.0),
        }
    }
    p.staffed();
    p
}

fn yard_page(ui: &Ui, world: &World, b: &Building) -> Page {
    // 22 blocks tall (notes/building_info.md "Decompile facts": anything not in the
    // named size classes is 22 blocks).
    let mut p = Page::new(22, ui.t(TEXT_YARD, 0));
    let total = world.total_stored(b.id);
    if b.road.is_none() {
        p.line(56.0, ui.t(TEXT_FRAME, 25));
    } else if total == 0 {
        p.line(56.0, ui.t(TEXT_YARD, 22));
    } else {
        // "Storing N units." and "Space for N units." (notes/building_info.md 5.1;
        // "units." is group 8 id 17, used unconditionally, not the singular/plural
        // 8.10/8.11 pair `units()` picks for a walker's cargo line).
        let space = (osiris_sim::storage::CAPACITY - total).max(0);
        let words = ui.t(TEXT_GENERAL, 17);
        p.labels.push((24.0, 95.0, format!("{} {} {}", ui.t(TEXT_YARD, 2), total, words)));
        p.labels.push((220.0, 95.0, format!("{} {} {}", ui.t(TEXT_YARD, 3), space, words)));
        // What it holds, in a 3x3 grid of up to 9 resources (notes/building_info.md 5.1:
        // icons at x 32/172/292, rows 30px apart starting at y 110).
        let mut held: Vec<(u16, i32)> = Vec::new();
        for &(r, n) in &b.spaces {
            if n <= 0 {
                continue;
            }
            match held.iter_mut().find(|h| h.0 == r) {
                Some(h) => h.1 += n,
                None => held.push((r, n)),
            }
        }
        held.sort();
        const COLUMNS: [f32; 3] = [32.0, 172.0, 292.0];
        for (i, &(r, n)) in held.iter().take(9).enumerate() {
            let (cx, cy) = (COLUMNS[i / 3], 110.0 + 30.0 * (i % 3) as f32);
            p.rows.push(Row { resource: r, icon: [cx, cy], text: yard_amount(ui, r, n), at: [cx + 22.0, cy + 4.0], yellow: false });
        }
    }
    p.panel = Some((198.0, 5));
    p.staff = Some(198.0);
    // Clear of the staff row's own (possibly two-line) labor-availability text, which
    // sits at panel+26 and can run to panel+56.
    cart_line(ui, world, b, &mut p, 255.0);
    if total >= osiris_sim::storage::CAPACITY {
        p.bottom(85.0, ui.t(TEXT_YARD, 13));
    } else if !b.spaces.is_empty() && b.spaces.iter().all(|s| s.1 > 0) {
        p.bottom(85.0, ui.t(TEXT_YARD, 14));
    }
    p
}

/// A yard's stock of `r`: stone in blocks of 100, weapons and chariots by the
/// hundred, everything else in units.
fn yard_amount(ui: &Ui, r: u16, n: i32) -> String {
    let name = |i: u16| ui.t(TEXT_RESOURCES, i as usize);
    match r {
        r::STONE | r::LIMESTONE | r::GRANITE | r::SANDSTONE | r::MARBLE => {
            let blocks = n / 100;
            format!("{} {} {}", blocks, ui.t(TEXT_YARD, if blocks == 1 { 23 } else { 24 }).trim(), name(r))
        }
        r::WEAPONS | r::CHARIOTS => {
            let count = n / 100;
            format!("{} {}", count, if count == 1 { name(r) } else { name(r + 54) })
        }
        _ => format!("{} {}", n, name(r)),
    }
}

/// A storage building's cart pusher, on the dark panel at `y`: what he is doing, with
/// the icon of what he carries.
fn cart_line(ui: &Ui, world: &World, b: &Building, p: &mut Page, y: f32) {
    use osiris_sim::economy::action as cart;
    match world.figures.get(b.walkers[2]).filter(|f| !f.dead) {
        Some(f) => {
            if f.cargo > 0 && f.amount > 0 {
                p.rows.push(Row { resource: f.cargo, icon: [32.0, y], text: String::new(), at: [0.0, 0.0], yellow: false });
            }
            let id = match f.action {
                cart::RETURNING => 17,
                cart::FETCHING | cart::BRINGING_HOME => 33,
                _ => 16,
            };
            p.dark.push((64.0, y + 3.0, ui.t(TEXT_YARD, id)));
        }
        None if b.workers > 0 => p.dark.push((32.0, y + 3.0, ui.t(TEXT_YARD, 15))),
        None => {}
    }
}

fn granary_page(ui: &Ui, world: &World, b: &Building) -> Page {
    // 17 blocks (notes/building_info.md 1.4 and 5.2 both give this directly, from
    // Akhenaten's ui_granary_info.js; the "Decompile facts" class list of 14/16/18/19/
    // 22/24 is admittedly incomplete ("Others need content matching"), so it doesn't
    // override two independent, direct citations of the real size).
    const G: usize = super::TEXT_GRANARY;
    let mut p = Page::new(17, ui.t(G, 0));
    if b.road.is_none() {
        p.line(40.0, ui.t(TEXT_FRAME, 25));
    } else {
        let total = world.total_stored(b.id);
        let words = ui.t(TEXT_GENERAL, 17);
        // "Storing N units." / "Space for N units.", at y+60 (notes/building_info.md
        // 5.2: storing/free_space at [34,60]/[220,60]).
        let storing = format!("{} {} {}", ui.t(G, 2), total, words);
        let space = format!("{} {} {}", ui.t(G, 3), (osiris_sim::storage::CAPACITY - total).max(0), words);
        p.labels.push((34.0, 60.0, storing));
        p.labels.push((220.0, 60.0, space));
        // The city's foods, two to a column.
        for (i, &r) in city_foods(world).iter().enumerate() {
            let (cx, cy) = (if i < 2 { 34.0 } else { 240.0 }, if i % 2 == 0 { 68.0 } else { 92.0 });
            p.rows.push(Row { resource: r, icon: [cx, cy], text: format!("{} {}", stock(b, r), ui.t(TEXT_RESOURCES, r as usize)), at: [cx + 34.0, cy + 7.0], yellow: false });
        }
    }
    // Employee panel at [16,142] size [27,5], glyph at panel+6 (notes/building_info.md
    // 5.2: panel [16,142] size [27,5]; glyph [40,148]). The cart line sits below the
    // staff row's own (possibly two-line) labor-availability text, which already uses
    // the panel+26 slot the doc's "desc[70,168]" describes.
    p.panel = Some((142.0, 5));
    p.staff = Some(142.0);
    cart_line(ui, world, b, &mut p, 199.0);
    p
}

fn bazaar_page(ui: &Ui, world: &World, b: &Building) -> Page {
    // Group 97, size 29x16 (notes/building_info.md 6).
    const G: usize = super::TEXT_BAZAAR;
    let mut p = Page::new(16, ui.t(G, 0));
    // The one warning_text slot always sits at (32,36), whichever sentence fills it
    // (notes/building_info.md 6).
    if b.road.is_none() {
        p.line(36.0, ui.t(TEXT_FRAME, 25));
    } else if b.workers <= 0 {
        p.line(36.0, ui.t(G, 2));
    } else {
        let foods = city_foods(world);
        if foods.iter().all(|&r| stock(b, r) <= 0) {
            p.line(36.0, ui.t(G, 4));
        }
        // Food icons at y 85, text at y 90; goods icons at y 110, text at y 114
        // (notes/building_info.md 6).
        for (i, &r) in foods.iter().enumerate() {
            let cx = 32.0 + 110.0 * i as f32;
            p.rows.push(Row { resource: r, icon: [cx, 85.0], text: stock(b, r).to_string(), at: [cx + 32.0, 90.0], yellow: !b.bazaar_buys(r) });
        }
        for (i, r) in [resource::POTTERY, resource::LUXURY_GOODS, resource::LINEN, resource::BEER].into_iter().enumerate() {
            let cx = 32.0 + 110.0 * i as f32;
            p.rows.push(Row { resource: r, icon: [cx, 110.0], text: stock(b, r).to_string(), at: [cx + 32.0, 114.0], yellow: !b.bazaar_buys(r) });
        }
    }
    // Employee panel [16,136] size [27,4] (notes/building_info.md 6), the same 4-block
    // panel every simple building uses.
    p.panel = Some((136.0, 4));
    p.staff = Some(142.0);
    if b.walkers[0] != 0 {
        p.dark.push((64.0, 184.0, ui.t(G, 11)));
    } else if b.workers > 0 {
        p.dark.push((32.0, 184.0, ui.t(G, 10)));
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staffing_bands_are_quarters() {
        let lines = [6, 7, 8, 9, 10];
        assert_eq!(by_staff(100, lines), 6);
        assert_eq!(by_staff(99, lines), 7);
        assert_eq!(by_staff(75, lines), 7);
        assert_eq!(by_staff(74, lines), 8);
        assert_eq!(by_staff(50, lines), 8);
        assert_eq!(by_staff(25, lines), 9);
        assert_eq!(by_staff(24, lines), 10);
        assert_eq!(by_staff(0, lines), 10);
    }

    #[test]
    fn labor_line_follows_the_original() {
        // Short of staff: no people, no houses in reach, few, or enough.
        assert_eq!(labor_line(2, 10, 0, 50), Some(16));
        assert_eq!(labor_line(2, 10, 500, 0), Some(17));
        assert_eq!(labor_line(2, 10, 500, 39), Some(20));
        assert_eq!(labor_line(2, 10, 500, 40), Some(18));
        // Fully staffed: only a warning of poor access.
        assert_eq!(labor_line(10, 10, 500, 40), None);
        assert_eq!(labor_line(10, 10, 500, 12), Some(20));
    }

    #[test]
    fn tables_cover_the_chariot_maker_and_meadow_farms() {
        assert_eq!(workshop(205).map(|w| w.0), Some(185));
        assert_eq!(workshop(204).map(|w| w.2.len()), Some(2));
        assert_eq!(farm(313), farm(102));
        assert_eq!(farm(317), farm(224));
        assert_eq!(raw_material(216), Some((192, r::GRANITE)));
    }
}
