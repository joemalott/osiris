//! The original's yes/no popup (FUN_00425570): a title and a question in a 30x10
//! panel, with the OK and cancel image buttons below.

use osiris_render::Renderer;
use osiris_ui::rich_text::{self, Options, RendererMeasure};
use osiris_ui::{Font, PanelImages, panel, text_width};

use crate::widgets::{Ui, UiImages};

pub struct Confirm {
    pub title: String,
    pub question: String,
}

impl Confirm {
    /// A popup from text group `group`: its title at `id` and question at `id + 1`,
    /// as the original lays out its popups.
    pub fn from_text(text: &osiris_formats::TextTable, group: usize, id: usize) -> Self {
        let t = |i| text.get(group, i).unwrap_or("").trim().to_owned();
        Self { title: t(id), question: t(id + 1) }
    }

    /// The popup's top-left corner: 80 in from the corner of the centred 640x480
    /// window.
    fn origin(screen: [f32; 2]) -> (f32, f32) {
        (((screen[0] - 640.0) / 2.0).floor() + 80.0, ((screen[1] - 480.0) / 2.0).floor() + 80.0)
    }

    /// Draws the popup; with `click` given, says whether it answered: `Some(true)`
    /// for OK, `Some(false)` for cancel.
    pub fn draw(&self, r: &mut Renderer, panels: &PanelImages, img: UiImages, text: &osiris_formats::TextTable, cursor: [f32; 2], click: Option<[f32; 2]>) -> Option<bool> {
        let (x, y) = Self::origin(r.screen);
        // A question too wide for one line wraps in a narrower column; past two
        // lines (a long translation) the panel grows and the buttons move down.
        let one_line = text_width(r, Font::NormalBlackOnLight, &self.question) < 420;
        let extra = if one_line {
            0.0
        } else {
            let opts = Options { font: Font::NormalBlackOnLight, width: 420, ..Default::default() };
            let h = rich_text::layout(&self.question, &opts, &mut RendererMeasure::new(r)).height as f32;
            (h - 2.0 * (Font::NormalBlackOnLight.line_height() as f32 + 5.0)).max(0.0)
        };
        let extra = (extra / 16.0).ceil() * 16.0;
        panel::outer_panel(r, panels, x, y, 30, 10 + (extra / 16.0) as i32);
        let mut ui = Ui { r, panels, img, text, cursor, click };
        ui.centred(Font::LargeBlackOnLight, &self.title, x, y + 20.0, 480.0);
        if one_line {
            ui.centred(Font::NormalBlackOnLight, &self.question, x, y + 60.0, 480.0);
        } else {
            ui.wrapped(Font::NormalBlackOnLight, &self.question, x + 30.0, y + 60.0, 420.0);
        }
        let ok = [x + 256.0, y + 100.0 + extra, 34.0, 34.0];
        let cancel = [x + 192.0, y + 100.0 + extra, 34.0, 34.0];
        let yes = ui.image_button(img.ok_cancel + ui.hot(ok) as u32, ok[0], ok[1], ok[2], ok[3]);
        let no = ui.image_button(img.ok_cancel + 4 + ui.hot(cancel) as u32, cancel[0], cancel[1], cancel[2], cancel[3]);
        if yes {
            Some(true)
        } else if no {
            Some(false)
        } else {
            None
        }
    }
}
