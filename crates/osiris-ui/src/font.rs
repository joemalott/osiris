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
            // The original draws it like the other normal fonts (FUN_004cc770).
            Font::NormalBlue => (536, 6, 0, 11),
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

/// Glyph slot of each Windows-1252 byte (0 = no glyph), as the exe's table at
/// 0x5e047c has it (FUN_004cca60 indexes it by the byte less 0x20). Besides the
/// Windows-1252 letters it keeps DOS code page 437's accented letters at 0x80-0xA8
/// (0x81 is ü, 0x84 ä, 0x94 ö, 0xA4 ñ), so a byte there draws what it did in DOS,
/// not what Windows-1252 puts there. The font has no « » or Ÿ.
const CHAR_MAP: [u8; 256] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x3F, 0x40, 0x00, 0x00, 0x41, 0x00, 0x4A, 0x43, 0x44, 0x42, 0x46, 0x4E, 0x45, 0x4F, 0x4D,
    0x3E, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A, 0x3B, 0x3C, 0x3D, 0x48, 0x49, 0x00, 0x47, 0x00, 0x4B,
    0x00, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29,
    0x2A, 0x2B, 0x2C, 0x2D, 0x2E, 0x2F, 0x30, 0x31, 0x32, 0x33, 0x34, 0x00, 0x63, 0x00, 0x00, 0x50,
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F,
    0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x65, 0x61, 0x56, 0x54, 0x51, 0x53, 0x01, 0x67, 0x81, 0x55, 0x57, 0x59, 0x6E, 0x5D, 0x69, 0x1B,
    0x6A, 0x67, 0x6D, 0x60, 0x5D, 0x5F, 0x64, 0x63, 0x19, 0x7B, 0x6B, 0x00, 0x6F, 0x00, 0x00, 0x00,
    0x52, 0x7F, 0x5E, 0x62, 0x66, 0x6C, 0x01, 0x0F, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x82, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80,
    0x72, 0x70, 0x71, 0x71, 0x69, 0x83, 0x6D, 0x65, 0x74, 0x6A, 0x73, 0x73, 0x77, 0x75, 0x76, 0x76,
    0x00, 0x6C, 0x7A, 0x78, 0x79, 0x79, 0x7B, 0x00, 0x84, 0x7E, 0x7C, 0x7D, 0x6B, 0x33, 0x00, 0x68,
    0x53, 0x52, 0x54, 0x51, 0x51, 0x85, 0x67, 0x65, 0x57, 0x56, 0x58, 0x55, 0x5B, 0x5A, 0x5C, 0x59,
    0x00, 0x66, 0x5F, 0x5E, 0x60, 0x60, 0x5D, 0x00, 0x86, 0x63, 0x62, 0x64, 0x61, 0x19, 0x00, 0x19,
];

/// The byte a character draws as: its Windows-1252 byte, or for the few marks the
/// font lacks, the nearest it has (guillemets become straight quotes, Ÿ a Y), so
/// French or German text never drops a character.
fn cp1252(c: char) -> u8 {
    match c {
        '«' | '»' => b'"',
        '‹' | '›' => b'\'',
        'Ÿ' => b'Y',
        _ => osiris_formats::text::cp1252_byte(c),
    }
}

fn glyph_id(font: Font, byte: u8) -> Option<u32> {
    let slot = CHAR_MAP[byte as usize] as u32;
    (slot != 0).then(|| FONT_PACK_BASE + SYSTEM_SLOTS + font.def().offset + slot - 1)
}

/// The plain letter an accented one is drawn over, to line their feet up.
fn base_letter(c: char) -> Option<char> {
    Some(match c {
        'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' | 'Æ' => 'A',
        'È' | 'É' | 'Ê' | 'Ë' => 'E',
        'Ì' | 'Í' | 'Î' | 'Ï' => 'I',
        'Ñ' => 'N',
        'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' | 'Œ' => 'O',
        'Ù' | 'Ú' | 'Û' | 'Ü' => 'U',
        'Š' => 'S',
        'Ž' => 'Z',
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'æ' => 'a',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'œ' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ý' | 'ÿ' => 'y',
        'š' => 's',
        'ž' => 'z',
        'ß' => 'b',
        _ => return None,
    })
}

/// The lowest row holding ink in image `id` (glyphs are drawn from their tops, and
/// the font's art puts letters at different depths in their boxes).
fn ink_bottom(r: &Renderer, id: u32) -> Option<i32> {
    use std::collections::HashMap;
    use std::sync::Mutex;
    static CACHE: Mutex<Option<HashMap<u32, Option<i32>>>> = Mutex::new(None);
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    *cache.get_or_insert_with(HashMap::new).entry(id).or_insert_with(|| {
        let img = r.library.resolve(id)?;
        let sprite = r.library.sg3(img.pack)?.decode(img.index as usize).ok()?;
        let w = sprite.width as usize;
        (0..sprite.height as usize).rev().find(|&row| sprite.pixels[row * w..(row + 1) * w].iter().any(|p| p[3] != 0)).map(|row| row as i32)
    })
}

/// How far above the line an accented letter is drawn. The exe raises every glyph
/// past 0x7F but ç by its height less the font's line (FUN_004cca60), which suits
/// the fonts its localized editions shipped; the English font's accented letters sit
/// lower in their boxes than that expects, so Osiris lines the letter's foot up with
/// its plain letter's instead, and falls back on the exe's rule for the rest (¿, ¡).
fn accent_lift(r: &Renderer, font: Font, c: char, b: u8, id: u32) -> i32 {
    if b < 0x80 || b == 0xE7 {
        return 0;
    }
    let plain = base_letter(c).and_then(|p| glyph_id(font, p as u8));
    match (plain.and_then(|p| ink_bottom(r, p)), ink_bottom(r, id)) {
        (Some(base), Some(own)) => (own - base).max(0),
        _ => r.record(id).map_or(0, |rec| (rec.height as i32 - font.def().line).max(0)),
    }
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

/// Width of `text` as the original measures it to centre it (FUN_004cbf30): as drawn,
/// except that the two large fonts count a space as 10 pixels and a pixel more for
/// each glyph, so centred large text sits a little left of the middle.
pub fn centring_width(r: &Renderer, font: Font, text: &str) -> i32 {
    let w = text_width(r, font, text);
    if !matches!(font, Font::LargeBlackOnLight | Font::LargeBlackOnDark) {
        return w;
    }
    let spaces = text.chars().filter(|&c| c == ' ').count() as i32;
    let glyphs = text.chars().filter(|&c| c != ' ' && glyph_id(font, cp1252(c)).is_some_and(|id| r.record(id).is_some())).count() as i32;
    w + spaces * (10 - font.def().space) + glyphs
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

/// Draws `text` with every glyph pixel in `color`, as the original draws text given
/// a colour of its own (red for the chosen mode in the editor's Kingdom map, say),
/// whatever colours the font's art holds.
pub fn draw_text_in(r: &mut Renderer, font: Font, text: &str, x: f32, y: f32, color: [f32; 4]) -> i32 {
    let sharp = std::mem::replace(&mut r.sharp, true);
    let w = draw_glyphs_as(r, font, text, x, y - RISE, color, WHITE, true);
    r.sharp = sharp;
    w
}

/// Draws `text` with its glyph tops at `y`, as the original draws message bodies.
pub fn draw_text_unrisen(r: &mut Renderer, font: Font, text: &str, x: f32, y: f32, color: [f32; 4]) -> i32 {
    draw(r, font, text, x, y, color, WHITE)
}

/// Tooltip text as the original sets it (FUN_004c8070 with the small font): cream
/// glyphs over a dark brown copy a pixel down and right, their tops at `y`.
pub fn draw_tooltip_text(r: &mut Renderer, text: &str, x: f32, y: f32) -> i32 {
    const SHADOW: [f32; 4] = [0x3a as f32 / 255.0, 0x25 as f32 / 255.0, 0x10 as f32 / 255.0, 1.0];
    const CREAM: [f32; 4] = [1.0, 0xe7 as f32 / 255.0, 0xd6 as f32 / 255.0, 1.0];
    draw(r, Font::SmallPlain, text, x + 1.0, y + 1.0, SHADOW, WHITE);
    draw(r, Font::SmallPlain, text, x, y, CREAM, WHITE) + 1
}

fn draw(r: &mut Renderer, font: Font, text: &str, x: f32, y: f32, color: [f32; 4], tint: [f32; 4]) -> i32 {
    // Glyphs land on whole device pixels and keep their pixels square at every
    // interface size, even where the art around them is filtered smoothly.
    let sharp = std::mem::replace(&mut r.sharp, true);
    let w = draw_glyphs(r, font, text, x, y, color, tint);
    r.sharp = sharp;
    w
}

fn draw_glyphs(r: &mut Renderer, font: Font, text: &str, x: f32, y: f32, color: [f32; 4], tint: [f32; 4]) -> i32 {
    draw_glyphs_as(r, font, text, x, y, color, tint, false)
}

#[allow(clippy::too_many_arguments)]
fn draw_glyphs_as(r: &mut Renderer, font: Font, text: &str, x: f32, y: f32, color: [f32; 4], tint: [f32; 4], recolour: bool) -> i32 {
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
        let w = rec.width as i32;
        let lift = accent_lift(r, font, c, b, id);
        let gy = y - lift as f32;
        match font {
            Font::SmallOutlined => {
                for (ox, oy) in [(1, 0), (2, 0), (0, 1), (1, 1), (2, 1), (0, 2), (1, 2), (2, 2)] {
                    r.image_painted(id, [cx + ox as f32, gy + oy as f32], BLACK, Space::Screen, Paint::Silhouette);
                }
                r.image_painted(id, [cx + 1.0, gy + 1.0], color, Space::Screen, Paint::Silhouette);
                cx += 2.0;
            }
            _ if recolour || font.silhouette() => {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Every letter German, French, Spanish and Italian write has a glyph, the
    /// accented ones the right one (slots as the font's art shows them).
    #[test]
    fn western_letters_have_glyphs() {
        let slot = |c: char| CHAR_MAP[cp1252(c) as usize];
        for c in "ÄÖÜäöüßÀÂÇÈÉÊËÎÏÔÙÛàâçèéêëîïôùûœŒÁÍÑÓÚáíñóú¡¿ÌÒìò«»Ÿÿ\"'".chars() {
            assert_ne!(slot(c), 0, "{c}");
        }
        let art = [('ä', 0x51), ('á', 0x52), ('à', 0x53), ('â', 0x54), ('ë', 0x55), ('é', 0x56), ('è', 0x57), ('ê', 0x58), ('ï', 0x59), ('í', 0x5A), ('ì', 0x5B), ('î', 0x5C), ('ö', 0x5D), ('ó', 0x5E), ('ò', 0x5F), ('ô', 0x60), ('ü', 0x61), ('ú', 0x62), ('ù', 0x63), ('û', 0x64), ('ç', 0x65), ('ñ', 0x66), ('ß', 0x68), ('Ä', 0x69), ('É', 0x6A), ('Ü', 0x6B), ('Ñ', 0x6C), ('Œ', 0x6E), ('œ', 0x6F), ('Á', 0x70), ('À', 0x72), ('È', 0x74), ('Í', 0x75), ('Ó', 0x78), ('Ò', 0x7A), ('Ö', 0x7B), ('Ú', 0x7C), ('Ù', 0x7E), ('¡', 0x7F), ('¿', 0x80)];
        for (c, s) in art {
            assert_eq!(slot(c), s, "{c}");
        }
    }
}
