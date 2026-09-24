//! The menu bar along the top of the city screen (File, Options, Help, Overlays,
//! Overseers) with its drop-down menus, and the city status on the right.

use osiris_formats::TextTable;
use osiris_render::Renderer;
use osiris_ui::{Font, PanelImages, draw_text, draw_text_tinted, font, panel, text_width};

/// What a menu entry asks for. The game handles some itself and passes the rest up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    MainMenu,
    Replay,
    Load,
    Save,
    Quit,
    Rules,
    Difficulty,
    Autosave,
    /// Out of time: play on at a lower difficulty.
    LowerDifficulty,
    Faster,
    Slower,
    Pause,
    Controls,
    About,
    Overlay(Option<crate::overlay::Overlay>),
    Overseer(crate::advisors::Advisor),
    /// Not in Osiris yet; shown greyed out.
    Unavailable,
}

#[derive(Clone)]
pub struct Entry {
    pub label: String,
    pub action: MenuAction,
}

#[derive(Clone)]
pub struct Header {
    pub label: String,
    pub entries: Vec<Entry>,
    x: f32,
    w: f32,
}

pub struct TopMenu {
    pub headers: Vec<Header>,
    /// The headers as built. On a screen narrower than 800 the Overlays header folds
    /// into Options, leaving File, Options, Help and Overseers as in the original.
    all: Vec<Header>,
    narrow: Option<bool>,
    pub open: Option<usize>,
    hover: Option<usize>,
    cursor: [f32; 2],
}

const BAR_Y: f32 = 6.0;
const ITEM_H: f32 = 20.0;
const DROP_W: f32 = 240.0;

impl TopMenu {
    /// Relabels the entries for `action` in every header.
    pub fn relabel(&mut self, action: MenuAction, label: &str) {
        for h in self.all.iter_mut().chain(self.headers.iter_mut()) {
            for e in h.entries.iter_mut().filter(|e| e.action == action) {
                e.label = label.to_owned();
            }
        }
    }

    pub fn new(text: &TextTable) -> Self {
        let t = |g: usize, i: usize, fallback: &str| text.get(g, i).map_or_else(|| fallback.to_owned(), |s| s.trim().to_owned());
        let e = |label: String, action| Entry { label, action };
        let mut overlays = vec![e(t(14, 0, "Normal"), MenuAction::Overlay(None))];
        for (o, id) in crate::overlay::MENU {
            overlays.push(e(t(14, id, "?"), MenuAction::Overlay(Some(o))));
        }
        let overseers = crate::advisors::ALL
            .iter()
            .enumerate()
            .map(|(i, &a)| e(t(4, i + 1, "Overseer"), if a.available() { MenuAction::Overseer(a) } else { MenuAction::Unavailable }))
            .collect();
        let headers = vec![
            (
                t(1, 0, "File"),
                vec![
                    e(t(1, 1, "New game"), MenuAction::MainMenu),
                    e(t(1, 2, "Replay mission"), MenuAction::Replay),
                    e(t(1, 3, "Load game"), MenuAction::Load),
                    e(t(1, 4, "Save game"), MenuAction::Save),
                    e("Exit to main menu".into(), MenuAction::MainMenu),
                    e(t(1, 5, "Exit game"), MenuAction::Quit),
                ],
            ),
            (
                t(2, 0, "Options"),
                vec![
                    e(t(2, 6, "Difficulty"), MenuAction::Difficulty),
                    e(t(2, 9, "Autosave - ON"), MenuAction::Autosave),
                    e("Game rules...".into(), MenuAction::Rules),
                    e("Faster  (Page Up)".into(), MenuAction::Faster),
                    e("Slower  (Page Down)".into(), MenuAction::Slower),
                    e("Pause  (P)".into(), MenuAction::Pause),
                ],
            ),
            (t(3, 0, "Help"), vec![e("Controls".into(), MenuAction::Controls), e(t(3, 7, "About"), MenuAction::About)]),
            ("Overlays".to_owned(), overlays),
            (t(4, 0, "Overseers"), overseers),
        ];
        let all: Vec<Header> = headers.into_iter().map(|(label, entries)| Header { label, entries, x: 0.0, w: 0.0 }).collect();
        Self {
            headers: all.clone(),
            all,
            narrow: None,
            open: None,
            hover: None,
            cursor: [0.0; 2],
        }
    }

    fn layout(&mut self, r: &Renderer) {
        let narrow = r.screen[0] < 800.0;
        if self.narrow != Some(narrow) {
            // A menu open across a change of layout closes.
            if self.narrow.is_some() {
                self.open = None;
            }
            self.narrow = Some(narrow);
            self.headers = self.all.clone();
            if narrow {
                let overlays = self.headers.remove(3);
                self.headers[1].entries.extend(overlays.entries);
            }
        }
        let mut x = 10.0;
        for h in &mut self.headers {
            h.w = text_width(r, Font::NormalBlackOnLight, &h.label) as f32;
            h.x = x;
            // The original leaves 10 pixels between headers.
            x += h.w + 10.0;
        }
    }

    fn header_at(&self, p: [f32; 2]) -> Option<usize> {
        if p[1] >= crate::sidebar::TOP {
            return None;
        }
        self.headers.iter().position(|h| p[0] >= h.x - 4.0 && p[0] < h.x + h.w + 4.0)
    }

    fn entry_at(&self, p: [f32; 2]) -> Option<usize> {
        let h = &self.headers[self.open?];
        let top = crate::sidebar::TOP + 4.0;
        if p[0] < h.x || p[0] >= h.x + DROP_W || p[1] < top + 8.0 {
            return None;
        }
        let i = ((p[1] - top - 8.0) / ITEM_H) as usize;
        (i < h.entries.len()).then_some(i)
    }

    /// Whether `p` is over the bar or an open drop-down.
    pub fn contains(&self, p: [f32; 2]) -> bool {
        if self.header_at(p).is_some() {
            return true;
        }
        let Some(open) = self.open else { return false };
        let h = &self.headers[open];
        let bottom = crate::sidebar::TOP + 4.0 + 16.0 + ITEM_H * h.entries.len() as f32;
        p[0] >= h.x && p[0] < h.x + DROP_W && p[1] < bottom
    }

    pub fn hover(&mut self, p: [f32; 2]) {
        self.cursor = p;
        if self.open.is_some()
            && let Some(h) = self.header_at(p)
        {
            self.open = Some(h);
        }
        self.hover = self.entry_at(p);
    }

    /// A click: opens or closes a menu, or returns the chosen entry's action.
    /// `None` with `handled` false means the click was not on the menu at all.
    pub fn click(&mut self, p: [f32; 2]) -> (bool, Option<MenuAction>) {
        if let Some(h) = self.header_at(p) {
            self.open = if self.open == Some(h) { None } else { Some(h) };
            return (true, None);
        }
        if let Some(open) = self.open {
            // Any click closes an open menu, and is used up doing so.
            let chosen = self.entry_at(p).map(|i| self.headers[open].entries[i].action);
            self.open = None;
            return (true, chosen.filter(|&a| a != MenuAction::Unavailable));
        }
        (false, None)
    }

    /// Draws the headers, the status on the right and any open drop-down.
    pub fn draw(&mut self, r: &mut Renderer, panels: &PanelImages, status: &[(String, String)], overlay: Option<&str>) {
        self.layout(r);
        for (i, h) in self.headers.iter().enumerate() {
            let hot = self.open == Some(i) || (self.open.is_none() && self.header_at(self.cursor) == Some(i));
            let f = if hot { Font::NormalYellow } else { Font::NormalBlackOnLight };
            draw_text(r, f, &h.label, h.x, BAR_Y, font::BLACK);
        }
        // The treasury, population and date at the original's places for its 640, 800
        // and 1024 wide screens, kept that far from the right edge on wider ones.
        let w = r.screen[0];
        let (xs, shift) = if w < 800.0 {
            ([250.0, 375.0, 515.0], w - 640.0)
        } else if w < 1024.0 {
            ([343.0, 470.0, 655.0], w - 800.0)
        } else {
            ([495.0, 645.0, 883.0], w - 1024.0)
        };
        for ((label, value), x) in status.iter().zip(xs) {
            let x = x + shift;
            let lw = text_width(r, Font::NormalBlackOnLight, label) as f32;
            draw_text(r, Font::NormalBlackOnLight, label, x, BAR_Y, font::BLACK);
            draw_text(r, Font::NormalBlackOnLight, value, x + lw + 4.0, BAR_Y, font::BLACK);
        }
        if let Some(name) = overlay {
            let label = format!("Overlay: {name}");
            draw_text(r, Font::SmallOutlined, &label, 10.0, crate::sidebar::TOP + 26.0, font::WHITE);
        }
        let Some(open) = self.open else { return };
        let h = &self.headers[open];
        let top = crate::sidebar::TOP + 4.0;
        let blocks_h = ((ITEM_H * h.entries.len() as f32 + 16.0) / 16.0).ceil() as i32;
        panel::outer_panel(r, panels, h.x, top, (DROP_W / 16.0) as i32, blocks_h);
        for (i, e) in h.entries.iter().enumerate() {
            let y = top + 8.0 + ITEM_H * i as f32;
            if e.action == MenuAction::Unavailable {
                draw_text_tinted(r, Font::NormalBlackOnLight, &e.label, h.x + 12.0, y + 2.0, [0.5, 0.45, 0.4, 1.0]);
                continue;
            }
            let f = if self.hover == Some(i) { Font::NormalYellow } else { Font::NormalBlackOnLight };
            draw_text(r, f, &e.label, h.x + 12.0, y + 2.0, font::BLACK);
        }
    }
}
