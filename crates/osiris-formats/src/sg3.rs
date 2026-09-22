//! `.sg3` image indexes and their `.555` pixel files.
//!
//! An `.sg3` file is a fixed header, 300 group start indexes, 200 bitmap names and then
//! one 64-byte record per image (72 bytes from version 214). Record 0 is always empty.
//! Pixel data lives in the `.555` file of the same name, or, for records flagged
//! external, in `<bitmap name>.555` next to it. Pixels are 16-bit RGB555.
//!
//! Three encodings exist:
//! - plain: `width * height` raw pixels, with 0x781F as the transparent key;
//! - RLE ("fully compressed"): runs of `[n][n pixels]` or `[0xFF][skip n]`;
//! - isometric: a raw diamond footprint made of 58x30 tiles, optionally followed by an
//!   RLE "top" (the part of a building that rises above its footprint).

use crate::bytes::Reader;
use crate::{Error, Result, read_file};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub type Rgba = [u8; 4];

pub const TRANSPARENT: Rgba = [0, 0, 0, 0];
const KEY_555: u16 = 0x781F;

const HEADER_INTS: usize = 20;
const GROUP_COUNT: usize = 300;
const BITMAP_NAME_LEN: usize = 200;
const BITMAP_NAME_SLOTS: usize = 200;
const RECORDS_START: usize =
    HEADER_INTS * 4 + GROUP_COUNT * 2 + BITMAP_NAME_SLOTS * BITMAP_NAME_LEN;

pub const TILE_WIDTH: i32 = 58;
pub const TILE_HEIGHT: i32 = 30;
const FOOTPRINT_X_START: [i32; 30] = [
    28, 26, 24, 22, 20, 18, 16, 14, 12, 10, 8, 6, 4, 2, 0, 0, 2, 4, 6, 8, 10, 12, 14, 16, 18, 20,
    22, 24, 26, 28,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    Transparent,
    Opaque,
    Tile16,
    Tile24,
    Tile32,
    Font,
    Isometric,
    Other(u8),
}

impl From<u8> for ImageKind {
    fn from(v: u8) -> Self {
        match v {
            0 => Self::Transparent,
            1 => Self::Opaque,
            10 => Self::Tile16,
            12 => Self::Tile24,
            13 => Self::Tile32,
            20 => Self::Font,
            30 => Self::Isometric,
            o => Self::Other(o),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImageRecord {
    pub offset: u32,
    pub data_length: u32,
    pub uncompressed_length: u32,
    /// Non-zero means "draw record `index + mirror_offset` flipped horizontally".
    pub mirror_offset: i32,
    pub width: u16,
    pub height: u16,
    pub num_animation_sprites: u16,
    pub sprite_offset_x: i16,
    pub sprite_offset_y: i16,
    pub animation_can_reverse: bool,
    pub kind: ImageKind,
    pub compressed: bool,
    pub external: bool,
    pub has_isometric_top: bool,
    pub bitmap_id: u8,
    pub animation_speed_id: u8,
}

impl ImageRecord {
    /// Footprint size in tiles for isometric images (1 for a 1x1 building, 2 for 2x2 ...).
    pub fn isometric_tiles(&self) -> i32 {
        (self.width as i32 + 2) / (TILE_WIDTH + 2)
    }
}

/// A decoded image: straight (non-premultiplied) RGBA8, row-major.
#[derive(Debug, Clone, Default)]
pub struct Sprite {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<Rgba>,
}

impl Sprite {
    fn blank(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![TRANSPARENT; (width * height) as usize],
        }
    }

    fn put(&mut self, x: i32, y: i32, c: Rgba) {
        if x >= 0 && y >= 0 && (x as u32) < self.width && (y as u32) < self.height {
            self.pixels[(y as u32 * self.width + x as u32) as usize] = c;
        }
    }

    fn flip_horizontal(&mut self) {
        let w = self.width as usize;
        for row in self.pixels.chunks_exact_mut(w.max(1)) {
            row.reverse();
        }
    }
}

pub fn rgb555_to_rgba(c: u16) -> Rgba {
    if c == KEY_555 {
        return TRANSPARENT;
    }
    let expand = |v: u16| ((v << 3) | (v >> 2)) as u8;
    [
        expand((c >> 10) & 0x1f),
        expand((c >> 5) & 0x1f),
        expand(c & 0x1f),
        255,
    ]
}

pub struct Sg3 {
    pub name: String,
    pub version: u32,
    /// Start index of each image group; group 0 starts at 0.
    pub group_starts: Vec<u16>,
    pub bitmap_names: Vec<String>,
    pub records: Vec<ImageRecord>,
    dir: PathBuf,
    data: Vec<u8>,
    external: Mutex<HashMap<String, Option<Vec<u8>>>>,
}

impl std::fmt::Debug for Sg3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sg3")
            .field("name", &self.name)
            .field("version", &self.version)
            .field("records", &self.records.len())
            .finish()
    }
}

impl Sg3 {
    /// Loads `<dir>/<name>.sg3` and its `.555`. A missing `.555` is allowed (some
    /// packs hold only external images); decoding an internal image then fails.
    pub fn open(dir: &Path, name: &str) -> Result<Self> {
        let index = read_file(&dir.join(format!("{name}.sg3")))?;
        let data = std::fs::read(dir.join(format!("{name}.555"))).unwrap_or_default();
        Self::parse(name, &index, data, dir.to_owned())
    }

    pub fn parse(name: &str, index: &[u8], data: Vec<u8>, dir: PathBuf) -> Result<Self> {
        let mut r = Reader::new(index, "sg3");
        let mut header = [0u32; HEADER_INTS];
        for h in &mut header {
            *h = r.u32()?;
        }
        let version = header[1];
        let num_records = header[4] as usize + 1;
        let num_bitmaps = header[5] as usize;
        if num_bitmaps > BITMAP_NAME_SLOTS {
            return Err(Error::Invalid(format!(
                "{name}.sg3: {num_bitmaps} bitmap names"
            )));
        }

        let mut group_starts = Vec::with_capacity(GROUP_COUNT);
        for _ in 0..GROUP_COUNT {
            group_starts.push(r.u16()?);
        }
        let mut bitmap_names = Vec::with_capacity(num_bitmaps);
        for _ in 0..num_bitmaps {
            bitmap_names.push(r.cstr(BITMAP_NAME_LEN)?);
        }

        r.seek(RECORDS_START)?;
        let record_size = if version >= 214 { 72 } else { 64 };
        let mut records = Vec::with_capacity(num_records);
        for _ in 0..num_records {
            let start = r.pos();
            let offset = r.u32()?;
            let data_length = r.u32()?;
            let uncompressed_length = r.u32()?;
            r.skip(4)?;
            let mirror_offset = r.i32()?;
            let width = r.i16()?.max(0) as u16;
            let height = r.i16()?.max(0) as u16;
            r.skip(6)?;
            let num_animation_sprites = r.u16()?;
            r.skip(2)?;
            let sprite_offset_x = r.i16()?;
            let sprite_offset_y = r.i16()?;
            r.skip(10)?;
            let animation_can_reverse = r.u8()? != 0;
            r.skip(1)?;
            let kind = ImageKind::from(r.u8()?);
            let compressed = r.u8()? != 0;
            let external = r.u8()? != 0;
            let has_isometric_top = r.u8()? != 0;
            r.skip(2)?;
            let bitmap_id = r.u8()?;
            r.skip(1)?;
            let animation_speed_id = r.u8()?;
            r.seek(start + record_size)?;
            records.push(ImageRecord {
                offset,
                data_length,
                uncompressed_length,
                mirror_offset,
                width,
                height,
                num_animation_sprites,
                sprite_offset_x,
                sprite_offset_y,
                animation_can_reverse,
                kind,
                compressed,
                external,
                has_isometric_top,
                bitmap_id,
                animation_speed_id,
            });
        }

        Ok(Self {
            name: name.to_owned(),
            version,
            group_starts,
            bitmap_names,
            records,
            dir,
            data,
            external: Mutex::new(HashMap::new()),
        })
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Index of the first image in `group` (the original game's "image group" ids).
    pub fn group_start(&self, group: usize) -> Option<usize> {
        let start = *self.group_starts.get(group)?;
        (group == 0 || start != 0).then_some(start as usize)
    }

    pub fn bitmap_name(&self, record: &ImageRecord) -> &str {
        self.bitmap_names
            .get(record.bitmap_id as usize)
            .map_or("", String::as_str)
    }

    /// Decodes image `index` to RGBA. Mirrored records decode their source and flip it.
    pub fn decode(&self, index: usize) -> Result<Sprite> {
        let rec = self.record(index)?;
        if rec.mirror_offset != 0 {
            let src = index as i64 + rec.mirror_offset as i64;
            let src_rec = self.record(src as usize)?;
            if src_rec.mirror_offset != 0 {
                return Err(Error::Invalid(format!(
                    "{}#{index}: chained mirror",
                    self.name
                )));
            }
            let mut sprite = self.decode(src as usize)?;
            sprite.flip_horizontal();
            return Ok(sprite);
        }
        if rec.width == 0 || rec.height == 0 || rec.data_length == 0 {
            return Ok(Sprite::blank(rec.width as u32, rec.height as u32));
        }
        if rec.external {
            let name = self.bitmap_name(rec).to_owned();
            let mut cache = self.external.lock().unwrap();
            let file = cache.entry(name.clone()).or_insert_with(|| {
                let stem = Path::new(&name).with_extension("555");
                std::fs::read(self.dir.join(stem)).ok()
            });
            let file = file.as_deref().ok_or_else(|| {
                Error::Invalid(format!(
                    "{}#{index}: external file for {name} missing",
                    self.name
                ))
            })?;
            // External offsets are 1-based.
            let start = (rec.offset as usize).saturating_sub(1);
            decode_pixels(rec, slice(file, start, rec.data_length, &self.name, index)?)
        } else {
            let data = slice(
                &self.data,
                rec.offset as usize,
                rec.data_length,
                &self.name,
                index,
            )?;
            decode_pixels(rec, data)
        }
    }

    pub fn record(&self, index: usize) -> Result<&ImageRecord> {
        self.records
            .get(index)
            .ok_or_else(|| Error::Invalid(format!("{}: no image {index}", self.name)))
    }
}

fn slice<'a>(data: &'a [u8], start: usize, len: u32, name: &str, index: usize) -> Result<&'a [u8]> {
    data.get(start..start + len as usize).ok_or_else(|| {
        Error::Invalid(format!(
            "{name}#{index}: pixel data {start}+{len} beyond {} bytes",
            data.len()
        ))
    })
}

fn decode_pixels(rec: &ImageRecord, data: &[u8]) -> Result<Sprite> {
    let mut sprite = Sprite::blank(rec.width as u32, rec.height as u32);
    if rec.kind == ImageKind::Isometric {
        let footprint_len = (rec.uncompressed_length as usize).min(data.len());
        decode_footprint(rec, &data[..footprint_len], &mut sprite);
        if rec.has_isometric_top {
            decode_rle(&data[footprint_len..], &mut sprite);
        }
    } else if rec.compressed {
        decode_rle(data, &mut sprite);
    } else {
        for (dst, px) in sprite.pixels.iter_mut().zip(data.as_chunks::<2>().0) {
            *dst = rgb555_to_rgba(u16::from_le_bytes(*px));
        }
    }
    Ok(sprite)
}

fn decode_rle(data: &[u8], sprite: &mut Sprite) {
    let width = sprite.width as usize;
    let total = width * sprite.height as usize;
    let mut pos = 0usize; // pixel index in the sprite
    let mut i = 0usize;
    while i < data.len() && pos < total {
        let control = data[i];
        i += 1;
        if control == 0xFF {
            let Some(&skip) = data.get(i) else { break };
            i += 1;
            pos += skip as usize;
        } else {
            for _ in 0..control {
                let Some(px) = data.get(i..i + 2) else { return };
                i += 2;
                if pos < total {
                    sprite.pixels[pos] = rgb555_to_rgba(u16::from_le_bytes([px[0], px[1]]));
                }
                pos += 1;
            }
        }
    }
}

/// The footprint is stored as whole 58x30 diamond tiles, back row first, each tile's
/// pixels raw and row by row. Adjacent tiles overlap by one pixel column pair.
fn decode_footprint(rec: &ImageRecord, data: &[u8], sprite: &mut Sprite) {
    let tiles = rec.isometric_tiles();
    if tiles <= 0 {
        return;
    }
    let x_start = (tiles - 1) * TILE_HEIGHT;
    let y_offset = rec.height as i32 - TILE_HEIGHT * tiles;
    let mut px = data
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| u16::from_le_bytes(*p));
    let mut tile = |sprite: &mut Sprite, ox: i32, oy: i32| {
        for (y, &xs) in FOOTPRINT_X_START.iter().enumerate() {
            for x in xs..TILE_WIDTH - xs {
                let Some(c) = px.next() else { return };
                sprite.put(x + ox, y as i32 + oy, rgb555_to_rgba(c));
            }
        }
    };
    for i in 0..tiles {
        let mut x = -TILE_HEIGHT * i + x_start;
        let y = TILE_HEIGHT / 2 * i + y_offset;
        for _ in 0..=i {
            tile(sprite, x, y);
            x += TILE_WIDTH + 2;
        }
    }
    for i in (0..tiles - 1).rev() {
        let mut x = -TILE_HEIGHT * i + x_start;
        let y = TILE_HEIGHT / 2 * (tiles * 2 - i - 2) + y_offset;
        for _ in 0..=i {
            tile(sprite, x, y);
            x += TILE_WIDTH + 2;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_expansion() {
        assert_eq!(rgb555_to_rgba(0x7fff), [255, 255, 255, 255]);
        assert_eq!(rgb555_to_rgba(0), [0, 0, 0, 255]);
        assert_eq!(rgb555_to_rgba(KEY_555), TRANSPARENT);
    }

    #[test]
    fn footprint_tile_pixel_count() {
        let n: i32 = FOOTPRINT_X_START.iter().map(|&x| TILE_WIDTH - 2 * x).sum();
        assert_eq!(n, 900); // 1800 bytes per tile, as in the files
    }
}
