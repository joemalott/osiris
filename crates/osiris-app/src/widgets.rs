//! Immediate-mode drawing for the game's windows: each frame a window draws its
//! widgets, and the widget a held click falls on acts on it. Layout and behaviour
//! stay in one place.

use osiris_formats::{ImageLibrary, TextTable};
use osiris_render::{Renderer, Space};
use osiris_ui::rich_text::{self, Options, RendererMeasure};
use osiris_ui::{Font, PanelImages, draw_text, font, panel, text_width};

#[derive(Clone, Copy)]
pub struct UiImages {
    pub arrow_up: u32,
    pub arrow_down: u32,
    /// Small resource icons, indexed by resource.
    pub resource_icons: u32,
    /// Help, close, worker and people glyphs (Pharaoh_General group 134).
    pub context_icons: u32,
    /// Small overseer buttons, three frames per overseer (Pharaoh_General group 106).
    pub advisor_buttons: u32,
    /// Walker portraits, indexed by figure type (Pharaoh_Unloaded group 25).
    pub portraits: u32,
    /// A company's experience rank, Green to The best (Pharaoh_General group 2).
    pub experience_icons: u32,
}

impl UiImages {
    pub fn load(lib: &ImageLibrary) -> osiris_formats::Result<Self> {
        Ok(Self {
            arrow_up: lib.group_id("Pharaoh_Unloaded", 0, 16)?,
            arrow_down: lib.group_id("Pharaoh_Unloaded", 0, 18)?,
            resource_icons: lib.group_id("Expansion", 3, 0)?,
            context_icons: lib.group_id("Pharaoh_General", 134, 0)?,
            advisor_buttons: lib.group_id("Pharaoh_General", 106, 0)?,
            portraits: lib.group_id("Pharaoh_Unloaded", 25, 0)?,
            experience_icons: lib.group_id("Pharaoh_General", 2, 0)?,
        })
    }
}

pub fn inside(rect: [f32; 4], p: [f32; 2]) -> bool {
    p[0] >= rect[0] && p[1] >= rect[1] && p[0] < rect[0] + rect[2] && p[1] < rect[1] + rect[3]
}

pub struct Ui<'a> {
    pub r: &'a mut Renderer,
    pub panels: &'a PanelImages,
    pub img: UiImages,
    pub text: &'a TextTable,
    pub cursor: [f32; 2],
    /// The click not yet used by any widget this frame.
    pub click: Option<[f32; 2]>,
}

impl Ui<'_> {
    /// Text `id` of group `group`, or empty.
    pub fn t(&self, group: usize, id: usize) -> String {
        self.text.get(group, id).unwrap_or("").to_owned()
    }

    pub fn image(&mut self, id: u32, x: f32, y: f32) {
        self.r.image(id, [x, y], [1.0; 4], Space::Screen);
    }

    pub fn icon(&mut self, resource: u16, x: f32, y: f32) {
        self.image(self.img.resource_icons + resource as u32, x, y);
    }

    pub fn label(&mut self, f: Font, s: &str, x: f32, y: f32) -> f32 {
        let color = if matches!(f, Font::NormalWhiteOnDark | Font::NormalYellow | Font::NormalBlue) { font::WHITE } else { font::BLACK };
        draw_text(self.r, f, s, x, y, color) as f32
    }

    /// Text centred in a band `w` wide starting at `x`.
    pub fn centred(&mut self, f: Font, s: &str, x: f32, y: f32, w: f32) {
        let tw = self.width(f, s);
        self.label(f, s, (x + (w - tw) / 2.0).floor(), y);
    }

    /// Text wrapped to `w` pixels; returns its height.
    pub fn wrapped(&mut self, f: Font, s: &str, x: f32, y: f32, w: f32) -> f32 {
        if s.is_empty() {
            return 0.0;
        }
        let opts = Options { font: f, width: w as i32, ..Default::default() };
        let laid = rich_text::layout(s, &opts, &mut RendererMeasure::new(self.r));
        let color = if matches!(f, Font::NormalWhiteOnDark | Font::NormalYellow) { font::WHITE } else { font::BLACK };
        rich_text::draw(self.r, &laid, [x, y], laid.height as f32, 0.0, color);
        laid.height as f32
    }

    pub fn width(&self, f: Font, s: &str) -> f32 {
        text_width(self.r, f, s) as f32
    }

    /// Whether the held click fell in `rect`; it is used up if so.
    pub fn clicked(&mut self, rect: [f32; 4]) -> bool {
        if self.click.is_some_and(|c| inside(rect, c)) {
            self.click = None;
            return true;
        }
        false
    }

    pub fn hot(&self, rect: [f32; 4]) -> bool {
        inside(rect, self.cursor)
    }

    /// A bordered text button.
    pub fn button(&mut self, rect: [f32; 4], s: &str, f: Font) -> bool {
        let hot = self.hot(rect);
        panel::button_border(self.r, self.panels, rect[0], rect[1], rect[2] as i32, rect[3] as i32, hot);
        self.centred(f, s, rect[0], rect[1] + ((rect[3] - 12.0) / 2.0).floor(), rect[2]);
        self.clicked(rect)
    }

    pub fn arrow(&mut self, x: f32, y: f32, up: bool) -> bool {
        let base = if up { self.img.arrow_up } else { self.img.arrow_down };
        self.image(base, x, y);
        self.clicked([x, y, 24.0, 24.0])
    }

    /// An image button (help, close): the image, and a click on it.
    pub fn image_button(&mut self, id: u32, x: f32, y: f32, w: f32, h: f32) -> bool {
        self.image(id, x, y);
        self.clicked([x, y, w, h])
    }
}
