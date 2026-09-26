//! Interface drawn with the original game's panels, buttons and fonts.

pub mod dialog;
pub mod font;
pub mod panel;
pub mod rich_text;

pub use dialog::MessageDialog;
pub use font::{Font, centring_width, draw_text, draw_text_in, draw_text_tinted, draw_text_unrisen, text_width};
pub use panel::PanelImages;
