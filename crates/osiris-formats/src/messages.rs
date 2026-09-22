//! `Pharaoh_MM.eng`: briefing and popup message entries. `Pharaoh_MM.txt` is the
//! human-edited source (each `*N` marker there is message `N` here); its field names
//! (`TYPE`, `BOX_X`, `PIC1`, `CAPTION_TEXT`, ...) are used below, but the compiled
//! binary does *not* store them in that display order (determined empirically by
//! correlating known non-zero field values from the `.txt` source against the
//! decoded `.eng` bytes for all 497 entries).
//!
//! Layout: 16-byte ASCII magic `"Pharaoh MM file."`, two `u32` header fields (a
//! declared slot count, always 1000, and a declared in-use count), then a *fixed*
//! array of 1000 entries of 80 bytes (40 `u16` fields) each, then the string blob
//! for the rest of the file. Entries `0..declared_count` are populated; the rest are
//! all-zero. Field layout within an entry (`u16` index):
//!
//! ```text
//!  0 TYPE          1 SUB_TYPE      2 DATA
//!  3 BOX_X         4 BOX_Y         5 BOX_W        6 BOX_H
//!  7 PIC1          8 PIC1_X        9 PIC1_Y
//! 10 PIC2         11 PIC2_X       12 PIC2_Y
//! 13 TITLE_X      14 TITLE_Y
//! 15 CAPTION_X    16 CAPTION_Y
//! 17 TEXT_X       18 TEXT_Y
//! 19..=27, 29     reserved (always zero in the shipped English file; ANIM_X and
//!                 ANIM_Y from the source are presumably two of these, but every
//!                 entry in the file leaves both at 0 so their exact slots can't be
//!                 pinned down empirically)
//! 28 DELAY
//! 30-31 (u32) ANIMATION text offset    32-33 (u32) SOUND text offset
//! 34-35 (u32) TITLE_TEXT offset        36-37 (u32) CAPTION_TEXT offset
//! 38-39 (u32) MAIN_TEXT offset
//! ```
//!
//! String offset fields are byte offsets from the start of the blob (0 lands on a
//! leading NUL, decoding to an empty string, so no special-casing is needed);
//! strings are NUL-terminated Windows-1252, decoded with [`crate::text`]'s decoder.

use crate::bytes::Reader;
use crate::text::decode_cp1252;
use crate::{Error, Result};

const MAGIC: &[u8; 16] = b"Pharaoh MM file.";
const SLOTS: usize = 1000;
const FIELDS: usize = 40;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImageRef {
    pub id: u16,
    pub x: i16,
    pub y: i16,
}

/// One briefing/popup message entry.
#[derive(Debug, Clone, Default)]
pub struct Message {
    pub id: u16,
    /// `TYPE`: matches the engine's message archetype (0 building/manual info,
    /// 1 about box, 2 general message, 3 mission briefing).
    pub kind: u16,
    /// `SUB_TYPE`.
    pub category: u16,
    pub data: u16,
    pub pos: (i16, i16),
    pub size: (i16, i16),
    pub image1: ImageRef,
    pub image2: ImageRef,
    pub title_pos: (i16, i16),
    pub subtitle_pos: (i16, i16),
    pub content_pos: (i16, i16),
    pub delay: u16,
    pub video: String,
    pub sound: String,
    pub title: String,
    pub subtitle: String,
    pub content: String,
}

impl Message {
    fn from_fields(id: u16, f: &[u16; FIELDS], blob: &[u8]) -> Self {
        let u32_at = |lo: usize| (f[lo] as u32) | ((f[lo + 1] as u32) << 16);
        Self {
            id,
            kind: f[0],
            category: f[1],
            data: f[2],
            pos: (f[3] as i16, f[4] as i16),
            size: (f[5] as i16, f[6] as i16),
            image1: ImageRef {
                id: f[7],
                x: f[8] as i16,
                y: f[9] as i16,
            },
            image2: ImageRef {
                id: f[10],
                x: f[11] as i16,
                y: f[12] as i16,
            },
            title_pos: (f[13] as i16, f[14] as i16),
            subtitle_pos: (f[15] as i16, f[16] as i16),
            content_pos: (f[17] as i16, f[18] as i16),
            delay: f[28],
            video: string_at(blob, u32_at(30)),
            sound: string_at(blob, u32_at(32)),
            title: string_at(blob, u32_at(34)),
            subtitle: string_at(blob, u32_at(36)),
            content: string_at(blob, u32_at(38)),
        }
    }
}

fn string_at(blob: &[u8], offset: u32) -> String {
    match blob.get(offset as usize..) {
        Some(rest) => {
            let end = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
            decode_cp1252(&rest[..end])
        }
        None => String::new(),
    }
}

#[derive(Debug, Clone, Default)]
pub struct MessageTable {
    messages: Vec<Message>,
}

impl MessageTable {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data, "Pharaoh_MM.eng");
        if r.bytes(MAGIC.len())? != MAGIC {
            return Err(Error::Invalid("Pharaoh_MM.eng: bad magic".into()));
        }
        let _declared_slots = r.u32()?;
        let declared_count = r.u32()? as usize;

        let mut raw = Vec::with_capacity(SLOTS);
        for _ in 0..SLOTS {
            let mut fields = [0u16; FIELDS];
            for field in &mut fields {
                *field = r.u16()?;
            }
            raw.push(fields);
        }
        let blob = r.bytes(r.remaining())?;

        let count = declared_count.min(SLOTS);
        let messages = raw
            .iter()
            .take(count)
            .enumerate()
            .map(|(id, fields)| Message::from_fields(id as u16, fields, blob))
            .collect();
        Ok(Self { messages })
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    pub fn get(&self, id: usize) -> Option<&Message> {
        self.messages.get(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(entries: &[(u16, &str, &str, &str)]) -> Vec<u8> {
        let mut blob = vec![0u8]; // offset 0 must decode to "".
        let mut push_str = |s: &str| -> u32 {
            if s.is_empty() {
                return 0;
            }
            let off = blob.len() as u32;
            blob.extend_from_slice(s.as_bytes());
            blob.push(0);
            off
        };

        let max_id = entries.iter().map(|&(id, ..)| id).max().unwrap_or(0);
        let mut slots = vec![[0u16; FIELDS]; SLOTS];
        for &(id, title, subtitle, content) in entries {
            let mut f = [0u16; FIELDS];
            f[0] = 3; // TYPE = mission briefing
            let t = push_str(title);
            let c = push_str(subtitle);
            let m = push_str(content);
            f[34] = t as u16;
            f[35] = (t >> 16) as u16;
            f[36] = c as u16;
            f[37] = (c >> 16) as u16;
            f[38] = m as u16;
            f[39] = (m >> 16) as u16;
            slots[id as usize] = f;
        }

        let mut data = Vec::new();
        data.extend_from_slice(MAGIC);
        data.extend_from_slice(&(SLOTS as u32).to_le_bytes());
        data.extend_from_slice(&(max_id as u32 + 1).to_le_bytes());
        for f in &slots {
            for v in f {
                data.extend_from_slice(&v.to_le_bytes());
            }
        }
        data.extend_from_slice(&blob);
        data
    }

    #[test]
    fn parses_title_subtitle_content() {
        let data = build(&[(0, "Nubt", "A Village is Born", "Welcome to ancient Egypt")]);
        let t = MessageTable::parse(&data).unwrap();
        let m = t.get(0).unwrap();
        assert_eq!(m.kind, 3);
        assert_eq!(m.title, "Nubt");
        assert_eq!(m.subtitle, "A Village is Born");
        assert_eq!(m.content, "Welcome to ancient Egypt");
        assert_eq!(t.len(), 1);
        assert!(t.get(1).is_none());
    }

    #[test]
    fn empty_offset_decodes_to_empty_string() {
        let data = build(&[(0, "Title", "", "Body")]);
        let t = MessageTable::parse(&data).unwrap();
        assert_eq!(t.get(0).unwrap().subtitle, "");
    }

    #[test]
    fn rejects_bad_magic() {
        let mut data = build(&[(0, "x", "y", "z")]);
        data[0] = b'X';
        assert!(MessageTable::parse(&data).is_err());
    }
}
