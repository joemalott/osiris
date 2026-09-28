//! The editor's screen: the map, the editor's control panel (Pharaoh_Unloaded group
//! 22) with its minimap, Kingdom and Options buttons, the black box that tells the
//! tool and what the map still lacks, and the twelve tool buttons (Pharaoh_General
//! group 137), their submenus, the menu bar, and the points' flags on the map.
//!
//! Layout from the original (FUN_0051e990, button table 0x5c7760): the panel is drawn
//! at the sidebar's left edge and y 24, below the 24-pixel menu bar; positions here
//! are from its top-left corner.

use crate::lang::{tr, trf};
use super::terrain::Paint;
use super::{Editor, Point, Request, Tool};
use crate::city_view::{self, CityView, Highlight, Overlay, Sprite};
use crate::minimap::Minimap;
use crate::popup::Confirm;
use crate::top_menu::{Entry, MenuAction, TopMenu};
use crate::widgets::UiImages;
use osiris_formats::scenario::TilePoint;
use osiris_render::{Paint as Tint, Renderer, Space, WHITE};

/// What a warning about missing points asks leave to do.
#[derive(Clone, Copy)]
enum Verb {
    Save,
    Play,
}
use osiris_sim::map::{Map, terrain as bits};
use osiris_ui::{Font, PanelImages, draw_text, font, panel, text_width};

/// The panel's top edge: under the editor's menu bar.
pub const TOP: f32 = 24.0;
/// The panel and the patterned strip beside it, from the screen's right edge.
pub const WIDTH: f32 = 186.0;
const PANEL_W: f32 = 162.0;
const STRIP_W: f32 = 24.0;
const RELIEF_Y: f32 = 480.0;

/// Left edge of the panel on a screen `w` wide.
pub fn panel_left(w: f32) -> f32 {
    w - WIDTH
}

/// The minimap's window in the panel, and the black box under it.
const MINIMAP: [f32; 2] = [8.0, 6.0];
const INFO_BOX: [f32; 4] = [8.0, 151.0, 145.0, 111.0];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Kingdom,
    Options,
    /// A tool button, by the number the original gives it (the tool names of text
    /// group 49).
    Tool(u8),
}

/// The panel's buttons: where, how big, and the first of their three images
/// (normal, under the mouse, pressed) in Pharaoh_General group 137. The killer type
/// button's images are the expansion's (group 2) instead.
const BUTTONS: [(Button, f32, f32, f32, f32, u32); 14] = [
    (Button::Kingdom, 7.0, 123.0, 71.0, 23.0, 45),
    (Button::Options, 84.0, 123.0, 71.0, 23.0, 45),
    (Button::Tool(0), 13.0, 267.0, 39.0, 26.0, 0),
    (Button::Tool(1), 63.0, 267.0, 39.0, 26.0, 3),
    (Button::Tool(2), 113.0, 267.0, 39.0, 26.0, 6),
    (Button::Tool(6), 13.0, 303.0, 39.0, 26.0, 18),
    (Button::Tool(10), 63.0, 303.0, 39.0, 26.0, 30),
    (Button::Tool(5), 113.0, 303.0, 39.0, 26.0, 15),
    (Button::Tool(17), 13.0, 339.0, 39.0, 26.0, 33),
    (Button::Tool(13), 63.0, 339.0, 39.0, 26.0, 39),
    (Button::Tool(14), 113.0, 339.0, 39.0, 26.0, 42),
    (Button::Tool(24), 13.0, 375.0, 39.0, 26.0, 27),
    (Button::Tool(8), 63.0, 375.0, 39.0, 26.0, 24),
    (Button::Tool(32), 113.0, 375.0, 39.0, 26.0, 0),
];

/// What an entry of a tool's submenu does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    Tool(Tool),
    Brush(u8),
    /// The climate's first or second beast.
    Killer(u16),
}

/// The entries of button `b`'s submenu with their labels (the menus at 0x5ce3bc),
/// or `None` for a button that picks its tool straight away.
fn submenu(e: &Editor, b: u8) -> Option<Vec<(String, Item)>> {
    let t = |g: usize, i: usize| e.text.get(g, i).unwrap_or("").trim().to_owned();
    let rock = |k| Item::Tool(Tool::Paint(Paint::Rock(k)));
    Some(match b {
        2 => vec![
            (t(48, 35), Item::Tool(Tool::Paint(Paint::Water))),
            (t(48, 36), Item::Tool(Tool::Paint(Paint::Floodplain))),
            (t(48, 38), Item::Tool(Tool::Paint(Paint::Marshland))),
        ],
        5 => vec![
            (t(28, 163), rock(bits::ROCK)),
            (t(28, 164), rock(bits::ROCK | bits::ORE)),
            (t(28, 198), Item::Tool(Tool::Paint(Paint::Dunes))),
            (t(28, 223), rock(bits::ROCK | bits::CLIFF)),
        ],
        17 => vec![(t(48, 18), Item::Tool(Tool::Point(Point::RiverIn))), (t(48, 19), Item::Tool(Tool::Point(Point::RiverOut)))],
        13 => (0..16u8).map(|i| (t(48, if i < 8 { 10 + i as usize } else { 40 + i as usize }), Item::Tool(Tool::Point(Point::Invasion(i))))).collect(),
        14 => vec![(t(48, 5), Item::Tool(Tool::Point(Point::Entry))), (t(48, 6), Item::Tool(Tool::Point(Point::Exit)))],
        24 => {
            let mut v: Vec<(String, Item)> = (0..8u8).map(|i| (t(48, 23 + i as usize), Item::Tool(Tool::Point(Point::Fishing(i))))).collect();
            v.extend((0..4u8).map(|i| (t(48, 31 + i as usize), Item::Tool(Tool::Point(Point::Predator(i))))));
            v.extend((0..4u8).map(|i| (t(48, 39 + i as usize), Item::Tool(Tool::Point(Point::Prey(i))))));
            v.extend((0..3u8).map(|i| (t(48, 56 + i as usize), Item::Tool(Tool::Point(Point::Disembark(i))))));
            v
        }
        8 => (0..5u8).map(|i| (t(48, i as usize), Item::Brush(i))).collect(),
        32 => {
            let climate = e.scenario.info.climate;
            let first = match climate {
                2 => 83,
                1 => 82,
                _ => 84,
            };
            vec![(t(64, first), Item::Killer(0)), (t(64, 102 + climate.min(2) as usize), Item::Killer(1))]
        }
        _ => return None,
    })
}

/// The tool a button picks straight away.
fn direct_tool(b: u8) -> Option<Tool> {
    Some(match b {
        0 => Tool::Paint(Paint::Grass),
        1 => Tool::Paint(Paint::Trees),
        6 => Tool::Paint(Paint::Meadow),
        10 => Tool::Road,
        _ => return None,
    })
}

/// Where the first of `n` submenu entries goes (the table at 0x5cf63c): the list is
/// centred about the lower buttons, 24 pixels an entry.
fn submenu_top(n: usize) -> f32 {
    const Y: [i32; 22] = [44, 322, 306, 274, 258, 226, 210, 178, 162, 130, 114, 82, 66, 34, 18, -14, -30, -46, -62, -78, -78, -94];
    (110 + Y[n.min(Y.len() - 1)]) as f32
}

/// A popup over the editor.
pub enum Popup {
    /// New Map's sizes (text group 33, the last entry Cancel).
    Sizes,
    /// Typing the name to save the map under.
    SaveName(String),
}

/// What answering "yes" to a yes/no warning goes on to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfirmThen {
    Play,
    SaveName,
    Exit,
    Open,
}

/// The images the editor draws, found once.
#[derive(Clone, Copy)]
struct Images {
    panel: u32,
    relief_small: u32,
    relief: u32,
    buttons: u32,
    killer: u32,
    top_bar: u32,
    strip: u32,
    flag: u32,
    flag_icons: u32,
}

impl Images {
    fn load(r: &Renderer) -> Option<Self> {
        let lib = &r.library;
        let g = |pack: &str, group, off| lib.group_id(pack, group, off).ok();
        Some(Self {
            panel: g("Pharaoh_Unloaded", 22, 0)?,
            relief_small: g("Pharaoh_Unloaded", 22, 1)?,
            relief: g("Pharaoh_Unloaded", 22, 2)?,
            buttons: g("Pharaoh_General", 137, 0)?,
            killer: g("Expansion", 2, 0)?,
            top_bar: g("Pharaoh_General", 121, 8)?,
            strip: g("Pharaoh_General", 121, 2)?,
            flag: g("Pharaoh_General", 139, 0)?,
            flag_icons: g("Pharaoh_General", 140, 0)?,
        })
    }
}

#[derive(Default)]
pub struct View {
    city: CityView,
    minimap: Option<Minimap>,
    map_dirty: bool,
    images: Option<Images>,
    pub cursor: [f32; 2],
    hover: Option<(i32, i32)>,
    /// A stroke in progress: the last tile painted.
    stroke: Option<(i32, i32)>,
    /// Where a road being dragged began.
    road_from: Option<(i32, i32)>,
    /// The open submenu: its button and entries.
    pub(super) menu: Option<(u8, Vec<(String, Item)>)>,
    pressed: Option<Button>,
    top: Option<TopMenu>,
    pub popup: Option<Popup>,
    /// A yes/no warning open (Play or Save with points missing, or unsaved changes
    /// about to be discarded), and what a yes goes on to do.
    confirm: Option<(Confirm, ConfirmThen)>,
    confirm_click: Option<[f32; 2]>,
    pub options: Option<super::options::Options>,
    /// The Kingdom map, while it is open (it takes the whole screen).
    pub kingdom: Option<super::kingdom::Kingdom>,
    /// A click on the Options screen, for its widgets to take as they are drawn.
    pub options_click: Option<[f32; 2]>,
    /// The camera as last drawn (its top-left corner, plus the game's 30-pixel bar
    /// the saved camera allows for).
    saved_camera: Option<[f32; 2]>,
    /// Alt+D: the view may scroll past the map's edges.
    pub free_scroll: bool,
    /// H: cliffs are drawn as plain rock.
    pub hide_cliffs: bool,
    pub clock: f32,
    status: Option<(String, f32)>,
    /// The camera was placed where the map's editor left it.
    placed: bool,
}

impl View {
    /// The map changed: the minimap and the view's bounds are worked out again.
    pub fn map_changed(&mut self) {
        self.map_dirty = true;
    }

    pub fn say(&mut self, s: &str) {
        self.status = Some((s.to_owned(), 3.0));
    }

    /// The camera as the map stores it (`city_view_camera`): the view's top-left
    /// corner in half tiles, the inverse of where the game starts its camera.
    pub fn camera_to_save(&self, map: &Map, s: &osiris_formats::Scenario) -> [i32; 2] {
        let Some([cx, cy]) = self.saved_camera else { return s.camera };
        let origin = city_view::tile_to_world(map, 0, 0);
        let a = ((cx - origin[0]) / (city_view::TILE_W / 2.0)).round() as i32;
        let b = ((cy - origin[1]) / (city_view::TILE_H / 2.0)).round() as i32;
        let g = osiris_formats::chunks::GRID_SIZE as i32;
        let (x0, y0) = (s.info.start_offset % g, s.info.start_offset / g);
        [(a + g + 2 + x0 - y0) / 2, b + 1 + x0 + y0]
    }
}

impl Editor {
    fn menu_bar(&self) -> TopMenu {
        let t = |g: usize, i: usize| self.text.get(g, i).unwrap_or("").trim().to_owned();
        let e = |label: String, code: u16| Entry { label, action: MenuAction::Editor(code) };
        let off = |label: String| Entry { label, action: MenuAction::Unavailable };
        TopMenu::from_headers(vec![
            (t(7, 0), vec![e(t(7, 1), 1), e(t(7, 2), 2), e(t(7, 3), 3), e(t(44, 215), 5), e(t(7, 4), 4)]),
            (t(2, 0), vec![off(t(2, 1)), off(t(2, 2)), off(t(2, 3))]),
            (t(3, 0), vec![off(t(3, 8)), off(t(3, 7))]),
            (t(10, 0), vec![e(tr("Undo").to_owned(), 21), e(t(10, 1), 11), e(t(10, 2), 12), e(t(10, 3), 13), e(t(10, 4), 14), e(t(10, 5), 15), off(t(10, 8)), e(t(10, 10), 20)]),
        ])
    }

    /// The question a yes/no warning asks before an action that would lose
    /// something: `verb` names the action ("Play", "Save") in "This map has no
    /// entry point. `verb` anyway?", listing whichever of entry, exit and river
    /// points the black info box is showing in red.
    fn warn_missing(&self, verb: Verb) -> Option<Confirm> {
        let missing: Vec<&str> = self.missing_points().into_iter().map(tr).collect();
        let list = match missing.as_slice() {
            [] => return None,
            [a] => a.to_string(),
            [a, b] => trf("{0} and {1}", &[a, b]),
            [a, b, c] => trf("{0}, {1} and {2}", &[a, b, c]),
            _ => unreachable!("only three kinds of point are checked"),
        };
        let question = match verb {
            Verb::Save => trf("This map has {0}. Save anyway?", &[&list]),
            Verb::Play => trf("This map has {0}. Play anyway?", &[&list]),
        };
        Some(Confirm { title: tr("Warning").to_owned(), question })
    }

    /// The question asked before discarding changes the title's "*" says aren't saved.
    fn warn_unsaved(&self) -> Option<Confirm> {
        self.dirty.then(|| Confirm { title: tr("Unsaved changes").to_owned(), question: tr("This map has changes that haven't been saved. Continue anyway?").to_owned() })
    }

    /// A choice from the menu bar.
    pub fn menu_command(&mut self, code: u16) {
        match code {
            1 => self.view.popup = Some(Popup::Sizes),
            2 => match self.warn_unsaved() {
                Some(c) => self.view.confirm = Some((c, ConfirmThen::Open)),
                None => self.request = Some(Request::Open),
            },
            3 => match self.warn_missing(Verb::Save) {
                Some(c) => self.view.confirm = Some((c, ConfirmThen::SaveName)),
                None => self.view.popup = Some(Popup::SaveName(self.name.clone())),
            },
            4 => match self.warn_unsaved() {
                Some(c) => self.view.confirm = Some((c, ConfirmThen::Exit)),
                None => self.request = Some(Request::Exit),
            },
            5 => match self.warn_missing(Verb::Play) {
                Some(c) => self.view.confirm = Some((c, ConfirmThen::Play)),
                None => self.run_confirm(ConfirmThen::Play),
            },
            11..=15 => self.clear_points(code as usize - 10),
            20 => {
                self.push_undo();
                self.refresh_map();
            }
            21 => self.undo(),
            _ => {}
        }
    }

    /// What answering "yes" to a warning goes on to do.
    fn run_confirm(&mut self, then: ConfirmThen) {
        match then {
            ConfirmThen::Play => {
                if let Err(e) = self.play() {
                    self.view.say(&trf("Could not play the map: {0}", &[&e]));
                }
            }
            ConfirmThen::SaveName => self.view.popup = Some(Popup::SaveName(self.name.clone())),
            ConfirmThen::Exit => self.request = Some(Request::Exit),
            ConfirmThen::Open => self.request = Some(Request::Open),
        }
    }

    /// Opens the submenu of tool button `b`, or picks its tool.
    pub fn press_button(&mut self, b: u8) {
        if let Some(items) = submenu(self, b) {
            self.view.menu = if self.view.menu.as_ref().is_some_and(|(open, _)| *open == b) { None } else { Some((b, items)) };
        } else if let Some(t) = direct_tool(b) {
            self.tool = t;
            self.view.menu = None;
        }
    }

    pub fn choose_item(&mut self, item: Item) {
        match item {
            Item::Tool(t) => self.tool = t,
            Item::Brush(b) => self.brush = b,
            Item::Killer(k) => {
                self.scenario.info.alt_predator_type = k;
                self.dirty = true;
            }
        }
        self.view.menu = None;
    }

    /// Opens a tool's submenu by button number, for scripted screenshots.
    pub fn open_submenu(&mut self, b: u8) {
        self.view.menu = submenu(self, b).map(|items| (b, items));
    }

    pub fn open_top_menu(&mut self, n: usize) {
        let mut bar = self.view.top.take().unwrap_or_else(|| self.menu_bar());
        bar.open = Some(n);
        self.view.top = Some(bar);
    }

    pub fn wants_text(&self) -> bool {
        matches!(self.view.popup, Some(Popup::SaveName(_))) || self.view.options.as_ref().is_some_and(|o| o.wants_text()) || self.kingdom_wants_text()
    }

    /// Typed text: backspace is `\u{8}`, Enter `\n`.
    pub fn type_text(&mut self, s: &str) {
        if self.kingdom_wants_text() {
            self.type_kingdom(s);
            return;
        }
        if let Some(o) = &mut self.view.options {
            o.type_text(&mut self.scenario, s);
            self.dirty = true;
            return;
        }
        let Some(Popup::SaveName(name)) = &mut self.view.popup else { return };
        for c in s.chars() {
            match c {
                '\u{8}' => {
                    name.pop();
                }
                '\n' | '\r' => {
                    let name = name.trim().to_owned();
                    self.view.popup = None;
                    if !name.is_empty() {
                        let path = self.path_for(&name);
                        match self.save_as(&path) {
                            Ok(()) => self.view.say(&trf("Saved {0}", &[&path.display()])),
                            Err(e) => self.view.say(&trf("Save failed: {0}", &[&e])),
                        }
                    }
                    return;
                }
                c if !c.is_control() && name.chars().count() < 32 => name.push(c),
                _ => {}
            }
        }
    }

    /// Keys the editor answers: H hides cliffs, Alt+Z refreshes the map, Ctrl+Z
    /// (Cmd+Z on macOS) undoes the last stroke, road, point or Refresh Map, Alt+D
    /// lets the view past the map's edges; Escape backs out of what is open.
    pub fn key(&mut self, key: &str, alt: bool, ctrl: bool) {
        match (key, alt, ctrl) {
            ("h", false, false) => self.view.hide_cliffs = !self.view.hide_cliffs,
            ("z", true, false) => {
                self.push_undo();
                self.refresh_map();
            }
            ("z", false, true) => self.undo(),
            ("d", true, false) => self.view.free_scroll = !self.view.free_scroll,
            ("escape", _, _) => self.cancel(),
            _ => {}
        }
    }

    /// A right-click: closes the popup, options or submenu open, else puts the tool
    /// down.
    pub fn cancel(&mut self) {
        if let Some(k) = &mut self.view.kingdom {
            // Escape closes what is open over the Kingdom map, else leaves it as OK does.
            if !k.close_popups() {
                self.leave_kingdom();
            }
            return;
        }
        if self.view.popup.is_some() {
            self.view.popup = None;
        } else if let Some(o) = &mut self.view.options {
            if o.back() {
                self.view.options = None;
            }
        } else if self.view.menu.is_some() {
            self.view.menu = None;
        } else if self.view.top.as_ref().is_some_and(|t| t.open.is_some()) {
            if let Some(t) = &mut self.view.top {
                t.open = None;
            }
        } else {
            self.tool = Tool::None;
        }
    }

    /// A right-click: the Kingdom map's own use of it, or else `cancel`.
    pub fn right_press(&mut self, screen: [f32; 2], p: [f32; 2]) {
        if self.view.kingdom.is_some() {
            self.kingdom_right(screen, p);
        } else {
            self.cancel();
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.view.clock += dt;
        if let Some(k) = &mut self.view.kingdom {
            k.tick(dt);
        }
        if let Some((_, t)) = &mut self.view.status {
            *t -= dt;
            if *t <= 0.0 {
                self.view.status = None;
            }
        }
    }

    fn button_at(&self, screen: [f32; 2], p: [f32; 2]) -> Option<Button> {
        let ox = panel_left(screen[0]);
        BUTTONS.iter().find(|b| p[0] >= ox + b.1 && p[0] < ox + b.1 + b.3 && p[1] >= TOP + b.2 && p[1] < TOP + b.2 + b.4).map(|b| b.0)
    }

    fn item_at(&self, screen: [f32; 2], p: [f32; 2]) -> Option<usize> {
        let (_, items) = self.view.menu.as_ref()?;
        let x = panel_left(screen[0]) - 170.0;
        let top = submenu_top(items.len());
        (0..items.len()).find(|&i| {
            let y = top + 24.0 * i as f32;
            p[0] >= x && p[0] < x + 160.0 && p[1] >= y && p[1] < y + 20.0
        })
    }

    fn over_panel(&self, screen: [f32; 2], p: [f32; 2]) -> bool {
        p[0] >= panel_left(screen[0]) || p[1] < TOP
    }

    pub fn set_cursor(&mut self, r: &Renderer, p: [f32; 2]) {
        self.view.cursor = p;
        if self.view.kingdom.is_some() {
            self.kingdom_move(r.screen, p);
            return;
        }
        if let Some(t) = &mut self.view.top {
            t.hover(p);
        }
        if self.view.options.is_some() || self.view.popup.is_some() || self.over_panel(r.screen, p) || self.item_at(r.screen, p).is_some() {
            self.view.hover = None;
            return;
        }
        self.view.hover = city_view::world_to_tile(&self.map, r.screen_to_world(p));
        if let (Some(last), Some(tile)) = (self.view.stroke, self.view.hover)
            && last != tile
        {
            self.view.stroke = Some(tile);
            self.apply(tile.0, tile.1);
        }
    }

    /// A left click. Returns a tile the view should centre on (a click on the
    /// minimap).
    pub fn press(&mut self, r: &Renderer, p: [f32; 2]) -> Option<(i32, i32)> {
        let screen = r.screen;
        if self.view.confirm.is_some() {
            self.view.confirm_click = Some(p);
            return None;
        }
        if self.view.kingdom.is_some() {
            self.kingdom_press(&r.library, screen, p);
            return None;
        }
        if let Some(popup) = &self.view.popup {
            self.click_popup(screen, p, matches!(popup, Popup::Sizes));
            return None;
        }
        if self.view.options.is_some() {
            self.view.options_click = Some(p);
            return None;
        }
        if self.view.top.is_none() {
            self.view.top = Some(self.menu_bar());
        }
        if let Some(bar) = &mut self.view.top {
            let (used, action) = bar.click(p);
            if let Some(MenuAction::Editor(code)) = action {
                self.menu_command(code);
            }
            if used {
                return None;
            }
        }
        if let Some(i) = self.item_at(screen, p) {
            if let Some(item) = self.view.menu.as_ref().and_then(|(_, items)| items.get(i)).map(|(_, it)| *it) {
                self.choose_item(item);
            }
            return None;
        }
        if let Some(b) = self.button_at(screen, p) {
            self.view.pressed = Some(b);
            match b {
                Button::Kingdom => self.open_kingdom(),
                Button::Options => {
                    self.view.menu = None;
                    self.view.options = Some(Default::default());
                }
                Button::Tool(t) => self.press_button(t),
            }
            return None;
        }
        let ox = panel_left(screen[0]);
        let origin = [ox + MINIMAP[0], TOP + MINIMAP[1]];
        if let Some(m) = &self.view.minimap
            && let Some(tile) = m.tile_at(&self.map, origin, p)
        {
            return Some(tile);
        }
        if self.over_panel(screen, p) {
            return None;
        }
        self.view.menu = None;
        let tile = city_view::world_to_tile(&self.map, r.screen_to_world(p))?;
        match self.tool {
            Tool::Road => {
                self.push_undo();
                self.view.road_from = Some(tile);
            }
            Tool::None => {}
            _ => {
                self.push_undo();
                self.view.stroke = Some(tile);
                self.apply(tile.0, tile.1);
            }
        }
        None
    }

    pub fn release(&mut self) {
        self.kingdom_release();
        self.view.pressed = None;
        self.view.stroke = None;
        if let (Some(a), Some(b)) = (self.view.road_from.take(), self.view.hover) {
            self.road(a, b);
        }
    }

    fn click_popup(&mut self, screen: [f32; 2], p: [f32; 2], sizes: bool) {
        let (x, y) = popup_origin(screen);
        if sizes {
            // Seven entries, Cancel last.
            for i in 0..7 {
                let rect = [x + 16.0, y + 50.0 + 24.0 * i as f32, 288.0, 20.0];
                if crate::widgets::inside(rect, p) {
                    self.view.popup = None;
                    if i < 6 {
                        self.new_map(i);
                    }
                    return;
                }
            }
            return;
        }
        if let Some(Popup::SaveName(_)) = &self.view.popup {
            if crate::widgets::inside([x + 192.0, y + 100.0, 34.0, 34.0], p) {
                self.view.popup = None;
            } else if crate::widgets::inside([x + 256.0, y + 100.0, 34.0, 34.0], p) {
                self.type_text("\n");
            }
        }
    }

    /// Puts the mouse over tile `t`, for scripted screenshots.
    pub fn set_hover(&mut self, t: Option<(i32, i32)>) {
        self.view.hover = t;
    }

    /// Scrolls the view to have tile `(x, y)` in its middle.
    pub fn centre_on(&mut self, r: &mut Renderer, x: i32, y: i32) {
        self.view.city.center_on(r, &self.map, x, y);
        self.view.placed = true;
    }

    /// Where the view starts: where the map's editor left it, else the middle.
    fn place_camera(&mut self, r: &mut Renderer) {
        self.view.placed = true;
        let [cx, cy] = self.scenario.camera;
        if cx > 0 || cy > 0 {
            let g = osiris_formats::chunks::GRID_SIZE as i32;
            let (x0, y0) = (self.scenario.info.start_offset % g, self.scenario.info.start_offset / g);
            let (a, b) = (2 * cx - (g + 2) - x0 + y0, cy - 1 - x0 - y0);
            let origin = city_view::tile_to_world(&self.map, 0, 0);
            r.camera.x = origin[0] + a as f32 * city_view::TILE_W / 2.0;
            r.camera.y = origin[1] + b as f32 * city_view::TILE_H / 2.0 - crate::sidebar::TOP / r.camera.zoom;
        } else {
            let (w, h) = (self.map.width, self.map.height);
            self.view.city.center_on(r, &self.map, w / 2, h / 2);
        }
    }

    /// The points' flags: a flag on its pole at the point, its kind's picture on the
    /// cloth (Pharaoh_General groups 139 and 140) and, where a kind has several, its
    /// number.
    fn flags(&self, img: &Images) -> (Vec<Sprite>, Vec<Overlay>, Vec<((i32, i32), String)>) {
        let frame = (self.view.clock * 10.0) as u32 % 8;
        let (mut sprites, mut icons, mut numbers) = (Vec::new(), Vec::new(), Vec::new());
        for (p, TilePoint { x, y }) in self.points() {
            let (icon, number) = match p {
                Point::Entry => (2, None),
                Point::Exit => (3, None),
                Point::RiverIn => (4, None),
                Point::RiverOut => (5, None),
                Point::Invasion(i) | Point::Fishing(i) | Point::Predator(i) | Point::Prey(i) | Point::Disembark(i) => (1, Some(i + 1)),
            };
            sprites.push(Sprite { behind: false, x, y, offset: (0, 0), image: img.flag + frame });
            let foot = city_view::tile_to_world(&self.map, x, y);
            // The flag's sprite hangs 15 pixels left of and 75 above its foot.
            let top = [foot[0] + 29.0 - 15.0, foot[1] + 23.0 - 75.0];
            icons.push(Overlay { x, y, pos: top, image: img.flag_icons + icon });
            if let Some(n) = number {
                numbers.push(((x, y), n.to_string()));
            }
        }
        (sprites, icons, numbers)
    }

    /// The map as drawn: cliffs shown as rock while H hides them.
    fn shown_map(&self) -> std::borrow::Cow<'_, Map> {
        if !self.view.hide_cliffs {
            return std::borrow::Cow::Borrowed(&self.map);
        }
        let mut m = self.map.clone();
        for y in 0..m.height {
            for x in 0..m.width {
                if m.terrain.at_or(x, y, 0) & bits::CLIFF != 0 {
                    let image = self.defs.terrain.rock + (m.random.at_or(x, y, 0) as u32 & 7);
                    m.set_single_image(x, y, image);
                }
            }
        }
        std::borrow::Cow::Owned(m)
    }

    pub fn draw(&mut self, r: &mut Renderer, panels: &PanelImages) {
        let Some(img) = self.view.images.or_else(|| Images::load(r)) else { return };
        self.view.images = Some(img);
        if !self.view.placed {
            self.place_camera(r);
        }
        if self.view.kingdom.is_some() {
            self.draw_kingdom(r, panels);
            if self.view.confirm.is_some() {
                self.draw_confirm(r, panels);
            }
            if let Some((s, _)) = &self.view.status {
                let sw = text_width(r, Font::LargeBlackOnDark, s) as f32;
                draw_text(r, Font::LargeBlackOnDark, s, ((r.screen[0] - sw) / 2.0).max(10.0), 60.0, font::WHITE);
            }
            return;
        }
        let [w, h] = r.screen;
        let ox = panel_left(w);
        if self.view.map_dirty {
            self.view.city = CityView::default();
        }
        if !self.view.free_scroll {
            self.view.city.clamp_camera(r, &self.map, ox, TOP);
        }
        self.view.saved_camera = Some([r.camera.x, r.camera.y + crate::sidebar::TOP / r.camera.zoom]);
        let (sprites, icons, numbers) = self.flags(&img);
        let marks = self.highlights();
        let mut city = std::mem::take(&mut self.view.city);
        city.art = crate::city_view::TerrainArt::new(&self.defs);
        city.draw(r, &self.shown_map(), &marks, &[], self.defs.terrain.empty_land, &sprites, &icons, None);
        self.view.city = city;
        for ((x, y), n) in numbers {
            let p = city_view::tile_to_world(&self.map, x, y);
            let world = [p[0] + 29.0 - 15.0 + 12.0, p[1] + 23.0 - 75.0 + 22.0];
            let s = [(world[0] - r.camera.x) * r.camera.zoom, (world[1] - r.camera.y) * r.camera.zoom];
            draw_text(r, Font::SmallPlain, &n, s[0], s[1], font::WHITE);
        }

        self.draw_panel(r, panels, &img);
        if self.view.top.is_none() {
            self.view.top = Some(self.menu_bar());
        }
        if let Some(bar) = &mut self.view.top {
            bar.draw(r, panels, &[], None);
        }
        // The map's name on the bar, as the guide says: "Your new mission name will
        // appear at the top of the screen."
        let name = if self.dirty { format!("{} *", self.name) } else { self.name.clone() };
        let nw = text_width(r, Font::NormalBlackOnLight, &name) as f32;
        draw_text(r, Font::NormalBlackOnLight, &name, (ox - nw - 16.0).max(300.0), 5.0, font::BLACK);
        if self.view.menu.is_some() {
            self.draw_submenu(r, panels);
        }
        if self.view.options.is_some() {
            self.draw_options(r, panels);
        }
        if self.view.popup.is_some() {
            self.draw_popup(r, panels);
        }
        if self.view.confirm.is_some() {
            self.draw_confirm(r, panels);
        }
        if let Some((s, _)) = &self.view.status {
            let sw = text_width(r, Font::LargeBlackOnDark, s) as f32;
            draw_text(r, Font::LargeBlackOnDark, s, ((ox - sw) / 2.0).max(10.0), 60.0, font::WHITE);
        }
        let _ = h;
    }

    /// The tiles the brush covers, under the cursor, or the point's tile tinted by
    /// whether it may go there.
    fn highlights(&self) -> Vec<Highlight> {
        let Some((x, y)) = self.view.hover else { return Vec::new() };
        let tint = |x: i32, y: i32, color: [f32; 4]| Highlight { x, y, color, paint: Tint::Silhouette };
        match self.tool {
            Tool::Paint(_) => super::terrain::brush(self.brush).into_iter().map(|(dx, dy)| tint(x + dx, y + dy, [1.0, 1.0, 1.0, 0.3])).collect(),
            Tool::Road => {
                let from = self.view.road_from.unwrap_or((x, y));
                let mut v = Vec::new();
                let (mut cx, cy) = from;
                while cx != x {
                    v.push(tint(cx, cy, [1.0, 1.0, 1.0, 0.3]));
                    cx += if x > cx { 1 } else { -1 };
                }
                let mut cy = cy;
                while cy != y {
                    v.push(tint(x, cy, [1.0, 1.0, 1.0, 0.3]));
                    cy += if y > cy { 1 } else { -1 };
                }
                v.push(tint(x, y, [1.0, 1.0, 1.0, 0.3]));
                v
            }
            Tool::Point(p) => {
                let ok = self.allowed(p, x, y);
                vec![Highlight { x, y, color: WHITE, paint: Tint::Filter(if ok { city_view::PLACE_OK } else { city_view::PLACE_BAD }) }]
            }
            Tool::None => vec![tint(x, y, [1.0, 1.0, 1.0, 0.25])],
        }
    }

    fn draw_panel(&mut self, r: &mut Renderer, panels: &PanelImages, img: &Images) {
        let [w, h] = r.screen;
        let ox = panel_left(w);
        // The top bar, laid leftwards from the strip: one copy ends at the strip, more
        // are laid a bordered length apart, each drawn after (so it covers) the plain
        // end of the one before it, as the sidebar's own top bar does.
        let bar_w = r.record(img.top_bar).map_or(1000.0, |rec| rec.width as f32);
        let mut starts = vec![w - STRIP_W - bar_w];
        while starts.last().is_some_and(|&x| x > 0.0) {
            starts.push(starts.last().copied().unwrap_or(0.0) - crate::sidebar::TOP_BAR_BORDERED);
        }
        for &x in starts.iter().rev() {
            r.image(img.top_bar, [x, 0.0], WHITE, Space::Screen);
        }
        r.rect([ox, TOP], [w - ox, h - TOP], [0.0, 0.0, 0.0, 1.0], Space::Screen);
        // The reliefs below the panel: the short one on an 800x600 screen, else the
        // tall one, stacked up from the bottom on taller screens.
        let relief = if h <= 620.0 { img.relief_small } else { img.relief };
        let rh = r.record(relief).map_or(285.0, |rec| rec.height as f32);
        r.set_clip(Some([ox, RELIEF_Y, PANEL_W, (h - RELIEF_Y).max(0.0)]));
        let mut y = if h <= RELIEF_Y + rh + 3.0 { RELIEF_Y } else { h - rh };
        while y + rh > RELIEF_Y {
            r.image(relief, [ox, y], WHITE, Space::Screen);
            y -= rh;
        }
        r.set_clip(None);
        let minimap = self.view.minimap.get_or_insert_with(|| Minimap::new(r));
        if self.view.map_dirty {
            minimap.mark_dirty();
            self.view.map_dirty = false;
        }
        minimap.draw_at(r, &self.map, |_| false, [ox + MINIMAP[0], TOP + MINIMAP[1]]);
        r.image(img.panel, [ox, TOP], WHITE, Space::Screen);
        let open = self.view.menu.as_ref().map(|(b, _)| *b);
        for &(b, bx, by, bw, bh, offset) in &BUTTONS {
            let hot = self.button_at(r.screen, self.view.cursor) == Some(b);
            let down = self.view.pressed == Some(b) || matches!(b, Button::Tool(t) if open == Some(t)) || (b == Button::Options && self.view.options.is_some());
            let frame = if down { 2 } else { hot as u32 };
            let base = if b == Button::Tool(32) { img.killer } else { img.buttons + offset };
            r.image(base + frame, [ox + bx, TOP + by], WHITE, Space::Screen);
            let _ = (bw, bh);
        }
        let t = |g: usize, i: usize| self.text.get(g, i).unwrap_or("").trim().to_owned();
        // Kingdom (44/132) and Options (2/0) on their buttons.
        draw_text(r, Font::NormalBlackOnLight, &t(44, 132), ox + 11.0, TOP + 128.0, font::BLACK);
        draw_text(r, Font::NormalBlackOnLight, &t(2, 0), ox + 94.0, TOP + 128.0, font::BLACK);
        self.draw_info(r, ox);
        // The strip down the right edge.
        let copies = ((h - 768.0) / (768.0 - 32.0)).ceil().max(0.0) as i32;
        for i in (0..=copies).rev() {
            let y = if i == 0 { 0.0 } else { i as f32 * (768.0 - 32.0) };
            r.image(img.strip, [w - STRIP_W, y], WHITE, Space::Screen);
        }
        let _ = panels;
    }

    /// The black box under the minimap (FUN_00533e70): the tool, the brush (dimmed
    /// for tools that don't use one), the killer type, then in red what the map still
    /// lacks and in green what is set: entry and exit points, invasion points, the
    /// river's points.
    fn draw_info(&self, r: &mut Renderer, ox: f32) {
        let [bx, by, bw, bh] = INFO_BOX;
        r.rect([ox + bx, TOP + by], [bw, bh], [0.0, 0.0, 0.0, 1.0], Space::Screen);
        let t = |g: usize, i: usize| self.text.get(g, i).unwrap_or("").trim().to_owned();
        let rgb565 = |c: u16| [((c >> 11) & 31) as f32 / 31.0, ((c >> 5) & 63) as f32 / 63.0, (c & 31) as f32 / 31.0, 1.0];
        let (text_c, grey, red, green) = (rgb565(0xfac1), rgb565(0x4208), rgb565(0xf800), rgb565(0x07e0));
        let x = ox + 10.0;
        let mut line = |s: &str, y: f32, c: [f32; 4]| {
            draw_text(r, Font::SmallPlain, s, x, y, c);
        };
        if let Some(id) = self.tool.name_id() {
            line(&t(49, id), 177.0, text_c);
        }
        let brushed = matches!(self.tool, Tool::Paint(_));
        line(&t(48, self.brush as usize), 193.0, if brushed { text_c } else { grey });
        let info = &self.scenario.info;
        let killer = if info.alt_predator_type == 1 {
            102 + info.climate.min(2) as usize
        } else {
            match info.climate {
                2 => 83,
                1 => 82,
                _ => 84,
            }
        };
        line(&t(64, killer), 209.0, text_c);
        let (entry, exit) = (info.entry_point.is_valid(), info.exit_point.is_valid());
        line(&t(44, if entry { 224 } else { 59 }), 225.0, if entry { green } else { red });
        line(&t(44, if exit { 225 } else { 61 }), 239.0, if exit { green } else { red });
        let invasions = info.invasion_points_land.iter().chain(&info.invasion_points_sea).filter(|p| p.is_valid()).count();
        match invasions {
            0 => line(&t(44, 63), 253.0, red),
            1 => line(&t(44, 64), 253.0, green),
            n => line(&format!("{n} {}", t(44, 65)), 253.0, green),
        }
        let (rin, rout) = (info.river_entry_point.is_valid(), info.river_exit_point.is_valid());
        let (id, c) = match (rin, rout) {
            (false, false) => (66, red),
            (true, true) => (67, green),
            _ => (226, red),
        };
        line(&t(44, id), 267.0, c);
    }

    /// The open submenu: a label strip per entry left of the panel, the one under the
    /// mouse lit (FUN_00520000).
    fn draw_submenu(&self, r: &mut Renderer, panels: &PanelImages) {
        let Some((_, items)) = &self.view.menu else { return };
        let x = panel_left(r.screen[0]) - 170.0;
        let top = submenu_top(items.len());
        let hot = self.item_at(r.screen, self.view.cursor);
        for (i, (label, _)) in items.iter().enumerate() {
            let y = top + 24.0 * i as f32;
            let lit = hot == Some(i);
            panel::label(r, panels, x, y, 10, if lit { 1 } else { 2 });
            let f = if lit { Font::NormalBlackOnDark } else { Font::NormalBlackOnLight };
            let tw = text_width(r, f, label) as f32;
            draw_text(r, f, label, x + ((160.0 - tw) / 2.0).max(4.0).floor(), y + 3.0, font::BLACK);
        }
    }

    fn draw_popup(&self, r: &mut Renderer, panels: &PanelImages) {
        let (x, y) = popup_origin(r.screen);
        let t = |g: usize, i: usize| self.text.get(g, i).unwrap_or("").trim().to_owned();
        let cursor = self.view.cursor;
        match &self.view.popup {
            Some(Popup::Sizes) => {
                panel::outer_panel(r, panels, x, y, 20, 15);
                let title = t(7, 1);
                let tw = text_width(r, Font::LargeBlackOnLight, &title) as f32;
                draw_text(r, Font::LargeBlackOnLight, &title, x + (320.0 - tw) / 2.0, y + 16.0, font::BLACK);
                for i in 0..7 {
                    let rect = [x + 16.0, y + 50.0 + 24.0 * i as f32, 288.0, 20.0];
                    let hot = crate::widgets::inside(rect, cursor);
                    let label = t(33, i);
                    let f = if hot { Font::NormalYellow } else { Font::NormalBlackOnLight };
                    let lw = text_width(r, f, &label) as f32;
                    draw_text(r, f, &label, rect[0] + (rect[2] - lw) / 2.0, rect[1] + 4.0, font::BLACK);
                }
            }
            Some(Popup::SaveName(name)) => {
                panel::outer_panel(r, panels, x, y, 30, 10);
                let title = t(7, 3);
                let tw = text_width(r, Font::LargeBlackOnLight, &title) as f32;
                draw_text(r, Font::LargeBlackOnLight, &title, x + (480.0 - tw) / 2.0, y + 20.0, font::BLACK);
                panel::inner_panel(r, panels, x + 64.0, y + 56.0, 22, 2);
                let caret = if (self.view.clock * 2.0) as i32 % 2 == 0 { "_" } else { "" };
                draw_text(r, Font::NormalWhiteOnDark, &format!("{name}{caret}"), x + 74.0, y + 66.0, font::WHITE);
                if let Ok(ok) = r.library.group_id("Pharaoh_General", 96, 0) {
                    let hot = |rect: [f32; 4]| crate::widgets::inside(rect, cursor) as u32;
                    r.image(ok + hot([x + 256.0, y + 100.0, 34.0, 34.0]), [x + 256.0, y + 100.0], WHITE, Space::Screen);
                    r.image(ok + 4 + hot([x + 192.0, y + 100.0, 34.0, 34.0]), [x + 192.0, y + 100.0], WHITE, Space::Screen);
                }
            }
            None => {}
        }
    }

    /// A yes/no warning (in popup.rs's style) over everything else, as the
    /// original's own popups sit.
    fn draw_confirm(&mut self, r: &mut Renderer, panels: &PanelImages) {
        let Some((c, _)) = &self.view.confirm else { return };
        let img = match UiImages::load(&r.library) {
            Ok(i) => i,
            Err(_) => return,
        };
        let cursor = self.view.cursor;
        let click = self.view.confirm_click.take();
        let text = self.text.clone();
        match c.draw(r, panels, img, &text, cursor, click) {
            Some(true) => {
                let (_, then) = self.view.confirm.take().expect("just matched Some");
                self.run_confirm(then);
            }
            Some(false) => self.view.confirm = None,
            None => {}
        }
    }
}

/// A popup's corner: 80 in from the corner of the centred 640x480 window, as the
/// original's popups sit.
fn popup_origin(screen: [f32; 2]) -> (f32, f32) {
    (((screen[0] - 640.0) / 2.0).floor() + 80.0, ((screen[1] - 480.0) / 2.0).floor() + 80.0)
}
