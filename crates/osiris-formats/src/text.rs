//! `Pharaoh_Text.eng`: the localized string table. `Pharaoh_Text.txt` is the
//! human-edited source used to build it (each `*N` marker there is group `N` here).
//!
//! Layout: 16-byte ASCII magic `"Pharaoh textfile"`, then three `u32` header fields
//! (a declared group count, a declared string count, and a third value we haven't
//! identified), then a *fixed* table of 1000 `(offset: u32, in_use: u32)` slots
//! (8000 bytes, always present regardless of the declared group count). The string
//! blob follows immediately after that table. `offset` is a byte offset from the
//! start of the blob, valid only when `in_use != 0`. A group's data runs from its
//! offset to the offset of the next in-use group (or end of file for the last one);
//! within that span, strings are NUL-terminated and packed back to back, decoded
//! from Windows-1252. String `index` of group `group` is the string found by
//! skipping `index` NUL terminators from the group's offset.
//!
//! Group numbers match the engine's own references (e.g. group 28 holds toolbar and
//! building names, group 160 holds full month names).

use crate::bytes::Reader;
use crate::{Error, Result};

const MAGIC: &[u8; 16] = b"Pharaoh textfile";
/// The slot table always holds this many entries, independent of the header's
/// declared group count.
const GROUP_SLOTS: usize = 1000;

/// A parsed `Pharaoh_Text.eng`. Every in-use group is fully decoded up front, so
/// `get` is a plain index.
#[derive(Debug, Clone, Default)]
pub struct TextTable {
    groups: Vec<Vec<String>>,
}

impl TextTable {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data, "Pharaoh_Text.eng");
        if r.bytes(MAGIC.len())? != MAGIC {
            return Err(Error::Invalid("Pharaoh_Text.eng: bad magic".into()));
        }
        let _declared_groups = r.u32()?;
        let _declared_strings = r.u32()?;
        let _unknown = r.u32()?;

        let mut slots = Vec::with_capacity(GROUP_SLOTS);
        for _ in 0..GROUP_SLOTS {
            let offset = r.u32()? as usize;
            let in_use = r.u32()? != 0;
            slots.push((offset, in_use));
        }
        let blob = r.bytes(r.remaining())?;

        let active: Vec<(usize, usize)> = slots
            .iter()
            .enumerate()
            .filter(|&(_, &(_, in_use))| in_use)
            .map(|(group, &(offset, _))| (group, offset))
            .collect();

        let mut groups = vec![Vec::new(); GROUP_SLOTS];
        for (i, &(group, start)) in active.iter().enumerate() {
            let end = active.get(i + 1).map_or(blob.len(), |&(_, o)| o);
            let span = blob
                .get(start..end.max(start))
                .ok_or(Error::Truncated("Pharaoh_Text.eng"))?;
            groups[group] = split_strings(span);
        }
        Ok(Self { groups })
    }

    /// Number of populated groups (groups whose slot has `in_use != 0`).
    pub fn group_count(&self) -> usize {
        self.groups.iter().filter(|g| !g.is_empty()).count()
    }

    /// Number of strings in `group`, or 0 if the group is empty or unused.
    pub fn group_len(&self, group: usize) -> usize {
        self.groups.get(group).map_or(0, Vec::len)
    }

    pub fn get(&self, group: usize, index: usize) -> Option<&str> {
        self.groups.get(group)?.get(index).map(String::as_str)
    }

    /// Every string of `group`, in order.
    pub fn group(&self, group: usize) -> &[String] {
        self.groups.get(group).map_or(&[], Vec::as_slice)
    }

    /// Replaces `group`'s strings (a group past the 1000 slots is ignored).
    pub fn set_group(&mut self, group: usize, strings: Vec<String>) {
        if self.groups.len() < GROUP_SLOTS {
            self.groups.resize(GROUP_SLOTS, Vec::new());
        }
        if let Some(g) = self.groups.get_mut(group) {
            *g = strings;
        }
    }

    /// The table in the file's own layout, strings encoded as Windows-1252 (a
    /// character it lacks becomes `?`). [`TextTable::parse`] reads it back.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut blob = Vec::new();
        let mut slots = vec![(0u32, false); GROUP_SLOTS];
        let mut strings = 0u32;
        for (group, list) in self.groups.iter().enumerate().take(GROUP_SLOTS) {
            if list.is_empty() {
                continue;
            }
            slots[group] = (blob.len() as u32, true);
            for s in list {
                blob.extend_from_slice(&encode_cp1252(s));
                blob.push(0);
                strings += 1;
            }
        }
        let mut data = Vec::with_capacity(MAGIC.len() + 12 + GROUP_SLOTS * 8 + blob.len());
        data.extend_from_slice(MAGIC);
        data.extend_from_slice(&(slots.iter().filter(|s| s.1).count() as u32).to_le_bytes());
        data.extend_from_slice(&strings.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        for (offset, in_use) in slots {
            data.extend_from_slice(&offset.to_le_bytes());
            data.extend_from_slice(&(in_use as u32).to_le_bytes());
        }
        data.extend_from_slice(&blob);
        data
    }
}

/// Splits a NUL-terminated, NUL-packed span into decoded strings. The final NUL
/// (every string in the format is terminated) leaves one trailing empty piece from
/// `split`, which is dropped; it is not a real trailing empty string.
fn split_strings(span: &[u8]) -> Vec<String> {
    let mut parts: Vec<String> = span.split(|&b| b == 0).map(decode_cp1252).collect();
    if parts.last().is_some_and(String::is_empty) {
        parts.pop();
    }
    parts
}

/// Decodes a Windows-1252 byte string: the encoding of every text file the game
/// reads (its string tables, `eventmsg.txt`, `campaign.txt`), whatever its language.
pub fn decode_cp1252(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| cp1252_char(b)).collect()
}

/// Encodes `s` as Windows-1252, a character outside it becoming `?`.
pub fn encode_cp1252(s: &str) -> Vec<u8> {
    s.chars().map(cp1252_byte).collect()
}

/// The Windows-1252 byte for `c`, or `?` if it has none.
pub fn cp1252_byte(c: char) -> u8 {
    let u = c as u32;
    if u < 0x80 || (0xA0..=0xFF).contains(&u) {
        return u as u8;
    }
    (0x80u8..=0x9F).find(|&b| cp1252_char(b) == c).unwrap_or(b'?')
}

fn cp1252_char(b: u8) -> char {
    // 0x00-0x7F and 0xA0-0xFF match Unicode directly (Latin-1 supplement). 0x80-0x9F
    // are the Windows-1252 extensions over Latin-1; five codes in that range (81, 8D,
    // 8F, 90, 9D) are undefined and map to themselves, matching common practice.
    const HIGH: [char; 32] = [
        '\u{20AC}', '\u{0081}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}',
        '\u{2021}', '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{008D}',
        '\u{017D}', '\u{008F}', '\u{0090}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}',
        '\u{2022}', '\u{2013}', '\u{2014}', '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}',
        '\u{0153}', '\u{009D}', '\u{017E}', '\u{0178}',
    ];
    match b {
        0x80..=0x9F => HIGH[(b - 0x80) as usize],
        _ => b as char,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(groups: &[(u32, &[&[u8]])]) -> Vec<u8> {
        let mut blob = Vec::new();
        let mut slots = vec![(0u32, false); GROUP_SLOTS];
        for &(group, strings) in groups {
            slots[group as usize] = (blob.len() as u32, true);
            for s in strings {
                blob.extend_from_slice(s);
                blob.push(0);
            }
        }
        let mut data = Vec::new();
        data.extend_from_slice(MAGIC);
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        for (offset, in_use) in slots {
            data.extend_from_slice(&offset.to_le_bytes());
            data.extend_from_slice(&(in_use as u32).to_le_bytes());
        }
        data.extend_from_slice(&blob);
        data
    }

    #[test]
    fn parses_groups_and_looks_up_strings() {
        let data = build(&[
            (1, &[b"File", b"New game"]),
            (28, &[b"Nowhere", b"Undo", b"Farm"]),
        ]);
        let t = TextTable::parse(&data).unwrap();
        assert_eq!(t.get(1, 0), Some("File"));
        assert_eq!(t.get(1, 1), Some("New game"));
        assert_eq!(t.get(28, 2), Some("Farm"));
        assert_eq!(t.get(28, 3), None);
        assert_eq!(t.get(2, 0), None);
        assert_eq!(t.group_len(28), 3);
        assert_eq!(t.group_count(), 2);
    }

    #[test]
    fn decodes_cp1252_high_bytes() {
        // 0x80 = EURO SIGN, 0xE9 = LATIN SMALL LETTER E WITH ACUTE.
        let data = build(&[(5, &[&[0x80, 0xE9]])]);
        let t = TextTable::parse(&data).unwrap();
        assert_eq!(t.get(5, 0), Some("\u{20AC}\u{00E9}"));
    }

    #[test]
    fn writes_what_it_reads() {
        let mut t = TextTable::default();
        t.set_group(1, vec!["Datei".into(), "Neues Spiel".into()]);
        t.set_group(28, vec!["Straße".into(), "Überfluß".into(), "Élite «Ñ» ¿¡".into(), "Œuvre – €".into()]);
        let bytes = t.to_bytes();
        assert!(bytes.windows(6).any(|w| w == b"Stra\xDFe"));
        let back = TextTable::parse(&bytes).unwrap();
        assert_eq!(back.get(1, 1), Some("Neues Spiel"));
        assert_eq!(back.get(28, 2), Some("Élite «Ñ» ¿¡"));
        assert_eq!(back.get(28, 3), Some("Œuvre – €"));
        assert_eq!(back.group_count(), 2);
    }

    #[test]
    fn encodes_cp1252() {
        assert_eq!(encode_cp1252("aé€\u{3b1}"), vec![b'a', 0xE9, 0x80, b'?']);
        for b in 0x20..=0xFFu8 {
            let c = cp1252_char(b);
            if !matches!(b, 0x81 | 0x8D | 0x8F | 0x90 | 0x9D) {
                assert_eq!(cp1252_byte(c), b);
            }
        }
    }

    #[test]
    fn rejects_bad_magic() {
        let mut data = build(&[(1, &[b"x"])]);
        data[0] = b'X';
        assert!(TextTable::parse(&data).is_err());
    }
}
