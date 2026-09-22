//! Window, panel and button frames assembled from the game's 16x16 border tiles.

use osiris_render::{Renderer, Space, WHITE};

/// Global image ids of the first tile of each frame group (Pharaoh_General).
#[derive(Clone)]
pub struct PanelImages {
    /// Group 132: outer (window) panel.
    pub dialog: u32,
    /// Group 133: sunken inner panel.
    pub sunken: u32,
    /// Group 15: labels and small panels.
    pub panel_button: u32,
    /// Group 174: bordered button frame.
    pub bordered_button: u32,
}

impl PanelImages {
    pub fn load(lib: &osiris_formats::ImageLibrary) -> osiris_formats::Result<Self> {
        let g = |group| lib.group_id("Pharaoh_General", group, 0);
        Ok(Self {
            dialog: g(132)?,
            sunken: g(133)?,
            panel_button: g(15)?,
            bordered_button: g(174)?,
        })
    }
}

fn img(r: &mut Renderer, id: u32, x: f32, y: f32) {
    r.image(id, [x, y], WHITE, Space::Screen);
}

/// A raised window panel of `w x h` 16-pixel blocks.
pub fn outer_panel(r: &mut Renderer, p: &PanelImages, x: f32, y: f32, w: i32, h: i32) {
    let mut image_y = 0;
    for yy in 0..h {
        let mut image_x = 0;
        
        for xx in 0..w {
            let offset = if yy == 0 {
                if xx == 0 {
                    0
                } else if xx < w - 1 {
                    image_x += 1;
                    image_x
                } else {
                    11
                }
            } else if yy < h - 1 {
                if xx == 0 {
                    12 + image_y
                } else if xx < w - 1 {
                    let v = 13 + image_y + image_x;
                    image_x += 1;
                    v
                } else {
                    23 + image_y
                }
            } else if xx == 0 {
                132
            } else if xx < w - 1 {
                let v = 133 + image_x;
                image_x += 1;
                v
            } else {
                143
            };
            img(r, p.dialog + offset as u32, x + 16.0 * xx as f32, y + 16.0 * yy as f32);
            if image_x >= 10 {
                image_x = 0;
            }
        }
        let y_add = if yy == 0 || yy == h - 1 { 0 } else { 12 };
        image_y += y_add;
        if image_y >= 120 {
            image_y = 0;
        }
    }
}

/// A sunken text area of `w x h` 16-pixel blocks.
pub fn inner_panel(r: &mut Renderer, p: &PanelImages, x: f32, y: f32, w: i32, h: i32) {
    let mut image_y = 0;
    for yy in 0..h {
        let mut image_x = 0;
        for xx in 0..w {
            let offset = if yy == 0 {
                if xx == 0 {
                    0
                } else if xx < w - 1 {
                    image_x += 1;
                    image_x
                } else {
                    6
                }
            } else if yy < h - 1 {
                if xx == 0 {
                    7 + image_y
                } else if xx < w - 1 {
                    let v = 8 + image_y + image_x;
                    image_x += 1;
                    v
                } else {
                    13 + image_y
                }
            } else if xx == 0 {
                42
            } else if xx < w - 1 {
                let v = 43 + image_x;
                image_x += 1;
                v
            } else {
                48
            };
            img(r, p.sunken + offset as u32, x + 16.0 * xx as f32, y + 16.0 * yy as f32);
            if image_x >= 5 {
                image_x = 0;
            }
        }
        if yy != 0 && yy != h - 1 {
            image_y += 7;
        }
        if image_y >= 35 {
            image_y = 0;
        }
    }
}

/// A 25-pixel-high sandstone button face of `w` 16-pixel blocks. `style` 0 is
/// plain, 1 highlighted.
pub fn large_label(r: &mut Renderer, p: &PanelImages, x: f32, y: f32, w: i32, style: u32) {
    for i in 0..w {
        let part = if i == 0 {
            0
        } else if i < w - 1 {
            1
        } else {
            2
        };
        img(r, p.panel_button + 3 * style + part, x + 16.0 * i as f32, y);
    }
}

/// A one-block-high label strip. `style` 0 is plain, 1 highlighted.
pub fn label(r: &mut Renderer, p: &PanelImages, x: f32, y: f32, w: i32, style: u32) {
    for i in 0..w {
        let offset = if i == 0 {
            3 * style + 40
        } else if i < w - 1 {
            3 * style + 41
        } else {
            3 * style + 42
        };
        img(r, p.panel_button + offset, x + 16.0 * i as f32, y);
    }
}

/// The thin frame drawn around text buttons, `w x h` pixels.
pub fn button_border(r: &mut Renderer, p: &PanelImages, x: f32, y: f32, w: i32, h: i32, focus: bool) {
    let wb = (w + 15) / 16;
    let hb = (h + 15) / 16;
    let last_x = (16 * wb - w) as f32;
    let last_y = (16 * hb - h) as f32;
    let base = p.bordered_button + if focus { 8 } else { 0 };
    for yy in 0..hb {
        let py = y + 16.0 * yy as f32;
        for xx in 0..wb {
            let px = x + 16.0 * xx as f32;
            if yy == 0 {
                if xx == 0 {
                    img(r, base, px, py);
                } else if xx < wb - 1 {
                    img(r, base + 1, px, py);
                } else {
                    img(r, base + 2, px - last_x, py);
                }
            } else if yy < hb - 1 {
                if xx == 0 {
                    img(r, base + 7, px, py);
                } else if xx >= wb - 1 {
                    img(r, base + 3, px - last_x, py);
                }
            } else if xx == 0 {
                img(r, base + 6, px, py - last_y);
            } else if xx < wb - 1 {
                img(r, base + 5, px, py - last_y);
            } else {
                img(r, base + 4, px - last_x, py - last_y);
            }
        }
    }
}
