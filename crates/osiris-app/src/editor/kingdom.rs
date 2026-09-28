//! The editor's Kingdom map (the Kingdom button under the minimap): the empire map
//! with its cities, region names, pictures and battle markers, which the designer
//! adds, moves, edits and deletes; the trade and invasion routes drawn across it;
//! each city's kind, name, goods, demand, route and cost; and the Kingdom's prices.
//!
//! Layout and behaviour are the original's (window 0x15): the frame of the empire
//! window (FUN_0052d630), the map (FUN_0052f8b0 with the editor's extras), the panel
//! along the bottom (FUN_0052d4e0: eight buttons, FUN_0052e6f0, and a box whose
//! contents follow the chosen button, FUN_0052ec50), the demand window (FUN_00534320)
//! and the prices window (FUN_00537cf0); the buttons' places and what they do come
//! from their tables (0x5c3268 and those beside it) and handlers (0x407370 on). As
//! the Mission Editor Guide says, right-clicking an object selects it, a left click
//! then moves it (or drags it); a route is drawn a waypoint per click and finished
//! with a right-click.
//!
//! Everything edits `Editor::scenario.empire`, which saving writes back into the
//! map's empire chunks.

use super::Editor;
use crate::empire_window::EmpireImages;
use crate::widgets::{UiImages, inside};
use osiris_formats::ImageLibrary;
use osiris_formats::empire::{self, EmpireObject, MAX_BUYS, MAX_POINTS, MAX_SELLS, city, object};
use osiris_render::{Renderer, Space, WHITE};
use osiris_ui::{Font, PanelImages, draw_text, draw_text_in, font, panel, text_width};

const MAP_W: f32 = 1200.0;
const MAP_H: f32 = 1600.0;
/// The frame's bars are 16 pixels thick; the middle bar sits 130 above the bottom.
const BAR: f32 = 16.0;
const DIVIDER: f32 = 130.0;
/// Cities the game can hold (the check at FUN_00407620).
const MAX_CITIES: usize = 60;
/// Routes a scenario can use (a city's route number is at most 19, FUN_004084b0).
const MAX_ROUTE_ID: usize = 19;
/// How near (in pixels) a click must be to a waypoint or the route to take it.
const NEAR: i32 = 8;

/// What a building needs to work, for the checks after a city's goods change.
#[derive(Clone, Copy)]
enum Need {
    Good(u8),
    Building(usize),
    /// The building allowed, or else the good obtainable.
    Either(usize, u8),
}

/// Whether any city (ours included) lists resource `r` among what it sells
/// (FUN_00443a80).
fn listed(s: &osiris_formats::Scenario, r: u8) -> bool {
    s.empire.objects.iter().any(|o| o.in_use && o.kind == object::CITY && o.sells.contains(&r))
}

/// The raw good a good is made from (FUN_00442870's table): meat from straw, straw
/// from grain, weapons from copper, pottery from clay, beer from barley, linen from
/// flax, luxury goods from gems, papyrus from reeds, chariots from wood, paint from
/// henna.
fn raw_of(r: u8) -> u8 {
    match r {
        2 => 9,
        9 => 1,
        10 => 29,
        13 => 11,
        15 => 14,
        17 => 16,
        19 => 18,
        23 => 22,
        28 => 20,
        33 => 32,
        _ => r,
    }
}

/// Whether our city can make `r` (FUN_00443820): it produces it, or for a made good,
/// a trading city sells what it is made from.
fn makes(s: &osiris_formats::Scenario, r: u8) -> bool {
    let ours = s.empire.objects.iter().filter(|o| o.in_use && o.kind == object::CITY && o.city_type == city::OURS);
    let made = matches!(r, 10 | 13 | 15 | 17 | 19 | 23 | 28 | 33);
    let raw = if made { raw_of(r) } else { r };
    for o in ours {
        if made && s.empire.objects.iter().any(|c| c.in_use && c.kind == object::CITY && city::trades(c.city_type) && c.sells.contains(&raw)) {
            return true;
        }
        if o.sells.contains(&raw) {
            return true;
        }
    }
    false
}

/// Whether the city can have resource `r`, made or bought (FUN_00442870): some city
/// lists it, or it can be made from what the city produces or buys; bricks from
/// straw and clay, lamps from oil and pottery; weapons and chariots only where the
/// weaponsmith and chariot maker are allowed.
pub fn obtainable(s: &osiris_formats::Scenario, r: u8) -> bool {
    let allowed = |id: usize| s.info.reserved.get(id).is_some_and(|&v| v != 0);
    match r {
        12 => listed(s, 12) || (obtainable(s, 9) && obtainable(s, 11)),
        34 => listed(s, 34) || (obtainable(s, 31) && obtainable(s, 13)),
        _ if listed(s, r) => true,
        10 | 28 => {
            let (raw, maker) = if r == 10 { (29, 41) } else { (20, 42) };
            if listed(s, raw) && allowed(maker) {
                return true;
            }
            makes(s, raw) && allowed(maker)
        }
        _ => {
            let raw = raw_of(r);
            (r != 9 && listed(s, raw)) || makes(s, raw)
        }
    }
}

/// A 16-bit colour as the original gives them (5-6-5).
fn rgb(c: u16) -> [f32; 4] {
    [((c >> 11) & 31) as f32 / 31.0, ((c >> 5) & 63) as f32 / 63.0, (c & 31) as f32 / 31.0, 1.0]
}
const RED: u16 = 0xf800;
/// The panel's fill, and the grey behind the Add object choices.
const BEIGE: u16 = 0xf73b;
const GREY: u16 = 0xc618;
/// City names (0x40e4 in the game's 5-5-5 colours) and region names (0x558a).
const NAME: [f32; 4] = [16.0 / 31.0, 7.0 / 31.0, 4.0 / 31.0, 1.0];
const REGION: [f32; 4] = [21.0 / 31.0, 12.0 / 31.0, 10.0 / 31.0, 1.0];

/// Which of the panel's buttons is chosen (the original's modes 1, 2, 4, 14, 15).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// "Add object": a click places the kind chosen at the right.
    Add,
    /// "Edit objects": a right-click selects, a left click moves the selection.
    Edit,
    /// "General": the button to the prices.
    #[default]
    General,
    /// "Add route": clicks lay waypoints, a right-click finishes.
    AddRoute,
    /// "Edit route": waypoints dragged, added on the route, deleted by right-click.
    EditRoute,
}

/// What a list chooses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pick {
    CityType,
    CityName,
    Region,
    /// A slot of our city's goods, of the goods a city sells, or buys.
    Ours(usize),
    Sells(usize),
    Buys(usize),
}

/// What a keypad sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Num {
    Route,
    Cost,
    Path,
    Order,
    /// Which route Edit route works on.
    RouteId,
    /// A resource's buying (false) or selling (true) price.
    Price(usize, bool),
}

/// A window over the map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Window {
    /// The selected city's demand for each of its goods.
    Demand,
    /// The Kingdom's prices.
    Prices,
}

struct List {
    pick: Pick,
    group: usize,
    ids: Vec<usize>,
}

struct Keypad {
    num: Num,
    typed: String,
    fresh: bool,
}

enum Grab {
    /// The selected object follows the mouse.
    Object,
    /// A waypoint of the route being edited.
    Waypoint(usize),
}

/// A scripted action (the headless harness's `editk...` steps).
#[derive(Debug, Clone)]
pub enum Scripted {
    /// A left or right click at a screen point.
    Click([f32; 2]),
    Right([f32; 2]),
    /// A left or right click at a pixel of the map.
    MapClick(i32, i32),
    MapRight(i32, i32),
    /// The mouse dragged to a pixel of the map, and let go.
    DragTo(i32, i32),
    /// One of the eight buttons, by its label's text id in group 44.
    Button(usize),
    /// Keys typed on the keypad.
    Type(String),
    /// Scrolls the map to have this pixel at the view's top-left.
    Scroll(i32, i32),
    /// Print the cities and routes (to check a script's work).
    State,
}

#[derive(Default)]
pub struct Kingdom {
    pub mode: Mode,
    /// What Add object places: 0 a picture, 1 a city, 2 a region's name.
    pub add_kind: u8,
    /// The picture Add object places or the selected picture shows: an image of
    /// Pharaoh_General group 190.
    pub ornament: u32,
    pub selected: Option<usize>,
    /// The route Add route and Edit route work on.
    pub route: usize,
    /// Map pixel at the top-left of the view.
    pub scroll: [f32; 2],
    pub window: Option<Window>,
    list: Option<List>,
    keypad: Option<Keypad>,
    /// A message of text group 5 (its title; the text follows it).
    notice: Option<usize>,
    /// A warning of text group 19 (the original's window 0x47, FUN_005344c0), for a
    /// good, monument or building an edit has taken away.
    warning: Option<usize>,
    grab: Option<Grab>,
    pub cursor: [f32; 2],
    clock: f32,
    placed: bool,
}

/// The images the Kingdom map draws, found once.
#[derive(Clone, Copy)]
struct Art {
    empire: EmpireImages,
    ornaments: u32,
    ornament_count: u32,
    land_marker: u32,
    sea_marker: u32,
}

impl Art {
    fn load(lib: &ImageLibrary) -> Option<Self> {
        let g = |group| lib.group_id("Pharaoh_General", group, 0).ok();
        let ornaments = g(190)?;
        let last = g(191)?;
        Some(Self {
            empire: EmpireImages::load(lib).ok()?,
            ornaments,
            // The original runs from group 190's first image to group 191's, both
            // included (0x4074e0, FUN_00442660): the fifteenth is the large ruin.
            ornament_count: last.saturating_sub(ornaments) + 1,
            land_marker: g(178)?,
            sea_marker: g(179)?,
        })
    }
}

fn size_of(lib: &ImageLibrary, id: u32) -> (i32, i32) {
    lib.resolve(id).map_or((0, 0), |p| {
        let rec = lib.record(p);
        (rec.width as i32, rec.height as i32)
    })
}

/// The map's rectangle on screen: inside the frame, above its middle bar.
fn view(screen: [f32; 2]) -> [f32; 4] {
    let w = (screen[0] - 2.0 * BAR).min(MAP_W);
    let h = (screen[1] - DIVIDER - BAR).min(MAP_H);
    [((screen[0] - w) / 2.0).floor(), BAR, w, h]
}

/// The eight buttons at the panel's left (table 0x5c3268, from (15, h-116)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Main {
    Mode(Mode),
    Delete,
    Reset,
    Ok,
}

fn main_buttons(h: f32) -> [(Main, usize, [f32; 4]); 8] {
    let b = |x: f32, row: f32| [15.0 + x, h - 86.0 + 20.0 * row, 100.0, 18.0];
    [
        (Main::Mode(Mode::Add), 0, b(0.0, 0.0)),
        (Main::Mode(Mode::Edit), 1, b(0.0, 1.0)),
        (Main::Delete, 2, b(0.0, 2.0)),
        (Main::Mode(Mode::General), 3, b(0.0, 3.0)),
        (Main::Mode(Mode::AddRoute), 166, b(104.0, 0.0)),
        (Main::Mode(Mode::EditRoute), 167, b(104.0, 1.0)),
        (Main::Reset, 223, b(104.0, 2.0)),
        (Main::Ok, 7, b(104.0, 3.0)),
    ]
}

/// The resources our city may be given (the table at 0x4086f0): foods and the raw
/// goods of farms, quarries, mines, clay pits, reed gatherers and woodcutters.
const OURS_GOODS: [usize; 20] = [1, 2, 3, 4, 5, 6, 7, 8, 11, 14, 16, 18, 20, 22, 24, 25, 26, 29, 30, 32];
/// The prices window's resources, in its order (0x5760c0): gold and the unused 27 are
/// left out; 18 starts the second row.
const PRICE_ORDER: [usize; 33] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 22, 23, 24, 25, 26, 28, 29, 30, 31, 32, 33, 34, 35];

/// Where each price's column is in the prices window: its row (0, 1) and column (1 on).
fn price_slot(r: usize) -> (usize, usize) {
    if r < 18 {
        (0, PRICE_ORDER.iter().position(|&x| x == r).unwrap_or(0) + 1)
    } else {
        (1, PRICE_ORDER.iter().filter(|&&x| (18..=r).contains(&x)).count())
    }
}

/// The 640x480 window's corner, centred on the screen.
fn origin(screen: [f32; 2]) -> (f32, f32) {
    (((screen[0] - 640.0) / 2.0).floor(), ((screen[1] - 480.0) / 2.0).floor())
}

/// The demand window's buttons (table 0x5c3488, from (x+16, y+32)): 8 a row.
fn demand_button(screen: [f32; 2], i: usize) -> [f32; 4] {
    let (x, y) = origin(screen);
    let (c, row) = ((i % 8) as f32, (i / 8) as f32);
    [x + 16.0 + 25.0 + 50.0 * c, y + 32.0 + 90.0 + 60.0 * row, 50.0, 22.0]
}

/// A price's buying or selling button in the prices window (table 0x5c3648).
fn price_button(screen: [f32; 2], r: usize, sell: bool) -> [f32; 4] {
    let (x, y) = origin(screen);
    let (row, col) = price_slot(r);
    [x - 19.0 + 35.0 * col as f32 + 34.0, y + 80.0 + 75.0 * row as f32 + if sell { 25.0 } else { 0.0 }, 35.0, 20.0]
}

fn reset_prices_button(screen: [f32; 2]) -> [f32; 4] {
    let (x, y) = origin(screen);
    [x + 16.0 + 450.0, y + 205.0, 120.0, 35.0]
}

/// The list window: entries in columns of 21, centred on the screen (the Options
/// screen's lists).
fn list_layout(screen: [f32; 2], n: usize) -> ([f32; 4], Vec<[f32; 4]>) {
    let cols = n.div_ceil(21).max(1);
    let rows = n.min(21);
    let pw = (cols * 12) as f32 * 16.0;
    let ph = (rows as f32 * 18.0 + 32.0).max(64.0);
    let (x, y) = (((screen[0] - pw) / 2.0).floor(), ((screen[1] - ph) / 2.0).floor().max(24.0));
    let rects = (0..n).map(|i| [x + 8.0 + 192.0 * (i / 21) as f32, y + 16.0 + 18.0 * (i % 21) as f32, 176.0, 18.0]).collect();
    ([x, y, pw, ph], rects)
}

/// The keypad (as the Options screen's): its corner, digit keys, Accept and Cancel.
fn keypad_layout(screen: [f32; 2]) -> ((f32, f32), Vec<(char, [f32; 4])>, [f32; 4], [f32; 4]) {
    let (x, y) = (((screen[0] - 208.0) / 2.0).floor(), ((screen[1] - 256.0) / 2.0).floor());
    let keys = ['7', '8', '9', '4', '5', '6', '1', '2', '3', '0'];
    let rects = keys
        .iter()
        .enumerate()
        .map(|(i, &k)| {
            let (c, r) = if i == 9 { (1, 3) } else { (i % 3, i / 3) };
            (k, [x + 32.0 + 50.0 * c as f32, y + 60.0 + 36.0 * r as f32, 42.0, 30.0])
        })
        .collect();
    ((x, y), rects, [x + 16.0, y + 212.0, 84.0, 25.0], [x + 108.0, y + 212.0, 84.0, 25.0])
}

/// The panel's boxes for the object being edited, from the tables at 0x5c3dc0 on
/// (placed from (223, h-114)).
struct Boxes {
    city_type: [f32; 4],
    name: [f32; 4],
    align: [f32; 4],
    route: [f32; 4],
    demand: [f32; 4],
    cost: [f32; 4],
    region: [f32; 4],
    path: [f32; 4],
    order: [f32; 4],
    /// Route modes: the route's number, its kind, and Delete route.
    route_id: [f32; 4],
    route_kind: [f32; 4],
    route_delete: [f32; 4],
    prices: [f32; 4],
    /// Add object's three kinds.
    kinds: [[f32; 4]; 3],
    /// A picture's five buttons (table 0x5c8c60, from (223, h-114)): the up arrow
    /// (back one), the down arrow (on one), and three plain buttons for the first,
    /// middle and last picture.
    arrows: [[f32; 4]; 5],
}

fn boxes(h: f32) -> Boxes {
    Boxes {
        city_type: [393.0, h - 112.0, 180.0, 20.0],
        name: [593.0, h - 112.0, 180.0, 20.0],
        align: [293.0, h - 112.0, 100.0, 20.0],
        route: [453.0, h - 87.0, 50.0, 20.0],
        demand: [523.0, h - 87.0, 100.0, 20.0],
        cost: [703.0, h - 87.0, 80.0, 20.0],
        region: [393.0, h - 84.0, 180.0, 20.0],
        path: [523.0, h - 68.0, 50.0, 20.0],
        order: [523.0, h - 38.0, 50.0, 20.0],
        route_id: [393.0, h - 104.0, 180.0, 25.0],
        route_kind: [393.0, h - 79.0, 180.0, 25.0],
        route_delete: [393.0, h - 29.0, 180.0, 25.0],
        prices: [231.0, h - 34.0, 150.0, 20.0],
        kinds: [[393.0, h - 106.0, 180.0, 20.0], [393.0, h - 81.0, 180.0, 20.0], [393.0, h - 56.0, 180.0, 20.0]],
        arrows: [
            [231.0, h - 106.0, 24.0, 24.0],
            [231.0, h - 82.0, 24.0, 24.0],
            [255.0, h - 106.0, 24.0, 24.0],
            [255.0, h - 82.0, 24.0, 24.0],
            [255.0, h - 58.0, 24.0, 24.0],
        ],
    }
}

/// Our city's 14 goods boxes (two rows of seven), and another city's 8 sold and 8
/// bought: the icon's corner.
fn ours_slot(h: f32, i: usize) -> [f32; 4] {
    let (c, row) = ((i % 7) as f32, (i / 7) as f32);
    [399.0 + 27.0 * c, h - 61.0 + 28.0 * row, 24.0, 24.0]
}

fn trade_slot(h: f32, i: usize, buys: bool) -> [f32; 4] {
    [524.0 + 30.0 * i as f32, if buys { h - 33.0 } else { h - 61.0 }, 24.0, 24.0]
}

/// The first unused object: the original stops at it, drawing and counting only the
/// objects before it.
fn live(objects: &[EmpireObject]) -> usize {
    objects.iter().position(|o| !o.in_use).unwrap_or(objects.len())
}

impl Kingdom {
    fn open(e: &Editor) -> Self {
        let mut k = Kingdom { route: 1, ..Default::default() };
        // Where the map was left for this empire (the file's copy of the header of
        // Pharaoh2.emp, one 32-byte entry per empire: FUN_0040a090 keeps the scroll
        // there), else our city.
        let id = e.scenario.info.empire_id.max(0) as usize;
        let at = e.template.get("junk11").and_then(|d| d.get(id * 32..id * 32 + 4)).map(|b| [i16::from_le_bytes([b[0], b[1]]), i16::from_le_bytes([b[2], b[3]])]);
        if let Some([x, y]) = at.filter(|p| *p != [0, 0]) {
            k.scroll = [x as f32, y as f32];
            k.placed = true;
        }
        k
    }

    /// Closes the keypad, list, message or window open; false when none was.
    pub fn close_popups(&mut self) -> bool {
        self.grab = None;
        self.notice.take().is_some() || self.warning.take().is_some() || self.keypad.take().is_some() || self.list.take().is_some() || self.window.take().is_some()
    }

    pub fn tick(&mut self, dt: f32) {
        self.clock += dt;
    }

    fn clamp(&mut self, screen: [f32; 2]) {
        let v = view(screen);
        self.scroll[0] = self.scroll[0].clamp(0.0, (MAP_W - v[2]).max(0.0));
        self.scroll[1] = self.scroll[1].clamp(0.0, (MAP_H - v[3]).max(0.0));
    }

    fn to_screen(&self, screen: [f32; 2], x: i32, y: i32) -> [f32; 2] {
        let v = view(screen);
        [v[0] + x as f32 - self.scroll[0].floor(), v[1] + y as f32 - self.scroll[1].floor()]
    }

    fn to_map(&self, screen: [f32; 2], p: [f32; 2]) -> (i32, i32) {
        let v = view(screen);
        ((p[0] - v[0] + self.scroll[0].floor()) as i32, (p[1] - v[1] + self.scroll[1].floor()) as i32)
    }

    fn over_map(screen: [f32; 2], p: [f32; 2]) -> bool {
        inside(view(screen), p)
    }
}

impl Editor {
    /// Opens the Kingdom map.
    pub fn open_kingdom(&mut self) {
        self.view.menu = None;
        self.view.kingdom = Some(Kingdom::open(self));
    }

    pub fn kingdom_open(&self) -> bool {
        self.view.kingdom.is_some()
    }

    /// The live objects: those before the first unused one.
    fn objects(&self) -> &[EmpireObject] {
        let o = &self.scenario.empire.objects;
        &o[..live(o)]
    }

    fn selected_object(&self) -> Option<&EmpireObject> {
        let k = self.view.kingdom.as_ref()?;
        self.objects().get(k.selected?)
    }

    fn selected_mut(&mut self) -> Option<&mut EmpireObject> {
        let i = self.view.kingdom.as_ref()?.selected?;
        let n = live(&self.scenario.empire.objects);
        (i < n).then(|| &mut self.scenario.empire.objects[i])
    }

    /// The object under map pixel `(x, y)`: the last drawn, which is on top.
    fn object_at(&self, x: i32, y: i32) -> Option<usize> {
        self.objects().iter().enumerate().rev().find_map(|(i, o)| {
            let (w, h) = if o.kind == object::REGION { (90, 20) } else { (o.width, o.height) };
            (o.kind < 4 && x >= o.x && x < o.x + w.max(8) && y >= o.y && y < o.y + h.max(8)).then_some(i)
        })
    }

    /// A city's picture and its size, for its kind and name (FUN_004425c0).
    fn refresh_city_image(&mut self, lib: &ImageLibrary, i: usize) {
        let Ok(images) = EmpireImages::load(lib) else { return };
        let o = &mut self.scenario.empire.objects[i];
        if o.kind != object::CITY {
            return;
        }
        o.image_id = images.city_image(o.city_type, o.city_name_id, false) as u16;
        let (w, h) = size_of(lib, o.image_id as u32);
        o.width = w;
        o.height = h;
    }

    /// Marks the selected object edited: the original sets byte 0x33 of the record
    /// to 10 whenever a field of the edit panel is changed.
    fn touched(&mut self) {
        if let Some(o) = self.selected_mut()
            && o.raw.len() > 0x33
        {
            o.raw[0x33] = 10;
        }
        self.dirty = true;
    }

    /// Arrow keys scroll the Kingdom map.
    pub fn scroll_kingdom(&mut self, screen: [f32; 2], dx: f32, dy: f32) {
        if let Some(k) = &mut self.view.kingdom {
            k.scroll[0] += dx;
            k.scroll[1] += dy;
            k.clamp(screen);
        }
    }

    /// Leaves the Kingdom map (OK): not with more than four foods about (FUN_00443710:
    /// the foods our city grows or any city with a route sells), else the scroll is
    /// kept for this empire as the original keeps it.
    pub(super) fn leave_kingdom(&mut self) {
        let mut foods = [false; 9];
        for o in self.objects().iter().filter(|o| o.kind == object::CITY) {
            let n = if o.city_type == city::OURS {
                8
            } else if o.city_type > 6 || o.trade_route_id == 0 {
                continue;
            } else {
                MAX_SELLS
            };
            for &r in o.sells.iter().take(n) {
                if (1..=8).contains(&r) {
                    foods[r as usize] = true;
                }
            }
        }
        if foods.iter().filter(|&&f| f).count() > 4 {
            if let Some(k) = &mut self.view.kingdom {
                k.notice = Some(125);
            }
            return;
        }
        let Some(k) = self.view.kingdom.take() else { return };
        let id = self.scenario.info.empire_id.max(0) as usize;
        if let Some(d) = self.template.get_mut("junk11")
            && let Some(b) = d.get_mut(id * 32..id * 32 + 4)
        {
            let (x, y) = (k.scroll[0] as i16, k.scroll[1] as i16);
            if b[..2] != x.to_le_bytes() || b[2..] != y.to_le_bytes() {
                b[..2].copy_from_slice(&x.to_le_bytes());
                b[2..].copy_from_slice(&y.to_le_bytes());
                self.dirty = true;
            }
        }
    }

    /// Reset (FUN_00444270): the Kingdom as Pharaoh2.emp holds it, every city
    /// foreign, with its place and goods; every route cleared.
    fn reset_kingdom(&mut self) {
        let path = self.data.join("Pharaoh2.emp");
        let objects = std::fs::read(&path).map_err(anyhow::Error::from).and_then(|d| empire::Empire::default_objects(&d).map_err(anyhow::Error::from));
        match objects {
            Ok(objects) => {
                let e = &mut self.scenario.empire;
                e.objects = objects;
                for r in &mut e.routes {
                    r.in_use = false;
                    r.points.clear();
                    r.step = 5;
                    r.route_type = 1;
                    r.from_object = -1;
                    r.to_object = -1;
                }
                if let Some(k) = &mut self.view.kingdom {
                    k.selected = None;
                    k.route = 1;
                }
                self.dirty = true;
            }
            Err(err) => self.view.say(&format!("Could not read {}: {err}", path.display())),
        }
    }

    /// The route Add route starts: the first of 1-19 not in use (FUN_00407340, which
    /// also drops routes of fewer than two points).
    fn next_free_route(&mut self) -> Option<usize> {
        let routes = &mut self.scenario.empire.routes;
        for (i, r) in routes.iter_mut().enumerate().take(MAX_ROUTE_ID + 1).skip(1) {
            if r.points.len() < 2 {
                r.in_use = false;
            }
            if !r.in_use {
                return Some(i);
            }
        }
        None
    }

    /// The cities a route's first and last points lie in (worked out after every
    /// edit of a route, as the original does), -1 where none.
    fn route_ends(&mut self, id: usize) {
        let objects = self.objects().to_vec();
        let Some(r) = self.scenario.empire.routes.get_mut(id) else { return };
        let within = |p: (i32, i32)| {
            objects.iter().enumerate().filter(|(_, o)| o.kind == object::CITY && p.0 >= o.x && p.0 <= o.x + o.width && p.1 >= o.y && p.1 <= o.y + o.height).map(|(i, _)| i as i16).last().unwrap_or(-1)
        };
        r.from_object = r.points.first().map_or(-1, |&p| within(p));
        r.to_object = r.points.last().map_or(-1, |&p| within(p));
        r.in_use = r.points.len() >= 2;
    }

    fn main_button(&mut self, lib: &ImageLibrary, b: Main) {
        let Some(k) = &mut self.view.kingdom else { return };
        k.grab = None;
        match b {
            Main::Mode(Mode::AddRoute) => {
                if let Some(id) = self.next_free_route() {
                    let r = &mut self.scenario.empire.routes[id];
                    r.points.clear();
                    r.in_use = false;
                    if !(1..=50).contains(&r.step) {
                        r.step = 5;
                    }
                    if !matches!(r.route_type, 1 | 2) {
                        r.route_type = 1;
                    }
                    if let Some(k) = &mut self.view.kingdom {
                        k.route = id;
                        k.mode = Mode::AddRoute;
                    }
                }
            }
            Main::Mode(Mode::EditRoute) => {
                k.route = 1;
                k.mode = Mode::EditRoute;
            }
            Main::Mode(Mode::Edit) => {
                k.mode = Mode::Edit;
                let sel = k.selected;
                if let Some(i) = sel {
                    self.refresh_city_image(lib, i);
                }
            }
            Main::Mode(m) => k.mode = m,
            Main::Delete => {
                if let Some(i) = k.selected.take() {
                    let objects = &mut self.scenario.empire.objects;
                    let end = live(objects);
                    if i < end {
                        objects[i..end].rotate_left(1);
                        objects[end - 1] = EmpireObject::default();
                        self.dirty = true;
                    }
                }
                if let Some(k) = &mut self.view.kingdom {
                    k.mode = Mode::Edit;
                }
            }
            Main::Reset => self.reset_kingdom(),
            Main::Ok => self.leave_kingdom(),
        }
    }

    /// Places a new object of the kind Add object has chosen, centred on map pixel
    /// `(x, y)` (FUN_00407620), and selects it for editing.
    fn add_object(&mut self, lib: &ImageLibrary, x: i32, y: i32) {
        let Some(art) = Art::load(lib) else { return };
        let Some(k) = &self.view.kingdom else { return };
        let (kind, ornament) = (k.add_kind, k.ornament);
        let objects = &self.scenario.empire.objects;
        let slot = live(objects);
        if slot >= objects.len() {
            return;
        }
        if kind == 1 && objects[..slot].iter().filter(|o| o.kind == object::CITY).count() >= MAX_CITIES {
            if let Some(k) = &mut self.view.kingdom {
                k.notice = Some(34);
                k.mode = Mode::General;
            }
            return;
        }
        let mut o = EmpireObject { in_use: true, demand: vec![0; empire::RESOURCES], ..Default::default() };
        match kind {
            0 => {
                o.kind = object::ORNAMENT;
                o.image_id = (art.ornaments + ornament) as u16;
                o.expanded_image_id = ornament as u16;
                (o.width, o.height) = size_of(lib, o.image_id as u32);
            }
            1 => {
                // A new city is an Egyptian trade city (FUN_00407620).
                o.kind = object::CITY;
                o.city_type = city::EGYPTIAN_TRADING;
                o.image_id = art.empire.city_image(o.city_type, 0, false) as u16;
                (o.width, o.height) = size_of(lib, o.image_id as u32);
            }
            _ => {
                o.kind = object::REGION;
                (o.width, o.height) = (90, 20);
            }
        }
        o.x = (x - o.width / 2).clamp(0, MAP_W as i32 - 1);
        o.y = (y - o.height / 2).clamp(0, MAP_H as i32 - 1);
        self.scenario.empire.objects[slot] = o;
        self.dirty = true;
        if let Some(k) = &mut self.view.kingdom {
            k.selected = Some(slot);
            k.mode = Mode::Edit;
        }
    }

    fn move_selected(&mut self, x: i32, y: i32) {
        if let Some(o) = self.selected_mut() {
            let (w, h) = if o.kind == object::REGION { (90, 20) } else { (o.width, o.height) };
            let (nx, ny) = ((x - w / 2).clamp(0, MAP_W as i32 - 1), (y - h / 2).clamp(0, MAP_H as i32 - 1));
            if (o.x, o.y) != (nx, ny) {
                (o.x, o.y) = (nx, ny);
                self.dirty = true;
            }
        }
    }

    /// A left click on the Kingdom map's screen.
    pub fn kingdom_press(&mut self, lib: &ImageLibrary, screen: [f32; 2], p: [f32; 2]) {
        if self.view.kingdom.is_none() {
            return;
        }
        // Kept as an undo step when the click (and any drag after it) changes the
        // Kingdom; kingdom_release closes it.
        self.kingdom_mark();
        let Some(k) = &mut self.view.kingdom else { return };
        k.cursor = p;
        if k.notice.take().is_some() || k.warning.take().is_some() {
            return;
        }
        if k.keypad.is_some() {
            self.keypad_press(screen, p);
            return;
        }
        if k.list.is_some() {
            self.list_press(lib, screen, p);
            return;
        }
        match k.window {
            Some(Window::Demand) => {
                self.demand_press(screen, p);
                return;
            }
            Some(Window::Prices) => {
                self.prices_press(screen, p);
                return;
            }
            None => {}
        }
        let h = screen[1];
        if let Some(&(b, _, _)) = main_buttons(h).iter().find(|(_, _, r)| inside(*r, p)) {
            self.main_button(lib, b);
            return;
        }
        if p[1] >= h - DIVIDER {
            self.panel_press(lib, h, p);
            return;
        }
        if !Kingdom::over_map(screen, p) {
            return;
        }
        let Some(k) = &self.view.kingdom else { return };
        let (x, y) = k.to_map(screen, p);
        match k.mode {
            Mode::Add => self.add_object(lib, x, y),
            Mode::Edit => match k.selected {
                Some(_) => {
                    self.move_selected(x, y);
                    if let Some(k) = &mut self.view.kingdom {
                        k.grab = Some(Grab::Object);
                    }
                }
                None => {
                    let hit = self.object_at(x, y);
                    if let Some(k) = &mut self.view.kingdom {
                        k.selected = hit;
                    }
                }
            },
            Mode::General => {}
            Mode::AddRoute => {
                let id = k.route;
                if let Some(r) = self.scenario.empire.routes.get_mut(id)
                    && r.points.len() < MAX_POINTS
                {
                    r.points.push((x, y));
                    self.route_ends(id);
                    self.dirty = true;
                }
            }
            Mode::EditRoute => {
                let id = k.route;
                let Some(r) = self.scenario.empire.routes.get_mut(id) else { return };
                let near = |q: (i32, i32)| (q.0 - x).abs() <= NEAR && (q.1 - y).abs() <= NEAR;
                if let Some(i) = r.points.iter().position(|&q| near(q)) {
                    if let Some(k) = &mut self.view.kingdom {
                        k.grab = Some(Grab::Waypoint(i));
                    }
                } else if r.points.len() >= 2 && r.points.len() < MAX_POINTS {
                    // A click on the route itself adds a waypoint there, after the
                    // segment's first waypoint.
                    let step = r.spacing() as f32;
                    let mut at = None;
                    for (s, pair) in r.points.windows(2).enumerate() {
                        let (a, b) = (pair[0], pair[1]);
                        let (dx, dy) = ((b.0 - a.0) as f32, (b.1 - a.1) as f32);
                        let len = (dx * dx + dy * dy).sqrt();
                        let mut d = 0.0;
                        while len > 0.0 && d <= len {
                            if near(((a.0 as f32 + dx * d / len) as i32, (a.1 as f32 + dy * d / len) as i32)) {
                                at = Some(s + 1);
                                break;
                            }
                            d += step;
                        }
                        if at.is_some() {
                            break;
                        }
                    }
                    if let Some(i) = at {
                        r.points.insert(i, (x, y));
                        self.route_ends(id);
                        self.dirty = true;
                        if let Some(k) = &mut self.view.kingdom {
                            k.grab = Some(Grab::Waypoint(i));
                        }
                    }
                }
            }
        }
    }

    /// A right-click on the Kingdom map's screen: closes what is open over it, else
    /// selects the object under the mouse (Edit objects), finishes the route being
    /// drawn (Add route) or deletes the waypoint under the mouse (Edit route).
    pub fn kingdom_right(&mut self, screen: [f32; 2], p: [f32; 2]) {
        self.kingdom_mark();
        self.kingdom_right_click(screen, p);
        self.kingdom_commit();
    }

    fn kingdom_right_click(&mut self, screen: [f32; 2], p: [f32; 2]) {
        let Some(k) = &mut self.view.kingdom else { return };
        if k.close_popups() {
            return;
        }
        if !Kingdom::over_map(screen, p) {
            return;
        }
        let (x, y) = k.to_map(screen, p);
        match k.mode {
            Mode::Edit | Mode::Add => {
                let hit = self.object_at(x, y);
                if let Some(k) = &mut self.view.kingdom {
                    k.selected = hit;
                    k.mode = Mode::Edit;
                    if let Some(i) = hit {
                        k.ornament = self.scenario.empire.objects[i].expanded_image_id as u32;
                    }
                }
            }
            Mode::AddRoute => {
                let id = k.route;
                k.mode = Mode::EditRoute;
                self.route_ends(id);
            }
            Mode::EditRoute => {
                let id = k.route;
                if let Some(r) = self.scenario.empire.routes.get_mut(id)
                    && let Some(i) = r.points.iter().position(|&q| (q.0 - x).abs() <= NEAR && (q.1 - y).abs() <= NEAR)
                {
                    r.points.remove(i);
                    self.route_ends(id);
                    self.dirty = true;
                }
            }
            Mode::General => {}
        }
    }

    /// The mouse moved to `p`: a held object or waypoint follows it.
    pub fn kingdom_move(&mut self, screen: [f32; 2], p: [f32; 2]) {
        let Some(k) = &mut self.view.kingdom else { return };
        k.cursor = p;
        let (x, y) = k.to_map(screen, p);
        let (x, y) = (x.clamp(0, MAP_W as i32 - 1), y.clamp(0, MAP_H as i32 - 1));
        match k.grab {
            Some(Grab::Object) => self.move_selected(x, y),
            Some(Grab::Waypoint(i)) => {
                let id = k.route;
                if let Some(q) = self.scenario.empire.routes.get_mut(id).and_then(|r| r.points.get_mut(i))
                    && *q != (x, y)
                {
                    *q = (x, y);
                    self.route_ends(id);
                    self.dirty = true;
                }
            }
            None => {}
        }
    }

    pub fn kingdom_release(&mut self) {
        if let Some(k) = &mut self.view.kingdom {
            k.grab = None;
        }
        self.kingdom_commit();
    }

    /// A click in the panel's right-hand box.
    fn panel_press(&mut self, lib: &ImageLibrary, h: f32, p: [f32; 2]) {
        let Some(k) = &self.view.kingdom else { return };
        let b = boxes(h);
        match k.mode {
            Mode::Add => {
                let art_last = Art::load(lib).map_or(0, |a| a.ornament_count - 1);
                for (i, r) in b.kinds.iter().enumerate() {
                    if inside(*r, p) {
                        if let Some(k) = &mut self.view.kingdom {
                            k.add_kind = i as u8;
                            // Choosing pictures starts at the last one (0x4081f0).
                            if i == 0 {
                                k.ornament = art_last;
                            }
                        }
                        return;
                    }
                }
                if k.add_kind == 0 {
                    self.ornament_arrows(lib, &b, p);
                }
            }
            Mode::General => {
                if inside(b.prices, p)
                    && let Some(k) = &mut self.view.kingdom
                {
                    k.window = Some(Window::Prices);
                }
            }
            Mode::AddRoute => {}
            Mode::EditRoute => {
                let id = k.route;
                if inside(b.route_id, p) {
                    self.open_keypad(Num::RouteId, id as i32);
                } else if inside(b.route_kind, p) {
                    if let Some(r) = self.scenario.empire.routes.get_mut(id) {
                        r.route_type = if r.route_type == 1 { 2 } else { 1 };
                        self.dirty = true;
                    }
                } else if inside(b.route_delete, p) {
                    if let Some(r) = self.scenario.empire.routes.get_mut(id) {
                        r.points.clear();
                        r.in_use = false;
                        self.dirty = true;
                    }
                    if let Some(k) = &mut self.view.kingdom
                        && k.route > 1
                    {
                        k.route -= 1;
                    }
                }
            }
            Mode::Edit => {
                let Some(o) = self.selected_object().cloned() else { return };
                match o.kind {
                    object::ORNAMENT => self.ornament_arrows(lib, &b, p),
                    object::REGION if inside(b.region, p) => self.open_list(Pick::Region, 196, (0..17).collect()),
                    object::BATTLE_ICON if inside(b.path, p) => self.open_keypad(Num::Path, o.invasion_path as i32),
                    object::BATTLE_ICON if inside(b.order, p) => self.open_keypad(Num::Order, o.invasion_years as i32),
                    object::CITY => self.city_press(&o, h, p),
                    _ => {}
                }
            }
        }
    }

    /// The picture buttons (0x4074e0): back one and on one, wrapping round, or
    /// straight to the first, the middle or the last picture.
    fn ornament_arrows(&mut self, lib: &ImageLibrary, b: &Boxes, p: [f32; 2]) {
        let Some(art) = Art::load(lib) else { return };
        let Some(k) = &mut self.view.kingdom else { return };
        let last = art.ornament_count - 1;
        let Some(button) = b.arrows.iter().position(|&r| inside(r, p)) else { return };
        k.ornament = match button {
            0 => k.ornament.checked_sub(1).unwrap_or(last),
            1 if k.ornament >= last => 0,
            1 => k.ornament + 1,
            2 => 0,
            3 => last / 2,
            _ => last,
        };
        let (orn, editing) = (k.ornament, k.mode == Mode::Edit);
        if editing && let Some(o) = self.selected_mut() {
            o.image_id = (art.ornaments + orn) as u16;
            o.expanded_image_id = orn as u16;
            (o.width, o.height) = size_of(lib, o.image_id as u32);
            self.dirty = true;
        }
    }

    fn city_press(&mut self, o: &EmpireObject, h: f32, p: [f32; 2]) {
        let b = boxes(h);
        if inside(b.city_type, p) {
            self.open_list(Pick::CityType, 39, (0..7).collect());
        } else if inside(b.name, p) {
            self.open_list(Pick::CityName, 195, (0..66).collect());
        } else if inside(b.align, p) {
            if let Some(o) = self.selected_mut() {
                o.text_align = (o.text_align + 1) % 4;
            }
            self.touched();
        } else if inside(b.route, p) {
            self.open_keypad(Num::Route, o.trade_route_id as i32);
        } else if o.city_type != city::OURS && inside(b.demand, p) {
            if let Some(k) = &mut self.view.kingdom {
                k.window = Some(Window::Demand);
            }
        } else if inside(b.cost, p) {
            self.open_keypad(Num::Cost, o.trade_route_cost as i32);
        } else if o.city_type == city::OURS {
            if let Some(i) = (0..MAX_SELLS).find(|&i| inside(ours_slot(h, i), p)) {
                let taken: Vec<usize> = o.sells.iter().enumerate().filter(|&(j, _)| j != i).map(|(_, &r)| r as usize).collect();
                let ids = std::iter::once(0).chain(OURS_GOODS.iter().copied().filter(|r| !taken.contains(r))).collect();
                self.open_list(Pick::Ours(i), 23, ids);
            }
        } else {
            for (buys, n) in [(false, 8), (true, MAX_BUYS)] {
                if let Some(i) = (0..n).find(|&i| inside(trade_slot(h, i, buys), p)) {
                    let list = if buys { &o.buys } else { &o.sells };
                    let taken: Vec<usize> = list.iter().enumerate().filter(|&(j, _)| j != i).map(|(_, &r)| r as usize).collect();
                    let ids = std::iter::once(0).chain((1..empire::RESOURCES).filter(|&r| r != 27 && !taken.contains(&r))).collect();
                    self.open_list(if buys { Pick::Buys(i) } else { Pick::Sells(i) }, 23, ids);
                    return;
                }
            }
        }
    }

    fn open_list(&mut self, pick: Pick, group: usize, ids: Vec<usize>) {
        if let Some(k) = &mut self.view.kingdom {
            k.list = Some(List { pick, group, ids });
        }
    }

    fn open_keypad(&mut self, num: Num, value: i32) {
        if let Some(k) = &mut self.view.kingdom {
            k.keypad = Some(Keypad { num, typed: value.to_string(), fresh: true });
        }
    }

    fn list_press(&mut self, lib: &ImageLibrary, screen: [f32; 2], p: [f32; 2]) {
        let Some(k) = &mut self.view.kingdom else { return };
        let Some(list) = k.list.take() else { return };
        let (frame, rects) = list_layout(screen, list.ids.len());
        let Some(i) = rects.iter().position(|r| inside(*r, p)) else {
            if inside(frame, p) {
                k.list = Some(list);
            }
            return;
        };
        let id = list.ids[i];
        let Some(sel) = k.selected else { return };
        match list.pick {
            Pick::CityType | Pick::CityName => {
                if let Some(o) = self.selected_mut() {
                    if list.pick == Pick::CityType {
                        o.city_type = id as u8;
                    } else {
                        o.city_name_id = id as u8;
                    }
                }
                self.refresh_city_image(lib, sel);
                if list.pick == Pick::CityType {
                    self.check_city(sel);
                    for i in 0..live(&self.scenario.empire.objects) {
                        self.refresh_city_image(lib, i);
                    }
                }
            }
            Pick::Region => {
                if let Some(o) = self.selected_mut() {
                    o.city_name_id = id as u8;
                }
            }
            Pick::Ours(slot) | Pick::Sells(slot) | Pick::Buys(slot) => {
                let buys = matches!(list.pick, Pick::Buys(_));
                let Some(o) = self.selected_mut() else { return };
                let trading = o.city_type != city::OURS;
                let (list, other) = if buys { (&mut o.buys, o.sells.clone()) } else { (&mut o.sells, o.buys.clone()) };
                let old = list.get(slot).copied().unwrap_or(0);
                if slot < list.len() {
                    if id == 0 {
                        list.remove(slot);
                    } else {
                        list[slot] = id as u8;
                    }
                } else if id != 0 {
                    list.push(id as u8);
                }
                // The demand follows (FUN_00408710): a good no longer sold or bought
                // loses it, a new one starts at the middle tier.
                if trading {
                    o.demand.resize(empire::RESOURCES, 0);
                    if old != 0 && old as usize != id && !other.contains(&old) && !list.contains(&old) {
                        o.demand[old as usize] = 0;
                    }
                    if id != 0 && o.demand[id] == 0 && !other.contains(&(id as u8)) {
                        o.demand[id] = 2;
                    }
                }
                self.touched();
                self.check_city(sel);
                return;
            }
        }
        self.touched();
    }

    /// What follows an edit of city `i`'s kind or goods (FUN_00442a40, FUN_00442ef0,
    /// FUN_00442f80, run by the original after each input on a city's panel): our city
    /// loses meat it can't raise without straw; the goods lists are sorted; monuments
    /// and buildings the city can no longer get what they need for are taken out of
    /// the scenario, the first with a warning; and the city edited stays the only
    /// "our city" and the only Pharaoh's city, the others becoming Egyptian ones.
    /// Shows warning `n` of text group 19 over the Kingdom map (for scripts).
    pub(super) fn kingdom_warning(&mut self, n: usize) {
        if let Some(k) = &mut self.view.kingdom {
            k.warning = Some(n);
        }
    }

    pub(super) fn check_city(&mut self, i: usize) {
        let mut warning = None;
        let s = &mut self.scenario;
        let Some(o) = s.empire.objects.get(i).filter(|o| o.in_use && o.kind == object::CITY) else { return };
        if o.city_type == city::OURS && o.sells.contains(&2) && !obtainable(s, 9) {
            let o = &mut s.empire.objects[i];
            if let Some(at) = o.sells.iter().position(|&r| r == 2) {
                o.sells.remove(at);
            }
            warning = Some(69);
        }
        let o = &mut s.empire.objects[i];
        for list in [&mut o.sells, &mut o.buys] {
            list.sort_unstable();
            list.dedup();
            list.retain(|&r| r != 0);
        }
        let mut gone = false;
        for n in 0..3 {
            if !super::options::monument_obtainable(s, s.info.monuments[n] as usize) {
                s.info.monuments[n] = 0;
                gone = true;
            }
        }
        if gone {
            let rating = super::options::monument_rating(&s.info);
            s.info.win.monuments.value = rating;
            s.info.win.monuments.enabled = rating != 0;
            warning = warning.or(Some(85));
        }
        if s.info.reserved.len() < 114 {
            s.info.reserved.resize(114, 0);
        }
        // Each building and the good (or building) it can't work without, and the
        // warning when it goes.
        let needs: [(usize, Need, usize); 14] = [
            (22, Need::Good(17), 176),
            (14, Need::Good(15), 177),
            (45, Need::Good(8), 234),
            (45, Need::Good(9), 234),
            (16, Need::Good(23), 178),
            (17, Need::Good(23), 179),
            (41, Need::Good(29), 180),
            (42, Need::Good(20), 181),
            (37, Need::Building(36), 182),
            (37, Need::Either(41, 10), 186),
            (38, Need::Building(36), 183),
            (39, Need::Building(36), 184),
            (39, Need::Either(42, 28), 187),
            (40, Need::Building(36), 185),
        ];
        for (b, need, text) in needs {
            if warning.is_some() || s.info.reserved[b] == 0 {
                continue;
            }
            let ok = match need {
                Need::Good(r) => obtainable(s, r),
                Need::Building(n) => s.info.reserved[n] != 0,
                Need::Either(n, r) => s.info.reserved[n] != 0 || obtainable(s, r),
            };
            if !ok {
                s.info.reserved[b] = 0;
                warning = Some(text);
            }
        }
        // One city of ours, one of Pharaoh's: the one just edited.
        let kind = s.empire.objects[i].city_type;
        let n = live(&s.empire.objects);
        for (j, o) in s.empire.objects[..n].iter_mut().enumerate() {
            if j == i || !o.in_use || o.kind != object::CITY {
                continue;
            }
            if kind == city::OURS && o.city_type == city::OURS {
                o.city_type = city::EGYPTIAN;
            }
            if matches!(kind, city::PHARAOH_TRADING | city::PHARAOH) && matches!(o.city_type, city::PHARAOH_TRADING | city::PHARAOH) {
                o.city_type = if o.city_type == city::PHARAOH_TRADING { city::EGYPTIAN_TRADING } else { city::EGYPTIAN };
            }
        }
        if let (Some(w), Some(k)) = (warning, &mut self.view.kingdom) {
            k.warning = Some(w);
        }
    }

    fn keypad_press(&mut self, screen: [f32; 2], p: [f32; 2]) {
        let (_, keys, accept, cancel) = keypad_layout(screen);
        let Some(k) = &mut self.view.kingdom else { return };
        let Some(pad) = &mut k.keypad else { return };
        if let Some(&(c, _)) = keys.iter().find(|(_, r)| inside(*r, p)) {
            if std::mem::take(&mut pad.fresh) {
                pad.typed.clear();
            }
            if pad.typed.len() < 6 {
                pad.typed.push(c);
            }
        } else if inside(accept, p) {
            self.type_kingdom("\n");
        } else if inside(cancel, p) {
            k.keypad = None;
        }
    }

    /// Whether the Kingdom map has a keypad open, taking typed digits.
    pub fn kingdom_wants_text(&self) -> bool {
        self.view.kingdom.as_ref().is_some_and(|k| k.keypad.is_some())
    }

    /// Typed digits, Backspace (`\u{8}`) and Enter on the Kingdom map's keypad.
    pub fn type_kingdom(&mut self, s: &str) {
        self.kingdom_mark();
        self.type_kingdom_keys(s);
        self.kingdom_commit();
    }

    fn type_kingdom_keys(&mut self, s: &str) {
        let Some(k) = &mut self.view.kingdom else { return };
        let Some(pad) = &mut k.keypad else { return };
        for c in s.chars() {
            match c {
                '0'..='9' => {
                    if std::mem::take(&mut pad.fresh) {
                        pad.typed.clear();
                    }
                    if pad.typed.len() < 6 {
                        pad.typed.push(c);
                    }
                }
                '\u{8}' => {
                    pad.fresh = false;
                    pad.typed.pop();
                }
                '\n' | '\r' => {
                    let (num, v) = (pad.num, pad.typed.parse::<i32>().unwrap_or(0));
                    k.keypad = None;
                    self.set_number(num, v);
                    return;
                }
                _ => {}
            }
        }
    }

    fn set_number(&mut self, num: Num, v: i32) {
        match num {
            Num::Route => {
                if let Some(o) = self.selected_mut() {
                    o.trade_route_id = v.clamp(0, MAX_ROUTE_ID as i32) as u8;
                }
                self.touched();
            }
            Num::Cost => {
                if let Some(o) = self.selected_mut() {
                    o.trade_route_cost = v.clamp(0, i16::MAX as i32) as u16;
                }
                self.touched();
            }
            Num::Path | Num::Order => {
                if let Some(o) = self.selected_mut() {
                    let v = v.clamp(0, 255) as u8;
                    if num == Num::Path {
                        o.invasion_path = v;
                    } else {
                        o.invasion_years = v;
                    }
                }
                self.touched();
            }
            Num::RouteId => {
                // FUN_00408120: a route of 1-19; one not in use is started afresh
                // with Add route.
                let id = v as usize;
                if !(1..=MAX_ROUTE_ID).contains(&id) {
                    return;
                }
                let Some(r) = self.scenario.empire.routes.get_mut(id) else { return };
                if r.points.len() < 2 {
                    r.in_use = false;
                }
                let fresh = !r.in_use;
                if fresh {
                    r.points.clear();
                    if !(1..=50).contains(&r.step) {
                        r.step = 5;
                    }
                    if !matches!(r.route_type, 1 | 2) {
                        r.route_type = 1;
                    }
                }
                if let Some(k) = &mut self.view.kingdom {
                    k.route = id;
                    if fresh {
                        k.mode = Mode::AddRoute;
                    }
                }
            }
            Num::Price(r, sell) => {
                let e = &mut self.scenario.empire;
                if e.prices.iter().all(|&p| p == (0, 0)) {
                    e.prices = empire::DEFAULT_PRICES.to_vec();
                }
                e.prices.resize(empire::RESOURCES, (0, 0));
                let v = v.clamp(0, 9999);
                if sell {
                    e.prices[r].1 = v;
                } else {
                    e.prices[r].0 = v;
                }
                self.dirty = true;
            }
        }
    }

    /// The goods the demand window lists for the selected city: those it sells or
    /// buys, in resource order, at most 16 (FUN_00408540).
    fn demand_goods(&self) -> Vec<usize> {
        let Some(o) = self.selected_object() else { return Vec::new() };
        (1..empire::RESOURCES).filter(|&r| o.sells.contains(&(r as u8)) || o.buys.contains(&(r as u8))).take(16).collect()
    }

    fn demand_press(&mut self, screen: [f32; 2], p: [f32; 2]) {
        let goods = self.demand_goods();
        if let Some(i) = (0..goods.len()).find(|&i| inside(demand_button(screen, i), p)) {
            let r = goods[i];
            if let Some(o) = self.selected_mut() {
                o.demand.resize(empire::RESOURCES, 0);
                o.demand[r] = (o.demand[r] + 1) % 4;
            }
            self.touched();
        }
    }

    /// Whether some city lists resource `r` among what it sells or buys: prices of
    /// the others are "N/A" (FUN_00443a80, FUN_00443ad0).
    fn available(&self, r: usize) -> bool {
        self.objects().iter().any(|o| o.kind == object::CITY && (o.sells.contains(&(r as u8)) || o.buys.contains(&(r as u8))))
    }

    fn prices(&self) -> Vec<(i32, i32)> {
        let p = &self.scenario.empire.prices;
        if p.iter().all(|&v| v == (0, 0)) { empire::DEFAULT_PRICES.to_vec() } else { p.clone() }
    }

    fn prices_press(&mut self, screen: [f32; 2], p: [f32; 2]) {
        if inside(reset_prices_button(screen), p) {
            let e = &mut self.scenario.empire;
            e.prices.resize(empire::RESOURCES, (0, 0));
            for &r in &PRICE_ORDER {
                e.prices[r] = empire::DEFAULT_PRICES[r];
            }
            self.dirty = true;
            return;
        }
        for &r in &PRICE_ORDER {
            for sell in [false, true] {
                if inside(price_button(screen, r, sell), p) {
                    if !self.available(r) {
                        if let Some(k) = &mut self.view.kingdom {
                            k.notice = Some(46);
                        }
                        return;
                    }
                    let v = self.prices().get(r).map_or(0, |&(b, s)| if sell { s } else { b });
                    self.open_keypad(Num::Price(r, sell), v);
                    return;
                }
            }
        }
    }

    // Drawing.

    /// Places the view for the first time: on our city, unless the map keeps where
    /// its designer left it.
    fn place_view(&mut self, screen: [f32; 2]) {
        let ours = self.objects().iter().find(|o| o.kind == object::CITY && o.city_type == city::OURS).map(|o| (o.x, o.y));
        let Some(k) = &mut self.view.kingdom else { return };
        if !k.placed {
            k.placed = true;
            let v = view(screen);
            if let Some((x, y)) = ours {
                k.scroll = [x as f32 - v[2] / 2.0, y as f32 - v[3] / 2.0];
            }
        }
        k.clamp(screen);
    }

    /// Carries out a scripted action.
    pub fn run_scripted(&mut self, lib: &ImageLibrary, screen: [f32; 2], a: Scripted) {
        self.place_view(screen);
        let Some(k) = &self.view.kingdom else { return };
        let map = |x: i32, y: i32| k.to_screen(screen, x, y);
        match a {
            Scripted::Click(p) => self.kingdom_press(lib, screen, p),
            Scripted::Right(p) => self.kingdom_right(screen, p),
            Scripted::MapClick(x, y) => {
                let p = map(x, y);
                self.kingdom_press(lib, screen, p);
            }
            Scripted::MapRight(x, y) => {
                let p = map(x, y);
                self.kingdom_right(screen, p);
            }
            Scripted::DragTo(x, y) => {
                let p = map(x, y);
                self.kingdom_move(screen, p);
                self.kingdom_release();
            }
            Scripted::Type(keys) => self.type_kingdom(&keys),
            Scripted::Scroll(x, y) => {
                if let Some(k) = &mut self.view.kingdom {
                    k.scroll = [x as f32, y as f32];
                    k.clamp(screen);
                }
            }
            Scripted::State => {
                for (i, o) in self.objects().iter().enumerate().filter(|(_, o)| o.kind == object::CITY) {
                    eprintln!("  object {i}: city type {} name {} at ({},{}) route {} cost {} sells {:?} buys {:?} demand {:?}", o.city_type, o.city_name_id, o.x, o.y, o.trade_route_id, o.trade_route_cost, o.sells, o.buys, o.demand);
                }
                for (i, rt) in self.scenario.empire.routes.iter().enumerate().filter(|(_, rt)| rt.in_use || !rt.points.is_empty()) {
                    eprintln!("  route {i}: in use {} type {} points {:?} from {} to {} length {}", rt.in_use, rt.route_type, rt.points, rt.from_object, rt.to_object, rt.length());
                }
                if let Some(k) = &self.view.kingdom {
                    eprintln!("  mode {:?} selected {:?} route {} scroll {:?} notice {:?}", k.mode, k.selected, k.route, k.scroll, k.notice);
                }
            }
            Scripted::Button(label) => {
                if let Some(&(b, _, _)) = main_buttons(screen[1]).iter().find(|(_, l, _)| *l == label) {
                    self.main_button(lib, b);
                }
            }
        }
    }

    pub fn draw_kingdom(&mut self, r: &mut Renderer, panels: &PanelImages) {
        let Some(art) = Art::load(&r.library) else { return };
        let screen = r.screen;
        let (w, h) = (screen[0], screen[1]);
        self.place_view(screen);
        let Some(k) = self.view.kingdom.as_ref() else { return };
        r.rect([0.0, 0.0], screen, [0.0, 0.0, 0.0, 1.0], Space::Screen);
        let v = view(screen);
        r.set_clip(Some(v));
        r.image(art.empire.map, [v[0] - k.scroll[0].floor(), v[1] - k.scroll[1].floor()], WHITE, Space::Screen);
        self.draw_objects(r, k, &art);
        self.draw_routes(r, k, &art);
        if let Some(o) = self.selected_object() {
            let (ow, oh) = if o.kind == object::REGION { (90, 20) } else { (o.width, o.height) };
            let at = k.to_screen(screen, o.x - 4, o.y - 4);
            dotted_rect(r, at, [(ow + 8) as f32, (oh + 8) as f32], k.clock);
        }
        // Add object: what a click will place, under the mouse.
        if k.mode == Mode::Add && Kingdom::over_map(screen, k.cursor) && k.window.is_none() {
            let image = match k.add_kind {
                0 => Some(art.ornaments + k.ornament),
                1 => Some(art.empire.city_image(city::EGYPTIAN_TRADING, 0, false)),
                _ => None,
            };
            match image {
                Some(id) => {
                    let (iw, ih) = r.record(id).map_or((0.0, 0.0), |rec| (rec.width as f32, rec.height as f32));
                    r.image(id, [k.cursor[0] - (iw / 2.0).floor(), k.cursor[1] - (ih / 2.0).floor()], [1.0, 1.0, 1.0, 0.7], Space::Screen);
                }
                None => {
                    let s = self.text.get(196, 0).unwrap_or("").trim().to_owned();
                    draw_text(r, Font::SmallPlain, &s, k.cursor[0] - 45.0, k.cursor[1] - 10.0, REGION);
                }
            }
        }
        r.set_clip(None);
        draw_frame(r, &art.empire);
        self.draw_kingdom_panel(r, k, &art, w, h);
        let k = self.view.kingdom.as_ref().expect("checked above");
        match k.window {
            Some(Window::Demand) => self.draw_demand(r, panels, k, &art),
            Some(Window::Prices) => self.draw_prices(r, panels, k, &art),
            None => {}
        }
        if let Some(list) = &k.list {
            self.draw_list(r, panels, k, list);
        }
        if let Some(pad) = &k.keypad {
            self.draw_kingdom_keypad(r, panels, k, pad);
        }
        if let Some(n) = k.notice
            && let Ok(img) = UiImages::load(&r.library)
        {
            let c = crate::popup::Confirm::from_text(&self.text, 5, n);
            c.draw(r, panels, img, &self.text, k.cursor, None);
        }
        if let Some(n) = k.warning {
            // FUN_005344c0: a strip of panel, the warning, and "click to continue".
            // A warning too wide for the strip goes on two lines, the strip a block
            // taller.
            let (x, y) = origin(r.screen);
            let t = self.text.get(19, n).unwrap_or("").trim().to_owned();
            let lines: Vec<String> = if text_width(r, Font::SmallPlain, &t) as f32 <= 554.0 {
                vec![t]
            } else {
                let words: Vec<&str> = t.split(' ').collect();
                let half = words.len().div_ceil(2);
                vec![words[..half].join(" "), words[half..].join(" ")]
            };
            let extra = 16.0 * (lines.len() as f32 - 1.0);
            panel::outer_panel(r, panels, x + 16.0, y + 32.0, 35, 4 + lines.len() as i32 - 1);
            for (i, l) in lines.iter().enumerate() {
                centred_in(r, Font::SmallPlain, l, x + 19.0, y + 48.0 + 16.0 * i as f32, 554.0, None);
            }
            let foot = self.text.get(13, 1).unwrap_or("").trim().to_owned();
            centred_in(r, Font::NormalBlackOnLight, &foot, x + 16.0, y + 68.0 + extra, 560.0, None);
        }
    }

    fn draw_objects(&self, r: &mut Renderer, k: &Kingdom, art: &Art) {
        let screen = r.screen;
        let t = |g: usize, i: usize| self.text.get(g, i).unwrap_or("").trim().to_owned();
        for o in self.objects() {
            let at = k.to_screen(screen, o.x, o.y);
            match o.kind {
                object::REGION => {
                    // Its name centred in 90 pixels, a black copy under a brown one.
                    let s = t(196, o.city_name_id as usize);
                    let tw = text_width(r, Font::SmallPlain, &s) as f32;
                    let x = at[0] + ((90.0 - tw) / 2.0).max(0.0).floor();
                    draw_text(r, Font::SmallPlain, &s, x, at[1], font::BLACK);
                    draw_text(r, Font::SmallPlain, &s, x - 1.0, at[1] - 1.0, REGION);
                    continue;
                }
                object::ORNAMENT | object::CITY | object::BATTLE_ICON => {}
                _ => continue,
            }
            if o.kind == object::BATTLE_ICON {
                // Its path and order beside it: white and red over black.
                let (path, order) = (o.invasion_path.to_string(), o.invasion_years.to_string());
                draw_text(r, Font::SmallPlain, &path, at[0] - 8.0, at[1] - 10.0, font::BLACK);
                draw_text(r, Font::SmallPlain, &path, at[0] - 9.0, at[1] - 9.0, WHITE);
                draw_text(r, Font::SmallPlain, &order, at[0] + 16.0, at[1] - 10.0, font::BLACK);
                draw_text(r, Font::SmallPlain, &order, at[0] + 15.0, at[1] - 9.0, rgb(RED));
            }
            let image = if o.kind == object::CITY { art.empire.city_image(o.city_type, o.city_name_id, false) } else { o.image_id as u32 };
            if image == 0 {
                continue;
            }
            r.image(image, at, WHITE, Space::Screen);
            if o.kind != object::CITY {
                continue;
            }
            let (iw, ih) = r.record(image).map_or((0.0, 0.0), |rec| (rec.width as f32, rec.height as f32));
            let name = t(195, o.city_name_id as usize);
            let tw = text_width(r, Font::SmallPlain, &name) as f32;
            let (x, y) = match o.text_align {
                0 => (at[0] - tw, at[1] + (ih / 2.0).floor()),
                1 => (at[0] + ((iw - tw) / 2.0).floor(), at[1] - 10.0),
                2 => (at[0] + iw, at[1] + (ih / 2.0).floor()),
                _ => (at[0] + ((iw - tw) / 2.0).floor(), at[1] + ih + 5.0),
            };
            draw_text(r, Font::SmallPlain, &name, x, y, NAME);
            if city::trades(o.city_type) {
                let flag = art.empire.flag;
                let n = r.record(flag).map_or(5, |rec| rec.num_animation_sprites.max(1) as u32);
                let frame = (k.clock * 8.0) as u32 % n + 1;
                let fh = r.record(flag + frame).map_or(0.0, |rec| rec.height as f32);
                r.image(flag + frame, [at[0] + (iw / 2.0).floor(), at[1] + ih - fh], WHITE, Space::Screen);
            }
        }
    }

    /// Every route in use as its dots; the route being worked on also with its
    /// waypoints, and an army (a ship by sea) walking it.
    fn draw_routes(&self, r: &mut Renderer, k: &Kingdom, art: &Art) {
        let screen = r.screen;
        let dot = art.empire.route_dot;
        let route_mode = matches!(k.mode, Mode::AddRoute | Mode::EditRoute);
        for (i, route) in self.scenario.empire.routes.iter().enumerate() {
            let current = route_mode && i == k.route;
            if !(route.in_use && route.points.len() >= 2) && !current {
                continue;
            }
            let dots = route.dots();
            for &(x, y) in &dots {
                r.image(dot, k.to_screen(screen, x, y), WHITE, Space::Screen);
            }
            if !current {
                continue;
            }
            for &(x, y) in &route.points {
                let at = k.to_screen(screen, x, y);
                let (ww, wh) = r.record(dot + 1).map_or((0.0, 0.0), |rec| (rec.width as f32, rec.height as f32));
                r.image(dot + 1, [at[0] - (ww / 2.0).floor() + 2.0, at[1] - (wh / 2.0).floor() + 2.0], WHITE, Space::Screen);
            }
            if !dots.is_empty() {
                let t = (k.clock * 33.0) as usize % 101;
                let (x, y) = dots[(dots.len() * t / 100).min(dots.len() - 1)];
                let marker = if route.route_type == 2 { art.sea_marker } else { art.land_marker };
                r.image(marker, k.to_screen(screen, x, y), WHITE, Space::Screen);
            }
        }
    }

    fn draw_kingdom_panel(&self, r: &mut Renderer, k: &Kingdom, art: &Art, w: f32, h: f32) {
        let t = |g: usize, i: usize| self.text.get(g, i).unwrap_or("").trim().to_owned();
        fill(r, [10.0, h - 116.0, w - 20.0, 115.0], BEIGE);
        raised(r, [10.0, h - 116.0, w - 20.0, 115.0]);
        draw_text(r, Font::LargeBlackOnLight, &t(44, 222), 15.0, h - 116.0, font::BLACK);
        for (b, label, rect) in main_buttons(h) {
            raised(r, rect);
            let chosen = matches!(b, Main::Mode(m) if m == k.mode);
            centred_in(r, Font::NormalBlackOnLight, &t(44, label), rect[0], rect[1] + 2.0, 100.0, chosen.then_some(RED));
        }
        fill(r, [223.0, h - 114.0, w - 238.0, 109.0], BEIGE);
        raised(r, [223.0, h - 114.0, w - 238.0, 109.0]);
        let b = boxes(h);
        match k.mode {
            Mode::Add => {
                if k.add_kind == 0 {
                    self.draw_ornament_box(r, art, &b, k.ornament, h);
                }
                fill(r, [387.0, h - 110.0, w - 408.0, 98.0], GREY);
                for (i, rect) in b.kinds.iter().enumerate() {
                    raised(r, *rect);
                    centred_in(r, Font::NormalBlackOnLight, &t(44, 8 + i), rect[0], rect[1] + 4.0, 180.0, (k.add_kind as usize == i).then_some(RED));
                }
            }
            Mode::Edit => {
                let Some(o) = self.selected_object() else { return };
                if o.kind == object::ORNAMENT {
                    self.draw_ornament_box(r, art, &b, o.expanded_image_id as u32, h);
                }
                fill(r, [387.0, h - 110.0, w - 408.0, 98.0], BEIGE);
                match o.kind {
                    object::CITY => self.draw_city_panel(r, art, o, h),
                    object::BATTLE_ICON => {
                        draw_text(r, Font::LargeBlackOnLight, &t(44, 69), 393.0, h - 100.0, font::BLACK);
                        for (label, rect, v) in [(70, b.path, o.invasion_path), (71, b.order, o.invasion_years)] {
                            draw_text(r, Font::NormalBlackOnLight, &t(44, label), 423.0, rect[1] + 4.0, font::BLACK);
                            raised(r, rect);
                            centred_in(r, Font::NormalBlackOnLight, &v.to_string(), rect[0], rect[1] + 4.0, rect[2], None);
                        }
                    }
                    object::REGION => {
                        raised(r, b.region);
                        centred_in(r, Font::SmallPlain, &t(196, o.city_name_id as usize), b.region[0], b.region[1] + 4.0, 180.0, None);
                    }
                    _ => {}
                }
            }
            Mode::General => {
                raised(r, b.prices);
                centred_in(r, Font::NormalBlackOnLight, &t(44, 204), b.prices[0], b.prices[1] + 4.0, b.prices[2], None);
            }
            Mode::AddRoute | Mode::EditRoute => {
                let e = &self.scenario.empire;
                let route = e.routes.get(k.route);
                raised(r, b.route_id);
                let id_colour = (k.keypad.as_ref().is_some_and(|p| p.num == Num::RouteId)).then_some(RED);
                let label = format!("{} {}", t(44, 32), k.route);
                label_in(r, Font::NormalBlackOnLight, &label, 413.0, b.route_id[1] + 5.0, id_colour);
                raised(r, b.route_kind);
                let kind = match route.map_or(1, |r| r.route_type) {
                    1 => 171,
                    2 => 170,
                    _ => 169,
                };
                label_in(r, Font::NormalBlackOnLight, &t(44, kind), 413.0, b.route_kind[1] + 5.0, None);
                raised(r, b.route_delete);
                label_in(r, Font::NormalBlackOnLight, &t(44, 168), 413.0, b.route_delete[1] + 5.0, None);
                if let Some(route) = route {
                    for (end, y) in [(route.from_object, h - 108.0), (route.to_object, h - 88.0)] {
                        if let Some(o) = usize::try_from(end).ok().and_then(|i| self.objects().get(i)).filter(|o| o.kind == object::CITY) {
                            centred_in(r, Font::NormalBlackOnLight, &t(195, o.city_name_id as usize), 593.0, y, 180.0, None);
                        }
                    }
                    fill(r, [593.0, h - 68.0, 180.0, 20.0], BEIGE);
                    let len = if route.points.len() >= 2 { route.length() } else { 0 };
                    label_in(r, Font::NormalBlackOnLight, &format!("{} {len}", t(44, 173)), 593.0, h - 64.0, None);
                }
            }
        }
    }

    /// A picture's box: white, the picture in it, its number, and the arrows.
    fn draw_ornament_box(&self, r: &mut Renderer, art: &Art, b: &Boxes, ornament: u32, h: f32) {
        fill(r, [293.0, h - 104.0, 90.0, 90.0], 0xffff);
        r.set_clip(Some([294.0, h - 103.0, 88.0, 88.0]));
        r.image(art.ornaments + ornament, [294.0, h - 103.0], WHITE, Space::Screen);
        r.set_clip(None);
        fill(r, [225.0, h - 30.0, 48.0, 20.0], BEIGE);
        centred_in(r, Font::NormalBlackOnLight, &(ornament + 1).to_string(), 225.0, h - 28.0, 48.0, None);
        if let Ok(img) = UiImages::load(&r.library)
            && let Ok(plain) = r.library.group_id("Pharaoh_Unloaded", 0, 25)
        {
            for (i, a) in b.arrows.iter().enumerate() {
                let id = match i {
                    0 => img.arrow_up,
                    1 => img.arrow_down,
                    _ => plain,
                };
                r.image(id, [a[0], a[1]], WHITE, Space::Screen);
            }
        }
    }

    fn draw_city_panel(&self, r: &mut Renderer, art: &Art, o: &EmpireObject, h: f32) {
        let t = |g: usize, i: usize| self.text.get(g, i).unwrap_or("").trim().to_owned();
        let b = boxes(h);
        let n = Font::NormalBlackOnLight;
        raised(r, b.city_type);
        centred_in(r, n, &t(39, o.city_type as usize), b.city_type[0], h - 108.0, 180.0, None);
        raised(r, b.name);
        centred_in(r, n, &t(195, o.city_name_id as usize), b.name[0], h - 108.0, 180.0, None);
        raised(r, b.align);
        centred_in(r, n, &t(86, o.text_align as usize), b.align[0], h - 108.0, 100.0, None);
        draw_text(r, n, &t(44, 32), 393.0, h - 83.0, font::BLACK);
        raised(r, b.route);
        centred_in(r, n, &o.trade_route_id.to_string(), b.route[0], h - 83.0, 50.0, None);
        if o.city_type != city::OURS {
            raised(r, b.demand);
            centred_in(r, n, &t(44, 33), b.demand[0], h - 83.0, 100.0, None);
        }
        draw_text(r, n, &t(44, 34), 653.0, h - 83.0, font::BLACK);
        raised(r, b.cost);
        centred_in(r, n, &o.trade_route_cost.to_string(), b.cost[0], h - 83.0, 60.0, None);
        let icon = |r: &mut Renderer, rect: [f32; 4], res: u8| {
            sunken(r, [rect[0] - 1.0, rect[1] - 1.0, 26.0, 26.0]);
            if res != 0 {
                r.image(art.empire.icon(res as u16), [rect[0], rect[1]], WHITE, Space::Screen);
            }
        };
        if o.city_type == city::OURS {
            for i in 0..MAX_SELLS {
                icon(r, ours_slot(h, i), o.sells.get(i).copied().unwrap_or(0));
            }
        } else {
            draw_text(r, n, &t(44, 36), 393.0, h - 55.0, font::BLACK);
            draw_text(r, n, &t(44, 5), 393.0, h - 27.0, font::BLACK);
            for i in 0..8 {
                icon(r, trade_slot(h, i, false), o.sells.get(i).copied().unwrap_or(0));
                icon(r, trade_slot(h, i, true), o.buys.get(i).copied().unwrap_or(0));
            }
        }
    }

    /// The demand window (FUN_00534320): each good the city sells or buys, with its
    /// demand under it (none, Low, Med, High).
    fn draw_demand(&self, r: &mut Renderer, panels: &PanelImages, k: &Kingdom, art: &Art) {
        let screen = r.screen;
        let (x, y) = origin(screen);
        let t = |g: usize, i: usize| self.text.get(g, i).unwrap_or("").trim().to_owned();
        panel::outer_panel(r, panels, x + 16.0, y + 32.0, 30, 14);
        centred_in(r, Font::LargeBlackOnLight, &t(44, 83), x + 16.0, y + 48.0, 480.0, None);
        let Some(o) = self.selected_object() else { return };
        for (i, res) in self.demand_goods().into_iter().enumerate() {
            let rect = demand_button(screen, i);
            r.image(art.empire.icon(res as u16), [rect[0] + 15.0, rect[1] - 30.0], WHITE, Space::Screen);
            panel::button_border(r, panels, rect[0], rect[1], 50, 22, inside(rect, k.cursor));
            let tier = o.demand.get(res).copied().unwrap_or(0) as usize;
            if tier != 0 {
                centred_in(r, Font::NormalBlackOnLight, &t(44, 84 + tier.min(3)), rect[0], rect[1] + 6.0, 50.0, None);
            }
        }
        centred_in(r, Font::NormalBlackOnLight, &t(13, 1), x + 16.0, y + 228.0, 480.0, None);
    }

    /// The prices window (FUN_00537cf0): each good's icon over its buying and selling
    /// price, "N/A" for goods no city trades; Reset prices.
    fn draw_prices(&self, r: &mut Renderer, panels: &PanelImages, k: &Kingdom, art: &Art) {
        let screen = r.screen;
        let (x, y) = origin(screen);
        let t = |g: usize, i: usize| self.text.get(g, i).unwrap_or("").trim().to_owned();
        panel::outer_panel(r, panels, x - 19.0, y, 43, 16);
        draw_text(r, Font::LargeBlackOnLight, &t(44, 204), x - 9.0, y + 9.0, font::BLACK);
        for row in [0.0, 75.0] {
            draw_text(r, Font::NormalBlackOnLight, &t(44, 205), x - 9.0, y + 84.0 + row, font::BLACK);
            draw_text(r, Font::NormalBlackOnLight, &t(44, 206), x - 9.0, y + 109.0 + row, font::BLACK);
        }
        let prices = self.prices();
        for &res in &PRICE_ORDER {
            let (row, col) = price_slot(res);
            let colx = x - 19.0 + 35.0 * col as f32;
            r.image(art.empire.icon(res as u16), [colx + 40.0, y + 50.0 + 78.0 * row as f32], WHITE, Space::Screen);
            let ok = self.available(res);
            for sell in [false, true] {
                let rect = price_button(screen, res, sell);
                panel::button_border(r, panels, rect[0], rect[1], 35, 20, inside(rect, k.cursor));
                let s = if ok {
                    let (b, s) = prices.get(res).copied().unwrap_or((0, 0));
                    (if sell { s } else { b }).to_string()
                } else {
                    t(18, 6)
                };
                centred_in(r, Font::SmallPlain, &s, rect[0], rect[1] + 5.0, 30.0, None);
            }
        }
        let reset = reset_prices_button(screen);
        panel::button_border(r, panels, reset[0], reset[1], 120, 35, inside(reset, k.cursor));
        draw_text(r, Font::NormalBlackOnLight, &t(44, 211), reset[0] + 5.0, reset[1] + 8.0, font::BLACK);
        centred_in(r, Font::NormalBlackOnLight, &t(13, 1), x - 19.0, y + 209.0, 608.0, None);
    }

    fn draw_list(&self, r: &mut Renderer, panels: &PanelImages, k: &Kingdom, list: &List) {
        let (frame, rects) = list_layout(r.screen, list.ids.len());
        panel::outer_panel(r, panels, frame[0], frame[1], (frame[2] / 16.0) as i32, (frame[3] / 16.0).ceil() as i32);
        for (rect, &id) in rects.iter().zip(&list.ids) {
            let s = self.text.get(list.group, id).unwrap_or("").trim().to_owned();
            let f = if inside(*rect, k.cursor) { Font::NormalYellow } else { Font::NormalBlackOnLight };
            let tw = osiris_ui::centring_width(r, f, &s) as f32;
            let colour = if f == Font::NormalYellow { WHITE } else { font::BLACK };
            draw_text(r, f, &s, rect[0] + ((rect[2] - tw) / 2.0).max(0.0).floor(), rect[1] + 3.0, colour);
        }
    }

    fn draw_kingdom_keypad(&self, r: &mut Renderer, panels: &PanelImages, k: &Kingdom, pad: &Keypad) {
        let ((x, y), keys, accept, cancel) = keypad_layout(r.screen);
        panel::outer_panel(r, panels, x, y, 13, 16);
        panel::inner_panel(r, panels, x + 24.0, y + 16.0, 10, 2);
        let tw = text_width(r, Font::NormalWhiteOnDark, &pad.typed) as f32;
        draw_text(r, Font::NormalWhiteOnDark, &pad.typed, x + 176.0 - tw, y + 26.0, WHITE);
        for (c, rect) in keys {
            panel::button_border(r, panels, rect[0], rect[1], rect[2] as i32, rect[3] as i32, inside(rect, k.cursor));
            centred_in(r, Font::LargeBlackOnLight, &c.to_string(), rect[0], rect[1] + 4.0, rect[2], None);
        }
        let t = |i: usize| self.text.get(44, i).unwrap_or("").trim().to_owned();
        for (rect, label) in [(accept, t(16)), (cancel, t(17))] {
            panel::button_border(r, panels, rect[0], rect[1], rect[2] as i32, rect[3] as i32, inside(rect, k.cursor));
            centred_in(r, Font::NormalBlackOnLight, &label, rect[0], rect[1] + 6.0, rect[2], None);
        }
    }
}

fn fill(r: &mut Renderer, rect: [f32; 4], c: u16) {
    r.rect([rect[0], rect[1]], [rect[2], rect[3]], rgb(c), Space::Screen);
}

/// A raised frame (FUN_004cd930): white top and left, grey bottom and right.
fn raised(r: &mut Renderer, rect: [f32; 4]) {
    edges(r, rect, rgb(0xffff), rgb(0x8410));
}

/// A sunken frame (FUN_004cd890): grey top and left, white bottom and right.
fn sunken(r: &mut Renderer, rect: [f32; 4]) {
    edges(r, rect, rgb(0x8410), rgb(0xffff));
}

fn edges(r: &mut Renderer, [x, y, w, h]: [f32; 4], light: [f32; 4], dark: [f32; 4]) {
    r.rect([x, y], [w, 1.0], light, Space::Screen);
    r.rect([x, y], [1.0, h], light, Space::Screen);
    r.rect([x + w - 1.0, y], [1.0, h], dark, Space::Screen);
    r.rect([x, y + h - 1.0], [w, 1.0], dark, Space::Screen);
}

/// The selection's marching dotted frame (FUN_004cd710).
fn dotted_rect(r: &mut Renderer, [x, y]: [f32; 2], [w, h]: [f32; 2], clock: f32) {
    let phase = (clock * 8.0) as i32;
    let c = |i: i32| if (i + phase).rem_euclid(4) < 2 { [1.0, 1.0, 1.0, 1.0] } else { [0.0, 0.0, 0.0, 1.0] };
    for i in 0..w as i32 {
        r.rect([x + i as f32, y], [1.0, 1.0], c(i), Space::Screen);
        r.rect([x + i as f32, y + h - 1.0], [1.0, 1.0], c(i + 1), Space::Screen);
    }
    for i in 0..h as i32 {
        r.rect([x, y + i as f32], [1.0, 1.0], c(i), Space::Screen);
        r.rect([x + w - 1.0, y + i as f32], [1.0, 1.0], c(i + 1), Space::Screen);
    }
}

/// Text at `(x, y)`, black or in the colour given.
fn label_in(r: &mut Renderer, f: Font, s: &str, x: f32, y: f32, colour: Option<u16>) {
    match colour {
        Some(c) => draw_text_in(r, f, s, x, y, rgb(c)),
        None => draw_text(r, f, s, x, y, font::BLACK),
    };
}

/// Text centred in `w` pixels from `x` (flush left when wider), black or coloured.
fn centred_in(r: &mut Renderer, f: Font, s: &str, x: f32, y: f32, w: f32, colour: Option<u16>) {
    let tw = osiris_ui::centring_width(r, f, s) as f32;
    label_in(r, f, s, x + ((w - tw) / 2.0).max(0.0).floor(), y, colour);
}

/// The empire window's frame (FUN_0052d630): bars along the top, above the panel
/// (130 up) and along the bottom, down both sides, crosses where they meet, and stone
/// between the lower bars.
fn draw_frame(r: &mut Renderer, images: &EmpireImages) {
    let [w, h] = r.screen;
    let (vert, horiz, cross, stone) = (images.bars, images.bars + 1, images.bars + 2, images.bars + 3);
    let img = |r: &mut Renderer, id: u32, x: f32, y: f32| r.image(id, [x, y], WHITE, Space::Screen);
    for y in [h - 120.0, h - 80.0, h - 40.0] {
        let mut x = 0.0;
        while x + 70.0 <= w {
            img(r, stone, x, y);
            x += 70.0;
        }
        img(r, stone, w - 70.0, y);
    }
    for y in [0.0, h - DIVIDER, h - BAR] {
        let mut x = 0.0;
        while x + 86.0 <= w {
            img(r, horiz, x, y);
            x += 86.0;
        }
        img(r, horiz, w - 86.0, y);
    }
    for x in [0.0, w - BAR] {
        let mut y = BAR;
        while y + 86.0 <= h {
            img(r, vert, x, y);
            y += 86.0;
        }
        img(r, vert, x, h - 86.0);
        for y in [0.0, h - DIVIDER, h - BAR] {
            img(r, cross, x, y);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use osiris_formats::{ChunkFile, Layout, Scenario, TextTable};
    use std::path::PathBuf;
    use std::sync::Arc;

    const SCREEN: [f32; 2] = [1024.0, 768.0];

    fn data() -> Option<PathBuf> {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../PharaohData");
        dir.is_dir().then_some(dir)
    }

    /// The Kingdom map worked as a designer would, by clicks on its screen: a new
    /// city made ours with two goods, a trade city's goods, demand, route and cost, a
    /// route drawn and made a sea route, and a price. All of it comes back from the
    /// saved map.
    #[test]
    fn kingdom_edits_save() {
        let Some(data) = data() else { return };
        let lib = ImageLibrary::open(&data.join("Data")).unwrap();
        let defs = Arc::new(osiris_sim::Defs::load(&lib).unwrap());
        let text = Arc::new(TextTable::parse(&std::fs::read(data.join("Pharaoh_Text.eng")).unwrap()).unwrap());
        let mut e = Editor::open(&data.join("Maps/Default.map"), defs, text, std::env::temp_dir().join("osiris-kingdom-test")).unwrap();
        e.open_kingdom();
        let run = |e: &mut Editor, a: Scripted| e.run_scripted(&lib, SCREEN, a);
        run(&mut e, Scripted::Scroll(187, 340));
        // Add object > City, placed at (700, 700): it is selected, as an Egyptian trade city.
        run(&mut e, Scripted::Button(0));
        run(&mut e, Scripted::Click([480.0, 698.0]));
        run(&mut e, Scripted::MapClick(700, 700));
        let new = e.view.kingdom.as_ref().unwrap().selected.unwrap();
        assert_eq!(e.scenario.empire.objects[new].city_type, city::EGYPTIAN_TRADING);
        // Its kind from the list: Our city (the first of seven).
        run(&mut e, Scripted::Click([480.0, 675.0]));
        run(&mut e, Scripted::Click([500.0, 328.0]));
        // Grain and chickpeas in its first two boxes (a list of 21: Nothing, Grain,
        // Meat, Lettuce, Chickpeas ...).
        run(&mut e, Scripted::Click([410.0, 715.0]));
        run(&mut e, Scripted::Click([500.0, 215.0]));
        run(&mut e, Scripted::Click([437.0, 715.0]));
        run(&mut e, Scripted::Click([500.0, 269.0]));
        let o = &e.scenario.empire.objects[new];
        assert_eq!((o.city_type, o.sells.clone()), (city::OURS, vec![1, 4]));

        // Men-nefer: wood sold at high demand, route 2, cost 1234.
        run(&mut e, Scripted::MapRight(560, 500));
        let men = e.view.kingdom.as_ref().unwrap().selected.unwrap();
        assert_eq!(e.scenario.empire.objects[men].city_name_id, 29);
        // Its third box sold papyrus: now wood.
        run(&mut e, Scripted::Click([590.0, 715.0]));
        let ids = e.view.kingdom.as_ref().unwrap().list.as_ref().unwrap().ids.clone();
        assert!(!ids.contains(&4) && !ids.contains(&13) && ids.contains(&23), "the other boxes' goods are left out");
        let (_, rects) = list_layout(SCREEN, ids.len());
        let at = rects[ids.iter().position(|&r| r == 20).unwrap()];
        run(&mut e, Scripted::Click([at[0] + 5.0, at[1] + 5.0]));
        assert!(e.scenario.empire.objects[men].sells.contains(&20), "{:?}", e.scenario.empire.objects[men].sells);
        run(&mut e, Scripted::Click([480.0, 690.0]));
        run(&mut e, Scripted::Type("2\n".into()));
        run(&mut e, Scripted::Click([740.0, 690.0]));
        run(&mut e, Scripted::Type("1234\n".into()));
        run(&mut e, Scripted::Click([570.0, 690.0]));
        let wood = e.demand_goods().iter().position(|&r| r == 20).unwrap();
        let b = demand_button(SCREEN, wood);
        run(&mut e, Scripted::Click([b[0] + 5.0, b[1] + 5.0]));
        run(&mut e, Scripted::Right([500.0, 500.0]));

        // Route 2 drawn from Men-nefer southwards, then made a sea route.
        run(&mut e, Scripted::Button(167));
        run(&mut e, Scripted::Click([480.0, 675.0]));
        run(&mut e, Scripted::Type("2\n".into()));
        assert_eq!(e.view.kingdom.as_ref().unwrap().mode, Mode::AddRoute);
        for (x, y) in [(560, 500), (600, 600), (590, 700)] {
            run(&mut e, Scripted::MapClick(x, y));
        }
        run(&mut e, Scripted::MapRight(590, 700));
        run(&mut e, Scripted::Click([480.0, 700.0]));

        // A price: General > Set Kingdom prices, grain's buying price.
        run(&mut e, Scripted::Button(3));
        run(&mut e, Scripted::Click([300.0, 744.0]));
        let p = price_button(SCREEN, 1, false);
        run(&mut e, Scripted::Click([p[0] + 5.0, p[1] + 5.0]));
        run(&mut e, Scripted::Type("77\n".into()));
        run(&mut e, Scripted::Right([500.0, 500.0]));
        run(&mut e, Scripted::Button(7));
        assert!(!e.kingdom_open(), "OK leaves the Kingdom map");

        let s = Scenario::from_chunks(&ChunkFile::parse(&e.to_bytes().unwrap(), Layout::Map).unwrap()).unwrap();
        let o = &s.empire.objects[new];
        assert_eq!((o.kind, o.city_type, o.sells.clone()), (object::CITY, city::OURS, vec![1, 4]));
        let m = &s.empire.objects[men];
        assert_eq!((m.trade_route_id, m.trade_route_cost, m.demand[20]), (2, 1234, 3));
        assert!(m.sells.contains(&20));
        let r = &s.empire.routes[2];
        assert_eq!((r.in_use, r.route_type, r.points.clone()), (true, 2, vec![(560, 500), (600, 600), (590, 700)]));
        assert_eq!(r.from_object, men as i16);
        assert_eq!(s.empire.prices[1].0, 77);

        // Ctrl+Z steps back through the Kingdom edits: the price first, then (all
        // the way back) the city added.
        e.undo();
        assert_ne!(e.scenario.empire.prices[1].0, 77);
        while !e.history.is_empty() {
            e.undo();
        }
        assert!(!e.scenario.empire.objects[new].in_use || e.scenario.empire.objects[new].city_type != city::OURS);
    }

    /// What the city can get decides what the scenario keeps (FUN_00442870,
    /// FUN_00442a40): with no grain grown and no straw sold anywhere, our city can't
    /// raise meat; with no limestone anywhere a pyramid can't be built; with no copper
    /// the weaponsmith can't work. Ctrl+Z puts it all back.
    #[test]
    fn goods_decide_monuments_and_buildings() {
        let Some(data) = data() else { return };
        let lib = ImageLibrary::open(&data.join("Data")).unwrap();
        let defs = Arc::new(osiris_sim::Defs::load(&lib).unwrap());
        let text = Arc::new(TextTable::parse(&std::fs::read(data.join("Pharaoh_Text.eng")).unwrap()).unwrap());
        let mut e = Editor::open(&data.join("Maps/Warfare.map"), defs, text, std::env::temp_dir().join("osiris-kingdom-test")).unwrap();
        e.open_kingdom();
        let ours = e.scenario.empire.objects.iter().position(|o| o.in_use && o.kind == object::CITY && o.city_type == city::OURS).unwrap();
        let before = e.scenario.empire.clone();
        let monuments = e.scenario.info.monuments;
        e.kingdom_mark();
        for o in &mut e.scenario.empire.objects {
            o.sells.retain(|&r| !matches!(r, 1 | 9 | 25 | 29 | 20 | 24 | 12));
        }
        e.scenario.empire.objects[ours].sells = vec![2, 11];
        e.scenario.info.monuments = [1, 0, 0];
        e.scenario.info.reserved.resize(114, 0);
        e.scenario.info.reserved[41] = 1;
        assert!(!obtainable(&e.scenario, 9) && !obtainable(&e.scenario, 25) && !obtainable(&e.scenario, 29));
        assert!(obtainable(&e.scenario, 13), "pottery from the clay our city digs");
        e.check_city(ours);
        e.kingdom_commit();
        assert_eq!(e.scenario.empire.objects[ours].sells, vec![11]);
        assert_eq!(e.view.kingdom.as_ref().unwrap().warning, Some(69), "meat's warning comes first");
        assert_eq!(e.scenario.info.monuments[0], 0);
        // One warning at a time: the buildings are checked on the next edit.
        assert_eq!(e.scenario.info.reserved[41], 1);
        e.kingdom_mark();
        e.check_city(ours);
        e.kingdom_commit();
        assert_eq!(e.scenario.info.reserved[41], 0, "no copper, no weaponsmith");
        assert_eq!(e.view.kingdom.as_ref().unwrap().warning, Some(180));
        e.undo();
        e.undo();
        assert!(e.scenario.empire == before);
        assert_eq!(e.scenario.info.monuments, monuments);
    }
}
