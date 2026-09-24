//! The game's bitmap fonts from `Pharaoh_Fonts.sg3`.
//!
//! Glyphs are addressed by a per-character index shared by all fonts plus a per-font
//! offset. Some fonts are stored in colour; the "plain" ones are drawn as silhouettes
//! in a caller-chosen colour, and the outlined font gets a black border.

use osiris_render::{Paint, Renderer, Space};

const FONT_PACK_BASE: u32 = 18765;
const SYSTEM_SLOTS: u32 = 201;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Font {
    SmallPlain,
    NormalBlackOnLight,
    NormalWhiteOnDark,
    NormalYellow,
    NormalBlue,
    LargeBlackOnLight,
    LargeBlackOnDark,
    SmallOutlined,
    NormalBlackOnDark,
    SmallShaded,
}

struct Def {
    offset: u32,
    space: i32,
    spacing: i32,
    line: i32,
}

impl Font {
    fn def(self) -> Def {
        let (offset, space, spacing, line) = match self {
            Font::SmallPlain => (0, 6, 1, 11),
            Font::NormalBlackOnLight => (134, 6, 0, 11),
            Font::NormalWhiteOnDark => (268, 6, 0, 11),
            Font::NormalYellow => (402, 6, 0, 11),
            Font::NormalBlue => (536, 8, 1, 11),
            Font::LargeBlackOnLight => (670, 8, 0, 23),
            Font::LargeBlackOnDark => (804, 8, 0, 23),
            Font::SmallOutlined => (938, 2, -1, 11),
            Font::NormalBlackOnDark => (1072, 6, 0, 11),
            Font::SmallShaded => (1206, 6, 2, 11),
        };
        Def {
            offset,
            space,
            spacing,
            line,
        }
    }

    fn silhouette(self) -> bool {
        matches!(self, Font::SmallPlain | Font::SmallShaded | Font::SmallOutlined)
    }

    pub fn line_height(self) -> i32 {
        self.def().line
    }
}

/// Glyph slot of each Windows-1252 byte (0 = no glyph).
const CHAR_MAP: [u8; 256] = [
    0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x01,
    0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x3F, 0x40, 0x00, 0x00, 0x41, 0x00, 0x4A, 0x43, 0x44, 0x42, 0x46, 0x4E, 0x45, 0x4F, 0x4D,
    0x3E, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A, 0x3B, 0x3C, 0x3D, 0x48, 0x49, 0x00, 0x47, 0x00, 0x4B,
    0x00, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29,
    0x2A, 0x2B, 0x2C, 0x2D, 0x2E, 0x2F, 0x30, 0x31, 0x32, 0x33, 0x34, 0x00, 0x00, 0x00, 0x00, 0x50,
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F,
    0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x81, 0x00, 0x00, 0x00, 0x6E, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x6F, 0x00, 0x00,
    0x00, 0x00, 0x7F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x82, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x80, 0x72, 0x70, 0x71, 0x71, 0x69, 0x83, 0x6D, 0x65, 0x74, 0x6A, 0x73, 0x73, 0x77, 0x75, 0x76,
    0x76, 0x00, 0x6C, 0x7A, 0x78, 0x79, 0x79, 0x7B, 0x00, 0x84, 0x7E, 0x7C, 0x7D, 0x6B, 0x33, 0x00,
    0x68, 0x53, 0x52, 0x54, 0x51, 0x51, 0x85, 0x67, 0x65, 0x57, 0x56, 0x58, 0x55, 0x5B, 0x5A, 0x5C,
    0x59, 0x00, 0x66, 0x5F, 0x5E, 0x60, 0x60, 0x5D, 0x00, 0x86, 0x63, 0x62, 0x64, 0x61, 0x19, 0x00,
];

/// Windows-1252 byte for a character, or `?` if it has none.
fn cp1252(c: char) -> u8 {
    let u = c as u32;
    if u < 0x80 || (0xA0..=0xFF).contains(&u) {
        return u as u8;
    }
    const HIGH: [(char, u8); 27] = [
        ('€', 0x80), ('‚', 0x82), ('ƒ', 0x83), ('„', 0x84), ('…', 0x85), ('†', 0x86), ('‡', 0x87),
        ('ˆ', 0x88), ('‰', 0x89), ('Š', 0x8A), ('‹', 0x8B), ('Œ', 0x8C), ('Ž', 0x8E), ('‘', 0x91),
        ('’', 0x92), ('“', 0x93), ('”', 0x94), ('•', 0x95), ('–', 0x96), ('—', 0x97), ('˜', 0x98),
        ('™', 0x99), ('š', 0x9A), ('›', 0x9B), ('œ', 0x9C), ('ž', 0x9E), ('Ÿ', 0x9F),
    ];
    HIGH.iter().find(|(ch, _)| *ch == c).map_or(b'?', |&(_, b)| b)
}

fn glyph_id(font: Font, byte: u8) -> Option<u32> {
    let slot = CHAR_MAP[byte as usize] as u32;
    (slot != 0).then(|| FONT_PACK_BASE + SYSTEM_SLOTS + font.def().offset + slot - 1)
}

/// Width in pixels of `text` in `font`.
pub fn text_width(r: &Renderer, font: Font, text: &str) -> i32 {
    let d = font.def();
    let mut w = 0;
    for c in text.chars() {
        let b = cp1252(c);
        if b == b' ' {
            w += d.space;
        } else if let Some(rec) = glyph_id(font, b).and_then(|id| r.record(id)) {
            w += rec.width as i32 + d.spacing;
        }
    }
    w
}

pub const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
pub const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

/// How far above the y it is given the original draws plain text (FUN_004cca60). Its
/// message-body text (FUN_004c8680) doesn't rise; [`draw_text_unrisen`] draws that.
const RISE: f32 = 3.0;

/// Draws plain text at the original's `(x, y)` (its glyph tops land three pixels
/// higher) and returns the width drawn. `color` tints the silhouette fonts and is
/// ignored by the coloured ones.
pub fn draw_text(r: &mut Renderer, font: Font, text: &str, x: f32, y: f32, color: [f32; 4]) -> i32 {
    draw(r, font, text, x, y - RISE, color, WHITE)
}

/// Like [`draw_text`], but also multiplies the coloured fonts by `tint` (to dim text).
pub fn draw_text_tinted(r: &mut Renderer, font: Font, text: &str, x: f32, y: f32, tint: [f32; 4]) -> i32 {
    draw(r, font, text, x, y - RISE, tint, tint)
}

/// Draws `text` with its glyph tops at `y`, as the original draws message bodies.
pub fn draw_text_unrisen(r: &mut Renderer, font: Font, text: &str, x: f32, y: f32, color: [f32; 4]) -> i32 {
    draw(r, font, text, x, y, color, WHITE)
}

fn draw(r: &mut Renderer, font: Font, text: &str, x: f32, y: f32, color: [f32; 4], tint: [f32; 4]) -> i32 {
    let d = font.def();
    let mut cx = x;
    for c in text.chars() {
        let b = cp1252(c);
        if b == b' ' {
            cx += d.space as f32;
            continue;
        }
        let Some(id) = glyph_id(font, b) else { continue };
        let Some(rec) = r.record(id) else { continue };
        let (w, h) = (rec.width as i32, rec.height as i32);
        // Accented capitals are taller than the line and rise above it.
        let lift = if b >= 0x80 && b != 0xE7 { (h - d.line).max(0) } else { 0 };
        let gy = y - lift as f32;
        match font {
            Font::SmallOutlined => {
                for (ox, oy) in [(1, 0), (2, 0), (0, 1), (1, 1), (2, 1), (0, 2), (1, 2), (2, 2)] {
                    r.image_painted(id, [cx + ox as f32, gy + oy as f32], BLACK, Space::Screen, Paint::Silhouette);
                }
                r.image_painted(id, [cx + 1.0, gy + 1.0], color, Space::Screen, Paint::Silhouette);
                cx += 2.0;
            }
            _ if font.silhouette() => {
                r.image_painted(id, [cx, gy], color, Space::Screen, Paint::Silhouette);
            }
            _ => {
                r.image_painted(id, [cx, gy], tint, Space::Screen, Paint::Normal);
            }
        }
        cx += (w + d.spacing) as f32;
    }
    (cx - x) as i32
}
