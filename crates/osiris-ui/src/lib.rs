//! Interface drawn with the original game's panels, buttons and fonts.

pub mod dialog;
pub mod font;
pub mod panel;
pub mod rich_text;

pub use dialog::MessageDialog;
pub use font::{Font, draw_text, text_width};
pub use panel::PanelImages;
