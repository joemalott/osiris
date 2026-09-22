//! Word-wrapped layout for message body text, following the original's inline codes.
//!
//! `@P` starts a new paragraph (line break, next line indented like the original's
//! first-line indent); `@L` is a plain line break (no indent); `@G<digits>` embeds an
//! image inline, breaking the line before and after it. Other `@` codes used by the
//! original for coloured runs and cross-links (`@Y...&`, `@<digits>...&`) aren't
//! reproduced as separate styling here — the task only calls for handling breaks and
//! images, and for other codes to fall back to plain text — so their marker characters
//! are dropped and the underlying words are laid out normally.
//!
//! [`layout`] is a pure function: given the text and a [`Measure`] (a thin
//! width/image-lookup interface so it can run without a GPU-backed `Renderer`), it
//! returns a [`Layout`] of positioned [`Run`]s plus the total content height. Drawing
//! ([`draw`]) is a thin layer that walks those runs, offsets them by an origin and a
//! vertical scroll, culls anything outside the given viewport, and calls into
//! `osiris_render`/`osiris_ui::font`.

use crate::font::{Font, draw_text, text_width};
use osiris_render::{Renderer, Space};

/// Width/height measurement used by [`layout`], so layout logic can be unit tested
/// without a GPU-backed `Renderer`.
pub trait Measure {
    /// Pixel width of `text` set in `font`.
    fn width(&self, font: Font, text: &str) -> i32;

    /// Resolves an inline `@G<code>` image reference to a global image id (for drawing)
    /// and its pixel size, or `None` to skip the image entirely (it contributes nothing
    /// to the layout). The default skips every image.
    fn image(&mut self, _code: u32) -> Option<(u32, i32, i32)> {
        None
    }
}

/// [`Measure`] backed by a real `Renderer`, using its bitmap fonts and, if given, the
/// original's `GROUP_MESSAGE_IMAGES` group (`Pharaoh_Unloaded` group 10) to resolve
/// `@G<code>` references (`code` is 1-based, matching the source format).
pub struct RendererMeasure<'a> {
    r: &'a Renderer,
    images_base: Option<u32>,
}

impl<'a> RendererMeasure<'a> {
    pub fn new(r: &'a Renderer) -> Self {
        let images_base = r.library.group_id("Pharaoh_Unloaded", 10, 0).ok();
        Self { r, images_base }
    }
}

impl Measure for RendererMeasure<'_> {
    fn width(&self, font: Font, text: &str) -> i32 {
        text_width(self.r, font, text)
    }

    fn image(&mut self, code: u32) -> Option<(u32, i32, i32)> {
        let base = self.images_base?;
        let id = base + code.checked_sub(1)?;
        let rec = self.r.record(id)?;
        Some((id, rec.width as i32, rec.height as i32))
    }
}

/// Layout options.
#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub font: Font,
    /// Pixel width available to wrap text into.
    pub width: i32,
    /// Indent (in pixels) of the first line of a new paragraph (`@P`); the original
    /// uses 50.
    pub paragraph_indent: i32,
}

impl Default for Options {
    fn default() -> Self {
        Self { font: Font::NormalBlackOnLight, width: 300, paragraph_indent: 50 }
    }
}

/// One positioned piece of laid-out content, in content-local pixels (origin at the
/// layout's top-left, `y` growing downward).
#[derive(Debug, Clone, PartialEq)]
pub enum Run {
    Text { x: i32, y: i32, font: Font, text: String },
    Image { x: i32, y: i32, id: u32, w: i32, h: i32 },
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Layout {
    pub runs: Vec<Run>,
    /// Total laid-out height in pixels.
    pub height: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Break {
    Paragraph,
    Line,
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Word(String),
    Break(Break),
    Image(u32),
}

/// Splits `text` into words, forced breaks and image references. `@P`/`@L`/`@G<digits>`
/// are recognised anywhere; any other `@` is dropped (along with a following link id or
/// `Y`/single following letter) so the remaining text still wraps as plain words. This
/// intentionally isn't a faithful reproduction of the original's link/colour parsing —
/// see the module doc.
fn tokenize(text: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut word = String::new();
    let mut chars = text.chars().peekable();
    let flush = |word: &mut String, tokens: &mut Vec<Token>| {
        if !word.is_empty() {
            tokens.push(Token::Word(std::mem::take(word)));
        }
    };
    while let Some(c) = chars.next() {
        match c {
            ' ' | '\n' | '\r' | '\t' => flush(&mut word, &mut tokens),
            '@' => {
                flush(&mut word, &mut tokens);
                match chars.peek().copied() {
                    Some('P') => {
                        chars.next();
                        tokens.push(Token::Break(Break::Paragraph));
                    }
                    Some('L') => {
                        chars.next();
                        tokens.push(Token::Break(Break::Line));
                    }
                    Some('G') => {
                        chars.next();
                        let mut digits = String::new();
                        while chars.peek().is_some_and(char::is_ascii_digit) {
                            digits.push(chars.next().unwrap());
                        }
                        if let Ok(code) = digits.parse() {
                            tokens.push(Token::Image(code));
                        }
                    }
                    Some('Y') => {
                        // Coloured run marker: drop it and its closing '&'.
                        chars.next();
                    }
                    Some('I') => {
                        // Inline small-image glyph: skip the code and its digits, and
                        // don't attempt to render it inline (see module doc).
                        chars.next();
                        while chars.peek().is_some_and(char::is_ascii_digit) {
                            chars.next();
                        }
                    }
                    Some(d) if d.is_ascii_digit() => {
                        // Link marker: drop the numeric id, keep the link text itself.
                        while chars.peek().is_some_and(char::is_ascii_digit) {
                            chars.next();
                        }
                    }
                    _ => {}
                }
            }
            '&' => {} // closes an `@Y`/link run; nothing to undo since we don't restyle.
            _ => word.push(c),
        }
    }
    flush(&mut word, &mut tokens);
    tokens
}

/// Lays `text` out into `opts.width` pixels, using `measure` for glyph widths and
/// (optionally) inline image sizes. Pure: performs no drawing.
/// Vertical distance between wrapped lines; the original spaces 11-pixel fonts 16 apart.
pub fn line_advance(font: Font) -> i32 {
    font.line_height() + if font.line_height() <= 11 { 5 } else { 3 }
}

pub fn layout(text: &str, opts: &Options, measure: &mut dyn Measure) -> Layout {
    let tokens = tokenize(text);
    let line_h = line_advance(opts.font);
    let space_w = measure.width(opts.font, " ");
    let mut runs = Vec::new();
    let mut y = 0;
    let mut indent = 0;
    let mut i = 0;
    while i < tokens.len() {
        let start_x = indent;
        indent = 0;
        let mut x = start_x;
        let mut words: Vec<String> = Vec::new();
        let mut pending_image = None;
        while let Some(tok) = tokens.get(i) {
            match tok {
                Token::Break(b) => {
                    indent = match b {
                        Break::Paragraph => opts.paragraph_indent,
                        Break::Line => 0,
                    };
                    i += 1;
                    break;
                }
                Token::Image(code) => {
                    if let Some((id, w, h)) = measure.image(*code) {
                        pending_image = Some((id, w, h));
                        i += 1;
                        break;
                    }
                    // No mapping for this code: skip it, keep flowing text.
                    i += 1;
                }
                Token::Word(w) => {
                    let ww = measure.width(opts.font, w);
                    let extra = if words.is_empty() { 0 } else { space_w };
                    if !words.is_empty() && x + extra + ww > opts.width {
                        break;
                    }
                    x += extra + ww;
                    words.push(w.clone());
                    i += 1;
                }
            }
        }
        if !words.is_empty() {
            runs.push(Run::Text { x: start_x, y, font: opts.font, text: words.join(" ") });
        }
        y += line_h;
        if let Some((id, w, h)) = pending_image {
            let img_x = ((opts.width - w) / 2).max(0);
            runs.push(Run::Image { x: img_x, y, id, w, h });
            let rows = (h + line_h - 1) / line_h.max(1);
            y += rows.max(1) * line_h;
        }
    }
    Layout { runs, height: y }
}

/// Draws `layout` with its top-left at `origin`, offset upward by `scroll` pixels, and
/// clipped to `origin.y .. origin.y + viewport_h` (runs entirely outside are skipped;
/// there's no GPU scissor, so partially visible text/images may draw a few pixels past
/// the edge).
pub fn draw(r: &mut Renderer, laid_out: &Layout, origin: [f32; 2], viewport_h: f32, scroll: f32, color: [f32; 4]) {
    for run in &laid_out.runs {
        match run {
            Run::Text { x, y, font, text } => {
                let py = origin[1] + *y as f32 - scroll;
                if py + font.line_height() as f32 <= origin[1] || py >= origin[1] + viewport_h {
                    continue;
                }
                draw_text(r, *font, text, origin[0] + *x as f32, py, color);
            }
            Run::Image { x, y, id, w: _, h } => {
                let py = origin[1] + *y as f32 - scroll;
                if py + *h as f32 <= origin[1] || py >= origin[1] + viewport_h {
                    continue;
                }
                r.image(*id, [origin[0] + *x as f32, py], osiris_render::WHITE, Space::Screen);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One pixel per character, fixed space width, no images — enough to exercise the
    /// wrapping logic without a GPU.
    struct FixedWidth {
        char_w: i32,
        space_w: i32,
        images: std::collections::HashMap<u32, (u32, i32, i32)>,
    }

    impl FixedWidth {
        fn new() -> Self {
            Self { char_w: 10, space_w: 5, images: Default::default() }
        }
    }

    impl Measure for FixedWidth {
        fn width(&self, _font: Font, text: &str) -> i32 {
            if text == " " {
                return self.space_w;
            }
            text.chars().count() as i32 * self.char_w
        }

        fn image(&mut self, code: u32) -> Option<(u32, i32, i32)> {
            self.images.get(&code).copied()
        }
    }

    fn opts(width: i32) -> Options {
        Options { font: Font::NormalBlackOnLight, width, paragraph_indent: 50 }
    }

    fn texts(l: &Layout) -> Vec<&str> {
        l.runs
            .iter()
            .filter_map(|r| match r {
                Run::Text { text, .. } => Some(text.as_str()),
                Run::Image { .. } => None,
            })
            .collect()
    }

    #[test]
    fn wraps_on_word_boundaries() {
        // Each word is 30px ("aaa"), space 5px; width 65 fits two words (30+5+30=65) but
        // not three.
        let mut m = FixedWidth::new();
        let l = layout("aaa bbb ccc", &opts(65), &mut m);
        assert_eq!(texts(&l), vec!["aaa bbb", "ccc"]);
        assert_eq!(l.height, line_advance(Font::NormalBlackOnLight) * 2);
    }

    #[test]
    fn overlong_word_does_not_hang() {
        let mut m = FixedWidth::new();
        let l = layout("aaaaaaaaaaaaaaaaaaaa short", &opts(50), &mut m);
        // The long word still gets its own line rather than looping forever.
        assert_eq!(texts(&l), vec!["aaaaaaaaaaaaaaaaaaaa", "short"]);
    }

    #[test]
    fn paragraph_break_indents_next_line() {
        let mut m = FixedWidth::new();
        let l = layout("aaa@Pbbb", &opts(200), &mut m);
        assert_eq!(texts(&l), vec!["aaa", "bbb"]);
        let bbb = l.runs.iter().find(|r| matches!(r, Run::Text{ text, .. } if text == "bbb")).unwrap();
        assert!(matches!(bbb, Run::Text { x: 50, .. }));
    }

    #[test]
    fn line_break_does_not_indent() {
        let mut m = FixedWidth::new();
        let l = layout("aaa@Lbbb", &opts(200), &mut m);
        let bbb = l.runs.iter().find(|r| matches!(r, Run::Text{ text, .. } if text == "bbb")).unwrap();
        assert!(matches!(bbb, Run::Text { x: 0, .. }));
    }

    #[test]
    fn unknown_at_codes_fall_back_to_plain_words() {
        let mut m = FixedWidth::new();
        // @Y...& colours "shiny", @3 links "here" to message 3.
        let l = layout("plain @Yshiny& and @3here& text", &opts(1000), &mut m);
        assert_eq!(texts(&l), vec!["plain shiny and here text"]);
    }

    #[test]
    fn image_code_without_mapping_is_skipped() {
        let mut m = FixedWidth::new();
        let l = layout("before @G1 after", &opts(1000), &mut m);
        assert_eq!(texts(&l), vec!["before after"]);
        assert!(l.runs.iter().all(|r| matches!(r, Run::Text { .. })));
    }

    #[test]
    fn resolved_image_breaks_the_line_and_adds_height() {
        let mut m = FixedWidth::new();
        m.images.insert(1, (999, 100, 32));
        let l = layout("before @G1 after", &opts(1000), &mut m);
        assert_eq!(l.runs.len(), 3);
        assert!(matches!(&l.runs[0], Run::Text { text, .. } if text == "before"));
        assert!(matches!(&l.runs[1], Run::Image { id: 999, w: 100, h: 32, .. }));
        assert!(matches!(&l.runs[2], Run::Text { text, .. } if text == "after"));
        // before (1 line) + image rows (32px / 11px line -> 3 rows) + after (1 line)
        let line_h = line_advance(Font::NormalBlackOnLight);
        let img_rows = (32 + line_h - 1) / line_h;
        assert_eq!(l.height, line_h * (2 + img_rows));
    }

    #[test]
    fn empty_text_has_zero_height() {
        let mut m = FixedWidth::new();
        let l = layout("", &opts(200), &mut m);
        assert!(l.runs.is_empty());
        assert_eq!(l.height, 0);
    }
}
